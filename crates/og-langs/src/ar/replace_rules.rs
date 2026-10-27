use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};
use std::collections::HashMap;
use std::sync::Arc;

const MAX_TOKENS_IN_MULTIWORD: usize = 20;

struct Suggestion {
    suggestion: String,
    message: Option<String>,
}

struct TokenView<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    whitespace_before: bool,
}

fn is_all_uppercase(word: &str) -> bool {
    word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
}

fn is_capitalized_word(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => chars.all(|c| !c.is_alphabetic() || c.is_lowercase()),
        _ => false,
    }
}

fn is_camel_case(token: &str) -> bool {
    let bytes = token.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_lowercase() {
        i += 1;
    }
    if i == 0 || i >= bytes.len() || !bytes[i].is_ascii_uppercase() {
        return false;
    }
    i += 1;
    while i < bytes.len() {
        if !bytes[i].is_ascii_alphabetic() {
            return false;
        }
        i += 1;
    }
    true
}

fn uppercase_first_char(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => word.to_string(),
    }
}

fn is_punctuation_start(word: &str) -> bool {
    word.chars().any(|c| c.is_numeric()) || !word.chars().any(|c| c.is_alphabetic())
}

fn replace_first(haystack: &str, needle: &str, replacement: &str) -> String {
    match haystack.find(needle) {
        Some(idx) => {
            let mut out = String::with_capacity(haystack.len() + replacement.len());
            out.push_str(&haystack[..idx]);
            out.push_str(replacement);
            out.push_str(&haystack[idx + needle.len()..]);
            out
        }
        None => haystack.to_string(),
    }
}

pub struct ArabicSimpleReplaceRule2 {
    id: String,
    description: String,
    short: String,
    message: String,
    suggestions_separator: String,
    category: Category,
    issue_type: IssueType,
    start_space: HashMap<String, usize>,
    start_no_space: HashMap<char, usize>,
    full_space: HashMap<String, Arc<Suggestion>>,
    full_no_space: HashMap<String, Arc<Suggestion>>,
}

impl ArabicSimpleReplaceRule2 {
    pub fn new(
        id: &str,
        description: &str,
        short: &str,
        message: &str,
        suggestions_separator: &str,
        category: Category,
        issue_type: IssueType,
    ) -> Self {
        Self {
            id: id.to_string(),
            description: description.to_string(),
            short: short.to_string(),
            message: message.to_string(),
            suggestions_separator: suggestions_separator.to_string(),
            category,
            issue_type,
            start_space: HashMap::new(),
            start_no_space: HashMap::new(),
            full_space: HashMap::new(),
            full_no_space: HashMap::new(),
        }
    }

    pub fn load_data(&mut self, data: &str) {
        for raw_line in data.lines() {
            let mut line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(hash) = line.find('#') {
                line = line[..hash].trim();
            }
            if line.is_empty() {
                continue;
            }
            let (conf_pair, message) = match line.split_once('\t') {
                Some((pair, msg)) => (pair, Some(msg.to_string())),
                None => (line, None),
            };
            let Some((wrong_side, suggestion)) = conf_pair.split_once('=') else {
                continue;
            };
            let suggestion = Arc::new(Suggestion {
                suggestion: suggestion.to_string(),
                message,
            });
            for wrong_form in wrong_side.split('|') {
                let search_key = wrong_form.to_lowercase();
                match search_key.find(' ') {
                    Some(pos) if pos > 0 => {
                        let first_token = search_key[..pos].to_string();
                        let token_count = search_key.split(' ').count();
                        let entry = self.start_space.entry(first_token).or_insert(0);
                        *entry = (*entry).max(token_count);
                        self.full_space.insert(search_key, Arc::clone(&suggestion));
                    }
                    _ => {
                        let Some(first_char) = search_key.chars().next() else {
                            continue;
                        };
                        let entry = self.start_no_space.entry(first_char).or_insert(0);
                        *entry = (*entry).max(search_key.len());
                        self.full_no_space
                            .insert(search_key, Arc::clone(&suggestion));
                    }
                }
            }
        }
    }

