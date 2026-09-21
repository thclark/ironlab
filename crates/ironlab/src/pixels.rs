//! Containers for the pixels of images: true-colour pixels, matrices of bytes and the
//! values of colour-indexed and colour-mapped images.

use std::ops::{Index, IndexMut};

use ironlab_ir::{Color, IrError, NdArray, Values};

use crate::error::Error;
use crate::matrix::Matrix;

/// The pixels of a true-colour image: 8-bit components stored together, pixel by
/// pixel, in row-major order.
///
/// The image has one row per y coordinate and one column per x coordinate, as a
/// [`Matrix`] has, so the pixel in row `j` and column `i` is drawn at the centre
/// `(x[i], y[j])`. Each pixel holds three components (red, green and blue) or four
/// (with alpha), one byte each, in the order in which image files and decoders lay
/// them out, so the bytes of a decoded image are used as they are. Alpha is straight
/// (not premultiplied).
///
/// Pixels are built from bytes ([`from_rgb8`](Pixels::from_rgb8) and
/// [`from_rgba8`](Pixels::from_rgba8)), from a function of the row and column
/// ([`rgb_from_fn`](Pixels::rgb_from_fn) and [`rgba_from_fn`](Pixels::rgba_from_fn))
/// or from one matrix per component ([`from_planes`](Pixels::from_planes)), and an
/// alpha plane is attached with [`with_alpha`](Pixels::with_alpha). They are drawn by
/// [`image`](crate::AxesMut::image).
///
/// ```
/// use ironlab::prelude::*;
///
/// let pixels = Pixels::rgb_from_fn(64, 64, |row, col| {
///     Color::rgb(row as f32 / 63.0, col as f32 / 63.0, 0.5)
/// });
/// assert_eq!((pixels.rows(), pixels.cols(), pixels.channels()), (64, 64, 3));
///
/// let mut fig = Figure::new();
/// fig.axes(0, 0).image(&pixels).pixel_columns(-1.0, 1.0).pixel_rows(-1.0, 1.0);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Pixels {
    rows: usize,
    cols: usize,
    channels: usize,
    bytes: Vec<u8>,
}

impl Pixels {
    /// Creates opaque pixels from the bytes of an image with three components per
    /// pixel: the red, green and blue bytes of the pixel in row 0 and column 0, then
    /// those of the next pixel along the row, and so on row by row.
    ///
    /// The bytes are copied, so a buffer decoded from a file may be dropped afterwards.
    ///
    /// ```
    /// use ironlab::Pixels;
    ///
    /// // A red pixel beside a green one, over a blue pixel beside a white one.
    /// let bytes = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    /// let pixels = Pixels::from_rgb8(2, 2, &bytes).unwrap();
    /// assert_eq!((pixels.rows(), pixels.cols(), pixels.channels()), (2, 2, 3));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ir`] holding [`IrError::InvalidShape`] when the number of bytes
    /// is not `rows * cols * 3`, or when that product overflows.
    pub fn from_rgb8(rows: usize, cols: usize, bytes: &[u8]) -> Result<Self, Error> {
        Self::from_bytes(rows, cols, 3, bytes)
    }

    /// Creates pixels from the bytes of an image with four components per pixel: red,
    /// green, blue and alpha, laid out as for [`from_rgb8`](Pixels::from_rgb8).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ir`] holding [`IrError::InvalidShape`] when the number of bytes
    /// is not `rows * cols * 4`, or when that product overflows.
    pub fn from_rgba8(rows: usize, cols: usize, bytes: &[u8]) -> Result<Self, Error> {
        Self::from_bytes(rows, cols, 4, bytes)
    }

    /// Creates opaque pixels whose colour in each row and column is computed by a
    /// function of the row and column indices, as [`Matrix::from_fn`] computes values.
    ///
    /// Each component of the colour is quantised to the nearest of 256 levels after
    /// being clamped to the range 0 to 1: 0.5 becomes 128, a component that has strayed
    /// outside the range (an infinite one included) takes the nearer end, and a NaN
    /// component becomes 0. The alpha of the colour is ignored, so the pixels have
    /// three channels; [`rgba_from_fn`](Pixels::rgba_from_fn) keeps it.
    ///
    /// ```
    /// use ironlab::{Color, Pixels};
    ///
    /// let pixels = Pixels::rgb_from_fn(1, 2, |_, col| Color::rgb(col as f32, 0.5, 0.0));
    /// assert_eq!(pixels.bytes(), &[0, 128, 0, 255, 128, 0]);
    /// ```
    #[must_use]
    pub fn rgb_from_fn(rows: usize, cols: usize, f: impl FnMut(usize, usize) -> Color) -> Self {
        Self::from_fn(rows, cols, 3, f)
    }

