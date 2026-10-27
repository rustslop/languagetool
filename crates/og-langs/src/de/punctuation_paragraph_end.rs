use super::helpers::is_paragraph_end;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::TextLevelRule,
};
use std::sync::LazyLock;

const PUNCTUATION_MARKS: [&str; 6] = [".", "!", "?", ":", ",", ";"];
const QUOTATION_MARKS: [&str; 13] = [
    "\u{201E}", "\u{00BB}", "\u{00AB}", "\"", "\u{201D}", "\u{2033}", "\u{2019}", "\u{201A}",
    "\u{2018}", "\u{203A}", "\u{2039}", "\u{2032}", "'",
];
const MAX_URL_LENGTH: usize = 30;

const PROTOCOLS: [&str; 19] = [
    "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel", "sms", "git",
    "ssh", "data", "magnet", "smb", "slack", "spotify", "magnet",
];

static P_NUMERIC: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[0-9.]+$").unwrap());
static NO_PROTOCOL_URL: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"^([a-zA-Z0-9][a-zA-Z0-9-]+\.)?([a-zA-Z0-9][a-zA-Z0-9-]+)\.([a-zA-Z0-9][a-zA-Z0-9-]+)/.*$",
    )
    .unwrap()
});

fn is_url(token: &str) -> bool {
    if PROTOCOLS
        .iter()
        .any(|p| token.starts_with(&format!("{p}://")))
        || token.starts_with("www.")
    {
        return true;
    }
    NO_PROTOCOL_URL.is_match(token)
}

fn is_quotation_mark(token: &str) -> bool {
    QUOTATION_MARKS.contains(&token)
}

fn is_punctuation_mark(token: &str) -> bool {
    PUNCTUATION_MARKS.contains(&token)
}

fn is_word(token: &str) -> bool {
    token.chars().next().is_some_and(char::is_alphabetic)
}

fn is_numeric(s: &str) -> bool {
    P_NUMERIC.is_match(s.trim())
}

fn is_linebreak(token: &og_core::AnalyzedTokenReadings) -> bool {
    let t = token.token().token();
    t.contains('\n') || t.contains('\r')
}

pub struct GermanPunctuationMarkAtParagraphEnd {
    category: Category,
}

impl GermanPunctuationMarkAtParagraphEnd {
    pub fn new() -> Self {
        Self {
            category: Category::new("PUNCTUATION", "Zeichensetzung"),
        }
    }
}

impl Default for GermanPunctuationMarkAtParagraphEnd {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for GermanPunctuationMarkAtParagraphEnd {
    fn id(&self) -> &str {
        "PUNCTUATION_PARAGRAPH_END"
    }

