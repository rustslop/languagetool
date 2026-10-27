use og_core::rule::TextLevelRule;
use og_core::{AnalyzedSentence, Category, IssueType, RuleMatch};

fn remote_rule_server_configured() -> bool {
    std::env::var("CA_REMOTE_RULE_SERVER")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

pub struct CatalanRemoteRule;

impl CatalanRemoteRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CatalanRemoteRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for CatalanRemoteRule {
    fn id(&self) -> &str {
        "CA_REMOTE_RULE"
    }

    fn description(&self) -> &str {
        "Recomanaci\u{f3} del model d'aprenentatge autom\u{e0}tic."
    }

    fn is_default_on(&self) -> bool {
        remote_rule_server_configured()
    }

    fn category(&self) -> Category {
        Category::new("PUNCTUATION", "Puntuaci\u{f3}")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }

    fn match_text(&self, _text: &str, _sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        Vec::new()
    }
}

pub struct CatalanSplitLongSentenceRule {
    max_words: usize,
}

impl CatalanSplitLongSentenceRule {
    pub fn new(max_words: usize) -> Self {
        Self { max_words }
    }

    pub fn max_words(&self) -> usize {
        self.max_words
    }
}

impl TextLevelRule for CatalanSplitLongSentenceRule {
    fn id(&self) -> &str {
        "CA_SPLIT_LONG_SENTENCE"
    }

    fn description(&self) -> &str {
        "Llegibilitat: frase amb m\u{e9}s de 60 paraules"
    }

    fn category(&self) -> Category {
        Category::new("STYLE", "Estil")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, _sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ca::example_suite::CatalanPipeline;

    #[test]
    fn remote_rule_produces_no_matches_offline() {
        let rule = CatalanRemoteRule::new();
        let pipeline = CatalanPipeline::shared();
        let sentences: Vec<AnalyzedSentence> = pipeline
            .analyze_offset_text("Aix\u{f2} \u{e9}s una frase. I aquesta \u{e9}s una altra.")
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        assert!(rule.match_text("", &sentences).is_empty());
    }

    #[test]
    fn split_long_sentence_emits_nothing_without_the_remote_rewriter() {
        let rule = CatalanSplitLongSentenceRule::new(60);
        let text = format!("{}paraula.", "paraula ".repeat(70));
        let pipeline = CatalanPipeline::shared();
        let sentences: Vec<AnalyzedSentence> = pipeline
            .analyze_offset_text(&text)
            .into_iter()
            .map(|(s, _)| s)
            .collect();
        assert_eq!(sentences.len(), 1, "control setup: one long sentence");
        let base = og_rules::text_level_rules::LongSentenceRule::new(60);
        let base_text: String = sentences
            .iter()
            .map(|s| s.text())
            .collect::<Vec<_>>()
            .concat();
        assert!(
            !base.match_text(&base_text, &sentences).is_empty(),
            "control: the 70-word sentence must trip the base rule"
        );
        assert!(rule.match_text(&text, &sentences).is_empty());
    }
}
