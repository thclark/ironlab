//! The generated JSON Schema of the `.fig.json` format.
//!
//! The schema is generated from the Rust types and committed in `schema/`, where the
//! pre-commit hook and CI check that the committed files equal those generated; these
//! tests check that the generated documents are usable on their own and describe what
//! the reader accepts.

use std::collections::BTreeSet;

use serde_json::{Value, json};

/// Collects the value of every `$ref` keyword in a schema document.
fn collect_refs<'a>(value: &'a Value, refs: &mut Vec<&'a str>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                match (key.as_str(), inner) {
                    ("$ref", Value::String(reference)) => refs.push(reference),
                    _ => collect_refs(inner, refs),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_refs(item, refs)),
        _ => {}
    }
}

// Why: the single documents `ironlab-figure.schema.json` and `ironlab-edit.schema.json`
// are what strands.octue.com versions and what tools that load one file validate
// against. Such a tool resolves nothing outside the document, so every `$ref` must be a
// local JSON pointer into `$defs` that names a definition the document holds. A
// reference to another file, or to a definition left behind in another document, would
// make the document unusable on its own and would hide part of the schema from the
// version comparison.
#[test]
fn the_single_documents_refer_only_to_their_own_definitions() {
    for (name, root) in [
        ("ironlab-figure.schema.json", ironlab_ir::json_schema()),
        (
            "ironlab-edit.schema.json",
            ironlab_ir::transaction_json_schema(),
        ),
    ] {
        let mut refs = Vec::new();
        collect_refs(&root, &mut refs);
        assert!(!refs.is_empty(), "{name} has no references to check");
        let problems: Vec<String> = refs
            .into_iter()
            .filter(|reference| {
                !reference.starts_with("#/$defs/") || root.pointer(&reference[1..]).is_none()
            })
            .map(|reference| format!("{name}: {reference} is not a local definition"))
            .collect();
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }
}

/// Collects every string that a schema allows under its `enum` and `const` keywords,
/// following local references (`#/$defs/...`) within `root`, so that the check does not
/// depend on whether the generator inlines an enumeration or refers to a definition, or
/// wraps it to allow `null`.
fn allowed_strings(
    root: &Value,
    schema: &Value,
    seen: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    match schema {
        Value::Object(map) => {
            for (key, inner) in map {
                match (key.as_str(), inner) {
                    ("enum", Value::Array(items)) => {
                        out.extend(items.iter().filter_map(Value::as_str).map(str::to_owned));
                    }
                    ("const", Value::String(text)) => {
                        out.insert(text.clone());
                    }
                    ("$ref", Value::String(reference)) => {
                        if seen.insert(reference.clone()) {
                            let pointer = reference
                                .strip_prefix('#')
                                .unwrap_or_else(|| panic!("{reference} is not a local reference"));
                            let target = root
                                .pointer(pointer)
                                .unwrap_or_else(|| panic!("{reference} does not resolve"));
                            allowed_strings(root, target, seen, out);
                        }
                    }
                    ("description" | "title" | "default" | "examples", _) => {}
                    (_, inner) => allowed_strings(root, inner, seen, out),
                }
            }
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| allowed_strings(root, item, seen, out)),
        _ => {}
    }
}

// Why: the array definition is what other tools validate figure files against. It must
// keep the name `NdArray`, which artists' data and the edit protocol refer to, whatever
// Rust type now produces it; its `element` property must admit exactly the element types
// that the reader accepts (`f64` and `u8`) and must not be required, because every file
// written before the element type existed omits it; its `values` must still admit `null`,
// which is how a missing float is written; and the Rust enum that holds the values must
// not surface as a definition of its own (`Values`), because it is an implementation
// detail of the reader with no counterpart in either encoding.
#[test]
fn the_array_definition_keeps_its_name_and_declares_the_element_type() {
    let schema = ironlab_ir::json_schema();
    let array = schema
        .pointer("/$defs/NdArray")
        .expect("the schema defines NdArray");
    let element = array
        .pointer("/properties/element")
        .expect("NdArray has an element property");
    let mut allowed = BTreeSet::new();
    allowed_strings(&schema, element, &mut BTreeSet::new(), &mut allowed);
    assert_eq!(
        allowed,
        BTreeSet::from(["f64".to_owned(), "u8".to_owned()]),
        "the element property allows {allowed:?}"
    );
    let required: Vec<&str> = array
        .get("required")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    assert!(
        !required.contains(&"element"),
        "element is required, so files without it would not validate: {required:?}"
    );
    assert!(
        required.contains(&"shape") && required.contains(&"values"),
        "shape and values are required: {required:?}"
    );
    assert_eq!(
        array.pointer("/properties/values/items/type"),
        Some(&json!(["number", "null"])),
        "the values must stay number-or-null, so that a missing float is still written as null"
    );
    assert!(
        schema.pointer("/$defs/Values").is_none(),
        "the schema defines Values"
    );
}

