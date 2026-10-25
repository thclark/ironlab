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

use crate::artist::Coords;
use crate::artist::{
    Artist, BarBase, BarLayout, BarSlot, CellEdges, ContourPlacement, Grid, ImagePlane, Levels,
    Orientation, QuiverScale, Series, StairsData, StepAt,
};
use crate::axes::{
    Aspect, AspectAdjust, Axes, ColormapName, InsideColorbarSide, Limits, Projection, Scale,
    TickLabels, Ticks, TitleAlign, View3d,
};
use crate::color_scale::{
    CategoricalColorScale, ColorCategory, ColorScaleKind, ColorScaleLevels, ColorScaleNorm,
    Colormap, ContinuousColorScale, DiscreteColorScale, IndexedColorScale, OutOfRange,
};
use crate::edit::path::PropertyPath;
use crate::edit::registry::properties;
use crate::edit::value::{Value, ValueType};
use crate::figure::{Figure, FontSetId};
use crate::ids::NodeId;
use crate::legend::{
    InsideLegend, Legend, LegendEntryColumns, LegendEntryOrder, LegendLocation, PanelLegend,
};
use crate::nodes::NodeKind;
use crate::panel::{
    ColorbarOrientation, ColorbarTitlePlacement, PanelColorbar, PanelContent, PanelLocation,
};
use crate::style::{Color, ColorSpec, DashStyle, MarkerShape};
use crate::text::{Interpreter, Text};

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
const FALLBACK_DATA: NodeId = NodeId(0);

/// The step that regularly stepped ticks start from when the value they replace holds
/// none.
const FALLBACK_TICK_STEP: f64 = 1.0;

/// The number of levels that automatic levels start from, which is the count of
/// [`Levels::default`].
const FALLBACK_LEVEL_COUNT: u32 = 10;

/// The multiplier that a quiver scale factor starts from, which leaves the arrows the
/// length that the automatic scale gives them.
const FALLBACK_QUIVER_FACTOR: f64 = 1.0;

/// The width that a fixed bar slot starts from when the value it replaces holds none:
/// one data unit, the pitch of bars at consecutive whole numbers.
const FALLBACK_BAR_WIDTH: f64 = 1.0;

/// The number of columns that a fixed number of legend columns starts from, which is the
/// number of columns of an inside legend by default.
const FALLBACK_LEGEND_COLUMNS: u32 = 1;

/// The colours that a listed colormap starts from when the value it replaces holds none:
/// black to white, the fewest colours a listed colormap may hold.
const FALLBACK_LISTED_COLORS: [Color; 2] = [Color::BLACK, Color::WHITE];

/// The panel that a legend listing its entries in a panel refers to when the value it
/// replaces names none. The figure need not have it, in which case the choice is
/// unavailable at the property and applying it is refused.
const FALLBACK_PANEL: NodeId = NodeId(0);

