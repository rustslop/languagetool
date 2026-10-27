use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule, rule::Rule,
};
use std::collections::HashSet;
use std::sync::OnceLock;

const EXCLUDED_WORDS: [&str; 31] = [
    "nie", "tuż", "aż", "to", "siebie", "być", "ani", "ni", "albo", "lub", "czy", "bądź", "jako",
    "zł", "np", "coraz", "bardzo", "bardziej", "proc", "ten", "jak", "mln", "tys", "swój", "mój",
    "twój", "nasz", "wasz", "i", "zbyt", "się",
];

pub struct PolishWordRepeatRule {
    excluded_words: HashSet<&'static str>,
    excluded_pos: regex::Regex,
    excluded_non_words: regex::Regex,
}

fn excluded_pos_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"\A(?:prep:.*|ppron.*)\z").expect("valid anchored pos regex")
    })
}

fn excluded_non_words_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"\A(?:&quot|&gt|&lt|&amp|[0-9].*|M*(?:D?C{0,3}|C[DM])(?:L?X{0,3}|X[LC])(?:V?I{0,3}|I[VX]))\z",
        )
        .expect("valid non-word regex")
    })
}

impl PolishWordRepeatRule {
    pub fn new() -> Self {
        Self {
            excluded_words: EXCLUDED_WORDS.into_iter().collect(),
            excluded_pos: excluded_pos_re().clone(),
            excluded_non_words: excluded_non_words_re().clone(),
        }
    }
}

impl Default for PolishWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for PolishWordRepeatRule {
    fn id(&self) -> &str {
        "PL_WORD_REPEAT"
    }

    fn description(&self) -> &str {
        "Powtórzenia wyrazów w zdaniu (monotonia stylistyczna)"
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let text = sentence.text();
        let tokens = sentence.non_whitespace_tokens();
        let mut inflected_words: HashSet<String> = HashSet::new();
        let mut rule_matches = Vec::new();
        let mut cur_token = 0usize;
        for (i, token) in tokens.iter().enumerate().skip(1) {
            let token_str = token.token().token();
            if token_str.chars().count() < 2 {
                continue;
            }
            let mut is_word = true;
            let mut has_lemma = true;
            for reading in token.readings() {
                let Some(pos_tag) = reading.pos_tags().first() else {
                    has_lemma = false;
                    continue;
                };
                if pos_tag.is_empty() {
                    is_word = false;
                    break;
                }
                let Some(lemma) = reading.lemma() else {
                    has_lemma = false;
                    break;
                };
                if self.excluded_words.contains(lemma) {
                    is_word = false;
                    break;
                }
                if self.excluded_pos.is_match(pos_tag) {
                    is_word = false;
                    break;
                }
            }
            if is_word && self.excluded_non_words.is_match(token_str) {
                is_word = false;
            }
            if !is_word {
                continue;
            }
            let mut repetition = false;
            let mut prev_lemma = String::new();
            let mut not_sent_end = false;
            for reading in token.readings() {
                if reading.has_pos_tag("SENT_END") {
                    not_sent_end = true;
                }
                if has_lemma {
                    let cur_lemma = reading.lemma().unwrap_or_default().to_string();
                    if prev_lemma != cur_lemma && !not_sent_end {
                        if inflected_words.contains(&cur_lemma) && cur_token != i {
                            repetition = true;
                        } else {
                            inflected_words.insert(cur_lemma.clone());
                            cur_token = i;
                        }
                    }
                    prev_lemma = cur_lemma;
                } else if inflected_words.contains(token_str) && !not_sent_end {
                    repetition = true;
                } else {
                    inflected_words.insert(token_str.to_string());
                }
            }
            if repetition {
                let pos = token.token().start();
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                let rel_start = pos.saturating_sub(sentence.start());
                let rel_end = rel_start + token_str.len();
                let ctx_start = rel_start.saturating_sub(40);
                let ctx_end = (rel_end + 40).min(text.len());
                rule_matches.push(
                    RuleMatch::new(
                        "Powtórzony wyraz w zdaniu",
                        pos,
                        token_str.len(),
                        rm_rule,
                        RuleMatchContext::new(
                            text[ctx_start..ctx_end].to_string(),
                            ctx_start,
                            ctx_end - ctx_start,
                        ),
                    )
                    .with_sentence(text.to_string())
                    .with_short_message("Powtórzenie wyrazu"),
                );
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches_for(text: &str) -> Vec<RuleMatch> {
        let sentence =
            crate::pl::test_support::PolishPipeline::shared().analyze_range(text, 0, text.len());
        PolishWordRepeatRule::new().match_sentence(&sentence)
    }

    #[test]
    fn java_pins_example_pair_flags_the_repeated_word() {
        let matches = matches_for("Mówiła długo, bo lubiła robić wszystko długo.");
        assert!(
            matches
                .iter()
                .any(|m| m.length() == "długo".len() && m.offset() > 20),
            "{matches:?}"
        );
        assert!(matches_for("Mówiła długo, bo lubiła robić wszystko powoli.").is_empty());
    }

    #[test]
    fn java_pins_excluded_function_words_do_not_fire() {
        assert!(
            matches_for("To jest to, co chciałem powiedzieć.").is_empty(),
            "to is in the excluded word list"
        );
        assert!(
            matches_for("Nie wiem, czy nie powinienem iść.").is_empty(),
            "nie is in the excluded word list"
        );
    }
}
