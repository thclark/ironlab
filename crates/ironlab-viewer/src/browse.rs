//! Choosing one figure from a collection of them.
//!
//! A figure carries named [`Parameter`]s that describe it, such as the Reynolds number of the flow it shows or the
//! solver that produced its data. Nobody knows their names when the viewer is built: an experimentalist gives a
//! figure whatever parameters the experiment had. This module turns a collection of such figures into something that
//! can be narrowed down — it works out what the collection can be filtered on, counts how many figures each choice
//! would leave, and applies the choices the user has made.
//!
//! It holds no egui: everything here is a value, and every question it answers is answered by a function of the
//! collection and the state, so the behaviour is tested without a window. [`crate::sidebar`] draws it.
//!
//! The pieces are:
//!
//! - A [`FigureCard`] is one figure as the browser sees it: its title, its labels, and its parameters. Building a
//!   card from a [`Figure`] adds the facets that can be read off the figure itself ([`derived_parameters`] and
//!   [`derived_labels`]), so a collection is filterable before anyone has given a figure a parameter of their own.
//! - A [`Facet`] is one thing the collection can be narrowed by, and [`describe_facets`] decides which facets are
//!   worth offering and in what order.
//! - A [`Query`] is the typed form of the same thing: `rig:CFD angle>=8 -stalled`.
//! - A [`Browse`] holds what the user has chosen — the query, the filters, the sort and the grouping — and
//!   [`Browse::results`] turns a collection into the list to show.
//!
//! Two rules run through all of it, and both come from how faceted search behaves when it is done well.
//!
//! **Values within one facet are alternatives; facets are cumulative.** Choosing two rigs widens the result to
//! figures from either; choosing a rig and a solver narrows it to figures with both. This is what a reader expects
//! without being told, and it is the only arrangement in which ticking another box never takes figures away.
//!
//! **A facet's counts are computed as though that facet were not filtered.** The count beside a value is how many
//! figures would remain if that value were chosen alongside the ones already chosen in the same facet, so a value
//! that is worth choosing never shows a zero and choosing a second value in a facet never empties the list.
//!
//! Sparseness is the normal case, not an edge case: a parameter given to some figures of a collection and not
//! others is what happens as soon as two experiments are compared. A figure that does not carry a parameter is not
//! in a refinement on it, and sorts after every figure that does.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use ironlab_ir::{Artist, Figure, Parameter, Projection, Scale};

/// The name of the derived parameter holding whether a figure is two- or three-dimensional.
pub const DIMENSIONALITY: &str = "dimensionality";

/// The name of the derived parameter holding the number of axes a figure has.
pub const AXES_COUNT: &str = "axes";

/// The name of the derived parameter holding the number of artists a figure draws.
pub const ARTIST_COUNT: &str = "artists";

/// The name of the derived parameter holding how many data values the figure carries.
///
/// It is one word, as every parameter name must be to be reachable from the query language, which takes a term to
/// end at the first space.
pub const DATA_VALUES: &str = "data_values";

/// The names of every parameter this module derives from a figure, in the order [`derived_parameters`] returns
/// them.
///
/// The list is public because the interface distinguishes what the viewer worked out from what the user wrote, and
/// because a user's own parameter of the same name takes precedence over the derived one.
pub const DERIVED: &[&str] = &[ARTIST_COUNT, AXES_COUNT, DATA_VALUES, DIMENSIONALITY];

/// The label given to a group of figures that the grouping parameter does not apply to.
pub const NOT_SET: &str = "not set";

/// The label given to a group of figures that carry no labels at all.
pub const NO_LABELS: &str = "no labels";

/// One value a [`Facet`] takes.
///
/// The four forms are the four forms of a [`Parameter`], with labels taking the text form. Values are ordered and
/// hashed so that they can be counted and listed in a stable order: numbers are ordered by [`f64::total_cmp`], which
/// is a total order over every double including the non-finite ones a figure should not contain but might, and
/// hashed by their bits, so that equal values always hash alike.
#[derive(Clone, Debug)]
pub enum FacetValue {
    /// A boolean.
    Bool(bool),
    /// A signed 64-bit integer.
    Integer(i64),
    /// A double-precision floating-point number.
    Number(f64),
    /// A string of Unicode text, which is also how a label is held.
    Text(String),
}

impl FacetValue {
    /// The position of the form in the order the forms are compared in, so that a facet whose figures disagree
    /// about the form of a value still lists them in a stable order.
    fn rank(&self) -> u8 {
        match self {
            FacetValue::Bool(_) => 0,
            FacetValue::Integer(_) => 1,
            FacetValue::Number(_) => 2,
            FacetValue::Text(_) => 3,
        }
    }

    /// The value as a number, for a form that has one.
    #[must_use]
    pub fn as_number(&self) -> Option<f64> {
        match self {
            FacetValue::Integer(value) => Some(*value as f64),
            FacetValue::Number(value) => Some(*value),
            FacetValue::Bool(_) | FacetValue::Text(_) => None,
        }
    }

