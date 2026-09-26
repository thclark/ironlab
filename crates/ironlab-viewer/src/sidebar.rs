//! The figure browser: a left-hand panel that narrows a collection of figures down to the one to look at.
//!
//! It draws what [`crate::browse`] works out, and holds nothing but the state of the controls themselves. The shape
//! is deliberate, and it is the one that survives an unbounded number of parameters:
//!
//! - **The controls are small and the list is large.** A panel of checkboxes, one section per parameter, grows with
//!   the collection until the list of figures starts below the bottom of the window. Here the controls take three
//!   rows whatever the collection holds, and everything below them is the list.
//! - **Filters are chips, added from a menu that opens within the panel.** "Edit", at the right of the "Filters"
//!   caption, opens beneath it a list of the parameters worth filtering on, most useful first, and then of that
//!   parameter's values with the count each would leave. The menu pushes the controls and the list down rather
//!   than floating over them, so it stays open while values are chosen one at a time, and "Done" shuts it. What
//!   has been chosen reads back as a row of chips beneath the menu, where their coming and going cannot move it;
//!   clicking a chip takes it away, and "Clear all", beside "Edit", takes every one away.
//! - **Every setting has a row of its own.** The filters, the order and the grouping are each a caption at the left
//!   of a row and a control at the right, so that the three are read the same way.
//! - **The search field is the whole of it for anyone who would rather type.** `rig:CFD angle>=8 -stalled` does what
//!   three chips do. It is also the only way to ask something the menus do not offer, so the interface never has to
//!   grow a control for every question.
//!
//! The panel is drawn to one set of measurements: each block of the controls sits in [`BLOCK_PADDING`], a row of
//! the list in [`ROW_PADDING`], and every colour and size comes from [`crate::style`]. They are the measurements of
//! the design that was approved for the browser, and the panel is meant to look exactly like it.
//!
//! The list is drawn with [`egui::ScrollArea::show_rows`], which lays out only the rows on screen, so a collection
//! of a few hundred figures costs the same per frame as a collection of ten.
//!
//! Every mark drawn here is one of [`crate::style::MARKS`], which [`crate::style::INTERFACE_CHARACTERS`] promises
//! the fonts carry, and each augments words rather than standing in for them: a chip says which parameter it
//! narrows and to what, and carries a cross to say that clicking it takes that away. The control that reverses
//! an order says "Ascending" or "Descending", and beside the word carries the triangle of a combo box, turned to
//! point the way the order runs; it is painted, as the combo box paints its own, and is no character at all.

use std::collections::{BTreeMap, BTreeSet};

use egui::text::LayoutJob;

use crate::browse::{
    Browse, Constraint, Facet, FacetKey, FacetValue, FigureCard, SortKey, describe_facets,
    has_nothing_to_browse_by,
};
use crate::style;

/// The width the browser opens at, in egui points: wide enough for the order and the grouping to share one row and
/// for a figure's title not to be cut short in the ordinary case.
pub const WIDTH: f32 = 336.0;

/// The narrowest the browser can be dragged, in egui points, below which a title tells the reader nothing.
pub const MIN_WIDTH: f32 = 220.0;

/// The identifier of the browser's panel, which is also what a test loads its geometry by.
pub const PANEL_ID: &str = "ironlab_figure_browser";

/// The room around each block of the controls, in egui points: the search field, the row of chips and the row of
/// order and grouping each sit in their own, so that two blocks are twice this apart.
const BLOCK_PADDING: egui::Margin = egui::Margin {
    left: 10,
    right: 10,
    top: 8,
    bottom: 8,
};

/// The room between the controls and the rule beneath them, and between the rule and the list, in egui points.
const RULE_MARGIN: i8 = 6;

/// The room between the menu's frame and the edge of the panel, in egui points: a block's padding sideways, nothing
/// above because the row of chips has its own padding beneath, and a block's padding below.
const MENU_MARGIN: egui::Margin = egui::Margin {
    left: 10,
    right: 10,
    top: 0,
    bottom: 8,
};

/// The room inside the menu's frame, in egui points.
const MENU_PADDING: egui::Margin = egui::Margin {
    left: 10,
    right: 10,
    top: 8,
    bottom: 8,
};

/// The radius of the corners of the menu's frame, in egui points.
const MENU_CORNER: u8 = 4;

/// The room between the menu's search field and the list beneath it, in egui points.
const MENU_FIELD_GAP: f32 = 5.0;

/// The height beyond which the menu's list of parameters scrolls, in egui points.
const MENU_PARAMETERS_HEIGHT: f32 = 210.0;

/// The height beyond which the menu's list of values scrolls, in egui points.
const MENU_VALUES_HEIGHT: f32 = 240.0;

/// How many values of a parameter the menu shows before the rest are reached by asking for more.
const MENU_VALUES: usize = 14;

/// The number of values above which the menu offers a search field for them, because a list that long is faster
/// to type into than to scroll.
const MENU_SEARCH_VALUES: usize = 60;

/// The room above the body of the menu's second page, and above its "Done" control, in egui points.
const MENU_BODY_GAP: (f32, f32) = (2.0, 8.0);

