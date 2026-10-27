use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};
use std::collections::HashMap;

const MAX_TOKENS_IN_MULTIWORD: usize = 20;
const MAX_LENGTH_SHORT_WORDS: usize = 4;

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

pub(crate) struct SimpleReplace2Engine {
    id: String,
    description: String,
    short: String,
    message: String,
    suggestions_separator: String,
    checking_case: bool,
    ignore_short_uppercase_words: bool,
    case_sensitive: bool,
    sub_rule_specific_ids: bool,
    category: Category,
    issue_type: IssueType,
    start_space: HashMap<String, usize>,
    start_no_space: HashMap<char, usize>,
    full_space: HashMap<String, Suggestion>,
    full_no_space: HashMap<String, Suggestion>,
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

fn to_id(input: &str) -> String {
    input
        .to_uppercase()
        .trim()
        .replace(' ', "_")
        .replace('\'', "_Q_")
}

fn is_punctuation_start(word: &str) -> bool {
    word.chars().any(|c| c.is_numeric()) || !word.chars().any(|c| c.is_alphabetic())
}

impl SimpleReplace2Engine {
    pub(crate) fn new(
        id: &str,
        description: &str,
        short: &str,
        message: &str,
        category: Category,
        issue_type: IssueType,
    ) -> Self {
        Self {
            id: id.to_string(),
            description: description.to_string(),
            short: short.to_string(),
            message: message.to_string(),
            suggestions_separator: ", ".to_string(),
            checking_case: false,
            ignore_short_uppercase_words: true,
            case_sensitive: false,
            sub_rule_specific_ids: false,
            category,
            issue_type,
            start_space: HashMap::new(),
            start_no_space: HashMap::new(),
            full_space: HashMap::new(),
            full_no_space: HashMap::new(),
        }
    }

    pub(crate) fn checking_case(mut self) -> Self {
        self.checking_case = true;
        self.ignore_short_uppercase_words = false;
        self
    }

    pub(crate) fn case_sensitive(mut self) -> Self {
        self.case_sensitive = true;
        self
    }

    pub(crate) fn sub_rule_specific_ids(mut self) -> Self {
        self.sub_rule_specific_ids = true;
        self
    }

    pub(crate) fn load(&mut self, data: &str) {
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
            let rebuilt;
            if self.checking_case {
                let first = line.split('=').next().unwrap_or("");
                let mut value = format!("{}={}", first.to_lowercase().trim(), first.trim());
                if let Some((_, message)) = line.split_once('=') {
                    value.push('\t');
                    value.push_str(message.trim());
                }
                rebuilt = value;
                line = &rebuilt;
            }
            let (conf_pair, message) = match line.split_once('\t') {
                Some((pair, message)) => (pair, Some(message.to_string())),
                None => (line, None),
            };
            let (wrong_side, suggestion) = match conf_pair.split_once('=') {
                Some((wrong, suggestion)) => (wrong, suggestion.to_string()),
                None => continue,
            };
            for wrong_form in wrong_side.split('|') {
                let search_key = if self.case_sensitive {
                    wrong_form.to_string()
                } else {
                    wrong_form.to_lowercase()
                };
                let suggestion = Suggestion {
                    suggestion: suggestion.clone(),
                    message: message.clone(),
                };
                match search_key.find(' ') {
                    Some(pos) if pos > 0 => {
                        let first_token = search_key[..pos].to_string();
                        let token_count = search_key.split(' ').count();
                        let entry = self.start_space.entry(first_token).or_insert(0);
                        *entry = (*entry).max(token_count);
                        self.full_space.insert(search_key, suggestion);
                    }
                    _ => {
                        let first_char = search_key.chars().next().unwrap_or('\0');
                        let entry = self.start_no_space.entry(first_char).or_insert(0);
                        *entry = (*entry).max(search_key.len());
                        self.full_no_space.insert(search_key, suggestion);
                    }
                }
            }
        }
    }

    fn build_message(&self, original: &str, suggestion: &Suggestion) -> String {
        if let Some(message) = &suggestion.message {
            if message.starts_with("http://") || message.starts_with("https://") {
                return self.render_default_message(original, suggestion);
            }
            return message.clone();
        }
        self.render_default_message(original, suggestion)
    }

