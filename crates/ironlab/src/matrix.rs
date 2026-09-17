//! Two-dimensional arrays and sampling helpers.

use std::ops::{Index, IndexMut};

use ironlab_ir::IrError;

/// A dense two-dimensional array of values stored in row-major order.
///
/// A matrix sampled on a grid has one row per y coordinate and one column per x
/// coordinate, as in MATLAB: the value in row `j` and column `i` belongs to the point
/// `(x[i], y[j])`. Values are indexed by `(row, col)`, both counted from zero.
///
/// ```
/// use ironlab::Matrix;
///
/// let m = Matrix::from_rows(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
/// assert_eq!((m.rows(), m.cols()), (2, 3));
/// assert_eq!(m[(1, 0)], 4.0);
/// assert_eq!(m.values(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    rows: usize,
    cols: usize,
    values: Vec<f64>,
}

impl Matrix {
    /// Creates a matrix of zeros.
    #[must_use]
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            values: vec![0.0; rows * cols],
        }
    }

    /// Creates a matrix whose value in each row and column is computed by a function
    /// of the row and column indices.
    ///
    /// ```
    /// use ironlab::{Matrix, linspace};
    ///
    /// let x = linspace(0.0, 1.0, 5);
    /// let y = linspace(0.0, 2.0, 3);
    /// let z = Matrix::from_fn(y.len(), x.len(), |row, col| x[col] * y[row]);
    /// assert_eq!(z[(2, 4)], 2.0);
    /// ```
    #[must_use]
    pub fn from_fn(rows: usize, cols: usize, mut f: impl FnMut(usize, usize) -> f64) -> Self {
        let values = (0..rows)
            .flat_map(|row| (0..cols).map(move |col| (row, col)))
            .map(|(row, col)| f(row, col))
            .collect();
        Self { rows, cols, values }
    }

    /// Creates a matrix from a slice of rows of equal length.
    ///
    /// ```
    /// use ironlab::Matrix;
    ///
    /// let m = Matrix::from_rows(&[[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]);
    /// assert_eq!((m.rows(), m.cols()), (3, 2));
    /// ```
    ///
    /// # Panics
    ///
    /// Panics with a message containing `matrix rows have different lengths` when the
    /// rows do not all have the same length.
    #[must_use]
    pub fn from_rows<R: AsRef<[f64]>>(rows: &[R]) -> Self {
        let cols = rows.first().map_or(0, |row| row.as_ref().len());
        assert!(
            rows.iter().all(|row| row.as_ref().len() == cols),
            "matrix rows have different lengths"
        );
        Self {
            rows: rows.len(),
            cols,
            values: rows.iter().flat_map(|row| row.as_ref()).copied().collect(),
        }
    }

    /// Creates a matrix from values in row-major order.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::InvalidShape`] when the number of values is not
    /// `rows * cols`.
    pub fn from_vec(rows: usize, cols: usize, values: Vec<f64>) -> Result<Self, IrError> {
        if rows.checked_mul(cols) == Some(values.len()) {
            Ok(Self { rows, cols, values })
        } else {
            Err(IrError::InvalidShape {
                shape: vec![rows, cols],
                len: values.len(),
            })
        }
    }

    /// Returns the number of rows.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Returns the number of columns.
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Returns the values in row-major order.
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// Returns the values of one row.
    ///
    /// # Panics
    ///
    /// Panics with a message containing `out of range` when the row index is not less
    /// than the number of rows.
    #[must_use]
    pub fn row(&self, row: usize) -> &[f64] {
        assert!(
            row < self.rows,
            "row {row} out of range for a matrix of {} rows",
            self.rows
        );
        let start = row * self.cols;
        &self.values[start..start + self.cols]
    }

    /// Returns a matrix of the same shape whose values are a function of this
    /// matrix's values.
    #[must_use]
    pub fn map(&self, mut f: impl FnMut(f64) -> f64) -> Self {
        Self {
            rows: self.rows,
            cols: self.cols,
            values: self.values.iter().map(|&v| f(v)).collect(),
        }
    }

    /// Returns a matrix whose values are a function of the corresponding values of
    /// this matrix and another of the same shape.
    ///
    /// This evaluates a field on the matrices returned by [`meshgrid`]:
    ///
    /// ```
    /// use ironlab::{linspace, meshgrid};
    ///
    /// let (x, y) = meshgrid(&linspace(-1.0, 1.0, 3), &linspace(0.0, 1.0, 2));
    /// let r2 = x.zip_map(&y, |x, y| x * x + y * y);
    /// assert_eq!(r2[(1, 0)], 2.0);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics with a message containing `matrices have different shapes` when the
    /// matrices have different numbers of rows or columns.
    #[must_use]
    pub fn zip_map(&self, other: &Matrix, mut f: impl FnMut(f64, f64) -> f64) -> Self {
        assert!(
            (self.rows, self.cols) == (other.rows, other.cols),
            "matrices have different shapes: {}x{} and {}x{}",
            self.rows,
            self.cols,
            other.rows,
            other.cols
        );
        Self {
            rows: self.rows,
            cols: self.cols,
            values: self
                .values
                .iter()
                .zip(&other.values)
                .map(|(&a, &b)| f(a, b))
                .collect(),
        }
    }

    /// Consumes the matrix and returns its values in row-major order.
    #[must_use]
    pub fn into_values(self) -> Vec<f64> {
        self.values
    }

    /// Returns the position of `(row, col)` in the row-major values, checking each
    /// index against its own dimension.
    fn flat_index(&self, row: usize, col: usize) -> usize {
        assert!(
            row < self.rows && col < self.cols,
            "index ({row}, {col}) out of range for a {}x{} matrix",
            self.rows,
            self.cols
        );
        row * self.cols + col
    }
}

