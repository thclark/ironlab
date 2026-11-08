//! Colour scales: the handle to a colour scale of the figure, the default scale of an
//! axes, and the sampling of a colourmap into a palette.

use ironlab_ir::{
    Artist, CategoricalColourScale, Colour, ColourCategory, ColourScale, ColourScaleKey,
    ColourScaleKind, ColourScaleLevels, ColourScaleNorm, ColourScaleOutOfRange, ColourmapName,
    ContinuousColourScale, DiscreteColourScale, IndexedColourScale, Limits, NodeId, OutOfRange,
    Text, TickLabels, Ticks,
};
use ironlab_scene::maths::colourmap::{self, Lut};

/// A handle to one colour scale of a figure, through which its colourmap, limits, norm,
/// bands and out-of-range policies are set.
///
/// A colour scale turns the numbers of colour-mapped artists into colours. It is a node
/// of the figure that artists name by its [`id`](ColourScaleMut::id), so one scale may
/// colour artists in several axes, which then share its limits and its colours. Scales
/// are created with [`Figure::colour_scale`](crate::Figure::colour_scale) (continuous),
/// [`Figure::indexed_scale`](crate::Figure::indexed_scale) and
/// [`Figure::categorical_scale`](crate::Figure::categorical_scale), and the scale that
/// the colour-mapped artists of an axes share by default is returned by
/// [`AxesMut::colour_scale`](crate::AxesMut::colour_scale).
///
/// The kind of a scale decides which setters have an effect. A continuous or discrete
/// scale has a colourmap, limits, a norm and the `below` and `above` policies; an indexed
/// or categorical scale has its entries and the `outside` policy. A setter that does not
/// apply to the kind of the scale leaves the scale as it is.
///
/// Like the other handles, this one borrows the figure mutably, so it is short-lived:
/// keep its identifier to give the scale to artists.
///
/// ```
/// use ironlab::prelude::*;
///
/// let x = linspace(-1.0, 1.0, 21);
/// let z = Matrix::from_fn(x.len(), x.len(), |row, col| x[col] * x[row]);
///
/// let mut fig = Figure::new().tiles(1, 2);
/// let shared = fig
///     .colour_scale()
///     .colourmap(Colourmap::Coolwarm)
///     .limits(-1.0, 1.0)
///     .title("Product")
///     .id();
/// fig.axes(0, 0).surface(&x, &x, &z).colour_scale(shared);
/// fig.axes(0, 1).contourf(&x, &x, &z).colour_scale(shared);
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct ColourScaleMut<'f> {
    fig: &'f mut ironlab_ir::Figure,
    id: NodeId,
}

impl<'f> ColourScaleMut<'f> {
    /// Creates a handle to the colour scale with the given identifier, which must be a
    /// colour scale of the figure.
    pub(crate) fn new(fig: &'f mut ironlab_ir::Figure, id: NodeId) -> Self {
        Self { fig, id }
    }

    /// Adds a colour scale of the given kind to the figure and returns a handle to it.
    pub(crate) fn add(fig: &'f mut ironlab_ir::Figure, kind: ColourScaleKind) -> Self {
        let id = fig.alloc_node_id();
        fig.colour_scales.push(ColourScale {
            id,
            kind,
            ..ColourScale::default()
        });
        Self::new(fig, id)
    }
}

