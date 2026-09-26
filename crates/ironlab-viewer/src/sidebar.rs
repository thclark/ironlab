//! The figure browser: a left-hand panel that narrows a collection of figures down to the one to look at.
//!
//! It draws what [`crate::browse`] works out, and holds nothing but the state of the controls themselves. The shape
//! is deliberate, and it is the one that survives an unbounded number of parameters:
//!
//! - **The controls are small and the list is large.** A panel of checkboxes, one section per parameter, grows with
//!   the collection until the list of figures starts below the bottom of the window. Here the controls take three
//!   rows whatever the collection holds, and everything below them is the list.
//! - **Filters are chips, added from a menu that opens within the panel.** "Add", at the right of the "Filters"
//!   caption, opens beneath it a list of the parameters worth filtering on, most useful first, and then of that
//!   parameter's values with the count each would leave. The menu pushes the controls and the list down rather
//!   than floating over them, so it stays open while values are chosen one at a time; "Done" shuts it, as does
//!   the same control, which reads "Close" while the menu is open. What has been chosen reads back as a row of
//!   chips beneath the menu, where their coming and going cannot move it; clicking a chip takes it away, and
//!   "Clear all", beside "Add", takes every one away.
//! - **Every setting has a row of its own.** The filters, the order and the grouping are each a caption at the left
//!   of a row and a control at the right, so that the three are read the same way.
//! - **The search field is the whole of it for anyone who would rather type.** `rig:CFD angle>=8 -stalled` does what
//!   three chips do. It is also the only way to ask something the menus do not offer, so the interface never has to
//!   grow a control for every question.
//!
//! Everything here is drawn from [`crate::widgets`]: every button is a [`Control`], every line of the list and of
//! the menu is a [`Row`], and the panel names no size, colour or padding of its own. The list is drawn with
//! [`egui::ScrollArea::show_rows`], which lays out only the rows on screen, so a collection of a few hundred figures
//! costs the same per frame as a collection of ten.

use std::collections::BTreeSet;

use crate::browse::{
    Browse, Constraint, Facet, FacetKey, FacetValue, FigureCard, SortKey, describe_facets,
    has_nothing_to_browse_by,
};
use crate::widgets::{
    Control, Detail, Icon, Leading, PanelKind, Role, Row, RowState, Spacing, Tint, block,
    captioned_row, combo, field, histogram, label, note, text, well,
};

/// The width the browser opens at, in egui points: wide enough for the order and the grouping to share one row and
/// for a figure's title not to be cut short in the ordinary case.
pub const WIDTH: f32 = 336.0;

/// The narrowest the browser can be dragged, in egui points, below which a title tells the reader nothing.
pub const MIN_WIDTH: f32 = 220.0;

/// The identifier of the browser's panel, which is also what a test loads its geometry by.
pub const PANEL_ID: &str = "ironlab_figure_browser";

/// The height beyond which the menu's list of parameters scrolls, in egui points.
const MENU_PARAMETERS_HEIGHT: f32 = 210.0;

/// The height beyond which the menu's list of values scrolls, in egui points.
const MENU_VALUES_HEIGHT: f32 = 240.0;

/// How many values of a parameter the menu shows before the rest are reached by asking for more.
const MENU_VALUES: usize = 14;

/// The number of values above which the menu offers a field to search them, because a list that long is faster to
/// type into than to scroll.
const MENU_SEARCH_VALUES: usize = 60;

/// The width of the combo boxes of the order and the grouping, in egui points, when the panel has room for it.
const COMBO_WIDTH: f32 = 150.0;

/// The narrowest a combo box of the order or the grouping is drawn, in egui points.
const COMBO_MIN_WIDTH: f32 = 56.0;

/// The hint shown in the empty search field, which is also where the typed form is taught.
const SEARCH_HINT: &str = "Search, or rig:CFD angle>=8 -stalled";

/// The page that explains how to describe a figure so that it can be found again.
///
/// The browser links to it when a collection carries nothing to browse by. The address is the published site
/// rather than a path, because the viewer is a desktop program with no documentation beside it; a test checks that
/// the page it names is still in `docs/`, so the link cannot rot unnoticed when a page is renamed.
pub const DESCRIBING_FIGURES_URL: &str = "https://ironlab.org/guides/describing-figures/";

