//! Pointer interaction as typed edits recorded in a view overlay.
//!
//! This module is pure logic with no GPU or windowing dependency, so every gesture can be unit tested. All positions
//! are in figure space (points, origin at the top-left corner of the figure, y down); the canvas converts screen
//! positions with [`crate::ScreenTransform::invert`] before calling in. Geometry comes from the [`HitMap`] of the most
//! recent compilation of the displayed figure.
//!
//! # The source, the overlay and the displayed figure
//!
//! As decided in ADR 0008, [`FigureState`] holds the figure as its owner defines it (the **source**, which for the
//! viewer is the figure as opened or last saved), an [`Overlay`] of the sets the user has made, and the **displayed
//! figure**, which is the composition of the two. A gesture never mutates the source: it builds a [`Transaction`] of
//! sets and records it in the overlay with [`FigureState::record`], after which the displayed figure is composed
//! again. Composition is the only expensive step, so it runs when the overlay changes rather than on every frame.
//!
//! Limits are set through [`ironlab_ir::command::set_limits`], so that the transaction carries the limits of every
//! axes of the link group and a client that applies it reaches the same figure. Three-dimensional views and artist
//! visibility are set literally, one property per edit.
//!
//! An entry that the figure cannot show (for example limits that are not increasing) is dropped when the displayed
//! figure is composed. Such an entry is removed from the overlay and its reason is reported by
//! [`FigureState::problems`], which the toolbar shows alongside the warnings of the scene compiler.
//!
//! The property editor commits through [`FigureState::try_record`], which refuses such a change before it is
//! recorded, so that the figure is left as it was and no undo step is spent; [`FigureState::begin_edit_step`] and
//! [`FigureState::end_edit_step`] make a drag of a numeric field one step, as a drag on the canvas is.
//!
//! # Semantics
//!
//! - **Wheel zoom** (2D) rescales the x and y limits of the axes under the pointer about the data point under the
//!   pointer, in the axis' scale space (linear values, or base-10 logarithms for log axes), so that this data point
//!   stays under the pointer. A factor greater than one zooms in. In 3D it multiplies `projection.view3d.zoom` by the
//!   factor.
//! - **Pan** (2D) shifts the limits so that the data point grabbed at the start of the drag stays under the pointer.
//!   Every update is computed from the axis maps captured at the start of the drag rather than incrementally, so
//!   repeated updates never accumulate rounding drift. In 3D, pan moves `projection.view3d.pan_x` and
//!   `projection.view3d.pan_y` by the pointer displacement as fractions of the plot rectangle: `pan_x` increases to
//!   the right and `pan_y` increases downwards (figure-space y), so the projected box follows the pointer.
//! - **Rotate** (3D) follows MATLAB's `rotate3d`, in which the object follows the pointer: dragging right by `dx`
//!   points decreases the azimuth by `dx ·` [`ROTATE_DEGREES_PER_POINT`], and dragging down (positive figure-space
//!   `dy`) increases the elevation by `dy ·` [`ROTATE_DEGREES_PER_POINT`]. The elevation is clamped to `[-90, 90]`;
//!   the azimuth is not wrapped. A Rotate-tool drag on a 2D axes does nothing.
//! - **Box zoom** (Zoom tool, 2D) records a rubber band from the drag start to the pointer, clamped to the plot
//!   rectangle, and on release sets the x and y limits to the data range the band covers. A band narrower or shorter
//!   than [`MIN_BOX_ZOOM_POINTS`] is ignored, so that an accidental click does not zoom to a sliver.
//! - **Double click** removes the view entries (limits and three-dimensional view) of the axes under the pointer from
//!   the overlay, so that it shows the source again, and sets the axes linked with it to the limits restored. On a
//!   legend entry, the second click of a double-click is a click like the first, so a double-click toggles the artist
//!   twice, leaves its visibility as it was and does not restore any limits.
//! - **Click** on a legend entry sets `visible` on its artist and selects it; a click elsewhere inside an axes
//!   selects that axes and changes nothing. The selection is what the property editor shows, and no gesture
//!   changes it.
//! - **Reset view** removes the view entries of every axes, but keeps visibility, because hiding a plot is a choice
//!   about content rather than about the view.
//!
//! Each gesture is one step of the undo history: a whole drag is one step, as is a wheel notch, a legend click, a
//! double-click and a reset. A 2D gesture on an axes whose limits are [`Limits::Auto`] starts from the limits the
//! compiler resolved (the hit map's [`AxisMap`]) and records [`Limits::Manual`] limits, leaving the source automatic.