/// The colour scale that a panel's colourbar refers to when the content it replaces names
/// none. The figure need not have it, in which case the choice is unavailable at the
/// property and applying it is refused.
const FALLBACK_SCALE: NodeId = NodeId(0);

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
/// the node [`NodeId(0)`](NodeId), which the figure need not have. Passing `None` uses every
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
        ValueType::Ticks => ticks_choices(replacing),
        ValueType::TickLabels => tick_labels_choices(replacing),
        ValueType::Aspect => aspect_choices(replacing),
        ValueType::AspectAdjust => aspect_adjust_choices(),
        ValueType::TitleAlign => title_align_choices(),
        ValueType::ColorSpec => color_spec_choices(replacing),
        ValueType::Grid => grid_choices(replacing),
        ValueType::Levels => levels_choices(replacing),
        ValueType::ContourPlacement => contour_placement_choices(replacing),
        ValueType::QuiverScale => quiver_scale_choices(replacing),
        ValueType::ImagePlane => image_plane_choices(replacing),
        ValueType::OutOfRange => out_of_range_choices(replacing),
        ValueType::ColorScaleKind => color_scale_kind_choices(replacing),
        ValueType::Colormap => colormap_choices(replacing),
        ValueType::ColorScaleLevels => color_scale_levels_choices(replacing),
        ValueType::ColorScaleNorm => color_scale_norm_choices(),
        ValueType::Legend => legend_choices(replacing),
        ValueType::PanelContent => panel_content_choices(replacing),
        ValueType::LegendEntryColumns => legend_entry_columns_choices(replacing),
        ValueType::Scale => scale_choices(),
        ValueType::ColormapName => colormap_name_choices(),
        ValueType::LegendLocation => legend_location_choices(),
        ValueType::PanelLocation => panel_location_choices(),
        ValueType::ColorbarOrientation => colorbar_orientation_choices(),
        ValueType::InsideColorbarSide => inside_colorbar_side_choices(),
        ValueType::ColorbarTitlePlacement => colorbar_title_placement_choices(),
        ValueType::LegendEntryOrder => legend_entry_order_choices(),
        ValueType::MarkerShape => marker_shape_choices(),
        ValueType::DashStyle => dash_style_choices(),
        ValueType::Coords => coords_choices(),
        ValueType::Interpreter => interpreter_choices(),
        ValueType::FontSetId => font_set_id_choices(),
        ValueType::Orientation => orientation_choices(),
        ValueType::BarBase => bar_base_choices(replacing),
        ValueType::BarSlot => bar_slot_choices(replacing),
        ValueType::BarLayout => bar_layout_choices(),
        ValueType::StairsData => stairs_data_choices(replacing),
        ValueType::StepAt => step_at_choices(),
        ValueType::CellEdges => cell_edges_choices(replacing),
        ValueType::Bool
        | ValueType::UInt32
        | ValueType::Double
        | ValueType::Float
        | ValueType::String
        | ValueType::NodeId
        | ValueType::Doubles
        | ValueType::TrackSizes
        | ValueType::Strings
        | ValueType::Texts
        | ValueType::Text
        | ValueType::FigureSize
        | ValueType::Color
        | ValueType::TileLayout
        | ValueType::Links
        | ValueType::Parameters
        | ValueType::Cell
        | ValueType::View3d
        | ValueType::Axis
        | ValueType::InsideLegend
        | ValueType::LegendEntryLayout
        | ValueType::PanelLegend
        | ValueType::PanelColorbar
        | ValueType::InsideColorbar
        | ValueType::LineStyle
        | ValueType::MarkerStyle
        | ValueType::ImagePlacement
        | ValueType::PixelRange
        | ValueType::ColorScaleOutOfRange
        | ValueType::ContinuousColorScale
        | ValueType::DiscreteColorScale
        | ValueType::IndexedColorScale
        | ValueType::CategoricalColorScale
        | ValueType::BarGroup
        | ValueType::CellGap => Vec::new(),
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
/// Three choices are unavailable somewhere. A colormapped colour is looked up through a
/// colour scale only where the artist names a colour scale of the figure and the IR gives
/// the colour a value to be looked up by: the level of each isoline of a contour
/// (`line.color`), the height or colour data of each face of a surface (`face` and
/// `edge`), and the colour data of a series (`line.color`, `marker.face` and
/// `marker.edge`). An artist that names no scale has nothing to look the colour up in, a
/// series without colour data has no value to look up, and neither has the colour of a
/// quiver, so the scene compiler paints such a colour in the middle colour of viridis
/// instead, or, on a marker, in the colour of its series. A marker face or edge of a
/// series coloured per segment has no value at its point, so a colormapped marker colour
/// is unavailable there too. A clamp, which paints a value in the nearest end colour of
/// its scale, is unavailable at the `non_finite` category of a colour scale, because a
/// non-finite value has no nearest end; it stays available at the other categories. A
/// legend that lists the entries of its axes in a panel is unavailable when the figure has
/// no panel of the identifier it names, because the figure would then list the entries
/// nowhere.
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
const RULES: [Rule; 5] = [
    colormapped_without_a_value_to_index_by,
    clamp_without_a_nearest_end_of_the_colormap,
    legend_panel_that_the_figure_lacks,
    colorbar_scale_that_the_figure_lacks,
    tick_text_without_matching_values,
];

/// Completes the reason a colormapped colour is unavailable at a property, which is why
/// the property cannot look a colour up followed by where a colour scale can be used
/// instead.
macro_rules! colormapped_unavailable {
    ($why:literal) => {
        concat!(
            $why,
            " Colouring through a colour scale is available for the faces and edges of a \
             surface and for the isolines of a contour, which take the colour of their \
             level, for the line and markers of a series with colour data, and for the \
             outlines of cells, which take the colour of their cell, when the artist names \
             a colour scale."
        )
    };
}

/// Why a colormapped colour is unavailable on an artist that names no colour scale.
const COLORMAPPED_WITHOUT_A_SCALE: &str = colormapped_unavailable!(
    "This artist names no colour scale of the figure, so it has nothing to look the colour \
     up in. A colour scale is created and named by the program that builds the figure."
);

/// Why a colormapped colour is unavailable on a series without colour data.
const COLORMAPPED_WITHOUT_COLOUR_DATA: &str = colormapped_unavailable!(
    "This series has no colour data, so it provides no value to look the colour up by: a \
     colormapped line is drawn in the middle colour of viridis, and a colormapped marker in \
     the colour of the series, as an automatic one. Colour data is given to a series by the \
     program that builds the figure."
);

