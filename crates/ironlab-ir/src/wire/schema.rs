//! Descriptions of the wire types, from which the `.proto` files are rendered.
//!
//! The descriptions are constructed only by the `proto_file!` macro, from the same
//! declarations that define the Rust wire types.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use super::{FILES, PACKAGE, PACKAGE_DIR};

/// A wire type described by the `proto_file!` macro.
pub(crate) trait ProtoItem {
    /// The description of the type.
    const DEF: ItemDef;
}

/// The description of one `.proto` file.
pub(crate) struct FileDef {
    /// The path of the Rust module that declares the file's types; its last segment
    /// names the file.
    pub module: &'static str,
    /// The messages and enums of the file, in declaration order.
    pub items: &'static [ItemDef],
}

/// A message or an enum.
pub(crate) enum ItemDef {
    /// A message.
    Message(MessageDef),
    /// An enum.
    Enum(EnumDef),
}

/// The description of a message.
pub(crate) struct MessageDef {
    pub name: &'static str,
    pub docs: &'static [&'static str],
    pub fields: &'static [FieldDef],
    pub reserved: &'static [ReservedDef],
}

/// A field or a oneof of a message.
pub(crate) enum FieldDef {
    /// A field outside any oneof.
    Field(FieldSpec),
    /// A oneof and its variant fields.
    Oneof {
        name: &'static str,
        docs: &'static [&'static str],
        variants: &'static [FieldSpec],
    },
}

/// The description of one field.
pub(crate) struct FieldSpec {
    pub docs: &'static [&'static str],
    pub label: Label,
    pub ty: TypeRef,
    pub name: &'static str,
    pub number: u32,
}

/// The label of a field.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Label {
    /// A field without a label: a scalar without presence, a message, an enum or a map.
    Singular,
    /// A scalar with explicit presence.
    Optional,
    /// A repeated field.
    Repeated,
}

/// The type of a field.
pub(crate) enum TypeRef {
    /// A scalar type, by its Protocol Buffers name.
    Scalar(&'static str),
    /// A message or enum of the package, by name.
    Named(&'static str),
    /// A map from a scalar key type to a named value type.
    Map(&'static str, &'static str),
}

/// The description of an enum.
pub(crate) struct EnumDef {
    pub name: &'static str,
    pub docs: &'static [&'static str],
    pub values: &'static [ValueDef],
    pub reserved: &'static [ReservedDef],
}

/// The description of one enum value other than the zero value.
pub(crate) struct ValueDef {
    pub name: &'static str,
    pub docs: &'static [&'static str],
    pub number: i32,
}

/// A `reserved` statement of a message or an enum.
pub(crate) struct ReservedDef {
    pub docs: &'static [&'static str],
    pub items: &'static [ReservedItem],
}

/// A number or a name that a message or an enum reserves.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "no declaration of the schema reserves anything yet; remove this once one does"
    )
)]
#[derive(Clone, Copy)]
pub(crate) enum ReservedItem {
    /// A field number of a message or a value number of an enum.
    Number(i64),
    /// A field name of a message, or the Rust name of an enum value, which is rendered
    /// with the enum's prefix in upper snake case.
    Name(&'static str),
}

impl ItemDef {
    fn name(&self) -> &'static str {
        match self {
            ItemDef::Message(m) => m.name,
            ItemDef::Enum(e) => e.name,
        }
    }
}

impl FileDef {
    /// Returns the name of the file without its extension.
    fn stem(&self) -> &'static str {
        self.module.rsplit("::").next().unwrap_or(self.module)
    }

    /// Returns the path of the file relative to the root of a proto source tree.
    fn path(&self) -> String {
        format!("{PACKAGE_DIR}/{}.proto", self.stem())
    }

    /// Returns the names of the package types that the file's fields refer to.
    fn referenced_types(&self) -> BTreeSet<&'static str> {
        let mut names = BTreeSet::new();
        for item in self.items {
            if let ItemDef::Message(message) = item {
                for field in message.fields {
                    let specs = match field {
                        FieldDef::Field(spec) => std::slice::from_ref(spec),
                        FieldDef::Oneof { variants, .. } => variants,
                    };
                    for spec in specs {
                        match spec.ty {
                            TypeRef::Scalar(_) => {}
                            TypeRef::Named(name) | TypeRef::Map(_, name) => {
                                names.insert(name);
                            }
                        }
                    }
                }
            }
        }
        names
    }
}

