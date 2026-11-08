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
mod axis_side;
mod bars;
mod blended;
mod cells;
mod colour_scales;
mod colourbar_hits;
mod colourbar_layout;
mod colourbars;
mod curved3d;
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
mod legend_colour_keys;
mod legend_items;
mod markers;
mod panel_legend;
mod polar;
mod polar_decor;
mod robustness;
mod scales;
mod series;
mod series_fill;
mod stairs;
mod surfaces_2d;
mod three_d;
mod ticks;
mod titles;
mod validity;
