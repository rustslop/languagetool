use super::helpers::{is_all_uppercase, is_capitalized_word, uppercase_first_char};
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};
use std::collections::HashMap;

const MAX_TOKENS_IN_MULTIWORD: usize = 20;

struct Entry {
    suggestions: Vec<String>,
    message: Option<String>,
}

pub struct SimpleReplaceRule {
    start_space: HashMap<String, usize>,
    full_space: HashMap<String, Entry>,
    start_no_space: HashMap<char, bool>,
    full_no_space: HashMap<String, Entry>,
}

fn is_punctuation_start(word: &str) -> bool {
    word.chars().any(|c| c.is_ascii_digit())
        || word.chars().all(|c| c.is_ascii_punctuation())
        || word.chars().all(|c| !c.is_alphabetic())
}

struct MatchSpan<'a> {
    original: &'a str,
    start: usize,
    end: usize,
}

impl SimpleReplaceRule {
    pub fn new() -> Self {
        let mut rule = Self {
            start_space: HashMap::new(),
            full_space: HashMap::new(),
            start_no_space: HashMap::new(),
            full_no_space: HashMap::new(),
        };
        rule.load_data(include_str!(
            "../../../../languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/rules/de/replace.txt"
        ));
        rule.load_data(include_str!(
            "../../../../languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/rules/de/replace_custom.txt"
        ));
        rule
    }

    fn load_data(&mut self, data: &str) {
        for line in data.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let Some((wrong_side, right_side)) = line.split_once('=') else {
                continue;
            };
            let (suggestions_str, message) = match right_side.find('\t') {
                Some(tab) => (&right_side[..tab], Some(right_side[tab + 1..].to_string())),
                None => (right_side, None),
            };
            let suggestions: Vec<String> = suggestions_str.split('|').map(str::to_string).collect();
            if suggestions.is_empty() {
                continue;
            }
            for wrong in wrong_side.split('|') {
                let wrong = wrong.trim();
                if wrong.is_empty() {
                    continue;
                }
                let entry = Entry {
                    suggestions: suggestions.clone(),
                    message: message.clone(),
                };
                if wrong.contains(' ') {
                    let key = wrong.to_lowercase();
                    let words = key.split(' ').count();
                    self.start_space
                        .entry(key.split(' ').next().unwrap_or_default().to_string())
                        .and_modify(|max| *max = (*max).max(words))
                        .or_insert(words);
                    self.full_space.insert(key, entry);
                } else {
                    let key = wrong.to_lowercase();
                    self.start_no_space
                        .insert(key.chars().next().unwrap_or(' '), true);
                    self.full_no_space.insert(key, entry);
                }
            }
        }
    }

    fn create_match(
        &self,
        matches: &mut Vec<RuleMatch>,
        entry: &Entry,
        span: MatchSpan<'_>,
        sentence: &AnalyzedSentence,
        at_sentence_start: bool,
    ) {
        let MatchSpan {
            original,
            start,
            end,
        } = span;
        if let Some(last) = matches.last()
            && last.offset() <= start
            && last.offset() + last.length() >= end
        {
            return;
        }
        let first_word = original.split(' ').next().unwrap_or(original);
        let is_all_upper = is_all_uppercase(original);
        let is_capitalized = is_capitalized_word(first_word);
        let mut final_replacements: Vec<String> = Vec::new();
        for repl in &entry.suggestions {
            let mut final_repl = repl.clone();
            if at_sentence_start || is_capitalized {
                final_repl = uppercase_first_char(repl);
            }
            if is_all_upper {
                final_repl = repl.to_uppercase();
            }
            let is_original = final_repl == original;
            if repl != original && !is_original && !final_replacements.contains(&final_repl) {
                final_replacements.push(final_repl.clone());
            }
            if is_original {
                final_replacements.clear();
                break;
            }
        }
        if final_replacements.is_empty() {
            return;
        }
        let message = match &entry.message {
            Some(msg) if msg.starts_with("http://") || msg.starts_with("https://") => {
                build_message(&entry.suggestions)
            }
            Some(msg) => msg.clone(),
            None => build_message(&entry.suggestions),
        };
        let sub_id = to_rule_id(&format!("DE_SIMPLE_REPLACE_{original}"));
        let desc = format!("Prüft auf bestimmte falsche Wörter/Phrasen: {original}");
        let rm_rule = RuleMatchRule::new(sub_id, desc)
            .with_category(self.category())
            .with_issue_type(self.issue_type().as_str());
        let context = sentence.text().to_string();
        let rule_match = RuleMatch::new(
            message,
            start,
            end - start,
            rm_rule,
            RuleMatchContext::new(context.clone(), start, end - start),
        )
        .with_short_message("Falsches Wort")
        .with_replacements(
            final_replacements
                .into_iter()
                .map(SuggestedReplacement::new)
                .collect(),
        )
        .with_sentence(context);
        if let Some(last) = matches.last()
            && last.offset() >= start
            && last.offset() + last.length() <= end
        {
            matches.pop();
        }
        matches.push(rule_match);
    }
}