/// What the browser is showing in its filter menu.
#[derive(Clone, Debug, Default, PartialEq)]
enum Menu {
    /// The menu is closed.
    #[default]
    Closed,
    /// The parameters that can be filtered on, narrowed by what has been typed.
    Parameters { search: String },
    /// The values of one parameter.
    Values {
        key: FacetKey,
        search: String,
        /// Whether every value is shown, rather than the first [`MENU_VALUES`] of them.
        all: bool,
    },
}

/// The state of the figure browser: what the user has chosen, and where its controls have got to.
///
/// The choices themselves live in [`FigureBrowser::browse`], which is a plain value that the tests drive directly;
/// everything else here is the state of a control, such as which menu is open.
#[derive(Clone, Debug, Default)]
pub struct FigureBrowser {
    /// What the user has chosen: the query, the filters, the order and the grouping.
    pub browse: Browse,
    /// Whether the panel is shown.
    pub open: bool,
    /// What the filter menu is showing.
    menu: Menu,
    /// The names of the groups the reader has closed.
    closed: BTreeSet<String>,
}

impl FigureBrowser {
    /// A browser for a collection of `count` figures, with nothing chosen.
    ///
    /// It opens with the collection when there is more than one figure, because a collection large enough to need
    /// browsing is one whose tab strip the reader cannot take in at a glance. One figure is not a collection, and
    /// the browser stays shut.
    #[must_use]
    pub fn for_collection(count: usize) -> Self {
        Self {
            open: count > 1,
            ..Self::default()
        }
    }
}

/// What the browser asks the application for, beyond the choices it records in its own state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrowserResponse {
    /// The figure the user chose this frame, as an index into the collection, when they chose one.
    pub chosen: Option<usize>,
}

/// One line of the list, which is either a figure or the heading of a group of them.
///
/// The list is flattened into lines of one height so that it can be drawn by [`egui::ScrollArea::show_rows`], which
/// needs to know where a line is without laying out the lines above it.
enum Line {
    /// The heading of a group, with how many figures it holds and whether it is open.
    Heading {
        name: String,
        count: usize,
        open: bool,
    },
    /// A figure, as an index into the collection.
    Figure(usize),
}

/// Draws the figure browser into `ui` as a left-hand panel, and returns what the user asked for.
///
/// `cards` is the collection, in the order the application holds it, and `selected` is the index of the figure being
/// shown, which is drawn as the selected row when the filters leave it in the list.
///
/// The panel is drawn only when [`FigureBrowser::open`]; when it is not, nothing is drawn and the caller keeps the
/// whole of `ui` for the figure.
pub fn figure_browser(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    selected: usize,
) -> BrowserResponse {
    if !browser.open {
        return BrowserResponse::default();
    }
    let frame = PanelKind::Bare.frame(ui);
    egui::Panel::left(PANEL_ID)
        .default_size(WIDTH)
        .min_size(MIN_WIDTH)
        .frame(frame)
        .show(ui, |ui| contents(ui, browser, cards, selected))
        .inner
}

/// Draws the contents of the panel: the controls, then the list, then the strip that says how many figures are left.
fn contents(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    selected: usize,
) -> BrowserResponse {
    let facets = describe_facets(cards);
    let results = browser.browse.results(cards);

    egui::Panel::top(egui::Id::new(PANEL_ID).with("controls"))
        .resizable(false)
        .frame(PanelKind::AboveRule.frame(ui))
        .show(ui, |ui| {
            crate::style::compact(ui);
            controls(ui, browser, cards, &facets);
        });
    // The strip at the foot is given its height before the list is drawn, so that the list never takes the room the
    // strip needs and the strip never moves as the list grows.
    let foot = Spacing::CONTROL_HEIGHT + 2.0 * Spacing::ROW_Y;
    egui::Panel::bottom(egui::Id::new(PANEL_ID).with("count"))
        .exact_size(foot)
        .frame(PanelKind::Foot.frame(ui))
        .show(ui, |ui| {
            crate::style::compact(ui);
            count_strip(ui, browser, results.matched, results.total);
        });
    egui::CentralPanel::default()
        .frame(PanelKind::BelowRule.frame(ui))
        .show(ui, |ui| list(ui, browser, cards, &results, selected))
        .inner
}

