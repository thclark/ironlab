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
//! `text`, `style` and `paths` hold shared helpers for text placement, colours and path geometry.

mod artists;
mod axes2d;
mod axes3d;
mod data;
mod decor;
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
/// - **Item granularity.** A line is stroked with one subpath per run of consecutive finite points. Each marker is
///   one path item that carries both its fill and its stroke. Each quiver arrow is one path item. Each surface face
///   is one path item that carries its face fill and, when edges are drawn, its edge stroke. Each filled-contour
///   band is one path item filled with the nonzero rule. A contour isoline path never mixes levels.
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
///   in MATLAB's `contour`, `contourf`, `contour3`, `surf` and `mesh`: along x and y, an end of the range that only
///   the grids of contour and surface artists reach is the exact end of those grids, while an end that other data
///   reaches beyond the grids is rounded outward as usual. The z limits of a 3D axes are always rounded outward. Data that cannot be placed on a
///   log axis is dropped and does not contribute to the limits. Automatic colour limits are the exact (unrounded)
///   range of the finite colour values of the axes' colormapped artists.
/// - **Surface colour.** A colormapped face takes the colormap sample of the mean of its four corner colour values
///   (`c` when present, otherwise `z`), normalised by the colour limits. A face with a NaN corner is not drawn.
/// - **Legend.** Only artists with a display name have legend entries, listed in artist order. The hit rectangle of
///   an entry encloses both its sample and its label.
/// - **Warnings.** A warning names the node that owns the offending data or text: the artist for invalid or dropped
///   data and for its display name, the axes for its title and axis labels, and the figure for its title.
/// - **3D.** Faces, segments and markers are painted back to front for the current view. A line, scatter or quiver
///   without z data lies in the plane z = 0. When an axis has its grid enabled, each of its major ticks draws one
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