fn build_message(suggestions: &[String]) -> String {
    let mut msg_suggestions = String::new();
    for (k, sugg) in suggestions.iter().enumerate() {
        if k > 0 {
            msg_suggestions.push_str(", ");
        }
        msg_suggestions.push_str(&format!("<suggestion>{sugg}</suggestion>"));
    }
    format!("Meinten Sie vielleicht {msg_suggestions}?")
}

fn to_rule_id(input: &str) -> String {
    let mut normalized: String = input
        .trim()
        .to_uppercase()
        .replace(' ', "_")
        .replace('\'', "_Q_")
        .replace('Ä', "AE")
        .replace('Ü', "UE")
        .replace('Ö', "OE");
    normalized = normalized
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    normalized
}

impl Default for SimpleReplaceRule {
    fn default() -> Self {
        Self::new()
    }
}

fn is_whitespace_before(tokens: &[&og_core::AnalyzedTokenReadings], i: usize) -> bool {
    i > 0 && tokens[i].token().start() > tokens[i - 1].token().end()
}

impl Rule for SimpleReplaceRule {
    fn id(&self) -> &str {
        "DE_SIMPLE_REPLACE"
    }

    fn description(&self) -> &str {
        "Prüft auf bestimmte falsche Wörter/Phrasen"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Verschiedenes")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("typographical".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        let mut sent_start = 0;
        while sent_start < tokens.len() && is_punctuation_start(tokens[sent_start].token().token())
        {
            sent_start += 1;
        }
        for start_index in sent_start..tokens.len() {
            if tokens[start_index].is_ignored_by_speller() || tokens[start_index].is_immunized() {
                continue;
            }
            let mut tok = tokens[start_index].token().token().to_string();
            let mut k = start_index + 1;
            while k < tokens.len() && !is_whitespace_before(&tokens, k) {
                tok.push_str(tokens[k].token().token());
                k += 1;
            }
            let tok_lower = tok.to_lowercase();
            if let Some(&max_token_len) = self.start_space.get(&tok_lower) {
                let mut key = String::new();
                let mut end_index = start_index;
                while end_index < tokens.len() && end_index - start_index < MAX_TOKENS_IN_MULTIWORD
                {
                    if end_index > start_index && is_whitespace_before(&tokens, end_index) {
                        key.push(' ');
                    }
                    key.push_str(tokens[end_index].token().token());
                    let number_of_spaces = key.matches(' ').count();
                    if number_of_spaces + 1 > max_token_len {
                        break;
                    }
                    if number_of_spaces > 0 {
                        let original = key.clone();
                        let key_lower = original.to_lowercase();
                        if let Some(entry) = self.full_space.get(&key_lower) {
                            let from = tokens[start_index].token().start();
                            let to = tokens[end_index].token().end();
                            self.create_match(
                                &mut matches,
                                entry,
                                MatchSpan {
                                    original: &original,
                                    start: from,
                                    end: to,
                                },
                                sentence,
                                sent_start == start_index,
                            );
                        }
                    }
                    end_index += 1;
                }
            }
            let first_char = tok_lower.chars().next().unwrap_or(' ');
            if self.start_no_space.contains_key(&first_char) {
                let mut key = String::new();
                let mut end_index = start_index;
                while end_index < tokens.len() && end_index - start_index < MAX_TOKENS_IN_MULTIWORD
                {
                    if end_index > start_index && is_whitespace_before(&tokens, end_index) {
                        break;
                    }
                    key.push_str(tokens[end_index].token().token());
                    let key_lower = key.to_lowercase();
                    if let Some(entry) = self.full_no_space.get(&key_lower) {
                        let from = tokens[start_index].token().start();
                        let to = tokens[end_index].token().end();
                        self.create_match(
                            &mut matches,
                            entry,
                            MatchSpan {
                                original: &key,
                                start: from,
                                end: to,
                            },
                            sentence,
                            sent_start == start_index,
                        );
                    }
                    end_index += 1;
                }
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::sentence;

    #[test]
    fn loads_entries() {
        let rule = SimpleReplaceRule::new();
        assert!(!rule.full_space.is_empty());
        assert!(rule.full_space.contains_key("conversation rate"));
    }

    fn matches_for(text: &str) -> Vec<RuleMatch> {
        SimpleReplaceRule::new().match_sentence(&sentence(text))
    }

    #[test]
    fn multiword_phrase_from_replace_txt_is_flagged_mid_sentence() {
        let matches = matches_for("Ein Oscar Wild Beispiel.");
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.rule().id(), "DE_SIMPLE_REPLACE_OSCAR_WILD");
        assert_eq!(m.offset(), 4);
        assert_eq!(m.length(), "Oscar Wild".len());
        assert_eq!(
            m.message(),
            "Meinten Sie vielleicht <suggestion>Oscar Wilde</suggestion>?"
        );
        assert_eq!(m.replacements()[0].value(), "Oscar Wilde");
    }

    #[test]
    fn all_uppercase_input_uppercases_the_suggestion() {
        let matches = matches_for("Im OSCAR WILD Kontext.");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "OSCAR WILDE");
    }

    #[test]
    fn tab_message_entries_carry_their_custom_message() {
        let matches = matches_for("Der Inglorious Bastards Film.");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Der Titel dieses Films von 2009 ist absichtlich falsch geschrieben."
        );
        assert_eq!(matches[0].replacements()[0].value(), "Inglourious Basterds");
    }

