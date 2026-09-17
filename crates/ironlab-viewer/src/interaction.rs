//! Pointer interaction as edits of the figure IR.
//!
//! This module is pure logic with no GPU or windowing dependency, so every gesture can be unit tested. All positions
//! are in figure space (points, origin at the top-left corner of the figure, y down); the canvas converts screen
//! positions with [`crate::ScreenTransform::invert`] before calling in. Geometry comes from the [`HitMap`] of the most
//! recent compilation of [`FigureState::current`].
//!
//! # Semantics
//!
//! - **Wheel zoom** (2D) rescales the x and y limits of the axes under the pointer about the data point under the
//!   pointer, in the axis' scale space (linear values, or base-10 logarithms for log axes), so that this data point
//!   stays under the pointer. A factor greater than one zooms in. In 3D it multiplies `view3d.zoom` by the factor.
//! - **Pan** (2D) shifts the limits so that the data point grabbed at the start of the drag stays under the pointer.
//!   Every update is computed from the axis maps captured at the start of the drag rather than incrementally, so
//!   repeated updates never accumulate rounding drift. In 3D, pan moves `view3d.pan_x` and `view3d.pan_y` by the pointer
//!   displacement as fractions of the plot rectangle: `pan_x` increases to the right and `pan_y` increases downwards
//!   (figure-space y), so the projected box follows the pointer.
//! - **Rotate** (3D) follows MATLAB's `rotate3d`, in which the object follows the pointer: dragging right by `dx`
//!   points decreases the azimuth by `dx ·` [`ROTATE_DEGREES_PER_POINT`], and dragging down (positive figure-space
//!   `dy`) increases the elevation by `dy ·` [`ROTATE_DEGREES_PER_POINT`]. The elevation is clamped to `[-90, 90]`;
//!   the azimuth is not wrapped. A Rotate-tool drag on a 2D axes does nothing.
//! - **Box zoom** (Zoom tool, 2D) records a rubber band from the drag start to the pointer, clamped to the plot
//!   rectangle, and on release sets the x and y limits to the data range the band covers. A band narrower or shorter
//!   than [`MIN_BOX_ZOOM_POINTS`] is ignored, so that an accidental click does not zoom to a sliver.
//! - **Double click** restores the limits (x, y and z) and 3D view of the axes under the pointer from the snapshot.
//!   On a legend entry, the second click of a double-click is a click like the first, so a double-click toggles the
//!   artist twice, leaves its visibility as it was and does not restore any limits.
//! - **Click** on a legend entry toggles the visibility of its artist.
//! - **Reset view** restores the limits and 3D views of every axes from the snapshot, but keeps artist visibility.
//!
//! Every limit change goes through [`Figure::set_limits`], so linked axes follow. A 2D gesture on an axes whose
//! limits are [`Limits::Auto`] starts from the limits the compiler resolved (the hit map's
//! [`AxisMap`]) and writes [`Limits::Manual`] limits.

use ironlab_ir::{Axes, Axis, Dimension, Figure, Limits, NodeId, Projection, Scale, View3d};
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::hit::{AxesHitKind, AxisMap, HitMap};

/// Degrees of azimuth or elevation per figure point of pointer travel in the Rotate tool.
pub const ROTATE_DEGREES_PER_POINT: f64 = 0.5;

/// The smallest width and height, in figure points, of a rubber band that the Zoom tool applies.
pub const MIN_BOX_ZOOM_POINTS: f64 = 3.0;

/// The gesture that a primary-button drag performs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Drag pans 2D limits or the 3D view.
    #[default]
    Pan,
    /// Drag draws a rubber band that zooms 2D axes to its extent.
    Zoom,
    /// Drag rotates 3D axes.
    Rotate,
}

