//! Paths to the entries of a palette: the colour of each entry of an indexed colour scale, the colour and the label of
//! each category of a categorical one, and each colour of a listed colormap (stage 6 of the colourbar epic).
//!
//! The viewer edits properties only. The entries of a palette come from the program that builds the figure, as its
//! data does, so their number and their order are structure, which no edit path reaches; but the colour of an entry
//! and the label of a category are properties of that entry, which a reader may well want to adjust. Each entry is
//! therefore addressed by its index, and the list itself stays read-only.
//!
//! The IR API these tests assume:
//!
//! ```text
//! // Paths. An entry is addressed by a path segment holding its index in canonical decimal form (no sign, no leading
//! // zero, no exponent), after the segment that names the list:
//! //   kind.indexed.colors.<i>                      Color  (and .r, .g, .b, .a below it)
//! //   kind.categorical.categories.<i>.color        Color  (and its components)
//! //   kind.categorical.categories.<i>.label        Text   (and .content, .interpreter)
//! //   kind.continuous.colormap.colors.<i>          Color  (while the colormap is Listed)
//! //   kind.discrete.colormap.colors.<i>            Color  (while the colormap is Listed)
//! // The list itself (`kind.indexed.colors`, `kind.categorical.categories`, `….colormap.colors`) stays read-only, so
//! // that no set changes how many entries there are.
//!
//! // crates/ironlab-ir/src/edit/mod.rs
//! pub enum EditError {
//!     ...,
//!     /// The path names an entry of a list by an index that the list does not have, such as
//!     /// `kind.indexed.colors.9` of a palette of nine entries.
//!     EntryOutOfRange { edit: Option<usize>, node: NodeId, path: PropertyPath, count: usize },
//! }
//!
//! // crates/ironlab-ir/src/edit/registry.rs
//! /// Every settable property of one node as it stands: the registry of its kind (`properties(kind)`), followed by
//! /// one property per entry of each palette the node holds, and the properties below each entry, every one of
//! /// them `conditional`, because which entries there are depends on the node.
//! pub fn properties_of(figure: &Figure, node: NodeId) -> Vec<Property>;
//! ```
//!
//! The registry of a kind, [`properties`], lists no entry path, because which entries a node has is not a property of
//! its kind; it stays the list compared with `main` for breaking changes. Composition drops an overlay entry of an
//! entry that the source no longer has, with [`EditError::EntryOutOfRange`] as its reason.

mod common;

use common::edits::{applied, path, set, tx};
use common::{FigureBuilder, color_scale_variants_figure, indexed, out_of_range};
use ironlab_ir::overlay::Overlay;
use ironlab_ir::*;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const GREY: Color = Color::rgb(0.5, 0.5, 0.5);

/// A figure of four scales and no artists: an indexed palette of red, green and blue (`palette`), a categorical
/// scale of the categories Water (blue) and Forest (green) (`classes`), a continuous scale of a listed colormap of
/// red, grey and blue (`listed`), and a continuous scale of a named colormap (`named`).
struct Palettes {
    figure: Figure,
    palette: NodeId,
    classes: NodeId,
    listed: NodeId,
    named: NodeId,
}

fn palettes() -> Palettes {
    let mut b = FigureBuilder::new();
    let palette = b.color_scale(indexed(vec![RED, GREEN, BLUE], OutOfRange::Strict));
    let classes = b.color_scale(ColorScaleKind::Categorical(CategoricalColorScale {
        categories: vec![
            ColorCategory {
                color: BLUE,
                label: Text::plain("Water"),
            },
            ColorCategory {
                color: GREEN,
                label: Text::plain("Forest"),
            },
        ],
        outside: out_of_range(OutOfRange::Strict),
    }));
    let listed = b.color_scale(ColorScaleKind::Continuous(ContinuousColorScale {
        colormap: Colormap::Listed {
            colors: vec![RED, GREY, BLUE],
        },
        ..ContinuousColorScale::default()
    }));
    let named = b.color_scale(ColorScaleKind::Continuous(ContinuousColorScale::default()));
    Palettes {
        figure: b.build(),
        palette,
        classes,
        listed,
        named,
    }
}

fn colors_of(figure: &Figure, scale: NodeId) -> Vec<Color> {
    match &figure.color_scale(scale).expect("the scale exists").kind {
        ColorScaleKind::Indexed(scale) => scale.colors.clone(),
        ColorScaleKind::Continuous(ContinuousColorScale {
            colormap: Colormap::Listed { colors },
            ..
        }) => colors.clone(),
        other => panic!("the scale has a list of colours: {other:?}"),
    }
}

