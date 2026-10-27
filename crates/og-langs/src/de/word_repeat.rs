use super::agreement::immunized_indices;
use super::helpers::{eq_ignore_case, has_pos_tag_starting_with};
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use std::collections::HashSet;
use std::sync::LazyLock;

static GERMAN_WORD_REPEAT_ANTI_PATTERNS: LazyLock<Option<Vec<crate::engine::XmlPatternRule>>> =
    LazyLock::new(|| {
        let compiler = og_xml::XmlCompiler::new();
        let src = include_str!("word_repeat_antipatterns.xml");
        match compiler.compile_file(src) {
            Ok(set) => Some(
                set.rules
                    .into_iter()
                    .map(crate::engine::XmlPatternRule::new)
                    .collect(),
            ),
            Err(_) => None,
        }
    });

static SINGLE_CHAR: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[a-zA-Z]$").unwrap());

const NAME_REPETITIONS: &[&str] = &[
    "Phi", "Li", "Xiao", "Duran", "Wagga", "Abdullah", "Nwe", "Pago", "Cao",
];

fn is_non_bmp(token: &str) -> bool {
    token.chars().any(|c| c as u32 > 0xFFFF)
}

fn word_for_speller(token: &str) -> bool {
    token.chars().all(|c| {
        c.is_alphabetic()
            || c.is_numeric()
            || c.is_ascii_punctuation()
            || c == ' '
            || matches!(
                c,
                '\u{00A0}' | '\u{2028}' | '\u{2029}' | '\u{1680}' | '\u{2000}'
                    ..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
            )
    })
}

fn is_numeric_space(token: &str) -> bool {
    token.chars().all(|c| c == ' ' || c.is_numeric())
}

fn is_word(token: &str) -> bool {
    if token.len() > 1 && is_non_bmp(token) && !word_for_speller(token) {
        return false;
    }
    if is_numeric_space(token) {
        return false;
    }
    let mut chars = token.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && !c.is_alphabetic()
    {
        return false;
    }
    true
}

pub struct GermanWordRepeatRule {
    category: Category,
}

impl GermanWordRepeatRule {
    pub fn new() -> Self {
        Self {
            category: Category::new("REDUNDANCY", "Redundanz"),
        }
    }

    fn word_repetition_of(word: &str, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        position > 0
            && tokens[position - 1].token().token() == word
            && tokens[position].token().token() == word
    }

    fn ignore(&self, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        let prev = tokens[position - 1].token().token();
        let curr = tokens[position].token().token();
        if (position != 2 && prev == "Sie" && curr == "sie") || (prev == "sie" && curr == "Sie") {
            return true;
        }
        if (position != 2 && prev == "Waren" && curr == "waren")
            || (prev == "waren" && curr == "Waren")
        {
            return true;
        }
        if position > 2 && prev == "sie" && curr == "sie" && self.sie_context(tokens, position) {
            return true;
        }
        if SINGLE_CHAR.is_match(curr)
            && position > 1
            && position + 1 < tokens.len()
            && tokens
                .get(position - 2)
                .is_some_and(|t| SINGLE_CHAR.is_match(t.token().token()))
            && SINGLE_CHAR.is_match(tokens[position + 1].token().token())
        {
            return true;
        }
        NAME_REPETITIONS
            .iter()
            .any(|name| Self::word_repetition_of(name, tokens, position))
    }

    fn sie_context(&self, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        if tokens[position - 2].has_pos_tag("KON:UNT") {
            return true;
        }
        if tokens.len() - 1 > position {
            let before = &tokens[position - 2];
            let after = &tokens[position + 1];
            return (has_pos_tag_starting_with(before, "VER:3:") && after.has_pos_tag("ZUS"))
                || (has_pos_tag_starting_with(before, "VER:MOD:3")
                    && after.has_pos_tag("VER:INF:NON"));
        }
        false
    }
}

impl Default for GermanWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for GermanWordRepeatRule {
    fn id(&self) -> &str {
        "GERMAN_WORD_REPEAT_RULE"
    }

