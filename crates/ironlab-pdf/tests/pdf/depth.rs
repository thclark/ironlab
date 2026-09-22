//! Depth groups: how the artists of a three-dimensional axes are drawn, checked or rasterised, and what the exporter
//! reports about them.
//!
//! The scene compiler wraps the artists of a three-dimensional axes in a depth group, in a painter's order that is
//! right wherever the artists do not interpenetrate and wrong where they do. The exporter cannot tell which without
//! a depth-tested render to compare against, so it asks the rasteriser, and these tests pin what it asks for, what it
//! embeds and what it reports under each `DepthPolicy`.
//!
//! The rasteriser is the stub of `common.rs`, which paints a flat colour and records what it was asked to render. It
//! paints one colour when the list it is given holds a depth group and another when it does not, so that the two
//! renders the exporter compares under `DepthPolicy::Auto` can be made to differ or agree at will. That a real
//! depth-tested render differs from a painter's-order one exactly when the artists overlap out of order is the
//! renderer's business, proved with the viewer's own renderer in `ironlab-viewer/tests`.
//!
//! Pages are rasterised at 72 dpi, so one pixel is one point, unless a test says otherwise.

use std::path::Path;

use ironlab_ir::NodeId;
use ironlab_pdf::raster::{DepthPolicy, Need, needs_rasteriser, rasterises_any};
use ironlab_pdf::{
    ExportWarningKind, PdfError, PdfOptions, RasterImage, RasterOptions, RasterPolicy, Rasteriser,
    SAME_PICTURE_PATCH_PT, SAME_PICTURE_TOLERANCE, UnverifiedCause, render_display_list,
    same_picture,
};
use ironlab_scene::display::{
    Depth, DepthPlane, DisplayList, Fill, FillRule, Item, ItemKind, PathItem, Rect, Rgba, Transform,
};

use crate::common::*;
use crate::require_tools;

/// The identifier of the three-dimensional axes whose artists the depth group holds.
const AXES: NodeId = NodeId(7);

/// What the stub paints for a render that holds a depth group, standing for a depth-tested picture.
const TESTED: [u8; 4] = [255, 0, 255, 255];
const TESTED_PX: [u8; 3] = [255, 0, 255];
/// What the stub paints for a render without one, standing for a painter's-order picture.
const PAINTED: [u8; 4] = [0, 255, 0, 255];
const BLUE_PX: [u8; 3] = [0, 0, 255];

/// A stub whose depth-tested and painter's-order renders differ everywhere, as a renderer's do when the artists
/// overlap in an order no painter's sort can draw.
fn differing() -> (Stub, Calls) {
    Stub::new(TESTED, PAINTED)
}

/// A stub whose two renders are identical, as a renderer's are when the painter's order is exact.
fn identical() -> (Stub, Calls) {
    Stub::new(TESTED, TESTED)
}

/// A rasteriser whose painter's-order render differs from its depth-tested one by a square of `side` pixels at the
/// image's top-left corner, painted `PAINTED` where the depth-tested render has `TESTED`, and nowhere else: as a
/// renderer's do when one marker or one face is misdrawn while the rest of the picture agrees. The renders are
/// otherwise those of [`identical`], which records the calls.
struct Defective {
    side: u32,
    stub: Stub,
}

impl Defective {
    fn new(side: u32) -> (Self, Calls) {
        let (stub, calls) = identical();
        (Self { side, stub }, calls)
    }
}

impl Rasteriser for Defective {
    fn rasterise(&mut self, list: &DisplayList, dpi: f64) -> Result<RasterImage, String> {
        let image = self.stub.rasterise(list, dpi)?;
        if holds_depth(list) {
            return Ok(image);
        }
        let side = self.side.min(image.width).min(image.height);
        Ok(with_pixels(&image, block(0, 0, side, side), PAINTED))
    }
}

/// The items of the single root group of a list planned for the rasteriser.
fn root_items(list: &DisplayList) -> &[Item] {
    assert_eq!(
        list.items.len(),
        1,
        "a planned list has one root item: {:?}",
        list.items
    );
    match &list.items[0].kind {
        ItemKind::Group { items, .. } => items,
        other => panic!("the root of a planned list is a group, not {other:?}"),
    }
}

/// The same planned list with every depth group in its root group unwrapped, so that the root group holds the
/// group's items directly.
fn without_depth(list: &DisplayList) -> DisplayList {
    let mut unwrapped = list.clone();
    if let Some(Item {
        kind: ItemKind::Group { items, .. },
        ..
    }) = unwrapped.items.first_mut()
    {
        *items = items
            .iter()
            .flat_map(|item| match &item.kind {
                ItemKind::Depth { items } => items.clone(),
                _ => vec![item.clone()],
            })
            .collect();
    }
    unwrapped
}

/// A filled rectangle at a constant depth, as every path inside a depth group carries one.
fn deep_rect(r: Rect, color: Rgba, depth: f64) -> Item {
    item(ItemKind::Path(PathItem {
        segments: rect_segments(r),
        fill: Some(Fill {
            color,
            rule: FillRule::NonZero,
        }),
        stroke: None,
        depth: Some(Depth::Plane(DepthPlane::constant(depth))),
    }))
}

