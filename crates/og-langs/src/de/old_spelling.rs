use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};
use std::collections::HashMap;
use std::sync::LazyLock;

const EXCEPTIONS: [&str; 19] = [
    "Schloß Holte",
    "Schloß Neuhaus",
    "Schloß Ricklingen",
    "Schloß-Nauses",
    "Schloß Rötteln",
    "Klinikum Schloß Winnenden",
    "Grazer Schloßberg",
    "Höchster Schloß",
    "Bell Telephone",
    "Telephone Company",
    "American Telephone",
    "England Telephone",
    "Mobile Telephone",
    "Cordless Telephone",
    "Telephone Line",
    "World Telephone",
    "Tip Top",
    "Hans Joachim Blaß",
    "kurz fassen",
];

type SpellingMaps = (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>);

static DATA: LazyLock<SpellingMaps> = LazyLock::new(|| {
    load(include_str!(
        "../../../../languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de/alt_neu.csv"
    ))
});

fn split_suggestions(new_spelling: &str) -> Vec<String> {
    new_spelling.split('|').map(str::to_string).collect()
}

fn load(data: &str) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) {
    let mut words = HashMap::new();
    let mut sent_start_words = HashMap::new();
    for line in data.lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut parts = line.split(';');
        let (Some(old), Some(new)) = (parts.next(), parts.next()) else {
            continue;
        };
        if old.is_empty() || new.is_empty() || old == new {
            continue;
        }
        words.insert(old.to_string(), split_suggestions(new));
        if old.chars().next().is_some_and(|c| c.is_lowercase())
            && new.chars().next().is_some_and(|c| c.is_lowercase())
        {
            let upper_old = super::helpers::uppercase_first_char(old);
            let upper_new = super::helpers::uppercase_first_char(new);
            sent_start_words.insert(upper_old, split_suggestions(&upper_new));
        } else {
            sent_start_words.insert(old.to_string(), split_suggestions(new));
        }
    }
    (words, sent_start_words)
}

fn is_boundary(c: char) -> bool {
    !(c.is_ascii_alphabetic() || "öäüß".contains(c))
}

