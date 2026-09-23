//! Markers: the markers items the compiler emits for the markers of lines and scatters, each one outline in unit
//! space shared by instances that carry their own position, size, colours, depth and data index.

use std::sync::Arc;

use ironlab_ir::{
    Color, ColorSpec, Legend, MarkerShape, NodeId, ScatterColor, ScatterSize, View3d,
};
use ironlab_scene::Scene;
use ironlab_scene::display::{MarkerInstance, MarkersItem, PathSegment, Point};
use ironlab_scene::hit::LegendHit;
use ironlab_scene::maths::colormap::{VIRIDIS, sample};
use ironlab_scene::maths::decimate::Sample;

use crate::common::{COLOUR_ORDER, Fx, compile_figure, linspace, rgb8, rgb8_close, rgba, text};
use crate::probe::{Leaf, axis_maps, from_source, inside, leaves, marker_instances, points_close};

/// Returns the markers leaves of a node, in paint order.
fn markers_of(leaves: &[Leaf], id: NodeId) -> Vec<&Leaf> {
    leaves
        .iter()
        .filter(|l| l.source == Some(id) && l.markers().is_some())
        .collect()
}

/// Returns the one markers item of a node, or fails naming `what` when it drew any other number of them.
#[track_caller]
fn the_markers_item<'a>(leaves: &'a [Leaf], id: NodeId, what: &str) -> &'a MarkersItem {
    let items = markers_of(leaves, id);
    assert_eq!(
        items.len(),
        1,
        "{what}: the markers of the artist are one item"
    );
    items[0].markers().expect("a markers leaf")
}

/// Returns the drawn points the hit map records for an artist.
fn samples_of(scene: &Scene, id: NodeId) -> &[Sample] {
    &scene
        .hit_map
        .artists
        .iter()
        .find(|a| a.artist == id)
        .unwrap_or_else(|| panic!("hit map has no drawn points for artist {id}"))
        .samples
}

/// Returns the legend entries of an axes.
fn entries(scene: &Scene, ax: NodeId) -> Vec<LegendHit> {
    scene
        .hit_map
        .legend_entries
        .iter()
        .filter(|e| e.axes == ax)
        .cloned()
        .collect()
}

/// Returns the end points of an outline's segments, dropping any repeat of an earlier point (the return of a
/// closed circle to its start).
fn distinct_endpoints(outline: &[PathSegment]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    for segment in outline {
        let p = match *segment {
            PathSegment::MoveTo(p) | PathSegment::LineTo(p) | PathSegment::CubicTo(_, _, p) => p,
            PathSegment::Close => continue,
        };
        if !out.iter().any(|q| points_close(*q, p, 1e-12)) {
            out.push(p);
        }
    }
    out
}

/// Returns the farthest an outline's end points lie from the origin: the radius of the circle the marker fits in.
fn reach(outline: &[PathSegment]) -> f64 {
    distinct_endpoints(outline)
        .iter()
        .map(|p| p.x.hypot(p.y))
        .fold(0.0, f64::max)
}

/// A 3D axes holding a flat surface and a scatter along its diagonal, half of the points below the surface and
/// half above it, so that the depth sort has to place the markers among the faces; returns the scene, the scatter
/// and the surface.
fn markers_among_faces() -> (Scene, NodeId, NodeId) {
    let mut fx = Fx::new();
    let ax = fx.axes3d(0, 0, View3d::default());
    let g = linspace(0.0, 1.0, 7);
    let surface = fx.surface(ax, &g, &g, |_, _| 0.5, |_| {});
    let t = linspace(0.0, 1.0, 9);
    let z: Vec<f64> = t.iter().map(|v| if *v < 0.5 { 0.1 } else { 0.9 }).collect();
    let scatter = fx.scatter(ax, &t, &t, Some(&z), |_, _| {});
    (compile_figure(&fx.build()), scatter, surface)
}

