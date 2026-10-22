//! Alpha and opacity: the transparency that surfaces, images and contours carry into the display list.
//!
//! An alpha array gives every node of a surface, or every pixel of a colour-indexed or colour-mapped image, its own
//! transparency, while an opacity is one number for the whole artist (or, for a surface, one for its faces and one
//! for their edges, and for a contour, one for its filled bands and one for its isolines). What reaches the display list is the product of the colour's own alpha, the alpha array and the
//! opacity, baked into the fill and stroke colours of a surface and into the samples of an image, so that every
//! backend draws it without knowing where it came from.

use ironlab_ir::{
    Color, ColorSpec, InsideLegend, Legend, LegendLocation, Levels, Limits, NodeId, OutOfRange,
    View3d,
};
use ironlab_scene::Scene;
use ironlab_scene::display::ImageItem;
use ironlab_scene::hit::LegendOwner;

use crate::common::{Fx, compile_figure, out_of_range, rgb8, text, xy};
use crate::probe::{Leaf, entry_artist, from_source, inside, leaves, pixel};

/// Columns and rows of the fixture surface: two faces side by side.
const X: [f64; 3] = [0.0, 1.0, 2.0];
const Y: [f64; 2] = [0.0, 1.0];

/// The face alpha of the fixture surface at its nodes, in row order. The left face has corners 0.2, 0.4, 0.8 and
/// 0.6, whose mean is 0.5; the right face has a NaN corner, so it is transparent.
const FACE_ALPHA: [f64; 6] = [0.2, 0.4, f64::NAN, 0.6, 0.8, 1.0];
/// The edge alpha of the fixture surface at its nodes, in row order. Clamped to `[0, 1]`, the left face has corners
/// 1, 1, 0 and 0, whose mean is 0.5, and the right face has corners 1, 1, 1 and 0, whose mean is 0.75: the values
/// are clamped before they are averaged, so the 2 and the −1 count as 1 and 0.
const EDGE_ALPHA: [f64; 6] = [2.0, 1.0, 1.0, -1.0, 0.0, 1.0];

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

/// A surface over the fixture grid, red faces with black edges, given the fixture alpha arrays and the opacities
/// `face` and `edge`, in a 2D or a 3D axes.
fn alpha_surface(three_d: bool, face: f64, edge: f64) -> (Scene, NodeId) {
    let mut fx = Fx::new();
    let ax = if three_d {
        fx.axes3d(0, 0, View3d::default())
    } else {
        fx.axes2d(0, 0)
    };
    let face_alpha = fx.matrix(2, 3, FACE_ALPHA.to_vec());
    let edge_alpha = fx.matrix(2, 3, EDGE_ALPHA.to_vec());
    let surface = fx.surface(
        ax,
        &X,
        &Y,
        |x, y| x + y,
        |s| {
            s.face = ColorSpec::Rgba { color: RED };
            s.face_alpha = Some(face_alpha);
            s.edge_alpha = Some(edge_alpha);
            s.face_opacity = face;
            s.edge_opacity = edge;
        },
    );
    (compile_figure(&fx.build()), surface)
}

#[track_caller]
fn assert_alpha(actual: f32, expected: f64, what: &str) {
    assert!(
        (f64::from(actual) - expected).abs() < 1e-6,
        "{what}: alpha {actual}, expected {expected}"
    );
}

/// Returns the alpha of every fill and every stroke of the leaves of `id`, each list sorted.
fn alphas(scene: &Scene, id: NodeId) -> (Vec<f32>, Vec<f32>) {
    let drawn = from_source(&leaves(scene), id);
    let mut fills: Vec<f32> = drawn
        .iter()
        .filter_map(Leaf::path)
        .filter_map(|p| p.fill.as_ref().map(|f| f.color.a))
        .collect();
    let mut strokes: Vec<f32> = drawn
        .iter()
        .filter_map(Leaf::path)
        .filter_map(|p| p.stroke.as_ref().map(|s| s.color.a))
        .collect();
    fills.sort_by(f32::total_cmp);
    strokes.sort_by(f32::total_cmp);
    (fills, strokes)
}

