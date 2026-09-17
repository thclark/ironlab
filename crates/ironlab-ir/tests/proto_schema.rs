//! The generated `.proto` files of the `.fig` format: that they compile, follow the
//! package conventions, pass `buf lint`, detect incompatible changes with `buf breaking`,
//! and describe exactly the bytes that the encoder writes.
//!
//! # Field numbers
//!
//! Explicit numbering is not tested here. The `proto_file!` macro that declares the wire
//! types has no syntax for a field, oneof variant or enum value without a number, so an
//! implicit number cannot compile. A number or name used twice in one message or enum,
//! including a reserved number or name, makes the rendering of the files panic; the unit
//! tests of `wire::schema` show this, together with the rendering of `reserved`
//! statements. The generated files are also compiled here, which
//! `the_schema_compiler_rejects_duplicate_numbers_and_names` shows to be a second guard.
//!
//! # Wire compatibility in CI
//!
//! The `.proto` files are build artefacts, so `buf breaking` cannot compare against a
//! committed copy or a Git reference; CI generates the files of both the base branch and
//! the pull request and compares the two directories:
//!
//! ```sh
//! git worktree add "$RUNNER_TEMP/base" origin/main
//! CARGO_TARGET_DIR="$RUNNER_TEMP/base-target" \
//!     cargo run --manifest-path "$RUNNER_TEMP/base/Cargo.toml" -p ironlab-ir --bin generate-proto
//! cargo run -p ironlab-ir --bin generate-proto
//! cd target/ironlab-proto
//! buf lint
//! buf breaking --against "$RUNNER_TEMP/base-target/ironlab-proto"
//! ```
//!
//! The comparison is skipped while the base branch has no `generate-proto` binary. The
//! tests `buf_breaking_detects_renumbering` and
//! `buf_breaking_detects_exchanged_numbers_of_fields_of_the_same_type` show that the
//! generated `buf.yaml` makes this comparison fail for the changes it must catch.
//!
//! The buf tests run when `buf` is installed and are skipped with a message otherwise;
//! setting `IRONLAB_REQUIRE_BUF` turns a missing `buf` into a failure, as CI should.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{float_bits, kitchen_sink_figure, special_values_figure};
use ironlab_ir::*;
use prost::Message;
use prost_reflect::{
    DescriptorPool, DynamicMessage, Kind, MessageDescriptor, ReflectMessage, Value,
};

/// The modules of the IR whose types appear on the wire, each of which has one file.
const MODULES: [&str; 7] = ["artist", "axes", "data", "figure", "link", "style", "text"];

/// Resolves imports from the generated files held in memory.
struct GeneratedFiles(BTreeMap<String, String>);

impl protox::file::FileResolver for GeneratedFiles {
    fn open_file(&self, name: &str) -> Result<protox::file::File, protox::Error> {
        match self.0.get(name) {
            Some(source) => protox::file::File::from_source(name, source),
            None => Err(protox::Error::file_not_found(name)),
        }
    }
}

/// Returns the generated files keyed by their path, with `/` separators.
fn generated_files() -> BTreeMap<String, String> {
    proto_files()
        .into_iter()
        .map(|(path, text)| {
            let name = path
                .iter()
                .map(|part| part.to_str().expect("generated paths are UTF-8"))
                .collect::<Vec<_>>()
                .join("/");
            (name, text)
        })
        .collect()
}

/// Compiles the given files with `protox`, resolving imports only among those files.
fn try_compile(files: BTreeMap<String, String>) -> Result<DescriptorPool, protox::Error> {
    let names: Vec<String> = files.keys().cloned().collect();
    let mut compiler = protox::Compiler::with_file_resolver(GeneratedFiles(files));
    compiler.open_files(&names)?;
    Ok(compiler.descriptor_pool())
}

/// Compiles the generated files with `protox`, resolving imports only among the
/// generated files themselves.
fn compile() -> DescriptorPool {
    try_compile(generated_files())
        .unwrap_or_else(|e| panic!("generated .proto files do not compile: {e:?}"))
}

