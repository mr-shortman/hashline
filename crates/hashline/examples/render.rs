//! Renders a document to a PNG with no window and no compositor.
//!
//! It drives the same `layout` code the widget does, so what comes out is the
//! real typography rather than a second implementation of it. That makes it
//! usable for the visual acceptance in SPEC.md, section 12 — comparing the
//! native setting against the reference captures — and for looking at the
//! output at all on a machine where screenshots are not permitted.
//!
//! Usage: `cargo run -p hashline --example render -- <file.md> [out.png] [width] [dark]`

use hashline::layout::{set_block, BlockPlan, Metrics, Style};
use hashline::theme::{document as tokens, DARK, LIGHT};
use pango::prelude::*;
use pangocairo::cairo;

fn main() {
    let mut arguments = std::env::args().skip(1);
    let source_path = arguments.next().expect("a Markdown file");
    let out_path = arguments.next().unwrap_or_else(|| "render.png".to_string());
    let width: f64 = arguments
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(900.0);
    let dark = arguments.next().is_some_and(|value| value == "dark");
    let palette = if dark { DARK } else { LIGHT };

    let source = std::fs::read_to_string(&source_path).expect("readable file");
    let document = hashline_markdown::parse(&source);

    let context = pangocairo::FontMap::default().create_context();
    let style = Style::new("sans", "monospace", tokens::BODY_PX, palette);

    let metrics = context.metrics(Some(&style.body), None);
    let char_width = metrics.approximate_char_width() as f64 / pango::SCALE as f64;
    let column = (char_width * tokens::COLUMN_CHARS).min(width - 2.0 * tokens::PAD_SIDE);
    let plan = BlockPlan::new(
        &document,
        Metrics {
            char_width,
            body_px: tokens::BODY_PX,
        },
        column,
    );

    // Set blocks until the page is full, exactly as the view would for its
    // first screen.
    let page_height = 1400.0;
    let left = ((width - column) / 2.0).max(tokens::PAD_SIDE);
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, width as i32, page_height as i32)
            .expect("surface");
    let cairo = cairo::Context::new(&surface).expect("context");
    cairo.set_source_rgb(
        palette.bg.red as f64,
        palette.bg.green as f64,
        palette.bg.blue as f64,
    );
    cairo.paint().unwrap();

    let mut y = tokens::PAD_TOP;
    let mut drawn = 0;
    for index in 0..plan.len() {
        if y > page_height {
            break;
        }
        let block = *plan.block(index);
        let set = set_block(&context, &document, &block, &style, column);
        cairo.set_source_rgb(
            palette.text.red as f64,
            palette.text.green as f64,
            palette.text.blue as f64,
        );
        cairo.move_to(left, y + set.baseline_offset());
        pangocairo::functions::show_layout(&cairo, &set.layout);
        y += set.height();
        drawn += 1;
    }

    drop(cairo);
    let mut file = std::fs::File::create(&out_path).expect("writable output");
    surface.write_to_png(&mut file).expect("png");
    println!(
        "{out_path}: {drawn} of {} blocks, column {column:.0}px of {width:.0}px",
        plan.len()
    );
}