/// The state captured at the start of a drag.
#[derive(Clone, Debug, PartialEq)]
struct Drag {
    /// The axes under the pointer when the drag started.
    axes: NodeId,
    /// The plot rectangle of that axes at the start of the drag.
    plot_rect: Rect,
    /// The pointer position at the start of the drag.
    start: Point,
    /// The most recent pointer position.
    last: Point,
    /// The geometry captured at the start of the drag.
    kind: DragKind,
}

#[derive(Clone, Debug, PartialEq)]
enum DragKind {
    /// A 2D axes, with its x and y axis maps at the start of the drag.
    TwoD { x: AxisMap, y: AxisMap },
    /// A 3D axes, with its view at the start of the drag.
    ThreeD { view: ironlab_ir::View3d },
}

/// The interactive state of one figure in the viewer.
#[derive(Clone, Debug)]
pub struct FigureState {
    /// The figure as currently displayed; interaction edits it and export writes it.
    pub current: Figure,
    /// The figure as loaded, used to reset limits and views.
    pub snapshot: Figure,
    /// The active drag tool.
    pub tool: Tool,
    drag: Option<Drag>,
}

impl FigureState {
    /// Creates the state for a freshly loaded figure, with the Pan tool active.
    #[must_use]
    pub fn new(figure: Figure) -> Self {
        Self {
            snapshot: figure.clone(),
            current: figure,
            tool: Tool::Pan,
            drag: None,
        }
    }

    /// Zooms the axes under `at` by `zoom_factor` (greater than one zooms in). Returns whether the figure changed.
    pub fn scroll(&mut self, hit: &HitMap, at: Point, zoom_factor: f64) -> bool {
        if !(zoom_factor.is_finite() && zoom_factor > 0.0) || zoom_factor == 1.0 {
            return false;
        }
        let Some(axes_hit) = hit.axes_at(at) else {
            return false;
        };
        let id = axes_hit.id;
        let Some(axes) = self.current.axes(id) else {
            return false;
        };
        match (&axes_hit.kind, axes.projection) {
            (AxesHitKind::TwoD { x, y }, Projection::TwoD) => {
                let x = current_map(&axes.x, x);
                let y = current_map(&axes.y, y);
                let zoom = |map: &AxisMap, coord: f64| {
                    let (a, b) = scale_limits(map);
                    let t = (coord - map.start) / (map.end - map.start);
                    let c = a + t * (b - a);
                    (c - (c - a) / zoom_factor, c + (b - c) / zoom_factor)
                };
                let x_limits = zoom(&x, at.x);
                let y_limits = zoom(&y, at.y);
                let changed_x = self.set_scaled_limits(id, Dimension::X, &x, x_limits);
                let changed_y = self.set_scaled_limits(id, Dimension::Y, &y, y_limits);
                changed_x || changed_y
            }
            (AxesHitKind::ThreeD, Projection::ThreeD { view3d }) => {
                let zoom = view3d.zoom * zoom_factor;
                if !(zoom.is_finite() && zoom > 0.0) {
                    return false;
                }
                self.set_view(id, View3d { zoom, ..view3d })
            }
            _ => false,
        }
    }

    /// Starts a drag at `at`, capturing the axes under the pointer and its axis maps or view.
    ///
    /// A drag that starts outside every axes is ignored by the subsequent updates.
    pub fn drag_start(&mut self, hit: &HitMap, at: Point) {
        self.drag = None;
        let Some(axes_hit) = hit.axes_at(at) else {
            return;
        };
        let Some(axes) = self.current.axes(axes_hit.id) else {
            return;
        };
        let kind = match (&axes_hit.kind, axes.projection) {
            (AxesHitKind::TwoD { x, y }, Projection::TwoD) => DragKind::TwoD {
                x: current_map(&axes.x, x),
                y: current_map(&axes.y, y),
            },
            (AxesHitKind::ThreeD, Projection::ThreeD { view3d }) => {
                DragKind::ThreeD { view: view3d }
            }
            _ => return,
        };
        self.drag = Some(Drag {
            axes: axes_hit.id,
            plot_rect: axes_hit.plot_rect,
            start: at,
            last: at,
            kind,
        });
    }

