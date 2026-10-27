use super::helpers::{is_pos_tag_unknown, levenshtein_at_most_one, starts_with_uppercase};
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::TextLevelRule,
};
use std::collections::HashSet;

const MIN_LENGTH: usize = 4;

const PRONOUN_EXCEPTIONS: [&str; 12] = [
    "Dein", "Deine", "Deinen", "Deiner", "Deines", "Deinem", "Ihr", "Ihre", "Ihren", "Ihrer",
    "Ihres", "Ihrem",
];

fn has_partial_pos_tag(atr: &og_core::AnalyzedTokenReadings, part: &str) -> bool {
    atr.readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.contains(part)))
}

pub struct SimilarNameRule;

impl SimilarNameRule {
    fn similar_name<'a>(
        &self,
        name_here: &str,
        names_so_far: &'a HashSet<String>,
    ) -> Option<&'a str> {
        let mut best: Option<&'a str> = None;
        for name in names_so_far {
            if name == name_here {
                continue;
            }
            let len_diff = name.len().abs_diff(name_here.len());
            let name_ends_with_s = name.ends_with('s') && !name_here.ends_with('s');
            let other_name_ends_with_s = !name.ends_with('s') && name_here.ends_with('s');
            let name_ends_with_n = name.ends_with('n') && !name_here.ends_with('n');
            let other_name_ends_with_n = !name.ends_with('n') && name_here.ends_with('n');
            if name_ends_with_s
                || other_name_ends_with_s
                || name_ends_with_n
                || other_name_ends_with_n
            {
                continue;
            }
            if len_diff <= 1 && levenshtein_at_most_one(name, name_here) {
                best = Some(name);
            }
        }
        best
    }
}

impl Default for SimilarNameRule {
    fn default() -> Self {
        Self
    }
}

impl TextLevelRule for SimilarNameRule {
    fn id(&self) -> &str {
        "DE_SIMILAR_NAMES"
    }

    fn description(&self) -> &str {
        "Mögliche Tippfehler in Namen finden"
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Mögliche Tippfehler")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut names_so_far: HashSet<String> = HashSet::new();
        let mut rule_matches = Vec::new();
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            for atr in tokens.iter() {
                let word = atr.token().token();
                let is_maybe_name = word.chars().count() >= MIN_LENGTH
                    && ((has_partial_pos_tag(atr, "EIG:") && !has_partial_pos_tag(atr, ":COU"))
                        || is_pos_tag_unknown(atr))
                    && !PRONOUN_EXCEPTIONS.contains(&word);
                if is_maybe_name && starts_with_uppercase(word) {
                    if let Some(similar) = self.similar_name(word, &names_so_far) {
                        let msg = format!(
                            "'{word}' ähnelt dem vorher benutzten '{similar}', handelt es sich evtl. um einen Tippfehler?"
                        );
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_category(self.category())
                            .with_issue_type(self.issue_type().as_str());
                        let from = atr.token().start();
                        let to = atr.token().end();
                        rule_matches.push(
                            RuleMatch::new(
                                msg,
                                from,
                                to - from,
                                rm_rule,
                                RuleMatchContext::new(sentence.text().to_string(), from, to - from),
                            )
                            .with_replacements(vec![SuggestedReplacement::new(similar)])
                            .with_sentence(sentence.text().to_string()),
                        );
                    }
                    names_so_far.insert(word.to_string());
                }
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::token_with_pos;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    #[test]
    fn levenshtein_one_detection() {
        assert!(levenshtein_at_most_one("Miller", "Müller"));
        assert!(!levenshtein_at_most_one("Meier", "Schulz"));
    }

    fn named_sentence(parts: &[(&str, &[&str])]) -> AnalyzedSentence {
        let full: String = parts.iter().map(|(t, _)| *t).collect();
        let mut tokens = Vec::with_capacity(parts.len() + 1);
        tokens.push(AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        ));
        let mut offset = 0usize;
        for (text, tags) in parts {
            let start = offset;
            offset += text.len();
            tokens.push(token_with_pos(text, start, offset, tags));
        }
        let mut sentence = AnalyzedSentence::new(&full, 0, full.len());
        sentence.set_tokens(tokens);
        sentence
    }

    fn eigen(name: &'static str) -> (&'static str, &'static [&'static str]) {
        (name, EIG)
    }