    fn description(&self) -> &str {
        "Wortwiederholung (z. B. 'als als')"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Redundancy
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let immunized: HashSet<usize> = GERMAN_WORD_REPEAT_ANTI_PATTERNS
            .as_ref()
            .map(|rules| immunized_indices(rules, sentence))
            .unwrap_or_default();
        let mut rule_matches = Vec::new();
        let mut prev_token: &str = "";
        for i in 1..tokens.len() {
            let token = tokens[i].token().token();
            if immunized.contains(&i) {
                prev_token = "";
                continue;
            }
            if is_word(token)
                && eq_ignore_case(prev_token, token)
                && !self.ignore(&tokens, i)
                && let Some(first) = tokens.get(i - 1)
            {
                let start = first.token().start();
                let end = tokens[i].token().start() + prev_token.len();
                if end > start {
                    let rel_start = start - sentence.start();
                    let rel_end = end - sentence.start();
                    let context_start = rel_start.saturating_sub(20);
                    let context_end = std::cmp::min(rel_end + 20, sentence.text().len());
                    let context_text = sentence.text()[context_start..context_end].to_string();
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category.clone())
                        .with_issue_type(IssueType::Redundancy.as_str());
                    rule_matches.push(
                        RuleMatch::new(
                            "Möglicher Tippfehler: ein Wort wird wiederholt",
                            start,
                            end - start,
                            rm_rule,
                            RuleMatchContext::new(
                                context_text,
                                context_start,
                                context_end - context_start,
                            ),
                        )
                        .with_sentence(sentence.text().to_string())
                        .with_replacements(vec![SuggestedReplacement::new(prev_token)]),
                    );
                }
            }
            prev_token = token;
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_sentence(text: &str, words: &[&str]) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut pos = 0usize;
        for (i, w) in words.iter().enumerate() {
            if i == 0 {
                tokens.push(AnalyzedTokenReadings::new(
                    og_core::AnalyzedToken::new("<S>", 0, 0)
                        .with_pos_tags(vec!["SENT_START".to_string()]),
                ));
                continue;
            }
            let offset = text[pos..].find(w).expect("word in text");
            let start = pos + offset;
            let end = start + w.len();
            pos = end;
            tokens.push(AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
                *w, start, end,
            )));
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn detects_simple_repetition() {
        let rule = GermanWordRepeatRule::new();
        let sentence = build_sentence(
            "In diesem Satz ist ist ein Wort doppelt.",
            &[
                "<S>", "In", "diesem", "Satz", "ist", "ist", "ein", "Wort", "doppelt", ".",
            ],
        );
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 15);
        assert_eq!(matches[0].length(), 7);
    }

    #[test]
    fn immunized_repetition_is_skipped() {
        let rule = GermanWordRepeatRule::new();
        let sentence = build_sentence(
            "Moin Moin, sagte er.",
            &["<S>", "Moin", "Moin", ",", "sagte", "er", "."],
        );
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn sie_sie_case_pair_ignored() {
        let rule = GermanWordRepeatRule::new();
        let sentence = build_sentence(
            "Warum fragen Sie sie nicht selbst?",
            &[
                "<S>", "Warum", "fragen", "Sie", "sie", "nicht", "selbst", "?",
            ],
        );
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn das_das_after_ist_is_immunized() {
        let rule = GermanWordRepeatRule::new();
        let mut sentence = build_sentence(
            "Wahrscheinlich ist das das Problem.",
            &["<S>", "Wahrscheinlich", "ist", "das", "das", "Problem", "."],
        );
        let token = og_core::AnalyzedToken::new("Problem", 31, 38)
            .with_pos_tags(vec!["SUB:NOM:SIN:NEU".to_string()]);
        if let Some(t) = sentence
            .tokens_mut()
            .iter_mut()
            .find(|t| t.token().token() == "Problem")
        {
            *t = AnalyzedTokenReadings::new(token);
        }
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn numeric_repetition_not_flagged() {
        let rule = GermanWordRepeatRule::new();
        let sentence = build_sentence(
            "Er zahlte 20 20 Euro.",
            &["<S>", "Er", "zahlte", "20", "20", "Euro", "."],
        );
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    fn build_tagged_sentence(text: &str, words: &[(&str, Vec<&str>)]) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut pos = 0usize;
        for (i, (w, tags)) in words.iter().enumerate() {
            if i == 0 {
                tokens.push(AnalyzedTokenReadings::new(
                    og_core::AnalyzedToken::new("<S>", 0, 0)
                        .with_pos_tags(vec!["SENT_START".to_string()]),
                ));
                continue;
            }
            let offset = text[pos..].find(w).expect("word in text");
            let start = pos + offset;
            pos = start + w.len();
            let mut token = og_core::AnalyzedToken::new(*w, start, pos);
            if !tags.is_empty() {
                token = token.with_pos_tags(tags.iter().map(|t| t.to_string()).collect());
            }
            tokens.push(AnalyzedTokenReadings::new(token));
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn name_repetitions_are_ignored() {
        let rule = GermanWordRepeatRule::new();
        let sentence = build_sentence(
            "Xiao Xiao kam vorbei.",
            &["<S>", "Xiao", "Xiao", "kam", "vorbei", "."],
        );
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn case_flipped_waren_pairs_are_ignored() {
        let rule = GermanWordRepeatRule::new();
        let first = build_sentence(
            "Dort waren Waren gelagert.",
            &["<S>", "Dort", "waren", "Waren", "gelagert", "."],
        );
        assert!(rule.match_sentence(&first).is_empty());
        let second = build_sentence(
            "Die Waren waren teuer.",
            &["<S>", "Die", "Waren", "waren", "teuer", "."],
        );
        assert!(rule.match_sentence(&second).is_empty());
    }

    #[test]
    fn sie_sie_after_subordinating_conjunction_is_ignored() {
        let rule = GermanWordRepeatRule::new();
        let sentence = build_tagged_sentence(
            "dass sie sie sehen.",
            &[
                ("<S>", vec![]),
                ("dass", vec!["KON:UNT"]),
                ("sie", vec![]),
                ("sie", vec![]),
                ("sehen", vec![]),
                (".", vec![]),
            ],
        );
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn sie_sie_between_verb_and_zus_is_ignored() {
        let rule = GermanWordRepeatRule::new();
        let sentence = build_tagged_sentence(
            "kamen sie sie an.",
            &[
                ("<S>", vec![]),
                ("kamen", vec!["VER:3:PLU"]),
                ("sie", vec![]),
                ("sie", vec![]),
                ("an", vec!["ZUS"]),
                (".", vec![]),
            ],
        );
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn sie_sie_between_modal_and_infinitive_is_ignored() {
        let rule = GermanWordRepeatRule::new();
        let sentence = build_tagged_sentence(
            "wollten sie sie gehen.",
            &[
                ("<S>", vec![]),
                ("wollten", vec!["VER:MOD:3:PLU"]),
                ("sie", vec![]),
                ("sie", vec![]),
                ("gehen", vec!["VER:INF:NON"]),
                (".", vec![]),
            ],
        );
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn single_letter_sandwich_is_ignored() {
        let rule = GermanWordRepeatRule::new();
        let sentence = build_sentence("b a a c.", &["<S>", "b", "a", "a", "c", "."]);
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn metadata_pins_the_registration() {
        let rule = GermanWordRepeatRule::default();
        assert_eq!(rule.id(), "GERMAN_WORD_REPEAT_RULE");
        assert_eq!(rule.description(), "Wortwiederholung (z. B. 'als als')");
        assert_eq!(rule.category().id().as_str(), "REDUNDANCY");
        assert_eq!(rule.category().name(), "Redundanz");
        assert!(matches!(rule.issue_type(), IssueType::Redundancy));
    }
}
