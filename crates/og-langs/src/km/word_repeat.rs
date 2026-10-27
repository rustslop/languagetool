use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchKind,
    RuleMatchRule, SuggestedReplacement,
};

pub const KM_WORD_REPEAT_RULE: &str = "KM_WORD_REPEAT_RULE";

const REPETITION: &str = "កំហុសវាយអក្សរ: អ្នកធ្វើឲ្យពាក្យជាន់គ្នា";
const DESC_REPETITION: &str =
    "មាន\u{200B}\u{200B}វារេណវាក្យ\u{200B}ពាក្យ\u{200B} (ឧទាហរណ៍ «បាន\u{200B}បាន»)";
const DESC_REPETITION_SHORT: &str = "មាន\u{200B}\u{200B}វារេណវាក្យ\u{200B}ពាក្យ";
const REPETITION_SIGN: char = '\u{17D7}';

pub struct KhmerWordRepeatRule {
    category: Category,
}

impl KhmerWordRepeatRule {
    pub fn new() -> Self {
        Self {
            category: Category::new("MISC", "របស់ផ្សេងៗ"),
        }
    }

    fn is_word(token: &str) -> bool {
        if token.chars().count() == 1 {
            let c = token.chars().next().unwrap_or('\0');
            if !c.is_alphabetic() {
                return false;
            }
        }
        true
    }

    fn ignore(sentence: &AnalyzedSentence, token_start: usize) -> bool {
        // Java checks tokensWithWS[origPos-1] == "\u0020" (KhmerWordRepeatRule.java:82-87), which is
        // true exactly when the character immediately before the token is a plain space delimiter.
        let relative = token_start.saturating_sub(sentence.start());
        if relative == 0 {
            return false;
        }
        sentence
            .text()
            .get(..relative)
            .and_then(|prefix| prefix.chars().next_back())
            .is_some_and(|c| c == '\u{0020}')
    }
}

impl Default for KhmerWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for KhmerWordRepeatRule {
    fn id(&self) -> &str {
        KM_WORD_REPEAT_RULE
    }

    fn description(&self) -> &str {
        DESC_REPETITION
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Redundancy
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        for i in 1..tokens.len() {
            let token_text = tokens[i].token().token();
            let prev_text = tokens[i - 1].token().token();
            if Self::is_word(token_text)
                && prev_text == token_text
                && !Self::ignore(sentence, tokens[i].token().start())
            {
                let start = tokens[i - 1].token().start();
                let end = tokens[i].token().start() + token_text.len();
                let replacements = vec![
                    SuggestedReplacement::new(format!("{prev_text} {token_text}")),
                    SuggestedReplacement::new(prev_text),
                    SuggestedReplacement::new(format!("{prev_text}{REPETITION_SIGN}")),
                ];
                let rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Redundancy.as_str());
                matches.push(
                    RuleMatch::new(
                        REPETITION,
                        start,
                        end - start,
                        rule,
                        RuleMatchContext::new(
                            sentence.text().to_string(),
                            sentence.start(),
                            sentence.text().len(),
                        ),
                    )
                    .with_short_message(DESC_REPETITION_SHORT)
                    .with_sentence(sentence.text().to_string())
                    .with_match_type(RuleMatchKind::Other)
                    .with_replacements(replacements),
                );
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule_matches(text: &str) -> Vec<(String, Vec<String>)> {
        let sentence = crate::km::example_suite::km_pipeline().analyze_range(text, 0, text.len());
        KhmerWordRepeatRule::new()
            .match_sentence(&sentence)
            .into_iter()
            .map(|m| {
                (
                    sentence.text()[m.offset()..m.offset() + m.length()].to_string(),
                    m.replacements()
                        .iter()
                        .map(|r| r.value().to_string())
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn correct_sentences_stay_clean_like_the_java_test() {
        assert_eq!(
            rule_matches("នេះ\u{200B}ហើយៗ\u{200B}នោះ។"),
            Vec::new(),
            "KhmerWordRepeatRuleTest: ហើយៗ carries the repetition sign, no repeat"
        );
        assert_eq!(
            rule_matches("គាត់\u{200B}ហើយ ហើយ\u{200B}ខ្ញុំ។"),
            Vec::new(),
            "KhmerWordRepeatRuleTest: a real space between the words exempts the pair"
        );
    }

    #[test]
    fn fires_on_zwsp_separated_repeats_with_java_replacements() {
        assert_eq!(
            rule_matches("នេះ\u{200B}ហើយ\u{200B}ហើយ\u{200B}នោះ។"),
            vec![(
                "ហើយ\u{200B}ហើយ".to_string(),
                vec!["ហើយ ហើយ".to_string(), "ហើយ".to_string(), "ហើយៗ".to_string(),],
            )],
            "KhmerWordRepeatRule.java:64-70: the match spans both words including the zero-width \
             space, and the three suggestions insert a real space, drop the duplicate, or append \
             the repetition sign U+17D7"
        );
        assert_eq!(
            rule_matches("ខ្ញុំ\u{200B}និង\u{200B}និង\u{200B}គាត់។").len(),
            1,
            "KhmerWordRepeatRuleTest: និង និង separated by ZWSP flags once"
        );
    }

    #[test]
    fn rule_carries_the_java_id_and_messages_bundle_strings() {
        let rule = KhmerWordRepeatRule::new();
        assert_eq!(rule.id(), "KM_WORD_REPEAT_RULE");
        assert_eq!(rule.description(), DESC_REPETITION);
        assert_eq!(rule.category().name(), "របស់ផ្សេងៗ");
    }
}