    fn render_default_message(&self, original: &str, suggestion: &Suggestion) -> String {
        let replacements: Vec<&str> = suggestion.suggestion.split('|').collect();
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
        replace_first(&self.message, "$match", original)
            .map(|m| replace_first(&m, "$suggestions", &msg_suggestions).unwrap_or(m))
            .unwrap_or_else(|| self.message.clone())
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
        end_index: usize,
        check_case_covered_upto: &mut usize,
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

        if self.checking_case {
            if end_index <= *check_case_covered_upto {
                return;
            }
            let replacement_check_case = replacements[0];
            if (sent_start == start_index
                && original == uppercase_first_char(replacement_check_case))
                || original == replacement_check_case
            {
                if let Some(last) = matches.last()
                    && last.offset() + last.length() > from_pos
                {
                    matches.pop();
                }
                *check_case_covered_upto = end_index;
                return;
            }
            if !first_word_in_sugg_is_camel_case
                && !original.chars().any(char::is_lowercase)
                && (self.ignore_short_uppercase_words || original.len() > MAX_LENGTH_SHORT_WORDS)
            {
                *check_case_covered_upto = end_index;
                return;
            }
        }

        let mut final_replacements: Vec<String> = Vec::new();
        for repl in &replacements {
            let mut final_repl = (*repl).to_string();
            if !first_word_in_sugg_is_camel_case
                && (sent_start == start_index || (is_capitalized && !self.checking_case))
            {
                final_repl = uppercase_first_char(repl);
            }
            if !self.checking_case && is_all_upper {
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

        let message = self.build_message(original, suggestion);
        let (rule_id, rule_description) = if self.sub_rule_specific_ids {
            (
                to_id(&format!("{}_{}", self.id, original)),
                self.description.replace("$match", original),
            )
        } else {
            (self.id.clone(), self.description.clone())
        };
        let rm_rule = RuleMatchRule::new(rule_id, rule_description)
            .with_category(self.category.clone())
            .with_issue_type(self.issue_type.as_str());
        let context = self.context_of(sentence, from_pos, to_pos);
        let rule_match = RuleMatch::new(
            message,
            from_pos,
            to_pos - from_pos,
            rm_rule,
            RuleMatchContext::new(context.0, context.1, context.2 - context.1),
        )
        .with_sentence(sentence.text().to_string())
        .with_short_message(self.short.clone())
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

    fn context_of(
        &self,
        sentence: &AnalyzedSentence,
        from: usize,
        to: usize,
    ) -> (String, usize, usize) {
        let text = sentence.text();
        let mut ctx_start = from.saturating_sub(40).min(text.len());
        while ctx_start > 0 && !text.is_char_boundary(ctx_start) {
            ctx_start -= 1;
        }
        let mut ctx_end = (to + 40).min(text.len());
        while ctx_end < text.len() && !text.is_char_boundary(ctx_end) {
            ctx_end += 1;
        }
        (text[ctx_start..ctx_end].to_string(), ctx_start, ctx_end)
    }

    fn tokens_of(sentence: &AnalyzedSentence) -> Vec<TokenView<'_>> {
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

    pub(crate) fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = Self::tokens_of(sentence);
        let mut matches = Vec::new();
        let mut sent_start = 1usize;
        while sent_start < tokens.len() && is_punctuation_start(tokens[sent_start].text) {
            sent_start += 1;
        }
        let mut check_case_covered_upto = 0usize;
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
            let tok = if self.case_sensitive {
                tok
            } else {
                tok.to_lowercase()
            };

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
                        let key = if self.case_sensitive {
                            original.clone()
                        } else {
                            original.to_lowercase()
                        };
                        self.create_match(
                            &mut matches,
                            self.full_space.get(&key),
                            &original,
                            tokens[start_index].start,
                            tokens[end_index].end,
                            sentence,
                            sent_start,
                            start_index,
                            end_index,
                            &mut check_case_covered_upto,
                        );
                    }
                    end_index += 1;
                }
            }
            if tok
                .chars()
                .next()
                .is_some_and(|c| self.start_no_space.contains_key(&c))
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
                    let key = if self.case_sensitive {
                        original.clone()
                    } else {
                        original.to_lowercase()
                    };
                    self.create_match(
                        &mut matches,
                        self.full_no_space.get(&key),
                        &original,
                        tokens[start_index].start,
                        tokens[end_index].end,
                        sentence,
                        sent_start,
                        start_index,
                        end_index,
                        &mut check_case_covered_upto,
                    );
                    end_index += 1;
                }
            }
        }
        matches
    }
}

fn replace_first(haystack: &str, needle: &str, replacement: &str) -> Option<String> {
    let idx = haystack.find(needle)?;
    let mut out = String::with_capacity(haystack.len() + replacement.len());
    out.push_str(&haystack[..idx]);
    out.push_str(replacement);
    out.push_str(&haystack[idx + needle.len()..]);
    Some(out)
}

pub struct DutchSimpleReplaceRule {
    engine: SimpleReplace2Engine,
}

