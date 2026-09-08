//! Document-wide text search (SPEC.md, section 8).
//!
//! The search runs on the parser's text blob, not on laid-out type. That is
//! what makes it independent of how much of the document has been set: a hit in
//! a block that has never been measured is found just as readily as one on
//! screen, and the jump to it goes through the block plan.
//!
//! It deliberately allocates nothing proportional to the document. Folding the
//! whole blob to lower case and keeping an offset table beside it would be the
//! obvious implementation and would cost, for the 10 MiB fixture, another
//! 10 MiB of text plus 40 MiB of offsets — which is the entire memory budget
//! from SPEC.md section 9 spent on a feature that is idle most of the time.
//! Instead each candidate position is compared with folding applied on the fly.

/// A hit, as byte offsets into the document text blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hit {
    pub start: u32,
    pub end: u32,
}

/// A prepared query. `None` for an empty one, which matches nothing rather
/// than everything.
#[derive(Clone, Debug)]
pub struct Query {
    folded: Vec<char>,
}

/// Case folding, one character at a time.
///
/// This handles case and every accented letter, which is what a reader
/// searching a document expects. It does **not** perform the expanding folds —
/// `ß` does not match `ss`, and `ﬁ` does not match `fi` — because those change
/// the byte length and would require the offset table this module exists to
/// avoid. The limitation is recorded rather than hidden.
fn fold(character: char) -> char {
    let mut lowered = character.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => single,
        // An expanding fold: keep the original so the comparison stays
        // one-to-one instead of silently matching a prefix.
        _ => character,
    }
}

impl Query {
    pub fn new(needle: &str) -> Option<Self> {
        let folded: Vec<char> = needle.chars().map(fold).collect();
        (!folded.is_empty()).then_some(Query { folded })
    }

    pub fn len(&self) -> usize {
        self.folded.len()
    }
    pub fn is_empty(&self) -> bool {
        self.folded.is_empty()
    }

    /// Every hit in `text`, in document order.
    ///
    /// Hits do not overlap: the scan continues after a hit, so searching `aa`
    /// in `aaaa` finds two, not three.
    pub fn matches(&self, text: &str) -> Vec<Hit> {
        let mut hits = Vec::new();
        let first = self.folded[0];
        let mut cursor = 0usize;
        while cursor < text.len() {
            let rest = &text[cursor..];
            let Some((offset, character)) = rest
                .char_indices()
                .find(|&(_, character)| fold(character) == first)
            else {
                break;
            };
            let start = cursor + offset;
            if let Some(end) = self.match_at(text, start) {
                hits.push(Hit {
                    start: start as u32,
                    end: end as u32,
                });
                cursor = end;
            } else {
                cursor = start + character.len_utf8();
            }
        }
        hits
    }

    /// The end of a match beginning exactly at `start`, if there is one.
    fn match_at(&self, text: &str, start: usize) -> Option<usize> {
        let mut characters = text[start..].chars();
        let mut end = start;
        for &wanted in &self.folded {
            let character = characters.next()?;
            if fold(character) != wanted {
                return None;
            }
            end += character.len_utf8();
        }
        Some(end)
    }
}

/// Which hit follows, or precedes, a position — wrapping around the document
/// so that `Enter` on the last hit returns to the first.
pub fn next_from(hits: &[Hit], offset: u32) -> Option<usize> {
    if hits.is_empty() {
        return None;
    }
    Some(hits.iter().position(|hit| hit.start > offset).unwrap_or(0))
}

pub fn previous_from(hits: &[Hit], offset: u32) -> Option<usize> {
    if hits.is_empty() {
        return None;
    }
    Some(
        hits.iter()
            .rposition(|hit| hit.start < offset)
            .unwrap_or(hits.len() - 1),
    )
}

#[cfg(test)]
mod tests {
    use super::{next_from, previous_from, Query};

    fn found(needle: &str, haystack: &str) -> Vec<String> {
        Query::new(needle)
            .map(|query| {
                query
                    .matches(haystack)
                    .into_iter()
                    .map(|hit| haystack[hit.start as usize..hit.end as usize].to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        assert!(Query::new("").is_none());
        assert_eq!(found("", "irgendwas"), Vec::<String>::new());
    }

    #[test]
    fn matching_ignores_case_on_both_sides() {
        assert_eq!(
            found("hashline", "Hashline und HASHLINE"),
            ["Hashline", "HASHLINE"]
        );
        assert_eq!(found("GRÜSSE", "grüsse"), ["grüsse"]);
    }

    #[test]
    fn hits_come_back_in_document_order_and_do_not_overlap() {
        assert_eq!(found("aa", "aaaa"), ["aa", "aa"]);
        let hits = Query::new("a").unwrap().matches("banana");
        assert_eq!(
            hits.iter().map(|hit| hit.start).collect::<Vec<_>>(),
            vec![1, 3, 5]
        );
    }

    #[test]
    fn offsets_are_byte_offsets_that_slice_the_text() {
        // The needle sits behind multi-byte characters, so a character count
        // would land in the wrong place.
        let text = "Grüße über äöü Ziel";
        let hits = Query::new("ziel").unwrap().matches(text);
        assert_eq!(hits.len(), 1);
        assert_eq!(&text[hits[0].start as usize..hits[0].end as usize], "Ziel");
    }

    #[test]
    fn a_match_never_runs_across_a_block_separator() {
        // The blob separates blocks with a newline exactly so that this cannot
        // happen (SPEC.md, section 6).
        assert_eq!(found("endeanfang", "Ende\nAnfang"), Vec::<String>::new());
        assert_eq!(found("ende", "Ende\nAnfang"), ["Ende"]);
    }

    #[test]
    fn an_expanding_fold_is_not_performed_and_says_so() {
        // Documented limitation: ß does not match ss. It must not match a
        // prefix by accident either.
        assert_eq!(found("strasse", "Straße"), Vec::<String>::new());
        assert_eq!(found("straße", "STRASSE"), Vec::<String>::new());
        assert_eq!(found("straße", "Straße"), ["Straße"]);
    }

    #[test]
    fn stepping_through_hits_wraps_around() {
        let text = "a b a b a";
        let hits = Query::new("a").unwrap().matches(text);
        assert_eq!(hits.len(), 3);
        assert_eq!(next_from(&hits, 0), Some(1));
        assert_eq!(next_from(&hits, hits[2].start), Some(0), "wraps forward");
        assert_eq!(previous_from(&hits, hits[2].start), Some(1));
        assert_eq!(previous_from(&hits, 0), Some(2), "wraps backward");
        assert_eq!(next_from(&[], 0), None);
    }

    #[test]
    fn a_query_longer_than_the_text_finds_nothing() {
        assert_eq!(found("suchbegriff", "kurz"), Vec::<String>::new());
    }
}
