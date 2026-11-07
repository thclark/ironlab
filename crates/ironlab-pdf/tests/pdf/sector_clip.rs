//! Raster checks of the sector clip that a polar axes draws its artists beneath.
//!
//! A polar plot region is an annular sector: the disc of the rim, less the hole, between the start and end angles of
//! the sector. Artists are clipped to it by an [`ItemKind::SectorClip`], which the exporter writes as a clip path of
//! the outer arc, the inner arc (or the centre) and the radial edges. These tests rasterise the page with both
//! engines and sample pixels inside and outside the sector, well away from its boundary so that anti-aliasing does
//! not affect them.
//!
//! Pages are rasterised at 72 dpi, so one pixel is one point. Angles are measured on the page in radians
//! anticlockwise from east, so that a point at angle `a` and radius `r` from the centre `c` lies at
//! `(c.x + r cos a, c.y − r sin a)`, the figure's y growing downwards.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use ironlab_pdf::{PdfOptions, RasterOptions, RasterPolicy};
use ironlab_scene::display::{DisplayList, Item, ItemKind, Point, Rect, Sector};

use crate::common::*;
use crate::require_tools;

const CENTRE: Point = Point::new(100.0, 100.0);

/// The page position at angle `a` (radians, anticlockwise from east) and radius `r` from [`CENTRE`], offset to the
/// centre of the pixel that contains it.
fn polar(a: f64, r: f64) -> (f64, f64) {
    let x = CENTRE.x + r * a.cos();
    let y = CENTRE.y - r * a.sin();
    (x.floor() + 0.5, y.floor() + 0.5)
}

fn sector_clip(sector: Sector, items: Vec<Item>) -> Item {
    item(ItemKind::SectorClip { sector, items })
}

/// A 200 × 200 point page whose whole area is painted red beneath a sector clip of `sector`, followed by a blue
/// square at the top-left corner drawn after the clip, outside it.
fn page_clipped_to(sector: Sector) -> DisplayList {
    let mut list = page(200.0, 200.0);
    list.items.push(sector_clip(
        sector,
        vec![filled_rect(Rect::new(0.0, 0.0, 200.0, 200.0), RED)],
    ));
    list.items
        .push(filled_rect(Rect::new(5.0, 5.0, 20.0, 20.0), BLUE));
    list
}

// WHY: a polar axes clips its artists to the annulus between the hole and the rim, so that data beyond the radial
// limits does not paint over the angular tick labels around the rim, and data inside the hole does not fill it. The
// samples at 22.5° just inside the rim catch an outer arc written as a coarse polygon (an octagon with a vertex at 0°
// leaves that point outside), and the blue square checks that the clip is popped when the sector clip ends.
#[test]
fn an_annular_sector_clip_paints_only_between_the_hole_and_the_rim() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("sector-annulus");
    let text = engine();
    let list = page_clipped_to(Sector {
        centre: CENTRE,
        inner: 30.0,
        outer: 80.0,
        start: 0.0,
        sweep: TAU,
    });

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        for k in 0..8 {
            let a = f64::from(k) * TAU / 8.0;
            let (x, y) = polar(a, 55.0);
            assert_pixel(
                engine,
                &image,
                x,
                y,
                RED_PX,
                3,
                "midway between hole and rim",
            );
            let (x, y) = polar(a, 90.0);
            assert_pixel(engine, &image, x, y, WHITE_PX, 3, "beyond the rim");
            let (x, y) = polar(a, 20.0);
            assert_pixel(engine, &image, x, y, WHITE_PX, 3, "inside the hole");
        }
        let (x, y) = polar(PI / 8.0, 77.0);
        assert_pixel(
            engine,
            &image,
            x,
            y,
            RED_PX,
            3,
            "just inside the rim at 22.5°",
        );
        assert_pixel(
            engine,
            &image,
            100.5,
            100.5,
            WHITE_PX,
            3,
            "the centre, in the hole",
        );
        assert_pixel(
            engine,
            &image,
            190.5,
            190.5,
            WHITE_PX,
            3,
            "the corner of the page",
        );
        assert_pixel(
            engine,
            &image,
            15.5,
            15.5,
            [0, 0, 255],
            3,
            "an item after the sector clip is not clipped",
        );
    }
}

