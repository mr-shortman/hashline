//! The op buffer (SPEC.md, section 6, and
//! docs/decisions/010-op-buffer-for-the-native-renderer.md).
//!
//! Offsets and lengths are **UTF-8 byte offsets** into `strings` and `text`.
//! They counted UTF-16 code units for as long as the consumer was JavaScript
//! and wanted `substring` without a translation table; the Rust renderer slices
//! `&str` by byte and that reason is gone.

//! ## The encoding
//!
//! Operations are a byte stream, not an array of four-word records. The record
//! form cost 16 bytes per operation and 24 MiB on the 10 MiB fixture — more
//! than the whole memory growth decision 014 allows for that document
//! (docs/decisions/014-competitive-targets.md, section 3.2). A close is one
//! byte here, an open one or two, a text run three to five.
//!
//! ```text
//! 0x00..=0x3E  OPEN, tag id in the byte itself
//! 0x40..=0x7E  OPEN with attributes: tag id in the low six bits, then a
//!              varint count and, per attribute, varint name, string offset
//!              and length
//! 0x80         CLOSE
//! 0x81         TEXT: varint text offset, varint length. The first run of a
//!              block carries an absolute offset, every later one the distance
//!              from the run before it, so a block can be decoded on its own.
//! ```
//!
//! Every number is LEB128. `opStart` and `opCount` of a block are therefore a
//! byte offset and a byte length, not an operation index and a count.

/// Attributes follow this open. The tag id occupies the low six bits, which is
/// why `ALLOWED_TAGS` may not grow past 63 entries.
pub const OP_ATTRS: u8 = 0x40;
pub const OP_TAG_MASK: u8 = 0x3F;
pub const OP_CLOSE: u8 = 0x80;
pub const OP_TEXT: u8 = 0x81;

/// Appends an unsigned LEB128 number.
pub fn write_varint(out: &mut Vec<u8>, mut value: u32) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

/// Reads an unsigned LEB128 number, advancing the cursor.
pub fn read_varint(bytes: &[u8], cursor: &mut usize) -> u32 {
    let (mut value, mut shift) = (0u32, 0u32);
    while *cursor < bytes.len() {
        let byte = bytes[*cursor];
        *cursor += 1;
        value |= ((byte & 0x7F) as u32) << shift;
        if byte < 0x80 {
            break;
        }
        shift += 7;
    }
    value
}

/// Exactly the tag allowlist `opbuffer.ts` exports; a tag outside it has no id
/// and is therefore not expressible in the buffer at all.
pub const ALLOWED_TAGS: [&str; 39] = [
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "em",
    "strong",
    "del",
    "s",
    "code",
    "pre",
    "blockquote",
    "ul",
    "ol",
    "li",
    "hr",
    "br",
    "a",
    "img",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "th",
    "td",
    "input",
    "sup",
    "sub",
    "kbd",
    "details",
    "summary",
    "div",
    "span",
    "dl",
    "dt",
    "dd",
];
/// Exactly the attribute allowlist `opbuffer.ts` exports.
pub const ALLOWED_ATTR: [&str; 16] = [
    "id", "href", "src", "alt", "title", "class", "start", "type", "checked", "disabled",
    "colspan", "rowspan", "align", "width", "height", "open",
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
    TAG_P,
    TAG_LI,
    TAG_PRE,
    TAG_TD,
    TAG_TH,
    1,
    2,
    3,
    4,
    5,
    6,
    TAG_SUMMARY,
    TAG_DT,
    TAG_DD,
];

#[derive(Default)]
pub struct Encoder {
    pub ops: Vec<u8>,
    /// Attribute values, heading ids and heading texts. Never document text.
    pub strings: String,
    /// The document's text in document order — what the search runs on.
    pub text: String,
    /// Top-level flow elements: what the block plan virtualizes over
    /// (SPEC.md, section 5). `BLOCK_WORDS` words each.
    pub blocks: Vec<u32>,
    hash: Fnv,
    pending: String,
    /// One entry per open element, holding the serial of the block enclosing
    /// it; its length is therefore the current element depth.
    stack: Vec<u32>,
    block_serial: u32,
    last_block: Option<u32>,
    /// Tag, first op and — once it has any — text start of the top-level
    /// element being built.
    open_block: Option<(u32, u32, Option<u32>)>,
    /// Attributes of the element being opened. One buffer serves every
    /// element, because an open is never nested inside another open.
    attributes: Vec<(u32, u32, u32)>,
    /// Whether the block being built has emitted a text run yet, and where the
    /// last one started: together they make a run's offset a small delta.
    text_seen: bool,
    text_cursor: u32,
}

