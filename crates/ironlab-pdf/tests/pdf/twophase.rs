//! Two-phase export: recording the rasteriser's requests, answering them elsewhere and replaying the answers.
//!
//! A browser cannot block on a GPU readback in the middle of the exporter's walk, so the export is split: a first
//! pass records every list the walk would hand to a rasteriser, the caller renders them at its leisure, and a final
//! pass replays the images into the same walk. The invariant that makes this sound is that the walk's requests
//! depend on the images only through the comparison of a three-dimensional axes' two renders: renders that show the
//! same picture descend into the axes and renders that differ stop there. A recording pass that answers every
//! request with one identical placeholder therefore descends as far as any real export ever does, and its sequence
//! of requests has every real export's sequence as an in-order subsequence.
//!
//! The rasteriser is the stub of `common.rs`, which paints a flat colour and records what it was asked to render,
//! so that the two renders of an axes can be made to agree or differ at will. These tests compare the bytes and
//! warnings of the two-phase export with those of the single-pass export of the same list, which krilla writes
//! deterministically; `export.rs` relies on the same determinism.

use ironlab_ir::NodeId;
use ironlab_pdf::raster::{Need, needs_rasteriser};
use ironlab_pdf::{
    DepthPolicy, ExportWarningKind, PdfError, PdfOptions, RasterImage, RasterOptions, RasterPolicy,
    RasterRequest, Rasteriser, Rendered, raster_requests, render_display_list, render_with_rasters,
};
use ironlab_scene::display::{DisplayList, Item, ItemKind, Rect, Rgba};

use crate::common::*;

/// The identifier of the dense artist drawn directly on the page.
const SURFACE_ON_PAGE: NodeId = NodeId(3);
/// The identifier of the dense artist drawn inside the second three-dimensional axes.
const SURFACE_IN_AXES: NodeId = NodeId(4);
/// The identifiers of the two three-dimensional axes.
const AXES_B: NodeId = NodeId(7);
const AXES_C: NodeId = NodeId(8);

/// The threshold at which a dense artist is rasterised in these tests; every dense group here is drawn at it.
const CELLS: u64 = 100;

/// What the stub paints for a render that holds a depth group, standing for a depth-tested picture, and what it
/// paints for one without, standing for a painter's-order picture, when the two are made to differ.
const TESTED: [u8; 4] = [255, 0, 255, 255];
const PAINTED: [u8; 4] = [0, 255, 0, 255];

/// A stub whose two renders of an axes are identical, as a renderer's are when the painter's order is exact.
fn agreeing() -> (Stub, Calls) {
    Stub::new(TESTED, TESTED)
}

/// A stub whose two renders differ everywhere, as a renderer's do when the artists overlap out of order.
fn differing() -> (Stub, Calls) {
    Stub::new(TESTED, PAINTED)
}

/// A dense group of `cells` cells drawn for the artist `node`, holding `items`.
///
/// `common::dense` names every dense group `SURFACE`; the page here holds two dense artists whose reports must be
/// told apart, so each is given its own node.
fn dense_for(node: NodeId, cells: u64, items: Vec<Item>) -> Item {
    Item {
        source: Some(node),
        kind: ItemKind::Dense { cells, items },
    }
}

/// The depth group of the three-dimensional axes `node`, holding `items`.
fn depth_for(node: NodeId, items: Vec<Item>) -> Item {
    Item {
        source: Some(node),
        kind: ItemKind::Depth { items },
    }
}

/// A `first` rectangle 40 by 20 points and a `second` one 30 by 20 points whose left third lies over the first
/// one's right quarter, with their box's top-left corner at `(x, y)`: the artists of one three-dimensional axes.
/// The painter never reads a path's depth, so the rectangles carry none.
fn artists_at(x: f64, y: f64, first: Rgba, second: Rgba) -> Vec<Item> {
    vec![
        filled_rect(Rect::new(x, y, 40.0, 20.0), first),
        filled_rect(Rect::new(x + 30.0, y, 30.0, 20.0), second),
    ]
}

