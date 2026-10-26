//! Compilation of a figure IR into a display list and hit map.
//!
//! The work is split into private modules, run in this order by [`compile`]:
//!
//! 1. `style` resolves every colour scale of the figure once, with its automatic limits over every artist that names
//!    it, and `data` resolves every artist's arrays, checks their shapes and colour scales, and reports invalid
//!    artists.
//! 2. `panel` prepares and measures the content of every panel (with `legend` for the entries of a legend and
//!    `colorbar` for a colourbar), from which `layout` sizes the fitted rows and columns of the tile layout.
//! 3. `layout` divides the page into tile cells.
//! 4. `limits` computes axis limits (over link groups) and ticks.
//! 5. `decor` measures titles, labels and tick labels, from which `layout` derives each plot rectangle. When the
//!    layout has fitted rows or columns, steps 3 to 5 are repeated with their sizes recomputed from the margins found,
//!    until the sizes settle.
//! 6. `axes2d` and `axes3d` draw each axes, using `artists` for the data, `colorbar` for a colourbar inside the plot
//!    area and `legend` for a legend inside it.
//! 7. `layout` places the panels, and `panel` draws the content of each.
//!
//! `text`, `style`, `paths` and `image` hold shared helpers for text placement, colours, path geometry and the
//! pixels of images.

mod artists;
mod aspect;
mod axes2d;
mod axes3d;
mod bars;
mod cells;
mod colorbar;
mod data;
mod decor;
mod image;
mod layout;
mod legend;
mod limits;
mod panel;
mod paths;
mod rects;
mod style;
mod text;

use ironlab_ir::{AspectAdjust, Figure, NodeId, Projection};
use ironlab_text::TextEngine;

use crate::display::{DisplayList, Item, Rect};
use crate::hit::{HitMap, LegendOwner};

/// The largest number of passes that thin x ticks to fit their labels; each pass lowers a target of at most ten.
const MAX_TICK_FITTING_PASSES: usize = 10;

/// The largest number of passes that lay the figure out again with the sizes of its fitted rows and columns
/// recomputed from the margins of the previous pass. The margins of the axes depend on the sizes of their cells only
/// through their tick labels, so the sizes settle within two or three passes.
const MAX_FIT_PASSES: usize = 4;

/// A problem found while compiling a figure that did not prevent the figure from being drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneWarning {
    /// The IR node the warning concerns, when it concerns a specific node.
    pub node: Option<NodeId>,
    pub message: String,
    /// What the warning is about, for the warnings a front end or a test tells apart by more than their message.
    pub kind: SceneWarningKind,
}

/// What a [`SceneWarning`] is about.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SceneWarningKind {
    /// A problem described by the message alone.
    #[default]
    General,
    /// Two entries of one legend have the same label but different samples, so that the reader cannot tell
    /// which series each name stands for. Both entries are listed.
    LegendNameClash {
        /// The legend that lists both entries.
        legend: LegendOwner,
        /// The source text of the label the entries share.
        name: String,
    },
    /// The entries of a legend do not fit the area available to them, even in as many columns as fit its width, so
    /// that the entries beyond that area are not drawn.
    LegendOverflow {
        /// The legend whose entries do not fit.
        legend: LegendOwner,
    },
    /// A title, an axis label or a tick label extends beyond the edge of the page, so that part of it is not drawn.
    /// The warning names the figure, for its title, or the axes, for its title, axis labels and tick labels.
    TextOutsidePage {
        /// The source text of the text that extends beyond the page.
        text: String,
    },
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
    /// The page, in points, which text must not extend beyond.
    pub page: Rect,
    pub warnings: Vec<SceneWarning>,
}

impl<'a> Ctx<'a> {
    /// Returns a context for measuring text whose warnings are discarded, for a measurement that is repeated while
    /// the layout settles and whose warnings the drawing raises once.
    pub fn scratch(&self) -> Ctx<'a> {
        Ctx {
            figure: self.figure,
            text: self.text,
            font_size: self.font_size,
            page: self.page,
            warnings: Vec::new(),
        }
    }

    /// Records a warning about `node`.
    pub fn warn(&mut self, node: Option<NodeId>, message: impl Into<String>) {
        self.warnings.push(SceneWarning {
            node,
            message: message.into(),
            kind: SceneWarningKind::General,
        });
    }

    /// Records a [`SceneWarningKind::TextOutsidePage`] about `node` when `bounds`, the box of the text `source`,
    /// extends beyond the page, unless the same text of the same node has already been reported.
    pub fn check_on_page(&mut self, node: NodeId, source: &str, bounds: Rect) {
        let page = self.page;
        let finite = [bounds.x, bounds.y, bounds.width, bounds.height]
            .iter()
            .all(|v| v.is_finite());
        let outside = bounds.x < page.x - PAGE_TOLERANCE
            || bounds.y < page.y - PAGE_TOLERANCE
            || bounds.right() > page.right() + PAGE_TOLERANCE
            || bounds.bottom() > page.bottom() + PAGE_TOLERANCE;
        if !finite || !outside {
            return;
        }
        let kind = SceneWarningKind::TextOutsidePage {
            text: source.to_owned(),
        };
        if self
            .warnings
            .iter()
            .any(|w| w.node == Some(node) && w.kind == kind)
        {
            return;
        }
        self.warnings.push(SceneWarning {
            node: Some(node),
            message: format!("The text {source:?} extends beyond the edge of the page, so part of it is not drawn."),
            kind,
        });
    }
}

