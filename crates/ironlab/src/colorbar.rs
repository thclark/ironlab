//! The handles to a colourbar held by a panel and to a colourbar inside an axes.

use ironlab_ir::{
    ColorbarOrientation, ColorbarTitlePlacement, InsideColorbar, NodeId, PanelColorbar,
    PanelContent,
};

/// A handle to the colourbar held by one panel of a figure, returned by
/// [`PanelMut::colorbar`](crate::PanelMut::colorbar), through which its orientation, its
/// length and the placement of its scale's title are set.
///
/// The bar shows a colour scale along the cells of the panel, inset by the aligned margins
/// of the rows or columns it spans, so that a vertical bar spanning a row or column of axes
/// is exactly as long as their plot boxes and a horizontal one is half as long. Its colours, limits, ticks, title and out-of-range
/// keys belong to the scale (see [`ColorScaleMut`](crate::ColorScaleMut)), so every bar
/// that shows one scale agrees with every other and with the artists drawn through it.
///
/// The handle borrows the figure mutably, so it is short-lived. Its
/// [`id`](PanelColorbarMut::id) is the identifier of the panel.
///
/// ```
/// use ironlab::prelude::*;
///
/// let field = Matrix::from_fn(8, 8, |row, col| (row + col) as f64);
/// let mut fig = Figure::new().tiles(2, 3).fit_row(1);
/// let shared = fig.color_scale().limits(0.0, 14.0).title("Temperature (K)").id();
/// for col in 0..3 {
///     fig.axes(0, col).scalar_image(&field).color_scale(shared);
/// }
/// fig.panel_span(1, 0, 1, 3)
///     .colorbar(shared)
///     .orientation(ColorbarOrientation::Horizontal);
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct PanelColorbarMut<'f> {
    fig: &'f mut ironlab_ir::Figure,
    id: NodeId,
}

impl<'f> PanelColorbarMut<'f> {
    /// Creates a handle to the colourbar of the panel with the given identifier, which must
    /// be a panel of the figure that holds a colourbar.
    pub(crate) fn new(fig: &'f mut ironlab_ir::Figure, id: NodeId) -> Self {
        Self { fig, id }
    }
}

impl PanelColorbarMut<'_> {
    /// Returns the node identifier of the panel that holds the colourbar.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets whether the bar stands upright along the rows of the panel (the default), with
    /// its tick labels on the right, or lies flat along its columns, with its tick labels
    /// below.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(2, 1).fit_row(1);
    /// let s = fig.color_scale().id();
    /// fig.panel(1, 0)
    ///     .colorbar(s)
    ///     .orientation(ColorbarOrientation::Horizontal);
    /// ```
    pub fn orientation(&mut self, orientation: ColorbarOrientation) -> &mut Self {
        self.colorbar().orientation = orientation;
        self
    }

    /// Sets the length of the bar, its out-of-range keys included, as a fraction of the
    /// span of the plot boxes beside it. The default is automatic: the whole span for a
    /// vertical bar and half of it for a horizontal one. A shorter bar is placed along the
    /// span by the panel's [`align`](crate::PanelMut::align): a location that names an end
    /// of the span pushes it there, and any other centres it. A length that is not a finite
    /// number greater than 0 and at most 1 is reported by
    /// [`Figure::validate`](crate::Figure::validate).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new().tiles(2, 1).fit_row(1);
    /// let s = fig.color_scale().id();
    /// fig.panel(1, 0)
    ///     .colorbar(s)
    ///     .orientation(ColorbarOrientation::Horizontal)
    ///     .length(0.8);
    /// ```
    pub fn length(&mut self, fraction: f64) -> &mut Self {
        self.colorbar().length = Some(fraction);
        self
    }

    /// Sets where the title of the scale is drawn beside the bar: along it, beyond its tick
    /// labels (the default), reading from bottom to top beside a vertical bar; or above it,
    /// as an upright heading.
    pub fn title_placement(&mut self, placement: ColorbarTitlePlacement) -> &mut Self {
        self.colorbar().title_placement = placement;
        self
    }

    /// Returns the colourbar of the panel, which exists because the handle is only created
    /// for a panel of the figure that holds a colourbar.
    fn colorbar(&mut self) -> &mut PanelColorbar {
        let panel = self
            .fig
            .panel_mut(self.id)
            .expect("a panel colourbar handle names a panel of the figure");
        match &mut panel.content {
            PanelContent::Colorbar(colorbar) => colorbar,
            PanelContent::Legend(_) => {
                unreachable!("a panel colourbar handle names a panel that holds a colourbar")
            }
        }
    }
}

/// A handle to the colourbar drawn inside the plot area of one axes, returned by
/// [`AxesMut::colorbar`](crate::AxesMut::colorbar), through which its length and the
/// placement of its scale's title are set.
///
/// The bar lies along one edge of the plot area, on a boxed backdrop as an inside legend
/// does, over the artists. It takes no margin, so the plot area keeps its size whether or
/// not the bar is shown; a bar that needs room of its own belongs in a panel.
///
/// The handle borrows the figure mutably, so it is short-lived.
///
/// ```
/// use ironlab::prelude::*;
///
/// let field = Matrix::from_fn(8, 8, |row, col| (row + col) as f64);
/// let mut fig = Figure::new();
/// let mut ax = fig.axes(0, 0);
/// ax.scalar_image(&field);
/// let s = ax.color_scale().title("Depth (m)").id();
/// ax.colorbar(s, InsideColorbarSide::East).length(0.6);
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct InsideColorbarMut<'f> {
    fig: &'f mut ironlab_ir::Figure,
    id: NodeId,
}

impl<'f> InsideColorbarMut<'f> {
    /// Creates a handle to the inside colourbar of the axes with the given identifier,
    /// which must be an axes of the figure that shows an inside colourbar.
    pub(crate) fn new(fig: &'f mut ironlab_ir::Figure, id: NodeId) -> Self {
        Self { fig, id }
    }
}

impl InsideColorbarMut<'_> {
    /// Returns the node identifier of the axes that shows the colourbar.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets the length of the bar, together with any out-of-range keys it shows, as a
    /// fraction of the edge it lies along; the bar is centred on the edge. The default is
    /// 0.8. A length that is not a finite number greater than 0 and at most 1 is reported
    /// by [`Figure::validate`](crate::Figure::validate).
    pub fn length(&mut self, length: f64) -> &mut Self {
        self.colorbar().length = length;
        self
    }

    /// Sets where the title of the scale is drawn beside the bar, within its box: along it,
    /// beyond its tick labels toward the interior of the plot area (the default); or as an
    /// upright heading, above a vertical bar and on the side of a horizontal bar away from
    /// its tick labels.
    pub fn title_placement(&mut self, placement: ColorbarTitlePlacement) -> &mut Self {
        self.colorbar().title_placement = placement;
        self
    }

    /// Returns the inside colourbar of the axes, which exists because the handle is only
    /// created for an axes of the figure that shows one.
    fn colorbar(&mut self) -> &mut InsideColorbar {
        self.fig
            .axes_mut(self.id)
            .expect("an inside colourbar handle names an axes of the figure")
            .colorbar
            .as_mut()
            .expect("an inside colourbar handle names an axes that shows a colourbar")
    }
}
