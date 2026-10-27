use super::agreement::immunized_indices;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, rule::Rule,
};
use std::collections::HashSet;
use std::sync::{Arc, LazyLock};

pub static SUBJECT_VERB_ANTI_PATTERNS: LazyLock<Option<Vec<crate::engine::XmlPatternRule>>> =
    LazyLock::new(|| {
        let compiler = og_xml::XmlCompiler::new();
        let src = include_str!("subject_verb_antipatterns.xml");
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

const SINGULAR: [&str; 2] = ["ist", "war"];
const PLURAL: [&str; 2] = ["sind", "waren"];
const PAIRS: [(&str, &str); 2] = [("ist", "sind"), ("war", "waren")];
const QUESTION_PRONOUNS: [&str; 1] = ["wie"];
const CURRENCIES: [&str; 3] = ["Dollar", "Euro", "Yen"];

static WER_ALLE_JEDE_MANCHE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^(?:wer|(?i)alle[nr]?|(?i)jede[rs]?|(?i)manche[nrs]?)$").unwrap()
});
static VER_PERSON_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"VER:[1-3]:.+").unwrap());

fn has_partial_pos_tag(token: &AnalyzedTokenReadings, needle: &str) -> bool {
    token
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.contains(needle)))
}

fn has_any_partial_pos_tag(token: &AnalyzedTokenReadings, tags: &[&str]) -> bool {
    tags.iter().any(|t| has_partial_pos_tag(token, t))
}

fn has_unknown_reading(token: &AnalyzedTokenReadings) -> bool {
    token
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().all(|t| t.is_empty()))
}

fn is_currency(token: &AnalyzedTokenReadings) -> bool {
    CURRENCIES.contains(&token.token().token())
}

fn plural_for(token: &str) -> &'static str {
    PAIRS
        .iter()
        .find(|(s, _)| *s == token)
        .map(|(_, p)| *p)
        .unwrap_or("sind")
}

fn singular_for(token: &str) -> &'static str {
    PAIRS
        .iter()
        .find(|(_, p)| *p == token)
        .map(|(s, _)| *s)
        .unwrap_or("ist")
}

pub struct SubjectVerbAgreementRule {
    tagger: Option<Arc<og_tagger::GermanTagger>>,
}

impl SubjectVerbAgreementRule {
    pub fn new(tagger: Option<Arc<og_tagger::GermanTagger>>) -> Self {
        Self { tagger }
    }

    fn prev_chunk_is_nominative(tokens: &[&AnalyzedTokenReadings], start_pos: usize) -> bool {
        let mut i = start_pos;
        while i > 0 {
            let t = tokens[i];
            if t.has_chunk("NPS") || t.has_chunk("NPP") {
                if has_partial_pos_tag(t, "NOM") {
                    return true;
                }
            } else {
                return false;
            }
            i -= 1;
        }
        false
    }

    fn has_unknown_token_at(
        tokens: &[&AnalyzedTokenReadings],
        start_pos: usize,
        end_pos: usize,
    ) -> bool {
        (start_pos..end_pos).any(|i| has_unknown_reading(tokens[i]))
    }

    fn has_unknown_token_to_the_left(tokens: &[&AnalyzedTokenReadings], start_pos: usize) -> bool {
        Self::has_unknown_token_at(tokens, 0, start_pos)
    }

    fn has_unknown_token_to_the_right(tokens: &[&AnalyzedTokenReadings], start_pos: usize) -> bool {
        Self::has_unknown_token_at(tokens, start_pos, tokens.len().saturating_sub(1))
    }

    fn has_question_pronoun_to_the_left(
        tokens: &[&AnalyzedTokenReadings],
        start_pos: usize,
    ) -> bool {
        let mut i = start_pos;
        while i > 0 {
            if QUESTION_PRONOUNS.contains(&tokens[i].token().token().to_lowercase().as_str()) {
                return true;
            }
            i -= 1;
        }
        false
    }

