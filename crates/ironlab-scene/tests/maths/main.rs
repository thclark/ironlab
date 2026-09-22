//! Intent-capturing tests for the pure plotting mathematics in `ironlab_scene::maths`.

mod camera;
mod colormap;
mod contour;
mod decimate;
mod polygon;
mod quiver;
mod ticks;

/// Asserts that two floating-point values agree to within an absolute tolerance.
#[track_caller]
pub fn assert_close(actual: f64, expected: f64, tol: f64) {
    assert!(
        (actual - expected).abs() <= tol,
        "expected {expected} ± {tol}, got {actual}"
    );
}

/// Returns whether `step` is 1, 2 or 5 times an integer power of ten.
pub fn is_nice_step(step: f64) -> bool {
    if !(step.is_finite() && step > 0.0) {
        return false;
    }
    let exponent = step.log10().floor();
    let mantissa = step / 10f64.powf(exponent);
    [1.0, 2.0, 5.0, 10.0]
        .iter()
        .any(|m| (mantissa - m).abs() < 1e-9)
}
