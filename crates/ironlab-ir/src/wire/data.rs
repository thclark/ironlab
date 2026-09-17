//! Wire types of `ironlab/ir/v0/data.proto`.

proto_file! {
    /// A dense, row-major, n-dimensional array of 64-bit floating-point values.
    ///
    /// A two-dimensional array with `ny` rows and `nx` columns has shape `[ny, nx]`,
    /// and the value in row `j` and column `i` is `values[j * nx + i]`. A missing
    /// value is NaN.
    message NdArray {
        /// The length of each dimension, outermost first.
        repeated uint64 shape = 1;
        /// The values in row-major order, as IEEE 754 doubles.
        repeated double values = 2;
    }
}
