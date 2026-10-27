use og_core::rule::{Rule, TextLevelRule};
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use regex::Regex;
use std::collections::{HashMap, HashSet};

pub(crate) fn uppercase_first_char(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => word.to_string(),
    }
}

pub(crate) fn is_capitalized_word(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => {
            chars.all(|c| !c.is_alphabetic() || c.is_lowercase())
        }
        _ => false,
    }
}

fn is_camel_case(word: &str) -> bool {
    let mut chars = word.chars();
    if !chars.next().is_some_and(char::is_lowercase) {
        return false;
    }
    chars.any(char::is_uppercase)
}

pub(crate) fn is_all_uppercase(word: &str) -> bool {
    word.chars()
        .all(|c| !(c.is_alphabetic() && c.is_lowercase()))
}

fn to_id_suffix(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

pub(crate) fn context_around(
    sentence: &AnalyzedSentence,
    start: usize,
    end: usize,
    pad: usize,
) -> RuleMatchContext {
    let text = sentence.text();
    let mut ctx_start = start.saturating_sub(pad);
    while ctx_start > 0 && !text.is_char_boundary(ctx_start) {
        ctx_start -= 1;
    }
    let mut ctx_end = (end + pad).min(text.len());
    while ctx_end < text.len() && !text.is_char_boundary(ctx_end) {
        ctx_end += 1;
    }
    RuleMatchContext::new(
        text[ctx_start..ctx_end].to_string(),
        ctx_start,
        ctx_end - ctx_start,
    )
}

const CHECK_CASE_MESSAGE: &str = "Majúscules i minúscules recomanades. Alguns llibres d'estil poden suggerir solucions diferents en alguns casos.";

#[derive(Clone)]
struct CaseEntry {
    suggestion: String,
    message: Option<String>,
}

pub struct CheckCaseRule {
    entries: HashMap<String, CaseEntry>,
    start_space: HashMap<String, usize>,
    full_space: HashMap<String, CaseEntry>,
    start_no_space: HashMap<char, usize>,
    full_no_space: HashMap<String, CaseEntry>,
}

impl CheckCaseRule {
    pub fn new() -> Self {
        Self::from_data(include_str!(
            "../../../../languagetool/languagetool-language-modules/ca/src/main/resources/org/languagetool/rules/ca/check_case.txt"
        ))
    }

    pub fn from_data(data: &str) -> Self {
        let mut rule = Self {
            entries: HashMap::new(),
            start_space: HashMap::new(),
            full_space: HashMap::new(),
            start_no_space: HashMap::new(),
            full_no_space: HashMap::new(),
        };
        for raw_line in data.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (phrase, message) = match line.split_once('=') {
                Some((phrase, message)) => (phrase.trim(), Some(message.trim().to_string())),
                None => (line, None),
            };
            if phrase.is_empty() {
                continue;
            }
            let entry = CaseEntry {
                suggestion: phrase.to_string(),
                message,
            };
            rule.entries.insert(phrase.to_lowercase(), entry.clone());
            if let Some(space) = phrase.find(' ')
                && space > 0
            {
                let first_token = phrase[..space].to_lowercase();
                let tokens = phrase.split(' ').count();
                let slot = rule.start_space.entry(first_token).or_insert(0);
                *slot = (*slot).max(tokens);
                rule.full_space.insert(phrase.to_lowercase(), entry);
            } else if let Some(first) = phrase.chars().next() {
                let slot = rule.start_no_space.entry(first).or_insert(0);
                *slot = (*slot).max(phrase.chars().count());
                rule.full_no_space.insert(phrase.to_lowercase(), entry);
            }
        }
        rule
    }
}

impl Default for CheckCaseRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for CheckCaseRule {
    fn id(&self) -> &str {
        "CA_CHECKCASE"
    }

    fn description(&self) -> &str {
        "Comprova majúscules i minúscules: $match"
    }

