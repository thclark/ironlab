//! Splitting label source into plain, monospace and math segments.

use crate::TextWarning;

/// One piece of label source, before shaping or typesetting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Segment {
    /// Text to shape verbatim, with escapes already resolved.
    Plain(String),
    /// LaTeX math source found between a pair of unescaped `$` delimiters,
    /// excluding the delimiters.
    Math(String),
    /// Text to set in monospace, from the argument of `\texttt{…}` outside math,
    /// with escapes resolved and grouping braces removed.
    Mono(String),
}

/// The command, with its opening brace, that sets its argument in monospace outside math.
const TEXTTT: &str = r"\texttt{";

/// Splits `content` into plain, monospace and math segments.
///
/// Outside math, `\$` produces a literal dollar sign, `\texttt{…}` sets its argument
/// in monospace, and every other character, including a lone backslash, is kept
/// verbatim. An unescaped `$` opens math, which extends to the next `$` that is not
/// preceded by a backslash command (so `\$` inside math does not close it). A
/// `$` with no closing partner is kept as a literal character and reported in
/// the returned warnings. Adjacent plain text is merged into one segment, and
/// no segment is empty except math between two adjacent delimiters.
///
/// The argument of `\texttt` ends at the brace that balances its opening brace;
/// braces nested inside it group, as in LaTeX, and are not printed. Within it, `\_`,
/// `\{`, `\}`, `\$`, `\&`, `\%`, `\#` and `\ ` (a backslash and a space) produce the
/// character after the backslash; a bare `_` is accepted as a literal underscore,
/// which LaTeX itself rejects outside math, so that identifiers can be written
/// unchanged; a `$` is a literal dollar sign rather than the start of math; and any
/// other backslash is kept verbatim together with the character after it. A
/// `\texttt{` whose argument is never closed is kept as literal text and reported in
/// the returned warnings, and the text after it is split as usual.
pub(crate) fn split(content: &str) -> (Vec<Segment>, Vec<TextWarning>) {
    let bytes = content.as_bytes();
    let mut segments = Vec::new();
    let mut warnings = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && bytes.get(i + 1) == Some(&b'$') {
            plain.push('$');
            i += 2;
        } else if content[i..].starts_with(TEXTTT) {
            if let Some((text, consumed)) = monospace_argument(&content[i + TEXTTT.len()..]) {
                if !plain.is_empty() {
                    segments.push(Segment::Plain(std::mem::take(&mut plain)));
                }
                segments.push(Segment::Mono(text));
                i += TEXTTT.len() + consumed;
            } else {
                warnings.push(TextWarning {
                    source: content[i..].to_owned(),
                    message: r"This \texttt has no closing brace, so it is shown literally."
                        .to_owned(),
                });
                plain.push_str(TEXTTT);
                i += TEXTTT.len();
            }
        } else if bytes[i] == b'$' {
            if let Some(close) = find_closing_dollar(bytes, i + 1) {
                if !plain.is_empty() {
                    segments.push(Segment::Plain(std::mem::take(&mut plain)));
                }
                segments.push(Segment::Math(content[i + 1..close].to_owned()));
                i = close + 1;
            } else {
                warnings.push(TextWarning {
                    source: content[i..].to_owned(),
                    message: "This dollar sign has no closing partner, so it is shown literally; \
                              write \\$ for a literal dollar sign."
                        .to_owned(),
                });
                plain.push('$');
                i += 1;
            }
        } else {
            // Indices only ever stop on ASCII bytes or character starts, so
            // this slice begins on a character boundary.
            let ch = content[i..].chars().next().unwrap_or('\u{FFFD}');
            plain.push(ch);
            i += ch.len_utf8().max(1);
        }
    }
    if !plain.is_empty() {
        segments.push(Segment::Plain(plain));
    }
    (segments, warnings)
}

/// Reads the argument of `\texttt{` from `rest`, the source after its opening brace.
///
/// Returns the text to set, with escapes resolved and grouping braces removed, and the
/// number of bytes consumed including the closing brace, or `None` when no brace closes
/// the argument.
fn monospace_argument(rest: &str) -> Option<(String, usize)> {
    let mut text = String::new();
    let mut depth = 1usize;
    let mut chars = rest.char_indices();
    while let Some((at, ch)) = chars.next() {
        match ch {
            '\\' => match chars.next() {
                Some((_, escaped @ ('_' | '{' | '}' | '$' | '&' | '%' | '#' | ' '))) => {
                    text.push(escaped);
                }
                Some((_, other)) => {
                    text.push('\\');
                    text.push(other);
                }
                None => text.push('\\'),
            },
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((text, at + 1));
                }
            }
            other => text.push(other),
        }
    }
    None
}

/// Returns the byte index of the `$` that closes math opened just before
/// `start`, skipping any character escaped by a backslash.
///
/// The scan compares ASCII bytes only; UTF-8 continuation bytes never equal
/// `\` or `$`, so stepping over a partial character after a backslash is
/// harmless.
fn find_closing_dollar(bytes: &[u8], start: usize) -> Option<usize> {
    let mut j = start;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 2,
            b'$' => return Some(j),
            _ => j += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_plain_and_math_in_order() {
        let (segments, warnings) = split("a $x$ b");
        assert!(warnings.is_empty());
        assert_eq!(
            segments,
            [
                Segment::Plain("a ".into()),
                Segment::Math("x".into()),
                Segment::Plain(" b".into()),
            ]
        );
    }

    #[test]
    fn escaped_dollar_inside_math_does_not_close_it() {
        let (segments, warnings) = split(r"$a\$b$");
        assert!(warnings.is_empty());
        assert_eq!(segments, [Segment::Math(r"a\$b".into())]);
    }

    #[test]
    fn unmatched_dollar_is_literal() {
        let (segments, warnings) = split("€ ($)");
        assert_eq!(warnings.len(), 1);
        assert_eq!(segments, [Segment::Plain("€ ($)".into())]);
    }
}
