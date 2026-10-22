//! Shared fixtures for the tests of edits, commands, overlays, selections and the edit
//! wire format.
//!
//! Like the other fixtures, these build figures by constructing the IR directly rather
//! than through the API under test, so that a fault in `Figure::apply` cannot hide in a
//! fixture.

use std::collections::{BTreeMap, BTreeSet};

use ironlab_ir::*;

use super::{FigureBuilder, kitchen_sink_figure};

/// Parses a property path, panicking with the text when it is not valid.
pub fn path(text: &str) -> PropertyPath {
    text.parse()
        .unwrap_or_else(|e| panic!("{text:?} is not a property path: {e:?}"))
}

/// A set of a property of a node.
pub fn set(node: NodeId, at: &str, value: Value) -> Edit {
    Edit::Set {
        node,
        path: path(at),
        value,
    }
}

/// A transaction of the given edits.
pub fn tx(edits: impl IntoIterator<Item = Edit>) -> Transaction {
    Transaction {
        edits: edits.into_iter().collect(),
    }
}

/// Manual limits.
pub fn manual(min: f64, max: f64) -> Limits {
    Limits::Manual { min, max }
}

/// Applies a transaction to a copy of a figure, returning the edited copy and the
/// inverse, and leaving the given figure untouched.
pub fn applied(
    fig: &Figure,
    transaction: &Transaction,
) -> Result<(Figure, Transaction), EditError> {
    let mut copy = fig.clone();
    let inverse = copy.apply(transaction)?;
    Ok((copy, inverse))
}

/// Returns the kind of an artist, found without the API under test.
pub fn artist_kind(artist: &Artist) -> NodeKind {
    match artist {
        Artist::Series(_) => NodeKind::Series,
        Artist::Contour(_) => NodeKind::Contour,
        Artist::Quiver(_) => NodeKind::Quiver,
        Artist::Surface(_) => NodeKind::Surface,
        Artist::Image(_) => NodeKind::Image,
        Artist::ScalarImage(_) => NodeKind::ScalarImage,
    }
}

/// Every node of a figure with its kind, in tree order (the figure, its data arrays, its
/// colour scales, its axes each followed by its artists, and its panels), found without
/// the API under test.
pub fn nodes(fig: &Figure) -> Vec<(NodeId, NodeKind)> {
    let mut nodes = vec![(fig.id, NodeKind::Figure)];
    nodes.extend(fig.data.iter().map(|data| (data.id, NodeKind::DataArray)));
    nodes.extend(
        fig.color_scales
            .iter()
            .map(|scale| (scale.id, NodeKind::ColorScale)),
    );
    for axes in &fig.axes {
        nodes.push((axes.id, NodeKind::Axes));
        nodes.extend(axes.artists.iter().map(|a| (a.id(), artist_kind(a))));
    }
    nodes.extend(fig.panels.iter().map(|panel| (panel.id, NodeKind::Panel)));
    nodes
}

/// The identifiers of the axes of a figure, in order.
pub fn axes_ids(fig: &Figure) -> Vec<NodeId> {
    fig.axes.iter().map(|a| a.id).collect()
}

/// The identifiers of the artists of an axes, in order.
pub fn artist_ids(fig: &Figure, axes: NodeId) -> Vec<NodeId> {
    super::find_axes(fig, axes)
        .artists
        .iter()
        .map(Artist::id)
        .collect()
}

/// A series on the given data, with the defaults of `Series` (a line without markers).
pub fn line(id: NodeId, x: NodeId, y: NodeId) -> Artist {
    Artist::Series(Series {
        id,
        x,
        y,
        ..Series::default()
    })
}

/// A data array node with the given identifier and no name.
pub fn data_node(id: NodeId, array: NdArray) -> Node {
    Node::DataArray(DataArray {
        id,
        name: None,
        array,
    })
}

/// A two-dimensional axes with the given identifier in the top-left cell.
pub fn axes_node(id: NodeId, artists: Vec<Artist>) -> Node {
    Node::Axes(Box::new(Axes {
        id,
        artists,
        ..Axes::default()
    }))
}