    /// Creates pixels whose colour and opacity in each row and column are computed by a
    /// function of the row and column indices.
    ///
    /// The components are quantised as by [`rgb_from_fn`](Pixels::rgb_from_fn), and
    /// the alpha of each colour is kept as a fourth channel, so an opaque colour gives
    /// an alpha byte of 255.
    ///
    /// ```
    /// use ironlab::{Color, Pixels};
    ///
    /// let pixels = Pixels::rgba_from_fn(1, 2, |_, col| Color::rgba(1.0, 0.0, 0.0, col as f32));
    /// assert_eq!(pixels.bytes(), &[255, 0, 0, 0, 255, 0, 0, 255]);
    /// ```
    #[must_use]
    pub fn rgba_from_fn(rows: usize, cols: usize, f: impl FnMut(usize, usize) -> Color) -> Self {
        Self::from_fn(rows, cols, 4, f)
    }

    /// Creates opaque pixels from one matrix per component: the red, green and blue
    /// planes of the image, as MATLAB's `cat(3, r, g, b)` builds a true-colour array.
    ///
    /// Each component is quantised as by [`rgb_from_fn`](Pixels::rgb_from_fn), except
    /// that a component that is not finite is refused: a plane is usually computed, and
    /// a NaN or an infinity in it means that the computation failed, which clamping
    /// would hide.
    ///
    /// ```
    /// use ironlab::{Matrix, Pixels, linspace, meshgrid};
    ///
    /// let (r, g) = meshgrid(&linspace(0.0, 1.0, 16), &linspace(0.0, 1.0, 16));
    /// let b = Matrix::zeros(16, 16);
    /// let pixels = Pixels::from_planes(&r, &g, &b).unwrap();
    /// assert_eq!((pixels.rows(), pixels.cols(), pixels.channels()), (16, 16, 3));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::PlaneShapeMismatch`] when the green or blue plane does not have
    /// the shape of the red plane, and [`Error::NonFiniteComponent`] when a component is
    /// NaN or infinite, naming its pixel and its channel (0 for red, 1 for green and 2
    /// for blue).
    pub fn from_planes(r: &Matrix, g: &Matrix, b: &Matrix) -> Result<Self, Error> {
        let expected = [r.rows(), r.cols()];
        check_shape(expected, g)?;
        check_shape(expected, b)?;
        let mut bytes = Vec::with_capacity(r.values().len() * 3);
        let pixels = r.values().iter().zip(g.values()).zip(b.values());
        for (index, ((&red, &green), &blue)) in pixels.enumerate() {
            let (row, col) = (index / r.cols(), index % r.cols());
            for (channel, component) in [red, green, blue].into_iter().enumerate() {
                bytes.push(checked_level(component, row, col, channel)?);
            }
        }
        Ok(Self {
            rows: r.rows(),
            cols: r.cols(),
            channels: 3,
            bytes,
        })
    }

    /// Attaches an alpha plane to the pixels: the opacity of each pixel, from 0
    /// (transparent) to 1 (opaque), as a matrix of the pixels' shape.
    ///
    /// The colour bytes are kept. Each opacity is quantised as by
    /// [`from_planes`](Pixels::from_planes) and stored as the fourth channel of its
    /// pixel, replacing the alpha of pixels that already have one.
    ///
    /// ```
    /// use ironlab::{Color, Matrix, Pixels};
    ///
    /// let pixels = Pixels::rgb_from_fn(2, 2, |_, _| Color::WHITE);
    /// let mask = Matrix::from_rows(&[[1.0, 0.0], [0.5, 1.0]]);
    /// let faded = pixels.with_alpha(&mask).unwrap();
    /// assert_eq!(faded.channels(), 4);
    /// assert_eq!(&faded.bytes()[4..8], &[255, 255, 255, 0]);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::PlaneShapeMismatch`] when the plane does not have the shape of
    /// the pixels, and [`Error::NonFiniteComponent`] in channel 3 when an opacity is NaN
    /// or infinite.
    pub fn with_alpha(self, alpha: &Matrix) -> Result<Self, Error> {
        check_shape([self.rows, self.cols], alpha)?;
        let mut bytes = Vec::with_capacity(alpha.values().len() * 4);
        let pixels = self.bytes.chunks_exact(self.channels).zip(alpha.values());
        for (index, (pixel, &opacity)) in pixels.enumerate() {
            bytes.extend_from_slice(&pixel[..3]);
            bytes.push(checked_level(
                opacity,
                index / self.cols,
                index % self.cols,
                3,
            )?);
        }
        Ok(Self {
            rows: self.rows,
            cols: self.cols,
            channels: 4,
            bytes,
        })
    }

