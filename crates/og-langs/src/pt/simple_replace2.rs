use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use std::collections::HashMap;

const MAX_TOKENS_IN_MULTIWORD: usize = 20;

struct Suggestion {
    suggestion: String,
    message: Option<String>,
}

pub struct SimpleReplace2Rule {
    id: String,
    description: String,
    short: String,
    message: String,
    separator: String,
    category: Category,
    issue_type: IssueType,
    default_on: bool,
    sub_rule_ids: bool,
    url: Option<String>,
    skip_proper_nouns: bool,
    start_space: HashMap<String, usize>,
    start_no_space: HashMap<String, usize>,
    full_space: HashMap<String, Suggestion>,
    full_no_space: HashMap<String, Suggestion>,
}

struct MatchSpan<'a> {
    original: &'a str,
    from: usize,
    to: usize,
    sent_start: usize,
    start_index: usize,
}

impl SimpleReplace2Rule {
    pub fn new(id: &str, description: &str, short: &str, message: &str) -> Self {
        Self {
            id: id.to_string(),
            description: description.to_string(),
            short: short.to_string(),
            message: message.to_string(),
            separator: ", ".to_string(),
            category: Category::new("MISC", "Miscellaneous"),
            issue_type: IssueType::Other("localeViolation".to_string()),
            default_on: true,
            sub_rule_ids: false,
            url: None,
            skip_proper_nouns: false,
            start_space: HashMap::new(),
            start_no_space: HashMap::new(),
            full_space: HashMap::new(),
            full_no_space: HashMap::new(),
        }
    }

    pub fn with_separator(mut self, separator: &str) -> Self {
        self.separator = separator.to_string();
        self
    }

    pub fn with_category(mut self, category: Category) -> Self {
        self.category = category;
        self
    }

    pub fn with_issue_type(mut self, issue_type: IssueType) -> Self {
        self.issue_type = issue_type;
        self
    }

    pub fn with_default_off(mut self) -> Self {
        self.default_on = false;
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

    pub fn with_proper_noun_exceptions(mut self) -> Self {
        self.skip_proper_nouns = true;
        self
    }

    pub fn load_data(&mut self, data: &str) {
        for raw_line in data.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut tab_parts = line.splitn(2, '\t');
            let conf_pair = tab_parts.next().unwrap_or("");
            let msg = tab_parts.next().map(str::to_string);
            let mut pair_parts = conf_pair.splitn(2, '=');
            let wrong_side = pair_parts.next().unwrap_or("");
            let suggestion = pair_parts.next().unwrap_or("").to_string();
            for wrong_form in wrong_side.split('|') {
                let search_key = wrong_form.to_lowercase();
                let sug = Suggestion {
                    suggestion: suggestion.clone(),
                    message: msg.clone(),
                };
                if let Some(space) = search_key.find(' ')
                    && space > 0
                {
                    let first_token = search_key[..space].to_string();
                    let tokens = search_key.split(' ').count();
                    let entry = self.start_space.entry(first_token).or_insert(0);
                    if *entry < tokens {
                        *entry = tokens;
                    }
                    self.full_space.insert(search_key, sug);
                } else {
                    let first_char = search_key.chars().next().map(|c| c.to_string());
                    if let Some(first_char) = first_char {
                        let len = search_key.chars().count();
                        let entry = self.start_no_space.entry(first_char).or_insert(0);
                        if *entry < len {
                            *entry = len;
                        }
                    }
                    self.full_no_space.insert(search_key, sug);
                }
            }
        }
    }

    fn is_token_exception(&self, token: &AnalyzedTokenReadings) -> bool {
        token.is_immunized()
            || (self.skip_proper_nouns && token.has_pos_tag_matching("NP.*"))
            || (self.skip_proper_nouns && token.has_pos_tag_matching("_english_ignore_.*"))
    }

