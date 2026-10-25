//! Resolution and checking of artist data.
//!
//! Every artist's data identifiers are looked up once, and the element types and shapes of its arrays are checked,
//! before limits are computed or anything is drawn. Every artist but the two image kinds requires floating-point
//! values, so an array of 8-bit values cannot be used for it; an image accepts either element type. An image is
//! also checked here for a placement that can be drawn and a plane that its axes can show. An artist that maps
//! numbers to colours is checked for a colour scale to map them through, and its numbers against the strict
//! policies of that scale. An artist whose data cannot be used, an artist whose colours cannot be mapped, and an
//! artist whose data gives it nothing to draw (a series or quiver of no points, an image with no rows or no
//! columns, or a field with no cell between its nodes), is reported with one warning naming it and is then ignored
//! by every later stage, so that the problems indicator of the viewer and the validation of the figure agree about
//! which artists are absent.

use ironlab_ir::{
    Artist, Axes, ColorSpec, ContourPlacement, Coords as ArtistCoords, Dimension, Figure, Grid,
    ImagePlacement, ImagePlane, NdArray, NodeId, PixelRange, QuiverScale, Scale, Series, Values,
};

use crate::maths::contour::{Coords, GridRef, GridShapeError};
use crate::maths::quiver;

use super::Ctx;
use super::artists::Placement;
use super::image;
use super::limits::{axes_axis, is_3d};
use super::style::ColourScales;

/// An artist together with its resolved data, or `None` when the data is unusable.
pub(crate) struct Prepared<'a> {
    pub artist: &'a Artist,
    pub data: Option<ArtistData<'a>>,
}

/// Coordinates of a set of points, one value per point in each array.
#[derive(Clone, Copy)]
pub(crate) struct Points<'a> {
    pub x: &'a [f64],
    pub y: &'a [f64],
    pub z: Option<&'a [f64]>,
}

impl Points<'_> {
    pub fn len(&self) -> usize {
        self.x.len()
    }

    /// Returns point `i`, lying in the plane z = 0 when there is no z data.
    pub fn get(&self, i: usize) -> [f64; 3] {
        [self.x[i], self.y[i], self.z.map_or(0.0, |z| z[i])]
    }
}

/// The kind of an image artist, which decides how its pixels are coloured.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageKind {
    /// Each pixel carries its own colour components.
    TrueColour,
    /// Each pixel holds one number, mapped through the colour scale of the artist.
    Scalar,
}

/// The resolved raster of an image artist of any kind, which has at least one row and one column of pixels.
#[derive(Clone, Copy)]
pub(crate) struct ImageData<'a> {
    pub kind: ImageKind,
    /// The array of the artist, of either element type, whose shape suits the kind.
    pub array: &'a NdArray,
    /// The number of columns of pixels.
    pub nx: usize,
    /// The number of rows of pixels.
    pub ny: usize,
    /// The number of values per pixel: 3 or 4 for a true-colour image, 1 for the other kinds.
    pub components: usize,
    pub placement: ImagePlacement,
    /// The alpha of every pixel of a scalar image, an array of `ny` rows and `nx` columns
    /// of either element type, or `None` when the image has none.
    pub alpha: Option<&'a NdArray>,
    /// The opacity of the image, which multiplies the alpha of every pixel.
    pub opacity: f64,
}

impl ImageData<'_> {
    /// Returns the indices of the dimensions along which the columns and the rows of the image lie, in that order.
    pub fn plane_dims(&self) -> [usize; 2] {
        self.placement.plane.axes().map(dimension_index)
    }

    /// Returns the index of the dimension along which the plane of the image is offset.
    pub fn offset_dim(&self) -> usize {
        let [columns, rows] = self.plane_dims();
        3 - columns - rows
    }
}

