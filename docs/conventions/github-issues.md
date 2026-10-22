# GitHub issues

This explains how issues are labelled: which labels exist, what each one means, and which labels an issue carries at each point in its life. The branch and pull request that eventually resolve an issue are covered in [Branching](git-branching.md) and [Pull requests](git-pull-requests.md).

## Label classes

Every label belongs to a class and is written `class: value`, for example `kind: bug` or `crate: scene`. Each class has its own colour family from the [Tailwind palette](https://tailwindcss.com/docs/colors), and the values within a class are shades of that family. No two families are adjacent in the palette, so the class of a label can be recognised from its colour alone.

Within a class the shades are ordered so that they carry meaning, and the tables below list each class from its lightest shade to its darkest. The hex colour is recorded so that a maintainer can recreate a label exactly.

### `kind`: what sort of work the issue is

The `kind` class uses the emerald family. Every issue carries exactly one `kind` label.

| Label               | Colour   | Meaning                                         |
| ------------------- | -------- | ----------------------------------------------- |
| `kind: devops`      | `d1fae5` | Build, CI, release or repository infrastructure |
| `kind: enhancement` | `6ee7b7` | An improvement to an existing capability        |
| `kind: feature`     | `10b981` | A new capability                                |
| `kind: epic`        | `047857` | A group of related issues delivered together    |
| `kind: bug`         | `064e3b` | Something isn't working                         |

### `priority`: how soon the issue should be done

The `priority` class uses the lime family, and the higher the priority, the darker the shade.

| Label              | Colour   | Meaning                                         |
| ------------------ | -------- | ----------------------------------------------- |
| `priority: low`    | `bef264` | Do eventually or wait for user need/urgency     |
| `priority: medium` | `84cc16` | Do in the normal course of work                 |
| `priority: high`   | `4d7c0f` | Do next                                         |

### `flow`: the issue's stage on its way to a pull request

The `flow` class uses the cyan family. How an issue moves between these stages is described in [Flow stages](#flow-stages).

| Label                        | Colour   | Meaning                                                                       |
| ---------------------------- | -------- | ----------------------------------------------------------------------------- |
| `flow: wont-fix`             | `a5f3fc` | Decided not to do this                                                        |
| `flow: information-required` | `67e8f9` | Cannot progress until the reporter or a maintainer supplies more information  |
| `flow: decision-required`    | `22d3ee` | Not yet decided whether to do this - awaiting more information or experience  |
| `flow: plan-required`        | `0891b2` | A plan is required before this can progress to ai-ready                       |
| `flow: ai-ready`             | `155e75` | Sufficiently described that an agent can take this and write a PR             |

### `group`: the family a chart type belongs to

The `group` class uses the blue family. It is applied only to chart-type issues, which are the sub-issues of the chart types epic ([#150](https://github.com/thclark/ironlab/issues/150)), and it sorts them into families of related chart types.

| Label                             | Colour   | Meaning                                                          |
| --------------------------------- | -------- | ---------------------------------------------------------------- |
| `group: basic-2d`                 | `93c5fd` | Basic two-dimensional chart types                                |
| `group: statistical-and-business` | `3b82f6` | Statistical and business chart types                             |
| `group: projections-and-fields`   | `1d4ed8` | Chart types needing new projections, or showing fields           |
| `group: 3d-and-axis-free`         | `1e3a8a` | Three-dimensional chart types and charts without Cartesian axes  |

### `area`: the purpose of the work

The `area` class uses the violet family.

| Label                 | Colour   | Meaning                                         |
| --------------------- | -------- | ----------------------------------------------- |
| `area: documentation` | `ddd6fe` | Improvements or additions to documentation      |
| `area: testing`       | `c4b5fd` | Tests, fixtures and test infrastructure         |
| `area: code`          | `8b5cf6` | Library, viewer or tooling source code          |
| `area: automation`    | `6d28d9` | Scripts, workflows and agent automation         |
| `area: marketing`     | `5b21b6` | Website, examples, showcases and outreach       |

### `crate`: where the work lands

The `crate` class uses the fuchsia family. Its shades follow the order of the pipeline, from the facade and the figure model through to the hosts and the gallery.

| Label            | Colour   | Meaning                                                                         |
| ---------------- | -------- | ------------------------------------------------------------------------------- |
| `crate: ironlab` | `fae8ff` | Mostly related to the ironlab facade (MATLAB-flavoured Rust API) crate          |
| `crate: ir`      | `f5d0fe` | Mostly related to the ironlab-ir figure model crate                             |
| `crate: text`    | `f0abfc` | Mostly related to the ironlab-text fonts, shaping and math typesetting crate    |
| `crate: scene`   | `e879f9` | Mostly related to the ironlab-scene layout and scene compilation crate          |
| `crate: canvas`  | `d946ef` | Mostly related to the ironlab-canvas wgpu figure canvas crate                   |
| `crate: pdf`     | `c026d3` | Mostly related to the ironlab-pdf export crate                                  |
| `crate: viewer`  | `a21caf` | Mostly related to the ironlab-viewer native egui viewer crate                   |
| `crate: web`     | `86198f` | Mostly related to the ironlab-web browser host crate                            |
| `crate: gallery` | `701a75` | Mostly related to the ironlab-gallery examples and docs generator crate         |

## Labelling an open issue

An open issue carries one `kind` label, at least one `area` label, one `priority` label and one `flow` label. It also carries a `crate` label for each crate that the work is expected to change.

The `area` and `crate` classes answer different questions, so they combine freely: `area` records what the work is for, and `crate` records where the work lands. For example, work on `crate: gallery` that is done for the sake of the showcase also carries `area: marketing`. An issue may carry several `area` labels and several `crate` labels. An issue that changes no crate, such as work on CI or an inbox of ideas, carries no `crate` label.

The `kind: epic` label marks an issue that groups related issues, which are usually attached to it as GitHub sub-issues. The chart types epic ([#150](https://github.com/thclark/ironlab/issues/150)) and the web viewer epic ([#54](https://github.com/thclark/ironlab/issues/54)) are examples.

### Chart types and other new entity types

An issue that adds a new chart type, or any other new entity type, starts at `flow: plan-required`. The project rules in [`CLAUDE.md`](https://github.com/thclark/ironlab/blob/main/CLAUDE.md) require the new type's data structure to be reviewed before it is built, and that structure consists of its IR type and its entry in the [figure schema reference](../reference/figure-schema.md). Once that review has taken place, the issue can move to `flow: ai-ready`.

A chart-type issue normally carries `crate: ir`, `crate: scene` and `crate: ironlab`, because each new chart type adds an IR type, the compilation of that type into a scene, and a facade function that creates it. It also carries the `group` label of the family it belongs to.

## Flow stages

The `flow` label records how close an issue is to being ready for a pull request. An issue that is not yet ready waits at one of two stages. At `flow: decision-required`, it has not been decided whether the work should be done at all. At `flow: information-required`, the work cannot progress until the reporter or a maintainer supplies more detail. Once those questions are settled, the issue moves to `flow: plan-required`, where a plan for the work is written and reviewed. When the issue is described well enough that an agent can take it and write the pull request without further guidance, it moves to `flow: ai-ready`. The `flow: wont-fix` stage ends an issue at any point.

An issue at `flow: ai-ready` is ready to enter the development cycle set out in [`CLAUDE.md`](https://github.com/thclark/ironlab/blob/main/CLAUDE.md), in which tests are written and reviewed before the implementation, and the work is committed to a new branch and opened as a pull request.

## Labelling a closed issue

A closed issue keeps its `kind`, `area` and `crate` labels, which continue to describe the work that was done. Its `priority` and `flow` labels are removed, because neither a priority nor a stage means anything once the issue is closed.

The one exception is an issue that is closed without being done. Such an issue carries `flow: wont-fix` and no `crate` labels, since no crate was changed, and it is closed with GitHub's "not planned" reason rather than "completed".

## Related notes

- [Branching](git-branching.md) — bases, naming, protected branches, updating
- [Commits and versioning](git-commits-and-versioning.md) — type codes, breaking changes, semver calculation
- [Pull requests](git-pull-requests.md) — opening, titling, never merging
