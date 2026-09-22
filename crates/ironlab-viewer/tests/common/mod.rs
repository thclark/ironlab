//! Shared fixtures for the viewer integration tests.
//!
//! Hit maps are built by hand rather than by compiling the figure, so that the interaction tests exercise only the
//! interaction logic and state their geometry explicitly.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, LazyLock};

use image::{Rgb, RgbImage};
use ironlab_ir::{
    Axes, Axis, AxisLink, Dimension, Edit, Figure, Limits, NodeId, Projection, PropertyPath,
    Transaction, Value, View3d, command,
};
use ironlab_scene::display::{
    GlyphsItem, ImageItem, Item, ItemKind, PlacedGlyph, Point, Rect, Rgba, Transform,
};
use ironlab_scene::hit::{AxesHit, AxesHitKind, AxisMap};
use ironlab_text::{TextEngine, TextItem};
use ironlab_viewer::{FigureState, RenderError, RenderedImage};

/// One text engine for the whole test binary; building it parses the bundled fonts.
pub static TEXT: LazyLock<Arc<TextEngine>> = LazyLock::new(|| Arc::new(TextEngine::new()));

/// Reports whether a missing graphics adapter fails a test rather than skipping it, which the environment variable
/// `IRONLAB_REQUIRE_GPU` asks for, as CI does.
pub fn gpu_required() -> bool {
    std::env::var_os("IRONLAB_REQUIRE_GPU").is_some()
}

/// Unwraps what an offscreen render produces or needs (an image, a renderer, a device): `None` (skipping the test)
/// when no adapter is available and a GPU is not required, and a panic on any other error.
pub fn gpu_or_skip<T>(result: Result<T, RenderError>) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(RenderError::NoAdapter(message)) if !gpu_required() => {
            eprintln!(
                "skipping: no graphics adapter ({message}); set IRONLAB_REQUIRE_GPU to make this a failure"
            );
            None
        }
        Err(error) => panic!("offscreen rendering failed: {error}"),
    }
}

/// Unwraps an offscreen render, or returns `None` (skipping the test) when no adapter is available and a GPU is not
/// required.
pub fn rendered_or_skip(result: Result<RenderedImage, RenderError>) -> Option<RenderedImage> {
    gpu_or_skip(result)
}

/// A run of the one glyph "H" of `size` points in `color` with its pen origin at `origin`, laid out by the text
/// engine so that the glyph id and font are real.
pub fn glyph_h(origin: Point, size: f64, color: Rgba) -> Item {
    let layout = TEXT.layout("H", false, size);
    let run = layout
        .items
        .iter()
        .find_map(|item| match item {
            TextItem::Glyphs(run) => Some(run),
            TextItem::Rule { .. } => None,
        })
        .expect("\"H\" lays out as a glyph run");
    Item {
        source: None,
        kind: ItemKind::Glyphs(GlyphsItem {
            font: run.font,
            size_pt: run.size_pt,
            color,
            text: "H".to_owned(),
            glyphs: vec![PlacedGlyph {
                id: run.glyphs[0].id,
                x: origin.x,
                y: origin.y,
                text_range: 0..1,
            }],
        }),
    }
}

/// A scaling by `sx` and `sy` followed by a translation to `(x, y)`.
pub fn scale_then_translate(sx: f64, sy: f64, x: f64, y: f64) -> Transform {
    Transform {
        a: sx,
        b: 0.0,
        c: 0.0,
        d: sy,
        e: x,
        f: y,
    }
}

pub const EPS: f64 = 1e-9;

pub fn manual(min: f64, max: f64) -> Limits {
    Limits::Manual { min, max }
}

/// A 2D axes with manual limits x ∈ [0, 10] and y ∈ [0, 5] and linear scales.
pub fn axes_2d(id: u64) -> Axes {
    Axes {
        id: NodeId(id),
        x: Axis {
            limits: manual(0.0, 10.0),
            ..Axis::default()
        },
        y: Axis {
            limits: manual(0.0, 5.0),
            ..Axis::default()
        },
        ..Axes::default()
    }
}

