//! Colour scales as figure nodes (#40, part of #24): one mapping for every artist, automatic limits per scale over
//! every artist that names it in every axes, the four kinds, both colormap variants, both norms, the out-of-range
//! policies of the scale, and contour levels taken from the scale.
//!
//! Expected colours are worked out here from the public 256-entry tables and the arithmetic stated in the plan,
//! rather than from the compiler's own mapping, so that the expectations do not depend on the code under test. A
//! named colormap at a normalised position `t` is the table entry `min(floor(256 t), 255)`, as
//! `ironlab_scene::maths::colormap::sample` gives; one entry of slack absorbs any other rounding of the same
//! position.
//!
//! Validation (which issues are raised, and against which node) belongs to the IR tests; these tests own what the
//! compiler draws, including what it draws for a figure that validation refuses. They pin these rules:
//!
//! - **Band edges.** A value exactly on an edge between two bands belongs to the upper band. A value exactly at the
//!   first or last edge, or at a manual limit, lies inside the range.
//! - **Logarithmic norm.** Automatic limits range over the positive finite values only; a value that is not positive
//!   lies below the range and follows `below`.
//! - **Index-mapped surfaces.** Through an indexed or categorical scale, a surface face takes the value of its first
//!   corner (lower `i`, lower `j`) rather than the mean of its four corners, because the mean of indices is not an
//!   index.
//! - **Index-mapped contours.** A contour with `levels: None` through an indexed or categorical scale of n entries
//!   places its levels at the half-integers 0.5, 1.5, …, n − 1.5 between entries, so each band holds exactly one
//!   whole number and takes that entry's colour, and each isoline takes the colour of the band below it. A contour
//!   with explicit levels colours each isoline by the entry its level names.
//! - **Colormapped lines and quivers.** In this stage a line or quiver has no values to map and names no scale (the
//!   `Series` of stage 3 resolves line colour properly). A `Colormapped` colour on one keeps today's drawing with
//!   the default colormap: the middle entry of viridis, whatever scales the other artists of its axes use.

use ironlab_ir::{Artist, CategoricalColorScale, ColorCategory, Text};
use ironlab_ir::{
    Color, ColorScaleKind, ColorScaleLevels, ColorScaleNorm, ColorSpec, Colormap, ColormapName,
    ContinuousColorScale, DiscreteColorScale, Edit, IndexedColorScale, Levels, Limits, NodeId,
    OutOfRange, PropertyPath, Transaction, Value,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{ImageItem, Rgba};
use ironlab_scene::maths::colormap::{Lut, VIRIDIS};

use crate::common::{
    Fx, compile_figure, linspace, nearest_lut_index, out_of_range, range, rgb8, xy,
};
use crate::probe::{Leaf, axis_maps, from_source, leaves, pixel};

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

/// Returns the image item of the one leaf an image artist drew.
#[track_caller]
fn image_item(scene: &Scene, id: NodeId) -> ImageItem {
    let drawn = from_source(&leaves(scene), id);
    assert_eq!(drawn.len(), 1, "an image is exactly one leaf: {drawn:?}");
    drawn[0]
        .image()
        .expect("the leaf of an image is an image item")
        .clone()
}

/// Returns the messages of the warnings that name `id`.
fn warnings_naming(scene: &Scene, id: NodeId) -> Vec<String> {
    scene
        .warnings
        .iter()
        .filter(|w| w.node == Some(id))
        .map(|w| w.message.clone())
        .collect()
}

/// An IR colour as the four 8-bit channels the pixel probe reads.
fn channels(color: Color) -> [u8; 4] {
    [color.r, color.g, color.b, color.a].map(|c| (c * 255.0).round() as u8)
}

/// The index of the entry of `lut` nearest to an 8-bit pixel.
fn lut_index(lut: &Lut, px: [u8; 4]) -> usize {
    nearest_lut_index(
        lut,
        Rgba::new(
            f32::from(px[0]) / 255.0,
            f32::from(px[1]) / 255.0,
            f32::from(px[2]) / 255.0,
            1.0,
        ),
    )
}

/// The table entry of a named colormap at the normalised position `t`.
fn entry(t: f64) -> usize {
    ((t * 256.0).floor() as usize).min(255)
}

/// Asserts that an opaque pixel shows the viridis entry `expected`, with one entry of slack.
#[track_caller]
fn assert_viridis(px: [u8; 4], expected: usize, what: &str) {
    assert_eq!(px[3], 255, "{what} is opaque");
    let actual = lut_index(&VIRIDIS, px);
    assert!(
        actual.abs_diff(expected) <= 1,
        "{what}: viridis entry {actual}, expected {expected}"
    );
}

/// Asserts that two 8-bit pixels differ by at most `slack` levels in every channel.
#[track_caller]
fn assert_pixel_close(actual: [u8; 4], expected: [u8; 4], slack: u8, what: &str) {
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(a, e)| a.abs_diff(e) <= slack),
        "{what}: {actual:?}, expected {expected:?}"
    );
}

/// A continuous scale of viridis with the given limits and the defaults of its kind otherwise.
fn viridis(limits: Limits) -> ColorScaleKind {
    ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Named {
            name: ColormapName::Viridis,
            reverse: false,
        },
        limits,
        ..ContinuousColorScale::default()
    })
}

/// A discrete scale of viridis with the given band edges and the defaults of its kind otherwise.
fn bands(edges: &[f64]) -> ColorScaleKind {
    ColorScaleKind::Discrete(DiscreteColorScale {
        levels: ColorScaleLevels::Explicit {
            values: edges.to_vec(),
        },
        ..DiscreteColorScale::default()
    })
}

