//! The figure builder.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use ironlab_ir::{Axes, Cell, Dimension, NodeId, Parameter, Projection, Text, ValidationReport};
use ironlab_text::TextEngine;

use crate::axes::AxesMut;
use crate::error::Error;

/// A figure: a page of a fixed physical size holding axes arranged in a grid of tiles.
///
/// Figure-level properties are set with consuming builder methods when the figure is
/// created, and axes are then obtained with [`axes`](Figure::axes),
/// [`axes3`](Figure::axes3) or [`axes_span`](Figure::axes_span).
///
/// ```
/// use ironlab::prelude::*;
///
/// # fn main() -> Result<(), ironlab::Error> {
/// let mut fig = Figure::new()
///     .size_mm(160.0, 60.0)
///     .tiles(1, 2)
///     .title("Two panels")
///     .font_size_pt(8.0);
/// fig.axes(0, 0).plot([0.0, 1.0], [0.0, 1.0]);
/// fig.axes(0, 1).plot([0.0, 1.0], [1.0, 0.0]);
/// fig.link_all_y()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Figure {
    ir: ironlab_ir::Figure,
}

impl Figure {
    /// Creates an empty figure with default properties: 160 mm by 100 mm, a base font
    /// size of 9 pt, a white background and a layout of one tile.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the physical width and height of the figure in millimetres.
    #[must_use]
    pub fn size_mm(mut self, width: f64, height: f64) -> Self {
        self.ir.size.width_mm = width;
        self.ir.size.height_mm = height;
        self
    }

    /// Sets the title drawn above all axes (MATLAB's `sgtitle`).
    ///
    /// Segments of the title delimited by `$…$` are typeset as LaTeX mathematics; pass
    /// [`Text::plain`] to render dollar signs literally.
    #[must_use]
    pub fn title(mut self, title: impl Into<Text>) -> Self {
        self.ir.title = Some(title.into());
        self
    }

    /// Sets the base font size in points, from which title and tick label sizes are
    /// scaled.
    #[must_use]
    pub fn font_size_pt(mut self, size: f64) -> Self {
        self.ir.font_size_pt = size;
        self
    }

    /// Sets the number of rows and columns of tiles in which axes are placed (MATLAB's
    /// `tiledlayout`).
    ///
    /// Axes placed outside the layout are not rejected when they are created; they
    /// are reported by [`validate`](Figure::validate).
    #[must_use]
    pub fn tiles(mut self, rows: u32, cols: u32) -> Self {
        self.ir.layout.rows = rows;
        self.ir.layout.cols = cols;
        self
    }

    /// Sets a named parameter that describes the figure, replacing any parameter of the
    /// same name.
    ///
    /// Parameters do not affect drawing; they are saved with the figure so that
    /// collections of figures can be sorted, filtered and searched. A value may be a
    /// `bool`, an integer (`i32` or `i64`), an `f64` or a string, and keeps that kind
    /// when it is saved in either format. A name that is empty, or a number that is not
    /// finite, is reported by [`validate`](Figure::validate).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let fig = Figure::new()
    ///     .parameter("reynolds_number", 1e5)
    ///     .parameter("mesh_cells", 250_000)
    ///     .parameter("solver", "k–ω SST")
    ///     .parameter("converged", true);
    /// assert_eq!(fig.parameters()["mesh_cells"], Parameter::Integer(250_000));
    /// ```
    #[must_use]
    pub fn parameter(mut self, name: impl Into<String>, value: impl Into<Parameter>) -> Self {
        self.ir.parameters.insert(name.into(), value.into());
        self
    }

    /// Returns the parameters of the figure, in ascending order of name.
    #[must_use]
    pub fn parameters(&self) -> &BTreeMap<String, Parameter> {
        &self.ir.parameters
    }

