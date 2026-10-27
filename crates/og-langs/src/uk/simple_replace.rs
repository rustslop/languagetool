use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

pub(crate) const REPLACE_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/uk/src/main/resources/org/languagetool/rules/uk/replace.txt"
);
pub(crate) const REPLACE_SOFT_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/uk/src/main/resources/org/languagetool/rules/uk/replace_soft.txt"
);
pub(crate) const REPLACE_RENAMED_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/uk/src/main/resources/org/languagetool/rules/uk/replace_renamed.txt"
);
const DERIVATS_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/uk/src/main/resources/org/languagetool/resource/uk/derivats.txt"
);

fn load_wrong_words(data: &str) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for line in data.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((wrong_side, suggestion_side)) = line.split_once('=') else {
            continue;
        };
        let suggestions: Vec<String> = suggestion_side.split('|').map(str::to_string).collect();
        for wrong_form in wrong_side.split('|') {
            map.insert(wrong_form.to_string(), suggestions.clone());
        }
    }
    map
}

fn load_renamed_lists(data: &str) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for raw in data.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split(['=', '|']).map(str::trim);
        let Some(key) = parts.next() else { continue };
        let list: Vec<String> = parts.map(str::to_string).collect();
        map.insert(key.to_string(), list);
    }
    map
}

pub(crate) fn derivatives_map() -> &'static HashMap<String, Vec<String>> {
    static MAP: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for line in DERIVATS_TXT.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split(' ');
            let (Some(key), Some(value)) = (parts.next(), parts.next()) else {
                continue;
            };
            let entry = map.entry(key.to_string()).or_default();
            for tag in value.split(':') {
                if !entry.iter().any(|v| v == tag) {
                    entry.push(tag.to_string());
                }
            }
        }
        map
    })
}

pub(crate) fn replace_words() -> &'static HashMap<String, Vec<String>> {
    static MAP: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    MAP.get_or_init(|| load_wrong_words(REPLACE_TXT))
}

fn clean_token(token: &str) -> String {
    token.replace(['\u{00AD}', '\u{0301}'], "")
}

fn uppercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            format!("{upper}{}", chars.as_str())
        }
        None => String::new(),
    }
}

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_uppercase())
        && s.chars().nth(1).is_none_or(|c| !c.is_uppercase())
}

fn context_for(sentence: &AnalyzedSentence, start: usize, end: usize) -> RuleMatchContext {
    let text = sentence.text();
    let mut ctx_start = start.saturating_sub(40);
    while ctx_start > 0 && !text.is_char_boundary(ctx_start) {
        ctx_start -= 1;
    }
    let mut ctx_end = std::cmp::min(end + 40, text.len());
    while ctx_end < text.len() && !text.is_char_boundary(ctx_end) {
        ctx_end += 1;
    }
    RuleMatchContext::new(
        text[ctx_start..ctx_end].to_string(),
        ctx_start,
        ctx_end - ctx_start,
    )
}

#[derive(Clone, Copy, PartialEq)]
pub enum ReplaceFlavor {
    Plain,
    Soft,
}

pub struct UkrainianSimpleReplaceRule {
    flavor: ReplaceFlavor,
    wrong_words: &'static HashMap<String, Vec<String>>,
}

impl UkrainianSimpleReplaceRule {
    pub fn plain() -> Self {
        Self {
            flavor: ReplaceFlavor::Plain,
            wrong_words: replace_words(),
        }
    }

    pub fn soft() -> Self {
        static MAP: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
        Self {
            flavor: ReplaceFlavor::Soft,
            wrong_words: MAP.get_or_init(|| load_wrong_words(REPLACE_SOFT_TXT)),
        }
    }

    fn is_token_exception(&self, atr: &AnalyzedTokenReadings) -> bool {
        match self.flavor {
            ReplaceFlavor::Soft => clean_token(atr.token().token()) == "завидна",
            ReplaceFlavor::Plain => false,
        }
    }

