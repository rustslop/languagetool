use super::native_rules::context_around;
use super::tagger::CatalanTagger;
use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchRule,
    SuggestedReplacement,
};
use std::collections::HashSet;
use std::sync::Arc;

const MAX_TERMS: usize = 5;

const WITH_HYPHEN_MESSAGE: &str = "S'escriu amb un guionet.";
const WITHOUT_HYPHEN_MESSAGE: &str = "S'escriu junt sense espai ni guionet.";
const WITH_OR_WITHOUT_HYPHEN_MESSAGE: &str = "S'escriu junt o amb guionet.";
const SHORT_DESCRIPTION: &str = "Error de mot compost";

#[derive(Default)]
struct CompoundRuleData {
    incorrect_compounds: HashSet<String>,
    joined_suggestion: HashSet<String>,
    dash_suggestion: HashSet<String>,
    joined_lower_case_suggestion: HashSet<String>,
    has_digit_patterns: bool,
}

impl CompoundRuleData {
    fn load(data: &str) -> Self {
        let mut rule = Self::default();
        for raw_line in data.lines() {
            if raw_line.is_empty() || raw_line.starts_with('#') {
                continue;
            }
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut line = line.replace('-', " ");
            if let Some(stripped) = line.strip_suffix('+') {
                line = stripped.to_string();
                rule.joined_suggestion.insert(line.clone());
            } else if let Some(stripped) = line.strip_suffix('*') {
                line = stripped.to_string();
                rule.dash_suggestion.insert(line.clone());
            } else if let Some(stripped) = line.strip_suffix('?') {
                line = stripped.to_string();
                rule.joined_suggestion.insert(line.clone());
                rule.joined_lower_case_suggestion.insert(line.clone());
            } else if let Some(stripped) = line.strip_suffix('$') {
                line = stripped.to_string();
                rule.joined_suggestion.insert(line.clone());
                rule.dash_suggestion.insert(line.clone());
                rule.joined_lower_case_suggestion.insert(line.clone());
            } else {
                rule.joined_suggestion.insert(line.clone());
                rule.dash_suggestion.insert(line.clone());
            }
            if line.contains("\\d") {
                rule.has_digit_patterns = true;
            }
            rule.incorrect_compounds.insert(line);
        }
        rule
    }
}

pub struct CompoundRule {
    data: CompoundRuleData,
    tagger: Option<Arc<CatalanTagger>>,
}

fn uncapitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_lowercase(), chars.as_str()),
        None => s.to_string(),
    }
}

fn is_not_all_uppercase(s: &str) -> bool {
    s.split(' ')
        .all(|part| part == "-" || !part.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()))
}

fn normalize(in_str: &str) -> String {
    let mut s = in_str.trim().to_string();
    s = s.replace(" - ", " ");
    s = s.replace('-', " ");
    while s.contains("  ") {
        s = s.replace("  ", " ");
    }
    s
}

