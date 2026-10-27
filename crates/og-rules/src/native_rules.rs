use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};

pub struct WordRepeatRule {
    id: String,
    category: Category,
    ignored_words: Vec<String>,
    ignore_single_letter_repeats: bool,
}

impl WordRepeatRule {
    pub fn new() -> Self {
        Self {
            id: "WORD_REPEAT_RULE".to_string(),
            category: Category::new("MISC", "Miscellaneous"),
            ignored_words: Vec::new(),
            ignore_single_letter_repeats: false,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_ignored_words(mut self, words: &[&str]) -> Self {
        self.ignored_words = words.iter().map(|w| w.to_string()).collect();
        self
    }

    pub fn with_ignore_single_letter_repeats(mut self) -> Self {
        self.ignore_single_letter_repeats = true;
        self
    }
}

impl Default for WordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for WordRepeatRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Word repetition"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Redundancy
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let tokens = sentence.non_whitespace_tokens();

        for i in 1..tokens.len() {
            let prev = tokens[i - 1];
            let curr = tokens[i];

            let prev_text = prev.token().token().to_lowercase();
            let curr_text = curr.token().token().to_lowercase();

            if !(prev_text != curr_text
                || prev.is_whitespace()
                || curr.is_whitespace()
                || !prev_text.chars().any(|c| c.is_alphabetic())
                || self.ignored_words.contains(&prev_text)
                || self.ignore_single_letter_repeats && prev_text.chars().count() == 1)
            {
                let start = curr.token().start();
                let end = curr.token().end();
                let text_len = sentence.text().len();
                let mut context_start = start.saturating_sub(20);
                while context_start > 0 && !sentence.text().is_char_boundary(context_start) {
                    context_start -= 1;
                }
                let mut context_end = std::cmp::min(end + 20, text_len);
                while context_end < text_len && !sentence.text().is_char_boundary(context_end) {
                    context_end += 1;
                }
                let context_text = sentence.text()[context_start..context_end].to_string();

                let rule = RuleMatchRule::new(&self.id, self.description())
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Redundancy.as_str());

                matches.push(
                    RuleMatch::new(
                        format!("Possible word repetition: '{}'", curr.token().token()),
                        start,
                        end - start,
                        rule,
                        RuleMatchContext::new(
                            context_text,
                            context_start,
                            context_end - context_start,
                        ),
                    )
                    .with_sentence(sentence.text().to_string())
                    .with_replacements(vec![SuggestedReplacement::new(curr.token().token())]),
                );
            }
        }

        matches
    }
}

pub struct DoublePunctuationRule {
    id: String,
    description: String,
    category: Category,
    dot_message: String,
    dot_short_message: String,
    comma_message: String,
    comma_short_message: String,
    comma_character: char,
}