    /// The value as the interface writes it.
    ///
    /// A boolean is written as "yes" or "no" rather than "true" or "false", because the interface asks the reader
    /// about the world rather than about the file. A number is written with enough digits to tell neighbouring
    /// values apart and without the noise of printing a binary fraction in full.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            FacetValue::Bool(true) => "yes".to_owned(),
            FacetValue::Bool(false) => "no".to_owned(),
            FacetValue::Integer(value) => value.to_string(),
            FacetValue::Number(value) => number_text(*value),
            FacetValue::Text(value) => value.clone(),
        }
    }
}

impl From<&Parameter> for FacetValue {
    fn from(parameter: &Parameter) -> Self {
        match parameter {
            Parameter::Bool(value) => FacetValue::Bool(*value),
            Parameter::Integer(value) => FacetValue::Integer(*value),
            Parameter::Number(value) => FacetValue::Number(*value),
            Parameter::String(value) => FacetValue::Text(value.clone()),
        }
    }
}

impl PartialEq for FacetValue {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for FacetValue {}

impl PartialOrd for FacetValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FacetValue {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (FacetValue::Bool(a), FacetValue::Bool(b)) => a.cmp(b),
            (FacetValue::Integer(a), FacetValue::Integer(b)) => a.cmp(b),
            (FacetValue::Number(a), FacetValue::Number(b)) => a.total_cmp(b),
            (FacetValue::Text(a), FacetValue::Text(b)) => a.cmp(b),
            _ => self.rank().cmp(&other.rank()),
        }
    }
}

impl Hash for FacetValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.rank().hash(state);
        match self {
            FacetValue::Bool(value) => value.hash(state),
            FacetValue::Integer(value) => value.hash(state),
            // The bits, so that values which compare equal under `total_cmp` hash alike; `0.0` and `-0.0` are
            // distinct under both.
            FacetValue::Number(value) => value.to_bits().hash(state),
            FacetValue::Text(value) => value.hash(state),
        }
    }
}

/// Formats a number for the interface, with enough digits to tell neighbouring values apart and without the noise
/// that printing a binary fraction in full would add.
///
/// It is the rule the datatips use, so that a value reads the same wherever the viewer writes it.
#[must_use]
pub fn number_text(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    if value != 0.0 && !(1e-4..1e6).contains(&value.abs()) {
        return format!("{value:.4e}");
    }
    let text = format!("{value:.6}");
    match text.split_once('.') {
        Some(_) => text.trim_end_matches('0').trim_end_matches('.').to_owned(),
        None => text,
    }
}

/// What a facet draws its values from.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FacetKey {
    /// The labels of a figure, which are a set of values rather than one value.
    Labels,
    /// A named parameter, whether the user gave it or the viewer derived it.
    Parameter(String),
}

impl FacetKey {
    /// A parameter facet of the given name.
    #[must_use]
    pub fn parameter(name: impl Into<String>) -> Self {
        FacetKey::Parameter(name.into())
    }

    /// The name of the facet as the interface writes it.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            FacetKey::Labels => "labels",
            FacetKey::Parameter(name) => name,
        }
    }

    /// Whether the facet is one the viewer derived from the figure rather than one the user wrote.
    #[must_use]
    pub fn is_derived(&self) -> bool {
        match self {
            FacetKey::Labels => false,
            FacetKey::Parameter(name) => DERIVED.contains(&name.as_str()),
        }
    }
}

/// How a facet offers its values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FacetKind {
    /// A set of labels, of which a figure may carry several.
    Labels,
    /// Yes or no.
    Bool,
    /// Whole numbers, offered as a range.
    Integer,
    /// Numbers, offered as a range.
    Number,
    /// Text, offered as a list of the values that occur.
    Text,
}

impl FacetKind {
    /// Whether the facet is narrowed by a range of numbers rather than by choosing among its values.
    #[must_use]
    pub fn is_numeric(self) -> bool {
        matches!(self, FacetKind::Integer | FacetKind::Number)
    }
}

/// One thing a collection of figures can be narrowed by.
#[derive(Clone, Debug)]
pub struct Facet {
    /// What the facet draws its values from.
    pub key: FacetKey,
    /// How the facet offers its values.
    pub kind: FacetKind,
    /// How many figures of the collection carry the facet at all.
    pub present: usize,
    /// Every value the facet takes across the collection, in ascending order.
    pub values: Vec<FacetValue>,
    /// The smallest and largest values of a numeric facet, or `None` when the facet is not numeric or holds no
    /// finite value.
    pub range: Option<(f64, f64)>,
    /// How well the facet divides the collection; see [`describe_facets`].
    pub score: f64,
}

impl Facet {
    /// How many distinct values the facet takes.
    #[must_use]
    pub fn cardinality(&self) -> usize {
        self.values.len()
    }
}

/// One figure as the browser sees it.
///
/// It is what the browser needs and nothing else, so that the figures themselves are read once rather than on every
/// frame: a figure's data can run to millions of values, and counting them for each keystroke would be felt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FigureCard {
    /// The title the figure is listed under, which is the title of its tab.
    pub title: String,
    /// The labels the figure carries, in the order they were given.
    pub labels: Vec<String>,
    /// The parameters the figure can be narrowed by, in ascending order of name: the user's own, and the ones
    /// [`derived_parameters`] read off the figure.
    pub parameters: BTreeMap<String, Parameter>,
}

