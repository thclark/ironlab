//! Data arrays as nodes of the figure, shared between artists through the user API (#26).
//!
//! The API these tests assume:
//!
//! - `Figure::data(&mut self, name: impl Into<String>, values: impl Into<DataSource>) ->
//!   DataArrayMut<'_>` adds a named data array, copied from a slice, vector or matrix, and
//!   returns a borrowing handle, as `Figure::axes` does. The handle offers `name(&mut self,
//!   impl Into<String>) -> &mut Self`, which renames the array, and `id(&self) -> NodeId`,
//!   following the pattern that every top-level node is
//!   created through `fig.<kind>(..)` returning a `<Kind>Mut` handle whose `id()` gives the
//!   `NodeId` that other calls accept.
//! - Every data argument of a plotting call is `impl Into<DataSource>`. A slice, vector or
//!   matrix is copied into a new, unnamed data array, as before; a `NodeId` is stored as
//!   the reference itself, so the artist shares the array and nothing is copied.
//! - Artist handles return the arrays they refer to: `SeriesMut::x_data` and
//!   `SeriesMut::y_data`, each `-> NodeId`.
//! - The gridded coordinates `x` and `y` accept a `NodeId` as well. The facade reads the
//!   rank of the array the identifier names: a vector is a coordinate per column or row and
//!   a matrix a coordinate per node, as for values. An identifier is never copied, so a
//!   vector named by identifier in a curvilinear grid is stored as given and reported by
//!   validation rather than repeated into a new matrix.
//! - A `NodeId` that names a node other than a data array, or no node at all, is stored as
//!   given, without a panic or an error from the plotting call, and reported by
//!   `Figure::validate` (see the test of that case for why).
//! - The setters that give an artist an array (`size_data`, `color_data`,
//!   `face_alpha`, `edge_alpha` and `alpha`) also take `impl Into<DataSource>`. A setter
//!   deletes the array it replaces only when no artist refers to it any more and the
//!   array has no name, which records in the IR that the facade copied it from values:
//!   `fig.data` requires a name, so an array made with `fig.data` is never deleted by a
//!   setter.

use std::path::PathBuf;

use ironlab::ValidationIssue;
use ironlab::ir::{self, Artist, IssueKind};
use ironlab::prelude::*;

/// Returns a path in Cargo's per-crate temporary directory, unique to the test, with no
/// file left there by an earlier run.
fn fresh(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ironlab-facade-tests");
    std::fs::create_dir_all(&dir).expect("create temporary directory");
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    path
}

/// Returns the series with the given identifier.
#[track_caller]
fn series(fig: &Figure, id: NodeId) -> &ir::Series {
    match fig.ir().artist(id) {
        Some((_, Artist::Series(series))) => series,
        other => panic!("expected the series {id}, found {other:?}"),
    }
}

/// Returns the values of the data array with the given identifier, as the artists that
/// refer to it are drawn from them.
#[track_caller]
fn values(fig: &Figure, id: NodeId) -> Vec<f64> {
    fig.ir()
        .data_array(id)
        .unwrap_or_else(|| panic!("{id} is not a data array"))
        .array
        .as_f64()
        .expect("f64 values")
        .to_vec()
}

/// Returns whether the issues include one of the kind at the node.
fn has(issues: &[ValidationIssue], kind: IssueKind, node: NodeId) -> bool {
    issues
        .iter()
        .any(|issue| issue.kind == kind && issue.node == Some(node))
}

/// A record of one time base and two channels: two lines plotted against one shared
/// time array that is named, in two axes. Returns the figure, the time array and the two
/// lines.
fn shared_time_base() -> (Figure, NodeId, NodeId, NodeId) {
    let t = linspace(0.0, 1.0, 5);
    let a: Vec<f64> = t.iter().map(|t| t * t).collect();
    let b: Vec<f64> = t.iter().map(|t| 1.0 - t).collect();
    let mut fig = Figure::new().tiles(2, 1);
    let time = fig.data("time (s)", &t).id();
    let first = fig.axes(0, 0).plot(time, &a).label("A").id();
    let second = fig.axes(1, 0).plot(time, &b).label("B").id();
    (fig, time, first, second)
}

// --- Sharing -------------------------------------------------------------------------