    fn description(&self) -> &str {
        "Kein Satzzeichen am Ende des Absatzes"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let mut last_para: isize = -1;
        for (n, sentence) in sentences.iter().enumerate() {
            if is_paragraph_end(sentences, n) {
                let tokens = sentence.non_whitespace_tokens();
                if tokens.len() > 2 {
                    let first_is_word = (is_word(tokens[1].token().token())
                        && !is_punctuation_mark(tokens[2].token().token()))
                        || (tokens.len() > 3
                            && is_quotation_mark(tokens[1].token().token())
                            && is_word(tokens[2].token().token())
                            && !is_punctuation_mark(tokens[3].token().token()));
                    let mut ignore_sentence = false;
                    if n == 1 && is_numeric(sentences[0].text()) {
                        ignore_sentence = true;
                    }
                    if n > 0 && is_numeric(sentences[n - 1].text()) {
                        ignore_sentence = true;
                    }
                    if n as isize - last_para > 1 && first_is_word && !ignore_sentence {
                        let mut last_nw = tokens.len() - 1;
                        while last_nw > 0 && is_linebreak(tokens[last_nw]) {
                            last_nw -= 1;
                        }
                        if tokens[tokens.len() - 2]
                            .token()
                            .token()
                            .eq_ignore_ascii_case(":")
                            && is_url(tokens[tokens.len() - 1].token().token())
                        {
                            last_para = n as isize;
                            continue;
                        }
                        let last_token = tokens[last_nw].token().token();
                        if (last_token.len() > MAX_URL_LENGTH
                            && last_token.to_lowercase().starts_with("http"))
                            || last_token.to_lowercase().starts_with("ftp")
                        {
                            continue;
                        }
                        if is_word(last_token)
                            || (is_quotation_mark(last_token)
                                && last_nw > 0
                                && is_word(tokens[last_nw - 1].token().token()))
                        {
                            let start = tokens[last_nw].token().start();
                            let end = tokens[last_nw].token().end();
                            let rel_start = start - sentence.start();
                            let rel_end = end - sentence.start();
                            let context_start = rel_start.saturating_sub(30);
                            let context_end = std::cmp::min(rel_end + 30, sentence.text().len());
                            let context_text =
                                sentence.text()[context_start..context_end].to_string();
                            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                                .with_category(self.category.clone())
                                .with_issue_type(IssueType::Grammar.as_str());
                            let replacements: Vec<SuggestedReplacement> = PUNCTUATION_MARKS
                                .iter()
                                .map(|mark| {
                                    SuggestedReplacement::new(format!("{last_token}{mark}"))
                                })
                                .collect();
                            rule_matches.push(
                                RuleMatch::new(
                                    "Bitte ein Satzzeichen am Ende des Absatzes einfügen.",
                                    start,
                                    end - start,
                                    rm_rule,
                                    RuleMatchContext::new(
                                        context_text,
                                        context_start,
                                        context_end - context_start,
                                    ),
                                )
                                .with_sentence(sentence.text().to_string())
                                .with_replacements(replacements),
                            );
                        }
                    }
                }
                last_para = n as isize;
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn detects_missing_punctuation_at_paragraph_end() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let text = "Das ist ein Satz. Das ist noch ein Satz";
        let mut request = og_core::CheckRequest::new(text, engine.language().clone());
        request.enabled_rules = vec!["PUNCTUATION_PARAGRAPH_END".to_string()];
        let matches = engine.check_with_request(&request);
        let hits: Vec<_> = matches
            .matches
            .iter()
            .filter(|m| m.rule().id() == "PUNCTUATION_PARAGRAPH_END")
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            &text[hits[0].offset()..hits[0].offset() + hits[0].length()],
            "Satz"
        );
    }

    #[test]
    fn single_sentence_paragraph_is_ignored() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let text = "Das ist ein einzelner Satz";
        let mut request = og_core::CheckRequest::new(text, engine.language().clone());
        request.enabled_rules = vec!["PUNCTUATION_PARAGRAPH_END".to_string()];
        let matches = engine.check_with_request(&request);
        assert!(
            matches
                .matches
                .iter()
                .all(|m| m.rule().id() != "PUNCTUATION_PARAGRAPH_END")
        );
    }

    use og_core::rule::TextLevelRule as _;

    fn paragraph_end_hits(text: &str) -> Vec<og_core::RuleMatch> {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let mut request = og_core::CheckRequest::new(text, engine.language().clone());
        request.enabled_rules = vec!["PUNCTUATION_PARAGRAPH_END".to_string()];
        engine
            .check_with_request(&request)
            .matches
            .into_iter()
            .filter(|m| m.rule().id() == "PUNCTUATION_PARAGRAPH_END")
            .collect()
    }

    #[test]
    fn is_url_follows_the_java_word_tokenizer_contract() {
        for url in [
            "http://example.com",
            "https://example.com/page",
            "ftp://example.com",
            "file://tmp/report.txt",
            "ws://example.com/socket",
            "www.example.org",
            "example.com/seite",
            "sub.example.org/a/b",
        ] {
            assert!(super::is_url(url), "{url} should be a URL");
        }
        for word in [
            "example.com",
            "Beispiel",
            "http",
            "httpx://example.com",
            "mailto:kontakt@example.com",
        ] {
            assert!(!super::is_url(word), "{word} should not be a URL");
        }
    }

