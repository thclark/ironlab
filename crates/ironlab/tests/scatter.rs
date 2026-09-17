//! Scatter plots: scatter and scatter3.

mod common;

use common::{data, is_3d, parent, scatter};
use ironlab::ir::{ColorSpec, MarkerShape, NdArray, Scatter, ScatterColor, ScatterSize};
use ironlab::prelude::*;

// WHY: scatter must store the positions and otherwise use the IR's scatter defaults
// (hollow circles in the automatic colour), which match MATLAB's scatter.
#[test]
fn scatter_stores_positions_with_default_style() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .scatter([1.0, 2.0, 3.0], [4.0, 5.0, 6.0])
        .id();
    let s = scatter(&fig, id);
    assert_eq!(data(&fig, s.x), &NdArray::vector(vec![1.0, 2.0, 3.0]));
    assert_eq!(data(&fig, s.y), &NdArray::vector(vec![4.0, 5.0, 6.0]));
    assert_eq!(s.z, None);
    let defaults = Scatter::default();
    assert_eq!(s.size, defaults.size);
    assert_eq!(
        s.color,
        ScatterColor::Spec {
            spec: ColorSpec::Auto
        }
    );
    assert_eq!(s.marker, defaults.marker);
    assert_eq!(s.marker.shape, MarkerShape::Circle);
    // Unfilled by default, which is what makes `filled` observable.
    assert_eq!(s.marker.face, ColorSpec::None);
}

// WHY: per-point sizes and colours are what distinguish scatter from plot; the data
// must be copied into the table and referenced, with colour values left for the
// colormap rather than converted to colours at build time.
#[test]
fn per_point_sizes_and_colours_are_stored_as_data() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .scatter([0.0, 1.0, 2.0], [0.0, 1.0, 2.0])
        .sizes([2.0, 4.0, 8.0])
        .colors([-1.0, 0.0, 1.0])
        .id();
    let s = scatter(&fig, id);
    match s.size {
        ScatterSize::Data { data: d } => {
            assert_eq!(data(&fig, d), &NdArray::vector(vec![2.0, 4.0, 8.0]));
        }
        other => panic!("expected per-point sizes, found {other:?}"),
    }
    match s.color {
        ScatterColor::Data { data: d } => {
            assert_eq!(data(&fig, d), &NdArray::vector(vec![-1.0, 0.0, 1.0]));
        }
        other => panic!("expected per-point colours, found {other:?}"),
    }
}

// WHY: the scalar forms must replace the per-point forms (and vice versa), so the last
// call wins as with MATLAB properties.
#[test]
fn scalar_size_and_colour_replace_per_point_data() {
    let green = Color::rgb(0.0, 0.6, 0.5);
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .scatter([0.0, 1.0], [0.0, 1.0])
        .sizes([1.0, 2.0])
        .size(9.0)
        .colors([0.0, 1.0])
        .color(green)
        .id();
    let s = scatter(&fig, id);
    assert_eq!(s.size, ScatterSize::Scalar { value: 9.0 });
    assert_eq!(
        s.color,
        ScatterColor::Spec {
            spec: ColorSpec::Rgba { color: green }
        }
    );
}

// WHY: marker and filled map to the marker style; filled must make the face follow the
// scatter colour (automatic), not a fixed colour, so colour-mapped scatters fill with
// their per-point colours.
#[test]
fn marker_and_filled_set_the_marker_style() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .scatter([0.0], [0.0])
        .marker(Marker::Square)
        .filled()
        .display_name("samples")
        .id();
    let s = scatter(&fig, id);
    assert_eq!(s.marker.shape, MarkerShape::Square);
    assert_eq!(s.marker.face, ColorSpec::Auto);
    assert_eq!(s.display_name, Some(Text::new("samples")));
}

// WHY: scatter3 must store z and promote the axes to 3D, like the other 3D functions.
#[test]
fn scatter3_stores_z_and_promotes_the_axes_to_3d() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .scatter3([0.0, 1.0], [2.0, 3.0], [4.0, 5.0])
        .id();
    let s = scatter(&fig, id);
    let z = s.z.expect("scatter3 stores z");
    assert_eq!(data(&fig, z), &NdArray::vector(vec![4.0, 5.0]));
    assert!(is_3d(parent(&fig, id)));
    assert!(fig.validate().is_valid());
}
