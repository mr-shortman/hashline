// Minimal timing probe: parse each fixture with the GFM options Hashline needs
// and report the median over N runs. Two variants are timed because P2.3 emits
// an op buffer, not HTML: `events` drains the parser, `html` renders a string.
use pulldown_cmark::{html, Options, Parser};
use std::time::Instant;

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

fn main() {
    let runs: usize = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(9);
    println!("[");
    let fixtures = ["small", "medium", "large"];
    for (index, name) in fixtures.iter().enumerate() {
        let path = format!("benchmarks/generated/{name}.md");
        let source = std::fs::read_to_string(&path).expect("fixture");
        let mut events = Vec::new();
        let mut rendered = Vec::new();
        let mut event_count = 0usize;
        let mut html_len = 0usize;
        for _ in 0..runs {
            let start = Instant::now();
            let count = Parser::new_ext(&source, options()).count();
            events.push(start.elapsed().as_secs_f64() * 1000.0);
            event_count = count;

            let start = Instant::now();
            let mut out = String::with_capacity(source.len() * 3 / 2);
            html::push_html(&mut out, Parser::new_ext(&source, options()));
            rendered.push(start.elapsed().as_secs_f64() * 1000.0);
            html_len = out.len();
        }
        println!(
            "  {{\"fixture\": \"{name}\", \"bytes\": {}, \"runs\": {runs}, \"events\": {event_count}, \"htmlBytes\": {html_len}, \"eventsMedianMs\": {:.3}, \"htmlMedianMs\": {:.3}}}{}",
            source.len(),
            median(events),
            median(rendered),
            if index + 1 == fixtures.len() { "" } else { "," }
        );
    }
    println!("]");
}