impl FigureCard {
    /// Builds the card of `figure`, listed under `title`.
    ///
    /// The card holds the figure's own parameters and the ones derived from the figure itself, with the figure's own
    /// winning where the two share a name: a parameter the user wrote says what they meant, and the viewer's guess
    /// at the same name does not.
    #[must_use]
    pub fn of(title: impl Into<String>, figure: &Figure) -> Self {
        let mut parameters = derived_parameters(figure);
        parameters.extend(
            figure
                .parameters
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
        Self {
            title: title.into(),
            labels: derived_labels(figure),
            parameters,
        }
    }

    /// The value of the named parameter, if the figure carries it.
    #[must_use]
    pub fn parameter(&self, name: &str) -> Option<&Parameter> {
        self.parameters.get(name)
    }

    /// The text a free-text search term is matched against: the title, the labels, and the names and values of every
    /// parameter, folded to lower case.
    ///
    /// Parameter names are included so that typing the name of a parameter finds the figures that have one, which is
    /// how someone looks for a parameter they half remember.
    #[must_use]
    pub fn haystack(&self) -> String {
        let mut text = self.title.to_lowercase();
        for label in &self.labels {
            text.push(' ');
            text.push_str(&label.to_lowercase());
        }
        for (name, value) in &self.parameters {
            text.push(' ');
            text.push_str(&name.to_lowercase());
            text.push(' ');
            text.push_str(&FacetValue::from(value).text().to_lowercase());
        }
        text
    }
}

/// The labels read off a figure itself: what kind of thing it draws, and whether it is flat.
///
/// They are what makes a collection worth browsing before anyone has labelled a figure by hand. A figure gets, in
/// this order:
///
/// 1. `"2d"` or `"3d"`;
/// 2. one label for each kind of artist it draws, in alphabetical order, from `"contour"`, `"image"`, `"line"`,
///    `"quiver"`, `"scatter"` and `"surface"`, with the three kinds of raster all counting as `"image"`;
/// 3. `"subplots"` when it has more than one axes;
/// 4. `"legend"` when any axes shows one;
/// 5. `"log"` when any axis is logarithmic.
///
/// Each label appears at most once.
#[must_use]
pub fn derived_labels(figure: &Figure) -> Vec<String> {
    let mut labels = Vec::new();
    let three_d = figure
        .axes
        .iter()
        .any(|axes| matches!(axes.projection, Projection::ThreeD { .. }));
    labels.push(if three_d { "3d" } else { "2d" }.to_owned());

    let mut kinds = BTreeSet::new();
    for axes in &figure.axes {
        for artist in &axes.artists {
            kinds.insert(artist_label(artist));
        }
    }
    labels.extend(kinds.into_iter().map(str::to_owned));

    if figure.axes.len() > 1 {
        labels.push("subplots".to_owned());
    }
    if figure.axes.iter().any(|axes| axes.legend.is_some()) {
        labels.push("legend".to_owned());
    }
    if figure.axes.iter().any(|axes| {
        [&axes.x, &axes.y, &axes.z]
            .iter()
            .any(|axis| axis.scale == Scale::Log)
    }) {
        labels.push("log".to_owned());
    }
    labels
}

/// The label naming what an artist draws.
fn artist_label(artist: &Artist) -> &'static str {
    match artist {
        Artist::Line(_) => "line",
        Artist::Scatter(_) => "scatter",
        Artist::Contour(_) => "contour",
        Artist::Quiver(_) => "quiver",
        Artist::Surface(_) => "surface",
        // The three rasters differ in how their pixels get their colour, which is not a distinction anyone browses
        // by; all three are an image.
        Artist::Image(_) | Artist::IndexedImage(_) | Artist::MappedImage(_) => "image",
    }
}

/// The parameters read off a figure itself: how many axes and artists it has, how many data values it carries, and
/// whether it is two- or three-dimensional.
///
/// They are named by [`DERIVED`], and are what a collection can be sorted and grouped by before anyone has given a
/// figure a parameter. The count of data values is the total length of every array in the figure's data table,
/// which is what makes a figure slow to draw and is therefore worth sorting by.
#[must_use]
pub fn derived_parameters(figure: &Figure) -> BTreeMap<String, Parameter> {
    let artists: usize = figure.axes.iter().map(|axes| axes.artists.len()).sum();
    let values: usize = figure
        .data
        .values()
        .map(|array| array.shape.iter().product::<usize>())
        .sum();
    let three_d = figure
        .axes
        .iter()
        .any(|axes| matches!(axes.projection, Projection::ThreeD { .. }));
    BTreeMap::from([
        (ARTIST_COUNT.to_owned(), Parameter::Integer(artists as i64)),
        (
            AXES_COUNT.to_owned(),
            Parameter::Integer(figure.axes.len() as i64),
        ),
        (DATA_VALUES.to_owned(), Parameter::Integer(values as i64)),
        (
            DIMENSIONALITY.to_owned(),
            Parameter::String(if three_d { "3D" } else { "2D" }.to_owned()),
        ),
    ])
}

