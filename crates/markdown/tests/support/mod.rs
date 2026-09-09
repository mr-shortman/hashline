//! Test-only decoding of the op buffer.
//!
//! The suite this replaces lived in TypeScript and leaned on the browser DOM
//! (docs/decisions/008-parser-reference.md). The method is carried over intact:
//! the parser's operations are replayed into HTML, both sides are parsed by the
//! same HTML parser, and the resulting trees are compared after the
//! normalizations 008 allows. What changes is only the host — Cargo instead of
//! vitest — because the JavaScript toolchain goes away with the WebView
//! (SPEC.md, section 13).

// Each integration test binary compiles this module separately and uses only
// the part it needs.
#![allow(dead_code)]

use hashline_markdown::{
    read_varint, OpDocument, ALLOWED_ATTR, ALLOWED_TAGS, OP_ATTRS, OP_CLOSE, OP_TAG_MASK, OP_TEXT,
};

/// Words per section and per heading, as `OpDocument` documents them.
pub const SECTION_WORDS: usize = hashline_markdown::SECTION_WORDS;
pub const HEADING_WORDS: usize = hashline_markdown::HEADING_WORDS;

/// Elements the HTML parser closes on its own; emitting an end tag for them
/// would be ignored at best and reparented at worst.
const VOID: [&str; 4] = ["br", "hr", "img", "input"];

/// One of the two blobs, with the addressing the buffer promises.
///
/// Offsets are UTF-8 byte offsets (SPEC.md, section 6). Slicing a `&str` by a
/// byte range panics unless the range lands on character boundaries, so this is
/// itself the check that the encoder counted correctly.
pub struct Blob {
    value: String,
}

impl Blob {
    pub fn new(value: &str) -> Self {
        Blob {
            value: value.to_string(),
        }
    }
    pub fn slice(&self, offset: u32, length: u32) -> String {
        let start = offset as usize;
        let end = start + length as usize;
        assert!(
            end <= self.value.len(),
            "offset {offset}+{length} out of range"
        );
        self.value[start..end].to_string()
    }
    pub fn len(&self) -> usize {
        self.value.len()
    }
}

pub struct Doc {
    pub document: OpDocument,
    pub strings: Blob,
    pub text: Blob,
}

pub fn parse(source: &str) -> Doc {
    let document = hashline_markdown::parse(source);
    let strings = Blob::new(&document.strings);
    let text = Blob::new(&document.text);
    Doc {
        document,
        strings,
        text,
    }
}

impl Doc {
    pub fn section_count(&self) -> usize {
        self.document.sections.len() / SECTION_WORDS
    }
    pub fn section(&self, index: usize) -> &[u32] {
        let start = index * SECTION_WORDS;
        &self.document.sections[start..start + SECTION_WORDS]
    }
    /// The section's content hash, as the two words that carry it.
    pub fn section_key(&self, index: usize) -> [u32; 2] {
        let words = self.section(index);
        [words[2], words[3]]
    }
    pub fn section_text_range(&self, index: usize) -> (u32, u32) {
        let words = self.section(index);
        (words[4], words[4] + words[5])
    }
    pub fn block_count(&self) -> usize {
        self.document.blocks.len() / hashline_markdown::BLOCK_WORDS
    }
    /// `(tag, opStart, opCount, textStart, textLen)`.
    pub fn block(&self, index: usize) -> &[u32] {
        let start = index * hashline_markdown::BLOCK_WORDS;
        &self.document.blocks[start..start + hashline_markdown::BLOCK_WORDS]
    }
    pub fn heading_count(&self) -> usize {
        self.document.headings.len() / HEADING_WORDS
    }
    /// `(level, id, text, section)`.
    pub fn heading(&self, index: usize) -> (u32, String, String, u32) {
        let start = index * HEADING_WORDS;
        let words = &self.document.headings[start..start + HEADING_WORDS];
        (
            words[0],
            self.strings.slice(words[1], words[2]),
            self.strings.slice(words[4], words[5]),
            words[3],
        )
    }

