//! Colour scales built through the user API (#40, part of #24): scales as top-level nodes,
//! the default scale of an axes, artists given a scale of their own, and the scalar image.
//!
//! The API these tests assume, beyond the plan's "Facade" section:
//!
//! - `Figure::color_scale(&mut self) -> ColorScaleMut<'_>` adds a continuous scale with the
//!   defaults of its kind. `ColorScaleMut` offers `colormap(&mut self, impl
//!   Into<ir::Colormap>) -> &mut Self`, which accepts the facade's `Colormap` (the
//!   re-exported `ColormapName`, converted to a named colormap that is not reversed) and an
//!   `ir::Colormap` of either variant; `limits(&mut self, min: f64, max: f64) -> &mut
//!   Self`, which sets manual limits; `title(&mut self, impl Into<Text>) -> &mut Self`;
//!   `norm(&mut self, ir::ColorScaleNorm) -> &mut Self`; `discrete(&mut self, edges: impl
//!   AsRef<[f64]>) -> &mut Self`, which makes the scale discrete with those explicit band
//!   edges and keeps its colormap and norm; `bands(&mut self, count: u32) -> &mut Self`,
//!   which makes the scale discrete with `ColorScaleLevels::Auto { count, limits }`, keeping
//!   its colormap, norm and limits; and `id(&self) -> NodeId`. On a discrete scale with
//!   automatic edges, `limits` sets the limits of those edges.
//! - `Figure::indexed_scale(&mut self, colors: impl IntoIterator<Item = Color>) ->
//!   ColorScaleMut<'_>` and `Figure::categorical_scale(&mut self, categories: impl
//!   IntoIterator<Item = (Color, impl Into<Text>)>) -> ColorScaleMut<'_>`.
//! - `AxesMut::scalar_image(&mut self, values: impl Into<DataSource>) -> ScalarImageMut<'_>`,
//!   which stores a `ByteMatrix` as bytes and a `Matrix` as floats, and whose handle offers
//!   `color_scale(&mut self, NodeId) -> &mut Self` and `id(&self) -> NodeId`, besides the
//!   placement, alpha and opacity setters that the image handles share.
//! - `AxesMut::color_scale(&mut self) -> ColorScaleMut<'_>` returns the axes' default scale,
//!   which is the scale named by the first artist of the axes, in artist order, that names
//!   one, and creates a continuous scale when no artist of the axes names one. The rule
//!   depends only on the IR, so a scale created this way before any colour-mapped plot is
//!   not remembered by the axes until an artist names it: the guide says to configure the
//!   default scale after the first colour-mapped plot, or to create a scale with
//!   `fig.color_scale()` and pass it with `.color_scale(id)`.
//! - Every plotting call that maps colours (`scalar_image`, `contour`, `contourf`, `surf`,
//!   `mesh`, `surface`, and `scatter` with `colors`) names the axes' default scale, creating
//!   it if need be, and `ContourMut`, `SurfaceMut`, `ScatterMut` and `ScalarImageMut` each
//!   offer `color_scale(&mut self, NodeId) -> &mut Self`.
//! - An artist handle remembers whether its own plotting call created the default scale.
//!   Giving the artist another scale through that handle deletes that scale when nothing
//!   names it any more, so that the figure keeps no scale it did not ask for. A scale that
//!   the plotting call did not create (one the author created with `Figure::color_scale`,
//!   `indexed_scale`, `categorical_scale` or `AxesMut::color_scale`, or a default that an
//!   earlier artist created) is never deleted this way.
//! - The prelude's `Colormap` remains the alias of `ColormapName`.
//!
//! Validation of what a scale refuses belongs to the IR tests, and drawing to the scene
//! tests; these tests own the facade's promises.

use ironlab::ir::{
    self, Artist, ColorScale, ColorScaleKind, ColorScaleLevels, ColorScaleNorm, Limits, OutOfRange,
    Values,
};
use ironlab::prelude::*;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

