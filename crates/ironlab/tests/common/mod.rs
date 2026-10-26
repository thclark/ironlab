//! Helpers that look up the IR produced by the facade, so that tests assert on the
//! figure description rather than on rendering.

#![allow(dead_code)]

use ironlab::ir::{self, Artist, Axes, IssueKind, NdArray, NodeId, ValidationReport};
use ironlab::{
    ByteMatrix, Color, Colormap, ColormapSample, Figure, ImagePlane, Matrix, OutOfRange, Pixels,
};

/// Returns the axes with the given identifier, panicking if it does not exist.
pub fn axes(fig: &Figure, id: NodeId) -> &Axes {
    fig.ir()
        .axes(id)
        .unwrap_or_else(|| panic!("{id} is not an axes"))
}

/// Returns the artist with the given identifier, panicking if it does not exist.
pub fn artist(fig: &Figure, id: NodeId) -> &Artist {
    fig.ir()
        .artist(id)
        .map(|(_, artist)| artist)
        .unwrap_or_else(|| panic!("{id} is not an artist"))
}

/// Returns the axes that contains the given artist.
pub fn parent(fig: &Figure, id: NodeId) -> &Axes {
    fig.ir()
        .artist(id)
        .map(|(axes, _)| axes)
        .unwrap_or_else(|| panic!("{id} is not an artist"))
}

pub fn series(fig: &Figure, id: NodeId) -> &ir::Series {
    match artist(fig, id) {
        Artist::Series(a) => a,
        other => panic!("expected a series, found {other:?}"),
    }
}

pub fn contour(fig: &Figure, id: NodeId) -> &ir::Contour {
    match artist(fig, id) {
        Artist::Contour(a) => a,
        other => panic!("expected a contour, found {other:?}"),
    }
}

pub fn quiver(fig: &Figure, id: NodeId) -> &ir::Quiver {
    match artist(fig, id) {
        Artist::Quiver(a) => a,
        other => panic!("expected a quiver, found {other:?}"),
    }
}

pub fn surface(fig: &Figure, id: NodeId) -> &ir::Surface {
    match artist(fig, id) {
        Artist::Surface(a) => a,
        other => panic!("expected a surface, found {other:?}"),
    }
}

pub fn image(fig: &Figure, id: NodeId) -> &ir::Image {
    match artist(fig, id) {
        Artist::Image(a) => a,
        other => panic!("expected an image, found {other:?}"),
    }
}

pub fn scalar_image(fig: &Figure, id: NodeId) -> &ir::ScalarImage {
    match artist(fig, id) {
        Artist::ScalarImage(a) => a,
        other => panic!("expected a scalar image, found {other:?}"),
    }
}

pub fn bar(fig: &Figure, id: NodeId) -> &ir::Bar {
    match artist(fig, id) {
        Artist::Bar(a) => a,
        other => panic!("expected a bar series, found {other:?}"),
    }
}

pub fn stairs(fig: &Figure, id: NodeId) -> &ir::Stairs {
    match artist(fig, id) {
        Artist::Stairs(a) => a,
        other => panic!("expected stairs, found {other:?}"),
    }
}

/// Returns true when the report has a warning of the given kind.
pub fn has_warning(report: &ValidationReport, kind: IssueKind) -> bool {
    report.warnings.iter().any(|issue| issue.kind == kind)
}

/// Returns the array of the data array with the given identifier, panicking if it does
/// not exist.
pub fn data(fig: &Figure, id: NodeId) -> &NdArray {
    &fig.ir()
        .data_array(id)
        .unwrap_or_else(|| panic!("{id} is not a data array"))
        .array
}

/// Returns true when the axes is three-dimensional.
pub fn is_3d(axes: &Axes) -> bool {
    matches!(axes.projection, ir::Projection::ThreeD { .. })
}

/// Returns true when the report has an error of the given kind.
pub fn has_error(report: &ValidationReport, kind: IssueKind) -> bool {
    report.errors.iter().any(|issue| issue.kind == kind)
}