/// An indexed scale of the given colours whose out-of-range indices follow `outside`.
fn indexed(colors: Vec<Color>, outside: OutOfRange) -> ColorScaleKind {
    ColorScaleKind::Indexed(IndexedColorScale {
        colors,
        outside: out_of_range(outside),
        ..IndexedColorScale::default()
    })
}

/// A categorical scale of the given colours, with a label for each, whose out-of-range indices follow `outside`.
fn categorical(colors: Vec<Color>, outside: OutOfRange) -> ColorScaleKind {
    ColorScaleKind::Categorical(CategoricalColorScale {
        categories: colors
            .into_iter()
            .enumerate()
            .map(|(k, color)| ColorCategory {
                color,
                label: Text::plain(format!("category {k}")),
            })
            .collect(),
        outside: out_of_range(outside),
    })
}

/// Compiles a figure of one 2D axes holding a 1 × n scalar image of `values` through one scale of the given kind,
/// and returns the scene, the image item, the image and the scale.
fn one_image(kind: ColorScaleKind, values: Vec<f64>) -> (Scene, ImageItem, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scale = fx.color_scale(kind);
    let n = values.len();
    let image = fx.scalar_image(ax, vec![1, n], values, Some(scale), xy(None, None), |_| {});
    let scene = compile_figure(&fx.build());
    let item = image_item(&scene, image);
    (scene, item, image, scale)
}

// ---------------------------------------------------------------------------------
// Automatic limits per scale
// ---------------------------------------------------------------------------------

// WHY: one scale shown beside several plots must mean one thing in all of them, so its automatic limits span every artist that names it, in every axes.
#[test]
fn automatic_limits_of_a_scale_span_every_artist_that_names_it_across_axes() {
    let build = |shared: bool| {
        let mut fx = Fx::new();
        fx.fig.layout.cols = 2;
        let left = fx.axes2d(0, 0);
        let right = fx.axes2d(0, 1);
        let first = fx.color_scale(viridis(Limits::Auto));
        let second = if shared {
            first
        } else {
            fx.color_scale(viridis(Limits::Auto))
        };
        let a = fx.scalar_image(
            left,
            vec![1, 2],
            vec![0.0, 1.0],
            Some(first),
            xy(None, None),
            |_| {},
        );
        let b = fx.scalar_image(
            right,
            vec![1, 2],
            vec![0.0, 3.0],
            Some(second),
            xy(None, None),
            |_| {},
        );
        let scene = compile_figure(&fx.build());
        assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
        (image_item(&scene, a), image_item(&scene, b))
    };
    let (left, right) = build(true);
    assert_viridis(pixel(&left, 0, 0), 0, "the shared minimum");
    assert_viridis(
        pixel(&left, 0, 1),
        entry(1.0 / 3.0),
        "1 lies a third of the way along limits shared with the right axes",
    );
    assert_viridis(pixel(&right, 0, 1), 255, "the shared maximum");

    let (left, _) = build(false);
    assert_viridis(
        pixel(&left, 0, 1),
        255,
        "with a scale of its own, 1 is the maximum of the left image",
    );
}

// WHY: hiding an artist must not recolour the others, so a hidden artist still contributes to its scale's automatic limits (ADR 0006).
#[test]
fn a_hidden_artist_still_contributes_to_its_scales_automatic_limits() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let right = fx.axes2d(0, 1);
    let scale = fx.color_scale(viridis(Limits::Auto));
    let shown = fx.scalar_image(
        left,
        vec![1, 2],
        vec![0.0, 1.0],
        Some(scale),
        xy(None, None),
        |_| {},
    );
    let hidden = fx.scalar_image(
        right,
        vec![1, 2],
        vec![0.0, 10.0],
        Some(scale),
        xy(None, None),
        |m| {
            m.visible = false;
        },
    );
    let scene = compile_figure(&fx.build());
    assert!(
        from_source(&leaves(&scene), hidden).is_empty(),
        "the hidden image draws nothing"
    );
    assert_viridis(
        pixel(&image_item(&scene, shown), 0, 1),
        entry(0.1),
        "1 lies a tenth of the way along limits that include the hidden 10",
    );
}

// WHY: dragging a colourbar is one edit of its scale's limits, and every artist that names the scale must follow it so that no plot disagrees with the bar.
#[test]
fn one_set_of_a_scales_limits_recolours_every_artist_that_names_it() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let right = fx.axes2d(0, 1);
    let scale = fx.color_scale(viridis(Limits::Auto));
    let a = fx.scalar_image(
        left,
        vec![1, 2],
        vec![0.0, 1.0],
        Some(scale),
        xy(None, None),
        |_| {},
    );
    let b = fx.scalar_image(
        right,
        vec![1, 2],
        vec![0.0, 2.0],
        Some(scale),
        xy(None, None),
        |_| {},
    );
    let mut fig = fx.build();
    fig.apply(&Transaction {
        edits: vec![Edit::Set {
            node: scale,
            path: "kind.continuous.limits"
                .parse::<PropertyPath>()
                .expect("a property path"),
            value: Value::Limits(Limits::Manual { min: 0.0, max: 4.0 }),
        }],
    })
    .expect("the edit of the scale's limits is accepted");
    let scene = compile_figure(&fig);
    assert_viridis(
        pixel(&image_item(&scene, a), 0, 1),
        entry(0.25),
        "1 of [0, 4]",
    );
    assert_viridis(
        pixel(&image_item(&scene, b), 0, 1),
        entry(0.5),
        "2 of [0, 4]",
    );
}

