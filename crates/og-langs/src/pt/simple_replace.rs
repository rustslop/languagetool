use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};
use std::collections::HashMap;

pub struct SimpleReplaceRule {
    id: String,
    description: String,
    short: String,
    message: String,
    category: Category,
    issue_type: IssueType,
    case_sensitive: bool,
    check_lemmas: bool,
    sub_rule_ids: bool,
    url: Option<String>,
    entries: HashMap<String, Vec<String>>,
}

impl SimpleReplaceRule {
    pub fn new(id: &str, description: &str, short: &str, message: &str) -> Self {
        Self {
            id: id.to_string(),
            description: description.to_string(),
            short: short.to_string(),
            message: message.to_string(),
            category: Category::new("MISC", "Miscellaneous"),
            issue_type: IssueType::Other("localeViolation".to_string()),
            case_sensitive: true,
            check_lemmas: true,
            sub_rule_ids: false,
            url: None,
            entries: HashMap::new(),
        }
    }

    pub fn with_category(mut self, category: Category) -> Self {
        self.category = category;
        self
    }

    pub fn with_issue_type(mut self, issue_type: IssueType) -> Self {
        self.issue_type = issue_type;
        self
    }

    pub fn with_case_insensitive(mut self) -> Self {
        self.case_sensitive = false;
        self
    }

    pub fn with_check_lemmas(mut self, check: bool) -> Self {
        self.check_lemmas = check;
        self
    }

    pub fn with_sub_rule_ids(mut self) -> Self {
        self.sub_rule_ids = true;
        self
    }

    pub fn with_url(mut self, url: &str) -> Self {
        self.url = Some(url.to_string());
        self
    }

    pub fn load_data(&mut self, data: &str) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(2, '=');
            let wrong_side = parts.next().unwrap_or("").trim();
            let Some(right) = parts.next() else {
                continue;
            };
            let right = right.trim();
            if right.is_empty() {
                continue;
            }
            let replacements: Vec<String> = right.split('|').map(str::to_string).collect();
            for wrong_form in wrong_side.split('|') {
                self.entries
                    .insert(wrong_form.trim().to_string(), replacements.clone());
            }
        }
    }

    fn message_for(&self, token: &str, replacements: &[String]) -> String {
        self.message.replacen("$match", token, 1).replacen(
            "$suggestions",
            &replacements.join(", "),
            1,
        )
    }
}

impl Rule for SimpleReplaceRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        self.issue_type.clone()
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        for token in sentence.non_whitespace_tokens() {
            if token.is_sentence_start() || token.is_immunized() || token.is_ignored_by_speller() {
                continue;
            }
            let original = token.token().token();
            let token_string = if self.case_sensitive {
                original.to_string()
            } else {
                original.to_lowercase()
            };
            let mut replacements: Option<Vec<String>> = self.entries.get(original).cloned();
            if replacements.is_none() {
                replacements = self.entries.get(&token_string).cloned();
            }
            if replacements.is_none() && self.check_lemmas {
                let mut lemmas: Vec<String> = Vec::new();
                for reading in token.readings() {
                    if let Some(lemma) = reading.lemma()
                        && self.entries.contains_key(lemma)
                        && !lemmas.contains(&lemma.to_lowercase())
                    {
                        lemmas.push(lemma.to_lowercase());
                    }
                }
                let mut collected: Vec<String> = Vec::new();
                for lemma in &lemmas {
                    if let Some(found) = self.entries.get(lemma) {
                        for r in found {
                            if !collected.contains(r) {
                                collected.push(r.clone());
                            }
                        }
                    }
                }
                if !collected.is_empty() {
                    replacements = Some(collected);
                }
            }
            let Some(mut replacements) = replacements else {
                continue;
            };
            if original
                .chars()
                .all(|c| !c.is_alphabetic() || c.is_uppercase())
            {
                replacements = replacements.iter().map(|s| s.to_uppercase()).collect();
            }
            replacements.retain(|r| r != original);
            if replacements.is_empty() {
                continue;
            }
            let message = self.message_for(original, &replacements);
            let mut rm_rule = RuleMatchRule::new(&self.id, &self.description)
                .with_category(self.category.clone())
                .with_issue_type(self.issue_type.as_str());
            if self.sub_rule_ids {
                let id = to_id(&format!("{}_{}", self.id, original));
                rm_rule = RuleMatchRule::new(id, self.description.clone())
                    .with_category(self.category.clone())
                    .with_issue_type(self.issue_type.as_str());
            }
            if let Some(url) = &self.url {
                rm_rule = rm_rule.with_urls(vec![url.clone()]);
            }
            if !self.case_sensitive && original.chars().next().is_some_and(|c| c.is_uppercase()) {
                replacements = replacements
                    .iter()
                    .map(|s| uppercase_first_char(s))
                    .collect();
            }
            let from = token.token().start();
            let to = token.token().end();
            let ctx_start = from.saturating_sub(40);
            let ctx_end = std::cmp::min(to + 40, sentence.text().len());
            let context_text = sentence.text()[ctx_start..ctx_end].to_string();
            rule_matches.push(
                RuleMatch::new(
                    message,
                    from,
                    to - from,
                    rm_rule,
                    RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                )
                .with_short_message(self.short.clone())
                .with_replacements(replacements.iter().map(SuggestedReplacement::new).collect())
                .with_sentence(sentence.text().to_string()),
            );
        }
        rule_matches
    }
}