/// Returns true when the report has an error of the given kind at the given node.
pub fn has_error_at(report: &ValidationReport, kind: IssueKind, node: NodeId) -> bool {
    report
        .errors
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

/// A 3 by 4 field (3 rows of y, 4 columns of x) with distinct values, and matching
/// coordinate vectors.
pub fn small_grid() -> (Vec<f64>, Vec<f64>, Matrix) {
    let x = vec![0.0, 1.0, 2.0, 3.0];
    let y = vec![10.0, 20.0, 30.0];
    let z = Matrix::from_fn(3, 4, |row, col| (row * 10 + col) as f64);
    (x, y, z)
}

/// A valid figure holding one image of each kind, built through the facade: a 2 by 2
/// true-colour image of RGBA bytes with mirrored rows and a label; a 2 by 3 scalar image
/// of bytes with explicit column centres, through an indexed scale of six entries whose
/// out-of-range policy is a clamp; and a 3 by 2 scalar image of floats holding a NaN, on
/// the xz wall of a three-dimensional axes at an offset, through the axes' default scale
/// with manual limits and a fixed colour for non-finite values.
pub fn image_figure() -> Figure {
    let pixels = Pixels::from_rgba8(
        2,
        2,
        &[255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 1, 2, 3, 4],
    )
    .unwrap();
    let indices = ByteMatrix::from_fn(2, 3, |row, col| (row * 3 + col) as u8);
    let values = Matrix::from_fn(3, 2, |row, col| {
        if (row, col) == (1, 1) {
            f64::NAN
        } else {
            (row * 2 + col) as f64 / 5.0
        }
    });
    let mut fig = Figure::new().tiles(1, 3).title("Images");
    fig.axes(0, 0)
        .image(&pixels)
        .pixel_columns(-1.0, 1.0)
        .pixel_rows(1.0, -1.0)
        .label("photo");
    let palette = fig
        .indexed_scale(Colormap::Viridis.sample(6))
        .outside(OutOfRange::Clamp)
        .id();
    fig.axes(0, 1)
        .scalar_image(&indices)
        .pixel_columns(0.5, 2.5)
        .color_scale(palette);
    fig.axes(0, 2)
        .scalar_image(&values)
        .plane(ImagePlane::Xz { y: Some(0.5) });
    fig.axes(0, 2)
        .color_scale()
        .limits(0.0, 1.0)
        // A multiple of 1/255, so that the eight-bit colour of the JSON form reloads exactly.
        .non_finite(Color::rgba(0.0, 0.0, 0.0, 128.0 / 255.0));
    fig
}

/// Compiles the figure into a scene, as every front end does before drawing it.
pub fn compile(fig: &Figure) -> ironlab_scene::Scene {
    ironlab_scene::compile(fig.ir(), &ironlab_text::TextEngine::new())
}

/// The filled paths of a visible colour that the scene compiler drew for the artist, as the fill colour (to 8 bits)
/// and the number of subpaths of each, in paint order.
pub fn filled_paths(fig: &Figure, id: NodeId) -> Vec<([u8; 4], usize)> {
    use ironlab_scene::display::{ItemKind, PathSegment};
    let scene = compile(fig);
    let mut out = Vec::new();
    scene.display_list.visit_leaves(|item, _, _| {
        if item.source != Some(id) {
            return;
        }
        if let ItemKind::Path(path) = &item.kind
            && let Some(fill) = path.fill.filter(|f| f.color.a > 0.0)
        {
            let c = fill.color;
            let colour = [c.r, c.g, c.b, c.a].map(|v| (v * 255.0).round() as u8);
            let subpaths = path
                .segments
                .iter()
                .filter(|s| matches!(s, PathSegment::MoveTo(_)))
                .count();
            out.push((colour, subpaths));
        }
    });
    out
}

/// The number of filled shapes of a visible colour that the scene compiler drew for the artist: one per subpath of
/// each filled path, so that the count does not depend on whether shapes of one colour share a path.
pub fn filled_shapes(fig: &Figure, id: NodeId) -> usize {
    filled_paths(fig, id).iter().map(|(_, n)| n).sum()
}

/// The range of values, `(min, max)`, over which the colourbar of the given colour scale is drawn in the compiled
/// figure, which for automatic limits is the range the scene compiler resolved from the artists.
pub fn colorbar_span(fig: &Figure, scale: NodeId) -> (f64, f64) {
    let scene = compile(fig);
    let bar = scene
        .hit_map
        .colorbars
        .iter()
        .find(|bar| bar.scale == scale)
        .unwrap_or_else(|| panic!("no colourbar of {scale} is drawn"));
    let map = bar
        .map
        .as_ref()
        .expect("a continuous scale's bar maps values");
    (map.min, map.max)
}

/// The distinct colours (to 8 bits) of the visible strokes that the scene compiler drew for the artist.
pub fn stroke_colours(fig: &Figure, id: NodeId) -> std::collections::BTreeSet<[u8; 4]> {
    use ironlab_scene::display::ItemKind;
    let scene = compile(fig);
    let mut out = std::collections::BTreeSet::new();
    scene.display_list.visit_leaves(|item, _, _| {
        if item.source != Some(id) {
            return;
        }
        if let ItemKind::Path(path) = &item.kind
            && let Some(stroke) = path.stroke.as_ref().filter(|s| s.color.a > 0.0)
        {
            let c = stroke.color;
            out.insert([c.r, c.g, c.b, c.a].map(|v| (v * 255.0).round() as u8));
        }
    });
    out
}

/// Where one bar of a series lies, in data units, as the model defines it (ADR 0026): from `low` to `high` along the
/// category axis, and from `base` to `end` (its base plus its value) along the value axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarExtent {
    pub low: f64,
    pub high: f64,
    pub base: f64,
    pub end: f64,
}

