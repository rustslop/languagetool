use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, TextLevelRule,
};
use regex::Regex;
use std::sync::LazyLock;

pub(crate) fn clean_token(token: &str) -> String {
    token.replace(['\u{00AD}', '\u{0301}'], "")
}

fn context_for(sentence: &AnalyzedSentence, start: usize, end: usize) -> RuleMatchContext {
    let text = sentence.text();
    let mut ctx_start = start.saturating_sub(40);
    while ctx_start > 0 && !text.is_char_boundary(ctx_start) {
        ctx_start -= 1;
    }
    let mut ctx_end = std::cmp::min(end + 40, text.len());
    while ctx_end < text.len() && !text.is_char_boundary(ctx_end) {
        ctx_end += 1;
    }
    RuleMatchContext::new(
        text[ctx_start..ctx_end].to_string(),
        ctx_start,
        ctx_end - ctx_start,
    )
}

fn non_ws_with_ws_before(sentence: &AnalyzedSentence) -> (Vec<&AnalyzedTokenReadings>, Vec<bool>) {
    let mut tokens = Vec::new();
    let mut ws_before = Vec::new();
    let mut prev_ws = false;
    for t in sentence.tokens() {
        if t.is_whitespace() {
            prev_ws = true;
            continue;
        }
        tokens.push(t);
        ws_before.push(prev_ws);
        prev_ws = false;
    }
    (tokens, ws_before)
}

struct MatchSpec<'a> {
    id: &'a str,
    description: &'a str,
    short: &'a str,
    message: String,
    category: Category,
    issue: IssueType,
    start: usize,
    end: usize,
    replacements: Vec<String>,
}

fn make_match(spec: MatchSpec<'_>, sentence: &AnalyzedSentence) -> RuleMatch {
    let MatchSpec {
        id,
        description,
        short,
        message,
        category,
        issue,
        start,
        end,
        replacements,
    } = spec;
    let rule = RuleMatchRule::new(id, description)
        .with_category(category)
        .with_issue_type(issue.as_str());
    RuleMatch::new(
        message,
        start,
        end - start,
        rule,
        context_for(sentence, start, end),
    )
    .with_short_message(short.to_string())
    .with_replacements(
        replacements
            .into_iter()
            .map(SuggestedReplacement::new)
            .collect(),
    )
    .with_sentence(sentence.text().to_string())
}

pub struct UkrainianTypographyRule;

impl UkrainianTypographyRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UkrainianTypographyRule {
    fn default() -> Self {
        Self::new()
    }
}

static SHORT_DASH_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?i)[а-яіїєґ']{2,}([\u{2013}\u{2014}][а-яіїєґ']{2,})+").unwrap());
static BAD_LATIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[ХІXIV]+[\u{2013}\u{2014}][ХІXIV]+").unwrap());
static ANY_CYRILLIC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(".*[а-яїієґА-ЯІЇЄҐ].*").unwrap());

impl Rule for UkrainianTypographyRule {
    fn id(&self) -> &str {
        "DASH"
    }

    fn description(&self) -> &str {
        "Коротка риска замість дефісу"
    }

    fn category(&self) -> Category {
        Category::new("TYPOGRAPHY", "Оформлення")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Typographical
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let (tokens, ws_before) = non_ws_with_ws_before(sentence);
        let dash_msg = "Риска всередині слова. Всередині слова вживайте дефіс, між словами виокремлюйте риску пробілами.";
        let is_number = |t: &AnalyzedTokenReadings| {
            t.readings()
                .iter()
                .any(|r| r.pos_tags().join("+").starts_with("number"))
        };
        for i in 1..tokens.len() {
            let token = tokens[i].token().token();
            let last_reading = tokens[i]
                .readings()
                .last()
                .map(|r| r.token().to_string())
                .unwrap_or_default();
            let short_dash_token =
                if last_reading.contains('\u{2013}') || last_reading.contains('\u{2014}') {
                    (SHORT_DASH_WORD.is_match(&last_reading) && !BAD_LATIN.is_match(&last_reading))
                        .then(|| last_reading.clone())
                } else {
                    None
                };
            if let Some(dashed) = short_dash_token {
                let replacements = vec![
                    dashed.replace(['\u{2013}', '\u{2014}'], "-"),
                    dashed.replace(['\u{2013}', '\u{2014}'], " \u{2014} "),
                ];
                rule_matches.push(make_match(
                    MatchSpec {
                        id: self.id(),
                        description: self.description(),
                        category: self.category(),
                        issue: self.issue_type(),
                        short: "Коротка риска",
                        message: dash_msg.to_string(),
                        start: tokens[i].token().start(),
                        end: tokens[i].token().end(),
                        replacements,
                    },
                    sentence,
                ));
                continue;
            }
            if token == "\u{2014}" || token == "\u{2013}" {
                let no_space_left = i > 1
                    && !ws_before[i]
                    && tokens[i - 1].token().token() != ","
                    && tokens[i - 1].token().token() != "«";
                let no_space_right = i < tokens.len() - 1
                    && !ws_before[i + 1]
                    && tokens[i + 1].token().token() != ">";
                if no_space_left || no_space_right {
                    if i > 1
                        && is_number(tokens[i - 1])
                        && i < tokens.len() - 1
                        && is_number(tokens[i + 1])
                    {
                        continue;
                    }
                    let mut replacements = Vec::new();
                    if i > 1
                        && i < tokens.len() - 1
                        && ANY_CYRILLIC.is_match(tokens[i - 1].token().token())
                        && ANY_CYRILLIC.is_match(tokens[i + 1].token().token())
                    {
                        replacements.push(format!(
                            "{}-{}",
                            tokens[i - 1].token().token(),
                            tokens[i + 1].token().token()
                        ));
                    }
                    let (start_pos, end_pos, repl);
                    if i > 1 {
                        repl = format!("{} ", tokens[i - 1].token().token());
                        start_pos = tokens[i - 1].token().start();
                    } else {
                        repl = String::new();
                        start_pos = tokens[i].token().start();
                    }
                    let mut repl = repl + "\u{2014}";
                    if i < tokens.len() - 1 {
                        repl += &format!(" {}", tokens[i + 1].token().token());
                        end_pos = tokens[i + 1].token().start();
                    } else {
                        end_pos = tokens[i].token().end();
                    }
                    replacements.push(repl);
                    rule_matches.push(make_match(
                        MatchSpec {
                            id: self.id(),
                            description: self.description(),
                            category: self.category(),
                            issue: self.issue_type(),
                            short: "Коротка риска",
                            message: dash_msg.to_string(),
                            start: start_pos,
                            end: end_pos,
                            replacements,
                        },
                        sentence,
                    ));
                }
            }
        }
        rule_matches
    }
}