/// Why a colormapped colour is unavailable on the markers of a series coloured per
/// segment.
const COLORMAPPED_MARKER_PER_SEGMENT: &str = colormapped_unavailable!(
    "The colour data of this series holds one value per segment of its line, so no value \
     belongs to a point and a marker has none to look its colour up by. A series whose \
     colour data holds one value per point can colour its markers."
);

/// Why a colormapped colour is unavailable on a quiver.
const COLORMAPPED_ON_A_QUIVER: &str = colormapped_unavailable!(
    "A quiver is drawn in one colour and provides no value to look that colour up by, so \
     colormapped arrows are drawn in the middle colour of viridis."
);

/// Why a colormapped colour is unavailable on bars and stairs.
const COLORMAPPED_ON_BARS_OR_STAIRS: &str = colormapped_unavailable!(
    "Bars and stairs take solid colours only and have no colour scale, so a colormapped \
     colour has nothing to look the colour up in and makes the figure invalid. Values that \
     must be coloured through a scale are drawn as cells instead."
);

/// Why a colormapped colour is unavailable anywhere else.
const COLORMAPPED_ELSEWHERE: &str = colormapped_unavailable!(
    "This property provides no value to look a colour up by, so a colormapped colour \
     here is drawn in the middle colour of viridis."
);

/// Marks a colormapped colour unavailable wherever the IR holds no value to look the
/// colour up by, or the artist names no colour scale of the figure to look it up in, and
/// leaves every other choice and every other property alone.
fn colormapped_without_a_value_to_index_by(
    figure: &Figure,
    kind: NodeKind,
    node: NodeId,
    path: &PropertyPath,
    value: &Value,
) -> Option<&'static str> {
    if !matches!(value, Value::ColorSpec(ColorSpec::Colormapped)) {
        return None;
    }
    let names_a_scale = || {
        figure
            .artist(node)
            .and_then(|(_, artist)| artist.color_scale())
            .is_some_and(|scale| figure.color_scale(scale).is_some())
    };
    if kind == NodeKind::Series {
        let Some((_, Artist::Series(series))) = figure.artist(node) else {
            return None;
        };
        return series_colormapped(figure, series, path, names_a_scale());
    }
    if indexes_the_colormap(kind, path) {
        return (!names_a_scale()).then_some(COLORMAPPED_WITHOUT_A_SCALE);
    }
    // The match is exhaustive, so a kind added to the IR does not compile until it is
    // said why a colour of that kind cannot be colormapped.
    Some(match kind {
        NodeKind::Quiver => COLORMAPPED_ON_A_QUIVER,
        NodeKind::Bar | NodeKind::Stairs => COLORMAPPED_ON_BARS_OR_STAIRS,
        // Every colour of a contour and of a surface indexes the colormap, every colour
        // of a series is decided above, and a figure, a data array, a colour scale, an
        // axes, a panel and an image hold no colour specification at all (the fixed
        // colour of an out-of-range policy is a colour, not a specification), so these
        // kinds reach this reason only if they gain a colour that indexes nothing.
        NodeKind::Series
        | NodeKind::Contour
        | NodeKind::Surface
        | NodeKind::Figure
        | NodeKind::DataArray
        | NodeKind::ColorScale
        | NodeKind::Axes
        | NodeKind::Image
        | NodeKind::ScalarImage
        | NodeKind::Cells
        | NodeKind::Panel => COLORMAPPED_ELSEWHERE,
    })
}

/// Returns why a colormapped colour is unavailable at a colour property of a series, or
/// `None` when it is available: the series must have colour data and name a colour scale
/// of the figure, and a marker colour needs one value per point.
fn series_colormapped(
    figure: &Figure,
    series: &Series,
    path: &PropertyPath,
    names_a_scale: bool,
) -> Option<&'static str> {
    let Some(colors) = series.color_data else {
        return Some(COLORMAPPED_WITHOUT_COLOUR_DATA);
    };
    if !names_a_scale {
        return Some(COLORMAPPED_WITHOUT_A_SCALE);
    }
    let on_a_marker = path
        .segments()
        .first()
        .is_some_and(|first| first == "marker");
    let length = |id| figure.data_array(id).map(|data| data.array.len());
    let per_segment = matches!(
        (length(colors), length(series.x)),
        (Some(values), Some(points)) if values + 1 == points
    );
    (on_a_marker && per_segment).then_some(COLORMAPPED_MARKER_PER_SEGMENT)
}

/// Why a clamp is unavailable at the non-finite category of a colour scale.
const CLAMP_AT_NON_FINITE: &str = "A non-finite value has no nearest end of the scale to \
     be clamped to, so clamping is available only below and above the range, and outside \
     the entries.";