// ---------------------------------------------------------------------------------
// Continuous scales: colormaps, norms and policies
// ---------------------------------------------------------------------------------

// WHY: a listed colormap is the author's own palette, so three colours are interpolated evenly between the limits rather than looked up as three blocks.
#[test]
fn a_listed_colormap_of_three_colours_interpolates_between_them() {
    let kind = ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Listed {
            colors: vec![RED, GREEN, BLUE],
        },
        limits: Limits::Manual { min: 0.0, max: 1.0 },
        ..ContinuousColorScale::default()
    });
    let (scene, item, _, _) = one_image(kind, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    for (i, expected, what) in [
        (0, [255, 0, 0, 255], "the first colour at the lower limit"),
        (1, [128, 128, 0, 255], "halfway from red to green"),
        (2, [0, 255, 0, 255], "the middle colour at the middle"),
        (3, [0, 128, 128, 255], "halfway from green to blue"),
        (4, [0, 0, 255, 255], "the last colour at the upper limit"),
    ] {
        assert_pixel_close(pixel(&item, 0, i), expected, 3, what);
    }
}

// WHY: a reversed colormap is the same palette run the other way, so the lower limit takes its last colour.
#[test]
fn a_reversed_named_colormap_runs_from_its_last_colour() {
    let kind = ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Named {
            name: ColormapName::Viridis,
            reverse: true,
        },
        limits: Limits::Manual { min: 0.0, max: 1.0 },
        ..ContinuousColorScale::default()
    });
    let (_, item, _, _) = one_image(kind, vec![0.0, 1.0]);
    assert_viridis(
        pixel(&item, 0, 0),
        255,
        "the lower limit of a reversed colormap",
    );
    assert_viridis(
        pixel(&item, 0, 1),
        0,
        "the upper limit of a reversed colormap",
    );
}

// WHY: a logarithmic scale must give each decade the same share of the colormap, under manual limits and under automatic ones.
#[test]
fn a_log_norm_spaces_decades_evenly() {
    let log = |limits: Limits| {
        ColorScaleKind::Continuous(ContinuousColorScale {
            limits,
            norm: ColorScaleNorm::Log,
            ..ContinuousColorScale::default()
        })
    };
    let (scene, item, _, _) = one_image(
        log(Limits::Manual {
            min: 1.0,
            max: 1000.0,
        }),
        vec![1.0, 10.0, 100.0, 1000.0],
    );
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    for (i, t) in [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0].into_iter().enumerate() {
        assert_viridis(
            pixel(&item, 0, i),
            entry(t),
            &format!("decade {i} of three"),
        );
    }

    let (_, item, _, _) = one_image(log(Limits::Auto), vec![1.0, 10.0, 100.0]);
    assert_viridis(
        pixel(&item, 0, 1),
        entry(0.5),
        "10 is midway between 1 and 100 on a logarithmic scale",
    );
}

// WHY: limits that are not positive have no place on a logarithmic scale; a figure holding them still compiles, and the scene says so against the scale rather than failing.
#[test]
fn non_positive_log_limits_are_reported_and_the_figure_still_compiles() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scale = fx.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
        limits: Limits::Manual {
            min: 0.0,
            max: 100.0,
        },
        norm: ColorScaleNorm::Log,
        ..ContinuousColorScale::default()
    }));
    fx.scalar_image(
        ax,
        vec![1, 2],
        vec![1.0, 10.0],
        Some(scale),
        xy(None, None),
        |_| {},
    );
    let scene = compile_figure(&fx.build());
    assert!(
        !warnings_naming(&scene, scale).is_empty(),
        "a warning names the scale: {:?}",
        scene.warnings
    );
}

// WHY: the out-of-range policies now belong to the scale and apply to every artist, and a continuous scale clamps by default, as surfaces always did (ADR 0011, workaround 1).
#[test]
fn a_continuous_scale_clamps_values_beyond_manual_limits_by_default_for_every_artist() {
    let mut fx = Fx::new();
    fx.fig.layout.cols = 2;
    let left = fx.axes2d(0, 0);
    let right = fx.axes2d(0, 1);
    let scale = fx.color_scale(viridis(Limits::Manual { min: 0.0, max: 1.0 }));
    let image = fx.scalar_image(
        left,
        vec![1, 3],
        vec![-5.0, 0.5, 5.0],
        Some(scale),
        xy(None, None),
        |_| {},
    );
    // Every face of the surface lies below the limits.
    let surface = fx.surface(
        right,
        &[0.0, 1.0],
        &[0.0, 1.0],
        |_, _| -3.0,
        |s| {
            s.color_scale = Some(scale);
            s.edge = ColorSpec::None;
        },
    );
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let item = image_item(&scene, image);
    assert_viridis(
        pixel(&item, 0, 0),
        0,
        "below the limits, clamped to the first colour",
    );
    assert_viridis(
        pixel(&item, 0, 2),
        255,
        "above the limits, clamped to the last colour",
    );
    let faces: Vec<Leaf> = from_source(&leaves(&scene), surface);
    assert_eq!(faces.len(), 1);
    let fill = faces[0]
        .path()
        .and_then(|p| p.fill)
        .expect("the face is filled");
    assert_eq!(
        nearest_lut_index(&VIRIDIS, fill.color),
        0,
        "the face below is clamped"
    );
}

