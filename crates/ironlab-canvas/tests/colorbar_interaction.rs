//! Hovering and gestures on colourbars (stage 6 of the colourbar epic), tested without a GPU or a window.
//!
//! A colourbar is the scale's own axis: hovering it reads the scale, and dragging or scrolling along a continuous or
//! discrete bar pans or zooms the scale's limits exactly as the same gesture on an axes pans or zooms its limits. The
//! gesture is one `Set` of the scale's limits recorded in the view overlay, so the bar can never disagree with the
//! scale: every bar that shows the scale and every artist that maps through it is drawn from the one edited scale.
//!
//! Most tests here build their hit map by hand, as the axis gesture tests in `interaction.rs` do, so that their
//! geometry is explicit; the few that must prove the canvas and the compiler agree compile the figure instead.
//!
//! The API these tests assume, beyond what `interaction.rs` exercises:
//!
//! ```text
//! // crates/ironlab-scene/src/hit.rs (see crates/ironlab-scene/tests/compile/colorbar_hits.rs)
//! pub struct ColorbarHit { ..., pub map: Option<AxisMap>, pub edges: Vec<f64> }
//! pub struct ColorbarKeyHit { pub key: ColorScaleKey, pub rect: Rect, pub label: String }
//!
//! // crates/ironlab-canvas/src/interaction.rs
//! pub enum Tip { Point(Datatip), Pixel(PixelDatatip), Colorbar(ColorbarTip) }
//!
//! /// What a colourbar says under the pointer.
//! pub struct ColorbarTip {
//!     /// What holds the bar: a panel, an axes, or a legend whose bar item it is.
//!     pub owner: ColorbarOwner,
//!     /// The colour scale the bar shows.
//!     pub scale: NodeId,
//!     /// The source text of the scale's title, when it has one.
//!     pub title: Option<String>,
//!     /// Where a callout is anchored, in figure space: the point on the centre line of a continuous bar level with
//!     /// the pointer, or the centre of the piece read.
//!     pub position: Point,
//!     /// The piece read, in figure space — a band, an entry's block, a category's square or a key's square — or
//!     /// `None` on a continuous bar, which has no pieces.
//!     pub piece: Option<Rect>,
//!     pub reading: ColorbarReading,
//! }
//!
//! pub enum ColorbarReading {
//!     /// The value under the pointer on a continuous bar.
//!     Value(f64),
//!     /// The band under the pointer on a discrete bar, from its lower edge to its upper edge; a value on an edge
//!     /// belongs to the upper band, as the scale classifies it.
//!     Band { low: f64, high: f64 },
//!     /// The entry under the pointer on an indexed bar, with the label of the tick at that entry, when the scale
//!     /// labels its ticks explicitly and one of them falls there.
//!     Entry { index: usize, label: Option<String> },
//!     /// The category of the square under the pointer, with its label's source text.
//!     Category { index: usize, label: String },
//!     /// The out-of-range key under the pointer, with the label it is drawn with.
//!     Key { key: ColorScaleKey, label: String },
//! }
//!
//! // ColorbarTip and ColorbarReading derive Clone, Debug and PartialEq, as the other tips do.
//!
//! impl FigureState {
//!     /// What the colourbar under the pointer says, or `None` where no bar, square or key lies.
//!     pub fn colorbar_tip(&self, hit: &HitMap, at: Point) -> Option<ColorbarTip>;
//! }
//! ```
//!
//! The rules these tests pin, beyond those the plan states:
//!
//! - **A bar is on top.** `tip_at` asks the colourbars before the points and pixels, because a bar inside a plot
//!   area is drawn over the artists; the box behind an inside bar hides what lies beneath it, so over the box but off
//!   the bar and its keys nothing is read, and no gesture acts there, neither on the scale nor on the axes beneath.
//! - **What a bar reads.** A discrete bar reads its band rather than a value, because a band is one colour. Hovering
//!   a title or a tick label reads nothing.
//! - **Which bars take gestures.** Only the continuous and discrete bars of panels and axes. An indexed or
//!   categorical bar has no limits; a bar item of a legend is part of its legend, and a gesture over it does not
//!   touch its scale; a discrete scale with explicit edges has no limits to set either, so a gesture on it changes
//!   nothing and spends no undo step (the edges are the author's, and moving them all is not a pan).
//! - **One gesture whatever the tool.** A bar has one dimension and no camera, so a drag along it pans the scale with
//!   the Pan, Zoom and Rotate tools alike, and the Zoom tool draws no rubber band on a bar.
//! - **Resetting.** A double-click on a bar restores the scale's limits from the source, and Refit restores the
//!   limits of every scale with those of every axes, because the overlay counts the limits of a scale among the view
//!   entries (see `the_limits_of_a_colour_scale_are_view_entries_and_its_other_properties_are_not` in
//!   `crates/ironlab-ir/tests/overlay.rs`).

mod common;

use common::*;
use ironlab_canvas::interaction::{ColorbarReading, ColorbarTip, Tip};
use ironlab_canvas::{FigureState, Tool};
use ironlab_ir::{
    Artist, Axes, CategoricalColorScale, Cell, Color, ColorCategory, ColorScale, ColorScaleKey,
    ColorScaleKind, ColorScaleLevels, ColorScaleNorm, ColorbarOrientation, ContinuousColorScale,
    Dimension, DiscreteColorScale, Figure, ImagePlacement, IndexedColorScale, InsideColorbar,
    InsideColorbarSide, LegendEntries, LegendItem, Limits, NdArray, NodeId, Panel, PanelColorbar,
    PanelContent, PanelLegend, PanelLocation, PixelRange, ScalarImage, Text, TickLabels, Ticks,
    TileLayout, TrackSize, Value,
};
use ironlab_scene::display::{Point, Rect};
use ironlab_scene::hit::{
    AxesHit, AxesHitKind, AxisMap, ColorbarHit, ColorbarKeyHit, ColorbarOwner, HitMap, LegendOwner,
};

// ---------------------------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------------------------