    #[test]
    fn metadata_matches_the_java_registration() {
        let rule = super::GermanPunctuationMarkAtParagraphEnd::default();
        assert_eq!(rule.id(), "PUNCTUATION_PARAGRAPH_END");
        assert_eq!(rule.description(), "Kein Satzzeichen am Ende des Absatzes");
        assert_eq!(rule.category().id().as_str(), "PUNCTUATION");
        assert_eq!(rule.category().name(), "Zeichensetzung");
        assert!(matches!(rule.issue_type(), og_core::IssueType::Grammar));
    }

    #[test]
    fn numeric_leading_sentence_is_ignored() {
        assert!(paragraph_end_hits("2.2.2. Dies ist ein Satz ohne Punkt").is_empty());
    }

    #[test]
    fn numeric_sentence_before_the_last_is_ignored() {
        assert!(
            paragraph_end_hits("Einleitung steht hier. 2.2.2. Und hier fehlt der Punkt").is_empty()
        );
    }

    #[test]
    fn colon_url_at_paragraph_end_is_exempt() {
        assert!(
            paragraph_end_hits("Das ist ein Satz. Mehr dazu gibt es unter: http://example.com")
                .is_empty()
        );
    }

    #[test]
    fn colon_www_url_at_paragraph_end_is_exempt() {
        assert!(
            paragraph_end_hits("Das ist ein Satz. Mehr dazu gibt es unter: www.example.com")
                .is_empty()
        );
    }

    #[test]
    fn long_http_url_at_paragraph_end_is_not_flagged() {
        assert!(
            paragraph_end_hits(
                "Das ist ein Satz. Siehe http://www.eine-ganz-lange-adresse-mit-vielen-zeichen.de"
            )
            .is_empty()
        );
    }

    #[test]
    fn ftp_url_at_paragraph_end_is_not_flagged() {
        assert!(
            paragraph_end_hits("Das ist ein Satz. Der Server steht auf ftp://example.com")
                .is_empty()
        );
    }

    #[test]
    fn closing_quotation_mark_at_paragraph_end_is_flagged() {
        let text = "Das ist ein Satz. Er sagte: \u{201E}Komm doch mal mit\u{201D}";
        let hits = paragraph_end_hits(text);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            &text[hits[0].offset()..hits[0].offset() + hits[0].length()],
            "\u{201D}"
        );
        let replacements: Vec<&str> = hits[0].replacements().iter().map(|r| r.value()).collect();
        assert_eq!(
            replacements,
            [
                "\u{201D}.",
                "\u{201D}!",
                "\u{201D}?",
                "\u{201D}:",
                "\u{201D},",
                "\u{201D};"
            ]
        );
    }

    #[test]
    fn paragraph_final_sentence_starting_with_a_quote_is_checked() {
        let text = "Das ist ein Satz.\n\nEin erster Satz. \u{201E}Hallo sagt er nur";
        let hits = paragraph_end_hits(text);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            &text[hits[0].offset()..hits[0].offset() + hits[0].length()],
            "nur"
        );
    }

    #[test]
    fn linebreak_glued_final_token_is_walked_past() {
        use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

        let mut first = AnalyzedSentence::new("Erster Satz.", 0, 12);
        first.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("<S>", 0, 0)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("Erster", 0, 6)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("Satz.", 7, 12)),
        ]);
        let mut second = AnalyzedSentence::new("Zweiter Satz\n", 19, 32);
        second.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("<S>", 19, 19)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("Zweiter", 19, 26)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("Satz\n", 27, 33)),
        ]);
        let rule = super::GermanPunctuationMarkAtParagraphEnd::new();
        let matches = rule.match_text("Erster Satz. Zweiter Satz\n", &[first, second]);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 19);
        assert_eq!(matches[0].length(), "Zweiter".len());
    }
}
