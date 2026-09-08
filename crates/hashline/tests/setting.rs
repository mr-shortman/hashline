//! Setting blocks with Pango, without a window (SPEC.md, section 12).

use hashline::layout::{set_block, BlockKind, BlockPlan, Metrics, NoImages, Style};
use hashline::theme::LIGHT;
use pango::prelude::*;

fn context() -> pango::Context {
    // A font map is all Pango needs; a widget would only provide the same.
    pangocairo::FontMap::default().create_context()
}

fn style() -> Style {
    Style::new("sans", "monospace", 17.0, LIGHT)
}

fn metrics() -> Metrics {
    Metrics {
        char_width: 8.5,
        body_px: 17.0,
    }
}

fn parse_plan(source: &str) -> (hashline_markdown::OpDocument, BlockPlan) {
    let document = hashline_markdown::parse(source);
    let plan = BlockPlan::new(&document, metrics(), 640.0);
    (document, plan)
}

const SOURCE: &str = "# Titel\n\nEin *kursiver* Absatz mit `code` und einem [Link](a.md).\n\n\
- eins\n- zwei\n\n```rust\nfn main() {}\n```\n";

#[test]
fn every_laid_out_byte_still_names_a_byte_of_the_document() {
    // The contract the whole view rests on: a position in a piece maps back to
    // the document, even where the piece contains text the document does not
    // have — a bullet, a checkbox, a separator between cells.
    let (document, plan) = parse_plan(SOURCE);
    let context = context();
    for index in 0..plan.len() {
        let block = *plan.block(index);
        let set = set_block(&context, &document, &block, &style(), 640.0, &NoImages);
        for piece in &set.pieces {
            let text = piece.layout.text();
            for (offset, _) in text.char_indices() {
                let mapped = piece
                    .map
                    .to_document(offset as u32)
                    .unwrap_or_else(|| panic!("block {index}: offset {offset} maps nowhere"));
                assert!(
                    mapped >= block.text_start && mapped <= block.text_start + block.text_len,
                    "block {index}: offset {offset} mapped to {mapped}, outside the block"
                );
            }
        }
    }
}

#[test]
fn a_set_block_has_a_real_height_and_the_estimate_is_in_the_same_league() {
    let (document, plan) = parse_plan(SOURCE);
    let context = context();
    for index in 0..plan.len() {
        let block = *plan.block(index);
        let measured = set_block(&context, &document, &block, &style(), 640.0, &NoImages).height();
        assert!(measured > 0.0, "block {index} measured {measured}");
        let estimate = block.height;
        // The estimate only has to make the scrollbar plausible, so this is a
        // loose bound — but an estimate off by an order of magnitude would make
        // scrolling lurch, and that is worth catching.
        assert!(
            measured < estimate * 10.0 + 400.0 && estimate < measured * 10.0 + 400.0,
            "block {index}: estimate {estimate}, measured {measured}"
        );
    }
}

#[test]
fn hit_testing_maps_points_across_a_wrapped_block() {
    // What the view needs is the point-to-index direction: a click anywhere in
    // the block must land on a real position, and moving right along a line
    // must never move backwards through the text.
    let (document, plan) = parse_plan("Ein Absatz mit genügend Text für mehrere Zeilen, damit das Umbrechen wirklich stattfindet und der Treffertest etwas zu tun bekommt.\n");
    let block = *plan.block(0);
    let set = set_block(&context(), &document, &block, &style(), 300.0, &NoImages);
    let piece = &set.pieces[0];
    let text = piece.layout.text();
    assert!(piece.layout.line_count() > 1, "the fixture must wrap");

    let (_, logical) = piece.layout.pixel_extents();
    for line in 0..piece.layout.line_count() {
        let Some(y) = piece.layout.line(line).map(|line| {
            let (_, extents) = line.extents();
            extents.y() + extents.height() / 2
        }) else {
            continue;
        };
        let mut previous = -1;
        for step in 0..40 {
            let x = (logical.width() * pango::SCALE * step / 40).max(0);
            let (_, index, _) = piece.layout.xy_to_index(x, y);
            assert!(
                text.is_char_boundary(index as usize),
                "index {index} splits a character"
            );
            assert!(index >= previous, "moving right moved back");
            previous = index;
        }
    }
}