/// How far, in points, text may reach beyond the page before it is reported: the rounding of its measured box.
const PAGE_TOLERANCE: f64 = 0.01;

/// The largest number of times the layout is repeated for limits widened by a fixed aspect.
const MAX_ASPECT_PASSES: usize = 3;

/// Measures the decorations of every axes for the given limits.
fn measure(
    ctx: &mut Ctx,
    ranges: &[[limits::Range; 3]],
    targets: &[[usize; 3]],
) -> Vec<decor::Decor> {
    let figure = ctx.figure;
    figure
        .axes
        .iter()
        .enumerate()
        .map(|(i, axes)| decor::measure(ctx, axes, &ranges[i], &targets[i]))
        .collect()
}

/// Measures the decorations of every axes for the given limits and places the axes and panels in the grid
/// area with the margins the decorations need.
fn arrange(
    ctx: &mut Ctx,
    ranges: &[[limits::Range; 3]],
    targets: &[[usize; 3]],
    fits: &layout::Fits,
    grid_area: Rect,
    panel_tops: &[f64],
) -> (Vec<decor::Decor>, layout::Placement) {
    let decorations = measure(ctx, ranges, targets);
    let margins: Vec<layout::Margins> = ctx
        .figure
        .axes
        .iter()
        .zip(&decorations)
        .map(|(axes, d)| match axes.projection {
            Projection::TwoD => axes2d::margins(ctx, d),
            Projection::ThreeD { .. } => axes3d::margins(ctx, d),
        })
        .collect();
    let placement = layout::place(ctx, fits, grid_area, &margins, panel_tops);
    (decorations, placement)
}

