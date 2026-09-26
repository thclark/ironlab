//! The figure browser: a left-hand panel that narrows a collection of figures down to the one to look at.
//!
//! It draws what [`crate::browse`] works out, and holds nothing but the state of the controls themselves. The shape
//! is deliberate, and it is the one that survives an unbounded number of parameters:
//!
//! - **The controls are small and the list is large.** A panel of checkboxes, one section per parameter, grows with
//!   the collection until the list of figures starts below the bottom of the window. Here the controls take four
//!   rows whatever the collection holds, and everything below them is the list.
//! - **Filters are chips, added from a menu.** "Add filter" opens a menu of the parameters worth filtering on, most
//!   useful first, and then of that parameter's values with the count each would leave. What has been chosen reads
//!   back as a row of chips, and clicking a chip takes it away.
//! - **The search field is the whole of it for anyone who would rather type.** `rig:CFD angle>=8 -stalled` does what
//!   three chips do. It is also the only way to ask something the menus do not offer, so the interface never has to
//!   grow a control for every question.
//!
//! The list is drawn with [`egui::ScrollArea::show_rows`], which lays out only the rows on screen, so a collection
//! of a few hundred figures costs the same per frame as a collection of ten.
//!
//! Every mark drawn here is one of [`crate::style::MARKS`], which [`crate::style::INTERFACE_CHARACTERS`] promises
//! the fonts carry, and each augments words rather than standing in for them: a chip says which parameter it
//! narrows and to what, and carries a cross to say that clicking it takes that away; the control that reverses an
//! order says "Ascending" or "Descending", and carries the arrow that says which at a glance.

use std::collections::BTreeSet;

use egui::text::LayoutJob;

use crate::browse::{
    Browse, Constraint, Facet, FacetKey, FacetValue, FigureCard, SortKey, describe_facets,
    has_nothing_to_browse_by,
};

/// The width the browser opens at, in egui points: wide enough for the order and the grouping to share one row and
/// for a figure's title not to be cut short in the ordinary case.
pub const WIDTH: f32 = 336.0;

/// The narrowest the browser can be dragged, in egui points, below which a title tells the reader nothing.
pub const MIN_WIDTH: f32 = 220.0;

/// The identifier of the browser's panel, which is also what a test loads its geometry by.
pub const PANEL_ID: &str = "ironlab_figure_browser";

/// The width of the menu that "Add filter" opens, in egui points.
const MENU_WIDTH: f32 = 268.0;

/// The height beyond which a list inside the menu scrolls, in egui points.
const MENU_LIST_HEIGHT: f32 = 240.0;

/// How many values of a facet the menu offers before the rest are reached by typing in its search field.
const MENU_VALUES: usize = 60;

/// The room between the edge of a row of the list and its text, in egui points, sideways and up and down.
const ROW_PADDING: egui::Vec2 = egui::vec2(9.0, 5.0);

/// The width of the bar drawn down the left edge of the row of the figure shown, in egui points.
const SELECTED_BAR: f32 = 2.0;

/// The hint shown in the empty search field, which is also where the typed form is taught.
const SEARCH_HINT: &str = "Search, or rig:CFD angle>=8 -stalled";

/// The page that explains how to describe a figure so that it can be found again.
///
/// The browser links to it when a collection carries nothing to browse by. The address is the published site
/// rather than a path, because the viewer is a desktop program with no documentation beside it; a test checks that
/// the page it names is still in `docs/`, so the link cannot rot unnoticed when a page is renamed.
pub const DESCRIBING_FIGURES_URL: &str = "https://ironlab.org/guides/describing-figures/";

/// What the browser is showing in its "Add filter" menu.
#[derive(Clone, Debug, Default, PartialEq)]
enum Menu {
    /// The menu is closed.
    #[default]
    Closed,
    /// The parameters that can be filtered on, narrowed by what has been typed.
    Parameters { search: String },
    /// The values of one parameter.
    Values { key: FacetKey, search: String },
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
    /// What the "Add filter" menu is showing.
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

/// One row of the list, which is either a figure or the heading of a group of them.
///
/// The list is flattened into rows of one height so that it can be drawn by [`egui::ScrollArea::show_rows`], which
/// needs to know where a row is without laying out the rows above it.
enum Row {
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
    egui::Panel::left(PANEL_ID)
        .default_size(WIDTH)
        .min_size(MIN_WIDTH)
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

