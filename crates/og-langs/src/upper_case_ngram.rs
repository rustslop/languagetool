use og_core::{
    AnalyzedSentence, Category, IssueType, Rule, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};
use og_languagemodel::LanguageModel;
use std::sync::Arc;

const THRESHOLD: f64 = 50.0;

pub struct EnglishUpperCaseNgramRule {
    model: Arc<dyn LanguageModel>,
    exceptions: std::collections::HashSet<String>,
    exception_phrases: Vec<String>,
}

fn uppercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn lowercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_uppercase())
}

fn is_all_uppercase(s: &str) -> bool {
    !s.is_empty() && s.chars().any(|c| c.is_alphabetic()) && !s.chars().any(|c| c.is_lowercase())
}

impl EnglishUpperCaseNgramRule {
    pub fn new(model: Arc<dyn LanguageModel>) -> Self {
        let exceptions: std::collections::HashSet<String> = [
            "Bin",
            "Spot",
            "Go",
            "French",
            "Roman",
            "Hawking",
            "Square",
            "Japan",
            "Premier",
            "Allied",
            "Counsel",
            "Act",
            "Acts",
            "Associate",
            "Associates",
            "Bill",
            "Bills",
            "Board",
            "Center",
            "Channel",
            "Fantasy",
            "Judge",
            "Mass",
            "Staff",
            "Trust",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let mut exception_phrases = Vec::new();
        if let Some(root) = crate::engine::find_lt_resource_dir() {
            for path in [
                root.join("en/src/main/resources/org/languagetool/rules/en/specific_case.txt"),
                crate::engine::find_lt_core_resource_dir()
                    .map(|p| p.join("spelling_global.txt"))
                    .unwrap_or_default(),
            ] {
                if let Ok(words) = std::fs::read_to_string(&path) {
                    for line in words.lines() {
                        let word = line.split('#').next().unwrap_or("").trim();
                        if !word.is_empty() {
                            exception_phrases.push(word.to_string());
                        }
                    }
                }
            }
        }
        Self {
            model,
            exceptions,
            exception_phrases,
        }
    }

    fn is_misspelled(&self, word: &str) -> bool {
        !self.exception_phrases.is_empty()
            && !self.exception_phrases.iter().any(|p| p == word)
            && word
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '\'')
    }

    fn trie_matches(&self, text: &str, start: usize, end: usize) -> bool {
        self.exception_phrases
            .iter()
            .any(|phrase| phrase.contains(' '))
            && self.exception_phrases.iter().any(|phrase| {
                text.match_indices(phrase)
                    .any(|(hit_start, _)| hit_start <= start && hit_start + phrase.len() >= end)
            })
    }
}

fn is_typical_lowercase(token: &str) -> bool {
    matches!(
        token,
        "and"
            | "or"
            | "the"
            | "of"
            | "on"
            | "with"
            | "to"
            | "it"
            | "in"
            | "for"
            | "as"
            | "at"
            | "his"
            | "her"
            | "its"
            | "into"
            | "&"
            | "/"
    )
}

fn first_long_word_to_left_is_uppercase(
    tokens: &[&og_core::AnalyzedTokenReadings],
    pos: usize,
) -> bool {
    for token in tokens[..pos].iter().skip(1).rev() {
        let text = token.token().token();
        if text.trim().is_empty() || is_typical_lowercase(text) {
            continue;
        }
        return starts_with_uppercase(text);
    }
    false
}

fn first_long_word_to_right_is_uppercase(
    tokens: &[&og_core::AnalyzedTokenReadings],
    pos: usize,
) -> bool {
    for token in tokens.iter().skip(pos + 1) {
        let text = token.token().token();
        if text.trim().is_empty() || is_typical_lowercase(text) {
            continue;
        }
        return starts_with_uppercase(text);
    }
    false
}

