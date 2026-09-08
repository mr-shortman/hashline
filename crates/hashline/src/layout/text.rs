//! Setting one block with Pango.
//!
//! This is the only place that turns operations into type (SPEC.md, section 6,
//! "Datenfluss", step 6). A block becomes a handful of *pieces* — a paragraph
//! is one, a list is one per item, a table is one per cell — plus flat
//! decoration behind them: the rule under an H2, the bar beside a quote, the
//! panel under a code block.
//!
//! Each piece carries a [`TextMap`], so a position in a laid-out piece still
//! names a byte in the document even where the piece contains text the
//! document does not have, such as a bullet. Selection, search highlighting
//! and hit-testing all go through that one mapping rather than each keeping
//! its own idea of where things are.

use hashline_markdown::{OpDocument, OP_OPEN, OP_TEXT};
use pango::prelude::*;

use crate::layout::{Block, BlockKind, TextMap};
use crate::theme::{document, Color, Palette};

/// Tag ids, as indices into `ALLOWED_TAGS`.
const TAG_EM: u32 = 7;
const TAG_STRONG: u32 = 8;
const TAG_DEL: u32 = 9;
const TAG_S: u32 = 10;
const TAG_CODE: u32 = 11;
const TAG_UL: u32 = 14;
const TAG_OL: u32 = 15;
const TAG_LI: u32 = 16;
const TAG_BR: u32 = 18;
const TAG_A: u32 = 19;
const TAG_IMG: u32 = 20;
const TAG_TABLE: u32 = 21;
const TAG_TR: u32 = 25;
const TAG_TH: u32 = 26;
const TAG_TD: u32 = 27;
const TAG_INPUT: u32 = 28;

const ATTR_HREF: u32 = 1;
const ATTR_SRC: u32 = 2;
const ATTR_ALT: u32 = 3;
const ATTR_CLASS: u32 = 5;
const ATTR_START: u32 = 6;
const ATTR_CHECKED: u32 = 8;

/// The type a document is set in, at the current zoom.
#[derive(Clone, Debug)]
pub struct Style {
    pub body: pango::FontDescription,
    pub mono: pango::FontDescription,
    /// Body size in logical pixels after zoom.
    pub body_px: f64,
    pub palette: Palette,
}

impl Style {
    pub fn new(body_family: &str, mono_family: &str, body_px: f64, palette: Palette) -> Self {
        let mut body = pango::FontDescription::new();
        body.set_family(body_family);
        body.set_absolute_size(body_px * pango::SCALE as f64);
        let mut mono = pango::FontDescription::new();
        mono.set_family(mono_family);
        mono.set_absolute_size(body_px * document::INLINE_CODE_EM * pango::SCALE as f64);
        Style {
            body,
            mono,
            body_px,
            palette,
        }
    }

    fn sized(&self, em: f64, mono: bool) -> pango::FontDescription {
        let mut font = if mono {
            self.mono.clone()
        } else {
            self.body.clone()
        };
        font.set_absolute_size(self.body_px * em * pango::SCALE as f64);
        font
    }
}

/// One laid-out run of type inside a block.
pub struct Piece {
    pub layout: pango::Layout,
    /// Offset from the block's left edge and top edge.
    pub x: f64,
    pub y: f64,
    pub color: Color,
    pub map: TextMap,
    pub links: Vec<Link>,
}

/// What the layout may ask about a picture. The layout itself never touches
/// the file system: it needs the intrinsic size to reserve the right space,
/// and the view — which owns the cache, the budget and the access rules —
/// answers (SPEC.md, sections 7 and 11).
pub trait ImageSource {
    /// Intrinsic size in logical pixels, or `None` when the picture will not
    /// be shown at all and a placeholder should take its place.
    fn intrinsic(&self, source: &str) -> Option<(f64, f64)>;
}

/// For contexts with no pictures: the offscreen renderer and the tests.
pub struct NoImages;

impl ImageSource for NoImages {
    fn intrinsic(&self, _source: &str) -> Option<(f64, f64)> {
        None
    }
}

/// Flat shapes drawn behind the type.
pub enum Decoration {
    Fill {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        radius: f32,
        color: Color,
    },
    Line {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        color: Color,
    },
    /// A picture, resolved to a texture by the view when it draws.
    Image {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        source: String,
    },
}

/// A block that has been set.
pub struct BlockLayout {
    pub pieces: Vec<Piece>,
    pub decorations: Vec<Decoration>,
    pub space_before: f64,
    pub space_after: f64,
    /// Height of the content between the two spacings.
    pub content_height: f64,
    /// Width the content wants. Wider than the column for code and tables,
    /// which scroll inside their own block rather than making the document
    /// scroll sideways (SPEC.md, section 3).
    pub content_width: f64,
}