/// Describes every facet the collection offers, the most useful first.
///
/// Which facets are worth offering cannot be decided in advance, because the parameters are the user's own. It is
/// decided from the collection: a facet is useful in proportion to how much of the collection carries it and how
/// evenly it divides what it covers. The evenness is the Shannon entropy of the counts of its values divided by the
/// entropy of the same number of equal values, which is 1 when every value is equally common and approaches 0 as
/// one value takes over.
///
/// The score is that product, and three adjustments keep it honest:
///
/// - A facet with one value divides nothing and scores zero, which is how [`crate::sidebar`] knows not to offer
///   it: choosing its only value would leave the list exactly as it is.
/// - A facet with a value for almost every figure — a run number, a note — is a search term rather than a facet,
///   and its score is cut hard.
/// - The labels always come first, because they are the facet the user wrote in order to browse by.
///
/// Facets with equal scores are ordered by name, so the list does not shuffle between frames.
#[must_use]
pub fn describe_facets(cards: &[FigureCard]) -> Vec<Facet> {
    let total = cards.len().max(1);
    let mut facets = Vec::new();

    let mut label_counts: BTreeMap<FacetValue, usize> = BTreeMap::new();
    for card in cards {
        for label in &card.labels {
            *label_counts
                .entry(FacetValue::Text(label.clone()))
                .or_default() += 1;
        }
    }
    if !label_counts.is_empty() {
        let present = cards.iter().filter(|card| !card.labels.is_empty()).count();
        facets.push(Facet {
            key: FacetKey::Labels,
            kind: FacetKind::Labels,
            present,
            values: label_counts.keys().cloned().collect(),
            range: None,
            // Labels lead whatever the arithmetic says, because they exist to be browsed by — but a single label
            // shared by every figure divides no more than a parameter would, and is not offered either.
            score: if label_counts.len() > 1 {
                f64::INFINITY
            } else {
                0.0
            },
        });
    }

    let mut by_name: BTreeMap<&str, Vec<&Parameter>> = BTreeMap::new();
    for card in cards {
        for (name, value) in &card.parameters {
            by_name.entry(name.as_str()).or_default().push(value);
        }
    }

    for (name, values) in by_name {
        let present = values.len();
        let mut counts: BTreeMap<FacetValue, usize> = BTreeMap::new();
        for value in &values {
            *counts.entry(FacetValue::from(*value)).or_default() += 1;
        }
        let kind = facet_kind(&values);
        let range = kind.is_numeric().then(|| number_range(&counts)).flatten();

        let cardinality = counts.len();
        let coverage = present as f64 / total as f64;
        let evenness = evenness(counts.values().copied(), present);
        // A facet whose values are nearly all distinct is a name rather than a division of the collection. The
        // threshold is half: below it the facet still puts figures together, above it it mostly does not. A numeric
        // facet is exempt, because a range narrows a column of distinct numbers perfectly well.
        let naming = !kind.is_numeric() && cardinality * 2 > present;
        let score = if cardinality < 2 {
            0.0
        } else {
            coverage * (0.35 + 0.65 * evenness) * if naming { 0.1 } else { 1.0 }
        };

        facets.push(Facet {
            key: FacetKey::parameter(name),
            kind,
            present,
            values: counts.keys().cloned().collect(),
            range,
            score,
        });
    }

    facets.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.key.name().cmp(b.key.name()))
    });
    facets
}

/// How evenly `counts`, which sum to `present`, are spread: the Shannon entropy of the counts over the entropy of
/// the same number of equal counts, which is 1 when the values are equally common and tends to 0 as one takes over.
fn evenness(counts: impl Iterator<Item = usize> + Clone, present: usize) -> f64 {
    let cardinality = counts.clone().count();
    if cardinality < 2 || present == 0 {
        return 0.0;
    }
    let mut entropy = 0.0;
    for count in counts {
        let share = count as f64 / present as f64;
        if share > 0.0 {
            entropy -= share * share.log2();
        }
    }
    entropy / (cardinality as f64).log2()
}

/// The form a facet offers, from the forms of its values: the one form they share, or text when they disagree,
/// because text is the only form every value can be written in.
///
/// Whole numbers and numbers are not a disagreement. One script writes an angle as `4` and another as `4.5`, and
/// the column is still a column of numbers that a range narrows perfectly well; calling it text because of that
/// would take the range away from exactly the parameters most worth having one.
fn facet_kind(values: &[&Parameter]) -> FacetKind {
    let of = |parameter: &Parameter| match parameter {
        Parameter::Bool(_) => FacetKind::Bool,
        Parameter::Integer(_) => FacetKind::Integer,
        Parameter::Number(_) => FacetKind::Number,
        Parameter::String(_) => FacetKind::Text,
    };
    let Some((first, rest)) = values.split_first() else {
        return FacetKind::Text;
    };
    let kind = of(first);
    if rest.iter().all(|value| of(value) == kind) {
        return kind;
    }
    if values.iter().all(|value| of(value).is_numeric()) {
        return FacetKind::Number;
    }
    FacetKind::Text
}

/// The smallest and largest finite numbers among `counts`, or `None` when there are none.
fn number_range(counts: &BTreeMap<FacetValue, usize>) -> Option<(f64, f64)> {
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for value in counts.keys() {
        if let Some(number) = value.as_number().filter(|number| number.is_finite()) {
            low = low.min(number);
            high = high.max(number);
        }
    }
    (low <= high).then_some((low, high))
}