/// The artists of one three-dimensional axes whose box has its top-left corner at `(x, y)`: a red rectangle 40 by 20
/// points at depth 1, then a blue one 30 by 20 points at depth 0 whose left third lies over the red one's right
/// quarter. The painter's order puts blue over red on the overlap; a depth test would put red, the nearer, on top,
/// which is the disagreement the exporter exists to catch.
fn artists_at(x: f64, y: f64) -> Vec<Item> {
    vec![
        deep_rect(Rect::new(x, y, 40.0, 20.0), RED, 1.0),
        deep_rect(Rect::new(x + 30.0, y, 30.0, 20.0), BLUE, 0.0),
    ]
}

/// The depth group of the axes `AXES`, holding `items`.
fn depth_group(items: Vec<Item>) -> Item {
    Item {
        source: Some(AXES),
        kind: ItemKind::Depth { items },
    }
}

/// A 200 by 100 point page holding the depth group of one axes whose box has its corner at (20, 10): red over 20 to
/// 60 points across, blue over 50 to 80, both from 10 to 30 points down.
fn page_with_axes() -> DisplayList {
    holding(vec![depth_group(artists_at(20.0, 10.0))])
}

/// A 200 by 100 point page holding `items`.
fn holding(items: Vec<Item>) -> DisplayList {
    let mut list = page(200.0, 100.0);
    list.items = items;
    list
}

/// One red square at depth 1, as the content of a dense group.
fn square() -> Vec<Item> {
    vec![deep_rect(Rect::new(20.0, 10.0, 40.0, 20.0), RED, 1.0)]
}

/// Options drawing depth groups under `depth` and dense content under `policy`, rasterising at `dpi`.
fn options(depth: DepthPolicy, policy: RasterPolicy, dpi: f64) -> PdfOptions {
    PdfOptions {
        raster: RasterOptions { policy, dpi, depth },
        ..PdfOptions::default()
    }
}

/// Options drawing depth groups under `depth` at 72 dots per inch, with the default policy for dense content.
fn depth_options(depth: DepthPolicy) -> PdfOptions {
    options(depth, RasterPolicy::default(), 72.0)
}

/// Asserts that the artists of `page_with_axes` were drawn as vector geometry in the painter's order, blue over red
/// where they overlap, with no image on the page.
fn assert_drawn_in_order_as_vectors(pdf: &Path, what: &str) {
    assert!(
        pdfimages(pdf).is_empty(),
        "{what}: nothing is embedded as an image"
    );
    let image = rasterise(pdf, Engine::Poppler);
    assert_pixel(
        Engine::Poppler,
        &image,
        30.5,
        20.5,
        RED_PX,
        3,
        &format!("{what}: the red rectangle alone"),
    );
    assert_pixel(
        Engine::Poppler,
        &image,
        55.5,
        20.5,
        BLUE_PX,
        3,
        &format!("{what}: blue, painted after red, covers it where they overlap"),
    );
    assert_pixel(
        Engine::Poppler,
        &image,
        70.5,
        20.5,
        BLUE_PX,
        3,
        &format!("{what}: the blue rectangle alone"),
    );
}

/// A flat image of `width` by `height` pixels of `color`.
fn flat(width: u32, height: u32, color: [u8; 4]) -> RasterImage {
    RasterImage {
        width,
        height,
        rgba: color
            .iter()
            .copied()
            .cycle()
            .take(width as usize * height as usize * 4)
            .collect(),
    }
}

/// The image with the pixels at each `(column, row)` of `at` painted `color`.
fn with_pixels(
    image: &RasterImage,
    at: impl IntoIterator<Item = (u32, u32)>,
    color: [u8; 4],
) -> RasterImage {
    let mut painted = image.clone();
    for (x, y) in at {
        let start = (y * image.width + x) as usize * 4;
        painted.rgba[start..start + 4].copy_from_slice(&color);
    }
    painted
}

/// The `(column, row)` of every pixel of the `width` by `height` block whose top-left pixel is `(x, y)`.
fn block(x: u32, y: u32, width: u32, height: u32) -> impl Iterator<Item = (u32, u32)> {
    (y..y + height).flat_map(move |row| (x..x + width).map(move |column| (column, row)))
}

// WHY: an axes that nobody verified is drawn in the painter's order the compiler chose, which is wrong where the
// artists interpenetrate, so the user must be told, naming the axes, or a wrong figure would go to print silently.
// `DepthPolicy::Vector` is the user's promise that the page stays vector, so the exporter must not so much as ask
// the rasteriser; and a caller with no renderer, such as a build that never touches a GPU, must still get the
// painter's picture rather than a hole or an error, whatever policy it asked for. The report must give a reason a
// program can act on, and the options come before the environment: a user who asked for vectors is told so whether
// or not a rasteriser was there, because their choice and not the machine is what to change, and only a caller who
// left the choice to the exporter is told that nothing was there to verify it. The message of the user's choice
// names the option.
#[test]
fn a_depth_group_nobody_verified_is_drawn_in_order_and_the_report_blames_the_options_before_the_machine()
 {
    require_tools!(IMAGE_TOOL, RASTER_TOOLS[0]);
    let ws = Workspace::new("depth-unverified");
    let list = page_with_axes();
    let (mut stub, calls) = differing();
    let rows: [(DepthPolicy, Option<&mut dyn Rasteriser>, UnverifiedCause); 4] = [
        (
            DepthPolicy::Vector,
            Some(&mut stub as &mut dyn Rasteriser),
            UnverifiedCause::PolicyVector,
        ),
        (DepthPolicy::Vector, None, UnverifiedCause::PolicyVector),
        (DepthPolicy::Auto, None, UnverifiedCause::NoRasteriser),
        (DepthPolicy::Raster, None, UnverifiedCause::NoRasteriser),
    ];

    for (index, (policy, raster, cause)) in rows.into_iter().enumerate() {
        let name = format!(
            "{policy:?} {}",
            if raster.is_some() {
                "with a rasteriser"
            } else {
                "without one"
            }
        );
        let rendered = render_reporting(&list, &depth_options(policy), raster);
        assert!(
            calls.borrow().is_empty(),
            "{name}: the rasteriser is never asked"
        );
        assert_warnings(
            &rendered.warnings,
            &[(AXES, ExportWarningKind::Unverified { cause })],
            &name,
        );
        if cause == UnverifiedCause::PolicyVector {
            assert_message_names(
                &rendered.warnings[0],
                "`DepthPolicy::Vector`",
                &format!("{name}: the choice that left the axes unverified"),
            );
        }
        let pdf = ws.write_pdf(&format!("figure-{index}"), &rendered.bytes);
        assert_drawn_in_order_as_vectors(&pdf, &name);
    }
}

