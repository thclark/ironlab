//! Fixture builders that assemble figures directly from `ironlab_ir` types.
//!
//! Node and data identifiers are allocated explicitly by [`Fx`], so the fixtures do not depend on
//! the IR's own allocation helpers.

use std::sync::OnceLock;

use ironlab_ir::{
    Artist, Axes, Cell, Contour, DataId, Figure, Grid, Line, NdArray, NodeId, Projection, Quiver,
    Scatter, Surface, Text, View3d,
};
use ironlab_scene::display::Rgba;
use ironlab_scene::maths::colormap::Lut;
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

/// A figure under construction with explicit identifier allocation.
pub struct Fx {
    pub fig: Figure,
    next_node: u64,
    next_data: u64,
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
            next_data: 1,
        }
    }

    /// Allocates a fresh node identifier.
    pub fn node(&mut self) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        id
    }

    /// Stores a one-dimensional array and returns its identifier.
    pub fn vector(&mut self, values: &[f64]) -> DataId {
        self.store(NdArray::vector(values.to_vec()))
    }

    /// Stores a two-dimensional array of shape `[ny, nx]` and returns its identifier.
    pub fn matrix(&mut self, ny: usize, nx: usize, values: Vec<f64>) -> DataId {
        self.store(NdArray::from_shape(vec![ny, nx], values).expect("matrix shape"))
    }

    fn store(&mut self, array: NdArray) -> DataId {
        let id = DataId(self.next_data);
        self.next_data += 1;
        self.fig.data.insert(id, array);
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

    /// Adds a line through `(x, y)` (and `z` when given), customised by `edit`.
    pub fn line(
        &mut self,
        axes: NodeId,
        x: &[f64],
        y: &[f64],
        z: Option<&[f64]>,
        edit: impl FnOnce(&mut Line),
    ) -> NodeId {
        let id = self.node();
        let mut line = Line {
            id,
            x: self.vector(x),
            y: self.vector(y),
            z: z.map(|z| self.vector(z)),
            ..Line::default()
        };
        edit(&mut line);
        self.push(axes, Artist::Line(line));
        id
    }

    /// Adds a scatter at `(x, y)` (and `z` when given), customised by `edit`.
    pub fn scatter(
        &mut self,
        axes: NodeId,
        x: &[f64],
        y: &[f64],
        z: Option<&[f64]>,
        edit: impl FnOnce(&mut Scatter, &mut Self),
    ) -> NodeId {
        let id = self.node();
        let mut scatter = Scatter {
            id,
            x: self.vector(x),
            y: self.vector(y),
            z: z.map(|z| self.vector(z)),
            ..Scatter::default()
        };
        edit(&mut scatter, self);
        self.push(axes, Artist::Scatter(scatter));
        id
    }

    /// Samples `f(x, y)` on the rectilinear grid `x × y` and returns the grid and the field.
    pub fn field(&mut self, x: &[f64], y: &[f64], f: impl Fn(f64, f64) -> f64) -> (Grid, DataId) {
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

    /// Finishes the figure.
    pub fn build(self) -> Figure {
        self.fig
    }
}

/// Shorthand for LaTeX-interpreted text.
pub fn text(content: &str) -> Option<Text> {
    Some(Text::new(content))
}
