//! Conversion of colour arguments into colour specifications.

use ironlab_ir::{Colour, ColourSpec};

/// A value that can be passed wherever a colour is set.
///
/// A [`Colour`] sets a fixed colour, `None` (an `Option<Colour>`) draws nothing, like
/// MATLAB's `'none'`, and a [`ColourSpec`] is used as given, which allows the
/// automatic colour ([`ColourSpec::Auto`]) or a colour taken from the colourmap
/// ([`ColourSpec::Colourmapped`]) to be chosen.
///
/// ```
/// use ironlab::prelude::*;
///
/// let x = [0.0, 1.0, 2.0];
/// let z = Matrix::from_fn(3, 3, |row, col| (row + col) as f64);
/// let mut fig = Figure::new();
/// let mut ax = fig.axes(0, 0);
/// ax.plot(x, x).colour(Colour::rgb(0.8, 0.1, 0.1));
/// ax.surf(x, x, &z).edge_colour(None).face_colour(ColourSpec::Colourmapped);
/// ```
pub trait IntoColourSpec {
    /// Converts the value into a colour specification.
    fn into_colour_spec(self) -> ColourSpec;
}

impl IntoColourSpec for ColourSpec {
    fn into_colour_spec(self) -> ColourSpec {
        self
    }
}

impl IntoColourSpec for Colour {
    fn into_colour_spec(self) -> ColourSpec {
        ColourSpec::Rgba { colour: self }
    }
}

impl IntoColourSpec for Option<Colour> {
    fn into_colour_spec(self) -> ColourSpec {
        self.map_or(ColourSpec::None, Colour::into_colour_spec)
    }
}
