use og_core::Token;

pub struct CrimeanTatarWordTokenizer {
    inner: og_tokenizer::DefaultWordTokenizer,
}

impl CrimeanTatarWordTokenizer {
    pub fn new() -> Self {
        Self {
            inner: og_tokenizer::DefaultWordTokenizer::new_generic(),
        }
    }
}

impl Default for CrimeanTatarWordTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl og_tokenizer::WordTokenizer for CrimeanTatarWordTokenizer {
    fn tokenize(&self, text: &str) -> Vec<Token> {
        self.inner.split_trailing_hyphens(self.inner.tokenize(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_tokenizer::WordTokenizer as _;

    fn texts(text: &str) -> Vec<String> {
        CrimeanTatarWordTokenizer::new()
            .tokenize(text)
            .into_iter()
            .map(|t| t.text().to_string())
            .collect()
    }

    #[test]
    fn java_pin_hyphen_stays_inside_words() {
        assert_eq!(
            texts("Qırımtatar Milliy Meclisiniñ 120-cı toplaşuvı olıp keçti"),
            vec![
                "Qırımtatar".to_string(),
                " ".to_string(),
                "Milliy".to_string(),
                " ".to_string(),
                "Meclisiniñ".to_string(),
                " ".to_string(),
                "120-cı".to_string(),
                " ".to_string(),
                "toplaşuvı".to_string(),
                " ".to_string(),
                "olıp".to_string(),
                " ".to_string(),
                "keçti".to_string(),
            ]
        );
    }

    #[test]
    fn java_pin_trailing_hyphen_becomes_its_own_token() {
        assert_eq!(texts("söz-"), vec!["söz".to_string(), "-".to_string()]);
        assert_eq!(texts("-"), vec!["-".to_string()]);
        assert_eq!(texts("a-"), vec!["a".to_string(), "-".to_string()]);
    }

    #[test]
    fn java_pin_n_dash_is_a_tokenizing_character() {
        assert_eq!(
            texts("bir–eki"),
            vec!["bir".to_string(), "–".to_string(), "eki".to_string(),]
        );
    }

    #[test]
    fn java_pin_email_run_is_joined() {
        assert_eq!(
            texts("adresim adam@qirim.net oldı"),
            vec![
                "adresim".to_string(),
                " ".to_string(),
                "adam@qirim.net".to_string(),
                " ".to_string(),
                "oldı".to_string(),
            ]
        );
    }

    #[test]
    fn token_offsets_still_index_the_original_text() {
        let text = "120-cı–üç";
        let tokens = CrimeanTatarWordTokenizer::new().tokenize(text);
        let spans: Vec<&str> = tokens.iter().map(|t| &text[t.start()..t.end()]).collect();
        assert_eq!(spans, vec!["120-cı", "–", "üç"]);
    }
}
