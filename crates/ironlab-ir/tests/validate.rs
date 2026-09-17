//! Structural validation of figures.
//!
//! Each error kind is triggered by the smallest change to an otherwise valid figure, so
//! that a test fails only if that specific check is missing or over-eager.

mod common;

use common::{FigureBuilder, kitchen_sink_figure, single_line_figure};
use ironlab_ir::*;

fn has_error(report: &ValidationReport, kind: IssueKind, node: Option<NodeId>) -> bool {
    report
        .errors
        .iter()
        .any(|issue| issue.kind == kind && issue.node == node)
}

fn has_warning(report: &ValidationReport, kind: IssueKind, node: Option<NodeId>) -> bool {
    report
        .warnings
        .iter()
        .any(|issue| issue.kind == kind && issue.node == node)
}

fn error_kinds(report: &ValidationReport) -> Vec<IssueKind> {
    report.errors.iter().map(|issue| issue.kind).collect()
}

fn line_mut(fig: &mut Figure) -> &mut Line {
    match &mut fig.axes[0].artists[0] {
        Artist::Line(line) => line,
        other => panic!("expected a line, found {other:?}"),
    }
}

/// A figure with one axes and one artist, built by [`grid_fixture`].
struct GridFixture {
    fig: Figure,
    artist: NodeId,
}

/// Data available to the artist of a [`GridFixture`]: a rectilinear grid (`gx` of 4
/// values along columns, `gy` of 3 values along rows), a matching 3 × 4 `field`,
/// curvilinear 3 × 4 coordinates `cx` and `cy`, three vectors `p`, `q` and `r` of 12
/// positive values, a vector `short` of 11 values and a vector `column` of 3 values.
struct GridData {
    gx: DataId,
    gy: DataId,
    field: DataId,
    cx: DataId,
    cy: DataId,
    p: DataId,
    q: DataId,
    r: DataId,
    short: DataId,
    column: DataId,
}

/// A deferred constructor of the artist placed in a [`GridFixture`].
type MakeArtist = Box<dyn FnOnce(NodeId, &GridData) -> Artist>;

/// Builds a figure with one 2D or 3D axes holding the single artist made by `make`.
fn grid_fixture(three_d: bool, make: impl FnOnce(NodeId, &GridData) -> Artist) -> GridFixture {
    let mut b = FigureBuilder::new();
    let axes = if three_d {
        b.axes3d(0, 0)
    } else {
        b.axes2d(0, 0)
    };
    let data = GridData {
        gx: b.vector(&[0.0, 1.0, 2.0, 3.0]),
        gy: b.vector(&[0.0, 1.0, 2.0]),
        field: b.matrix(3, 4, |j, i| (j + i) as f64 + 1.0),
        cx: b.matrix(3, 4, |j, i| i as f64 + 0.1 * j as f64),
        cy: b.matrix(3, 4, |j, i| j as f64 + 0.1 * i as f64),
        p: b.vector(&[1.0; 12]),
        q: b.vector(&[2.0; 12]),
        r: b.vector(&[3.0; 12]),
        short: b.vector(&[1.0; 11]),
        column: b.vector(&[1.0, 2.0, 3.0]),
    };
    let artist = b.node();
    let made = make(artist, &data);
    b.push(axes, made);
    GridFixture {
        fig: b.build(),
        artist,
    }
}

// Why: a check that reports errors on a correct figure is as harmful as a missing check;
// the figure that exercises every artist and enum variant must validate cleanly.
#[test]
fn figure_using_every_feature_correctly_has_no_issues() {
    let report = kitchen_sink_figure().validate();
    assert_eq!(report.errors, vec![], "unexpected errors");
    assert_eq!(report.warnings, vec![], "unexpected warnings");
    assert!(report.is_valid());
}

// Why: a default figure (no axes, no data) is where every user starts and must be valid.
#[test]
fn empty_default_figure_is_valid() {
    let report = Figure::new().validate();
    assert!(report.errors.is_empty() && report.warnings.is_empty());
}

// Why: an artist that refers to data not in the table cannot be drawn, and the error
// must point at the artist so a user can find it.
#[test]
fn reference_to_missing_data_is_an_error_on_the_artist() {
    let (mut fig, _, line) = single_line_figure();
    line_mut(&mut fig).y = DataId(999);
    let report = fig.validate();
    assert!(
        has_error(&report, IssueKind::UnknownData, Some(line)),
        "{report:?}"
    );
    assert!(!report.is_valid());
}