impl ColourScaleMut<'_> {
    /// Returns the node identifier of the scale, which the `colour_scale` setters of
    /// artist handles accept.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Sets the title of the scale, which every colourbar that shows the scale draws
    /// beside its bar: along it, beyond its tick labels, or above it as a heading, as the
    /// bar's title placement says.
    pub fn title(&mut self, title: impl Into<Text>) -> &mut Self {
        self.scale().title = Some(title.into());
        self
    }

    /// Sets the colourmap of a continuous or discrete scale (MATLAB's `colormap`): a
    /// bundled [`Colourmap`](crate::Colourmap), or an [`ir::Colourmap`](crate::ir::Colourmap)
    /// that reverses a bundled colourmap or lists colours of its own.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// fig.colour_scale().colourmap(Colourmap::Magma);
    /// fig.colour_scale().colourmap(ironlab::ir::Colourmap::Listed {
    ///     colours: vec![Colour::rgb(0.0, 0.2, 0.6), Colour::WHITE, Colour::rgb(0.7, 0.1, 0.1)],
    /// });
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn colourmap(&mut self, colourmap: impl Into<ironlab_ir::Colourmap>) -> &mut Self {
        let colourmap = colourmap.into();
        match &mut self.scale().kind {
            ColourScaleKind::Continuous(scale) => scale.colourmap = colourmap,
            ColourScaleKind::Discrete(scale) => scale.colourmap = colourmap,
            ColourScaleKind::Indexed(_) | ColourScaleKind::Categorical(_) => {}
        }
        self
    }

    /// Fixes the values mapped to the first and last colours of a continuous scale
    /// (MATLAB's `clim`), or the range that the automatic band edges of a discrete scale
    /// span.
    ///
    /// The limits of a new scale are automatic: the exact range of the values of every
    /// artist that names the scale, in every axes. On a discrete scale with explicit
    /// edges, the limits replace the edges by automatic edges of the same number of bands
    /// within them. Limits that are not finite and increasing, or not positive under a
    /// logarithmic norm, are reported by [`Figure::validate`](crate::Figure::validate).
    pub fn limits(&mut self, min: f64, max: f64) -> &mut Self {
        let limits = Limits::Manual { min, max };
        match &mut self.scale().kind {
            ColourScaleKind::Continuous(scale) => scale.limits = limits,
            ColourScaleKind::Discrete(scale) => {
                let count = match &scale.levels {
                    ColourScaleLevels::Auto { count, .. } => *count,
                    ColourScaleLevels::Explicit { values } => band_count(values),
                };
                scale.levels = ColourScaleLevels::Auto { count, limits };
            }
            ColourScaleKind::Indexed(_) | ColourScaleKind::Categorical(_) => {}
        }
        self
    }

    /// Sets whether a continuous or discrete scale places values in linear space (the
    /// default) or logarithmic space.
    ///
    /// Under a logarithmic norm, automatic limits range over the positive values only,
    /// and a value that is not positive lies below the range.
    pub fn norm(&mut self, norm: ColourScaleNorm) -> &mut Self {
        match &mut self.scale().kind {
            ColourScaleKind::Continuous(scale) => scale.norm = norm,
            ColourScaleKind::Discrete(scale) => scale.norm = norm,
            ColourScaleKind::Indexed(_) | ColourScaleKind::Categorical(_) => {}
        }
        self
    }

    /// Makes the scale discrete, with bands between the given edges, each drawn in one
    /// colour of the colourmap.
    ///
    /// A continuous scale keeps its colourmap, norm and out-of-range policies; a discrete
    /// scale keeps everything but its edges; and an indexed or categorical scale becomes
    /// a discrete scale with the defaults of that kind. A filled contour that names the
    /// scale, and has no levels of its own, fills exactly these bands (MATLAB's
    /// `contourf` with a vector of levels and a matching `colourmap`). Edges that are not
    /// at least two finite, strictly increasing values are reported by
    /// [`Figure::validate`](crate::Figure::validate).
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let x = linspace(0.0, 1.0, 21);
    /// let z = Matrix::from_fn(x.len(), x.len(), |row, col| x[col] + x[row]);
    ///
    /// let mut fig = Figure::new();
    /// let bands = fig.colour_scale().discrete([0.0, 0.5, 1.0, 1.5, 2.0]).id();
    /// fig.axes(0, 0).contourf(&x, &x, &z).colour_scale(bands);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn discrete(&mut self, edges: impl AsRef<[f64]>) -> &mut Self {
        let levels = ColourScaleLevels::Explicit {
            values: edges.as_ref().to_vec(),
        };
        self.make_discrete(|_| levels);
        self
    }

    /// Makes the scale discrete, with about `count` bands between nice edges chosen
    /// within its limits.
    ///
    /// The scale keeps what [`discrete`](Self::discrete) keeps, and also its limits: a
    /// continuous scale's limits become the limits of the edges, and a discrete scale
    /// with explicit edges keeps its first and last edges as the limits.
    pub fn bands(&mut self, count: u32) -> &mut Self {
        self.make_discrete(|limits| ColourScaleLevels::Auto { count, limits });
        self
    }

    /// Sets what is drawn for a value below the range of a continuous or discrete scale:
    /// below its lower limit or first edge, or not positive under a logarithmic norm.
    ///
    /// The policy is [`OutOfRange::Clamp`] by default, which draws the first colour of
    /// the colourmap. A [`Colour`] draws that colour, [`OutOfRange::Transparent`] draws
    /// nothing, and [`OutOfRange::Strict`] makes such a value a validation error.
    pub fn below(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        let policy = policy.into();
        match &mut self.scale().kind {
            ColourScaleKind::Continuous(scale) => scale.below.policy = policy,
            ColourScaleKind::Discrete(scale) => scale.below.policy = policy,
            ColourScaleKind::Indexed(_) | ColourScaleKind::Categorical(_) => {}
        }
        self
    }

    /// Sets what is drawn for a value above the range of a continuous or discrete scale,
    /// as [`below`](Self::below) does for values below it; a clamp draws the last colour
    /// of the colourmap.
    pub fn above(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        let policy = policy.into();
        match &mut self.scale().kind {
            ColourScaleKind::Continuous(scale) => scale.above.policy = policy,
            ColourScaleKind::Discrete(scale) => scale.above.policy = policy,
            ColourScaleKind::Indexed(_) | ColourScaleKind::Categorical(_) => {}
        }
        self
    }

    /// Sets what is drawn for a value that names no entry of an indexed or categorical
    /// scale: a negative value, a value not less than the number of entries, or a value
    /// that is not a whole number.
    ///
    /// The policy is [`OutOfRange::Strict`] by default, so such a value is a validation
    /// error. [`OutOfRange::Clamp`] truncates the value towards zero and clamps it to the
    /// first or last entry, which suits indices computed as floating-point numbers.
    pub fn outside(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        let policy = policy.into();
        match &mut self.scale().kind {
            ColourScaleKind::Indexed(scale) => scale.outside.policy = policy,
            ColourScaleKind::Categorical(scale) => scale.outside.policy = policy,
            ColourScaleKind::Continuous(_) | ColourScaleKind::Discrete(_) => {}
        }
        self
    }

    /// Sets what is drawn for a value that is NaN or infinite, on a scale of any kind.
    ///
    /// The policy is [`OutOfRange::Transparent`] by default, because a NaN is the usual
    /// way of saying that nothing is drawn at a point. A clamp also draws nothing, because
    /// a non-finite value has no nearest end of the scale.
    pub fn non_finite(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        self.scale().non_finite.policy = policy.into();
        self
    }

    /// Places the ticks of every colourbar that shows the scale at the given values
    /// (MATLAB's `colorbar('Ticks', …)`), on a continuous, discrete or indexed scale. The
    /// ticks of an indexed scale are entry indices, and each is drawn at the centre of its
    /// entry.
    ///
    /// The ticks are labelled with their values unless
    /// [`tick_labels`](Self::tick_labels) names them or
    /// [`tick_labels_hidden`](Self::tick_labels_hidden) hides them. A categorical scale,
    /// which labels each category itself, is left as it is.
    pub fn ticks(&mut self, values: impl AsRef<[f64]>) -> &mut Self {
        let values = values.as_ref().to_vec();
        self.with_ticks(|ticks, _| *ticks = Ticks::Values(values))
    }

    /// Places a tick of every colourbar that shows the scale at every multiple of `step`
    /// within the range of the scale, as an axis's
    /// [`xtick_step`](crate::AxesMut::xtick_step) does. A categorical scale is left as it
    /// is.
    pub fn tick_step(&mut self, step: f64) -> &mut Self {
        self.with_ticks(|ticks, _| *ticks = Ticks::Step { step, origin: 0.0 })
    }

    /// Places the ticks of every colourbar that shows the scale at the given values and
    /// labels each with its text, in one call, because a label means nothing apart from its
    /// value (MATLAB's `Ticks` and `TickLabels` together).
    ///
    /// A plain string is interpreted as LaTeX, as for every other text of the figure. A
    /// categorical scale is left as it is.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// fig.indexed_scale(Colourmap::Grey.sample(256))
    ///     .tick_labels([(0, "water"), (128, "land"), (255, "ice")]);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn tick_labels<V: Into<f64>, T: Into<Text>>(
        &mut self,
        labels: impl IntoIterator<Item = (V, T)>,
    ) -> &mut Self {
        let (values, labels): (Vec<f64>, Vec<Text>) = labels
            .into_iter()
            .map(|(value, label)| (value.into(), label.into()))
            .unzip();
        self.with_ticks(|ticks, tick_labels| {
            *ticks = Ticks::Values(values);
            *tick_labels = TickLabels::Text(labels);
        })
    }

    /// Hides the tick labels of every colourbar that shows the scale, keeping its ticks. A
    /// categorical scale is left as it is.
    pub fn tick_labels_hidden(&mut self) -> &mut Self {
        self.with_ticks(|_, tick_labels| *tick_labels = TickLabels::Hidden)
    }

    /// Shows the key of one out-of-range category, with its default label, on every
    /// colourbar that shows the scale: a square of the colour drawn for such values,
    /// separate from the bar. Keys are off by default.
    ///
    /// The default labels are `> upper` for values above the range and `< lower` for
    /// values below it, with the limits formatted as the ticks are, `NaN / Inf` for values
    /// that are not finite, and `Out of range` for values that name no entry. The policy of
    /// the category, which [`below`](Self::below), [`above`](Self::above),
    /// [`outside`](Self::outside) and [`non_finite`](Self::non_finite) set, is kept. A key
    /// that the kind of the scale does not have (below or above on an indexed or
    /// categorical scale, outside on a continuous or discrete one) leaves the scale as it
    /// is.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// fig.colour_scale()
    ///     .limits(0.0, 40.0)
    ///     .above(Colour::rgb(1.0, 0.0, 1.0))
    ///     .show_key(ColourScaleKey::Above);
    /// ```
    pub fn show_key(&mut self, key: ColourScaleKey) -> &mut Self {
        if let Some(category) = self.key(key) {
            category.show_key = true;
        }
        self
    }

    /// Shows the key of one out-of-range category with the given label, as
    /// [`show_key`](Self::show_key) does with the default label.
    pub fn key_label(&mut self, key: ColourScaleKey, label: impl Into<Text>) -> &mut Self {
        if let Some(category) = self.key(key) {
            category.show_key = true;
            category.label = Some(label.into());
        }
        self
    }

    /// Applies `change` to the ticks and tick labels of a continuous, discrete or indexed
    /// scale, leaving a categorical scale as it is.
    fn with_ticks(&mut self, change: impl FnOnce(&mut Ticks, &mut TickLabels)) -> &mut Self {
        match &mut self.scale().kind {
            ColourScaleKind::Continuous(s) => change(&mut s.ticks, &mut s.tick_labels),
            ColourScaleKind::Discrete(s) => change(&mut s.ticks, &mut s.tick_labels),
            ColourScaleKind::Indexed(s) => change(&mut s.ticks, &mut s.tick_labels),
            ColourScaleKind::Categorical(_) => {}
        }
        self
    }

    /// Returns the out-of-range category behind a key, or `None` when the kind of the scale
    /// does not have the key, as [`ColourScale::key`] does.
    fn key(&mut self, key: ColourScaleKey) -> Option<&mut ColourScaleOutOfRange> {
        let scale = self.scale();
        match (&mut scale.kind, key) {
            (_, ColourScaleKey::NonFinite) => Some(&mut scale.non_finite),
            (ColourScaleKind::Continuous(s), ColourScaleKey::Below) => Some(&mut s.below),
            (ColourScaleKind::Continuous(s), ColourScaleKey::Above) => Some(&mut s.above),
            (ColourScaleKind::Discrete(s), ColourScaleKey::Below) => Some(&mut s.below),
            (ColourScaleKind::Discrete(s), ColourScaleKey::Above) => Some(&mut s.above),
            (ColourScaleKind::Indexed(s), ColourScaleKey::Outside) => Some(&mut s.outside),
            (ColourScaleKind::Categorical(s), ColourScaleKey::Outside) => Some(&mut s.outside),
            _ => None,
        }
    }

    /// Returns the scale in the figure IR.
    fn scale(&mut self) -> &mut ColourScale {
        self.fig
            .colour_scale_mut(self.id)
            .expect("a colour scale handle always refers to a colour scale of its figure")
    }

    /// Makes the scale discrete with the levels that `levels` builds from the limits the
    /// scale had, keeping the colourmap, norm and policies of a continuous or discrete
    /// scale.
    fn make_discrete(&mut self, levels: impl FnOnce(Limits) -> ColourScaleLevels) {
        let scale = self.scale();
        let discrete = match std::mem::take(&mut scale.kind) {
            ColourScaleKind::Continuous(ContinuousColourScale {
                colourmap,
                limits,
                norm,
                below,
                above,
                ticks,
                tick_labels,
            }) => DiscreteColourScale {
                colourmap,
                levels: levels(limits),
                norm,
                below,
                above,
                ticks,
                tick_labels,
            },
            ColourScaleKind::Discrete(discrete) => {
                let limits = match &discrete.levels {
                    ColourScaleLevels::Auto { limits, .. } => *limits,
                    ColourScaleLevels::Explicit { values } => {
                        match (values.first(), values.last()) {
                            (Some(&min), Some(&max)) => Limits::Manual { min, max },
                            _ => Limits::Auto,
                        }
                    }
                };
                DiscreteColourScale {
                    levels: levels(limits),
                    ..discrete
                }
            }
            ColourScaleKind::Indexed(_) | ColourScaleKind::Categorical(_) => DiscreteColourScale {
                levels: levels(Limits::Auto),
                ..DiscreteColourScale::default()
            },
        };
        scale.kind = ColourScaleKind::Discrete(discrete);
    }
}