impl BlockLayout {
    pub fn height(&self) -> f64 {
        self.space_before + self.content_height + self.space_after
    }
    /// Where the content begins, relative to the block's top edge.
    pub fn baseline_offset(&self) -> f64 {
        self.space_before
    }
    /// The link covering a document offset, if any.
    pub fn link_at(&self, offset: u32) -> Option<&Link> {
        self.pieces
            .iter()
            .flat_map(|piece| piece.links.iter())
            .find(|link| offset >= link.from && offset < link.to)
    }

    /// The document range this block's pieces cover.
    pub fn document_range(&self) -> Option<(u32, u32)> {
        let mut range: Option<(u32, u32)> = None;
        for piece in &self.pieces {
            if let Some((from, to)) = piece.map.document_range() {
                range = Some(match range {
                    Some((a, b)) => (a.min(from), b.max(to)),
                    None => (from, to),
                });
            }
        }
        range
    }
}

/// Assembles one piece's text while remembering where each byte came from.
struct Compose {
    text: String,
    map: TextMap,
    spans: Vec<Span>,
    open: Vec<usize>,
    /// A marker that is written just before the first document text, so that
    /// a task-list checkbox can replace a bullet after the fact.
    pending: Option<String>,
    /// Whether the next run begins with a hard break.
    keep_break: bool,
}

struct Span {
    tag: u32,
    raw_html: bool,
    href: Option<String>,
    from: Option<u32>,
    to: u32,
}

/// A link and the document bytes it covers, so that a click can find it
/// without the view re-walking the operations.
#[derive(Clone, Debug)]
pub struct Link {
    pub from: u32,
    pub to: u32,
    pub href: String,
}

impl Compose {
    fn new() -> Self {
        Compose {
            text: String::new(),
            map: TextMap::default(),
            spans: Vec::new(),
            open: Vec::new(),
            pending: None,
            keep_break: false,
        }
    }
    /// The next run starts right after a `<br>`, so its leading newline is a
    /// hard break and stays one.
    fn after_break(&mut self) {
        self.keep_break = true;
    }
    fn is_empty(&self) -> bool {
        self.text.is_empty() && self.pending.is_none()
    }
    fn flush_marker(&mut self) {
        if let Some(marker) = self.pending.take() {
            self.text.push_str(&marker);
        }
    }
    /// Text the document does not contain.
    fn insert(&mut self, value: &str) {
        self.text.push_str(value);
    }
    /// Text from the document, at `offset`.
    ///
    /// With `collapse`, a newline inside the run is a Markdown soft break and
    /// is set as a space — that is what the reference does, because in HTML a
    /// newline in text content is whitespace. The substitution is one byte for
    /// one byte, so the offset mapping stays exact. A hard break keeps its
    /// newline; the walker announces one through [`Compose::after_break`].
    fn document(&mut self, offset: u32, value: &str, collapse: bool) {
        self.flush_marker();
        // A jump in document offsets means a block separator was skipped —
        // two paragraphs inside one list item or one quote. Without putting a
        // break back the two would run together as one sentence.
        if let Some((_, end)) = self.map.document_range() {
            if offset > end {
                self.text.push('\n');
            }
        }
        let at = self.text.len() as u32;
        self.map.push(at, offset, value.len() as u32);
        if collapse && value.contains('\n') {
            let keep = std::mem::take(&mut self.keep_break);
            for (index, character) in value.char_indices() {
                if character == '\n' && !(keep && index == 0) {
                    self.text.push(' ');
                } else {
                    self.text.push(character);
                }
            }
        } else {
            self.keep_break = false;
            self.text.push_str(value);
        }
        let to = at + value.len() as u32;
        for &index in &self.open {
            let span = &mut self.spans[index];
            span.from.get_or_insert(at);
            span.to = to;
        }
    }
    /// Puts a marker in front of everything composed so far.
    ///
    /// A list marker is only known once the item has been walked: an ordered
    /// item needs its number, and a task item replaces the bullet with its
    /// checkbox, which is an operation that arrives after the item opens.
    /// Prepending shifts what is already there rather than deferring the text.
    fn prepend(&mut self, marker: &str) {
        if marker.is_empty() {
            return;
        }
        let shift = marker.len() as u32;
        self.text.insert_str(0, marker);
        self.map.shift(shift);
        for span in self.spans.iter_mut() {
            if let Some(from) = span.from.as_mut() {
                *from += shift;
            }
            span.to += shift;
        }
    }
    fn open_span(&mut self, tag: u32, raw_html: bool, href: Option<String>) {
        self.spans.push(Span {
            tag,
            raw_html,
            href,
            from: None,
            to: 0,
        });
        self.open.push(self.spans.len() - 1);
    }
    fn close_span(&mut self) {
        self.open.pop();
    }

