use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement,
    rule::Rule,
};

/// Rust port of LanguageTool's EnglishDashRule (EN_DASH_RULE).
/// Detects em-dashes and en-dashes used where hyphens should be
/// in compound words (e.g., "T—shirt" → "T-shirt").
pub struct DashRule {
    compounds: Vec<String>,
}

impl DashRule {
    pub fn new() -> Self {
        let compounds = Self::load_data(include_str!("../../../../../languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/compounds.txt"));
        Self { compounds }
    }

    fn load_data(data: &str) -> Vec<String> {
        let mut result = Vec::new();
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Strip suffix markers: +, *, ?, $
            let clean = line.trim_end_matches(|c| c == '+' || c == '*' || c == '?' || c == '$');
            if clean.contains('-') {
                result.push(clean.to_string());
            }
        }
        result
    }

    fn has_dash(s: &str) -> bool {
        s.contains('\u{2014}') || s.contains('\u{2013}') || s.contains('\u{2012}')
    }

    fn replace_dashes_with_hyphens(s: &str) -> String {
        s.replace('\u{2014}', "-")
         .replace('\u{2013}', "-")
         .replace('\u{2012}', "-")
    }
}

impl Default for DashRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DashRule {
    fn id(&self) -> &str {
        "EN_DASH_RULE"
    }

    fn description(&self) -> &str {
        "Checks if hyphenated words were spelled with dashes"
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
        let text = sentence.text();
        let mut matches = Vec::new();

        for token in &tokens {
            let token_text = token.token().token();
            if !Self::has_dash(token_text) {
                continue;
            }

            let normalized = Self::replace_dashes_with_hyphens(token_text);
            let normalized_lower = normalized.to_lowercase();

            for compound in &self.compounds {
                if compound.to_lowercase() == normalized_lower {
                    let match_start = token.token().start();
                    let match_length = token.token().end() - match_start;

                    let suggestion = if token_text.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                        let mut c = compound.chars();
                        match c.next() {
                            Some(first) => format!("{}{}", first.to_uppercase(), c.as_str()),
                            None => compound.clone(),
                        }
                    } else {
                        compound.clone()
                    };

                    if token_text == suggestion {
                        continue;
                    }

                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_urls(vec!["https://languagetool.org/insights/post/hyphen/".to_string()]);

                    let ctx_start = if match_start >= 40 { match_start - 40 } else { 0 };
                    let ctx_end = std::cmp::min(match_start + match_length + 40, text.len());
                    let context_text = text[ctx_start..ctx_end].to_string();

                    matches.push(
                        RuleMatch::new("A dash was used instead of a hyphen.", match_start, match_length, rm_rule,
                            RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start))
                            .with_replacements(vec![SuggestedReplacement::new(&suggestion)])
                            .with_sentence(text.to_string())
                    );
                    break;
                }
            }
        }

        matches
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
    fn test_dash_rule_em_dash() {
        let rule = DashRule::new();
        let sentence = make_sentence("I bought a T\u{2014}shirt");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Should detect em-dash in T—shirt");
        let repl = matches[0].replacements()[0].value();
        assert!(repl.contains('-'), "Should suggest hyphenated form, got '{}'", repl);
    }

    #[test]
    fn test_dash_rule_en_dash() {
        let rule = DashRule::new();
        let sentence = make_sentence("I bought a T\u{2013}shirt");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Should detect en-dash in T–shirt");
    }

    #[test]
    fn test_dash_rule_hyphen_ok() {
        let rule = DashRule::new();
        let sentence = make_sentence("I bought a T-shirt");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "Correct hyphen should not trigger");
    }
}
