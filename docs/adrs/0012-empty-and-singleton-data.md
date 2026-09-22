# ADR 0012: An artist with nothing to draw is valid and is reported as a warning

**Status:** Accepted

**Related:** [ADR 0003](0003-shared-scene-compiler-and-display-list.md), [ADR 0008](0008-typed-edits-and-a-view-overlay.md), [ADR 0011](0011-image-artists.md)

## Context

Every artist draws something at or between the entries of its data: a line joins consecutive points, a marker sits at each point, a contour and a surface draw the cells between the nodes of a grid, and an image paints its pixels. Data can give an artist nothing to draw in two ways. Data is **empty** when a dimension of an array has length zero: a line of no points, an image of no rows, a field of no columns. Data is **singleton** when it holds values but too few for the artist to draw anything from them: a field of one row or one column, from which a contour or surface with `n × m` nodes draws `(n − 1) × (m − 1)` cells, and so none.

Both states are ordinary. A figure that is streamed through the edit protocol of [ADR 0008](0008-typed-edits-and-a-view-overlay.md) begins with empty arrays and grows them one append at a time, so a field passes through no rows and then one row before it has a cell; an edit that retains a window of no entries clears an array to empty; a filter that happens to match nothing produces an empty series; and a program that builds a figure before its data arrives has nothing to put in it yet. [ADR 0011](0011-image-artists.md) decided that an image with no rows or no columns is valid and draws nothing.

Both states can also be a mistake, and before this decision the mistake was easy to miss. A user who writes a pseudocolour plot of a single row of values may expect `n` values to give `n` cells, as they do for a colour-mapped image, and sees no surface at all; a user whose filter matched nothing sees an empty axes. Validation said nothing about either. The scene compiler of [ADR 0003](0003-shared-scene-compiler-and-display-list.md) left a singleton field out with a warning, which the viewer shows in its problems indicator, but drew an empty artist as nothing without comment, and `Figure::export_pdf` discarded the compiler's warnings altogether, so a PDF exported from code was simply missing the artist. The report a person reads when a figure does not show what was expected, and the report an agent programming on a person's behalf reads before it decides the figure is finished, is `Figure::validate`, and it was silent.

A validation *error* for a singleton field was tried and rejected. It refuses the figure, and it refuses every stream at the append that gives a field its first row, for a state that is not wrong but merely has nothing to show.

## Decision

### The rule: errors for figures that are wrong, warnings for artists drawn with less than their data holds

Validation already reports two warnings of one family: `NonPositiveOnLogAxis`, for data that a logarithmic axis cannot show and that is left out, and `ImageOnLogAxis`, for an image that cannot be placed and is left out. Both describe an artist that is drawn with less than its data holds, or not at all, in a figure that is otherwise sound. This decision makes that the rule for every artist: **validation warns of every artist that will draw nothing, names it, and says why.** An error remains reserved for a figure that is wrong, such as arrays whose shapes disagree or a strict out-of-range policy that has been violated.

### One warning kind, `NothingToDraw`

Validation reports the warning `NothingToDraw` against an artist in each of the following cases, with a message that states which applies and the shape of the data:

| Artist | Nothing to draw when |
| --- | --- |
| Line, scatter, quiver | The artist has no points. |
| Image, indexed image, mapped image | The image has no rows or no columns. |
| Contour, surface | The field has no rows or no columns. |
| Contour, surface | The field has a single row or a single column: values, but no cell between nodes to draw them in. |

One kind serves every case because the reader's question is the same in each: why is this artist not in the picture? The message answers it. A reader who filters the report by kind finds every artist that contributes nothing in one place, without needing to know which shape rule was the cause. The check is made from the shapes of the arrays alone, so it is cheap and deterministic and reports nothing that depends on the values. An artist whose values are all NaN also draws nothing, but only a scan of the values can tell, and that case is left to the scene compiler's warnings.

The warning is reported only when the artist's data has otherwise passed validation: an artist whose arrays are missing or whose shapes disagree is reported for that error and not also for drawing nothing.

### Everything stays valid

Empty and singleton data are valid in every artist and both projections. Warnings never refuse a figure: it is exported, shown and edited as usual, and the edit protocol accepts a transaction whatever warnings the figure has afterwards, so a stream may grow an array from nothing and a field may pass through a single row.

### The scene compiler follows the same rule

The scene compiler warns of every artist it leaves out, including an artist with empty data, so that the viewer's problems indicator and `validate()` agree about which artists are absent and why. A compiler warning names the artist, as a validation warning does.

### Export returns a report

`Figure::export_pdf` and `Figure::export_pdf_with` return an `ExportReport` rather than nothing. The report holds the validation warnings of the figure and the warnings the scene compiler raised while drawing it, each with the node it concerns, so that a program learns from the call itself what was left off the page. Nothing is printed and nothing is logged: a library that writes to the terminal is a nuisance to the programs that embed it, and a returned value is what a program or an agent can test. `Figure::show` is unchanged, because the viewer shows the same warnings in its problems indicator while the figure is open.

## Consequences

- No figure is refused for having too little data. A program can build a figure before its data arrives, and a stream can grow an array from nothing, without a validation error at any step.
- A user or an agent who finds an artist missing from a figure gets the reason from `validate()` and from the report that `export_pdf` returns, and sees it in the viewer's problems indicator. The reason names the artist.
- A figure that is streamed from nothing carries `NothingToDraw` warnings until its arrays hold enough to draw. These are true statements about the figure at that moment, and the edit protocol ignores warnings, so the stream is unaffected; a viewer of such a figure sees them in the problems indicator until the data arrives.
- The signature of `export_pdf` changes from `Result<(), Error>` to `Result<ExportReport, Error>`. A caller that wrote `fig.export_pdf(path)?;` is unaffected, because the report may be dropped; a caller that matched on `Ok(())` must change.
- The two lists of an export report overlap where the compiler leaves out an artist that validation warned of, and differ where the compiler finds a reason that validation cannot see, such as a corner of an image that cannot be placed or a piece of LaTeX the typesetter does not support. A program that wants one reason per artist reads the validation list; one that wants every reason reads both.