// WHY: `DepthPolicy::Raster` asks for the depth-tested picture outright, so the exporter must render the group
// exactly once, keeping its depth group so that the renderer tests depths rather than painting in order, at the
// export resolution, and must embed that render where the artists' vector geometry would have been. The report must
// give the user's request as the reason, not a finding about the figure, and name the option that made it.
#[test]
fn under_the_raster_policy_a_depth_group_is_rendered_once_with_its_depth_test_and_embedded() {
    require_tools!(IMAGE_TOOL, RASTER_TOOLS[0]);
    let ws = Workspace::new("depth-raster");
    let list = page_with_axes();
    let (mut stub, calls) = differing();
    let rendered = render_reporting(
        &list,
        &options(DepthPolicy::Raster, RasterPolicy::default(), 144.0),
        Some(&mut stub),
    );
    let pdf = ws.write_pdf("figure", &rendered.bytes);

    {
        let calls = calls.borrow();
        assert_eq!(calls.len(), 1, "one depth group, one render");
        let (planned, dpi) = &calls[0];
        assert_eq!(*dpi, 144.0, "the render is at the export resolution");
        assert_eq!(
            (planned.width_pt, planned.height_pt),
            (60.0, 20.0),
            "the list is the size of the artists' box, not of the page"
        );
        let root = root_items(planned);
        assert!(
            matches!(
                root,
                [Item {
                    kind: ItemKind::Depth { items },
                    ..
                }] if *items == artists_at(20.0, 10.0)
            ),
            "the root group holds the depth group with its items unchanged, not {root:?}"
        );
    }
    assert_warnings(
        &rendered.warnings,
        &[(AXES, ExportWarningKind::RasterisedByRequest)],
        "under Raster",
    );
    assert_message_names(
        &rendered.warnings[0],
        "`DepthPolicy::Raster`",
        "the request that forced the raster",
    );

    let images = pdfimages(&pdf);
    assert_eq!(images.len(), 1, "one image is embedded: {images:?}");
    assert!(
        (images[0].x_ppi - 144.0).abs() <= 1.0 && (images[0].y_ppi - 144.0).abs() <= 1.0,
        "the image is placed at 144 pixels per inch on the page, not {} by {}",
        images[0].x_ppi,
        images[0].y_ppi
    );
    let image = rasterise(&pdf, Engine::Poppler);
    assert_pixel(
        Engine::Poppler,
        &image,
        50.5,
        20.5,
        TESTED_PX,
        3,
        "the depth-tested render at the centre of the artists' box",
    );
    assert_pixel(
        Engine::Poppler,
        &image,
        81.5,
        20.5,
        WHITE_PX,
        3,
        "the page right of the box",
    );
}

// WHY: a depth group sits beneath the group that positions its axes on the page and the clip that binds it to its
// plot box, and a PDF image is drawn in the space of the enclosing transforms. The image must be placed by undoing
// them and rendered only for the part inside the clip, exactly as a dense raster is, and `dense.rs` proves the
// placement itself; what is particular to a depth group is that the list handed to the rasteriser must still hold
// the depth group beneath those transforms and clips, or the renderer would paint in order after all.
#[test]
fn under_the_raster_policy_the_image_is_placed_and_clipped_as_the_artists_would_have_been_with_its_depth_group_kept()
 {
    require_tools!(RASTER_TOOLS[0]);
    let ws = Workspace::new("depth-placement");
    // The clip is expressed in the parent space: it keeps the 30 points of the translated artists from 100 to 130.
    let list = holding(vec![group(
        Some(Rect::new(100.0, 40.0, 30.0, 100.0)),
        Some(Transform::translate(100.0, 40.0)),
        vec![depth_group(artists_at(0.0, 0.0))],
    )]);
    let (mut stub, calls) = differing();
    let rendered = render_reporting(&list, &depth_options(DepthPolicy::Raster), Some(&mut stub));
    let pdf = ws.write_pdf("figure", &rendered.bytes);

    {
        let calls = calls.borrow();
        assert_eq!(calls.len(), 1, "one depth group, one render");
        let planned = &calls[0].0;
        assert!(
            planned.width_pt <= 30.0,
            "the exporter renders only the 30 points of the artists inside the clip, not {} points",
            planned.width_pt
        );
        assert!(
            holds_depth(planned),
            "the render keeps the depth group beneath the transform and the clip"
        );
    }
    let image = rasterise(&pdf, Engine::Poppler);
    assert_pixel(
        Engine::Poppler,
        &image,
        115.5,
        50.5,
        TESTED_PX,
        3,
        "inside the clip, where the translated artists are",
    );
    assert_pixel(
        Engine::Poppler,
        &image,
        135.5,
        50.5,
        WHITE_PX,
        3,
        "beyond the clip, where the artists overlap",
    );
    assert_pixel(
        Engine::Poppler,
        &image,
        15.5,
        10.5,
        WHITE_PX,
        3,
        "where an untransformed raster would have landed",
    );
}

