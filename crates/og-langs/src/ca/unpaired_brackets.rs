use og_core::rule::TextLevelRule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use std::sync::LazyLock;

pub(crate) const CA_START_SYMBOLS: [&str; 8] =
    ["[", "(", "{", "\u{201c}", "\u{ab}", "\"", "'", "\u{2018}"];
pub(crate) const CA_END_SYMBOLS: [&str; 8] =
    ["]", ")", "}", "\u{201d}", "\u{bb}", "\"", "'", "\u{2019}"];

const MESSAGE: &str = "S\u{ed}mbol sense parella. Afegiu-lo i situeu-lo manualment en el lloc adequat, o b\u{e9} esborreu-lo.";

static NUMERALS: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        "(?i)^(?:\\d{1,2}?[a-z']*|M*(?:D?C{0,3}|C[DM])(?:L?X{0,3}|X[LC])(?:V?I{0,3}|I[VX]))$",
    )
    .unwrap()
});
static VALID_BEFORE_CLOSING_PARENTHESIS: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^(?:\d+|[a-zA-Z])$").unwrap());
static NUMBER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^(?:\d[\d., ]+\d|\d{1,2})$").unwrap());
static URL: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new("^https?://.+$").unwrap());
static LETTER_CLOSE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[a-zA-Z]\\)$").unwrap());
static NEWLINE_LETTER_CLOSE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^\n[a-zA-Z]\\)$").unwrap());

fn is_punct_char(c: char) -> bool {
    c.is_ascii_punctuation() || matches!(c, '\u{2026}' | '\u{2013}' | '\u{2014}')
}

fn is_punctuation_token(token: &str) -> bool {
    let mut chars = token.chars();
    chars.next().is_some_and(is_punct_char) && chars.next().is_none()
}

fn is_punctuation_no_dot_token(token: &str) -> bool {
    if matches!(
        token,
        "l'" | "d'" | "m'" | "n'" | "s'" | "t'" | "L'" | "D'" | "M'" | "N'" | "S'" | "T'"
    ) {
        return true;
    }
    let mut chars = token.chars();
    match chars.next() {
        Some(c) => (is_punct_char(c) && c != '.') && chars.next().is_none(),
        None => false,
    }
}

#[derive(Clone)]
struct SymbolLocator {
    symbol: String,
    is_opening: bool,
    start_pos: usize,
    sentence_idx: usize,
}

struct SentenceTokens<'a> {
    tokens: Vec<&'a AnalyzedTokenReadings>,
    whitespace_before: Vec<bool>,
}

fn sentence_tokens(sentence: &AnalyzedSentence) -> SentenceTokens<'_> {
    let mut tokens = Vec::with_capacity(sentence.tokens().len());
    let mut whitespace_before = Vec::with_capacity(sentence.tokens().len());
    let mut prev_whitespace = false;
    for token in sentence.tokens() {
        if token.is_whitespace() {
            prev_whitespace = true;
            continue;
        }
        tokens.push(token);
        whitespace_before.push(prev_whitespace);
        prev_whitespace = false;
    }
    SentenceTokens {
        tokens,
        whitespace_before,
    }
}

fn has_pos_tag_starting_with(atr: &AnalyzedTokenReadings, prefix: &str) -> bool {
    atr.readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.starts_with(prefix)))
}

fn is_end_symbol_unique(symbol: &str) -> bool {
    CA_END_SYMBOLS.iter().filter(|s| **s == symbol).count() == 1
}

pub struct CatalanUnpairedBracketsRule;

impl CatalanUnpairedBracketsRule {
    pub fn new() -> Self {
        Self
    }