    fn is_tagged(&self, token: &AnalyzedTokenReadings) -> bool {
        if self.flavor != ReplaceFlavor::Plain {
            return false;
        }
        for reading in token.readings() {
            let pos_tag = reading.pos_tags().join("+");
            if pos_tag.is_empty() {
                return false;
            }
            if pos_tag != "SENT_END"
                && pos_tag != "PARAGRAPH_END"
                && !pos_tag.contains("bad")
                && !pos_tag.contains("subst")
                && !pos_tag.starts_with('<')
            {
                return true;
            }
        }
        false
    }

    fn message(&self, token_str: &str, replacements: &mut Vec<String>) -> String {
        match self.flavor {
            ReplaceFlavor::Plain => {
                format!(
                    "«{token_str}» - помилкове слово, виправлення: {}.",
                    replacements.join(", ")
                )
            }
            ReplaceFlavor::Soft => {
                let mut contexts = Vec::new();
                let mut repls = Vec::new();
                for repl in replacements.iter() {
                    if let Some(ctx) = repl.strip_prefix("ctx:") {
                        for part in ctx.trim().split(',') {
                            let part = part.trim();
                            if !part.is_empty() {
                                contexts.push(part.to_string());
                            }
                        }
                    } else {
                        repls.push(repl.clone());
                    }
                }
                replacements.retain(|r| !r.starts_with("ctx:"));
                if !contexts.is_empty() {
                    return format!(
                        "«{token_str}» вживається лише в таких контекстах: {}, можливо, ви мали на увазі: {}?",
                        contexts.join(", "),
                        repls.join(", ")
                    );
                }
                format!(
                    "«{token_str}» — нерекомендоване слово, кращий варіант: {}.",
                    repls.join(", ")
                )
            }
        }
    }

    fn find_derivat_suggestions(&self, w: &str) -> Vec<String> {
        if self.flavor != ReplaceFlavor::Plain {
            return Vec::new();
        }
        let Some(bases) = derivatives_map().get(w) else {
            return Vec::new();
        };
        let ending: String = w
            .chars()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let mut suggestions = Vec::new();
        for base in bases {
            let Some(replacements) = self.wrong_words.get(base) else {
                continue;
            };
            for t in replacements {
                let mapped = derivatives_map()
                    .iter()
                    .find(|(key, values)| values.iter().any(|v| v == t) && key.ends_with(&ending))
                    .map(|(key, _)| key.clone())
                    .unwrap_or_else(|| t.clone());
                if !suggestions.contains(&mapped) {
                    suggestions.push(mapped);
                }
            }
        }
        suggestions
    }

    fn find_matches(
        &self,
        token: &AnalyzedTokenReadings,
        sentence: &AnalyzedSentence,
    ) -> Vec<RuleMatch> {
        let original_token_str = token.token().token();
        let token_string = original_token_str.to_lowercase();
        let is_all_uppercase = !original_token_str.is_empty()
            && original_token_str.chars().all(|c| !c.is_lowercase())
            && original_token_str.chars().any(|c| c.is_uppercase());

        let possible = self
            .wrong_words
            .get(original_token_str)
            .or_else(|| self.wrong_words.get(&token_string))
            .cloned();

        let mut replacements: Vec<String> = Vec::new();
        if let Some(possible) = possible.filter(|p| !p.is_empty()) {
            if is_all_uppercase {
                replacements = possible.iter().map(|s| s.to_uppercase()).collect();
            } else {
                replacements = possible.clone();
            }
            replacements.retain(|r| r != original_token_str);
        }

        if replacements.is_empty() {
            let derivat_suggestions =
                self.find_derivat_suggestions(&clean_token(original_token_str).to_lowercase());
            if !derivat_suggestions.is_empty() {
                let start = token.token().start();
                let end = start + original_token_str.len();
                let rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                return vec![
                    RuleMatch::new(
                        "Неправильне слово.".to_string(),
                        start,
                        end - start,
                        rule,
                        context_for(sentence, start, end),
                    )
                    .with_short_message(self.short().to_string())
                    .with_replacements(
                        derivat_suggestions
                            .into_iter()
                            .map(SuggestedReplacement::new)
                            .collect(),
                    )
                    .with_sentence(sentence.text().to_string()),
                ];
            }
            return Vec::new();
        }

        let message = self.message(original_token_str, &mut replacements);
        if !self.case_sensitive() && starts_with_uppercase(original_token_str) {
            for repl in replacements.iter_mut() {
                *repl = uppercase_first(repl);
            }
        }
        let start = token.token().start();
        let end = start + original_token_str.len();
        let rule = RuleMatchRule::new(self.id(), self.description())
            .with_category(self.category())
            .with_issue_type(self.issue_type().as_str());
        vec![
            RuleMatch::new(
                message,
                start,
                end - start,
                rule,
                context_for(sentence, start, end),
            )
            .with_short_message(self.short().to_string())
            .with_replacements(
                replacements
                    .into_iter()
                    .map(SuggestedReplacement::new)
                    .collect(),
            )
            .with_sentence(sentence.text().to_string()),
        ]
    }