// Why: the issue asks for the faces and the edges of a surface to be controlled independently, and for the rendered
// transparency to be the alpha times the opacity. In a 2D axes a face is one path carrying its fill and its edge, so
// the fill must take the face alpha (the mean of its corners) times the face opacity and the edge the edge alpha
// times the edge opacity, a face with a NaN corner of face alpha must lose its fill but keep its edge, and alpha
// outside [0, 1] must be clamped before it is averaged.
#[test]
fn a_2d_surface_takes_face_and_edge_alpha_times_their_opacities_independently() {
    let (scene, surface) = alpha_surface(false, 0.5, 0.8);
    let drawn = from_source(&leaves(&scene), surface);
    assert_eq!(drawn.len(), 2, "both faces are drawn: {drawn:?}");
    let (fills, strokes) = alphas(&scene, surface);
    assert_eq!(fills.len(), 1, "the face with a NaN corner has no fill");
    assert_alpha(fills[0], 0.5 * 0.5, "the left face fill");
    let fill = drawn
        .iter()
        .filter_map(Leaf::path)
        .find_map(|p| p.fill.as_ref())
        .unwrap();
    assert_eq!(
        rgb8(fill.color),
        [255, 0, 0],
        "the colour itself is unchanged"
    );
    assert_eq!(strokes.len(), 2, "both faces keep their edges");
    assert_alpha(strokes[0], 0.5 * 0.8, "the left face edge");
    assert_alpha(strokes[1], 0.75 * 0.8, "the right face edge");
}

// Why: in a 3D axes the fill of a face and its edge are separate items, the edge being a ring filled with the edge
// colour so that it never spills over its neighbours; the ring must therefore carry the edge transparency in its
// fill, and the face its own, or a translucent surface would show opaque edges (or opaque faces) in 3D alone.
#[test]
fn a_3d_surface_carries_face_and_edge_transparency_into_its_faces_and_edge_rings() {
    let (scene, surface) = alpha_surface(true, 0.5, 0.8);
    let drawn = from_source(&leaves(&scene), surface);
    let rings: Vec<f32> = drawn
        .iter()
        .filter(|l| l.subpaths().len() == 2)
        .filter_map(|l| l.path().and_then(|p| p.fill.as_ref()).map(|f| f.color.a))
        .collect();
    let faces: Vec<f32> = drawn
        .iter()
        .filter(|l| l.subpaths().len() == 1)
        .filter_map(|l| l.path().and_then(|p| p.fill.as_ref()).map(|f| f.color.a))
        .collect();
    assert_eq!(
        faces.len(),
        1,
        "the face with a NaN corner has no fill: {drawn:?}"
    );
    assert_alpha(faces[0], 0.5 * 0.5, "the left face fill");
    let mut rings = rings;
    rings.sort_by(f32::total_cmp);
    assert_eq!(
        rings.len(),
        2,
        "both faces keep their edge rings: {drawn:?}"
    );
    assert_alpha(rings[0], 0.5 * 0.8, "the left face edge ring");
    assert_alpha(rings[1], 0.75 * 0.8, "the right face edge ring");
}

// Why: an opacity of 0 is the cheap way to hide the faces or the edges of a surface (a wireframe from a surface, or
// faces without a mesh), so the part made fully transparent must not be emitted at all, rather than drawn invisibly
// at the cost of the geometry, while the other part is drawn as usual; with both at 0 nothing is drawn.
#[test]
fn an_opacity_of_zero_omits_the_faces_or_the_edges() {
    for three_d in [false, true] {
        let (scene, surface) = alpha_surface(three_d, 0.0, 1.0);
        let (fills, strokes) = alphas(&scene, surface);
        assert!(fills.iter().all(|&a| a != 0.0), "3D {three_d}: {fills:?}");
        let rings = from_source(&leaves(&scene), surface)
            .iter()
            .filter(|l| l.subpaths().len() == 2)
            .count();
        assert_eq!(
            strokes.len() + rings,
            2,
            "3D {three_d}: the edges of both faces are drawn"
        );
        if !three_d {
            assert!(fills.is_empty(), "no face is filled: {fills:?}");
        } else {
            assert_eq!(fills.len(), rings, "only the rings are filled");
        }

        let (scene, surface) = alpha_surface(three_d, 0.0, 0.0);
        assert!(
            from_source(&leaves(&scene), surface).is_empty(),
            "3D {three_d}: a surface with neither faces nor edges draws nothing"
        );
        assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    }
}