/// Returns the number of bands between explicit edges, at least one.
fn band_count(edges: &[f64]) -> u32 {
    u32::try_from(edges.len().saturating_sub(1).max(1)).unwrap_or(u32::MAX)
}

/// Returns a continuous scale kind with the defaults of its kind.
pub(crate) fn continuous() -> ColourScaleKind {
    ColourScaleKind::Continuous(ContinuousColourScale::default())
}

/// Returns an indexed scale kind with the given entries and the defaults of its kind.
pub(crate) fn indexed(colours: impl IntoIterator<Item = Colour>) -> ColourScaleKind {
    ColourScaleKind::Indexed(IndexedColourScale {
        colours: colours.into_iter().collect(),
        outside: ColourScaleOutOfRange::new(OutOfRange::Strict),
        ..IndexedColourScale::default()
    })
}

/// Returns a categorical scale kind with the given categories and the defaults of its
/// kind.
pub(crate) fn categorical<T: Into<Text>>(
    categories: impl IntoIterator<Item = (Colour, T)>,
) -> ColourScaleKind {
    ColourScaleKind::Categorical(CategoricalColourScale {
        categories: categories
            .into_iter()
            .map(|(colour, label)| ColourCategory {
                colour,
                label: label.into(),
            })
            .collect(),
        ..CategoricalColourScale::default()
    })
}

