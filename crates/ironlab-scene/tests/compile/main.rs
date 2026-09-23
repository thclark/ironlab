//! Intent-capturing tests for `ironlab_scene::compile`, which turns a figure IR into a display list,
//! a hit map and warnings.
//!
//! Figures are assembled directly from `ironlab_ir` types by the fixture builders in [`common`], and
//! compiled scenes are inspected in figure space through the helpers in [`probe`].

mod common;
mod probe;

mod artists;
mod decimation;
mod dense;
mod depth;
mod images;
mod layout;
mod legend;
mod markers;
mod robustness;
mod scales;
mod surfaces_2d;
mod three_d;
mod validity;