    fn has_verb_to_the_left(tokens: &[&AnalyzedTokenReadings], start_pos: usize) -> bool {
        let mut i = start_pos;
        while i > 0 {
            if tokens[i].has_pos_tag_matching_regex(&VER_PERSON_REGEX) {
                return true;
            }
            i -= 1;
        }
        false
    }

    fn contains_regex_to_the_left(
        re: &regex::Regex,
        tokens: &[&AnalyzedTokenReadings],
        start_pos: usize,
    ) -> bool {
        let mut i = start_pos;
        while i > 0 {
            if re.is_match(tokens[i].token().token()) {
                return true;
            }
            i -= 1;
        }
        false
    }

    fn contains_only_infinitives_to_the_left(
        &self,
        tokens: &[&AnalyzedTokenReadings],
        start_pos: usize,
    ) -> bool {
        let mut infinitives = 0usize;
        let mut i = start_pos;
        while i > 0 {
            let token = tokens[i];
            if has_partial_pos_tag(token, "SUB:") {
                let lower = token.token().token().to_lowercase();
                let lookup_infinitive = self
                    .tagger
                    .as_deref()
                    .and_then(|t| t.lookup(&lower))
                    .is_some_and(|readings| {
                        readings
                            .iter()
                            .any(|r| r.pos_tags().iter().any(|t| t.starts_with("VER:INF")))
                    });
                if lookup_infinitive {
                    infinitives += 1;
                } else {
                    return false;
                }
            }
            i -= 1;
        }
        infinitives >= 2
    }

    fn is_followed_by_nominative_plural(
        tokens: &[&AnalyzedTokenReadings],
        start_pos: usize,
    ) -> bool {
        (start_pos..tokens.len()).any(|i| {
            let t = tokens[i];
            has_any_partial_pos_tag(t, &["SUB", "PRO"])
                && (has_partial_pos_tag(t, "NOM:PLU") || t.has_chunk("NPP"))
        })
    }

    fn singular_match(
        &self,
        tokens: &[&AnalyzedTokenReadings],
        i: usize,
        sentence: &AnalyzedSentence,
    ) -> Option<RuleMatch> {
        let token = tokens[i];
        let token_str = token.token().token();
        if !SINGULAR.contains(&token_str) {
            return None;
        }
        let prev_token = tokens[i - 1];
        let next_token = tokens.get(i + 1);
        let matches = prev_token.has_chunk("NPP")
            && !prev_token.has_chunk("PP")
            && prev_token.token().token() != "Uhr"
            && !is_currency(prev_token)
            && next_token.is_none_or(|t| t.token().token() != "es")
            && Self::prev_chunk_is_nominative(tokens, i - 1)
            && !Self::has_unknown_token_to_the_left(tokens, i)
            && !Self::has_question_pronoun_to_the_left(tokens, i - 1)
            && !Self::has_verb_to_the_left(tokens, i - 1)
            && !Self::contains_regex_to_the_left(&WER_ALLE_JEDE_MANCHE, tokens, i - 1)
            && !self.contains_only_infinitives_to_the_left(tokens, i - 1);
        if !matches {
            return None;
        }
        let message = format!(
            "Bitte prüfen, ob hier <suggestion>{}</suggestion> stehen sollte.",
            plural_for(token_str)
        );
        Some(make_rule_match(sentence, token, message))
    }

    fn plural_match(
        &self,
        tokens: &[&AnalyzedTokenReadings],
        i: usize,
        sentence: &AnalyzedSentence,
    ) -> Option<RuleMatch> {
        let token = tokens[i];
        let token_str = token.token().token();
        if !PLURAL.contains(&token_str) {
            return None;
        }
        let prev_token = tokens[i - 1];
        let next_token = tokens.get(i + 1);
        let first_word = tokens.get(1).map(|t| t.token().token()).unwrap_or("");
        let matches = prev_token.has_chunk("NPS")
            && next_token.is_none_or(|t| t.token().token() != "Sie")
            && !prev_token.has_chunk("NPP")
            && !prev_token.has_chunk("PP")
            && !is_currency(prev_token)
            && Self::prev_chunk_is_nominative(tokens, i - 1)
            && !Self::has_unknown_token_to_the_left(tokens, i)
            && !Self::has_unknown_token_to_the_right(tokens, i + 1)
            && !matches!(first_word, "Alle" | "Viele")
            && !Self::is_followed_by_nominative_plural(tokens, i + 1);
        if !matches {
            return None;
        }
        let message = format!(
            "Bitte prüfen, ob hier <suggestion>{}</suggestion> stehen sollte.",
            singular_for(token_str)
        );
        Some(make_rule_match(sentence, token, message))
    }
}

