use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, TextLevelRule,
};
use regex::Regex;
use std::sync::OnceLock;

const ES_START_SYMBOLS: [&str; 8] = ["[", "(", "{", "“", "«", "\"", "'", "‘"];
const ES_END_SYMBOLS: [&str; 8] = ["]", ")", "}", "”", "»", "\"", "'", "’"];

fn numerals_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(?:\d{1,2}?[a-z']*|M*(D?C{0,3}|C[DM])(L?X{0,3}|X[LC])(V?I{0,3}|I[VX]))$")
            .unwrap()
    })
}

fn punctuation_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[\p{P}…–—]$").unwrap())
}

fn punctuation_no_dot_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new("^(?:[ldmnstLDMNST]'|[–—\\p{P}])$").unwrap())
}

fn is_punctuation_no_dot(token: &str) -> bool {
    token != "." && punctuation_no_dot_re().is_match(token)
}

fn valid_before_closing_parenthesis_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d+$|^[a-zA-Z]$").unwrap())
}

fn number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d[\d., ]+\d$|^\d{1,2}$").unwrap())
}

#[derive(Clone)]
struct SymbolEntry {
    symbol: &'static str,
    is_opening: bool,
    start_pos: usize,
    sentence_idx: usize,
}

pub struct SpanishUnpairedBracketsRule {
    id: &'static str,
}

impl SpanishUnpairedBracketsRule {
    pub fn new() -> Self {
        Self {
            id: "ES_UNPAIRED_BRACKETS",
        }
    }

    fn is_quote(token_str: &str) -> bool {
        token_str == "'" || token_str == "’"
    }

    fn is_no_exception(
        &self,
        token_str: &str,
        view: &SentenceView<'_>,
        i: usize,
        _j: usize,
    ) -> bool {
        let tokens = &view.tokens;
        if i > 0 {
            let prev = tokens[i - 1].token().token();
            if (prev.starts_with("http://") || prev.starts_with("https://")) && prev.contains('(') {
                return false;
            }
        }
        if i >= 2 {
            let prev_prev = tokens[i - 2].token().token();
            let prev = tokens[i - 1].token().token();
            if prev_prev == ":" && prev == "-" && (token_str == ")" || token_str == "(") {
                return false;
            }
            if prev_prev == ";" && prev == "-" && (token_str == ")" || token_str == "(") {
                return false;
            }
        }
        if i >= 1 {
            let prev = tokens[i - 1].token().token();
            if prev == ":" && !view.ws_before[i] && (token_str == ")" || token_str == "(") {
                return false;
            }
            if prev == ";" && !view.ws_before[i] && (token_str == ")" || token_str == "(") {
                return false;
            }
        }
        if (token_str == "’" || token_str == "'")
            && (has_pos_tag_starting_with(tokens[i], "N")
                || has_pos_tag_starting_with(tokens[i], "A")
                || tokens[i].has_pos_tag("_allow_apostrophe"))
        {
            return false;
        }
        if i + 1 < tokens.len()
            && Self::is_quote(token_str)
            && !view.ws_before[i + 1]
            && tokens[i + 1].token().token().eq_ignore_ascii_case("s")
        {
            return false;
        }
        if (token_str == "\"" || token_str == "'")
            && !view.ws_before[i]
            && number_re().is_match(tokens[i - 1].token().token())
            && ((i > 2
                && (tokens[i - 2].token().token().contains('º')
                    || tokens[i - 2].token().token().contains('°')))
                || (i > 4
                    && (tokens[i - 4].token().token().contains('º')
                        || tokens[i - 4].token().token().contains('°'))))
        {
            return false;
        }
        if i == 1 && token_str == "»" {
            return false;
        }
        if i > 1 && token_str == ")" {
            let mut is_there_opening_parenthesis = false;
            let mut k = 1;
            while i > k {
                let t = tokens[i - k].token().token();
                if t == ")" {
                    break;
                }
                if t == "(" {
                    is_there_opening_parenthesis = true;
                    break;
                }
                k += 1;
            }
            if !is_there_opening_parenthesis
                && valid_before_closing_parenthesis_re().is_match(tokens[i - 1].token().token())
            {
                return false;
            }
        }
        true
    }
}

fn has_pos_tag_starting_with(atr: &AnalyzedTokenReadings, prefix: &str) -> bool {
    atr.readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.starts_with(prefix)))
}