    /// Turns the composed text into a piece.
    #[allow(clippy::too_many_arguments)]
    fn finish(
        mut self,
        context: &pango::Context,
        font: &pango::FontDescription,
        style: &Style,
        em: f64,
        line_height: f64,
        width: Option<f64>,
        color: Color,
    ) -> Piece {
        self.flush_marker();
        let layout = pango::Layout::new(context);
        layout.set_text(&self.text);
        layout.set_font_description(Some(font));
        match width {
            Some(width) => {
                layout.set_width((width * pango::SCALE as f64) as i32);
                layout.set_wrap(pango::WrapMode::WordChar);
            }
            None => layout.set_width(-1),
        }

        let attributes = pango::AttrList::new();
        let add = |mut attribute: pango::Attribute, from: u32, to: u32| {
            attribute.set_start_index(from);
            attribute.set_end_index(to);
            attributes.insert(attribute);
        };
        // Line height is set absolutely, not with `set_line_spacing`: that
        // multiplies the font's *natural* spacing, which already contains the
        // face's leading, so the design reference's `line-height: 1.65` would
        // land near 2.0 and read far too airy.
        add(
            pango::AttrInt::new_line_height_absolute(
                (em * line_height * pango::SCALE as f64) as i32,
            )
            .upcast(),
            0,
            self.text.len() as u32,
        );

        for span in &self.spans {
            let Some(from) = span.from else { continue };
            let to = span.to;
            match span.tag {
                TAG_EM => add(
                    pango::AttrInt::new_style(pango::Style::Italic).upcast(),
                    from,
                    to,
                ),
                TAG_STRONG => add(
                    pango::AttrInt::new_weight(pango::Weight::Bold).upcast(),
                    from,
                    to,
                ),
                TAG_DEL | TAG_S => add(pango::AttrInt::new_strikethrough(true).upcast(), from, to),
                TAG_CODE => {
                    add(
                        pango::AttrFontDesc::new(&style.sized(document::INLINE_CODE_EM, true))
                            .upcast(),
                        from,
                        to,
                    );
                    // `:not(pre) > code { background: var(--code) }`. Pango
                    // paints a flat rectangle; the reference's 4px radius and
                    // padding are not expressible as a text attribute.
                    add(background(style.palette.code), from, to);
                    // Raw HTML is source text, not the author's code, and is
                    // set apart more quietly (SPEC.md, section 6).
                    if span.raw_html {
                        add(foreground(style.palette.muted), from, to);
                    }
                }
                TAG_A if span.href.is_some() => {
                    add(foreground(style.palette.accent), from, to);
                    add(
                        pango::AttrInt::new_underline(pango::Underline::Single).upcast(),
                        from,
                        to,
                    );
                }
                _ => {}
            }
        }
        layout.set_attributes(Some(&attributes));

        // Link ranges are recorded in *document* coordinates: that is what a
        // hit test produces and what survives the piece being re-set.
        let mut links = Vec::new();
        for span in &self.spans {
            let (Some(href), Some(from)) = (span.href.as_ref(), span.from) else {
                continue;
            };
            if span.tag != TAG_A || span.to <= from {
                continue;
            }
            if let (Some(a), Some(b)) = (
                self.map.to_document(from),
                self.map.to_document(span.to - 1),
            ) {
                links.push(Link {
                    from: a,
                    to: b + 1,
                    href: href.clone(),
                });
            }
        }

        Piece {
            layout,
            x: 0.0,
            y: 0.0,
            color,
            map: self.map,
            links,
        }
    }
}

fn foreground(color: Color) -> pango::Attribute {
    pango::AttrColor::new_foreground(
        (color.red * 65535.0) as u16,
        (color.green * 65535.0) as u16,
        (color.blue * 65535.0) as u16,
    )
    .upcast()
}

fn background(color: Color) -> pango::Attribute {
    pango::AttrColor::new_background(
        (color.red * 65535.0) as u16,
        (color.green * 65535.0) as u16,
        (color.blue * 65535.0) as u16,
    )
    .upcast()
}

/// One operation of the buffer, decoded.
enum Op {
    Open { tag: u32, attrs: u32, count: u32 },
    Close,
    Text { offset: u32, len: u32 },
}

fn ops_of<'a>(document: &'a OpDocument, block: &Block) -> impl Iterator<Item = Op> + 'a {
    let first = block.op_start as usize * 4;
    let last = first + block.op_count as usize * 4;
    document.ops[first..last]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|op| match op[0] {
            OP_OPEN => Op::Open {
                tag: op[1],
                attrs: op[2],
                count: op[3],
            },
            OP_TEXT => Op::Text {
                offset: op[1],
                len: op[2],
            },
            _ => Op::Close,
        })
}