/// Compiles `figure` into a scene.
///
/// This is the only place where layout, tick generation, text placement, contour extraction, projection and depth
/// sorting happen; every backend draws the resulting display list without further interpretation. Compilation never
/// fails: an artist whose data cannot be used, and an artist whose data gives it nothing to draw, is skipped and
/// reported as a warning, so a figure is always drawable and every artist left out is named.
///
/// # Output conventions
///
/// The integration tests in `tests/compile` rely on the following conventions.
///
/// - **Page.** The display list is `size.width_mm × 72 / 25.4` points wide and `size.height_mm × 72 / 25.4` points
///   high, and its background is the figure background.
/// - **Sources.** Geometry drawn for an artist names the artist as its source. Axes decorations (box, tick marks,
///   tick labels, common exponent labels, grid lines, axis labels, the axes title) and every part of the legend inside
///   its plot area (its box, entry samples and entry labels) name the axes. Every part of a legend panel (its box,
///   entry samples and entry labels) names the panel, and the title of a panel names the panel, whatever it holds. Every part of a colourbar (its bar, outline, tick marks, tick
///   labels, squares, key labels and title) names its holder: the panel of a panel colourbar, and the axes of an
///   inside colourbar. The figure title names the figure.
/// - **Paint order.** In 2D axes, artists are painted in the order in which the axes lists them, so a later artist
///   covers an earlier one. The legend inside a plot area is painted after every artist of its axes, and the legend
///   panels are painted after every axes, in the order the figure lists them. An inside colourbar is painted after
///   the artists, box, ticks and labels of its axes and before its inside legend.
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
/// - **Item granularity.** The line of a series of one colour in a 2D axes is stroked as one path, with one subpath
///   per run of consecutive placeable points, or, where the series is decimated, one per stretch of such a run that
///   reaches the axes. Every other line of a series (one colour-mapped by colour data in a 2D axes, and every line
///   in a 3D axes) is drawn segment by segment: each segment is one straight stroke, or, as a gradient, a run of
///   pieces (see **Series**), and each corner between two drawn segments where the dash pattern is on is filled
///   with a disc of the line's width in the colour at the corner, painted before the segments in a 2D axes and just
///   behind them in a 3D axes, so that the corner looks as a round join of one path would. The markers of an artist are one
///   [`crate::display::ItemKind::Markers`] item in a 2D axes, and in a 3D axes one item per run of its markers
///   that the depth sort leaves together; every instance carries its position, size, colours and data index, and
///   the item the outline of the marker shape in unit space and the edge width. Each quiver arrow is one path item. Each surface face
///   is one path item that carries its face fill and, when edges are drawn, its edge stroke, except in a 3D axes,
///   where a face with both is two path items, the fill and then the edge, the edge a ring filled with the edge
///   colour between the face's outline and that outline moved half the edge width inwards, so that it never spills
///   over the neighbouring faces (see **3D**); a face whose outline has no such inset is stroked instead. Each
///   filled-contour band is one path item filled with the nonzero rule. A contour isoline path never mixes levels. Each image
///   artist is one image item beneath one group that carries its placement (see **Images**).
/// - **Decimation.** A series with more points than its plot rectangle can resolve is thinned to about
///   [`crate::maths::decimate::SAMPLES_PER_POINT`] points per point of plot width: a line by the
///   largest-triangle-three-buckets rule, applied to each run of placeable points separately so that a break in the
///   line is never smoothed over, and a set of markers by keeping the one painted on top in each square of half a
///   marker width. The thinning depends on the axis limits and on the 3D camera, so it is redone whenever the view
///   changes, and the display list the PDF exporter draws is the one the screen shows. Every colour of a thinned
///   series is read at the source index of what it colours: a marker and an end of a gradient segment at the index
///   of its point, and a segment coloured by one value per segment at the index of the first source segment it
///   replaces, the one leaving its first point.
/// - **Series.** A series draws its line unless its dash or its line colour is `none`, and its markers unless their
///   shape is `none`. With one colour value per point, a colour-mapped line strokes each segment as a gradient: a
///   run of pieces of one colour, laid end to end, of equal length, as many as the segment needs for no piece to be
///   longer than 1 pt but at most 256, so that their number depends only on the segment's length on the page; each
///   piece takes the colour that the straight sRGB interpolation between the colours of the segment's two points
///   takes at its middle. With one value per segment, each segment is one stroke of the colour of its value. Every
///   stroke of a line drawn segment by segment keeps the dash array of the line, with a dash offset equal (modulo
///   the pattern) to its distance from the start of its run, so that the pattern runs on across segments and
///   pieces. The colour of a series is its line colour, which an automatic marker face or edge follows: with a
///   colour-mapped line and one value per point, the colour of each point. A colormapped marker face or edge takes
///   its point's colour, and without colour data the colour of the series. A point whose colour value its scale
///   draws nothing for draws no marker and breaks the line, so neither segment that meets it is drawn. Size data
///   gives each marker its own diameter in points. A series that draws a line strokes its marker edges at its line
///   width held to 0.5–1.5 pt, and one that draws no line at 0.5 pt.
/// - **Picking.** The hit map records the points every series drew, each naming the index it has in the
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
///   rasterising the rest. Each group holds one maximal run of consecutive items of one artist in paint order (in a
///   3D axes the fill and the edge of a face are two items of one run), so a surface whose faces the 3D depth sort
///   interleaves with the geometry of other artists — including a line thinned by the decimation above — yields
///   several groups, each recording the artist's total face count. A dense group has
///   no clip and no transform of its own, so a backend that ignores the marking draws exactly the same picture. An
///   image artist is already a raster and is never wrapped in a dense group, so that no backend resamples the pixels
///   the user supplied.
/// - **Colour order.** Automatic colours are taken from the Okabe–Ito palette without black, starting at orange:
///   `#E69F00`, `#56B4E9`, `#009E73`, `#F0E442`, `#0072B2`, `#D55E00`, `#CC79A7`, after which the order repeats.
///   Within each axes, every series and quiver whose primary colour, its `line.color`, is `ColorSpec::Auto` takes
///   the next entry, in the order in which the axes lists its artists and including hidden artists. Artists with an
///   explicit or colormapped primary colour, and contour and surface artists, take no entry, with one exception: a
///   series whose line is colour-mapped by one value per segment and that draws markers with an automatic face or
///   edge takes the next entry as its own colour, in which those markers are drawn. A marker face or edge set to
///   `ColorSpec::Auto` takes the resolved primary colour of its artist and takes no entry of its own.
/// - **Colour scales.** Every artist maps numbers to colours through the colour scale it names, by one rule
///   (`ResolvedScale::colour` in the `style` module): a continuous scale interpolates its colormap between its
///   limits in linear or logarithmic space; a discrete scale gives band `i` of `k` the colormap colour at `i / (k − 1)`, a value on an
///   inner edge belonging to the upper band; an indexed or categorical scale gives a whole number `i` entry `i`. A
///   named colormap of 256 entries gives position `t` entry `min(⌊256 t⌋, 255)`, counted from its end when it is
///   reversed, and a listed colormap is interpolated linearly in sRGB between its colours. A value the scale cannot
///   place (below or above its range, outside its entries, or not finite) follows the policy of that category of the
///   scale, for every artist alike. Automatic limits are the exact range of the finite numbers (the positive ones
///   under a logarithmic norm) of every artist that names the scale, in every axes, hidden artists included, and
///   automatic band edges are the two limits with nice values between them. A scale whose limits, band edges or
///   listed colormap cannot be used is drawn with automatic limits, ten automatic bands or viridis, and a warning
///   names it. An artist that maps numbers but names no colour scale, or names a node that is not one, and an
///   artist of which a number falls in a category whose policy is strict, is skipped with one warning naming it.
/// - **Colormapped colours.** `ColorSpec::Auto` on the isolines of a contour and on the faces or edges of a surface
///   means `ColorSpec::Colormapped`. A quiver whose colour is colormapped, and the colormapped line of a series
///   without colour data, has no number to map, so it takes the middle colour of viridis whatever scales the axes
///   uses; so does
///   a colormapped contour or surface colour of an artist that names no usable scale. The isolines of a filled
///   contour are drawn only when their colour is an explicit colour, because colormapped isolines would coincide
///   with the band colours.
/// - **Contour levels.** A contour without levels of its own takes them from its colour scale: the band edges of a
///   discrete scale, so that the contour agrees with the bands of the scale; the half-integers 0.5, 1.5, …, n − 1.5
///   between the n entries of an indexed or categorical scale, so that each band holds one whole number and takes
///   that entry, and each isoline takes the entry of the band below it; and ten automatic levels otherwise. A
///   filled band otherwise takes the colour of the middle of its range within the field, and an isoline the colour
///   of its level.
/// - **Limits.** Automatic axis limits round the data range outward to major ticks, are computed over the data of
///   every axes in the same link group, and are `[0, 1]` for an axes without data. Gridded data is the exception, as
///   in MATLAB's `contour`, `contourf`, `contour3`, `surf`, `mesh` and `image`: along x and y, an end of the range
///   that only the grids of contour and surface artists and the pixel edges of images reach is the exact end of
///   those grids and edges, while an end that other data reaches beyond them is rounded outward as usual. The z
///   limits of a 3D axes are always rounded outward, the edges of an image on a wall of the axes included. An image
///   whose plane has an explicit offset places that offset along the third axis of its plane, where it is rounded
///   like other data; a plane without an offset places nothing along that axis. Data that cannot be placed on a
///   log axis is dropped and does not contribute to the limits. Automatic colour limits are the exact (unrounded)
///   range of the numbers mapped through each colour scale (see **Colour scales**); the components of true-colour
///   images are not such numbers.
/// - **Surface colour.** A colormapped face takes the colour its scale gives the mean of its four corner colour
///   values (`c` when present, otherwise `z`), except through an indexed or categorical scale, where it takes the
///   value of its first corner (lower `i`, lower `j`), because the mean of indices is not an index. A face with a NaN
///   corner is not drawn.
/// - **Surfaces in 2D axes.** A surface is drawn in either projection. A 2D axes shows it from directly above, as a
///   pseudocolour plot: the grid alone places the faces through the x and y axis maps, so that the field `z` positions
///   nothing, reaches no axis limit and is not subject to the scale of the z axis, while it still colours the faces
///   unless `c` is given. The faces are painted where the surface stands in the artist order and are clipped to the
///   plot rectangle. A grid of `ny × nx` nodes draws `(ny − 1) × (nx − 1)` faces, so a surface or contour whose
///   field has no rows or no columns, or a single row or a single column of nodes, has no face and is skipped with
///   one warning naming it, in either projection; a series or quiver of no points is skipped the same way.
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
///   by rounding, copies 8-bit components, and draws a pixel with a non-finite component transparent. A scalar
///   image maps each number, an 8-bit value as the whole number it denotes, through its colour scale (see **Colour
///   scales**), and quantises the colour with its alpha; a pixel the scale draws nothing for is transparent, and a
///   strict policy skips the image with one warning naming it, the first offending pixel and the category. The
///   samples have three channels when every pixel is opaque and four otherwise. An image is skipped with one
///   warning naming it when its array is missing or has the wrong shape (`[ny, nx, 3]` or `[ny, nx, 4]` for pixels,
///   `[ny, nx]` for values), when it names no colour scale, when a pixel centre or
///   the plane offset is not finite or the centres of the first and last pixels coincide along an axis of more
///   than one pixel, when its plane is `xz` or `yz` in a 2D axes, when an axis of its plane is logarithmic (the
///   third axis may be, but then the offset must be positive), when it has no rows or no columns, or when a corner
///   cannot be placed. A skipped image contributes nothing to the axis limits, while its numbers still count
///   towards the automatic limits of the colour scale it names, as those of a hidden image do.
/// - **Tile sizes.** The aligned margins of every row and column of the tile layout are taken out of the area below
///   the figure title first. A fitted row then takes the share that the tallest panel lying in that row alone needs:
///   a legend panel for its block of entries and its title (with the border inset of a boxed panel), and a colourbar
///   panel for the extent of its bar, ticks, labels and titles across its length, with the page padding added on a side at which no
///   axes starts or ends; a fitted column takes the widest such share in the same way, and a fitted track in which no
///   panel lies alone takes none. The plot areas of the weighted rows share the rest of the height, and
///   those of the weighted columns the rest of the width, in proportion to their weights; a missing size, and a weight
///   that is not finite and positive, counts as a weight of one, so that equal weights draw exactly what no weights
///   draw. Plot areas of equal weight are therefore of equal size whatever their tick labels, axis labels, titles and
///   the fitted tracks beside them. Fitted tracks that need more than the space left share it in proportion to their
///   needs and leave the weighted tracks nothing. A cell block takes the shares of its rows and columns and the
///   margins between them. When the margins leave less than a fifth of the length, they are all scaled down alike.
///   The margins depend on the sizes through the tick labels, so a layout with fitted tracks is laid out again with
///   the shares recomputed until they settle, at most four times; only the warnings of the last pass are kept.
/// - **Axis direction.** A reversed axis is drawn from its upper limit to its lower limit: in a 2D axes from the left
///   or the bottom of the plot, so that its axis map has `start` at the upper limit, and in a 3D axes by mirroring
///   the normalised box along that axis, so that every artist, tick, grid line and label follows. The limits keep
///   `min < max`, automatic limits are computed as for the axis unreversed, and links share limits only, so linked
///   axes may differ in direction.
/// - **Aspect.** An axes with a fixed aspect draws one unit of y `y` times, and one unit of z `z` times, as long as one
///   unit of x, a unit of a log axis being one decade; a 2D axes uses `y` alone. A 3D axes kept by `AspectAdjust::Box`
///   keeps its limits and gives the sides of its box the lengths of their scale-space spans multiplied by the ratios;
///   one kept by `AspectAdjust::Limits` keeps the cube and widens the shorter scaled spans about their centres, a link
///   group widening as one. The box is projected after everything is placed in the normalised box, and is fitted by
///   its circumscribing sphere, so its size and proportions do not depend on the view. In a 2D axes: With `AspectAdjust::Box` its limits are kept and its plot rectangle is the largest one
///   of the right shape centred in the rectangle the layout gives it. With `AspectAdjust::Limits` the axis too short
///   for the plot rectangle widens symmetrically about its centre in scale space, a link group widening as one, and
///   the layout is repeated, at most three times, until the widened limits stop changing; when axes
///   that are linked need different shapes, no limits are widened and every such axes is fitted by its plot
///   rectangle or its box instead, with a warning naming it. Every axes with a fixed aspect is fitted last, so its ratio is
///   exact. A ratio of y or of z that is not finite and positive is drawn as an automatic aspect with a warning.
/// - **Titles.** An axes title is centred over the plot rectangle, or has its left or right edge on the left or right
///   edge of the plot rectangle, as `title_align` says.
/// - **Text beyond the page.** A title, axis label, tick label or common exponent label whose box extends beyond the
///   page raises one [`SceneWarningKind::TextOutsidePage`] for each node and text.
/// - **Ticks.** Automatic ticks are chosen by [`crate::maths::ticks`] from the limits and a target count of intervals
///   set by the length of the axis; on a linear axis whose limits are narrower than the round step of that target,
///   the target is raised until at least two ticks (on the x axis of a 2D axes) or three (on any other axis) lie
///   within the limits. A regular step places a tick at every `origin + k · step` within the limits, and explicit
///   values a tick at each of them within the limits; a step that would place more than 500 ticks, or cannot be
///   placed, falls back to automatic ticks with a warning naming the axes. Ticks never change the limits. Explicit
///   ticks carry no minor ticks.
/// - **Tick labels.** Automatic labels of automatic ticks are described under **Tick labels** above. Automatic labels
///   of a regular step or of explicit values show each value with the fewest decimals that show every one of them
///   (see [`crate::maths::ticks::decimals_for`]), with the common exponent of automatic labels, and on a log axis as
///   automatic log labels are written. Explicit labels are typeset like any other text of the figure and drawn at
///   the values they name; explicit labels that do not name explicit values one by one, which validation reports,
///   are replaced by automatic labels. Hidden labels draw the ticks without labels or a common exponent, and the
///   layout reserves no space for them.
/// - **Legend.** Only artists with usable data have legend entries, and only those with a label have
///   automatic ones. An axes whose legend is inside its plot area lists its entries there, in artist order, or the
///   items of its explicit list, in their order, an artist item naming an artist of another axes being left out. An
///   axes whose legend names a legend panel lists its entries in that panel and draws none of its own; a panel the
///   figure does not have lists them nowhere. A panel with an explicit list lists its items in their order: an
///   artist item names an artist of any axes, is left out when the figure has no such artist with usable data, and
///   stands for that artist and for every artist of the axes that name the panel with an equal label and an
///   equal sample, in reading order; an artist without a label is listed with an empty label. A custom item
///   draws its own sample (a line sample as a series', its automatic line colour being the ink of the labels and
///   its automatic marker colours the line colour; a swatch as a filled patch), is never greyed and is not recorded
///   in the hit map. An item on a row of its own spans every column at its position in the list, and the items
///   between such rows form groups that each fill the columns, with shared column widths; its label sets no column's
///   width. A panel in fitted rows alone chooses automatic columns as the fewest rows its width allows, in the fewest
///   columns that give that many rows. A legend
///   panel is placed in its cells as an axes is and inset by the aligned margins of the rows and columns it occupies
///   (by the page padding on a side whose row or column holds no axes), and its area is recorded in the hit map
///   whether or not it lists anything. A panel lists the entries of every axes that names it in reading order: the
///   axes are ordered by the top-left tile of their cells, clamped into the layout, by row and then by column, with
///   the order of the axes in the figure breaking ties, and the artists of each axes follow in their own order. It
///   merges entries whose labels are equal and whose samples draw the same picture into one entry that
///   stands for every one of their artists, at the position of the first of them.
///   Entries that share a label but not a sample are all listed, with one
///   [`SceneWarningKind::LegendNameClash`] for the name. A merged entry is greyed only when all of its artists are
///   hidden. Entries are arranged in rows and columns: a fixed count of columns is used as given, but never more
///   columns than entries, and automatic columns are the fewest for which the block of entries fits the height
///   available, which is the plot height less the legend inset for an inside legend and the panel height for a
///   panel. Rows are as tall as their tallest entry and columns as wide as their widest, and column-major order
///   fills each column from the top before the next, row-major order each row from the left. A block that does not
///   fit the available area, even in as many columns as fit its width, raises one
///   [`SceneWarningKind::LegendOverflow`], is drawn from the top-left corner of that area inside a group clipped to
///   it, and leaves out the entries that do not lie wholly inside it. A panel places its block by its alignment, the
///   `best` alignment meaning the centre, and an empty panel draws nothing. The hit rectangle of an entry encloses
///   both its sample and its label. The sample of a series draws what the series draws: a line across the sample
///   with its marker at the centre when it draws a line, and its marker alone otherwise, the marker always of the
///   marker size rather than of any size data. A colour that colour data varies (a colour-mapped line, and a marker
///   colour that takes each point's colour) takes the middle colour of the series' colour scale. The sample of an
///   image is a filled patch: the middle colour
///   of its colour scale for a scalar image, and for a true-colour image the mean of the
///   quantised red, green and blue components of its pixels whose components are all finite, or no patch when it
///   has no such pixel. The four corners of every drawn image count among the data points that the `best` location
///   keeps clear of.
/// - **Colourbars.** A colourbar shows a colour scale through the resolved scale its artists use, so every bar of a
///   scale agrees with every other and with the artists. A continuous bar is one image item of 256 samples along the
///   bar, the sample at `(k + 0.5) / 256` of the way from the low end taking the colour of that position of the scale;
///   an indexed bar is one image item of one sample per entry, entry 0 at the low end; a discrete bar is one filled
///   rectangle per band, as long as the range of values it holds in the space of the norm; a categorical scale is one
///   square per category, filled with its colour and outlined in black, from category 0 at the top of a standing key
///   and the left of a lying one. Image samples run from row 0 at the top, so the low end is always the bottom of a
///   standing bar and the left of a lying one, whether or not the colormap is reversed. A bar is outlined as an axes
///   box is and its ends are flat; no out-of-range colour is drawn at an end. Its ticks are those of an axis built
///   from the scale's limits (the first and last band edges of a discrete scale, and the entries −0.5 to n − 0.5 of an
///   indexed scale, so that entry `k` is ticked at the centre of its block), ticks and tick labels, measured as an
///   axis's are, with the density of a 2D x axis along a lying bar and of a y axis along a standing one; automatic
///   ticks of a discrete scale mark every band edge, and those of an indexed scale whole entries only. Tick marks
///   leave the bar on its label side. Each out-of-range category whose key is shown is a square as wide as the bar is
///   thick, filled with the colour painted for it (the fixed colour, or the end colour under a clamp) or left empty,
///   and outlined in black; one margin separates every square from the bar and from the next square. Along a standing
///   bar the pieces read, from the top, the above key, the bar, the below (or outside) key and the non-finite key;
///   along a lying bar, from the left, the non-finite key, the below (or outside) key, the bar and the above key; a
///   categorical scale's keys follow its categories. Along a lying colourbar the margin is wide enough for the labels
///   under neighbouring squares not to meet. A key's default label is `> upper`, `< lower`, `Out of range` or
///   `NaN / Inf`, with a limit written as the tick at that limit is. The thickness of a bar, and every distance of a
///   colourbar, is a multiple of the base font size; a bar is three quarters of it thick. A panel colourbar runs along
///   the span from the near edge of the first drawn plot box to the far edge of the last among the axes in the rows
///   or columns it spans, so that a box shrunk by a fixed aspect does not leave it longer than the plots, and along the
///   whole area when those tracks hold no axes; the bar, keys included, is its `length` of that span (the whole span
///   standing and half of it lying when the length is automatic), and is placed along the span and across the area by
///   the panel's alignment; a standing bar's labels lie to its right and a lying bar's below it. A title placed along
///   the bar lies one title gap beyond the band of its labels, centred on the bar, turned to read upwards beside a
///   standing bar. A title placed above heads a lying bar inside the panel's area, and lies in the margin above the
///   area of a standing bar, which widens the top margin of the first row the panel spans to the largest that any
///   cell of that row needs; the title of the panel lies above both. In a fitted track, a panel colourbar takes its
///   thickness, ticks, labels and titles across its length, whatever its length. An inside colourbar lies along its
///   edge of the plot area, inset by one margin on every side, its bar and keys together `length` of the edge and
///   centred along it (a categorical key its own length), its labels toward the interior and kept within the plot
///   area along the bar; a title along it lies beyond its labels, and a title above heads the top end of a standing
///   bar and lies on the side of a lying bar away from its labels. It takes no margin, so the plot area is the same
///   with or without it. A colourbar whose
///   scale reference names no colour scale is not drawn, with one warning naming its holder. The hit map records
///   every colourbar drawn as a [`crate::hit::ColorbarHit`].
/// - **Warnings.** A warning names the node that owns the offending data or text: the artist for invalid or dropped
///   data, for data that gives it nothing to draw, for a placement or plane that cannot be drawn, for a pixel a
///   strict policy refuses and for its label, the axes for its title and axis labels, and the figure for its
///   title.
/// - **3D.** The artists of a 3D axes are wrapped in one [`crate::display::ItemKind::Depth`] group, placed after
///   the grid and the back edges of the box and before its front edges, which a backend with a depth buffer clears
///   its buffer at and tests every item of; the group is absent when no artist draws anything. Inside it, faces,
///   segments, markers and images are painted back to front for the current view, each sorted at a key of its own,
///   and primitives at equal keys keep artist order. Every path and image in the group carries the depth at which a
///   depth buffer tests it, larger being nearer: a polyline (a segment or gradient piece of a series, a contour
///   isoline, a quiver arrow) one depth per vertex, the depth of a gradient piece's ends interpolated along its
///   segment, a corner disc of a series the constant depth of its corner less
///   [`crate::maths::camera::EDGE_DEPTH_LIFT`], a marker its own constant depth (carried by the instance), a filled contour band and a
///   surface face the plane fitted to
///   their projected vertices (see [`crate::maths::camera::depth_plane`]; exact for a planar face and the
///   least-squares plane for a twisted one), and an image the plane over its pixel space. A face, fill and edge
///   ring alike, and an image inside the box are pushed [`crate::maths::camera::FACE_DEPTH_BIAS`] behind their
///   geometry and sorted at their mean depth less the bias, so that the lines and markers lying on a surface are
///   painted, and depth-tested, in front of it; the painter's order and the depth test therefore agree wherever
///   the order is exact. An image whose plane lies on a face of the box — its coordinate along the third
///   axis of its plane, the offset or the lower limit of that axis when it has none, is the lower or the upper
///   limit of that axis, to within one part in 10⁹ of the extent of the axis — keeps its unpushed plane and is
///   painted before every other primitive of the axes when that face is a back plane of the view (see
///   [`crate::maths::camera::back_planes`]) and after every other primitive when it is a front face, because
///   everything inside the box is in front of a back face and behind a front face; several images on faces keep
///   artist order among themselves. Each segment of a series, and each piece of a gradient, is a primitive of its
///   own, sorted at the mean depth of its two ends, so that a line passing through a surface is painted behind it
///   and in front of it where it should be. A series or quiver without z data lies in the plane z = 0. When an axis has its grid enabled, each of its major ticks draws one
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
        page: Rect::new(0.0, 0.0, 0.0, 0.0),
        warnings: Vec::new(),
    };
    let (width, height) = layout::page_size(&mut ctx);
    ctx.page = Rect::new(0.0, 0.0, width, height);
    let mut items: Vec<Item> = Vec::new();
    let mut hit_map = HitMap::default();

    let page = Rect::new(0.0, 0.0, width, height);
    let grid_area = layout::figure_title(&mut ctx, page, &mut items);

    let scales = style::ColourScales::resolve(&mut ctx);
    let prepared: Vec<Vec<data::Prepared>> = figure
        .axes
        .iter()
        .map(|axes| data::prepare_axes(&mut ctx, axes, &scales))
        .collect();
    let primaries: Vec<Vec<style::Paint>> = figure
        .axes
        .iter()
        .map(|axes| style::primaries(figure, axes))
        .collect();

    // The content of the panels is prepared and measured before the layout, because a fitted row or column takes
    // its size from it.
    let candidates: Vec<Vec<legend::Candidate>> = figure
        .axes
        .iter()
        .enumerate()
        .map(|(i, axes)| legend::candidates(axes, &prepared[i], &scales, &primaries[i]))
        .collect();
    let contents = panel::prepare(&mut ctx, &candidates, &scales);
    let panel_tops: Vec<f64> = contents.iter().map(|c| c.top_margin(&ctx)).collect();
    let content = |ctx: &Ctx, p: usize, along, cross| contents[p].size(ctx, along, cross);
    let mut fits = layout::Fits::new(&figure.layout);
    if fits.any() {
        fits = layout::fit_tracks(&ctx, grid_area, None, &panel_tops, &fits, content);
    }

    // The warnings of a pass that is repeated would only repeat those of the next, so they are discarded.
    let pass_start = ctx.warnings.len();
    let mut pass = 0;
    let (_, ranges, targets, mut decorations, margins) = loop {
        ctx.warnings.truncate(pass_start);
        let laid_out = lay_out(&mut ctx, &fits, grid_area, &prepared);
        pass += 1;
        if !fits.any() || pass == MAX_FIT_PASSES {
            break laid_out;
        }
        let next = layout::fit_tracks(
            &ctx,
            grid_area,
            Some(&laid_out.4),
            &panel_tops,
            &fits,
            content,
        );
        if next.settled(&fits) {
            break laid_out;
        }
        fits = next;
    };
    let mut placement = layout::place(&ctx, &fits, grid_area, &margins, &panel_tops);

    // A fixed aspect is kept once the margins are known. Widening limits for it changes the tick labels and so the
    // margins, which change the plot areas the limits are widened for, so the layout is repeated until the widened
    // limits stop changing; the warnings of the repeated measurements are those of the last.
    let requests = aspect::requests(&mut ctx);
    let measured = ctx.warnings.len();
    let mut drawn = ranges.clone();
    let mut settled = true;
    if requests
        .iter()
        .flatten()
        .any(|r| r.adjust == AspectAdjust::Limits)
    {
        for pass in 0..=MAX_ASPECT_PASSES {
            let widened;
            (widened, settled) = aspect::widen_limits(&ctx, &ranges, &placement.plots, &requests);
            if widened == drawn {
                break;
            }
            drawn = widened;
            ctx.warnings.truncate(measured);
            if pass == MAX_ASPECT_PASSES {
                // The last widening is drawn as it is, with tick labels measured for it, in the places the previous
                // margins left; the aspect stays exact, because the limits were widened for these plot areas.
                decorations = measure(&mut ctx, &drawn, &targets);
            } else {
                (decorations, placement) =
                    arrange(&mut ctx, &drawn, &targets, &fits, grid_area, &panel_tops);
            }
        }
    }
    // A warning of the measurements repeated for the aspect that the first measurement already raised is dropped.
    let (first, repeated) = ctx.warnings.split_at(measured);
    let fresh: Vec<SceneWarning> = repeated
        .iter()
        .filter(|w| !first.contains(w))
        .cloned()
        .collect();
    ctx.warnings.truncate(measured);
    ctx.warnings.extend(fresh);
    if !settled {
        aspect::warn_unsettled(&mut ctx, &requests);
    }
    aspect::fit_plots(&mut placement.plots, &drawn, &requests);
    let ranges = drawn;

    for (i, axes) in figure.axes.iter().enumerate() {
        let input = artists::AxesInput {
            axes,
            prepared: &prepared[i],
            ranges: &ranges[i],
            scales: &scales,
            decor: &decorations[i],
            plot: placement.plots[i],
            outer: placement.outer[i],
            page,
        };
        match axes.projection {
            Projection::TwoD => axes2d::emit(&mut ctx, &input, &mut items, &mut hit_map),
            Projection::ThreeD { view3d } => {
                axes3d::emit(&mut ctx, &input, view3d, &mut items, &mut hit_map)
            }
        }
    }

    // Panels are drawn after every axes.
    panel::draw(
        &mut ctx,
        &fits,
        placement.panels,
        &placement.plots,
        &contents,
        &mut items,
        &mut hit_map,
    );

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

