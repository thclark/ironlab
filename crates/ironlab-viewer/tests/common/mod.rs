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
