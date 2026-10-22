//! Shared fixtures for the viewer integration tests.
//!
//! Hit maps are built by hand rather than by compiling the figure, so that the interaction tests exercise only the
//! interaction logic and state their geometry explicitly.

#![allow(dead_code)]

use std::sync::{Arc, LazyLock};

use ironlab_canvas::FigureState;
use ironlab_ir::{
    Axes, Axis, AxisLink, Dimension, Edit, Figure, Limits, NodeId, Projection, PropertyPath,
    Transaction, Value, View3d, command,
};
use ironlab_scene::display::Rect;
use ironlab_scene::hit::{AxesHit, AxesHitKind, AxisMap};
use ironlab_text::TextEngine;

/// The series that `scatter` makes: automatic circles without a line.
pub fn markers() -> ironlab_ir::Series {
    ironlab_ir::Series {
        line: ironlab_ir::LineStyle {
            dash: ironlab_ir::DashStyle::None,
            ..ironlab_ir::LineStyle::default()
        },
        marker: ironlab_ir::MarkerStyle {
            shape: ironlab_ir::MarkerShape::Circle,
            ..ironlab_ir::MarkerStyle::default()
        },
        ..ironlab_ir::Series::default()
    }
}

/// The node identifier of data array `k` of a fixture: `100 + k`, apart from the identifiers below 100 that the
/// fixtures give the figure, its axes, its artists and its panels, with which a data array shares one space of node
/// identifiers.
pub const fn array_id(k: u64) -> NodeId {
    NodeId(100 + k)
}

/// An unnamed data array of a fixture.
pub fn data_array(id: NodeId, array: ironlab_ir::NdArray) -> ironlab_ir::DataArray {
    ironlab_ir::DataArray {
        id,
        name: None,
        array,
    }
}

/// The unnamed data arrays of a fixture, in the order given.
pub fn data_arrays<const N: usize>(
    arrays: [(NodeId, ironlab_ir::NdArray); N],
) -> Vec<ironlab_ir::DataArray> {
    arrays
        .into_iter()
        .map(|(id, array)| data_array(id, array))
        .collect()
}

/// One text engine for the whole test binary; building it parses the bundled fonts.
pub static TEXT: LazyLock<Arc<TextEngine>> = LazyLock::new(|| Arc::new(TextEngine::new()));

pub const EPS: f64 = 1e-9;

pub fn manual(min: f64, max: f64) -> Limits {
    Limits::Manual { min, max }
}

/// A 2D axes with manual limits x ∈ [0, 10] and y ∈ [0, 5] and linear scales.
pub fn axes_2d(id: u64) -> Axes {
    Axes {
        id: NodeId(id),
        x: Axis {
            limits: manual(0.0, 10.0),
            ..Axis::default()
        },
        y: Axis {
            limits: manual(0.0, 5.0),
            ..Axis::default()
        },
        ..Axes::default()
    }
}

/// A 3D axes with the default view and manual limits on every axis.
pub fn axes_3d(id: u64) -> Axes {
    Axes {
        id: NodeId(id),
        projection: Projection::ThreeD {
            view3d: View3d::default(),
        },
        x: Axis {
            limits: manual(-1.0, 1.0),
            ..Axis::default()
        },
        y: Axis {
            limits: manual(-2.0, 2.0),
            ..Axis::default()
        },
        z: Axis {
            limits: manual(0.0, 3.0),
            ..Axis::default()
        },
        ..Axes::default()
    }
}

