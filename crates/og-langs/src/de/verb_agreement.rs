use super::agreement::{SYNTH_REMOVE, immunized_indices, levenshtein, starts_with_lowercase};
use super::helpers::{starts_with_uppercase, uppercase_first_char};
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::TextLevelRule,
};
use og_tagger::morfologik::MorfologikDictionary;
use std::collections::HashSet;
use std::sync::{Arc, LazyLock};

pub static VERB_AGREEMENT_ANTI_PATTERNS: LazyLock<Option<Vec<crate::engine::XmlPatternRule>>> =
    LazyLock::new(|| {
        let compiler = og_xml::XmlCompiler::new();
        let src = include_str!("verb_agreement_antipatterns.xml");
        match compiler.compile_file(src) {
            Ok(set) => Some(
                set.rules
                    .into_iter()
                    .map(crate::engine::XmlPatternRule::new)
                    .collect(),
            ),
            Err(_) => None,
        }
    });

const BIN_IGNORE: &[&str] = &[
    "Suleiman", "Mohamed", "Muhammad", "Muhammed", "Mohammed", "Mohammad", "Mansour", "Qaboos",
    "Qabus", "Tamim", "Majid", "Salman", "Ghazi", "Mahathir", "Madschid", "Maktum", "al-Aziz",
    "Asis", "Numan", "Hussein", "Abdul", "Abdulla", "Abdullah", "Isa", "Osama", "Said", "Zayid",
    "Zayed", "Hamad", "Chalifa", "Raschid", "Turki", "/",
];

const CONJUNCTIONS: &[&str] = &["weil", "obwohl", "dass", "indem", "sodass"];
const QUOTATION_MARKS: [&str; 2] = ["\"", "„"];

static VER_1_SIN: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"VER.*:1:SIN.*").unwrap());
static VER_2_SIN: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"VER.*:2:SIN.*").unwrap());
static VER_3_SIN: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"VER.*:3:SIN.*").unwrap());
static VER_1_PLU: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"VER.*:1:PLU.*").unwrap());

fn pos_regex(expected_verb_pos: &str) -> &'static regex::Regex {
    match expected_verb_pos {
        "1:SIN" => &VER_1_SIN,
        "2:SIN" => &VER_2_SIN,
        "3:SIN" => &VER_3_SIN,
        _ => &VER_1_PLU,
    }
}

fn has_partial_pos_tag(token: &AnalyzedTokenReadings, needle: &str) -> bool {
    token
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.contains(needle)))
}

fn has_any_partial_pos_tag(token: &AnalyzedTokenReadings, tags: &[&str]) -> bool {
    tags.iter().any(|t| has_partial_pos_tag(token, t))
}

fn is_quotation_mark(token: &AnalyzedTokenReadings) -> bool {
    QUOTATION_MARKS.contains(&token.token().token())
}

fn is_near(a: isize, b: isize) -> bool {
    a != -1 && (a - b).abs() < 5
}

fn next_but_one_is_modal(tokens: &[&AnalyzedTokenReadings], pos: usize) -> bool {
    pos + 2 < tokens.len() && has_partial_pos_tag(tokens[pos + 2], ":MOD:")
}

fn has_unambiguously_person_and_number(
    token: &AnalyzedTokenReadings,
    base: usize,
    person: &str,
    number: &str,
) -> bool {
    let text = token.token().token();
    if text.is_empty()
        || (text.chars().next().is_some_and(char::is_uppercase) && token.token().start() != base)
        || !super::helpers::has_pos_tag_starting_with(token, "VER")
    {
        return false;
    }
    let needle = format!(":{person}:{number}");
    token
        .readings()
        .iter()
        .filter(|r| r.pos_tags().iter().any(|t| !t.ends_with("_END")))
        .all(|r| r.pos_tags().iter().any(|t| t.contains(&needle)))
}

fn is_finite_verb(token: &AnalyzedTokenReadings, base: usize) -> bool {
    let text = token.token().token();
    if text.is_empty()
        || (text.chars().next().is_some_and(char::is_uppercase) && token.token().start() != base)
        || !super::helpers::has_pos_tag_starting_with(token, "VER")
        || has_any_partial_pos_tag(token, &["PA2", "PRO:", "ZAL"])
        || text == "einst"
    {
        return false;
    }
    has_any_partial_pos_tag(token, &[":1:", ":2:", ":3:"])
}

