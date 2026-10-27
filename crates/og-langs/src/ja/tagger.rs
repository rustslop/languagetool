use og_core::{AnalyzedToken, AnalyzedTokenReadings};

pub struct JapaneseTagger;

impl JapaneseTagger {
    pub fn new() -> Self {
        Self
    }

    pub fn as_analyzed_token(word: &str) -> AnalyzedToken {
        let mut parts: Vec<&str> = word.split(' ').collect();
        while parts.last() == Some(&"") {
            parts.pop();
        }
        if parts.len() != 3 {
            return AnalyzedToken::new(" ", 0, " ".len());
        }
        let mut token = AnalyzedToken::new(parts[0], 0, parts[0].len()).with_lemma(parts[2]);
        if !parts[1].is_empty() {
            token = token.with_pos_tags(vec![parts[1].to_string()]);
        }
        token
    }

    pub fn tag_token_strings(&self, tokens: &[String]) -> Vec<AnalyzedTokenReadings> {
        let mut readings = Vec::with_capacity(tokens.len());
        let mut pos = 0usize;
        for word in tokens {
            let at = Self::as_analyzed_token(word);
            let len = at.token().len();
            let start = pos;
            let end = pos + len;
            let mut adjusted = AnalyzedToken::new(at.token(), start, end);
            if let Some(lemma) = at.lemma() {
                adjusted = adjusted.with_lemma(lemma);
            }
            if !at.pos_tags().is_empty() {
                adjusted = adjusted.with_pos_tags(at.pos_tags().to_vec());
            }
            readings.push(AnalyzedTokenReadings::new(adjusted));
            pos = end;
        }
        readings
    }
}

impl Default for JapaneseTagger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn my_assert(text: &str, expected: &str) {
        let tokenizer = crate::ja::word_tokenizer::JapaneseWordTokenizer::new();
        let tagger = JapaneseTagger::new();
        let triples = tokenizer.tokenize_triples(text);
        let readings = tagger.tag_token_strings(&triples);
        let rendered: Vec<String> = readings
            .iter()
            .map(|r| {
                let t = r.token();
                format!(
                    "{}/[{}]{}",
                    t.token(),
                    t.lemma().unwrap_or("null"),
                    t.pos_tags().first().map(String::as_str).unwrap_or("null")
                )
            })
            .collect();
        assert_eq!(rendered.join(" -- "), expected);
    }

    #[test]
    fn java_pin_tagger_test_sentence_one() {
        my_assert(
            "これは簡単なテストです。",
            "これ/[これ]名詞-代名詞-一般 -- は/[は]助詞-係助詞 -- 簡単/[簡単]名詞-形容動詞語幹 -- \
             な/[だ]助動詞 -- テスト/[テスト]名詞-サ変接続 -- です/[です]助動詞 -- 。/[。]記号-句点",
        );
    }

    #[test]
    fn java_pin_tagger_test_sentence_two() {
        my_assert(
            "私は眠い。",
            "私/[私]名詞-代名詞-一般 -- は/[は]助詞-係助詞 -- 眠い/[眠い]形容詞-自立 -- 。/[。]記号-句点",
        );
    }

    #[test]
    fn java_pin_tagger_test_sentence_three() {
        my_assert(
            "とても冷たい飲み物。",
            "とても/[とても]副詞-助詞類接続 -- 冷たい/[冷たい]形容詞-自立 -- 飲み物/[飲み物]名詞-一般 \
             -- 。/[。]記号-句点",
        );
    }

    #[test]
    fn malformed_triples_decode_to_the_java_space_token() {
        let at = JapaneseTagger::as_analyzed_token("notelement");
        assert_eq!(at.token(), " ");
        assert!(at.pos_tags().is_empty());
        assert_eq!(at.lemma(), None);
        let at = JapaneseTagger::as_analyzed_token("a  b");
        assert_eq!(at.token(), "a");
        assert!(at.pos_tags().is_empty());
        assert_eq!(at.lemma(), Some("b"));
        let readings = JapaneseTagger::new().tag_token_strings(&["x".to_string()]);
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].token().token(), " ");
    }
}