/// The colour data of a series, by what each value colours.
#[derive(Clone, Copy)]
pub(crate) enum SeriesColours<'a> {
    /// One value per point, which colours the point's marker and the ends of the segments that meet it.
    PerPoint(&'a [f64]),
    /// One value per segment, the value at index `k` colouring the segment from point `k` to point `k + 1`.
    PerSegment(&'a [f64]),
}

/// The resolved arrays of an artist.
#[derive(Clone, Copy)]
pub(crate) enum ArtistData<'a> {
    Series {
        points: Points<'a>,
        /// The diameter of each marker in points, one per point.
        sizes: Option<&'a [f64]>,
        colours: Option<SeriesColours<'a>>,
    },
    Contour(GridRef<'a>),
    Quiver {
        points: Points<'a>,
        vectors: Points<'a>,
        /// The factor applied to the vectors, in data units.
        scale: f64,
    },
    Surface {
        grid: GridRef<'a>,
        colours: Option<&'a [f64]>,
        /// The alpha of the faces at every node, one value per node of the field.
        face_alpha: Option<&'a [f64]>,
        /// The alpha of the face edges at every node, one value per node of the field.
        edge_alpha: Option<&'a [f64]>,
    },
    /// An image of any kind.
    Image(ImageData<'a>),
}

/// Resolves and checks the data of every artist of an axes, and the colour scale of every artist that maps numbers
/// to colours.
pub(super) fn prepare_axes<'a>(
    ctx: &mut Ctx<'a>,
    axes: &'a Axes,
    scales: &ColourScales,
) -> Vec<Prepared<'a>> {
    let figure = ctx.figure;
    axes.artists
        .iter()
        .map(|artist| {
            let data = resolve(figure, axes, artist).and_then(|data| {
                check_colour_mapping(figure, artist, &data, scales)?;
                Ok(data)
            });
            let data = match data {
                Ok(data) => {
                    warn_log_drops(ctx, axes, artist, &data);
                    Some(data)
                }
                Err(message) => {
                    ctx.warn(
                        Some(artist.id()),
                        format!("The artist is not drawn because {message}."),
                    );
                    None
                }
            };
            Prepared { artist, data }
        })
        .collect()
}

/// Looks up an array and checks that its values match its shape.
fn array(figure: &Figure, id: NodeId) -> Result<&NdArray, String> {
    let array = &figure
        .data_array(id)
        .ok_or_else(|| format!("it refers to {id}, which is not a data array of the figure"))?
        .array;
    let expected = array
        .shape
        .iter()
        .try_fold(1usize, |product, &len| product.checked_mul(len));
    if expected != Some(array.len()) {
        return Err(format!(
            "{id} holds {} values, which does not match its shape {:?}",
            array.len(),
            array.shape
        ));
    }
    Ok(array)
}

/// Looks up an array that must hold floating-point values, which every artist requires. `what` names the role of
/// the array in the artist, such as `x` or `colour`.
fn f64_values<'a>(figure: &'a Figure, id: NodeId, what: &str) -> Result<&'a [f64], String> {
    array(figure, id)?.as_f64().ok_or_else(|| {
        format!(
            "its {what} array {id} holds 8-bit values and the artist requires floating-point values"
        )
    })
}

/// Looks up an array of floating-point values that must hold `len` values.
fn values_of_len<'a>(
    figure: &'a Figure,
    id: NodeId,
    len: usize,
    what: &str,
) -> Result<&'a [f64], String> {
    let values = f64_values(figure, id, what)?;
    if values.len() != len {
        return Err(format!(
            "its {what} array holds {} values but its x array holds {len}",
            values.len()
        ));
    }
    Ok(values)
}

/// Resolves the x, y and optional z arrays of a point set, which must have equal lengths.
fn points(figure: &Figure, x: NodeId, y: NodeId, z: Option<NodeId>) -> Result<Points<'_>, String> {
    let x = f64_values(figure, x, "x")?;
    let y = values_of_len(figure, y, x.len(), "y")?;
    let z = z
        .map(|z| values_of_len(figure, z, x.len(), "z"))
        .transpose()?;
    Ok(Points { x, y, z })
}

