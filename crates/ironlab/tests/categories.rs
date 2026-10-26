//! Categorical axes in the facade (#74): `xcategories`, `ycategories` and `ycategory_groups`, and the `CategoryRows`
//! value that places data in the rows `ycategory_groups` lays out.
//!
//! These tests assert the figure description the facade stores, and, where the promise is about the page (which row
//! is at the top), the axis map of the compiled figure. They assume this API, exported in the prelude:
//!
//! - `AxesMut::xcategories(names)` and `AxesMut::ycategories(names)`, each taking anything that iterates over values
//!   convertible into `Text`, place a tick at 0, 1, …, N − 1 labelled with the names in order; `ycategories` also
//!   reverses the y axis, so that the first category is at the top, as a table is read. Neither changes the limits.
//! - `AxesMut::ycategory_groups(groups) -> CategoryRows`, taking pairs of a group name and its member names (each
//!   `AsRef<str>`, the LaTeX source of the label), lays the rows out from row 0 as, for each group, a header row whose
//!   tick label is the group name in bold (`\textbf{…}`, with the LaTeX interpreter), then one row per member labelled
//!   with its name, with one blank row, which has no tick, between consecutive groups; it reverses the y axis.
//!   A header row keeps its tick mark, because an axis cannot draw a tick label without its mark.
//! - `CategoryRows::len()` (and `is_empty()`): the number of rows laid out, blank rows and header rows included;
//!   `CategoryRows::row(group, member) -> Option<usize>`: the row of a member of a group, which is its y coordinate
//!   on unit rows; `CategoryRows::header(group) -> Option<usize>`: the row of a group's header; and
//!   `CategoryRows::place(member_rows) -> Vec<Vec<f64>>`, which takes one row of data per member, in the order the
//!   members were given, group by group, and returns every row of the layout, an empty row for each header and blank
//!   row, ready for `occupancy`. It panics when it is given a number of rows other than the number of members.

mod common;

use common::{axes, data};
use ironlab::ir::{Artist, Interpreter};
use ironlab::prelude::*;
use ironlab::{TickLabels, Ticks};
use ironlab_scene::hit::AxesHitKind;
use ironlab_text::TextEngine;

const STATUS: [(Color, &str); 5] = [
    (Color::rgb(0.0, 0.6, 0.3), "taken by the finite set"),
    (Color::rgb(0.3, 0.7, 0.9), "taken by the non-finite set"),
    (
        Color::rgb(0.9, 0.6, 0.0),
        "taken only in pathological cases",
    ),
    (Color::rgb(0.6, 0.6, 0.6), "not reachable"),
    (Color::rgb(0.8, 0.2, 0.1), "not taken"),
];

/// The tick values and the source text of the tick labels of an axis.
fn ticks_of(axis: &ironlab::ir::Axis) -> (Vec<f64>, Vec<Text>) {
    let Ticks::Values(values) = &axis.ticks else {
        panic!("the axis has explicit ticks, not {:?}", axis.ticks);
    };
    let TickLabels::Text(labels) = &axis.tick_labels else {
        panic!("the axis is labelled with text, not {:?}", axis.tick_labels);
    };
    (values.clone(), labels.clone())
}

/// Returns the source text of the tick label at `position` of an axis, if a tick falls exactly there.
fn label_at(axis: &ironlab::ir::Axis, position: f64) -> Option<String> {
    let (values, labels) = ticks_of(axis);
    let at = values.iter().position(|v| *v == position)?;
    Some(labels[at].content.clone())
}

/// The groups of the branch map of yFoil, abridged: three source files and their subroutines.
fn source_files() -> [(&'static str, Vec<&'static str>); 3] {
    [
        ("xpanel.f", vec!["psilin", "ggcalc"]),
        ("xbl.f", vec!["blsolv"]),
        ("xsolve.f", vec!["gauss", "baksub", "ludcmp"]),
    ]
}

