//! Colours and their hexadecimal representation.

use ironlab_ir::*;
use proptest::prelude::*;
use serde_json::json;

// Why: `#rrggbb` is the documented colour format, and each byte maps to a component in
// the range 0 to 1 with full opacity implied.
#[test]
fn six_digit_hex_parses_as_an_opaque_colour() {
    let c = Colour::from_hex("#ff8000").unwrap();
    assert_eq!(c, Colour::rgba(1.0, 128.0 / 255.0, 0.0, 1.0));
}

// Why: `#rrggbbaa` carries opacity as the fourth byte, used for translucent fills.
#[test]
fn eight_digit_hex_parses_with_alpha() {
    let c = Colour::from_hex("#0072b280").unwrap();
    assert_eq!(
        c,
        Colour::rgba(0.0, 114.0 / 255.0, 178.0 / 255.0, 128.0 / 255.0)
    );
}

// Why: colours copied from other tools are often upper case; both cases name the same
// colour.
#[test]
fn hex_digits_are_case_insensitive() {
    assert_eq!(
        Colour::from_hex("#ABCDEF").unwrap(),
        Colour::from_hex("#abcdef").unwrap()
    );
}

// Why: output must be canonical (lower case, alpha omitted when opaque) so that files
// are stable and diffable.
#[test]
fn hex_output_is_lower_case_and_omits_opaque_alpha() {
    assert_eq!(Colour::rgb(1.0, 128.0 / 255.0, 0.0).to_hex(), "#ff8000");
    assert_eq!(
        Colour::rgba(1.0, 1.0, 1.0, 128.0 / 255.0).to_hex(),
        "#ffffff80"
    );
    assert_eq!(Colour::rgba(0.0, 0.0, 0.0, 0.0).to_hex(), "#00000000");
    assert_eq!(Colour::BLACK.to_hex(), "#000000");
    assert_eq!(Colour::WHITE.to_hex(), "#ffffff");
}

// Why: components between the 256 representable levels must round to the nearest level
// rather than truncate, and out-of-range components must not wrap around. Exact ties
// (such as 0.5, which is 127.5 levels) are avoided because the tie-breaking rule is not
// part of the contract.
#[test]
fn hex_output_rounds_and_clamps_components() {
    assert_eq!(Colour::rgb(0.25, 0.999, 0.001).to_hex(), "#40ff00");
    assert_eq!(Colour::rgb(1.5, -0.5, 0.0).to_hex(), "#ff0000");
}

// Why: a malformed colour in a file must be an error, not a silently wrong colour.
#[test]
fn malformed_hex_strings_are_rejected() {
    for bad in [
        "",
        "#",
        "ff8000",
        "#fff",
        "#ff80",
        "#ff800",
        "#ff80001",
        "#ff8000ff0",
        "#gg0000",
        "# ff800",
        "#ff8000 ",
        "red",
    ] {
        assert!(
            matches!(Colour::from_hex(bad), Err(IrError::InvalidColour(ref s)) if s == bad),
            "{bad:?} was not rejected as an invalid colour"
        );
    }
}

// Why: colours are embedded in figure JSON as strings, and a malformed string there must
// fail deserialisation.
#[test]
fn colours_serialise_as_hex_strings() {
    assert_eq!(
        serde_json::to_value(Colour::WHITE).unwrap(),
        json!("#ffffff")
    );
    let c: Colour = serde_json::from_value(json!("#00000080")).unwrap();
    assert_eq!(c, Colour::rgba(0.0, 0.0, 0.0, 128.0 / 255.0));
    assert!(serde_json::from_value::<Colour>(json!("#nothex")).is_err());
    assert!(serde_json::from_value::<Colour>(json!([0, 0, 0])).is_err());
}

proptest! {
    // Why: every 8-bit colour must survive formatting and parsing unchanged, so that
    // colours in saved figures never drift across repeated saves.
    #[test]
    fn every_eight_bit_colour_round_trips(r: u8, g: u8, b: u8, a: u8) {
        let hex = if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        };
        let colour = Colour::from_hex(&hex).unwrap();
        prop_assert_eq!(colour.to_hex(), hex);
    }
}