fn region_matches_ci(text: &str, at: usize, pattern: &str) -> bool {
    let mut text_chars = text[at..].chars();
    let mut pattern_chars = pattern.chars();
    loop {
        match (text_chars.next(), pattern_chars.next()) {
            (None, None) => return true,
            (Some(t), Some(p)) => {
                if t != p && t.to_lowercase().ne(p.to_lowercase()) {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

pub struct OldSpellingRule {
    country: String,
}

impl OldSpellingRule {
    pub fn new(country: &str) -> Self {
        Self {
            country: country.to_string(),
        }
    }

    fn ignore_match(&self, begin: usize, end: usize, text: &str) -> bool {
        for exception in EXCEPTIONS {
            if region_matches_ci(text, begin, exception)
                || (end >= exception.len()
                    && region_matches_ci(text, end - exception.len(), exception))
            {
                return true;
            }
        }
        let bytes = text.as_bytes();
        if begin > 0 {
            let prev = text[..begin].chars().next_back();
            if prev.is_some_and(|c| !is_boundary(c)) {
                return true;
            }
        }
        if end < text.len() {
            let next = text[end..].chars().next();
            if next.is_some_and(|c| !is_boundary(c)) {
                return true;
            }
        }
        if begin >= 6 && &bytes[begin - 6..begin] == b"Prof." {
            return true;
        }
        if begin >= 5
            && let Some(before5) = text.get(begin - 5..begin)
            && matches!(&before5[1..], "Herr" | "Frau")
        {
            return true;
        }
        if begin >= 4
            && let Some(before4) = text.get(begin - 4..begin)
            && matches!(&before4[1..], "Hr." | "Fr." | "Dr.")
        {
            return true;
        }
        false
    }

    fn add_match(
        &self,
        sentence: &AnalyzedSentence,
        begin: usize,
        end: usize,
        suggestions: &[String],
        matches: &mut Vec<RuleMatch>,
    ) {
        let base_message = "Diese Schreibweise war nur in der alten Rechtschreibung korrekt.";
        let covered = sentence.text().get(begin..end).unwrap_or_default();
        let mut message = base_message.to_string();
        if let Some(first) = suggestions.first()
            && first.replacen("ss", "ß", 1) == covered
        {
            if self.country == "AT" && covered.to_lowercase().contains("geschoß") {
                return;
            }
            message = format!(
                "{base_message} Das Wort wird mit 'ss' geschrieben, wenn davor eine kurz gesprochene Silbe steht."
            );
        }
        let rm_rule = RuleMatchRule::new(self.id(), self.description())
            .with_category(self.category())
            .with_issue_type(self.issue_type().as_str());
        matches.push(
            RuleMatch::new(
                message,
                begin,
                end - begin,
                rm_rule,
                RuleMatchContext::new(covered.to_string(), begin, end - begin),
            )
            .with_short_message("Alte Rechtschreibung")
            .with_replacements(
                suggestions
                    .iter()
                    .map(|s| SuggestedReplacement::new(s.clone()))
                    .collect(),
            )
            .with_sentence(sentence.text().to_string()),
        );
    }
}

impl Rule for OldSpellingRule {
    fn id(&self) -> &str {
        "OLD_SPELLING_RULE"
    }

    fn description(&self) -> &str {
        "Findet Schreibweisen, die nur in der alten Rechtschreibung gültig waren"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Mögliche Tippfehler")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let text = sentence.text();
        let (words, sent_start_words) = &*DATA;
        let max_len = words
            .keys()
            .map(|k| k.len())
            .max()
            .unwrap_or(0)
            .max(sent_start_words.keys().map(|k| k.len()).max().unwrap_or(0));
        let mut matches = Vec::new();
        let mut start_positions: Vec<usize> = Vec::new();
        let mut begin = 0;
        while begin <= text.len() {
            if !text.is_char_boundary(begin) {
                begin += 1;
                continue;
            }
            let mut matched_len = None;
            let mut matched_suggestions: &[String] = &[];
            let mut len = max_len.min(text.len() - begin);
            while len > 0 {
                if !text.is_char_boundary(begin + len) {
                    len -= 1;
                    continue;
                }
                if let Some(suggestions) = words.get(&text[begin..begin + len])
                    && !self.ignore_match(begin, begin + len, text)
                {
                    matched_len = Some(len);
                    matched_suggestions = suggestions;
                    break;
                }
                len -= 1;
            }
            if let Some(len) = matched_len {
                self.add_match(
                    sentence,
                    begin,
                    begin + len,
                    matched_suggestions,
                    &mut matches,
                );
                start_positions.push(begin);
            }
            begin += 1;
        }
        if !start_positions.contains(&0) && text.is_char_boundary(0) {
            let mut len = max_len.min(text.len());
            while len > 0 {
                if !text.is_char_boundary(len) {
                    len -= 1;
                    continue;
                }
                if let Some(suggestions) = sent_start_words.get(&text[0..len])
                    && !self.ignore_match(0, len, text)
                {
                    self.add_match(sentence, 0, len, suggestions, &mut matches);
                    break;
                }
                len -= 1;
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_old_spelling_abfluss() {
        let rule = OldSpellingRule::new("DE");
        let mut sentence = AnalyzedSentence::new("Der Abfluß ist schon wieder verstopft.", 0, 37);
        sentence.set_tokens(vec![]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].replacements()[0].value(), "Abfluss");
    }

    #[test]
    fn ignores_substring_matches() {
        let rule = OldSpellingRule::new("DE");
        let mut sentence = AnalyzedSentence::new("Die Photographie ist beliebt.", 0, 29);
        sentence.set_tokens(vec![]);
        let matches = rule.match_sentence(&sentence);
        assert!(
            !matches
                .iter()
                .any(|m| &sentence.text()[m.offset()..m.offset() + m.length()] == "Photo"),
            "{matches:?}"
        );
    }

    #[test]
    fn sentence_start_uppercase_laesst() {
        let rule = OldSpellingRule::new("DE");
        let mut sentence = AnalyzedSentence::new("Läßt du das bitte offen?", 0, 24);
        sentence.set_tokens(vec![]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].replacements()[0].value(), "Lässt");
    }
}
