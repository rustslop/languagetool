use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchRule,
    SuggestedReplacement, TextLevelRule,
};
use og_rules::postponed_adjective::LangSynthesizer;
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

const MAX_WORDS_DISTANCE: u32 = 150;
const SYNONYMS_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/fr/src/main/resources/org/languagetool/rules/fr/synonyms.txt"
);

struct SynonymsData {
    postag: Option<String>,
    chunk: Option<String>,
    synonyms: Vec<String>,
}

fn java_split(value: &str, sep: char) -> Vec<String> {
    let mut parts: Vec<String> = value.split(sep).map(str::to_string).collect();
    while parts.last().is_some_and(String::is_empty) {
        parts.pop();
    }
    parts
}

fn load_words(data: &str) -> HashMap<String, SynonymsData> {
    let mut map = HashMap::new();
    for line in data.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let main_parts = java_split(line, '=');
        if main_parts.len() == 2 {
            let word_pos_chunk = java_split(&main_parts[0], '/');
            let word = word_pos_chunk[0].clone();
            let postag = word_pos_chunk.get(1).cloned();
            let chunk = word_pos_chunk.get(2).cloned();
            let synonyms = java_split(&main_parts[1], ';');
            if !word.is_empty() && !synonyms.is_empty() && !map.contains_key(&word) {
                map.insert(
                    word,
                    SynonymsData {
                        postag,
                        chunk,
                        synonyms,
                    },
                );
            }
        } else if main_parts.len() == 1 {
            let parts = java_split(line, ';');
            for key in &parts {
                if map.contains_key(key) {
                    continue;
                }
                let values: Vec<String> = parts.iter().filter(|v| *v != key).cloned().collect();
                map.insert(
                    key.clone(),
                    SynonymsData {
                        postag: None,
                        chunk: None,
                        synonyms: values,
                    },
                );
            }
        }
    }
    map
}

fn adjust_postag(postag: &str) -> String {
    let replacements: &[(&str, &str)] = &[
        ("e sp", ". .*"),
        ("m s", "[me] sp?"),
        ("f s", "[fe] sp?"),
        ("m p", "[me] s?p"),
        ("f p", "[fe] s?p"),
        ("e s", "[me] sp?"),
        ("e p", "[me] s?p"),
        ("m sp", "[me] s?p?"),
        ("f sp", "[fe] s?p?"),
    ];
    for (suffix, target) in replacements {
        if let Some(prefix) = postag.strip_suffix(suffix) {
            return format!("{prefix}{target}");
        }
    }
    postag.to_string()
}

fn is_all_uppercase(s: &str) -> bool {
    s.chars().any(char::is_alphabetic) && !s.chars().any(char::is_lowercase)
}

fn is_capitalized_word(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_uppercase)
        && s.chars().skip(1).all(|c| !c.is_uppercase())
        && s.chars().any(char::is_alphabetic)
}

fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn to_id_suffix(s: &str) -> String {
    s.to_uppercase()
        .replace(' ', "_")
        .replace('\'', "_Q_")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn pattern_full_match(value: &str, pattern: &str) -> bool {
    static CACHE: OnceLock<std::sync::Mutex<HashMap<String, Option<Regex>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut guard = cache.lock().unwrap();
    let compiled = guard
        .entry(pattern.to_string())
        .or_insert_with(|| Regex::new(&format!("^(?:{pattern})$")).ok())
        .clone();
    match compiled {
        Some(re) => re.is_match(value),
        None => value == pattern,
    }
}

fn is_punctuation(token: &str) -> bool {
    pattern_full_match(token, r"\p{P}")
}

pub struct FrenchRepeatedWordsRule {
    words: HashMap<String, SynonymsData>,
    synthesizer: Option<Arc<LangSynthesizer>>,
}

impl FrenchRepeatedWordsRule {
    pub fn new(synthesizer: Option<Arc<LangSynthesizer>>) -> Self {
        Self {
            words: load_words(SYNONYMS_TXT),
            synthesizer,
        }
    }

    pub fn entry_count(&self) -> usize {
        self.words.len()
    }

    fn is_exception(
        &self,
        atr: &AnalyzedTokenReadings,
        sent_start: bool,
        is_capitalized: bool,
        is_all_upper: bool,
    ) -> bool {
        if is_all_upper || (is_capitalized && !sent_start) {
            return true;
        }
        atr.readings()
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t.starts_with('Z')))
    }
}

impl TextLevelRule for FrenchRepeatedWordsRule {
    fn id(&self) -> &str {
        "FR_REPEATEDWORDS"
    }

