//! The vocabulary of what is drawn and what is not: an artist's label is drawn beside its
//! sample in a legend, and a figure's tags are never drawn and serve the figure browser.
//!
//! The API these tests assume, after the renames of stage 1:
//!
//! - every artist handle has `label(&mut self, impl Into<Text>) -> &mut Self`, which
//!   replaces `label` (what a legend entry shows for a label is the IR's promise,
//!   tested in `ironlab-ir`);
//! - `Figure::tag(self, impl Into<String>) -> Self` and `Figure::tags(&self) -> &[String]`
//!   replace `Figure::label` and `Figure::labels`.

use ironlab::ir::IssueKind;
use ironlab::prelude::*;

/// Returns the label of the artist with the given identifier.
#[track_caller]
fn label_of(fig: &Figure, id: NodeId) -> Option<&Text> {
    fig.ir()
        .artist(id)
        .unwrap_or_else(|| panic!("{id} is not an artist"))
        .1
        .label()
}

// PROMISE: every kind of artist is labelled through the same setter, so that the rename
// leaves no handle with the old name or without the new one.
#[test]
fn every_artist_handle_sets_its_label() {
    let x = linspace(0.0, 1.0, 4);
    let y = linspace(0.0, 1.0, 3);
    let z = Matrix::from_fn(3, 4, |row, col| (row + col) as f64);
    let pixels = Pixels::from_rgba8(1, 1, &[255, 0, 0, 255]).unwrap();
    let mut fig = Figure::new().tiles(1, 2);
    let mut ax = fig.axes(0, 0);
    let ids = [
        ax.plot(&x, &x).label("line").id(),
        ax.scatter(&x, &x).label("scatter").id(),
        ax.contour(&x, &y, &z).label("contour").id(),
        ax.quiver(&x, &x, &x, &x).label("quiver").id(),
        ax.image(&pixels).label("image").id(),
        ax.indexed_image(&z).label("indexed image").id(),
        ax.mapped_image(&z).label("mapped image").id(),
    ];
    let surface = fig.axes3(0, 1).surf(&x, &y, &z).label("surface").id();
    for (id, text) in ids
        .into_iter()
        .zip([
            "line",
            "scatter",
            "contour",
            "quiver",
            "image",
            "indexed image",
            "mapped image",
        ])
        .chain([(surface, "surface")])
    {
        assert_eq!(label_of(&fig, id), Some(&Text::new(text)), "{text}");
    }
}

// PROMISE: tags are a sequence, not a set and not a map, and the order a figure carries
// them in is the order the browser lists them in. The builder therefore adds to them
// rather than replacing them, which is the opposite of what setting a parameter of the
// same name does, and reads them back in that order, from a wrapped IR as well.
#[test]
fn tag_adds_to_the_tags_in_the_order_they_are_given() {
    let fig = Figure::new()
        .tag("wake")
        .tag("piv")
        .tag(String::from("2025"));
    assert_eq!(fig.tags(), ["wake", "piv", "2025"]);
    assert_eq!(fig.ir().tags, ["wake", "piv", "2025"]);
    assert!(fig.validate().is_valid(), "{:?}", fig.validate());
    assert_eq!(
        Figure::from_ir(fig.into_ir()).tags(),
        ["wake", "piv", "2025"]
    );
}

// PROMISE: the builder stores the tags it is given, never tidying away an empty or a
// repeated one, which validation reports instead (the IR tests what the report says).
#[test]
fn an_empty_or_repeated_tag_is_kept_and_reported_by_validation() {
    for fig in [
        Figure::new().tag("wake").tag(""),
        Figure::new().tag("wake").tag("wake"),
    ] {
        assert_eq!(fig.tags().len(), 2, "{:?}", fig.tags());
        let kinds: Vec<IssueKind> = fig.validate().errors.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, [IssueKind::InvalidTag], "{:?}", fig.tags());
    }
}