    // The strip at the foot is given its height before the list is drawn, so that the list never takes the room the
    // strip needs and the strip never moves as the list grows.
    let foot = ui.spacing().interact_size.y + 2.0 * ui.spacing().button_padding.y;
    egui::Panel::top(egui::Id::new(PANEL_ID).with("controls"))
        .resizable(false)
        .show(ui, |ui| controls(ui, browser, cards, &facets));
    egui::Panel::bottom(egui::Id::new(PANEL_ID).with("count"))
        .exact_size(foot)
        .show(ui, |ui| {
            count_strip(ui, browser, results.matched, results.total);
        });
    egui::CentralPanel::default()
        .show(ui, |ui| list(ui, browser, cards, &results, selected))
        .inner
}

/// Draws the search field, the filter chips, and the order and grouping on one row.
fn controls(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    facets: &[Facet],
) {
    let search = egui::TextEdit::singleline(&mut browser.browse.query)
        .hint_text(SEARCH_HINT)
        .desired_width(f32::INFINITY);
    ui.add(search).on_hover_text(
        "Type words to search the titles, labels and parameters. \
         A term such as rig:CFD or angle>=8 asks about one parameter, and a leading minus excludes.",
    );

    if has_nothing_to_browse_by(cards) {
        tip(ui);
    }

    ui.horizontal_wrapped(|ui| {
        add_filter_menu(ui, browser, cards, facets);
        chips(ui, browser);
    });

    order_and_group(ui, browser, facets);
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

/// Draws the "Add filter" button and the menu it opens.
///
/// The menu has two stages: the parameters worth filtering on, and then the values of the one chosen. Two stages
/// rather than one keeps the menu the same height whether the collection offers three parameters or three hundred,
/// and it is what lets the menu show the count each value would leave, which a flat list has no room for.
fn add_filter_menu(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    facets: &[Facet],
) {
    let response = ui
        .button("+ Filter")
        .on_hover_text("Narrow the list by one of the figures' parameters.");
    if response.clicked() {
        // Each opening starts at the list of parameters, because the parameter wanted this time is rarely the one
        // wanted last time, and an empty search field is the fastest way to any of them.
        browser.menu = Menu::Parameters {
            search: String::new(),
        };
    }
    let open = egui::Popup::from_toggle_button_response(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(MENU_WIDTH)
        .show(|ui| menu(ui, browser, cards, facets));
    if open.is_none() {
        browser.menu = Menu::Closed;
    }
}

/// Draws whichever stage of the "Add filter" menu is open.
fn menu(ui: &mut egui::Ui, browser: &mut FigureBrowser, cards: &[FigureCard], facets: &[Facet]) {
    ui.set_max_width(MENU_WIDTH);
    let menu = std::mem::take(&mut browser.menu);
    browser.menu = match menu {
        Menu::Closed | Menu::Parameters { .. } => {
            let search = match menu {
                Menu::Parameters { search } => search,
                _ => String::new(),
            };
            parameter_menu(ui, browser, facets, search)
        }
        Menu::Values { key, search } => match facets.iter().find(|facet| facet.key == key) {
            // The collection changed under the menu and the parameter is gone; the list of parameters is the only
            // honest thing to show.
            None => parameter_menu(ui, browser, facets, String::new()),
            Some(facet) => value_menu(ui, browser, cards, facet, search),
        },
    };
}

/// Draws the first stage of the menu: the parameters worth filtering on, most useful first.
fn parameter_menu(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    facets: &[Facet],
    mut search: String,
) -> Menu {
    ui.add(
        egui::TextEdit::singleline(&mut search)
            .hint_text("Which parameter?")
            .desired_width(ui.available_width()),
    );
    let wanted = search.to_lowercase();
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .id_salt("ironlab_browser_parameters")
        .max_height(MENU_LIST_HEIGHT)
        .show(ui, |ui| {
            let mut offered = 0;
            for facet in facets {
                // A parameter that takes one value divides nothing, so it is never offered: choosing it would leave
                // the list exactly as it is.
                if facet.cardinality() < 2 || !facet.key.name().to_lowercase().contains(&wanted) {
                    continue;
                }
                offered += 1;
                let held = browser.browse.filter(&facet.key).is_some();
                let label = facet.key.name().to_owned();
                let detail = format!(
                    "{} values, on {} of the figures",
                    facet.cardinality(),
                    facet.present
                );
                if ui
                    .add(
                        egui::Button::selectable(held, two_line(ui, &label, &detail))
                            .min_size(egui::vec2(ui.available_width(), 0.0)),
                    )
                    .on_hover_text(detail)
                    .clicked()
                {
                    chosen = Some(facet.key.clone());
                }
            }
            if offered == 0 {
                ui.label(egui::RichText::new("No parameter of these figures divides them.").weak());
            }
        });
    match chosen {
        Some(key) => Menu::Values {
            key,
            search: String::new(),
        },
        None => Menu::Parameters { search },
    }
}

/// Draws the second stage of the menu: the values of one parameter, with the count each would leave.
fn value_menu(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    facet: &Facet,
    mut search: String,
) -> Menu {
    let mut back = false;
    ui.horizontal(|ui| {
        back = ui.button("All parameters").clicked();
        ui.label(egui::RichText::new(facet.key.name()).strong());
    });
    if back {
        return Menu::Parameters {
            search: String::new(),
        };
    }

    if facet.kind.is_numeric() {
        range_control(ui, browser, facet);
        return Menu::Values {
            key: facet.key.clone(),
            search,
        };
    }

    let counts = browser.browse.counts(cards, facet);
    let chosen: Vec<FacetValue> = match browser.browse.filter(&facet.key).map(|f| &f.constraint) {
        Some(Constraint::AnyOf(values)) => values.clone(),
        _ => Vec::new(),
    };

    if facet.cardinality() > MENU_VALUES {
        ui.add(
            egui::TextEdit::singleline(&mut search)
                .hint_text("Which value?")
                .desired_width(ui.available_width()),
        );
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

    let mut toggled = None;
    egui::ScrollArea::vertical()
        .id_salt("ironlab_browser_values")
        .max_height(MENU_LIST_HEIGHT)
        .show(ui, |ui| {
            for (value, count) in values.iter().take(MENU_VALUES) {
                let held = chosen.contains(value);
                let text = value.text();
                // A value that would leave nothing is shown rather than hidden, and disabled: a reader reaching for
                // it learns that the collection has nothing there, where a value that vanished as they reached
                // would look like a fault.
                let enabled = held || *count > 0;
                let mut flag = held;
                let response = ui.add_enabled(
                    enabled,
                    egui::Checkbox::new(&mut flag, two_line(ui, &text, &count.to_string())),
                );
                let label = format!("{text}, {count}");
                let is_enabled = enabled;
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::WidgetType::Checkbox,
                        is_enabled,
                        flag,
                        label.clone(),
                    )
                });
                if response.changed() {
                    toggled = Some((*value).clone());
                }
            }
            if values.is_empty() {
                ui.label(egui::RichText::new("No value matches.").weak());
            }
        });

    if let Some(value) = toggled {
        browser.browse.toggle(&facet.key, &value);
    }
    Menu::Values {
        key: facet.key.clone(),
        search,
    }
}

/// Draws the control of a numeric parameter: the two ends of the range kept.
///
/// egui's slider carries one value, so a range is two number fields rather than a two-ended slider. They are the
/// controls the property editor already uses for a number, and they say exactly what they mean, which a pair of
/// handles on one track does not.
fn range_control(ui: &mut egui::Ui, browser: &mut FigureBrowser, facet: &Facet) {
    let Some((least, most)) = facet.range else {
        ui.label(egui::RichText::new("The parameter holds no number to compare.").weak());
        return;
    };
    let (mut low, mut high) = match browser.browse.filter(&facet.key).map(|f| &f.constraint) {
        Some(Constraint::Between { low, high }) => (*low, *high),
        _ => (least, most),
    };
    // A step of a thousandth of the span moves the end across the whole range in a drag of reasonable length,
    // whatever the parameter is measured in.
    let speed = ((most - least) / 1000.0).abs().max(f64::MIN_POSITIVE);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("from");
        changed |= ui
            .add(
                egui::DragValue::new(&mut low)
                    .speed(speed)
                    .range(least..=most),
            )
            .changed();
        ui.label("to");
        changed |= ui
            .add(
                egui::DragValue::new(&mut high)
                    .speed(speed)
                    .range(least..=most),
            )
            .changed();
    });
    ui.label(egui::RichText::new(format!("{} of the figures carry it", facet.present)).weak());
    if ui.button("Whole range").clicked() {
        browser.browse.remove(&facet.key);
        return;
    }
    if changed {
        browser.browse.set_range(facet, low, high);
    }
}

