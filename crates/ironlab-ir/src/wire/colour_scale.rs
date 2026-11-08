//! Wire types of `ironlab/ir/v0/colour_scale.proto`.

proto_file! {
    /// A colour scale: a node of the figure that turns numbers into colours, which
    /// colour-mapped artists name by its identifier.
    message ColourScale {
        /// The node identifier of the scale, unique within the figure; it must be present.
        optional uint64 id = 1;
        /// The title drawn with the scale; absent when there is no title.
        message Text title = 2;
        /// How values become colours and how the scale is drawn; absent means a
        /// continuous scale with the defaults of its kind.
        message ColourScaleKind kind = 3;
        /// What is drawn for a value that is NaN or infinite; absent means transparent,
        /// without a key.
        message ColourScaleOutOfRange non_finite = 4;
    }

    /// One category of values that a colour scale cannot place within its range.
    message ColourScaleOutOfRange {
        /// What is drawn for a value of the category; absent means the default of the
        /// category: clamped below and above, strict outside the entries, and transparent
        /// for non-finite values.
        message OutOfRange policy = 1;
        /// Whether a colourbar that shows the scale draws a key square for the category.
        optional bool show_key = 2;
        /// The label of the key square; absent for the default label.
        message Text label = 3;
    }

    /// What is drawn for a value that a colour scale cannot place within its range.
    message OutOfRange {
        /// The policy; unset means the default of the context.
        oneof kind: OutOfRangeKind {
            /// Such a value makes the figure invalid.
            Strict(OutOfRangeStrict) strict = 1;
            /// Nothing is drawn for the value.
            Transparent(OutOfRangeTransparent) transparent = 2;
            /// The value takes the nearest end colour of the scale.
            Clamp(OutOfRangeClamp) clamp = 3;
            /// The value is drawn in a fixed colour.
            Rgba(OutOfRangeRgba) rgba = 4;
        }
    }

    /// Such a value makes the figure invalid, so it is refused until the data is
    /// corrected.
    message OutOfRangeStrict {}

    /// Nothing is drawn for the value.
    message OutOfRangeTransparent {}

    /// The value takes the nearest end colour of the scale: its first colour below the
    /// range and its last colour above it, and, outside the entries of an indexed or
    /// categorical scale, the entry nearest to the index truncated towards zero; a
    /// non-finite value has no nearest end, so nothing is drawn for it.
    message OutOfRangeClamp {}

    /// The value is drawn in a fixed colour.
    message OutOfRangeRgba {
        /// The colour of the value; absent means black.
        message Colour colour = 1;
    }

    /// How a colour scale turns values into colours, which also fixes how it is drawn.
    message ColourScaleKind {
        /// The kind of scale; unset means a continuous scale with the defaults of its
        /// kind.
        oneof kind: ColourScaleKindKind {
            /// A colourmap interpolated between limits.
            Continuous(ColourScaleKindContinuous) continuous = 1;
            /// Bands between edges, each in one colour of a colourmap.
            Discrete(ColourScaleKindDiscrete) discrete = 2;
            /// A palette of which a whole number `i` takes entry `i`.
            Indexed(ColourScaleKindIndexed) indexed = 3;
            /// Labelled categories of which a whole number `i` takes category `i`.
            Categorical(ColourScaleKindCategorical) categorical = 4;
        }
    }

    /// A continuous colour scale.
    message ColourScaleKindContinuous {
        /// The scale; absent means the defaults of a continuous scale.
        message ContinuousColourScale continuous = 1;
    }

    /// A discrete colour scale.
    message ColourScaleKindDiscrete {
        /// The scale; absent means the defaults of a discrete scale.
        message DiscreteColourScale discrete = 1;
    }

    /// An indexed colour scale.
    message ColourScaleKindIndexed {
        /// The scale; absent means the defaults of an indexed scale.
        message IndexedColourScale indexed = 1;
    }

    /// A categorical colour scale.
    message ColourScaleKindCategorical {
        /// The scale; absent means the defaults of a categorical scale.
        message CategoricalColourScale categorical = 1;
    }

    /// A continuous colour scale: a colourmap interpolated between two limits.
    message ContinuousColourScale {
        /// The palette that the scale interpolates; absent means viridis.
        message Colourmap colourmap = 1;
        /// The values mapped to the first and last colours of the colourmap; absent means
        /// automatic.
        message Limits limits = 2;
        /// Whether values are interpolated in linear or logarithmic space.
        enum ColourScaleNorm norm = 3;
        /// What is drawn for a value below the lower limit.
        message ColourScaleOutOfRange below = 4;
        /// What is drawn for a value above the upper limit.
        message ColourScaleOutOfRange above = 5;
        /// Where the ticks of a colourbar that shows the scale are placed.
        message Ticks ticks = 6;
        /// What is written at the ticks of a colourbar that shows the scale.
        message TickLabels tick_labels = 7;
    }

    /// A discrete colour scale: bands between edges, each in one colour of a colourmap.
    message DiscreteColourScale {
        /// The palette from which the bands take their colours; absent means viridis.
        message Colourmap colourmap = 1;
        /// The edges of the bands; absent means ten automatic bands between automatic
        /// limits.
        message ColourScaleLevels levels = 2;
        /// Whether band edges are chosen, and the colourmap sampled, in linear or
        /// logarithmic space.
        enum ColourScaleNorm norm = 3;
        /// What is drawn for a value below the first edge.
        message ColourScaleOutOfRange below = 4;
        /// What is drawn for a value above the last edge.
        message ColourScaleOutOfRange above = 5;
        /// Where the ticks of a colourbar that shows the scale are placed.
        message Ticks ticks = 6;
        /// What is written at the ticks of a colourbar that shows the scale.
        message TickLabels tick_labels = 7;
    }

    /// The edges of the bands of a discrete colour scale.
    message ColourScaleLevels {
        /// The kind of edges; unset means the default of the context.
        oneof kind: ColourScaleLevelsKind {
            /// Edges at nice values from the lower limit to the upper limit.
            Auto(ColourScaleLevelsAuto) automatic = 1;
            /// Edges given explicitly.
            Explicit(ColourScaleLevelsExplicit) explicit_values = 2;
        }
    }

    /// Edges at nice values from the lower limit to the upper limit, which are the first
    /// and last edges.
    message ColourScaleLevelsAuto {
        /// The approximate number of bands; absent means ten.
        optional uint32 count = 1;
        /// The range that the edges span; absent means automatic.
        message Limits limits = 2;
    }

    /// Edges given explicitly.
    message ColourScaleLevelsExplicit {
        /// The edges, strictly increasing.
        repeated double values = 1;
    }

    /// An indexed colour scale: a palette of which a whole number `i` takes entry `i`.
    message IndexedColourScale {
        /// The entries of the palette.
        repeated message Colour colours = 1;
        /// What is drawn for a value that names no entry.
        message ColourScaleOutOfRange outside = 2;
        /// Where the ticks of a colourbar that shows the scale are placed.
        message Ticks ticks = 3;
        /// What is written at the ticks of a colourbar that shows the scale.
        message TickLabels tick_labels = 4;
    }

    /// A categorical colour scale: labelled categories of which a whole number `i` takes
    /// category `i`.
    message CategoricalColourScale {
        /// The categories.
        repeated message ColourCategory categories = 1;
        /// What is drawn for a value that names no category.
        message ColourScaleOutOfRange outside = 2;
    }

    /// One category of a categorical colour scale.
    message ColourCategory {
        /// The colour of the category; absent means black.
        message Colour colour = 1;
        /// The label drawn beside the colour; it must be present.
        message Text label = 2;
    }

    /// The palette that a continuous or discrete colour scale interpolates.
    message Colourmap {
        /// The kind of colourmap; unset means viridis.
        oneof kind: ColourmapKind {
            /// A bundled colourmap.
            Named(ColourmapNamed) named = 1;
            /// A list of colours.
            Listed(ColourmapListed) listed = 2;
        }
    }

    /// A bundled colourmap.
    message ColourmapNamed {
        /// The name of the colourmap; unspecified means viridis.
        enum ColourmapName name = 1;
        /// Whether the colourmap runs from its last colour to its first.
        optional bool reverse = 2;
    }

    /// A list of colours, interpolated from the first to the last.
    message ColourmapListed {
        /// The colours: from 2 to 1024 in a valid figure.
        repeated message Colour colours = 1;
    }

    /// The space in which a colour scale places values between its limits.
    enum ColourScaleNorm {
        /// Positions are proportional to values.
        Linear = 1;
        /// Positions are proportional to the base-10 logarithm of values.
        Log = 2;
    }
}
