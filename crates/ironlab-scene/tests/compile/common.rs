//! Fixture builders that assemble figures directly from `ironlab_ir` types.
//!
//! Node identifiers, data arrays' included, are allocated explicitly by [`Fx`], so the fixtures do not depend on
//! the IR's own allocation helpers.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use ironlab_ir::{
    Artist, Axes, Cell, Color, ColorScale, ColorScaleKind, ColorScaleOutOfRange,
    ContinuousColorScale, Contour, DashStyle, DataArray, Figure, Grid, Image, ImagePlacement,
    ImagePlane, IndexedColorScale, Legend, LineStyle, MarkerShape, MarkerStyle, NdArray, NodeId,
    OutOfRange, Panel, PanelContent, PanelLegend, PixelRange, Projection, Quiver, ScalarImage,
    Series, Surface, Text, View3d,
};
use ironlab_scene::display::Rgba;
use ironlab_scene::maths::colormap::{Lut, VIRIDIS};
use ironlab_scene::{Scene, compile};
use ironlab_text::TextEngine;

/// Returns the text engine shared by every test, so fonts are parsed once per test binary.
pub fn engine() -> &'static TextEngine {
    static ENGINE: OnceLock<TextEngine> = OnceLock::new();
    ENGINE.get_or_init(TextEngine::new)
}

/// Compiles a figure with the shared text engine.
pub fn compile_figure(figure: &Figure) -> Scene {
    compile(figure, engine())
}

/// Returns `n` evenly spaced values from `a` to `b` inclusive.
pub fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    assert!(n >= 2, "linspace needs at least two values");
    (0..n)
        .map(|i| a + (b - a) * i as f64 / (n - 1) as f64)
        .collect()
}

/// Converts an IR colour into the display list colour it should compile to.
pub fn rgba(color: ironlab_ir::Color) -> Rgba {
    Rgba::new(color.r, color.g, color.b, color.a)
}

/// Rounds a display colour to 8-bit sRGB, ignoring alpha.
pub fn rgb8(color: Rgba) -> [u8; 3] {
    [color.r, color.g, color.b].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Returns whether two 8-bit colours differ by at most one level per channel.
pub fn rgb8_close(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1)
}

/// The automatic colour order: the Okabe–Ito palette without black, starting at orange.
pub const COLOUR_ORDER: [[u8; 3]; 7] = [
    [0xE6, 0x9F, 0x00],
    [0x56, 0xB4, 0xE9],
    [0x00, 0x9E, 0x73],
    [0xF0, 0xE4, 0x42],
    [0x00, 0x72, 0xB2],
    [0xD5, 0x5E, 0x00],
    [0xCC, 0x79, 0xA7],
];

/// Returns the index of the colormap entry nearest to `color`.
///
/// Comparing indices rather than exact colours lets tests check the direction of a colour mapping
/// without depending on how a value is rounded to an entry.
pub fn nearest_lut_index(lut: &Lut, color: Rgba) -> usize {
    let c = rgb8(color);
    (0..lut.len())
        .min_by_key(|&k| {
            lut[k]
                .iter()
                .zip(c)
                .map(|(a, b)| (i32::from(*a) - i32::from(b)).pow(2))
                .sum::<i32>()
        })
        .expect("colormaps are not empty")
}

/// The values of an image array, of either element type the IR stores.
pub enum Samples {
    F64(Vec<f64>),
    U8(Vec<u8>),
}

impl From<Vec<f64>> for Samples {
    fn from(values: Vec<f64>) -> Self {
        Samples::F64(values)
    }
}

impl From<Vec<u8>> for Samples {
    fn from(values: Vec<u8>) -> Self {
        Samples::U8(values)
    }
}

/// The centres of the first and last pixels of an image along one axis of its plane.
pub fn range(first: f64, last: f64) -> Option<PixelRange> {
    Some(PixelRange { first, last })
}

/// A placement of an image in `plane` with the given centre ranges, where `None` leaves the default
/// centres 0, 1, …, n − 1.
pub fn placement(
    plane: ImagePlane,
    columns: Option<PixelRange>,
    rows: Option<PixelRange>,
) -> ImagePlacement {
    ImagePlacement {
        plane,
        columns,
        rows,
    }
}

