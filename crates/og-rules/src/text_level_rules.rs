use og_core::rule::TextLevelRule;
use og_core::{AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule, SuggestedReplacement};

/// Detects multiple consecutive whitespace characters that should be a single space.
/// Text-level rule that checks across all sentences.
pub struct MultipleWhitespaceRule {
    id: String,
    category: Category,
}

impl MultipleWhitespaceRule {
    pub fn new() -> Self {
        Self {
            id: "WHITESPACE_RULE".to_string(),
            category: Category::new("TYPOGRAPHY", "Typography"),
        }
    }
}

impl Default for MultipleWhitespaceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for MultipleWhitespaceRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Whitespace repetition"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Whitespace
    }

    fn match_text(&self, text: &str, _sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let bytes = text.as_bytes();
        let mut i = 0;

        while i < text.len() {
            if bytes[i] == b' ' {
                let start = i;
                while i < text.len() && bytes[i] == b' ' {
                    i += 1;
                }
                let end = i;
                if end - start > 1 {
                    let context_start = if start >= 30 { start - 30 } else { 0 };
                    let context_end = std::cmp::min(end + 30, text.len());
                    let context_text = text[context_start..context_end].to_string();

                    let rule = RuleMatchRule::new(&self.id, self.description())
                        .with_category(self.category.clone())
                        .with_issue_type(IssueType::Whitespace.as_str());

                    matches.push(
                        RuleMatch::new(
                            "There is duplicated whitespace.".to_string(),
                            start,
                            end - start,
                            rule,
                            RuleMatchContext::new(context_text, context_start, context_end - context_start),
                        )
                        .with_replacements(vec![SuggestedReplacement::new(" ")])
                    );
                }
            } else {
                i += 1;
            }
        }

        matches
    }
}

/// Detects missing whitespace between sentences.
/// Text-level rule that checks cross-sentence boundaries.
pub struct SentenceWhitespaceRule {
    id: String,
    category: Category,
}

impl SentenceWhitespaceRule {
    pub fn new() -> Self {
        Self {
            id: "SENTENCE_WHITESPACE".to_string(),
            category: Category::new("TYPOGRAPHY", "Typography"),
        }
    }
}

impl Default for SentenceWhitespaceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for SentenceWhitespaceRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Missing space between sentences"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Whitespace
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        if sentences.len() < 2 {
            return matches;
        }

        let mut pos = 0usize;
        for i in 1..sentences.len() {
            let prev_end = pos + sentences[i - 1].text().len();
            let curr_text = sentences[i].text();

            let prev_sentence_text = sentences[i - 1].text();
            let ends_with_whitespace = prev_sentence_text
                .chars()
                .last()
                .map(|c| c.is_whitespace())
                .unwrap_or(false);

            if !ends_with_whitespace && !curr_text.starts_with(|c: char| c.is_whitespace()) {
                let start = prev_end;
                let first_char_len = curr_text.chars().next().map(|c| c.len_utf8()).unwrap_or(0);
                let end = prev_end + first_char_len;
                let context_start = if start >= 30 { start - 30 } else { 0 };
                let context_end = std::cmp::min(end + 30, text.len());
                let context_text = text[context_start..context_end].to_string();

                let first_word: String = curr_text.chars().take_while(|c| !c.is_whitespace()).collect();
                let suggestion = format!(" {}", first_word);

                let rule = RuleMatchRule::new(&self.id, self.description())
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Whitespace.as_str());

                matches.push(
                    RuleMatch::new(
                        "Add a space between sentences.".to_string(),
                        start,
                        end - start,
                        rule,
                        RuleMatchContext::new(context_text, context_start, context_end - context_start),
                    )
                    .with_sentence(curr_text.to_string())
                    .with_replacements(vec![SuggestedReplacement::new(suggestion)])
                );
            }

            pos += sentences[i - 1].text().len();
        }

        matches
    }
}

/// Detects unpaired brackets [] {} () using stack-based pairing.
/// Text-level rule that tracks opening/closing symbols across sentences.
pub struct GenericUnpairedBracketsRule {
    id: String,
    category: Category,
    start_symbols: Vec<String>,
    end_symbols: Vec<String>,
}