/// Checks that a point set, whose arrays agree in length, holds at least one point, because an artist of no points
/// draws nothing and is left out with a warning that names it.
fn at_least_one_point(points: Points<'_>, x: NodeId) -> Result<Points<'_>, String> {
    if points.len() == 0 {
        return Err(format!(
            "its x array {x} holds no values, so it has no points"
        ));
    }
    Ok(points)
}

/// Describes a grid of `ny` rows and `nx` columns of nodes that has no cell between its nodes, or returns `None`
/// for a grid with a cell.
fn nodes_without_cells(ny: usize, nx: usize) -> Option<&'static str> {
    Some(match (ny, nx) {
        (0, 0) => "has no rows and no columns",
        (0, _) => "has no rows",
        (_, 0) => "has no columns",
        (1, 1) => "has a single node",
        (1, _) => "has a single row of nodes",
        (_, 1) => "has a single column of nodes",
        _ => return None,
    })
}

/// Resolves a gridded field: its `[ny, nx]` values and the grid coordinates. A field with no rows or no columns, or
/// a single row or a single column, has no cell between its nodes, and a contour or surface draws the cells, so
/// such a field is an error that names the case and the shape of the field.
fn grid<'a>(figure: &'a Figure, grid: &Grid, z: NodeId) -> Result<GridRef<'a>, String> {
    let field = array(figure, z)?;
    let [ny, nx] = field.shape[..] else {
        return Err(format!(
            "its field {z} has shape {:?}, but a two-dimensional array is required",
            field.shape
        ));
    };
    let coords = match *grid {
        Grid::Rectilinear { x, y } => Coords::Rectilinear {
            x: f64_values(figure, x, "x")?,
            y: f64_values(figure, y, "y")?,
        },
        Grid::Curvilinear { x, y } => Coords::Curvilinear {
            x: f64_values(figure, x, "x")?,
            y: f64_values(figure, y, "y")?,
        },
    };
    let grid = GridRef {
        nx,
        ny,
        coords,
        z: f64_values(figure, z, "z")?,
    };
    grid.validate().map_err(|e| match e {
        GridShapeError::TooSmall { .. } => format!(
            "its field {z} has shape {:?}: it {}, and a contour or surface draws the cells between the nodes of \
             its grid, of which there is none",
            field.shape,
            nodes_without_cells(ny, nx).unwrap_or("has no cells")
        ),
        e => format!("its grid is invalid: {e}"),
    })?;
    Ok(grid)
}

