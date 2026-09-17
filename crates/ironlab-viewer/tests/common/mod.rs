//! Shared fixtures for the viewer integration tests.
//!
//! Hit maps are built by hand rather than by compiling the figure, so that the interaction tests exercise only the
//! interaction logic and state their geometry explicitly.

#![allow(dead_code)]

use std::sync::{Arc, LazyLock};

use ironlab_ir::{
    Axes, Axis, AxisLink, Dimension, Edit, Figure, Limits, NodeId, Projection, PropertyPath,
    Transaction, Value, View3d, command,
};
use ironlab_scene::display::Rect;
use ironlab_scene::hit::{AxesHit, AxesHitKind, AxisMap};
use ironlab_text::TextEngine;
use ironlab_viewer::FigureState;

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
/// is titled "Speed" and holds a line (node 4) named "Measured" and a hidden scatter
/// (node 5) with no name. The second axes (node 3) is three-dimensional, occupies row 0
/// and column 1, has no title, and holds a surface (node 6) over a rectilinear grid.
/// Every artist refers to data of the shapes the IR requires, so the figure is valid.
pub fn figure_with_artists() -> Figure {
    use ironlab_ir::{
        Artist, Cell, DataId, Grid, Line, NdArray, Scatter, Surface, Text, TileLayout,
    };

    let (x, y) = (DataId(0), DataId(1));
    let (gx, gy, field) = (DataId(2), DataId(3), DataId(4));
    let data = std::collections::BTreeMap::from([
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
            Artist::Line(Line {
                id: NodeId(4),
                display_name: Some(Text::plain("Measured")),
                x,
                y,
                ..Line::default()
            }),
            Artist::Scatter(Scatter {
                id: NodeId(5),
                visible: false,
                x,
                y,
                ..Scatter::default()
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
            ..Surface::default()
        })],
        ..axes_3d(3)
    };
    Figure {
        id: NodeId(1),
        layout: TileLayout { rows: 1, cols: 2 },
        data,
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

pub fn hit_3d(id: u64, rect: Rect) -> AxesHit {
    AxesHit {
        id: NodeId(id),
        plot_rect: rect,
        kind: AxesHitKind::ThreeD,
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

pub fn view_of(figure: &Figure, id: u64) -> View3d {
    match figure.axes(NodeId(id)).expect("axes exists").projection {
        Projection::ThreeD { view3d } => view3d,
        Projection::TwoD => panic!("axes {id} is not 3D"),
    }
}

pub fn set_view(figure: &mut Figure, id: u64, view: View3d) {
    figure.axes_mut(NodeId(id)).expect("axes exists").projection =
        Projection::ThreeD { view3d: view };
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
/// A three-dimensional axes (node 2) holds a line (3), a scatter (4) whose size and
/// colour come from data, a contour (5) with explicit levels, a quiver (6) with a scale
/// factor and a surface (7). Every artist is three-dimensional, so the figure is valid.
pub fn figure_with_every_artist() -> Figure {
    use ironlab_ir::{
        Artist, Contour, Grid, Levels, Line, NdArray, Quiver, QuiverScale, Scatter, ScatterColor,
        ScatterSize, Surface,
    };
    use ironlab_ir::{ContourPlacement, DataId};

    let ids: Vec<DataId> = (0..8).map(DataId).collect();
    let (x, y, z, u, v, w) = (ids[0], ids[1], ids[2], ids[3], ids[4], ids[5]);
    let (gx, field) = (ids[6], ids[7]);
    let vector = |start: f64| NdArray::vector(vec![start, start + 1.0, start + 2.0]);
    let data = std::collections::BTreeMap::from([
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
    ]);
    let artists = vec![
        Artist::Line(Line {
            id: NodeId(3),
            x,
            y,
            z: Some(z),
            ..Line::default()
        }),
        Artist::Scatter(Scatter {
            id: NodeId(4),
            x,
            y,
            z: Some(z),
            size: ScatterSize::Data { data: u },
            color: ScatterColor::Data { data: v },
            ..Scatter::default()
        }),
        Artist::Contour(Contour {
            id: NodeId(5),
            grid: Grid::Rectilinear { x: gx, y: gx },
            z: field,
            levels: Levels::Explicit {
                values: vec![1.0, 4.0],
            },
            placement: ContourPlacement::Plane { z: Some(0.0) },
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
            ..Surface::default()
        }),
    ];
    Figure {
        id: NodeId(1),
        data,
        axes: vec![Axes {
            artists,
            ..axes_3d(2)
        }],
        ..Figure::new()
    }
}