impl GenericUnpairedBracketsRule {
    pub fn new() -> Self {
        Self {
            id: "UNPAIRED_BRACKETS".to_string(),
            category: Category::new("PUNCTUATION", "Punctuation"),
            start_symbols: vec!["[".to_string(), "(".to_string(), "{".to_string()],
            end_symbols: vec!["]".to_string(), ")".to_string(), "}".to_string()],
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_symbols(mut self, start: Vec<&str>, end: Vec<&str>) -> Self {
        assert_eq!(start.len(), end.len(), "Start and end symbol lists must be equal length");
        self.start_symbols = start.iter().map(|s| s.to_string()).collect();
        self.end_symbols = end.iter().map(|s| s.to_string()).collect();
        self
    }
}

impl Default for GenericUnpairedBracketsRule {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct SymbolEntry {
    symbol: String,
    is_opening: bool,
    start_pos: usize,
    #[allow(dead_code)]
    sentence_idx: usize,
}

impl TextLevelRule for GenericUnpairedBracketsRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Unpaired brackets"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("other".to_string())
    }

    fn match_text(&self, text: &str, _sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut stack: Vec<SymbolEntry> = Vec::new();

        // Build all bracket symbols for fast lookup
        let _all_symbols: Vec<&str> = self.start_symbols.iter()
            .chain(self.end_symbols.iter())
            .map(|s| s.as_str())
            .collect();

        let mut chars = text.char_indices().peekable();
        while let Some((byte_idx, ch)) = chars.next() {
            let ch_str = ch.to_string();

            // Skip smiley patterns: :) :( ;) ;-(
            if ch == ')' || ch == '(' {
                if byte_idx >= 1 {
                    let prev = &text[byte_idx - 1..byte_idx];
                    if prev == ":" || prev == ";" {
                        continue;
                    }
                }
                if byte_idx >= 2 {
                    let prev2 = &text[byte_idx - 2..byte_idx - 1];
                    let prev1 = &text[byte_idx - 1..byte_idx];
                    if prev1 == "-" && (prev2 == ":" || prev2 == ";") {
                        continue;
                    }
                }
            }

            for j in 0..self.start_symbols.len() {
                if ch_str == self.start_symbols[j] {
                    stack.push(SymbolEntry {
                        symbol: self.start_symbols[j].clone(),
                        is_opening: true,
                        start_pos: byte_idx,
                        sentence_idx: 0,
                    });
                    break;
                }
                if ch_str == self.end_symbols[j] {
                    if let Some(idx) = stack.iter().rposition(|s| {
                        s.is_opening && s.symbol == self.start_symbols[j]
                    }) {
                        stack.remove(idx);
                    } else {
                        stack.push(SymbolEntry {
                            symbol: self.end_symbols[j].clone(),
                            is_opening: false,
                            start_pos: byte_idx,
                            sentence_idx: 0,
                        });
                    }
                    break;
                }
            }
        }

        // Report all unpaired symbols
        for entry in &stack {
            let start = entry.start_pos;
            let end = start + entry.symbol.len();
            let context_start = if start >= 30 { start - 30 } else { 0 };
            let context_end = std::cmp::min(end + 30, text.len());
            let context_text = text[context_start..context_end].to_string();

            let other = if entry.is_opening {
                let idx = self.start_symbols.iter().position(|s| s == &entry.symbol).unwrap();
                self.end_symbols[idx].clone()
            } else {
                let idx = self.end_symbols.iter().position(|s| s == &entry.symbol).unwrap();
                self.start_symbols[idx].clone()
            };

            let rule = RuleMatchRule::new(&self.id, self.description())
                .with_category(self.category.clone())
                .with_issue_type(IssueType::Other("other".to_string()).as_str());

            matches.push(
                RuleMatch::new(
                    format!("Unpaired '{}' (missing '{}')", entry.symbol, other),
                    start,
                    end - start,
                    rule,
                    RuleMatchContext::new(context_text, context_start, context_end - context_start),
                )
                .with_sentence("".to_string())
                .with_replacements(vec![SuggestedReplacement::new(entry.symbol.clone())])
            );
        }

        matches
    }
}

/// Detects unpaired quotes using stack-based pairing.
pub struct GenericUnpairedQuotesRule {
    id: String,
    category: Category,
    quote_symbols: Vec<(String, String)>,
}

impl GenericUnpairedQuotesRule {
    pub fn new() -> Self {
        Self {
            id: "UNPAIRED_QUOTES".to_string(),
            category: Category::new("PUNCTUATION", "Punctuation"),
            quote_symbols: vec![
                ("\u{201C}".to_string(), "\u{201D}".to_string()), // "" smart double
                ("\u{2018}".to_string(), "\u{2019}".to_string()), // '' smart single
                ("\"".to_string(), "\"".to_string()),              // regular double
            ],
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
}

impl Default for GenericUnpairedQuotesRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for GenericUnpairedQuotesRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Unpaired quotes"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("other".to_string())
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut open_counts: Vec<usize> = vec![0; self.quote_symbols.len()];
        let mut open_positions: Vec<Vec<usize>> = vec![Vec::new(); self.quote_symbols.len()];
        let mut pos = 0usize;

        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();

            for token_reading in &tokens {
                let token_text = token_reading.token().token();
                let token_start = pos + token_reading.token().start();

                for (qi, (open_q, close_q)) in self.quote_symbols.iter().enumerate() {
                    if open_q == close_q {
                        // Same symbol for open/close (e.g. regular double quote)
                        if token_text.contains(open_q.as_str()) {
                            if open_counts[qi] % 2 == 0 {
                                open_positions[qi].push(token_start);
                            }
                            open_counts[qi] += 1;
                        }
                    } else {
                        if token_text.contains(open_q.as_str()) {
                            open_positions[qi].push(token_start);
                            open_counts[qi] += 1;
                        }
                        if token_text.contains(close_q.as_str()) {
                            if open_counts[qi] > 0 {
                                open_positions[qi].pop();
                                open_counts[qi] -= 1;
                            } else {
                                // Unpaired closing quote
                                let start = token_start;
                                let end = start + close_q.len();
                                let context_start = if start >= 30 { start - 30 } else { 0 };
                                let context_end = std::cmp::min(end + 30, text.len());
                                let context_text = text[context_start..context_end].to_string();

                                let rule = RuleMatchRule::new(&self.id, self.description())
                                    .with_category(self.category.clone())
                                    .with_issue_type(IssueType::Other("other".to_string()).as_str());

                                matches.push(
                                    RuleMatch::new(
                                        format!("Unpaired closing quote '{}'", close_q),
                                        start,
                                        end - start,
                                        rule,
                                        RuleMatchContext::new(context_text, context_start, context_end - context_start),
                                    )
                                    .with_sentence(sentence.text().to_string())
                                );
                            }
                        }
                    }
                }
            }
            pos += sentence.text().len();
        }

