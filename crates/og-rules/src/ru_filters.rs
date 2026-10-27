use og_core::{AnalyzedToken, AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub const RUSSIAN_SUPPRESS_MISSPELLED_FILTER_CLASS: &str =
    "org.languagetool.rules.ru.RussianSuppressMisspelledSuggestionsFilter";
pub const RUSSIAN_ADVANCED_SYNTHESIZER_FILTER_CLASS: &str =
    "org.languagetool.rules.ru.AdvancedSynthesizerFilter";

pub fn ru_resource_dir() -> Option<PathBuf> {
    [
        PathBuf::from("languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru"),
        PathBuf::from("../languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru"),
        PathBuf::from("../languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru"),
        PathBuf::from("../../languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru"),
        PathBuf::from("../../languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru"),
        PathBuf::from("../../../languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru"),
        PathBuf::from("/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru"),
    ]
    .into_iter()
    .find(|p| p.join("russian_synth.dict").exists())
    .and_then(|p| p.canonicalize().ok())
}

pub struct RussianSynthesizer {
    dict: Arc<og_tagger::morfologik::MorfologikDictionary>,
    possible_tags: Vec<String>,
}

impl RussianSynthesizer {
    pub fn from_resource_dir(resource_dir: &Path) -> Option<Self> {
        let dict = Arc::new(og_tagger::morfologik::MorfologikDictionary::load(
            &resource_dir.join("russian_synth.dict"),
            &resource_dir.join("russian_synth.info"),
        )?);
        let possible_tags = std::fs::read_to_string(resource_dir.join("tags_russian.txt"))
            .map(|text| {
                text.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            dict,
            possible_tags,
        })
    }

    pub fn shared() -> Option<Arc<RussianSynthesizer>> {
        static CACHE: og_core::releasable::Releasable<RussianSynthesizer> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&CACHE, || {
            ru_resource_dir()
                .as_deref()
                .and_then(RussianSynthesizer::from_resource_dir)
                .map(Arc::new)
        })
    }

    pub fn synthesize(&self, lemma: &str, tag: &Regex) -> Vec<String> {
        let Some(synth) = self.dict.synth_forms_by_lemma() else {
            return Vec::new();
        };
        let Some(entries) = synth.get(lemma) else {
            return Vec::new();
        };
        let mut results = Vec::new();
        for possible in &self.possible_tags {
            if !tag.is_match(possible) {
                continue;
            }
            for (entry_tag, form) in entries {
                if &**entry_tag == possible.as_str()
                    && !results.iter().any(|r: &String| r == &**form)
                {
                    results.push(form.to_string());
                }
            }
        }
        results
    }

    pub fn synthesize_exact(&self, lemma: &str, postag: &str) -> Vec<String> {
        let Ok(re) = Regex::new(&format!("^{postag}$")) else {
            return Vec::new();
        };
        self.synthesize(lemma, &re)
    }
}

fn resolve_args<'a>(args: &'a str, matched_texts: &[String]) -> HashMap<&'a str, String> {
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
        } else {
            val.to_string()
        };
        result.insert(key, resolved);
    }
    result
}

fn full_matcher(pattern: &str) -> Regex {
    static CACHE: OnceLock<std::sync::Mutex<HashMap<String, Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap();
    Regex::clone(
        guard
            .entry(pattern.to_string())
            .or_insert_with(|| Regex::new(&format!("^(?:{pattern})$")).unwrap()),
    )
}

fn marker_or_index(
    spec: &str,
    pattern_tokens: &[&AnalyzedTokenReadings],
    rm_from: usize,
) -> Option<usize> {
    if let Some(suffix) = spec.strip_prefix("marker") {
        let mut i = 0usize;
        while i < pattern_tokens.len() && pattern_tokens[i].token().start() < rm_from {
            i += 1;
        }
        i += 1;
        let extra: usize = suffix.parse().unwrap_or(0);
        let one_based = i + extra;
        if one_based < 1 || one_based > pattern_tokens.len() {
            return None;
        }
        Some(one_based - 1)
    } else {
        let n: usize = spec.parse().ok()?;
        if n < 1 || n > pattern_tokens.len() {
            return None;
        }
        Some(n - 1)
    }
}

