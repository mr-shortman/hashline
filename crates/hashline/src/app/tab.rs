//! One open document: its view, its file, its watch and its search.
//!
//! Everything that belongs to a document rather than to the window lives here,
//! which is what makes several of them at once possible at all
//! (docs/decisions/014-competitive-targets.md, section 2.2). The window owns
//! one search field, one outline and one header bar; each tab owns the state
//! those show.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::prelude::*;

use crate::document::Watch;
use crate::view::DocumentView;

pub struct Tab {
    pub view: DocumentView,
    /// What the notebook holds for this tab.
    pub page: gtk::ScrolledWindow,
    /// The tab's label, and the box the notebook shows as its handle.
    pub title: gtk::Label,
    pub handle: gtk::Box,
    pub close: gtk::Button,
    /// The file showing, once one has loaded.
    pub path: RefCell<Option<PathBuf>>,
    /// The watch on that file. Replacing it stops the previous one.
    pub watch: RefCell<Option<Watch>>,
    /// Digest of the source now showing, so a watch event that changed
    /// nothing does not cost a re-render (SPEC.md, section 7).
    pub digest: Cell<u64>,
    /// Rising request id; only the newest load may replace this tab's document
    /// (SPEC.md, section 5, "Zustandsmodell").
    pub request: Cell<u64>,
    /// What the search field held for this tab, and whether its bar was open.
    /// A tab keeps its own search (decision 014, section 2.2).
    pub query: RefCell<String>,
    pub searching: Cell<bool>,
}

impl Tab {
    pub fn new() -> Rc<Self> {
        let view = DocumentView::new();
        let page = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&view)
            .build();
        let title = gtk::Label::new(Some("Ohne Titel"));
        title.set_ellipsize(pango::EllipsizeMode::Middle);
        title.set_max_width_chars(20);
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.set_tooltip_text(Some("Tab schließen (Ctrl+W)"));
        let handle = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        handle.append(&title);
        handle.append(&close);
        Rc::new(Tab {
            view,
            page,
            title,
            handle,
            close,
            path: RefCell::new(None),
            watch: RefCell::new(None),
            digest: Cell::new(0),
            request: Cell::new(0),
            query: RefCell::new(String::new()),
            searching: Cell::new(false),
        })
    }

    pub fn set_path(&self, path: &Path) {
        if let Some(name) = path.file_name() {
            self.title.set_text(&name.to_string_lossy());
        }
        self.handle.set_tooltip_text(Some(&path.to_string_lossy()));
        *self.path.borrow_mut() = Some(path.to_path_buf());
    }

    /// Everything this tab holds that can be built again. Called when the tab
    /// stops showing, so that an inactive tab costs its document and its plan
    /// and nothing else.
    pub fn deactivate(&self) {
        self.view.release_layout_cache();
    }

    /// Stops watching. A closed tab must not keep a file descriptor or wake
    /// the process for a document nobody is reading.
    pub fn close(&self) {
        self.watch.borrow_mut().take();
        self.request.set(self.request.get() + 1);
    }
}

/// A notebook that shows tabs only once there is more than one document, so a
/// single open file looks exactly as it did before tabs existed.
pub fn notebook() -> gtk::Notebook {
    let notebook = gtk::Notebook::new();
    notebook.set_scrollable(true);
    notebook.set_show_border(false);
    notebook.set_show_tabs(false);
    notebook
}

/// Keeps the tab strip hidden while one document is open.
pub fn update_strip(notebook: &gtk::Notebook) {
    notebook.set_show_tabs(notebook.n_pages() > 1);
}