/// The width of the combo boxes of the order and the grouping, in egui points, when the panel has room for it.
const COMBO_WIDTH: f32 = 150.0;

/// The narrowest a combo box of the order or the grouping is drawn, in egui points.
const COMBO_MIN_WIDTH: f32 = 56.0;

/// The room between the edge of a chip and its words, in egui points, the same at both ends so that the mark at
/// the right end of a chip has the room the name at its left end has.
const CHIP_PADDING: egui::Margin = egui::Margin::symmetric(7, 2);

/// The size of the mark on a chip that says clicking it takes the filter away, in points: a size above the words
/// of the chip, because the cross is a small glyph and at their size it reads as a speck.
const REMOVE_MARK_SIZE_PT: f32 = 14.0;

/// How far below the size of its words the mark on "Clear all" is set, in points: the circling arrow is a tall
/// glyph, and at the size of the words it stands above them rather than beside them.
const CLEAR_ALL_MARK_STEP_PT: f32 = 2.0;

/// The room between the edge of a text field and its text, in egui points.
const FIELD_PADDING: egui::Margin = egui::Margin::symmetric(7, 4);

/// The size of the text of the menu's search field, in points, which is set in monospace because what is typed
/// there is the name of a parameter or a value, and reads as data.
const MENU_FIELD_SIZE_PT: f32 = 13.0;

/// The room between the edge of an entry of the menu's list of parameters and its text, in egui points.
const ENTRY_PADDING: egui::Vec2 = egui::vec2(7.0, 3.0);

/// The room between the edge of a checkbox row and its contents, in egui points.
const CHECK_PADDING: egui::Vec2 = egui::vec2(7.0, 2.0);

/// The side of the box of a checkbox, in egui points.
const CHECK_BOX: f32 = 14.0;

/// The room between the box of a checkbox and its label, in egui points.
const CHECK_GAP: f32 = 7.0;

/// The opacity a value that would leave nothing is drawn at.
const FADED: f32 = 0.42;

/// How many bars the histogram above a numeric range has.
const HISTOGRAM_BINS: usize = 18;

/// The height of the histogram above a numeric range, in egui points.
const HISTOGRAM_HEIGHT: f32 = 26.0;

/// The room between two bars of the histogram, in egui points.
const HISTOGRAM_GAP: f32 = 1.0;

/// The room above and below the histogram, in egui points.
const HISTOGRAM_MARGIN: (f32, f32) = (2.0, 4.0);

/// The room between the edge of a row of the list and its text, in egui points, sideways and up and down.
const ROW_PADDING: egui::Vec2 = egui::vec2(9.0, 5.0);

/// The room between the two lines of a row of the list, in egui points.
const LINE_GAP: f32 = 2.0;

/// The width of the bar drawn down the left edge of the row of the figure shown, in egui points.
const SELECTED_BAR: f32 = 2.0;

/// The size of the text of a group heading, in points.
const GROUP_HEADING_SIZE_PT: f32 = 12.5;

/// The letter spacing of a group heading, as a fraction of its size: a heading in capitals needs its letters
/// spread a little to read as a word.
const GROUP_HEADING_SPACING: f32 = 0.04;

/// The letter spacing of the caption of a control, as a fraction of its size.
const CAPTION_SPACING: f32 = 0.09;

/// The room around the note shown when nothing matches, in egui points.
const EMPTY_PADDING: egui::Margin = egui::Margin {
    left: 12,
    right: 12,
    top: 22,
    bottom: 22,
};

/// The room between the two lines of the note shown when nothing matches, in egui points.
const EMPTY_GAP: f32 = 4.0;

/// The room between the edge of the strip at the foot of the panel and its text, in egui points.
const FOOT_PADDING: egui::Margin = egui::Margin::symmetric(10, 5);

/// The hint shown in the empty search field, which is also where the typed form is taught.
const SEARCH_HINT: &str = "Search, or rig:CFD angle>=8 -stalled";

/// The page that explains how to describe a figure so that it can be found again.
///
/// The browser links to it when a collection carries nothing to browse by. The address is the published site
/// rather than a path, because the viewer is a desktop program with no documentation beside it; a test checks that
/// the page it names is still in `docs/`, so the link cannot rot unnoticed when a page is renamed.
pub const DESCRIBING_FIGURES_URL: &str = "https://ironlab.org/guides/describing-figures/";