/// Returns where each bar of the bar series lies, resolving its width (a fraction of the pitch, a constant or one per
/// bar), its offset (in widths or in data units) and its base (a constant or one per bar) as the `Bar` artist
/// documents them, so that a test of the geometry of bars does not depend on which of the equivalent forms the facade
/// chose to store.
pub fn bar_extents(fig: &Figure, id: NodeId) -> Vec<BarExtent> {
    use ir::{BarOffset, BarWidth, PerBar};
    let vector = |id: NodeId| -> Vec<f64> {
        data(fig, id)
            .as_f64()
            .expect("the arrays of bars are floating-point")
            .to_vec()
    };
    let b = bar(fig, id);
    let positions = vector(b.positions);
    let values = vector(b.values);
    let n = positions.len();
    let widths = match b.width {
        BarWidth::Pitch(fraction) => {
            let mut distinct: Vec<f64> = positions
                .iter()
                .copied()
                .filter(|p| p.is_finite())
                .collect();
            distinct.sort_by(f64::total_cmp);
            distinct.dedup();
            let pitch = distinct
                .windows(2)
                .map(|w| w[1] - w[0])
                .fold(f64::INFINITY, f64::min);
            let pitch = if pitch.is_finite() { pitch } else { 1.0 };
            vec![fraction * pitch; n]
        }
        BarWidth::Constant(width) => vec![width; n],
        BarWidth::Data(widths) => vector(widths),
    };
    let bases = match b.base {
        PerBar::Constant(base) => vec![base; n],
        PerBar::Data(bases) => vector(bases),
    };
    (0..n)
        .map(|k| {
            let offset = match b.offset {
                BarOffset::Widths(multiple) => multiple * widths[k],
                BarOffset::Absolute(distance) => distance,
            };
            let centre = positions[k] + offset;
            BarExtent {
                low: centre - widths[k] / 2.0,
                high: centre + widths[k] / 2.0,
                base: bases[k],
                end: bases[k] + values[k],
            }
        })
        .collect()
}