    fn category(&self) -> Category {
        Category::new("CASING", "Majúscules i minúscules")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Typographical
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        const MAX_TOKENS_IN_MULTIWORD: usize = 20;
        const MAX_LENGTH_SHORT_WORDS: usize = 4;
        let all_tokens = sentence.tokens();
        let mut tokens: Vec<&AnalyzedTokenReadings> = Vec::with_capacity(all_tokens.len());
        let mut whitespace_before: Vec<bool> = Vec::with_capacity(all_tokens.len());
        let mut previous_was_whitespace = false;
        for token in all_tokens {
            if token.is_whitespace() {
                previous_was_whitespace = true;
                continue;
            }
            tokens.push(token);
            whitespace_before.push(previous_was_whitespace);
            previous_was_whitespace = false;
        }
        let mut sent_start = 0usize;
        while sent_start < tokens.len()
            && (tokens[sent_start].is_sentence_start()
                || tokens[sent_start]
                    .token()
                    .token()
                    .chars()
                    .all(|c| c.is_ascii_punctuation()))
        {
            sent_start += 1;
        }
        let mut rule_matches: Vec<RuleMatch> = Vec::new();
        let mut check_case_covered_upto = 0usize;
        for start_index in sent_start..tokens.len() {
            if tokens[start_index].is_immunized() {
                continue;
            }
            let first = tokens[start_index].token().token();
            if first.is_empty() {
                continue;
            }
            let mut tok = first.to_string();
            let mut k = start_index + 1;
            while k < tokens.len() && !whitespace_before[k] {
                tok.push_str(tokens[k].token().token());
                k += 1;
            }
            let tok_lower = tok.to_lowercase();
            if let Some(&max_token_len) = self.start_space.get(&tok_lower) {
                let mut key_builder = String::new();
                let mut end_index = start_index;
                while end_index < tokens.len() && end_index - start_index < MAX_TOKENS_IN_MULTIWORD
                {
                    if end_index > start_index && whitespace_before[end_index] {
                        key_builder.push(' ');
                    }
                    key_builder.push_str(tokens[end_index].token().token());
                    let original = key_builder.clone();
                    let number_of_spaces = original.matches(' ').count();
                    if number_of_spaces + 1 > max_token_len {
                        break;
                    }
                    if number_of_spaces > 0
                        && let Some(entry) = self.full_space.get(&original.to_lowercase())
                        && let Some(rule_match) = self.create_case_match(
                            entry,
                            &original,
                            &tokens,
                            start_index,
                            end_index,
                            sentence,
                            sent_start,
                            &mut check_case_covered_upto,
                            &mut rule_matches,
                            MAX_LENGTH_SHORT_WORDS,
                        )
                    {
                        rule_matches.push(rule_match);
                    }
                    end_index += 1;
                }
            }
            if let Some(first_char) = tok.chars().next()
                && self.start_no_space.contains_key(&first_char)
            {
                let mut end_index = start_index;
                let mut key_builder = String::new();
                while end_index < tokens.len() && end_index - start_index < MAX_TOKENS_IN_MULTIWORD
                {
                    if end_index > start_index && whitespace_before[end_index] {
                        break;
                    }
                    key_builder.push_str(tokens[end_index].token().token());
                    let original = key_builder.clone();
                    if let Some(entry) = self.full_no_space.get(&original.to_lowercase())
                        && let Some(rule_match) = self.create_case_match(
                            entry,
                            &original,
                            &tokens,
                            start_index,
                            end_index,
                            sentence,
                            sent_start,
                            &mut check_case_covered_upto,
                            &mut rule_matches,
                            MAX_LENGTH_SHORT_WORDS,
                        )
                    {
                        rule_matches.push(rule_match);
                    }
                    end_index += 1;
                }
            }
        }
        rule_matches
    }
}