struct PersonNumberCheck<'a> {
    verb_does_match: bool,
    finite_verb: Option<&'a AnalyzedTokenReadings>,
}

fn verb_does_match_person_and_number<'a>(
    token1: &'a AnalyzedTokenReadings,
    token2: &'a AnalyzedTokenReadings,
    person: &str,
    number: &str,
    base: usize,
) -> PersonNumberCheck<'a> {
    let sep = [",", "und", "sowie", "&"];
    if sep.contains(&token1.token().token()) || sep.contains(&token2.token().token()) {
        return PersonNumberCheck {
            verb_does_match: true,
            finite_verb: None,
        };
    }
    let needle = format!(":{person}:{number}");
    let mut found_finite_verb = false;
    let mut finite_verb = None;
    if is_finite_verb(token1, base) {
        found_finite_verb = true;
        finite_verb = Some(token1);
        if has_partial_pos_tag(token1, &needle) {
            return PersonNumberCheck {
                verb_does_match: true,
                finite_verb,
            };
        }
    }
    if is_finite_verb(token2, base) {
        found_finite_verb = true;
        finite_verb = Some(token2);
        if has_partial_pos_tag(token2, &needle) {
            return PersonNumberCheck {
                verb_does_match: true,
                finite_verb,
            };
        }
    }
    PersonNumberCheck {
        verb_does_match: !found_finite_verb,
        finite_verb,
    }
}

fn get_pronoun_suggestions(verb: &AnalyzedTokenReadings, to_uppercase: bool) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    if has_partial_pos_tag(verb, ":1:SIN") {
        result.push("ich".to_string());
    }
    if has_partial_pos_tag(verb, ":2:SIN") {
        result.push("du".to_string());
    }
    if has_partial_pos_tag(verb, ":3:SIN") {
        result.push("er".to_string());
        result.push("sie".to_string());
        result.push("es".to_string());
    }
    if has_partial_pos_tag(verb, ":1:PLU") {
        result.push("wir".to_string());
    }
    if has_partial_pos_tag(verb, ":2:PLU") {
        result.push("ihr".to_string());
    }
    if has_partial_pos_tag(verb, ":3:PLU") && !result.iter().any(|s| s == "sie") {
        result.push("sie".to_string());
    }
    if to_uppercase {
        for s in &mut result {
            *s = uppercase_first_char(s);
        }
    }
    result
}

pub struct VerbAgreementRule {
    synth_dictionary: Option<Arc<MorfologikDictionary>>,
}

impl VerbAgreementRule {
    pub fn new(synth_dictionary: Option<Arc<MorfologikDictionary>>) -> Self {
        Self { synth_dictionary }
    }

    fn get_verb_suggestions(
        &self,
        verb: &AnalyzedTokenReadings,
        expected_verb_pos: &str,
        to_uppercase: bool,
    ) -> Vec<String> {
        let verb_lemma = verb
            .readings()
            .iter()
            .find(|r| r.pos_tags().iter().any(|t| t.starts_with("VER:")))
            .and_then(|r| r.lemma())
            .unwrap_or("");
        let mut suggestions: Vec<String> = Vec::new();
        if let Some(dict) = &self.synth_dictionary
            && let Some(entries) = dict.forms_by_lemma().get(verb_lemma)
        {
            let re = pos_regex(expected_verb_pos);
            let lc_lemma = !starts_with_uppercase(verb_lemma);
            for (tag, word) in entries {
                if !re.is_match(tag) {
                    continue;
                }
                if (starts_with_lowercase(word) == lc_lemma || verb_lemma == "mein")
                    && !SYNTH_REMOVE.contains(&&**word)
                    && !suggestions.iter().any(|s| s == &**word)
                {
                    suggestions.push(word.to_string());
                }
            }
        }
        if to_uppercase {
            for s in &mut suggestions {
                *s = uppercase_first_char(s);
            }
        }
        suggestions
    }

    fn rule_match_wrong_verb(
        &self,
        token: &AnalyzedTokenReadings,
        sentence: &AnalyzedSentence,
    ) -> RuleMatch {
        let msg = format!(
            "Möglicherweise fehlende grammatische Übereinstimmung zwischen Subjekt und Prädikat ({}) bezüglich Person oder Numerus (Einzahl, Mehrzahl - Beispiel: 'Max bist' statt 'Max ist').",
            token.token().token()
        );
        let from = token.token().start();
        let to = token.token().end();
        make_rule_match(msg, from, to, sentence)
    }

