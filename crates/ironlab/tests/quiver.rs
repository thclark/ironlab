//! Vector field plots: quiver and quiver3.

mod common;

use common::{data, has_error_at, is_3d, parent, quiver};
use ironlab::ir::{ColorSpec, IssueKind, NdArray, Quiver, QuiverScale};
use ironlab::prelude::*;

// WHY: quiver takes four arrays in MATLAB order (x, y, u, v); swapping positions and
// components would draw a wrong but plausible field, so each must reach its own field.
#[test]
fn quiver_maps_positions_and_components_in_matlab_order() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .quiver([1.0, 2.0], [3.0, 4.0], [5.0, 6.0], [7.0, 8.0])
        .id();
    let q = quiver(&fig, id);
    assert_eq!(data(&fig, q.x), &NdArray::vector(vec![1.0, 2.0]));
    assert_eq!(data(&fig, q.y), &NdArray::vector(vec![3.0, 4.0]));
    assert_eq!(data(&fig, q.u), &NdArray::vector(vec![5.0, 6.0]));
    assert_eq!(data(&fig, q.v), &NdArray::vector(vec![7.0, 8.0]));
    assert_eq!((q.z, q.w), (None, None));
    assert!(!is_3d(parent(&fig, id)));
}

// WHY: the defaults (automatic scale, automatic colour, IR head size) are what MATLAB
// users expect from a bare quiver call.
#[test]
fn quiver_uses_ir_defaults() {
    let mut fig = Figure::new();
    let id = fig.axes(0, 0).quiver([0.0], [0.0], [1.0], [1.0]).id();
    let q = quiver(&fig, id);
    let defaults = Quiver::default();
    assert_eq!(q.scale, QuiverScale::Auto);
    assert_eq!(q.line, defaults.line);
    assert_eq!(q.line.color, ColorSpec::Auto);
    assert_eq!(q.head_size, defaults.head_size);
}

// WHY: quiver3 has six arrays in MATLAB order (x, y, z, u, v, w) and must promote the
// axes to 3D.
#[test]
fn quiver3_maps_all_six_arrays_and_promotes_the_axes() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .quiver3([1.0], [2.0], [3.0], [4.0], [5.0], [6.0])
        .id();
    let q = quiver(&fig, id);
    assert_eq!(data(&fig, q.x).values, vec![1.0]);
    assert_eq!(data(&fig, q.y).values, vec![2.0]);
    assert_eq!(data(&fig, q.z.expect("quiver3 stores z")).values, vec![3.0]);
    assert_eq!(data(&fig, q.u).values, vec![4.0]);
    assert_eq!(data(&fig, q.v).values, vec![5.0]);
    assert_eq!(data(&fig, q.w.expect("quiver3 stores w")).values, vec![6.0]);
    assert!(is_3d(parent(&fig, id)));
    assert!(fig.validate().is_valid());
}

// WHY: scale(f) and no_scale() correspond to MATLAB's quiver(..., scale) and
// quiver(..., 'off'), which change arrow lengths by orders of magnitude.
#[test]
fn scale_options_map_to_quiver_scale() {
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let factor = ax.quiver([0.0], [0.0], [1.0], [0.0]).scale(0.5).id();
    let off = ax.quiver([0.0], [0.0], [1.0], [0.0]).no_scale().id();
    assert_eq!(
        quiver(&fig, factor).scale,
        QuiverScale::Factor { value: 0.5 }
    );
    assert_eq!(quiver(&fig, off).scale, QuiverScale::Off);
}

// WHY: the style setters map to the arrow style used by the renderer.
#[test]
fn quiver_setters_map_to_ir_fields() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .quiver([0.0], [0.0], [1.0], [0.0])
        .color(Color::BLACK)
        .line_width(0.4)
        .head_size(0.2)
        .display_name("$\\nabla f$")
        .id();
    let q = quiver(&fig, id);
    assert_eq!(
        q.line.color,
        ColorSpec::Rgba {
            color: Color::BLACK
        }
    );
    assert_eq!(q.line.width_pt, 0.4);
    assert_eq!(q.head_size, 0.2);
    assert_eq!(q.display_name, Some(Text::new("$\\nabla f$")));
}

// WHY: component arrays shorter than the positions must be reported, not panic.
#[test]
fn mismatched_quiver_lengths_are_reported_by_validate() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .quiver([0.0, 1.0], [0.0, 1.0], [1.0], [1.0, 2.0])
        .id();
    assert!(has_error_at(&fig.validate(), IssueKind::ShapeMismatch, id));
}
