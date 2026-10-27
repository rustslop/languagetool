use og_core::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, Category, IssueType, RuleMatch,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use std::collections::{HashMap, HashSet, VecDeque};

const MAX_TERMS: usize = 5;
const COMPOUNDS_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/ga/src/main/resources/org/languagetool/resource/ga/compounds.txt"
);

struct CompoundRuleData {
    incorrect: HashSet<String>,
    joined: HashSet<String>,
    joined_lower: HashSet<String>,
    dash: HashSet<String>,
    has_digit_patterns: bool,
}

impl CompoundRuleData {
    fn load(data: &str) -> Self {
        let mut out = Self {
            incorrect: HashSet::new(),
            joined: HashSet::new(),
            joined_lower: HashSet::new(),
            dash: HashSet::new(),
            has_digit_patterns: false,
        };
        for line in data.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut exp = line.replace('-', " ");
            if exp.ends_with('+') {
                exp.truncate(exp.len() - 1);
                out.joined.insert(exp.clone());
            } else if exp.ends_with('*') {
                exp.truncate(exp.len() - 1);
                out.dash.insert(exp.clone());
            } else if exp.ends_with('?') {
                exp.truncate(exp.len() - 1);
                out.joined.insert(exp.clone());
                out.joined_lower.insert(exp.clone());
            } else if exp.ends_with('$') {
                exp.truncate(exp.len() - 1);
                out.joined.insert(exp.clone());
                out.joined_lower.insert(exp.clone());
                out.dash.insert(exp.clone());
            } else {
                out.joined.insert(exp.clone());
                out.dash.insert(exp.clone());
            }
            if exp.contains("\\d") {
                out.has_digit_patterns = true;
            }
            out.incorrect.insert(exp);
        }
        out
    }
}

pub struct IrishCompoundRule {
    data: CompoundRuleData,
}

fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize(input: &str) -> String {
    collapse_whitespace(&input.trim().replace(" - ", " ").replace('-', " "))
}

fn uncapitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => {
            let mut out: String = first.to_lowercase().collect();
            out.push_str(chars.as_str());
            out
        }
        None => String::new(),
    }
}

fn merge_compound(s: &str, uncapitalize_mid: bool) -> String {
    let mut out = String::new();
    for (k, part) in s.replace('-', " ").split(' ').enumerate() {
        if k == 0 {
            out.push_str(part);
        } else if uncapitalize_mid {
            out.push_str(&uncapitalize_first(part));
        } else {
            out.push_str(part);
        }
    }
    out
}

fn collapse_dashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        while c == '-' && chars.peek() == Some(&'-') {
            chars.next();
        }
        out.push(c);
    }
    out
}

fn is_not_all_uppercase(s: &str) -> bool {
    s.split(' ')
        .all(|part| part == "-" || !part.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()))
}

fn replace_digit_runs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            while chars.peek().is_some_and(|n| n.is_ascii_digit()) {
                chars.next();
            }
            out.push_str("\\d+");
        } else {
            out.push(c);
        }
    }
    out
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

impl IrishCompoundRule {
    pub fn new() -> Self {
        Self {
            data: CompoundRuleData::load(COMPOUNDS_TXT),
        }
    }

    pub fn entry_count(&self) -> usize {
        self.data.incorrect.len()
    }
}

impl Default for IrishCompoundRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for IrishCompoundRule {
    fn id(&self) -> &str {
        "GA_COMPOUNDS"
    }