// WHY: a categorical axis places its categories at 0, 1, …, N − 1 and names them, which is the recipe for bars and
// cells by category; `xcategories` writes that recipe in one call, keeping the order of the names and interpreting
// them as LaTeX, like every other text of the figure, so that `\texttt{…}` names are set in monospace. Along x the
// first category is at the left, as it is read, so the axis is not reversed.
#[test]
fn xcategories_names_ticks_at_whole_numbers_in_order() {
    let mut fig = Figure::new();
    let id = fig
        .axes(0, 0)
        .xcategories([r"\texttt{case_a}", r"\texttt{case_b}", r"\texttt{case_c}"])
        .id();
    let x = &axes(&fig, id).x;
    let (values, labels) = ticks_of(x);
    assert_eq!(values, vec![0.0, 1.0, 2.0]);
    assert_eq!(
        labels,
        vec![
            Text::new(r"\texttt{case_a}"),
            Text::new(r"\texttt{case_b}"),
            Text::new(r"\texttt{case_c}")
        ]
    );
    assert!(!x.reverse, "the first category is at the left");
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: a list of categories down the page is read from the top, as a table is, so the first category must be drawn
// at the top; on an ordinary y axis 0 is at the bottom, which would put the first subroutine of a branch map last.
// The promise is about the page, so it is checked on the compiled figure: row 0 is drawn above row 2.
#[test]
fn ycategories_puts_the_first_category_at_the_top() {
    let mut fig = Figure::new();
    let id = {
        let mut ax = fig.axes(0, 0);
        ax.occupancy([vec![0.0, 1.0], vec![1.0], vec![0.0]]);
        ax.ycategories(["psilin", "ggcalc", "blsolv"]).id()
    };
    let y = &axes(&fig, id).y;
    let (values, labels) = ticks_of(y);
    assert_eq!(values, vec![0.0, 1.0, 2.0]);
    assert_eq!(labels[0], Text::new("psilin"));
    assert!(y.reverse, "the y axis is reversed");
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());

    let scene = ironlab_scene::compile(fig.ir(), &TextEngine::new());
    let AxesHitKind::TwoD { y: ymap, .. } = scene.hit_map.axes[0].kind else {
        panic!("the axes is two-dimensional");
    };
    assert!(
        ymap.to_figure(0.0) < ymap.to_figure(2.0),
        "the first category is drawn above the last: {ymap:?}"
    );
}

// WHY: the branch map of #74 groups its subroutines under a header per source file, so the layout must be exactly:
// for each group a header row named by the group in bold, then its members in the order given, with one blank row
// between groups and none before the first or after the last, read from the top. A blank row has no tick, so that
// nothing is written beside it; a header keeps its tick mark, because an axis cannot write a tick label without one.
// The rows it returns are those of this layout, so that data can be placed beside the right names.
#[test]
fn ycategory_groups_lays_out_bold_headers_members_and_blank_rows_between_groups() {
    let mut fig = Figure::new();
    let (id, rows) = {
        let mut ax = fig.axes(0, 0);
        let rows = ax.ycategory_groups(source_files());
        (ax.id(), rows)
    };
    // 0 xpanel.f, 1 psilin, 2 ggcalc, 3 blank, 4 xbl.f, 5 blsolv, 6 blank, 7 xsolve.f, 8 gauss, 9 baksub, 10 ludcmp.
    assert_eq!(rows.len(), 11);
    assert!(!rows.is_empty());
    for (group, header) in [("xpanel.f", 0), ("xbl.f", 4), ("xsolve.f", 7)] {
        assert_eq!(rows.header(group), Some(header), "the header of {group}");
    }
    for (group, member, row) in [
        ("xpanel.f", "psilin", 1),
        ("xpanel.f", "ggcalc", 2),
        ("xbl.f", "blsolv", 5),
        ("xsolve.f", "gauss", 8),
        ("xsolve.f", "baksub", 9),
        ("xsolve.f", "ludcmp", 10),
    ] {
        assert_eq!(rows.row(group, member), Some(row), "{member} of {group}");
    }
    assert_eq!(
        rows.row("xbl.f", "psilin"),
        None,
        "a member is found in its own group only"
    );
    assert_eq!(rows.row("xpanel.f", "missing"), None);
    assert_eq!(rows.header("missing.f"), None);

    let y = &axes(&fig, id).y;
    let (values, labels) = ticks_of(y);
    assert_eq!(
        values,
        vec![0.0, 1.0, 2.0, 4.0, 5.0, 7.0, 8.0, 9.0, 10.0],
        "no tick on a blank row"
    );
    let sources: Vec<&str> = labels.iter().map(|t| t.content.as_str()).collect();
    assert_eq!(
        sources,
        vec![
            r"\textbf{xpanel.f}",
            "psilin",
            "ggcalc",
            r"\textbf{xbl.f}",
            "blsolv",
            r"\textbf{xsolve.f}",
            "gauss",
            "baksub",
            "ludcmp"
        ]
    );
    assert!(
        labels.iter().all(|t| t.interpreter == Interpreter::Latex),
        "the bold headers are LaTeX"
    );
    assert!(y.reverse, "the first group is at the top");
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: one group has no neighbour to be set apart from, so it has no blank row: a header and its members only.
#[test]
fn a_single_group_has_no_blank_row() {
    let mut fig = Figure::new();
    let rows = fig
        .axes(0, 0)
        .ycategory_groups([("xbl.f", vec!["blsolv", "blvar"])]);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows.row("xbl.f", "blvar"), Some(2));
}