fn attribute(document: &OpDocument, attrs: u32, count: u32, name: u32) -> Option<&str> {
    (0..count as usize).find_map(|index| {
        let at = (attrs as usize + index) * 3;
        if document.attrs[at] != name {
            return None;
        }
        let from = document.attrs[at + 1] as usize;
        let to = from + document.attrs[at + 2] as usize;
        Some(&document.strings[from..to])
    })
}

fn has_class(document: &OpDocument, attrs: u32, count: u32, value: &str) -> bool {
    attribute(document, attrs, count, ATTR_CLASS) == Some(value)
}

/// Sets one block. `width` is the reading column in logical pixels.
pub fn set_block(
    context: &pango::Context,
    document: &OpDocument,
    block: &Block,
    style: &Style,
    width: f64,
    images: &dyn ImageSource,
) -> BlockLayout {
    let em = style.body_px;
    if let Some(layout) = picture(context, document, block, style, width, images) {
        return layout.with_spacing(block.kind, em);
    }
    match block.kind {
        BlockKind::Rule => rule(style, width),
        BlockKind::Code => code(context, document, block, style, width),
        BlockKind::List => list(context, document, block, style, width),
        BlockKind::Table => table(context, document, block, style, width),
        BlockKind::Quote => quote(context, document, block, style, width),
        _ => simple(context, document, block, style, width),
    }
    .with_spacing(block.kind, em)
}

impl BlockLayout {
    fn with_spacing(mut self, kind: BlockKind, em: f64) -> Self {
        self.space_before = match kind {
            BlockKind::Heading(_) => document::HEADING_SPACE_BEFORE_EM * em,
            _ => 0.0,
        };
        self.space_after = match kind {
            BlockKind::Heading(_) => document::HEADING_SPACE_AFTER_EM * em,
            BlockKind::Rule => document::RULE_SPACING_EM * em,
            BlockKind::Code => document::CODE_SPACING_EM * em,
            BlockKind::Table => document::TABLE_SPACING_EM * em,
            _ => document::BLOCK_SPACING_EM * em,
        };
        self
    }
}

/// Walks the inline content of a block into one `Compose`.
fn inline_into(compose: &mut Compose, document: &OpDocument, block: &Block) {
    for op in ops_of(document, block) {
        match op {
            Op::Open { tag, attrs, count } => {
                if tag == TAG_BR {
                    compose.after_break();
                }
                compose.open_span(
                    tag,
                    has_class(document, attrs, count, "raw-html"),
                    attribute(document, attrs, count, ATTR_HREF).map(str::to_owned),
                )
            }
            Op::Close => compose.close_span(),
            Op::Text { offset, len } => {
                let from = offset as usize;
                compose.document(offset, &document.text[from..from + len as usize], true);
            }
        }
    }
}

fn simple(
    context: &pango::Context,
    document: &OpDocument,
    block: &Block,
    style: &Style,
    width: f64,
) -> BlockLayout {
    let em_scale = block.kind.size_em();
    let size = style.body_px * em_scale;
    let mut compose = Compose::new();
    inline_into(&mut compose, document, block);

    let font = {
        let mut font = style.sized(em_scale, false);
        if let BlockKind::Heading(_) = block.kind {
            font.set_weight(pango::Weight::__Unknown(document::HEADING_WEIGHT));
        }
        font
    };
    let line_height = match block.kind {
        BlockKind::Heading(_) => document::HEADING_LINE_HEIGHT,
        _ => document::LINE_HEIGHT,
    };
    let piece = compose.finish(
        context,
        &font,
        style,
        size,
        line_height,
        Some(width),
        style.palette.text,
    );
    if let BlockKind::Heading(_) = block.kind {
        // letter-spacing: -0.025em
        let attributes = piece.layout.attributes().unwrap_or_default();
        let mut tracking = pango::AttrInt::new_letter_spacing(
            (size * document::HEADING_TRACKING_EM * pango::SCALE as f64) as i32,
        )
        .upcast();
        tracking.set_start_index(0);
        tracking.set_end_index(u32::MAX);
        attributes.insert(tracking);
        piece.layout.set_attributes(Some(&attributes));
    }

    let (_, logical) = piece.layout.pixel_extents();
    let mut content_height = logical.height() as f64;
    let mut decorations = Vec::new();
    // `h2 { border-bottom: 1px solid var(--border); padding-bottom: 0.35em }`
    if block.kind == BlockKind::Heading(2) {
        let gap = document::H2_RULE_GAP_EM * size;
        decorations.push(Decoration::Line {
            x: 0.0,
            y: content_height + gap,
            width,
            height: document::RULE_PX,
            color: style.palette.border,
        });
        content_height += gap + document::RULE_PX;
    }

    BlockLayout {
        pieces: vec![piece],
        decorations,
        space_before: 0.0,
        space_after: 0.0,
        content_height,
        content_width: width,
    }
}

