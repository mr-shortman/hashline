//! The application: window, header bar, actions (SPEC.md, sections 3 and 5).
//!
//! M0 keeps this deliberately thin. Its job is to put the document widget on
//! screen so the four risks of the milestone can be judged, not to be the
//! finished shell — the file dialog, the outline and the search bar arrive with
//! M1 and M2.

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

use crate::theme::{document as tokens, Variant, DARK, LIGHT};
use crate::view::DocumentView;

pub const APP_ID: &str = "de.kalendium.Hashline";

pub fn run() -> glib::ExitCode {
    let application = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    application.connect_open(|application, files, _| {
        let path = files.first().and_then(|file| file.path());
        // Several files at once: the first one opens, and v1 says so rather
        // than silently dropping the rest (SPEC.md, section 3).
        if files.len() > 1 {
            eprintln!("hashline: opening only the first of {} files", files.len());
        }
        present(application, path);
    });
    application.connect_activate(|application| present(application, None));

    application.run()
}

fn present(application: &gtk::Application, path: Option<std::path::PathBuf>) {
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
    let header = gtk::HeaderBar::builder().title_widget(&title).build();
    window.set_titlebar(Some(&header));
    window.set_child(Some(&scroller));

    apply_theme(&view);
    install_actions(application, &window, &view);

    if let Some(path) = path {
        match std::fs::read_to_string(&path) {
            Ok(source) => {
                let parsed = std::rc::Rc::new(hashline_markdown::parse(&source));
                view.set_document(parsed);
                if let Some(name) = path.file_name() {
                    title.set_text(&name.to_string_lossy());
                }
                window.set_tooltip_text(Some(&path.to_string_lossy()));
            }
            Err(error) => eprintln!("hashline: {}: {error}", path.display()),
        }
    }

    window.present();
}

/// The theme is settled before the window shows content, so no wrong colour
/// scheme flashes on start (SPEC.md, section 3).
fn apply_theme(view: &DocumentView) {
    let settings = gtk::Settings::default();
    let dark = settings
        .as_ref()
        .map(|settings| settings.is_gtk_application_prefer_dark_theme())
        .unwrap_or(false);
    view.set_palette(if dark { DARK } else { LIGHT });
    if let Some(settings) = settings {
        settings.connect_gtk_application_prefer_dark_theme_notify(glib::clone!(
            #[weak]
            view,
            move |settings| {
                view.set_palette(if settings.is_gtk_application_prefer_dark_theme() {
                    DARK
                } else {
                    LIGHT
                });
            }
        ));
    }
    let _ = Variant::Light;
}

/// Actions are registered once and bound to keys through the application, so
/// menu, keyboard and accessibility share one source (SPEC.md, section 3).
fn install_actions(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    view: &DocumentView,
) {
    let add = |name: &str, keys: &[&str], callback: Box<dyn Fn()>| {
        let action = gio::SimpleAction::new(name, None);
        action.connect_activate(move |_, _| callback());
        window.add_action(&action);
        application.set_accels_for_action(&format!("win.{name}"), keys);
    };

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
}
