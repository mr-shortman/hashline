//! The application: window, header bar, actions, document loading
//! (SPEC.md, sections 3, 5 and 7).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

use crate::document::{self, Anchor, Watch};
use crate::preferences::Preferences;
use crate::theme::{document as tokens, DARK, LIGHT};
use crate::view::DocumentView;

pub const APP_ID: &str = "de.kalendium.Hashline";

/// A development ceiling on how much Markdown is read at all, so a stray file
/// cannot pull the process over (SPEC.md, section 10).
const SOURCE_LIMIT: u64 = 20 * 1024 * 1024;

/// How long typing settles before a search runs (SPEC.md, section 8).
const SEARCH_DEBOUNCE_MS: u64 = 120;

/// Above this window width the outline gets its own column instead of
/// floating over the text (SPEC.md, section 3).
const OUTLINE_SIDEBAR_WIDTH: i32 = 900;

pub fn run() -> glib::ExitCode {
    glib::set_application_name("Hashline");
    let application = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN | gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    application.set_option_context_parameter_string(Some("[FILE …]"));
    application.set_option_context_summary(Some(
        "Open Markdown in one window. If several files are given, the first is opened.",
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
    view: DocumentView,
    title: gtk::Label,
    stack: gtk::Stack,
    search_bar: gtk::SearchBar,
    search_entry: gtk::SearchEntry,
    search_count: gtk::Label,
    menu_button: gtk::MenuButton,
    notice: gtk::Revealer,
    notice_label: gtk::Label,
    notice_generation: Cell<u64>,
    overlay_order: RefCell<Vec<&'static str>>,
    theme_mode: RefCell<String>,
    system_dark: Cell<bool>,
    theme_ready: Cell<bool>,
    present_requested: Cell<bool>,
    outline_revealer: gtk::Revealer,
    outline_list: gtk::ListBox,
    /// The document area, indented when the outline shows as a sidebar.
    content: gtk::Box,
    /// Which blocks the outline rows point at, by row index.
    outline_blocks: RefCell<Vec<usize>>,
    banner: gtk::Revealer,
    banner_label: gtk::Label,
    current: RefCell<Option<PathBuf>>,
    /// The watch on the open file. Replacing it stops the previous one.
    watch: RefCell<Option<Watch>>,
    /// Digest of the source now showing, so a watch event that changed
    /// nothing does not cost a re-render (SPEC.md, section 7).
    digest: Cell<u64>,
    preferences: Preferences,
    /// Rising request id; only the newest load may replace the document
    /// (SPEC.md, section 5, "Zustandsmodell").
    request: Cell<u64>,
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
        if maximized {
            window.maximize();
        }

        let view = DocumentView::new();
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&view)
            .build();

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
        stack.add_named(&scroller, Some("document"));
        stack.set_visible_child_name("empty");

        // Search: a bar over the document, with the hit count beside the field
        // and the usual next/previous (SPEC.md, section 3).
        let search_entry = gtk::SearchEntry::new();
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

        // The outline: an overlay over the document, given room as a sidebar
        // once the window is wide enough (SPEC.md, section 3).
        let outline_list = gtk::ListBox::new();
        outline_list.set_selection_mode(gtk::SelectionMode::Single);
        let outline_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .width_request(260)
            .child(&outline_list)
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
        content.append(&stack);

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&content));
        overlay.add_overlay(&outline_revealer);

        let outline_button = gtk::ToggleButton::new();
        outline_button.set_icon_name("view-list-symbolic");
        outline_button.set_tooltip_text(Some("Inhaltsverzeichnis (Ctrl+Shift+O)"));
        let search_button = gtk::ToggleButton::new();
        search_button.set_icon_name("system-search-symbolic");
        search_button.set_tooltip_text(Some("Suchen (Ctrl+F)"));
        let menu = gio::Menu::new();
        menu.append(Some("Datei öffnen …"), Some("win.open"));
        menu.append(Some("Neu laden"), Some("win.reload"));
        let navigation = gio::Menu::new();
        navigation.append(Some("Inhaltsverzeichnis"), Some("win.outline"));
        navigation.append(Some("Suchen"), Some("win.find"));
        menu.append_section(None, &navigation);
        let appearance = gio::Menu::new();
        appearance.append(Some("System"), Some("win.theme::system"));
        appearance.append(Some("Hell"), Some("win.theme::light"));
        appearance.append(Some("Dunkel"), Some("win.theme::dark"));
        menu.append_section(Some("Darstellung"), &appearance);
        let zoom = gio::Menu::new();
        zoom.append(Some("Text vergrößern"), Some("win.zoom-in"));
        zoom.append(Some("Text verkleinern"), Some("win.zoom-out"));
        zoom.append(Some("Originalgröße"), Some("win.zoom-reset"));
        menu.append_section(None, &zoom);
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .tooltip_text("Menü")
            .menu_model(&menu)
            .build();
        header.pack_end(&menu_button);
        header.pack_end(&search_button);
        header.pack_end(&outline_button);

        window.set_titlebar(Some(&header));
        window.set_child(Some(&overlay));

        let ui = Rc::new(Ui {
            window,
            view,
            title,
            stack,
            banner,
            banner_label,
            search_bar: search_bar.clone(),
            search_entry: search_entry.clone(),
            search_count,
            menu_button,
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
            content,
            outline_blocks: RefCell::new(Vec::new()),
            current: RefCell::new(None),
            watch: RefCell::new(None),
            digest: Cell::new(0),
            preferences,
            request: Cell::new(0),
        });

        ui.install_theme();
        ui.install_escape();
        ui.install_search(&previous_hit, &next_hit);
        ui.install_outline();
        ui.install_actions(application);
        ui.install_drop_target();
        ui.install_links();

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
        ui.view.set_zoom(ui.preferences.zoom());
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
                ui.remember_position();
                ui.watch.borrow_mut().take();
                ui.request.set(ui.request.get() + 1);
                ui.preferences.set_window_size(
                    window.width(),
                    window.height(),
                    window.is_maximized(),
                );
                ui.preferences.set_zoom(ui.view.zoom());
                ui.preferences
                    .set_outline_visible(ui.outline_revealer.reveals_child());
                glib::Propagation::Proceed
            }
        ));

        place_on_monitor(&ui.window);
        ui
    }

    /// Records where reading stopped in the file that is showing.
    fn remember_position(&self) {
        let path = self.current.borrow().clone();
        if let Some(path) = path {
            self.preferences
                .remember_position(&path, &self.view.reading_anchor());
        }
    }

    /// Debounced so that typing does not run a scan per keystroke, and the
    /// answer to an older query can never overwrite a newer one
    /// (SPEC.md, section 8).
    fn install_search(self: &Rc<Self>, previous: &gtk::Button, next: &gtk::Button) {
        let pending: Rc<Cell<u64>> = Rc::new(Cell::new(0));
        let ui = self.clone();
        let token = pending.clone();
        self.search_entry.connect_search_changed(move |entry| {
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
            ui.view.search_next();
            ui.update_search_count();
        });
        let ui = self.clone();
        self.search_entry.connect_previous_match(move |_| {
            ui.view.search_previous();
            ui.update_search_count();
        });
        let ui = self.clone();
        next.connect_clicked(move |_| {
            ui.view.search_next();
            ui.update_search_count();
        });
        let ui = self.clone();
        previous.connect_clicked(move |_| {
            ui.view.search_previous();
            ui.update_search_count();
        });
        // Closing the bar clears the marks, and the document keeps the focus
        // it had (SPEC.md, section 3).
        let ui = self.clone();
        self.search_bar
            .connect_search_mode_enabled_notify(move |bar| {
                if !bar.is_search_mode() {
                    ui.view.clear_search();
                    ui.search_count.set_text("");
                    ui.view.grab_focus();
                }
            });
    }

    fn run_search(&self, needle: &str) {
        if needle.is_empty() {
            self.view.clear_search();
            self.search_count.set_text("");
            return;
        }
        self.view.search(needle);
        self.update_search_count();
    }

    fn update_search_count(&self) {
        let (current, total) = self.view.search_position();
        self.search_count.set_text(&match (current, total) {
            (_, 0) => "Kein Treffer".to_string(),
            (0, total) => format!("{total} Treffer"),
            (current, total) => format!("{current} von {total}"),
        });
    }

    fn install_outline(self: &Rc<Self>) {
        let ui = Rc::downgrade(self);
        self.view
            .connect_local("active-section-changed", false, move |_| {
                if let Some(ui) = ui.upgrade() {
                    ui.update_active_section();
                    ui.update_outline_mode();
                }
                None
            });
        let ui = self.clone();
        self.outline_list.connect_row_activated(move |_, row| {
            let index = row.index();
            if index < 0 {
                return;
            }
            if let Some(&block) = ui.outline_blocks.borrow().get(index as usize) {
                ui.view.scroll_to_block(block);
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
        self.content
            .set_margin_start(if wide && showing { 260 } else { 0 });
    }

    fn fill_outline(&self) {
        while let Some(row) = self.outline_list.first_child() {
            self.outline_list.remove(&row);
        }
        let outline = self.view.outline();
        let mut blocks = Vec::new();
        for entry in outline.entries() {
            let label = gtk::Label::new(Some(&entry.text));
            label.set_xalign(0.0);
            label.set_ellipsize(pango::EllipsizeMode::End);
            // Depth by indent, so the structure is visible without markup.
            label.set_margin_start(8 + 12 * (entry.level.saturating_sub(1)) as i32);
            label.set_margin_end(8);
            label.set_margin_top(4);
            label.set_margin_bottom(4);
            self.outline_list.append(&label);
            blocks.push(entry.block);
        }
        *self.outline_blocks.borrow_mut() = blocks;
        self.update_active_section();
    }

    fn update_active_section(&self) {
        let row = self
            .view
            .active_section()
            .and_then(|index| self.outline_list.row_at_index(index as i32));
        if self.outline_list.selected_row() != row {
            self.outline_list.select_row(row.as_ref());
        }
    }

    fn open_files(self: &Rc<Self>, files: &[gio::File]) {
        if let Some(file) = files.first() {
            if let Some(path) = file.path() {
                self.open(&path);
            } else {
                self.note("Nur lokale Markdown-Dateien können geöffnet werden.");
            }
        }
        if files.len() > 1 {
            self.note("Es kann nur eine Datei geöffnet sein. Die erste Datei wurde gewählt.");
        }
    }

    /// Reads and parses off the main thread, then applies the result if it is
    /// still the newest request (SPEC.md, sections 5 and 10).
    fn open(self: &Rc<Self>, path: &Path) {
        self.load(path, None);
    }

    /// A reload of the file already showing, keeping the reading position.
    fn reload(self: &Rc<Self>) {
        let path = self.current.borrow().clone();
        if let Some(path) = path {
            let anchor = self.view.reading_anchor();
            self.load(&path, Some(anchor));
        }
    }

    fn load(self: &Rc<Self>, path: &Path, anchor: Option<Anchor>) {
        let path = path.to_path_buf();
        self.request.set(self.request.get() + 1);
        let request = self.request.get();
        let (sender, receiver) = async_channel::bounded(1);
        let for_thread = path.clone();
        std::thread::spawn(move || {
            let result = read_and_parse(&for_thread);
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
            if ui.request.get() != request {
                return;
            }
            match loaded.result {
                Ok((parsed, digest)) => {
                    let reloading = loaded.anchor.is_some();
                    // Nothing changed: leave the view, and the reader, alone.
                    if reloading && digest == ui.digest.get() {
                        return;
                    }
                    // Leaving one document for another: keep the place in the
                    // one being left (SPEC.md, section 7).
                    if !reloading {
                        ui.remember_position();
                    }
                    let stored = (!reloading)
                        .then(|| ui.preferences.reading_position(&loaded.path))
                        .flatten();
                    ui.digest.set(digest);
                    ui.show(loaded.path, parsed);
                    if let Some(anchor) = loaded.anchor.or(stored) {
                        ui.view.restore_anchor(&anchor);
                    }
                }
                Err(error) => ui.show_error(&loaded.path, &error),
            }
        });
    }

    fn show(self: &Rc<Self>, path: PathBuf, document: hashline_markdown::OpDocument) {
        // Relative picture paths resolve against the document's directory,
        // never the process working directory (SPEC.md, section 7).
        self.view
            .set_base_directory(path.parent().map(Path::to_path_buf));
        self.view.set_document(Rc::new(document));
        // The shown name changes only once the new document is actually in
        // place (SPEC.md, section 7).
        if let Some(name) = path.file_name() {
            self.title.set_text(&name.to_string_lossy());
        }
        self.window.set_tooltip_text(Some(&path.to_string_lossy()));
        *self.current.borrow_mut() = Some(path.clone());
        self.start_watch(&path);
        self.stack.set_visible_child_name("document");
        self.banner.set_reveal_child(false);
        self.fill_outline();
    }

    /// Replaces the watch, which stops the previous one, and reloads on
    /// change. A failure to watch is not fatal: manual reload stays available
    /// (SPEC.md, section 7).
    fn start_watch(self: &Rc<Self>, path: &Path) {
        let ui = Rc::downgrade(self);
        let watch = document::watch(path, move || {
            if let Some(ui) = ui.upgrade() {
                ui.reload();
            }
        });
        *self.watch.borrow_mut() = watch;
    }

    fn show_error(&self, path: &Path, error: &str) {
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
    fn install_links(self: &Rc<Self>) {
        let ui = self.clone();
        self.view.connect_link_activated(move |href| {
            let decoded = glib::Uri::unescape_string(href, None)
                .map(|value| value.to_string())
                .unwrap_or_else(|| href.to_string());

            if let Some(fragment) = decoded.strip_prefix('#') {
                // Markdown writes `#kapitel`; the generated ids are prefixed,
                // so both spellings are tried before giving up.
                if !ui.view.scroll_to_anchor(fragment)
                    && !ui.view.scroll_to_anchor(&format!("doc-{fragment}"))
                {
                    ui.note(&format!("Kein Abschnitt „{fragment}“ in diesem Dokument"));
                }
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
                    let base = ui
                        .current
                        .borrow()
                        .as_ref()
                        .and_then(|path| path.parent().map(Path::to_path_buf))
                        .unwrap_or_else(|| PathBuf::from("."));
                    // Relative paths resolve against the document, never
                    // against the process working directory.
                    let (target, fragment) = match decoded.split_once('#') {
                        Some((path, fragment)) => (path, Some(fragment.to_string())),
                        None => (decoded.as_str(), None),
                    };
                    let resolved = base.join(target);
                    if is_markdown(&resolved) {
                        ui.open(&resolved);
                        if let Some(fragment) = fragment {
                            // The jump happens once the document is in place.
                            let ui = ui.clone();
                            glib::idle_add_local_once(move || {
                                ui.view.scroll_to_anchor(&format!("doc-{fragment}"));
                            });
                        }
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
                ui.outline_list.grab_focus();
            } else {
                ui.view.grab_focus();
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
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_application_prefer_dark_theme(dark);
        }
        self.view.set_palette(if dark { DARK } else { LIGHT });
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
                ui.apply_theme();
            }
        });
        self.window.add_action(&theme);
        let view = &self.view;
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
                        ui.view.copy_selection();
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
                        ui.view.select_all();
                    }
                }
            )),
        );
        add(
            "zoom-in",
            &["<Control>plus", "<Control>equal"],
            Box::new(glib::clone!(
                #[weak]
                view,
                move || view.set_zoom(view.zoom() + tokens::ZOOM_STEP)
            )),
        );
        add(
            "zoom-out",
            &["<Control>minus"],
            Box::new(glib::clone!(
                #[weak]
                view,
                move || view.set_zoom(view.zoom() - tokens::ZOOM_STEP)
            )),
        );
        add(
            "zoom-reset",
            &["<Control>0"],
            Box::new(glib::clone!(
                #[weak]
                view,
                move || view.set_zoom(100)
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

    #[test]
    #[ignore = "requires a GTK display; run with gtk4-broadwayd and GDK_BACKEND=broadway"]
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
        ui.view.verify_accessibility_and_selection();
        ui.fill_outline();
        assert!(ui.outline_list.row_at_index(0).is_some());
        assert_eq!(ui.outline_list.selected_row().unwrap().index(), 0);
        for mode in ["dark", "light", "system"] {
            let action = ui.window.lookup_action("theme").unwrap();
            action.activate(Some(&mode.to_variant()));
            assert_eq!(action.state().unwrap().str(), Some(mode));
            assert_eq!(ui.theme_mode.borrow().as_str(), mode);
        }
        assert!(ui.menu_button.menu_model().unwrap().n_items() >= 4);
        ui.search_bar.set_search_mode(true);
        ui.outline_revealer.set_reveal_child(true);
        assert_eq!(*ui.overlay_order.borrow(), vec!["search", "outline"]);
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
        let first = dir.join("eins.md");
        let second = dir.join("zwei.md");
        std::fs::write(&first, "# Eins\n\nText\n").unwrap();
        std::fs::write(&second, "# Zwei\n\nAnderer Text\n").unwrap();
        ui.open_files(&[gio::File::for_path(&first), gio::File::for_path(&second)]);
        pump();
        assert_eq!(ui.current.borrow().as_ref(), Some(&first));
        assert!(
            ui.notice.reveals_child(),
            "loading must not hide the multiple-file notice"
        );
        ui.open_files(&[gio::File::for_path(&second)]);
        pump();
        assert_eq!(ui.current.borrow().as_ref(), Some(&second));
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