// WHY: an instance draws the outline scaled by its size, so the drawn marker is `size_pt` wide only when the
// outline is one unit across and centred on the origin; an outline of the wrong reach would draw every marker of
// that shape the wrong size while each instance still looked right, and one off centre would draw every marker
// beside its point. The reach of each shape is the compiler's contract: a circle fills the width, a point is a
// third of it, a square is inset so that it does not look larger than the circle, a diamond and the triangles are
// enlarged so that they do not look smaller, and a plus and a cross span the width. An open outline is never
// closed, so that it is stroked and nothing tries to fill it.
#[test]
fn every_marker_shape_is_an_outline_of_unit_width_centred_on_the_origin() {
    let cases = [
        (MarkerShape::Circle, 0.5, true),
        (MarkerShape::Point, 1.0 / 6.0, true),
        (MarkerShape::Square, 0.45 * std::f64::consts::SQRT_2, true),
        (MarkerShape::Diamond, 1.15 * 0.5, true),
        (MarkerShape::TriangleUp, 1.15 * 0.5, true),
        (MarkerShape::TriangleDown, 1.15 * 0.5, true),
        (MarkerShape::Plus, 0.5, false),
        (MarkerShape::Cross, 0.5, false),
    ];
    for (shape, expected_reach, closed) in cases {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let scatter = fx.scatter(ax, &[0.0], &[0.0], None, |s, _| {
            s.marker.shape = shape;
            s.marker.face = ColorSpec::Auto;
        });
        let scene = compile_figure(&fx.build());
        let leaves = leaves(&scene);
        let markers = the_markers_item(&leaves, scatter, &format!("{shape:?}"));
        assert!(
            matches!(markers.outline.first(), Some(PathSegment::MoveTo(_))),
            "{shape:?}: the outline starts with MoveTo"
        );
        let found = reach(&markers.outline);
        assert!(
            (found - expected_reach).abs() <= 1e-12,
            "{shape:?}: the outline reaches {expected_reach} from the origin, not {found}"
        );
        let points = distinct_endpoints(&markers.outline);
        let n = points.len() as f64;
        let mean = Point::new(
            points.iter().map(|p| p.x).sum::<f64>() / n,
            points.iter().map(|p| p.y).sum::<f64>() / n,
        );
        assert!(
            points_close(mean, Point::new(0.0, 0.0), 1e-12),
            "{shape:?}: the outline is centred on the origin, not on {mean:?}"
        );
        assert_eq!(
            markers
                .outline
                .iter()
                .any(|s| matches!(s, PathSegment::Close)),
            closed,
            "{shape:?}: {}",
            if closed {
                "a closed outline"
            } else {
                "an open outline"
            }
        );
    }
}

// WHY: the markers of a 2D artist are one item so that a backend tessellates the outline once and draws every
// marker of the artist in one instanced call; each instance must sit at its own point, name the index of that
// point for picking and take the artist's marker size, and a point that cannot be placed must have no instance,
// or the run would show a marker where there is no data. The polyline is painted before the markers, so that the
// markers lie over the line rather than under it.
#[test]
fn a_line_with_markers_draws_one_item_with_one_instance_per_finite_point() {
    let x = [0.0, 1.0, 2.0, 3.0, 4.0];
    let y = [0.0, 1.0, f64::NAN, 3.0, 4.0];
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let line = fx.line(ax, &x, &y, None, |l| {
        l.marker.shape = MarkerShape::Square;
        l.marker.size_pt = 6.0;
    });
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let (xmap, ymap) = axis_maps(&scene, ax);

    the_markers_item(&leaves, line, "a line with markers");
    let of_line = from_source(&leaves, line);
    let paths: Vec<usize> = (0..of_line.len())
        .filter(|k| of_line[*k].path().is_some())
        .collect();
    assert_eq!(
        paths.len(),
        1,
        "the polyline is a path item of its own beside the markers"
    );
    let markers_at = of_line
        .iter()
        .position(|l| l.markers().is_some())
        .expect("the markers leaf of the line");
    assert!(
        paths[0] < markers_at,
        "the polyline (leaf {}) is painted before the markers (leaf {markers_at}) of the line",
        paths[0]
    );
    let instances = marker_instances(&leaves, line);
    let expected = [(0, 0.0, 0.0), (1, 1.0, 1.0), (3, 3.0, 3.0), (4, 4.0, 4.0)];
    assert_eq!(
        instances.len(),
        expected.len(),
        "one instance per finite point: {instances:?}"
    );
    for (index, px, py) in expected {
        let of_point: Vec<&MarkerInstance> = instances
            .iter()
            .filter(|m| m.source_index == index)
            .collect();
        assert_eq!(of_point.len(), 1, "one instance names point {index}");
        let (dx, dy) = (
            xmap.to_data(of_point[0].position.x),
            ymap.to_data(of_point[0].position.y),
        );
        assert!(
            (dx - px).abs() < 1e-6 && (dy - py).abs() < 1e-6,
            "the instance of point {index} sits at ({px}, {py}), not ({dx}, {dy})"
        );
        assert_eq!(
            of_point[0].size_pt, 6.0,
            "the instance of point {index} takes the line's marker size"
        );
    }
}