fn to_id(input: &str) -> String {
    input
        .trim()
        .to_uppercase()
        .replace(' ', "_")
        .replace('\'', "_Q_")
}

fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => s.to_string(),
    }
}

pub fn portuguese_replace_rule() -> SimpleReplaceRule {
    let mut rule = SimpleReplaceRule::new(
        "PT_SIMPLE_REPLACE",
        "Palavras estrangeiras facilmente confundidas em Português",
        "Estrangeirismo",
        "'$match' é um estrangeirismo. Em Português é mais comum usar: $suggestions.",
    )
    .with_category(Category::new("STYLE", "Style"))
    .with_issue_type(IssueType::Other("localeViolation".to_string()))
    .with_case_insensitive()
    .with_sub_rule_ids()
    .with_url("https://pt.wikipedia.org/wiki/Estrangeirismo");
    rule.load_data(include_str!("../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/replace.txt"));
    rule
}

pub fn orthography_replace_rule() -> SimpleReplaceRule {
    let mut rule = SimpleReplaceRule::new(
        "PT_SIMPLE_REPLACE_ORTHOGRAPHY",
        "Possible spelling mistake",
        "Possible spelling mistake",
        "Possible spelling mistake.",
    )
    .with_category(Category::new("TYPOS", "Possible Typo"))
    .with_issue_type(IssueType::Misspelling)
    .with_case_insensitive()
    .with_sub_rule_ids();
    rule.load_data(include_str!("../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/replace_orthography.txt"));
    rule
}

pub fn agreement_replace_rule() -> SimpleReplaceRule {
    let mut rule = SimpleReplaceRule::new(
        "PT_AGREEMENT_REPLACE",
        "Palavras alteradas pelo Acordo Ortográfico de 90",
        "Forma do Acordo Ortográfico de 45.",
        "'$match' é uma forma do antigo acordo ortográfico. No novo acordo ortográfico, a palavra escreve-se assim: $suggestions.",
    )
    .with_category(Category::new("TYPOS", "Possible Typo"))
    .with_issue_type(IssueType::Misspelling)
    .with_case_insensitive()
    .with_sub_rule_ids()
    .with_url("https://pt.wikipedia.org/wiki/Lista_das_alterações_previstas_pelo_acordo_ortográfico_de_1990");
    rule.load_data(include_str!("../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/AOreplace.txt"));
    rule
}

pub fn english_contraction_rule() -> SimpleReplaceRule {
    let mut rule = SimpleReplaceRule::new(
        "PT_ENGLISH_CONTRACTION_ORTHOGRAPHY",
        "Ortografia de contrações inglesas",
        "Erro de ortografia inglesa",
        "Caso seja uma contração da língua inglesa, prefira \"$suggestions\".",
    )
    .with_category(Category::new("TYPOS", "Possible Typo"))
    .with_issue_type(IssueType::Misspelling)
    .with_check_lemmas(false);
    rule.load_data(include_str!("../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/english_contractions.txt"));
    rule
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pt::test_util::make_sentence;

    fn first_replacement(rule: &SimpleReplaceRule, text: &str) -> Option<String> {
        let sentence = make_sentence(text);
        rule.match_sentence(&sentence)
            .first()
            .and_then(|m| m.replacements().first().map(|r| r.value().to_string()))
    }

    #[test]
    fn agreement_replace_matches_java_example() {
        let rule = agreement_replace_rule();
        assert_eq!(
            first_replacement(&rule, "abstracto"),
            Some("abstrato".into())
        );
    }

    #[test]
    fn orthography_replace_suggests_accented_form() {
        let rule = orthography_replace_rule();
        assert_eq!(first_replacement(&rule, "nao"), Some("não".into()));
    }

    #[test]
    fn orthography_replace_is_case_insensitive() {
        let rule = orthography_replace_rule();
        let sentence = make_sentence("Nao é assim.");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches[0].replacements()[0].value(), "Não");
    }

    #[test]
    fn english_contraction_matches_java_example() {
        let rule = english_contraction_rule();
        let sentence = make_sentence("Ele adorava assistir whats cooking às sextas-feiras.");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty());
        assert!(matches[0].replacements()[0].value().contains("what's"));
    }

    #[test]
    fn english_contraction_is_case_sensitive() {
        let rule = english_contraction_rule();
        assert!(first_replacement(&rule, "AINT").is_some());
        assert!(first_replacement(&rule, "aInT").is_none());
    }

    #[test]
    fn lemma_path_finds_replacements() {
        let mut rule =
            SimpleReplaceRule::new("T", "d", "s", "$match -> $suggestions").with_case_insensitive();
        rule.load_data("correr=andar\n");
        let mut sentence = make_sentence("corro");
        sentence.tokens_mut()[1].readings_mut().push(
            og_core::AnalyzedToken::new("corro", 0, 5)
                .with_lemma("correr")
                .with_pos_tags(vec!["VMIP1S0".to_string()]),
        );
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "andar");
    }

    #[test]
    fn base_replace_rule_has_no_data() {
        let rule = portuguese_replace_rule();
        assert_eq!(rule.id(), "PT_SIMPLE_REPLACE");
        assert!(rule.entries.is_empty());
    }
}