/// What the browser is showing in its "+ Filter" menu.
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
    /// What the "+ Filter" menu is showing.
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
    // The panel has no margin of its own: each block of the controls carries its padding, and the rows of the list
    // run from edge to edge.
    let frame = egui::Frame::new().fill(ui.visuals().panel_fill);
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
    let fill = ui.visuals().panel_fill;

    egui::Panel::top(egui::Id::new(PANEL_ID).with("controls"))
        .resizable(false)
        .frame(egui::Frame::new().fill(fill).inner_margin(egui::Margin {
            bottom: RULE_MARGIN,
            ..egui::Margin::ZERO
        }))
        .show(ui, |ui| {
            style::compact(ui);
            controls(ui, browser, cards, &facets);
        });
    // The strip at the foot is given its height before the list is drawn, so that the list never takes the room the
    // strip needs and the strip never moves as the list grows.
    let foot = foot_height(ui);
    egui::Panel::bottom(egui::Id::new(PANEL_ID).with("count"))
        .exact_size(foot)
        .frame(
            egui::Frame::new()
                .fill(style::FOOT_FILL)
                .inner_margin(FOOT_PADDING),
        )
        .show(ui, |ui| {
            style::compact(ui);
            count_strip(ui, browser, results.matched, results.total);
        });
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(fill).inner_margin(egui::Margin {
            top: RULE_MARGIN,
            ..egui::Margin::ZERO
        }))
        .show(ui, |ui| list(ui, browser, cards, &results, selected))
        .inner
}

/// The height of the strip at the foot of the panel: room for its "Show all" control, so that the strip is the same
/// height whether or not the control is shown.
fn foot_height(ui: &egui::Ui) -> f32 {
    let caption = ui.fonts_mut(|fonts| {
        fonts.row_height(&egui::FontId::proportional(style::SMALL_BUTTON_SIZE_PT))
    });
    let button = caption + 2.0 * style::BUTTON_PADDING.y;
    button.max(ui.spacing().interact_size.y) + f32::from(FOOT_PADDING.top + FOOT_PADDING.bottom)
}

/// Draws one block of the controls in its padding, as wide as the panel.
fn block<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(BLOCK_PADDING)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        })
        .inner
}

/// Draws the search field, the filter chips and the menu they are added from, and the order and grouping on one row.
fn controls(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    facets: &[Facet],
) {
    block(ui, |ui| {
        text_field(
            ui,
            &mut browser.browse.query,
            SEARCH_HINT,
            egui::Id::new(PANEL_ID).with("search"),
            false,
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
        ui.horizontal(|ui| {
            caption(ui, "FILTERS");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Laid out from the right edge inwards: Edit at the edge, and Clear all beside it when there is
                // anything to clear.
                let edit = edit_filters_button(ui, browser);
                clear_all_button(ui, browser, edit.rect.height());
            });
        });
    });

    if browser.menu != Menu::Closed {
        menu_frame(ui, browser, cards, facets);
    }

    if !browser.browse.filters.is_empty() {
        block(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(style::ROW_GAP, style::ROW_GAP);
            ui.horizontal_wrapped(|ui| chips(ui, browser));
        });
    }

    block(ui, |ui| sort_row(ui, browser, facets));
    block(ui, |ui| group_row(ui, browser, facets));
}

/// Draws a single-line text field as wide as the room it is given: a dark well with an outline, which takes the
/// selection colour while the field has focus.
///
/// The field is drawn by hand rather than by egui's own frame so that a field the panel draws and a field the menu
/// draws are the same field, and so that its padding and outline are the ones of the design.
fn text_field(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: &str,
    id: egui::Id,
    monospace: bool,
) -> egui::Response {
    let focused = ui.memory(|memory| memory.has_focus(id));
    let stroke = if focused {
        ui.visuals().selection.stroke
    } else {
        egui::Stroke::new(1.0, style::STROKE)
    };
    let font = if monospace {
        egui::FontId::monospace(MENU_FIELD_SIZE_PT)
    } else {
        egui::TextStyle::Body.resolve(ui.style())
    };
    egui::Frame::new()
        .fill(style::FIELD)
        .stroke(stroke)
        .corner_radius(2)
        .inner_margin(FIELD_PADDING)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .id(id)
                    .hint_text(hint)
                    .font(font)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .desired_width(f32::INFINITY),
            )
        })
        .inner
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

/// Draws the "Edit" control, which opens the menu when it is shut and shuts it when it is open, and carries the mark
/// that says which it will do: [`crate::style::EXPAND`] when the menu is shut, [`crate::style::COLLAPSE`] when it is
/// open.
fn edit_filters_button(ui: &mut egui::Ui, browser: &mut FigureBrowser) -> egui::Response {
    let open = browser.menu != Menu::Closed;
    let mark = if open { style::COLLAPSE } else { style::EXPAND };
    let text = format!("{mark} Edit");
    let response = ui.button(&text).on_hover_text(if open {
        "Shut the menu, keeping what has been chosen."
    } else {
        "Narrow the list by one of the figures' parameters."
    });
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, open, text.clone())
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
    response
}