use ironlab_ir::overlay::Overlay;
use ironlab_ir::{
    Axes, Axis, Dimension, Edit, EditError, Figure, Limits, NodeId, Projection, PropertyPath,
    Scale, Transaction, Value, View3d, command,
};
use ironlab_scene::SceneWarning;
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
    ThreeD { view: View3d },
}

/// The interactive state of one figure in the viewer: its source, the user's overlay, and their composition.
#[derive(Clone, Debug)]
pub struct FigureState {
    /// The figure as its owner defines it: as opened, or as last saved. Gestures never change it.
    source: Figure,
    /// The sets the user has made, with their undo history.
    overlay: Overlay,
    /// The source with the overlay applied: what is drawn, hit-tested, exported and saved. Recomposed when the
    /// overlay changes rather than on every frame, because composing clones the source.
    composed: Figure,
    /// The overlay entries that composition dropped, as problems to show in the toolbar.
    problems: Vec<SceneWarning>,
    /// The active drag tool.
    pub tool: Tool,
    drag: Option<Drag>,
    /// The node the property editor shows, which a click on the canvas and a click in the
    /// object tree both set.
    selected: Option<NodeId>,
}

impl FigureState {
    /// Creates the state for a freshly opened figure, with the Pan tool active and an empty overlay.
    #[must_use]
    pub fn new(figure: Figure) -> Self {
        let id = figure.id;
        Self {
            composed: figure.clone(),
            source: figure,
            overlay: Overlay::new(),
            problems: Vec::new(),
            tool: Tool::Pan,
            drag: None,
            selected: Some(id),
        }
    }

    /// Returns the displayed figure: the source with the overlay applied, which is what is drawn, exported and saved.
    #[must_use]
    pub fn figure(&self) -> &Figure {
        &self.composed
    }

    /// Returns the source figure, which no gesture changes.
    #[must_use]
    pub fn source(&self) -> &Figure {
        &self.source
    }

    /// Returns the overlay of the user's changes.
    #[must_use]
    pub fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    /// Returns the node that the property editor shows, which is the figure itself until
    /// something else is selected.
    #[must_use]
    pub fn selection(&self) -> Option<NodeId> {
        self.selected
    }

    /// Selects a node, or nothing. A node that is not in the displayed figure selects
    /// nothing, so that the property editor never shows a node that has gone.
    pub fn select(&mut self, node: Option<NodeId>) {
        self.selected = node.filter(|node| self.composed.node_kind(*node).is_some());
    }

    /// Opens an undo step, so that everything recorded until [`FigureState::end_edit_step`]
    /// is undone in one step.
    ///
    /// The property editor holds a step open while a numeric field is dragged or a text
    /// field is being typed into, so that the change ends as one step rather than one per
    /// frame, exactly as a drag on the canvas does.
    pub fn begin_edit_step(&mut self) {
        self.overlay.begin_step();
    }

    /// Closes the step opened by [`FigureState::begin_edit_step`]. A step that changed
    /// nothing adds nothing to the history.
    pub fn end_edit_step(&mut self) {
        self.overlay.end_step();
    }

    /// Records a transaction that the displayed figure must accept, which is what the
    /// property editor commits.
    ///
    /// The transaction is first applied to a copy of the displayed figure. When the IR
    /// refuses it (limits that are not increasing, a projection that an artist cannot be
    /// drawn in), nothing is recorded: the figure is left exactly as it was, no undo step
    /// is spent, and the reason is added to [`FigureState::problems`], which the toolbar
    /// shows. Otherwise the transaction is recorded as [`FigureState::record`] does.
    ///
    /// Returns whether the displayed figure changed.
    pub fn try_record(&mut self, transaction: &Transaction) -> bool {
        let mut trial = self.composed.clone();
        if let Err(error) = trial.apply(transaction) {
            self.report(refusal(transaction, &error));
            return false;
        }
        self.record(transaction)
    }

    /// Removes the overlay entry for exactly one property of one node, as the property
    /// editor's revert control does, and recomposes the displayed figure so that the
    /// source's own value is shown again.
    ///
    /// The revert is its own undo step. Returns whether there was an entry to remove.
    pub fn revert(&mut self, node: NodeId, path: &PropertyPath) -> bool {
        if !self.overlay.revert(node, path) {
            return false;
        }
        self.recompose();
        true
    }