/// A 3D axes with the default view and manual limits on every axis.
pub fn axes_3d(id: u64) -> Axes {
    Axes {
        id: NodeId(id),
        projection: Projection::ThreeD {
            view3d: View3d::default(),
        },
        x: Axis {
            limits: manual(-1.0, 1.0),
            ..Axis::default()
        },
        y: Axis {
            limits: manual(-2.0, 2.0),
            ..Axis::default()
        },
        z: Axis {
            limits: manual(0.0, 3.0),
            ..Axis::default()
        },
        ..Axes::default()
    }
}

/// A figure of two axes and three artists, for the tests of the object tree and the
/// inspector.
///
/// The first axes (node 2) is two-dimensional, occupies the cell in row 0 and column 0,
/// is titled "Speed" and holds a line (node 4) named "Measured" and a hidden scatter
/// (node 5) with no name. The second axes (node 3) is three-dimensional, occupies row 0
/// and column 1, has no title, and holds a surface (node 6) over a rectilinear grid.
/// Every artist refers to data of the shapes the IR requires, so the figure is valid.
pub fn figure_with_artists() -> Figure {
    use ironlab_ir::{
        Artist, Cell, DataId, Grid, Line, NdArray, Scatter, Surface, Text, TileLayout,
    };

    let (x, y) = (DataId(0), DataId(1));
    let (gx, gy, field) = (DataId(2), DataId(3), DataId(4));
    let data = std::collections::BTreeMap::from([
        (x, NdArray::vector(vec![1.0, 2.0, 3.0])),
        (y, NdArray::vector(vec![1.0, 4.0, 9.0])),
        (gx, NdArray::vector(vec![0.0, 1.0, 2.0])),
        (gy, NdArray::vector(vec![0.0, 1.0])),
        (
            field,
            NdArray::from_shape(vec![2, 3], vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0])
                .expect("the shape matches the values"),
        ),
    ]);
    let flat = Axes {
        id: NodeId(2),
        title: Some(Text::plain("Speed")),
        artists: vec![
            Artist::Line(Line {
                id: NodeId(4),
                display_name: Some(Text::plain("Measured")),
                x,
                y,
                ..Line::default()
            }),
            Artist::Scatter(Scatter {
                id: NodeId(5),
                visible: false,
                x,
                y,
                ..Scatter::default()
            }),
        ],
        ..Axes::default()
    };
    let solid = Axes {
        cell: Cell {
            col: 1,
            ..Cell::default()
        },
        artists: vec![Artist::Surface(Surface {
            id: NodeId(6),
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: field,
            ..Surface::default()
        })],
        ..axes_3d(3)
    };
    Figure {
        id: NodeId(1),
        layout: TileLayout { rows: 1, cols: 2 },
        data,
        axes: vec![flat, solid],
        ..Figure::new()
    }
}

pub fn figure_with(axes: Vec<Axes>, links: Vec<AxisLink>) -> Figure {
    Figure {
        id: NodeId(1),
        axes,
        links,
        ..Figure::new()
    }
}

pub fn link(dimension: Dimension, ids: &[u64]) -> AxisLink {
    AxisLink {
        dimension,
        axes: ids.iter().copied().map(NodeId).collect(),
    }
}

/// A horizontal axis map from `[min, max]` onto the horizontal extent of `rect`.
pub fn x_map(rect: Rect, min: f64, max: f64, log: bool) -> AxisMap {
    AxisMap {
        min,
        max,
        log,
        start: rect.x,
        end: rect.right(),
    }
}

/// A vertical axis map from `[min, max]` onto the vertical extent of `rect`, with `min` at the bottom.
pub fn y_map(rect: Rect, min: f64, max: f64, log: bool) -> AxisMap {
    AxisMap {
        min,
        max,
        log,
        start: rect.bottom(),
        end: rect.y,
    }
}