impl CheckCaseRule {
    #[allow(clippy::too_many_arguments)]
    fn create_case_match(
        &self,
        entry: &CaseEntry,
        original: &str,
        tokens: &[&AnalyzedTokenReadings],
        start_index: usize,
        end_index: usize,
        sentence: &AnalyzedSentence,
        sent_start: usize,
        check_case_covered_upto: &mut usize,
        rule_matches: &mut Vec<RuleMatch>,
        max_length_short_words: usize,
    ) -> Option<RuleMatch> {
        let replacements: Vec<&str> = vec![entry.suggestion.as_str()];
        let from_pos = tokens[start_index].token().start();
        let to_pos = tokens[end_index].token().end();
        if let Some(last) = rule_matches.last()
            && last.offset() <= from_pos
            && last.offset() + last.length() >= to_pos
        {
            return None;
        }
        let first_word_in_sugg_is_camel_case = replacements
            .iter()
            .any(|k| is_camel_case(k.split(' ').next().unwrap_or("")));
        if end_index <= *check_case_covered_upto {
            return None;
        }
        let replacement = replacements[0];
        if (sent_start == start_index && original == uppercase_first_char(replacement))
            || original == replacement
        {
            if let Some(last) = rule_matches.last()
                && last.offset() + last.length() > from_pos
            {
                rule_matches.pop();
            }
            *check_case_covered_upto = end_index;
            return None;
        }
        if !first_word_in_sugg_is_camel_case
            && original.chars().all(|c| !c.is_lowercase())
            && original.chars().count() > max_length_short_words
        {
            *check_case_covered_upto = end_index;
            return None;
        }
        let mut final_replacements: Vec<String> = Vec::new();
        for repl in &replacements {
            let mut final_repl = (*repl).to_string();
            if sent_start == start_index {
                final_repl = uppercase_first_char(repl);
            }
            if final_repl == original {
                final_replacements.clear();
                break;
            }
            if *repl != original && !final_replacements.contains(&final_repl) {
                final_replacements.push(final_repl);
            }
        }
        if final_replacements.is_empty() {
            return None;
        }
        let message = entry
            .message
            .clone()
            .unwrap_or_else(|| CHECK_CASE_MESSAGE.to_string());
        let id = format!("CA_CHECKCASE_{}", to_id_suffix(original));
        let rm_rule = RuleMatchRule::new(id, self.description().replace("$match", original))
            .with_category(self.category())
            .with_issue_type(IssueType::Typographical.as_str());
        Some(
            RuleMatch::new(
                message,
                from_pos,
                to_pos - from_pos,
                rm_rule,
                context_around(sentence, from_pos, to_pos, 40),
            )
            .with_short_message("Majúscules i minúscules")
            .with_replacements(
                final_replacements
                    .iter()
                    .map(SuggestedReplacement::new)
                    .collect(),
            ),
        )
    }
}

pub struct CatalanUnpairedMarksRule {
    start_symbol: &'static str,
    end_symbol: &'static str,
    id: &'static str,
    description: &'static str,
}

pub fn catalan_unpaired_question_marks_rule() -> CatalanUnpairedMarksRule {
    CatalanUnpairedMarksRule::question()
}

pub fn catalan_unpaired_exclamation_marks_rule() -> CatalanUnpairedMarksRule {
    CatalanUnpairedMarksRule::exclamation()
}

impl CatalanUnpairedMarksRule {
    pub fn question() -> Self {
        Self {
            start_symbol: "\u{BF}",
            end_symbol: "?",
            id: "CA_UNPAIRED_QUESTION",
            description: "Exigeix signe d'interrogació inicial",
        }
    }

    pub fn exclamation() -> Self {
        Self {
            start_symbol: "\u{A1}",
            end_symbol: "!",
            id: "CA_UNPAIRED_EXCLAMATION",
            description: "Exigeix signe d'exclamació inicial",
        }
    }
}

fn is_punctuation_mark(token: &str) -> bool {
    let mut chars = token.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_punctuation() || c.is_whitespace())
        && chars.next().is_none()
}

impl TextLevelRule for CatalanUnpairedMarksRule {
    fn id(&self) -> &str {
        self.id
    }