// WHY: `DepthPolicy::Auto` is the exporter finding out whether the painter's order is exact, and the only way it can
// is to render the group twice, once with its depth test and once without, and compare. When the two differ the
// vector page would be wrong, so the depth-tested render, and not the painter's-order one, must be what is embedded,
// at the export resolution, and the report must say that the figure, not the user, forced the raster, and name the
// option that overrides the finding.
#[test]
fn under_the_auto_policy_renders_that_differ_embed_the_depth_tested_one_and_report_it() {
    require_tools!(IMAGE_TOOL, RASTER_TOOLS[0]);
    let ws = Workspace::new("depth-auto-differ");
    let list = page_with_axes();
    let (mut stub, calls) = differing();
    let rendered = render_reporting(
        &list,
        &options(DepthPolicy::Auto, RasterPolicy::default(), 144.0),
        Some(&mut stub),
    );
    let pdf = ws.write_pdf("figure", &rendered.bytes);

    {
        let calls = calls.borrow();
        assert_eq!(
            calls.len(),
            2,
            "the group is rendered twice: with its depth test and without"
        );
        let ((tested, first_dpi), (painted, second_dpi)) = (&calls[0], &calls[1]);
        assert_eq!(
            (*first_dpi, *second_dpi),
            (144.0, 144.0),
            "both renders are at the export resolution"
        );
        assert!(
            holds_depth(tested),
            "the first render keeps the depth group, so that the renderer tests depths: {tested:?}"
        );
        assert_eq!(
            *painted,
            without_depth(tested),
            "the second render is the same list with the depth group unwrapped, so that it paints in order"
        );
    }
    assert_warnings(
        &rendered.warnings,
        &[(AXES, ExportWarningKind::RasterisedForDepth)],
        "under Auto with renders that differ",
    );
    assert_message_names(
        &rendered.warnings[0],
        "`DepthPolicy::Vector`",
        "the option that writes the axes as vectors regardless",
    );

    let images = pdfimages(&pdf);
    assert_eq!(images.len(), 1, "one image is embedded: {images:?}");
    let image = rasterise(&pdf, Engine::Poppler);
    assert_pixel(
        Engine::Poppler,
        &image,
        55.5,
        20.5,
        TESTED_PX,
        3,
        "the depth-tested render, not the painter's-order one, where the artists overlap",
    );
    assert_pixel(
        Engine::Poppler,
        &image,
        81.5,
        20.5,
        WHITE_PX,
        3,
        "the page right of the box",
    );
}

// WHY: when the two renders agree the painter's order is exact and the vector page is right, so the exporter must
// keep the vectors, which are smaller, sharper and editable, and embed nothing; and it must raise no warning,
// because a report that cried wolf on every three-dimensional figure would be ignored on the one that matters.
#[test]
fn under_the_auto_policy_renders_that_agree_keep_the_vectors_and_report_nothing() {
    require_tools!(IMAGE_TOOL, RASTER_TOOLS[0]);
    let ws = Workspace::new("depth-auto-agree");
    let list = page_with_axes();
    let (mut stub, calls) = identical();
    let rendered = render_reporting(&list, &depth_options(DepthPolicy::Auto), Some(&mut stub));
    let pdf = ws.write_pdf("figure", &rendered.bytes);

    assert_eq!(
        calls.borrow().len(),
        2,
        "the group is rendered twice to be compared"
    );
    assert!(
        rendered.warnings.is_empty(),
        "there is nothing to report: {:?}",
        rendered.warnings
    );
    assert_drawn_in_order_as_vectors(&pdf, "under Auto with renders that agree");
}

