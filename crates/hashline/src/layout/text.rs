//! Setting one block with Pango.
//!
//! This is the only place that turns operations into type (SPEC.md, section 6,
//! "Datenfluss", step 6). The important decision here is what the layout's text
//! is: it is **exactly** the block's slice of the document text blob, byte for
//! byte. Nothing is inserted and nothing is dropped.
//!
//! That keeps the mapping between a position in the layout and a position in
//! the document trivial — `layout_offset + block.text_start` — which is what
//! makes selection across block boundaries, search highlighting and hit-testing
//! agree with one another instead of each carrying its own translation table.

use hashline_markdown::{OpDocument, ALLOWED_ATTR, OP_CLOSE, OP_OPEN, OP_TEXT};
use pango::prelude::*;

use crate::layout::{Block, BlockKind};
use crate::theme::{document, Palette};

/// Tag ids for the inline elements that carry type, as indices into
/// `ALLOWED_TAGS`.
const TAG_EM: u32 = 7;
const TAG_STRONG: u32 = 8;
const TAG_DEL: u32 = 9;
const TAG_S: u32 = 10;
const TAG_CODE: u32 = 11;
const TAG_A: u32 = 19;

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

    /// The font a block is set in, before inline attributes refine it.
    fn font_for(&self, kind: BlockKind) -> pango::FontDescription {
        let mut font = match kind {
            BlockKind::Code => self.mono.clone(),
            _ => self.body.clone(),
        };
        font.set_absolute_size(self.body_px * kind.size_em() * pango::SCALE as f64);
        if let BlockKind::Heading(_) = kind {
            font.set_weight(pango::Weight::__Unknown(document::HEADING_WEIGHT));
        }
        font
    }
}

/// A block that has been set: the Pango layout plus what it took to place it.
pub struct BlockLayout {
    pub layout: pango::Layout,
    /// Where the layout's text starts in the document text blob, so that a
    /// byte offset in the layout is `text_start + offset` in the document.
    pub text_start: u32,
    /// Space above the block, already excluded from the layout's own height.
    pub space_before: f64,
    pub space_after: f64,
}

impl BlockLayout {
    /// Total height the block occupies, spacing included.
    pub fn height(&self) -> f64 {
        let (_, logical) = self.layout.pixel_extents();
        self.space_before + logical.height() as f64 + self.space_after
    }
    /// Where the type itself begins, relative to the block's top edge.
    pub fn baseline_offset(&self) -> f64 {
        self.space_before
    }
}

/// Sets one block. `width` is the reading column in logical pixels.
pub fn set_block(
    context: &pango::Context,
    document: &OpDocument,
    block: &Block,
    style: &Style,
    width: f64,
) -> BlockLayout {
    let layout = pango::Layout::new(context);
    let start = block.text_start as usize;
    let text = &document.text[start..start + block.text_len as usize];
    // A fenced block's content ends in the newline that closed its last line.
    // Setting it would add an empty line under every code block. Dropping it
    // keeps the offset mapping intact — it only shortens the range at the end,
    // and there is nothing there to select.
    let text = text.strip_suffix('\n').unwrap_or(text);
    layout.set_text(text);
    layout.set_font_description(Some(&style.font_for(block.kind)));

    match block.kind {
        // A code block does not wrap; it scrolls inside its own block, so the
        // document itself never scrolls horizontally (SPEC.md, section 3).
        BlockKind::Code => layout.set_width(-1),
        _ => {
            layout.set_width((width * pango::SCALE as f64) as i32);
            layout.set_wrap(pango::WrapMode::WordChar);
        }
    }

    let attributes = inline_attributes(document, block, style);

    // Line height is set absolutely, not through `set_line_spacing`. That
    // function multiplies the font's *natural* line spacing, which already
    // contains the face's leading, so a factor of 1.65 lands near 2.0 and the
    // text reads far too airy. CSS `line-height` — which the design reference
    // is written in — is a multiple of the *font size*, and this is how to say
    // that in Pango.
    let factor = match block.kind {
        BlockKind::Heading(_) => document::HEADING_LINE_HEIGHT,
        _ => document::LINE_HEIGHT,
    };
    let size = style.body_px * block.kind.size_em();
    let mut line_height =
        pango::AttrInt::new_line_height_absolute((size * factor * pango::SCALE as f64) as i32)
            .upcast();
    line_height.set_start_index(0);
    line_height.set_end_index(text.len() as u32);
    attributes.insert(line_height);

    layout.set_attributes(Some(&attributes));

    let em = style.body_px;
    BlockLayout {
        layout,
        text_start: block.text_start,
        space_before: space_before_em(block.kind) * em,
        space_after: space_after_em(block.kind) * em,
    }
}