/// Returns the scale with the given identifier.
#[track_caller]
fn scale(fig: &Figure, id: NodeId) -> &ColorScale {
    fig.ir()
        .color_scale(id)
        .unwrap_or_else(|| panic!("{id} is not a colour scale"))
}

/// Returns the scale that the artist with the given identifier names.
#[track_caller]
fn scale_of(fig: &Figure, artist: NodeId) -> Option<NodeId> {
    match fig.ir().artist(artist) {
        Some((_, Artist::ScalarImage(a))) => a.color_scale,
        Some((_, Artist::Contour(a))) => a.color_scale,
        Some((_, Artist::Surface(a))) => a.color_scale,
        Some((_, Artist::Scatter(a))) => a.color_scale,
        other => panic!("{artist} is not an artist that names a scale: {other:?}"),
    }
}

/// Returns the scalar image with the given identifier.
#[track_caller]
fn scalar_image(fig: &Figure, id: NodeId) -> &ir::ScalarImage {
    match fig.ir().artist(id) {
        Some((_, Artist::ScalarImage(image))) => image,
        other => panic!("expected the scalar image {id}, found {other:?}"),
    }
}

/// A small field with matching coordinate vectors.
fn grid() -> (Vec<f64>, Vec<f64>, Matrix) {
    let x = vec![0.0, 1.0, 2.0];
    let y = vec![0.0, 1.0];
    let z = Matrix::from_fn(2, 3, |row, col| (row * 3 + col) as f64);
    (x, y, z)
}

// ---------------------------------------------------------------------------------
// The default scale of an axes
// ---------------------------------------------------------------------------------

