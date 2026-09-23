# ADR 0015: Figures are described by their authors, and the viewer derives nothing

**Status:** Accepted

**Related:** [ADR 0001](0001-retained-figure-ir-and-protobuf-wire-format.md), [ADR 0002](0002-egui-viewer-on-rerun-family-crates.md), [ADR 0008](0008-typed-edits-and-a-view-overlay.md)

## Context

A study produces figures faster than anyone can name them. The viewer opened each figure in a tab of its own, so a
collection of a few dozen became a strip of truncated titles that had to be scrolled, and finding a particular
figure meant reading every one of them. The viewer needed a way to narrow a collection down to the figure wanted,
and the question this decision answers is what it should narrow it by.

The figure schema already carried **parameters**: named values that describe a figure, such as the Reynolds number
of the flow it shows, added in schema version 0.2.0 for precisely this purpose and, until now, used by nothing.
Parameters alone are not enough to browse by. A figure belongs to several categories at once — it is
three-dimensional, it is a surface, it is from the second campaign, it is a result rather than a diagnostic — and a
category is not a name with a value under it. Expressing "this figure is a surface" as a parameter forces either a
boolean per category, which multiplies names without bound, or a single name whose value is one category, which
cannot hold two.

The names of both are the author's own. Nobody knows, when the viewer is built, that a user will describe their
figures by `rig`, `wall_y_plus` and `campaign`, so the interface cannot be laid out around a known set of
properties in the way an interface for a known domain can. Whatever the browser offers has to be worked out from the
collection in front of it.

That left a question with two defensible answers, and this decision is mostly about which of them is right.

### The alternative that was built first and rejected

The browser was first built to **derive** facets from each figure's structure: labels for `2d` or `3d`, one for each
kind of artist drawn, and `subplots`, `legend` and `log`; and parameters `dimensionality`, `axes`, `artists` and
`data_values`. A figure's structure is knowable without anybody saying anything, so derivation made a collection
browsable with no preparation at all — including the gallery, whose 26 figures carried no description between them,
and including every figure written before descriptions existed.

The argument was that the feature would otherwise look broken on the very collection that motivated building it.
That argument was wrong, and it is worth writing down why, because it is the kind of argument that recurs. It is an
argument for describing the gallery, not for the viewer inventing descriptions. Deriving facets has the viewer
decide what a collection can be narrowed by, and that decision belongs to the program that built the figures,
because that program is the only thing that knows which of a figure's properties matter. They are almost never the
structural ones: whether a figure happens to draw a surface is rarely the reason anyone is looking for it, and an
author who does care can say so in a line.

Derivation also carried costs that its convenience hid. It created a second namespace beside the author's, which
could collide and so needed a precedence rule. It produced facets that were not part of the figure, so they could
not be edited, were not saved, and changed meaning between releases — a figure sitting on disk would gain a new
label the day the viewer learned to derive one, silently changing what an existing query matched. And it had no
natural bound: every axis scale, every colormap, every artist property is derivable in principle, so each release
would have faced the same question again with no principle to answer it.

## Decision

### Figures are described by their authors alone

**The viewer offers exactly the labels and parameters that a figure carries, and works out nothing of its own.** A
collection whose figures carry no description can be searched by title and listed, and nothing else.

The cost is accepted deliberately: a collection is unbrowsable until its author says what it can be browsed by, and
that includes facts that are plainly visible in the figure. The compensation is that what the browser offers is
always exactly what somebody decided was worth offering.

### Labels are a field of their own, beside parameters

A **label** is a free word describing a figure, of which a figure carries any number. `labels` is a field of
`Figure`, a `repeated string`, added in schema version 0.3.1.

It is a distinguished field rather than a set-valued variant of `Parameter` because a label is a different thing
from a named value, and the difference is the one the browser turns on: a parameter answers *which value does this
figure have*, and admits ranges and an order, while a label answers *is this figure one of these*, and a figure may
be many of them at once. A set-valued parameter would have expressed labels, but it would also have offered every
name an author invented as a possible set, which is a second way to say the same thing and one more decision for an
author to get wrong.

Labels are kept **in the order they were given**, which makes them the only collection in the schema not sorted on
writing. The order is part of the figure's value and is the order the viewer lists them in.

A label must not be empty, and no label may occur twice in one figure; both are validation errors. The rules are
stated in the JSON Schema as `minLength` on the items and `uniqueItems` on the array, so a program generating
figures from the schema is stopped before it writes one. The Protocol Buffers schema can express neither, so a file
in that encoding may carry either fault: the browser shows each label once and drops the empty ones, and **leaves
the figure exactly as it was read**, because opening a file and saving it again must not change it behind the
reader's back. Validation reports the fault instead.

### Which facets to offer is measured, not declared

Because the names are the author's own, the browser cannot be told in advance which are worth offering, and offering
all of them in an arbitrary order is little better. A facet is therefore scored by **how much of the collection
carries it, multiplied by how evenly it divides what it covers** — the normalised Shannon entropy of the counts of
its values, which is 1 when every value is equally common and approaches 0 as one takes over.

Three adjustments follow from what the score is for. A facet with one value divides nothing and scores zero, so it
is not offered. A facet with a distinct value on nearly every figure is a name rather than a division — a run
number, a free-text note — and its score is cut hard, though it is still offered, because nothing is hidden from a
reader who goes looking. The labels lead whatever the arithmetic says, because they are the facet an author wrote in
order to browse by.

The near-unique penalty is deliberately **not** applied to the labels, however many distinct labels a collection
holds. A label carried by a single figure is not a label wasted: the browser lists every label with the count behind
it, and that list is how a reader learns what a collection can be narrowed by at all.

### Counts are computed as though the facet were not filtered

The number beside a value is how many figures would remain if that value were chosen alongside the ones already
chosen **in other** facets, with the facet's own refinement left out. Values within a facet are alternatives and
facets are cumulative, so choosing a second value in a facet always widens the result; computing its counts under
its own refinement would show zero against every value not already chosen and make the interface appear to
dead-end. A value that would leave nothing is shown and disabled rather than removed, because a value that vanishes
as the reader reaches for it reads as a fault.

### A figure that lacks a parameter is outside a refinement on it

Sparseness is the normal case rather than an edge case: a parameter given to some figures and not others is what
happens as soon as two studies are compared. A figure that does not carry a parameter does not pass a refinement on
it, and sorts after every figure that does, whichever way the order runs. Asking for `solver:LES` means asking about
a solver, and a figure with no solver is not an answer to it.

## Consequences

The gallery is labelled by hand, in full — what each figure holds as well as what each entry demonstrates — because
nothing is read off it any more. A test reconciles the structural labels of every entry with its own IR, so an entry
that grows a surface and forgets to say so is caught. That test is what the derivation used to do, asked as a
question rather than assumed as an answer, and it is the pattern to reach for wherever a description could drift
from the figure it describes.

The documentation has to teach this, because it is not what a user will assume. [Describing
figures](../guides/describing-figures.md) states the rule, explains when to reach for a label and when for a
parameter, and says plainly that whatever you might want to search by must be written down. The browser tells a user
the same thing at the moment it matters: a collection of several figures carrying no description at all is offered a
tip pointing at that page, rather than an empty menu that looks broken.

What the viewer knows about a figure stays narrow, which keeps `browse` a pure function of the collection and the
user's choices, testable without a window, in the same way that [ADR 0008](0008-typed-edits-and-a-view-overlay.md)
keeps interaction a pure mapping onto typed edits.

Should deriving facets ever be wanted again — for a collection nobody will go back and describe — it should arrive
as something an author opts into, not as something the viewer does on their behalf.
