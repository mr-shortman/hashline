//! Where a laid-out character came from.
//!
//! Until M1 a block's layout text *was* its slice of the document, so a layout
//! offset and a document offset were the same number. The rendering contract
//! breaks that: a list item is set as `• eins`, and the bullet is not in the
//! document. Table cells are set separately, so their text is not contiguous
//! either.
//!
//! Rather than give up the exact mapping — selection, search highlighting and
//! hit-testing all depend on it — every laid-out piece carries the runs it took
//! from the document. Inserted decoration simply has no run, and therefore no
//! document position, which is the truth of the matter.

/// One stretch of document text inside a layout.
#[derive(Clone, Copy, Debug)]
struct Run {
    layout: u32,
    document: u32,
    len: u32,
}

#[derive(Clone, Debug, Default)]
pub struct TextMap {
    runs: Vec<Run>,
}

impl TextMap {
    /// A layout whose text is exactly `len` bytes of the document starting at
    /// `document` — the common case, and the one M0 assumed everywhere.
    pub fn identity(document: u32, len: u32) -> Self {
        TextMap {
            runs: vec![Run {
                layout: 0,
                document,
                len,
            }],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Moves every run forward in the layout, for text prepended in front of
    /// what has already been composed.
    pub(crate) fn shift(&mut self, by: u32) {
        for run in self.runs.iter_mut() {
            run.layout += by;
        }
    }

    pub(crate) fn push(&mut self, layout: u32, document: u32, len: u32) {
        // Runs that continue the previous one are merged, so the common case
        // stays a single entry and lookups stay trivial.
        if let Some(last) = self.runs.last_mut() {
            if last.layout + last.len == layout && last.document + last.len == document {
                last.len += len;
                return;
            }
        }
        self.runs.push(Run {
            layout,
            document,
            len,
        });
    }

    /// The document offset a layout offset stands for.
    ///
    /// An offset inside inserted decoration has no document position; it
    /// resolves to the start of the next run, so a click on a bullet selects
    /// from the beginning of the item rather than from nowhere.
    pub fn to_document(&self, layout: u32) -> Option<u32> {
        let mut fallback = None;
        for run in &self.runs {
            if layout < run.layout {
                return Some(run.document);
            }
            if layout < run.layout + run.len {
                return Some(run.document + (layout - run.layout));
            }
            fallback = Some(run.document + run.len);
        }
        fallback
    }

    /// Where a document offset sits in this layout, if it does at all.
    pub fn to_layout(&self, document: u32) -> Option<u32> {
        for run in &self.runs {
            if document < run.document {
                return Some(run.layout);
            }
            if document < run.document + run.len {
                return Some(run.layout + (document - run.document));
            }
        }
        self.runs.last().map(|run| run.layout + run.len)
    }

    /// The document range this layout covers, for deciding whether a selection
    /// touches it at all.
    pub fn document_range(&self) -> Option<(u32, u32)> {
        let first = self.runs.first()?;
        let last = self.runs.last()?;
        Some((first.document, last.document + last.len))
    }

    /// Clips a document range to this layout and returns it in layout
    /// coordinates.
    pub fn clip(&self, from: u32, to: u32) -> Option<(u32, u32)> {
        let (start, end) = self.document_range()?;
        let from = from.max(start);
        let to = to.min(end);
        if from >= to {
            return None;
        }
        Some((self.to_layout(from)?, self.to_layout(to)?))
    }
}

#[cfg(test)]
mod tests {
    use super::TextMap;

    #[test]
    fn an_identity_map_is_a_shift_by_the_block_start() {
        let map = TextMap::identity(100, 10);
        assert_eq!(map.to_document(0), Some(100));
        assert_eq!(map.to_document(9), Some(109));
        assert_eq!(map.to_layout(105), Some(5));
        assert_eq!(map.document_range(), Some((100, 110)));
    }

    #[test]
    fn inserted_decoration_has_no_document_position() {
        // "• " inserted, then six bytes of document text at 40.
        let mut map = TextMap::default();
        map.push(2, 40, 6);
        // Inside the bullet: resolves forward to where the item begins.
        assert_eq!(map.to_document(0), Some(40));
        assert_eq!(map.to_document(1), Some(40));
        assert_eq!(map.to_document(2), Some(40));
        assert_eq!(map.to_document(5), Some(43));
        assert_eq!(map.to_layout(40), Some(2));
        assert_eq!(map.to_layout(43), Some(5));
    }

    #[test]
    fn runs_that_continue_each_other_are_merged() {
        let mut map = TextMap::default();
        map.push(0, 10, 4);
        map.push(4, 14, 6);
        assert_eq!(map.document_range(), Some((10, 20)));
        assert_eq!(map.to_document(9), Some(19));
    }

    #[test]
    fn a_gap_in_the_document_stays_a_gap() {
        // Two table cells in one layout: text at 10 and at 50.
        let mut map = TextMap::default();
        map.push(0, 10, 3);
        map.push(4, 50, 3);
        assert_eq!(map.to_document(1), Some(11));
        // The separator between them belongs to neither cell.
        assert_eq!(map.to_document(3), Some(50));
        assert_eq!(map.to_layout(50), Some(4));
    }

    #[test]
    fn clipping_a_selection_range_yields_layout_coordinates() {
        let map = TextMap::identity(100, 10);
        assert_eq!(map.clip(102, 106), Some((2, 6)));
        // Beyond the layout on both sides clamps to it.
        assert_eq!(map.clip(0, 500), Some((0, 10)));
        // Entirely outside is not a hit.
        assert_eq!(map.clip(200, 300), None);
    }
}