/// Returns the generated files with the given replacements applied to one file, each of
/// which must match exactly once, so that a change to the schema cannot silently turn
/// a mutation into no change.
fn mutated(path: &str, replacements: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut files = generated_files();
    let text = files
        .get_mut(path)
        .unwrap_or_else(|| panic!("{path} is not generated"));
    for (from, to) in replacements {
        assert_eq!(
            text.matches(from).count(),
            1,
            "{path} must contain {from:?} exactly once"
        );
        *text = text.replacen(from, to, 1);
    }
    files
}

/// Returns the descriptor of the root message.
fn figure_descriptor(pool: &DescriptorPool) -> MessageDescriptor {
    pool.get_message_by_name("ironlab.ir.v0.Figure")
        .expect("the package declares Figure")
}

/// Collects the full names of every message and enum reachable from a message through
/// its fields, excluding synthetic map entry messages.
fn reachable_types(message: &MessageDescriptor, seen: &mut BTreeSet<String>) {
    if !message.is_map_entry() && !seen.insert(message.full_name().to_owned()) {
        return;
    }
    for field in message.fields() {
        match field.kind() {
            Kind::Message(inner) => reachable_types(&inner, seen),
            Kind::Enum(inner) => {
                seen.insert(inner.full_name().to_owned());
            }
            _ => {}
        }
    }
}