// WHY: NaN is the author's way of saying "draw nothing here", so on every kind of scale a NaN is transparent by default and raises no warning.
#[test]
fn nan_draws_nothing_by_default_on_every_kind() {
    for kind in [
        viridis(Limits::Auto),
        bands(&[0.0, 1.0, 2.0]),
        indexed(vec![RED, GREEN], OutOfRange::Strict),
        categorical(vec![RED, GREEN], OutOfRange::Strict),
    ] {
        let (scene, item, _, _) = one_image(kind.clone(), vec![0.0, f64::NAN, 1.0]);
        assert!(scene.warnings.is_empty(), "{kind:?}: {:?}", scene.warnings);
        assert_eq!(pixel(&item, 0, 1)[3], 0, "{kind:?}: NaN is transparent");
        assert_eq!(pixel(&item, 0, 0)[3], 255, "{kind:?}: 0 is painted");
    }
}

// ---------------------------------------------------------------------------------
// Discrete scales
// ---------------------------------------------------------------------------------

// WHY: a discrete bar shows bands of single colours that use both ends of the colormap, so band i of k must take the colour at i / (k − 1), and values beyond the edges take the end bands by default.
#[test]
fn discrete_band_i_of_k_takes_the_colormap_colour_at_i_over_k_minus_one() {
    let edges = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
    let (scene, item, _, _) =
        one_image(bands(&edges), vec![0.5, 1.5, 2.5, 3.5, 4.5, 1.9, -1.0, 9.0]);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    for i in 0..5 {
        assert_viridis(
            pixel(&item, 0, i),
            entry(i as f64 / 4.0),
            &format!("band {i} of 5"),
        );
    }
    assert_eq!(
        pixel(&item, 0, 5),
        pixel(&item, 0, 1),
        "every value of a band takes its one colour"
    );
    assert_eq!(
        pixel(&item, 0, 6),
        pixel(&item, 0, 0),
        "below the edges, the first band"
    );
    assert_eq!(
        pixel(&item, 0, 7),
        pixel(&item, 0, 4),
        "above the edges, the last band"
    );
}

// WHY: a discrete scale bands every artist, a smooth surface included, so the plot agrees with its banded bar.
#[test]
fn a_smooth_surface_is_banded_by_a_discrete_scale_using_both_ends_of_the_colormap() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let edges = [0.0, 0.25, 0.5, 0.75, 1.0];
    let scale = fx.color_scale(bands(&edges));
    let x = linspace(0.0, 1.0, 21);
    let surface = fx.surface(
        ax,
        &x,
        &[0.0, 1.0],
        |x, _| x,
        |s| {
            s.color_scale = Some(scale);
            s.edge = ColorSpec::None;
        },
    );
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let (xmap, _) = axis_maps(&scene, ax);
    let band_entries = [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0].map(entry);
    let mut seen = [false; 4];
    let faces = from_source(&leaves(&scene), surface);
    assert_eq!(faces.len(), 20, "one face per cell");
    for face in &faces {
        let xs: Vec<f64> = face.subpaths()[0]
            .iter()
            .map(|p| xmap.to_data(p.x))
            .collect();
        let mean = xs.iter().sum::<f64>() / xs.len() as f64;
        let band = ((mean / 0.25).floor() as usize).min(3);
        let fill = face.path().and_then(|p| p.fill).expect("faces are filled");
        let actual = nearest_lut_index(&VIRIDIS, fill.color);
        assert!(
            actual.abs_diff(band_entries[band]) <= 1,
            "the face at x = {mean} takes the colour of band {band}: entry {actual}, expected {}",
            band_entries[band]
        );
        seen[band] = true;
    }
    assert_eq!(
        seen, [true; 4],
        "every band, both ends of the colormap included, is used"
    );
    let distinct: std::collections::BTreeSet<[u8; 3]> = faces
        .iter()
        .map(|f| rgb8(f.path().and_then(|p| p.fill).unwrap().color))
        .collect();
    assert_eq!(
        distinct.len(),
        4,
        "a banded surface shows exactly four colours: {distinct:?}"
    );
}

// WHY: a logarithmic scale cannot place a value that is not positive, so automatic limits are taken over the positive values alone, and zero and negative values lie below the range and follow `below` rather than dragging the limits.
#[test]
fn under_a_log_norm_automatic_limits_ignore_values_that_are_not_positive() {
    let log = |below: OutOfRange| {
        ColorScaleKind::Continuous(ContinuousColorScale {
            norm: ColorScaleNorm::Log,
            below: out_of_range(below),
            ..ContinuousColorScale::default()
        })
    };
    let values = vec![-5.0, 0.0, 1.0, 10.0, 100.0];
    let (scene, item, _, _) = one_image(log(OutOfRange::Clamp), values.clone());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    assert_viridis(pixel(&item, 0, 2), 0, "1 is the lower automatic limit");
    assert_viridis(
        pixel(&item, 0, 3),
        entry(0.5),
        "10 is midway between the positive limits 1 and 100",
    );
    assert_viridis(pixel(&item, 0, 4), 255, "100 is the upper automatic limit");
    assert_viridis(pixel(&item, 0, 0), 0, "−5 lies below and is clamped");
    assert_viridis(pixel(&item, 0, 1), 0, "0 lies below and is clamped");

    let (_, item, _, _) = one_image(log(OutOfRange::Transparent), values);
    assert_eq!(pixel(&item, 0, 0)[3], 0, "−5 follows a transparent `below`");
    assert_eq!(pixel(&item, 0, 1)[3], 0, "0 follows a transparent `below`");
    assert_eq!(pixel(&item, 0, 2)[3], 255, "1 is inside the range");
}