/// A figure of two axes and three artists, for the tests of the object tree and the
/// inspector.
///
/// The first axes (node 2) is two-dimensional, occupies the cell in row 0 and column 0,
/// is titled "Speed" and holds a line (node 4) named "Measured" and a hidden series of markers
/// (node 5) with no name. The second axes (node 3) is three-dimensional, occupies row 0
/// and column 1, has no title, and holds a surface (node 6) over a rectilinear grid.
/// Every artist refers to data of the shapes the IR requires, so the figure is valid.
pub fn figure_with_artists() -> Figure {
    use ironlab_ir::{Artist, Cell, Grid, NdArray, Series, Surface, Text, TileLayout};

    let (x, y) = (array_id(0), array_id(1));
    let (gx, gy, field) = (array_id(2), array_id(3), array_id(4));
    let data = data_arrays([
        (x, NdArray::vector(vec![1.0, 2.0, 3.0])),
        (y, NdArray::vector(vec![1.0, 4.0, 9.0])),
        (gx, NdArray::vector(vec![0.0, 1.0, 2.0])),
        (gy, NdArray::vector(vec![0.0, 1.0])),
        (
            field,
            NdArray::from_shape(vec![2, 3], vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0])
                .expect("the shape matches the values"),
        ),
    ]);
    let flat = Axes {
        id: NodeId(2),
        title: Some(Text::plain("Speed")),
        artists: vec![
            Artist::Series(Series {
                id: NodeId(4),
                label: Some(Text::plain("Measured")),
                x,
                y,
                ..Series::default()
            }),
            Artist::Series(Series {
                id: NodeId(5),
                visible: false,
                x,
                y,
                ..markers()
            }),
        ],
        ..Axes::default()
    };
    let solid = Axes {
        cell: Cell {
            col: 1,
            ..Cell::default()
        },
        artists: vec![Artist::Surface(Surface {
            id: NodeId(6),
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: field,
            color_scale: Some(NodeId(7)),
            ..Surface::default()
        })],
        ..axes_3d(3)
    };
    Figure {
        id: NodeId(1),
        layout: TileLayout::new(1, 2),
        data,
        color_scales: vec![ironlab_ir::ColorScale {
            id: NodeId(7),
            ..ironlab_ir::ColorScale::default()
        }],
        axes: vec![flat, solid],
        ..Figure::new()
    }
}

pub fn figure_with(axes: Vec<Axes>, links: Vec<AxisLink>) -> Figure {
    Figure {
        id: NodeId(1),
        axes,
        links,
        ..Figure::new()
    }
}

pub fn link(dimension: Dimension, ids: &[u64]) -> AxisLink {
    AxisLink {
        dimension,
        axes: ids.iter().copied().map(NodeId).collect(),
    }
}

/// A horizontal axis map from `[min, max]` onto the horizontal extent of `rect`.
pub fn x_map(rect: Rect, min: f64, max: f64, log: bool) -> AxisMap {
    AxisMap {
        min,
        max,
        log,
        start: rect.x,
        end: rect.right(),
    }
}

/// A vertical axis map from `[min, max]` onto the vertical extent of `rect`, with `min` at the bottom.
pub fn y_map(rect: Rect, min: f64, max: f64, log: bool) -> AxisMap {
    AxisMap {
        min,
        max,
        log,
        start: rect.bottom(),
        end: rect.y,
    }
}

/// Hit geometry of a linear 2D axes whose data limits are x ∈ [0, 10] and y ∈ [0, 5], as for [`axes_2d`].
pub fn hit_2d(id: u64, rect: Rect) -> AxesHit {
    AxesHit {
        id: NodeId(id),
        plot_rect: rect,
        kind: AxesHitKind::TwoD {
            x: x_map(rect, 0.0, 10.0, false),
            y: y_map(rect, 0.0, 5.0, false),
        },
    }
}

pub fn limits_of(figure: &Figure, id: u64, dimension: Dimension) -> Limits {
    let axes = figure.axes(NodeId(id)).expect("axes exists");
    match dimension {
        Dimension::X => axes.x.limits,
        Dimension::Y => axes.y.limits,
        Dimension::Z => axes.z.limits,
    }
}

pub fn manual_of(figure: &Figure, id: u64, dimension: Dimension) -> (f64, f64) {
    match limits_of(figure, id, dimension) {
        Limits::Manual { min, max } => (min, max),
        Limits::Auto => panic!("expected manual limits on axes {id} {dimension:?}"),
    }
}

/// A property path, panicking with the text when it is not one.
pub fn path(text: &str) -> PropertyPath {
    text.parse().expect("a property path")
}

/// A transaction of one set of a property of a node.
pub fn set(node: u64, at: &str, value: Value) -> Transaction {
    Transaction {
        edits: vec![Edit::Set {
            node: NodeId(node),
            path: path(at),
            value,
        }],
    }
}