fn select_reading<'a>(atr: &'a AnalyzedTokenReadings, select: &str) -> (Option<&'a str>, &'a str) {
    const UNKNOWN: &str = "UNKNOWN";
    let matcher = full_matcher(select);
    for reading in atr.readings() {
        for tag in reading.pos_tags() {
            if matcher.is_match(tag) {
                return (reading.lemma(), tag);
            }
        }
    }
    match atr.readings().first() {
        Some(reading) => (
            reading.lemma(),
            reading
                .pos_tags()
                .first()
                .map(String::as_str)
                .unwrap_or(UNKNOWN),
        ),
        None => (None, UNKNOWN),
    }
}

fn composite_postag(
    lemma_select: &str,
    postag_select: &str,
    original_postag: &str,
    desired_postag: &str,
    postag_replace: &str,
) -> String {
    let a = full_matcher(lemma_select).captures(original_postag);
    let b = full_matcher(postag_select).captures(desired_postag);
    let mut result = postag_replace.to_string();
    if let (Some(a), Some(b)) = (a, b) {
        for i in 1..=a.len() {
            if let Some(g) = a.get(i) {
                result = result.replace(&format!(r"\a{i}"), g.as_str());
            }
        }
        for i in 1..=b.len() {
            if let Some(g) = b.get(i) {
                result = result.replace(&format!(r"\b{i}"), g.as_str());
            }
        }
    }
    result
}

fn is_all_uppercase(s: &str) -> bool {
    s.chars().all(|c| !c.is_alphabetic() || !c.is_lowercase())
}

fn is_capitalized_word(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => {
            chars.all(|c| !c.is_alphabetic() || c.is_lowercase())
        }
        _ => false,
    }
}

fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn advanced_synthesizer_filter(
    synthesizer: Option<&RussianSynthesizer>,
    args: &str,
    matched_texts: &[String],
    matched_positions: &[usize],
    tokens: &[&AnalyzedTokenReadings],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let (&first, &last) = (matched_positions.first()?, matched_positions.last()?);
    if first > last || last >= tokens.len() {
        return Some(rm);
    }
    let pattern_tokens = &tokens[first..=last];
    let parsed = resolve_args(args, matched_texts);
    let (Some(postag_select), Some(lemma_select), Some(postag_from_str), Some(lemma_from_str)) = (
        parsed.get("postagSelect"),
        parsed.get("lemmaSelect"),
        parsed.get("postagFrom"),
        parsed.get("lemmaFrom"),
    ) else {
        return Some(rm);
    };
    let new_lemma = parsed.get("newLemma").cloned().unwrap_or_default();
    let postag_replace = parsed.get("postagReplace").cloned();
    let lemma_from = marker_or_index(lemma_from_str, pattern_tokens, rm.offset())?;
    let postag_from = marker_or_index(postag_from_str, pattern_tokens, rm.offset())?;
    let (lemma, original_postag) = {
        let (l, t) = select_reading(pattern_tokens[lemma_from], lemma_select);
        (l.map(str::to_string), t.to_string())
    };
    let desired_postag = select_reading(pattern_tokens[postag_from], postag_select)
        .1
        .to_string();
    let mut desired_lemma = lemma;
    if !new_lemma.is_empty() {
        if new_lemma.starts_with('_') {
            desired_lemma = None;
        } else {
            desired_lemma = Some(new_lemma);
        }
    }
    let desired_lemma = desired_lemma?;
    let desired_postag = match postag_replace {
        Some(replace) => composite_postag(
            lemma_select,
            postag_select,
            &original_postag,
            &desired_postag,
            &replace,
        ),
        None => desired_postag,
    };
    let surface = pattern_tokens[lemma_from].token().token();
    let is_word_capitalized = is_capitalized_word(surface);
    let is_word_all_upper = is_all_uppercase(surface);
    let Some(synthesizer) = synthesizer else {
        return Some(rm);
    };
    let tag_regex = match Regex::new(&desired_postag) {
        Ok(re) => re,
        Err(_) => return Some(rm),
    };
    let replacements = synthesizer.synthesize(&desired_lemma, &tag_regex);
    if replacements.is_empty() {
        return Some(rm);
    }
    let mut list: Vec<String> = Vec::new();
    let mut suggestion_used = false;
    for r in rm.replacements().iter().map(|r| r.value().to_string()) {
        for nr in &replacements {
            if r.contains("{suggestion}")
                || r.contains("{Suggestion}")
                || r.contains("{SUGGESTION}")
            {
                suggestion_used = true;
            }
            let mut nr = nr.clone();
            if is_word_capitalized {
                nr = uppercase_first_char(&nr);
            }
            if is_word_all_upper {
                nr = nr.to_uppercase();
            }
            let complete = r
                .replace("{suggestion}", &nr)
                .replace("{Suggestion}", &uppercase_first_char(&nr))
                .replace("{SUGGESTION}", &nr.to_uppercase());
            if !list.contains(&complete) {
                list.push(complete);
            }
        }
    }
    if !suggestion_used {
        list.extend(replacements.iter().cloned());
    }
    let adapted: Vec<SuggestedReplacement> =
        list.into_iter().map(SuggestedReplacement::new).collect();
    rm.set_replacements(adapted);
    Some(rm)
}