// WHY: a 2D axes merges the markers of consecutive items into one item per artist, and the merge must stop at the
// boundary between artists: two scatters drawn one after the other would otherwise become one item attributed to
// the first, so that hiding or picking the second would act on the first's markers. `coalesce_markers` keys the
// merge on the source, the outline and the edge width, and every artist builds an outline of its own, so the items
// of two scatters of one shape stay apart. The edge width has no case of its own here: the compiler resolves one
// edge width per artist, so two widths within one artist cannot arise.
#[test]
fn two_scatters_on_one_axes_are_two_items_each_holding_the_instances_of_its_own_points() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let first = fx.scatter(ax, &[0.0, 1.0, 2.0], &[0.0, 0.0, 0.0], None, |_, _| {});
    let second = fx.scatter(ax, &[0.0, 1.0], &[1.0, 1.0], None, |_, _| {});
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let (xmap, ymap) = axis_maps(&scene, ax);
    let at = |id: NodeId| {
        leaves
            .iter()
            .position(|l| l.source == Some(id) && l.markers().is_some())
            .unwrap_or_else(|| panic!("{id} draws a markers item"))
    };
    assert_eq!(
        at(second),
        at(first) + 1,
        "the two items follow each other in the paint order, so the merge met them together and kept them apart"
    );
    for (id, what, count, y) in [
        (first, "the first scatter", 3, 0.0),
        (second, "the second scatter", 2, 1.0),
    ] {
        let item = the_markers_item(&leaves, id, what);
        assert_eq!(
            item.instances.len(),
            count,
            "{what}: one instance per point of its own"
        );
        for m in marker_instances(&leaves, id) {
            let (dx, dy) = (xmap.to_data(m.position.x), ymap.to_data(m.position.y));
            let (px, py) = (m.source_index as f64, y);
            assert!(
                (dx - px).abs() < 1e-6 && (dy - py).abs() < 1e-6,
                "{what}: the instance of point {} sits at its own point ({px}, {py}), not ({dx}, {dy})",
                m.source_index
            );
        }
    }
}

/// Where the face or the edge of a marker takes its colour from, in the scatter test below.
#[derive(Clone, Copy)]
enum Colour {
    /// The colour of the point through the colormap.
    OfPoint,
    /// The fixed colour of the spec.
    Fixed,
    /// No colour: that part is not drawn.
    Absent,
}

