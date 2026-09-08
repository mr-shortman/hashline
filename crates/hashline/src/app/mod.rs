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
    let application = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    application.connect_open(|application, files, _| {
        if files.len() > 1 {
            eprintln!("hashline: opening only the first of {} files", files.len());
        }
        let ui = Ui::build(application);
        if let Some(path) = files.first().and_then(|file| file.path()) {
            ui.open(&path);
        }
        ui.window.present();
    });
    application.connect_activate(|application| {
        let ui = Ui::build(application);
        ui.window.present();
    });

    application.run()
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

        ui.apply_theme();
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
        let ui = self.clone();
        self.window
            .connect_default_width_notify(move |_| ui.update_outline_mode());
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
        let target = gtk::DropTarget::new(gio::File::static_type(), gtk::gdk::DragAction::COPY);
        let ui = self.clone();
        target.connect_drop(move |_, value, _, _| {
            if let Ok(file) = value.get::<gio::File>() {
                if let Some(path) = file.path() {
                    ui.open(&path);
                    return true;
                }
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

    fn note(&self, message: &str) {
        self.banner_label.set_text(message);
        self.banner.set_reveal_child(true);
    }

    /// The theme is settled before the window shows content, so no wrong
    /// colour scheme flashes on start (SPEC.md, section 3).
    fn apply_theme(&self) {
        let settings = gtk::Settings::default();
        let dark = settings
            .as_ref()
            .map(|settings| settings.is_gtk_application_prefer_dark_theme())
            .unwrap_or(false);
        self.view.set_palette(if dark { DARK } else { LIGHT });
        if let Some(settings) = settings {
            settings.connect_gtk_application_prefer_dark_theme_notify(glib::clone!(
                #[weak(rename_to = view)]
                self.view,
                move |settings| {
                    view.set_palette(if settings.is_gtk_application_prefer_dark_theme() {
                        DARK
                    } else {
                        LIGHT
                    });
                }
            ));
        }
    }

    /// Actions are registered once and bound to keys through the application,
    /// so menu, keyboard and accessibility share one source
    /// (SPEC.md, section 3).
    fn install_actions(self: &Rc<Self>, application: &gtk::Application) {
        let add = |name: &str, keys: &[&str], callback: Box<dyn Fn()>| {
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| callback());
            self.window.add_action(&action);
            application.set_accels_for_action(&format!("win.{name}"), keys);
        };

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
                #[weak]
                view,
                move || view.copy_selection()
            )),
        );
        add(
            "select-all",
            &["<Control>a"],
            Box::new(glib::clone!(
                #[weak]
                view,
                move || view.select_all()
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
