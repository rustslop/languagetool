use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    rule::TextLevelRule,
};
use std::collections::HashMap;

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
        token
            .readings()
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t.starts_with(self.pos_prefix)))
    }

    fn is_exception(&self, token: &og_core::AnalyzedTokenReadings) -> bool {
        for exc_pos in self.exception_pos {
            if token.has_pos_tag(exc_pos) {
                return true;
            }
        }
        for reading in token.readings() {
            if let Some(lemma) = reading.lemma()
                && self
                    .exception_lemmas
                    .contains(&lemma.to_lowercase().as_str())
            {
                return true;
            }
        }
        false
    }

    fn get_lemma(&self, token: &og_core::AnalyzedTokenReadings) -> Option<String> {
        for reading in token.readings() {
            if reading
                .pos_tags()
                .iter()
                .any(|t| t.starts_with(self.pos_prefix))
                && let Some(lemma) = reading.lemma()
            {
                return Some(lemma.to_string());
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

        let overused: HashMap<String, u32> = word_counts
            .iter()
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

        let mut matches = Vec::new();
        let mut pos = 0usize;

        for sentence in sentences {
            for token in sentence.non_whitespace_tokens() {
                if !self.is_target_word(token) || self.is_exception(token) {
                    continue;
                }
                if let Some(lemma) = self.get_lemma(token)
                    && let Some(&percent) = overused.get(&lemma)
                {
                    let from_pos = pos + token.token().start();
                    let to_pos = pos + token.token().end();
                    let length = to_pos - from_pos;

                    let message = format!(
                        "'{}' is used quite often ({}% of all {}). Consider using a synonym.",
                        lemma, percent, self.pos_prefix
                    );

                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str());

                    let ctx_start = (from_pos.saturating_sub(40)).max(pos);
                    let ctx_end = std::cmp::min(to_pos + 40, pos + sentence.text().len());
                    let context_text = text[ctx_start..ctx_end].to_string();

                    matches.push(
                        RuleMatch::new(
                            &message,
                            from_pos,
                            length,
                            rm_rule,
                            RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                        )
                        .with_sentence(sentence.text().to_string()),
                    );
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
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

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

    fn sentence_from_words(words: &[(&str, &str, &str)]) -> AnalyzedSentence {
        let parts: Vec<&str> = words.iter().map(|(w, _, _)| *w).collect();
        let text = parts.join(" ");
        let mut sentence = AnalyzedSentence::new(&text, 0, text.len());
        let mut tokens = Vec::new();
        let mut byte_pos = 0;
        for (word, lemma, pos_tags) in words {
            let start = byte_pos + text[byte_pos..].find(*word).expect("word order");
            let end = start + word.len();
            let readings: Vec<AnalyzedToken> = pos_tags
                .split('+')
                .map(|t| {
                    AnalyzedToken::new(*word, start, end)
                        .with_lemma((*lemma).to_string())
                        .with_pos_tags(vec![t.to_string()])
                })
                .collect();
            tokens.push(AnalyzedTokenReadings::new(readings[0].clone()).with_readings(readings));
            byte_pos = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn adjective_flags_each_overused_occurrence_over_the_minimum() {
        let rule = StyleFrequencyRule::adjective();
        let first: Vec<(&str, &str, &str)> = (0..50).map(|_| ("big", "big", "JJ")).collect();
        let mut second: Vec<(&str, &str, &str)> = (0..45).map(|_| ("big", "big", "JJ")).collect();
        second.extend((0..5).map(|_| ("small", "small", "JJ")));
        let s1 = sentence_from_words(&first);
        let s2 = sentence_from_words(&second);
        let text = format!("{}{}", s1.text(), s2.text());
        let matches = rule.match_text(&text, &[s1, s2]);
        assert_eq!(matches.len(), 100);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].length(), 3);
        assert_eq!(matches[49].offset(), 196);
        assert_eq!(matches[50].offset(), 199);
        let big = matches.iter().filter(|m| {
            m.message()
                .contains("'big' is used quite often (95% of all JJ)")
        });
        assert_eq!(big.count(), 95);
        let small = matches.iter().filter(|m| {
            m.message()
                .contains("'small' is used quite often (5% of all JJ)")
        });
        assert_eq!(small.count(), 5);
    }

    #[test]
    fn adjective_skips_words_with_exception_readings() {
        let rule = StyleFrequencyRule::adjective();
        let mut words: Vec<(&str, &str, &str)> = (0..60).map(|_| ("big", "big", "JJ")).collect();
        words.extend((0..40).map(|_| ("fast", "fast", "JJ+RB")));
        let sentence = sentence_from_words(&words);
        let text = sentence.text().to_string();
        let matches = rule.match_text(&text, &[sentence]);
        assert!(matches.is_empty());
    }

    #[test]
    fn adjective_ignores_lemmas_below_the_percent_threshold() {
        let rule = StyleFrequencyRule::adjective();
        let mut words: Vec<(&str, &str, &str)> = (0..96).map(|_| ("big", "big", "JJ")).collect();
        words.extend((0..2).map(|_| ("red", "red", "JJ")));
        words.extend((0..2).map(|_| ("blue", "blue", "JJ")));
        let sentence = sentence_from_words(&words);
        let text = sentence.text().to_string();
        let matches = rule.match_text(&text, &[sentence]);
        assert_eq!(matches.len(), 96);
        assert!(matches.iter().all(|m| m.message().contains("'big'")));
    }

    #[test]
    fn verb_rule_skips_be_have_do_lemmas() {
        let rule = StyleFrequencyRule::verb();
        let mut words: Vec<(&str, &str, &str)> = (0..50).map(|_| ("run", "run", "VB")).collect();
        words.extend((0..50).map(|_| ("was", "be", "VB")));
        let sentence = sentence_from_words(&words);
        let text = sentence.text().to_string();
        assert!(rule.match_text(&text, &[sentence]).is_empty());

        let only: Vec<(&str, &str, &str)> = (0..100).map(|_| ("run", "run", "VB")).collect();
        let sentence = sentence_from_words(&only);
        let text = sentence.text().to_string();
        let matches = rule.match_text(&text, &[sentence]);
        assert_eq!(matches.len(), 100);
        assert!(matches[0].message().contains("100% of all VB"));
    }

    #[test]
    fn metadata_pins_creative_writing_style_and_default_off() {
        let rule = StyleFrequencyRule::adjective();
        assert_eq!(rule.id(), "TOO_OFTEN_USED_ADJECTIVE_EN");
        assert_eq!(rule.description(), "Statistical Style: Overused Adjective");
        assert!(!rule.is_default_on());
        assert_eq!(rule.issue_type(), IssueType::Style);
        assert_eq!(rule.category().name(), "Creative Writing");
        assert_eq!(StyleFrequencyRule::noun().id(), "TOO_OFTEN_USED_NOUN_EN");
        assert_eq!(StyleFrequencyRule::verb().id(), "TOO_OFTEN_USED_VERB_EN");
    }
}
