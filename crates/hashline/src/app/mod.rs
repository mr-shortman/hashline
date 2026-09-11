//! The application: window, header bar, actions, document loading
//! (SPEC.md, sections 3, 5 and 7).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

mod menu;
mod outline_view;
mod tab;

use crate::document::{self, Anchor};
use crate::preferences::Preferences;
use crate::theme::{document as tokens, DARK, LIGHT};
use crate::view::DocumentView;
use tab::Tab;

pub const APP_ID: &str = "de.kalendium.Hashline";

/// A development ceiling on how much Markdown is read at all, so a stray file
/// cannot pull the process over (SPEC.md, section 10).
const SOURCE_LIMIT: u64 = 20 * 1024 * 1024;

/// How long typing settles before a search runs (SPEC.md, section 8).
///
/// The budget is the whole distance from the last keystroke to the marks on
/// screen: 100 ms for a medium document, 120 ms for a large one
/// (docs/decisions/014-competitive-targets.md, section 3.3). Scanning the
/// 10 MiB fixture takes 5 ms, so nearly all of that budget is this wait, and
/// 120 ms of it left nothing. Sixty milliseconds still collects a fast typist's
/// keystrokes into one scan, and a scan that does happen per keystroke costs
/// less than a frame.
const SEARCH_DEBOUNCE_MS: u64 = 60;

/// Above this window width the outline gets its own column instead of
/// floating over the text (SPEC.md, section 3).
const OUTLINE_SIDEBAR_WIDTH: i32 = 900;

pub fn run() -> glib::ExitCode {
    tune_allocator();
    glib::set_application_name("Hashline");
    let application = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN | gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    application.set_option_context_parameter_string(Some("[FILE …]"));
    application.set_option_context_summary(Some(
        "Open Markdown in one window. Each file given opens in its own tab.",
    ));
    application.add_main_option(
        "version",
        glib::Char::from(b'v'),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        "Show the application version",
        None,
    );
    application.connect_handle_local_options(|_, options| {
        if options.contains("version") {
            println!("Hashline {}", env!("CARGO_PKG_VERSION"));
            std::ops::ControlFlow::Break(glib::ExitCode::SUCCESS)
        } else {
            std::ops::ControlFlow::Continue(())
        }
    });
    application.connect_startup(|_| gtk::Window::set_default_icon_name(APP_ID));
    install_application(&application);
    application.run()
}

fn install_application(application: &gtk::Application) {
    // Keep one controller for the application's one window. GApplication
    // forwards later invocations here, including paths resolved by the caller.
    let current: Rc<RefCell<Option<Rc<Ui>>>> = Rc::new(RefCell::new(None));
    let get_ui = move |application: &gtk::Application| {
        if let Some(ui) = current.borrow().as_ref() {
            return ui.clone();
        }
        let ui = Ui::build(application);
        let current_on_close = current.clone();
        ui.window.connect_close_request(move |_| {
            current_on_close.borrow_mut().take();
            glib::Propagation::Proceed
        });
        *current.borrow_mut() = Some(ui.clone());
        ui
    };
    let command_ui = get_ui.clone();
    application.connect_command_line(move |application, command| {
        // GOption can retain the option terminator in the remaining arguments.
        // Consume it once, then treat even a literal "--" as a filename. GIO
        // resolves each path against the *calling* process's working directory.
        let mut terminated = false;
        let files: Vec<_> = command
            .arguments()
            .into_iter()
            .skip(1)
            .filter(|argument| {
                if !terminated && argument == "--" {
                    terminated = true;
                    false
                } else {
                    true
                }
            })
            .map(|argument| command.create_file_for_arg(argument))
            .collect();
        let ui = command_ui(application);
        if !files.is_empty() {
            ui.open_files(&files);
        }
        ui.present();
        glib::ExitCode::SUCCESS
    });
    let activate_ui = get_ui.clone();
    application.connect_open(move |application, files, _| {
        let ui = get_ui(application);
        ui.open_files(files);
        ui.present();
    });
    application.connect_activate(move |application| {
        activate_ui(application).present();
    });
}

/// What a load produced, handed back to the main thread.
struct Loaded {
    path: PathBuf,
    result: Result<(hashline_markdown::OpDocument, u64), String>,
    /// Where the reader was, when this load is a reload of the same file.
    anchor: Option<Anchor>,
}

struct Ui {
    window: gtk::ApplicationWindow,
    /// One page per open document (docs/decisions/017-tabs.md).
    notebook: gtk::Notebook,
    tabs: RefCell<Vec<Rc<Tab>>>,
    /// The tab the window is currently showing, so that what belongs to it can
    /// be taken off the shared widgets before another tab takes them over.
    showing: RefCell<Option<Rc<Tab>>>,
    /// Set while a tab is being switched to, so the search field's own change
    /// notification does not run a search the reader did not ask for.
    restoring: Cell<bool>,
    title: gtk::Label,
    stack: gtk::Stack,
    search_bar: gtk::SearchBar,
    search_entry: gtk::SearchEntry,
    search_count: gtk::Label,
    menu_button: gtk::MenuButton,
    menu: menu::Menu,
    notice: gtk::Revealer,
    notice_label: gtk::Label,
    notice_generation: Cell<u64>,
    overlay_order: RefCell<Vec<&'static str>>,
    theme_mode: RefCell<String>,
    system_dark: Cell<bool>,
    theme_ready: Cell<bool>,
    present_requested: Cell<bool>,
    outline_revealer: gtk::Revealer,
    outline_list: outline_view::OutlineList,
    banner: gtk::Revealer,
    banner_label: gtk::Label,
    preferences: Preferences,
}

impl Ui {
    fn build(application: &gtk::Application) -> Rc<Self> {
        let preferences = Preferences::load();
        let (width, height, maximized) = preferences.window_size();
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .default_width(width)
            .default_height(height)
            .build();
        // Optional benchmark provenance: a requested GSK renderer can fall back.
        // Report the actual native renderer once it exists, never infer it from
        // GSK_RENDERER. Normal launches do not install this handler.
        if std::env::var_os("HASHLINE_BENCH_METADATA").is_some() {
            window.connect_map(|window| {
                if let Some(renderer) = window.renderer() {
                    eprintln!(
                        "HASHLINE_BENCH renderer={} backend={}",
                        renderer.type_().name(),
                        gtk::prelude::WidgetExt::display(window).type_().name()
                    );
                }
            });
        }
        if maximized {
            window.maximize();
        }

        let notebook = tab::notebook();

        let title = gtk::Label::new(Some("Hashline"));
        title.add_css_class("title");
        title.set_ellipsize(pango::EllipsizeMode::Middle);

        let open_button = gtk::Button::from_icon_name("document-open-symbolic");
        open_button.set_tooltip_text(Some("Markdown-Datei öffnen (Ctrl+O)"));
        let header = gtk::HeaderBar::builder().title_widget(&title).build();
        header.pack_start(&open_button);

        // The quiet empty view: what to do, and that dropping a file works
        // (SPEC.md, section 3).
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 12);
        empty.set_valign(gtk::Align::Center);
        empty.set_halign(gtk::Align::Center);
        let empty_button = gtk::Button::with_label("Markdown-Datei öffnen");
        empty_button.add_css_class("suggested-action");
        empty_button.add_css_class("pill");
        let hint = gtk::Label::new(Some("oder eine Datei hierher ziehen"));
        hint.add_css_class("dim-label");
        empty.append(&empty_button);
        empty.append(&hint);

        let stack = gtk::Stack::new();
        stack.add_named(&empty, Some("empty"));
        stack.add_named(&notebook, Some("document"));
        stack.set_visible_child_name("empty");

        // Search: a bar over the document, with the hit count beside the field
        // and the usual next/previous (SPEC.md, section 3).
        let search_entry = gtk::SearchEntry::new();
        // GtkSearchEntry delays its own change notification by 150 ms, which
        // would come on top of the wait below and put every search over budget
        // before the first byte is compared. The waiting is done in one place.
        search_entry.set_search_delay(0);
        search_entry.set_hexpand(true);
        search_entry.set_placeholder_text(Some("Im Dokument suchen"));
        let search_count = gtk::Label::new(None);
        search_count.add_css_class("dim-label");
        let previous_hit = gtk::Button::from_icon_name("go-up-symbolic");
        previous_hit.set_tooltip_text(Some("Vorheriger Treffer (Shift+Enter)"));
        let next_hit = gtk::Button::from_icon_name("go-down-symbolic");
        next_hit.set_tooltip_text(Some("Nächster Treffer (Enter)"));
        let search_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        search_box.append(&search_entry);
        search_box.append(&search_count);
        search_box.append(&previous_hit);
        search_box.append(&next_hit);
        let search_bar = gtk::SearchBar::builder()
            .child(&search_box)
            .key_capture_widget(&window)
            .build();
        search_bar.connect_entry(&search_entry);
        // GtkSearchBar wraps its child in a revealer of its own and gives it
        // GTK's default 250 ms slide, twice this app's own motion constant and
        // a number nothing here chose. Measured over 90 runs, the placeholder
        // is not legible for the first 62-68 ms after Ctrl+F and the proof
        // cannot read it before 89-92 ms, against a 25 ms budget for opening
        // the search (docs/decisions/014-competitive-targets.md, section 1).
        // The bar is the answer to a keystroke, so it arrives with the frame
        // that answers it.
        if let Some(revealer) = search_bar.first_child().and_downcast::<gtk::Revealer>() {
            revealer.set_transition_type(gtk::RevealerTransitionType::None);
        }

