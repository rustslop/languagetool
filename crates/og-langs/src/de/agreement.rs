use super::helpers::{has_pos_tag_starting_with, lowercase_first_char, uppercase_first_char};
use og_core::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use og_tagger::morfologik::MorfologikDictionary;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

pub static AGREEMENT_ANTI_PATTERNS: LazyLock<Option<Vec<crate::engine::XmlPatternRule>>> =
    LazyLock::new(|| {
        let compiler = og_xml::XmlCompiler::new();
        let mut rules = Vec::new();
        for src in [
            include_str!("agreement_antipatterns.xml"),
            include_str!("agreement_antipatterns2.xml"),
            include_str!("agreement_antipatterns3.xml"),
        ] {
            match compiler.compile_file(src) {
                Ok(set) => rules.extend(
                    set.rules
                        .into_iter()
                        .map(crate::engine::XmlPatternRule::new),
                ),
                Err(_) => return None,
            }
        }
        Some(rules)
    });

pub fn immunized_indices(
    rules: &[crate::engine::XmlPatternRule],
    sentence: &AnalyzedSentence,
) -> HashSet<usize> {
    let tokens = sentence.non_whitespace_tokens();
    let mut set = HashSet::new();
    for rule in rules {
        for m in rule.match_sentence(sentence) {
            let from = m.offset();
            let to = from + m.length();
            for (i, t) in tokens.iter().enumerate() {
                if t.token().start() >= from && t.token().end() <= to {
                    set.insert(i);
                }
            }
        }
    }
    set
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kasus {
    Nominativ,
    Akkusativ,
    Dativ,
    Genitiv,
}

impl Kasus {
    fn name(self) -> &'static str {
        match self {
            Kasus::Nominativ => "Nominativ",
            Kasus::Akkusativ => "Akkusativ",
            Kasus::Dativ => "Dativ",
            Kasus::Genitiv => "Genitiv",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Numerus {
    Singular,
    Plural,
}

impl Numerus {
    fn name(self) -> &'static str {
        match self {
            Numerus::Singular => "Singular",
            Numerus::Plural => "Plural",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Genus {
    Maskulinum,
    Femininum,
    Neutrum,
    Allgemein,
}

impl Genus {
    fn name(self) -> &'static str {
        match self {
            Genus::Maskulinum => "Maskulinum",
            Genus::Femininum => "Femininum",
            Genus::Neutrum => "Neutrum",
            Genus::Allgemein => "Allgemein",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Determination {
    Definite,
    Indefinite,
}

impl Determination {
    fn name(self) -> &'static str {
        match self {
            Determination::Definite => "definit",
            Determination::Indefinite => "indefinit",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PosType {
    Nomen,
    Verb,
    Adjektiv,
    Determiner,
    Pronomen,
    Partizip,
    ProperNoun,
}

#[derive(Clone, Copy, Default, Debug)]
struct GermanReading {
    pos_type: Option<PosType>,
    casus: Option<Kasus>,
    numerus: Option<Numerus>,
    genus: Option<Genus>,
    determination: Option<Determination>,
}

fn parse_german_reading(pos_tag: Option<&str>) -> GermanReading {
    let Some(tag) = pos_tag else {
        return GermanReading::default();
    };
    let parts: Vec<&str> = tag.split(':').collect();
    if parts.len() < 3 {
        return GermanReading::default();
    }
    let mut r = GermanReading::default();
    for part in parts {
        match part {
            "EIG" => r.pos_type = Some(PosType::ProperNoun),
            "SUB" if r.pos_type.is_none() => r.pos_type = Some(PosType::Nomen),
            "PA1" | "PA2" => r.pos_type = Some(PosType::Partizip),
            "VER" if r.pos_type.is_none() => r.pos_type = Some(PosType::Verb),
            "ADJ" if r.pos_type.is_none() => r.pos_type = Some(PosType::Adjektiv),
            "PRO" if r.pos_type.is_none() => r.pos_type = Some(PosType::Pronomen),
            "ART" if r.pos_type.is_none() => r.pos_type = Some(PosType::Determiner),
            "AKK" => r.casus = Some(Kasus::Akkusativ),
            "GEN" => r.casus = Some(Kasus::Genitiv),
            "NOM" => r.casus = Some(Kasus::Nominativ),
            "DAT" => r.casus = Some(Kasus::Dativ),
            "PLU" => r.numerus = Some(Numerus::Plural),
            "SIN" => r.numerus = Some(Numerus::Singular),
            "MAS" => r.genus = Some(Genus::Maskulinum),
            "FEM" => r.genus = Some(Genus::Femininum),
            "NEU" => r.genus = Some(Genus::Neutrum),
            "NOG" => r.genus = Some(Genus::Femininum),
            "ALG" => r.genus = Some(Genus::Allgemein),
            "IND" => r.determination = Some(Determination::Indefinite),
            "DEF" => r.determination = Some(Determination::Definite),
            _ => {}
        }
    }
    r
}

pub(crate) fn has_reading_of_type(t: &AnalyzedTokenReadings, ty: PosType) -> bool {
    for reading in t.readings() {
        for tag in reading.pos_tags() {
            if tag == "SENT_END" {
                return false;
            }
            if parse_german_reading(Some(tag)).pos_type == Some(ty) {
                return true;
            }
        }
    }
    false
}

fn make_string(
    casus: Option<Kasus>,
    num: Option<Numerus>,
    gender: Option<Genus>,
    det: Option<Determination>,
) -> String {
    let mut l: Vec<&str> = Vec::new();
    if let Some(c) = casus {
        l.push(c.name());
    }
    if let Some(n) = num {
        l.push(n.name());
    }
    if let Some(g) = gender {
        l.push(g.name());
    }
    if let Some(d) = det {
        l.push(d.name());
    }
    l.join("/")
}

fn possessive_special_case(a_token: &AnalyzedTokenReadings, lemma: Option<&str>) -> bool {
    has_pos_tag_starting_with(a_token, "PRO:POS") && matches!(lemma, Some("ich") | Some("sich"))
}

pub fn get_agreement_categories(
    a_token: &AnalyzedTokenReadings,
    skip_sol: bool,
) -> HashSet<String> {
    let mut set = HashSet::new();
    for reading in a_token.readings() {
        for tmp_tag in reading.pos_tags() {
            if skip_sol && tmp_tag.ends_with(":SOL") {
                continue;
            }
            let gr = parse_german_reading(Some(tmp_tag));
            if gr.casus.is_none() && gr.numerus.is_none() && gr.genus.is_none() {
                continue;
            }
            let lemma = reading.lemma();
            if gr.genus == Some(Genus::Allgemein)
                && !tmp_tag.ends_with(":STV")
                && !possessive_special_case(a_token, lemma)
            {
                if gr.determination.is_none() {
                    for g in [Genus::Maskulinum, Genus::Femininum, Genus::Neutrum] {
                        for d in [Determination::Definite, Determination::Indefinite] {
                            set.insert(make_string(gr.casus, gr.numerus, Some(g), Some(d)));
                        }
                    }
                } else {
                    for g in [Genus::Maskulinum, Genus::Femininum, Genus::Neutrum] {
                        set.insert(make_string(gr.casus, gr.numerus, Some(g), gr.determination));
                    }
                }
            } else if gr.determination.is_none() || lemma == Some("jed") || lemma == Some("manch") {
                for d in [Determination::Definite, Determination::Indefinite] {
                    set.insert(make_string(gr.casus, gr.numerus, gr.genus, Some(d)));
                }
            } else {
                set.insert(make_string(
                    gr.casus,
                    gr.numerus,
                    gr.genus,
                    gr.determination,
                ));
            }
        }
    }
    set
}

static MIT_MIR_ETC: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^mit (mir|dir|ihm|ihr|ihnen|uns|euch)$").unwrap());
static OHNE_MICH_ETC: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^ohne (mich|dich|ihn|sie|uns|euch)$").unwrap());
static ZUGESCHRIEBENEN_GENANNTEN: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^zugeschriebenen?|genannten?$").unwrap());
static VIEL_WEIT: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^(viel|weit)$").unwrap());
static WENIGER_EHER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^(weniger|eher)$").unwrap());
static HERR_FRAU: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^(Herr|Frau)$").unwrap());

const MODIFIERS: &[&str] = &[
    "zu",
    "überraschend",
    "ungeahnt",
    "absolut",
    "ausgesprochen",
    "außergewöhnlich",
    "außerordentlich",
    "äußerst",
    "besonders",
    "dringend",
    "echt",
    "einigermaßen",
    "enorm",
    "extrem",
    "fast",
    "ganz",
    "entschieden",
    "geradezu",
    "zeitweise",
    "halbwegs",
    "höchst",
    "komplett",
    "laufend",
    "recht",
    "relativ",
    "sehr",
    "total",
    "überaus",
    "ungewöhnlich",
    "unglaublich",
    "völlig",
    "weit",
    "wirklich",
    "gerade",
    "vereint",
    "überwiegend",
    "gewollt",
    "angestrengt",
    "ziemlich",
];

const VIELE_WENIGE_LOWERCASE: &[&str] = &[
    "manche",
    "jegliche",
    "jeglicher",
    "andere",
    "anderer",
    "anderen",
    "sämtliche",
    "sämtlicher",
    "etliche",
    "etlicher",
    "viele",
    "vieler",
    "wenige",
    "weniger",
    "einige",
    "einiger",
    "mehrerer",
    "mehrere",
];

const REL_PRONOUN_LEMMAS: &[&str] = &["der", "welch"];

const PRONOUNS_TO_BE_IGNORED: &[&str] = &[
    "nichts",
    "alles",
    "dies",
    "ebendies",
    "ich",
    "dir",
    "dich",
    "du",
    "d",
    "er",
    "sie",
    "es",
    "wir",
    "mich",
    "mir",
    "uns",
    "ihnen",
    "euch",
    "ihm",
    "ihr",
    "ihn",
    "dessen",
    "deren",
    "denen",
    "sich",
    "aller",
    "allen",
    "man",
    "beide",
    "beiden",
    "beider",
    "wessen",
    "a",
    "alle",
    "etwas",
    "irgendetwas",
    "irgendwas",
    "irgendwer",
    "was",
    "wer",
    "wem",
    "jenen",
    "diejenigen",
    "irgendjemand",
    "irgendjemandes",
    "jemand",
    "jemandes",
    "niemand",
    "niemandes",
];

const NOUNS_TO_BE_IGNORED: &[&str] = &[
    "A",
    "Prozent",
    "Wollen",
    "Gramm",
    "Kilogramm",
    "Flippers",
    "Standart",
    "Stellungsname",
    "Kündigungsscheiben",
    "Piepen",
    "Badlands",
    "Visual",
    "Special",
    "Multiple",
    "Chief",
    "Carina",
    "Wüstenrot",
    "Rückgrad",
    "Rückgrads",
    "Anteilname",
    "Aalen",
    "Meter",
    "Boots",
    "Taxameter",
    "Bild",
    "Emirates",
    "Uhr",
    "cm",
    "km",
    "Nr",
    "KSC",
    "ANC",
    "DJK",
    "RP",
];

static HUNSPELL_WORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let mut words = HashSet::new();
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplacementType {
    Ins,
    Zur,
}

const MSG: &str = "Möglicherweise passen das Nomen und die Wörter, die das Nomen beschreiben, grammatisch nicht zusammen.";
const SHORT_MSG: &str = "Evtl. passen Wörter grammatisch nicht zusammen.";

pub struct AgreementRule {
    tagger: Option<Arc<dyn og_core::checker::Tagger>>,
    disambiguator: Option<Arc<dyn og_core::checker::Disambiguator>>,
    dictionary: Option<Arc<og_spell::Dictionary>>,
    synth_dictionary: Option<Arc<MorfologikDictionary>>,
}

impl AgreementRule {
    pub fn new(
        tagger: Option<Arc<dyn og_core::checker::Tagger>>,
        disambiguator: Option<Arc<dyn og_core::checker::Disambiguator>>,
        dictionary: Option<Arc<og_spell::Dictionary>>,
        synth_dictionary: Option<Arc<MorfologikDictionary>>,
    ) -> Self {
        Self {
            tagger,
            disambiguator,
            dictionary,
            synth_dictionary,
        }
    }

    fn is_known_word(&self, word: &str) -> bool {
        self.dictionary.as_ref().is_some_and(|d| d.contains(word))
            || HUNSPELL_WORDS.contains(word)
            || HUNSPELL_WORDS.contains(&lowercase_first_char(word))
    }

    fn analyze_phrase(&self, phrase: &str) -> AnalyzedSentence {
        use og_tokenizer::WordTokenizer as _;
        let tokenizer = og_tokenizer::DefaultWordTokenizer::new_generic();
        let mut tokens: Vec<AnalyzedTokenReadings> = tokenizer
            .tokenize(phrase)
            .into_iter()
            .map(|t| AnalyzedTokenReadings::new(AnalyzedToken::from_token(&t)))
            .collect();
        let start_token = AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        );
        tokens.insert(0, start_token);
        let mut sentence = AnalyzedSentence::new(phrase, 0, phrase.len());
        sentence.set_tokens(tokens);
        if let Some(tagger) = &self.tagger {
            let toks = sentence.tokens_mut();
            tagger.tag(toks);
        }
        if let Some(d) = &self.disambiguator {
            d.disambiguate(&mut sentence);
        }
        sentence
    }

    fn phrase_clean(&self, phrase: &str) -> bool {
        let sentence = self.analyze_phrase(phrase);
        let nw = sentence.non_whitespace_tokens();
        let immunized = AGREEMENT_ANTI_PATTERNS
            .as_ref()
            .map(|rules| immunized_indices(rules, &sentence))
            .unwrap_or_default();
        if !self
            .match_core(&sentence, &nw, &immunized, false)
            .is_empty()
        {
            return false;
        }
        nw.iter().all(|t| {
            let text = t.token().token();
            text == "<S>" || !text.chars().any(|c| c.is_alphabetic()) || self.is_known_word(text)
        })
    }

    fn match_core(
        &self,
        sentence: &AnalyzedSentence,
        tokens: &[&AnalyzedTokenReadings],
        immunized: &HashSet<usize>,
        check_compounds: bool,
    ) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let text = sentence.text();
        let text_base = sentence.start();
        let mut repl_map: HashMap<usize, ReplacementType> = HashMap::new();
        let original_tokens = tokens;
        let replaced: Vec<AnalyzedTokenReadings> = tokens
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let token_text = t.token().token();
                if matches!(
                    token_text,
                    "ins"
                        | "ans"
                        | "aufs"
                        | "vors"
                        | "durchs"
                        | "hinters"
                        | "unters"
                        | "übers"
                        | "fürs"
                        | "ums"
                ) {
                    repl_map.insert(i, ReplacementType::Ins);
                    AnalyzedTokenReadings::new(
                        AnalyzedToken::new("das", t.token().start(), t.token().end())
                            .with_pos_tags(vec!["ART:DEF:AKK:SIN:NEU".to_string()])
                            .with_lemma("das"),
                    )
                } else if token_text == "zur" {
                    repl_map.insert(i, ReplacementType::Zur);
                    AnalyzedTokenReadings::new(
                        AnalyzedToken::new("der", t.token().start(), t.token().end())
                            .with_pos_tags(vec!["ART:DEF:DAT:SIN:FEM".to_string()])
                            .with_lemma("der"),
                    )
                } else {
                    (*t).clone()
                }
            })
            .collect();
        let tokens: Vec<&AnalyzedTokenReadings> = replaced.iter().collect();
        for i in 0..tokens.len() {
            let first_tag = tokens[i]
                .readings()
                .first()
                .and_then(|r| r.pos_tags().first())
                .map(String::as_str);
            if first_tag == Some("SENT_START") || immunized.contains(&i) {
                continue;
            }
            if could_be_relative_or_dependent_clause(&tokens, i) {
                continue;
            }
            if i > 0 {
                let prev_token = tokens[i - 1].token().token().to_lowercase();
                if matches!(
                    prev_token.as_str(),
                    "der" | "die" | "das" | "des" | "dieses"
                ) && matches!(tokens[i].token().token(), "eine" | "einen")
                {
                    continue;
                }
            }
            let token_readings = tokens[i];
            let det_abbrev = i + 2 < tokens.len()
                && tokens[i + 1].token().token() == "Art"
                && tokens[i + 2].token().token() == ".";
            let det_adj_abbrev = i + 3 < tokens.len()
                && tokens[i + 2].token().token() == "Art"
                && tokens[i + 3].token().token() == ".";
            let following_participle = i + 3 < tokens.len()
                && (tokens[i + 2].has_partial_pos_tag("PA1")
                    || ZUGESCHRIEBENEN_GENANNTEN.is_match(tokens[i + 2].token().token()));
            if det_abbrev || det_adj_abbrev || following_participle {
                continue;
            }
            if has_reading_of_type(token_readings, PosType::Determiner)
                || is_relevant_pronoun(&tokens, i)
            {
                let token_pos_after_modifier = pos_after_modifier(i + 1, &tokens);
                let mut skipped_str: Option<String> = None;
                if token_pos_after_modifier > i + 1 {
                    let from = tokens[i + 1].token().start() - text_base;
                    let to = tokens[token_pos_after_modifier - 1].token().end() - text_base;
                    skipped_str = Some(text.get(from..to).unwrap_or("").to_string());
                }
                let mut token_pos = token_pos_after_modifier;
                if token_pos >= tokens.len() {
                    break;
                }
                let next_token = tokens[token_pos];
                let maybe_preposition = if i >= 1 { Some(tokens[i - 1]) } else { None };
                let maybe_preposition =
                    if i >= 2 && tokens[i - 2].token().token().eq_ignore_ascii_case("was") {
                        None
                    } else {
                        maybe_preposition
                    };
                if is_non_predicative_adjective(next_token) || is_participle(next_token) {
                    token_pos = token_pos_after_modifier + 1;
                    if token_pos >= tokens.len() {
                        break;
                    }
                    if has_reading_of_type(tokens[token_pos], PosType::Nomen) {
                        if i >= 2
                            && has_reading_of_type(tokens[i - 2], PosType::Adjektiv)
                            && tokens[i - 1].token().token() == "als"
                            && tokens[i].token().token() == "das"
                        {
                            continue;
                        }
                        let allow_suggestion = token_pos == i + 2;
                        if let Some(rm) = self.check_det_adj_noun_agreement(
                            maybe_preposition,
                            tokens[i],
                            next_token,
                            tokens[token_pos],
                            sentence,
                            i,
                            if allow_suggestion {
                                repl_map.get(&i).copied()
                            } else {
                                None
                            },
                            check_compounds,
                            skipped_str.as_deref(),
                            original_tokens,
                        ) {
                            rule_matches.push(rm);
                        }
                    } else if token_pos + 1 < tokens.len()
                        && has_reading_of_type(tokens[token_pos + 1], PosType::Nomen)
                        && has_reading_of_type(tokens[token_pos], PosType::Adjektiv)
                        && let Some(rm) = self.check_det_adj_adj_noun_agreement(
                            maybe_preposition,
                            tokens[i],
                            next_token,
                            tokens[token_pos],
                            tokens[token_pos + 1],
                            sentence,
                            i,
                            repl_map.get(&i).copied(),
                            skipped_str.as_deref(),
                            check_compounds,
                            original_tokens,
                        )
                    {
                        rule_matches.push(rm);
                    }
                } else if has_reading_of_type(next_token, PosType::Nomen)
                    && next_token.token().token() != "Herr"
                    && let Some(rm) = self.check_det_noun_agreement(
                        maybe_preposition,
                        tokens[i],
                        next_token,
                        sentence,
                        i,
                        repl_map.get(&i).copied(),
                        skipped_str.as_deref(),
                        check_compounds,
                        original_tokens,
                    )
                {
                    rule_matches.push(rm);
                }
            }
        }
        rule_matches
    }

    #[allow(clippy::too_many_arguments)]
    fn check_det_noun_agreement(
        &self,
        maybe_preposition: Option<&AnalyzedTokenReadings>,
        token1: &AnalyzedTokenReadings,
        token2: &AnalyzedTokenReadings,
        sentence: &AnalyzedSentence,
        token_pos: usize,
        replacement_type: Option<ReplacementType>,
        skipped_str: Option<&str>,
        check_compounds: bool,
        tokens: &[&AnalyzedTokenReadings],
    ) -> Option<RuleMatch> {
        if NOUNS_TO_BE_IGNORED.contains(&token2.token().token()) || token2.token().token() == "-" {
            return None;
        }
        let mut set1: HashSet<String> = if token1.readings().len() == 1
            && token1.readings()[0]
                .pos_tags()
                .first()
                .is_some_and(|t| t.ends_with(":STV"))
        {
            HashSet::new()
        } else {
            get_agreement_categories(token1, false)
        };
        let set2 = get_agreement_categories(token2, false);
        set1.retain(|s| set2.contains(s));
        if set1.is_empty() && !is_exception(token1, token2) {
            if check_compounds
                && let Some(compound_match) =
                    self.get_compound_error_det_noun(token1, token2, token_pos, sentence, tokens)
            {
                return Some(compound_match);
            }
            let rm_rule = RuleMatchRule::new("DE_AGREEMENT", self.description())
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str())
                .with_urls(vec![
                    "https://languagetool.org/insights/de/beitrag/deklination/".to_string(),
                ]);
            let from = token1.token().start();
            let to = token2.token().end();
            let mut rm = RuleMatch::new(
                MSG,
                from,
                to - from,
                rm_rule,
                RuleMatchContext::new(sentence.text().to_string(), from, to - from),
            )
            .with_short_message(SHORT_MSG)
            .with_sentence(sentence.text().to_string());
            let suggestions = self.suggest(
                maybe_preposition,
                token1,
                None,
                None,
                token2,
                replacement_type,
                skipped_str,
            );
            if !suggestions.is_empty() {
                rm = rm.with_replacements(
                    suggestions
                        .into_iter()
                        .map(SuggestedReplacement::new)
                        .collect(),
                );
            }
            return Some(rm);
        }
        None
    }

    fn get_compound_error_det_noun(
        &self,
        token1: &AnalyzedTokenReadings,
        token2: &AnalyzedTokenReadings,
        token_pos: usize,
        sentence: &AnalyzedSentence,
        tokens: &[&AnalyzedTokenReadings],
    ) -> Option<RuleMatch> {
        if token_pos + 2 >= tokens.len() {
            return None;
        }
        let next_token = tokens[token_pos + 2];
        if !starts_with_uppercase(next_token.token().token()) {
            return None;
        }
        if token2.token().start() == next_token.token().start() {
            return None;
        }
        let potential_compound = format!(
            "{}{}",
            token2.token().token(),
            lowercase_first_char(next_token.token().token())
        );
        let orig_token1 = tokens[token_pos].token().token();
        let test_phrase = format!("{orig_token1} {potential_compound}");
        let hyphen_potential_compound =
            format!("{}-{}", token2.token().token(), next_token.token().token());
        let hyphen_test_phrase = format!("{orig_token1} {hyphen_potential_compound}");
        self.get_rule_match_for_compound(
            token1,
            next_token,
            sentence,
            &test_phrase,
            &hyphen_test_phrase,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn check_det_adj_noun_agreement(
        &self,
        maybe_preposition: Option<&AnalyzedTokenReadings>,
        token1: &AnalyzedTokenReadings,
        token2: &AnalyzedTokenReadings,
        token3: &AnalyzedTokenReadings,
        sentence: &AnalyzedSentence,
        token_pos: usize,
        replacement_type: Option<ReplacementType>,
        check_compounds: bool,
        skipped_str: Option<&str>,
        tokens: &[&AnalyzedTokenReadings],
    ) -> Option<RuleMatch> {
        if token3.token().token().chars().count() < 2 {
            return None;
        }
        let set = retain_common_categories_3(token1, token2, token3);
        if set.is_empty() {
            if HERR_FRAU.is_match(token3.token().token()) && token_pos + 3 < tokens.len() {
                let token4 = tokens[token_pos + 3];
                if !is_tagged(token4) || has_pos_tag_starting_with(token4, "EIG:") {
                    return None;
                }
            }
            if check_compounds {
                if token_pos + 4 < tokens.len()
                    && let Some(compound_match) = self.get_compound_error_det_adj_adj_noun(
                        tokens[token_pos],
                        tokens[token_pos + 1],
                        tokens[token_pos + 2],
                        tokens[token_pos + 3],
                        token_pos,
                        None,
                        sentence,
                        tokens,
                    )
                {
                    return Some(compound_match);
                }
                if let Some(compound_match) = self.get_compound_error_det_adj_noun(
                    token1, token2, token3, token_pos, sentence, tokens,
                ) {
                    return Some(compound_match);
                }
            }
            if has_pos_tag_starting_with(token3, "ABK") {
                return None;
            }
            let rm_rule = RuleMatchRule::new("DE_AGREEMENT", self.description())
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str())
                .with_urls(vec![
                    "https://languagetool.org/insights/de/beitrag/deklination/".to_string(),
                ]);
            let from = token1.token().start();
            let to = token3.token().end();
            let mut rm = RuleMatch::new(
                MSG,
                from,
                to - from,
                rm_rule,
                RuleMatchContext::new(sentence.text().to_string(), from, to - from),
            )
            .with_short_message(SHORT_MSG)
            .with_sentence(sentence.text().to_string());
            let suggestions = self.suggest(
                maybe_preposition,
                token1,
                Some(token2),
                None,
                token3,
                replacement_type,
                skipped_str,
            );
            if !suggestions.is_empty() {
                rm = rm.with_replacements(
                    suggestions
                        .into_iter()
                        .map(SuggestedReplacement::new)
                        .collect(),
                );
            }
            return Some(rm);
        }
        None
    }

    fn get_compound_error_det_adj_noun(
        &self,
        token1: &AnalyzedTokenReadings,
        token2: &AnalyzedTokenReadings,
        token3: &AnalyzedTokenReadings,
        token_pos: usize,
        sentence: &AnalyzedSentence,
        tokens: &[&AnalyzedTokenReadings],
    ) -> Option<RuleMatch> {
        if token_pos + 3 >= tokens.len() {
            return None;
        }
        let next_token = tokens[token_pos + 3];
        if !starts_with_uppercase(next_token.token().token()) {
            return None;
        }
        if token3.token().start() == next_token.token().start() {
            return None;
        }
        let potential_compound = format!(
            "{}{}",
            token3.token().token(),
            lowercase_first_char(next_token.token().token())
        );
        let orig_token1 = tokens[token_pos].token().token();
        let test_phrase = format!(
            "{orig_token1} {} {potential_compound}",
            token2.token().token()
        );
        let hyphen_potential_compound =
            format!("{}-{}", token3.token().token(), next_token.token().token());
        let hyphen_test_phrase = format!(
            "{orig_token1} {} {hyphen_potential_compound}",
            token2.token().token()
        );
        self.get_rule_match_for_compound(
            token1,
            next_token,
            sentence,
            &test_phrase,
            &hyphen_test_phrase,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn get_compound_error_det_adj_adj_noun(
        &self,
        token1: &AnalyzedTokenReadings,
        token2: &AnalyzedTokenReadings,
        token3: &AnalyzedTokenReadings,
        token4: &AnalyzedTokenReadings,
        token_pos: usize,
        skipped_str: Option<&str>,
        sentence: &AnalyzedSentence,
        tokens: &[&AnalyzedTokenReadings],
    ) -> Option<RuleMatch> {
        let idx = token_pos + 4 + usize::from(skipped_str.is_some());
        if idx >= tokens.len() {
            return None;
        }
        let next_token = tokens[idx];
        if !starts_with_uppercase(token4.token().token())
            || !starts_with_uppercase(next_token.token().token())
        {
            return None;
        }
        if token4.token().start() == next_token.token().start() {
            return None;
        }
        let potential_compound = format!(
            "{}{}",
            token4.token().token(),
            lowercase_first_char(next_token.token().token())
        );
        let orig_token1 = tokens[token_pos].token().token();
        let skip_part = skipped_str
            .map(|s| format!(" {s} "))
            .unwrap_or_else(|| " ".to_string());
        let test_phrase = format!(
            "{orig_token1}{skip_part}{} {} {potential_compound}",
            token2.token().token(),
            token3.token().token()
        );
        let hyphen_potential_compound =
            format!("{}-{}", token4.token().token(), next_token.token().token());
        let hyphen_test_phrase = format!(
            "{orig_token1}{skip_part}{} {} {hyphen_potential_compound}",
            token2.token().token(),
            token3.token().token()
        );
        self.get_rule_match_for_compound(
            token1,
            next_token,
            sentence,
            &test_phrase,
            &hyphen_test_phrase,
        )
    }

    fn get_rule_match_for_compound(
        &self,
        token: &AnalyzedTokenReadings,
        token2: &AnalyzedTokenReadings,
        sentence: &AnalyzedSentence,
        test_phrase: &str,
        hyphen_test_phrase: &str,
    ) -> Option<RuleMatch> {
        let all_non_sub = token2
            .readings()
            .iter()
            .all(|r| r.pos_tags().iter().all(|t| !t.starts_with("SUB")));
        if all_non_sub {
            return None;
        }
        let mut replacements: Vec<String> = Vec::new();
        if self.phrase_clean(test_phrase) && is_tagged(token2) {
            replacements.push(test_phrase.to_string());
        }
        if self.phrase_clean(hyphen_test_phrase) && is_tagged(token2) {
            replacements.push(hyphen_test_phrase.to_string());
        }
        if !replacements.is_empty() {
            let message =
                "Wenn es sich um ein zusammengesetztes Nomen handelt, wird es zusammengeschrieben.";
            let from = token.token().start();
            let to = token2.token().end();
            let rm_rule = RuleMatchRule::new("DE_AGREEMENT", self.description())
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str())
                .with_urls(vec![
                    "https://dict.leo.org/grammatik/deutsch/Rechtschreibung/Regeln/Getrennt-zusammen/Nomen.html#grammarAnchor-Nomen-49575"
                        .to_string(),
                ]);
            return Some(
                RuleMatch::new(
                    message,
                    from,
                    to - from,
                    rm_rule,
                    RuleMatchContext::new(sentence.text().to_string(), from, to - from),
                )
                .with_replacements(
                    replacements
                        .into_iter()
                        .map(SuggestedReplacement::new)
                        .collect(),
                ),
            );
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn check_det_adj_adj_noun_agreement(
        &self,
        maybe_preposition: Option<&AnalyzedTokenReadings>,
        token1: &AnalyzedTokenReadings,
        token2: &AnalyzedTokenReadings,
        token3: &AnalyzedTokenReadings,
        token4: &AnalyzedTokenReadings,
        sentence: &AnalyzedSentence,
        token_pos: usize,
        replacement_type: Option<ReplacementType>,
        skipped_str: Option<&str>,
        check_compounds: bool,
        tokens: &[&AnalyzedTokenReadings],
    ) -> Option<RuleMatch> {
        let set = retain_common_categories_4(token1, token2, token3, token4);
        if set.is_empty() {
            if check_compounds
                && let Some(compound_match) = self.get_compound_error_det_adj_adj_noun(
                    token1,
                    token2,
                    token3,
                    token4,
                    token_pos,
                    skipped_str,
                    sentence,
                    tokens,
                )
            {
                return Some(compound_match);
            }
            if has_pos_tag_starting_with(token4, "ABK") {
                return None;
            }
            let rm_rule = RuleMatchRule::new("DE_AGREEMENT", self.description())
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str())
                .with_urls(vec![
                    "https://languagetool.org/insights/de/beitrag/deklination/".to_string(),
                ]);
            let from = token1.token().start();
            let to = token4.token().end();
            let mut rm = RuleMatch::new(
                MSG,
                from,
                to - from,
                rm_rule,
                RuleMatchContext::new(sentence.text().to_string(), from, to - from),
            )
            .with_short_message(SHORT_MSG)
            .with_sentence(sentence.text().to_string());
            if let Some(rt) = replacement_type {
                let suggestions = self.suggest(
                    maybe_preposition,
                    token1,
                    Some(token2),
                    Some(token3),
                    token4,
                    Some(rt),
                    skipped_str,
                );
                if !suggestions.is_empty() {
                    rm = rm.with_replacements(
                        suggestions
                            .into_iter()
                            .map(SuggestedReplacement::new)
                            .collect(),
                    );
                }
            }
            return Some(rm);
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn suggest(
        &self,
        maybe_preposition: Option<&AnalyzedTokenReadings>,
        determiner: &AnalyzedTokenReadings,
        adj1: Option<&AnalyzedTokenReadings>,
        adj2: Option<&AnalyzedTokenReadings>,
        noun: &AnalyzedTokenReadings,
        replacement_type: Option<ReplacementType>,
        skipped_str: Option<&str>,
    ) -> Vec<String> {
        AgreementSuggestor::new(
            self.synth_dictionary.clone(),
            determiner,
            adj1,
            adj2,
            noun,
            replacement_type,
        )
        .set_preposition(maybe_preposition)
        .set_skipped(skipped_str)
        .get_suggestions(true)
    }
}

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_uppercase())
}

pub(crate) fn starts_with_lowercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_lowercase())
}

fn is_tagged(t: &AnalyzedTokenReadings) -> bool {
    t.readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|tag| !tag.is_empty()))
}

trait PartialPosTag {
    fn has_partial_pos_tag(&self, needle: &str) -> bool;
}

impl PartialPosTag for AnalyzedTokenReadings {
    fn has_partial_pos_tag(&self, needle: &str) -> bool {
        self.readings()
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t.contains(needle)))
    }
}

fn is_non_predicative_adjective(t: &AnalyzedTokenReadings) -> bool {
    t.readings().iter().any(|r| {
        r.pos_tags()
            .iter()
            .any(|tag| tag.starts_with("ADJ") && !tag.contains("PRD"))
    })
}

fn is_participle(t: &AnalyzedTokenReadings) -> bool {
    t.has_partial_pos_tag("PA1") || t.has_partial_pos_tag("PA2")
}

fn is_relevant_pronoun(tokens: &[&AnalyzedTokenReadings], pos: usize) -> bool {
    let analyzed_token = tokens[pos];
    let mut relevant_pronoun = has_reading_of_type(analyzed_token, PosType::Pronomen);
    let token = tokens[pos].token().token();
    if PRONOUNS_TO_BE_IGNORED.contains(&token.to_lowercase().as_str())
        || (pos > 0
            && tokens[pos - 1].token().token().eq_ignore_ascii_case("vor")
            && token.eq_ignore_ascii_case("allem"))
    {
        relevant_pronoun = false;
    }
    relevant_pronoun
}

fn could_be_relative_or_dependent_clause(tokens: &[&AnalyzedTokenReadings], pos: usize) -> bool {
    if pos >= 1 {
        let comma = tokens[pos - 1].token().token() == ",";
        let rel_pronoun = comma
            && tokens[pos]
                .readings()
                .iter()
                .any(|r| r.lemma().is_some_and(|l| REL_PRONOUN_LEMMAS.contains(&l)));
        if rel_pronoun && pos + 3 < tokens.len() {
            return true;
        }
    }
    if pos >= 2 {
        let comma = tokens[pos - 2].token().token() == ",";
        if comma {
            let prep = has_pos_tag_starting_with(tokens[pos - 1], "PRP:");
            let rel_pronoun = tokens[pos]
                .readings()
                .iter()
                .any(|r| r.lemma().is_some_and(|l| REL_PRONOUN_LEMMAS.contains(&l)));
            return prep && rel_pronoun
                || (tokens[pos - 1].has_pos_tag("KON:UNT")
                    && (tokens[pos].has_lemma("jen")
                        || tokens[pos].has_lemma("dies")
                        || tokens[pos].has_lemma("ebendies")));
        }
    }
    false
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn pos_after_modifier(mut start_at: usize, tokens: &[&AnalyzedTokenReadings]) -> usize {
    if start_at < tokens.len()
        && tokens[start_at].token().token() == "relativ"
        && start_at + 1 < tokens.len()
        && tokens[start_at + 1].token().token() == "gesehen"
    {
        start_at += 2;
    }
    if start_at < tokens.len()
        && VIEL_WEIT.is_match(tokens[start_at].token().token())
        && start_at + 1 < tokens.len()
        && WENIGER_EHER.is_match(tokens[start_at + 1].token().token())
    {
        start_at += 2;
    } else if start_at + 1 < tokens.len() && MODIFIERS.contains(&tokens[start_at].token().token()) {
        start_at += 1;
    }
    if start_at + 1 < tokens.len() {
        let phrase = format!(
            "{} {}",
            tokens[start_at].token().token(),
            tokens[start_at + 1].token().token()
        )
        .to_lowercase();
        if MIT_MIR_ETC.is_match(&phrase) || OHNE_MICH_ETC.is_match(&phrase) {
            start_at += 2;
        }
    }
    if start_at + 1 < tokens.len()
        && (is_numeric(tokens[start_at].token().token()) || tokens[start_at].has_pos_tag("ZAL"))
    {
        let mut pos_after_modifier = start_at + 1;
        if start_at + 3 < tokens.len()
            && tokens[start_at + 1].token().token() == ","
            && is_numeric(tokens[start_at + 2].token().token())
        {
            pos_after_modifier = start_at + 3;
        }
        if ["gramm", "Gramm", "Meter", "meter"]
            .iter()
            .any(|s| tokens[pos_after_modifier].token().token().ends_with(s))
        {
            return pos_after_modifier + 1;
        }
    }
    start_at
}

fn is_exception(token1: &AnalyzedTokenReadings, token2: &AnalyzedTokenReadings) -> bool {
    token1.token().token() == "allen" && token2.token().token() == "Grund"
}

fn retain_common_categories_3(
    token1: &AnalyzedTokenReadings,
    token2: &AnalyzedTokenReadings,
    token3: &AnalyzedTokenReadings,
) -> HashSet<String> {
    let skip_sol =
        !VIELE_WENIGE_LOWERCASE.contains(&token1.token().token().to_lowercase().as_str());
    let mut set1 = get_agreement_categories(token1, skip_sol);
    let set2 = get_agreement_categories(token2, skip_sol);
    let set3 = get_agreement_categories(token3, true);
    set1.retain(|s| set2.contains(s) && set3.contains(s));
    set1
}

fn retain_common_categories_4(
    token1: &AnalyzedTokenReadings,
    token2: &AnalyzedTokenReadings,
    token3: &AnalyzedTokenReadings,
    token4: &AnalyzedTokenReadings,
) -> HashSet<String> {
    let skip_sol =
        !VIELE_WENIGE_LOWERCASE.contains(&token1.token().token().to_lowercase().as_str());
    let mut set1 = get_agreement_categories(token1, skip_sol);
    let set2 = get_agreement_categories(token2, skip_sol);
    let set3 = get_agreement_categories(token3, skip_sol);
    let set4 = get_agreement_categories(token4, true);
    set1.retain(|s| set2.contains(s) && set3.contains(s) && set4.contains(s));
    set1
}

pub(crate) const SYNTH_REMOVE: &[&str] = &[
    "unsren",
    "unsrem",
    "unsres",
    "unsre",
    "unsern",
    "unserm",
    "unsrer",
    "angepaßt",
    "beschloß",
    "biß",
    "entschloß",
    "ergoß",
    "faßt",
    "genoß",
    "paßt",
    "paßte",
    "preßt",
    "preßte",
    "riß",
    "schloß",
    "streßtest",
    "vergißt",
    "verlaß",
    "verläßt",
    "vermiß",
    "vermißt",
    "wißt",
    "wußtest",
    "wüßtest",
];

fn preposition_cases(prep: &str) -> Option<&'static [&'static str]> {
    Some(match prep {
        "ab" => &["DAT"],
        "abseits" | "abzüglich" | "angesichts" | "anhand" | "anlässlich" | "anstatt"
        | "anstelle" | "aufgrund" | "ausgangs" | "ausschließlich" | "ausweislich" | "außerhalb"
        | "bar" | "behufs" | "beidseits" | "betreffs" | "bezüglich" | "diesseits" | "eingangs"
        | "eingedenk" | "einschließlich" | "exklusive" | "fernab" | "halber" | "hinsichtlich"
        | "in puncto" | "infolge" | "inklusive" | "inmitten" | "innerhalb" | "innert"
        | "jenseits" | "kraft" | "längs" | "längsseits" | "mangels" | "mittels" | "namens"
        | "oberhalb" | "rücksichtlich" | "seitens" | "seitlich" | "statt" | "trotz"
        | "unbeschadet" | "uneingedenk" | "ungeachtet" | "unterhalb" | "unweit" | "vermittels"
        | "vermittelst" | "vermöge" | "vorbehaltlich" | "wegen" | "weitab" | "während" | "zeit"
        | "zugunsten" | "zuungunsten" | "zuzüglich" => &["GEN"],
        "an" | "auf" | "entlang" | "hinter" | "neben" | "unter" | "vor" | "über" | "zwischen" => {
            &["DAT", "AKK"]
        }
        "ausgenommen" | "betreffend" | "bis" | "bis auf" | "durch" | "je" | "kontra" | "ohne"
        | "per" | "pro" | "um" | "versus" | "via" | "wider" => &["AKK"],
        "aus" | "außer" | "bei" | "binnen" | "entgegen" | "entsprechend" | "fern" | "gegenüber"
        | "gemäß" | "getreu" | "mit" | "mitsamt" | "nach" | "nebst" | "nächst" | "samt"
        | "seit" | "vis-à-vis" | "von" | "zu" | "zufolge" | "zuliebe" | "zuwider" => &["DAT"],
        "dank" => &["DAT", "GEN"],
        "gegen" => &["AKK"],
        "laut" => &["GEN", "DAT"],
        _ => return None,
    })
}

struct AgreementSuggestor<'a> {
    synth_dictionary: Option<Arc<MorfologikDictionary>>,
    determiner: &'a AnalyzedTokenReadings,
    adj1: Option<&'a AnalyzedTokenReadings>,
    adj2: Option<&'a AnalyzedTokenReadings>,
    noun: &'a AnalyzedTokenReadings,
    replacement_type: Option<ReplacementType>,
    preposition: Option<&'a AnalyzedTokenReadings>,
    skipped: Option<&'a str>,
    orig_phrase: String,
}