// WHY: a bar's band edge must be read one way only, so a value exactly on an inner edge takes the upper band, and a value exactly at the first or last edge lies inside the range rather than following `below` or `above`.
#[test]
fn a_value_on_a_band_edge_belongs_to_the_upper_band_and_the_outer_edges_are_inside() {
    let kind = ColorScaleKind::Discrete(DiscreteColorScale {
        levels: ColorScaleLevels::Explicit {
            values: vec![0.0, 1.0, 2.0, 3.0],
        },
        below: out_of_range(OutOfRange::Transparent),
        above: out_of_range(OutOfRange::Transparent),
        ..DiscreteColorScale::default()
    });
    let (scene, item, _, _) =
        one_image(kind, vec![0.5, 1.5, 2.5, 1.0, 2.0, 0.0, 3.0, -0.001, 3.001]);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let band = |k: usize| pixel(&item, 0, k);
    assert_eq!(band(3), band(1), "1.0 takes the band above the edge at 1");
    assert_eq!(band(4), band(2), "2.0 takes the band above the edge at 2");
    assert_eq!(
        band(5),
        band(0),
        "the first edge lies inside the first band"
    );
    assert_eq!(band(6), band(2), "the last edge lies inside the last band");
    assert_eq!(
        band(7)[3],
        0,
        "just below the first edge is below the range"
    );
    assert_eq!(band(8)[3], 0, "just above the last edge is above the range");
}

// ---------------------------------------------------------------------------------
// Indexed and categorical scales
// ---------------------------------------------------------------------------------

// WHY: 8-bit data are whole numbers, so each byte takes its entry directly, on an indexed and on a categorical scale.
#[test]
fn eight_bit_values_take_their_entry_directly() {
    for kind in [
        indexed(vec![RED, GREEN, BLUE], OutOfRange::Strict),
        categorical(vec![RED, GREEN, BLUE], OutOfRange::Strict),
    ] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let scale = fx.color_scale(kind.clone());
        let image = fx.scalar_image(
            ax,
            vec![1, 4],
            vec![2u8, 0, 1, 2],
            Some(scale),
            xy(None, None),
            |_| {},
        );
        let scene = compile_figure(&fx.build());
        assert!(scene.warnings.is_empty(), "{kind:?}: {:?}", scene.warnings);
        let item = image_item(&scene, image);
        for (i, color) in [BLUE, RED, GREEN, BLUE].into_iter().enumerate() {
            assert_eq!(pixel(&item, 0, i), channels(color), "{kind:?}: pixel {i}");
        }
    }
}

// WHY: #40 lifts the 256-entry limit, so a palette of 1024 entries draws each of its indices in its own colour.
#[test]
fn an_indexed_scale_of_1024_entries_draws_each_index_in_its_own_colour() {
    let color = |k: usize| {
        Color::rgb(
            (k % 256) as f32 / 255.0,
            (k / 256) as f32 * 85.0 / 255.0,
            1.0,
        )
    };
    let colors: Vec<Color> = (0..1024).map(color).collect();
    let indices = [0usize, 255, 256, 511, 768, 1023];
    let (scene, item, _, _) = one_image(
        indexed(colors, OutOfRange::Strict),
        indices.iter().map(|&k| k as f64).collect(),
    );
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    for (i, k) in indices.into_iter().enumerate() {
        assert_eq!(pixel(&item, 0, i), channels(color(k)), "index {k}");
    }
}

// WHY: under the default strict policy an index that names no entry, 1.5 or 3 of three, refuses the artist, so the scene skips it with one warning rather than drawing another entry, and draws the rest.
#[test]
fn a_fractional_or_out_of_range_index_under_strict_is_refused_on_both_index_mapped_kinds() {
    for (name, kind) in [
        (
            "indexed",
            indexed(vec![RED, GREEN, BLUE], OutOfRange::Strict),
        ),
        (
            "categorical",
            categorical(vec![RED, GREEN, BLUE], OutOfRange::Strict),
        ),
    ] {
        for (what, offending) in [
            ("fractional", 1.5),
            ("past the last entry", 3.0),
            ("negative", -1.0),
        ] {
            let mut fx = Fx::new();
            let ax = fx.axes2d(0, 0);
            let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
            let scale = fx.color_scale(kind.clone());
            let image = fx.scalar_image(
                ax,
                vec![1, 2],
                vec![0.0, offending],
                Some(scale),
                xy(None, None),
                |_| {},
            );
            let scene = compile_figure(&fx.build());
            assert!(
                from_source(&leaves(&scene), image).is_empty(),
                "{name}, {what}: the image is not drawn"
            );
            assert_eq!(
                warnings_naming(&scene, image).len(),
                1,
                "{name}, {what}: one warning names the image: {:?}",
                scene.warnings
            );
            assert!(
                !from_source(&leaves(&scene), line).is_empty(),
                "{name}, {what}: the rest of the figure is drawn"
            );
        }
    }
}

// WHY: under clamp an index is truncated toward zero and then clamped to the ends, so 1.7 draws entry 1 and 7 the last entry, on both index-mapped kinds.
#[test]
fn clamp_truncates_toward_zero_then_clamps_to_the_ends() {
    for kind in [
        indexed(vec![RED, GREEN, BLUE], OutOfRange::Clamp),
        categorical(vec![RED, GREEN, BLUE], OutOfRange::Clamp),
    ] {
        let (scene, item, _, _) = one_image(kind.clone(), vec![-0.5, 1.7, 2.9, 3.5, -3.0, 7.0]);
        assert!(scene.warnings.is_empty(), "{kind:?}: {:?}", scene.warnings);
        for (i, color, what) in [
            (0, RED, "−0.5 truncates to 0"),
            (1, GREEN, "1.7 truncates to 1"),
            (2, BLUE, "2.9 truncates to 2"),
            (3, BLUE, "3.5 truncates to 3 and clamps to the last entry"),
            (4, RED, "−3 clamps to the first entry"),
            (5, BLUE, "7 clamps to the last entry"),
        ] {
            assert_eq!(pixel(&item, 0, i), channels(color), "{kind:?}: {what}");
        }
    }
}