/// A 300 by 100 point page holding, left to right: a dense artist at the raster threshold; the axes `AXES_B`; and
/// the axes `AXES_C`, whose first artist is a dense one at the threshold. Every dense artist and first rectangle is
/// `primary` and every second rectangle `secondary`, so that two pages of the same layout in different colours
/// plan lists that are equal in shape and nowhere equal in content.
///
/// When every pair of renders agrees, the single pass asks for six renders in this order: the dense artist on the
/// page (1), the depth-tested and painter's-order renders of `AXES_B` (2), the same two of `AXES_C` (2), and, once
/// those prove the order exact and the walk descends into `AXES_C`, the dense artist inside it (1).
fn page_with_everything_in(primary: Rgba, secondary: Rgba) -> DisplayList {
    let mut list = page(300.0, 100.0);
    list.items = vec![
        dense_for(
            SURFACE_ON_PAGE,
            CELLS,
            vec![filled_rect(Rect::new(20.0, 10.0, 40.0, 20.0), primary)],
        ),
        depth_for(AXES_B, artists_at(100.0, 10.0, primary, secondary)),
        depth_for(
            AXES_C,
            vec![
                dense_for(
                    SURFACE_IN_AXES,
                    CELLS,
                    vec![filled_rect(Rect::new(200.0, 10.0, 40.0, 20.0), primary)],
                ),
                filled_rect(Rect::new(230.0, 10.0, 30.0, 20.0), secondary),
            ],
        ),
    ];
    list
}

/// The page of [`page_with_everything_in`] in red and blue.
fn page_with_everything() -> DisplayList {
    page_with_everything_in(RED, BLUE)
}

/// The number of renders the single pass asks for over `page_with_everything` when every pair agrees.
const REQUESTS_WHEN_AGREEING: usize = 1 + 2 + 2 + 1;

/// The number it asks for when every pair differs: the same, less the dense artist inside `AXES_C`, which the walk
/// never reaches once the axes' renders differ.
const REQUESTS_WHEN_DIFFERING: usize = 1 + 2 + 2;

/// Options leaving both decisions to the exporter, rasterising dense content at `CELLS` cells and rendering at
/// `dpi`.
fn auto(dpi: f64) -> PdfOptions {
    PdfOptions {
        raster: RasterOptions {
            policy: RasterPolicy::Auto { cells: CELLS },
            dpi,
            depth: DepthPolicy::Auto,
        },
        ..PdfOptions::default()
    }
}

/// Phase 2 as a caller performs it: renders each recorded request, in order, with `rasteriser`, so that the image
/// at each index answers the request at the same index.
fn answer(requests: &[RasterRequest], rasteriser: &mut dyn Rasteriser) -> Vec<RasterImage> {
    requests
        .iter()
        .map(|request| {
            rasteriser
                .rasterise(&request.list, request.dpi)
                .expect("the stub renders every request")
        })
        .collect()
}

/// Runs both phases of the two-phase export over `list` with `rasteriser` answering the recorded requests, and
/// returns the requests with the page they produced.
fn export_in_two_phases(
    list: &DisplayList,
    options: &PdfOptions,
    rasteriser: &mut dyn Rasteriser,
) -> (Vec<RasterRequest>, Rendered) {
    let text = engine();
    let requests = raster_requests(list, &text, options).expect("record the requests");
    let rasters = answer(&requests, rasteriser);
    let rendered =
        render_with_rasters(list, &text, options, &requests, rasters).expect("replay the rasters");
    (requests, rendered)
}

// WHY: the two-phase export exists so that a caller who cannot render inside the walk still gets exactly the page
// the single pass would have written, bytes and warnings alike; anything else would make the browser's export a
// different document from the desktop's. This page holds every kind of request the walk can make, a dense artist,
// an axes verified as vectors, and an axes whose children are themselves rasterised, so that the recording must
// plan renders of three kinds and the replay must feed each to the right place: six in all, one for the dense
// artist, two for each axes and one for the dense artist inside the second axes, which is reached only because the
// second axes' renders agree.
#[test]
fn recording_then_replay_writes_the_bytes_and_warnings_of_a_single_pass() {
    let list = page_with_everything();
    let options = auto(72.0);

    let (mut single, calls) = agreeing();
    let expected = render_reporting(&list, &options, Some(&mut single));
    assert_eq!(
        calls.borrow().len(),
        REQUESTS_WHEN_AGREEING,
        "the single pass asks for one dense render, two per axes and one for the dense artist inside the \
         second axes"
    );
    assert_warnings(
        &expected.warnings,
        &[
            (
                SURFACE_ON_PAGE,
                ExportWarningKind::RasterisedForSize { cells: CELLS },
            ),
            (
                SURFACE_IN_AXES,
                ExportWarningKind::RasterisedForSize { cells: CELLS },
            ),
        ],
        "the single pass rasterises both dense artists and keeps both axes vector",
    );

    let (mut answering, _) = agreeing();
    let (requests, rendered) = export_in_two_phases(&list, &options, &mut answering);
    assert_eq!(
        requests.len(),
        REQUESTS_WHEN_AGREEING,
        "the recording plans 1 + 2 + 2 + 1 renders: the dense artist, two per axes, and the dense artist inside \
         the second axes"
    );
    assert_eq!(
        rendered, expected,
        "the replay writes the same bytes and reports the same warnings as the single pass"
    );
}