pub struct UkrainianHiddenCharacterRule;

impl UkrainianHiddenCharacterRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UkrainianHiddenCharacterRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for UkrainianHiddenCharacterRule {
    fn id(&self) -> &str {
        "UK_HIDDEN_CHARS"
    }

    fn description(&self) -> &str {
        "Приховані символи: знак м’якого перенесення"
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Інше")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("miscellaneous".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        for token in sentence.non_whitespace_tokens() {
            let token_str = token.token().token();
            if token_str.contains('\u{00AD}') {
                let replacement = token_str.replace('\u{00AD}', "");
                let highlighted = token_str.replace('\u{00AD}', "-");
                let msg = format!(
                    "{token_str} містить невидимий знак м’якого перенесення: «{highlighted}», виправлення: {replacement}"
                );
                rule_matches.push(make_match(
                    MatchSpec {
                        id: self.id(),
                        description: self.description(),
                        category: self.category(),
                        issue: self.issue_type(),
                        short: "Приховані символи",
                        message: msg,
                        start: token.token().start(),
                        end: token.token().end(),
                        replacements: vec![replacement],
                    },
                    sentence,
                ));
            }
        }
        rule_matches
    }
}

static MIXED_ALPHABETS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        ".*([a-zA-ZïáÁéÉíÍḯḮóÓúýÝ]'?[а-яіїєґА-ЯІЇЄҐ]|[а-яіїєґА-ЯІЇЄҐ]'?[a-zA-ZïáÁéÉíÍḯḮóÓúýÝ]).*",
    )
    .unwrap()
});
static LIKELY_LATIN_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[XVIХІ]{2,8}(-[а-яіїє]{1,3})?").unwrap());
static LATIN_NUMBER_WITH_CYRILLICS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("(Х{1,3}І{1,3}|І{1,3}Х{1,3}|Х{2,3}|І{2,3})(-[а-яіїє]{1,4})?").unwrap()
});
static CYRILLIC_ONLY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(".*[бвгґдєжзийїлнпфцчшщьюяБГҐДЄЖЗИЙЇЛПФЦЧШЩЬЮЯ].*").unwrap());
static LATIN_ONLY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(".*[bdfghjlqrstvzDFGJLNQRSUVZ].*").unwrap());
static COMMON_CYR_LETTERS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[АВЕІКОРСТУХ]+").unwrap());
static CYRILLIC_FIRST_LETTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[а-яіїєґА-ЯІЇЄҐ].*").unwrap());

const TO_LAT_MAP: &[(char, char)] = &[
    ('а', 'a'),
    ('е', 'e'),
    ('і', 'i'),
    ('ї', 'ï'),
    ('к', 'k'),
    ('м', 'm'),
    ('о', 'o'),
    ('р', 'p'),
    ('с', 'c'),
    ('т', 't'),
    ('у', 'y'),
    ('х', 'x'),
    ('А', 'A'),
    ('В', 'B'),
    ('Е', 'E'),
    ('І', 'I'),
    ('К', 'K'),
    ('М', 'M'),
    ('Н', 'H'),
    ('О', 'O'),
    ('Р', 'P'),
    ('С', 'C'),
    ('Т', 'T'),
    ('У', 'Y'),
    ('Х', 'X'),
];

const UMLAUTS: &[(&str, &str)] = &[
    ("á", "а́"),
    ("Á", "А́"),
    ("é", "е́"),
    ("É", "Е́"),
    ("í", "і́"),
    ("Í", "І́"),
    ("ḯ", "ї́"),
    ("Ḯ", "Ї́"),
    ("ó", "о́"),
    ("Ó", "О́"),
    ("ú", "и́"),
    ("ý", "у́"),
    ("Ý", "У́"),
];

fn to_cyrillic(word: &str) -> String {
    let mut out = word.to_string();
    for (cyr, lat) in TO_LAT_MAP {
        out = out.replace(*lat, &cyr.to_string());
    }
    for (umlaut, composed) in UMLAUTS {
        out = out.replace(umlaut, composed);
    }
    out
}

fn to_latin(word: &str) -> String {
    let mut out = word.to_string();
    for (cyr, lat) in TO_LAT_MAP {
        out = out.replace(*cyr, &lat.to_string());
    }
    out
}

