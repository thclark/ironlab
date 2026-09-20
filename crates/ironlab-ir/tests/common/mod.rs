//! Shared fixtures for the IR integration tests.
//!
//! Fixtures assign node and data identifiers explicitly rather than through
//! `Figure::alloc_node_id` and `Figure::add_data`, so that tests of other behaviour
//! do not depend on the allocation behaviour under test elsewhere.

#![allow(dead_code)]

pub mod edits;

use std::collections::BTreeMap;

use ironlab_ir::*;

/// The fields of the wire schema whose IR values may be absent (an optional title, label,
/// legend, display name, coordinate array, pixel range or plane offset), by full Protocol
/// Buffers name.
///
/// The list is written by hand, independently of the encoder and of the property
/// registry, so that the tests that use it check those against the IR rather than against
/// themselves.
pub const OPTIONAL_IN_IR: [&str; 23] = [
    "ironlab.ir.v0.Figure.title",
    "ironlab.ir.v0.Axes.title",
    "ironlab.ir.v0.Axes.legend",
    "ironlab.ir.v0.Axis.label",
    "ironlab.ir.v0.Line.display_name",
    "ironlab.ir.v0.Line.z",
    "ironlab.ir.v0.Scatter.display_name",
    "ironlab.ir.v0.Scatter.z",
    "ironlab.ir.v0.Contour.display_name",
    "ironlab.ir.v0.ContourPlacementPlane.z",
    "ironlab.ir.v0.Quiver.display_name",
    "ironlab.ir.v0.Quiver.z",
    "ironlab.ir.v0.Quiver.w",
    "ironlab.ir.v0.Surface.display_name",
    "ironlab.ir.v0.Surface.c",
    "ironlab.ir.v0.Image.display_name",
    "ironlab.ir.v0.IndexedImage.display_name",
    "ironlab.ir.v0.MappedImage.display_name",
    "ironlab.ir.v0.ImagePlacement.columns",
    "ironlab.ir.v0.ImagePlacement.rows",
    "ironlab.ir.v0.ImagePlaneXy.z",
    "ironlab.ir.v0.ImagePlaneXz.y",
    "ironlab.ir.v0.ImagePlaneYz.x",
];

/// Builds figures with explicitly numbered nodes and data arrays.
pub struct FigureBuilder {
    pub fig: Figure,
    next_node: u64,
    next_data: u64,
}

impl Default for FigureBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FigureBuilder {
    pub fn new() -> Self {
        let fig = Figure {
            id: NodeId(1),
            ..Figure::new()
        };
        Self {
            fig,
            next_node: 2,
            next_data: 0,
        }
    }

    /// Returns a fresh node identifier.
    pub fn node(&mut self) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        id
    }

    /// Inserts an array under a fresh data identifier.
    pub fn data(&mut self, array: NdArray) -> DataId {
        let id = DataId(self.next_data);
        self.next_data += 1;
        self.fig.data.insert(id, array);
        id
    }

    /// Inserts a one-dimensional array.
    pub fn vector(&mut self, values: &[f64]) -> DataId {
        self.data(NdArray::vector(values.to_vec()))
    }

    /// Inserts a row-major `ny` × `nx` array whose value in row `j` and column `i` is
    /// `f(j, i)`.
    pub fn matrix(&mut self, ny: usize, nx: usize, f: impl Fn(usize, usize) -> f64) -> DataId {
        let values = (0..ny)
            .flat_map(|j| (0..nx).map(move |i| (j, i)))
            .map(|(j, i)| f(j, i))
            .collect();
        self.data(NdArray::from_shape(vec![ny, nx], values).expect("the shape matches the values"))
    }

    /// Inserts an array of 8-bit values with the given shape.
    pub fn bytes(&mut self, shape: Vec<usize>, values: Vec<u8>) -> DataId {
        self.data(NdArray::from_shape_u8(shape, values).expect("the shape matches the values"))
    }

    /// Adds a 2D axes occupying one cell and returns its identifier.
    pub fn axes2d(&mut self, row: u32, col: u32) -> NodeId {
        let id = self.node();
        self.fig.axes.push(Axes {
            id,
            cell: Cell {
                row,
                col,
                row_span: 1,
                col_span: 1,
            },
            ..Axes::default()
        });
        id
    }

    /// Adds a 3D axes occupying one cell and returns its identifier.
    pub fn axes3d(&mut self, row: u32, col: u32) -> NodeId {
        let id = self.axes2d(row, col);
        self.axes(id).projection = Projection::ThreeD {
            view3d: View3d::default(),
        };
        id
    }

    /// Returns the axes with the given identifier, found without the API under test.
    pub fn axes(&mut self, id: NodeId) -> &mut Axes {
        self.fig
            .axes
            .iter_mut()
            .find(|a| a.id == id)
            .expect("fixture axes exists")
    }

    /// Appends an artist to an axes.
    pub fn push(&mut self, axes: NodeId, artist: Artist) {
        self.axes(axes).artists.push(artist);
    }

    pub fn build(self) -> Figure {
        self.fig
    }
}