#[test]
fn a_list_item_carries_its_marker_and_the_marker_maps_to_no_document_byte() {
    let (document, plan) = parse_plan("- eins\n- zwei\n");
    let set = set_block(
        &context(),
        &document,
        plan.block(0),
        &style(),
        640.0,
        &NoImages,
    );
    assert_eq!(set.pieces.len(), 2, "one piece per item");
    let first = &set.pieces[0];
    let text = first.layout.text();
    assert!(text.starts_with('•'), "marker missing: {text:?}");
    assert!(text.ends_with("eins"), "item text missing: {text:?}");
    // The first document byte of the item is where "eins" begins, not where
    // the bullet does.
    let item_start = first.map.document_range().expect("a range").0;
    let from = item_start as usize;
    assert_eq!(&document.text[from..from + 4], "eins");
}

#[test]
fn ordered_items_are_numbered_and_task_items_show_their_state() {
    let (document, plan) = parse_plan("1. eins\n2. zwei\n");
    let set = set_block(
        &context(),
        &document,
        plan.block(0),
        &style(),
        640.0,
        &NoImages,
    );
    let markers: Vec<String> = set
        .pieces
        .iter()
        .map(|piece| piece.layout.text().chars().take(2).collect())
        .collect();
    assert_eq!(markers, vec!["1.".to_string(), "2.".to_string()]);

    let (document, plan) = parse_plan("- [x] fertig\n- [ ] offen\n");
    let set = set_block(
        &context(),
        &document,
        plan.block(0),
        &style(),
        640.0,
        &NoImages,
    );
    let first: String = set.pieces[0].layout.text().chars().take(1).collect();
    let second: String = set.pieces[1].layout.text().chars().take(1).collect();
    assert_eq!((first.as_str(), second.as_str()), ("☑", "☐"));
}

#[test]
fn a_nested_list_keeps_document_order_and_deepens_its_indent() {
    let (document, plan) = parse_plan("- eins\n  - tief\n- zwei\n");
    let set = set_block(
        &context(),
        &document,
        plan.block(0),
        &style(),
        640.0,
        &NoImages,
    );
    assert_eq!(set.pieces.len(), 3);
    let starts: Vec<u32> = set
        .pieces
        .iter()
        .map(|piece| piece.map.document_range().unwrap().0)
        .collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted, "items must be in document order");
    // The nested item sits further right than either of its siblings.
    assert!(set.pieces[1].x > set.pieces[0].x);
    assert!(set.pieces[1].x > set.pieces[2].x);
}

#[test]
fn a_soft_break_is_set_as_a_space_without_moving_any_offset() {
    // In HTML a newline in text content is whitespace, and the design
    // reference is HTML. The substitution is one byte for one byte, so the
    // mapping must be untouched.
    let (document, plan) = parse_plan("Erste Zeile\nzweite Zeile.\n");
    let set = set_block(
        &context(),
        &document,
        plan.block(0),
        &style(),
        640.0,
        &NoImages,
    );
    let piece = &set.pieces[0];
    let text = piece.layout.text();
    assert!(!text.contains('\n'), "soft break survived: {text:?}");
    assert!(text.contains("Zeile zweite"), "{text:?}");
    assert_eq!(piece.layout.line_count(), 1);
    // Same length, same mapping.
    let (from, to) = piece.map.document_range().unwrap();
    assert_eq!((to - from) as usize, text.len());
}

#[test]
fn a_table_becomes_one_piece_per_cell_with_a_header_and_borders() {
    let (document, plan) = parse_plan("| A | B |\n| - | - |\n| 1 | 2 |\n");
    let set = set_block(
        &context(),
        &document,
        plan.block(0),
        &style(),
        640.0,
        &NoImages,
    );
    let texts: Vec<String> = set
        .pieces
        .iter()
        .map(|piece| piece.layout.text().to_string())
        .collect();
    assert_eq!(texts, vec!["A", "B", "1", "2"]);
    // Cells of one row share a top edge and differ in x.
    assert_eq!(set.pieces[0].y, set.pieces[1].y);
    assert!(set.pieces[1].x > set.pieces[0].x);
    assert!(set.pieces[2].y > set.pieces[0].y);
    assert!(
        !set.decorations.is_empty(),
        "a table needs its rules and header fill"
    );
}