/// Resolves the data of one artist, or returns why the artist is not drawn: its data cannot be used, or gives it
/// nothing to draw.
fn resolve<'a>(figure: &'a Figure, axes: &Axes, artist: &Artist) -> Result<ArtistData<'a>, String> {
    Ok(match artist {
        Artist::Series(series) => {
            let points = points(figure, series.x, series.y, series.z)?;
            let sizes = series
                .size_data
                .map(|data| values_of_len(figure, data, points.len(), "size"))
                .transpose()?;
            let colours = series
                .color_data
                .map(|data| series_colours(figure, series, data, points.len()))
                .transpose()?;
            ArtistData::Series {
                points: at_least_one_point(points, series.x)?,
                sizes,
                colours,
            }
        }
        Artist::Contour(contour) => ArtistData::Contour(grid(figure, &contour.grid, contour.z)?),
        Artist::Quiver(q) => {
            let points = points(figure, q.x, q.y, q.z)?;
            let vectors = Points {
                x: values_of_len(figure, q.u, points.len(), "u")?,
                y: values_of_len(figure, q.v, points.len(), "v")?,
                z: q.w
                    .map(|w| values_of_len(figure, w, points.len(), "w"))
                    .transpose()?,
            };
            let points = at_least_one_point(points, q.x)?;
            let scale = quiver_scale(q.scale, points, vectors);
            ArtistData::Quiver {
                points,
                vectors,
                scale,
            }
        }
        Artist::Surface(surface) => {
            let grid = grid(figure, &surface.grid, surface.z)?;
            let per_node = |id: Option<NodeId>, what: &str| {
                id.map(|id| {
                    let values = f64_values(figure, id, what)?;
                    if values.len() == grid.z.len() {
                        Ok(values)
                    } else {
                        Err(format!(
                            "its {what} array {id} holds {} values but its field holds {}",
                            values.len(),
                            grid.z.len()
                        ))
                    }
                })
                .transpose()
            };
            ArtistData::Surface {
                colours: per_node(surface.c, "colour")?,
                face_alpha: per_node(surface.face_alpha, "face alpha")?,
                edge_alpha: per_node(surface.edge_alpha, "edge alpha")?,
                grid,
            }
        }
        Artist::Image(i) => {
            let look = Look {
                alpha: None,
                opacity: i.opacity,
            };
            image_data(
                figure,
                axes,
                i.pixels,
                ImageKind::TrueColour,
                i.placement,
                look,
            )?
        }
        Artist::ScalarImage(i) => {
            let look = Look {
                alpha: i.alpha,
                opacity: i.opacity,
            };
            image_data(figure, axes, i.values, ImageKind::Scalar, i.placement, look)?
        }
        // The scene compiler does not draw these kinds yet, so each is left out with the reason as its warning.
        Artist::Bar(_) => return Err("bar artists are not drawn yet".to_owned()),
        Artist::Stairs(_) => return Err("stairs artists are not drawn yet".to_owned()),
        Artist::Cells(_) => return Err("cells artists are not drawn yet".to_owned()),
    })
}

/// Resolves the colour data of a series of `points` points, which holds one value per point, or one value per
/// segment when the series draws a line and no marker face or edge of it is colormapped, as validation requires.
fn series_colours<'a>(
    figure: &'a Figure,
    series: &Series,
    data: NodeId,
    points: usize,
) -> Result<SeriesColours<'a>, String> {
    let values = f64_values(figure, data, "colour")?;
    if values.len() == points {
        return Ok(SeriesColours::PerPoint(values));
    }
    if values.len() + 1 != points {
        return Err(format!(
            "its colour array {data} holds {} values, but a series of {points} points needs one per point or one \
             per segment",
            values.len()
        ));
    }
    if !series.draws_line() {
        return Err(format!(
            "its colour array {data} holds one value per segment, but it draws no line, so it has no segments to \
             colour"
        ));
    }
    let mapped_markers = series.draws_markers()
        && [series.marker.face, series.marker.edge].contains(&ColorSpec::Colormapped);
    if mapped_markers {
        return Err(format!(
            "its colour array {data} holds one value per segment, but a colormapped marker face or edge needs one \
             value per point"
        ));
    }
    Ok(SeriesColours::PerSegment(values))
}

/// The transparency of an image artist: the identifier of its alpha array, if it has one, and its opacity.
struct Look {
    alpha: Option<NodeId>,
    opacity: f64,
}