    fn create_match(
        &self,
        sug: &Suggestion,
        span: MatchSpan<'_>,
        sentence: &AnalyzedSentence,
    ) -> Option<RuleMatch> {
        let MatchSpan {
            original: original_str,
            from: from_pos,
            to: to_pos,
            sent_start,
            start_index,
        } = span;
        let replacements: Vec<&str> = sug.suggestion.split('|').collect();
        let first_word_in_sugg_is_camel_case = replacements
            .iter()
            .any(|k| is_camel_case(k.split(' ').next().unwrap_or("")));
        let is_all_uppercase = is_all_uppercase(original_str);
        let is_capitalized = is_capitalized_word(original_str.split(' ').next().unwrap_or(""));
        let mut final_replacements: Vec<String> = Vec::new();
        for repl in &replacements {
            let mut final_repl = (*repl).to_string();
            if !first_word_in_sugg_is_camel_case
                && (sent_start == start_index || (is_capitalized && !self.checking_case()))
            {
                final_repl = uppercase_first_char(repl);
            }
            if !self.checking_case() && is_all_uppercase {
                final_repl = repl.to_uppercase();
            }
            if final_repl == original_str {
                final_replacements.clear();
                break;
            }
            if repl != &original_str && !final_replacements.contains(&final_repl) {
                final_replacements.push(final_repl);
            }
        }
        if final_replacements.is_empty() {
            return None;
        }
        let mut msg = sug.message.clone();
        if msg
            .as_deref()
            .is_some_and(|m| m.starts_with("http://") || m.starts_with("https://"))
        {
            msg = None;
        }
        let message = msg.unwrap_or_else(|| {
            let mut msg_suggestions = String::new();
            let last = replacements.len().saturating_sub(1);
            for (k, sugg) in replacements.iter().enumerate() {
                if k > 0 {
                    msg_suggestions.push_str(if k == last { &self.separator } else { ", " });
                }
                msg_suggestions.push_str(&format!("<suggestion>{sugg}</suggestion>"));
            }
            self.message.replacen("$match", original_str, 1).replacen(
                "$suggestions",
                &msg_suggestions,
                1,
            )
        });
        let mut rm_rule = RuleMatchRule::new(&self.id, &self.description)
            .with_category(self.category.clone())
            .with_issue_type(self.issue_type.as_str());
        if self.sub_rule_ids {
            let id = to_id(&format!("{}_{}", self.id, original_str));
            let desc = self.description.replace("$match", original_str);
            rm_rule = RuleMatchRule::new(id, desc)
                .with_category(self.category.clone())
                .with_issue_type(self.issue_type.as_str());
        }
        if let Some(url) = &self.url {
            rm_rule = rm_rule.with_urls(vec![url.clone()]);
        }
        let mut ctx_start = from_pos.saturating_sub(40);
        while ctx_start > 0 && !sentence.text().is_char_boundary(ctx_start) {
            ctx_start -= 1;
        }
        let mut ctx_end = std::cmp::min(to_pos + 40, sentence.text().len());
        while ctx_end < sentence.text().len() && !sentence.text().is_char_boundary(ctx_end) {
            ctx_end += 1;
        }
        let context_text = sentence.text()[ctx_start..ctx_end].to_string();
        Some(
            RuleMatch::new(
                message,
                from_pos,
                to_pos - from_pos,
                rm_rule,
                RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
            )
            .with_short_message(self.short.clone())
            .with_replacements(
                final_replacements
                    .iter()
                    .map(SuggestedReplacement::new)
                    .collect(),
            )
            .with_sentence(sentence.text().to_string()),
        )
    }

    const fn checking_case(&self) -> bool {
        false
    }
}