// Why: web clients that build transactions as JSON need a schema to validate them against,
// generated like that of figures: `ironlab-edit.schema.json` must describe a transaction
// document at its root (inline, or by a reference to its own definition), and hold the
// definitions of edits, values and nodes together with the IR types that values and nodes
// contain, such as limits and artists, because the document is used on its own.
#[test]
fn the_json_schema_of_a_transaction_describes_a_transaction_on_its_own() {
    let edit = ironlab_ir::transaction_json_schema();
    for name in ["Edit", "Value", "Node", "Limits", "Artist"] {
        assert!(
            edit.pointer(&format!("/$defs/{name}")).is_some(),
            "ironlab-edit.schema.json does not define {name}"
        );
    }
    let root_is_transaction = edit.pointer("/properties/edits").is_some()
        || (edit.get("$ref") == Some(&Value::from("#/$defs/Transaction"))
            && edit
                .pointer("/$defs/Transaction/properties/edits")
                .is_some());
    assert!(
        root_is_transaction,
        "the root of ironlab-edit.schema.json does not describe a transaction"
    );
}

// Why: tools that validate `.fig.json` files find the definitions of the image kinds and
// the colour scales by name in the figure schema, so a definition missing or renamed would
// leave such files unvalidatable. The tagged types must also admit exactly the variant
// names of the documented form, which is what a hand-written file writes, and the
// replaced image kinds must be gone.
#[test]
fn the_image_and_colour_scale_types_are_defined_with_their_variant_names() {
    let schema = ironlab_ir::json_schema();
    for name in [
        "Image",
        "ScalarImage",
        "ImagePlacement",
        "PixelRange",
        "ImagePlane",
        "ColorScale",
        "ColorScaleKind",
        "ColorScaleOutOfRange",
        "ContinuousColorScale",
        "DiscreteColorScale",
        "IndexedColorScale",
        "CategoricalColorScale",
        "ColorCategory",
        "Colormap",
        "ColorScaleNorm",
        "ColorScaleLevels",
        "OutOfRange",
    ] {
        assert!(
            schema.pointer(&format!("/$defs/{name}")).is_some(),
            "the schema does not define {name}"
        );
    }
    for gone in ["IndexedImage", "MappedImage"] {
        assert!(
            schema.pointer(&format!("/$defs/{gone}")).is_none(),
            "the schema still defines {gone}"
        );
    }

    for name in ["Binning", "Binning2d", "Bins", "BinRule", "HistNorm"] {
        assert!(
            schema.pointer(&format!("/$defs/{name}")).is_some(),
            "the schema does not define {name}"
        );
    }
    for gone in ["BarBase", "BarSlot", "BarGroup", "BarLayout"] {
        assert!(
            schema.pointer(&format!("/$defs/{gone}")).is_none(),
            "the schema still defines {gone}"
        );
    }

    let strings_under = |name: &str| {
        let definition = schema
            .pointer(&format!("/$defs/{name}"))
            .unwrap_or_else(|| panic!("the schema defines {name}"));
        let mut allowed = BTreeSet::new();
        allowed_strings(&schema, definition, &mut BTreeSet::new(), &mut allowed);
        allowed
    };
    for (name, variants) in [
        ("ImagePlane", &["xy", "xz", "yz"][..]),
        (
            "OutOfRange",
            &["strict", "transparent", "clamp", "rgba"][..],
        ),
        ("Artist", &["image", "scalar_image"][..]),
        (
            "ColorScaleKind",
            &["continuous", "discrete", "indexed", "categorical"][..],
        ),
        ("Colormap", &["named", "listed"][..]),
        ("ColorScaleLevels", &["auto", "explicit"][..]),
        ("ColorScaleNorm", &["linear", "log"][..]),
    ] {
        let allowed = strings_under(name);
        for variant in variants {
            assert!(
                allowed.contains(*variant),
                "{name} does not admit {variant:?}; it admits {allowed:?}"
            );
        }
    }
    let artists = strings_under("Artist");
    for gone in ["indexed_image", "mapped_image"] {
        assert!(!artists.contains(gone), "Artist still admits {gone:?}");
    }
}