/// The figures on which every settable property of every kind of node is reachable
/// somewhere: the kitchen-sink figure, and a copy of it in which every optional title,
/// axis label, legend, artist label and data array name is present, every automatic axis
/// limit is manual (and positive, for logarithmic axes), every colour specification of an
/// artist is a fixed colour, so that the colour components below it are reachable, every
/// out-of-range category of a colour scale has a label and a fixed colour for the same
/// reason, every automatic limit of a colour scale is manual, the ticks of every colour
/// scale take another variant, every contour has levels of its own, both pixel ranges and
/// the plane offset of every image are present, and every image on a wall of a
/// three-dimensional axes is moved to the other wall.
///
/// Together they set every variant of every tagged value (automatic and manual limits,
/// both projections, both grids, both kinds of levels and of band edges, both colormaps,
/// every quiver scale, both contour placements, every kind of
/// colour scale, and for each image kind every plane), so a path below any variant is
/// reachable in one of them.
pub fn representative_figures() -> Vec<Figure> {
    /// Gives the plane offset and both pixel ranges a value, and moves an image on a wall
    /// to the other wall, so that between the kitchen-sink figure (where each kind lies on
    /// the floor of a 2D axes and on one wall of a 3D axes) and the copy, each kind lies
    /// on every plane in one of the two figures, and every offset and every range of the
    /// copy holds a value. The kitchen-sink figure places wall images only in
    /// three-dimensional axes, so the move keeps the copy valid.
    fn fill_placement(placement: &mut ImagePlacement) {
        placement.plane = match placement.plane {
            ImagePlane::Xy { z } => ImagePlane::Xy {
                z: Some(z.unwrap_or(0.25)),
            },
            ImagePlane::Xz { y } => ImagePlane::Yz {
                x: Some(y.unwrap_or(0.25)),
            },
            ImagePlane::Yz { x } => ImagePlane::Xz {
                y: Some(x.unwrap_or(0.25)),
            },
        };
        for range in [&mut placement.columns, &mut placement.rows] {
            range.get_or_insert(PixelRange {
                first: 0.0,
                last: 1.0,
            });
        }
    }

    let base = kitchen_sink_figure();
    let mut full = base.clone();
    for (k, data) in full.data.iter_mut().enumerate() {
        data.name.get_or_insert_with(|| format!("array {k}"));
    }
    let fixed = ColorSpec::Rgba {
        color: Color::rgb(0.0, 114.0 / 255.0, 178.0 / 255.0),
    };
    let painted = OutOfRange::Rgba {
        color: Color::rgb(0.0, 114.0 / 255.0, 178.0 / 255.0),
    };
    for (k, axes) in full.axes.iter_mut().enumerate() {
        axes.title
            .get_or_insert_with(|| Text::new(format!("axes {k}")));
        for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
            axis.label.get_or_insert_with(|| Text::plain("label"));
        }
        axes.legend.get_or_insert_with(Legend::default);
        // Every axes takes a fixed ratio of y and of z, kept by its limits and with its title
        // on the right, so that the ratio's values and the variants the kitchen-sink figure
        // leaves at their defaults are set.
        axes.aspect = Aspect::Ratio { y: 2.0, z: 0.5 };
        axes.aspect_adjust = AspectAdjust::Limits;
        axes.title_align = TitleAlign::Right;
        // Every axis takes the kind of ticks it does not have in the kitchen-sink figure, so
        // that between the two figures every variant of the ticks of every axis is set:
        // automatic ticks become explicit values with explicit labels, explicit values a
        // regular step with hidden labels, and a regular step explicit values.
        for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
            (axis.ticks, axis.tick_labels) = match axis.ticks {
                Ticks::Auto => (
                    Ticks::Values(vec![2.0, 5.0]),
                    TickLabels::Text(vec![Text::plain("two"), Text::new("$5$")]),
                ),
                Ticks::Values(_) => (
                    Ticks::Step {
                        step: 0.5,
                        origin: 1.0,
                    },
                    TickLabels::Hidden,
                ),
                Ticks::Step { .. } => (Ticks::Values(vec![2.0, 4.0]), TickLabels::Auto),
            };
        }
        // The kitchen-sink figure sets manual limits only on some x axes and colour limits.
        for axis in [&mut axes.x, &mut axes.y, &mut axes.z] {
            if axis.limits == Limits::Auto {
                axis.limits = manual(1.0, 10.0);
            }
        }
        for artist in &mut axes.artists {
            match artist {
                Artist::Series(a) => {
                    a.label.get_or_insert_with(|| Text::new("series"));
                    a.line.color = fixed;
                    a.marker.face = fixed;
                    a.marker.edge = fixed;
                }
                Artist::Contour(a) => {
                    a.label.get_or_insert_with(|| Text::new("contour"));
                    a.line.color = fixed;
                }
                Artist::Quiver(a) => {
                    a.label.get_or_insert_with(|| Text::new("quiver"));
                    a.line.color = fixed;
                }
                Artist::Surface(a) => {
                    a.label.get_or_insert_with(|| Text::new("surface"));
                    a.face = fixed;
                    a.edge = fixed;
                }
                Artist::Image(a) => {
                    a.label.get_or_insert_with(|| Text::new("image"));
                    fill_placement(&mut a.placement);
                }
                Artist::ScalarImage(a) => {
                    a.label.get_or_insert_with(|| Text::new("scalar image"));
                    fill_placement(&mut a.placement);
                }
            }
            if let Artist::Contour(a) = artist {
                a.levels.get_or_insert(Levels::Auto { count: 5 });
            }
        }
    }
    for scale in &mut full.color_scales {
        scale.title.get_or_insert_with(|| Text::plain("scale"));
        let paint = |category: &mut ColorScaleOutOfRange| {
            category.policy = painted;
            category.label.get_or_insert_with(|| Text::plain("key"));
        };
        paint(&mut scale.non_finite);
        // The ticks of each scale take a variant they do not have in the kitchen-sink
        // figure, as the ticks of the axes do.
        let rotate = |ticks: &mut Ticks, labels: &mut TickLabels| {
            (*ticks, *labels) = match ticks {
                Ticks::Auto => (
                    Ticks::Values(vec![2.0, 5.0]),
                    TickLabels::Text(vec![Text::plain("two"), Text::new("$5$")]),
                ),
                Ticks::Values(_) => (
                    Ticks::Step {
                        step: 0.5,
                        origin: 1.0,
                    },
                    TickLabels::Hidden,
                ),
                Ticks::Step { .. } => (Ticks::Values(vec![2.0, 4.0]), TickLabels::Auto),
            };
        };
        match &mut scale.kind {
            ColorScaleKind::Continuous(c) => {
                paint(&mut c.below);
                paint(&mut c.above);
                rotate(&mut c.ticks, &mut c.tick_labels);
                if c.limits == Limits::Auto {
                    c.limits = manual(1.0, 10.0);
                }
            }
            ColorScaleKind::Discrete(d) => {
                paint(&mut d.below);
                paint(&mut d.above);
                rotate(&mut d.ticks, &mut d.tick_labels);
                if let ColorScaleLevels::Auto { limits, .. } = &mut d.levels
                    && *limits == Limits::Auto
                {
                    *limits = manual(1.0, 10.0);
                }
            }
            ColorScaleKind::Indexed(i) => {
                paint(&mut i.outside);
                rotate(&mut i.ticks, &mut i.tick_labels);
            }
            ColorScaleKind::Categorical(k) => paint(&mut k.outside),
        }
    }
    // Every panel takes a title, aligned otherwise than in the kitchen-sink figure, and
    // every colourbar the length and the title placement it does not have there, so that
    // between the two figures every value of the panel's layout properties is set.
    for panel in &mut full.panels {
        panel.title.get_or_insert_with(|| Text::plain("panel"));
        panel.title_align = match panel.title_align {
            TitleAlign::Left => TitleAlign::Center,
            TitleAlign::Center => TitleAlign::Right,
            TitleAlign::Right => TitleAlign::Left,
        };
        if let PanelContent::Colorbar(colorbar) = &mut panel.content {
            colorbar.length = match colorbar.length {
                None => Some(0.5),
                Some(_) => None,
            };
            colorbar.title_placement = match colorbar.title_placement {
                ColorbarTitlePlacement::Along => ColorbarTitlePlacement::Above,
                ColorbarTitlePlacement::Above => ColorbarTitlePlacement::Along,
            };
        }
    }
    for axes in &mut full.axes {
        if let Some(colorbar) = &mut axes.colorbar {
            colorbar.title_placement = match colorbar.title_placement {
                ColorbarTitlePlacement::Along => ColorbarTitlePlacement::Above,
                ColorbarTitlePlacement::Above => ColorbarTitlePlacement::Along,
            };
        }
    }
    vec![base, full]
}