// Why: every figure made before alpha and opacity existed is opaque, and must be drawn exactly as before; a surface
// whose alpha is 1 everywhere and whose opacities are 1 is the same opaque surface, so it must compile to the same
// display list, bit for bit, which a multiplication that is not exact at 1 would break.
#[test]
fn alpha_of_one_and_opacity_of_one_leave_a_surface_exactly_as_it_was() {
    for three_d in [false, true] {
        let build = |with_alpha: bool| {
            let mut fx = Fx::new();
            let ax = if three_d {
                fx.axes3d(0, 0, View3d::default())
            } else {
                fx.axes2d(0, 0)
            };
            let ones = fx.matrix(2, 3, vec![1.0; 6]);
            fx.surface(
                ax,
                &X,
                &Y,
                |x, y| x * y,
                |s| {
                    s.edge = ColorSpec::Rgba {
                        color: Color::rgba(0.0, 0.0, 0.0, 0.3),
                    };
                    if with_alpha {
                        s.face_alpha = Some(ones);
                        s.edge_alpha = Some(ones);
                    }
                },
            );
            compile_figure(&fx.build()).display_list
        };
        assert_eq!(build(true), build(false), "3D {three_d}");
    }
}

/// The colour of the isolines of the fixture contour: blue, itself half transparent.
const HALF_BLUE: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 0.5,
};

/// A 2D contour of `x + y` over the fixture grid, at levels 1 and 2, filled or not, with isolines of the colour
/// `line` and the opacities `line_opacity` and `fill_opacity`.
fn opacity_contour(
    fill: bool,
    line: ColorSpec,
    line_opacity: f64,
    fill_opacity: f64,
) -> (Scene, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let contour = fx.contour(
        ax,
        &X,
        &Y,
        |x, y| x + y,
        |c| {
            c.levels = Some(Levels::Explicit {
                values: vec![1.0, 2.0],
            });
            c.fill = fill;
            c.line.color = line;
            c.line_opacity = line_opacity;
            c.fill_opacity = fill_opacity;
        },
    );
    (compile_figure(&fx.build()), contour)
}

// Why: a filled contour is often laid over an image or a surface, so its bands must be fadeable without fading the
// isolines that mark the levels, and the reverse: the band fills must take the colormap colour's alpha times the
// fill opacity alone, and the isolines their own colour's alpha times the line opacity alone. Isolines that take
// the colormap (those of an unfilled contour) must be faded by the line opacity too.
#[test]
fn a_contour_fades_its_bands_and_its_isolines_by_their_own_opacities() {
    let (scene, contour) = opacity_contour(true, ColorSpec::Rgba { color: HALF_BLUE }, 0.8, 0.4);
    let (fills, strokes) = alphas(&scene, contour);
    assert!(!fills.is_empty(), "the bands are drawn");
    for &a in &fills {
        assert_alpha(a, 0.4, "a band fill");
    }
    assert_eq!(strokes.len(), 2, "both isolines are drawn: {strokes:?}");
    for &a in &strokes {
        assert_alpha(a, 0.5 * 0.8, "an isoline");
    }

    let (scene, contour) = opacity_contour(false, ColorSpec::Colormapped, 0.25, 0.4);
    let (fills, strokes) = alphas(&scene, contour);
    assert!(fills.is_empty(), "an unfilled contour has no bands");
    assert_eq!(strokes.len(), 2, "both isolines are drawn: {strokes:?}");
    for &a in &strokes {
        assert_alpha(a, 0.25, "a colormapped isoline");
    }
}

// Why: an opacity of 0 is the cheap way to show only the bands or only the isolines of a filled contour, so the part
// made fully transparent must not be emitted at all, rather than drawn invisibly at the cost of the geometry, while
// the other part is drawn as usual; with both at 0 nothing is drawn, and nothing is warned about.
#[test]
fn an_opacity_of_zero_omits_the_bands_or_the_isolines() {
    let line = ColorSpec::Rgba { color: HALF_BLUE };
    let (scene, contour) = opacity_contour(true, line, 1.0, 0.0);
    let (fills, strokes) = alphas(&scene, contour);
    assert!(fills.is_empty(), "no band is drawn: {fills:?}");
    assert_eq!(strokes.len(), 2, "both isolines are drawn: {strokes:?}");

    let (scene, contour) = opacity_contour(true, line, 0.0, 1.0);
    let (fills, strokes) = alphas(&scene, contour);
    assert!(!fills.is_empty(), "the bands are drawn");
    assert!(strokes.is_empty(), "no isoline is drawn: {strokes:?}");

    let (scene, contour) = opacity_contour(true, line, 0.0, 0.0);
    assert!(
        from_source(&leaves(&scene), contour).is_empty(),
        "a contour with neither bands nor isolines draws nothing"
    );
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
}

