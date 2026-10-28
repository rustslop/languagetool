use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchRule,
    SuggestedReplacement,
};
use std::sync::LazyLock;

const MAX_PHRASE_LENGTH: usize = 3;
const MIN_PHRASE_LENGTH: usize = 2;

const DESC_REPETITION: &str = "Repetició de paraules (p. ex. «mà mà»)";
const DESC_REPETITION_SHORT: &str = "Repetició de paraules";

const UNITATS_TEMPS: [&str; 23] = [
    "segon",
    "minut",
    "hora",
    "horeta",
    "dia",
    "jorn",
    "jornada",
    "setmana",
    "quinzena",
    "mes",
    "trimestre",
    "quadrimestre",
    "semestre",
    "any",
    "lustre",
    "dècada",
    "decenni",
    "segle",
    "mil·lenni",
    "mil·lenari",
    "minutet",
    "segonet",
    "anyet",
];

const POSTAG_EXCEPTIONS: [&str; 11] = [
    "_emoji_",
    "_PUNCT",
    "_PUNCT_CONT",
    "allow_repetition",
    "_allow_repeat",
    "LOC_ADJ",
    "LOC_ADV",
    "LOC_CONJ",
    "LOC_PREP",
    "SENT_START",
    "UNKNOWN",
];

const CONJUNCIONS_I_O: [&str; 2] = ["i", "o"];

const LITERAL_ANTIPATTERNS: [&[&str]; 7] = [
    &["casa", "a", "casa", "a"],
    &["boca", "a", "boca", "a"],
    &["braç", "a", "braç", "a"],
    &["gen", "a", "gen", "a"],
    &["de", "tu", "a", "tu"],
    &["milions", "de"],
    &["res", "de", "res", "de"],
];

static PUNCT_AND_SYMBOL: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[\p{P}\p{S}']+$").unwrap());
static WORD_FOR_SPELLER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[\p{L}\d\p{P}\p{Zs}]+$").unwrap());

fn is_punctuation_or_symbol(token: &str) -> bool {
    PUNCT_AND_SYMBOL.is_match(token)
}

fn is_emoji(token: &str) -> bool {
    token.encode_utf16().count() > 1
        && token.chars().count() != token.encode_utf16().count()
        && !WORD_FOR_SPELLER.is_match(token)
}

fn is_repeat_word(token: &str) -> bool {
    if is_emoji(token) {
        return false;
    }
    if token.chars().all(|c| c.is_numeric() || c == ' ') {
        return false;
    }
    if token.chars().count() == 1 {
        return token.chars().next().is_some_and(char::is_alphabetic);
    }
    true
}

fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.chars().zip(b.chars()).all(|(x, y)| {
            x == y || x.to_lowercase().eq(y.to_lowercase()) || x.to_uppercase().eq(y.to_uppercase())
        })
}

fn sequence_matches(tokens: &[&AnalyzedTokenReadings], start: usize, words: &[&str]) -> bool {
    tokens
        .iter()
        .skip(start)
        .zip(words.iter())
        .all(|(token, word)| eq_ignore_case(token.token().token(), word))
}

fn overlaps(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start <= b_end && b_start <= a_end
}

