use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};
use std::collections::HashMap;

const REPLACE_DATA: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/replace.txt"
);
const REPLACE_CUSTOM_DATA: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/replace_custom.txt"
);
const WIKIPEDIA_DATA: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/wikipedia.txt"
);

fn is_tagged(atr: &og_core::AnalyzedTokenReadings) -> bool {
    atr.readings().iter().any(|r| {
        r.pos_tags()
            .iter()
            .any(|t| t != "SENT_START" && t != "SENT_END" && t != "PARAGRAPH_END")
    })
}

fn is_all_uppercase(word: &str) -> bool {
    word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
}

pub struct SimpleReplaceRule {
    id: &'static str,
    description: &'static str,
    message: &'static str,
    category: (String, String),
    issue_type: IssueType,
    default_on: bool,
    ignore_tagged_words: bool,
    entries: HashMap<String, Vec<String>>,
}

impl SimpleReplaceRule {
    fn load_data(entries: &mut HashMap<String, Vec<String>>, data: &str) {
        for line in data.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((wrong_side, right_side)) = line.split_once('=') else {
                continue;
            };
            if right_side.trim().is_empty() {
                continue;
            }
            let replacements: Vec<String> = right_side.split('|').map(str::to_string).collect();
            for wrong_form in wrong_side.split('|') {
                entries.insert(wrong_form.to_string(), replacements.clone());
            }
        }
    }

    pub fn spanish() -> Self {
        let mut entries = HashMap::new();
        Self::load_data(&mut entries, REPLACE_DATA);
        Self::load_data(&mut entries, REPLACE_CUSTOM_DATA);
        Self {
            id: "ES_SIMPLE_REPLACE_SIMPLE",
            description: "Palabra incorrecta: $match",
            message: "",
            category: ("TYPOS".to_string(), "Ortografía".to_string()),
            issue_type: IssueType::Misspelling,
            default_on: true,
            ignore_tagged_words: true,
            entries,
        }
    }

    pub fn wikipedia() -> Self {
        let mut entries = HashMap::new();
        Self::load_data(&mut entries, WIKIPEDIA_DATA);
        Self {
            id: "ES_WIKIPEDIA_COMMON_ERRORS",
            description: "Errores frecuentes en los artículos de la Wikipedia",
            message: "'$match' es una expresión errónea. Pruebe a utilizar $suggestions",
            category: ("WIKIPEDIA".to_string(), "Wikipedia".to_string()),
            issue_type: IssueType::Grammar,
            default_on: false,
            ignore_tagged_words: false,
            entries,
        }
    }

    #[cfg(test)]
    fn load_data_for_test(&mut self, data: &str) {
        Self::load_data(&mut self.entries, data);
    }

    fn render_message(&self, matched: &str, replacements: &[String]) -> String {
        if self.message.is_empty() {
            if replacements.is_empty() {
                return "Palabra incorrecta".to_string();
            }
            return format!("¿Quería decir «{}»?", replacements[0]);
        }
        let suggestions = replacements.join(" o ");
        self.message
            .replace("$match", matched)
            .replace("$suggestions", &suggestions)
    }
}

impl Rule for SimpleReplaceRule {
    fn id(&self) -> &str {
        self.id
    }

    fn description(&self) -> &str {
        self.description
    }

    fn is_default_on(&self) -> bool {
        self.default_on
    }

    fn category(&self) -> Category {
        Category::new(&self.category.0, &self.category.1)
    }

    fn issue_type(&self) -> IssueType {
        self.issue_type.clone()
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let tokens = sentence.non_whitespace_tokens();
        for i in 0..tokens.len() {
            if tokens[i].token().token() == "<S>" {
                continue;
            }
            if tokens[i].is_immunized() || tokens[i].is_ignored_by_speller() {
                continue;
            }
            if self.ignore_tagged_words && is_tagged(tokens[i]) {
                continue;
            }
            let mut possible: Option<&Vec<String>> = None;
            let mut phrase_len = 0usize;
            for len in (1..=3).rev() {
                if i + len > tokens.len() {
                    continue;
                }
                let phrase: String = tokens[i..i + len]
                    .iter()
                    .map(|t| t.token().token())
                    .collect::<Vec<_>>()
                    .join(" ");
                let phrase_lower = phrase.to_lowercase();
                if let Some(entry) = self
                    .entries
                    .get(&phrase)
                    .or_else(|| self.entries.get(&phrase_lower))
                {
                    possible = Some(entry);
                    phrase_len = len;
                    break;
                }
            }
            let Some(possible) = possible else {
                continue;
            };
            let original: String = tokens[i..i + phrase_len]
                .iter()
                .map(|t| t.token().token())
                .collect::<Vec<_>>()
                .join(" ");
            let atr = tokens[i];
            let replacements: Vec<String> = if is_all_uppercase(&original) {
                possible.iter().map(|s| s.to_uppercase()).collect()
            } else {
                possible.clone()
            };
            let replacements: Vec<String> = replacements
                .into_iter()
                .filter(|s| *s != original)
                .collect();
            if replacements.is_empty() {
                continue;
            }
            let message = self.render_message(&original, &replacements);
            let start = atr.token().start();
            let length = tokens[i + phrase_len - 1].token().end() - start;
            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str());
            let ctx_start = start.saturating_sub(40);
            let ctx_end = std::cmp::min(start + length + 40, sentence.text().len());
            matches.push(
                RuleMatch::new(
                    message,
                    start,
                    length,
                    rm_rule,
                    RuleMatchContext::new(
                        sentence.text()[ctx_start..ctx_end].to_string(),
                        ctx_start,
                        ctx_end - ctx_start,
                    ),
                )
                .with_replacements(
                    replacements
                        .iter()
                        .map(|r| SuggestedReplacement::new(r.clone()))
                        .collect(),
                )
                .with_sentence(sentence.text().to_string()),
            );
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::es::test_support::analyze_spanish;

    #[test]
    fn flags_untagged_wrong_word_with_first_suggestion_message() {
        let rule = SimpleReplaceRule::spanish();
        let sentences = analyze_spanish("satisfacere");
        let matches = rule.match_sentence(&sentences[0]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "satisfaré");
        assert_eq!(matches[0].message(), "¿Quería decir «satisfaré»?");
    }

    #[test]
    fn skips_tagged_words() {
        let mut rule = SimpleReplaceRule::spanish();
        rule.load_data_for_test("casa=kasa\n");
        let sentences = analyze_spanish("La casa es grande.");
        let matches = rule.match_sentence(&sentences[0]);
        assert!(matches.is_empty());
    }

    #[test]
    fn wikipedia_matches_untagged_common_error() {
        let rule = SimpleReplaceRule::wikipedia();
        let sentences = analyze_spanish("a basto");
        let matches = rule.match_sentence(&sentences[0]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "abasto");
        assert!(!rule.is_default_on());
    }
}
