//! Colour scales: the handle to a colour scale of the figure, the default scale of an
//! axes, and the sampling of a colormap into a palette.

use ironlab_ir::{
    Artist, CategoricalColorScale, Color, ColorCategory, ColorScale, ColorScaleKey, ColorScaleKind,
    ColorScaleLevels, ColorScaleNorm, ColorScaleOutOfRange, ColormapName, ContinuousColorScale,
    DiscreteColorScale, IndexedColorScale, Limits, NodeId, OutOfRange, Text, TickLabels, Ticks,
};
use ironlab_scene::maths::colormap::{self, Lut};

/// A handle to one colour scale of a figure, through which its colormap, limits, norm,
/// bands and out-of-range policies are set.
///
/// A colour scale turns the numbers of colour-mapped artists into colours. It is a node
/// of the figure that artists name by its [`id`](ColorScaleMut::id), so one scale may
/// colour artists in several axes, which then share its limits and its colours. Scales
/// are created with [`Figure::color_scale`](crate::Figure::color_scale) (continuous),
/// [`Figure::indexed_scale`](crate::Figure::indexed_scale) and
/// [`Figure::categorical_scale`](crate::Figure::categorical_scale), and the scale that
/// the colour-mapped artists of an axes share by default is returned by
/// [`AxesMut::color_scale`](crate::AxesMut::color_scale).
///
/// The kind of a scale decides which setters have an effect. A continuous or discrete
/// scale has a colormap, limits, a norm and the `below` and `above` policies; an indexed
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
///     .color_scale()
///     .colormap(Colormap::Coolwarm)
///     .limits(-1.0, 1.0)
///     .title("Product")
///     .id();
/// fig.axes(0, 0).surface(&x, &x, &z).color_scale(shared);
/// fig.axes(0, 1).contourf(&x, &x, &z).color_scale(shared);
/// assert!(fig.validate().is_valid());
/// ```
#[derive(Debug)]
pub struct ColorScaleMut<'f> {
    fig: &'f mut ironlab_ir::Figure,
    id: NodeId,
}

impl<'f> ColorScaleMut<'f> {
    /// Creates a handle to the colour scale with the given identifier, which must be a
    /// colour scale of the figure.
    pub(crate) fn new(fig: &'f mut ironlab_ir::Figure, id: NodeId) -> Self {
        Self { fig, id }
    }

    /// Adds a colour scale of the given kind to the figure and returns a handle to it.
    pub(crate) fn add(fig: &'f mut ironlab_ir::Figure, kind: ColorScaleKind) -> Self {
        let id = fig.alloc_node_id();
        fig.color_scales.push(ColorScale {
            id,
            kind,
            ..ColorScale::default()
        });
        Self::new(fig, id)
    }
}

