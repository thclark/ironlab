//! The Protocol Buffers wire format of the figure IR, used by `.fig` files.
//!
//! The domain types of this crate are the source of truth for the figure model. This
//! module defines a second set of Rust types that mirror the domain types in the shape
//! of Protocol Buffers messages, derive their encoding with `prost`, and convert
//! losslessly to and from the domain types. The `.proto` files that describe the wire
//! format to other languages are generated from the same definitions by
//! [`proto_files`](crate::proto_files); they are an output of the build, never an
//! input to it.
//!
//! # A single definition
//!
//! Every wire type is declared once, with the `proto_file!` macro, which expands the
//! declaration into both the Rust type (with its `prost` attributes) and a description
//! from which the type's `.proto` text is rendered. The field numbers, labels and types
//! of the Rust type and of the generated schema therefore cannot diverge. The macro's
//! grammar has no form without a number, so every field, oneof variant and enum value
//! has an explicit number; the `prost` derive macros alone would infer a missing number
//! from the position of a field, so that reordering fields would silently change the
//! wire format.
//!
//! A number or name used twice in one message or enum (including a number or name that
//! the message or enum reserves) is rejected when the files are rendered, with a panic
//! that names the declaration. The `prost` derive macros and the Rust compiler reject
//! some of these duplicates earlier, but the Protocol Buffers names of oneof variants
//! and reserved declarations are not Rust identifiers, so only the rendering sees them.
//!
//! # Conventions
//!
//! - The package is `ironlab.ir.v0`, with one file per module of this crate, at
//!   `ironlab/ir/v0/<module>.proto`. Imports between the files are derived from the
//!   types that each file refers to.
//! - Every enum has the zero value `<ENUM>_UNSPECIFIED`, and every value is prefixed
//!   with the enum's name in upper snake case.
//! - Singular numeric and boolean fields are `optional`, so that an explicitly written
//!   value is distinguished from an absent one and every value, including a negative
//!   zero, reloads bit for bit. The four `float` components of a colour are the only
//!   exception, because a colour component has no meaningful negative zero.
//! - A tagged domain enum is a message with a single oneof named `kind`, whose
//!   variants are messages named after the enum and the variant, so that a variant can
//!   gain fields in a later version without changing the others. This holds even where
//!   a variant holds a single scalar, as in `ScatterSizeScalar` and in the variants of
//!   a figure `Parameter` (such as `ParameterNumber`, whose field `value` holds the
//!   number), so that every tagged entity is read in the same way.
//! - No field, oneof or oneof variant is named with a keyword of a language for which
//!   code is commonly generated from `.proto` files (such as `auto` and `explicit` in
//!   C++), so that generated code needs no renamed accessors. The Protocol Buffers
//!   name of such a variant therefore differs from its domain name: the automatic
//!   variants are named `automatic`, the explicit levels `explicit_values`, and the
//!   variants of a parameter `bool_value`, `integer_value`, `number_value` and
//!   `string_value` (because `bool` and `string` are keywords of C++ and C#; the
//!   suffix is used on all four variants so that their names are uniform).
//! - A field number or name that is removed is reserved, with `reserved`, so that it
//!   is never reused with another meaning.
//! - A numeric array is a `repeated uint64 shape`, an `NdArrayElement element` that
//!   names the type of its values, and one of two payloads: a packed `repeated double
//!   values` for 64-bit floating-point values, in which NaN is stored natively, or a
//!   `bytes u8_values` for 8-bit unsigned integers, one byte per value. The encoder
//!   writes the element of every array and leaves the other payload empty. The decoder
//!   takes an unspecified element as floating-point values, which is what every file
//!   written before the element existed holds, and rejects an array whose payloads
//!   disagree with its element (values in the other payload, or in both) as
//!   [`ProtobufError::InvalidValue`](crate::ProtobufError::InvalidValue).
//! - The data table of a figure is a `map<uint64, NdArray>`, encoded in ascending key
//!   order so that a figure always encodes to the same bytes. (Figures that compare
//!   equal may still encode differently, because the equality of figures does not
//!   distinguish negative from positive zero or one NaN from another.)
//! - The parameters of a figure are a `map<string, Parameter>`, encoded in ascending
//!   order of the UTF-8 bytes of the name, for the same reason.
//! - Field 1 of `Figure` is `schema_version`. It never changes, so that any reader can
//!   check the version of a file before decoding the rest of it.
//!
//! # Absent and unknown values
//!
//! The encoder writes every field that has a value, including every enum value and
//! oneof variant, so decoding a file written by IronLAB never relies on defaults.
//!
//! When decoding, an absent message, an absent field with presence, an unset oneof and
//! an `_UNSPECIFIED` enum value take the value that the domain type's [`Default`] gives
//! them in their context. For example, an absent line colour of a contour is
//! colormapped, as in [`Contour::default`](crate::Contour::default), whereas an absent
//! line colour of a line is automatic.
//!
//! Absence is an error, reported as
//! [`ProtobufError::MissingField`](crate::ProtobufError::MissingField), where the
//! domain has no meaningful default:
//!
//! - the kind of an artist, the dimension of an axis link, and the kind of a figure
//!   parameter together with its value when it is a boolean, an integer or a number;
//! - every identifier that refers to a node or a data array and is not optional in the
//!   domain: the identifier of the figure, of each axes and of each artist, the x and y
//!   data of a line, scatter or quiver, the u and v data of a quiver, the z data of a
//!   contour or surface, the grid of a contour or surface (its kind and both of its
//!   coordinate arrays), and the data of per-point scatter sizes and colours. Zero is a
//!   valid identifier, so substituting it would silently attach an artist to the wrong
//!   data;
//! - the bounds of manual limits and the factor of a quiver scale;
//! - in a transaction (`edit.proto`), the kind of an edit, of a node and of a value, the
//!   identifiers that an edit names, the value of a set, the node of an insertion, the
//!   array of a data edit, and the value held by a value message (an unspecified enum
//!   value included), because an edit has no context from which a default could be
//!   taken. The path of a set is a string without presence, so an absent path is the
//!   empty path, which is reported as
//!   [`ProtobufError::InvalidValue`](crate::ProtobufError::InvalidValue) like any other
//!   path that is not valid.
//!
//! Optional references (the z data of a line, scatter or quiver, the w data of a
//! quiver and the colour data of a surface) are absent in the domain when they are
//! absent on the wire. An absent [`Provenance`](crate::Provenance) decodes as an empty
//! provenance (empty strings and no fonts) rather than as the provenance of this build,
//! which did not write the file.
//!
//! Strings, bytes, repeated fields, maps and colour components have no presence, so an
//! empty or zero value cannot be told apart from an absent one. They decode as the
//! value on the wire: an empty string is empty and an empty list is empty, even where
//! the domain's default is not (such as the fonts of a present
//! [`Provenance`](crate::Provenance)).
//!
//! An enum value that this build does not define is an error rather than a default.
//! New enum values require a new minor schema version, and files of another minor
//! version are rejected before they are decoded, so an unknown value in a compatible
//! file can only come from a corrupt file or a faulty writer.

