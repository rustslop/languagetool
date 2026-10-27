use og_core::rule::{Rule, TextLevelRule};
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};
use std::collections::HashMap;
use std::sync::Arc;

pub mod word_tokenizer;

pub use word_tokenizer::PersianWordTokenizer;

#[cfg(test)]
mod example_suite;

const FA_COMMA: char = '\u{060C}';

const DESC_REPETITION: &str = "تکرار کلمه (برای نمونه 'شد شد')";
const MSG_REPETITION: &str = "اشتباه تایپی متحمل: شما یک کلمه را تکرار کرده\u{200c}اید";
const SHORT_REPETITION: &str = "تکرار کلمه";

fn local_context(
    sentence: &AnalyzedSentence,
    start: usize,
    end: usize,
    pad: usize,
) -> (String, usize, usize) {
    let text = sentence.text();
    let text_len = text.len();
    let mut ctx_start = start.saturating_sub(sentence.start()).saturating_sub(pad);
    while ctx_start > 0 && !text.is_char_boundary(ctx_start) {
        ctx_start -= 1;
    }
    let mut ctx_end = ((end.saturating_sub(sentence.start())) + pad).min(text_len);
    while ctx_end < text_len && !text.is_char_boundary(ctx_end) {
        ctx_end += 1;
    }
    (
        text[ctx_start..ctx_end].to_string(),
        sentence.start() + ctx_start,
        sentence.start() + ctx_end,
    )
}

fn eq_ignore_case(a: &str, b: &str) -> bool {
    let mut ai = a.chars().flat_map(char::to_uppercase);
    let mut bi = b.chars().flat_map(char::to_uppercase);
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if x == y => {}
            _ => return false,
        }
    }
}

fn is_emoji_token(token: &str) -> bool {
    token.chars().any(|c| (c as u32) > 0xFFFF)
        && !token.chars().all(|c| {
            c.is_alphabetic()
                || c.is_numeric()
                || c.is_whitespace()
                || c.is_ascii_punctuation()
                || matches!(c, '\u{00A1}'..='\u{00BF}' | '\u{2010}'..='\u{2027}')
        })
}

fn is_repeat_word(token: &str) -> bool {
    if is_emoji_token(token) {
        return false;
    }
    if !token.is_empty() && token.chars().all(|c| c.is_numeric() || c == ' ') {
        return false;
    }
    if token.chars().count() == 1 {
        return token.chars().next().is_some_and(char::is_alphabetic);
    }
    true
}

pub struct PersianCommaWhitespaceRule {
    inner: og_rules::native_rules::CommaWhitespaceRule,
}

impl PersianCommaWhitespaceRule {
    pub fn new() -> Self {
        Self {
            inner: og_rules::native_rules::CommaWhitespaceRule::new()
                .with_id("PERSIAN_COMMA_PARENTHESIS_WHITESPACE")
                .with_comma_character(FA_COMMA),
        }
    }
}

impl Default for PersianCommaWhitespaceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for PersianCommaWhitespaceRule {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn description(&self) -> &str {
        "استفاده از فاصله\u{200c}های پیش از کاما و بعد و قبل از پرانتزها"
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("TYPOGRAPHY", "Typography")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Whitespace
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        self.inner.match_sentence(sentence)
    }
}

pub fn persian_double_punctuation_rule() -> og_rules::native_rules::DoublePunctuationRule {
    og_rules::native_rules::DoublePunctuationRule::new()
        .with_id("PERSIAN_DOUBLE_PUNCTUATION")
        .with_description("استفاده از دو نقطهٔ پشت\u{200c}سر هم یا کاماها")
        .with_comma_character(FA_COMMA)
        .with_messages(
            "دو نقطهٔ پشت سر هم",
            "دو نقطهٔ پشت\u{200c}سرهم",
            "دو کامای پشت سر هم",
            "دو کامای پشت\u{200c}سرهم",
        )
}

const PERSIAN_REDUPLICATIONS: [&str; 6] = ["لی", "سی", "لک", "ریز", "جز", "کل"];
const BASE_NAME_REPEATS: [&str; 9] = [
    "Phi", "Li", "Xiao", "Duran", "Wagga", "Abdullah", "Nwe", "Pago", "Cao",
];