impl Rule for SimpleReplace2Rule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn is_default_on(&self) -> bool {
        self.default_on
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        self.issue_type.clone()
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
        let mut sent_start = 0usize;
        while sent_start < tokens.len()
            && (tokens[sent_start].is_sentence_start()
                || is_punctuation_start(tokens[sent_start].token().token()))
        {
            sent_start += 1;
        }
        let mut rule_matches: Vec<RuleMatch> = Vec::new();
        for start_index in sent_start..tokens.len() {
            if self.is_token_exception(tokens[start_index]) {
                continue;
            }
            let first = tokens[start_index].token().token();
            if first.is_empty() {
                continue;
            }
            let mut tok = first.to_string();
            let mut k = start_index + 1;
            while k < tokens.len() && !ws_before[k] {
                tok.push_str(tokens[k].token().token());
                k += 1;
            }
            let tok_lower = tok.to_lowercase();
            if let Some(&max_token_len) = self.start_space.get(&tok_lower) {
                let mut key_builder = String::new();
                let mut end_index = start_index;
                while end_index < tokens.len() && end_index - start_index < MAX_TOKENS_IN_MULTIWORD
                {
                    if end_index > start_index && ws_before[end_index] {
                        key_builder.push(' ');
                    }
                    key_builder.push_str(tokens[end_index].token().token());
                    let original_str = key_builder.clone();
                    let number_of_spaces = original_str.matches(' ').count();
                    if number_of_spaces + 1 > max_token_len {
                        break;
                    }
                    if number_of_spaces > 0 {
                        let key_str = original_str.to_lowercase();
                        if let Some(sug) = self.full_space.get(&key_str)
                            && let Some(m) = self
                                .create_match(
                                    sug,
                                    MatchSpan {
                                        original: &original_str,
                                        from: tokens[start_index].token().start(),
                                        to: tokens[end_index].token().end(),
                                        sent_start,
                                        start_index,
                                    },
                                    sentence,
                                )
                                .filter(|m| {
                                    !covered_by_last(
                                        &rule_matches,
                                        m.offset(),
                                        m.offset() + m.length(),
                                    )
                                })
                        {
                            retain_longest(&mut rule_matches, m.offset(), m.offset() + m.length());
                            rule_matches.push(m);
                        }
                    }
                    end_index += 1;
                }
            }
            if let Some(first_char) = tok_lower.chars().next()
                && self.start_no_space.contains_key(&first_char.to_string())
            {
                let mut end_index = start_index;
                let mut key_builder = String::new();
                while end_index < tokens.len() && end_index - start_index < MAX_TOKENS_IN_MULTIWORD
                {
                    if end_index > start_index && ws_before[end_index] {
                        break;
                    }
                    key_builder.push_str(tokens[end_index].token().token());
                    let original_str = key_builder.clone();
                    let key_str = original_str.to_lowercase();
                    if let Some(sug) = self.full_no_space.get(&key_str)
                        && let Some(m) = self
                            .create_match(
                                sug,
                                MatchSpan {
                                    original: &original_str,
                                    from: tokens[start_index].token().start(),
                                    to: tokens[end_index].token().end(),
                                    sent_start,
                                    start_index,
                                },
                                sentence,
                            )
                            .filter(|m| {
                                !covered_by_last(&rule_matches, m.offset(), m.offset() + m.length())
                            })
                    {
                        retain_longest(&mut rule_matches, m.offset(), m.offset() + m.length());
                        rule_matches.push(m);
                    }
                    end_index += 1;
                }
            }
        }
        rule_matches
    }
}

fn covered_by_last(matches: &[RuleMatch], from: usize, to: usize) -> bool {
    matches
        .last()
        .is_some_and(|last| last.offset() <= from && last.offset() + last.length() >= to)
}

fn retain_longest(matches: &mut Vec<RuleMatch>, from: usize, to: usize) {
    if matches
        .last()
        .is_some_and(|last| last.offset() >= from && last.offset() + last.length() <= to)
    {
        matches.pop();
    }
}

fn to_id(input: &str) -> String {
    input
        .trim()
        .to_uppercase()
        .replace(' ', "_")
        .replace('\'', "_Q_")
}

fn is_camel_case(token: &str) -> bool {
    let mut seen_lower = false;
    for c in token.chars() {
        if c.is_lowercase() {
            seen_lower = true;
        } else if c.is_uppercase() {
            return seen_lower;
        }
    }
    false
}

fn is_all_uppercase(s: &str) -> bool {
    s.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
}

fn is_capitalized_word(s: &str) -> bool {
    match s.chars().next() {
        Some(first) if first.is_uppercase() => s
            .chars()
            .skip(1)
            .all(|c| !c.is_alphabetic() || c.is_lowercase()),
        _ => false,
    }
}

fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => s.to_string(),
    }
}

fn is_punctuation_start(word: &str) -> bool {
    word.chars().any(|c| c.is_ascii_digit()) || !word.chars().any(|c| c.is_alphanumeric())
}

