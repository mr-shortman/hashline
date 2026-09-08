//! Character offsets for AT-SPI, independent of UTF-8 bytes and visual layout.
use super::Position;
use crate::layout::BlockPlan;

#[derive(Default)]
pub(super) struct Text {
    pub content: String,
    /// Character offset of each block in the accessible text.
    blocks: Vec<u32>,
    pub len: u32,
}

impl Text {
    pub fn build(plan: &BlockPlan, source: &str) -> Self {
        let mut result = Self::default();
        for index in 0..plan.len() {
            if index > 0 {
                result.content.push_str("\n\n");
                result.len += 2;
            }
            result.blocks.push(result.len);
            let block = plan.block(index);
            let text =
                &source[block.text_start as usize..(block.text_start + block.text_len) as usize];
            result.content.push_str(text);
            result.len += text.chars().count() as u32;
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
        let block = plan.block(index);
        let text = &source[block.text_start as usize..(block.text_start + block.text_len) as usize];
        let byte = text
            .char_indices()
            .nth((offset - self.blocks[index]) as usize)
            .map(|(byte, _)| byte)
            .unwrap_or(text.len());
        Some(Position::new(index, byte as u32))
    }

    pub fn slice(&self, start: u32, end: u32) -> String {
        self.content
            .chars()
            .skip(start as usize)
            .take(end.min(self.len).saturating_sub(start) as usize)
            .collect()
    }
}

/// Returns a character range, including the separators following a word or
/// sentence, as expected by screen readers traversing contiguous text ranges.
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
        assert_eq!(text.content, "Grüße 🌍\n\nÄpfel und Öl.");
        assert_eq!(text.slice(4, 7), "e 🌍");
        assert_eq!(text.offset(Position::new(1, 2), &plan, &document.text), 10);
        assert_eq!(text.slice(9, u32::MAX), "Äpfel und Öl.");
        assert_eq!(
            span(&text.content, 10, gtk::AccessibleTextGranularity::Word),
            (9, 15)
        );
    }
}
