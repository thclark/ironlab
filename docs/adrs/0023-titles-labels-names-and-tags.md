# ADR 0023: Titles, labels, names and tags

**Status:** Accepted

**Related:** [ADR 0015](0015-figures-are-described-by-their-authors.md), whose naming of figure labels this record supersedes, and [ADR 0022](0022-figure-nodes-in-one-identifier-space-and-panels.md)

## Context

The schema used the same word for different things. An artist's `display_name` was text drawn in a legend beside the artist's sample, while a legend item that stands for no artist called the same text its `label`. The figure's `labels` were free words for the figure browser, which are never drawn, while an axis `label` is drawn beside its axis. The introduction of data arrays, which tools need to identify but which show nothing on the page, needed a word for text that is never drawn.

## Decision

Each word is used for one kind of text throughout the schema and the facade:

- A **title** is a `Text` drawn as a heading above the thing it names, such as `Figure.title`, `Axes.title` and `Panel.title`.
- A **label** is a `Text` drawn beside the thing it identifies, such as an axis label, a tick label, an artist's legend label and a custom legend item's label.
- A **name** is a `String` that identifies a node to tools and is never drawn, such as `DataArray.name` and the keys of `Figure.parameters`.
- A **tag** is a `String` that describes a figure to the figure browser and is never drawn, held in `Figure.tags`.

Two fields are renamed to follow this rule. An artist's `display_name` becomes `label`, in the IR, the property paths, the wire format, the inspector and the facade, where `.display_name(..)` becomes `.label(..)`. The figure's `labels` become `tags`, with the facade's `.label(..)` and `.labels()` becoming `.tag(..)` and `.tags()`, the figure browser's query keys becoming `tag:` and `tags:`, and the validation error becoming `InvalidTag`.

The inspector names a node by what it shows on the page, such as an artist by its label or an axes by its title. Only a data array, which shows nothing, carries a name for the inspector's use.

This supersedes the naming of figure labels in ADR 0015, whose substance stands: a figure is described only by the tags and parameters its author gives it.

## Consequences

- A field that holds text is named by whether and where its text is drawn, so a reader of the schema can tell from the name alone whether changing it changes the page.
- Programs and saved queries that used `display_name`, `labels` or the `label:` query key must be updated; no aliases are kept.
