use og_core::{AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchRule, TextLevelRule};
use std::collections::HashMap;

const MAX_WORDS_DISTANCE: u32 = 150;

struct SynonymsData {
    postag: Option<String>,
    chunk: Option<String>,
    synonyms: Vec<String>,
}

pub struct EnglishRepeatedWordsRule {
    words: HashMap<String, SynonymsData>,
}

fn parse_synonyms_file(data: &str) -> HashMap<String, SynonymsData> {
    let mut map = HashMap::new();
    for line_original in data.lines() {
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

impl EnglishRepeatedWordsRule {
    pub fn new() -> Self {
        let words = og_langs_support::english_synonyms_data().unwrap_or_default();
        Self {
            words: parse_synonyms_file(&words),
        }
    }
}

impl Default for EnglishRepeatedWordsRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for EnglishRepeatedWordsRule {
    fn id(&self) -> &str {
        "EN_REPEATEDWORDS"
    }

    fn is_picky(&self) -> bool {
        true
    }

    fn description(&self) -> &str {
        "Suggest synonyms for repeated words."
    }

    fn category(&self) -> Category {
        Category::new("REPETITIONS_STYLE", "Repetitions")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
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
            let mut lemmas_in_sentence: Vec<String> = Vec::new();
            for atrs in &tokens {
                let token = atrs.token().token();
                if !token.is_empty() {
                    word_number += 1;
                }
                let is_capitalized = token.chars().next().is_some_and(|c| c.is_uppercase())
                    && token.chars().skip(1).all(|c| !c.is_uppercase())
                    && token.chars().any(|c| c.is_alphabetic());
                let is_all_uppercase =
                    !token.is_empty() && token.chars().all(|c| !c.is_lowercase());
                if sent_start
                    && !token.is_empty()
                    && !token.chars().all(|c| c.is_ascii_punctuation())
                {
                    sent_start = false;
                }
                if token.is_empty() {
                    continue;
                }
                let mut lemmas: Vec<Option<&str>> = Vec::new();
                for reading in atrs.readings() {
                    let Some(lemma) = reading.lemma() else {
                        continue;
                    };
                    lemmas.push(Some(lemma));
                    let Some(data) = self.words.get(lemma) else {
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
                    let postag_ok = match &data.postag {
                        Some(postag) => {
                            let re = regex::Regex::new(&format!("^(?:{postag})$"));
                            reading.pos_tags().iter().any(|t| match re {
                                Ok(ref compiled) => compiled.is_match(t),
                                Err(_) => t == postag,
                            })
                        }
                        None => true,
                    };
                    let chunk_ok = match &data.chunk {
                        Some(chunk) => {
                            atrs.chunks()
                                .iter()
                                .any(|c| c.starts_with("B-") && c[2..].starts_with(chunk.as_str()))
                                || atrs.chunks().iter().any(|c| c == chunk)
                        }
                        None => true,
                    };
                    if !postag_ok || !chunk_ok {
                        continue;
                    }
                    let sub_id = format!("EN_REPEATEDWORDS_{}", lemma.replace(' ', "_"));
                    let mut replacements: Vec<String> = Vec::new();
                    for synonym in &data.synonyms {
                        let mut r = synonym.clone();
                        if is_all_uppercase {
                            r = r.to_uppercase();
                        } else if is_capitalized {
                            let mut chars = r.chars();
                            if let Some(first) = chars.next() {
                                r = first.to_uppercase().collect::<String>() + chars.as_str();
                            }
                        }
                        if !replacements.contains(&r) {
                            replacements.push(r);
                        }
                    }
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_picky(true)
                        .with_category(self.category())
                        .with_issue_type("style")
                        .with_sub_id(sub_id)
                        .with_urls(vec![
                            "https://languagetool.org/insights/post/i-agree-synonyms/".to_string(),
                        ]);
                    matches.push(
                        RuleMatch::new(
                            "Possible typo: you repeated a word. Did you mean:",
                            atrs.token().start(),
                            atrs.token().end() - atrs.token().start(),
                            rm_rule,
                            og_core::RuleMatchContext::new(
                                token.to_string(),
                                atrs.token().start(),
                                atrs.token().end() - atrs.token().start(),
                            ),
                        )
                        .with_replacements(
                            replacements
                                .into_iter()
                                .map(og_core::SuggestedReplacement::new)
                                .collect(),
                        ),
                    );
                    break;
                }
                for lemma in lemmas.iter().flatten() {
                    if self.words.contains_key(*lemma) {
                        words_last_seen.insert(lemma.to_string(), word_number);
                        if !lemmas_in_sentence.iter().any(|l| l == lemma) {
                            lemmas_in_sentence.push(lemma.to_string());
                        }
                    }
                }
            }
        }
        let _ = text;
        matches
    }
}

mod og_langs_support {
    pub fn english_synonyms_data() -> Option<String> {
        let candidates = [
            "languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/synonyms.txt",
            "../languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/synonyms.txt",
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/synonyms.txt",
        ];
        candidates
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence_with_lemmas(text: &str, lemmas: &[(&str, &str)]) -> AnalyzedSentence {
        use og_core::{AnalyzedToken, AnalyzedTokenReadings};
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for (surface, lemma) in lemmas {
            let start = text[offset..]
                .find(surface)
                .map(|i| offset + i)
                .unwrap_or(offset);
            let end = start + surface.len();
            let reading = AnalyzedToken::new(*surface, start, end).with_lemma(lemma.to_string());
            tokens.push(AnalyzedTokenReadings::new(reading.clone()).with_readings(vec![reading]));
            offset = end;
        }
        tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
            ".",
            text.len(),
            text.len(),
        )));
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn same_sentence_repetition_not_flagged() {
        let rule = EnglishRepeatedWordsRule::new();
        let sentence = sentence_with_lemmas(
            "This is definitely the best definitely approach we have.",
            &[
                ("This", "this"),
                ("is", "be"),
                ("definitely", "definitely"),
                ("the", "the"),
                ("best", "good"),
                ("definitely", "definitely"),
                ("approach", "approach"),
            ],
        );
        let matches = rule.match_text("irrelevant", &[sentence]);
        assert!(matches.is_empty(), "{matches:?}");
    }

    #[test]
    fn suggests_synonym_for_cross_sentence_repetition() {
        let rule = EnglishRepeatedWordsRule::new();
        let s1 = sentence_with_lemmas(
            "It is definitely good.",
            &[
                ("It", "it"),
                ("is", "be"),
                ("definitely", "definitely"),
                ("good", "good"),
            ],
        );
        let s2 = sentence_with_lemmas(
            "This is definitely the way.",
            &[
                ("This", "this"),
                ("is", "be"),
                ("definitely", "definitely"),
                ("the", "the"),
                ("way", "way"),
            ],
        );
        let matches = rule.match_text("", &[s1, s2]);
        assert!(!matches.is_empty(), "expected a repeated-words match");
        let replacements: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(replacements.contains(&"certainly"), "{replacements:?}");
    }
}