/// Draws the search field, the filters row and the menu it opens, the chips, and the rows of order and grouping.
fn controls(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    facets: &[Facet],
) {
    block(ui, |ui| {
        field(
            ui,
            &mut browser.browse.query,
            SEARCH_HINT,
            egui::Id::new(PANEL_ID).with("search"),
        )
        .on_hover_text(
            "Type words to search the titles, labels and parameters. \
             A term such as rig:CFD or angle>=8 asks about one parameter, and a leading minus excludes.",
        );
    });

    if has_nothing_to_browse_by(cards) {
        block(ui, tip);
    }

    block(ui, |ui| {
        captioned_row(ui, "Filters", |ui| {
            // Laid out from the right edge inwards: Add at the edge, and Clear all beside it when there is
            // anything to clear.
            add_filter_button(ui, browser);
            clear_all_button(ui, browser);
        });
    });

    if browser.menu != Menu::Closed {
        well(ui, |ui| menu(ui, browser, cards, facets));
    }

    if !browser.browse.filters.is_empty() {
        block(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::splat(Spacing::GAP);
            ui.horizontal_wrapped(|ui| chips(ui, browser));
        });
    }

    let width = combo_width(ui);
    block(ui, |ui| sort_row(ui, browser, facets, width));
    block(ui, |ui| group_row(ui, browser, facets, width));
}

/// Draws the note shown when no figure of the collection carries a label or a parameter.
///
/// Without one, the panel would offer an empty menu and a reader would reasonably conclude that it does not work.
/// The note says what is missing, that it is added in the code that builds the figures, and where to read about
/// doing so: the viewer works nothing out for itself, so this is the one thing it cannot fix on the reader's
/// behalf.
fn tip(ui: &mut egui::Ui) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(
                "None of these figures carries a label or a parameter, so there is nothing to \
                 narrow them by. Add them where the figures are built, and this panel will \
                 filter, sort and group by them.",
            )
            .weak(),
        );
        ui.hyperlink_to("How to describe figures", DESCRIBING_FIGURES_URL);
    });
}

/// Draws the control that opens the menu, "Add", and shuts it again, "Close". Adding is what the menu is for; a
/// filter once added is not edited but taken away by its chip.
fn add_filter_button(ui: &mut egui::Ui, browser: &mut FigureBrowser) {
    let open = browser.menu != Menu::Closed;
    let control = if open {
        Control::button("Close").before(Icon::Minus)
    } else {
        Control::button("Add").before(Icon::Plus)
    };
    let response = control.show(ui).on_hover_text(if open {
        "Shut the menu, keeping what has been chosen."
    } else {
        "Narrow the list by one of the figures' parameters."
    });
    if response.clicked() {
        browser.menu = if open {
            Menu::Closed
        } else {
            // Each opening starts at the list of parameters, because the parameter wanted this time is rarely the
            // one wanted last time, and an empty search field is the fastest way to any of them.
            Menu::Parameters {
                search: String::new(),
            }
        };
    }
}

/// Draws the control that takes every filter away, beside "Add", when there is a filter to take away.
fn clear_all_button(ui: &mut egui::Ui, browser: &mut FigureBrowser) {
    if browser.browse.filters.is_empty() {
        return;
    }
    if Control::button("Clear all")
        .before(Icon::Restore)
        .show(ui)
        .on_hover_text("Remove every filter.")
        .clicked()
    {
        browser.browse.filters.clear();
    }
}

/// Draws whichever page of the menu is open.
///
/// The menu has two pages: the parameters worth filtering on, and then the values of the one chosen. Two pages
/// rather than one keeps the menu the same height whether the collection offers three parameters or three hundred,
/// and it is what lets the menu show the count each value would leave, which a flat list has no room for.
fn menu(ui: &mut egui::Ui, browser: &mut FigureBrowser, cards: &[FigureCard], facets: &[Facet]) {
    let menu = std::mem::take(&mut browser.menu);
    browser.menu = match menu {
        Menu::Closed | Menu::Parameters { .. } => {
            let search = match menu {
                Menu::Parameters { search } => search,
                _ => String::new(),
            };
            parameter_menu(ui, browser, cards.len(), facets, search)
        }
        Menu::Values { key, search, all } => match facets.iter().find(|facet| facet.key == key) {
            // The collection changed under the menu and the parameter is gone; the list of parameters is the only
            // honest thing to show.
            None => parameter_menu(ui, browser, cards.len(), facets, String::new()),
            Some(facet) => value_menu(ui, browser, cards, facet, search, all),
        },
    };
}