fn adjust_for_invalid_suffix(token: &str) -> String {
    if token.contains('-') {
        static SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new("-[а-яіїє]{1,4}").unwrap());
        SUFFIX.replace(token, "").to_string()
    } else {
        token.to_string()
    }
}

pub struct UkrainianMixedAlphabetsRule;

impl UkrainianMixedAlphabetsRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UkrainianMixedAlphabetsRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for UkrainianMixedAlphabetsRule {
    fn id(&self) -> &str {
        "UK_MIXED_ALPHABETS"
    }

    fn description(&self) -> &str {
        "Змішування кирилиці й латиниці"
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Інше")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("miscellaneous".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let tokens = sentence.non_whitespace_tokens();
        let token_str_at = |i: usize| -> String { clean_token(tokens[i].token().token()) };
        let push = |rule_matches: &mut Vec<RuleMatch>,
                    idx: usize,
                    replacements: Vec<String>,
                    msg: &str| {
            rule_matches.push(make_match(
                MatchSpec {
                    id: "UK_MIXED_ALPHABETS",
                    description: self.description(),
                    category: self.category(),
                    issue: self.issue_type(),
                    short: "Мішанина розкладок",
                    message: msg.to_string(),
                    start: tokens[idx].token().start(),
                    end: tokens[idx].token().end(),
                    replacements,
                },
                sentence,
            ));
        };
        let none_matches =
            |pred: &dyn Fn(&str) -> bool| tokens.iter().all(|t| !pred(t.token().token()));
        for i in 1..tokens.len() {
            let token_string = token_str_at(i);
            if i < tokens.len() - 1
                && (matches!(token_string.as_str(), "i" | "y" | "a")
                    || (token_string == "A" && i == 1))
                && CYRILLIC_FIRST_LETTER.is_match(tokens[i + 1].token().token())
                && none_matches(&|t| t == "x" || t == "b" || t == "B")
            {
                let msg = format!("Вжито латинську «{token_string}» замість кириличної");
                push(&mut rule_matches, i, vec![to_cyrillic(&token_string)], &msg);
            } else if token_string == "І"
                && i > 1
                && (lemma_is_capitalized(&clean_token(tokens[i - 1].token().token()))
                    || (tokens[i - 1].has_pos_tag_matching("prep.*")
                        && i < tokens.len() - 1
                        && !is_all_uppercase_uk(&clean_token(tokens[i + 1].token().token())))
                    || (i < tokens.len() - 1
                        && matches!(
                            clean_token(tokens[i + 1].token().token()).as_str(),
                            "ст." | "тис."
                        ))
                    || (i < tokens.len() - 1
                        && matches!(
                            clean_token(tokens[i + 1].token().token()).as_str(),
                            "квартал" | "півріччя" | "тисячоліття" | "половина"
                        ))
                    || (token_string == "І."
                        && i > 1
                        && tokens[i - 1].token().token() != "Тому"
                        && tokens[i - 1].token().token() != "Франко"
                        && tokens[i - 1].has_pos_tag_matching("(?!.*:abbr).*fname.*")))
            {
                let msg = "Вжито кириличну літеру замість латинської";
                push(&mut rule_matches, i, vec![to_latin(&token_string)], msg);
            } else if COMMON_CYR_LETTERS.is_match(&token_string) {
                let prev_lemma = tokens[i - 1]
                    .readings()
                    .first()
                    .and_then(|r| r.lemma().map(str::to_string));
                if prev_lemma
                    .as_deref()
                    .is_some_and(|l| l == "гепатит" || l == "група" || l == "турнір")
                {
                    let msg = "Вжито кириличну літеру замість латинської";
                    push(&mut rule_matches, i, vec![to_latin(&token_string)], msg);
                }
            }

            if token_string.chars().count() < 2 {
                if token_string == "°"
                    && i < tokens.len() - 1
                    && clean_token(tokens[i + 1].token().token()) == "С"
                {
                    let msg = "Вжито кириличну літеру замість латинської";
                    push(&mut rule_matches, i + 1, vec!["C".to_string()], msg);
                }
                continue;
            }

            if MIXED_ALPHABETS.is_match(&token_string) {
                let mut replacements = Vec::new();
                let mut msg = "Вжито кириличні й латинські літери в одному слові".to_string();
                if !LATIN_ONLY.is_match(&token_string)
                    && !LIKELY_LATIN_NUMBER.is_match(&token_string)
                {
                    replacements.push(to_cyrillic(&token_string));
                }
                if (token_string.chars().count() > 2 && !CYRILLIC_ONLY.is_match(&token_string))
                    || LIKELY_LATIN_NUMBER.is_match(&token_string)
                {
                    let converted = to_latin_left_only(&token_string);
                    replacements.push(adjust_for_invalid_suffix(&converted));
                    msg = "Вжито кириличні літери замість латинських".to_string();
                    if invalid_suffix_present(&token_string) {
                        msg += ". Також: до римських цифр букви не дописуються.";
                    }
                }
                if !replacements.is_empty() {
                    push(&mut rule_matches, i, replacements, &msg);
                }
            } else if LATIN_NUMBER_WITH_CYRILLICS.is_match(&token_string) {
                let converted = to_latin_left_only(&token_string);
                let replacements = vec![adjust_for_invalid_suffix(&converted)];
                let mut msg =
                    "Вжито кириличні літери замість латинських на позначення римської цифри"
                        .to_string();
                if invalid_suffix_present(&token_string) {
                    msg += ". Також: до римських цифр букви не дописуються.";
                }
                push(&mut rule_matches, i, replacements, &msg);
            }

            if (token_string.contains('\u{0306}') || token_string.contains('\u{0308}'))
                && (token_string.contains("и\u{0306}") || token_string.contains("і\u{0308}"))
            {
                let fix = token_string
                    .replace("и\u{0306}", "й")
                    .replace("і\u{0308}", "ї");
                let msg = "Вжито комбіновані символи замість українських літер";
                push(&mut rule_matches, i, vec![fix], msg);
            }
        }
        rule_matches
    }
}

