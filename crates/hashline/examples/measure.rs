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

use hashline::layout::{set_block, BlockPlan, Metrics, NoImages, Style};
use hashline::theme::{document as tokens, LIGHT};
use pango::prelude::*;
use std::time::Duration;

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

/// Median and p95 of a set of samples, as SPEC.md section 9 asks for.
fn quantiles(samples: &mut [Duration]) -> (f64, f64) {
    samples.sort();
    let millis = |value: Duration| value.as_secs_f64() * 1000.0;
    let median = millis(samples[samples.len() / 2]);
    // Nearest-rank p95: the smallest sample at or above the 95th percentile.
    let rank = ((samples.len() as f64) * 0.95).ceil() as usize;
    let p95 = millis(samples[rank.saturating_sub(1).min(samples.len() - 1)]);
    (median, p95)
}

fn main() {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut repetitions = 30usize;
    if let Some(index) = arguments.iter().position(|value| value == "--repetitions") {
        repetitions = arguments
            .get(index + 1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(30);
        arguments.drain(index..=index + 1);
    }
    let files = arguments;

    let context = pangocairo::FontMap::default().create_context();
    let style = Style::new("sans", "monospace", tokens::BODY_PX, LIGHT);
    let metrics = context.metrics(Some(&style.body), None);
    let char_width = metrics.approximate_char_width() as f64 / pango::SCALE as f64;
    let column = char_width * tokens::COLUMN_CHARS;
    // One 900x700 window's worth, plus the buffer the view keeps.
    let viewport = 700.0;

    println!("n={repetitions} per fixture; times are median / p95 in ms");
    println!(
        "{:<16} {:>9} {:>15} {:>15} {:>15} {:>8} {:>6} {:>7}",
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

        let mut parses = Vec::with_capacity(repetitions);
        let mut plans = Vec::with_capacity(repetitions);
        let mut screens = Vec::with_capacity(repetitions);
        let mut blocks = 0usize;
        let mut set_count = 0usize;

        for _ in 0..repetitions {
            let started = std::time::Instant::now();
            let document = hashline_markdown::parse(&source);
            parses.push(started.elapsed());

            let started = std::time::Instant::now();
            let mut plan = BlockPlan::new(
                &document,
                Metrics {
                    char_width,
                    body_px: tokens::BODY_PX,
                },
                column,
            );
            plans.push(started.elapsed());

            // The first readable screen: set blocks until the viewport is
            // covered, which is all the view does before it can draw.
            let started = std::time::Instant::now();
            let mut y = 0.0;
            let mut set = 0usize;
            for index in 0..plan.len() {
                if y > viewport * 2.0 {
                    break;
                }
                let block = *plan.block(index);
                let laid = set_block(&context, &document, &block, &style, column, &NoImages);
                let height = laid.height();
                plan.set_measured(index, height);
                y += height;
                set += 1;
            }
            screens.push(started.elapsed());
            blocks = plan.len();
            set_count = set;
        }

        let (parse_median, parse_p95) = quantiles(&mut parses);
        let (plan_median, plan_p95) = quantiles(&mut plans);
        let (screen_median, screen_p95) = quantiles(&mut screens);
        let name = std::path::Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        println!(
            "{:<16} {:>9} {:>7.1}/{:<7.1} {:>7.2}/{:<7.2} {:>7.1}/{:<7.1} {:>8} {:>6} {:>6}M",
            name,
            source.len(),
            parse_median,
            parse_p95,
            plan_median,
            plan_p95,
            screen_median,
            screen_p95,
            blocks,
            set_count,
            rss_kib() / 1024
        );
    }
}