    /// Returns the axes that occupies the tile at a zero-based row and column,
    /// creating a two-dimensional axes in that single tile if no axes occupies it.
    ///
    /// Calling this again with the same tile returns the same axes, and a tile covered
    /// by an axes that spans several tiles returns that axes. An existing axes keeps
    /// its projection, so a three-dimensional axes is returned unchanged.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(2, 1);
    /// let top = fig.axes(0, 0).id();
    /// assert_eq!(fig.axes(0, 0).id(), top);
    /// assert_ne!(fig.axes(1, 0).id(), top);
    /// ```
    pub fn axes(&mut self, row: u32, col: u32) -> AxesMut<'_> {
        let covers = |cell: &Cell| {
            row.checked_sub(cell.row)
                .is_some_and(|offset| offset < cell.row_span)
                && col
                    .checked_sub(cell.col)
                    .is_some_and(|offset| offset < cell.col_span)
        };
        let id = match self.ir.axes.iter().find(|axes| covers(&axes.cell)) {
            Some(axes) => axes.id,
            None => self.add_axes(Cell {
                row,
                col,
                ..Cell::default()
            }),
        };
        AxesMut::new(&mut self.ir, id)
    }

    /// Returns the axes that occupies the tile at a zero-based row and column as a
    /// three-dimensional axes, creating one if no axes occupies the tile.
    ///
    /// An existing two-dimensional axes is converted to three dimensions with the
    /// default view (azimuth −37.5°, elevation 30°), keeping its artists and
    /// properties; an existing three-dimensional axes keeps its view.
    pub fn axes3(&mut self, row: u32, col: u32) -> AxesMut<'_> {
        let mut axes = self.axes(row, col);
        axes.make_3d();
        axes
    }

    /// Returns the axes whose top-left tile is at a zero-based row and column, and
    /// makes it span the given numbers of rows and columns.
    ///
    /// A two-dimensional axes is created when no axes has that top-left tile.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(2, 2);
    /// let wide = fig.axes_span(0, 0, 1, 2).id();
    /// assert_eq!(fig.axes(0, 1).id(), wide);
    /// ```
    pub fn axes_span(&mut self, row: u32, col: u32, row_span: u32, col_span: u32) -> AxesMut<'_> {
        let cell = Cell {
            row,
            col,
            row_span,
            col_span,
        };
        let existing = self
            .ir
            .axes
            .iter_mut()
            .find(|axes| (axes.cell.row, axes.cell.col) == (row, col));
        let id = match existing {
            Some(axes) => {
                axes.cell = cell;
                axes.id
            }
            None => self.add_axes(cell),
        };
        AxesMut::new(&mut self.ir, id)
    }

    /// Links the limits of the given axes along a dimension, so that zooming or
    /// panning one of them, or setting its limits, changes all of them.
    ///
    /// Linking axes that already belong to a group for that dimension merges the
    /// groups. The limits of every axes in the group are set to those of the first
    /// given axes.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// # fn main() -> Result<(), ironlab::Error> {
    /// let t = linspace(0.0, 10.0, 101);
    /// let decay: Vec<f64> = t.iter().map(|t| (-t / 3.0).exp()).collect();
    /// let growth: Vec<f64> = t.iter().map(|t| (t / 3.0).exp()).collect();
    ///
    /// let mut fig = Figure::new().tiles(1, 3);
    /// let left = fig.axes(0, 0).id();
    /// fig.axes(0, 0).plot(&t, &decay);
    /// fig.axes(0, 1).semilogy(&t, &growth);
    /// let right = fig.axes(0, 2).id();
    /// fig.axes(0, 2).plot(&t, &growth);
    ///
    /// // The outer panels share their time axis; the middle panel pans independently.
    /// fig.link(Dim::X, &[left, right])?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ir`] when an identifier is not an axes of this figure, or when an
    /// axes of the group cannot show the limits it would take, such as limits that reach
    /// below zero on a logarithmic axis. Axes that cannot share limits cannot be linked,
    /// so the whole change is refused and no links are changed.
    pub fn link(&mut self, dim: Dimension, axes: &[NodeId]) -> Result<&mut Self, Error> {
        self.ir.link(dim, axes)?;
        Ok(self)
    }

    /// Links the limits of every axes of the figure along a dimension.
    ///
    /// Only axes that exist when this is called are linked.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ir`] when an axes of the figure cannot show the limits of the
    /// first axes, as [`link`](Figure::link) describes; no links are then changed.
    pub fn link_all(&mut self, dim: Dimension) -> Result<&mut Self, Error> {
        self.ir.link_all(dim)?;
        Ok(self)
    }

    /// Links the x limits of every axes of the figure (MATLAB's `linkaxes(ax, 'x')`).
    ///
    /// # Errors
    ///
    /// As [`link_all`](Figure::link_all).
    pub fn link_all_x(&mut self) -> Result<&mut Self, Error> {
        self.link_all(Dimension::X)
    }

    /// Links the y limits of every axes of the figure (MATLAB's `linkaxes(ax, 'y')`).
    ///
    /// # Errors
    ///
    /// As [`link_all`](Figure::link_all).
    pub fn link_all_y(&mut self) -> Result<&mut Self, Error> {
        self.link_all(Dimension::Y)
    }

    /// Returns the figure IR.
    #[must_use]
    pub fn ir(&self) -> &ironlab_ir::Figure {
        &self.ir
    }

    /// Returns the figure IR, mutably, for properties that the builder does not cover.
    pub fn ir_mut(&mut self) -> &mut ironlab_ir::Figure {
        &mut self.ir
    }

    /// Consumes the figure and returns its IR.
    #[must_use]
    pub fn into_ir(self) -> ironlab_ir::Figure {
        self.ir
    }

    /// Wraps an existing figure IR, for example one built by hand or loaded by another
    /// tool, so that it can be extended with the builder.
    #[must_use]
    pub fn from_ir(ir: ironlab_ir::Figure) -> Self {
        Self { ir }
    }

    /// Checks the figure for problems such as arrays of different lengths, axes outside
    /// the tile layout or invalid limits.
    ///
    /// The builder never panics because of such problems, so this is the place to
    /// find them; exporting and showing a figure check it first.
    #[must_use]
    pub fn validate(&self) -> ValidationReport {
        self.ir.validate()
    }

    /// Saves the figure in the format named by the extension of `path`.
    ///
    /// A path ending in `.fig` is written in the default Protocol Buffers format, and a
    /// path ending in `.json` (including `.fig.json`) is written as JSON. Extensions are
    /// matched without regard to case. Both formats describe the figure completely and
    /// are described in the figure schema reference of the documentation.
    ///
    /// The figure is saved even if it has validation errors, so that a figure can be
    /// inspected or repaired later.
    ///
    /// ```no_run
    /// use ironlab::prelude::*;
    ///
    /// # fn main() -> Result<(), ironlab::Error> {
    /// let mut fig = Figure::new();
    /// fig.axes(0, 0).plot([0.0, 1.0, 2.0], [0.0, 1.0, 4.0]);
    /// fig.save("squares.fig")?;
    /// fig.save("squares.fig.json")?;
    /// assert_eq!(Figure::load("squares.fig")?, fig);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFormat`] when the extension is neither `.fig` nor
    /// `.json` (and writes no file), and [`Error::Io`] when the file cannot be written.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let path = path.as_ref();
        match Format::of(path)? {
            Format::Protobuf => std::fs::write(path, self.to_protobuf())?,
            Format::Json => self.save_json(path)?,
        }
        Ok(())
    }

    /// Loads a figure from a file in the format named by the extension of `path`: a
    /// `.fig` file is read as Protocol Buffers and a `.json` file (including
    /// `.fig.json`) as JSON, as for [`save`](Figure::save).
    ///
    /// The loaded figure is not validated; call [`validate`](Figure::validate) to
    /// check it.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedFormat`] when the extension is neither `.fig` nor
    /// `.json` (without reading the file), [`Error::Io`] when the file cannot be read,
    /// and [`Error::Ir`] when its content is not a figure of a compatible schema
    /// version in that format.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        match Format::of(path)? {
            Format::Protobuf => Ok(Self::from_protobuf(&std::fs::read(path)?)?),
            Format::Json => Self::load_json(path),
        }
    }

    /// Saves the figure as JSON, whatever the extension of `path`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the file cannot be written.
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        std::fs::write(path, self.ir.to_json())?;
        Ok(())
    }

    /// Loads a figure from a JSON file, whatever the extension of `path`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the file cannot be read, and [`Error::Ir`] when its
    /// content is not a JSON figure of a compatible schema version.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, Error> {
        let json = std::fs::read_to_string(path)?;
        Ok(Self::from_ir(ironlab_ir::Figure::from_json(&json)?))
    }

    /// Encodes the figure as Protocol Buffers bytes, the content of a `.fig` file and
    /// the form in which figures are transported between processes.
    #[must_use]
    pub fn to_protobuf(&self) -> Vec<u8> {
        self.ir.to_protobuf()
    }

    /// Decodes a figure from Protocol Buffers bytes, as written by
    /// [`to_protobuf`](Figure::to_protobuf).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ir`] when the bytes are not a figure of a compatible schema
    /// version.
    pub fn from_protobuf(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self::from_ir(ironlab_ir::Figure::from_protobuf(bytes)?))
    }

    /// Exports the figure as a single-page PDF whose page is the size of the figure.
    ///
    /// Text is embedded as real, selectable text in the bundled fonts, so the PDF can
    /// be included unscaled in a LaTeX document.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the figure has validation errors (and writes no
    /// file), [`Error::Pdf`] when the exporter fails and [`Error::Io`] when the file
    /// cannot be written.
    pub fn export_pdf(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        self.check_valid()?;
        let bytes = ironlab_pdf::export_pdf(&self.ir, text_engine())?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Opens the figure in the interactive viewer and blocks until the window is
    /// closed.
    ///
    /// The viewer supports panning, zooming, rotating three-dimensional axes, toggling
    /// series from the legend and exporting to PDF.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Invalid`] when the figure has validation errors (and opens no
    /// window), and [`Error::Viewer`] when the viewer cannot be started.
    pub fn show(self) -> Result<(), Error> {
        self.check_valid()?;
        let name = self
            .ir
            .title
            .as_ref()
            .map_or_else(|| "Figure".to_owned(), |title| title.content.clone());
        ironlab_viewer::run(vec![(name, self.ir)])?;
        Ok(())
    }

    /// Returns [`Error::Invalid`] when the figure has validation errors.
    fn check_valid(&self) -> Result<(), Error> {
        let report = self.validate();
        if report.is_valid() {
            Ok(())
        } else {
            Err(Error::Invalid(report))
        }
    }

    /// Adds a two-dimensional axes occupying a cell and returns its identifier.
    fn add_axes(&mut self, cell: Cell) -> NodeId {
        let id = self.ir.alloc_node_id();
        self.ir.axes.push(Axes {
            id,
            cell,
            projection: Projection::TwoD,
            ..Axes::default()
        });
        id
    }
}

