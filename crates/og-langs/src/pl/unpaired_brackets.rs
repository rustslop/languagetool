use og_core::rule::TextLevelRule;
use og_core::{AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule};
use regex::Regex;
use std::sync::LazyLock;

const ID: &str = "PL_UNPAIRED_BRACKETS";
const DESCRIPTION: &str = "Niesparowane nawiasy i cudzysłowy";

const START_SYMBOLS: [&str; 6] = ["[", "(", "{", "\u{201e}", "\u{bb}", "\""];
const END_SYMBOLS: [&str; 6] = ["]", ")", "}", "\u{201d}", "\u{ab}", "\""];

static NUMERALS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[0-9]{1,2}?[a-z']*|M*(?:D?C{0,3}|C[DM])(?:L?X{0,3}|X[LC])(?:V?I{0,3}|I[VX]))$")
        .expect("Java NUMERALS_EN translates to the regex crate")
});

static URL_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^https?://.+$").expect("url token regex"));

pub struct PolishUnpairedBracketsRule {
    category: Category,
}

impl PolishUnpairedBracketsRule {
    pub fn new() -> Self {
        Self {
            category: Category::new("PUNCTUATION", "Punctuation"),
        }
    }
}

impl Default for PolishUnpairedBracketsRule {
    fn default() -> Self {
        Self::new()
    }
}

struct StackEntry {
    symbol: &'static str,
    opening: bool,
    start: usize,
    sentence_idx: usize,
}

struct SentenceTok<'a> {
    text: &'a str,
    start: usize,
    ws_before: bool,
    sentence_end: bool,
    virtual_start: bool,
}

fn sentence_tokens(sentence: &AnalyzedSentence) -> Vec<SentenceTok<'_>> {
    let full = sentence.tokens();
    let mut out = Vec::with_capacity(full.len());
    for (idx, t) in full.iter().enumerate() {
        if t.is_whitespace() {
            continue;
        }
        out.push(SentenceTok {
            text: t.token().token(),
            start: t.token().start(),
            ws_before: idx > 0 && full[idx - 1].is_whitespace(),
            sentence_end: t.is_sentence_end(),
            virtual_start: idx == 0,
        });
    }
    out
}

fn is_java_punct(c: char) -> bool {
    c.is_ascii_punctuation() || matches!(c, '\u{2026}' | '\u{2013}' | '\u{2014}')
}

fn punct_matches(token: &str) -> bool {
    let mut chars = token.chars();
    chars.next().is_some_and(is_java_punct) && chars.next().is_none()
}

fn punct_no_dot_matches(token: &str) -> bool {
    let mut chars = token.chars();
    if let (Some(first), Some(second), None) = (chars.next(), chars.next(), chars.next()) {
        return matches!(
            first,
            'l' | 'd' | 'm' | 'n' | 's' | 't' | 'L' | 'D' | 'M' | 'N' | 'S' | 'T'
        ) && second == '\'';
    }
    token.chars().count() == 1 && token.chars().all(|c| is_java_punct(c) && c != '.')
}

fn is_numeral(token: &str) -> bool {
    NUMERALS.is_match(&token.to_ascii_lowercase())
}