pub fn barbarisms_rule(data: &'static str) -> SimpleReplace2Rule {
    let mut rule = SimpleReplace2Rule::new(
        "PT_BARBARISMS_REPLACE",
        "Palavras de origem estrangeira evitáveis: $match",
        "Estrangeirismo",
        "\"$match\" é um estrangeirismo. É preferível dizer $suggestions.",
    )
    .with_separator(" ou ")
    .with_category(Category::new("STYLE", "Style"))
    .with_issue_type(IssueType::Other("localeViolation".to_string()))
    .with_sub_rule_ids()
    .with_url("https://pt.wikipedia.org/wiki/Estrangeirismo")
    .with_proper_noun_exceptions();
    rule.load_data(data);
    rule
}

pub fn barbarisms_base() -> SimpleReplace2Rule {
    barbarisms_rule(include_str!(
        "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/barbarisms.txt"
    ))
}

pub fn portugal_replace_rule() -> SimpleReplace2Rule {
    let mut rule = SimpleReplace2Rule::new(
        "PT_PT_SIMPLE_REPLACE",
        "Brasileirismo: 1. palavras confundidas com as de Portugal",
        "Palavra de português do Brasil",
        "'$match' é uma expressão brasileira, em português de Portugal utiliza-se: $suggestions",
    )
    .with_separator(" ou ")
    .with_category(Category::new("STYLE", "Style"))
    .with_issue_type(IssueType::Other("localeViolation".to_string()))
    .with_sub_rule_ids()
    .with_proper_noun_exceptions();
    rule.load_data(include_str!("../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-PT/replace.txt"));
    rule
}

pub fn brazilian_replace_rule() -> SimpleReplace2Rule {
    let mut rule = SimpleReplace2Rule::new(
        "PT_BR_SIMPLE_REPLACE",
        "Palavras portuguesas facilmente confundidas com as do Brasil",
        "Palavra do português de Portugal",
        "\"$match\" é uma expressão usada sobretudo em Portugal. No português brasileiro diz-se $suggestions",
    )
    .with_separator(" ou ")
    .with_category(Category::new("STYLE", "Style"))
    .with_issue_type(IssueType::Other("localeViolation".to_string()))
    .with_sub_rule_ids()
    .with_proper_noun_exceptions();
    rule.load_data(include_str!("../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-BR/replace.txt"));
    rule
}

pub fn cliche_rule(id: &str, data: &'static str) -> SimpleReplace2Rule {
    let mut rule = SimpleReplace2Rule::new(
        id,
        "Frases-feitas e expressões idiomáticas: $match",
        "Frase-feita",
        "\"$match\" é uma frase-feita. É preferível dizer $suggestions.",
    )
    .with_separator(" ou ")
    .with_category(Category::new("STYLE", "Style"))
    .with_issue_type(IssueType::Style);
    rule.load_data(data);
    rule
}

pub fn redundancy_rule(id: &str, data: &'static str) -> SimpleReplace2Rule {
    let mut rule = SimpleReplace2Rule::new(
        id,
        "1. Pleonasmos e redundâncias: $match",
        "Pleonasmo",
        "\"$match\" é um pleonasmo. É preferível dizer $suggestions",
    )
    .with_separator(" ou ")
    .with_category(Category::new("REDUNDANCY", "Redundancy"))
    .with_issue_type(IssueType::Style);
    rule.load_data(data);
    rule
}

pub fn wordiness_rule(id: &str, data: &'static str) -> SimpleReplace2Rule {
    let mut rule = SimpleReplace2Rule::new(
        id,
        "2. Expressões prolixas: $match",
        "Expressão prolixa",
        "\"$match\" é uma expressão prolixa. É preferível dizer $suggestions.",
    )
    .with_separator(" ou ")
    .with_category(Category::new("REDUNDANCY", "Redundancy"))
    .with_issue_type(IssueType::Style);
    rule.load_data(data);
    rule
}

pub fn wikipedia_rule(id: &str, data: &'static str) -> SimpleReplace2Rule {
    let mut rule = SimpleReplace2Rule::new(
        id,
        "Erros frequentes nos artigos da Wikipédia: $match",
        "Erro gramatical ou de normativa",
        "Possível erro em \"$match\". Prefira $suggestions",
    )
    .with_separator(" ou ")
    .with_category(Category::new("WIKIPEDIA", "Wikipedia"))
    .with_issue_type(IssueType::Grammar);
    rule.load_data(data);
    rule
}