// WHY: a scatter encodes further variables in the size and colour of each marker, so every instance must carry the
// size and colour of its own point rather than the artist's, and the face and edge specs decide where the point's
// colour goes: `Auto` and `Colormapped` take it (a scatter's automatic colour is its colour scale), `Rgba` keeps
// its fixed colour whatever the point, and `None` leaves that part undrawn; a scatter that gave every marker the
// first point's colour, or painted a fixed edge in the data colour, would misreport the data while every marker
// still looked plausible.
#[test]
fn a_scatter_gives_each_instance_the_size_of_its_point_and_the_colours_its_specs_ask_for() {
    let sizes = [3.0, 5.0, 8.0];
    let values = [10.0, 15.0, 20.0];
    let fixed = Color::rgb(1.0, 0.0, 0.0);
    let cases = [
        (
            "an auto face and edge",
            ColorSpec::Auto,
            ColorSpec::Auto,
            Colour::OfPoint,
            Colour::OfPoint,
        ),
        (
            "a colormapped face and edge",
            ColorSpec::Colormapped,
            ColorSpec::Colormapped,
            Colour::OfPoint,
            Colour::OfPoint,
        ),
        (
            "a fixed face and an auto edge",
            ColorSpec::Rgba { color: fixed },
            ColorSpec::Auto,
            Colour::Fixed,
            Colour::OfPoint,
        ),
        (
            "no face and a fixed edge",
            ColorSpec::None,
            ColorSpec::Rgba { color: fixed },
            Colour::Absent,
            Colour::Fixed,
        ),
        (
            "an auto face and no edge",
            ColorSpec::Auto,
            ColorSpec::None,
            Colour::OfPoint,
            Colour::Absent,
        ),
    ];
    for (what, face, edge, expected_face, expected_edge) in cases {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let scatter = fx.scatter(ax, &[0.0, 1.0, 2.0], &[0.0, 0.0, 0.0], None, |s, fx| {
            s.size = ScatterSize::Data {
                data: fx.vector(&sizes),
            };
            s.color = ScatterColor::Data {
                data: fx.vector(&values),
            };
            s.marker.face = face;
            s.marker.edge = edge;
        });
        let scene = compile_figure(&fx.build());
        let instances = marker_instances(&leaves(&scene), scatter);
        assert_eq!(instances.len(), 3, "{what}: one instance per point");
        for m in &instances {
            let i = m.source_index;
            assert_eq!(
                m.size_pt, sizes[i],
                "{what}: the instance of point {i} takes the point's size"
            );
            let of_point = sample(&VIRIDIS, i as f64 / 2.0).expect("a value inside the limits");
            for (part, colour, expected) in [
                ("face", m.face, expected_face),
                ("edge", m.edge, expected_edge),
            ] {
                match expected {
                    Colour::OfPoint => {
                        let c = colour.unwrap_or_else(|| panic!("{what}: point {i} has a {part}"));
                        assert!(
                            rgb8_close(rgb8(c), of_point),
                            "{what}: the {part} of point {i} is the point's colour {of_point:?}, not {c:?}"
                        );
                    }
                    Colour::Fixed => assert_eq!(
                        colour,
                        Some(rgba(fixed)),
                        "{what}: the {part} of point {i} is the fixed colour"
                    ),
                    Colour::Absent => {
                        assert_eq!(colour, None, "{what}: point {i} has no {part}");
                    }
                }
            }
        }
    }
}

// WHY: a point marker is a solid dot a third of the marker width across, the style that reads as plain data; it is
// filled with the edge colour because the edge is where a line's markers take the line's colour by default, so a
// line with point markers gets dots in its own colour, and it has no edge, or the stroke would swell the dot by
// the edge width whatever its size. A dot given only a face colour still uses it, and one given no colour at all
// is nothing to draw.
#[test]
fn a_point_marker_is_a_dot_filled_with_the_edge_colour_and_without_an_edge() {
    let red = Color::rgb(1.0, 0.0, 0.0);
    let blue = Color::rgb(0.0, 0.0, 1.0);
    let cases = [
        (
            "a face and an edge",
            ColorSpec::Rgba { color: red },
            ColorSpec::Rgba { color: blue },
            Some(blue),
        ),
        (
            "a face alone",
            ColorSpec::Rgba { color: red },
            ColorSpec::None,
            Some(red),
        ),
        (
            "an edge alone",
            ColorSpec::None,
            ColorSpec::Rgba { color: blue },
            Some(blue),
        ),
        ("neither", ColorSpec::None, ColorSpec::None, None),
    ];
    for (what, face, edge, fill) in cases {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let scatter = fx.scatter(ax, &[0.0, 1.0], &[0.0, 1.0], None, |s, _| {
            s.marker.shape = MarkerShape::Point;
            s.marker.face = face;
            s.marker.edge = edge;
        });
        let scene = compile_figure(&fx.build());
        let leaves = leaves(&scene);
        let Some(fill) = fill else {
            assert!(
                from_source(&leaves, scatter).is_empty(),
                "{what}: a dot with no colour draws nothing"
            );
            continue;
        };
        let markers = the_markers_item(&leaves, scatter, what);
        let radius = reach(&markers.outline);
        assert!(
            (radius - 1.0 / 6.0).abs() <= 1e-12,
            "{what}: the dot's outline has a radius of a sixth, not {radius}"
        );
        assert_eq!(markers.instances.len(), 2, "{what}: one dot per point");
        for m in &markers.instances {
            assert_eq!(
                m.face,
                Some(rgba(fill)),
                "{what}: dot {} is filled with {fill:?}",
                m.source_index
            );
            assert_eq!(m.edge, None, "{what}: dot {} has no edge", m.source_index);
        }
    }
}

