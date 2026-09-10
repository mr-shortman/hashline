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
        // Controls — the copy affordance on a code block — are not document
        // text and deliberately map nowhere.
        for piece in set.pieces.iter().filter(|piece| !piece.control) {
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
        let estimate = block.height();
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
    let mut iter = piece.layout.iter();
    loop {
        let (_, extents) = iter.line_extents();
        let y = extents.y() + extents.height() / 2;
        let line = iter.line_readonly().unwrap();
        let mut previous = -1;
        for step in 0..40 {
            let x = (logical.width() * pango::SCALE * step / 40).max(0);
            let (_, index, _) = piece.layout.xy_to_index(x, y);
            assert!(
                text.is_char_boundary(index as usize),
                "index {index} splits a character"
            );
            assert!(index >= previous, "moving right moved back");
            assert!(index >= line.start_index() && index <= line.start_index() + line.length());
            previous = index;
        }
        if !iter.next_line() {
            break;
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

// Blocks the plan cut into parts. Setting a part has to give what the same
// stretch of the block gives inside the whole (SPEC.md, section 5).

fn piece_text(set: &hashline::layout::BlockLayout) -> String {
    set.pieces
        .iter()
        .filter(|piece| !piece.control)
        .map(|piece| piece.layout.text().to_string())
        .collect()
}

#[test]
fn the_parts_of_a_code_block_set_the_same_type_as_the_whole_block() {
    let source = format!("```text\n{}```\n", "eine Zeile Code\n".repeat(2000));
    let (document, plan) = parse_plan(&source);
    let context = context();
    let parts = plan.source_blocks(0);
    assert!(parts.len() > 1, "the block was not cut up");

    let entire = set_block(
        &context,
        &document,
        &plan.whole(0),
        &style(),
        640.0,
        &NoImages,
    );
    let mut set_parts: Vec<String> = Vec::new();
    let mut height = 0.0;
    for index in parts.clone() {
        let block = *plan.block(index);
        let set = set_block(&context, &document, &block, &style(), 640.0, &NoImages);
        // The panel's padding, its copy control and the space under the block
        // belong to the outer parts only, so the parts stack into one panel.
        assert_eq!(
            set.pieces.iter().filter(|piece| piece.control).count(),
            usize::from(block.is_first())
        );
        assert_eq!(set.space_after > 0.0, block.is_last());
        set_parts.push(piece_text(&set));
        height += set.height();
    }
    // Each part drops the newline that closed its last line, exactly as the
    // whole block does — the parts are stacked, not concatenated. Put those
    // newlines back and the two are the same 2000 lines.
    let text = set_parts.join("\n");
    assert_eq!(text.lines().count(), 2000);
    assert_eq!(text, piece_text(&entire));
    // Pango reports a layout's extents in whole pixels, so each part rounds
    // once where the whole block rounded once — the only difference there is.
    assert!(
        (height - entire.height()).abs() <= parts.len() as f64,
        "parts stack to {height} over {} parts, the whole block is {} high",
        parts.len(),
        entire.height()
    );
}

#[test]
fn the_parts_of_a_paragraph_carry_its_text_once_and_in_order() {
    let word = "Wortfolge mit Leerzeichen und etwas Text darin. ";
    let source = word.repeat(1000);
    let (document, plan) = parse_plan(&source);
    let context = context();
    let parts = plan.source_blocks(0);
    assert!(parts.len() > 1, "the block was not cut up");

    let mut text = String::new();
    for index in parts.clone() {
        let block = *plan.block(index);
        let set = set_block(&context, &document, &block, &style(), 640.0, &NoImages);
        text.push_str(&piece_text(&set));
        // Every byte of a part still names a byte of the document, and it is a
        // byte of that part.
        for piece in set.pieces.iter().filter(|piece| !piece.control) {
            let (from, to) = piece.map.document_range().unwrap();
            assert!(from >= block.text_start);
            assert!(to <= block.text_start + block.text_len);
        }
    }
    assert_eq!(text, document.text);
}

#[test]
fn setting_one_part_of_a_huge_code_block_is_not_setting_the_block() {
    // `large-code.md` in miniature. Setting the whole block took 67 seconds on
    // the reference machine because a code block was one block of the plan;
    // what the view now sets for one screen is a part.
    let source = format!("```text\n{}```\n", "eine Zeile Code\n".repeat(70_000));
    let (document, plan) = parse_plan(&source);
    let context = context();
    let parts = plan.source_blocks(0);
    assert!(
        parts.len() > 200,
        "70 000 lines became {} parts",
        parts.len()
    );

    let started = std::time::Instant::now();
    for index in parts.clone().take(3) {
        set_block(
            &context,
            &document,
            plan.block(index),
            &style(),
            640.0,
            &NoImages,
        );
    }
    let elapsed = started.elapsed();
    println!("three parts of a 70 000 line block: {elapsed:?}");
    // Generous, because a test machine is not a reference machine — but three
    // orders of magnitude below the freeze this replaces.
    assert!(elapsed.as_millis() < 500, "three parts took {elapsed:?}");
}

/// Every face a set block asks Pango for is one the warm-up asks for too.
///
/// This is the drift the startup budget depends on: a type style added to
/// `set_block` and not to `faces` is a face the frame that shows the first
/// screen has to instantiate itself, and one face is 1.4 to 2.6 ms of a 16 ms
/// frame (docs/decisions/014-competitive-targets.md, section 3.3).
///
/// Both sides are collected the same way — the faces Pango reports having
/// used, not the descriptions it was handed — because a description names one
/// face and shaping can reach several: the list markers alone pull in a
/// fallback that the body font does not cover.
///
/// The source is Latin so that no fallback for another script appears; those
/// are deliberately not warmed.
#[test]
fn the_warm_list_covers_every_face_a_block_is_set_in() {
    let source = "# Titel\n\n## Abschnitt\n\n### Unterabschnitt\n\n#### Vierte\n\n\
##### Fuenfte\n\n###### Sechste\n\n\
Ein *kursiver* und ein **fetter** Absatz mit `code` und einem [Link](a.md).\n\n\
> Ein Zitat mit **Nachdruck**.\n\n\
- eins\n- zwei\n  - drei\n\n\
1. erstens\n2. zweitens\n\n\
- [ ] offen\n- [x] erledigt\n\n\
| Name | Wert |\n| --- | --- |\n| Beispiel | 1 |\n\n\
```rust\nfn main() {}\n```\n\n---\n";
    let (document, plan) = parse_plan(source);
    let context = context();
    let style = style();

    fn used_in(layout: &pango::Layout, into: &mut std::collections::BTreeSet<String>) {
        for line in layout.lines() {
            for run in line.runs() {
                into.insert(run.item().analysis().font().describe().to_string());
            }
        }
    }

    let mut warmed = std::collections::BTreeSet::new();
    for face in hashline::layout::faces(&style) {
        let layout = pango::Layout::new(&context);
        layout.set_text(face.sample);
        layout.set_font_description(Some(&face.font));
        used_in(&layout, &mut warmed);
    }

    let mut used = std::collections::BTreeSet::new();
    for index in 0..plan.len() {
        let block = *plan.block(index);
        let set = set_block(&context, &document, &block, &style, 640.0, &NoImages);
        for piece in &set.pieces {
            used_in(&piece.layout, &mut used);
        }
    }

    assert!(!used.is_empty(), "no block was set in any face");
    let missing: Vec<&String> = used.difference(&warmed).collect();
    assert!(
        missing.is_empty(),
        "set in faces the warm-up does not reach: {missing:?}\nwarmed: {warmed:?}"
    );
}