    fn description(&self) -> &str {
        "Synonymes de mots répétés."
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("REPETITIONS_STYLE", "Répétitions (Style)")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut words_last_seen: HashMap<&str, u32> = HashMap::new();
        let mut word_number: u32 = 0;
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            let last_token = tokens.last().map(|t| t.token().token()).unwrap_or_default();
            if last_token != "." && last_token != "!" && last_token != "?" {
                continue;
            }
            let mut sent_start = true;
            let mut lemmas_in_sentence: Vec<&str> = Vec::new();
            for atr in &tokens {
                let token = atr.token().token();
                if !token.is_empty() {
                    word_number += 1;
                }
                let is_capitalized = is_capitalized_word(token);
                let is_all_upper = is_all_uppercase(token);
                if sent_start && !token.is_empty() && !is_punctuation(token) {
                    sent_start = false;
                }
                if token.is_empty()
                    || self.is_exception(atr, sent_start, is_capitalized, is_all_upper)
                {
                    continue;
                }
                let mut lemmas: Vec<&str> = Vec::new();
                for reading in atr.readings() {
                    let Some(lemma) = reading.lemma() else {
                        continue;
                    };
                    lemmas.push(lemma);
                    let Some(data) = self.words.get(lemma) else {
                        continue;
                    };
                    let Some(&seen) = words_last_seen.get(lemma) else {
                        continue;
                    };
                    if lemmas_in_sentence.contains(&lemma)
                        || word_number.saturating_sub(seen) > MAX_WORDS_DISTANCE
                    {
                        continue;
                    }
                    let postag_ok = match &data.postag {
                        Some(postag) => pattern_full_match(
                            reading.pos_tags().first().map(String::as_str).unwrap_or(""),
                            postag,
                        ),
                        None => true,
                    };
                    let chunk_ok = match &data.chunk {
                        Some(chunk) => atr.chunks().iter().any(|c| pattern_full_match(c, chunk)),
                        None => true,
                    };
                    if !postag_ok || !chunk_ok {
                        continue;
                    }
                    let sub_id = format!("{}_{}", self.id(), to_id_suffix(lemma));
                    let mut replacements: Vec<String> = Vec::new();
                    let adjusted =
                        adjust_postag(reading.pos_tags().first().map(String::as_str).unwrap_or(""));
                    for synonym in &data.synonyms {
                        let mut forms = Vec::new();
                        if let Some(synth) = &self.synthesizer
                            && let Ok(re) = Regex::new(&format!("^(?:{adjusted})$"))
                        {
                            forms = synth.synthesize(synonym, &re);
                        }
                        if forms.is_empty() {
                            forms.push(synonym.clone());
                        }
                        for mut form in forms {
                            if is_all_upper {
                                form = form.to_uppercase();
                            } else if is_capitalized {
                                form = uppercase_first_char(&form);
                            }
                            replacements.push(form);
                        }
                    }
                    let from = atr.token().start();
                    let to = atr.token().end();
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str())
                        .with_sub_id(sub_id);
                    let ctx_text =
                        crate::fr::context_window(sentence.text(), sentence.start(), from, to);
                    matches.push(
                        RuleMatch::new(
                            "Ce mot apparaît déjà dans l'une des phrases précédant immédiatement celle-ci. Utilisez un synonyme pour apporter plus de variété à votre texte, excepté si la répétition est intentionnelle.",
                            from,
                            to - from,
                            rm_rule,
                            og_core::RuleMatchContext::new(ctx_text, from, to - from),
                        )
                        .with_short_message("Style : Mot répété")
                        .with_replacements(
                            replacements.iter().map(SuggestedReplacement::new).collect(),
                        ),
                    );
                    break;
                }
                for lemma in lemmas {
                    if self.words.contains_key(lemma) {
                        words_last_seen.insert(lemma, word_number);
                        if !lemmas_in_sentence.contains(&lemma) {
                            lemmas_in_sentence.push(lemma);
                        }
                    }
                }
            }
        }
        let _ = text;
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    fn sentence_with_readings(text: &str, words: &[(&str, &str, &str)]) -> AnalyzedSentence {
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0usize;
        for (surface, lemma, postag) in words {
            let start = text[offset..]
                .find(surface)
                .map(|i| offset + i)
                .unwrap_or(offset);
            let end = start + surface.len();
            let reading = AnalyzedToken::new(*surface, start, end)
                .with_lemma(lemma.to_string())
                .with_pos_tags(vec![postag.to_string()]);
            tokens.push(AnalyzedTokenReadings::new(reading.clone()).with_readings(vec![reading]));
            offset = end;
        }
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        sentence.set_tokens(tokens);
        sentence
    }

    fn fr_sentences() -> Vec<AnalyzedSentence> {
        vec![
            sentence_with_readings(
                "Elle est notamment phénoménale.",
                &[
                    ("Elle", "il", "P f s"),
                    ("est", "être", "V e 3 s"),
                    ("notamment", "notamment", "A"),
                    ("phénoménale", "phénoménal", "J f s"),
                    (".", ".", "PUNCT"),
                ],
            ),
            sentence_with_readings(
                " Les choses sont notamment compliquées.",
                &[
                    ("Les", "le", "D f p"),
                    ("choses", "chose", "N f p"),
                    ("sont", "être", "V e 3 p"),
                    ("notamment", "notamment", "A"),
                    ("compliquées", "compliqué", "J f p"),
                    (".", ".", "PUNCT"),
                ],
            ),
        ]
    }

    fn replacement_values(matches: &[RuleMatch]) -> Vec<String> {
        matches[0]
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect()
    }

    #[test]
    fn same_sentence_repetition_is_not_flagged() {
        let rule = FrenchRepeatedWordsRule::new(None);
        let sentences = vec![sentence_with_readings(
            "Elle est notamment phénoménale notamment.",
            &[
                ("Elle", "il", "P f s"),
                ("est", "être", "V e 3 s"),
                ("notamment", "notamment", "A"),
                ("phénoménale", "phénoménal", "J f s"),
                ("notamment", "notamment", "A"),
                (".", ".", "PUNCT"),
            ],
        )];
        assert!(rule.match_text("", &sentences).is_empty());
    }

    #[test]
    fn cross_sentence_repetition_suggests_synonyms() {
        let rule = FrenchRepeatedWordsRule::new(None);
        let matches = rule.match_text("", &fr_sentences());
        assert_eq!(matches.len(), 1);
        assert_eq!(
            replacement_values(&matches),
            vec![
                "particulièrement",
                "spécialement",
                "singulièrement",
                "surtout",
                "spécifiquement",
            ]
        );
    }

    #[test]
    fn cross_sentence_repetition_for_maintenant() {
        let rule = FrenchRepeatedWordsRule::new(None);
        let sentences = vec![
            sentence_with_readings(
                "Elle est maintenant phénoménale.",
                &[
                    ("Elle", "il", "P f s"),
                    ("est", "être", "V e 3 s"),
                    ("maintenant", "maintenant", "A"),
                    ("phénoménale", "phénoménal", "J f s"),
                    (".", ".", "PUNCT"),
                ],
            ),
            sentence_with_readings(
                " Les choses sont maintenant compliquées.",
                &[
                    ("Les", "le", "D f p"),
                    ("choses", "chose", "N f p"),
                    ("sont", "être", "V e 3 p"),
                    ("maintenant", "maintenant", "A"),
                    ("compliquées", "compliqué", "J f p"),
                    (".", ".", "PUNCT"),
                ],
            ),
        ];
        let matches = rule.match_text("", &sentences);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            replacement_values(&matches),
            vec!["présentement", "ce jour-ci", "désormais", "à présent"]
        );
    }

    #[test]
    fn sentence_without_terminal_punctuation_is_skipped() {
        let rule = FrenchRepeatedWordsRule::new(None);
        let sentences = vec![
            sentence_with_readings(
                "Elle est notamment phénoménale",
                &[
                    ("Elle", "il", "P f s"),
                    ("est", "être", "V e 3 s"),
                    ("notamment", "notamment", "A"),
                    ("phénoménale", "phénoménal", "J f s"),
                ],
            ),
            sentence_with_readings(
                " Les choses sont notamment compliquées.",
                &[
                    ("Les", "le", "D f p"),
                    ("choses", "chose", "N f p"),
                    ("sont", "être", "V e 3 p"),
                    ("notamment", "notamment", "A"),
                    ("compliquées", "compliqué", "J f p"),
                    (".", ".", "PUNCT"),
                ],
            ),
        ];
        assert!(rule.match_text("", &sentences).is_empty());
    }

    #[test]
    fn data_loads_expected_entries() {
        let rule = FrenchRepeatedWordsRule::new(None);
        assert!(rule.entry_count() > 100);
        assert!(rule.words.contains_key("notamment"));
        assert!(rule.words.contains_key("maintenant"));
    }

    #[test]
    fn adjust_postag_maps_gender_number_suffixes() {
        assert_eq!(adjust_postag("J e sp"), "J . .*");
        assert_eq!(adjust_postag("N m s"), "N [me] sp?");
        assert_eq!(adjust_postag("N f p"), "N [fe] s?p");
        assert_eq!(adjust_postag("A"), "A");
    }
}