/// How a named term of a [`Query`] compares the parameter to the text that was typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Comparison {
    /// The value contains the text, ignoring case: `solver:sst`.
    Contains,
    /// The value is the text, ignoring case: `rig=CFD`.
    Equals,
    /// The value is above the number: `angle>8`.
    Above,
    /// The value is the number or above it: `angle>=8`.
    AtLeast,
    /// The value is below the number: `angle<8`.
    Below,
    /// The value is the number or below it: `angle<=8`.
    AtMost,
}

/// One term of a [`Query`].
#[derive(Clone, Debug, PartialEq)]
pub enum Term {
    /// Text that must occur somewhere in the figure: its title, a label, or the name or value of a parameter.
    Anywhere {
        /// The text, folded to lower case.
        text: String,
        /// Whether the figure must instead not contain it.
        negated: bool,
    },
    /// A comparison against one named parameter, or against the labels when the name is `label` or `labels`.
    Named {
        /// The name of the parameter.
        key: String,
        /// How the value is compared.
        comparison: Comparison,
        /// The text that was typed after the operator.
        value: String,
        /// Whether the comparison must instead fail.
        negated: bool,
    },
}

/// What the user typed into the search field, parsed.
///
/// The field is the whole of the filtering for someone who would rather type than click, and the shape it takes is
/// the one that spread from issue trackers because it reads as what it means. A term is one run of non-space
/// characters:
///
/// | Typed | Meaning |
/// | --- | --- |
/// | `surface` | the figure mentions "surface" anywhere |
/// | `-stalled` | it does not mention "stalled" |
/// | `rig:CFD` | its `rig` parameter contains "CFD", ignoring case |
/// | `rig=CFD` | its `rig` parameter is exactly "CFD", ignoring case |
/// | `angle>=8` | its `angle` parameter is 8 or more |
/// | `label:piv` | one of its labels contains "piv" |
///
/// Terms are cumulative: every one of them must hold. A term whose name is not a parameter of a figure does not
/// match that figure, so `solver:LES` finds the computed runs and leaves the measured ones out, which is what
/// asking about a solver means.
///
/// A word only asks about a parameter when the collection has one of that name; `label` and `labels` always ask
/// about the labels. Anything else is free text, including a word with a colon in it that names no parameter, so
/// that a web address, a file path or a colon in a title narrows the list to the figures that mention it instead of
/// silently emptying it. A query therefore never fails to parse, and typing is never interrupted by an error.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    /// The terms, every one of which must hold.
    pub terms: Vec<Term>,
}

impl Query {
    /// Parses what was typed, against the parameter names the collection has, which
    /// [`parameter_names`] gathers.
    ///
    /// The names decide which words are questions about a parameter and which are text to search for; without them
    /// every word with a colon in it would be a question about a parameter that does not exist, and would match
    /// nothing.
    #[must_use]
    pub fn parse(text: &str, names: &BTreeSet<String>) -> Self {
        let terms = text
            .split_whitespace()
            .filter_map(|word| parse_term(word, names))
            .collect();
        Self { terms }
    }

    /// Whether the query asks nothing, so that every figure matches it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Whether `card` satisfies every term.
    #[must_use]
    pub fn matches(&self, card: &FigureCard) -> bool {
        if self.terms.is_empty() {
            return true;
        }
        let haystack = card.haystack();
        self.terms
            .iter()
            .all(|term| matches_term(term, card, &haystack))
    }
}

/// Parses one run of non-space characters, or nothing when it is a bare `-`.
fn parse_term(word: &str, names: &BTreeSet<String>) -> Option<Term> {
    let (negated, body) = match word.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, word),
    };
    if body.is_empty() {
        return None;
    }
    if let Some((key, comparison, value)) = split_comparison(body)
        && !value.is_empty()
        && (names.contains(key) || is_labels_key(key))
    {
        return Some(Term::Named {
            key: key.to_owned(),
            comparison,
            value: value.to_owned(),
            negated,
        });
    }
    Some(Term::Anywhere {
        text: body.to_lowercase(),
        negated,
    })
}

/// Whether a name in a query asks about the labels rather than about a parameter.
fn is_labels_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("label") || key.eq_ignore_ascii_case("labels")
}

/// The name of every parameter carried by any figure of the collection.
///
/// It is what tells a question about a parameter from a word that merely has a colon in it, and it is gathered from
/// the collection rather than declared, because the names are the user's own.
#[must_use]
pub fn parameter_names(cards: &[FigureCard]) -> BTreeSet<String> {
    cards
        .iter()
        .flat_map(|card| card.parameters.keys().cloned())
        .collect()
}

/// Splits `body` into a parameter name, a comparison and the text after it, when it has the shape of a named term:
/// a name of word characters, then one of the operators.
fn split_comparison(body: &str) -> Option<(&str, Comparison, &str)> {
    let operators = [
        (">=", Comparison::AtLeast),
        ("<=", Comparison::AtMost),
        (">", Comparison::Above),
        ("<", Comparison::Below),
        (":", Comparison::Contains),
        ("=", Comparison::Equals),
    ];
    let mut best: Option<(usize, &str, Comparison)> = None;
    for (mark, comparison) in operators {
        if let Some(at) = body.find(mark)
            && best.is_none_or(|(found, _, _)| at < found)
        {
            best = Some((at, mark, comparison));
        }
    }
    let (at, mark, comparison) = best?;
    let key = &body[..at];
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '-')
    {
        return None;
    }
    Some((key, comparison, &body[at + mark.len()..]))
}