// Why: tools that validate `.fig.json` files find the definitions of bars, stairs and cells
// by name in the figure schema, so a type missing or defined under another name would
// leave such files unvalidatable. The tagged types must admit exactly the variant names
// of the documented form, which is what a hand-written file writes: the kinds of artist,
// the orientations, the bases, widths and offsets of bars, the two forms of stairs data
// with the three step positions, the two kinds of cell edges, and the kinds of bins, the
// bin rules and the normalisations of a binning record.
#[test]
fn the_bar_stairs_and_cells_types_are_defined_with_their_variant_names() {
    let schema = ironlab_ir::json_schema();
    for name in [
        "Bar",
        "Stairs",
        "Cells",
        "Orientation",
        "PerBar",
        "BarWidth",
        "BarOffset",
        "StairsData",
        "StepAt",
        "CellEdges",
        "CellGap",
    ] {
        assert!(
            schema.pointer(&format!("/$defs/{name}")).is_some(),
            "the schema does not define {name}"
        );
    }

    let strings_under = |name: &str| {
        let definition = schema
            .pointer(&format!("/$defs/{name}"))
            .unwrap_or_else(|| panic!("the schema defines {name}"));
        let mut allowed = BTreeSet::new();
        allowed_strings(&schema, definition, &mut BTreeSet::new(), &mut allowed);
        allowed
    };
    for (name, variants) in [
        ("Artist", &["bar", "stairs", "cells"][..]),
        ("Orientation", &["vertical", "horizontal"][..]),
        ("PerBar", &["constant", "data"][..]),
        ("BarWidth", &["pitch", "constant", "data"][..]),
        ("BarOffset", &["widths", "absolute"][..]),
        ("Bins", &["count", "width", "edges", "rule"][..]),
        (
            "BinRule",
            &[
                "auto",
                "sturges",
                "scott",
                "freedman_diaconis",
                "sqrt",
                "integers",
            ][..],
        ),
        (
            "HistNorm",
            &[
                "count",
                "probability",
                "percentage",
                "count_density",
                "density",
                "cumulative_count",
                "cdf",
            ][..],
        ),
        ("StairsData", &["edges", "samples"][..]),
        ("StepAt", &["pre", "post", "mid"][..]),
        ("CellEdges", &["unit", "edges"][..]),
    ] {
        let allowed = strings_under(name);
        for variant in variants {
            assert!(
                allowed.contains(*variant),
                "{name} does not admit {variant:?}; it admits {allowed:?}"
            );
        }
    }
}

// Why: files written before an axis could be hidden, its tick labels rotated or its side
// chosen have none of these properties, so the schema must not require them, or every
// existing file would fail validation against the schema while IronLAB still reads it.
#[test]
fn the_axis_definition_does_not_require_its_visibility_tick_label_angle_or_side() {
    let schema = ironlab_ir::json_schema();
    let axis = schema
        .pointer("/$defs/Axis")
        .expect("the schema defines Axis");
    for property in ["visible", "tick_label_angle", "side"] {
        assert!(
            axis.pointer(&format!("/properties/{property}")).is_some(),
            "Axis has no {property} property"
        );
    }
    let required: Vec<&str> = axis
        .get("required")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    for property in ["visible", "tick_label_angle", "side"] {
        assert!(
            !required.contains(&property),
            "{property} is required, so files written before it would not validate: {required:?}"
        );
    }
}

// Why: a binning record is left out of the file of a bar chart, stairs or cells that were
// not made from samples, so the schema must not require it, or every plain bar chart would
// fail validation against the schema while IronLAB still reads it; and the removed slot, gap
// and group of bars must no longer be described, since a hand-written file that follows the
// schema would then be refused by the reader.
#[test]
fn the_binning_record_is_described_but_not_required_and_bar_groups_are_gone() {
    let schema = ironlab_ir::json_schema();
    for name in ["Bar", "Stairs", "Cells"] {
        let definition = schema
            .pointer(&format!("/$defs/{name}"))
            .unwrap_or_else(|| panic!("the schema defines {name}"));
        let required: Vec<&str> = definition
            .get("required")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        assert!(
            definition.pointer("/properties/binning").is_some(),
            "{name} has no binning property"
        );
        assert!(
            !required.contains(&"binning"),
            "{name} requires binning: {required:?}"
        );
    }
    let bar = schema
        .pointer("/$defs/Bar")
        .expect("the schema defines Bar");
    for property in ["width", "offset", "margin"] {
        assert!(
            bar.pointer(&format!("/properties/{property}")).is_some(),
            "Bar has no {property} property"
        );
    }
    for gone in ["slot", "gap", "group"] {
        assert!(
            bar.pointer(&format!("/properties/{gone}")).is_none(),
            "Bar still has a {gone} property"
        );
    }
}

// Why: an opacity of 1 is left out of a written file, so the schema must not require any
// opacity of bars, stairs or cells, or every file that keeps them opaque would fail
// validation against the schema while IronLAB still reads it; nor may it still describe the
// per-bar colours or the single opacity of cells, which colour data and the face and edge
// opacities replaced, since a hand-written file that follows the schema would then be
// refused by the reader.
#[test]
fn the_opacities_of_bars_stairs_and_cells_are_described_but_not_required() {
    let schema = ironlab_ir::json_schema();
    for (name, opacities, gone) in [
        ("Bar", &["face_opacity", "edge_opacity"][..], "colors"),
        ("Stairs", &["line_opacity", "fill_opacity"][..], "opacity"),
        ("Cells", &["face_opacity", "edge_opacity"][..], "opacity"),
    ] {
        let definition = schema
            .pointer(&format!("/$defs/{name}"))
            .unwrap_or_else(|| panic!("the schema defines {name}"));
        let required: Vec<&str> = definition
            .get("required")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        for opacity in opacities {
            assert!(
                definition
                    .pointer(&format!("/properties/{opacity}"))
                    .is_some(),
                "{name} has no {opacity} property"
            );
            assert!(
                !required.contains(opacity),
                "{name} requires {opacity}: {required:?}"
            );
        }
        assert!(
            definition.pointer(&format!("/properties/{gone}")).is_none(),
            "{name} still has a {gone} property"
        );
    }
}