    /// Every TEXT operation in document order, as `(offset, length)`.
    ///
    /// Text offsets are relative to the run before them inside a block, so the
    /// whole stream is walked rather than scanned for text bytes.
    pub fn text_runs(&self) -> Vec<(u32, u32)> {
        let mut runs = Vec::new();
        for index in 0..self.block_count() {
            let words = self.block(index);
            let (start, end) = (words[1] as usize, (words[1] + words[2]) as usize);
            runs.extend(text_runs_in(&self.document.ops[start..end]));
        }
        runs
    }

    /// Replays the whole document the way the renderer would, into HTML.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for index in 0..self.section_count() {
            self.replay_section(index, &mut out);
        }
        out
    }

    fn replay_section(&self, index: usize, out: &mut String) {
        let words = self.section(index);
        let start = words[0] as usize;
        let end = start + words[1] as usize;
        let ops = &self.document.ops[start..end];
        let mut open: Vec<&str> = Vec::new();
        let mut cursor = 0usize;
        // A section may hold several blocks, and a text offset is absolute
        // only for the first run of each. The replay therefore restarts its
        // delta whenever a top-level element opens.
        let mut depth = 0usize;
        let mut text_seen = false;
        let mut text_cursor = 0u32;
        while cursor < ops.len() {
            let byte = ops[cursor];
            cursor += 1;
            match byte {
                OP_CLOSE => {
                    let tag = open.pop().expect("close without open");
                    depth -= 1;
                    if !VOID.contains(&tag) {
                        out.push_str("</");
                        out.push_str(tag);
                        out.push('>');
                    }
                }
                OP_TEXT => {
                    let value = read_varint(ops, &mut cursor);
                    let length = read_varint(ops, &mut cursor);
                    let offset = if text_seen {
                        text_cursor + value
                    } else {
                        value
                    };
                    text_seen = true;
                    text_cursor = offset;
                    out.push_str(&escape_text(&self.text.slice(offset, length)));
                }
                byte => {
                    if depth == 0 {
                        text_seen = false;
                    }
                    depth += 1;
                    let tag = ALLOWED_TAGS[(byte & OP_TAG_MASK) as usize];
                    out.push('<');
                    out.push_str(tag);
                    if byte & OP_ATTRS != 0 {
                        let count = read_varint(ops, &mut cursor);
                        for _ in 0..count {
                            let name = ALLOWED_ATTR[read_varint(ops, &mut cursor) as usize];
                            let at = read_varint(ops, &mut cursor);
                            let length = read_varint(ops, &mut cursor);
                            out.push(' ');
                            out.push_str(name);
                            out.push_str("=\"");
                            out.push_str(&escape_attribute(&self.strings.slice(at, length)));
                            out.push('"');
                        }
                    }
                    out.push('>');
                    open.push(tag);
                }
            }
        }
        assert!(open.is_empty(), "section {index} left elements open");
    }
}

/// Every text run of one block's operations, as absolute `(offset, length)`.
pub fn text_runs_in(ops: &[u8]) -> Vec<(u32, u32)> {
    let mut runs = Vec::new();
    let (mut cursor, mut seen, mut previous) = (0usize, false, 0u32);
    while cursor < ops.len() {
        let byte = ops[cursor];
        cursor += 1;
        match byte {
            OP_CLOSE => {}
            OP_TEXT => {
                let value = read_varint(ops, &mut cursor);
                let length = read_varint(ops, &mut cursor);
                let offset = if seen { previous + value } else { value };
                seen = true;
                previous = offset;
                runs.push((offset, length));
            }
            byte if byte & OP_ATTRS != 0 => {
                let count = read_varint(ops, &mut cursor);
                for _ in 0..count * 3 {
                    read_varint(ops, &mut cursor);
                }
            }
            _ => {}
        }
    }
    runs
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attribute(value: &str) -> String {
    value.replace('&', "&amp;").replace('"', "&quot;")
}

pub mod html;