        // Report unpaired opening quotes
        for (qi, positions) in open_positions.iter().enumerate() {
            let (open_q, _) = &self.quote_symbols[qi];
            for &start_pos in positions {
                let end = start_pos + open_q.len();
                let context_start = if start_pos >= 30 { start_pos - 30 } else { 0 };
                let context_end = std::cmp::min(end + 30, text.len());
                let context_text = text[context_start..context_end].to_string();

                let rule = RuleMatchRule::new(&self.id, self.description())
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Other("other".to_string()).as_str());

                matches.push(
                    RuleMatch::new(
                        format!("Unpaired opening quote '{}'", open_q),
                        start_pos,
                        end - start_pos,
                        rule,
                        RuleMatchContext::new(context_text, context_start, context_end - context_start),
                    )
                    .with_sentence("".to_string())
                );
            }
        }

        matches
    }
}

/// Warns when sentences exceed a configurable word count.
pub struct LongSentenceRule {
    id: String,
    category: Category,
    max_words: usize,
}

impl LongSentenceRule {
    pub fn new(max_words: usize) -> Self {
        Self {
            id: "TOO_LONG_SENTENCE".to_string(),
            category: Category::new("STYLE", "Style"),
            max_words,
        }
    }
}

impl Default for LongSentenceRule {
    fn default() -> Self {
        Self::new(40)
    }
}