// PROMISE: plotting several channels against one time base stores the time base once,
// which is the point of #26: a figure of a long record is not multiplied in size by its
// number of channels.
#[test]
fn artists_plotted_with_one_node_id_share_one_array() {
    let (fig, time, first, second) = shared_time_base();
    assert_eq!(series(&fig, first).x, time);
    assert_eq!(series(&fig, second).x, time);
    // The time base and one y array per line: three arrays, not four.
    assert_eq!(fig.ir().data.len(), 3);
    let report = fig.validate();
    assert!(report.errors.is_empty(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

// PROMISE: giving values rather than an identifier keeps today's behaviour, so code that
// does not opt into sharing gets independent arrays that one artist's data edit cannot
// change under another.
#[test]
fn plotting_the_same_slice_twice_stores_two_copies() {
    let t = linspace(0.0, 1.0, 5);
    let mut fig = Figure::new();
    let mut ax = fig.axes(0, 0);
    let first = ax.plot(&t, &t).x_data();
    let second = ax.plot(&t, &t).x_data();
    assert_ne!(first, second);
    assert_eq!(fig.ir().data.len(), 4);
}

// PROMISE: passing an identifier adds nothing to the figure's data, while passing values
// in the same call still copies them, so the two forms can be mixed argument by argument.
#[test]
fn a_node_id_argument_adds_no_array_and_a_slice_argument_adds_one() {
    let t = linspace(0.0, 1.0, 5);
    let mut fig = Figure::new();
    let time = fig.data("time", &t).id();
    assert_eq!(fig.ir().data.len(), 1);
    let mut ax = fig.axes(0, 0);
    let handle = ax.plot(time, &t);
    assert_eq!(handle.x_data(), time);
    let y = handle.y_data();
    assert_ne!(y, time);
    assert_eq!(fig.ir().data.len(), 2);
    assert_eq!(values(&fig, y), t);
}

// PROMISE: the identifier a handle returns is the one to share: an array that came from
// a slice in one plotting call can be reused by the next without the author creating it
// with `fig.data` first, and in any role, across kinds of artist.
#[test]
fn the_arrays_of_one_artist_can_be_shared_by_another_in_any_role() {
    let x = linspace(0.0, 1.0, 5);
    let y: Vec<f64> = x.iter().map(|x| x.sin()).collect();
    let mut fig = Figure::new().tiles(1, 2);
    let mut ax = fig.axes(0, 0);
    let handle = ax.plot(&x, &y);
    let (xd, yd) = (handle.x_data(), handle.y_data());
    // The same curve with its axes exchanged, as markers, in another axes.
    let swapped = fig.axes(0, 1).scatter(yd, xd).id();
    assert_eq!(series(&fig, swapped).x, yd);
    assert_eq!(series(&fig, swapped).y, xd);
    assert_eq!(fig.ir().data.len(), 2);
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: a gridded field is shared as a vector is, so a contour drawn over a surface of
// the same field does not store the field twice.
#[test]
fn a_matrix_is_shared_by_a_contour_and_a_surface() {
    let x = linspace(0.0, 1.0, 4);
    let y = linspace(0.0, 1.0, 3);
    let field = Matrix::from_fn(y.len(), x.len(), |row, col| (row * 4 + col) as f64);
    let mut fig = Figure::new().tiles(1, 2);
    let z = fig.data("field", &field).id();
    let contour = fig.axes(0, 0).contour(&x, &y, z).id();
    let surface = fig.axes3(0, 1).surf(&x, &y, z).id();
    match fig.ir().artist(contour) {
        Some((_, Artist::Contour(c))) => assert_eq!(c.z, z),
        other => panic!("expected a contour, found {other:?}"),
    }
    match fig.ir().artist(surface) {
        Some((_, Artist::Surface(s))) => assert_eq!(s.z, z),
        other => panic!("expected a surface, found {other:?}"),
    }
    let fields = fig
        .ir()
        .data
        .iter()
        .filter(|data| data.array.shape == [3, 4])
        .count();
    assert_eq!(fields, 1, "the field is stored once");
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: every data argument accepts an identifier, including the coordinates of a
// grid, so a field sampled on one grid shares the grid as well as the field. The facade
// works out the kind of grid from the arrays the identifiers name: vectors make a
// rectilinear grid and matrices a curvilinear one, as values do.
#[test]
fn grid_coordinates_given_by_identifier_are_shared() {
    let x = linspace(0.0, 1.0, 4);
    let y = linspace(0.0, 1.0, 3);
    let (xm, ym) = meshgrid(&x, &y);
    let field = Matrix::from_fn(y.len(), x.len(), |row, col| (row * 4 + col) as f64);
    let mut fig = Figure::new().tiles(1, 2);
    let (xv, yv) = (fig.data("x", &x).id(), fig.data("y", &y).id());
    let (xn, yn) = (fig.data("x nodes", &xm).id(), fig.data("y nodes", &ym).id());
    let z = fig.data("field", &field).id();
    let rectilinear = fig.axes(0, 0).contour(xv, yv, z).id();
    let curvilinear = fig.axes3(0, 1).surf(xn, yn, z).id();
    match fig.ir().artist(rectilinear) {
        Some((_, Artist::Contour(c))) => {
            assert_eq!(c.grid, ir::Grid::Rectilinear { x: xv, y: yv });
        }
        other => panic!("expected a contour, found {other:?}"),
    }
    match fig.ir().artist(curvilinear) {
        Some((_, Artist::Surface(s))) => {
            assert_eq!(s.grid, ir::Grid::Curvilinear { x: xn, y: yn });
        }
        other => panic!("expected a surface, found {other:?}"),
    }
    assert_eq!(fig.ir().data.len(), 5, "nothing was copied");
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// PROMISE: an empty array is a valid array to create and share (an empty record is not an
// error, as ADR 0012 records), so a program need not special-case a run that produced no
// samples.
#[test]
fn an_empty_array_can_be_created_and_shared() {
    let empty: Vec<f64> = Vec::new();
    let mut fig = Figure::new();
    let none = fig.data("no samples", &empty).id();
    let mut ax = fig.axes(0, 0);
    ax.plot(none, none);
    ax.scatter(none, none);
    assert_eq!(fig.ir().data.len(), 1);
    assert!(values(&fig, none).is_empty());
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// --- Wrapping and naming -----------------------------------------------------------

// PROMISE: a figure wrapped from its IR, as a loaded or hand-built figure is, keeps its
// sharing, and its existing arrays can be shared by new plots without copying them.
#[test]
fn a_wrapped_figure_keeps_its_sharing_and_its_arrays_can_be_reused() {
    let (fig, time, first, _) = shared_time_base();
    let mut wrapped = Figure::from_ir(fig.ir().clone());
    assert_eq!(wrapped, fig);
    let y = series(&wrapped, first).y;
    let added = wrapped.axes(1, 0).plot(time, y).id();
    assert_eq!(series(&wrapped, added).x, time);
    assert_eq!(series(&wrapped, added).y, y);
    assert_eq!(wrapped.ir().data.len(), 3, "nothing was copied");
    assert!(wrapped.validate().is_valid(), "{:?}", wrapped.validate());
}

// PROMISE: the name given to `fig.data`, or the last one given to `name`, is the one the
// inspector shows for the array, and an array the facade copied from values gets none, so
// the inspector labels it by its shape. (That a name survives saving is the IR's promise, tested in `ironlab-ir`.)
#[test]
fn name_sets_the_name_of_the_array_and_copies_from_values_are_unnamed() {
    let (mut fig, time, first, _) = shared_time_base();
    assert_eq!(
        fig.ir().data_array(time).unwrap().name.as_deref(),
        Some("time (s)")
    );
    let copied = series(&fig, first).y;
    assert_eq!(fig.ir().data_array(copied).unwrap().name, None);
    let renamed = fig.data("first", [1.0]).name("second").id();
    assert_eq!(
        fig.ir().data_array(renamed).unwrap().name.as_deref(),
        Some("second"),
        "the last name given wins"
    );
}

// --- Setters that replace an array ---------------------------------------------------

// PROMISE: a setter that replaces an array cleans up after itself, so calling it again with
// new values does not leave the old values in every saved file, but it never deletes an
// array the author made with `fig.data`, which is always named, because the author holds
// its identifier and may plot it again. Such an array is left in place and, while unused,
// reported by the `UnusedData` warning. An identifier given to a setter is shared, as
// one given to a plotting call is.
#[test]
fn a_setter_deletes_its_own_copies_but_never_an_array_made_with_fig_data() {
    let x = [0.0, 1.0, 2.0];
    let mut fig = Figure::new();
    let reused = fig.data("squares", [4.0, 9.0, 16.0]).id();
    let unused = fig.data("unit sizes", [1.0, 1.0, 1.0]).id();
    let mut ax = fig.axes(0, 0);
    let replaced = ax
        .scatter(x, x)
        .size_data(reused)
        .size_data(unused)
        .size_data([2.0, 3.0, 4.0])
        .size_data([5.0, 6.0, 7.0])
        .id();
    // The array replaced first is still there to be reused, by identifier.
    let reuser = ax.scatter(x, x).size_data(reused).id();
    let size = |id| {
        series(&fig, id)
            .size_data
            .expect("the series has size data")
    };
    assert_eq!(size(reuser), reused);
    let last = size(replaced);
    assert_eq!(values(&fig, last), [5.0, 6.0, 7.0]);
    assert_eq!(values(&fig, reused), [4.0, 9.0, 16.0], "kept unchanged");
    assert_eq!(values(&fig, unused), [1.0, 1.0, 1.0], "kept unchanged");
    // Two x and two y copies, the two arrays made with `fig.data` and the last sizes; the
    // first copied sizes were deleted when the second replaced them.
    assert_eq!(fig.ir().data.len(), 7);
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(
        has(&report.warnings, IssueKind::UnusedData, unused),
        "{report:?}"
    );
    assert!(
        !has(&report.warnings, IssueKind::UnusedData, reused),
        "{report:?}"
    );
}

// PROMISE: the rule depends only on the IR, so a figure wrapped from its IR, as a loaded
// figure is, treats its arrays exactly as the session that built it did: an array made
// with `fig.data` before saving is still never deleted, and a setter's own copies still
// are.
#[test]
fn a_wrapped_figure_applies_the_same_rule_to_its_arrays() {
    let x = [0.0, 1.0, 2.0];
    let mut fig = Figure::new();
    let made = fig.data("squares", [4.0, 9.0, 16.0]).id();
    let mut wrapped = Figure::from_ir(fig.into_ir());
    let mut ax = wrapped.axes(0, 0);
    ax.scatter(x, x)
        .size_data(made)
        .size_data([2.0, 3.0, 4.0])
        .size_data([5.0, 6.0, 7.0]);
    assert_eq!(values(&wrapped, made), [4.0, 9.0, 16.0]);
    // x, y, the array made before wrapping and the last sizes.
    assert_eq!(wrapped.ir().data.len(), 4);
}

// --- Invalid and unused references ---------------------------------------------------

// PROMISE: an array created and never plotted is reported, because it costs space in
// every saved file, but it is a warning that never stops a figure being exported: an
// array may be created ahead of the plot that will use it.
#[test]
fn unused_data_is_a_warning_that_does_not_prevent_export() {
    let t = linspace(0.0, 1.0, 5);
    let mut fig = Figure::new();
    let spare = fig.data("spare", &t).id();
    fig.axes(0, 0).plot(&t, &t);
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(
        has(&report.warnings, IssueKind::UnusedData, spare),
        "{report:?}"
    );

    let path = fresh("unused_data.pdf");
    let exported = fig.export_pdf(&path).unwrap();
    assert!(path.exists());
    assert!(
        has(&exported.validation, IssueKind::UnusedData, spare),
        "{exported:?}"
    );
}

// PROMISE: an identifier that is not a data array is caught before anything is drawn,
// and the report names the artist that holds it. The plotting call itself neither panics
// nor returns an error, because the builder reports every inconsistent reference this
// way: `legend_in` accepts any identifier and validation reports one that is not a panel
// holding a legend, and plotting calls do not return `Result`. A reference to the wrong
// kind of node is `WrongNodeKind`, and one to no node at all is `UnknownData`; export
// refuses both.
#[test]
fn plotting_an_identifier_that_is_not_a_data_array_is_reported_by_validation() {
    let t = linspace(0.0, 1.0, 4);
    let mut fig = Figure::new().tiles(1, 2);
    let axes = fig.axes(0, 0).id();
    let wrong = fig.axes(0, 0).plot(axes, &t).id();
    // A gridded call cannot read the shape of a node that is not an array; it must
    // still not panic.
    let wrong_grid = fig.axes(0, 1).contour(&t, [0.0, 1.0, 2.0], axes).id();
    let missing = NodeId(u64::MAX - 1);
    let dangling = fig.axes(0, 1).plot(&t, missing).id();
    let report = fig.validate();
    assert!(
        has(&report.errors, IssueKind::WrongNodeKind, wrong),
        "{report:?}"
    );
    assert!(
        has(&report.errors, IssueKind::WrongNodeKind, wrong_grid),
        "{report:?}"
    );
    assert!(
        has(&report.errors, IssueKind::UnknownData, dangling),
        "{report:?}"
    );

    let path = fresh("wrong_node_kind.pdf");
    assert!(matches!(fig.export_pdf(&path), Err(Error::Invalid(_))));
    assert!(!path.exists());
}
