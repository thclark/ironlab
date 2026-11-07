//! The fill of a series down to a base (#123, #124): the area between the line through the
//! points of a series and a constant value of y, which a radar chart fills to close its
//! outline and an area chart fills to show a quantity.
//!
//! The IR these tests assume mirrors the vocabulary of stairs (ADR 0028):
//!
//! ```text
//! pub struct Series {
//!     /* the existing fields */
//!     pub base: Option<f64>,   // the value of y down to which the area is filled
//!     pub fill: ColorSpec,     // none (the default) draws no fill; auto takes the series colour
//!     pub fill_opacity: f64,   // from 0 to 1, multiplying the alpha of the fill; default 1
//! }
//! ```
//!
//! - Each of the three is omitted from JSON when it holds its default (no base, no fill, an
//!   opacity of 1), so that the JSON of a figure written before the fields existed, and of
//!   every unfilled series since, is unchanged. In the wire schema they are new optional
//!   fields, `base` optional in the IR as the base of stairs is.
//! - Validation: a fill other than none without a base is `FillWithoutBase` (the kind
//!   stairs report, reused); a colormapped fill is `ColormapNotSupported` (the kind stairs
//!   report, reused), because a fill has no colour data to map even where the line does; a
//!   fill opacity outside `[0, 1]` or not finite is `InvalidOpacity`; and a fill in any
//!   three-dimensional projection (Cartesian, cylindrical or spherical) is the new error
//!   `SeriesFillInThreeDAxes`. All are reported against the series. A base that is not
//!   positive on a logarithmic y axis is the warning `NonPositiveOnLogAxis`, as it is for
//!   stairs, because the fill then drops to the low edge of the plot area.
//! - Edits: `base`, `fill` and `fill_opacity` are settable properties of a series.

mod common;

use common::FigureBuilder;
use common::edits::{applied, path, set, tx};
use common::{continuous, named};
use ironlab_ir::*;
use serde_json::{Value as JsonValue, json};

const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

// ---------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------

/// A figure of one axes of the given projection holding one series through the points
/// (0, 1), (1, 3) and (2, 2), built from the defaults by `edit`. Returns the figure and
/// the identifier of the series.
fn series_in(projection: Projection, edit: impl FnOnce(&mut Series)) -> (Figure, NodeId) {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    b.axes(axes).projection = projection;
    let x = b.vector(&[0.0, 1.0, 2.0]);
    let y = b.vector(&[1.0, 3.0, 2.0]);
    let id = b.node();
    let mut series = Series {
        id,
        x,
        y,
        ..Series::default()
    };
    edit(&mut series);
    b.push(axes, Artist::Series(series));
    (b.build(), id)
}

/// The series of [`series_in`] in two-dimensional Cartesian axes.
fn series_with(edit: impl FnOnce(&mut Series)) -> (Figure, NodeId) {
    series_in(Projection::TwoD, edit)
}

/// Gives a series an automatic fill down to a base of 0.
fn filled(s: &mut Series) {
    s.base = Some(0.0);
    s.fill = ColorSpec::Auto;
}

/// Returns the series of a figure, found without the API under test.
fn series_of(fig: &Figure, id: NodeId) -> &Series {
    fig.axes
        .iter()
        .flat_map(|axes| &axes.artists)
        .find_map(|artist| match artist {
            Artist::Series(s) if s.id == id => Some(s),
            _ => None,
        })
        .expect("the fixture series exists")
}

/// The JSON value of the first artist of the first axes of a figure.
fn artist_json(fig: &Figure) -> JsonValue {
    let value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    value["axes"][0]["artists"][0].clone()
}

/// Asserts that a report holds exactly one error, of the given kind and against the given
/// node, and no warning, so that a check cannot pass by raising some other issue instead.
#[track_caller]
fn assert_only_error(report: &ValidationReport, kind: IssueKind, node: NodeId, at: &str) {
    assert_eq!(report.errors.len(), 1, "{at}: {report:?}");
    assert_eq!(report.errors[0].kind, kind, "{at}: {report:?}");
    assert_eq!(report.errors[0].node, Some(node), "{at}: {report:?}");
    assert_eq!(report.warnings, vec![], "{at}");
}

#[track_caller]
fn assert_no_issues(report: &ValidationReport, at: &str) {
    assert_eq!(report.errors, vec![], "{at}");
    assert_eq!(report.warnings, vec![], "{at}");
}