// WHY: a plus and a cross are open strokes with nothing to fill, so a face colour given to them must go to the
// stroke rather than be dropped, or a scatter whose colour reaches its markers through `face` alone would vanish
// when its shape was changed to a cross; the edge colour still wins when both are given, since it is the colour
// of the outline. No face is carried, so that no backend fills the open outline as a sliver, and a marker given
// no colour at all is nothing to draw.
#[test]
fn plus_and_cross_markers_have_no_face_and_an_edge_of_the_edge_or_else_the_face_colour() {
    let red = Color::rgb(1.0, 0.0, 0.0);
    let blue = Color::rgb(0.0, 0.0, 1.0);
    let colours = [
        (
            "a face and an edge",
            ColorSpec::Rgba { color: red },
            ColorSpec::Rgba { color: blue },
            Some(blue),
        ),
        (
            "a face alone",
            ColorSpec::Rgba { color: red },
            ColorSpec::None,
            Some(red),
        ),
        (
            "an edge alone",
            ColorSpec::None,
            ColorSpec::Rgba { color: blue },
            Some(blue),
        ),
        ("neither", ColorSpec::None, ColorSpec::None, None),
    ];
    for shape in [MarkerShape::Plus, MarkerShape::Cross] {
        for (what, face, edge, stroke) in colours {
            let mut fx = Fx::new();
            let ax = fx.axes2d(0, 0);
            let scatter = fx.scatter(ax, &[0.0, 1.0], &[0.0, 1.0], None, |s, _| {
                s.marker.shape = shape;
                s.marker.face = face;
                s.marker.edge = edge;
            });
            let scene = compile_figure(&fx.build());
            let leaves = leaves(&scene);
            let Some(stroke) = stroke else {
                assert!(
                    from_source(&leaves, scatter).is_empty(),
                    "{shape:?} with {what}: nothing to draw"
                );
                continue;
            };
            let markers = the_markers_item(&leaves, scatter, &format!("{shape:?} with {what}"));
            assert_eq!(
                markers.instances.len(),
                2,
                "{shape:?} with {what}: one marker per point"
            );
            for m in &markers.instances {
                assert_eq!(
                    m.face, None,
                    "{shape:?} with {what}: marker {} has no face",
                    m.source_index
                );
                assert_eq!(
                    m.edge,
                    Some(rgba(stroke)),
                    "{shape:?} with {what}: marker {} is stroked with {stroke:?}",
                    m.source_index
                );
            }
        }
    }
}