    /// Returns the problems raised by changes that the figure could not show, in the order they arose.
    ///
    /// They are kept until the view is reset or the overlay is folded into the source, and the toolbar shows them
    /// alongside the warnings of the scene compiler.
    #[must_use]
    pub fn problems(&self) -> &[SceneWarning] {
        &self.problems
    }

    /// Returns the number of changes the user has made, which is the number of entries in
    /// the overlay: what "Revert all changes" would discard.
    #[must_use]
    pub fn change_count(&self) -> usize {
        self.overlay.entries().len()
    }

    /// Discards every change the user has made — limits, three-dimensional views,
    /// visibility and every property edited — so that the figure its owner defines is
    /// shown again, and returns whether there was one to discard.
    ///
    /// The source is not touched, so this removes every change made to the figure rather
    /// than making one. It is a clean slate rather than a step of the history: the undo
    /// and redo history goes with the changes, so nothing can be undone straight after
    /// it, and every problem raised by those changes goes too.
    ///
    /// This is wider than [`FigureState::reset_view`], which discards only the limits and
    /// three-dimensional views, keeps the visibility of plots and every property edited,
    /// and is itself a step of the history.
    pub fn revert_all(&mut self) -> bool {
        self.end_drag();
        let discarded = !self.overlay.entries().is_empty();
        self.overlay.clear();
        self.problems.clear();
        if discarded {
            self.recompose();
        }
        discarded
    }

    /// Records a transaction of sets made by the user in the overlay and recomposes the displayed figure.
    ///
    /// A set whose value the displayed figure already has is left out, so that a gesture that changes nothing records
    /// nothing. A transaction that holds any edit other than a set is refused, because the overlay holds only sets.
    /// Unless a drag is in progress, the transaction is one step of the undo history. Returns whether the overlay
    /// changed, which is what tells the canvas to recompile the scene.
    pub fn record(&mut self, transaction: &Transaction) -> bool {
        let edits: Vec<Edit> = transaction
            .edits
            .iter()
            .filter(|edit| self.changes_the_figure(edit))
            .cloned()
            .collect();
        if edits.is_empty() {
            return false;
        }
        let before = self.overlay.entries().to_vec();
        if self.overlay.record(&Transaction { edits }).is_err() {
            return false;
        }
        self.recompose();
        self.overlay.entries() != before
    }