/// Records limits for an axes, and for the axes linked with it, as a gesture does.
///
/// Returns whether the displayed figure changed.
pub fn record_limits(
    state: &mut FigureState,
    id: u64,
    dimension: Dimension,
    limits: Limits,
) -> bool {
    let transaction = command::set_limits(state.figure(), NodeId(id), dimension, limits)
        .expect("the axes accepts the limits");
    state.record(&transaction)
}

pub fn assert_close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected}, got {actual} (tolerance {tolerance})"
    );
}

/// A figure that holds one artist of every kind, so that a test can exercise every
/// property the inspector can show.
///
/// A three-dimensional axes (node 2) holds a line (3), a series of markers (4) whose size and
/// colour come from data, a contour (5) with explicit levels, a quiver (6) with a scale
/// factor, a surface (7), a true-colour image (8) of 8-bit RGBA pixels on the xz wall, a
/// scalar image (9) of 8-bit indices on the yz wall and a scalar image (10) of the field on
/// the floor, the images with both pixel ranges set. The series of markers, the contour, the surface
/// and the image of the field map their numbers through a continuous colour scale (12)
/// that draws nothing below and above its limits and a fixed colour for a non-finite
/// value, and the image of indices through an indexed colour scale (13) of nine entries
/// that clamps an index outside them, so that the scales hold a fixed colour, a clamp and a
/// transparent policy between them. Every artist is three-dimensional or planar, so the
/// figure is valid. The axes lies in the first of two columns, and a legend panel (11) in
/// the second, so that the figure holds a node of every kind.
pub fn figure_with_every_artist() -> Figure {
    use ironlab_ir::ContourPlacement;
    use ironlab_ir::{
        Artist, Color, ColorScale, ColorScaleKind, ColorScaleOutOfRange, ColorSpec,
        ContinuousColorScale, Contour, Grid, Image, ImagePlacement, ImagePlane, IndexedColorScale,
        Levels, LineStyle, NdArray, OutOfRange, PixelRange, Quiver, QuiverScale, ScalarImage,
        Series, Surface,
    };

    let ids: Vec<NodeId> = (0..10).map(array_id).collect();
    let (x, y, z, u, v, w) = (ids[0], ids[1], ids[2], ids[3], ids[4], ids[5]);
    let (gx, field, pixels, indices) = (ids[6], ids[7], ids[8], ids[9]);
    let vector = |start: f64| NdArray::vector(vec![start, start + 1.0, start + 2.0]);
    let data = data_arrays([
        (x, vector(1.0)),
        (y, vector(2.0)),
        (z, vector(3.0)),
        (u, vector(0.5)),
        (v, vector(1.5)),
        (w, vector(2.5)),
        (gx, vector(0.0)),
        (
            field,
            NdArray::from_shape(vec![3, 3], (0..9).map(f64::from).collect())
                .expect("the shape matches the values"),
        ),
        (
            pixels,
            NdArray::from_shape_u8(vec![2, 2, 4], (0..16).collect())
                .expect("the shape matches the values"),
        ),
        (
            indices,
            NdArray::from_shape_u8(vec![3, 3], (0..9).collect())
                .expect("the shape matches the values"),
        ),
    ]);
    let ranges = ImagePlacement {
        plane: ImagePlane::default(),
        columns: Some(PixelRange {
            first: -1.0,
            last: 1.0,
        }),
        rows: Some(PixelRange {
            first: 0.0,
            last: 3.0,
        }),
    };
    let artists = vec![
        Artist::Series(Series {
            id: NodeId(3),
            x,
            y,
            z: Some(z),
            ..Series::default()
        }),
        Artist::Series(Series {
            id: NodeId(4),
            x,
            y,
            z: Some(z),
            line: LineStyle {
                color: ColorSpec::Colormapped,
                ..markers().line
            },
            size_data: Some(u),
            color_data: Some(v),
            color_scale: Some(NodeId(12)),
            ..markers()
        }),
        Artist::Contour(Contour {
            id: NodeId(5),
            grid: Grid::Rectilinear { x: gx, y: gx },
            z: field,
            levels: Some(Levels::Explicit {
                values: vec![1.0, 4.0],
            }),
            placement: ContourPlacement::Plane { z: Some(0.0) },
            color_scale: Some(NodeId(12)),
            ..Contour::default()
        }),
        Artist::Quiver(Quiver {
            id: NodeId(6),
            x,
            y,
            z: Some(z),
            u,
            v,
            w: Some(w),
            scale: QuiverScale::Factor { value: 2.0 },
            ..Quiver::default()
        }),
        Artist::Surface(Surface {
            id: NodeId(7),
            grid: Grid::Rectilinear { x: gx, y: gx },
            z: field,
            c: Some(field),
            color_scale: Some(NodeId(12)),
            ..Surface::default()
        }),
        Artist::Image(Image {
            id: NodeId(8),
            pixels,
            placement: ImagePlacement {
                plane: ImagePlane::Xz { y: Some(0.0) },
                ..ranges
            },
            ..Image::default()
        }),
        Artist::ScalarImage(ScalarImage {
            id: NodeId(9),
            values: indices,
            color_scale: Some(NodeId(13)),
            placement: ImagePlacement {
                plane: ImagePlane::Yz { x: Some(0.5) },
                ..ranges
            },
            ..ScalarImage::default()
        }),
        Artist::ScalarImage(ScalarImage {
            id: NodeId(10),
            values: field,
            color_scale: Some(NodeId(12)),
            placement: ImagePlacement {
                plane: ImagePlane::Xy { z: Some(1.0) },
                ..ranges
            },
            ..ScalarImage::default()
        }),
    ];
    let continuous = ColorScale {
        id: NodeId(12),
        kind: ColorScaleKind::Continuous(ContinuousColorScale {
            below: ColorScaleOutOfRange::new(OutOfRange::Transparent),
            above: ColorScaleOutOfRange::new(OutOfRange::Transparent),
            ..ContinuousColorScale::default()
        }),
        non_finite: ColorScaleOutOfRange::new(OutOfRange::Rgba {
            color: Color::BLACK,
        }),
        ..ColorScale::default()
    };
    let indexed = ColorScale {
        id: NodeId(13),
        kind: ColorScaleKind::Indexed(IndexedColorScale {
            colors: (0..9u8)
                .map(|k| Color::rgb(f32::from(k) / 8.0, 0.5, 1.0 - f32::from(k) / 8.0))
                .collect(),
            outside: ColorScaleOutOfRange::new(OutOfRange::Clamp),
            ..IndexedColorScale::default()
        }),
        ..ColorScale::default()
    };
    Figure {
        id: NodeId(1),
        layout: ironlab_ir::TileLayout::new(1, 2),
        data,
        color_scales: vec![continuous, indexed],
        axes: vec![Axes {
            artists,
            ..axes_3d(2)
        }],
        panels: vec![ironlab_ir::Panel {
            id: NodeId(11),
            cell: ironlab_ir::Cell {
                col: 1,
                ..ironlab_ir::Cell::default()
            },
            align: ironlab_ir::PanelLocation::Center,
            content: ironlab_ir::PanelContent::Legend(ironlab_ir::PanelLegend::default()),
        }],
        ..Figure::new()
    }
}

