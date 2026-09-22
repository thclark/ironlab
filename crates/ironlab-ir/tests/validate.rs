//! Structural validation of figures.
//!
//! Each error kind is triggered by the smallest change to an otherwise valid figure, so
//! that a test fails only if that specific check is missing or over-eager.

mod common;

use common::{
    FigureBuilder, floats_mut, image_variants_figure, kitchen_sink_figure, single_line_figure,
};
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
/// positive values, a vector `short` of 11 values, a vector `column` of 3 values, a
/// 3 × 4 array `bytes` of 8-bit values, and the pixels of a 3 × 4 image as 8-bit RGB
/// components (`rgb`) and as floating-point RGBA components (`rgba`).
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
    bytes: DataId,
    rgb: DataId,
    rgba: DataId,
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
        bytes: b.bytes(vec![3, 4], (0..12).collect()),
        rgb: b.bytes(vec![3, 4, 3], (0..36).collect()),
        rgba: b.data(
            NdArray::from_shape(vec![3, 4, 4], (0..48).map(|k| k as f64 / 47.0).collect())
                .expect("the shape matches the values"),
        ),
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
            values: Values::F64(vec![1.0, 2.0, 3.0]),
        },
    );
    let report = fig.validate();
    assert!(
        error_kinds(&report).contains(&IssueKind::InvalidArray),
        "{report:?}"
    );
}

// Why: an array of bytes is checked against its shape exactly as an array of floats,
// because a byte count that disagrees with the shape would make every consumer index
// past the end of the bytes; it can only arrive through a hand-edited file, since the
// checked constructor refuses it.
#[test]
fn array_of_bytes_whose_shape_does_not_match_its_values_is_an_error() {
    let (mut fig, _, _) = single_line_figure();
    fig.data.insert(
        DataId(50),
        NdArray {
            shape: vec![2, 2],
            values: Values::U8(vec![1, 2, 3]),
        },
    );
    let report = fig.validate();
    assert_eq!(
        error_kinds(&report),
        vec![IssueKind::InvalidArray],
        "{report:?}"
    );
}

// Why: an array of bytes that no artist refers to is ordinary data (an image's pixels
// waiting for their artist), so a check that reported it would make correct figures
// invalid.
#[test]
fn unreferenced_array_of_bytes_is_valid() {
    let (mut fig, _, _) = single_line_figure();
    fig.data.insert(
        DataId(50),
        NdArray::from_shape_u8(vec![2, 2], vec![0, 1, 254, 255]).unwrap(),
    );
    let report = fig.validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![]);
}