    fn description(&self) -> &str {
        self.description
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("PUNCTUATION", "Punctuation")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        for sentence in sentences {
            let mut tokens: Vec<&AnalyzedTokenReadings> = Vec::new();
            let mut whitespace_before: Vec<bool> = Vec::new();
            let mut previous_was_whitespace = false;
            for token in sentence.tokens() {
                if token.is_whitespace() {
                    previous_was_whitespace = true;
                    continue;
                }
                tokens.push(token);
                whitespace_before.push(previous_was_whitespace);
                previous_was_whitespace = false;
            }
            let needs_inv_mark_at = has_token_at_pos(self.end_symbol, &tokens, &whitespace_before);
            if needs_inv_mark_at <= 1 {
                continue;
            }
            let mut has_inv_mark = false;
            let mut first_token: Option<usize> = None;
            for i in 0..tokens.len() {
                let token = tokens[i];
                let surface = token.token().token();
                if first_token.is_none()
                    && !token.is_sentence_start()
                    && !is_punctuation_mark(surface)
                {
                    first_token = Some(i);
                }
                if surface == self.start_symbol && i < needs_inv_mark_at {
                    has_inv_mark = true;
                }
                if !token.is_sentence_end() && surface == self.end_symbol && i < needs_inv_mark_at {
                    first_token = None;
                }
                if i > 2 && i + 2 < tokens.len() {
                    let prev = tokens[i - 1].token().token();
                    let next = tokens[i + 1];
                    let next2 = tokens[i + 2];
                    let next_pt_or_dt =
                        next.has_pos_tag_matching("PT.*") || next.has_pos_tag_matching("DT.*");
                    if prev == ","
                        && token.has_pos_tag("CC")
                        && next.has_pos_tag("SPS00")
                        && (next2.has_pos_tag_matching("PT.*")
                            || next2.has_pos_tag_matching("DT.*"))
                    {
                        first_token = Some(i);
                    }
                    if prev == "," && token.has_pos_tag("SPS00") && next_pt_or_dt {
                        first_token = Some(i);
                    }
                    if prev == "," && token.has_pos_tag("CC") && next_pt_or_dt {
                        first_token = Some(i);
                    }
                    if prev == ","
                        && (token.has_pos_tag_matching("PT.*")
                            || token.has_pos_tag_matching("DT.*"))
                    {
                        first_token = Some(i);
                    }
                    if prev == ","
                        && token.has_pos_tag("CC")
                        && matches!(next.token().token(), "no" | "s\u{ED}")
                    {
                        first_token = Some(i);
                    }
                }
                if i > 2
                    && tokens[i - 1].token().token() == ","
                    && matches!(surface, "no" | "s\u{ED}" | "oi" | "eh")
                {
                    first_token = Some(i);
                }
            }
            if let Some(index) = first_token {
                let mut symbol = None;
                if needs_inv_mark_at > 1 && !has_inv_mark {
                    symbol = Some(self.start_symbol);
                }
                if let Some(s) = symbol {
                    let message = format!("S\u{ED}mbol sense parella: Sembla que falta un '{s}'");
                    let start = tokens[index].token().start();
                    let end = tokens[index].token().end();
                    let rm_rule = RuleMatchRule::new(self.id, self.description)
                        .with_category(self.category())
                        .with_issue_type(IssueType::Style.as_str());
                    matches.push(
                        RuleMatch::new(
                            message,
                            start,
                            end - start,
                            rm_rule,
                            context_around(sentence, start, end, 30),
                        )
                        .with_replacements(vec![
                            SuggestedReplacement::new(format!(
                                "{s}{}",
                                tokens[index].token().token()
                            )),
                        ]),
                    );
                }
            }
        }
        matches
    }
}

fn has_token_at_pos(
    ch: &str,
    tokens: &[&AnalyzedTokenReadings],
    whitespace_before: &[bool],
) -> usize {
    let mut i = tokens.len() as isize - 1;
    while i > 0 {
        if tokens[i as usize].token().token() == ch {
            let next = tokens.get(i as usize + 1);
            let joined_to_next = match next {
                Some(next) => {
                    !whitespace_before[i as usize + 1]
                        && !is_punctuation_mark(next.token().token())
                        && !next.is_whitespace()
                }
                None => false,
            };
            if !joined_to_next {
                return i as usize;
            }
        }
        i -= 1;
    }
    0
}

pub struct CatalanWordRepeatRule {
    ignored_repetitions: [&'static str; 9],
}

impl CatalanWordRepeatRule {
    pub fn new() -> Self {
        Self {
            ignored_repetitions: [
                "Phi", "Li", "Xiao", "Duran", "Wagga", "Abdullah", "Nwe", "Pago", "Cao",
            ],
        }
    }
}

impl Default for CatalanWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for CatalanWordRepeatRule {
    fn id(&self) -> &str {
        "CATALAN_WORD_REPEAT_RULE"
    }