    #[test]
    fn single_word_and_hyphen_glued_entries_match() {
        let artwort = matches_for("Die Artwort stimmt.");
        assert_eq!(artwort.len(), 1);
        assert_eq!(artwort[0].replacements()[0].value(), "Antwort");
        assert_eq!(artwort[0].rule().id(), "DE_SIMPLE_REPLACE_ARTWORT");

        let hyphenated = matches_for("Nach Île-de-France fahren.");
        assert!(hyphenated.is_empty(), "{hyphenated:?}");
        let wrong = matches_for("Nach Ile-de-France fahren.");
        assert_eq!(wrong.len(), 1);
        assert_eq!(wrong[0].replacements()[0].value(), "Île-de-France");
    }

    #[test]
    fn correct_words_and_immunized_tokens_stay_silent() {
        assert!(matches_for("Die Antwort stimmt.").is_empty());
        assert!(matches_for("Ein kurzer Satz.").is_empty());

        let mut analyzed = sentence("Die Artwort stimmt.");
        for token in analyzed.tokens_mut() {
            if token.token().token() == "Artwort" {
                token.immunize();
            }
        }
        assert!(
            SimpleReplaceRule::new()
                .match_sentence(&analyzed)
                .is_empty()
        );
    }

    #[test]
    fn rule_id_normalization_follows_the_java_transliteration() {
        assert_eq!(to_rule_id("äÖß"), "AEOESS");
        assert_eq!(to_rule_id("d'art"), "D_Q_ART");
        assert_eq!(to_rule_id("Thomas Müller"), "THOMAS_MUELLER");
    }
}
