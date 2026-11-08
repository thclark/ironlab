//! Polar axes: converting an axes to polar axes, the setters of its polar view, the
//! readability aliases of its limits and scale, `polarplot` and `polarscatter`, and the charts
//! built from ordinary artists in polar axes: radar charts, polar histograms and wind roses.
//!
//! A polar axes is an ordinary axes whose projection is polar: x carries the angle and y the
//! radius. Every builder draws into it unchanged, so these functions only set the projection
//! and its view, name the x and y setters in the words of polar plots, and prepare the data of
//! the charts that are drawn with series, histograms and stacked bars.

use ironlab_ir::{
    AngleUnits, ColourSpec, Limits, MarkerShape, PolarAngle, PolarView, Projection, Scale,
    SphericalView, Text, Ticks, View3d,
};

use crate::artists::{BarsMut, SeriesMut};
use crate::axes::AxesMut;
use crate::binning::Bins;
use crate::data::DataSource;
use crate::hist::HistMut;

/// The number of bins of a polar histogram, and of sectors of a wind rose, over a full turn:
/// the sixteen points of the compass.
const COMPASS_POINTS: usize = 16;

/// The fraction of its sector that each petal of a wind rose leaves empty, so that
/// neighbouring petals read as separate sectors.
const WIND_ROSE_GAP: f64 = 0.1;

/// The opacity of the fill of a radar outline, low enough that overlaid outlines stay readable.
const RADAR_FILL_OPACITY: f64 = 0.25;

/// The direction on the page in which angle 0 points: a compass point, or any angle in
/// degrees anticlockwise from east.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThetaZero {
    /// To the right, which is 0 degrees and the default.
    East,
    /// Up the page, which is 90 degrees, as on a compass or an antenna pattern.
    North,
    /// To the left, which is 180 degrees.
    West,
    /// Down the page, which is 270 degrees.
    South,
    /// The given number of degrees anticlockwise from east.
    Degrees(f64),
}

impl ThetaZero {
    /// Returns the direction in degrees anticlockwise from east.
    #[must_use]
    pub fn degrees(self) -> f64 {
        match self {
            ThetaZero::East => 0.0,
            ThetaZero::North => 90.0,
            ThetaZero::West => 180.0,
            ThetaZero::South => 270.0,
            ThetaZero::Degrees(degrees) => degrees,
        }
    }
}

