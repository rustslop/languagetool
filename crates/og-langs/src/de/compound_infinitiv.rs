use super::helpers::{has_pos_tag_starting_with, lowercase_first_char};
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use std::sync::{Arc, LazyLock};

#[derive(Clone)]
enum Matcher {
    Token(&'static str),
    Regex(&'static str),
    PosRegex(&'static str),
    OptionalPos(&'static str),
    TokenSkip(&'static str, usize),
}

fn seq(matchers: &[Matcher]) -> Vec<Matcher> {
    matchers.to_vec()
}

static ANTI_PATTERNS: LazyLock<Vec<Vec<Matcher>>> = LazyLock::new(|| {
    vec![
        seq(&[
            Matcher::Token("auf"),
            Matcher::Token("Nummer"),
            Matcher::Token("sicher"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Token("ab"),
            Matcher::Regex("und|&"),
            Matcher::Token("an"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Token("ganz"),
            Matcher::Token("schön"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Regex("fang|fängst|fängt|fangt|fangen|fing|fingen"),
            Matcher::PosRegex("ADV.*"),
            Matcher::Token("an"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Regex("dazu|darüber"),
            Matcher::Token("zu"),
            Matcher::Token("machen"),
        ]),
        seq(&[
            Matcher::Token("schön"),
            Matcher::Token("zu"),
            Matcher::Token("machen"),
        ]),
        seq(&[
            Matcher::Token("kurz"),
            Matcher::Token("davor"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Regex("Jahr|Monat|Zeit"),
            Matcher::Token("über"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Token("endlich"),
            Matcher::Token("wieder"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Token("bis"),
            Matcher::Token("hin"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Token("von"),
            Matcher::Regex(".*[a-z].*"),
            Matcher::Token("her"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Regex("sehr|ganz|äu(ss|ß)erst|zu|nicht|absolut|total|wirklich|möglichst"),
            Matcher::PosRegex("ADJ.*"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Token("Schritt"),
            Matcher::Token("weiter"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Token("und"),
            Matcher::Token("so"),
            Matcher::Token("weiter"),
        ]),
        seq(&[
            Matcher::Token("darauf"),
            Matcher::Token("zu"),
            Matcher::PosRegex("VER.*"),
            Matcher::Token("dass"),
        ]),
        seq(&[
            Matcher::Token("darauf"),
            Matcher::Token("zu"),
            Matcher::PosRegex("VER.*"),
            Matcher::Token(","),
        ]),
        seq(&[
            Matcher::Regex(
                "Spiel|Tag|Nacht|Morgen|Nachmittag|Abend|Zeit|.+zeit|Jahr(zehnt)?|Monat|.+tag|Mittwoch|Januar|Februar|März|April|Mai|Juni|Juli|August|September|Oktober|November|Dezember",
            ),
            Matcher::Token("über"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Token("kurz"),
            Matcher::Token("zu"),
            Matcher::Token("machen"),
        ]),
        seq(&[
            Matcher::Token("dazu"),
            Matcher::Token("zu"),
            Matcher::Token("haben"),
        ]),
        seq(&[
            Matcher::Regex("deutlich|viel|Stück|nichts|nix|noch"),
            Matcher::Token("weiter"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Token("auf"),
            Matcher::Regex("und|&|oder|/"),
            Matcher::OptionalPos("ADV.*"),
            Matcher::Token("ab"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Token("zu"),
            Matcher::PosRegex("ADJ.*"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Token("hin"),
            Matcher::Regex("und|&|oder|/"),
            Matcher::OptionalPos("ADV.*"),
            Matcher::Token("her"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::Regex("rauf|hoch"),
            Matcher::Regex("und|&|oder|/"),
            Matcher::OptionalPos("ADV.*"),
            Matcher::Token("runter"),
            Matcher::Token("zu"),
        ]),
        seq(&[
            Matcher::TokenSkip("aus", 3),
            Matcher::Token("heraus"),
            Matcher::Token("zu"),
        ]),
    ]
});

static MATCHER_REGEXES: LazyLock<Vec<Vec<Option<regex::Regex>>>> = LazyLock::new(|| {
    ANTI_PATTERNS
        .iter()
        .map(|pattern| {
            pattern
                .iter()
                .map(|m| match m {
                    Matcher::Regex(re) | Matcher::PosRegex(re) | Matcher::OptionalPos(re) => {
                        Some(regex::Regex::new(re).unwrap())
                    }
                    _ => None,
                })
                .collect()
        })
        .collect()
});

const ADJ_EXCEPTION: [&str; 12] = [
    "schwer", "klar", "verloren", "bekannt", "rot", "blau", "gelb", "grün", "schwarz", "weiß",
    "fertig", "neu",
];

fn matcher_matches(
    matcher: &Matcher,
    re: Option<&regex::Regex>,
    atr: &AnalyzedTokenReadings,
) -> bool {
    match matcher {
        Matcher::Token(t) => atr.token().token() == *t,
        Matcher::Regex(_) | Matcher::PosRegex(_) | Matcher::OptionalPos(_) => {
            let re = re.unwrap_or_else(|| unreachable!());
            match matcher {
                Matcher::Regex(_) => re.is_match(atr.token().token()),
                _ => atr
                    .readings()
                    .iter()
                    .any(|r| r.pos_tags().iter().any(|t| re.is_match(t))),
            }
        }
        Matcher::TokenSkip(t, _) => atr.token().token() == *t,
    }
}

fn token_matches_anti_pattern_at(tokens: &[&AnalyzedTokenReadings], zu_index: usize) -> bool {
    fn covers(
        tokens: &[&AnalyzedTokenReadings],
        regexes: &[Option<regex::Regex>],
        pattern: &[Matcher],
        pattern_pos: usize,
        token_pos: usize,
        zu_index: usize,
    ) -> bool {
        if pattern_pos == pattern.len() {
            return token_pos > zu_index;
        }
        let matcher = &pattern[pattern_pos];
        if let Matcher::TokenSkip(_, skip) = matcher {
            if token_pos < tokens.len()
                && matcher_matches(matcher, regexes[pattern_pos].as_ref(), tokens[token_pos])
            {
                for gap in 0..=*skip {
                    if covers(
                        tokens,
                        regexes,
                        pattern,
                        pattern_pos + 1,
                        token_pos + 1 + gap,
                        zu_index,
                    ) {
                        return true;
                    }
                }
            }
            return false;
        }
        if token_pos < tokens.len()
            && matcher_matches(matcher, regexes[pattern_pos].as_ref(), tokens[token_pos])
            && covers(
                tokens,
                regexes,
                pattern,
                pattern_pos + 1,
                token_pos + 1,
                zu_index,
            )
        {
            return true;
        }
        matches!(matcher, Matcher::OptionalPos(_))
            && covers(
                tokens,
                regexes,
                pattern,
                pattern_pos + 1,
                token_pos,
                zu_index,
            )
    }
    for (pattern_idx, pattern) in ANTI_PATTERNS.iter().enumerate() {
        if pattern.len() > tokens.len() {
            continue;
        }
        let regexes = &MATCHER_REGEXES[pattern_idx];
        for start in 0..=zu_index {
            if covers(tokens, regexes, pattern, 0, start, zu_index) {
                return true;
            }
        }
    }
    false
}

fn is_punctuation(word: &str) -> bool {
    word.chars().count() == 1
        && matches!(
            word,
            "." | "?" | "!" | "…" | ":" | ";" | "," | "(" | ")" | "[" | "]"
        )
}

static HUNSPELL_WORDS: LazyLock<std::collections::HashSet<String>> = LazyLock::new(|| {
    let mut words = std::collections::HashSet::new();
    if let Some(lt_root) = crate::engine::find_lt_resource_dir() {
        let path =
            lt_root.join("de/src/main/resources/org/languagetool/resource/de/hunspell/de_DE.dic");
        if let Ok(data) = std::fs::read_to_string(path) {
            for (idx, line) in data.lines().enumerate() {
                if idx == 0 || line.starts_with('#') {
                    continue;
                }
                let word = line.split('\t').next().unwrap_or("");
                let word = word.split('/').next().unwrap_or("");
                if !word.is_empty() {
                    words.insert(word.to_string());
                }
            }
        }
    }
    words
});

pub struct CompoundInfinitivRule {
    dictionary: Option<Arc<og_spell::Dictionary>>,
}

impl CompoundInfinitivRule {
    pub fn new(dictionary: Option<Arc<og_spell::Dictionary>>) -> Self {
        Self { dictionary }
    }

    fn is_infinitiv(&self, token: &AnalyzedTokenReadings) -> bool {
        has_pos_tag_starting_with(token, "VER:INF")
    }

    fn is_relevant(&self, token: &AnalyzedTokenReadings) -> bool {
        token.has_pos_tag("ZUS") && !token.token().token().eq_ignore_ascii_case("um")
    }

    fn is_misspelled(&self, word: &str) -> bool {
        let word = lowercase_first_char(word);
        let dict = self.dictionary.clone();
        let known = move |w: &str| {
            HUNSPELL_WORDS.contains(w) || dict.as_ref().is_some_and(|d| d.contains(w))
        };
        if known(&word) {
            return false;
        }
        let bytes = word.as_bytes();
        let mut i = 0;
        while i + 2 <= bytes.len() {
            if &word[i..i + 2] != "zu" || !word.is_char_boundary(i) {
                i += 1;
                continue;
            }
            let left = &word[..i];
            let right = &word[i + 2..];
            if !left.is_empty() && !right.is_empty() && known(left) && known(right) {
                return false;
            }
            i += 1;
        }
        true
    }

    fn lemma_of(token: &AnalyzedTokenReadings) -> Option<&str> {
        token.readings().iter().find_map(|r| r.lemma())
    }

    fn is_exception(&self, tokens: &[&AnalyzedTokenReadings], n: usize) -> bool {
        if has_pos_tag_starting_with(tokens[n - 2], "VER") {
            return true;
        }
        if ADJ_EXCEPTION.contains(&tokens[n - 1].token().token()) {
            return true;
        }
        if tokens[n + 1].token().token() == "sagen"
            && matches!(tokens[n - 1].token().token(), "weiter" | "dazu")
        {
            return true;
        }
        if matches!(tokens[n + 1].token().token(), "tragen" | "machen")
            && tokens[n - 1].token().token() == "davon"
        {
            return true;
        }
        if tokens[n + 1].token().token() == "geben" && tokens[n - 1].token().token() == "daran" {
            return true;
        }
        if tokens[n + 1].token().token() == "gehen" && tokens[n - 1].token().token() == "ab" {
            return true;
        }
        if tokens[n + 1].token().token() == "errichten" && tokens[n - 1].token().token() == "wieder"
        {
            return true;
        }
        let mut i = n as isize - 2;
        while i > 0 && !is_punctuation(tokens[i as usize].token().token()) {
            let token = tokens[i as usize];
            let mut verb: Option<String> = None;
            if has_pos_tag_starting_with(token, "VER:IMP") {
                verb = Self::lemma_of(token).map(lowercase_first_char);
            } else if has_pos_tag_starting_with(token, "VER") {
                verb = Some(token.token().token().to_lowercase());
            } else if token.token().token() == "Fang" {
                verb = Some("fangen".to_string());
            }
            if let Some(verb) = verb {
                if !self.is_misspelled(&format!("{}{verb}", tokens[n - 1].token().token())) {
                    return true;
                }
                break;
            }
            i -= 1;
        }
        if matches!(tokens[n - 1].token().token(), "aus" | "an") {
            let mut i = n as isize - 2;
            while i > 0 && !is_punctuation(tokens[i as usize].token().token()) {
                if matches!(tokens[i as usize].token().token(), "von" | "vom") {
                    return true;
                }
                i -= 1;
            }
        }
        if tokens[n - 1].token().token() == "her" {
            let mut i = n as isize - 2;
            while i > 0 && !is_punctuation(tokens[i as usize].token().token()) {
                if tokens[i as usize].token().token() == "vor" {
                    return true;
                }
                i -= 1;
            }
        }
        false
    }
}

impl Rule for CompoundInfinitivRule {
    fn id(&self) -> &str {
        "COMPOUND_INFINITIV_RULE"
    }

    fn description(&self) -> &str {
        "Erweiterter Infinitiv mit zu (Zusammenschreibung)"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("COMPOUNDING", "Zusammenschreibung")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        if tokens.len() < 4 {
            return rule_matches;
        }
        for i in 2..tokens.len() - 1 {
            if tokens[i].token().token() == "zu"
                && self.is_infinitiv(tokens[i + 1])
                && self.is_relevant(tokens[i - 1])
                && !token_matches_anti_pattern_at(&tokens, i)
                && !self.is_exception(&tokens, i)
                && !self.is_misspelled(&format!(
                    "{}{}",
                    tokens[i - 1].token().token(),
                    tokens[i + 1].token().token()
                ))
            {
                let compound = format!(
                    "{}{}{}",
                    tokens[i - 1].token().token(),
                    tokens[i].token().token(),
                    tokens[i + 1].token().token()
                );
                let msg = format!(
                    "Wenn der erweiterte Infinitiv von dem Verb '{}{}' abgeleitet ist, sollte er zusammengeschrieben werden.",
                    tokens[i - 1].token().token(),
                    tokens[i + 1].token().token()
                );
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str())
                    .with_urls(vec![
                        "https://languagetool.org/insights/de/beitrag/zu-zusammen-oder-getrennt/"
                            .to_string(),
                    ]);
                let from = tokens[i - 1].token().start();
                let to = tokens[i + 1].token().end();
                rule_matches.push(
                    RuleMatch::new(
                        msg,
                        from,
                        to - from,
                        rm_rule,
                        RuleMatchContext::new(sentence.text().to_string(), from, to - from),
                    )
                    .with_replacements(vec![SuggestedReplacement::new(compound)])
                    .with_sentence(sentence.text().to_string()),
                );
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_getrenntes_zu_infinitiv() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Er überprüfte die Rechnungen noch einmal, um sicher zu gehen.");
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "COMPOUND_INFINITIV_RULE"),
            "expected COMPOUND_INFINITIV_RULE match, got {:?}",
            matches
                .matches
                .iter()
                .map(|m| m.rule().id().to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_fire_on_correct_compound() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Er überprüfte die Rechnungen noch einmal, um sicherzugehen.");
        assert!(
            !matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "COMPOUND_INFINITIV_RULE")
        );
    }

    #[test]
    fn no_fire_when_the_aus_heraus_gap_stays_within_the_java_skip() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        for text in [
            "Sie scheint aus Spanien heraus zu kaufen.",
            "Er will aus dem alten Haus heraus zu kaufen.",
        ] {
            let matches = engine.check(text);
            assert!(
                !matches
                    .matches
                    .iter()
                    .any(|m| m.rule().id() == "COMPOUND_INFINITIV_RULE"),
                "{text}: Java's token(\"aus\").setSkip(3) anti-pattern must suppress the rule"
            );
        }
    }

    #[test]
    fn fires_when_the_aus_heraus_gap_exceeds_the_java_skip() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Sie kauft aus dem alten leerstehenden Haus heraus zu gehen.");
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "COMPOUND_INFINITIV_RULE"),
            "four tokens between 'aus' and 'heraus' fall outside setSkip(3)"
        );
    }

    use og_core::AnalyzedToken;

    fn tok(word: &str, tags: &[&str]) -> AnalyzedTokenReadings {
        let primary = AnalyzedToken::new(word, 0, word.len());
        let readings = if tags.is_empty() {
            Vec::new()
        } else {
            tags.iter()
                .map(|t| AnalyzedToken::new(word, 0, word.len()).with_pos_tags(vec![t.to_string()]))
                .collect()
        };
        AnalyzedTokenReadings::new(primary).with_readings(readings)
    }

    fn imp(word: &str, lemma: &str) -> AnalyzedTokenReadings {
        let primary = AnalyzedToken::new(word, 0, word.len());
        AnalyzedTokenReadings::new(primary).with_readings(vec![
            AnalyzedToken::new(word, 0, word.len())
                .with_lemma(lemma)
                .with_pos_tags(vec!["VER:IMP".to_string()]),
        ])
    }

    #[test]
    fn exception_arms_match_the_java_list() {
        let rule = CompoundInfinitivRule::new(None);
        for tokens in [
            vec![
                tok("Er", &[]),
                tok("kommt", &["VER:3S"]),
                tok("an", &[]),
                tok("zu", &[]),
            ],
            vec![
                tok("Er", &[]),
                tok("ist", &[]),
                tok("schwer", &[]),
                tok("zu", &[]),
            ],
            vec![
                tok("Er", &[]),
                tok("weiter", &[]),
                tok("zu", &[]),
                tok("sagen", &[]),
            ],
            vec![
                tok("Er", &[]),
                tok("dazu", &[]),
                tok("zu", &[]),
                tok("sagen", &[]),
            ],
            vec![
                tok("Er", &[]),
                tok("davon", &[]),
                tok("zu", &[]),
                tok("tragen", &[]),
            ],
            vec![
                tok("Er", &[]),
                tok("davon", &[]),
                tok("zu", &[]),
                tok("machen", &[]),
            ],
            vec![
                tok("Er", &[]),
                tok("daran", &[]),
                tok("zu", &[]),
                tok("geben", &[]),
            ],
            vec![
                tok("Er", &[]),
                tok("ab", &[]),
                tok("zu", &[]),
                tok("gehen", &[]),
            ],
            vec![
                tok("Er", &[]),
                tok("wieder", &[]),
                tok("zu", &[]),
                tok("errichten", &[]),
            ],
        ] {
            let n = tokens
                .iter()
                .position(|t| t.token().token() == "zu")
                .unwrap();
            let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
            assert!(
                rule.is_exception(&refs, n),
                "{:?}: expected the Java exception",
                refs.iter().map(|t| t.token().token()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn imperative_and_fang_walks_accept_known_compounds() {
        let rule = CompoundInfinitivRule::new(None);
        let tokens = [
            tok("Er", &[]),
            imp("Gib", "geben"),
            tok("aus", &[]),
            tok("zu", &[]),
            tok("geben", &[]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        assert!(rule.is_exception(&refs, 3), "ausgeben is a known compound");

        let tokens = [
            tok("Er", &[]),
            tok("hat", &[]),
            tok("Fang", &[]),
            tok("an", &[]),
            tok("zu", &[]),
            tok("fangen", &[]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        assert!(rule.is_exception(&refs, 4), "anfangen is a known compound");
    }

    #[test]
    fn finite_verb_walk_falls_through_to_the_preposition_walks() {
        let rule = CompoundInfinitivRule::new(None);
        let tokens = [
            tok("Er", &[]),
            tok("sann", &["VER:3S"]),
            tok("dann", &[]),
            tok("an", &[]),
            tok("zu", &[]),
            tok("ziehen", &[]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        assert!(
            !rule.is_exception(&refs, 4),
            "'ansann' is not a word, so the verb walk cannot excuse the split infinitive"
        );

        for (word, earlier) in [("aus", "von"), ("aus", "vom"), ("her", "vor")] {
            let tokens = [
                tok("Er", &[]),
                tok("kam", &["VER:3S"]),
                tok(earlier, &[]),
                tok("gestern", &[]),
                tok(word, &[]),
                tok("zu", &[]),
                tok("kaufen", &[]),
            ];
            let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
            assert!(
                rule.is_exception(&refs, 5),
                "{earlier} ... {word} ... zu: Java's preposition walk must fire"
            );
        }
    }

    #[test]
    fn anti_patterns_tolerate_optional_adverbs() {
        let with_adverb: Vec<AnalyzedTokenReadings> = vec![
            tok("hin", &[]),
            tok("und", &[]),
            tok("wieder", &["ADV"]),
            tok("her", &[]),
            tok("zu", &[]),
            tok("gehen", &[]),
        ];
        let without_adverb: Vec<AnalyzedTokenReadings> = vec![
            tok("hin", &[]),
            tok("und", &[]),
            tok("her", &[]),
            tok("zu", &[]),
            tok("gehen", &[]),
        ];
        for tokens in [with_adverb, without_adverb] {
            let zu_index = tokens
                .iter()
                .position(|t| t.token().token() == "zu")
                .unwrap();
            let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
            assert!(token_matches_anti_pattern_at(&refs, zu_index));
        }
    }

    #[test]
    fn misspelling_check_splits_on_known_zu_compounds() {
        let rule = CompoundInfinitivRule::new(None);
        assert!(!rule.is_misspelled("haus"), "known words stay quiet");
        assert!(
            !rule.is_misspelled("anzuzeigen"),
            "an + zeigen around zu counts as known"
        );
        assert!(rule.is_misspelled("qzwuuvv"), "unknown junk is misspelled");
    }
}