/// Returns the name of the module of this crate that declares the wire type of the
/// given name, such as `artist` for `Line`.
///
/// Every domain type that appears on the wire has a wire type of the same name in the
/// wire module of the same name, so this also locates most domain types.
pub(crate) fn module_declaring(type_name: &str) -> Option<&'static str> {
    FILES
        .iter()
        .find(|file| file.items.iter().any(|item| item.name() == type_name))
        .map(FileDef::stem)
}

/// Renders every `.proto` file of the package, with paths relative to the root of a
/// proto source tree.
///
/// # Panics
///
/// Panics when the wire declarations are inconsistent, as described in [`render`].
pub(crate) fn render_files() -> Vec<(PathBuf, String)> {
    render(FILES)
}

/// Renders the given `.proto` files of the package, with paths relative to the root of
/// a proto source tree.
///
/// # Panics
///
/// Panics when a field refers to a type that no file declares, when two files declare
/// a type of the same name, or when a message or an enum uses a number or a name twice
/// (as described in [`check_unique`]); each is a mistake in the wire declarations.
fn render(files: &[FileDef]) -> Vec<(PathBuf, String)> {
    for file in files {
        for item in file.items {
            if let Err(problem) = check_unique(item) {
                panic!("invalid wire declaration in {}: {problem}", file.path());
            }
        }
    }
    let mut owners: BTreeMap<&'static str, &FileDef> = BTreeMap::new();
    for file in files {
        for item in file.items {
            if let Some(other) = owners.insert(item.name(), file) {
                panic!(
                    "wire type {} is declared in both {} and {}",
                    item.name(),
                    other.path(),
                    file.path()
                );
            }
        }
    }
    files
        .iter()
        .map(|file| {
            let imports: BTreeSet<String> = file
                .referenced_types()
                .into_iter()
                .map(|name| {
                    owners.get(name).copied().unwrap_or_else(|| {
                        panic!("{} refers to undeclared wire type {name}", file.path())
                    })
                })
                .filter(|owner| owner.module != file.module)
                .map(FileDef::path)
                .collect();
            (PathBuf::from(file.path()), render_file(file, &imports))
        })
        .collect()
}

/// Renders the text of one `.proto` file.
fn render_file(file: &FileDef, imports: &BTreeSet<String>) -> String {
    let mut out = String::new();
    out.push_str("// Generated from the Rust wire types of ironlab-ir; do not edit.\n\n");
    out.push_str("syntax = \"proto3\";\n\n");
    let _ = writeln!(out, "package {PACKAGE};\n");
    for import in imports {
        let _ = writeln!(out, "import \"{import}\";");
    }
    if !imports.is_empty() {
        out.push('\n');
    }
    for (index, item) in file.items.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        match item {
            ItemDef::Message(message) => render_message(&mut out, item, message),
            ItemDef::Enum(def) => render_enum(&mut out, item, def),
        }
    }
    out
}

/// Checks that a message or an enum uses every number and every name once.
///
/// In a message, the numbers are those of its fields, its oneof variants and its
/// reserved numbers, and the names are those of its fields, its oneofs, its oneof
/// variants and its reserved names. In an enum, the numbers are those of its values
/// (including the zero value) and its reserved numbers, and the names are the rendered
/// names of its values and its reserved names.
///
/// # Errors
///
/// Returns a description of the first number or name that is used twice.
fn check_unique(item: &ItemDef) -> Result<(), String> {
    let mut numbers: BTreeMap<i64, String> = BTreeMap::new();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut add_number = |number: i64, user: String| match numbers.get(&number) {
        Some(other) => Err(format!(
            "{} uses number {number} for both {other} and {user}",
            item.name()
        )),
        None => {
            numbers.insert(number, user);
            Ok(())
        }
    };
    let mut add_name = |name: String, user: String| match names.get(&name) {
        Some(other) => Err(format!(
            "{} uses name {name} for both {other} and {user}",
            item.name()
        )),
        None => {
            names.insert(name, user);
            Ok(())
        }
    };
    let reserved = match item {
        ItemDef::Message(message) => {
            for field in message.fields {
                match field {
                    FieldDef::Field(spec) => {
                        let name = spec.name.trim_start_matches("r#");
                        add_number(i64::from(spec.number), format!("field {name}"))?;
                        add_name(name.to_owned(), format!("field {name}"))?;
                    }
                    FieldDef::Oneof { name, variants, .. } => {
                        add_name((*name).to_owned(), format!("oneof {name}"))?;
                        for spec in *variants {
                            let user = format!("variant {} of oneof {name}", spec.name);
                            add_number(i64::from(spec.number), user.clone())?;
                            add_name(spec.name.to_owned(), user)?;
                        }
                    }
                }
            }
            message.reserved
        }
        ItemDef::Enum(def) => {
            let prefix = upper_snake(def.name);
            let zero = format!("{prefix}_UNSPECIFIED");
            add_number(0, format!("value {zero}"))?;
            add_name(zero.clone(), format!("value {zero}"))?;
            for value in def.values {
                let name = format!("{prefix}_{}", upper_snake(value.name));
                add_number(i64::from(value.number), format!("value {name}"))?;
                add_name(name.clone(), format!("value {name}"))?;
            }
            def.reserved
        }
    };
    for statement in reserved {
        for &reserved_item in statement.items {
            match reserved_item {
                ReservedItem::Number(number) => {
                    add_number(number, format!("reserved number {number}"))?;
                }
                ReservedItem::Name(name) => {
                    let name = reserved_name(item, name);
                    add_name(name.clone(), format!("reserved name {name}"))?;
                }
            }
        }
    }
    Ok(())
}