fn invalid_suffix_present(token: &str) -> bool {
    if !token.contains('-') {
        return false;
    }
    static PAT: LazyLock<Regex> = LazyLock::new(|| Regex::new("[IVXІХ]+-[а-яіїє]{1,4}").unwrap());
    PAT.is_match(token)
}

fn to_latin_left_only(token: &str) -> String {
    match token.split_once('-') {
        Some((left, right)) => format!("{}-{}", to_latin(left), right),
        None => to_latin(token),
    }
}

pub struct UkrainianWordRepeatRule;

impl UkrainianWordRepeatRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UkrainianWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

static DATE_TIME_NUM: LazyLock<Regex> = LazyLock::new(|| Regex::new("date|time|number.*").unwrap());

fn is_word_token(token: &str) -> bool {
    if token.chars().all(|c| c.is_ascii_digit() || c == ' ') && !token.is_empty() {
        return false;
    }
    if token.chars().count() == 1 {
        return token.chars().next().is_some_and(char::is_alphabetic);
    }
    true
}

impl Rule for UkrainianWordRepeatRule {
    fn id(&self) -> &str {
        "UKRAINIAN_WORD_REPEAT_RULE"
    }

    fn description(&self) -> &str {
        "Повторення слів (напр., 'буде буде')"
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Інше")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("duplication".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        let mut prev_token = String::new();
        for i in 1..tokens.len() {
            let token = tokens[i].token().token();
            if tokens[i].is_immunized() {
                prev_token = String::new();
                continue;
            }
            if is_word_token(token)
                && !token.is_empty()
                && prev_token.to_lowercase() == token.to_lowercase()
                && !self.ignore(&tokens, i)
            {
                let mut msg = "Можлива механічна помилка: повторення слова".to_string();
                let mut replacements = vec![prev_token.clone()];
                if prev_token == "І" && token == "і" {
                    msg += " або, можливо, перша І має бути латинською.";
                    replacements.push("I і".to_string());
                }
                rule_matches.push(make_match(
                    MatchSpec {
                        id: self.id(),
                        description: self.description(),
                        category: self.category(),
                        issue: self.issue_type(),
                        short: "Повторення слів",
                        message: msg,
                        start: tokens[i - 1].token().start(),
                        end: tokens[i].token().start() + prev_token.len(),
                        replacements,
                    },
                    sentence,
                ));
            }
            prev_token = token.to_string();
        }
        rule_matches
    }
}

impl UkrainianWordRepeatRule {
    fn ignore(&self, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        let token = tokens[position].token().token();
        if position > 2
            && token == "добра"
            && tokens[position - 2]
                .token()
                .token()
                .eq_ignore_ascii_case("від")
        {
            return true;
        }
        if position > 1
            && token == "що"
            && tokens[position - 2]
                .token()
                .token()
                .eq_ignore_ascii_case("тому")
        {
            return true;
        }
        if position > 3
            && token == "ні"
            && tokens[position - 2].token().token() == ","
            && tokens[position - 3]
                .token()
                .token()
                .eq_ignore_ascii_case("так")
        {
            return true;
        }
        if token == "ст." {
            return true;
        }
        if matches!(token, "Джей" | "Бі" | "Сі" | "Ла") {
            return true;
        }
        if tokens[position]
            .readings()
            .iter()
            .any(|r| DATE_TIME_NUM.is_match(&r.pos_tags().join("+")))
        {
            return true;
        }
        for reading in tokens[position].readings() {
            let pos_tag = reading.pos_tags().join("+");
            if !pos_tag.is_empty() {
                let is_initial = pos_tag.contains("abbr")
                    || (reading.token().chars().count() == 1
                        && reading
                            .token()
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_uppercase())
                        && position < tokens.len() - 1
                        && tokens[position + 1].token().token() == ".");
                if !is_initial && pos_tag != "SENT_END" {
                    return false;
                }
            }
        }
        true
    }
}

pub struct UkrainianCommaWhitespaceRule;

impl UkrainianCommaWhitespaceRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UkrainianCommaWhitespaceRule {
    fn default() -> Self {
        Self::new()
    }
}

fn is_comma_whitespace_quote(str_: &str) -> bool {
    matches!(
        str_,
        "'" | "\"" | "\u{2019}" | "\u{201D}" | "\u{201C}" | "«" | "»"
    )
}

fn is_left_bracket(str_: &str) -> bool {
    str_ == "(" || str_ == "["
}

fn is_right_bracket(str_: &str) -> bool {
    str_ == ")" || str_ == "]"
}

fn contains_digit(str_: &str) -> bool {
    str_.chars().any(|c| c.is_ascii_digit())
}

impl UkrainianCommaWhitespaceRule {
    fn is_exception(&self, tokens: &[AnalyzedTokenReadings], token_idx: usize) -> bool {
        let token = tokens[token_idx].token().token();
        token == "\u{2014}" || token == "\u{2013}"
    }
}

impl Rule for UkrainianCommaWhitespaceRule {
    fn id(&self) -> &str {
        "COMMA_PARENTHESIS_WHITESPACE"
    }

