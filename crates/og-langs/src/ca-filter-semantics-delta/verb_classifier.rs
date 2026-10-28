use std::collections::HashMap;
use std::sync::LazyLock;

const VTR_CODE: u8 = 0b0000_0001;
const INTR_CODE: u8 = 0b0000_0010;
#[allow(dead_code)]
const PRON_CODE: u8 = 0b0000_0100;

const VERBS_CLASSIFICATION: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/ca/src/main/resources/org/languagetool/resource/ca/verbs_classification.txt"
);

static CLASSIFIER: LazyLock<HashMap<&'static str, u8>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for raw in VERBS_CLASSIFICATION.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((lemma, tags)) = line.split_once('=') else {
            continue;
        };
        let mut code = 0u8;
        if tags.contains("vtr") {
            code |= VTR_CODE;
        }
        if tags.contains("intr") || tags.contains("abs") {
            code |= INTR_CODE;
        }
        if tags.contains("pron") {
            code |= PRON_CODE;
        }
        map.insert(lemma, code);
    }
    map
});

pub fn is_verb_code(lemma: &str, code: u8) -> bool {
    CLASSIFIER.get(lemma).is_some_and(|c| c & code != 0)
}

pub fn is_intransitive(lemma: &str) -> bool {
    is_verb_code(lemma, INTR_CODE) && !is_verb_code(lemma, VTR_CODE)
}
