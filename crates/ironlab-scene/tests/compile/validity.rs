//! The validity guarantees of the display list, checked on a figure that exercises every artist.

use ironlab_ir::{
    ColorSpec, DashStyle, ImagePlane, Legend, Levels, MarkerShape, Scale, ScatterColor, View3d,
};
use ironlab_scene::display::Item;
use ironlab_scene::display::{ItemKind, PathSegment, Point, Rect, Rgba, Transform};

use crate::common::{Fx, compile_figure, linspace, placement, range, text, xy};

/// A 2×3 figure resembling the gallery: every artist type, log axes, legends, LaTeX text, NaN
/// data, dashes, markers, a 3D surface, and the three image kinds, one of them on a wall of the
/// 3D axes.
fn gallery_like() -> ironlab_ir::Figure {
    let mut fx = Fx::new();
    fx.fig.layout.rows = 2;
    fx.fig.layout.cols = 3;
    fx.fig.title = text("Gallery $\\alpha$");

    let lines = fx.axes2d(0, 0);
    let x = linspace(0.0, 6.0, 25);
    let mut y: Vec<f64> = x.iter().map(|v| v.sin()).collect();
    y[7] = f64::NAN;
    fx.line(lines, &x, &y, None, |l| {
        l.display_name = text("$\\sin x$");
        l.line.dash = DashStyle::DashDot;
        l.marker.shape = MarkerShape::Diamond;
    });
    fx.scatter(lines, &x, &x, None, |s, fx| {
        s.display_name = text("Scatter");
        s.color = ScatterColor::Data {
            data: fx.vector(&linspace(-1.0, 1.0, 25)),
        };
        s.marker.face = ColorSpec::Auto;
    });
    let ax = fx.ax(lines);
    ax.legend = Some(Legend::default());
    ax.x.label = text("$x$ (m)");
    ax.y.label = text("$y$");
    ax.x.grid = true;

    let g = linspace(-1.5, 1.5, 15);
    let field = |a: f64, b: f64| (a * a - b * b).sin();
    let filled = fx.axes2d(0, 1);
    fx.contour(filled, &g, &g, field, |c| {
        c.fill = true;
        c.levels = Levels::Auto { count: 8 };
    });
    fx.contour(filled, &g, &g, field, |_| {});
    // A mapped image with a missing value left transparent, an indexed image of bytes, and a
    // true-colour image with an alpha channel and a pixel with a non-finite component.
    let values: Vec<f64> = (0..12)
        .map(|v| if v == 5 { f64::NAN } else { v as f64 })
        .collect();
    fx.mapped_image(
        filled,
        vec![3, 4],
        values,
        xy(range(-1.4, -0.6), range(0.6, 1.4)),
        |_| {},
    );
    fx.indexed_image(
        filled,
        vec![2, 2],
        vec![0u8, 85, 170, 255],
        xy(range(0.6, 1.4), range(-1.4, -0.6)),
        |_| {},
    );
    fx.image(
        filled,
        vec![2, 2, 4],
        vec![
            1.0,
            0.0,
            0.0,
            0.5,
            0.0,
            1.0,
            0.0,
            1.0,
            0.0,
            0.0,
            1.0,
            0.25,
            f64::NAN,
            1.0,
            1.0,
            1.0,
        ],
        xy(range(0.6, 1.4), range(0.6, 1.4)),
        |_| {},
    );

    let arrows = fx.axes2d(0, 2);
    let q = linspace(0.0, 1.0, 4);
    let (mut qx, mut qy) = (Vec::new(), Vec::new());
    for b in &q {
        for a in &q {
            qx.push(*a);
            qy.push(*b);
        }
    }
    let u: Vec<f64> = qy.iter().map(|v| -v).collect();
    fx.quiver(arrows, &qx, &qy, None, &u, &qx, None);

    let log = fx.axes2d(1, 0);
    let lx = [0.01, 0.1, 1.0, 10.0, 100.0];
    fx.line(log, &lx, &lx, None, |_| {});
    fx.ax(log).x.scale = Scale::Log;
    fx.ax(log).y.scale = Scale::Log;

    let surf = fx.axes3d(1, 1, View3d::default());
    fx.surface(surf, &g, &g, field, |_| {});
    fx.mapped_image(
        surf,
        vec![3, 3],
        (0..9).map(|v| v as f64).collect::<Vec<_>>(),
        placement(
            ImagePlane::Xz { y: None },
            range(-1.0, 1.0),
            range(-0.5, 0.5),
        ),
        |_| {},
    );
    fx.ax(surf).z.label = text("$z$");

    let mesh = fx.axes3d(
        1,
        2,
        View3d {
            azimuth_deg: 120.0,
            elevation_deg: -20.0,
            ..View3d::default()
        },
    );
    fx.surface(mesh, &g, &g, field, |s| {
        s.face = ColorSpec::Rgba {
            color: ironlab_ir::Color::WHITE,
        };
        s.edge = ColorSpec::Colormapped;
    });
    fx.scatter(mesh, &q, &q, Some(&q), |_, _| {});
    fx.build()
}