// WHY: in a 3D axes each marker is sorted among the faces around it, so an artist's markers come out as the runs
// the sort leaves, back to front with faces between them; a backend without a depth buffer paints the runs where
// they fall, and one with a depth buffer draws each instance at the instance's own depth, which must be the depth
// the hit map records for the point so that picking and drawing agree. Markers kept in one item would all be
// painted before or after the surface, and a run whose instances shared one depth would sink into a face that only
// some of them lie behind.
#[test]
fn in_three_dimensions_the_markers_come_as_runs_in_painters_order_each_at_the_depth_of_its_point() {
    let (scene, scatter, surface) = markers_among_faces();
    let leaves = leaves(&scene);
    let runs: Vec<usize> = (0..leaves.len())
        .filter(|k| leaves[*k].source == Some(scatter))
        .collect();
    assert!(
        runs.len() > 1,
        "the depth sort splits the markers into several runs, not {}",
        runs.len()
    );
    assert!(
        runs.iter().all(|k| leaves[*k].markers().is_some()),
        "every leaf of the scatter is a markers item"
    );
    for pair in runs.windows(2) {
        assert!(
            leaves[pair[0] + 1..pair[1]]
                .iter()
                .any(|l| l.source == Some(surface)),
            "faces of the surface lie between the runs at {} and {}",
            pair[0],
            pair[1]
        );
    }

    let instances: Vec<MarkerInstance> = runs.iter().flat_map(|k| leaves[*k].instances()).collect();
    let samples = samples_of(&scene, scatter);
    let mut drawn: Vec<usize> = instances.iter().map(|m| m.source_index).collect();
    drawn.sort_unstable();
    assert_eq!(
        drawn,
        samples.iter().map(|s| s.source_index).collect::<Vec<_>>(),
        "the runs between them hold every drawn point once"
    );
    assert_eq!(
        drawn,
        (0..9).collect::<Vec<_>>(),
        "every point of the scatter is drawn"
    );
    let depths: Vec<f64> = instances.iter().map(|m| m.depth).collect();
    assert!(
        depths.windows(2).all(|w| w[0] <= w[1]),
        "the instances are painted back to front: {depths:?}"
    );
    for m in &instances {
        let s = samples
            .iter()
            .find(|s| s.source_index == m.source_index)
            .expect("the hit map records every drawn point");
        assert_eq!(
            m.depth, s.depth,
            "the instance of point {} lies at the depth of its sample",
            m.source_index
        );
        assert!(
            points_close(m.position, s.position, 1e-9),
            "the instance of point {} sits at its sample's position",
            m.source_index
        );
    }
}

// WHY: the point of one outline per artist is that a backend tessellates it once; the depth sort of a 3D axes
// breaks an artist's markers into several runs, and a run that rebuilt its outline would carry an equal but
// separate copy, which a backend keyed on the outline's identity would tessellate afresh for every run.
#[test]
fn the_runs_of_one_artist_share_one_outline() {
    let (scene, scatter, _) = markers_among_faces();
    let leaves = leaves(&scene);
    let items = markers_of(&leaves, scatter);
    assert!(
        items.len() > 1,
        "the scatter is drawn as several runs, not {}",
        items.len()
    );
    let first = &items[0].markers().expect("a markers leaf").outline;
    for (k, item) in items.iter().enumerate().skip(1) {
        let outline = &item.markers().expect("a markers leaf").outline;
        assert!(
            Arc::ptr_eq(first, outline),
            "run {k} shares the outline of run 0 rather than carrying a copy"
        );
    }
}