// WHY: the exporter compares the two renders with `same_picture`, not for equality: anti-aliasing leaves a sliver
// of difference along every shared edge of every render, so an exact comparison would rasterise every
// three-dimensional figure and single out none. A difference the size of a character, which is a misdrawn face or
// marker, must still force the raster. The tolerance is set in points, so this runs at 144 dots per inch, where one
// pixel is a small part of a 6 pt patch; at 72 dots per inch a single contrasting pixel already exceeds the budget
// of a patch, and a test there could not tell the comparison from equality.
#[test]
fn under_the_auto_policy_a_speck_of_difference_keeps_the_vectors_and_a_patch_of_it_forces_the_raster()
 {
    require_tools!(IMAGE_TOOL, RASTER_TOOLS[0]);
    let ws = Workspace::new("depth-auto-tolerance");
    let list = page_with_axes();
    let options = options(DepthPolicy::Auto, RasterPolicy::default(), 144.0);
    let patch_px = (SAME_PICTURE_PATCH_PT * 144.0 / 72.0).round() as u32;

    let (mut speck, calls) = Defective::new(1);
    let rendered = render_reporting(&list, &options, Some(&mut speck));
    let pdf = ws.write_pdf("speck", &rendered.bytes);
    assert_eq!(
        calls.borrow().len(),
        2,
        "the group is rendered twice to be compared"
    );
    assert!(
        rendered.warnings.is_empty(),
        "one pixel of difference is within the tolerance, so there is nothing to report: {:?}",
        rendered.warnings
    );
    assert_drawn_in_order_as_vectors(&pdf, "with a speck of difference");

    let (mut patch, calls) = Defective::new(patch_px);
    let rendered = render_reporting(&list, &options, Some(&mut patch));
    let pdf = ws.write_pdf("patch", &rendered.bytes);
    assert_eq!(
        calls.borrow().len(),
        2,
        "the group is rendered twice to be compared"
    );
    assert_warnings(
        &rendered.warnings,
        &[(AXES, ExportWarningKind::RasterisedForDepth)],
        "with a patch of difference",
    );
    assert_eq!(
        pdfimages(&pdf).len(),
        1,
        "the depth-tested render is embedded"
    );
}

// WHY: an axes that shows nothing on the page, because it holds no artist or because none of what it holds lies
// within its plot box or the page, has nothing to verify and nothing to rasterise: rendering it would spend a pass
// through the adapter on an empty image, and reporting it would tell the user of a problem the page cannot have.
// It must be drawn in order, so that whatever it holds beyond the clip stays in the file as the vectors it was,
// without a render, an image or a warning, whatever the policy and whether or not a rasteriser is there.
#[test]
fn a_depth_group_that_shows_nothing_is_drawn_in_order_without_a_render_or_a_warning() {
    require_tools!(IMAGE_TOOL);
    let ws = Workspace::new("depth-nothing");
    let cases = [
        ("empty", depth_group(vec![])),
        (
            "outside-its-clip",
            group(
                Some(Rect::new(0.0, 0.0, 10.0, 10.0)),
                None,
                vec![depth_group(artists_at(20.0, 10.0))],
            ),
        ),
        ("off-the-page", depth_group(artists_at(300.0, 10.0))),
    ];

    for (case, item) in cases {
        let list = holding(vec![item]);
        for policy in [DepthPolicy::Auto, DepthPolicy::Vector, DepthPolicy::Raster] {
            let name = format!("{case} under {policy:?}");
            let (mut stub, calls) = differing();
            let with = render_reporting(&list, &depth_options(policy), Some(&mut stub));
            let without = render_reporting(&list, &depth_options(policy), None);
            assert!(
                calls.borrow().is_empty(),
                "{name}: the rasteriser is never asked"
            );
            for (index, (environment, rendered)) in
                [("with a rasteriser", with), ("without one", without)]
                    .into_iter()
                    .enumerate()
            {
                assert!(
                    rendered.warnings.is_empty(),
                    "{name} {environment}: there is nothing to report: {:?}",
                    rendered.warnings
                );
                let pdf = ws.write_pdf(&format!("{case}-{policy:?}-{index}"), &rendered.bytes);
                assert!(
                    pdfimages(&pdf).is_empty(),
                    "{name} {environment}: nothing is embedded as an image"
                );
            }
        }
    }
}

// WHY: the scene compiler never puts a depth group beneath a transform that cannot be inverted, but a list built by
// hand can, and then there is nowhere to put an image: the image's rectangle is found in figure space and drawn
// beneath the inverse of the transform. The exporter must neither render, nor embed, nor fail, but draw the artists
// in order as it draws dense content in the same place, and it must not report a raster it did not make. The
// options still come first: under `DepthPolicy::Vector` the user is told, as ever, that their choice left the axes
// unverified, because the transform changes nothing about that.
#[test]
fn a_depth_group_beneath_a_transform_that_cannot_be_inverted_is_drawn_in_order_without_a_render() {
    require_tools!(IMAGE_TOOL);
    let ws = Workspace::new("depth-uninvertible");
    // A scale so small that its inverse overflows single precision, which is how a finite transform that the
    // painter accepts fails to invert.
    let list = holding(vec![group(
        None,
        Some(Transform {
            a: 1e-40,
            d: 1e-40,
            ..Transform::IDENTITY
        }),
        vec![depth_group(artists_at(20.0, 10.0))],
    )]);

    for policy in [DepthPolicy::Auto, DepthPolicy::Raster, DepthPolicy::Vector] {
        let name = format!("under {policy:?}");
        let (mut stub, calls) = differing();
        let rendered = render_reporting(&list, &depth_options(policy), Some(&mut stub));
        assert!(
            calls.borrow().is_empty(),
            "{name}: the rasteriser is never asked"
        );
        let expected: Vec<(NodeId, ExportWarningKind)> = if policy == DepthPolicy::Vector {
            vec![(
                AXES,
                ExportWarningKind::Unverified {
                    cause: UnverifiedCause::PolicyVector,
                },
            )]
        } else {
            vec![]
        };
        assert_warnings(&rendered.warnings, &expected, &name);
        let pdf = ws.write_pdf(&format!("figure-{policy:?}"), &rendered.bytes);
        assert!(
            pdfimages(&pdf).is_empty(),
            "{name}: nothing is embedded as an image"
        );
    }
}

