//! Setting blocks with Pango, without a window (SPEC.md, section 12).

use hashline::layout::{set_block, BlockPlan, Metrics, Style};
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

const SOURCE: &str = "# Titel\n\nEin *kursiver* Absatz mit `code` und einem [Link](a.md).\n\n\
- eins\n- zwei\n\n```rust\nfn main() {}\n```\n";

#[test]
fn a_layouts_text_is_the_blocks_slice_of_the_document() {
    // The whole mapping between view and document rests on this.
    let document = hashline_markdown::parse(SOURCE);
    let plan = BlockPlan::new(&document, metrics(), 640.0);
    let context = context();
    for index in 0..plan.len() {
        let block = *plan.block(index);
        let set = set_block(&context, &document, &block, &style(), 640.0);
        let from = block.text_start as usize;
        let expected = &document.text[from..from + block.text_len as usize];
        let actual = set.layout.text();
        // A prefix, not merely equal: a fenced block drops the newline that
        // closed its last line. Being a prefix is what matters, because it
        // means every offset in the layout still means the same byte in the
        // document — nothing is shifted, only the tail is shorter.
        assert!(
            expected.starts_with(actual.as_str()),
            "block {index}: {actual:?} is not a prefix of {expected:?}"
        );
        assert!(
            expected.len() - actual.len() <= 1,
            "block {index}: dropped more than the closing newline"
        );
        assert_eq!(set.text_start, block.text_start);
    }
}

#[test]
fn a_set_block_has_a_real_height_and_the_estimate_is_in_the_same_league() {
    let document = hashline_markdown::parse(SOURCE);
    let plan = BlockPlan::new(&document, metrics(), 640.0);
    let context = context();
    for index in 0..plan.len() {
        let block = *plan.block(index);
        let measured = set_block(&context, &document, &block, &style(), 640.0).height();
        assert!(measured > 0.0, "block {index} measured {measured}");
        let estimate = block.height;
        // The estimate only has to make the scrollbar plausible, so this is a
        // loose bound — but an estimate off by an order of magnitude would make
        // scrolling lurch, and that is worth catching.
        assert!(
            measured < estimate * 10.0 + 200.0 && estimate < measured * 10.0 + 200.0,
            "block {index}: estimate {estimate}, measured {measured}"
        );
    }
}

#[test]
fn hit_testing_maps_points_across_a_wrapped_block() {
    // What the view needs is the point-to-index direction: a click anywhere in
    // the block must land on a real position, and moving right along a line
    // must never move backwards through the text.
    let document = hashline_markdown::parse("Ein Absatz mit genügend Text für mehrere Zeilen, damit das Umbrechen wirklich stattfindet und der Treffertest etwas zu tun bekommt.\n");
    let plan = BlockPlan::new(&document, metrics(), 300.0);
    let block = *plan.block(0);
    let set = set_block(&context(), &document, &block, &style(), 300.0);
    let text = set.layout.text().to_string();
    assert!(set.layout.line_count() > 1, "the fixture must wrap");

    let (_, logical) = set.layout.pixel_extents();
    for line in 0..set.layout.line_count() {
        let y = set.layout.line(line).map(|l| {
            let (_, extents) = l.extents();
            extents.y() + extents.height() / 2
        });
        let Some(y) = y else { continue };
        let mut previous = -1;
        for step in 0..40 {
            let x = (logical.width() * pango::SCALE * step / 40).max(0);
            let (_, index, _) = set.layout.xy_to_index(x, y);
            assert!(
                (index as usize) <= text.len(),
                "index {index} outside a {} byte block",
                text.len()
            );
            assert!(
                text.is_char_boundary(index as usize),
                "index {index} splits a character"
            );
            assert!(
                index >= previous,
                "moving right moved back: {previous} then {index}"
            );
            previous = index;
        }
    }
}

#[test]
fn a_position_maps_to_the_line_it_belongs_to() {
    let document = hashline_markdown::parse("Ein Absatz mit genügend Text für mehrere Zeilen, damit das Umbrechen wirklich stattfindet und der Treffertest etwas zu tun bekommt.\n");
    let plan = BlockPlan::new(&document, metrics(), 300.0);
    let set = set_block(&context(), &document, plan.block(0), &style(), 300.0);
    let text = set.layout.text().to_string();
    let needle = text.find("Treffertest").expect("needle") as i32;
    let (line, _) = set.layout.index_to_line_x(needle, false);
    // Late in a wrapped paragraph, so not on the first line.
    assert!(line > 0, "the needle should not be on line 0");
    let rectangle = set.layout.index_to_pos(needle);
    let (_, index, _) = set.layout.xy_to_index(
        rectangle.x() + rectangle.width() / 2,
        rectangle.y() + rectangle.height() / 2,
    );
    assert_eq!(
        index, needle,
        "the point for a position must map back to it"
    );
}

#[test]
fn inline_elements_become_attributes_over_the_right_bytes() {
    let document = hashline_markdown::parse("Ein *kursiver* Teil.\n");
    let plan = BlockPlan::new(&document, metrics(), 640.0);
    let set = set_block(&context(), &document, plan.block(0), &style(), 640.0);
    let attributes = set.layout.attributes().expect("attributes");
    let text = set.layout.text().to_string();
    let from = text.find("kursiver").unwrap() as u32;
    let italic = attributes
        .attributes()
        .into_iter()
        .find(|attribute| attribute.type_() == pango::AttrType::Style)
        .expect("an italic attribute");
    assert_eq!(italic.start_index(), from);
    assert_eq!(italic.end_index(), from + "kursiver".len() as u32);
}

#[test]
fn a_code_block_does_not_wrap() {
    let long = format!("```\n{}\n```\n", "x".repeat(400));
    let document = hashline_markdown::parse(&long);
    let plan = BlockPlan::new(&document, metrics(), 300.0);
    let set = set_block(&context(), &document, plan.block(0), &style(), 300.0);
    // One source line stays one laid-out line; the block scrolls horizontally
    // on its own so the document never does (SPEC.md, section 3).
    assert_eq!(set.layout.line_count(), 1);
    let (_, logical) = set.layout.pixel_extents();
    assert!(logical.width() > 300, "width {}", logical.width());
}
