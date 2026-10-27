use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};
use std::sync::OnceLock;

const PREFERRED_WORDS: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl/preferredwords.csv"
);

struct PreferredEntry {
    words: Vec<&'static str>,
    old_word: &'static str,
    new_word: &'static str,
}

fn entries() -> &'static Vec<PreferredEntry> {
    static ENTRIES: OnceLock<Vec<PreferredEntry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        PREFERRED_WORDS
            .lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .filter_map(|line| {
                let (old_word, new_word) = line.split_once(';')?;
                if old_word.is_empty() || new_word.is_empty() {
                    return None;
                }
                Some(PreferredEntry {
                    words: old_word.split(' ').collect(),
                    old_word,
                    new_word,
                })
            })
            .collect()
    })
}

pub struct PreferredWordRule;

pub fn preferred_word_rule() -> PreferredWordRule {
    PreferredWordRule
}

impl Rule for PreferredWordRule {
    fn id(&self) -> &str {
        "NL_PREFERRED_WORD_RULE"
    }

    fn description(&self) -> &str {
        "Suggereert een gebruikelijker woord."
    }

    fn category(&self) -> Category {
        Category::new("STYLE", "Stijl")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let text = sentence.text();
        let mut matches = Vec::new();
        for entry in entries() {
            let count = entry.words.len();
            for start in 0..tokens.len().saturating_sub(count - 1) {
                let window = &tokens[start..start + count];
                if window
                    .iter()
                    .zip(entry.words.iter())
                    .any(|(token, word)| token.token().token() != *word)
                {
                    continue;
                }
                let from = window[0].token().start();
                let to = window[count - 1].token().end();
                let matched_text = &text[from..to];
                let suggestion = matched_text.replace(entry.old_word, entry.new_word);
                if suggestion == matched_text {
                    continue;
                }
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                let mut ctx_start = from.saturating_sub(40);
                while !text.is_char_boundary(ctx_start) {
                    ctx_start -= 1;
                }
                let mut ctx_end = (to + 40).min(text.len());
                while !text.is_char_boundary(ctx_end) {
                    ctx_end += 1;
                }
                matches.push(
                    RuleMatch::new(
                        "Voor dit woord is een gebruikelijker alternatief.",
                        from,
                        to - from,
                        rm_rule,
                        RuleMatchContext::new(
                            text[ctx_start..ctx_end].to_string(),
                            ctx_start,
                            ctx_end - ctx_start,
                        ),
                    )
                    .with_short_message("Gebruikelijker woord")
                    .with_sentence(text.to_string())
                    .with_replacements(vec![SuggestedReplacement::new(&suggestion)]),
                );
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nl::test_support::DutchPipeline;

    fn sentence(text: &str) -> AnalyzedSentence {
        let pipeline = DutchPipeline::shared();
        pipeline.analyze_range(text, 0, text.len())
    }

    #[test]
    fn java_pins_rijwiel() {
        let rule = preferred_word_rule();
        let matches = rule.match_sentence(&sentence("rijwiel"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements().len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "fiets");
    }

    #[test]
    fn embedded_example_pair_matches_the_marker() {
        let rule = preferred_word_rule();
        let matches = rule.match_sentence(&sentence("Hij vindt rijwiel een ouderwets woord."));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 10);
        assert_eq!(matches[0].length(), 7);
    }
}
