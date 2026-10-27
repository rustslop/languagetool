use og_core::rule::Rule;
use og_core::{AnalyzedSentence, RuleMatch};
use og_spell::Dictionary;
use og_tokenizer::WordTokenizer as _;
use std::collections::HashSet;
use std::sync::Arc;

pub const MORFOLOGIK_RULE_NL_NL: &str = "MORFOLOGIK_RULE_NL_NL";

pub fn shared_speller_dictionary() -> Option<Arc<Dictionary>> {
    static CACHE: og_core::releasable::Releasable<Dictionary> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let resource_dir = super::tagger::nl_resource_dir()?;
        let tagger_dict = og_tagger::generic_tagger::shared_dictionary(&resource_dir, "dutch")?;
        let mut dict = Dictionary::from_shared_words(Arc::new(
            og_spell::morfologik_view::MorfologikView::new(tagger_dict),
        ));
        if let Ok(words) = std::fs::read_to_string(resource_dir.join("spelling/spelling.txt")) {
            for line in words.lines() {
                let word = line.split(['\t', ';']).next().unwrap_or("").trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }
        Some(Arc::new(dict))
    })
}

fn shared_ignore_words() -> HashSet<String> {
    let mut ignore = HashSet::new();
    if let Some(resource_dir) = super::tagger::nl_resource_dir()
        && let Ok(words) = std::fs::read_to_string(resource_dir.join("spelling/ignore.txt"))
    {
        for line in words.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                ignore.insert(word.to_string());
            }
        }
    }
    ignore
}

pub struct DutchSpellerRule {
    inner: og_spell::SpellingCheckRule,
    acceptor: Option<Arc<super::compound_acceptor::CompoundAcceptor>>,
}

impl DutchSpellerRule {
    pub fn new() -> Self {
        let dictionary = shared_speller_dictionary().unwrap_or_else(|| Arc::new(Dictionary::new()));
        let inner = og_spell::SpellingCheckRule::new_shared(
            dictionary,
            MORFOLOGIK_RULE_NL_NL,
            HashSet::new(),
        )
        .with_ignore_words(shared_ignore_words());
        Self {
            inner,
            acceptor: Some(super::compound_acceptor::shared_compound_acceptor()),
        }
    }

    pub fn is_misspelled(&self, text: &str) -> bool {
        let tokenizer = og_tokenizer::DefaultWordTokenizer::new_dutch();
        tokenizer
            .tokenize(text)
            .iter()
            .any(|token| !self.inner.is_known_word(token.text()))
    }

    fn word_of_match<'a>(&self, sentence: &'a AnalyzedSentence, m: &RuleMatch) -> &'a str {
        let rel = m.offset().saturating_sub(sentence.start());
        &sentence.text()[rel..rel + m.length()]
    }

    fn match_is_droppable(&self, sentence: &AnalyzedSentence, m: &RuleMatch) -> bool {
        let end = m.offset() + m.length();
        let english_ignore = sentence.non_whitespace_tokens().into_iter().any(|t| {
            t.token().start() < end
                && t.token().end() > m.offset()
                && t.has_pos_tag("_english_ignore_")
        });
        if english_ignore {
            return true;
        }
        let word = self.word_of_match(sentence, m);
        self.acceptor
            .as_deref()
            .is_some_and(|acceptor| acceptor.accept_compound(word))
    }
}

impl Default for DutchSpellerRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DutchSpellerRule {
    fn id(&self) -> &str {
        MORFOLOGIK_RULE_NL_NL
    }

    fn description(&self) -> &str {
        "Mogelijke spelfout"
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        self.inner
            .match_sentence(sentence)
            .into_iter()
            .filter(|m| !self.match_is_droppable(sentence, m))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> DutchSpellerRule {
        DutchSpellerRule::new()
    }

    fn matches_for(text: &str) -> Vec<RuleMatch> {
        let pipeline = crate::nl::test_support::DutchPipeline::shared();
        let sentence = pipeline.analyze_range(text, 0, text.len());
        rule().match_sentence(&sentence)
    }

    #[test]
    fn java_pins_known_and_ignored_words() {
        assert_eq!(matches_for("Amsterdam").len(), 0);
        assert_eq!(matches_for("ipv").len(), 0);
        assert_eq!(matches_for("voorzover").len(), 0);
    }

    #[test]
    fn java_pins_misspellings() {
        assert_eq!(matches_for("FoobarWrongxx").len(), 1);
        assert_eq!(matches_for("foobarwrong").len(), 1);
        assert_eq!(matches_for("déur").len(), 1);
    }

    #[test]
    fn java_pins_tagger_driven_acceptance() {
        assert_eq!(matches_for("kómen").len(), 0);
        assert_eq!(matches_for("háár").len(), 0);
        assert_eq!(matches_for("kán").len(), 0);
        assert_eq!(matches_for("ín").len(), 0);
        assert_eq!(matches_for("géén").len(), 0);
        assert_eq!(matches_for("déúr").len(), 0);
        assert_eq!(matches_for("deur-knop").len(), 0);
    }

    #[test]
    fn java_pins_thailan_suggestion() {
        let matches = matches_for("thailan.");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0]
                .replacements()
                .first()
                .map(|r| r.value().to_string()),
            Some("Thailand".to_string())
        );
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].length(), 7);
    }

    #[test]
    fn accepted_compounds_are_not_flagged() {
        assert_eq!(matches_for("beoordelingsgeschiedenis").len(), 0);
        assert_eq!(matches_for("oorlogsbeker").len(), 0);
    }
}