// Why: an optional data reference (here a 3D line's z) must be checked like a required one.
#[test]
fn optional_reference_to_missing_data_is_an_error() {
    let fx = grid_fixture(true, |id, d| {
        Artist::Line(Line {
            id,
            x: d.p,
            y: d.q,
            z: Some(DataId(999)),
            ..Line::default()
        })
    });
    let report = fx.fig.validate();
    assert!(
        has_error(&report, IssueKind::UnknownData, Some(fx.artist)),
        "{report:?}"
    );
}

// Why: an array whose shape disagrees with its value count would cause out-of-bounds
// indexing in every renderer, and it can only arrive through a hand-edited file.
#[test]
fn array_whose_shape_does_not_match_its_values_is_an_error() {
    let (mut fig, _, _) = single_line_figure();
    fig.data.insert(
        DataId(50),
        NdArray {
            shape: vec![2, 2],
            values: vec![1.0, 2.0, 3.0],
        },
    );
    let report = fig.validate();
    assert!(
        error_kinds(&report).contains(&IssueKind::InvalidArray),
        "{report:?}"
    );
}

// Why: a line needs one x and one y per point; unequal lengths have no meaningful drawing.
#[test]
fn line_with_unequal_x_and_y_lengths_is_a_shape_mismatch() {
    let fx = grid_fixture(false, |id, d| {
        Artist::Line(Line {
            id,
            x: d.p,
            y: d.short,
            ..Line::default()
        })
    });
    let report = fx.fig.validate();
    assert!(
        has_error(&report, IssueKind::ShapeMismatch, Some(fx.artist)),
        "{report:?}"
    );
}

// Why: point-set artists pair values by index, so a row vector and a matrix with the same
// number of elements are compatible (as in MATLAB), and must not be reported.
#[test]
fn line_compares_element_counts_not_shapes() {
    let fx = grid_fixture(false, |id, d| {
        Artist::Line(Line {
            id,
            x: d.p,
            y: d.field,
            ..Line::default()
        })
    });
    assert_eq!(fx.fig.validate().errors, vec![]);
}

// Why: per-point scatter sizes and colours must have one entry per point.
#[test]
fn scatter_size_or_colour_data_of_the_wrong_length_is_a_shape_mismatch() {
    let sized = grid_fixture(false, |id, d| {
        Artist::Scatter(Scatter {
            id,
            x: d.p,
            y: d.q,
            size: ScatterSize::Data { data: d.short },
            ..Scatter::default()
        })
    });
    let report = sized.fig.validate();
    assert!(
        has_error(&report, IssueKind::ShapeMismatch, Some(sized.artist)),
        "{report:?}"
    );

    let coloured = grid_fixture(false, |id, d| {
        Artist::Scatter(Scatter {
            id,
            x: d.p,
            y: d.q,
            color: ScatterColor::Data { data: d.short },
            ..Scatter::default()
        })
    });
    let report = coloured.fig.validate();
    assert!(
        has_error(&report, IssueKind::ShapeMismatch, Some(coloured.artist)),
        "{report:?}"
    );
}

// Why: a rectilinear grid's x vector runs along columns (nx) and y along rows (ny) of the
// row-major field; swapping them is an easy mistake that would silently transpose the plot.
#[test]
fn rectilinear_grid_with_transposed_coordinate_vectors_is_a_shape_mismatch() {
    let correct = grid_fixture(false, |id, d| {
        Artist::Contour(Contour {
            id,
            grid: Grid::Rectilinear { x: d.gx, y: d.gy },
            z: d.field,
            ..Contour::default()
        })
    });
    assert_eq!(correct.fig.validate().errors, vec![]);

    let swapped = grid_fixture(false, |id, d| {
        Artist::Contour(Contour {
            id,
            grid: Grid::Rectilinear { x: d.gy, y: d.gx },
            z: d.field,
            ..Contour::default()
        })
    });
    let report = swapped.fig.validate();
    assert!(
        has_error(&report, IssueKind::ShapeMismatch, Some(swapped.artist)),
        "{report:?}"
    );
}