    #[allow(clippy::too_many_arguments)]
    fn rule_match_wrong_verb_subject(
        &self,
        subject: &AnalyzedTokenReadings,
        verb: &AnalyzedTokenReadings,
        expected_verb_pos: &str,
        text: &str,
        sentence: &AnalyzedSentence,
    ) -> RuleMatch {
        let msg = format!(
            "Möglicherweise fehlende grammatische Übereinstimmung zwischen Subjekt ({}) und Prädikat ({}) bezüglich Person oder Numerus (Einzahl, Mehrzahl - Beispiel: 'ich sind' statt 'ich bin').",
            subject.token().token(),
            verb.token().token()
        );
        let mut suggestions: Vec<String> = Vec::new();
        let (from, to, marked_text) = if subject.token().start() < verb.token().start() {
            let from = subject.token().start();
            let to = verb.token().end();
            for vs in self.get_verb_suggestions(verb, expected_verb_pos, false) {
                suggestions.push(format!("{} {vs}", subject.token().token()));
            }
            for ps in get_pronoun_suggestions(verb, starts_with_uppercase(subject.token().token()))
            {
                suggestions.push(format!("{ps} {}", verb.token().token()));
            }
            (from, to, text[from..to].to_string())
        } else {
            let from = verb.token().start();
            let to = subject.token().end();
            for vs in self.get_verb_suggestions(
                verb,
                expected_verb_pos,
                starts_with_uppercase(verb.token().token()),
            ) {
                suggestions.push(format!("{vs} {}", subject.token().token()));
            }
            for ps in get_pronoun_suggestions(verb, false) {
                suggestions.push(format!("{} {ps}", verb.token().token()));
            }
            (from, to, text[from..to].to_string())
        };
        suggestions.sort_by_key(|s| levenshtein(&marked_text, s));
        make_rule_match(msg, from, to, sentence).with_replacements(
            suggestions
                .into_iter()
                .map(SuggestedReplacement::new)
                .collect(),
        )
    }

    fn match_partial(
        &self,
        partial: &AnalyzedSentence,
        whole_sentence: &AnalyzedSentence,
        text: &str,
    ) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let tokens = partial.non_whitespace_tokens();
        if tokens.len() < 4 {
            return rule_matches;
        }
        let immunized: HashSet<usize> = VERB_AGREEMENT_ANTI_PATTERNS
            .as_ref()
            .map(|rules| immunized_indices(rules, partial))
            .unwrap_or_default();
        let base = whole_sentence.start();

        let mut pos_ich: isize = -1;
        let mut pos_du: isize = -1;
        let mut pos_er: isize = -1;
        let mut pos_wir: isize = -1;
        let mut pos_ver1_sin: isize = -1;
        let mut pos_ver2_sin: isize = -1;
        let mut pos_ver1_plu: isize = -1;
        let mut pos_possible_ver1_sin: isize = -1;
        let mut pos_possible_ver2_sin: isize = -1;
        let mut pos_possible_ver3_sin: isize = -1;
        let mut pos_possible_ver1_plu: isize = -1;

        for i in 1..tokens.len() {
            let token = tokens[i];
            let raw = token.token().token();
            let str_token: String = raw.to_lowercase().chars().filter(|c| *c != '‚').collect();
            match str_token.as_str() {
                "ich" => pos_ich = i as isize,
                "du" => pos_du = i as isize,
                "er" => pos_er = i as isize,
                "wir" => pos_wir = i as isize,
                _ => {}
            }

            if has_partial_pos_tag(token, "VER")
                && (raw.chars().next().is_some_and(char::is_lowercase)
                    || i == 1
                    || is_quotation_mark(tokens[i - 1]))
            {
                let prev_text = tokens[i - 1].token().token();
                let bin_guard = str_token != "bin"
                    || !(BIN_IGNORE.contains(&prev_text)
                        || (tokens.len() != i + 1
                            && tokens[i + 1].token().token().starts_with("Laden")));
                if has_unambiguously_person_and_number(token, base, "1", "SIN") && bin_guard {
                    pos_ver1_sin = i as isize;
                } else if has_unambiguously_person_and_number(token, base, "2", "SIN")
                    && raw != "Probst"
                {
                    pos_ver2_sin = i as isize;
                } else if has_unambiguously_person_and_number(token, base, "1", "PLU") {
                    pos_ver1_plu = i as isize;
                }

                if has_partial_pos_tag(token, ":1:SIN") {
                    pos_possible_ver1_sin = i as isize;
                }
                if has_partial_pos_tag(token, ":2:SIN") {
                    pos_possible_ver2_sin = i as isize;
                }
                if has_partial_pos_tag(token, ":3:SIN") {
                    pos_possible_ver3_sin = i as isize;
                }
                if has_partial_pos_tag(token, ":1:PLU") {
                    pos_possible_ver1_plu = i as isize;
                }
            }
        }