// WHY: the rows of an occupancy matrix must match the rows of the layout one for one, or every subroutine after the
// first header would be drawn beside another's name. `place` takes the data of each member in the order given and
// puts it in that member's row, leaving header and blank rows empty, which `occupancy` pads with NaN and so draws as
// nothing; the cells of the row that `row` returns for a member then hold that member's data, and the tick label at
// that row names that member.
#[test]
fn placed_data_lies_in_the_rows_that_ycategory_groups_returns_and_names() {
    let member_data = vec![
        vec![0.0, 1.0],      // psilin
        vec![4.0],           // ggcalc
        vec![0.0, 0.0, 3.0], // blsolv
        vec![1.0],           // gauss
        vec![2.0, 2.0],      // baksub
        vec![0.0],           // ludcmp
    ];
    let mut fig = Figure::new();
    let status = fig.categorical_scale(STATUS).id();
    let (ax_id, cells_id, rows) = {
        let mut ax = fig.axes(0, 0);
        let rows = ax.ycategory_groups(source_files());
        let placed = rows.place(&member_data);
        assert_eq!(
            placed.len(),
            rows.len(),
            "every row of the layout is returned"
        );
        for empty in [0, 3, 4, 6, 7] {
            assert!(
                placed[empty].is_empty(),
                "row {empty} is a header or blank row"
            );
        }
        let cells_id = ax.occupancy(&placed).color_scale(status).id();
        (ax.id(), cells_id, rows)
    };
    let Some((_, Artist::Cells(cells))) = fig.ir().artist(cells_id) else {
        panic!("occupancy stores cells");
    };
    let values = data(&fig, cells.values);
    assert_eq!(
        values.shape,
        vec![11, 3],
        "eleven rows padded to the longest, of three"
    );
    let row_values = |row: usize| -> Vec<f64> {
        values.as_f64().expect("floating-point")[row * 3..row * 3 + 3]
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .collect()
    };
    let y = &axes(&fig, ax_id).y;
    let members = [
        ("xpanel.f", "psilin"),
        ("xpanel.f", "ggcalc"),
        ("xbl.f", "blsolv"),
        ("xsolve.f", "gauss"),
        ("xsolve.f", "baksub"),
        ("xsolve.f", "ludcmp"),
    ];
    for ((group, member), expected) in members.into_iter().zip(&member_data) {
        let row = rows.row(group, member).expect("a member has a row");
        assert_eq!(
            &row_values(row),
            expected,
            "the cells of row {row} hold the data of {member}"
        );
        assert_eq!(
            label_at(y, row as f64).as_deref(),
            Some(member),
            "the tick at row {row} names {member}"
        );
    }
    for group in ["xpanel.f", "xbl.f", "xsolve.f"] {
        let header = rows.header(group).expect("a group has a header");
        assert!(
            row_values(header).is_empty(),
            "the header of {group} has no cells"
        );
    }
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
}

// WHY: data for a different number of members than were laid out cannot be matched to the rows; placing it anyway
// would shift every later member onto another's name without a sign of the mistake, so `place` refuses it loudly, as
// `Matrix::from_rows` refuses ragged rows.
#[test]
#[should_panic(expected = "one row per member")]
fn placing_data_for_another_number_of_members_panics() {
    let mut fig = Figure::new();
    let rows = fig.axes(0, 0).ycategory_groups(source_files());
    let _ = rows.place(&[vec![0.0], vec![1.0]]);
}

