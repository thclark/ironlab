//! The handles to a panel and to the legend it holds.

use ironlab_ir::{
    ColorbarOrientation, Legend, LegendEntries, LegendEntryColumns, LegendEntryOrder, LegendItem,
    LegendSample, NodeId, Panel, PanelColorbar, PanelContent, PanelLegend, PanelLocation, Text,
};

use crate::colorbar::PanelColorbarMut;

/// A handle to one panel of a figure, returned by [`Figure::panel`](crate::Figure::panel)
/// and [`Figure::panel_span`](crate::Figure::panel_span), through which its placement and
/// its content are set.
///
/// A panel occupies cells of the figure's tile layout, as an axes does, and holds one key:
/// a legend, set with [`legend`](PanelMut::legend), or a colourbar, set with
/// [`colorbar`](PanelMut::colorbar). A panel created without a call to either holds a
/// legend with the default settings.
///
/// Like [`AxesMut`](crate::AxesMut), the handle borrows the figure mutably, so it is
/// short-lived: keep its [`id`](PanelMut::id) to point axes at the panel later.
///
/// ```
/// use ironlab::prelude::*;
///
/// let mut fig = Figure::new().tiles(1, 2);
/// let key = fig
///     .panel(0, 1)
///     .align(PanelLocation::NorthWest)
///     .legend()
///     .boxed(false)
///     .id();
/// assert_eq!(fig.panel(0, 1).id(), key);
/// ```
#[derive(Debug)]
pub struct PanelMut<'f> {
    pub(crate) fig: &'f mut ironlab_ir::Figure,
    pub(crate) id: NodeId,
}

impl<'f> PanelMut<'f> {
    /// Creates a handle to the panel with the given identifier, which must be a panel of
    /// the figure.
    pub(crate) fn new(fig: &'f mut ironlab_ir::Figure, id: NodeId) -> Self {
        Self { fig, id }
    }
}

impl PanelMut<'_> {
    /// Returns the node identifier of the panel, which axes name to list their entries
    /// in it.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets where the content is placed within the panel. The default is the centre.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(1, 2);
    /// fig.panel(0, 1).align(PanelLocation::NorthWest);
    /// ```
    pub fn align(&mut self, location: PanelLocation) -> &mut Self {
        self.panel().align = location;
        self
    }

    /// Makes the content of the panel a legend, keeping the legend the panel already
    /// holds, and returns the handle through which the legend is set. A colourbar the panel
    /// holds is replaced by a legend with the default settings.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(2, 1);
    /// fig.panel(1, 0).legend().columns(LegendEntryColumns::Count(4));
    /// ```
    pub fn legend(&mut self) -> PanelLegendMut<'_> {
        let panel = self.panel();
        if !matches!(panel.content, PanelContent::Legend(_)) {
            panel.content = PanelContent::Legend(PanelLegend::default());
        }
        PanelLegendMut::new(self.fig, self.id)
    }

    /// Makes the content of the panel a colourbar that shows the colour scale with the
    /// given identifier (MATLAB's and matplotlib's `colorbar`), and returns the handle
    /// through which the colourbar is set.
    ///
    /// A colourbar the panel already holds keeps its orientation and takes the new scale; a
    /// legend the panel holds is replaced by a vertical colourbar. The bar stretches along
    /// the cells of the panel, so a panel spanning a row of axes, in a row whose size fits
    /// its content, shows one scale for the whole row. An identifier that names no colour
    /// scale is reported by [`Figure::validate`](crate::Figure::validate).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let field = Matrix::from_fn(8, 8, |row, col| (row * col) as f64);
    /// let mut fig = Figure::new().tiles(1, 2).fit_col(1);
    /// fig.axes(0, 0).scalar_image(&field);
    /// let s = fig.axes(0, 0).color_scale().title("Intensity").id();
    /// fig.panel(0, 1).colorbar(s);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn colorbar(&mut self, scale: NodeId) -> PanelColorbarMut<'_> {
        let panel = self.panel();
        match &mut panel.content {
            PanelContent::Colorbar(colorbar) => colorbar.scale = scale,
            PanelContent::Legend(_) => {
                panel.content = PanelContent::Colorbar(PanelColorbar {
                    scale,
                    orientation: ColorbarOrientation::default(),
                });
            }
        }
        PanelColorbarMut::new(self.fig, self.id)
    }

    /// Returns the panel, which exists because the handle is only created for a panel of
    /// the figure.
    fn panel(&mut self) -> &mut Panel {
        self.fig
            .panel_mut(self.id)
            .expect("a panel handle names a panel of the figure")
    }
}