    fn description(&self) -> &str {
        "Вживання пробілу перед комою та перед і після дужок"
    }

    fn category(&self) -> Category {
        Category::new("TYPOGRAPHY", "Оформлення")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Whitespace
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        static DOMAIN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new("(?i)(com|org|net|int|edu|gov|mil|[a-z]{2})").unwrap());
        static FILE_EXTENSION: LazyLock<Regex> =
            LazyLock::new(|| Regex::new("([a-z]{3,4}|[A-Z]{3,4}|ai|mp[34]|MP[34])(-.+)?").unwrap());
        static DIGIT_OR_DOT: LazyLock<Regex> = LazyLock::new(|| Regex::new("[0-9.]").unwrap());
        static SLASH_CMD: LazyLock<Regex> = LazyLock::new(|| Regex::new("[a-zA-Z]+").unwrap());

        let tokens = sentence.tokens();
        let mut rule_matches = Vec::new();
        let mut prev_token = String::new();
        let mut prev_prev_token = String::new();
        let mut prev_white = false;
        for i in 0..tokens.len() {
            let token = tokens[i].token().token();
            let is_whitespace = tokens[i].is_whitespace() && token != "\u{200B}";
            let mut two_suggestions = false;
            let mut msg: Option<&str> = None;
            let mut suggestion_text = String::new();
            if is_whitespace && is_left_bracket(&prev_token) {
                let is_exception = tokens
                    .get(i + 1)
                    .is_some_and(|t| prev_token == "[" && token == " " && t.token().token() == "]");
                if !is_exception {
                    msg = Some("Не ставте пробіл після лівої дужки");
                    suggestion_text = prev_token.clone();
                }
            } else if is_whitespace
                && is_comma_whitespace_quote(&prev_token)
                && prev_prev_token == " "
            {
                msg = Some("Не ставте пробіл з обох боків символу лапок.");
                suggestion_text = prev_token.clone();
                two_suggestions = true;
            } else if !is_whitespace
                && prev_token == ","
                && !is_comma_whitespace_quote(token)
                && token != "-"
                && token != ","
                && !contains_digit(&prev_prev_token)
                && !contains_digit(token)
                && prev_prev_token != ","
            {
                msg = Some("Вставте пробіл після коми.");
                suggestion_text = format!(", {token}");
            } else if prev_white {
                if is_right_bracket(token) {
                    let is_exception = token == "]" && prev_token == " " && prev_prev_token == "[";
                    if !is_exception {
                        msg = Some("Не ставте пробіл перед правою дужкою");
                        suggestion_text = token.to_string();
                    }
                } else if token == "," {
                    msg = Some("Поставте пробіл після коми, а не перед комою.");
                    suggestion_text = ",".to_string();
                    if tokens.get(i + 1).is_some_and(|t| !t.is_whitespace()) {
                        suggestion_text = ", ".to_string();
                    }
                } else if token == "." {
                    let next = tokens.get(i + 1).map(|t| t.token().token()).unwrap_or("");
                    let domain = DOMAIN.is_match(next);
                    let file_ext = FILE_EXTENSION.is_match(next);
                    let proper_noun = tokens[i].has_pos_tag("NNP")
                        || tokens[i].has_pos_tag("NNPS")
                        || tokens[i].has_pos_tag_matching("NP.*");
                    if !domain && !file_ext && !proper_noun {
                        msg = Some("Не ставте пробіл перед крапкою.");
                        suggestion_text = ".".to_string();
                        if DIGIT_OR_DOT.is_match(next)
                            || (tokens.get(i + 1).is_some_and(|t| t.token().token() == "/")
                                && tokens
                                    .get(i + 2)
                                    .is_some_and(|t| SLASH_CMD.is_match(t.token().token())))
                        {
                            msg = None;
                        }
                    }
                }
            }
            if let Some(message) = msg
                && !self.is_exception(sentence.tokens(), i)
                && !tokens[i].is_immunized()
            {
                let from_idx = if two_suggestions { i - 2 } else { i - 1 };
                let from_pos = tokens[from_idx].token().start();
                let to_pos = tokens[i].token().end();
                if !two_suggestions
                    && to_pos <= sentence.text().len()
                    && sentence.text().get(from_pos..to_pos) == Some(suggestion_text.as_str())
                {
                    prev_prev_token = prev_token.clone();
                    prev_token = token.to_string();
                    prev_white = is_whitespace;
                    continue;
                }
                let replacements = if two_suggestions {
                    vec![format!("{suggestion_text} "), format!(" {suggestion_text}")]
                } else {
                    vec![suggestion_text.clone()]
                };
                rule_matches.push(make_match(
                    MatchSpec {
                        id: self.id(),
                        description: self.description(),
                        category: self.category(),
                        issue: self.issue_type(),
                        short: self.description(),
                        message: message.to_string(),
                        start: from_pos,
                        end: to_pos,
                        replacements,
                    },
                    sentence,
                ));
            }
            prev_prev_token = prev_token.clone();
            prev_token = token.to_string();
            prev_white = is_whitespace;
        }
        rule_matches
    }
}

pub struct UkrainianUppercaseSentenceStartRule;

impl UkrainianUppercaseSentenceStartRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UkrainianUppercaseSentenceStartRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for UkrainianUppercaseSentenceStartRule {
    fn id(&self) -> &str {
        "UPPERCASE_SENTENCE_START"
    }

    fn description(&self) -> &str {
        "Перевіряє, чи речення починається з великої літери"
    }