// Why: every artist other than an image reads floats, so such an artist that refers to
// an array of bytes, as a coordinate or as a surface's colour data, cannot be drawn.
// Validation must report it against the artist as an element type mismatch, and only
// as that: the bytes have the right shape and count, so a shape mismatch beside it
// would send the user looking for a problem that does not exist.
#[test]
fn artist_referring_to_an_array_of_bytes_is_an_element_type_mismatch() {
    let (mut fig, _, line) = single_line_figure();
    let x = line_mut(&mut fig).x;
    fig.data
        .insert(x, NdArray::from_shape_u8(vec![3], vec![1, 2, 3]).unwrap());
    let report = fig.validate();
    assert!(
        has_error(&report, IssueKind::ElementTypeMismatch, Some(line)),
        "{report:?}"
    );
    assert_eq!(
        error_kinds(&report),
        vec![IssueKind::ElementTypeMismatch],
        "{report:?}"
    );

    let fx = grid_fixture(true, |id, d| {
        Artist::Surface(Surface {
            id,
            grid: Grid::Rectilinear { x: d.gx, y: d.gy },
            z: d.field,
            c: Some(d.bytes),
            ..Surface::default()
        })
    });
    let report = fx.fig.validate();
    assert!(
        has_error(&report, IssueKind::ElementTypeMismatch, Some(fx.artist)),
        "{report:?}"
    );
    assert_eq!(
        error_kinds(&report),
        vec![IssueKind::ElementTypeMismatch],
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

// Why: 2D axes have no z axis, so an artist that needs one to be placed would be drawn
// wrongly; every such form (z data, w data, contours at level, images on a wall) must be
// caught. A surface is not among them: seen from directly above it is the pseudocolour
// plot of a 2D axes (see `surface_in_two_dimensional_axes_is_valid`).
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
        (
            "image on the xz wall",
            Box::new(|id, d: &GridData| image_at(id, d.rgb, on(ImagePlane::Xz { y: None }))),
        ),
        (
            "indexed image on the yz wall",
            Box::new(|id, d: &GridData| {
                indexed_at(id, d.bytes, on(ImagePlane::Yz { x: Some(1.0) }))
            }),
        ),
        (
            "mapped image on the xz wall",
            Box::new(|id, d: &GridData| {
                mapped_at(id, d.field, on(ImagePlane::Xz { y: Some(-1.0) }))
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

// Why: a surface in a 2D axes is how a pseudocolour plot (MATLAB's `pcolor`) is made: the
// grid is seen from directly above and the field colours the faces. It must validate on
// both kinds of grid, with and without separate colour data, because reporting it as a 3D
// artist would leave users with no pseudocolour plot at all.
#[test]
fn surface_in_two_dimensional_axes_is_valid() {
    let cases: Vec<(&str, MakeArtist)> = vec![
        (
            "rectilinear grid coloured by its field",
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
            "curvilinear grid with separate colour data",
            Box::new(|id, d: &GridData| {
                Artist::Surface(Surface {
                    id,
                    grid: Grid::Curvilinear { x: d.cx, y: d.cy },
                    z: d.field,
                    c: Some(d.cy),
                    ..Surface::default()
                })
            }),
        ),
    ];
    for (name, make) in cases {
        let report = grid_fixture(false, make).fig.validate();
        assert_eq!(report.errors, vec![], "{name}");
        assert_eq!(report.warnings, vec![], "{name}");
    }
}

// Why: allowing a surface in a 2D axes must not switch off its other checks, as it would
// if a surface in a 2D axes were simply passed over. The grid and the colour data are
// combined with the field node by node whatever the projection, and they are checked
// separately, so one mistake of each kind must still be reported in a 2D axes, and as the
// mismatch it is rather than as a misplaced 3D artist. The remaining ways of getting a
// grid wrong are covered, whatever the projection, by the grid tests above.
#[test]
fn surface_shape_checks_still_apply_in_two_dimensional_axes() {
    let cases: Vec<(&str, MakeArtist)> = vec![
        (
            "transposed rectilinear grid",
            Box::new(|id, d: &GridData| {
                Artist::Surface(Surface {
                    id,
                    grid: Grid::Rectilinear { x: d.gy, y: d.gx },
                    z: d.field,
                    ..Surface::default()
                })
            }),
        ),
        (
            "colour data of another shape",
            Box::new(|id, d: &GridData| {
                Artist::Surface(Surface {
                    id,
                    grid: Grid::Rectilinear { x: d.gx, y: d.gy },
                    z: d.field,
                    c: Some(d.p),
                    ..Surface::default()
                })
            }),
        ),
    ];
    for (name, make) in cases {
        let fx = grid_fixture(false, make);
        let report = fx.fig.validate();
        assert_eq!(
            error_kinds(&report),
            vec![IssueKind::ShapeMismatch],
            "{name}: {report:?}"
        );
        assert_eq!(report.errors[0].node, Some(fx.artist), "{name}");
    }
}

// ---------------------------------------------------------------------------------
// Nothing to draw (ADR 0012)
// ---------------------------------------------------------------------------------

/// Asserts that a report has no errors and exactly one warning, of kind `NothingToDraw`,
/// against `artist`, whose message states `shape`, the shape of the artist's array as
/// stored (written as every validation message writes a shape, in the form `[ny, nx]`);
/// `at` names the case in the failure message.
#[track_caller]
fn assert_nothing_to_draw(report: &ValidationReport, artist: NodeId, shape: &[usize], at: &str) {
    assert_eq!(report.errors, vec![], "{at}: {report:?}");
    assert!(
        report.is_valid(),
        "{at}: a warning must not make the figure invalid"
    );
    assert_eq!(report.warnings.len(), 1, "{at}: {report:?}");
    let warning = &report.warnings[0];
    assert_eq!(warning.kind, IssueKind::NothingToDraw, "{at}: {warning:?}");
    assert_eq!(
        warning.node,
        Some(artist),
        "{at}: the warning must name the artist"
    );
    let shape = format!("{shape:?}");
    assert!(
        warning.message.contains(&shape),
        "{at}: the message must state the shape {shape}: {}",
        warning.message
    );
}

/// Asserts that a report has no errors and no warnings; `at` names the case.
#[track_caller]
fn assert_no_issues(report: &ValidationReport, at: &str) {
    assert_eq!(report.errors, vec![], "{at}: {report:?}");
    assert_eq!(report.warnings, vec![], "{at}: {report:?}");
}

// Why: a contour and a surface draw the cells between the nodes of their grid, so a field
// with no rows, no columns, a single row or a single column gives the artist nothing to
// draw, and the compiler leaves it out. A user who plots a single row of values expects a
// strip of cells, as a mapped image would give, and a user whose computation returned no
// data expects to be told; the report they read is `validate()`, so it must name the
// artist, say that it draws nothing and state the shape, as ADR 0012 decides. It must do
// so as a warning, not an error, because a field streamed into a figure has no rows and
// then one row before it has a cell, and the stream must not be refused at either step.
// The rule must hold for both artists, both projections and both kinds of grid, and must
// not fire for the smallest grid that has a cell.
#[test]
fn a_field_with_no_cells_is_a_warning_that_the_artist_draws_nothing() {
    type Make = fn(NodeId, Grid, DataId) -> Artist;
    let contour: Make = |id, grid, z| {
        Artist::Contour(Contour {
            id,
            grid,
            z,
            ..Contour::default()
        })
    };
    let surface: Make = |id, grid, z| {
        Artist::Surface(Surface {
            id,
            grid,
            z,
            ..Surface::default()
        })
    };
    // (name, rows, columns, whether the field gives the artist nothing to draw)
    let shapes = [
        ("a single row", 1, 4, true),
        ("a single column", 3, 1, true),
        ("a single node", 1, 1, true),
        ("no rows", 0, 4, true),
        ("no columns", 3, 0, true),
        ("no rows and no columns", 0, 0, true),
        ("the smallest grid with a cell", 2, 2, false),
    ];
    for (artist_name, make) in [("contour", contour), ("surface", surface)] {
        for three_d in [false, true] {
            for (shape_name, ny, nx, nothing) in shapes {
                for curvilinear in [false, true] {
                    let mut b = FigureBuilder::new();
                    let axes = if three_d {
                        b.axes3d(0, 0)
                    } else {
                        b.axes2d(0, 0)
                    };
                    let field = b.matrix(ny, nx, |j, i| (j + i) as f64);
                    let grid = if curvilinear {
                        Grid::Curvilinear {
                            x: b.matrix(ny, nx, |_, i| i as f64),
                            y: b.matrix(ny, nx, |j, _| j as f64),
                        }
                    } else {
                        Grid::Rectilinear {
                            x: b.vector(&(0..nx).map(|i| i as f64).collect::<Vec<_>>()),
                            y: b.vector(&(0..ny).map(|j| j as f64).collect::<Vec<_>>()),
                        }
                    };
                    let id = b.node();
                    b.push(axes, make(id, grid, field));
                    let report = b.build().validate();
                    let at = format!(
                        "a {artist_name} on {shape_name} of a {} grid in a {} axes",
                        if curvilinear {
                            "curvilinear"
                        } else {
                            "rectilinear"
                        },
                        if three_d { "3D" } else { "2D" }
                    );
                    if nothing {
                        assert_nothing_to_draw(&report, id, &[ny, nx], &at);
                    } else {
                        assert_no_issues(&report, &at);
                    }
                }
            }
        }
    }
}

// Why: a line, a scatter and a quiver draw at their points, so an artist of no points, which
// a filter that matched nothing or a figure built before its data arrives produces, draws
// nothing and must be reported as such against the artist, once, in both projections; a
// figure holding several such artists must name each, so that the reader of the report
// need not guess which is missing. The figure stays valid, so that a stream may grow a
// series from nothing. ADR 0012 draws the line at no points, not at one: a single point is
// something to draw (a scatter puts a marker on it, a quiver an arrow, a line its marker
// if it has one), and the rule is made from the shape of the arrays alone, so the first
// sample of a stream must not be reported.
#[test]
fn a_line_scatter_or_quiver_of_no_points_is_a_warning_that_names_each_artist() {
    for three_d in [false, true] {
        let mut b = FigureBuilder::new();
        let axes = if three_d {
            b.axes3d(0, 0)
        } else {
            b.axes2d(0, 0)
        };
        let e = || NdArray::vector(vec![]);
        let (x, y) = (b.data(e()), b.data(e()));
        let z = three_d.then(|| b.data(e()));
        let line = b.node();
        b.push(
            axes,
            Artist::Line(Line {
                id: line,
                x,
                y,
                z,
                ..Line::default()
            }),
        );
        let scatter = b.node();
        b.push(
            axes,
            Artist::Scatter(Scatter {
                id: scatter,
                x,
                y,
                z,
                ..Scatter::default()
            }),
        );
        let (u, v) = (b.data(e()), b.data(e()));
        let quiver = b.node();
        b.push(
            axes,
            Artist::Quiver(Quiver {
                id: quiver,
                x,
                y,
                z,
                u,
                v,
                w: z,
                ..Quiver::default()
            }),
        );
        let report = b.build().validate();
        let at = format!("three_d = {three_d}");
        assert_eq!(report.errors, vec![], "{at}: {report:?}");
        assert!(report.is_valid(), "{at}");
        let mut named: Vec<Option<NodeId>> = report.warnings.iter().map(|w| w.node).collect();
        named.sort();
        assert_eq!(
            named,
            vec![Some(line), Some(scatter), Some(quiver)],
            "{at}: one warning names each empty artist: {report:?}"
        );
        for warning in &report.warnings {
            assert_eq!(warning.kind, IssueKind::NothingToDraw, "{at}: {warning:?}");
        }

        // A single point is drawn, so it is not reported.
        let mut b = FigureBuilder::new();
        let axes = if three_d {
            b.axes3d(0, 0)
        } else {
            b.axes2d(0, 0)
        };
        let one = || NdArray::vector(vec![1.0]);
        let (x, y) = (b.data(one()), b.data(one()));
        let z = three_d.then(|| b.data(one()));
        let (u, v) = (b.data(one()), b.data(one()));
        let ids = [b.node(), b.node(), b.node()];
        b.push(
            axes,
            Artist::Line(Line {
                id: ids[0],
                x,
                y,
                z,
                ..Line::default()
            }),
        );
        b.push(
            axes,
            Artist::Scatter(Scatter {
                id: ids[1],
                x,
                y,
                z,
                ..Scatter::default()
            }),
        );
        b.push(
            axes,
            Artist::Quiver(Quiver {
                id: ids[2],
                x,
                y,
                z,
                u,
                v,
                w: z,
                ..Quiver::default()
            }),
        );
        assert_no_issues(&b.build().validate(), &format!("{at}, one point"));
    }
}

// Why: an image with no rows or no columns holds no pixels, so it draws nothing, and the
// user must learn why the image is missing from the report rather than from an empty
// axes; every image kind, of bytes and of floats alike, in both projections, must be
// reported the same way, with the shape of its array, and the figure stays valid because
// such an image is what a computation that returned no data produces. An image of a single
// pixel draws that pixel, so it must not be reported: unlike a field, an image needs no
// second row or column to have something to show.
#[test]
fn an_image_with_no_rows_or_no_columns_is_a_warning_that_the_image_draws_nothing() {
    let kinds: [(&str, MakeImage, bool); 3] = [
        ("an image", image_at, true),
        ("an indexed image", indexed_at, false),
        ("a mapped image", mapped_at, false),
    ];
    let shape = |components: bool, ny: usize, nx: usize| {
        if components {
            vec![ny, nx, 3]
        } else {
            vec![ny, nx]
        }
    };
    for (name, make, components) in kinds {
        for three_d in [false, true] {
            let projection = if three_d { "3D" } else { "2D" };
            for bytes in [false, true] {
                for (what, ny, nx) in [("no rows", 0, 4), ("no columns", 3, 0), ("no pixels", 0, 0)]
                {
                    let shape = shape(components, ny, nx);
                    let fx = image_fixture(three_d, shape.clone(), bytes, |id, data| {
                        make(id, data, ImagePlacement::default())
                    });
                    let at = format!("{name} with {what} (bytes: {bytes}) in a {projection} axes");
                    assert_nothing_to_draw(&fx.fig.validate(), fx.artist, &shape, &at);
                }
                let fx = image_fixture(three_d, shape(components, 1, 1), bytes, |id, data| {
                    make(id, data, ImagePlacement::default())
                });
                assert_no_issues(
                    &fx.fig.validate(),
                    &format!("{name} of one pixel (bytes: {bytes}) in a {projection} axes"),
                );
            }
        }
    }
}

// Why: the warning describes an artist whose data is sound but gives it nothing to draw.
// An artist whose arrays are missing, hold bytes where floats are needed or disagree in
// shape is already reported for that error, which is what the user must fix; a second
// report against the same artist would send them after a problem that the first repair
// removes. Each such error must therefore stand alone, in every artist family, even when
// the data that can be seen is empty or has no cell.
#[test]
fn an_artist_reported_for_its_references_or_shapes_is_not_also_reported_as_drawing_nothing() {
    /// A deferred constructor of an artist that stores its own arrays in the builder.
    type MakeWith = Box<dyn FnOnce(&mut FigureBuilder, NodeId) -> Artist>;
    /// Stores an empty vector and returns its identifier.
    fn empty(b: &mut FigureBuilder) -> DataId {
        b.data(NdArray::vector(vec![]))
    }
    let cases: Vec<(&str, IssueKind, MakeWith)> = vec![
        (
            "an empty line whose y is unknown",
            IssueKind::UnknownData,
            Box::new(|b, id| {
                Artist::Line(Line {
                    id,
                    x: empty(b),
                    y: DataId(999),
                    ..Line::default()
                })
            }),
        ),
        (
            "an empty scatter whose x holds bytes",
            IssueKind::ElementTypeMismatch,
            Box::new(|b, id| {
                Artist::Scatter(Scatter {
                    id,
                    x: b.bytes(vec![0], vec![]),
                    y: empty(b),
                    ..Scatter::default()
                })
            }),
        ),
        (
            "a quiver of no positions with one component",
            IssueKind::ShapeMismatch,
            Box::new(|b, id| {
                Artist::Quiver(Quiver {
                    id,
                    x: empty(b),
                    y: empty(b),
                    u: b.vector(&[1.0]),
                    v: empty(b),
                    ..Quiver::default()
                })
            }),
        ),
        (
            "a contour of a single row on a transposed grid",
            IssueKind::ShapeMismatch,
            Box::new(|b, id| {
                Artist::Contour(Contour {
                    id,
                    grid: Grid::Rectilinear {
                        x: b.vector(&[0.0]),
                        y: b.vector(&[0.0, 1.0, 2.0, 3.0]),
                    },
                    z: b.matrix(1, 4, |_, i| i as f64),
                    ..Contour::default()
                })
            }),
        ),
        (
            "a surface of no rows whose field is unknown",
            IssueKind::UnknownData,
            Box::new(|b, id| {
                Artist::Surface(Surface {
                    id,
                    grid: Grid::Rectilinear {
                        x: b.vector(&[0.0, 1.0]),
                        y: empty(b),
                    },
                    z: DataId(999),
                    ..Surface::default()
                })
            }),
        ),
        (
            "an image of no rows with two components per pixel",
            IssueKind::ShapeMismatch,
            Box::new(|b, id| {
                image_at(
                    id,
                    b.bytes(vec![0, 4, 2], vec![]),
                    ImagePlacement::default(),
                )
            }),
        ),
        (
            "a mapped image of no columns that is not a matrix",
            IssueKind::ShapeMismatch,
            Box::new(|b, id| {
                mapped_at(
                    id,
                    b.bytes(vec![3, 0, 1], vec![]),
                    ImagePlacement::default(),
                )
            }),
        ),
    ];
    for (name, kind, make) in cases {
        let mut b = FigureBuilder::new();
        let axes = b.axes2d(0, 0);
        let id = b.node();
        let artist = make(&mut b, id);
        b.push(axes, artist);
        let report = b.build().validate();
        assert_eq!(error_kinds(&report), vec![kind], "{name}: {report:?}");
        assert_eq!(report.errors[0].node, Some(id), "{name}");
        assert_eq!(
            report.warnings,
            vec![],
            "{name}: the error must stand alone: {report:?}"
        );
    }
}

// Why: in a 2D axes the field of a surface colours the faces and positions nothing, so a
// logarithmic z scale left on the axes (a 2D axes keeps a z axis it never shows) must not
// warn about non-positive field values, which are ordinary for a pseudocolour plot. The
// grid is still plotted along x, so the same surface must warn about a non-positive x
// coordinate on a logarithmic x axis; otherwise the first half would pass with the
// log-axis check simply missing for surfaces.
#[test]
fn surface_field_in_two_dimensional_axes_is_not_a_position_on_a_log_axis() {
    let mut fx = grid_fixture(false, |id, d| {
        Artist::Surface(Surface {
            id,
            grid: Grid::Curvilinear { x: d.cx, y: d.cy },
            z: d.field,
            ..Surface::default()
        })
    });
    let (field, cx) = match &fx.fig.axes[0].artists[0] {
        Artist::Surface(Surface {
            z,
            grid: Grid::Curvilinear { x, .. },
            ..
        }) => (*z, *x),
        _ => unreachable!(),
    };
    fx.fig.axes[0].z.scale = Scale::Log;
    floats_mut(fx.fig.data.get_mut(&field).unwrap())[5] = -2.0;
    let report = fx.fig.validate();
    assert_eq!(report.errors, vec![]);
    assert_eq!(report.warnings, vec![]);

    // The x coordinate of the first node of the fixture is zero.
    fx.fig.axes[0].x.scale = Scale::Log;
    assert_eq!(floats_mut(fx.fig.data.get_mut(&cx).unwrap())[0], 0.0);
    let report = fx.fig.validate();
    assert_eq!(report.errors, vec![]);
    assert!(
        has_warning(&report, IssueKind::NonPositiveOnLogAxis, Some(fx.artist)),
        "{report:?}"
    );
    assert_eq!(report.warnings.len(), 1, "{report:?}");
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
        floats_mut(fig.data.get_mut(&y).unwrap())[1] = bad;
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
    floats_mut(fig.data.get_mut(&y).unwrap())[1] = -5.0;
    floats_mut(fig.data.get_mut(&x).unwrap())[1] = f64::NAN;
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
    floats_mut(fx.fig.data.get_mut(&field).unwrap())[5] = 0.0;
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
    floats_mut(fx.fig.data.get_mut(&colour).unwrap())[0] = -1.0;
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

// Why: a parameter is found by its name when collections of figures are sorted, filtered
// and searched, so a parameter without a name can never be addressed and is a mistake of
// the writer. It must be reported against the figure, because parameters belong to no
// other node.
#[test]
fn parameter_with_an_empty_name_is_an_error_on_the_figure() {
    let (mut fig, _, _) = single_line_figure();
    fig.parameters.insert(String::new(), Parameter::Bool(true));
    let report = fig.validate();
    assert_eq!(error_kinds(&report), vec![IssueKind::InvalidParameter]);
    assert!(has_error(
        &report,
        IssueKind::InvalidParameter,
        Some(fig.id)
    ));
}

// Why: JSON cannot represent a non-finite number, so a figure holding one could be saved
// to `.fig.json` but not reloaded from it, and a NaN never compares equal when figures are
// sorted or filtered by it. Each non-finite value must be reported, with the name of
// the parameter, so that the user can find it.
#[test]
fn non_finite_number_parameter_is_an_error_that_names_the_parameter() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let (mut fig, _, _) = single_line_figure();
        fig.parameters
            .insert("reynolds_number".to_owned(), Parameter::Number(bad));
        let report = fig.validate();
        assert_eq!(
            error_kinds(&report),
            vec![IssueKind::InvalidParameter],
            "{bad}: {report:?}"
        );
        assert!(has_error(
            &report,
            IssueKind::InvalidParameter,
            Some(fig.id)
        ));
        assert!(
            report.errors[0].message.contains("reynolds_number"),
            "the message does not name the parameter: {}",
            report.errors[0].message
        );
    }
}

// Why: the check must not be over-eager. Every non-empty name and every finite value is
// valid, including the extremes of the integer range, negative zero, the smallest
// subnormal, an empty string value, and names that are only whitespace or are not ASCII;
// rejecting any of them would stop a user describing a real figure.
#[test]
fn every_parameter_value_that_both_encodings_represent_is_valid() {
    let (mut fig, _, _) = single_line_figure();
    fig.parameters = [
        (" ", Parameter::Bool(false)),
        ("min", Parameter::Integer(i64::MIN)),
        ("max", Parameter::Integer(i64::MAX)),
        ("negative zero", Parameter::Number(-0.0)),
        ("subnormal", Parameter::Number(f64::from_bits(1))),
        ("largest", Parameter::Number(f64::MAX)),
        ("empty", Parameter::String(String::new())),
        ("Überströmung", Parameter::String("ja".to_owned())),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value))
    .collect();
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// Why: a user repairing a figure needs every problem at once, so the check must not stop
// at the first invalid parameter.
#[test]
fn every_invalid_parameter_is_reported() {
    let (mut fig, _, _) = single_line_figure();
    fig.parameters.insert(String::new(), Parameter::Integer(1));
    fig.parameters
        .insert("a".to_owned(), Parameter::Number(f64::NAN));
    fig.parameters
        .insert("b".to_owned(), Parameter::Number(f64::INFINITY));
    let report = fig.validate();
    assert_eq!(
        error_kinds(&report),
        vec![IssueKind::InvalidParameter; 3],
        "{report:?}"
    );
}

// ---------------------------------------------------------------------------------
// Images
// ---------------------------------------------------------------------------------

/// A true-colour image of the given pixels with the given placement.
fn image_at(id: NodeId, pixels: DataId, placement: ImagePlacement) -> Artist {
    Artist::Image(Image {
        id,
        pixels,
        placement,
        ..Image::default()
    })
}

/// A colour-indexed image of the given indices with the given placement and the default
/// (transparent) policies.
fn indexed_at(id: NodeId, indices: DataId, placement: ImagePlacement) -> Artist {
    Artist::IndexedImage(IndexedImage {
        id,
        indices,
        placement,
        ..IndexedImage::default()
    })
}

/// A colour-mapped image of the given values with the given placement and the default
/// (transparent) policies.
fn mapped_at(id: NodeId, values: DataId, placement: ImagePlacement) -> Artist {
    Artist::MappedImage(MappedImage {
        id,
        values,
        placement,
        ..MappedImage::default()
    })
}

/// A colour-indexed image in the default placement with the given policies for indices
/// below the map, above it and non-finite.
fn indexed_with(
    id: NodeId,
    indices: DataId,
    [below, above, non_finite]: [OutOfRange; 3],
) -> Artist {
    Artist::IndexedImage(IndexedImage {
        id,
        indices,
        below,
        above,
        non_finite,
        ..IndexedImage::default()
    })
}

/// A colour-mapped image in the default placement with the given policies for values
/// below the colour limits, above them and non-finite.
fn mapped_with(id: NodeId, values: DataId, [below, above, non_finite]: [OutOfRange; 3]) -> Artist {
    Artist::MappedImage(MappedImage {
        id,
        values,
        below,
        above,
        non_finite,
        ..MappedImage::default()
    })
}

/// A placement on the given plane with no pixel ranges.
fn on(plane: ImagePlane) -> ImagePlacement {
    ImagePlacement {
        plane,
        ..ImagePlacement::default()
    }
}

/// A constructor of an image artist from an identifier, its array and its placement.
type MakeImage = fn(NodeId, DataId, ImagePlacement) -> Artist;

/// One image kind named for assertion messages, with its constructor and the array of
/// a [`GridFixture`] that suits it.
type ImageKind = (&'static str, MakeImage, fn(&GridData) -> DataId);

/// The three image kinds, named for assertion messages, each with the array of a
/// [`GridFixture`] that suits it: the RGB pixels, the 8-bit indices and the field.
fn image_kinds() -> [ImageKind; 3] {
    [
        ("an image", image_at, |d| d.rgb),
        ("an indexed image", indexed_at, |d| d.bytes),
        ("a mapped image", mapped_at, |d| d.field),
    ]
}

/// Builds a figure with one 2D or 3D axes holding the single image artist made by
/// `make` on a fresh array of the given shape, of floats in `[0, 1)` or of bytes.
fn image_fixture(
    three_d: bool,
    shape: Vec<usize>,
    bytes: bool,
    make: impl FnOnce(NodeId, DataId) -> Artist,
) -> GridFixture {
    let mut b = FigureBuilder::new();
    let axes = if three_d {
        b.axes3d(0, 0)
    } else {
        b.axes2d(0, 0)
    };
    let len = shape.iter().product::<usize>();
    let data = if bytes {
        b.bytes(shape, (0..len).map(|k| (k % 256) as u8).collect())
    } else {
        b.data(
            NdArray::from_shape(shape, (0..len).map(|k| k as f64 / len as f64).collect())
                .expect("the shape matches the values"),
        )
    };
    let artist = b.node();
    b.push(axes, make(artist, data));
    GridFixture {
        fig: b.build(),
        artist,
    }
}

/// Returns the array that the single image artist of a fixture draws.
fn image_data(fx: &GridFixture) -> DataId {
    match &fx.fig.axes[0].artists[0] {
        Artist::Image(a) => a.pixels,
        Artist::IndexedImage(a) => a.indices,
        Artist::MappedImage(a) => a.values,
        other => panic!("expected an image, found {other:?}"),
    }
}

// Why: a true-colour image reads one pixel per row and column with three or four
// components each, so pixels of any other shape would be read as garbage colours or
// past the end of the array; validation must refuse every other shape as a shape
// mismatch (and as nothing else, so that the user is not sent after a second problem)
// and accept exactly the two shapes the scene compiler draws, of floats and of bytes
// alike, because an image is the one artist that reads bytes. An image with no pixels
// along a dimension has the right shape and is accepted too; that it is then reported
// as having nothing to draw is pinned in
// `an_image_with_no_rows_or_no_columns_is_a_warning_that_the_image_draws_nothing`.
#[test]
fn image_pixels_must_be_three_dimensional_with_three_or_four_components() {
    for shape in [
        vec![3, 4],
        vec![3, 4, 2],
        vec![3, 4, 5],
        vec![36],
        vec![1, 3, 4, 3],
        vec![3, 4, 3, 1],
    ] {
        for bytes in [false, true] {
            let fx = image_fixture(false, shape.clone(), bytes, |id, pixels| {
                image_at(id, pixels, ImagePlacement::default())
            });
            let report = fx.fig.validate();
            assert!(
                has_error(&report, IssueKind::ShapeMismatch, Some(fx.artist)),
                "pixels of shape {shape:?} (bytes: {bytes}): {report:?}"
            );
            assert_eq!(
                error_kinds(&report),
                vec![IssueKind::ShapeMismatch],
                "pixels of shape {shape:?} (bytes: {bytes}): {report:?}"
            );
        }
    }
    for shape in [vec![3, 4, 3], vec![3, 4, 4], vec![1, 1, 3], vec![2, 1, 4]] {
        for bytes in [false, true] {
            let fx = image_fixture(false, shape.clone(), bytes, |id, pixels| {
                image_at(id, pixels, ImagePlacement::default())
            });
            let report = fx.fig.validate();
            assert_eq!(
                report.errors,
                vec![],
                "pixels of shape {shape:?} (bytes: {bytes})"
            );
            assert_eq!(report.warnings, vec![]);
        }
    }
    for shape in [vec![0, 4, 3], vec![0, 0, 4]] {
        for bytes in [false, true] {
            let fx = image_fixture(false, shape.clone(), bytes, |id, pixels| {
                image_at(id, pixels, ImagePlacement::default())
            });
            assert_eq!(
                fx.fig.validate().errors,
                vec![],
                "pixels of shape {shape:?} (bytes: {bytes})"
            );
        }
    }
}

// Why: the two mapped image kinds read one index or value per pixel of a matrix, so a
// vector or a stack of matrices has no rows and columns to place; validation must refuse
// them as shape mismatches only, and accept a matrix of floats or of bytes, because
// 8-bit indices are the natural form of a colour-indexed image and 8-bit values map
// through the colormap like any others. A matrix with no pixels along a dimension has
// the right shape and is accepted too; that it is then reported as having nothing to
// draw is pinned in
// `an_image_with_no_rows_or_no_columns_is_a_warning_that_the_image_draws_nothing`.
#[test]
fn indices_and_values_of_the_mapped_image_kinds_must_be_two_dimensional() {
    let kinds: [(&str, MakeImage); 2] = [
        ("an indexed image", indexed_at),
        ("a mapped image", mapped_at),
    ];
    for (name, make) in kinds {
        for shape in [vec![12], vec![3, 4, 1], vec![3, 4, 3], vec![1, 3, 4]] {
            for bytes in [false, true] {
                let fx = image_fixture(false, shape.clone(), bytes, |id, data| {
                    make(id, data, ImagePlacement::default())
                });
                let report = fx.fig.validate();
                assert!(
                    has_error(&report, IssueKind::ShapeMismatch, Some(fx.artist)),
                    "{name} of shape {shape:?} (bytes: {bytes}): {report:?}"
                );
                assert_eq!(
                    error_kinds(&report),
                    vec![IssueKind::ShapeMismatch],
                    "{name} of shape {shape:?} (bytes: {bytes}): {report:?}"
                );
            }
        }
        for shape in [vec![3, 4], vec![1, 1], vec![1, 4]] {
            for bytes in [false, true] {
                let fx = image_fixture(false, shape.clone(), bytes, |id, data| {
                    make(id, data, ImagePlacement::default())
                });
                let report = fx.fig.validate();
                assert_eq!(
                    report.errors,
                    vec![],
                    "{name} of shape {shape:?} (bytes: {bytes})"
                );
                assert_eq!(report.warnings, vec![]);
            }
        }
        for bytes in [false, true] {
            let fx = image_fixture(false, vec![3, 0], bytes, |id, data| {
                make(id, data, ImagePlacement::default())
            });
            assert_eq!(
                fx.fig.validate().errors,
                vec![],
                "{name} of shape [3, 0] (bytes: {bytes})"
            );
        }
    }
}

// Why: an image on the floor is the ordinary two-dimensional image, so it must not be
// reported in 2D axes even when a floor offset is given (the offset is simply ignored,
// as the height of a contour plane is), or every image saved from a 3D axes would be
// unusable in a 2D one.
#[test]
fn a_floor_image_in_a_two_dimensional_axes_is_valid_whatever_its_offset() {
    for z in [None, Some(2.0), Some(-2.0)] {
        for (name, make, data) in image_kinds() {
            let fx = grid_fixture(false, |id, d| make(id, data(d), on(ImagePlane::Xy { z })));
            let report = fx.fig.validate();
            assert_eq!(report.errors, vec![], "{name} at z = {z:?}");
            assert_eq!(report.warnings, vec![], "{name} at z = {z:?}");
        }
        // Floating-point RGBA pixels are placed exactly as 8-bit RGB pixels are.
        let fx = grid_fixture(false, |id, d| {
            image_at(id, d.rgba, on(ImagePlane::Xy { z }))
        });
        assert_eq!(
            fx.fig.validate().errors,
            vec![],
            "floating-point RGBA pixels at z = {z:?}"
        );
    }
}

// Why: the scene compiler places an image by an affine map of its pixel centres and its
// plane offset through the axes, so a centre or offset that is NaN or infinite has no
// position and would be drawn as garbage or dropped without a word; and JSON cannot
// hold such a number, so a figure with one could be saved but not reloaded. Each must
// be reported against the image as an invalid placement, and as that alone.
#[test]
fn non_finite_pixel_centres_and_plane_offsets_are_invalid_placements() {
    let range = |first, last| Some(PixelRange { first, last });
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let placements = [
            (
                "columns.first",
                ImagePlacement {
                    columns: range(bad, 1.0),
                    ..ImagePlacement::default()
                },
            ),
            (
                "columns.last",
                ImagePlacement {
                    columns: range(0.0, bad),
                    ..ImagePlacement::default()
                },
            ),
            (
                "rows.first",
                ImagePlacement {
                    rows: range(bad, 1.0),
                    ..ImagePlacement::default()
                },
            ),
            (
                "rows.last",
                ImagePlacement {
                    rows: range(0.0, bad),
                    ..ImagePlacement::default()
                },
            ),
            ("the xy offset", on(ImagePlane::Xy { z: Some(bad) })),
            ("the xz offset", on(ImagePlane::Xz { y: Some(bad) })),
            ("the yz offset", on(ImagePlane::Yz { x: Some(bad) })),
        ];
        for (what, placement) in placements {
            for (name, make, data) in image_kinds() {
                // A 3D axes, so that the wall planes are valid and only the value is at
                // fault.
                let fx = grid_fixture(true, |id, d| make(id, data(d), placement));
                let report = fx.fig.validate();
                assert!(
                    has_error(&report, IssueKind::InvalidImagePlacement, Some(fx.artist)),
                    "{name} with {what} = {bad}: {report:?}"
                );
                assert_eq!(
                    error_kinds(&report),
                    vec![IssueKind::InvalidImagePlacement],
                    "{name} with {what} = {bad}: {report:?}"
                );
            }
        }
    }
    // In a 2D axes the floor offset is ignored when drawing, but a non-finite one is
    // still a placement that no file can hold, and is reported the same way.
    let fx = grid_fixture(false, |id, d| {
        image_at(id, d.rgb, on(ImagePlane::Xy { z: Some(f64::NAN) }))
    });
    let report = fx.fig.validate();
    assert_eq!(
        error_kinds(&report),
        vec![IssueKind::InvalidImagePlacement],
        "{report:?}"
    );
}

// Why: the pixel pitch along an axis is the distance between the first and last centres
// divided by the number of pixels between them, so coincident centres on an axis of
// several pixels give a pitch of zero and an image of no width; on an axis of one pixel
// there is one centre, which may be given twice, and a range that runs backwards
// mirrors the image and is how a user flips it, so neither may be refused.
#[test]
fn coincident_pixel_centres_are_invalid_unless_the_image_has_one_pixel_along_that_axis() {
    let kinds: [(&str, MakeImage, bool); 3] = [
        ("an image", image_at, true),
        ("an indexed image", indexed_at, false),
        ("a mapped image", mapped_at, false),
    ];
    let shape = |components: bool, ny: usize, nx: usize| {
        if components {
            vec![ny, nx, 3]
        } else {
            vec![ny, nx]
        }
    };
    let columns = |first, last| ImagePlacement {
        columns: Some(PixelRange { first, last }),
        ..ImagePlacement::default()
    };
    let rows = |first, last| ImagePlacement {
        rows: Some(PixelRange { first, last }),
        ..ImagePlacement::default()
    };
    for (name, make, components) in kinds {
        for (what, placement) in [("columns", columns(1.0, 1.0)), ("rows", rows(-2.0, -2.0))] {
            let fx = image_fixture(false, shape(components, 3, 4), false, |id, data| {
                make(id, data, placement)
            });
            let report = fx.fig.validate();
            assert!(
                has_error(&report, IssueKind::InvalidImagePlacement, Some(fx.artist)),
                "{name} with coincident {what}: {report:?}"
            );
            assert_eq!(
                error_kinds(&report),
                vec![IssueKind::InvalidImagePlacement],
                "{name} with coincident {what}: {report:?}"
            );
        }
        for (what, ny, nx, placement) in [
            ("one column", 3, 1, columns(1.0, 1.0)),
            ("one row", 1, 4, rows(-2.0, -2.0)),
        ] {
            let fx = image_fixture(false, shape(components, ny, nx), false, |id, data| {
                make(id, data, placement)
            });
            assert_eq!(fx.fig.validate().errors, vec![], "{name} with {what}");
        }
        let mirrored = ImagePlacement {
            columns: Some(PixelRange {
                first: 4.0,
                last: 0.0,
            }),
            rows: Some(PixelRange {
                first: 1.0,
                last: -1.0,
            }),
            ..ImagePlacement::default()
        };
        let fx = image_fixture(false, shape(components, 3, 4), false, |id, data| {
            make(id, data, mirrored)
        });
        assert_eq!(fx.fig.validate().errors, vec![], "{name} mirrored");
    }
}

// WHY: the scene compiler places a plane by the coordinate of its offset along the third
// axis, and a logarithmic axis has no place for zero or a negative coordinate, so such an
// image is not drawn; without this warning the figure would validate cleanly and the
// image would vanish silently. A positive offset, an absent offset (the low end of the
// axis, which is positive on a logarithmic axis) and a two-dimensional axes, which
// ignores the offset, must not warn.
#[test]
fn a_non_positive_offset_along_a_logarithmic_third_axis_is_a_warning() {
    // Whether the axes is 3D, the plane, the dimension made logarithmic, and whether a
    // warning is expected.
    let cases = [
        (true, ImagePlane::Xy { z: Some(0.0) }, Dimension::Z, true),
        (true, ImagePlane::Xy { z: Some(-1.0) }, Dimension::Z, true),
        (true, ImagePlane::Xz { y: Some(0.0) }, Dimension::Y, true),
        (true, ImagePlane::Yz { x: Some(-2.0) }, Dimension::X, true),
        (true, ImagePlane::Xy { z: Some(1.0) }, Dimension::Z, false),
        (true, ImagePlane::Xy { z: None }, Dimension::Z, false),
        (false, ImagePlane::Xy { z: Some(-1.0) }, Dimension::Z, false),
    ];
    for (three_d, plane, dimension, warns) in cases {
        for (name, make, data) in image_kinds() {
            let mut fx = grid_fixture(three_d, |id, d| make(id, data(d), on(plane)));
            let axes = &mut fx.fig.axes[0];
            let axis = match dimension {
                Dimension::X => &mut axes.x,
                Dimension::Y => &mut axes.y,
                Dimension::Z => &mut axes.z,
            };
            axis.scale = Scale::Log;
            let report = fx.fig.validate();
            let at = format!("{name} on {plane:?} with a logarithmic {dimension:?} axis");
            assert_eq!(report.errors, vec![], "{at}: {report:?}");
            let kinds: Vec<IssueKind> = report.warnings.iter().map(|issue| issue.kind).collect();
            if warns {
                assert_eq!(kinds, vec![IssueKind::ImageOnLogAxis], "{at}");
                assert!(
                    report.warnings[0].message.contains("logarithmic"),
                    "{at}: {}",
                    report.warnings[0].message
                );
            } else {
                assert_eq!(kinds, vec![], "{at}");
            }
        }
    }
}

// Why: an image is drawn as one rectangle mapped affinely through the two axes of its
// plane, which a logarithmic axis cannot do, so such an image is not drawn and the user
// must be told; the figure is still drawable, so it is a warning, given for a
// logarithmic axis of the image's plane only (not for the axis it is offset along, and
// not for the z axis of a 2D axes, which is ignored), and it is not the warning for
// non-positive data, which concerns positions and not pixels.
#[test]
fn an_image_on_a_logarithmic_plane_axis_is_a_warning_and_nothing_else() {
    // Whether the axes is 3D, the plane, the dimension made logarithmic, and whether a
    // warning is expected.
    let cases = [
        (false, ImagePlane::Xy { z: None }, Dimension::X, true),
        (false, ImagePlane::Xy { z: Some(1.0) }, Dimension::Y, true),
        (false, ImagePlane::Xy { z: None }, Dimension::Z, false),
        (true, ImagePlane::Xy { z: Some(1.0) }, Dimension::Z, false),
        (true, ImagePlane::Xy { z: None }, Dimension::X, true),
        (true, ImagePlane::Xz { y: None }, Dimension::X, true),
        (true, ImagePlane::Xz { y: None }, Dimension::Z, true),
        (true, ImagePlane::Xz { y: Some(1.0) }, Dimension::Y, false),
        (true, ImagePlane::Yz { x: None }, Dimension::Y, true),
        (true, ImagePlane::Yz { x: None }, Dimension::Z, true),
        (true, ImagePlane::Yz { x: Some(1.0) }, Dimension::X, false),
    ];
    for (three_d, plane, dimension, warns) in cases {
        for (name, make, data) in image_kinds() {
            let mut fx = grid_fixture(three_d, |id, d| make(id, data(d), on(plane)));
            let axes = &mut fx.fig.axes[0];
            let axis = match dimension {
                Dimension::X => &mut axes.x,
                Dimension::Y => &mut axes.y,
                Dimension::Z => &mut axes.z,
            };
            axis.scale = Scale::Log;
            let report = fx.fig.validate();
            let at = format!("{name} on {plane:?} with a logarithmic {dimension:?} axis");
            assert_eq!(report.errors, vec![], "{at}: {report:?}");
            let expected = if warns {
                vec![IssueKind::ImageOnLogAxis]
            } else {
                vec![]
            };
            assert_eq!(
                report
                    .warnings
                    .iter()
                    .map(|issue| issue.kind)
                    .collect::<Vec<_>>(),
                expected,
                "{at}: {report:?}"
            );
            if warns {
                assert!(
                    has_warning(&report, IssueKind::ImageOnLogAxis, Some(fx.artist)),
                    "{at}: the warning must name the image: {report:?}"
                );
            }
        }
    }
}

// Why: a strict category is how a user says that indices outside the map are a fault in
// the data rather than something to paint over, so validation must report a pixel that
// the category covers and only such a pixel: an index is truncated toward zero before it
// is looked up, so -0.5 is entry 0 and 255.9 is entry 255, and a non-finite index is
// neither below nor above the map. The error must name the category, so that the user
// knows which policy to loosen; and 8-bit indices always lie within the map, so every
// category may be strict on them.
#[test]
fn strict_categories_of_an_indexed_image_report_exactly_the_pixels_they_cover() {
    let names = ["below", "above", "non_finite"];
    // An index and the category that covers it, or none when every category accepts it.
    let cases = [
        (-1.0, Some(0)),
        (-256.0, Some(0)),
        (-0.5, None),
        (0.0, None),
        (255.0, None),
        (255.9, None),
        (256.0, Some(1)),
        (1.0e9, Some(1)),
        (f64::NAN, Some(2)),
        (f64::INFINITY, Some(2)),
        (f64::NEG_INFINITY, Some(2)),
    ];
    for (index, covered_by) in cases {
        for strict in 0..3 {
            let mut fx = image_fixture(false, vec![3, 4], false, |id, data| {
                let mut policies = [OutOfRange::Transparent; 3];
                policies[strict] = OutOfRange::Strict;
                indexed_with(id, data, policies)
            });
            let data = image_data(&fx);
            floats_mut(
                fx.fig
                    .data
                    .get_mut(&data)
                    .expect("the fixture holds the indices"),
            )[5] = index;
            let report = fx.fig.validate();
            let at = format!("index {index} with a strict {} category", names[strict]);
            if covered_by == Some(strict) {
                assert!(
                    has_error(&report, IssueKind::PixelOutOfRange, Some(fx.artist)),
                    "{at}: {report:?}"
                );
                assert_eq!(
                    error_kinds(&report),
                    vec![IssueKind::PixelOutOfRange],
                    "{at}: {report:?}"
                );
                let message = &report.errors[0].message;
                let named = match strict {
                    2 => message.contains("non-finite") || message.contains("non_finite"),
                    _ => message.contains(names[strict]),
                };
                assert!(named, "{at}: the error must name the category: {message}");
            } else {
                assert_eq!(report.errors, vec![], "{at}: {report:?}");
            }
        }
    }
    let fx = image_fixture(false, vec![3, 4], true, |id, data| {
        indexed_with(id, data, [OutOfRange::Strict; 3])
    });
    assert_eq!(
        fx.fig.validate().errors,
        vec![],
        "8-bit indices cannot lie outside the map"
    );
}

// Why: the below and above categories of a mapped image concern values outside the
// colour limits, which automatic limits, being the range of the data, can never place a
// value outside; only manual limits can, and only valid ones, because reversed or
// non-finite limits are already reported against the axes and a scan against them would
// report every pixel for a fault that is not the image's. A value on a limit is inside
// it, a non-finite value (NaN or either infinity) is reported by its own category
// whatever the limits and by no other, and 8-bit values are compared with the limits
// like floats.
#[test]
fn strict_categories_of_a_mapped_image_are_checked_against_manual_valid_colour_limits_only() {
    /// A 2D figure with the given colour limits holding one mapped image of a 2 × 3
    /// array of the given values with the given policies.
    fn mapped_fixture(values: Vec<f64>, clim: Limits, policies: [OutOfRange; 3]) -> GridFixture {
        let mut b = FigureBuilder::new();
        let axes = b.axes2d(0, 0);
        b.axes(axes).clim = clim;
        let data = b.data(NdArray::from_shape(vec![2, 3], values).expect("six values"));
        let artist = b.node();
        b.push(axes, mapped_with(artist, data, policies));
        GridFixture {
            fig: b.build(),
            artist,
        }
    }
    let manual = |min, max| Limits::Manual { min, max };
    let strict = |category: usize| {
        let mut policies = [OutOfRange::Transparent; 3];
        policies[category] = OutOfRange::Strict;
        policies
    };
    let (below, above, non_finite) = (0, 1, 2);
    let spread = vec![-0.5, 0.0, 0.5, 1.0, 1.5, 2.0];

    let fx = mapped_fixture(spread.clone(), Limits::Auto, [OutOfRange::Strict; 3]);
    assert_eq!(
        fx.fig.validate().errors,
        vec![],
        "no value lies outside automatic limits"
    );

    for (category, offending) in [(below, true), (above, true), (non_finite, false)] {
        let fx = mapped_fixture(spread.clone(), manual(0.0, 1.0), strict(category));
        let report = fx.fig.validate();
        let at = format!("strict category {category} over {spread:?}");
        if offending {
            assert!(
                has_error(&report, IssueKind::PixelOutOfRange, Some(fx.artist)),
                "{at}: {report:?}"
            );
            assert_eq!(
                error_kinds(&report),
                vec![IssueKind::PixelOutOfRange],
                "{at}: {report:?}"
            );
        } else {
            assert_eq!(report.errors, vec![], "{at}: {report:?}");
        }
    }
    let fx = mapped_fixture(
        vec![0.0, 0.25, 0.5, 0.75, 1.0, 1.0],
        manual(0.0, 1.0),
        [OutOfRange::Strict; 3],
    );
    assert_eq!(
        fx.fig.validate().errors,
        vec![],
        "values on the limits are inside them"
    );

    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let values = vec![0.0, bad, 0.5, 0.75, 1.0, 1.0];
        for clim in [Limits::Auto, manual(0.0, 1.0)] {
            let fx = mapped_fixture(values.clone(), clim, strict(non_finite));
            let report = fx.fig.validate();
            assert!(
                has_error(&report, IssueKind::PixelOutOfRange, Some(fx.artist)),
                "{bad} under {clim:?}: {report:?}"
            );
            assert_eq!(
                error_kinds(&report),
                vec![IssueKind::PixelOutOfRange],
                "{bad} under {clim:?}: {report:?}"
            );
            let fx = mapped_fixture(
                values.clone(),
                clim,
                [
                    OutOfRange::Strict,
                    OutOfRange::Strict,
                    OutOfRange::Transparent,
                ],
            );
            assert_eq!(
                fx.fig.validate().errors,
                vec![],
                "{bad} is neither below nor above the limits {clim:?}"
            );
        }
    }

    let fx = mapped_fixture(vec![5.0; 6], manual(1.0, 0.0), [OutOfRange::Strict; 3]);
    let report = fx.fig.validate();
    assert_eq!(
        error_kinds(&report),
        vec![IssueKind::InvalidLimits],
        "invalid limits are the fault of the axes alone: {report:?}"
    );
    assert!(has_error(
        &report,
        IssueKind::InvalidLimits,
        Some(fx.fig.axes[0].id)
    ));

    // The bytes of the fixture count from 0 to 5.
    let mut fx = image_fixture(false, vec![2, 3], true, |id, data| {
        mapped_with(id, data, strict(above))
    });
    fx.fig.axes[0].clim = manual(0.0, 2.0);
    let report = fx.fig.validate();
    assert!(
        has_error(&report, IssueKind::PixelOutOfRange, Some(fx.artist)),
        "bytes above the limits: {report:?}"
    );
    fx.fig.axes[0].clim = manual(0.0, 5.0);
    assert_eq!(fx.fig.validate().errors, vec![]);
}

// Why: the lenient policies exist so that an image reaches the page whatever its data
// holds, with the offending pixels painted over rather than the figure refused; a
// lenient category must therefore accept every index and value, including those that a
// strict category would report, for both image kinds that have policies.
#[test]
fn lenient_policies_accept_every_pixel() {
    let lenient = [
        OutOfRange::Transparent,
        OutOfRange::Clamp,
        OutOfRange::Rgba {
            color: Color::BLACK,
        },
    ];
    // Below the map or the limits, above them, non-finite, and inside.
    let offending = vec![-5.0, 300.0, f64::NAN, f64::INFINITY, 0.5, 1.0];
    for policy in lenient {
        let mut fx = image_fixture(false, vec![2, 3], false, |id, data| {
            indexed_with(id, data, [policy; 3])
        });
        let data = image_data(&fx);
        *floats_mut(fx.fig.data.get_mut(&data).expect("the indices")) = offending.clone();
        assert_eq!(
            fx.fig.validate().errors,
            vec![],
            "an indexed image under {policy:?}"
        );

        let mut fx = image_fixture(false, vec![2, 3], false, |id, data| {
            mapped_with(id, data, [policy; 3])
        });
        fx.fig.axes[0].clim = Limits::Manual { min: 0.0, max: 1.0 };
        let data = image_data(&fx);
        *floats_mut(fx.fig.data.get_mut(&data).expect("the values")) = offending.clone();
        assert_eq!(
            fx.fig.validate().errors,
            vec![],
            "a mapped image under {policy:?}"
        );
    }
}

// Why: an image that refers to data not in the table cannot be drawn, and the error must
// point at the image and be the only one, because the shape of an array that does not
// exist cannot be checked.
#[test]
fn an_image_referring_to_unknown_data_is_an_unknown_data_error() {
    for (name, make, _) in image_kinds() {
        let fx = grid_fixture(false, |id, _| {
            make(id, DataId(999), ImagePlacement::default())
        });
        let report = fx.fig.validate();
        assert!(
            has_error(&report, IssueKind::UnknownData, Some(fx.artist)),
            "{name}: {report:?}"
        );
        assert_eq!(
            error_kinds(&report),
            vec![IssueKind::UnknownData],
            "{name}: {report:?}"
        );
    }
}

// Why: a check that reports errors on a correct figure is as harmful as a missing
// check; the figure that sets every plane, offset, pixel range and policy of every image
// kind, on floats and on bytes, must validate cleanly, including the strict policies
// where the data cannot violate them and the wall planes in a three-dimensional axes.
#[test]
fn every_image_variant_used_correctly_has_no_issues() {
    let report = image_variants_figure().validate();
    assert_eq!(report.errors, vec![], "unexpected errors");
    assert_eq!(report.warnings, vec![], "unexpected warnings");
}
