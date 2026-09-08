use unicode_general_category::{get_general_category, GeneralCategory};
use unicode_normalization::UnicodeNormalization;

// JavaScript's `\s` under the `u` flag: WhiteSpace ∪ LineTerminator. It is not
// Unicode's White_Space property — it contains U+FEFF and it does not contain
// U+0085. `String.prototype.trim` removes exactly this set as well.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0b}'
            | '\u{0c}'
            | '\r'
            | ' '
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

// `\p{L}` and `\p{N}` are General_Category groups, not the Alphabetic and
// Numeric_Type properties `char::is_alphabetic`/`is_numeric` expose.
fn is_letter_or_number(c: char) -> bool {
    matches!(
        get_general_category(c),
        GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
            | GeneralCategory::DecimalNumber
            | GeneralCategory::LetterNumber
            | GeneralCategory::OtherNumber
    )
}

/// Port of `slugBase` from the former `parser.ts`, kept bit-exact:
/// NFKC, lowercase, drop everything outside `\p{L}\p{N}\s_-`, trim, collapse
/// runs of whitespace and underscores into `-`, fall back to `section`.
pub fn slug_base(text: &str) -> String {
    // Lowercasing runs over the whole string, not per character: the final
    // sigma rule needs its context, exactly as `String.prototype.toLowerCase`
    // applies it.
    let lowered = text.nfkc().collect::<String>().to_lowercase();
    let kept: String = lowered
        .chars()
        .filter(|&c| is_letter_or_number(c) || is_js_space(c) || c == '_' || c == '-')
        .collect();
    let trimmed = kept.trim_matches(is_js_space);
    let mut out = String::with_capacity(trimmed.len());
    let mut in_run = false;
    for c in trimmed.chars() {
        if is_js_space(c) || c == '_' {
            if !in_run {
                out.push('-');
                in_run = true;
            }
        } else {
            out.push(c);
            in_run = false;
        }
    }
    if out.is_empty() {
        "section".to_owned()
    } else {
        out
    }
}

/// Document-wide unique heading ids: `doc-<slug>`, deduplicated with `-1`, `-2` …
#[derive(Default)]
pub struct Slugs {
    used: std::collections::HashSet<String>,
}

impl Slugs {
    pub fn id(&mut self, text: &str) -> String {
        let base = slug_base(text);
        let mut slug = base.clone();
        let mut suffix = 1u32;
        while self.used.contains(&slug) {
            slug = format!("{base}-{suffix}");
            suffix += 1;
        }
        self.used.insert(slug.clone());
        format!("doc-{slug}")
    }
}