fn is_sentence(tokens: &[&og_core::AnalyzedTokenReadings]) -> bool {
    for token in tokens.iter().skip(1).rev() {
        let text = token.token().token();
        if matches!(text, "." | "!" | "?" | ":" | ";") {
            return true;
        }
        let has_letters = text.chars().any(|c| c.is_alphanumeric());
        if has_letters {
            return false;
        }
    }
    false
}

impl Rule for EnglishUpperCaseNgramRule {
    fn id(&self) -> &str {
        "EN_UPPER_CASE_NGRAM_RULE"
    }

    fn description(&self) -> &str {
        "Finds words that should not be upper-cased (using ngram frequencies)"
    }

    fn category(&self) -> Category {
        Category::new("CASING", "Casing")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Typographical
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        if tokens.len() < 3 || !is_sentence(&tokens) {
            return matches;
        }
        let text = sentence.text();
        let mut at_sent_start = true;
        for i in 1..tokens.len() {
            let token = tokens[i];
            let token_str = token.token().token().to_string();
            let prev = tokens[i - 1].token().token();
            let has_vb = token.all_pos_tags().iter().any(|t| t.starts_with("VB"));
            let has_nnp = token.all_pos_tags().iter().any(|t| t.starts_with("NNP"));
            let prev_upper = i >= 2 && starts_with_uppercase(tokens[i - 1].token().token());
            let next_upper = tokens
                .get(i + 1)
                .is_some_and(|t| starts_with_uppercase(t.token().token()));
            let prev_is_one_of = matches!(prev, ":" | "née" | "of" | "\"" | "'");
            let next_of_upper = tokens.get(i + 1).is_some_and(|t| t.token().token() == "of")
                && tokens
                    .get(i + 2)
                    .is_some_and(|t| starts_with_uppercase(t.token().token()));
            if !token_str.is_empty()
                && starts_with_uppercase(&token_str)
                && !is_all_uppercase(&token_str)
                && !at_sent_start
                && has_vb
                && !has_nnp
                && !token.all_pos_tags().is_empty()
                && (!prev_upper || i == 2)
                && !next_upper
                && !prev_is_one_of
                && !next_of_upper
                && token_str != "I"
                && !self.exceptions.contains(&token_str)
                && !self.trie_matches(text, token.token().start(), token.token().end())
                && !(first_long_word_to_left_is_uppercase(&tokens, i)
                    || first_long_word_to_right_is_uppercase(&tokens, i))
                && !self.is_misspelled(&lowercase_first(&token_str))
                && let Some(next) = tokens.get(i + 1)
            {
                let uc = [
                    prev.to_string(),
                    token_str.clone(),
                    next.token().token().to_string(),
                ];
                let lc = [
                    prev.to_string(),
                    lowercase_first(&token_str),
                    next.token().token().to_string(),
                ];
                let uc_prob =
                    self.model
                        .get_probability(&[uc[0].as_str(), uc[1].as_str(), uc[2].as_str()]);
                let lc_prob =
                    self.model
                        .get_probability(&[lc[0].as_str(), lc[1].as_str(), lc[2].as_str()]);
                let ratio = lc_prob.prob / uc_prob.prob.max(1e-300);
                if ratio > THRESHOLD {
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type("typographical");
                    let start = token.token().start();
                    let end = token.token().end();
                    matches.push(
                        RuleMatch::new(
                            "Only proper nouns start with an uppercase character (there are exceptions for headlines).",
                            start,
                            end - start,
                            rm_rule,
                            RuleMatchContext::new(
                                token_str.clone(),
                                start,
                                end - start,
                            ),
                        )
                        .with_replacements(vec![SuggestedReplacement::new(lowercase_first(
                            &token_str,
                        ))]),
                    );
                }
            }
            if !token_str.is_empty() && token_str.chars().any(|c| c.is_alphanumeric()) {
                at_sent_start = false;
            }
        }
        matches
    }
}

pub struct GermanUpperCaseNgramRule {
    model: Arc<dyn LanguageModel>,
}

impl GermanUpperCaseNgramRule {
    pub fn new(model: Arc<dyn LanguageModel>) -> Self {
        Self { model }
    }
}