fn finite_point(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn valid_colour(c: Rgba) -> bool {
    [c.r, c.g, c.b, c.a].iter().all(|v| (0.0..=1.0).contains(v))
}

fn finite_transform(t: &Transform) -> bool {
    [t.a, t.b, t.c, t.d, t.e, t.f].iter().all(|v| v.is_finite())
}

fn finite_rect(r: &Rect) -> bool {
    [r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite()) && r.width >= 0.0 && r.height >= 0.0
}

/// Checks every item recursively; `rotated` is whether an enclosing group rotates or skews.
fn check(items: &[Item], rotated: bool, problems: &mut Vec<String>) {
    for item in items {
        let at = format!("item from {:?}", item.source);
        match &item.kind {
            ItemKind::Path(path) => {
                if !matches!(path.segments.first(), Some(PathSegment::MoveTo(_))) {
                    problems.push(format!("{at}: path does not start with MoveTo"));
                }
                for seg in &path.segments {
                    let ok = match *seg {
                        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => finite_point(p),
                        PathSegment::CubicTo(a, b, c) => {
                            finite_point(a) && finite_point(b) && finite_point(c)
                        }
                        PathSegment::Close => true,
                    };
                    if !ok {
                        problems.push(format!("{at}: non-finite segment {seg:?}"));
                    }
                }
                if let Some(fill) = &path.fill
                    && !valid_colour(fill.color)
                {
                    problems.push(format!("{at}: fill colour {:?}", fill.color));
                }
                if let Some(stroke) = &path.stroke {
                    if !valid_colour(stroke.color) {
                        problems.push(format!("{at}: stroke colour {:?}", stroke.color));
                    }
                    if !(stroke.width.is_finite() && stroke.width >= 0.0) {
                        problems.push(format!("{at}: stroke width {}", stroke.width));
                    }
                    if !stroke.dash_offset.is_finite() {
                        problems.push(format!("{at}: dash offset {}", stroke.dash_offset));
                    }
                    let dash_ok = stroke.dash.is_empty()
                        || (stroke.dash.iter().all(|d| d.is_finite() && *d >= 0.0)
                            && stroke.dash.iter().sum::<f64>() > 0.0);
                    if !dash_ok {
                        problems.push(format!("{at}: dash array {:?}", stroke.dash));
                    }
                }
            }
            ItemKind::Glyphs(run) => {
                if !valid_colour(run.color) {
                    problems.push(format!("{at}: glyph colour {:?}", run.color));
                }
                if !(run.size_pt.is_finite() && run.size_pt > 0.0) {
                    problems.push(format!("{at}: glyph size {}", run.size_pt));
                }
                for g in &run.glyphs {
                    if !finite_point(Point::new(g.x, g.y)) {
                        problems.push(format!("{at}: glyph {} at ({}, {})", g.id, g.x, g.y));
                    }
                    let r = &g.text_range;
                    let in_text = r.start <= r.end
                        && r.end <= run.text.len()
                        && run.text.is_char_boundary(r.start)
                        && run.text.is_char_boundary(r.end);
                    if !in_text {
                        problems.push(format!("{at}: text range {r:?} in {:?}", run.text));
                    }
                }
            }
            ItemKind::Group {
                clip,
                transform,
                items,
            } => {
                if let Some(c) = clip {
                    if !finite_rect(c) {
                        problems.push(format!("{at}: clip {c:?}"));
                    }
                    if rotated {
                        problems.push(format!("{at}: clipped group beneath a rotation"));
                    }
                }
                let mut rotates = rotated;
                if let Some(t) = transform {
                    if !finite_transform(t) {
                        problems.push(format!("{at}: transform {t:?}"));
                    }
                    rotates |= t.b != 0.0 || t.c != 0.0;
                }
                check(items, rotates, problems);
            }
            ItemKind::Dense { cells, items } => {
                if *cells == 0 {
                    problems.push(format!("{at}: dense group with no cells"));
                }
                if items.is_empty() {
                    problems.push(format!("{at}: dense group with no items"));
                }
                check(items, rotated, problems);
            }
            ItemKind::Image(image) => {
                if !image.is_valid() {
                    problems.push(format!("{at}: invalid image {image:?}"));
                }
            }
        }
    }
}

// Why: backends trust the compiler's validity guarantees (finite geometry, paths starting with
// MoveTo, sane dashes and colours, text ranges inside their text, no clip beneath a rotation), so a
// figure exercising every artist must satisfy all of them.
#[test]
fn compiled_display_list_satisfies_the_validity_contract() {
    let scene = compile_figure(&gallery_like());
    let dl = &scene.display_list;
    assert!(dl.width_pt.is_finite() && dl.width_pt > 0.0);
    assert!(dl.height_pt.is_finite() && dl.height_pt > 0.0);
    assert!(valid_colour(dl.background));
    assert!(!dl.items.is_empty());
    let mut problems = Vec::new();
    check(&dl.items, false, &mut problems);
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
