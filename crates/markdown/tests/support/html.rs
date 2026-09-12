//! Canonical HTML comparison, ported from `tests/sections.test.ts`.
//!
//! Both sides are parsed by the same HTML parser and serialized back through
//! the same walker, so element structure, attribute set, attribute values and
//! text content are binding, while attribute order, whitespace between block
//! elements and the choice of escape form are not — that is the tolerance the
//! comparison grants. Whitespace inside `pre` and `code` is content and is
//! preserved.

use html5ever::driver::ParseOpts;
use html5ever::tendril::TendrilSink;
use html5ever::{local_name, ns, parse_fragment, QualName};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

const BLOCK: [&str; 26] = [
    "p",
    "div",
    "ul",
    "ol",
    "li",
    "blockquote",
    "pre",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "th",
    "td",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "dl",
    "dt",
    "dd",
    "details",
    "summary",
];

/// The canonical form of an HTML string, for comparison and for diffs.
///
/// The whole walk happens while the `RcDom` is alive: it drops its tree on the
/// way out, so a `Handle` that outlives it is detached and walks as empty.
pub fn canonical(html: &str) -> String {
    let context = QualName::new(None, ns!(html), local_name!("div"));
    let dom = parse_fragment(
        RcDom::default(),
        ParseOpts::default(),
        context,
        Vec::new(),
        false,
    )
    .one(html.to_string());
    // A fragment parse wraps the nodes in a synthetic `html` element.
    let root = dom.document.children.borrow()[0].clone();
    let mut out = String::new();
    walk(&root, false, &mut out);
    out
}

fn name_of(node: &Handle) -> Option<String> {
    match &node.data {
        NodeData::Element { name, .. } => Some(name.local.to_string()),
        _ => None,
    }
}

fn is_boundary(node: Option<&Handle>) -> bool {
    match node {
        None => true,
        Some(node) => match name_of(node) {
            Some(tag) => BLOCK.contains(&tag.as_str()),
            None => false,
        },
    }
}

/// Serializes `node`'s children into a canonical form.
fn walk(node: &Handle, verbatim: bool, out: &mut String) {
    let children = node.children.borrow();
    for (index, child) in children.iter().enumerate() {
        match &child.data {
            NodeData::Text { contents } => {
                let mut text = contents.borrow().to_string();
                if !verbatim {
                    if is_boundary(index.checked_sub(1).and_then(|i| children.get(i))) {
                        text = text.trim_start().to_string();
                    }
                    if is_boundary(children.get(index + 1)) {
                        text = text.trim_end().to_string();
                    }
                    if text.is_empty() {
                        continue;
                    }
                }
                out.push_str(&text);
            }
            NodeData::Comment { contents } => {
                out.push_str("<!--");
                out.push_str(contents.as_ref());
                out.push_str("-->");
            }
            NodeData::Element { name, attrs, .. } => {
                let tag = name.local.to_string();
                out.push('<');
                out.push_str(&tag);
                let mut pairs: Vec<(String, String)> = attrs
                    .borrow()
                    .iter()
                    .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
                    .filter(|(key, value)| {
                        // Generated heading ids are ours; the specification
                        // does not know them.
                        !(key == "id"
                            && value.starts_with("doc-")
                            && BLOCK.contains(&tag.as_str())
                            && tag.starts_with('h'))
                    })
                    .collect();
                pairs.sort();
                for (key, value) in pairs {
                    out.push(' ');
                    out.push_str(&key);
                    out.push('=');
                    out.push('"');
                    out.push_str(&value);
                    out.push('"');
                }
                out.push('>');
                walk(child, verbatim || tag == "pre" || tag == "code", out);
                out.push_str("</");
                out.push_str(&tag);
                out.push('>');
            }
            _ => {}
        }
    }
}