/// Converts an upper camel case name to upper snake case, independently of the
/// generator's own conversion.
fn upper_snake(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let boundary = i > 0
            && c.is_ascii_uppercase()
            && (chars[i - 1].is_ascii_lowercase()
                || chars[i - 1].is_ascii_digit()
                || chars.get(i + 1).is_some_and(char::is_ascii_lowercase)
                    && chars[i - 1].is_ascii_uppercase());
        if boundary {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

// ---------------------------------------------------------------------------------
// Compilation and layout
// ---------------------------------------------------------------------------------

// Why: the generated files are the contract offered to other languages, so they must
// compile with a standard protobuf compiler using only each other as imports, in package
// `ironlab.ir.v0`, with each file in the directory that its package names.
#[test]
fn generated_files_compile_in_one_package_with_resolvable_imports() {
    let pool = compile();
    let files = generated_files();
    assert_eq!(pool.files().len(), files.len());
    for file in pool.files() {
        assert_eq!(file.package_name(), "ironlab.ir.v0", "{}", file.name());
        assert!(
            Path::new(file.name()).starts_with("ironlab/ir/v0"),
            "{} is not in the package directory",
            file.name()
        );
        for dependency in file.dependencies() {
            assert!(
                files.contains_key(dependency.name()),
                "{} imports {}, which is not generated",
                file.name(),
                dependency.name()
            );
        }
    }
}

// Why: files mirror the IR's modules, so that a reader of the schema finds a type where
// the Rust source declares it and a change to one module touches one file.
#[test]
fn there_is_one_file_per_module() {
    let names: BTreeSet<String> = generated_files().into_keys().collect();
    let expected: BTreeSet<String> = MODULES
        .iter()
        .map(|module| format!("ironlab/ir/v0/{module}.proto"))
        .collect();
    assert_eq!(names, expected);
}

// Why: every wire type must be declared in exactly one file, and every declared type
// must be part of a figure, so that the generated schema neither duplicates a type nor
// carries a type that the encoder never writes (a sign of a type left out of `Figure`'s
// tree or of a stale declaration).
#[test]
fn every_wire_type_is_declared_in_exactly_one_file_and_reachable_from_figure() {
    let pool = compile();
    let mut declared: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in pool.files() {
        let names = file
            .messages()
            .map(|m| m.full_name().to_owned())
            .chain(file.enums().map(|e| e.full_name().to_owned()));
        for name in names {
            declared
                .entry(name)
                .or_default()
                .push(file.name().to_owned());
        }
    }
    for (name, files) in &declared {
        assert_eq!(files.len(), 1, "{name} is declared in {files:?}");
    }
    for module in MODULES {
        let path = format!("ironlab/ir/v0/{module}.proto");
        assert!(
            declared.values().any(|files| files[0] == path),
            "{path} declares no types"
        );
    }

    let mut reachable = BTreeSet::new();
    reachable_types(&figure_descriptor(&pool), &mut reachable);
    let declared: BTreeSet<String> = declared.into_keys().collect();
    assert_eq!(
        declared.difference(&reachable).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "declared types that no figure can contain"
    );
}

// Why: the Rust compiler and `prost` reject duplicate field numbers and duplicate Rust
// names, but the Protocol Buffers name of a oneof variant is only a string in the
// schema description, so a declaration can give it the name of another field. The
// compilation test above must therefore reject a duplicate number or name in a message;
// this test shows that it does, by compiling generated files altered in those ways.
#[test]
fn the_schema_compiler_rejects_duplicate_numbers_and_names() {
    let cases = [
        (
            "a field number used twice in a message",
            "ironlab/ir/v0/figure.proto",
            (
                "optional double height_mm = 2;",
                "optional double height_mm = 1;",
            ),
        ),
        (
            "a field name used twice in a message",
            "ironlab/ir/v0/figure.proto",
            (
                "optional double height_mm = 2;",
                "optional double width_mm = 2;",
            ),
        ),
        (
            "a oneof variant named like another variant",
            "ironlab/ir/v0/artist.proto",
            ("Surface surface = 5;", "Surface line = 5;"),
        ),
        (
            "a oneof variant named like its oneof",
            "ironlab/ir/v0/artist.proto",
            ("Surface surface = 5;", "Surface kind = 5;"),
        ),
        (
            "an enum number used twice",
            "ironlab/ir/v0/link.proto",
            ("DIMENSION_Z = 3;", "DIMENSION_Z = 2;"),
        ),
    ];
    for (case, path, replacement) in cases {
        assert!(
            try_compile(mutated(path, &[replacement])).is_err(),
            "{case} was accepted"
        );
    }
}

// Why: the generated files are read by people writing clients in other languages, who
// have no access to the Rust source; every message, enum, enum value, oneof and field
// must carry the explanation from its Rust declaration as a leading comment, which
// protoc plugins copy into generated code.
#[test]
fn every_declaration_in_the_generated_files_has_a_leading_comment() {
    let mut undocumented = Vec::new();
    for (path, text) in generated_files() {
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        for (i, line) in lines.iter().enumerate() {
            let is_type = line.starts_with("message ")
                || line.starts_with("enum ")
                || line.starts_with("oneof ");
            let is_member = line.ends_with(';')
                && line.contains(" = ")
                && !line.starts_with("syntax ")
                && !line.starts_with("//");
            if (is_type || is_member) && (i == 0 || !lines[i - 1].starts_with("//")) {
                undocumented.push(format!("{path}:{}: {line}", i + 1));
            }
        }
    }
    assert!(
        undocumented.is_empty(),
        "declarations without a leading comment:\n{}",
        undocumented.join("\n")
    );
}

// ---------------------------------------------------------------------------------
// buf lint and buf breaking
// ---------------------------------------------------------------------------------

/// Returns whether `buf` can be run. When it cannot, the calling test is skipped with a
/// message, unless `IRONLAB_REQUIRE_BUF` is set, in which case it fails.
fn buf_available(test: &str) -> bool {
    let runs = Command::new("buf")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !runs {
        assert!(
            std::env::var_os("IRONLAB_REQUIRE_BUF").is_none(),
            "IRONLAB_REQUIRE_BUF is set, but `buf --version` cannot be run"
        );
        eprintln!(
            "skipping {test}: buf is not installed (set IRONLAB_REQUIRE_BUF to fail instead)"
        );
    }
    runs
}

/// Writes the given files and the generated `buf.yaml` into a fresh directory named
/// `name` under Cargo's temporary directory for integration tests, as
/// `generate-proto` writes them, and returns the directory.
fn write_buf_module(name: &str, files: &BTreeMap<String, String>) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("ironlab-proto")
        .join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("remove the previous module");
    }
    let buf_yaml = ("buf.yaml".to_owned(), wire::BUF_YAML.to_owned());
    for (path, text) in files.iter().chain([(&buf_yaml.0, &buf_yaml.1)]) {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().expect("files lie in the module"))
            .expect("create the module directories");
        std::fs::write(&path, text).expect("write a module file");
    }
    dir
}