// WHY: the branch map of #74 is the figure this work exists for, so the recipe the guide gives must produce it as a
// valid figure that lays out without a warning: grouped rows of subroutines named in monospace under bold file
// headers, ragged rows of 2 to 40 branches coloured by one of five statuses, the x axis hidden, and a key of the
// statuses as swatches in a panel under the plot.
#[test]
fn a_branch_map_is_a_valid_figure_without_warnings() {
    let groups: Vec<(&str, Vec<String>)> = vec![
        (
            "xpanel.f",
            (0..6).map(|k| format!(r"\texttt{{psilin_{k}}}")).collect(),
        ),
        (
            "xbl.f",
            (0..4).map(|k| format!(r"\texttt{{blvar_{k}}}")).collect(),
        ),
        (
            "xsolve.f",
            (0..5).map(|k| format!(r"\texttt{{gauss_{k}}}")).collect(),
        ),
    ];
    let member_data: Vec<Vec<f64>> = (0..15)
        .map(|k| {
            let branches = 2 + (k * 7) % 39;
            (0..branches).map(|b| ((k + b) % 5) as f64).collect()
        })
        .collect();
    assert!(member_data.iter().any(|r| r.len() == 2) && member_data.iter().any(|r| r.len() == 40));

    let mut fig = Figure::new().tiles(2, 1).fit_row(1);
    let status = fig.categorical_scale(STATUS).id();
    {
        let mut ax = fig.axes(0, 0);
        let rows = ax.ycategory_groups(groups.iter().map(|(g, m)| (*g, m.iter())));
        ax.occupancy(rows.place(&member_data))
            .color_scale(status)
            .label("Branches");
    }
    fig.panel(1, 0)
        .legend()
        .columns(LegendEntryColumns::Count(3))
        .item(status);
    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
    let warnings = fig.layout_warnings();
    assert!(
        warnings.is_empty(),
        "the branch map lays out without a warning: {warnings:?}"
    );
}

// WHY: the case × subroutine matrix of #74 is the second figure this work exists for, so it must be expressible as a
// valid figure that lays out without a warning: cells of fractions on categorical axes whose column names are long
// code identifiers set in monospace and turned through 90°, row names in monospace with the first row at the top, a
// stepped (discrete) colour scale shown by a horizontal colourbar panel under the plot with its title above it, a
// grey "not entered" colour for the non-finite cells shown as a key square beside the bar, and a dot overlay at some
// cell centres with its own entry in a legend panel. Each ingredient has its own tests; this one checks that they
// compose, which is what the yFoil figure needs. A layout warning here would be a figure that a reader sees clipped
// or overflowing.
#[test]
fn a_case_matrix_is_a_valid_figure_without_warnings() {
    let cases = [
        r"\texttt{naca0012_re1e6_alpha_sweep_minus4_to_12}",
        r"\texttt{naca4412_re3e6_cl_target_0p8}",
        r"\texttt{cylinder_inviscid}",
        r"\texttt{flat_plate_transition_forced}",
    ];
    let subroutines = [r"\texttt{psilin}", r"\texttt{ggcalc}", r"\texttt{blsolv}"];
    let nan = f64::NAN;
    let fraction = Matrix::from_rows(&[
        [0.25, 0.5, nan, 1.0],
        [0.0, 0.75, 0.125, nan],
        [nan, 0.375, 0.875, 0.5],
    ]);
    let edges: Vec<f64> = (0..=8).map(|k| f64::from(k) / 8.0).collect();

    let mut fig = Figure::new().tiles(3, 1).fit_row(1).fit_row(2);
    let scale = fig
        .color_scale()
        .title("fraction of the subroutine's branches the case took")
        .limits(0.0, 1.0)
        .discrete(&edges)
        .non_finite(Color::rgb(0.8, 0.8, 0.8))
        .key_label(
            ColorScaleKey::NonFinite,
            "subroutine not entered by the case",
        )
        .id();
    let key = fig.panel(2, 0).legend().id();
    {
        let mut ax = fig.axes(0, 0);
        ax.xcategories(cases)
            .xtick_label_angle(90.0)
            .ycategories(subroutines)
            .legend_in(key);
        ax.cells(&fraction)
            .color_scale(scale)
            .label("Fraction taken");
        ax.scatter([1.0, 2.0], [0.0, 2.0])
            .marker(Marker::Point)
            .label("takes a branch no other case takes");
    }
    fig.panel(1, 0)
        .colorbar(scale)
        .orientation(ColorbarOrientation::Horizontal)
        .title_placement(ColorbarTitlePlacement::Above);

    let report = fig.validate();
    assert!(report.is_valid(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
    let warnings = fig.layout_warnings();
    assert!(
        warnings.is_empty(),
        "the case matrix lays out without a warning: {warnings:?}"
    );
}
