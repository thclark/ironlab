//! Shared fixtures for the IR integration tests.
//!
//! Fixtures assign node identifiers, data arrays included, explicitly rather than
//! through `Figure::alloc_node_id` and `Figure::add_data`, so that tests of other
//! behaviour do not depend on the allocation behaviour under test elsewhere.

#![allow(dead_code)]

pub mod edits;

use std::collections::{BTreeMap, BTreeSet};

use ironlab_ir::*;

/// The fields of the wire schema whose IR values may be absent (an optional title, label,
/// legend, inside colourbar, label, name of a data array, coordinate array, colour or alpha array, colour
/// scale reference, contour levels, pixel range or plane offset, the colour data and
/// binning record of a bar series, the base and binning record of stairs, the binning record
/// of cells, and the ranges and minimum count of a binning record), by full Protocol Buffers
/// name.
///
/// The list is written by hand, independently of the encoder and of the property
/// registry, so that the tests that use it check those against the IR rather than against
/// themselves.
pub const OPTIONAL_IN_IR: [&str; 51] = [
    "ironlab.ir.v0.Figure.title",
    "ironlab.ir.v0.Panel.title",
    "ironlab.ir.v0.PanelColorbar.length",
    "ironlab.ir.v0.DataArray.name",
    "ironlab.ir.v0.Axes.title",
    "ironlab.ir.v0.Axes.legend",
    "ironlab.ir.v0.Axes.colorbar",
    "ironlab.ir.v0.Axis.label",
    "ironlab.ir.v0.Series.label",
    "ironlab.ir.v0.Series.z",
    "ironlab.ir.v0.Series.size_data",
    "ironlab.ir.v0.Series.color_data",
    "ironlab.ir.v0.Series.color_scale",
    "ironlab.ir.v0.Contour.label",
    "ironlab.ir.v0.Contour.levels",
    "ironlab.ir.v0.Contour.color_scale",
    "ironlab.ir.v0.ContourPlacementPlane.z",
    "ironlab.ir.v0.Quiver.label",
    "ironlab.ir.v0.Quiver.z",
    "ironlab.ir.v0.Quiver.w",
    "ironlab.ir.v0.Surface.label",
    "ironlab.ir.v0.Surface.c",
    "ironlab.ir.v0.Surface.face_alpha",
    "ironlab.ir.v0.Surface.edge_alpha",
    "ironlab.ir.v0.Surface.color_scale",
    "ironlab.ir.v0.Image.label",
    "ironlab.ir.v0.ScalarImage.label",
    "ironlab.ir.v0.ScalarImage.color_scale",
    "ironlab.ir.v0.ScalarImage.alpha",
    "ironlab.ir.v0.Bar.label",
    "ironlab.ir.v0.Bar.color_data",
    "ironlab.ir.v0.Bar.color_scale",
    "ironlab.ir.v0.Bar.binning",
    "ironlab.ir.v0.Stairs.label",
    "ironlab.ir.v0.Stairs.base",
    "ironlab.ir.v0.Stairs.binning",
    "ironlab.ir.v0.Cells.label",
    "ironlab.ir.v0.Cells.color_scale",
    "ironlab.ir.v0.Cells.alpha",
    "ironlab.ir.v0.Cells.binning",
    "ironlab.ir.v0.Binning.range",
    "ironlab.ir.v0.Binning2d.x_range",
    "ironlab.ir.v0.Binning2d.y_range",
    "ironlab.ir.v0.Binning2d.min_count",
    "ironlab.ir.v0.ColorScale.title",
    "ironlab.ir.v0.ColorScaleOutOfRange.label",
    "ironlab.ir.v0.ImagePlacement.columns",
    "ironlab.ir.v0.ImagePlacement.rows",
    "ironlab.ir.v0.ImagePlaneXy.z",
    "ironlab.ir.v0.ImagePlaneXz.y",
    "ironlab.ir.v0.ImagePlaneYz.x",
];

