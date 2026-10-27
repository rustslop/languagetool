use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

const NBSP: &str = "\u{00A0}";
const NARROW_NBSP: &str = "\u{202F}";
const SHORT_MSG: &str = "Insérer une espace insécable";

fn url_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(file|s?ftp|finger|git|gopher|hdl|https?|shttp|imap|mailto|mms|nntp|s?news(post|reply)?|prospero|rsync|rtspu|sips?|svn|svn\+ssh|telnet|wais)$",
        )
        .unwrap()
    })
}

struct AntiElement {
    regex: &'static str,
    whitespace_before_false: bool,
}

const ANTI_PATTERNS: [&[AntiElement]; 7] = [
    &[
        AntiElement {
            regex: r"[:;]",
            whitespace_before_false: false,
        },
        AntiElement {
            regex: r"-",
            whitespace_before_false: true,
        },
        AntiElement {
            regex: r"[\(\)D]",
            whitespace_before_false: true,
        },
    ],
    &[
        AntiElement {
            regex: r"[:;]",
            whitespace_before_false: false,
        },
        AntiElement {
            regex: r"[\(\)D]",
            whitespace_before_false: true,
        },
    ],
    &[
        AntiElement {
            regex: r".*\d{1,2}",
            whitespace_before_false: false,
        },
        AntiElement {
            regex: r":",
            whitespace_before_false: false,
        },
        AntiElement {
            regex: r"\d{1,2}",
            whitespace_before_false: false,
        },
    ],
    &[
        AntiElement {
            regex: r"[?!]",
            whitespace_before_false: false,
        },
        AntiElement {
            regex: r"[?!]",
            whitespace_before_false: false,
        },
    ],
    &[
        AntiElement {
            regex: r"[a-z0-9]{2}",
            whitespace_before_false: false,
        },
        AntiElement {
            regex: r":",
            whitespace_before_false: false,
        },
        AntiElement {
            regex: r"[a-z0-9]{2}",
            whitespace_before_false: false,
        },
        AntiElement {
            regex: r":",
            whitespace_before_false: false,
        },
        AntiElement {
            regex: r"[a-z0-9]{2}",
            whitespace_before_false: false,
        },
    ],
    &[
        AntiElement {
            regex: r";",
            whitespace_before_false: false,
        },
        AntiElement {
            regex: r".+",
            whitespace_before_false: true,
        },
        AntiElement {
            regex: r";",
            whitespace_before_false: true,
        },
    ],
    &[
        AntiElement {
            regex: r".+",
            whitespace_before_false: false,
        },
        AntiElement {
            regex: r";",
            whitespace_before_false: true,
        },
        AntiElement {
            regex: r".+",
            whitespace_before_false: true,
        },
    ],
];

pub struct QuestionWhitespaceRule {
    id: &'static str,
    strict: bool,
    picky: bool,
    default_on: bool,
}

impl QuestionWhitespaceRule {
    pub fn new() -> Self {
        Self {
            id: "FRENCH_WHITESPACE",
            strict: false,
            picky: false,
            default_on: true,
        }
    }

    pub fn strict() -> Self {
        Self {
            id: "FRENCH_WHITESPACE_STRICT",
            strict: true,
            picky: true,
            default_on: false,
        }
    }

    fn token_text(atr: &AnalyzedTokenReadings) -> &str {
        if atr.is_sentence_start() {
            ""
        } else {
            atr.token().token()
        }
    }

    fn is_allowed_whitespace_char(
        tokens: &[AnalyzedTokenReadings],
        i: isize,
        strict: bool,
    ) -> bool {
        if i < 0 {
            return false;
        }
        let atr = &tokens[i as usize];
        if strict {
            let text = Self::token_text(atr);
            text == "\u{202F}" || text == "\u{00A0}" || !atr.is_whitespace()
        } else {
            atr.is_whitespace()
        }
    }

    fn matches_full(regex_source: &str, text: &str) -> bool {
        static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Regex>>> =
            std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        let mut guard = cache.lock().unwrap();
        let re = guard
            .entry(regex_source.to_string())
            .or_insert_with(|| Regex::new(&format!("^(?:{regex_source})$")).unwrap());
        re.is_match(text)
    }

    fn immunized_flags(tokens: &[AnalyzedTokenReadings]) -> Vec<bool> {
        let mut flags = vec![false; tokens.len()];
        let candidates: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_whitespace() && !t.is_sentence_start())
            .map(|(i, _)| i)
            .collect();
        for pattern in ANTI_PATTERNS.iter() {
            if candidates.len() < pattern.len() {
                continue;
            }
            for start in 0..candidates.len() + 1 - pattern.len() {
                let mut matched = true;
                for (offset, element) in pattern.iter().enumerate() {
                    let pos = candidates[start + offset];
                    if offset > 0
                        && element.whitespace_before_false
                        && tokens[pos].token().start()
                            != tokens[candidates[start + offset - 1]].token().end()
                    {
                        matched = false;
                        break;
                    }
                    if !Self::matches_full(element.regex, tokens[pos].token().token()) {
                        matched = false;
                        break;
                    }
                }
                if matched {
                    for offset in 0..pattern.len() {
                        flags[candidates[start + offset]] = true;
                    }
                }
            }
        }
        flags
    }
}