    /// Returns the number of rows of pixels.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Returns the number of columns of pixels.
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Returns the number of components of each pixel: 3 for red, green and blue, or 4
    /// with alpha.
    #[must_use]
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Returns the bytes: the components of every pixel, [`channels`](Pixels::channels)
    /// bytes per pixel, in row-major pixel order.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Creates pixels with the given number of channels from a copy of the bytes,
    /// checking that their number matches the shape.
    fn from_bytes(rows: usize, cols: usize, channels: usize, bytes: &[u8]) -> Result<Self, Error> {
        let expected = rows
            .checked_mul(cols)
            .and_then(|pixels| pixels.checked_mul(channels));
        if expected == Some(bytes.len()) {
            Ok(Self {
                rows,
                cols,
                channels,
                bytes: bytes.to_vec(),
            })
        } else {
            Err(Error::Ir(IrError::InvalidShape {
                shape: vec![rows, cols, channels],
                len: bytes.len(),
            }))
        }
    }

    /// Creates pixels with the given number of channels from a function of the row and
    /// column, quantising the first `channels` components of each colour.
    fn from_fn(
        rows: usize,
        cols: usize,
        channels: usize,
        mut f: impl FnMut(usize, usize) -> Color,
    ) -> Self {
        let bytes = (0..rows)
            .flat_map(|row| (0..cols).map(move |col| (row, col)))
            .flat_map(|(row, col)| {
                let Color { r, g, b, a } = f(row, col);
                [r, g, b, a]
                    .into_iter()
                    .take(channels)
                    .map(|component| level(f64::from(component)))
            })
            .collect();
        Self {
            rows,
            cols,
            channels,
            bytes,
        }
    }

    /// Converts the pixels into an array of 8-bit values of shape
    /// `[rows, cols, channels]`, the form in which an image artist holds them.
    pub(crate) fn to_array(&self) -> NdArray {
        NdArray {
            shape: vec![self.rows, self.cols, self.channels],
            values: Values::U8(self.bytes.clone()),
        }
    }
}