fn no_exception(toks: &[SentenceTok<'_>], i: usize) -> bool {
    let tok = toks[i].text;
    if i >= 1 {
        let prev = toks[i - 1].text;
        if URL_TOKEN.is_match(prev) && prev.contains('(') {
            return false;
        }
        if (prev == ":" || prev == ";") && !toks[i].ws_before && (tok == ")" || tok == "(") {
            return false;
        }
    }
    if i >= 2 {
        let prev_prev = toks[i - 2].text;
        let prev = toks[i - 1].text;
        if (prev_prev == ":" || prev_prev == ";") && prev == "-" && (tok == ")" || tok == "(") {
            return false;
        }
    }
    true
}

fn preceded_by_whitespace(toks: &[SentenceTok<'_>], i: usize, j: usize) -> bool {
    if START_SYMBOLS[j] != END_SYMBOLS[j] {
        return true;
    }
    let prev = &toks[i - 1];
    prev.virtual_start
        || toks[i].ws_before
        || punct_no_dot_matches(prev.text)
        || START_SYMBOLS.contains(&prev.text)
}

fn special_case(toks: &[SentenceTok<'_>], i: usize, j: usize) -> bool {
    if i >= toks.len() - 1 || START_SYMBOLS[j] != END_SYMBOLS[j] {
        return true;
    }
    let next = &toks[i + 1];
    next.ws_before
        || punct_matches(next.text)
        || END_SYMBOLS.contains(&next.text)
        || toks[i - 1].text.ends_with('-')
        || next.text.starts_with('-')
        || next.text == "s"
}

fn numeral_guard(toks: &[SentenceTok<'_>], i: usize, j: usize, stack: &[StackEntry]) -> bool {
    if END_SYMBOLS[j] != ")" {
        return false;
    }
    let open_paren_top = stack.last().is_some_and(|e| e.symbol == "(");
    if i > 2
        && (toks[i - 3].virtual_start || toks[i - 2].ws_before)
        && toks[i - 1].text == "."
        && is_numeral(toks[i - 2].text)
        && !open_paren_top
    {
        return true;
    }
    i > 1 && is_numeral(toks[i - 1].text) && !open_paren_top
}

fn end_symbol_is_unique(symbol: &str) -> bool {
    END_SYMBOLS.iter().filter(|s| **s == symbol).count() == 1
}

fn fill_symbol_stack(
    toks: &[SentenceTok<'_>],
    i: usize,
    j: usize,
    sentence_idx: usize,
    stack: &mut Vec<StackEntry>,
) -> bool {
    let tok = &toks[i];
    let is_start = tok.text == START_SYMBOLS[j];
    let is_end = tok.text == END_SYMBOLS[j];
    if !is_start && !is_end {
        return false;
    }
    let no_exc = no_exception(toks, i);
    if no_exc && preceded_by_whitespace(toks, i, j) && is_start {
        stack.push(StackEntry {
            symbol: START_SYMBOLS[j],
            opening: true,
            start: tok.start,
            sentence_idx,
        });
        return true;
    }
    if !no_exc || !(special_case(toks, i, j) || tok.sentence_end) || !is_end {
        return false;
    }
    if numeral_guard(toks, i, j, stack) {
        return false;
    }
    if stack.is_empty() {
        stack.push(StackEntry {
            symbol: END_SYMBOLS[j],
            opening: false,
            start: tok.start,
            sentence_idx,
        });
        return true;
    }
    if stack.last().is_some_and(|e| e.symbol == START_SYMBOLS[j]) {
        stack.pop();
        return true;
    }
    if end_symbol_is_unique(END_SYMBOLS[j]) || j == END_SYMBOLS.len() - 1 {
        stack.push(StackEntry {
            symbol: END_SYMBOLS[j],
            opening: false,
            start: tok.start,
            sentence_idx,
        });
        return true;
    }
    false
}

fn symbol_index(symbols: &[&str], symbol: &str) -> i32 {
    symbols
        .iter()
        .position(|s| *s == symbol)
        .map_or(-1, |p| p as i32)
}

fn corresponding_symbol(symbol: &str) -> &'static str {
    if let Some(idx) = START_SYMBOLS.iter().position(|s| *s == symbol) {
        return END_SYMBOLS[idx];
    }
    let idx = END_SYMBOLS
        .iter()
        .position(|s| *s == symbol)
        .expect("stack symbols come from the symbol lists");
    START_SYMBOLS[idx]
}

fn ends_like_real_sentence(sentence_text: &str) -> bool {
    let s = sentence_text.trim();
    s.ends_with('.') || s.ends_with('?') || s.ends_with('!')
}

fn char_floor(text: &str, mut i: usize) -> usize {
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn char_ceil(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

impl PolishUnpairedBracketsRule {
    fn create_match(
        &self,
        text: &str,
        sentences: &[AnalyzedSentence],
        loc: &StackEntry,
        matches: &mut Vec<RuleMatch>,
        rule_match_stack: &mut Vec<(&'static str, usize)>,
    ) -> Option<RuleMatch> {
        if let Some(&(sym, r_idx)) = rule_match_stack.last()
            && let Some(index) = END_SYMBOLS.iter().position(|s| *s == loc.symbol)
            && sym == START_SYMBOLS[index]
            && matches.len() > r_idx
        {
            matches.remove(r_idx);
            rule_match_stack.pop();
            return None;
        }
        rule_match_stack.push((loc.symbol, matches.len()));
        if list_item_guard(text, loc) {
            return None;
        }
        let other = corresponding_symbol(loc.symbol);
        let len = loc.symbol.len();
        let context_start = char_floor(text, loc.start.saturating_sub(30));
        let context_end = char_ceil(text, (loc.start + len + 30).min(text.len()));
        let context_text = text[context_start..context_end].to_string();
        let rule = RuleMatchRule::new(ID, DESCRIPTION)
            .with_category(self.category.clone())
            .with_issue_type(IssueType::Other("other".to_string()).as_str());
        Some(
            RuleMatch::new(
                format!("Brak niesparowanego symbolu: \u{201e}{other}\u{201d}"),
                loc.start,
                len,
                rule,
                RuleMatchContext::new(context_text, context_start, context_end - context_start),
            )
            .with_sentence(sentences[loc.sentence_idx].text().to_string()),
        )
    }
}

fn list_item_guard(text: &str, loc: &StackEntry) -> bool {
    let len = loc.symbol.len();
    if loc.start + len >= text.len() {
        return false;
    }
    let bytes = text.as_bytes();
    if loc.start >= 2 {
        return loc.symbol == ")"
            && bytes[loc.start - 2] == b'\n'
            && bytes[loc.start - 1].is_ascii_alphabetic();
    }
    if loc.start >= 1 {
        return loc.symbol == ")" && bytes[loc.start - 1].is_ascii_alphabetic();
    }
    false
}

impl TextLevelRule for PolishUnpairedBracketsRule {
    fn id(&self) -> &str {
        ID
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("other".to_string())
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut stack: Vec<StackEntry> = Vec::new();
        let mut matches: Vec<RuleMatch> = Vec::new();
        let mut rule_match_stack: Vec<(&'static str, usize)> = Vec::new();

        for (sentence_idx, sentence) in sentences.iter().enumerate() {
            let toks = sentence_tokens(sentence);
            for i in 1..toks.len() {
                for j in 0..START_SYMBOLS.len() {
                    if fill_symbol_stack(&toks, i, j, sentence_idx, &mut stack) {
                        break;
                    }
                }
            }
        }

        let ss = stack.len();
        let mut symmetric = false;
        if ss > 2 && ss % 2 == 1 {
            // Java compares each half-pair against the LAST stack element, not the mirrored one.
            symmetric = (0..ss / 2).all(|i| {
                symbol_index(&START_SYMBOLS, stack[i].symbol)
                    == symbol_index(&END_SYMBOLS, stack[ss - 1].symbol)
            });
        }

        if symmetric {
            if let Some(m) = self.create_match(
                text,
                sentences,
                &stack[ss / 2],
                &mut matches,
                &mut rule_match_stack,
            ) {
                matches.push(m);
            }
        } else {
            for loc in &stack {
                if let Some(m) =
                    self.create_match(text, sentences, loc, &mut matches, &mut rule_match_stack)
                    && (!loc.opening
                        || ends_like_real_sentence(sentences[loc.sentence_idx].text())
                        || sentences.len() - 1 > loc.sentence_idx)
                {
                    matches.push(m);
                }
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pl::test_support::PolishPipeline;

    fn run(text: &str) -> Vec<RuleMatch> {
        let rule = PolishUnpairedBracketsRule::new();
        let sentence = PolishPipeline::shared().analyze_range(text, 0, text.len());
        og_core::rule::TextLevelRule::match_text(&rule, text, std::slice::from_ref(&sentence))
    }

    #[test]
    fn java_pins_the_symbol_lists() {
        assert_eq!(START_SYMBOLS, ["[", "(", "{", "\u{201e}", "\u{bb}", "\""]);
        assert_eq!(END_SYMBOLS, ["]", ")", "}", "\u{201d}", "\u{ab}", "\""]);
    }

    #[test]
    fn unmatched_straight_double_quote_is_reported() {
        let text = "To jest zdanie z \"cudzys\u{142}owem.";
        let matches = run(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        let m = &matches[0];
        assert_eq!(&text[m.offset()..m.offset() + m.length()], "\"");
        assert_eq!(m.rule().id(), "PL_UNPAIRED_BRACKETS");
        assert_eq!(
            m.message(),
            "Brak niesparowanego symbolu: \u{201e}\"\u{201d}"
        );
    }

    #[test]
    fn paired_straight_double_quotes_stay_silent() {
        assert!(
            run("To jest \"cytat\" w tek\u{15b}cie.").is_empty(),
            "Java pairs the symmetric quote via whitespace gating"
        );
    }

    #[test]
    fn backslash_is_not_a_bracket_symbol() {
        assert!(
            run("\u{15a}cie\u{17c}ka C:\\katalog\\plik.txt.").is_empty(),
            "Java has no backslash in PL_START_SYMBOLS/PL_END_SYMBOLS"
        );
    }

    #[test]
    fn polish_low_quotes_pair_and_linger() {
        let text = "To jest zdanie z \u{201e}cudzys\u{142}owem.";
        let matches = run(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(
            &text[matches[0].offset()..matches[0].offset() + matches[0].length()],
            "\u{201e}"
        );
        assert_eq!(
            matches[0].message(),
            "Brak niesparowanego symbolu: \u{201e}\u{201d}\u{201d}"
        );
        assert!(run("To jest zdanie z \u{201e}cudzys\u{142}owem\u{201d}.").is_empty());
    }

    #[test]
    fn guillemets_pair_both_ways() {
        assert!(run("To jest \u{bb}cytat\u{ab} w tek\u{15b}cie.").is_empty());
        let matches = run("To jest \u{bb}cytat w tek\u{15b}cie.");
        assert_eq!(matches.len(), 1, "{matches:?}");
    }

    #[test]
    fn round_and_square_brackets_follow_the_stack() {
        assert!(run("To (jest) [test] {ok}.").is_empty());
        let matches = run("To (jest [test.");
        assert_eq!(matches.len(), 2, "{matches:?}");
        assert!(
            matches
                .iter()
                .any(|m| m.offset() == text_pos("To (jest [test.", '('))
        );
    }

    #[test]
    fn smileys_do_not_count_as_brackets() {
        assert!(run("U\u{15b}miech :) szeroki.").is_empty());
        assert!(run("Mrugni\u{119}cie ;-) pełne.").is_empty());
    }

    #[test]
    fn enumerated_list_markers_do_not_count() {
        assert!(run("a) pierwszy punkt.").is_empty());
        assert!(run("1) pierwszy punkt.").is_empty());
    }

    #[test]
    fn url_parentheses_do_not_count() {
        assert!(run("Zobacz https://example.com/a(b) dalej.").is_empty());
    }

    fn text_pos(text: &str, c: char) -> usize {
        text.find(c).unwrap_or(usize::MAX)
    }

    #[test]
    fn unmatched_closing_paren_is_reported() {
        let text = "To jest zdanie).";
        let matches = run(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(&text[matches[0].offset()..matches[0].offset() + 1], ")");
        assert!(matches[0].message().contains('('));
    }

    #[test]
    fn paired_parens_stay_silent() {
        assert!(
            run("To jest (cytat) w tekście.").is_empty(),
            "Java pairs the round brackets"
        );
    }

    #[test]
    fn ordinal_paren_after_numeral_is_ignored() {
        for text in [
            "Zobacz rozdział 12) dalej.",
            "Rozdział 12.5) wygląda inaczej.",
        ] {
            assert!(run(text).is_empty(), "{text}: {:?}", run(text));
        }
    }

    #[test]
    fn colon_dash_smileys_are_ignored() {
        assert!(run("Uśmiechnął się :-) szeroko.").is_empty());
    }

    #[test]
    fn list_item_paren_at_line_start_is_ignored() {
        assert!(run("Punkt:\na) jest ważny.").is_empty());
        assert!(!run("Punkt a) jest ważny.").is_empty());
    }

    #[test]
    fn curly_lower_quotes_pair_with_upper_close() {
        assert!(
            run("To jest \u{201e}cytat\u{201d} w tekście.").is_empty(),
            "Java pairs \u{201e} with \u{201d}"
        );
        let matches = run("To jest \u{201e}cytat w tekście.");
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert!(matches[0].message().contains('\u{201d}'));
    }

    #[test]
    fn metadata_pins_punctuation_category() {
        let rule = PolishUnpairedBracketsRule::default();
        assert_eq!(rule.id(), "PL_UNPAIRED_BRACKETS");
        assert_eq!(rule.description(), "Niesparowane nawiasy i cudzysłowy");
        assert_eq!(rule.category().id().as_str(), "PUNCTUATION");
        assert_eq!(
            rule.issue_type(),
            og_core::IssueType::Other("other".to_string())
        );
    }

    #[test]
    fn java_pin_paired_quotes_with_trailing_comma_stay_silent() {
        assert!(run(
            "Piosenka ta trafi\u{142}a na wiele list \"Best of...\", w\u{142}\u{105}czaj\u{105}c w to te, kt\u{f3}re zosta\u{142}y utworzone przez magazyn Rolling Stone."
        )
        .is_empty());
    }

    #[test]
    fn java_pin_three_quotes_in_one_sentence_stay_silent() {
        assert!(run("(To jest zdanie do testowania).").is_empty());
        assert!(run("A \"B\" C.").is_empty());
        assert!(run("\"A\" B \"C\".").is_empty());
    }

    #[test]
    fn symmetric_odd_quote_stack_reports_the_middle_quote() {
        let text = "To \" a \" b \" c.";
        let matches = run(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(
            matches[0].offset(),
            text.find("\" b").expect("middle quote")
        );
        assert_eq!(matches[0].length(), 1);
    }

    #[test]
    fn second_unmatched_quote_annihilates_the_first_match() {
        assert!(run("To \" a \" b.").is_empty());
    }

    #[test]
    fn numeral_then_dot_before_closing_paren_is_ignored() {
        assert!(run("1. 2. ) koniec.").is_empty());
    }

    #[test]
    fn square_close_over_round_open_reports_both() {
        let text = "To (by\u{142} b\u{142}\u{105}d].";
        let matches = run(text);
        let offsets: Vec<usize> = matches.iter().map(|m| m.offset()).collect();
        assert_eq!(
            offsets,
            vec![text.find('(').unwrap(), text.find(']').unwrap()]
        );
        assert!(matches[1].message().contains('['));
    }

    #[test]
    fn lone_paren_after_url_token_is_ignored() {
        assert!(run("Zobacz https://example.com/a(b ).").is_empty());
    }

    #[test]
    fn quote_before_a_low_opening_quote_reports_the_low_quote() {
        let text = "To x\"\u{201e} dalej.";
        let matches = run(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(
            &text[matches[0].offset()..matches[0].offset() + matches[0].length()],
            "\u{201e}"
        );
    }

    #[test]
    fn hyphen_then_quote_is_treated_as_opening() {
        let text = "To xxx-\"yyy.";
        let matches = run(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].offset(), text.find('"').unwrap());
    }

    #[test]
    fn special_case_next_token_clauses_pair_the_quotes() {
        assert!(run("To \"Go\"-button.").is_empty());
        assert!(run("To \"I\"s.").is_empty());
    }

    #[test]
    fn bracket_at_text_start_is_reported() {
        let text = "\"Niedomkni\u{119}ty cytat.";
        let matches = run(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].offset(), 0);
    }

    #[test]
    fn closing_paren_at_text_end_is_reported() {
        let text = "B\u{142}\u{105}d tutaj)";
        let matches = run(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].offset(), text.find(')').unwrap());
    }

    #[test]
    fn comma_glued_quote_is_treated_as_opening() {
        let text = "Zobacz,\"cytat dalej.";
        let matches = run(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].offset(), text.find('"').unwrap());
    }

    #[test]
    fn context_snaps_to_char_boundaries_around_multibyte_text() {
        let text = format!(
            "\u{2026}{} \"\u{105}\u{2026}{}.",
            "a".repeat(27),
            "\u{2026}".repeat(10)
        );
        let matches = run(&text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].offset(), 31);
        assert_eq!(matches[0].context().text(), &text[3..61]);
    }

    #[test]
    fn opening_quote_before_a_later_sentence_is_reported() {
        let text = "To \"cytat Drugi.";
        let pipeline = PolishPipeline::shared();
        let first = pipeline.analyze_range(text, 0, 9);
        let second = pipeline.analyze_range(text, 10, 16);
        let rule = PolishUnpairedBracketsRule::new();
        let matches = og_core::rule::TextLevelRule::match_text(&rule, text, &[first, second]);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].offset(), text.find('"').unwrap());
    }

    #[test]
    fn punct_helpers_pin_the_java_punctuation_patterns() {
        assert!(punct_matches("."));
        assert!(punct_matches("\u{2026}"));
        assert!(!punct_matches("ab"));
        assert!(!punct_matches(""));
        for apostrophed in ["l'", "d'", "m'", "n'", "s'", "t'", "T'"] {
            assert!(punct_no_dot_matches(apostrophed), "{apostrophed}");
        }
        assert!(punct_no_dot_matches(","));
        assert!(punct_no_dot_matches("-"));
        assert!(!punct_no_dot_matches("."));
        assert!(!punct_no_dot_matches("a"));
        assert!(!punct_no_dot_matches("a'"));
        assert!(!punct_no_dot_matches("l"));
        assert!(!is_java_punct('\u{201e}'));
        assert!(is_java_punct('\u{2014}'));
    }
}