/// Resolves the array of an image artist of any kind, and checks that its shape suits the kind, that its alpha, if
/// it has one, has a value for every pixel, that the placement of the image can be drawn, that the axes can show its
/// plane, and that it has at least one row and one column of pixels, because an image with none draws nothing and
/// is left out with a warning that names it.
fn image_data<'a>(
    figure: &'a Figure,
    axes: &Axes,
    id: NodeId,
    kind: ImageKind,
    placement: ImagePlacement,
    look: Look,
) -> Result<ArtistData<'a>, String> {
    let array = array(figure, id)?;
    let what = match kind {
        ImageKind::TrueColour => "pixels",
        ImageKind::Scalar => "values",
    };
    let (ny, nx, components) = match (kind, array.shape.as_slice()) {
        (ImageKind::TrueColour, &[ny, nx, components @ (3 | 4)]) => (ny, nx, components),
        (ImageKind::TrueColour, shape) => {
            return Err(format!(
                "its pixels {id} have shape {shape:?}, but they must be a three-dimensional array whose last \
                 dimension holds the 3 or 4 colour components of a pixel"
            ));
        }
        (ImageKind::Scalar, &[ny, nx]) => (ny, nx, 1),
        (ImageKind::Scalar, shape) => {
            return Err(format!(
                "its {what} {id} have shape {shape:?}, but they must be two-dimensional"
            ));
        }
    };
    let alpha = look
        .alpha
        .map(|alpha_id| {
            let alpha = self::array(figure, alpha_id)?;
            if alpha.shape == [ny, nx] {
                Ok(alpha)
            } else {
                Err(format!(
                    "its alpha {alpha_id} has shape {:?}, but its {what} {id} have shape {:?}",
                    alpha.shape,
                    [ny, nx]
                ))
            }
        })
        .transpose()?;
    check_placement(placement, nx, ny)?;
    check_plane(axes, placement.plane)?;
    let empty = match (ny, nx) {
        (0, 0) => Some("no rows and no columns"),
        (0, _) => Some("no rows"),
        (_, 0) => Some("no columns"),
        _ => None,
    };
    if let Some(case) = empty {
        return Err(format!(
            "its {what} {id} have shape {:?}, so it has {case} of pixels",
            array.shape
        ));
    }
    if u32::try_from(nx).is_err() || u32::try_from(ny).is_err() {
        return Err(format!(
            "it has {ny} rows and {nx} columns of pixels, and an image can have at most {} of each",
            u32::MAX
        ));
    }
    Ok(ArtistData::Image(ImageData {
        kind,
        array,
        nx,
        ny,
        components,
        placement,
        alpha,
        opacity: look.opacity,
    }))
}

/// Checks that an artist that maps numbers to colours names a colour scale of the figure, and that none of the
/// numbers it maps falls in a category of the scale whose policy is strict, as validation does: the values of a
/// scalar image, the field of a contour, the colour data of a surface (or its field) and the colour data of a
/// series. Numbers derived from them, such as the mean of the corners of a surface face, are not checked. The first
/// refused number is named, by its row and column in an image and by its index otherwise.
fn check_colour_mapping(
    figure: &Figure,
    artist: &Artist,
    data: &ArtistData,
    scales: &ColourScales,
) -> Result<(), String> {
    let Some(mapping) = artist.color_mapping() else {
        return Ok(());
    };
    let scale = scales.named(mapping.color_scale)?;
    let Some(array) = figure.data_array(mapping.values) else {
        return Ok(());
    };
    let refused = |k: usize, value: f64| {
        let (category, condition) = scale.strict_category(value)?;
        let place = match data {
            ArtistData::Image(image) => format!(
                "the pixel in row {} and column {}",
                k / image.nx,
                k % image.nx
            ),
            _ => format!("the value at index {k} of {}", mapping.values),
        };
        Some(format!(
            "{place} {condition}, which the strict {category} policy of its colour scale {} refuses",
            scale.scale.id
        ))
    };
    let first = match &array.array.values {
        Values::F64(values) => values
            .iter()
            .enumerate()
            .find_map(|(k, &value)| refused(k, value)),
        Values::U8(values) => values
            .iter()
            .enumerate()
            .find_map(|(k, &value)| refused(k, f64::from(value))),
    };
    first.map_or(Ok(()), Err)
}

/// Checks that an image of `nx` columns and `ny` rows can be placed: its pixel centres and plane offset must be
/// finite, and the centres of its first and last pixels along an axis may coincide only when it has one pixel
/// along that axis, which validation reports in the same terms.
fn check_placement(placement: ImagePlacement, nx: usize, ny: usize) -> Result<(), String> {
    if let Some(offset) = placement.plane.offset()
        && !offset.is_finite()
    {
        return Err(format!("the offset {offset} of its plane is not finite"));
    }
    let ranges = [
        ("columns", placement.columns, nx),
        ("rows", placement.rows, ny),
    ];
    for (what, range, count) in ranges {
        let Some(PixelRange { first, last }) = range else {
            continue;
        };
        if !(first.is_finite() && last.is_finite()) {
            return Err(format!(
                "the centres of its first and last {what} [{first}, {last}] are not finite"
            ));
        }
        if first == last && count > 1 {
            return Err(format!(
                "the centres of its first and last {what} coincide at {first}, but it has {count} {what}"
            ));
        }
    }
    Ok(())
}

