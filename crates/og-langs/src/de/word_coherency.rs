use super::helpers::{eq_ignore_case, starts_with_uppercase, uppercase_first_char};
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::TextLevelRule,
};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

static WORD_MAP: LazyLock<HashMap<String, HashSet<String>>> = LazyLock::new(|| {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    for line in include_str!(
            "../../../../languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/rules/de/coherency.txt"
        )
        .lines()
    {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut parts = line.split(';');
        let (Some(a), Some(b)) = (parts.next(), parts.next()) else {
            continue;
        };
        map.entry(a.to_string()).or_default().insert(b.to_string());
        map.entry(b.to_string()).or_default().insert(a.to_string());
    }
    map
});

fn replace_first_ci(text: &str, from: &str, to: &str) -> String {
    let from_lower = from.to_lowercase();
    match text.to_lowercase().find(&from_lower) {
        Some(pos) => {
            let mut result = String::with_capacity(text.len());
            result.push_str(&text[..pos]);
            result.push_str(to);
            result.push_str(&text[pos + from.len()..]);
            result
        }
        None => text.to_string(),
    }
}

pub struct GermanWordCoherencyRule;

impl Default for GermanWordCoherencyRule {
    fn default() -> Self {
        Self
    }
}

impl TextLevelRule for GermanWordCoherencyRule {
    fn id(&self) -> &str {
        "DE_WORD_COHERENCY"
    }

    fn description(&self) -> &str {
        "Einheitliche Schreibweise für Wörter mit mehr als einer korrekten Schreibweise"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Verschiedenes")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Inconsistency
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let mut should_not_appear: HashMap<String, String> = HashMap::new();
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            for atr in tokens.iter() {
                if atr.readings().is_empty() {
                    continue;
                }
                let mut baseforms: Vec<&str> =
                    atr.readings().iter().filter_map(|r| r.lemma()).collect();
                if baseforms.is_empty() {
                    baseforms.push(atr.token().token());
                }
                let mut seen: HashSet<&str> = HashSet::new();
                for baseform in baseforms {
                    if !seen.insert(baseform) {
                        continue;
                    }
                    let token = baseform;
                    let from_pos = atr.token().start();
                    let to_pos = atr.token().end();
                    if let Some(other_spelling) = should_not_appear.get(token) {
                        let msg = format!(
                            "'{token}' und '{other_spelling}' sollten nicht gleichzeitig benutzt werden."
                        );
                        let marked = &text[atr.token().start()..atr.token().end()];
                        let mut replacement = replace_first_ci(marked, token, other_spelling);
                        if starts_with_uppercase(atr.token().token()) {
                            replacement = uppercase_first_char(&replacement);
                        }
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_category(self.category())
                            .with_issue_type(self.issue_type().as_str());
                        if !eq_ignore_case(marked, &replacement) {
                            rule_matches.push(
                                RuleMatch::new(
                                    msg,
                                    from_pos,
                                    to_pos - from_pos,
                                    rm_rule,
                                    RuleMatchContext::new(
                                        sentence.text().to_string(),
                                        from_pos,
                                        to_pos - from_pos,
                                    ),
                                )
                                .with_replacements(vec![SuggestedReplacement::new(replacement)])
                                .with_sentence(sentence.text().to_string()),
                            );
                        }
                        break;
                    } else if let Some(should_not_set) = WORD_MAP.get(token) {
                        for should_not in should_not_set {
                            should_not_appear.insert(should_not.clone(), token.to_string());
                        }
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
    fn detects_inconsistent_spelling() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check(
            "Die Delfine gehören zu den Zahnwalen. Delphine sind in allen Meeren verbreitet.",
        );
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_WORD_COHERENCY"),
            "expected DE_WORD_COHERENCY match"
        );
    }

    #[test]
    fn consistent_spelling_ok() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check(
            "Die Delfine gehören zu den Zahnwalen. Delfine sind in allen Meeren verbreitet.",
        );
        assert!(
            !matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_WORD_COHERENCY")
        );
    }
}
