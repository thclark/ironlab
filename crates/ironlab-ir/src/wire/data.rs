//! Wire types of `ironlab/ir/v0/data.proto`.

proto_file! {
    /// A numeric array of the figure, which artists refer to by its node identifier.
    message DataArray {
        /// The node identifier of the array, unique within the figure; it must be
        /// present.
        optional uint64 id = 1;
        /// The values of the array; it must be present.
        message NdArray array = 2;
        /// The name that identifies the array to tools; absent when it has none.
        optional string name = 3;
    }

    /// The element type of an array, which names the field of the array that holds
    /// its values.
    enum NdArrayElement {
        /// 64-bit floating-point values, held in `values`.
        F64 = 1;
        /// 8-bit unsigned integers, held in `u8_values`.
        U8 = 2;
    }

    /// A dense, row-major, n-dimensional array of numeric values.
    ///
    /// A two-dimensional array with `ny` rows and `nx` columns has shape `[ny, nx]`,
    /// and the value in row `j` and column `i` is at index `j * nx + i` of the field
    /// that holds the values. The `element` names that field: `values` for 64-bit
    /// floating-point values, in which a missing value is NaN, or `u8_values` for
    /// 8-bit unsigned integers. The other field is empty. An unspecified element
    /// means floating-point values, which is what every file written before the
    /// element existed holds.
    message NdArray {
        /// The length of each dimension, outermost first.
        repeated uint64 shape = 1;
        /// The values in row-major order, as IEEE 754 doubles, when the element is
        /// `ND_ARRAY_ELEMENT_F64` or unspecified; otherwise empty.
        repeated double values = 2;
        /// The element type of the values, which names the field that holds them.
        enum NdArrayElement element = 3;
        /// The values in row-major order, one byte per value, when the element is
        /// `ND_ARRAY_ELEMENT_U8`; otherwise empty.
        bytes u8_values = 4;
    }
}
