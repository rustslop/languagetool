use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, TextLevelRule,
};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

const SYNONYMS_DATA: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/synonyms.txt"
);

const MAX_WORDS_DISTANCE: u32 = 150;

struct SynonymsData {
    postag: Option<String>,
    chunk: Option<String>,
    synonyms: Vec<String>,
}

fn parse_synonyms_file(data: &str) -> HashMap<String, SynonymsData> {
    let mut map = HashMap::new();
    for line_original in data.lines() {
        let hash_stripped = line_original
            .split('#')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if hash_stripped.is_empty() {
            continue;
        }
        let Some((word_part, rest)) = hash_stripped.split_once('=') else {
            continue;
        };
        let mut word_parts = word_part.split('/');
        let Some(word) = word_parts.next() else {
            continue;
        };
        let postag = word_parts.next().map(str::to_string);
        let chunk = word_parts.next().map(str::to_string);
        let synonyms: Vec<String> = rest
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if synonyms.is_empty() {
            continue;
        }
        map.insert(
            word.to_string(),
            SynonymsData {
                postag,
                chunk,
                synonyms,
            },
        );
    }
    map
}

fn shared_words() -> &'static HashMap<String, SynonymsData> {
    static WORDS: OnceLock<HashMap<String, SynonymsData>> = OnceLock::new();
    WORDS.get_or_init(|| parse_synonyms_file(SYNONYMS_DATA))
}

fn shared_synthesizer() -> Option<Arc<og_rules::postponed_adjective::LangSynthesizer>> {
    static SYNTH: og_core::releasable::Releasable<og_rules::postponed_adjective::LangSynthesizer> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&SYNTH, || {
        crate::es::es_resource_dir().and_then(|dir| {
            og_rules::postponed_adjective::LangSynthesizer::for_language(&dir, "es")
                .map(std::sync::Arc::new)
        })
    })
}

fn adjust_postag(postag: &str) -> String {
    for (needle, replacement) in [
        ("CN", ".."),
        ("MS", "[MC][SN]"),
        ("FS", "[FC][SN]"),
        ("MP", "[MC][PN]"),
        ("FP", "[FC][PN]"),
        ("CS", "[MC][SN]"),
        ("CP", "[MC][PN]"),
        ("MN", "[MC][SPN]"),
        ("FN", "[FC][SPN]"),
    ] {
        if let Some(pos) = postag.find(needle) {
            return format!(
                "{}{}{}",
                &postag[..pos],
                replacement,
                &postag[pos + needle.len()..]
            );
        }
    }
    postag.to_string()
}

fn is_capitalized_word(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_uppercase() => chars.all(|c| !c.is_uppercase()),
        _ => false,
    }
}

fn is_all_uppercase(word: &str) -> bool {
    word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
}

fn uppercase_first_char(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => word.to_string(),
    }
}

fn has_pos_tag_starting_with(atr: &AnalyzedTokenReadings, prefix: &str) -> bool {
    atr.readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.starts_with(prefix)))
}

struct AntiPattern {
    tokens: Vec<AntiPatternToken>,
}

enum AntiPatternToken {
    Literal(String),
    Regex(regex::Regex),
}

fn anti_patterns() -> &'static Vec<AntiPattern> {
    static PATTERNS: OnceLock<Vec<AntiPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let literal = |w: &str| AntiPatternToken::Literal(w.to_string());
        let cs_regex = |re: &str| AntiPatternToken::Regex(regex::Regex::new(re).unwrap());
        vec![
            AntiPattern {
                tokens: vec![literal("también"), cs_regex(r".+")],
            },
            AntiPattern {
                tokens: vec![cs_regex(r".+"), literal("también")],
            },
            AntiPattern {
                tokens: vec![cs_regex(r"[Aa]ntes|[Dd]espués"), cs_regex(r"de|del")],
            },
            AntiPattern {
                tokens: vec![cs_regex(r"[Tt]ema|TEMA"), cs_regex(r"\d+|[IXVC]+")],
            },
            AntiPattern {
                tokens: vec![cs_regex(r"[Aa]sí"), literal("que")],
            },
        ]
    })
}

fn matches_anti_pattern(
    pattern: &AntiPattern,
    tokens: &[&AnalyzedTokenReadings],
    i: usize,
) -> bool {
    if i + pattern.tokens.len() > tokens.len() {
        return false;
    }
    pattern
        .tokens
        .iter()
        .enumerate()
        .all(|(k, expected)| match expected {
            AntiPatternToken::Literal(word) => {
                tokens[i - 1 + k].token().token().eq_ignore_ascii_case(word)
            }
            AntiPatternToken::Regex(re) => re.is_match(tokens[i - 1 + k].token().token()),
        })
}

pub struct SpanishRepeatedWordsRule;

impl SpanishRepeatedWordsRule {
    pub fn new() -> Self {
        Self
    }

    fn is_exception(
        &self,
        tokens: &[&AnalyzedTokenReadings],
        i: usize,
        sent_start: bool,
        is_capitalized: bool,
        is_all_upper: bool,
    ) -> bool {
        if is_all_upper || (is_capitalized && !sent_start) {
            return true;
        }
        has_pos_tag_starting_with(tokens[i], "NP") || tokens[i].has_pos_tag("_english_ignore_")
    }
}

impl Default for SpanishRepeatedWordsRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for SpanishRepeatedWordsRule {
    fn id(&self) -> &str {
        "ES_REPEATEDWORDS"
    }