// WHY: a depth group whose painter's order proved exact is drawn as vectors, and a surface inside it is still the
// dense artist it always was: the raster policy must apply to it exactly as it does outside a three-dimensional
// axes, with the same reasons reported, the same options named, and the surface, not the axes, named; otherwise a
// huge surface would be written as vectors just because its axes was three-dimensional, and a program reading the
// report would not know which artist became an image.
#[test]
fn a_dense_artist_inside_a_verified_depth_group_still_follows_the_raster_policy() {
    require_tools!(IMAGE_TOOL, RASTER_TOOLS[0]);
    let ws = Workspace::new("depth-dense");
    let page_with_surface = |cells: u64| {
        holding(vec![depth_group(vec![
            dense(cells, square()),
            deep_rect(Rect::new(50.0, 10.0, 30.0, 20.0), BLUE, 0.0),
        ])])
    };
    let cases = [
        (
            "always",
            1,
            RasterPolicy::Always,
            Some((
                ExportWarningKind::RasterisedByRequest,
                "`RasterPolicy::Always`",
            )),
        ),
        (
            "at-the-threshold",
            100,
            RasterPolicy::Auto { cells: 100 },
            Some((
                ExportWarningKind::RasterisedForSize { cells: 100 },
                "`RasterPolicy::Never`",
            )),
        ),
        (
            "below-the-threshold",
            99,
            RasterPolicy::Auto { cells: 100 },
            None,
        ),
        ("never", 1_000_000, RasterPolicy::Never, None),
    ];

    for (name, cells, policy, expected) in cases {
        let (mut stub, calls) = identical();
        let rendered = render_reporting(
            &page_with_surface(cells),
            &options(DepthPolicy::Auto, policy, 72.0),
            Some(&mut stub),
        );
        let pdf = ws.write_pdf(&format!("figure-{name}"), &rendered.bytes);
        let rasterised = expected.is_some();

        assert_eq!(
            calls.borrow().len(),
            2 + usize::from(rasterised),
            "{name}: two renders verify the group, and one more draws the surface only when it is rasterised"
        );
        let want: Vec<_> = expected
            .iter()
            .map(|(kind, _)| (SURFACE, kind.clone()))
            .collect();
        assert_warnings(&rendered.warnings, &want, name);
        if let Some((_, option)) = &expected {
            assert_message_names(
                &rendered.warnings[0],
                option,
                &format!("{name}: the option behind the raster"),
            );
        }
        assert_eq!(
            pdfimages(&pdf).len(),
            usize::from(rasterised),
            "{name}: images embedded"
        );
        let image = rasterise(&pdf, Engine::Poppler);
        assert_pixel(
            Engine::Poppler,
            &image,
            30.5,
            20.5,
            if rasterised { TESTED_PX } else { RED_PX },
            3,
            &format!("{name}: the surface is drawn either way"),
        );
        assert_pixel(
            Engine::Poppler,
            &image,
            55.5,
            20.5,
            BLUE_PX,
            3,
            &format!("{name}: the artist painted after the surface still covers it"),
        );
    }
}

// WHY: the report is a list in the order the exporter painted, so that a program can relate each warning to the
// page's stacking and a reader can follow the page from top to bottom; a report gathered by kind, or with the axes
// collected before the dense artists, would still name the right nodes but would not tell which came first. An
// axes drawn unverified and a surface rasterised for its size on one page must therefore be reported in the order
// they were painted, whichever came first.
#[test]
fn the_warnings_of_one_page_arrive_in_painting_order() {
    let axes = || depth_group(artists_at(20.0, 10.0));
    let surface = || {
        dense(
            100,
            vec![filled_rect(Rect::new(100.0, 50.0, 40.0, 20.0), RED)],
        )
    };
    let unverified = (
        AXES,
        ExportWarningKind::Unverified {
            cause: UnverifiedCause::PolicyVector,
        },
    );
    let for_size = (SURFACE, ExportWarningKind::RasterisedForSize { cells: 100 });
    let options = options(DepthPolicy::Vector, RasterPolicy::Auto { cells: 100 }, 72.0);

    for (name, items, expected) in [
        (
            "the axes then the surface",
            vec![axes(), surface()],
            [unverified.clone(), for_size.clone()],
        ),
        (
            "the surface then the axes",
            vec![surface(), axes()],
            [for_size.clone(), unverified.clone()],
        ),
    ] {
        let (mut stub, calls) = differing();
        let rendered = render_reporting(&holding(items), &options, Some(&mut stub));
        assert_eq!(
            calls.borrow().len(),
            1,
            "{name}: only the surface is rendered, since the policy keeps the axes vector"
        );
        assert_warnings(&rendered.warnings, &expected, name);
    }
}

