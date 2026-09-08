// Port of `html-boundary.ts`. This only proves that a raw HTML token cannot
// leave an open container across section boundaries. It does not parse or
// sanitize HTML. Uncertain or ill-formed syntax takes the conservative
// whole-document path; DOMPurify remains mandatory for these sections.
const VOID_TAGS: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
const OPAQUE_TAGS: [&str; 10] = [
    "script", "style", "textarea", "title", "template", "plaintext", "xmp", "iframe", "noembed",
    "noscript",
];

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0c' | b'\r')
}
fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic()
}
fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b':' || b == b'-'
}

/// True when every element the fragment opens is also closed inside it.
pub fn is_closed_html(html: &str) -> bool {
    let bytes = html.as_bytes();
    let mut stack: Vec<String> = Vec::new();
    let mut offset = 0usize;
    while let Some(found) = bytes[offset..].iter().position(|&b| b == b'<') {
        let lt = offset + found;
        if bytes[lt..].starts_with(b"<!--") {
            match html[lt + 4..].find("-->") {
                Some(end) => {
                    offset = lt + 4 + end + 3;
                    continue;
                }
                None => return false,
            }
        }
        let mut i = lt + 1;
        let closing = bytes.get(i) == Some(&b'/');
        if closing {
            i += 1;
        }
        let name_start = i;
        if bytes.get(i).copied().is_none_or(|b| !is_name_start(b)) {
            return false;
        }
        while i < bytes.len() && is_name_char(bytes[i]) {
            i += 1;
        }
        let name = html[name_start..i].to_ascii_lowercase();
        // Raw-text and template insertion modes need a real HTML parser to prove
        // isolation, including apparent tags inside their text. Do not guess.
        if OPAQUE_TAGS.contains(&name.as_str()) {
            return false;
        }
        // Attributes, matching the tolerant expression the TypeScript version used.
        loop {
            while i < bytes.len() && is_space(bytes[i]) {
                i += 1;
            }
            match bytes.get(i) {
                None => return false,
                Some(b'>') => {
                    i += 1;
                    break;
                }
                Some(b'/') => {
                    i += 1;
                    continue;
                }
                _ => {}
            }
            let start = i;
            while i < bytes.len()
                && !is_space(bytes[i])
                && !matches!(bytes[i], b'>' | b'=' | b'/' | b'"' | b'\'' | b'<')
            {
                i += 1;
            }
            if i == start {
                return false;
            }
            while i < bytes.len() && is_space(bytes[i]) {
                i += 1;
            }
            if bytes.get(i) == Some(&b'=') {
                i += 1;
                while i < bytes.len() && is_space(bytes[i]) {
                    i += 1;
                }
                match bytes.get(i) {
                    Some(&q @ (b'"' | b'\'')) => match bytes[i + 1..].iter().position(|&b| b == q) {
                        Some(end) => i += 1 + end + 1,
                        None => return false,
                    },
                    Some(_) => {
                        while i < bytes.len()
                            && !is_space(bytes[i])
                            && !matches!(bytes[i], b'"' | b'\'' | b'=' | b'<' | b'>' | b'`')
                        {
                            i += 1;
                        }
                    }
                    None => return false,
                }
            }
        }
        if closing {
            if stack.pop().as_deref() != Some(name.as_str()) {
                return false;
            }
        } else if !VOID_TAGS.contains(&name.as_str()) {
            stack.push(name);
        }
        offset = i;
    }
    stack.is_empty()
}
