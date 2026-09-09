//! Temporary: what the reader retains for a document, post by post.
use hashline::layout::{Block, BlockPlan, Metrics};
use hashline::outline::Outline;

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))?
                .split_whitespace()
                .nth(1)?
                .parse()
                .ok()
        })
        .unwrap_or(0)
}
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let source = std::fs::read_to_string(&path).unwrap();
    let mut doc = hashline_markdown::parse(&source);
    drop(source);
    let plan = BlockPlan::new(
        &doc,
        Metrics {
            char_width: 8.5,
            body_px: 17.0,
        },
        640.0,
    );
    let entries = Outline::entries_of(&doc, &plan);
    doc.blocks = Vec::new();
    doc.headings = Vec::new();
    let kib = |bytes: usize| bytes / 1024;
    println!("ops        {:>7} KiB", kib(doc.ops.capacity()));
    println!("strings    {:>7} KiB", kib(doc.strings.capacity()));
    println!("text       {:>7} KiB", kib(doc.text.capacity()));
    println!("sections   {:>7} KiB", kib(doc.sections.capacity() * 4));
    println!(
        "plan       {:>7} KiB ({} blocks x {} B)",
        kib(plan.len() * std::mem::size_of::<Block>()),
        plan.len(),
        std::mem::size_of::<Block>()
    );
    println!("offsets    {:>7} KiB", kib((plan.len() + 1) * 8));
    println!(
        "outline    {:>7} KiB ({} entries)",
        kib(entries.len() * 24),
        entries.len()
    );
    println!("rss        {:>7} KiB", rss_kib());
}