/// Draws the menu in its frame, beneath the "Filters" row and above the chips.
///
/// The menu is a block of the panel rather than a popup, so that it pushes what is beneath it down and stays open
/// until it is shut: a reader ticking values one at a time keeps their place, where a popup would shut the moment
/// the pointer strayed outside it.
fn menu_frame(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    cards: &[FigureCard],
    facets: &[Facet],
) {
    egui::Frame::new()
        .fill(style::MENU_FILL)
        .stroke(egui::Stroke::new(1.0, style::STROKE))
        .corner_radius(MENU_CORNER)
        .outer_margin(MENU_MARGIN)
        .inner_margin(MENU_PADDING)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            menu(ui, browser, cards, facets);
        });
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
/// values it takes and how much of the collection carries it.
fn parameter_menu(
    ui: &mut egui::Ui,
    browser: &mut FigureBrowser,
    total: usize,
    facets: &[Facet],
    mut search: String,
) -> Menu {
    text_field(
        ui,
        &mut search,
        "Filter on…",
        egui::Id::new(PANEL_ID).with("parameter_search"),
        true,
    );
    ui.add_space(MENU_FIELD_GAP);
    let wanted = search.to_lowercase();
    let mut chosen = None;
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
                if parameter_entry(ui, facet.key.name(), &detail, held) {
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
            all: false,
        },
        None => Menu::Parameters { search },
    }
}

/// Draws one entry of the list of parameters: its name in ordinary text, then how many values it takes and how much
/// of the collection carries it, in small monospaced text. Returns whether it was clicked.
///
/// An entry whose parameter already has a filter is drawn in the selection colour, so that the reader can see at a
/// glance which parameters the chips came from.
fn parameter_entry(ui: &mut egui::Ui, name: &str, detail: &str, held: bool) -> bool {
    let height = ui.text_style_height(&egui::TextStyle::Body) + 2.0 * ENTRY_PADDING.y;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    let label = format!("{name}, {detail}");
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, held, label.clone())
    });
    if !ui.is_rect_visible(rect) {
        return response.clicked();
    }
    let visuals = ui.visuals();
    let (fill, name_color, detail_color) = if held {
        (
            visuals.selection.bg_fill,
            visuals.strong_text_color(),
            style::SELECTED_DETAIL,
        )
    } else if response.hovered() {
        (
            style::WIDGET,
            visuals.text_color(),
            visuals.weak_text_color(),
        )
    } else {
        (
            egui::Color32::TRANSPARENT,
            visuals.text_color(),
            visuals.weak_text_color(),
        )
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, fill);

    let mut job = LayoutJob::default();
    job.append(
        name,
        0.0,
        egui::TextFormat {
            font_id: egui::TextStyle::Body.resolve(ui.style()),
            color: name_color,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.append(
        detail,
        4.0,
        egui::TextFormat {
            font_id: monospace_count(),
            color: detail_color,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.wrap = single_line(rect.width() - 2.0 * ENTRY_PADDING.x);
    let galley = painter.layout_job(job);
    painter.galley(
        egui::pos2(
            rect.min.x + ENTRY_PADDING.x,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        name_color,
    );
    response
        .on_hover_text("Choose which of its values to keep.")
        .clicked()
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
        back = quiet_button(ui, "Back to all parameters")
            .on_hover_text("Choose another parameter.")
            .clicked();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            caption(ui, &facet.key.name().to_uppercase());
        });
    });
    if back {
        return Menu::Parameters {
            search: String::new(),
        };
    }
    ui.add_space(MENU_BODY_GAP.0);

    if facet.kind.is_numeric() {
        range_control(ui, browser, cards, facet);
    } else {
        value_list(ui, browser, cards, facet, &mut search, &mut all);
    }

    ui.add_space(MENU_BODY_GAP.1);
    if ui
        .button("Done")
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
        text_field(
            ui,
            search,
            "Which value?",
            egui::Id::new(PANEL_ID).with("value_search"),
            true,
        );
        ui.add_space(MENU_FIELD_GAP);
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
    egui::ScrollArea::vertical()
        .id_salt("ironlab_browser_values")
        .max_height(MENU_VALUES_HEIGHT)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (value, count) in values.iter().take(shown) {
                let held = chosen.contains(value);
                if check_row(ui, &value.text(), *count, held) {
                    toggled = Some((*value).clone());
                }
            }
            if values.len() > shown
                && quiet_button(ui, &format!("{} more…", values.len() - shown))
                    .on_hover_text("Show every value.")
                    .clicked()
            {
                *all = true;
            }
            if values.is_empty() {
                ui.label(egui::RichText::new("No value matches.").weak());
            }
        });

    if let Some(value) = toggled {
        browser.browse.toggle(&facet.key, &value);
    }
}

