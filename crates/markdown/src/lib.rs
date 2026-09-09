//! Hashline's Markdown parser.
//!
//! It reads Markdown once and emits the op buffer the renderer replays
//! (SPEC.md, section 6). No HTML string is
//! produced at all: the native renderer has no HTML parser and no sanitizer, so
//! raw HTML is shown as source text instead of being interpreted
//! (SPEC.md, section 6). That removes DOMPurify and the whole class of
//! sanitization bugs with it.
//!
//! The crate stays free of toolkit and platform dependencies
//! (docs/decisions/008-parser-reference.md).

mod opbuffer;
mod slug;

use opbuffer::*;
use pulldown_cmark::{
    Alignment, CodeBlockKind, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd,
};
use std::collections::HashMap;

// The op vocabulary is the contract between parser and renderer
// (SPEC.md, section 6). The native view decodes the buffer with these.
pub use opbuffer::{
    read_varint, ALLOWED_ATTR, ALLOWED_TAGS, BLOCK_WORDS, OP_ATTRS, OP_CLOSE, OP_TAG_MASK, OP_TEXT,
};
pub use slug::slug_base;

/// The GFM set Hashline supports, unchanged from the marked configuration it
/// replaces: tables, strikethrough, task lists and footnotes.
pub fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
}

/// Sections aim for the same 16 KiB of rendered markup the marked pipeline
/// used, so section counts and the measurement series stay comparable.
const SECTION_BUDGET: usize = 16_384;

/// Words per section: opStart, opCount, hashLow, hashHigh, textStart, textLen.
pub const SECTION_WORDS: usize = 6;
pub const HEADING_WORDS: usize = 6;

#[derive(Default)]
pub struct OpDocument {
    /// The operation stream (see `opbuffer`), a byte encoding.
    pub ops: Vec<u8>,
    /// Attribute values, heading ids and heading texts.
    pub strings: String,
    /// The document's text in document order, with a separator between blocks.
    /// TEXT operations index into this, and the search runs on it directly
    /// (SPEC.md, section 8).
    pub text: String,
    /// Top-level flow elements, `BLOCK_WORDS` words each: tag, opStart,
    /// opCount, textStart, textLen. The block plan is built straight from this
    /// (SPEC.md, section 5), and the per-block text range is what maps a search
    /// hit and a selection position onto a block (SPEC.md, section 8).
    pub blocks: Vec<u32>,
    /// `SECTION_WORDS` words per section.
    pub sections: Vec<u32>,
    /// 6 words per heading: level, idOffset, idLen, sectionIndex, textOffset,
    /// textLen.
    pub headings: Vec<u32>,
    /// Whether the document contained raw HTML shown as source text. The view
    /// uses it for the single quiet notice SPEC.md, section 6 asks for.
    pub raw_html: bool,
}

struct Heading {
    level: u32,
    id: String,
    text: String,
    section: u32,
}

/// Turns one section's events into operations. Everything that needs
/// document-wide state — heading slugs, footnote numbers, the string blob —
/// lives here rather than in the per-section pass.
struct Builder {
    enc: Encoder,
    slugs: slug::Slugs,
    sections: Vec<u32>,
    headings: Vec<Heading>,
    footnotes: HashMap<String, usize>,
    aligns: Vec<Alignment>,
    cell: usize,
    in_head: bool,
    image: Option<(String, String, String)>,
    image_depth: usize,
    /// Raw HTML of the block being collected, shown as source text on end.
    html_block: Option<String>,
    raw_html: bool,
}

fn tag_id(name: &str) -> u32 {
    ALLOWED_TAGS.iter().position(|&t| t == name).unwrap() as u32
}

// The character set CommonMark's reference implementation leaves unescaped in
// URLs. Without this the buffer would carry raw spaces, backslashes and
// non-ASCII bytes where the specification — and marked's `encodeURI` before it —
// produce percent escapes.
const HREF_SAFE: &[u8] = b"-_.+!*'(),%#@?=;:/&$~";