fn rule(style: &Style, width: f64) -> BlockLayout {
    BlockLayout {
        pieces: Vec::new(),
        decorations: vec![Decoration::Line {
            x: 0.0,
            y: 0.0,
            width,
            height: document::RULE_PX,
            color: style.palette.border,
        }],
        space_before: 0.0,
        space_after: 0.0,
        content_height: document::RULE_PX,
        content_width: width,
    }
}

fn quote(
    context: &pango::Context,
    document: &OpDocument,
    block: &Block,
    style: &Style,
    width: f64,
) -> BlockLayout {
    let em = style.body_px;
    let pad_x = document::QUOTE_PAD_X_EM * em;
    let pad_y = document::QUOTE_PAD_Y_EM * em;
    let inner = (width - document::QUOTE_BAR_PX - pad_x).max(40.0);
    let mut compose = Compose::new();
    inline_into(&mut compose, document, block);
    let mut piece = compose.finish(
        context,
        &style.sized(1.0, false),
        style,
        em,
        document::LINE_HEIGHT,
        Some(inner),
        // `blockquote { color: var(--muted) }`
        style.palette.muted,
    );
    piece.x = document::QUOTE_BAR_PX + pad_x;
    piece.y = pad_y;
    let (_, logical) = piece.layout.pixel_extents();
    let content_height = logical.height() as f64 + 2.0 * pad_y;
    BlockLayout {
        // `border-left: 3px solid var(--accent)`
        decorations: vec![Decoration::Fill {
            x: 0.0,
            y: 0.0,
            width: document::QUOTE_BAR_PX,
            height: content_height,
            radius: 0.0,
            color: style.palette.accent,
        }],
        pieces: vec![piece],
        space_before: 0.0,
        space_after: 0.0,
        content_height,
        content_width: width,
    }
}

fn code(
    context: &pango::Context,
    document: &OpDocument,
    block: &Block,
    style: &Style,
    width: f64,
) -> BlockLayout {
    let mut compose = Compose::new();
    // A fenced block's content ends in the newline that closed its last line;
    // setting it would put an empty line under every code block.
    let mut runs: Vec<(u32, &str)> = Vec::new();
    for op in ops_of(document, block) {
        if let Op::Text { offset, len } = op {
            let from = offset as usize;
            runs.push((offset, &document.text[from..from + len as usize]));
        }
    }
    if let Some(last) = runs.last_mut() {
        if let Some(trimmed) = last.1.strip_suffix('\n') {
            last.1 = trimmed;
        }
    }
    for (offset, value) in runs {
        if !value.is_empty() {
            // Code keeps every newline: they are the block's lines.
            compose.document(offset, value, false);
        }
    }
    let size = style.body_px * document::INLINE_CODE_EM;
    // No width: a code block does not wrap, it scrolls inside its own block.
    let mut piece = compose.finish(
        context,
        &style.sized(document::INLINE_CODE_EM, true),
        style,
        size,
        document::CODE_LINE_HEIGHT,
        None,
        style.palette.text,
    );
    piece.x = document::CODE_PAD_X;
    piece.y = document::CODE_PAD_TOP;
    let (_, logical) = piece.layout.pixel_extents();
    let content_height =
        document::CODE_PAD_TOP + logical.height() as f64 + document::CODE_PAD_BOTTOM;
    let content_width = (logical.width() as f64 + 2.0 * document::CODE_PAD_X).max(width);
    BlockLayout {
        decorations: vec![
            Decoration::Fill {
                x: 0.0,
                y: 0.0,
                width,
                height: content_height,
                radius: crate::theme::RADIUS,
                color: style.palette.code,
            },
            Decoration::Line {
                x: 0.0,
                y: 0.0,
                width,
                height: document::CELL_BORDER_PX,
                color: style.palette.border,
            },
            Decoration::Line {
                x: 0.0,
                y: content_height - document::CELL_BORDER_PX,
                width,
                height: document::CELL_BORDER_PX,
                color: style.palette.border,
            },
        ],
        pieces: vec![piece],
        space_before: 0.0,
        space_after: 0.0,
        content_height,
        content_width,
    }
}

/// One open list level and its counter.
struct Level {
    ordered: bool,
    next: u64,
}

/// What is currently open, so that a CLOSE is matched to the thing it closes
/// rather than guessed at from what happens to be on another stack.
enum Frame {
    List,
    Item,
    Span,
    Void,
}