/// A placement of an image in the xy plane with the given centre ranges.
pub fn xy(columns: Option<PixelRange>, rows: Option<PixelRange>) -> ImagePlacement {
    placement(ImagePlane::Xy { z: None }, columns, rows)
}

/// A figure under construction with explicit identifier allocation.
///
/// Every colour-mapped artist names its colour scale. So that a fixture keeps the drawing of the figure model in which
/// each axes held one colormap and one pair of colour limits, a contour or surface added by [`Fx::contour`] or
/// [`Fx::surface`], and a scatter coloured by data, that names no scale after its edit names the scale of its axes
/// ([`Fx::axes_scale`]): a continuous scale of viridis between automatic limits, created for the axes on first use.
pub struct Fx {
    pub fig: Figure,
    next_node: u64,
    /// The continuous colour scale of each axes that has one, by axes.
    axes_scales: BTreeMap<NodeId, NodeId>,
    /// The 256-entry indexed colour scale of each axes that has one, by axes.
    palettes: BTreeMap<NodeId, NodeId>,
}

impl Fx {
    /// Starts a default figure (160 mm × 100 mm, one tile) whose own identifier is node 1.
    pub fn new() -> Self {
        let fig = Figure {
            id: NodeId(1),
            ..Figure::default()
        };
        Self {
            fig,
            next_node: 2,
            axes_scales: BTreeMap::new(),
            palettes: BTreeMap::new(),
        }
    }