/// The outer rectangles, axis ranges, tick targets, decorations and margins of every axes, from one layout pass.
type LaidOut = (
    Vec<Rect>,
    Vec<[limits::Range; 3]>,
    Vec<[usize; 3]>,
    Vec<decor::Decor>,
    Vec<layout::Margins>,
);

/// Divides the page into cells with the given sizes of the fitted rows and columns, and computes the limits, ticks,
/// decorations and margins of every axes in its cell.
fn lay_out(
    ctx: &mut Ctx,
    fits: &layout::Fits,
    grid_area: Rect,
    prepared: &[Vec<data::Prepared>],
) -> LaidOut {
    let figure = ctx.figure;
    let outer: Vec<Rect> = figure
        .axes
        .iter()
        .map(|axes| layout::cell_rect(&figure.layout, fits, axes.cell, grid_area))
        .collect();
    let mut targets: Vec<[usize; 3]> = figure
        .axes
        .iter()
        .zip(&outer)
        .map(|(axes, rect)| limits::tick_targets(ctx, axes, *rect))
        .collect();
    let mut ranges = limits::axis_ranges(ctx, prepared, &targets);
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
                    decor::fit_x_target(ctx, axes, ranges[i][0], x, outer[i]),
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
        ranges = limits::axis_ranges(ctx, prepared, &targets);
        ctx.warnings.truncate(kept);
    }

    let decorations: Vec<decor::Decor> = figure
        .axes
        .iter()
        .enumerate()
        .map(|(i, axes)| decor::measure(ctx, axes, &ranges[i], &targets[i]))
        .collect();
    let margins: Vec<layout::Margins> = figure
        .axes
        .iter()
        .zip(&decorations)
        .map(|(axes, d)| match axes.projection {
            Projection::TwoD => axes2d::margins(ctx, d),
            Projection::ThreeD { .. } => axes3d::margins(ctx, d),
        })
        .collect();
    (outer, ranges, targets, decorations, margins)
}