struct Item {
    compose: Compose,
    depth: usize,
    task: Option<bool>,
    marker: String,
}

fn marker_for(ordered: bool, number: u64, depth: usize, task: Option<bool>) -> String {
    if let Some(checked) = task {
        // The reference uses a real checkbox input; a viewer that never writes
        // renders its state instead (SPEC.md, section 2: task lists are
        // read-only).
        return if checked {
            "☑  ".into()
        } else {
            "☐  ".into()
        };
    }
    if ordered {
        return format!("{number}.  ");
    }
    match depth {
        0 | 1 => "•  ".into(),
        2 => "◦  ".into(),
        _ => "▪  ".into(),
    }
}

fn list(
    context: &pango::Context,
    document: &OpDocument,
    block: &Block,
    style: &Style,
    width: f64,
) -> BlockLayout {
    let em = style.body_px;
    let indent = document::LIST_INDENT_EM * em;
    let mut levels: Vec<Level> = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    let mut open_items: Vec<usize> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();

    for op in ops_of(document, block) {
        match op {
            Op::Open { tag, attrs, count } => match tag {
                TAG_UL | TAG_OL => {
                    levels.push(Level {
                        ordered: tag == TAG_OL,
                        next: attribute(document, attrs, count, ATTR_START)
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(1),
                    });
                    stack.push(Frame::List);
                }
                TAG_LI => {
                    items.push(Item {
                        compose: Compose::new(),
                        depth: levels.len(),
                        task: None,
                        marker: String::new(),
                    });
                    open_items.push(items.len() - 1);
                    stack.push(Frame::Item);
                }
                TAG_INPUT => {
                    let checked = attribute(document, attrs, count, ATTR_CHECKED).is_some();
                    if let Some(&index) = open_items.last() {
                        items[index].task = Some(checked);
                    }
                    stack.push(Frame::Void);
                }
                TAG_BR => {
                    if let Some(&index) = open_items.last() {
                        items[index].compose.after_break();
                    }
                    stack.push(Frame::Void);
                }
                _ => {
                    if let Some(&index) = open_items.last() {
                        items[index].compose.open_span(
                            tag,
                            has_class(document, attrs, count, "raw-html"),
                            attribute(document, attrs, count, ATTR_HREF).map(str::to_owned),
                        );
                        stack.push(Frame::Span);
                    } else {
                        stack.push(Frame::Void);
                    }
                }
            },
            Op::Close => match stack.pop() {
                Some(Frame::List) => {
                    levels.pop();
                }
                Some(Frame::Item) => {
                    if let Some(index) = open_items.pop() {
                        let (ordered, number) = levels
                            .last()
                            .map(|level| (level.ordered, level.next))
                            .unwrap_or((false, 1));
                        let depth = items[index].depth;
                        let task = items[index].task;
                        items[index].marker = marker_for(ordered, number, depth, task);
                        if let Some(level) = levels.last_mut() {
                            level.next += 1;
                        }
                    }
                }
                Some(Frame::Span) => {
                    if let Some(&index) = open_items.last() {
                        items[index].compose.close_span();
                    }
                }
                _ => {}
            },
            Op::Text { offset, len } => {
                if let Some(&index) = open_items.last() {
                    let from = offset as usize;
                    items[index].compose.document(
                        offset,
                        &document.text[from..from + len as usize],
                        true,
                    );
                }
            }
        }
    }

    let font = style.sized(1.0, false);
    let mut pieces = Vec::new();
    let mut y = 0.0;
    let item_gap = document::LIST_ITEM_SPACING_EM * em;
    // Items were created when their `li` opened, so they are already in
    // document order — including nested ones, which sit between their
    // siblings exactly where they belong.
    for mut item in items {
        if item.compose.is_empty() && item.task.is_none() {
            continue;
        }
        // The marker hangs to the left of the text, so wrapped lines align
        // with the item's first character rather than with its bullet.
        let hang = measure_width(context, &font, &item.marker);
        item.compose.prepend(&item.marker);
        let left = item.depth as f64 * indent;
        let available = (width - left).max(60.0);
        let mut piece = item.compose.finish(
            context,
            &font,
            style,
            em,
            document::LINE_HEIGHT,
            Some(available),
            style.palette.text,
        );
        piece
            .layout
            .set_indent(-((hang * pango::SCALE as f64) as i32));
        piece.x = left + hang;
        piece.y = y;
        let (_, logical) = piece.layout.pixel_extents();
        y += logical.height() as f64 + item_gap;
        pieces.push(piece);
    }
    let content_height = (y - item_gap).max(0.0);
    BlockLayout {
        pieces,
        decorations: Vec::new(),
        space_before: 0.0,
        space_after: 0.0,
        content_height,
        content_width: width,
    }
}