/// A figure of many axes and many artists, for the tests of how the property editor
/// divides its height when the object tree is long.
///
/// Six two-dimensional axes fill a layout of two rows and three columns. Each is titled,
/// each holds four artists — two lines and two series of markers, named so that a test can find
/// one of them — and all of them draw the same two data arrays, so that the figure is
/// valid and the object tree has thirty-one rows.
pub fn figure_with_many_axes() -> Figure {
    use ironlab_ir::{Artist, Cell, NdArray, Series, Text, TileLayout};

    let (x, y) = (array_id(0), array_id(1));
    let data = data_arrays([
        (x, NdArray::vector(vec![1.0, 2.0, 3.0])),
        (y, NdArray::vector(vec![1.0, 4.0, 9.0])),
    ]);
    let mut next: u64 = 2;
    let mut all = Vec::new();
    for index in 0u32..6 {
        let id = next;
        next += 1;
        let mut artists = Vec::new();
        for plot in 0u32..4 {
            let artist = NodeId(next);
            next += 1;
            let name = Text::plain(format!("Plot {index}.{plot}"));
            artists.push(if plot.is_multiple_of(2) {
                Artist::Series(Series {
                    id: artist,
                    label: Some(name),
                    x,
                    y,
                    ..Series::default()
                })
            } else {
                Artist::Series(Series {
                    id: artist,
                    label: Some(name),
                    x,
                    y,
                    ..markers()
                })
            });
        }
        all.push(Axes {
            title: Some(Text::plain(format!("Axes {index}"))),
            cell: Cell {
                row: index / 3,
                col: index % 3,
                ..Cell::default()
            },
            artists,
            ..axes_2d(id)
        });
    }
    Figure {
        id: NodeId(1),
        layout: TileLayout::new(2, 3),
        data,
        axes: all,
        ..Figure::new()
    }
}