        let plus1 = |pos: usize| if pos + 1 == tokens.len() { 0 } else { 1 };

        if pos_ver1_sin != -1
            && pos_ich == -1
            && !is_quotation_mark(tokens[(pos_ver1_sin - 1) as usize])
            && !immunized.contains(&(pos_ver1_sin as usize))
        {
            rule_matches
                .push(self.rule_match_wrong_verb(tokens[pos_ver1_sin as usize], whole_sentence));
        } else if pos_ich > 0
            && !is_near(pos_possible_ver1_sin, pos_ich)
            && ({
                let idx = pos_ich as usize;
                let tok = tokens[idx];
                let rel_start = tok.token().start().saturating_sub(base);
                tok.token().token() == "ich"
                    || rel_start <= 1
                    || (tok.token().token() == "Ich"
                        && idx >= 2
                        && tokens[idx - 2].token().token() == ":")
                    || (tok.token().token() == "Ich"
                        && idx >= 1
                        && tokens[idx - 1].token().token() == ":")
            })
            && (!is_quotation_mark(tokens[pos_ich as usize - 1])
                || (pos_ich as usize) < 3
                || (pos_ich > 1 && tokens[pos_ich as usize - 2].token().token() == ":"))
        {
            let idx = pos_ich as usize;
            let p1 = plus1(idx);
            let check = verb_does_match_person_and_number(
                tokens[idx - 1],
                tokens[idx + p1],
                "1",
                "SIN",
                base,
            );
            if !check.verb_does_match
                && !next_but_one_is_modal(&tokens, idx)
                && !matches!(check.finite_verb, Some(v) if v.token().token() == "äußerst")
                && !immunized.contains(&idx)
                && let Some(finite_verb) = check.finite_verb
            {
                rule_matches.push(self.rule_match_wrong_verb_subject(
                    tokens[idx],
                    finite_verb,
                    "1:SIN",
                    text,
                    whole_sentence,
                ));
            }
        }

        if pos_ver2_sin != -1
            && pos_du == -1
            && !is_quotation_mark(tokens[(pos_ver2_sin - 1) as usize])
            && !immunized.contains(&(pos_ver2_sin as usize))
        {
            rule_matches
                .push(self.rule_match_wrong_verb(tokens[pos_ver2_sin as usize], whole_sentence));
        } else if pos_du > 0
            && !is_near(pos_possible_ver2_sin, pos_du)
            && (!is_quotation_mark(tokens[pos_du as usize - 1])
                || (pos_du as usize) < 3
                || (pos_du > 1 && tokens[pos_du as usize - 2].token().token() == ":"))
        {
            let idx = pos_du as usize;
            let p1 = plus1(idx);
            let check = verb_does_match_person_and_number(
                tokens[idx - 1],
                tokens[idx + p1],
                "2",
                "SIN",
                base,
            );
            if !check.verb_does_match
                && !super::helpers::has_pos_tag_starting_with(tokens[idx + p1], "VER:1:SIN:KJ2")
                && !({
                    let t = tokens[idx + p1];
                    super::helpers::has_pos_tag_starting_with(t, "ADJ:")
                        && !t.has_pos_tag("ADJ:PRD:GRU")
                })
                && !super::helpers::has_pos_tag_starting_with(tokens[idx - 1], "VER:1:SIN:KJ2")
                && !next_but_one_is_modal(&tokens, idx)
                && !immunized.contains(&idx)
                && let Some(finite_verb) = check.finite_verb
            {
                rule_matches.push(self.rule_match_wrong_verb_subject(
                    tokens[idx],
                    finite_verb,
                    "2:SIN",
                    text,
                    whole_sentence,
                ));
            }
        }