// WHY: a partial sector (for example a sonar sweep of −60° to 60°) must show data only between its radial edges,
// and the angles must run anticlockwise from east on the page: a clip that measured angles clockwise (forgetting that
// the figure's y grows downwards) would show the lower-left quadrant instead of the upper-left. Without a hole, the
// sector closes at the centre. The sample at 135° and radius 70 lies beyond the chord between the ends of the outer
// arc, so it is painted only if the arc is a curve.
#[test]
fn a_partial_sector_clip_paints_only_between_its_radial_edges_anticlockwise_from_its_start() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("sector-partial");
    let text = engine();
    // From north (90°) anticlockwise to west (180°): the upper-left quadrant of the page.
    let list = page_clipped_to(Sector {
        centre: CENTRE,
        inner: 0.0,
        outer: 80.0,
        start: FRAC_PI_2,
        sweep: FRAC_PI_2,
    });

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        for (degrees, r, expected, what) in [
            (135.0, 40.0, RED_PX, "the middle of the sector"),
            (100.0, 55.0, RED_PX, "near the start edge, inside"),
            (170.0, 55.0, RED_PX, "near the end edge, inside"),
            (135.0, 70.0, RED_PX, "beyond the chord of the outer arc"),
            (
                135.0,
                5.0,
                RED_PX,
                "near the centre, which closes the sector",
            ),
            (
                45.0,
                40.0,
                WHITE_PX,
                "the upper-right quadrant, before the start",
            ),
            (
                225.0,
                40.0,
                WHITE_PX,
                "the lower-left quadrant, after the end",
            ),
            (315.0, 40.0, WHITE_PX, "the opposite quadrant"),
            (
                135.0,
                90.0,
                WHITE_PX,
                "beyond the rim inside the angular range",
            ),
        ] {
            let (x, y) = polar(f64::to_radians(degrees), r);
            assert_pixel(engine, &image, x, y, expected, 3, what);
        }
    }
}

// WHY: a dense artist inside a polar axes (a surface of many faces, a scalar image) may be rasterised, and the raster
// covers the bounding box of its content, which spills beyond the rim. The raster must be drawn beneath the same
// sector clip as the vector geometry it replaces, or the export would show square corners of colour outside the disc
// that the screen does not show.
#[test]
fn a_rasterised_dense_group_inside_a_sector_clip_is_clipped_to_the_sector() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("sector-dense");
    let mut list = page(200.0, 200.0);
    list.items.push(sector_clip(
        Sector {
            centre: CENTRE,
            inner: 0.0,
            outer: 80.0,
            start: 0.0,
            sweep: TAU,
        },
        vec![dense(
            10_000,
            vec![filled_rect(Rect::new(10.0, 10.0, 180.0, 180.0), RED)],
        )],
    ));
    let options = PdfOptions {
        raster: RasterOptions {
            policy: RasterPolicy::Always,
            dpi: 72.0,
            ..RasterOptions::default()
        },
        ..PdfOptions::default()
    };
    let (mut stub, calls) = Stub::flat([255, 0, 0, 255]);
    let rendered = render_reporting(&list, &options, Some(&mut stub));
    assert!(
        !calls.borrow().is_empty(),
        "the dense group is rasterised under RasterPolicy::Always"
    );
    let pdf = ws.write_pdf("figure", &rendered.bytes);

    for engine in ENGINES {
        let image = rasterise(&pdf, engine);
        assert_pixel(
            engine,
            &image,
            100.5,
            100.5,
            RED_PX,
            3,
            "the centre of the disc",
        );
        let (x, y) = polar(PI / 4.0, 70.0);
        assert_pixel(engine, &image, x, y, RED_PX, 3, "inside the rim");
        for (x, y, what) in [
            (
                15.5,
                15.5,
                "the top-left corner of the raster, outside the disc",
            ),
            (184.5, 15.5, "the top-right corner of the raster"),
            (15.5, 184.5, "the bottom-left corner of the raster"),
            (184.5, 184.5, "the bottom-right corner of the raster"),
        ] {
            assert_pixel(engine, &image, x, y, WHITE_PX, 3, what);
        }
    }
}

// WHY: a sweep about east crosses page angle 0, and a sector may describe it with a start near a full turn so that
// `start + sweep` exceeds a turn; the clip path must still run from the start edge through angle 0 to the end edge.
// The sector has a hole as well, so the clip path of a partial annulus (outer arc, radial edge, inner arc back,
// radial edge) is tested, which a path closed through the centre would fill into the hole.
#[test]
fn a_partial_annulus_clip_that_crosses_angle_zero_keeps_both_sides_of_it() {
    require_tools!(RASTER_TOOLS[0], RASTER_TOOLS[1]);
    let ws = Workspace::new("sector-wrap");
    let text = engine();
    let list = page_clipped_to(Sector {
        centre: CENTRE,
        inner: 20.0,
        outer: 80.0,
        start: 5.0 * PI / 3.0,
        sweep: 2.0 * PI / 3.0,
    });

    for (engine, image) in render_and_rasterise(&ws, &list, &text) {
        for (degrees, r, expected, what) in [
            (0.0, 50.0, RED_PX, "on angle 0"),
            (30.0, 50.0, RED_PX, "above angle 0"),
            (330.0, 50.0, RED_PX, "below angle 0"),
            (52.0, 70.0, RED_PX, "near the end edge, beyond the chord"),
            (308.0, 50.0, RED_PX, "near the start edge, inside"),
            (90.0, 50.0, WHITE_PX, "north, beyond the end"),
            (180.0, 50.0, WHITE_PX, "west, opposite the sector"),
            (270.0, 50.0, WHITE_PX, "south, before the start"),
            (0.0, 10.0, WHITE_PX, "in the hole"),
            (0.0, 90.0, WHITE_PX, "beyond the rim"),
        ] {
            let (x, y) = polar(f64::to_radians(degrees), r);
            assert_pixel(engine, &image, x, y, expected, 3, what);
        }
    }
}
