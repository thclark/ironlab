//! Conversion of colour arguments into colour specifications.

use ironlab_ir::{Color, ColorSpec};

/// A value that can be passed wherever a colour is set.
///
/// A [`Color`] sets a fixed colour, `None` (an `Option<Color>`) draws nothing, like
/// MATLAB's `'none'`, and a [`ColorSpec`] is used as given, which allows the
/// automatic colour ([`ColorSpec::Auto`]) or a colour taken from the colormap
/// ([`ColorSpec::Colormapped`]) to be chosen.
///
/// ```
/// use ironlab::prelude::*;
///
/// let x = [0.0, 1.0, 2.0];
/// let z = Matrix::from_fn(3, 3, |row, col| (row + col) as f64);
/// let mut fig = Figure::new();
/// let mut ax = fig.axes(0, 0);
/// ax.plot(x, x).color(Color::rgb(0.8, 0.1, 0.1));
/// ax.surf(x, x, &z).edge_color(None).face_color(ColorSpec::Colormapped);
/// ```
pub trait IntoColorSpec {
    /// Converts the value into a colour specification.
    fn into_color_spec(self) -> ColorSpec;
}

impl IntoColorSpec for ColorSpec {
    fn into_color_spec(self) -> ColorSpec {
        self
    }
}

impl IntoColorSpec for Color {
    fn into_color_spec(self) -> ColorSpec {
        ColorSpec::Rgba { color: self }
    }
}

impl IntoColorSpec for Option<Color> {
    fn into_color_spec(self) -> ColorSpec {
        self.map_or(ColorSpec::None, Color::into_color_spec)
    }
}
