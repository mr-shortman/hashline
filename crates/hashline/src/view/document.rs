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

use crate::layout::{set_block, BlockLayout, BlockPlan, Decoration, Metrics, Style};
use crate::outline::Outline;
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
    outline: Outline,
    images: Rc<ImageCache>,
    /// Called when a link is clicked. The view resolves nothing itself: what a
    /// relative path or a fragment means is the document controller's business.
    on_link: Option<LinkHandler>,
    /// Set while the widget itself is moving the adjustment, so that the
    /// resulting notification is not mistaken for the user scrolling.
    adjusting: bool,
}

/// The handler a clicked link is passed to.
type LinkHandler = Rc<dyn Fn(&str)>;

/// How many set blocks to keep. A block that is evicted keeps its measured
/// height in the plan, so eviction costs re-setting, never a jump.
const CACHE_LIMIT: usize = 240;

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
            outline: Outline::default(),
            images: Rc::new(ImageCache::default()),
            on_link: None,
            adjusting: false,
        }
    }
    fn body_px(&self) -> f64 {
        tokens::BODY_PX * self.zoom as f64 / 100.0
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
    }

    impl Default for DocumentView {
        fn default() -> Self {
            DocumentView {
                vadjustment: RefCell::new(None),
                hadjustment: RefCell::new(None),
                vscroll_policy: RefCell::new(gtk::ScrollablePolicy::Minimum),
                hscroll_policy: RefCell::new(gtk::ScrollablePolicy::Minimum),
                state: RefCell::new(State::empty()),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DocumentView {
        const NAME: &'static str = "HashlineDocumentView";
        type Type = super::DocumentView;
        type ParentType = gtk::Widget;
        type Interfaces = (gtk::Scrollable,);
    }

    #[glib::derived_properties]
    impl ObjectImpl for DocumentView {
        fn constructed(&self) {
            self.parent_constructed();
            let widget = self.obj();
            widget.set_focusable(true);
            widget.set_can_focus(true);
            widget.setup_gestures();
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
                        if !widget.imp().state.borrow().adjusting {
                            widget.queue_draw();
                        }
                    }
                ));
            }
            self.vadjustment.replace(adjustment);
            widget.queue_allocate();
        }
    }

    impl WidgetImpl for DocumentView {
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
            widget.reflow_for(width as f64);
            widget.update_adjustment(width as f64, height as f64);
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().draw(snapshot);
        }
    }

    impl ScrollableImpl for DocumentView {}
}

glib::wrapper! {
    pub struct DocumentView(ObjectSubclass<imp::DocumentView>)
        @extends gtk::Widget,
        @implements gtk::Scrollable, gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
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
    pub fn set_document(&self, document: Rc<OpDocument>) {
        {
            let mut state = self.imp().state.borrow_mut();
            state.document = document;
            state.cache.clear();
            state.recent.clear();
            state.selection = None;
        }
        let width = self.view_width().max(1.0);
        self.reflow_for(width);
        if let Some(adjustment) = self.vadjustment() {
            adjustment.set_value(0.0);
        }
        self.queue_draw();
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
        let mut state = self.imp().state.borrow_mut();
        let (body, mono) = font_families();
        let body_px = state.body_px();
        state.style = Style::new(&body, &mono, body_px, state.palette);
        state.cache.clear();
        state.recent.clear();
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
        let document = state.document.clone();
        state.plan = BlockPlan::new(&document, metrics, column);
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
    fn measure_visible(&self, height: f64) {
        let top = self.scroll_top();
        let context = self.pango_context();
        let mut shift = 0.0;
        let range = {
            let state = self.imp().state.borrow();
            state.plan.visible_range(top, height)
        };
        for index in range {
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
            let measured = set.height();
            let mut state = self.imp().state.borrow_mut();
            let was_above = state.plan.y_of(index) + block.height <= top;
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
        }
    }

    fn draw(&self, snapshot: &gtk::Snapshot) {
        let width = self.view_width();
        let height = self.view_height();
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        self.measure_visible(height);
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
                        if *radius > 0.0 {
                            let rounded = gtk::gsk::RoundedRect::from_rect(rect, *radius);
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
                if let Some(selection) = state.selection {
                    if let Some((from, to)) = selection.in_block(index, block.text_len) {
                        let base = block.text_start;
                        if let Some((lo, hi)) = piece.map.clip(base + from, base + to) {
                            draw_selection(snapshot, piece, x, y, lo, hi, &palette);
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
        for piece in &set.pieces {
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

    fn setup_gestures(&self) {
        let click = gtk::GestureClick::new();
        click.connect_pressed(glib::clone!(
            #[weak(rename_to = widget)]
            self,
            move |_, clicks, x, y| {
                widget.grab_focus();
                // A link is followed rather than selected — but only on a
                // single click, so that double-clicking to select a word
                // inside a link still works.
                if clicks == 1 {
                    if let Some(href) = widget.link_at(x, y) {
                        let handler = widget.imp().state.borrow().on_link.clone();
                        if let Some(handler) = handler {
                            handler(&href);
                            return;
                        }
                    }
                }
                if let Some(position) = widget.position_at(x, y) {
                    widget.imp().state.borrow_mut().selection = Some(Selection::at(position));
                    widget.queue_draw();
                }
            }
        ));
        self.add_controller(click);

        let drag = gtk::GestureDrag::new();
        drag.connect_drag_update(glib::clone!(
            #[weak(rename_to = widget)]
            self,
            move |gesture, dx, dy| {
                let Some((x, y)) = gesture.start_point() else {
                    return;
                };
                if let Some(cursor) = widget.position_at(x + dx, y + dy) {
                    let mut state = widget.imp().state.borrow_mut();
                    if let Some(selection) = state.selection {
                        state.selection = Some(selection.to(cursor));
                    }
                    drop(state);
                    widget.queue_draw();
                }
            }
        ));
        self.add_controller(drag);

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

    pub fn outline(&self) -> Outline {
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
        self.queue_draw();
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

fn draw_selection(
    snapshot: &gtk::Snapshot,
    piece: &crate::layout::Piece,
    x: f64,
    y: f64,
    from: u32,
    to: u32,
    palette: &Palette,
) {
    let colour = palette.selection().to_gdk();
    let mut line_index = 0;
    while let Some(line) = piece.layout.line(line_index) {
        let start = line.start_index() as u32;
        let end = start + line.length() as u32;
        let overlap_from = from.max(start);
        let overlap_to = to.min(end);
        if overlap_from < overlap_to {
            let (_, extents) = line.extents();
            let x0 = line.index_to_x(overlap_from as i32, false) as f64 / pango::SCALE as f64;
            let x1 = line.index_to_x(overlap_to as i32, false) as f64 / pango::SCALE as f64;
            let line_top = extents.y() as f64 / pango::SCALE as f64;
            let line_height = extents.height() as f64 / pango::SCALE as f64;
            snapshot.append_color(
                &colour,
                &gtk::graphene::Rect::new(
                    (x + x0.min(x1)) as f32,
                    (y + line_top) as f32,
                    (x1 - x0).abs().max(1.0) as f32,
                    line_height as f32,
                ),
            );
        }
        line_index += 1;
    }
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