/// Marks a clamp unavailable at the policy of the `non_finite` category of a colour
/// scale, where the values have no nearest end of the scale to be clamped to, and leaves
/// every other choice and every other property alone.
fn clamp_without_a_nearest_end_of_the_colormap(
    _figure: &Figure,
    kind: NodeKind,
    _node: NodeId,
    path: &PropertyPath,
    value: &Value,
) -> Option<&'static str> {
    let clamp = matches!(value, Value::OutOfRange(OutOfRange::Clamp));
    let at_non_finite = kind == NodeKind::ColorScale && path.segments() == ["non_finite", "policy"];
    (clamp && at_non_finite).then_some(CLAMP_AT_NON_FINITE)
}

/// Why listing the entries of an axes in a panel is unavailable when the figure has no
/// panel of the identifier the choice names.
const NO_SUCH_LEGEND_PANEL: &str = "The figure has no panel of this identifier in which to \
     list the entries. A panel holding a legend is added by the program that builds the \
     figure, and the entries of an axes can then be listed in it.";

/// Marks a legend that lists its entries in a panel unavailable when the figure has no
/// panel of the identifier it names, and leaves every other choice alone.
fn legend_panel_that_the_figure_lacks(
    figure: &Figure,
    _kind: NodeKind,
    _node: NodeId,
    _path: &PropertyPath,
    value: &Value,
) -> Option<&'static str> {
    match value {
        Value::Legend(Legend::Panel(panel)) if figure.panel(*panel).is_none() => {
            Some(NO_SUCH_LEGEND_PANEL)
        }
        _ => None,
    }
}

/// Why a colourbar is unavailable as the content of a panel when it names no colour scale.
const NO_SUCH_COLOR_SCALE: &str = "The figure has no colour scale of this identifier for the \
     colourbar to show. A colour scale is created by the program that builds the figure, and \
     a panel can then show it as a colourbar.";

/// Marks a panel's colourbar unavailable when the figure has no colour scale of the
/// identifier it names, and leaves every other choice alone.
fn colorbar_scale_that_the_figure_lacks(
    figure: &Figure,
    _kind: NodeKind,
    _node: NodeId,
    _path: &PropertyPath,
    value: &Value,
) -> Option<&'static str> {
    match value {
        Value::PanelContent(PanelContent::Colorbar(colorbar))
            if figure.color_scale(colorbar.scale).is_none() =>
        {
            Some(NO_SUCH_COLOR_SCALE)
        }
        _ => None,
    }
}

/// Why explicit tick labels are unavailable on an axis whose ticks are not explicit values.
const TICK_TEXT_WITHOUT_VALUES: &str = "Explicit tick labels name the ticks one by one, so \
     they are available only where the ticks of the axis are explicit values, with one label \
     for each value. The ticks and their labels are set by the program that builds the figure.";

/// Marks explicit tick labels unavailable on an axis whose ticks are not explicit values of
/// as many ticks as the labels hold, which validation would refuse, and leaves every other
/// choice alone.
fn tick_text_without_matching_values(
    figure: &Figure,
    kind: NodeKind,
    node: NodeId,
    path: &PropertyPath,
    value: &Value,
) -> Option<&'static str> {
    let Value::TickLabels(TickLabels::Text(texts)) = value else {
        return None;
    };
    if kind != NodeKind::Axes {
        return None;
    }
    let [axis, field] = path.segments() else {
        return None;
    };
    if field != "tick_labels" {
        return None;
    }
    let axes: &Axes = figure.axes(node)?;
    let ticks = match axis.as_str() {
        "x" => &axes.x.ticks,
        "y" => &axes.y.ticks,
        "z" => &axes.z.ticks,
        _ => return None,
    };
    match ticks {
        Ticks::Values(values) if values.len() == texts.len() => None,
        _ => Some(TICK_TEXT_WITHOUT_VALUES),
    }
}