const AXES: NodeId = NodeId(2);
const IMAGE: NodeId = NodeId(3);
/// A continuous scale of automatic limits, through which the image maps the values 0 to 10.
const AUTO: NodeId = NodeId(10);
/// A continuous scale of manual limits [1, 100] under a logarithmic norm.
const LOG: NodeId = NodeId(11);
/// A discrete scale of four automatic bands between automatic limits, which no artist uses: [0, 1].
const BANDS: NodeId = NodeId(12);
/// A discrete scale of the explicit edges 0, 0.5, 1 and 2.
const EDGES: NodeId = NodeId(13);
/// An indexed scale of four entries, whose ticks at entries 0 and 2 are labelled Low and High.
const PALETTE: NodeId = NodeId(14);
/// A categorical scale of the categories Water, Forest and Rock.
const CLASSES: NodeId = NodeId(15);
/// A continuous scale of manual limits [0, 10] titled Pressure.
const TITLED: NodeId = NodeId(16);
const BAR_PANEL: NodeId = NodeId(20);
const LEGEND_PANEL: NodeId = NodeId(21);

/// The plot rectangle of the axes.
const PLOT: Rect = Rect::new(50.0, 20.0, 200.0, 100.0);
/// A standing panel bar, right of the plot, as tall as it.
const BAR: Rect = Rect::new(300.0, 20.0, 10.0, 100.0);
/// A lying panel bar, below the plot, as wide as it.
const LYING: Rect = Rect::new(50.0, 140.0, 200.0, 10.0);
/// A standing inside bar along the east edge of the plot, and the box behind it.
const INSIDE: Rect = Rect::new(235.0, 30.0, 8.0, 80.0);
const BACKDROP: Rect = Rect::new(222.0, 25.0, 25.0, 90.0);

fn continuous(limits: Limits, norm: ColorScaleNorm) -> ColorScaleKind {
    ColorScaleKind::Continuous(ContinuousColorScale {
        limits,
        norm,
        ..ContinuousColorScale::default()
    })
}

fn scale(id: NodeId, kind: ColorScaleKind) -> ColorScale {
    ColorScale {
        id,
        kind,
        ..ColorScale::default()
    }
}

/// The scales of the fixture, as the constants above describe them.
fn scales() -> Vec<ColorScale> {
    let grey = Color::rgb(0.5, 0.5, 0.5);
    vec![
        scale(AUTO, continuous(Limits::Auto, ColorScaleNorm::Linear)),
        scale(LOG, continuous(manual(1.0, 100.0), ColorScaleNorm::Log)),
        scale(
            BANDS,
            ColorScaleKind::Discrete(DiscreteColorScale {
                levels: ColorScaleLevels::Auto {
                    count: 4,
                    limits: Limits::Auto,
                },
                ..DiscreteColorScale::default()
            }),
        ),
        scale(
            EDGES,
            ColorScaleKind::Discrete(DiscreteColorScale {
                levels: ColorScaleLevels::Explicit {
                    values: vec![0.0, 0.5, 1.0, 2.0],
                },
                ..DiscreteColorScale::default()
            }),
        ),
        scale(
            PALETTE,
            ColorScaleKind::Indexed(IndexedColorScale {
                colors: vec![grey; 4],
                ticks: Ticks::Values(vec![0.0, 2.0]),
                tick_labels: TickLabels::Text(vec![Text::plain("Low"), Text::plain("High")]),
                ..IndexedColorScale::default()
            }),
        ),
        scale(
            CLASSES,
            ColorScaleKind::Categorical(CategoricalColorScale {
                categories: ["Water", "Forest", "Rock"]
                    .into_iter()
                    .map(|label| ColorCategory {
                        color: grey,
                        label: Text::plain(label),
                    })
                    .collect(),
                ..CategoricalColorScale::default()
            }),
        ),
        ColorScale {
            title: Some(Text::plain("Pressure")),
            ..scale(
                TITLED,
                continuous(manual(0.0, 10.0), ColorScaleNorm::Linear),
            )
        },
    ]
}

/// A figure of one 2D axes (x ∈ [0, 10], y ∈ [0, 5]) holding an image of the values 0 to 10 through [`AUTO`] that
/// covers the whole plot area, a colourbar panel of [`AUTO`] and a legend panel listing [`BANDS`], with every scale
/// above.
fn figure() -> Figure {
    let values = NdArray::from_shape(vec![2, 3], vec![0.0, 4.0, 10.0, 10.0, 4.0, 0.0])
        .expect("the shape matches the values");
    Figure {
        id: NodeId(1),
        layout: TileLayout::new(1, 3),
        data: vec![data_array(array_id(0), values)],
        color_scales: scales(),
        axes: vec![Axes {
            artists: vec![Artist::ScalarImage(ScalarImage {
                id: IMAGE,
                values: array_id(0),
                color_scale: Some(AUTO),
                // Three columns of pitch 10/3 and two rows of pitch 2.5 cover x ∈ [0, 10] and y ∈ [0, 5], so that
                // an inside bar anywhere in the plot area lies over the image.
                placement: ImagePlacement {
                    columns: Some(PixelRange {
                        first: 5.0 / 3.0,
                        last: 25.0 / 3.0,
                    }),
                    rows: Some(PixelRange {
                        first: 1.25,
                        last: 3.75,
                    }),
                    ..ImagePlacement::default()
                },
                ..ScalarImage::default()
            })],
            ..axes_2d(AXES.0)
        }],
        panels: vec![
            Panel {
                id: BAR_PANEL,
                cell: Cell {
                    col: 1,
                    ..Cell::default()
                },
                align: PanelLocation::Center,
                title: None,
                title_align: ironlab_ir::TitleAlign::Left,
                content: PanelContent::Colorbar(PanelColorbar {
                    scale: AUTO,
                    orientation: ColorbarOrientation::Vertical,
                    ..PanelColorbar::default()
                }),
            },
            Panel {
                id: LEGEND_PANEL,
                cell: Cell {
                    col: 2,
                    ..Cell::default()
                },
                align: PanelLocation::Center,
                title: None,
                title_align: ironlab_ir::TitleAlign::Left,
                content: PanelContent::Legend(PanelLegend {
                    entries: LegendEntries::Explicit(vec![LegendItem::color_scale(BANDS)]),
                    ..PanelLegend::default()
                }),
            },
        ],
        ..Figure::new()
    }
}