impl Default for QuestionWhitespaceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for QuestionWhitespaceRule {
    fn id(&self) -> &str {
        self.id
    }

    fn description(&self) -> &str {
        "Insertion des espaces fines insécables"
    }

    fn is_default_on(&self) -> bool {
        self.default_on
    }

    fn is_picky(&self) -> bool {
        self.picky
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Règles de base")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Typographical
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.tokens();
        let immunized = Self::immunized_flags(tokens);
        let mut rule_matches = Vec::new();
        let mut prev_prev_token = String::new();
        let mut prev_token = String::new();
        for i in 1..tokens.len() {
            let token = Self::token_text(&tokens[i]);
            if immunized[i] || prev_token == "(" || prev_token == "[" {
                prev_prev_token = prev_token.clone();
                prev_token = token.to_string();
                continue;
            }
            let mut msg: Option<&str> = None;
            let mut suggestion_text = String::new();
            let mut i_from = i - 1;
            let mut i_to = i;
            let is_previous_whitespace = tokens[i - 1].is_whitespace();
            let mut prev_token_to_change = prev_token.clone();
            if is_previous_whitespace {
                prev_token_to_change.clear();
            }
            if !Self::is_allowed_whitespace_char(tokens, i as isize - 1, self.strict) {
                if token == "?" && prev_token != "!" {
                    msg = Some("Le point d'interrogation est précédé d'une espace fine insécable.");
                    suggestion_text = format!("{prev_token_to_change}{NARROW_NBSP}?");
                } else if token == "!" && prev_token != "?" {
                    msg = Some("Le point d'exclamation est précédé d'une espace fine insécable.");
                    suggestion_text = format!("{prev_token_to_change}{NARROW_NBSP}!");
                } else if token == ";" {
                    msg = Some("Le point-virgule est précédé d'une espace fine insécable.");
                    suggestion_text = format!("{prev_token_to_change}{NARROW_NBSP};");
                } else if token == ":" {
                    if !url_pattern().is_match(&prev_token) {
                        msg = Some("Les deux-points sont précédés d'une espace insécable.");
                        suggestion_text = format!("{prev_token_to_change}{NBSP}:");
                    }
                } else if token == "»" {
                    if prev_prev_token == "«" {
                        msg = Some(
                            "Les guillemets sont toujours accompagnés d'une espace insécable.",
                        );
                        suggestion_text = format!("«{NBSP}{prev_token_to_change}{NBSP}»");
                        i_from = i.saturating_sub(2);
                    } else {
                        msg = Some("Le guillemet fermant est précédé d'une espace insécable.");
                        suggestion_text = format!("{prev_token_to_change}{NBSP}»");
                    }
                }
            }

            if prev_token == "«" {
                if token.is_empty() {
                    msg = Some("Le guillemet ouvrant est suivi d'une espace insécable.");
                    suggestion_text = format!("«{NBSP}");
                    i_to = i - 1;
                } else if !Self::is_allowed_whitespace_char(tokens, i as isize, self.strict) {
                    let next_token = tokens.get(i + 1).map(Self::token_text).unwrap_or_default();
                    if next_token != "»" {
                        msg = Some("Le guillemet ouvrant est suivi d'une espace insécable.");
                        if !tokens[i].is_whitespace() {
                            suggestion_text = format!("«{NBSP}{token}");
                        } else {
                            suggestion_text = format!("«{NBSP}");
                        }
                    }
                }
            }

            if let Some(message) = msg {
                let from_pos = tokens[i_from].token().start();
                let to_pos = tokens[i_to].token().end();
                let rm_rule = RuleMatchRule::new(self.id, self.description())
                    .with_picky(self.picky)
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                let ctx_text =
                    crate::fr::context_window(sentence.text(), sentence.start(), from_pos, to_pos);
                rule_matches.push(
                    RuleMatch::new(
                        message,
                        from_pos,
                        to_pos - from_pos,
                        rm_rule,
                        RuleMatchContext::new(ctx_text, from_pos, to_pos - from_pos),
                    )
                    .with_short_message(SHORT_MSG)
                    .with_replacements(vec![SuggestedReplacement::new(&suggestion_text)])
                    .with_sentence(sentence.text().to_string()),
                );
            }
            prev_prev_token = prev_token.clone();
            prev_token = token.to_string();
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings};
    use og_tokenizer::FrenchWordTokenizer;
    use std::sync::Arc;

    use og_tokenizer::WordTokenizer as _;

    fn sentence_from(text: &str) -> AnalyzedSentence {
        let tokenizer = FrenchWordTokenizer::new(Arc::new(|_: &str| false));
        let mut tokens: Vec<AnalyzedTokenReadings> = tokenizer
            .tokenize(text)
            .iter()
            .map(|t| AnalyzedTokenReadings::new(AnalyzedToken::from_token(t)))
            .collect();
        tokens.insert(
            0,
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
        );
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        sentence.set_tokens(tokens);
        sentence
    }

    fn lenient(text: &str) -> Vec<RuleMatch> {
        QuestionWhitespaceRule::new().match_sentence(&sentence_from(text))
    }

    fn strict(text: &str) -> Vec<RuleMatch> {
        QuestionWhitespaceRule::strict().match_sentence(&sentence_from(text))
    }

    fn replacement(m: &RuleMatch) -> String {
        m.replacements()[0].value().to_string()
    }

    #[test]
    fn correct_sentences_do_not_match() {
        for text in [
            "C'est vrai\u{00A0}!",
            "C'est vrai !",
            "Qu'est ce que c'est\u{00A0}?",
            "Qu'est ce que c'est ?",
            "L'enjeu de ce livre est donc triple\u{00A0}: philosophique",
            "L'enjeu de ce livre est donc triple : philosophique",
            "Bonjour :)",
            "Bonjour :-)",
            "5/08/2019 23:30",
            "C'est vrai !!",
            "C'est vrai ??",
            "9:00",
            "00:80:41:ae:fd:7e",
            "Je suis.[!]",
            "Je suis[?]",
            "Je suis(?)",
            "1;2;3",
            "asd@dsa.fr;test@foo.com;",
            "« Je suis Chris… »",
            "« Je suis Chris ! »",
        ] {
            assert!(lenient(text).is_empty(), "{text}: {:?}", lenient(text));
        }
    }

    #[test]
    fn missing_narrow_space_before_punctuation_matches() {
        assert_eq!(lenient("C'est vrai!").len(), 1);
        assert_eq!(lenient("Qu'est ce que c'est?").len(), 1);
        assert_eq!(
            lenient("L'enjeu de ce livre est donc triple: philosophique;").len(),
            2
        );
    }

    #[test]
    fn colon_match_carries_position_and_suggestion() {
        let matches = lenient("L'enjeu de ce livre est donc triple: philosophique ;");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 29);
        assert_eq!(matches[0].offset() + matches[0].length(), 36);
        assert_eq!(replacement(&matches[0]), format!("triple{NBSP}:"));
    }

