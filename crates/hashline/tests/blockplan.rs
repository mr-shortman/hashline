//! The block plan's two load-bearing properties (SPEC.md, section 5).
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
        expected += plan.block(index).height;
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
    let estimate = plan.block(1).height;
    let delta = plan.set_measured(1, estimate + 23.0);
    assert!((delta - 23.0).abs() < 1e-9);
    assert!((plan.y_of(reading) - (before + delta)).abs() < 1e-9);
    // Blocks above the change do not move.
    assert!((plan.y_of(1) - (document::PAD_TOP + plan.block(0).height)).abs() < 1e-9);
}

#[test]
fn a_measurement_below_the_reading_position_leaves_it_where_it_was() {
    let mut plan = plan(SOURCE);
    let reading = 1;
    let before = plan.y_of(reading);
    plan.set_measured(5, plan.block(5).height + 40.0);
    assert_eq!(plan.y_of(reading), before);
}

#[test]
fn measuring_marks_the_block_and_shrinking_works_too() {
    let mut plan = plan(SOURCE);
    assert!(!plan.block(3).measured);
    let delta = plan.set_measured(3, 5.0);
    assert!(plan.block(3).measured);
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
    assert!(!plan.block(1).measured);
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
    // (SPEC.md, sections 5 and 9).
    let source = "## Kapitel\n\nEin Absatz mit etwas Text darin.\n\n".repeat(60_000);
    let started = std::time::Instant::now();
    let document = hashline_markdown::parse(&source);
    let parsed = started.elapsed();
    let started = std::time::Instant::now();
    let plan = BlockPlan::new(&document, metrics(), 646.0);
    let planned = started.elapsed();
    assert_eq!(plan.len(), 120_000);
    assert!(plan.total_height() > 0.0);
    assert!((0..plan.len()).all(|i| !plan.block(i).measured));
    println!(
        "{} MiB source: parse {parsed:?}, plan {planned:?}, {} blocks",
        source.len() / (1024 * 1024),
        plan.len()
    );
}
