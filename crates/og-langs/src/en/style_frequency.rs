use std::collections::HashMap;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule,
    rule::TextLevelRule,
};

/// Rust port of LanguageTool's AbstractStyleTooOftenUsedWordRule.
/// Text-level rule that detects words of a specific POS category used too frequently.
pub struct StyleFrequencyRule {
    id: &'static str,
    description: &'static str,
    pos_prefix: &'static str,
    min_percent: u32,
    min_word_count: usize,
    exception_pos: &'static [&'static str],
    exception_lemmas: &'static [&'static str],
}

impl StyleFrequencyRule {
    pub fn adjective() -> Self {
        Self {
            id: "TOO_OFTEN_USED_ADJECTIVE_EN",
            description: "Statistical Style: Overused Adjective",
            pos_prefix: "JJ",
            min_percent: 5,
            min_word_count: 100,
            exception_pos: &["RB", "IN", "CD", "DT", "NN"],
            exception_lemmas: &[],
        }
    }

    pub fn noun() -> Self {
        Self {
            id: "TOO_OFTEN_USED_NOUN_EN",
            description: "Statistical Style: Overused Noun",
            pos_prefix: "NN",
            min_percent: 5,
            min_word_count: 100,
            exception_pos: &["NNP", "IN", "JJ", "RB", "VB"],
            exception_lemmas: &[],
        }
    }

    pub fn verb() -> Self {
        Self {
            id: "TOO_OFTEN_USED_VERB_EN",
            description: "Statistical Style: Overused Verb",
            pos_prefix: "VB",
            min_percent: 5,
            min_word_count: 100,
            exception_pos: &["IN", "NN"],
            exception_lemmas: &["be", "have", "do"],
        }
    }

    fn is_target_word(&self, token: &og_core::AnalyzedTokenReadings) -> bool {
        token.readings().iter().any(|r| {
            r.pos_tags().iter().any(|t| t.starts_with(self.pos_prefix))
        })
    }

    fn is_exception(&self, token: &og_core::AnalyzedTokenReadings) -> bool {
        // Check exception POS tags
        for exc_pos in self.exception_pos {
            if token.has_pos_tag(exc_pos) {
                return true;
            }
        }
        // Check exception lemmas
        for reading in token.readings() {
            if let Some(lemma) = reading.lemma() {
                if self.exception_lemmas.contains(&lemma.to_lowercase().as_str()) {
                    return true;
                }
            }
        }
        false
    }

    fn get_lemma(&self, token: &og_core::AnalyzedTokenReadings) -> Option<String> {
        for reading in token.readings() {
            if reading.pos_tags().iter().any(|t| t.starts_with(self.pos_prefix)) {
                if let Some(lemma) = reading.lemma() {
                    return Some(lemma.to_string());
                }
            }
        }
        None
    }
}

impl TextLevelRule for StyleFrequencyRule {
    fn id(&self) -> &str {
        self.id
    }

    fn description(&self) -> &str {
        self.description
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("CREATIVE_WRITING", "Creative Writing")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        // Pass 1: count lemma frequencies
        let mut word_counts: HashMap<String, usize> = HashMap::new();
        for sentence in sentences {
            for token in sentence.non_whitespace_tokens() {
                if !self.is_target_word(token) || self.is_exception(token) {
                    continue;
                }
                if let Some(lemma) = self.get_lemma(token) {
                    *word_counts.entry(lemma).or_insert(0) += 1;
                }
            }
        }

        let total: usize = word_counts.values().sum();
        if total < self.min_word_count {
            return Vec::new();
        }

        // Find overused words
        let overused: HashMap<String, u32> = word_counts.iter()
            .filter_map(|(lemma, &count)| {
                let percent = (count as u32 * 100) / total as u32;
                if percent >= self.min_percent {
                    Some((lemma.clone(), percent))
                } else {
                    None
                }
            })
            .collect();

        if overused.is_empty() {
            return Vec::new();
        }

        // Pass 2: create matches
        let mut matches = Vec::new();
        let mut pos = 0usize;

        for sentence in sentences {
            for token in sentence.non_whitespace_tokens() {
                if !self.is_target_word(token) || self.is_exception(token) {
                    continue;
                }
                if let Some(lemma) = self.get_lemma(token) {
                    if let Some(&percent) = overused.get(&lemma) {
                        let from_pos = pos + token.token().start();
                        let to_pos = pos + token.token().end();
                        let length = to_pos - from_pos;

                        let message = format!(
                            "'{}' is used quite often ({}% of all {}). Consider using a synonym.",
                            lemma, percent, self.pos_prefix
                        );

                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_category(self.category());

                        let ctx_start = if from_pos >= 40 { from_pos - 40 } else { 0 };
                        let ctx_end = std::cmp::min(to_pos + 40, pos + text.len());
                        let context_text = text[ctx_start..ctx_end].to_string();

                        matches.push(
                            RuleMatch::new(&message, from_pos, length, rm_rule,
                                RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start))
                                .with_sentence(sentence.text().to_string())
                        );
                    }
                }
            }
            pos += sentence.text().len();
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings, AnalyzedSentence};

    fn make_sentence_with_pos(text: &str, pos_tags: &[&str]) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut byte_pos = 0;
        let words: Vec<&str> = text.split_whitespace().collect();
        for (idx, word) in words.iter().enumerate() {
            if let Some(offset) = text[byte_pos..].find(word) {
                let start = byte_pos + offset;
                let end = start + word.len();
                let pos_tag = pos_tags.get(idx).unwrap_or(&"NN");
                let reading = AnalyzedToken::new(*word, start, end)
                    .with_lemma((*word).to_string())
                    .with_pos_tags(vec![pos_tag.to_string()]);
                let atr = AnalyzedTokenReadings::new(reading);
                tokens.push(atr);
                byte_pos = end;
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn test_style_adjective_no_match_short_text() {
        let rule = StyleFrequencyRule::adjective();
        let s = make_sentence_with_pos("big big big", &["JJ", "JJ", "JJ"]);
        let matches = rule.match_text("big big big", &[s]);
        assert!(matches.is_empty(), "Too few words to trigger");
    }

    #[test]
    fn test_style_noun_no_match() {
        let rule = StyleFrequencyRule::noun();
        let s = make_sentence_with_pos("the dog runs", &["DT", "NN", "VBZ"]);
        let matches = rule.match_text("the dog runs", &[s]);
        assert!(matches.is_empty());
    }
}