// Why: contour and surface fields must be two-dimensional; a vector of the right element
// count is not a grid.
#[test]
fn gridded_field_that_is_not_two_dimensional_is_a_shape_mismatch() {
    let fx = grid_fixture(false, |id, d| {
        Artist::Contour(Contour {
            id,
            grid: Grid::Rectilinear { x: d.gx, y: d.gy },
            z: d.p,
            ..Contour::default()
        })
    });
    let report = fx.fig.validate();
    assert!(
        has_error(&report, IssueKind::ShapeMismatch, Some(fx.artist)),
        "{report:?}"
    );
}

// Why: a curvilinear grid gives coordinates for every node, so its coordinate arrays must
// have exactly the field's shape.
#[test]
fn curvilinear_grid_coordinates_must_match_the_field_shape() {
    let correct = grid_fixture(true, |id, d| {
        Artist::Surface(Surface {
            id,
            grid: Grid::Curvilinear { x: d.cx, y: d.cy },
            z: d.field,
            ..Surface::default()
        })
    });
    assert_eq!(correct.fig.validate().errors, vec![]);

    let vectors = grid_fixture(true, |id, d| {
        Artist::Surface(Surface {
            id,
            grid: Grid::Curvilinear { x: d.gx, y: d.cy },
            z: d.field,
            ..Surface::default()
        })
    });
    let report = vectors.fig.validate();
    assert!(
        has_error(&report, IssueKind::ShapeMismatch, Some(vectors.artist)),
        "{report:?}"
    );
}

// Why: surface colour data is sampled at the same nodes as the heights.
#[test]
fn surface_colour_data_must_match_the_field_shape() {
    let fx = grid_fixture(true, |id, d| {
        Artist::Surface(Surface {
            id,
            grid: Grid::Rectilinear { x: d.gx, y: d.gy },
            z: d.field,
            c: Some(d.p),
            ..Surface::default()
        })
    });
    let report = fx.fig.validate();
    assert!(
        has_error(&report, IssueKind::ShapeMismatch, Some(fx.artist)),
        "{report:?}"
    );
}

// Why: each arrow needs a position and a vector; a component array of a different length
// leaves arrows without components.
#[test]
fn quiver_components_must_match_the_positions() {
    let short_v = grid_fixture(false, |id, d| {
        Artist::Quiver(Quiver {
            id,
            x: d.p,
            y: d.q,
            u: d.r,
            v: d.short,
            ..Quiver::default()
        })
    });
    let report = short_v.fig.validate();
    assert!(
        has_error(&report, IssueKind::ShapeMismatch, Some(short_v.artist)),
        "{report:?}"
    );

    let short_w = grid_fixture(true, |id, d| {
        Artist::Quiver(Quiver {
            id,
            x: d.p,
            y: d.q,
            z: Some(d.r),
            u: d.r,
            v: d.q,
            w: Some(d.column),
            ..Quiver::default()
        })
    });
    let report = short_w.fig.validate();
    assert!(
        has_error(&report, IssueKind::ShapeMismatch, Some(short_w.artist)),
        "{report:?}"
    );
}

// Why: 2D axes have no z axis, so a 3D artist placed in them would be drawn wrongly;
// every 3D form (z data, w data, surfaces, contours at level) must be caught.
#[test]
fn three_dimensional_artists_in_two_dimensional_axes_are_errors() {
    let cases: Vec<(&str, MakeArtist)> = vec![
        (
            "line with z",
            Box::new(|id, d: &GridData| {
                Artist::Line(Line {
                    id,
                    x: d.p,
                    y: d.q,
                    z: Some(d.r),
                    ..Line::default()
                })
            }),
        ),
        (
            "scatter with z",
            Box::new(|id, d: &GridData| {
                Artist::Scatter(Scatter {
                    id,
                    x: d.p,
                    y: d.q,
                    z: Some(d.r),
                    ..Scatter::default()
                })
            }),
        ),
        (
            "quiver with z and w",
            Box::new(|id, d: &GridData| {
                Artist::Quiver(Quiver {
                    id,
                    x: d.p,
                    y: d.q,
                    z: Some(d.r),
                    u: d.p,
                    v: d.q,
                    w: Some(d.r),
                    ..Quiver::default()
                })
            }),
        ),
        (
            "surface",
            Box::new(|id, d: &GridData| {
                Artist::Surface(Surface {
                    id,
                    grid: Grid::Rectilinear { x: d.gx, y: d.gy },
                    z: d.field,
                    ..Surface::default()
                })
            }),
        ),
        (
            "contour at level",
            Box::new(|id, d: &GridData| {
                Artist::Contour(Contour {
                    id,
                    grid: Grid::Rectilinear { x: d.gx, y: d.gy },
                    z: d.field,
                    placement: ContourPlacement::AtLevel,
                    ..Contour::default()
                })
            }),
        ),
    ];
    for (name, make) in cases {
        let fx = grid_fixture(false, make);
        let report = fx.fig.validate();
        assert!(
            has_error(&report, IssueKind::ThreeDArtistInTwoDAxes, Some(fx.artist)),
            "{name}: {report:?}"
        );
    }
}