// WHY: the recording answers every request with one placeholder, so it always takes the branch that descends, and
// plans renders beneath an axes that a real export never asks for once the axes' renders differ. The replay must
// tolerate that: it must skip the planned requests the walk does not make, rather than fail or, worse, hand the
// image planned for the dense artist inside the axes to the next request that comes along. The page must then be
// the single pass's page, with the second axes embedded for depth and nothing reported about the dense artist
// beneath it, which never became an image.
#[test]
fn a_verification_that_fails_skips_the_requests_beneath_it_and_the_replay_tolerates_them() {
    let list = page_with_everything();
    let options = auto(72.0);

    let (mut single, calls) = differing();
    let expected = render_reporting(&list, &options, Some(&mut single));
    assert_eq!(
        calls.borrow().len(),
        REQUESTS_WHEN_DIFFERING,
        "the single pass asks for 1 + 2 + 2 renders and never reaches the dense artist inside the second axes"
    );
    assert_warnings(
        &expected.warnings,
        &[
            (
                SURFACE_ON_PAGE,
                ExportWarningKind::RasterisedForSize { cells: CELLS },
            ),
            (AXES_B, ExportWarningKind::RasterisedForDepth),
            (AXES_C, ExportWarningKind::RasterisedForDepth),
        ],
        "both axes are embedded for depth and the dense artist inside the second is not reported",
    );

    let (mut answering, _) = differing();
    let (requests, rendered) = export_in_two_phases(&list, &options, &mut answering);
    assert_eq!(
        requests.len(),
        REQUESTS_WHEN_AGREEING,
        "the recording still plans all six renders, because its placeholder makes every pair agree"
    );
    assert_eq!(
        rendered, expected,
        "the replay skips the planned render of the dense artist inside the second axes and writes the single \
         pass's page"
    );
}

// WHY: the replay answers each request with the image planned for it, and a request the recording did not plan has
// no image to answer it.
// Guessing, by handing over the next image or by drawing the content as vectors, would put the wrong picture on
// the page or quietly change its kind, so it must be an error naming the invariant that was broken. That holds when
// nothing was planned at all, since an empty plan against a page that rasterises is a caller's mistake and not a
// request for a vector page, which `render_display_list` without a rasteriser already provides; and it holds when
// the plan is another page's, even one of the same shape whose every request has an image of the right size, which
// a replay that matched by position alone would embed without complaint.
#[test]
fn a_request_the_recording_did_not_plan_is_an_error() {
    let text = engine();
    let list = page_with_everything();
    let options = auto(72.0);

    let other = page_with_everything_in(BLUE, RED);
    let planned_for_other = raster_requests(&other, &text, &options).expect("record the requests");
    assert_eq!(
        planned_for_other.len(),
        REQUESTS_WHEN_AGREEING,
        "the other page plans as many renders as this one"
    );
    let planned_here = raster_requests(&list, &text, &options).expect("record the requests");
    assert!(
        planned_here
            .iter()
            .zip(&planned_for_other)
            .all(|(here, there)| here != there),
        "no request of the other page is a request of this one"
    );
    let (mut stub, _) = agreeing();
    let rasters_for_other = answer(&planned_for_other, &mut stub);

    let cases: [(&str, &[RasterRequest], Vec<RasterImage>); 2] = [
        ("an empty plan", &[], Vec::new()),
        (
            "the plan of a different page",
            &planned_for_other,
            rasters_for_other,
        ),
    ];
    for (name, requests, rasters) in cases {
        let result = render_with_rasters(&list, &text, &options, requests, rasters);
        let Err(error) = result else {
            panic!("{name}: a request that was not planned must not be answered by guesswork")
        };
        assert!(
            matches!(error, PdfError::Raster(_)),
            "{name}: got {error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains("not planned"),
            "{name}: the message names the broken invariant: {message}"
        );
    }
}

