pub struct MorfologikView(std::sync::Arc<og_tagger::morfologik::MorfologikDictionary>);

impl MorfologikView {
    pub fn new(dictionary: std::sync::Arc<og_tagger::morfologik::MorfologikDictionary>) -> Self {
        Self(dictionary)
    }

    pub fn contains(&self, word: &str) -> bool {
        !self.0.lookup(word).is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        !self.0.has_entries()
    }

    pub fn word_list(&self) -> Vec<String> {
        self.0.fsa_word_list().unwrap_or_else(|| self.0.word_list())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn decoded_source_view_reports_entries_and_words() {
        let dict = MorfologikView::new(Arc::new(
            og_tagger::morfologik::MorfologikDictionary::from_decoded_text(
                "Buch\tBuch\tSUB:NOM:SIN:NEU\nAuto\tAuto\tSUB:NOM:SIN:NEU\n",
            ),
        ));
        assert!(!dict.is_empty());
        assert_eq!(dict.len(), 2);
        assert!(dict.contains("Buch"));
        assert!(dict.contains("Auto"));
        assert!(!dict.contains("Kein"));
        let mut words = dict.word_list();
        words.sort();
        assert_eq!(words, vec!["Auto".to_string(), "Buch".to_string()]);
    }

    #[test]
    fn empty_decoded_source_view_is_empty() {
        let view = MorfologikView::new(Arc::new(
            og_tagger::morfologik::MorfologikDictionary::from_decoded_text(""),
        ));
        assert!(view.is_empty());
        assert_eq!(view.len(), 0);
        assert!(view.word_list().is_empty());
        assert!(!view.contains("anything"));
    }

    #[test]
    fn fsa_source_view_lists_words_from_the_fsa() {
        let dir = std::path::PathBuf::from(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/tl/src/main/resources/org/languagetool/resource/tl",
        );
        if !dir.join("tagalog.dict").exists() {
            eprintln!("Skipping: tl resources not present");
            return;
        }
        let dict = MorfologikView::new(Arc::new(
            og_tagger::morfologik::MorfologikDictionary::load(
                &dir.join("tagalog.dict"),
                &dir.join("tagalog.info"),
            )
            .expect("tagalog loads"),
        ));
        assert!(!dict.is_empty());
        assert!(dict.len() > 1000);
        let words = dict.word_list();
        assert!(!words.is_empty());
        assert!(dict.contains(&words[0]));
        assert!(!dict.contains("zzzznotawordzzzz"));
    }
}
