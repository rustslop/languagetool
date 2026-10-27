use crate::spelling_support::{SpellerVariant, prepare_line_for_speller};
use og_core::{AnalyzedToken, AnalyzedTokenReadings, RuleMatch};
use og_spell::Dictionary;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub const FRENCH_SUPPRESS_MISSPELLED_FILTER_CLASS: &str =
    "org.languagetool.rules.fr.FrenchSuppressMisspelledSuggestionsFilter";

pub fn fr_resource_dir() -> Option<PathBuf> {
    [
        PathBuf::from("languagetool/languagetool-language-modules/fr/src/main/resources/org/languagetool/resource/fr"),
        PathBuf::from("../languagetool/languagetool-language-modules/fr/src/main/resources/org/languagetool/resource/fr"),
        PathBuf::from("../languagetool-language-modules/fr/src/main/resources/org/languagetool/resource/fr"),
        PathBuf::from("../../languagetool/languagetool-language-modules/fr/src/main/resources/org/languagetool/resource/fr"),
        PathBuf::from("../../languagetool-language-modules/fr/src/main/resources/org/languagetool/resource/fr"),
        PathBuf::from("../../../languagetool-language-modules/fr/src/main/resources/org/languagetool/resource/fr"),
        PathBuf::from("/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/fr/src/main/resources/org/languagetool/resource/fr"),
    ]
    .into_iter()
    .find(|p| p.join("french.dict").exists())
    .and_then(|p| p.canonicalize().ok())
}

pub fn shared_french_spelling_dictionary() -> Option<Arc<Dictionary>> {
    static CACHE: og_core::releasable::Releasable<Dictionary> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let resource_dir = fr_resource_dir()?;
        let dictionary = og_tagger::generic_tagger::shared_dictionary(&resource_dir, "french")?;
        let mut dict = Dictionary::from_shared_words(Arc::new(
            og_spell::morfologik_view::MorfologikView::new(dictionary),
        ));
        if let Ok(text) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt")) {
            for line in text.lines() {
                for word in prepare_line_for_speller(SpellerVariant::French, line) {
                    if !word.is_empty() {
                        dict.add_word(&word);
                    }
                }
            }
        }
        Some(Arc::new(dict))
    })
}

pub fn is_misspelled(word: &str) -> bool {
    if word.is_empty() || !word.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    match shared_french_spelling_dictionary() {
        Some(dictionary) => !dictionary.contains(word),
        None => false,
    }
}

fn shared_french_tagger() -> Option<Arc<og_tagger::GenericTagger>> {
    static CACHE: og_core::releasable::Releasable<og_tagger::GenericTagger> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let tagger = og_tagger::GenericTagger::french(&fr_resource_dir()?);
        Some(Arc::new(tagger))
    })
}

fn shared_french_tokenizer() -> Option<Arc<og_tokenizer::FrenchWordTokenizer>> {
    static CACHE: og_core::releasable::Releasable<og_tokenizer::FrenchWordTokenizer> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let tagger = shared_french_tagger()?;
        Some(Arc::new(og_tokenizer::FrenchWordTokenizer::new(Arc::new(
            move |w: &str| tagger.is_word_tagged(w),
        ))))
    })
}

pub fn replacement_is_misspelled(value: &str) -> bool {
    match shared_french_tokenizer() {
        Some(tokenizer) => {
            use og_tokenizer::WordTokenizer as _;
            tokenizer
                .tokenize(value)
                .iter()
                .any(|t| is_misspelled(t.text()))
        }
        None => value.split_whitespace().any(is_misspelled),
    }
}

pub fn parse_filter_args(args: &str, matched_texts: &[String]) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for part in args.split_whitespace() {
        let Some((key, val)) = part.split_once(':') else {
            continue;
        };
        let resolved = if let Some(rest) = val.strip_prefix('\\')
            && let Ok(idx) = rest.parse::<usize>()
            && idx > 0
            && idx <= matched_texts.len()
        {
            matched_texts[idx - 1].clone()
        } else if let Ok(idx) = val.parse::<usize>()
            && idx > 0
            && idx <= matched_texts.len()
        {
            matched_texts[idx - 1].clone()
        } else {
            val.to_string()
        };
        result.insert(key.to_string(), resolved);
    }
    result
}

pub fn replacement_matches_postag(word: &str, pattern: &str) -> bool {
    let Some(tagger) = shared_french_tagger() else {
        return false;
    };
    let readings = og_tagger::Tagger::tag(tagger.as_ref(), &[word])
        .into_iter()
        .next()
        .map(|atr| atr.into_readings())
        .unwrap_or_default();
    AnalyzedTokenReadings::new(AnalyzedToken::new(word, 0, 0))
        .with_readings(readings)
        .has_pos_tag_matching(pattern)
}