/// A figure of streamed data: a 2D axes holding a line on vectors of five values and a
/// contour of a field of shape `[4, 3]` on a rectilinear grid, and a second 2D axes
/// holding another line on vectors of three values.
pub struct Streaming {
    pub fig: Figure,
    pub axes: NodeId,
    pub line: NodeId,
    pub x: NodeId,
    pub y: NodeId,
    pub contour: NodeId,
    pub gx: NodeId,
    pub gy: NodeId,
    pub z: NodeId,
    pub other_axes: NodeId,
    pub other_line: NodeId,
    pub other_x: NodeId,
    pub other_y: NodeId,
}

/// Builds the [`Streaming`] figure. The x data of the first line contains zero, so a
/// logarithmic x axis over it produces a warning.
pub fn streaming_figure() -> Streaming {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout::new(1, 2);
    let axes = b.axes2d(0, 0);
    let x = b.vector(&[0.0, 1.0, 2.0, 3.0, 4.0]);
    let y = b.vector(&[10.0, 11.0, 12.0, 13.0, 14.0]);
    let line_id = b.node();
    b.push(axes, line(line_id, x, y));
    let gx = b.vector(&[0.0, 1.0, 2.0]);
    let gy = b.vector(&[0.0, 1.0, 2.0, 3.0]);
    let z = b.matrix(4, 3, |j, i| (10 * j + i) as f64);
    let contour = b.node();
    b.push(
        axes,
        Artist::Contour(Contour {
            id: contour,
            grid: Grid::Rectilinear { x: gx, y: gy },
            z,
            ..Contour::default()
        }),
    );
    let other_axes = b.axes2d(0, 1);
    let other_x = b.vector(&[1.0, 2.0, 3.0]);
    let other_y = b.vector(&[1.0, 4.0, 9.0]);
    let other_line = b.node();
    b.push(other_axes, line(other_line, other_x, other_y));
    // The colour scale of the contour, added last so that the other identifiers are
    // those the figure had before contours named scales.
    let scale = b.color_scale(ColorScaleKind::default());
    if let Some(Artist::Contour(c)) = b.fig.artist_mut(contour) {
        c.color_scale = Some(scale);
    }
    Streaming {
        fig: b.build(),
        axes,
        line: line_id,
        x,
        y,
        contour,
        gx,
        gy,
        z,
        other_axes,
        other_line,
        other_x,
        other_y,
    }
}

/// An array of shape `[rows, cols]` whose values count up from `start`.
pub fn rows(rows: usize, cols: usize, start: f64) -> NdArray {
    NdArray::from_shape(
        vec![rows, cols],
        (0..rows * cols).map(|k| start + k as f64).collect(),
    )
    .expect("the shape matches the values")
}