fn to_id(input: &str) -> String {
    let upper = input
        .trim()
        .to_uppercase()
        .replace(' ', "_")
        .replace('\'', "_Q_");
    let mut out = String::with_capacity(upper.len());
    for c in upper.chars() {
        if c.is_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

fn merge_compound(s: &str, uncapitalize_mid_words: bool) -> String {
    let replaced = s.replace('-', " ");
    let parts: Vec<&str> = replaced.split(' ').collect();
    let mut sb = String::new();
    for (k, part) in parts.iter().enumerate() {
        if k == 0 {
            sb.push_str(parts[0]);
        } else if uncapitalize_mid_words {
            sb.push_str(&uncapitalize_first(part));
        } else {
            sb.push_str(part);
        }
    }
    sb
}

impl CompoundRule {
    pub fn new() -> Self {
        Self::from_data(include_str!(
            "../../../../languagetool/languagetool-language-modules/ca/src/main/resources/org/languagetool/resource/ca/compounds.txt"
        ))
    }

    pub fn from_data(data: &str) -> Self {
        Self {
            data: CompoundRuleData::load(data),
            tagger: CatalanTagger::shared_arc(),
        }
    }

    fn is_misspelled(&self, word: &str) -> bool {
        match &self.tagger {
            Some(tagger) => !tagger.is_word_tagged(word),
            None => true,
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
        "CA_COMPOUNDS"
    }

    fn description(&self) -> &str {
        "Paraules compostes amb guionet: $match"
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscel\u{b7}l\u{00e0}nia")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let all_tokens = sentence.tokens();
        let mut tokens: Vec<&AnalyzedTokenReadings> = Vec::with_capacity(all_tokens.len());
        let mut ws_before: Vec<bool> = Vec::with_capacity(all_tokens.len());
        let mut prev_ws = false;
        for t in all_tokens {
            if t.is_whitespace() {
                prev_ws = true;
                continue;
            }
            tokens.push(t);
            ws_before.push(prev_ws);
            prev_ws = false;
        }
        let mut rule_matches: Vec<RuleMatch> = Vec::new();
        let mut prev_from: Option<usize> = None;
        let mut prev_tokens: Vec<Option<usize>> = Vec::with_capacity(MAX_TERMS);
        for i in 0..tokens.len() + MAX_TERMS {
            let token: Option<&AnalyzedTokenReadings> = tokens.get(i).copied();
            if i == 0 {
                push_queue((i < tokens.len()).then_some(i), &mut prev_tokens);
                continue;
            }
            if let Some(token) = token
                && token.is_immunized()
            {
                continue;
            }
            let mut strings_to_check: Vec<String> = Vec::with_capacity(MAX_TERMS);
            let mut orig_strings_to_check: Vec<String> = Vec::with_capacity(MAX_TERMS);
            let mut string_to_token: Vec<(String, usize)> = Vec::with_capacity(MAX_TERMS);
            {
                let mut sb = String::new();
                let mut first_is_sent_start = false;
                for (j, atr_idx) in prev_tokens.iter().enumerate() {
                    let Some(atr_idx) = atr_idx else {
                        continue;
                    };
                    let atr = tokens[*atr_idx];
                    if j > 0 && ws_before[*atr_idx] {
                        sb.push(' ');
                    }
                    if !atr.is_sentence_start() {
                        sb.push_str(atr.token().token());
                    }
                    if j == 0 {
                        first_is_sent_start = atr.is_sentence_start();
                    }
                    if j >= 1 || !first_is_sent_start {
                        let mut string_to_check = normalize(&sb);
                        if first_is_sent_start {
                            string_to_check = uncapitalize_first(&string_to_check);
                        }
                        if !string_to_token.iter().any(|(k, _)| *k == string_to_check) {
                            string_to_token.push((string_to_check.clone(), *atr_idx));
                        }
                        strings_to_check.push(string_to_check);
                        orig_strings_to_check.push(sb.trim().to_string());
                    }
                }
            }
            for k in (0..strings_to_check.len()).rev() {
                let string_to_check = &strings_to_check[k];
                let orig_string_to_check = &orig_strings_to_check[k];
                let contains_digits = self.data.has_digit_patterns
                    && string_to_check
                        .split(' ')
                        .any(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()));
                let digits_regexp = string_to_check.replace(|c: char| c.is_ascii_digit(), "\\d+");
                let known = self.data.incorrect_compounds.contains(string_to_check)
                    || (contains_digits && self.data.incorrect_compounds.contains(&digits_regexp));
                if !known {
                    continue;
                }
                let end_token_idx = string_to_token
                    .iter()
                    .find(|(key, _)| key == string_to_check)
                    .map(|(_, idx)| *idx)
                    .unwrap_or(i - 1);
                let atr = tokens.get(end_token_idx).copied().or(token);
                let mut msg: Option<&str> = None;
                let mut replacement: Vec<String> = Vec::new();
                if self.data.dash_suggestion.contains(string_to_check)
                    && !orig_string_to_check.contains(' ')
                {
                    break;
                }
                if self.data.dash_suggestion.contains(string_to_check)
                    || (contains_digits && self.data.incorrect_compounds.contains(&digits_regexp))
                {
                    replacement.push(orig_string_to_check.replace(' ', "-"));
                    msg = Some(WITH_HYPHEN_MESSAGE);
                }
                if is_not_all_uppercase(orig_string_to_check)
                    && self.data.joined_suggestion.contains(string_to_check)
                {
                    let uncapitalize = self
                        .data
                        .joined_lower_case_suggestion
                        .iter()
                        .any(|s| string_to_check.contains(s.as_str()));
                    replacement.push(merge_compound(orig_string_to_check, uncapitalize));
                    msg = Some(WITHOUT_HYPHEN_MESSAGE);
                }
                let parts: Vec<&str> = string_to_check.split(' ').collect();
                if parts
                    .first()
                    .is_some_and(|first| first.chars().count() == 1)
                {
                    replacement.clear();
                    replacement.push(orig_string_to_check.replace(' ', "-"));
                    msg = Some(WITH_HYPHEN_MESSAGE);
                } else if replacement.is_empty() || replacement.len() == 2 {
                    msg = Some(WITH_OR_WITHOUT_HYPHEN_MESSAGE);
                }
                let first_token = prev_tokens
                    .iter()
                    .flatten()
                    .copied()
                    .find_map(|idx| tokens.get(idx).copied());
                let (Some(first_token), Some(atr)) = (first_token, atr) else {
                    break;
                };
                let original_span =
                    &sentence.text()[first_token.token().start()..atr.token().end()];
                let filtered: Vec<String> = replacement
                    .iter()
                    .map(|r| {
                        let mut r = r.clone();
                        while r.contains("--") {
                            r = r.replace("--", "-");
                        }
                        r
                    })
                    .filter(|r| r != original_span && !self.is_misspelled(r))
                    .collect();
                if filtered.is_empty() {
                    break;
                }
                let start_pos = first_token.token().start();
                let end_pos = atr.token().end();
                let message = msg.unwrap_or(WITH_OR_WITHOUT_HYPHEN_MESSAGE);
                let sub_id = to_id(&format!("{}_{}", self.id(), string_to_check));
                let description = self.description().replace("$match", orig_string_to_check);
                let rm_rule = RuleMatchRule::new(sub_id, description)
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                let rule_match = RuleMatch::new(
                    message,
                    start_pos,
                    end_pos - start_pos,
                    rm_rule,
                    context_around(sentence, start_pos, end_pos, 30),
                )
                .with_short_message(SHORT_DESCRIPTION)
                .with_replacements(
                    filtered
                        .iter()
                        .map(|r| SuggestedReplacement::new(r.clone()))
                        .collect(),
                );
                if prev_from == Some(start_pos) {
                    break;
                }
                prev_from = Some(start_pos);
                rule_matches.push(rule_match);
                break;
            }
            push_queue((i < tokens.len()).then_some(i), &mut prev_tokens);
        }
        rule_matches
    }
}

fn push_queue(token_idx: Option<usize>, prev_tokens: &mut Vec<Option<usize>>) {
    if prev_tokens.len() == MAX_TERMS {
        prev_tokens.remove(0);
    }
    prev_tokens.push(token_idx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::AnalyzedToken;

    fn sentence_from_words(words: &[&str]) -> AnalyzedSentence {
        let text = words.join(" ");
        let mut s = AnalyzedSentence::new(&text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0usize;
        let mut first = true;
        for word in words {
            if !first {
                let sp = AnalyzedTokenReadings::new(AnalyzedToken::new(" ", offset, offset + 1));
                tokens.push(sp);
                offset += 1;
            }
            first = false;
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                *word,
                offset,
                offset + word.len(),
            )));
            offset += word.len();
        }
        s.set_tokens(tokens);
        s
    }

    #[test]
    fn loads_the_java_compound_list() {
        let rule = CompoundRule::new();
        assert!(
            rule.data.incorrect_compounds.len() > 1000,
            "compounds.txt must load, got {}",
            rule.data.incorrect_compounds.len()
        );
        assert!(rule.data.joined_suggestion.contains("blau grana"));
        assert!(rule.data.dash_suggestion.contains("Iang Ts\u{e9}"));
        assert!(!rule.data.dash_suggestion.contains("blau grana"));
    }

    #[test]
    fn joined_suggestion_fires_when_the_merged_form_is_tagged() {
        let rule = CompoundRule::new();
        let sentence = sentence_from_words(&["blau", "grana"]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "our-data outcome over the added.txt dict");
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "blaugrana"),
            "expected blaugrana suggestion, got {:?}",
            matches[0].replacements()
        );
    }

    #[test]
    fn dash_suggestion_hyphenates_iang_tse() {
        let rule = CompoundRule::new();
        let sentence = sentence_from_words(&["Iang", "Tsé"]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "our-data outcome over the added.txt dict");
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "Iang-Tsé"),
            "expected Iang-Tsé suggestion, got {:?}",
            matches[0].replacements()
        );
    }

    #[test]
    fn already_hyphenated_form_does_not_match() {
        let rule = CompoundRule::new();
        let text = "Iang-Tsé";
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
            text,
            0,
            text.len(),
        )));
        let mut s = AnalyzedSentence::new(text, 0, text.len());
        s.set_tokens(tokens);
        let matches = rule.match_sentence(&s);
        assert!(
            matches.is_empty(),
            "the correct hyphenated compound must stay silent: {:?}",
            matches
                .iter()
                .map(|m| m
                    .replacements()
                    .iter()
                    .map(|r| r.value())
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn sub_rule_ids_follow_the_java_toid_scheme() {
        assert_eq!(to_id("CA_COMPOUNDS_Iang Tsé"), "CA_COMPOUNDS_IANG_TSÉ");
    }
}