/// Draws one value of the checklist: a box, the value, and at the right the number of figures choosing it would
/// leave. Returns whether it was clicked.
///
/// A value that would leave nothing is shown rather than hidden, and faded: a reader reaching for it learns that
/// the collection has nothing there, where a value that vanished as they reached would look like a fault.
fn check_row(ui: &mut egui::Ui, text: &str, count: usize, held: bool) -> bool {
    let enabled = held || count > 0;
    let height = ui.text_style_height(&egui::TextStyle::Body) + 2.0 * CHECK_PADDING.y;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    let label = format!("{text}, {count}");
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, held, label.clone())
    });
    if !ui.is_rect_visible(rect) {
        return enabled && response.clicked();
    }
    let visuals = ui.visuals();
    let fade = if enabled { 1.0 } else { FADED };
    let painter = ui.painter();
    if enabled && response.hovered() {
        painter.rect_filled(rect, 2.0, style::WIDGET);
    }

    let box_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.min.x + CHECK_PADDING.x + CHECK_BOX / 2.0,
            rect.center().y,
        ),
        egui::Vec2::splat(CHECK_BOX),
    );
    let (fill, stroke) = if held {
        (visuals.selection.bg_fill, visuals.selection.stroke)
    } else {
        (style::WIDGET, egui::Stroke::new(1.0, style::STROKE))
    };
    painter.rect(
        box_rect,
        2.0,
        fill.gamma_multiply(fade),
        stroke,
        egui::StrokeKind::Inside,
    );
    if held {
        // The tick is painted, as egui paints its own, so that it needs no character the fonts might lack.
        painter.add(egui::Shape::line(
            vec![
                box_rect.min + egui::vec2(3.0, 7.0),
                box_rect.min + egui::vec2(6.0, 10.0),
                box_rect.min + egui::vec2(11.0, 4.0),
            ],
            egui::Stroke::new(1.5, style::BRIGHT),
        ));
    }

    let count = painter.layout_no_wrap(
        count.to_string(),
        monospace_count(),
        visuals.weak_text_color().gamma_multiply(fade),
    );
    let count_x = rect.max.x - CHECK_PADDING.x - count.size().x;
    let count_color = count.job.sections[0].format.color;
    painter.galley(
        egui::pos2(count_x, rect.center().y - count.size().y / 2.0),
        count,
        count_color,
    );

    let text_x = box_rect.max.x + CHECK_GAP;
    let text_color = visuals.text_color().gamma_multiply(fade);
    let galley = truncated(
        ui,
        text,
        egui::TextStyle::Body.resolve(ui.style()),
        text_color,
        count_x - CHECK_GAP - text_x,
    );
    painter.galley(
        egui::pos2(text_x, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    enabled && response.clicked()
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
        ui.label(egui::RichText::new("The parameter holds no number to compare.").weak());
        return;
    };
    let (mut low, mut high) = match browser.browse.filter(&facet.key).map(|f| &f.constraint) {
        Some(Constraint::Between { low, high }) => (*low, *high),
        _ => (least, most),
    };

    ui.add_space(HISTOGRAM_MARGIN.0);
    histogram(
        ui,
        &browser.browse.counts(cards, facet),
        (least, most),
        (low, high),
    );
    ui.add_space(HISTOGRAM_MARGIN.1);

    // A step of a thousandth of the span moves the end across the whole range in a drag of reasonable length,
    // whatever the parameter is measured in.
    let speed = ((most - least) / 1000.0).abs().max(f64::MIN_POSITIVE);
    let mut changed = false;
    ui.horizontal(|ui| {
        let ellipsis = ui
            .painter()
            .layout_no_wrap("…".to_owned(), monospace_count(), egui::Color32::WHITE)
            .size()
            .x;
        let field = ((ui.available_width() - ellipsis - 2.0 * style::ROW_GAP) / 2.0)
            .max(ui.spacing().interact_size.x);
        ui.spacing_mut().interact_size.x = field;
        changed |= ui
            .add(
                egui::DragValue::new(&mut low)
                    .speed(speed)
                    .range(least..=most),
            )
            .on_hover_text("The least value kept.")
            .changed();
        ui.label(
            egui::RichText::new("…")
                .monospace()
                .size(style::MONOSPACE_SMALL_SIZE_PT)
                .weak(),
        );
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
        ui.label(
            egui::RichText::new(format!(
                "{} of {} figures carry this",
                facet.present,
                cards.len()
            ))
            .monospace()
            .size(style::MONOSPACE_SMALL_SIZE_PT)
            .weak(),
        );
    }
    if changed {
        browser.browse.set_range(facet, low, high);
    }
}

/// Paints a histogram of a numeric parameter's values across the room available: one bar per bin from the least
/// value to the greatest, in the kept colour where the bin's centre lies within the range kept.
fn histogram(
    ui: &mut egui::Ui,
    counts: &BTreeMap<FacetValue, usize>,
    (least, most): (f64, f64),
    (low, high): (f64, f64),
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), HISTOGRAM_HEIGHT),
        egui::Sense::hover(),
    );
    if !ui.is_rect_visible(rect) {
        return;
    }
    let span = if most > least { most - least } else { 1.0 };
    let mut tally = [0usize; HISTOGRAM_BINS];
    for (value, count) in counts {
        if let Some(number) = value.as_number() {
            let bin = ((number - least) / span * HISTOGRAM_BINS as f64).floor();
            // The cast saturates a bin beyond the last, which the greatest value lands in, back to the last.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let bin = (bin.max(0.0) as usize).min(HISTOGRAM_BINS - 1);
            tally[bin] += count;
        }
    }
    let peak = tally.iter().copied().max().unwrap_or(0).max(1);
    #[allow(clippy::cast_precision_loss)]
    let bins = HISTOGRAM_BINS as f32;
    let bar = (rect.width() - (bins - 1.0) * HISTOGRAM_GAP) / bins;
    let painter = ui.painter();
    for (index, count) in tally.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let (index_f, count_f, peak_f) = (index as f32, *count as f32, peak as f32);
        let height = (count_f / peak_f * HISTOGRAM_HEIGHT).round().max(1.0);
        let x = rect.min.x + index_f * (bar + HISTOGRAM_GAP);
        let centre = least + (f64::from(index_f) + 0.5) / f64::from(bins) * span;
        let kept = centre >= low && centre <= high;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x, rect.max.y - height),
                egui::pos2(x + bar, rect.max.y),
            ),
            0.0,
            if kept {
                style::HISTOGRAM_KEPT
            } else {
                style::HISTOGRAM_BAR
            },
        );
    }
}

