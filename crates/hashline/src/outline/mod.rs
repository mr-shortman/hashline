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
//! (docs/metrics.md). An entry is
//! therefore two ranges into that blob, and the outline holds the document
//! alive for as long as it needs them.

use std::rc::Rc;

use hashline_markdown::{slug_base, OpDocument, ANCHOR_WORDS, HEADING_WORDS};

use crate::layout::{BlockKind, BlockPlan};

#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub level: u8,
    /// Index into the block plan, so a jump needs no measuring.
    pub block: u32,
    /// The generated id, as a range into the document's string blob.
    id: (u32, u32),
    /// The heading's own text, as a range into the document's *text* blob —
    /// the same range the heading block occupies, so nothing is copied.
    text: (u32, u32),
}

/// Something other than a heading that a `#fragment` can point at: a
/// footnote's definition. It is not part of the outline the reader sees.
#[derive(Clone, Copy, Debug)]
pub struct Target {
    /// The element's id, as a range into the document's string blob.
    id: (u32, u32),
    block: u32,
}

#[derive(Clone, Default)]
pub struct Outline {
    document: Rc<OpDocument>,
    entries: Vec<Entry>,
    targets: Vec<Target>,
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
            let heading = plan.block(block);
            entries.push(Entry {
                level: words[0] as u8,
                block: block as u32,
                id: (words[1], words[2]),
                text: (heading.text_start, heading.text_len),
            });
        }
        entries
    }

    /// The other link targets, from the parser's anchor table. Released by
    /// the caller along with the heading table.
    pub fn targets_of(document: &OpDocument, plan: &BlockPlan) -> Vec<Target> {
        document
            .anchors
            .as_chunks::<ANCHOR_WORDS>()
            .0
            .iter()
            .filter_map(|words| {
                let block = plan.block_for_op(words[2])?;
                Some(Target {
                    id: (words[0], words[1]),
                    block: block as u32,
                })
            })
            .collect()
    }

    pub fn new(document: Rc<OpDocument>, entries: Vec<Entry>, targets: Vec<Target>) -> Self {
        Outline {
            document,
            entries,
            targets,
        }
    }

    pub fn build(document: &Rc<OpDocument>, plan: &BlockPlan) -> Self {
        let entries = Self::entries_of(document, plan);
        let targets = Self::targets_of(document, plan);
        Outline::new(document.clone(), entries, targets)
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

    fn slice(blob: &str, (offset, len): (u32, u32)) -> &str {
        let from = offset as usize;
        blob.get(from..from + len as usize).unwrap_or("")
    }
    /// The heading's text, borrowed from the document's text blob.
    pub fn text(&self, index: usize) -> &str {
        self.entries
            .get(index)
            .map_or("", |entry| Self::slice(&self.document.text, entry.text))
    }
    /// The heading's generated id.
    pub fn id(&self, index: usize) -> &str {
        self.entries
            .get(index)
            .map_or("", |entry| Self::slice(&self.document.strings, entry.id))
    }

    /// The block carrying exactly this id: a heading's, or a footnote's.
    pub fn block_for_id(&self, id: &str) -> Option<usize> {
        let strings = &self.document.strings;
        self.entries
            .iter()
            .find(|entry| Self::slice(strings, entry.id) == id)
            .map(|entry| entry.block as usize)
            .or_else(|| {
                self.targets
                    .iter()
                    .find(|target| Self::slice(strings, target.id) == id)
                    .map(|target| target.block as usize)
            })
    }

    /// The block a link's `#fragment` points at.
    ///
    /// The id is tried as written first, which is what a footnote reference
    /// and a link copied from the outline carry. Then with the `doc-` prefix
    /// every generated heading id has, because Markdown written for GitHub
    /// links `#kapitel`. Last as the anchor the fragment's own text would get,
    /// so that `#Foo_Bar` and `#FOO-BAR` still find their heading.
    pub fn block_for_fragment(&self, fragment: &str) -> Option<usize> {
        // An empty fragment would slug to `section` and find a heading of
        // that name.
        if fragment.is_empty() {
            return None;
        }
        self.block_for_id(fragment)
            .or_else(|| self.block_for_id(&format!("doc-{fragment}")))
            .or_else(|| self.block_for_id(&format!("doc-{}", slug_base(fragment))))
    }

    /// The heading a reader is currently under, from the scroll position alone
    /// — no measuring while scrolling (docs/architecture.md).
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
    fn a_footnote_reference_finds_its_definition() {
        let source = "# Text\n\nEin Satz[^quelle].\n\n## Quelle\n\nMehr.\n\n\
                      > [^quelle]: In einem Zitat.\n";
        let (outline, plan) = built(source);
        // The reference names the footnote, not the heading of the same name.
        let block = outline
            .block_for_fragment("fn-quelle")
            .expect("the definition");
        assert!(matches!(
            plan.block(block).kind,
            crate::layout::BlockKind::Quote
        ));
        assert_eq!(
            outline.block_for_fragment("quelle"),
            outline.block_for_id("doc-quelle")
        );
        // Footnotes are link targets, not rows of the outline.
        assert_eq!(outline.len(), 2);
    }

    #[test]
    fn a_fragment_finds_its_heading_the_way_github_links_it() {
        let (outline, _) = built("# Einleitung\n\n## Kopf_zeile\n\n## Schritt 1:  Los\n");
        let heading = |index: usize| Some(outline.entries()[index].block as usize);
        assert_eq!(outline.block_for_fragment("doc-einleitung"), heading(0));
        assert_eq!(outline.block_for_fragment("einleitung"), heading(0));
        assert_eq!(outline.block_for_fragment("kopf_zeile"), heading(1));
        assert_eq!(outline.block_for_fragment("Kopf_Zeile"), heading(1));
        assert_eq!(outline.block_for_fragment("schritt-1--los"), heading(2));
        assert_eq!(outline.block_for_fragment("kopf-zeile"), None);
        assert_eq!(outline.block_for_fragment(""), None);
    }

    #[test]
    fn a_document_without_headings_has_an_empty_outline() {
        let (outline, _) = built("Nur Text.\n");
        assert!(outline.is_empty());
        assert_eq!(outline.active_for_block(0), None);
        assert_eq!(outline.block_for_id("doc-x"), None);
    }
}