/// Words per block: tag, opStart (byte), opCount (bytes), textStart, textLen.
pub const BLOCK_WORDS: usize = 5;

impl Encoder {
    fn intern(&mut self, value: &str) -> u32 {
        let offset = self.strings.len() as u32;
        self.strings.push_str(value);
        offset
    }
    fn current_block(&self) -> Option<u32> {
        self.stack.last().copied()
    }
    /// Length of the document text so far, in bytes.
    pub fn text_len(&self) -> u32 {
        self.text.len() as u32
    }
    /// Text runs are merged the way an HTML parser merges them, so that fewer
    /// operations are produced than there are parser events.
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
        }
        self.last_block = block;
        let offset = self.text.len() as u32;
        // The block's text begins at its first run, never at the separator
        // written above: that separator divides two blocks and belongs to
        // neither, so a block's range is exactly its own text.
        if let Some((_, _, start)) = self.open_block.as_mut() {
            start.get_or_insert(offset);
        }
        self.text.push_str(&pending);
        let length = pending.len() as u32;
        self.ops.push(OP_TEXT);
        // The first run of a block is absolute so the block decodes alone.
        write_varint(
            &mut self.ops,
            if self.text_seen {
                offset - self.text_cursor
            } else {
                offset
            },
        );
        write_varint(&mut self.ops, length);
        self.text_seen = true;
        self.text_cursor = offset;
        self.pending = pending;
        self.pending.clear();
    }
    pub fn open(&mut self, tag: u32) -> Open {
        self.flush_text();
        // A top-level element starts a block of the plan. Text flushed above
        // still belongs to the block before it.
        if self.stack.is_empty() {
            self.open_block = Some((tag, self.ops.len() as u32, None));
            self.text_seen = false;
        }
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
        self.stack.push(block.unwrap_or(0));
        self.attributes.clear();
        Open { tag }
    }
    pub fn attribute(&mut self, _open: &Open, name: u32, value: &str) {
        self.hash.write(b"\x03");
        self.hash.write_u32(name);
        self.hash.write(value.as_bytes());
        let offset = self.intern(value);
        self.attributes.push((name, offset, value.len() as u32));
    }
    /// Emits the OPEN once its attributes are known.
    pub fn opened(&mut self, open: Open) {
        debug_assert!(open.tag <= OP_TAG_MASK as u32, "tag id must fit six bits");
        if self.attributes.is_empty() {
            self.ops.push(open.tag as u8);
            return;
        }
        self.ops.push(open.tag as u8 | OP_ATTRS);
        // Taken out and put back so the buffer keeps its capacity across
        // elements while the operations are written.
        let attributes = std::mem::take(&mut self.attributes);
        write_varint(&mut self.ops, attributes.len() as u32);
        for &(name, offset, length) in &attributes {
            write_varint(&mut self.ops, name);
            write_varint(&mut self.ops, offset);
            write_varint(&mut self.ops, length);
        }
        self.attributes = attributes;
        self.attributes.clear();
    }
    pub fn close(&mut self) {
        self.flush_text();
        self.hash.write(b"\x01");
        self.stack.pop();
        self.ops.push(OP_CLOSE);
        // Closing back to depth zero completes the block: its operations and
        // its slice of the text blob are now both known.
        if self.stack.is_empty() {
            if let Some((tag, op_start, text_start)) = self.open_block.take() {
                let op_count = self.ops.len() as u32 - op_start;
                // A block without text — a rule, an image on its own — is an
                // empty range at the position it occupies.
                let text_start = text_start.unwrap_or(self.text.len() as u32);
                self.blocks.extend_from_slice(&[
                    tag,
                    op_start,
                    op_count,
                    text_start,
                    self.text.len() as u32 - text_start,
                ]);
            }
        }
    }
    pub fn end_section(&mut self) -> (usize, u64) {
        self.flush_text();
        let hash = self.hash.value();
        self.hash = Fnv::default();
        (self.ops.len(), hash)
    }
    /// Offset and length of `value` after appending it; for ids and heading text.
    pub fn reference(&mut self, value: &str) -> (u32, u32) {
        (self.intern(value), value.len() as u32)
    }
}

/// An element whose attributes are still being collected.
pub struct Open {
    tag: u32,
}
