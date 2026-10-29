use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_tokenizer::WordTokenizer as _;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

static TOKENIZER: LazyLock<og_tokenizer::DefaultWordTokenizer> =
    LazyLock::new(og_tokenizer::DefaultWordTokenizer::new);

fn xml_content_hash(xml: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    xml.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn shared_compiled_rules(
    path: &Path,
    xml: &str,
) -> Option<Arc<og_xml::compiler::CompiledRuleSet>> {
    static CELLS: OnceLock<
        Mutex<HashMap<String, &'static og_core::releasable::Releasable<og_xml::CompiledRuleSet>>>,
    > = OnceLock::new();
    let mut key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    key.set_extension(format!("{:016x}", xml_content_hash(xml)));
    let cell = {
        let cells = CELLS.get_or_init(Default::default);
        let mut guard = cells.lock().unwrap();
        *guard
            .entry(key.to_string_lossy().into_owned())
            .or_insert_with(|| Box::leak(Box::new(og_core::releasable::Releasable::new())))
    };
    if let Some(hit) = og_core::releasable::shared_probe(cell) {
        return Some(hit);
    }
    og_core::releasable::shared_get_or_build(cell, || {
        let rule_set = og_xml::compiler::XmlCompiler::new()
            .compile_file_with_base(xml, path.parent())
            .expect("compile rules file");
        Some(Arc::new(rule_set))
    })
}

pub(crate) fn sentence_range(text: &str, start: usize, end: usize) -> AnalyzedSentence {
    let mut sentence = AnalyzedSentence::new(&text[start..end], start, end);
    let mut tokens = vec![AnalyzedTokenReadings::new(
        AnalyzedToken::new("<S>", start, start).with_pos_tags(vec!["SENT_START".to_string()]),
    )];
    tokens.extend(
        TOKENIZER
            .tokenize(&text[start..end])
            .into_iter()
            .filter(|t| !t.text().trim().is_empty())
            .map(|t| {
                AnalyzedTokenReadings::new(AnalyzedToken::new(
                    t.text(),
                    start + t.start(),
                    start + t.end(),
                ))
            }),
    );
    sentence.set_tokens(tokens);
    sentence
}

pub(crate) fn sentence(text: &str) -> AnalyzedSentence {
    sentence_range(text, 0, text.len())
}

pub(crate) fn sentences_split_after(text: &str, markers: &[&str]) -> Vec<AnalyzedSentence> {
    let mut bounds: Vec<usize> = Vec::new();
    let mut search_from = 0usize;
    for marker in markers {
        let idx = text[search_from..]
            .find(marker)
            .unwrap_or_else(|| panic!("marker {marker} not found in {text}"));
        let end = search_from + idx + marker.len();
        bounds.push(end);
        search_from = end;
    }
    let mut start = 0usize;
    let mut sentences = Vec::with_capacity(bounds.len() + 1);
    for end in bounds {
        sentences.push(sentence_range(text, start, end));
        start = end;
    }
    sentences.push(sentence_range(text, start, text.len()));
    sentences
}

pub(crate) fn token_with_pos(
    text: &str,
    start: usize,
    end: usize,
    pos_tags: &[&str],
) -> AnalyzedTokenReadings {
    AnalyzedTokenReadings::new(
        AnalyzedToken::new(text, start, end)
            .with_pos_tags(pos_tags.iter().map(|t| t.to_string()).collect()),
    )
}