    fn category(&self) -> Category {
        Category::new("CASING", "Великі літери")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Typographical
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        static NUMERALS_EN: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new("[a-z]|(m{0,4}(c[md]|d?c{0,3})(x[cl]|l?x{0,3})(i[xv]|v?i{0,3}))$").unwrap()
        });
        static CONTAINS_DIGIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r".*\d.*").unwrap());
        static ONLY_LOWERCASE_START: LazyLock<Regex> =
            LazyLock::new(|| Regex::new("[a-z][A-Z].*").unwrap());
        static DIGIT_DOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\. .*$").unwrap());
        static LINEBREAK_DIGIT_DOT: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^.*\n\d+\. .*$").unwrap());
        static CAMEL_CASE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new("^[a-z]+[A-Z][A-Za-z]+$").unwrap());
        const EXCEPTIONS: &[&str] = &[
            "n", "w", "x86", "\u{24d2}", "\u{f8}", "cc", "pH", "heylogin",
        ];

        let is_url = |token: &str| {
            const PROTOCOLS: &[&str] = &[
                "http", "https", "ws", "wss", "ftp", "ftps", "sftp", "file", "mailto", "tel",
                "sms", "git", "ssh", "data", "magnet", "smb", "slack", "spotify",
            ];
            PROTOCOLS
                .iter()
                .any(|p| token.starts_with(&format!("{p}://")))
                || token.starts_with("www.")
        };
        static E_MAIL: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap());
        let is_e_mail = |token: &str| E_MAIL.is_match(token);

        let mut rule_matches = Vec::new();
        if sentences.len() == 1 && sentences[0].tokens().len() == 2 {
            return rule_matches;
        }
        let mut last_paragraph_string = String::new();
        let mut is_prev_sentence_numbered_list = false;
        for sentence in sentences {
            let (tokens, _) = non_ws_with_ws_before(sentence);
            if tokens.len() < 2 {
                return rule_matches;
            }
            let mut match_token_pos = 1usize;
            let first_token = tokens[1].token().token();
            let mut second_token: Option<&str> = None;
            if tokens.len() >= 3
                && matches!(
                    first_token,
                    "\"" | "'"
                        | "\u{201e}"
                        | "\u{bb}"
                        | "\u{ab}"
                        | "\u{201c}"
                        | "\u{2018}"
                        | "\u{a1}"
                        | "\u{bf}"
                )
            {
                match_token_pos = 2;
                second_token = Some(tokens[2].token().token());
            }
            if match_token_pos == 1
                && tokens.len() > 2
                && clean_token(tokens[1].token().token()).chars().count() == 1
                && is_cyr_lower(
                    clean_token(tokens[1].token().token())
                        .chars()
                        .next()
                        .unwrap(),
                )
                && tokens.get(2).is_some_and(|t| t.token().token() == ")")
            {
                return rule_matches;
            }
            let check_token = second_token.unwrap_or(first_token);
            let mut last_token = tokens[tokens.len() - 1].token().token();
            if last_token.chars().count() == 1
                && matches!(
                    last_token,
                    " " | "'"
                        | "„"
                        | "«"
                        | "»"
                        | "\u{2018}"
                        | "\u{2019}"
                        | "\u{201c}"
                        | "\u{201d}"
                        | "\n"
                )
                && tokens.len() > 1
            {
                last_token = tokens[tokens.len() - 2].token().token();
            }
            let mut prevent_error = false;
            if last_paragraph_string == "," || last_paragraph_string == ";" {
                prevent_error = true;
            }
            if CONTAINS_DIGIT.is_match(tokens[match_token_pos].token().token()) {
                prevent_error = true;
            }
            let prev_sentence_ends = last_paragraph_string.chars().count() == 1
                && matches!(last_paragraph_string.as_str(), "." | "?" | "!" | "\u{2026}");
            if !prev_sentence_ends && !matches!(last_token, "." | "?" | "!" | "\u{2026}") {
                prevent_error = true;
            }
            if !sentence.text().replace('\u{00A0}', " ").trim().is_empty() {
                last_paragraph_string = last_token.to_string();
            }
            if match_token_pos + 1 < tokens.len()
                && NUMERALS_EN.is_match(tokens[match_token_pos].token().token())
                && matches!(tokens[match_token_pos + 1].token().token(), "." | ")")
            {
                prevent_error = true;
            }
            if is_prev_sentence_numbered_list
                || is_url(check_token)
                || is_e_mail(check_token)
                || tokens[1].is_immunized()
                || tokens[match_token_pos].has_pos_tag("_IS_URL")
            {
                prevent_error = true;
            }
            if !check_token.is_empty() {
                let first_char = check_token.chars().next().unwrap_or('\0');
                let upper: String = first_char.to_uppercase().collect();
                let capitalized = format!("{}{}", upper, &check_token[first_char.len_utf8()..]);
                if capitalized != check_token
                    && !prevent_error
                    && first_char.is_lowercase()
                    && !ONLY_LOWERCASE_START.is_match(check_token)
                    && !EXCEPTIONS.contains(&check_token)
                    && !CAMEL_CASE.is_match(check_token)
                {
                    let target = tokens[match_token_pos];
                    rule_matches.push(make_match(
                        MatchSpec {
                            id: self.id(),
                            description: self.description(),
                            category: self.category(),
                            issue: self.issue_type(),
                            short: "Великі літери",
                            message: "Це речення не починається з великої літери.".to_string(),
                            start: target.token().start(),
                            end: target.token().end(),
                            replacements: vec![capitalized],
                        },
                        sentence,
                    ));
                }
            }
            is_prev_sentence_numbered_list = DIGIT_DOT.is_match(sentence.text())
                || LINEBREAK_DIGIT_DOT.is_match(sentence.text());
        }
        rule_matches
    }
}

