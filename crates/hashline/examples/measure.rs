//! Times the stages a document goes through before it is readable, without a
//! window (SPEC.md, section 9: "Parsen, Blockplan, Layout des Sichtbereichs und
//! erstes Zeichnen separat instrumentieren").
//!
//! This is not the acceptance measurement — that one is external, over the
//! Wayland protocol and a monitor capture, and it measures the running
//! application. What this gives is the part of the budget that belongs to
//! Hashline's own code, which is what has to be optimized when the budget is
//! missed.
//!
//! Usage: `cargo run --release -p hashline --example measure -- <file.md>…`
//!
//! RSS is this process's absolute resident size after the fixture, so give each
//! fixture its own run when the number matters — an allocator does not return
//! everything between them.

use hashline::layout::{set_block, BlockPlan, Metrics, Style};
use hashline::theme::{document as tokens, LIGHT};
use pango::prelude::*;

/// Resident set size of this process, in KiB.
fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmRSS:"))
                .and_then(|line| line.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    let context = pangocairo::FontMap::default().create_context();
    let style = Style::new("sans", "monospace", tokens::BODY_PX, LIGHT);
    let metrics = context.metrics(Some(&style.body), None);
    let char_width = metrics.approximate_char_width() as f64 / pango::SCALE as f64;
    let column = char_width * tokens::COLUMN_CHARS;
    // One 900x700 window's worth, plus the buffer the view keeps.
    let viewport = 700.0;

    println!(
        "{:<22} {:>9} {:>8} {:>8} {:>8} {:>7} {:>9} {:>8}",
        "fixture", "bytes", "parse", "plan", "screen", "blocks", "set", "RSS"
    );
    for path in &files {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("{path}: {error}");
                continue;
            }
        };

        let started = std::time::Instant::now();
        let document = hashline_markdown::parse(&source);
        let parse = started.elapsed();

        let started = std::time::Instant::now();
        let mut plan = BlockPlan::new(
            &document,
            Metrics {
                char_width,
                body_px: tokens::BODY_PX,
            },
            column,
        );
        let planning = started.elapsed();

        // The first readable screen: set blocks until the viewport is covered.
        let started = std::time::Instant::now();
        let mut y = 0.0;
        let mut set = 0usize;
        for index in 0..plan.len() {
            if y > viewport * 2.0 {
                break;
            }
            let block = *plan.block(index);
            let laid = set_block(&context, &document, &block, &style, column);
            let height = laid.height();
            plan.set_measured(index, height);
            y += height;
            set += 1;
        }
        let screen = started.elapsed();

        let name = std::path::Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        println!(
            "{:<22} {:>9} {:>7.1}ms {:>6.1}ms {:>6.1}ms {:>7} {:>9} {:>6}M",
            name,
            source.len(),
            parse.as_secs_f64() * 1000.0,
            planning.as_secs_f64() * 1000.0,
            screen.as_secs_f64() * 1000.0,
            plan.len(),
            set,
            rss_kib() / 1024
        );
    }
}