/// The mapping of `[min, max]` along a bar from its low end: the bottom of a standing bar, the left of a lying one.
fn along(bar: Rect, orientation: ColorbarOrientation, min: f64, max: f64, log: bool) -> AxisMap {
    let (start, end) = match orientation {
        ColorbarOrientation::Vertical => (bar.bottom(), bar.y),
        ColorbarOrientation::Horizontal => (bar.x, bar.right()),
    };
    AxisMap {
        min,
        max,
        log,
        start,
        end,
    }
}

/// The hit geometry of a standing bar of `scale` held by `owner` over `bar`, mapping `[min, max]`.
fn standing(
    owner: ColorbarOwner,
    scale: NodeId,
    bar: Rect,
    min: f64,
    max: f64,
    log: bool,
) -> ColorbarHit {
    ColorbarHit {
        owner,
        scale,
        orientation: ColorbarOrientation::Vertical,
        bar,
        entries: vec![],
        keys: vec![],
        backdrop: None,
        map: Some(along(bar, ColorbarOrientation::Vertical, min, max, log)),
        edges: vec![],
    }
}

fn panel_bar(scale: NodeId, min: f64, max: f64, log: bool) -> ColorbarHit {
    standing(ColorbarOwner::Panel(BAR_PANEL), scale, BAR, min, max, log)
}

fn inside_bar(scale: NodeId, min: f64, max: f64) -> ColorbarHit {
    ColorbarHit {
        backdrop: Some(BACKDROP),
        ..standing(ColorbarOwner::Axes(AXES), scale, INSIDE, min, max, false)
    }
}

/// A hit map of the 2D axes at [`PLOT`] and the given bars.
fn hit_with(colorbars: Vec<ColorbarHit>) -> HitMap {
    HitMap {
        axes: vec![hit_2d(AXES.0, PLOT)],
        legend_entries: vec![],
        panels: vec![],
        artists: vec![],
        images: vec![],
        colorbars,
    }
}

/// The point on the centre line of a standing bar at fraction `f` of its length from the bottom.
fn up(bar: Rect, f: f64) -> Point {
    Point::new(bar.x + bar.width / 2.0, bar.bottom() - f * bar.height)
}

/// The limits of a continuous scale, or of the automatic band edges of a discrete one.
fn scale_limits(figure: &Figure, id: NodeId) -> Limits {
    figure
        .color_scale(id)
        .and_then(|scale| scale.kind.limits())
        .unwrap_or_else(|| panic!("the scale {id} has limits"))
}

#[track_caller]
fn manual_scale(figure: &Figure, id: NodeId) -> (f64, f64) {
    match scale_limits(figure, id) {
        Limits::Manual { min, max } => (min, max),
        Limits::Auto => panic!("the limits of {id} are automatic"),
    }
}

#[track_caller]
fn tip(state: &FigureState, hit: &HitMap, at: Point) -> ColorbarTip {
    state
        .colorbar_tip(hit, at)
        .unwrap_or_else(|| panic!("a colourbar is read at {at:?}"))
}

fn drag(state: &mut FigureState, hit: &HitMap, from: Point, to: Point) -> bool {
    state.drag_start(hit, from);
    let changed = state.drag_update(to);
    state.drag_end(to) || changed
}

// ---------------------------------------------------------------------------------------------------------------
// Reading a bar
// ---------------------------------------------------------------------------------------------------------------

// Why: a colourbar is how a reader turns a colour back into a number, and hovering the bar is the precise way to do
// it: the value under the pointer is the inverse of the mapping the bar was drawn with, low values at the bottom, so
// a quarter of the way up a bar of [0, 10] reads 2.5. The callout is anchored on the bar's centre line level with the
// pointer, so that it points at the colour read whichever side of the centre the pointer is.
#[test]
fn hovering_a_continuous_bar_reads_the_value_under_the_pointer() {
    let state = FigureState::new(figure());
    let hit = hit_with(vec![panel_bar(AUTO, 0.0, 10.0, false)]);
    let at = Point::new(BAR.x + 2.0, BAR.bottom() - 25.0);

    let read = tip(&state, &hit, at);
    assert_eq!(read.scale, AUTO);
    assert_eq!(read.owner, ColorbarOwner::Panel(BAR_PANEL));
    assert_eq!(read.title, None, "the scale has no title");
    match read.reading {
        ColorbarReading::Value(v) => assert_close(v, 2.5, EPS, "a quarter of the way up"),
        other => panic!("a continuous bar reads a value, not {other:?}"),
    }
    assert_eq!(read.piece, None, "a continuous bar has no pieces");
    assert_close(
        read.position.x,
        BAR.x + BAR.width / 2.0,
        EPS,
        "on the centre line",
    );
    assert_close(read.position.y, at.y, EPS, "level with the pointer");
    assert_eq!(state.overlay().entries(), [], "reading changes nothing");
}

// Why: a logarithmic scale spaces decades evenly along its bar, so the reading must be taken in logarithms: halfway
// up a bar of [1, 100] is 10, not 50.5.
#[test]
fn hovering_a_logarithmic_bar_reads_in_decades() {
    let state = FigureState::new(figure());
    let hit = hit_with(vec![panel_bar(LOG, 1.0, 100.0, true)]);
    match tip(&state, &hit, up(BAR, 0.5)).reading {
        ColorbarReading::Value(v) => assert_close(v, 10.0, 1e-9, "the middle decade"),
        other => panic!("{other:?}"),
    }
}

// Why: a lying bar has its low values at the left, as every lying bar in the figure does, so a reading a quarter of
// the way from the left is a quarter of the way up the range.
#[test]
fn hovering_a_lying_bar_reads_from_the_left() {
    let state = FigureState::new(figure());
    let hit = hit_with(vec![
        ColorbarHit {
            orientation: ColorbarOrientation::Horizontal,
            map: Some(along(
                LYING,
                ColorbarOrientation::Horizontal,
                0.0,
                10.0,
                false,
            )),
            ..panel_bar(AUTO, 0.0, 10.0, false)
        }
        .with_bar(LYING),
    ]);
    let at = Point::new(LYING.x + LYING.width / 4.0, LYING.y + LYING.height / 2.0);
    match tip(&state, &hit, at).reading {
        ColorbarReading::Value(v) => {
            assert_close(v, 2.5, EPS, "a quarter of the way from the left")
        }
        other => panic!("{other:?}"),
    }
}

