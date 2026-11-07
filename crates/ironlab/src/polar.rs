//! Polar axes: converting an axes to polar axes, the setters of its polar view, the
//! readability aliases of its limits and scale, and `polarplot` and `polarscatter`.
//!
//! A polar axes is an ordinary axes whose projection is polar: x carries the angle and y the
//! radius. Every builder draws into it unchanged, so these functions only set the projection
//! and its view, and name the x and y setters in the words of polar plots.

use ironlab_ir::{AngleUnits, PolarView, Projection, Scale};

use crate::artists::SeriesMut;
use crate::axes::AxesMut;
use crate::data::DataSource;

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
    pub fn radial_labels_at(&mut self, angle: f64) -> &mut Self {
        self.edit_polar(|polar| polar.radial_labels_at = Some(angle))
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
}