    /// Allocates a fresh node identifier.
    pub fn node(&mut self) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        id
    }

    /// Stores a one-dimensional array and returns its identifier.
    pub fn vector(&mut self, values: &[f64]) -> NodeId {
        self.store(NdArray::vector(values.to_vec()))
    }

    /// Stores a two-dimensional array of shape `[ny, nx]` and returns its identifier.
    pub fn matrix(&mut self, ny: usize, nx: usize, values: Vec<f64>) -> NodeId {
        self.store(NdArray::from_shape(vec![ny, nx], values).expect("matrix shape"))
    }

    /// Stores an array of 8-bit values with the given shape and returns its identifier.
    pub fn bytes(&mut self, shape: Vec<usize>, values: Vec<u8>) -> NodeId {
        self.store(NdArray::from_shape_u8(shape, values).expect("byte array shape"))
    }

    fn store(&mut self, array: NdArray) -> NodeId {
        let id = self.node();
        self.fig.data.push(DataArray {
            id,
            name: None,
            array,
        });
        id
    }

    /// Adds a 2D axes in one tile cell and returns its identifier.
    pub fn axes2d(&mut self, row: u32, col: u32) -> NodeId {
        self.axes_in(
            Cell {
                row,
                col,
                row_span: 1,
                col_span: 1,
            },
            Projection::TwoD,
        )
    }

    /// Adds a 3D axes with the given view in one tile cell and returns its identifier.
    pub fn axes3d(&mut self, row: u32, col: u32, view3d: View3d) -> NodeId {
        self.axes_in(
            Cell {
                row,
                col,
                row_span: 1,
                col_span: 1,
            },
            Projection::ThreeD { view3d },
        )
    }

    /// Adds an axes in an arbitrary cell block and returns its identifier.
    pub fn axes_in(&mut self, cell: Cell, projection: Projection) -> NodeId {
        let id = self.node();
        self.fig.axes.push(Axes {
            id,
            cell,
            projection,
            ..Axes::default()
        });
        id
    }

    /// Returns the axes with the given identifier for modification.
    pub fn ax(&mut self, id: NodeId) -> &mut Axes {
        self.fig
            .axes
            .iter_mut()
            .find(|a| a.id == id)
            .expect("fixture axes exists")
    }

    fn push(&mut self, axes: NodeId, artist: Artist) {
        self.ax(axes).artists.push(artist);
    }

    /// Adds a series through `(x, y)` (and `z` when given) with the defaults of `plot` (a solid automatic line
    /// without markers), customised by `edit`.
    pub fn line(
        &mut self,
        axes: NodeId,
        x: &[f64],
        y: &[f64],
        z: Option<&[f64]>,
        edit: impl FnOnce(&mut Series),
    ) -> NodeId {
        self.series(axes, x, y, z, |s, _| edit(s))
    }

    /// Adds a series at `(x, y)` (and `z` when given) with the defaults of `scatter` (automatic circles without a
    /// line), customised by `edit`. A series given colour data without a scale names the scale of its axes.
    pub fn scatter(
        &mut self,
        axes: NodeId,
        x: &[f64],
        y: &[f64],
        z: Option<&[f64]>,
        edit: impl FnOnce(&mut Series, &mut Self),
    ) -> NodeId {
        let id = self.series(axes, x, y, z, |s, fx| {
            s.line = LineStyle {
                dash: DashStyle::None,
                ..LineStyle::default()
            };
            s.marker = MarkerStyle {
                shape: MarkerShape::Circle,
                ..MarkerStyle::default()
            };
            edit(s, fx);
        });
        let unscaled = matches!(
            self.ax(axes).artists.last(),
            Some(Artist::Series(s)) if s.color_data.is_some() && s.color_scale.is_none()
        );
        if unscaled {
            let scale = self.axes_scale(axes);
            if let Some(Artist::Series(s)) = self.ax(axes).artists.last_mut() {
                s.color_scale = Some(scale);
            }
        }
        id
    }

    /// Adds a series through `(x, y)` (and `z` when given) with the defaults of `Series` (a solid automatic line
    /// without markers), customised by `edit`. Unlike [`Fx::scatter`], it names no colour scale on its own: a series
    /// coloured by data names the scale its test gives it.
    pub fn series(
        &mut self,
        axes: NodeId,
        x: &[f64],
        y: &[f64],
        z: Option<&[f64]>,
        edit: impl FnOnce(&mut Series, &mut Self),
    ) -> NodeId {
        let id = self.node();
        let mut series = Series {
            id,
            x: self.vector(x),
            y: self.vector(y),
            z: z.map(|z| self.vector(z)),
            ..Series::default()
        };
        edit(&mut series, self);
        self.push(axes, Artist::Series(series));
        id
    }

    /// Samples `f(x, y)` on the rectilinear grid `x × y` and returns the grid and the field.
    pub fn field(&mut self, x: &[f64], y: &[f64], f: impl Fn(f64, f64) -> f64) -> (Grid, NodeId) {
        let values = y
            .iter()
            .flat_map(|&yv| x.iter().map(move |&xv| (xv, yv)))
            .map(|(xv, yv)| f(xv, yv))
            .collect();
        let z = self.matrix(y.len(), x.len(), values);
        let grid = Grid::Rectilinear {
            x: self.vector(x),
            y: self.vector(y),
        };
        (grid, z)
    }

    /// Adds a contour of `f` over the grid `x × y`, customised by `edit`.
    pub fn contour(
        &mut self,
        axes: NodeId,
        x: &[f64],
        y: &[f64],
        f: impl Fn(f64, f64) -> f64,
        edit: impl FnOnce(&mut Contour),
    ) -> NodeId {
        let id = self.node();
        let (grid, z) = self.field(x, y, f);
        let mut contour = Contour {
            id,
            grid,
            z,
            ..Contour::default()
        };
        edit(&mut contour);
        if contour.color_scale.is_none() {
            contour.color_scale = Some(self.axes_scale(axes));
        }
        self.push(axes, Artist::Contour(contour));
        id
    }

    /// Adds a surface of `f` over the grid `x × y`, customised by `edit`.
    pub fn surface(
        &mut self,
        axes: NodeId,
        x: &[f64],
        y: &[f64],
        f: impl Fn(f64, f64) -> f64,
        edit: impl FnOnce(&mut Surface),
    ) -> NodeId {
        let id = self.node();
        let (grid, z) = self.field(x, y, f);
        let mut surface = Surface {
            id,
            grid,
            z,
            ..Surface::default()
        };
        edit(&mut surface);
        if surface.color_scale.is_none() {
            surface.color_scale = Some(self.axes_scale(axes));
        }
        self.push(axes, Artist::Surface(surface));
        id
    }

    /// Adds a quiver with bases `(x, y[, z])` and components `(u, v[, w])`.
    #[allow(clippy::too_many_arguments)]
    pub fn quiver(
        &mut self,
        axes: NodeId,
        x: &[f64],
        y: &[f64],
        z: Option<&[f64]>,
        u: &[f64],
        v: &[f64],
        w: Option<&[f64]>,
    ) -> NodeId {
        let id = self.node();
        let quiver = Quiver {
            id,
            x: self.vector(x),
            y: self.vector(y),
            z: z.map(|z| self.vector(z)),
            u: self.vector(u),
            v: self.vector(v),
            w: w.map(|w| self.vector(w)),
            ..Quiver::default()
        };
        self.push(axes, Artist::Quiver(quiver));
        id
    }

    /// Stores an array of either element type with the given shape and returns its identifier.
    pub fn samples(&mut self, shape: Vec<usize>, values: impl Into<Samples>) -> NodeId {
        let array = match values.into() {
            Samples::F64(values) => NdArray::from_shape(shape, values),
            Samples::U8(values) => NdArray::from_shape_u8(shape, values),
        };
        self.store(array.expect("image array shape"))
    }

    /// Adds a true-colour image of `pixels` with the given shape (`[ny, nx, 3]` or `[ny, nx, 4]` when
    /// it is valid), placed by `placement` and customised by `edit`.
    pub fn image(
        &mut self,
        axes: NodeId,
        shape: Vec<usize>,
        pixels: impl Into<Samples>,
        placement: ImagePlacement,
        edit: impl FnOnce(&mut Image),
    ) -> NodeId {
        let id = self.node();
        let mut image = Image {
            id,
            pixels: self.samples(shape, pixels),
            placement,
            ..Image::default()
        };
        edit(&mut image);
        self.push(axes, Artist::Image(image));
        id
    }

    /// Returns the continuous colour scale of an axes, creating it on first use: viridis between automatic limits,
    /// clamping values beyond them, as the colormap and colour limits that an axes once held drew.
    pub fn axes_scale(&mut self, axes: NodeId) -> NodeId {
        if let Some(&scale) = self.axes_scales.get(&axes) {
            return scale;
        }
        let scale = self.color_scale(ColorScaleKind::Continuous(ContinuousColorScale::default()));
        self.axes_scales.insert(axes, scale);
        scale
    }

    /// Returns the continuous colour scale of an axes for modification, creating it on first use.
    pub fn axes_scale_mut(&mut self, axes: NodeId) -> &mut ContinuousColorScale {
        let id = self.axes_scale(axes);
        match &mut self.scale_mut(id).kind {
            ColorScaleKind::Continuous(scale) => scale,
            other => panic!("the axes scale is continuous, found {other:?}"),
        }
    }

    /// Returns the indexed colour scale of an axes, creating it on first use: the 256 entries of viridis, with an
    /// index outside them drawn as nothing, as an indexed image once looked its indices up in the colormap of its
    /// axes.
    pub fn palette(&mut self, axes: NodeId) -> NodeId {
        if let Some(&scale) = self.palettes.get(&axes) {
            return scale;
        }
        let scale = self.color_scale(ColorScaleKind::Indexed(IndexedColorScale {
            colors: palette_colors(&VIRIDIS),
            outside: out_of_range(OutOfRange::Transparent),
            ..IndexedColorScale::default()
        }));
        self.palettes.insert(axes, scale);
        scale
    }

    /// Returns the indexed colour scale of an axes for modification, creating it on first use.
    pub fn palette_mut(&mut self, axes: NodeId) -> &mut IndexedColorScale {
        let id = self.palette(axes);
        match &mut self.scale_mut(id).kind {
            ColorScaleKind::Indexed(scale) => scale,
            other => panic!("the palette is indexed, found {other:?}"),
        }
    }

    /// Adds a scalar image of indices with the given shape (`[ny, nx]` when it is valid), mapped through the indexed
    /// colour scale of its axes ([`Fx::palette`]), placed by `placement` and customised by `edit`.
    pub fn indexed_image(
        &mut self,
        axes: NodeId,
        shape: Vec<usize>,
        indices: impl Into<Samples>,
        placement: ImagePlacement,
        edit: impl FnOnce(&mut ScalarImage),
    ) -> NodeId {
        let scale = self.palette(axes);
        self.scalar_image(axes, shape, indices, Some(scale), placement, edit)
    }

    /// Adds a scalar image of values with the given shape (`[ny, nx]` when it is valid), mapped through the
    /// continuous colour scale of its axes ([`Fx::axes_scale`]) as `imagesc` maps them, placed by `placement` and
    /// customised by `edit`.
    pub fn imagesc(
        &mut self,
        axes: NodeId,
        shape: Vec<usize>,
        values: impl Into<Samples>,
        placement: ImagePlacement,
        edit: impl FnOnce(&mut ScalarImage),
    ) -> NodeId {
        let scale = self.axes_scale(axes);
        self.scalar_image(axes, shape, values, Some(scale), placement, edit)
    }

    /// Adds a colour scale of the given kind, with no title and with non-finite values drawn as nothing, and returns
    /// its identifier.
    pub fn color_scale(&mut self, kind: ColorScaleKind) -> NodeId {
        let id = self.node();
        self.fig.color_scales.push(ColorScale {
            id,
            title: None,
            kind,
            non_finite: out_of_range(OutOfRange::Transparent),
        });
        id
    }

    /// Returns the colour scale with the given identifier for modification.
    pub fn scale_mut(&mut self, id: NodeId) -> &mut ColorScale {
        self.fig
            .color_scales
            .iter_mut()
            .find(|s| s.id == id)
            .expect("fixture colour scale exists")
    }

    /// Adds a scalar image of `values` with the given shape (`[ny, nx]` when it is valid), mapped through
    /// `color_scale`, placed by `placement` and customised by `edit`.
    pub fn scalar_image(
        &mut self,
        axes: NodeId,
        shape: Vec<usize>,
        values: impl Into<Samples>,
        color_scale: Option<NodeId>,
        placement: ImagePlacement,
        edit: impl FnOnce(&mut ScalarImage),
    ) -> NodeId {
        let id = self.node();
        let mut image = ScalarImage {
            id,
            values: self.samples(shape, values),
            color_scale,
            placement,
            ..ScalarImage::default()
        };
        edit(&mut image);
        self.push(axes, Artist::ScalarImage(image));
        id
    }

    /// Adds a legend panel with default properties in the given cell block and returns its identifier.
    pub fn panel(&mut self, cell: Cell) -> NodeId {
        let id = self.node();
        self.fig.panels.push(Panel {
            id,
            cell,
            align: Default::default(),
            content: PanelContent::Legend(PanelLegend::default()),
        });
        id
    }

    /// Returns the panel with the given identifier for modification.
    pub fn panel_mut(&mut self, id: NodeId) -> &mut Panel {
        self.fig.panel_mut(id).expect("fixture panel exists")
    }

    /// Returns the legend of the legend panel with the given identifier for modification.
    pub fn legend_mut(&mut self, id: NodeId) -> &mut PanelLegend {
        let PanelContent::Legend(legend) = &mut self.panel_mut(id).content;
        legend
    }

    /// Lists the entries of an axes in the legend panel `panel`.
    pub fn legend_in(&mut self, axes: NodeId, panel: NodeId) {
        self.ax(axes).legend = Some(Legend::Panel(panel));
    }

    /// Adds a line from (0, 0) to (1, 1) with the given label, customised by `edit`.
    pub fn named_line(
        &mut self,
        axes: NodeId,
        name: &str,
        edit: impl FnOnce(&mut Series),
    ) -> NodeId {
        self.line(axes, &[0.0, 1.0], &[0.0, 1.0], None, |l| {
            l.label = text(name);
            edit(l);
        })
    }

    /// Finishes the figure.
    pub fn build(self) -> Figure {
        self.fig
    }
}

/// Shorthand for LaTeX-interpreted text.
pub fn text(content: &str) -> Option<Text> {
    Some(Text::new(content))
}

/// A cell block of the tile layout.
pub fn cell(row: u32, col: u32, row_span: u32, col_span: u32) -> Cell {
    Cell {
        row,
        col,
        row_span,
        col_span,
    }
}

/// An out-of-range category of a colour scale with the given policy, no key and the default label.
pub fn out_of_range(policy: OutOfRange) -> ColorScaleOutOfRange {
    ColorScaleOutOfRange {
        policy,
        show_key: false,
        label: None,
    }
}

/// The entries of an indexed colour scale holding the 256 colours of a colormap table.
pub fn palette_colors(lut: &Lut) -> Vec<Color> {
    lut.iter()
        .map(|rgb| {
            Color::rgb(
                f32::from(rgb[0]) / 255.0,
                f32::from(rgb[1]) / 255.0,
                f32::from(rgb[2]) / 255.0,
            )
        })
        .collect()
}
