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

use crate::artist::{ContourPlacement, Grid, Levels, QuiverScale, ScatterColor, ScatterSize};
use crate::axes::{ColormapName, LegendLocation, Limits, Projection, Scale, View3d};
use crate::edit::value::{Value, ValueType};
use crate::figure::FontSetId;
use crate::ids::DataId;
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
}

impl Choice {
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
        | ValueType::MarkerStyle => Vec::new(),
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
        | Value::MarkerStyle(_) => return None,
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
            vec![$( Choice { label: $label, value: Value::$variant($default) }, )+]
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
            vec![$( Choice { label: $label, value: Value::$variant($case) }, )+]
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
/// fixed colour, of a scatter colour that holds one, or black.
fn replaced_color(replacing: Option<&Value>) -> Color {
    match replacing {
        Some(Value::ColorSpec(ColorSpec::Rgba { color })) => *color,
        Some(Value::ScatterColor(ScatterColor::Spec {
            spec: ColorSpec::Rgba { color },
        })) => *color,
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