    fn case_sensitive(&self) -> bool {
        false
    }

    fn short(&self) -> &'static str {
        match self.flavor {
            ReplaceFlavor::Plain => "Помилка?",
            ReplaceFlavor::Soft => "Нерекомендоване слово",
        }
    }
}

impl Rule for UkrainianSimpleReplaceRule {
    fn id(&self) -> &str {
        match self.flavor {
            ReplaceFlavor::Plain => "UK_SIMPLE_REPLACE",
            ReplaceFlavor::Soft => "UK_SIMPLE_REPLACE_SOFT",
        }
    }

    fn description(&self) -> &str {
        match self.flavor {
            ReplaceFlavor::Plain => "Пошук помилкових слів",
            ReplaceFlavor::Soft => "Пошук нерекомендованих слів",
        }
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Інше")
    }

    fn issue_type(&self) -> IssueType {
        match self.flavor {
            ReplaceFlavor::Plain => IssueType::Misspelling,
            ReplaceFlavor::Soft => IssueType::Style,
        }
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        for token in sentence.non_whitespace_tokens() {
            if token.is_sentence_start()
                || token.is_immunized()
                || token.is_ignored_by_speller()
                || self.is_token_exception(token)
                || self.is_tagged(token)
            {
                continue;
            }
            rule_matches.extend(self.find_matches(token, sentence));
        }
        rule_matches
    }
}

pub struct UkrainianSimpleReplaceRenamedRule {
    renamed_list: &'static HashMap<String, Vec<String>>,
}

impl UkrainianSimpleReplaceRenamedRule {
    pub fn new() -> Self {
        static MAP: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
        Self {
            renamed_list: MAP.get_or_init(|| load_renamed_lists(REPLACE_RENAMED_TXT)),
        }
    }
}

impl Default for UkrainianSimpleReplaceRenamedRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for UkrainianSimpleReplaceRenamedRule {
    fn id(&self) -> &str {
        "UK_SIMPLE_REPLACE_RENAMED"
    }

