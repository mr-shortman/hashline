//! Syntax highlighting for code blocks (SPEC.md, sections 4 and 10).
//!
//! Only syntect's *parser* is used, not its theme machinery. Hashline's palette
//! carries exactly three syntax colours — keyword, string and number — so a
//! Sublime colour scheme would have to be flattened into them anyway. Mapping
//! scopes to those three directly keeps the design tokens the single source of
//! colour and keeps a theme file out of the build
//! ([003-highlighting.md](../../docs/decisions/003-highlighting.md) kept the
//! same principle for highlight.js).
//!
//! Highlighting never runs on the main thread and never on the whole document:
//! it is requested per visible code block and cached by block.

use std::sync::OnceLock;

use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

/// What a run of code is coloured as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Keyword,
    Literal,
    Number,
}

/// A coloured run, as byte offsets into the document text blob.
#[derive(Clone, Copy, Debug)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub kind: Kind,
}

/// Code longer than this stays uncoloured. A very large block would cost more
/// than it is worth and SPEC.md section 10 says so explicitly.
pub const MAX_CODE_BYTES: usize = 128 * 1024;

fn syntaxes() -> &'static SyntaxSet {
    // Loading the definitions costs tens of milliseconds and a few megabytes,
    // so it happens once, on whichever worker thread needs it first.
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn syntax_for(language: &str) -> Option<&'static SyntaxReference> {
    let set = syntaxes();
    // The fence's info string is a token, not a file name: `rust`, `ts`, `sh`.
    set.find_syntax_by_token(language)
        .or_else(|| set.find_syntax_by_extension(language))
}

/// Whether a language is known at all. Unknown languages get readable,
/// uncoloured code rather than a guess (SPEC.md, section 6).
pub fn is_supported(language: &str) -> bool {
    !language.is_empty() && syntax_for(language).is_some()
}

/// Classifies a scope stack into one of the three colours, or none.
///
/// The prefixes are the stable part of the TextMate scope convention, which is
/// why matching on them survives syntax definitions being updated.
fn classify(stack: &ScopeStack) -> Option<Kind> {
    // The most specific scope wins, so the stack is read from the top.
    for scope in stack.as_slice().iter().rev() {
        let name = scope.build_string();
        if name.starts_with("constant.numeric") {
            return Some(Kind::Number);
        }
        if name.starts_with("string") || name.starts_with("constant.character") {
            return Some(Kind::Literal);
        }
        if name.starts_with("keyword")
            || name.starts_with("storage")
            || name.starts_with("constant.language")
            || name.starts_with("support.type")
            || name.starts_with("entity.name.tag")
        {
            return Some(Kind::Keyword);
        }
    }
    None
}

/// Colours `code`, returning runs offset by `base` so they address the
/// document text blob directly.
///
/// Runs are merged where neighbours share a colour, because the result becomes
/// Pango attributes and fewer of them is faster to apply.
pub fn spans(language: &str, code: &str, base: u32) -> Vec<Span> {
    if code.len() > MAX_CODE_BYTES {
        return Vec::new();
    }
    let Some(syntax) = syntax_for(language) else {
        return Vec::new();
    };
    let set = syntaxes();
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut line_start = 0usize;

    for line in code.split_inclusive('\n') {
        let Ok(operations) = state.parse_line(line, set) else {
            break;
        };
        let mut cursor = 0usize;
        let mut push = |from: usize, to: usize, kind: Option<Kind>| {
            let Some(kind) = kind else { return };
            if to <= from {
                return;
            }
            let start = (line_start + from) as u32 + base;
            let end = (line_start + to) as u32 + base;
            match spans.last_mut() {
                Some(last) if last.kind == kind && last.end == start => last.end = end,
                _ => spans.push(Span { start, end, kind }),
            }
        };
        for (offset, operation) in operations {
            push(cursor, offset, classify(&stack));
            if stack.apply(&operation).is_err() {
                break;
            }
            cursor = offset;
        }
        push(cursor, line.len(), classify(&stack));
        line_start += line.len();
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::{is_supported, spans, Kind, MAX_CODE_BYTES};

    fn kinds(language: &str, code: &str) -> Vec<(String, Kind)> {
        spans(language, code, 0)
            .into_iter()
            .map(|span| {
                (
                    code[span.start as usize..span.end as usize].to_string(),
                    span.kind,
                )
            })
            .collect()
    }

    #[test]
    fn a_known_language_colours_keywords_strings_and_numbers() {
        let found = kinds(
            "rust",
            "fn main() {\n    let x = 42;\n    let s = \"text\";\n}\n",
        );
        let keyword = found.iter().any(|(text, kind)| {
            *kind == Kind::Keyword && (text.contains("fn") || text.contains("let"))
        });
        let number = found
            .iter()
            .any(|(text, kind)| *kind == Kind::Number && text.contains("42"));
        let literal = found
            .iter()
            .any(|(text, kind)| *kind == Kind::Literal && text.contains("text"));
        assert!(keyword, "no keyword coloured: {found:?}");
        assert!(number, "no number coloured: {found:?}");
        assert!(literal, "no string coloured: {found:?}");
    }

    #[test]
    fn an_unknown_language_is_left_uncoloured() {
        assert!(!is_supported("klingon"));
        assert!(spans("klingon", "fn main() {}", 0).is_empty());
        assert!(!is_supported(""));
    }

    #[test]
    fn offsets_are_document_offsets_and_never_overlap() {
        let code = "let x = 1;\nlet y = 2;\n";
        let base = 1000;
        let found = spans("rust", code, base);
        assert!(!found.is_empty());
        let mut previous_end = base;
        for span in &found {
            assert!(span.start >= previous_end, "spans overlap: {found:?}");
            assert!(span.end > span.start);
            assert!(span.start >= base);
            assert!(span.end <= base + code.len() as u32);
            previous_end = span.end;
        }
    }

    #[test]
    fn a_very_large_block_stays_uncoloured() {
        let code = "let x = 1;\n".repeat(MAX_CODE_BYTES / 10);
        assert!(code.len() > MAX_CODE_BYTES);
        assert!(spans("rust", &code, 0).is_empty());
    }

    #[test]
    fn common_languages_are_recognised() {
        for language in ["rust", "js", "python", "sh", "json", "html", "c", "go"] {
            assert!(is_supported(language), "{language} unsupported");
        }
    }
}
