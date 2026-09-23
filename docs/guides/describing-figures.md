# Describing figures

A study produces figures faster than anyone can name them. Once there are more than a screenful, finding the one you
want means describing them, and IronLAB stores two kinds of description with a figure for exactly that: **labels**
and **parameters**. Neither affects drawing. Both are saved with the figure, in either format, and both are what the
viewer's [figure browser](viewer.md#the-figure-browser) filters, sorts and groups a collection by.

## The viewer works nothing out for itself

One rule governs everything on this page: **a figure is found by what its author wrote on it, and by nothing else.**

The viewer reads no properties off a figure of its own accord. It does not decide that a figure is
three-dimensional, or that it draws a surface, or that it carries a million data values, and it does not offer any of
those as something to filter by. The program that builds a figure is the only thing that knows which of that
figure's properties matter — usually none of the structural ones — and a program that does care about a structural
property can write it down in a line.

The practical consequence is short: **whatever you might later want to search by, write it down.** A collection of
figures carrying no labels and no parameters can be searched by title and listed, and nothing else.

## Labels

A label is a free word describing a figure, such as `surface`, `piv` or `stalled`. A figure carries any number of
them, and none of them is a name with a value under it.

```rust
let fig = Figure::new()
    .title("Wake behind a cylinder")
    .label("wake")
    .label("piv")
    .label("2d");
```

Labels are kept in the order they were given, which is the order the viewer lists them in. A label must not be
empty, and adding the same label twice is an error that [`validate`](getting-started.md#validating-a-figure)
reports. Labels are compared exactly, so `Wake` and `wake` are two labels and neither is a repeat of the other;
picking one case and keeping to it saves a reader guessing.

A label need not be shared with another figure to be worth adding. The browser lists every label it finds, with the
number of figures behind each, so a label carried by a single figure is how that figure is found rather than a label
wasted. The list of labels is itself what tells a reader what a collection can be narrowed by.

## Parameters

A parameter is a named value describing a figure, such as the Reynolds number of the flow it shows or the solver
that produced its data. A value may be a boolean, an integer, a number or a string, and it keeps that kind when the
figure is saved.

```rust
let fig = Figure::new()
    .title("Wake behind a cylinder")
    .parameter("reynolds_number", 3900.0)
    .parameter("angle_of_attack_deg", 4.0)
    .parameter("solver", "LES")
    .parameter("converged", true);
```

A name occurs at most once, so setting a parameter again replaces its value. A name must not be empty, and a number
must be finite. Names are compared exactly, and a name without spaces in it can be asked about in the browser's
search field, so `reynolds_number` can be searched for where `reynolds number` cannot.

## Choosing between them

The two answer different questions, and the question you expect to ask is what decides.

| | Use a label | Use a parameter |
| --- | --- | --- |
| The question | Is the figure one of these? | Which value does the figure have? |
| Values | The word is the whole of the answer | A name with a value under it |
| How many | Any number per figure | One value per name |
| In the browser | Chosen from a list of words | Chosen from a list of values, or narrowed by a range |
| Ordering | None | Sorted and grouped by value; numbers compared with `>` and `<` |

A useful test is whether you would ever want a **range** or an **order**. An angle of attack is a parameter, because
`angle_of_attack_deg>=8` is a question worth asking and `stalled` is not the same question. A rig is a parameter if
you expect to group by it and read the groups in order; it is a label if you only ever want to say which figures came
from it.

The second test is how many answers a figure can have at once. A figure is two-dimensional or three-dimensional and
never both, so `dimensionality` is a parameter with the values `2D` and `3D`, and a collection can be grouped by it
into two runs. A figure may draw a surface *and* an image, so the kinds it draws are labels, and a figure carries one
for each. The two are not alternatives, and this repository's own gallery uses both at once: every entry carries a
`kind` parameter naming the artist the entry is about, which the browser sorts and groups by, and a label for each
kind it actually draws, which answers whether there is a surface in there anywhere.

Something that is simply true or false can be either, and the difference is what absence means. As the parameter
`converged: false` a figure is positively marked as not having converged, and the browser can group the converged
against the rest. As the label `converged`, a figure without it is either one that did not converge or one nobody
checked, and the browser cannot tell those apart. Prefer the parameter when the opposite is meaningful.

## What to write down

Write what someone would search for, including facts about the figure's own structure when those are among them.
The viewer will not add them for you, and a collection is easier to browse for having them: `surface` and `subplots`
are perfectly good labels, and how many data values a figure holds is a perfectly good parameter, sorted by when you
want to know which figure is making the viewer work.

Beyond that, the descriptions worth carrying are the ones that separate one figure from its neighbours: the
conditions of the run, the campaign it belongs to, the quantity plotted, whether it is a result or a diagnostic. A
description that is the same on every figure divides nothing, and the browser ranks it last accordingly.

Descriptions cost nothing to draw and very little to store, so the error worth avoiding is writing too few.

## Using them

In the viewer, the [figure browser](viewer.md#the-figure-browser) narrows a collection by both. Its search field
takes words and, for parameters, comparisons: `rig:CFD`, `angle_of_attack_deg>=8`, `-stalled`, `label:piv`. Labels
and parameters are also edited in the [property editor](viewer.md#labels) without going back to the code.

Both are stored with the figure in Protocol Buffers and in JSON; how they are encoded is described in the
[figure schema](../reference/figure-schema.md#labels).