/// Builds figures with explicitly numbered nodes and data arrays.
///
/// Data arrays are nodes and share the one identifier space, but the builder numbers them
/// from 1000 rather than interleaving them with axes and artists, so that the identifiers
/// of axes and artists stay small and stable however many arrays a fixture adds. No
/// fixture holds anywhere near a thousand axes and artists, so the two ranges never meet.
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
            next_data: 1_000,
        }
    }

    /// Returns a fresh node identifier.
    pub fn node(&mut self) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        id
    }

    /// Adds an unnamed data array under a fresh node identifier.
    pub fn data(&mut self, array: NdArray) -> NodeId {
        let id = NodeId(self.next_data);
        self.next_data += 1;
        self.fig.data.push(DataArray {
            id,
            name: None,
            array,
        });
        id
    }

    /// Returns the data array with the given identifier, found without the API under test.
    pub fn data_array(&mut self, id: NodeId) -> &mut DataArray {
        self.fig
            .data
            .iter_mut()
            .find(|d| d.id == id)
            .expect("fixture data array exists")
    }

    /// Inserts a one-dimensional array.
    pub fn vector(&mut self, values: &[f64]) -> NodeId {
        self.data(NdArray::vector(values.to_vec()))
    }

    /// Inserts a row-major `ny` × `nx` array whose value in row `j` and column `i` is
    /// `f(j, i)`.
    pub fn matrix(&mut self, ny: usize, nx: usize, f: impl Fn(usize, usize) -> f64) -> NodeId {
        let values = (0..ny)
            .flat_map(|j| (0..nx).map(move |i| (j, i)))
            .map(|(j, i)| f(j, i))
            .collect();
        self.data(NdArray::from_shape(vec![ny, nx], values).expect("the shape matches the values"))
    }

    /// Inserts an array of 8-bit values with the given shape.
    pub fn bytes(&mut self, shape: Vec<usize>, values: Vec<u8>) -> NodeId {
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

impl FigureBuilder {
    /// Adds a colour scale of the given kind under a fresh node identifier, with no title
    /// and the default non-finite category (transparent, no key, default label).
    pub fn color_scale(&mut self, kind: ColorScaleKind) -> NodeId {
        let id = self.node();
        self.fig.color_scales.push(ColorScale {
            id,
            title: None,
            kind,
            non_finite: out_of_range(OutOfRange::Transparent),
        });
        id
    }

    /// Returns the colour scale with the given identifier, found without the API under test.
    pub fn scale(&mut self, id: NodeId) -> &mut ColorScale {
        self.fig
            .color_scales
            .iter_mut()
            .find(|s| s.id == id)
            .expect("fixture colour scale exists")
    }

    /// Adds a scalar image of the given values, mapped through the given scale (or none),
    /// in the default placement, and returns its identifier.
    pub fn scalar_image(
        &mut self,
        axes: NodeId,
        values: NodeId,
        color_scale: Option<NodeId>,
    ) -> NodeId {
        let id = self.node();
        self.push(
            axes,
            Artist::ScalarImage(ScalarImage {
                id,
                values,
                color_scale,
                ..ScalarImage::default()
            }),
        );
        id
    }
}

/// An out-of-range category of a colour scale with the given policy, no key and the
/// default label.
pub fn out_of_range(policy: OutOfRange) -> ColorScaleOutOfRange {
    ColorScaleOutOfRange {
        policy,
        show_key: false,
        label: None,
    }
}

/// A continuous scale with the defaults of its kind, except for the given colormap and
/// limits.
pub fn continuous(colormap: Colormap, limits: Limits) -> ColorScaleKind {
    ColorScaleKind::Continuous(ContinuousColorScale {
        colormap,
        limits,
        ..ContinuousColorScale::default()
    })
}

/// A discrete scale with the defaults of its kind, except for explicit band edges.
pub fn discrete(edges: &[f64]) -> ColorScaleKind {
    ColorScaleKind::Discrete(DiscreteColorScale {
        levels: ColorScaleLevels::Explicit {
            values: edges.to_vec(),
        },
        ..DiscreteColorScale::default()
    })
}

/// An indexed scale of the given colours whose out-of-range indices follow `outside`.
pub fn indexed(colors: Vec<Color>, outside: OutOfRange) -> ColorScaleKind {
    ColorScaleKind::Indexed(IndexedColorScale {
        colors,
        outside: out_of_range(outside),
        ..IndexedColorScale::default()
    })
}

/// A categorical scale of the given colours, labelled "category 0", "category 1", and so
/// on, whose out-of-range indices follow `outside`.
pub fn categorical(colors: Vec<Color>, outside: OutOfRange) -> ColorScaleKind {
    ColorScaleKind::Categorical(CategoricalColorScale {
        categories: colors
            .into_iter()
            .enumerate()
            .map(|(k, color)| ColorCategory {
                color,
                label: Text::plain(format!("category {k}")),
            })
            .collect(),
        outside: out_of_range(outside),
    })
}

/// The named colormap of the given name, not reversed.
pub fn named(name: ColormapName) -> Colormap {
    Colormap::Named {
        name,
        reverse: false,
    }
}

/// A valid figure that sets every variant of every colour-scale type: a scale of each of
/// the four kinds; both colormap variants, a named one reversed; both norms; automatic and
/// manual limits; automatic and explicit band edges, the automatic ones with manual
/// limits; every out-of-range policy at some category of some scale, with keys shown and
/// hidden and labels present and absent; a title present and absent; ticks and tick labels
/// of every kind on the kinds that have them; and one scale that no artist uses. Every
/// artist that takes a scale names one: scalar images of floats and of bytes, a filled
/// contour with no levels of its own on the discrete scale, a contour with explicit levels,
/// a series of markers coloured by data and a surface in a three-dimensional axes.
///
/// The strict policies fall on data that cannot violate them, so the figure validates
/// without issues.
pub fn color_scale_variants_figure() -> Figure {
    // Colours are multiples of 1/255, so that the eight-bit colour of the JSON form
    // reloads exactly.
    let red = Color::rgb(1.0, 0.0, 0.0);
    let green = Color::rgb(0.0, 1.0, 0.0);
    let blue = Color::rgb(0.0, 0.0, 1.0);
    let gray = Color::rgb(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0);

    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout::new(2, 2);
    let flat = b.axes2d(0, 0);
    let contours = b.axes2d(0, 1);
    let solid = b.axes3d(1, 0);
    let more = b.axes2d(1, 1);

    let magma = b.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Named {
            name: ColormapName::Magma,
            reverse: true,
        },
        limits: Limits::Manual {
            min: 0.0,
            max: 10.0,
        },
        norm: ColorScaleNorm::Linear,
        below: ColorScaleOutOfRange {
            policy: OutOfRange::Rgba { color: blue },
            show_key: true,
            label: Some(Text::plain("too cold")),
        },
        above: ColorScaleOutOfRange {
            policy: OutOfRange::Transparent,
            show_key: true,
            label: None,
        },
        ticks: Ticks::Step {
            step: 2.5,
            origin: 0.0,
        },
        tick_labels: TickLabels::Auto,
    }));
    b.scale(magma).title = Some(Text::new("$T$ (K)"));
    b.scale(magma).non_finite = ColorScaleOutOfRange {
        policy: OutOfRange::Rgba { color: gray },
        show_key: true,
        label: Some(Text::plain("no data")),
    };

    let listed_log = b.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Listed {
            colors: vec![red, green, blue],
        },
        limits: Limits::Auto,
        norm: ColorScaleNorm::Log,
        below: out_of_range(OutOfRange::Strict),
        above: out_of_range(OutOfRange::Clamp),
        ticks: Ticks::Values(vec![1.0, 10.0, 100.0]),
        tick_labels: TickLabels::Text(vec![
            Text::plain("one"),
            Text::new("$10$"),
            Text::plain("hundred"),
        ]),
    }));

    let bands = b.color_scale(ColorScaleKind::Discrete(DiscreteColorScale {
        colormap: named(ColormapName::Cividis),
        levels: ColorScaleLevels::Explicit {
            values: vec![0.0, 2.5, 5.0, 12.0],
        },
        norm: ColorScaleNorm::Linear,
        below: out_of_range(OutOfRange::Clamp),
        above: out_of_range(OutOfRange::Rgba { color: red }),
        ticks: Ticks::Auto,
        tick_labels: TickLabels::Hidden,
    }));
    b.scale(bands).title = Some(Text::plain("bands"));

    let log_bands = b.color_scale(ColorScaleKind::Discrete(DiscreteColorScale {
        colormap: Colormap::Listed {
            colors: vec![red, green],
        },
        levels: ColorScaleLevels::Auto {
            count: 5,
            limits: Limits::Manual {
                min: 1.0,
                max: 100.0,
            },
        },
        norm: ColorScaleNorm::Log,
        below: out_of_range(OutOfRange::Transparent),
        above: out_of_range(OutOfRange::Strict),
        ticks: Ticks::Values(vec![1.0, 100.0]),
        tick_labels: TickLabels::Auto,
    }));

    let four = b.color_scale(ColorScaleKind::Indexed(IndexedColorScale {
        colors: vec![red, green, blue, gray],
        outside: ColorScaleOutOfRange {
            policy: OutOfRange::Clamp,
            show_key: true,
            label: Some(Text::plain("clipped")),
        },
        ticks: Ticks::Values(vec![0.0, 3.0]),
        tick_labels: TickLabels::Text(vec![Text::plain("low"), Text::plain("high")]),
    }));

    // A 256-entry grey palette with the default, strict, policy outside its entries.
    let palette = b.color_scale(ColorScaleKind::Indexed(IndexedColorScale {
        colors: (0..=255u8)
            .map(|k| {
                let v = f32::from(k) / 255.0;
                Color::rgb(v, v, v)
            })
            .collect(),
        ..IndexedColorScale::default()
    }));

    let status = b.color_scale(ColorScaleKind::Categorical(CategoricalColorScale {
        categories: vec![
            ColorCategory {
                color: green,
                label: Text::plain("converged"),
            },
            ColorCategory {
                color: red,
                label: Text::new(r"$\infty$"),
            },
            ColorCategory {
                color: gray,
                label: Text::plain("not entered"),
            },
        ],
        outside: out_of_range(OutOfRange::Transparent),
    }));
    b.scale(status).non_finite = out_of_range(OutOfRange::Strict);

    // A scale that no artist uses, as a custom colourbar would need.
    let unused = b.node();
    b.fig.color_scales.push(ColorScale {
        id: unused,
        ..ColorScale::default()
    });

    // A field with values below, inside and above the manual limits of `magma`, and a NaN.
    let field = b.data(
        NdArray::from_shape(
            vec![3, 4],
            vec![
                -1.0,
                0.0,
                2.5,
                5.0,
                7.5,
                10.0,
                11.0,
                f64::NAN,
                1.0,
                2.0,
                3.0,
                4.0,
            ],
        )
        .expect("the shape matches the values"),
    );
    let alpha = b.matrix(3, 4, |j, i| (j * 4 + i) as f64 / 11.0);
    let bytes = b.bytes(vec![2, 2], vec![0, 1, 254, 255]);
    // Category codes, one of them outside the categories, which `status` leaves transparent.
    let codes = b.matrix(2, 2, |j, i| [0.0, 1.0, 2.0, 5.0][j * 2 + i]);
    let gx = b.vector(&[0.0, 1.0, 2.0, 3.0]);
    let gy = b.vector(&[0.0, 0.5, 1.0]);
    // A positive field, so that the logarithmic norms see no value they cannot place.
    let positive = b.matrix(3, 4, |j, i| 1.0 + 4.0 * (j * 4 + i) as f64);
    let ramp = b.matrix(3, 4, |j, i| (j * 4 + i) as f64);
    let px = b.vector(&[0.0, 1.0, 2.0, 3.0]);
    let classes = b.vector(&[0.0, 1.0, 2.0, 3.0]);

    let id = b.node();
    b.push(
        flat,
        Artist::ScalarImage(ScalarImage {
            id,
            label: Some(Text::plain("temperature")),
            visible: true,
            values: field,
            color_scale: Some(magma),
            placement: ImagePlacement {
                plane: ImagePlane::Xy { z: None },
                columns: Some(PixelRange {
                    first: 0.0,
                    last: 3.0,
                }),
                rows: Some(PixelRange {
                    first: 2.0,
                    last: 0.0,
                }),
            },
            alpha: Some(alpha),
            opacity: 0.5,
        }),
    );
    b.scalar_image(flat, bytes, Some(palette));
    b.scalar_image(more, codes, Some(status));

    let id = b.node();
    b.push(
        contours,
        Artist::Contour(Contour {
            id,
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: ramp,
            levels: None,
            fill: true,
            color_scale: Some(bands),
            ..Contour::default()
        }),
    );
    let id = b.node();
    b.push(
        contours,
        Artist::Contour(Contour {
            id,
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: positive,
            levels: Some(Levels::Explicit {
                values: vec![2.0, 20.0],
            }),
            color_scale: Some(log_bands),
            ..Contour::default()
        }),
    );
    let id = b.node();
    b.push(
        contours,
        Artist::Series(Series {
            id,
            x: px,
            y: px,
            line: LineStyle {
                color: ColorSpec::Colormapped,
                dash: DashStyle::None,
                ..LineStyle::default()
            },
            marker: MarkerStyle {
                shape: MarkerShape::Circle,
                ..MarkerStyle::default()
            },
            color_data: Some(classes),
            color_scale: Some(four),
            ..Series::default()
        }),
    );

    let id = b.node();
    b.push(
        solid,
        Artist::Surface(Surface {
            id,
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: positive,
            face: ColorSpec::Colormapped,
            edge: ColorSpec::Colormapped,
            color_scale: Some(listed_log),
            ..Surface::default()
        }),
    );
    b.build()
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

/// Finds the array of a data array by identifier without using the API under test.
pub fn array(fig: &Figure, id: NodeId) -> &NdArray {
    &fig.data
        .iter()
        .find(|d| d.id == id)
        .unwrap_or_else(|| panic!("{id} is not a data array of the figure"))
        .array
}

/// Finds the array of a data array by identifier for modification, without using the API
/// under test.
pub fn array_mut(fig: &mut Figure, id: NodeId) -> &mut NdArray {
    &mut fig
        .data
        .iter_mut()
        .find(|d| d.id == id)
        .unwrap_or_else(|| panic!("{id} is not a data array of the figure"))
        .array
}