/// Returns a value of the same type that differs from the given one, chosen so that most
/// such changes keep a valid figure valid; [`Value::Unset`] is returned unchanged.
///
/// The match is exhaustive, so a new variant of [`Value`] must be given a perturbation
/// before the tests compile.
pub fn perturb(value: &Value) -> Value {
    let color_spec = |spec: &ColorSpec| match spec {
        ColorSpec::Auto => ColorSpec::None,
        ColorSpec::None => ColorSpec::Colormapped,
        ColorSpec::Colormapped => ColorSpec::Rgba {
            color: Color::rgb(0.25, 0.5, 0.75),
        },
        ColorSpec::Rgba { .. } => ColorSpec::Auto,
    };
    match value {
        Value::Unset => Value::Unset,
        Value::Bool(b) => Value::Bool(!b),
        Value::UInt32(n) => Value::UInt32(n + 1),
        Value::Double(x) if x.is_finite() => Value::Double(x + 1.5),
        Value::Double(_) => Value::Double(1.0),
        Value::Float(x) => Value::Float(if *x > 0.5 { x - 0.25 } else { x + 0.25 }),
        Value::String(s) => Value::String(format!("{s} (edited)")),
        Value::Doubles(values) if values.is_empty() => Value::Doubles(vec![1.0]),
        Value::TrackSizes(sizes) if sizes.is_empty() => Value::TrackSizes(vec![TrackSize::Fit]),
        Value::TrackSizes(sizes) => Value::TrackSizes(
            sizes
                .iter()
                .map(|size| match size {
                    TrackSize::Weight(_) => TrackSize::Fit,
                    TrackSize::Fit => TrackSize::Weight(1.5),
                })
                .collect(),
        ),
        Value::Doubles(values) => Value::Doubles(values.iter().map(|v| v + 0.25).collect()),
        Value::Strings(values) if values.is_empty() => Value::Strings(vec!["edited".to_owned()]),
        Value::Strings(values) => Value::Strings(values.iter().skip(1).cloned().collect()),
        Value::Texts(texts) => Value::Texts(
            texts
                .iter()
                .map(|t| Text::plain(format!("{} (edited)", t.content)))
                .collect(),
        ),
        Value::Text(text) => Value::Text(Text {
            content: format!("{} (edited)", text.content),
            interpreter: text.interpreter,
        }),
        Value::Interpreter(Interpreter::Latex) => Value::Interpreter(Interpreter::None),
        Value::Interpreter(Interpreter::None) => Value::Interpreter(Interpreter::Latex),
        Value::FigureSize(size) => Value::FigureSize(FigureSize {
            width_mm: size.width_mm + 10.0,
            height_mm: size.height_mm,
        }),
        // There is only one font set.
        Value::FontSetId(id) => Value::FontSetId(*id),
        Value::Color(color) => {
            let other = Color::rgb(0.25, 0.5, 0.75);
            Value::Color(if *color == other { Color::BLACK } else { other })
        }
        Value::TileLayout(layout) => Value::TileLayout(TileLayout {
            rows: layout.rows + 1,
            ..layout.clone()
        }),
        Value::Links(links) => Value::Links(links[..links.len().saturating_sub(1)].to_vec()),
        Value::Parameters(parameters) => {
            let mut parameters = parameters.clone();
            parameters.insert("edited".to_owned(), Parameter::Bool(true));
            Value::Parameters(parameters)
        }
        Value::Cell(cell) => Value::Cell(Cell {
            col_span: cell.col_span + 1,
            ..*cell
        }),
        Value::Projection(Projection::TwoD) => Value::Projection(Projection::ThreeD {
            view3d: View3d::default(),
        }),
        Value::Projection(Projection::ThreeD { .. }) => Value::Projection(Projection::TwoD),
        Value::View3d(view) => Value::View3d(View3d {
            zoom: view.zoom + 0.5,
            ..*view
        }),
        Value::Axis(axis) => Value::Axis(Axis {
            grid: !axis.grid,
            ..axis.clone()
        }),
        Value::Scale(Scale::Linear) => Value::Scale(Scale::Log),
        Value::Scale(Scale::Log) => Value::Scale(Scale::Linear),
        Value::Limits(Limits::Auto) => Value::Limits(manual(1.0, 2.0)),
        Value::Limits(Limits::Manual { .. }) => Value::Limits(Limits::Auto),
        Value::Ticks(Ticks::Auto) => Value::Ticks(Ticks::Step {
            step: 0.5,
            origin: 0.0,
        }),
        Value::Ticks(Ticks::Step { .. }) => Value::Ticks(Ticks::Auto),
        Value::Ticks(Ticks::Values(values)) => {
            Value::Ticks(Ticks::Values(values.iter().map(|v| v * 2.0).collect()))
        }
        Value::TickLabels(TickLabels::Auto) => Value::TickLabels(TickLabels::Hidden),
        Value::TickLabels(TickLabels::Hidden) => Value::TickLabels(TickLabels::Auto),
        Value::TickLabels(TickLabels::Text(texts)) => Value::TickLabels(TickLabels::Text(
            texts
                .iter()
                .map(|t| Text::plain(format!("{} (edited)", t.content)))
                .collect(),
        )),
        Value::Aspect(Aspect::Auto) => Value::Aspect(Aspect::Equal),
        Value::Aspect(Aspect::Equal) => Value::Aspect(Aspect::Ratio { y: 2.0, z: 1.0 }),
        Value::Aspect(Aspect::Ratio { .. }) => Value::Aspect(Aspect::Auto),
        Value::AspectAdjust(AspectAdjust::Box) => Value::AspectAdjust(AspectAdjust::Limits),
        Value::AspectAdjust(AspectAdjust::Limits) => Value::AspectAdjust(AspectAdjust::Box),
        Value::TitleAlign(TitleAlign::Center) => Value::TitleAlign(TitleAlign::Left),
        Value::TitleAlign(TitleAlign::Left) => Value::TitleAlign(TitleAlign::Right),
        Value::TitleAlign(TitleAlign::Right) => Value::TitleAlign(TitleAlign::Center),
        Value::ColormapName(ColormapName::Gray) => Value::ColormapName(ColormapName::Viridis),
        Value::ColormapName(_) => Value::ColormapName(ColormapName::Gray),
        Value::Legend(Legend::Inside(legend)) => Value::Legend(Legend::Inside(InsideLegend {
            boxed: !legend.boxed,
            ..legend.clone()
        })),
        Value::Legend(Legend::Panel(panel)) => Value::Legend(Legend::Panel(NodeId(panel.0 + 1))),
        Value::InsideLegend(legend) => Value::InsideLegend(InsideLegend {
            boxed: !legend.boxed,
            ..legend.clone()
        }),
        Value::LegendEntryLayout(layout) => Value::LegendEntryLayout(LegendEntryLayout {
            order: match layout.order {
                LegendEntryOrder::ColumnMajor => LegendEntryOrder::RowMajor,
                LegendEntryOrder::RowMajor => LegendEntryOrder::ColumnMajor,
            },
            ..*layout
        }),
        Value::LegendEntryColumns(LegendEntryColumns::Auto) => {
            Value::LegendEntryColumns(LegendEntryColumns::Count(2))
        }
        Value::LegendEntryColumns(LegendEntryColumns::Count(_)) => {
            Value::LegendEntryColumns(LegendEntryColumns::Auto)
        }
        Value::LegendEntryOrder(LegendEntryOrder::ColumnMajor) => {
            Value::LegendEntryOrder(LegendEntryOrder::RowMajor)
        }
        Value::LegendEntryOrder(LegendEntryOrder::RowMajor) => {
            Value::LegendEntryOrder(LegendEntryOrder::ColumnMajor)
        }
        Value::NodeId(id) => Value::NodeId(NodeId(id.0 + 1)),
        Value::LegendLocation(LegendLocation::Best) => {
            Value::LegendLocation(LegendLocation::NorthEast)
        }
        Value::LegendLocation(_) => Value::LegendLocation(LegendLocation::Best),
        Value::PanelLocation(PanelLocation::Center) => {
            Value::PanelLocation(PanelLocation::NorthEast)
        }
        Value::PanelLocation(_) => Value::PanelLocation(PanelLocation::Center),
        Value::PanelLegend(legend) => Value::PanelLegend(PanelLegend {
            boxed: !legend.boxed,
            ..legend.clone()
        }),
        Value::PanelContent(PanelContent::Legend(legend)) => {
            Value::PanelContent(PanelContent::Legend(PanelLegend {
                boxed: !legend.boxed,
                ..legend.clone()
            }))
        }
        Value::PanelContent(PanelContent::Colorbar(colorbar)) => {
            Value::PanelContent(PanelContent::Colorbar(PanelColorbar {
                orientation: other_orientation(colorbar.orientation),
                ..*colorbar
            }))
        }
        Value::PanelColorbar(colorbar) => Value::PanelColorbar(PanelColorbar {
            orientation: other_orientation(colorbar.orientation),
            ..*colorbar
        }),
        Value::ColorbarOrientation(orientation) => {
            Value::ColorbarOrientation(other_orientation(*orientation))
        }
        Value::InsideColorbar(colorbar) => Value::InsideColorbar(InsideColorbar {
            length: if colorbar.length == 0.5 { 0.8 } else { 0.5 },
            ..*colorbar
        }),
        Value::InsideColorbarSide(InsideColorbarSide::South) => {
            Value::InsideColorbarSide(InsideColorbarSide::North)
        }
        Value::InsideColorbarSide(_) => Value::InsideColorbarSide(InsideColorbarSide::South),
        Value::ColorbarTitlePlacement(ColorbarTitlePlacement::Along) => {
            Value::ColorbarTitlePlacement(ColorbarTitlePlacement::Above)
        }
        Value::ColorbarTitlePlacement(ColorbarTitlePlacement::Above) => {
            Value::ColorbarTitlePlacement(ColorbarTitlePlacement::Along)
        }
        Value::ColorSpec(spec) => Value::ColorSpec(color_spec(spec)),
        Value::LineStyle(style) => Value::LineStyle(LineStyle {
            width_pt: style.width_pt + 0.5,
            ..*style
        }),
        Value::DashStyle(DashStyle::Solid) => Value::DashStyle(DashStyle::Dashed),
        Value::DashStyle(_) => Value::DashStyle(DashStyle::Solid),
        Value::MarkerStyle(style) => Value::MarkerStyle(MarkerStyle {
            size_pt: style.size_pt + 1.0,
            ..*style
        }),
        Value::MarkerShape(MarkerShape::Square) => Value::MarkerShape(MarkerShape::Circle),
        Value::MarkerShape(_) => Value::MarkerShape(MarkerShape::Square),
        Value::Coords(Coords::Data) => Value::Coords(Coords::AxesFraction),
        Value::Coords(Coords::AxesFraction) => Value::Coords(Coords::Data),
        Value::Grid(Grid::Rectilinear { x, y }) => Value::Grid(Grid::Rectilinear { x: *y, y: *x }),
        Value::Grid(Grid::Curvilinear { x, y }) => Value::Grid(Grid::Curvilinear { x: *y, y: *x }),
        Value::Levels(Levels::Auto { count }) => Value::Levels(Levels::Auto { count: count + 1 }),
        Value::Levels(Levels::Explicit { .. }) => Value::Levels(Levels::Auto { count: 10 }),
        Value::ContourPlacement(ContourPlacement::Plane { .. }) => {
            Value::ContourPlacement(ContourPlacement::AtLevel)
        }
        Value::ContourPlacement(ContourPlacement::AtLevel) => {
            Value::ContourPlacement(ContourPlacement::Plane { z: None })
        }
        Value::QuiverScale(QuiverScale::Auto) => Value::QuiverScale(QuiverScale::Off),
        Value::QuiverScale(QuiverScale::Off) => {
            Value::QuiverScale(QuiverScale::Factor { value: 2.0 })
        }
        Value::QuiverScale(QuiverScale::Factor { .. }) => Value::QuiverScale(QuiverScale::Auto),
        // A placement gains or loses its column range, which keeps every figure valid.
        Value::ImagePlacement(placement) => Value::ImagePlacement(ImagePlacement {
            columns: match placement.columns {
                None => Some(PixelRange {
                    first: 0.0,
                    last: 1.0,
                }),
                Some(_) => None,
            },
            ..*placement
        }),
        // Both centres move together, so a mirrored or single-pixel range stays one.
        Value::PixelRange(range) => Value::PixelRange(PixelRange {
            first: range.first + 1.0,
            last: range.last + 1.0,
        }),
        // The next plane, keeping the offset; a wall in a 2D axes is refused as invalid.
        Value::ImagePlane(ImagePlane::Xy { z }) => Value::ImagePlane(ImagePlane::Xz { y: *z }),
        Value::ImagePlane(ImagePlane::Xz { y }) => Value::ImagePlane(ImagePlane::Yz { x: *y }),
        Value::ImagePlane(ImagePlane::Yz { x }) => Value::ImagePlane(ImagePlane::Xy { z: *x }),
        Value::OutOfRange(OutOfRange::Strict) => Value::OutOfRange(OutOfRange::Transparent),
        Value::OutOfRange(OutOfRange::Transparent) => Value::OutOfRange(OutOfRange::Clamp),
        Value::OutOfRange(OutOfRange::Clamp) => Value::OutOfRange(OutOfRange::Rgba {
            color: Color::rgb(0.25, 0.5, 0.75),
        }),
        Value::OutOfRange(OutOfRange::Rgba { .. }) => Value::OutOfRange(OutOfRange::Strict),
        Value::ColorScaleOutOfRange(category) => {
            Value::ColorScaleOutOfRange(ColorScaleOutOfRange {
                show_key: !category.show_key,
                ..category.clone()
            })
        }
        // A kind keeps its kind and changes a property that keeps a valid scale valid.
        Value::ColorScaleKind(ColorScaleKind::Continuous(c)) => {
            Value::ColorScaleKind(ColorScaleKind::Continuous(ContinuousColorScale {
                above: ColorScaleOutOfRange {
                    show_key: !c.above.show_key,
                    ..c.above.clone()
                },
                ..c.clone()
            }))
        }
        Value::ColorScaleKind(ColorScaleKind::Discrete(d)) => {
            Value::ColorScaleKind(ColorScaleKind::Discrete(DiscreteColorScale {
                above: ColorScaleOutOfRange {
                    show_key: !d.above.show_key,
                    ..d.above.clone()
                },
                ..d.clone()
            }))
        }
        Value::ColorScaleKind(ColorScaleKind::Indexed(i)) => {
            Value::ColorScaleKind(ColorScaleKind::Indexed(IndexedColorScale {
                outside: ColorScaleOutOfRange {
                    show_key: !i.outside.show_key,
                    ..i.outside.clone()
                },
                ..i.clone()
            }))
        }
        Value::ColorScaleKind(ColorScaleKind::Categorical(k)) => {
            Value::ColorScaleKind(ColorScaleKind::Categorical(CategoricalColorScale {
                outside: ColorScaleOutOfRange {
                    show_key: !k.outside.show_key,
                    ..k.outside.clone()
                },
                ..k.clone()
            }))
        }
        Value::ContinuousColorScale(c) => Value::ContinuousColorScale(ContinuousColorScale {
            tick_labels: match c.tick_labels {
                TickLabels::Hidden => TickLabels::Auto,
                _ => TickLabels::Hidden,
            },
            ..c.clone()
        }),
        Value::DiscreteColorScale(d) => Value::DiscreteColorScale(DiscreteColorScale {
            tick_labels: match d.tick_labels {
                TickLabels::Hidden => TickLabels::Auto,
                _ => TickLabels::Hidden,
            },
            ..d.clone()
        }),
        Value::IndexedColorScale(i) => Value::IndexedColorScale(IndexedColorScale {
            tick_labels: match i.tick_labels {
                TickLabels::Hidden => TickLabels::Auto,
                _ => TickLabels::Hidden,
            },
            ..i.clone()
        }),
        Value::CategoricalColorScale(k) => Value::CategoricalColorScale(CategoricalColorScale {
            outside: ColorScaleOutOfRange {
                show_key: !k.outside.show_key,
                ..k.outside.clone()
            },
            ..k.clone()
        }),
        Value::Colormap(Colormap::Named { name, reverse }) => Value::Colormap(Colormap::Named {
            name: *name,
            reverse: !reverse,
        }),
        Value::Colormap(Colormap::Listed { colors }) => Value::Colormap(Colormap::Listed {
            colors: colors.iter().rev().copied().collect(),
        }),
        Value::ColorScaleNorm(ColorScaleNorm::Linear) => Value::ColorScaleNorm(ColorScaleNorm::Log),
        Value::ColorScaleNorm(ColorScaleNorm::Log) => Value::ColorScaleNorm(ColorScaleNorm::Linear),
        Value::ColorScaleLevels(ColorScaleLevels::Auto { count, limits }) => {
            Value::ColorScaleLevels(ColorScaleLevels::Auto {
                count: count + 1,
                limits: *limits,
            })
        }
        Value::ColorScaleLevels(ColorScaleLevels::Explicit { values }) => {
            Value::ColorScaleLevels(ColorScaleLevels::Explicit {
                values: values.iter().map(|v| v * 2.0).collect(),
            })
        }
    }
}