/// Draws the first page of the menu: the parameters worth filtering on, most useful first, each with how many
/// values it takes and how much of the collection carries it, at the right edge.
fn parameter_menu(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    total: usize,
    facets: &[Facet],
    mut search: String,
) -> Menu {
    field(
        ui,
        &mut search,
        "Filter on…",
        egui::Id::new(PANEL_ID).with("parameter_search"),
    );
    ui.add_space(Spacing::GAP);
    let wanted = search.to_lowercase();
    let mut chosen = None;
    let height = Row::height(ui, false);
    egui::ScrollArea::vertical()
        .id_salt("ironlab_browser_parameters")
        .max_height(MENU_PARAMETERS_HEIGHT)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut offered = 0;
            for facet in facets {
                // A parameter that takes one value divides nothing, so it is never offered: choosing it would leave
                // the list exactly as it is.
                if facet.cardinality() < 2 || !facet.key.name().to_lowercase().contains(&wanted) {
                    continue;
                }
                offered += 1;
                let held = browser.browse.filter(&facet.key).is_some();
                let coverage = (facet.present * 100 + total.max(1) / 2) / total.max(1);
                let detail = format!("{} values · {coverage}%", facet.cardinality());
                let row = Row::new(
                    text(Role::Body, facet.key.name()),
                    Detail::Trailing(&detail),
                )
                .state(RowState {
                    selected: held,
                    ..RowState::default()
                });
                if row
                    .show(ui, height)
                    .on_hover_text("Choose which of its values to keep.")
                    .clicked()
                {
                    chosen = Some(facet.key.clone());
                }
            }
            if offered == 0 {
                label(
                    ui,
                    text(Role::Control, "No parameter of these figures divides them."),
                );
            }
        });
    match chosen {
        Some(key) => Menu::Values {
            key,
            search: String::new(),
            all: false,
        },
        None => Menu::Parameters { search },
    }
}

/// Draws the second page of the menu: the values of one parameter, with the count each would leave, and the
/// controls that go back to the parameters and that shut the menu.
fn value_menu(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    facet: &Facet,
    mut search: String,
    mut all: bool,
) -> Menu {
    let mut back = false;
    ui.horizontal(|ui| {
        back = Control::button("Back to all parameters")
            .quiet()
            .show(ui)
            .on_hover_text("Choose another parameter.")
            .clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            label(ui, text(Role::Label, facet.key.name()));
        });
    });
    if back {
        return Menu::Parameters {
            search: String::new(),
        };
    }
    ui.add_space(Spacing::LINE_GAP);

    if facet.kind.is_numeric() {
        range_control(ui, browser, cards, facet);
    } else {
        value_list(ui, browser, cards, facet, &mut search, &mut all);
    }

    ui.add_space(Spacing::GAP);
    if Control::button("Done")
        .show(ui)
        .on_hover_text("Shut the menu, keeping what has been chosen.")
        .clicked()
    {
        return Menu::Closed;
    }
    Menu::Values {
        key: facet.key.clone(),
        search,
        all,
    }
}

