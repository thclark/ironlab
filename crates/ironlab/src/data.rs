//! Data arrays: the values that plotting calls take, and the handle to an array of the
//! figure.

use ironlab_ir::{Grid, NdArray, NodeId, Values};

use crate::matrix::Matrix;
use crate::pixels::{ByteMatrix, ImageValues, Pixels};

/// The data given to a plotting call or to a setter that gives an artist an array: either
/// values, which are copied into a new data array of the figure, or the identifier of a
/// data array the figure already holds, which the artist then shares.
///
/// Every data argument of the builder takes `impl Into<DataSource>`, so values and
/// identifiers can be mixed argument by argument. Values come from a slice, an array or a
/// vector of `f64` (a vector of values), a [`Matrix`] (a two-dimensional array), a
/// [`ByteMatrix`] or [`ImageValues`] (the values of an image) or [`Pixels`] (the pixels of
/// a true-colour image), by value or by reference. Each copy is a new data array without a
/// name, so the caller's values may be dropped or changed afterwards.
///
/// A [`NodeId`] is stored as the reference itself and nothing is copied, so every artist
/// given the same identifier draws the same array, and an edit of that array (in the
/// viewer, or through [`Edit::ReplaceData`](crate::ir::Edit::ReplaceData)) changes all of
/// them. The identifier is the one returned by [`Figure::data`](crate::Figure::data) or by
/// an accessor of an artist handle, such as [`SeriesMut::x_data`](crate::SeriesMut::x_data).
/// An identifier that names no data array is stored all the same, without a panic or an
/// error from the call, and [`Figure::validate`](crate::Figure::validate) reports it: as
/// `WrongNodeKind` when it names another kind of node, and as `UnknownData` when it names
/// no node.
///
/// ```
/// use ironlab::prelude::*;
///
/// let t = linspace(0.0, 1.0, 101);
/// let a: Vec<f64> = t.iter().map(|t| t.sin()).collect();
/// let b: Vec<f64> = t.iter().map(|t| t.cos()).collect();
///
/// let mut fig = Figure::new().tiles(2, 1);
/// // The time base is stored once and shared by both lines.
/// let time = fig.data("time (s)", &t).id();
/// fig.axes(0, 0).plot(time, &a);
/// fig.axes(1, 0).plot(time, &b);
/// assert_eq!(fig.ir().data.len(), 3);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum DataSource {
    /// Values, copied into a new data array without a name.
    Values(NdArray),
    /// The identifier of a data array of the figure, shared rather than copied.
    Node(NodeId),
}

impl From<NodeId> for DataSource {
    fn from(id: NodeId) -> Self {
        DataSource::Node(id)
    }
}

impl From<NdArray> for DataSource {
    fn from(array: NdArray) -> Self {
        DataSource::Values(array)
    }
}

impl From<Vec<f64>> for DataSource {
    fn from(values: Vec<f64>) -> Self {
        DataSource::Values(NdArray::vector(values))
    }
}

impl From<&Vec<f64>> for DataSource {
    fn from(values: &Vec<f64>) -> Self {
        DataSource::Values(NdArray::vector(values.clone()))
    }
}

impl From<&[f64]> for DataSource {
    fn from(values: &[f64]) -> Self {
        DataSource::Values(NdArray::vector(values.to_vec()))
    }
}

impl From<&&[f64]> for DataSource {
    fn from(values: &&[f64]) -> Self {
        DataSource::Values(NdArray::vector(values.to_vec()))
    }
}

impl<const N: usize> From<[f64; N]> for DataSource {
    fn from(values: [f64; N]) -> Self {
        DataSource::Values(NdArray::vector(values.to_vec()))
    }
}

impl<const N: usize> From<&[f64; N]> for DataSource {
    fn from(values: &[f64; N]) -> Self {
        DataSource::Values(NdArray::vector(values.to_vec()))
    }
}

impl From<Matrix> for DataSource {
    fn from(matrix: Matrix) -> Self {
        DataSource::Values(ImageValues::Floats(matrix).into_array())
    }
}

impl From<&Matrix> for DataSource {
    fn from(matrix: &Matrix) -> Self {
        DataSource::Values(matrix_array(matrix))
    }
}

impl From<ByteMatrix> for DataSource {
    fn from(matrix: ByteMatrix) -> Self {
        DataSource::Values(ImageValues::Bytes(matrix).into_array())
    }
}

impl From<&ByteMatrix> for DataSource {
    fn from(matrix: &ByteMatrix) -> Self {
        DataSource::Values(ImageValues::Bytes(matrix.clone()).into_array())
    }
}

impl From<ImageValues> for DataSource {
    fn from(values: ImageValues) -> Self {
        DataSource::Values(values.into_array())
    }
}

impl From<&ImageValues> for DataSource {
    fn from(values: &ImageValues) -> Self {
        DataSource::Values(values.clone().into_array())
    }
}

impl From<Pixels> for DataSource {
    fn from(pixels: Pixels) -> Self {
        DataSource::Values(pixels.to_array())
    }
}

impl From<&Pixels> for DataSource {
    fn from(pixels: &Pixels) -> Self {
        DataSource::Values(pixels.to_array())
    }
}