/// The three-dimensional projections, each with its default camera and view.
fn three_d_projections() -> [(&'static str, Projection); 3] {
    [
        (
            "Cartesian",
            Projection::ThreeD {
                view3d: View3d::default(),
            },
        ),
        (
            "cylindrical",
            Projection::Cylindrical {
                view3d: View3d::default(),
                polar: PolarView::default(),
            },
        ),
        (
            "spherical",
            Projection::Spherical {
                view3d: View3d::default(),
                spherical: SphericalView::default(),
            },
        ),
    ]
}

// ---------------------------------------------------------------------------------
// Defaults and JSON
// ---------------------------------------------------------------------------------

// Why: a series made without a choice of fill is the line `plot` draws, which fills
// nothing; and its fill opacity, which only matters once a fill is chosen, must start
// opaque, so that choosing a fill shows it in full as choosing a fill of stairs does.
#[test]
fn a_default_series_has_no_base_no_fill_and_an_opaque_fill_opacity() {
    let s = Series::default();
    assert_eq!(s.base, None);
    assert_eq!(s.fill, ColorSpec::None);
    assert_eq!(s.fill_opacity, 1.0);
}

// Why: series are the most common artist in saved files, so adding the fill must not
// change how an unfilled series is written: its JSON must carry none of the three new
// properties, which keeps every existing file byte-identical on its next save and lets a
// reader that does not know them open files of unfilled series.
#[test]
fn the_json_of_an_unfilled_series_carries_none_of_the_fill_properties() {
    let (fig, _) = series_with(|_| {});
    let artist = artist_json(&fig);
    for absent in ["base", "fill", "fill_opacity"] {
        assert!(
            artist.get(absent).is_none(),
            "an unfilled series writes no `{absent}`: {artist}"
        );
    }
}

// Why: a hand-written or other-language file must be able to read and write a fill in the
// form the schema documents, which is the form stairs already use: the base a plain
// number, the fill a tagged colour specification and the opacity a plain number. Each is
// written only when it differs from its default, so a base alone writes no fill and an
// opaque fill writes no opacity.
#[test]
fn a_fill_is_written_in_the_documented_json_form_and_each_default_is_left_out() {
    let (fig, _) = series_with(|s| {
        s.base = Some(-0.5);
        s.fill = ColorSpec::Rgba { color: BLUE };
        s.fill_opacity = 0.25;
    });
    let artist = artist_json(&fig);
    assert_eq!(artist["base"], json!(-0.5));
    assert_eq!(
        artist["fill"],
        json!({ "type": "rgba", "color": "#0000ff" })
    );
    assert_eq!(artist["fill_opacity"], json!(0.25));

    let (fig, _) = series_with(|s| s.base = Some(2.0));
    let artist = artist_json(&fig);
    assert_eq!(artist["base"], json!(2.0));
    assert!(artist.get("fill").is_none(), "{artist}");
    assert!(artist.get("fill_opacity").is_none(), "{artist}");

    let (fig, _) = series_with(filled);
    let artist = artist_json(&fig);
    assert_eq!(artist["fill"], json!({ "type": "auto" }));
    assert!(artist.get("fill_opacity").is_none(), "{artist}");
}

// Why: a file written before the fill existed has none of its properties; reading it must
// give exactly the unfilled series it described, not an error and not a fill.
#[test]
fn json_without_the_fill_properties_reads_as_an_unfilled_series() {
    let (fig, id) = series_with(|_| {});
    let mut value: JsonValue = serde_json::from_str(&fig.to_json()).unwrap();
    let artist = value["axes"][0]["artists"][0]
        .as_object_mut()
        .expect("an artist is an object");
    artist.remove("base");
    artist.remove("fill");
    artist.remove("fill_opacity");
    let reread = Figure::from_json(&value.to_string()).expect("the older form reads");
    let s = series_of(&reread, id);
    assert_eq!(s.base, None);
    assert_eq!(s.fill, ColorSpec::None);
    assert_eq!(s.fill_opacity, 1.0);
    assert_eq!(reread, fig);
}

// Why: a saved filled series must reopen exactly as it was, whichever format it was saved
// in, including the awkward values: a negative base, a base of exactly 0 (which an
// optional encoder could confuse with an absent one), a fill opacity of 0 and every kind
// of fill colour. The second save must reproduce the first byte for byte.
#[test]
fn every_fill_survives_json_and_protobuf_round_trips() {
    let cases: [(Option<f64>, ColorSpec, f64); 5] = [
        (None, ColorSpec::None, 1.0),
        (Some(0.0), ColorSpec::Auto, 1.0),
        (Some(-2.5), ColorSpec::Rgba { color: BLUE }, 0.25),
        (Some(1e-300), ColorSpec::Auto, 0.0),
        (Some(7.0), ColorSpec::None, 0.5),
    ];
    for (base, fill, fill_opacity) in cases {
        let (fig, _) = series_with(|s| {
            s.base = base;
            s.fill = fill;
            s.fill_opacity = fill_opacity;
        });
        let at = format!("base {base:?}, fill {fill:?}, opacity {fill_opacity}");
        let text = fig.to_json();
        let reopened = Figure::from_json(&text).expect("own JSON parses");
        assert_eq!(reopened, fig, "JSON of {at}");
        assert_eq!(reopened.to_json(), text, "JSON of {at} is stable");
        let bytes = fig.to_protobuf();
        let decoded = Figure::from_protobuf(&bytes).expect("own protobuf decodes");
        assert_eq!(decoded, fig, "protobuf of {at}");
        assert_eq!(decoded.to_protobuf(), bytes, "protobuf of {at} is stable");
    }
}

// Why: the JSON schema is what hand-written files are checked against; a file that keeps
// a series unfilled leaves the three properties out, so the schema must describe them
// without requiring them, or every existing file would fail validation against the schema
// while IronLAB still reads it.
#[test]
fn the_schema_describes_the_fill_properties_of_a_series_without_requiring_them() {
    let schema = ironlab_ir::json_schema();
    let definition = schema
        .pointer("/$defs/Series")
        .expect("the schema defines Series");
    let required: Vec<&str> = definition
        .get("required")
        .and_then(JsonValue::as_array)
        .map(|items| items.iter().filter_map(JsonValue::as_str).collect())
        .unwrap_or_default();
    for property in ["base", "fill", "fill_opacity"] {
        assert!(
            definition
                .pointer(&format!("/properties/{property}"))
                .is_some(),
            "Series has no {property} property in the schema"
        );
        assert!(
            !required.contains(&property),
            "Series requires {property}: {required:?}"
        );
    }
}

// ---------------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------------

// Why: the ordinary uses of a fill (an area under a line, a radar outline in polar axes)
// must be accepted without a word, as must a base given without a fill, which draws
// nothing more but is harmless and is what a user has before choosing a colour.
#[test]
fn a_filled_series_in_cartesian_or_polar_axes_and_a_base_alone_raise_no_issue() {
    let (fig, _) = series_with(filled);
    assert_no_issues(&fig.validate(), "a filled series in Cartesian axes");
    let (fig, _) = series_in(
        Projection::Polar {
            polar: PolarView::default(),
            view: View2d::default(),
        },
        |s| {
            filled(s);
            s.fill_opacity = 0.25;
        },
    );
    assert_no_issues(&fig.validate(), "a filled series in polar axes");
    let (fig, _) = series_with(|s| s.base = Some(1.0));
    assert_no_issues(&fig.validate(), "a base without a fill");
}

// Why: a fill is the area between the line and its base, so a fill without a base has
// nothing to fill down to; the user must be told, against the series, with the same kind
// stairs report for the same mistake so that a client handles both alike.
#[test]
fn a_fill_without_a_base_is_an_error() {
    let (fig, id) = series_with(|s| s.fill = ColorSpec::Auto);
    assert_only_error(
        &fig.validate(),
        IssueKind::FillWithoutBase,
        id,
        "an automatic fill without a base",
    );
}

// Why: the colour data of a series colours its points and segments, not the area under
// them, and a fill has no value of its own to look up; so a colormapped fill cannot be
// drawn, even on a series whose line is colormapped. It is refused with the kind stairs
// report for a colormapped fill.
#[test]
fn a_colormapped_fill_is_an_error_even_on_a_series_with_colour_data() {
    let (fig, id) = series_with(|s| {
        s.base = Some(0.0);
        s.fill = ColorSpec::Colormapped;
    });
    assert_only_error(
        &fig.validate(),
        IssueKind::ColormapNotSupported,
        id,
        "a colormapped fill",
    );

    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    let x = b.vector(&[0.0, 1.0, 2.0]);
    let y = b.vector(&[1.0, 3.0, 2.0]);
    let colours = b.vector(&[0.0, 0.5, 1.0]);
    let scale = b.color_scale(continuous(named(ColormapName::Viridis), Limits::Auto));
    let id = b.node();
    b.push(
        axes,
        Artist::Series(Series {
            id,
            x,
            y,
            line: LineStyle {
                color: ColorSpec::Colormapped,
                ..LineStyle::default()
            },
            color_data: Some(colours),
            color_scale: Some(scale),
            base: Some(0.0),
            fill: ColorSpec::Colormapped,
            ..Series::default()
        }),
    );
    assert_only_error(
        &b.build().validate(),
        IssueKind::ColormapNotSupported,
        id,
        "a colormapped fill on a series with colour data",
    );
}

// Why: the fill opacity multiplies an alpha, so it must be a finite number from 0 to 1;
// anything else has no meaning as an opacity and is refused, as the opacities of stairs
// are, while both ends of the range are accepted.
#[test]
fn a_fill_opacity_outside_zero_to_one_is_an_error() {
    for bad in [-0.01, 1.01, f64::NAN, f64::INFINITY] {
        let (fig, id) = series_with(|s| {
            filled(s);
            s.fill_opacity = bad;
        });
        assert_only_error(
            &fig.validate(),
            IssueKind::InvalidOpacity,
            id,
            &format!("a fill opacity of {bad}"),
        );
    }
    for good in [0.0, 1.0] {
        let (fig, _) = series_with(|s| {
            filled(s);
            s.fill_opacity = good;
        });
        assert_no_issues(&fig.validate(), &format!("a fill opacity of {good}"));
    }
}

// Why: the area under a line in three dimensions has no single plane to lie in (the line
// may twist through z), so the compiler does not draw it; a fill set there would be
// silently ignored, so it is refused against the series in every three-dimensional
// projection, Cartesian, cylindrical and spherical alike. A base alone draws nothing and is
// not refused.
#[test]
fn a_fill_in_any_three_dimensional_projection_is_an_error() {
    for (name, projection) in three_d_projections() {
        let (fig, id) = series_in(projection, filled);
        assert_only_error(
            &fig.validate(),
            IssueKind::SeriesFillInThreeDAxes,
            id,
            &format!("a fill in {name} axes"),
        );
        let (fig, _) = series_in(projection, |s| s.base = Some(0.0));
        assert_no_issues(&fig.validate(), &format!("a base alone in {name} axes"));
    }
}

// Why: a logarithmic y axis has no place for a base of 0 or below, so the fill drops to the
// low edge of the plot area instead, as the outline of stairs does; the figure still draws,
// so the user is warned rather than refused, with the warning stairs give.
#[test]
fn a_base_that_is_not_positive_on_a_logarithmic_y_axis_is_warned_of() {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    b.axes(axes).y.scale = Scale::Log;
    let x = b.vector(&[0.0, 1.0, 2.0]);
    let y = b.vector(&[1.0, 30.0, 200.0]);
    let id = b.node();
    b.push(
        axes,
        Artist::Series(Series {
            id,
            x,
            y,
            base: Some(0.0),
            fill: ColorSpec::Auto,
            ..Series::default()
        }),
    );
    let report = b.build().validate();
    assert_eq!(report.errors, vec![]);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.kind == IssueKind::NonPositiveOnLogAxis && w.node == Some(id)),
        "{report:?}"
    );
}