/// Returns the values of an `f64` array, panicking for an array of 8-bit values.
pub fn floats(array: &NdArray) -> &[f64] {
    array.as_f64().expect("the array holds f64 values")
}

/// Returns the values of an `f64` array for modification, so that a fixture can place a
/// particular value in an existing array; panics for an array of 8-bit values.
pub fn floats_mut(array: &mut NdArray) -> &mut Vec<f64> {
    match &mut array.values {
        Values::F64(values) => values,
        Values::U8(_) => panic!("the fixture array holds 8-bit values, not floats"),
    }
}

/// Finds an axes by identifier without using the API under test.
pub fn find_axes(fig: &Figure, id: NodeId) -> &Axes {
    fig.axes
        .iter()
        .find(|a| a.id == id)
        .expect("fixture axes exists")
}

/// A valid figure with one 2D axes containing one line of three points.
///
/// Returns the figure, the axes identifier and the line identifier.
pub fn single_line_figure() -> (Figure, NodeId, NodeId) {
    let mut b = FigureBuilder::new();
    let axes = b.axes2d(0, 0);
    let x = b.vector(&[1.0, 2.0, 3.0]);
    let y = b.vector(&[1.0, 4.0, 9.0]);
    let line = b.node();
    b.push(
        axes,
        Artist::Line(Line {
            id: line,
            x,
            y,
            ..Line::default()
        }),
    );
    (b.build(), axes, line)
}

/// A figure with `n` 2D axes in one row, each with distinct manual x, y and z limits.
///
/// The axes at index `k` has limits `[k, k + 1]` on x, `[10k, 10k + 1]` on y and
/// `[100k, 100k + 1]` on z, so that every limit in the figure is distinguishable.
pub fn row_of_axes(n: u32) -> (Figure, Vec<NodeId>) {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout { rows: 1, cols: n };
    let ids: Vec<NodeId> = (0..n)
        .map(|k| {
            let id = b.axes2d(0, k);
            let k = f64::from(k);
            let axes = b.axes(id);
            axes.x.limits = Limits::Manual {
                min: k,
                max: k + 1.0,
            };
            axes.y.limits = Limits::Manual {
                min: 10.0 * k,
                max: 10.0 * k + 1.0,
            };
            axes.z.limits = Limits::Manual {
                min: 100.0 * k,
                max: 100.0 * k + 1.0,
            };
            id
        })
        .collect();
    (b.build(), ids)
}

/// Returns the limits of an axes along a dimension, found without the API under test.
pub fn limits_of(fig: &Figure, axes: NodeId, dimension: Dimension) -> Limits {
    let a = find_axes(fig, axes);
    match dimension {
        Dimension::X => a.x.limits,
        Dimension::Y => a.y.limits,
        Dimension::Z => a.z.limits,
    }
}