    fn description(&self) -> &str {
        "Repetici\u{F3} de paraules (p. ex. \u{AB}m\u{E0} m\u{E0}\u{BB})"
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("duplication".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        let mut prev_token = "";
        for i in 1..tokens.len() {
            let token = tokens[i].token().token();
            if tokens[i].is_immunized() {
                prev_token = "";
                continue;
            }
            let is_word = token
                .chars()
                .any(|c| c.is_alphabetic() || c.is_ascii_digit());
            if is_word && prev_token.eq_ignore_ascii_case(token) && !self.ignore(&tokens, i) {
                let start = tokens[i - 1].token().start();
                let end = tokens[i].token().end();
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(IssueType::Other("duplication".to_string()).as_str());
                matches.push(
                    RuleMatch::new(
                        "Possible error: heu repetit una paraula",
                        start,
                        end - start,
                        rm_rule,
                        context_around(sentence, start, end, 20),
                    )
                    .with_short_message("Repetici\u{F3} de paraules")
                    .with_replacements(vec![SuggestedReplacement::new(
                        tokens[i - 1].token().token(),
                    )]),
                );
            }
            prev_token = token;
        }
        matches
    }
}

impl CatalanWordRepeatRule {
    fn ignore(&self, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        let token = tokens[position];
        let previous = tokens[position - 1];
        if token.has_pos_tag("_allow_repeat")
            || previous.has_pos_tag("_allow_repeat")
            || token.has_pos_tag("LOC_ADV")
            || token.has_lemma("Joan-Llu\u{ED}s Llu\u{ED}s")
            || token.has_lemma("Chitty Chitty Bang Bang")
        {
            return true;
        }
        let surface = token.token().token();
        self.ignored_repetitions.iter().any(|word| {
            surface.eq_ignore_ascii_case(word)
                && previous.token().token().eq_ignore_ascii_case(word)
        })
    }
}

pub struct IgnoreProperNouns;

impl TextLevelRule for IgnoreProperNouns {
    fn id(&self) -> &str {
        "IGNORE_PROPER_NOUNS"
    }

    fn description(&self) -> &str {
        "Ignora noms propis que hagin aparegut en altres parts del text."
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("miscellaneous".to_string())
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut seen_proper_nouns: HashSet<&str> = HashSet::new();
        for sentence in sentences {
            for token in sentence.tokens() {
                let is_proper = token.all_pos_tags().iter().any(|tag| tag.starts_with("NP"));
                if is_proper {
                    seen_proper_nouns.insert(token.token().token());
                }
                let untagged = token.readings().is_empty()
                    || token.readings().iter().all(|r| r.pos_tags().is_empty());
                if untagged && seen_proper_nouns.contains(token.token().token()) {
                    let start = token.token().start();
                    let end = token.token().end();
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str());
                    matches.push(RuleMatch::new(
                        "Aquesta paraula ja aparegut abans i es pot donar per correcta.",
                        start,
                        end - start,
                        rm_rule,
                        context_around(sentence, start, end, 30),
                    ));
                }
            }
        }
        matches
    }
}

struct ContextWords {
    words: [Regex; 2],
    matches: [String; 2],
    contexts: [Regex; 2],
    explanations: [String; 2],
}

fn add_boundaries(pattern: &str) -> String {
    if let Some(stripped) = pattern.strip_prefix("(?i)") {
        format!("(?i)\\b({stripped})\\b")
    } else {
        format!("\\b({pattern})\\b")
    }
}

pub struct CatalanWrongWordInContextRule {
    context_words: Vec<ContextWords>,
}

impl CatalanWrongWordInContextRule {
    pub fn new() -> Self {
        Self::from_data(include_str!(
            "../../../../languagetool/languagetool-language-modules/ca/src/main/resources/org/languagetool/rules/ca/wrongWordInContext.txt"
        ))
    }

    fn from_data(data: &str) -> Self {
        let mut context_words = Vec::new();
        for line in data.lines() {
            let line = line.trim_end();
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let column: Vec<&str> = line.split('\t').collect();
            if column.len() < 6 {
                continue;
            }
            let compile = |pattern: &str| Regex::new(&add_boundaries(pattern));
            let (Ok(word0), Ok(word1), Ok(context0), Ok(context1)) = (
                compile(column[0]),
                compile(column[1]),
                compile(column[4]),
                compile(column[5]),
            ) else {
                continue;
            };
            context_words.push(ContextWords {
                words: [word0, word1],
                matches: [column[2].to_string(), column[3].to_string()],
                contexts: [context0, context1],
                explanations: [
                    column.get(6).unwrap_or(&"").to_string(),
                    column.get(7).unwrap_or(&"").to_string(),
                ],
            });
        }
        Self { context_words }
    }
}

impl Default for CatalanWrongWordInContextRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for CatalanWrongWordInContextRule {
    fn id(&self) -> &str {
        "CATALAN_WRONG_WORD_IN_CONTEXT"
    }