struct Suggestion {
    phrase: String,
    token_level_edits: i32,
    char_level_edits: i32,
}

pub(crate) fn levenshtein(a: &str, b: &str) -> i32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<i32> = (0..=b.len() as i32).collect();
    let mut cur = vec![0i32; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i as i32;
        for j in 1..=b.len() {
            let cost = i32::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

impl<'a> AgreementSuggestor<'a> {
    fn new(
        synth_dictionary: Option<Arc<MorfologikDictionary>>,
        determiner: &'a AnalyzedTokenReadings,
        adj1: Option<&'a AnalyzedTokenReadings>,
        adj2: Option<&'a AnalyzedTokenReadings>,
        noun: &'a AnalyzedTokenReadings,
        replacement_type: Option<ReplacementType>,
    ) -> Self {
        let mut orig = determiner.token().token().to_string();
        if let Some(a) = adj1 {
            orig.push(' ');
            orig.push_str(a.token().token());
        }
        if let Some(a) = adj2 {
            orig.push(' ');
            orig.push_str(a.token().token());
        }
        orig.push(' ');
        orig.push_str(noun.token().token());
        Self {
            synth_dictionary,
            determiner,
            adj1,
            adj2,
            noun,
            replacement_type,
            preposition: None,
            skipped: None,
            orig_phrase: orig,
        }
    }

    fn set_preposition(mut self, prep: Option<&'a AnalyzedTokenReadings>) -> Self {
        self.preposition = prep;
        self
    }

    fn set_skipped(mut self, skipped: Option<&'a str>) -> Self {
        self.skipped = skipped;
        self
    }

    fn synthesize(&self, lemma: &str, pos: &str) -> Vec<String> {
        let Some(dict) = &self.synth_dictionary else {
            return Vec::new();
        };
        let Some(entries) = dict.forms_by_lemma().get(lemma) else {
            return Vec::new();
        };
        let lc_lemma = starts_with_lowercase(lemma);
        let mut out: Vec<String> = Vec::new();
        for (tag, word) in entries {
            if &**tag != pos {
                continue;
            }
            if (starts_with_lowercase(word) == lc_lemma || lemma == "mein")
                && !SYNTH_REMOVE.contains(&&**word)
                && !out.iter().any(|w| w == &**word)
            {
                out.push(word.to_string());
            }
        }
        out
    }

    fn get_suggestions(&mut self, filter: bool) -> Vec<String> {
        if self.replacement_type == Some(ReplacementType::Zur) {
            self.preposition = Some(dummy_prep("zu"));
        } else if self.replacement_type == Some(ReplacementType::Ins) {
            self.preposition = Some(dummy_prep("in"));
        }
        let mut suggestions = self.get_suggestions_internal();
        suggestions.sort_by(|a, b| {
            (a.token_level_edits, a.char_level_edits)
                .cmp(&(b.token_level_edits, b.char_level_edits))
        });
        self.add_contraction(&mut suggestions);
        if filter {
            let mut filtered: Vec<Suggestion> = Vec::new();
            let prev_corrections = suggestions
                .first()
                .map(|s| s.token_level_edits)
                .unwrap_or(0);
            let mut had_real = false;
            for s in suggestions {
                if had_real && s.token_level_edits > prev_corrections {
                    break;
                }
                had_real = s.token_level_edits > 0;
                filtered.push(s);
            }
            filtered.into_iter().map(|s| s.phrase).collect()
        } else {
            suggestions.into_iter().map(|s| s.phrase).collect()
        }
    }

    fn add_contraction(&self, suggestions: &mut Vec<Suggestion>) {
        if self.replacement_type == Some(ReplacementType::Zur) {
            for k in suggestions.iter_mut() {
                if k.phrase.starts_with("der") {
                    k.phrase = k.phrase.replacen("der", "zur", 1);
                } else if k.phrase.starts_with("den") {
                    k.phrase = k.phrase.replacen("den", "zu", 1);
                } else if k.phrase.starts_with("dem") {
                    k.phrase = k.phrase.replacen("dem", "zum", 1);
                }
            }
        } else if self.replacement_type == Some(ReplacementType::Ins) {
            suggestions.retain_mut(|k| {
                if k.phrase.starts_with("das") {
                    k.phrase = k.phrase.replacen("das", "ins", 1);
                    true
                } else if k.phrase.starts_with("dem") {
                    k.phrase = k.phrase.replacen("dem", "im", 1);
                    true
                } else if k.phrase.starts_with("den") {
                    k.phrase = k.phrase.replacen("den", "in den", 1);
                    true
                } else if k.phrase.starts_with("die") {
                    k.phrase = k.phrase.replacen("die", "in die", 1);
                    true
                } else {
                    false
                }
            });
        }
    }

    fn noun_cases(&self) -> Vec<&'static str> {
        if let Some(prep) = self.preposition {
            let lowered = prep.token().token().to_lowercase();
            if let Some(cases) = preposition_cases(&lowered) {
                return cases.to_vec();
            }
        }
        vec!["NOM", "AKK", "DAT", "GEN"]
    }

    fn get_suggestions_internal(&mut self) -> Vec<Suggestion> {
        let noun_cases = self.noun_cases();
        let mut result: Vec<Suggestion> = Vec::new();
        for num in ["SIN", "PLU"] {
            for gender in ["MAS", "FEM", "NEU", "NOG"] {
                for a_case in ["NOM", "AKK", "DAT", "GEN"] {
                    if !noun_cases.contains(&a_case) {
                        continue;
                    }
                    for det_reading in self.determiner.readings() {
                        for det_tag in det_reading.pos_tags() {
                            let (
                                det_synthesized,
                                adj1_synthesized,
                                adj2_synthesized,
                                noun_synthesized,
                            ) = if gender == "NOG" {
                                (
                                    self.get_det_or_pronoun_synth(
                                        num,
                                        "MAS",
                                        a_case,
                                        det_tag,
                                        det_reading,
                                    ),
                                    self.get_adj_synth(num, "MAS", a_case, self.adj1, det_tag),
                                    self.get_adj_synth(num, "MAS", a_case, self.adj2, det_tag),
                                    self.get_noun_synth(num, "NOG", a_case),
                                )
                            } else {
                                (
                                    self.get_det_or_pronoun_synth(
                                        num,
                                        gender,
                                        a_case,
                                        det_tag,
                                        det_reading,
                                    ),
                                    self.get_adj_synth(num, gender, a_case, self.adj1, det_tag),
                                    self.get_adj_synth(num, gender, a_case, self.adj2, det_tag),
                                    self.get_noun_synth(num, gender, a_case),
                                )
                            };
                            self.combine_synth(
                                &mut result,
                                &det_synthesized,
                                &adj1_synthesized,
                                &adj2_synthesized,
                                &noun_synthesized,
                            );
                        }
                    }
                }
            }
        }
        result
    }

    fn get_det_or_pronoun_synth(
        &self,
        num: &str,
        gender: &str,
        a_case: &str,
        det_tag: &str,
        det_reading: &og_core::AnalyzedToken,
    ) -> Vec<String> {
        static DERSELBEN: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r"^([Dd]as|[Dd]er|[Dd]ie|[Dd]em|[Dd]es)selben?$").unwrap()
        });
        static WELCHER: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"^[Ww]elche[nmsr]?$").unwrap());
        let det_pos = det_tag;
        let is_def = det_pos.contains(":DEF:");
        let det_token = det_reading.token();
        let own_lemma = || det_reading.lemma().unwrap_or(det_token).to_string();
        let (templates, lemma): (Vec<&str>, String) = if DERSELBEN.is_match(det_token) {
            (
                vec!["PRO:DEM:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU"],
                own_lemma(),
            )
        } else if WELCHER.is_match(det_token) {
            (
                vec!["PRO:RIN:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU:B/S"],
                own_lemma(),
            )
        } else if det_pos.contains("ART:") {
            (
                vec!["ART:IND/DEF:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU"],
                own_lemma(),
            )
        } else if det_pos.contains("PRO:POS:") {
            (
                vec![
                    "PRO:POS:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU:BEG",
                    "PRO:POS:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU:B/S",
                ],
                own_lemma(),
            )
        } else if det_pos.contains("PRO:DEM:") {
            (
                vec![
                    "PRO:DEM:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU:BEG",
                    "PRO:DEM:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU:B/S",
                ],
                own_lemma(),
            )
        } else if det_pos.contains("PRO:IND:") {
            (
                vec![
                    "PRO:IND:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU:BEG",
                    "PRO:IND:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU:B/S",
                ],
                own_lemma(),
            )
        } else if det_token == "zur" {
            (
                vec!["ART:IND/DEF:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU"],
                "der".to_string(),
            )
        } else if det_token == "ins" {
            (
                vec!["ART:IND/DEF:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU"],
                "das".to_string(),
            )
        } else {
            return Vec::new();
        };
        let mut synthesized: Vec<String> = Vec::new();
        let orig_first_char = det_token.chars().next().unwrap_or_default().to_string();
        for template in templates {
            let template = template.replacen("IND/DEF", if is_def { "DEF" } else { "IND" }, 1);
            let pos = replace_vars(&template, num, gender, a_case);
            for k in self.synthesize(&lemma, &pos) {
                if !k
                    .to_lowercase()
                    .starts_with(&orig_first_char.to_lowercase())
                {
                    continue;
                }
                if SYNTH_REMOVE.contains(&k.to_lowercase().as_str()) {
                    continue;
                }
                let k = if orig_first_char
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase())
                {
                    uppercase_first_char(&k)
                } else {
                    k
                };
                synthesized.push(k);
            }
        }
        synthesized
    }

    fn get_adj_synth(
        &self,
        num: &str,
        gender: &str,
        a_case: &str,
        adj_token: Option<&AnalyzedTokenReadings>,
        det_tag: &str,
    ) -> Vec<String> {
        let mut adj_synthesized: Vec<String> = Vec::new();
        if let Some(adj_token) = adj_token {
            for adj_reading in adj_token.readings() {
                for adj_tag in adj_reading.pos_tags() {
                    if det_tag.is_empty() {
                        continue;
                    }
                    if adj_reading.token() == "meisten" && num == "SIN" {
                        continue;
                    }
                    if adj_tag.starts_with("ADV:") {
                        adj_synthesized.push(adj_reading.token().to_string());
                        continue;
                    }
                    let det_is_def = det_tag.contains(":DEF:") || det_tag == "ins";
                    let template = if adj_tag.starts_with("PA1") {
                        "PA1:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU:GRU:IND/DEF:VER"
                    } else if adj_tag.starts_with("PA2") {
                        "PA2:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU:GRU:IND/DEF:VER"
                    } else {
                        "ADJ:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU:GRU:IND/DEF"
                    };
                    let template = if adj_tag.contains(":KOM:") {
                        template.replace(":GRU:", ":KOM:")
                    } else if adj_tag.contains(":SUP:") {
                        template.replace(":GRU:", ":SUP:")
                    } else {
                        template.to_string()
                    };
                    let template =
                        template.replacen("IND/DEF", if det_is_def { "DEF" } else { "IND" }, 1);
                    let adj_pos = replace_vars(&template, num, gender, a_case);
                    let lemma = adj_reading.lemma().unwrap_or(adj_reading.token());
                    for s in self.synthesize(lemma, &adj_pos) {
                        if !adj_synthesized.contains(&s) {
                            adj_synthesized.push(s);
                        }
                    }
                }
            }
        } else {
            adj_synthesized.push(String::new());
        }
        adj_synthesized
    }

    fn get_noun_synth(&self, num: &str, gender: &str, a_case: &str) -> Vec<String> {
        let mut result: Vec<String> = Vec::new();
        for noun_reading in self.noun.readings() {
            for noun_template in [
                "SUB:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU",
                "SUB:NOM/AKK/DAT/GEN:SIN/PLU:MAS/FEM/NEU:INF",
            ] {
                let noun_pos = replace_vars(noun_template, num, gender, a_case);
                let lemma = noun_reading.lemma().unwrap_or(noun_reading.token());
                let noun_synthesized = self.synthesize(lemma, &noun_pos);
                if noun_synthesized.is_empty() && noun_reading.token().contains('-') {
                    let token = noun_reading.token();
                    let first_part =
                        &token[..token.rfind('-').map(|i| i + 1).unwrap_or(token.len())];
                    let last_lemma_part = noun_reading
                        .lemma()
                        .map(|l| l.rsplit('-').next().unwrap_or(l))
                        .unwrap_or("");
                    for last_part_inflected in self.synthesize(last_lemma_part, &noun_pos) {
                        result.push(format!("{first_part}{last_part_inflected}"));
                    }
                } else {
                    result.extend(noun_synthesized);
                }
            }
        }
        let old_spelling: HashSet<String> = result
            .iter()
            .filter(|k| k.contains("ss"))
            .map(|k| k.replace("ss", "ß"))
            .collect();
        result.retain(|k| !old_spelling.contains(k));
        result
    }

    fn combine_synth(
        &self,
        result: &mut Vec<Suggestion>,
        det_synthesized: &[String],
        adj1_synthesized: &[String],
        adj2_synthesized: &[String],
        noun_synthesized: &[String],
    ) {
        for det in det_synthesized {
            for adj1 in adj1_synthesized {
                for adj2 in adj2_synthesized {
                    for noun in noun_synthesized {
                        let mut elem = det.clone();
                        if let Some(skipped) = self.skipped {
                            elem.push(' ');
                            elem.push_str(skipped);
                        }
                        if !adj1.is_empty() {
                            elem.push(' ');
                            elem.push_str(adj1);
                        }
                        if !adj2.is_empty() {
                            elem.push(' ');
                            elem.push_str(adj2);
                        }
                        elem.push(' ');
                        elem.push_str(noun);
                        let edits = i32::from(det != self.determiner.token().token())
                            + i32::from(self.adj1.is_none_or(|a| adj1 == a.token().token()))
                            + i32::from(self.adj2.is_none_or(|a| adj2 == a.token().token()))
                            + i32::from(noun != self.noun.token().token());
                        if edits == 0 {
                            continue;
                        }
                        let char_level_edits = levenshtein(&elem, &self.orig_phrase);
                        if !result
                            .iter()
                            .any(|s| s.token_level_edits == edits && s.phrase == elem)
                        {
                            result.push(Suggestion {
                                phrase: elem,
                                token_level_edits: edits,
                                char_level_edits,
                            });
                        }
                    }
                }
            }
        }
    }
}