#[test]
fn a_code_block_does_not_wrap_and_asks_for_more_width_than_the_column() {
    let long = format!("```\n{}\n```\n", "x".repeat(400));
    let (document, plan) = parse_plan(&long);
    let set = set_block(
        &context(),
        &document,
        plan.block(0),
        &style(),
        300.0,
        &NoImages,
    );
    assert_eq!(set.pieces[0].layout.line_count(), 1);
    // Wider than its column, so it scrolls inside its own block rather than
    // making the document scroll sideways.
    assert!(
        set.content_width > 300.0,
        "content width {}",
        set.content_width
    );
}

#[test]
fn a_rule_is_decoration_with_no_type_at_all() {
    let (document, plan) = parse_plan("davor\n\n---\n\ndanach\n");
    let index = (0..plan.len())
        .find(|&i| plan.block(i).kind == BlockKind::Rule)
        .expect("a rule");
    let set = set_block(
        &context(),
        &document,
        plan.block(index),
        &style(),
        640.0,
        &NoImages,
    );
    assert!(set.pieces.is_empty());
    assert_eq!(set.decorations.len(), 1);
}

/// A stand-in for the view's picture cache, so the layout can be tested
/// without touching the file system.
struct Stub(f64, f64);

impl hashline::layout::ImageSource for Stub {
    fn intrinsic(&self, source: &str) -> Option<(f64, f64)> {
        (!source.starts_with("missing")).then_some((self.0, self.1))
    }
}

#[test]
fn a_picture_on_its_own_line_becomes_a_picture_block_fitted_to_the_column() {
    let (document, plan) = parse_plan("![Ein Bild](bild.png)\n");
    // Wider than the column: scaled down, aspect kept.
    let set = set_block(
        &context(),
        &document,
        plan.block(0),
        &style(),
        640.0,
        &Stub(1280.0, 640.0),
    );
    assert!(set.pieces.is_empty(), "a picture sets no type");
    match &set.decorations[..] {
        [hashline::layout::Decoration::Image {
            width,
            height,
            source,
            ..
        }] => {
            assert_eq!(source, "bild.png");
            assert!((width - 640.0).abs() < 0.5, "width {width}");
            assert!((height - 320.0).abs() < 0.5, "height {height}");
        }
        other => panic!("expected one image decoration, got {}", other.len()),
    }
}

#[test]
fn a_picture_smaller_than_the_column_is_not_enlarged() {
    let (document, plan) = parse_plan("![Klein](klein.png)\n");
    let set = set_block(
        &context(),
        &document,
        plan.block(0),
        &style(),
        640.0,
        &Stub(80.0, 40.0),
    );
    match &set.decorations[..] {
        [hashline::layout::Decoration::Image { width, height, .. }] => {
            assert!((width - 80.0).abs() < 0.5, "width {width}");
            assert!((height - 40.0).abs() < 0.5, "height {height}");
        }
        _ => panic!("expected one image decoration"),
    }
}

#[test]
fn a_picture_that_cannot_be_shown_leaves_a_placeholder_naming_it() {
    let (document, plan) = parse_plan("![Alternativtext](missing.png)\n");
    let set = set_block(
        &context(),
        &document,
        plan.block(0),
        &style(),
        640.0,
        &Stub(100.0, 100.0),
    );
    let text = set.pieces[0].layout.text().to_string();
    assert!(text.contains("Alternativtext"), "{text}");
    assert!(text.contains("missing.png"), "{text}");
    // The placeholder keeps the height the layout reserved anyway.
    assert!(set.content_height >= 42.0, "{}", set.content_height);
}

#[test]
fn a_picture_inside_running_text_stays_running_text() {
    let (document, plan) = parse_plan("Davor ![Bild](bild.png) danach.\n");
    let set = set_block(
        &context(),
        &document,
        plan.block(0),
        &style(),
        640.0,
        &Stub(100.0, 100.0),
    );
    assert!(
        !set.pieces.is_empty(),
        "a paragraph with text is set as text"
    );
    assert!(set.pieces[0].layout.text().contains("Davor"));
}