/// Whether `card` satisfies one term.
fn matches_term(term: &Term, card: &FigureCard, haystack: &str) -> bool {
    let held = match term {
        Term::Anywhere { text, .. } => haystack.contains(text.as_str()),
        Term::Named {
            key,
            comparison,
            value,
            ..
        } => {
            if is_labels_key(key) {
                card.labels
                    .iter()
                    .any(|label| compare_text(label, *comparison, value))
            } else {
                match card.parameter(key) {
                    Some(parameter) => compare(&FacetValue::from(parameter), *comparison, value),
                    // A figure that does not carry the parameter is not what was asked for.
                    None => false,
                }
            }
        }
    };
    let negated = match term {
        Term::Anywhere { negated, .. } | Term::Named { negated, .. } => *negated,
    };
    held != negated
}

/// Compares a facet value against the text that was typed.
fn compare(value: &FacetValue, comparison: Comparison, text: &str) -> bool {
    match comparison {
        Comparison::Contains | Comparison::Equals => compare_text(&value.text(), comparison, text),
        Comparison::Above | Comparison::AtLeast | Comparison::Below | Comparison::AtMost => {
            // Underscores are allowed inside a typed number so that `mesh_cells>=2_000_000` reads as it is written
            // in the code that set the parameter.
            let Ok(wanted) = text.replace('_', "").parse::<f64>() else {
                return false;
            };
            let Some(held) = value.as_number() else {
                return false;
            };
            match comparison {
                Comparison::Above => held > wanted,
                Comparison::AtLeast => held >= wanted,
                Comparison::Below => held < wanted,
                _ => held <= wanted,
            }
        }
    }
}

/// Compares text against text, ignoring case.
fn compare_text(value: &str, comparison: Comparison, text: &str) -> bool {
    let value = value.to_lowercase();
    let text = text.to_lowercase();
    match comparison {
        Comparison::Equals => value == text,
        _ => value.contains(&text),
    }
}

/// How one facet has been narrowed.
#[derive(Clone, Debug, PartialEq)]
pub enum Constraint {
    /// The figure's value is one of these, so choosing more values widens the result.
    AnyOf(Vec<FacetValue>),
    /// The figure's value is a number from `low` to `high`, both included.
    Between {
        /// The smallest value kept.
        low: f64,
        /// The largest value kept.
        high: f64,
    },
}

/// One facet, narrowed.
#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    /// The facet narrowed.
    pub key: FacetKey,
    /// How it is narrowed.
    pub constraint: Constraint,
}

impl Filter {
    /// Whether `card` passes the filter.
    ///
    /// A figure that does not carry the facet never passes: a refinement is a statement about the parameter, and a
    /// figure without it cannot satisfy one.
    #[must_use]
    pub fn matches(&self, card: &FigureCard) -> bool {
        match (&self.key, &self.constraint) {
            (FacetKey::Labels, Constraint::AnyOf(wanted)) => card.labels.iter().any(|label| {
                wanted
                    .iter()
                    .any(|value| matches!(value, FacetValue::Text(text) if text == label))
            }),
            (FacetKey::Labels, Constraint::Between { .. }) => false,
            (FacetKey::Parameter(name), constraint) => match card.parameter(name) {
                None => false,
                Some(parameter) => {
                    let value = FacetValue::from(parameter);
                    match constraint {
                        Constraint::AnyOf(wanted) => wanted.contains(&value),
                        Constraint::Between { low, high } => value
                            .as_number()
                            .is_some_and(|number| number >= *low && number <= *high),
                    }
                }
            },
        }
    }
}

/// What a collection is put in order by.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SortKey {
    /// The title of the figure.
    Title,
    /// A named parameter. Figures without it come last, whichever way the order runs.
    Parameter(String),
}

/// The order a collection is listed in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Sort {
    /// What the order is taken from.
    pub key: SortKey,
    /// Whether the order runs from the largest to the smallest.
    pub descending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self {
            key: SortKey::Title,
            descending: false,
        }
    }
}

/// One run of figures in the result, under the value they share.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// The value the figures of the group share, or [`NOT_SET`] or [`NO_LABELS`] for the figures the grouping does
    /// not apply to. It is `None` when the result is not grouped.
    pub name: Option<String>,
    /// The figures of the group, as indices into the collection, in the sorted order.
    pub members: Vec<usize>,
}

/// What a collection narrows to.
#[derive(Clone, Debug, PartialEq)]
pub struct Results {
    /// The figures to show, in one group when the result is not grouped.
    ///
    /// Groups are in the order their names read in, by the same rule the figures inside them are ordered by, so
    /// that a grouping by run number runs 9, 10, 11. The group of the figures the grouping does not apply to is
    /// last whatever its name.
    pub groups: Vec<Group>,
    /// How many figures matched.
    pub matched: usize,
    /// How many figures the collection holds.
    pub total: usize,
}