    fn base_is_no_exception(
        token_str: &str,
        tokens: &[&AnalyzedTokenReadings],
        i: usize,
        whitespace_before: &[bool],
    ) -> bool {
        if i > 0 {
            let prev = tokens[i - 1].token().token();
            if URL.is_match(prev) && prev.contains('(') {
                return false;
            }
        }
        if i >= 2 {
            let prev_prev = tokens[i - 2].token().token();
            let prev = tokens[i - 1].token().token();
            let paren = token_str == ")" || token_str == "(";
            if paren && prev_prev == ":" && prev == "-" {
                return false;
            }
            if paren && prev_prev == ";" && prev == "-" {
                return false;
            }
        }
        if i >= 1 {
            let prev = tokens[i - 1].token().token();
            let paren = token_str == ")" || token_str == "(";
            if paren && (prev == ":" || prev == ";") && !whitespace_before[i] {
                return false;
            }
        }
        true
    }

    fn is_no_exception(
        &self,
        token_str: &str,
        tokens: &[&AnalyzedTokenReadings],
        i: usize,
        whitespace_before: &[bool],
    ) -> bool {
        if i < 1 {
            return true;
        }
        if has_pos_tag_starting_with(tokens[i], "NP")
            || has_pos_tag_starting_with(tokens[i], "allow_saxon_genitive")
        {
            return false;
        }
        if (token_str == "\u{2019}" || token_str == "'")
            && (has_pos_tag_starting_with(tokens[i], "N")
                || has_pos_tag_starting_with(tokens[i], "A"))
        {
            return false;
        }
        if !Self::base_is_no_exception(token_str, tokens, i, whitespace_before) {
            return false;
        }
        let degrees = (token_str == "\"" || token_str == "'")
            && NUMBER.is_match(tokens[i - 1].token().token())
            && !whitespace_before[i]
            && ((i > 2
                && (tokens[i - 2].token().token().contains('\u{ba}')
                    || tokens[i - 2].token().token().contains('\u{b0}')))
                || (i > 4
                    && (tokens[i - 4].token().token().contains('\u{ba}')
                        || tokens[i - 4].token().token().contains('\u{b0}'))));
        if degrees {
            return false;
        }
        if i == 1 && token_str == "\u{bb}" {
            return false;
        }
        if i > 1 && token_str == ")" {
            let mut is_there_opening_parenthesis = false;
            let mut k = 1usize;
            while i > k {
                let t = tokens[i - k].token().token();
                if t == ")" {
                    break;
                }
                if t == "(" {
                    is_there_opening_parenthesis = true;
                    break;
                }
                k += 1;
            }
            if !is_there_opening_parenthesis
                && VALID_BEFORE_CLOSING_PARENTHESIS.is_match(tokens[i - 1].token().token())
            {
                return false;
            }
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_symbol_stack(
        &self,
        start_pos: usize,
        tokens: &[&AnalyzedTokenReadings],
        whitespace_before: &[bool],
        i: usize,
        j: usize,
        symbol_stack: &mut Vec<SymbolLocator>,
        sentence_idx: usize,
    ) -> bool {
        let token = tokens[i].token().token();
        if token != CA_START_SYMBOLS[j] && token != CA_END_SYMBOLS[j] {
            return false;
        }
        let self_paired = CA_START_SYMBOLS[j] == CA_END_SYMBOLS[j];
        let preceded_by_whitespace = !self_paired
            || tokens[i - 1].is_sentence_start()
            || whitespace_before[i]
            || is_punctuation_no_dot_token(tokens[i - 1].token().token())
            || CA_START_SYMBOLS.contains(&tokens[i - 1].token().token());
        let is_special_case = if self_paired && i < tokens.len() - 1 {
            whitespace_before.get(i + 1).copied().unwrap_or(false)
                || is_punctuation_token(tokens[i + 1].token().token())
                || CA_END_SYMBOLS.contains(&tokens[i + 1].token().token())
                || tokens[i - 1].token().token().ends_with('-')
                || tokens[i + 1].token().token().starts_with('-')
                || tokens[i + 1].token().token() == "s"
        } else {
            true
        };
        let no_exception = self.is_no_exception(token, tokens, i, whitespace_before);
        if no_exception && preceded_by_whitespace && token == CA_START_SYMBOLS[j] {
            symbol_stack.push(SymbolLocator {
                symbol: CA_START_SYMBOLS[j].to_string(),
                is_opening: true,
                start_pos,
                sentence_idx,
            });
            return true;
        } else if no_exception
            && (is_special_case || tokens[i].is_sentence_end())
            && token == CA_END_SYMBOLS[j]
        {
            let numerals_bullet = (i > 2
                && CA_END_SYMBOLS[j] == ")"
                && (tokens[i - 3].has_pos_tag("SENT_START") || whitespace_before[i - 2])
                && tokens[i - 1].token().token() == "."
                && NUMERALS.is_match(tokens[i - 2].token().token())
                && symbol_stack.last().map(|l| l.symbol.as_str()) != Some("("))
                || (i > 1
                    && CA_END_SYMBOLS[j] == ")"
                    && NUMERALS.is_match(tokens[i - 1].token().token())
                    && symbol_stack.last().map(|l| l.symbol.as_str()) != Some("("));
            if numerals_bullet {
                return false;
            }
            match symbol_stack.last() {
                None => {
                    symbol_stack.push(SymbolLocator {
                        symbol: CA_END_SYMBOLS[j].to_string(),
                        is_opening: false,
                        start_pos,
                        sentence_idx,
                    });
                    return true;
                }
                Some(top) => {
                    if top.symbol == CA_START_SYMBOLS[j] {
                        symbol_stack.pop();
                        return true;
                    }
                    if is_end_symbol_unique(CA_END_SYMBOLS[j]) {
                        symbol_stack.push(SymbolLocator {
                            symbol: CA_END_SYMBOLS[j].to_string(),
                            is_opening: false,
                            start_pos,
                            sentence_idx,
                        });
                        return true;
                    }
                    if j == CA_END_SYMBOLS.len() - 1 {
                        symbol_stack.push(SymbolLocator {
                            symbol: CA_END_SYMBOLS[j].to_string(),
                            is_opening: false,
                            start_pos,
                            sentence_idx,
                        });
                        return true;
                    }
                }
            }
        }
        false
    }

    fn find_corresponding_symbol(symbol: &str) -> String {
        if let Some(idx) = CA_START_SYMBOLS.iter().position(|s| *s == symbol) {
            CA_END_SYMBOLS[idx].to_string()
        } else if let Some(idx) = CA_END_SYMBOLS.iter().position(|s| *s == symbol) {
            CA_START_SYMBOLS[idx].to_string()
        } else {
            String::new()
        }
    }
}

impl Default for CatalanUnpairedBracketsRule {
    fn default() -> Self {
        Self::new()
    }
}

struct MatchStackEntry {
    symbol: String,
    index: usize,
}

fn ends_like_real_sentence(text: &str) -> bool {
    let s = text.trim();
    s.ends_with('.') || s.ends_with('?') || s.ends_with('!')
}

fn char_boundary_range(text: &str, from: usize, to: usize) -> (usize, usize) {
    let mut start = from.min(text.len());
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = to.min(text.len());
    while end < text.len() && !text.is_char_boundary(end) {
        end += 1;
    }
    (start, end)
}

impl TextLevelRule for CatalanUnpairedBracketsRule {
    fn id(&self) -> &str {
        "UNPAIRED_BRACKETS"
    }

    fn description(&self) -> &str {
        "Claus, cometes, par\u{e8}ntesis i s\u{ed}mbols semblants desaparellats"
    }

    fn category(&self) -> Category {
        Category::new("PUNCTUATION", "Puntuaci\u{f3}")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Typographical
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let bases = super::sentence_frame_bases(sentences);
        let mut symbol_stack: Vec<SymbolLocator> = Vec::new();
        for (s_idx, sentence) in sentences.iter().enumerate() {
            let SentenceTokens {
                tokens,
                whitespace_before,
            } = sentence_tokens(sentence);
            for i in 1..tokens.len() {
                let start_pos = bases[s_idx] + tokens[i].token().start();
                for j in 0..CA_START_SYMBOLS.len() {
                    if self.fill_symbol_stack(
                        start_pos,
                        &tokens,
                        &whitespace_before,
                        i,
                        j,
                        &mut symbol_stack,
                        s_idx,
                    ) {
                        break;
                    }
                }
            }
        }
        let mut rule_matches: Vec<RuleMatch> = Vec::new();
        let mut rule_match_stack: Vec<MatchStackEntry> = Vec::new();
        let full_text: String = sentences
            .iter()
            .map(|s| s.text())
            .collect::<Vec<_>>()
            .concat();
        let ss_size = symbol_stack.len();
        let mut is_symmetric = false;
        if ss_size > 2 && ss_size % 2 == 1 {
            is_symmetric = true;
            for i in 0..ss_size / 2 {
                let start_idx = CA_START_SYMBOLS
                    .iter()
                    .position(|s| *s == symbol_stack[i].symbol.as_str());
                let end_idx = CA_END_SYMBOLS
                    .iter()
                    .position(|s| *s == symbol_stack[ss_size - 1].symbol.as_str());
                if start_idx != end_idx {
                    is_symmetric = false;
                    break;
                }
            }
        }
        let emit = |locator: &SymbolLocator,
                    rule_matches: &mut Vec<RuleMatch>,
                    rule_match_stack: &mut Vec<MatchStackEntry>|
         -> Option<RuleMatch> {
            if let Some(top) = rule_match_stack.last()
                && let Some(index) = CA_END_SYMBOLS
                    .iter()
                    .position(|s| *s == locator.symbol.as_str())
                && top.symbol == CA_START_SYMBOLS[index]
                && rule_matches.len() > top.index
            {
                rule_matches.remove(top.index);
                rule_match_stack.pop();
                return None;
            }
            rule_match_stack.push(MatchStackEntry {
                symbol: locator.symbol.clone(),
                index: rule_matches.len(),
            });
            let other_symbol = Self::find_corresponding_symbol(&locator.symbol);
            if locator.start_pos + locator.symbol.len() < full_text.len() {
                if locator.start_pos >= 2 {
                    let (ctx_start, ctx_end) = char_boundary_range(
                        &full_text,
                        locator.start_pos - 2,
                        locator.start_pos + locator.symbol.len(),
                    );
                    let context = &full_text[ctx_start..ctx_end];
                    if NEWLINE_LETTER_CLOSE.is_match(context) {
                        return None;
                    }
                } else if locator.start_pos >= 1 {
                    let (ctx_start, ctx_end) = char_boundary_range(
                        &full_text,
                        locator.start_pos - 1,
                        locator.start_pos + locator.symbol.len(),
                    );
                    let context = &full_text[ctx_start..ctx_end];
                    if LETTER_CLOSE.is_match(context) {
                        return None;
                    }
                }
            }
            let sentence = &sentences[locator.sentence_idx.min(sentences.len().saturating_sub(1))];
            let (rel_start, rel_end) = char_boundary_range(
                sentence.text(),
                locator
                    .start_pos
                    .saturating_sub(bases[locator.sentence_idx]),
                locator.start_pos + locator.symbol.len() - bases[locator.sentence_idx],
            );
            let context_text = &sentence.text()[rel_start..rel_end.min(sentence.text().len())];
            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category())
                .with_issue_type(IssueType::Typographical.as_str());
            let replacements = if locator.is_opening {
                vec![
                    SuggestedReplacement::new(format!("{}{}", locator.symbol, other_symbol)),
                    SuggestedReplacement::new(String::new()),
                ]
            } else {
                vec![
                    SuggestedReplacement::new(format!("{}{}", other_symbol, locator.symbol)),
                    SuggestedReplacement::new(String::new()),
                ]
            };
            Some(
                RuleMatch::new(
                    MESSAGE,
                    locator.start_pos,
                    locator.symbol.len(),
                    rm_rule,
                    RuleMatchContext::new(
                        context_text.to_string(),
                        rel_start,
                        rel_end.saturating_sub(rel_start),
                    ),
                )
                .with_replacements(replacements),
            )
        };
        if is_symmetric {
            let locator = symbol_stack[ss_size / 2].clone();
            if let Some(m) = emit(&locator, &mut rule_matches, &mut rule_match_stack) {
                rule_matches.push(m);
            }
        } else {
            for locator in &symbol_stack {
                if let Some(m) = emit(locator, &mut rule_matches, &mut rule_match_stack) {
                    let sentence = &sentences[locator.sentence_idx.min(sentences.len() - 1)];
                    if !locator.is_opening
                        || ends_like_real_sentence(sentence.text())
                        || sentences.len() - 1 > locator.sentence_idx
                    {
                        rule_matches.push(m);
                    }
                }
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ca::example_suite::CatalanPipeline;
    use og_core::AnalyzedToken;

    fn matches_for(text: &str) -> Vec<RuleMatch> {
        let pipeline = CatalanPipeline::shared();
        let sentences: Vec<AnalyzedSentence> = pipeline
            .analyze_offset_text(text)
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        CatalanUnpairedBracketsRule::new().match_text(text, &sentences)
    }

    type WordSpec<'a> = (&'a str, &'a [&'a str], bool);

    fn built_sentence(text: &str, words: &[WordSpec<'_>]) -> AnalyzedSentence {
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0usize;
        let mut first = true;
        for (surface, tags, ws_before) in words {
            if !first && *ws_before {
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    " ",
                    offset,
                    offset + 1,
                )));
                offset += 1;
            }
            first = false;
            let start = offset;
            let end = start + surface.len();
            offset = end;
            let mut atr = AnalyzedTokenReadings::new(AnalyzedToken::new(*surface, start, end));
            if !tags.is_empty() {
                let readings: Vec<AnalyzedToken> = tags
                    .iter()
                    .map(|tag| {
                        AnalyzedToken::new(*surface, start, end)
                            .with_lemma(surface.to_lowercase())
                            .with_pos_tags(vec![tag.to_string()])
                    })
                    .collect();
                atr = atr.with_readings(readings);
            }
            tokens.push(atr);
        }
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        sentence.set_tokens(tokens);
        sentence
    }

