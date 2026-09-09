//! The outline as a virtualized list.
//!
//! A `GtkListBox` with one row per heading was the first implementation and is
//! the single largest memory post the reader had: 28 931 headings in the 10 MiB
//! fixture cost 153 MiB of widgets, whether or not the outline was ever opened
//! (benchmarks/results/native-current/REPORT.md, finding 2). A `GtkListView`
//! over a list model builds a widget per *visible* row instead, and this model
//! creates a row object only when it is asked for one — so an outline of thirty
//! thousand headings costs the twenty rows on screen.

use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, pango};

use crate::outline::Outline;

/// Indent per heading level, in pixels.
const INDENT: i32 = 12;

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct Row {
        pub index: Cell<u32>,
        pub level: Cell<u32>,
        pub text: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Row {
        const NAME: &'static str = "HashlineOutlineRow";
        type Type = super::Row;
    }
    impl ObjectImpl for Row {}

    #[derive(Default)]
    pub struct Model {
        pub outline: RefCell<Rc<Outline>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Model {
        const NAME: &'static str = "HashlineOutlineModel";
        type Type = super::Model;
        type Interfaces = (gio::ListModel,);
    }
    impl ObjectImpl for Model {}

    impl ListModelImpl for Model {
        fn item_type(&self) -> glib::Type {
            super::Row::static_type()
        }
        fn n_items(&self) -> u32 {
            self.outline.borrow().len() as u32
        }
        /// Built on demand: the list view asks only for the rows it shows.
        fn item(&self, position: u32) -> Option<glib::Object> {
            let outline = self.outline.borrow();
            let entry = outline.entry(position as usize)?;
            Some(
                super::Row::new(
                    position,
                    entry.level as u32,
                    outline.text(position as usize),
                )
                .upcast(),
            )
        }
    }
}

glib::wrapper! {
    pub struct Row(ObjectSubclass<imp::Row>);
}

impl Row {
    fn new(index: u32, level: u32, text: &str) -> Self {
        let row: Self = glib::Object::new();
        row.imp().index.set(index);
        row.imp().level.set(level);
        *row.imp().text.borrow_mut() = text.to_string();
        row
    }
    pub fn index(&self) -> u32 {
        self.imp().index.get()
    }
}

glib::wrapper! {
    pub struct Model(ObjectSubclass<imp::Model>) @implements gio::ListModel;
}

impl Default for Model {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Model {
    /// Replaces the whole outline. One change notification covers it: the list
    /// view drops its rows and asks again for the ones it needs.
    pub fn set_outline(&self, outline: Rc<Outline>) {
        let removed = self.n_items();
        let added = outline.len() as u32;
        *self.imp().outline.borrow_mut() = outline;
        self.items_changed(0, removed, added);
    }
    pub fn outline(&self) -> Rc<Outline> {
        self.imp().outline.borrow().clone()
    }
}

/// The list view, its selection and the factory that fills a row.
pub struct OutlineList {
    pub view: gtk::ListView,
    pub model: Model,
    pub selection: gtk::SingleSelection,
}

impl OutlineList {
    pub fn new() -> Self {
        let model = Model::default();
        let selection = gtk::SingleSelection::builder()
            .model(&model)
            .autoselect(false)
            .can_unselect(true)
            .build();
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let label = gtk::Label::new(None);
            label.set_xalign(0.0);
            label.set_ellipsize(pango::EllipsizeMode::End);
            label.set_margin_end(8);
            label.set_margin_top(4);
            label.set_margin_bottom(4);
            item.downcast_ref::<gtk::ListItem>()
                .expect("list item")
                .set_child(Some(&label));
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("list item");
            let Some(row) = item.item().and_downcast::<Row>() else {
                return;
            };
            let Some(label) = item.child().and_downcast::<gtk::Label>() else {
                return;
            };
            label.set_text(&row.imp().text.borrow());
            // Depth by indent, so the structure is visible without markup.
            label.set_margin_start(8 + INDENT * row.imp().level.get().saturating_sub(1) as i32);
        });
        let view = gtk::ListView::builder()
            .model(&selection)
            .factory(&factory)
            // A heading is followed the way a list row always was: one click.
            .single_click_activate(true)
            .build();
        OutlineList {
            view,
            model,
            selection,
        }
    }

    /// The heading a row stands for, as a block of the plan.
    pub fn block_at(&self, position: u32) -> Option<usize> {
        self.model
            .outline()
            .entry(position as usize)
            .map(|entry| entry.block as usize)
    }

    pub fn select(&self, index: Option<u32>) {
        let wanted = index.unwrap_or(gtk::INVALID_LIST_POSITION);
        if self.selection.selected() != wanted {
            self.selection.set_selected(wanted);
        }
    }

    #[cfg(test)]
    pub fn selected(&self) -> Option<u32> {
        let selected = self.selection.selected();
        (selected != gtk::INVALID_LIST_POSITION).then_some(selected)
    }
}