pub fn shared_russian_spelling_dictionary() -> Option<Arc<og_spell::Dictionary>> {
    static CACHE: og_core::releasable::Releasable<og_spell::Dictionary> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let resource_dir = ru_resource_dir()?;
        let dictionary = og_tagger::generic_tagger::shared_dictionary(&resource_dir, "russian")?;
        let mut dict = og_spell::Dictionary::from_shared_words(Arc::new(
            og_spell::morfologik_view::MorfologikView::new(dictionary),
        ));
        if let Ok(text) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt")) {
            for line in text.lines() {
                let word = line.split('#').next().unwrap_or("").trim();
                if !word.is_empty() {
                    dict.add_word(word);
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
    match shared_russian_spelling_dictionary() {
        Some(dictionary) => !dictionary.contains(word),
        None => false,
    }
}

fn shared_russian_tagger() -> Option<Arc<og_tagger::GenericTagger>> {
    static CACHE: og_core::releasable::Releasable<og_tagger::GenericTagger> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let tagger = og_tagger::GenericTagger::russian(&ru_resource_dir()?);
        Some(Arc::new(tagger))
    })
}

pub fn tokenize_for_misspelling(text: &str) -> Vec<String> {
    use og_tokenizer::WordTokenizer as _;
    let raw = og_tokenizer::DefaultWordTokenizer::new_generic().tokenize(text);
    let mut out: Vec<String> = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i + 2 < raw.len() {
        let contiguous =
            raw[i].end() == raw[i + 1].start() && raw[i + 1].end() == raw[i + 2].start();
        let is_abbreviation = raw[i].text() == "б"
            && raw[i + 1].text() == "/"
            && (raw[i + 2].text() == "у" || raw[i + 2].text() == "н");
        if contiguous && is_abbreviation {
            out.push(format!("б/{}", raw[i + 2].text()));
            i += 3;
        } else {
            out.push(raw[i].text().to_string());
            i += 1;
        }
    }
    out.extend(raw[i..].iter().map(|t| t.text().to_string()));
    out
}

pub fn replacement_is_misspelled(value: &str) -> bool {
    tokenize_for_misspelling(value)
        .iter()
        .any(|t| is_misspelled(t))
}

