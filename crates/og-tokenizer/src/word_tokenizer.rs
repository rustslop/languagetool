use og_core::Token;
use crate::WordTokenizer;

pub struct DefaultWordTokenizer;

impl DefaultWordTokenizer {
    pub fn new() -> Self {
        Self
    }

    fn is_word_char(&self, c: char) -> bool {
        c.is_alphanumeric() || c == '_' || c == '\'' || c == '\u{2019}'
    }

    /// Split English contractions into separate tokens, matching Java LT behavior.
    /// E.g. "don't" → "Do" + "n't", "can't" → "ca" + "n't", "it's" → "it" + "'s"
    fn split_contractions(&self, tokens: Vec<Token>) -> Vec<Token> {
        let mut result = Vec::with_capacity(tokens.len());

        for token in tokens {
            let text = token.text();
            let start = token.start();
            let _end = token.end();

            // Only try splitting word tokens that contain an apostrophe
            if !text.contains('\'') && !text.contains('\u{2019}') {
                result.push(token);
                continue;
            }

            if let Some(split) = self.try_split_contraction(text, start) {
                result.extend(split);
            } else {
                result.push(token);
            }
        }

        result
    }

    fn try_split_contraction(&self, text: &str, start: usize) -> Option<Vec<Token>> {
        let lower = text.to_lowercase();
        let normalized = lower.replace('\u{2019}', "'");
        let chars: Vec<char> = text.chars().collect();
        let text_len = text.len();

        // Pattern 1: n't contractions (Java regex group 1 + group 2)
        // Matches: (stem)(n't) where stem is are/is/was/do/does/did/have/etc.
        let nt_stems = [
            "are", "is", "were", "was", "do", "does", "did",
            "have", "has", "had", "wo", "would", "ca", "could",
            "sha", "should", "must", "ai", "ought", "might",
            "need", "may", "am", "dare", "used", "use",
        ];
        for stem in &nt_stems {
            if let Some(rest) = normalized.strip_prefix(stem) {
                if rest == "n't" {
                    // The split is BEFORE the 'n', so prefix = stem, suffix = n't
                    // stem is ASCII, so its byte length = char count
                    let byte_offset = stem.len();
                    if byte_offset > 0 && byte_offset < text_len {
                        return Some(vec![
                            Token::new(&text[..byte_offset], start, start + byte_offset),
                            Token::new(&text[byte_offset..], start + byte_offset, start + text_len),
                        ]);
                    }
                }
            }
        }

        // Pattern 2: 't prefix for 'twas, 'twere
        let t_prefixes = ["'twas", "'twere", "'tis", "'twas"];
        for tp in &t_prefixes {
            if normalized == *tp {
                // Split at 't + rest
                let byte_offset: usize = chars[..2].iter().map(|c| c.len_utf8()).sum(); // 't is 2 chars
                if byte_offset > 0 && byte_offset < text_len {
                    return Some(vec![
                        Token::new(&text[..byte_offset], start, start + byte_offset),
                        Token::new(&text[byte_offset..], start + byte_offset, start + text_len),
                    ]);
                }
            }
        }

        // Pattern 3: suffix contractions: 's, 'm, 're, 'll, 've, 'd
        // Find the apostrophe in the original text
        let apos_char_idx = chars.iter().position(|c| *c == '\'' || *c == '\u{2019}')?;
        let byte_offset: usize = chars[..apos_char_idx].iter().map(|c| c.len_utf8()).sum();

        if byte_offset == 0 || byte_offset >= text_len {
            return None;
        }

        let suffix_text = &text[byte_offset..];
        let suffix_normalized: String = suffix_text.to_lowercase().replace('\u{2019}', "'");
        let valid_suffixes = ["'s", "'m", "'re", "'ll", "'ve", "'d"];
        if valid_suffixes.contains(&suffix_normalized.as_str()) {
            return Some(vec![
                Token::new(&text[..byte_offset], start, start + byte_offset),
                Token::new(suffix_text, start + byte_offset, start + text_len),
            ]);
        }

        None
    }
}



