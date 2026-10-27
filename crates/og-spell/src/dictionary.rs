use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DictionaryError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid dictionary format: {0}")]
    InvalidFormat(String),
}

#[derive(Clone)]
pub struct Dictionary {
    words: HashSet<String>,
    lowercased: HashSet<String>,
    shared: Option<std::sync::Arc<crate::morfologik_view::MorfologikView>>,
    fsa: Option<std::sync::Arc<og_tagger::morfologik::FsaWordSet>>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self {
            words: HashSet::new(),
            lowercased: HashSet::new(),
            shared: None,
            fsa: None,
        }
    }

    pub fn from_shared_words(
        shared: std::sync::Arc<crate::morfologik_view::MorfologikView>,
    ) -> Self {
        Self {
            words: HashSet::new(),
            lowercased: HashSet::new(),
            shared: Some(shared),
            fsa: None,
        }
    }

    pub fn from_fsa_word_set(fsa: std::sync::Arc<og_tagger::morfologik::FsaWordSet>) -> Self {
        Self {
            words: HashSet::new(),
            lowercased: HashSet::new(),
            shared: None,
            fsa: Some(fsa),
        }
    }

    pub fn from_file(path: &Path) -> Result<Self, DictionaryError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut dict = Self::new();

        for line in reader.lines() {
            let line = line?;
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                dict.words.insert(word.to_string());
                dict.lowercased.insert(word.to_lowercase());
            }
        }

        Ok(dict)
    }

    pub fn from_morfologik(path: &Path) -> Result<Self, DictionaryError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut dict = Self::new();

        for line in reader.lines() {
            let line = line?;
            let word = line.split('\t').next().unwrap_or("").trim();
            if !word.is_empty() && !word.starts_with('#') {
                dict.words.insert(word.to_string());
                dict.lowercased.insert(word.to_lowercase());
            }
        }

        Ok(dict)
    }

    pub fn from_words(words: &[&str]) -> Self {
        let mut dict = Self::new();
        for word in words {
            dict.add_word(word);
        }
        dict
    }

    pub fn add_word(&mut self, word: &str) {
        self.words.insert(word.to_string());
        let lowered = word.to_lowercase();
        if lowered != word {
            self.lowercased.insert(lowered);
        }
    }

    pub fn contains(&self, word: &str) -> bool {
        let lowered = word.to_lowercase();
        if self.words.contains(word) || self.words.contains(&lowered) {
            return true;
        }
        if self.lowercased.contains(&lowered) {
            return true;
        }
        if let Some(view) = self.shared.as_ref()
            && (view.contains(word) || view.contains(&lowered))
        {
            return true;
        }
        if let Some(fsa) = self.fsa.as_ref() {
            return fsa.contains(word) || fsa.contains(&lowered);
        }
        false
    }

    pub fn contains_exact(&self, word: &str) -> bool {
        self.words.contains(word)
            || self.shared.as_ref().is_some_and(|view| view.contains(word))
            || self.fsa.as_ref().is_some_and(|fsa| fsa.contains(word))
    }

    pub fn len(&self) -> usize {
        self.shared.as_ref().map_or(0, |view| view.len())
            + self.fsa.as_ref().map_or(0, |fsa| fsa.word_count() as usize)
            + self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
            && self.shared.as_ref().is_none_or(|view| view.is_empty())
            && self.fsa.as_ref().is_none_or(|fsa| fsa.is_empty())
    }

    pub fn word_list(&self) -> Vec<String> {
        let mut words: Vec<String> = self.words.iter().cloned().collect();
        if let Some(view) = self.shared.as_ref() {
            words.extend(view.word_list());
        }
        if let Some(fsa) = self.fsa.as_ref() {
            words.extend(fsa.word_list());
        }
        words
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct TempDictFile {
        path: std::path::PathBuf,
    }

    impl TempDictFile {
        fn new(name: &str, content: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("og-spell-dict-{name}-{}.txt", std::process::id()));
            std::fs::write(&path, content).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDictFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn tl_hunspell_dir() -> Option<std::path::PathBuf> {
        let dir = std::path::PathBuf::from(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/tl/src/main/resources/org/languagetool/resource/tl/hunspell",
        );
        dir.join("tl_PH.dict").exists().then_some(dir)
    }

    #[test]
    fn default_and_new_start_empty() {
        let dict = Dictionary::default();
        assert!(dict.is_empty());
        assert_eq!(dict.len(), 0);
        assert!(dict.word_list().is_empty());
        assert!(!dict.contains("anything"));
        assert!(!dict.contains_exact("anything"));
    }

    #[test]
    fn from_file_skips_comments_and_blank_lines() {
        let file = TempDictFile::new(
            "comments",
            "# a comment\n\nHaus\n  Haus  \ntür\n#trailing comment\n",
        );
        let dict = Dictionary::from_file(file.path()).expect("temp file parses");
        assert!(dict.contains("Haus"));
        assert!(dict.contains("tür"));
        assert!(!dict.contains("comment"));
        assert_eq!(dict.words.len(), 2);
    }

    #[test]
    fn from_file_missing_path_is_an_io_error() {
        let missing = std::env::temp_dir().join("og-spell-dict-definitely-missing.txt");
        let err = Dictionary::from_file(&missing)
            .err()
            .expect("missing file errors");
        assert!(matches!(err, DictionaryError::Io(_)));
        let err = Dictionary::from_morfologik(&missing)
            .err()
            .expect("missing file errors");
        assert!(matches!(err, DictionaryError::Io(_)));
    }

    #[test]
    fn from_morfologik_takes_first_tab_column() {
        let file = TempDictFile::new(
            "morfologik",
            "# header\nBuch\tSUB:NOM:SIN\tBuch\nAuto\tSUB:NOM:PLU:NEU\tAuto\nno-columns\n",
        );
        let dict = Dictionary::from_morfologik(file.path()).expect("temp file parses");
        assert!(dict.contains("Buch"));
        assert!(dict.contains("Auto"));
        assert!(dict.contains("no-columns"));
        assert!(!dict.contains("SUB:NOM:SIN"));
    }

    #[test]
    fn contains_matches_case_insensitive_equivalence() {
        let mut dict = Dictionary::new();
        dict.add_word("Haus");
        assert!(dict.contains("Haus"));
        assert!(dict.contains("haus"));
        assert!(dict.contains("HAUS"));
        assert!(!dict.contains("Häuser"));
        dict.add_word("tür");
        assert!(dict.contains("TÜR"));
    }

    #[test]
    fn from_words_deduplicates_but_keeps_case_variants() {
        let dict = Dictionary::from_words(&["haus", "haus", "Haus"]);
        assert_eq!(dict.words.len(), 2);
        assert!(dict.contains("haus"));
        assert!(dict.contains("Haus"));
    }

    #[test]
    fn fsa_backend_answers_contains_exact_len_and_word_list() {
        let Some(dir) = tl_hunspell_dir() else {
            eprintln!("Skipping: tl hunspell resources not present");
            return;
        };
        let fsa = Arc::new(
            og_tagger::morfologik::FsaWordSet::load(
                &dir.join("tl_PH.dict"),
                &dir.join("tl_PH.info"),
            )
            .expect("tl_PH FSA loads"),
        );
        let fsa_words = fsa.word_list();
        assert!(!fsa_words.is_empty());
        let probe = &fsa_words[0];

        let dict = Dictionary::from_fsa_word_set(Arc::clone(&fsa));
        assert!(!dict.is_empty());
        assert_eq!(dict.len(), fsa.word_count() as usize);
        assert!(dict.contains(probe));
        assert!(dict.contains_exact(probe));
        assert!(!dict.contains("zzzznotawordzzzz"));

        let mut combined = dict;
        combined.add_word("Haus");
        assert!(combined.contains("haus"));
        assert_eq!(combined.len(), fsa.word_count() as usize + 1);
        let listed = combined.word_list();
        assert!(listed.contains(&"Haus".to_string()));
        assert!(listed.iter().any(|w| w == probe));
    }

    #[test]
    fn shared_and_fsa_backends_stack_behind_inline_words() {
        let Some(hunspell_dir) = tl_hunspell_dir() else {
            eprintln!("Skipping: tl hunspell resources not present");
            return;
        };
        let tagger_dir = hunspell_dir.parent().unwrap();
        let view = Arc::new(crate::morfologik_view::MorfologikView::new(Arc::new(
            og_tagger::morfologik::MorfologikDictionary::load(
                &tagger_dir.join("tagalog.dict"),
                &tagger_dir.join("tagalog.info"),
            )
            .expect("tagalog loads"),
        )));
        let fsa = Arc::new(
            og_tagger::morfologik::FsaWordSet::load(
                &hunspell_dir.join("tl_PH.dict"),
                &hunspell_dir.join("tl_PH.info"),
            )
            .expect("tl_PH FSA loads"),
        );
        let probe = fsa.word_list()[0].clone();

        let dict = Dictionary::from_shared_words(Arc::clone(&view));
        assert!(dict.contains("siguro"));
        assert!(dict.contains_exact("siguro"));
        assert!(!dict.contains("zzzznotawordzzzz"));
        assert!(!dict.is_empty());

        let both = Dictionary {
            words: ["inline".to_string()].into_iter().collect(),
            lowercased: HashSet::new(),
            shared: Some(Arc::clone(&view)),
            fsa: Some(Arc::clone(&fsa)),
        };
        assert!(both.contains("inline"));
        assert!(both.contains(&probe));
        assert!(both.contains("siguro"));
        assert!(both.contains_exact(&probe));
        assert_eq!(both.len(), 1 + view.len() + fsa.word_count() as usize);
        assert!(!both.is_empty());
    }
}
