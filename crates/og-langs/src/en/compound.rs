use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement,
    rule::Rule,
};

enum SuggestionMode {
    /// No hyphen, suggest joined form only
    JoinOnly,
    /// Hyphenated only
    HyphenOnly,
    /// Suggest both hyphenated and joined
    Both,
}

struct CompoundEntry {
    parts: Vec<String>,
    hyphenated: String,
    joined: String,
    mode: SuggestionMode,
}

/// Rust port of LanguageTool's CompoundRule (EN_COMPOUNDS).
/// Checks that compound words are not written as separate words.
pub struct CompoundRule {
    entries: Vec<CompoundEntry>,
    max_parts: usize,
}

impl CompoundRule {
    pub fn new() -> Self {
        let mut rule = Self {
            entries: Vec::new(),
            max_parts: 0,
        };
        rule.load_data(include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/compounds.txt"));
        rule
    }

    fn load_data(&mut self, data: &str) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let (word_part, mode) = if line.ends_with('+') {
                (&line[..line.len() - 1], SuggestionMode::JoinOnly)
            } else if line.ends_with('*') {
                (&line[..line.len() - 1], SuggestionMode::HyphenOnly)
            } else if line.ends_with('?') {
                (&line[..line.len() - 1], SuggestionMode::JoinOnly)
            } else if line.ends_with('$') {
                (&line[..line.len() - 1], SuggestionMode::Both)
            } else {
                (line, SuggestionMode::Both)
            };

            let parts: Vec<String> = word_part.split('-').map(String::from).collect();
            if parts.len() < 2 {
                continue;
            }

            let hyphenated = word_part.to_string();
            let joined = parts.join("");

            let part_count = parts.len();
            if part_count > self.max_parts {
                self.max_parts = part_count;
            }

            self.entries.push(CompoundEntry {
                parts,
                hyphenated,
                joined,
                mode,
            });
        }
    }
}

impl Default for CompoundRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for CompoundRule {
    fn id(&self) -> &str {
        "EN_COMPOUNDS"
    }

    fn description(&self) -> &str {
        "Hyphenated words"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("COMPOUNDING", "Compounding")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        let n = tokens.len();
        let text = sentence.text();

        for i in 0..n {
            for entry in &self.entries {
                let part_count = entry.parts.len();
                if i + part_count > n {
                    continue;
                }

                let mut matched = true;
                for (j, part) in entry.parts.iter().enumerate() {
                    let token_text = tokens[i + j].token().token();
                    if token_text.to_lowercase() != part.to_lowercase() {
                        matched = false;
                        break;
                    }
                }

                if !matched {
                    continue;
                }

                let match_start = tokens[i].token().start();
                let match_end = tokens[i + part_count - 1].token().end();
                let match_length = match_end - match_start;

                let matched_text = &text[match_start..match_end];

                let suggestions: Vec<SuggestedReplacement> = match &entry.mode {
                    SuggestionMode::JoinOnly => {
                        vec![SuggestedReplacement::new(preserve_case_multi(matched_text, &entry.joined))]
                    }
                    SuggestionMode::HyphenOnly => {
                        vec![SuggestedReplacement::new(preserve_case_multi(matched_text, &entry.hyphenated))]
                    }
                    SuggestionMode::Both => {
                        let joined = preserve_case_multi(matched_text, &entry.joined);
                        let hyphenated = preserve_case_multi(matched_text, &entry.hyphenated);
                        let mut s = vec![SuggestedReplacement::new(&hyphenated)];
                        if joined != hyphenated {
                            s.push(SuggestedReplacement::new(&joined));
                        }
                        s
                    }
                };

                let msg = if entry.hyphenated.contains('-') {
                    "This word is normally spelled with a hyphen."
                } else {
                    "This word is normally spelled as one."
                };

                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_urls(vec!["https://languagetool.org/insights/post/hyphen/".to_string()]);

                let ctx_start = if match_start >= 40 { match_start - 40 } else { 0 };
                let ctx_end = std::cmp::min(match_end + 40, text.len());
                let context_text = text[ctx_start..ctx_end].to_string();

                matches.push(
                    RuleMatch::new(msg, match_start, match_length, rm_rule,
                        RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start))
                        .with_replacements(suggestions)
                        .with_sentence(text.to_string())
                );
                break; // one match per start position
            }
        }

        matches
    }
}

fn preserve_case_multi(original: &str, replacement: &str) -> String {
    let is_all_upper = original.chars().all(|c| !c.is_alphabetic() || c.is_uppercase());
    let is_title = original.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        && original.chars().skip(1).all(|c| !c.is_alphabetic() || c.is_lowercase());

    if is_all_upper {
        replacement.to_uppercase()
    } else if is_title {
        let mut r = replacement.chars();
        match r.next() {
            Some(first) => format!("{}{}", first.to_uppercase(), r.as_str()),
            None => replacement.to_string(),
        }
    } else {
        replacement.to_string()
    }
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
    fn test_compound_part_time() {
        let rule = CompoundRule::new();
        let sentence = make_sentence("I have a part time job");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Should detect 'part time' as compound");
        let repl: Vec<&str> = matches[0].replacements().iter().map(|r| r.value()).collect();
        assert!(repl.iter().any(|r| r.contains(&"part-time")), "Should suggest 'part-time', got {:?}", repl);
    }

    #[test]
    fn test_compound_already_hyphenated() {
        let rule = CompoundRule::new();
        let sentence = make_sentence("I have a part-time job");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "Already hyphenated should not trigger");
    }

    #[test]
    fn test_compound_no_match() {
        let rule = CompoundRule::new();
        let sentence = make_sentence("the quick brown fox");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }
}
