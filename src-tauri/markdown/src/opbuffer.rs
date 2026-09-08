//! The op buffer from docs/decisions/007-performance-path.md, section 4.
//!
//! The layout is the one `src/core/markdown/opbuffer.ts` documents and decodes.
//! Offsets and lengths are **UTF-16 code units** into the string the renderer
//! obtains by decoding `strings` once per document — never byte offsets. The
//! encoder carries that count forward while it appends, which is the whole
//! reason the renderer can use `substring` without a translation table.

pub const OP_OPEN: u32 = 0;
pub const OP_CLOSE: u32 = 1;
pub const OP_TEXT: u32 = 2;
pub const SECTION_FALLBACK: u32 = 1;

/// Exactly the tag allowlist `opbuffer.ts` exports; a tag outside it has no id
/// and is therefore not expressible in the buffer at all.
pub const ALLOWED_TAGS: [&str; 39] = [
    "p", "h1", "h2", "h3", "h4", "h5", "h6", "em", "strong", "del", "s", "code", "pre",
    "blockquote", "ul", "ol", "li", "hr", "br", "a", "img", "table", "thead", "tbody", "tfoot",
    "tr", "th", "td", "input", "sup", "sub", "kbd", "details", "summary", "div", "span", "dl", "dt",
    "dd",
];
/// Exactly the attribute allowlist `opbuffer.ts` exports.
pub const ALLOWED_ATTR: [&str; 16] = [
    "id", "href", "src", "alt", "title", "class", "start", "type", "checked", "disabled", "colspan",
    "rowspan", "align", "width", "height", "open",
];

pub const TAG_P: u32 = 0;
pub const TAG_EM: u32 = 7;
pub const TAG_STRONG: u32 = 8;
pub const TAG_DEL: u32 = 9;
pub const TAG_CODE: u32 = 11;
pub const TAG_PRE: u32 = 12;
pub const TAG_BLOCKQUOTE: u32 = 13;
pub const TAG_UL: u32 = 14;
pub const TAG_OL: u32 = 15;
pub const TAG_LI: u32 = 16;
pub const TAG_HR: u32 = 17;
pub const TAG_BR: u32 = 18;
pub const TAG_A: u32 = 19;
pub const TAG_IMG: u32 = 20;
pub const TAG_TABLE: u32 = 21;
pub const TAG_THEAD: u32 = 22;
pub const TAG_TBODY: u32 = 23;
pub const TAG_TR: u32 = 25;
pub const TAG_SUMMARY: u32 = 33;
pub const TAG_DT: u32 = 37;
pub const TAG_DD: u32 = 38;
pub const TAG_TH: u32 = 26;
pub const TAG_TD: u32 = 27;
pub const TAG_INPUT: u32 = 28;
pub const TAG_SUP: u32 = 29;
pub const TAG_DIV: u32 = 34;

pub const ATTR_ID: u32 = 0;
pub const ATTR_HREF: u32 = 1;
pub const ATTR_SRC: u32 = 2;
pub const ATTR_ALT: u32 = 3;
pub const ATTR_TITLE: u32 = 4;
pub const ATTR_CLASS: u32 = 5;
pub const ATTR_START: u32 = 6;
pub const ATTR_TYPE: u32 = 7;
pub const ATTR_CHECKED: u32 = 8;
pub const ATTR_DISABLED: u32 = 9;
pub const ATTR_ALIGN: u32 = 12;

fn utf16_len(value: &str) -> u32 {
    if value.is_ascii() {
        value.len() as u32
    } else {
        value.chars().map(|c| c.len_utf16() as u32).sum()
    }
}

/// FNV-1a. Used only for section identity (see 008, section 5), never for
/// integrity or security.
#[derive(Clone, Copy)]
pub struct Fnv(u64);
impl Default for Fnv {
    fn default() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}
impl Fnv {
    pub fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(0x1000_0000_01b3);
        }
    }
    pub fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }
    pub fn value(self) -> u64 {
        self.0
    }
}

// The blocks `indexText` treated as separate for search, unchanged: a match
// must not run from the end of one block into the start of the next.
const BLOCK_TAGS: [u32; 14] = [
    TAG_P, TAG_LI, TAG_PRE, TAG_TD, TAG_TH, 1, 2, 3, 4, 5, 6, TAG_SUMMARY, TAG_DT, TAG_DD,
];