// WHY: a caller decides from the plan whether to open a graphics adapter at all, so a page that needs no
// rasteriser must plan nothing, without running the export to find out; and the replay of an empty plan must be
// the export without a rasteriser, warnings included, so that the two-phase path is a complete substitute for the
// single pass on every page and not only on the ones that rasterise. The three pages here are the three ways
// `needs_rasteriser` answers `No`: nothing dense or three-dimensional, dense content below the threshold, and a
// three-dimensional axes the user keeps vector.
#[test]
fn no_requests_are_planned_when_no_rasteriser_is_needed() {
    let text = engine();
    let auto_options = auto(72.0);
    let vector_options = PdfOptions {
        raster: RasterOptions {
            depth: DepthPolicy::Vector,
            ..auto_options.raster
        },
        ..PdfOptions::default()
    };
    let flat = {
        let mut list = page(200.0, 100.0);
        list.items = vec![filled_rect(Rect::new(20.0, 10.0, 40.0, 20.0), RED)];
        list
    };
    let below_threshold = {
        let mut list = page(200.0, 100.0);
        list.items = vec![dense_for(
            SURFACE_ON_PAGE,
            CELLS - 1,
            vec![filled_rect(Rect::new(20.0, 10.0, 40.0, 20.0), RED)],
        )];
        list
    };
    let kept_vector = {
        let mut list = page(200.0, 100.0);
        list.items = vec![depth_for(AXES_B, artists_at(20.0, 10.0, RED, BLUE))];
        list
    };

    for (name, list, options) in [
        ("a page of plain vector geometry", &flat, &auto_options),
        (
            "a dense artist below the threshold",
            &below_threshold,
            &auto_options,
        ),
        (
            "a three-dimensional axes the user keeps vector",
            &kept_vector,
            &vector_options,
        ),
    ] {
        assert_eq!(
            needs_rasteriser(list, &options.raster),
            Need::No,
            "{name}: the premise of this case is that no rasteriser is needed"
        );
        let requests = raster_requests(list, &text, options).expect("record the requests");
        assert!(
            requests.is_empty(),
            "{name}: nothing is planned, not {requests:?}"
        );
        let replayed = render_with_rasters(list, &text, options, &requests, Vec::new())
            .expect("replay an empty plan");
        let without =
            render_display_list(list, &text, options, None).expect("render without a rasteriser");
        assert_eq!(
            replayed, without,
            "{name}: the replay of an empty plan is the export without a rasteriser"
        );
    }
}

// WHY: the caller renders exactly what the recording hands it, so the recorded requests must be the very lists and
// resolutions the single pass would have handed a rasteriser, in the same order: a list still in figure
// coordinates, a resolution other than the export's, or a request out of order would make the caller render the
// wrong picture and the replay embed it without complaint. The single pass is the reference, through the stub that
// records what it is asked for, on the page whose every verification agrees, so that the two sequences are
// complete and must match element for element.
#[test]
fn the_recorded_requests_are_the_lists_and_resolutions_the_single_pass_asked_for() {
    let text = engine();
    let list = page_with_everything();
    let options = auto(144.0);

    let (mut stub, calls) = agreeing();
    render_reporting(&list, &options, Some(&mut stub));
    let asked_for: Vec<RasterRequest> = calls
        .borrow()
        .iter()
        .map(|(list, dpi)| RasterRequest {
            list: list.clone(),
            dpi: *dpi,
        })
        .collect();
    assert_eq!(
        asked_for.len(),
        REQUESTS_WHEN_AGREEING,
        "the single pass asks for every render this page can need"
    );

    let requests = raster_requests(&list, &text, &options).expect("record the requests");
    assert_eq!(
        requests, asked_for,
        "the recording plans exactly the lists, at exactly the resolution, in exactly the order the single \
         pass asked for"
    );
    assert!(
        requests.iter().all(|request| request.dpi == 144.0),
        "every request is at the export resolution"
    );
}

// WHY: the replay pairs each image with the request it answers by position, so a plan of N requests given any other
// number of images has either a request with no image or an image with no request, and nothing says which one is
// missing or surplus. Guessing, by answering the requests it can and drawing the rest as vectors, would quietly change
// the kind of part of the page; so the mismatch must be an error before anything is drawn, naming both counts so that
// a caller who dropped or duplicated a render can see which it did.
#[test]
fn the_rasters_must_answer_the_requests_one_for_one() {
    let text = engine();
    let list = page_with_everything();
    let options = auto(72.0);
    let requests = raster_requests(&list, &text, &options).expect("record the requests");
    assert_eq!(
        requests.len(),
        REQUESTS_WHEN_AGREEING,
        "the premise of this test is a plan of several requests"
    );
    let (mut stub, _) = agreeing();
    let rasters = answer(&requests, &mut stub);

    let one_short = rasters[..rasters.len() - 1].to_vec();
    let mut one_over = rasters.clone();
    one_over.push(rasters[0].clone());
    for (name, rasters) in [
        ("one raster short", one_short),
        ("one raster over", one_over),
    ] {
        let given = rasters.len();
        let result = render_with_rasters(&list, &text, &options, &requests, rasters);
        let Err(error) = result else {
            panic!("{name}: a plan and its rasters that do not pair off must not be replayed")
        };
        assert!(
            matches!(error, PdfError::Raster(_)),
            "{name}: got {error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains(&requests.len().to_string()) && message.contains(&given.to_string()),
            "{name}: the message names both counts, {} planned and {given} given: {message}",
            requests.len()
        );
    }
}