pub fn replacement_matches_postag(word: &str, pattern: &str) -> bool {
    let Some(tagger) = shared_russian_tagger() else {
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
    let parsed = resolve_args(args, matched_texts);
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

    fn synth() -> Option<Arc<RussianSynthesizer>> {
        RussianSynthesizer::shared()
    }

    #[test]
    fn walker_resolves_the_lt_module_layout() {
        let dir = ru_resource_dir().expect("ru resource dir");
        assert!(dir.join("russian_synth.dict").is_file());
        assert!(dir.join("tags_russian.txt").is_file());
        assert!(dir.join("russian.dict").is_file());
    }

    #[test]
    fn synthesizer_loads_and_synthesizes_inflected_forms() {
        let Some(synth) = synth() else {
            panic!("russian_synth.dict must load");
        };
        assert_eq!(
            synth.synthesize_exact("мой", "ADJ:MPR:Neut:Nom"),
            vec!["моё".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact("глупый", "ADJ:Posit:Fem:V"),
            vec!["глупую".to_string()]
        );
        let dative = Regex::new("^ADJ:MPR:(Masc|Neut):D$").unwrap();
        let forms = synth.synthesize("мой", &dative);
        assert!(forms.contains(&"моему".to_string()), "{forms:?}");
        assert!(
            synth
                .synthesize_exact("мой", "ADJ:MPR:Nonexistent:Case")
                .is_empty(),
            "unknown tag must not synthesize"
        );
    }

    fn tagged(tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let tagger = shared_russian_tagger().expect("russian tagger").clone();
        let atrs = og_tagger::Tagger::tag(tagger.as_ref(), tokens);
        let mut out = Vec::with_capacity(atrs.len());
        let mut offset = 0;
        for atr in atrs {
            let len = atr.token().token().len();
            let token = AnalyzedToken::new(atr.token().token(), offset, offset + len);
            out.push(AnalyzedTokenReadings::new(token).with_readings(atr.into_readings()));
            offset += len + 1;
        }
        out
    }

    fn unify_args() -> &'static str {
        "lemmaFrom:1 lemmaSelect:ADJ:MPR:.*:.* postagFrom:2 postagSelect:NN:.*:(.*):.*:(.*) postagReplace:ADJ:MPR:\\b1:\\b2"
    }

    fn run_unify(words: &[&str], args: &str, template: &str) -> Vec<String> {
        let readings = tagged(words);
        let tokens: Vec<&AnalyzedTokenReadings> = readings.iter().collect();
        let matched_texts: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        let text = words.join(" ");
        let mut rm = RuleMatch::new(
            "",
            0,
            text.len(),
            og_core::RuleMatchRule::new("X", "d"),
            og_core::RuleMatchContext::new(text.clone(), 0, text.len()),
        );
        rm.set_replacements(vec![SuggestedReplacement::new(template)]);
        let out = advanced_synthesizer_filter(
            synth().as_deref(),
            args,
            &matched_texts,
            &[0, 1],
            &tokens,
            rm,
        )
        .expect("match kept");
        out.replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect()
    }

    #[test]
    fn advanced_synthesizer_restores_the_unify_padj_nn_case_correction() {
        let values = run_unify(
            &["моему", "отношение"],
            unify_args(),
            "{suggestion} отношение",
        );
        assert_eq!(values, vec!["моё отношение".to_string()]);
    }

    #[test]
    fn advanced_synthesizer_restores_the_unify_padj_nn_gender_correction() {
        let values = run_unify(
            &["моей", "отношение"],
            unify_args(),
            "{suggestion} отношение",
        );
        assert_eq!(values, vec!["моё отношение".to_string()]);
    }

    #[test]
    fn advanced_synthesizer_restores_the_unify_padj_nn_number_correction() {
        let values = run_unify(
            &["эти", "отношение"],
            unify_args(),
            "{suggestion} отношение",
        );
        assert_eq!(values, vec!["это отношение".to_string()]);
    }

    #[test]
    fn advanced_synthesizer_restores_the_unify_adj_nn_case_correction() {
        let values = run_unify(
            &["глупою", "женщину"],
            "lemmaFrom:1 lemmaSelect:ADJ:(Posit|Sup):.*:.* postagFrom:2 postagSelect:NN:.*:(.*):.*:(.*) postagReplace:ADJ:\\a1:\\b1:\\b2",
            "{suggestion} женщину",
        );
        assert_eq!(values, vec!["глупую женщину".to_string()]);
    }

    fn bare_rm(replacements: &[&str]) -> RuleMatch {
        RuleMatch::new(
            "",
            0,
            5,
            og_core::RuleMatchRule::new("X", "d"),
            og_core::RuleMatchContext::new("текст".to_string(), 0, 5),
        )
        .with_replacements(
            replacements
                .iter()
                .map(|r| SuggestedReplacement::new(*r))
                .collect(),
        )
    }

    #[test]
    fn suppress_producer_keeps_known_words() {
        let out = accept_rule_match(bare_rm(&["жаренная"]), "suppressMatch:true", &[]).unwrap();
        let values: Vec<String> = out
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        assert_eq!(values, vec!["жаренная".to_string()]);
    }

    #[test]
    fn suppress_producer_nulls_the_match_when_all_suggestions_misspell() {
        let out = accept_rule_match(bare_rm(&["каждя"]), "suppressMatch:true", &[]);
        assert!(
            out.is_none(),
            "Java drops the match: no suggestion survives"
        );
    }

    #[test]
    fn suppress_producer_keeps_empty_list_when_suppress_match_false() {
        let out = accept_rule_match(bare_rm(&["каждя"]), "suppressMatch:false", &[]).unwrap();
        assert!(out.replacements().is_empty());
    }

    #[test]
    fn misspelling_oracle_matches_java_speller_examples() {
        assert!(!is_misspelled("жаренная"));
        assert!(is_misspelled("каждя"));
        assert!(!is_misspelled("123"));
    }

    #[test]
    fn advanced_synthesizer_handles_degenerate_and_marker_args() {
        let Some(synth) = synth() else {
            panic!("russian_synth.dict must load");
        };
        let readings = tagged(&["моему", "отношение"]);
        let tokens: Vec<&AnalyzedTokenReadings> = readings.iter().collect();
        let matched_texts: Vec<String> = vec!["моему".to_string(), "отношение".to_string()];
        let rm = bare_rm(&["{suggestion} отношение"]);

        assert!(
            advanced_synthesizer_filter(
                Some(synth.as_ref()),
                unify_args(),
                &matched_texts,
                &[],
                &tokens,
                rm.clone()
            )
            .is_none(),
            "no matched positions drops the match"
        );

        let out = advanced_synthesizer_filter(
            Some(synth.as_ref()),
            unify_args(),
            &matched_texts,
            &[0, 9],
            &tokens,
            rm.clone(),
        )
        .expect("kept unchanged");
        assert_eq!(out.replacements()[0].value(), "{suggestion} отношение");

        let out = advanced_synthesizer_filter(
            Some(synth.as_ref()),
            "lemmaFrom:1 lemmaSelect:ADJ:MPR:.*:.*",
            &matched_texts,
            &[0, 1],
            &tokens,
            rm.clone(),
        )
        .expect("kept without the required args");
        assert_eq!(out.replacements()[0].value(), "{suggestion} отношение");

        let underscore = format!("{} newLemma:_", unify_args());
        assert!(
            advanced_synthesizer_filter(
                Some(synth.as_ref()),
                &underscore,
                &matched_texts,
                &[0, 1],
                &tokens,
                rm.clone(),
            )
            .is_none(),
            "newLemma:_ removes the lemma and the match"
        );

        let literal = format!("{} newLemma:наш", unify_args());
        let out = advanced_synthesizer_filter(
            Some(synth.as_ref()),
            &literal,
            &matched_texts,
            &[0, 1],
            &tokens,
            rm.clone(),
        )
        .expect("kept with a literal lemma");
        assert!(
            out.replacements()[0].value().starts_with("наш"),
            "{:?}",
            out.replacements()[0].value()
        );
    }

    #[test]
    fn advanced_synthesizer_applies_marker_specs_and_case() {
        let values = run_unify(
            &["моему", "отношение"],
            "lemmaFrom:marker lemmaSelect:ADJ:MPR:.*:.* postagFrom:2 postagSelect:NN:.*:(.*):.*:(.*) postagReplace:ADJ:MPR:\\b1:\\b2",
            "{suggestion} отношение",
        );
        assert_eq!(values, vec!["моё отношение".to_string()]);

        let readings = tagged(&["Моему", "отношение"]);
        let tokens: Vec<&AnalyzedTokenReadings> = readings.iter().collect();
        let matched_texts: Vec<String> = vec!["Моему".to_string(), "отношение".to_string()];
        let text = "Моему отношение";
        let mut rm = RuleMatch::new(
            "",
            0,
            text.len(),
            og_core::RuleMatchRule::new("X", "d"),
            og_core::RuleMatchContext::new(text.to_string(), 0, text.len()),
        );
        rm.set_replacements(vec![SuggestedReplacement::new("{SUGGESTION} отношение")]);
        let Some(synth) = synth() else {
            panic!("russian_synth.dict must load");
        };
        let out = advanced_synthesizer_filter(
            Some(synth.as_ref()),
            unify_args(),
            &matched_texts,
            &[0, 1],
            &tokens,
            rm,
        )
        .expect("kept");
        assert_eq!(
            out.replacements()
                .iter()
                .map(|r| r.value().to_string())
                .collect::<Vec<_>>(),
            vec!["МОЁ отношение".to_string()],
            "capitalized surface uppercases the SUGGESTION placeholder form"
        );
    }

    #[test]
    fn marker_or_index_resolves_specs() {
        let readings = tagged(&["моему", "отношение"]);
        let tokens: Vec<&AnalyzedTokenReadings> = readings.iter().collect();
        assert_eq!(marker_or_index("2", &tokens, 0), Some(1));
        assert_eq!(marker_or_index("0", &tokens, 0), None);
        assert_eq!(marker_or_index("9", &tokens, 0), None);
        assert_eq!(marker_or_index("marker", &tokens, 0), Some(0));
        assert_eq!(
            marker_or_index("marker", &tokens, readings[1].token().start()),
            Some(1)
        );
        assert_eq!(marker_or_index("junk", &tokens, 0), None);
    }

    #[test]
    fn select_reading_falls_back_to_the_first_reading() {
        let untagged = AnalyzedTokenReadings::new(AnalyzedToken::new("слово", 0, 5));
        assert_eq!(select_reading(&untagged, "NN:.*"), (None, "UNKNOWN"));

        let readings = vec![
            AnalyzedToken::new("слово", 0, 5).with_lemma("слово"),
            AnalyzedToken::new("слово", 0, 5).with_lemma("иное"),
        ];
        let atr =
            AnalyzedTokenReadings::new(AnalyzedToken::new("слово", 0, 5)).with_readings(readings);
        assert_eq!(select_reading(&atr, "NN:.*"), (Some("слово"), "UNKNOWN"));
    }

    #[test]
    fn tokenizer_merges_the_b_slash_abbreviations() {
        assert_eq!(
            tokenize_for_misspelling("б/у авто"),
            vec!["б/у".to_string(), " ".to_string(), "авто".to_string()]
        );
        assert_eq!(
            tokenize_for_misspelling("просто текст"),
            vec!["просто".to_string(), " ".to_string(), "текст".to_string()]
        );
        assert!(!is_misspelled("авто"));
    }

    #[test]
    fn suppress_producer_filters_by_postag() {
        let out = accept_rule_match(bare_rm(&["красный"]), "SuppressPostag:ADJ.*", &[]);
        assert!(
            out.is_none(),
            "adjective suggestion suppressed and the match drops"
        );

        let out =
            accept_rule_match(bare_rm(&["стол", "быстро"]), "FilterPostag:NN.*", &[]).unwrap();
        let values: Vec<String> = out
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        assert_eq!(values, vec!["стол".to_string()]);
    }

    #[test]
    fn synthesizer_rejects_unknown_lemmas_and_patterns() {
        let Some(synth) = synth() else {
            panic!("russian_synth.dict must load");
        };
        assert!(
            synth
                .synthesize_exact("несуществующееслово", "ADJ:MPR:Neut:Nom")
                .is_empty()
        );
        assert!(synth.synthesize_exact("мой", "([").is_empty());
    }
}
