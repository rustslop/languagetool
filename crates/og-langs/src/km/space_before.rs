use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};
use regex::Regex;
use std::sync::OnceLock;

pub const KM_SPACE_BEFORE_CONJUNCTION: &str = "KM_SPACE_BEFORE_CONJUNCTION";

const SUGGESTION: &str = "Missing white space before conjunction";
const SHORT: &str = "Missing white space";

fn conjunctions_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("^(?:ដើម្បី|និង|ពីព្រោះ)$").unwrap())
}

pub struct KhmerSpaceBeforeRule {
    category: Category,
}

impl KhmerSpaceBeforeRule {
    pub fn new() -> Self {
        Self {
            category: Category::new("MISC", "របស់ផ្សេងៗ"),
        }
    }
}

impl Default for KhmerSpaceBeforeRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for KhmerSpaceBeforeRule {
    fn id(&self) -> &str {
        KM_SPACE_BEFORE_CONJUNCTION
    }

    fn description(&self) -> &str {
        "Checks for missing space before some conjunctions"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Whitespace
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.tokens();
        let mut matches = Vec::new();
        for i in 1..tokens.len() {
            let token = tokens[i].token().token();
            if !conjunctions_re().is_match(token) {
                continue;
            }
            let previous_token = tokens[i - 1].token().token();
            if previous_token == "\u{0020}" || previous_token == "(" {
                continue;
            }
            let start = tokens[i].token().start();
            let rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category.clone())
                .with_issue_type(IssueType::Whitespace.as_str());
            matches.push(
                RuleMatch::new(
                    SUGGESTION,
                    start,
                    token.len(),
                    rule,
                    RuleMatchContext::new(
                        sentence.text().to_string(),
                        sentence.start(),
                        sentence.text().len(),
                    ),
                )
                .with_short_message(SHORT)
                .with_sentence(sentence.text().to_string())
                .with_replacements(vec![SuggestedReplacement::new(format!(" {token}"))]),
            );
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule_matches(text: &str) -> Vec<String> {
        let sentence = crate::km::example_suite::km_pipeline().analyze_range(text, 0, text.len());
        KhmerSpaceBeforeRule::new()
            .match_sentence(&sentence)
            .into_iter()
            .map(|m| m.replacements()[0].value().to_string())
            .collect()
    }

    #[test]
    fn correct_sentences_stay_clean_like_the_java_test() {
        assert_eq!(
            rule_matches("គាត់\u{200B}បាន\u{200B}ទៅ ដើម្បី\u{200B}ទិញ\u{200B}ម្ហូប។"),
            Vec::<String>::new(),
            "KhmerSpaceBeforeRuleTest: a space before ដើម្បី exempts the conjunction"
        );
        assert_eq!(rule_matches("ខ្ញុំ និង\u{200B}គាត់។"), Vec::<String>::new());
        assert_eq!(
            rule_matches("គាត់\u{200B}ចង់\u{200B}បាន ពីព្រោះ\u{200B}គាត់\u{200B}អត់\u{200B}មាន។"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn fires_when_a_conjunction_glues_to_the_previous_word() {
        assert_eq!(
            rule_matches("គាត់\u{200B}បាន\u{200B}ទៅ\u{200B}ដើម្បី\u{200B}ទិញ\u{200B}ម្ហូប។"),
            vec![" ដើម្បី".to_string()]
        );
        assert_eq!(
            rule_matches("ខ្ញុំ\u{200B}និង\u{200B}គាត់។"),
            vec![" និង".to_string()]
        );
        assert_eq!(
            rule_matches("គាត់\u{200B}ចង់\u{200B}បាន\u{200B}ពីព្រោះ\u{200B}គាត់\u{200B}អត់\u{200B}មាន។"),
            vec![" ពីព្រោះ".to_string()]
        );
    }

    #[test]
    fn rule_carries_the_java_id_and_description() {
        let rule = KhmerSpaceBeforeRule::new();
        assert_eq!(rule.id(), "KM_SPACE_BEFORE_CONJUNCTION");
        assert_eq!(
            rule.description(),
            "Checks for missing space before some conjunctions"
        );
    }
}