impl DoublePunctuationRule {
    pub fn new() -> Self {
        Self {
            id: "DOUBLE_PUNCTUATION".to_string(),
            description: "Double punctuation".to_string(),
            category: Category::new("PUNCTUATION", "Punctuation"),
            dot_message: "Two consecutive dots".to_string(),
            dot_short_message: "Two consecutive dots".to_string(),
            comma_message: "Two consecutive commas".to_string(),
            comma_short_message: "Two consecutive commas".to_string(),
            comma_character: ',',
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_comma_character(mut self, comma: char) -> Self {
        self.comma_character = comma;
        self
    }

    pub fn with_messages(
        mut self,
        dot: impl Into<String>,
        dot_short: impl Into<String>,
        comma: impl Into<String>,
        comma_short: impl Into<String>,
    ) -> Self {
        self.dot_message = dot.into();
        self.dot_short_message = dot_short.into();
        self.comma_message = comma.into();
        self.comma_short_message = comma_short.into();
        self
    }
}

impl Default for DoublePunctuationRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DoublePunctuationRule {
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
        IssueType::Typographical
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let tokens = sentence.non_whitespace_tokens();
        let comma = self.comma_character.to_string();

        let mut dot_count = 0u32;
        let mut comma_count = 0u32;
        for i in 1..tokens.len() {
            let token = tokens[i].token().token();
            let next_token = tokens.get(i + 1).map(|t| t.token().token());
            let prev_prev_token = if i > 1 {
                tokens.get(i - 2).map(|t| t.token().token())
            } else {
                None
            };
            if token == "." {
                dot_count += 1;
                comma_count = 0;
            } else if token == comma {
                comma_count += 1;
                dot_count = 0;
            }

            if dot_count == 2
                && next_token != Some(".")
                && next_token != Some("\u{2026}")
                && token != "/"
                && next_token != Some("/")
                && token != "\\"
                && next_token != Some("\\")
                && prev_prev_token != Some("?")
                && prev_prev_token != Some("!")
                && prev_prev_token != Some("\u{2026}")
                && prev_prev_token != Some(".")
            {
                let from_pos = tokens[i - 1].token().start();
                let to_pos = tokens[i].token().end();
                let context_start = from_pos.saturating_sub(20);
                let context_end = std::cmp::min(to_pos + 20, sentence.text().len());
                let context_start = context_start.min(context_end);
                let context_text = sentence
                    .text()
                    .get(context_start..context_end)
                    .unwrap_or("")
                    .to_string();
                let rule = RuleMatchRule::new(&self.id, self.description())
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Typographical.as_str());
                matches.push(
                    RuleMatch::new(
                        self.dot_message.clone(),
                        from_pos,
                        to_pos - from_pos,
                        rule,
                        RuleMatchContext::new(
                            context_text,
                            context_start,
                            context_end - context_start,
                        ),
                    )
                    .with_short_message(self.dot_short_message.clone())
                    .with_sentence(sentence.text().to_string())
                    .with_replacements(vec![
                        SuggestedReplacement::new("."),
                        SuggestedReplacement::new("\u{2026}"),
                    ]),
                );
                dot_count = 0;
            } else if comma_count == 2 && next_token != Some(comma.as_str()) {
                let from_pos = tokens[i - 1].token().start();
                let to_pos = tokens[i].token().end();
                let context_start = from_pos.saturating_sub(20);
                let context_end = std::cmp::min(to_pos + 20, sentence.text().len());
                let context_start = context_start.min(context_end);
                let context_text = sentence
                    .text()
                    .get(context_start..context_end)
                    .unwrap_or("")
                    .to_string();
                let rule = RuleMatchRule::new(&self.id, self.description())
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Typographical.as_str());
                matches.push(
                    RuleMatch::new(
                        self.comma_message.clone(),
                        from_pos,
                        to_pos - from_pos,
                        rule,
                        RuleMatchContext::new(
                            context_text,
                            context_start,
                            context_end - context_start,
                        ),
                    )
                    .with_short_message(self.comma_short_message.clone())
                    .with_sentence(sentence.text().to_string())
                    .with_replacements(vec![SuggestedReplacement::new(comma.as_str())]),
                );
                comma_count = 0;
            }

            if token != "." && token != comma {
                dot_count = 0;
                comma_count = 0;
            }
        }

        matches
    }
}

pub struct UppercaseSentenceStartRule {
    id: String,
    category: Category,
}

impl UppercaseSentenceStartRule {
    pub fn new() -> Self {
        Self {
            id: "UPPERCASE_SENTENCE_START".to_string(),
            category: Category::new("CASING", "Capitalization"),
        }
    }
}

