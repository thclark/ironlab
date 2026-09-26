//! Export in two phases: recording the renders a page needs, and replaying the images into the same export.
//!
//! [`render_display_list`] asks its [`Rasteriser`] for each image while it walks the display list, and waits for
//! the answer before it draws on. A caller that cannot wait for its renderer inside that walk, such as a browser,
//! which may not block on a GPU readback, splits the export into three phases instead: [`raster_requests`] records
//! every list the walk would hand to a rasteriser, with the resolution to render it at (phase 1); the caller renders
//! each request however and whenever it can (phase 2); and [`render_with_rasters`] runs the export again with the
//! images answering the requests (phase 3). The viewer takes this path on every target, so that one export produces
//! one document wherever it runs.
//!
//! # Why the recording is complete
//!
//! The walk's decisions, and therefore its sequence of requests, depend on the images it is given in exactly one
//! place: under [`DepthPolicy::Auto`], the two renders of a three-dimensional axes are compared with
//! [`same_picture`], and renders that show the same picture make the walk descend into the axes, where it may make
//! further requests, while renders that differ make it embed the depth-tested render and stop. Nowhere else does an
//! image change what is asked for next: a dense group is rendered once and drawn, and a group's placement is
//! computed from its geometry before it is rendered. The recording therefore answers every request with one
//! placeholder, a single transparent pixel, which `same_picture` reports equal to itself and which the exporter
//! accepts as an image, so the recording descends into every axes it ever could and records a sequence of which
//! every real export's sequence is an in-order subsequence. This holds for any rasteriser that returns a well-formed
//! image or an error for every request, which the viewer's renderer does; a rasteriser that returned a malformed
//! image for a dense group would make the exporter draw the group's children as vectors, and the recording would
//! not have descended into them.
//!
//! # Why the replay matches by content
//!
//! The replay holds the recorded requests and their images, in order, with a cursor. Each request the export makes
//! is matched against the recorded requests from the cursor onwards, by the equality of its list and its
//! resolution, and the first that matches answers it; the cursor then moves past that request. Recorded requests the
//! cursor skips over were planned beneath an axes whose renders differed, and are never made. Matching by content
//! rather than by position is what makes the skipping safe: a request planned for a dense artist inside such an
//! axes can never be handed to the next request that comes along. It is also what makes the replay refuse another
//! page's plan: the rendered list of a request is expressed in the image's own coordinates, so a page and a
//! translated copy of it plan equal lists, and content, not position on the page, is what identifies a request. A
//! request that matches no recorded request from the cursor onwards is an error, because the recording and the
//! replay were not made for the same list and options.

use ironlab_scene::display::DisplayList;
use ironlab_text::TextEngine;

use crate::raster::{Need, RasterImage, Rasteriser, needs_rasteriser};
use crate::{PdfError, PdfOptions, Rendered, render_display_list};

#[cfg(doc)]
use crate::raster::{DepthPolicy, same_picture};

/// One raster the exporter asks for: the list to draw and the resolution in dots per inch.
///
/// The list is in the image's own coordinates, one point of which is one point of the page, and the image it
/// expects is `round(width_pt · dpi / 72)` by `round(height_pt · dpi / 72)` pixels, as [`Rasteriser::rasterise`]
/// describes.
#[derive(Clone, Debug, PartialEq)]
pub struct RasterRequest {
    /// The display list to render.
    pub list: DisplayList,
    /// The resolution to render it at, in dots per inch.
    pub dpi: f64,
}

/// The message with which a replay refuses a request the recording did not plan.
const NOT_PLANNED: &str = "the export asked for a render that was not planned by the recording; a replay must answer \
                           the requests raster_requests recorded for the same list and options";

/// Records the requests an export makes, answering each with one transparent pixel.
struct Recording {
    requests: Vec<RasterRequest>,
}

impl Rasteriser for Recording {
    fn rasterise(&mut self, list: &DisplayList, dpi: f64) -> Result<RasterImage, String> {
        self.requests.push(RasterRequest {
            list: list.clone(),
            dpi,
        });
        Ok(RasterImage {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        })
    }
}

/// Records the renders an export of `list` under `options` asks for, in the order it asks for them: phase 1 of the
/// two-phase export.
///
/// Nothing is recorded, and the export is not run, when [`needs_rasteriser`] reports that `list` needs no
/// rasteriser under `options`; a caller therefore learns from an empty plan that it need not open a graphics
/// adapter at all. Otherwise the export is run once with a rasteriser that records what it is asked for, and its
/// page is discarded.
///
/// # Errors
///
/// Returns the errors of [`render_display_list`] other than [`PdfError::Raster`], which the recording never raises.
pub fn raster_requests(
    list: &DisplayList,
    text: &TextEngine,
    options: &PdfOptions,
) -> Result<Vec<RasterRequest>, PdfError> {
    if needs_rasteriser(list, &options.raster) == Need::No {
        return Ok(Vec::new());
    }
    let mut recording = Recording {
        requests: Vec::new(),
    };
    render_display_list(list, text, options, Some(&mut recording))?;
    Ok(recording.requests)
}

/// Answers the requests of an export from the images recorded for them, matching each by content from a cursor that
/// only moves forwards.
struct Replay<'a> {
    requests: &'a [RasterRequest],
    /// The image answering the request at the same index, taken when it is handed over.
    rasters: Vec<Option<RasterImage>>,
    /// The index of the first recorded request that may still answer a request.
    cursor: usize,
}

impl Rasteriser for Replay<'_> {
    fn rasterise(&mut self, list: &DisplayList, dpi: f64) -> Result<RasterImage, String> {
        let offset = self.requests[self.cursor..]
            .iter()
            .position(|request| request.dpi == dpi && request.list == *list)
            .ok_or_else(|| NOT_PLANNED.to_owned())?;
        let index = self.cursor + offset;
        self.cursor = index + 1;
        Ok(self.rasters[index].take().expect(
            "each recorded request is answered at most once, because the cursor moves past it",
        ))
    }
}

/// Exports `list` with `rasters` answering `requests`, where `rasters[i]` is the render of `requests[i]`: phase 3
/// of the two-phase export.
///
/// `requests` must be what [`raster_requests`] recorded for the same `list` and `options`, and `rasters` must
/// answer them one for one, in order. The page and warnings are those [`render_display_list`] produces with a
/// rasteriser that renders each request as the caller did, including the skipping of every recorded request that
/// lies beneath a three-dimensional axes whose renders differed, which the export never makes.
///
/// # Errors
///
/// Returns [`PdfError::Raster`] when `rasters` and `requests` differ in number, and when the export asks for a
/// render that `requests` does not hold from the last answered request onwards, which means that the recording was
/// not made for this list and these options; and otherwise the errors of [`render_display_list`].
pub fn render_with_rasters(
    list: &DisplayList,
    text: &TextEngine,
    options: &PdfOptions,
    requests: &[RasterRequest],
    rasters: Vec<RasterImage>,
) -> Result<Rendered, PdfError> {
    if rasters.len() != requests.len() {
        return Err(PdfError::Raster(format!(
            "{} rasters were given for {} planned requests; a replay must answer every request that \
             raster_requests recorded, one raster each, in order",
            rasters.len(),
            requests.len()
        )));
    }
    let mut replay = Replay {
        requests,
        rasters: rasters.into_iter().map(Some).collect(),
        cursor: 0,
    };
    render_display_list(list, text, options, Some(&mut replay))
}