pub struct PersianWordRepeatRule {
    id: String,
    description: String,
    category: Category,
}

impl PersianWordRepeatRule {
    pub fn new() -> Self {
        Self {
            id: "PERSIAN_WORD_REPEAT_RULE".to_string(),
            description: DESC_REPETITION.to_string(),
            category: Category::new("MISC", "متفرقه"),
        }
    }

    fn is_ignored_repeat(&self, prev: &str, curr: &str) -> bool {
        let repeated = prev == curr;
        repeated
            && (PERSIAN_REDUPLICATIONS.contains(&prev)
                || BASE_NAME_REPEATS.iter().any(|w| eq_ignore_case(w, prev)))
    }
}

impl Default for PersianWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for PersianWordRepeatRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Redundancy
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        let mut prev_token = "";
        let mut prev_start = 0usize;
        for token in &tokens {
            let text = token.token().token();
            if token.is_immunized() {
                prev_token = "";
                prev_start = token.token().start();
                continue;
            }
            if is_repeat_word(text)
                && !prev_token.is_empty()
                && eq_ignore_case(prev_token, text)
                && !self.is_ignored_repeat(prev_token, text)
            {
                let start = prev_start;
                let end = token.token().start() + prev_token.len();
                let (context_text, ctx_start, ctx_end) = local_context(sentence, start, end, 20);
                let rule = RuleMatchRule::new(&self.id, &self.description)
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Redundancy.as_str());
                matches.push(
                    RuleMatch::new(
                        MSG_REPETITION.to_string(),
                        start,
                        end - start,
                        rule,
                        RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                    )
                    .with_short_message(SHORT_REPETITION.to_string())
                    .with_replacements(vec![SuggestedReplacement::new(prev_token)])
                    .with_sentence(sentence.text().to_string()),
                );
            }
            prev_token = text;
            prev_start = token.token().start();
        }
        matches
    }
}

const ADVERBS: [&str; 9] = [
    "هم",
    "همچنین",
    "نیز",
    "از یک سو",
    "از یک طرف",
    "از طرف ديگر",
    "بنابراین",
    "حتی",
    "چنانچه",
];

const BEGINNING_EXCEPTIONS: [&str; 8] = [":", "–", "-", "✔️", "➡️", "—", "⭐️", "⚠️"];

const MSG_BEGINNING_ADV: &str = "دو جملهٔ پست سر هم با حرف ربط مشابه شروع شده\u{200c}اند";
const MSG_BEGINNING_WORD: &str = "سه جملهٔ پشت سر هم با کلمه\u{200c}ای مشابه شروع شده\u{200c}اند";
const MSG_BEGINNING_THESAURUS: &str =
    "Consider rewording the sentence or use a thesaurus to find a synonym.";

pub struct PersianWordRepeatBeginningRule {
    id: String,
    description: String,
    category: Category,
}

impl PersianWordRepeatBeginningRule {
    pub fn new() -> Self {
        Self {
            id: "PERSIAN_WORD_REPEAT_BEGINNING_RULE".to_string(),
            description: "جملهٔ  بعدی هم  با کلمه\u{200c}ای مشابه شروع شده\u{200c}است".to_string(),
            category: Category::new("REPETITIONS_STYLE", "Repetitions (Style)"),
        }
    }

    fn is_exception(token: &str) -> bool {
        BEGINNING_EXCEPTIONS.contains(&token)
    }

    fn is_adverb(token: &str) -> bool {
        ADVERBS.contains(&token)
    }
}

impl Default for PersianWordRepeatBeginningRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for PersianWordRepeatBeginningRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut last_token = String::new();
        let mut before_last_token = String::new();
        let mut prev_sentence_ends_with_punct = false;
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            let mut token = "";
            if tokens.len() > 1 {
                let analyzed = &tokens[1];
                token = analyzed.token().token();
                if tokens.len() > 3 {
                    let is_word = token.chars().count() != 1
                        || token.chars().next().is_some_and(char::is_alphabetic);
                    if is_word
                        && last_token == token
                        && !Self::is_exception(token)
                        && !Self::is_exception(tokens[2].token().token())
                        && !Self::is_exception(tokens[3].token().token())
                        && prev_sentence_ends_with_punct
                    {
                        let short_msg = if Self::is_adverb(token) {
                            MSG_BEGINNING_ADV
                        } else if before_last_token == token {
                            MSG_BEGINNING_WORD
                        } else {
                            ""
                        };
                        if !short_msg.is_empty() {
                            let start = analyzed.token().start();
                            let end = start + token.len();
                            let (context_text, ctx_start, ctx_end) =
                                local_context(sentence, start, end, 30);
                            let rule = RuleMatchRule::new(&self.id, &self.description)
                                .with_category(self.category.clone())
                                .with_issue_type(IssueType::Style.as_str());
                            matches.push(
                                RuleMatch::new(
                                    format!("{short_msg} {MSG_BEGINNING_THESAURUS}"),
                                    start,
                                    end - start,
                                    rule,
                                    RuleMatchContext::new(
                                        context_text,
                                        ctx_start,
                                        ctx_end - ctx_start,
                                    ),
                                )
                                .with_short_message(short_msg.to_string())
                                .with_sentence(sentence.text().to_string()),
                            );
                        }
                    }
                }
            }
            before_last_token = last_token.clone();
            last_token = token.to_string();
            let trimmed = sentence.text().trim();
            prev_sentence_ends_with_punct =
                trimmed.len() >= 2 && trimmed.ends_with(['.', '?', '!']);
        }
        matches
    }
}

pub struct PersianSimpleReplaceRule {
    id: String,
    description: String,
    short: String,
    category: Category,
    wrong_words: HashMap<String, Vec<String>>,
}

impl PersianSimpleReplaceRule {
    pub fn new() -> Self {
        let mut wrong_words = HashMap::new();
        Self::load_data(
            include_str!(
                "../../../../languagetool/languagetool-language-modules/fa/src/main/resources/org/languagetool/rules/fa/replace.txt"
            ),
            &mut wrong_words,
        );
        Self {
            id: "FA_SIMPLE_REPLACE".to_string(),
            description: "اشتباه محتمل املائی".to_string(),
            short: "اشتباه محتمل املائی".to_string(),
            category: Category::new("CONFUSED_WORDS", "Commonly confused words"),
            wrong_words,
        }
    }

    fn load_data(data: &str, map: &mut HashMap<String, Vec<String>>) {
        for line in data.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(2, '=');
            let (Some(wrong_side), Some(replacement_side)) = (parts.next(), parts.next()) else {
                continue;
            };
            if replacement_side.trim().is_empty() {
                continue;
            }
            let replacements: Vec<String> =
                replacement_side.split('|').map(str::to_string).collect();
            for wrong_form in wrong_side.split('|') {
                map.insert(wrong_form.to_string(), replacements.clone());
            }
        }
    }

    fn message(&self, replacements: &[String]) -> String {
        format!("اشتباه محتمل املائی پیداشده: {}.", replacements.join("، "))
    }
}

impl Default for PersianSimpleReplaceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for PersianSimpleReplaceRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        for token_reading in &tokens {
            if token_reading.is_sentence_start()
                || token_reading.is_immunized()
                || token_reading.is_ignored_by_speller()
            {
                continue;
            }
            let original = token_reading.token().token();
            let possible = self
                .wrong_words
                .get(original)
                .or_else(|| self.wrong_words.get(&original.to_lowercase()));
            let Some(possible) = possible else {
                continue;
            };
            let mut replacements: Vec<String> = possible.to_vec();
            replacements.retain(|r| r != original);
            if replacements.is_empty() {
                continue;
            }
            let from = token_reading.token().start();
            let to = token_reading.token().end();
            let (context_text, ctx_start, ctx_end) = local_context(sentence, from, to, 40);
            let rule = RuleMatchRule::new(&self.id, &self.description)
                .with_category(self.category.clone())
                .with_issue_type(IssueType::Misspelling.as_str());
            matches.push(
                RuleMatch::new(
                    self.message(&replacements),
                    from,
                    to - from,
                    rule,
                    RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                )
                .with_short_message(self.short.clone())
                .with_replacements(replacements.iter().map(SuggestedReplacement::new).collect())
                .with_sentence(sentence.text().to_string()),
            );
        }
        matches
    }
}