/// Draws one chip per filter, and the control that takes them all back.
///
/// A chip names the parameter in small text and what it was narrowed to in ordinary text, and carries
/// [`crate::style::REMOVE`] to say that clicking it takes the filter away. The words are what the chip means; the
/// mark is there to be found at a glance among several of them.
fn chips(ui: &mut egui::Ui, browser: &mut FigureBrowser) {
    let mut remove = None;
    for filter in &browser.browse.filters {
        let text = match &filter.constraint {
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
        let name = filter.key.name();
        let label = format!("{name}: {text} {}", crate::style::REMOVE);
        let mut job = LayoutJob::default();
        job.append(
            name,
            0.0,
            egui::TextFormat {
                font_id: monospace_small(ui),
                color: ui.visuals().selection.stroke.color,
                valign: egui::Align::Center,
                ..Default::default()
            },
        );
        job.append(
            &format!(" {text} {}", crate::style::REMOVE),
            0.0,
            egui::TextFormat {
                font_id: egui::TextStyle::Body.resolve(ui.style()),
                color: ui.visuals().strong_text_color(),
                valign: egui::Align::Center,
                ..Default::default()
            },
        );
        let chip = egui::Button::new(job)
            .fill(ui.visuals().selection.bg_fill)
            .stroke(ui.visuals().selection.stroke);
        let response = ui.add(chip).on_hover_text("Click to remove this filter.");
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone())
        });
        if response.clicked() {
            remove = Some(filter.key.clone());
        }
    }
    if let Some(key) = remove {
        browser.browse.remove(&key);
    }
    if !browser.browse.is_unfiltered()
        && ui
            .button("Clear")
            .on_hover_text("Remove every filter and clear the search.")
            .clicked()
    {
        browser.browse.clear();
    }
}