impl Default for UppercaseSentenceStartRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for UppercaseSentenceStartRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Sentence should start with an uppercase letter"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Capitalization
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        use std::sync::LazyLock;

        static NUMERALS_EN: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(
                "^(?:[a-z]|(m{0,4}(c[md]|d?c{0,3})(x[cl]|l?x{0,3})(i[xv]|v?i{0,3}))$)",
            )
            .expect("valid NUMERALS_EN pattern")
        });
        static CONTAINS_DIGIT: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r".*\d.*").expect("valid CONTAINS_DIGIT pattern"));
        static ONLY_LOWERCASE_START: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new("^[a-z][A-Z].*$").expect("valid ONLY_LOWERCASE_START pattern")
        });
        static NO_PROTOCOL_URL: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new("^(?:([a-zA-Z0-9][a-zA-Z0-9-]+\\.)?([a-zA-Z0-9][a-zA-Z0-9-]+)\\.([a-zA-Z0-9][a-zA-Z0-9-]+)/.*)$")
                .expect("valid NO_PROTOCOL_URL pattern")
        });
        static E_MAIL: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
            fancy_regex::Regex::new(
                "^(?:@?\\b[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@((\\[[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}\\])|(([a-zA-Z\\-0-9]+\\.)+[a-zA-Z]{2,}))\\b)$",
            )
            .expect("valid E_MAIL pattern")
        });
        static CAMEL_CASE: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new("^[a-z]+[A-Z][A-Za-z]+$").expect("valid CAMEL_CASE pattern")
        });
        const EXCEPTIONS: &[&str] = &[
            "n", "w", "x86", "\u{24d2}", "\u{f8}", "cc", "pH", "heylogin",
        ];
        const PROTOCOLS: &[&str] = &[
            "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel", "sms",
            "git", "ssh", "data", "magnet", "smb", "slack", "spotify",
        ];

        let is_url = |token: &str| {
            PROTOCOLS
                .iter()
                .any(|p| token.starts_with(&format!("{p}://")))
                || token.starts_with("www.")
                || NO_PROTOCOL_URL.is_match(token)
        };
        let is_quote_start = |word: &str| {
            matches!(
                word,
                "\"" | "'"
                    | "\u{201e}"
                    | "\u{bb}"
                    | "\u{ab}"
                    | "\u{201c}"
                    | "\u{2018}"
                    | "\u{a1}"
                    | "\u{bf}"
            )
        };

        let tokens = sentence.non_whitespace_tokens();
        if tokens.len() < 2 || tokens.len() == 2 {
            return Vec::new();
        }

        let first_token_obj = tokens[1];
        let first_token = first_token_obj.token().token();
        let mut match_token_pos = 1usize;
        let mut second_token: Option<&str> = None;
        if tokens.len() >= 3 && is_quote_start(first_token) {
            match_token_pos = 2;
            second_token = Some(tokens[2].token().token());
        }
        let check_token = second_token.unwrap_or(first_token);

        let mut prevent_error = false;
        if CONTAINS_DIGIT.is_match(tokens[match_token_pos].token().token()) {
            prevent_error = true;
        }
        if match_token_pos + 1 < tokens.len() {
            let next = tokens[match_token_pos + 1].token().token();
            if NUMERALS_EN.is_match(tokens[match_token_pos].token().token())
                && (next == "." || next == ")")
            {
                prevent_error = true;
            }
        }
        if is_url(check_token)
            || E_MAIL.is_match(check_token).unwrap_or(false)
            || first_token_obj.is_immunized()
        {
            prevent_error = true;
        }

        if check_token.is_empty() {
            return Vec::new();
        }

        let first_char = check_token.chars().next().unwrap_or('\0');
        let capitalized = match first_char.to_uppercase().next() {
            Some(upper) => format!("{}{}", upper, &check_token[first_char.len_utf8()..]),
            None => check_token.to_string(),
        };

        if capitalized != check_token
            && !prevent_error
            && first_char.is_lowercase()
            && !ONLY_LOWERCASE_START.is_match(check_token)
            && !EXCEPTIONS.contains(&check_token)
            && !CAMEL_CASE.is_match(check_token)
        {
            let target = tokens[match_token_pos];
            let start = target.token().start();
            let end = target.token().end();
            let text_len = sentence.text().len();
            let mut context_start = start.saturating_sub(20);
            while context_start > 0 && !sentence.text().is_char_boundary(context_start) {
                context_start -= 1;
            }
            let mut context_end = std::cmp::min(end + 20, text_len);
            while context_end < text_len && !sentence.text().is_char_boundary(context_end) {
                context_end += 1;
            }
            let context_text = sentence.text()[context_start..context_end].to_string();

            let rule = RuleMatchRule::new(&self.id, self.description())
                .with_category(self.category.clone())
                .with_issue_type(IssueType::Capitalization.as_str());

            return vec![
                RuleMatch::new(
                    "This sentence does not start with an uppercase letter.",
                    start,
                    end - start,
                    rule,
                    RuleMatchContext::new(context_text, context_start, context_end - context_start),
                )
                .with_sentence(sentence.text().to_string())
                .with_replacements(vec![SuggestedReplacement::new(capitalized)]),
            ];
        }

        Vec::new()
    }
}

pub struct CommaWhitespaceRule {
    id: String,
    category: Category,
    domain_after_dot_exception: bool,
    comma_character: char,
}

impl CommaWhitespaceRule {
    pub fn new() -> Self {
        Self {
            id: "COMMA_PARENTHESIS_WHITESPACE".to_string(),
            category: Category::new("TYPOGRAPHY", "Typography"),
            domain_after_dot_exception: false,
            comma_character: ',',
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_comma_character(mut self, comma: char) -> Self {
        self.comma_character = comma;
        self
    }

    pub fn with_domain_after_dot_exception(mut self) -> Self {
        self.domain_after_dot_exception = true;
        self
    }

    fn is_exception(&self, tokens: &[og_core::AnalyzedTokenReadings], i: usize) -> bool {
        if !self.domain_after_dot_exception {
            return false;
        }
        static DOMAINS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            regex::Regex::new("^[a-z]{2,10}-Domains?$").expect("valid DOMAINS pattern")
        });
        tokens[i].token().token() == "."
            && tokens
                .get(i + 1)
                .is_some_and(|t| DOMAINS.is_match(t.token().token()))
    }
}