/// Returns the name of the plane of an image.
fn plane_name(plane: ImagePlane) -> &'static str {
    match plane {
        ImagePlane::Xy { .. } => "xy",
        ImagePlane::Xz { .. } => "xz",
        ImagePlane::Yz { .. } => "yz",
    }
}

/// Checks that an axes can show the plane of an image: a two-dimensional axes shows only the xy plane, and a raster
/// of flat pixels, whose pitch is one distance everywhere, cannot be placed along a logarithmic axis of its plane.
/// The third axis, along which the plane is only offset, may be logarithmic, but then the offset must be positive
/// to be placed on it; a two-dimensional axes ignores the offset.
fn check_plane(axes: &Axes, plane: ImagePlane) -> Result<(), String> {
    let three_d = is_3d(axes);
    let name = plane_name(plane);
    if !three_d && !matches!(plane, ImagePlane::Xy { .. }) {
        return Err(format!(
            "it lies in the {name} plane, which only a three-dimensional axes has"
        ));
    }
    let logarithmic: Vec<&str> = plane
        .axes()
        .into_iter()
        .filter(|&dimension| axes_axis(axes, dimension_index(dimension)).scale == Scale::Log)
        .map(|dimension| DIMENSION_NAMES[dimension_index(dimension)])
        .collect();
    if !logarithmic.is_empty() {
        let (noun, verb) = if logarithmic.len() == 1 {
            ("axis", "is")
        } else {
            ("axes", "are")
        };
        return Err(format!(
            "it lies in the {name} plane, whose {} {noun} {verb} logarithmic, and a raster of flat pixels \
             cannot be placed along a logarithmic axis",
            logarithmic.join(" and ")
        ));
    }
    if three_d && let Some(offset) = plane.offset() {
        let [columns, rows] = plane.axes().map(dimension_index);
        let third = 3 - columns - rows;
        if axes_axis(axes, third).scale == Scale::Log && offset <= 0.0 {
            return Err(format!(
                "its plane is offset to {offset} along the logarithmic {} axis, where it cannot be placed",
                DIMENSION_NAMES[third]
            ));
        }
    }
    Ok(())
}

/// Computes the factor applied to quiver vectors, counting only arrows whose base and vector are finite.
fn quiver_scale(scale: QuiverScale, points: Points, vectors: Points) -> f64 {
    let auto = || {
        let (bases, vecs): (Vec<[f64; 3]>, Vec<[f64; 3]>) = (0..points.len())
            .map(|i| (points.get(i), vectors.get(i)))
            .filter(|(b, v)| b.iter().chain(v).all(|c| c.is_finite()))
            .unzip();
        quiver::auto_scale(&bases, &vecs)
    };
    match scale {
        QuiverScale::Auto => auto(),
        QuiverScale::Factor { value } if value.is_finite() => auto() * value,
        QuiverScale::Factor { .. } => auto(),
        QuiverScale::Off => 1.0,
    }
}

/// Returns the index of a dimension in a data point: 0, 1 or 2 for x, y or z.
pub(super) fn dimension_index(dimension: Dimension) -> usize {
    match dimension {
        Dimension::X => 0,
        Dimension::Y => 1,
        Dimension::Z => 2,
    }
}