        if pos_er > 0
            && !is_near(pos_possible_ver3_sin, pos_er)
            && (!is_quotation_mark(tokens[pos_er as usize - 1])
                || (pos_er as usize) < 3
                || (pos_er > 1 && tokens[pos_er as usize - 2].token().token() == ":"))
        {
            let idx = pos_er as usize;
            let p1 = plus1(idx);
            let check = verb_does_match_person_and_number(
                tokens[idx - 1],
                tokens[idx + p1],
                "3",
                "SIN",
                base,
            );
            if !check.verb_does_match
                && !next_but_one_is_modal(&tokens, idx)
                && !matches!(check.finite_verb, Some(v) if v.token().token() == "äußerst")
                && !matches!(check.finite_verb, Some(v) if v.token().token() == "regen")
                && !immunized.contains(&idx)
                && let Some(finite_verb) = check.finite_verb
            {
                rule_matches.push(self.rule_match_wrong_verb_subject(
                    tokens[idx],
                    finite_verb,
                    "3:SIN",
                    text,
                    whole_sentence,
                ));
            }
        }

        if pos_ver1_plu != -1
            && pos_wir == -1
            && !is_quotation_mark(tokens[(pos_ver1_plu - 1) as usize])
            && !immunized.contains(&(pos_ver1_plu as usize))
        {
            rule_matches
                .push(self.rule_match_wrong_verb(tokens[pos_ver1_plu as usize], whole_sentence));
        } else if pos_wir > 0
            && !is_near(pos_possible_ver1_plu, pos_wir)
            && !is_quotation_mark(tokens[pos_wir as usize - 1])
        {
            let idx = pos_wir as usize;
            let p1 = plus1(idx);
            let check = verb_does_match_person_and_number(
                tokens[idx - 1],
                tokens[idx + p1],
                "1",
                "PLU",
                base,
            );
            if !check.verb_does_match
                && !next_but_one_is_modal(&tokens, idx)
                && !immunized.contains(&idx)
                && !matches!(check.finite_verb, Some(v) if v.token().token() == "äußerst")
                && let Some(finite_verb) = check.finite_verb
            {
                rule_matches.push(self.rule_match_wrong_verb_subject(
                    tokens[idx],
                    finite_verb,
                    "1:PLU",
                    text,
                    whole_sentence,
                ));
            }
        }

        rule_matches
    }
}

fn make_rule_match(msg: String, from: usize, to: usize, sentence: &AnalyzedSentence) -> RuleMatch {
    let rm_rule = RuleMatchRule::new(
        "DE_VERBAGREEMENT",
        "Kongruenz von Subjekt und Prädikat (nur 1. u. 2. Person oder m. Personalpronomen), z.B. 'Er bist (ist)'",
    )
    .with_category(Category::new("GRAMMAR", "Grammatik"))
    .with_issue_type("grammar");
    RuleMatch::new(
        msg,
        from,
        to - from,
        rm_rule,
        RuleMatchContext::new(sentence.text().to_string(), from, to - from),
    )
    .with_sentence(sentence.text().to_string())
}

impl TextLevelRule for VerbAgreementRule {
    fn id(&self) -> &str {
        "DE_VERBAGREEMENT"
    }

    fn description(&self) -> &str {
        "Kongruenz von Subjekt und Prädikat (nur 1. u. 2. Person oder m. Personalpronomen), z.B. 'Er bist (ist)'"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("GRAMMAR", "Grammatik")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }

    fn match_text(&self, text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        for sentence in sentences {
            let tokens = sentence.tokens();
            let mut idx = 0usize;
            for i in 2..tokens.len() {
                if tokens[i - 2].token().token() == ","
                    && CONJUNCTIONS.contains(&tokens[i].token().token())
                {
                    let partial = partial_sentence(sentence, idx, i);
                    rule_matches.extend(self.match_partial(&partial, sentence, text));
                    idx = i;
                }
            }
            let partial = partial_sentence(sentence, idx, tokens.len());
            rule_matches.extend(self.match_partial(&partial, sentence, text));
        }
        rule_matches
    }
}

fn partial_sentence(sentence: &AnalyzedSentence, from: usize, to: usize) -> AnalyzedSentence {
    let mut partial = AnalyzedSentence::new(sentence.text(), sentence.start(), sentence.end());
    partial.set_tokens(sentence.tokens()[from..to].to_vec());
    partial
}

