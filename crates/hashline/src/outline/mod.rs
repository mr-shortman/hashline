//! The document's headings, and where they are.
//!
//! The parser records headings with their generated ids in document order, and
//! the block plan records blocks in document order. Both come out of the same
//! parse, so the *n*-th heading block is the *n*-th heading — no matching by
//! text is needed, and none would be reliable anyway with repeated headings.
//!
//! An entry holds no strings of its own. The parser already keeps every
//! heading id and heading text in the document's string blob, and a copy of
//! them costs about a megabyte on the 10 MiB fixture for no gain
//! (docs/decisions/014-competitive-targets.md, section 3.2). An entry is
//! therefore two ranges into that blob, and the outline holds the document
//! alive for as long as it needs them.

use std::rc::Rc;

use hashline_markdown::{OpDocument, HEADING_WORDS};

use crate::layout::{BlockKind, BlockPlan};

#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub level: u8,
    /// Index into the block plan, so a jump needs no measuring.
    pub block: u32,
    id: (u32, u32),
    text: (u32, u32),
}

#[derive(Clone, Default)]
pub struct Outline {
    document: Rc<OpDocument>,
    entries: Vec<Entry>,
}

impl Outline {
    /// The entries alone, from a document that is not shared yet. Splitting
    /// this from `new` is what lets the caller release the parser's heading
    /// table before the document is put behind an `Rc`.
    pub fn entries_of(document: &OpDocument, plan: &BlockPlan) -> Vec<Entry> {
        // One entry per heading of the document, so a heading long enough for
        // the plan to cut into parts still counts once, at its first part.
        let headings: Vec<usize> = (0..plan.len())
            .filter(|&index| {
                let block = plan.block(index);
                matches!(block.kind, BlockKind::Heading(_)) && block.is_first()
            })
            .collect();
        let count = document.headings.len() / HEADING_WORDS;
        let mut entries = Vec::with_capacity(count.min(headings.len()));
        for (index, &block) in headings.iter().enumerate().take(count) {
            let words = &document.headings[index * HEADING_WORDS..(index + 1) * HEADING_WORDS];
            entries.push(Entry {
                level: words[0] as u8,
                block: block as u32,
                id: (words[1], words[2]),
                text: (words[4], words[5]),
            });
        }
        entries
    }

    pub fn new(document: Rc<OpDocument>, entries: Vec<Entry>) -> Self {
        Outline { document, entries }
    }

    pub fn build(document: &Rc<OpDocument>, plan: &BlockPlan) -> Self {
        let entries = Self::entries_of(document, plan);
        Outline::new(document.clone(), entries)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    fn slice(&self, (offset, len): (u32, u32)) -> &str {
        let from = offset as usize;
        self.document
            .strings
            .get(from..from + len as usize)
            .unwrap_or("")
    }
    /// The heading's text, borrowed from the document's string blob.
    pub fn text(&self, index: usize) -> &str {
        self.entries
            .get(index)
            .map_or("", |entry| self.slice(entry.text))
    }
    /// The heading's generated id.
    pub fn id(&self, index: usize) -> &str {
        self.entries
            .get(index)
            .map_or("", |entry| self.slice(entry.id))
    }

    /// The block a `#fragment` link points at.
    pub fn block_for_id(&self, id: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| self.slice(entry.id) == id)
            .map(|index| self.entries[index].block as usize)
    }

    /// The heading a reader is currently under, from the scroll position alone
    /// — no measuring while scrolling (SPEC.md, section 8).
    pub fn active_for_block(&self, block: usize) -> Option<usize> {
        self.entries
            .iter()
            .rposition(|entry| entry.block as usize <= block)
    }
}

#[cfg(test)]
mod tests {
    use super::Outline;
    use crate::layout::{BlockPlan, Metrics};
    use std::rc::Rc;

    fn built(source: &str) -> (Outline, BlockPlan) {
        let document = Rc::new(hashline_markdown::parse(source));
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
            (0..outline.len())
                .map(|i| outline.text(i))
                .collect::<Vec<_>>(),
            vec!["Eins", "Zwei", "Drei"]
        );
        for entry in entries {
            assert!(matches!(
                plan.block(entry.block as usize).kind,
                crate::layout::BlockKind::Heading(_)
            ));
        }
    }

    #[test]
    fn repeated_headings_keep_distinct_ids_and_distinct_blocks() {
        let (outline, _) = built("## Kapitel\n\na\n\n## Kapitel\n\nb\n");
        let ids: Vec<&str> = (0..outline.len()).map(|i| outline.id(i)).collect();
        assert_eq!(ids, vec!["doc-kapitel", "doc-kapitel-1"]);
        assert_ne!(outline.entries()[0].block, outline.entries()[1].block);
        assert_eq!(
            outline.block_for_id("doc-kapitel-1"),
            Some(outline.entries()[1].block as usize)
        );
    }

    #[test]
    fn the_active_heading_is_the_last_one_at_or_above_the_block() {
        let (outline, _) = built("# Eins\n\na\n\n## Zwei\n\nb\n");
        let second = outline.entries()[1].block as usize;
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
