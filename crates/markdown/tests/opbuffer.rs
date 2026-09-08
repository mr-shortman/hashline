//! The op buffer's own contract, ported from `tests/opbuffer.test.ts`.
//!
//! These are the properties the renderer relies on and the parser tests alone
//! would not catch: that offsets address what they claim to, that the document
//! text is contiguous and block-separated, and that sections stay identifiable.

mod support;

use support::html::canonical;

#[test]
fn carries_characters_outside_the_bmp_through_the_offsets() {
    // An emoji is two UTF-16 units and four UTF-8 bytes, so any confusion of
    // the two shows up here whichever unit the offsets count.
    let doc = support::parse("# 😀 Ende\n\nText 😀 mit ü und 𝄞 dahinter.\n");
    let html = canonical(&doc.render());
    assert!(html.contains("😀 Ende"), "{html}");
    assert!(html.contains("Text 😀 mit ü und 𝄞 dahinter."), "{html}");
    // The slug rule keeps letters, numbers and whitespace; the emoji is a
    // symbol and disappears.
    assert_eq!(doc.heading(0).1, "doc-ende");
}

#[test]
fn encodes_an_empty_document_without_operations_or_sections() {
    let doc = support::parse("");
    assert_eq!(doc.section_count(), 0);
    assert!(doc.document.ops.is_empty());
    assert!(doc.document.sections.is_empty());
    assert!(doc.document.headings.is_empty());
}

#[test]
fn reports_headings_with_level_text_and_section() {
    let source = format!("# Eins\n\n{}## Zwei\n", "Absatz.\n\n".repeat(4000));
    let doc = support::parse(&source);
    assert_eq!(doc.heading_count(), 2);
    assert_eq!(
        (doc.heading(0).0, doc.heading(0).1, doc.heading(0).2),
        (1, "doc-eins".to_string(), "Eins".to_string())
    );
    assert_eq!(
        (doc.heading(1).0, doc.heading(1).1, doc.heading(1).2),
        (2, "doc-zwei".to_string(), "Zwei".to_string())
    );
    // The second heading is far enough in to have landed in a later section.
    assert!(doc.heading(1).3 > 0, "expected a later section");
}

#[test]
fn gives_identical_content_the_same_section_key() {
    let block = "Ein Absatz mit **Inhalt**.\n\n".repeat(400);
    let a = support::parse(&block);
    let b = support::parse(&block);
    let c = support::parse(&block.replace("Inhalt", "Anderes"));
    assert!(a.section_count() > 1);
    assert_eq!(a.section_key(0), b.section_key(0));
    assert_ne!(a.section_key(0), c.section_key(0));
}

#[test]
fn replays_tables_task_lists_code_languages_and_links() {
    let html = canonical(
        &support::parse(
            "| A | B |\n| :-- | --: |\n| 1 | 2 |\n\n\
             - [x] erledigt\n- [ ] offen\n\n\
             ```js\nconst a = 1;\n```\n\n\
             [Text](ziel.md \"Titel\")\n",
        )
        .render(),
    );
    assert!(html.contains("<th align=\"left\">"), "{html}");
    assert!(html.contains("<th align=\"right\">"), "{html}");
    assert!(
        html.contains("<tbody><tr><td align=\"left\">1</td>"),
        "{html}"
    );
    assert_eq!(html.matches("type=\"checkbox\"").count(), 2);
    assert!(html.contains("checked=\"\""), "{html}");
    assert!(html.contains("<code class=\"language-js\">"), "{html}");
    assert!(html.contains("title=\"Titel\""), "{html}");
}

#[test]
fn keeps_the_document_text_contiguous_in_order_and_block_separated() {
    let doc = support::parse(
        "# Titel\n\nfoo\n\nbar\n\n- eins\n- zwei\n\n| a | b |\n| - | - |\n| c | d |\n",
    );
    // A separator between blocks is what keeps a match from running from the
    // end of one block into the start of the next.
    assert_eq!(doc.document.text, "Titel\nfoo\nbar\neins\nzwei\na\nb\nc\nd");
    assert!(!doc.document.text.contains("foobar"));
}

#[test]
fn maps_every_text_offset_to_the_run_it_addresses() {
    let doc = support::parse("## Kapitel\n\nEin *kursiver* Absatz mit `code` und 😀.\n\n> Zitat\n");
    let runs = doc.text_runs();
    assert!(runs.len() > 3, "expected several text runs");
    let mut previous = 0;
    for (offset, length) in runs {
        // Slicing panics if the range is not addressable at all.
        let value = doc.text.slice(offset, length);
        assert!(!value.is_empty());
        // Ascending, so a binary search over the offsets is valid.
        assert!(offset >= previous, "offsets must not go backwards");
        previous = offset;
    }
}

#[test]
fn assigns_every_section_a_text_range_covering_the_blob() {
    let source = "## Abschnitt\n\nEin Absatz mit Text.\n\n".repeat(360);
    let doc = support::parse(&source);
    assert!(doc.section_count() > 1);
    let mut end = 0;
    for index in 0..doc.section_count() {
        let (start, stop) = doc.section_text_range(index);
        assert_eq!(
            start, end,
            "section {index} must continue where the last ended"
        );
        end = stop;
    }
    assert_eq!(end as usize, doc.text.len());
}