fn measure_width(context: &pango::Context, font: &pango::FontDescription, text: &str) -> f64 {
    let layout = pango::Layout::new(context);
    layout.set_font_description(Some(font));
    layout.set_text(text);
    let (_, logical) = layout.pixel_extents();
    logical.width() as f64
}

fn table(
    context: &pango::Context,
    document: &OpDocument,
    block: &Block,
    style: &Style,
    width: f64,
) -> BlockLayout {
    let em = style.body_px * document::TABLE_EM;
    let pad_x = document::CELL_PAD_X_EM * em;
    let pad_y = document::CELL_PAD_Y_EM * em;
    let border = document::CELL_BORDER_PX;

    // Collect the cells row by row.
    struct Cell {
        compose: Compose,
        header: bool,
    }
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut header_rows = 0usize;
    let mut in_cell = false;
    for op in ops_of(document, block) {
        match op {
            Op::Open { tag, attrs, count } => match tag {
                TAG_TABLE => {}
                TAG_TR => rows.push(Vec::new()),
                TAG_TH | TAG_TD => {
                    in_cell = true;
                    if let Some(row) = rows.last_mut() {
                        row.push(Cell {
                            compose: Compose::new(),
                            header: tag == TAG_TH,
                        });
                        if tag == TAG_TH && header_rows < rows.len() {
                            header_rows = rows.len();
                        }
                    }
                }
                _ => {
                    if in_cell {
                        if let Some(cell) = rows.last_mut().and_then(|row| row.last_mut()) {
                            cell.compose.open_span(
                                tag,
                                has_class(document, attrs, count, "raw-html"),
                                attribute(document, attrs, count, ATTR_HREF).map(str::to_owned),
                            );
                        }
                    }
                }
            },
            Op::Close => {
                if let Some(cell) = rows.last_mut().and_then(|row| row.last_mut()) {
                    if !cell.compose.open.is_empty() {
                        cell.compose.close_span();
                        continue;
                    }
                }
                in_cell = false;
            }
            Op::Text { offset, len } => {
                if in_cell {
                    if let Some(cell) = rows.last_mut().and_then(|row| row.last_mut()) {
                        let from = offset as usize;
                        cell.compose.document(
                            offset,
                            &document.text[from..from + len as usize],
                            true,
                        );
                    }
                }
            }
        }
    }
    rows.retain(|row| !row.is_empty());
    if rows.is_empty() {
        return BlockLayout {
            pieces: Vec::new(),
            decorations: Vec::new(),
            space_before: 0.0,
            space_after: 0.0,
            content_height: 0.0,
            content_width: width,
        };
    }

    let font = style.sized(document::TABLE_EM, false);
    let bold = {
        let mut font = font.clone();
        font.set_weight(pango::Weight::Semibold);
        font
    };
    let columns = rows.iter().map(|row| row.len()).max().unwrap_or(0);

    // Natural width of each column, then a fair share of what is available.
    let mut natural = vec![0.0f64; columns];
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            let want = measure_width(
                context,
                if cell.header { &bold } else { &font },
                &cell.compose.text,
            );
            natural[index] = natural[index].max(want + 2.0 * pad_x);
        }
    }
    let total: f64 = natural.iter().sum();
    let available = width - border * (columns as f64 + 1.0);
    let widths: Vec<f64> = if total <= available && total > 0.0 {
        // Grow to fill the column, keeping the proportions.
        natural
            .iter()
            .map(|value| value * available / total)
            .collect()
    } else {
        natural
    };
    let content_width = widths.iter().sum::<f64>() + border * (columns as f64 + 1.0);

    let mut pieces = Vec::new();
    let mut decorations = Vec::new();
    let mut y = 0.0;
    for (row_index, row) in rows.into_iter().enumerate() {
        let is_header = row_index < header_rows;
        let mut laid: Vec<(Piece, f64)> = Vec::new();
        let mut height: f64 = 0.0;
        let mut x = border;
        for (index, cell) in row.into_iter().enumerate() {
            let column = widths.get(index).copied().unwrap_or(0.0);
            let inner = (column - 2.0 * pad_x).max(20.0);
            let mut piece = cell.compose.finish(
                context,
                if cell.header { &bold } else { &font },
                style,
                em,
                document::LINE_HEIGHT,
                Some(inner),
                style.palette.text,
            );
            piece.x = x + pad_x;
            let (_, logical) = piece.layout.pixel_extents();
            height = height.max(logical.height() as f64);
            laid.push((piece, column));
            x += column + border;
        }
        let row_height = height + 2.0 * pad_y + border;
        if is_header {
            decorations.push(Decoration::Fill {
                x: 0.0,
                y,
                width: content_width,
                height: row_height,
                radius: 0.0,
                color: style.palette.subtle,
            });
        }
        // Cell borders: one line per edge, drawn as thin fills.
        let mut x = 0.0;
        for (_, column) in &laid {
            decorations.push(Decoration::Line {
                x,
                y,
                width: border,
                height: row_height,
                color: style.palette.border,
            });
            x += column + border;
        }
        decorations.push(Decoration::Line {
            x,
            y,
            width: border,
            height: row_height,
            color: style.palette.border,
        });
        decorations.push(Decoration::Line {
            x: 0.0,
            y,
            width: content_width,
            height: border,
            color: style.palette.border,
        });
        for (mut piece, _) in laid {
            piece.y = y + border + pad_y;
            pieces.push(piece);
        }
        y += row_height;
    }
    decorations.push(Decoration::Line {
        x: 0.0,
        y,
        width: content_width,
        height: border,
        color: style.palette.border,
    });
    let content_height = y + border;

    BlockLayout {
        pieces,
        decorations,
        space_before: 0.0,
        space_after: 0.0,
        content_height,
        content_width,
    }
}