#[derive(Default)]
pub struct Encoder {
    pub ops: Vec<u32>,
    pub attrs: Vec<u32>,
    /// Attribute values, heading ids and texts, and the HTML of fallback
    /// sections. Never document text.
    pub strings: String,
    units: u32,
    /// The document's text in document order — what the search runs on.
    pub text: String,
    text_units: u32,
    pub attr_count: u32,
    hash: Fnv,
    pending: String,
    /// Serial of the block element enclosing the current position, and of the
    /// one that produced the previous text run.
    blocks: Vec<u32>,
    block_serial: u32,
    last_block: Option<u32>,
}

impl Encoder {
    fn intern(&mut self, value: &str) -> u32 {
        let offset = self.units;
        self.strings.push_str(value);
        self.units += utf16_len(value);
        offset
    }
    fn current_block(&self) -> Option<u32> {
        self.blocks.last().copied()
    }
    /// Length of the document text so far, in UTF-16 code units.
    pub fn text_units(&self) -> u32 {
        self.text_units
    }
    /// Separates two sections whose text must not run together, for sections
    /// that contribute no operations at all.
    pub fn separate(&mut self) {
        self.text.push('\n');
        self.text_units += 1;
        self.last_block = None;
    }
    /// Text runs are merged the way an HTML parser merges them, so that
    /// `isEqualNode` stays usable as a contract and fewer nodes are created.
    pub fn text(&mut self, value: &str) {
        self.pending.push_str(value);
    }
    fn flush_text(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending);
        self.hash.write(b"\x02");
        self.hash.write(pending.as_bytes());
        let block = self.current_block();
        if self.last_block.is_some() && block != self.last_block {
            // The separator belongs to no operation; it only keeps a match from
            // spanning a block boundary, exactly as `indexText` did.
            self.text.push('\n');
            self.text_units += 1;
        }
        self.last_block = block;
        let offset = self.text_units;
        self.text.push_str(&pending);
        let length = utf16_len(&pending);
        self.text_units += length;
        self.ops.extend_from_slice(&[OP_TEXT, offset, length, 0]);
        self.pending = pending;
        self.pending.clear();
    }
    pub fn open(&mut self, tag: u32) -> Open {
        self.flush_text();
        self.hash.write(b"\x00");
        self.hash.write_u32(tag);
        // Every open pushes the block in force inside it, so the enclosing
        // block is one lookup rather than a walk up the stack.
        let block = if BLOCK_TAGS.contains(&tag) {
            self.block_serial += 1;
            Some(self.block_serial)
        } else {
            self.current_block()
        };
        self.blocks.push(block.unwrap_or(0));
        Open {
            tag,
            attr_start: self.attr_count,
        }
    }
    pub fn attribute(&mut self, open: &Open, name: u32, value: &str) {
        debug_assert!(open.attr_start <= self.attr_count);
        self.hash.write(b"\x03");
        self.hash.write_u32(name);
        self.hash.write(value.as_bytes());
        let offset = self.intern(value);
        let length = utf16_len(value);
        self.attrs.extend_from_slice(&[name, offset, length]);
        self.attr_count += 1;
    }
    /// Emits the OPEN once its attributes are known.
    pub fn opened(&mut self, open: Open) {
        let count = self.attr_count - open.attr_start;
        self.ops
            .extend_from_slice(&[OP_OPEN, open.tag, open.attr_start, count]);
    }
    pub fn close(&mut self) {
        self.flush_text();
        self.hash.write(b"\x01");
        self.blocks.pop();
        self.ops.extend_from_slice(&[OP_CLOSE, 0, 0, 0]);
    }
    pub fn end_section(&mut self) -> (usize, u64) {
        self.flush_text();
        let hash = self.hash.value();
        self.hash = Fnv::default();
        (self.ops.len() / 4, hash)
    }
    /// Offset and length of `value` after appending it; for ids and heading text.
    pub fn reference(&mut self, value: &str) -> (u32, u32) {
        (self.intern(value), utf16_len(value))
    }
    pub fn hash_bytes(&mut self, bytes: &[u8]) {
        self.hash.write(bytes);
    }
    pub fn take_hash(&mut self) -> u64 {
        let hash = self.hash.value();
        self.hash = Fnv::default();
        hash
    }
}

/// An element whose attributes are still being collected.
pub struct Open {
    tag: u32,
    attr_start: u32,
}