/// Draws one chip per filter.
///
/// A chip names the parameter in small monospaced text and what it was narrowed to in ordinary text, and carries
/// [`crate::style::REMOVE`] to say that clicking it takes the filter away. The words are what the chip means; the
/// mark is there to be found at a glance among several of them. A chip on the labels is drawn in the green of a
/// tag, so that it reads as a filter on labels rather than on a parameter.
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
        if chip(ui, &filter.key, &text).clicked() {
            remove = Some(filter.key.clone());
        }
    }
    if let Some(key) = remove {
        browser.browse.remove(&key);
    }
}

/// Draws the control that takes every filter away, beside "Edit" on the "Filters" row, when there is a filter to
/// take away. It is the same button as "Edit", `height` tall as "Edit" is, with [`crate::style::RESTORE`] before
/// its words and set two points smaller than them.
fn clear_all_button(ui: &mut egui::Ui, browser: &mut FigureBrowser, height: f32) {
    if browser.browse.filters.is_empty() {
        return;
    }
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let color = ui.visuals().widgets.inactive.fg_stroke.color;
    let mut caption = LayoutJob::default();
    caption.append(
        style::RESTORE,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(font_id.size - CLEAR_ALL_MARK_STEP_PT),
            color,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    caption.append(
        " Clear all",
        0.0,
        egui::TextFormat {
            font_id,
            color,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    // A caption of two sizes lays out a point shorter than one line of the button font, so the button is given
    // `height`, which is the height "Edit" was just drawn at, and the two stand level.
    if ui
        .add(egui::Button::new(caption).min_size(egui::vec2(0.0, height)))
        .on_hover_text("Remove every filter.")
        .clicked()
    {
        browser.browse.filters.clear();
    }
}

/// The colours a chip is drawn in: its fill, its fill under the pointer, its outline, the name of its parameter and
/// its value.
struct ChipPalette {
    fill: egui::Color32,
    hover: egui::Color32,
    stroke: egui::Color32,
    key: egui::Color32,
    text: egui::Color32,
}

/// The colours of the chip of a filter on `key`.
fn chip_palette(key: &FacetKey) -> ChipPalette {
    match key {
        FacetKey::Labels => ChipPalette {
            fill: style::LABEL_CHIP_FILL,
            hover: style::LABEL_CHIP_HOVER,
            stroke: style::LABEL_CHIP_STROKE,
            key: style::LABEL_CHIP_KEY,
            text: style::LABEL_CHIP_TEXT,
        },
        FacetKey::Parameter(_) => ChipPalette {
            fill: style::CHIP_FILL,
            hover: style::CHIP_HOVER,
            stroke: style::CHIP_STROKE,
            key: style::CHIP_KEY,
            text: style::CHIP_TEXT,
        },
    }
}

/// Draws the chip of one filter, and returns its response.
///
/// The chip is painted rather than built from a button, because a button under the pointer takes egui's hovered
/// visuals, whose outline and rounding differ from the chip's own and make it change size by a point as the pointer
/// reaches it. A chip's size is decided by its words alone; the pointer changes its fill and nothing else.
fn chip(ui: &mut egui::Ui, key: &FacetKey, text: &str) -> egui::Response {
    let palette = chip_palette(key);
    let name = key.name();
    let label = format!("{name}: {text} {}", style::REMOVE);
    let mut job = LayoutJob::default();
    job.append(
        name,
        0.0,
        egui::TextFormat {
            font_id: monospace_count(),
            color: palette.key,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.append(
        text,
        style::ROW_GAP,
        egui::TextFormat {
            font_id: egui::FontId::proportional(style::SMALL_BUTTON_SIZE_PT),
            color: palette.text,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    // The mark is a size larger than the words and centred on their row, so that it reads as a control rather
    // than as punctuation.
    job.append(
        &format!(" {}", style::REMOVE),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(REMOVE_MARK_SIZE_PT),
            color: palette.text,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    // A chip may wrap onto a second line, but never wider than the row it is in.
    job.wrap.max_width = ui.max_rect().width() - CHIP_PADDING.sum().x;
    let galley = ui.painter().layout_job(job);
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + CHIP_PADDING.sum(), egui::Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone()));
    if ui.is_rect_visible(rect) {
        let fill = if response.hovered() {
            palette.hover
        } else {
            palette.fill
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            2.0,
            fill,
            egui::Stroke::new(1.0, palette.stroke),
            egui::StrokeKind::Inside,
        );
        painter.galley(rect.min + CHIP_PADDING.left_top(), galley, palette.text);
    }
    response.on_hover_text("Click to remove this filter.")
}

/// The word on the control that reverses the order, which says which way it runs.
fn direction_word(descending: bool) -> &'static str {
    if descending {
        "Descending"
    } else {
        "Ascending"
    }
}

/// Draws the control that reverses the order: its word, and beside it the triangle a combo box carries, pointing
/// up for ascending and down for descending.
///
/// It is drawn as egui draws the button of a combo box, with the same face, padding, icon and text size, because
/// it shares a row with one and the two must read as a pair. The triangle is painted rather than typed, so that it
/// is the very shape the combo box draws and needs no character the fonts might lack.
fn direction_button(ui: &mut egui::Ui, descending: bool) -> egui::Response {
    let padding = ui.spacing().button_padding;
    let icon = egui::Vec2::splat(ui.spacing().icon_width);
    let icon_spacing = ui.spacing().icon_spacing;
    let galley = ui.painter().layout_no_wrap(
        direction_word(descending).to_owned(),
        egui::FontId::proportional(style::COMBO_SIZE_PT),
        egui::Color32::WHITE,
    );
    let inner = egui::vec2(
        galley.size().x + icon_spacing + icon.x,
        galley.size().y.max(icon.y),
    );
    let size = (inner + 2.0 * padding).max(egui::vec2(0.0, ui.spacing().interact_size.y));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            direction_word(descending).to_owned(),
        )
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let visuals = ui.style().interact(&response);
    let painter = ui.painter();
    painter.rect(
        rect.expand(visuals.expansion),
        visuals.corner_radius,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let inner_rect = rect.shrink2(padding);
    let text_rect = egui::Align2::LEFT_CENTER.align_size_within_rect(galley.size(), inner_rect);
    painter.galley(text_rect.min, galley, visuals.text_color());
    // The triangle is the one egui paints on a combo box: the icon's rectangle narrowed to seven tenths and
    // shortened to forty-five hundredths, filled between its two upper corners and the middle of its base, or
    // turned over for ascending.
    let icon_rect = egui::Align2::RIGHT_CENTER.align_size_within_rect(icon, inner_rect);
    let triangle = egui::Rect::from_center_size(
        icon_rect.center(),
        egui::vec2(icon_rect.width() * 0.7, icon_rect.height() * 0.45),
    );
    let points = if descending {
        vec![
            triangle.left_top(),
            triangle.right_top(),
            triangle.center_bottom(),
        ]
    } else {
        vec![
            triangle.left_bottom(),
            triangle.right_bottom(),
            triangle.center_top(),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        points,
        visuals.fg_stroke.color,
        egui::Stroke::NONE,
    ));
    response
}

/// The width of a combo box of the order or the grouping: [`COMBO_WIDTH`] when the panel has room for the widest
/// row, which is the order's caption, its direction control and its box, and what that row leaves otherwise.
///
/// The two rows share one width, whatever each holds, so that their boxes end at one edge.
fn combo_width(ui: &egui::Ui) -> f32 {
    let measure = |text: &str, font_id: egui::FontId| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), font_id, egui::Color32::WHITE)
            .size()
            .x
    };
    let font = egui::FontId::proportional(style::COMBO_SIZE_PT);
    let direction = measure(direction_word(true), font.clone())
        .max(measure(direction_word(false), font))
        + ui.spacing().icon_spacing
        + ui.spacing().icon_width
        + 2.0 * ui.spacing().button_padding.x;
    let caption = measure("GROUP", caption_font()).max(measure("SORT", caption_font()));
    let room = ui.available_width() - caption - direction - 2.0 * ui.spacing().item_spacing.x;
    room.clamp(COMBO_MIN_WIDTH, COMBO_WIDTH)
}

/// Draws the row of the order: its caption at the left, and at the right the box that says what the order is taken
/// from and, beside it, the control that reverses it.
fn sort_row(ui: &mut egui::Ui, browser: &mut FigureBrowser, facets: &[Facet]) {
    let width = combo_width(ui);
    ui.horizontal(|ui| {
        caption(ui, "SORT");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let selected = match &browser.browse.sort.key {
                SortKey::Title => "Title".to_owned(),
                SortKey::Parameter(name) => name.clone(),
            };
            egui::ComboBox::from_id_salt("ironlab_browser_sort")
                .selected_text(egui::RichText::new(selected).size(style::COMBO_SIZE_PT))
                .width(width)
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
            let descending = browser.browse.sort.descending;
            if direction_button(ui, descending)
                .on_hover_text("Reverse the order.")
                .clicked()
            {
                browser.browse.sort.descending = !descending;
            }
        });
    });
}