// WHY: a caller decides from this answer whether to open a graphics adapter before exporting, so a wrong answer
// either opens an adapter that a wholly vector figure never needed, or exports a three-dimensional figure unchecked
// without knowing it. The three answers must therefore be told apart exactly: nothing for a flat page, for a depth
// group the user keeps vector, or for a resolution at which no image can be made; a check for a depth group left to
// the exporter; and a render whenever the policy will put pixels on the page, which wins over a check because a
// render needs the adapter anyway, and holds however the depth policy is set.
#[test]
fn needs_rasteriser_tells_a_check_from_a_render_from_nothing() {
    let under = |depth: DepthPolicy| RasterOptions {
        depth,
        ..RasterOptions::default()
    };
    let at_threshold = |depth: DepthPolicy| RasterOptions {
        policy: RasterPolicy::Auto { cells: 100 },
        dpi: 72.0,
        depth,
    };

    assert_eq!(
        needs_rasteriser(&page(200.0, 100.0), &RasterOptions::default()),
        Need::No,
        "an empty page"
    );
    assert_eq!(
        needs_rasteriser(&holding(square()), &RasterOptions::default()),
        Need::No,
        "vector geometry outside any depth group"
    );
    assert_eq!(
        needs_rasteriser(&page_with_axes(), &under(DepthPolicy::Auto)),
        Need::ToVerify,
        "a depth group left to the exporter"
    );
    assert_eq!(
        needs_rasteriser(&page_with_axes(), &under(DepthPolicy::Vector)),
        Need::No,
        "a depth group the user keeps vector"
    );
    assert_eq!(
        needs_rasteriser(&page_with_axes(), &under(DepthPolicy::Raster)),
        Need::ToDraw,
        "a depth group the user rasterises"
    );
    for policy in [DepthPolicy::Auto, DepthPolicy::Raster] {
        assert_eq!(
            needs_rasteriser(
                &page_with_axes(),
                &RasterOptions {
                    dpi: f64::NAN,
                    ..under(policy)
                }
            ),
            Need::No,
            "a depth group under {policy:?} at a resolution that is not finite, where no image can be made"
        );
    }
    assert_eq!(
        needs_rasteriser(
            &holding(vec![dense(100, square())]),
            &at_threshold(DepthPolicy::Auto)
        ),
        Need::ToDraw,
        "dense content at the threshold"
    );
    assert_eq!(
        needs_rasteriser(
            &holding(vec![dense(99, square())]),
            &at_threshold(DepthPolicy::Auto)
        ),
        Need::No,
        "dense content below the threshold"
    );
    assert_eq!(
        needs_rasteriser(
            &holding(vec![
                depth_group(artists_at(20.0, 10.0)),
                dense(100, square())
            ]),
            &at_threshold(DepthPolicy::Auto)
        ),
        Need::ToDraw,
        "dense content at the threshold beside a depth group left to the exporter"
    );
    assert_eq!(
        needs_rasteriser(
            &holding(vec![depth_group(vec![dense(100, square())])]),
            &at_threshold(DepthPolicy::Auto)
        ),
        Need::ToDraw,
        "dense content at the threshold inside a depth group left to the exporter"
    );
    assert_eq!(
        needs_rasteriser(
            &holding(vec![
                depth_group(artists_at(20.0, 10.0)),
                dense(99, square())
            ]),
            &at_threshold(DepthPolicy::Auto)
        ),
        Need::ToVerify,
        "dense content below the threshold beside a depth group left to the exporter"
    );
    assert_eq!(
        needs_rasteriser(
            &holding(vec![
                depth_group(artists_at(20.0, 10.0)),
                dense(100, square())
            ]),
            &at_threshold(DepthPolicy::Vector)
        ),
        Need::ToDraw,
        "dense content at the threshold beside a depth group the user keeps vector"
    );
}

// WHY: `rasterises_any` answers one question, whether any dense artist would become an image, which a caller uses
// to tell a raster forced by size or by request from the check or the raster of a three-dimensional axes, for
// which `needs_rasteriser` is the wider answer. A depth group under `DepthPolicy::Raster` must therefore leave it
// false, a dense artist inside a depth group must count as the dense content it is, and a resolution that is not
// finite must leave it false however dense the content, because no image can be made at it.
#[test]
fn rasterises_any_speaks_of_dense_content_at_a_usable_resolution_only() {
    let at_threshold = RasterOptions {
        policy: RasterPolicy::Auto { cells: 100 },
        dpi: 72.0,
        depth: DepthPolicy::Auto,
    };

    assert!(
        rasterises_any(&holding(vec![dense(100, square())]), &at_threshold),
        "dense content at the threshold"
    );
    assert!(
        !rasterises_any(&holding(vec![dense(99, square())]), &at_threshold),
        "dense content below the threshold"
    );
    assert!(
        rasterises_any(
            &holding(vec![depth_group(vec![dense(100, square())])]),
            &at_threshold
        ),
        "dense content at the threshold inside a depth group"
    );
    assert!(
        !rasterises_any(
            &page_with_axes(),
            &RasterOptions {
                depth: DepthPolicy::Raster,
                ..RasterOptions::default()
            }
        ),
        "a depth group the user rasterises is not dense content"
    );
    assert!(
        !rasterises_any(
            &holding(vec![dense(100, square())]),
            &RasterOptions {
                dpi: f64::NAN,
                ..at_threshold
            }
        ),
        "no image can be made at a resolution that is not finite"
    );
}