/// Runs `buf` with the given arguments in a module directory.
fn run_buf(dir: &Path, args: &[&str]) -> Output {
    Command::new("buf")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("buf runs")
}

/// Formats the output of a `buf` run for an assertion message.
fn describe(output: &Output) -> String {
    format!(
        "exit status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Runs `buf breaking` on a module holding `files` against a module holding the
/// unaltered generated files.
fn buf_breaking_against_generated(name: &str, files: &BTreeMap<String, String>) -> Output {
    let baseline = write_buf_module(&format!("{name}-baseline"), &generated_files());
    let candidate = write_buf_module(name, files);
    run_buf(
        &candidate,
        &[
            "breaking",
            "--against",
            baseline.to_str().expect("temporary paths are UTF-8"),
        ],
    )
}

// Why: the generated files must satisfy buf's standard lint rules (naming, enum zero
// values, package layout, comments aside), which are the conventions that other
// languages' code generators and reviewers expect; the generated `buf.yaml` excepts
// only the `v0` package version, which is a documented decision.
#[test]
fn generated_files_pass_buf_lint() {
    if !buf_available("generated_files_pass_buf_lint") {
        return;
    }
    let dir = write_buf_module("lint", &generated_files());
    let output = run_buf(&dir, &["lint"]);
    assert!(
        output.status.success(),
        "buf lint failed: {}",
        describe(&output)
    );
}

// Why: CI compares the schema of a pull request with that of the base branch using the
// generated `buf.yaml`, and that comparison is the guard that keeps old `.fig` files and
// deployed clients readable. It must pass for an unchanged schema and fail when a field,
// an enum value or a oneof variant is renumbered, each of which silently changes the
// meaning of existing bytes.
#[test]
fn buf_breaking_detects_renumbering() {
    if !buf_available("buf_breaking_detects_renumbering") {
        return;
    }
    let unchanged = buf_breaking_against_generated("unchanged", &generated_files());
    assert!(
        unchanged.status.success(),
        "an unchanged schema is reported as breaking: {}",
        describe(&unchanged)
    );

    let cases = [
        (
            "renumbered-field",
            "ironlab/ir/v0/figure.proto",
            (
                "optional double width_mm = 1;",
                "optional double width_mm = 3;",
            ),
        ),
        (
            "renumbered-enum-value",
            "ironlab/ir/v0/link.proto",
            ("DIMENSION_Z = 3;", "DIMENSION_Z = 4;"),
        ),
        (
            "renumbered-oneof-variant",
            "ironlab/ir/v0/axes.proto",
            ("LimitsManual manual = 2;", "LimitsManual manual = 3;"),
        ),
    ];
    for (name, path, replacement) in cases {
        let output = buf_breaking_against_generated(name, &mutated(path, &[replacement]));
        assert!(
            !output.status.success(),
            "{name} is not reported as breaking: {}",
            describe(&output)
        );
    }
}

// Why: exchanging the numbers of two fields of the same type (the minimum and maximum
// of a range, or the x and y data of a line) keeps every number and type in the schema,
// so buf's `WIRE` rules accept it, yet every existing file then decodes with the two
// values exchanged. The generated `buf.yaml` must use rules that also compare field
// names (`WIRE_JSON` or stricter; it uses `PACKAGE`, which also keeps generated client
// code compiling) so that CI reports the exchange.
#[test]
fn buf_breaking_detects_exchanged_numbers_of_fields_of_the_same_type() {
    if !buf_available("buf_breaking_detects_exchanged_numbers_of_fields_of_the_same_type") {
        return;
    }
    let files = mutated(
        "ironlab/ir/v0/axes.proto",
        &[
            ("optional double min = 1;", "optional double min = 2;"),
            ("optional double max = 2;", "optional double max = 1;"),
        ],
    );
    let output = buf_breaking_against_generated("exchanged-min-max", &files);
    assert!(
        !output.status.success(),
        "exchanging the numbers of LimitsManual.min and max is not reported as breaking: {}",
        describe(&output)
    );
}

// ---------------------------------------------------------------------------------
// Conventions
// ---------------------------------------------------------------------------------

// Why: readers check compatibility by decoding field 1 alone, in every version of the
// schema, so it must remain the string `schema_version`.
#[test]
fn field_one_of_figure_is_the_schema_version_string() {
    let pool = compile();
    let field = figure_descriptor(&pool)
        .get_field(1)
        .expect("Figure has field 1");
    assert_eq!(field.name(), "schema_version");
    assert_eq!(field.kind(), Kind::String);
    assert!(!field.is_list());
}

// Why: proto3 decodes an absent enum as its zero value, so the zero value must mean
// "unspecified" rather than a real choice, and enum values share the package's
// namespace, so each must carry its enum's name (as the buf STANDARD rules require).
#[test]
fn every_enum_has_an_unspecified_zero_value_and_prefixed_values() {
    let pool = compile();
    let enums: Vec<_> = pool.all_enums().collect();
    assert!(!enums.is_empty());
    for descriptor in enums {
        let prefix = upper_snake(descriptor.name());
        let zero = descriptor
            .get_value(0)
            .unwrap_or_else(|| panic!("{} has no zero value", descriptor.name()));
        assert_eq!(zero.name(), format!("{prefix}_UNSPECIFIED"));
        for value in descriptor.values() {
            assert!(
                value.name().starts_with(&format!("{prefix}_")),
                "{} is not prefixed with {prefix}_",
                value.name()
            );
        }
    }
}

// Why: singular numeric and boolean fields must have explicit presence, so that a
// written zero (or negative zero) is distinguished from an absent value and reloads as
// itself; colour components are the documented exception.
#[test]
fn singular_numeric_and_boolean_fields_have_presence_except_colour_components() {
    let pool = compile();
    for message in pool.all_messages() {
        if message.is_map_entry() || message.name() == "Color" {
            continue;
        }
        for field in message.fields() {
            // Every scalar kind other than a string or bytes is numeric or boolean.
            let numeric_or_bool = !matches!(
                field.kind(),
                Kind::String | Kind::Bytes | Kind::Message(_) | Kind::Enum(_)
            );
            if numeric_or_bool && !field.is_list() {
                assert!(
                    field.supports_presence(),
                    "{} has no presence",
                    field.full_name()
                );
            }
        }
    }
}

// Why: readers in other languages do not know the IR's context-dependent defaults, so
// IronLAB must write every value explicitly: every enum specified, every oneof set, and
// every field with presence set unless the IR value is itself absent (an optional title,
// label, legend, display name or coordinate array).
#[test]
fn the_encoder_writes_every_value_explicitly() {
    const OPTIONAL_IN_IR: [&str; 15] = [
        "ironlab.ir.v0.Figure.title",
        "ironlab.ir.v0.Axes.title",
        "ironlab.ir.v0.Axes.legend",
        "ironlab.ir.v0.Axis.label",
        "ironlab.ir.v0.Line.display_name",
        "ironlab.ir.v0.Line.z",
        "ironlab.ir.v0.Scatter.display_name",
        "ironlab.ir.v0.Scatter.z",
        "ironlab.ir.v0.Contour.display_name",
        "ironlab.ir.v0.ContourPlacementPlane.z",
        "ironlab.ir.v0.Quiver.display_name",
        "ironlab.ir.v0.Quiver.z",
        "ironlab.ir.v0.Quiver.w",
        "ironlab.ir.v0.Surface.display_name",
        "ironlab.ir.v0.Surface.c",
    ];

    fn check(message: &DynamicMessage, path: &str, problems: &mut Vec<String>) {
        let descriptor = message.descriptor();
        for oneof in descriptor.oneofs().filter(|o| !o.is_synthetic()) {
            if !oneof.fields().any(|f| message.has_field(&f)) {
                problems.push(format!("{path}: oneof {} is unset", oneof.name()));
            }
        }
        for field in descriptor.fields() {
            let name = field.full_name();
            let optional = OPTIONAL_IN_IR.contains(&name);
            let in_oneof = field.containing_oneof().is_some_and(|o| !o.is_synthetic());
            if field.supports_presence() && !in_oneof && !optional && !message.has_field(&field) {
                problems.push(format!("{path}.{}: absent", field.name()));
            }
            if let Kind::Enum(_) = field.kind()
                && let Value::EnumNumber(0) = *message.get_field(&field)
            {
                problems.push(format!("{path}.{}: unspecified", field.name()));
            }
            match &*message.get_field(&field) {
                Value::Message(inner) if message.has_field(&field) => {
                    check(inner, &format!("{path}.{}", field.name()), problems);
                }
                Value::List(items) => {
                    for (i, item) in items.iter().enumerate() {
                        if let Value::Message(inner) = item {
                            check(inner, &format!("{path}.{}[{i}]", field.name()), problems);
                        }
                    }
                }
                Value::Map(entries) => {
                    for (key, item) in entries {
                        if let Value::Message(inner) = item {
                            check(
                                inner,
                                &format!("{path}.{}[{key:?}]", field.name()),
                                problems,
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let pool = compile();
    let bytes = kitchen_sink_figure().to_protobuf();
    let message = DynamicMessage::decode(figure_descriptor(&pool), bytes.as_slice())
        .expect("the generated schema decodes the encoder's output");
    let mut problems = Vec::new();
    check(&message, "Figure", &mut problems);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

// ---------------------------------------------------------------------------------
// Agreement between the generated schema and the encoder
// ---------------------------------------------------------------------------------

/// Returns the paths of every unknown field in a dynamic message and its descendants.
fn unknown_field_paths(message: &DynamicMessage, path: &str) -> Vec<String> {
    let mut paths: Vec<String> = message
        .unknown_fields()
        .map(|field| format!("{path}#{}", field.number()))
        .collect();
    for (field, value) in message.fields() {
        let at = format!("{path}.{}", field.name());
        match value {
            Value::Message(inner) => paths.extend(unknown_field_paths(inner, &at)),
            Value::List(items) => {
                for (i, item) in items.iter().enumerate() {
                    if let Value::Message(inner) = item {
                        paths.extend(unknown_field_paths(inner, &format!("{at}[{i}]")));
                    }
                }
            }
            Value::Map(entries) => {
                for (key, item) in entries {
                    if let Value::Message(inner) = item {
                        paths.extend(unknown_field_paths(inner, &format!("{at}[{key:?}]")));
                    }
                }
            }
            _ => {}
        }
    }
    paths
}

/// Decodes bytes with the generated schema through reflection, requires that the
/// schema accounts for every field, and re-encodes the dynamic message.
fn transcode_through_generated_schema(bytes: &[u8]) -> Vec<u8> {
    let pool = compile();
    let message = DynamicMessage::decode(figure_descriptor(&pool), bytes)
        .expect("the generated schema decodes the encoder's output");
    let unknown = unknown_field_paths(&message, "Figure");
    assert!(
        unknown.is_empty(),
        "fields written by the encoder but absent from the generated schema: {unknown:?}"
    );
    message.encode_to_vec()
}

// Why: the generated schema and the Rust encoder must describe the same wire format, so
// that a program in another language, using only the `.proto` files, reads and writes
// the figures that IronLAB writes and reads. A generic protobuf implementation decodes
// IronLAB's bytes with the generated descriptors (finding no field that the schema does
// not declare), re-encodes them in its own way, and IronLAB must read back the same
// figure.
#[test]
fn a_generic_implementation_using_the_generated_schema_round_trips_a_figure() {
    let original = kitchen_sink_figure();
    let transcoded = transcode_through_generated_schema(&original.to_protobuf());
    assert_eq!(Figure::from_protobuf(&transcoded).unwrap(), original);
}

// Why: the agreement must extend to the bits of every float, so that the generated
// schema declares each float field with the same width and presence as the encoder (a
// `float` declared where the encoder writes a `double`, or a field without presence,
// would lose bits in another language).
#[test]
fn a_generic_implementation_using_the_generated_schema_preserves_float_bits() {
    let original = special_values_figure();
    let transcoded = transcode_through_generated_schema(&original.to_protobuf());
    let restored = Figure::from_protobuf(&transcoded).unwrap();
    assert_eq!(float_bits(&restored), float_bits(&original));
}