fn is_cyr_lower(c: char) -> bool {
    matches!(c, 'а'..='я' | 'ї' | 'є' | 'ґ' | 'і')
}

fn lemma_is_capitalized(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < 2 {
        return false;
    }
    let c0 = chars[0];
    if !c0.is_uppercase() {
        return false;
    }
    if c0.is_ascii_uppercase() && chars[1].is_lowercase() {
        return true;
    }
    let sz = chars.len();
    let mut prev_dash = false;
    for i in 1..sz {
        let ch = chars[i];
        if ch == '\u{00AD}' || ch == '\u{0301}' {
            continue;
        }
        let dash = ch == '-' || ch == '\u{2013}';
        if dash {
            if i == sz - 2 && chars[i + 1].is_ascii_digit() {
                return true;
            }
            prev_dash = true;
            continue;
        }
        if ch != '\'' && ch != '\u{0301}' && ch != '\u{00AD}' && (prev_dash != ch.is_uppercase()) {
            return false;
        }
        prev_dash = false;
    }
    true
}

fn is_all_uppercase_uk(word: &str) -> bool {
    word.chars().all(|c| !c.is_lowercase()) && word.chars().any(|c| c.is_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uk::word_tokenizer::UkrainianWordTokenizer;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
    use og_tokenizer::WordTokenizer as _;

    fn sent_start() -> AnalyzedTokenReadings {
        AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )
    }

    fn tokenized(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![sent_start()];
        tokens.extend(
            UkrainianWordTokenizer::new()
                .tokenize(text)
                .into_iter()
                .map(|t| {
                    AnalyzedTokenReadings::new(AnalyzedToken::new(t.text(), t.start(), t.end()))
                }),
        );
        sentence.set_tokens(tokens);
        sentence
    }

    fn tokenized_with_readings(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![sent_start()];
        for t in UkrainianWordTokenizer::new().tokenize(text) {
            let atr = AnalyzedTokenReadings::new(AnalyzedToken::new(t.text(), t.start(), t.end()));
            if !t.text().trim().is_empty() {
                let reading = AnalyzedToken::new(t.text(), t.start(), t.end())
                    .with_pos_tags(vec!["noun:inanim".to_string()]);
                tokens.push(atr.with_readings(vec![reading]));
            } else {
                tokens.push(atr);
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    fn plain_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![sent_start()];
        let mut pos = 0usize;
        for word in text.split_whitespace() {
            let start = pos + text[pos..].find(word).expect("word offset");
            let end = start + word.len();
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                word, start, end,
            )));
            pos = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    fn values(m: &RuleMatch) -> Vec<&str> {
        m.replacements().iter().map(|r| r.value()).collect()
    }

    #[test]
    fn java_pin_mixed_alphabets_correct_sentences() {
        let rule = UkrainianMixedAlphabetsRule::new();
        for text in [
            "сміття",
            "not mixed",
            "123454",
            "ЛЮДИНИ І НАЦІЇ",
            "ЗА І ПРОТИ",
        ] {
            assert_eq!(
                rule.match_sentence(&tokenized(text)).len(),
                0,
                "expected no matches for {text}"
            );
        }
    }

    #[test]
    fn java_pin_mixed_alphabets_single_word_mixes() {
        let rule = UkrainianMixedAlphabetsRule::new();
        let matches = rule.match_sentence(&tokenized("смiття"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Вжито кириличні й латинські літери в одному слові"
        );
        assert_eq!(values(&matches[0]), vec!["сміття"]);
        let matches = rule.match_sentence(&tokenized("mіхed"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Вжито кириличні літери замість латинських"
        );
        assert_eq!(values(&matches[0]), vec!["mixed"]);
        let matches = rule.match_sentence(&tokenized("горíти"));
        assert_eq!(matches.len(), 1);
        assert_eq!(values(&matches[0]), vec!["горі́ти"]);
        let matches = rule.match_sentence(&tokenized("австрo-турецької"));
        assert_eq!(matches.len(), 1);
        assert_eq!(values(&matches[0]), vec!["австро-турецької"]);
    }

    #[test]
    fn java_pin_mixed_alphabets_latin_letters_for_cyrillic() {
        let rule = UkrainianMixedAlphabetsRule::new();
        let matches = rule.match_sentence(&tokenized("Чорного i Азовського"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Вжито латинську «i» замість кириличної"
        );
        assert_eq!(values(&matches[0]), vec!["і"]);
        let matches = rule.match_sentence(&tokenized("A нема"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Вжито латинську «A» замість кириличної"
        );
        assert_eq!(values(&matches[0]), vec!["А"]);
        let matches = rule.match_sentence(&tokenized("Петро І"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Вжито кириличну літеру замість латинської"
        );
        assert_eq!(values(&matches[0]), vec!["I"]);
    }

    #[test]
    fn java_pin_mixed_alphabets_roman_numerals() {
        let rule = UkrainianMixedAlphabetsRule::new();
        let matches = rule.match_sentence(&tokenized("XІ"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Вжито кириличні літери замість латинських"
        );
        assert_eq!(values(&matches[0]), vec!["XI"]);
        let matches = rule.match_sentence(&tokenized("ХI"));
        assert_eq!(values(&matches[0]), vec!["XI"]);
        let matches = rule.match_sentence(&tokenized("VIIІ-го"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Вжито кириличні літери замість латинських. Також: до римських цифр букви не дописуються."
        );
        assert_eq!(values(&matches[0]), vec!["VIII"]);
        let matches = rule.match_sentence(&tokenized("ІІІ-го"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Вжито кириличні літери замість латинських на позначення римської цифри. Також: до римських цифр букви не дописуються."
        );
        assert_eq!(values(&matches[0]), vec!["III"]);
        let matches = rule.match_sentence(&tokenized("ХІ"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Вжито кириличні літери замість латинських на позначення римської цифри"
        );
        assert_eq!(values(&matches[0]), vec!["XI"]);
        let matches = rule.match_sentence(&tokenized("СOVID-19"));
        assert_eq!(values(&matches[0]), vec!["COVID-19"]);
    }

    #[test]
    fn java_pin_mixed_alphabets_degree_sign() {
        let matches = UkrainianMixedAlphabetsRule::new().match_sentence(&tokenized("На 0,6°С."));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "Вжито кириличну літеру замість латинської"
        );
        assert_eq!(values(&matches[0]), vec!["C"]);
    }

    #[test]
    fn java_pin_typography_rule() {
        let rule = UkrainianTypographyRule::new();
        assert_eq!(rule.match_sentence(&tokenized("як-небудь")).len(), 0);
        assert_eq!(rule.match_sentence(&tokenized("Вовка,— волкова")).len(), 0);
        assert_eq!(
            rule.match_sentence(&tokenized("\u{2014} Київ, 1994")).len(),
            0
        );
        let matches = rule.match_sentence(&tokenized("яскраво\u{2013}рожевий."));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            values(&matches[0]),
            vec!["яскраво-рожевий", "яскраво \u{2014} рожевий"]
        );
        let matches = rule.match_sentence(&tokenized("Вовка\u{2014}Волкова."));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            values(&matches[0]),
            vec!["Вовка-Волкова", "Вовка \u{2014} Волкова"]
        );
        let matches = rule.match_sentence(&tokenized("цукерок \u{2014}знову низька"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            values(&matches[0]),
            vec!["цукерок-знову", "цукерок \u{2014} знову"]
        );
        let matches = rule.match_sentence(&tokenized("\u{2014}знову низька"));
        assert_eq!(matches.len(), 1);
        assert_eq!(values(&matches[0]), vec!["\u{2014} знову"]);
        let matches = rule.match_sentence(&tokenized("знову\u{2014}"));
        assert_eq!(matches.len(), 1);
        assert_eq!(values(&matches[0]), vec!["знову \u{2014}"]);
        let matches = rule.match_sentence(&tokenized("важливіше \u{2013}потенційні"));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            values(&matches[0]),
            vec!["важливіше-потенційні", "важливіше \u{2014} потенційні"]
        );
    }

    #[test]
    fn hidden_character_rule_strips_the_soft_hyphen() {
        let text = "слово\u{00AD}м у реченні";
        let matches = UkrainianHiddenCharacterRule::new().match_sentence(&plain_sentence(text));
        assert_eq!(matches.len(), 1);
        assert_eq!(values(&matches[0]), vec!["словом"]);
        assert!(
            matches[0]
                .message()
                .contains("невидимий знак м’якого перенесення")
        );
        assert_eq!(rule_matches_text(&matches[0], text), "слово\u{00AD}м");
    }

    fn rule_matches_text<'a>(m: &RuleMatch, text: &'a str) -> &'a str {
        &text[m.offset()..m.offset() + m.length()]
    }

    #[test]
    fn java_pin_word_repeat_ignored_cases() {
        let rule = UkrainianWordRepeatRule::new();
        for text in [
            "без повного розрахунку",
            "без бугіма бугіма",
            "без 100 100",
            "1.30 3.20 3.20",
            "ще в В.Кандинського",
            "Від добра добра не шукають.",
            "Що, що, а кіно в Україні...",
            "Відповідно до ст. ст. 3, 7, 18.",
            "Не можу сказати ні так, ні ні.",
        ] {
            assert_eq!(
                rule.match_sentence(&tokenized(text)).len(),
                0,
                "expected no matches for {text}"
            );
        }
    }

    #[test]
    fn java_pin_word_repeat_flagged_cases() {
        let rule = UkrainianWordRepeatRule::new();
        let matches = rule.match_sentence(&tokenized_with_readings("без без повного розрахунку"));
        assert_eq!(matches.len(), 1);
        assert_eq!(values(&matches[0]), vec!["без"]);
        let matches =
            rule.match_sentence(&tokenized_with_readings("Верховної Ради І і ІІ скликань"));
        assert_eq!(matches.len(), 1);
        assert_eq!(values(&matches[0]), vec!["І", "I і"]);
        assert_eq!(matches[0].rule().id(), "UKRAINIAN_WORD_REPEAT_RULE");
    }

    #[test]
    fn java_pin_uppercase_sentence_start() {
        let rule = UkrainianUppercaseSentenceStartRule::new();
        let sentences = vec![tokenized("Автор написав це речення з великої літери.")];
        assert_eq!(rule.match_text("", &sentences).len(), 0);
        let sentences = vec![tokenized("Він приїхав в с. Вижівка.")];
        assert_eq!(rule.match_text("", &sentences).len(), 0);
        let sentences = vec![tokenized("автор написав це речення з маленької літери.")];
        let matches = rule.match_text("", &sentences);
        assert_eq!(matches.len(), 1);
        assert_eq!(values(&matches[0]), vec!["Автор"]);
        assert_eq!(matches[0].rule().id(), "UPPERCASE_SENTENCE_START");
    }
}