fn escape_href(url: &str) -> String {
    if url
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || HREF_SAFE.contains(&b))
    {
        return url.to_owned();
    }
    let mut out = String::with_capacity(url.len() + 8);
    for &byte in url.as_bytes() {
        if byte.is_ascii_alphanumeric() || HREF_SAFE.contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

impl Builder {
    fn new() -> Self {
        Builder {
            enc: Encoder::default(),
            slugs: slug::Slugs::default(),
            sections: Vec::new(),
            headings: Vec::new(),
            footnotes: HashMap::new(),
            aligns: Vec::new(),
            cell: 0,
            in_head: false,
            image: None,
            image_depth: 0,
            html_block: None,
            raw_html: false,
        }
    }
    fn open(&mut self, tag: u32) {
        let open = self.enc.open(tag);
        self.enc.opened(open);
    }
    fn open_with(&mut self, tag: u32, attributes: &[(u32, &str)]) {
        let open = self.enc.open(tag);
        for (name, value) in attributes {
            self.enc.attribute(&open, *name, value);
        }
        self.enc.opened(open);
    }
    fn void(&mut self, tag: u32, attributes: &[(u32, &str)]) {
        self.open_with(tag, attributes);
        self.enc.close();
    }
    fn slugs_id(&mut self, text: &str) -> String {
        self.slugs.id(text)
    }
    fn footnote_number(&mut self, name: &str) -> usize {
        let next = self.footnotes.len() + 1;
        *self.footnotes.entry(name.to_owned()).or_insert(next)
    }
    fn text(&mut self, value: &str) {
        match self.image.as_mut() {
            Some(image) => image.2.push_str(value),
            None => self.enc.text(value),
        }
    }
    /// Raw HTML is not expressible as operations and is not interpreted. It is
    /// shown verbatim, monospace and set apart, exactly like a code block
    /// (SPEC.md, section 6). The class is what lets the view mark it as source
    /// rather than as the author's own code.
    fn raw_block(&mut self, value: &str) {
        let value = value.trim_end_matches('\n');
        if value.is_empty() {
            return;
        }
        self.raw_html = true;
        self.open(TAG_PRE);
        self.open_with(TAG_CODE, &[(ATTR_CLASS, "raw-html")]);
        self.text(value);
        self.enc.close();
        self.enc.close();
    }
    fn raw_inline(&mut self, value: &str) {
        self.raw_html = true;
        self.open_with(TAG_CODE, &[(ATTR_CLASS, "raw-html")]);
        self.text(value);
        self.enc.close();
    }

    fn start(&mut self, tag: &Tag<'_>) {
        match tag {
            Tag::Paragraph => self.open(TAG_P),
            Tag::Heading { level, id, .. } => {
                let heading = tag_id(&format!("h{}", *level as u8));
                match id {
                    Some(value) if !value.is_empty() => {
                        self.open_with(heading, &[(ATTR_ID, value)])
                    }
                    _ => self.open(heading),
                }
            }
            // The alert classes the html renderer would add for GitHub alerts
            // are not in the attribute allowlist; the quote stays a quote.
            Tag::BlockQuote(_) => self.open(TAG_BLOCKQUOTE),
            Tag::CodeBlock(kind) => {
                self.open(TAG_PRE);
                match kind {
                    CodeBlockKind::Fenced(info) if !info.is_empty() => {
                        let language = info.split(' ').next().unwrap_or_default();
                        let class = format!("language-{language}");
                        self.open_with(TAG_CODE, &[(ATTR_CLASS, &class)]);
                    }
                    _ => self.open(TAG_CODE),
                }
            }
            Tag::List(Some(start)) if *start != 1 => {
                let value = start.to_string();
                self.open_with(TAG_OL, &[(ATTR_START, &value)]);
            }
            Tag::List(Some(_)) => self.open(TAG_OL),
            Tag::List(None) => self.open(TAG_UL),
            Tag::Item => self.open(TAG_LI),
            Tag::FootnoteDefinition(name) => {
                let number = self.footnote_number(name).to_string();
                self.open_with(
                    TAG_DIV,
                    &[(ATTR_CLASS, "footnote-definition"), (ATTR_ID, name)],
                );
                self.open_with(TAG_SUP, &[(ATTR_CLASS, "footnote-definition-label")]);
                self.text(&number);
                self.enc.close();
            }
            Tag::Table(aligns) => {
                self.aligns = aligns.clone();
                self.open(TAG_TABLE);
            }
            Tag::TableHead => {
                self.in_head = true;
                self.cell = 0;
                self.open(TAG_THEAD);
                self.open(TAG_TR);
            }
            Tag::TableRow => {
                self.cell = 0;
                self.open(TAG_TR);
            }
            Tag::TableCell => {
                // The html renderer emits `style="text-align: …"`, which is not
                // in the attribute allowlist. `align` carries the same
                // information (docs/decisions/008, section 7).
                let tag = if self.in_head { TAG_TH } else { TAG_TD };
                let align = match self.aligns.get(self.cell) {
                    Some(Alignment::Left) => Some("left"),
                    Some(Alignment::Center) => Some("center"),
                    Some(Alignment::Right) => Some("right"),
                    _ => None,
                };
                self.cell += 1;
                match align {
                    Some(value) => self.open_with(tag, &[(ATTR_ALIGN, value)]),
                    None => self.open(tag),
                }
            }
            Tag::Emphasis => self.open(TAG_EM),
            Tag::Strong => self.open(TAG_STRONG),
            Tag::Strikethrough => self.open(TAG_DEL),
            Tag::Link {
                link_type,
                dest_url,
                title,
                ..
            } => {
                // Email autolinks carry the address without its scheme.
                let href = match link_type {
                    LinkType::Email => format!("mailto:{}", escape_href(dest_url)),
                    _ => escape_href(dest_url),
                };
                let mut attributes = vec![(ATTR_HREF, href.as_str())];
                if !title.is_empty() {
                    attributes.push((ATTR_TITLE, title.as_ref()));
                }
                self.open_with(TAG_A, &attributes);
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                self.image = Some((
                    escape_href(dest_url),
                    title.as_ref().to_owned(),
                    String::new(),
                ));
                self.image_depth += 1;
            }
            Tag::HtmlBlock => self.html_block = Some(String::new()),
            // Unreachable with `options()`; refusing beats guessing.
            _ => self.open(TAG_P),
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::CodeBlock | TagEnd::Table => {
                self.enc.close();
                self.enc.close();
            }
            TagEnd::TableHead => {
                self.enc.close();
                self.enc.close();
                self.in_head = false;
                self.open(TAG_TBODY);
            }
            TagEnd::Image => {
                self.image_depth -= 1;
                if self.image_depth > 0 {
                    return;
                }
                if let Some((source, title, alt)) = self.image.take() {
                    let mut attributes =
                        vec![(ATTR_SRC, source.as_str()), (ATTR_ALT, alt.as_str())];
                    if !title.is_empty() {
                        attributes.push((ATTR_TITLE, title.as_str()));
                    }
                    self.void(TAG_IMG, &attributes);
                }
            }
            TagEnd::HtmlBlock => {
                if let Some(html) = self.html_block.take() {
                    self.raw_block(&html);
                }
            }
            _ => self.enc.close(),
        }
    }

    fn event(&mut self, event: &Event<'_>) {
        // Inside an image only the alternative text is collected: nested links
        // and emphasis contribute their text, never elements of their own.
        if self.image_depth > 0 {
            match event {
                Event::Start(Tag::Image { .. }) => {
                    self.image_depth += 1;
                    return;
                }
                Event::End(TagEnd::Image) => {}
                Event::Text(text) | Event::Code(text) => {
                    self.text(text);
                    return;
                }
                Event::SoftBreak | Event::HardBreak => {
                    self.text("\n");
                    return;
                }
                _ => return,
            }
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(*tag),
            Event::Text(text) => self.text(text),
            Event::Code(text) => {
                self.open(TAG_CODE);
                self.text(text);
                self.enc.close();
            }
            Event::SoftBreak => self.text("\n"),
            Event::HardBreak => {
                // The line break the html renderer writes after `<br>` is part
                // of the specification's output and of the document's text.
                self.void(TAG_BR, &[]);
                self.text("\n");
            }
            Event::Rule => self.void(TAG_HR, &[]),
            Event::TaskListMarker(checked) => {
                let mut attributes = vec![(ATTR_DISABLED, ""), (ATTR_TYPE, "checkbox")];
                if *checked {
                    attributes.push((ATTR_CHECKED, ""));
                }
                self.void(TAG_INPUT, &attributes);
            }
            Event::FootnoteReference(name) => {
                let number = self.footnote_number(name).to_string();
                let href = format!("#{name}");
                self.open_with(TAG_SUP, &[(ATTR_CLASS, "footnote-reference")]);
                self.open_with(TAG_A, &[(ATTR_HREF, &href)]);
                self.text(&number);
                self.enc.close();
                self.enc.close();
            }
            Event::Html(html) => match self.html_block.as_mut() {
                Some(buffer) => buffer.push_str(html),
                // Raw HTML outside a block wrapper still gets its own block.
                None => self.raw_block(html),
            },
            Event::InlineHtml(html) => self.raw_inline(html),
            _ => {}
        }
    }
}

fn estimate(event: &Event<'_>) -> usize {
    match event {
        Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
            text.len()
        }
        Event::Start(_) | Event::End(_) => 8,
        _ => 4,
    }
}

