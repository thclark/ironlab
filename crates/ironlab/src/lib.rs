//! MATLAB-flavoured Rust API for building, viewing and exporting scientific figures.
//!
//! A [`Figure`] is built by placing axes in a grid of tiles and adding plots to them
//! with functions named after their MATLAB equivalents: [`plot`](AxesMut::plot),
//! [`scatter`](AxesMut::scatter), [`contour`](AxesMut::contour),
//! [`quiver`](AxesMut::quiver), [`surf`](AxesMut::surf), [`image`](AxesMut::image)
//! and their relatives (MATLAB's `imagesc` is [`mapped_image`](AxesMut::mapped_image),
//! and MATLAB's `pcolor` is [`surface`](AxesMut::surface) in a two-dimensional axes).
//! Each plotting function returns a handle whose chained setters change the properties
//! of the new plot, in the way that MATLAB name–value arguments do. The figure can then
//! be shown in the interactive viewer, saved as a `.fig` file (or as JSON) or exported
//! to PDF.
//!
//! Every call writes directly to the retained figure IR of the [`ir`] crate, which is
//! the single source of truth for what is drawn. Builder calls never panic because of
//! inconsistent input (such as arrays of different lengths or a tile outside the
//! layout); such problems are reported by [`Figure::validate`], and are checked again
//! before the figure is exported or shown.
//!
//! # Example
//!
//! ```
//! use ironlab::prelude::*;
//!
//! let x = linspace(0.0, 2.0 * std::f64::consts::PI, 200);
//! let sin: Vec<f64> = x.iter().map(|x| x.sin()).collect();
//! let cos: Vec<f64> = x.iter().map(|x| x.cos()).collect();
//!
//! let mut fig = Figure::new().size_mm(120.0, 80.0).title("Trigonometric functions");
//! let mut ax = fig.axes(0, 0);
//! ax.plot(&x, &sin).display_name("$\\sin x$");
//! ax.plot(&x, &cos).display_name("$\\cos x$").dash(Dash::Dashed);
//! ax.xlabel("$x$").ylabel("$f(x)$").legend(LegendLocation::NorthEast);
//!
//! assert!(fig.validate().is_valid());
//! ```
//!
//! Showing and exporting a figure:
//!
//! ```no_run
//! use ironlab::prelude::*;
//!
//! # fn main() -> Result<(), ironlab::Error> {
//! let x = linspace(-1.5, 1.5, 61);
//! let y = linspace(-1.5, 1.5, 61);
//! let z = Matrix::from_fn(y.len(), x.len(), |row, col| x[col].powi(2) - y[row].powi(2));
//!
//! let mut fig = Figure::new().title("A saddle");
//! fig.axes(0, 0).surf(&x, &y, &z);
//! fig.export_pdf("saddle.pdf")?;
//! fig.show()?;
//! # Ok(())
//! # }
//! ```

mod artists;
mod axes;
mod color;
mod error;
mod figure;
mod grid;
mod matrix;
mod pixels;

pub use ironlab_ir as ir;

pub use artists::{
    ContourMut, ImageMut, IndexedImageMut, LineMut, MappedImageMut, QuiverMut, ScatterMut,
    SurfaceMut,
};
pub use axes::AxesMut;
pub use color::IntoColorSpec;
pub use error::Error;
pub use figure::Figure;
pub use grid::GridCoords;
pub use matrix::{Matrix, linspace, logspace, meshgrid};
pub use pixels::{ByteMatrix, ImageValues, Pixels};

/// The coordinate dimension of an axes, used to link axes and set limits.
pub use ironlab_ir::Dimension as Dim;

/// The shape of the markers drawn at data points.
pub use ironlab_ir::MarkerShape as Marker;

/// The dash pattern of a line.
pub use ironlab_ir::DashStyle as Dash;

/// The name of a colormap.
pub use ironlab_ir::ColormapName as Colormap;

/// The plane of an axes in which an image lies, with its offset along the third axis.
pub use ironlab_ir::ImagePlane;

/// What a colour-indexed or colour-mapped image draws for a pixel it cannot colour.
pub use ironlab_ir::OutOfRange;

pub use ironlab_ir::{
    Color, ColorSpec, Interpreter, IssueKind, LegendLocation, NodeId, Parameter, Scale, Text,
    ValidationIssue, ValidationReport,
};

/// How a dense artist is drawn when the figure is exported, and at what resolution it is
/// rasterised. See [`Figure::export_pdf_with`].
pub use ironlab_pdf::{RasterOptions, RasterPolicy};

/// A problem the scene compiler found while drawing a figure that did not prevent the
/// figure from being drawn, naming the node it concerns. See [`ExportReport`].
pub use ironlab_scene::SceneWarning;

/// What an export left off the page, returned by [`Figure::export_pdf`] and
/// [`Figure::export_pdf_with`].
///
/// A warning never refuses a figure, so the page is written whatever the report holds;
/// the report tells a program what the page does not show. Both lists are empty for a
/// figure from which nothing was left out. The two lists overlap where the compiler
/// leaves out an artist that validation warned of, such as a surface whose field has a
/// single row, and differ where the compiler finds a reason that validation cannot see,
/// such as a piece of LaTeX the typesetter does not support. A program that wants one
/// reason per artist reads `validation`; one that wants every reason reads both.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportReport {
    /// The warnings of the figure's validation, as [`Figure::validate`] returns them: an
    /// artist with nothing to draw, data that a logarithmic axis cannot show, or an image
    /// that cannot be placed. Each names the node it concerns.
    pub validation: Vec<ValidationIssue>,
    /// The warnings the scene compiler raised while drawing the figure, each naming the
    /// node it concerns.
    pub scene: Vec<SceneWarning>,
}

/// The types and functions needed to build figures, for glob import.
///
/// ```
/// use ironlab::prelude::*;
/// ```
pub mod prelude {
    pub use crate::AxesMut;
    pub use crate::{
        ByteMatrix, Color, ColorSpec, Colormap, ContourMut, Dash, Dim, Error, Figure, GridCoords,
        ImageMut, ImagePlane, ImageValues, IndexedImageMut, IntoColorSpec, LegendLocation, LineMut,
        MappedImageMut, Marker, Matrix, NodeId, OutOfRange, Parameter, Pixels, QuiverMut,
        RasterOptions, RasterPolicy, Scale, ScatterMut, SurfaceMut, Text, linspace, logspace,
        meshgrid,
    };
}
