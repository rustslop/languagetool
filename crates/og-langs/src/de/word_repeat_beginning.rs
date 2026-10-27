use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    rule::TextLevelRule,
};
use std::sync::LazyLock;

const ADVERBS: &[&str] = &[
    "Auch",
    "Anschließend",
    "Außerdem",
    "Danach",
    "Ferner",
    "Nebenher",
    "Nebenbei",
    "Überdies",
    "Weiterführend",
    "Zudem",
    "Zusätzlich",
];

static SENTENCE_END_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^.+[.?!]$").unwrap());

const MSG_ADV: &str = "Zwei aufeinanderfolgende Sätze beginnen mit dem gleichen Adverb.";
const MSG_WORD: &str = "Drei aufeinanderfolgende Sätze beginnen mit dem gleichen Wort.";
const MSG_THESAURUS: &str =
    "Evtl. können Sie den Satz umformulieren, zum Beispiel, indem Sie ein Synonym nutzen.";

fn is_exception(token: &str) -> bool {
    matches!(token, ":" | "–" | "-" | "✔️" | "➡️" | "—" | "⭐️" | "⚠️")
}

fn is_adverb(token: &str) -> bool {
    ADVERBS.contains(&token)
}

pub struct GermanWordRepeatBeginningRule {
    category: Category,
}

impl GermanWordRepeatBeginningRule {
    pub fn new() -> Self {
        Self {
            category: Category::new("REPETITIONS_STYLE", "Wiederholungen (Stil)"),
        }
    }
}

impl Default for GermanWordRepeatBeginningRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for GermanWordRepeatBeginningRule {
    fn id(&self) -> &str {
        "GERMAN_WORD_REPEAT_BEGINNING_RULE"
    }

    fn description(&self) -> &str {
        "Aufeinanderfolgende Sätze beginnen mit dem gleichen Wort"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let mut last_token = "";
        let mut before_last_token = "";
        let mut prev_sentence: Option<&AnalyzedSentence> = None;
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            let mut token = "";
            if tokens.len() > 1 {
                let analyzed_token = tokens[1];
                token = analyzed_token.token().token();
                if tokens.len() > 3 {
                    let is_word = token.chars().count() != 1
                        || token.chars().next().is_some_and(char::is_alphabetic);
                    let prev_ends_sentence =
                        prev_sentence.is_some_and(|p| SENTENCE_END_REGEX.is_match(p.text().trim()));
                    if is_word
                        && last_token == token
                        && !is_exception(token)
                        && !is_exception(tokens[2].token().token())
                        && !is_exception(tokens[3].token().token())
                        && prev_ends_sentence
                    {
                        let short_msg = if is_adverb(analyzed_token.token().token()) {
                            MSG_ADV
                        } else if before_last_token == token {
                            MSG_WORD
                        } else {
                            ""
                        };
                        if !short_msg.is_empty() {
                            let start = analyzed_token.token().start();
                            let end = start + token.len();
                            let rel_start = start - sentence.start();
                            let rel_end = end - sentence.start();
                            let context_start = rel_start.saturating_sub(30);
                            let context_end = std::cmp::min(rel_end + 30, sentence.text().len());
                            let context_text =
                                sentence.text()[context_start..context_end].to_string();
                            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                                .with_category(self.category.clone())
                                .with_issue_type(IssueType::Style.as_str());
                            rule_matches.push(
                                RuleMatch::new(
                                    format!("{short_msg} {MSG_THESAURUS}"),
                                    start,
                                    end - start,
                                    rm_rule,
                                    RuleMatchContext::new(
                                        context_text,
                                        context_start,
                                        context_end - context_start,
                                    ),
                                )
                                .with_sentence(sentence.text().to_string()),
                            );
                        }
                    }
                }
            }
            before_last_token = last_token;
            last_token = token;
            prev_sentence = Some(sentence);
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn detects_repeated_sentence_beginning() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let text = "Dann hatten wir Freizeit. Dann gab es Essen. Dann gingen wir schlafen.";
        let mut request = og_core::CheckRequest::new(text, engine.language().clone());
        request.enabled_rules = vec!["GERMAN_WORD_REPEAT_BEGINNING_RULE".to_string()];
        let matches = engine.check_with_request(&request);
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "GERMAN_WORD_REPEAT_BEGINNING_RULE"),
            "expected word repeat beginning match, got {:?}",
            matches
                .matches
                .iter()
                .map(|m| (m.rule().id().to_string(), m.offset(), m.length()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn adverb_pair_is_flagged() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let text = "Außerdem ist er krank. Außerdem mag er sie.";
        let mut request = og_core::CheckRequest::new(text, engine.language().clone());
        request.enabled_rules = vec!["GERMAN_WORD_REPEAT_BEGINNING_RULE".to_string()];
        let matches = engine.check_with_request(&request);
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "GERMAN_WORD_REPEAT_BEGINNING_RULE"),
            "expected adverb repeat match, got {:?}",
            matches
                .matches
                .iter()
                .map(|m| (m.rule().id().to_string(), m.offset(), m.length()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn metadata_pins_the_registration() {
        use og_core::rule::TextLevelRule as _;

        let rule = super::GermanWordRepeatBeginningRule::default();
        assert_eq!(rule.id(), "GERMAN_WORD_REPEAT_BEGINNING_RULE");
        assert_eq!(
            rule.description(),
            "Aufeinanderfolgende S\u{e4}tze beginnen mit dem gleichen Wort"
        );
        assert_eq!(rule.category().id().as_str(), "REPETITIONS_STYLE");
        assert_eq!(rule.category().name(), "Wiederholungen (Stil)");
        assert!(matches!(rule.issue_type(), og_core::IssueType::Style));
    }

    #[test]
    fn single_token_sentences_are_skipped() {
        use og_core::rule::TextLevelRule as _;
        use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

        let mut sentence = AnalyzedSentence::new("Ja.", 0, 3);
        sentence.set_tokens(vec![AnalyzedTokenReadings::new(AnalyzedToken::new(
            "<S>", 0, 0,
        ))]);
        let rule = super::GermanWordRepeatBeginningRule::new();
        assert!(rule.match_text("Ja.", &[sentence]).is_empty());
    }
}