    fn description(&self) -> &str {
        "Пропозиція поточної назви для перейменованих власних назв"
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Інше")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        static GEO_POSTAG: std::sync::LazyLock<Regex> =
            std::sync::LazyLock::new(|| Regex::new("noun:inanim.*?:prop.*|adj.*").unwrap());
        let mut rule_matches = Vec::new();
        for token in sentence.non_whitespace_tokens() {
            let mut renamed_lemmas: Vec<String> = Vec::new();
            let mut overlaps_with_normal = false;
            for reading in token.readings() {
                let pos_tag = reading.pos_tags().join("+");
                if pos_tag == "SENT_END" {
                    continue;
                }
                if let Some(lemma) = reading.lemma() {
                    if self.renamed_list.contains_key(lemma)
                        && GEO_POSTAG.is_match(&pos_tag)
                        && !renamed_lemmas.iter().any(|l| l == lemma)
                    {
                        renamed_lemmas.push(lemma.to_string());
                    } else {
                        overlaps_with_normal = true;
                        break;
                    }
                }
            }
            if overlaps_with_normal || renamed_lemmas.is_empty() {
                continue;
            }
            let mut replacements = Vec::new();
            let mut info = String::new();
            for lemma in &renamed_lemmas {
                let repl = &self.renamed_list[lemma];
                if let Some(first) = repl.first() {
                    replacements.push(first.clone());
                }
                for extra in repl.iter().take(repl.len().saturating_sub(1)).skip(1) {
                    replacements.push(extra.clone());
                }
                if info.is_empty() && repl.len() > 1 {
                    info = repl.last().cloned().unwrap_or_default();
                }
            }
            let mut msg = format!("«{}» було перейменовано", renamed_lemmas[0]);
            if !info.is_empty() {
                msg += &format!(" ({info})");
            }
            let start = token.token().start();
            let end = start + token.token().token().len();
            let rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str());
            rule_matches.push(
                RuleMatch::new(
                    msg,
                    start,
                    end - start,
                    rule,
                    context_for(sentence, start, end),
                )
                .with_short_message("Перейменована назва".to_string())
                .with_replacements(
                    replacements
                        .into_iter()
                        .map(SuggestedReplacement::new)
                        .collect(),
                )
                .with_sentence(sentence.text().to_string()),
            );
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut pos = 0usize;
        for word in text.split_whitespace() {
            let start = pos + text[pos..].find(word).expect("word offset");
            let end = start + word.len();
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                word, start, end,
            )));
            pos = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    fn make_lemma_sentence(pairs: &[(&str, &str, &str)]) -> AnalyzedSentence {
        let text = pairs
            .iter()
            .map(|(t, _, _)| *t)
            .collect::<Vec<_>>()
            .join(" ");
        let mut sentence = AnalyzedSentence::new(text.clone(), 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut pos = 0usize;
        for (token, lemma, pos_tag) in pairs {
            let start = pos + text[pos..].find(token).expect("token offset");
            let end = start + token.len();
            let reading = AnalyzedToken::new(*token, start, end)
                .with_lemma(*lemma)
                .with_pos_tags(vec![pos_tag.to_string()]);
            tokens.push(
                AnalyzedTokenReadings::new(AnalyzedToken::new(*token, start, end))
                    .with_readings(vec![reading]),
            );
            pos = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    fn suggestion_values(m: &og_core::RuleMatch) -> Vec<&str> {
        m.replacements().iter().map(|r| r.value()).collect()
    }

    #[test]
    fn java_pin_plain_rule_battery() {
        let rule = UkrainianSimpleReplaceRule::plain();
        assert_eq!(
            rule.match_sentence(&make_sentence("Ці рядки повинні збігатися."))
                .len(),
            0
        );
        let text = "Ці рядки повинні співпадати";
        let matches = rule.match_sentence(&make_sentence(text));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            suggestion_values(&matches[0]),
            vec!["збігатися", "сходитися"]
        );
        assert_eq!(
            matches[0].message(),
            "«співпадати» - помилкове слово, виправлення: збігатися, сходитися."
        );
        assert_eq!(matches[0].offset(), text.find("співпадати").unwrap());
        let matches = rule.match_sentence(&make_sentence("Нападаючий"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            suggestion_values(&matches[0]),
            vec!["Нападник", "Нападальний", "Нападний"]
        );
        let matches = rule.match_sentence(&make_sentence("щедрота"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            suggestion_values(&matches[0]),
            vec!["щедрість", "гойність", "щедриня"]
        );
        assert_eq!(rule.match_sentence(&make_sentence("щедроти")).len(), 0);
    }

    #[test]
    fn java_pin_derivat_suggestions() {
        let matches =
            UkrainianSimpleReplaceRule::plain().match_sentence(&make_sentence("перелиставши"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            suggestion_values(&matches[0]),
            vec!["перегорнувши", "прогортаючи"]
        );
    }

    #[test]
    fn derivat_fallback_maps_the_base_replacement_to_a_matching_key() {
        let matches =
            UkrainianSimpleReplaceRule::plain().match_sentence(&make_sentence("аукнувшись"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message(), "Неправильне слово.");
        assert_eq!(matches[0].short_message(), Some("Помилка?"));
        assert_eq!(suggestion_values(&matches[0]), vec!["відгукнувшись"]);
    }

    #[test]
    fn plain_rule_skips_verb_tagged_but_not_subst_tagged_tokens() {
        let rule = UkrainianSimpleReplaceRule::plain();
        let verb_tagged = make_lemma_sentence(&[("співпадати", "співпадати", "verb:inf")]);
        assert_eq!(rule.match_sentence(&verb_tagged).len(), 0);
        let subst_tagged = make_lemma_sentence(&[("співпадати", "співпадати", "noun:subst")]);
        assert_eq!(rule.match_sentence(&subst_tagged).len(), 1);
    }

    #[test]
    fn java_pin_soft_rule_battery() {
        let rule = UkrainianSimpleReplaceRule::soft();
        assert_eq!(
            rule.match_sentence(&make_sentence("Ці рядки повинні збігатися."))
                .len(),
            0
        );
        assert_eq!(rule.match_sentence(&make_sentence("завидна")).len(), 0);
        let matches = rule.match_sentence(&make_sentence("Цей брелок"));
        assert_eq!(matches.len(), 1);
        assert_eq!(suggestion_values(&matches[0]), vec!["дармовис"]);
    }

    #[test]
    fn soft_rule_strips_the_ctx_marker_from_suggestions_like_java() {
        let matches =
            UkrainianSimpleReplaceRule::soft().match_sentence(&make_sentence("Не знайде спасіння"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            suggestion_values(&matches[0]),
            vec!["рятування", "рятунок", "порятунок", "визволення"]
        );
        assert!(matches[0].message().contains(": релігія"));
        assert_eq!(
            matches[0].message(),
            "«спасіння» вживається лише в таких контекстах: релігія, поезія, можливо, ви мали на увазі: рятування, рятунок, порятунок, визволення?"
        );
    }

    #[test]
    fn soft_rule_message_without_context_uses_the_recommended_form() {
        let matches =
            UkrainianSimpleReplaceRule::soft().match_sentence(&make_sentence("Цей брелок"));
        assert_eq!(
            matches[0].message(),
            "«брелок» — нерекомендоване слово, кращий варіант: дармовис."
        );
        assert_eq!(matches[0].rule().id(), "UK_SIMPLE_REPLACE_SOFT");
    }

    #[test]
    fn renamed_rule_suggests_the_current_toponym() {
        let sentence = make_lemma_sentence(&[("Аврора", "Аврора", "noun:inanim:f:prop")]);
        let matches = UkrainianSimpleReplaceRenamedRule::new().match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(suggestion_values(&matches[0]), vec!["Татиш-Конрат"]);
        assert_eq!(
            matches[0].message(),
            "«Аврора» було перейменовано (декомунізовано)"
        );
        assert_eq!(matches[0].short_message(), Some("Перейменована назва"));
    }

    #[test]
    fn renamed_rule_ignores_non_geo_or_mixed_readings() {
        let rule = UkrainianSimpleReplaceRenamedRule::new();
        let verb_reading = make_lemma_sentence(&[("Аврора", "Аврора", "verb:past")]);
        assert_eq!(rule.match_sentence(&verb_reading).len(), 0);
        let mixed = make_lemma_sentence(&[
            ("Аврора", "Аврора", "noun:inanim:f:prop"),
            ("тануть", "танути", "verb:pres"),
        ]);
        assert_eq!(rule.match_sentence(&mixed).len(), 1);
        let overlapping = make_lemma_sentence(&[("Аврора", "аврора", "noun:anim:f:prop")]);
        assert_eq!(rule.match_sentence(&overlapping).len(), 0);
    }
}
