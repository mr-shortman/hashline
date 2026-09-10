//! The document widget: virtualized block layout, scrolling, selection.
//!
//! It implements `GtkScrollable` rather than growing to the document's full
//! height inside a viewport. That is what keeps the promise in SPEC.md,
//! section 9: for no fixture does a state exist in which the whole document is
//! laid out. The widget is always exactly the size of the viewport, and the
//! adjustment describes a document it has mostly never measured.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use hashline_markdown::OpDocument;

use crate::document::Anchor;
use crate::highlight::{self, Kind, Span};
use crate::layout::{set_block, BlockLayout, BlockPlan, Decoration, Metrics, Style};
use crate::outline::Outline;
use crate::search::{self, Hit, Query};
use crate::theme::{document as tokens, Palette, LIGHT};
use crate::view::images::ImageCache;
use crate::view::{Position, Selection};

/// Everything the view owns. The layout cache lives here and nowhere else
/// (SPEC.md, section 5, "Modulgrenzen").
pub(crate) struct State {
    document: Rc<OpDocument>,
    plan: BlockPlan,
    /// Set blocks, addressed by block index and bounded in size.
    cache: std::collections::HashMap<usize, BlockLayout>,
    /// Insertion order, for evicting the least recently set block.
    recent: std::collections::VecDeque<usize>,
    style: Style,
    palette: Palette,
    zoom: i32,
    selection: Option<Selection>,
    outline: Rc<Outline>,
    /// Character offsets for AT-SPI, built the first time a screen reader asks
    /// for them. Nothing else needs them, and on the 10 MiB fixture building
    /// them costs half a megabyte and a pass over the whole text.
    accessible: Option<super::accessibility::Text>,
    images: Rc<ImageCache>,
    /// Search hits over the whole document, in document order, and which of
    /// them is the current one.
    hits: Vec<Hit>,
    current_hit: Option<usize>,
    /// Syntax colours per code block, `None` while the worker is still
    /// running. Cached by block so scrolling back does not re-parse
    /// (SPEC.md, section 10), and bounded like the layout cache beside it: a
    /// long reading session through a document of 28 931 code blocks must not
    /// end up holding the spans of all of them.
    highlights: std::collections::HashMap<usize, Option<Vec<Span>>>,
    /// Insertion order of `highlights`, for evicting the oldest entry.
    coloured: std::collections::VecDeque<usize>,
    /// Called when a link is clicked. The view resolves nothing itself: what a
    /// relative path or a fragment means is the document controller's business.
    on_link: Option<LinkHandler>,
    /// The search hit the view is still working its way onto, and how many
    /// more passes it may take. See `settle_aim`.
    aim: Option<(usize, u8)>,
    /// Set while the widget itself is moving the adjustment, so that the
    /// resulting notification is not mistaken for the user scrolling.
    adjusting: bool,
    /// Whether a pass over the buffer around the viewport is already pending.
    buffering: bool,
    /// Faces of the current style that nothing has been set in yet, in the
    /// order a document is most likely to need them. Drained between the
    /// document arriving and the frame that shows it; see `schedule_warm`.
    warm: std::collections::VecDeque<crate::layout::Face>,
    /// Whether a warming slice is already pending.
    warming: bool,
}

/// The handler a clicked link is passed to.
type LinkHandler = Rc<dyn Fn(&str)>;

/// How many set blocks to keep. A block that is evicted keeps its measured
/// height in the plan, so eviction costs re-setting, never a jump.
const CACHE_LIMIT: usize = 240;

/// How long one idle pass over the viewport's buffer may take. Well inside the
/// 16 ms a frame has, so a frame that lands on top of one still fits.
const BUFFER_SLICE: f64 = 6.0;

/// How many frames a jump to a search hit may spend closing in on it. One pass
/// was enough for the jump measured on the 100 KiB fixture; the rest is
/// headroom for a plan whose estimates are further out than that, and a bound
/// so that a target it never agrees with ends rather than loops.
const AIM_PASSES: u8 = 8;

/// How many code blocks keep their syntax colours. Twice the layout cache,
/// because a span is 12 bytes where a set block is a Pango layout: scrolling
/// back a little further than the layouts reach then shows colour at once
/// instead of a frame of grey. It is still a bound, and a small one: nothing
/// over `highlight::MAX_CODE_BYTES` is coloured at all, and the plan keeps a
/// code block's parts well under that. Evicting an entry costs re-parsing that
/// block, never a jump.
const HIGHLIGHT_LIMIT: usize = 480;

impl State {
    fn empty() -> Self {
        let document = Rc::new(OpDocument::default());
        let metrics = Metrics {
            char_width: 8.0,
            body_px: tokens::BODY_PX,
        };
        State {
            plan: BlockPlan::new(&document, metrics, 640.0),
            document,
            cache: std::collections::HashMap::new(),
            recent: std::collections::VecDeque::new(),
            style: Style::new("sans", "monospace", tokens::BODY_PX, LIGHT),
            palette: LIGHT,
            zoom: 100,
            selection: None,
            outline: Rc::new(Outline::default()),
            accessible: None,
            images: Rc::new(ImageCache::default()),
            hits: Vec::new(),
            current_hit: None,
            highlights: std::collections::HashMap::new(),
            coloured: std::collections::VecDeque::new(),
            on_link: None,
            aim: None,
            adjusting: false,
            buffering: false,
            warm: std::collections::VecDeque::new(),
            warming: false,
        }
    }
    fn body_px(&self) -> f64 {
        tokens::BODY_PX * self.zoom as f64 / 100.0
    }
    /// The accessible text and the two things every query on it needs, built
    /// on first use. They come out together because they all borrow the state.
    fn accessible(&mut self) -> (&super::accessibility::Text, &BlockPlan, &str) {
        if self.accessible.is_none() {
            self.accessible = Some(super::accessibility::Text::build(
                &self.plan,
                &self.document.text,
            ));
        }
        (
            self.accessible.as_ref().expect("just built"),
            &self.plan,
            &self.document.text,
        )
    }
    /// How long the accessible text is, without building it: a client that has
    /// never asked is not listening for a change either.
    fn accessible_len(&self) -> u32 {
        self.accessible.as_ref().map_or(0, |text| text.len)
    }
    fn remember(&mut self, index: usize, set: BlockLayout) {
        if self.cache.insert(index, set).is_none() {
            self.recent.push_back(index);
        }
        while self.recent.len() > CACHE_LIMIT {
            if let Some(oldest) = self.recent.pop_front() {
                self.cache.remove(&oldest);
            }
        }
    }
    /// Records that a block's colours were asked for, and later what came
    /// back. An entry that is evicted while its worker is still running is
    /// simply not put back: the block asks again when it is next drawn.
    fn remember_highlight(&mut self, index: usize, spans: Option<Vec<Span>>) {
        if self.highlights.insert(index, spans).is_none() {
            self.coloured.push_back(index);
        }
        while self.coloured.len() > HIGHLIGHT_LIMIT {
            if let Some(oldest) = self.coloured.pop_front() {
                self.highlights.remove(&oldest);
            }
        }
    }
}

mod imp {
    use super::*;

    #[derive(glib::Properties)]
    #[properties(wrapper_type = super::DocumentView)]
    pub struct DocumentView {
        #[property(get, set = Self::set_vadjustment, override_interface = gtk::Scrollable)]
        pub vadjustment: RefCell<Option<gtk::Adjustment>>,
        #[property(get, set, override_interface = gtk::Scrollable)]
        pub hadjustment: RefCell<Option<gtk::Adjustment>>,
        #[property(get, set, override_interface = gtk::Scrollable, builder(gtk::ScrollablePolicy::Minimum))]
        pub vscroll_policy: RefCell<gtk::ScrollablePolicy>,
        #[property(get, set, override_interface = gtk::Scrollable, builder(gtk::ScrollablePolicy::Minimum))]
        pub hscroll_policy: RefCell<gtk::ScrollablePolicy>,
        pub(crate) state: RefCell<State>,
        pub navigation: std::cell::Cell<Option<(i32, Option<usize>)>>,
    }