impl DataSource {
    /// Returns the identifier of the data array that holds the data: the identifier
    /// given, or that of a new, unnamed array of the figure holding a copy of the values.
    pub(crate) fn store(self, fig: &mut ironlab_ir::Figure) -> NodeId {
        match self {
            DataSource::Values(array) => fig.add_data(array),
            DataSource::Node(id) => id,
        }
    }

    /// Returns a copy of the values of the data as floating-point numbers, in row-major
    /// order: the values given, or those of the data array the identifier names, with
    /// 8-bit values widened to the numbers they denote. An identifier that names no data
    /// array gives no values.
    pub(crate) fn read(&self, fig: &ironlab_ir::Figure) -> Vec<f64> {
        let array = match self {
            DataSource::Values(array) => Some(array),
            DataSource::Node(id) => fig.data_array(*id).map(|data| &data.array),
        };
        array.map_or_else(Vec::new, |array| match &array.values {
            Values::F64(values) => values.clone(),
            Values::U8(values) => values.iter().map(|&v| f64::from(v)).collect(),
        })
    }

    /// Returns the shape of the data: that of the values, or that of the data array the
    /// identifier names, or `None` when it names no data array.
    fn shape<'a>(&'a self, fig: &'a ironlab_ir::Figure) -> Option<&'a [usize]> {
        match self {
            DataSource::Values(array) => Some(&array.shape),
            DataSource::Node(id) => fig.data_array(*id).map(|data| data.array.shape.as_slice()),
        }
    }
}

/// A handle to one data array of a figure, returned by [`Figure::data`](crate::Figure::data).
///
/// Like [`AxesMut`](crate::AxesMut), the handle borrows the figure mutably, so it is
/// short-lived: keep its [`id`](DataArrayMut::id) to give the array to plotting calls.
#[derive(Debug)]
pub struct DataArrayMut<'f> {
    pub(crate) fig: &'f mut ironlab_ir::Figure,
    pub(crate) id: NodeId,
}

impl<'f> DataArrayMut<'f> {
    /// Creates a handle to the node with the given identifier, which is normally a data
    /// array of the figure.
    pub(crate) fn new(fig: &'f mut ironlab_ir::Figure, id: NodeId) -> Self {
        Self { fig, id }
    }
}

impl DataArrayMut<'_> {
    /// Returns the node identifier of the array, which plotting calls and setters accept
    /// in place of values to share the array.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Renames the array.
    ///
    /// The name identifies the array to tools such as the inspector of the viewer; it is
    /// never drawn and constrains nothing, so two arrays may share a name. A handle to an
    /// identifier that names no data array changes nothing.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// let t = fig.data("t", &linspace(0.0, 1.0, 11)).name("time (s)").id();
    /// assert_eq!(fig.ir().data_array(t).unwrap().name.as_deref(), Some("time (s)"));
    /// ```
    pub fn name(&mut self, name: impl Into<String>) -> &mut Self {
        if let Some(data) = self.fig.data_array_mut(self.id) {
            data.name = Some(name.into());
        }
        self
    }
}

/// Converts a matrix into a two-dimensional array of shape `[rows, cols]`.
pub(crate) fn matrix_array(matrix: &Matrix) -> NdArray {
    NdArray {
        shape: vec![matrix.rows(), matrix.cols()],
        values: Values::F64(matrix.values().to_vec()),
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

/// Stores the coordinates of a grid for a field and returns the grid that refers to them,
/// together with the identifier of the field.
///
/// The coordinates make a rectilinear grid unless either of them is two-dimensional, in
/// which case they make a curvilinear grid. In a curvilinear grid, a vector of values
/// whose length matches the field along its dimension is repeated into a matrix of the
/// field's shape, as [`meshgrid`](crate::meshgrid) would; any other vector of values, and
/// every identifier, is stored as given, so that validation reports a mismatch.
pub(crate) fn store_grid(
    fig: &mut ironlab_ir::Figure,
    x: DataSource,
    y: DataSource,
    z: DataSource,
) -> (Grid, NodeId) {
    let is_matrix = |source: &DataSource, fig: &ironlab_ir::Figure| {
        source.shape(fig).is_some_and(|shape| shape.len() >= 2)
    };
    let curvilinear = is_matrix(&x, fig) || is_matrix(&y, fig);
    let field = z.shape(fig).and_then(|shape| match *shape {
        [rows, cols] => Some((rows, cols)),
        _ => None,
    });
    let grid = if curvilinear {
        Grid::Curvilinear {
            x: node_coordinates(x, field, Along::Cols).store(fig),
            y: node_coordinates(y, field, Along::Rows).store(fig),
        }
    } else {
        Grid::Rectilinear {
            x: x.store(fig),
            y: y.store(fig),
        }
    };
    (grid, z.store(fig))
}

/// Repeats a vector of values whose length matches the field along a dimension into a
/// matrix of the field's shape, leaving every other source as it is.
fn node_coordinates(source: DataSource, field: Option<(usize, usize)>, along: Along) -> DataSource {
    let (DataSource::Values(array), Some((rows, cols))) = (&source, field) else {
        return source;
    };
    let (Some(v), [len]) = (array.as_f64(), array.shape.as_slice()) else {
        return source;
    };
    match along {
        Along::Cols if *len == cols => {
            DataSource::from(Matrix::from_fn(rows, cols, |_, col| v[col]))
        }
        Along::Rows if *len == rows => {
            DataSource::from(Matrix::from_fn(rows, cols, |row, _| v[row]))
        }
        _ => source,
    }
}
