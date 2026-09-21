//! Compilation of a figure IR into a display list and hit map.
//!
//! The work is split into private modules, run in this order by [`compile`]:
//!
//! 1. `data` resolves every artist's arrays, checks their shapes and reports invalid artists.
//! 2. `layout` divides the page into tile cells.
//! 3. `limits` computes axis limits (over link groups), colour limits and ticks.
//! 4. `decor` measures titles, labels and tick labels, from which `layout` derives each plot rectangle.
//! 5. `axes2d` and `axes3d` draw each axes, using `artists` for the data and `legend` for the legend.
//!
//! `text`, `style`, `paths` and `image` hold shared helpers for text placement, colours, path geometry and the
//! pixels of images.

mod artists;
mod axes2d;
mod axes3d;
mod data;
mod decor;
mod image;
mod layout;
mod legend;
mod limits;
mod paths;
mod style;
mod text;

use ironlab_ir::{Figure, NodeId, Projection};
use ironlab_text::TextEngine;

use crate::display::{DisplayList, Item, Rect};
use crate::hit::HitMap;

/// The largest number of passes that thin x ticks to fit their labels; each pass lowers a target of at most ten.
const MAX_TICK_FITTING_PASSES: usize = 10;

/// A problem found while compiling a figure that did not prevent the figure from being drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneWarning {
    /// The IR node the warning concerns, when it concerns a specific node.
    pub node: Option<NodeId>,
    pub message: String,
}

/// The compiled form of a figure: everything a backend draws, and everything a front end needs to interact with it.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub display_list: DisplayList,
    pub hit_map: HitMap,
    pub warnings: Vec<SceneWarning>,
}

/// State shared by every stage of one compilation.
pub(crate) struct Ctx<'a> {
    pub figure: &'a Figure,
    pub text: &'a TextEngine,
    /// The base font size in points, after replacing an invalid size with the default.
    pub font_size: f64,
    pub warnings: Vec<SceneWarning>,
}

impl Ctx<'_> {
    /// Records a warning about `node`.
    pub fn warn(&mut self, node: Option<NodeId>, message: impl Into<String>) {
        self.warnings.push(SceneWarning {
            node,
            message: message.into(),
        });
    }
}

