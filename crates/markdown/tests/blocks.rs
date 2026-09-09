//! The block plan's contract (SPEC.md, sections 5 and 6).
//!
//! The renderer virtualizes over these blocks: it measures only the visible
//! ones and trusts the recorded ranges for everything else. If a block's op
//! range or text range were wrong, the error would show as a jumping viewport
//! or a search hit landing in the wrong place — both expensive to debug from
//! the view. They are cheap to pin down here.

mod support;

const SOURCE: &str = "# Titel\n\n\
Ein Absatz mit *Betonung*.\n\n\
- eins\n- zwei\n  - tiefer\n\n\
> Ein Zitat.\n\n\
| A | B |\n| - | - |\n| 1 | 2 |\n\n\
```rust\nfn main() {}\n```\n\n\
---\n\n\
Letzter Absatz.\n";

#[test]
fn every_operation_belongs_to_exactly_one_block() {
    let doc = support::parse(SOURCE);
    assert!(doc.block_count() > 5, "expected several blocks");
    let mut next_op = 0;
    for index in 0..doc.block_count() {
        let block = doc.block(index);
        assert_eq!(
            block[1], next_op,
            "block {index} must start where the last ended"
        );
        assert!(block[2] > 0, "block {index} is empty");
        next_op = block[1] + block[2];
    }
    assert_eq!(next_op as usize, doc.document.ops.len());
}

#[test]
fn block_text_ranges_are_ordered_and_address_the_blob() {
    let doc = support::parse(SOURCE);
    let mut end = 0;
    for index in 0..doc.block_count() {
        let block = doc.block(index);
        let (start, length) = (block[3], block[4]);
        assert!(start >= end, "block {index} text must not go backwards");
        // Panics unless the range lands on character boundaries.
        let text = doc.text.slice(start, length);
        assert!(text.len() as u32 == length);
        end = start + length;
    }
    assert!(end as usize <= doc.text.len());
}

#[test]
fn a_blocks_text_range_contains_the_text_its_operations_reference() {
    let doc = support::parse(SOURCE);
    for index in 0..doc.block_count() {
        let block = doc.block(index);
        let (op_start, op_count) = (block[1] as usize, block[2] as usize);
        let (start, length) = (block[3], block[4]);
        for (offset, run) in support::text_runs_in(&doc.document.ops[op_start..op_start + op_count])
        {
            assert!(
                offset >= start && offset + run <= start + length,
                "block {index}: text run {offset}+{run} escapes {start}+{length}"
            );
        }
    }
}

#[test]
fn a_list_and_a_table_are_each_one_block() {
    let doc = support::parse(SOURCE);
    let tags: Vec<&str> = (0..doc.block_count())
        .map(|index| hashline_markdown::ALLOWED_TAGS[doc.block(index)[0] as usize])
        .collect();
    // A nested list does not open a block of its own, and a table is one block
    // however many rows it has: the view scrolls a table inside its own block.
    assert_eq!(
        tags,
        vec!["h1", "p", "ul", "blockquote", "table", "pre", "hr", "p"],
        "top-level flow elements, in document order"
    );
}

#[test]
fn a_large_document_yields_blocks_without_measuring_anything() {
    // The plan is built for the whole document in one pass; only the visible
    // blocks are ever laid out (SPEC.md, section 5).
    let doc = support::parse(&"Ein Absatz.\n\n".repeat(20_000));
    assert_eq!(doc.block_count(), 20_000);
    let last = doc.block(doc.block_count() - 1);
    assert_eq!(doc.text.slice(last[3], last[4]), "Ein Absatz.");
}