/// Returns the other orientation of a colourbar.
fn other_orientation(orientation: ColorbarOrientation) -> ColorbarOrientation {
    match orientation {
        ColorbarOrientation::Vertical => ColorbarOrientation::Horizontal,
        ColorbarOrientation::Horizontal => ColorbarOrientation::Vertical,
    }
}

/// The number of variants of [`Value`].
///
/// There is no `Value::DataId`: a reference to a data array is a [`Value::NodeId`] like
/// any other reference to a node.
pub const VALUE_VARIANTS: usize = 68;

/// Returns a distinct index from zero for each variant of [`Value`].
///
/// The match is exhaustive, so adding a variant to [`Value`] stops the tests compiling
/// until the variant is given an index here, [`VALUE_VARIANTS`] is raised and
/// [`sample_values`] holds a value of the variant.
pub fn value_variant(value: &Value) -> usize {
    match value {
        Value::Unset => 0,
        Value::Bool(_) => 1,
        Value::UInt32(_) => 2,
        Value::Double(_) => 3,
        Value::Float(_) => 4,
        Value::String(_) => 5,
        Value::PanelContent(_) => 6,
        Value::Doubles(_) => 7,
        Value::Text(_) => 8,
        Value::Interpreter(_) => 9,
        Value::FigureSize(_) => 10,
        Value::FontSetId(_) => 11,
        Value::Color(_) => 12,
        Value::TileLayout(_) => 13,
        Value::Links(_) => 14,
        Value::Parameters(_) => 15,
        Value::Cell(_) => 16,
        Value::Projection(_) => 17,
        Value::View3d(_) => 18,
        Value::Axis(_) => 19,
        Value::Scale(_) => 20,
        Value::Limits(_) => 21,
        Value::ColormapName(_) => 22,
        Value::Legend(_) => 23,
        Value::LegendLocation(_) => 24,
        Value::ColorSpec(_) => 25,
        Value::LineStyle(_) => 26,
        Value::DashStyle(_) => 27,
        Value::MarkerStyle(_) => 28,
        Value::MarkerShape(_) => 29,
        Value::ColorScaleNorm(_) => 30,
        Value::ColorScaleLevels(_) => 31,
        Value::Grid(_) => 32,
        Value::Levels(_) => 33,
        Value::ContourPlacement(_) => 34,
        Value::QuiverScale(_) => 35,
        Value::ImagePlacement(_) => 36,
        Value::PixelRange(_) => 37,
        Value::ImagePlane(_) => 38,
        Value::OutOfRange(_) => 39,
        Value::Strings(_) => 40,
        Value::Texts(_) => 48,
        Value::InsideLegend(_) => 41,
        Value::LegendEntryLayout(_) => 42,
        Value::LegendEntryColumns(_) => 43,
        Value::LegendEntryOrder(_) => 44,
        Value::NodeId(_) => 45,
        Value::Ticks(_) => 46,
        Value::TickLabels(_) => 47,
        Value::Coords(_) => 49,
        Value::TrackSizes(_) => 50,
        Value::Aspect(_) => 51,
        Value::AspectAdjust(_) => 52,
        Value::TitleAlign(_) => 53,
        Value::PanelLocation(_) => 54,
        Value::PanelLegend(_) => 55,
        Value::ColorScaleOutOfRange(_) => 56,
        Value::ColorScaleKind(_) => 57,
        Value::ContinuousColorScale(_) => 58,
        Value::DiscreteColorScale(_) => 59,
        Value::IndexedColorScale(_) => 60,
        Value::CategoricalColorScale(_) => 61,
        Value::Colormap(_) => 62,
        Value::PanelColorbar(_) => 63,
        Value::ColorbarOrientation(_) => 64,
        Value::InsideColorbar(_) => 65,
        Value::InsideColorbarSide(_) => 66,
        Value::ColorbarTitlePlacement(_) => 67,
    }
}