/// Returns the name that a reserved name of a message or an enum renders as.
fn reserved_name(item: &ItemDef, name: &str) -> String {
    match item {
        ItemDef::Message(_) => name.trim_start_matches("r#").to_owned(),
        ItemDef::Enum(def) => format!("{}_{}", upper_snake(def.name), upper_snake(name)),
    }
}

/// Renders the `reserved` statements of a message or an enum, with numbers and names in
/// separate statements as Protocol Buffers requires.
fn render_reserved(out: &mut String, item: &ItemDef, reserved: &[ReservedDef]) {
    for statement in reserved {
        render_docs(out, "  ", statement.docs);
        let numbers: Vec<String> = statement
            .items
            .iter()
            .filter_map(|reserved_item| match reserved_item {
                ReservedItem::Number(number) => Some(number.to_string()),
                ReservedItem::Name(_) => None,
            })
            .collect();
        let names: Vec<String> = statement
            .items
            .iter()
            .filter_map(|reserved_item| match reserved_item {
                ReservedItem::Number(_) => None,
                ReservedItem::Name(name) => Some(format!("\"{}\"", reserved_name(item, name))),
            })
            .collect();
        for list in [numbers, names] {
            if !list.is_empty() {
                let _ = writeln!(out, "  reserved {};", list.join(", "));
            }
        }
        out.push('\n');
    }
}

fn render_docs(out: &mut String, indent: &str, docs: &[&str]) {
    for line in docs {
        let _ = writeln!(out, "{indent}//{}", line.trim_end());
    }
}

fn render_message(out: &mut String, item: &ItemDef, message: &MessageDef) {
    render_docs(out, "", message.docs);
    if message.fields.is_empty() && message.reserved.is_empty() {
        let _ = writeln!(out, "message {} {{}}", message.name);
        return;
    }
    let _ = writeln!(out, "message {} {{", message.name);
    render_reserved(out, item, message.reserved);
    if message.fields.is_empty() {
        // Remove the blank line that follows the last reserved statement.
        out.pop();
    }
    for (index, field) in message.fields.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        match field {
            FieldDef::Field(spec) => render_field(out, "  ", spec),
            FieldDef::Oneof {
                name,
                docs,
                variants,
            } => {
                render_docs(out, "  ", docs);
                let _ = writeln!(out, "  oneof {name} {{");
                for (index, spec) in variants.iter().enumerate() {
                    if index > 0 {
                        out.push('\n');
                    }
                    render_field(out, "    ", spec);
                }
                out.push_str("  }\n");
            }
        }
    }
    out.push_str("}\n");
}

fn render_field(out: &mut String, indent: &str, spec: &FieldSpec) {
    render_docs(out, indent, spec.docs);
    let label = match spec.label {
        Label::Singular => "",
        Label::Optional => "optional ",
        Label::Repeated => "repeated ",
    };
    let ty = match spec.ty {
        TypeRef::Scalar(name) | TypeRef::Named(name) => name.to_owned(),
        TypeRef::Map(key, value) => format!("map<{key}, {value}>"),
    };
    let name = spec.name.trim_start_matches("r#");
    let _ = writeln!(out, "{indent}{label}{ty} {name} = {};", spec.number);
}

