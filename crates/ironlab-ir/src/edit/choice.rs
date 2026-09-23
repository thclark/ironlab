//! The choices of a value type: the values a property of that type can be given by
//! picking one from a list.
//!
//! A property whose type is a tagged value (limits, a colour specification, a
//! projection and the rest) or a plain enumeration (a scale, a colormap name, a marker
//! shape and the rest) is changed by choosing one of a fixed set of alternatives rather
//! than by typing a number. [`choices`] returns that set, each alternative labelled for
//! a combo box and carrying a value that the property can be set to at once.
//!
//! Every type and every variant is listed exactly once in this module, in an exhaustive
//! match, so a value type or a variant added to the IR does not compile until it is
//! given its choices here.
//!
//! [`choices`] answers what a *type* can hold, which is all a caller with no figure can
//! ask. [`property_choices`] answers the same question for a *property of a node*, and
//! marks each choice with whether the IR would act on it there: a colour that the
//! colormap indexes means nothing where the IR has no value to index it by. Such a
//! choice is still returned, carrying the sentence that says why it is unavailable and
//! where it is available, so that a combo box can show it disabled rather than hide it
//! and leave the user to guess that the value exists. Which choices the IR acts on is a
//! fact about the semantics of the IR rather than about any user interface, so it is
//! stated here and not in the viewer.

use crate::artist::{
    ContourPlacement, Grid, ImagePlane, Levels, OutOfRange, QuiverScale, ScatterColor, ScatterSize,
};
use crate::axes::{ColormapName, LegendLocation, Limits, Projection, Scale, View3d};
use crate::edit::path::PropertyPath;
use crate::edit::registry::{NodeKind, properties};
use crate::edit::value::{Value, ValueType};
use crate::figure::{Figure, FontSetId};
use crate::ids::{DataId, NodeId};
use crate::style::{Color, ColorSpec, DashStyle, MarkerShape};
use crate::text::Interpreter;

/// One of the values that a property of a given type can be given, as offered by
/// [`choices`].
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    /// The name of the choice, written for a combo box: a short noun phrase with only
    /// its first word capitalised, such as `Manual` or `Three-dimensional`. The labels
    /// of one type are distinct.
    pub label: &'static str,
    /// A value of the choice, which the property can be set to as it stands.
    pub value: Value,
    /// Why the IR would not act on this choice at the property it was asked for, or
    /// `None` when it would.
    ///
    /// The reason is a short sentence or two, written for the user: it says what the
    /// property lacks and where the same choice is available instead, so that a choice
    /// shown disabled tells the user how to reach it rather than only that they cannot.
    /// [`choices`] is asked about a type rather than about a property and leaves this
    /// `None`; [`property_choices`] fills it in.
    pub unavailable: Option<&'static str>,
}

impl Choice {
    /// Returns whether the choice can be taken at the property it was asked for, which is
    /// the negation of [`unavailable`](Choice::unavailable).
    #[must_use]
    pub fn available(&self) -> bool {
        self.unavailable.is_none()
    }

    /// Returns whether `value` is of the variant that this choice selects, whatever
    /// contents it carries.
    ///
    /// The choice of manual limits therefore matches every manual limits, not only the
    /// bounds that this choice would set, so the combo box of a property shows the
    /// choice the property currently holds. A value of another type never matches.
    #[must_use]
    pub fn matches(&self, value: &Value) -> bool {
        let (Some(mine), Some(theirs)) = (label_of(&self.value), label_of(value)) else {
            return false;
        };
        std::mem::discriminant(&self.value) == std::mem::discriminant(value) && mine == theirs
    }
}

/// The bounds that manual limits are given when the value they replace holds none,
/// because automatic limits are resolved by the scene compiler rather than by the IR.
const FALLBACK_LIMITS: (f64, f64) = (0.0, 1.0);

/// The level that explicit contour levels start from when the value they replace holds
/// none; a single level is the smallest list that the IR accepts.
const FALLBACK_LEVEL: f64 = 0.0;

/// The array that a choice refers to when it needs data and the value it replaces names
/// none. The figure need not have it, in which case applying the choice is refused and
/// the user picks an array of the right shape instead.
const FALLBACK_DATA: DataId = DataId(0);