fn space_before_em(kind: BlockKind) -> f64 {
    match kind {
        BlockKind::Heading(_) => document::HEADING_SPACE_BEFORE_EM,
        _ => 0.0,
    }
}

fn space_after_em(kind: BlockKind) -> f64 {
    match kind {
        BlockKind::Heading(_) => document::HEADING_SPACE_AFTER_EM,
        BlockKind::Rule => document::RULE_SPACING_EM,
        _ => document::BLOCK_SPACING_EM,
    }
}

/// One open inline element and the span of layout text it has covered so far.
struct Span {
    tag: u32,
    /// Class attribute, when the element carried one. Raw HTML is marked this
    /// way and is set apart from the author's own code.
    raw_html: bool,
    start: Option<u32>,
    end: u32,
}

/// Walks the block's operations and turns the inline elements into Pango
/// attributes over the layout's text.
fn inline_attributes(document: &OpDocument, block: &Block, style: &Style) -> pango::AttrList {
    let attributes = pango::AttrList::new();
    let mut open: Vec<Span> = Vec::new();
    let base = block.text_start;

    let first = block.op_start as usize * 4;
    let last = first + block.op_count as usize * 4;
    let mut cursor = first;
    while cursor < last {
        let op = &document.ops[cursor..cursor + 4];
        cursor += 4;
        match op[0] {
            OP_OPEN => open.push(Span {
                tag: op[1],
                raw_html: has_raw_html_class(document, op[2], op[3]),
                start: None,
                end: 0,
            }),
            OP_TEXT => {
                let from = op[1].saturating_sub(base);
                let to = from + op[2];
                for span in open.iter_mut() {
                    span.start.get_or_insert(from);
                    span.end = to;
                }
            }
            OP_CLOSE => {
                if let Some(span) = open.pop() {
                    if let Some(from) = span.start {
                        push_attributes(&attributes, &span, from, span.end, style);
                    }
                }
            }
            _ => {}
        }
    }
    attributes
}

fn has_raw_html_class(document: &OpDocument, attr_start: u32, count: u32) -> bool {
    (0..count as usize).any(|index| {
        let at = (attr_start as usize + index) * 3;
        let name = ALLOWED_ATTR[document.attrs[at] as usize];
        if name != "class" {
            return false;
        }
        let from = document.attrs[at + 1] as usize;
        let to = from + document.attrs[at + 2] as usize;
        &document.strings[from..to] == "raw-html"
    })
}

fn push_attributes(attributes: &pango::AttrList, span: &Span, from: u32, to: u32, style: &Style) {
    let add = |attribute: pango::Attribute| {
        let mut attribute = attribute;
        attribute.set_start_index(from);
        attribute.set_end_index(to);
        attributes.insert(attribute);
    };
    let colour = |c: crate::theme::Color| {
        pango::AttrColor::new_foreground(
            (c.red * 65535.0) as u16,
            (c.green * 65535.0) as u16,
            (c.blue * 65535.0) as u16,
        )
        .upcast()
    };
    match span.tag {
        TAG_EM => add(pango::AttrInt::new_style(pango::Style::Italic).upcast()),
        TAG_STRONG => add(pango::AttrInt::new_weight(pango::Weight::Bold).upcast()),
        TAG_DEL | TAG_S => add(pango::AttrInt::new_strikethrough(true).upcast()),
        TAG_CODE => {
            add(pango::AttrFontDesc::new(&style.mono).upcast());
            // Raw HTML is source text, not the author's code: it is set apart
            // more quietly (SPEC.md, section 6).
            add(colour(if span.raw_html {
                style.palette.muted
            } else {
                style.palette.text
            }));
        }
        TAG_A => {
            add(colour(style.palette.accent));
            add(pango::AttrInt::new_underline(pango::Underline::Single).upcast());
        }
        _ => {}
    }
}