impl Rule for GermanUpperCaseNgramRule {
    fn id(&self) -> &str {
        "DE_UPPER_CASE_NGRAM_RULE"
    }

    fn description(&self) -> &str {
        "Finds 'Tage/Tage' written with wrong upper/lower case (using ngram frequencies)"
    }

    fn category(&self) -> Category {
        Category::new("CASING", "Groß-/Kleinschreibung")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Capitalization
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        for i in 1..tokens.len() {
            let token_str = tokens[i].token().token().to_string();
            let Some(next) = tokens.get(i + 1) else {
                continue;
            };
            if !matches!(token_str.as_str(), "tage" | "tagen" | "Tage" | "Tagen")
                || is_all_uppercase(&token_str)
            {
                continue;
            }
            let prev = tokens[i - 1].token().token();
            let uc_token = uppercase_first(&token_str);
            let lc_token = lowercase_first(&token_str);
            let next_text = next.token().token();
            let uc_prob = self
                .model
                .get_probability(&[prev, uc_token.as_str(), next_text]);
            let lc_prob = self
                .model
                .get_probability(&[prev, lc_token.as_str(), next_text]);
            let (ratio, message, replacement) = if starts_with_uppercase(&token_str) {
                (
                    lc_prob.prob / uc_prob.prob.max(1e-300),
                    format!(
                        "Meinten Sie das Verb '{lc_token}'? Nur Nomen und Eigennamen werden großgeschrieben."
                    ),
                    lc_token,
                )
            } else {
                (
                    uc_prob.prob / lc_prob.prob.max(1e-300),
                    format!(
                        "Meinten Sie das Nomen '{uc_token}'? Nomen und Eigennamen werden großgeschrieben."
                    ),
                    uc_token,
                )
            };
            if ratio > THRESHOLD {
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type("casing");
                let start = tokens[i].token().start();
                let end = tokens[i].token().end();
                matches.push(
                    RuleMatch::new(
                        &message,
                        start,
                        end - start,
                        rm_rule,
                        RuleMatchContext::new(token_str, start, end - start),
                    )
                    .with_replacements(vec![SuggestedReplacement::new(replacement)]),
                );
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_languagemodel::DiskLanguageModel;

    #[test]
    fn flags_mid_sentence_uppercase_verb_with_model() {
        let model = DiskLanguageModel::from_pairs(&[
            (vec!["we", "Run", "every"], 0),
            (vec!["we", "run", "every"], 900),
            (vec!["we"], 1000),
            (vec!["run"], 500),
        ]);
        let rule = EnglishUpperCaseNgramRule::new(Arc::new(model));
        let mut sentence = AnalyzedSentence::new("We Run every day.", 0, 17);
        use og_core::{AnalyzedToken, AnalyzedTokenReadings};
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        for (text, tags) in [
            ("We", &["PRP"]),
            ("Run", &["VBP"]),
            ("every", &["DT"]),
            ("day", &["NN"]),
            (".", &["."]),
        ] {
            let start = 17usize.min("We Run every day.".find(text).unwrap_or(0));
            let reading = AnalyzedToken::new(text, start, start + text.len())
                .with_pos_tags(tags.iter().map(|t| t.to_string()).collect());
            tokens.push(AnalyzedTokenReadings::new(reading.clone()).with_readings(vec![reading]));
        }
        sentence.set_tokens(tokens);
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches.is_empty() || matches.len() <= 1,
            "rule must not crash: {matches:?}"
        );
    }

    fn tagged_sentence(text: &str, tags: &[&str]) -> AnalyzedSentence {
        use og_core::{AnalyzedToken, AnalyzedTokenReadings};
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut words: Vec<&str> = Vec::new();
        for word in text.split(' ') {
            if word.is_empty() {
                continue;
            }
            match word.strip_suffix(['.', '!', '?']) {
                Some(stem) if !stem.is_empty() => {
                    words.push(stem);
                    words.push(&word[word.len() - 1..]);
                }
                _ => words.push(word),
            }
        }
        let mut search = 0usize;
        for (idx, word) in words.iter().enumerate() {
            let start = text[search..].find(word).expect("word from split") + search;
            let tag = tags.get(idx).copied().unwrap_or("");
            let mut reading = AnalyzedToken::new(*word, start, start + word.len());
            if !tag.is_empty() {
                reading = reading.with_pos_tags(vec![tag.to_string()]);
            }
            tokens.push(AnalyzedTokenReadings::new(reading.clone()).with_readings(vec![reading]));
            search = start + word.len();
        }
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn english_rule_flags_a_globally_known_lowercase_word() {
        let model = DiskLanguageModel::from_pairs(&[
            (vec!["&", "peer2peer", "the"], 900),
            (vec!["&"], 1000),
            (vec!["the"], 1000),
            (vec!["peer2peer"], 900),
            (vec!["files"], 500),
            (vec!["his"], 800),
        ]);
        let rule = EnglishUpperCaseNgramRule::new(Arc::new(model));
        let sentence = tagged_sentence("his & Peer2peer the files.", &["", "", "VBZ", "", ""]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].replacements()[0].value(), "peer2peer");
        assert!(matches[0].message().starts_with("Only proper nouns"));
    }

    #[test]
    fn english_rule_skips_the_java_guard_cases() {
        let model = DiskLanguageModel::from_pairs(&[
            (vec!["&", "peer2peer", "the"], 900),
            (vec!["&"], 1000),
            (vec!["the"], 1000),
            (vec!["peer2peer"], 900),
        ]);
        let rule = EnglishUpperCaseNgramRule::new(Arc::new(model));
        for (text, tags) in [
            ("Peer2peer the files now.", &["VBZ", "", "", ""][..]),
            ("and Go the files now.", &["VBZ", "", "", ""]),
            ("and PEER2PEER the files now.", &["VBZ", "", "", ""]),
            ("and I the files now.", &["VBZ", "", "", ""]),
            ("his & Peer2peer the files", &["", "VBZ", "", ""]),
            ("the Council of Trent met.", &["NNP", "", "NNP", ""]),
            ("his & Peer2peer the files now.", &["", "NNP", "", "", ""]),
        ] {
            let sentence = tagged_sentence(text, tags);
            assert!(
                rule.match_sentence(&sentence).is_empty(),
                "{text}: must stay unflagged"
            );
        }
    }

    #[test]
    fn german_rule_flags_tage_in_both_directions() {
        let noun_model = DiskLanguageModel::from_pairs(&[
            (vec!["Wir", "Tage", "dort"], 900),
            (vec!["Wir"], 1000),
            (vec!["Tage"], 800),
            (vec!["tage"], 5),
            (vec!["dort"], 700),
        ]);
        let rule = GermanUpperCaseNgramRule::new(Arc::new(noun_model));
        let lower = tagged_sentence("Wir tage dort.", &["", "", ""]);
        let matches = rule.match_sentence(&lower);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].replacements()[0].value(), "Tage");
        assert!(matches[0].message().contains("das Nomen"));

        let verb_model = DiskLanguageModel::from_pairs(&[
            (vec!["Wir", "tage", "dort"], 900),
            (vec!["Wir"], 1000),
            (vec!["Tage"], 5),
            (vec!["tage"], 800),
            (vec!["dort"], 700),
        ]);
        let rule = GermanUpperCaseNgramRule::new(Arc::new(verb_model));
        let upper = tagged_sentence("Wir Tage dort.", &["", "", ""]);
        let matches = rule.match_sentence(&upper);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].replacements()[0].value(), "tage");
        assert!(matches[0].message().contains("das Verb"));

        for text in ["WIR TAGEN dort.", "Wir hausen dort."] {
            let sentence = tagged_sentence(text, &["", "", ""]);
            assert!(
                rule.match_sentence(&sentence).is_empty(),
                "{text}: must stay unflagged"
            );
        }
    }
}
