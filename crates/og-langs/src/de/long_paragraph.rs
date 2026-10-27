use super::helpers::is_paragraph_end;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    rule::TextLevelRule,
};
use std::sync::LazyLock;

const DEFAULT_MAX_WORDS: usize = 220;

static NON_WORD_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r#"^[\.\?!…:;,~’'"“„”»«‚‘›‹()\[\]\-–—*×∗·+÷/=]$"#).unwrap());

fn is_non_word(token: &str) -> bool {
    NON_WORD_REGEX.is_match(token)
}

pub struct GermanLongParagraphRule {
    category: Category,
    max_words: usize,
}

impl GermanLongParagraphRule {
    pub fn new() -> Self {
        Self {
            category: Category::new("STYLE", "Stil"),
            max_words: DEFAULT_MAX_WORDS,
        }
    }
}

impl Default for GermanLongParagraphRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for GermanLongParagraphRule {
    fn id(&self) -> &str {
        "TOO_LONG_PARAGRAPH"
    }

    fn is_picky(&self) -> bool {
        true
    }

    fn description(&self) -> &str {
        "Lesbarkeit: Absatz mit mehr als 220 Wörtern"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let mut start_pos = 0usize;
        let mut end_pos = 0usize;
        let mut word_count = 0usize;
        let mut para_has_linebreaks = false;
        for (n, sentence) in sentences.iter().enumerate() {
            let paragraph_end = is_paragraph_end(sentences, n);
            if !paragraph_end && sentence.text().trim_start_matches('\n').contains('\n') {
                para_has_linebreaks = true;
            }
            let tokens = sentence.non_whitespace_tokens();
            for token in tokens {
                if !token.is_sentence_start() && !is_non_word(token.token().token()) {
                    word_count += 1;
                    if word_count == self.max_words {
                        end_pos = token.token().end();
                    } else if word_count == self.max_words - 1 {
                        start_pos = token.token().start();
                    }
                }
            }
            if paragraph_end {
                if word_count > self.max_words + 5 && !para_has_linebreaks {
                    self.push_match(sentence, start_pos, end_pos, &mut rule_matches);
                }
                word_count = 0;
                para_has_linebreaks = false;
            }
        }
        if word_count > self.max_words
            && let Some(last) = sentences.last()
        {
            self.push_match(last, start_pos, end_pos, &mut rule_matches);
        }
        rule_matches
    }
}

impl GermanLongParagraphRule {
    fn push_match(
        &self,
        sentence: &AnalyzedSentence,
        start: usize,
        end: usize,
        rule_matches: &mut Vec<RuleMatch>,
    ) {
        if end <= start {
            return;
        }
        let rel_start = start.saturating_sub(sentence.start());
        let rel_end = end.saturating_sub(sentence.start());
        let context_start = rel_start.saturating_sub(30);
        let context_end = std::cmp::min(rel_end + 30, sentence.text().len());
        let context_text = sentence.text()[context_start..context_end].to_string();
        let rm_rule = RuleMatchRule::new(self.id(), self.description())
            .with_picky(true)
            .with_category(self.category.clone())
            .with_issue_type(IssueType::Style.as_str());
        rule_matches.push(
            RuleMatch::new(
                "Der Absatz hat an dieser Stelle mehr als 220 Wörter und sollte evtl. umformuliert werden.",
                start,
                end - start,
                rm_rule,
                RuleMatchContext::new(context_text, context_start, context_end - context_start),
            )
            .with_sentence(sentence.text().to_string()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::sentences_split_after;

    #[test]
    fn single_non_word_tokens_are_not_counted() {
        assert!(is_non_word("."));
        assert!(is_non_word(","));
        assert!(!is_non_word("Haus"));
    }

    fn words(count: usize) -> String {
        (1..=count)
            .map(|i| format!("w{i:03}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn matches_for(text: &str, markers: &[&str]) -> Vec<RuleMatch> {
        GermanLongParagraphRule::new().match_text(text, &sentences_split_after(text, markers))
    }

    #[test]
    fn java_test_short_paragraphs_stay_silent() {
        assert!(matches_for("This is a short paragraph.", &[" "]).is_empty());
        assert!(matches_for(&words(223), &[" "]).is_empty());
    }

    #[test]
    fn java_test_long_paragraph_is_flagged_from_word_219_to_220() {
        let text = format!("{}\n\nKurz.", words(230));
        let matches = matches_for(&text, &["\n\n"]);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.rule().id(), "TOO_LONG_PARAGRAPH");
        assert_eq!(m.offset(), text.find("w219").unwrap());
        assert_eq!(
            m.offset() + m.length(),
            text.find("w220").unwrap() + "w220".len()
        );
        assert!(m.message().contains("mehr als 220 Wörter"));
    }

    #[test]
    fn java_test_two_long_paragraphs_yield_two_matches() {
        let text = format!("Kurz.\n\n{}\n\n{}", words(230), words(235));
        let matches = matches_for(&text, &["\n\n", "\n\n"]);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].offset(), text.find("w219").unwrap());
        assert_eq!(
            matches[1].offset(),
            text.rfind("w219").unwrap(),
            "the second paragraph's match"
        );
    }

    #[test]
    fn java_test_manual_linebreaks_suppress_the_warning() {
        let text = format!("Filler. A test.\n{}\nMore words here.", words(230));
        assert!(matches_for(&text, &["\n", "\n"]).is_empty());
    }
}