impl Default for CommaWhitespaceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for CommaWhitespaceRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Whitespace around comma/parenthesis"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Whitespace
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let tokens = sentence.tokens();

        let mut prev_token = "";
        let mut prev_prev_token = "";
        let mut prev_was_whitespace = false;

        for i in 0..tokens.len() {
            let token_reading = &tokens[i];
            let token = token_reading.token().token();
            let is_whitespace = token_reading.is_whitespace();

            let mut msg: Option<String> = None;
            let mut suggestion: Option<String> = None;

            if is_whitespace && is_left_bracket(prev_token) {
                msg = Some("Don't put a space after an opening bracket.".to_string());
                suggestion = Some(prev_token.to_string());
            } else if !is_whitespace
                && !prev_was_whitespace
                && prev_token == self.comma_character.to_string()
                && !is_quote_char(token)
                && !is_hyphen_or_comma(token)
                && !contains_digit(prev_prev_token)
                && !contains_digit(token)
                && prev_prev_token != self.comma_character.to_string()
            {
                msg = Some("Put a space after a comma.".to_string());
                suggestion = Some(format!("{} {}", self.comma_character, token));
            } else if prev_was_whitespace {
                if is_right_bracket(token) {
                    msg = Some("Don't put a space before a closing bracket.".to_string());
                    suggestion = Some(token.to_string());
                } else if token == self.comma_character.to_string() {
                    if i + 1 < tokens.len()
                        && tokens[i + 1].token().token() == self.comma_character.to_string()
                    {
                    } else {
                        msg = Some("Don't put a space before a comma.".to_string());
                        let next_space = if i + 1 < tokens.len() && !tokens[i + 1].is_whitespace() {
                            format!("{} ", self.comma_character)
                        } else {
                            self.comma_character.to_string()
                        };
                        suggestion = Some(next_space);
                    }
                } else if token == "." {
                    let next_is_digit_or_dot =
                        i + 1 < tokens.len() && is_digit_or_dot(tokens[i + 1].token().token());
                    let next_is_slash_alpha = i + 2 < tokens.len()
                        && tokens[i + 1].token().token() == "/"
                        && tokens[i + 2]
                            .token()
                            .token()
                            .chars()
                            .any(|c| c.is_alphabetic());
                    if !next_is_digit_or_dot && !next_is_slash_alpha {
                        msg = Some("Don't put a space before a period.".to_string());
                        suggestion = Some(".".to_string());
                    }
                }
            }

            if let Some(m) = msg
                && !self.is_exception(tokens, i)
                && !token_reading.is_immunized()
            {
                let from_pos = if i >= 1 {
                    tokens[i - 1].token().start()
                } else {
                    0
                };
                let to_pos = token_reading.token().end();

                if let Some(ref s) = suggestion {
                    let marked =
                        if to_pos <= sentence.text().len() && from_pos <= sentence.text().len() {
                            &sentence.text()[from_pos..to_pos]
                        } else {
                            ""
                        };
                    if marked == s.as_str() {
                        prev_prev_token = prev_token;
                        prev_token = token;
                        prev_was_whitespace = is_whitespace;
                        continue;
                    }
                }

                let text_len = sentence.text().len();
                let mut context_start = from_pos.saturating_sub(30);
                while context_start > 0 && !sentence.text().is_char_boundary(context_start) {
                    context_start -= 1;
                }
                let mut context_end = std::cmp::min(to_pos + 30, text_len);
                while context_end < text_len && !sentence.text().is_char_boundary(context_end) {
                    context_end += 1;
                }
                let context_text = sentence.text()[context_start..context_end].to_string();

                let rule = RuleMatchRule::new(&self.id, self.description())
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Whitespace.as_str());

                let mut m = RuleMatch::new(
                    m,
                    from_pos,
                    to_pos - from_pos,
                    rule,
                    RuleMatchContext::new(context_text, context_start, context_end - context_start),
                )
                .with_sentence(sentence.text().to_string());

                if let Some(s) = suggestion {
                    m = m.with_replacements(vec![SuggestedReplacement::new(s)]);
                }

                matches.push(m);
            }

            prev_prev_token = prev_token;
            prev_token = token;
            prev_was_whitespace = is_whitespace;
        }

        matches
    }
}

fn is_left_bracket(s: &str) -> bool {
    s == "(" || s == "[" || s == "{"
}

fn is_right_bracket(s: &str) -> bool {
    s == ")" || s == "]" || s == "}"
}

fn is_quote_char(s: &str) -> bool {
    matches!(
        s,
        "'" | "\"" | "\u{2019}" | "\u{201D}" | "\u{201C}" | "\u{AB}" | "\u{BB}"
    )
}

fn is_hyphen_or_comma(s: &str) -> bool {
    s == "-" || s == ","
}

fn is_digit_or_dot(s: &str) -> bool {
    s.starts_with('.') || s.starts_with(|c: char| c.is_ascii_digit())
}

fn contains_digit(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_digit())
}

pub struct WhitespaceBeforePunctuationRule {
    id: String,
    category: Category,
}