/// A figure of one two-dimensional axes (node 2) holding a scalar image (node 3) of `ny`
/// rows and `nx` columns whose values are 0, 1, …, ny · nx − 1 in row order, mapped through a
/// continuous colour scale of viridis (node 4) between automatic limits and placed by
/// default, so that the image fills the axes and neighbouring pixels differ in colour along
/// both axes: by one step of the colormap along a row and by `nx` steps down a column.
pub fn figure_with_scalar_image(ny: usize, nx: usize) -> Figure {
    use ironlab_ir::{Artist, ColorScale, NdArray, ScalarImage};

    let values: Vec<f64> = (0..ny * nx).map(|k| k as f64).collect();
    let data = data_arrays([(
        array_id(0),
        NdArray::from_shape(vec![ny, nx], values).expect("the shape matches the values"),
    )]);
    Figure {
        id: NodeId(1),
        data,
        axes: vec![Axes {
            id: NodeId(2),
            artists: vec![Artist::ScalarImage(ScalarImage {
                id: NodeId(3),
                values: array_id(0),
                color_scale: Some(NodeId(4)),
                ..ScalarImage::default()
            })],
            ..Axes::default()
        }],
        color_scales: vec![ColorScale {
            id: NodeId(4),
            ..ColorScale::default()
        }],
        ..Figure::new()
    }
}

/// The scalar image of [`figure_with_scalar_image`], `ny` × `nx`, named "Temperature", so that a callout
/// over it names the image, and so that any position inside the plot rectangle lies over a pixel with no drawn
/// point anywhere near it.
pub fn figure_with_named_image(ny: usize, nx: usize) -> Figure {
    use ironlab_ir::{Artist, Text};

    let mut figure = figure_with_scalar_image(ny, nx);
    match figure.artist_mut(NodeId(3)) {
        Some(Artist::ScalarImage(image)) => {
            image.label = Some(Text::plain("Temperature"));
        }
        other => panic!("node 3 is the scalar image, not {other:?}"),
    }
    figure
}

/// A figure of one axes (node 2) holding a series of markers (node 3) of `n` points spread over the whole plot, named
/// "Samples", so that any position inside the plot rectangle is close to a drawn marker and a callout over it
/// names the series.
pub fn figure_with_dense_scatter(n: usize) -> Figure {
    use ironlab_ir::{Artist, NdArray, Series, Text};

    let x: Vec<f64> = (0..n).map(|i| i as f64 / n as f64).collect();
    let y: Vec<f64> = (0..n)
        .map(|i| ((i as u64).wrapping_mul(2_654_435_761) % 10_000) as f64 / 10_000.0)
        .collect();
    let (xi, yi) = (array_id(0), array_id(1));
    Figure {
        id: NodeId(1),
        data: data_arrays([(xi, NdArray::vector(x)), (yi, NdArray::vector(y))]),
        axes: vec![Axes {
            id: NodeId(2),
            artists: vec![Artist::Series(Series {
                id: NodeId(3),
                label: Some(Text::plain("Samples")),
                x: xi,
                y: yi,
                ..markers()
            })],
            ..Axes::default()
        }],
        ..Figure::new()
    }
}