    fn built_matches(text: &str, words: &[WordSpec<'_>]) -> Vec<RuleMatch> {
        let sentence = built_sentence(text, words);
        CatalanUnpairedBracketsRule::new().match_text(text, &[sentence])
    }

    #[test]
    fn correct_java_cases_stay_silent() {
        for text in [
            "This is a [test] sentence...",
            "(Una frase de prova).",
            "Aquesta \u{e9}s la paraula 'prova'.",
            "\u{ab}\"\u{c9}s aix\u{ed}\" o no\u{bb}",
            "This is a sentence with a smiley :-)",
        ] {
            let matches = matches_for(text);
            assert!(
                matches.is_empty(),
                "{text:?} must not fire: {:?}",
                matches
                    .iter()
                    .map(|m| (m.offset(), m.length()))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn unpaired_opening_symbols_fire() {
        let matches = matches_for("Aix\u{f2} \u{e9}s (impossible. ");
        assert_eq!(matches.len(), 1);
        let matches = matches_for("Aix\u{f2} \u{e9}s \u{ab}important, oi que s\u{ed}?");
        assert_eq!(matches.len(), 1);
        let matches = matches_for("Aix\u{f2} \u{e9}s \"important i ara qu\u{e8} passa. ");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn stray_closing_parenthesis_fires() {
        let matches = matches_for("Aix\u{f2} \u{e9}s) impossible. ");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn leading_closed_guillemet_is_exempt() {
        assert!(matches_for("\u{bb}Aix\u{f2} em porta a demanar-t'ho.").is_empty());
    }

    #[test]
    fn closing_after_letter_without_opener_is_exempt() {
        assert!(matches_for("We discussed this in Chapter 1).").is_empty());
        assert!(matches_for("We discussed this in section 1a).").is_empty());
    }

    #[test]
    fn apostrophe_after_a_noun_reading_is_exempt() {
        let matches = built_matches(
            "Aquesta \u{e9}s l\u{2019}hora de les decisions.",
            &[
                ("Aquesta", &["PD0FS000"], true),
                ("\u{e9}s", &["VSIP3S00"], true),
                ("l", &["DA0CS0"], true),
                ("\u{2019}", &["NCFS000"], false),
                ("hora", &["NCFS000"], true),
                ("de", &["SPS00"], true),
                ("les", &["DA0FP0"], true),
                ("decisions", &["NCFP000"], true),
                (".", &["Fp"], true),
            ],
        );
        assert!(matches.is_empty(), "{matches:?}");
    }

    #[test]
    fn saxon_genitive_with_np_reading_is_exempt() {
        let matches = built_matches(
            "Harper's Dictionary of Classical Antiquities",
            &[
                ("Harper", &["NPMS00"], true),
                ("'", &["NPMS00"], false),
                ("Dictionary", &["NCMS00"], true),
                ("of", &["SPS00"], true),
                ("Classical", &["AQ0MS0"], true),
                ("Antiquities", &["NCMP00"], true),
            ],
        );
        assert!(matches.is_empty(), "{matches:?}");
    }

    #[test]
    fn degrees_minutes_seconds_stay_silent() {
        let matches = built_matches(
            "\u{c9}s a 60\u{ba} 50' 23\"",
            &[
                ("\u{c9}s", &["VSIP3S00"], true),
                ("a", &["SPS00"], true),
                ("60\u{ba}", &[], true),
                ("50", &["Zp-"], true),
                ("'", &[], false),
                ("23", &["Zp-"], false),
                ("\"", &[], false),
            ],
        );
        assert!(matches.is_empty(), "{matches:?}");
    }

    #[test]
    fn unmatched_degree_quote_without_the_degree_mark_fires() {
        let matches = built_matches(
            "Mida 50'",
            &[
                ("Mida", &["NCFS000"], true),
                ("50", &["Zp-"], true),
                ("'", &[], false),
            ],
        );
        assert_eq!(matches.len(), 1, "{matches:?}");
    }

    #[test]
    fn closing_parenthesis_after_digit_without_opener_is_exempt() {
        let matches = built_matches(
            "Chapter 1).",
            &[
                ("Chapter", &["NPMS00"], true),
                ("1", &["Zp-"], true),
                (")", &[], false),
                (".", &["Fp"], true),
            ],
        );
        assert!(matches.is_empty(), "{matches:?}");
    }

    #[test]
    fn roman_numeral_bullet_closing_paren_is_exempt() {
        assert!(matches_for("We discussed this in section iv).").is_empty());
    }

    #[test]
    fn numbered_bullet_with_dot_before_closing_paren_is_exempt() {
        assert!(matches_for("1.) El primer punt del programa.").is_empty());
    }

    #[test]
    fn mismatched_bracket_kinds_fire_one_match_per_symbol() {
        let matches = matches_for("This [is (a test} sentence.");
        assert_eq!(
            matches
                .iter()
                .map(|m| (m.offset(), m.length()))
                .collect::<Vec<_>>(),
            vec![(5, 1), (9, 1), (16, 1)],
            "{matches:?}"
        );

        let matches = matches_for("(This is a test\u{201d} sentence.");
        assert_eq!(
            matches
                .iter()
                .map(|m| (m.offset(), m.length()))
                .collect::<Vec<_>>(),
            vec![(0, 1), (15, 3)],
            "{matches:?}"
        );
    }

    #[test]
    fn odd_symmetric_stack_reports_only_the_middle_symbol() {
        let matches = built_matches(
            "Va dir \"una \"cosa \"estranya.",
            &[
                ("Va", &[], true),
                ("dir", &[], true),
                ("\"", &[], true),
                ("una", &[], false),
                ("\"", &[], true),
                ("cosa", &[], false),
                ("\"", &[], true),
                ("estranya", &[], false),
                (".", &[], false),
            ],
        );
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].offset(), 12);
        assert_eq!(matches[0].length(), 1);
    }

    #[test]
    fn url_token_containing_a_paren_is_exempt() {
        let matches = built_matches(
            "Vegeu http://example.com/f(x) avui.",
            &[
                ("Vegeu", &[], true),
                ("http://example.com/f(x", &[], true),
                (")", &[], false),
                ("avui", &[], true),
                (".", &[], true),
            ],
        );
        assert!(matches.is_empty(), "{matches:?}");
    }

    #[test]
    fn smiley_paren_tokens_are_exempt() {
        for head in [":", ";"] {
            let matches = built_matches(
                &format!("{head}-)"),
                &[(head, &[], true), ("-", &[], false), (")", &[], false)],
            );
            assert!(matches.is_empty(), "{head:?}: {matches:?}");
        }
    }

    #[test]
    fn paren_glued_to_a_colon_or_semicolon_is_exempt() {
        for text in [":(", ";(", ":)", ";)"] {
            let (head, tail) = text.split_at(1);
            let matches = built_matches(text, &[(head, &[], true), (tail, &[], false)]);
            assert!(matches.is_empty(), "{text:?}: {matches:?}");
        }
    }

    #[test]
    fn minute_and_second_quotes_after_a_degree_mark_are_exempt() {
        let adjacent = built_matches(
            "60\u{b0} 50' 23'.",
            &[
                ("60\u{b0}", &[], true),
                ("50", &["Zp-"], true),
                ("'", &[], false),
                ("23", &["Zp-"], false),
                ("'", &[], false),
                (".", &[], false),
            ],
        );
        assert!(adjacent.is_empty(), "{adjacent:?}");

        let distant = built_matches(
            "60\u{b0} C 50 23'.",
            &[
                ("60\u{b0}", &[], true),
                ("C", &[], true),
                ("50", &["Zp-"], true),
                ("23", &["Zp-"], false),
                ("'", &[], false),
                (".", &[], false),
            ],
        );
        assert!(distant.is_empty(), "{distant:?}");
    }

    #[test]
    fn two_stray_closing_parens_each_fire() {
        let matches = built_matches(
            "un) i dos).",
            &[
                ("un", &["NPMS00"], true),
                (")", &[], false),
                ("i", &[], true),
                ("dos", &["NCMP00"], true),
                (")", &[], false),
                (".", &[], false),
            ],
        );
        assert_eq!(
            matches.iter().map(|m| m.offset()).collect::<Vec<_>>(),
            vec![2, 9],
            "{matches:?}"
        );
    }

    #[test]
    fn unpaired_opening_in_an_earlier_sentence_fires() {
        let first = built_sentence(
            "Aix\u{f2} \u{e9}s (impossible sin\u{f3}",
            &[
                ("Aix\u{f2}", &[], true),
                ("\u{e9}s", &[], true),
                ("(", &[], true),
                ("impossible", &[], false),
                ("sin\u{f3}", &[], true),
            ],
        );
        let second = built_sentence("res.", &[("res", &[], true), (".", &[], false)]);
        let text = "Aix\u{f2} \u{e9}s (impossible sin\u{f3} res.";
        let matches = CatalanUnpairedBracketsRule::new().match_text(text, &[first, second]);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].offset(), 10);
    }

    #[test]
    fn default_rule_matches_and_reports_the_typographical_issue_type() {
        let rule = CatalanUnpairedBracketsRule::default();
        assert_eq!(rule.id(), "UNPAIRED_BRACKETS");
        assert_eq!(rule.issue_type(), IssueType::Typographical);
        let matches = matches_for("Aix\u{f2} \u{e9}s (impossible. ");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].rule().category().id().as_str(), "PUNCTUATION");
    }
}
