use super::helpers::starts_with_uppercase;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule, rule::Rule,
};
use regex::Regex;
use std::sync::LazyLock;

struct ContextWords {
    words: [Regex; 2],
    matches: [String; 2],
    contexts: [Regex; 2],
    explanations: [String; 2],
}

static ENTRIES: LazyLock<Vec<ContextWords>> = LazyLock::new(|| {
    load(include_str!(
        "../../../../languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/rules/de/wrongWordInContext.txt"
    ))
});

fn add_boundaries(pattern: &str) -> String {
    if let Some(stripped) = pattern.strip_prefix("(?i)") {
        format!(r"(?i)\b({stripped})\b")
    } else {
        format!(r"\b({pattern})\b")
    }
}

fn load(data: &str) -> Vec<ContextWords> {
    let mut entries = Vec::new();
    for line in data.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let column: Vec<&str> = line.split('\t').collect();
        if column.len() < 6 {
            continue;
        }
        let (Ok(word0), Ok(word1), Ok(context0), Ok(context1)) = (
            Regex::new(&add_boundaries(column[0])),
            Regex::new(&add_boundaries(column[1])),
            Regex::new(&add_boundaries(column[4])),
            Regex::new(&add_boundaries(column[5])),
        ) else {
            continue;
        };
        entries.push(ContextWords {
            words: [word0, word1],
            matches: [column[2].to_string(), column[3].to_string()],
            contexts: [context0, context1],
            explanations: [
                column.get(6).unwrap_or(&"").to_string(),
                column.get(7).unwrap_or(&"").to_string(),
            ],
        });
    }
    entries
}

fn preserve_case(source: &str, replacement: &str) -> String {
    if source
        .chars()
        .all(|c| !c.is_alphabetic() || c.is_uppercase())
    {
        replacement.to_uppercase()
    } else if starts_with_uppercase(source) {
        super::helpers::uppercase_first_char(replacement)
    } else {
        replacement.to_string()
    }
}

fn to_rule_id(input: &str) -> String {
    input
        .trim()
        .to_uppercase()
        .replace(' ', "_")
        .replace('\'', "_Q_")
        .replace('Ä', "AE")
        .replace('Ü', "UE")
        .replace('Ö', "OE")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub struct GermanWrongWordInContextRule;

impl GermanWrongWordInContextRule {
    fn message(
        &self,
        wrong_word: &str,
        suggestion: &str,
        expl_sugg: &str,
        expl_wrong: &str,
    ) -> String {
        if expl_sugg.is_empty() || expl_wrong.is_empty() {
            format!(
                "Mögliche Wortverwechslung: Meinten Sie <suggestion>{suggestion}</suggestion> anstatt '{wrong_word}'?"
            )
        } else {
            format!(
                "Mögliche Wortverwechslung: Meinten Sie <suggestion>{suggestion}</suggestion> (= {expl_sugg}) anstatt '{wrong_word}' (= {expl_wrong})?"
            )
        }
    }
}

impl Default for GermanWrongWordInContextRule {
    fn default() -> Self {
        Self
    }
}

impl Rule for GermanWrongWordInContextRule {
    fn id(&self) -> &str {
        "GERMAN_WRONG_WORD_IN_CONTEXT"
    }

    fn description(&self) -> &str {
        "Mögliche Wortverwechslungen"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("CONFUSED_WORDS", "Leicht zu verwechselnde Wörter")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("confused_word".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        for entry in ENTRIES.iter() {
            let mut matched_word = [false, false];
            let mut matched_token = ["", ""];
            let mut matched_pos = [0usize; 2];
            for atr in tokens.iter() {
                let token = atr.token().token();
                for side in 0..2 {
                    if !matched_word[side] && entry.words[side].is_match(token) {
                        matched_word[side] = true;
                        matched_token[side] = token;
                        matched_pos[side] = atr.token().start();
                    }
                }
            }
            let found_word = if matched_word[0] && !matched_word[1] {
                0
            } else if matched_word[1] && !matched_word[0] {
                1
            } else {
                continue;
            };
            let not_found_word = 1 - found_word;
            let mut matched_context = [false, false];
            for atr in tokens.iter() {
                let token = atr.token().token();
                for side in [found_word, not_found_word] {
                    if !matched_context[side] && entry.contexts[side].is_match(token) {
                        matched_context[side] = true;
                    }
                }
            }
            if !matched_context[not_found_word] || matched_context[found_word] {
                continue;
            }
            let token = matched_token[found_word];
            let original = &entry.matches[found_word];
            let replacement = &entry.matches[not_found_word];
            let replaced = replace_first_ci(token, original, replacement);
            let repl = preserve_case(token, &replaced);
            let msg = self.message(
                token,
                &repl,
                &entry.explanations[not_found_word],
                &entry.explanations[found_word],
            );
            let specific_id = to_rule_id(&format!("GERMAN_WRONG_WORD_IN_CONTEXT_{token}_{repl}"));
            let desc = format!("Mögliche Wortverwechslungen: {token}/{repl}");
            let rm_rule = RuleMatchRule::new(specific_id, desc)
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str());
            let start = matched_pos[found_word];
            let end = start + token.len();
            rule_matches.push(
                RuleMatch::new(
                    msg,
                    start,
                    end - start,
                    rm_rule,
                    RuleMatchContext::new(sentence.text().to_string(), start, end - start),
                )
                .with_short_message("Mögliche Wortverwechslung")
                .with_sentence(sentence.text().to_string()),
            );
        }
        rule_matches
    }
}

fn replace_first_ci(text: &str, from: &str, to: &str) -> String {
    let from_lower = from.to_lowercase();
    let text_lower = text.to_lowercase();
    if let Some(pos) = text_lower.find(&from_lower) {
        let mut result = String::with_capacity(text.len());
        result.push_str(&text[..pos]);
        result.push_str(to);
        result.push_str(&text[pos + from.len()..]);
        result
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn detects_mine_miene_confusion() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Die Miene vom Kugelschreiber ist leer.");
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id().starts_with("GERMAN_WRONG_WORD_IN_CONTEXT")),
            "expected GERMAN_WRONG_WORD_IN_CONTEXT match, got {:?}",
            matches
                .matches
                .iter()
                .map(|m| m.rule().id())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_confusion_on_correct_miene() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Er verzog das Gesicht, seine Miene war streng.");
        assert!(
            !matches
                .matches
                .iter()
                .any(|m| m.rule().id().starts_with("GERMAN_WRONG_WORD_IN_CONTEXT"))
        );
    }
}