#[cfg(test)]
mod tests {
    use crate::engine::LanguageEngine;

    fn shared_engine() -> std::sync::Arc<LanguageEngine> {
        crate::engine::LanguageEngine::shared_for_code("de").expect("german engine")
    }

    fn va_matches(text: &str) -> Vec<(String, usize, Vec<String>)> {
        shared_engine()
            .check(text)
            .matches
            .iter()
            .filter(|m| m.rule().id() == "DE_VERBAGREEMENT")
            .map(|m| {
                (
                    m.message().to_string(),
                    m.offset(),
                    m.replacements()
                        .iter()
                        .map(|r| r.value().to_string())
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn flags_ich_with_wrong_verb_form() {
        let matches = va_matches("Ich bist über die Entwicklung sehr froh.");
        assert!(
            matches
                .iter()
                .any(|(msg, _, repl)| msg.contains("Subjekt (Ich)") && !repl.is_empty()),
            "got {matches:?}"
        );
    }

    #[test]
    fn flags_second_person_verb_without_du() {
        let matches = va_matches("Max bist heute da.");
        assert!(
            matches
                .iter()
                .any(|(msg, _, _)| msg.contains("Subjekt und Prädikat (bist)")),
            "got {matches:?}"
        );
    }

    #[test]
    fn accepts_correct_sentences() {
        for text in [
            "Ich bin über die Entwicklung sehr froh.",
            "Max ist heute da.",
            "Wir sind sicher, dass er kommt.",
            "Ken dachte, du wärst ich.",
        ] {
            assert!(
                va_matches(text).is_empty(),
                "{text}: got {:?}",
                va_matches(text)
            );
        }
    }

    #[test]
    fn flags_du_with_wrong_verb_form() {
        let matches = va_matches("Du ist heute da.");
        assert!(
            matches
                .iter()
                .any(|(msg, _, repl)| msg.contains("Subjekt (Du)")
                    && msg.contains("(ist)")
                    && !repl.is_empty()),
            "got {matches:?}"
        );
    }

    #[test]
    fn flags_er_with_wrong_verb_form() {
        let matches = va_matches("Er bist müde.");
        assert!(
            matches
                .iter()
                .any(|(msg, _, repl)| msg.contains("Subjekt (Er)")
                    && repl.iter().any(|r| r.contains("ist"))),
            "got {matches:?}"
        );
    }

    #[test]
    fn flags_wir_with_wrong_verb_form() {
        let matches = va_matches("Wir ist müde.");
        assert!(
            matches
                .iter()
                .any(|(msg, _, repl)| msg.contains("Subjekt (Wir)")
                    && repl.iter().any(|r| r.contains("sind"))),
            "got {matches:?}"
        );
    }

    #[test]
    fn checks_partial_sentences_after_subordinating_conjunctions() {
        for text in [
            "Ich bin froh, weil du bist müde.",
            "Ich bin froh, obwohl er ist müde.",
            "Wir sind sicher, dass ich bin müde.",
        ] {
            assert!(
                va_matches(text).is_empty(),
                "{text}: got {:?}",
                va_matches(text)
            );
        }
        let matches = va_matches("Ich bin froh, weil du ist müde.");
        assert!(
            matches
                .iter()
                .any(|(msg, _, _)| msg.contains("Subjekt (du)")),
            "got {matches:?}"
        );
    }

    #[test]
    fn bin_after_ignored_names_and_laden_is_not_flagged() {
        for text in [
            "Abdullah bin Laden war reich.",
            "Tim bin Laden war reich.",
            "Suleiman bin Abdul war König.",
        ] {
            assert!(
                va_matches(text).is_empty(),
                "{text}: got {:?}",
                va_matches(text)
            );
        }
    }

    #[test]
    fn metadata_is_default_on_grammar() {
        use og_core::rule::TextLevelRule as _;
        let rule = super::VerbAgreementRule::new(None);
        assert_eq!(rule.id(), "DE_VERBAGREEMENT");
        assert!(rule.is_default_on());
        assert_eq!(rule.category().id().as_str(), "GRAMMAR");
        assert_eq!(rule.issue_type(), og_core::IssueType::Grammar);
        assert!(rule.description().contains("Kongruenz"));
    }
}