const CONJUNCTIONS: [&str; 8] = ["و", "به", "با", "تا", "زیرا", "چون", "بنابراین", "چونکه"];

pub struct PersianSpaceBeforeRule {
    id: String,
    description: String,
    short: String,
    suggestion: String,
    category: Category,
}

impl PersianSpaceBeforeRule {
    pub fn new() -> Self {
        Self {
            id: "FA_SPACE_BEFORE_CONJUNCTION".to_string(),
            description: "بررسی\u{200c}کردن فاصله قبل از حرف ربط".to_string(),
            short: "فاصلهٔ حذف\u{200c}شده".to_string(),
            suggestion: "فاصلهٔ قبل از حرف ربط حذف شده\u{200c}است".to_string(),
            category: Category::new("MISC", "متفرقه"),
        }
    }
}

impl Default for PersianSpaceBeforeRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for PersianSpaceBeforeRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.tokens();
        let mut matches = Vec::new();
        for i in 1..tokens.len() {
            let token = tokens[i].token().token();
            if !CONJUNCTIONS.contains(&token) {
                continue;
            }
            let previous = tokens[i - 1].token().token();
            if previous == " " || previous == "(" {
                continue;
            }
            let pos = tokens[i].token().start();
            let end = tokens[i].token().end();
            let (context_text, ctx_start, ctx_end) = local_context(sentence, pos, end, 30);
            let rule = RuleMatchRule::new(&self.id, &self.description)
                .with_category(self.category.clone());
            matches.push(
                RuleMatch::new(
                    self.suggestion.clone(),
                    pos,
                    end - pos,
                    rule,
                    RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                )
                .with_short_message(self.short.clone())
                .with_replacements(vec![SuggestedReplacement::new(format!(" {token}"))])
                .with_sentence(sentence.text().to_string()),
            );
        }
        matches
    }
}

pub struct PersianWordCoherencyRule {
    id: String,
    description: String,
    category: Category,
    word_map: HashMap<String, Vec<String>>,
}

impl PersianWordCoherencyRule {
    pub fn new() -> Self {
        let mut word_map: HashMap<String, Vec<String>> = HashMap::new();
        for line in include_str!(
            "../../../../languagetool/languagetool-language-modules/fa/src/main/resources/org/languagetool/rules/fa/coherency.txt"
        )
        .lines()
        {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(2, ';');
            let (Some(first), Some(second)) = (parts.next(), parts.next()) else {
                continue;
            };
            word_map
                .entry(first.to_string())
                .or_default()
                .push(second.to_string());
            word_map
                .entry(second.to_string())
                .or_default()
                .push(first.to_string());
        }
        Self {
            id: "FA_WORD_COHERENCY".to_string(),
            description: "چند املا برای یک کلمه که یکی از آنها اولویت بیشتری دارد".to_string(),
            category: Category::new("MISC", "متفرقه"),
            word_map,
        }
    }

    fn message(&self, word1: &str, word2: &str) -> String {
        format!("'{word1}' و '{word2}' نباید در یک جا استفاده شوند")
    }
}

impl Default for PersianWordCoherencyRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for PersianWordCoherencyRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let mut should_not_appear: HashMap<&str, &str> = HashMap::new();
        for sentence in sentences {
            for token_reading in sentence.non_whitespace_tokens() {
                let surface = token_reading.token().token();
                let key = token_reading
                    .readings()
                    .first()
                    .and_then(|r| r.lemma())
                    .unwrap_or(surface);
                let Some(&other_spelling) = should_not_appear.get(key) else {
                    if let Some(should_not_set) = self.word_map.get(key) {
                        for should_not in should_not_set {
                            should_not_appear.insert(should_not.as_str(), key);
                        }
                    }
                    continue;
                };
                let marked = surface;
                let replacement = marked.replacen(key, other_spelling, 1);
                if eq_ignore_case(marked, &replacement) {
                    continue;
                }
                let from = token_reading.token().start();
                let to = token_reading.token().end();
                let (context_text, ctx_start, ctx_end) = local_context(sentence, from, to, 30);
                let rule = RuleMatchRule::new(&self.id, &self.description)
                    .with_category(self.category.clone());
                matches.push(
                    RuleMatch::new(
                        self.message(key, other_spelling),
                        from,
                        to - from,
                        rule,
                        RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                    )
                    .with_replacements(vec![SuggestedReplacement::new(&replacement)])
                    .with_sentence(sentence.text().to_string()),
                );
                break;
            }
        }
        matches
    }
}