impl WhitespaceBeforePunctuationRule {
    pub fn new() -> Self {
        Self {
            id: "WHITESPACE_PUNCTUATION".to_string(),
            category: Category::new("TYPOGRAPHY", "Typography"),
        }
    }
}

impl Default for WhitespaceBeforePunctuationRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for WhitespaceBeforePunctuationRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Whitespace before punctuation"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Whitespace
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let tokens = sentence.tokens();

        let mut prev_white = false;
        let mut prev_len = 0usize;
        for (i, token_reading) in tokens.iter().enumerate() {
            let token = token_reading.token().token();
            let is_whitespace =
                token_reading.is_whitespace() || token == "\u{a0}" || token == "\u{202f}";

            let mut msg: Option<&str> = None;
            let mut suggestion: Option<&str> = None;
            if prev_white {
                match token {
                    ":" => {
                        msg = Some("No space before colon.");
                        suggestion = Some(":");
                        if i + 2 < tokens.len()
                            && tokens[i + 1].is_whitespace()
                            && tokens[i + 2]
                                .token()
                                .token()
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_numeric())
                        {
                            msg = None;
                        }
                    }
                    ";" => {
                        msg = Some("No space before semicolon.");
                        suggestion = Some(";");
                    }
                    "%" if i > 1 => {
                        let prev_prev = tokens[i - 2].token().token();
                        if prev_prev.chars().next().is_some_and(|c| c.is_numeric()) {
                            msg = Some("No space before percentage sign.");
                            suggestion = Some("%");
                        }
                    }
                    _ => {}
                }
            }
            if let (Some(msg), Some(suggestion)) = (msg, suggestion) {
                let from = tokens[i - 1].token().start();
                let to = from + 1 + prev_len;
                let text_len = sentence.text().len();
                let mut context_start = from.saturating_sub(20);
                while context_start > 0 && !sentence.text().is_char_boundary(context_start) {
                    context_start -= 1;
                }
                let mut context_end = std::cmp::min(to + 20, text_len);
                while context_end < text_len && !sentence.text().is_char_boundary(context_end) {
                    context_end += 1;
                }
                let context_text = sentence.text()[context_start..context_end].to_string();

                let rule = RuleMatchRule::new(&self.id, self.description())
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Whitespace.as_str());

                matches.push(
                    RuleMatch::new(
                        msg,
                        from,
                        to - from,
                        rule,
                        RuleMatchContext::new(
                            context_text,
                            context_start,
                            context_end - context_start,
                        ),
                    )
                    .with_sentence(sentence.text().to_string())
                    .with_replacements(vec![SuggestedReplacement::new(suggestion)]),
                );
            }
            prev_white = is_whitespace;
            prev_len = token.len();
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::rule::Rule;

    fn make_sentence(text: &str) -> AnalyzedSentence {
        use og_core::{AnalyzedToken, AnalyzedTokenReadings};
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut pos = 0;
        for word in text.split_whitespace() {
            let start = text[pos..].find(word).unwrap() + pos;
            let end = start + word.len();
            let at = AnalyzedToken::new(word, start, end);
            tokens.push(AnalyzedTokenReadings::new(at));
            pos = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    fn make_sentence_with_start(text: &str) -> AnalyzedSentence {
        use og_core::{AnalyzedToken, AnalyzedTokenReadings};
        let mut sentence = make_sentence(text);
        let tokens = sentence.tokens_mut();
        tokens.insert(
            0,
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
        );
        sentence
    }

    fn make_punct_sentence(text: &str) -> AnalyzedSentence {
        use og_core::{AnalyzedToken, AnalyzedTokenReadings};
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let punct_chars = ",.()[]{}!?;:";
        while i < chars.len() {
            if chars[i].is_whitespace() {
                let start = i;
                let mut end = i + 1;
                while end < chars.len() && chars[end].is_whitespace() {
                    end += 1;
                }
                let ws: String = chars[start..end].iter().collect();
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    &ws, start, end,
                )));
                i = end;
            } else if punct_chars.contains(chars[i]) {
                let ch = chars[i].to_string();
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    &ch,
                    i,
                    i + 1,
                )));
                i += 1;
            } else {
                let start = i;
                let mut end = i + 1;
                while end < chars.len()
                    && !chars[end].is_whitespace()
                    && !punct_chars.contains(chars[end])
                {
                    end += 1;
                }
                let word: String = chars[start..end].iter().collect();
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    &word, start, end,
                )));
                i = end;
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn test_word_repeat_rule() {
        let rule = WordRepeatRule::new();
        let sentence = make_sentence("the the quick brown fox");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message(), "Possible word repetition: 'the'");
    }

    #[test]
    fn test_word_repeat_no_false_positive() {
        let rule = WordRepeatRule::new();
        let sentence = make_sentence("the quick brown fox");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_word_repeat_case_insensitive() {
        let rule = WordRepeatRule::new();
        let sentence = make_sentence("The the quick brown fox");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "Case-insensitive repeat");
    }

    #[test]
    fn test_word_repeat_three_times() {
        let rule = WordRepeatRule::new();
        let sentence = make_sentence("the the the quick");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 2, "Three repeats = two matches");
    }

    #[test]
    fn test_word_repeat_no_pure_numbers() {
        let rule = WordRepeatRule::new();
        let sentence = make_sentence("1 1 2 2");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "Number repetition should not trigger");
    }

    #[test]
    fn test_double_period() {
        let rule = DoublePunctuationRule::new();
        let sentence = make_punct_sentence("Hello..");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "Double period should trigger");
    }

    #[test]
    fn test_double_comma() {
        let rule = DoublePunctuationRule::new();
        let sentence = make_punct_sentence("Hello,,");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_double_exclamation() {
        let rule = DoublePunctuationRule::new();
        let sentence = make_punct_sentence("Hello!!");
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches.is_empty(),
            "Java flags only dots and commas, not '!!'"
        );
    }

    #[test]
    fn test_triple_period_is_ellipsis() {
        let rule = DoublePunctuationRule::new();
        let sentence = make_punct_sentence("Hello...");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "Java flags '..' but never '...'");
    }

    #[test]
    fn test_single_punctuation_ok() {
        let rule = DoublePunctuationRule::new();
        let sentence = make_punct_sentence("Hello.");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "Single punctuation should not trigger");
    }

    #[test]
    fn test_uppercase_sentence_start() {
        let rule = UppercaseSentenceStartRule::new();
        let sentence = make_sentence_with_start("hello world again");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "Hello");
    }

    #[test]
    fn test_uppercase_sentence_start_single_word_skipped() {
        let rule = UppercaseSentenceStartRule::new();
        let sentence = make_sentence_with_start("hello");
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn test_uppercase_sentence_start_already_capital() {
        let rule = UppercaseSentenceStartRule::new();
        let sentence = make_sentence_with_start("Hello world");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_uppercase_sentence_start_number_ok() {
        let rule = UppercaseSentenceStartRule::new();
        let sentence = make_sentence_with_start("123 people");
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches.is_empty(),
            "Starting with number should not trigger"
        );
    }

    #[test]
    fn test_uppercase_sentence_start_url_and_email_ok() {
        let rule = UppercaseSentenceStartRule::new();
        for text in [
            "www.example.com is a site",
            "example.org/path works too",
            "user@mail.org wrote",
            "eBay is a marketplace",
        ] {
            let sentence = make_sentence_with_start(text);
            assert!(
                rule.match_sentence(&sentence).is_empty(),
                "should not flag '{text}'"
            );
        }
    }

    #[test]
    fn test_uppercase_sentence_start_enumeration_ok() {
        let rule = UppercaseSentenceStartRule::new();
        for text in ["a) first item", "iv. second item"] {
            use og_core::{AnalyzedToken, AnalyzedTokenReadings};
            let mut sentence = make_punct_sentence(text);
            let tokens = sentence.tokens_mut();
            tokens.insert(
                0,
                AnalyzedTokenReadings::new(
                    AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
                ),
            );
            assert!(
                rule.match_sentence(&sentence).is_empty(),
                "should not flag '{text}'"
            );
        }
    }

    #[test]
    fn test_space_before_comma() {
        let rule = CommaWhitespaceRule::new();
        let sentence = make_punct_sentence("Hello , world");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Space before comma should trigger");
    }

    #[test]
    fn test_no_space_after_comma() {
        let rule = CommaWhitespaceRule::new();
        let sentence = make_punct_sentence("Hello,world");
        let matches = rule.match_sentence(&sentence);
        assert!(
            !matches.is_empty(),
            "Missing space after comma should trigger"
        );
    }

    #[test]
    fn test_correct_comma_spacing() {
        let rule = CommaWhitespaceRule::new();
        let sentence = make_punct_sentence("Hello, world");
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches.is_empty(),
            "Correct comma spacing should not trigger"
        );
    }

    #[test]
    fn test_space_after_opening_bracket() {
        let rule = CommaWhitespaceRule::new();
        let sentence = make_punct_sentence("Hello ( world)");
        let matches = rule.match_sentence(&sentence);
        assert!(
            !matches.is_empty(),
            "Space after opening bracket should trigger"
        );
    }

    #[test]
    fn test_space_before_closing_bracket() {
        let rule = CommaWhitespaceRule::new();
        let sentence = make_punct_sentence("Hello (world )");
        let matches = rule.match_sentence(&sentence);
        assert!(
            !matches.is_empty(),
            "Space before closing bracket should trigger"
        );
    }

    #[test]
    fn test_correct_bracket_spacing() {
        let rule = CommaWhitespaceRule::new();
        let sentence = make_punct_sentence("Hello (world)");
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches.is_empty(),
            "Correct bracket spacing should not trigger"
        );
    }

    #[test]
    fn test_space_before_period() {
        let rule = CommaWhitespaceRule::new();
        let sentence = make_punct_sentence("Hello .");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Space before period should trigger");
    }

    #[test]
    fn test_multiple_comma_issues() {
        let rule = CommaWhitespaceRule::new();
        let sentence = make_punct_sentence("Hello ,world");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Space before comma and no space after");
    }
    #[test]
    fn whitespace_before_punctuation_flags_colon_semicolon_percent() {
        let rule = WhitespaceBeforePunctuationRule::new();
        assert_eq!(rule.id(), "WHITESPACE_PUNCTUATION");
        let s = make_punct_sentence("word : more");
        assert!(
            !rule.match_sentence(&s).is_empty(),
            "space before colon is flagged"
        );
        let s = make_punct_sentence("word ; more");
        assert!(
            !rule.match_sentence(&s).is_empty(),
            "space before semicolon is flagged"
        );
        let s = make_punct_sentence("50 %");
        assert!(
            !rule.match_sentence(&s).is_empty(),
            "space before percentage after a number is flagged"
        );
        let s = make_punct_sentence("ratio : 2");
        assert!(
            rule.match_sentence(&s).is_empty(),
            "colon before a digit is tolerated"
        );
        let clean = make_punct_sentence("word: more; 50%.");
        assert!(rule.match_sentence(&clean).is_empty());
    }

    #[test]
    fn comma_whitespace_period_arms_and_domain_exception() {
        let rule = CommaWhitespaceRule::new();
        let s = make_punct_sentence("end .");
        assert!(
            !rule.match_sentence(&s).is_empty(),
            "space before period is flagged"
        );
        let s = make_punct_sentence("end .5");
        assert!(
            rule.match_sentence(&s).is_empty(),
            "digit after period is tolerated"
        );
        let with_exception = CommaWhitespaceRule::new().with_domain_after_dot_exception();
        let s = make_punct_sentence("end .web-Domains");
        assert!(
            !rule.match_sentence(&s).is_empty(),
            "plain rule flags the period"
        );
        assert!(
            with_exception.match_sentence(&s).is_empty(),
            "domain exception suppresses the period match"
        );
    }

    #[test]
    fn builders_override_ids_messages_and_flags() {
        let repeat = WordRepeatRule::new()
            .with_id("CUSTOM_REPEAT")
            .with_ignored_words(&["very"])
            .with_ignore_single_letter_repeats();
        assert_eq!(repeat.id(), "CUSTOM_REPEAT");
        let s = make_sentence("very very good");
        assert!(
            repeat.match_sentence(&s).is_empty(),
            "ignored words skip repeats"
        );
        let letters = make_sentence("a a");
        assert!(
            repeat.match_sentence(&letters).is_empty(),
            "single letter repeats skipped"
        );
        let plain = WordRepeatRule::new();
        assert!(!plain.match_sentence(&letters).is_empty());

        let punct = DoublePunctuationRule::new()
            .with_id("CUSTOM_DP")
            .with_description("custom")
            .with_comma_character(';')
            .with_messages("d", "ds", "c", "cs");
        assert_eq!(punct.id(), "CUSTOM_DP");
        assert_eq!(punct.description(), "custom");
        let s = make_punct_sentence("word;; more");
        assert!(
            !punct.match_sentence(&s).is_empty(),
            "custom comma character flagged"
        );

        let comma = CommaWhitespaceRule::new()
            .with_id("CUSTOM_CW")
            .with_comma_character(';');
        assert_eq!(comma.id(), "CUSTOM_CW");
        let s = make_punct_sentence("word ; ; more");
        assert!(
            !comma.match_sentence(&s).is_empty(),
            "custom comma pair flagged"
        );
    }

    #[test]
    fn native_rules_expose_java_metadata() {
        let repeat = WordRepeatRule::default();
        assert_eq!(repeat.id(), "WORD_REPEAT_RULE");
        assert_eq!(repeat.description(), "Word repetition");
        assert_eq!(repeat.category().id().as_str(), "MISC");
        assert_eq!(repeat.issue_type().as_str(), "redundancy");

        let double = DoublePunctuationRule::default();
        assert_eq!(double.id(), "DOUBLE_PUNCTUATION");
        assert_eq!(double.category().id().as_str(), "PUNCTUATION");
        assert_eq!(double.issue_type().as_str(), "typographical");

        let upper = UppercaseSentenceStartRule::default();
        assert_eq!(upper.id(), "UPPERCASE_SENTENCE_START");
        assert_eq!(upper.category().id().as_str(), "CASING");
        assert_eq!(upper.issue_type().as_str(), "capitalization");

        let comma = CommaWhitespaceRule::default();
        assert_eq!(comma.id(), "COMMA_PARENTHESIS_WHITESPACE");
        assert_eq!(comma.description(), "Whitespace around comma/parenthesis");
        assert_eq!(comma.category().id().as_str(), "TYPOGRAPHY");
        assert_eq!(comma.issue_type().as_str(), "whitespace");

        let before = WhitespaceBeforePunctuationRule::default();
        assert_eq!(before.id(), "WHITESPACE_PUNCTUATION");
        assert_eq!(before.description(), "Whitespace before punctuation");
        assert_eq!(before.category().id().as_str(), "TYPOGRAPHY");
        assert_eq!(before.issue_type().as_str(), "whitespace");
    }

    #[test]
    fn word_repeat_rule_skips_ignored_words_and_multibyte_context() {
        let rule = WordRepeatRule::new().with_ignored_words(&["very", "so"]);
        let sentence = make_sentence("very very good");
        assert!(rule.match_sentence(&sentence).is_empty());

        let prefix = "é".repeat(30);
        let text = format!("{prefix} word word {prefix}");
        let sentence = make_sentence(&text);
        let matches = WordRepeatRule::new().match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].context().text().starts_with('é'));
    }

    #[test]
    fn uppercase_sentence_start_rule_skips_protected_starts() {
        let rule = UppercaseSentenceStartRule::new();
        assert!(
            rule.match_sentence(&make_sentence_with_start("a fine day it is ."))
                .len()
                > 0,
            "lowercase sentence start is flagged"
        );
        for text in [
            "\u{201e}alles gut hier .",
            "http://example.org is down .",
            "www.example.org is down .",
            "john@example.org writes .",
            "iPhone sales rise .",
        ] {
            let sentence = make_sentence_with_start(text);
            assert!(
                rule.match_sentence(&sentence).is_empty(),
                "{text} should not be flagged"
            );
        }

        let mut numeral = make_punct_sentence("iii. something .");
        numeral.tokens_mut().insert(
            0,
            og_core::AnalyzedTokenReadings::new(
                og_core::AnalyzedToken::new("<S>", 0, 0)
                    .with_pos_tags(vec!["SENT_START".to_string()]),
            ),
        );
        assert!(
            rule.match_sentence(&numeral).is_empty(),
            "roman numeral with period is fine"
        );

        let mut immunized = make_sentence_with_start("a fine day .");
        immunized.tokens_mut()[1].immunize();
        assert!(rule.match_sentence(&immunized).is_empty());
    }

    #[test]
    fn comma_whitespace_rule_skips_domains_and_double_commas() {
        let rule = CommaWhitespaceRule::new().with_domain_after_dot_exception();
        let sentence = make_punct_sentence("word .my-Domains more");
        assert!(
            rule.match_sentence(&sentence).is_empty(),
            "'. Domains' is an accepted abbreviation pattern"
        );

        let sentence = make_punct_sentence("word ,, more");
        assert!(
            rule.match_sentence(&sentence).is_empty(),
            "adjacent commas are handled by DoublePunctuation, not here"
        );

        let text = "word ./tree more";
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        sentence.set_tokens(
            ["word", " ", ".", "/", "tree", " ", "more"]
                .iter()
                .scan(0usize, |pos, piece| {
                    let start = *pos;
                    *pos += piece.len();
                    Some(og_core::AnalyzedTokenReadings::new(
                        og_core::AnalyzedToken::new(*piece, start, *pos),
                    ))
                })
                .collect(),
        );
        assert!(
            rule.match_sentence(&sentence).is_empty(),
            "a period directly before a slash path keeps the spacing"
        );
    }

    #[test]
    fn whitespace_before_punctuation_rule_flags_colon_semicolon_percent() {
        let rule = WhitespaceBeforePunctuationRule::new();
        let sentence = make_punct_sentence("word : more");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message(), "No space before colon.");

        let sentence = make_punct_sentence("word ; more");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message(), "No space before semicolon.");

        let sentence = make_punct_sentence("50 % more");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message(), "No space before percentage sign.");

        assert!(
            rule.match_sentence(&make_punct_sentence("word: more"))
                .is_empty()
        );
        let ratio = make_punct_sentence("word : 4 more");
        assert!(
            rule.match_sentence(&ratio).is_empty(),
            "colon before a ratio number is fine"
        );
    }
}
