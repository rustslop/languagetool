use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::TextLevelRule,
};

const START_SYMBOLS: &[&str] = &[
    "[", "(", "{", "\u{201C}", "\u{2039}", "\u{201C}", "\u{201E}", "\"",
];
const END_SYMBOLS: &[&str] = &[
    "]", ")", "}", "\u{201D}", "\u{203A}", "\u{201D}", "\u{201D}", "\"",
];

struct TokenView<'a> {
    text: &'a str,
    start: usize,
    whitespace_before: bool,
    sentence_end: bool,
}

struct StackEntry {
    symbol: &'static str,
    is_opening: bool,
    start_pos: usize,
}

fn is_ascii_punctuation(c: char) -> bool {
    c.is_ascii_punctuation()
}

fn is_punctuation(token: &str) -> bool {
    token
        .chars()
        .all(|c| is_ascii_punctuation(c) || matches!(c, '\u{2026}' | '\u{2013}' | '\u{2014}'))
        && !token.is_empty()
}

fn is_punctuation_no_dot(token: &str) -> bool {
    let mut chars = token.chars();
    match chars.next() {
        Some(c) if is_ascii_punctuation(c) && c != '.' => chars.all(is_ascii_punctuation),
        Some('l' | 'd' | 'm' | 'n' | 's' | 't' | 'L' | 'D' | 'M' | 'N' | 'S' | 'T') => {
            token.len() == 2 && chars.next() == Some('\'')
        }
        Some('\u{2013}' | '\u{2014}') => true,
        _ => false,
    }
}

fn end_symbol_is_unique(symbol: &str) -> bool {
    END_SYMBOLS.iter().filter(|s| **s == symbol).count() == 1
}

fn corresponding_symbol(symbol: &str) -> &'static str {
    if let Some(i) = START_SYMBOLS.iter().position(|s| *s == symbol) {
        return END_SYMBOLS[i];
    }
    if let Some(i) = END_SYMBOLS.iter().position(|s| *s == symbol) {
        return START_SYMBOLS[i];
    }
    ""
}

pub struct DutchUnpairedBracketsRule;

pub fn dutch_unpaired_brackets_rule() -> DutchUnpairedBracketsRule {
    DutchUnpairedBracketsRule
}

fn views_of(sentence: &AnalyzedSentence) -> Vec<TokenView<'_>> {
    let all = sentence.tokens();
    let mut out = Vec::new();
    for (idx, t) in all.iter().enumerate() {
        if t.is_whitespace() {
            continue;
        }
        out.push(TokenView {
            text: t.token().token(),
            start: t.token().start(),
            whitespace_before: idx > 0 && all[idx - 1].is_whitespace(),
            sentence_end: t.has_pos_tag("SENT_END"),
        });
    }
    out
}