/// The marker size that a single scatter size starts from, which is the size of a
/// scatter built from [`ScatterSize::default`].
const FALLBACK_MARKER_SIZE_PT: f64 = 4.0;

/// The number of levels that automatic levels start from, which is the count of
/// [`Levels::default`].
const FALLBACK_LEVEL_COUNT: u32 = 10;

/// The multiplier that a quiver scale factor starts from, which leaves the arrows the
/// length that the automatic scale gives them.
const FALLBACK_QUIVER_FACTOR: f64 = 1.0;

/// Returns the choices of a value type, in the order in which they are offered, each
/// carrying a value that suits the value it replaces.
///
/// `replacing` is the value that the choice would replace, which the viewer reads from
/// the property being edited. A choice takes from it everything that still applies, so
/// that switching between the variants of a tagged value loses as little as possible:
/// manual limits keep the bounds of the manual limits they replace, a three-dimensional
/// projection keeps the camera of the projection it replaces, a fixed colour keeps the
/// colour, a grid keeps its coordinate arrays, and explicit levels keep their values.
/// What a choice cannot take it falls back on: manual limits run from 0 to 1, explicit
/// levels hold the single level 0, and a choice that needs a data array refers to
/// [`DataId(0)`](DataId), which the figure need not have. Passing `None` uses every
/// fallback.
///
/// A type that is neither a tagged value nor an enumeration has no choices and returns
/// an empty list: a number, a string, a colour, a text and every value that is a
/// structure of fields are edited field by field instead. The match over the value types
/// and the matches over the variants of each type are exhaustive, so a type or a variant
/// added to the IR does not compile until its choices are declared here.
///
/// Every choice returned here is [available](Choice::available), because whether the IR
/// acts on a choice depends on the property it is offered for rather than on its type.
/// [`property_choices`] answers that question.
#[must_use]
pub fn choices(value_type: ValueType, replacing: Option<&Value>) -> Vec<Choice> {
    match value_type {
        ValueType::Projection => projection_choices(replacing),
        ValueType::Limits => limits_choices(replacing),
        ValueType::ColorSpec => color_spec_choices(replacing),
        ValueType::ScatterSize => scatter_size_choices(replacing),
        ValueType::ScatterColor => scatter_color_choices(replacing),
        ValueType::Grid => grid_choices(replacing),
        ValueType::Levels => levels_choices(replacing),
        ValueType::ContourPlacement => contour_placement_choices(replacing),
        ValueType::QuiverScale => quiver_scale_choices(replacing),
        ValueType::ImagePlane => image_plane_choices(replacing),
        ValueType::OutOfRange => out_of_range_choices(replacing),
        ValueType::Scale => scale_choices(),
        ValueType::ColormapName => colormap_name_choices(),
        ValueType::LegendLocation => legend_location_choices(),
        ValueType::MarkerShape => marker_shape_choices(),
        ValueType::DashStyle => dash_style_choices(),
        ValueType::Interpreter => interpreter_choices(),
        ValueType::FontSetId => font_set_id_choices(),
        ValueType::Bool
        | ValueType::UInt32
        | ValueType::Double
        | ValueType::Float
        | ValueType::String
        | ValueType::DataId
        | ValueType::Doubles
        | ValueType::Strings
        | ValueType::Text
        | ValueType::FigureSize
        | ValueType::Color
        | ValueType::TileLayout
        | ValueType::Links
        | ValueType::Parameters
        | ValueType::Cell
        | ValueType::View3d
        | ValueType::Axis
        | ValueType::Legend
        | ValueType::LineStyle
        | ValueType::MarkerStyle
        | ValueType::ImagePlacement
        | ValueType::PixelRange => Vec::new(),
    }
}