pub fn archaisms_rule(data: &'static str) -> SimpleReplace2Rule {
    let mut rule = SimpleReplace2Rule::new(
        "PT_ARCHAISMS_REPLACE",
        "Palavras arcaicas evitáveis",
        "Arcaísmo",
        "\"$match\" é um arcaísmo. É preferível dizer $suggestions.",
    )
    .with_separator(" ou ")
    .with_category(Category::new("STYLE", "Style"))
    .with_issue_type(IssueType::Other("localeViolation".to_string()));
    rule.load_data(data);
    rule
}

pub fn diacritics_rule() -> SimpleReplace2Rule {
    let mut rule = SimpleReplace2Rule::new(
        "PT_DIACRITICS_REPLACE",
        "Palavras estrangeiras com diacríticos: $match",
        "A palavra estrangeira original tem diacrítico",
        "'$match' é uma expressão estrangeira importada cuja grafia tem diacríticos. É preferível escrever $suggestions",
    )
    .with_separator(" ou ")
    .with_category(Category::new("TYPOS", "Possible Typo"))
    .with_issue_type(IssueType::Misspelling)
    .with_default_off()
    .with_sub_rule_ids()
    .with_url("http://escreverbem.com.br/o-frances-no-portugues-2/");
    rule.load_data(include_str!("../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/diacritics.txt"));
    rule
}

pub fn cliche_base() -> SimpleReplace2Rule {
    cliche_rule(
        "PT_CLICHE_REPLACE",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/cliches.txt"
        ),
    )
}

pub fn redundancy_base() -> SimpleReplace2Rule {
    redundancy_rule(
        "PT_REDUNDANCY_REPLACE",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/redundancies.txt"
        ),
    )
}

pub fn wordiness_base() -> SimpleReplace2Rule {
    wordiness_rule(
        "PT_WORDINESS_REPLACE",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/wordiness.txt"
        ),
    )
}

pub fn wikipedia_base() -> SimpleReplace2Rule {
    wikipedia_rule(
        "PT_WIKIPEDIA_COMMON_ERRORS",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/wikipedia.txt"
        ),
    )
}

pub fn cliche_portugal() -> SimpleReplace2Rule {
    cliche_rule(
        "PT_CLICHE_REPLACE_PT_PT",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-PT/cliches.txt"
        ),
    )
}

pub fn redundancy_portugal() -> SimpleReplace2Rule {
    redundancy_rule(
        "PT_REDUNDANCY_REPLACE_PT_PT",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-PT/redundancies.txt"
        ),
    )
}

pub fn wordiness_portugal() -> SimpleReplace2Rule {
    wordiness_rule(
        "PT_WORDINESS_REPLACE_PT_PT",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-PT/wordiness.txt"
        ),
    )
}

pub fn wikipedia_portugal() -> SimpleReplace2Rule {
    wikipedia_rule(
        "PT_WIKIPEDIA_COMMON_ERRORS_PT_PT",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-PT/wikipedia.txt"
        ),
    )
}

pub fn archaisms_portugal() -> SimpleReplace2Rule {
    archaisms_rule(include_str!(
        "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-PT/archaisms.txt"
    ))
}

pub fn barbarisms_portugal() -> SimpleReplace2Rule {
    barbarisms_rule(include_str!(
        "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-PT/barbarisms.txt"
    ))
}

pub fn cliche_brazil() -> SimpleReplace2Rule {
    cliche_rule(
        "PT_CLICHE_REPLACE_PT_BR",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-BR/cliches.txt"
        ),
    )
}

pub fn redundancy_brazil() -> SimpleReplace2Rule {
    redundancy_rule(
        "PT_REDUNDANCY_REPLACE_PT_BR",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-BR/redundancies.txt"
        ),
    )
}

pub fn wordiness_brazil() -> SimpleReplace2Rule {
    wordiness_rule(
        "PT_WORDINESS_REPLACE_PT_BR",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-BR/wordiness.txt"
        ),
    )
}

pub fn wikipedia_brazil() -> SimpleReplace2Rule {
    wikipedia_rule(
        "PT_WIKIPEDIA_COMMON_ERRORS_PT_BR",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-BR/wikipedia.txt"
        ),
    )
}

