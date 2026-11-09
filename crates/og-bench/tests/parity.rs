//! Pins the English engine's output on the three bench corpora.
//!
//! Each corpus is checked twice (the two signatures must agree within the
//! process) and compared byte-for-byte with `golden/<corpus>.json`. A
//! signature holds every match's rule id, offset, length, message and full
//! ordered replacement list. Bless an intended behavior change with:
//!
//! ```sh
//! UPDATE_GOLDEN=1 cargo test -p og-bench --test parity -- --test-threads=1
//! ```

use og_bench::{Corpus, UPDATE_GOLDEN_ENV};
use og_langs::engine::LanguageEngine;
use std::sync::OnceLock;

/// One English engine for all three tests: a build costs seconds and
/// hundreds of MB.
fn engine() -> &'static LanguageEngine {
    static ENGINE: OnceLock<LanguageEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        assert!(
            og_langs::engine::find_lt_resource_dir().is_some(),
            "LanguageTool resources not found: the goldens pin the full English engine"
        );
        LanguageEngine::english()
    })
}

fn blessing() -> bool {
    std::env::var_os(UPDATE_GOLDEN_ENV).is_some_and(|value| value == "1")
}

fn signature(corpus: &Corpus, en_ngram_data: bool) -> String {
    let result = engine().check(corpus.text);
    og_bench::signature_json(corpus, en_ngram_data, &result.matches)
}

fn assert_matches_golden(name: &str) {
    let corpus = og_bench::corpus(name).expect("a bench corpus");
    let en_ngram_data = og_bench::en_ngram_data_available();
    let actual = signature(corpus, en_ngram_data);
    let again = signature(corpus, en_ngram_data);
    if let Some((line, first, second)) = og_bench::first_difference(&actual, &again) {
        panic!(
            "{name}: two checks in one process disagree at line {line}\n  \
             first:  {first}\n  second: {second}"
        );
    }

    let path = og_bench::golden_path(name);
    if blessing() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("golden dir");
        }
        std::fs::write(&path, &actual)
            .unwrap_or_else(|error| panic!("writing {}: {error}", path.display()));
        eprintln!("blessed {}", path.display());
        return;
    }

    let expected = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{}: {error}\nbless with {UPDATE_GOLDEN_ENV}=1 cargo test -p og-bench --test parity",
            path.display()
        )
    });
    if let Some((line, want, got)) = og_bench::first_difference(&expected, &actual) {
        panic!(
            "{name}: output differs from {} at line {line}\n  golden: {want}\n  actual: {got}\n\
             full actual output:\n{actual}\n\
             if the change is intended, bless it with \
             {UPDATE_GOLDEN_ENV}=1 cargo test -p og-bench --test parity",
            path.display()
        );
    }
}

#[test]
fn clean_corpus_matches_golden() {
    assert_matches_golden("clean");
}

#[test]
fn err_corpus_matches_golden() {
    assert_matches_golden("err");
}

#[test]
fn multi_corpus_matches_golden() {
    assert_matches_golden("multi");
}