/// Hit geometry of a linear 2D axes whose data limits are x ∈ [0, 10] and y ∈ [0, 5], as for [`axes_2d`].
pub fn hit_2d(id: u64, rect: Rect) -> AxesHit {
    AxesHit {
        id: NodeId(id),
        plot_rect: rect,
        kind: AxesHitKind::TwoD {
            x: x_map(rect, 0.0, 10.0, false),
            y: y_map(rect, 0.0, 5.0, false),
        },
    }
}

pub fn hit_3d(id: u64, rect: Rect) -> AxesHit {
    AxesHit {
        id: NodeId(id),
        plot_rect: rect,
        kind: AxesHitKind::ThreeD,
    }
}

pub fn limits_of(figure: &Figure, id: u64, dimension: Dimension) -> Limits {
    let axes = figure.axes(NodeId(id)).expect("axes exists");
    match dimension {
        Dimension::X => axes.x.limits,
        Dimension::Y => axes.y.limits,
        Dimension::Z => axes.z.limits,
    }
}

pub fn manual_of(figure: &Figure, id: u64, dimension: Dimension) -> (f64, f64) {
    match limits_of(figure, id, dimension) {
        Limits::Manual { min, max } => (min, max),
        Limits::Auto => panic!("expected manual limits on axes {id} {dimension:?}"),
    }
}

pub fn view_of(figure: &Figure, id: u64) -> View3d {
    match figure.axes(NodeId(id)).expect("axes exists").projection {
        Projection::ThreeD { view3d } => view3d,
        Projection::TwoD => panic!("axes {id} is not 3D"),
    }
}

pub fn set_view(figure: &mut Figure, id: u64, view: View3d) {
    figure.axes_mut(NodeId(id)).expect("axes exists").projection =
        Projection::ThreeD { view3d: view };
}

/// A property path, panicking with the text when it is not one.
pub fn path(text: &str) -> PropertyPath {
    text.parse().expect("a property path")
}

/// A transaction of one set of a property of a node.
pub fn set(node: u64, at: &str, value: Value) -> Transaction {
    Transaction {
        edits: vec![Edit::Set {
            node: NodeId(node),
            path: path(at),
            value,
        }],
    }
}

/// Records limits for an axes, and for the axes linked with it, as a gesture does.
///
/// Returns whether the displayed figure changed.
pub fn record_limits(
    state: &mut FigureState,
    id: u64,
    dimension: Dimension,
    limits: Limits,
) -> bool {
    let transaction = command::set_limits(state.figure(), NodeId(id), dimension, limits)
        .expect("the axes accepts the limits");
    state.record(&transaction)
}

pub fn assert_close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected}, got {actual} (tolerance {tolerance})"
    );
}

