//! Rectangles on the page shared by bars and cells: the inset that a gap measured on the page makes, the clamping of
//! a rectangle that reaches far beyond the plot area, and the part of a drawn rectangle that a reader can point at.

use crate::display::{Rect, Rgba};

use super::style;

/// How far beyond the clip rectangle, in points, a bar, a cell or a step may be drawn before its coordinates are
/// clamped, so that a rectangle reaching far beyond the limits is drawn at a reasonable size whose clamped edges lie
/// outside the clip and are never seen.
pub(super) const CLAMP_MARGIN: f64 = 16.0;

/// Returns a gap as a fraction from 0 to 1, or 0 for a gap that is not finite, which validation refuses.
pub(super) fn unit_gap(gap: f64) -> f64 {
    if gap.is_finite() {
        gap.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Returns where the drawn part of the span from `low` to `high` on the page starts and how long it is, signed as the
/// span is, when the fraction `gap` of the span is left empty, half on each side. The inset is measured on the page,
/// so that it is symmetric along a logarithmic axis, and it moves both ends inwards whichever way the span runs.
pub(super) fn inset(low: f64, high: f64, gap: f64) -> (f64, f64) {
    let span = high - low;
    (low + span * gap / 2.0, span * (1.0 - gap))
}

/// Returns `colour` with its alpha multiplied by `opacity`, which is taken as a number from 0 to 1.
pub(super) fn faded(colour: Option<Rgba>, opacity: f64) -> Option<Rgba> {
    colour.map(|c| c.with_alpha_factor(style::unit_opacity(opacity) as f32))
}

/// Returns `rect` grown by `margin` on every side.
pub(super) fn expand(rect: Rect, margin: f64) -> Rect {
    Rect::new(
        rect.x - margin,
        rect.y - margin,
        rect.width + 2.0 * margin,
        rect.height + 2.0 * margin,
    )
}

/// Returns the rectangle that two rectangles share, or `None` when they share none.
pub(super) fn intersection(a: Rect, b: Rect) -> Option<Rect> {
    let (left, right) = (a.x.max(b.x), a.right().min(b.right()));
    let (top, bottom) = (a.y.max(b.y), a.bottom().min(b.bottom()));
    (left <= right && top <= bottom).then(|| Rect::new(left, top, right - left, bottom - top))
}

/// Returns the span from `a` to `b`, in increasing order, clamped to `[low, high]`.
pub(super) fn clamp_span(a: f64, b: f64, low: f64, high: f64) -> (f64, f64) {
    (a.min(b).clamp(low, high), a.max(b).clamp(low, high))
}

/// Returns the rectangle with the horizontal span from `x0` to `x1` and the vertical span from `y0` to `y1`, in
/// either order, clamped to `bounds`.
pub(super) fn clamped_rect(x0: f64, x1: f64, y0: f64, y1: f64, bounds: Rect) -> Rect {
    let (left, right) = clamp_span(x0, x1, bounds.x, bounds.right());
    let (top, bottom) = clamp_span(y0, y1, bounds.y, bounds.bottom());
    Rect::new(left, top, right - left, bottom - top)
}