/// Draws the values of a parameter as a counted checklist, most figures first.
///
/// A value that would leave nothing is shown rather than hidden, and faded: a reader reaching for it learns that
/// the collection has nothing there, where a value that vanished as they reached would look like a fault.
fn value_list(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    facet: &Facet,
    search: &mut String,
    all: &mut bool,
) {
    let counts = browser.browse.counts(cards, facet);
    let chosen: Vec<FacetValue> = match browser.browse.filter(&facet.key).map(|f| &f.constraint) {
        Some(Constraint::AnyOf(values)) => values.clone(),
        _ => Vec::new(),
    };

    if facet.cardinality() > MENU_SEARCH_VALUES {
        field(
            ui,
            search,
            "Which value?",
            egui::Id::new(PANEL_ID).with("value_search"),
        );
        ui.add_space(Spacing::GAP);
    }
    let wanted = search.to_lowercase();

    // The values with the most figures behind them come first, because that is the order in which they are worth
    // choosing; ties fall back to the value's own order so the list does not shuffle.
    let mut values: Vec<(&FacetValue, usize)> = counts
        .iter()
        .map(|(value, count)| (value, *count))
        .filter(|(value, _)| value.text().to_lowercase().contains(&wanted))
        .collect();
    values.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let shown = if *all { values.len() } else { MENU_VALUES };

    let mut toggled = None;
    let height = Row::height(ui, false);
    egui::ScrollArea::vertical()
        .id_salt("ironlab_browser_values")
        .max_height(MENU_VALUES_HEIGHT)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (value, count) in values.iter().take(shown) {
                let ticked = chosen.contains(value);
                let words = value.text();
                let count_text = count.to_string();
                let row = Row::new(text(Role::Body, &words), Detail::Trailing(&count_text))
                    .leading(Leading::Check { ticked })
                    .state(RowState {
                        faded: !ticked && *count == 0,
                        ..RowState::default()
                    });
                if row.show(ui, height).clicked() {
                    toggled = Some((*value).clone());
                }
            }
            if values.len() > shown
                && Control::button(&format!("{} more…", values.len() - shown))
                    .quiet()
                    .show(ui)
                    .on_hover_text("Show every value.")
                    .clicked()
            {
                *all = true;
            }
            if values.is_empty() {
                label(ui, text(Role::Control, "No value matches."));
            }
        });

    if let Some(value) = toggled {
        browser.browse.toggle(&facet.key, &value);
    }
}

/// Draws the control of a numeric parameter: a histogram of its values, and beneath it the two ends of the range
/// kept.
///
/// egui's slider carries one value, so a range is two number fields rather than a two-ended slider. They are the
/// controls the property editor already uses for a number, and they say exactly what they mean, which a pair of
/// handles on one track does not. Widening them to the whole of the parameter takes the filter away, because a
/// range that keeps everything narrows nothing.
fn range_control(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    facet: &Facet,
) {
    let Some((least, most)) = facet.range else {
        label(
            ui,
            text(Role::Control, "The parameter holds no number to compare."),
        );
        return;
    };
    let (mut low, mut high) = match browser.browse.filter(&facet.key).map(|f| &f.constraint) {
        Some(Constraint::Between { low, high }) => (*low, *high),
        _ => (least, most),
    };

    let counts: Vec<(f64, usize)> = browser
        .browse
        .counts(cards, facet)
        .iter()
        .filter_map(|(value, count)| value.as_number().map(|number| (number, *count)))
        .collect();
    ui.add_space(Spacing::LINE_GAP);
    histogram(ui, &counts, (least, most), (low, high));
    ui.add_space(Spacing::GAP);

    // A step of a thousandth of the span moves the end across the whole range in a drag of reasonable length,
    // whatever the parameter is measured in.
    let speed = ((most - least) / 1000.0).abs().max(f64::MIN_POSITIVE);
    let mut changed = false;
    ui.horizontal(|ui| {
        let ellipsis = ui
            .painter()
            .layout_no_wrap("…".to_owned(), Role::Data.font(), egui::Color32::WHITE)
            .size()
            .x;
        let end = ((ui.available_width() - ellipsis - 2.0 * Spacing::GAP) / 2.0)
            .max(ui.spacing().interact_size.x);
        ui.spacing_mut().interact_size.x = end;
        changed |= ui
            .add(
                egui::DragValue::new(&mut low)
                    .speed(speed)
                    .range(least..=most),
            )
            .on_hover_text("The least value kept.")
            .changed();
        label(ui, text(Role::Data, "…"));
        changed |= ui
            .add(
                egui::DragValue::new(&mut high)
                    .speed(speed)
                    .range(least..=most),
            )
            .on_hover_text("The greatest value kept.")
            .changed();
    });
    if facet.present < cards.len() {
        let carry = format!("{} of {} figures carry this", facet.present, cards.len());
        label(ui, text(Role::Data, &carry));
    }
    if changed {
        browser.browse.set_range(facet, low, high);
    }
}