/// The Rust type of a Protocol Buffers scalar type.
macro_rules! wire_scalar {
    (double) => {
        f64
    };
    (float) => {
        f32
    };
    (uint32) => {
        u32
    };
    (uint64) => {
        u64
    };
    (int64) => {
        i64
    };
    (bool) => {
        bool
    };
    (string) => {
        ::std::string::String
    };
    (bytes) => {
        ::std::vec::Vec<u8>
    };
}

/// Declares the wire types of one `.proto` file, which is named after the Rust module
/// that invokes the macro.
///
/// Each item is a `message` or an `enum`, preceded by its documentation. A message
/// field has one of the following forms, each preceded by its documentation:
///
/// - `string name = N;`, `bytes name = N;` and `float name = N;`, a scalar without
///   presence;
/// - `optional double name = N;`, a scalar with presence (any scalar type);
/// - `repeated double name = N;`, a repeated scalar, packed when numeric;
/// - `message Type name = N;`, a singular message, absent when `None`;
/// - `repeated message Type name = N;`, a repeated message;
/// - `enum Type name = N;`, an enum, stored as `i32` so that unknown values are kept;
/// - `map<uint64, message Type> name = N;`, a map from a scalar (here `uint64`) to a
///   message;
/// - `oneof name: RustEnum { Variant(Type) field = N; ... }`, a oneof of messages.
///
/// An enum lists `Variant = N;` values; the zero value `Unspecified` is added by the
/// macro.
///
/// A message or an enum may also contain, anywhere among its members and optionally
/// documented, `reserved 4, 7, old_name;` statements. A literal reserves a number and
/// an identifier reserves a name: the name of a field in a message, and the Rust name
/// of a value in an enum, which is rendered with the enum's prefix in upper snake case.
macro_rules! proto_file {
    (
        $(
            $(#[doc = $doc:literal])*
            $kind:ident $name:ident { $($body:tt)* }
        )*
    ) => {
        // The types of every file share one namespace, as in the package.
        #[allow(unused_imports)]
        use super::*;

        $(
            proto_file!(@item [$($doc)*] $kind $name { $($body)* });
        )*

        /// The definition of this module's `.proto` file.
        pub(crate) const FILE: $crate::wire::schema::FileDef = $crate::wire::schema::FileDef {
            module: module_path!(),
            items: &[$(<$name as $crate::wire::schema::ProtoItem>::DEF),*],
        };
    };

    (@item [$($doc:literal)*] enum $name:ident { $($body:tt)* }) => {
        proto_file!(@values [$($doc)*] $name [] [] $($body)*);
    };

    // All values have been normalised: emit the enum.
    (@values [$($doc:literal)*] $name:ident
        [$( { [$($vdoc:literal)*] $variant:ident = $number:tt } )*]
        [$($reserved:expr),*]
    ) => {
        $(#[doc = $doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
        #[repr(i32)]
        pub enum $name {
            /// The value is absent; it decodes to the default of its context.
            Unspecified = 0,
            $( $(#[doc = $vdoc])* $variant = $number, )*
        }

        impl $crate::wire::schema::ProtoItem for $name {
            const DEF: $crate::wire::schema::ItemDef =
                $crate::wire::schema::ItemDef::Enum($crate::wire::schema::EnumDef {
                    name: stringify!($name),
                    docs: &[$($doc),*],
                    values: &[$(
                        $crate::wire::schema::ValueDef {
                            name: stringify!($variant),
                            docs: &[$($vdoc),*],
                            number: $number,
                        }
                    ),*],
                    reserved: &[$($reserved),*],
                });
        }
    };

    (@values $docs:tt $name:ident $values:tt [$($reserved:expr),*]
        $(#[doc = $rdoc:literal])* reserved $($item:tt),+ ; $($rest:tt)*
    ) => {
        proto_file!(@values $docs $name $values [$($reserved,)* $crate::wire::schema::ReservedDef {
            docs: &[$($rdoc),*],
            items: &[$(proto_file!(@reserved $item)),+],
        }] $($rest)*);
    };

    (@values $docs:tt $name:ident [$($values:tt)*] $reserved:tt
        $(#[doc = $vdoc:literal])* $variant:ident = $number:tt ; $($rest:tt)*
    ) => {
        proto_file!(@values $docs $name [$($values)* {
            [$($vdoc)*] $variant = $number
        }] $reserved $($rest)*);
    };

    (@reserved $number:literal) => {
        $crate::wire::schema::ReservedItem::Number($number)
    };

    (@reserved $name:ident) => {
        $crate::wire::schema::ReservedItem::Name(stringify!($name))
    };

    (@item [$($doc:literal)*] message $name:ident { $($body:tt)* }) => {
        proto_file!(@fields [$($doc)*] $name [] [] $($body)*);
    };

    // All fields have been normalised: emit the message.
    (@fields [$($doc:literal)*] $name:ident [$(
        { [$($fdoc:literal)*] $field:ident : $rty:ty ; $attr:tt ; $def:expr }
    )*] [$($reserved:expr),*]) => {
        $(#[doc = $doc])*
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct $name {
            $(
                $(#[doc = $fdoc])*
                #[prost $attr]
                pub $field: $rty,
            )*
        }

        impl $crate::wire::schema::ProtoItem for $name {
            const DEF: $crate::wire::schema::ItemDef =
                $crate::wire::schema::ItemDef::Message($crate::wire::schema::MessageDef {
                    name: stringify!($name),
                    docs: &[$($doc),*],
                    fields: &[$($def),*],
                    reserved: &[$($reserved),*],
                });
        }
    };

    (@fields $docs:tt $name:ident $acc:tt [$($reserved:expr),*]
        $(#[doc = $rdoc:literal])* reserved $($item:tt),+ ; $($rest:tt)*
    ) => {
        proto_file!(@fields $docs $name $acc [$($reserved,)* $crate::wire::schema::ReservedDef {
            docs: &[$($rdoc),*],
            items: &[$(proto_file!(@reserved $item)),+],
        }] $($rest)*);
    };

    (@fields $docs:tt $name:ident [$($acc:tt)*] $reserved:tt
        $(#[doc = $fdoc:literal])* optional $scalar:ident $field:ident = $tag:tt; $($rest:tt)*
    ) => {
        proto_file!(@fields $docs $name [$($acc)* {
            [$($fdoc)*] $field : ::core::option::Option<wire_scalar!($scalar)> ;
            ($scalar, optional, tag = $tag) ;
            proto_file!(@def [$($fdoc)*] Optional Scalar(stringify!($scalar)) $field $tag)
        }] $reserved $($rest)*);
    };

    (@fields $docs:tt $name:ident [$($acc:tt)*] $reserved:tt
        $(#[doc = $fdoc:literal])* repeated message $ty:ident $field:ident = $tag:tt; $($rest:tt)*
    ) => {
        proto_file!(@fields $docs $name [$($acc)* {
            [$($fdoc)*] $field : ::std::vec::Vec<$ty> ;
            (message, repeated, tag = $tag) ;
            proto_file!(@def [$($fdoc)*] Repeated Named(stringify!($ty)) $field $tag)
        }] $reserved $($rest)*);
    };

    (@fields $docs:tt $name:ident [$($acc:tt)*] $reserved:tt
        $(#[doc = $fdoc:literal])* repeated $scalar:ident $field:ident = $tag:tt; $($rest:tt)*
    ) => {
        proto_file!(@fields $docs $name [$($acc)* {
            [$($fdoc)*] $field : ::std::vec::Vec<wire_scalar!($scalar)> ;
            ($scalar, repeated, tag = $tag) ;
            proto_file!(@def [$($fdoc)*] Repeated Scalar(stringify!($scalar)) $field $tag)
        }] $reserved $($rest)*);
    };

    (@fields $docs:tt $name:ident [$($acc:tt)*] $reserved:tt
        $(#[doc = $fdoc:literal])* message $ty:ident $field:ident = $tag:tt; $($rest:tt)*
    ) => {
        proto_file!(@fields $docs $name [$($acc)* {
            [$($fdoc)*] $field : ::core::option::Option<$ty> ;
            (message, optional, tag = $tag) ;
            proto_file!(@def [$($fdoc)*] Singular Named(stringify!($ty)) $field $tag)
        }] $reserved $($rest)*);
    };

    (@fields $docs:tt $name:ident [$($acc:tt)*] $reserved:tt
        $(#[doc = $fdoc:literal])* enum $ty:ident $field:ident = $tag:tt; $($rest:tt)*
    ) => {
        proto_file!(@fields $docs $name [$($acc)* {
            [$($fdoc)*] $field : i32 ;
            (enumeration($ty), tag = $tag) ;
            proto_file!(@def [$($fdoc)*] Singular Named(stringify!($ty)) $field $tag)
        }] $reserved $($rest)*);
    };

    (@fields $docs:tt $name:ident [$($acc:tt)*] $reserved:tt
        $(#[doc = $fdoc:literal])* map<$key:ident, message $ty:ident> $field:ident = $tag:tt;
        $($rest:tt)*
    ) => {
        proto_file!(@fields $docs $name [$($acc)* {
            [$($fdoc)*] $field : ::std::collections::BTreeMap<wire_scalar!($key), $ty> ;
            (btree_map($key, message), tag = $tag) ;
            proto_file!(@def [$($fdoc)*] Singular Map(stringify!($key), stringify!($ty)) $field $tag)
        }] $reserved $($rest)*);
    };

    (@fields $docs:tt $name:ident [$($acc:tt)*] $reserved:tt
        $(#[doc = $fdoc:literal])* oneof $field:ident : $enum:ident {
            $( $(#[doc = $vdoc:literal])* $variant:ident($vty:ident) $vfield:ident = $vtag:tt; )*
        }
        $($rest:tt)*
    ) => {
        #[doc = concat!(
            "The variants of the `", stringify!($field), "` oneof of [`", stringify!($name), "`]."
        )]
        #[derive(Clone, PartialEq, ::prost::Oneof)]
        pub enum $enum {
            $(
                $(#[doc = $vdoc])*
                #[prost(message, tag = $vtag)]
                $variant($vty),
            )*
        }

        proto_file!(@fields $docs $name [$($acc)* {
            [$($fdoc)*] $field : ::core::option::Option<$enum> ;
            (oneof($enum), tags($($vtag),*)) ;
            $crate::wire::schema::FieldDef::Oneof {
                name: stringify!($field),
                docs: &[$($fdoc),*],
                variants: &[$(
                    proto_file!(@spec [$($vdoc)*] Singular Named(stringify!($vty)) $vfield $vtag)
                ),*],
            }
        }] $reserved $($rest)*);
    };

    (@fields $docs:tt $name:ident [$($acc:tt)*] $reserved:tt
        $(#[doc = $fdoc:literal])* $scalar:ident $field:ident = $tag:tt; $($rest:tt)*
    ) => {
        proto_file!(@fields $docs $name [$($acc)* {
            [$($fdoc)*] $field : wire_scalar!($scalar) ;
            ($scalar, tag = $tag) ;
            proto_file!(@def [$($fdoc)*] Singular Scalar(stringify!($scalar)) $field $tag)
        }] $reserved $($rest)*);
    };

    (@def [$($fdoc:literal)*] $label:ident $ty:ident ($($tyarg:expr),*) $field:ident $tag:tt) => {
        $crate::wire::schema::FieldDef::Field(
            proto_file!(@spec [$($fdoc)*] $label $ty ($($tyarg),*) $field $tag)
        )
    };

    (@spec [$($fdoc:literal)*] $label:ident $ty:ident ($($tyarg:expr),*) $field:ident $tag:tt) => {
        $crate::wire::schema::FieldSpec {
            docs: &[$($fdoc),*],
            label: $crate::wire::schema::Label::$label,
            ty: $crate::wire::schema::TypeRef::$ty($($tyarg),*),
            name: stringify!($field),
            number: $tag,
        }
    };
}

// The modules are declared after the macros, which are in scope only below their
// definitions.
mod convert;
pub(crate) mod schema;

mod artist;
mod axes;
mod data;
mod edit;
mod figure;
mod link;
mod style;
mod text;

pub use artist::*;
pub use axes::*;
pub use data::*;
pub use edit::*;
pub use figure::*;
pub use link::*;
pub use style::*;
pub use text::*;

/// The Protocol Buffers package of the figure format.
pub const PACKAGE: &str = "ironlab.ir.v0";

/// The directory of the package's `.proto` files, relative to the root of a proto
/// source tree.
pub const PACKAGE_DIR: &str = "ironlab/ir/v0";

/// The configuration of a `buf` module rooted at the directory into which the
/// `.proto` files are generated, which lints them with the standard rules and checks
/// changes for compatibility.
///
/// Changes are checked with buf's `PACKAGE` rules rather than only its `WIRE` rules,
/// so that code generated from an earlier version of the files (in C++, Python,
/// MATLAB or any other language) keeps compiling against a later version, and so that
/// exchanging the numbers of two fields of the same type, which keeps the wire format
/// valid but changes the meaning of every existing file, is reported.
///
/// The standard rule `PACKAGE_VERSION_SUFFIX` is excepted because it accepts only
/// versions from `v1` upwards (including `v1alpha1`), whereas the package is `v0`
/// until the format is declared stable.
pub const BUF_YAML: &str = "\
# Generated by ironlab-ir; do not edit.
version: v2
modules:
  - path: .
lint:
  use:
    - STANDARD
  except:
    # buf accepts only versions from v1 upwards; the package is v0 until the format
    # is declared stable.
    - PACKAGE_VERSION_SUFFIX
breaking:
  use:
    - PACKAGE
";

/// The `.proto` file of every module, in the order in which they are generated.
pub(crate) const FILES: &[schema::FileDef] = &[
    figure::FILE,
    axes::FILE,
    artist::FILE,
    style::FILE,
    text::FILE,
    data::FILE,
    link::FILE,
    edit::FILE,
];
