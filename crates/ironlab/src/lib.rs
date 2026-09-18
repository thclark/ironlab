//! MATLAB-flavoured Rust API for building, viewing and exporting scientific figures.
//!
//! A [`Figure`] is built by placing axes in a grid of tiles and adding plots to them
//! with functions named after their MATLAB equivalents: [`plot`](AxesMut::plot),
//! [`scatter`](AxesMut::scatter), [`contour`](AxesMut::contour),
//! [`quiver`](AxesMut::quiver), [`surf`](AxesMut::surf) and their relatives. Each
//! plotting function returns a handle whose chained setters change the properties of
//! the new plot, in the way that MATLAB name–value arguments do. The figure can then be
//! shown in the interactive viewer, saved as a `.fig` file (or as JSON) or exported to PDF.
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

pub use ironlab_ir as ir;

pub use artists::{ContourMut, LineMut, QuiverMut, ScatterMut, SurfaceMut};
pub use axes::AxesMut;
pub use color::IntoColorSpec;
pub use error::Error;
pub use figure::Figure;
pub use grid::GridCoords;
pub use matrix::{Matrix, linspace, logspace, meshgrid};

/// The coordinate dimension of an axes, used to link axes and set limits.
pub use ironlab_ir::Dimension as Dim;

/// The shape of the markers drawn at data points.
pub use ironlab_ir::MarkerShape as Marker;

/// The dash pattern of a line.
pub use ironlab_ir::DashStyle as Dash;

/// The name of a colormap.
pub use ironlab_ir::ColormapName as Colormap;

pub use ironlab_ir::{
    Color, ColorSpec, Interpreter, LegendLocation, NodeId, Parameter, Scale, Text, ValidationReport,
};

/// How a dense artist is drawn when the figure is exported, and at what resolution it is
/// rasterised. See [`Figure::export_pdf_with`].
pub use ironlab_pdf::{RasterOptions, RasterPolicy};

/// The types and functions needed to build figures, for glob import.
///
/// ```
/// use ironlab::prelude::*;
/// ```
pub mod prelude {
    pub use crate::AxesMut;
    pub use crate::{
        Color, ColorSpec, Colormap, ContourMut, Dash, Dim, Error, Figure, GridCoords,
        IntoColorSpec, LegendLocation, LineMut, Marker, Matrix, NodeId, Parameter, QuiverMut,
        RasterOptions, RasterPolicy, Scale, ScatterMut, SurfaceMut, Text, linspace, logspace,
        meshgrid,
    };
}
