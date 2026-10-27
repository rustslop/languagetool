use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    rule::TextLevelRule,
};
use regex::Regex;
use std::sync::LazyLock;

const FILLER_WORDS: &[&str] = &[
    "ах",
    "аа",
    "ааа",
    "аааа",
    "ау",
    "бу",
    "вау",
    "ох",
    "однако",
    "эээ",
    "э",
    "эй",
    "эх",
    "ух-ты",
    "ух",
];

const MIN_PERCENT: i64 = 8;

static OPENING_QUOTES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[\"\u{201C}\u{201E}\u{00BB}\u{00AB}]").expect("valid regex"));
static ENDING_QUOTES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[\"\u{201C}\u{201D}\u{00BB}\u{00AB}]").expect("valid regex"));
static NON_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[.?!…:;,~’'\"„“”»«‚‘›‹()\[\]\-–—*×∗·+÷/=]"#).expect("valid regex")
});

pub struct RussianFillerWordsRule;

impl RussianFillerWordsRule {
    pub fn new() -> Self {
        Self
    }

    fn is_filler_word(token: &str) -> bool {
        FILLER_WORDS.contains(&token)
    }
}

impl Default for RussianFillerWordsRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for RussianFillerWordsRule {
    fn id(&self) -> &str {
        "FILLER_WORDS_RU"
    }

    fn description(&self) -> &str {
        "Filler words"
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("STYLE", "Style")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let mut pending: Vec<(usize, usize, &AnalyzedSentence)> = Vec::new();
        let mut word_count = 0usize;
        let mut is_direct_speech = false;

        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            for n in 1..tokens.len() {
                let token = tokens[n];
                let s_token = token.token().token();
                let opens =
                    !is_direct_speech && OPENING_QUOTES.is_match(s_token) && n < tokens.len() - 1;
                if opens {
                    is_direct_speech = true;
                } else if is_direct_speech && ENDING_QUOTES.is_match(s_token) {
                    is_direct_speech = false;
                } else if !token.is_whitespace() && !NON_WORD.is_match(s_token) {
                    word_count += 1;
                    if Self::is_filler_word(s_token) {
                        pending.push((token.token().start(), token.token().end(), sentence));
                    }
                }
            }
        }

        let num_matches = pending.len();
        if word_count > 0 {
            let percent = (num_matches * 100 / word_count.max(1)) as i64;
            if percent > MIN_PERCENT {
                for (start, end, sentence) in pending {
                    let ctx_start = start.saturating_sub(40);
                    let ctx_end = std::cmp::min(end + 40, sentence.text().len());
                    let context_text = sentence.text()[ctx_start..ctx_end].to_string();
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str());
                    rule_matches.push(
                        RuleMatch::new(
                            "Это — слово-паразит. Удалите его, если это возможно.",
                            start,
                            end - start,
                            rm_rule,
                            RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                        )
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

    fn sentence(words: &[&str]) -> AnalyzedSentence {
        let text = words.join(" ");
        let mut s = AnalyzedSentence::new(&text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for word in words {
            let start = offset;
            let end = start + word.len();
            offset = end + 1;
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                *word, start, end,
            )));
        }
        s.set_tokens(tokens);
        s
    }

    #[test]
    fn flags_fillers_above_percent_limit() {
        let words: Vec<&str> = vec!["Однако", "ох", "ах", "вау", "ух"];
        let s = sentence(&words);
        let text = words.join(" ");
        let matches = RussianFillerWordsRule::new().match_text(&text, &[s]);
        assert!(
            !matches.is_empty(),
            "4 of 5 words are fillers, must exceed 8 percent"
        );
    }

    #[test]
    fn stays_quiet_below_percent_limit() {
        let words: Vec<&str> = vec![
            "Однако",
            "мы",
            "решили",
            "поехать",
            "на",
            "море",
            "летом",
            "всей",
            "семьёй",
        ];
        let s = sentence(&words);
        let text = words.join(" ");
        let matches = RussianFillerWordsRule::new().match_text(&text, &[s]);
        assert!(matches.is_empty(), "1 filler of 9 words must stay quiet");
    }
}