pub struct PersianNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn persian_native_rules() -> PersianNativeRules {
    PersianNativeRules {
        rules: vec![
            Arc::new(PersianCommaWhitespaceRule::new()),
            Arc::new(persian_double_punctuation_rule()),
            Arc::new(PersianWordRepeatRule::new()),
            Arc::new(PersianSimpleReplaceRule::new()),
            Arc::new(PersianSpaceBeforeRule::new()),
        ],
        text_level_rules: vec![
            Arc::new(PersianWordRepeatBeginningRule::new()),
            Arc::new(PersianWordCoherencyRule::new()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::example_suite::fa_pipeline;
    use super::*;

    fn repeat_matches(text: &str) -> Vec<(String, String)> {
        let sentence = fa_pipeline().analyze_whole(text);
        PersianWordRepeatRule::new()
            .match_sentence(&sentence)
            .into_iter()
            .map(|m| {
                (
                    sentence.text()[m.offset()..m.offset() + m.length()].to_string(),
                    m.replacements()
                        .first()
                        .map(|r| r.value().to_string())
                        .unwrap_or_default(),
                )
            })
            .collect()
    }

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = persian_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "PERSIAN_COMMA_PARENTHESIS_WHITESPACE",
                "PERSIAN_DOUBLE_PUNCTUATION",
                "PERSIAN_WORD_REPEAT_RULE",
                "FA_SIMPLE_REPLACE",
                "FA_SPACE_BEFORE_CONJUNCTION",
            ],
            "the Persian-specific half of Persian.java:88-102 getRelevantRules order, next to \
             the generic CommaWhitespace/DoublePunctuation/MultipleWhitespace/LongSentence(50) \
             the shared engine path already installs"
        );
        let text_level_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            text_level_ids,
            vec!["PERSIAN_WORD_REPEAT_BEGINNING_RULE", "FA_WORD_COHERENCY"]
        );
    }

    #[test]
    fn default_off_rules_stay_off() {
        assert!(!PersianCommaWhitespaceRule::new().is_default_on());
        assert!(!PersianSpaceBeforeRule::new().is_default_on());
        assert!(persian_double_punctuation_rule().is_default_on());
        assert!(PersianWordRepeatRule::new().is_default_on());
    }

    #[test]
    fn word_repeat_rule_pins_the_java_example_pair() {
        assert_eq!(
            repeat_matches("این کار برای برای تو بود."),
            vec![("برای برای".to_string(), "برای".to_string())],
            "the span covers both tokens and the suggestion is the single word \
             (WordRepeatRule.java:118-128, example from PersianWordRepeatRule.java:50-51)"
        );
    }

    #[test]
    fn word_repeat_rule_ignores_the_persian_reduplications() {
        for text in [
            "من لی لی را دیدم",
            "آب درون بطری ۱۰۰ سی سی بود",
            "لک لک یک پرنده است",
            "غذایش را ریز ریز کرد",
            "جز جز این کتاب را بلدم",
            "با من کل کل نکن",
        ] {
            assert!(
                repeat_matches(text).is_empty(),
                "'{text}' must not fire: the ignore() override lists it"
            );
        }
    }

    #[test]
    fn double_punctuation_flags_the_persian_comma_pair() {
        let sentence = fa_pipeline().analyze_whole("این است،، واقعاً.");
        let matches = persian_double_punctuation_rule().match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), sentence.text().find("،،").unwrap());
        assert_eq!(matches[0].length(), "،،".len());
        assert_eq!(
            matches[0].replacements()[0].value(),
            "\u{060C}",
            "DoublePunctuationRule suggests the single configured comma character"
        );
    }

    #[test]
    fn word_repeat_beginning_pins_the_java_example_pair() {
        let rule = PersianWordRepeatBeginningRule::new();
        let text = "همچنین، خیابان تقریباً کاملاً مسکونی است. همچنین، به افتخار یک شاعر \
                    نامگذاری شده\u{200c}است.";
        let sentences = fa_pipeline().analyze_text(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            &text[matches[0].offset()..matches[0].offset() + matches[0].length()],
            "همچنین",
            "the second sentence-starting adverb is flagged alone \
             (PersianWordRepeatBeginningRule.java:53-56 example pair)"
        );
        assert_eq!(
            matches[0].short_message().unwrap_or_default(),
            "دو جملهٔ پست سر هم با حرف ربط مشابه شروع شده\u{200c}اند",
            "همچنین is in the Persian adverb set, so the two-sentence adverb message applies"
        );
        assert!(
            matches[0].replacements().is_empty(),
            "the base WordRepeatBeginningRule contributes no suggestions and the Persian \
             subclass does not override getSuggestions"
        );
    }

    #[test]
    fn word_repeat_beginning_needs_three_sentences_for_plain_words() {
        let rule = PersianWordRepeatBeginningRule::new();
        let two = "او آمد. او رفت.";
        let sentences = fa_pipeline().analyze_text(two);
        assert!(
            rule.match_text(two, &sentences).is_empty(),
            "two sentences starting with the non-adverb 'او' stay silent"
        );
        let three = "او آمد. او رفت. او خوابید.";
        let sentences = fa_pipeline().analyze_text(three);
        let matches = rule.match_text(three, &sentences);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].short_message().unwrap_or_default(),
            "سه جملهٔ پشت سر هم با کلمه\u{200c}ای مشابه شروع شده\u{200c}اند"
        );
    }

    #[test]
    fn space_before_rule_pins_the_java_test_matrix() {
        let rule = PersianSpaceBeforeRule::new();
        let count = |text: &str| {
            rule.match_sentence(&fa_pipeline().analyze_whole(text))
                .len()
        };
        assert_eq!(count("به اینجا"), 1);
        assert_eq!(count("من به اینجا"), 0);
        assert_eq!(count("(به اینجا"), 0);
    }

    #[test]
    fn simple_replace_rule_pins_the_java_example_pair() {
        let rule = PersianSimpleReplaceRule::new();
        let sentence = fa_pipeline().analyze_whole("وی حاظر به همکاری شد.");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            &sentence.text()[matches[0].offset()..matches[0].offset() + matches[0].length()],
            "حاظر"
        );
        assert_eq!(matches[0].replacements()[0].value(), "حاضر");
        assert_eq!(
            matches[0].message(),
            "اشتباه محتمل املائی پیداشده: حاضر.",
            "SimpleReplaceRule.java:64-66 joins the replacements with '، ' but the \
             single-replacement case carries no separator"
        );
        assert_eq!(
            matches[0].rule().category().name(),
            "Commonly confused words"
        );
    }

    #[test]
    fn word_coherency_rule_pins_the_java_test() {
        let rule = PersianWordCoherencyRule::new();
        let single = "این یک اتاق است.";
        let sentences = fa_pipeline().analyze_text(single);
        assert!(rule.match_text(single, &sentences).is_empty());
        let text = "این یک اتاق است. این یک اطاق است.";
        let sentences = fa_pipeline().analyze_text(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1, "the second spelling must be flagged");
        assert_eq!(
            &text[matches[0].offset()..matches[0].offset() + matches[0].length()],
            "اطاق"
        );
        assert_eq!(matches[0].replacements()[0].value(), "اتاق");
    }

    #[test]
    fn word_coherency_reverse_direction_also_fires() {
        let rule = PersianWordCoherencyRule::new();
        let text = "این یک اطاق است. این یک اتاق است.";
        let sentences = fa_pipeline().analyze_text(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "اطاق");
    }
}
