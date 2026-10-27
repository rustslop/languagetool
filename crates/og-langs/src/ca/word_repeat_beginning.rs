use og_core::rule::TextLevelRule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};

const MSG_ADV: &str = "Dues frases consecutives comencen amb el mateix element.";
const MSG_WORD: &str = "Tres frases successives comencen amb la mateixa paraula.";
const MSG_THESAURUS: &str = "Considereu reescriure la frase o usar un sin\u{f2}nim.";

const ADD_ADVERBS: [&str; 3] = ["Igualment", "Tamb\u{e9}", "Addicionalment"];
const CONTRAST_CONJ: [&str; 3] = ["Per\u{f2}", "Emper\u{f2}", "Mes"];
const CAUSE_CONJ: [&str; 2] = ["Perqu\u{e8}", "Car"];
const EMPHASIS_ADVERBS: [&str; 4] = [
    "\u{d2}bviament",
    "Clarament",
    "Absolutament",
    "Definitivament",
];
const EXPLAIN_ADVERBS: [&str; 4] = [
    "Espec\u{ed}ficament",
    "Concretament",
    "Particularment",
    "Precisament",
];
const PERSONAL_PRONOUNS: [&str; 13] = [
    "jo",
    "tu",
    "ell",
    "ella",
    "nosaltres",
    "vosaltres",
    "ells",
    "elles",
    "vost\u{e8}",
    "vost\u{e8}s",
    "vost\u{e9}",
    "vost\u{e9}s",
    "v\u{f3}s",
];
const ADD_EXPRESSIONS: [&str; 2] = ["Aix\u{ed} mateix", "A m\u{e9}s a m\u{e9}s"];
const CONTRAST_EXPRESSIONS: [&str; 3] = ["Aix\u{ed} i tot", "D'altra banda", "Per altra part"];
const CAUSE_EXPRESSIONS: [&str; 4] = ["Ja que", "Per tal com", "Pel fet que", "Puix que"];
const EXCEPTIONS_START: [&str; 16] = [
    "l'",
    "el",
    "la",
    "els",
    "les",
    "punt",
    "article",
    "m\u{f2}dul",
    "part",
    "sessi\u{f3}",
    "unitat",
    "tema",
    "a",
    "per",
    "en",
    "com",
];

fn base_is_exception(token: &str) -> bool {
    matches!(
        token,
        ":" | "\u{2013}"
            | "-"
            | "\u{2714}\u{fe0f}"
            | "\u{27a1}\u{fe0f}"
            | "\u{2014}"
            | "\u{2b50}\u{fe0f}"
            | "\u{26a0}\u{fe0f}"
    )
}

fn is_adverb(atr: &AnalyzedTokenReadings) -> bool {
    if atr.has_pos_tag("RG") || atr.has_pos_tag("LOC_ADV") {
        return true;
    }
    let tok = atr.token().token();
    ADD_ADVERBS.contains(&tok)
        || CONTRAST_CONJ.contains(&tok)
        || EMPHASIS_ADVERBS.contains(&tok)
        || EXPLAIN_ADVERBS.contains(&tok)
        || CAUSE_CONJ.contains(&tok)
}

fn suggestions_for(token: &str) -> Vec<String> {
    let lower_tok = token.to_lowercase();
    if PERSONAL_PRONOUNS.contains(&lower_tok.as_str()) {
        return vec![
            format!("A m\u{e9}s a m\u{e9}s, {lower_tok}"),
            format!("Igualment, {lower_tok}"),
            format!("No sols aix\u{f3}, sin\u{f3} que {lower_tok}"),
        ];
    }
    if ADD_ADVERBS.contains(&token) {
        let mut out: Vec<String> = ADD_ADVERBS
            .iter()
            .filter(|adv| **adv != token)
            .map(|s| s.to_string())
            .collect();
        out.extend(ADD_EXPRESSIONS.iter().map(|s| s.to_string()));
        return out;
    }
    if CONTRAST_CONJ.contains(&token) {
        return CONTRAST_EXPRESSIONS.iter().map(|s| s.to_string()).collect();
    }
    if EMPHASIS_ADVERBS.contains(&token) {
        return EMPHASIS_ADVERBS
            .iter()
            .filter(|adv| **adv != token)
            .map(|s| s.to_string())
            .collect();
    }
    if EXPLAIN_ADVERBS.contains(&token) {
        return EXPLAIN_ADVERBS
            .iter()
            .filter(|adv| **adv != token)
            .map(|s| s.to_string())
            .collect();
    }
    if CAUSE_CONJ.contains(&token) {
        let mut out: Vec<String> = CAUSE_CONJ
            .iter()
            .filter(|adv| **adv != token)
            .map(|s| s.to_string())
            .collect();
        out.extend(CAUSE_EXPRESSIONS.iter().map(|s| s.to_string()));
        return out;
    }
    Vec::new()
}