fn render_enum(out: &mut String, item: &ItemDef, def: &EnumDef) {
    let prefix = upper_snake(def.name);
    render_docs(out, "", def.docs);
    let _ = writeln!(out, "enum {} {{", def.name);
    render_reserved(out, item, def.reserved);
    let _ = writeln!(
        out,
        "  // The value is absent; it decodes to the default of its context.\n  {prefix}_UNSPECIFIED = 0;"
    );
    for value in def.values {
        out.push('\n');
        render_docs(out, "  ", value.docs);
        let _ = writeln!(
            out,
            "  {prefix}_{} = {};",
            upper_snake(value.name),
            value.number
        );
    }
    out.push_str("}\n");
}

/// Converts a Rust type or variant name in upper camel case to upper snake case, so
/// that `NorthEast` becomes `NORTH_EAST` and `View3d` becomes `VIEW3D`.
pub(crate) fn upper_snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            let previous = chars[i - 1];
            let next_is_lower = chars.get(i + 1).is_some_and(char::is_ascii_lowercase);
            if previous.is_ascii_lowercase()
                || previous.is_ascii_digit()
                || (previous.is_ascii_uppercase() && next_is_lower)
            {
                out.push('_');
            }
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// Wire types that reserve numbers and names, as a removed field or value would.
    mod reserving {
        proto_file! {
            /// A message with reserved numbers and names.
            message Sample {
                /// A field that remains.
                optional uint64 kept = 1;
                /// Fields removed in an earlier version.
                reserved 2, 3, old_name;
                reserved older_name;
            }

            /// An enum with a reserved number and a reserved name.
            enum Mode {
                /// A value that remains.
                On = 1;
                /// A value removed in an earlier version.
                reserved 2, Off;
            }
        }
    }

    /// Resolves imports from files held in memory.
    struct Files(BTreeMap<String, String>);

    impl protox::file::FileResolver for Files {
        fn open_file(&self, name: &str) -> Result<protox::file::File, protox::Error> {
            match self.0.get(name) {
                Some(source) => protox::file::File::from_source(name, source),
                None => Err(protox::Error::file_not_found(name)),
            }
        }
    }

    // Why: a removed field or enum value must be reserved so that its number and name
    // are never reused with another meaning, which only works when the reservation
    // reaches the generated file that other languages compile and buf compares.
    #[test]
    fn reserved_numbers_and_names_are_rendered_and_compile() {
        let files = render(&[reserving::FILE]);
        let [(path, text)] = files.as_slice() else {
            panic!("one file is rendered");
        };
        assert_eq!(path, &PathBuf::from("ironlab/ir/v0/reserving.proto"));
        for line in [
            "  // Fields removed in an earlier version.\n  reserved 2, 3;\n  reserved \"old_name\";\n",
            "  reserved \"older_name\";\n",
            "  reserved 2;\n  reserved \"MODE_OFF\";\n",
        ] {
            assert!(text.contains(line), "{line:?} is not in:\n{text}");
        }

        let name = path.to_str().expect("the path is UTF-8").to_owned();
        let mut compiler = protox::Compiler::with_file_resolver(Files(BTreeMap::from([(
            name.clone(),
            text.clone(),
        )])));
        compiler
            .open_files([&name])
            .unwrap_or_else(|e| panic!("the rendered file does not compile: {e:?}\n{text}"));
        let pool = compiler.descriptor_pool();
        let sample = pool
            .get_message_by_name("ironlab.ir.v0.Sample")
            .expect("Sample is declared");
        let reserved: Vec<_> = sample.reserved_names().collect();
        assert_eq!(reserved, ["old_name", "older_name"]);
        assert!(sample.reserved_ranges().any(|range| range.contains(&2)));
        let mode = pool
            .get_enum_by_name("ironlab.ir.v0.Mode")
            .expect("Mode is declared");
        assert_eq!(mode.reserved_names().collect::<Vec<_>>(), ["MODE_OFF"]);
    }

    const fn field(name: &'static str, number: u32) -> FieldSpec {
        FieldSpec {
            docs: &[],
            label: Label::Optional,
            ty: TypeRef::Scalar("uint64"),
            name,
            number,
        }
    }

    const fn message(fields: &'static [FieldDef], reserved: &'static [ReservedDef]) -> ItemDef {
        ItemDef::Message(MessageDef {
            name: "Sample",
            docs: &[],
            fields,
            reserved,
        })
    }

    const fn value(name: &'static str, number: i32) -> ValueDef {
        ValueDef {
            name,
            docs: &[],
            number,
        }
    }

    const fn enumeration(values: &'static [ValueDef], reserved: &'static [ReservedDef]) -> ItemDef {
        ItemDef::Enum(EnumDef {
            name: "Mode",
            docs: &[],
            values,
            reserved,
        })
    }

    const fn reserve(items: &'static [ReservedItem]) -> ReservedDef {
        ReservedDef { docs: &[], items }
    }

    // Why: a number or name used twice makes the generated files invalid for every other
    // language (and a reused reserved number silently changes the meaning of old files),
    // yet the Rust compiler cannot see the Protocol Buffers names of oneof variants or
    // reserved declarations. The renderer must reject each kind of duplicate, and name
    // the declaration so that the mistake is easy to find.
    #[test]
    fn duplicate_numbers_and_names_are_rejected() {
        const CASES: [(&str, ItemDef, &str); 10] = [
            (
                "a field number used twice",
                message(
                    &[
                        FieldDef::Field(field("a", 1)),
                        FieldDef::Field(field("b", 1)),
                    ],
                    &[],
                ),
                "Sample uses number 1 for both field a and field b",
            ),
            (
                "a field name used twice",
                message(
                    &[
                        FieldDef::Field(field("a", 1)),
                        FieldDef::Field(field("a", 2)),
                    ],
                    &[],
                ),
                "Sample uses name a for both field a and field a",
            ),
            (
                "a oneof variant numbered like a field",
                message(
                    &[
                        FieldDef::Field(field("a", 1)),
                        FieldDef::Oneof {
                            name: "kind",
                            docs: &[],
                            variants: &[field("b", 1)],
                        },
                    ],
                    &[],
                ),
                "Sample uses number 1 for both field a and variant b of oneof kind",
            ),
            (
                "a oneof variant named like its oneof",
                message(
                    &[FieldDef::Oneof {
                        name: "kind",
                        docs: &[],
                        variants: &[field("kind", 1)],
                    }],
                    &[],
                ),
                "Sample uses name kind for both oneof kind and variant kind of oneof kind",
            ),
            (
                "a field that uses a reserved number",
                message(
                    &[FieldDef::Field(field("a", 2))],
                    &[reserve(&[ReservedItem::Number(2)])],
                ),
                "Sample uses number 2 for both field a and reserved number 2",
            ),
            (
                "a field that uses a reserved name",
                message(
                    &[FieldDef::Field(field("a", 1))],
                    &[reserve(&[ReservedItem::Name("a")])],
                ),
                "Sample uses name a for both field a and reserved name a",
            ),
            (
                "an enum value numbered like the zero value",
                enumeration(&[value("On", 0)], &[]),
                "Mode uses number 0 for both value MODE_UNSPECIFIED and value MODE_ON",
            ),
            (
                "an enum value named like the zero value",
                enumeration(&[value("Unspecified", 1)], &[]),
                "Mode uses name MODE_UNSPECIFIED for both value MODE_UNSPECIFIED and value MODE_UNSPECIFIED",
            ),
            (
                "an enum value that uses a reserved name",
                enumeration(&[value("Off", 1)], &[reserve(&[ReservedItem::Name("Off")])]),
                "Mode uses name MODE_OFF for both value MODE_OFF and reserved name MODE_OFF",
            ),
            (
                "an enum value that uses a reserved number",
                enumeration(&[value("On", 1)], &[reserve(&[ReservedItem::Number(1)])]),
                "Mode uses number 1 for both value MODE_ON and reserved number 1",
            ),
        ];
        for (case, item, expected) in &CASES {
            assert_eq!(check_unique(item), Err((*expected).to_owned()), "{case}");
        }
    }

    // Why: the current declarations must pass the check, or no file could be generated.
    #[test]
    fn the_declarations_of_the_package_use_every_number_and_name_once() {
        for file in FILES {
            for item in file.items {
                assert_eq!(check_unique(item), Ok(()), "{}", file.path());
            }
        }
    }

    // Why: the check must stop generation rather than only report, so that invalid files
    // are never written.
    #[test]
    #[should_panic(
        expected = "invalid wire declaration in ironlab/ir/v0/duplicate.proto: \
                               Sample uses number 1 for both field a and field b"
    )]
    fn rendering_panics_on_a_duplicate() {
        const ITEMS: &[ItemDef] = &[message(
            &[
                FieldDef::Field(field("a", 1)),
                FieldDef::Field(field("b", 1)),
            ],
            &[],
        )];
        render(&[FileDef {
            module: "ironlab_ir::wire::duplicate",
            items: ITEMS,
        }]);
    }
}
