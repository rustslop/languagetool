use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use std::collections::HashSet;

const MAX_TERMS: usize = 5;

#[derive(Default)]
struct CompoundRuleData {
    incorrect_compounds: HashSet<String>,
    joined_suggestion: HashSet<String>,
    dash_suggestion: HashSet<String>,
    joined_lower_case_suggestion: HashSet<String>,
}

impl CompoundRuleData {
    fn load(&mut self, data: &str) {
        for raw in data.lines() {
            if raw.is_empty() || raw.starts_with('#') {
                continue;
            }
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut line = line.replace('-', " ");
            if let Some(stripped) = line.strip_suffix('+') {
                line = stripped.to_string();
                self.joined_suggestion.insert(line.clone());
            } else if let Some(stripped) = line.strip_suffix('*') {
                line = stripped.to_string();
                self.dash_suggestion.insert(line.clone());
            } else if let Some(stripped) = line.strip_suffix('?') {
                line = stripped.to_string();
                self.joined_suggestion.insert(line.clone());
                self.joined_lower_case_suggestion.insert(line.clone());
            } else if let Some(stripped) = line.strip_suffix('$') {
                line = stripped.to_string();
                self.joined_suggestion.insert(line.clone());
                self.dash_suggestion.insert(line.clone());
                self.joined_lower_case_suggestion.insert(line.clone());
            } else {
                self.joined_suggestion.insert(line.clone());
                self.dash_suggestion.insert(line.clone());
            }
            self.incorrect_compounds.insert(line);
        }
    }
}

pub struct RomanianCompoundRule {
    id: String,
    description: String,
    with_hyphen_msg: String,
    without_hyphen_msg: String,
    with_or_without_hyphen_msg: String,
    short_desc: Option<String>,
    data: CompoundRuleData,
}

impl RomanianCompoundRule {
    pub fn new(
        id: &str,
        description: &str,
        with_hyphen_msg: &str,
        without_hyphen_msg: &str,
        with_or_without_hyphen_msg: &str,
        short_desc: Option<&str>,
        data: &str,
    ) -> Self {
        let mut rule = Self {
            id: id.to_string(),
            description: description.to_string(),
            with_hyphen_msg: with_hyphen_msg.to_string(),
            without_hyphen_msg: without_hyphen_msg.to_string(),
            with_or_without_hyphen_msg: with_or_without_hyphen_msg.to_string(),
            short_desc: short_desc.map(str::to_string),
            data: CompoundRuleData::default(),
        };
        rule.data.load(data);
        rule
    }

    fn merge_compound(&self, s: &str, uncapitalize_mid_words: bool) -> String {
        let replaced = s.replace('-', " ");
        let parts: Vec<&str> = replaced.split(' ').collect();
        let mut sb = String::new();
        for (k, part) in parts.iter().enumerate() {
            if k == 0 {
                sb.push_str(parts[0]);
            } else {
                let mut part = (*part).to_string();
                if uncapitalize_mid_words {
                    part = uncapitalize_first(&part);
                }
                sb.push_str(&part);
            }
        }
        sb
    }
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
    let mut str = in_str.trim().to_string();
    str = str.replace(" - ", " ");
    str = str.replace('-', " ");
    while str.contains("  ") {
        str = str.replace("  ", " ");
    }
    str
}

