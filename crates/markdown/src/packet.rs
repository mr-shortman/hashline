//! Binary document packet, decoded by `src/platform/document-packet.ts`.
//!
//! ```text
//! u32  headerLen              little endian
//! …    header JSON, UTF-8, padded with spaces to a multiple of four
//! …    ops       u32 little endian
//! …    attrs     u32
//! …    sections  u32
//! …    headings  u32
//! …    strings   UTF-8
//! …    text      UTF-8
//! ```
//!
//! The padding is what lets the frontend build `Uint32Array` views straight on
//! the received `ArrayBuffer`: those need a four-byte aligned offset. Little
//! endian is not a portability compromise — producer and consumer are the same
//! process group on the same machine.

use crate::OpDocument;

pub struct Layout {
    pub ops: usize,
    pub attrs: usize,
    pub sections: usize,
    pub headings: usize,
    pub strings: usize,
    pub text: usize,
}

impl Layout {
    pub fn of(document: &OpDocument) -> Self {
        Layout {
            ops: document.ops.len(),
            attrs: document.attrs.len(),
            sections: document.sections.len(),
            headings: document.headings.len(),
            strings: document.strings.len(),
            text: document.text.len(),
        }
    }
    pub fn json(&self) -> String {
        format!(
            "\"layout\":{{\"ops\":{},\"attrs\":{},\"sections\":{},\"headings\":{},\"strings\":{},\"text\":{}}}",
            self.ops, self.attrs, self.sections, self.headings, self.strings, self.text
        )
    }
}

fn extend_words(packet: &mut Vec<u8>, words: &[u32]) {
    for word in words {
        packet.extend_from_slice(&word.to_le_bytes());
    }
}

/// `fields` is already-serialized JSON object content without the braces, for
/// example `"id":"7","path":"/a.md"`. It may be empty.
pub fn to_packet(document: &OpDocument, fields: &str) -> Vec<u8> {
    let separator = if fields.is_empty() { "" } else { "," };
    let header = format!("{{{fields}{separator}{}}}", Layout::of(document).json());
    let padding = (4 - header.len() % 4) % 4;
    let mut packet = Vec::with_capacity(
        4 + header.len()
            + padding
            + 4 * (document.ops.len()
                + document.attrs.len()
                + document.sections.len()
                + document.headings.len())
            + document.strings.len()
            + document.text.len(),
    );
    packet.extend_from_slice(&((header.len() + padding) as u32).to_le_bytes());
    packet.extend_from_slice(header.as_bytes());
    packet.extend(std::iter::repeat_n(b' ', padding));
    extend_words(&mut packet, &document.ops);
    extend_words(&mut packet, &document.attrs);
    extend_words(&mut packet, &document.sections);
    extend_words(&mut packet, &document.headings);
    packet.extend_from_slice(document.strings.as_bytes());
    packet.extend_from_slice(document.text.as_bytes());
    packet
}