/// A handle to the legend held by one panel of a figure, returned by
/// [`PanelMut::legend`], through which its properties are set.
///
/// The legend lists the entries of every axes whose legend names the panel (see
/// [`AxesMut::legend_in`](crate::AxesMut::legend_in)). Entries of different axes with the
/// same label and the same look are listed once.
///
/// The handle borrows the figure mutably, so it is short-lived: keep its
/// [`id`](PanelLegendMut::id), which is the identifier of the panel, to point axes at the
/// panel later. Property setters return the handle so that they can be chained.
///
/// ```
/// use ironlab::prelude::*;
///
/// let x = linspace(0.0, 1.0, 11);
/// let mut fig = Figure::new().tiles(2, 2).row_weights([3.0, 1.0]);
/// let key = fig.panel_span(1, 0, 1, 2).legend().id();
/// for col in 0..2 {
///     let mut ax = fig.axes(0, col);
///     ax.plot(&x, &x).label("Linear");
///     ax.legend_in(key);
/// }
/// ```
#[derive(Debug)]
pub struct PanelLegendMut<'f> {
    pub(crate) fig: &'f mut ironlab_ir::Figure,
    pub(crate) id: NodeId,
}

impl<'f> PanelLegendMut<'f> {
    /// Creates a handle to the legend of the panel with the given identifier, which must
    /// be a panel of the figure that holds a legend.
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

    /// Sets whether the area of the panel is outlined by a border, which is the default.
    /// The border is drawn as the box of an axes is drawn, and continues the edges of the
    /// boxes of the axes in the panel's rows and columns, because the area of the panel is
    /// aligned with their plot areas.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(1, 2);
    /// fig.panel(0, 1).legend().boxed(false);
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
    /// fig.panel(1, 0).legend().columns(LegendEntryColumns::Count(4));
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
    /// fig.panel(1, 0)
    ///     .legend()
    ///     .columns(LegendEntryColumns::Count(3))
    ///     .order(LegendEntryOrder::RowMajor);
    /// ```
    pub fn order(&mut self, order: LegendEntryOrder) -> &mut Self {
        self.panel().layout.order = order;
        self
    }

    /// Sets which entries the panel lists, and in what order.
    ///
    /// `LegendEntries::Auto`, the default, lists every artist with a label of the
    /// axes that name the panel, merged by label and look, in the reading order of the
    /// layout. An explicit list names exactly the items to list, in their order. The
    /// item setters [`item`](PanelLegendMut::item),
    /// [`full_row`](PanelLegendMut::full_row) and
    /// [`custom_item`](PanelLegendMut::custom_item) build such a list one item at a time.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(2, 1);
    /// fig.panel(1, 0).legend().entries(LegendEntries::Explicit(vec![LegendItem::custom(
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
    /// that artist, labelled by its label beside the sample drawn for it. The
    /// artist may belong to any axes of the figure. The item also stands for every artist
    /// of the axes that name the panel whose label and look match it, so that
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
    /// let member = ax.plot(&x, &x).label("Member").id();
    /// let reference = ax.plot(&x, &x).label("Reference").id();
    /// fig.panel(1, 0).legend().item(reference).item(member);
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
    /// let small = ax.plot(&x, &x).label("Small").id();
    /// let large = ax.plot(&x, &x).label("Large").id();
    /// let base = ax.plot(&x, &x).label("Base case, drawn on top").id();
    /// fig.panel(1, 0)
    ///     .legend()
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
    /// fig.panel(1, 0)
    ///     .legend()
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
    ///     fig.axes(0, col).plot(&x, &x).label("Linear");
    /// }
    /// fig.panel(0, 2).legend().collect_all();
    /// assert!(fig.validate().warnings.is_empty());
    /// ```
    pub fn collect_all(&mut self) -> &mut Self {
        for axes in &mut self.fig.axes {
            axes.legend = Some(Legend::Panel(self.id));
        }
        self
    }

    /// Returns the legend of the panel, which exists because the handle is only created
    /// for a panel of the figure that holds a legend.
    fn panel(&mut self) -> &mut PanelLegend {
        let panel = self
            .fig
            .panel_mut(self.id)
            .expect("a panel legend handle names a panel of the figure");
        match &mut panel.content {
            PanelContent::Legend(legend) => legend,
            PanelContent::Colorbar(_) => {
                unreachable!("a panel legend handle names a panel that holds a legend")
            }
        }
    }
}