// Why: a contour in a plane is the ordinary 2D contour, so it must not be reported in 2D
// axes even when a plane height is given (the height is simply ignored).
#[test]
fn planar_contour_in_two_dimensional_axes_is_not_an_error() {
    let fx = grid_fixture(false, |id, d| {
        Artist::Contour(Contour {
            id,
            grid: Grid::Rectilinear { x: d.gx, y: d.gy },
            z: d.field,
            placement: ContourPlacement::Plane { z: Some(2.0) },
            ..Contour::default()
        })
    });
    assert_eq!(fx.fig.validate().errors, vec![]);
}

// Why: a link to an identifier that is not an axes (missing, or naming an artist) cannot
// be honoured and indicates a corrupted or hand-edited file.
#[test]
fn link_to_a_non_axes_identifier_is_a_dangling_link() {
    let (mut fig, axes, line) = single_line_figure();
    for bad in [NodeId(999), line, fig.id] {
        fig.links = vec![AxisLink {
            dimension: Dimension::X,
            axes: vec![axes, bad],
        }];
        let report = fig.validate();
        assert!(
            error_kinds(&report).contains(&IssueKind::DanglingLink),
            "{bad:?}: {report:?}"
        );
    }
}

// Why: node identifiers address axes and artists for interaction and links, so every
// node (figure, axes and artist alike) must have a distinct identifier.
#[test]
fn nodes_sharing_an_identifier_are_errors() {
    let (mut fig, axes, _) = single_line_figure();
    let mut second = fig.axes[0].clone();
    second.cell = Cell::default();
    second.artists.clear();
    fig.layout = TileLayout { rows: 1, cols: 2 };
    second.cell.col = 1;
    fig.axes.push(second);
    let report = fig.validate();
    assert!(
        has_error(&report, IssueKind::DuplicateNodeId, Some(axes)),
        "{report:?}"
    );

    let (mut fig, _, _) = single_line_figure();
    line_mut(&mut fig).id = fig.id;
    let report = fig.validate();
    assert!(
        has_error(&report, IssueKind::DuplicateNodeId, Some(fig.id)),
        "{report:?}"
    );

    let (mut fig, axes, _) = single_line_figure();
    line_mut(&mut fig).id = axes;
    let report = fig.validate();
    assert!(
        has_error(&report, IssueKind::DuplicateNodeId, Some(axes)),
        "{report:?}"
    );
}

// Why: a figure with a zero, negative or non-finite size has no page to draw on (a PDF
// page box must have a positive extent), and such a base font size gives no legible text,
// so each must be reported against the figure rather than producing a corrupt export.
#[test]
fn non_positive_or_non_finite_figure_size_or_font_size_is_an_error() {
    for bad in [0.0, -10.0, f64::NAN, f64::INFINITY] {
        for field in ["width", "height", "font size"] {
            let (mut fig, _, _) = single_line_figure();
            match field {
                "width" => fig.size.width_mm = bad,
                "height" => fig.size.height_mm = bad,
                _ => fig.font_size_pt = bad,
            }
            let report = fig.validate();
            assert!(
                has_error(&report, IssueKind::InvalidSize, Some(fig.id)),
                "{field} {bad}: {report:?}"
            );
        }
    }
}