    impl Default for DocumentView {
        fn default() -> Self {
            DocumentView {
                vadjustment: RefCell::new(None),
                hadjustment: RefCell::new(None),
                vscroll_policy: RefCell::new(gtk::ScrollablePolicy::Minimum),
                hscroll_policy: RefCell::new(gtk::ScrollablePolicy::Minimum),
                state: RefCell::new(State::empty()),
                navigation: std::cell::Cell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DocumentView {
        const NAME: &'static str = "HashlineDocumentView";
        type Type = super::DocumentView;
        type ParentType = gtk::Widget;
        type Interfaces = (gtk::Scrollable, gtk::AccessibleText);

        fn class_init(klass: &mut Self::Class) {
            klass.set_accessible_role(gtk::AccessibleRole::Document);
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for DocumentView {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
                std::sync::OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![glib::subclass::Signal::builder("active-section-changed").build()]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            let widget = self.obj();
            widget.set_focusable(true);
            widget.set_can_focus(true);
            // A block that is half scrolled off the top is drawn from a
            // negative y, and a widget may draw outside its allocation unless
            // it says otherwise — so the top of a code block appeared over the
            // search bar and the header. Nothing of the document belongs
            // outside the viewport.
            widget.set_overflow(gtk::Overflow::Hidden);
            widget.setup_gestures();
            widget.update_property(&[
                gtk::accessible::Property::Label("Markdown-Dokument"),
                gtk::accessible::Property::ReadOnly(true),
            ]);
        }
    }

    impl DocumentView {
        fn set_vadjustment(&self, adjustment: Option<gtk::Adjustment>) {
            let widget = self.obj();
            if let Some(adjustment) = adjustment.as_ref() {
                adjustment.connect_value_changed(glib::clone!(
                    #[weak]
                    widget,
                    move |_| {
                        if widget.imp().state.try_borrow().is_ok_and(|s| !s.adjusting) {
                            // A reader who scrolls during a search jump has
                            // said where they want to be, and outranks it.
                            if let Ok(mut state) = widget.imp().state.try_borrow_mut() {
                                state.aim = None;
                            }
                            widget.queue_draw();
                            widget.emit_by_name::<()>("active-section-changed", &[]);
                        }
                    }
                ));
            }
            self.vadjustment.replace(adjustment);
            widget.queue_allocate();
        }
    }

    impl WidgetImpl for DocumentView {
        fn realize(&self) {
            self.parent_realize();
            // Only now is the style settled: the family comes from GTK's
            // settings and the context's resolution from the display. This
            // fills the list of faces to load; nothing is loaded until there
            // is a document to show.
            self.obj().rebuild_style();
        }

        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            // The widget is the viewport, never the document: it asks for a
            // readable minimum and takes whatever it is given.
            match orientation {
                gtk::Orientation::Horizontal => (240, 720, -1, -1),
                _ => (120, 480, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let widget = self.obj();
            crate::view::mainthread::timed("size-allocate", || {
                widget.reflow_for(width as f64);
                widget.update_adjustment(width as f64, height as f64);
                widget.notify_navigation();
            });
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            crate::view::mainthread::timed("snapshot", || self.obj().draw(snapshot));
        }
    }

    impl ScrollableImpl for DocumentView {}

    impl AccessibleTextImpl for DocumentView {
        fn contents(&self, start: u32, end: u32) -> Option<glib::Bytes> {
            let mut state = self.state.borrow_mut();
            let (accessible, plan, source) = state.accessible();
            let text = accessible.slice(plan, source, start, end);
            Some(glib::Bytes::from_owned(text.into_bytes()))
        }
        fn contents_at(
            &self,
            offset: u32,
            granularity: gtk::AccessibleTextGranularity,
        ) -> Option<(u32, u32, glib::Bytes)> {
            let line = (granularity == gtk::AccessibleTextGranularity::Line)
                .then(|| self.obj().accessible_line(offset))
                .flatten();
            let mut state = self.state.borrow_mut();
            let (accessible, plan, source) = state.accessible();
            let (start, end) = line.unwrap_or_else(|| {
                super::super::accessibility::span_at(accessible, plan, source, offset, granularity)
            });
            let text = accessible.slice(plan, source, start, end);
            Some((start, end, glib::Bytes::from_owned(text.into_bytes())))
        }
        fn caret_position(&self) -> u32 {
            let mut state = self.state.borrow_mut();
            let Some(cursor) = state.selection.map(|s| s.cursor) else {
                return 0;
            };
            let (accessible, plan, source) = state.accessible();
            accessible.offset(cursor, plan, source)
        }
        fn selection(&self) -> Vec<gtk::AccessibleTextRange> {
            let mut state = self.state.borrow_mut();
            let Some(selection) = state.selection.filter(|s| !s.is_empty()) else {
                return vec![];
            };
            let (start, end) = selection.range();
            let (accessible, plan, source) = state.accessible();
            let start = accessible.offset(start, plan, source);
            let end = accessible.offset(end, plan, source);
            vec![gtk::AccessibleTextRange::new(
                start as usize,
                (end - start) as usize,
            )]
        }
        fn attributes(
            &self,
            _offset: u32,
        ) -> Vec<(gtk::AccessibleTextRange, glib::GString, glib::GString)> {
            vec![]
        }
        fn default_attributes(&self) -> Vec<(glib::GString, glib::GString)> {
            vec![]
        }
    }
}

glib::wrapper! {
    pub struct DocumentView(ObjectSubclass<imp::DocumentView>)
        @extends gtk::Widget,
        @implements gtk::Scrollable, gtk::AccessibleText, gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for DocumentView {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentView {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    fn view_width(&self) -> f64 {
        gtk::prelude::WidgetExt::width(self) as f64
    }
    fn view_height(&self) -> f64 {
        gtk::prelude::WidgetExt::height(self) as f64
    }

    /// Shows a parsed document, from the top.
    ///
    /// The document is taken by value so that the parts of it the plan has
    /// copied out can be released before it is shared: the block table is
    /// exactly what `BlockPlan` holds, and keeping both costs 2.8 MiB on the
    /// 10 MiB fixture (docs/decisions/014-competitive-targets.md, section 3.2).
    pub fn set_document(&self, document: OpDocument) {
        super::mainthread::timed("set-document", || self.take_document(document));
    }

    fn take_document(&self, mut document: OpDocument) {
        let old_len = self.imp().state.borrow().accessible_len();
        if old_len > 0 {
            self.update_contents(gtk::AccessibleTextContentChange::Remove, 0, old_len);
        }
        let width = self.view_width().max(1.0);
        let column = self.column_width(width);
        let char_width = self.char_width();
        let body_px = self.imp().state.borrow().body_px();
        let plan = BlockPlan::new(
            &document,
            Metrics {
                char_width,
                body_px,
            },
            column,
        );
        let entries = Outline::entries_of(&document, &plan);
        // Both tables have been copied out now: the plan holds the blocks and
        // the outline holds the headings it needs.
        document.blocks = Vec::new();
        document.headings = Vec::new();
        let document = Rc::new(document);
        let outline = Rc::new(Outline::new(document.clone(), entries));
        {
            let mut state = self.imp().state.borrow_mut();
            state.accessible = None;
            state.document = document;
            state.plan = plan;
            state.outline = outline;
            state.cache.clear();
            state.recent.clear();
            state.selection = None;
            state.hits.clear();
            state.current_hit = None;
            state.highlights.clear();
            state.coloured.clear();
        }
        // Rebuilt only for a client that had already asked for the old text.
        let len = if old_len > 0 {
            self.imp().state.borrow_mut().accessible().0.len
        } else {
            0
        };
        if len > 0 {
            self.update_contents(gtk::AccessibleTextContentChange::Insert, 0, len);
        }
        self.selection_changed();
        if let Some(adjustment) = self.vadjustment() {
            adjustment.set_value(0.0);
        }
        // There is something to show now, so the faces it will be shown in
        // are worth loading before the frame that shows it.
        self.schedule_warm();
        self.queue_draw();
    }

    /// Drops everything that can be built again from the document and the
    /// plan: the set blocks and their syntax colours.
    ///
    /// A tab that is not showing must not hold a layout cache
    /// (docs/decisions/014-competitive-targets.md, section 2.2). What stays is
    /// the document, the plan with its measured heights and the reading
    /// position, which is what makes coming back a few milliseconds rather
    /// than a reload.
    pub fn release_layout_cache(&self) {
        let mut state = self.imp().state.borrow_mut();
        state.cache.clear();
        state.recent.clear();
        state.highlights.clear();
        state.coloured.clear();
        state.images.clear();
    }

    /// Whether any block of this document is currently set. What a tab that
    /// is not showing must not answer with yes.
    pub fn holds_layouts(&self) -> bool {
        !self.imp().state.borrow().cache.is_empty()
    }

    /// The directory relative picture paths resolve against. Setting it clears
    /// what was cached for the previous document.
    pub fn set_base_directory(&self, directory: Option<std::path::PathBuf>) {
        let images = self.imp().state.borrow().images.clone();
        images.set_base(directory);
        let mut state = self.imp().state.borrow_mut();
        state.cache.clear();
        state.recent.clear();
    }

    pub fn set_palette(&self, palette: Palette) {
        self.imp().state.borrow_mut().palette = palette;
        self.rebuild_style();
        self.queue_draw();
    }

    pub fn zoom(&self) -> i32 {
        self.imp().state.borrow().zoom
    }

    pub fn set_zoom(&self, percent: i32) {
        {
            let mut state = self.imp().state.borrow_mut();
            let zoom = tokens::clamp_zoom(percent);
            if zoom == state.zoom {
                return;
            }
            state.zoom = zoom;
        }
        self.rebuild_style();
        self.reflow_for(self.view_width().max(1.0));
        self.queue_draw();
    }

    fn rebuild_style(&self) {
        {
            let mut state = self.imp().state.borrow_mut();
            let (body, mono) = font_families();
            let body_px = state.body_px();
            state.style = Style::new(&body, &mono, body_px, state.palette);
            state.cache.clear();
            state.recent.clear();
            // A new family or a new zoom means new faces at new sizes, and
            // every cached block has just been dropped, so the whole screen is
            // about to be set again.
            state.warm = crate::layout::faces(&state.style).into();
        }
        self.schedule_warm();
    }

    /// The reading column: 76 characters of the body font, centred, never a
    /// fixed pixel count (SPEC.md, section 3).
    fn column_width(&self, width: f64) -> f64 {
        let state = self.imp().state.borrow();
        let context = self.pango_context();
        let metrics = context.metrics(Some(&state.style.body), None);
        let char_width = metrics.approximate_char_width() as f64 / pango::SCALE as f64;
        let column = char_width * tokens::COLUMN_CHARS;
        let available = (width - 2.0 * tokens::PAD_SIDE).max(120.0);
        column.min(available)
    }

    fn char_width(&self) -> f64 {
        let state = self.imp().state.borrow();
        let metrics = self.pango_context().metrics(Some(&state.style.body), None);
        (metrics.approximate_char_width() as f64 / pango::SCALE as f64).max(1.0)
    }

    /// Estimates every block again for a new width, zoom or font. The cuts a
    /// block was made of do not depend on either, so the plan is re-estimated
    /// in place rather than rebuilt — which is also what lets the document's
    /// own block table be released once the plan exists.
    fn reflow_for(&self, width: f64) {
        let column = self.column_width(width);
        let char_width = self.char_width();
        let mut state = self.imp().state.borrow_mut();
        let body_px = state.body_px();
        if (state.plan.width() - column).abs() < 0.5 && !state.cache.is_empty() {
            return;
        }
        let metrics = Metrics {
            char_width,
            body_px,
        };
        state.plan.reflow(metrics, column);
        state.cache.clear();
        state.recent.clear();
    }

    fn update_adjustment(&self, _width: f64, height: f64) {
        let total = self.imp().state.borrow().plan.total_height();
        if let Some(adjustment) = self.vadjustment() {
            let mut state = self.imp().state.borrow_mut();
            state.adjusting = true;
            adjustment.configure(
                adjustment.value().min((total - height).max(0.0)),
                0.0,
                total.max(height),
                height * 0.1,
                height * 0.9,
                height,
            );
            state.adjusting = false;
        }
    }

    fn scroll_top(&self) -> f64 {
        self.vadjustment().map(|a| a.value()).unwrap_or(0.0)
    }

    /// Lays out every block the viewport needs, then folds the measurements
    /// into the plan. Measuring and drawing stay separate passes
    /// (SPEC.md, section 10).
    ///
    /// A block above the viewport whose measurement differs from its estimate
    /// moves everything below it; the scroll offset moves with it so the text
    /// on screen stands still.
    /// Sets what this frame needs, and leaves the rest for an idle moment.
    ///
    /// The plan lays out a screen of buffer above and below the viewport so
    /// that the next scroll step has nothing to do. Setting all three screens
    /// in the frame that jumped there cost 22 ms on the 10 MiB fixture against
    /// a 16 ms budget, so only the screen that is actually shown is set here
    /// (docs/decisions/014-competitive-targets.md, section 3.3).
    ///
    /// One screen still cost 21 ms at startup, and almost none of it was the
    /// document: it was the faces the screen's first block in each type style
    /// had to instantiate. `schedule_warm` has them loaded by the time this
    /// runs, which leaves 6 ms.
    fn measure_visible(&self, height: f64) {
        let top = self.scroll_top();
        let onscreen = {
            let state = self.imp().state.borrow();
            state.plan.onscreen_range(top, height)
        };
        super::mainthread::timed("measure-visible", || self.lay_out(onscreen, height, None));
        self.schedule_buffer(height);
    }

    /// Fills the buffer around the viewport in idle slices, so that no single
    /// piece of it is long enough to hold up a frame. The range is worked out
    /// when the slice runs, not when it was asked for: by then the reader may
    /// be somewhere else, and the buffer that matters is the one around where
    /// they are now.
    fn schedule_buffer(&self, height: f64) {
        if self.imp().state.borrow().buffering {
            return;
        }
        self.imp().state.borrow_mut().buffering = true;
        glib::idle_add_local_once(glib::clone!(
            #[weak(rename_to = view)]
            self,
            move || {
                view.imp().state.borrow_mut().buffering = false;
                let range = {
                    let state = view.imp().state.borrow();
                    state.plan.visible_range(view.scroll_top(), height)
                };
                let done = super::mainthread::timed("buffer", || {
                    view.lay_out(range, height, Some(BUFFER_SLICE))
                });
                if !done {
                    view.schedule_buffer(height);
                }
            }
        ));
    }

    /// Loads the faces of the current style, one per slice, in the gap between
    /// a document arriving and the frame that shows it.
    ///
    /// Setting the first block in a face costs the fontconfig match, the font
    /// file and the scaled font at that size, and on the startup screen of
    /// every fixture that came to 21 ms in one task: five faces, and the same
    /// five whether the document is 100 KiB or 10 MiB, because a screen holds
    /// about the same handful of type styles either way. Once the faces exist,
    /// setting a block costs 0.03 ms
    /// (docs/decisions/014-competitive-targets.md, section 3.3).
    ///
    /// The gap is real time, not a rearrangement of the metric: a parsed
    /// document reaches the main thread ten to eighty milliseconds before the
    /// compositor asks for the frame that shows it, and until now the reader
    /// spent that gap idle and then did all the font work inside the frame.
    /// The frame that shows the first text arrives sooner for it, which is the
    /// only reason moving the work is worth anything. The longest task of a
    /// startup fell from 22 to 7 ms for it, and the whole screen is still set
    /// in one pass.
    ///
    /// The chain stops when the list is empty rather than rescheduling, so a
    /// reader at rest has no idle source of its own (SPEC.md, section 10).
    fn schedule_warm(&self) {
        {
            let state = self.imp().state.borrow();
            if state.warming || state.warm.is_empty() {
                return;
            }
            // Not before the widget is realized: a face matched against a
            // context whose resolution is not settled would have to be
            // matched again. And not before there is a document, which is the
            // later of the two and the one that matters — GTK loads the font
            // configuration on a thread of its own at startup, and a face
            // asked for while that is still running costs up to 14 ms of
            // waiting for that thread rather than any work of the reader's.
            // By the time a parsed document arrives it has long finished.
            if !self.is_realized() || state.plan.is_empty() {
                return;
            }
        }
        self.imp().state.borrow_mut().warming = true;
        // Above the frame clock, which is the whole point: an idle at the
        // default priority is not dispatched at all while a frame is pending,
        // and a face loaded after the frame that needed it has saved nothing.
        // Still below the default priority, so a parsed document, an input
        // event or a Wayland message is never held up by more than the one
        // face being loaded.
        glib::idle_add_local_full(
            glib::Priority::HIGH_IDLE,
            glib::clone!(
                #[weak(rename_to = view)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    view.imp().state.borrow_mut().warming = false;
                    super::mainthread::timed("warm-type", || view.warm_type());
                    view.schedule_warm();
                    glib::ControlFlow::Break
                }
            ),
        );
    }

    /// Loads the next face.
    fn warm_type(&self) {
        let Some(face) = self.imp().state.borrow_mut().warm.pop_front() else {
            return;
        };
        crate::layout::load_face(&self.pango_context(), &face);
    }

    /// Sets every block of `range` that is not set yet, stopping after `budget`
    /// if one is given. Returns whether it finished the range.
    fn lay_out(&self, range: std::ops::Range<usize>, height: f64, budget: Option<f64>) -> bool {
        let started = std::time::Instant::now();
        let top = self.scroll_top();
        let context = self.pango_context();
        let mut shift = 0.0;
        let mut finished = true;
        for index in range {
            if let Some(budget) = budget {
                if started.elapsed().as_secs_f64() * 1000.0 >= budget {
                    finished = false;
                    break;
                }
            }
            let (needs, block, width) = {
                let state = self.imp().state.borrow();
                if index >= state.plan.len() {
                    break;
                }
                (
                    !state.cache.contains_key(&index),
                    *state.plan.block(index),
                    state.plan.width(),
                )
            };
            if !needs {
                continue;
            }
            // Cloned out of the borrow: setting a block runs Pango, and the
            // state must not stay borrowed across it.
            let (document, style) = {
                let state = self.imp().state.borrow();
                (state.document.clone(), state.style.clone())
            };
            let images = self.imp().state.borrow().images.clone();
            let set = set_block(&context, &document, &block, &style, width, images.as_ref());
            if block.kind == crate::layout::BlockKind::Code {
                self.colour_code(index, &block, &document, &set);
            }
            let measured = set.height();
            let mut state = self.imp().state.borrow_mut();
            let was_above = state.plan.y_of(index) + block.height as f64 <= top;
            let delta = state.plan.set_measured(index, measured);
            if was_above {
                shift += delta;
            }
            state.remember(index, set);
        }
        if shift.abs() > f64::EPSILON {
            if let Some(adjustment) = self.vadjustment() {
                let mut state = self.imp().state.borrow_mut();
                state.adjusting = true;
                let total = state.plan.total_height();
                adjustment.set_upper(total.max(height));
                adjustment.set_value(adjustment.value() + shift);
                state.adjusting = false;
            }
            self.queue_draw();
        }
        finished
    }

    /// Applies the colours a code block already has, or asks for them.
    ///
    /// The request runs on a worker thread, and the answer is applied to the
    /// layout that is already cached rather than re-setting the block: the
    /// attributes change, the type does not.
    fn colour_code(
        &self,
        index: usize,
        block: &crate::layout::Block,
        document: &Rc<OpDocument>,
        set: &BlockLayout,
    ) {
        let palette = self.imp().state.borrow().palette;
        match self.imp().state.borrow().highlights.get(&index) {
            Some(Some(spans)) => {
                apply_highlight(set, spans, palette);
                return;
            }
            // Asked for and still running.
            Some(None) => return,
            None => {}
        }
        let language = crate::layout::code_language(document, block);
        if !highlight::is_supported(&language) {
            return;
        }
        let Some((from, to)) = set.content_range() else {
            return;
        };
        if (to - from) as usize > highlight::MAX_CODE_BYTES {
            return;
        }
        self.imp()
            .state
            .borrow_mut()
            .remember_highlight(index, None);
        // A code block the plan cut into parts is coloured part by part, which
        // is the whole point: colouring it as one would cost what setting it
        // as one costs. A string or comment running across a cut is coloured
        // as if it began there.
        let code = document.text[from as usize..to as usize].to_string();
        let (sender, receiver) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = sender.send_blocking(highlight::spans(&language, &code, from));
        });
        let widget = self.clone();
        glib::spawn_future_local(async move {
            let Ok(spans) = receiver.recv().await else {
                return;
            };
            let palette = widget.imp().state.borrow().palette;
            {
                let mut state = widget.imp().state.borrow_mut();
                // Evicted while the worker ran, or the document was replaced:
                // the answer belongs to a question nobody is asking any more.
                if !state.highlights.contains_key(&index) {
                    return;
                }
                state.highlights.insert(index, Some(spans));
            }
            let state = widget.imp().state.borrow();
            if let (Some(set), Some(Some(spans))) =
                (state.cache.get(&index), state.highlights.get(&index))
            {
                apply_highlight(set, spans, palette);
            }
            drop(state);
            widget.queue_draw();
        });
    }

    fn draw(&self, snapshot: &gtk::Snapshot) {
        let width = self.view_width();
        let height = self.view_height();
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        self.measure_visible(height);
        // Setting the viewport is what makes the plan's answer for where the
        // hit sits better than the estimate the jump used, so the aim is worth
        // repeating here and nowhere else.
        if self.settle_aim() {
            self.measure_visible(height);
        }
        self.update_adjustment(width, height);

        let state = self.imp().state.borrow();
        let palette = state.palette;
        snapshot.append_color(
            &palette.bg.to_gdk(),
            &gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32),
        );
        if state.plan.is_empty() {
            return;
        }

        let top = self.scroll_top();
        let column = state.plan.width();
        let left = ((width - column) / 2.0).max(tokens::PAD_SIDE);
        let range = state.plan.visible_range(top, height);

        for index in range {
            if index >= state.plan.len() {
                break;
            }
            let Some(set) = state.cache.get(&index) else {
                continue;
            };
            let block_top = state.plan.y_of(index) - top + set.baseline_offset();
            if block_top > height || block_top + set.content_height < 0.0 {
                continue;
            }
            let block = state.plan.block(index);
            // Content wider than the column — a code block, a wide table —
            // scrolls inside its own block, so it is clipped to the column
            // instead of spilling into the margins (SPEC.md, section 3).
            let clipped = set.content_width > column + 0.5;
            if clipped {
                snapshot.push_clip(&gtk::graphene::Rect::new(
                    left as f32,
                    block_top as f32,
                    column as f32,
                    set.content_height as f32,
                ));
            }

            for decoration in &set.decorations {
                match decoration {
                    Decoration::Fill {
                        x,
                        y,
                        width: w,
                        height: h,
                        radius,
                        color,
                    } => {
                        let rect = gtk::graphene::Rect::new(
                            (left + x) as f32,
                            (block_top + y) as f32,
                            *w as f32,
                            *h as f32,
                        );
                        if radius.iter().any(|corner| *corner > 0.0) {
                            let size = |corner: f32| gtk::graphene::Size::new(corner, corner);
                            let rounded = gtk::gsk::RoundedRect::new(
                                rect,
                                size(radius[0]),
                                size(radius[1]),
                                size(radius[2]),
                                size(radius[3]),
                            );
                            snapshot.push_rounded_clip(&rounded);
                            snapshot.append_color(&color.to_gdk(), &rect);
                            snapshot.pop();
                        } else {
                            snapshot.append_color(&color.to_gdk(), &rect);
                        }
                    }
                    Decoration::Image {
                        x,
                        y,
                        width: w,
                        height: h,
                        source,
                    } => {
                        if let Some(texture) = state.images.texture(source) {
                            snapshot.append_texture(
                                &texture,
                                &gtk::graphene::Rect::new(
                                    (left + x) as f32,
                                    (block_top + y) as f32,
                                    *w as f32,
                                    *h as f32,
                                ),
                            );
                        }
                    }
                    Decoration::Line {
                        x,
                        y,
                        width: w,
                        height: h,
                        color,
                    } => snapshot.append_color(
                        &color.to_gdk(),
                        &gtk::graphene::Rect::new(
                            (left + x) as f32,
                            (block_top + y) as f32,
                            *w as f32,
                            *h as f32,
                        ),
                    ),
                }
            }

            for piece in &set.pieces {
                let x = left + piece.x;
                let y = block_top + piece.y;
                // The selection wash goes under the type, per line, so a
                // selection across wrapped lines reads as one shape.
                // Search hits sit under the selection wash: a hit that is also
                // selected should still read as selected.
                for (position, hit) in state.hits.iter().enumerate() {
                    if let Some((lo, hi)) = piece.map.clip(hit.start, hit.end) {
                        let current = state.current_hit == Some(position);
                        let colour = if current {
                            palette.accent.with_alpha(0.45)
                        } else {
                            palette.accent.with_alpha(0.18)
                        };
                        draw_ranges(snapshot, piece, x, y, lo, hi, colour);
                    }
                }
                if let Some(selection) = state.selection {
                    if let Some((from, to)) = selection.in_block(index, block.text_len) {
                        let base = block.text_start;
                        if let Some((lo, hi)) = piece.map.clip(base + from, base + to) {
                            draw_ranges(snapshot, piece, x, y, lo, hi, palette.selection());
                        }
                    }
                }
                snapshot.save();
                snapshot.translate(&gtk::graphene::Point::new(x as f32, y as f32));
                snapshot.append_layout(&piece.layout, &piece.color.to_gdk());
                snapshot.restore();
            }

            if clipped {
                snapshot.pop();
            }
        }
        drop(state);
        self.notify_navigation();
    }

    /// The document position under a point in widget coordinates.
    fn position_at(&self, x: f64, y: f64) -> Option<Position> {
        let height = self.view_height();
        self.measure_visible(height);
        let state = self.imp().state.borrow();
        if state.plan.is_empty() {
            return None;
        }
        let top = self.scroll_top();
        let width = self.view_width();
        let column = state.plan.width();
        let left = ((width - column) / 2.0).max(tokens::PAD_SIDE);
        let index = state.plan.block_at(top + y);
        let set = state.cache.get(&index)?;
        let block_top = state.plan.y_of(index) - top + set.baseline_offset();
        let local_x = x - left;
        let local_y = y - block_top;

        // The nearest piece, preferring the right row over the right column:
        // a click in the gutter beside a list item belongs to that item, not
        // to whatever happens to be horizontally closest.
        let mut best: Option<(f64, &crate::layout::Piece)> = None;
        for piece in set.pieces.iter().filter(|piece| !piece.control) {
            let (_, logical) = piece.layout.pixel_extents();
            let x0 = piece.x;
            let y0 = piece.y;
            let x1 = x0 + logical.width() as f64;
            let y1 = y0 + logical.height() as f64;
            let dx = (x0 - local_x).max(local_x - x1).max(0.0);
            let dy = (y0 - local_y).max(local_y - y1).max(0.0);
            let distance = dy * 4096.0 + dx;
            if best.is_none_or(|(previous, _)| distance < previous) {
                best = Some((distance, piece));
            }
        }
        let piece = best?.1;
        let layout_x = ((local_x - piece.x) * pango::SCALE as f64) as i32;
        let layout_y = ((local_y - piece.y) * pango::SCALE as f64) as i32;
        let (_, offset, trailing) = piece.layout.xy_to_index(layout_x, layout_y);
        let text = piece.layout.text();
        let mut offset = offset as usize;
        // `trailing` counts characters past the reported index, which is how
        // Pango says "the caret belongs after this glyph".
        for _ in 0..trailing {
            offset = next_boundary(text.as_str(), offset);
        }
        let in_document = piece.map.to_document(offset as u32)?;
        let start = state.plan.block(index).text_start;
        Some(Position::new(index, in_document.saturating_sub(start)))
    }

    /// Set only the requested block for offscreen screen-reader queries.
    fn accessible_line(&self, offset: u32) -> Option<(u32, u32)> {
        let mut state = self.imp().state.borrow_mut();
        let position = {
            let (accessible, plan, source) = state.accessible();
            accessible.position(offset, plan, source)?
        };
        let state = &*state;
        let block = state.plan.block(position.block);
        let temporary;
        let set = if let Some(set) = state.cache.get(&position.block) {
            set
        } else {
            temporary = set_block(
                &self.pango_context(),
                &state.document,
                block,
                &state.style,
                state.plan.width(),
                state.images.as_ref(),
            );
            &temporary
        };
        let document_offset = position.in_document(&state.plan);
        for piece in set.pieces.iter().filter(|p| !p.control) {
            let Some((start, end)) = piece.map.document_range() else {
                continue;
            };
            if document_offset < start || document_offset > end {
                continue;
            }
            let layout_offset = piece.map.to_layout(document_offset)?;
            for line in piece.layout.lines_readonly() {
                let start = line.start_index() as u32;
                let end = start + line.length() as u32;
                if layout_offset >= start
                    && (layout_offset < end || document_offset == block.text_start + block.text_len)
                {
                    let convert = |byte| {
                        let byte = piece
                            .map
                            .to_document(byte)?
                            .saturating_sub(block.text_start);
                        Some(state.accessible.as_ref()?.offset(
                            Position::new(position.block, byte),
                            &state.plan,
                            &state.document.text,
                        ))
                    };
                    return Some((convert(start)?, convert(end)?));
                }
            }
        }
        None
    }

    fn selection_changed(&self) {
        self.update_caret_position();
        self.update_selection_bound();
        self.queue_draw();
    }

    fn set_selection_at(&self, position: Position, clicks: i32, extend: bool) {
        let mut state = self.imp().state.borrow_mut();
        let selection = if clicks >= 3 {
            // The whole block, which for a block the plan cut up means all of
            // its parts: a reader who selects a paragraph means the paragraph.
            let parts = state.plan.source_blocks(position.block);
            let last = parts.end - 1;
            Selection::at(Position::new(parts.start, 0))
                .to(Position::new(last, state.plan.block(last).text_len))
        } else if clicks == 2 {
            let block = state.plan.block(position.block);
            let text = &state.document.text
                [block.text_start as usize..(block.text_start + block.text_len) as usize];
            let offset = text[..position.offset as usize].chars().count() as u32;
            let (from, to) =
                super::accessibility::span(text, offset, gtk::AccessibleTextGranularity::Word);
            let to = from
                + text
                    .chars()
                    .skip(from as usize)
                    .take((to - from) as usize)
                    .collect::<String>()
                    .trim_end()
                    .chars()
                    .count() as u32;
            let byte = |offset| {
                text.char_indices()
                    .nth(offset as usize)
                    .map(|(b, _)| b)
                    .unwrap_or(text.len()) as u32
            };
            Selection::at(Position::new(position.block, byte(from)))
                .to(Position::new(position.block, byte(to)))
        } else if extend {
            state
                .selection
                .unwrap_or(Selection::at(position))
                .to(position)
        } else {
            Selection::at(position)
        };
        state.selection = Some(selection);
        drop(state);
        self.selection_changed();
    }

    fn setup_gestures(&self) {
        let click = gtk::GestureClick::new();
        click.set_button(gtk::gdk::BUTTON_PRIMARY);
        click.connect_pressed(glib::clone!(
            #[weak(rename_to = widget)]
            self,
            move |gesture, clicks, x, y| {
                widget.grab_focus();
                if let Some(position) = widget.position_at(x, y) {
                    widget.set_selection_at(
                        position,
                        clicks,
                        gesture
                            .current_event_state()
                            .contains(gtk::gdk::ModifierType::SHIFT_MASK),
                    );
                }
            }
        ));
        click.connect_released(glib::clone!(
            #[weak(rename_to = widget)]
            self,
            move |_, clicks, x, y| {
                let empty = widget
                    .imp()
                    .state
                    .borrow()
                    .selection
                    .is_none_or(|s| s.is_empty());
                if clicks != 1 || !empty {
                    return;
                }
                if widget.copy_code_at(x, y) {
                    return;
                }
                if let Some(href) = widget.link_at(x, y) {
                    let handler = widget.imp().state.borrow().on_link.clone();
                    if let Some(handler) = handler {
                        handler(&href);
                    }
                }
            }
        ));
        self.add_controller(click.clone());
        let drag = gtk::GestureDrag::new();
        drag.set_button(gtk::gdk::BUTTON_PRIMARY);
        self.add_controller(drag.clone());
        // Both gestures must observe the same sequence; claiming a drag must
        // not cancel the click that established its anchor.
        drag.group_with(&click);
        drag.connect_drag_update(glib::clone!(
            #[weak(rename_to = widget)]
            self,
            move |gesture, dx, dy| {
                let Some((x, y)) = gesture.start_point() else {
                    return;
                };
                if dx.abs() + dy.abs() < 3.0 {
                    return;
                }
                gesture.set_state(gtk::EventSequenceState::Claimed);
                if let Some(cursor) = widget.position_at(x + dx, y + dy) {
                    let mut state = widget.imp().state.borrow_mut();
                    if let Some(selection) = state.selection {
                        state.selection = Some(selection.to(cursor));
                    }
                    drop(state);
                    widget.selection_changed();
                }
            }
        ));
        drag.connect_drag_end(glib::clone!(
            #[weak(rename_to = widget)]
            self,
            move |_, _, _| {
                widget.selection_changed();
            }
        ));

        // The pointer says what is clickable, which is the only affordance a
        // link has in a document that renders no controls of its own.
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(glib::clone!(
            #[weak(rename_to = widget)]
            self,
            move |_, x, y| {
                let over_link = widget.link_at(x, y).is_some();
                widget.set_cursor_from_name(Some(if over_link { "pointer" } else { "text" }));
            }
        ));
        self.add_controller(motion);
    }

    fn notify_navigation(&self) {
        let current = (self.width(), self.active_section());
        if self.imp().navigation.replace(Some(current)) != Some(current) {
            glib::idle_add_local_once(glib::clone!(
                #[weak(rename_to = view)]
                self,
                move || {
                    view.emit_by_name::<()>("active-section-changed", &[]);
                }
            ));
        }
    }

    pub fn active_section(&self) -> Option<usize> {
        let state = self.imp().state.borrow();
        if state.plan.is_empty() {
            return None;
        }
        state
            .outline
            .active_for_block(state.plan.block_at(self.scroll_top() + tokens::PAD_TOP))
    }

    pub fn outline(&self) -> Rc<Outline> {
        self.imp().state.borrow().outline.clone()
    }

    /// Registers the handler for a clicked link.
    pub fn connect_link_activated(&self, handler: impl Fn(&str) + 'static) {
        self.imp().state.borrow_mut().on_link = Some(Rc::new(handler));
    }

    /// Jumps to a heading by its generated id, for a `#fragment` link.
    pub fn scroll_to_anchor(&self, id: &str) -> bool {
        let target = self.imp().state.borrow().outline.block_for_id(id);
        match target {
            Some(block) => {
                self.scroll_to_block(block);
                true
            }
            None => false,
        }
    }

    /// The link under a point, if there is one.
    fn link_at(&self, x: f64, y: f64) -> Option<String> {
        let position = self.position_at(x, y)?;
        let state = self.imp().state.borrow();
        let block = state.plan.block(position.block);
        let set = state.cache.get(&position.block)?;
        set.link_at(block.text_start + position.offset)
            .map(|link| link.href.clone())
    }

    /// Where the reader is, in terms that survive a reparse.
    pub fn reading_anchor(&self) -> Anchor {
        let state = self.imp().state.borrow();
        if state.plan.is_empty() {
            return Anchor::default();
        }
        let top = self.scroll_top();
        let block = state.plan.block_at(top);
        let heading = state
            .outline
            .active_for_block(block)
            .map(|index| state.outline.id(index).to_string());
        // Measured from the anchor itself, so the same line stays at the same
        // height even when the blocks above it changed size.
        let from = match (&heading, &state.outline) {
            (Some(id), outline) => outline
                .block_for_id(id)
                .map(|block| state.plan.y_of(block))
                .unwrap_or(0.0),
            _ => state.plan.y_of(block),
        };
        Anchor {
            heading,
            block,
            distance: top - from,
        }
    }

    /// Puts the reader back where the anchor says, after a reload.
    pub fn restore_anchor(&self, anchor: &Anchor) {
        let target = {
            let state = self.imp().state.borrow();
            if state.plan.is_empty() {
                return;
            }
            let block = anchor
                .heading
                .as_deref()
                .and_then(|id| state.outline.block_for_id(id))
                .unwrap_or_else(|| anchor.block.min(state.plan.len() - 1));
            state.plan.y_of(block) + anchor.distance
        };
        if let Some(adjustment) = self.vadjustment() {
            adjustment.set_value(target.max(0.0));
        }
        self.queue_draw();
    }

    /// Copies the code block under a point, if the copy control was hit.
    fn copy_code_at(&self, x: f64, y: f64) -> bool {
        let text = {
            let state = self.imp().state.borrow();
            if state.plan.is_empty() {
                return false;
            }
            let top = self.scroll_top();
            let width = self.view_width();
            let column = state.plan.width();
            let left = ((width - column) / 2.0).max(tokens::PAD_SIDE);
            let index = state.plan.block_at(top + y);
            let Some(set) = state.cache.get(&index) else {
                return false;
            };
            let block_top = state.plan.y_of(index) - top + set.baseline_offset();
            if !set.control_at(x - left, y - block_top) {
                return false;
            }
            // The original code text of the whole block — the control sits on
            // its first part, but a cut-up block is copied entire — without
            // the newline that closed its last line (SPEC.md, section 8).
            let (from, to) = state.plan.source_text_range(index);
            let text = &state.document.text[from as usize..to as usize];
            Some(text.strip_suffix('\n').unwrap_or(text).to_string())
        };
        match text {
            Some(text) if !text.is_empty() => {
                gtk::prelude::WidgetExt::clipboard(self).set_text(&text);
                true
            }
            _ => false,
        }
    }

    /// Runs a search over the whole document text and returns the number of
    /// hits. Nothing is re-parsed and no layout is discarded: typing in the
    /// search field must not cost a re-set (SPEC.md, sections 5 and 8).
    pub fn search(&self, needle: &str) -> usize {
        let hits = match Query::new(needle) {
            Some(query) => {
                let text = self.imp().state.borrow().document.clone();
                query.matches(&text.text)
            }
            None => Vec::new(),
        };
        let count = hits.len();
        {
            let mut state = self.imp().state.borrow_mut();
            state.hits = hits;
            state.current_hit = None;
        }
        // Land on the first hit at or after where the reader is.
        if count > 0 {
            self.step_hit(true);
        } else {
            self.queue_draw();
        }
        count
    }

    pub fn clear_search(&self) {
        let mut state = self.imp().state.borrow_mut();
        state.hits.clear();
        state.current_hit = None;
        drop(state);
        self.queue_draw();
    }

    /// `(current, total)`, one-based for display.
    pub fn search_position(&self) -> (usize, usize) {
        let state = self.imp().state.borrow();
        (
            state.current_hit.map(|index| index + 1).unwrap_or(0),
            state.hits.len(),
        )
    }

    pub fn search_next(&self) {
        self.step_hit(true);
    }
    pub fn search_previous(&self) {
        self.step_hit(false);
    }

    fn step_hit(&self, forward: bool) {
        let target = {
            let state = self.imp().state.borrow();
            if state.hits.is_empty() {
                None
            } else {
                // Without a current hit, step relative to the reading
                // position, so the first Enter goes to the nearest one below.
                let from = match state.current_hit {
                    Some(index) => state.hits[index].start,
                    None => {
                        let block = state.plan.block_at(self.scroll_top());
                        state.plan.block(block).text_start
                    }
                };
                if forward {
                    search::next_from(&state.hits, from)
                } else {
                    search::previous_from(&state.hits, from)
                }
            }
        };
        let Some(index) = target else {
            self.queue_draw();
            return;
        };
        let block = {
            let mut state = self.imp().state.borrow_mut();
            state.current_hit = Some(index);
            let offset = state.hits[index].start;
            state.plan.block_for_text(offset)
        };
        // The jump goes through the plan, so the target need never have been
        // set before (SPEC.md, section 8).
        if let Some(block) = block {
            self.scroll_to_block_centred(block);
        }
        self.queue_draw();
    }

    fn scroll_to_block_centred(&self, index: usize) {
        if index >= self.imp().state.borrow().plan.len() {
            return;
        }
        self.imp().state.borrow_mut().aim = Some((index, AIM_PASSES));
        self.settle_aim();
    }

    /// Brings the aimed-at hit a third of the way down the viewport, and says
    /// whether that moved the view.
    ///
    /// `y_of` is exact for blocks that have been set and an estimate for the
    /// rest, so one aim at a block far below the reading position lands
    /// wherever the estimates happened to put it. On the 100 KiB fixture that
    /// was a whole screen short: the search reported "1 von 1" and the line it
    /// had found stayed below the bottom edge, at three seconds and at eight,
    /// and only a second `Enter` brought it into view. The draw pass sets what
    /// the first aim landed on, which replaces the estimates that were most of
    /// the error, and this then aims again at the better answer. Each pass
    /// costs one viewport of setting and one frame, and the counter bounds a
    /// target the plan never agrees with to a handful of them rather than a
    /// livelock.
    fn settle_aim(&self) -> bool {
        let Some((index, left)) = self.imp().state.borrow().aim else {
            return false;
        };
        let height = self.view_height();
        let y = {
            let state = self.imp().state.borrow();
            (index < state.plan.len()).then(|| state.plan.y_of(index))
        };
        let (Some(y), Some(adjustment)) = (y, self.vadjustment()) else {
            self.imp().state.borrow_mut().aim = None;
            return false;
        };
        let value = (y - height / 3.0).max(0.0);
        let moved = (adjustment.value() - value).abs() >= 1.0;
        self.imp().state.borrow_mut().aim = match moved && left > 0 {
            true => Some((index, left - 1)),
            false => None,
        };
        if moved {
            // The move is the view's own, so the reader-scrolled path above
            // must not see it and cancel the aim it is part of.
            self.imp().state.borrow_mut().adjusting = true;
            adjustment.set_value(value);
            self.imp().state.borrow_mut().adjusting = false;
            self.queue_draw();
        }
        moved
    }

    /// Selects the whole document.
    pub fn select_all(&self) {
        let mut state = self.imp().state.borrow_mut();
        if state.plan.is_empty() {
            return;
        }
        let last = state.plan.len() - 1;
        let end = state.plan.block(last).text_len;
        state.selection = Some(Selection::at(Position::new(0, 0)).to(Position::new(last, end)));
        drop(state);
        self.selection_changed();
    }

    /// The selected text, in document order.
    pub fn selected_text(&self) -> String {
        let state = self.imp().state.borrow();
        match state.selection {
            Some(selection) => selection.text(&state.plan, &state.document.text),
            None => String::new(),
        }
    }

    /// Copies the selection to the clipboard.
    pub fn copy_selection(&self) {
        let text = self.selected_text();
        if !text.is_empty() {
            gtk::prelude::WidgetExt::clipboard(self).set_text(&text);
        }
    }

    /// Scrolls so that a block is in view, for an anchor or a search hit. It
    /// works from the plan, so the target need never have been set before
    /// (SPEC.md, section 8).
    pub fn scroll_to_block(&self, index: usize) {
        let y = {
            let state = self.imp().state.borrow();
            if index >= state.plan.len() {
                return;
            }
            state.plan.y_of(index)
        };
        if let Some(adjustment) = self.vadjustment() {
            adjustment.set_value(y - tokens::PAD_TOP);
        }
        self.queue_draw();
    }
}

fn next_boundary(text: &str, from: usize) -> usize {
    let mut index = from + 1;
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index.min(text.len())
}

/// Puts syntax colours onto a code block's layout as Pango attributes.
fn apply_highlight(set: &BlockLayout, spans: &[Span], palette: Palette) {
    for piece in set.pieces.iter().filter(|piece| !piece.control) {
        let attributes = piece.layout.attributes().unwrap_or_default();
        for span in spans {
            let Some((from, to)) = piece.map.clip(span.start, span.end) else {
                continue;
            };
            let colour = match span.kind {
                Kind::Keyword => palette.syntax_keyword,
                Kind::Literal => palette.syntax_string,
                Kind::Number => palette.syntax_number,
            };
            let mut attribute = pango::AttrColor::new_foreground(
                (colour.red * 65535.0) as u16,
                (colour.green * 65535.0) as u16,
                (colour.blue * 65535.0) as u16,
            )
            .upcast();
            attribute.set_start_index(from);
            attribute.set_end_index(to);
            attributes.insert(attribute);
        }
        piece.layout.set_attributes(Some(&attributes));
    }
}

/// Paints a byte range of a piece, line by line, so that a range spanning
/// wrapped lines reads as one shape.
fn draw_ranges(
    snapshot: &gtk::Snapshot,
    piece: &crate::layout::Piece,
    x: f64,
    y: f64,
    from: u32,
    to: u32,
    colour: crate::theme::Color,
) {
    let colour = colour.to_gdk();
    for rect in selection_rects(&piece.layout, from, to) {
        snapshot.append_color(
            &colour,
            &gtk::graphene::Rect::new(
                x as f32 + rect.x(),
                y as f32 + rect.y(),
                rect.width(),
                rect.height(),
            ),
        );
    }
}

/// LayoutIter extents are relative to the layout origin. LayoutLine extents
/// are relative to the baseline and cannot be used to paint a selection.
fn selection_rects(layout: &pango::Layout, from: u32, to: u32) -> Vec<gtk::graphene::Rect> {
    let mut result = Vec::new();
    let mut iter = layout.iter();
    loop {
        if let Some(line) = iter.line_readonly() {
            let start = from.max(line.start_index() as u32);
            let end = to.min((line.start_index() + line.length()) as u32);
            if start < end {
                let (_, logical) = iter.line_extents();
                // One logical range can occupy multiple visual ranges in bidi text.
                for range in line.x_ranges(start as i32, end as i32).as_chunks::<2>().0 {
                    let scale = pango::SCALE as f32;
                    result.push(gtk::graphene::Rect::new(
                        range[0] as f32 / scale,
                        logical.y() as f32 / scale,
                        (range[1] - range[0]) as f32 / scale,
                        logical.height() as f32 / scale,
                    ));
                }
            }
        }
        if !iter.next_line() {
            break;
        }
    }
    result
}

/// The body font comes from the system; code uses the fontconfig `monospace`
/// alias. Nothing is bundled and nothing is downloaded (SPEC.md, section 3).
fn font_families() -> (String, String) {
    let body = gtk::Settings::default()
        .map(|settings| settings.gtk_font_name())
        .and_then(|name| name.map(|name| name.to_string()))
        .and_then(|name| {
            let description = pango::FontDescription::from_string(&name);
            description.family().map(|family| family.to_string())
        })
        .unwrap_or_else(|| "sans".to_string());
    (body, "monospace".to_string())
}

#[cfg(test)]
impl DocumentView {
    /// Where the reader is, in document coordinates, and where a block sits.
    /// The reload test compares the two across a reparse.
    pub(crate) fn scroll_offset(&self) -> f64 {
        self.scroll_top()
    }
    pub(crate) fn block_top(&self, index: usize) -> f64 {
        self.imp().state.borrow().plan.y_of(index)
    }

    pub(crate) fn verify_accessibility_and_selection(&self) {
        self.set_document(hashline_markdown::parse("# Grüße 🌍\n\nÄpfel und Öl.\n"));
        assert_eq!(self.accessible_role(), gtk::AccessibleRole::Document);
        assert!(self.is::<gtk::AccessibleText>());
        assert_eq!(
            self.imp().contents(4, 7).unwrap().as_ref(),
            "e 🌍".as_bytes()
        );
        self.set_selection_at(Position::new(0, 0), 1, false);
        self.set_selection_at(Position::new(1, 2), 1, true);
        assert_eq!(self.imp().caret_position(), 10);
        let ranges = self.imp().selection();
        assert_eq!((ranges[0].start(), ranges[0].length()), (0, 10));
        assert_eq!(self.selected_text(), "Grüße 🌍\n\nÄ");
        self.set_selection_at(Position::new(1, 3), 2, false);
        assert_eq!(self.selected_text(), "Äpfel");
        self.set_selection_at(Position::new(1, 3), 3, false);
        assert_eq!(self.selected_text(), "Äpfel und Öl.");
        let selection = self.selected_text();
        // Releasing a click/drag must never clear an existing selection.
        let controllers = self.observe_controllers();
        for i in 0..controllers.n_items() {
            let controller = controllers.item(i).unwrap();
            if let Some(click) = controller.downcast_ref::<gtk::GestureClick>() {
                click.emit_by_name::<()>("released", &[&1i32, &60.0f64, &100.0f64]);
            }
            if let Some(drag) = controller.downcast_ref::<gtk::GestureDrag>() {
                drag.emit_by_name::<()>("drag-end", &[&30.0f64, &0.0f64]);
            }
        }
        assert_eq!(self.selected_text(), selection);
        let body = "Grüße aus Berlin und Äpfel mit Öl. ".repeat(20);
        self.set_document(hashline_markdown::parse(&body));
        let len = self.imp().state.borrow_mut().accessible().0.len;
        let mut offset = 0;
        let mut lines = 0;
        while offset < len {
            let (start, end, _) = self
                .imp()
                .contents_at(offset, gtk::AccessibleTextGranularity::Line)
                .unwrap();
            assert_eq!(start, offset);
            assert!(end > start);
            offset = end;
            lines += 1;
        }
        assert!(
            lines > 1,
            "screen-reader queries must follow visual wrapping"
        );
        self.set_document(hashline_markdown::parse("# Grüße 🌍\n\nÄpfel und Öl.\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_syntax_colour_cache_is_bounded_like_the_layout_cache() {
        // Both caches used to be cleared only when the document changed, so a
        // reader scrolling through the 28 931 code blocks of `large.md` in one
        // sitting collected the spans of all of them. The fifty-switch
        // stability run never saw it, because every switch cleared everything.
        let mut state = State::empty();
        for index in 0..HIGHLIGHT_LIMIT * 3 {
            state.remember_highlight(index, None);
            // The worker answering must not enter the block a second time.
            state.remember_highlight(index, Some(Vec::new()));
        }
        assert_eq!(state.highlights.len(), HIGHLIGHT_LIMIT);
        assert_eq!(state.coloured.len(), HIGHLIGHT_LIMIT);
        // What it keeps is what was asked for last, which is what scrolling
        // back a screen needs.
        assert!(state.highlights.contains_key(&(HIGHLIGHT_LIMIT * 3 - 1)));
        assert!(!state.highlights.contains_key(&0));
    }

    #[test]
    fn selection_covers_each_wrapped_line_at_its_actual_position() {
        let context = pangocairo::FontMap::default().create_context();
        let layout = pango::Layout::new(&context);
        layout.set_text("Grüße aus Berlin, mit genügend Text für mehrere umgebrochene Zeilen und eine letzte Zeile.");
        layout.set_width(180 * pango::SCALE);
        layout.set_line_spacing(1.65);
        let rects = selection_rects(&layout, 0, layout.text().len() as u32);
        assert!(rects.len() >= 3);
        for pair in rects.windows(2) {
            assert!(pair[1].y() > pair[0].y());
        }
        for rect in rects {
            assert!(rect.y() >= 0.0);
            let (_, index, _) = layout.xy_to_index(
                ((rect.x() + rect.width() / 2.0) * pango::SCALE as f32) as i32,
                ((rect.y() + rect.height() / 2.0) * pango::SCALE as f32) as i32,
            );
            let caret = layout.index_to_pos(index);
            let caret_y = caret.y() as f32 / pango::SCALE as f32;
            assert!(caret_y >= rect.y() - 1.0 && caret_y <= rect.y() + rect.height());
        }
    }
}