fn fill_symbol_stack(
    views: &[TokenView<'_>],
    i: usize,
    j: usize,
    base: usize,
    stack: &mut Vec<StackEntry>,
) -> bool {
    let token = views[i].text;
    let symmetric = START_SYMBOLS[j] == END_SYMBOLS[j];
    let start_pos = base + views[i].start;
    if token != START_SYMBOLS[j] && token != END_SYMBOLS[j] {
        return false;
    }
    let preceded_by_whitespace = if symmetric {
        views[i - 1].text == "<S>"
            || views[i].whitespace_before
            || is_punctuation_no_dot(views[i - 1].text)
            || START_SYMBOLS.contains(&views[i - 1].text)
    } else {
        true
    };
    let is_special_case = if symmetric && i + 1 < views.len() {
        let next = &views[i + 1];
        next.whitespace_before
            || is_punctuation(next.text)
            || END_SYMBOLS.contains(&next.text)
            || views[i - 1].text.ends_with('-')
            || next.text.starts_with('-')
            || next.text == "s"
    } else {
        true
    };
    if i >= 2 {
        let prev_prev = views[i - 2].text;
        let prev = views[i - 1].text;
        if (prev_prev == ":" || prev_prev == ";") && prev == "-" && (token == ")" || token == "(") {
            return false;
        }
    }
    if i >= 1 {
        let prev = views[i - 1].text;
        if (prev == ":" || prev == ";")
            && !views[i].whitespace_before
            && (token == ")" || token == "(")
        {
            return false;
        }
        if prev.starts_with("http://") && prev.contains('(') && prev.len() > token.len() {
            return false;
        }
    }

    if preceded_by_whitespace && token == START_SYMBOLS[j] {
        stack.push(StackEntry {
            symbol: START_SYMBOLS[j],
            is_opening: true,
            start_pos,
        });
        return true;
    }
    if (is_special_case || views[i].sentence_end) && token == END_SYMBOLS[j] {
        if let Some(last) = stack.last() {
            if last.symbol == START_SYMBOLS[j] && last.is_opening {
                stack.pop();
                return true;
            }
            if end_symbol_is_unique(END_SYMBOLS[j]) || j == END_SYMBOLS.len() - 1 {
                stack.push(StackEntry {
                    symbol: END_SYMBOLS[j],
                    is_opening: false,
                    start_pos,
                });
                return true;
            }
        } else {
            stack.push(StackEntry {
                symbol: END_SYMBOLS[j],
                is_opening: false,
                start_pos,
            });
            return true;
        }
    }
    false
}

impl TextLevelRule for DutchUnpairedBracketsRule {
    fn id(&self) -> &str {
        "UNPAIRED_BRACKETS"
    }

    fn description(&self) -> &str {
        "Niet-gecombineerde haakjes, aanhalingstekens of andere symbolen"
    }

    fn category(&self) -> Category {
        Category::new("PUNCTUATION", "Interpunctie")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Typographical
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut stack: Vec<StackEntry> = Vec::new();
        let mut base = 0usize;
        for sentence in sentences {
            let views = views_of(sentence);
            for i in 1..views.len() {
                for j in 0..START_SYMBOLS.len() {
                    if fill_symbol_stack(&views, i, j, base, &mut stack) {
                        break;
                    }
                }
            }
            base += sentence.text().len();
        }
        let mut matches = Vec::new();
        for entry in &stack {
            let other = corresponding_symbol(entry.symbol);
            let context_start = entry.start_pos.saturating_sub(30);
            let context_end = std::cmp::min(entry.start_pos + entry.symbol.len() + 30, text.len());
            let rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str());
            matches.push(
                RuleMatch::new(
                    format!("Niet-gecombineerd symbool: \"{other}\" lijkt te ontbreken"),
                    entry.start_pos,
                    entry.symbol.len(),
                    rule,
                    RuleMatchContext::new(
                        text.get(context_start..context_end)
                            .unwrap_or_default()
                            .to_string(),
                        context_start,
                        context_end - context_start,
                    ),
                )
                .with_sentence(text.to_string())
                .with_replacements(vec![SuggestedReplacement::new(entry.symbol)]),
            );
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nl::test_support::DutchPipeline;

    fn matches_for(text: &str) -> Vec<RuleMatch> {
        let pipeline = DutchPipeline::shared();
        let sentences = pipeline.analyze_text(text);
        dutch_unpaired_brackets_rule().match_text(text, &sentences)
    }

    #[test]
    fn java_pins_correct_sentences() {
        assert!(
            matches_for("Het centrale probleem van het werk is de ‘dichterlijke kuischheid’.")
                .is_empty()
        );
        assert!(
            matches_for(
                " Eurlings: “De gegevens van de dienst zijn van cruciaal belang voor de veiligheid van de luchtvaart en de scheepvaart”."
            )
            .is_empty(),
            "several pairs start with the same closing char"
        );
        assert!(
            matches_for(
                " Eurlings: \u{201e}De gegevens van de dienst zijn van cruciaal belang voor de veiligheid van de luchtvaart en de scheepvaart\u{201d}."
            )
            .is_empty(),
            "the try-all-indices loop must pair „ with ”"
        );
    }

    #[test]
    fn java_pins_incorrect_sentences() {
        assert_eq!(
            matches_for("Het centrale probleem van het werk is de „dichterlijke kuischheid.").len(),
            1
        );
        assert_eq!(
            matches_for(
                " Eurlings: “De gegevens van de dienst zijn van cruciaal belang voor de veiligheid van de luchtvaart en de scheepvaart."
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_unmatched_closing_bracket() {
        let matches = matches_for("Dit is een zin).");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), "Dit is een zin".len());
        assert_eq!(matches[0].length(), 1);
        assert!(matches[0].message().contains("(\" lijkt te ontbreken"));
        assert_eq!(matches[0].replacements()[0].value(), ")");
    }

    #[test]
    fn flags_unmatched_opening_bracket_mid_sentence() {
        let matches = matches_for("Een ( zin zonder sluiting.");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), "Een ".len());
        assert!(matches[0].message().contains(")\" lijkt te ontbreken"));
    }

    #[test]
    fn keeps_global_offsets_across_sentences() {
        let text = "Een ( zin zonder sluiting. Nog een zin.";
        let matches = matches_for(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), "Een ".len());
    }

    #[test]
    fn colon_dash_smileys_are_not_brackets() {
        for text in ["Hij lachte :-) breed.", "Zij fronste :-( even."] {
            assert!(matches_for(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn colon_before_brackets_is_ignored() {
        assert!(matches_for("Zie hier: (uitleg) voor details.").is_empty());
    }

    #[test]
    fn flags_unmatched_single_guillemets() {
        let matches = matches_for("Hij zeide ‹ hallo.");
        assert_eq!(matches.len(), 1);
        assert!(matches[0].message().contains("›"));
    }

    #[test]
    fn metadata_pins_punctuation_typographical() {
        use og_core::rule::TextLevelRule as _;
        let rule = dutch_unpaired_brackets_rule();
        assert_eq!(rule.id(), "UNPAIRED_BRACKETS");
        assert_eq!(rule.category().id().as_str(), "PUNCTUATION");
        assert_eq!(rule.issue_type(), IssueType::Typographical);
        assert!(rule.description().contains("haakjes"));
    }

    #[test]
    fn closing_quote_before_a_comma_pairs() {
        assert!(matches_for("Dit is \"fout\", en verder.").is_empty());
    }

    #[test]
    fn quote_glued_to_a_comma_opens_a_quote() {
        let text = "Zie hier,\"citaat verder.";
        let matches = matches_for(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].offset(), text.find('"').unwrap());
    }

    #[test]
    fn quote_glued_to_the_next_word_leaves_the_opening_reported() {
        let text = "Dit is \"fout\"gewijs.";
        let matches = matches_for(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].offset(), text.find('"').unwrap());
        assert!(matches[0].message().contains('"'));
    }

    #[test]
    fn colon_glued_opening_paren_is_ignored_but_the_close_is_flagged() {
        let text = "Zie hier:(uitleg) voor details.";
        let matches = matches_for(text);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].offset(), text.find(')').unwrap());
    }

    #[test]
    fn mismatched_close_pushes_over_a_different_opener() {
        let text = "Dit (was fout].";
        let matches = matches_for(text);
        let offsets: Vec<usize> = matches.iter().map(|m| m.offset()).collect();
        assert_eq!(
            offsets,
            vec![text.find('(').unwrap(), text.find(']').unwrap()]
        );
        assert!(matches[0].message().contains(')'));
        assert!(matches[1].message().contains('['));
    }

    #[test]
    fn punctuation_helpers_pin_the_java_patterns() {
        for apostrophed in ["l'", "d'", "t'", "S'"] {
            assert!(is_punctuation_no_dot(apostrophed), "{apostrophed}");
        }
        for no_dot in [",", "-", "\u{2013}", "\u{2014}"] {
            assert!(is_punctuation_no_dot(no_dot), "{no_dot}");
        }
        for word in [".", "a", "X'", "l", "fout"] {
            assert!(!is_punctuation_no_dot(word), "{word}");
        }
        for punct in ["\u{2026}", "\u{2013}", ",", "!?"] {
            assert!(is_punctuation(punct), "{punct}");
        }
        for word in ["", "fout", "fout,"] {
            assert!(!is_punctuation(word), "{word}");
        }
    }
}
