use og_core::rule::TextLevelRule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use regex::Regex;
use std::collections::{HashMap, HashSet};

pub struct ArabicWordCoherencyRule {
    word_map: HashMap<String, HashSet<String>>,
}

fn load_word_map(data: &str) -> HashMap<String, HashSet<String>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    for line in data.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() != 2 {
            continue;
        }
        map.entry(parts[0].to_string())
            .or_default()
            .insert(parts[1].to_string());
        map.entry(parts[1].to_string())
            .or_default()
            .insert(parts[0].to_string());
    }
    map
}

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_uppercase())
}

fn uppercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => s.to_string(),
    }
}

fn replace_first(marked: &str, token: &str, other_spelling: &str) -> String {
    match Regex::new(&format!("(?i){}", regex::escape(token))) {
        Ok(re) => re.replacen(marked, 1, other_spelling).into_owned(),
        Err(_) => marked.to_string(),
    }
}

impl ArabicWordCoherencyRule {
    pub fn new() -> Self {
        Self {
            word_map: load_word_map(include_str!(
                "../../../../languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/rules/ar/coherency.txt"
            )),
        }
    }

    fn message(word1: &str, word2: &str) -> String {
        format!("تجنب استعمال شكلين للكلمة نفسها ('{word1}' و '{word2}') في  النص نفسه.")
    }

    fn build_match(
        &self,
        sentence: &AnalyzedSentence,
        token_reading: &AnalyzedTokenReadings,
        token: &str,
        first_spelling: &str,
    ) -> Option<RuleMatch> {
        let from_pos = token_reading.token().start();
        let to_pos = token_reading.token().end();
        let text = sentence.text();
        let marked = &text[from_pos - sentence.start()..to_pos - sentence.start()];
        let mut replacement = replace_first(marked, token, first_spelling);
        if starts_with_uppercase(token_reading.token().token()) {
            replacement = uppercase_first(&replacement);
        }
        if marked.eq_ignore_ascii_case(&replacement) {
            return None;
        }
        let text_len = text.len();
        let mut context_start = (from_pos - sentence.start()).saturating_sub(40);
        while context_start > 0 && !text.is_char_boundary(context_start) {
            context_start -= 1;
        }
        let mut context_end = std::cmp::min(to_pos - sentence.start() + 40, text_len);
        while context_end < text_len && !text.is_char_boundary(context_end) {
            context_end += 1;
        }
        let context_text = text[context_start..context_end].to_string();
        let rm_rule = RuleMatchRule::new(self.id(), self.description())
            .with_category(self.category())
            .with_issue_type(self.issue_type().as_str());
        Some(
            RuleMatch::new(
                Self::message(token, first_spelling),
                from_pos,
                to_pos - from_pos,
                rm_rule,
                RuleMatchContext::new(context_text, context_start, context_end - context_start),
            )
            .with_replacements(vec![SuggestedReplacement::new(&replacement)])
            .with_sentence(text.to_string()),
        )
    }
}

impl Default for ArabicWordCoherencyRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for ArabicWordCoherencyRule {
    fn id(&self) -> &str {
        "AR_WORD_COHERENCY"
    }

    fn description(&self) -> &str {
        "ضبط انسجام التهجئة للكلمات التي تكتب بطرق مختلفة مقبولة."
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Inconsistency
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut should_not_appear: HashMap<&str, &str> = HashMap::new();

        for sentence in sentences {
            for token_reading in sentence.non_whitespace_tokens() {
                let readings = token_reading.readings();
                if readings.is_empty() {
                    continue;
                }
                let mut current: &str = token_reading.token().token();
                for reading in readings {
                    if let Some(lemma) = reading.lemma() {
                        current = lemma;
                    }
                    if let Some(first_spelling) = should_not_appear.get(current) {
                        if let Some(rule_match) =
                            self.build_match(sentence, token_reading, current, first_spelling)
                        {
                            matches.push(rule_match);
                        }
                        break;
                    }
                    if let Some(variants) = self.word_map.get(current) {
                        for variant in variants {
                            should_not_appear.insert(variant.as_str(), current);
                        }
                    }
                }
            }
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_bidirectional_variants() {
        let rule = ArabicWordCoherencyRule::new();
        assert!(rule.word_map.contains_key("شؤون"));
        assert!(rule.word_map["شؤون"].contains("شئون"));
        assert!(rule.word_map["شئون"].contains("شؤون"));
    }
}