// ---------------------------------------------------------------------------------
// Edits
// ---------------------------------------------------------------------------------

// Why: the property editor and clients in other languages fill a series by path; the base
// is optional (unset to remove it, as the base of stairs is), the fill a colour
// specification and the opacity a number, each documented for the user who has not read
// the source.
#[test]
fn the_registry_lists_the_fill_properties_of_a_series() {
    let listed = properties(NodeKind::Series);
    for (at, value_type, optional) in [
        ("base", ValueType::Double, true),
        ("fill", ValueType::ColorSpec, false),
        ("fill_opacity", ValueType::Double, false),
    ] {
        let property = listed
            .iter()
            .find(|p| p.path.to_string() == at)
            .unwrap_or_else(|| panic!("the registry of a series lacks {at}"));
        assert_eq!(property.value_type, value_type, "{at}");
        assert_eq!(property.optional, optional, "{at}");
        assert!(!property.docs.trim().is_empty(), "{at} is documented");
    }
}

// Why: filling a series and taking the fill away are each one transaction, which an editor
// must be able to undo exactly; the base is removed by unsetting it, which leaves an
// unfilled series that writes exactly the JSON it wrote before it was filled.
#[test]
fn a_fill_is_set_and_removed_by_path_and_undone() {
    let (fig, id) = series_with(|_| {});
    let (edited, inverse) = applied(
        &fig,
        &tx([
            set(id, "base", Value::Double(-1.0)),
            set(
                id,
                "fill",
                Value::ColorSpec(ColorSpec::Rgba { color: BLUE }),
            ),
            set(id, "fill_opacity", Value::Double(0.25)),
        ]),
    )
    .unwrap();
    let s = series_of(&edited, id);
    assert_eq!(s.base, Some(-1.0));
    assert_eq!(s.fill, ColorSpec::Rgba { color: BLUE });
    assert_eq!(s.fill_opacity, 0.25);
    assert_eq!(edited.get(id, &path("base")).unwrap(), Value::Double(-1.0));
    let (restored, _) = applied(&edited, &inverse).unwrap();
    assert_eq!(restored, fig, "the inverse restores the unfilled series");

    let (removed, _) = applied(
        &edited,
        &tx([
            set(id, "fill", Value::ColorSpec(ColorSpec::None)),
            set(id, "base", Value::Unset),
            set(id, "fill_opacity", Value::Double(1.0)),
        ]),
    )
    .unwrap();
    assert_eq!(removed.get(id, &path("base")).unwrap(), Value::Unset);
    assert_eq!(removed.to_json(), fig.to_json());
}