// Why: an axes whose cell extends past the tile layout, or spans no cells, has no place
// on the page.
#[test]
fn cell_outside_the_layout_or_with_zero_span_is_an_error() {
    let cells = [
        Cell {
            row: 0,
            col: 2,
            row_span: 1,
            col_span: 1,
        },
        Cell {
            row: 0,
            col: 1,
            row_span: 1,
            col_span: 2,
        },
        Cell {
            row: 1,
            col: 0,
            row_span: 1,
            col_span: 1,
        },
        Cell {
            row: 0,
            col: 0,
            row_span: 2,
            col_span: 1,
        },
        Cell {
            row: 0,
            col: 0,
            row_span: 0,
            col_span: 1,
        },
        Cell {
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 0,
        },
    ];
    for cell in cells {
        let (mut fig, axes, _) = single_line_figure();
        fig.layout = TileLayout { rows: 1, cols: 2 };
        fig.axes[0].cell = cell;
        let report = fig.validate();
        assert!(
            has_error(&report, IssueKind::CellOutOfLayout, Some(axes)),
            "{cell:?}: {report:?}"
        );
    }

    let (mut fig, _, _) = single_line_figure();
    fig.layout = TileLayout { rows: 1, cols: 2 };
    fig.axes[0].cell = Cell {
        row: 0,
        col: 0,
        row_span: 1,
        col_span: 2,
    };
    assert_eq!(
        fig.validate().errors,
        vec![],
        "a cell spanning the full layout is valid"
    );
}

// Why: log axes cannot show zero or negative values, which are dropped when drawing; the
// user must be told, but the figure is still drawable, so this is a warning, not an error.
#[test]
fn non_positive_data_on_a_log_axis_is_a_warning() {
    for bad in [0.0, -1.0] {
        let (mut fig, _, line) = single_line_figure();
        fig.axes[0].y.scale = Scale::Log;
        let y = line_mut(&mut fig).y;
        fig.data.get_mut(&y).unwrap().values[1] = bad;
        let report = fig.validate();
        assert!(
            has_warning(&report, IssueKind::NonPositiveOnLogAxis, Some(line)),
            "{bad}: {report:?}"
        );
        assert!(
            report.is_valid(),
            "a warning must not make the figure invalid"
        );
    }
}

// Why: the log warning concerns only data plotted along the log axis; non-positive data
// on a linear axis and missing (NaN) values on a log axis are ordinary.
#[test]
fn log_axis_warning_is_limited_to_finite_data_on_that_axis() {
    let (mut fig, _, _) = single_line_figure();
    fig.axes[0].x.scale = Scale::Log;
    let y = line_mut(&mut fig).y;
    let x = line_mut(&mut fig).x;
    fig.data.get_mut(&y).unwrap().values[1] = -5.0;
    fig.data.get_mut(&x).unwrap().values[1] = f64::NAN;
    let report = fig.validate();
    assert_eq!(report.warnings, vec![]);
}

// Why: in 3D axes the z data of a surface is plotted along z, so a log z axis must warn
// about non-positive heights.
#[test]
fn non_positive_surface_heights_on_a_log_z_axis_are_a_warning() {
    let mut fx = grid_fixture(true, |id, d| {
        Artist::Surface(Surface {
            id,
            grid: Grid::Rectilinear { x: d.gx, y: d.gy },
            z: d.field,
            ..Surface::default()
        })
    });
    fx.fig.axes[0].z.scale = Scale::Log;
    assert_eq!(fx.fig.validate().warnings, vec![]);
    let field = match &fx.fig.axes[0].artists[0] {
        Artist::Surface(s) => s.z,
        _ => unreachable!(),
    };
    fx.fig.data.get_mut(&field).unwrap().values[5] = 0.0;
    let report = fx.fig.validate();
    assert!(
        has_warning(&report, IssueKind::NonPositiveOnLogAxis, Some(fx.artist)),
        "{report:?}"
    );
}