/// Draws the row of the grouping: its caption at the left, and at the right the box that says what the list is
/// grouped by.
fn group_row(ui: &mut egui::Ui, browser: &mut FigureBrowser, facets: &[Facet]) {
    let width = combo_width(ui);
    ui.horizontal(|ui| {
        caption(ui, "GROUP");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let selected = match &browser.browse.group {
                None => "None".to_owned(),
                Some(key) => key.name().to_owned(),
            };
            egui::ComboBox::from_id_salt("ironlab_browser_group")
                .selected_text(egui::RichText::new(selected).size(style::COMBO_SIZE_PT))
                .width(width)
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
    });
}

/// Draws the caption of a control: a short word in small capitals, spread a little, quieter than the control it
/// names.
fn caption(ui: &mut egui::Ui, text: &str) {
    let mut job = LayoutJob::default();
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: caption_font(),
            color: ui.visuals().weak_text_color(),
            extra_letter_spacing: CAPTION_SPACING * style::SMALL_SIZE_PT,
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    ui.label(job);
}

/// The font of the caption of a control: small proportional text.
fn caption_font() -> egui::FontId {
    egui::FontId::proportional(style::SMALL_SIZE_PT)
}

/// Draws a quiet button: a caption with no face until the pointer is over it, for a control that must not compete
/// with the ones beside it.
fn quiet_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(ui.visuals().weak_text_color()))
            .frame_when_inactive(false),
    )
}

