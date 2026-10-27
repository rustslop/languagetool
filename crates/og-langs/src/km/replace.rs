use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use std::collections::HashMap;

const MAX_TOKENS_IN_MULTIWORD: usize = 20;

pub const KM_SIMPLE_REPLACE: &str = "KM_SIMPLE_REPLACE";

struct Suggestion {
    suggestion: String,
    message: Option<String>,
}

pub struct KhmerSimpleReplaceRule {
    id: String,
    description: String,
    short: String,
    message: String,
    separator: String,
    category: Category,
    issue_type: IssueType,
    url: Option<String>,
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

impl KhmerSimpleReplaceRule {
    pub fn new() -> Self {
        Self {
            id: KM_SIMPLE_REPLACE.to_string(),
            description: "Words or groups of words that are incorrect or obsolete".to_string(),
            short: "Consider following the spelling of Chuon Nath".to_string(),
            message: " Consider following the spelling of Chuon Nath ".to_string(),
            separator: " or ".to_string(),
            category: Category::new("MISC", "របស់ផ្សេងៗ"),
            issue_type: IssueType::Grammar,
            url: None,
            start_space: HashMap::new(),
            start_no_space: HashMap::new(),
            full_space: HashMap::new(),
            full_no_space: HashMap::new(),
        }
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
        if let Some(url) = &self.url {
            rm_rule = rm_rule.with_urls(vec![url.clone()]);
        }
        let text = sentence.text();
        let from_rel = from_pos.saturating_sub(sentence.start());
        let to_rel = to_pos.saturating_sub(sentence.start());
        let mut ctx_start = from_rel.saturating_sub(40);
        while ctx_start > 0 && !text.is_char_boundary(ctx_start) {
            ctx_start -= 1;
        }
        let mut ctx_end = std::cmp::min(to_rel + 40, text.len());
        while ctx_end < text.len() && !text.is_char_boundary(ctx_end) {
            ctx_end += 1;
        }
        let context_text = text[ctx_start..ctx_end].to_string();
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

impl Default for KhmerSimpleReplaceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for KhmerSimpleReplaceRule {
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

pub fn khmer_simple_replace_rule() -> KhmerSimpleReplaceRule {
    let mut rule = KhmerSimpleReplaceRule::new();
    rule.load_data(include_str!(
        "../../../../languagetool/languagetool-language-modules/km/src/main/resources/org/languagetool/rules/km/coherency.txt"
    ));
    rule
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule_matches(text: &str) -> Vec<(String, String, Vec<String>)> {
        let rule = khmer_simple_replace_rule();
        let sentence = crate::km::example_suite::km_pipeline().analyze_range(text, 0, text.len());
        rule.match_sentence(&sentence)
            .into_iter()
            .map(|m| {
                (
                    sentence.text()[m.offset()..m.offset() + m.length()].to_string(),
                    m.message().to_string(),
                    m.replacements()
                        .iter()
                        .map(|r| r.value().to_string())
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn fires_on_the_demo_text_word_with_java_strings() {
        assert_eq!(
            rule_matches("គាត់\u{200B}សំរាប់\u{200B}តែ\u{200B}ហូប\u{200B}បាយ។"),
            vec![(
                "សំរាប់".to_string(),
                " Consider following the spelling of Chuon Nath ".to_string(),
                vec!["សម្រាប់".to_string()],
            )],
            "KhmerSimpleReplaceRule: coherency.txt maps សំរាប់ to សម្រាប់ and the km message \
             override carries no $match/$suggestions placeholders"
        );
    }

    #[test]
    fn correct_spelling_stays_clean() {
        assert!(rule_matches("គាត់\u{200B}សម្រាប់\u{200B}តែ\u{200B}ហូប\u{200B}បាយ។").is_empty());
    }

    #[test]
    fn carries_the_java_configuration() {
        let rule = khmer_simple_replace_rule();
        assert_eq!(rule.id(), KM_SIMPLE_REPLACE);
        assert_eq!(
            rule.description(),
            "Words or groups of words that are incorrect or obsolete"
        );
        assert_eq!(rule.category().name(), "របស់ផ្សេងៗ");
    }

    fn rule_from(data: &str) -> KhmerSimpleReplaceRule {
        let mut rule = KhmerSimpleReplaceRule::new();
        rule.load_data(data);
        rule
    }

    fn sentence_from_parts(parts: &[&str]) -> AnalyzedSentence {
        use og_core::AnalyzedToken;
        let text = parts.concat();
        let mut sentence = AnalyzedSentence::new(text.clone(), 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut pos = 0usize;
        for part in parts {
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                *part,
                pos,
                pos + part.len(),
            )));
            pos += part.len();
        }
        sentence.set_tokens(tokens);
        sentence
    }

    fn triples(
        rule: &KhmerSimpleReplaceRule,
        sentence: &AnalyzedSentence,
    ) -> Vec<(String, String)> {
        rule.match_sentence(sentence)
            .into_iter()
            .map(|m| {
                (
                    sentence.text()[m.offset()..m.offset() + m.length()].to_string(),
                    m.replacements()
                        .iter()
                        .map(|r| r.value().to_string())
                        .collect::<Vec<_>>()
                        .join("|"),
                )
            })
            .collect()
    }

    #[test]
    fn multiword_entries_with_spaces_load_and_match() {
        let rule = rule_from("# header\n\nfoo bar=baz qux # inline note\n");
        let sentence = sentence_from_parts(&["The", " ", "foo", " ", "bar", "."]);
        assert_eq!(
            triples(&rule, &sentence),
            vec![("foo bar".to_string(), "baz qux".to_string())]
        );
    }

    #[test]
    fn adjacent_tokens_join_into_the_no_space_key() {
        let rule = rule_from("ab=xy\n");
        let sentence = sentence_from_parts(&["The", " ", "a", "b"]);
        assert_eq!(
            triples(&rule, &sentence),
            vec![("ab".to_string(), "xy".to_string())]
        );
    }

    #[test]
    fn sentence_start_and_uppercase_inputs_uppercase_the_suggestion() {
        let rule = rule_from("foo bar=baz qux\n");
        let start = sentence_from_parts(&["Foo", " ", "bar", "."]);
        assert_eq!(
            triples(&rule, &start),
            vec![("Foo bar".to_string(), "Baz qux".to_string())]
        );
        let shouting = sentence_from_parts(&["FOO", " ", "BAR", "."]);
        assert_eq!(
            triples(&rule, &shouting),
            vec![("FOO BAR".to_string(), "BAZ QUX".to_string())]
        );
    }

    #[test]
    fn a_suggestion_equal_to_the_original_drops_the_match() {
        let rule = rule_from("same=same\n");
        let sentence = sentence_from_parts(&["same", "."]);
        assert!(triples(&rule, &sentence).is_empty());
    }

    #[test]
    fn tab_messages_win_and_urls_fall_back_to_the_placeholder_message() {
        let noted = rule_from("foo=bar\ta custom note\n");
        let sentence = sentence_from_parts(&["foo", "."]);
        assert_eq!(
            noted
                .match_sentence(&sentence)
                .first()
                .map(|m| m.message().to_string()),
            Some("a custom note".to_string())
        );

        let linked = rule_from("foo=bar\thttps://example.com/why\n");
        assert_eq!(
            linked
                .match_sentence(&sentence)
                .first()
                .map(|m| m.message().to_string()),
            Some(" Consider following the spelling of Chuon Nath ".to_string())
        );

        let paired = rule_from("zip=alpha|beta\n");
        let zip = sentence_from_parts(&["zip", "."]);
        assert_eq!(
            paired
                .match_sentence(&zip)
                .first()
                .map(|m| m.message().to_string()),
            Some(" Consider following the spelling of Chuon Nath ".to_string())
        );
        assert_eq!(triples(&paired, &zip)[0].1, "Alpha|Beta");
    }

    #[test]
    fn the_longest_multiword_match_wins_over_covered_shorter_ones() {
        let rule = rule_from("foo bar=short\nfoo bar baz=longest\n");
        let sentence = sentence_from_parts(&["foo", " ", "bar", " ", "baz", "."]);
        assert_eq!(
            triples(&rule, &sentence),
            vec![("foo bar baz".to_string(), "Longest".to_string())]
        );
    }

    #[test]
    fn immunized_and_punctuation_start_tokens_never_match() {
        let rule = rule_from("1234=xyz\nfoo=bar\n");
        let digits = sentence_from_parts(&["1234", " ", "foo"]);
        assert_eq!(
            triples(&rule, &digits),
            vec![("foo".to_string(), "Bar".to_string())],
            "a digit-leading sentence token is skipped as sentence-start punctuation"
        );

        let mut immunized = sentence_from_parts(&["foo", "."]);
        immunized.tokens_mut()[1].immunize();
        assert!(triples(&rule, &immunized).is_empty());
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(
            KhmerSimpleReplaceRule::default().id(),
            KhmerSimpleReplaceRule::new().id()
        );
    }
}
