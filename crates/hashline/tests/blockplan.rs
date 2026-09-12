//! The block plan's two load-bearing properties (docs/architecture.md).
//!
//! Risk 1 of M0 is that a virtualized viewer jumps when an estimate is replaced
//! by a measurement. These tests pin the arithmetic that prevents it; the view
//! only has to apply it.

use hashline::layout::{BlockKind, BlockPlan, Metrics};
use hashline::theme::document;

fn metrics() -> Metrics {
    Metrics {
        char_width: 8.5,
        body_px: 17.0,
    }
}

fn plan(source: &str) -> BlockPlan {
    let document = hashline_markdown::parse(source);
    BlockPlan::new(&document, metrics(), 646.0)
}

const SOURCE: &str =
    "# Titel\n\nEin Absatz.\n\n## Kapitel\n\nNoch einer.\n\n- eins\n- zwei\n\n---\n\nSchluss.\n";

#[test]
fn the_plan_covers_every_block_of_the_document() {
    let plan = plan(SOURCE);
    assert_eq!(plan.len(), 7);
    let kinds: Vec<BlockKind> = (0..plan.len()).map(|i| plan.block(i).kind).collect();
    assert_eq!(
        kinds,
        vec![
            BlockKind::Heading(1),
            BlockKind::Paragraph,
            BlockKind::Heading(2),
            BlockKind::Paragraph,
            BlockKind::List,
            BlockKind::Rule,
            BlockKind::Paragraph,
        ]
    );
}

#[test]
fn positions_are_consistent_with_the_heights_held() {
    let plan = plan(SOURCE);
    let mut expected = document::PAD_TOP;
    for index in 0..plan.len() {
        assert!(
            (plan.y_of(index) - expected).abs() < 1e-9,
            "block {index} sits at {} not {expected}",
            plan.y_of(index)
        );
        expected += plan.block(index).height();
    }
    assert!((plan.total_height() - (expected + document::PAD_BOTTOM)).abs() < 1e-9);
}

#[test]
fn a_measurement_above_the_reading_position_moves_everything_below_by_its_delta() {
    // This is the anti-jump contract. The view holds the visible text still by
    // adding the returned delta to its scroll offset; if the two ever disagree,
    // the document jumps while scrolling upwards.
    let mut plan = plan(SOURCE);
    let reading = 4;
    let before = plan.y_of(reading);
    let estimate = plan.block(1).height();
    let delta = plan.set_measured(1, estimate + 23.0);
    assert!((delta - 23.0).abs() < 1e-9);
    assert!((plan.y_of(reading) - (before + delta)).abs() < 1e-9);
    // Blocks above the change do not move.
    assert!((plan.y_of(1) - (document::PAD_TOP + plan.block(0).height())).abs() < 1e-9);
}

#[test]
fn a_measurement_below_the_reading_position_leaves_it_where_it_was() {
    let mut plan = plan(SOURCE);
    let reading = 1;
    let before = plan.y_of(reading);
    plan.set_measured(5, plan.block(5).height() + 40.0);
    assert_eq!(plan.y_of(reading), before);
}

#[test]
fn measuring_marks_the_block_and_shrinking_works_too() {
    let mut plan = plan(SOURCE);
    assert!(!plan.block(3).measured());
    let delta = plan.set_measured(3, 5.0);
    assert!(plan.block(3).measured());
    assert!(delta < 0.0, "an over-estimate must report a negative delta");
    assert_eq!(plan.block(3).height, 5.0);
}

#[test]
fn the_visible_range_holds_a_screen_of_buffer_on_each_side() {
    let plan = plan(&"Ein Absatz.\n\n".repeat(400));
    let viewport = 600.0;
    let top = plan.y_of(200);
    let range = plan.visible_range(top, viewport);
    // Everything on screen is in the range …
    assert!(range.contains(&200));
    assert!(range.contains(&plan.block_at(top + viewport - 1.0)));
    // … and the range reaches beyond the screen in both directions.
    assert!(plan.y_of(range.start) < top);
    assert!(plan.y_of(range.end - 1) > top + viewport);
    assert!(range.end <= plan.len());
}

#[test]
fn a_reflow_drops_measurements_and_re_estimates() {
    let mut plan = plan(SOURCE);
    plan.set_measured(1, 999.0);
    plan.reflow(metrics(), 320.0);
    assert!(!plan.block(1).measured());
    assert_ne!(plan.block(1).height, 999.0);
    // A wider column cannot make the document taller.
    let narrow = plan.total_height();
    plan.reflow(metrics(), 1200.0);
    assert!(plan.total_height() <= narrow);
}