// PROMISE: plotting a colour-mapped artist with no scale just works: the axes gets one continuous scale with the defaults, and the artist names it.
#[test]
fn a_colour_mapped_artist_with_no_scale_names_a_new_default_scale() {
    let (x, y, z) = grid();
    let mut fig = Figure::new();
    let surface = fig.axes(0, 0).surf(&x, &y, &z).id();
    assert_eq!(fig.ir().color_scales.len(), 1, "one scale is created");
    let s = scale_of(&fig, surface).expect("the surface names a scale");
    assert_eq!(
        scale(&fig, s).kind,
        ColorScaleKind::Continuous(Default::default()),
        "the default scale is continuous with the defaults of its kind"
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: every colour-mapped artist of one axes shares its default scale, as they shared the axes' colormap and limits before.
#[test]
fn colour_mapped_artists_of_one_axes_share_its_default_scale() {
    let (x, y, z) = grid();
    let values = Matrix::from_fn(2, 2, |row, col| (row + col) as f64);
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let ids = [
        ax.surface(&x, &y, &z).id(),
        ax.contourf(&x, &y, &z).id(),
        ax.contour(&x, &y, &z).id(),
        ax.scalar_image(&values).id(),
        ax.scatter([0.0, 1.0], [0.0, 1.0]).colors([0.0, 1.0]).id(),
    ];
    let default = fig.axes(0, 0).color_scale().id();
    for id in ids {
        assert_eq!(scale_of(&fig, id), Some(default), "{id}");
    }
    assert_eq!(fig.ir().color_scales.len(), 1);
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: each axes has a default scale of its own, so two plots side by side are not tied together unless the author says so.
#[test]
fn each_axes_has_a_default_scale_of_its_own() {
    let (x, y, z) = grid();
    let mut fig = Figure::new().tiles(1, 2);
    let left = fig.axes(0, 0).surface(&x, &y, &z).id();
    let right = fig.axes(0, 1).surface(&x, &y, &z).id();
    assert_ne!(scale_of(&fig, left), scale_of(&fig, right));
    assert_eq!(fig.ir().color_scales.len(), 2);
    assert_eq!(
        fig.axes(0, 0).color_scale().id(),
        scale_of(&fig, left).unwrap()
    );
    assert_eq!(
        fig.axes(0, 1).color_scale().id(),
        scale_of(&fig, right).unwrap()
    );
}

// PROMISE: one scale is shared across axes by naming it, which is how a row of plots gets one colourbar (#88), and no unused default is left behind.
#[test]
fn a_scale_is_shared_across_axes_by_naming_it() {
    let (x, y, z) = grid();
    let mut fig = Figure::new().tiles(1, 3);
    let shared = fig.axes(0, 0).surface(&x, &y, &z).id();
    let s = fig.axes(0, 0).color_scale().id();
    let others = [
        fig.axes(0, 1).surface(&x, &y, &z).color_scale(s).id(),
        fig.axes(0, 2).contourf(&x, &y, &z).color_scale(s).id(),
    ];
    assert_eq!(scale_of(&fig, shared), Some(s));
    for id in others {
        assert_eq!(scale_of(&fig, id), Some(s));
    }
    assert_eq!(
        fig.ir().color_scales.len(),
        1,
        "the defaults the other calls created were replaced and deleted"
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: an artist given a scale of its own uses it and leaves the axes' default scale to the others.
#[test]
fn an_artist_given_another_scale_uses_it_and_the_others_keep_the_default() {
    let (x, y, z) = grid();
    let mut fig = Figure::new();
    let speed = fig
        .color_scale()
        .colormap(Colormap::Magma)
        .limits(0.0, 1.0)
        .id();
    let first = fig.axes(0, 0).surface(&x, &y, &z).id();
    let own = fig.axes(0, 0).contour(&x, &y, &z).color_scale(speed).id();
    let default = scale_of(&fig, first).expect("the surface names the default");
    assert_ne!(default, speed);
    assert_eq!(scale_of(&fig, own), Some(speed));
    assert_eq!(fig.axes(0, 0).color_scale().id(), default);
    assert_eq!(fig.ir().color_scales.len(), 2);
}

// PROMISE: the default scale is the scale of the axes' first colour-mapped artist, so an artist plotted later with no scale joins it, whatever kind it is.
#[test]
fn the_default_scale_is_the_scale_of_the_first_colour_mapped_artist() {
    let (x, y, z) = grid();
    let codes = ByteMatrix::from_fn(2, 2, |row, col| (row * 2 + col) as u8);
    let mut fig = Figure::new();
    let status = fig
        .categorical_scale([
            (GREEN, "converged"),
            (RED, "diverged"),
            (BLUE, "stalled"),
            (Color::BLACK, "not entered"),
        ])
        .id();
    fig.axes(0, 0).scalar_image(&codes).color_scale(status);
    let later = fig.axes(0, 0).surface(&x, &y, &z).id();
    assert_eq!(fig.axes(0, 0).color_scale().id(), status);
    assert_eq!(scale_of(&fig, later), Some(status));
}

// PROMISE: the default-scale rule depends only on the IR, so a figure saved and reloaded, or wrapped with `from_ir`, keeps adding artists to the same scale.
#[test]
fn the_default_scale_survives_from_ir_and_a_round_trip_through_both_formats() {
    let (x, y, z) = grid();
    let mut fig = Figure::new();
    let first = fig.axes(0, 0).surface(&x, &y, &z).id();
    let s = scale_of(&fig, first).unwrap();
    let saved = fig.into_ir();
    for (name, restored) in [
        ("from_ir", saved.clone()),
        ("JSON", ir::Figure::from_json(&saved.to_json()).unwrap()),
        (
            "protobuf",
            ir::Figure::from_protobuf(&saved.to_protobuf()).unwrap(),
        ),
    ] {
        let mut wrapped = Figure::from_ir(restored);
        assert_eq!(wrapped.axes(0, 0).color_scale().id(), s, "{name}");
        let later = wrapped.axes(0, 0).contourf(&x, &y, &z).id();
        assert_eq!(scale_of(&wrapped, later), Some(s), "{name}");
        assert_eq!(wrapped.ir().color_scales.len(), 1, "{name}");
    }
}

// PROMISE: replacing a scale never deletes one the author made: only the default that the artist's own plotting call created is deleted, and only when nothing else names it.
#[test]
fn replacing_a_scale_deletes_only_the_default_the_plotting_call_created() {
    let (x, y, z) = grid();
    let mut fig = Figure::new();
    let first = fig.color_scale().colormap(Colormap::Magma).id();
    let second = fig.color_scale().colormap(Colormap::Plasma).id();

    // The surface's own default is replaced by `first` and deleted; `first` is then
    // replaced by `second` and kept, unused, because the author made it.
    let surface = fig
        .axes(0, 0)
        .surface(&x, &y, &z)
        .color_scale(first)
        .color_scale(second)
        .id();
    assert_eq!(scale_of(&fig, surface), Some(second));
    let mut ids: Vec<NodeId> = fig.ir().color_scales.iter().map(|s| s.id).collect();
    ids.sort();
    assert_eq!(
        ids,
        [first, second],
        "the created default is gone and the author's scales are kept"
    );

    // A default that an earlier artist created and still names is kept when a later
    // artist, which joined it, is given another scale.
    let mut fig = Figure::new();
    let own = fig.color_scale().id();
    let earlier = fig.axes(0, 0).contourf(&x, &y, &z).id();
    let default = scale_of(&fig, earlier).unwrap();
    let later = fig
        .axes(0, 0)
        .scatter([0.0, 1.0], [0.0, 1.0])
        .colors([0.0, 1.0])
        .color_scale(own)
        .id();
    assert_eq!(scale_of(&fig, later), Some(own));
    assert_eq!(scale_of(&fig, earlier), Some(default));
    assert!(
        fig.ir().color_scale(default).is_some(),
        "the shared default is kept"
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: the documented ways to give an axes a configured scale work: configure the default after the first colour-mapped plot, or create a scale and pass it to the first plot, after which it is the axes' default.
#[test]
fn a_configured_scale_reaches_the_artists_of_an_axes_in_the_documented_ways() {
    let (x, y, z) = grid();

    // After the first plot, the axes' default scale is the plotted artist's scale.
    let mut fig = Figure::new();
    let surface = fig.axes(0, 0).surf(&x, &y, &z).id();
    fig.axes(0, 0)
        .color_scale()
        .colormap(Colormap::Magma)
        .limits(0.0, 5.0);
    let s = scale_of(&fig, surface).unwrap();
    let ColorScaleKind::Continuous(kind) = &scale(&fig, s).kind else {
        panic!("the default scale is continuous");
    };
    assert_eq!(
        kind.colormap,
        ir::Colormap::Named {
            name: Colormap::Magma,
            reverse: false
        }
    );
    assert_eq!(fig.ir().color_scales.len(), 1);

    // A scale passed to the first plot of an axes becomes its default for later plots.
    let mut fig = Figure::new();
    let speed = fig.color_scale().colormap(Colormap::Plasma).id();
    fig.axes(0, 0).contourf(&x, &y, &z).color_scale(speed);
    let later = fig.axes(0, 0).surf(&x, &y, &z).id();
    assert_eq!(fig.axes(0, 0).color_scale().id(), speed);
    assert_eq!(scale_of(&fig, later), Some(speed));
    assert_eq!(fig.ir().color_scales.len(), 1, "no default is left behind");
}

// ---------------------------------------------------------------------------------
// Scales as top-level nodes
// ---------------------------------------------------------------------------------

// PROMISE: `fig.color_scale()` returns a continuous scale whose setters reach the IR and chain as the guide shows.
#[test]
fn color_scale_setters_chain_and_set_the_scale() {
    let mut fig = Figure::new();
    let s = fig
        .color_scale()
        .colormap(Colormap::Magma)
        .limits(1.0, 100.0)
        .title("Speed (m/s)")
        .norm(ColorScaleNorm::Log)
        .id();
    let scale = scale(&fig, s);
    assert_eq!(scale.title, Some(Text::from("Speed (m/s)")));
    let ColorScaleKind::Continuous(kind) = &scale.kind else {
        panic!("the scale is continuous: {:?}", scale.kind);
    };
    assert_eq!(
        kind.colormap,
        ir::Colormap::Named {
            name: Colormap::Magma,
            reverse: false
        }
    );
    assert_eq!(
        kind.limits,
        Limits::Manual {
            min: 1.0,
            max: 100.0
        }
    );
    assert_eq!(kind.norm, ColorScaleNorm::Log);
    assert!(fig.validate().is_valid(), "an unused scale is valid");
}

// PROMISE: an author's own palette is a listed colormap, which a scale takes as it takes a named one.
#[test]
fn a_scale_takes_a_listed_colormap() {
    let mut fig = Figure::new();
    let listed = ir::Colormap::Listed {
        colors: vec![RED, GREEN, BLUE],
    };
    let s = fig.color_scale().colormap(listed.clone()).id();
    let ColorScaleKind::Continuous(kind) = &scale(&fig, s).kind else {
        panic!("the scale is continuous");
    };
    assert_eq!(kind.colormap, listed);
}

// PROMISE: `.discrete(edges)` switches a scale to bands at those edges and keeps the colormap and norm the author chose.
#[test]
fn discrete_switches_a_scale_to_bands_and_keeps_its_colormap_and_norm() {
    let (x, y, z) = grid();
    let mut fig = Figure::new();
    let s = fig
        .color_scale()
        .colormap(Colormap::Cividis)
        .discrete([0.0, 1.0, 2.5, 5.0])
        .id();
    let ColorScaleKind::Discrete(kind) = &scale(&fig, s).kind else {
        panic!("the scale is discrete: {:?}", scale(&fig, s).kind);
    };
    assert_eq!(
        kind.levels,
        ColorScaleLevels::Explicit {
            values: vec![0.0, 1.0, 2.5, 5.0]
        }
    );
    assert_eq!(
        kind.colormap,
        ir::Colormap::Named {
            name: Colormap::Cividis,
            reverse: false
        }
    );
    // A filled contour on the discrete scale takes the band edges as its levels.
    let contour = fig.axes(0, 0).contourf(&x, &y, &z).color_scale(s).id();
    match fig.ir().artist(contour) {
        Some((_, Artist::Contour(c))) => assert_eq!(c.levels, None),
        other => panic!("expected a contour, found {other:?}"),
    }
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: `.bands(count)` switches a scale to automatic band edges, keeping its colormap, norm and limits, and `limits` then sets the limits of those edges, in either order.
#[test]
fn bands_switches_a_scale_to_automatic_edges_within_its_limits() {
    let mut fig = Figure::new();
    let before = fig
        .color_scale()
        .colormap(Colormap::Cividis)
        .norm(ColorScaleNorm::Log)
        .limits(1.0, 1000.0)
        .bands(6)
        .id();
    let after = fig
        .color_scale()
        .colormap(Colormap::Cividis)
        .norm(ColorScaleNorm::Log)
        .bands(6)
        .limits(1.0, 1000.0)
        .id();
    for (order, id) in [("limits then bands", before), ("bands then limits", after)] {
        let ColorScaleKind::Discrete(kind) = &scale(&fig, id).kind else {
            panic!("{order}: the scale is discrete: {:?}", scale(&fig, id).kind);
        };
        assert_eq!(
            kind.levels,
            ColorScaleLevels::Auto {
                count: 6,
                limits: Limits::Manual {
                    min: 1.0,
                    max: 1000.0
                }
            },
            "{order}"
        );
        assert_eq!(
            kind.colormap,
            ir::Colormap::Named {
                name: Colormap::Cividis,
                reverse: false
            },
            "{order}"
        );
        assert_eq!(kind.norm, ColorScaleNorm::Log, "{order}");
    }
}

// PROMISE: a contour plotted without levels follows its scale (band edges, or ten automatic levels), and `.levels(n)` still gives it levels of its own.
#[test]
fn a_contour_has_no_levels_of_its_own_until_it_is_given_some() {
    let (x, y, z) = grid();
    let mut fig = Figure::new();
    let plain = fig.axes(0, 0).contourf(&x, &y, &z).id();
    let counted = fig.axes(0, 0).contour(&x, &y, &z).levels(5).id();
    let levels = |id: NodeId| match fig.ir().artist(id) {
        Some((_, Artist::Contour(c))) => c.levels.clone(),
        other => panic!("expected a contour, found {other:?}"),
    };
    assert_eq!(levels(plain), None);
    assert_eq!(levels(counted), Some(ir::Levels::Auto { count: 5 }));
}

// PROMISE: an indexed scale is a palette given colour by colour, and it refuses indices outside its entries unless told otherwise.
#[test]
fn indexed_scale_holds_its_colours_and_is_strict_by_default() {
    let mut fig = Figure::new();
    let colors: Vec<Color> = (0..=255u8)
        .map(|k| {
            let v = f32::from(k) / 255.0;
            Color::rgb(v, v, v)
        })
        .collect();
    let s = fig.indexed_scale(colors.clone()).id();
    let ColorScaleKind::Indexed(kind) = &scale(&fig, s).kind else {
        panic!("the scale is indexed: {:?}", scale(&fig, s).kind);
    };
    assert_eq!(kind.colors, colors);
    assert_eq!(kind.outside.policy, OutOfRange::Strict);
}

// PROMISE: a categorical scale is built from colour and label pairs, every category labelled, in the order given.
#[test]
fn categorical_scale_holds_its_categories_in_order_with_their_labels() {
    let mut fig = Figure::new();
    let s = fig
        .categorical_scale([(GREEN, "taken by the finite set"), (RED, "not entered")])
        .id();
    let ColorScaleKind::Categorical(kind) = &scale(&fig, s).kind else {
        panic!("the scale is categorical: {:?}", scale(&fig, s).kind);
    };
    let pairs: Vec<(Color, Text)> = kind
        .categories
        .iter()
        .map(|c| (c.color, c.label.clone()))
        .collect();
    assert_eq!(
        pairs,
        [
            (GREEN, Text::from("taken by the finite set")),
            (RED, Text::from("not entered"))
        ]
    );
    assert_eq!(kind.outside.policy, OutOfRange::Strict);
}

// ---------------------------------------------------------------------------------
// The scalar image
// ---------------------------------------------------------------------------------

// PROMISE: one image call covers both indexed and mapped rasters: bytes are stored as bytes and floats as floats, and either is coloured by the scale the image names.
#[test]
fn scalar_image_stores_bytes_or_floats_and_names_its_scale() {
    let codes = ByteMatrix::from_fn(2, 3, |row, col| (row * 3 + col) as u8);
    let field = Matrix::from_fn(3, 2, |row, col| (row * 2 + col) as f64 / 5.0);
    let mut fig = Figure::new().tiles(1, 2);
    let palette = fig
        .indexed_scale((0..6).map(|k| Color::rgb(k as f32 / 5.0, 0.0, 0.0)))
        .id();
    let bytes = fig
        .axes(0, 0)
        .scalar_image(&codes)
        .color_scale(palette)
        .id();
    let floats = fig.axes(0, 1).scalar_image(&field).id();

    let image = scalar_image(&fig, bytes);
    assert_eq!(image.color_scale, Some(palette));
    let array = &fig
        .ir()
        .data_array(image.values)
        .expect("the values are stored")
        .array;
    assert_eq!(array.shape, [2, 3]);
    assert!(matches!(&array.values, Values::U8(v) if *v == [0, 1, 2, 3, 4, 5]));

    let image = scalar_image(&fig, floats);
    let array = &fig
        .ir()
        .data_array(image.values)
        .expect("the values are stored")
        .array;
    assert_eq!(array.shape, [3, 2]);
    assert!(matches!(&array.values, Values::F64(v) if v.len() == 6));
    let default = image
        .color_scale
        .expect("an image with no scale takes the default");
    assert_eq!(fig.axes(0, 1).color_scale().id(), default);

    assert_eq!(
        fig.ir().color_scales.len(),
        2,
        "the default created for the byte image was replaced by the palette and deleted"
    );
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}
