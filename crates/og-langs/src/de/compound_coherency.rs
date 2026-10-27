use super::helpers::{eq_ignore_case, has_same_lemmas};
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::TextLevelRule,
};
use std::collections::HashMap;

pub struct CompoundCoherencyRule;

impl Default for CompoundCoherencyRule {
    fn default() -> Self {
        Self
    }
}

fn contains_hyphen_inside(token: &str) -> bool {
    token.contains('-') && !token.starts_with('-') && !token.ends_with('-')
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn lemma_of(atr: &og_core::AnalyzedTokenReadings) -> Option<String> {
    let lemma = if has_same_lemmas(atr) && !atr.readings().is_empty() {
        atr.readings()[0].lemma().map(str::to_string)
    } else {
        None
    }?;
    let token = atr.token().token();
    if !lemma.contains('-') && token.contains('-') {
        let token_chars: Vec<char> = token.chars().collect();
        let lemma_chars: Vec<char> = lemma.chars().collect();
        let mut lemma_builder = String::new();
        let mut token_pos = 0usize;
        for (lemma_pos, lemma_char) in lemma_chars.iter().enumerate() {
            let lemma_char = *lemma_char;
            if token_pos >= token_chars.len() {
                break;
            }
            let token_char = token_chars[token_pos];
            if lemma_char == token_char {
                lemma_builder.push(lemma_char);
            } else if token_char == '-' {
                token_pos += 1;
                lemma_builder.push('-');
                if lemma_pos + 1 < token_chars.len()
                    && token_chars.get(token_pos).is_some_and(|c| c.is_uppercase())
                {
                    lemma_builder.push(lemma_char.to_uppercase().next().unwrap_or(lemma_char));
                } else {
                    lemma_builder.push(lemma_char);
                }
            }
            token_pos += 1;
        }
        return Some(lemma_builder);
    }
    Some(lemma)
}

impl TextLevelRule for CompoundCoherencyRule {
    fn id(&self) -> &str {
        "DE_COMPOUND_COHERENCY"
    }

    fn description(&self) -> &str {
        "Einheitliche Schreibweise bei Komposita (mit oder ohne Bindestrich)"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("STYLE", "Stil")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let mut norm_to_text_occurrences: HashMap<String, Vec<String>> = HashMap::new();
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            for atr in tokens.iter() {
                let token = atr.token().token();
                if token.is_empty() {
                    continue;
                }
                let lemma = lemma_of(atr).unwrap_or_else(|| token.to_string());
                let norm_token = lemma.replace('-', "").to_lowercase();
                if is_numeric(&norm_token) {
                    break;
                }
                let entry = norm_to_text_occurrences.entry(norm_token).or_default();
                if entry.is_empty() {
                    entry.push(lemma);
                } else if !entry.iter().any(|f| eq_ignore_case(f, &lemma)) {
                    let other = entry[0].clone();
                    if contains_hyphen_inside(&other) || contains_hyphen_inside(token) {
                        let msg = format!(
                            "Uneinheitliche Verwendung von Bindestrichen. Der Text enthält sowohl '{token}' als auch '{other}'."
                        );
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_category(self.category())
                            .with_issue_type(self.issue_type().as_str());
                        let from = atr.token().start();
                        let to = atr.token().end();
                        let mut rule_match = RuleMatch::new(
                            msg,
                            from,
                            to - from,
                            rm_rule,
                            RuleMatchContext::new(sentence.text().to_string(), from, to - from),
                        )
                        .with_sentence(sentence.text().to_string());
                        if eq_ignore_case(&token.replace('-', ""), &other.replace('-', "")) {
                            rule_match = rule_match
                                .with_replacements(vec![SuggestedReplacement::new(other)]);
                        }
                        rule_matches.push(rule_match);
                    }
                }
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn detects_helpdesk_inconsistency() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check(
            "Ein Helpdesk gliedert sich in verschiedene Level. Die Qualität des Help-Desks ist wichtig.",
        );
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_COMPOUND_COHERENCY"),
            "expected DE_COMPOUND_COHERENCY match"
        );
    }
}
