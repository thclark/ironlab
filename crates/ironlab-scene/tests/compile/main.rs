//! Intent-capturing tests for `ironlab_scene::compile`, which turns a figure IR into a display list,
//! a hit map and warnings.
//!
//! Figures are assembled directly from `ironlab_ir` types by the fixture builders in [`common`], and
//! compiled scenes are inspected in figure space through the helpers in [`probe`].

mod common;
mod probe;

mod alpha;
mod artists;
mod aspect;
mod axis_display;
mod bars;
mod blended;
mod cells;
mod color_scales;
mod colorbar_hits;
mod colorbar_layout;
mod colorbars;
mod decimation;
mod dense;
mod depth;
mod direction;
mod embolden;
mod fitted_tracks;
mod images;
mod layout;
mod legend;
mod legend_bars;
mod legend_cells;
mod legend_color_keys;
mod legend_items;
mod markers;
mod panel_legend;
mod robustness;
mod scales;
mod series;
mod stairs;
mod surfaces_2d;
mod three_d;
mod ticks;
mod titles;
mod validity;
