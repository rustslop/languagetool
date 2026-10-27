use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use std::collections::HashMap;

const REPLACE_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/fr/src/main/resources/org/languagetool/rules/fr/replace.txt"
);
const REPLACE_CUSTOM_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/fr/src/main/resources/org/languagetool/rules/fr/replace_custom.txt"
);

pub struct SimpleReplaceRule {
    wrong_words: HashMap<String, Vec<String>>,
}

fn java_split_semicolons(value: &str) -> Vec<String> {
    let mut parts: Vec<String> = value.split(';').map(str::to_string).collect();
    while parts.last().is_some_and(String::is_empty) {
        parts.pop();
    }
    parts
}

fn load_data(data: &str) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for line in data.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((wrong_side, replacements)) = line.split_once('=') else {
            continue;
        };
        for wrong in wrong_side.split('|') {
            let key = wrong.trim().to_lowercase();
            if key.is_empty() {
                continue;
            }
            map.insert(key, java_split_semicolons(replacements));
        }
    }
    map
}

fn is_all_uppercase(s: &str) -> bool {
    s.chars().any(char::is_alphabetic) && !s.chars().any(char::is_lowercase)
}

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_uppercase())
}

fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn to_id_suffix(s: &str) -> String {
    s.to_uppercase()
        .replace(' ', "_")
        .replace('\'', "_Q_")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn is_tagged(atr: &AnalyzedTokenReadings) -> bool {
    atr.readings().iter().any(|r| !r.pos_tags().is_empty())
}

impl SimpleReplaceRule {
    pub fn french() -> Self {
        let mut wrong_words = load_data(REPLACE_TXT);
        for (key, value) in load_data(REPLACE_CUSTOM_TXT) {
            wrong_words.insert(key, value);
        }
        Self { wrong_words }
    }

    pub fn entry_count(&self) -> usize {
        self.wrong_words.len()
    }
}

impl Default for SimpleReplaceRule {
    fn default() -> Self {
        Self::french()
    }
}

impl Rule for SimpleReplaceRule {
    fn id(&self) -> &str {
        "FR_SIMPLE_REPLACE_SIMPLE"
    }

    fn description(&self) -> &str {
        "Mot incorrect : $match"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Faute de frappe possible")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let text = sentence.text();
        let sent_start = sentence.start();
        for atr in sentence.non_whitespace_tokens() {
            if atr.is_sentence_start()
                || atr.is_immunized()
                || atr.is_ignored_by_speller()
                || is_tagged(atr)
            {
                continue;
            }
            let original_token = atr.token().token();
            let Some(possible) = self
                .wrong_words
                .get(original_token)
                .or_else(|| self.wrong_words.get(&original_token.to_lowercase()))
            else {
                continue;
            };
            let all_uppercase = is_all_uppercase(original_token);
            let mut replacements: Vec<String> = if all_uppercase {
                possible.iter().map(|s| s.to_uppercase()).collect()
            } else {
                possible.clone()
            };
            if !all_uppercase && starts_with_uppercase(original_token) {
                for r in &mut replacements {
                    *r = uppercase_first_char(r);
                }
            }
            replacements.retain(|r| r != original_token);
            if replacements.is_empty() {
                continue;
            }
            let sub_id = format!("{}_{}", self.id(), to_id_suffix(original_token));
            let desc = self.description().replace("$match", original_token);
            let message = format!("Vouliez-vous dire « {} » ?", replacements[0]);
            let rm_rule = RuleMatchRule::new(sub_id, desc)
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str());
            let from = atr.token().start();
            let to = atr.token().end();
            let ctx_text = crate::fr::context_window(text, sent_start, from, to);
            rule_matches.push(
                RuleMatch::new(
                    message,
                    from,
                    to - from,
                    rm_rule,
                    RuleMatchContext::new(ctx_text, from, to - from),
                )
                .with_short_message("Mot incorrect")
                .with_replacements(replacements.iter().map(SuggestedReplacement::new).collect())
                .with_sentence(text.to_string()),
            );
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
    use og_tokenizer::FrenchWordTokenizer;
    use std::sync::Arc;

    use og_tokenizer::WordTokenizer as _;

    fn sentence_from(text: &str, tagged: &[&str]) -> AnalyzedSentence {
        let tagged: Vec<String> = tagged.iter().map(|s| s.to_string()).collect();
        let tokenizer =
            FrenchWordTokenizer::new(Arc::new(move |w: &str| tagged.iter().any(|t| t == w)));
        let mut tokens: Vec<AnalyzedTokenReadings> = tokenizer
            .tokenize(text)
            .iter()
            .map(|t| AnalyzedTokenReadings::new(AnalyzedToken::from_token(t)))
            .collect();
        tokens.insert(
            0,
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
        );
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn tagged_words_are_ignored() {
        let rule = SimpleReplaceRule::french();
        let sentence = sentence_from("J'ai pas de quoi", &["J'", "ai", "pas", "de", "quoi"]);
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn untagged_wrong_word_matches_with_suggestion() {
        let rule = SimpleReplaceRule::french();
        let sentence = sentence_from("jai pas de quoi", &["pas", "de", "quoi"]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "j'ai");
    }

    #[test]
    fn capitalized_suggestion_uppercases_first_char() {
        let rule = SimpleReplaceRule::french();
        let sentence = sentence_from("Jai pas de quoi", &["pas", "de", "quoi"]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "J'ai");
    }

    #[test]
    fn sub_rule_id_and_description_carry_the_token() {
        let rule = SimpleReplaceRule::french();
        let sentence = sentence_from("jai pas de quoi", &["pas", "de", "quoi"]);
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches[0]
                .rule()
                .id()
                .starts_with("FR_SIMPLE_REPLACE_SIMPLE_JAI")
        );
        assert_eq!(matches[0].rule().description(), "Mot incorrect : jai");
    }

    #[test]
    fn data_loads_expected_entries() {
        let rule = SimpleReplaceRule::french();
        assert!(rule.entry_count() >= 30);
        assert_eq!(
            rule.wrong_words.get("jai").map(Vec::as_slice),
            Some(["j'ai".to_string()].as_slice())
        );
    }
}