/// Returns whether the IR gives the colour at this property of this kind of node a value
/// to look up through a colour scale.
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
        // The outline of each cell takes the colour of its cell.
        NodeKind::Cells => segments == ["edge", "color"],
        // An image holds no colour specification: a true-colour image carries its own
        // colours, and a scalar image maps its numbers through its scale rather than
        // through a colour property. The colours of a series index the colormap when it
        // has colour data, which `series_colormapped` decides.
        NodeKind::Figure
        | NodeKind::DataArray
        | NodeKind::ColorScale
        | NodeKind::Axes
        | NodeKind::Series
        | NodeKind::Quiver
        | NodeKind::Image
        | NodeKind::ScalarImage
        | NodeKind::Bar
        | NodeKind::Stairs
        | NodeKind::Panel => false,
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
        Value::Ticks(v) => ticks_label(v),
        Value::TickLabels(v) => tick_labels_label(v),
        Value::Aspect(v) => aspect_label(v),
        Value::AspectAdjust(v) => aspect_adjust_label(v),
        Value::TitleAlign(v) => title_align_label(v),
        Value::ColorSpec(v) => color_spec_label(v),
        Value::Grid(v) => grid_label(v),
        Value::Levels(v) => levels_label(v),
        Value::ContourPlacement(v) => contour_placement_label(v),
        Value::QuiverScale(v) => quiver_scale_label(v),
        Value::ImagePlane(v) => image_plane_label(v),
        Value::OutOfRange(v) => out_of_range_label(v),
        Value::ColorScaleKind(v) => color_scale_kind_label(v),
        Value::Colormap(v) => colormap_label(v),
        Value::ColorScaleLevels(v) => color_scale_levels_label(v),
        Value::ColorScaleNorm(v) => color_scale_norm_label(v),
        Value::Legend(v) => legend_label(v),
        Value::PanelContent(v) => panel_content_label(v),
        Value::LegendEntryColumns(v) => legend_entry_columns_label(v),
        Value::Scale(v) => scale_label(v),
        Value::ColormapName(v) => colormap_name_label(v),
        Value::LegendLocation(v) => legend_location_label(v),
        Value::PanelLocation(v) => panel_location_label(v),
        Value::ColorbarOrientation(v) => colorbar_orientation_label(v),
        Value::InsideColorbarSide(v) => inside_colorbar_side_label(v),
        Value::ColorbarTitlePlacement(v) => colorbar_title_placement_label(v),
        Value::LegendEntryOrder(v) => legend_entry_order_label(v),
        Value::MarkerShape(v) => marker_shape_label(v),
        Value::DashStyle(v) => dash_style_label(v),
        Value::Coords(v) => coords_label(v),
        Value::Interpreter(v) => interpreter_label(v),
        Value::FontSetId(v) => font_set_id_label(v),
        Value::Orientation(v) => orientation_label(v),
        Value::BarBase(v) => bar_base_label(v),
        Value::BarSlot(v) => bar_slot_label(v),
        Value::BarLayout(v) => bar_layout_label(v),
        Value::StairsData(v) => stairs_data_label(v),
        Value::StepAt(v) => step_at_label(v),
        Value::CellEdges(v) => cell_edges_label(v),
        Value::Unset
        | Value::Bool(_)
        | Value::UInt32(_)
        | Value::Double(_)
        | Value::Float(_)
        | Value::String(_)
        | Value::NodeId(_)
        | Value::Doubles(_)
        | Value::TrackSizes(_)
        | Value::Strings(_)
        | Value::Texts(_)
        | Value::Text(_)
        | Value::FigureSize(_)
        | Value::Color(_)
        | Value::TileLayout(_)
        | Value::Links(_)
        | Value::Parameters(_)
        | Value::Cell(_)
        | Value::View3d(_)
        | Value::Axis(_)
        | Value::InsideLegend(_)
        | Value::LegendEntryLayout(_)
        | Value::PanelLegend(_)
        | Value::PanelColorbar(_)
        | Value::InsideColorbar(_)
        | Value::LineStyle(_)
        | Value::MarkerStyle(_)
        | Value::ImagePlacement(_)
        | Value::PixelRange(_)
        | Value::ColorScaleOutOfRange(_)
        | Value::ContinuousColorScale(_)
        | Value::DiscreteColorScale(_)
        | Value::IndexedColorScale(_)
        | Value::CategoricalColorScale(_)
        | Value::BarGroup(_)
        | Value::CellGap(_) => return None,
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

    Ticks as Ticks (ticks_choices, ticks_label) replacing replacing {
        Ticks::Auto => "Automatic" = Ticks::Auto;
        Ticks::Step { .. } => "Regular step" = match replacing {
            Some(Value::Ticks(Ticks::Step { step, origin })) => Ticks::Step {
                step: *step,
                origin: *origin,
            },
            _ => Ticks::Step {
                step: FALLBACK_TICK_STEP,
                origin: 0.0,
            },
        };
        Ticks::Values(_) => "Explicit values" = Ticks::Values(match replacing {
            Some(Value::Ticks(Ticks::Values(values))) => values.clone(),
            _ => Vec::new(),
        });
    }

    Aspect as Aspect (aspect_choices, aspect_label) replacing replacing {
        Aspect::Auto => "Automatic" = Aspect::Auto;
        Aspect::Equal => "Equal" = Aspect::Equal;
        Aspect::Ratio { .. } => "Ratio" = match replacing {
            Some(Value::Aspect(Aspect::Ratio { y, z })) => Aspect::Ratio { y: *y, z: *z },
            _ => Aspect::Ratio { y: 1.0, z: 1.0 },
        };
    }

    TickLabels as TickLabels (tick_labels_choices, tick_labels_label) replacing replacing {
        TickLabels::Auto => "Automatic" = TickLabels::Auto;
        TickLabels::Hidden => "Hidden" = TickLabels::Hidden;
        TickLabels::Text(_) => "Explicit text" = TickLabels::Text(match replacing {
            Some(Value::TickLabels(TickLabels::Text(texts))) => texts.clone(),
            _ => Vec::<Text>::new(),
        });
    }

    ColorSpec as ColorSpec (color_spec_choices, color_spec_label) replacing replacing {
        ColorSpec::Auto => "Automatic" = ColorSpec::Auto;
        ColorSpec::Rgba { .. } => "Fixed colour" = ColorSpec::Rgba {
            color: replaced_color(replacing),
        };
        ColorSpec::None => "None" = ColorSpec::None;
        ColorSpec::Colormapped => "Colormapped" = ColorSpec::Colormapped;
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

    Legend as Legend (legend_choices, legend_label) replacing replacing {
        Legend::Inside(_) => "Inside the plot area" = Legend::Inside(match replacing {
            Some(Value::Legend(Legend::Inside(inside))) => inside.clone(),
            _ => InsideLegend::default(),
        });
        Legend::Panel(_) => "In a legend panel" = Legend::Panel(match replacing {
            Some(Value::Legend(Legend::Panel(panel))) => *panel,
            _ => FALLBACK_PANEL,
        });
    }

    PanelContent as PanelContent (panel_content_choices, panel_content_label) replacing replacing {
        PanelContent::Legend(_) => "Legend" = PanelContent::Legend(match replacing {
            Some(Value::PanelContent(PanelContent::Legend(legend))) => legend.clone(),
            _ => PanelLegend::default(),
        });
        PanelContent::Colorbar(_) => "Colourbar" = PanelContent::Colorbar(match replacing {
            Some(Value::PanelContent(PanelContent::Colorbar(colorbar))) => *colorbar,
            _ => PanelColorbar {
                scale: FALLBACK_SCALE,
                ..PanelColorbar::default()
            },
        });
    }

    // A kind keeps what the kind it replaces shares with it: a continuous and a discrete
    // scale keep the colormap, the norm, the below and above categories, the ticks and the
    // limits (of a continuous scale, or of automatic band edges), and an indexed and a
    // categorical scale keep their colours and the outside category.
    ColorScaleKind as ColorScaleKind (color_scale_kind_choices, color_scale_kind_label) replacing replacing {
        ColorScaleKind::Continuous(_) => "Continuous" = ColorScaleKind::Continuous(match replacing {
            Some(Value::ColorScaleKind(ColorScaleKind::Continuous(scale))) => scale.clone(),
            Some(Value::ColorScaleKind(ColorScaleKind::Discrete(scale))) => ContinuousColorScale {
                colormap: scale.colormap.clone(),
                limits: match &scale.levels {
                    ColorScaleLevels::Auto { limits, .. } => *limits,
                    ColorScaleLevels::Explicit { values } => match (values.first(), values.last()) {
                        (Some(&min), Some(&max)) if min < max => Limits::Manual { min, max },
                        _ => Limits::Auto,
                    },
                },
                norm: scale.norm,
                below: scale.below.clone(),
                above: scale.above.clone(),
                ticks: scale.ticks.clone(),
                tick_labels: scale.tick_labels.clone(),
            },
            _ => ContinuousColorScale::default(),
        });
        ColorScaleKind::Discrete(_) => "Discrete" = ColorScaleKind::Discrete(match replacing {
            Some(Value::ColorScaleKind(ColorScaleKind::Discrete(scale))) => scale.clone(),
            Some(Value::ColorScaleKind(ColorScaleKind::Continuous(scale))) => DiscreteColorScale {
                colormap: scale.colormap.clone(),
                levels: ColorScaleLevels::Auto {
                    count: FALLBACK_LEVEL_COUNT,
                    limits: scale.limits,
                },
                norm: scale.norm,
                below: scale.below.clone(),
                above: scale.above.clone(),
                ticks: scale.ticks.clone(),
                tick_labels: scale.tick_labels.clone(),
            },
            _ => DiscreteColorScale::default(),
        });
        ColorScaleKind::Indexed(_) => "Indexed" = ColorScaleKind::Indexed(match replacing {
            Some(Value::ColorScaleKind(ColorScaleKind::Indexed(scale))) => scale.clone(),
            Some(Value::ColorScaleKind(ColorScaleKind::Categorical(scale))) => IndexedColorScale {
                colors: scale.categories.iter().map(|category| category.color).collect(),
                outside: scale.outside.clone(),
                ..IndexedColorScale::default()
            },
            _ => IndexedColorScale::default(),
        });
        // A category needs a label, so the entries of an indexed scale become categories
        // labelled with their indices.
        ColorScaleKind::Categorical(_) => "Categorical" = ColorScaleKind::Categorical(match replacing {
            Some(Value::ColorScaleKind(ColorScaleKind::Categorical(scale))) => scale.clone(),
            Some(Value::ColorScaleKind(ColorScaleKind::Indexed(scale))) => CategoricalColorScale {
                categories: scale
                    .colors
                    .iter()
                    .enumerate()
                    .map(|(index, &color)| ColorCategory {
                        color,
                        label: Text::plain(index.to_string()),
                    })
                    .collect(),
                outside: scale.outside.clone(),
            },
            _ => CategoricalColorScale::default(),
        });
    }

    Colormap as Colormap (colormap_choices, colormap_label) replacing replacing {
        Colormap::Named { .. } => "Named" = match replacing {
            Some(Value::Colormap(named @ Colormap::Named { .. })) => named.clone(),
            _ => Colormap::default(),
        };
        Colormap::Listed { .. } => "Listed" = Colormap::Listed {
            colors: match replacing {
                Some(Value::Colormap(Colormap::Listed { colors })) => colors.clone(),
                _ => FALLBACK_LISTED_COLORS.to_vec(),
            },
        };
    }

    ColorScaleLevels as ColorScaleLevels (color_scale_levels_choices, color_scale_levels_label) replacing replacing {
        ColorScaleLevels::Auto { .. } => "Automatic" = match replacing {
            Some(Value::ColorScaleLevels(auto @ ColorScaleLevels::Auto { .. })) => auto.clone(),
            _ => ColorScaleLevels::default(),
        };
        ColorScaleLevels::Explicit { .. } => "Explicit" = ColorScaleLevels::Explicit {
            values: match replacing {
                Some(Value::ColorScaleLevels(ColorScaleLevels::Explicit { values })) => values.clone(),
                _ => vec![FALLBACK_LIMITS.0, FALLBACK_LIMITS.1],
            },
        };
    }

    BarBase as BarBase (bar_base_choices, bar_base_label) replacing replacing {
        BarBase::Constant(_) => "Constant" = BarBase::Constant(match replacing {
            Some(Value::BarBase(BarBase::Constant(base))) => *base,
            _ => 0.0,
        });
        BarBase::Data(_) => "Per bar" = BarBase::Data(match replacing {
            Some(Value::BarBase(BarBase::Data(data))) => *data,
            _ => FALLBACK_DATA,
        });
    }

    BarSlot as BarSlot (bar_slot_choices, bar_slot_label) replacing replacing {
        BarSlot::Pitch => "Pitch" = BarSlot::Pitch;
        BarSlot::Width(_) => "Fixed width" = BarSlot::Width(match replacing {
            Some(Value::BarSlot(BarSlot::Width(width))) => *width,
            _ => FALLBACK_BAR_WIDTH,
        });
        BarSlot::Data(_) => "Width per bar" = BarSlot::Data(match replacing {
            Some(Value::BarSlot(BarSlot::Data(data))) => *data,
            _ => FALLBACK_DATA,
        });
    }

    // Both forms keep the values; the edges and the samples are different arrays (one
    // more edge than values, one sample per value), so neither is taken from the other.
    StairsData as StairsData (stairs_data_choices, stairs_data_label) replacing replacing {
        StairsData::Edges { .. } => "Between edges" = match replacing {
            Some(Value::StairsData(edges @ StairsData::Edges { .. })) => *edges,
            Some(Value::StairsData(StairsData::Samples { values, .. })) => StairsData::Edges {
                edges: FALLBACK_DATA,
                values: *values,
            },
            _ => StairsData::default(),
        };
        StairsData::Samples { .. } => "At samples" = match replacing {
            Some(Value::StairsData(samples @ StairsData::Samples { .. })) => *samples,
            Some(Value::StairsData(StairsData::Edges { values, .. })) => StairsData::Samples {
                x: FALLBACK_DATA,
                values: *values,
                step: StepAt::default(),
            },
            _ => StairsData::Samples {
                x: FALLBACK_DATA,
                values: FALLBACK_DATA,
                step: StepAt::default(),
            },
        };
    }

    CellEdges as CellEdges (cell_edges_choices, cell_edges_label) replacing replacing {
        CellEdges::Unit => "Unit" = CellEdges::Unit;
        CellEdges::Edges(_) => "Explicit edges" = CellEdges::Edges(match replacing {
            Some(Value::CellEdges(CellEdges::Edges(edges))) => *edges,
            _ => FALLBACK_DATA,
        });
    }

    LegendEntryColumns as LegendEntryColumns (legend_entry_columns_choices, legend_entry_columns_label) replacing replacing {
        LegendEntryColumns::Auto => "Automatic" = LegendEntryColumns::Auto;
        LegendEntryColumns::Count(_) => "Fixed number" = LegendEntryColumns::Count(match replacing {
            Some(Value::LegendEntryColumns(LegendEntryColumns::Count(count))) => *count,
            _ => FALLBACK_LEGEND_COLUMNS,
        });
    }
}