pub struct CatalanWordRepeatBeginningRule;

impl CatalanWordRepeatBeginningRule {
    pub fn new() -> Self {
        Self
    }

    fn is_exception(&self, token: &str) -> bool {
        base_is_exception(token)
            || token.chars().next().is_some_and(char::is_numeric)
            || EXCEPTIONS_START.contains(&token.to_lowercase().as_str())
    }
}

impl Default for CatalanWordRepeatBeginningRule {
    fn default() -> Self {
        Self::new()
    }
}

fn ends_like_real_sentence(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.ends_with('.') || trimmed.ends_with('?') || trimmed.ends_with('!')
}

impl TextLevelRule for CatalanWordRepeatBeginningRule {
    fn id(&self) -> &str {
        "CATALAN_WORD_REPEAT_BEGINNING_RULE"
    }

    fn description(&self) -> &str {
        "Frases successives que comencen amb la mateixa paraula"
    }

    fn is_picky(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("REPETITIONS_STYLE", "Repeticions (estil)")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let bases = super::sentence_frame_bases(sentences);
        let mut last_token = String::new();
        let mut before_last_token = String::new();
        let mut matches = Vec::new();
        let mut prev_ends_like_sentence = false;
        for (s_idx, sentence) in sentences.iter().enumerate() {
            let tokens = sentence.non_whitespace_tokens();
            let mut token = String::new();
            if tokens.len() > 1 {
                token = tokens[1].token().token().to_string();
                if tokens.len() > 3 {
                    let analyzed_token = tokens[1];
                    let is_word = token.chars().count() != 1
                        || token.chars().next().is_some_and(char::is_alphabetic);
                    let tok2 = tokens[2].token().token();
                    let tok3 = tokens[3].token().token();
                    if is_word
                        && last_token == token
                        && !self.is_exception(&token)
                        && !self.is_exception(tok2)
                        && !self.is_exception(tok3)
                        && prev_ends_like_sentence
                    {
                        let short_msg = if is_adverb(analyzed_token) {
                            MSG_ADV
                        } else if before_last_token == token {
                            MSG_WORD
                        } else {
                            ""
                        };
                        if !short_msg.is_empty() {
                            let msg = format!("{short_msg} {MSG_THESAURUS}");
                            let start_pos = bases[s_idx] + analyzed_token.token().start();
                            let length = token.len();
                            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                                .with_picky(true)
                                .with_category(self.category())
                                .with_issue_type(IssueType::Style.as_str());
                            let mut rm = RuleMatch::new(
                                msg,
                                start_pos,
                                length,
                                rm_rule,
                                RuleMatchContext::new(token.clone(), start_pos, length),
                            )
                            .with_short_message(short_msg)
                            .with_sentence(sentence.text().to_string());
                            let suggestions = suggestions_for(&token);
                            if !suggestions.is_empty() {
                                rm = rm.with_replacements(
                                    suggestions
                                        .iter()
                                        .map(|s| SuggestedReplacement::new(s.clone()))
                                        .collect(),
                                );
                            }
                            matches.push(rm);
                        }
                    }
                }
            }
            before_last_token = last_token;
            last_token = token;
            prev_ends_like_sentence = ends_like_real_sentence(sentence.text());
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ca::example_suite::CatalanPipeline;

    fn analyze(text: &str) -> Vec<AnalyzedSentence> {
        CatalanPipeline::shared()
            .analyze_offset_text(text)
            .into_iter()
            .map(|(s, _)| s)
            .collect()
    }

    #[test]
    fn flags_repeated_però_beginning_with_contrast_suggestions() {
        let rule = CatalanWordRepeatBeginningRule::new();
        let text = "Per\u{f2} el carrer \u{e9}s tot modernista. Per\u{f2} t\u{e9} nom de poeta.";
        let sentences = analyze(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1);
        let values: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(
            values,
            vec!["Aix\u{ed} i tot", "D'altra banda", "Per altra part"]
        );
        assert!(matches[0].rule().is_picky());
    }

    #[test]
    fn three_sentences_with_the_same_word_flag_only_from_the_third_on() {
        let rule = CatalanWordRepeatBeginningRule::new();
        let text = "Ell \u{e9}s alt. Ell \u{e9}s gran. Ell \u{e9}s fort.";
        let sentences = analyze(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(
            matches.len(),
            1,
            "two equal beginnings stay silent, the third fires MSG_WORD"
        );
        assert!(matches[0].message().starts_with(MSG_WORD));
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value().starts_with("A m\u{e9}s a m\u{e9}s, ell"))
        );
    }

    #[test]
    fn distinct_beginnings_stay_silent() {
        let rule = CatalanWordRepeatBeginningRule::new();
        let text = "La casa \u{e9}s gran. El cotxe \u{e9}s blau.";
        let sentences = analyze(text);
        assert!(rule.match_text(text, &sentences).is_empty());
    }

    #[test]
    fn articles_in_the_exception_list_stay_silent() {
        let rule = CatalanWordRepeatBeginningRule::new();
        let text = "El pa \u{e9}s bo. El vi \u{e9}s millor.";
        let sentences = analyze(text);
        assert!(rule.match_text(text, &sentences).is_empty());
    }

    #[test]
    fn perquè_gets_the_cause_expressions() {
        let rule = CatalanWordRepeatBeginningRule::new();
        let text = "Perqu\u{e8} plou. Perqu\u{e8} fa fred.";
        let sentences = analyze(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1);
        let values: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(
            values,
            vec!["Car", "Ja que", "Per tal com", "Pel fet que", "Puix que"]
        );
    }

    #[test]
    fn també_gets_the_add_sibling_adverbs_and_expressions() {
        let rule = CatalanWordRepeatBeginningRule::new();
        let text = "Tamb\u{e9} plou. Tamb\u{e9} fa fred.";
        let sentences = analyze(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1, "{matches:?}");
        let values: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(
            values,
            vec![
                "Igualment",
                "Addicionalment",
                "Aix\u{ed} mateix",
                "A m\u{e9}s a m\u{e9}s"
            ]
        );
    }

    #[test]
    fn emphasis_adverbs_suggest_their_siblings() {
        let rule = CatalanWordRepeatBeginningRule::new();
        let text = "Clarament guanya. Clarament perd.";
        let sentences = analyze(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1, "{matches:?}");
        let values: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(
            values,
            vec!["\u{d2}bviament", "Absolutament", "Definitivament"]
        );
    }

    #[test]
    fn explain_adverbs_suggest_their_siblings() {
        let rule = CatalanWordRepeatBeginningRule::new();
        let text = "Concretament aix\u{f2}. Concretament all\u{f2}.";
        let sentences = analyze(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1, "{matches:?}");
        let values: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(
            values,
            vec!["Espec\u{ed}ficament", "Particularment", "Precisament"]
        );
    }

    #[test]
    fn dash_as_the_second_token_is_an_exception() {
        let rule = CatalanWordRepeatBeginningRule::new();
        let text = "Paraula \u{2013} gran. Paraula \u{2013} petit.";
        let sentences = analyze(text);
        assert!(rule.match_text(text, &sentences).is_empty());
    }

    #[test]
    fn single_letter_tokens_run_the_alphabetic_check() {
        let rule = CatalanWordRepeatBeginningRule::new();
        let text = "A casa vostra. A fora plou.";
        let sentences = analyze(text);
        assert!(rule.match_text(text, &sentences).is_empty());
    }

    #[test]
    fn two_token_sentences_skip_the_comparison() {
        let rule = CatalanWordRepeatBeginningRule::new();
        let text = "Plou. Plou.";
        let sentences = analyze(text);
        assert!(rule.match_text(text, &sentences).is_empty());
    }

    #[test]
    fn rg_tagged_adverbs_fire_with_no_list_suggestions() {
        use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

        fn rg_sentence(text: &str, adverb: &str) -> AnalyzedSentence {
            let mut sentence = AnalyzedSentence::new(text, 0, text.len());
            sentence.set_tokens(vec![
                AnalyzedTokenReadings::new(
                    AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
                ),
                AnalyzedTokenReadings::new(
                    AnalyzedToken::new(adverb, 0, adverb.len())
                        .with_pos_tags(vec!["RG".to_string()]),
                ),
                AnalyzedTokenReadings::new(AnalyzedToken::new(
                    "plou",
                    adverb.len() + 1,
                    text.len() - 1,
                )),
                AnalyzedTokenReadings::new(AnalyzedToken::new(".", text.len() - 1, text.len())),
            ]);
            sentence
        }

        let rule = CatalanWordRepeatBeginningRule::new();
        let sentences = [
            rg_sentence("Sovint plou.", "Sovint"),
            rg_sentence("Sovint plou.", "Sovint"),
        ];
        let matches = rule.match_text("Sovint plou. Sovint plou.", &sentences);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert!(matches[0].message().starts_with(MSG_ADV));
        assert!(matches[0].replacements().is_empty());
    }

    #[test]
    fn metadata_pins_the_registration() {
        let rule = CatalanWordRepeatBeginningRule::default();
        assert_eq!(rule.id(), "CATALAN_WORD_REPEAT_BEGINNING_RULE");
        assert_eq!(
            rule.description(),
            "Frases successives que comencen amb la mateixa paraula"
        );
        assert!(rule.is_picky());
        assert_eq!(rule.category().id().as_str(), "REPETITIONS_STYLE");
        assert!(matches!(rule.issue_type(), IssueType::Style));
    }
}