fn categories_of(figure: &Figure, scale: NodeId) -> Vec<ColorCategory> {
    match &figure.color_scale(scale).expect("the scale exists").kind {
        ColorScaleKind::Categorical(scale) => scale.categories.clone(),
        other => panic!("the scale is categorical: {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Reading and setting one entry
// ---------------------------------------------------------------------------------------------------------------

// Why: the colour of one entry of a palette is what a reader adjusts when a class is hard to tell from its
// neighbour, and the change must be exactly that: one entry takes the new colour, every other entry and the number
// of entries are untouched, and the inverse puts the old colour back bit for bit, as for every other set.
#[test]
fn setting_one_entry_of_an_indexed_palette_changes_that_entry_alone() {
    let p = palettes();
    assert_eq!(
        p.figure
            .get(p.palette, &path("kind.indexed.colors.1"))
            .unwrap(),
        Value::Color(GREEN)
    );

    let (edited, inverse) = applied(
        &p.figure,
        &tx([set(p.palette, "kind.indexed.colors.1", Value::Color(GREY))]),
    )
    .unwrap();
    assert_eq!(colors_of(&edited, p.palette), [RED, GREY, BLUE]);
    assert_eq!(
        edited
            .get(p.palette, &path("kind.indexed.colors.1"))
            .unwrap(),
        Value::Color(GREY)
    );
    let (restored, _) = applied(&edited, &inverse).unwrap();
    assert_eq!(
        restored, p.figure,
        "the inverse restores the palette exactly"
    );

    let (edited, _) = applied(
        &p.figure,
        &tx([set(p.palette, "kind.indexed.colors.2.g", Value::Float(0.5))]),
    )
    .unwrap();
    assert_eq!(
        colors_of(&edited, p.palette)[2],
        Color::rgb(0.0, 0.5, 1.0),
        "a component of an entry is addressed below it, as the components of any colour are"
    );
}

// Why: a category is a colour and a label, and both are what a reader adjusts: the colour to tell classes apart, and
// the label to correct or translate a name. Each is set on its own, leaving the other and every other category alone.
#[test]
fn the_colour_and_the_label_of_one_category_are_set_on_their_own() {
    let p = palettes();
    assert_eq!(
        p.figure
            .get(p.classes, &path("kind.categorical.categories.1.label"))
            .unwrap(),
        Value::Text(Text::plain("Forest"))
    );

    let (edited, _) = applied(
        &p.figure,
        &tx([
            set(
                p.classes,
                "kind.categorical.categories.1.label",
                Value::Text(Text::plain("Woodland")),
            ),
            set(
                p.classes,
                "kind.categorical.categories.0.color",
                Value::Color(GREY),
            ),
        ]),
    )
    .unwrap();
    let categories = categories_of(&edited, p.classes);
    assert_eq!(categories.len(), 2, "no category is added or removed");
    assert_eq!(categories[0].color, GREY);
    assert_eq!(
        categories[0].label,
        Text::plain("Water"),
        "the label of the recoloured category stays"
    );
    assert_eq!(
        categories[1].color, GREEN,
        "the colour of the renamed category stays"
    );
    assert_eq!(categories[1].label, Text::plain("Woodland"));

    let (edited, _) = applied(
        &p.figure,
        &tx([set(
            p.classes,
            "kind.categorical.categories.0.label.content",
            Value::String("Sea".to_owned()),
        )]),
    )
    .unwrap();
    assert_eq!(categories_of(&edited, p.classes)[0].label.content, "Sea");
}

// Why: a listed colormap is a palette the author typed, and its colours are adjusted exactly as the entries of an
// indexed palette are; while the colormap is a named one there is no list to address, which is the inactive-variant
// error that every path into an unset variant gives.
#[test]
fn a_colour_of_a_listed_colormap_is_set_by_index_and_a_named_colormap_has_none() {
    let p = palettes();
    let (edited, _) = applied(
        &p.figure,
        &tx([set(
            p.listed,
            "kind.continuous.colormap.colors.2",
            Value::Color(GREEN),
        )]),
    )
    .unwrap();
    assert_eq!(colors_of(&edited, p.listed), [RED, GREY, GREEN]);

    assert!(
        matches!(
            p.figure
                .get(p.named, &path("kind.continuous.colormap.colors.0")),
            Err(EditError::InactiveVariant { edit: None, .. })
        ),
        "a named colormap has no listed colours"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// What cannot be set
// ---------------------------------------------------------------------------------------------------------------

// Why: the entries of a palette are structure, as the data of a plot is: setting the whole list would let an edit add
// or remove entries, which only the program that builds the figure does. The lists therefore stay read-only, and the
// entries below them are what is settable.
#[test]
fn the_list_of_entries_itself_stays_read_only() {
    let p = palettes();
    for (node, at, value) in [
        (p.palette, "kind.indexed.colors", Value::Color(RED)),
        (p.classes, "kind.categorical.categories", Value::Color(RED)),
        (
            p.listed,
            "kind.continuous.colormap.colors",
            Value::Color(RED),
        ),
    ] {
        assert!(
            matches!(
                p.figure.get(node, &path(at)),
                Err(EditError::ReadOnly { .. })
            ),
            "reading {at} whole"
        );
        let mut edited = p.figure.clone();
        assert!(
            matches!(
                edited.apply(&tx([set(node, at, value)])),
                Err(EditError::ReadOnly { edit: Some(0), .. })
            ),
            "setting {at} whole"
        );
        assert_eq!(edited, p.figure);
    }
}

// Why: an index the list does not have names nothing, and an edit of nothing must be refused, with an error that
// says the entry is missing rather than that the path is misspelt, so that a client which mirrors a figure whose
// palette shrank can tell the two apart; the figure is left exactly as it was.
#[test]
fn an_index_past_the_last_entry_is_refused_as_out_of_range() {
    let p = palettes();
    for (node, at, count) in [
        (p.palette, "kind.indexed.colors.3", 3),
        (p.palette, "kind.indexed.colors.18446744073709551616", 3),
        (p.classes, "kind.categorical.categories.2.color", 2),
        (p.classes, "kind.categorical.categories.7.label", 2),
        (p.listed, "kind.continuous.colormap.colors.3", 3),
    ] {
        assert!(
            matches!(
                p.figure.get(node, &path(at)),
                Err(EditError::EntryOutOfRange { edit: None, node: n, count: c, .. })
                    if n == node && c == count
            ),
            "reading {at}: {:?}",
            p.figure.get(node, &path(at))
        );
        let mut edited = p.figure.clone();
        let result = edited.apply(&tx([set(node, at, Value::Color(GREY))]));
        assert!(
            matches!(
                result,
                Err(EditError::EntryOutOfRange { edit: Some(0), .. })
            ),
            "setting {at}: {result:?}"
        );
        assert_eq!(edited, p.figure);
    }
}

// Why: a path is an address that clients compare as text, in the overlay and in conflict detection, so one entry must
// have exactly one address; an index written with a sign, a leading zero or an exponent is not that address and is
// an unknown path, exactly as a misspelt field name is.
#[test]
fn an_index_that_is_not_canonical_decimal_is_an_unknown_path() {
    let p = palettes();
    for at in [
        "kind.indexed.colors.01",
        "kind.indexed.colors.+1",
        "kind.indexed.colors.-1",
        "kind.indexed.colors.1e0",
        "kind.indexed.colors.one",
        "kind.indexed.colors. 1",
    ] {
        assert!(
            matches!(
                p.figure.get(p.palette, &path(at)),
                Err(EditError::UnknownPath { edit: None, .. })
            ),
            "{at}: {:?}",
            p.figure.get(p.palette, &path(at))
        );
    }
}

// Why: values are typed when applied, and an entry of a palette is a colour, so a value of another type is refused
// with the type the entry expects, as for every other property.
#[test]
fn an_entry_takes_a_value_of_its_own_type_only() {
    let p = palettes();
    for (node, at, value, expected) in [
        (
            p.palette,
            "kind.indexed.colors.0",
            Value::Text(Text::plain("red")),
            ValueType::Color,
        ),
        (
            p.classes,
            "kind.categorical.categories.0.label",
            Value::Color(RED),
            ValueType::Text,
        ),
    ] {
        let mut edited = p.figure.clone();
        let result = edited.apply(&tx([set(node, at, value)]));
        assert!(
            matches!(
                result,
                Err(EditError::TypeMismatch { edit: Some(0), expected: e, .. }) if e == expected
            ),
            "{at}: {result:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The properties of one node
// ---------------------------------------------------------------------------------------------------------------

// Why: the inspector lists what a node has, so the listing of one node must name exactly the entries it holds — one
// path per entry, in index order, and none past the last — while the registry of the kind, which describes every
// node of the kind alike, names no entry at all.
#[test]
fn the_properties_of_a_node_list_one_path_per_entry_it_holds() {
    let p = palettes();
    let listed: Vec<String> = properties_of(&p.figure, p.palette)
        .into_iter()
        .map(|property| property.path.to_string())
        .filter(|at| at.starts_with("kind.indexed.colors."))
        .filter(|at| at.split('.').count() == 4)
        .collect();
    assert_eq!(
        listed,
        [
            "kind.indexed.colors.0",
            "kind.indexed.colors.1",
            "kind.indexed.colors.2"
        ]
    );

    let classes: Vec<(String, ValueType)> = properties_of(&p.figure, p.classes)
        .into_iter()
        .filter(|property| {
            property
                .path
                .to_string()
                .starts_with("kind.categorical.categories.")
        })
        .filter(|property| property.path.segments().len() == 5)
        .map(|property| (property.path.to_string(), property.value_type))
        .collect();
    for (k, field, value_type) in [
        (0, "color", ValueType::Color),
        (0, "label", ValueType::Text),
        (1, "color", ValueType::Color),
        (1, "label", ValueType::Text),
    ] {
        let at = format!("kind.categorical.categories.{k}.{field}");
        assert!(
            classes.contains(&(at.clone(), value_type)),
            "{at} is listed as {value_type:?}: {classes:?}"
        );
    }
    assert!(
        !classes
            .iter()
            .any(|(at, _)| at.starts_with("kind.categorical.categories.2.")),
        "no category past the last is listed"
    );

    for kind in NodeKind::ALL {
        for property in properties(kind) {
            assert!(
                !property
                    .path
                    .segments()
                    .iter()
                    .any(|segment| segment.chars().all(|c| c.is_ascii_digit())),
                "the registry of {kind:?} names an entry: {}",
                property.path
            );
        }
    }
}

// Why: this keeps the per-node listing honest in the way the registry test keeps the registry honest: across figures
// that set every kind of palette, everything listed for a node must be readable, hold a value of the listed type and
// take its own value back without any change; everything beyond the registry of the kind must be an entry path; and
// every entry must be marked conditional, because whether it exists depends on the node.
#[test]
fn every_property_listed_for_a_node_can_be_read_and_set_to_its_own_value() {
    for figure in [palettes().figure, color_scale_variants_figure()] {
        for scale in &figure.color_scales {
            let node = scale.id;
            let registry: Vec<PropertyPath> = properties(NodeKind::ColorScale)
                .into_iter()
                .map(|property| property.path)
                .collect();
            for property in properties_of(&figure, node) {
                let at = format!("{node} {}", property.path);
                if !registry.contains(&property.path) {
                    assert!(
                        property.conditional,
                        "{at}: an entry path is conditional on the node"
                    );
                    assert!(
                        property
                            .path
                            .segments()
                            .iter()
                            .any(|segment| segment.chars().all(|c| c.is_ascii_digit())),
                        "{at}: what the registry does not list is an entry path"
                    );
                    assert!(
                        !property.docs.is_empty(),
                        "{at}: an entry path is documented"
                    );
                }
                match figure.get(node, &property.path) {
                    Ok(value) => {
                        if let Some(found) = value.value_type() {
                            assert_eq!(found, property.value_type, "{at}");
                        }
                        let (edited, _) = applied(
                            &figure,
                            &tx([Edit::Set {
                                node,
                                path: property.path.clone(),
                                value,
                            }]),
                        )
                        .unwrap_or_else(|e| panic!("{at}: setting its own value failed: {e:?}"));
                        assert_eq!(
                            edited, figure,
                            "{at}: setting its own value changed the figure"
                        );
                    }
                    Err(EditError::InactiveVariant { .. } | EditError::AbsentValue { .. })
                        if property.conditional => {}
                    Err(error) => panic!("{at}: {error:?}"),
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The overlay
// ---------------------------------------------------------------------------------------------------------------

// Why: the viewer records the colour of an entry in its overlay, and the program that owns the figure may later send
// a palette with fewer entries; the entry the user recoloured is then gone, and its overlay entry must be dropped with
// the reason that names a missing entry, while a recoloured entry the palette still has keeps its colour.
#[test]
fn composition_drops_the_colour_of_an_entry_the_source_no_longer_has() {
    let p = palettes();
    let mut overlay = Overlay::new();
    overlay
        .record(&tx([
            set(p.palette, "kind.indexed.colors.0", Value::Color(GREY)),
            set(p.palette, "kind.indexed.colors.2", Value::Color(GREY)),
        ]))
        .unwrap();

    let mut shrunk = p.figure.clone();
    match &mut shrunk.color_scale_mut(p.palette).unwrap().kind {
        ColorScaleKind::Indexed(scale) => scale.colors.truncate(2),
        other => panic!("the palette is indexed: {other:?}"),
    }
    let composition = overlay.compose(&shrunk);
    assert_eq!(colors_of(&composition.figure, p.palette), [GREY, GREEN]);
    assert_eq!(composition.dropped.len(), 1);
    assert_eq!(
        composition.dropped[0].entry.path,
        path("kind.indexed.colors.2")
    );
    assert!(
        matches!(
            composition.dropped[0].reason,
            EditError::EntryOutOfRange { count: 2, .. }
        ),
        "{:?}",
        composition.dropped[0].reason
    );
}