impl Default for SpanishUnpairedBracketsRule {
    fn default() -> Self {
        Self::new()
    }
}

struct SentenceView<'a> {
    tokens: Vec<&'a AnalyzedTokenReadings>,
    ws_before: Vec<bool>,
}

fn sentence_view(sentence: &AnalyzedSentence) -> SentenceView<'_> {
    let mut tokens = Vec::new();
    let mut ws_before = Vec::new();
    let mut pending_ws = false;
    for atr in sentence.tokens().iter() {
        if atr.is_whitespace() {
            pending_ws = true;
            continue;
        }
        tokens.push(atr);
        ws_before.push(pending_ws);
        pending_ws = false;
    }
    SentenceView { tokens, ws_before }
}

impl TextLevelRule for SpanishUnpairedBracketsRule {
    fn id(&self) -> &str {
        self.id
    }

    fn description(&self) -> &str {
        "Símbolo sin pareja"
    }

    fn category(&self) -> Category {
        Category::new("PUNCTUATION", "Puntuación")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("typographical".to_string())
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut symbol_stack: Vec<SymbolEntry> = Vec::new();
        let mut start_pos_base = 0usize;
        for (sentence_idx, sentence) in sentences.iter().enumerate() {
            let view = sentence_view(sentence);
            for i in 1..view.tokens.len() {
                for j in 0..ES_START_SYMBOLS.len() {
                    let token = view.tokens[i].token().token();
                    let start_pos = start_pos_base + view.tokens[i].token().start();
                    if token != ES_START_SYMBOLS[j] && token != ES_END_SYMBOLS[j] {
                        continue;
                    }
                    let preceded_by_whitespace = if ES_START_SYMBOLS[j] == ES_END_SYMBOLS[j] {
                        view.tokens[i - 1].is_sentence_start()
                            || view.ws_before[i]
                            || is_punctuation_no_dot(view.tokens[i - 1].token().token())
                            || ES_START_SYMBOLS.contains(&view.tokens[i - 1].token().token())
                    } else {
                        true
                    };
                    let is_special_case =
                        if i < view.tokens.len() - 1 && ES_START_SYMBOLS[j] == ES_END_SYMBOLS[j] {
                            let next = view.tokens[i + 1];
                            view.ws_before[i + 1]
                                || punctuation_re().is_match(next.token().token())
                                || ES_END_SYMBOLS.contains(&next.token().token())
                                || view.tokens[i - 1].token().token().ends_with('-')
                                || next.token().token().starts_with('-')
                                || next.token().token() == "s"
                        } else {
                            true
                        };
                    let no_exception = self.is_no_exception(token, &view, i, j);
                    if no_exception && preceded_by_whitespace && token == ES_START_SYMBOLS[j] {
                        symbol_stack.push(SymbolEntry {
                            symbol: ES_START_SYMBOLS[j],
                            is_opening: true,
                            start_pos,
                            sentence_idx,
                        });
                        break;
                    } else if no_exception
                        && (is_special_case || view.tokens[i].is_sentence_end())
                        && token == ES_END_SYMBOLS[j]
                    {
                        let numerals_before_paren = !view.ws_before[i]
                            && ((i > 2
                                && ES_END_SYMBOLS[j] == ")"
                                && (view.tokens[i - 3].has_pos_tag("SENT_START")
                                    || view.ws_before[i - 2])
                                && view.tokens[i - 1].token().token() == "."
                                && numerals_re().is_match(view.tokens[i - 2].token().token())
                                && !matches!(symbol_stack.last(), Some(se) if se.symbol == "("))
                                || (i > 1
                                    && ES_END_SYMBOLS[j] == ")"
                                    && numerals_re().is_match(view.tokens[i - 1].token().token())
                                    && !matches!(symbol_stack.last(), Some(se) if se.symbol == "(")));
                        if numerals_before_paren {
                        } else if let Some(last) = symbol_stack.last() {
                            if last.symbol == ES_START_SYMBOLS[j] {
                                symbol_stack.pop();
                                break;
                            } else if ES_END_SYMBOLS
                                .iter()
                                .filter(|s| **s == ES_END_SYMBOLS[j])
                                .count()
                                == 1
                                || j == ES_END_SYMBOLS.len() - 1
                            {
                                symbol_stack.push(SymbolEntry {
                                    symbol: ES_END_SYMBOLS[j],
                                    is_opening: false,
                                    start_pos,
                                    sentence_idx,
                                });
                                break;
                            }
                        } else {
                            symbol_stack.push(SymbolEntry {
                                symbol: ES_END_SYMBOLS[j],
                                is_opening: false,
                                start_pos,
                                sentence_idx,
                            });
                            break;
                        }
                    }
                }
            }
            start_pos_base += sentence.text().len();
        }

        let mut matches = Vec::new();
        for entry in &symbol_stack {
            let other_symbol = if entry.is_opening {
                ES_END_SYMBOLS[ES_START_SYMBOLS
                    .iter()
                    .position(|s| *s == entry.symbol)
                    .unwrap()]
            } else {
                ES_START_SYMBOLS[ES_END_SYMBOLS
                    .iter()
                    .position(|s| *s == entry.symbol)
                    .unwrap()]
            };
            let sentence = &sentences[entry.sentence_idx];
            if entry.sentence_idx < sentences.len() - 1
                || !entry.is_opening
                || sentence.text().trim_end().ends_with('.')
                || sentence.text().trim_end().ends_with('?')
                || sentence.text().trim_end().ends_with('!')
            {
                let message = format!(
                    "Símbolo sin pareja: falta añadir '{}' o quitar '{}'",
                    other_symbol, entry.symbol
                );
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                let replacement = if entry.is_opening {
                    format!("{other_symbol}{}", entry.symbol)
                } else {
                    format!("{}{other_symbol}", entry.symbol)
                };
                matches.push(
                    RuleMatch::new(
                        message,
                        entry.start_pos,
                        entry.symbol.len(),
                        rm_rule,
                        RuleMatchContext::new(
                            entry.symbol.to_string(),
                            entry.start_pos,
                            entry.symbol.len(),
                        ),
                    )
                    .with_replacements(vec![
                        SuggestedReplacement::new(replacement),
                        SuggestedReplacement::new(String::new()),
                    ])
                    .with_sentence(sentence.text().to_string()),
                );
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::es::test_support::analyze_spanish;

    #[test]
    fn flags_missing_closing_parenthesis() {
        let rule = SpanishUnpairedBracketsRule::new();
        let sentences = analyze_spanish("Eso (no está bien.");
        let matches = rule.match_text("Eso (no está bien.", &sentences);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 4);
    }

    #[test]
    fn ignores_paired_symbols() {
        let rule = SpanishUnpairedBracketsRule::new();
        let sentences = analyze_spanish("Eso (sí está) bien.");
        let matches = rule.match_text("Eso (sí está) bien.", &sentences);
        assert!(matches.is_empty());
    }

    fn match_count(text: &str) -> usize {
        let rule = SpanishUnpairedBracketsRule::new();
        rule.match_text(text, &analyze_spanish(text)).len()
    }

    #[test]
    fn java_test_correct_sentences() {
        for text in [
            "Soy un hombre (muy honrado).",
            "D'Hondt.",
            "Guns N’ Roses",
            "Guns N' Roses",
            "D’Hondt.",
            "L’Équipe",
            "rock ’n’ roll",
            "Harper's Dictionary of Classical Antiquities",
            "Harper’s Dictionary of Classical Antiquities",
        ] {
            assert_eq!(match_count(text), 0, "{text}");
        }
    }

    #[test]
    fn java_test_unpaired_symbols_are_flagged() {
        for text in [
            "Soy un hombre muy honrado).",
            "Soy un hombre (muy honrado.",
            "Eso es “importante y qué pasa. ",
            "Eso es \"importante y qué. ",
            "Eso es (imposible. ",
            "Eso es (imposible.\n\n",
            "Eso es) imposible. ",
            "Eso es imposible).\t\n ",
            "Eso es «importante, ¿ah que sí?",
        ] {
            assert_eq!(match_count(text), 1, "{text}");
        }
    }

    #[test]
    fn java_test_numbered_and_lettered_list_items_pass() {
        for text in [
            "\n\na) New York\nb) Boston\n",
            "\n\n1.) New York\n2.) Boston\n",
            "\n\nXII.) New York\nXIII.) Boston\n",
            "\n\nA) New York\nB) Boston\nC) Foo\n",
        ] {
            assert_eq!(match_count(text), 0, "{text}");
        }
    }

    #[test]
    fn sentence_initial_symbols_are_processed_like_java() {
        assert_eq!(match_count("(hola."), 1);
        assert_eq!(match_count("\"Hola\" dijo."), 0);
    }
}