    fn token_views(sentence: &AnalyzedSentence) -> Vec<TokenView<'_>> {
        let all = sentence.tokens();
        let mut out = Vec::new();
        for (idx, t) in all.iter().enumerate() {
            if t.is_whitespace() {
                continue;
            }
            let whitespace_before = idx > 0 && all[idx - 1].is_whitespace();
            out.push(TokenView {
                text: t.token().token(),
                start: t.token().start(),
                end: t.token().end(),
                whitespace_before,
            });
        }
        out
    }

    fn render_default_message(&self, original: &str, replacements: &[&str]) -> String {
        let mut msg_suggestions = String::new();
        let last = replacements.len().saturating_sub(1);
        for (k, sugg) in replacements.iter().enumerate() {
            if k > 0 {
                msg_suggestions.push_str(if k == last {
                    &self.suggestions_separator
                } else {
                    ", "
                });
            }
            msg_suggestions.push_str(&format!("<suggestion>{sugg}</suggestion>"));
        }
        let with_match = replace_first(&self.message, "$match", original);
        replace_first(&with_match, "$suggestions", &msg_suggestions)
    }

    #[allow(clippy::too_many_arguments)]
    fn create_match(
        &self,
        matches: &mut Vec<RuleMatch>,
        suggestion: Option<&Suggestion>,
        original: &str,
        from_pos: usize,
        to_pos: usize,
        sentence: &AnalyzedSentence,
        sent_start: usize,
        start_index: usize,
    ) {
        let Some(suggestion) = suggestion else {
            return;
        };
        if let Some(last) = matches.last()
            && last.offset() <= from_pos
            && last.offset() + last.length() >= to_pos
        {
            return;
        }
        let replacements: Vec<&str> = suggestion.suggestion.split('|').collect();
        let first_word_in_sugg_is_camel_case = replacements
            .iter()
            .any(|r| is_camel_case(r.split(' ').next().unwrap_or("")));
        let is_all_upper = is_all_uppercase(original);
        let is_capitalized = is_capitalized_word(original.split(' ').next().unwrap_or(""));

        let mut final_replacements: Vec<String> = Vec::new();
        for repl in &replacements {
            let mut final_repl = if !first_word_in_sugg_is_camel_case
                && (sent_start == start_index || is_capitalized)
            {
                uppercase_first_char(repl)
            } else {
                (*repl).to_string()
            };
            if is_all_upper {
                final_repl = repl.to_uppercase();
            }
            if final_repl == *original {
                final_replacements.clear();
                break;
            }
            if *repl != original && !final_replacements.contains(&final_repl) {
                final_replacements.push(final_repl);
            }
        }
        if final_replacements.is_empty() {
            return;
        }

        let message = match &suggestion.message {
            Some(m) if !m.starts_with("http://") && !m.starts_with("https://") => m.clone(),
            _ => self.render_default_message(original, &replacements),
        };

        let rm_rule = RuleMatchRule::new(self.id.clone(), self.description.clone())
            .with_category(self.category.clone())
            .with_issue_type(self.issue_type.as_str());
        let (context_text, ctx_start, ctx_end) = Self::context_of(sentence, from_pos, to_pos);
        let rule_match = RuleMatch::new(
            message,
            from_pos,
            to_pos - from_pos,
            rm_rule,
            RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
        )
        .with_short_message(self.short.clone())
        .with_sentence(sentence.text().to_string())
        .with_replacements(
            final_replacements
                .iter()
                .map(SuggestedReplacement::new)
                .collect(),
        );
        if let Some(last) = matches.last()
            && last.offset() >= from_pos
            && last.offset() + last.length() <= to_pos
        {
            matches.pop();
        }
        matches.push(rule_match);
    }

    fn context_of(sentence: &AnalyzedSentence, from: usize, to: usize) -> (String, usize, usize) {
        let text = sentence.text();
        let rel_from = from - sentence.start();
        let rel_to = to - sentence.start();
        let mut ctx_start = rel_from.saturating_sub(40).min(text.len());
        while ctx_start > 0 && !text.is_char_boundary(ctx_start) {
            ctx_start -= 1;
        }
        let mut ctx_end = (rel_to + 40).min(text.len());
        while ctx_end < text.len() && !text.is_char_boundary(ctx_end) {
            ctx_end += 1;
        }
        (text[ctx_start..ctx_end].to_string(), ctx_start, ctx_end)
    }
}