/// Returns every choice of one property of one node of a figure, each marked with
/// whether the IR would act on it there and, when it would not, with the reason.
///
/// A choice the IR would not act on is returned rather than withheld, carrying its
/// reason in [`Choice::unavailable`], so that the property editor can show it disabled.
/// A user who can see a value they cannot pick, and read what would make it available,
/// learns what the figure can do; a value removed from the list teaches nothing.
///
/// Two choices are unavailable somewhere. A colormapped colour is looked up in the axes'
/// colormap only where the IR gives it a value to be looked up by: the level of each
/// isoline of a contour (`line.color`), and the height or colour data of each face of a
/// surface (`face` and `edge`). The colour of a line or of a quiver, and the single
/// colour of a scatter, have no such value, and the scene compiler paints the whole
/// artist in the middle colour of the colormap instead; a marker takes the colour of the
/// plot it belongs to, so a colormapped marker is drawn exactly as an automatic one.
/// Choosing a scatter's colour "from data" stays available, because that names the array
/// to look the colour up by. A clamp, which paints a pixel of an image in the nearest end
/// colour of the colormap, is unavailable at the `non_finite` category of a
/// colour-indexed or colour-mapped image, because a non-finite value has no nearest end;
/// it stays available at the `below` and `above` categories.
///
/// Returns an empty list when the node is not in the figure, when the path is not a
/// property of its kind, when the path is not currently reachable (a property of a
/// variant that is not set), or when the property's type has no choices at all.
#[must_use]
pub fn property_choices(figure: &Figure, node: NodeId, path: &PropertyPath) -> Vec<Choice> {
    let Some(kind) = figure.node_kind(node) else {
        return Vec::new();
    };
    let Some(property) = properties(kind)
        .into_iter()
        .find(|property| property.path == *path)
    else {
        return Vec::new();
    };
    let Ok(value) = figure.get(node, path) else {
        return Vec::new();
    };
    let mut offered = choices(property.value_type, Some(&value));
    for choice in &mut offered {
        choice.unavailable = RULES
            .iter()
            .find_map(|rule| rule(figure, kind, node, path, &choice.value));
    }
    offered
}

/// A rule that says whether the IR would act on one choice at one property of one node.
///
/// A rule is given the figure, the kind of the node, the node, the path of the property
/// and the value the choice would set, and returns the sentence shown when the IR would
/// not act on that value there, or `None` when it would. A rule that concerns another
/// value type, or another kind of node, answers `None` for everything else.
type Rule = fn(&Figure, NodeKind, NodeId, &PropertyPath, &Value) -> Option<&'static str>;

/// Every rule that marks a choice unavailable, asked in order about every choice of every
/// property; the first reason given is the one shown.
///
/// A new rule is a function of the type above added to this list, so marking a choice
/// unavailable never needs the property editor, or this module's callers, to change.
const RULES: [Rule; 2] = [
    colormapped_without_a_value_to_index_by,
    clamp_without_a_nearest_end_of_the_colormap,
];

/// Completes the reason a colormapped colour is unavailable at a property, which is why
/// the property holds no value to look a colour up by followed by where the colormap can
/// be used instead.
macro_rules! colormapped_unavailable {
    ($why:literal) => {
        concat!(
            $why,
            " Colouring by the colormap is available for the faces and edges of a \
             surface, for the isolines of a contour, which take the colour of their \
             level, and for a scatter whose colour comes from data."
        )
    };
}

/// Why a colormapped colour is unavailable on a line.
const COLORMAPPED_ON_A_LINE: &str = colormapped_unavailable!(
    "A line is drawn in one colour and provides no value to look that colour up by, so a \
     colormapped line is drawn in the middle colour of the colormap."
);

/// Why a colormapped colour is unavailable on a quiver.
const COLORMAPPED_ON_A_QUIVER: &str = colormapped_unavailable!(
    "A quiver is drawn in one colour and provides no value to look that colour up by, so \
     colormapped arrows are drawn in the middle colour of the colormap."
);

/// Why a colormapped colour is unavailable as the single colour of a scatter.
const COLORMAPPED_ON_A_SINGLE_SCATTER_COLOUR: &str = colormapped_unavailable!(
    "A scatter that has one colour for every marker provides no value to look that \
     colour up by, so colormapped markers are drawn in the middle colour of the colormap."
);

/// Why a colormapped colour is unavailable on the face or the edge of a marker.
const COLORMAPPED_ON_A_MARKER: &str = colormapped_unavailable!(
    "A marker takes the colour of the plot it belongs to rather than looking a colour up \
     itself, so a colormapped marker is drawn exactly as an automatic one."
);