/// Returns the default colour scale of the axes with the given identifier: the colour
/// scale of the figure that the first artist of the axes, in artist order, to name one
/// names. Returns `None` when no artist of the axes names a colour scale of the figure.
pub(crate) fn default_scale(fig: &ironlab_ir::Figure, axes: NodeId) -> Option<NodeId> {
    fig.axes(axes)?
        .artists
        .iter()
        .filter_map(Artist::colour_scale)
        .find(|&scale| fig.colour_scale(scale).is_some())
}

/// Returns the default colour scale of an axes, creating a continuous scale with the
/// defaults of its kind when the axes has none, and whether it was created.
pub(crate) fn default_or_new(fig: &mut ironlab_ir::Figure, axes: NodeId) -> (NodeId, bool) {
    match default_scale(fig, axes) {
        Some(scale) => (scale, false),
        None => (ColourScaleMut::add(fig, continuous()).id(), true),
    }
}

/// Gives an artist a colour scale, deleting the scale it named before when the artist's
/// own plotting call created that scale and nothing else names it any more, and returns
/// the scale that the plotting call created and that still exists.
pub(crate) fn replace_scale(
    fig: &mut ironlab_ir::Figure,
    artist: NodeId,
    scale: NodeId,
    created: Option<NodeId>,
) -> Option<NodeId> {
    let previous = match fig.artist_mut(artist) {
        Some(Artist::ScalarImage(a)) => a.colour_scale.replace(scale),
        Some(Artist::Contour(a)) => a.colour_scale.replace(scale),
        Some(Artist::Surface(a)) => a.colour_scale.replace(scale),
        Some(Artist::Series(a)) => a.colour_scale.replace(scale),
        Some(Artist::Bar(a)) => a.colour_scale.replace(scale),
        Some(Artist::Cells(a)) => a.colour_scale.replace(scale),
        _ => unreachable!("only an artist that takes a colour scale is given one"),
    };
    let Some(created) = created.filter(|&created| Some(created) == previous && created != scale)
    else {
        return created;
    };
    let named = fig
        .axes
        .iter()
        .flat_map(|axes| &axes.artists)
        .any(|artist| artist.colour_scale() == Some(created));
    if named {
        Some(created)
    } else {
        fig.colour_scales.retain(|s| s.id != created);
        None
    }
}

