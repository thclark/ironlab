//! The legend panel handle.

use ironlab_ir::{
    Legend, LegendEntryColumns, LegendEntryOrder, LegendLocation, NodeId, PanelLegend,
};

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
/// ```
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
    pub(crate) fn new(fig: &'f mut ironlab_ir::Figure, id: NodeId) -> Self {
        Self { fig, id }
    }
}

impl PanelLegendMut<'_> {
    /// Returns the node identifier of the panel, which axes name to list their entries
    /// in it.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets where the block of entries is placed within the panel. `Best` places it at
    /// the centre, because a panel holds no data for the entries to avoid.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(1, 2);
    /// fig.panel_legend(0, 1).align(LegendLocation::NorthWest);
    /// ```
    pub fn align(&mut self, location: LegendLocation) -> &mut Self {
        self.panel().align = location;
        self
    }

    /// Sets whether the area of the panel is outlined by a border, which is the default.
    /// The border is drawn as the box of an axes is drawn, and continues the edges of the
    /// boxes of the axes in the panel's rows and columns, because the area of the panel is
    /// aligned with their plot areas.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(1, 2);
    /// fig.panel_legend(0, 1).boxed(false);
    /// ```
    pub fn boxed(&mut self, on: bool) -> &mut Self {
        self.panel().boxed = on;
        self
    }

    /// Sets the number of columns in which the entries are arranged.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(2, 1);
    /// fig.panel_legend(1, 0).columns(LegendEntryColumns::Count(4));
    /// ```
    pub fn columns(&mut self, columns: LegendEntryColumns) -> &mut Self {
        self.panel().layout.columns = columns;
        self
    }

    /// Sets the order in which the entries fill the columns.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(2, 1);
    /// fig.panel_legend(1, 0)
    ///     .columns(LegendEntryColumns::Count(3))
    ///     .order(LegendEntryOrder::RowMajor);
    /// ```
    pub fn order(&mut self, order: LegendEntryOrder) -> &mut Self {
        self.panel().layout.order = order;
        self
    }

    /// Points the legend of every axes the figure has when this is called at this panel,
    /// replacing any legend inside their plot areas. Axes created afterwards are not
    /// affected.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(0.0, 1.0, 11);
    /// let mut fig = Figure::new().tiles(1, 3);
    /// for col in 0..2 {
    ///     fig.axes(0, col).plot(&x, &x).display_name("Linear");
    /// }
    /// fig.panel_legend(0, 2).collect_all();
    /// assert!(fig.validate().warnings.is_empty());
    /// ```
    pub fn collect_all(&mut self) -> &mut Self {
        for axes in &mut self.fig.axes {
            axes.legend = Some(Legend::Panel(self.id));
        }
        self
    }

    /// Returns the panel, which exists because the handle is only created for a panel of
    /// the figure.
    fn panel(&mut self) -> &mut PanelLegend {
        self.fig
            .panel_legend_mut(self.id)
            .expect("a legend panel handle names a panel of the figure")
    }
}