    /// Updates the drag with the pointer at `at`. Returns whether the figure changed.
    pub fn drag_update(&mut self, at: Point) -> bool {
        if !is_finite(at) {
            return false;
        }
        let Some(drag) = self.drag.as_mut() else {
            return false;
        };
        drag.last = at;
        let drag = drag.clone();
        let (dx, dy) = (at.x - drag.start.x, at.y - drag.start.y);
        match (self.tool, &drag.kind) {
            (Tool::Pan, DragKind::TwoD { x, y }) => {
                let pan = |map: &AxisMap, from: f64, to: f64| {
                    let (a, b) = scale_limits(map);
                    let per_point = (b - a) / (map.end - map.start);
                    let shift = (from - to) * per_point;
                    (a + shift, b + shift)
                };
                let changed_x = dx != 0.0
                    && self.set_scaled_limits(
                        drag.axes,
                        Dimension::X,
                        x,
                        pan(x, drag.start.x, at.x),
                    );
                let changed_y = dy != 0.0
                    && self.set_scaled_limits(
                        drag.axes,
                        Dimension::Y,
                        y,
                        pan(y, drag.start.y, at.y),
                    );
                // Returning to the start of the drag restores the limits captured at its start.
                let restored_x = dx == 0.0 && self.restore_captured(drag.axes, Dimension::X, x);
                let restored_y = dy == 0.0 && self.restore_captured(drag.axes, Dimension::Y, y);
                changed_x || changed_y || restored_x || restored_y
            }
            (Tool::Pan, DragKind::ThreeD { view }) => {
                let Some(current) = self.view(drag.axes) else {
                    return false;
                };
                let pan_x = view.pan_x + dx / drag.plot_rect.width;
                let pan_y = view.pan_y + dy / drag.plot_rect.height;
                if !(pan_x.is_finite() && pan_y.is_finite()) {
                    return false;
                }
                self.set_view(
                    drag.axes,
                    View3d {
                        pan_x,
                        pan_y,
                        ..current
                    },
                )
            }
            (Tool::Rotate, DragKind::ThreeD { view }) => {
                let Some(current) = self.view(drag.axes) else {
                    return false;
                };
                let azimuth_deg = view.azimuth_deg - dx * ROTATE_DEGREES_PER_POINT;
                let elevation_deg =
                    (view.elevation_deg + dy * ROTATE_DEGREES_PER_POINT).clamp(-90.0, 90.0);
                self.set_view(
                    drag.axes,
                    View3d {
                        azimuth_deg,
                        elevation_deg,
                        ..current
                    },
                )
            }
            // The Zoom tool only moves its rubber band while dragging, a 3D axes has no band, and a 2D axes has
            // nothing to rotate.
            (Tool::Zoom, _) | (Tool::Rotate, DragKind::TwoD { .. }) => false,
        }
    }

    /// Ends the drag with the pointer at `at`, applying a box zoom when the Zoom tool is active. Returns whether the
    /// figure changed.
    pub fn drag_end(&mut self, at: Point) -> bool {
        if self.drag.is_none() {
            return false;
        }
        let mut changed = false;
        if self.tool == Tool::Zoom {
            if is_finite(at)
                && let Some(drag) = self.drag.as_mut()
            {
                drag.last = at;
            }
            if let (
                Some(band),
                Some(Drag {
                    axes,
                    kind: DragKind::TwoD { x, y },
                    ..
                }),
            ) = (self.rubber_band(), self.drag.clone())
                && band.width >= MIN_BOX_ZOOM_POINTS
                && band.height >= MIN_BOX_ZOOM_POINTS
            {
                let span = |map: &AxisMap, p: f64, q: f64| {
                    let (u, v) = (to_scale(map, map.to_data(p)), to_scale(map, map.to_data(q)));
                    (u.min(v), u.max(v))
                };
                let x_limits = span(&x, band.x, band.right());
                let y_limits = span(&y, band.y, band.bottom());
                let changed_x = self.set_scaled_limits(axes, Dimension::X, &x, x_limits);
                let changed_y = self.set_scaled_limits(axes, Dimension::Y, &y, y_limits);
                changed = changed_x || changed_y;
            }
        } else {
            changed = self.drag_update(at);
        }
        self.drag = None;
        changed
    }

