use std::collections::HashSet;

use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};

pub struct RussianDashRule {
    compounds: HashSet<String>,
}

impl RussianDashRule {
    pub fn new() -> Self {
        Self::from_data(include_str!(
            "../../../../languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru/compounds.txt"
        ))
    }

    fn from_data(data: &str) -> Self {
        let mut compounds = HashSet::new();
        for line in data.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut word = line;
            if word.ends_with('+') || word.ends_with('?') {
                continue;
            }
            if word.ends_with('*') || word.ends_with('$') {
                word = &word[..word.len() - 1];
            }
            compounds.insert(word.to_string());
        }
        Self { compounds }
    }
}

fn is_dash(c: char) -> bool {
    c == '\u{2013}' || c == '\u{2014}' || c == '\u{2012}'
}

fn is_word_char(c: char) -> bool {
    !c.is_whitespace()
        && c != ','
        && c != '.'
        && c != '!'
        && c != '?'
        && c != ':'
        && c != ';'
        && c != ')'
        && c != '('
}

impl Default for RussianDashRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for RussianDashRule {
    fn id(&self) -> &str {
        "RU_DASH_RULE"
    }

    fn is_picky(&self) -> bool {
        true
    }

    fn description(&self) -> &str {
        "Тире вместо дефиса («из — за» вместо «из-за»)."
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("TYPOGRAPHY", "Typography")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let text = sentence.text();
        let char_text: Vec<(usize, char)> = text.char_indices().collect();
        let mut matches = Vec::new();
        let mut idx = 0usize;

        while idx < char_text.len() {
            if !is_dash(char_text[idx].1) {
                idx += 1;
                continue;
            }
            let mut left = idx;
            if left > 0 && char_text[left - 1].1 == ' ' {
                left -= 1;
            }
            while left > 0 && is_word_char(char_text[left - 1].1) {
                left -= 1;
            }
            let mut right = idx + 1;
            if right < char_text.len() && char_text[right].1 == ' ' {
                right += 1;
            }
            loop {
                while right < char_text.len() && is_word_char(char_text[right].1) {
                    right += 1;
                }
                let next = right;
                let mut peek = next;
                if peek < char_text.len() && char_text[peek].1 == ' ' {
                    peek += 1;
                }
                if peek < char_text.len() && is_dash(char_text[peek].1) {
                    let mut after = peek + 1;
                    if after < char_text.len() && char_text[after].1 == ' ' {
                        after += 1;
                    }
                    right = after;
                } else {
                    break;
                }
            }
            if left == idx || right == idx + 1 {
                idx += 1;
                continue;
            }
            let start = char_text[left].0;
            let end = if right < char_text.len() {
                char_text[right].0
            } else {
                text.len()
            };
            let covered = &text[start..end];
            let suggestion: String = covered
                .chars()
                .filter(|c| *c != ' ')
                .map(|c| if is_dash(c) { '-' } else { c })
                .collect();
            if self.compounds.contains(&suggestion) {
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_picky(true)
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                let ctx_start = start.saturating_sub(40);
                let ctx_end = std::cmp::min(end + 40, text.len());
                let context_text = text[ctx_start..ctx_end].to_string();
                matches.push(
                    RuleMatch::new(
                        "Использовано тире вместо дефиса.",
                        start,
                        end - start,
                        rm_rule,
                        RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                    )
                    .with_replacements(vec![SuggestedReplacement::new(&suggestion)])
                    .with_sentence(text.to_string()),
                );
            }
            idx = right;
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_spaced_em_dash_compound() {
        let rule = RussianDashRule::from_data("из-за\n");
        let text = "из — за дождя";
        let sentence = AnalyzedSentence::new(text, 0, text.len());
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "из-за");
    }

    #[test]
    fn flags_tight_en_dash_compound() {
        let rule = RussianDashRule::from_data("из-за\n");
        let text = "из–за дождя";
        let sentence = AnalyzedSentence::new(text, 0, text.len());
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "из-за");
    }

    #[test]
    fn leaves_hyphen_compound_alone() {
        let rule = RussianDashRule::from_data("из-за\n");
        let text = "Из-за дождя";
        let sentence = AnalyzedSentence::new(text, 0, text.len());
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn skips_unknown_dash_pairs() {
        let rule = RussianDashRule::from_data("из-за\n");
        let text = "Москва — большой город";
        let sentence = AnalyzedSentence::new(text, 0, text.len());
        assert!(rule.match_sentence(&sentence).is_empty());
    }
}