    /// Restores the overlay to its state before the most recent gesture. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        self.undone(Overlay::undo)
    }

    /// Re-applies the most recently undone gesture. Returns whether there was one.
    pub fn redo(&mut self) -> bool {
        self.undone(Overlay::redo)
    }

    /// Returns whether there is a gesture to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.overlay.can_undo()
    }

    /// Returns whether there is a gesture to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.overlay.can_redo()
    }

    /// Makes the displayed figure the source and empties the overlay, as saving does.
    ///
    /// The viewer owns the source, so the figure it has just written is the figure its owner now defines; the changes
    /// written can no longer be undone, and resetting the view restores the figure as saved.
    pub fn fold_overlay(&mut self) {
        self.source = self.composed.clone();
        self.overlay = Overlay::new();
        self.problems.clear();
        self.drag = None;
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
        let Some(axes) = self.composed.axes(id) else {
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
                self.step(|state| {
                    let changed_x = state.set_scaled_limits(id, Dimension::X, &x, x_limits);
                    let changed_y = state.set_scaled_limits(id, Dimension::Y, &y, y_limits);
                    changed_x || changed_y
                })
            }
            (AxesHitKind::ThreeD, Projection::ThreeD { view3d }) => {
                let zoom = view3d.zoom * zoom_factor;
                if !(zoom.is_finite() && zoom > 0.0) {
                    return false;
                }
                self.set_view(id, &[("zoom", zoom)])
            }
            _ => false,
        }
    }

    /// Starts a drag at `at`, capturing the axes under the pointer and its axis maps or view.
    ///
    /// Everything the drag records is one step of the undo history. A drag that starts outside every axes is ignored
    /// by the subsequent updates.
    pub fn drag_start(&mut self, hit: &HitMap, at: Point) {
        self.end_drag();
        let Some(axes_hit) = hit.axes_at(at) else {
            return;
        };
        let Some(axes) = self.composed.axes(axes_hit.id) else {
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
        self.overlay.begin_step();
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
                if self.view(drag.axes).is_none() {
                    return false;
                }
                let pan_x = view.pan_x + dx / drag.plot_rect.width;
                let pan_y = view.pan_y + dy / drag.plot_rect.height;
                if !(pan_x.is_finite() && pan_y.is_finite()) {
                    return false;
                }
                self.set_view(drag.axes, &[("pan_x", pan_x), ("pan_y", pan_y)])
            }
            (Tool::Rotate, DragKind::ThreeD { view }) => {
                if self.view(drag.axes).is_none() {
                    return false;
                }
                let azimuth_deg = view.azimuth_deg - dx * ROTATE_DEGREES_PER_POINT;
                let elevation_deg =
                    (view.elevation_deg + dy * ROTATE_DEGREES_PER_POINT).clamp(-90.0, 90.0);
                self.set_view(
                    drag.axes,
                    &[
                        ("azimuth_deg", azimuth_deg),
                        ("elevation_deg", elevation_deg),
                    ],
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
        self.end_drag();
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

    /// Removes the view entries of the axes under `at` from the overlay, so that it shows the source again, and sets
    /// the axes linked with it to the limits restored.
    ///
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
        if self.composed.axes(id).is_none() {
            return false;
        }
        self.select(Some(id));
        self.step(|state| {
            let before = state.overlay.entries().to_vec();
            state.overlay.reset_view(id);
            let mut changed = state.overlay.entries() != before;
            if changed {
                state.recompose();
            }
            // The entries of the axes linked with this one are not removed, so they are set to the limits restored;
            // a set of limits that an axes already shows records nothing.
            for dimension in [Dimension::X, Dimension::Y, Dimension::Z] {
                let axes = state.composed.axes(id).expect("the axes is in the figure");
                let limits = axis(axes, dimension).limits;
                if let Ok(transaction) = command::set_limits(&state.composed, id, dimension, limits)
                {
                    changed |= state.record(&transaction);
                }
            }
            changed
        })
    }

    /// Sets `visible` on the artist whose legend entry is under `at`, to the opposite of what is displayed. Returns
    /// whether the figure changed.
    pub fn click(&mut self, hit: &HitMap, at: Point) -> bool {
        let Some(entry) = hit.legend_entry_at(at) else {
            if let Some(axes) = hit.axes_at(at) {
                self.select(Some(axes.id));
            }
            return false;
        };
        let Some((_, artist)) = self.composed.artist(entry.artist) else {
            return false;
        };
        let (artist_id, visible) = (entry.artist, artist.visible());
        self.select(Some(artist_id));
        self.record(&Transaction {
            edits: vec![Edit::Set {
                node: artist_id,
                path: path(&["visible"]),
                value: Value::Bool(!visible),
            }],
        })
    }

    /// Removes the view entries of every axes from the overlay, so that the figure shows the limits and
    /// three-dimensional views of the source again, and keeps the visibility of every artist.
    ///
    /// Returns whether the figure changed.
    pub fn reset_view(&mut self) -> bool {
        self.end_drag();
        self.problems.clear();
        let before = self.overlay.entries().to_vec();
        self.overlay.reset_all_views();
        if self.overlay.entries() == before {
            return false;
        }
        self.recompose();
        true
    }

    /// Returns whether the displayed figure has at least one 3D axes.
    #[must_use]
    pub fn has_3d(&self) -> bool {
        self.composed
            .axes
            .iter()
            .any(|axes| matches!(axes.projection, Projection::ThreeD { .. }))
    }

    /// Composes the displayed figure from the source and the overlay, discarding the entries that it cannot apply and
    /// keeping their reasons as problems.
    fn recompose(&mut self) {
        let composition = self.overlay.compose(&self.source);
        let mut problems = Vec::new();
        for dropped in &composition.dropped {
            problems.push(SceneWarning {
                node: Some(dropped.entry.node),
                message: format!(
                    "the change to {} was dropped: {}",
                    dropped.entry.path,
                    reason(&dropped.reason)
                ),
            });
        }
        for problem in problems {
            self.report(problem);
        }
        self.overlay.discard(&composition.dropped);
        self.composed = composition.figure;
        let selected = self.selected;
        self.select(selected);
    }

    /// Adds a problem, unless it is already reported.
    fn report(&mut self, problem: SceneWarning) {
        if !self.problems.contains(&problem) {
            self.problems.push(problem);
        }
    }

    /// Runs a gesture whose changes are one step of the undo history, unless a drag is already open, in which case
    /// they belong to the drag.
    fn step(&mut self, gesture: impl FnOnce(&mut Self) -> bool) -> bool {
        let open = self.drag.is_some();
        if !open {
            self.overlay.begin_step();
        }
        let changed = gesture(self);
        if !open {
            self.overlay.end_step();
        }
        changed
    }

    /// Ends the drag, closing its undo step.
    fn end_drag(&mut self) {
        self.drag = None;
        self.overlay.end_step();
    }

    /// Moves through the undo history and recomposes the displayed figure. Returns whether there was a step.
    fn undone(&mut self, step: impl FnOnce(&mut Overlay) -> bool) -> bool {
        self.end_drag();
        if !step(&mut self.overlay) {
            return false;
        }
        self.recompose();
        true
    }

    /// Returns whether an edit changes the displayed figure, so that a set of the value a property already has is
    /// left out of the overlay.
    fn changes_the_figure(&self, edit: &Edit) -> bool {
        match edit {
            Edit::Set { node, path, value } => match self.composed.get(*node, path) {
                Ok(current) => current != *value,
                Err(_) => true,
            },
            _ => true,
        }
    }

    /// Returns the 3D view of an axes of the displayed figure, or `None` when it is not a 3D axes.
    fn view(&self, id: NodeId) -> Option<View3d> {
        match self.composed.axes(id)?.projection {
            Projection::ThreeD { view3d } => Some(view3d),
            Projection::TwoD => None,
        }
    }

    /// Sets properties of the 3D view of an axes, each by name, such as `zoom` or `azimuth_deg`. Returns whether the
    /// figure changed.
    fn set_view(&mut self, id: NodeId, properties: &[(&str, f64)]) -> bool {
        if self.view(id).is_none() {
            return false;
        }
        let edits = properties
            .iter()
            .map(|&(property, value)| Edit::Set {
                node: id,
                path: path(&["projection", "view3d", property]),
                value: Value::Double(value),
            })
            .collect();
        self.record(&Transaction { edits })
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

    /// Sets manual limits on an axes and on the axes linked with it. Returns whether the figure changed.
    fn set_manual(&mut self, id: NodeId, dimension: Dimension, min: f64, max: f64) -> bool {
        let limits = Limits::Manual { min, max };
        match command::set_limits(&self.composed, id, dimension, limits) {
            Ok(transaction) => self.record(&transaction),
            Err(_) => false,
        }
    }

    /// Restores the limits captured at the start of a drag, unless the figure still shows them (including when they
    /// are automatic and unchanged). Returns whether the figure changed.
    fn restore_captured(&mut self, id: NodeId, dimension: Dimension, map: &AxisMap) -> bool {
        let Some(axes) = self.composed.axes(id) else {
            return false;
        };
        match axis(axes, dimension).limits {
            Limits::Auto => false,
            Limits::Manual { min, max } if min == map.min && max == map.max => false,
            Limits::Manual { .. } => self.set_manual(id, dimension, map.min, map.max),
        }
    }
}

/// Why an overlay entry could not be applied, in a sentence fit for the problems indicator.
///
/// The validation errors that an entry would introduce are given by their messages, because the error itself formats
/// them as the debug form of the whole report.
fn reason(error: &EditError) -> String {
    match error {
        EditError::Invalid(issues) => issues
            .iter()
            .map(|issue| issue.message.clone())
            .collect::<Vec<String>>()
            .join("; "),
        other => other.to_string(),
    }
}

/// The problem raised by a transaction that the figure refused, naming the property it concerned.
///
/// The error of an edit names its own path; an error of validation names none, so the path of the edit the error
/// concerns, or of the first set of the transaction, is used instead.
fn refusal(transaction: &Transaction, error: &EditError) -> SceneWarning {
    let named = transaction.edits.first().and_then(|edit| match edit {
        Edit::Set { node, path, .. } => Some((*node, path.clone())),
        _ => None,
    });
    let message = match &named {
        Some((_, path)) => format!("the change to {path} was refused: {}", reason(error)),
        None => format!("the change was refused: {}", reason(error)),
    };
    SceneWarning {
        node: named.map(|(node, _)| node),
        message,
    }
}

/// The property path of the given segments, which are the field names of the wire schema.
fn path(segments: &[&str]) -> PropertyPath {
    PropertyPath::new(segments.iter().copied()).expect("the segments name a property")
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