/// Returns the data arrays an artist refers to, in the order of its fields, with
/// repeats, written out by hand so that tests do not depend on the validator's own list.
pub fn data_references(artist: &Artist) -> Vec<NodeId> {
    let grid = |grid: &Grid| match *grid {
        Grid::Rectilinear { x, y } | Grid::Curvilinear { x, y } => [x, y],
    };
    match artist {
        Artist::Series(a) => [a.x, a.y]
            .into_iter()
            .chain(a.z)
            .chain(a.size_data)
            .chain(a.color_data)
            .collect(),
        Artist::Contour(a) => grid(&a.grid).into_iter().chain([a.z]).collect(),
        Artist::Quiver(a) => [a.x, a.y]
            .into_iter()
            .chain(a.z)
            .chain([a.u, a.v])
            .chain(a.w)
            .collect(),
        Artist::Surface(a) => grid(&a.grid)
            .into_iter()
            .chain([a.z])
            .chain(a.c)
            .chain(a.face_alpha)
            .chain(a.edge_alpha)
            .collect(),
        Artist::Image(a) => vec![a.pixels],
        Artist::ScalarImage(a) => [a.values].into_iter().chain(a.alpha).collect(),
        Artist::Bar(a) => {
            let base = match a.base {
                PerBar::Constant(_) => None,
                PerBar::Data(id) => Some(id),
            };
            let slot = match a.width {
                BarWidth::Pitch(_) | BarWidth::Constant(_) => None,
                BarWidth::Data(id) => Some(id),
            };
            [a.positions, a.values]
                .into_iter()
                .chain(base)
                .chain(slot)
                .chain(a.color_data)
                .collect()
        }
        Artist::Stairs(a) => match a.data {
            StairsData::Edges { edges, values } => vec![edges, values],
            StairsData::Samples { x, values, .. } => vec![x, values],
        },
        Artist::Cells(a) => {
            let edges = |edges: &CellEdges| match *edges {
                CellEdges::Unit => None,
                CellEdges::Edges(id) => Some(id),
            };
            [a.values]
                .into_iter()
                .chain(edges(&a.columns))
                .chain(edges(&a.rows))
                .chain(a.alpha)
                .collect()
        }
    }
}

/// Returns the data references of an artist for modification, in the same order as
/// [`data_references`], written out by hand so that tests can retarget each reference in
/// turn without depending on the validator's own list.
pub fn data_references_mut(artist: &mut Artist) -> Vec<&mut NodeId> {
    fn grid(grid: &mut Grid) -> [&mut NodeId; 2] {
        match grid {
            Grid::Rectilinear { x, y } | Grid::Curvilinear { x, y } => [x, y],
        }
    }
    match artist {
        Artist::Series(a) => [&mut a.x, &mut a.y]
            .into_iter()
            .chain(a.z.as_mut())
            .chain(a.size_data.as_mut())
            .chain(a.color_data.as_mut())
            .collect(),
        Artist::Contour(a) => grid(&mut a.grid).into_iter().chain([&mut a.z]).collect(),
        Artist::Quiver(a) => [&mut a.x, &mut a.y]
            .into_iter()
            .chain(a.z.as_mut())
            .chain([&mut a.u, &mut a.v])
            .chain(a.w.as_mut())
            .collect(),
        Artist::Surface(a) => grid(&mut a.grid)
            .into_iter()
            .chain([&mut a.z])
            .chain(a.c.as_mut())
            .chain(a.face_alpha.as_mut())
            .chain(a.edge_alpha.as_mut())
            .collect(),
        Artist::Image(a) => vec![&mut a.pixels],
        Artist::ScalarImage(a) => [&mut a.values]
            .into_iter()
            .chain(a.alpha.as_mut())
            .collect(),
        Artist::Bar(a) => {
            let base = match &mut a.base {
                PerBar::Constant(_) => None,
                PerBar::Data(id) => Some(id),
            };
            let slot = match &mut a.width {
                BarWidth::Pitch(_) | BarWidth::Constant(_) => None,
                BarWidth::Data(id) => Some(id),
            };
            [&mut a.positions, &mut a.values]
                .into_iter()
                .chain(base)
                .chain(slot)
                .chain(a.color_data.as_mut())
                .collect()
        }
        Artist::Stairs(a) => match &mut a.data {
            StairsData::Edges { edges, values } => vec![edges, values],
            StairsData::Samples { x, values, .. } => vec![x, values],
        },
        Artist::Cells(a) => {
            fn edges(edges: &mut CellEdges) -> Option<&mut NodeId> {
                match edges {
                    CellEdges::Unit => None,
                    CellEdges::Edges(id) => Some(id),
                }
            }
            [&mut a.values]
                .into_iter()
                .chain(edges(&mut a.columns))
                .chain(edges(&mut a.rows))
                .chain(a.alpha.as_mut())
                .collect()
        }
    }
}

/// Removes every data array that no artist of the figure refers to, so that a fixture
/// built from a pool of arrays holds only what its artists use and validates without an
/// unused-data warning.
pub fn drop_unreferenced_data(fig: &mut Figure) {
    let used: BTreeSet<NodeId> = fig
        .axes
        .iter()
        .flat_map(|axes| axes.artists.iter().flat_map(data_references))
        .collect();
    fig.data.retain(|data| used.contains(&data.id));
}

/// A legend panel: a panel whose content is a legend with the defaults of
/// [`PanelLegend`], centred in the given cell.
pub fn legend_panel(id: NodeId, cell: Cell) -> Panel {
    Panel {
        id,
        cell,
        align: PanelLocation::Center,
        title: None,
        title_align: TitleAlign::Left,
        content: PanelContent::Legend(PanelLegend::default()),
    }
}

/// Returns the legend of a panel, panicking for a panel of another content.
pub fn panel_legend(panel: &Panel) -> &PanelLegend {
    match &panel.content {
        PanelContent::Legend(legend) => legend,
        PanelContent::Colorbar(_) => panic!("panel {} holds a colourbar", panel.id),
    }
}