    #[test]
    fn guillemets_without_spaces_match() {
        let matches = lenient("LanguageTool offre une «vérification» orthographique.");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 23);
        assert_eq!(matches[0].offset() + matches[0].length(), 40);
        assert_eq!(
            replacement(&matches[0]),
            format!("«{NBSP}vérification{NBSP}»")
        );
    }

    #[test]
    fn opening_and_closing_guillemets_match_separately() {
        let matches = lenient(
            "Le guillemet ouvrant est suivi d'un espace insécable\u{00A0}: «mais le lieu [...] et le guillemet fermant est précédé d'un espace insécable\u{00A0}: [...] littérature».",
        );
        assert_eq!(matches.len(), 2);
        assert_eq!(replacement(&matches[0]), format!("«{NBSP}mais"));
        assert_eq!(matches[0].offset(), 57);
        assert_eq!(matches[0].offset() + matches[0].length(), 63);
        assert_eq!(replacement(&matches[1]), format!("littérature{NBSP}»"));
        assert_eq!(matches[1].offset(), 149);
        assert_eq!(matches[1].offset() + matches[1].length(), 163);
    }

    #[test]
    fn strict_rule_is_mutually_exclusive_with_lenient() {
        assert!(strict("C'est vrai\u{00A0}!").is_empty());
        assert!(strict("Qu'est ce que c'est\u{00A0}?").is_empty());
        assert!(strict("L'enjeu de ce livre est donc triple\u{00A0}: philosophique").is_empty());
        assert!(strict("Bonjour :)").is_empty());
        assert!(strict("C'est vrai!").is_empty());
        assert!(strict("Qu'est ce que c'est?").is_empty());
        assert!(strict("L'enjeu de ce livre est donc triple: philosophique;").is_empty());
        assert!(strict(
            "Le guillemet ouvrant est suivi d'un espace insécable\u{00A0}: «mais le lieu [...] et le guillemet fermant est précédé d'un espace insécable\u{00A0}: [...] littérature»."
        )
        .is_empty());
    }

    #[test]
    fn strict_rule_flags_regular_space_before_punctuation() {
        let matches = strict("L'enjeu de ce livre est donc triple: philosophique ;");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 50);
        assert_eq!(matches[0].offset() + matches[0].length(), 52);
        assert_eq!(replacement(&matches[0]), format!("{NARROW_NBSP};"));
        assert_eq!(strict("C'est vrai !").len(), 1);
        assert_eq!(strict("Qu'est ce que c'est ?").len(), 1);
        assert_eq!(strict("Bonjour : )").len(), 1);
        assert_eq!(
            strict(
                "Le guillemet ouvrant est suivi d'un espace insécable\u{00A0}: « mais le lieu [...] et le guillemet fermant est précédé d'un espace insécable\u{00A0}: [...] littérature »."
            )
            .len(),
            2
        );
    }
}
