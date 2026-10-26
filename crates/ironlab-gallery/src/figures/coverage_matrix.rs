use ironlab::prelude::*;

pub const TITLE: &str = "Case by subroutine coverage matrix";
pub const DESCRIPTION: &str = "The fraction of the branches of each subroutine of a synthetic panel-method code \
     that each test case takes, one column per case and one row per subroutine. The cases are named by their \
     identifiers, set in monospace with `\\texttt` and turned by 90° so that they stand clear of one another; the \
     layout reserves the height they take, so none is clipped. The cells are drawn through a discrete colour scale \
     of eight bands an eighth wide, shown by a horizontal colourbar in a panel under the plot, and they are square, \
     because the axes is drawn at equal aspect. A subroutine that a case never entered has no fraction, so its \
     value is NaN; the scale draws it in a neutral grey, and the colourbar's non-finite key, labelled \"not \
     entered\", keys it. White dots mark the cells in which a case takes a branch that no other case takes, and a \
     legend panel under the colourbar names them.";

/// The test cases, one per column.
const CASES: [&str; 10] = [
    "naca0012_re1e6_alpha_sweep",
    "naca0012_inviscid_m05",
    "naca4412_re3e6_cl_0p8",
    "naca4412_re3e6_xtr_forced",
    "naca63415_re6e6_polar",
    "e387_re2e5_bubble",
    "rae2822_case9_m073",
    "cylinder_inviscid",
    "flat_plate_forced",
    "nonfinite_inputs",
];

/// The subroutines, one per row.
const SUBROUTINES: [&str; 8] = [
    "psilin", "ggcalc", "qdcalc", "setbl", "mrchue", "mrchdu", "blsolv", "trchek",
];

/// Which subroutines are entered by every case: an inviscid case never enters the boundary-layer routines.
fn entered(case: &str, subroutine: &str) -> bool {
    let viscous = ["setbl", "mrchue", "mrchdu", "blsolv", "trchek"].contains(&subroutine);
    let inviscid = case.contains("inviscid");
    !(viscous && inviscid) && !(subroutine == "qdcalc" && case.contains("cylinder"))
}

/// A small deterministic generator, so that the fractions are the same on every run.
struct Lcg(u64);

impl Lcg {
    /// Returns a number drawn from `[0, 1)`.
    fn unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub fn figure() -> Figure {
    let mut rng = Lcg(153);
    let mut unique = (Vec::new(), Vec::new());
    let fraction = Matrix::from_fn(SUBROUTINES.len(), CASES.len(), |row, col| {
        if !entered(CASES[col], SUBROUTINES[row]) {
            return f64::NAN;
        }
        // The non-finite case exercises the guards, which the ordinary cases skip, so it takes a branch that no
        // other case takes in most of the subroutines it enters.
        let guards = CASES[col] == "nonfinite_inputs";
        if (guards && rng.unit() < 0.75) || rng.unit() < 0.08 {
            unique.0.push(col as f64);
            unique.1.push(row as f64);
        }
        let (least, spread) = if guards { (0.1, 0.5) } else { (0.2, 0.8) };
        least + spread * rng.unit()
    });

    let mut fig = Figure::new()
        .size_mm(150.0, 175.0)
        .tiles(3, 1)
        .fit_row(1)
        .fit_row(2)
        .title("Branches taken by each case")
        .tag("cells")
        .tag("scatter")
        .tag("color-scale")
        .tag("colorbar")
        .tag("legend")
        .tag("categorical")
        .tag("ticks")
        .tag("aspect")
        .tag("simulations")
        .tag("occupancy")
        .tag("heatmap")
        .tag("tick-angle")
        .tag("coverage")
        .tag("testing")
        .parameter("dimensionality", "2D")
        .parameter("artists", 2)
        .parameter("data_points", 100)
        .parameter("has_legend", true);
    let edges: Vec<f64> = (0..=8).map(|k| f64::from(k) / 8.0).collect();
    let taken = fig
        .color_scale()
        .colormap(Colormap::Viridis)
        .discrete(&edges)
        .tick_step(0.25)
        .non_finite(Color::rgb(0.85, 0.85, 0.85))
        .key_label(ColorScaleKey::NonFinite, "not entered")
        .title("Fraction of the subroutine's branches taken")
        .id();
    let key = fig.panel(2, 0).legend().boxed(false).id();

    let mut ax = fig.axes(0, 0);
    ax.cells(&fraction)
        .color_scale(taken)
        .edge_color(Color::WHITE)
        .edge_width(1.0);
    ax.scatter(&unique.0, &unique.1)
        .label("takes a branch that no other case takes")
        .marker(Marker::Circle)
        .marker_size(5.0)
        .marker_face(Color::WHITE)
        .marker_edge(Color::BLACK);
    ax.xcategories(CASES.map(|case| format!(r"\texttt{{{case}}}")))
        .xtick_label_angle(90.0)
        .ycategories(SUBROUTINES.map(|name| format!(r"\texttt{{{name}}}")))
        .aspect(Aspect::Equal)
        .legend_in(key);

    fig.panel(1, 0)
        .colorbar(taken)
        .orientation(ColorbarOrientation::Horizontal)
        .title_placement(ColorbarTitlePlacement::Above)
        .length(0.8);
    fig
}