    fn description(&self) -> &str {
        "Focail fhleiscínithe, e.g., Moltar 'ró-úsáid' seachas 'ró úsáid'"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let full = sentence.tokens();
        let mut toks: Vec<(&AnalyzedTokenReadings, bool)> = Vec::with_capacity(full.len());
        for (i, t) in full.iter().enumerate() {
            if t.is_whitespace() {
                continue;
            }
            toks.push((t, i > 0 && full[i - 1].is_whitespace()));
        }
        let n = toks.len();
        let sent_start = sentence.start();
        let text = sentence.text();
        let mut prev_tokens: VecDeque<(&AnalyzedTokenReadings, bool)> =
            VecDeque::with_capacity(MAX_TERMS);
        let synthetic = AnalyzedTokenReadings::new(AnalyzedToken::new("", 0, 0));
        let mut rule_matches: Vec<RuleMatch> = Vec::new();
        let mut prev_from: Option<usize> = None;
        for i in 0..n + MAX_TERMS {
            let current: Option<(&AnalyzedTokenReadings, bool)> = toks.get(i).copied();
            if i == 0 {
                if let Some(pair) = current {
                    prev_tokens.push_back(pair);
                }
                continue;
            }
            if current.is_some_and(|(t, _)| t.is_immunized()) {
                continue;
            }
            let mut strings: Vec<String> = Vec::with_capacity(MAX_TERMS);
            let mut origs: Vec<String> = Vec::with_capacity(MAX_TERMS);
            let mut string_end: HashMap<String, usize> = HashMap::with_capacity(MAX_TERMS * 2);
            let mut sb = String::new();
            let mut is_first_sent_start = false;
            for (j, (atr, ws_before)) in prev_tokens.iter().enumerate() {
                if *ws_before {
                    sb.push(' ');
                }
                // Java's SENT_START token has empty text (JLanguageTool.java:1813);
                // this engine's carries "<S>", so drop it to keep keys identical.
                if !atr.is_sentence_start() {
                    sb.push_str(atr.token().token());
                }
                if j == 0 {
                    is_first_sent_start = atr.has_pos_tag("SENT_START");
                }
                if j >= 1 || (j == 0 && !is_first_sent_start) {
                    let mut key = normalize(&sb);
                    if is_first_sent_start {
                        key = uncapitalize_first(&key);
                    }
                    string_end.entry(key.clone()).or_insert(j);
                    strings.push(key);
                    origs.push(sb.trim().to_string());
                }
            }
            for k in (0..strings.len()).rev() {
                let key = strings[k].as_str();
                let contains_digits =
                    self.data.has_digit_patterns && key.split(' ').any(is_numeric);
                let digit_key = contains_digits.then(|| replace_digit_runs(key));
                let hit = self.data.incorrect.contains(key)
                    || digit_key
                        .as_deref()
                        .is_some_and(|dk| self.data.incorrect.contains(dk));
                if !hit {
                    continue;
                }
                let orig = origs[k].as_str();
                if self.data.dash.contains(key) && !orig.contains(' ') {
                    break;
                }
                let mut replacement: Vec<String> = Vec::new();
                let mut msg: &str = "Litrítear an nath seo mar fhocal amháin nó le fleiscín.";
                if self.data.dash.contains(key)
                    || digit_key
                        .as_deref()
                        .is_some_and(|dk| self.data.incorrect.contains(dk))
                {
                    replacement.push(orig.replace(' ', "-"));
                    msg = "Litrítear an focal seo le fleiscín de ghnáth.";
                }
                if is_not_all_uppercase(orig) && self.data.joined.contains(key) {
                    replacement.push(merge_compound(orig, self.data.joined_lower.contains(key)));
                    msg = "Litrítear an focal seo mar fhocal amháin de ghnáth.";
                }
                if key
                    .split(' ')
                    .next()
                    .is_some_and(|p| p.chars().count() == 1)
                {
                    replacement.clear();
                    replacement.push(orig.replace(' ', "-"));
                    msg = "Litrítear an focal seo le fleiscín de ghnáth.";
                } else if replacement.is_empty() || replacement.len() == 2 {
                    msg = "Litrítear an nath seo mar fhocal amháin nó le fleiscín.";
                }
                let end_j = string_end[strings[k].as_str()];
                let first = prev_tokens
                    .front()
                    .map(|(t, _)| t.token().start())
                    .unwrap_or(0);
                let end = prev_tokens[end_j].0.token().end();
                let original_slice = &text[first - sent_start..end - sent_start];
                replacement = replacement
                    .into_iter()
                    .map(|r| collapse_dashes(&r))
                    .filter(|r| r != original_slice)
                    .collect();
                if replacement.is_empty() {
                    break;
                }
                if prev_from == Some(first) {
                    break;
                }
                prev_from = Some(first);
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                rule_matches.push(
                    RuleMatch::new(
                        msg,
                        first,
                        end - first,
                        rm_rule,
                        super::context_window(sentence, first, end, 40),
                    )
                    .with_short_message("Fadhb leis an bhfleiscíniú")
                    .with_replacements(replacement.iter().map(SuggestedReplacement::new).collect())
                    .with_sentence(text.to_string()),
                );
                break;
            }
            if prev_tokens.len() == MAX_TERMS {
                prev_tokens.pop_front();
            }
            if let Some(pair) = current {
                prev_tokens.push_back(pair);
            } else {
                prev_tokens.push_back((&synthetic, false));
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken};

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let bytes = text.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            let is_space = bytes[i] == b' ';
            let mut j = i;
            while j < bytes.len() && (bytes[j] == b' ') == is_space {
                j += 1;
            }
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                &text[i..j],
                i,
                j,
            )));
            i = j;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn loads_the_two_compound_entries() {
        assert_eq!(IrishCompoundRule::new().entry_count(), 2);
    }

    #[test]
    fn suggests_the_hyphenated_form_mid_sentence() {
        let text = "is ró úsáid a bhí ann";
        let matches = IrishCompoundRule::new().match_sentence(&make_sentence(text));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), text.find("ró úsáid").unwrap());
        assert_eq!(matches[0].length(), "ró úsáid".len());
        assert_eq!(matches[0].replacements()[0].value(), "ró-úsáid");
        assert_eq!(
            matches[0].message(),
            "Litrítear an focal seo le fleiscín de ghnáth."
        );
        assert_eq!(
            matches[0].short_message(),
            Some("Fadhb leis an bhfleiscíniú")
        );
    }

    #[test]
    fn sentence_start_compound_keeps_the_initial_capital() {
        let text = "Ró úsáid a bhí ann";
        let matches = IrishCompoundRule::new().match_sentence(&make_sentence(text));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), text.find("Ró úsáid").unwrap());
        assert_eq!(matches[0].replacements()[0].value(), "Ró-úsáid");
    }

    #[test]
    fn already_hyphenated_compound_is_accepted() {
        let rule = IrishCompoundRule::new();
        assert_eq!(
            rule.match_sentence(&make_sentence("is ró-úsáid a bhí ann"))
                .len(),
            0
        );
        assert_eq!(
            rule.match_sentence(&make_sentence("Ró-úsáid a bhí ann"))
                .len(),
            0
        );
    }

    #[test]
    fn second_dash_entry_and_plain_text_are_ignored() {
        let rule = IrishCompoundRule::new();
        assert_eq!(
            rule.match_sentence(&make_sentence("is mí úsáid a bhí ann"))
                .len(),
            1
        );
        assert_eq!(
            rule.match_sentence(&make_sentence("níl aon fhadhb ann anseo"))
                .len(),
            0
        );
    }

    fn rule_over(data: &str) -> IrishCompoundRule {
        IrishCompoundRule {
            data: CompoundRuleData::load(data),
        }
    }

    fn replacements(text: &str, rule: &IrishCompoundRule) -> Vec<String> {
        rule.match_sentence(&make_sentence(text))
            .first()
            .map(|m| {
                m.replacements()
                    .iter()
                    .map(|r| r.value().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn suffix_markers_pick_the_entry_sets() {
        let data = CompoundRuleData::load(
            "# comment\n\nplus-one+\nlower?\ndollar$\ndash*\nplain-pair\nnum-\\d+-run*\n",
        );
        assert!(data.joined.contains("plus one"));
        assert!(!data.dash.contains("plus one"));
        assert!(data.joined.contains("lower"));
        assert!(data.joined_lower.contains("lower"));
        assert!(!data.dash.contains("lower"));
        assert!(data.joined.contains("dollar"));
        assert!(data.joined_lower.contains("dollar"));
        assert!(data.dash.contains("dollar"));
        assert!(data.joined.contains("plain pair"));
        assert!(data.dash.contains("plain pair"));
        assert!(!data.joined_lower.contains("plain pair"));
        assert!(data.has_digit_patterns);
    }

    #[test]
    fn plus_entries_suggest_only_the_joined_form() {
        let rule = rule_over("ró-úsáid+\n");
        let matches = rule.match_sentence(&make_sentence("is ró úsáid a bhí ann"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "róúsáid");
        assert_eq!(
            matches[0].message(),
            "Litrítear an focal seo mar fhocal amháin de ghnáth."
        );
    }

    #[test]
    fn question_entries_lowercase_the_middle_parts() {
        let rule = rule_over("buí-Sholas?\n");
        assert_eq!(
            replacements("is buí Sholas ann", &rule),
            vec!["buísholas".to_string()],
            "a joined_lower entry uncapitalizes the middle part"
        );
        assert_eq!(
            replacements("Buí Sholas a bhí ann", &rule),
            vec!["Buísholas".to_string()],
            "the sentence-start capital is kept and the middle part lowercased"
        );
    }

    #[test]
    fn dollar_entries_offer_both_forms_with_the_generic_message() {
        let rule = rule_over("ró-úsáid$\n");
        assert_eq!(
            replacements("is ró úsáid a bhí ann", &rule),
            vec!["ró-úsáid".to_string(), "róúsáid".to_string()]
        );
        let matches = rule.match_sentence(&make_sentence("is ró úsáid a bhí ann"));
        assert_eq!(
            matches[0].message(),
            "Litrítear an nath seo mar fhocal amháin nó le fleiscín."
        );
    }

    #[test]
    fn all_uppercase_text_only_gets_the_dash_form() {
        let rule = rule_over("RÓ-ÚSÁID$\n");
        assert_eq!(
            replacements("is RÓ ÚSÁID a bhí ann", &rule),
            vec!["RÓ-ÚSÁID".to_string()],
            "the joined form is skipped when the original is all uppercase"
        );
    }

    #[test]
    fn digit_runs_match_through_the_backslash_d_pattern() {
        let rule = rule_over("ar-\\d+-uair*\n");
        let text = "ar 12 uair";
        let matches = rule.match_sentence(&make_sentence(text));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "ar-12-uair");
        assert_eq!(
            matches[0].message(),
            "Litrítear an focal seo le fleiscín de ghnáth."
        );
    }

    #[test]
    fn single_letter_first_parts_only_get_the_dash_form() {
        let rule = rule_over("a-bhéan$\n");
        assert_eq!(
            replacements("is a bhéan ann", &rule),
            vec!["a-bhéan".to_string()]
        );
        let matches = rule.match_sentence(&make_sentence("is a bhéan ann"));
        assert_eq!(
            matches[0].message(),
            "Litrítear an focal seo le fleiscín de ghnáth."
        );
    }

    #[test]
    fn immunized_tokens_are_skipped() {
        let rule = IrishCompoundRule::new();
        let mut sentence = make_sentence("is ró úsáid a bhí ann");
        sentence.tokens_mut()[3].immunize();
        assert_eq!(rule.match_sentence(&sentence).len(), 0);
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(
            IrishCompoundRule::default().id(),
            IrishCompoundRule::new().id()
        );
    }
}
