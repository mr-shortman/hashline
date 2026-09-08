//! The application: window, header bar, actions, document loading
//! (SPEC.md, sections 3, 5 and 7).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

use crate::theme::{document as tokens, DARK, LIGHT};
use crate::view::DocumentView;

pub const APP_ID: &str = "de.kalendium.Hashline";

/// A development ceiling on how much Markdown is read at all, so a stray file
/// cannot pull the process over (SPEC.md, section 10).
const SOURCE_LIMIT: u64 = 20 * 1024 * 1024;

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
    result: Result<hashline_markdown::OpDocument, String>,
}

struct Ui {
    window: gtk::ApplicationWindow,
    view: DocumentView,
    title: gtk::Label,
    stack: gtk::Stack,
    banner: gtk::Revealer,
    banner_label: gtk::Label,
    current: RefCell<Option<PathBuf>>,
    /// Rising request id; only the newest load may replace the document
    /// (SPEC.md, section 5, "Zustandsmodell").
    request: Cell<u64>,
}

impl Ui {
    fn build(application: &gtk::Application) -> Rc<Self> {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .default_width(900)
            .default_height(700)
            .build();

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
        content.append(&banner);
        content.append(&stack);

        window.set_titlebar(Some(&header));
        window.set_child(Some(&content));

        let ui = Rc::new(Ui {
            window,
            view,
            title,
            stack,
            banner,
            banner_label,
            current: RefCell::new(None),
            request: Cell::new(0),
        });

        ui.apply_theme();
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
            move |_| {
                let path = ui.current.borrow().clone();
                if let Some(path) = path {
                    ui.open(&path);
                }
            }
        ));
        place_on_monitor(&ui.window);
        ui
    }

    /// Reads and parses off the main thread, then applies the result if it is
    /// still the newest request (SPEC.md, sections 5 and 10).
    fn open(self: &Rc<Self>, path: &Path) {
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
                Ok(document) => ui.show(loaded.path, document),
                Err(error) => ui.show_error(&loaded.path, &error),
            }
        });
    }

    fn show(&self, path: PathBuf, document: hashline_markdown::OpDocument) {
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
        *self.current.borrow_mut() = Some(path);
        self.stack.set_visible_child_name("document");
        self.banner.set_reveal_child(false);
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
            "reload",
            &["<Control>r"],
            Box::new(glib::clone!(
                #[strong(rename_to = ui)]
                self,
                move || {
                    let path = ui.current.borrow().clone();
                    if let Some(path) = path {
                        ui.open(&path);
                    }
                }
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

fn read_and_parse(path: &Path) -> Result<hashline_markdown::OpDocument, String> {
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
    Ok(hashline_markdown::parse(source))
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
