//! Character offsets for AT-SPI, independent of UTF-8 bytes and visual layout.
//!
//! The accessible text is the document's text with a blank line between
//! blocks. Holding it as a second string was the obvious implementation and
//! cost a full copy of the document — 8 MiB on the 10 MiB fixture, for an
//! interface that is idle unless a screen reader is running
//! (docs/metrics.md). What is kept
//! instead is one character offset per block; every string a caller asks for
//! is cut out of the document's own text when it is asked for.
use super::Position;
use crate::layout::BlockPlan;

#[derive(Default)]
pub(super) struct Text {
    /// Character offset of each block in the accessible text.
    blocks: Vec<u32>,
    pub len: u32,
}

/// The separator between two blocks of the document, in the accessible text.
const SEPARATOR: &str = "\n\n";

impl Text {
    pub fn build(plan: &BlockPlan, source: &str) -> Self {
        let mut result = Self {
            blocks: Vec::with_capacity(plan.len()),
            len: 0,
        };
        for index in 0..plan.len() {
            // A blank line separates blocks. The parts a large block was cut
            // into are one block of the document and are joined seamlessly, so
            // that a screen reader hears the paragraph the author wrote.
            if index > 0 && plan.block(index).is_first() {
                result.len += SEPARATOR.chars().count() as u32;
            }
            result.blocks.push(result.len);
            result.len += block_text(plan, source, index).chars().count() as u32;
        }
        result
    }

    pub fn offset(&self, position: Position, plan: &BlockPlan, source: &str) -> u32 {
        let Some(&base) = self.blocks.get(position.block) else {
            return 0;
        };
        let block = plan.block(position.block);
        let start = block.text_start as usize;
        base + source[start..start + position.offset.min(block.text_len) as usize]
            .chars()
            .count() as u32
    }

    pub fn position(&self, offset: u32, plan: &BlockPlan, source: &str) -> Option<Position> {
        let index = self
            .blocks
            .partition_point(|&start| start <= offset)
            .checked_sub(1)?;
        let text = block_text(plan, source, index);
        let byte = text
            .char_indices()
            .nth((offset - self.blocks[index]) as usize)
            .map(|(byte, _)| byte)
            .unwrap_or(text.len());
        Some(Position::new(index, byte as u32))
    }

    /// The accessible text between two character offsets, cut out of the
    /// document rather than out of a copy of it.
    pub fn slice(&self, plan: &BlockPlan, source: &str, start: u32, end: u32) -> String {
        let end = end.min(self.len);
        if start >= end {
            return String::new();
        }
        let first = self.blocks.partition_point(|&at| at <= start).max(1) - 1;
        let mut result = String::new();
        let mut cursor = self.blocks[first];
        for index in first..self.blocks.len() {
            if cursor >= end {
                break;
            }
            if index > first && plan.block(index).is_first() {
                push_range(&mut result, SEPARATOR, &mut cursor, start, end);
            }
            let text = block_text(plan, source, index);
            push_range(&mut result, text, &mut cursor, start, end);
        }
        result
    }
}

/// One block's own text, without the separators around it.
fn block_text<'a>(plan: &BlockPlan, source: &'a str, index: usize) -> &'a str {
    let block = plan.block(index);
    &source[block.text_start as usize..(block.text_start + block.text_len) as usize]
}

/// Appends the part of `text` that falls inside `start..end`, advancing the
/// running character position.
fn push_range(result: &mut String, text: &str, cursor: &mut u32, start: u32, end: u32) {
    for character in text.chars() {
        if *cursor >= start && *cursor < end {
            result.push(character);
        }
        *cursor += 1;
    }
}

/// Returns a character range, including the separators following a word or
/// sentence, as expected by screen readers traversing contiguous text ranges.
///
/// Granularity is resolved inside the block the offset falls in. Blocks are
/// separated by a blank line, so a word and a line never cross that boundary
/// anyway, and a sentence that would is a sentence the author ended with a
/// paragraph break.
pub(super) fn span_at(
    text: &Text,
    plan: &BlockPlan,
    source: &str,
    offset: u32,
    granularity: gtk::AccessibleTextGranularity,
) -> (u32, u32) {
    let Some(position) = text.position(offset, plan, source) else {
        return (offset, offset);
    };
    let base = text.blocks[position.block];
    let body = block_text(plan, source, position.block);
    let (start, end) = span(body, offset.saturating_sub(base), granularity);
    (base + start, base + end)
}

pub(super) fn span(
    text: &str,
    offset: u32,
    granularity: gtk::AccessibleTextGranularity,
) -> (u32, u32) {
    use gtk::AccessibleTextGranularity as G;
    let chars: Vec<char> = text.chars().collect();
    let offset = (offset as usize).min(chars.len());
    if offset == chars.len() {
        return (offset as u32, offset as u32);
    }
    if granularity == G::Character {
        return (offset as u32, offset as u32 + 1);
    }
    let boundary = |index: usize| match granularity {
        G::Word => {
            chars[index].is_alphanumeric() && (index == 0 || !chars[index - 1].is_alphanumeric())
        }
        G::Sentence => {
            index > 0 && !chars[index].is_whitespace() && {
                chars[..index]
                    .iter()
                    .rev()
                    .find(|c| !c.is_whitespace())
                    .is_some_and(|c| matches!(c, '.' | '!' | '?' | '。' | '！' | '？'))
            }
        }
        G::Line | G::Paragraph => index > 0 && chars[index - 1] == '\n',
        _ => false,
    };
    let start = (0..=offset).rev().find(|&i| boundary(i)).unwrap_or(0);
    let end = (offset + 1..chars.len())
        .find(|&i| boundary(i))
        .unwrap_or(chars.len());
    (start as u32, end as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_offsets_and_paragraphs_are_character_based() {
        let document = hashline_markdown::parse("# Grüße 🌍\n\nÄpfel und Öl.\n");
        let plan = BlockPlan::new(
            &document,
            crate::layout::Metrics {
                char_width: 8.0,
                body_px: 17.0,
            },
            640.0,
        );
        let text = Text::build(&plan, &document.text);
        let source = &document.text;
        assert_eq!(
            text.slice(&plan, source, 0, u32::MAX),
            "Grüße 🌍\n\nÄpfel und Öl."
        );
        assert_eq!(text.slice(&plan, source, 4, 7), "e 🌍");
        assert_eq!(text.offset(Position::new(1, 2), &plan, source), 10);
        assert_eq!(text.slice(&plan, source, 9, u32::MAX), "Äpfel und Öl.");
        assert_eq!(
            span_at(
                &text,
                &plan,
                source,
                10,
                gtk::AccessibleTextGranularity::Word
            ),
            (9, 15)
        );
    }
}
