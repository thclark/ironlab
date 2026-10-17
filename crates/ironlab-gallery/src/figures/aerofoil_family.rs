use ironlab::prelude::*;

pub const TITLE: &str = "NACA 4-digit family sheet at equal aspect";
pub const DESCRIPTION: &str = "The NACA 4-digit aerofoil sections, computed analytically, varied one parameter per \
     panel about the default section NACA 4412: the maximum camber $m$, the chordwise position $p$ of the maximum \
     camber and the thickness $t$. Each panel is drawn at equal aspect, so that one unit of chord has the same \
     length along x and y and a 12 % thick section looks 12 % thick; the aspect is kept by shrinking each plot area \
     to the fixed limits, which every panel shares through x and y links, with the same regular ticks. The long \
     panel titles are aligned with the left edge of each plot area. The five members of each panel are told apart \
     by five dash patterns, the default section being the solid line.";

/// The default section of the family: maximum camber, its chordwise position and thickness, as fractions of the
/// chord.
const DEFAULT: Section = Section {
    m: 0.04,
    p: 0.4,
    t: 0.12,
};

/// The dash patterns of the five members of a panel, the default section first.
const DASHES: [Dash; 5] = [
    Dash::Solid,
    Dash::Dashed,
    Dash::DashDot,
    Dash::DashDotDot,
    Dash::LongDash,
];

/// A NACA 4-digit section.
#[derive(Clone, Copy)]
struct Section {
    /// The maximum camber, as a fraction of the chord.
    m: f64,
    /// The chordwise position of the maximum camber, as a fraction of the chord.
    p: f64,
    /// The maximum thickness, as a fraction of the chord.
    t: f64,
}

impl Section {
    /// The NACA designation, such as `4412`, of a section whose parameters are whole digits.
    fn designation(self) -> String {
        format!(
            "{}{}{:02}",
            (self.m * 100.0).round(),
            (self.p * 10.0).round(),
            (self.t * 100.0).round()
        )
    }

    /// The half thickness at chordwise position `x`, with the closed trailing edge of the modified coefficient.
    fn half_thickness(self, x: f64) -> f64 {
        5.0 * self.t
            * (0.2969 * x.sqrt() - 0.1260 * x - 0.3516 * x.powi(2) + 0.2843 * x.powi(3)
                - 0.1036 * x.powi(4))
    }

    /// The height of the camber line at `x` and its slope.
    fn camber(self, x: f64) -> (f64, f64) {
        let (m, p) = (self.m, self.p);
        if m == 0.0 || p == 0.0 {
            (0.0, 0.0)
        } else if x < p {
            (m / (p * p) * (2.0 * p * x - x * x), 2.0 * m / (p * p) * (p - x))
        } else {
            (
                m / ((1.0 - p) * (1.0 - p)) * (1.0 - 2.0 * p + 2.0 * p * x - x * x),
                2.0 * m / ((1.0 - p) * (1.0 - p)) * (p - x),
            )
        }
    }

    /// The outline of the section as one closed curve of `2n - 1` points: from the trailing edge over the upper
    /// surface to the leading edge, and back along the lower surface. The chordwise stations are spaced by cosines,
    /// so that they crowd towards the leading and trailing edges, where the surface curves most.
    fn outline(self, n: usize) -> (Vec<f64>, Vec<f64>) {
        let stations: Vec<f64> = (0..n)
            .map(|k| (1.0 - (std::f64::consts::PI * k as f64 / (n - 1) as f64).cos()) / 2.0)
            .collect();
        let surface = |x: f64, side: f64| {
            let (yc, slope) = self.camber(x);
            let theta = slope.atan();
            let yt = self.half_thickness(x);
            (x - side * yt * theta.sin(), yc + side * yt * theta.cos())
        };
        let upper = stations.iter().rev().map(|&x| surface(x, 1.0));
        let lower = stations.iter().skip(1).map(|&x| surface(x, -1.0));
        upper.chain(lower).unzip()
    }
}

/// One panel of the sheet: its title and the five members it overlays, the default section first.
struct Panel {
    title: &'static str,
    members: [Section; 5],
}

fn panels() -> [Panel; 3] {
    let with = |change: fn(&mut Section, f64), values: [f64; 4]| {
        let mut members = [DEFAULT; 5];
        for (member, value) in members[1..].iter_mut().zip(values) {
            change(member, value);
        }
        members
    };
    [
        Panel {
            title: "Maximum camber $m$ (position $p = 0.4$, thickness $t = 0.12$)",
            members: with(|s, v| s.m = v, [0.0, 0.02, 0.06, 0.08]),
        },
        Panel {
            title: "Position of maximum camber $p$ ($m = 0.04$, $t = 0.12$)",
            members: with(|s, v| s.p = v, [0.2, 0.3, 0.5, 0.6]),
        },
        Panel {
            title: "Thickness $t$ (maximum camber $m = 0.04$, position $p = 0.4$)",
            members: with(|s, v| s.t = v, [0.06, 0.09, 0.15, 0.18]),
        },
    ]
}

pub fn figure() -> Figure {
    let panels = panels();
    let mut fig = Figure::new()
        .size_mm(110.0, 190.0)
        .tiles(3, 1)
        .title("NACA 4-digit sections")
        .label("line")
        .label("subplots")
        .label("legend")
        .label("linked-axes")
        .label("ticks")
        .label("aspect")
        .parameter("dimensionality", "2D")
        .parameter("artists", 15)
        .parameter("data_points", 2 * 15 * 159)
        .parameter("has_legend", true);

    let mut ids = Vec::new();
    for (row, panel) in panels.iter().enumerate() {
        let mut ax = fig.axes(row as u32, 0);
        for (member, dash) in panel.members.iter().zip(DASHES) {
            let (x, y) = member.outline(80);
            ax.plot(&x, &y)
                .display_name(format!("NACA {}", member.designation()))
                .color(Color::BLACK)
                .dash(dash);
        }
        ax.title(panel.title)
            .title_align(TitleAlign::Left)
            .aspect(Aspect::Equal)
            .aspect_adjust(AspectAdjust::Box)
            .xlim(-0.2, 1.2)
            .ylim(-0.4, 0.4)
            .xtick_step(0.2)
            .ytick_step(0.2)
            .ylabel("$y/c$")
            .legend(LegendLocation::South)
            .legend_columns(LegendEntryColumns::Count(2))
            .legend_order(LegendEntryOrder::RowMajor)
            .legend_boxed(false);
        if row + 1 == panels.len() {
            ax.xlabel("Chordwise position $x/c$");
        }
        ids.push(ax.id());
    }
    fig.link(Dim::X, &ids)
        .expect("every axes belongs to this figure");
    fig.link(Dim::Y, &ids)
        .expect("every axes belongs to this figure");
    fig
}