/// Returns the coordinate values an artist places along dimension `dim` (0, 1 or 2 for x, y or z).
///
/// For a 2D axes, nothing is placed along z. For a series or quiver in a 3D axes without z data, the value
/// 0 is placed along z. The quiver values include the arrow tips. Contour values along z depend on the placement:
/// a plane at an explicit height places that height, the bottom plane places nothing, and contours at their levels
/// place the field values. An image places the two edges of its pixels along each axis of its plane and, along the
/// third axis, the offset of its plane when it has one.
///
/// A series in a 2D axes places nothing along a dimension whose values are fractions of the plot area,
/// because those values are not positions along the axis: they take no part in its automatic limits, and a zero or
/// negative fraction is no concern of a logarithmic axis. Its other dimension, in data coordinates, places its values
/// as usual, so a reference line spanning the axes at a data value still takes part in the automatic limits of the
/// axis it crosses, as Matplotlib's `axhline` and `axvline` do.
pub(crate) fn values_along(
    artist: &Artist,
    data: &ArtistData,
    three_d: bool,
    dim: usize,
    visit: &mut dyn FnMut(f64),
) {
    if dim == 2 && !three_d {
        return;
    }
    if !three_d && dim < 2 && Placement::of(artist).coords[dim] == ArtistCoords::AxesFraction {
        return;
    }
    let mut point_values = |points: &Points| match dim {
        0 => points.x.iter().for_each(|v| visit(*v)),
        1 => points.y.iter().for_each(|v| visit(*v)),
        _ => match points.z {
            Some(z) => z.iter().for_each(|v| visit(*v)),
            None if points.len() > 0 => visit(0.0),
            None => {}
        },
    };
    match data {
        ArtistData::Series { points, .. } => point_values(points),
        ArtistData::Quiver {
            points,
            vectors,
            scale,
        } => {
            point_values(points);
            for i in 0..points.len() {
                let (b, v) = (points.get(i), vectors.get(i));
                if b.iter().chain(&v).all(|c| c.is_finite()) {
                    visit(b[dim] + scale * v[dim]);
                }
            }
        }
        ArtistData::Contour(grid) | ArtistData::Surface { grid, .. } => {
            let (x, y) = match grid.coords {
                Coords::Rectilinear { x, y } | Coords::Curvilinear { x, y } => (x, y),
            };
            match dim {
                0 => x.iter().for_each(|v| visit(*v)),
                1 => y.iter().for_each(|v| visit(*v)),
                _ => match artist {
                    Artist::Contour(c) => match c.placement {
                        ContourPlacement::Plane { z: Some(z) } => visit(z),
                        ContourPlacement::Plane { z: None } => {}
                        ContourPlacement::AtLevel => grid.z.iter().for_each(|v| visit(*v)),
                    },
                    _ => grid.z.iter().for_each(|v| visit(*v)),
                },
            }
        }
        ArtistData::Image(image) => {
            let [columns, rows] = image.plane_dims();
            let edges = if dim == columns {
                Some(image::edges(image.placement.columns, image.nx))
            } else if dim == rows {
                Some(image::edges(image.placement.rows, image.ny))
            } else {
                None
            };
            match edges {
                Some((lo, hi)) => {
                    visit(lo);
                    visit(hi);
                }
                None => {
                    if let Some(offset) = image.placement.plane.offset() {
                        visit(offset);
                    }
                }
            }
        }
    }
}

const DIMENSION_NAMES: [&str; 3] = ["x", "y", "z"];

/// Warns when an artist places non-positive values along a logarithmic axis, where they cannot be drawn.
fn warn_log_drops(ctx: &mut Ctx, axes: &Axes, artist: &Artist, data: &ArtistData) {
    let three_d = is_3d(axes);
    for (dim, axis) in [&axes.x, &axes.y, &axes.z].into_iter().enumerate() {
        if axis.scale != Scale::Log {
            continue;
        }
        let mut dropped = false;
        values_along(artist, data, three_d, dim, &mut |v| {
            dropped |= v.is_finite() && v <= 0.0
        });
        if dropped {
            let name = DIMENSION_NAMES[dim];
            ctx.warn(
                Some(artist.id()),
                format!(
                    "Some {name} values are zero or negative; they cannot be placed on the logarithmic {name} axis \
                     and are not drawn."
                ),
            );
        }
    }
}