// Why: an edit that would leave the figure invalid is refused as a whole, so a fill set on
// a series without a base, or a fill opacity beyond 1, must leave the figure untouched;
// otherwise the property editor could save a figure that does not validate.
#[test]
fn an_edit_that_leaves_a_fill_without_a_base_or_an_invalid_opacity_is_refused() {
    let (fig, id) = series_with(|_| {});
    assert!(matches!(
        applied(
            &fig,
            &tx([set(id, "fill", Value::ColorSpec(ColorSpec::Auto))])
        ),
        Err(EditError::Invalid(_))
    ));
    let (fig, id) = series_with(filled);
    assert!(matches!(
        applied(&fig, &tx([set(id, "base", Value::Unset)])),
        Err(EditError::Invalid(_))
    ));
    assert!(matches!(
        applied(&fig, &tx([set(id, "fill_opacity", Value::Double(1.5))])),
        Err(EditError::Invalid(_))
    ));
}

// Why: the property editor offers the colour choices of a property and greys out those
// the IR would not act on; a colormapped fill is never drawn, so it must be offered as
// unavailable at `fill` even on a series whose line can be colormapped, with a reason.
#[test]
fn a_colormapped_fill_is_offered_as_unavailable() {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    let x = b.vector(&[0.0, 1.0, 2.0]);
    let y = b.vector(&[1.0, 3.0, 2.0]);
    let colours = b.vector(&[0.0, 0.5, 1.0]);
    let scale = b.color_scale(continuous(named(ColormapName::Viridis), Limits::Auto));
    let id = b.node();
    b.push(
        axes,
        Artist::Series(Series {
            id,
            x,
            y,
            color_data: Some(colours),
            color_scale: Some(scale),
            base: Some(0.0),
            fill: ColorSpec::Auto,
            ..Series::default()
        }),
    );
    let fig = b.build();
    let offered = property_choices(&fig, id, &path("fill"));
    let colormapped = offered
        .iter()
        .find(|choice| choice.value == Value::ColorSpec(ColorSpec::Colormapped))
        .expect("the fill offers a colormapped colour");
    assert!(
        colormapped.unavailable.is_some(),
        "a colormapped fill is unavailable: {offered:?}"
    );
    let line = property_choices(&fig, id, &path("line.color"));
    assert!(
        line.iter()
            .any(|c| c.value == Value::ColorSpec(ColorSpec::Colormapped)
                && c.unavailable.is_none()),
        "the line of the same series can be colormapped: {line:?}"
    );
}
