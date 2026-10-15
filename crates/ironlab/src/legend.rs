//! The legend panel handle.

use ironlab_ir::{LegendEntryColumns, LegendEntryOrder, LegendLocation, NodeId};

/// A handle to one legend panel of a figure, through which its properties are set.
///
/// A legend panel occupies cells of the figure's tile layout, as an axes does, and lists
/// the entries of every axes whose legend names it (see
/// [`AxesMut::legend_in`](crate::AxesMut::legend_in)). Entries of different axes with the
/// same display name and the same look are listed once.
///
/// Like [`AxesMut`](crate::AxesMut), the handle borrows the figure mutably, so it is
/// short-lived: keep its [`id`](PanelLegendMut::id) to point axes at the panel later.
/// Property setters return the handle so that they can be chained.
///
/// ```no_run
/// use ironlab::prelude::*;
///
/// let x = linspace(0.0, 1.0, 11);
/// let mut fig = Figure::new().tiles(2, 2).row_weights([3.0, 1.0]);
/// let key = fig.panel_legend_span(1, 0, 1, 2).id();
/// for col in 0..2 {
///     let mut ax = fig.axes(0, col);
///     ax.plot(&x, &x).display_name("Linear");
///     ax.legend_in(key);
/// }
/// ```
#[derive(Debug)]
pub struct PanelLegendMut<'f> {
    pub(crate) fig: &'f mut ironlab_ir::Figure,
    pub(crate) id: NodeId,
}

impl<'f> PanelLegendMut<'f> {
    /// Creates a handle to the legend panel with the given identifier, which must be a
    /// legend panel of the figure.
    #[allow(dead_code)]
    pub(crate) fn new(fig: &'f mut ironlab_ir::Figure, id: NodeId) -> Self {
        Self { fig, id }
    }
}

impl PanelLegendMut<'_> {
    /// Returns the node identifier of the panel, which axes name to list their entries
    /// in it.
    #[must_use]
    pub fn id(&self) -> NodeId {
        todo!("return the identifier of the panel {}", self.id)
    }

    /// Sets where the block of entries is placed within the panel. `Best` places it at
    /// the centre, because a panel holds no data for the entries to avoid.
    pub fn align(&mut self, location: LegendLocation) -> &mut Self {
        let _ = (&self.fig, location);
        todo!("set the alignment of the panel")
    }

    /// Sets whether the block of entries is drawn with a background and outline.
    pub fn boxed(&mut self, on: bool) -> &mut Self {
        let _ = on;
        todo!("set whether the panel is boxed")
    }

    /// Sets the number of columns in which the entries are arranged.
    pub fn columns(&mut self, columns: LegendEntryColumns) -> &mut Self {
        let _ = columns;
        todo!("set the columns of the panel")
    }

    /// Sets the order in which the entries fill the columns.
    pub fn order(&mut self, order: LegendEntryOrder) -> &mut Self {
        let _ = order;
        todo!("set the order of the panel")
    }

    /// Points the legend of every axes the figure has when this is called at this panel,
    /// replacing any legend inside their plot areas. Axes created afterwards are not
    /// affected.
    pub fn collect_all(&mut self) -> &mut Self {
        todo!("point every axes at the panel")
    }
}
