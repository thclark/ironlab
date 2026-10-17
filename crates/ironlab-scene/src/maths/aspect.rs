//! The geometry of a fixed aspect: the ratio of the drawn length of one data unit of y to that of one data unit of x.
//!
//! Every function works in scale space, where a value on a linear axis is itself and a value on a logarithmic axis
//! is its base-10 logarithm, so that the unit of a logarithmic axis is one decade. The scene compiler keeps the
//! aspect of a 2D axes with these functions, and the viewer uses [`widen`] to navigate from the limits the compiler
//! draws rather than from narrower manual limits it stores.

use crate::display::Rect;

/// Returns the drawn length of one unit of y over that of one unit of x, for a plot `width` by `height` points that
/// shows the scale-space spans `x_span` and `y_span`.
#[must_use]
pub fn drawn_ratio(width: f64, height: f64, x_span: f64, y_span: f64) -> f64 {
    (height / y_span) / (width / x_span)
}

/// Widens one of two scale-space intervals symmetrically about its centre, so that a plot `width` by `height` points
/// draws one unit of y `ratio` times as long as one unit of x, and returns both intervals.
///
/// Only the interval that is too short for the plot grows; the other is returned unchanged, so that intervals that
/// already have the ratio are a fixed point. Arguments that are not finite and positive where they must be return
/// the intervals unchanged.
#[must_use]
pub fn widen(
    x: (f64, f64),
    y: (f64, f64),
    width: f64,
    height: f64,
    ratio: f64,
) -> ((f64, f64), (f64, f64)) {
    let (x_span, y_span) = (x.1 - x.0, y.1 - y.0);
    let usable = |v: f64| v.is_finite() && v > 0.0;
    if !(usable(x_span) && usable(y_span) && usable(width) && usable(height) && usable(ratio)) {
        return (x, y);
    }
    // The y span the plot needs for the x span it shows, from height / y_span = ratio · width / x_span.
    let y_needed = height * x_span / (ratio * width);
    if y_needed > y_span {
        (x, about_centre(y, y_needed))
    } else {
        let x_needed = ratio * width * y_span / height;
        if x_needed > x_span {
            (about_centre(x, x_needed), y)
        } else {
            (x, y)
        }
    }
}

/// Returns the interval of the given span centred where `interval` is centred.
#[must_use]
pub fn about_centre(interval: (f64, f64), span: f64) -> (f64, f64) {
    let centre = (interval.0 + interval.1) / 2.0;
    (centre - span / 2.0, centre + span / 2.0)
}

/// Returns the largest rectangle centred in `space` whose width and height draw the scale-space spans `x_span` and
/// `y_span` with one unit of y `ratio` times as long as one unit of x.
///
/// Only one side of `space` shrinks: the width when the space is too wide for the ratio, the height when it is too
/// tall. Arguments that are not finite and positive where they must be return `space` unchanged.
#[must_use]
pub fn fit_box(space: Rect, x_span: f64, y_span: f64, ratio: f64) -> Rect {
    let usable = |v: f64| v.is_finite() && v > 0.0;
    if !(usable(x_span)
        && usable(y_span)
        && usable(ratio)
        && usable(space.width)
        && usable(space.height))
    {
        return space;
    }
    // The height the full width needs, from height / y_span = ratio · width / x_span.
    let height = ratio * space.width * y_span / x_span;
    let (width, height) = if height <= space.height {
        (space.width, height)
    } else {
        (space.height * x_span / (ratio * y_span), space.height)
    };
    Rect::new(
        space.x + (space.width - width) / 2.0,
        space.y + (space.height - height) / 2.0,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Why: the widened intervals must give exactly the ratio asked for, and a second widening must change nothing,
    // because the viewer and the compiler both widen limits that may already have been widened.
    #[test]
    fn widening_gives_the_ratio_and_is_idempotent() {
        let (x, y) = widen((0.0, 2.0), (0.0, 0.5), 300.0, 200.0, 1.0);
        assert_eq!(x, (0.0, 2.0));
        let ratio = drawn_ratio(300.0, 200.0, x.1 - x.0, y.1 - y.0);
        assert!((ratio - 1.0).abs() < 1e-12, "{ratio}");
        assert_eq!(widen(x, y, 300.0, 200.0, 1.0), (x, y));
        assert!(
            ((y.0 + y.1) / 2.0 - 0.25).abs() < 1e-12,
            "y widens about its centre"
        );
    }

    // Why: the box keeps the full length of the side that already fits and is centred in its space.
    #[test]
    fn the_fitted_box_shrinks_one_side_and_is_centred() {
        let space = Rect::new(10.0, 20.0, 300.0, 100.0);
        let fitted = fit_box(space, 1.0, 1.0, 1.0);
        assert_eq!(fitted.height, 100.0);
        assert_eq!(fitted.width, 100.0);
        assert_eq!(fitted.x, 110.0);
    }
}
