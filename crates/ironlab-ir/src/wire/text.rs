//! Wire types of `ironlab/ir/v0/text.proto`.

proto_file! {
    /// A piece of text, stored as its source so that renderers typeset it lazily.
    message Text {
        /// The source text. With the LaTeX interpreter, segments delimited by `$…$`
        /// are typeset as mathematics and the remainder as plain text.
        string content = 1;
        /// How the source text is interpreted; unspecified means LaTeX.
        enum Interpreter interpreter = 2;
    }

    /// How the source of a text is interpreted.
    enum Interpreter {
        /// Mixed plain text and `$…$` LaTeX mathematics.
        Latex = 1;
        /// Plain text, rendered literally, including any dollar signs.
        None = 2;
    }
}
