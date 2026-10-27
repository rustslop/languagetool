use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};
use std::collections::HashSet;

pub struct AvsAnRule {
    words_requiring_a: HashSet<String>,
    words_requiring_a_case_sensitive: HashSet<String>,
    words_requiring_an: HashSet<String>,
    words_requiring_an_case_sensitive: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Determiner {
    A,
    An,
    AOrAn,
    Unknown,
}

impl AvsAnRule {
    pub fn new() -> Self {
        Self {
            words_requiring_a: HashSet::new(),
            words_requiring_a_case_sensitive: HashSet::new(),
            words_requiring_an: HashSet::new(),
            words_requiring_an_case_sensitive: HashSet::new(),
        }
    }

    pub fn load_data(&mut self, det_a_data: &str, det_an_data: &str) {
        for line in det_a_data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(word) = line.strip_prefix('*') {
                self.words_requiring_a_case_sensitive
                    .insert(word.to_string());
            } else {
                self.words_requiring_a.insert(line.to_lowercase());
            }
        }

        for line in det_an_data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(word) = line.strip_prefix('*') {
                self.words_requiring_an_case_sensitive
                    .insert(word.to_string());
            } else {
                self.words_requiring_an.insert(line.to_lowercase());
            }
        }
    }

    fn get_correct_determiner(&self, word: &str) -> Determiner {
        let clean = word
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '\'' || *c == '-')
            .collect::<String>();

        if clean.is_empty() {
            return Determiner::Unknown;
        }

        let first_part = clean.split('-').next().unwrap_or(&clean);
        let lower = first_part.to_lowercase();

        let requires_a = self.words_requiring_a.contains(&lower)
            || self
                .words_requiring_a_case_sensitive
                .iter()
                .any(|w| first_part == *w);
        let requires_an = self.words_requiring_an.contains(&lower)
            || self
                .words_requiring_an_case_sensitive
                .iter()
                .any(|w| first_part == *w);

        if requires_a && requires_an {
            return Determiner::AOrAn;
        }
        if requires_a {
            return Determiner::A;
        }
        if requires_an {
            return Determiner::An;
        }

        let first_char = match first_part.chars().next() {
            Some(c) => c,
            None => return Determiner::Unknown,
        };

        if first_part
            .chars()
            .all(|c| c.is_ascii_uppercase() || !c.is_alphabetic())
            && first_part.len() > 1
        {
            return Determiner::Unknown;
        }

        if lower.starts_with("unidentif") || lower.starts_with("unin") || lower.starts_with("unim")
        {
            return Determiner::An;
        }

        let is_vowel = matches!(first_char.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u');

        if is_vowel {
            let exception_prefixes = ["eu", "one", "uni", "ur", "us", "ut"];
            for prefix in &exception_prefixes {
                if lower.starts_with(prefix) {
                    return Determiner::A;
                }
            }
            Determiner::An
        } else {
            Determiner::A
        }
    }
}

impl Rule for AvsAnRule {
    fn id(&self) -> &str {
        "EN_A_VS_AN"
    }

    fn description(&self) -> &str {
        "Use of 'a' vs 'an'"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();

        let mut i = 0;
        while i < tokens.len() {
            let token_text = tokens[i].token().token();
            let lower = token_text.to_lowercase();

            let actual_det = if lower == "a" || lower == "an" {
                if lower == "a" {
                    Determiner::A
                } else {
                    Determiner::An
                }
            } else {
                i += 1;
                continue;
            };

            let mut j = i + 1;
            while j < tokens.len() {
                let next_text = tokens[j].token().token();
                if next_text == "'"
                    || next_text == "\""
                    || next_text == "`"
                    || next_text == "\u{2018}"
                    || next_text == "\u{2019}"
                    || next_text == "\u{201C}"
                    || next_text == "\u{201D}"
                    || next_text == "("
                    || next_text == "["
                {
                    j += 1;
                    continue;
                }
                break;
            }

            if j >= tokens.len() {
                i += 1;
                continue;
            }

            let next_word = tokens[j].token().token();
            let expected_det = self.get_correct_determiner(next_word);

            let should_flag = matches!(
                (actual_det, expected_det),
                (Determiner::A, Determiner::An) | (Determiner::An, Determiner::A)
            );

            if should_flag {
                let det_start = tokens[i].token().start();
                let det_end = tokens[i].token().end();
                let det_len = det_end - det_start;

                let suggestion = if actual_det == Determiner::A {
                    "an"
                } else {
                    "a"
                };

                let rm_rule = RuleMatchRule::new("EN_A_VS_AN", "Use of 'a' vs 'an'")
                    .with_category(Category::new("MISC", "Miscellaneous"))
                    .with_issue_type(self.issue_type().as_str());

                let context_start = det_start.saturating_sub(40);
                let context_end = std::cmp::min(det_end + 40, sentence.text().len());
                let context_text = sentence.text()[context_start..context_end].to_string();

                matches.push(
                    RuleMatch::new(
                        format!("Use '{}' instead of '{}'.", suggestion, token_text),
                        det_start,
                        det_len,
                        rm_rule,
                        RuleMatchContext::new(
                            context_text,
                            context_start,
                            context_end - context_start,
                        ),
                    )
                    .with_replacements(vec![SuggestedReplacement::new(suggestion)])
                    .with_sentence(sentence.text().to_string()),
                );
            }

            i += 1;
        }

        matches
    }
}

impl Default for AvsAnRule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut pos = 0;
        let chars: Vec<char> = text.chars().collect();
        while pos < chars.len() {
            while pos < chars.len() && chars[pos].is_whitespace() {
                pos += 1;
            }
            if pos >= chars.len() {
                break;
            }
            let start_byte = text[..text
                .char_indices()
                .nth(pos)
                .map(|(i, _)| i)
                .unwrap_or(text.len())]
                .len();
            if chars[pos].is_alphanumeric() || chars[pos] == '\'' {
                let word_start = pos;
                while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '\'') {
                    pos += 1;
                }
                let word: String = chars[word_start..pos].iter().collect();
                let end_byte = text[..text
                    .char_indices()
                    .nth(pos)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len())]
                    .len();
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    &word, start_byte, end_byte,
                )));
            } else {
                let ch = chars[pos].to_string();
                pos += 1;
                let end_byte = text[..text
                    .char_indices()
                    .nth(pos)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len())]
                    .len();
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    &ch, start_byte, end_byte,
                )));
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    fn make_rule() -> AvsAnRule {
        let mut rule = AvsAnRule::new();
        let det_a = "euphemism\neuropean\nunicorn\none-way\n";
        let det_an = "hour\nheir\nhonest\n8\n";
        rule.load_data(det_a, det_an);
        rule
    }

    #[test]
    fn test_a_before_vowel() {
        let rule = make_rule();
        let sentence = make_sentence("a elephant");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].message().contains("'an'"));
    }

    #[test]
    fn test_an_before_consonant() {
        let rule = make_rule();
        let sentence = make_sentence("an book");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].message().contains("'a'"));
    }

    #[test]
    fn test_correct_a_before_consonant() {
        let rule = make_rule();
        let sentence = make_sentence("a book");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_correct_an_before_vowel() {
        let rule = make_rule();
        let sentence = make_sentence("an elephant");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_an_before_exception_word() {
        let rule = make_rule();
        let sentence = make_sentence("an euphemism");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_a_before_an_word() {
        let rule = make_rule();
        let sentence = make_sentence("a hour");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_a_before_number() {
        let rule = make_rule();
        let sentence = make_sentence("a 8 ball");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_correct_usage() {
        let rule = make_rule();
        let sentence = make_sentence("This is a test of an idea.");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }
}
