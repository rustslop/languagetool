use og_tagger::GermanTagger;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub struct CommaTagger {
    tagger: Option<GermanTagger>,
    cache: std::sync::Mutex<HashMap<String, Vec<String>>>,
}

impl CommaTagger {
    pub fn tags_of(&self, word: &str) -> Vec<String> {
        if let Some(hit) = self.cache.lock().unwrap().get(word) {
            return hit.clone();
        }
        let tags: Vec<String> = self
            .tagger
            .as_ref()
            .map(|t| {
                t.tag_word(word, 0, 0)
                    .iter()
                    .flat_map(|r| r.pos_tags().to_vec())
                    .collect()
            })
            .unwrap_or_default();
        self.cache
            .lock()
            .unwrap()
            .insert(word.to_string(), tags.clone());
        tags
    }
}

pub fn shared() -> Arc<CommaTagger> {
    static SHARED: og_core::releasable::Releasable<CommaTagger> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&SHARED, || {
        let tagger = find_resource_dir().and_then(|dir| GermanTagger::from_resource_dir(&dir));
        Some(Arc::new(CommaTagger {
            tagger,
            cache: std::sync::Mutex::new(HashMap::new()),
        }))
    })
    .expect("german comma tagger")
}

fn find_resource_dir() -> Option<PathBuf> {
    [
        PathBuf::from(
            "languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
        ),
        PathBuf::from(
            "../languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
        ),
        PathBuf::from(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
        ),
    ]
    .into_iter()
    .find(|p| p.join("german.dict").exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_lookup_returns_verbs() {
        let shared = shared();
        if shared.tagger.is_none() {
            eprintln!("Skipping: German dictionary not present");
            return;
        }
        let tags = shared.tags_of("komme");
        assert!(
            tags.iter().any(|t| t.starts_with("VER:")),
            "expected verb tags for 'komme', got {tags:?}"
        );
    }
}