impl Rule for RomanianCompoundRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
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
        let mut prev_tokens: Vec<&AnalyzedTokenReadings> = Vec::with_capacity(MAX_TERMS);
        let mut prev_ws_tokens: Vec<bool> = Vec::with_capacity(MAX_TERMS);
        let pad_start = tokens.last().map(|t| t.token().start()).unwrap_or(0);
        let empty_padding =
            AnalyzedTokenReadings::new(og_core::AnalyzedToken::new("", pad_start, pad_start));
        for i in 0..tokens.len() + MAX_TERMS {
            let token: &AnalyzedTokenReadings = if i >= tokens.len() {
                &empty_padding
            } else {
                tokens[i]
            };
            if i == 0 {
                push_queue((token, false), &mut prev_tokens, &mut prev_ws_tokens);
                continue;
            }
            if token.is_immunized() {
                continue;
            }
            let mut strings_to_check: Vec<String> = Vec::with_capacity(MAX_TERMS);
            let mut orig_strings_to_check: Vec<String> = Vec::with_capacity(MAX_TERMS);
            let mut string_to_token: Vec<(String, usize)> = Vec::with_capacity(MAX_TERMS);
            {
                let mut sb = String::new();
                let mut first_is_sent_start = false;
                for (j, atr) in prev_tokens.iter().enumerate() {
                    if prev_ws_tokens[j] {
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
                            string_to_token
                                .push((string_to_check.clone(), j + i - prev_tokens.len()));
                        }
                        strings_to_check.push(string_to_check);
                        orig_strings_to_check.push(sb.trim().to_string());
                    }
                }
            }
            for k in (0..strings_to_check.len()).rev() {
                let string_to_check = &strings_to_check[k];
                let orig_string_to_check = &orig_strings_to_check[k];
                if !self.data.incorrect_compounds.contains(string_to_check) {
                    continue;
                }
                let end_token_idx = string_to_token
                    .iter()
                    .find(|(key, _)| key == string_to_check)
                    .map(|(_, idx)| *idx)
                    .unwrap_or(i - 1);
                let atr = tokens.get(end_token_idx).copied().unwrap_or(token);
                let mut msg: Option<&String> = None;
                let mut replacement: Vec<String> = Vec::new();
                if self.data.dash_suggestion.contains(string_to_check)
                    && !orig_string_to_check.contains(' ')
                {
                    break;
                }
                if self.data.dash_suggestion.contains(string_to_check) {
                    replacement.push(orig_string_to_check.replace(' ', "-"));
                    msg = Some(&self.with_hyphen_msg);
                }
                if is_not_all_uppercase(orig_string_to_check)
                    && self.data.joined_suggestion.contains(string_to_check)
                {
                    let uncapitalize = self
                        .data
                        .joined_lower_case_suggestion
                        .iter()
                        .any(|s| string_to_check.contains(s.as_str()));
                    replacement.push(self.merge_compound(orig_string_to_check, uncapitalize));
                    msg = Some(&self.without_hyphen_msg);
                }
                let parts: Vec<&str> = string_to_check.split(' ').collect();
                if let Some(first) = parts.first()
                    && first.chars().count() == 1
                {
                    replacement.clear();
                    replacement.push(orig_string_to_check.replace(' ', "-"));
                    msg = Some(&self.with_hyphen_msg);
                } else if replacement.is_empty() || replacement.len() == 2 {
                    msg = Some(&self.with_or_without_hyphen_msg);
                }
                let first_token = prev_tokens[0];
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
                    .filter(|r| r != original_span)
                    .collect();
                if filtered.is_empty() {
                    break;
                }
                let start_pos = first_token.token().start();
                let end_pos = atr.token().end();
                let message = msg.map_or(self.with_or_without_hyphen_msg.clone(), Clone::clone);
                let rm_rule = RuleMatchRule::new(&self.id, &self.description)
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                let mut ctx_start = start_pos.saturating_sub(40);
                while ctx_start > 0 && !sentence.text().is_char_boundary(ctx_start) {
                    ctx_start -= 1;
                }
                let mut ctx_end = std::cmp::min(end_pos + 40, sentence.text().len());
                while ctx_end < sentence.text().len() && !sentence.text().is_char_boundary(ctx_end)
                {
                    ctx_end += 1;
                }
                let context_text = sentence.text()[ctx_start..ctx_end].to_string();
                let mut rule_match = RuleMatch::new(
                    message,
                    start_pos,
                    end_pos - start_pos,
                    rm_rule,
                    RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                )
                .with_replacements(filtered.iter().map(SuggestedReplacement::new).collect())
                .with_sentence(sentence.text().to_string());
                if let Some(short) = &self.short_desc {
                    rule_match = rule_match.with_short_message(short.clone());
                }
                if prev_from == Some(start_pos) {
                    break;
                }
                prev_from = Some(start_pos);
                rule_matches.push(rule_match);
                break;
            }
            push_queue(
                (token, ws_before.get(i).copied().unwrap_or(false)),
                &mut prev_tokens,
                &mut prev_ws_tokens,
            );
        }
        rule_matches
    }
}

fn push_queue<'a>(
    token: (&'a AnalyzedTokenReadings, bool),
    prev_tokens: &mut Vec<&'a AnalyzedTokenReadings>,
    prev_ws: &mut Vec<bool>,
) {
    if prev_tokens.len() == MAX_TERMS {
        prev_tokens.remove(0);
        prev_ws.remove(0);
    }
    prev_tokens.push(token.0);
    prev_ws.push(token.1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ro::example_suite::ro_pipeline;

    fn matches_for(text: &str) -> Vec<(String, usize, usize)> {
        let sentences = ro_pipeline().analyze_text(text);
        let rule = super::super::romanian_compound_rule();
        let mut out = Vec::new();
        for sentence in &sentences {
            for m in rule.match_sentence(sentence) {
                let repl: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
                out.push((
                    repl.first().copied().unwrap_or("").to_string(),
                    m.offset(),
                    m.length(),
                ));
            }
        }
        out
    }

    #[test]
    fn compound_rule_test_corpus() {
        assert!(
            matches_for("Au plecat câteșitrei.").is_empty(),
            "already-joined spelling is correct (CompoundRuleTest.java:35)"
        );
        assert_eq!(
            matches_for("câte și trei"),
            vec![("câteșitrei".to_string(), 0, "câte și trei".len())]
        );
        assert_eq!(
            matches_for("Câte și trei"),
            vec![("Câteșitrei".to_string(), 0, "Câte și trei".len())],
            "mergeCompound keeps the original capitalization of every part"
        );
        assert_eq!(
            matches_for("câte-și-trei"),
            vec![("câteșitrei".to_string(), 0, "câte-și-trei".len())],
            "the hyphenated spelling normalizes to the same key, so it flags too"
        );
        assert_eq!(
            matches_for("tus trei"),
            vec![("tustrei".to_string(), 0, "tus trei".len())]
        );
        assert_eq!(
            matches_for("tus-trei"),
            vec![("tustrei".to_string(), 0, "tus-trei".len())]
        );
    }
}