pub fn archaisms_brazil() -> SimpleReplace2Rule {
    archaisms_rule(include_str!(
        "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-BR/archaisms.txt"
    ))
}

pub fn barbarisms_brazil() -> SimpleReplace2Rule {
    barbarisms_rule(include_str!(
        "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/pt-BR/barbarisms.txt"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pt::test_util::make_sentence;
    use og_core::AnalyzedToken;

    fn first_replacement(rule: &SimpleReplace2Rule, text: &str) -> Option<String> {
        let sentence = make_sentence(text);
        let matches = rule.match_sentence(&sentence);
        matches
            .first()
            .and_then(|m| m.replacements().first().map(|r| r.value().to_string()))
    }

    #[test]
    fn barbarisms_matches_java_example() {
        let rule = barbarisms_base();
        assert_eq!(
            first_replacement(&rule, "curriculum vitae"),
            Some("Currículo".into())
        );
    }

    #[test]
    fn portugal_replace_matches_java_example() {
        let rule = portugal_replace_rule();
        assert_eq!(
            first_replacement(&rule, "aeromoça"),
            Some("Hospedeira de bordo".into())
        );
    }

    #[test]
    fn brazilian_replace_matches_java_example() {
        let rule = brazilian_replace_rule();
        assert_eq!(
            first_replacement(&rule, "hospedeira de bordo"),
            Some("Aeromoça".into())
        );
    }

    #[test]
    fn cliche_matches_java_example() {
        let rule = cliche_base();
        assert_eq!(
            first_replacement(&rule, "quente como uma fornalha"),
            Some("Quente".into())
        );
    }

    #[test]
    fn redundancy_matches_java_example() {
        let rule = redundancy_base();
        assert_eq!(
            first_replacement(&rule, "duna de areia"),
            Some("Duna".into())
        );
    }

    #[test]
    fn wordiness_matches_java_example() {
        let rule = wordiness_base();
        assert_eq!(
            first_replacement(&rule, "Raramente é o caso em que acontece isto."),
            Some("Raramente acontece".into())
        );
    }

    #[test]
    fn diacritics_matches_java_example() {
        let rule = diacritics_rule();
        assert!(!rule.is_default_on());
        assert_eq!(first_replacement(&rule, "coupe"), Some("Coupé".into()));
    }

    #[test]
    fn wikipedia_data_loads() {
        let rule = wikipedia_base();
        assert_eq!(rule.id(), "PT_WIKIPEDIA_COMMON_ERRORS");
        assert!(!rule.full_space.is_empty() || !rule.full_no_space.is_empty());
    }

    #[test]
    fn variant_archaisms_load() {
        let portugal = archaisms_portugal();
        assert_eq!(
            portugal
                .full_no_space
                .get("câmera")
                .map(|s| s.suggestion.clone()),
            Some("câmara".to_string())
        );
        assert!(archaisms_brazil().full_no_space.is_empty());
    }

    #[test]
    fn sentence_start_capitalizes_suggestion() {
        let rule = barbarisms_base();
        let sentence = make_sentence("Curriculum vitae foi enviado.");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches[0].replacements()[0].value(), "Currículo");
    }

    #[test]
    fn sub_rule_id_is_specific() {
        let rule = portugal_replace_rule();
        let sentence = make_sentence("aeromoça");
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches[0]
                .rule()
                .id()
                .starts_with("PT_PT_SIMPLE_REPLACE_AEROMO")
        );
    }

    #[test]
    fn proper_noun_tokens_are_skipped() {
        let rule = portugal_replace_rule();
        let mut sentence = make_sentence("aeromoça");
        sentence.tokens_mut()[1]
            .readings_mut()
            .push(AnalyzedToken::new("aeromoça", 0, 8).with_pos_tags(vec!["NPFS000".to_string()]));
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn message_uses_match_and_suggestions() {
        let rule = redundancy_base();
        let sentence = make_sentence("duna de areia");
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches[0]
                .message()
                .contains("\"duna de areia\" é um pleonasmo")
        );
        assert!(
            matches[0]
                .message()
                .contains("<suggestion>duna</suggestion>")
        );
    }
}
