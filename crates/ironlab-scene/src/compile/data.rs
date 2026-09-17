//! Resolution and checking of artist data.
//!
//! Every artist's data identifiers are looked up once, and the shapes of its arrays are checked, before limits are
//! computed or anything is drawn. An artist whose data cannot be used is reported with a warning naming it and is
//! then ignored by every later stage.

use ironlab_ir::{
    Artist, Axes, ContourPlacement, DataId, Figure, Grid, NdArray, Projection, QuiverScale, Scale,
    ScatterColor, ScatterSize,
};

use crate::maths::contour::{Coords, GridRef};
use crate::maths::quiver;

use super::Ctx;

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

/// The resolved arrays of an artist.
#[derive(Clone, Copy)]
pub(crate) enum ArtistData<'a> {
    Line(Points<'a>),
    Scatter {
        points: Points<'a>,
        sizes: Option<&'a [f64]>,
        colours: Option<&'a [f64]>,
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
    },
}

/// Resolves and checks the data of every artist of an axes.
pub(super) fn prepare_axes<'a>(ctx: &mut Ctx<'a>, axes: &'a Axes) -> Vec<Prepared<'a>> {
    let figure = ctx.figure;
    axes.artists
        .iter()
        .map(|artist| {
            let data = match resolve(figure, artist) {
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
fn array(figure: &Figure, id: DataId) -> Result<&NdArray, String> {
    let array = figure
        .data
        .get(&id)
        .ok_or_else(|| format!("it refers to {id}, which is not in the data table"))?;
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

/// Looks up an array that must hold `len` values.
fn values_of_len<'a>(
    figure: &'a Figure,
    id: DataId,
    len: usize,
    what: &str,
) -> Result<&'a [f64], String> {
    let values = &array(figure, id)?.values;
    if values.len() != len {
        return Err(format!(
            "its {what} array holds {} values but its x array holds {len}",
            values.len()
        ));
    }
    Ok(values)
}

/// Resolves the x, y and optional z arrays of a point set, which must have equal lengths.
fn points(figure: &Figure, x: DataId, y: DataId, z: Option<DataId>) -> Result<Points<'_>, String> {
    let x = array(figure, x)?.values.as_slice();
    let y = values_of_len(figure, y, x.len(), "y")?;
    let z = z
        .map(|z| values_of_len(figure, z, x.len(), "z"))
        .transpose()?;
    Ok(Points { x, y, z })
}

/// Resolves a gridded field: its `[ny, nx]` values and the grid coordinates.
fn grid<'a>(figure: &'a Figure, grid: &Grid, z: DataId) -> Result<GridRef<'a>, String> {
    let field = array(figure, z)?;
    let [ny, nx] = field.shape[..] else {
        return Err(format!(
            "its field {z} has shape {:?}, but a two-dimensional array is required",
            field.shape
        ));
    };
    let coords = match *grid {
        Grid::Rectilinear { x, y } => Coords::Rectilinear {
            x: &array(figure, x)?.values,
            y: &array(figure, y)?.values,
        },
        Grid::Curvilinear { x, y } => Coords::Curvilinear {
            x: &array(figure, x)?.values,
            y: &array(figure, y)?.values,
        },
    };
    let grid = GridRef {
        nx,
        ny,
        coords,
        z: &field.values,
    };
    grid.validate()
        .map_err(|e| format!("its grid is invalid: {e}"))?;
    Ok(grid)
}

/// Resolves the data of one artist.
fn resolve<'a>(figure: &'a Figure, artist: &Artist) -> Result<ArtistData<'a>, String> {
    Ok(match artist {
        Artist::Line(line) => ArtistData::Line(points(figure, line.x, line.y, line.z)?),
        Artist::Scatter(scatter) => {
            let points = points(figure, scatter.x, scatter.y, scatter.z)?;
            let sizes = match scatter.size {
                ScatterSize::Scalar { .. } => None,
                ScatterSize::Data { data } => {
                    Some(values_of_len(figure, data, points.len(), "size")?)
                }
            };
            let colours = match scatter.color {
                ScatterColor::Spec { .. } => None,
                ScatterColor::Data { data } => {
                    Some(values_of_len(figure, data, points.len(), "colour")?)
                }
            };
            ArtistData::Scatter {
                points,
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
            let scale = quiver_scale(q.scale, points, vectors);
            ArtistData::Quiver {
                points,
                vectors,
                scale,
            }
        }
        Artist::Surface(surface) => {
            let grid = grid(figure, &surface.grid, surface.z)?;
            let colours = surface
                .c
                .map(|c| {
                    let values = &array(figure, c)?.values;
                    if values.len() == grid.z.len() {
                        Ok(values.as_slice())
                    } else {
                        Err(format!(
                            "its colour array {c} holds {} values but its field holds {}",
                            values.len(),
                            grid.z.len()
                        ))
                    }
                })
                .transpose()?;
            ArtistData::Surface { grid, colours }
        }
    })
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

/// Returns the coordinate values an artist places along dimension `dim` (0, 1 or 2 for x, y or z).
///
/// For a 2D axes, nothing is placed along z. For a line, scatter or quiver in a 3D axes without z data, the value
/// 0 is placed along z. The quiver values include the arrow tips. Contour values along z depend on the placement:
/// a plane at an explicit height places that height, the bottom plane places nothing, and contours at their levels
/// place the field values.
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
        ArtistData::Line(points) | ArtistData::Scatter { points, .. } => point_values(points),
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
    }
}

const DIMENSION_NAMES: [&str; 3] = ["x", "y", "z"];

/// Warns when an artist places non-positive values along a logarithmic axis, where they cannot be drawn.
fn warn_log_drops(ctx: &mut Ctx, axes: &Axes, artist: &Artist, data: &ArtistData) {
    let three_d = matches!(axes.projection, Projection::ThreeD { .. });
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