impl Results {
    /// Every matching figure in the order shown, as indices into the collection.
    ///
    /// A figure grouped by labels is listed under each of its labels, so this may be longer than
    /// [`Results::matched`].
    pub fn members(&self) -> impl Iterator<Item = usize> + '_ {
        self.groups
            .iter()
            .flat_map(|group| group.members.iter().copied())
    }

    /// Whether nothing matched.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.matched == 0
    }
}

/// What the user has chosen: the query they typed, the facets they narrowed, the order and the grouping.
///
/// It is a plain value, so the interface can keep one per collection and the tests can drive it directly.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Browse {
    /// What is typed in the search field.
    pub query: String,
    /// The facets narrowed, in the order they were narrowed, which is the order their chips are shown in.
    pub filters: Vec<Filter>,
    /// The order the result is listed in.
    pub sort: Sort,
    /// The facet the result is grouped by, or `None` for a flat list.
    pub group: Option<FacetKey>,
}

impl Browse {
    /// Whether nothing has been typed or ticked, so that there is nothing for the reader to take back.
    ///
    /// It asks whether anything has been *entered*, not whether the result is the whole collection: text that
    /// parses to no terms, such as a lone minus, leaves every figure in the list but is still something sitting in
    /// the search field, and the control that clears it has to be offered while it is there.
    #[must_use]
    pub fn is_unfiltered(&self) -> bool {
        self.filters.is_empty() && self.query.trim().is_empty()
    }

    /// The filter on a facet, if it has one.
    #[must_use]
    pub fn filter(&self, key: &FacetKey) -> Option<&Filter> {
        self.filters.iter().find(|filter| &filter.key == key)
    }

    /// Adds `value` to the facet's chosen values, or takes it away when it is already chosen, removing the filter
    /// when the last value is taken away.
    ///
    /// This is what clicking a value in the interface does, and it is why a facet's filter can never be an empty
    /// list of alternatives, which would match nothing.
    pub fn toggle(&mut self, key: &FacetKey, value: &FacetValue) {
        let Some(position) = self.filters.iter().position(|filter| &filter.key == key) else {
            self.filters.push(Filter {
                key: key.clone(),
                constraint: Constraint::AnyOf(vec![value.clone()]),
            });
            return;
        };
        let Constraint::AnyOf(values) = &mut self.filters[position].constraint else {
            // The facet was narrowed by a range, and choosing a value replaces it.
            self.filters[position].constraint = Constraint::AnyOf(vec![value.clone()]);
            return;
        };
        match values.iter().position(|held| held == value) {
            Some(at) => {
                values.remove(at);
                if values.is_empty() {
                    self.filters.remove(position);
                }
            }
            None => values.push(value.clone()),
        }
    }

    /// Narrows a numeric facet to the numbers from `low` to `high`.
    ///
    /// A range that covers the whole of `facet` is no narrowing at all, and removes the filter instead, so that the
    /// interface never shows a chip that keeps everything.
    pub fn set_range(&mut self, facet: &Facet, low: f64, high: f64) {
        let covers_all = facet
            .range
            .is_some_and(|(least, most)| low <= least && high >= most);
        if covers_all || low > high {
            self.remove(&facet.key);
            return;
        }
        let constraint = Constraint::Between { low, high };
        match self
            .filters
            .iter_mut()
            .find(|filter| filter.key == facet.key)
        {
            Some(filter) => filter.constraint = constraint,
            None => self.filters.push(Filter {
                key: facet.key.clone(),
                constraint,
            }),
        }
    }

    /// Removes the filter on a facet, if it has one.
    pub fn remove(&mut self, key: &FacetKey) {
        self.filters.retain(|filter| &filter.key != key);
    }

    /// Takes back every choice, including what was typed.
    pub fn clear(&mut self) {
        self.query.clear();
        self.filters.clear();
    }

    /// Narrows `cards` to what the user asked for, in the order and grouping they asked for.
    #[must_use]
    pub fn results(&self, cards: &[FigureCard]) -> Results {
        let query = Query::parse(&self.query, &parameter_names(cards));
        let mut kept: Vec<usize> = (0..cards.len())
            .filter(|index| self.keeps(&cards[*index], &query, None))
            .collect();
        kept.sort_by(|a, b| self.order(&cards[*a], &cards[*b]));
        let matched = kept.len();

        let groups = match &self.group {
            None => vec![Group {
                name: None,
                members: kept,
            }],
            Some(key) => {
                // A map keyed by the group's name gathers the members of each group in the sorted order, because
                // the kept indices are visited in that order.
                let mut buckets: BTreeMap<String, Vec<usize>> = BTreeMap::new();
                let mut absent: Vec<usize> = Vec::new();
                for index in kept {
                    match group_names(&cards[index], key) {
                        None => absent.push(index),
                        Some(names) => {
                            for name in names {
                                buckets.entry(name).or_default().push(index);
                            }
                        }
                    }
                }
                let mut groups: Vec<Group> = buckets
                    .into_iter()
                    .map(|(name, members)| Group {
                        name: Some(name),
                        members,
                    })
                    .collect();
                // Groups are ordered as the figures inside them are, so that grouping by a run number reads 9, 10,
                // 11 rather than 10, 11, 9.
                groups.sort_by(|a, b| match (&a.name, &b.name) {
                    (Some(a), Some(b)) => natural(a, b),
                    _ => Ordering::Equal,
                });
                if !absent.is_empty() {
                    // The figures the grouping does not apply to come last, because they are what the reader is
                    // least likely to be looking for under a grouping they chose.
                    groups.push(Group {
                        name: Some(
                            match key {
                                FacetKey::Labels => NO_LABELS,
                                FacetKey::Parameter(_) => NOT_SET,
                            }
                            .to_owned(),
                        ),
                        members: absent,
                    });
                }
                groups
            }
        };

        Results {
            groups,
            matched,
            total: cards.len(),
        }
    }