// Why: every contour made before its opacities existed has opacities of 1, and must be drawn exactly as before: the
// bands exactly opaque and the isolines exactly their own colour's alpha. A multiplication that is not exact at 1
// would make every such contour faintly translucent, which a backend would then blend (or, in a PDF, rasterise)
// for nothing.
#[test]
fn opacities_of_one_leave_the_colours_of_a_contour_exactly_as_given() {
    let (scene, contour) = opacity_contour(true, ColorSpec::Rgba { color: HALF_BLUE }, 1.0, 1.0);
    let (fills, strokes) = alphas(&scene, contour);
    assert!(!fills.is_empty(), "the bands are drawn");
    assert!(fills.iter().all(|&a| a == 1.0), "bands: {fills:?}");
    assert_eq!(
        strokes,
        [0.5, 0.5],
        "the isolines keep the alpha of their colour"
    );
}

/// Compiles a figure holding one 2D axes, prepared by `add`, and returns the image item of the artist it adds.
fn compiled_image(add: impl FnOnce(&mut Fx, NodeId) -> NodeId) -> ImageItem {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let id = add(&mut fx, ax);
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let drawn = from_source(&leaves(&scene), id);
    assert_eq!(drawn.len(), 1, "an image is one leaf");
    drawn[0].image().expect("an image item").clone()
}

// Why: alpha data is how a measurement says where it can be trusted (a PIV confidence, a mask), so each pixel's
// alpha must multiply its colour's alpha and the opacity: a NaN alpha (no confidence at all) is transparent, an
// alpha outside [0, 1] is clamped, and the colour of a translucent pixel is left as the colormap gives it.
#[test]
fn a_mapped_image_multiplies_each_pixel_by_its_alpha_and_by_the_opacity() {
    let item = compiled_image(|fx, ax| {
        let alpha = fx.matrix(1, 4, vec![0.5, f64::NAN, 2.0, -1.0]);
        let id = fx.imagesc(
            ax,
            vec![1, 4],
            vec![1.0, 2.0, 3.0, 4.0],
            xy(None, None),
            |m| {
                m.alpha = Some(alpha);
                m.opacity = 0.5;
            },
        );
        fx.axes_scale_mut(ax).limits = Limits::Manual {
            min: 0.0,
            max: 10.0,
        };
        id
    });
    assert_eq!(item.channels, ImageItem::RGBA);
    assert_eq!(pixel(&item, 0, 0)[3], 64, "0.5 × 0.5 = 0.25, quantised");
    assert_eq!(
        pixel(&item, 0, 1),
        [0, 0, 0, 0],
        "a NaN alpha is transparent"
    );
    assert_eq!(pixel(&item, 0, 2)[3], 128, "an alpha of 2 counts as 1");
    assert_eq!(
        pixel(&item, 0, 3),
        [0, 0, 0, 0],
        "an alpha of −1 counts as 0"
    );
}

// Why: an image's arrays may hold bytes from 0 to 255, and its alpha may too, as a camera mask or an 8-bit alpha
// channel is delivered; a byte alpha must be read as a fraction of 255, not as a number to clamp to 1.
#[test]
fn an_indexed_image_reads_an_alpha_of_bytes_as_fractions_of_255() {
    let item = compiled_image(|fx, ax| {
        let alpha = fx.bytes(vec![1, 3], vec![51, 0, 255]);
        fx.indexed_image(ax, vec![1, 3], vec![0u8, 10, 255], xy(None, None), |i| {
            i.alpha = Some(alpha)
        })
    });
    assert_eq!(item.channels, ImageItem::RGBA);
    assert_eq!(pixel(&item, 0, 0)[3], 51);
    assert_eq!(pixel(&item, 0, 1)[3], 0);
    assert_eq!(pixel(&item, 0, 2)[3], 255);
}