/// A paragraph whose whole content is one picture becomes a picture block.
///
/// That is how pictures appear in practice — on a line of their own. A picture
/// *inside* running text would have to be shaped into the line, which Pango
/// can do but the offset mapping cannot yet follow; those still show their
/// alternative text, which is honest and readable.
fn picture(
    context: &pango::Context,
    document: &OpDocument,
    block: &Block,
    style: &Style,
    width: f64,
    images: &dyn ImageSource,
) -> Option<BlockLayout> {
    if block.kind != BlockKind::Paragraph {
        return None;
    }
    let mut source = None;
    let mut alt = String::new();
    let mut elements = 0;
    for op in ops_of(document, block) {
        match op {
            Op::Open { tag, attrs, count } => {
                elements += 1;
                if tag == TAG_IMG {
                    source = attribute(document, attrs, count, ATTR_SRC).map(str::to_owned);
                    alt = attribute(document, attrs, count, ATTR_ALT)
                        .unwrap_or_default()
                        .to_owned();
                } else if tag != 0 {
                    return None;
                }
            }
            // Any text alongside the picture makes this ordinary running text.
            Op::Text { len, .. } if len > 0 => return None,
            _ => {}
        }
    }
    let source = source?;
    if elements != 2 {
        return None;
    }

    match images.intrinsic(&source) {
        Some((natural_width, natural_height)) => {
            // `img { max-width: 100% }`: never wider than the reading column,
            // never enlarged beyond its own size.
            let scale = (width / natural_width).min(1.0);
            let height = natural_height * scale;
            let shown = natural_width * scale;
            Some(BlockLayout {
                pieces: Vec::new(),
                decorations: vec![Decoration::Image {
                    x: 0.0,
                    y: 0.0,
                    width: shown,
                    height,
                    source,
                }],
                space_before: 0.0,
                space_after: 0.0,
                content_height: height,
                content_width: width,
            })
        }
        None => Some(placeholder(context, style, width, &alt, &source)),
    }
}

/// `.image-placeholder`: an unobtrusive box with the alternative text, in the
/// height the layout had reserved anyway (SPEC.md, section 3).
fn placeholder(
    context: &pango::Context,
    style: &Style,
    width: f64,
    alt: &str,
    source: &str,
) -> BlockLayout {
    let pad = 10.0;
    let em = style.body_px * 0.8;
    let mut compose = Compose::new();
    let label = if alt.is_empty() {
        source.to_string()
    } else {
        format!("{alt} — {source}")
    };
    compose.insert(&label);
    let mut font = style.body.clone();
    font.set_absolute_size(em * pango::SCALE as f64);
    let mut piece = compose.finish(
        context,
        &font,
        style,
        em,
        document::LINE_HEIGHT,
        Some(width - 2.0 * pad),
        style.palette.muted,
    );
    piece.x = pad;
    piece.y = pad;
    let (_, logical) = piece.layout.pixel_extents();
    let content_height = (logical.height() as f64 + 2.0 * pad).max(42.0);
    BlockLayout {
        decorations: vec![
            Decoration::Line {
                x: 0.0,
                y: 0.0,
                width,
                height: document::CELL_BORDER_PX,
                color: style.palette.border,
            },
            Decoration::Line {
                x: 0.0,
                y: content_height - document::CELL_BORDER_PX,
                width,
                height: document::CELL_BORDER_PX,
                color: style.palette.border,
            },
        ],
        pieces: vec![piece],
        space_before: 0.0,
        space_after: 0.0,
        content_height,
        content_width: width,
    }
}