impl Default for DefaultWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl WordTokenizer for DefaultWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut tokens = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut byte_offset = 0;

        while i < chars.len() {
            let c = chars[i];

            if c.is_whitespace() {
                // Consume whitespace as a single token
                let start_byte = byte_offset;
                let mut end_byte = byte_offset;
                while i < chars.len() && chars[i].is_whitespace() {
                    end_byte += chars[i].len_utf8();
                    i += 1;
                }
                tokens.push(Token::new(&text[start_byte..end_byte], start_byte, end_byte));
                byte_offset = end_byte;
            } else if self.is_word_char(c) {
                // Consume word characters
                let start_byte = byte_offset;
                let mut end_byte = byte_offset;
                while i < chars.len() && self.is_word_char(chars[i]) {
                    end_byte += chars[i].len_utf8();
                    i += 1;
                }
                tokens.push(Token::new(&text[start_byte..end_byte], start_byte, end_byte));
                byte_offset = end_byte;
            } else {
                // Punctuation or other single character
                let start_byte = byte_offset;
                let end_byte = byte_offset + c.len_utf8();
                tokens.push(Token::new(&text[start_byte..end_byte], start_byte, end_byte));
                byte_offset = end_byte;
                i += 1;
            }
        }

        self.split_contractions(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WordTokenizer;

    #[test]
    fn test_basic_tokenization() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("Hello world");
        assert_eq!(tokens.len(), 3); // "Hello", " ", "world"
        assert_eq!(tokens[0].text(), "Hello");
        assert_eq!(tokens[1].text(), " ");
        assert_eq!(tokens[2].text(), "world");
    }

    #[test]
    fn test_punctuation() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("Hello, world!");
        assert_eq!(tokens[0].text(), "Hello");
        assert_eq!(tokens[1].text(), ",");
        assert_eq!(tokens[2].text(), " ");
        assert_eq!(tokens[3].text(), "world");
        assert_eq!(tokens[4].text(), "!");
    }

    #[test]
    fn test_offsets() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("ab cd");
        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 2);
        assert_eq!(tokens[1].start(), 2);
        assert_eq!(tokens[1].end(), 3);
        assert_eq!(tokens[2].start(), 3);
        assert_eq!(tokens[2].end(), 5);
    }

    #[test]
    fn test_apostrophe() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("don't");
        // Java LT splits "don't" → "do" + "n't"
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "do");
        assert_eq!(tokens[1].text(), "n't");
    }

    #[test]
    fn test_empty() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_multiple_spaces() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("Hello  world");
        assert_eq!(tokens[1].text(), "  ");
    }

    #[test]
    fn test_unicode() {
        let tokenizer = DefaultWordTokenizer::new();
        let tokens = tokenizer.tokenize("café");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "café");
    }

    // ========================================================================
    // Exhaustive test suite
    // ========================================================================

    /// Helper: collect just the token texts from a tokenize call.
    fn token_texts(tokenizer: &DefaultWordTokenizer, input: &str) -> Vec<String> {
        tokenizer.tokenize(input).iter().map(|t| t.text().to_string()).collect()
    }

    /// Helper: verify that start/end offsets are monotonically increasing,
    /// contiguous (no gaps, no overlaps), and that each token's text length
    /// matches its byte span.
    fn assert_offsets_valid(tokens: &[Token]) {
        for (i, tok) in tokens.iter().enumerate() {
            // The text length in bytes must equal end - start.
            assert_eq!(
                tok.text().len(),
                tok.end() - tok.start(),
                "Token #{} ({:?}): text byte length != end - start",
                i,
                tok.text()
            );
            if i > 0 {
                let prev = &tokens[i - 1];
                assert_eq!(
                    tok.start(),
                    prev.end(),
                    "Gap or overlap between token #{} ({:?}) and token #{} ({:?})",
                    i - 1,
                    prev.text(),
                    i,
                    tok.text()
                );
            }
        }
    }

    /// Helper: verify that the concatenation of all token texts equals the
    /// original input string.
    fn assert_reconstruction(tokens: &[Token], original: &str) {
        let reconstructed: String = tokens.iter().map(|t| t.text()).collect();
        assert_eq!(reconstructed, original);
    }

    // -- 1. Basic tokenization with offset checks ----------------------------

    #[test]
    fn test_basic_two_words() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello world");
        assert_eq!(token_texts(&tz, "Hello world"), vec!["Hello", " ", "world"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello world");

        // Verify individual offsets
        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 5);
        assert_eq!(tokens[1].start(), 5);
        assert_eq!(tokens[1].end(), 6);
        assert_eq!(tokens[2].start(), 6);
        assert_eq!(tokens[2].end(), 11);
    }

    // -- 2. Punctuation separation -------------------------------------------

    #[test]
    fn test_period_at_end() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello.");
        assert_eq!(token_texts(&tz, "Hello."), vec!["Hello", "."]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello.");

        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 5);
        assert_eq!(tokens[1].start(), 5);
        assert_eq!(tokens[1].end(), 6);
    }

    #[test]
    fn test_multiple_trailing_punctuation() {
        let tz = DefaultWordTokenizer::new();
        // Each punctuation character is a separate token (non-word, non-whitespace).
        let tokens = tz.tokenize("Wow!!!");
        assert_eq!(token_texts(&tz, "Wow!!!"), vec!["Wow", "!", "!", "!"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Wow!!!");
    }

    // -- 3. Comma handling ---------------------------------------------------

    #[test]
    fn test_comma_separation() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello, world");
        assert_eq!(token_texts(&tz, "Hello, world"), vec!["Hello", ",", " ", "world"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello, world");

        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[1].start(), 5);
        assert_eq!(tokens[2].start(), 6);
        assert_eq!(tokens[3].start(), 7);
    }

    #[test]
    fn test_comma_no_space() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a,b");
        assert_eq!(token_texts(&tz, "a,b"), vec!["a", ",", "b"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "a,b");
    }

    // -- 4. Apostrophe preservation ------------------------------------------

    #[test]
    fn test_apostrophe_in_contractions() {
        let tz = DefaultWordTokenizer::new();

        // don't → "do" + "n't" (Java LT behavior)
        let tokens = tz.tokenize("don't");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "do");
        assert_eq!(tokens[1].text(), "n't");
        assert_offsets_valid(&tokens);

        // it's → "it" + "'s"
        let tokens = tz.tokenize("it's");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "it");
        assert_eq!(tokens[1].text(), "'s");

        // I'm → "I" + "'m"
        let tokens = tz.tokenize("I'm");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "I");
        assert_eq!(tokens[1].text(), "'m");

        // can't → "ca" + "n't"
        let tokens = tz.tokenize("can't");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "ca");
        assert_eq!(tokens[1].text(), "n't");

        // won't → "wo" + "n't"
        let tokens = tz.tokenize("won't");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "wo");
        assert_eq!(tokens[1].text(), "n't");

        // they're → "they" + "'re"
        let tokens = tz.tokenize("they're");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "they");
        assert_eq!(tokens[1].text(), "'re");
    }

    #[test]
    fn test_apostrophe_with_sentence() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("I don't know");
        assert_eq!(token_texts(&tz, "I don't know"), vec!["I", " ", "do", "n't", " ", "know"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "I don't know");
    }

    #[test]
    fn test_unicode_right_single_quotation_mark() {
        // \u{2019} is the right single quotation mark (curly apostrophe)
        // Java LT also splits curly apostrophe contractions: "don't" → "do" + "n't"
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("don\u{2019}t");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "do");
        assert_eq!(tokens[1].text(), "n\u{2019}t");
        assert_offsets_valid(&tokens);
    }

    // -- 5. Multiple spaces --------------------------------------------------

    #[test]
    fn test_two_spaces() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello  world");
        assert_eq!(token_texts(&tz, "Hello  world"), vec!["Hello", "  ", "world"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello  world");
    }

    #[test]
    fn test_three_spaces() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a   b");
        assert_eq!(token_texts(&tz, "a   b"), vec!["a", "   ", "b"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_tabs() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a\tb");
        assert_eq!(token_texts(&tz, "a\tb"), vec!["a", "\t", "b"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_mixed_whitespace() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a \t b");
        // " \t " is three whitespace characters consumed as one token
        assert_eq!(tokens[1].text(), " \t ");
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "a \t b");
    }

    // -- 6. Numbers ----------------------------------------------------------

    #[test]
    fn test_number_at_start() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("42 is the answer");
        assert_eq!(token_texts(&tz, "42 is the answer"), vec!["42", " ", "is", " ", "the", " ", "answer"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "42 is the answer");
    }

    #[test]
    fn test_number_only() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("12345");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "12345");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_alphanumeric_mixed() {
        let tz = DefaultWordTokenizer::new();
        // Underscore is a word char, so this is a single token
        let tokens = tz.tokenize("hello_123");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "hello_123");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_decimal_number() {
        let tz = DefaultWordTokenizer::new();
        // Period is NOT a word char, so "3.14" splits into "3", ".", "14"
        let tokens = tz.tokenize("3.14");
        assert_eq!(token_texts(&tz, "3.14"), vec!["3", ".", "14"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_number_with_comma() {
        let tz = DefaultWordTokenizer::new();
        // Comma is not a word char: "1,000" -> "1", ",", "000"
        let tokens = tz.tokenize("1,000");
        assert_eq!(token_texts(&tz, "1,000"), vec!["1", ",", "000"]);
        assert_offsets_valid(&tokens);
    }

    // -- 7. Mixed punctuation ------------------------------------------------

    #[test]
    fn test_exclamation_and_question() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello! How are you?");
        assert_eq!(
            token_texts(&tz, "Hello! How are you?"),
            vec!["Hello", "!", " ", "How", " ", "are", " ", "you", "?"]
        );
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello! How are you?");
    }

    #[test]
    fn test_semicolon_and_colon() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("yes:no;maybe");
        assert_eq!(token_texts(&tz, "yes:no;maybe"), vec!["yes", ":", "no", ";", "maybe"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_ellipsis_standalone() {
        let tz = DefaultWordTokenizer::new();
        // Each dot is a separate token since '.' is not a word/whitespace char
        let tokens = tz.tokenize("...");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text(), ".");
        assert_eq!(tokens[1].text(), ".");
        assert_eq!(tokens[2].text(), ".");
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "...");
    }

    // -- 8. Hyphenated words -------------------------------------------------

    #[test]
    fn test_hyphenated_word() {
        let tz = DefaultWordTokenizer::new();
        // Hyphen '-' is NOT a word char, so "well-known" splits
        let tokens = tz.tokenize("well-known");
        assert_eq!(token_texts(&tz, "well-known"), vec!["well", "-", "known"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "well-known");
    }

    #[test]
    fn test_double_hyphenated() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("state-of-the-art");
        assert_eq!(
            token_texts(&tz, "state-of-the-art"),
            vec!["state", "-", "of", "-", "the", "-", "art"]
        );
        assert_offsets_valid(&tokens);
    }

    // -- 9. Parentheses ------------------------------------------------------

    #[test]
    fn test_parentheses_around_word() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello (world)");
        assert_eq!(
            token_texts(&tz, "Hello (world)"),
            vec!["Hello", " ", "(", "world", ")"]
        );
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello (world)");
    }

    #[test]
    fn test_parentheses_no_space() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("(hello)");
        assert_eq!(token_texts(&tz, "(hello)"), vec!["(", "hello", ")"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_nested_parentheses() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("((x))");
        assert_eq!(token_texts(&tz, "((x))"), vec!["(", "(", "x", ")", ")"]);
        assert_offsets_valid(&tokens);
    }

    // -- 10. Empty string ----------------------------------------------------

    #[test]
    fn test_empty_string_returns_no_tokens() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("");
        assert!(tokens.is_empty());
    }

    // -- 11. Single word -----------------------------------------------------

    #[test]
    fn test_single_word() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "Hello");
        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 5);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_single_character() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "a");
        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 1);
    }

    // -- 12. Multiple punctuation (standalone) --------------------------------

    #[test]
    fn test_only_punctuation() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("!?.,;");
        assert_eq!(token_texts(&tz, "!?.,;"), vec!["!", "?", ".", ",", ";"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "!?.,;");
    }

    // -- 13. Special characters -----------------------------------------------

    #[test]
    fn test_at_symbol() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("user@host");
        assert_eq!(token_texts(&tz, "user@host"), vec!["user", "@", "host"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_hash_symbol() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("#tag");
        assert_eq!(token_texts(&tz, "#tag"), vec!["#", "tag"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_dollar_sign() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("$100");
        assert_eq!(token_texts(&tz, "$100"), vec!["$", "100"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_percent_sign() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("100%");
        assert_eq!(token_texts(&tz, "100%"), vec!["100", "%"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_ampersand() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("A&B");
        assert_eq!(token_texts(&tz, "A&B"), vec!["A", "&", "B"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_slash() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("and/or");
        assert_eq!(token_texts(&tz, "and/or"), vec!["and", "/", "or"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_backslash() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a\\b");
        assert_eq!(token_texts(&tz, "a\\b"), vec!["a", "\\", "b"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_caret() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("x^2");
        assert_eq!(token_texts(&tz, "x^2"), vec!["x", "^", "2"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_tilde() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("~test");
        assert_eq!(token_texts(&tz, "~test"), vec!["~", "test"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_pipe() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a|b");
        assert_eq!(token_texts(&tz, "a|b"), vec!["a", "|", "b"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_angle_brackets() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("<html>");
        assert_eq!(token_texts(&tz, "<html>"), vec!["<", "html", ">"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_square_brackets() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("[item]");
        assert_eq!(token_texts(&tz, "[item]"), vec!["[", "item", "]"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_curly_braces() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("{key}");
        assert_eq!(token_texts(&tz, "{key}"), vec!["{", "key", "}"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_equals_sign() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("x=1");
        assert_eq!(token_texts(&tz, "x=1"), vec!["x", "=", "1"]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_plus_and_asterisk() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("1+2*3");
        assert_eq!(token_texts(&tz, "1+2*3"), vec!["1", "+", "2", "*", "3"]);
        assert_offsets_valid(&tokens);
    }

    // -- 14. Leading and trailing whitespace ----------------------------------

    #[test]
    fn test_leading_space() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize(" Hello");
        assert_eq!(token_texts(&tz, " Hello"), vec![" ", "Hello"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, " Hello");
    }

    #[test]
    fn test_trailing_space() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello ");
        assert_eq!(token_texts(&tz, "Hello "), vec!["Hello", " "]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "Hello ");
    }

    #[test]
    fn test_leading_and_trailing_spaces() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("  Hello  ");
        assert_eq!(token_texts(&tz, "  Hello  "), vec!["  ", "Hello", "  "]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "  Hello  ");
    }

    #[test]
    fn test_only_whitespace() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("   ");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "   ");
        assert!(tokens[0].is_whitespace());
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_only_tab() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("\t");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "\t");
        assert!(tokens[0].is_whitespace());
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_only_newline() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("\n");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "\n");
        assert!(tokens[0].is_whitespace());
    }

    #[test]
    fn test_newline_between_words() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("line1\nline2");
        assert_eq!(token_texts(&tz, "line1\nline2"), vec!["line1", "\n", "line2"]);
        assert_offsets_valid(&tokens);
    }

    // -- Token classification via Token methods --------------------------------

    #[test]
    fn test_token_is_word_classification() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hello, world!");
        assert!(tokens[0].is_word());    // "Hello"
        assert!(!tokens[1].is_word());   // ","
        assert!(!tokens[2].is_word());   // " "
        assert!(tokens[3].is_word());    // "world"
        assert!(!tokens[4].is_word());   // "!"
    }

    #[test]
    fn test_token_is_whitespace_classification() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a b");
        assert!(!tokens[0].is_whitespace()); // "a"
        assert!(tokens[1].is_whitespace());  // " "
        assert!(!tokens[2].is_whitespace()); // "b"
    }

    #[test]
    fn test_token_is_punctuation_classification() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("a, b!");
        // tokens: ["a", ",", " ", "b", "!"]
        assert!(!tokens[0].is_punctuation()); // "a"
        assert!(tokens[1].is_punctuation());  // ","
        assert!(!tokens[2].is_punctuation()); // " "
        assert!(!tokens[3].is_punctuation()); // "b"
        assert!(tokens[4].is_punctuation());  // "!"
    }

    #[test]
    fn test_token_len() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("Hi  world");
        assert_eq!(tokens[0].len(), 2); // "Hi"
        assert_eq!(tokens[1].len(), 2); // "  "
        assert_eq!(tokens[2].len(), 5); // "world"
    }

    // -- Unicode edge cases ---------------------------------------------------

    #[test]
    fn test_unicode_multi_byte_characters() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("café résumé");
        assert_eq!(tokens.len(), 3); // "café", " ", "résumé"
        assert_eq!(tokens[0].text(), "café");
        assert_eq!(tokens[1].text(), " ");
        assert_eq!(tokens[2].text(), "résumé");
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, "café résumé");

        // Verify byte offsets account for multi-byte 'é' (2 bytes each in UTF-8)
        // "café" = 5 bytes (c=1, a=1, f=1, é=2)
        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 5);
        // " " = 1 byte
        assert_eq!(tokens[1].start(), 5);
        assert_eq!(tokens[1].end(), 6);
        // "résumé" = 8 bytes (r=1, é=2, s=1, u=1, m=1, é=2)
        assert_eq!(tokens[2].start(), 6);
        assert_eq!(tokens[2].end(), 14);
    }

    #[test]
    fn test_cjk_characters() {
        let tz = DefaultWordTokenizer::new();
        // CJK characters are alphabetic, so they form word tokens
        let tokens = tz.tokenize("你好");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "你好");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_emoji_separate_from_word() {
        let tz = DefaultWordTokenizer::new();
        // Emojis are not alphanumeric/underscore/apostrophe, so they are
        // separate tokens
        let tokens = tz.tokenize("Hi👍");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text(), "Hi");
        assert_eq!(tokens[1].text(), "👍");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_underscore_in_word() {
        let tz = DefaultWordTokenizer::new();
        // Underscore is a word char
        let tokens = tz.tokenize("snake_case");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "snake_case");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_underscore_leading() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("_private");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "_private");
    }

    #[test]
    fn test_underscore_only() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("_");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "_");
    }

    // -- Long and complex inputs ----------------------------------------------

    #[test]
    fn test_long_sentence() {
        let tz = DefaultWordTokenizer::new();
        let input = "The quick brown fox jumps over the lazy dog.";
        let tokens = tz.tokenize(input);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, input);
        // Last token should be the period
        assert_eq!(tokens.last().unwrap().text(), ".");
        // First token should be "The"
        assert_eq!(tokens.first().unwrap().text(), "The");
    }

    #[test]
    fn test_complex_mixed_input() {
        let tz = DefaultWordTokenizer::new();
        let input = "Hello, (world)! Don't you agree? Yes—no; maybe...";
        let tokens = tz.tokenize(input);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, input);
    }

    #[test]
    fn test_repeated_punctuation_word_pattern() {
        let tz = DefaultWordTokenizer::new();
        let input = "a.b.c.d";
        let tokens = tz.tokenize(input);
        assert_eq!(token_texts(&tz, input), vec!["a", ".", "b", ".", "c", ".", "d"]);
        assert_offsets_valid(&tokens);
        assert_reconstruction(&tokens, input);
    }

    // -- Single punctuation character -----------------------------------------

    #[test]
    fn test_single_period() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize(".");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), ".");
        assert_eq!(tokens[0].start(), 0);
        assert_eq!(tokens[0].end(), 1);
    }

    #[test]
    fn test_single_comma() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize(",");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), ",");
    }

    #[test]
    fn test_single_exclamation() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("!");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "!");
    }

    // -- Double quotes and single quotes (non-apostrophe context) -------------

    #[test]
    fn test_double_quotes() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("\"hello\"");
        assert_eq!(token_texts(&tz, "\"hello\""), vec!["\"", "hello", "\""]);
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_single_quotes_around_word() {
        let tz = DefaultWordTokenizer::new();
        // ' at start/end of a word: 'hello'
        // The tokenizer treats ' as a word char, so "'hello'" becomes one token
        let tokens = tz.tokenize("'hello'");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "'hello'");
    }

    // -- Dash characters ------------------------------------------------------

    #[test]
    fn test_en_dash() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("10–20");
        // en dash (U+2013) is not a word char, so it separates tokens
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text(), "10");
        assert_eq!(tokens[1].text(), "\u{2013}");
        assert_eq!(tokens[2].text(), "20");
        assert_offsets_valid(&tokens);
    }

    #[test]
    fn test_em_dash() {
        let tz = DefaultWordTokenizer::new();
        let tokens = tz.tokenize("word—another");
        // em dash (U+2014) is not a word char
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text(), "word");
        assert_eq!(tokens[1].text(), "\u{2014}");
        assert_eq!(tokens[2].text(), "another");
        assert_offsets_valid(&tokens);
    }

    // -- Default trait --------------------------------------------------------

    #[test]
    fn test_default_trait() {
        let tz = DefaultWordTokenizer::default();
        let tokens = tz.tokenize("test");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text(), "test");
    }
}