#[test]
fn a_text_offset_resolves_to_the_block_that_holds_it() {
    let plan = plan(SOURCE);
    for index in 0..plan.len() {
        let block = *plan.block(index);
        if block.text_len == 0 {
            continue;
        }
        assert_eq!(plan.block_for_text(block.text_start), Some(index));
        assert_eq!(
            plan.block_for_text(block.text_start + block.text_len - 1),
            Some(index)
        );
    }
}

#[test]
fn a_large_document_is_planned_without_measuring_anything() {
    // The stress fixture; the plan must exist before a single block is laid out
    // (docs/architecture.md).
    let source = "## Kapitel\n\nEin Absatz mit etwas Text darin.\n\n".repeat(60_000);
    let started = std::time::Instant::now();
    let document = hashline_markdown::parse(&source);
    let parsed = started.elapsed();
    let started = std::time::Instant::now();
    let plan = BlockPlan::new(&document, metrics(), 646.0);
    let planned = started.elapsed();
    assert_eq!(plan.len(), 120_000);
    assert!(plan.total_height() > 0.0);
    assert!((0..plan.len()).all(|i| !plan.block(i).measured()));
    println!(
        "{} MiB source: parse {parsed:?}, plan {planned:?}, {} blocks",
        source.len() / (1024 * 1024),
        plan.len()
    );
}

// Blocks the plan has to cut up, because setting them whole is what
// virtualization exists to avoid (docs/architecture.md).

/// One fenced code block of `lines` lines.
fn code_document(lines: usize) -> String {
    format!("```text\n{}```\n", "eine Zeile Code\n".repeat(lines))
}

#[test]
fn a_code_block_is_estimated_by_its_lines_and_not_as_a_single_one() {
    // The estimate used to take every code block for one line high. On a
    // document of two-line blocks that makes the scrollbar half as long as it
    // should be; on one 70 000 line block it makes it 28 000 times too short,
    // and every jump into the document lands somewhere else.
    let short = plan(&code_document(1));
    let long = plan(&code_document(400));
    let line = 17.0 * document::INLINE_CODE_EM * document::CODE_LINE_HEIGHT;
    let grown = long.total_height() - short.total_height();
    assert!(
        (grown - 399.0 * line).abs() < 1.0,
        "399 further lines added {grown} instead of {}",
        399.0 * line
    );
}

#[test]
fn a_code_block_too_large_to_set_at_once_becomes_parts_covering_it_exactly() {
    let source = code_document(2000);
    let document = hashline_markdown::parse(&source);
    let plan = BlockPlan::new(&document, metrics(), 646.0);
    assert!(plan.len() > 4, "2000 lines stayed {} blocks", plan.len());

    let indices: Vec<usize> = (0..plan.len())
        .filter(|&index| plan.block(index).kind == BlockKind::Code)
        .collect();
    let parts: Vec<_> = indices.iter().map(|&index| *plan.block(index)).collect();
    // The parts are contiguous, cover the block's text exactly, and each holds
    // whole lines — a part that began mid-line would set differently than the
    // same line does inside the whole block.
    let first = parts.first().unwrap();
    let last = parts.last().unwrap();
    let mut cursor = first.text_start;
    for (offset, part) in parts.iter().enumerate() {
        assert_eq!(
            plan.lines(indices[offset]) as usize,
            document.text[part.text_start as usize..(part.text_start + part.text_len) as usize]
                .matches('\n')
                .count()
        );
        assert_eq!(part.text_start, cursor);
        assert!(part.text_len > 0);
        let text =
            &document.text[part.text_start as usize..(part.text_start + part.text_len) as usize];
        assert!(text.ends_with('\n'), "a part must end at a line boundary");

        cursor += part.text_len;
    }
    assert_eq!(cursor, last.text_start + last.text_len);
    assert_eq!(
        cursor as usize - first.text_start as usize,
        source.len() - "```text\n```\n".len()
    );

    // Only the outer parts are outer.
    assert!(first.is_first() && !first.is_last());
    assert!(last.is_last() && !last.is_first());
    assert!(parts[1..parts.len() - 1]
        .iter()
        .all(|part| !part.is_first() && !part.is_last()));
}