    /// How many figures each value of `facet` would leave.
    ///
    /// Every filter is applied except the one on `facet` itself, so that the count beside a value is how many
    /// figures choosing it would add, and a facet's own choices never hide the alternatives to them. Values of the
    /// facet that nothing would leave are present with a count of zero, so the interface can show them as
    /// unavailable rather than have them disappear as the reader reaches for them.
    #[must_use]
    pub fn counts(&self, cards: &[FigureCard], facet: &Facet) -> BTreeMap<FacetValue, usize> {
        let query = Query::parse(&self.query, &parameter_names(cards));
        let mut counts: BTreeMap<FacetValue, usize> = facet
            .values
            .iter()
            .map(|value| (value.clone(), 0))
            .collect();
        for card in cards {
            if !self.keeps(card, &query, Some(&facet.key)) {
                continue;
            }
            match &facet.key {
                FacetKey::Labels => {
                    for label in &card.labels {
                        if let Some(count) = counts.get_mut(&FacetValue::Text(label.clone())) {
                            *count += 1;
                        }
                    }
                }
                FacetKey::Parameter(name) => {
                    if let Some(parameter) = card.parameter(name)
                        && let Some(count) = counts.get_mut(&FacetValue::from(parameter))
                    {
                        *count += 1;
                    }
                }
            }
        }
        counts
    }

    /// Whether `card` passes the query and every filter but the one on `except`.
    fn keeps(&self, card: &FigureCard, query: &Query, except: Option<&FacetKey>) -> bool {
        query.matches(card)
            && self
                .filters
                .iter()
                .filter(|filter| Some(&filter.key) != except)
                .all(|filter| filter.matches(card))
    }

    /// The order of two figures under the chosen sort.
    ///
    /// Titles break a tie, so the list is in the same order every time it is built, and a figure that does not
    /// carry the parameter comes last whichever way the order runs: it has no place in an order taken from a value
    /// it does not have, and burying it at the far end would hide it from someone who reversed the order to find
    /// it.
    fn order(&self, a: &FigureCard, b: &FigureCard) -> Ordering {
        let ordering = match &self.sort.key {
            SortKey::Title => natural(&a.title, &b.title),
            SortKey::Parameter(name) => match (a.parameter(name), b.parameter(name)) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => return Ordering::Greater,
                (Some(_), None) => return Ordering::Less,
                (Some(left), Some(right)) => {
                    let (left, right) = (FacetValue::from(left), FacetValue::from(right));
                    match (left.as_number(), right.as_number()) {
                        (Some(left), Some(right)) => left.total_cmp(&right),
                        _ => natural(&left.text(), &right.text()),
                    }
                }
            },
        };
        let ordering = if self.sort.descending {
            ordering.reverse()
        } else {
            ordering
        };
        ordering.then_with(|| natural(&a.title, &b.title))
    }
}

/// The names of the groups `card` belongs to under `key`, or `None` when the grouping does not apply to it.
///
/// A figure belongs to one group under a parameter and to one group per label under the labels, because a figure
/// has several labels and browsing by them means finding it under each.
fn group_names(card: &FigureCard, key: &FacetKey) -> Option<Vec<String>> {
    match key {
        FacetKey::Labels => (!card.labels.is_empty()).then(|| card.labels.clone()),
        FacetKey::Parameter(name) => card
            .parameter(name)
            .map(|parameter| vec![FacetValue::from(parameter).text()]),
    }
}

/// Compares two strings so that the numbers in them read as numbers: "Run 9" comes before "Run 10".
///
/// Titles of figures from a campaign are numbered, and ordering them by their characters would put every figure
/// whose number begins with a 1 before every figure whose number begins with a 2.
fn natural(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let run = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut digits = String::new();
                    while chars.peek().is_some_and(char::is_ascii_digit) {
                        digits.push(chars.next().expect("peeked"));
                    }
                    digits
                };
                let (x, y) = (run(&mut left), run(&mut right));
                // A run of digits longer than eighteen characters cannot be a figure number; comparing the runs as
                // text is then as good an answer as any, and it never panics.
                let ordering = match (x.parse::<u128>(), y.parse::<u128>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    _ => x.cmp(&y),
                };
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(x), Some(y)) => {
                let ordering = x
                    .to_lowercase()
                    .cmp(y.to_lowercase())
                    .then_with(|| x.cmp(&y));
                if ordering != Ordering::Equal {
                    return ordering;
                }
                left.next();
                right.next();
            }
        }
    }
}