pub fn dutch_simple_replace_rule() -> DutchSimpleReplaceRule {
    let mut engine = SimpleReplace2Engine::new(
        "NL_SIMPLE_REPLACE",
        "Snelle correctie van veel voorkomende vergissingen ($match)",
        "Vergissing?",
        "'$match' zou fout kunnen zijn. Misschien bedoelt u: $suggestions",
        Category::new("VERGISSINGEN", "Vergissingen"),
        IssueType::Misspelling,
    )
    .case_sensitive()
    .sub_rule_specific_ids();
    engine.load(include_str!(
        "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/rules/nl/replace.txt"
    ));
    DutchSimpleReplaceRule { engine }
}

impl Rule for DutchSimpleReplaceRule {
    fn id(&self) -> &str {
        "NL_SIMPLE_REPLACE"
    }

    fn description(&self) -> &str {
        "Snelle correctie van veel voorkomende vergissingen ($match)"
    }

    fn category(&self) -> Category {
        Category::new("VERGISSINGEN", "Vergissingen")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        self.engine.match_sentence(sentence)
    }
}

pub struct DutchCheckCaseRule {
    engine: SimpleReplace2Engine,
}

pub fn dutch_check_case_rule() -> DutchCheckCaseRule {
    let mut engine = SimpleReplace2Engine::new(
        "NL_CHECKCASE",
        "Controle op hoofd- en kleine letters: $match",
        "Schrijfwijze",
        "Juiste schrijfwijze",
        Category::new("CASING", "Hoofdlettergebruik"),
        IssueType::Typographical,
    )
    .checking_case()
    .sub_rule_specific_ids();
    engine.load(include_str!(
        "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/rules/nl/check_case.txt"
    ));
    DutchCheckCaseRule { engine }
}

impl Rule for DutchCheckCaseRule {
    fn id(&self) -> &str {
        "NL_CHECKCASE"
    }

    fn description(&self) -> &str {
        "Controle op hoofd- en kleine letters: $match"
    }

    fn category(&self) -> Category {
        Category::new("CASING", "Hoofdlettergebruik")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Typographical
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        self.engine.match_sentence(sentence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nl::test_support::DutchPipeline;

    fn sentence(text: &str) -> og_core::AnalyzedSentence {
        let pipeline = DutchPipeline::shared();
        pipeline.analyze_range(text, 0, text.len())
    }

    fn simple_matches(text: &str) -> Vec<(String, Vec<String>)> {
        let rule = dutch_simple_replace_rule();
        rule.match_sentence(&sentence(text))
            .into_iter()
            .map(|m| {
                (
                    sentence(text).text()[m.offset()..m.offset() + m.length()].to_string(),
                    m.replacements()
                        .iter()
                        .map(|r| r.value().to_string())
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn java_pins_simple_replace_correct_sentences() {
        assert!(simple_matches("all right").is_empty());
        assert!(
            simple_matches("De Kudde eigenschappen").is_empty(),
            "case-sensitive key 'kudde eigenschappen' must not match 'Kudde eigenschappen'"
        );
    }

    #[test]
    fn java_pins_simple_replace_suggestions() {
        assert_eq!(
            simple_matches("klaa"),
            vec![("klaa".to_string(), vec!["Klaar".to_string()])]
        );
        assert_eq!(
            simple_matches("een BTW nummer"),
            vec![("BTW nummer".to_string(), vec!["btw-nummer".to_string()])]
        );
        assert_eq!(
            simple_matches("kleurweergave eigenschappen."),
            vec![(
                "kleurweergave eigenschappen".to_string(),
                vec!["Kleurweergave-eigenschappen".to_string()]
            )]
        );
        assert_eq!(
            simple_matches("De kleurweergave eigenschappen."),
            vec![(
                "kleurweergave eigenschappen".to_string(),
                vec!["kleurweergave-eigenschappen".to_string()]
            )]
        );
    }

    #[test]
    fn simple_replace_uses_sub_rule_specific_ids() {
        let rule = dutch_simple_replace_rule();
        let matches = rule.match_sentence(&sentence("klaa"));
        assert_eq!(matches.len(), 1);
        assert!(matches[0].rule().id().starts_with("NL_SIMPLE_REPLACE_"));
    }

    #[test]
    fn java_pins_check_case_rule() {
        let rule = dutch_check_case_rule();
        assert!(rule.match_sentence(&sentence("een bisschop")).is_empty());
        assert!(rule.match_sentence(&sentence("Een bisschop")).is_empty());
        assert!(rule.match_sentence(&sentence("EEN BISSCHOP")).is_empty());

        let matches = rule.match_sentence(&sentence("Een Bisschop"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacements()[0].value(),
            "Een bisschop",
            "correct phrase wins over the single-word match"
        );

        let matches = rule.match_sentence(&sentence("Hij is een Bisschop."));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "een bisschop");

        let matches = rule.match_sentence(&sentence("Mag ik die DVD lenen?"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "dvd");

        let matches = rule.match_sentence(&sentence("Heb je de nieuwe IPAD al gezien?"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "iPad");
    }
}