/// A figure that holds one artist of every kind, so that a test can exercise every
/// property the inspector can show.
///
/// A three-dimensional axes (node 2) holds a line (3), a scatter (4) whose size and
/// colour come from data, a contour (5) with explicit levels, a quiver (6) with a scale
/// factor, a surface (7), a true-colour image (8) of 8-bit RGBA pixels on the xz wall, a
/// colour-indexed image (9) of 8-bit indices on the yz wall and a colour-mapped image
/// (10) of the field on the floor, the images with both pixel ranges set and the two
/// mapped kinds with a fixed colour, a clamp and a transparent policy between them.
/// Every artist is three-dimensional or planar, so the figure is valid.
pub fn figure_with_every_artist() -> Figure {
    use ironlab_ir::{
        Artist, Color, Contour, Grid, Image, ImagePlacement, ImagePlane, IndexedImage, Levels,
        Line, MappedImage, NdArray, OutOfRange, PixelRange, Quiver, QuiverScale, Scatter,
        ScatterColor, ScatterSize, Surface,
    };
    use ironlab_ir::{ContourPlacement, DataId};

    let ids: Vec<DataId> = (0..10).map(DataId).collect();
    let (x, y, z, u, v, w) = (ids[0], ids[1], ids[2], ids[3], ids[4], ids[5]);
    let (gx, field, pixels, indices) = (ids[6], ids[7], ids[8], ids[9]);
    let vector = |start: f64| NdArray::vector(vec![start, start + 1.0, start + 2.0]);
    let data = std::collections::BTreeMap::from([
        (x, vector(1.0)),
        (y, vector(2.0)),
        (z, vector(3.0)),
        (u, vector(0.5)),
        (v, vector(1.5)),
        (w, vector(2.5)),
        (gx, vector(0.0)),
        (
            field,
            NdArray::from_shape(vec![3, 3], (0..9).map(f64::from).collect())
                .expect("the shape matches the values"),
        ),
        (
            pixels,
            NdArray::from_shape_u8(vec![2, 2, 4], (0..16).collect())
                .expect("the shape matches the values"),
        ),
        (
            indices,
            NdArray::from_shape_u8(vec![3, 3], (0..9).collect())
                .expect("the shape matches the values"),
        ),
    ]);
    let ranges = ImagePlacement {
        plane: ImagePlane::default(),
        columns: Some(PixelRange {
            first: -1.0,
            last: 1.0,
        }),
        rows: Some(PixelRange {
            first: 0.0,
            last: 3.0,
        }),
    };
    let artists = vec![
        Artist::Line(Line {
            id: NodeId(3),
            x,
            y,
            z: Some(z),
            ..Line::default()
        }),
        Artist::Scatter(Scatter {
            id: NodeId(4),
            x,
            y,
            z: Some(z),
            size: ScatterSize::Data { data: u },
            color: ScatterColor::Data { data: v },
            ..Scatter::default()
        }),
        Artist::Contour(Contour {
            id: NodeId(5),
            grid: Grid::Rectilinear { x: gx, y: gx },
            z: field,
            levels: Levels::Explicit {
                values: vec![1.0, 4.0],
            },
            placement: ContourPlacement::Plane { z: Some(0.0) },
            ..Contour::default()
        }),
        Artist::Quiver(Quiver {
            id: NodeId(6),
            x,
            y,
            z: Some(z),
            u,
            v,
            w: Some(w),
            scale: QuiverScale::Factor { value: 2.0 },
            ..Quiver::default()
        }),
        Artist::Surface(Surface {
            id: NodeId(7),
            grid: Grid::Rectilinear { x: gx, y: gx },
            z: field,
            c: Some(field),
            ..Surface::default()
        }),
        Artist::Image(Image {
            id: NodeId(8),
            pixels,
            placement: ImagePlacement {
                plane: ImagePlane::Xz { y: Some(0.0) },
                ..ranges
            },
            ..Image::default()
        }),
        Artist::IndexedImage(IndexedImage {
            id: NodeId(9),
            indices,
            placement: ImagePlacement {
                plane: ImagePlane::Yz { x: Some(0.5) },
                ..ranges
            },
            below: OutOfRange::Rgba {
                color: Color::BLACK,
            },
            above: OutOfRange::Clamp,
            non_finite: OutOfRange::Transparent,
            ..IndexedImage::default()
        }),
        Artist::MappedImage(MappedImage {
            id: NodeId(10),
            values: field,
            placement: ImagePlacement {
                plane: ImagePlane::Xy { z: Some(1.0) },
                ..ranges
            },
            below: OutOfRange::Transparent,
            above: OutOfRange::Transparent,
            non_finite: OutOfRange::Rgba {
                color: Color::BLACK,
            },
            ..MappedImage::default()
        }),
    ];
    Figure {
        id: NodeId(1),
        data,
        axes: vec![Axes {
            artists,
            ..axes_3d(2)
        }],
        ..Figure::new()
    }
}

