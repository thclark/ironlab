//! The legend panel handle.

use ironlab_ir::{
    Legend, LegendEntries, LegendEntryColumns, LegendEntryOrder, LegendItem, LegendLocation,
    LegendSample, NodeId, PanelLegend, Text,
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

    /// Sets which entries the panel lists, and in what order.
    ///
    /// `LegendEntries::Auto`, the default, lists every artist with a display name of the
    /// axes that name the panel, merged by name and look, in the reading order of the
    /// layout. An explicit list names exactly the items to list, in their order. The
    /// item setters [`item`](PanelLegendMut::item),
    /// [`full_row`](PanelLegendMut::full_row) and
    /// [`custom_item`](PanelLegendMut::custom_item) build such a list one item at a time.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(2, 1);
    /// fig.panel_legend(1, 0).entries(LegendEntries::Explicit(vec![LegendItem::custom(
    ///     LegendSample::swatch(Color::rgb(0.9, 0.6, 0.0)),
    ///     "Upper surface",
    /// )]));
    /// ```
    pub fn entries(&mut self, entries: LegendEntries) -> &mut Self {
        self.panel().entries = entries;
        self
    }

    /// Appends an item to the explicit list of entries of the panel, starting an empty
    /// list if the entries are automatic.
    ///
    /// The item is either a [`LegendItem`] or the identifier of an artist, which lists
    /// that artist, labelled by its display name, beside the sample drawn for it. The
    /// artist may belong to any axes of the figure. The item also stands for every artist
    /// of the axes that name the panel whose display name and look match it, so that
    /// clicking it in the viewer hides or shows a series drawn in several axes at once.
    ///
    /// Items are listed in the order they are appended, whatever the order in which their
    /// artists are drawn, so a reference series drawn last, on top of the others, can be
    /// listed first.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(0.0, 1.0, 11);
    /// let mut fig = Figure::new().tiles(2, 1);
    /// let mut ax = fig.axes(0, 0);
    /// let member = ax.plot(&x, &x).display_name("Member").id();
    /// let reference = ax.plot(&x, &x).display_name("Reference").id();
    /// fig.panel_legend(1, 0).item(reference).item(member);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn item(&mut self, item: impl Into<LegendItem>) -> &mut Self {
        let item = item.into();
        let panel = self.panel();
        match &mut panel.entries {
            LegendEntries::Explicit(items) => items.push(item),
            LegendEntries::Auto => panel.entries = LegendEntries::Explicit(vec![item]),
        }
        self
    }

    /// Appends an item that lists an artist on a row of its own, spanning every column,
    /// to the explicit list of entries of the panel, as [`item`](PanelLegendMut::item)
    /// does. The label of the item does not set the width of any column.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(0.0, 1.0, 11);
    /// let mut fig = Figure::new().tiles(2, 1);
    /// let mut ax = fig.axes(0, 0);
    /// let small = ax.plot(&x, &x).display_name("Small").id();
    /// let large = ax.plot(&x, &x).display_name("Large").id();
    /// let base = ax.plot(&x, &x).display_name("Base case, drawn on top").id();
    /// fig.panel_legend(1, 0)
    ///     .full_row(base)
    ///     .item(small)
    ///     .item(large)
    ///     .columns(LegendEntryColumns::Count(2));
    /// ```
    pub fn full_row(&mut self, artist: NodeId) -> &mut Self {
        self.item(LegendItem {
            full_row: true,
            ..LegendItem::artist(artist)
        })
    }

    /// Appends an item that shows a sample and a label and stands for no artist to the
    /// explicit list of entries of the panel, as [`item`](PanelLegendMut::item) does.
    ///
    /// Such an item explains an encoding that many artists share, such as a marker shape
    /// that stands for a kind of point whatever its colour. A panel that no axes names
    /// may list such items alone, as a second key.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let grey = Color::rgb(0.45, 0.45, 0.45);
    /// let mut fig = Figure::new().tiles(2, 1);
    /// fig.panel_legend(1, 0)
    ///     .custom_item(LegendSample::marker(Marker::Square, grey), "Transition")
    ///     .custom_item(LegendSample::marker(Marker::TriangleUp, grey), "Separation")
    ///     .custom_item(LegendSample::swatch(Color::rgb(0.9, 0.6, 0.0)), "Upper surface");
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn custom_item(&mut self, sample: LegendSample, label: impl Into<Text>) -> &mut Self {
        self.item(LegendItem::custom(sample, label))
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