/// Heading text the way marked's text renderer produced it: element content
/// without markup, image alternative text included, raw inline HTML dropped.
fn heading_text(events: &[Event<'_>]) -> String {
    let mut text = String::new();
    for event in events {
        match event {
            Event::Text(value) | Event::Code(value) => text.push_str(value),
            Event::SoftBreak => text.push('\n'),
            _ => {}
        }
    }
    text
}

/// Parses `source` into the op buffer the renderer replays.
pub fn parse(source: &str) -> OpDocument {
    let mut builder = Builder::new();
    let mut events: Vec<Event<'_>> = Vec::new();
    let mut depth = 0usize;
    let mut size = 0usize;
    let mut heading_start: Option<usize> = None;

    // Raw HTML no longer constrains where a section may end: nothing downstream
    // parses HTML, so no boundary can change a tree. The pre-pass that decided
    // this — a second walk over the whole document — is gone with it.
    let flush = |builder: &mut Builder, events: &mut Vec<Event<'_>>| {
        if events.is_empty() {
            return;
        }
        let op_start = builder.enc.ops.len() as u32;
        let text_start = builder.enc.text_len();
        for event in events.iter() {
            builder.event(event);
        }
        events.clear();
        let (ops_end, hash) = builder.enc.end_section();
        builder.sections.extend_from_slice(&[
            op_start,
            ops_end as u32 - op_start,
            hash as u32,
            (hash >> 32) as u32,
            text_start,
            builder.enc.text_len() - text_start,
        ]);
    };

    for event in Parser::new_ext(source, options()) {
        size += estimate(&event);
        match &event {
            Event::Start(Tag::Heading { .. }) => {
                depth += 1;
                heading_start = Some(events.len());
            }
            Event::Start(_) => depth += 1,
            Event::End(TagEnd::Heading(level)) => {
                depth -= 1;
                if let Some(start) = heading_start.take() {
                    let text = heading_text(&events[start + 1..]);
                    let id = builder.slugs_id(&text);
                    let section = (builder.sections.len() / SECTION_WORDS) as u32;
                    if let Event::Start(Tag::Heading { id: slot, .. }) = &mut events[start] {
                        *slot = Some(CowStr::Boxed(id.clone().into_boxed_str()));
                    }
                    builder.headings.push(Heading {
                        level: *level as u32,
                        id,
                        text,
                        section,
                    });
                }
            }
            Event::End(_) => depth -= 1,
            _ => {}
        }
        events.push(event);
        if depth == 0 && size >= SECTION_BUDGET {
            flush(&mut builder, &mut events);
            size = 0;
        }
    }
    flush(&mut builder, &mut events);

    let mut document = OpDocument {
        sections: std::mem::take(&mut builder.sections),
        ..Default::default()
    };
    let headings = std::mem::take(&mut builder.headings);
    for heading in &headings {
        let (id_offset, id_len) = builder.enc.reference(&heading.id);
        let (text_offset, text_len) = builder.enc.reference(&heading.text);
        document.headings.extend_from_slice(&[
            heading.level,
            id_offset,
            id_len,
            heading.section,
            text_offset,
            text_len,
        ]);
    }
    document.raw_html = builder.raw_html;
    document.blocks = std::mem::take(&mut builder.enc.blocks);
    document.ops = std::mem::take(&mut builder.enc.ops);
    document.strings = std::mem::take(&mut builder.enc.strings);
    document.text = std::mem::take(&mut builder.enc.text);
    // Every blob grew by doubling, so each can hold up to twice what it needs.
    // On the 10 MiB fixture that slack is several megabytes against a budget of
    // twice the file size (docs/decisions/014-competitive-targets.md,
    // section 3.2), and the document is never appended to again.
    document.ops.shrink_to_fit();
    document.strings.shrink_to_fit();
    document.text.shrink_to_fit();
    document.blocks.shrink_to_fit();
    document.sections.shrink_to_fit();
    document.headings.shrink_to_fit();
    document
}