/// Returns the legend of a panel for modification, panicking for a panel of another
/// content.
pub fn panel_legend_mut(panel: &mut Panel) -> &mut PanelLegend {
    let id = panel.id;
    match &mut panel.content {
        PanelContent::Legend(legend) => legend,
        PanelContent::Colorbar(_) => panic!("panel {id} holds a colourbar"),
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
        Artist::Series(Series {
            id: line,
            x,
            y,
            ..Series::default()
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
    b.fig.layout = TileLayout::new(1, n);
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
/// figure parameter of every kind, tags that are not in alphabetical order, data arrays
/// with and without a name, data shared by artists of several axes, colour scales of every
/// kind (named by the colour-mapped artists of every axes, with every colormap name, both
/// colormap variants, both norms, every out-of-range policy and two scales that no artist
/// names), and images of both kinds on the floor of a 2D axes and on the walls of a 3D
/// axes, of floats and of bytes, with every plane and pixel ranges that are absent,
/// ascending and mirrored. Its tile layout has weights for some of its rows and
/// all of its columns but the last, which is fitted, and its last row holds two legend
/// panels, which between them and the inside legends use every variant of the layout of
/// legend entries, each listing a named line of an axes whose legend names it; one panel
/// lists its entries automatically and the other lists an explicit item of every kind,
/// colour scales included.
/// Two colourbar panels follow the legend panels, one of each orientation, each showing
/// a scale that no artist names, one of them spanning two rows of the fitted column; and
/// four axes hold an inside colourbar, one on each side. The axes without a title or
/// legend holds bars, stairs and cells using every variant of their tagged values (see
/// [`add_bars_stairs_and_cells`]), with a hidden x axis and rotated y tick labels.
pub fn kitchen_sink_figure() -> Figure {
    let mut b = FigureBuilder::new();
    b.fig.title = Some(Text::new(r"Every artist, $\alpha^2$"));
    b.fig.size = FigureSize {
        width_mm: 180.5,
        height_mm: 120.25,
    };
    b.fig.font_size_pt = 8.5;
    b.fig.background = Color::rgba(1.0, 1.0, 1.0, 128.0 / 255.0);
    // Fewer row sizes than rows, so that a missing size is exercised, and a fitted last
    // column, which holds only a legend panel and an axes that also spans a weighted
    // column, because an axes in fitted columns alone is invalid.
    b.fig.layout = TileLayout {
        row_weights: vec![
            TrackSize::Weight(1.0),
            TrackSize::Weight(1.5),
            TrackSize::Weight(0.75),
        ],
        col_weights: vec![
            TrackSize::Weight(2.0),
            TrackSize::Weight(1.0),
            TrackSize::Weight(1.0),
            TrackSize::Weight(0.5),
            TrackSize::Fit,
        ],
        ..TileLayout::new(4, 5)
    };
    b.fig.provenance = Provenance {
        ironlab_version: "0.1.0".to_owned(),
        typesetter: "latex-rust".to_owned(),
        typesetter_version: "1.0.2".to_owned(),
        fonts: vec!["STIX Two Text".to_owned(), "STIX Two Math".to_owned()],
    };
    b.fig.parameters = BTreeMap::from([
        ("converged".to_owned(), Parameter::Bool(true)),
        ("mesh_cells".to_owned(), Parameter::Integer(-4_096)),
        ("reynolds_number".to_owned(), Parameter::Number(1.0e5)),
        ("solver".to_owned(), Parameter::String("k–ω SST".to_owned())),
    ]);
    // Out of alphabetical order, and one tag not ASCII, so that a fixture written or
    // read through a sorted collection is caught.
    b.fig.tags = vec![
        "validated".to_owned(),
        "grenzschicht".to_owned(),
        "k–ω".to_owned(),
    ];

    // Shared data, each array stored once and referred to by artists of several axes.
    // Positive values so that log axes produce no warnings. Two arrays carry a name, one
    // of them not ASCII, and the rest have none, so that both forms are exercised.
    let t = b.vector(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    b.data_array(t).name = Some("time (s)".to_owned());
    let t_nan = b.vector(&[1.0, 4.0, f64::NAN, 16.0, 25.0, 36.0, 49.0, 64.0, 81.0]);
    b.data_array(t_nan).name = Some("t² with a gap".to_owned());
    let (ny, nx) = (3, 4);
    let gx = b.vector(&[0.0, 1.0, 2.0, 3.0]);
    let gy = b.vector(&[0.0, 0.5, 1.0]);
    let field = b.matrix(ny, nx, |j, i| (j * nx + i) as f64 + 0.5);
    let cx = b.matrix(ny, nx, |j, i| i as f64 + 0.1 * j as f64);
    let cy = b.matrix(ny, nx, |j, i| j as f64 - 0.1 * i as f64);
    // An array of 8-bit values, the indices of a scalar image through an indexed scale, so
    // that the element type is exercised by every format and edit that covers the data
    // table.
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

    // Colour scales, numbered from 900 so that the identifiers of the axes and artists
    // below are those they had before the figure held scales. One scale of every kind and
    // variant is named by the colour-mapped artists of some axes; between them the scales
    // use every colormap name, both colormap variants, both norms and every out-of-range
    // policy at some category, and two scales are named by no artist, as a custom
    // colourbar needs. A strict policy falls only on values that cannot violate it.
    let blue = Color::rgb(0.0, 114.0 / 255.0, 178.0 / 255.0);
    let grey = Color::rgb(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0);
    let [
        scatter_scale,
        contour_scale,
        three_scale,
        surface_scale,
        classes_scale,
        wall_scale,
        palette_scale,
        west_scale,
        gray_scale,
        status_scale,
    ] = std::array::from_fn(|k| NodeId(900 + k as u64));
    b.fig.color_scales = vec![
        ColorScale {
            id: scatter_scale,
            title: Some(Text::new("colour $c$")),
            kind: ColorScaleKind::Continuous(ContinuousColorScale {
                colormap: Colormap::Named {
                    name: ColormapName::Cividis,
                    reverse: true,
                },
                limits: Limits::Manual {
                    min: -1.0,
                    max: 1.0,
                },
                norm: ColorScaleNorm::Linear,
                below: out_of_range(OutOfRange::Clamp),
                above: ColorScaleOutOfRange {
                    policy: OutOfRange::Rgba { color: blue },
                    show_key: true,
                    label: Some(Text::plain("too high")),
                },
                ticks: Ticks::Step {
                    step: 0.5,
                    origin: 0.0,
                },
                tick_labels: TickLabels::Hidden,
            }),
            non_finite: out_of_range(OutOfRange::Transparent),
        },
        ColorScale {
            id: contour_scale,
            title: None,
            kind: ColorScaleKind::Discrete(DiscreteColorScale {
                colormap: named(ColormapName::Magma),
                levels: ColorScaleLevels::Explicit {
                    values: vec![0.0, 2.5, 5.0, 12.0],
                },
                ticks: Ticks::Values(vec![0.0, 5.0, 12.0]),
                ..DiscreteColorScale::default()
            }),
            non_finite: out_of_range(OutOfRange::Transparent),
        },
        ColorScale {
            id: three_scale,
            title: None,
            kind: ColorScaleKind::Continuous(ContinuousColorScale {
                colormap: named(ColormapName::Plasma),
                norm: ColorScaleNorm::Log,
                ticks: Ticks::Values(vec![1.0, 10.0]),
                tick_labels: TickLabels::Text(vec![Text::plain("one"), Text::new("$10$")]),
                ..ContinuousColorScale::default()
            }),
            non_finite: out_of_range(OutOfRange::Transparent),
        },
        ColorScale {
            id: surface_scale,
            title: Some(Text::plain("height")),
            kind: ColorScaleKind::Discrete(DiscreteColorScale {
                colormap: named(ColormapName::Coolwarm),
                levels: ColorScaleLevels::Auto {
                    count: 6,
                    limits: Limits::Manual {
                        min: 0.0,
                        max: 12.0,
                    },
                },
                norm: ColorScaleNorm::Linear,
                below: out_of_range(OutOfRange::Transparent),
                above: out_of_range(OutOfRange::Clamp),
                ticks: Ticks::Auto,
                tick_labels: TickLabels::Auto,
            }),
            non_finite: out_of_range(OutOfRange::Transparent),
        },
        ColorScale {
            id: classes_scale,
            title: None,
            kind: ColorScaleKind::Indexed(IndexedColorScale {
                colors: vec![Color::BLACK, blue, grey, Color::WHITE],
                outside: ColorScaleOutOfRange {
                    policy: OutOfRange::Clamp,
                    show_key: true,
                    label: Some(Text::plain("clipped")),
                },
                ticks: Ticks::Values(vec![0.0, 3.0]),
                tick_labels: TickLabels::Text(vec![Text::plain("low"), Text::plain("high")]),
            }),
            non_finite: out_of_range(OutOfRange::Transparent),
        },
        ColorScale {
            id: wall_scale,
            title: None,
            kind: ColorScaleKind::Continuous(ContinuousColorScale {
                colormap: named(ColormapName::Inferno),
                below: out_of_range(OutOfRange::Rgba { color: blue }),
                above: out_of_range(OutOfRange::Transparent),
                ..ContinuousColorScale::default()
            }),
            // The field holds no NaN, so a strict non-finite category refuses nothing.
            non_finite: out_of_range(OutOfRange::Strict),
        },
        ColorScale {
            id: palette_scale,
            title: None,
            // A 256-entry grey palette, strict outside its entries and for non-finite
            // values, which 8-bit indices cannot violate.
            kind: ColorScaleKind::Indexed(IndexedColorScale {
                colors: (0..=255u8)
                    .map(|k| {
                        let v = f32::from(k) / 255.0;
                        Color::rgb(v, v, v)
                    })
                    .collect(),
                outside: out_of_range(OutOfRange::Strict),
                ticks: Ticks::Auto,
                tick_labels: TickLabels::Auto,
            }),
            non_finite: out_of_range(OutOfRange::Strict),
        },
        ColorScale {
            id: west_scale,
            title: Some(Text::new("$T$ (K)")),
            kind: ColorScaleKind::Continuous(ContinuousColorScale {
                colormap: Colormap::Listed {
                    colors: vec![Color::BLACK, blue, Color::WHITE],
                },
                below: out_of_range(OutOfRange::Transparent),
                above: out_of_range(OutOfRange::Clamp),
                ..ContinuousColorScale::default()
            }),
            non_finite: ColorScaleOutOfRange {
                policy: OutOfRange::Rgba {
                    color: Color::rgba(1.0, 0.0, 0.0, 128.0 / 255.0),
                },
                show_key: true,
                label: None,
            },
        },
        ColorScale {
            id: gray_scale,
            kind: ColorScaleKind::Continuous(ContinuousColorScale {
                colormap: named(ColormapName::Gray),
                ..ContinuousColorScale::default()
            }),
            ..ColorScale::default()
        },
        ColorScale {
            id: status_scale,
            title: Some(Text::plain("status")),
            kind: ColorScaleKind::Categorical(CategoricalColorScale {
                categories: vec![
                    ColorCategory {
                        color: blue,
                        label: Text::plain("converged"),
                    },
                    ColorCategory {
                        color: grey,
                        label: Text::new(r"$\infty$"),
                    },
                ],
                outside: out_of_range(OutOfRange::Transparent),
            }),
            non_finite: ColorScaleOutOfRange {
                policy: OutOfRange::Transparent,
                show_key: true,
                label: Some(Text::plain("not entered")),
            },
        },
    ];

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
        MarkerShape::VLine,
        MarkerShape::HLine,
    ];
    // Every combination of coordinates along x and y, each line taking one in turn.
    let coords = [
        (Coords::Data, Coords::Data),
        (Coords::AxesFraction, Coords::Data),
        (Coords::Data, Coords::AxesFraction),
        (Coords::AxesFraction, Coords::AxesFraction),
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
            reverse: true,
            ticks: Ticks::Values(vec![1.0, 2.0, 10.0]),
            tick_labels: TickLabels::Text(vec![
                Text::plain("one"),
                Text::new("$2$"),
                Text::plain("ten"),
            ]),
            grid: true,
            visible: true,
            tick_label_angle: 30.0,
        };
        a.y.ticks = Ticks::Step {
            step: 0.25,
            origin: -0.125,
        };
        a.y.tick_labels = TickLabels::Hidden;
        a.y.scale = Scale::Log;
        a.box_ = false;
        a.legend = Some(Legend::Inside(InsideLegend {
            location: LegendLocation::NorthEast,
            boxed: false,
            ..InsideLegend::default()
        }));
    }
    for (k, shape) in shapes.into_iter().enumerate() {
        let id = b.node();
        b.push(
            lines,
            Artist::Series(Series {
                id,
                label: Some(Text::new(format!("series ${k}$"))),
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
                size_data: None,
                color_data: None,
                color_scale: None,
                x_coords: coords[k % coords.len()].0,
                y_coords: coords[k % coords.len()].1,
                clip: k % 2 == 0,
            }),
        );
    }

    // Axes 2: 2D series of markers without a line, of fixed size and colour and sized and
    // coloured by data.
    let scatters = b.axes2d(0, 1);
    b.axes(scatters).legend = Some(Legend::Inside(InsideLegend {
        location: LegendLocation::NorthWest,
        boxed: true,
        ..InsideLegend::default()
    }));
    let sizes = b.vector(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
    let colours = b.vector(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]);
    let id = b.node();
    b.push(
        scatters,
        Artist::Series(Series {
            id,
            label: None,
            visible: true,
            x: t,
            y: t,
            z: None,
            line: LineStyle {
                color: ColorSpec::Rgba {
                    color: Color::BLACK,
                },
                dash: DashStyle::None,
                ..LineStyle::default()
            },
            marker: MarkerStyle {
                shape: MarkerShape::Diamond,
                size_pt: 6.0,
                ..MarkerStyle::default()
            },
            size_data: None,
            color_data: None,
            color_scale: None,
            x_coords: Coords::Data,
            y_coords: Coords::AxesFraction,
            clip: false,
        }),
    );
    let id = b.node();
    b.push(
        scatters,
        Artist::Series(Series {
            id,
            x: t,
            y: t_nan,
            line: LineStyle {
                color: ColorSpec::Colormapped,
                dash: DashStyle::None,
                ..LineStyle::default()
            },
            marker: MarkerStyle {
                shape: MarkerShape::Circle,
                ..MarkerStyle::default()
            },
            size_data: Some(sizes),
            color_data: Some(colours),
            color_scale: Some(scatter_scale),
            ..Series::default()
        }),
    );

    // Axes 3: 2D contours on both grid kinds, both level kinds, filled and unfilled.
    let contours = b.axes2d(0, 2);
    b.axes(contours).legend = Some(Legend::Inside(InsideLegend {
        location: LegendLocation::SouthEast,
        boxed: true,
        ..InsideLegend::default()
    }));
    let id = b.node();
    b.push(
        contours,
        Artist::Contour(Contour {
            id,
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: field,
            levels: Some(Levels::Auto { count: 7 }),
            fill: false,
            placement: ContourPlacement::Plane { z: None },
            color_scale: Some(contour_scale),
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
            levels: Some(Levels::Explicit {
                values: vec![1.0, 2.5, 4.0],
            }),
            fill: true,
            placement: ContourPlacement::Plane { z: Some(-1.5) },
            color_scale: Some(contour_scale),
            line_opacity: 0.5,
            fill_opacity: 0.75,
            ..Contour::default()
        }),
    );

    // Axes 4: 2D quivers with automatic and factor scaling.
    let quivers = b.axes2d(0, 3);
    b.axes(quivers).legend = Some(Legend::Inside(InsideLegend {
        location: LegendLocation::SouthWest,
        boxed: true,
        ..InsideLegend::default()
    }));
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
            reverse: true,
            ticks: Ticks::Values(vec![0.1, 1.0, 10.0]),
            tick_labels: TickLabels::Auto,
            grid: true,
            // A three-dimensional axes ignores a tick label angle with a warning, so the
            // only valid angle here is the default.
            visible: true,
            tick_label_angle: 0.0,
        };
        a.legend = Some(Legend::Inside(InsideLegend {
            location: LegendLocation::North,
            boxed: true,
            ..InsideLegend::default()
        }));
    }
    let id = b.node();
    b.push(
        three,
        Artist::Series(Series {
            id,
            x: t,
            y: t,
            z: Some(t_nan),
            ..Series::default()
        }),
    );
    let id = b.node();
    b.push(
        three,
        Artist::Series(Series {
            id,
            x: t,
            y: t,
            z: Some(t),
            line: LineStyle {
                dash: DashStyle::None,
                ..LineStyle::default()
            },
            marker: MarkerStyle {
                shape: MarkerShape::Circle,
                ..MarkerStyle::default()
            },
            ..Series::default()
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
            // No levels of its own, so that it takes those of its scale.
            color_scale: Some(three_scale),
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
    b.axes(surfaces).legend = Some(Legend::Inside(InsideLegend {
        location: LegendLocation::South,
        boxed: true,
        ..InsideLegend::default()
    }));
    let id = b.node();
    b.push(
        surfaces,
        Artist::Surface(Surface {
            id,
            label: Some(Text::plain("surf")),
            grid: Grid::Rectilinear { x: gx, y: gy },
            z: field,
            c: Some(cx),
            color_scale: Some(surface_scale),
            ..Surface::default()
        }),
    );
    let id = b.node();
    b.push(
        surfaces,
        Artist::Surface(Surface {
            id,
            label: Some(Text::plain("mesh")),
            visible: false,
            grid: Grid::Curvilinear { x: cx, y: cy },
            z: field,
            c: None,
            face: ColorSpec::Rgba {
                color: Color::WHITE,
            },
            edge: ColorSpec::Colormapped,
            edge_width_pt: 0.3,
            color_scale: Some(surface_scale),
            // One array is shared by the faces and the edges, as the IR allows.
            face_alpha: Some(cx),
            edge_alpha: Some(cx),
            face_opacity: 0.25,
            edge_opacity: 0.75,
        }),
    );
    // Images on the walls of the 3D axes: a hidden true-colour image of floating-point
    // RGBA pixels on the xz wall, a scalar image of fractional indices through an indexed
    // scale that clamps them on the yz wall, and a scalar image of values on the xz wall
    // through a scale that is strict only for non-finite values, so that an edit of the
    // scale's limits is not refused.
    let id = b.node();
    b.push(
        surfaces,
        Artist::Image(Image {
            id,
            label: None,
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
            opacity: 0.5,
        }),
    );
    let id = b.node();
    b.push(
        surfaces,
        Artist::ScalarImage(ScalarImage {
            id,
            label: Some(Text::plain("classes")),
            values: cx,
            color_scale: Some(classes_scale),
            placement: ImagePlacement {
                plane: ImagePlane::Yz { x: None },
                columns: None,
                rows: Some(PixelRange {
                    first: 3.0,
                    last: 1.0,
                }),
            },
            ..ScalarImage::default()
        }),
    );
    let id = b.node();
    b.push(
        surfaces,
        Artist::ScalarImage(ScalarImage {
            id,
            values: field,
            color_scale: Some(wall_scale),
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
            ..ScalarImage::default()
        }),
    );

    // Axes 7 to 9: the remaining legend locations, and a spanning cell. The east axes
    // holds the 2D images of 8-bit data: a true-colour image in the default placement and
    // a hidden scalar image whose floor offset is ignored, whose columns are mirrored and
    // whose 256-entry palette is strict at every category (which 8-bit indices cannot
    // violate). The west axes holds a scalar image with both pixel ranges through a scale
    // with a lenient policy of each kind.
    let east = b.axes2d(1, 2);
    b.axes(east).legend = Some(Legend::Inside(InsideLegend {
        location: LegendLocation::East,
        boxed: true,
        ..InsideLegend::default()
    }));
    let id = b.node();
    b.push(
        east,
        Artist::Image(Image {
            id,
            label: Some(Text::plain("photo")),
            pixels: rgb_pixels,
            ..Image::default()
        }),
    );
    let id = b.node();
    b.push(
        east,
        Artist::ScalarImage(ScalarImage {
            id,
            visible: false,
            values: bytes,
            color_scale: Some(palette_scale),
            placement: ImagePlacement {
                plane: ImagePlane::Xy { z: Some(-2.0) },
                columns: Some(PixelRange {
                    first: 2.0,
                    last: 0.0,
                }),
                rows: None,
            },
            alpha: Some(bytes),
            opacity: 0.6,
            ..ScalarImage::default()
        }),
    );
    let west = b.axes2d(1, 3);
    b.axes(west).legend = Some(Legend::Inside(InsideLegend {
        location: LegendLocation::West,
        boxed: true,
        ..InsideLegend::default()
    }));
    let id = b.node();
    b.push(
        west,
        Artist::ScalarImage(ScalarImage {
            id,
            label: Some(Text::new("$T$ (K)")),
            values: field,
            color_scale: Some(west_scale),
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
            alpha: Some(cx),
            opacity: 0.9,
            ..ScalarImage::default()
        }),
    );
    let best = b.axes2d(2, 0);
    {
        let a = b.axes(best);
        a.cell = Cell {
            row: 2,
            col: 0,
            row_span: 1,
            col_span: 2,
        };
        a.legend = Some(Legend::Inside(InsideLegend {
            location: LegendLocation::Best,
            boxed: true,
            layout: LegendEntryLayout {
                columns: LegendEntryColumns::Count(3),
                order: LegendEntryOrder::RowMajor,
            },
            entries: LegendEntries::Auto,
        }));
    }

    // Axes 10: an axes without a legend or title, which spans the last weighted column and
    // the fitted column.
    let plain = b.axes2d(2, 3);
    b.axes(plain).cell.col_span = 2;
    add_bars_stairs_and_cells(&mut b, plain, bytes, palette_scale);

    // Axes 11 and 12 and two legend panels: two axes that each list a named line in a
    // panel, so that neither panel is empty; a boxed panel with a fixed number of columns
    // filled row by row; and an unboxed panel with automatic columns filled column by
    // column, centred in a cell that spans two columns.
    let listed = b.axes2d(3, 0);
    let also_listed = b.axes2d(2, 2);
    let fixed_panel = b.node();
    let auto_panel = b.node();
    b.axes(listed).legend = Some(Legend::Panel(fixed_panel));
    b.axes(also_listed).legend = Some(Legend::Panel(auto_panel));
    let mut auto_listed_line = NodeId(0);
    for (axes, name) in [
        (listed, "in a fixed panel"),
        (also_listed, "in an automatic panel"),
    ] {
        let id = b.node();
        auto_listed_line = id;
        b.push(
            axes,
            Artist::Series(Series {
                id,
                label: Some(Text::plain(name)),
                x: t,
                y: t_nan,
                ..Series::default()
            }),
        );
    }
    b.fig.panels = vec![
        Panel {
            id: fixed_panel,
            // In the fitted column alone, which takes the width of this panel.
            cell: Cell {
                row: 3,
                col: 4,
                row_span: 1,
                col_span: 1,
            },
            align: PanelLocation::SouthWest,
            // A titled legend panel whose title is aligned with the right edge of its
            // entries.
            title: Some(Text::new(r"Key, $\beta$")),
            title_align: TitleAlign::Right,
            content: PanelContent::Legend(PanelLegend {
                boxed: true,
                layout: LegendEntryLayout {
                    columns: LegendEntryColumns::Count(2),
                    order: LegendEntryOrder::RowMajor,
                },
                entries: LegendEntries::Auto,
            }),
        },
        Panel {
            id: auto_panel,
            cell: Cell {
                row: 3,
                col: 2,
                row_span: 1,
                col_span: 2,
            },
            align: PanelLocation::Center,
            title: None,
            title_align: TitleAlign::Left,
            content: PanelContent::Legend(PanelLegend {
                boxed: false,
                layout: LegendEntryLayout {
                    columns: LegendEntryColumns::Auto,
                    order: LegendEntryOrder::ColumnMajor,
                },
                // An explicit list of every kind of item: the artist of the axes that names
                // the panel, on a row of its own, then two items that stand for no artist,
                // one of each kind of sample, then two colour scales.
                entries: LegendEntries::Explicit(vec![
                    LegendItem {
                        full_row: true,
                        ..LegendItem::artist(auto_listed_line)
                    },
                    LegendItem::custom(
                        LegendSample::Line {
                            line: LineStyle {
                                color: ColorSpec::None,
                                width_pt: 1.25,
                                dash: DashStyle::None,
                            },
                            marker: MarkerStyle {
                                shape: MarkerShape::TriangleUp,
                                size_pt: 5.5,
                                face: ColorSpec::Rgba {
                                    color: Color::rgb(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0),
                                },
                                edge: ColorSpec::Auto,
                            },
                        },
                        Text::new("Separation $x_s$"),
                    ),
                    LegendItem::custom(
                        LegendSample::swatch(Color::rgba(
                            0.0,
                            128.0 / 255.0,
                            64.0 / 255.0,
                            128.0 / 255.0,
                        )),
                        Text::plain("Lower surface"),
                    ),
                    // Two colour scales that no artist names: the categories of the
                    // status scale, and the grey palette as a bar on a row of its own.
                    // The status scale with rows other than the default, which a
                    // categorical scale ignores but the encodings must still carry.
                    LegendItem::color_scale(status_scale).with_rows(3),
                    LegendItem {
                        full_row: true,
                        ..LegendItem::color_scale(gray_scale)
                    },
                ]),
            }),
        },
    ];

    // Two colourbar panels, after the legend panels so that those keep their positions:
    // a vertical bar of the grey scale, which no artist names, spanning the first two rows
    // of the fitted column beside the plots of those rows (#88); and a horizontal key of
    // the categorical status scale, which no artist names either, with its "not entered"
    // key, in the free cell of the last row. Inside colourbars on four axes, two of them
    // three-dimensional, use every side, the default length, the full length and shorter
    // ones.
    let tall_bar = b.node();
    let wide_bar = b.node();
    b.fig.panels.extend([
        Panel {
            id: tall_bar,
            cell: Cell {
                row: 0,
                col: 4,
                row_span: 2,
                col_span: 1,
            },
            align: PanelLocation::West,
            // A centred panel title over a bar of automatic length whose scale title is a
            // heading above it.
            title: Some(Text::plain("Grey level")),
            title_align: TitleAlign::Center,
            content: PanelContent::Colorbar(PanelColorbar {
                scale: gray_scale,
                orientation: ColorbarOrientation::Vertical,
                length: None,
                title_placement: ColorbarTitlePlacement::Above,
            }),
        },
        Panel {
            id: wide_bar,
            cell: Cell {
                row: 3,
                col: 1,
                row_span: 1,
                col_span: 1,
            },
            align: PanelLocation::North,
            title: None,
            title_align: TitleAlign::Left,
            // Three quarters of the span, with the scale's title along the bar.
            content: PanelContent::Colorbar(PanelColorbar {
                scale: status_scale,
                orientation: ColorbarOrientation::Horizontal,
                length: Some(0.75),
                title_placement: ColorbarTitlePlacement::Along,
            }),
        },
    ]);
    for (axes, scale, side, length, title_placement) in [
        (
            scatters,
            scatter_scale,
            InsideColorbarSide::South,
            0.8,
            ColorbarTitlePlacement::Along,
        ),
        (
            contours,
            contour_scale,
            InsideColorbarSide::East,
            1.0,
            ColorbarTitlePlacement::Above,
        ),
        (
            three,
            three_scale,
            InsideColorbarSide::West,
            0.5,
            ColorbarTitlePlacement::Along,
        ),
        (
            surfaces,
            surface_scale,
            InsideColorbarSide::North,
            0.25,
            ColorbarTitlePlacement::Above,
        ),
    ] {
        b.axes(axes).colorbar = Some(InsideColorbar {
            scale,
            side,
            length,
            title_placement,
        });
    }

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

/// Adds to a 2D axes of linear scales bars, stairs and cells that between them use every
/// variant of every enum and tagged value of those artists, and hides the x axis of the
/// axes and rotates the labels of its y ticks.
///
/// The bars are three series of four categories placed side by side as the facade's
/// grouping helper places them (a third of four fifths of the pitch wide, offset by −1, 0
/// and 1 widths), one of them with a NaN value and one whose faces and outlines are
/// coloured by data through `palette`; two horizontal series of a fixed width and an
/// absolute offset stacked as the stacking helper stacks them (the second on bases given
/// per bar from the positive parts of the first), one with negative values, translucent
/// faces and translucent edges, and with margins of their own; and a hidden series of
/// floating bars on a base and widths given as data, offset by half a width, which records
/// the binning of a density histogram. The stairs are an open outline on edges, which
/// records the binning of a cumulative histogram by a named rule, and three filled or
/// unfilled outlines on samples, one for each step position. The stairs on samples have
/// translucent lines. The cells are a grid of floats on decreasing column edges and unit
/// rows, with gaps, edges, alpha and face and edge opacities, mapped through a scale of
/// their own (numbered 910, appended after the kitchen-sink scales so that their indices
/// are unchanged), which records a two-dimensional binning with a minimum count, and a
/// hidden grid of the 8-bit `bytes` on unit columns and explicit rows through `palette`,
/// which records a binning of bins of a width along x and at edges along y. Every array
/// is referred to, so the axes adds no unused-data warning.
fn add_bars_stairs_and_cells(b: &mut FigureBuilder, axes: NodeId, bytes: NodeId, palette: NodeId) {
    let red = Color::rgb(1.0, 0.0, 0.0);
    let blue = Color::rgb(0.0, 114.0 / 255.0, 178.0 / 255.0);
    {
        let a = b.axes(axes);
        a.x.visible = false;
        a.y.tick_label_angle = 45.0;
    }
    let categories = b.vector(&[1.0, 2.0, 3.0, 4.0]);
    let first = b.vector(&[3.0, 5.0, 2.0, 4.0]);
    let second = b.vector(&[4.0, 1.0, 3.0, 5.0]);
    let gappy = b.vector(&[2.0, f64::NAN, 4.0, 1.0]);
    let signed = b.vector(&[-1.0, 2.0, -3.0, 4.0]);
    let stacked_on = b.vector(&[0.0, 2.0, 0.0, 4.0]);
    let floor = b.vector(&[0.5, 0.5, 1.0, 1.0]);
    let widths = b.vector(&[0.5, 0.8, 0.5, 0.8]);
    let edges = b.vector(&[0.0, 1.0, 2.0, 3.0, 4.0]);
    let field = b.matrix(2, 3, |j, i| (j * 3 + i) as f64 - 1.5);
    let alpha = b.matrix(2, 3, |j, i| (j * 3 + i) as f64 / 5.0);
    let falling = b.vector(&[3.0, 2.0, 1.0, 0.0]);
    let rows = b.vector(&[0.0, 10.0, 20.0]);
    let codes = b.vector(&[0.0, 1.0, 2.0, 3.0]);

    for (k, (values, face, color_data)) in [
        (first, ColorSpec::Auto, None),
        (second, ColorSpec::Colormapped, Some(codes)),
        (gappy, ColorSpec::None, None),
    ]
    .into_iter()
    .enumerate()
    {
        let id = b.node();
        b.push(
            axes,
            Artist::Bar(Bar {
                id,
                label: Some(Text::new(format!("model ${k}$"))),
                positions: categories,
                values,
                width: BarWidth::Pitch(0.8 / 3.0),
                offset: BarOffset::Widths(k as f64 - 1.0),
                face,
                color_data,
                color_scale: color_data.map(|_| palette),
                edge: LineStyle {
                    color: match k {
                        1 => ColorSpec::Colormapped,
                        2 => ColorSpec::Auto,
                        _ => ColorSpec::None,
                    },
                    ..Bar::default().edge
                },
                ..Bar::default()
            }),
        );
    }
    for (values, base) in [
        (signed, PerBar::Constant(0.0)),
        (first, PerBar::Data(stacked_on)),
    ] {
        let id = b.node();
        b.push(
            axes,
            Artist::Bar(Bar {
                id,
                orientation: Orientation::Horizontal,
                positions: categories,
                values,
                base,
                width: BarWidth::Constant(0.6),
                offset: BarOffset::Absolute(0.1),
                edge: LineStyle {
                    color: ColorSpec::Rgba { color: red },
                    width_pt: 0.25,
                    dash: DashStyle::Dashed,
                },
                face_opacity: 0.5,
                edge_opacity: 0.75,
                margin: 0.1,
                ..Bar::default()
            }),
        );
    }
    let id = b.node();
    b.push(
        axes,
        Artist::Bar(Bar {
            id,
            visible: false,
            positions: categories,
            values: second,
            base: PerBar::Data(floor),
            width: BarWidth::Data(widths),
            offset: BarOffset::Widths(0.5),
            margin: 0.0,
            binning: Some(Binning {
                bins: Bins::Edges(vec![0.5, 1.5, 2.5, 3.5, 4.5]),
                range: Some([0.5, 4.5]),
                normalization: HistNorm::Density,
                total: 12.0,
                excluded_non_finite: 1,
                excluded_out_of_range: 2,
            }),
            ..Bar::default()
        }),
    );
    let id = b.node();
    b.push(
        axes,
        Artist::Stairs(Stairs {
            id,
            label: Some(Text::plain("histogram")),
            data: StairsData::Edges {
                edges,
                values: first,
            },
            binning: Some(Binning {
                bins: Bins::Rule(BinRule::Sturges),
                range: None,
                normalization: HistNorm::CumulativeCount,
                total: 14.0,
                excluded_non_finite: 0,
                excluded_out_of_range: 3,
            }),
            ..Stairs::default()
        }),
    );
    for (values, step, base, fill) in [
        (
            second,
            StepAt::Pre,
            Some(0.0),
            ColorSpec::Rgba { color: blue },
        ),
        (gappy, StepAt::Mid, Some(-1.0), ColorSpec::Auto),
        (signed, StepAt::Post, None, ColorSpec::None),
    ] {
        let id = b.node();
        b.push(
            axes,
            Artist::Stairs(Stairs {
                id,
                orientation: if step == StepAt::Pre {
                    Orientation::Horizontal
                } else {
                    Orientation::Vertical
                },
                data: StairsData::Samples {
                    x: categories,
                    values,
                    step,
                },
                base,
                line: LineStyle {
                    dash: DashStyle::Dotted,
                    ..LineStyle::default()
                },
                fill,
                line_opacity: 0.5,
                fill_opacity: if base.is_some() { 0.25 } else { 1.0 },
                ..Stairs::default()
            }),
        );
    }
    let scale = NodeId(910);
    b.fig.color_scales.push(ColorScale {
        id: scale,
        title: Some(Text::plain("cells")),
        ..ColorScale::default()
    });
    let id = b.node();
    b.push(
        axes,
        Artist::Cells(Cells {
            id,
            label: Some(Text::plain("matrix")),
            values: field,
            columns: CellEdges::Edges(falling),
            rows: CellEdges::Unit,
            gap: CellGap { x: 0.1, y: 0.25 },
            edge: LineStyle {
                color: ColorSpec::Rgba {
                    color: Color::WHITE,
                },
                width_pt: 1.0,
                dash: DashStyle::Solid,
            },
            color_scale: Some(scale),
            alpha: Some(alpha),
            face_opacity: 0.8,
            edge_opacity: 0.6,
            binning: Some(Binning2d {
                x_bins: Bins::Count(3),
                y_bins: Bins::Width(0.5),
                x_range: Some([0.0, 3.0]),
                y_range: None,
                normalization: HistNorm::Percentage,
                min_count: Some(1.0),
                total: 30.0,
                excluded_non_finite: 2,
                excluded_out_of_range: 0,
            }),
            ..Cells::default()
        }),
    );
    let id = b.node();
    b.push(
        axes,
        Artist::Cells(Cells {
            id,
            visible: false,
            values: bytes,
            columns: CellEdges::Unit,
            rows: CellEdges::Edges(rows),
            color_scale: Some(palette),
            binning: Some(Binning2d {
                x_bins: Bins::Width(1.0),
                y_bins: Bins::Edges(vec![0.0, 10.0, 20.0]),
                x_range: None,
                y_range: Some([0.0, 20.0]),
                normalization: HistNorm::Count,
                min_count: None,
                total: 9.0,
                excluded_non_finite: 0,
                excluded_out_of_range: 0,
            }),
            ..Cells::default()
        }),
    );
}

/// A valid figure that sets every variant of the placement of images and of the
/// out-of-range policies of the scales they name, beyond what the kitchen-sink figure
/// holds: in a 3D axes, on each plane, with and without an offset and with offsets of both
/// signs, a true-colour image, a scalar image of 8-bit indices through an indexed scale of
/// its own and a scalar image of values through a continuous scale of its own, the scales
/// cycling through the policies so that every policy occurs at every category, and pixel
/// ranges that are absent, ascending and mirrored; and in a 2D axes, a true-colour image
/// whose floor offset is ignored and whose single row has coincident centres, a scalar
/// image of floating-point indices through an indexed scale and a scalar image of 8-bit
/// values through a continuous scale, both in the default placement.
///
/// The strict policies fall on 8-bit indices into 256 entries and on finite values under
/// automatic limits, which cannot violate them, so the figure validates without issues.
pub fn image_variants_figure() -> Figure {
    let mut b = FigureBuilder::new();
    b.fig.layout = TileLayout::new(1, 2);
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
    // A 256-entry palette, which every 8-bit index names.
    let palette: Vec<Color> = (0..=255u8)
        .map(|k| {
            let v = f32::from(k) / 255.0;
            Color::rgb(v, v, v)
        })
        .collect();
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
        let indexed_scale = b.color_scale(ColorScaleKind::Indexed(IndexedColorScale {
            colors: palette.clone(),
            outside: out_of_range(policies[k % 4]),
            ..IndexedColorScale::default()
        }));
        b.scale(indexed_scale).non_finite = out_of_range(policies[(k + 2) % 4]);
        let id = b.node();
        b.push(
            solid,
            Artist::ScalarImage(ScalarImage {
                id,
                values: byte_indices,
                color_scale: Some(indexed_scale),
                placement: ImagePlacement {
                    plane,
                    columns: ranges[(k + 1) % 3],
                    rows: ranges[(k + 2) % 3],
                },
                ..ScalarImage::default()
            }),
        );
        let continuous_scale = b.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
            below: out_of_range(policies[(k + 3) % 4]),
            above: out_of_range(policies[(k + 2) % 4]),
            ..ContinuousColorScale::default()
        }));
        b.scale(continuous_scale).non_finite = out_of_range(policies[(k + 1) % 4]);
        let id = b.node();
        b.push(
            solid,
            Artist::ScalarImage(ScalarImage {
                id,
                values,
                color_scale: Some(continuous_scale),
                placement: ImagePlacement {
                    plane,
                    columns: ranges[(k + 2) % 3],
                    rows: ranges[k % 3],
                },
                ..ScalarImage::default()
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
    let indexed_scale = b.color_scale(ColorScaleKind::Indexed(IndexedColorScale {
        colors: palette,
        ..IndexedColorScale::default()
    }));
    let id = b.node();
    b.push(
        flat,
        Artist::ScalarImage(ScalarImage {
            id,
            values: float_indices,
            color_scale: Some(indexed_scale),
            ..ScalarImage::default()
        }),
    );
    let continuous_scale = b.color_scale(ColorScaleKind::default());
    let id = b.node();
    b.push(
        flat,
        Artist::ScalarImage(ScalarImage {
            id,
            values: byte_indices,
            color_scale: Some(continuous_scale),
            ..ScalarImage::default()
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

/// Visits the numbers of a binning record of one dimension: its bin width or edges, its
/// range and its total.
fn visit_binning(visit: &mut dyn FnMut(String, &mut f64), at: &str, binning: &mut Binning) {
    visit_bins(visit, &format!("{at}.bins"), &mut binning.bins);
    if let Some([low, high]) = &mut binning.range {
        visit(format!("{at}.range[0]"), low);
        visit(format!("{at}.range[1]"), high);
    }
    visit(format!("{at}.total"), &mut binning.total);
}

/// Visits the numbers of bins: a width, or every edge.
fn visit_bins(visit: &mut dyn FnMut(String, &mut f64), at: &str, bins: &mut Bins) {
    match bins {
        Bins::Width(width) => visit(format!("{at}.width"), width),
        Bins::Edges(edges) => {
            for (i, edge) in edges.iter_mut().enumerate() {
                visit(format!("{at}.edges[{i}]"), edge);
            }
        }
        Bins::Count(_) | Bins::Rule(_) => {}
    }
}

/// Calls `visit` with a path and a mutable reference for every `f64` held by the
/// figure outside colours and ticks: sizes, numeric figure parameters, limits (of axes
/// and of colour scales), tick label angles, views, style widths and sizes (of artists and
/// of the custom line samples of legends), artist parameters (the bases, widths, offsets,
/// margins and opacities of bars, the gaps and opacities of cells, the bases and opacities of
/// stairs, and every number of their binning records among them), explicit levels and band
/// edges, and
/// every value of every `f64` data array. An array of 8-bit values
/// holds no floats and is skipped.
///
/// A single traversal serves both to set special values and to read them back, so
/// that a field cannot be set without also being checked.
pub fn visit_floats_mut(fig: &mut Figure, visit: &mut dyn FnMut(String, &mut f64)) {
    visit("size.width_mm".into(), &mut fig.size.width_mm);
    visit("size.height_mm".into(), &mut fig.size.height_mm);
    visit("font_size_pt".into(), &mut fig.font_size_pt);
    for (i, size) in fig.layout.row_weights.iter_mut().enumerate() {
        if let TrackSize::Weight(weight) = size {
            visit(format!("layout.row_weights[{i}]"), weight);
        }
    }
    for (i, size) in fig.layout.col_weights.iter_mut().enumerate() {
        if let TrackSize::Weight(weight) = size {
            visit(format!("layout.col_weights[{i}]"), weight);
        }
    }
    for (name, parameter) in fig.parameters.iter_mut() {
        if let Parameter::Number(value) = parameter {
            visit(format!("parameters[{name:?}]"), value);
        }
    }
    for data in fig.data.iter_mut() {
        if let Values::F64(values) = &mut data.array.values {
            for (i, value) in values.iter_mut().enumerate() {
                visit(format!("data[{}][{i}]", data.id.0), value);
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
        if let Some(colorbar) = &mut axes.colorbar {
            visit(at("colorbar.length"), &mut colorbar.length);
        }
        for (name, limits) in [
            ("x.limits", &mut axes.x.limits),
            ("y.limits", &mut axes.y.limits),
            ("z.limits", &mut axes.z.limits),
        ] {
            if let Limits::Manual { min, max } = limits {
                visit(at(&format!("{name}.min")), min);
                visit(at(&format!("{name}.max")), max);
            }
        }
        visit(at("x.tick_label_angle"), &mut axes.x.tick_label_angle);
        visit(at("y.tick_label_angle"), &mut axes.y.tick_label_angle);
        visit(at("z.tick_label_angle"), &mut axes.z.tick_label_angle);
        for (k, artist) in axes.artists.iter_mut().enumerate() {
            let at = |field: &str| format!("axes[{a}].artists[{k}].{field}");
            match artist {
                Artist::Series(series) => {
                    visit(at("line.width_pt"), &mut series.line.width_pt);
                    visit(at("marker.size_pt"), &mut series.marker.size_pt);
                }
                Artist::Contour(contour) => {
                    if let Some(Levels::Explicit { values }) = &mut contour.levels {
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
                Artist::ScalarImage(image) => visit_placement(visit, &at, &mut image.placement),
                Artist::Bar(bar) => {
                    if let PerBar::Constant(base) = &mut bar.base {
                        visit(at("base.constant"), base);
                    }
                    match &mut bar.width {
                        BarWidth::Pitch(fraction) => visit(at("width.pitch"), fraction),
                        BarWidth::Constant(width) => visit(at("width.constant"), width),
                        BarWidth::Data(_) => {}
                    }
                    match &mut bar.offset {
                        BarOffset::Widths(widths) => visit(at("offset.widths"), widths),
                        BarOffset::Absolute(offset) => visit(at("offset.absolute"), offset),
                    }
                    visit(at("edge.width_pt"), &mut bar.edge.width_pt);
                    visit(at("face_opacity"), &mut bar.face_opacity);
                    visit(at("edge_opacity"), &mut bar.edge_opacity);
                    visit(at("margin"), &mut bar.margin);
                    if let Some(binning) = &mut bar.binning {
                        visit_binning(visit, &at("binning"), binning);
                    }
                }
                Artist::Stairs(stairs) => {
                    if let Some(base) = &mut stairs.base {
                        visit(at("base"), base);
                    }
                    visit(at("line.width_pt"), &mut stairs.line.width_pt);
                    visit(at("line_opacity"), &mut stairs.line_opacity);
                    visit(at("fill_opacity"), &mut stairs.fill_opacity);
                    if let Some(binning) = &mut stairs.binning {
                        visit_binning(visit, &at("binning"), binning);
                    }
                }
                Artist::Cells(cells) => {
                    visit(at("gap.x"), &mut cells.gap.x);
                    visit(at("gap.y"), &mut cells.gap.y);
                    visit(at("edge.width_pt"), &mut cells.edge.width_pt);
                    visit(at("face_opacity"), &mut cells.face_opacity);
                    visit(at("edge_opacity"), &mut cells.edge_opacity);
                    if let Some(binning) = &mut cells.binning {
                        let at = at("binning");
                        visit_bins(visit, &format!("{at}.x_bins"), &mut binning.x_bins);
                        visit_bins(visit, &format!("{at}.y_bins"), &mut binning.y_bins);
                        for (name, range) in [
                            ("x_range", &mut binning.x_range),
                            ("y_range", &mut binning.y_range),
                        ] {
                            if let Some([low, high]) = range {
                                visit(format!("{at}.{name}[0]"), low);
                                visit(format!("{at}.{name}[1]"), high);
                            }
                        }
                        if let Some(min_count) = &mut binning.min_count {
                            visit(format!("{at}.min_count"), min_count);
                        }
                        visit(format!("{at}.total"), &mut binning.total);
                    }
                }
            }
        }
        if let Some(Legend::Inside(inside)) = &mut axes.legend {
            visit_legend_entries(visit, &at("legend.entries"), &mut inside.entries);
        }
    }
    for (c, scale) in fig.color_scales.iter_mut().enumerate() {
        let at = |field: &str| format!("color_scales[{c}].{field}");
        let (limits, edges) = match &mut scale.kind {
            ColorScaleKind::Continuous(continuous) => (Some(&mut continuous.limits), None),
            ColorScaleKind::Discrete(discrete) => match &mut discrete.levels {
                ColorScaleLevels::Auto { limits, .. } => (Some(limits), None),
                ColorScaleLevels::Explicit { values } => (None, Some(values)),
            },
            ColorScaleKind::Indexed(_) | ColorScaleKind::Categorical(_) => (None, None),
        };
        if let Some(Limits::Manual { min, max }) = limits {
            visit(at("limits.min"), min);
            visit(at("limits.max"), max);
        }
        for (i, edge) in edges.into_iter().flatten().enumerate() {
            visit(at(&format!("levels[{i}]")), edge);
        }
    }
    for (p, panel) in fig.panels.iter_mut().enumerate() {
        match &mut panel.content {
            PanelContent::Legend(legend) => {
                let at = format!("panels[{p}].content.legend.entries");
                visit_legend_entries(visit, &at, &mut legend.entries);
            }
            // A colourbar holds no number of its own: its scale holds them.
            PanelContent::Colorbar(_) => {}
        }
    }
}

/// Calls `visit` with the width of the line and the size of the marker of every custom
/// line sample of an explicit list of legend entries.
fn visit_legend_entries(
    visit: &mut dyn FnMut(String, &mut f64),
    at: &str,
    entries: &mut LegendEntries,
) {
    let LegendEntries::Explicit(items) = entries else {
        return;
    };
    for (i, item) in items.iter_mut().enumerate() {
        if let LegendItemContent::Custom {
            sample: LegendSample::Line { line, marker },
            ..
        } = &mut item.content
        {
            visit(format!("{at}[{i}].line.width_pt"), &mut line.width_pt);
            visit(format!("{at}[{i}].marker.size_pt"), &mut marker.size_pt);
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

/// Reads a base-128 varint from the front of the bytes, advancing them past it.
pub fn read_varint(bytes: &mut &[u8]) -> u64 {
    let mut value = 0;
    for shift in (0..64).step_by(7) {
        let byte = bytes[0];
        *bytes = &bytes[1..];
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
    }
    value
}

/// Returns the payloads of the top-level length-delimited fields of the given number, in
/// the order in which they appear in the encoded message.
///
/// The bytes are walked without the generated wire types, because decoding a repeated
/// field into a map or a set would discard the order that the encoder wrote, which is
/// what the tests of order are about.
pub fn raw_fields(mut bytes: &[u8], number: u64) -> Vec<Vec<u8>> {
    let mut payloads = Vec::new();
    while !bytes.is_empty() {
        let key = read_varint(&mut bytes);
        let (found, wire_type) = (key >> 3, key & 7);
        match wire_type {
            0 => {
                read_varint(&mut bytes);
            }
            1 => bytes = &bytes[8..],
            5 => bytes = &bytes[4..],
            2 => {
                let len = read_varint(&mut bytes) as usize;
                let (payload, rest) = bytes.split_at(len);
                bytes = rest;
                if found == number {
                    payloads.push(payload.to_vec());
                }
            }
            other => panic!("unexpected wire type {other}"),
        }
    }
    payloads
}