/// Draws the order and the grouping on one row: what the order is taken from, which way it runs, and what the list
/// is grouped by.
///
/// The two share a row because they are two settings of the same list and neither needs the width of the panel;
/// the width left after their captions and the direction button is split between the two boxes.
fn order_and_group(ui: &mut egui::Ui, browser: &mut FigureBrowser, facets: &[Facet]) {
    let descending = browser.browse.sort.descending;
    let direction = if descending {
        format!("Descending {}", crate::style::DESCENDING)
    } else {
        format!("Ascending {}", crate::style::ASCENDING)
    };
    ui.horizontal(|ui| {
        let spacing = ui.spacing().item_spacing.x;
        let measure = |ui: &egui::Ui, text: &str, style: egui::TextStyle| {
            ui.painter()
                .layout_no_wrap(
                    text.to_owned(),
                    style.resolve(ui.style()),
                    egui::Color32::WHITE,
                )
                .size()
                .x
        };
        let reserved = measure(ui, "SORT", egui::TextStyle::Small)
            + measure(ui, "GROUP", egui::TextStyle::Small)
            + measure(ui, &direction, egui::TextStyle::Button)
            + 2.0 * ui.spacing().button_padding.x
            + 6.0 * spacing;
        let box_width = ((ui.available_width() - reserved) / 2.0).max(56.0);

        heading_label(ui, "SORT");
        let selected = match &browser.browse.sort.key {
            SortKey::Title => "Title".to_owned(),
            SortKey::Parameter(name) => name.clone(),
        };
        egui::ComboBox::from_id_salt("ironlab_browser_sort")
            .selected_text(selected)
            .width(box_width)
            .show_ui(ui, |ui| {
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
        // The caption says which way the order runs and the arrow shows it. The arrow never stands alone: a mark
        // on its own would leave the control unreadable to anyone who does not take the mark in.
        if ui
            .button(direction)
            .on_hover_text("Reverse the order.")
            .clicked()
        {
            browser.browse.sort.descending = !descending;
        }

        heading_label(ui, "GROUP");
        let selected = match &browser.browse.group {
            None => "None".to_owned(),
            Some(key) => key.name().to_owned(),
        };
        egui::ComboBox::from_id_salt("ironlab_browser_group")
            .selected_text(selected)
            .width(box_width)
            .show_ui(ui, |ui| {
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

/// Draws the caption of a control: a short word in small capitals, quieter than the control it names.
fn heading_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).small().weak());
}

/// The small monospaced font, which the second line of a row, the name on a chip and the count on a heading are
/// set in: the details of a figure read as data beside its title.
fn monospace_small(ui: &egui::Ui) -> egui::FontId {
    egui::FontId::new(
        egui::TextStyle::Small.resolve(ui.style()).size,
        egui::FontFamily::Monospace,
    )
}

/// Lays out one line of text, cut short with an ellipsis where it would run past `width`.
fn truncated(
    ui: &egui::Ui,
    text: &str,
    font_id: egui::FontId,
    color: egui::Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font_id, color);
    job.wrap = egui::text::TextWrapping {
        max_width: width,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('\u{2026}'),
    };
    ui.painter().layout_job(job)
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
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("No figure matches").strong());
            ui.label(egui::RichText::new("Take away a filter, or clear them all.").weak());
        });
        return response;
    }

    let rows = flatten(browser, results);
    let height = row_height(ui);
    let mut toggle = None;
    egui::ScrollArea::vertical()
        .id_salt("ironlab_browser_list")
        .auto_shrink([false, false])
        .show_rows(ui, height, rows.len(), |ui, range| {
            ui.set_min_width(ui.available_width());
            for index in range {
                match &rows[index] {
                    Row::Heading { name, count, open } => {
                        if heading(ui, name, *count, *open, height) {
                            toggle = Some(name.clone());
                        }
                    }
                    Row::Figure(card) => {
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

/// The height of one row of the list: a line of ordinary text, a line of small text, and the padding above and
/// below.
///
/// Every row is the same height, headings included, because [`egui::ScrollArea::show_rows`] finds a row by
/// multiplying rather than by laying out the rows above it.
fn row_height(ui: &egui::Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body)
        + ui.text_style_height(&egui::TextStyle::Small)
        + 2.0 * ROW_PADDING.y
}

/// Flattens the grouped result into the rows to draw, leaving out the figures of the groups that are closed.
fn flatten(browser: &FigureBrowser, results: &crate::browse::Results) -> Vec<Row> {
    let mut rows = Vec::new();
    for group in &results.groups {
        match &group.name {
            None => rows.extend(group.members.iter().copied().map(Row::Figure)),
            Some(name) => {
                let open = !browser.closed.contains(name);
                rows.push(Row::Heading {
                    name: name.clone(),
                    count: group.members.len(),
                    open,
                });
                if open {
                    rows.extend(group.members.iter().copied().map(Row::Figure));
                }
            }
        }
    }
    rows
}

/// Draws the heading of a group: a band across the list carrying a triangle that says whether the group is open,
/// the name of the group in small capitals, and how many figures it holds. Returns whether it was clicked.
fn heading(ui: &mut egui::Ui, name: &str, count: usize, open: bool, height: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    let label = format!(
        "{name}, {count} figures, {}",
        if open { "open" } else { "closed" }
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, open, label.clone())
    });
    if !ui.is_rect_visible(rect) {
        return response.clicked();
    }
    let visuals = ui.visuals();
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, visuals.faint_bg_color);
    let hairline = visuals.widgets.noninteractive.bg_stroke;
    painter.hline(rect.x_range(), rect.top(), hairline);
    painter.hline(rect.x_range(), rect.bottom(), hairline);

    // The triangle is painted rather than typed, as egui paints the one on a collapsing header, so that it needs
    // no character the fonts might lack.
    let centre = egui::pos2(rect.min.x + ROW_PADDING.x + 4.0, rect.center().y);
    let points = if open {
        vec![
            centre + egui::vec2(-4.0, -2.0),
            centre + egui::vec2(4.0, -2.0),
            centre + egui::vec2(0.0, 3.0),
        ]
    } else {
        vec![
            centre + egui::vec2(-2.0, -4.0),
            centre + egui::vec2(-2.0, 4.0),
            centre + egui::vec2(3.0, 0.0),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        points,
        visuals.weak_text_color(),
        egui::Stroke::NONE,
    ));

    let count = painter.layout_no_wrap(
        count.to_string(),
        monospace_small(ui),
        visuals.weak_text_color(),
    );
    let count_x = rect.max.x - ROW_PADDING.x - count.size().x;
    painter.galley(
        egui::pos2(count_x, rect.center().y - count.size().y / 2.0),
        count,
        visuals.weak_text_color(),
    );

    let text_x = rect.min.x + ROW_PADDING.x + 16.0;
    let title = truncated(
        ui,
        &name.to_uppercase(),
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.weak_text_color(),
        count_x - text_x - ROW_PADDING.x,
    );
    painter.galley(
        egui::pos2(text_x, rect.center().y - title.size().y / 2.0),
        title,
        visuals.weak_text_color(),
    );
    response
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
/// the value the list is ordered by when it is ordered by a parameter, and the figure's labels otherwise. Rows
/// alternate between the panel and a fainter fill so that the eye can follow one across, and the row of the figure
/// shown carries the selection colour with a bar down its left edge, so that it can be found in a long list at a
/// glance.
///
/// The row is painted rather than built from a button, because a button centres its caption and frames itself,
/// and neither is what a list looks like.
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
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    let label = format!("{}, {detail}", card.title);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            true,
            selected,
            label.clone(),
        )
    });
    if !ui.is_rect_visible(rect) {
        return response.clicked();
    }

    let visuals = ui.visuals();
    let (fill, title_color, detail_color) = if selected {
        (
            visuals.selection.bg_fill,
            visuals.strong_text_color(),
            visuals.selection.stroke.color,
        )
    } else {
        let fill = if response.hovered() {
            visuals.widgets.hovered.weak_bg_fill
        } else if striped {
            visuals.faint_bg_color
        } else {
            egui::Color32::TRANSPARENT
        };
        (fill, visuals.text_color(), visuals.weak_text_color())
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, fill);
    if selected {
        painter.rect_filled(
            egui::Rect::from_min_size(rect.min, egui::vec2(SELECTED_BAR, rect.height())),
            0.0,
            visuals.selection.stroke.color,
        );
    }

    let text_width = rect.width() - 2.0 * ROW_PADDING.x - SELECTED_BAR;
    let origin = rect.min + ROW_PADDING + egui::vec2(SELECTED_BAR, 0.0);
    let title = truncated(
        ui,
        &card.title,
        egui::TextStyle::Body.resolve(ui.style()),
        title_color,
        text_width,
    );
    let title_height = title.size().y;
    painter.galley(origin, title, title_color);
    if !detail.is_empty() {
        let detail = truncated(ui, &detail, monospace_small(ui), detail_color, text_width);
        painter.galley(origin + egui::vec2(0.0, title_height), detail, detail_color);
    }
    response.clicked()
}

