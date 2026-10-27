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
const DESCRIPTION: &str = "Falta un signe de puntuaci\u{f3} al final del par\u{e0}graf";
const MESSAGE: &str = "Afegiu un signe de puntuaci\u{f3} al final del par\u{e0}graf.";

const PROTOCOLS: [&str; 18] = [
    "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel", "sms", "git",
    "ssh", "data", "magnet", "smb", "slack", "spotify",
];

static P_NUMERIC: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[0-9.]+$").unwrap());
static NO_PROTOCOL_URL: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"^([a-zA-Z0-9][a-zA-Z0-9-]+\.)?([a-zA-Z0-9][a-zA-Z0-9-]+)\.([a-zA-Z0-9][a-zA-Z0-9-]+)/.*$",
    )
    .unwrap()
});

fn is_paragraph_end(sentences: &[AnalyzedSentence], n: usize) -> bool {
    if sentences.is_empty() || n >= sentences.len() - 1 {
        return true;
    }
    let text = sentences[n].text();
    if text.ends_with("\n\n") || text.ends_with("\n\r\n\r") || text.ends_with("\r\n\r\n") {
        return true;
    }
    let next = sentences[n + 1].text();
    next.starts_with('\n') || next.starts_with("\r\n")
}

fn is_url(token: &str) -> bool {
    token.starts_with("www.")
        || token
            .split_once("://")
            .is_some_and(|(scheme, _)| PROTOCOLS.contains(&scheme))
        || NO_PROTOCOL_URL.is_match(token)
}

