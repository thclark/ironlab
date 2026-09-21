//! Coordinates of gridded data.

use ironlab_ir::{DataId, Grid, NdArray, Values};

use crate::matrix::Matrix;

/// The coordinates of a grid along one dimension, passed as the `x` or `y` argument of
/// gridded plots such as [`contour`](crate::AxesMut::contour),
/// [`surf`](crate::AxesMut::surf) and [`surface`](crate::AxesMut::surface).
///
/// A vector gives one coordinate per column (for `x`) or per row (for `y`) and
/// describes an axis-aligned (rectilinear) grid. A matrix gives the coordinate of
/// every grid node, with the same shape as the field, and describes a curvilinear
/// grid.
///
/// When both coordinates are vectors, the plot stores a rectilinear grid. Otherwise it
/// stores a curvilinear grid: a vector whose length matches the corresponding
/// dimension of the field is first repeated into a matrix of the field's shape, as
/// [`meshgrid`](crate::meshgrid) would, and a vector whose length does not match is
/// stored unchanged so that [`Figure::validate`](crate::Figure::validate) reports the
/// mismatch.
///
/// Vectors are the preferred form for rectilinear data, because they are stored once
/// per dimension rather than once per node.
#[derive(Debug, Clone, PartialEq)]
pub enum GridCoords {
    /// One coordinate per column or per row of a rectilinear grid.
    Vector(Vec<f64>),
    /// One coordinate per node of a curvilinear grid.
    Matrix(Matrix),
}

impl From<Vec<f64>> for GridCoords {
    fn from(values: Vec<f64>) -> Self {
        GridCoords::Vector(values)
    }
}

impl From<&Vec<f64>> for GridCoords {
    fn from(values: &Vec<f64>) -> Self {
        GridCoords::Vector(values.clone())
    }
}

impl From<&[f64]> for GridCoords {
    fn from(values: &[f64]) -> Self {
        GridCoords::Vector(values.to_vec())
    }
}

impl<const N: usize> From<[f64; N]> for GridCoords {
    fn from(values: [f64; N]) -> Self {
        GridCoords::Vector(values.to_vec())
    }
}

impl<const N: usize> From<&[f64; N]> for GridCoords {
    fn from(values: &[f64; N]) -> Self {
        GridCoords::Vector(values.to_vec())
    }
}

impl From<Matrix> for GridCoords {
    fn from(matrix: Matrix) -> Self {
        GridCoords::Matrix(matrix)
    }
}

impl From<&Matrix> for GridCoords {
    fn from(matrix: &Matrix) -> Self {
        GridCoords::Matrix(matrix.clone())
    }
}

/// The dimension of a field along which a grid coordinate varies.
#[derive(Clone, Copy)]
enum Along {
    /// The coordinate varies along the columns of the field (x).
    Cols,
    /// The coordinate varies along the rows of the field (y).
    Rows,
}

/// Stores the coordinates of a grid for a field in the figure's data table and returns
/// the grid that refers to them, following the rules documented on [`GridCoords`].
pub(crate) fn store_grid(
    fig: &mut ironlab_ir::Figure,
    x: GridCoords,
    y: GridCoords,
    field: &Matrix,
) -> Grid {
    match (x, y) {
        (GridCoords::Vector(x), GridCoords::Vector(y)) => Grid::Rectilinear {
            x: fig.add_data(NdArray::vector(x)),
            y: fig.add_data(NdArray::vector(y)),
        },
        (x, y) => Grid::Curvilinear {
            x: store_node_coordinates(fig, x, field, Along::Cols),
            y: store_node_coordinates(fig, y, field, Along::Rows),
        },
    }
}

/// Stores the coordinates of every node of a curvilinear grid along one dimension.
///
/// A vector whose length matches the field along that dimension is repeated to the
/// field's shape; any other vector is stored unchanged, so that validation reports
/// the mismatch.
fn store_node_coordinates(
    fig: &mut ironlab_ir::Figure,
    coords: GridCoords,
    field: &Matrix,
    along: Along,
) -> DataId {
    let (rows, cols) = (field.rows(), field.cols());
    let matrix = match coords {
        GridCoords::Matrix(matrix) => matrix,
        GridCoords::Vector(v) => match along {
            Along::Cols if v.len() == cols => Matrix::from_fn(rows, cols, |_, col| v[col]),
            Along::Rows if v.len() == rows => Matrix::from_fn(rows, cols, |row, _| v[row]),
            _ => return fig.add_data(NdArray::vector(v)),
        },
    };
    fig.add_data(matrix_array(&matrix))
}

/// Converts a matrix into a two-dimensional array of shape `[rows, cols]`.
pub(crate) fn matrix_array(matrix: &Matrix) -> NdArray {
    NdArray {
        shape: vec![matrix.rows(), matrix.cols()],
        values: Values::F64(matrix.values().to_vec()),
    }
}