#[test]
fn a_paragraph_too_large_to_set_at_once_is_cut_between_words() {
    let word = "Wortfolge mit Leerzeichen und etwas Text darin. ";
    let source = word.repeat(1000);
    let document = hashline_markdown::parse(&source);
    let plan = BlockPlan::new(&document, metrics(), 646.0);
    assert!(
        plan.len() > 4,
        "a 47 KiB paragraph stayed {} blocks",
        plan.len()
    );

    let mut cursor = 0;
    for index in 0..plan.len() {
        let block = plan.block(index);
        assert_eq!(block.text_start, cursor);
        let text =
            &document.text[block.text_start as usize..(block.text_start + block.text_len) as usize];
        // Text with spaces in it is never cut inside a word.
        if !block.is_first() {
            assert!(!text.starts_with(' '), "a part must begin at a word");
            assert!(document.text[..block.text_start as usize].ends_with(' '));
        }
        cursor += block.text_len;
    }
    assert_eq!(cursor as usize, document.text.len());
}

#[test]
fn text_without_a_single_space_is_still_cut_up() {
    // The fixture that took 172 ms and held one Pango layout over a million
    // characters. There is no word boundary anywhere in it.
    let source = "abcdefghij".repeat(20_000);
    let document = hashline_markdown::parse(&source);
    let plan = BlockPlan::new(&document, metrics(), 646.0);
    assert!(
        plan.len() > 20,
        "{} blocks for 200 000 characters",
        plan.len()
    );
    assert!((0..plan.len()).all(|index| plan.block(index).text_len <= 2048));
}

#[test]
fn the_parts_of_one_block_know_they_belong_together() {
    let source = format!("Kurz.\n\n{}\n", code_document(2000));
    let document = hashline_markdown::parse(&source);
    let plan = BlockPlan::new(&document, metrics(), 646.0);
    // The short paragraph is alone in its range; every part of the code block
    // resolves to the same range and to the same text.
    assert_eq!(plan.source_blocks(0), 0..1);
    let expected = plan.source_blocks(1);
    assert!(expected.len() > 1);
    let range = plan.source_text_range(1);
    for index in expected.clone() {
        assert_eq!(plan.source_blocks(index), expected);
        assert_eq!(plan.source_text_range(index), range);
    }
    assert_eq!(range.0, plan.block(expected.start).text_start);
    let last = plan.block(expected.end - 1);
    assert_eq!(range.1, last.text_start + last.text_len);
}

#[test]
fn only_the_outer_parts_carry_the_space_around_a_block() {
    let source = format!("{}\nDanach.\n", code_document(2000));
    let document = hashline_markdown::parse(&source);
    let plan = BlockPlan::new(&document, metrics(), 646.0);
    let parts = plan.source_blocks(0);
    assert!(parts.len() > 2);
    // Every middle part is exactly its lines high: no padding, no spacing.
    let line = 17.0 * document::INLINE_CODE_EM * document::CODE_LINE_HEIGHT;
    for index in parts.start + 1..parts.end - 1 {
        let block = plan.block(index);
        assert!(
            (block.height() - plan.lines(index) as f64 * line).abs() < 1.0,
            "part {index} is {} high for {} lines",
            block.height,
            plan.lines(index)
        );
    }
}

#[test]
fn a_list_or_a_table_is_never_cut_up() {
    // Their parts are items and rows, which the plan addresses by text range
    // and therefore cannot cut through (docs/limitations.md).
    let list = plan(&"- ein Punkt mit etwas Text darin\n".repeat(500));
    assert_eq!(list.len(), 1);
    let row = format!("|{}\n", " Zelle |".repeat(60));
    let table = plan(&format!(
        "{row}|{}\n{}",
        " --- |".repeat(60),
        row.repeat(60)
    ));
    assert_eq!(
        table.len(),
        1,
        "the wide table became {} blocks",
        table.len()
    );
}

#[test]
fn a_cut_never_lands_inside_a_character() {
    // The limit falling inside a multi-byte character is the normal case, not
    // the exception: slicing there would panic, and the panic would be in the
    // document that happens to be long and not in English.
    for filler in ["Grüße über Grüße ", "🌍🌍🌍🌍", "日本語のテキストです"] {
        let source = filler.repeat(40_000);
        let document = hashline_markdown::parse(&source);
        let plan = BlockPlan::new(&document, metrics(), 646.0);
        assert!(plan.len() > 1, "{filler:?} stayed one block");
        let mut cursor = 0;
        for index in 0..plan.len() {
            let block = plan.block(index);
            assert_eq!(block.text_start, cursor);
            // Slicing is what would have panicked.
            let text = &document.text
                [block.text_start as usize..(block.text_start + block.text_len) as usize];
            assert!(!text.is_empty());
            cursor += block.text_len;
        }
        assert_eq!(cursor as usize, document.text.len());
    }
}
