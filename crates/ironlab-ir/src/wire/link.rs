//! Wire types of `ironlab/ir/v0/link.proto`.

proto_file! {
    /// A group of axes whose limits along one dimension are kept equal.
    message AxisLink {
        /// The dimension whose limits are linked; it must be specified.
        enum Dimension dimension = 1;
        /// The node identifiers of the linked axes.
        repeated uint64 axes = 2;
    }

    /// A coordinate dimension of an axes.
    enum Dimension {
        /// The x axis.
        X = 1;
        /// The y axis.
        Y = 2;
        /// The z axis.
        Z = 3;
    }
}
