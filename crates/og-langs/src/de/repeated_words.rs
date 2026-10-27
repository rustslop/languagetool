use super::helpers::{has_pos_tag_starting_with, is_all_uppercase, is_capitalized_word};
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::TextLevelRule,
};
use std::collections::HashMap;
use std::sync::LazyLock;

const MAX_WORDS_DISTANCE: u32 = 150;

struct SynonymsData {
    postag: Option<regex::Regex>,
    chunk: Option<String>,
    synonyms: Vec<String>,
}

static WORDS: LazyLock<HashMap<String, SynonymsData>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for line_original in include_str!(
            "../../../../languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/rules/de/synonyms.txt"
        )
        .lines()
    {
        let hash_stripped = line_original.split('#').next().unwrap_or("").trim();
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
        let postag = word_parts
            .next()
            .and_then(|p| regex::Regex::new(&format!("^(?:{p})$")).ok());
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
});

pub struct GermanRepeatedWordsRule;

impl Default for GermanRepeatedWordsRule {
    fn default() -> Self {
        Self
    }
}

fn is_exception(
    tokens: &[&og_core::AnalyzedTokenReadings],
    i: usize,
    sent_start: bool,
    is_capitalized: bool,
    is_all_uppercase_word: bool,
) -> bool {
    if is_all_uppercase_word || (is_capitalized && !sent_start) {
        return true;
    }
    has_pos_tag_starting_with(tokens[i], "EIG:")
}

impl TextLevelRule for GermanRepeatedWordsRule {
    fn id(&self) -> &str {
        "DE_REPEATEDWORDS"
    }

    fn description(&self) -> &str {
        "Synonyme für wiederholte Wörter."
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("REPETITIONS_STYLE", "Wiederholungen (Stil)")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut words_last_seen: HashMap<String, u32> = HashMap::new();
        let mut word_number: u32 = 0;
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            let last_token = tokens.last().map(|t| t.token().token()).unwrap_or_default();
            if last_token != "." && last_token != "!" && last_token != "?" {
                continue;
            }
            let mut sent_start = true;
            let mut lemmas_in_sentence: Vec<&str> = Vec::new();
            for (i, atr) in tokens.iter().enumerate() {
                let token = atr.token().token();
                if !token.is_empty() {
                    word_number += 1;
                }
                let is_capitalized = is_capitalized_word(token);
                let is_all_uppercase_word = is_all_uppercase(token);
                if sent_start
                    && !token.is_empty()
                    && !token.chars().all(|c| c.is_ascii_punctuation())
                {
                    sent_start = false;
                }
                if token.is_empty() {
                    continue;
                }
                let mut matched = false;
                for reading in atr.readings() {
                    if matched {
                        break;
                    }
                    let Some(lemma) = reading.lemma() else {
                        continue;
                    };
                    let Some(data) = WORDS.get(lemma) else {
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
                        Some(postag) => reading.pos_tags().iter().any(|t| postag.is_match(t)),
                        None => true,
                    };
                    let chunk_ok = match &data.chunk {
                        Some(chunk) => atr.chunks().iter().any(|c| c == chunk),
                        None => true,
                    };
                    if !postag_ok
                        || !chunk_ok
                        || is_exception(
                            &tokens,
                            i,
                            sent_start,
                            is_capitalized,
                            is_all_uppercase_word,
                        )
                    {
                        continue;
                    }
                    let sub_id = format!("DE_REPEATEDWORDS_{}", lemma.replace(' ', "_"));
                    let mut replacements: Vec<String> = Vec::new();
                    for synonym in &data.synonyms {
                        let mut r = synonym.clone();
                        if is_all_uppercase_word {
                            r = r.to_uppercase();
                        } else if is_capitalized {
                            r = super::helpers::uppercase_first_char(&r);
                        }
                        if !replacements.contains(&r) {
                            replacements.push(r);
                        }
                    }
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type("style")
                        .with_sub_id(sub_id);
                    let from = atr.token().start();
                    let to = atr.token().end();
                    matches.push(
                        RuleMatch::new(
                            "Dieses Wort kommt in einem nahe gelegenen vorherigen Satz bereits vor. Verwenden Sie ein Synonym, um Ihren Text abwechslungsreicher zu gestalten, außer die Wiederholung ist beabsichtigt.",
                            from,
                            to - from,
                            rm_rule,
                            RuleMatchContext::new(token.to_string(), from, to - from),
                        )
                        .with_short_message("Stil: Wortwiederholung")
                        .with_replacements(
                            replacements
                                .into_iter()
                                .map(SuggestedReplacement::new)
                                .collect(),
                        ),
                    );
                    matched = true;
                }
                for reading in atr.readings() {
                    if let Some(lemma) = reading.lemma()
                        && WORDS.contains_key(lemma)
                    {
                        words_last_seen.insert(lemma.to_string(), word_number);
                        if !lemmas_in_sentence.contains(&lemma) {
                            lemmas_in_sentence.push(lemma);
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
    use crate::engine::LanguageEngine;

    fn repeated_words_matches(text: &str) -> Vec<(String, Vec<String>)> {
        let engine = LanguageEngine::shared_for_code("de").expect("german engine");
        engine
            .check(text)
            .matches
            .iter()
            .filter(|m| m.rule().id() == "DE_REPEATEDWORDS")
            .map(|m| {
                (
                    m.short_message().unwrap_or_default().to_string(),
                    m.replacements()
                        .iter()
                        .map(|r| r.value().to_string())
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn accepts_capitalized_repetition() {
        let matches = repeated_words_matches("Lauf so schnell wie möglich. Das ist MÖGLICH!");
        assert!(matches.is_empty(), "got {matches:?}");
    }

    #[test]
    fn suggests_hervorragend_synonyms() {
        let matches = repeated_words_matches(
            "Das ist hervorragende Arbeit. Die Ergebnisse sind hervorragend.",
        );
        assert_eq!(matches.len(), 1, "got {matches:?}");
        assert_eq!(
            matches[0].1,
            vec![
                "brillant".to_string(),
                "exzellent".to_string(),
                "perfekt".to_string(),
                "fantastisch".to_string(),
                "traumhaft".to_string()
            ]
        );
    }
}
