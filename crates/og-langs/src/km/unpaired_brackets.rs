use og_core::rule::TextLevelRule;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};

pub const KM_UNPAIRED_BRACKETS_ID: &str = "KM_UNPAIRED_BRACKETS";

#[derive(Clone)]
struct SymbolEntry {
    symbol: String,
    is_opening: bool,
    start_pos: usize,
}

pub struct KhmerUnpairedBracketsRule {
    id: String,
    category: Category,
    start_symbols: Vec<String>,
    end_symbols: Vec<String>,
}

impl KhmerUnpairedBracketsRule {
    pub fn new() -> Self {
        Self::with_symbols(
            vec!["[", "(", "{", "\u{201C}", "\"", "'", "\u{00AB}"],
            vec!["]", ")", "}", "\u{201D}", "\"", "'", "\u{00BB}"],
        )
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_symbols(start: Vec<&str>, end: Vec<&str>) -> Self {
        Self {
            id: KM_UNPAIRED_BRACKETS_ID.to_string(),
            category: Category::new("PUNCTUATION", "Punctuation"),
            start_symbols: start.into_iter().map(str::to_string).collect(),
            end_symbols: end.into_iter().map(str::to_string).collect(),
        }
    }

    fn context_span(&self, text: &str, start: usize, end: usize) -> (usize, usize) {
        let mut context_start = start.saturating_sub(30);
        while context_start > 0 && !text.is_char_boundary(context_start) {
            context_start -= 1;
        }
        let mut context_end = std::cmp::min(end + 30, text.len());
        while context_end < text.len() && !text.is_char_boundary(context_end) {
            context_end += 1;
        }
        (context_start, context_end)
    }
}

impl Default for KhmerUnpairedBracketsRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for KhmerUnpairedBracketsRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Unpaired brackets"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("other".to_string())
    }

    fn match_text(&self, text: &str, _sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut stack: Vec<SymbolEntry> = Vec::new();

        for (byte_idx, ch) in text.char_indices() {
            let ch_str = ch.to_string();

            if (ch == ')' || ch == '(') && byte_idx >= 1 && text.is_char_boundary(byte_idx - 1) {
                let prev = &text[byte_idx - 1..byte_idx];
                if prev == ":" || prev == ";" {
                    continue;
                }
                if byte_idx >= 2 && text.is_char_boundary(byte_idx - 2) {
                    let prev2 = &text[byte_idx - 2..byte_idx - 1];
                    if prev == "-" && (prev2 == ":" || prev2 == ";") {
                        continue;
                    }
                }
            }

            for j in 0..self.start_symbols.len() {
                if ch_str == self.start_symbols[j] {
                    stack.push(SymbolEntry {
                        symbol: self.start_symbols[j].clone(),
                        is_opening: true,
                        start_pos: byte_idx,
                    });
                    break;
                }
                if ch_str == self.end_symbols[j] {
                    if let Some(idx) = stack
                        .iter()
                        .rposition(|s| s.is_opening && s.symbol == self.start_symbols[j])
                    {
                        stack.remove(idx);
                    } else {
                        stack.push(SymbolEntry {
                            symbol: self.end_symbols[j].clone(),
                            is_opening: false,
                            start_pos: byte_idx,
                        });
                    }
                    break;
                }
            }
        }

        for entry in &stack {
            let start = entry.start_pos;
            let end = start + entry.symbol.len();
            let (context_start, context_end) = self.context_span(text, start, end);
            let context_text = text[context_start..context_end].to_string();

            let other = if entry.is_opening {
                let idx = self
                    .start_symbols
                    .iter()
                    .position(|s| s == &entry.symbol)
                    .unwrap();
                self.end_symbols[idx].clone()
            } else {
                let idx = self
                    .end_symbols
                    .iter()
                    .position(|s| s == &entry.symbol)
                    .unwrap();
                self.start_symbols[idx].clone()
            };

            let rule = RuleMatchRule::new(&self.id, self.description())
                .with_category(self.category.clone())
                .with_issue_type(IssueType::Other("other".to_string()).as_str());

            matches.push(
                RuleMatch::new(
                    format!("Unpaired '{}' (missing '{}')", entry.symbol, other),
                    start,
                    end - start,
                    rule,
                    RuleMatchContext::new(context_text, context_start, context_end - context_start),
                )
                .with_sentence("".to_string())
                .with_replacements(vec![SuggestedReplacement::new(entry.symbol.clone())]),
            );
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_the_java_symbol_pairs_and_id() {
        let rule = KhmerUnpairedBracketsRule::new();
        assert_eq!(rule.id(), "KM_UNPAIRED_BRACKETS");
    }

    #[test]
    fn flags_an_unclosed_guillemet_on_khmer_text() {
        let rule = KhmerUnpairedBracketsRule::new();
        let text = "គាត់\u{200B}បាន\u{200B}និយាយ\u{200B}ថា «\u{200B}ខ្ញុំ\u{200B}សប្បាយ\u{200B}ចិត្ត។";
        let matches = rule.match_text(text, &[]);
        assert_eq!(matches.len(), 1);
        assert!(text[matches[0].offset()..].starts_with('«'));
    }

    #[test]
    fn paired_guillemets_stay_clean() {
        let rule = KhmerUnpairedBracketsRule::new();
        let text = "«\u{200B}ខ្ញុំ\u{200B}សប្បាយ\u{200B}ចិត្ត\u{200B}»។";
        assert!(rule.match_text(text, &[]).is_empty());
    }

    #[test]
    fn closing_without_opening_flags_the_missing_opener() {
        let rule = KhmerUnpairedBracketsRule::new();
        let text = "ឃ្លា\u{200B}បាន\u{200B}បិទ\u{200B}ដោយ » មួយ។";
        let matches = rule.match_text(text, &[]);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].message().contains("«"));
    }
}
