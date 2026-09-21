//! Pure numerical building blocks for scene compilation.
//!
//! Nothing in this module depends on the figure IR or on text shaping. Each submodule is a
//! self-contained piece of plotting mathematics that the scene compiler composes:
//!
//! - [`ticks`] chooses tick positions, axis limits and tick label strings.
//! - [`colormap`] holds the 256-entry colour lookup tables and maps values into them.
//! - [`image`] resolves the pixels of images: it classifies mapped values and indices against the colormap and
//!   quantises true-colour components.
//! - [`contour`] extracts isolines and filled isobands from gridded scalar fields.
//! - [`quiver`] scales vector fields and builds arrow geometry.
//! - [`camera`] projects the normalised 3D data box onto the screen and orders geometry by depth.
//! - [`decimate`] thins large point series to the resolution the current view can show.

pub mod camera;
pub mod colormap;
mod colormap_data;
pub mod contour;
pub mod decimate;
pub mod image;
pub mod quiver;
pub mod ticks;
