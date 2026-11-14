# ADR 0036: Generated schemas are committed, and the schema version is set by rules

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), which this record supersedes in part

## Context

[ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md) generates the Protocol Buffers definition and the JSON Schema from the Rust types and never commits them, so that neither can drift from the code. It splits the JSON Schema into one file per module, treats it as an optional artefact rather than a compatibility check, and leaves the schema version to be chosen by hand. A reviewer therefore had to generate the schemas on both sides of a change to see what it did to the format, and nothing checked that the version agreed with the change.

## Decision

The generated schemas are committed in `schema/`: the figure and edit-protocol JSON Schemas, each as one self-contained document, and the `.proto` files. The Rust types remain the only definition of the model. A pre-commit hook regenerates the files, and CI fails when the committed files differ from those generated, so a change to the types and its effect on the schemas appear together in one diff and cannot drift apart.

The schema version is the version that rule-based tools require, never one chosen by judgement. strands.octue.com compares each JSON Schema with its last published version and suggests the next version, and `buf breaking` compares the `.proto` files with those of `main`. The higher of their requirements is required, and CI fails a pull request into `main` whose schema version differs from it. The only exception is the step from 0.x to 1.0.0, which is a decision to declare the format stable. On release, both schemas are published to strands at the schema version.

This supersedes the parts of ADR 0001 that never commit the schemas, that split the JSON Schema into one file per module, that treat the JSON Schema as no compatibility check, and that leave `buf breaking` as the only check of compatibility.

## Alternatives considered

### Generating the schemas without committing them

Generation alone prevents drift, but a reviewer sees a change to the format only by generating both sides, and other projects must run the generators to read the schema. The freshness checks prevent drift as surely, while the diff shows the change.

### One JSON Schema file per module

Files per module mirror the `.proto` files, but a validator must load all of them and follow references between them, and strands and reviewers read one document more easily. Each change would also appear twice if both forms were committed.

### Choosing the version by hand

A person or an agent judging the size of a change can disagree with themselves from one change to the next. Rule-based tools give the same answer for the same change, and a check that the version equals their answer cannot be argued with.
