use std::path::Path;
use std::sync::{Arc, LazyLock};

use og_rules::postponed_adjective::LangSynthesizer;
use regex::Regex;

const LEMMAS_TO_IGNORE: [&str; 4] = ["enterar", "sentar", "conseguir", "alcançar"];
const CA_ES_VERB_TAGS: &str = "[0CXY12]";

static P_VERB: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^V.*[CVBXYZ0123456]$").unwrap());

pub struct CaSynthesizer {
    inner: Arc<LangSynthesizer>,
    tags: Vec<String>,
}

impl CaSynthesizer {
    pub fn build(resource_dir: &Path, inner: Arc<LangSynthesizer>) -> Self {
        let mut tags: Vec<String> = std::fs::read_to_string(resource_dir.join("ca-ES_tags.txt"))
            .map(|data| {
                data.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if let Ok(added) = std::fs::read_to_string(resource_dir.join("added.txt")) {
            for line in added.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let mut parts = line.splitn(3, '\t');
                if let (Some(_), Some(_), Some(tag)) =
                    (parts.next(), parts.next(), parts.next())
                {
                    let tag = tag.trim().to_string();
                    if !tag.is_empty() && !tags.contains(&tag) {
                        tags.push(tag);
                    }
                }
            }
        }
        Self { inner, tags }
    }

    pub fn synthesize_regexp(&self, lemma: &str, postag: &str) -> Vec<String> {
        if LEMMAS_TO_IGNORE.contains(&lemma) {
            return Vec::new();
        }
        self.synthesize_with_verb_retry(lemma, postag, P_VERB.is_match(postag))
    }

    pub fn synthesize_plain(&self, lemma: &str, postag: &str) -> Vec<String> {
        self.synthesize_with_verb_retry(lemma, postag, postag.starts_with('V'))
    }

    fn synthesize_with_verb_retry(
        &self,
        lemma: &str,
        postag: &str,
        retry_condition: bool,
    ) -> Vec<String> {
        let (head, tail) = if postag.starts_with('V') {
            match lemma.split_once(' ') {
                Some((head, tail)) => (head, Some(tail)),
                None => (lemma, None),
            }
        } else {
            (lemma, None)
        };
        let mut results = self.enumerate(head, postag);
        if results.is_empty() && retry_condition {
            if let Some((last_cut, _)) = postag.char_indices().next_back() {
                let retry = format!("{}{CA_ES_VERB_TAGS}", &postag[..last_cut]);
                results = self.enumerate(head, &retry);
            }
        }
        match tail {
            Some(tail) => results
                .into_iter()
                .map(|form| format!("{form} {tail}"))
                .collect(),
            None => results,
        }
    }

    fn enumerate(&self, lemma: &str, postag: &str) -> Vec<String> {
        let Ok(re) = Regex::new(&format!("^(?:{postag})$")) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for tag in &self.tags {
            if re.is_match(tag) {
                out.extend(self.inner.synthesize_exact(lemma, tag));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> Option<CaSynthesizer> {
        let dir = crate::ca::tagger::ca_resource_dir()?;
        let inner = LangSynthesizer::for_language(&dir, "ca")?;
        Some(CaSynthesizer::build(&dir, Arc::new(inner)))
    }

    #[test]
    fn regexp_mode_enumerates_possible_tags() {
        let Some(synth) = shared() else {
            return;
        };
        assert_eq!(
            synth.synthesize_regexp("entès", "NC[FC][SN]000"),
            vec!["entesa"]
        );
        assert_eq!(synth.synthesize_regexp("un", "DI0[FC][SN]0"), vec!["una"]);
    }

    #[test]
    fn regexp_mode_drops_lemmas_to_ignore() {
        let synth = test_synth("formi\tsentar\tVMIP1S00\n");
        assert!(synth.synthesize_regexp("sentar", "VMIP1S0Z").is_empty());
        assert_eq!(
            synth.synthesize_plain("sentar", "VMIP1S00"),
            vec!["formi"],
            "plain mode has no ignore list"
        );
    }

    #[test]
    fn regexp_mode_applies_the_variant_verb_retry() {
        let synth = test_synth("menjo\tmenjar\tVMIP1S00\n");
        assert!(synth.synthesize_plain("menjar", "VMIP1S00").is_empty()
            || synth.synthesize_plain("menjar", "VMIP1S00") == vec!["menjo"]);
        let forms = synth.synthesize_regexp("menjar", "VMIP1S0Z");
        assert_eq!(
            forms,
            vec!["menjo"],
            "the empty direct enumeration must retry with the ca-ES [0CXY12] tag class"
        );
    }

    #[test]
    fn verb_lemma_with_space_appends_the_tail() {
        let synth = test_synth("portar\tportar\tVMN0000\n");
        let forms = synth.synthesize_regexp("portar al davant", "VMN0000");
        assert_eq!(forms, vec!["portar al davant"]);
    }

    fn test_synth(added: &str) -> CaSynthesizer {
        let base = std::env::temp_dir().join(format!(
            "og-ca-synth-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let resource_dir = base.join("ca");
        std::fs::create_dir_all(&resource_dir).unwrap();
        std::fs::write(
            resource_dir.join("ca-ES_tags.txt"),
            "VMN00000\nVMN0000\nVMIP1S00\n",
        )
        .unwrap();
        std::fs::write(resource_dir.join("added.txt"), added).unwrap();
        std::fs::write(
            resource_dir.join("ca-ES_synth.dict"),
            std::fs::read(crate::ca::tagger::ca_resource_dir()
                .expect("ca resource dir")
                .join("ca-ES_synth.dict"))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            resource_dir.join("ca-ES_synth.info"),
            std::fs::read(crate::ca::tagger::ca_resource_dir()
                .expect("ca resource dir")
                .join("ca-ES_synth.info"))
            .unwrap(),
        )
        .unwrap();
        let inner = LangSynthesizer::for_language(&resource_dir, "ca").unwrap();
        CaSynthesizer::build(&resource_dir, Arc::new(inner))
    }
}