/// The small monospaced font the second line of a row is set in: the details of a figure read as data beside its
/// title.
fn monospace_small() -> egui::FontId {
    egui::FontId::monospace(style::SMALL_SIZE_PT)
}

/// The monospaced font a count, or the name of a parameter on a chip, is set in beside ordinary text.
fn monospace_count() -> egui::FontId {
    egui::FontId::monospace(style::MONOSPACE_SMALL_SIZE_PT)
}

/// The wrapping that keeps text to one line no wider than `width`, cut short with an ellipsis.
fn single_line(width: f32) -> egui::text::TextWrapping {
    egui::text::TextWrapping {
        max_width: width,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('\u{2026}'),
    }
}

/// Lays out one line of text, cut short with an ellipsis where it would run past `width`.
fn truncated(
    ui: &egui::Ui,
    text: &str,
    font_id: egui::FontId,
    color: egui::Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    truncated_job(
        ui,
        text,
        egui::TextFormat {
            font_id,
            color,
            ..Default::default()
        },
        width,
    )
}

/// Lays out one line of text in `format`, cut short with an ellipsis where it would run past `width`.
fn truncated_job(
    ui: &egui::Ui,
    text: &str,
    format: egui::TextFormat,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::default();
    job.append(text, 0.0, format);
    job.wrap = single_line(width);
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
        egui::Frame::new()
            .inner_margin(EMPTY_PADDING)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.label("No figure matches");
                    ui.add_space(EMPTY_GAP);
                    ui.label(
                        egui::RichText::new("Loosen a filter, or clear them all.")
                            .size(style::COMBO_SIZE_PT)
                            .weak(),
                    );
                });
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
            ui.spacing_mut().item_spacing.y = 0.0;
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

/// The height of one row of the list: a line of ordinary text, a line of small text, the room between them, and
/// the padding above and below.
///
/// Every row is the same height, headings included, because [`egui::ScrollArea::show_rows`] finds a row by
/// multiplying rather than by laying out the rows above it.
fn row_height(ui: &egui::Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body)
        + LINE_GAP
        + ui.fonts_mut(|fonts| fonts.row_height(&monospace_small()))
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
/// the name of the group in spaced capitals, and how many figures it holds. Returns whether it was clicked.
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
    let text_color = if response.hovered() {
        visuals.text_color()
    } else {
        visuals.weak_text_color()
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, style::GROUP_FILL);
    let hairline = egui::Stroke::new(1.0, style::STROKE);
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
        monospace_count(),
        visuals.weak_text_color(),
    );
    let count_x = rect.max.x - ROW_PADDING.x - count.size().x;
    painter.galley(
        egui::pos2(count_x, rect.center().y - count.size().y / 2.0),
        count,
        visuals.weak_text_color(),
    );

    let text_x = rect.min.x + ROW_PADDING.x + 16.0;
    let title = truncated_job(
        ui,
        &name.to_uppercase(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(GROUP_HEADING_SIZE_PT),
            color: text_color,
            extra_letter_spacing: GROUP_HEADING_SPACING * GROUP_HEADING_SIZE_PT,
            ..Default::default()
        },
        count_x - text_x - ROW_PADDING.x,
    );
    painter.galley(
        egui::pos2(text_x, rect.center().y - title.size().y / 2.0),
        title,
        text_color,
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
            style::SELECTED_DETAIL,
        )
    } else {
        let fill = if response.hovered() {
            style::WIDGET
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
        let detail = truncated(ui, &detail, monospace_small(), detail_color, text_width);
        painter.galley(
            origin + egui::vec2(0.0, title_height + LINE_GAP),
            detail,
            detail_color,
        );
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
        ui.label(
            egui::RichText::new(text)
                .monospace()
                .size(style::MONOSPACE_SMALL_SIZE_PT)
                .weak(),
        );
        if !browser.browse.is_unfiltered() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if quiet_button(ui, "Show all")
                    .on_hover_text("Remove every filter and clear the search.")
                    .clicked()
                {
                    browser.browse.clear();
                }
            });
        }
    });
}