// Why: the issue defines the transparency of an RGBA image as its alpha channel times the opacity, and an RGB image
// with an opacity below 1 as uniformly translucent: so an RGB image must gain an alpha channel when, and only when,
// its opacity is below 1, and an RGBA image's own alpha must be multiplied rather than replaced.
#[test]
fn a_true_colour_image_multiplies_its_alpha_channel_by_the_opacity() {
    let rgba = compiled_image(|fx, ax| {
        fx.image(
            ax,
            vec![1, 2, 4],
            vec![1.0, 0.0, 0.0, 0.5, 0.0, 1.0, 0.0, 1.0],
            xy(None, None),
            |i| i.opacity = 0.5,
        )
    });
    assert_eq!(pixel(&rgba, 0, 0), [255, 0, 0, 64], "0.5 × 0.5 = 0.25");
    assert_eq!(pixel(&rgba, 0, 1), [0, 255, 0, 128], "1 × 0.5");

    let rgb = compiled_image(|fx, ax| {
        fx.image(ax, vec![1, 1, 3], vec![10u8, 20, 30], xy(None, None), |i| {
            i.opacity = 0.5
        })
    });
    assert_eq!(
        rgb.channels,
        ImageItem::RGBA,
        "a translucent RGB image gains alpha"
    );
    assert_eq!(pixel(&rgb, 0, 0), [10, 20, 30, 128]);

    let opaque = compiled_image(|fx, ax| {
        fx.image(
            ax,
            vec![1, 1, 3],
            vec![10u8, 20, 30],
            xy(None, None),
            |_| {},
        )
    });
    assert_eq!(
        opaque.channels,
        ImageItem::RGB,
        "an opaque RGB image stays RGB"
    );
}

// Why: an out-of-range policy colour stands in for the colour of a pixel, so it is that pixel's colour as far as
// transparency goes: the pixel's alpha and the image's opacity must multiply the policy colour's own alpha, or a
// masked or faded image would show its out-of-range pixels at full strength.
#[test]
fn an_out_of_range_policy_colour_is_multiplied_by_the_pixel_alpha_and_the_opacity() {
    let item = compiled_image(|fx, ax| {
        let alpha = fx.matrix(1, 2, vec![0.5, 1.0]);
        let id = fx.imagesc(ax, vec![1, 2], vec![-5.0, 5.0], xy(None, None), |m| {
            m.alpha = Some(alpha);
            m.opacity = 0.5;
        });
        let scale = fx.axes_scale_mut(ax);
        scale.limits = Limits::Manual {
            min: 0.0,
            max: 10.0,
        };
        scale.below = out_of_range(OutOfRange::Rgba {
            color: Color::rgba(0.0, 0.0, 1.0, 0.8),
        });
        id
    });
    assert_eq!(pixel(&item, 0, 0), [0, 0, 255, 51], "0.8 × 0.5 × 0.5 = 0.2");
    assert_eq!(pixel(&item, 0, 1)[3], 128, "1 × 1 × 0.5");
}

// Why: an image without alpha data and with an opacity of 1 is every image made before alpha and opacity existed,
// and a backend uploads a three-channel image without blending; an alpha of 255 everywhere and an opacity of 1 must
// leave the samples and their channel count exactly as they were.
#[test]
fn alpha_of_one_and_opacity_of_one_leave_an_image_exactly_as_it_was() {
    let build = |with_alpha: bool| {
        compiled_image(|fx, ax| {
            let alpha = fx.bytes(vec![2, 2], vec![255; 4]);
            fx.imagesc(
                ax,
                vec![2, 2],
                vec![1.0, 2.0, 3.0, 4.0],
                xy(None, None),
                |m| {
                    if with_alpha {
                        m.alpha = Some(alpha);
                    }
                },
            )
        })
    };
    let (with, without) = (build(true), build(false));
    assert_eq!(with.channels, ImageItem::RGB);
    assert_eq!(with, without);
}

