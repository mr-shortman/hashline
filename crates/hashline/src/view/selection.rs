//! Selection across block boundaries (docs/architecture.md).
//!
//! In the DOM this came for free; here it is ours to build, and it was risk 2
//! of M0 for that reason. The whole trick is the representation: a position is
//! `(block index, byte offset)` in document order, so a selection spanning
//! blocks is an ordering over pairs rather than a special case with its own
//! code path. Everything below follows from that and is testable without a
//! window.

use crate::layout::BlockPlan;

/// A caret position: which block, and how many bytes into that block's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub block: usize,
    /// Byte offset within the block's text, not within the document.
    pub offset: u32,
}

impl Position {
    pub fn new(block: usize, offset: u32) -> Self {
        Position { block, offset }
    }
    /// The same position as an offset into the document text blob.
    pub fn in_document(&self, plan: &BlockPlan) -> u32 {
        plan.block(self.block).text_start + self.offset
    }
}

/// An anchor and a cursor. The anchor is where the drag began, so it may sit
/// after the cursor; every consumer works on the ordered pair instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Position,
    pub cursor: Position,
}

impl Selection {
    pub fn at(position: Position) -> Self {
        Selection {
            anchor: position,
            cursor: position,
        }
    }
    pub fn to(self, cursor: Position) -> Self {
        Selection { cursor, ..self }
    }
    /// Start and end in document order.
    pub fn range(&self) -> (Position, Position) {
        if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.cursor
    }
    pub fn blocks(&self) -> std::ops::RangeInclusive<usize> {
        let (start, end) = self.range();
        start.block..=end.block
    }

    /// The byte range of `block` that is selected, if any.
    ///
    /// This is what the view hands to Pango when it draws the selection wash,
    /// and it is the one place the three cases — first block, middle block,
    /// last block — are resolved.
    pub fn in_block(&self, block: usize, block_len: u32) -> Option<(u32, u32)> {
        let (start, end) = self.range();
        if block < start.block || block > end.block {
            return None;
        }
        let from = if block == start.block {
            start.offset
        } else {
            0
        };
        let to = if block == end.block {
            end.offset
        } else {
            block_len
        };
        let from = from.min(block_len);
        let to = to.min(block_len);
        if from >= to {
            None
        } else {
            Some((from, to))
        }
    }

    /// The selected text as plain text in document order.
    ///
    /// Blocks are separated by a blank line: a selection spanning paragraphs
    /// should paste as paragraphs. Separators *inside* a block — between list
    /// items or table cells — are already in the document text and come along
    /// unchanged (docs/architecture.md).
    pub fn text(&self, plan: &BlockPlan, document: &str) -> String {
        if self.is_empty() {
            return String::new();
        }
        let mut result = String::new();
        for index in self.blocks() {
            if index >= plan.len() {
                break;
            }
            let block = plan.block(index);
            let Some((from, to)) = self.in_block(index, block.text_len) else {
                continue;
            };
            // The parts a large block was cut into are one block of the
            // document, so they are joined back without a separator.
            if !result.is_empty() && block.is_first() {
                result.push_str("\n\n");
            }
            let base = block.text_start as usize;
            result.push_str(&document[base + from as usize..base + to as usize]);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{Position, Selection};
    use crate::layout::{BlockPlan, Metrics};

    fn planned(source: &str) -> (hashline_markdown::OpDocument, BlockPlan) {
        let document = hashline_markdown::parse(source);
        let plan = BlockPlan::new(
            &document,
            Metrics {
                char_width: 8.5,
                body_px: 17.0,
            },
            646.0,
        );
        (document, plan)
    }

    #[test]
    fn a_blank_line_separates_blocks_but_not_the_parts_of_one() {
        let (document, plan) = planned("Kurz.\n\nEin Absatz. Noch einer.\n");
        let last = plan.len() - 1;
        let all =
            Selection::at(Position::new(0, 0)).to(Position::new(last, plan.block(last).text_len));
        assert_eq!(
            all.text(&plan, &document.text),
            "Kurz.\n\nEin Absatz. Noch einer."
        );

        // The same for a paragraph the plan had to cut up: selecting all of it
        // must give back the paragraph, not its parts with blank lines between.
        let long = "Wortfolge mit Leerzeichen und etwas Text darin. ".repeat(500);
        let (document, plan) = planned(&long);
        assert!(plan.len() > 1, "the paragraph was not cut up");
        let last = plan.len() - 1;
        let all =
            Selection::at(Position::new(0, 0)).to(Position::new(last, plan.block(last).text_len));
        assert_eq!(all.text(&plan, &document.text), document.text);
    }

    #[test]
    fn positions_order_by_block_then_offset() {
        assert!(Position::new(0, 5) < Position::new(1, 0));
        assert!(Position::new(1, 2) < Position::new(1, 3));
        assert_eq!(Position::new(2, 7), Position::new(2, 7));
    }

    #[test]
    fn a_backwards_drag_still_reports_a_forward_range() {
        let selection = Selection::at(Position::new(3, 4)).to(Position::new(1, 2));
        assert_eq!(
            selection.range(),
            (Position::new(1, 2), Position::new(3, 4))
        );
        assert_eq!(selection.blocks().collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn a_middle_block_is_selected_whole_and_the_ends_are_partial() {
        let selection = Selection::at(Position::new(1, 3)).to(Position::new(3, 4));
        assert_eq!(selection.in_block(0, 10), None);
        assert_eq!(selection.in_block(1, 10), Some((3, 10)));
        assert_eq!(selection.in_block(2, 10), Some((0, 10)));
        assert_eq!(selection.in_block(3, 10), Some((0, 4)));
        assert_eq!(selection.in_block(4, 10), None);
    }

    #[test]
    fn an_empty_selection_covers_nothing() {
        let selection = Selection::at(Position::new(2, 5));
        assert!(selection.is_empty());
        assert_eq!(selection.in_block(2, 10), None);
    }

    #[test]
    fn a_selection_within_one_block_is_just_a_range() {
        let selection = Selection::at(Position::new(2, 2)).to(Position::new(2, 6));
        assert_eq!(selection.in_block(2, 10), Some((2, 6)));
        assert_eq!(selection.blocks().collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn a_range_is_clamped_to_the_block_it_lands_in() {
        // Blocks shrink when a document is reloaded; a stale offset must not
        // slice out of bounds.
        let selection = Selection::at(Position::new(0, 0)).to(Position::new(0, 99));
        assert_eq!(selection.in_block(0, 4), Some((0, 4)));
    }
}