        // The outline: an overlay over the document, given room as a sidebar
        // once the window is wide enough (SPEC.md, section 3).
        let outline_list = outline_view::OutlineList::new();
        let outline_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .width_request(260)
            .child(&outline_list.view)
            .build();
        outline_scroller.add_css_class("sidebar");
        let outline_revealer = gtk::Revealer::builder()
            .child(&outline_scroller)
            .transition_type(gtk::RevealerTransitionType::SlideRight)
            .reveal_child(false)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Fill)
            .build();

        // A read failure leaves the document that is showing in place and says
        // so above it (SPEC.md, section 7).
        let banner_label = gtk::Label::new(None);
        banner_label.set_xalign(0.0);
        banner_label.set_wrap(true);
        let banner_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        banner_box.set_margin_top(8);
        banner_box.set_margin_bottom(8);
        banner_box.set_margin_start(12);
        banner_box.set_margin_end(12);
        banner_box.append(&banner_label);
        let retry = gtk::Button::with_label("Erneut versuchen");
        retry.set_halign(gtk::Align::End);
        retry.set_hexpand(true);
        banner_box.append(&retry);
        let banner = gtk::Revealer::builder()
            .child(&banner_box)
            .reveal_child(false)
            .build();

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&search_bar);
        content.append(&banner);
        let notice_label = gtk::Label::builder()
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        notice_label.set_wrap(true);
        notice_label.set_margin_top(8);
        notice_label.set_margin_bottom(8);
        let notice = gtk::Revealer::builder().child(&notice_label).build();
        content.append(&notice);
        // The outline covers the document and nothing above it. It used to
        // overlay the whole content column, so an open outline sat over the
        // search bar as well.
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&stack));
        overlay.add_overlay(&outline_revealer);
        content.append(&overlay);

        let outline_button = gtk::ToggleButton::new();
        outline_button.set_icon_name("view-list-symbolic");
        outline_button.set_tooltip_text(Some("Inhaltsverzeichnis (Ctrl+Shift+O)"));
        let search_button = gtk::ToggleButton::new();
        search_button.set_icon_name("system-search-symbolic");
        search_button.set_tooltip_text(Some("Suchen (Ctrl+F)"));
        // Neither the outline nor the search has a row: both already have a
        // button beside this one, and both say their key in its tooltip.
        let menu = menu::build();
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Menü")
            .popover(&menu.popover)
            // The window's primary menu, so F10 opens it as every GNOME
            // application's does.
            .primary(true)
            .build();
        header.pack_end(&menu_button);
        header.pack_end(&search_button);
        header.pack_end(&outline_button);

        window.set_titlebar(Some(&header));
        window.set_child(Some(&content));

        let ui = Rc::new(Ui {
            window,
            notebook,
            tabs: RefCell::new(Vec::new()),
            showing: RefCell::new(None),
            restoring: Cell::new(false),
            title,
            stack,
            banner,
            banner_label,
            search_bar: search_bar.clone(),
            search_entry: search_entry.clone(),
            search_count,
            menu_button,
            menu,
            notice,
            notice_label,
            notice_generation: Cell::new(0),
            overlay_order: RefCell::new(Vec::new()),
            theme_ready: Cell::new(preferences.theme() != "system"),
            present_requested: Cell::new(false),
            theme_mode: RefCell::new(preferences.theme()),
            system_dark: Cell::new(gtk::Settings::default().is_some_and(|s| {
                s.is_gtk_application_prefer_dark_theme()
                    || s.gtk_theme_name()
                        .is_some_and(|n| n.to_lowercase().contains("dark"))
            })),
            outline_revealer: outline_revealer.clone(),
            outline_list,
            preferences,
        });

        ui.menu.set_theme(&ui.theme_mode.borrow());
        ui.menu.set_zoom(ui.zoom());
        ui.install_theme();
        ui.install_escape();
        ui.install_search(&previous_hit, &next_hit);
        ui.install_outline();
        ui.install_actions(application);
        ui.install_drop_target();
        ui.install_tabs();

        for button in [&open_button, &empty_button] {
            button.connect_clicked(glib::clone!(
                #[strong]
                ui,
                move |_| ui.choose_file()
            ));
        }
        retry.connect_clicked(glib::clone!(
            #[strong]
            ui,
            move |_| ui.reload()
        ));
        search_button
            .bind_property("active", &search_bar, "search-mode-enabled")
            .bidirectional()
            .build();
        outline_button
            .bind_property("active", &outline_revealer, "reveal-child")
            .bidirectional()
            .build();

        // The stored view preferences take effect before anything is shown,
        // so no wrong zoom or layout flashes (SPEC.md, section 3).
        let showing = ui.preferences.outline_visible();
        outline_button.set_active(showing);
        ui.outline_revealer.set_reveal_child(showing);
        ui.update_outline_mode();

        // Everything worth keeping is written when the window closes, once,
        // rather than on every change.
        ui.window.connect_close_request(glib::clone!(
            #[strong]
            ui,
            move |window| {
                for tab in ui.tabs.borrow().iter() {
                    ui.remember_position_of(tab);
                    tab.close();
                }
                ui.preferences.set_window_size(
                    window.width(),
                    window.height(),
                    window.is_maximized(),
                );
                ui.preferences.set_zoom(ui.zoom());
                ui.preferences
                    .set_outline_visible(ui.outline_revealer.reveals_child());
                glib::Propagation::Proceed
            }
        ));

        place_on_monitor(&ui.window);
        ui
    }

    /// The tab showing, if a document is open at all.
    fn tab(&self) -> Option<Rc<Tab>> {
        let index = self.notebook.current_page()?;
        self.tabs.borrow().get(index as usize).cloned()
    }

    fn view(&self) -> Option<DocumentView> {
        self.tab().map(|tab| tab.view.clone())
    }

    /// Sets the type size of every tab, so switching never resizes the text.
    fn set_zoom(&self, percent: i32) {
        for tab in self.tabs.borrow().iter() {
            tab.view.set_zoom(percent);
        }
        self.menu.set_zoom(self.zoom());
        self.preferences.set_zoom(self.zoom());
    }

    /// Moves to the next or previous tab, wrapping round.
    fn step_tab(&self, step: i32) {
        let count = self.notebook.n_pages() as i32;
        if count < 2 {
            return;
        }
        let current = self.notebook.current_page().unwrap_or(0) as i32;
        let next = (current + step).rem_euclid(count);
        self.notebook.set_current_page(Some(next as u32));
    }

    /// The zoom every tab is set at. It is a window-wide setting, so the tabs
    /// never disagree and any one of them can be asked.
    fn zoom(&self) -> i32 {
        self.tabs
            .borrow()
            .first()
            .map(|tab| tab.view.zoom())
            .unwrap_or_else(|| self.preferences.zoom())
    }

    /// Records where reading stopped in one tab's file.
    fn remember_position_of(&self, tab: &Tab) {
        let path = tab.path.borrow().clone();
        if let Some(path) = path {
            self.preferences
                .remember_position(&path, &tab.view.reading_anchor());
        }
    }

    /// A new page of the notebook, with everything a document needs wired to
    /// it: links, the outline's notifications, and its own close button.
    fn new_tab(self: &Rc<Self>) -> Rc<Tab> {
        let tab = Tab::new();
        tab.view.set_zoom(self.zoom());
        self.install_links(&tab);
        self.install_section_notice(&tab);
        let index = self.notebook.append_page(&tab.page, Some(&tab.handle));
        self.tabs.borrow_mut().insert(index as usize, tab.clone());
        tab::update_strip(&self.notebook);
        let ui = self.clone();
        let weak = Rc::downgrade(&tab);
        tab.close.connect_clicked(move |_| {
            if let Some(tab) = weak.upgrade() {
                ui.close_tab(&tab);
            }
        });
        self.stack.set_visible_child_name("document");
        tab
    }

    /// Closes one tab, keeping the reading position of the file it held.
    fn close_tab(self: &Rc<Self>, tab: &Rc<Tab>) {
        let Some(index) = self.notebook.page_num(&tab.page) else {
            return;
        };
        self.remember_position_of(tab);
        tab.close();
        self.tabs.borrow_mut().retain(|open| !Rc::ptr_eq(open, tab));
        self.notebook.remove_page(Some(index));
        tab::update_strip(&self.notebook);
        if self.tabs.borrow().is_empty() {
            self.stack.set_visible_child_name("empty");
            self.title.set_text("Hashline");
            self.window.set_tooltip_text(None);
            self.banner.set_reveal_child(false);
            self.outline_list.model.set_outline(Rc::default());
            self.showing.replace(None);
        } else {
            self.enter_tab();
        }
    }

    /// Everything the window shows for the tab that is now in front.
    ///
    /// The search field is one widget shared by every tab, so what the tab
    /// being left had in it is taken off it here rather than when it was
    /// typed: `GtkSearchEntry` reports a change after a delay of its own, and
    /// that report can arrive after the switch.
    fn enter_tab(self: &Rc<Self>) {
        let Some(tab) = self.tab() else {
            return;
        };
        let left = self.showing.replace(Some(tab.clone()));
        if let Some(left) = left.filter(|left| !Rc::ptr_eq(left, &tab)) {
            *left.query.borrow_mut() = self.search_entry.text().to_string();
            left.searching.set(self.search_bar.is_search_mode());
        }
        for other in self.tabs.borrow().iter() {
            if !Rc::ptr_eq(other, &tab) {
                other.deactivate();
            }
        }
        self.title.set_text(&tab.title.text());
        self.window
            .set_tooltip_text(tab.handle.tooltip_text().as_deref());
        // The search field belongs to the window, its contents to the tab.
        self.restoring.set(true);
        self.search_entry.set_text(&tab.query.borrow());
        self.search_bar.set_search_mode(tab.searching.get());
        self.restoring.set(false);
        self.update_search_count();
        self.fill_outline();
        self.banner.set_reveal_child(false);
    }

    /// Switching pages, closing with Ctrl+W and cycling with Ctrl+Tab.
    fn install_tabs(self: &Rc<Self>) {
        let ui = self.clone();
        self.notebook.connect_switch_page(move |_, _, _| {
            // The notebook reports the page it is switching *to* before it is
            // current, so the rest of the window is updated once it is.
            let ui = ui.clone();
            glib::idle_add_local_once(move || ui.enter_tab());
        });
    }

    /// Debounced so that typing does not run a scan per keystroke, and the
    /// answer to an older query can never overwrite a newer one
    /// (SPEC.md, section 8).
    fn install_search(self: &Rc<Self>, previous: &gtk::Button, next: &gtk::Button) {
        let pending: Rc<Cell<u64>> = Rc::new(Cell::new(0));
        let ui = self.clone();
        let token = pending.clone();
        self.search_entry.connect_search_changed(move |entry| {
            if ui.restoring.get() {
                return;
            }
            token.set(token.get() + 1);
            let generation = token.get();
            let needle = entry.text().to_string();
            let ui = ui.clone();
            let token = token.clone();
            glib::timeout_add_local_once(
                std::time::Duration::from_millis(SEARCH_DEBOUNCE_MS),
                move || {
                    if token.get() != generation {
                        return;
                    }
                    ui.run_search(&needle);
                },
            );
        });

        let ui = self.clone();
        self.search_entry.connect_activate(move |_| {
            if let Some(view) = ui.view() {
                view.search_next();
            }
            ui.update_search_count();
        });
        let ui = self.clone();
        self.search_entry.connect_previous_match(move |_| {
            if let Some(view) = ui.view() {
                view.search_previous();
            }
            ui.update_search_count();
        });
        let ui = self.clone();
        next.connect_clicked(move |_| {
            if let Some(view) = ui.view() {
                view.search_next();
            }
            ui.update_search_count();
        });
        let ui = self.clone();
        previous.connect_clicked(move |_| {
            if let Some(view) = ui.view() {
                view.search_previous();
            }
            ui.update_search_count();
        });
        // Closing the bar clears the marks, and the document keeps the focus
        // it had (SPEC.md, section 3).
        let ui = self.clone();
        self.search_bar
            .connect_search_mode_enabled_notify(move |bar| {
                if !bar.is_search_mode() && !ui.restoring.get() {
                    if let Some(view) = ui.view() {
                        view.clear_search();
                        view.grab_focus();
                    }
                    ui.search_count.set_text("");
                }
            });
    }

    fn run_search(&self, needle: &str) {
        let Some(view) = self.view() else {
            return;
        };
        if needle.is_empty() {
            view.clear_search();
            self.search_count.set_text("");
            return;
        }
        view.search(needle);
        self.update_search_count();
    }

    fn update_search_count(&self) {
        let Some(view) = self.view() else {
            self.search_count.set_text("");
            return;
        };
        let (current, total) = view.search_position();
        self.search_count.set_text(&match (current, total) {
            (_, 0) => "Kein Treffer".to_string(),
            (0, total) => format!("{total} Treffer"),
            (current, total) => format!("{current} von {total}"),
        });
    }

    /// One tab's report that the reader has moved into another section.
    fn install_section_notice(self: &Rc<Self>, tab: &Rc<Tab>) {
        let ui = Rc::downgrade(self);
        let page = tab.page.clone();
        tab.view
            .connect_local("active-section-changed", false, move |_| {
                if let Some(ui) = ui.upgrade() {
                    // Only the tab in front may move the outline's selection.
                    if ui.tab().is_some_and(|tab| tab.page == page) {
                        ui.update_active_section();
                        ui.update_outline_mode();
                    }
                }
                None
            });
    }

    fn install_outline(self: &Rc<Self>) {
        let ui = self.clone();
        self.outline_list.view.connect_activate(move |_, position| {
            if let (Some(block), Some(view)) = (ui.outline_list.block_at(position), ui.view()) {
                view.scroll_to_block(block);
            }
        });
        // Wide enough for a sidebar: the document moves over rather than
        // being covered. Narrower, the outline floats above it.
        let ui = self.clone();
        self.outline_revealer
            .connect_child_revealed_notify(move |_| ui.update_outline_mode());
    }

    fn update_outline_mode(&self) {
        let wide = self.window.width() >= OUTLINE_SIDEBAR_WIDTH;
        let showing = self.outline_revealer.reveals_child();
        self.stack
            .set_margin_start(if wide && showing { 260 } else { 0 });
    }

    fn fill_outline(&self) {
        // No widgets are built here: the model hands the list view a row only
        // when that row is on screen.
        let outline = self.view().map(|view| view.outline()).unwrap_or_default();
        self.outline_list.model.set_outline(outline);
        self.update_active_section();
    }

    fn update_active_section(&self) {
        let active = self.view().and_then(|view| view.active_section());
        self.outline_list.select(active.map(|index| index as u32));
    }

    /// Every file opens in its own tab, and a file that is already open is
    /// brought to the front instead of read again
    /// (docs/decisions/014-competitive-targets.md, section 2.2).
    fn open_files(self: &Rc<Self>, files: &[gio::File]) {
        let mut refused = false;
        for file in files {
            match file.path() {
                Some(path) => self.open(&path),
                None => refused = true,
            }
        }
        if refused {
            self.note("Nur lokale Markdown-Dateien können geöffnet werden.");
        }
    }

    /// Reads and parses off the main thread, then applies the result if it is
    /// still the newest request (SPEC.md, sections 5 and 10).
    fn open(self: &Rc<Self>, path: &Path) {
        self.open_at(path, None);
    }

    /// Opens a file and, once it is in place, jumps to a `#fragment` in it.
    ///
    /// The jump has to wait for the load, which runs on a thread of its own:
    /// scheduled as an idle callback instead, it ran against the tab that was
    /// in front before the document had even been read.
    fn open_at(self: &Rc<Self>, path: &Path, fragment: Option<String>) {
        let path = document::normalize(path);
        if let Some(open) = self.tab_for(&path) {
            if let Some(index) = self.notebook.page_num(&open.page) {
                self.notebook.set_current_page(Some(index));
            }
            if let Some(fragment) = fragment {
                self.follow_fragment(&open, &fragment);
            }
            return;
        }
        let tab = self.new_tab();
        if let Some(index) = self.notebook.page_num(&tab.page) {
            self.notebook.set_current_page(Some(index));
        }
        self.load(&tab, &path, None, fragment);
    }

    /// Jumps to a link's `#fragment` in one tab, or says that it is not there.
    fn follow_fragment(self: &Rc<Self>, tab: &Tab, fragment: &str) -> bool {
        let found = tab.view.scroll_to_anchor(fragment);
        if !found {
            self.note(&format!("Kein Abschnitt „{fragment}“ in diesem Dokument"));
        }
        found
    }

    /// The tab already showing `path`, if there is one.
    fn tab_for(&self, path: &Path) -> Option<Rc<Tab>> {
        self.tabs
            .borrow()
            .iter()
            .find(|tab| tab.path.borrow().as_deref() == Some(path))
            .cloned()
    }

    /// A reload of the file showing in one tab, keeping the reading position.
    fn reload_tab(self: &Rc<Self>, tab: &Rc<Tab>) {
        let path = tab.path.borrow().clone();
        if let Some(path) = path {
            let anchor = tab.view.reading_anchor();
            self.load(tab, &path, Some(anchor), None);
        }
    }

    /// A reload of the file that is showing.
    fn reload(self: &Rc<Self>) {
        if let Some(tab) = self.tab() {
            self.reload_tab(&tab);
        }
    }

    /// `anchor` is where the reader was, for a reload; `fragment` is where a
    /// link into this file points, for a first load.
    fn load(
        self: &Rc<Self>,
        tab: &Rc<Tab>,
        path: &Path,
        anchor: Option<Anchor>,
        fragment: Option<String>,
    ) {
        let path = path.to_path_buf();
        tab.request.set(tab.request.get() + 1);
        let request = tab.request.get();
        let tab = tab.clone();
        let (sender, receiver) = async_channel::bounded(1);
        let for_thread = path.clone();
        std::thread::spawn(move || {
            let result = read_and_parse(&for_thread);
            // The parse ran on this thread's own allocation arena, and what it
            // allocated and freed on the way — the source text, the parser's
            // events, the interning table — is returned here rather than left
            // for the arena to keep.
            release_free_memory();
            let _ = sender.send_blocking(Loaded {
                path: for_thread,
                result,
                anchor,
            });
        });
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let Ok(loaded) = receiver.recv().await else {
                return;
            };
            // A later request has already been made: this answer is stale.
            if tab.request.get() != request {
                return;
            }
            match loaded.result {
                Ok((parsed, digest)) => {
                    let reloading = loaded.anchor.is_some();
                    // Nothing changed: leave the view, and the reader, alone.
                    if reloading && digest == tab.digest.get() {
                        return;
                    }
                    let stored = (!reloading)
                        .then(|| ui.preferences.reading_position(&loaded.path))
                        .flatten();
                    tab.digest.set(digest);
                    ui.show(&tab, loaded.path, parsed);
                    // A link's target outranks where reading last stopped;
                    // a target that is not there falls back to it.
                    let jumped =
                        fragment.is_some_and(|fragment| ui.follow_fragment(&tab, &fragment));
                    if let Some(anchor) = loaded.anchor.or(stored).filter(|_| !jumped) {
                        tab.view.restore_anchor(&anchor);
                    }
                    release_free_memory();
                }
                Err(error) => ui.show_error(&tab, &loaded.path, &error),
            }
        });
    }

    fn show(
        self: &Rc<Self>,
        tab: &Rc<Tab>,
        path: PathBuf,
        document: hashline_markdown::OpDocument,
    ) {
        // Relative picture paths resolve against the document's directory,
        // never the process working directory (SPEC.md, section 7).
        tab.view
            .set_base_directory(path.parent().map(Path::to_path_buf));
        tab.view.set_document(document);
        // The shown name changes only once the new document is actually in
        // place (SPEC.md, section 7).
        tab.set_path(&path);
        self.start_watch(tab, &path);
        self.stack.set_visible_child_name("document");
        if self.tab().is_some_and(|current| Rc::ptr_eq(&current, tab)) {
            self.title.set_text(&tab.title.text());
            self.window.set_tooltip_text(Some(&path.to_string_lossy()));
            self.banner.set_reveal_child(false);
            self.fill_outline();
        }
    }

    /// Replaces the watch, which stops the previous one, and reloads on
    /// change. A failure to watch is not fatal: manual reload stays available
    /// (SPEC.md, section 7).
    fn start_watch(self: &Rc<Self>, tab: &Rc<Tab>, path: &Path) {
        let ui = Rc::downgrade(self);
        let weak = Rc::downgrade(tab);
        // Every tab watches its own file, whether or not it is the one in
        // front (docs/decisions/014-competitive-targets.md, section 2.2).
        let watch = document::watch(path, move || {
            if let (Some(ui), Some(tab)) = (ui.upgrade(), weak.upgrade()) {
                ui.reload_tab(&tab);
            }
        });
        *tab.watch.borrow_mut() = watch;
    }

    fn show_error(self: &Rc<Self>, tab: &Rc<Tab>, path: &Path, error: &str) {
        // A tab opened for a file that cannot be read has nothing to show, so
        // it goes again rather than standing there empty.
        if tab.path.borrow().is_none() {
            self.close_tab(tab);
        }
        // The file that failed is named, because the one still on screen is a
        // different one.
        self.banner_label
            .set_text(&format!("{}: {error}", path.display()));
        self.banner.set_reveal_child(true);
    }

    fn choose_file(self: &Rc<Self>) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Markdown"));
        for pattern in ["*.md", "*.markdown", "*.mdown", "*.mkd", "*.mkdn", "*.mdwn"] {
            filter.add_pattern(pattern);
        }
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title("Markdown-Datei öffnen")
            .filters(&filters)
            .modal(true)
            .build();
        let ui = self.clone();
        dialog.open(Some(&self.window), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    ui.open(&path);
                }
            }
        });
    }

    fn install_drop_target(self: &Rc<Self>) {
        let target = gtk::DropTarget::new(
            gtk::gdk::FileList::static_type(),
            gtk::gdk::DragAction::COPY,
        );
        let ui = self.clone();
        target.connect_drop(move |_, value, _, _| {
            if let Ok(files) = value.get::<gtk::gdk::FileList>() {
                ui.open_files(&files.files());
                return true;
            }
            false
        });
        self.window.add_controller(target);
    }

    /// What a clicked link means. The view resolves nothing itself; this is the
    /// single place a document's content can ask for anything
    /// (SPEC.md, sections 7 and 11).
    fn install_links(self: &Rc<Self>, tab: &Rc<Tab>) {
        let ui = self.clone();
        let weak = Rc::downgrade(tab);
        tab.view.connect_link_activated(move |href| {
            let Some(tab) = weak.upgrade() else {
                return;
            };
            let decoded = glib::Uri::unescape_string(href, None)
                .map(|value| value.to_string())
                .unwrap_or_else(|| href.to_string());

            if let Some(fragment) = decoded.strip_prefix('#') {
                ui.follow_fragment(&tab, fragment);
                return;
            }

            let scheme = decoded.split_once(':').map(|(scheme, _)| scheme);
            match scheme {
                // Only after a click, and only with an allowed scheme.
                Some("https") | Some("http") | Some("mailto") => {
                    gtk::UriLauncher::new(&decoded).launch(
                        Some(&ui.window),
                        gio::Cancellable::NONE,
                        |_| {},
                    );
                }
                Some(other) => ui.note(&format!("Nicht unterstütztes Schema „{other}:“")),
                None => {
                    let base = tab
                        .path
                        .borrow()
                        .as_ref()
                        .and_then(|path| path.parent().map(Path::to_path_buf))
                        .unwrap_or_else(|| PathBuf::from("."));
                    // Relative paths resolve against the document, never
                    // against the process working directory.
                    // `andere.md#` has nothing to jump to.
                    let (target, fragment) = match decoded.split_once('#') {
                        Some((path, fragment)) => (
                            path,
                            Some(fragment.to_string()).filter(|fragment| !fragment.is_empty()),
                        ),
                        None => (decoded.as_str(), None),
                    };
                    let resolved = base.join(target);
                    if is_markdown(&resolved) {
                        ui.open_at(&resolved, fragment);
                    } else {
                        ui.note(&format!(
                            "Nur Markdown-Dateien werden geöffnet: {}",
                            resolved.display()
                        ));
                    }
                }
            }
        });
    }

    fn note(self: &Rc<Self>, message: &str) {
        self.notice_label.set_text(message);
        self.notice_label
            .update_property(&[gtk::accessible::Property::Label(message)]);
        self.notice.set_reveal_child(true);
        let generation = self.notice_generation.get() + 1;
        self.notice_generation.set(generation);
        let ui = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_secs(6), move || {
            if let Some(ui) = ui.upgrade() {
                if ui.notice_generation.get() == generation {
                    ui.notice.set_reveal_child(false);
                }
            }
        });
    }

    fn track_overlay(&self, name: &'static str, open: bool) {
        let mut order = self.overlay_order.borrow_mut();
        order.retain(|item| *item != name);
        if open {
            order.push(name);
        }
    }

    fn install_escape(self: &Rc<Self>) {
        let ui = Rc::downgrade(self);
        self.search_bar
            .connect_search_mode_enabled_notify(move |bar| {
                if let Some(ui) = ui.upgrade() {
                    ui.track_overlay("search", bar.is_search_mode());
                }
            });
        let ui = Rc::downgrade(self);
        self.outline_revealer
            .connect_reveal_child_notify(move |revealer| {
                if let Some(ui) = ui.upgrade() {
                    ui.track_overlay("outline", revealer.reveals_child());
                    ui.update_outline_mode();
                }
            });
        let keys = gtk::EventControllerKey::new();
        keys.set_name(Some("close-overlay"));
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let ui = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(ui) = ui.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if key != gtk::gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            if ui.menu_button.popover().is_some_and(|p| p.is_visible()) {
                ui.menu_button.popdown();
                ui.menu_button.grab_focus();
                return glib::Propagation::Stop;
            }
            let top = ui.overlay_order.borrow().last().copied();
            match top {
                Some("search") => ui.search_bar.set_search_mode(false),
                Some("outline") => ui.outline_revealer.set_reveal_child(false),
                _ => return glib::Propagation::Proceed,
            }
            if ui.overlay_order.borrow().last() == Some(&"search") {
                ui.search_entry.grab_focus();
            } else if ui.outline_revealer.reveals_child() {
                ui.outline_list.view.grab_focus();
            } else {
                if let Some(view) = ui.view() {
                    view.grab_focus();
                }
            }
            glib::Propagation::Stop
        });
        self.window.add_controller(keys);
    }

    fn present(&self) {
        self.present_requested.set(true);
        if self.theme_ready.get() {
            self.window.present();
        }
    }

    fn theme_ready(&self) {
        self.theme_ready.set(true);
        if self.present_requested.get() {
            self.window.present();
        }
    }

    fn apply_theme(&self) {
        let dark = match self.theme_mode.borrow().as_str() {
            "dark" => true,
            "light" => false,
            _ => self.system_dark.get(),
        };
        // Writing this reloads the whole GTK stylesheet, so it is written only
        // on a real change. It is a hint for the parts of the toolkit the
        // window does not draw itself — file dialogs above all; what the
        // window shows comes from the palette below, because a system theme is
        // free to ignore the hint and stay dark.
        if let Some(settings) = gtk::Settings::default() {
            if settings.is_gtk_application_prefer_dark_theme() != dark {
                settings.set_gtk_application_prefer_dark_theme(dark);
            }
        }
        let palette = if dark { DARK } else { LIGHT };
        crate::theme::chrome::apply(palette);
        for tab in self.tabs.borrow().iter() {
            tab.view.set_palette(palette);
        }
    }

    fn install_theme(self: &Rc<Self>) {
        self.apply_theme();
        if let Some(settings) = gtk::Settings::default() {
            let ui = Rc::downgrade(self);
            settings.connect_gtk_theme_name_notify(move |settings| {
                if let Some(ui) = ui.upgrade() {
                    ui.system_dark.set(
                        settings
                            .gtk_theme_name()
                            .is_some_and(|n| n.to_lowercase().contains("dark")),
                    );
                    ui.apply_theme();
                }
            });
        }
        // The desktop portal reports the system preference independently of
        // the application's override of GtkSettings.
        let ui = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(proxy) = gio::DBusProxy::for_bus_future(
                gio::BusType::Session,
                gio::DBusProxyFlags::DO_NOT_AUTO_START
                    | gio::DBusProxyFlags::DO_NOT_LOAD_PROPERTIES,
                None,
                "org.freedesktop.portal.Desktop",
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.Settings",
            )
            .await
            else {
                if let Some(ui) = ui.upgrade() {
                    ui.theme_ready();
                }
                return;
            };
            let weak = ui.clone();
            proxy.connect_local("g-signal", false, move |values| {
                let signal = values[2].get::<String>().unwrap();
                let parameters = values[3].get::<glib::Variant>().unwrap();
                if signal != "SettingChanged" {
                    return None;
                }
                if let Some((namespace, key, value)) =
                    parameters.get::<(String, String, glib::Variant)>()
                {
                    if namespace == "org.freedesktop.appearance" && key == "color-scheme" {
                        if let (Some(ui), Some(value)) = (weak.upgrade(), value.get::<u32>()) {
                            ui.system_dark.set(value == 1);
                            ui.apply_theme();
                        }
                    }
                }
                None
            });
            if let Ok(reply) = proxy
                .call_future(
                    "Read",
                    Some(&("org.freedesktop.appearance", "color-scheme").to_variant()),
                    gio::DBusCallFlags::NO_AUTO_START,
                    250,
                )
                .await
            {
                let value = reply
                    .child_value(0)
                    .as_variant()
                    .and_then(|v| v.get::<u32>());
                if let (Some(ui), Some(value)) = (ui.upgrade(), value) {
                    ui.system_dark.set(value == 1);
                    ui.apply_theme();
                }
            }
            // Keep the signal subscription alive for this window's lifetime.
            if let Some(ui) = ui.upgrade() {
                ui.theme_ready();
                ui.window.connect_destroy(move |_| {
                    let _ = &proxy;
                });
            }
        });
    }

    /// Actions are registered once and bound to keys through the application,
    /// so menu, keyboard and accessibility share one source
    /// (SPEC.md, section 3).
    fn focused_editable(&self) -> Option<gtk::Editable> {
        gtk::prelude::GtkWindowExt::focus(&self.window)
            .and_then(|w| w.downcast::<gtk::Editable>().ok())
    }

    fn install_actions(self: &Rc<Self>, application: &gtk::Application) {
        let add = |name: &str, keys: &[&str], callback: Box<dyn Fn()>| {
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| callback());
            self.window.add_action(&action);
            application.set_accels_for_action(&format!("win.{name}"), keys);
        };

        let theme = gio::SimpleAction::new_stateful(
            "theme",
            Some(glib::VariantTy::STRING),
            &self.theme_mode.borrow().to_variant(),
        );
        let ui = Rc::downgrade(self);
        theme.connect_activate(move |action, value| {
            let Some(mode) = value.and_then(|v| v.str()) else {
                return;
            };
            if !matches!(mode, "system" | "light" | "dark") {
                return;
            }
            if let Some(ui) = ui.upgrade() {
                *ui.theme_mode.borrow_mut() = mode.to_string();
                ui.preferences.set_theme(mode);
                action.set_state(&mode.to_variant());
                ui.menu.set_theme(mode);
                ui.apply_theme();
            }
        });
        self.window.add_action(&theme);
        add(
            "open",
            &["<Control>o"],
            Box::new(glib::clone!(
                #[strong(rename_to = ui)]
                self,
                move || ui.choose_file()
            )),
        );
        add(
            "copy",
            &["<Control>c"],
            Box::new(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move || {
                    if let Some(editable) = ui.focused_editable() {
                        editable.emit_by_name::<()>("copy-clipboard", &[]);
                    } else {
                        if let Some(view) = ui.view() {
                            view.copy_selection();
                        }
                    }
                }
            )),
        );
        add(
            "select-all",
            &["<Control>a"],
            Box::new(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move || {
                    if let Some(editable) = ui.focused_editable() {
                        editable.select_region(0, -1);
                    } else {
                        if let Some(view) = ui.view() {
                            view.select_all();
                        }
                    }
                }
            )),
        );
        // Zoom is a window setting, so every tab is set to the same size and
        // a tab switch never changes the type.
        add(
            "zoom-in",
            &["<Control>plus", "<Control>equal"],
            Box::new(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move || ui.set_zoom(ui.zoom() + tokens::ZOOM_STEP)
            )),
        );
        add(
            "zoom-out",
            &["<Control>minus"],
            Box::new(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move || ui.set_zoom(ui.zoom() - tokens::ZOOM_STEP)
            )),
        );
        add(
            "zoom-reset",
            &["<Control>0"],
            Box::new(glib::clone!(
                #[weak(rename_to = ui)]
                self,
                move || ui.set_zoom(100)
            )),
        );
        add(
            "close-tab",
            &["<Control>w"],
            Box::new(glib::clone!(
                #[strong(rename_to = ui)]
                self,
                move || {
                    if let Some(tab) = ui.tab() {
                        ui.close_tab(&tab);
                    }
                }
            )),
        );
        add(
            "next-tab",
            &["<Control>Tab", "<Control>Page_Down"],
            Box::new(glib::clone!(
                #[strong(rename_to = ui)]
                self,
                move || ui.step_tab(1)
            )),
        );
        add(
            "previous-tab",
            &["<Control><Shift>Tab", "<Control>Page_Up"],
            Box::new(glib::clone!(
                #[strong(rename_to = ui)]
                self,
                move || ui.step_tab(-1)
            )),
        );
        add(
            "find",
            &["<Control>f"],
            Box::new(glib::clone!(
                #[strong(rename_to = ui)]
                self,
                move || {
                    ui.search_bar.set_search_mode(true);
                    ui.search_entry.grab_focus();
                }
            )),
        );
        add(
            "outline",
            &["<Control><Shift>o"],
            Box::new(glib::clone!(
                #[strong(rename_to = ui)]
                self,
                move || {
                    let showing = ui.outline_revealer.reveals_child();
                    ui.outline_revealer.set_reveal_child(!showing);
                    ui.update_outline_mode();
                }
            )),
        );
        add(
            "reload",
            &["<Control>r"],
            Box::new(glib::clone!(
                #[strong(rename_to = ui)]
                self,
                move || ui.reload()
            )),
        );
    }
}