    /// Returns the rubber band of an ongoing Zoom-tool drag, in figure space.
    #[must_use]
    pub fn rubber_band(&self) -> Option<Rect> {
        if self.tool != Tool::Zoom {
            return None;
        }
        let drag = self.drag.as_ref()?;
        if !matches!(drag.kind, DragKind::TwoD { .. }) {
            return None;
        }
        let r = drag.plot_rect;
        let clamp = |p: Point| Point::new(p.x.clamp(r.x, r.right()), p.y.clamp(r.y, r.bottom()));
        let (a, b) = (clamp(drag.start), clamp(drag.last));
        Some(Rect::new(
            a.x.min(b.x),
            a.y.min(b.y),
            (a.x - b.x).abs(),
            (a.y - b.y).abs(),
        ))
    }

    /// Restores the limits and view of the axes under `at` (and, through links, its linked axes) from the snapshot.
    /// Over a legend entry it toggles the entry's artist instead, as [`FigureState::click`] does. Returns whether the
    /// figure changed.
    pub fn double_click(&mut self, hit: &HitMap, at: Point) -> bool {
        if hit.legend_entry_at(at).is_some() {
            return self.click(hit, at);
        }
        let Some(axes_hit) = hit.axes_at(at) else {
            return false;
        };
        let id = axes_hit.id;
        let Some(snapshot) = self.snapshot.axes(id).cloned() else {
            return false;
        };
        let Some(current) = self.current.axes(id).cloned() else {
            return false;
        };
        let mut changed = false;
        for dimension in [Dimension::X, Dimension::Y, Dimension::Z] {
            let (old, now) = (
                axis(&snapshot, dimension).limits,
                axis(&current, dimension).limits,
            );
            let group_differs = self
                .current
                .linked_axes(id, dimension)
                .iter()
                .filter_map(|&a| self.current.axes(a))
                .any(|a| axis(a, dimension).limits != old);
            if (old != now || group_differs) && self.current.set_limits(id, dimension, old).is_ok()
            {
                changed = true;
            }
        }
        if let (Projection::ThreeD { view3d: old }, Projection::ThreeD { .. }) =
            (snapshot.projection, current.projection)
        {
            changed |= self.set_view(id, old);
        }
        changed
    }

    /// Toggles the visibility of the artist whose legend entry is under `at`. Returns whether the figure changed.
    pub fn click(&mut self, hit: &HitMap, at: Point) -> bool {
        let Some(entry) = hit.legend_entry_at(at) else {
            return false;
        };
        let Some(artist) = self.current.artist_mut(entry.artist) else {
            return false;
        };
        let visible = artist.visible();
        artist.set_visible(!visible);
        true
    }

    /// Restores the limits and 3D views of every axes from the snapshot, keeping artist visibility. Returns whether
    /// the figure changed.
    pub fn reset_view(&mut self) -> bool {
        self.drag = None;
        let mut changed = false;
        for axes in &mut self.current.axes {
            let Some(snapshot) = self.snapshot.axes.iter().find(|a| a.id == axes.id) else {
                continue;
            };
            for (axis, old) in [
                (&mut axes.x, &snapshot.x),
                (&mut axes.y, &snapshot.y),
                (&mut axes.z, &snapshot.z),
            ] {
                if axis.limits != old.limits {
                    axis.limits = old.limits;
                    changed = true;
                }
            }
            if let (Projection::ThreeD { view3d }, Projection::ThreeD { view3d: old }) =
                (&mut axes.projection, snapshot.projection)
                && *view3d != old
            {
                *view3d = old;
                changed = true;
            }
        }
        changed
    }