fn make_rule_match(
    sentence: &AnalyzedSentence,
    token: &AnalyzedTokenReadings,
    message: String,
) -> RuleMatch {
    let from = token.token().start();
    let to = token.token().end();
    let rm_rule = RuleMatchRule::new(
        "DE_SUBJECT_VERB_AGREEMENT",
        "Kongruenz von Subjekt und Prädikat (unvollständig)",
    )
    .with_category(Category::new("GRAMMAR", "Grammatik"))
    .with_issue_type("grammar")
    .with_urls(vec![
        "https://dict.leo.org/grammatik/deutsch/Wort/Verb/Kategorien/Numerus-Person/ProblemNum.html"
            .to_string(),
    ]);
    RuleMatch::new(
        message,
        from,
        to - from,
        rm_rule,
        RuleMatchContext::new(sentence.text().to_string(), from, to - from),
    )
    .with_sentence(sentence.text().to_string())
}

impl Rule for SubjectVerbAgreementRule {
    fn id(&self) -> &str {
        "DE_SUBJECT_VERB_AGREEMENT"
    }

    fn description(&self) -> &str {
        "Kongruenz von Subjekt und Prädikat (unvollständig)"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("GRAMMAR", "Grammatik")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let immunized: HashSet<usize> = SUBJECT_VERB_ANTI_PATTERNS
            .as_ref()
            .map(|rules| immunized_indices(rules, sentence))
            .unwrap_or_default();
        let mut rule_matches = Vec::new();
        for i in 1..tokens.len() {
            if immunized.contains(&i) {
                continue;
            }
            if let Some(m) = self.singular_match(&tokens, i, sentence) {
                rule_matches.push(m);
            }
            if let Some(m) = self.plural_match(&tokens, i, sentence) {
                rule_matches.push(m);
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::LanguageEngine;

    fn shared_engine() -> std::sync::Arc<LanguageEngine> {
        crate::engine::LanguageEngine::shared_for_code("de").expect("german engine")
    }

    fn sva_matches(text: &str) -> Vec<(String, usize)> {
        shared_engine()
            .check(text)
            .matches
            .iter()
            .filter(|m| m.rule().id() == "DE_SUBJECT_VERB_AGREEMENT")
            .map(|m| (m.message().to_string(), m.offset()))
            .collect()
    }

    #[test]
    fn flags_plural_verb_after_singular_subject() {
        let matches = sva_matches("Drei Katzen ist im Haus.");
        assert!(
            matches
                .iter()
                .any(|(msg, _)| msg.contains("<suggestion>sind</suggestion>")),
            "got {matches:?}"
        );
    }

    #[test]
    fn flags_singular_verb_after_plural_subject() {
        let matches = sva_matches("Das Auto sind schnell.");
        assert!(
            matches
                .iter()
                .any(|(msg, _)| msg.contains("<suggestion>ist</suggestion>")),
            "got {matches:?}"
        );
    }

    #[test]
    fn accepts_correct_sentences() {
        for text in [
            "Die Autos sind schnell.",
            "Das Auto ist schnell.",
            "Um 18 Uhr ist Feierabend.",
            "Zehn Jahre ist es her.",
            "Das Kopieren und Einfügen ist sehr nützlich.",
            "Wie viel Prozent der Menschen sind total bescheuert?",
            "Obst und Gemüse ist gesund.",
        ] {
            assert!(
                sva_matches(text).is_empty(),
                "{text}: got {:?}",
                sva_matches(text)
            );
        }
    }
}