/// Two allocator settings, both about giving memory back rather than keeping
/// it (docs/decisions/014-competitive-targets.md, section 3.2).
///
/// glibc gives each thread that allocates its own arena and keeps that arena
/// for reuse. The document is read and parsed on a worker thread, so the
/// megabytes it touches on the way stay in an arena the reader never uses
/// again — 2 MiB of the 10 MiB fixture's footprint. One arena for the whole
/// process costs a lock a viewer never contends for.
///
/// The second setting fixes the threshold above which an allocation becomes
/// its own mapping. Left dynamic, glibc raises it as large blocks are freed,
/// so the document blobs end up inside the arena and are kept when a document
/// is closed; fixed, they are mappings that go back to the system.
fn tune_allocator() {
    #[cfg(target_env = "gnu")]
    {
        // Negative option ids, as glibc's malloc.h defines them.
        const M_TRIM_THRESHOLD: std::ffi::c_int = -1;
        const M_MMAP_THRESHOLD: std::ffi::c_int = -3;
        const M_ARENA_MAX: std::ffi::c_int = -8;
        extern "C" {
            fn mallopt(param: std::ffi::c_int, value: std::ffi::c_int) -> std::ffi::c_int;
        }
        // Safe: two integers, and neither can invalidate an existing pointer.
        unsafe {
            mallopt(M_ARENA_MAX, 1);
            mallopt(M_MMAP_THRESHOLD, 128 * 1024);
            mallopt(M_TRIM_THRESHOLD, 128 * 1024);
        }
    }
}

