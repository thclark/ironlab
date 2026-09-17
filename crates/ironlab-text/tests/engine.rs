//! Engine-level tests: memoisation, scaling and thread safety.

mod common;

use std::sync::Arc;

use common::EPS;
use ironlab_text::{TextEngine, TextLayout};

// The scene is recompiled on every pan and zoom; identical labels must be served from the memo rather than re-shaped and re-typeset.
#[test]
fn identical_requests_are_memoised() {
    let engine = TextEngine::new();
    let a = engine.layout(r"Pressure $\frac{p}{p_0}$", true, 9.0);
    let b = engine.layout(r"Pressure $\frac{p}{p_0}$", true, 9.0);
    assert!(Arc::ptr_eq(&a, &b));
}

// The memo key must include every argument, or a label requested at one size or interpretation would be returned for another.
#[test]
fn memo_distinguishes_size_and_interpretation() {
    let engine = TextEngine::new();
    let base = engine.layout("$x$", true, 9.0);
    let other_size = engine.layout("$x$", true, 10.0);
    let verbatim = engine.layout("$x$", false, 9.0);
    assert!(!Arc::ptr_eq(&base, &other_size));
    assert!(!Arc::ptr_eq(&base, &verbatim));
    assert_ne!(*base, *verbatim);
    assert!(other_size.width > base.width);
}

// Titles are drawn at a multiple of the base font size; all geometry must scale linearly with size so that layout computed at one size is consistent with another.
#[test]
fn layout_scales_linearly_with_size() {
    let engine = TextEngine::new();
    let source = r"Pressure $p^2/\alpha$ (Pa)";
    let small = engine.layout(source, true, 9.0);
    let large = engine.layout(source, true, 18.0);
    assert!(small.width > 0.0);
    let ratio = large.width / small.width;
    assert!((ratio - 2.0).abs() < 0.02, "width ratio {ratio}");
    assert!((large.height - 2.0 * small.height).abs() < 0.02 * large.height + EPS);
    assert!((large.depth - 2.0 * small.depth).abs() < 0.02 * large.depth + EPS);
}

// The engine is shared between the viewer's UI thread and background export, so it and its outputs must be Send and Sync.
#[test]
fn engine_and_layout_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TextEngine>();
    assert_send_sync::<TextLayout>();
}

// Concurrent callers must see consistent results and never deadlock or poison the memo.
#[test]
fn concurrent_layouts_agree() {
    let engine = Arc::new(TextEngine::default());
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let engine = Arc::clone(&engine);
            std::thread::spawn(move || engine.layout(r"$\frac{a}{b}$ and $x^2$", true, 9.0))
        })
        .collect();
    let results: Vec<Arc<TextLayout>> = handles
        .into_iter()
        .map(|h| h.join().expect("thread did not panic"))
        .collect();
    assert!(results.windows(2).all(|w| *w[0] == *w[1]));
}