/// One value of every variant of [`Value`], each with non-default content where the type
/// has any, and with finite numbers and colours of eight bits per component so that the
/// values also survive JSON.
pub fn sample_values() -> Vec<Value> {
    let blue = Color::rgb(0.0, 114.0 / 255.0, 178.0 / 255.0);
    vec![
        Value::Unset,
        Value::Bool(false),
        Value::UInt32(u32::MAX),
        Value::Double(-2.5e-300),
        Value::Float(0.5),
        Value::String("k–ω $\\alpha$".to_owned()),
        Value::PanelContent(PanelContent::Legend(PanelLegend {
            boxed: false,
            layout: LegendEntryLayout {
                columns: LegendEntryColumns::Count(2),
                order: LegendEntryOrder::RowMajor,
            },
            entries: LegendEntries::Explicit(vec![LegendItem::artist(NodeId((1 << 53) + 7))]),
        })),
        Value::PanelLocation(PanelLocation::SouthWest),
        Value::PanelLegend(PanelLegend {
            boxed: true,
            layout: LegendEntryLayout {
                columns: LegendEntryColumns::Auto,
                order: LegendEntryOrder::ColumnMajor,
            },
            entries: LegendEntries::Explicit(vec![LegendItem::custom(
                LegendSample::swatch(blue),
                Text::plain("Lower $s$"),
            )]),
        }),
        Value::Doubles(vec![-1.0, 0.0, 2.5]),
        Value::TrackSizes(vec![TrackSize::Fit, TrackSize::Weight(2.5)]),
        Value::Strings(vec!["surface".to_owned(), "k–ω".to_owned()]),
        Value::Texts(vec![Text::plain("NACA 0012"), Text::new("$Re = 10^6$")]),
        Value::Text(Text::plain("Plain $5")),
        Value::Interpreter(Interpreter::None),
        Value::FigureSize(FigureSize {
            width_mm: 90.5,
            height_mm: 60.25,
        }),
        Value::FontSetId(FontSetId::StixTwo),
        Value::Color(Color::rgba(1.0, 0.0, 128.0 / 255.0, 64.0 / 255.0)),
        Value::TileLayout(TileLayout::new(2, 3)),
        Value::Links(vec![AxisLink {
            dimension: Dimension::Z,
            axes: vec![NodeId(4), NodeId(u64::MAX)],
        }]),
        Value::Parameters(BTreeMap::from([
            ("converged".to_owned(), Parameter::Bool(true)),
            ("reynolds_number".to_owned(), Parameter::Number(1.0e5)),
        ])),
        Value::Cell(Cell {
            row: 1,
            col: 2,
            row_span: 1,
            col_span: 2,
        }),
        Value::Projection(Projection::ThreeD {
            view3d: View3d {
                azimuth_deg: 10.0,
                elevation_deg: -20.0,
                zoom: 2.0,
                pan_x: 0.25,
                pan_y: -0.125,
            },
        }),
        Value::View3d(View3d::default()),
        Value::Axis(Axis {
            label: Some(Text::new("$t$")),
            scale: Scale::Log,
            limits: manual(1.0, 100.0),
            reverse: true,
            ticks: Ticks::Values(vec![1.0, 10.0, 100.0]),
            tick_labels: TickLabels::Text(vec![
                Text::plain("1"),
                Text::new("$10$"),
                Text::plain("100"),
            ]),
            grid: true,
        }),
        Value::Ticks(Ticks::Step {
            step: 0.2,
            origin: -0.1,
        }),
        Value::TickLabels(TickLabels::Text(vec![
            Text::plain("upper surface"),
            Text::new("$x/c$"),
        ])),
        Value::Aspect(Aspect::Ratio { y: 0.25, z: 3.0 }),
        Value::AspectAdjust(AspectAdjust::Limits),
        Value::TitleAlign(TitleAlign::Left),
        Value::Scale(Scale::Log),
        Value::Limits(manual(-1.0, 1.0)),
        Value::ColormapName(ColormapName::Coolwarm),
        Value::Legend(Legend::Inside(InsideLegend {
            location: LegendLocation::SouthWest,
            boxed: false,
            ..InsideLegend::default()
        })),
        Value::LegendLocation(LegendLocation::Best),
        Value::InsideLegend(InsideLegend {
            location: LegendLocation::Center,
            boxed: false,
            layout: LegendEntryLayout {
                columns: LegendEntryColumns::Count(3),
                order: LegendEntryOrder::RowMajor,
            },
            entries: LegendEntries::Explicit(vec![
                LegendItem {
                    full_row: true,
                    ..LegendItem::artist(NodeId((1 << 53) + 5))
                },
                LegendItem::custom(LegendSample::swatch(blue), Text::plain("Upper $s$")),
            ]),
        }),
        Value::LegendEntryLayout(LegendEntryLayout {
            columns: LegendEntryColumns::Auto,
            order: LegendEntryOrder::RowMajor,
        }),
        Value::LegendEntryColumns(LegendEntryColumns::Count(u32::MAX)),
        Value::LegendEntryOrder(LegendEntryOrder::RowMajor),
        Value::NodeId(NodeId((1 << 53) + 3)),
        Value::ColorSpec(ColorSpec::Rgba { color: blue }),
        Value::LineStyle(LineStyle {
            color: ColorSpec::Colormapped,
            width_pt: 1.25,
            dash: DashStyle::DashDot,
        }),
        Value::DashStyle(DashStyle::DashDotDot),
        Value::MarkerStyle(MarkerStyle {
            shape: MarkerShape::TriangleDown,
            size_pt: 6.0,
            face: ColorSpec::None,
            edge: ColorSpec::Rgba { color: blue },
        }),
        Value::MarkerShape(MarkerShape::Cross),
        Value::Coords(Coords::AxesFraction),
        Value::Grid(Grid::Curvilinear {
            x: NodeId(1),
            y: NodeId(2),
        }),
        Value::Levels(Levels::Explicit {
            values: vec![0.5, 1.5],
        }),
        Value::ContourPlacement(ContourPlacement::Plane { z: Some(-0.5) }),
        Value::QuiverScale(QuiverScale::Factor { value: 0.75 }),
        Value::ImagePlacement(ImagePlacement {
            plane: ImagePlane::Xz { y: Some(-0.5) },
            columns: Some(PixelRange {
                first: -1.5,
                last: 1.5,
            }),
            rows: None,
        }),
        Value::PixelRange(PixelRange {
            first: 2.0,
            last: -2.0,
        }),
        Value::ImagePlane(ImagePlane::Yz { x: Some(0.25) }),
        Value::OutOfRange(OutOfRange::Rgba {
            color: Color::rgba(1.0, 0.0, 0.0, 128.0 / 255.0),
        }),
        Value::ColorScaleOutOfRange(ColorScaleOutOfRange {
            policy: OutOfRange::Strict,
            show_key: true,
            label: Some(Text::new("$> 10$")),
        }),
        Value::ColorScaleKind(ColorScaleKind::Categorical(CategoricalColorScale {
            categories: vec![ColorCategory {
                color: blue,
                label: Text::plain("water"),
            }],
            outside: ColorScaleOutOfRange {
                policy: OutOfRange::Clamp,
                show_key: false,
                label: None,
            },
        })),
        Value::ContinuousColorScale(ContinuousColorScale {
            colormap: Colormap::Named {
                name: ColormapName::Magma,
                reverse: true,
            },
            limits: manual(1.0, 1000.0),
            norm: ColorScaleNorm::Log,
            below: ColorScaleOutOfRange {
                policy: OutOfRange::Rgba { color: blue },
                show_key: true,
                label: Some(Text::plain("too low")),
            },
            ..ContinuousColorScale::default()
        }),
        Value::DiscreteColorScale(DiscreteColorScale {
            levels: ColorScaleLevels::Auto {
                count: 4,
                limits: manual(-1.0, 1.0),
            },
            ticks: Ticks::Values(vec![-1.0, 1.0]),
            tick_labels: TickLabels::Text(vec![Text::plain("low"), Text::plain("high")]),
            ..DiscreteColorScale::default()
        }),
        Value::IndexedColorScale(IndexedColorScale {
            colors: vec![blue, Color::BLACK, Color::WHITE],
            ticks: Ticks::Step {
                step: 1.0,
                origin: 0.0,
            },
            ..IndexedColorScale::default()
        }),
        Value::CategoricalColorScale(CategoricalColorScale {
            categories: vec![
                ColorCategory {
                    color: Color::BLACK,
                    label: Text::plain("land"),
                },
                ColorCategory {
                    color: blue,
                    label: Text::new("$\\infty$"),
                },
            ],
            outside: ColorScaleOutOfRange {
                policy: OutOfRange::Transparent,
                show_key: true,
                label: Some(Text::plain("other")),
            },
        }),
        Value::Colormap(Colormap::Listed {
            colors: vec![blue, Color::rgba(1.0, 0.0, 0.0, 128.0 / 255.0)],
        }),
        Value::ColorScaleNorm(ColorScaleNorm::Log),
        Value::ColorScaleLevels(ColorScaleLevels::Explicit {
            values: vec![-2.0, 0.0, 2.5],
        }),
        Value::PanelColorbar(PanelColorbar {
            scale: NodeId((1 << 53) + 11),
            orientation: ColorbarOrientation::Horizontal,
            length: Some(0.25),
            title_placement: ColorbarTitlePlacement::Above,
        }),
        Value::ColorbarOrientation(ColorbarOrientation::Horizontal),
        Value::InsideColorbar(InsideColorbar {
            scale: NodeId(12),
            side: InsideColorbarSide::West,
            length: 0.625,
            title_placement: ColorbarTitlePlacement::Above,
        }),
        Value::InsideColorbarSide(InsideColorbarSide::East),
        Value::ColorbarTitlePlacement(ColorbarTitlePlacement::Above),
    ]
}