/// Draws the strip that says how much of the collection is left, and the control that takes every choice back.
fn count_strip(ui: &mut egui::Ui, browser: &mut FigureBrowser, matched: usize, total: usize) {
    ui.horizontal(|ui| {
        let text = if matched == total {
            format!("{total} figures")
        } else {
            format!("{matched} of {total} figures")
        };
        ui.label(egui::RichText::new(text).monospace().small().weak());
        if !browser.browse.is_unfiltered() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Reset")
                    .on_hover_text("Remove every filter and clear the search.")
                    .clicked()
                {
                    browser.browse.clear();
                }
            });
        }
    });
}

/// Lays out a line of ordinary text above a line of quieter, smaller text, which is the shape of every row of the
/// list and of every entry of the menu.
fn two_line(ui: &egui::Ui, title: &str, detail: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        title,
        0.0,
        egui::TextFormat {
            font_id: egui::TextStyle::Body.resolve(ui.style()),
            color: ui.visuals().text_color(),
            ..Default::default()
        },
    );
    if !detail.is_empty() {
        job.append(
            &format!("\n{detail}"),
            0.0,
            egui::TextFormat {
                font_id: egui::TextStyle::Small.resolve(ui.style()),
                color: ui.visuals().weak_text_color(),
                ..Default::default()
            },
        );
    }
    job
}