/// Builder sugar so that a fixture can move a bar without repeating every field.
trait WithBar {
    fn with_bar(self, bar: Rect) -> Self;
}

impl WithBar for ColorbarHit {
    fn with_bar(self, bar: Rect) -> Self {
        ColorbarHit { bar, ..self }
    }
}

// Why: every value in one band of a discrete scale is drawn in the one colour of that band, so a single value would
// claim a precision the colour does not carry; the bar reads the band, from edge to edge, with its block outlined.
// Edges need not be evenly spaced, so the band is found from the edges the bar drew, and a value exactly on an edge
// belongs to the upper band, as the scale classifies it.
#[test]
fn hovering_a_discrete_bar_reads_the_band_under_the_pointer() {
    let state = FigureState::new(figure());
    let map = along(BAR, ColorbarOrientation::Vertical, 0.0, 2.0, false);
    let hit = hit_with(vec![ColorbarHit {
        edges: vec![0.0, 0.5, 1.0, 2.0],
        ..panel_bar(EDGES, 0.0, 2.0, false)
    }]);
    let at_value = |v: f64| Point::new(BAR.x + BAR.width / 2.0, map.to_figure(v));

    let read = tip(&state, &hit, at_value(0.75));
    assert_eq!(
        read.reading,
        ColorbarReading::Band {
            low: 0.5,
            high: 1.0
        }
    );
    let band = read.piece.expect("a band is outlined");
    assert_close(
        band.y,
        map.to_figure(1.0),
        EPS,
        "the band's top is its upper edge",
    );
    assert_close(
        band.bottom(),
        map.to_figure(0.5),
        EPS,
        "the band's bottom is its lower edge",
    );
    assert_eq!(
        (band.x, band.width),
        (BAR.x, BAR.width),
        "the band spans the bar's thickness"
    );

    assert_eq!(
        tip(&state, &hit, at_value(1.0)).reading,
        ColorbarReading::Band {
            low: 1.0,
            high: 2.0
        },
        "a value on an edge belongs to the upper band"
    );
    assert_eq!(
        tip(&state, &hit, at_value(1.9)).reading,
        ColorbarReading::Band {
            low: 1.0,
            high: 2.0
        },
        "a band twice as long as the others is read as one band"
    );
}

// Why: an indexed bar is a palette laid out as blocks, and the reader hovers to learn which entry a colour is, so
// the bar reads the entry whose block is under the pointer; where the scale names its ticks, the name of the tick at
// that entry is what the reader recognises, so it is read with the index, and an entry without a named tick has none.
#[test]
fn hovering_an_indexed_bar_reads_the_entry_and_the_name_of_its_tick() {
    let state = FigureState::new(figure());
    let hit = hit_with(vec![panel_bar(PALETTE, -0.5, 3.5, false)]);
    let block = BAR.height / 4.0;
    let in_block = |k: f64, f: f64| Point::new(BAR.x + 5.0, BAR.bottom() - (k + f) * block);

    assert_eq!(
        tip(&state, &hit, in_block(2.0, 0.5)).reading,
        ColorbarReading::Entry {
            index: 2,
            label: Some("High".to_owned())
        }
    );
    assert_eq!(
        tip(&state, &hit, in_block(1.0, 0.9)).reading,
        ColorbarReading::Entry {
            index: 1,
            label: None
        },
        "entry 1 has no named tick"
    );
    assert_eq!(
        tip(&state, &hit, in_block(0.0, 0.1)).reading,
        ColorbarReading::Entry {
            index: 0,
            label: Some("Low".to_owned())
        }
    );
    let read = tip(&state, &hit, in_block(3.0, 0.5));
    assert_eq!(
        read.reading,
        ColorbarReading::Entry {
            index: 3,
            label: None
        }
    );
    let piece = read.piece.expect("the entry's block is outlined");
    assert_close(piece.height, block, EPS, "one block of four");
    assert_close(piece.y, BAR.y, EPS, "the last entry is at the top");
}

// Why: a categorical key is a list of named squares, and the name is the whole meaning of the square; the gaps
// between squares belong to no category and read nothing.
#[test]
fn hovering_a_category_square_reads_its_label_and_a_gap_reads_nothing() {
    let state = FigureState::new(figure());
    let squares = vec![
        Rect::new(300.0, 20.0, 10.0, 10.0),
        Rect::new(300.0, 35.0, 10.0, 10.0),
        Rect::new(300.0, 50.0, 10.0, 10.0),
    ];
    let hit = hit_with(vec![
        ColorbarHit {
            entries: squares.clone(),
            map: None,
            ..panel_bar(CLASSES, 0.0, 0.0, false)
        }
        .with_bar(Rect::new(300.0, 20.0, 10.0, 40.0)),
    ]);

    let read = tip(&state, &hit, Point::new(305.0, 40.0));
    assert_eq!(
        read.reading,
        ColorbarReading::Category {
            index: 1,
            label: "Forest".to_owned()
        }
    );
    assert_eq!(read.piece, Some(squares[1]));
    assert_eq!(
        state.colorbar_tip(&hit, Point::new(305.0, 32.5)),
        None,
        "the gap between two squares is no category"
    );
}

// Why: a key stands for every value the bar cannot show, and its label is the only statement of which values those
// are; the reading is the label as drawn, the default one formatted as the ticks are or the author's own.
#[test]
fn hovering_a_key_reads_the_label_it_is_drawn_with() {
    let state = FigureState::new(figure());
    let above = Rect::new(300.0, 2.0, 10.0, 10.0);
    let non_finite = Rect::new(300.0, 125.0, 10.0, 10.0);
    let hit = hit_with(vec![ColorbarHit {
        keys: vec![
            ColorbarKeyHit {
                key: ColorScaleKey::Above,
                rect: above,
                label: "> 10".to_owned(),
            },
            ColorbarKeyHit {
                key: ColorScaleKey::NonFinite,
                rect: non_finite,
                label: "Missing".to_owned(),
            },
        ],
        ..panel_bar(AUTO, 0.0, 10.0, false)
    }]);

    let read = tip(&state, &hit, Point::new(305.0, 7.0));
    assert_eq!(
        read.reading,
        ColorbarReading::Key {
            key: ColorScaleKey::Above,
            label: "> 10".to_owned()
        }
    );
    assert_eq!(read.piece, Some(above));
    assert_eq!(
        tip(&state, &hit, Point::new(305.0, 130.0)).reading,
        ColorbarReading::Key {
            key: ColorScaleKey::NonFinite,
            label: "Missing".to_owned()
        }
    );
}