/// A figure of many axes and many artists, for the tests of how the property editor
/// divides its height when the object tree is long.
///
/// Six two-dimensional axes fill a layout of two rows and three columns. Each is titled,
/// each holds four artists — two lines and two scatters, named so that a test can find
/// one of them — and all of them draw the same two data arrays, so that the figure is
/// valid and the object tree has thirty-one rows.
pub fn figure_with_many_axes() -> Figure {
    use ironlab_ir::{Artist, Cell, DataId, Line, NdArray, Scatter, Text, TileLayout};

    let (x, y) = (DataId(0), DataId(1));
    let data = std::collections::BTreeMap::from([
        (x, NdArray::vector(vec![1.0, 2.0, 3.0])),
        (y, NdArray::vector(vec![1.0, 4.0, 9.0])),
    ]);
    let mut next: u64 = 2;
    let mut all = Vec::new();
    for index in 0u32..6 {
        let id = next;
        next += 1;
        let mut artists = Vec::new();
        for plot in 0u32..4 {
            let artist = NodeId(next);
            next += 1;
            let name = Text::plain(format!("Plot {index}.{plot}"));
            artists.push(if plot.is_multiple_of(2) {
                Artist::Line(Line {
                    id: artist,
                    display_name: Some(name),
                    x,
                    y,
                    ..Line::default()
                })
            } else {
                Artist::Scatter(Scatter {
                    id: artist,
                    display_name: Some(name),
                    x,
                    y,
                    ..Scatter::default()
                })
            });
        }
        all.push(Axes {
            title: Some(Text::plain(format!("Axes {index}"))),
            cell: Cell {
                row: index / 3,
                col: index % 3,
                ..Cell::default()
            },
            artists,
            ..axes_2d(id)
        });
    }
    Figure {
        id: NodeId(1),
        layout: TileLayout { rows: 2, cols: 3 },
        data,
        axes: all,
        ..Figure::new()
    }
}

/// A figure of one two-dimensional axes (node 2) holding a colour-mapped image (node 3) of
/// `ny` rows and `nx` columns whose values are 0, 1, …, ny · nx − 1 in row order, placed by
/// default, so that the image fills the axes and neighbouring pixels differ in colour along
/// both axes: by one step of the colormap along a row and by `nx` steps down a column.
pub fn figure_with_mapped_image(ny: usize, nx: usize) -> Figure {
    use ironlab_ir::{Artist, DataId, MappedImage, NdArray};

    let values: Vec<f64> = (0..ny * nx).map(|k| k as f64).collect();
    let data = std::collections::BTreeMap::from([(
        DataId(0),
        NdArray::from_shape(vec![ny, nx], values).expect("the shape matches the values"),
    )]);
    Figure {
        id: NodeId(1),
        data,
        axes: vec![Axes {
            id: NodeId(2),
            artists: vec![Artist::MappedImage(MappedImage {
                id: NodeId(3),
                values: DataId(0),
                ..MappedImage::default()
            })],
            ..Axes::default()
        }],
        ..Figure::new()
    }
}

/// The first image item among `items`, at any depth of grouping, in paint order.
pub fn find_image(items: &[Item]) -> Option<&ImageItem> {
    items.iter().find_map(|item| match &item.kind {
        ItemKind::Image(image) => Some(image),
        ItemKind::Group { items, .. }
        | ItemKind::Dense { items, .. }
        | ItemKind::Depth { items } => find_image(items),
        ItemKind::Path(_) | ItemKind::Glyphs(_) => None,
    })
}

/// The colour of the pixel in row `row` and column `column` of an image item, with an alpha
/// of 255 for a three-channel image.
pub fn image_sample(image: &ImageItem, row: u32, column: u32) -> [u8; 4] {
    let channels = usize::from(image.channels);
    let start = (row as usize * image.width as usize + column as usize) * channels;
    let s = &image.samples[start..start + channels];
    [s[0], s[1], s[2], if channels == 4 { s[3] } else { 255 }]
}

/// A depth group holding `items`, as the scene compiler emits the artists of one three-dimensional axes: a backend
/// with a depth buffer clears the buffer at the group and tests every item inside against it.
pub fn depth_group(items: Vec<Item>) -> Item {
    Item {
        source: None,
        kind: ItemKind::Depth { items },
    }
}