// WHY: the lenient policies outside the entries paint what they say: nothing for transparent, and the given colour for a fixed colour.
#[test]
fn transparent_and_fixed_colour_policies_paint_an_index_outside_the_entries_as_stated() {
    let (_, item, _, _) = one_image(
        indexed(vec![RED, GREEN], OutOfRange::Transparent),
        vec![0.0, 1.5, 2.0],
    );
    assert_eq!(pixel(&item, 0, 0), channels(RED));
    assert_eq!(
        pixel(&item, 0, 1)[3],
        0,
        "a fractional index is transparent"
    );
    assert_eq!(
        pixel(&item, 0, 2)[3],
        0,
        "an index past the last entry is transparent"
    );

    let gray = Color::rgba(0.5, 0.5, 0.5, 0.8);
    let (_, item, _, _) = one_image(
        categorical(vec![RED, GREEN], OutOfRange::Rgba { color: gray }),
        vec![0.0, 1.5, 2.0],
    );
    assert_eq!(pixel(&item, 0, 0), channels(RED));
    assert_pixel_close(
        pixel(&item, 0, 1),
        channels(gray),
        1,
        "a fractional index takes the fixed colour",
    );
    assert_pixel_close(
        pixel(&item, 0, 2),
        channels(gray),
        1,
        "an index past the last entry takes the fixed colour",
    );
}

// ---------------------------------------------------------------------------------
// Contour levels from the scale
// ---------------------------------------------------------------------------------

/// Returns the distinct data x of the stroked isolines of a contour of a field that varies with x alone, each
/// asserted to hold one level.
fn isoline_levels(scene: &Scene, ax: NodeId, contour: NodeId) -> Vec<f64> {
    let (xmap, _) = axis_maps(scene, ax);
    let mut levels: Vec<f64> = Vec::new();
    for leaf in from_source(&leaves(scene), contour) {
        if !leaf.path().is_some_and(|p| p.stroke.is_some()) {
            continue;
        }
        let xs: Vec<f64> = leaf
            .subpaths()
            .into_iter()
            .flatten()
            .map(|p| xmap.to_data(p.x))
            .collect();
        assert!(
            xs.iter().all(|x| (x - xs[0]).abs() < 1e-6),
            "an isoline of a field of x alone is vertical: {xs:?}"
        );
        if !levels.iter().any(|l| (l - xs[0]).abs() < 1e-6) {
            levels.push(xs[0]);
        }
    }
    levels.sort_by(f64::total_cmp);
    levels
}

// WHY: a contour with no levels of its own on a discrete scale takes the scale's band edges, so the contours and the banded bar cannot disagree.
#[test]
fn a_contour_with_no_levels_on_a_discrete_scale_draws_its_isolines_at_the_band_edges() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    // Uneven edges, which no automatic choice of levels would make, and a grid whose nodes avoid them.
    let edges = [0.0, 2.0, 7.0, 10.0];
    let scale = fx.color_scale(bands(&edges));
    let x = linspace(-0.9, 10.9, 9);
    let contour = fx.contour(
        ax,
        &x,
        &[0.0, 1.0],
        |x, _| x,
        |c| {
            c.levels = None;
            c.color_scale = Some(scale);
        },
    );
    let fig = fx.build();
    assert_eq!(fig.validate().errors, vec![]);
    let scene = compile_figure(&fig);
    let levels = isoline_levels(&scene, ax, contour);
    assert_eq!(
        levels.len(),
        edges.len(),
        "one isoline per band edge: {levels:?}"
    );
    for (level, edge) in levels.iter().zip(edges) {
        assert!((level - edge).abs() < 1e-6, "{levels:?} vs {edges:?}");
    }
}

// WHY: a filled contour on a discrete scale paints each band in that band's colour, so it shows the same bands as its bar.
#[test]
fn a_filled_contour_with_no_levels_on_a_discrete_scale_fills_each_band_in_its_colour() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let edges = [0.0, 2.0, 7.0, 10.0];
    let scale = fx.color_scale(bands(&edges));
    let x = linspace(0.05, 9.95, 9);
    let contour = fx.contour(
        ax,
        &x,
        &[0.0, 1.0],
        |x, _| x,
        |c| {
            c.levels = None;
            c.fill = true;
            c.color_scale = Some(scale);
        },
    );
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    let band_entries = [0.0, 0.5, 1.0].map(entry);
    let mut seen = [false; 3];
    for leaf in from_source(&leaves(&scene), contour) {
        let Some(fill) = leaf.path().and_then(|p| p.fill) else {
            continue;
        };
        for piece in leaf.subpaths() {
            let mean = piece.iter().map(|p| xmap.to_data(p.x)).sum::<f64>() / piece.len() as f64;
            let band = edges
                .windows(2)
                .position(|w| mean > w[0] && mean < w[1])
                .expect("a piece lies in a band");
            let actual = nearest_lut_index(&VIRIDIS, fill.color);
            assert!(
                actual.abs_diff(band_entries[band]) <= 1,
                "the piece at x = {mean} takes band {band}'s colour: entry {actual}, expected {}",
                band_entries[band]
            );
            seen[band] = true;
        }
    }
    assert_eq!(seen, [true; 3], "every band is filled");
}

