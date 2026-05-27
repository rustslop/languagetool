use regex::Regex;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement,
    rule::Rule,
};

struct ContextWordEntry {
    word1_re: Regex,
    word2_re: Regex,
    match1: String,
    match2: String,
    context1_re: Regex,
    context2_re: Regex,
    explanation1: String,
    explanation2: String,
    name1: String,
    name2: String,
}

/// Rust port of LanguageTool's EnglishWrongWordInContextRule.
/// Detects commonly confused words using context clues.
pub struct WrongWordInContextRule {
    entries: Vec<ContextWordEntry>,
}

impl WrongWordInContextRule {
    pub fn new() -> Self {
        let entries = Self::load_data(include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/wrongWordInContext.txt"));
        Self { entries }
    }

    fn load_data(data: &str) -> Vec<ContextWordEntry> {
        let mut entries = Vec::new();
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 6 {
                continue;
            }
            let word1_re = match Regex::new(&format!("(?i)\\b{}\\b", parts[0])) {
                Ok(re) => re,
                Err(_) => continue,
            };
            let word2_re = match Regex::new(&format!("(?i)\\b{}\\b", parts[1])) {
                Ok(re) => re,
                Err(_) => continue,
            };
            let context1_re = match Regex::new(&format!("(?i)\\b({})\\b", parts[4])) {
                Ok(re) => re,
                Err(_) => continue,
            };
            let context2_re = match Regex::new(&format!("(?i)\\b({})\\b", parts[5])) {
                Ok(re) => re,
                Err(_) => continue,
            };
            let expl1 = parts.get(6).map(|s| s.to_string()).unwrap_or_default();
            let expl2 = parts.get(7).map(|s| s.to_string()).unwrap_or_default();

            entries.push(ContextWordEntry {
                name1: parts[0].to_string(),
                name2: parts[1].to_string(),
                word1_re,
                word2_re,
                match1: parts[2].to_string(),
                match2: parts[3].to_string(),
                context1_re,
                context2_re,
                explanation1: expl1,
                explanation2: expl2,
            });
        }
        entries
    }

    fn preserve_case(original: &str, replacement: &str) -> String {
        if original.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
            replacement.to_uppercase()
        } else if original.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            let mut r = replacement.chars();
            match r.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), r.as_str()),
                None => replacement.to_string(),
            }
        } else {
            replacement.to_string()
        }
    }
}

impl Default for WrongWordInContextRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for WrongWordInContextRule {
    fn id(&self) -> &str {
        "ENGLISH_WRONG_WORD_IN_CONTEXT"
    }

    fn description(&self) -> &str {
        "Commonly confused words"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("CONFUSED_WORDS", "Confused Words")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("confused_word".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let text = sentence.text();
        let text_lower = text.to_lowercase();
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();

        for entry in &self.entries {
            // Quick pre-filter: check if either word's prefix is in the text
            if !text_lower.contains(&entry.match1.to_lowercase()) && !text_lower.contains(&entry.match2.to_lowercase()) {
                continue;
            }

            let w1_match = word_match_positions(&entry.word1_re, &tokens);
            let w2_match = word_match_positions(&entry.word2_re, &tokens);

            // Need exactly one word present
            if w1_match.is_empty() && w2_match.is_empty() { continue; }
            if !w1_match.is_empty() && !w2_match.is_empty() { continue; }

            let (found_word, found_idx, not_found_match, found_context, not_found_context, found_expl, not_found_expl, _found_name, not_found_name) =
                if !w1_match.is_empty() {
                    let (idx, _) = w1_match[0];
                    (tokens[idx].token().token(), idx, &entry.match2, &entry.context1_re, &entry.context2_re, &entry.explanation1, &entry.explanation2, &entry.name1, &entry.name2)
                } else {
                    let (idx, _) = w2_match[0];
                    (tokens[idx].token().token(), idx, &entry.match1, &entry.context2_re, &entry.context1_re, &entry.explanation2, &entry.explanation1, &entry.name2, &entry.name1)
                };

            // Check context: not_found's context should be present, found's context should NOT
            let sentence_text = text.to_string();
            let has_not_found_context = not_found_context.is_match(&sentence_text);
            let has_found_context = found_context.is_match(&sentence_text);

            if has_not_found_context && !has_found_context {
                // Create suggestion by replacing the wrong part
                let replacement = found_word.replace(&entry.match1[..], not_found_match);
                let replacement = if replacement == found_word {
                    // Try the other match string
                    let lowered = found_word.to_lowercase();
                    lowered.replace(&entry.match2.to_lowercase()[..], &not_found_match.to_lowercase())
                } else {
                    replacement
                };
                let replacement = if replacement == found_word || replacement.is_empty() {
                    Self::preserve_case(found_word, not_found_name)
                } else {
                    Self::preserve_case(found_word, &replacement)
                };

                let start = tokens[found_idx].token().start();
                let end = tokens[found_idx].token().end();

                let message = if found_expl.is_empty() {
                    format!("Possibly confused word: Did you mean '{}'?", replacement)
                } else {
                    format!("Possibly confused word: Did you mean '{}' (= {}) instead of '{}' (= {})?",
                        replacement, not_found_expl, found_word, found_expl)
                };

                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category());

                let ctx_start = if start >= 40 { start - 40 } else { 0 };
                let ctx_end = std::cmp::min(end + 40, text.len());
                let context_text = text[ctx_start..ctx_end].to_string();

                matches.push(
                    RuleMatch::new(&message, start, end - start, rm_rule,
                        RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start))
                        .with_replacements(vec![SuggestedReplacement::new(&replacement)])
                        .with_sentence(text.to_string())
                );
            }
        }

        matches
    }
}

fn word_match_positions<'a>(re: &Regex, tokens: &[&'a og_core::AnalyzedTokenReadings]) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if re.is_match(token.token().token()) {
            result.push((i, token.token().start()));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings, AnalyzedSentence};

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut byte_pos = 0;
        for part in text.split(|c: char| c.is_whitespace()) {
            if part.is_empty() { continue; }
            if let Some(offset) = text[byte_pos..].find(part) {
                let start = byte_pos + offset;
                let end = start + part.len();
                tokens.push(AnalyzedTokenReadings::new(
                    AnalyzedToken::new(part, start, end)
                ));
                byte_pos = end;
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn test_wrong_word_statue_statute() {
        let rule = WrongWordInContextRule::new();
        let sentence = make_sentence("The government passed a new statue");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Should detect 'statue' in government context");
    }

    #[test]
    fn test_wrong_word_no_match() {
        let rule = WrongWordInContextRule::new();
        let sentence = make_sentence("The quick brown fox jumps");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_wrong_word_correct_usage() {
        let rule = WrongWordInContextRule::new();
        let sentence = make_sentence("The beautiful statue is made of marble");
        let matches = rule.match_sentence(&sentence);
        // "statue" with "marble" context is correct, should not trigger
        assert!(matches.is_empty() || !matches.iter().any(|m| m.message().contains("statue")));
    }
}