/// The colours of a colourmap sampled at evenly spaced positions, the palette from which
/// an indexed scale is built.
///
/// The trait is implemented for a bundled [`Colourmap`](crate::Colourmap) and for an
/// [`ir::Colourmap`](crate::ir::Colourmap), and is in the prelude.
///
/// ```
/// use ironlab::prelude::*;
///
/// // The 256 entries of a grey palette, of which an 8-bit value takes its own entry.
/// let grey = Colourmap::Grey.sample(256);
/// assert_eq!(grey.len(), 256);
/// assert_eq!(grey[0], Colour::BLACK);
/// assert_eq!(grey[255], Colour::WHITE);
///
/// let mut fig = Figure::new();
/// let palette = fig.indexed_scale(grey).id();
/// let codes = ByteMatrix::from_fn(16, 16, |row, col| (row * 16 + col) as u8);
/// fig.axes(0, 0).scalar_image(&codes).colour_scale(palette);
/// assert!(fig.validate().is_valid());
/// ```
pub trait ColourmapSample {
    /// Returns `count` colours of the colourmap, from its start to its end at evenly
    /// spaced positions, as a continuous scale draws them; one colour is the colour at
    /// the start.
    ///
    /// A bundled colourmap has 256 entries, so `count` of 256 returns exactly those
    /// entries, in order. A listed colourmap is interpolated linearly in sRGB between its
    /// colours; one of fewer than two colours returns its colours repeated, or black when
    /// it has none.
    fn sample(&self, count: usize) -> Vec<Colour>;
}