/// Why a colormapped colour is unavailable anywhere else.
const COLORMAPPED_ELSEWHERE: &str = colormapped_unavailable!(
    "This property provides no value to look a colour up by, so a colormapped colour \
     here is drawn in the middle colour of the colormap."
);

/// Marks a colormapped colour unavailable wherever the IR holds no value to look the
/// colour up by, and leaves every other choice and every other property alone.
fn colormapped_without_a_value_to_index_by(
    _figure: &Figure,
    kind: NodeKind,
    _node: NodeId,
    path: &PropertyPath,
    value: &Value,
) -> Option<&'static str> {
    if !matches!(value, Value::ColorSpec(ColorSpec::Colormapped))
        || indexes_the_colormap(kind, path)
    {
        return None;
    }
    if path
        .segments()
        .first()
        .is_some_and(|first| first == "marker")
    {
        return Some(COLORMAPPED_ON_A_MARKER);
    }
    // The match is exhaustive, so a kind added to the IR does not compile until it is
    // said why a colour of that kind cannot be colormapped.
    Some(match kind {
        NodeKind::Line => COLORMAPPED_ON_A_LINE,
        NodeKind::Quiver => COLORMAPPED_ON_A_QUIVER,
        NodeKind::Scatter => COLORMAPPED_ON_A_SINGLE_SCATTER_COLOUR,
        // Every colour of a contour and of a surface indexes the colormap, and a figure,
        // an axes and an image hold no colour specification at all (the fixed colour of
        // an image's out-of-range policy is a colour, not a specification), so these
        // kinds reach this reason only if they gain a colour that indexes nothing.
        NodeKind::Contour
        | NodeKind::Surface
        | NodeKind::Figure
        | NodeKind::Axes
        | NodeKind::Image
        | NodeKind::IndexedImage
        | NodeKind::MappedImage => COLORMAPPED_ELSEWHERE,
    })
}

/// Why a clamp is unavailable at the non-finite category of an image.
const CLAMP_AT_NON_FINITE: &str = "A non-finite value has no nearest end of the colormap \
     to be clamped to, so clamping is available only below and above the range.";

/// Marks a clamp unavailable at the `non_finite` category of a colour-indexed or
/// colour-mapped image, where the pixels have no nearest end of the colormap to be
/// clamped to, and leaves every other choice and every other property alone.
fn clamp_without_a_nearest_end_of_the_colormap(
    _figure: &Figure,
    kind: NodeKind,
    _node: NodeId,
    path: &PropertyPath,
    value: &Value,
) -> Option<&'static str> {
    let clamp = matches!(value, Value::OutOfRange(OutOfRange::Clamp));
    let image = matches!(kind, NodeKind::IndexedImage | NodeKind::MappedImage);
    (clamp && image && path.segments() == ["non_finite"]).then_some(CLAMP_AT_NON_FINITE)
}

/// Returns whether the IR gives the colour at this property of this kind of node a value
/// to look up in the colormap.
///
/// The match over the kinds of node is exhaustive, so a kind added to the IR does not
/// compile until it is said what its colormapped colours index.
fn indexes_the_colormap(kind: NodeKind, path: &PropertyPath) -> bool {
    let segments = path.segments();
    match kind {
        // Each isoline is coloured by its own level.
        NodeKind::Contour => segments == ["line", "color"],
        // Each face is coloured by its colour data, or by its height when it has none.
        NodeKind::Surface => segments == ["face"] || segments == ["edge"],
        // An image holds no colour specification: a true-colour image carries its own
        // colours, and the two mapped kinds index the colormap by their data rather
        // than by a colour property.
        NodeKind::Figure
        | NodeKind::Axes
        | NodeKind::Line
        | NodeKind::Scatter
        | NodeKind::Quiver
        | NodeKind::Image
        | NodeKind::IndexedImage
        | NodeKind::MappedImage => false,
    }
}