/// Draws one chip per filter: the parameter's name, what it was narrowed to, and the cross that says clicking it
/// takes the filter away. A chip on the labels is drawn in the green of a tag, so that it reads as a filter on
/// labels rather than on a parameter.
fn chips(ui: &mut egui::Ui, browser: &mut FigureBrowser) {
    let mut remove = None;
    for filter in &browser.browse.filters {
        let value = match &filter.constraint {
            Constraint::AnyOf(values) => values
                .iter()
                .map(FacetValue::text)
                .collect::<Vec<_>>()
                .join(" or "),
            Constraint::Between { low, high } => format!(
                "{} to {}",
                crate::browse::number_text(*low),
                crate::browse::number_text(*high)
            ),
        };
        let tint = match filter.key {
            FacetKey::Labels => Tint::LABEL,
            FacetKey::Parameter(_) => Tint::PARAMETER,
        };
        if Control::chip(filter.key.name(), &value, tint)
            .show(ui)
            .on_hover_text("Click to remove this filter.")
            .clicked()
        {
            remove = Some(filter.key.clone());
        }
    }
    if let Some(key) = remove {
        browser.browse.remove(&key);
    }
}

/// The word on the control that reverses the order, which says which way it runs.
fn direction_word(descending: bool) -> &'static str {
    if descending {
        "Descending"
    } else {
        "Ascending"
    }
}

/// The width of a combo box of the order or the grouping: [`COMBO_WIDTH`] when the panel has room for the widest
/// row, which is the order's caption, its direction control and its box, and what that row leaves otherwise.
///
/// The two rows share one width, whatever each holds, so that their boxes end at one edge.
fn combo_width(ui: &egui::Ui) -> f32 {
    let measure = |words: &str, role: Role| {
        ui.painter()
            .layout_no_wrap(words.to_owned(), role.font(), egui::Color32::WHITE)
            .size()
            .x
    };
    let direction = measure(direction_word(true), Role::Control)
        .max(measure(direction_word(false), Role::Control))
        + Spacing::GAP
        + Icon::SLOT
        + 2.0 * Spacing::CONTROL_PADDING.x;
    let caption = measure("GROUP", Role::Label).max(measure("SORT", Role::Label));
    let room =
        ui.available_width() - 2.0 * Spacing::INSET - caption - direction - 2.0 * Spacing::GAP;
    room.clamp(COMBO_MIN_WIDTH, COMBO_WIDTH)
}

/// Draws the row of the order: its caption at the left, and at the right the box that says what the order is taken
/// from and, beside it, the control that reverses it, carrying the triangle of a combo box turned the way the
/// order runs.
fn sort_row(ui: &mut egui::Ui, browser: &mut FigureBrowser, facets: &[Facet], width: f32) {
    captioned_row(ui, "Sort", |ui| {
        let selected = match &browser.browse.sort.key {
            SortKey::Title => "Title".to_owned(),
            SortKey::Parameter(name) => name.clone(),
        };
        combo(ui, "ironlab_browser_sort", &selected, width, |ui| {
            ui.selectable_value(&mut browser.browse.sort.key, SortKey::Title, "Title");
            for facet in facets {
                if let FacetKey::Parameter(name) = &facet.key {
                    ui.selectable_value(
                        &mut browser.browse.sort.key,
                        SortKey::Parameter(name.clone()),
                        name,
                    );
                }
            }
        });
        let descending = browser.browse.sort.descending;
        let icon = if descending {
            Icon::TriangleDown
        } else {
            Icon::TriangleUp
        };
        if Control::button(direction_word(descending))
            .after(icon)
            .show(ui)
            .on_hover_text("Reverse the order.")
            .clicked()
        {
            browser.browse.sort.descending = !descending;
        }
    });
}

/// Draws the row of the grouping: its caption at the left, and at the right the box that says what the list is
/// grouped by.
fn group_row(ui: &mut egui::Ui, browser: &mut FigureBrowser, facets: &[Facet], width: f32) {
    captioned_row(ui, "Group", |ui| {
        let selected = match &browser.browse.group {
            None => "None".to_owned(),
            Some(key) => key.name().to_owned(),
        };
        combo(ui, "ironlab_browser_group", &selected, width, |ui| {
            ui.selectable_value(&mut browser.browse.group, None, "None");
            for facet in facets {
                if facet.cardinality() < 2 {
                    continue;
                }
                ui.selectable_value(
                    &mut browser.browse.group,
                    Some(facet.key.clone()),
                    facet.key.name(),
                );
            }
        });
    });
}