impl Rule for ArabicSimpleReplaceRule2 {
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
        let tokens = Self::token_views(sentence);
        let mut matches = Vec::new();
        let mut sent_start = 1usize;
        while sent_start < tokens.len() && is_punctuation_start(tokens[sent_start].text) {
            sent_start += 1;
        }
        for start_index in sent_start..tokens.len() {
            let mut tok = tokens[start_index].text.to_string();
            if tok.is_empty() {
                continue;
            }
            let mut k = start_index + 1;
            while k < tokens.len() && !tokens[k].whitespace_before {
                tok.push_str(tokens[k].text);
                k += 1;
            }
            let tok = tok.to_lowercase();

            if let Some(&max_token_len) = self.start_space.get(&tok) {
                let mut key_builder = String::new();
                let mut end_index = start_index;
                while end_index < tokens.len() && end_index - start_index < MAX_TOKENS_IN_MULTIWORD
                {
                    if end_index > start_index && tokens[end_index].whitespace_before {
                        key_builder.push(' ');
                    }
                    key_builder.push_str(tokens[end_index].text);
                    let original = key_builder.clone();
                    let number_of_spaces = original.matches(' ').count();
                    if number_of_spaces + 1 > max_token_len {
                        break;
                    }
                    if number_of_spaces > 0 {
                        let key = original.to_lowercase();
                        self.create_match(
                            &mut matches,
                            self.full_space.get(&key).map(Arc::as_ref),
                            &original,
                            tokens[start_index].start,
                            tokens[end_index].end,
                            sentence,
                            sent_start,
                            start_index,
                        );
                    }
                    end_index += 1;
                }
            }
            if let Some(first_char) = tok.chars().next()
                && self.start_no_space.contains_key(&first_char)
            {
                let mut key_builder = String::new();
                let mut end_index = start_index;
                while end_index < tokens.len() && end_index - start_index < MAX_TOKENS_IN_MULTIWORD
                {
                    if end_index > start_index && tokens[end_index].whitespace_before {
                        break;
                    }
                    key_builder.push_str(tokens[end_index].text);
                    let original = key_builder.clone();
                    let key = original.to_lowercase();
                    self.create_match(
                        &mut matches,
                        self.full_no_space.get(&key).map(Arc::as_ref),
                        &original,
                        tokens[start_index].start,
                        tokens[end_index].end,
                        sentence,
                        sent_start,
                        start_index,
                    );
                    end_index += 1;
                }
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule_with(data: &str) -> ArabicSimpleReplaceRule2 {
        let mut rule = ArabicSimpleReplaceRule2::new(
            "AR_TEST_REPLACE",
            "test",
            "short",
            "قل $suggestions",
            " أو ",
            Category::new("CONFUSED_WORDS", "Confused Words"),
            IssueType::Uncategorized,
        );
        rule.load_data(data);
        rule
    }

    fn sentence_from_parts(parts: &[&str]) -> AnalyzedSentence {
        let text = parts.join(" ");
        let mut sentence = AnalyzedSentence::new(&text, 0, text.len());
        let mut tokens = vec![og_core::AnalyzedTokenReadings::new(
            og_core::AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for part in parts {
            let start = offset + text[offset..].find(part).unwrap_or(0);
            if start > offset {
                tokens.push(og_core::AnalyzedTokenReadings::new(
                    og_core::AnalyzedToken::new(&text[offset..start], offset, start),
                ));
            }
            let end = start + part.len();
            tokens.push(og_core::AnalyzedTokenReadings::new(
                og_core::AnalyzedToken::new(*part, start, end),
            ));
            offset = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn load_indexes_space_and_no_space_maps() {
        let rule = rule_with("هضبة الجولان=هضبة الجَوْلان\tالجَوْلان بالفتح\nهضبة=هَضْبَة");
        assert_eq!(rule.start_space.get("هضبة"), Some(&2));
        assert!(rule.full_space.contains_key("هضبة الجولان"));
        assert!(rule.full_no_space.contains_key("هضبة"));
        let dead = rule_with("هضبة=هَضْبَة");
        assert!(dead.full_space.is_empty());
    }

    #[test]
    fn inline_hash_comments_are_stripped_like_java() {
        let mut rule = rule_with("تجربة=تجرِبة#comment");
        rule.load_data("#full comment\nأسطحة=سطوح|أسْطح");
        assert!(rule.full_no_space.contains_key("تجربة"));
        assert!(rule.full_no_space.contains_key("أسطحة"));
    }

    #[test]
    fn multiword_phrases_match_beyond_three_tokens() {
        let rule = rule_with("هناك خطأ في العبارة=في العبارة خطأ");
        let matches = rule.match_sentence(&sentence_from_parts(&["هناك", "خطأ", "في", "العبارة"]));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "في العبارة خطأ");
    }

    #[test]
    fn longest_match_suppresses_the_prefix_key() {
        let rule = rule_with("هضبة=هَضْبَة\nهضبة الجولان=هضبة الجَوْلان");
        let matches = rule.match_sentence(&sentence_from_parts(&["هضبة", "الجولان"]));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "هضبة الجَوْلان");
    }

    #[test]
    fn no_space_run_concatenates_like_java() {
        let rule = rule_with("أبالفقيد=أبو الفقيد");
        let text = "أبالفقيد";
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        sentence.set_tokens(vec![
            og_core::AnalyzedTokenReadings::new(
                og_core::AnalyzedToken::new("<S>", 0, 0)
                    .with_pos_tags(vec!["SENT_START".to_string()]),
            ),
            og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
                "أبال",
                0,
                "أبال".len(),
            )),
            og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
                "فقيد",
                "أبال".len(),
                text.len(),
            )),
        ]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "أبو الفقيد");
    }

    #[test]
    fn tab_message_wins_and_template_falls_back() {
        let mut rule = rule_with("طرشي=فلفل حلو\tالكلمة عامية");
        rule.load_data("أسطحة=سطوح|أسْطح");
        let with_tab = rule.match_sentence(&sentence_from_parts(&["طرشي"]));
        assert_eq!(with_tab.len(), 1);
        assert_eq!(with_tab[0].message(), "الكلمة عامية");

        let templated = rule.match_sentence(&sentence_from_parts(&["أسطحة"]));
        assert_eq!(templated.len(), 1);
        assert_eq!(
            templated[0].message(),
            "قل <suggestion>سطوح</suggestion> أو <suggestion>أسْطح</suggestion>"
        );
        assert_eq!(templated[0].short_message(), Some("short"));
    }
}