impl ColorScaleMut<'_> {
    /// Returns the node identifier of the scale, which the `color_scale` setters of
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

    /// Sets the colormap of a continuous or discrete scale (MATLAB's `colormap`): a
    /// bundled [`Colormap`](crate::Colormap), or an [`ir::Colormap`](crate::ir::Colormap)
    /// that reverses a bundled colormap or lists colours of its own.
    ///
    /// ```
    /// use ironlab::prelude::*;
    ///
    /// let mut fig = Figure::new();
    /// fig.color_scale().colormap(Colormap::Magma);
    /// fig.color_scale().colormap(ironlab::ir::Colormap::Listed {
    ///     colors: vec![Color::rgb(0.0, 0.2, 0.6), Color::WHITE, Color::rgb(0.7, 0.1, 0.1)],
    /// });
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn colormap(&mut self, colormap: impl Into<ironlab_ir::Colormap>) -> &mut Self {
        let colormap = colormap.into();
        match &mut self.scale().kind {
            ColorScaleKind::Continuous(scale) => scale.colormap = colormap,
            ColorScaleKind::Discrete(scale) => scale.colormap = colormap,
            ColorScaleKind::Indexed(_) | ColorScaleKind::Categorical(_) => {}
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
            ColorScaleKind::Continuous(scale) => scale.limits = limits,
            ColorScaleKind::Discrete(scale) => {
                let count = match &scale.levels {
                    ColorScaleLevels::Auto { count, .. } => *count,
                    ColorScaleLevels::Explicit { values } => band_count(values),
                };
                scale.levels = ColorScaleLevels::Auto { count, limits };
            }
            ColorScaleKind::Indexed(_) | ColorScaleKind::Categorical(_) => {}
        }
        self
    }

    /// Sets whether a continuous or discrete scale places values in linear space (the
    /// default) or logarithmic space.
    ///
    /// Under a logarithmic norm, automatic limits range over the positive values only,
    /// and a value that is not positive lies below the range.
    pub fn norm(&mut self, norm: ColorScaleNorm) -> &mut Self {
        match &mut self.scale().kind {
            ColorScaleKind::Continuous(scale) => scale.norm = norm,
            ColorScaleKind::Discrete(scale) => scale.norm = norm,
            ColorScaleKind::Indexed(_) | ColorScaleKind::Categorical(_) => {}
        }
        self
    }

    /// Makes the scale discrete, with bands between the given edges, each drawn in one
    /// colour of the colormap.
    ///
    /// A continuous scale keeps its colormap, norm and out-of-range policies; a discrete
    /// scale keeps everything but its edges; and an indexed or categorical scale becomes
    /// a discrete scale with the defaults of that kind. A filled contour that names the
    /// scale, and has no levels of its own, fills exactly these bands (MATLAB's
    /// `contourf` with a vector of levels and a matching `colormap`). Edges that are not
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
    /// let bands = fig.color_scale().discrete([0.0, 0.5, 1.0, 1.5, 2.0]).id();
    /// fig.axes(0, 0).contourf(&x, &x, &z).color_scale(bands);
    /// assert!(fig.validate().is_valid());
    /// ```
    pub fn discrete(&mut self, edges: impl AsRef<[f64]>) -> &mut Self {
        let levels = ColorScaleLevels::Explicit {
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
        self.make_discrete(|limits| ColorScaleLevels::Auto { count, limits });
        self
    }

    /// Sets what is drawn for a value below the range of a continuous or discrete scale:
    /// below its lower limit or first edge, or not positive under a logarithmic norm.
    ///
    /// The policy is [`OutOfRange::Clamp`] by default, which draws the first colour of
    /// the colormap. A [`Color`] draws that colour, [`OutOfRange::Transparent`] draws
    /// nothing, and [`OutOfRange::Strict`] makes such a value a validation error.
    pub fn below(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        let policy = policy.into();
        match &mut self.scale().kind {
            ColorScaleKind::Continuous(scale) => scale.below.policy = policy,
            ColorScaleKind::Discrete(scale) => scale.below.policy = policy,
            ColorScaleKind::Indexed(_) | ColorScaleKind::Categorical(_) => {}
        }
        self
    }

    /// Sets what is drawn for a value above the range of a continuous or discrete scale,
    /// as [`below`](Self::below) does for values below it; a clamp draws the last colour
    /// of the colormap.
    pub fn above(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        let policy = policy.into();
        match &mut self.scale().kind {
            ColorScaleKind::Continuous(scale) => scale.above.policy = policy,
            ColorScaleKind::Discrete(scale) => scale.above.policy = policy,
            ColorScaleKind::Indexed(_) | ColorScaleKind::Categorical(_) => {}
        }
        self
    }

    /// Sets what is drawn for a value that names no entry of an indexed or categorical
    /// scale: a negative value, a value not less than the number of entries, or a value
    /// that is not a whole number.
    ///
    /// The policy is [`OutOfRange::Strict`] by default, so such a value is a validation
    /// error. [`OutOfRange::Clamp`] truncates the value toward zero and clamps it to the
    /// first or last entry, which suits indices computed as floating-point numbers.
    pub fn outside(&mut self, policy: impl Into<OutOfRange>) -> &mut Self {
        let policy = policy.into();
        match &mut self.scale().kind {
            ColorScaleKind::Indexed(scale) => scale.outside.policy = policy,
            ColorScaleKind::Categorical(scale) => scale.outside.policy = policy,
            ColorScaleKind::Continuous(_) | ColorScaleKind::Discrete(_) => {}
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
    /// fig.indexed_scale(Colormap::Gray.sample(256))
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
    /// fig.color_scale()
    ///     .limits(0.0, 40.0)
    ///     .above(Color::rgb(1.0, 0.0, 1.0))
    ///     .show_key(ColorScaleKey::Above);
    /// ```
    pub fn show_key(&mut self, key: ColorScaleKey) -> &mut Self {
        if let Some(category) = self.key(key) {
            category.show_key = true;
        }
        self
    }

    /// Shows the key of one out-of-range category with the given label, as
    /// [`show_key`](Self::show_key) does with the default label.
    pub fn key_label(&mut self, key: ColorScaleKey, label: impl Into<Text>) -> &mut Self {
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
            ColorScaleKind::Continuous(s) => change(&mut s.ticks, &mut s.tick_labels),
            ColorScaleKind::Discrete(s) => change(&mut s.ticks, &mut s.tick_labels),
            ColorScaleKind::Indexed(s) => change(&mut s.ticks, &mut s.tick_labels),
            ColorScaleKind::Categorical(_) => {}
        }
        self
    }

    /// Returns the out-of-range category behind a key, or `None` when the kind of the scale
    /// does not have the key, as [`ColorScale::key`] does.
    fn key(&mut self, key: ColorScaleKey) -> Option<&mut ColorScaleOutOfRange> {
        let scale = self.scale();
        match (&mut scale.kind, key) {
            (_, ColorScaleKey::NonFinite) => Some(&mut scale.non_finite),
            (ColorScaleKind::Continuous(s), ColorScaleKey::Below) => Some(&mut s.below),
            (ColorScaleKind::Continuous(s), ColorScaleKey::Above) => Some(&mut s.above),
            (ColorScaleKind::Discrete(s), ColorScaleKey::Below) => Some(&mut s.below),
            (ColorScaleKind::Discrete(s), ColorScaleKey::Above) => Some(&mut s.above),
            (ColorScaleKind::Indexed(s), ColorScaleKey::Outside) => Some(&mut s.outside),
            (ColorScaleKind::Categorical(s), ColorScaleKey::Outside) => Some(&mut s.outside),
            _ => None,
        }
    }

    /// Returns the scale in the figure IR.
    fn scale(&mut self) -> &mut ColorScale {
        self.fig
            .color_scale_mut(self.id)
            .expect("a colour scale handle always refers to a colour scale of its figure")
    }

    /// Makes the scale discrete with the levels that `levels` builds from the limits the
    /// scale had, keeping the colormap, norm and policies of a continuous or discrete
    /// scale.
    fn make_discrete(&mut self, levels: impl FnOnce(Limits) -> ColorScaleLevels) {
        let scale = self.scale();
        let discrete = match std::mem::take(&mut scale.kind) {
            ColorScaleKind::Continuous(ContinuousColorScale {
                colormap,
                limits,
                norm,
                below,
                above,
                ticks,
                tick_labels,
            }) => DiscreteColorScale {
                colormap,
                levels: levels(limits),
                norm,
                below,
                above,
                ticks,
                tick_labels,
            },
            ColorScaleKind::Discrete(discrete) => {
                let limits = match &discrete.levels {
                    ColorScaleLevels::Auto { limits, .. } => *limits,
                    ColorScaleLevels::Explicit { values } => {
                        match (values.first(), values.last()) {
                            (Some(&min), Some(&max)) => Limits::Manual { min, max },
                            _ => Limits::Auto,
                        }
                    }
                };
                DiscreteColorScale {
                    levels: levels(limits),
                    ..discrete
                }
            }
            ColorScaleKind::Indexed(_) | ColorScaleKind::Categorical(_) => DiscreteColorScale {
                levels: levels(Limits::Auto),
                ..DiscreteColorScale::default()
            },
        };
        scale.kind = ColorScaleKind::Discrete(discrete);
    }
}

/// Returns the number of bands between explicit edges, at least one.
fn band_count(edges: &[f64]) -> u32 {
    u32::try_from(edges.len().saturating_sub(1).max(1)).unwrap_or(u32::MAX)
}

/// Returns a continuous scale kind with the defaults of its kind.
pub(crate) fn continuous() -> ColorScaleKind {
    ColorScaleKind::Continuous(ContinuousColorScale::default())
}

/// Returns an indexed scale kind with the given entries and the defaults of its kind.
pub(crate) fn indexed(colors: impl IntoIterator<Item = Color>) -> ColorScaleKind {
    ColorScaleKind::Indexed(IndexedColorScale {
        colors: colors.into_iter().collect(),
        outside: ColorScaleOutOfRange::new(OutOfRange::Strict),
        ..IndexedColorScale::default()
    })
}

/// Returns a categorical scale kind with the given categories and the defaults of its
/// kind.
pub(crate) fn categorical<T: Into<Text>>(
    categories: impl IntoIterator<Item = (Color, T)>,
) -> ColorScaleKind {
    ColorScaleKind::Categorical(CategoricalColorScale {
        categories: categories
            .into_iter()
            .map(|(color, label)| ColorCategory {
                color,
                label: label.into(),
            })
            .collect(),
        ..CategoricalColorScale::default()
    })
}

/// Returns the default colour scale of the axes with the given identifier: the colour
/// scale of the figure that the first artist of the axes, in artist order, to name one
/// names. Returns `None` when no artist of the axes names a colour scale of the figure.
pub(crate) fn default_scale(fig: &ironlab_ir::Figure, axes: NodeId) -> Option<NodeId> {
    fig.axes(axes)?
        .artists
        .iter()
        .filter_map(Artist::color_scale)
        .find(|&scale| fig.color_scale(scale).is_some())
}

/// Returns the default colour scale of an axes, creating a continuous scale with the
/// defaults of its kind when the axes has none, and whether it was created.
pub(crate) fn default_or_new(fig: &mut ironlab_ir::Figure, axes: NodeId) -> (NodeId, bool) {
    match default_scale(fig, axes) {
        Some(scale) => (scale, false),
        None => (ColorScaleMut::add(fig, continuous()).id(), true),
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
        Some(Artist::ScalarImage(a)) => a.color_scale.replace(scale),
        Some(Artist::Contour(a)) => a.color_scale.replace(scale),
        Some(Artist::Surface(a)) => a.color_scale.replace(scale),
        Some(Artist::Series(a)) => a.color_scale.replace(scale),
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
        .any(|artist| artist.color_scale() == Some(created));
    if named {
        Some(created)
    } else {
        fig.color_scales.retain(|s| s.id != created);
        None
    }
}

/// The colours of a colormap sampled at evenly spaced positions, the palette from which
/// an indexed scale is built.
///
/// The trait is implemented for a bundled [`Colormap`](crate::Colormap) and for an
/// [`ir::Colormap`](crate::ir::Colormap), and is in the prelude.
///
/// ```
/// use ironlab::prelude::*;
///
/// // The 256 entries of a grey palette, of which an 8-bit value takes its own entry.
/// let grey = Colormap::Gray.sample(256);
/// assert_eq!(grey.len(), 256);
/// assert_eq!(grey[0], Color::BLACK);
/// assert_eq!(grey[255], Color::WHITE);
///
/// let mut fig = Figure::new();
/// let palette = fig.indexed_scale(grey).id();
/// let codes = ByteMatrix::from_fn(16, 16, |row, col| (row * 16 + col) as u8);
/// fig.axes(0, 0).scalar_image(&codes).color_scale(palette);
/// assert!(fig.validate().is_valid());
/// ```
pub trait ColormapSample {
    /// Returns `count` colours of the colormap, from its start to its end at evenly
    /// spaced positions, as a continuous scale draws them; one colour is the colour at
    /// the start.
    ///
    /// A bundled colormap has 256 entries, so `count` of 256 returns exactly those
    /// entries, in order. A listed colormap is interpolated linearly in sRGB between its
    /// colours; one of fewer than two colours returns its colours repeated, or black when
    /// it has none.
    fn sample(&self, count: usize) -> Vec<Color>;
}

impl ColormapSample for ColormapName {
    fn sample(&self, count: usize) -> Vec<Color> {
        ironlab_ir::Colormap::from(*self).sample(count)
    }
}

impl ColormapSample for ironlab_ir::Colormap {
    fn sample(&self, count: usize) -> Vec<Color> {
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

/// Returns the colour of a colormap at a position from 0 to 1, as the scene compiler
/// draws a continuous scale.
fn colour_at(colormap: &ironlab_ir::Colormap, t: f64) -> Color {
    match colormap {
        ironlab_ir::Colormap::Named { name, reverse } => {
            // The entry is chosen as the scene compiler chooses it: each of the 256
            // entries takes an equal share of the positions, and a reversed colormap
            // takes the entry at the mirrored index.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the position lies from 0 to 1, so the product lies from 0 to 256"
            )]
            let index = ((t.clamp(0.0, 1.0) * 256.0).floor() as usize).min(255);
            let index = if *reverse { 255 - index } else { index };
            let [r, g, b] = lut(*name)[index];
            Color::rgb(
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
            )
        }
        ironlab_ir::Colormap::Listed { colors } => match colors.len() {
            0 => Color::BLACK,
            1 => colors[0],
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
                let (a, b) = (colors[i], colors[i + 1]);
                let mix = |p: f32, q: f32| p + (q - p) * f;
                Color::rgba(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b), mix(a.a, b.a))
            }
        },
    }
}

/// Returns the lookup table of a bundled colormap.
fn lut(name: ColormapName) -> &'static Lut {
    match name {
        ColormapName::Viridis => &colormap::VIRIDIS,
        ColormapName::Cividis => &colormap::CIVIDIS,
        ColormapName::Magma => &colormap::MAGMA,
        ColormapName::Inferno => &colormap::INFERNO,
        ColormapName::Plasma => &colormap::PLASMA,
        ColormapName::Coolwarm => &colormap::COOLWARM,
        ColormapName::Gray => &colormap::GRAY,
    }
}