plain_choices! {
    Orientation as Orientation (orientation_choices, orientation_label) {
        Orientation::Vertical => "Vertical";
        Orientation::Horizontal => "Horizontal";
    }

    BarLayout as BarLayout (bar_layout_choices, bar_layout_label) {
        BarLayout::Grouped => "Grouped";
        BarLayout::Stacked => "Stacked";
    }

    StepAt as StepAt (step_at_choices, step_at_label) {
        StepAt::Pre => "At the earlier sample";
        StepAt::Post => "At the later sample";
        StepAt::Mid => "Midway";
    }

    Scale as Scale (scale_choices, scale_label) {
        Scale::Linear => "Linear";
        Scale::Log => "Logarithmic";
    }

    ColorScaleNorm as ColorScaleNorm (color_scale_norm_choices, color_scale_norm_label) {
        ColorScaleNorm::Linear => "Linear";
        ColorScaleNorm::Log => "Logarithmic";
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
        LegendLocation::Center => "Centre";
        LegendLocation::Best => "Best";
    }

    PanelLocation as PanelLocation (panel_location_choices, panel_location_label) {
        PanelLocation::Center => "Centre";
        PanelLocation::North => "North";
        PanelLocation::South => "South";
        PanelLocation::East => "East";
        PanelLocation::West => "West";
        PanelLocation::NorthEast => "North east";
        PanelLocation::NorthWest => "North west";
        PanelLocation::SouthEast => "South east";
        PanelLocation::SouthWest => "South west";
    }

    ColorbarOrientation as ColorbarOrientation (colorbar_orientation_choices, colorbar_orientation_label) {
        ColorbarOrientation::Vertical => "Vertical";
        ColorbarOrientation::Horizontal => "Horizontal";
    }

    InsideColorbarSide as InsideColorbarSide (inside_colorbar_side_choices, inside_colorbar_side_label) {
        InsideColorbarSide::South => "South";
        InsideColorbarSide::North => "North";
        InsideColorbarSide::East => "East";
        InsideColorbarSide::West => "West";
    }

    ColorbarTitlePlacement as ColorbarTitlePlacement (colorbar_title_placement_choices, colorbar_title_placement_label) {
        ColorbarTitlePlacement::Along => "Along";
        ColorbarTitlePlacement::Above => "Above";
    }

    LegendEntryOrder as LegendEntryOrder (legend_entry_order_choices, legend_entry_order_label) {
        LegendEntryOrder::ColumnMajor => "Column by column";
        LegendEntryOrder::RowMajor => "Row by row";
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
        MarkerShape::VLine => "Vertical tick";
        MarkerShape::HLine => "Horizontal tick";
    }

    Coords as Coords (coords_choices, coords_label) {
        Coords::Data => "Data";
        Coords::AxesFraction => "Axes fraction";
    }

    DashStyle as DashStyle (dash_style_choices, dash_style_label) {
        DashStyle::Solid => "Solid";
        DashStyle::Dashed => "Dashed";
        DashStyle::Dotted => "Dotted";
        DashStyle::DashDot => "Dash-dot";
        DashStyle::DashDotDot => "Dash-dot-dot";
        DashStyle::LongDash => "Long dash";
        DashStyle::None => "None";
    }

    AspectAdjust as AspectAdjust (aspect_adjust_choices, aspect_adjust_label) {
        AspectAdjust::Box => "Shape the box";
        AspectAdjust::Limits => "Widen the limits";
    }

    TitleAlign as TitleAlign (title_align_choices, title_align_label) {
        TitleAlign::Center => "Centre";
        TitleAlign::Left => "Left";
        TitleAlign::Right => "Right";
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
/// fixed colour, of an out-of-range policy that paints one, or black.
fn replaced_color(replacing: Option<&Value>) -> Color {
    match replacing {
        Some(Value::ColorSpec(ColorSpec::Rgba { color })) => *color,
        Some(Value::OutOfRange(OutOfRange::Rgba { color })) => *color,
        _ => Color::BLACK,
    }
}

/// The coordinate arrays that a grid takes from the grid it replaces, which both kinds
/// of grid have, or the fallback array for both.
fn replaced_grid(replacing: Option<&Value>) -> (NodeId, NodeId) {
    match replacing {
        Some(Value::Grid(Grid::Rectilinear { x, y } | Grid::Curvilinear { x, y })) => (*x, *y),
        _ => (FALLBACK_DATA, FALLBACK_DATA),
    }
}
