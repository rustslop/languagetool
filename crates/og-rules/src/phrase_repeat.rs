use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use og_xml::unifier::eq_ignore_case;
use std::sync::LazyLock;

const MAX_PHRASE_LENGTH: usize = 3;
const MIN_PHRASE_LENGTH: usize = 2;
const DESC_REPETITION: &str = "Word repetition (e.g. 'will will')";
const DESC_REPETITION_SHORT: &str = "Word repetition";
const ISSUE_TYPE_DUPLICATION: &str = "duplication";

static WORD_FOR_SPELLER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[\p{L}\d\p{P}\p{Zs}]+$").unwrap());

pub type PhraseRepeatIgnore = dyn Fn(&[&AnalyzedTokenReadings], usize, usize) -> bool + Send + Sync;

pub struct PhraseRepeatRule {
    id: String,
    description: String,
    short_message: String,
    ignore: Option<Box<PhraseRepeatIgnore>>,
}

impl PhraseRepeatRule {
    pub fn new() -> Self {
        Self {
            id: "PHRASE_REPEAT_RULE".to_string(),
            description: DESC_REPETITION.to_string(),
            short_message: DESC_REPETITION_SHORT.to_string(),
            ignore: None,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_short_message(mut self, short_message: impl Into<String>) -> Self {
        self.short_message = short_message.into();
        self
    }

    pub fn with_ignore<F>(mut self, ignore: F) -> Self
    where
        F: Fn(&[&AnalyzedTokenReadings], usize, usize) -> bool + Send + Sync + 'static,
    {
        self.ignore = Some(Box::new(ignore));
        self
    }
}

impl Default for PhraseRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for PhraseRepeatRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other(ISSUE_TYPE_DUPLICATION.to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let ignore = self.ignore.as_deref();
        scan_phrase_repeats(
            sentence,
            &self.id,
            &self.description,
            &self.short_message,
            &|tokens, position, phrase_length| {
                ignore.is_some_and(|hook| hook(tokens, position, phrase_length))
            },
        )
    }
}

pub fn scan_phrase_repeats(
    sentence: &AnalyzedSentence,
    id: &str,
    description: &str,
    short_message: &str,
    ignore: &dyn Fn(&[&AnalyzedTokenReadings], usize, usize) -> bool,
) -> Vec<RuleMatch> {
    let tokens = sentence.non_whitespace_tokens();
    let mut matches = Vec::new();
    let mut i = 1usize;
    while i < tokens.len() {
        let mut matched_length = 0usize;
        for phrase_length in (MIN_PHRASE_LENGTH..=MAX_PHRASE_LENGTH).rev() {
            if i + 2 * phrase_length <= tokens.len()
                && phrase_repeated_at(&tokens, i, phrase_length)
                && !ignore(&tokens, i, phrase_length)
            {
                matched_length = phrase_length;
                break;
            }
        }
        if matched_length > 0 {
            let start = tokens[i].token().start();
            let end = tokens[i + 2 * matched_length - 1].token().end();
            let phrase = phrase_to_string(&tokens, i, matched_length);
            let rule = RuleMatchRule::new(id, description)
                .with_category(Category::new("MISC", "Miscellaneous"))
                .with_issue_type(ISSUE_TYPE_DUPLICATION);
            matches.push(
                RuleMatch::new(
                    description,
                    start,
                    end - start,
                    rule,
                    context_around(sentence, start, end),
                )
                .with_short_message(short_message)
                .with_replacements(vec![SuggestedReplacement::new(phrase)])
                .with_context_for_sure_match(1),
            );
            i += 2 * matched_length;
        } else {
            i += 1;
        }
    }
    matches
}

fn phrase_repeated_at(
    tokens: &[&AnalyzedTokenReadings],
    position: usize,
    phrase_length: usize,
) -> bool {
    let mut saw_word = false;
    for j in 0..phrase_length {
        let first = tokens[position + j];
        let second = tokens[position + phrase_length + j];
        if first.is_immunized() || second.is_immunized() {
            return false;
        }
        let first_token = first.token().token();
        let second_token = second.token().token();
        if !eq_ignore_case(first_token, second_token) {
            return false;
        }
        if is_word(first_token) {
            saw_word = true;
        }
    }
    saw_word
}

fn phrase_to_string(
    tokens: &[&AnalyzedTokenReadings],
    position: usize,
    phrase_length: usize,
) -> String {
    let mut phrase = String::new();
    for j in 0..phrase_length {
        let token = tokens[position + j];
        if j > 0 && token.token().start() > tokens[position + j - 1].token().end() {
            phrase.push(' ');
        }
        phrase.push_str(token.token().token());
    }
    phrase
}

pub fn is_word(token: &str) -> bool {
    if is_emoji(token) {
        return false;
    }
    if is_numeric_space(token) {
        return false;
    }
    let mut chars = token.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && !c.is_alphabetic()
    {
        return false;
    }
    true
}

// Java StringTools.isEmoji: only tokens whose UTF-16 length exceeds their code
// point count (i.e. containing a surrogate pair) can be an emoji.
fn is_emoji(token: &str) -> bool {
    let utf16_len = token.encode_utf16().count();
    utf16_len > 1 && utf16_len != token.chars().count() && !WORD_FOR_SPELLER.is_match(token)
}

fn is_numeric_space(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| c == ' ' || c.is_numeric())
}