// WHY: a contour with no levels of its own on a continuous scale draws ten automatic levels, exactly as one that asks for ten.
#[test]
fn a_contour_with_no_levels_on_a_continuous_scale_draws_ten_automatic_levels() {
    let levels_of = |levels: Option<Levels>| {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let scale = fx.color_scale(viridis(Limits::Auto));
        let x = linspace(-0.37, 9.71, 13);
        let contour = fx.contour(
            ax,
            &x,
            &[0.0, 1.0],
            |x, _| x,
            |c| {
                c.levels = levels;
                c.color_scale = Some(scale);
            },
        );
        let scene = compile_figure(&fx.build());
        isoline_levels(&scene, ax, contour)
    };
    let absent = levels_of(None);
    assert!(!absent.is_empty());
    assert_eq!(absent, levels_of(Some(Levels::Auto { count: 10 })));
}

// WHY: a contour through an indexed scale colours each isoline by the entry its level names, so class boundaries take their class colours.
#[test]
fn a_contour_through_an_indexed_scale_colours_each_isoline_by_the_entry_its_level_names() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scale = fx.color_scale(indexed(vec![RED, GREEN, BLUE], OutOfRange::Transparent));
    let x = linspace(-0.45, 2.35, 8);
    let contour = fx.contour(
        ax,
        &x,
        &[0.0, 1.0],
        |x, _| x,
        |c| {
            c.levels = Some(Levels::Explicit {
                values: vec![0.0, 1.0, 2.0],
            });
            c.color_scale = Some(scale);
        },
    );
    let scene = compile_figure(&fx.build());
    let (xmap, _) = axis_maps(&scene, ax);
    let mut seen = [false; 3];
    for leaf in from_source(&leaves(&scene), contour) {
        let Some(stroke) = leaf.path().and_then(|p| p.stroke.as_ref().map(|s| s.color)) else {
            continue;
        };
        let level = xmap.to_data(leaf.subpaths()[0][0].x).round() as usize;
        let expected = channels([RED, GREEN, BLUE][level]);
        assert_eq!(
            rgb8(stroke),
            [expected[0], expected[1], expected[2]],
            "the isoline at level {level} takes entry {level}"
        );
        seen[level] = true;
    }
    assert_eq!(seen, [true; 3]);
}

// WHY: the mean of indices is not an index, so a surface face through an index-mapped scale takes the value of its first corner (lower i, lower j); a face whose corners name 0, 2, 1 and 1 shows entry 0, not entry 1.
#[test]
fn a_surface_face_through_an_index_mapped_scale_takes_the_value_of_its_first_corner() {
    for kind in [
        indexed(vec![RED, GREEN, BLUE], OutOfRange::Strict),
        categorical(vec![RED, GREEN, BLUE], OutOfRange::Strict),
    ] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let scale = fx.color_scale(kind.clone());
        // Row j = 0 holds 0, 2, 0 and row j = 1 holds 1, 1, 1, so that each face's corner mean is 1 while its
        // four corners name different entries: the face from x = 0 has first corner 0 (red), and the face from
        // x = 1 has first corner 2 (blue). Any other corner, or the mean, gives another colour.
        let surface = fx.surface(
            ax,
            &[0.0, 1.0, 2.0],
            &[0.0, 1.0],
            |x, y| {
                if y == 0.0 {
                    [0.0, 2.0, 0.0][x as usize]
                } else {
                    1.0
                }
            },
            |s| {
                s.color_scale = Some(scale);
                s.edge = ColorSpec::None;
            },
        );
        let scene = compile_figure(&fx.build());
        assert!(scene.warnings.is_empty(), "{kind:?}: {:?}", scene.warnings);
        let (xmap, _) = axis_maps(&scene, ax);
        let faces = from_source(&leaves(&scene), surface);
        assert_eq!(faces.len(), 2, "{kind:?}: one face per cell");
        for face in &faces {
            let xs: Vec<f64> = face.subpaths()[0]
                .iter()
                .map(|p| xmap.to_data(p.x))
                .collect();
            let left = xs.iter().copied().fold(f64::INFINITY, f64::min).round() as usize;
            let expected = [RED, BLUE][left];
            let fill = face.path().and_then(|p| p.fill).expect("faces are filled");
            let want = channels(expected);
            assert_eq!(
                rgb8(fill.color),
                [want[0], want[1], want[2]],
                "{kind:?}: the face from x = {left} takes its first corner's entry"
            );
        }
    }
}

// WHY: an automatic level through an index-mapped scale must separate entries, never fall on one, so the levels lie at the half-integers between entries and each isoline takes the colour of the band below it.
#[test]
fn a_contour_with_no_levels_through_an_index_mapped_scale_draws_isolines_between_entries() {
    for kind in [
        indexed(vec![RED, GREEN, BLUE], OutOfRange::Strict),
        categorical(vec![RED, GREEN, BLUE], OutOfRange::Strict),
    ] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let scale = fx.color_scale(kind.clone());
        // Whole-number nodes 0, 1 and 2, so that every data value names an entry.
        let contour = fx.contour(
            ax,
            &[0.0, 1.0, 2.0],
            &[0.0, 1.0],
            |x, _| x,
            |c| {
                c.levels = None;
                c.color_scale = Some(scale);
            },
        );
        let scene = compile_figure(&fx.build());
        assert!(scene.warnings.is_empty(), "{kind:?}: {:?}", scene.warnings);
        let levels = isoline_levels(&scene, ax, contour);
        assert!(
            levels.len() == 2 && (levels[0] - 0.5).abs() < 1e-9 && (levels[1] - 1.5).abs() < 1e-9,
            "{kind:?}: one isoline between each pair of entries, at 0.5 and 1.5: {levels:?}"
        );
        let (xmap, _) = axis_maps(&scene, ax);
        for leaf in from_source(&leaves(&scene), contour) {
            let Some(stroke) = leaf.path().and_then(|p| p.stroke.as_ref().map(|s| s.color)) else {
                continue;
            };
            let level = xmap.to_data(leaf.subpaths()[0][0].x);
            let below = level.floor() as usize;
            let want = channels([RED, GREEN, BLUE][below]);
            assert_eq!(
                rgb8(stroke),
                [want[0], want[1], want[2]],
                "{kind:?}: the isoline at {level} takes entry {below}, the band below it"
            );
        }
    }
}

