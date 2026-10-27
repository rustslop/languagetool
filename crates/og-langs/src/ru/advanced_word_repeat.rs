use std::collections::BTreeSet;

use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule, rule::Rule,
};
use regex::Regex;
use std::sync::LazyLock;

const EXC_WORDS: &[&str] = &[
    "не",
    "ни",
    "а",
    "их",
    "на",
    "в",
    "по",
    "минута",
    "друг",
    "час",
    "секунда",
    "ПАО",
    "ООО",
    "табл",
    "рис",
];

static EXC_POS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("^(?:INTERJECTION|PRDC|PREP|CONJ|PARTICLE|ABR|NumC:.*|Num:.*)$").expect("regex")
});

static EXC_NONWORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        "^(?:&quot|&gt|&lt|&amp|[0-9].*|M*(?:D?C{0,3}|C[DM])(?:L?X{0,3}|X[LC])(?:V?I{0,3}|I[VX]))$",
    )
    .expect("regex")
});

pub struct RussianWordRepeatRule;

impl RussianWordRepeatRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RussianWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for RussianWordRepeatRule {
    fn id(&self) -> &str {
        "RU_WORD_REPEAT"
    }

    fn description(&self) -> &str {
        "Повтор слов в предложении"
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
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        let mut inflected_words: BTreeSet<String> = BTreeSet::new();
        let mut cur_token = 0usize;

        for (i, &token) in tokens.iter().enumerate().skip(1) {
            let token_text = token.token().token();
            let mut is_word = token_text.chars().count() >= 2;
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
                if EXC_WORDS.contains(&lemma) {
                    is_word = false;
                    break;
                }
                if EXC_POS.is_match(pos_tag) {
                    is_word = false;
                    break;
                }
            }

            if is_word && EXC_NONWORDS.is_match(token_text) {
                is_word = false;
            }

            if is_word {
                let mut repetition = false;
                let mut prev_lemma = String::new();
                let mut not_sent_end = false;
                for reading in token.readings() {
                    let pos = reading.pos_tags().first().map(String::as_str);
                    if let Some(pos) = pos {
                        not_sent_end |= pos == "SENT_END";
                    }
                    if has_lemma {
                        let cur_lemma = reading.lemma().unwrap_or("").to_string();
                        if prev_lemma != cur_lemma && !not_sent_end {
                            if inflected_words.contains(&cur_lemma) && cur_token != i {
                                repetition = true;
                            } else {
                                inflected_words.insert(cur_lemma.clone());
                                cur_token = i;
                            }
                        }
                        prev_lemma = cur_lemma;
                    } else if !not_sent_end {
                        if inflected_words.contains(token_text) {
                            repetition = true;
                        } else {
                            inflected_words.insert(token_text.to_string());
                        }
                    }
                }

                if repetition {
                    let start = token.token().start();
                    let end = token.token().end();
                    let ctx_start = start.saturating_sub(40);
                    let ctx_end = std::cmp::min(end + 40, sentence.text().len());
                    let context_text = sentence.text()[ctx_start..ctx_end].to_string();
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str());
                    rule_matches.push(
                        RuleMatch::new(
                            "Повтор слов в предложении",
                            start,
                            end - start,
                            rm_rule,
                            RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                        )
                        .with_short_message("Повтор слов в предложении")
                        .with_sentence(sentence.text().to_string()),
                    );
                }
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings};

    fn sentence(pairs: &[(&str, &str, &str)]) -> AnalyzedSentence {
        let text = pairs.iter().map(|p| p.0).collect::<Vec<_>>().join(" ");
        let mut sentence = AnalyzedSentence::new(&text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for (word, lemma, pos) in pairs {
            let start = offset;
            let end = start + word.len();
            offset = end + 1;
            tokens.push(AnalyzedTokenReadings::new(
                AnalyzedToken::new(*word, start, end)
                    .with_lemma(lemma.to_string())
                    .with_pos_tags(vec![pos.to_string()]),
            ));
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn flags_repeated_lemma_within_sentence() {
        let s = sentence(&[
            ("Дом", "дом", "NN:m:n"),
            ("стоит", "стоять", "VB"),
            ("у", "у", "PREP"),
            ("дома", "дом", "NN"),
        ]);
        let matches = RussianWordRepeatRule::new().match_sentence(&s);
        assert_eq!(matches.len(), 1, "repeated lemma 'дом' must be flagged");
    }

    #[test]
    fn ignores_excluded_pos_and_short_tokens() {
        let s = sentence(&[
            ("Он", "он", "PNN"),
            ("пришёл", "прийти", "VB"),
            ("и", "и", "CONJ"),
            ("ушёл", "уйти", "VB"),
        ]);
        let matches = RussianWordRepeatRule::new().match_sentence(&s);
        assert!(matches.is_empty(), "no repetition here");
    }
}
