use unicode_general_category::{get_general_category, GeneralCategory};
use unicode_normalization::UnicodeNormalization;

// JavaScript's `\s` under the `u` flag: WhiteSpace ∪ LineTerminator. It is not
// Unicode's White_Space property — it contains U+FEFF and it does not contain
// U+0085. `String.prototype.trim` removes exactly this set as well.
// The list reads as one column per code point; rustfmt would reflow it into a
// paragraph and split the range across lines.
#[rustfmt::skip]
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

// `\p{L}`, `\p{M}`, `\p{N}` and `\p{Pc}` are General_Category groups, not the
// Alphabetic and Numeric_Type properties `char::is_alphabetic`/`is_numeric`
// expose. Together they are what GitHub's anchors keep of a heading's text.
fn is_word(c: char) -> bool {
    matches!(
        get_general_category(c),
        GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
            | GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark
            | GeneralCategory::DecimalNumber
            | GeneralCategory::LetterNumber
            | GeneralCategory::OtherNumber
            | GeneralCategory::ConnectorPunctuation
    )
}

/// The anchor a heading's text gets, the way GitHub forms it: NFKC,
/// lowercase, drop everything outside `\p{L}\p{M}\p{N}\p{Pc}\s-`, trim, and
/// turn every whitespace character into `-`. An empty result falls back to
/// `section`.
///
/// The underscore stays an underscore, and two spaces become two hyphens,
/// because that is what a link written against GitHub's rendering says:
/// `#foo_bar` for "foo_bar", `#a--b` for "a  b". The rule this replaced came
/// from the former `parser.ts` and folded both into a single `-`, so such
/// links found nothing. Marks are kept for the same reason: without them a
/// Devanagari heading lost its vowel signs.
pub fn slug_base(text: &str) -> String {
    // Lowercasing runs over the whole string, not per character: the final
    // sigma rule needs its context, exactly as `String.prototype.toLowerCase`
    // applies it.
    let lowered = text.nfkc().collect::<String>().to_lowercase();
    let kept: String = lowered
        .chars()
        .filter(|&c| is_word(c) || is_js_space(c) || c == '-')
        .collect();
    let out: String = kept
        .trim_matches(is_js_space)
        .chars()
        .map(|c| if is_js_space(c) { '-' } else { c })
        .collect();
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
    /// The next suffix worth trying for a base that has already been handed
    /// out. Probing from `1` every time is quadratic in the number of equal
    /// headings, and documents repeat headings for a living — a changelog or a
    /// generated reference has thousands of identical ones. At 60 000 repeats
    /// that pass alone runs for minutes, which the 10 MiB budget in SPEC.md,
    /// section 9 does not have.
    next: std::collections::HashMap<String, u32>,
}

impl Slugs {
    pub fn id(&mut self, text: &str) -> String {
        let base = slug_base(text);
        let mut slug = base.clone();
        let mut suffix = self.next.get(&base).copied().unwrap_or(1);
        // The set is still consulted: a suffixed id can collide with a heading
        // whose own text ends in that number, and then the probe has to move on.
        if suffix > 1 || self.used.contains(&slug) {
            loop {
                slug = format!("{base}-{suffix}");
                suffix += 1;
                if !self.used.contains(&slug) {
                    break;
                }
            }
        }
        self.next.insert(base, suffix);
        self.used.insert(slug.clone());
        format!("doc-{slug}")
    }
}

#[cfg(test)]
mod tests {
    use super::{slug_base, Slugs};

    #[test]
    fn anchors_are_spelled_the_way_github_spells_them() {
        assert_eq!(slug_base("Kopf_zeile"), "kopf_zeile");
        assert_eq!(slug_base("foo_bar()"), "foo_bar");
        assert_eq!(
            slug_base("Schritt 1:  Installieren"),
            "schritt-1--installieren"
        );
        assert_eq!(slug_base("A - B"), "a---b");
        assert_eq!(slug_base("  Grüße, Welt!  "), "grüße-welt");
        // Vowel signs and the virama are marks, not letters.
        assert_eq!(slug_base("हिन्दी"), "हिन्दी");
        assert_eq!(slug_base("!!!"), "section");
    }

    #[test]
    fn repeated_headings_are_numbered_in_order() {
        let mut slugs = Slugs::default();
        let ids: Vec<String> = (0..4).map(|_| slugs.id("Kapitel")).collect();
        assert_eq!(
            ids,
            [
                "doc-kapitel",
                "doc-kapitel-1",
                "doc-kapitel-2",
                "doc-kapitel-3"
            ]
        );
    }

    #[test]
    fn a_suffixed_id_taken_by_another_heading_is_skipped() {
        let mut slugs = Slugs::default();
        assert_eq!(slugs.id("Kapitel"), "doc-kapitel");
        // This heading's own slug is exactly what the deduplication would have
        // produced next, so the next "Kapitel" has to step over it.
        assert_eq!(slugs.id("Kapitel 1"), "doc-kapitel-1");
        assert_eq!(slugs.id("Kapitel"), "doc-kapitel-2");
    }
}