// WHY: a filled contour through an index-mapped scale is a map of classes, so each band holds exactly one whole number and is painted in that entry's colour, the outermost bands included.
#[test]
fn a_filled_contour_with_no_levels_through_an_index_mapped_scale_fills_one_band_per_entry() {
    for kind in [
        indexed(vec![RED, GREEN, BLUE], OutOfRange::Strict),
        categorical(vec![RED, GREEN, BLUE], OutOfRange::Strict),
    ] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let scale = fx.color_scale(kind.clone());
        let contour = fx.contour(
            ax,
            &[0.0, 1.0, 2.0],
            &[0.0, 1.0],
            |x, _| x,
            |c| {
                c.levels = None;
                c.fill = true;
                c.color_scale = Some(scale);
            },
        );
        let scene = compile_figure(&fx.build());
        assert!(scene.warnings.is_empty(), "{kind:?}: {:?}", scene.warnings);
        let (xmap, _) = axis_maps(&scene, ax);
        let mut seen = [false; 3];
        for leaf in from_source(&leaves(&scene), contour) {
            let Some(fill) = leaf.path().and_then(|p| p.fill) else {
                continue;
            };
            for piece in leaf.subpaths() {
                let mean =
                    piece.iter().map(|p| xmap.to_data(p.x)).sum::<f64>() / piece.len() as f64;
                // The bands are (−∞, 0.5], [0.5, 1.5] and [1.5, ∞), clipped to the field's 0 to 2.
                let entry = if mean < 0.5 {
                    0
                } else if mean < 1.5 {
                    1
                } else {
                    2
                };
                let want = channels([RED, GREEN, BLUE][entry]);
                assert_eq!(
                    rgb8(fill.color),
                    [want[0], want[1], want[2]],
                    "{kind:?}: the band at x = {mean} holds {entry}"
                );
                seen[entry] = true;
            }
        }
        assert_eq!(seen, [true; 3], "{kind:?}: one band per entry");
    }
}

// ---------------------------------------------------------------------------------
// Colormapped lines and quivers
// ---------------------------------------------------------------------------------

// WHY: a line or quiver has no values to map in this stage and the axes no longer has a colormap, so a colormapped colour keeps today's drawing with the default colormap, the middle of viridis, and does not borrow a scale another artist of the axes uses.
#[test]
fn a_colormapped_line_or_quiver_takes_the_middle_of_viridis() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    // A magma scale used by a surface in the same axes, which the line and quiver must ignore.
    let magma = fx.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Named {
            name: ColormapName::Magma,
            reverse: false,
        },
        ..ContinuousColorScale::default()
    }));
    fx.surface(
        ax,
        &[0.0, 1.0],
        &[0.0, 1.0],
        |x, _| x,
        |s| s.color_scale = Some(magma),
    );
    let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |l| {
        l.line.color = ColorSpec::Colormapped;
    });
    let quiver = fx.quiver(ax, &[0.5], &[0.5], None, &[0.2], &[0.2], None);
    match fx.fig.artist_mut(quiver) {
        Some(Artist::Quiver(q)) => q.line.color = ColorSpec::Colormapped,
        other => panic!("expected the quiver, found {other:?}"),
    }
    let scene = compile_figure(&fx.build());
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    for (what, id) in [("line", line), ("quiver", quiver)] {
        let strokes: Vec<Rgba> = from_source(&leaves(&scene), id)
            .iter()
            .filter_map(|leaf| leaf.path().and_then(|p| p.stroke.as_ref().map(|s| s.color)))
            .collect();
        assert!(!strokes.is_empty(), "the {what} is stroked");
        for stroke in strokes {
            assert_eq!(
                rgb8(stroke),
                VIRIDIS[128],
                "the {what} takes the middle entry of viridis"
            );
        }
    }
}

// ---------------------------------------------------------------------------------
// References that cannot be followed
// ---------------------------------------------------------------------------------

// WHY: an artist whose scale is missing, names a node of another kind or names nothing cannot be coloured, so the scene skips it with a warning while drawing the rest, rather than failing or guessing a colormap.
#[test]
fn an_artist_with_no_scale_or_a_scale_of_the_wrong_kind_is_reported_and_skipped() {
    for (what, reference) in [
        ("no scale", None),
        ("an axes as its scale", Some(NodeId(2))),
        ("a node that does not exist", Some(NodeId(999))),
    ] {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        assert_eq!(ax, NodeId(2), "the axes is the first node allocated");
        let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_| {});
        let image = fx.scalar_image(
            ax,
            vec![1, 2],
            vec![0.0, 1.0],
            reference,
            xy(range(2.0, 3.0), None),
            |_| {},
        );
        let scene = compile_figure(&fx.build());
        assert!(
            from_source(&leaves(&scene), image).is_empty(),
            "{what}: the image is not drawn"
        );
        assert!(
            !warnings_naming(&scene, image).is_empty(),
            "{what}: a warning names the image"
        );
        assert!(
            !from_source(&leaves(&scene), line).is_empty(),
            "{what}: the line is drawn"
        );
    }
}