    fn plain(word: &'static str) -> (&'static str, &'static [&'static str]) {
        (word, NO_TAGS)
    }

    fn matches_for(sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        SimilarNameRule.match_text("", sentences)
    }

    const EIG: &[&str] = &["EIG:SUB:NOM:SIN:MAS"];
    const NO_TAGS: &[&str] = &[];
    const EIG_AND_COU: &[&str] = &["EIG:SUB:NOM:SIN:NEU", "EIG:COU:NOM:SIN:NEU"];

    #[test]
    fn java_test_flags_the_similar_name_across_sentences() {
        let text1 = "Hier steht Angela Müller.";
        let text2 = "Im nächsten Satz dann Miller.";
        let first = named_sentence(&[
            plain("Hier"),
            plain(" "),
            plain("steht"),
            plain(" "),
            plain("Angela"),
            plain(" "),
            eigen("Müller"),
            plain("."),
        ]);
        let second = named_sentence(&[
            plain("Im"),
            plain(" "),
            plain("nächsten"),
            plain(" "),
            plain("Satz"),
            plain(" "),
            plain("dann"),
            plain(" "),
            eigen("Miller"),
            plain("."),
        ]);
        let matches = matches_for(&[first, second]);
        assert_eq!(matches.len(), 1, "{text1} {text2}");
        let m = &matches[0];
        assert_eq!(m.rule().id(), "DE_SIMILAR_NAMES");
        assert_eq!(
            m.message(),
            "'Miller' ähnelt dem vorher benutzten 'Müller', handelt es sich evtl. um einen Tippfehler?"
        );
        assert_eq!(m.offset(), text2.find("Miller").unwrap());
        assert_eq!(m.length(), "Miller".len());
        assert_eq!(m.replacements()[0].value(), "Müller");
    }

    #[test]
    fn java_test_genitive_s_and_dative_n_endings_are_skipped() {
        let genitive = named_sentence(&[
            eigen("Müller"),
            plain(" "),
            eigen("Müllers"),
            plain(" "),
            plain("Ehemann"),
            plain("."),
        ]);
        assert!(matches_for(&[genitive]).is_empty());

        let dative = named_sentence(&[
            plain("Ein"),
            plain(" "),
            eigen("Mikrocontroller"),
            plain(","),
            plain(" "),
            plain("bei"),
            plain(" "),
            eigen("Mikrocontrollern"),
        ]);
        assert!(matches_for(&[dative]).is_empty());
    }

    #[test]
    fn java_test_distance_two_is_not_similar_enough() {
        let sentence = named_sentence(&[
            eigen("Müller"),
            plain(" "),
            plain("Dann"),
            plain(" "),
            eigen("Mulla"),
            plain(","),
        ]);
        assert!(matches_for(&[sentence]).is_empty());
    }

    #[test]
    fn java_test_pronoun_exceptions_stay_silent_even_untagged() {
        let sentence = named_sentence(&[
            plain("Und"),
            plain(" "),
            plain("Deiner"),
            plain(" "),
            plain("Freundin"),
            plain("."),
            plain(" "),
            plain("Deinem"),
            plain(" "),
            plain("Hund"),
            plain(" "),
            plain("geht"),
            plain(" "),
            plain("es"),
            plain(" "),
            plain("gut"),
            plain("?"),
        ]);
        assert!(matches_for(&[sentence]).is_empty());
    }

    #[test]
    fn untagged_uppercase_words_count_as_names_but_short_or_lowercase_do_not() {
        let sentence = named_sentence(&[
            plain("Kiel"),
            plain(" "),
            plain("und"),
            plain(" "),
            plain("kiel"),
            plain(" "),
            plain("und"),
            plain(" "),
            plain("Al"),
            plain(" "),
            plain("und"),
            plain(" "),
            plain("Aal"),
        ]);
        assert!(matches_for(&[sentence]).is_empty());

        let untagged = named_sentence(&[
            plain("Petra"),
            plain(" "),
            plain("und"),
            plain(" "),
            plain("Petry"),
        ]);
        let matches = matches_for(&[untagged]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "Petra");
    }

    #[test]
    fn cou_readings_and_repeats_never_flag() {
        let country = named_sentence(&[
            ("Turkmenistan", EIG_AND_COU),
            plain(" "),
            plain("und"),
            plain(" "),
            ("Turkmenistas", EIG_AND_COU),
        ]);
        assert!(matches_for(&[country]).is_empty());

        let repeated = named_sentence(&[
            eigen("Müller"),
            plain(" "),
            plain("und"),
            plain(" "),
            plain("nochmals"),
            plain(" "),
            eigen("Müller"),
        ]);
        assert!(matches_for(&[repeated]).is_empty());
    }
}