/// Returns the label of the choice that a value is, or `None` when its type has no
/// choices.
///
/// The match is exhaustive, so a variant added to [`Value`] does not compile until it is
/// said here whether its type has choices.
fn label_of(value: &Value) -> Option<&'static str> {
    Some(match value {
        Value::Projection(v) => projection_label(v),
        Value::Limits(v) => limits_label(v),
        Value::ColorSpec(v) => color_spec_label(v),
        Value::ScatterSize(v) => scatter_size_label(v),
        Value::ScatterColor(v) => scatter_color_label(v),
        Value::Grid(v) => grid_label(v),
        Value::Levels(v) => levels_label(v),
        Value::ContourPlacement(v) => contour_placement_label(v),
        Value::QuiverScale(v) => quiver_scale_label(v),
        Value::ImagePlane(v) => image_plane_label(v),
        Value::OutOfRange(v) => out_of_range_label(v),
        Value::Scale(v) => scale_label(v),
        Value::ColormapName(v) => colormap_name_label(v),
        Value::LegendLocation(v) => legend_location_label(v),
        Value::MarkerShape(v) => marker_shape_label(v),
        Value::DashStyle(v) => dash_style_label(v),
        Value::Interpreter(v) => interpreter_label(v),
        Value::FontSetId(v) => font_set_id_label(v),
        Value::Unset
        | Value::Bool(_)
        | Value::UInt32(_)
        | Value::Double(_)
        | Value::Float(_)
        | Value::String(_)
        | Value::DataId(_)
        | Value::Doubles(_)
        | Value::Strings(_)
        | Value::Text(_)
        | Value::FigureSize(_)
        | Value::Color(_)
        | Value::TileLayout(_)
        | Value::Links(_)
        | Value::Parameters(_)
        | Value::Cell(_)
        | Value::View3d(_)
        | Value::Axis(_)
        | Value::Legend(_)
        | Value::LineStyle(_)
        | Value::MarkerStyle(_)
        | Value::ImagePlacement(_)
        | Value::PixelRange(_) => return None,
    })
}

/// Declares the choices of a type whose variants carry fields, from which the list of
/// choices and the label of a value are generated together.
///
/// Each variant names the pattern that recognises it, the label under which it is
/// offered, and the value offered, which may read the value being replaced under the
/// name the declaration gives it after `replacing`.
macro_rules! tagged_choices {
    ($(
        $ty:ident as $variant:ident ($offer:ident, $label_of:ident) replacing $replacing:ident {
            $( $case:pat => $label:literal = $default:expr; )+
        }
    )*) => {$(
        fn $offer($replacing: Option<&Value>) -> Vec<Choice> {
            vec![$( Choice {
                label: $label,
                value: Value::$variant($default),
                unavailable: None,
            }, )+]
        }

        /// The match is exhaustive, so a variant added to the type does not compile
        /// until it is given a choice.
        fn $label_of(value: &$ty) -> &'static str {
            match value { $( $case => $label, )+ }
        }
    )*};
}

/// Declares the choices of an enumeration whose variants carry nothing, whose value is
/// the variant itself.
macro_rules! plain_choices {
    ($(
        $ty:ident as $variant:ident ($offer:ident, $label_of:ident) {
            $( $case:path => $label:literal; )+
        }
    )*) => {$(
        fn $offer() -> Vec<Choice> {
            vec![$( Choice {
                label: $label,
                value: Value::$variant($case),
                unavailable: None,
            }, )+]
        }

        /// The match is exhaustive, so a variant added to the enumeration does not
        /// compile until it is given a choice.
        fn $label_of(value: &$ty) -> &'static str {
            match value { $( $case => $label, )+ }
        }
    )*};
}