    fn description(&self) -> &str {
        "Confusi\u{F3} segons el context: $match"
    }

    fn category(&self) -> Category {
        Category::new("CONFUSED_WORDS", "Z) Confusions")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        for context_words in &self.context_words {
            let mut matched_word = [false, false];
            let mut matched_pos = [0usize; 2];
            let mut token_text = [String::new(), String::new()];
            for token in tokens.iter().skip(1) {
                if matched_word[0] && matched_word[1] {
                    break;
                }
                if token.all_pos_tags().contains(&"IS_URL") {
                    continue;
                }
                let surface = token.token().token();
                if !matched_word[0] && context_words.words[0].is_match(surface) {
                    matched_word[0] = true;
                    matched_pos[0] = token.token().start();
                    token_text[0] = surface.to_string();
                }
                if !matched_word[1] && context_words.words[1].is_match(surface) {
                    matched_word[1] = true;
                    matched_pos[1] = token.token().start();
                    token_text[1] = surface.to_string();
                }
            }
            let found_word = if matched_word[0] && !matched_word[1] {
                Some(0)
            } else if matched_word[1] && !matched_word[0] {
                Some(1)
            } else {
                None
            };
            let Some(found_word) = found_word else {
                continue;
            };
            let not_found_word = 1 - found_word;
            let mut matched_context = [false, false];
            for token in tokens.iter().skip(1) {
                if matched_context[found_word] && matched_context[not_found_word] {
                    break;
                }
                let mut lemma_or_token = Vec::new();
                for reading in token.readings() {
                    if let Some(lemma) = reading.lemma()
                        && !lemma.is_empty()
                        && !lemma_or_token.iter().any(|l: &String| l == lemma)
                    {
                        lemma_or_token.push(lemma.to_string());
                    }
                }
                if lemma_or_token.is_empty() {
                    lemma_or_token.push(token.token().token().to_string());
                }
                for candidate in &lemma_or_token {
                    if !matched_context[found_word]
                        && context_words.contexts[found_word].is_match(candidate)
                    {
                        matched_context[found_word] = true;
                    }
                    if !matched_context[not_found_word]
                        && context_words.contexts[not_found_word].is_match(candidate)
                    {
                        matched_context[not_found_word] = true;
                    }
                }
            }
            if !matched_context[not_found_word] || matched_context[found_word] {
                continue;
            }
            let matched_token = &token_text[found_word];
            let original = &context_words.matches[found_word];
            let replacement = &context_words.matches[not_found_word];
            let repl = preserve_case(
                &replace_first_case_insensitive(matched_token, original, replacement),
                matched_token,
            );
            let message = if context_words.explanations[not_found_word].is_empty()
                || context_words.explanations[found_word].is_empty()
            {
                format!(
                    "\u{BF}Vol\u{ED}eu dir <suggestion>{repl}</suggestion> en lloc de \"{matched_token}\"?"
                )
            } else {
                format!(
                    "\u{BF}Vol\u{ED}eu dir <suggestion>{repl}</suggestion> ({}) en lloc de \"{matched_token}\" ({})?",
                    context_words.explanations[not_found_word],
                    context_words.explanations[found_word]
                )
            };
            let start = matched_pos[found_word];
            let end = start + matched_token.len();
            let id = format!(
                "CATALAN_WRONG_WORD_IN_CONTEXT_{}_{}",
                to_id_suffix(matched_token),
                to_id_suffix(&repl)
            );
            let rm_rule = RuleMatchRule::new(
                id,
                self.description()
                    .replace("$match", &format!("{matched_token}/{repl}")),
            )
            .with_category(self.category())
            .with_issue_type(IssueType::Misspelling.as_str());
            rule_matches.push(
                RuleMatch::new(
                    message,
                    start,
                    end - start,
                    rm_rule,
                    context_around(sentence, start, end, 30),
                )
                .with_short_message("Possible confusi\u{F3}"),
            );
        }
        rule_matches
    }
}

fn preserve_case(s: &str, original: &str) -> String {
    if original.chars().next().is_some_and(char::is_uppercase) {
        uppercase_first_char(s)
    } else {
        s.to_string()
    }
}

fn replace_first_case_insensitive(haystack: &str, from: &str, to: &str) -> String {
    let lower = haystack.to_lowercase();
    if let Some(pos) = lower.find(&from.to_lowercase()) {
        let mut out = String::with_capacity(haystack.len());
        out.push_str(&haystack[..pos]);
        out.push_str(to);
        out.push_str(&haystack[pos + from.len()..]);
        out
    } else {
        haystack.to_string()
    }
}

pub fn catalan_word_coherency_rule() -> crate::en::WordCoherencyRule {
    crate::en::WordCoherencyRule::for_language(
        "CA_WORD_COHERENCY",
        "Coherència d'ús de variants",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/ca/src/main/resources/org/languagetool/rules/ca/coherency.txt"
        ),
    )
    .with_message_template("No és coherent usar '{word1}' i '{word2}' dins d'un mateix text.")
}