// WHY: a rasteriser that was asked for the depth-tested picture and could not produce it has drawn nothing, and
// quietly falling back to the painter's order would hand the user an unchecked page of a different kind from the
// one they asked for. That holds under `Auto` as much as under `Raster`: a device that fails is a fault, not an
// absence, unlike the missing rasteriser that `Auto` tolerates with a warning. The error must repeat the cause and
// name the way out, which for a depth group is the vector depth policy, not the raster policy that governs dense
// content.
#[test]
fn a_rasteriser_that_fails_on_a_depth_group_is_an_error_naming_the_vector_policy_as_the_way_out() {
    let text = engine();
    for policy in [DepthPolicy::Raster, DepthPolicy::Auto] {
        let mut failing = Failing;
        let result = render_display_list(
            &page_with_axes(),
            &text,
            &depth_options(policy),
            Some(&mut failing),
        );
        let Err(error) = result else {
            panic!("{policy:?}: a failing rasteriser must not be ignored")
        };

        assert!(
            matches!(error, PdfError::Raster(_)),
            "{policy:?}: got {error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains("no adapter") && message.contains("`DepthPolicy::Vector`"),
            "{policy:?}: the message repeats the cause and names the way out: {message}"
        );
    }
}

// WHY: the comparison must admit what anti-aliasing does, a sliver of a pixel or so along every shared edge of the
// faces, and refuse what a wrong painter's order does, a whole face, marker or stretch of line in the wrong
// colour; and it must do so at the export resolution, where the tolerance is a fraction of a 6 pt patch and not of
// a pixel. At 600 dots per inch a patch is 50 pixels square with a budget of 4 levels per pixel and channel: a
// one-pixel column of the contrasting colour along a 6 pt edge, 50 pixels of 765 summed over four channels, fits
// it and a two-pixel column does not, while a patch that changes colour is refused outright. Images of different
// sizes are never the same picture, whatever they hold, because nothing says which pixels correspond.
#[test]
fn same_picture_admits_a_sliver_along_an_edge_and_refuses_a_patch_that_changes() {
    let dpi = 600.0;
    let patch = (SAME_PICTURE_PATCH_PT * dpi / 72.0).round() as u32;
    assert_eq!(
        patch, 50,
        "a 6 pt patch at 600 dots per inch is 50 pixels square"
    );
    let budget = SAME_PICTURE_TOLERANCE * 4.0 * f64::from(patch * patch);
    assert!(
        f64::from(50 * 765) <= budget && f64::from(100 * 765) > budget,
        "one column of a patch fits the budget of {budget} and two do not"
    );
    let image = flat(patch * 4, patch * 2, TESTED);

    assert!(
        same_picture(&image, &image, dpi),
        "an image is the same picture as itself"
    );
    assert!(
        !same_picture(&image, &flat(patch * 4, patch * 2 + 1, TESTED), dpi),
        "images of different sizes are never the same picture"
    );
    let one_column = with_pixels(&image, block(patch, 0, 1, patch), PAINTED);
    assert!(
        same_picture(&image, &one_column, dpi),
        "a one-pixel column along the edge of a patch is within the tolerance"
    );
    let two_columns = with_pixels(&image, block(patch, 0, 2, patch), PAINTED);
    assert!(
        !same_picture(&image, &two_columns, dpi),
        "a two-pixel column is beyond it"
    );
    let whole_patch = with_pixels(&image, block(patch, 0, patch, patch), PAINTED);
    assert!(
        !same_picture(&image, &whole_patch, dpi),
        "a patch that changes colour is a different picture"
    );
}

// WHY: the patches tile the image from its top-left corner, and an image whose width the tiling does not divide
// would leave a sliver of a patch along its right edge, judged by a budget as thin as itself, so that a one-pixel
// column there, the very sliver anti-aliasing leaves along the edge of a plot box, would fail and force a raster
// for nothing. The last patch of a row or column is therefore pulled back to the edge so that it is a whole patch:
// with an image 60 pixels wide at 600 dots per inch, the last patch is the 50 pixels from 10 to 60, and a one-pixel
// column in the last column of the image passes as it does anywhere else, where the 10-pixel sliver from 50 to 60
// would have refused it.
#[test]
fn same_picture_judges_the_edge_of_an_image_by_a_whole_patch_pulled_back_to_it() {
    let dpi = 600.0;
    let image = flat(60, 50, TESTED);

    let last_column = with_pixels(&image, block(59, 0, 1, 50), PAINTED);
    assert!(
        same_picture(&image, &last_column, dpi),
        "a one-pixel column in the last column of the image is within the tolerance of a whole patch"
    );
    let two_columns = with_pixels(&image, block(58, 0, 2, 50), PAINTED);
    assert!(
        !same_picture(&image, &two_columns, dpi),
        "two columns there are beyond it, as they are anywhere else"
    );
}

// WHY: an image smaller than a patch, such as the render of a tiny axes, is compared as one patch, so that its
// budget is the tolerance times its own pixels and not the budget of a patch it does not fill; a smaller image must
// not be judged more leniently than the picture it is part of. Twenty pixels square at 600 dots per inch is one
// patch of 400 pixels with a budget of 6400 summed over four channels, which eight contrasting pixels, 6120, fit
// and nine, 6885, do not.
#[test]
fn same_picture_treats_an_image_smaller_than_a_patch_as_one_patch() {
    let dpi = 600.0;
    let image = flat(20, 20, TESTED);

    let eight = with_pixels(&image, block(0, 0, 8, 1), PAINTED);
    assert!(
        same_picture(&image, &eight, dpi),
        "eight contrasting pixels of four hundred fit the budget"
    );
    let nine = with_pixels(&image, block(0, 0, 9, 1), PAINTED);
    assert!(!same_picture(&image, &nine, dpi), "nine do not");
}
