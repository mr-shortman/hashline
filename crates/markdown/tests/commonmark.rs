//! The 652 CommonMark 0.31.2 examples, compared against the specification's
//! own expected output.
//!
//! This is the conformance net decision 008 established and SPEC.md section 12
//! requires in Cargo. It runs on the op buffer: the operations are replayed
//! into HTML and compared with the specification's, so what is under test is
//! the encoder, not `pulldown-cmark`'s HTML renderer.

mod support;

use support::html::canonical;

fn examples() -> Vec<(u32, String, String)> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/commonmark-0.31.2.json"
    );
    let raw = std::fs::read_to_string(path).expect("CommonMark fixtures");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("fixture JSON");
    parsed
        .as_array()
        .expect("fixture array")
        .iter()
        .map(|item| {
            (
                item["example"].as_u64().unwrap() as u32,
                item["markdown"].as_str().unwrap().to_string(),
                item["html"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn parser_output_matches_commonmark() {
    let mut failures: Vec<String> = Vec::new();
    let mut raw = 0usize;
    for (number, markdown, html) in examples() {
        let doc = support::parse(&markdown);
        if doc.document.raw_html {
            // Raw HTML is shown as source text and therefore cannot match the
            // specification's output. That is the deliberate loss SPEC.md,
            // section 6 accepts in exchange for having no HTML parser at all.
            raw += 1;
            continue;
        }
        let actual = canonical(&doc.render());
        let expected = canonical(&html);
        if actual != expected {
            failures.push(format!(
                "example {number}\n  source:   {markdown:?}\n  expected: {expected}\n  actual:   {actual}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of 652 CommonMark examples differ:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    // Pinned so that the HTML-free share cannot shrink unnoticed: anything
    // drifting onto the source-text path is a rendering regression.
    assert_eq!(raw, 72, "examples containing raw HTML");
}

#[test]
fn raw_html_is_shown_as_source_text() {
    let doc = support::parse("Davor\n\n<div class=\"x\">roh &amp; wild</div>\n\nDanach\n");
    assert!(doc.document.raw_html, "the document must report raw HTML");
    // Asserted on the replay itself, not on its canonical form: canonicalizing
    // re-parses the HTML and would turn the escaped source back into markup,
    // which is precisely what must not happen in the view.
    let html = doc.render();
    // Set apart as source, not interpreted: no `div` element is created.
    assert!(html.contains("<pre><code class=\"raw-html\">"), "{html}");
    assert!(!html.contains("<div"), "{html}");
    assert!(html.contains("&lt;div class=\"x\"&gt;"), "{html}");
    // The markup is readable, and searchable, as the text it is.
    assert!(doc
        .document
        .text
        .contains("<div class=\"x\">roh &amp; wild</div>"));
}

#[test]
fn inline_html_stays_inline_source() {
    let doc = support::parse("Ein <b>fettes</b> Wort.\n");
    assert!(doc.document.raw_html);
    let html = doc.render();
    assert!(!html.contains("<b>"), "{html}");
    assert!(
        html.contains("<code class=\"raw-html\">&lt;b&gt;</code>"),
        "{html}"
    );
    // Still one paragraph: inline HTML does not break the block.
    assert_eq!(html.matches("<p>").count(), 1, "{html}");
}

#[test]
fn section_boundaries_do_not_change_the_result() {
    let block = concat!(
        "## Abschnitt **eins**\n\n",
        "Ein Absatz mit [Link](other.md), `code`, *kursiv* und ![Bild](a.png).\n\n",
        "- Punkt eins\n  - verschachtelt\n- [x] erledigt\n- [ ] offen\n\n",
        "> Zitat mit &amp; und <https://example.com/>\n\n",
        "| A | B |\n| :-- | --: |\n| 1 | 2 |\n\n",
        "```js\nconst a = 1 < 2 && \"x\";\n```\n\n",
        "1. eins\n2. zwei\n\n---\n\n"
    );
    let repetitions = 40;
    let source = block.repeat(repetitions);
    let whole = support::parse(&source);
    assert!(
        whole.section_count() > 1,
        "the fixture must cross a section boundary"
    );
    // Heading ids differ by design across repetitions; the structure must not.
    let piecewise: String = (0..repetitions)
        .map(|_| support::parse(block).render())
        .collect();
    assert_eq!(canonical(&whole.render()), canonical(&piecewise));
}