/// Compiles `figure` into a scene.
///
/// This is the only place where layout, tick generation, text placement, contour extraction, projection and depth
/// sorting happen; every backend draws the resulting display list without further interpretation. Compilation never
/// fails: invalid artists are skipped and reported as warnings, so a figure is always drawable.
///
/// # Output conventions
///
/// The integration tests in `tests/compile` rely on the following conventions.
///
/// - **Page.** The display list is `size.width_mm × 72 / 25.4` points wide and `size.height_mm × 72 / 25.4` points
///   high, and its background is the figure background.
/// - **Sources.** Geometry drawn for an artist names the artist as its source. Axes decorations (box, tick marks,
///   tick labels, common exponent labels, grid lines, axis labels, the axes title) and every part of the legend (its
///   box, entry samples and entry labels) name the axes. The figure title names the figure.
/// - **Paint order.** In 2D axes, artists are painted in the order in which the axes lists them, so a later artist
///   covers an earlier one. The legend is painted after every artist of its axes.
/// - **Visibility.** An artist whose `visible` flag is false emits no items, but otherwise takes part in compilation
///   as if it were visible: it keeps its colour-order entry, and its data still contributes to automatic axis limits
///   and automatic colour limits. Its legend entry is still drawn, and every colour in the entry has an alpha below
///   1. Hiding or showing an artist therefore never moves, rescales or recolours anything else.
/// - **Clipping.** In 2D axes, artist geometry is drawn inside a group clipped to the axes' plot rectangle.
/// - **Rotated text.** A y label, and the z label of a 3D axes, reads from bottom to top: it is placed inside a group
///   whose transform turns the text a quarter turn. The z label of a 3D axes lies to the left of the z axis and of
///   the z tick labels.
/// - **Tick labels.** Each tick label of a linear axis is one glyph run of plain text; each decade label of a log
///   axis is the math text `$10^{n}$` (see [`crate::maths::ticks::format_log`]). When
///   [`crate::maths::ticks::common_exponent`] of an axis's major ticks is `k ≠ 0`, the labels show the ticks divided
///   by `10^k`, and a single math label `$\times 10^{k}$` is drawn at the far end of the axis: for an x axis beyond
///   the right end of the axis, below the tick labels, and for a y axis above the top end of the axis. In a 3D axes
///   the label is drawn at the end of the axis's row of tick labels, beyond the label of the largest tick.
/// - **Item granularity.** A line is stroked with one subpath per run of consecutive finite points, or, where the
///   series is decimated, one per stretch of such a run that reaches the axes. Each marker is
///   one path item that carries both its fill and its stroke. Each quiver arrow is one path item. Each surface face
///   is one path item that carries its face fill and, when edges are drawn, its edge stroke. Each filled-contour
///   band is one path item filled with the nonzero rule. A contour isoline path never mixes levels. Each image
///   artist is one image item beneath one group that carries its placement (see **Images**).
/// - **Decimation.** A line or scatter with more points than its plot rectangle can resolve is thinned to about
///   [`crate::maths::decimate::SAMPLES_PER_POINT`] points per point of plot width: a line by the
///   largest-triangle-three-buckets rule, applied to each run of placeable points separately so that a break in the
///   line is never smoothed over, and a set of markers by keeping the one painted on top in each square of half a
///   marker width. The thinning depends on the axis limits and on the 3D camera, so it is redone whenever the view
///   changes, and the display list the PDF exporter draws is the one the screen shows.
/// - **Picking.** The hit map records the points every line and scatter drew, each naming the index it has in the
///   artist's own data arrays, so a front end reports the index and the values of the user's data whether or not the
///   series was decimated. For every image drawn in a 2D axes it records an [`crate::hit::ImageHit`]: the inverse
///   of the image's placement with its pixel counts, through which [`crate::hit::HitMap::pixel_at`] finds the row
///   and column of the artist's array under a pointer, the pixel coordinates being floored so that the image is the
///   half-open extent `[0, nx) × [0, ny)` and a boundary shared by two pixels belongs to the higher index. A hidden
///   image, a skipped image and an image drawn in a 3D axes record nothing.
/// - **Dense content.** The faces of a surface are wrapped in [`crate::display::ItemKind::Dense`] groups that record
///   how many faces the artist drew, so that a backend can replace them with a raster image instead of drawing one
///   vector path per face. The count is of the faces actually drawn, not of the cells the grid holds: a face dropped
///   for a NaN corner, and geometry a future decimation removes, cost a backend nothing and do not count towards
///   rasterising the rest. Each group holds one maximal run of consecutive faces of one artist in paint order, so a
///   surface whose faces the 3D depth sort interleaves with the geometry of other artists — including a line thinned
///   by the decimation above — yields several groups, each recording the artist's total face count. A dense group has
///   no clip and no transform of its own, so a backend that ignores the marking draws exactly the same picture. An
///   image artist is already a raster and is never wrapped in a dense group, so that no backend resamples the pixels
///   the user supplied.
/// - **Colour order.** Automatic colours are taken from the Okabe–Ito palette without black, starting at orange:
///   `#E69F00`, `#56B4E9`, `#009E73`, `#F0E442`, `#0072B2`, `#D55E00`, `#CC79A7`, after which the order repeats.
///   Within each axes, every line, scatter and quiver artist whose primary colour is `ColorSpec::Auto` (the
///   `line.color` of a line or quiver, the `color` of a scatter) takes the next entry, in the order in which the axes
///   lists its artists and including hidden artists. Artists with an explicit or colormapped primary colour, and
///   contour and surface artists, take no entry. A marker face or edge set to `ColorSpec::Auto` takes the resolved
///   primary colour of its artist and takes no entry of its own.
/// - **Colormapped colours.** `ColorSpec::Auto` on the isolines of a contour and on the faces or edges of a surface
///   means `ColorSpec::Colormapped`. A line or quiver whose colour is colormapped has no colour data, so it takes
///   the middle colour of the colormap. The isolines of a filled contour are drawn only when their colour is an
///   explicit colour, because colormapped isolines would coincide with the band colours.
/// - **Limits.** Automatic axis limits round the data range outward to major ticks, are computed over the data of
///   every axes in the same link group, and are `[0, 1]` for an axes without data. Gridded data is the exception, as
///   in MATLAB's `contour`, `contourf`, `contour3`, `surf`, `mesh` and `image`: along x and y, an end of the range
///   that only the grids of contour and surface artists and the pixel edges of images reach is the exact end of
///   those grids and edges, while an end that other data reaches beyond them is rounded outward as usual. The z
///   limits of a 3D axes are always rounded outward, the edges of an image on a wall of the axes included. An image
///   whose plane has an explicit offset places that offset along the third axis of its plane, where it is rounded
///   like other data; a plane without an offset places nothing along that axis. Data that cannot be placed on a
///   log axis is dropped and does not contribute to the limits. Automatic colour limits are the exact (unrounded)
///   range of the finite colour values of the axes' colormapped artists, the values of colour-mapped images among
///   them; the indices of colour-indexed images and the components of true-colour images are not colour values.
/// - **Surface colour.** A colormapped face takes the colormap sample of the mean of its four corner colour values
///   (`c` when present, otherwise `z`), normalised by the colour limits. A face with a NaN corner is not drawn.
/// - **Images.** An image artist of any kind is drawn as one image item in pixel space, `[0, nx] × [0, ny]` for
///   `nx` columns and `ny` rows with the pixel in row `j` and column `i` covering `[i, i + 1] × [j, j + 1]`, beneath
///   one group whose transform maps pixel space into figure space; the group has no clip of its own and lies inside
///   the clipped group of the axes like every other artist geometry. The samples run in row order from row 0 and are
///   never reordered: a range whose `last` centre lies before its `first` mirrors the image through the transform
///   alone. Along each axis of its plane the pixels are placed by the centres of the first and last pixels, with a
///   pitch of `(last − first) / (n − 1)` between centres and half a pitch of raster beyond each of the two centres;
///   one pixel has a pitch of 1 whatever its range, and an absent range centres the pixels on 0 to n − 1. The
///   columns run along the first axis of the plane and the rows along the second; the coordinate along the third
///   axis is the offset of the plane, or the low end of that axis when it has none, and a 2D axes ignores it. In a
///   3D axes the image is one primitive sorted at the mean depth of its four corners, unless its plane lies on a
///   face of the box, when it is painted before or after every other primitive (see **3D**). The pixels are resolved to
///   eight-bit sRGB by kind. A true-colour image clamps floating-point components into `[0, 1]` and quantises them
///   by rounding, copies 8-bit components, and draws a pixel with a non-finite component transparent. A
///   colour-mapped image normalises each value by the colour limits and takes the colormap sample of the result, as
///   a surface does; a value that normalises below 0, one that normalises above 1 and a non-finite value fall in
///   the `below`, `above` and `non_finite` categories. A colour-indexed image takes entry `i` of the colormap for an
///   8-bit index `i`, and for a floating-point index truncated toward zero into 0 to 255; a truncated index below 0,
///   one above 255 and a non-finite index fall in the same three categories, and the colour limits play no part. A
///   pixel in a category takes what the artist's policy for that category says: `transparent` draws nothing,
///   `clamp` takes the first colormap entry below and the last above and draws nothing for a non-finite value,
///   `rgba` takes the colour quantised with its alpha, and `strict` skips the artist with one warning naming it,
///   the first offending pixel and the category. The samples have three channels when every pixel is opaque and
///   four otherwise. An image is skipped with one warning naming it when its array is missing or has the wrong
///   shape (`[ny, nx, 3]` or `[ny, nx, 4]` for pixels, `[ny, nx]` for indices and values), when a pixel centre or
///   the plane offset is not finite or the centres of the first and last pixels coincide along an axis of more
///   than one pixel, when its plane is `xz` or `yz` in a 2D axes, when an axis of its plane is logarithmic (the
///   third axis may be, but then the offset must be positive), or when a corner cannot be placed; an image with no
///   rows or no columns is skipped silently. A skipped image contributes nothing to the axis or colour limits,
///   with one exception: an image skipped by a strict policy is skipped when its colours are resolved, after the
///   limits are computed, so its pixel edges and values still count as those of a hidden image do.
/// - **Legend.** Only artists with a display name have legend entries, listed in artist order. The hit rectangle of
///   an entry encloses both its sample and its label. The sample of an image is a filled patch: the middle colour
///   of the colormap for a colour-indexed or colour-mapped image, and for a true-colour image the mean of the
///   quantised red, green and blue components of its pixels whose components are all finite, or no patch when it
///   has no such pixel. The four corners of every drawn image count among the data points that the `best` location
///   keeps clear of.
/// - **Warnings.** A warning names the node that owns the offending data or text: the artist for invalid or dropped
///   data, for a placement or plane that cannot be drawn, for a pixel a strict policy refuses and for its display
///   name, the axes for its title and axis labels, and the figure for its title.
/// - **3D.** Faces, segments, markers and images are painted back to front for the current view, each sorted at a
///   depth of its own, and primitives at equal depth keep artist order. An image whose plane lies on a face of the
///   box — its coordinate along the third axis of its plane, the offset or the lower limit of that axis when it has
///   none, is the lower or the upper limit of that axis, to within one part in 10⁹ of the extent of the axis — is
///   painted before every other primitive of the axes when that face is a back plane of the view (see
///   [`crate::maths::camera::back_planes`]) and after every other primitive when it is a front face, because
///   everything inside the box is in front of a back face and behind a front face; several images on faces keep
///   artist order among themselves. A line, scatter
///   or quiver without z data lies in the plane z = 0. When an axis has its grid enabled, each of its major ticks draws one
///   grid line on each of the two back planes (see [`crate::maths::camera::back_planes`]) that contain that axis's
///   direction, except where the grid line would coincide with an edge of the box. The edge that carries an axis's
///   tick labels has a short tick mark at each major tick, pointing away from the box towards the label. Any two tick
///   labels of a 3D axes keep a clear gap of at least 0.3 font sizes: labels are placed for x, then y, then z, and a
///   label that would come closer to one already placed is left out, while its tick mark is still drawn. The hit map describes a 3D axes
///   with [`crate::hit::AxesHitKind::ThreeD`], which carries no projection data.
/// - **Determinism.** Compiling the same figure twice gives equal scenes.
pub fn compile(figure: &Figure, text: &TextEngine) -> Scene {
    let mut ctx = Ctx {
        figure,
        text,
        font_size: layout::font_size(figure),
        warnings: Vec::new(),
    };
    let (width, height) = layout::page_size(&mut ctx);
    let mut items: Vec<Item> = Vec::new();
    let mut hit_map = HitMap::default();

    let page = Rect::new(0.0, 0.0, width, height);
    let grid_area = layout::figure_title(&mut ctx, page, &mut items);
    let outer: Vec<Rect> = figure
        .axes
        .iter()
        .map(|axes| layout::cell_rect(figure.layout, axes.cell, grid_area))
        .collect();

    let prepared: Vec<Vec<data::Prepared>> = figure
        .axes
        .iter()
        .map(|axes| data::prepare_axes(&mut ctx, axes))
        .collect();
    let mut targets: Vec<[usize; 3]> = figure
        .axes
        .iter()
        .zip(&outer)
        .map(|(axes, rect)| limits::tick_targets(&ctx, axes, *rect))
        .collect();
    let mut ranges = limits::axis_ranges(&mut ctx, &prepared, &targets);
    // Thin the x ticks until their labels fit. Every pass lowers at least one target or stops, and repeated passes
    // would only repeat the warnings of the first, so those are discarded.
    let kept = ctx.warnings.len();
    for _ in 0..MAX_TICK_FITTING_PASSES {
        let fitted: Vec<[usize; 3]> = figure
            .axes
            .iter()
            .enumerate()
            .map(|(i, axes)| {
                let [x, y, z] = targets[i];
                [
                    decor::fit_x_target(&mut ctx, axes, ranges[i][0], x, outer[i]),
                    y,
                    z,
                ]
            })
            .collect();
        ctx.warnings.truncate(kept);
        if fitted == targets {
            break;
        }
        targets = fitted;
        ranges = limits::axis_ranges(&mut ctx, &prepared, &targets);
        ctx.warnings.truncate(kept);
    }

    let decorations: Vec<decor::Decor> = figure
        .axes
        .iter()
        .enumerate()
        .map(|(i, axes)| decor::measure(&mut ctx, axes, &ranges[i], &targets[i]))
        .collect();
    let margins: Vec<layout::Margins> = figure
        .axes
        .iter()
        .zip(&decorations)
        .map(|(axes, d)| match axes.projection {
            Projection::TwoD => axes2d::margins(&ctx, d),
            Projection::ThreeD { .. } => axes3d::margins(&ctx, d),
        })
        .collect();
    let plots = layout::plot_rects(figure, &outer, &margins);

    for (i, axes) in figure.axes.iter().enumerate() {
        let colour_scale = limits::colour_scale(&mut ctx, axes, &prepared[i]);
        let input = artists::AxesInput {
            axes,
            prepared: &prepared[i],
            ranges: &ranges[i],
            colours: colour_scale,
            decor: &decorations[i],
            plot: plots[i],
            outer: outer[i],
        };
        match axes.projection {
            Projection::TwoD => axes2d::emit(&mut ctx, &input, &mut items, &mut hit_map),
            Projection::ThreeD { view3d } => {
                axes3d::emit(&mut ctx, &input, view3d, &mut items, &mut hit_map)
            }
        }
    }

    Scene {
        display_list: DisplayList {
            width_pt: width,
            height_pt: height,
            background: style::ir_colour(figure.background),
            items,
        },
        hit_map,
        warnings: ctx.warnings,
    }
}