    fn description(&self) -> &str {
        "Sinónimos para palabras repetidas."
    }

    fn is_picky(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("REPETITIONS_STYLE", "Repeticiones")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let words = shared_words();
        let synth = shared_synthesizer();
        let mut matches = Vec::new();
        let mut words_last_seen: HashMap<String, u32> = HashMap::new();
        let mut word_number: u32 = 0;
        let mut pos = 0usize;
        let mut prev_sentence_length = 0usize;
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            pos += prev_sentence_length;
            prev_sentence_length = sentence.text().len();
            let Some(last) = tokens.last() else {
                continue;
            };
            let last_token = last.token().token();
            if last_token != "." && last_token != "!" && last_token != "?" {
                continue;
            }
            let mut sent_start = true;
            let mut lemmas_in_sentence: Vec<String> = Vec::new();
            for (i, atrs) in tokens.iter().enumerate() {
                if atrs.is_immunized() {
                    continue;
                }
                let token = atrs.token().token();
                if !token.is_empty() {
                    word_number += 1;
                }
                let is_capitalized = is_capitalized_word(token);
                let is_all_upper = is_all_uppercase(token);
                let is_exception = token.is_empty()
                    || self.is_exception(&tokens, i, sent_start, is_capitalized, is_all_upper)
                    || anti_patterns()
                        .iter()
                        .any(|p| matches_anti_pattern(p, &tokens, i + 1));
                if sent_start
                    && !token.is_empty()
                    && !(token.chars().count() == 1
                        && token
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_punctuation()))
                {
                    sent_start = false;
                }
                if is_exception {
                    continue;
                }
                let mut lemmas: Vec<Option<&str>> = Vec::new();
                let mut matched = false;
                for reading in atrs.readings() {
                    let lemma = reading.lemma();
                    lemmas.push(lemma);
                    let Some(lemma) = lemma else {
                        continue;
                    };
                    let Some(&seen) = words_last_seen.get(lemma) else {
                        continue;
                    };
                    if lemmas_in_sentence.iter().any(|l| l == lemma)
                        || word_number.saturating_sub(seen) > MAX_WORDS_DISTANCE
                    {
                        continue;
                    }
                    let Some(data) = words.get(lemma) else {
                        continue;
                    };
                    let postag_ok = match &data.postag {
                        Some(postag) => reading.pos_tags().iter().any(|t| {
                            regex::Regex::new(&format!("^(?:{postag})$"))
                                .is_ok_and(|re| re.is_match(t))
                        }),
                        None => true,
                    };
                    if !postag_ok {
                        continue;
                    }
                    let chunk_ok = match &data.chunk {
                        Some(chunk) => atrs.chunks().iter().any(|c| c == chunk),
                        None => true,
                    };
                    if !chunk_ok {
                        continue;
                    }
                    let start = pos + atrs.token().start();
                    let length = atrs.token().end() - atrs.token().start();
                    let sub_id = format!("ES_REPEATEDWORDS_{}", lemma.replace(' ', "_"));
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_picky(true)
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str())
                        .with_sub_id(sub_id);
                    let mut replacements: Vec<String> = Vec::new();
                    if let Some(synth) = synth.as_ref() {
                        for reading in atrs.readings() {
                            let Some(source_tag) = reading.pos_tags().first() else {
                                continue;
                            };
                            let adjusted = adjust_postag(source_tag);
                            for synonym in &data.synonyms {
                                let forms = synth.synthesize_full(synonym, &adjusted);
                                let forms = if forms.is_empty() {
                                    vec![synonym.clone()]
                                } else {
                                    forms
                                };
                                for mut r in forms {
                                    if is_all_upper {
                                        r = r.to_uppercase();
                                    } else if is_capitalized {
                                        r = uppercase_first_char(&r);
                                    }
                                    if !replacements.contains(&r) {
                                        replacements.push(r);
                                    }
                                }
                            }
                        }
                    } else {
                        replacements = data.synonyms.clone();
                    }
                    matches.push(
                        RuleMatch::new(
                            "Esta palabra ya ha aparecido en una de las frases inmediatamente anteriores. Puede usar un sinónimo para hacer más interesante el texto, excepto si la repetición es intencionada.",
                            start,
                            length,
                            rm_rule,
                            RuleMatchContext::new(token.to_string(), start, length),
                        )
                        .with_replacements(
                            replacements
                                .into_iter()
                                .map(SuggestedReplacement::new)
                                .collect(),
                        )
                        .with_sentence(sentence.text().to_string()),
                    );
                    matched = true;
                    break;
                }
                let _ = matched;
                for lemma in lemmas.iter().flatten() {
                    if words.contains_key(*lemma) {
                        words_last_seen.insert(lemma.to_string(), word_number);
                        if !lemmas_in_sentence.iter().any(|l| l == lemma) {
                            lemmas_in_sentence.push(lemma.to_string());
                        }
                    }
                }
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::es::test_support::analyze_spanish_sentences;

    #[test]
    fn suggests_synonym_for_repeated_verb() {
        let rule = SpanishRepeatedWordsRule::new();
        let text = "Te sugiero que lo hagas. Después de todo, te sugiero otra cosa.";
        let sentences = analyze_spanish_sentences(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1);
        assert!(!matches[0].replacements().is_empty());
    }
}