// Why: the title names what the colours measure, which a reading on its own does not say, so the tip carries it, as
// a point datatip carries the label of its series.
#[test]
fn a_bar_reading_carries_the_title_of_its_scale() {
    let state = FigureState::new(figure());
    let hit = hit_with(vec![panel_bar(TITLED, 0.0, 10.0, false)]);
    assert_eq!(
        tip(&state, &hit, up(BAR, 0.5)).title.as_deref(),
        Some("Pressure")
    );
}

// Why: the title and the tick labels beside a bar are text the reader can already read, and between the bar and a
// key lies nothing; a reading there would report a colour the pointer is not on.
#[test]
fn beside_the_bar_and_between_its_pieces_nothing_is_read() {
    let state = FigureState::new(figure());
    let hit = hit_with(vec![ColorbarHit {
        keys: vec![ColorbarKeyHit {
            key: ColorScaleKey::Above,
            rect: Rect::new(300.0, 2.0, 10.0, 10.0),
            label: "> 10".to_owned(),
        }],
        ..panel_bar(AUTO, 0.0, 10.0, false)
    }]);
    for at in [
        Point::new(BAR.right() + 8.0, BAR.y + 50.0),
        Point::new(BAR.x - 6.0, BAR.y + 50.0),
        Point::new(305.0, 15.0),
        Point::new(305.0, BAR.bottom() + 5.0),
    ] {
        assert_eq!(
            state.colorbar_tip(&hit, at),
            None,
            "nothing is read at {at:?}"
        );
        assert_eq!(
            state.tip_at(&hit, at),
            None,
            "and nothing else either at {at:?}"
        );
    }
}

// Why: a bar item of a legend is drawn exactly as a colourbar is, and the reader reads it the same way; what it does
// not take is a gesture (below), because it belongs to its legend.
#[test]
fn a_bar_item_of_a_legend_is_read_as_any_bar_is() {
    let state = FigureState::new(figure());
    let hit = hit_with(vec![
        standing(
            ColorbarOwner::Legend(LegendOwner::Panel(LEGEND_PANEL)),
            BANDS,
            BAR,
            0.0,
            1.0,
            false,
        )
        .with_edges(vec![0.0, 0.25, 0.5, 0.75, 1.0]),
    ]);
    assert_eq!(
        tip(&state, &hit, up(BAR, 0.6)).reading,
        ColorbarReading::Band {
            low: 0.5,
            high: 0.75
        }
    );
}

trait WithEdges {
    fn with_edges(self, edges: Vec<f64>) -> Self;
}

impl WithEdges for ColorbarHit {
    fn with_edges(self, edges: Vec<f64>) -> Self {
        ColorbarHit { edges, ..self }
    }
}