impl TextLevelRule for LongSentenceRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Sentence is too long"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn is_default_on(&self) -> bool {
        false // Only on in picky mode
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut pos = 0usize;

        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            let word_count = tokens.iter()
                .filter(|t| t.token().token().chars().any(|c| c.is_alphabetic()))
                .count();

            if word_count > self.max_words {
                // Find the first and last word token for the match range
                let first_word = tokens.iter().find(|t|
                    t.token().token().chars().any(|c| c.is_alphabetic())
                );
                let last_word = tokens.iter().rev().find(|t|
                    t.token().token().chars().any(|c| c.is_alphabetic())
                );

                if let (Some(first), Some(last)) = (first_word, last_word) {
                    let start = pos + first.token().start();
                    let end = pos + last.token().end();
                    let context_start = if start >= 30 { start - 30 } else { 0 };
                    let context_end = std::cmp::min(end + 30, text.len());
                    let context_text = text[context_start..context_end].to_string();

                    let rule = RuleMatchRule::new(&self.id, self.description())
                        .with_category(self.category.clone())
                        .with_issue_type(IssueType::Style.as_str());

                    matches.push(
                        RuleMatch::new(
                            format!("Sentence is too long ({} words, max: {})", word_count, self.max_words),
                            start,
                            end - start,
                            rule,
                            RuleMatchContext::new(context_text, context_start, context_end - context_start),
                        )
                        .with_sentence(sentence.text().to_string())
                    );
                }
            }

            pos += sentence.text().len();
        }

        matches
    }
}

/// Detects when consecutive sentences start with the same word.
pub struct WordRepeatBeginningRule {
    id: String,
    category: Category,
    min_sentences: usize,
}

impl WordRepeatBeginningRule {
    pub fn new() -> Self {
        Self {
            id: "WORD_REPEAT_BEGINNING_RULE".to_string(),
            category: Category::new("STYLE", "Style"),
            min_sentences: 3,
        }
    }
}

impl Default for WordRepeatBeginningRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for WordRepeatBeginningRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Successive sentences start with the same word"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut first_words: Vec<(String, usize, usize, usize)> = Vec::new(); // (word, pos, start, end)

        let mut pos = 0usize;
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            if let Some(first) = tokens.first() {
                let word = first.token().token().to_lowercase();
                let start = pos + first.token().start();
                let end = pos + first.token().end();
                first_words.push((word, pos, start, end));
            }
            pos += sentence.text().len();
        }

        // Check for runs of same starting word
        let mut i = 0;
        while i < first_words.len() {
            let mut run_len = 1;
            while i + run_len < first_words.len() && first_words[i + run_len].0 == first_words[i].0 {
                run_len += 1;
            }

            if run_len >= self.min_sentences {
                for j in i..i + run_len {
                    let (_, _, start, end) = &first_words[j];
                    let context_start = if *start >= 30 { start - 30 } else { 0 };
                    let context_end = std::cmp::min(end + 30, text.len());
                    let context_text = text[context_start..context_end].to_string();

                    let rule = RuleMatchRule::new(&self.id, self.description())
                        .with_category(self.category.clone())
                        .with_issue_type(IssueType::Style.as_str());

                    matches.push(
                        RuleMatch::new(
                            format!("Three or more sentences start with the word '{}'.", first_words[i].0),
                            *start,
                            *end - start,
                            rule,
                            RuleMatchContext::new(context_text, context_start, context_end - context_start),
                        )
                        .with_sentence("".to_string())
                    );
                }
            }

            i += run_len;
        }

        matches
    }
}

// Helper functions

#[allow(dead_code)]
fn is_normal_whitespace(token: &AnalyzedTokenReadings) -> bool {
    token.is_whitespace() || token.token().token() == "\u{A0}"
}

#[allow(dead_code)]
fn is_removable_whitespace(token: &AnalyzedTokenReadings) -> bool {
    (token.is_whitespace() || token.token().token() == "\u{A0}")
        && !is_linebreak(token)
        && token.token().token() != "\t"
        && !contains_invisible(token)
}

