use og_core::Token;

pub struct RussianWordTokenizer {
    inner: og_tokenizer::DefaultWordTokenizer,
}

impl RussianWordTokenizer {
    pub fn new() -> Self {
        Self {
            inner: og_tokenizer::DefaultWordTokenizer::new_generic(),
        }
    }
}

impl Default for RussianWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

// Java RussianWordTokenizer's sentinel chain only observably keeps contiguous
// б/у and б/н unsplit; its SP_DOT sentinels round-trip to the unguarded stream.
impl og_tokenizer::WordTokenizer for RussianWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        let raw = self.inner.tokenize(text);
        let mut out: Vec<Token> = Vec::with_capacity(raw.len());
        let mut i = 0;
        while i + 2 < raw.len() {
            let contiguous =
                raw[i].end() == raw[i + 1].start() && raw[i + 1].end() == raw[i + 2].start();
            let is_abbreviation = raw[i].text() == "б"
                && raw[i + 1].text() == "/"
                && (raw[i + 2].text() == "у" || raw[i + 2].text() == "н");
            if contiguous && is_abbreviation {
                let start = raw[i].start();
                let end = raw[i + 2].end();
                out.push(Token::new(&text[start..end], start, end));
                i += 3;
            } else {
                out.push(raw[i].clone());
                i += 1;
            }
        }
        out.extend(raw[i..].iter().cloned());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_tokenizer::WordTokenizer as _;

    fn texts(text: &str) -> Vec<String> {
        RussianWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn keeps_bu_abbreviation_as_one_token() {
        let tokens = texts("Продам б/у компьютер.");
        assert!(tokens.contains(&"б/у".to_string()), "{tokens:?}");
        assert!(!tokens.contains(&"/".to_string()));
    }

    #[test]
    fn keeps_bn_abbreviation_as_one_token() {
        let tokens = texts("Оплачу счёт б/н переводом.");
        assert!(tokens.contains(&"б/н".to_string()), "{tokens:?}");
    }

    #[test]
    fn does_not_merge_spaced_or_other_slash_triples() {
        let tokens = texts("б / у 以及 x/y");
        assert!(!tokens.contains(&"б/у".to_string()), "{tokens:?}");
        assert!(!tokens.contains(&"x/y".to_string()), "{tokens:?}");
    }

    #[test]
    fn merged_token_offsets_index_the_original_text() {
        let text = "Продам б/у компьютер.";
        let tokens = RussianWordTokenizer::new().tokenize(text);
        let expected = text.find("компьютер").expect("word present");
        let computer = tokens
            .iter()
            .find(|t| t.text() == "компьютер")
            .expect("token present");
        assert_eq!(computer.start(), expected);
        assert_eq!(&text[computer.start()..computer.end()], "компьютер");
        let bu = tokens.iter().find(|t| t.text() == "б/у").expect("guarded");
        assert_eq!(&text[bu.start()..bu.end()], "б/у");
    }

    #[test]
    fn plain_text_tokens_match_the_generic_tokenizer() {
        let text = "Москва — большой город.";
        let guarded = RussianWordTokenizer::new().tokenize(text);
        let plain = og_tokenizer::DefaultWordTokenizer::new_generic().tokenize(text);
        let strip = |tokens: Vec<Token>| {
            tokens
                .into_iter()
                .map(|t| (t.text().to_string(), t.start(), t.end()))
                .collect::<Vec<_>>()
        };
        assert_eq!(strip(guarded), strip(plain));
    }
}