impl ColourmapSample for ColourmapName {
    fn sample(&self, count: usize) -> Vec<Colour> {
        ironlab_ir::Colourmap::from(*self).sample(count)
    }
}

impl ColourmapSample for ironlab_ir::Colourmap {
    fn sample(&self, count: usize) -> Vec<Colour> {
        let position = |k: usize| {
            if count > 1 {
                k as f64 / (count - 1) as f64
            } else {
                0.0
            }
        };
        (0..count).map(|k| colour_at(self, position(k))).collect()
    }
}

/// Returns the colour of a colourmap at a position from 0 to 1, as the scene compiler
/// draws a continuous scale.
fn colour_at(colourmap: &ironlab_ir::Colourmap, t: f64) -> Colour {
    match colourmap {
        ironlab_ir::Colourmap::Named { name, reverse } => {
            // The entry is chosen as the scene compiler chooses it: each of the 256
            // entries takes an equal share of the positions, and a reversed colourmap
            // takes the entry at the mirrored index.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the position lies from 0 to 1, so the product lies from 0 to 256"
            )]
            let index = ((t.clamp(0.0, 1.0) * 256.0).floor() as usize).min(255);
            let index = if *reverse { 255 - index } else { index };
            let [r, g, b] = lut(*name)[index];
            Colour::rgb(
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
            )
        }
        ironlab_ir::Colourmap::Listed { colours } => match colours.len() {
            0 => Colour::BLACK,
            1 => colours[0],
            n => {
                let last = n - 1;
                let x = t.clamp(0.0, 1.0) * last as f64;
                // The position is at most `last`, so the floor is a valid index.
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "the position lies from 0 to the index of the last colour"
                )]
                let i = (x.floor() as usize).min(last - 1);
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "the fraction lies from 0 to 1"
                )]
                let f = (x - i as f64) as f32;
                let (a, b) = (colours[i], colours[i + 1]);
                let mix = |p: f32, q: f32| p + (q - p) * f;
                Colour::rgba(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b), mix(a.a, b.a))
            }
        },
    }
}

/// Returns the lookup table of a bundled colourmap.
fn lut(name: ColourmapName) -> &'static Lut {
    match name {
        ColourmapName::Viridis => &colourmap::VIRIDIS,
        ColourmapName::Cividis => &colourmap::CIVIDIS,
        ColourmapName::Magma => &colourmap::MAGMA,
        ColourmapName::Inferno => &colourmap::INFERNO,
        ColourmapName::Plasma => &colourmap::PLASMA,
        ColourmapName::Coolwarm => &colourmap::COOLWARM,
        ColourmapName::Grey => &colourmap::GREY,
    }
}