/// Returns the memory freed by a load to the operating system.
///
/// Reading and parsing a document allocates a great deal that is released
/// again immediately: the source text, the parser's events, the block table
/// the plan has copied out. glibc keeps those pages for reuse, and they count
/// towards PSS whether or not the reader ever uses them again — 2 MiB on a
/// 100 KiB document and far more on a large one, against a budget that allows
/// twice the file size in total (docs/decisions/014-competitive-targets.md,
/// section 3.2). This is called once per load, never while drawing.
fn release_free_memory() {
    #[cfg(target_env = "gnu")]
    {
        extern "C" {
            fn malloc_trim(pad: usize) -> std::ffi::c_int;
        }
        // Safe: no arguments, no pointers, and it only returns unused pages.
        unsafe {
            malloc_trim(0);
        }
    }
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("md" | "markdown" | "mdown" | "mkd" | "mkdn" | "mdwn")
    )
}

fn read_and_parse(path: &Path) -> Result<(hashline_markdown::OpDocument, u64), String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > SOURCE_LIMIT {
        return Err(format!(
            "Datei ist {:.1} MiB groß, die Grenze liegt bei {} MiB",
            metadata.len() as f64 / (1024.0 * 1024.0),
            SOURCE_LIMIT / (1024 * 1024)
        ));
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    // UTF-8 including a byte order mark; anything else is reported plainly.
    let source =
        String::from_utf8(bytes).map_err(|_| "Datei ist nicht UTF-8-kodiert".to_string())?;
    let source = source.strip_prefix('\u{feff}').unwrap_or(&source);
    Ok((hashline_markdown::parse(source), document::digest(source)))
}