pub fn accept_rule_match(
    mut rm: RuleMatch,
    args: &str,
    matched_texts: &[String],
) -> Option<RuleMatch> {
    let parsed = parse_filter_args(args, matched_texts);
    let suppress_match = parsed
        .get("suppressMatch")
        .is_none_or(|v| !v.eq_ignore_ascii_case("false"));
    let suppress_postag = parsed.get("SuppressPostag").cloned();
    let filter_postag = parsed.get("FilterPostag").cloned();
    let mut kept = Vec::new();
    for repl in rm.replacements() {
        let value = repl.value();
        if replacement_is_misspelled(value) {
            continue;
        }
        if let Some(sp) = suppress_postag.as_deref()
            && replacement_matches_postag(value, sp)
        {
            continue;
        }
        if let Some(fp) = filter_postag.as_deref()
            && !replacement_matches_postag(value, fp)
        {
            continue;
        }
        kept.push(repl.clone());
    }
    if kept.is_empty() && suppress_match {
        None
    } else {
        rm.set_replacements(kept);
        Some(rm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{RuleMatchContext, RuleMatchRule, SuggestedReplacement};

    fn rm_with(replacements: &[&str]) -> RuleMatch {
        let mut rm = RuleMatch::new(
            "msg",
            0,
            4,
            RuleMatchRule::new("X", "d"),
            RuleMatchContext::new("text".to_string(), 0, 4),
        );
        rm.set_replacements(
            replacements
                .iter()
                .map(|r| SuggestedReplacement::new(*r))
                .collect(),
        );
        rm
    }

    #[test]
    fn walker_resolves_the_french_resource_layout() {
        let dir = fr_resource_dir().expect("fr resource dir");
        assert!(dir.join("french.dict").is_file());
        assert!(dir.join("hunspell/spelling.txt").is_file());
        assert!(shared_french_spelling_dictionary().is_some());
    }

    #[test]
    fn is_misspelled_checks_the_shared_dictionary() {
        assert!(is_misspelled("bonjou"));
        assert!(!is_misspelled("bonjour"));
        assert!(!is_misspelled(""));
        assert!(!is_misspelled("123"));
        assert!(is_misspelled("śćć"));
    }

    #[test]
    fn replacement_is_misspelled_tokenizes_the_value() {
        assert!(replacement_is_misspelled("je bonjou"));
        assert!(!replacement_is_misspelled("je mange"));
        assert!(!replacement_is_misspelled(""));
    }

    #[test]
    fn parse_filter_args_resolves_bare_and_escaped_indices() {
        let matched = vec!["un".to_string(), "deux".to_string()];
        let parsed = parse_filter_args("a:1 b:\\2 c:plain d:\\9", &matched);
        assert_eq!(parsed.get("a").map(String::as_str), Some("un"));
        assert_eq!(parsed.get("b").map(String::as_str), Some("deux"));
        assert_eq!(parsed.get("c").map(String::as_str), Some("plain"));
        assert_eq!(parsed.get("d").map(String::as_str), Some("\\9"));
        assert_eq!(parse_filter_args("nokey a:x", &[]).len(), 1);
    }

    #[test]
    fn replacement_matches_postag_uses_the_french_tagger() {
        assert!(replacement_matches_postag("voiture", "N.*"));
        assert!(!replacement_matches_postag("voiture", "V.*"));
        assert!(replacement_matches_postag("manger", "V.*"));
    }

    #[test]
    fn accept_rule_match_suppresses_fully_misspelled_suggestions() {
        assert!(accept_rule_match(rm_with(&["bonjou", "śćć"]), "", &[]).is_none());
        let out = accept_rule_match(rm_with(&["bonjou", "bonjour"]), "", &[]).expect("kept");
        let values: Vec<&str> = out.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(values, vec!["bonjour"]);
    }

    #[test]
    fn accept_rule_match_can_keep_the_match_without_suggestions() {
        let out = accept_rule_match(rm_with(&["bonjou"]), "suppressMatch:false", &[])
            .expect("kept even with no replacements left");
        assert!(out.replacements().is_empty());
    }

    #[test]
    fn accept_rule_match_honours_suppress_and_filter_postags() {
        let out = accept_rule_match(rm_with(&["voiture", "mangera"]), "SuppressPostag:N.*", &[])
            .expect("kept");
        let values: Vec<&str> = out.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(values, vec!["mangera"]);
        let out = accept_rule_match(
            rm_with(&["voiture"]),
            "FilterPostag:V.* suppressMatch:false",
            &[],
        )
        .expect("kept");
        assert!(out.replacements().is_empty());
        let out = accept_rule_match(rm_with(&["voiture"]), "FilterPostag:N.*", &[]).expect("kept");
        let values: Vec<&str> = out.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(values, vec!["voiture"]);
    }
}