#[allow(dead_code)]
fn is_linebreak(token: &AnalyzedTokenReadings) -> bool {
    let t = token.token().token();
    t.contains('\n') || t.contains('\r')
}

#[allow(dead_code)]
fn contains_invisible(token: &AnalyzedTokenReadings) -> bool {
    let t = token.token().token();
    t.contains('\u{200B}') || t.contains('\u{FEFF}') || t.contains('\u{2060}')
}

#[allow(dead_code)]
fn is_smiley_exception(
    token_text: &str,
    all_tokens: &[AnalyzedTokenReadings],
    current: &AnalyzedTokenReadings,
) -> bool {
    // Find current token index in all_tokens
    let current_idx = match all_tokens.iter().position(|t| std::ptr::eq(t, current)) {
        Some(idx) => idx,
        None => return false,
    };

    // Check for :) :(
    if (token_text == ")" || token_text == "(") && current_idx >= 1 {
        let prev = all_tokens[current_idx - 1].token().token();
        if prev == ":" || prev == ";" {
            return true;
        }
    }

    // Check for :-) ;-(
    if (token_text == ")" || token_text == "(") && current_idx >= 2 {
        let prev = all_tokens[current_idx - 1].token().token();
        let prev_prev = all_tokens[current_idx - 2].token().token();
        if prev == "-" && (prev_prev == ":" || prev_prev == ";") {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::rule::TextLevelRule;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings};

    fn make_sentence_with_whitespace(text: &str, offset: usize) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, offset, offset + text.len());
        let mut tokens = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i].is_whitespace() {
                let start = i;
                let mut end = i + 1;
                while end < chars.len() && chars[end].is_whitespace() {
                    end += 1;
                }
                let ws_text: String = chars[start..end].iter().collect();
                let at = AnalyzedToken::new(&ws_text, offset + start, offset + end);
                tokens.push(AnalyzedTokenReadings::new(at));
                i = end;
            } else {
                let start = i;
                let mut end = i + 1;
                while end < chars.len() && !chars[end].is_whitespace() {
                    end += 1;
                }
                let word: String = chars[start..end].iter().collect();
                let at = AnalyzedToken::new(&word, offset + start, offset + end);
                tokens.push(AnalyzedTokenReadings::new(at));
                i = end;
            }
        }

        sentence.set_tokens(tokens);
        sentence
    }

    fn make_simple_sentence(text: &str, offset: usize) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, offset, offset + text.len());
        let mut tokens = Vec::new();
        let mut pos = 0;
        for word in text.split_whitespace() {
            let start = text[pos..].find(word).unwrap_or(0) + pos;
            let end = start + word.len();
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(word, offset + start, offset + end)));
            pos = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    // MultipleWhitespaceRule tests
    #[test]
    fn test_multiple_whitespace_double_space() {
        let rule = MultipleWhitespaceRule::new();
        let s1 = make_sentence_with_whitespace("Hello  world", 0);
        let text = "Hello  world";
        let matches = rule.match_text(text, &[s1]);
        assert_eq!(matches.len(), 1, "Expected 1 match for double space");
        assert!(matches[0].message().contains("duplicated whitespace"));
    }

    #[test]
    fn test_multiple_whitespace_triple_space() {
        let rule = MultipleWhitespaceRule::new();
        let s1 = make_sentence_with_whitespace("Hello   world", 0);
        let text = "Hello   world";
        let matches = rule.match_text(text, &[s1]);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_multiple_whitespace_single_space_ok() {
        let rule = MultipleWhitespaceRule::new();
        let s1 = make_sentence_with_whitespace("Hello world", 0);
        let text = "Hello world";
        let matches = rule.match_text(text, &[s1]);
        assert!(matches.is_empty(), "Single space should not trigger");
    }

    #[test]
    fn test_multiple_whitespace_leading_space_ok() {
        let rule = MultipleWhitespaceRule::new();
        let s1 = make_sentence_with_whitespace("Hello world", 0);
        let text = "Hello world";
        let matches = rule.match_text(text, &[s1]);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_multiple_whitespace_across_sentences() {
        let rule = MultipleWhitespaceRule::new();
        let s1 = make_sentence_with_whitespace("Hello.  ", 0);
        let s2 = make_sentence_with_whitespace("World.", 7);
        let text = "Hello.  World.";
        let matches = rule.match_text(text, &[s1, s2]);
        assert_eq!(matches.len(), 1, "Double space between sentences");
    }

    // SentenceWhitespaceRule tests
    #[test]
    fn test_sentence_whitespace_missing_space() {
        let rule = SentenceWhitespaceRule::new();
        let s1 = AnalyzedSentence::new("Hello.", 0, 6);
        let mut s2 = AnalyzedSentence::new("World.", 6, 12);
        s1_clone_tokens_for_test(&mut s2, "World.", 6);

        let text = "Hello.World.";
        let matches = rule.match_text(text, &[s1, s2]);
        assert_eq!(matches.len(), 1, "Missing space between sentences");
    }

    #[test]
    fn test_sentence_whitespace_has_space_ok() {
        let rule = SentenceWhitespaceRule::new();
        let mut s1 = AnalyzedSentence::new("Hello. ", 0, 7);
        let mut s2 = AnalyzedSentence::new("World.", 7, 13);

        let tokens1 = vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("Hello", 0, 5)),
            AnalyzedTokenReadings::new(AnalyzedToken::new(".", 5, 6)),
            AnalyzedTokenReadings::new(AnalyzedToken::new(" ", 6, 7)),
        ];
        s1.set_tokens(tokens1);

        s1_clone_tokens_for_test(&mut s2, "World.", 7);

        let text = "Hello. World.";
        let matches = rule.match_text(text, &[s1, s2]);
        assert!(matches.is_empty(), "Space present, no error");
    }

    fn s1_clone_tokens_for_test(sentence: &mut AnalyzedSentence, text: &str, offset: usize) {
        let mut tokens = Vec::new();
        let mut pos = 0;
        for word in text.split_whitespace() {
            let start = text[pos..].find(word).unwrap() + pos;
            let end = start + word.len();
            let at = AnalyzedToken::new(word, offset + start, offset + end);
            tokens.push(AnalyzedTokenReadings::new(at));
            pos = end;
        }
        sentence.set_tokens(tokens);
    }

    // GenericUnpairedBracketsRule tests
    #[test]
    fn test_unpaired_opening_bracket() {
        let rule = GenericUnpairedBracketsRule::new();
        let s = make_simple_sentence("Hello world", 0);
        let text = "Hello ( world";
        let matches = rule.match_text(text, &[s]);
        assert_eq!(matches.len(), 1, "Unpaired opening bracket");
        assert!(matches[0].message().contains("Unpaired"));
    }

    #[test]
    fn test_unpaired_closing_bracket() {
        let rule = GenericUnpairedBracketsRule::new();
        let s = make_simple_sentence("Hello world", 0);
        let text = "Hello ) world";
        let matches = rule.match_text(text, &[s]);
        assert_eq!(matches.len(), 1, "Unpaired closing bracket");
    }

    #[test]
    fn test_paired_brackets_ok() {
        let rule = GenericUnpairedBracketsRule::new();
        let s = make_simple_sentence("Hello world", 0);
        let text = "Hello (world)";
        let matches = rule.match_text(text, &[s]);
        assert!(matches.is_empty(), "Paired brackets should not trigger");
    }

    #[test]
    fn test_nested_brackets_ok() {
        let rule = GenericUnpairedBracketsRule::new();
        let s = make_simple_sentence("Hello world", 0);
        let text = "Hello ((world))";
        let matches = rule.match_text(text, &[s]);
        assert!(matches.is_empty(), "Nested paired brackets should not trigger");
    }

    #[test]
    fn test_smiley_exception() {
        let rule = GenericUnpairedBracketsRule::new();
        let s = make_simple_sentence("Hello world", 0);
        let text = "Hello :) world";
        let matches = rule.match_text(text, &[s]);
        assert!(matches.is_empty(), "Smiley :) should not trigger unpaired bracket");
    }

    #[test]
    fn test_multiple_bracket_types() {
        let rule = GenericUnpairedBracketsRule::new();
        let s = make_simple_sentence("Hello", 0);
        let text = "[{(Hello)}]";
        let matches = rule.match_text(text, &[s]);
        assert!(matches.is_empty(), "All paired should not trigger");
    }

    // LongSentenceRule tests
    #[test]
    fn test_long_sentence_triggers() {
        let rule = LongSentenceRule::new(5);
        let mut sentence = AnalyzedSentence::new("one two three four five six", 0, 26);
        let mut tokens = Vec::new();
        let mut pos = 0;
        for word in "one two three four five six".split_whitespace() {
            let start = pos;
            let end = start + word.len();
            let at = AnalyzedToken::new(word, start, end);
            tokens.push(AnalyzedTokenReadings::new(at));
            pos = end + 1;
        }
        sentence.set_tokens(tokens);

        let text = "one two three four five six";
        let matches = rule.match_text(text, &[sentence]);
        assert_eq!(matches.len(), 1, "Sentence with 6 words, max 5, should trigger");
    }

    #[test]
    fn test_short_sentence_ok() {
        let rule = LongSentenceRule::new(10);
        let mut sentence = AnalyzedSentence::new("Short sentence.", 0, 15);
        let mut tokens = Vec::new();
        for word in "Short sentence.".split_whitespace() {
            let _start = 0usize;
            let at = AnalyzedToken::new(word, 0, word.len());
            tokens.push(AnalyzedTokenReadings::new(at));
        }
        sentence.set_tokens(tokens);

        let text = "Short sentence.";
        let matches = rule.match_text(text, &[sentence]);
        assert!(matches.is_empty(), "Short sentence should not trigger");
    }

    // WordRepeatBeginningRule tests
    #[test]
    fn test_repeat_beginning_triggers() {
        let rule = WordRepeatBeginningRule::new();

        let mut s1 = AnalyzedSentence::new("The cat sat.", 0, 12);
        let mut s2 = AnalyzedSentence::new("The dog ran.", 13, 25);
        let mut s3 = AnalyzedSentence::new("The bird flew.", 26, 40);

        s1_clone_tokens_for_test(&mut s1, "The cat sat.", 0);
        s1_clone_tokens_for_test(&mut s2, "The dog ran.", 13);
        s1_clone_tokens_for_test(&mut s3, "The bird flew.", 26);

        let text = "The cat sat. The dog ran. The bird flew.";
        let matches = rule.match_text(text, &[s1, s2, s3]);
        assert_eq!(matches.len(), 3, "Three sentences starting with 'The' should trigger 3 matches");
    }

    #[test]
    fn test_no_repeat_beginning_ok() {
        let rule = WordRepeatBeginningRule::new();

        let mut s1 = AnalyzedSentence::new("The cat sat.", 0, 12);
        let mut s2 = AnalyzedSentence::new("A dog ran.", 13, 23);
        let mut s3 = AnalyzedSentence::new("Some bird flew.", 24, 39);

        s1_clone_tokens_for_test(&mut s1, "The cat sat.", 0);
        s1_clone_tokens_for_test(&mut s2, "A dog ran.", 13);
        s1_clone_tokens_for_test(&mut s3, "Some bird flew.", 24);

        let text = "The cat sat. A dog ran. Some bird flew.";
        let matches = rule.match_text(text, &[s1, s2, s3]);
        assert!(matches.is_empty(), "Different starting words should not trigger");
    }

    #[test]
    fn test_two_repeats_not_enough() {
        let rule = WordRepeatBeginningRule::new();

        let mut s1 = AnalyzedSentence::new("The cat sat.", 0, 12);
        let mut s2 = AnalyzedSentence::new("The dog ran.", 13, 25);

        s1_clone_tokens_for_test(&mut s1, "The cat sat.", 0);
        s1_clone_tokens_for_test(&mut s2, "The dog ran.", 13);

        let text = "The cat sat. The dog ran.";
        let matches = rule.match_text(text, &[s1, s2]);
        assert!(matches.is_empty(), "Only 2 repeats, need 3");
    }
}