/// A figure of one three-dimensional axes (node 2) holding one surface (node 3) over a 3 × 3 rectilinear grid whose
/// points all lie strictly inside the limits of [`axes_3d`], so that every face is drawn whole. With `visible` false
/// the surface is hidden, which leaves the axes without a depth group; the difference between the two renders is
/// therefore the surface alone.
pub fn figure_with_surface(visible: bool) -> Figure {
    use ironlab_ir::{Artist, DataId, Grid, NdArray, Surface};

    let (gx, gy, field) = (DataId(0), DataId(1), DataId(2));
    let data = std::collections::BTreeMap::from([
        (gx, NdArray::vector(vec![-0.5, 0.0, 0.5])),
        (gy, NdArray::vector(vec![-1.0, 0.0, 1.0])),
        (
            field,
            NdArray::from_shape(
                vec![3, 3],
                vec![0.5, 1.0, 1.5, 1.0, 1.5, 2.0, 1.5, 2.0, 2.5],
            )
            .expect("the shape matches the values"),
        ),
    ]);
    Figure {
        id: NodeId(1),
        data,
        axes: vec![Axes {
            artists: vec![Artist::Surface(Surface {
                id: NodeId(3),
                visible,
                grid: Grid::Rectilinear { x: gx, y: gy },
                z: field,
                ..Surface::default()
            })],
            ..axes_3d(2)
        }],
        ..Figure::new()
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Rasters of exported pages, for the tests that compare a PDF with what the viewer draws. They need poppler's tools
// on `PATH`; a missing tool skips a test unless `IRONLAB_REQUIRE_PDF_TOOLS` is set, as it is in CI.
// ---------------------------------------------------------------------------------------------------------------------

/// Reports whether a missing PDF tool fails a test rather than skipping it, which the environment variable
/// `IRONLAB_REQUIRE_PDF_TOOLS` asks for, as CI does.
pub fn tools_required() -> bool {
    std::env::var_os("IRONLAB_REQUIRE_PDF_TOOLS").is_some()
}

/// Reports whether every tool is on `PATH`, printing a skip message when one is missing and tools are not required.
pub fn tools_available(tools: &[&str]) -> bool {
    let missing: Vec<&str> = tools
        .iter()
        .copied()
        .filter(|tool| {
            std::env::var_os("PATH").is_none_or(|path| {
                !std::env::split_paths(&path).any(|dir| dir.join(tool).is_file())
            })
        })
        .collect();
    if missing.is_empty() {
        return true;
    }
    assert!(
        !tools_required(),
        "required PDF tools are missing from PATH: {missing:?}"
    );
    eprintln!("skipping: PDF tools missing from PATH: {missing:?}");
    false
}

/// A scratch directory for one test, removed unless the test panics, in which case its path is printed so that the
/// files it holds can be inspected.
pub struct Workspace(PathBuf);

impl Workspace {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ironlab-viewer-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("create the test workspace");
        Self(dir)
    }

    /// The path of `file` inside the directory.
    pub fn path(&self, file: &str) -> PathBuf {
        self.0.join(file)
    }

    /// Writes `bytes` as `<name>.pdf` inside the directory and returns the path of the file.
    pub fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(format!("{name}.pdf"));
        std::fs::write(&path, bytes).expect("write the PDF");
        path
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("test artefacts kept in {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// Runs a command to completion, panicking with its standard error when it fails, and returns its standard output.
pub fn run(command: &mut Command) -> String {
    let description = format!("{command:?}");
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("failed to run {description}: {e}"));
    assert!(
        output.status.success(),
        "{description} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Rasterises the single page of a PDF at `dpi` dots per inch.
///
/// Pages are compared at the resolution the figure was exported at, never below it. An embedded image is drawn with
/// `/Interpolate false`, so a PDF rasteriser asked for fewer pixels than the image has point-samples it and drops
/// the thin lines between faces, which would make a correctly placed raster look nothing like the paths it replaces.
pub fn rasterise(pdf: &Path, dpi: f64) -> RgbImage {
    let prefix = pdf.with_extension("");
    run(Command::new("pdftoppm")
        .args(["-r", &dpi.to_string(), "-png", "-singlefile"])
        .arg(pdf)
        .arg(&prefix));
    image::open(prefix.with_extension("png"))
        .expect("decode the rasterised page")
        .to_rgb8()
}

/// How much two rasters of the same page differ, in levels out of 255.
///
/// Block averages are compared rather than single pixels because two rasterisers of one page, the GPU and the PDF
/// rasteriser, do not resolve a boundary identically: an edge that falls inside a pixel is covered by the GPU's
/// multisampling in one and by the PDF rasteriser's own anti-aliasing in the other, and the two disagree by tens of
/// levels on that pixel alone. Averaging over a block conserves the ink, so a shape in the wrong place, at the wrong
/// scale or in the wrong colours still changes the blocks it covers, while a sub-pixel difference along a boundary
/// does not.
///
/// Both measures are needed. The mean over the page detects a shape that is displaced, rescaled or recoloured,
/// because such a shape disagrees with the other raster nearly everywhere. The worst block detects a local defect,
/// such as a corner of a surface left unrendered or a join missing from a stroke, which the rest of the page would
/// dilute out of the mean.
pub fn difference(a: &RgbImage, b: &RgbImage, block: u32) -> (f64, f64) {
    assert_eq!(a.dimensions(), b.dimensions(), "rasters of the same page");
    let (width, height) = a.dimensions();
    let mean = |image: &RgbImage, column: u32, row: u32, channel: usize| -> f64 {
        let total: u32 = (row * block..(row + 1) * block)
            .flat_map(|y| (column * block..(column + 1) * block).map(move |x| (x, y)))
            .map(|(x, y)| u32::from(image.get_pixel(x, y).0[channel]))
            .sum();
        f64::from(total) / f64::from(block * block)
    };
    let blocks: Vec<f64> = (0..height / block)
        .flat_map(|row| (0..width / block).map(move |column| (column, row)))
        .map(|(column, row)| {
            (0..3)
                .map(|channel| {
                    (mean(a, column, row, channel) - mean(b, column, row, channel)).abs()
                })
                .sum::<f64>()
                / 3.0
        })
        .collect();
    (
        blocks.iter().sum::<f64>() / blocks.len() as f64,
        blocks.iter().copied().fold(0.0, f64::max),
    )
}

/// The largest mean difference tolerated across the page, out of 255. A raster displaced by as little as a point, or
/// at a scale wrong by a percent, disagrees with the paths over the whole surface and moves this far beyond it.
pub const MEAN_TOLERANCE: f64 = 3.0;

/// The largest difference tolerated in any one block, out of 255. What remains within it is the disagreement between
/// two anti-aliasers where the projected faces are most foreshortened and several of them fall in one pixel, which no
/// correct implementation can remove.
pub const BLOCK_TOLERANCE: f64 = 16.0;

/// The side of the blocks compared, in points.
pub const BLOCK_PT: f64 = 6.0;

/// Asserts that two rasters of the same page, taken at `dpi`, show the same picture, naming `what` was compared
/// when they do not.
#[track_caller]
pub fn assert_same_picture(a: &RgbImage, b: &RgbImage, dpi: f64, what: &str) {
    let block = (BLOCK_PT * dpi / 72.0).round().max(1.0) as u32;
    let (mean, worst) = difference(a, b, block);
    assert!(
        mean <= MEAN_TOLERANCE && worst <= BLOCK_TOLERANCE,
        "{what}: the two rasters differ by {mean:.2} of 255 on average (tolerance {MEAN_TOLERANCE}) and by \
         {worst:.1} in the worst block of {BLOCK_PT} points square (tolerance {BLOCK_TOLERANCE})"
    );
}

/// The pixels of an offscreen render as an RGB image, which is what a raster of a PDF page is; the page is opaque.
pub fn rgb_of(rendered: &RenderedImage) -> RgbImage {
    RgbImage::from_fn(rendered.width, rendered.height, |x, y| {
        let [r, g, b, _] = rendered.pixel(x, y);
        Rgb([r, g, b])
    })
}
