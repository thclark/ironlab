//! Text stored as source.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A piece of text, stored as its source so that renderers typeset it lazily.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Text {
    /// The source text. With the LaTeX interpreter, segments delimited by `$…$`
    /// are typeset as mathematics and the remainder as plain text.
    pub content: String,
    /// How the source text is interpreted.
    pub interpreter: Interpreter,
}

/// How the source of a [`Text`] is interpreted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Interpreter {
    /// Mixed plain text and `$…$` LaTeX mathematics.
    #[default]
    Latex,
    /// Plain text, rendered literally, including any dollar signs.
    None,
}

impl Text {
    /// Creates text with the default (LaTeX) interpreter.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            interpreter: Interpreter::Latex,
        }
    }

    /// Creates text that is rendered literally, without interpreting mathematics.
    pub fn plain(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            interpreter: Interpreter::None,
        }
    }
}

impl From<&str> for Text {
    fn from(content: &str) -> Self {
        Self::new(content)
    }
}

impl From<String> for Text {
    fn from(content: String) -> Self {
        Self::new(content)
    }
}