impl Index<(usize, usize)> for Matrix {
    type Output = f64;

    /// Returns the value at `(row, col)`.
    ///
    /// # Panics
    ///
    /// Panics with a message containing `out of range` when the row is not less than
    /// the number of rows or the column is not less than the number of columns. Each
    /// index is checked separately, so a column past the end of a row never reads a
    /// value of the next row.
    fn index(&self, (row, col): (usize, usize)) -> &f64 {
        &self.values[self.flat_index(row, col)]
    }
}

impl IndexMut<(usize, usize)> for Matrix {
    /// Returns the value at `(row, col)`, mutably.
    ///
    /// # Panics
    ///
    /// Panics under the same conditions, and with the same message, as
    /// [`Index::index`].
    fn index_mut(&mut self, (row, col): (usize, usize)) -> &mut f64 {
        let index = self.flat_index(row, col);
        &mut self.values[index]
    }
}

/// Returns `n` evenly spaced values from `start` to `end` inclusive.
///
/// As in MATLAB, the first value is exactly `start` and the last is exactly `end`;
/// one value yields `[end]` and zero values yield an empty vector.
///
/// ```
/// use ironlab::linspace;
///
/// assert_eq!(linspace(0.0, 1.0, 5), vec![0.0, 0.25, 0.5, 0.75, 1.0]);
/// ```
#[must_use]
pub fn linspace(start: f64, end: f64, n: usize) -> Vec<f64> {
    match n {
        0 => Vec::new(),
        1 => vec![end],
        _ => {
            let intervals = (n - 1) as f64;
            let step = (end - start) / intervals;
            let mut values: Vec<f64> = (0..n).map(|i| start + i as f64 * step).collect();
            values[n - 1] = end;
            values
        }
    }
}

/// Returns `n` logarithmically spaced values from `10^start_exp` to `10^end_exp`
/// inclusive.
///
/// The exponents are evenly spaced as by [`linspace`]. A value whose exponent is an
/// integer is computed by integer exponentiation, so that decades such as 0.01 and
/// 1000 are the closest floating-point numbers to their exact values and logarithmic
/// axis limits computed from them fall exactly on decades.
///
/// ```
/// use ironlab::logspace;
///
/// assert_eq!(logspace(-1.0, 2.0, 4), vec![0.1, 1.0, 10.0, 100.0]);
/// ```
#[must_use]
pub fn logspace(start_exp: f64, end_exp: f64, n: usize) -> Vec<f64> {
    linspace(start_exp, end_exp, n)
        .into_iter()
        .map(|exponent| {
            if exponent.fract() == 0.0 && exponent.abs() <= f64::from(i32::MAX) {
                // The exponent is an integer in range, so the cast is exact.
                10f64.powi(exponent as i32)
            } else {
                10f64.powf(exponent)
            }
        })
        .collect()
}

/// Returns the coordinates of every point of the rectilinear grid defined by the
/// vectors `x` and `y`, as two matrices of shape `y.len()` by `x.len()`.
///
/// As in MATLAB, x varies along the columns and y varies along the rows: the first
/// matrix holds `x[col]` in every row and the second holds `y[row]` in every column.
///
/// ```
/// use ironlab::meshgrid;
///
/// let (xx, yy) = meshgrid(&[1.0, 2.0, 3.0], &[10.0, 20.0]);
/// assert_eq!((xx.rows(), xx.cols()), (2, 3));
/// assert_eq!(xx[(1, 2)], 3.0);
/// assert_eq!(yy[(1, 2)], 20.0);
/// ```
#[must_use]
pub fn meshgrid(x: &[f64], y: &[f64]) -> (Matrix, Matrix) {
    (
        Matrix::from_fn(y.len(), x.len(), |_, col| x[col]),
        Matrix::from_fn(y.len(), x.len(), |row, _| y[row]),
    )
}