/// A valid figure that uses every artist variant and every variant of every enum in
/// the schema, including NaN data, non-default styles, links on every dimension, a
/// figure parameter of every kind, and images of every kind on the floor of a 2D axes
/// and on the walls of a 3D axes, of floats and of bytes, with every plane, every
/// out-of-range policy, and pixel ranges that are absent, ascending and mirrored.
pub fn kitchen_sink_figure() -> Figure {
    let mut b = FigureBuilder::new();
    b.fig.title = Some(Text::new(r"Every artist, $\alpha^2$"));
    b.fig.size = FigureSize {
        width_mm: 180.5,
        height_mm: 120.25,
    };
    b.fig.font_size_pt = 8.5;
    b.fig.background = Color::rgba(1.0, 1.0, 1.0, 128.0 / 255.0);
    b.fig.layout = TileLayout { rows: 3, cols: 4 };
    b.fig.provenance = Provenance {
        ironlab_version: "0.1.0".to_owned(),
        typesetter: "latex-rust 1.0.2".to_owned(),
        fonts: vec!["STIX Two Text".to_owned(), "STIX Two Math".to_owned()],
    };
    b.fig.parameters = BTreeMap::from([
        ("converged".to_owned(), Parameter::Bool(true)),
        ("mesh_cells".to_owned(), Parameter::Integer(-4_096)),
        ("reynolds_number".to_owned(), Parameter::Number(1.0e5)),
        ("solver".to_owned(), Parameter::String("k–ω SST".to_owned())),
    ]);

    // Shared data. Positive values so that log axes produce no warnings.
    let t = b.vector(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let t_nan = b.vector(&[1.0, 4.0, f64::NAN, 16.0, 25.0, 36.0, 49.0, 64.0, 81.0]);
    let (ny, nx) = (3, 4);
    let gx = b.vector(&[0.0, 1.0, 2.0, 3.0]);
    let gy = b.vector(&[0.0, 0.5, 1.0]);
    let field = b.matrix(ny, nx, |j, i| (j * nx + i) as f64 + 0.5);
    let cx = b.matrix(ny, nx, |j, i| i as f64 + 0.1 * j as f64);
    let cy = b.matrix(ny, nx, |j, i| j as f64 - 0.1 * i as f64);
    // An array of 8-bit values, the indices of a colour-indexed image, so that the
    // element type is exercised by every format and edit that covers the data table.
    let bytes = b.bytes(vec![2, 3], vec![0, 1, 2, 253, 254, 255]);
    // The pixels of true-colour images: 8-bit red, green and blue components of a 2 × 3
    // image, and floating-point red, green, blue and alpha components of a 2 × 2 image.
    let rgb_pixels = b.bytes(
        vec![2, 3, 3],
        vec![
            255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0, 0, 255, 255, 255, 0, 255,
        ],
    );
    let rgba_pixels = b.data(
        NdArray::from_shape(
            vec![2, 2, 4],
            vec![
                1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.5, 0.0, 0.0, 1.0, 0.25, 0.5, 0.5, 0.5, 0.0,
            ],
        )
        .expect("the shape matches the values"),
    );

    let shapes = [
        MarkerShape::None,
        MarkerShape::Circle,
        MarkerShape::Square,
        MarkerShape::Diamond,
        MarkerShape::TriangleUp,
        MarkerShape::TriangleDown,
        MarkerShape::Plus,
        MarkerShape::Cross,
        MarkerShape::Point,
    ];
    let dashes = [
        DashStyle::Solid,
        DashStyle::Dashed,
        DashStyle::Dotted,
        DashStyle::DashDot,
        DashStyle::None,
    ];
    let color_specs = [
        ColorSpec::Auto,
        ColorSpec::Rgba {
            color: Color::rgb(0.0, 114.0 / 255.0, 178.0 / 255.0),
        },
        ColorSpec::None,
        ColorSpec::Colormapped,
    ];

    // Axes 1: 2D lines on log-log axes with every marker, dash and colour spec.
    let lines = b.axes2d(0, 0);
    {
        let a = b.axes(lines);
        a.title = Some(Text::plain("Lines, $5 literal"));
        a.x = Axis {
            label: Some(Text::new("$t$ (s)")),
            scale: Scale::Log,
            limits: Limits::Manual {
                min: 0.5,
                max: 20.0,
            },
            grid: true,
        };
        a.y.scale = Scale::Log;
        a.box_ = false;
        a.colormap = ColormapName::Viridis;
        a.clim = Limits::Manual {
            min: -1.0,
            max: 1.0,
        };
        a.legend = Some(Legend {
            location: LegendLocation::NorthEast,
            boxed: false,
        });
    }
    for (k, shape) in shapes.into_iter().enumerate() {
        let id = b.node();
        b.push(
            lines,
            Artist::Line(Line {
                id,
                display_name: Some(Text::new(format!("series ${k}$"))),
                visible: k % 2 == 0,
                x: t,
                y: t_nan,
                z: None,
                line: LineStyle {
                    color: color_specs[k % color_specs.len()],
                    width_pt: 0.25 * (k + 1) as f64,
                    dash: dashes[k % dashes.len()],
                },
                marker: MarkerStyle {
                    shape,
                    size_pt: 3.0 + k as f64,
                    face: color_specs[(k + 1) % color_specs.len()],
                    edge: color_specs[(k + 2) % color_specs.len()],
                },
            }),
        );
    }

    // Axes 2: 2D scatter with both size and both colour variants.
    let scatters = b.axes2d(0, 1);
    b.axes(scatters).legend = Some(Legend {
        location: LegendLocation::NorthWest,
        boxed: true,
    });
    b.axes(scatters).colormap = ColormapName::Cividis;
    let sizes = b.vector(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let colours = b.vector(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]);
    let id = b.node();
    b.push(
        scatters,
        Artist::Scatter(Scatter {
            id,
            display_name: None,
            visible: true,
            x: t,
            y: t,
            z: None,
            size: ScatterSize::Scalar { value: 6.0 },
            color: ScatterColor::Spec {
                spec: ColorSpec::Rgba {
                    color: Color::BLACK,
                },
            },
            marker: MarkerStyle {
                shape: MarkerShape::Diamond,
                ..MarkerStyle::default()
            },
        }),
    );
    let id = b.node();
    b.push(
        scatters,
        Artist::Scatter(Scatter {
            id,
            x: t,
            y: t_nan,
            size: ScatterSize::Data { data: sizes },
            color: ScatterColor::Data { data: colours },
            ..Scatter::default()
        }),
    );

    // Axes 3: 2D contours on both grid kinds, both level kinds, filled and unfilled.
    let contours = b.axes2d(0, 2);
    b.axes(contours).legend = Some(Legend {
        location: LegendLocation::SouthEast,
        boxed: true,
    });
    b.axes(contours).colormap = ColormapName::Magma;
    let id = b.node();
    b.push(
        contours,
        Artist::Contour(Contour {
            id,
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: field,
            levels: Levels::Auto { count: 7 },
            fill: false,
            placement: ContourPlacement::Plane { z: None },
            ..Contour::default()
        }),
    );
    let id = b.node();
    b.push(
        contours,
        Artist::Contour(Contour {
            id,
            grid: Grid::Curvilinear { x: cx, y: cy },
            z: field,
            levels: Levels::Explicit {
                values: vec![1.0, 2.5, 4.0],
            },
            fill: true,
            placement: ContourPlacement::Plane { z: Some(-1.5) },
            ..Contour::default()
        }),
    );

    // Axes 4: 2D quivers with automatic and factor scaling.
    let quivers = b.axes2d(0, 3);
    b.axes(quivers).legend = Some(Legend {
        location: LegendLocation::SouthWest,
        boxed: true,
    });
    b.axes(quivers).colormap = ColormapName::Inferno;
    let id = b.node();
    b.push(
        quivers,
        Artist::Quiver(Quiver {
            id,
            x: t,
            y: t,
            u: sizes,
            v: colours,
            scale: QuiverScale::Auto,
            ..Quiver::default()
        }),
    );
    let id = b.node();
    b.push(
        quivers,
        Artist::Quiver(Quiver {
            id,
            x: t,
            y: t,
            u: colours,
            v: sizes,
            scale: QuiverScale::Factor { value: 0.5 },
            head_size: 0.2,
            ..Quiver::default()
        }),
    );

    // Axes 5: 3D line, scatter, contour at level and quiver without automatic scaling.
    let three = b.axes3d(1, 0);
    {
        let a = b.axes(three);
        a.projection = Projection::ThreeD {
            view3d: View3d {
                azimuth_deg: 45.0,
                elevation_deg: -15.0,
                zoom: 1.5,
                pan_x: 0.1,
                pan_y: -0.2,
            },
        };
        a.z = Axis {
            label: Some(Text::new("$z$")),
            scale: Scale::Log,
            limits: Limits::Auto,
            grid: true,
        };
        a.legend = Some(Legend {
            location: LegendLocation::North,
            boxed: true,
        });
        a.colormap = ColormapName::Plasma;
    }
    let id = b.node();
    b.push(
        three,
        Artist::Line(Line {
            id,
            x: t,
            y: t,
            z: Some(t_nan),
            ..Line::default()
        }),
    );
    let id = b.node();
    b.push(
        three,
        Artist::Scatter(Scatter {
            id,
            x: t,
            y: t,
            z: Some(t),
            ..Scatter::default()
        }),
    );
    let id = b.node();
    b.push(
        three,
        Artist::Contour(Contour {
            id,
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: field,
            placement: ContourPlacement::AtLevel,
            ..Contour::default()
        }),
    );
    let id = b.node();
    b.push(
        three,
        Artist::Quiver(Quiver {
            id,
            x: t,
            y: t,
            z: Some(t),
            u: sizes,
            v: sizes,
            w: Some(colours),
            scale: QuiverScale::Off,
            ..Quiver::default()
        }),
    );

    // Axes 6: 3D surfaces in surf and mesh styles on both grid kinds.
    let surfaces = b.axes3d(1, 1);
    b.axes(surfaces).legend = Some(Legend {
        location: LegendLocation::South,
        boxed: true,
    });
    b.axes(surfaces).colormap = ColormapName::Coolwarm;
    let id = b.node();
    b.push(
        surfaces,
        Artist::Surface(Surface {
            id,
            display_name: Some(Text::plain("surf")),
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: field,
            c: Some(cx),
            ..Surface::default()
        }),
    );
    let id = b.node();
    b.push(
        surfaces,
        Artist::Surface(Surface {
            id,
            display_name: Some(Text::plain("mesh")),
            visible: false,
            grid: Grid::Curvilinear { x: cx, y: cy },
            z: field,
            c: None,
            face: ColorSpec::Rgba {
                color: Color::WHITE,
            },
            edge: ColorSpec::Colormapped,
            edge_width_pt: 0.3,
        }),
    );
    // Images on the walls of the 3D axes: a hidden true-colour image of floating-point
    // RGBA pixels on the xz wall, a colour-indexed image of floating-point indices on
    // the yz wall, and a colour-mapped image on the xz wall that is strict only for
    // non-finite values, so that an edit of the axes' colour limits is not refused. With
    // the 2D images, every policy occurs at some category.
    let id = b.node();
    b.push(
        surfaces,
        Artist::Image(Image {
            id,
            display_name: None,
            visible: false,
            pixels: rgba_pixels,
            placement: ImagePlacement {
                plane: ImagePlane::Xz { y: Some(0.5) },
                columns: Some(PixelRange {
                    first: 0.0,
                    last: 3.0,
                }),
                rows: None,
            },
        }),
    );
    let id = b.node();
    b.push(
        surfaces,
        Artist::IndexedImage(IndexedImage {
            id,
            display_name: Some(Text::plain("classes")),
            indices: cx,
            placement: ImagePlacement {
                plane: ImagePlane::Yz { x: None },
                columns: None,
                rows: Some(PixelRange {
                    first: 3.0,
                    last: 1.0,
                }),
            },
            below: OutOfRange::Clamp,
            above: OutOfRange::Rgba {
                color: Color::rgb(0.0, 114.0 / 255.0, 178.0 / 255.0),
            },
            non_finite: OutOfRange::Transparent,
            ..IndexedImage::default()
        }),
    );
    let id = b.node();
    b.push(
        surfaces,
        Artist::MappedImage(MappedImage {
            id,
            values: field,
            placement: ImagePlacement {
                plane: ImagePlane::Xz { y: Some(-1.0) },
                columns: Some(PixelRange {
                    first: 0.0,
                    last: 3.0,
                }),
                rows: Some(PixelRange {
                    first: 0.0,
                    last: 2.0,
                }),
            },
            below: OutOfRange::Rgba {
                color: Color::rgb(0.0, 114.0 / 255.0, 178.0 / 255.0),
            },
            above: OutOfRange::Transparent,
            non_finite: OutOfRange::Strict,
            ..MappedImage::default()
        }),
    );

    // Axes 7 to 9: the remaining legend locations and colormaps, and a spanning cell.
    // The east axes holds the 2D images of 8-bit data: a true-colour image in the default
    // placement and a hidden colour-indexed image whose floor offset is ignored, whose
    // columns are mirrored and whose every policy is strict (which 8-bit indices cannot
    // violate). The west axes holds a colour-mapped image with both pixel ranges and a
    // lenient policy of each kind.
    let east = b.axes2d(1, 2);
    b.axes(east).legend = Some(Legend {
        location: LegendLocation::East,
        boxed: true,
    });
    b.axes(east).colormap = ColormapName::Gray;
    let id = b.node();
    b.push(
        east,
        Artist::Image(Image {
            id,
            display_name: Some(Text::plain("photo")),
            pixels: rgb_pixels,
            ..Image::default()
        }),
    );
    let id = b.node();
    b.push(
        east,
        Artist::IndexedImage(IndexedImage {
            id,
            visible: false,
            indices: bytes,
            placement: ImagePlacement {
                plane: ImagePlane::Xy { z: Some(-2.0) },
                columns: Some(PixelRange {
                    first: 2.0,
                    last: 0.0,
                }),
                rows: None,
            },
            below: OutOfRange::Strict,
            above: OutOfRange::Strict,
            non_finite: OutOfRange::Strict,
            ..IndexedImage::default()
        }),
    );
    let west = b.axes2d(1, 3);
    b.axes(west).legend = Some(Legend {
        location: LegendLocation::West,
        boxed: true,
    });
    let id = b.node();
    b.push(
        west,
        Artist::MappedImage(MappedImage {
            id,
            display_name: Some(Text::new("$T$ (K)")),
            values: field,
            placement: ImagePlacement {
                plane: ImagePlane::Xy { z: None },
                columns: Some(PixelRange {
                    first: -1.5,
                    last: 1.5,
                }),
                rows: Some(PixelRange {
                    first: 0.0,
                    last: 1.0,
                }),
            },
            below: OutOfRange::Transparent,
            above: OutOfRange::Clamp,
            non_finite: OutOfRange::Rgba {
                color: Color::rgba(1.0, 0.0, 0.0, 128.0 / 255.0),
            },
            ..MappedImage::default()
        }),
    );
    let best = b.axes2d(2, 0);
    {
        let a = b.axes(best);
        a.cell = Cell {
            row: 2,
            col: 0,
            row_span: 1,
            col_span: 3,
        };
        a.legend = Some(Legend {
            location: LegendLocation::Best,
            boxed: true,
        });
    }

    // Axes 10: an axes without a legend or title.
    let _plain = b.axes2d(2, 3);

    b.fig.links = vec![
        AxisLink {
            dimension: Dimension::X,
            axes: vec![lines, scatters],
        },
        AxisLink {
            dimension: Dimension::Y,
            axes: vec![contours, quivers, east],
        },
        AxisLink {
            dimension: Dimension::Z,
            axes: vec![three, surfaces],
        },
    ];

    b.build()
}

/// A valid figure that sets every variant of the placement and of the out-of-range
/// policies of images, beyond what the kitchen-sink figure holds: in a 3D axes, an artist
/// of each image kind on each plane, with and without an offset and with offsets of both
/// signs, the two mapped kinds cycling through the policies so that every policy occurs
/// at every category, and pixel ranges that are absent, ascending and mirrored; and in a
/// 2D axes, a true-colour image whose floor offset is ignored and whose single row has
/// coincident centres, a colour-indexed image of floating-point indices and a
/// colour-mapped image of 8-bit values, both in the default placement.
///
/// The strict policies fall on 8-bit indices and on finite values under automatic colour
/// limits, which cannot violate them, so the figure validates without issues.
pub fn image_variants_figure() -> Figure {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout { rows: 1, cols: 2 };
    let solid = b.axes3d(0, 0);
    let flat = b.axes2d(0, 1);

    // A 1 × 2 image of floating-point RGB pixels and a 2 × 1 image of 8-bit RGBA pixels
    // (the kitchen-sink figure pairs the element types the other way round), and 2 × 2
    // arrays of 8-bit indices, of floating-point indices and of values.
    let rgb = b.data(
        NdArray::from_shape(vec![1, 2, 3], vec![1.0, 0.0, 0.0, 0.0, 0.0, 1.0])
            .expect("the shape matches the values"),
    );
    let rgba = b.bytes(vec![2, 1, 4], vec![255, 0, 0, 255, 0, 0, 255, 128]);
    let byte_indices = b.bytes(vec![2, 2], vec![0, 1, 254, 255]);
    let float_indices = b.matrix(2, 2, |j, i| (2 * j + i) as f64 * 85.0);
    let values = b.matrix(2, 2, |j, i| (j as f64 - 0.5) * (i as f64 + 1.0));

    let planes = [
        ImagePlane::Xy { z: None },
        ImagePlane::Xy { z: Some(-0.5) },
        ImagePlane::Xz { y: None },
        ImagePlane::Xz { y: Some(1.5) },
        ImagePlane::Yz { x: None },
        ImagePlane::Yz { x: Some(-2.0) },
    ];
    let policies = [
        OutOfRange::Strict,
        OutOfRange::Transparent,
        OutOfRange::Clamp,
        OutOfRange::Rgba {
            color: Color::rgba(1.0, 0.0, 0.0, 128.0 / 255.0),
        },
    ];
    let ranges = [
        None,
        Some(PixelRange {
            first: 0.0,
            last: 1.0,
        }),
        Some(PixelRange {
            first: 1.0,
            last: -1.0,
        }),
    ];
    for (k, plane) in planes.into_iter().enumerate() {
        let id = b.node();
        b.push(
            solid,
            Artist::Image(Image {
                id,
                pixels: if k % 2 == 0 { rgb } else { rgba },
                placement: ImagePlacement {
                    plane,
                    columns: ranges[k % 3],
                    rows: ranges[(k + 1) % 3],
                },
                ..Image::default()
            }),
        );
        let id = b.node();
        b.push(
            solid,
            Artist::IndexedImage(IndexedImage {
                id,
                indices: byte_indices,
                placement: ImagePlacement {
                    plane,
                    columns: ranges[(k + 1) % 3],
                    rows: ranges[(k + 2) % 3],
                },
                below: policies[k % 4],
                above: policies[(k + 1) % 4],
                non_finite: policies[(k + 2) % 4],
                ..IndexedImage::default()
            }),
        );
        let id = b.node();
        b.push(
            solid,
            Artist::MappedImage(MappedImage {
                id,
                values,
                placement: ImagePlacement {
                    plane,
                    columns: ranges[(k + 2) % 3],
                    rows: ranges[k % 3],
                },
                below: policies[(k + 3) % 4],
                above: policies[(k + 2) % 4],
                non_finite: policies[(k + 1) % 4],
                ..MappedImage::default()
            }),
        );
    }

    let id = b.node();
    b.push(
        flat,
        Artist::Image(Image {
            id,
            pixels: rgb,
            placement: ImagePlacement {
                plane: ImagePlane::Xy { z: Some(5.0) },
                columns: Some(PixelRange {
                    first: 4.0,
                    last: 2.0,
                }),
                rows: Some(PixelRange {
                    first: 1.0,
                    last: 1.0,
                }),
            },
            ..Image::default()
        }),
    );
    let id = b.node();
    b.push(
        flat,
        Artist::IndexedImage(IndexedImage {
            id,
            indices: float_indices,
            ..IndexedImage::default()
        }),
    );
    let id = b.node();
    b.push(
        flat,
        Artist::MappedImage(MappedImage {
            id,
            values: byte_indices,
            ..MappedImage::default()
        }),
    );
    b.build()
}

/// Floating-point values whose bits a lossless format must preserve: NaNs of both signs
/// and with payloads (including a signalling NaN), both infinities, both zeros, the
/// smallest subnormals of both signs, and the extremes of the finite range.
pub const SPECIAL_F64: [f64; 12] = [
    f64::NAN,
    -f64::NAN,
    f64::from_bits(0x7ff8_dead_beef_0001),
    f64::from_bits(0x7ff0_0000_0000_0001),
    f64::INFINITY,
    f64::NEG_INFINITY,
    -0.0,
    0.0,
    f64::from_bits(1),
    f64::from_bits(0x8000_0000_0000_0001),
    f64::MAX,
    f64::MIN,
];

/// Calls `visit` with a path and a mutable reference for every `f64` held by the
/// figure outside colours: sizes, numeric figure parameters, limits, views, style widths
/// and sizes, artist parameters, explicit levels and every value of every `f64` data
/// array. An array of 8-bit values holds no floats and is skipped.
///
/// A single traversal serves both to set special values and to read them back, so
/// that a field cannot be set without also being checked.
pub fn visit_floats_mut(fig: &mut Figure, visit: &mut dyn FnMut(String, &mut f64)) {
    visit("size.width_mm".into(), &mut fig.size.width_mm);
    visit("size.height_mm".into(), &mut fig.size.height_mm);
    visit("font_size_pt".into(), &mut fig.font_size_pt);
    for (name, parameter) in fig.parameters.iter_mut() {
        if let Parameter::Number(value) = parameter {
            visit(format!("parameters[{name:?}]"), value);
        }
    }
    for (id, array) in fig.data.iter_mut() {
        if let Values::F64(values) = &mut array.values {
            for (i, value) in values.iter_mut().enumerate() {
                visit(format!("data[{}][{i}]", id.0), value);
            }
        }
    }
    for (a, axes) in fig.axes.iter_mut().enumerate() {
        let at = |field: &str| format!("axes[{a}].{field}");
        if let Projection::ThreeD { view3d } = &mut axes.projection {
            visit(at("view3d.azimuth_deg"), &mut view3d.azimuth_deg);
            visit(at("view3d.elevation_deg"), &mut view3d.elevation_deg);
            visit(at("view3d.zoom"), &mut view3d.zoom);
            visit(at("view3d.pan_x"), &mut view3d.pan_x);
            visit(at("view3d.pan_y"), &mut view3d.pan_y);
        }
        for (name, limits) in [
            ("x.limits", &mut axes.x.limits),
            ("y.limits", &mut axes.y.limits),
            ("z.limits", &mut axes.z.limits),
            ("clim", &mut axes.clim),
        ] {
            if let Limits::Manual { min, max } = limits {
                visit(at(&format!("{name}.min")), min);
                visit(at(&format!("{name}.max")), max);
            }
        }
        for (k, artist) in axes.artists.iter_mut().enumerate() {
            let at = |field: &str| format!("axes[{a}].artists[{k}].{field}");
            match artist {
                Artist::Line(line) => {
                    visit(at("line.width_pt"), &mut line.line.width_pt);
                    visit(at("marker.size_pt"), &mut line.marker.size_pt);
                }
                Artist::Scatter(scatter) => {
                    if let ScatterSize::Scalar { value } = &mut scatter.size {
                        visit(at("size.value"), value);
                    }
                    visit(at("marker.size_pt"), &mut scatter.marker.size_pt);
                }
                Artist::Contour(contour) => {
                    if let Levels::Explicit { values } = &mut contour.levels {
                        for (i, value) in values.iter_mut().enumerate() {
                            visit(at(&format!("levels[{i}]")), value);
                        }
                    }
                    if let ContourPlacement::Plane { z: Some(z) } = &mut contour.placement {
                        visit(at("placement.z"), z);
                    }
                    visit(at("line.width_pt"), &mut contour.line.width_pt);
                }
                Artist::Quiver(quiver) => {
                    if let QuiverScale::Factor { value } = &mut quiver.scale {
                        visit(at("scale.value"), value);
                    }
                    visit(at("line.width_pt"), &mut quiver.line.width_pt);
                    visit(at("head_size"), &mut quiver.head_size);
                }
                Artist::Surface(surface) => {
                    visit(at("edge_width_pt"), &mut surface.edge_width_pt);
                }
                Artist::Image(image) => visit_placement(visit, &at, &mut image.placement),
                Artist::IndexedImage(image) => visit_placement(visit, &at, &mut image.placement),
                Artist::MappedImage(image) => visit_placement(visit, &at, &mut image.placement),
            }
        }
    }
}

/// Calls `visit` with every `f64` of an image placement: the offset of the plane when it
/// has one, and the centres of the first and last pixels of each present range.
fn visit_placement(
    visit: &mut dyn FnMut(String, &mut f64),
    at: &dyn Fn(&str) -> String,
    placement: &mut ImagePlacement,
) {
    match &mut placement.plane {
        ImagePlane::Xy { z: Some(z) } => visit(at("placement.plane.z"), z),
        ImagePlane::Xz { y: Some(y) } => visit(at("placement.plane.y"), y),
        ImagePlane::Yz { x: Some(x) } => visit(at("placement.plane.x"), x),
        ImagePlane::Xy { z: None } | ImagePlane::Xz { y: None } | ImagePlane::Yz { x: None } => {}
    }
    for (name, range) in [
        ("columns", &mut placement.columns),
        ("rows", &mut placement.rows),
    ] {
        if let Some(range) = range {
            visit(at(&format!("placement.{name}.first")), &mut range.first);
            visit(at(&format!("placement.{name}.last")), &mut range.last);
        }
    }
}

/// Returns the path and bits of every `f64` that [`visit_floats_mut`] visits.
pub fn float_bits(fig: &Figure) -> Vec<(String, u64)> {
    let mut fig = fig.clone();
    let mut bits = Vec::new();
    visit_floats_mut(&mut fig, &mut |path, value| {
        bits.push((path, value.to_bits()))
    });
    bits
}

/// The kitchen-sink figure with the special values of [`SPECIAL_F64`] assigned in turn
/// to every `f64` field and array value, so that each kind of field holds each kind of
/// special value somewhere in the figure.
///
/// Such a figure is not valid and cannot be written as JSON, whose numbers exclude
/// non-finite values; it exists to test the losslessness of the binary format.
pub fn special_values_figure() -> Figure {
    let mut fig = kitchen_sink_figure();
    let mut next = SPECIAL_F64.iter().cycle();
    visit_floats_mut(&mut fig, &mut |_, value| {
        *value = *next.next().expect("the cycle is infinite");
    });
    fig
}