/// Opens the window on one particular monitor, selected by `HASHLINE_MONITOR`
/// against the connector, model, manufacturer or description — `GIGA`, `G27FC`
/// and `DP-3` all find the same panel.
///
/// Wayland gives a client no way to position its own windows, deliberately.
/// `fullscreen_on_monitor` is the one exception, because a fullscreen surface
/// has to name its output. Going fullscreen and straight back out leaves the
/// window on that monitor at its normal size. This is a development aid for a
/// multi-monitor desk, not a product feature: without the variable nothing
/// happens and the compositor places the window.
fn place_on_monitor(window: &gtk::ApplicationWindow) {
    let Ok(wanted) = std::env::var("HASHLINE_MONITOR") else {
        return;
    };
    let wanted = wanted.to_lowercase();
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let monitors = display.monitors();
    let mut found = None;
    let mut available = Vec::new();
    for index in 0..monitors.n_items() {
        let Some(monitor) = monitors.item(index).and_downcast::<gtk::gdk::Monitor>() else {
            continue;
        };
        let fields: Vec<String> = [
            monitor.connector(),
            monitor.model(),
            monitor.manufacturer(),
            monitor.description(),
        ]
        .into_iter()
        .flatten()
        .map(|value| value.to_string())
        .collect();
        available.push(fields.join(" / "));
        if fields
            .iter()
            .any(|value| value.to_lowercase().contains(&wanted))
        {
            found = Some(monitor);
            break;
        }
    }
    let Some(monitor) = found else {
        eprintln!("hashline: no monitor matching {wanted:?}; available:");
        for entry in available {
            eprintln!("  {entry}");
        }
        return;
    };
    window.fullscreen_on_monitor(&monitor);
    glib::idle_add_local_once(glib::clone!(
        #[weak]
        window,
        move || window.unfullscreen()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pump() {
        let context = glib::MainContext::default();
        let until = std::time::Instant::now() + std::time::Duration::from_millis(100);
        while std::time::Instant::now() < until {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// A reload must leave the reader where they were, even when the text
    /// above them changed length (SPEC.md, section 7, and
    /// docs/decisions/014-competitive-targets.md, section 3.3).
    ///
    /// It runs against a mapped window, because the scroll position only
    /// exists once the view has been given a size.
    fn verify_reading_anchor_survives_a_reload(view: &DocumentView) {
        let sections = |prefix: &str| {
            let mut source = String::from(prefix);
            for index in 0..60 {
                source.push_str(&format!(
                    "## Abschnitt {index}\n\nEin Absatz mit genug Text, um Höhe zu haben.\n\n"
                ));
            }
            source
        };
        view.set_document(hashline_markdown::parse(&sections("")));
        pump();
        let target = view
            .outline()
            .block_for_id("doc-abschnitt-40")
            .expect("the forty-first heading");
        view.scroll_to_block(target);
        pump();
        assert!(
            view.scroll_offset() > 0.0,
            "the check must actually scroll away from the top"
        );
        let anchor = view.reading_anchor();
        let held = anchor.heading.clone().expect("a heading to anchor to");

        // The reload: three paragraphs appear above everything, so every block
        // below them moves.
        view.set_document(hashline_markdown::parse(&sections(
            "Neu eins.\n\nNeu zwei.\n\nNeu drei.\n\n",
        )));
        pump();
        view.restore_anchor(&anchor);
        pump();
        let moved = view
            .outline()
            .block_for_id(&held)
            .expect("the heading is still there");
        assert_ne!(moved, target, "the block index must have moved");
        assert!(
            view.scroll_offset() > 0.0,
            "a reload must not send the reader to the top"
        );
        // Within a line of where it sat. Not to the pixel: the blocks now on
        // screen are measured as they are drawn, and each measurement that
        // replaces an estimate above the reader moves the whole document by
        // that difference — which is the mechanism that keeps the *text* still.
        let wanted = view.block_top(moved) + anchor.distance;
        assert!(
            (view.scroll_offset() - wanted).abs() < 40.0,
            "the anchored heading must sit where it sat: {} against {wanted}",
            view.scroll_offset()
        );
        assert_eq!(
            view.reading_anchor().heading.as_deref(),
            Some(held.as_str()),
            "the reader must still be under the heading they were under"
        );

        // A heading that is gone falls back to the block index rather than to
        // the top of the document.
        view.set_document(hashline_markdown::parse(&sections("").replace(
            &format!(
                "## Abschnitt {}\n",
                held.trim_start_matches("doc-abschnitt-")
            ),
            "## Umbenannt\n",
        )));
        pump();
        view.restore_anchor(&anchor);
        pump();
        assert!(
            view.scroll_offset() > 0.0,
            "a renamed heading must not send the reader to the top"
        );
        // Left at the top of a short document, for the checks that follow.
        view.set_document(hashline_markdown::parse("# Eins\n\nText\n"));
        pump();
    }

    /// Pumps until `done` holds, for work that finishes on another thread.
    fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
        for _ in 0..50 {
            pump();
            if done() {
                return;
            }
        }
        panic!("gave up waiting for {what}");
    }

    /// The outline row of a heading id, which is what `active_section` names.
    fn heading_index(view: &DocumentView, id: &str) -> Option<usize> {
        let outline = view.outline();
        (0..outline.len()).find(|&index| outline.id(index) == id)
    }

    fn sections_with_long_paragraphs(count: usize) -> String {
        let paragraph = "Ein Absatz, der lang genug ist, um in einer schmalen Spalte \
                         mehrmals umzubrechen, und in einer breiten seltener. "
            .repeat(3);
        (0..count)
            .map(|index| format!("## Abschnitt {index}\n\n{paragraph}\n\n"))
            .collect()
    }

    /// The line being read stays on screen through everything that sets the
    /// document again without changing it: a zoom, a narrower column, and
    /// another tab in front for a while. Each of them re-estimated the plan
    /// and kept the scroll offset, which by then pointed at other text.
    fn verify_reading_line_survives_a_reflow(ui: &Rc<Ui>) {
        let tab = ui.new_tab();
        tab.view
            .set_document(hashline_markdown::parse(&sections_with_long_paragraphs(80)));
        let other = ui.new_tab();
        other
            .view
            .set_document(hashline_markdown::parse("# Anderes\n\nText.\n"));
        let front = |tab: &Tab| {
            let index = ui.notebook.page_num(&tab.page).expect("a page");
            ui.notebook.set_current_page(Some(index));
            pump();
        };
        front(&tab);
        let target = tab
            .view
            .outline()
            .block_for_id("doc-abschnitt-50")
            .expect("the fifty-first heading");
        tab.view.scroll_to_block(target);
        pump();
        // A little into the section, the way a reader sits in one: a line
        // exactly on a block's edge belongs to either block by rounding.
        let adjustment = tab.view.vadjustment().expect("a scrollable view");
        adjustment.set_value(adjustment.value() + 20.0);
        pump();
        let section = tab.view.active_section();
        assert_eq!(section, heading_index(&tab.view, "doc-abschnitt-50"));
        let offset = || tab.view.scroll_offset() - tab.view.block_top(target);
        // Where the heading sits against where it sat, within what re-setting
        // the text at another size may fairly move a line inside its block.
        let check = |what: &str, before: f64, tolerance: f64| {
            assert_eq!(
                tab.view.active_section(),
                section,
                "{what}: the reader must still be under the same heading"
            );
            let now = offset();
            assert!(
                (now - before).abs() < tolerance,
                "{what}: the heading moved on screen, {now} against {before}"
            );
        };

        let before = offset();
        ui.set_zoom(150);
        pump();
        check("zoom in", before, 40.0);
        ui.set_zoom(100);
        pump();
        check("zoom back", before, 40.0);

        // Narrow enough that the column has to give way, however wide the
        // test window came up.
        let before = offset();
        let width = tab.page.width();
        tab.page.set_margin_end(width - (width * 3 / 5).min(420));
        pump();
        check("narrower column", before, 40.0);
        tab.page.set_margin_end(0);
        pump();
        check("wider column", before, 40.0);

        // Coming back to a tab re-sets the blocks on screen. It must not
        // estimate them again, even when the tab comes back at another height:
        // that is an allocation while its layout cache is still empty, which
        // re-estimated the whole plan and moved the heading by 34 pixels.
        // Nothing about the column changed, so nothing may move at all.
        let before = offset();
        front(&other);
        assert!(!tab.view.holds_layouts());
        tab.page.set_margin_bottom(1);
        front(&tab);
        check("tab switch", before, 1.0);
        tab.page.set_margin_bottom(0);
        pump();
        check("height back", before, 1.0);

        ui.close_tab(&other);
        ui.close_tab(&tab);
    }

    /// Links into another file land on their target, a file reached by two
    /// spellings of its path is one tab, and footnote and GitHub-style
    /// fragments find what they name.
    fn verify_links_between_files(ui: &Rc<Ui>, dir: &Path) {
        let below = dir.join("unter");
        std::fs::create_dir_all(&below).unwrap();
        let mut source = sections_with_long_paragraphs(60);
        source = source.replacen(
            "## Abschnitt 2\n\n",
            "## Abschnitt 2\n\nEin Satz mit Fußnote[^Quelle].\n\n",
            1,
        );
        source = source.replacen(
            "## Abschnitt 40\n\n",
            "[^quelle]: Die Quelle, mitten im Dokument.\n\n## Abschnitt 40\n\n",
            1,
        );
        source = source.replacen(
            "## Abschnitt 45\n\n",
            "## Kopf_zeile\n\nText.\n\n## Abschnitt 45\n\n",
            1,
        );
        let target = dir.join("ziel.md");
        std::fs::write(&target, source).unwrap();
        let start = below.join("start.md");
        std::fs::write(&start, "[weiter](../ziel.md#abschnitt-30)\n").unwrap();
        let target = document::normalize(&target);
        let count = ui.tabs.borrow().len();
        let in_front = |path: &Path| {
            ui.tab()
                .is_some_and(|tab| tab.path.borrow().as_deref() == Some(path))
        };

        ui.open(&start);
        wait_for("the start file", || in_front(&document::normalize(&start)));
        let from = ui.tab().unwrap();

        // Into a file that is not open yet: the jump waits for the load.
        from.view.follow_link("../ziel.md#abschnitt-30");
        wait_for("the linked file", || in_front(&target));
        let ziel = ui.tab().unwrap();
        pump();
        assert_eq!(
            ziel.view.active_section(),
            heading_index(&ziel.view, "doc-abschnitt-30"),
            "the link's fragment must be where the new tab opens"
        );

        // The same file by another path is the tab already open, and the
        // GitHub spelling of an underscore heading finds it.
        from.view.follow_link("../unter/../ziel.md#kopf_zeile");
        pump();
        assert_eq!(ui.tabs.borrow().len(), count + 2, "no second tab");
        assert!(in_front(&target));
        let kopf = heading_index(&ziel.view, "doc-kopf_zeile");
        assert!(kopf.is_some(), "the heading keeps its underscore");
        assert_eq!(ziel.view.active_section(), kopf);

        // A footnote reference jumps to its definition, which is not a heading.
        ziel.view.follow_link("#fn-quelle");
        pump();
        let footnote = ziel
            .view
            .outline()
            .block_for_fragment("fn-quelle")
            .expect("the definition");
        assert!(
            (ziel.view.scroll_offset() - (ziel.view.block_top(footnote) - tokens::PAD_TOP)).abs()
                < 1.0,
            "the definition must be at the top of the view"
        );
        ziel.view.follow_link("#gibt-es-nicht");
        assert!(ui.notice.reveals_child());
        assert!(ui.notice_label.text().contains("gibt-es-nicht"));

        // Closing keeps the position under the one spelling of the path, and
        // opening the file again by the other one finds it.
        ziel.view.follow_link("#kopf_zeile");
        pump();
        ui.close_tab(&ziel);
        ui.open(&below.join("../ziel.md"));
        wait_for("the file again", || in_front(&target));
        pump();
        let again = ui.tab().unwrap();
        // To the pixel, not to the section: a stored distance is whole pixels,
        // and the heading sat exactly on the line that decides the section.
        let kopf = again
            .view
            .outline()
            .block_for_fragment("kopf_zeile")
            .expect("the heading");
        let wanted = again.view.block_top(kopf) - tokens::PAD_TOP;
        assert!(
            (again.view.scroll_offset() - wanted).abs() < 2.0,
            "a file opened again must open where reading stopped: {} against {wanted}",
            again.view.scroll_offset()
        );
        ui.close_tab(&again);
        ui.close_tab(&from);
        assert_eq!(ui.tabs.borrow().len(), count);
    }

    /// Reads a benchmark fixture, if the generated ones are there. They are
    /// not in the repository (`benchmarks/fixtures.py` rebuilds them), so a
    /// check that needs one says so rather than passing on nothing.
    fn fixture(name: &str) -> Option<PathBuf> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/generated")
            .join(name);
        path.is_file().then_some(path)
    }

    /// No single piece of main-thread work over 16 ms, on the fixtures that
    /// used to produce one (SPEC.md, section 9, and
    /// docs/decisions/014-competitive-targets.md, section 3.3).
    ///
    /// Scrolling is done by moving the adjustment, which is what a scroll event
    /// does, so this measures the reader's own work rather than the input
    /// stack's. What it cannot see is a task in code no step here reaches; the
    /// same instrumentation reports from a running program under
    /// `HASHLINE_BENCH_MAIN_THREAD`.
    #[test]
    #[ignore = "needs a GTK display, benchmarks/generated, and a process of its \
                own: GTK may only be initialized once per process"]
    fn main_thread_work_stays_inside_the_frame_budget() {
        gtk::init().expect("GTK display");
        let application = gtk::Application::builder()
            .application_id("de.kalendium.Hashline.Budget")
            .flags(gio::ApplicationFlags::NON_UNIQUE | gio::ApplicationFlags::HANDLES_OPEN)
            .build();
        application.register(gio::Cancellable::NONE).unwrap();
        let ui = Ui::build(&application);
        ui.window.present();
        pump();

        let mut worst: Vec<(String, f64)> = Vec::new();
        for name in [
            "large.md",
            "wide-table.md",
            "long-line.md",
            "large-code.md",
            "many-blocks.md",
            "deep-list.md",
        ] {
            let Some(path) = fixture(name) else {
                panic!("{name} is missing; run: python3 benchmarks/fixtures.py");
            };
            ui.open(&path);
            for _ in 0..40 {
                pump();
                if ui.tab().is_some_and(|tab| tab.path.borrow().is_some()) {
                    break;
                }
            }
            let tab = ui.tab().expect("a tab for the fixture");
            crate::view::mainthread::forget();
            // Through the document in twenty steps, and back up in ten, so
            // that both a fresh layout and a return to evicted blocks are
            // included.
            let adjustment = tab.view.vadjustment().expect("a scrollable view");
            let upper = adjustment.upper() - adjustment.page_size();
            for step in 0..30 {
                let fraction = if step < 20 {
                    step as f64 / 19.0
                } else {
                    1.0 - (step - 20) as f64 / 9.0
                };
                adjustment.set_value(upper * fraction);
                pump();
            }
            worst.push((name.to_string(), crate::view::mainthread::longest()));
            ui.close_tab(&tab);
            pump();
        }
        ui.window.close();
        for (name, longest) in &worst {
            println!("{name}: longest main-thread task {longest:.2} ms");
        }
        let over: Vec<&(String, f64)> = worst.iter().filter(|(_, ms)| *ms > 16.0).collect();
        assert!(over.is_empty(), "over the 16 ms budget: {over:?}");
    }

    #[test]
    #[ignore = "needs a GTK display and a process of its own: GTK may only be \
                initialized once per process"]
    fn native_ui() {
        gtk::init().expect("GTK display");
        let application = gtk::Application::builder()
            .application_id("de.kalendium.Hashline.Test")
            .flags(gio::ApplicationFlags::NON_UNIQUE | gio::ApplicationFlags::HANDLES_OPEN)
            .build();
        application.register(gio::Cancellable::NONE).unwrap();
        let ui = Ui::build(&application);
        ui.window.present();
        pump();
        let probe = ui.new_tab();
        probe.view.verify_accessibility_and_selection();
        verify_reading_anchor_survives_a_reload(&probe.view);
        verify_reading_line_survives_a_reflow(&ui);
        ui.fill_outline();
        assert!(ui.outline_list.model.n_items() > 0);
        assert_eq!(ui.outline_list.selected(), Some(0));
        // The theme switch in the menu is the theme action seen from the
        // other side: whichever way the mode is set, both agree.
        for mode in ["dark", "light", "system"] {
            let action = ui.window.lookup_action("theme").unwrap();
            action.activate(Some(&mode.to_variant()));
            assert_eq!(action.state().unwrap().str(), Some(mode));
            assert_eq!(ui.theme_mode.borrow().as_str(), mode);
            assert_eq!(ui.menu.showing().0, Some(mode));
        }
        // The menu carries no row for the outline or for the search: both have
        // a button of their own in the header bar.
        assert!(ui.menu_button.menu_model().is_none());
        assert_eq!(ui.menu_button.popover().unwrap(), ui.menu.popover);
        ui.window.lookup_action("zoom-in").unwrap().activate(None);
        assert_eq!(ui.menu.showing().1, format!("{} %", ui.zoom()));
        ui.window
            .lookup_action("zoom-reset")
            .unwrap()
            .activate(None);
        assert_eq!(ui.menu.showing().1, "100 %");
        // From closed, whatever the stored preference opened: the order is
        // the order they were opened in, and Escape closes the newest first.
        ui.search_bar.set_search_mode(false);
        ui.outline_revealer.set_reveal_child(false);
        assert!(ui.overlay_order.borrow().is_empty());
        ui.search_bar.set_search_mode(true);
        ui.outline_revealer.set_reveal_child(true);
        assert_eq!(*ui.overlay_order.borrow(), vec!["search", "outline"]);
        // The outline overlays the document and nothing else: the search bar
        // is above the overlay, not underneath it.
        let overlay = ui
            .outline_revealer
            .parent()
            .and_downcast::<gtk::Overlay>()
            .expect("the outline is an overlay");
        assert_eq!(
            overlay.child().unwrap(),
            *ui.stack.upcast_ref::<gtk::Widget>()
        );
        assert!(!ui.search_bar.is_ancestor(&overlay));
        let keys = ui.window.observe_controllers();
        let escape = || {
            for i in 0..keys.n_items() {
                if let Some(key) = keys.item(i).and_downcast::<gtk::EventControllerKey>() {
                    if key.name().as_deref() != Some("close-overlay") {
                        continue;
                    }
                    let _: bool = key.emit_by_name(
                        "key-pressed",
                        &[
                            &gtk::gdk::Key::Escape,
                            &0u32,
                            &gtk::gdk::ModifierType::empty(),
                        ],
                    );
                }
            }
        };
        escape();
        assert!(!ui.outline_revealer.reveals_child());
        assert!(ui.search_bar.is_search_mode());
        escape();
        assert!(!ui.search_bar.is_search_mode());
        ui.menu_button.popup();
        pump();
        escape();
        assert!(!ui.menu_button.popover().unwrap().is_visible());
        let dir = std::env::temp_dir().join(format!("hashline-native-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        verify_links_between_files(&ui, &dir);
        let first = dir.join("eins.md");
        let second = dir.join("zwei.md");
        std::fs::write(&first, "# Eins\n\nText\n").unwrap();
        std::fs::write(&second, "# Zwei\n\nAnderer Text\n").unwrap();
        ui.close_tab(&probe);
        // Two files given at once open two tabs, and the second is in front.
        ui.open_files(&[gio::File::for_path(&first), gio::File::for_path(&second)]);
        pump();
        assert_eq!(ui.tabs.borrow().len(), 2);
        assert_eq!(ui.tab().unwrap().path.borrow().as_ref(), Some(&second));
        assert!(ui.notebook.shows_tabs());
        // A file that is already open is brought forward, not read again.
        ui.open_files(&[gio::File::for_path(&first)]);
        pump();
        assert_eq!(ui.tabs.borrow().len(), 2);
        assert_eq!(ui.tab().unwrap().path.borrow().as_ref(), Some(&first));
        // Each tab keeps its own search, and only the tab in front is set.
        ui.search_bar.set_search_mode(true);
        ui.search_entry.set_text("Text");
        ui.run_search("Text");
        pump();
        let (front, back) = (ui.tabs.borrow()[0].clone(), ui.tabs.borrow()[1].clone());
        assert_eq!(front.view.search_position().1, 1);
        assert_eq!(back.view.search_position().1, 0);
        ui.step_tab(1);
        pump();
        assert_eq!(ui.tab().unwrap().path.borrow().as_ref(), Some(&second));
        assert_eq!(ui.search_entry.text().as_str(), "");
        // An inactive tab holds no layout cache.
        assert!(!front.view.holds_layouts());
        assert!(back.view.holds_layouts());
        ui.step_tab(-1);
        pump();
        assert_eq!(ui.tab().unwrap().path.borrow().as_ref(), Some(&first));
        assert_eq!(ui.search_entry.text().as_str(), "Text");
        // Ctrl+W closes the tab in front and leaves the other open.
        ui.window.lookup_action("close-tab").unwrap().activate(None);
        pump();
        assert_eq!(ui.tabs.borrow().len(), 1);
        assert!(!ui.notebook.shows_tabs());
        ui.window.lookup_action("close-tab").unwrap().activate(None);
        pump();
        assert!(ui.tabs.borrow().is_empty());
        assert_eq!(ui.stack.visible_child_name().as_deref(), Some("empty"));
        ui.open_files(&[gio::File::for_path(&second)]);
        pump();
        assert_eq!(application.windows().len(), 1);
        ui.window.close();
        // Exercise the real activation/open handlers, including a fresh
        // activation after closing the only window.
        install_application(&application);
        application.activate();
        application.activate();
        pump();
        assert_eq!(application.windows().len(), 1);
        let window = application.active_window().unwrap();
        application.open(&[gio::File::for_path(&first)], "");
        application.open(&[gio::File::for_path(&second)], "");
        pump();
        // A second invocation opens another tab of the same window.
        assert_eq!(application.windows().len(), 1);
        assert_eq!(application.active_window().unwrap(), window);
        assert_eq!(window.tooltip_text().as_deref(), second.to_str());
        window.close();
        application.activate();
        pump();
        assert_eq!(application.windows().len(), 1);
        application.active_window().unwrap().close();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