impl AxesMut<'_> {
    /// Converts the axes to polar axes, in which x is the angle and y the radius, keeping its
    /// artists and properties (MATLAB's `polaraxes`, matplotlib's `projection="polar"`).
    ///
    /// A two-dimensional axes becomes a polar axes with the default view: angles in degrees,
    /// angle 0 to the east, increasing anticlockwise, and no hole. A three-dimensional axes
    /// becomes a cylindrical one with the same camera. A polar, cylindrical or spherical axes is
    /// left as it is.
    pub fn make_polar(&mut self) -> &mut Self {
        let axes = self.axes();
        axes.projection = match axes.projection {
            Projection::TwoD => Projection::Polar {
                polar: PolarView::default(),
            },
            Projection::ThreeD { view3d } => Projection::Cylindrical {
                view3d,
                polar: PolarView::default(),
            },
            other => other,
        };
        self
    }

    /// Converts the axes to cylindrical axes, in which x is the angle about the z axis, y the
    /// distance from it and z the height, keeping its artists and properties.
    ///
    /// A two-dimensional or spherical axes becomes cylindrical with the default polar view
    /// (angles in degrees, angle 0 along +X, increasing anticlockwise seen from above, and no
    /// hole); a polar axes keeps its polar view. A three-dimensional or spherical axes keeps
    /// its camera, and any other axes takes the default camera. A cylindrical axes is left as
    /// it is.
    pub fn make_cylindrical(&mut self) -> &mut Self {
        let axes = self.axes();
        axes.projection = match axes.projection {
            Projection::TwoD => Projection::Cylindrical {
                view3d: View3d::default(),
                polar: PolarView::default(),
            },
            Projection::Polar { polar } => Projection::Cylindrical {
                view3d: View3d::default(),
                polar,
            },
            Projection::ThreeD { view3d } | Projection::Spherical { view3d, .. } => {
                Projection::Cylindrical {
                    view3d,
                    polar: PolarView::default(),
                }
            }
            cylindrical @ Projection::Cylindrical { .. } => cylindrical,
        };
        self
    }

    /// Converts the axes to spherical axes, in which x is the azimuth, y the polar angle and
    /// z the distance from the centre, keeping its artists and properties.
    ///
    /// The axes takes the default spherical view (angles in degrees, the polar angle measured
    /// as an elevation above the X–Y plane), keeping the angle units of a polar or cylindrical
    /// axes. A three-dimensional or cylindrical axes keeps its camera, and any other axes
    /// takes the default camera. A spherical axes is left as it is.
    pub fn make_spherical(&mut self) -> &mut Self {
        let axes = self.axes();
        let spherical = |units: AngleUnits| SphericalView {
            angle_units: units,
            ..SphericalView::default()
        };
        axes.projection = match axes.projection {
            Projection::TwoD => Projection::Spherical {
                view3d: View3d::default(),
                spherical: SphericalView::default(),
            },
            Projection::Polar { polar } => Projection::Spherical {
                view3d: View3d::default(),
                spherical: spherical(polar.angle_units),
            },
            Projection::ThreeD { view3d } => Projection::Spherical {
                view3d,
                spherical: SphericalView::default(),
            },
            Projection::Cylindrical { view3d, polar } => Projection::Spherical {
                view3d,
                spherical: spherical(polar.angle_units),
            },
            existing @ Projection::Spherical { .. } => existing,
        };
        self
    }

    /// Sets how the polar angle (y) of a spherical axes is measured: up from the X–Y plane
    /// (elevation, the default, as in antenna patterns and geography) or down from +Z
    /// (inclination, as in physics), converting the axes to spherical axes as
    /// [`make_spherical`](AxesMut::make_spherical) does.
    pub fn polar_angle(&mut self, polar_angle: PolarAngle) -> &mut Self {
        self.make_spherical();
        if let Projection::Spherical { spherical, .. } = &mut self.axes().projection {
            spherical.polar_angle = polar_angle;
        }
        self
    }

    /// Applies `edit` to the polar view of the axes, converting the axes to polar axes first,
    /// as [`make_polar`](AxesMut::make_polar) does. A spherical axes has no polar view and is
    /// left as it is.
    fn edit_polar(&mut self, edit: impl FnOnce(&mut PolarView)) -> &mut Self {
        self.make_polar();
        match &mut self.axes().projection {
            Projection::Polar { polar } | Projection::Cylindrical { polar, .. } => edit(polar),
            _ => {}
        }
        self
    }

    /// Sets the direction on the page in which angle 0 points, converting the axes to polar
    /// axes (matplotlib's `set_theta_zero_location`, MATLAB's `ThetaZeroLocation`).
    pub fn theta_zero(&mut self, zero: ThetaZero) -> &mut Self {
        self.edit_polar(|polar| polar.zero_deg = zero.degrees())
    }

    /// Sets whether angles increase clockwise rather than anticlockwise, converting the axes to
    /// polar axes (MATLAB's `ThetaDir`, matplotlib's `set_theta_direction`).
    ///
    /// The direction is the direction of the x axis: clockwise angles are a reversed x axis.
    pub fn clockwise(&mut self, clockwise: bool) -> &mut Self {
        self.make_polar();
        self.axes().x.reverse = clockwise;
        self
    }

    /// Sets the units in which the angles of the data are given, converting the axes to polar
    /// axes: degrees (the default), radians, or any number to a turn, such as 24 hours or the
    /// number of categories of a radar chart. In a spherical axes it sets the units of both
    /// angles.
    pub fn angle_units(&mut self, units: AngleUnits) -> &mut Self {
        if let Projection::Spherical { spherical, .. } = &mut self.axes().projection {
            spherical.angle_units = units;
            return self;
        }
        self.edit_polar(|polar| polar.angle_units = units)
    }

    /// Sets the fraction of the rim radius left empty at the centre, at least 0 and less than
    /// 1, converting the axes to polar axes; the lower radial limit lies on the edge of the
    /// hole (matplotlib's `set_rorigin`, as a fraction).
    pub fn hole(&mut self, fraction: f64) -> &mut Self {
        self.edit_polar(|polar| polar.hole = fraction)
    }

    /// Sets the angle, in the units of the data, of the spoke along which the radial tick
    /// labels are drawn, converting the axes to polar axes (matplotlib's
    /// `set_rlabel_position`, MATLAB's `RAxisLocation`).
    pub fn radial_axis_angle(&mut self, angle: f64) -> &mut Self {
        self.edit_polar(|polar| polar.radial_axis_angle = Some(angle))
    }

    /// Fixes the angular limits, which are the sector the axes shows; an alias of
    /// [`xlim`](AxesMut::xlim) (MATLAB's `thetalim`).
    pub fn thetalim(&mut self, min: f64, max: f64) -> &mut Self {
        self.xlim(min, max)
    }

    /// Fixes the radial limits: the lower limit lies at the centre, or on the edge of the
    /// hole, and the upper limit at the rim; an alias of [`ylim`](AxesMut::ylim) (MATLAB's
    /// `rlim`).
    pub fn rlim(&mut self, min: f64, max: f64) -> &mut Self {
        self.ylim(min, max)
    }

    /// Sets whether the radius is linear or logarithmic; an alias of
    /// [`yscale`](AxesMut::yscale) (matplotlib's `set_rscale`).
    pub fn rscale(&mut self, scale: Scale) -> &mut Self {
        self.yscale(scale)
    }

    /// Plots a line through the points at angles `theta` and radii `r` (MATLAB's
    /// `polarplot`), converting the axes to polar axes.
    ///
    /// The series is exactly the one [`plot`](AxesMut::plot) adds: consecutive points are
    /// joined by straight lines, and the line is not closed, so a closed curve repeats its
    /// first point at its end.
    pub fn polarplot(
        &mut self,
        theta: impl Into<DataSource>,
        r: impl Into<DataSource>,
    ) -> SeriesMut<'_> {
        self.make_polar();
        self.plot(theta, r)
    }

    /// Draws markers at the points at angles `theta` and radii `r` (MATLAB's `polarscatter`),
    /// converting the axes to polar axes; the series is exactly the one
    /// [`scatter`](AxesMut::scatter) adds.
    pub fn polarscatter(
        &mut self,
        theta: impl Into<DataSource>,
        r: impl Into<DataSource>,
    ) -> SeriesMut<'_> {
        self.make_polar();
        self.scatter(theta, r)
    }

    /// Returns one turn in the angle units of the axes: 360 for degrees, 2π for radians, or
    /// the number of units to a turn.
    fn turn(&mut self) -> f64 {
        match self.axes().projection {
            Projection::Polar { polar } | Projection::Cylindrical { polar, .. } => {
                polar.angle_units.turn()
            }
            Projection::Spherical { spherical, .. } => spherical.angle_units.turn(),
            Projection::TwoD | Projection::ThreeD { .. } => AngleUnits::Degrees.turn(),
        }
    }

    /// Converts the axes to the polar axes of a radar chart with one spoke for each label
    /// (Plotly's `scatterpolar`, Chart.js's `radar`).
    ///
    /// The angle is measured in one unit per spoke, so that spoke `k` lies at angle `k`: the
    /// angle units are `n` to a turn for `n` labels, the angular limits are fixed at `[0, n]`,
    /// the angular ticks are `0, 1, …, n − 1` labelled with the labels, spoke 0 points north,
    /// the spokes follow one another clockwise, and the spokes and rings of the grid are drawn.
    /// The radial limits are shared by every outline, as in any axes, so criteria measured on
    /// different scales are normalised before they are plotted.
    pub fn spokes<T: Clone + Into<Text>>(&mut self, labels: &[T]) -> &mut Self {
        let n = labels.len() as f64;
        self.angle_units(AngleUnits::PerTurn(n))
            .theta_zero(ThetaZero::North)
            .clockwise(true);
        let axes = self.axes();
        axes.x.limits = Limits::Manual { min: 0.0, max: n };
        axes.x.ticks = Ticks::Values((0..labels.len()).map(|k| k as f64).collect());
        axes.x.grid = true;
        axes.y.grid = true;
        self.xtick_labels(labels.iter().cloned())
    }

    /// Adds the closed outline of a radar chart through one value per spoke, converting the
    /// axes to polar axes. The axes are prepared by [`spokes`](Self::spokes), whose number of
    /// labels is the number of spokes; without spokes there is one spoke per value.
    ///
    /// The outline is an ordinary series through the points `(k, values[k])`, with the first
    /// value repeated at `x = n`, which is spoke 0 a turn later, so that the outline closes
    /// with a straight edge back to the first spoke. It draws a line and circle markers, and is
    /// filled in the colour of the series at an opacity of 0.25 down to its base: the lower
    /// radial limit, when the radial limits are fixed before the outline is added, and 0
    /// otherwise. Values of the
    /// wrong number for the spokes are stored as given, and
    /// [`Figure::validate`](crate::Figure::validate) reports the mismatch.
    pub fn radar(&mut self, values: impl Into<DataSource>) -> SeriesMut<'_> {
        self.make_polar();
        let mut values = values.into().read(self.fig);
        let spokes = match self.axes().projection {
            Projection::Polar {
                polar:
                    PolarView {
                        angle_units: AngleUnits::PerTurn(n),
                        ..
                    },
            } if n.is_finite() && n >= 1.0 && n.fract() == 0.0 => n as usize,
            _ => values.len(),
        };
        let base = match self.axes().y.limits {
            Limits::Manual { min, .. } => min,
            Limits::Auto => 0.0,
        };
        if let Some(&first) = values.first() {
            values.push(first);
        }
        let x: Vec<f64> = (0..=spokes).map(|k| k as f64).collect();
        let mut series = self.plot(x, values);
        series
            .marker(MarkerShape::Circle)
            .base(base)
            .fill(ColourSpec::Auto)
            .fill_opacity(RADAR_FILL_OPACITY);
        series
    }

    /// Draws a histogram of angles over one turn (MATLAB's `polarhistogram`), converting the
    /// axes to polar axes.
    ///
    /// Each finite angle is wrapped into `[0, turn)` of the angle units of the axes, so that an
    /// angle a whole number of turns away from another counts in the same bin: −10° counts as
    /// 350°. The wrapped angles are then binned exactly as [`hist`](Self::hist) bins a sample,
    /// with the range fixed to one turn, so that the last bin adjoins the first, and sixteen
    /// bins unless [`HistMut::bins`] sets others. The bars are wedges of the polar axes, and the
    /// step style draws stairs whose runs are arcs. The record of the binning describes the
    /// wrapped sample.
    pub fn polar_hist(&mut self, angles: impl Into<DataSource>) -> HistMut<'_> {
        self.make_polar();
        let turn = self.turn();
        let wrapped: Vec<f64> = angles
            .into()
            .read(self.fig)
            .into_iter()
            .map(|angle| {
                if angle.is_finite() {
                    angle.rem_euclid(turn)
                } else {
                    angle
                }
            })
            .collect();
        let mut hist = HistMut::create(self.fig, self.id, wrapped.into());
        hist.bins(Bins::Count(COMPASS_POINTS as u32))
            .range(0.0, turn);
        hist
    }

    /// Draws a wind rose of observations of the direction the wind blows from, in degrees
    /// clockwise from north, and of its speed (python-windrose's `WindroseAxes.bar`),
    /// converting the axes to polar axes in degrees with north up and bearings clockwise.
    ///
    /// The directions are binned into sixteen sectors centred on the compass points 0°, 22.5°,
    /// …, 337.5°, each spanning half a sector either side of its centre, so that the sector of
    /// north spans from 348.75° to 11.25°. The speeds are binned into the classes between
    /// consecutive edges of `class_edges`, which increase: a class holds the speeds from its
    /// lower edge up to, but not including, its upper edge, and the last class is open above
    /// its edge. A speed below the first edge is a calm: it counts towards the total but is
    /// drawn in no class. Each class is a stacked bar series at the sector centres, labelled by
    /// its range of speeds without a unit (`"2–4"`, and `"≥ 8"`, set as `$\geq$ 8`, for the last), whose values
    /// are the percentages of all the observations whose direction and speed are both finite;
    /// the petals therefore sum to 100 less the percentage of calms. The returned helper sets
    /// the gap between petals and styles the classes.
    pub fn wind_rose(
        &mut self,
        directions: impl Into<DataSource>,
        speeds: impl Into<DataSource>,
        class_edges: impl AsRef<[f64]>,
    ) -> BarsMut<'_> {
        self.angle_units(AngleUnits::Degrees)
            .theta_zero(ThetaZero::North)
            .clockwise(true);
        let directions = directions.into().read(self.fig);
        let speeds = speeds.into().read(self.fig);
        let edges = class_edges.as_ref();
        let sector = 360.0 / COMPASS_POINTS as f64;
        let mut counts = vec![vec![0.0; COMPASS_POINTS]; edges.len()];
        let mut total = 0.0;
        for (&direction, &speed) in directions.iter().zip(&speeds) {
            if !(direction.is_finite() && speed.is_finite()) {
                continue;
            }
            total += 1.0;
            let Some(class) = edges.iter().rposition(|&edge| speed >= edge) else {
                continue;
            };
            let k = ((direction.rem_euclid(360.0) + sector / 2.0) / sector).floor() as usize
                % COMPASS_POINTS;
            counts[class][k] += 1.0;
        }
        let centres: Vec<f64> = (0..COMPASS_POINTS).map(|k| sector * k as f64).collect();
        let mut rose = self.bars(centres);
        rose.gap(WIND_ROSE_GAP);
        for (class, counts) in counts.into_iter().enumerate() {
            let label = match edges.get(class + 1) {
                Some(upper) => format!("{}–{upper}", edges[class]),
                // The text font has no sign for "at least", so it is set as mathematics.
                None => format!("$\\geq$ {}", edges[class]),
            };
            let percentages: Vec<f64> = counts
                .into_iter()
                .map(|count| {
                    if total > 0.0 {
                        100.0 * count / total
                    } else {
                        0.0
                    }
                })
                .collect();
            rose.stacked("wind rose", label, percentages);
        }
        rose
    }
}