    /// Returns whether the current figure has at least one 3D axes.
    #[must_use]
    pub fn has_3d(&self) -> bool {
        self.current
            .axes
            .iter()
            .any(|axes| matches!(axes.projection, Projection::ThreeD { .. }))
    }

    /// Returns the current 3D view of an axes, or `None` when it is not a 3D axes.
    fn view(&self, id: NodeId) -> Option<View3d> {
        match self.current.axes(id)?.projection {
            Projection::ThreeD { view3d } => Some(view3d),
            Projection::TwoD => None,
        }
    }

    /// Sets the 3D view of an axes. Returns whether the figure changed.
    fn set_view(&mut self, id: NodeId, view: View3d) -> bool {
        let Some(axes) = self.current.axes_mut(id) else {
            return false;
        };
        match &mut axes.projection {
            Projection::ThreeD { view3d } if *view3d != view => {
                *view3d = view;
                true
            }
            _ => false,
        }
    }

    /// Sets manual limits given in the scale space of `map` (base-10 logarithms on a log axis). Returns whether the
    /// figure changed; invalid limits (for example after overflow) are ignored.
    fn set_scaled_limits(
        &mut self,
        id: NodeId,
        dimension: Dimension,
        map: &AxisMap,
        (a, b): (f64, f64),
    ) -> bool {
        let (min, max) = (from_scale(map, a), from_scale(map, b));
        self.set_manual(id, dimension, min, max)
    }

    /// Sets manual limits, through links. Returns whether the figure changed.
    fn set_manual(&mut self, id: NodeId, dimension: Dimension, min: f64, max: f64) -> bool {
        let limits = Limits::Manual { min, max };
        let unchanged = self
            .current
            .linked_axes(id, dimension)
            .iter()
            .filter_map(|&a| self.current.axes(a))
            .all(|a| axis(a, dimension).limits == limits);
        if unchanged {
            return false;
        }
        self.current.set_limits(id, dimension, limits).is_ok()
    }

    /// Restores the limits captured at the start of a drag, unless the figure still shows them (including when they
    /// are automatic and unchanged). Returns whether the figure changed.
    fn restore_captured(&mut self, id: NodeId, dimension: Dimension, map: &AxisMap) -> bool {
        let Some(axes) = self.current.axes(id) else {
            return false;
        };
        match axis(axes, dimension).limits {
            Limits::Auto => false,
            Limits::Manual { min, max } if min == map.min && max == map.max => false,
            Limits::Manual { .. } => self.set_manual(id, dimension, map.min, map.max),
        }
    }
}

fn is_finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn axis(axes: &Axes, dimension: Dimension) -> &Axis {
    match dimension {
        Dimension::X => &axes.x,
        Dimension::Y => &axes.y,
        Dimension::Z => &axes.z,
    }
}

/// Returns the axis map to navigate from: the figure-space extent always comes from the hit map, but manual limits
/// and the scale come from the figure itself, so that a hit map compiled before the latest edit cannot undo it.
fn current_map(axis: &Axis, hit: &AxisMap) -> AxisMap {
    let (min, max) = match axis.limits {
        Limits::Manual { min, max } => (min, max),
        Limits::Auto => (hit.min, hit.max),
    };
    AxisMap {
        min,
        max,
        log: axis.scale == Scale::Log,
        start: hit.start,
        end: hit.end,
    }
}

fn to_scale(map: &AxisMap, value: f64) -> f64 {
    if map.log { value.log10() } else { value }
}

fn from_scale(map: &AxisMap, value: f64) -> f64 {
    if map.log { 10f64.powf(value) } else { value }
}

/// The limits of `map` in its scale space.
fn scale_limits(map: &AxisMap) -> (f64, f64) {
    (to_scale(map, map.min), to_scale(map, map.max))
}