// Why: the legend key of an artist should look like the artist, so a translucent surface or image is keyed by a
// translucent patch; the alpha arrays vary over the artist and have no single value to show, so only the opacity
// carries into the key.
#[test]
fn the_legend_patch_of_a_surface_or_image_takes_its_opacity() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let surface = fx.surface(
        ax,
        &X,
        &Y,
        |x, y| x + y,
        |s| {
            s.label = text("surface");
            s.face = ColorSpec::Rgba { color: RED };
            s.face_opacity = 0.4;
        },
    );
    let image = fx.imagesc(ax, vec![1, 2], vec![1.0, 2.0], xy(None, None), |m| {
        m.label = text("image");
        m.opacity = 0.25;
    });
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend {
        location: LegendLocation::NorthEast,
        boxed: true,
        ..InsideLegend::default()
    }));
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    for (artist, expected) in [(surface, 0.4), (image, 0.25)] {
        let entry = scene
            .hit_map
            .legend_entries
            .iter()
            .find(|e| e.legend == LegendOwner::Axes(ax) && entry_artist(e) == artist)
            .expect("the artist has a legend entry");
        let fills: Vec<f32> = leaves
            .iter()
            .filter(|l| {
                l.source == Some(ax) && l.bbox().is_some_and(|b| inside(b, entry.rect, 0.5))
            })
            .filter_map(|l| l.path().and_then(|p| p.fill.as_ref()).map(|f| f.color.a))
            .collect();
        assert_eq!(fills.len(), 1, "one patch keys {artist}: {fills:?}");
        assert_alpha(fills[0], expected, "the legend patch");
    }
}

// Why: the legend key of a contour should look like the contour, so a contour whose bands or isolines are faded is
// keyed by a sample faded the same way: the patch of a filled contour takes the fill opacity in its fill and the
// line opacity in its edge, each independently, and the line of an unfilled contour takes the line opacity.
#[test]
fn the_legend_sample_of_a_contour_takes_its_opacities() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let filled = fx.contour(
        ax,
        &X,
        &Y,
        |x, y| x + y,
        |c| {
            c.label = text("filled");
            c.fill = true;
            c.line.color = ColorSpec::Rgba { color: HALF_BLUE };
            c.line_opacity = 0.8;
            c.fill_opacity = 0.4;
        },
    );
    let lines = fx.contour(
        ax,
        &X,
        &Y,
        |x, y| x + y,
        |c| {
            c.label = text("lines");
            c.fill = false;
            c.line_opacity = 0.25;
            c.fill_opacity = 0.4;
        },
    );
    fx.ax(ax).legend = Some(Legend::Inside(InsideLegend {
        location: LegendLocation::NorthEast,
        boxed: true,
        ..InsideLegend::default()
    }));
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let sample = |artist: NodeId| {
        let entry = scene
            .hit_map
            .legend_entries
            .iter()
            .find(|e| e.legend == LegendOwner::Axes(ax) && entry_artist(e) == artist)
            .expect("the artist has a legend entry");
        let paths: Vec<_> = leaves
            .iter()
            .filter(|l| {
                l.source == Some(ax) && l.bbox().is_some_and(|b| inside(b, entry.rect, 0.5))
            })
            .filter_map(Leaf::path)
            .map(|p| {
                (
                    p.fill.as_ref().map(|f| f.color.a),
                    p.stroke.as_ref().map(|s| s.color.a),
                )
            })
            .collect();
        assert_eq!(paths.len(), 1, "one sample keys {artist}: {paths:?}");
        paths[0]
    };
    let (fill, edge) = sample(filled);
    assert_alpha(fill.expect("the patch is filled"), 0.4, "the patch fill");
    assert_alpha(
        edge.expect("the patch has an edge"),
        0.5 * 0.8,
        "the patch edge",
    );
    let (fill, stroke) = sample(lines);
    assert_eq!(
        fill, None,
        "the line sample of an unfilled contour has no fill"
    );
    assert_alpha(
        stroke.expect("the line is stroked"),
        0.25,
        "the line sample",
    );
}

// Why: compilation never fails, but a figure can reach it without validation (built by hand, or mid-edit); an alpha
// array that does not have a value for every node or pixel cannot be paired with them, so the artist is left out
// with a warning naming it, as for any other array of the wrong shape, rather than read out of bounds.
#[test]
fn an_alpha_array_of_the_wrong_shape_leaves_the_artist_out_with_a_warning() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let short = fx.vector(&[1.0, 1.0]);
    let surface = fx.surface(ax, &X, &Y, |x, y| x + y, |s| s.edge_alpha = Some(short));
    let image = fx.imagesc(ax, vec![2, 2], vec![1.0; 4], xy(None, None), |m| {
        m.alpha = Some(short)
    });
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    for id in [surface, image] {
        assert!(from_source(&leaves, id).is_empty(), "{id} is not drawn");
        let warnings: Vec<_> = scene
            .warnings
            .iter()
            .filter(|w| w.node == Some(id))
            .collect();
        assert_eq!(warnings.len(), 1, "{:?}", scene.warnings);
        assert!(warnings[0].message.contains("alpha"), "{:?}", warnings[0]);
    }
}