tagged_choices! {
    Projection as Projection (projection_choices, projection_label) replacing replacing {
        Projection::TwoD => "Two-dimensional" = Projection::TwoD;
        Projection::ThreeD { .. } => "Three-dimensional" = Projection::ThreeD {
            view3d: match replacing {
                Some(Value::Projection(Projection::ThreeD { view3d })) => *view3d,
                _ => View3d::default(),
            },
        };
    }

    Limits as Limits (limits_choices, limits_label) replacing replacing {
        Limits::Auto => "Automatic" = Limits::Auto;
        Limits::Manual { .. } => "Manual" = {
            let (min, max) = match replacing {
                Some(Value::Limits(Limits::Manual { min, max })) => (*min, *max),
                _ => FALLBACK_LIMITS,
            };
            Limits::Manual { min, max }
        };
    }

    ColorSpec as ColorSpec (color_spec_choices, color_spec_label) replacing replacing {
        ColorSpec::Auto => "Automatic" = ColorSpec::Auto;
        ColorSpec::Rgba { .. } => "Fixed colour" = ColorSpec::Rgba {
            color: replaced_color(replacing),
        };
        ColorSpec::None => "None" = ColorSpec::None;
        ColorSpec::Colormapped => "Colormapped" = ColorSpec::Colormapped;
    }

    ScatterSize as ScatterSize (scatter_size_choices, scatter_size_label) replacing replacing {
        ScatterSize::Scalar { .. } => "Single size" = ScatterSize::Scalar {
            value: match replacing {
                Some(Value::ScatterSize(ScatterSize::Scalar { value })) => *value,
                _ => FALLBACK_MARKER_SIZE_PT,
            },
        };
        ScatterSize::Data { .. } => "From data" = ScatterSize::Data {
            data: match replacing {
                Some(Value::ScatterSize(ScatterSize::Data { data })) => *data,
                _ => FALLBACK_DATA,
            },
        };
    }

    ScatterColor as ScatterColor (scatter_color_choices, scatter_color_label) replacing replacing {
        ScatterColor::Spec { .. } => "Single colour" = ScatterColor::Spec {
            spec: match replacing {
                Some(Value::ScatterColor(ScatterColor::Spec { spec })) => *spec,
                _ => ColorSpec::Auto,
            },
        };
        ScatterColor::Data { .. } => "From data" = ScatterColor::Data {
            data: match replacing {
                Some(Value::ScatterColor(ScatterColor::Data { data })) => *data,
                _ => FALLBACK_DATA,
            },
        };
    }

    Grid as Grid (grid_choices, grid_label) replacing replacing {
        Grid::Rectilinear { .. } => "Rectilinear" = {
            let (x, y) = replaced_grid(replacing);
            Grid::Rectilinear { x, y }
        };
        Grid::Curvilinear { .. } => "Curvilinear" = {
            let (x, y) = replaced_grid(replacing);
            Grid::Curvilinear { x, y }
        };
    }

    Levels as Levels (levels_choices, levels_label) replacing replacing {
        Levels::Auto { .. } => "Automatic" = Levels::Auto {
            count: match replacing {
                Some(Value::Levels(Levels::Auto { count })) => *count,
                _ => FALLBACK_LEVEL_COUNT,
            },
        };
        Levels::Explicit { .. } => "Explicit" = Levels::Explicit {
            values: match replacing {
                Some(Value::Levels(Levels::Explicit { values })) => values.clone(),
                _ => vec![FALLBACK_LEVEL],
            },
        };
    }

    ContourPlacement as ContourPlacement (contour_placement_choices, contour_placement_label) replacing replacing {
        ContourPlacement::Plane { .. } => "In one plane" = ContourPlacement::Plane {
            z: match replacing {
                Some(Value::ContourPlacement(ContourPlacement::Plane { z })) => *z,
                _ => None,
            },
        };
        ContourPlacement::AtLevel => "At each level" = ContourPlacement::AtLevel;
    }

    QuiverScale as QuiverScale (quiver_scale_choices, quiver_scale_label) replacing replacing {
        QuiverScale::Auto => "Automatic" = QuiverScale::Auto;
        QuiverScale::Factor { .. } => "Factor" = QuiverScale::Factor {
            value: match replacing {
                Some(Value::QuiverScale(QuiverScale::Factor { value })) => *value,
                _ => FALLBACK_QUIVER_FACTOR,
            },
        };
        QuiverScale::Off => "Data units" = QuiverScale::Off;
    }

    // A plane keeps its offset when it is the plane the image already lies in, and starts
    // without one otherwise, because an offset along the third axis of one plane says
    // nothing about the third axis of another.
    ImagePlane as ImagePlane (image_plane_choices, image_plane_label) replacing replacing {
        ImagePlane::Xy { .. } => "Plane xy" = ImagePlane::Xy {
            z: match replacing {
                Some(Value::ImagePlane(ImagePlane::Xy { z })) => *z,
                _ => None,
            },
        };
        ImagePlane::Xz { .. } => "Plane xz" = ImagePlane::Xz {
            y: match replacing {
                Some(Value::ImagePlane(ImagePlane::Xz { y })) => *y,
                _ => None,
            },
        };
        ImagePlane::Yz { .. } => "Plane yz" = ImagePlane::Yz {
            x: match replacing {
                Some(Value::ImagePlane(ImagePlane::Yz { x })) => *x,
                _ => None,
            },
        };
    }

    OutOfRange as OutOfRange (out_of_range_choices, out_of_range_label) replacing replacing {
        OutOfRange::Strict => "Strict" = OutOfRange::Strict;
        OutOfRange::Transparent => "Transparent" = OutOfRange::Transparent;
        OutOfRange::Clamp => "Clamp" = OutOfRange::Clamp;
        OutOfRange::Rgba { .. } => "Fixed colour" = OutOfRange::Rgba {
            color: replaced_color(replacing),
        };
    }
}