fn context_around(sentence: &AnalyzedSentence, start: usize, end: usize) -> RuleMatchContext {
    let text = sentence.text();
    let mut ctx_start = start.saturating_sub(20);
    while ctx_start > 0 && !text.is_char_boundary(ctx_start) {
        ctx_start -= 1;
    }
    let mut ctx_end = (end + 20).min(text.len());
    while ctx_end < text.len() && !text.is_char_boundary(ctx_end) {
        ctx_end += 1;
    }
    RuleMatchContext::new(
        text[ctx_start..ctx_end].to_string(),
        ctx_start,
        ctx_end - ctx_start,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::AnalyzedToken;

    fn token_at(text: &str, offset: usize) -> AnalyzedTokenReadings {
        AnalyzedTokenReadings::new(AnalyzedToken::new(text, offset, offset + text.len()))
    }

    fn sentence_with_words(text: &str, words: &[&str]) -> AnalyzedSentence {
        let mut sent_start = token_at("<S>", 0);
        sent_start
            .token_mut()
            .set_pos_tags(vec!["SENT_START".to_string()]);
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut all = vec![sent_start];
        let mut cursor = 0usize;
        for word in words {
            let start = cursor
                + text[cursor..]
                    .find(word)
                    .unwrap_or_else(|| panic!("{word} not in {text}"));
            let end = start + word.len();
            all.push(token_at(word, start));
            cursor = end;
        }
        sentence.set_tokens(all);
        sentence
    }

    fn first_suggestion(matches: &[RuleMatch]) -> String {
        matches[0]
            .replacements()
            .first()
            .map(|r| r.value().to_string())
            .expect("a suggested replacement")
    }

    #[test]
    fn flags_two_word_phrase_repeat_with_the_phrase_as_suggestion() {
        let sentence = sentence_with_words(
            "the house the house is green",
            &["the", "house", "the", "house", "is", "green"],
        );
        let matches = PhraseRepeatRule::new().match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(first_suggestion(&matches), "the house");
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].length(), "the house the house".len());
        assert_eq!(matches[0].message(), DESC_REPETITION);
        assert_eq!(matches[0].short_message(), Some(DESC_REPETITION_SHORT));
        assert_eq!(matches[0].context().offset(), 0);
        assert_eq!(matches[0].context().length(), sentence.text().len());
    }

    #[test]
    fn prefers_the_three_word_phrase_over_two_word_overlaps() {
        let sentence = sentence_with_words(
            "the house is the house is green",
            &["the", "house", "is", "the", "house", "is", "green"],
        );
        let matches = PhraseRepeatRule::new().match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(first_suggestion(&matches), "the house is");
        assert_eq!(matches[0].length(), "the house is the house is".len());
    }

    #[test]
    fn compares_tokens_case_insensitively() {
        let sentence = sentence_with_words(
            "The house the HOUSE is green",
            &["The", "house", "the", "HOUSE", "is", "green"],
        );
        let matches = PhraseRepeatRule::new().match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(first_suggestion(&matches), "The house");
    }

    #[test]
    fn skips_past_the_whole_repeated_span() {
        let sentence = sentence_with_words("a la a la a la", &["a", "la", "a", "la", "a", "la"]);
        let matches = PhraseRepeatRule::new().match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(first_suggestion(&matches), "a la");
        assert_eq!(matches[0].length(), "a la a la".len());
    }

    #[test]
    fn immunized_tokens_block_the_phrase_match() {
        let mut sentence = sentence_with_words("a la a la", &["a", "la", "a", "la"]);
        sentence.tokens_mut()[3].immunize();
        assert!(PhraseRepeatRule::new().match_sentence(&sentence).is_empty());
        let mut sentence = sentence_with_words("a la a la", &["a", "la", "a", "la"]);
        sentence.tokens_mut()[1].immunize();
        assert!(PhraseRepeatRule::new().match_sentence(&sentence).is_empty());
    }

    #[test]
    fn punctuation_only_phrases_are_not_flagged() {
        let sentence = sentence_with_words(", , , ,", &[",", ",", ",", ","]);
        assert!(PhraseRepeatRule::new().match_sentence(&sentence).is_empty());
    }

    #[test]
    fn numeric_runs_are_not_flagged() {
        let sentence =
            sentence_with_words("16 15 15 16 15 15", &["16", "15", "15", "16", "15", "15"]);
        assert!(PhraseRepeatRule::new().match_sentence(&sentence).is_empty());
    }

    #[test]
    fn emoji_runs_are_not_flagged() {
        let sentence = sentence_with_words("📚 📚 📚 📚", &["📚", "📚", "📚", "📚"]);
        assert!(PhraseRepeatRule::new().match_sentence(&sentence).is_empty());
    }

    #[test]
    fn single_word_repeats_are_out_of_scope() {
        let sentence = sentence_with_words("the the the", &["the", "the", "the"]);
        assert!(PhraseRepeatRule::new().match_sentence(&sentence).is_empty());
    }

    #[test]
    fn sentences_without_a_repeat_produce_no_matches() {
        let sentence = sentence_with_words("the house is green", &["the", "house", "is", "green"]);
        assert!(PhraseRepeatRule::new().match_sentence(&sentence).is_empty());
        let bare = sentence_with_words("", &[]);
        assert!(PhraseRepeatRule::new().match_sentence(&bare).is_empty());
    }

    #[test]
    fn ignore_hook_suppresses_matches() {
        let sentence = sentence_with_words("a la a la", &["a", "la", "a", "la"]);
        assert_eq!(PhraseRepeatRule::new().match_sentence(&sentence).len(), 1);
        let suppressed = PhraseRepeatRule::new().with_ignore(|_, _, _| true);
        assert!(suppressed.match_sentence(&sentence).is_empty());
        let selective = PhraseRepeatRule::new()
            .with_ignore(|tokens, position, _| tokens[position].token().token() == "a");
        assert!(selective.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn builders_customize_id_and_messages() {
        let sentence = sentence_with_words("a la a la", &["a", "la", "a", "la"]);
        let rule = PhraseRepeatRule::new()
            .with_id("CATALAN_PHRASE_REPEAT_RULE")
            .with_description("Repetició de paraules")
            .with_short_message("Repetició");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].rule().id(), "CATALAN_PHRASE_REPEAT_RULE");
        assert_eq!(matches[0].rule().description(), "Repetició de paraules");
        assert_eq!(matches[0].message(), "Repetició de paraules");
        assert_eq!(matches[0].short_message(), Some("Repetició"));
    }

    #[test]
    fn rule_metadata_matches_java() {
        let rule = PhraseRepeatRule::new();
        assert_eq!(rule.id(), "PHRASE_REPEAT_RULE");
        assert_eq!(rule.description(), "Word repetition (e.g. 'will will')");
        assert_eq!(rule.category().id().as_str(), "MISC");
        assert_eq!(rule.category().name(), "Miscellaneous");
        assert_eq!(rule.issue_type().as_str(), "duplication");
        assert!(rule.is_default_on());
    }

    #[test]
    fn match_carries_duplication_issue_type_and_sure_match_context() {
        let sentence = sentence_with_words("a la a la", &["a", "la", "a", "la"]);
        let matches = PhraseRepeatRule::new().match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        let json = serde_json::to_value(&matches[0]).unwrap();
        assert_eq!(json["rule"]["issueType"], "duplication");
        assert_eq!(json["rule"]["category"]["id"], "MISC");
        assert_eq!(json["contextForSureMatch"], 1);
    }

    #[test]
    fn adjacent_tokens_join_without_a_space_in_the_suggestion() {
        let sentence =
            sentence_with_words("l'americà l'americà", &["l'", "americà", "l'", "americà"]);
        let matches = PhraseRepeatRule::new().match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(first_suggestion(&matches), "l'americà");
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].length(), sentence.text().len());
    }
}
