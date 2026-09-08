//! The document's headings, and where they are.
//!
//! The parser records headings with their generated ids in document order, and
//! the block plan records blocks in document order. Both come out of the same
//! parse, so the *n*-th heading block is the *n*-th heading — no matching by
//! text is needed, and none would be reliable anyway with repeated headings.

use hashline_markdown::{OpDocument, HEADING_WORDS};

use crate::layout::{BlockKind, BlockPlan};

#[derive(Clone, Debug)]
pub struct Entry {
    pub level: u32,
    pub id: String,
    pub text: String,
    /// Index into the block plan, so a jump needs no measuring.
    pub block: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Outline {
    entries: Vec<Entry>,
}

impl Outline {
    pub fn build(document: &OpDocument, plan: &BlockPlan) -> Self {
        let headings: Vec<usize> = (0..plan.len())
            .filter(|&index| matches!(plan.block(index).kind, BlockKind::Heading(_)))
            .collect();
        let count = document.headings.len() / HEADING_WORDS;
        let mut entries = Vec::with_capacity(count.min(headings.len()));
        for (index, &block) in headings.iter().enumerate().take(count) {
            let words = &document.headings[index * HEADING_WORDS..(index + 1) * HEADING_WORDS];
            let slice = |offset: u32, len: u32| {
                let from = offset as usize;
                document.strings[from..from + len as usize].to_string()
            };
            entries.push(Entry {
                level: words[0],
                id: slice(words[1], words[2]),
                text: slice(words[4], words[5]),
                block,
            });
        }
        Outline { entries }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The block a `#fragment` link points at.
    pub fn block_for_id(&self, id: &str) -> Option<usize> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.block)
    }

    /// The heading a reader is currently under, from the scroll position alone
    /// — no measuring while scrolling (SPEC.md, section 8).
    pub fn active_for_block(&self, block: usize) -> Option<usize> {
        self.entries.iter().rposition(|entry| entry.block <= block)
    }
}

#[cfg(test)]
mod tests {
    use super::Outline;
    use crate::layout::{BlockPlan, Metrics};

    fn built(source: &str) -> (Outline, BlockPlan) {
        let document = hashline_markdown::parse(source);
        let plan = BlockPlan::new(
            &document,
            Metrics {
                char_width: 8.5,
                body_px: 17.0,
            },
            640.0,
        );
        (Outline::build(&document, &plan), plan)
    }

    #[test]
    fn headings_line_up_with_the_blocks_they_are() {
        let (outline, plan) = built("# Eins\n\nText.\n\n## Zwei\n\nText.\n\n### Drei\n");
        let entries = outline.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries.iter().map(|e| e.level).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            entries.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
            vec!["Eins", "Zwei", "Drei"]
        );
        for entry in entries {
            assert!(matches!(
                plan.block(entry.block).kind,
                crate::layout::BlockKind::Heading(_)
            ));
        }
    }

    #[test]
    fn repeated_headings_keep_distinct_ids_and_distinct_blocks() {
        let (outline, _) = built("## Kapitel\n\na\n\n## Kapitel\n\nb\n");
        let ids: Vec<&str> = outline.entries().iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["doc-kapitel", "doc-kapitel-1"]);
        assert_ne!(outline.entries()[0].block, outline.entries()[1].block);
        assert_eq!(
            outline.block_for_id("doc-kapitel-1"),
            Some(outline.entries()[1].block)
        );
    }

    #[test]
    fn the_active_heading_is_the_last_one_at_or_above_the_block() {
        let (outline, _) = built("# Eins\n\na\n\n## Zwei\n\nb\n");
        let second = outline.entries()[1].block;
        assert_eq!(outline.active_for_block(0), Some(0));
        assert_eq!(outline.active_for_block(second), Some(1));
        assert_eq!(outline.active_for_block(second + 1), Some(1));
    }

    #[test]
    fn a_document_without_headings_has_an_empty_outline() {
        let (outline, _) = built("Nur Text.\n");
        assert!(outline.is_empty());
        assert_eq!(outline.active_for_block(0), None);
        assert_eq!(outline.block_for_id("doc-x"), None);
    }
}