// WHY: a legend entry explains a series by showing its marker, so the sample of a marked line and of a scatter is
// one marker in the sample column, in the artist's colour, drawn as the legend's own item (named for the axes, like
// the rest of the legend) so that hiding the artist keeps its entry; a sample of several markers, or of none, would
// misdescribe the series, and one attributed to the artist would vanish with it.
#[test]
fn the_legend_sample_of_a_marked_line_and_of_a_scatter_is_one_instance_in_the_artist_colour() {
    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let line = fx.line(ax, &[0.0, 1.0], &[0.0, 0.2], None, |l| {
        l.display_name = text("Line");
        l.marker.shape = MarkerShape::Diamond;
    });
    let scatter = fx.scatter(ax, &[0.2, 0.8], &[0.5, 0.5], None, |s, _| {
        s.display_name = text("Scatter");
    });
    fx.ax(ax).legend = Some(Legend::default());
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let entries = entries(&scene, ax);
    assert_eq!(
        entries.iter().map(|e| e.artist).collect::<Vec<_>>(),
        vec![line, scatter],
        "one entry per named artist"
    );
    for (entry, colour) in entries.iter().zip(COLOUR_ORDER) {
        let in_entry: Vec<&Leaf> = markers_of(&leaves, ax)
            .into_iter()
            .filter(|l| l.bbox().is_some_and(|b| inside(b, entry.rect, 0.5)))
            .collect();
        assert_eq!(
            in_entry.len(),
            1,
            "the entry of {} holds one markers item of the legend",
            entry.artist
        );
        let instances = in_entry[0].instances();
        assert_eq!(
            instances.len(),
            1,
            "the sample of {} is one marker",
            entry.artist
        );
        let edge = instances[0]
            .edge
            .unwrap_or_else(|| panic!("the sample of {} has an edge", entry.artist));
        assert!(
            rgb8_close(rgb8(edge), colour),
            "the sample of {} takes the artist's colour {colour:?}, not {edge:?}",
            entry.artist
        );
    }
}

// WHY: an artist that draws nothing must leave no markers item behind: a hidden scatter would otherwise still be
// painted, and a scatter whose every point is NaN has no instance to hold, so an item for it would only make a
// backend set up a draw for nothing.
#[test]
fn a_hidden_scatter_and_one_without_a_placeable_point_draw_no_markers_item() {
    let hidden = {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let id = fx.scatter(ax, &[0.0, 1.0], &[0.0, 1.0], None, |s, _| {
            s.visible = false;
        });
        (compile_figure(&fx.build()), id)
    };
    let unplaceable = {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let nan = [f64::NAN; 3];
        let id = fx.scatter(ax, &nan, &nan, None, |_, _| {});
        (compile_figure(&fx.build()), id)
    };
    for ((scene, id), what) in [
        (hidden, "a hidden scatter"),
        (unplaceable, "a scatter of NaN points"),
    ] {
        assert!(
            from_source(&leaves(&scene), id).is_empty(),
            "{what} draws no item"
        );
    }
}

// WHY: a marker's edge is stroked at a width of the item's own, not scaled with the marker, and a line's markers
// take the line's width so that a bold line gets bold markers, but only between half a point and one and a half:
// a hairline would give its markers no visible edge and a thick line would swallow small markers whole. A scatter
// has no line to follow and takes the thin edge that lets its face colours show.
#[test]
fn a_line_marker_edge_follows_the_line_width_within_bounds_and_a_scatter_edge_is_half_a_point() {
    let cases = [
        ("thinner than the floor", 0.2, 0.5),
        ("within the bounds", 1.0, 1.0),
        ("thicker than the ceiling", 3.0, 1.5),
        ("not a width", f64::NAN, 0.75),
    ];
    for (what, width_pt, expected) in cases {
        let mut fx = Fx::new();
        let ax = fx.axes2d(0, 0);
        let line = fx.line(ax, &[0.0, 1.0], &[0.0, 1.0], None, |l| {
            l.line.width_pt = width_pt;
            l.marker.shape = MarkerShape::Circle;
        });
        let scene = compile_figure(&fx.build());
        let leaves = leaves(&scene);
        let markers = the_markers_item(&leaves, line, &format!("a line of width {what}"));
        assert_eq!(
            markers.edge_width, expected,
            "a line of width {what} ({width_pt}) strokes its markers at {expected}"
        );
    }

    let mut fx = Fx::new();
    let ax = fx.axes2d(0, 0);
    let scatter = fx.scatter(ax, &[0.0, 1.0], &[0.0, 1.0], None, |_, _| {});
    let scene = compile_figure(&fx.build());
    let leaves = leaves(&scene);
    let markers = the_markers_item(&leaves, scatter, "a scatter");
    assert_eq!(
        markers.edge_width, 0.5,
        "a scatter strokes its markers at half a point"
    );
}