/// Draws the list of figures, and returns the one chosen this frame.
fn list(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    results: &crate::browse::Results,
    selected: usize,
) -> BrowserResponse {
    let mut response = BrowserResponse::default();
    if results.is_empty() {
        note(
            ui,
            "No figure matches",
            "Loosen a filter, or clear them all.",
        );
        return response;
    }

    let lines = flatten(browser, results);
    let height = Row::height(ui, true);
    let mut toggle = None;
    egui::ScrollArea::vertical()
        .id_salt("ironlab_browser_list")
        .auto_shrink([false, false])
        .show_rows(ui, height, lines.len(), |ui, range| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for index in range {
                match &lines[index] {
                    Line::Heading { name, count, open } => {
                        if heading(ui, name, *count, *open, height) {
                            toggle = Some(name.clone());
                        }
                    }
                    Line::Figure(card) => {
                        let striped = index % 2 == 1;
                        if figure_row(
                            ui,
                            &cards[*card],
                            browser,
                            *card == selected,
                            striped,
                            height,
                        ) {
                            response.chosen = Some(*card);
                        }
                    }
                }
            }
        });
    if let Some(name) = toggle
        && !browser.closed.remove(&name)
    {
        browser.closed.insert(name);
    }
    response
}

/// Flattens the grouped result into the lines to draw, leaving out the figures of the groups that are closed.
fn flatten(browser: &FigureBrowser, results: &crate::browse::Results) -> Vec<Line> {
    let mut lines = Vec::new();
    for group in &results.groups {
        match &group.name {
            None => lines.extend(group.members.iter().copied().map(Line::Figure)),
            Some(name) => {
                let open = !browser.closed.contains(name);
                lines.push(Line::Heading {
                    name: name.clone(),
                    count: group.members.len(),
                    open,
                });
                if open {
                    lines.extend(group.members.iter().copied().map(Line::Figure));
                }
            }
        }
    }
    lines
}

/// Draws the heading of a group: a band across the list carrying a triangle that says whether the group is open,
/// the name of the group, and how many figures it holds. Returns whether it was clicked.
fn heading(ui: &mut egui::Ui, name: &str, count: usize, open: bool, height: f32) -> bool {
    let count_text = count.to_string();
    Row::new(text(Role::Label, name), Detail::Trailing(&count_text))
        .leading(Leading::Disclosure { open })
        .state(RowState {
            band: true,
            ..RowState::default()
        })
        .spoken(format!(
            "{name}, {count} figures, {}",
            if open { "open" } else { "closed" }
        ))
        .show(ui, height)
        .on_hover_text(if open {
            "Close this group."
        } else {
            "Open this group."
        })
        .clicked()
}

/// Draws one figure of the list, and returns whether it was chosen.
///
/// The row is two lines: the title, and beneath it whatever the reader is most likely to want beside the title —
/// the value the list is ordered by when it is ordered by a parameter, and the figure's labels otherwise.
fn figure_row(
    ui: &mut egui::Ui,
    card: &FigureCard,
    browser: &FigureBrowser,
    selected: bool,
    striped: bool,
    height: f32,
) -> bool {
    let detail = match &browser.browse.sort.key {
        SortKey::Parameter(name) => match card.parameter(name) {
            Some(value) => format!("{name} = {}", FacetValue::from(value).text()),
            None => format!("no {name}"),
        },
        SortKey::Title => card
            .labels
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join(", "),
    };
    Row::new(text(Role::Body, &card.title), Detail::Beneath(&detail))
        .state(RowState {
            selected,
            striped,
            ..RowState::default()
        })
        .show(ui, height)
        .clicked()
}

/// Draws the strip that says how much of the collection is left, and the control that takes every choice back.
fn count_strip(ui: &mut egui::Ui, browser: &mut FigureBrowser, matched: usize, total: usize) {
    ui.horizontal(|ui| {
        let words = if matched == total {
            format!("{total} figures")
        } else {
            format!("{matched} of {total} figures")
        };
        label(ui, text(Role::Data, &words));
        if !browser.browse.is_unfiltered() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Control::button("Show all")
                    .quiet()
                    .show(ui)
                    .on_hover_text("Remove every filter and clear the search.")
                    .clicked()
                {
                    browser.browse.clear();
                }
            });
        }
    });
}