fn is_pos_tag_unknown(atr: &AnalyzedTokenReadings) -> bool {
    atr.readings().len() == 1 && atr.readings()[0].pos_tags().is_empty()
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
        if is_repeat_word(first_token) {
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

#[derive(Default)]
pub struct CatalanPhraseRepeatRule;

impl CatalanPhraseRepeatRule {
    pub fn new() -> Self {
        Self
    }

    fn ignore(
        &self,
        tokens: &[&AnalyzedTokenReadings],
        unknown: &[bool],
        position: usize,
        phrase_length: usize,
    ) -> bool {
        if phrase_length == 2
            && self.matches_antipattern(tokens, position, position + phrase_length)
        {
            return true;
        }
        self.violates_pattern_exceptions(tokens, unknown, position, phrase_length)
    }

    fn violates_pattern_exceptions(
        &self,
        tokens: &[&AnalyzedTokenReadings],
        unknown: &[bool],
        position: usize,
        phrase_length: usize,
    ) -> bool {
        for i in position..position + phrase_length {
            let first = tokens[i];
            let second = tokens[i + phrase_length];
            let token_str = first.token().token().to_lowercase();
            if unknown[i] || is_pos_tag_unknown(first) {
                return true;
            }
            if CONJUNCIONS_I_O.contains(&token_str.as_str()) || is_punctuation_or_symbol(&token_str)
            {
                return true;
            }
            if POSTAG_EXCEPTIONS
                .iter()
                .any(|exception| first.has_pos_tag(exception) || second.has_pos_tag(exception))
            {
                return true;
            }
        }
        false
    }

    fn matches_antipattern(
        &self,
        tokens: &[&AnalyzedTokenReadings],
        match_start: usize,
        match_end: usize,
    ) -> bool {
        // Java passes position+phraseLength as the inclusive "end" of the match;
        // mirroring that exact overlap convention decides which antipatterns block.
        let window_start = match_start.saturating_sub(3).max(1);
        let window_end = (match_end + 3).min(tokens.len() - 1);
        for antipattern in LITERAL_ANTIPATTERNS {
            let last = window_end.saturating_sub(antipattern.len() - 1);
            if window_start > last {
                continue;
            }
            for j in window_start..=last {
                if sequence_matches(tokens, j, antipattern)
                    && overlaps(j, j + antipattern.len() - 1, match_start, match_end)
                {
                    return true;
                }
            }
        }
        let last = window_end.saturating_sub(3);
        for j in window_start..=last {
            if UNITATS_TEMPS.contains(&tokens[j].token().token().to_lowercase().as_str())
                && eq_ignore_case(tokens[j + 1].token().token(), "a")
                && eq_ignore_case(tokens[j].token().token(), tokens[j + 2].token().token())
                && eq_ignore_case(tokens[j + 3].token().token(), "a")
                && overlaps(j, j + 3, match_start, match_end)
            {
                return true;
            }
        }
        let last = window_end.saturating_sub(2);
        for j in window_start..=last {
            if eq_ignore_case(tokens[j].token().token(), "ni")
                && eq_ignore_case(tokens[j + 1].token().token(), "massa")
                && tokens[j + 2].has_lemma("poc")
                && overlaps(j, j + 2, match_start, match_end)
            {
                return true;
            }
            if tokens[j].has_pos_tag("_QM_OPEN")
                && tokens[j + 2].has_pos_tag("_QM_CLOSE")
                && overlaps(j, j + 2, match_start, match_end)
            {
                return true;
            }
        }
        false
    }
}

impl Rule for CatalanPhraseRepeatRule {
    fn id(&self) -> &str {
        "CATALAN_PHRASE_REPEAT_RULE"
    }

    fn description(&self) -> &str {
        DESC_REPETITION
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("duplication".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let unknown: Vec<bool> = sentence
            .pre_disambiguation_non_whitespace_tokens()
            .iter()
            .map(|atr| is_pos_tag_unknown(atr))
            .collect();
        let unknown = if unknown.len() == tokens.len() {
            unknown
        } else {
            tokens.iter().map(|atr| is_pos_tag_unknown(atr)).collect()
        };
        let mut matches = Vec::new();
        let mut i = 1usize;
        while i < tokens.len() {
            let mut matched_length = 0usize;
            for phrase_length in (MIN_PHRASE_LENGTH..=MAX_PHRASE_LENGTH).rev() {
                if i + 2 * phrase_length <= tokens.len()
                    && phrase_repeated_at(&tokens, i, phrase_length)
                    && !self.ignore(&tokens, &unknown, i, phrase_length)
                {
                    matched_length = phrase_length;
                    break;
                }
            }
            if matched_length > 0 {
                let start = tokens[i].token().start();
                let end = tokens[i + 2 * matched_length - 1].token().end();
                let phrase = phrase_to_string(&tokens, i, matched_length);
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                matches.push(
                    RuleMatch::new(
                        DESC_REPETITION,
                        start,
                        end - start,
                        rm_rule,
                        crate::ca::native_rules::context_around(sentence, start, end, 20),
                    )
                    .with_short_message(DESC_REPETITION_SHORT)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    fn token_at(text: &str, offset: usize) -> AnalyzedTokenReadings {
        AnalyzedTokenReadings::new(AnalyzedToken::new(text, offset, offset + text.len()))
    }

    fn reading_at(
        text: &str,
        offset: usize,
        lemma: &str,
        pos_tags: &[&str],
    ) -> AnalyzedTokenReadings {
        token_at(text, offset).with_readings(vec![
            AnalyzedToken::new(text, offset, offset + text.len())
                .with_lemma(lemma)
                .with_pos_tags(pos_tags.iter().map(|t| t.to_string()).collect()),
        ])
    }

    fn sentence(text: &str, tokens: Vec<AnalyzedTokenReadings>) -> AnalyzedSentence {
        let mut sent_start = token_at("<S>", 0);
        sent_start
            .token_mut()
            .set_pos_tags(vec!["SENT_START".to_string()]);
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut all = vec![sent_start];
        all.extend(tokens);
        sentence.set_tokens(all);
        sentence
    }

    fn lay_out(text: &str, words: &[(&str, &str, &[&str])]) -> AnalyzedSentence {
        let mut tokens = Vec::new();
        let mut cursor = 0usize;
        for (word, lemma, tags) in words {
            let start = cursor
                + text[cursor..]
                    .find(word)
                    .unwrap_or_else(|| panic!("{word} not in {text}"));
            let end = start + word.len();
            if lemma.is_empty() {
                tokens.push(token_at(word, start));
            } else {
                tokens.push(reading_at(word, start, lemma, tags));
            }
            cursor = end;
        }
        sentence(text, tokens)
    }

    fn rule_matches(sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        CatalanPhraseRepeatRule::new().match_sentence(sentence)
    }

    fn first_suggestion(matches: &[RuleMatch]) -> String {
        matches[0]
            .replacements()
            .first()
            .map(|r| r.value().to_string())
            .expect("a suggested replacement")
    }

    #[test]
    fn flags_two_word_repeat_with_phrase_suggestion() {
        let sentence = lay_out(
            "Benvinguts a casa a casa meva.",
            &[
                ("Benvinguts", "benvingut", &["VMP00PM0"]),
                ("a", "a", &["SPS00"]),
                ("casa", "casa", &["NCFS000"]),
                ("a", "a", &["SPS00"]),
                ("casa", "casa", &["NCFS000"]),
                ("meva", "meu", &["DP1FSS"]),
                (".", ".", &["Fp"]),
            ],
        );
        let matches = rule_matches(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(first_suggestion(&matches), "a casa");
        assert_eq!(matches[0].offset(), 11);
        assert_eq!(matches[0].length(), "a casa a casa".len());
        assert_eq!(matches[0].short_message(), Some(DESC_REPETITION_SHORT));
        assert_eq!(matches[0].message(), DESC_REPETITION);
    }

    #[test]
    fn prefers_the_three_word_phrase_over_two_overlaps() {
        let sentence = lay_out(
            "La casa és la casa és verda",
            &[
                ("La", "el", &["DA0FS0"]),
                ("casa", "casa", &["NCFS000"]),
                ("és", "ser", &["VSIP3S00"]),
                ("la", "el", &["DA0FS0"]),
                ("casa", "casa", &["NCFS000"]),
                ("és", "ser", &["VSIP3S00"]),
                ("verda", "verd", &["AQ0FS0"]),
            ],
        );
        let matches = rule_matches(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(first_suggestion(&matches), "La casa és");
    }

    #[test]
    fn skips_past_the_whole_repeated_span() {
        let sentence = lay_out(
            "a la a la a la",
            &[
                ("a", "a", &["SPS00"]),
                ("la", "el", &["DA0FS0"]),
                ("a", "a", &["SPS00"]),
                ("la", "el", &["DA0FS0"]),
                ("a", "a", &["SPS00"]),
                ("la", "el", &["DA0FS0"]),
            ],
        );
        let matches = rule_matches(&sentence);
        assert_eq!(matches.len(), 1, "{matches:?}");
    }

    #[test]
    fn immunized_tokens_are_skipped() {
        let mut sentence = lay_out(
            "a la a la",
            &[
                ("a", "a", &["SPS00"]),
                ("la", "el", &["DA0FS0"]),
                ("a", "a", &["SPS00"]),
                ("la", "el", &["DA0FS0"]),
            ],
        );
        sentence.tokens_mut()[2].immunize();
        assert!(rule_matches(&sentence).is_empty());
    }

    #[test]
    fn phrases_without_words_are_not_flagged() {
        let sentence = lay_out(
            ", , , ,",
            &[
                (",", ",", &["Fp"]),
                (",", ",", &["Fp"]),
                (",", ",", &["Fp"]),
                (",", ",", &["Fp"]),
            ],
        );
        assert!(rule_matches(&sentence).is_empty());
    }

    #[test]
    fn numeric_and_emoji_repeats_are_not_flagged() {
        let numeric = lay_out(
            "16 15 15 16 15 15",
            &[
                ("16", "16", &["Z"]),
                ("15", "15", &["Z"]),
                ("15", "15", &["Z"]),
                ("16", "16", &["Z"]),
                ("15", "15", &["Z"]),
                ("15", "15", &["Z"]),
            ],
        );
        assert!(rule_matches(&numeric).is_empty());
        let emoji = lay_out(
            "📚 📚 📚 📚",
            &[
                ("📚", "", &[]),
                ("📚", "", &[]),
                ("📚", "", &[]),
                ("📚", "", &[]),
            ],
        );
        assert!(rule_matches(&emoji).is_empty());
    }

    #[test]
    fn conjunction_repeats_are_ignored() {
        let sentence = lay_out(
            "una altra i una altra i una altra",
            &[
                ("una", "un", &["DI0FS0"]),
                ("altra", "altre", &["AQ0FS0"]),
                ("i", "i", &["CC"]),
                ("una", "un", &["DI0FS0"]),
                ("altra", "altre", &["AQ0FS0"]),
                ("i", "i", &["CC"]),
                ("una", "un", &["DI0FS0"]),
                ("altra", "altre", &["AQ0FS0"]),
            ],
        );
        assert!(rule_matches(&sentence).is_empty());
    }

    #[test]
    fn punctuation_anchors_are_ignored() {
        type Case = (
            &'static str,
            &'static [(&'static str, &'static str, &'static [&'static str])],
        );
        let cases: &[Case] = &[
            (
                "=r=r[s[i]]||{}",
                &[
                    ("=", "", &[]),
                    ("r", "r", &["NCMS000"]),
                    ("=", "", &[]),
                    ("r", "r", &["NCMS000"]),
                    ("[", "", &[]),
                    ("s", "s", &["NCFP000"]),
                    ("[", "", &[]),
                    ("i", "i", &["CC"]),
                    ("]", "", &[]),
                    ("]", "", &[]),
                    ("|", "", &[]),
                    ("|", "", &[]),
                    ("{", "", &[]),
                    ("}", "", &[]),
                ],
            ),
            (
                "l'honestedat d’alguns ˗alguns˗ humans.",
                &[
                    ("l'", "el", &["DA0MS0"]),
                    ("honestedat", "honestedat", &["NCFS000"]),
                    ("d’", "de", &["SPS00"]),
                    ("alguns", "algun", &["DI0MP0"]),
                    ("˗", "", &[]),
                    ("alguns", "algun", &["DI0MP0"]),
                    ("˗", "", &[]),
                    ("humans", "humà", &["NCMP000"]),
                    (".", "", &[]),
                ],
            ),
        ];
        for (text, words) in cases {
            let sentence = lay_out(text, words);
            assert!(
                rule_matches(&sentence).is_empty(),
                "{text}: {:?}",
                rule_matches(&sentence)
            );
        }
    }

    #[test]
    fn literal_antipatterns_block_the_match() {
        for text in [
            "casa a casa a",
            "boca a boca a",
            "braç a braç a",
            "gen a gen a",
            "de tu a tu a",
            "milions de milions de",
            "res de res de",
        ] {
            let word_lemmas: Vec<(&str, &str, &[&str])> = text
                .split(' ')
                .map(|word| match word {
                    "a" | "de" => (word, word, &["SPS00"][..]),
                    "casa" | "boca" | "gen" | "milions" | "res" => (word, word, &["NCMP000"][..]),
                    "braç" => (word, word, &["NCMS000"][..]),
                    "tu" => (word, word, &["PP2CS00"][..]),
                    _ => (word, word, &["NCMS000"][..]),
                })
                .collect();
            let sentence = lay_out(text, &word_lemmas);
            assert!(
                rule_matches(&sentence).is_empty(),
                "{text}: {:?}",
                rule_matches(&sentence)
            );
        }
    }

    #[test]
    fn unitats_temps_a_repeat_a_is_ignored() {
        for text in ["dia a dia a", "mes a mes a", "Dia a dia a"] {
            let words: Vec<(&str, &str, &[&str])> = text
                .split(' ')
                .map(|word| {
                    if word == "a" {
                        (word, word, &["SPS00"][..])
                    } else {
                        (word, word, &["NCMS000"][..])
                    }
                })
                .collect();
            let sentence = lay_out(text, &words);
            assert!(
                rule_matches(&sentence).is_empty(),
                "{text}: {:?}",
                rule_matches(&sentence)
            );
        }
    }

    #[test]
    fn ni_massa_poc_needs_the_poc_lemma() {
        let with_lemma = lay_out(
            "No n'hi havia ni massa ni massa pocs",
            &[
                ("No", "no", &["RN"]),
                ("n'", "en", &["PP3MS000"]),
                ("hi", "hi", &["PP3CN000"]),
                ("havia", "haver", &["VAII1S00"]),
                ("ni", "ni", &["CC"]),
                ("massa", "massa", &["RG"]),
                ("ni", "ni", &["CC"]),
                ("massa", "massa", &["RG"]),
                ("pocs", "poc", &["DI0MP0"]),
            ],
        );
        assert!(rule_matches(&with_lemma).is_empty());
        let with_other_lemma = lay_out(
            "No n'hi havia ni massa ni massa pocs",
            &[
                ("No", "no", &["RN"]),
                ("n'", "en", &["PP3MS000"]),
                ("hi", "hi", &["PP3CN000"]),
                ("havia", "haver", &["VAII1S00"]),
                ("ni", "ni", &["CC"]),
                ("massa", "massa", &["RG"]),
                ("ni", "ni", &["CC"]),
                ("massa", "massa", &["RG"]),
                ("pocs", "pocs", &["DI0MP0"]),
            ],
        );
        let matches = rule_matches(&with_other_lemma);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(first_suggestion(&matches), "ni massa");
    }

    #[test]
    fn quoted_single_token_is_ignored_via_qm_postags() {
        let sentence = lay_out(
            "No cal posar 'a' a tot arreu.",
            &[
                ("No", "no", &["RN"]),
                ("cal", "caldre", &["VMIP3S00"]),
                ("posar", "posar", &["VMN0000"]),
                ("'", "'", &["_QM_OPEN"]),
                ("a", "a", &["SPS00"]),
                ("'", "'", &["_QM_CLOSE"]),
                ("a", "a", &["SPS00"]),
                ("tot", "tot", &["DI0MS0"]),
                ("arreu", "arreu", &["RG"]),
                (".", ".", &["Fp"]),
            ],
        );
        assert!(rule_matches(&sentence).is_empty());
    }

    #[test]
    fn exception_postags_are_ignored() {
        for tag in [
            "LOC_ADV",
            "LOC_ADJ",
            "LOC_PREP",
            "LOC_CONJ",
            "allow_repetition",
            "_allow_repeat",
            "UNKNOWN",
            "SENT_START",
        ] {
            let sentence = lay_out(
                "a la a la",
                &[
                    ("a", "a", &[tag]),
                    ("la", "el", &["DA0FS0"]),
                    ("a", "a", &["SPS00"]),
                    ("la", "el", &["DA0FS0"]),
                ],
            );
            assert!(
                rule_matches(&sentence).is_empty(),
                "{tag}: {:?}",
                rule_matches(&sentence)
            );
        }
    }

    #[test]
    fn untagged_first_token_is_ignored() {
        let sentence = lay_out(
            "a la a la",
            &[
                ("a", "", &[]),
                ("la", "el", &["DA0FS0"]),
                ("a", "", &[]),
                ("la", "el", &["DA0FS0"]),
            ],
        );
        assert!(rule_matches(&sentence).is_empty());
    }

    #[test]
    fn java_incorrect_sentences_flag_with_the_expected_suggestion() {
        let cases: &[(&str, &str)] = &[
            ("Benvinguts a la a la casa.", "a la"),
            ("Ho poso com a com a exemple", "com a"),
            ("És l'americà l'americà.", "l'americà"),
            (
                "És d'aconseguir-los d'aconseguir-los així",
                "d'aconseguir-los",
            ),
        ];
        for (text, suggestion) in cases {
            let mut words: Vec<(&str, &str, &[&str])> = Vec::new();
            for piece in text.split(' ') {
                let (word, tail) = match piece.strip_suffix('.') {
                    Some(stem) => (stem, Some(".")),
                    None => (piece, None),
                };
                match word {
                    "l'americà" | "d'aconseguir-los" => {
                        let (prefix, rest) = word.split_at(2);
                        let tag: &[&str] = if prefix.starts_with('l') {
                            &["DA0MS0"]
                        } else {
                            &["SPS00"]
                        };
                        words.push((prefix, prefix, tag));
                        words.push((rest, rest, &["AQ0MS0"]));
                    }
                    "És" => words.push((word, "ser", &["VSIP3S00"])),
                    "a" | "com" => words.push((word, word, &["SPS00"])),
                    _ => words.push((word, word, &["NCMS000"])),
                }
                if let Some(tail) = tail {
                    words.push((tail, tail, &["Fp"]));
                }
            }
            let matches = rule_matches(&lay_out(text, &words));
            assert_eq!(matches.len(), 1, "{text}: {matches:?}");
            assert_eq!(first_suggestion(&matches), *suggestion, "{text}");
        }
    }

    #[test]
    fn adjacent_clitic_tokens_join_without_a_space_in_the_suggestion() {
        let sentence = lay_out(
            "És l'americà l'americà.",
            &[
                ("És", "ser", &["VSIP3S00"]),
                ("l'", "el", &["DA0MS0"]),
                ("americà", "americà", &["NCMS000"]),
                ("l'", "el", &["DA0MS0"]),
                ("americà", "americà", &["NCMS000"]),
                (".", ".", &["Fp"]),
            ],
        );
        let matches = rule_matches(&sentence);
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(first_suggestion(&matches), "l'americà");
        assert_eq!(matches[0].offset(), 4);
        assert_eq!(matches[0].length(), "l'americà l'americà".len());
    }

    mod java_pipeline_spec {
        use super::*;
        use std::sync::Arc;

        struct Pipeline {
            tokenizer: crate::ca::word_tokenizer::CatalanWordTokenizer,
            tagger: Arc<crate::ca::tagger::CatalanTagger>,
            disambiguator: Option<Arc<dyn og_core::checker::Disambiguator>>,
        }

        fn pipeline() -> Arc<Pipeline> {
            static PIPE: og_core::releasable::Releasable<Pipeline> =
                og_core::releasable::Releasable::new();
            og_core::releasable::shared_get_or_build(&PIPE, || {
                let resource_dir = crate::ca::tagger::ca_resource_dir().expect("ca resource dir");
                let disambiguator =
                    std::fs::read_to_string(resource_dir.join("disambiguation.xml"))
                        .ok()
                        .and_then(|xml| og_tagger::XmlDisambiguator::from_xml(&xml).ok())
                        .and_then(|xml| {
                            crate::engine::find_lt_core_resource_dir().map(|core| {
                                og_tagger::hybrid_disambiguator("ca", &resource_dir, &core, xml)
                            })
                        });
                Some(Arc::new(Pipeline {
                    tokenizer: crate::ca::word_tokenizer::CatalanWordTokenizer::new(),
                    tagger: crate::ca::tagger::CatalanTagger::shared_arc()
                        .expect("ca tagger over added.txt"),
                    disambiguator,
                }))
            })
            .expect("ca pipeline")
        }

        fn analyze(text: &str) -> AnalyzedSentence {
            use og_tokenizer::WordTokenizer as _;
            let pipe = pipeline();
            let mut sentence = AnalyzedSentence::new(text, 0, text.len());
            let mut sent_start = token_at("<S>", 0);
            sent_start
                .token_mut()
                .set_pos_tags(vec!["SENT_START".to_string()]);
            let mut tokens = vec![sent_start];
            tokens.extend(
                pipe.tokenizer
                    .tokenize(text)
                    .into_iter()
                    .map(|t| token_at(t.text(), t.start())),
            );
            sentence.set_tokens(tokens);
            let token_texts: Vec<String> = sentence
                .tokens()
                .iter()
                .skip(1)
                .map(|t| t.token().token().to_string())
                .collect();
            let refs: Vec<&str> = token_texts.iter().map(|s| s.as_str()).collect();
            let tagged = og_tagger::Tagger::tag(pipe.tagger.as_ref(), &refs);
            let tokens_mut = sentence.tokens_mut();
            for (i, atr) in tagged.into_iter().enumerate() {
                let token_idx = i + 1;
                if token_idx >= tokens_mut.len() {
                    continue;
                }
                let readings = atr.into_readings();
                if !readings.is_empty() {
                    let primary = tokens_mut[token_idx].token().clone();
                    let updated_primary =
                        AnalyzedToken::new(primary.token(), primary.start(), primary.end())
                            .with_lemma(readings[0].lemma().unwrap_or(primary.token()).to_string());
                    tokens_mut[token_idx] =
                        AnalyzedTokenReadings::new(updated_primary).with_readings(readings);
                }
            }
            for token in sentence.tokens_mut().iter_mut().rev() {
                if !token.is_whitespace()
                    && !token.token().pos_tags().contains(&"SENT_START".to_string())
                {
                    if !token.has_pos_tag("SENT_END") {
                        token.add_sent_end();
                    }
                    break;
                }
            }
            if pipe.disambiguator.is_some() {
                sentence.set_pre_disambiguation_tokens(sentence.tokens().to_vec());
            }
            if let Some(disambiguator) = &pipe.disambiguator {
                disambiguator.disambiguate(&mut sentence);
            }
            crate::ca::gv_chunker::add_gv_chunks(&mut sentence);
            sentence
        }

        fn matches_for(text: &str) -> Vec<RuleMatch> {
            CatalanPhraseRepeatRule::new().match_sentence(&analyze(text))
        }

        const CORRECT: &[&str] = &[
            "sobretot, que dubta, que dubta constantment",
            "no deixava de tenir febre i més febre i més febre",
            "una vegada i una altra i una altra i una altra. ",
            "trobava aquest títol o aquest o aquest altre,",
            " Que sí, que sí, que ho heu llegit bé.",
            "—Molt bé, molt bé, tampoc cal que et posis així ",
            "però la mort –la Mort–,",
            "Que…, que…, ja hi ha les entrades?",
            "Que et penses que parlant descobrirem res de res de per què som aquí?",
            "Què es podia esperar d'un… d'un…?",
            "Imperfet de subjuntiu [é] [é] [í]",
            "Soc molt vella molt vella, però que encara hi soc.",
            "No volia res més que mirar i mirar i mirar.",
            "A més a més, ho va fer a poc a poc.",
            "A diferència dels dels ocells.",
            "Puja pas a pas a les ruïnes",
            "No cal posar 'a' a tot arreu.",
            "estava malalt de bo de bo",
            "Ve ací de tant en tant en lloc d'anar allà",
            "Sigui qui sigui qui vingui.",
            "D'hereu a hereu a través de generacions.",
            "De dos en dos en l'arbre.",
            "No n'hi havia ni massa ni massa pocs",
            "Podia tractar qualsevol persona de tu a tu a qualsevol nivell.",
            "Eren xifres molt -molt- modestes.",
            "Revisa milions de milions de dades en pocs segons.",
            " © © © © ©",
            " © © © © © © © ©",
            "p=(16+16+1+1)",
            "p=(16 + + 16 + +)",
            "16 15 15 16 15 15",
            "16 15 16 15 16 15",
            "~~~~",
            "~~~~~~~~",
            "l'honestedat d’alguns ˗alguns˗ humans.",
            "Mader | Andorra | Andorra La Vella ",
            "==== Arc de Zou ====",
            "============",
            "=r=r[s[i]]||{}",
            "Serveis que ofereix el CRAI > Préstec > Préstec, reserves",
            "total de folis: 166 (III + III + I+ 156 + III) ff.",
            "📚📚📚📚📚📚📚📚📚📚",
        ];

        #[test]
        fn java_correct_sentences_produce_no_match() {
            let _release_guard = crate::release_guard();
            for text in CORRECT {
                let matches = matches_for(text);
                assert!(matches.is_empty(), "{text}: {matches:?}");
            }
        }

        const INCORRECT: &[(&str, &str)] = &[
            ("Benvinguts a casa a casa meva.", "a casa"),
            ("Benvinguts a la a la casa.", "a la"),
            ("Benvinguts a casa meva a casa meva.", "a casa meva"),
            ("Ho poso com a com a exemple", "com a"),
            ("És l'americà l'americà.", "l'americà"),
            (
                "És d'aconseguir-los d'aconseguir-los així",
                "d'aconseguir-los",
            ),
            ("La casa és la casa és verda", "La casa és"),
        ];

        #[test]
        fn java_incorrect_sentences_are_dict_starved() {
            let _release_guard = crate::release_guard();
            for (text, java_suggestion) in INCORRECT {
                let matches = matches_for(text);
                assert_eq!(
                    matches.len(),
                    0,
                    "{text}: Java flags the repetition suggesting '{java_suggestion}' \
                     (CatalanPhraseRepeatRuleTest.java:99-105); with the ca tagger limited to \
                     added.txt every content token is isPosTagUnknown and the pattern \
                     exceptions veto the match - flips to Java parity when ca-ES.dict lands"
                );
            }
        }
    }
}
