//! Hashline's Markdown parser.
//!
//! It reads Markdown once and emits the op buffer the renderer replays
//! (docs/decisions/007-performance-path.md, section 4). No HTML string is
//! produced on the fast path; only sections carrying raw HTML keep an HTML
//! representation, because raw HTML is not expressible as operations and stays
//! on the DOMPurify path in the frontend.
//!
//! The same code serves the desktop through IPC and the browser preview and the
//! test suite through WebAssembly (docs/decisions/008-parser-reference.md).

mod boundary;
mod opbuffer;
mod packet;
mod slug;

use opbuffer::*;
use pulldown_cmark::{
    Alignment, CodeBlockKind, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd,
};
use std::collections::HashMap;

pub use opbuffer::{ALLOWED_ATTR, ALLOWED_TAGS};
pub use packet::{to_packet, Layout};
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

pub const SECTION_WORDS: usize = 9;
pub const HEADING_WORDS: usize = 6;

#[derive(Default)]
pub struct OpDocument {
    pub ops: Vec<u32>,
    pub attrs: Vec<u32>,
    /// Attribute values, heading ids and texts, fallback HTML.
    pub strings: String,
    /// The document's text in document order, with a separator between blocks.
    /// TEXT operations index into this, and the search runs on it directly
    /// (docs/decisions/007, P2.4).
    pub text: String,
    /// 9 words per section: opStart, opCount, flags, hashLow, hashHigh,
    /// htmlOffset, htmlLen, textStart, textLen. The HTML is present only for
    /// fallback sections.
    pub sections: Vec<u32>,
    /// 6 words per heading: level, idOffset, idLen, sectionIndex, textOffset,
    /// textLen.
    pub headings: Vec<u32>,
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
            Tag::HtmlBlock => {}
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
            TagEnd::HtmlBlock => {}
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
            // Raw HTML never reaches here: such sections take the HTML path.
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
    // Raw HTML that leaves a container open cannot be split at all: a section
    // boundary would change the tree the HTML parser builds. Deciding this needs
    // one extra pass, which costs single-digit milliseconds per MiB.
    let mut allow_split = true;
    for event in Parser::new_ext(source, options()) {
        if let Event::Html(html) | Event::InlineHtml(html) = event {
            if !boundary::is_closed_html(&html) {
                allow_split = false;
                break;
            }
        }
    }

    let mut builder = Builder::new();
    let mut events: Vec<Event<'_>> = Vec::new();
    let mut depth = 0usize;
    let mut size = 0usize;
    let mut raw = false;
    let mut heading_start: Option<usize> = None;

    let flush = |builder: &mut Builder, events: &mut Vec<Event<'_>>, raw: &mut bool| {
        if events.is_empty() {
            return;
        }
        let op_start = (builder.enc.ops.len() / 4) as u32;
        let text_start = builder.enc.text_units();
        if *raw {
            let mut html = String::new();
            pulldown_cmark::html::push_html(&mut html, events.drain(..));
            builder.enc.hash_bytes(html.as_bytes());
            let hash = builder.enc.take_hash();
            let (offset, length) = builder.enc.reference(&html);
            // A section the renderer sanitizes contributes no operations, so it
            // contributes no searchable text either; the separator keeps the
            // neighbouring sections' text from running together.
            builder.enc.separate();
            builder.sections.extend_from_slice(&[
                op_start,
                0,
                SECTION_FALLBACK,
                hash as u32,
                (hash >> 32) as u32,
                offset,
                length,
                text_start,
                builder.enc.text_units() - text_start,
            ]);
        } else {
            for event in events.iter() {
                builder.event(event);
            }
            events.clear();
            let (ops_end, hash) = builder.enc.end_section();
            builder.sections.extend_from_slice(&[
                op_start,
                ops_end as u32 - op_start,
                0,
                hash as u32,
                (hash >> 32) as u32,
                0,
                0,
                text_start,
                builder.enc.text_units() - text_start,
            ]);
        }
        *raw = false;
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
            Event::Html(_) | Event::InlineHtml(_) => raw = true,
            _ => {}
        }
        events.push(event);
        if depth == 0 && size >= SECTION_BUDGET && allow_split {
            flush(&mut builder, &mut events, &mut raw);
            size = 0;
        }
    }
    flush(&mut builder, &mut events, &mut raw);

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
    document.ops = std::mem::take(&mut builder.enc.ops);
    document.attrs = std::mem::take(&mut builder.enc.attrs);
    document.strings = std::mem::take(&mut builder.enc.strings);
    document.text = std::mem::take(&mut builder.enc.text);
    document
}
