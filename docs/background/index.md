# Background

IronLAB's design was worked out in a conversation, held in September 2026, between the project's author and an AI assistant. The conversation is kept as the [concept discussion](concept-discussion.md) because it records the alternatives that were considered and the reasoning by which they were accepted or rejected.

The discussion covers, in order: the choice of a retained figure model as the centre of the system; candidate architectures for connecting programs to a viewer (in process, over a socket, in a browser, or through files); desktop user-interface toolkits for Rust; styling and component libraries for egui; data inspection and GPU picking; PDF as the first export format; and the choice of an embedded LaTeX mathematics typesetter.

The discussion is a historical record, not a specification. Several of its suggestions were revised later in the same conversation, and others were deliberately left out of the first release. The decisions that were taken are stated authoritatively in the [architecture decision records](../adrs/index.md), and the scope of the first release in [ADR 0007](../adrs/0007-mvp-scope.md).