/// Quantises a component to the nearest of 256 levels after clamping it to the range 0
/// to 1. A NaN component stays NaN through the clamp and casts to 0.
fn level(component: f64) -> u8 {
    (component.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Quantises a component of the pixel in a row and column as [`level`] does, refusing
/// one that is not finite.
fn checked_level(component: f64, row: usize, col: usize, channel: usize) -> Result<u8, Error> {
    if component.is_finite() {
        Ok(level(component))
    } else {
        Err(Error::NonFiniteComponent { row, col, channel })
    }
}

/// Checks that a plane has the rows and columns of the pixels it forms or joins.
fn check_shape(expected: [usize; 2], plane: &Matrix) -> Result<(), Error> {
    let found = [plane.rows(), plane.cols()];
    if found == expected {
        Ok(())
    } else {
        Err(Error::PlaneShapeMismatch { expected, found })
    }
}

/// A dense two-dimensional array of bytes stored in row-major order: the counterpart of
/// [`Matrix`] for 8-bit data, such as the classes of a colour-indexed image or the
/// samples of a greyscale image.
///
/// As in a matrix, the value in row `j` and column `i` belongs to the point
/// `(x[i], y[j])`, and values are indexed by `(row, col)`, both counted from zero. A
/// byte matrix given to [`indexed_image`](crate::AxesMut::indexed_image) or
/// [`mapped_image`](crate::AxesMut::mapped_image) is stored as bytes: exactly, and at a
/// quarter of the size of the same values as floating-point numbers.
///
/// ```
/// use ironlab::ByteMatrix;
///
/// let m = ByteMatrix::from_rows(&[[1, 2, 3], [4, 5, 6]]);
/// assert_eq!((m.rows(), m.cols()), (2, 3));
/// assert_eq!(m[(1, 0)], 4);
/// assert_eq!(m.values(), &[1, 2, 3, 4, 5, 6]);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct ByteMatrix {
    rows: usize,
    cols: usize,
    values: Vec<u8>,
}

impl ByteMatrix {
    /// Creates a matrix of zeros.
    #[must_use]
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            values: vec![0; rows * cols],
        }
    }

    /// Creates a matrix whose value in each row and column is computed by a function
    /// of the row and column indices.
    ///
    /// ```
    /// use ironlab::ByteMatrix;
    ///
    /// let classes = ByteMatrix::from_fn(4, 4, |row, col| ((row + col) % 3) as u8);
    /// assert_eq!(classes[(2, 2)], 1);
    /// ```
    #[must_use]
    pub fn from_fn(rows: usize, cols: usize, mut f: impl FnMut(usize, usize) -> u8) -> Self {
        let values = (0..rows)
            .flat_map(|row| (0..cols).map(move |col| (row, col)))
            .map(|(row, col)| f(row, col))
            .collect();
        Self { rows, cols, values }
    }

    /// Creates a matrix from a slice of rows of equal length.
    ///
    /// ```
    /// use ironlab::ByteMatrix;
    ///
    /// let m = ByteMatrix::from_rows(&[[1, 2], [3, 4], [5, 6]]);
    /// assert_eq!((m.rows(), m.cols()), (3, 2));
    /// ```
    ///
    /// # Panics
    ///
    /// Panics with a message containing `matrix rows have different lengths` when the
    /// rows do not all have the same length.
    #[must_use]
    pub fn from_rows<R: AsRef<[u8]>>(rows: &[R]) -> Self {
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
    pub fn from_vec(rows: usize, cols: usize, values: Vec<u8>) -> Result<Self, IrError> {
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
    pub fn values(&self) -> &[u8] {
        &self.values
    }

    /// Consumes the matrix and returns its values in row-major order.
    #[must_use]
    pub fn into_values(self) -> Vec<u8> {
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

impl Index<(usize, usize)> for ByteMatrix {
    type Output = u8;

    /// Returns the value at `(row, col)`.
    ///
    /// # Panics
    ///
    /// Panics with a message containing `out of range` when the row is not less than
    /// the number of rows or the column is not less than the number of columns. Each
    /// index is checked separately, so a column past the end of a row never reads a
    /// value of the next row.
    fn index(&self, (row, col): (usize, usize)) -> &u8 {
        &self.values[self.flat_index(row, col)]
    }
}

impl IndexMut<(usize, usize)> for ByteMatrix {
    /// Returns the value at `(row, col)`, mutably.
    ///
    /// # Panics
    ///
    /// Panics under the same conditions, and with the same message, as
    /// [`Index::index`].
    fn index_mut(&mut self, (row, col): (usize, usize)) -> &mut u8 {
        let index = self.flat_index(row, col);
        &mut self.values[index]
    }
}

/// The values of a colour-indexed or colour-mapped image: a matrix of floating-point
/// values or a matrix of bytes.
///
/// [`indexed_image`](crate::AxesMut::indexed_image) and
/// [`mapped_image`](crate::AxesMut::mapped_image) take anything that converts into this
/// type: a [`Matrix`] or a [`ByteMatrix`], by value or by reference. Floating-point
/// values are stored as such, so a NaN reaches the figure and is drawn as the image's
/// policy for non-finite values says; bytes are stored as bytes.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageValues {
    /// Floating-point values.
    Floats(Matrix),
    /// 8-bit values.
    Bytes(ByteMatrix),
}

impl ImageValues {
    /// Converts the values into a two-dimensional array of shape `[rows, cols]` with the
    /// element type of the container.
    pub(crate) fn into_array(self) -> NdArray {
        match self {
            ImageValues::Floats(matrix) => NdArray {
                shape: vec![matrix.rows(), matrix.cols()],
                values: Values::F64(matrix.into_values()),
            },
            ImageValues::Bytes(matrix) => NdArray {
                shape: vec![matrix.rows(), matrix.cols()],
                values: Values::U8(matrix.into_values()),
            },
        }
    }
}

impl From<Matrix> for ImageValues {
    fn from(matrix: Matrix) -> Self {
        ImageValues::Floats(matrix)
    }
}

impl From<&Matrix> for ImageValues {
    fn from(matrix: &Matrix) -> Self {
        ImageValues::Floats(matrix.clone())
    }
}

impl From<ByteMatrix> for ImageValues {
    fn from(matrix: ByteMatrix) -> Self {
        ImageValues::Bytes(matrix)
    }
}

impl From<&ByteMatrix> for ImageValues {
    fn from(matrix: &ByteMatrix) -> Self {
        ImageValues::Bytes(matrix.clone())
    }
}