// Why: only data plotted along a logarithmic axis is dropped from drawing; quiver
// components, colour data and marker sizes are not positions, so negative values in them
// are ordinary and must not be reported against a log axis.
#[test]
fn log_axis_warning_ignores_data_not_plotted_along_that_axis() {
    let negative = |b: &mut FigureBuilder| b.vector(&[-1.0; 12]);

    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    {
        let a = b.axes(axes);
        a.x.scale = Scale::Log;
        a.y.scale = Scale::Log;
    }
    let positions = b.vector(&[1.0; 12]);
    let components = negative(&mut b);
    let quiver = b.node();
    b.push(
        axes,
        Artist::Quiver(Quiver {
            id: quiver,
            x: positions,
            y: positions,
            u: components,
            v: components,
            ..Quiver::default()
        }),
    );
    let scatter = b.node();
    b.push(
        axes,
        Artist::Scatter(Scatter {
            id: scatter,
            x: positions,
            y: positions,
            color: ScatterColor::Data { data: components },
            ..Scatter::default()
        }),
    );
    let report = b.build().validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![]);

    let mut fx = grid_fixture(true, |id, d| {
        Artist::Surface(Surface {
            id,
            grid: Grid::Rectilinear { x: d.gx, y: d.gy },
            z: d.field,
            c: Some(d.cy),
            ..Surface::default()
        })
    });
    fx.fig.axes[0].z.scale = Scale::Log;
    let colour = match &fx.fig.axes[0].artists[0] {
        Artist::Surface(s) => s.c.unwrap(),
        _ => unreachable!(),
    };
    fx.fig.data.get_mut(&colour).unwrap().values[0] = -1.0;
    let report = fx.fig.validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![]);
}

// Why: the scene compiler maps data through `(value - min) / (max - min)` (in log space
// on log axes), so manual limits that are non-finite, empty, reversed or non-positive on a
// log axis cannot be drawn; the viewer's zoom could produce such limits, so they must be
// caught with the axes identified rather than rendered as garbage.
#[test]
fn invalid_manual_limits_are_errors_on_the_axes() {
    let manual = |min, max| Limits::Manual { min, max };
    let bad = [
        manual(1.0, 1.0),
        manual(2.0, 1.0),
        manual(f64::NAN, 1.0),
        manual(0.0, f64::INFINITY),
    ];
    for limits in bad {
        let (mut fig, axes, _) = single_line_figure();
        fig.axes[0].y.limits = limits;
        let report = fig.validate();
        assert!(
            has_error(&report, IssueKind::InvalidLimits, Some(axes)),
            "{limits:?}: {report:?}"
        );

        let (mut fig, axes, _) = single_line_figure();
        fig.axes[0].clim = limits;
        let report = fig.validate();
        assert!(
            has_error(&report, IssueKind::InvalidLimits, Some(axes)),
            "clim {limits:?}: {report:?}"
        );
    }

    let (mut fig, axes, _) = single_line_figure();
    fig.axes[0].x.scale = Scale::Log;
    fig.axes[0].x.limits = manual(0.0, 10.0);
    let report = fig.validate();
    assert!(
        has_error(&report, IssueKind::InvalidLimits, Some(axes)),
        "log axis from zero: {report:?}"
    );

    // Negative limits are ordinary on a linear axis, and colour limits have no scale.
    let (mut fig, _, _) = single_line_figure();
    fig.axes[0].x.limits = manual(-10.0, -1.0);
    fig.axes[0].clim = manual(-10.0, -1.0);
    assert_eq!(fig.validate().errors, vec![]);
}

// Why: contouring needs at least one level and relies on levels being ordered to assign
// isobands, so empty, non-finite, unordered or repeated explicit levels, or an automatic
// count of zero, cannot be drawn and must be reported against the contour.
#[test]
fn invalid_contour_levels_are_errors_on_the_contour() {
    let bad = [
        Levels::Explicit { values: vec![] },
        Levels::Explicit {
            values: vec![1.0, f64::NAN],
        },
        Levels::Explicit {
            values: vec![2.0, 1.0],
        },
        Levels::Explicit {
            values: vec![1.0, 1.0],
        },
        Levels::Auto { count: 0 },
    ];
    for levels in bad {
        let fx = grid_fixture(false, |id, d| {
            Artist::Contour(Contour {
                id,
                grid: Grid::Rectilinear { x: d.gx, y: d.gy },
                z: d.field,
                levels: levels.clone(),
                ..Contour::default()
            })
        });
        let report = fx.fig.validate();
        assert!(
            has_error(&report, IssueKind::InvalidLevels, Some(fx.artist)),
            "{levels:?}: {report:?}"
        );
    }

    // A single level is a valid contour (for example the zero isoline).
    let fx = grid_fixture(false, |id, d| {
        Artist::Contour(Contour {
            id,
            grid: Grid::Rectilinear { x: d.gx, y: d.gy },
            z: d.field,
            levels: Levels::Explicit { values: vec![0.0] },
            ..Contour::default()
        })
    });
    assert_eq!(fx.fig.validate().errors, vec![]);
}