fn dummy_prep(text: &'static str) -> &'static AnalyzedTokenReadings {
    static ZU: LazyLock<AnalyzedTokenReadings> = LazyLock::new(|| {
        AnalyzedTokenReadings::new(AnalyzedToken::new("zu", 0, 0).with_lemma("zu"))
    });
    static IN: LazyLock<AnalyzedTokenReadings> = LazyLock::new(|| {
        AnalyzedTokenReadings::new(AnalyzedToken::new("in", 0, 0).with_lemma("in"))
    });
    if text == "zu" { &ZU } else { &IN }
}

fn replace_vars(template: &str, num: &str, gender: &str, a_case: &str) -> String {
    template
        .replacen("SIN/PLU", num, 1)
        .replacen("MAS/FEM/NEU", gender, 1)
        .replacen("NOM/AKK/DAT/GEN", a_case, 1)
}

impl Rule for AgreementRule {
    fn id(&self) -> &str {
        "DE_AGREEMENT"
    }

    fn description(&self) -> &str {
        "Kongruenz von Nominalphrasen (unvollständig!), z.B. 'mein kleiner (kleines) Haus'"
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

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let immunized = AGREEMENT_ANTI_PATTERNS
            .as_ref()
            .map(|rules| immunized_indices(rules, sentence))
            .unwrap_or_default();
        self.match_core(sentence, &tokens, &immunized, true)
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::LanguageEngine;

    fn agreement_family(text: &str) -> Vec<(String, usize, Vec<String>)> {
        let engine = LanguageEngine::shared_for_code("de").expect("german engine");
        engine
            .check(text)
            .matches
            .iter()
            .filter(|m| {
                m.rule().id() == "DE_AGREEMENT" || m.rule().id().starts_with("SIMPLE_AGREEMENT")
            })
            .map(|m| {
                (
                    m.rule().id().to_string(),
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
    fn java_good_sentences_stay_clean() {
        use rayon::prelude::*;
        let offenders: Vec<String> = [
            "Bis zur Anfang Juni geplanten Eröffnung gebe es noch einiges zu tun.",
            "Der fließend Französisch sprechende Präsident dankt stilvoll ab.",
            "Spricht dieser fließend Französisch, muss er viel Geld verdienen.",
            "Der letzte Woche beschlossene Etat ist unwirksam.",
            "Die Einen sagen dies, die Anderen das.",
            "So ist es in den USA.",
            "Das ist der Tisch.",
            "Das ist das Auto der Frau.",
            "Das gehört dem Mann.",
            "Das Auto eines Mannes.",
            "Des großen Mannes.",
            "Sie wiesen dieselben Verzierungen auf.",
            "Die erwähnte Konferenz ist am Samstag.",
            "Sie erreichten mehrere Prozent Zustimmung.",
            "Ich tat für ihn, was dieser andere Autor für ihn tat.",
            "Ich tat für ihn, was jedes andere Kind für ihn tat.",
            "Klebe ein Preisschild auf jedes einzelne Produkt.",
            "Mancher ausscheidende Politiker hinterlässt eine Lücke.",
            "Kern einer jeden Tragödie ist es, ..",
            "Das viele Geld wird ihr helfen.",
            "Er verspricht jedem hohe Gewinne.",
            "Sind das Eier aus Bodenhaltung?",
            "Das Orange ist schön.",
            "Dieses rötliche Orange gefällt mir am besten.",
            "Das ist ein super Tipp.",
            "Sie kann einem Angst einjagen.",
            "Nichts ist mit dieser einen Nacht zu vergleichen.",
            "Das Dach von meinem Auto.",
            "Das Dach von meinen Autos.",
            "Das Dach meines Autos.",
            "Das Dach meiner großen Autos.",
            "Es war Karl, dessen Leiche Donnerstag gefunden wurde.",
            "Erst recht wir fleißigen Arbeiter.",
            "Aller Kommunikation liegt dies zugrunde.",
            "Der eine große Polizist rief dem anderen zu...",
            "Das erfordert Können.",
            "Ist das Kunst oder Abfall?",
            "In den 80er Jahren.",
            "Wir machen das Januar.",
            "Wir teilen das Morgen mit.",
            "Können Sie das nächsten Monat erledigen?",
            "War das Absicht?",
            "Alles Große und Edle ist einfacher Art.",
            "Es gibt ein Sprichwort, dem zufolge der tägliche Genuss einer Mandel dem Gedächtnis förderlich sei.",
            "Dieses Bild stammt von einem lange Zeit unbekannten Maler.",
            "Das Staatsoberhaupt ist der Verfassung zufolge der König.",
            "Der Mitte 2001 umgestaltete Garten ist schön.",
            "Das Wahlrecht, das Frauen damals zugesprochen bekamen.",
            "Es birgt für mich ein zu hohes juristisches Risiko.",
            "Die Beibehaltung des Art. 1 ist geplant.",
            "Picassos Kunstwerke werden häufiger gestohlen als die anderer Künstler.",
            "Er war müde vom vielen Laufen.",
            "Sind das echte Diamanten?",
        ]
        .into_par_iter()
        .filter_map(|text| {
            let matches = agreement_family(text);
            (!matches.is_empty()).then(|| format!("{text}: got {matches:?}"))
        })
        .collect();
        assert!(offenders.is_empty(), "{}", offenders.join("\n"));
    }

    #[test]
    fn java_bad_sentences_flag_the_nominal_phrase() {
        for text in [
            "Ein Buch mit einem ganz ähnlichem Titel.",
            "Meiner Chef raucht.",
            "Die Galerie zu den Bilder findet sich hier.",
            "Die erwähnt Konferenz ist am Samstag.",
            "Die erwähntes Konferenz ist am Samstag.",
            "Die erwähnten Konferenz ist am Samstag.",
            "Die erwähnter Konferenz ist am Samstag.",
            "Die erwähntem Konferenz ist am Samstag.",
            "In diesen Viertel bin ich aufgewachsen.",
            "Im November wurde auf den Gelände der Wettbewerb ausgetragen.",
            "Sie hatte das kleinen Kaninchen.",
            "Frau Müller hat das wichtigen Dokument gefunden.",
            "Es birgt für mich ein zu hohes juristische Risiko.",
        ] {
            let matches = agreement_family(text);
            assert!(!matches.is_empty(), "{text}: expected a DE_AGREEMENT match");
        }
    }

    #[test]
    fn java_bad_sentences_pin_the_suggestion_lists() {
        for (text, expected) in [
            (
                "Es sind die Tisch.",
                vec!["die Tische", "der Tisch", "den Tisch", "dem Tisch"],
            ),
            (
                "Es sind das Tisch.",
                vec!["der Tisch", "den Tisch", "dem Tisch"],
            ),
            (
                "Es sind die Haus.",
                vec!["das Haus", "dem Haus", "die Häuser"],
            ),
            (
                "Es sind der Haus.",
                vec!["dem Haus", "das Haus", "der Häuser"],
            ),
            ("Es sind das Frau.", vec!["die Frau", "der Frau"]),
            (
                "Das Auto des Mann.",
                vec![
                    "der Mann",
                    "den Mann",
                    "dem Mann",
                    "des Manns",
                    "des Mannes",
                ],
            ),
            (
                "Das interessiert das Mann.",
                vec!["der Mann", "den Mann", "dem Mann"],
            ),
            (
                "Das interessiert die Mann.",
                vec!["der Mann", "den Mann", "dem Mann", "die Männer"],
            ),
            ("Das Auto ein Mannes.", vec!["ein Mann", "eines Mannes"]),
            ("Das Auto einer Mannes.", vec!["eines Mannes"]),
            ("Das Auto einen Mannes.", vec!["eines Mannes", "einen Mann"]),
            (
                "Ich habe einen Feder gefunden.",
                vec!["eine Feder", "einer Feder"],
            ),
            (
                "Das Dach mein Autos.",
                vec![
                    "mein Auto",
                    "meine Autos",
                    "meines Autos",
                    "meinen Autos",
                    "meiner Autos",
                ],
            ),
        ] {
            let matches = agreement_family(text);
            let de_matches: Vec<&(String, usize, Vec<String>)> = matches
                .iter()
                .filter(|(id, _, _)| id == "DE_AGREEMENT")
                .collect();
            assert_eq!(
                de_matches.len(),
                1,
                "{text}: expected exactly one DE_AGREEMENT match, got {matches:?}"
            );
            assert_eq!(
                de_matches[0].2, expected,
                "{text}: suggestion list diverges from Java"
            );
        }
    }

    #[test]
    fn java_compound_matches_suggest_the_closed_forms() {
        for (text, expected) in [
            (
                "Wir gehen ins Fitness Studio",
                vec!["ins Fitnessstudio", "ins Fitness-Studio"],
            ),
            (
                "Mit seinem Konkurrent Alistair Müller",
                vec!["seinem Konkurrenten"],
            ),
        ] {
            let matches = agreement_family(text);
            let de_matches: Vec<&(String, usize, Vec<String>)> = matches
                .iter()
                .filter(|(id, _, _)| id == "DE_AGREEMENT")
                .collect();
            assert_eq!(
                de_matches.len(),
                1,
                "{text}: expected exactly one DE_AGREEMENT match, got {matches:?}"
            );
            assert_eq!(
                de_matches[0].2, expected,
                "{text}: compound suggestion list diverges from Java"
            );
        }
    }

    #[test]
    fn compound_det_adj_noun_suggests_the_closed_forms() {
        let matches = agreement_family("Wir gehen in dem neuen Fitness Studio.");
        assert_eq!(
            matches,
            vec![(
                "DE_AGREEMENT".to_string(),
                13,
                vec![
                    "dem neuen Fitnessstudio".to_string(),
                    "dem neuen Fitness-Studio".to_string()
                ]
            )]
        );
        let matches = agreement_family("Das ist ein interessantes Fitness Studio.");
        assert_eq!(
            matches,
            vec![(
                "DE_AGREEMENT".to_string(),
                8,
                vec![
                    "ein interessantes Fitnessstudio".to_string(),
                    "ein interessantes Fitness-Studio".to_string()
                ]
            )]
        );
    }

    #[test]
    fn compound_det_adj_adj_noun_suggests_the_closed_forms() {
        let matches = agreement_family("Wir gehen in dem neuen großen Fitness Studio.");
        assert_eq!(
            matches,
            vec![(
                "DE_AGREEMENT".to_string(),
                13,
                vec![
                    "dem neuen großen Fitnessstudio".to_string(),
                    "dem neuen großen Fitness-Studio".to_string()
                ]
            )]
        );
        let matches = agreement_family("Das ist ein wirklich schönes Fitness Studio.");
        assert_eq!(
            matches,
            vec![(
                "DE_AGREEMENT".to_string(),
                8,
                vec![
                    "ein wirklich schönes Fitnessstudio".to_string(),
                    "ein wirklich schönes Fitness-Studio".to_string()
                ]
            )],
            "the skipped adverb must stay inside the compound test phrase"
        );
    }

    #[test]
    fn zur_contraction_rewrites_the_article_leads() {
        let matches = agreement_family("Wir fahren zur großen Städten.");
        assert_eq!(
            matches,
            vec![(
                "DE_AGREEMENT".to_string(),
                11,
                vec![
                    "zu großen Städten".to_string(),
                    "zur großen Stadt".to_string()
                ]
            )]
        );
        let matches = agreement_family("Wir fahren zur alten Häusern.");
        assert_eq!(
            matches,
            vec![(
                "DE_AGREEMENT".to_string(),
                11,
                vec!["zu alten Häusern".to_string()]
            )]
        );
    }

    #[test]
    fn ins_contraction_keeps_only_article_led_suggestions() {
        let matches = agreement_family("Wir gehen ins großen Fitness Studio.");
        assert_eq!(
            matches,
            vec![("DE_AGREEMENT".to_string(), 10, Vec::new())],
            "the contraction filter drops every suggestion whose lead is not das/dem/den/die"
        );
    }

    #[test]
    fn derselben_and_welcher_lead_the_determiner_synthesis() {
        let matches = agreement_family("Derselbe alten Fehler passiert heute.");
        assert_eq!(
            matches,
            vec![(
                "DE_AGREEMENT".to_string(),
                0,
                vec!["Derselbe alter Fehler".to_string()]
            )]
        );
        let matches = agreement_family("Welche alten Mann kommt heute?");
        assert_eq!(
            matches,
            vec![(
                "DE_AGREEMENT".to_string(),
                0,
                vec!["Welcher alter Mann".to_string()]
            )]
        );
    }

    #[test]
    fn java_divergences_pin_the_current_residue() {
        let matches = agreement_family("Der Ende der 1960er Jahre umgestaltete Garten ist schön.");
        assert_eq!(
            matches,
            vec![("DE_AGREEMENT".to_string(), 0, vec!["Der Ende".to_string()])],
            "Java CaseRuleTest/AgreementRuleTest accept 'Der Ende der <year> …'; the Rust tagger \
             keeps the masculine article reading where Java's disambiguator drops it"
        );
        let matches = agreement_family("Das ist die Original Mail");
        assert_eq!(
            matches,
            vec![(
                "DE_AGREEMENT".to_string(),
                8,
                vec![
                    "die Originale".to_string(),
                    "das Original".to_string(),
                    "dem Original".to_string()
                ]
            )],
            "Java's CompoundAwareHunspellRule accepts 'die Originalmail' so the compound match \
             fires; is_known_word has no compound-aware lookup, so the regular suggestor answers"
        );
        let matches = agreement_family("Er hat eine 34-jährigen Sohn.");
        assert!(
            matches.is_empty(),
            "Java flags the hyphenated adjective; the generic tokenizer keeps '34-jährigen' \
             unsplit so no det-adj-noun triple forms"
        );
        let matches = agreement_family("Das Auto einem Mannes.");
        assert_eq!(
            matches,
            vec![(
                "DE_AGREEMENT".to_string(),
                9,
                vec![
                    "einem Manne".to_string(),
                    "eines Mannes".to_string(),
                    "einem Mann".to_string()
                ]
            )],
            "the synth dictionary offers the archaic dative 'Manne', which Java's \
             AgreementSuggestor2 filter drops"
        );
        let matches = agreement_family("Was für eine schöner Sonnenuntergang!");
        assert_eq!(
            matches,
            vec![(
                "DE_AGREEMENT".to_string(),
                9,
                vec![
                    "einen schönen Sonnenuntergang".to_string(),
                    "einem schönen Sonnenuntergang".to_string()
                ]
            )],
            "Java offers 'ein schöner Sonnenuntergang' (mixed NOM inflection); the Rust adj \
             synthesis only produces the weak endings here"
        );
        for text in ["Das Dach von meine Auto.", "Das Dach von meinen Auto."] {
            let matches = agreement_family(text);
            assert!(
                matches.is_empty(),
                "{text}: Java flags 'meinem Auto'/'meinen Autos' after 'von'; the Rust \
                 possessive-after-von path stays silent"
            );
        }
    }

    #[test]
    fn fires_on_der_haus() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Der Haus wurde letztes Jahr gebaut.");
        let rule_ids: Vec<&str> = matches.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            rule_ids.contains(&"SIMPLE_AGREEMENT_NEU"),
            "expected SIMPLE_AGREEMENT_NEU match, got {:?}",
            matches
                .matches
                .iter()
                .map(|m| (m.rule().id().to_string(), m.offset()))
                .collect::<Vec<_>>()
        );
        assert!(
            !rule_ids.contains(&"DE_AGREEMENT"),
            "DE_AGREEMENT must be suppressed by the overlapping SIMPLE_AGREEMENT_NEU (Java priority -1 vs 0), got {:?}",
            rule_ids
        );
    }

    #[test]
    fn no_fire_on_correct_phrase() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Das Haus wurde letztes Jahr gebaut.");
        assert!(
            !matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_AGREEMENT")
        );
    }

    #[test]
    fn no_fire_on_relative_clause() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Das Wahlrecht, das Frauen zugesprochen bekamen, ist wichtig.");
        assert!(
            !matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_AGREEMENT")
        );
    }
}