// Why: an inside bar is drawn over the artists of its axes, on a box that hides them; what the reader points at
// there is the bar, or, on the box beside it, nothing they can see. The datatip must therefore read the bar over the
// image beneath it, read nothing over the box, and still read the image away from the box. The geometry comes from
// the compiler, so that the order of the checks is proved against what is really drawn.
#[test]
fn an_inside_bar_and_its_box_hide_the_image_beneath_them() {
    let mut figure = figure();
    figure.axes[0].colorbar = Some(InsideColorbar {
        scale: AUTO,
        side: InsideColorbarSide::East,
        length: 0.8,
        ..InsideColorbar::default()
    });
    let state = FigureState::new(figure);
    let hit = ironlab_scene::compile(state.figure(), &TEXT).hit_map;
    let bar = hit
        .colorbars
        .iter()
        .find(|c| c.owner == ColorbarOwner::Axes(AXES))
        .expect("the inside bar is drawn")
        .clone();
    let backdrop = bar.backdrop.expect("an inside bar has a box");

    let centre = Point::new(
        bar.bar.x + bar.bar.width / 2.0,
        bar.bar.y + bar.bar.height / 2.0,
    );
    assert!(
        hit.pixel_at(centre).is_some(),
        "the image lies beneath the bar"
    );
    assert!(
        matches!(state.tip_at(&hit, centre), Some(Tip::Colorbar(_))),
        "the bar is read, not the pixel beneath it"
    );

    let on_box = Point::new(backdrop.x + 1.0, centre.y);
    assert!(
        !bar.bar.contains(on_box),
        "the point lies on the box beside the bar"
    );
    assert!(
        hit.pixel_at(on_box).is_some(),
        "the image lies beneath the box"
    );
    assert_eq!(state.tip_at(&hit, on_box), None, "the box hides the image");

    let plot = hit.axes[0].plot_rect;
    let clear = Point::new(plot.x + 5.0, plot.y + plot.height / 2.0);
    assert!(
        matches!(state.tip_at(&hit, clear), Some(Tip::Pixel(_))),
        "away from the box the image is read as before"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Zooming and panning the limits of a scale
// ---------------------------------------------------------------------------------------------------------------

// Why: the wheel over a bar zooms the scale about the value under the pointer, as the wheel over an axes zooms it
// about the data point under the pointer, so that the colour the reader is looking at stays where it is. On a scale
// of automatic limits the zoom starts from the limits the reader sees and records them as manual, in one set of the
// scale's limits, while the source keeps its automatic limits so that resetting brings them back; the axes the
// pointer is not over is untouched.
#[test]
fn the_wheel_over_a_bar_zooms_the_scale_about_the_value_under_the_pointer() {
    let mut state = FigureState::new(figure());
    let hit = hit_with(vec![panel_bar(AUTO, 0.0, 10.0, false)]);
    let at = up(BAR, 0.3);
    let map = hit.colorbars[0].map.unwrap();
    let grabbed = map.to_data(at.y);

    assert!(state.scroll(&hit, at, 2.0));

    let (min, max) = manual_scale(state.figure(), AUTO);
    assert_close(max - min, 5.0, EPS, "the range halves");
    assert_close(
        AxisMap { min, max, ..map }.to_data(at.y),
        grabbed,
        EPS,
        "the value under the pointer stays under it",
    );
    let entries = state.overlay().entries();
    assert_eq!(entries.len(), 1, "one set: {entries:?}");
    assert_eq!(entries[0].node, AUTO);
    assert_eq!(entries[0].path.to_string(), "kind.continuous.limits");
    assert_eq!(entries[0].value, Value::Limits(manual(min, max)));
    assert_eq!(
        scale_limits(state.source(), AUTO),
        Limits::Auto,
        "the source keeps automatic limits"
    );
    assert_eq!(
        manual_of(state.figure(), AXES.0, Dimension::X),
        (0.0, 10.0),
        "the axes is not zoomed"
    );
}

// Why: a logarithmic scale must be zoomed in decades, as a logarithmic axis is, or the value under the pointer would
// slide and a zoom out could make the lower limit zero or negative, which a logarithmic scale cannot show.
#[test]
fn the_wheel_over_a_logarithmic_bar_zooms_in_decades_and_keeps_the_limits_positive() {
    let mut state = FigureState::new(figure());
    let hit = hit_with(vec![panel_bar(LOG, 1.0, 100.0, true)]);
    let at = up(BAR, 0.25);
    let map = hit.colorbars[0].map.unwrap();
    let grabbed = map.to_data(at.y);

    assert!(state.scroll(&hit, at, 2.0));
    let (min, max) = manual_scale(state.figure(), LOG);
    assert_close(
        max.log10() - min.log10(),
        1.0,
        1e-9,
        "two decades become one",
    );
    assert_close(
        AxisMap { min, max, ..map }.to_data(at.y),
        grabbed,
        1e-9 * grabbed,
        "the value under the pointer stays under it",
    );

    let zoomed = hit_with(vec![panel_bar(LOG, min, max, true)]);
    assert!(state.scroll(&zoomed, at, 1e-2));
    let (min, max) = manual_scale(state.figure(), LOG);
    assert!(
        min > 0.0 && min.is_finite() && max.is_finite() && min < max,
        "a far zoom out leaves positive limits: [{min}, {max}]"
    );
}

// Why: a drag along a bar slides the scale so that the value grabbed stays under the pointer, as a pan of an axes
// does. A bar has one dimension, so motion across it pans nothing; a drag that returns to where it began leaves the
// limits where they began; and however many updates the drag has, it is one step of the undo history, whose undo
// brings back the automatic limits.
#[test]
fn a_drag_along_a_bar_pans_the_scale_and_is_one_undo_step() {
    let mut state = FigureState::new(figure());
    let hit = hit_with(vec![panel_bar(AUTO, 0.0, 10.0, false)]);
    let map = hit.colorbars[0].map.unwrap();
    let start = up(BAR, 0.25);
    let grabbed = map.to_data(start.y);

    state.drag_start(&hit, start);
    assert!(
        !state.drag_update(Point::new(start.x + 30.0, start.y)),
        "motion across the bar pans nothing"
    );
    for k in 1..=40 {
        state.drag_update(Point::new(start.x, start.y - f64::from(k)));
    }
    let end = Point::new(start.x, start.y - 40.0);
    state.drag_end(end);

    let (min, max) = manual_scale(state.figure(), AUTO);
    assert_close(max - min, 10.0, EPS, "a pan keeps the range");
    assert_close(
        AxisMap { min, max, ..map }.to_figure(grabbed),
        end.y,
        1e-9,
        "the grabbed value is under the pointer",
    );
    assert_eq!(state.overlay().entries().len(), 1);

    assert!(state.undo());
    assert_eq!(
        scale_limits(state.figure(), AUTO),
        Limits::Auto,
        "one undo restores the automatic limits"
    );
    assert!(!state.can_undo(), "the whole drag was one step");
    assert!(state.redo());
    assert_eq!(manual_scale(state.figure(), AUTO), (min, max));

    let mut state = FigureState::new(figure());
    state.drag_start(&hit, start);
    assert!(state.drag_update(Point::new(start.x, start.y - 30.0)));
    state.drag_update(start);
    state.drag_end(start);
    let (min, max) = manual_scale(state.figure(), AUTO);
    assert_close(
        min,
        0.0,
        1e-9,
        "returning to the start restores the lower limit",
    );
    assert_close(max, 10.0, 1e-9, "and the upper limit");
}

// Why: the limits of a scale are the author's to choose, and nothing in the data bounds them: a reader may pan past
// the values drawn to see where they sit in a wider range. The drag must therefore not stop at the data, and the
// limits it reaches are accepted without a problem.
#[test]
fn a_pan_of_a_scale_stops_at_no_bound_of_the_data() {
    let mut state = FigureState::new(figure());
    let hit = hit_with(vec![panel_bar(AUTO, 0.0, 10.0, false)]);
    assert!(drag(&mut state, &hit, up(BAR, 0.5), up(BAR, 1.5)));
    let (min, max) = manual_scale(state.figure(), AUTO);
    assert_close(min, -10.0, 1e-9, "the range moved wholly below the data");
    assert_close(max, 0.0, 1e-9, "the range moved wholly below the data");
    assert!(state.problems().is_empty(), "{:?}", state.problems());
}

// Why: a bar has one dimension and no camera, so the gesture a drag makes along it is the same whatever tool is
// chosen; the Zoom tool's rubber band selects a rectangle of data, which a bar does not have, so none is drawn.
#[test]
fn a_drag_along_a_bar_pans_the_scale_with_every_tool_and_draws_no_rubber_band() {
    for tool in [Tool::Pan, Tool::Zoom, Tool::Rotate] {
        let mut state = FigureState::new(figure());
        state.tool = tool;
        let hit = hit_with(vec![panel_bar(AUTO, 0.0, 10.0, false)]);
        let start = up(BAR, 0.5);
        state.drag_start(&hit, start);
        assert!(
            state.drag_update(Point::new(start.x, start.y - 20.0)),
            "the {tool:?} tool pans the scale"
        );
        assert_eq!(
            state.rubber_band(),
            None,
            "no band is drawn with the {tool:?} tool"
        );
        state.drag_end(Point::new(start.x, start.y - 20.0));
        let (min, max) = manual_scale(state.figure(), AUTO);
        assert_close(
            max - min,
            10.0,
            EPS,
            "the {tool:?} tool pans rather than zooms",
        );
    }
}

// Why: the limits of a discrete scale with automatic levels are the range its bands span, and a gesture on its bar
// moves that range exactly as on a continuous bar, through the one property that holds it, so that the number of
// bands the author asked for is kept.
#[test]
fn a_gesture_on_a_discrete_bar_of_automatic_levels_sets_the_limits_of_its_levels() {
    let mut state = FigureState::new(figure());
    let hit = hit_with(vec![
        panel_bar(BANDS, 0.0, 1.0, false).with_edges(vec![0.0, 0.25, 0.5, 0.75, 1.0]),
    ]);
    assert!(state.scroll(&hit, up(BAR, 0.5), 2.0));

    let (min, max) = manual_scale(state.figure(), BANDS);
    assert_close(min, 0.25, EPS, "zoomed about the middle");
    assert_close(max, 0.75, EPS, "zoomed about the middle");
    let entries = state.overlay().entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path.to_string(), "kind.discrete.levels.limits");
    match &state.figure().color_scale(BANDS).unwrap().kind {
        ColorScaleKind::Discrete(DiscreteColorScale {
            levels: ColorScaleLevels::Auto { count, .. },
            ..
        }) => assert_eq!(*count, 4, "the number of bands is kept"),
        other => panic!("{other:?}"),
    }
}

// Why: some bars have no limits to move. An indexed or categorical scale is a list of entries; a discrete scale of
// explicit edges is the author's list of edges, which a pan would have to rewrite wholesale; and a bar item of a
// legend belongs to its legend. A gesture on any of them must change nothing and spend no undo step, so that a
// mistaken scroll is not something to undo.
#[test]
fn a_gesture_on_a_bar_without_limits_or_on_a_legend_bar_item_changes_nothing() {
    let legend_item = standing(
        ColorbarOwner::Legend(LegendOwner::Panel(LEGEND_PANEL)),
        BANDS,
        BAR,
        0.0,
        1.0,
        false,
    )
    .with_edges(vec![0.0, 0.5, 1.0]);
    for bar in [
        panel_bar(EDGES, 0.0, 2.0, false).with_edges(vec![0.0, 0.5, 1.0, 2.0]),
        panel_bar(PALETTE, -0.5, 3.5, false),
        ColorbarHit {
            map: None,
            entries: vec![BAR],
            ..panel_bar(CLASSES, 0.0, 0.0, false)
        },
        legend_item,
    ] {
        let what = format!("{:?} of {}", bar.owner, bar.scale);
        let mut state = FigureState::new(figure());
        let before = state.figure().clone();
        let hit = hit_with(vec![bar]);
        assert!(
            !state.scroll(&hit, up(BAR, 0.5), 2.0),
            "the wheel over {what}"
        );
        assert!(
            !drag(&mut state, &hit, up(BAR, 0.5), up(BAR, 0.8)),
            "a drag on {what}"
        );
        assert_eq!(state.figure(), &before, "{what} is unchanged");
        assert!(
            !state.can_undo(),
            "nothing to undo after gestures on {what}"
        );
    }
}

// Why: an inside bar lies over the plot area of its axes, and a gesture the reader makes on the bar is meant for the
// bar: it zooms or pans the scale, and leaves the limits of the axes beneath alone.
#[test]
fn a_gesture_on_an_inside_bar_moves_the_scale_and_not_the_axes_beneath() {
    let mut state = FigureState::new(figure());
    let hit = hit_with(vec![inside_bar(AUTO, 0.0, 10.0)]);
    assert!(PLOT.contains(up(INSIDE, 0.5)), "the bar lies over the plot");

    assert!(state.scroll(&hit, up(INSIDE, 0.5), 2.0));
    assert!(drag(&mut state, &hit, up(INSIDE, 0.5), up(INSIDE, 0.7)));
    assert!(matches!(
        scale_limits(state.figure(), AUTO),
        Limits::Manual { .. }
    ));
    assert_eq!(manual_of(state.figure(), AXES.0, Dimension::X), (0.0, 10.0));
    assert_eq!(manual_of(state.figure(), AXES.0, Dimension::Y), (0.0, 5.0));
    assert!(
        state
            .overlay()
            .entries()
            .iter()
            .all(|entry| entry.node == AUTO),
        "only the scale is edited: {:?}",
        state.overlay().entries()
    );
}

// Why: the box behind an inside bar hides the plot beneath it, so a gesture there is on neither the plot the reader
// cannot see nor the bar they are not on; it changes nothing.
#[test]
fn a_gesture_on_the_box_behind_an_inside_bar_changes_nothing() {
    let mut state = FigureState::new(figure());
    let hit = hit_with(vec![inside_bar(AUTO, 0.0, 10.0)]);
    let on_box = Point::new(BACKDROP.x + 2.0, BACKDROP.y + BACKDROP.height / 2.0);
    assert!(PLOT.contains(on_box) && !INSIDE.contains(on_box));

    assert!(!state.scroll(&hit, on_box, 2.0));
    assert!(!drag(
        &mut state,
        &hit,
        on_box,
        Point::new(on_box.x - 20.0, on_box.y + 10.0)
    ));
    assert_eq!(state.overlay().entries(), []);
}

// Why: a three-dimensional axes is navigated through its camera, but its inside bar is a scale like any other: the
// wheel and a drag on the bar move the scale's limits, whatever tool is active, and leave the camera alone.
#[test]
fn a_gesture_on_the_inside_bar_of_a_three_dimensional_axes_moves_the_scale_and_not_the_camera() {
    let mut figure = figure();
    figure.axes[0] = Axes {
        artists: figure.axes[0].artists.clone(),
        ..axes_3d(AXES.0)
    };
    let mut state = FigureState::new(figure);
    let before = view_of(state.figure(), AXES.0);
    let hit = HitMap {
        axes: vec![AxesHit {
            id: AXES,
            plot_rect: PLOT,
            kind: AxesHitKind::ThreeD,
        }],
        ..hit_with(vec![inside_bar(AUTO, 0.0, 10.0)])
    };

    assert!(state.scroll(&hit, up(INSIDE, 0.5), 2.0));
    let zoomed = manual_scale(state.figure(), AUTO);
    assert_close(zoomed.1 - zoomed.0, 5.0, EPS, "the scale is zoomed");
    state.tool = Tool::Rotate;
    assert!(drag(&mut state, &hit, up(INSIDE, 0.5), up(INSIDE, 0.6)));
    assert_ne!(
        manual_scale(state.figure(), AUTO),
        zoomed,
        "the scale is panned"
    );
    assert_eq!(
        view_of(state.figure(), AXES.0),
        before,
        "the camera is untouched"
    );
}

// Why: the hit map is compiled from the figure as it was drawn, and the next gesture may arrive before the figure is
// compiled again; manual limits are therefore read from the figure, as an axis gesture reads them, so that two quick
// wheel notches zoom twice rather than the second undoing the first.
#[test]
fn a_gesture_starts_from_the_limits_of_the_figure_not_from_a_stale_hit_map() {
    let mut state = FigureState::new(figure());
    let stale = hit_with(vec![panel_bar(AUTO, 0.0, 10.0, false)]);
    let at = up(BAR, 0.5);
    assert!(state.scroll(&stale, at, 2.0));
    assert!(state.scroll(&stale, at, 2.0));
    let (min, max) = manual_scale(state.figure(), AUTO);
    assert_close(max - min, 2.5, EPS, "two notches zoom by four");
}

// Why: a double-click on an axes restores its view, and a bar is the scale's axis, so a double-click on it restores
// the scale's limits from the source, as one step that can itself be undone; Refit restores the limits of every scale
// with the limits of every axes. Neither touches anything else the reader changed.
#[test]
fn a_double_click_on_a_bar_and_refit_restore_the_limits_of_the_scale() {
    let mut state = FigureState::new(figure());
    let hit = hit_with(vec![panel_bar(AUTO, 0.0, 10.0, false)]);
    record_limits(&mut state, AXES.0, Dimension::X, manual(2.0, 3.0));
    assert!(state.scroll(&hit, up(BAR, 0.5), 2.0));

    assert!(state.double_click(&hit, up(BAR, 0.5)));
    assert_eq!(
        scale_limits(state.figure(), AUTO),
        Limits::Auto,
        "the scale is automatic again"
    );
    assert_eq!(
        manual_of(state.figure(), AXES.0, Dimension::X),
        (2.0, 3.0),
        "the axes keeps its view"
    );
    assert!(state.undo(), "the restoration is a step of its own");
    assert!(matches!(
        scale_limits(state.figure(), AUTO),
        Limits::Manual { .. }
    ));

    assert!(state.reset_view());
    assert_eq!(
        scale_limits(state.figure(), AUTO),
        Limits::Auto,
        "Refit restores the scale"
    );
    assert_eq!(
        limits_of(state.figure(), AXES.0, Dimension::X),
        manual(0.0, 10.0),
        "and the axes"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// The bar never disagrees with the scale
// ---------------------------------------------------------------------------------------------------------------

// Why: this is what #8 asks of a colourbar whose limits change interactively. A scale may be shown at once by a panel
// colourbar, an inside colourbar and a bar item of a legend, and mapped through by an image; a gesture on one bar
// edits the one scale, so when the figure is compiled again every bar of the scale shows the new limits, and the
// value the reader zoomed about is still under the pointer on the bar they used. Undoing the gesture brings every bar
// back to the automatic limits together.
#[test]
fn after_a_gesture_every_bar_of_the_scale_shows_the_new_limits() {
    let mut figure = figure();
    figure.layout.col_weights = vec![TrackSize::Weight(1.0), TrackSize::Fit, TrackSize::Fit];
    figure.axes[0].colorbar = Some(InsideColorbar {
        scale: AUTO,
        side: InsideColorbarSide::West,
        length: 0.8,
        ..InsideColorbar::default()
    });
    if let PanelContent::Legend(legend) = &mut figure.panels[1].content {
        legend.entries = LegendEntries::Explicit(vec![LegendItem::color_scale(AUTO)]);
    }
    let mut state = FigureState::new(figure);
    let scene = ironlab_scene::compile(state.figure(), &TEXT);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    let hit = scene.hit_map;
    let owners = [
        ColorbarOwner::Panel(BAR_PANEL),
        ColorbarOwner::Axes(AXES),
        ColorbarOwner::Legend(LegendOwner::Panel(LEGEND_PANEL)),
    ];
    let bar_of = |hit: &HitMap, owner: ColorbarOwner| -> ColorbarHit {
        hit.colorbars
            .iter()
            .find(|c| c.owner == owner && c.scale == AUTO)
            .unwrap_or_else(|| panic!("{owner:?} draws a bar of the scale"))
            .clone()
    };
    let panel = bar_of(&hit, ColorbarOwner::Panel(BAR_PANEL));
    let map = panel.map.expect("the bar publishes its mapping");
    assert_eq!(
        (map.min, map.max),
        (0.0, 10.0),
        "the bar shows the automatic limits"
    );
    let at = up(panel.bar, 0.7);
    let value = map.to_data(at.y);

    assert!(state.scroll(&hit, at, 1.6));
    let (min, max) = manual_scale(state.figure(), AUTO);
    let again = ironlab_scene::compile(state.figure(), &TEXT).hit_map;
    for owner in owners {
        let map = bar_of(&again, owner)
            .map
            .expect("every bar publishes its mapping");
        assert_eq!(
            (map.min, map.max),
            (min, max),
            "{owner:?} shows the new limits"
        );
    }
    let panel = bar_of(&again, ColorbarOwner::Panel(BAR_PANEL));
    assert_close(
        panel.map.unwrap().to_data(at.y),
        value,
        1e-6,
        "the value zoomed about is still under the pointer",
    );

    assert!(state.undo());
    let undone = ironlab_scene::compile(state.figure(), &TEXT).hit_map;
    for owner in owners {
        let map = bar_of(&undone, owner).map.unwrap();
        assert_eq!(
            (map.min, map.max),
            (0.0, 10.0),
            "{owner:?} shows the automatic limits again"
        );
    }
}