/// A transaction that holds every kind of edit, every kind of inserted node and a set of
/// every sample value, with NaN in a data array, arrays of 8-bit values in a replacement
/// and in an append, and identifiers above 2^53.
///
/// It exercises the encodings only and is not meant to be applied: its sets name paths
/// and nodes that do not match their values.
pub fn every_kind_transaction() -> Transaction {
    let kitchen_sink = kitchen_sink_figure();
    let mut edits: Vec<Edit> = sample_values()
        .into_iter()
        .enumerate()
        .map(|(k, value)| Edit::Set {
            node: NodeId(k as u64),
            path: path("x.limits.min"),
            value,
        })
        .collect();
    let big = NodeId(u64::MAX);
    edits.extend([
        Edit::Insert {
            parent: NodeId(1),
            index: Some(2),
            node: Node::Axes(Box::new(kitchen_sink.axes[0].clone())),
        },
        Edit::Insert {
            parent: NodeId((1 << 53) + 1),
            index: None,
            node: Node::Artist(kitchen_sink.axes[5].artists[1].clone()),
        },
        Edit::Remove { node: big },
        Edit::Move {
            node: NodeId(3),
            parent: NodeId(2),
            index: Some(0),
        },
        Edit::Move {
            node: NodeId(4),
            parent: big,
            index: None,
        },
        Edit::Insert {
            parent: NodeId(1),
            index: Some(0),
            node: Node::DataArray(DataArray {
                id: NodeId((1 << 53) + 9),
                name: Some("pressure, $p$ (Pa)".to_owned()),
                array: NdArray::vector(vec![f64::NAN, -0.0, 2.5]),
            }),
        },
        Edit::Insert {
            parent: NodeId(1),
            index: None,
            node: Node::Panel(kitchen_sink.panels[1].clone()),
        },
        Edit::Insert {
            parent: NodeId(1),
            index: Some(1),
            node: Node::ColorScale(kitchen_sink.color_scales[0].clone()),
        },
        Edit::ReplaceData {
            node: NodeId(u64::MAX),
            array: NdArray::vector(vec![1.0, f64::NAN, -3.5]),
        },
        Edit::ReplaceData {
            node: NodeId(3),
            array: NdArray::from_shape_u8(vec![2, 2], vec![0, 1, 254, 255])
                .expect("the shape matches the values"),
        },
        Edit::AppendData {
            node: NodeId(0),
            array: rows(2, 3, 0.5),
            retain: Some(10),
        },
        Edit::AppendData {
            node: NodeId(1),
            array: NdArray::vector(vec![]),
            retain: None,
        },
        Edit::AppendData {
            node: NodeId(3),
            array: NdArray::from_shape_u8(vec![1, 2], vec![7, 8])
                .expect("the shape matches the values"),
            retain: Some(3),
        },
    ]);
    tx(edits)
}

/// Returns the set of variant indices of the values set by a transaction.
pub fn value_variants_in(transaction: &Transaction) -> BTreeSet<usize> {
    transaction
        .edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::Set { value, .. } => Some(value_variant(value)),
            _ => None,
        })
        .collect()
}