impl From<ironlab_ir::Figure> for Figure {
    fn from(ir: ironlab_ir::Figure) -> Self {
        Self::from_ir(ir)
    }
}

impl From<Figure> for ironlab_ir::Figure {
    fn from(figure: Figure) -> Self {
        figure.into_ir()
    }
}

/// A file format of a figure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    /// Protocol Buffers, the default format, in a `.fig` file.
    Protobuf,
    /// JSON, the secondary format, in a `.json` or `.fig.json` file.
    Json,
}

impl Format {
    /// Returns the format named by the extension of a path.
    ///
    /// A `.fig.json` file has the extension `json`, so it is JSON.
    fn of(path: &Path) -> Result<Self, Error> {
        let extension = path.extension().and_then(|extension| extension.to_str());
        match extension {
            Some(ext) if ext.eq_ignore_ascii_case("fig") => Ok(Self::Protobuf),
            Some(ext) if ext.eq_ignore_ascii_case("json") => Ok(Self::Json),
            _ => Err(Error::UnsupportedFormat(path.to_path_buf())),
        }
    }
}

/// Returns the text engine shared by every export in this process.
///
/// Creating an engine parses the bundled fonts, and its layout memo is only useful when
/// reused, so one engine is created on first use and kept for the life of the process.
fn text_engine() -> &'static TextEngine {
    static ENGINE: OnceLock<TextEngine> = OnceLock::new();
    ENGINE.get_or_init(TextEngine::new)
}