#[cfg(test)]
pub(crate) fn sentence_with_readings(words: &[(&str, &[(&str, &str)])]) -> AnalyzedSentence {
    use og_core::AnalyzedToken;
    let text = words.iter().map(|(w, _)| *w).collect::<Vec<_>>().join(" ");
    let mut s = AnalyzedSentence::new(&text, 0, text.len());
    let mut tokens = vec![AnalyzedTokenReadings::new(
        AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
    )];
    let mut offset = 0;
    let mut first = true;
    for (word, readings) in words {
        if !first {
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                " ",
                offset,
                offset + 1,
            )));
            offset += 1;
        }
        first = false;
        let start = offset;
        let end = start + word.len();
        offset = end;
        let mut atr = AnalyzedTokenReadings::new(AnalyzedToken::new(*word, start, end));
        if !readings.is_empty() {
            let reading_tokens: Vec<AnalyzedToken> = readings
                .iter()
                .map(|(lemma, tag)| {
                    AnalyzedToken::new(*word, start, end)
                        .with_lemma(lemma.to_string())
                        .with_pos_tags(vec![tag.to_string()])
                })
                .collect();
            let primary = reading_tokens[0].clone();
            atr = AnalyzedTokenReadings::new(primary).with_readings(reading_tokens);
        }
        tokens.push(atr);
    }
    s.set_tokens(tokens);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_case_flags_a_wrong_case_and_accepts_the_file_case() {
        let rule = CheckCaseRule::new();
        let sentence =
            sentence_with_readings(&[("El", &[]), ("lliure", &[]), ("Da", &[]), ("Vinci", &[])]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "wrong-case 'Da Vinci' must be flagged");
        assert_eq!(matches[0].replacements()[0].value(), "da Vinci");
        let sentence =
            sentence_with_readings(&[("el", &[]), ("lliure", &[]), ("da", &[]), ("Vinci", &[])]);
        assert!(
            rule.match_sentence(&sentence).is_empty(),
            "the file's own casing stays clean"
        );
    }

    #[test]
    fn check_case_allows_sentence_start_capitalization() {
        let rule = CheckCaseRule::new();
        let sentence = sentence_with_readings(&[
            ("Da", &[]),
            ("Vinci", &[]),
            ("\u{E9}s", &[]),
            ("un", &[]),
            ("pintor", &[]),
        ]);
        assert!(
            rule.match_sentence(&sentence).is_empty(),
            "sentence-start capitalization of 'da Vinci' is allowed like Java"
        );
    }

    #[test]
    fn unpaired_question_rule_flags_a_missing_inverted_mark() {
        let rule = catalan_unpaired_question_marks_rule();
        let sentence = sentence_with_readings(&[
            ("Com", &[]),
            ("est\u{E0}s", &[("estar", "VMIP2S00")]),
            ("avui", &[]),
            ("?", &[]),
        ]);
        let matches = rule.match_text("", &[sentence]);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "S\u{ED}mbol sense parella: Sembla que falta un '\u{BF}'"
        );
        assert_eq!(matches[0].replacements()[0].value(), "\u{BF}Com");
    }

    #[test]
    fn unpaired_question_rule_stays_quiet_when_the_pair_exists() {
        let rule = catalan_unpaired_question_marks_rule();
        let sentence = sentence_with_readings(&[
            ("\u{BF}", &[]),
            ("Com", &[]),
            ("est\u{E0}s", &[]),
            ("?", &[]),
        ]);
        assert!(rule.match_text("", &[sentence]).is_empty());
    }

    #[test]
    fn unpaired_exclamation_rule_uses_the_java_id() {
        let rule = catalan_unpaired_exclamation_marks_rule();
        assert_eq!(rule.id(), "CA_UNPAIRED_EXCLAMATION");
        let sentence = sentence_with_readings(&[("Quin", &[]), ("goig", &[]), ("!", &[])]);
        assert_eq!(rule.match_text("", &[sentence]).len(), 1);
    }

    #[test]
    fn word_repeat_rule_pins_the_java_ignore_list() {
        let rule = CatalanWordRepeatRule::new();
        let sentence = sentence_with_readings(&[("m\u{E0}", &[]), ("m\u{E0}", &[])]);
        assert_eq!(rule.match_sentence(&sentence).len(), 1);
        let sentence = sentence_with_readings(&[("Pago", &[]), ("Pago", &[])]);
        assert!(
            rule.match_sentence(&sentence).is_empty(),
            "Pago Pago is on the shared ignore list"
        );
        let sentence = sentence_with_readings(&[
            ("paraula", &[("paraula", "_allow_repeat")]),
            ("paraula", &[]),
        ]);
        assert!(
            rule.match_sentence(&sentence).is_empty(),
            "_allow_repeat on the previous token suppresses"
        );
    }

    #[test]
    fn ignore_proper_nouns_flags_repeated_untagged_surfaces() {
        let rule = IgnoreProperNouns;
        let sentence = sentence_with_readings(&[
            ("Alguer", &[("Alguer", "NPMS000")]),
            ("\u{E9}s", &[]),
            ("bonic", &[]),
            (".", &[]),
            ("Alguer", &[]),
            ("tamb\u{E9}", &[]),
        ]);
        let second_alguer = sentence
            .text()
            .rfind("Alguer")
            .expect("second Alguer occurrence");
        let matches = rule.match_text("", &[sentence]);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].offset(),
            second_alguer,
            "the second, untagged Alguer occurrence is the flagged one"
        );
    }

    #[test]
    fn wrong_word_in_context_pins_the_java_examples() {
        let rule = CatalanWrongWordInContextRule::new();
        let matches = rule.match_sentence(&sentence_with_readings(&[
            ("Li", &[]),
            ("va", &[]),
            ("infringir", &[]),
            ("un", &[]),
            ("mal", &[]),
            ("terrible", &[]),
            (".", &[]),
        ]));
        assert_eq!(matches.len(), 1, "infringir/inflictir confusion must fire");
        assert!(
            matches[0]
                .message()
                .contains("<suggestion>infligir</suggestion>"),
            "{}",
            matches[0].message()
        );
        let matches = rule.match_sentence(&sentence_with_readings(&[
            ("Sota", &[]),
            ("els", &[]),
            ("palis", &[]),
            (".", &[]),
        ]));
        assert_eq!(matches.len(), 1, "pali/pal\u{B7}li confusion must fire");
        let matches = rule.match_sentence(&sentence_with_readings(&[
            ("Els", &[]),
            ("pal\u{B7}lis", &[]),
            (".", &[]),
        ]));
        assert!(matches.is_empty(), "correct pal\u{B7}lis stays clean");
        let matches = rule.match_sentence(&sentence_with_readings(&[
            ("El", &[]),
            ("pal\u{B7}li", &[]),
            ("i", &[]),
            ("el", &[]),
            ("s\u{E0}nscrit", &[]),
            (".", &[]),
        ]));
        assert_eq!(
            matches.len(),
            1,
            "pal\u{B7}li in the s\u{E0}nscrit context flips to pali"
        );
        let matches = rule.match_sentence(&sentence_with_readings(&[
            ("El", &[]),
            ("pali", &[]),
            ("i", &[]),
            ("el", &[]),
            ("s\u{E0}nscrit", &[]),
            (".", &[]),
        ]));
        assert!(matches.is_empty());
    }

    #[test]
    fn word_coherency_rule_carries_the_java_id_and_message() {
        let rule = catalan_word_coherency_rule();
        assert_eq!(rule.id(), "CA_WORD_COHERENCY");
    }
}