plain_choices! {
    Scale as Scale (scale_choices, scale_label) {
        Scale::Linear => "Linear";
        Scale::Log => "Logarithmic";
    }

    ColormapName as ColormapName (colormap_name_choices, colormap_name_label) {
        ColormapName::Viridis => "Viridis";
        ColormapName::Cividis => "Cividis";
        ColormapName::Magma => "Magma";
        ColormapName::Inferno => "Inferno";
        ColormapName::Plasma => "Plasma";
        ColormapName::Coolwarm => "Coolwarm";
        ColormapName::Gray => "Gray";
    }

    LegendLocation as LegendLocation (legend_location_choices, legend_location_label) {
        LegendLocation::NorthEast => "North east";
        LegendLocation::NorthWest => "North west";
        LegendLocation::SouthEast => "South east";
        LegendLocation::SouthWest => "South west";
        LegendLocation::North => "North";
        LegendLocation::South => "South";
        LegendLocation::East => "East";
        LegendLocation::West => "West";
        LegendLocation::Best => "Best";
    }

    MarkerShape as MarkerShape (marker_shape_choices, marker_shape_label) {
        MarkerShape::None => "None";
        MarkerShape::Circle => "Circle";
        MarkerShape::Square => "Square";
        MarkerShape::Diamond => "Diamond";
        MarkerShape::TriangleUp => "Triangle up";
        MarkerShape::TriangleDown => "Triangle down";
        MarkerShape::Plus => "Plus";
        MarkerShape::Cross => "Cross";
        MarkerShape::Point => "Point";
    }

    DashStyle as DashStyle (dash_style_choices, dash_style_label) {
        DashStyle::Solid => "Solid";
        DashStyle::Dashed => "Dashed";
        DashStyle::Dotted => "Dotted";
        DashStyle::DashDot => "Dash-dot";
        DashStyle::None => "None";
    }

    Interpreter as Interpreter (interpreter_choices, interpreter_label) {
        Interpreter::Latex => "LaTeX";
        Interpreter::None => "Plain";
    }

    FontSetId as FontSetId (font_set_id_choices, font_set_id_label) {
        FontSetId::StixTwo => "STIX Two";
    }
}

/// The colour that a fixed colour takes from the value it replaces: the colour of a
/// fixed colour, of a scatter colour that holds one, of an out-of-range policy that
/// paints one, or black.
fn replaced_color(replacing: Option<&Value>) -> Color {
    match replacing {
        Some(Value::ColorSpec(ColorSpec::Rgba { color })) => *color,
        Some(Value::ScatterColor(ScatterColor::Spec {
            spec: ColorSpec::Rgba { color },
        })) => *color,
        Some(Value::OutOfRange(OutOfRange::Rgba { color })) => *color,
        _ => Color::BLACK,
    }
}

/// The coordinate arrays that a grid takes from the grid it replaces, which both kinds
/// of grid have, or the fallback array for both.
fn replaced_grid(replacing: Option<&Value>) -> (DataId, DataId) {
    match replacing {
        Some(Value::Grid(Grid::Rectilinear { x, y } | Grid::Curvilinear { x, y })) => (*x, *y),
        _ => (FALLBACK_DATA, FALLBACK_DATA),
    }
}