fn starts_with_ignoring_ascii_case(token: &str, prefix: &str) -> bool {
    token
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
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

fn is_numeric(text: &str) -> bool {
    P_NUMERIC.is_match(text.trim())
}

fn is_linebreak(token: &og_core::AnalyzedTokenReadings) -> bool {
    let t = token.token().token();
    t.contains('\n') || t.contains('\r')
}

pub struct CatalanPunctuationMarkAtParagraphEnd {
    category: Category,
}

impl CatalanPunctuationMarkAtParagraphEnd {
    pub fn new() -> Self {
        Self {
            category: Category::new("PUNCTUATION", "Puntuaci\u{f3}"),
        }
    }
}

impl Default for CatalanPunctuationMarkAtParagraphEnd {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for CatalanPunctuationMarkAtParagraphEnd {
    fn id(&self) -> &str {
        "PUNCTUATION_PARAGRAPH_END"
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn is_picky(&self) -> bool {
        true
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
                            && starts_with_ignoring_ascii_case(last_token, "http"))
                            || starts_with_ignoring_ascii_case(last_token, "ftp")
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
                            let text_len = sentence.text().len();
                            let mut context_start = rel_start.saturating_sub(30);
                            while context_start > 0
                                && !sentence.text().is_char_boundary(context_start)
                            {
                                context_start -= 1;
                            }
                            let mut context_end = std::cmp::min(rel_end + 30, text_len);
                            while context_end < text_len
                                && !sentence.text().is_char_boundary(context_end)
                            {
                                context_end += 1;
                            }
                            let context_text =
                                sentence.text()[context_start..context_end].to_string();
                            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                                .with_category(self.category.clone())
                                .with_issue_type(IssueType::Grammar.as_str())
                                .with_picky(true);
                            let replacements: Vec<SuggestedReplacement> = PUNCTUATION_MARKS
                                .iter()
                                .map(|mark| {
                                    SuggestedReplacement::new(format!("{last_token}{mark}"))
                                })
                                .collect();
                            rule_matches.push(
                                RuleMatch::new(
                                    MESSAGE,
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
    use super::CatalanPunctuationMarkAtParagraphEnd;
    use og_core::rule::TextLevelRule as _;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    fn engine_style_sentences(text: &str) -> Vec<AnalyzedSentence> {
        use og_core::checker::WordTokenizer as _;
        use og_tokenizer::SentenceTokenizer as _;
        let splitter = og_tokenizer::srx::srx_splitter("ca").expect("ca srx splitter");
        let tokenizer = super::super::word_tokenizer::CatalanWordTokenizer::new();
        splitter
            .split(text)
            .iter()
            .map(|range| {
                let sentence_text = &text[range.start()..range.end()];
                let mut sentence = AnalyzedSentence::new(sentence_text, range.start(), range.end());
                let mut tokens = tokenizer.tokenize(sentence_text, range.start());
                tokens.insert(
                    0,
                    AnalyzedTokenReadings::new(
                        AnalyzedToken::new("<S>", range.start(), range.start())
                            .with_pos_tags(vec!["SENT_START".to_string()]),
                    ),
                );
                sentence.set_tokens(tokens);
                sentence
            })
            .collect()
    }

    fn hits(text: &str) -> Vec<og_core::RuleMatch> {
        let rule = CatalanPunctuationMarkAtParagraphEnd::new();
        rule.match_text(text, &engine_style_sentences(text))
    }

    #[test]
    fn java_demo_language_corpus_keeps_its_expectations() {
        for (text, expected) in [
            ("A paragraph.\n2. Some headline\n\n(a) A new sentence.", 0),
            ("A paragraph.\n\n2. Some headline\n\n(a) A new sentence.", 0),
            (
                "A paragraph.\n2.2.1 Some headline\n\n(a) A new sentence.",
                0,
            ),
            (
                "A paragraph.\n\n2.2.1 Some headline\n\n(a) A new sentence.",
                0,
            ),
            ("2. This is an item in a list", 0),
            ("2.2.2. This is an item in a list", 0),
            ("a) This is an item in a list", 0),
            ("a.) This is an item in a list", 0),
            ("\u{2713} This is an item in a list", 0),
            ("* This is an item in a list", 0),
            ("This is a test sentence.", 0),
            ("This is a test headline", 0),
            (
                "This is a test sentence. This is a link: http://example.com",
                0,
            ),
            (
                "This is a test sentence. It can be found at http://example.com/foobar",
                1,
            ),
            (
                "This is a test sentence. And this is a second test sentence",
                1,
            ),
            (
                "\"This is a test sentence. And this is a second test sentence",
                1,
            ),
            (
                "This is a test sentence. And this is a second test sentence.",
                0,
            ),
            ("B. v. \u{2013} Beschluss vom", 0),
            (
                "This is a test sentence.\nAnd this is a second test sentence. Here is a dot missing",
                1,
            ),
            (
                "This is a test sentence.\nAnd this is a second test sentence. Here is a dot missing.",
                0,
            ),
            (
                "This is a sentence. Another one: https://languagetool.org/foo\n\nAnother sentence\n",
                0,
            ),
        ] {
            assert_eq!(
                hits(text).len(),
                expected,
                "PunctuationMarkAtParagraphEndTest.java expects {expected} for {text:?}"
            );
        }
    }

    #[test]
    fn flags_catalan_paragraph_with_java_message_and_replacements() {
        let text = "Aquesta \u{e9}s una frase. I aquesta \u{e9}s una segona frase";
        let matches = hits(text);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(&text[m.offset()..m.offset() + m.length()], "frase");
        assert_eq!(
            m.message(),
            "Afegiu un signe de puntuaci\u{f3} al final del par\u{e0}graf."
        );
        let values: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(
            values,
            vec!["frase.", "frase!", "frase?", "frase:", "frase,", "frase;"]
        );
        assert_eq!(
            m.rule().description(),
            "Falta un signe de puntuaci\u{f3} al final del par\u{e0}graf"
        );
        assert_eq!(m.rule().category().id().as_str(), "PUNCTUATION");
        assert_eq!(m.rule().category().name(), "Puntuaci\u{f3}");
        assert!(
            m.rule().is_picky(),
            "PunctuationMarkAtParagraphEnd.java:53 tags the rule picky so CleanOverlappingFilter.java:96 applies"
        );
    }

    #[test]
    fn picky_gate_stays_silent_at_default_level_even_with_explicit_enable() {
        let rule = CatalanPunctuationMarkAtParagraphEnd::new();
        let language = og_core::Language::from_code("ca").expect("catalan language");
        let default_request = og_core::CheckRequest::new("", language.clone());
        assert!(!default_request.rule_enabled_by_id(
            rule.id(),
            rule.is_default_on(),
            rule.is_picky()
        ));
        let mut picky_request = og_core::CheckRequest::new("", language.clone());
        picky_request.picky = true;
        assert!(picky_request.rule_enabled_by_id(rule.id(), rule.is_default_on(), rule.is_picky()));
        let mut level_request = og_core::CheckRequest::new("", language.clone());
        level_request.level = Some("picky".to_string());
        assert!(level_request.rule_enabled_by_id(rule.id(), rule.is_default_on(), rule.is_picky()));
        let mut enabled_request = og_core::CheckRequest::new("", language);
        enabled_request.enabled_rules = vec![rule.id().to_string()];
        assert!(
            !enabled_request.rule_enabled_by_id(rule.id(), rule.is_default_on(), rule.is_picky()),
            "JLanguageTool.java:1228 suppresses picky-tagged rules at Level.DEFAULT and \
             getActiveRulesForLevelAndToneTags (:1263) filters them after ignoreRule, so \
             enabledRules never opens the default-level picky gate"
        );
    }

    #[test]
    fn url_length_ftp_and_closing_quote_branches_match_java() {
        assert!(hits(
            "This is a test sentence. It can be found at http://example.com/verylong/path/foobar"
        )
        .is_empty());
        assert!(
            hits("This is a test sentence. Get the file via ftp://example.com/pub/file").is_empty()
        );
        let text = "\u{AB}This is a test sentence. And this is a second sentence\u{BB}";
        let matches = hits(text);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(&text[m.offset()..m.offset() + m.length()], "\u{BB}");
        let values: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(values[0], "\u{BB}.");
    }
}
