use std::sync::LazyLock;

use og_core::AnalyzedTokenReadings;
use regex::Regex;

use super::token_scan::{eq_ignore_case, matches_pos_tag_regex, reading_with_tag_regex};
use super::verb_classifier::is_intransitive;
use super::verb_synthesizer::{P_VERB, VerbSynthesizer};
use crate::ca::pronoms_febles::{PronounPosition, transform};

static PRONOM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("^P0.{6}$|^PP3CN000$|^PP3NN000$|^PP3..A00$|^PP[123]CP000$|^PP3CSD00$").unwrap()
});
static FINITE_VERB: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.[SI].*$").unwrap());
static DET: LazyLock<Regex> = LazyLock::new(|| Regex::new("^D.*$").unwrap());
static TRAILING_DISLOCATION_TAGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^D.*$|^N.*$|^A.*$|^PP3.*$|^PX.*$|^PD.*$").unwrap());
static VERB_3S: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.[SI].3S.*$").unwrap());
static VERB_3P: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.[SI].3P.*$").unwrap());
static VERB_2P: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.[SI].2P.*$").unwrap());
static NP_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new("^NP.*$").unwrap());
static NC_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new("^NC.*$").unwrap());
static NC_SINGULAR: LazyLock<Regex> = LazyLock::new(|| Regex::new("^NC.S.*$").unwrap());
static NC_PLURAL: LazyLock<Regex> = LazyLock::new(|| Regex::new("^NC.P.*$|^NCCN000$").unwrap());
static SUBJECT_PRONOUN_TAGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^D.*$|^N.*$|^NP.*$|^PI.*$").unwrap());
static INFINITIVE_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new("^V.N.*$").unwrap());
static ADVERB_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new("^R.$|^RG$|^RN$").unwrap());
static OVERT_SUBJECT_TAGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^PP[123].*$|^NP.*$").unwrap());
static FULL_ATTRIBUTE_TAGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^A.*$|^V.P.*$|^N.*$|^D.*$|^PI.*$|^PX.*$|^Z.*$|^SPS.*$").unwrap());
static NOMINAL_CONTENT_TAGS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^D.*$|^N.*$|^PI.*$|^PX.*$|^PD.*$|^Z.*$").unwrap());

const SINO_LAST_WORDS: [&str; 2] = ["potser", "oi"];
const COPULAR_VERBS: [&str; 5] = ["ser", "ésser", "estar", "semblar", "parèixer"];
const ANIMATE_SUBJECT_NOUNS: &[&str] = &[
    "persona",
    "gent",
    "home",
    "dona",
    "noi",
    "noia",
    "nen",
    "nena",
    "nan",
    "senyor",
    "senyora",
    "senyoreta",
    "pare",
    "mare",
    "fill",
    "filla",
    "germà",
    "germana",
    "amic",
    "amiga",
    "professor",
    "professora",
    "mestre",
    "mestra",
    "metge",
    "metgessa",
    "doctor",
    "doctora",
    "alcalde",
    "rei",
    "reina",
    "policia",
    "nadó",
    "avi",
    "àvia",
    "tia",
    "tio",
    "oncle",
    "cosí",
    "cosina",
    "veí",
    "veïna",
    "client",
    "clienta",
    "jutge",
    "gos",
    "gossa",
    "gat",
    "gata",
    "criatura",
    "company",
    "companya",
    "merdós",
    "cabró",
    "imbècil",
    "idiota",
    "desgraciat",
    "malparit",
    "beneit",
    "individu",
    "paio",
    "tipus",
    "subjecte",
    "element",
    "marit",
    "muller",
    "xiquet",
];
const NEVER_SUBJECT_NOUNS: &[&str] = &[
    "cop",
    "vegada",
    "volta",
    "mica",
    "miqueta",
    "moment",
    "instant",
    "dilluns",
    "dimarts",
    "dimecres",
    "dijous",
    "divendres",
    "dissabte",
    "diumenge",
    "dia",
    "nit",
    "matí",
    "tarda",
    "vespre",
    "hora",
    "temps",
    "setmana",
    "mes",
    "any",
];
const TEMPORAL_ADJUNCT_NOUNS: &[&str] = &[
    "cop",
    "vegada",
    "volta",
    "moment",
    "instant",
    "dilluns",
    "dimarts",
    "dimecres",
    "dijous",
    "divendres",
    "dissabte",
    "diumenge",
    "dia",
    "nit",
    "nit_1",
    "matí",
    "tarda",
    "vespre",
    "hora",
    "temps",
    "setmana",
    "mes",
    "any",
];
const WEEKDAY_NOUNS: [&str; 7] = [
    "dilluns",
    "dimarts",
    "dimecres",
    "dijous",
    "divendres",
    "dissabte",
    "diumenge",
];
const ACCUSATIVE_PRONOUNS: [&str; 8] = ["em", "et", "el", "la", "ens", "us", "les", "els"];
const DATIVE_PRONOUNS: [&str; 6] = ["em", "et", "li", "ens", "us", "els"];
const INTERROGATIVE_WORDS: &[&str] = &[
    "que", "què", "quin", "quina", "quins", "quines", "qui", "quant", "quants", "quanta",
    "quantes", "res",
];
const COMPLETIVE_GAP_VERBS: &[&str] = &[
    "creure",
    "pensar",
    "dir",
    "opinar",
    "afirmar",
    "suposar",
    "imaginar",
    "saber",
    "considerar",
    "sospitar",
    "témer",
    "voler",
    "desitjar",
    "necessitar",
    "esperar",
    "preferir",
];
const NEUTER_ATTRIBUTE_WORDS: [&str; 4] = ["això", "allò", "açò", "ho"];
const CONFIRMATION_TAGS: [&str; 6] = ["veritat", "oi", "eh", "no", "cert", "potser"];

pub fn accept_rule_match(
    rule_id: &str,
    mode: Option<&str>,
    tokens: &[&AnalyzedTokenReadings],
) -> bool {
    let is_amb = rule_id.contains("AMBACCENT") || rule_id.contains("AMB_ACCENT");
    let is_sense = rule_id.contains("SENSEACCENT") || rule_id.contains("SENSE_ACCENT");
    if !is_amb && !is_sense {
        return true;
    }
    let completive_only = mode == Some("completive");
    let predicts = predicts_accent(tokens, completive_only);
    if mode == Some("completive") && predicts.is_none() {
        return true;
    }
    let Some(predicts) = predicts else {
        return true;
    };
    if is_amb { predicts } else { !predicts }
}

struct VerbGroupInfo {
    first_verb_index: isize,
    last_verb_index: isize,
    last_index: isize,
    num_pronouns_before: isize,
}

pub fn predicts_accent(tokens: &[&AnalyzedTokenReadings], completive_only: bool) -> Option<bool> {
    let que_pos = find_que(tokens)?;
    let starts_with_accent = eq_ignore_case(tokens[que_pos].token().token(), "què");
    let question_pos = find_question_mark(tokens, que_pos);
    let core_end = if question_pos >= 0 {
        strip_confirmation_tag(tokens, que_pos, question_pos)
    } else {
        tokens.len() as isize
    };
    let last_word = if tokens.len() > 2 {
        tokens[tokens.len() - 2].token().token()
    } else {
        ""
    };
    let is_si_no_question = tokens.len() > 3
        && (SINO_LAST_WORDS.contains(&last_word.to_lowercase().as_str())
            || (last_word == "què" && tokens[tokens.len() - 3].token().token() == "o")
            || (last_word == "no" && tokens[tokens.len() - 3].token().token() == ","));
    let is_another_subject = last_word == "això";

    let que_pos_signed = que_pos as isize;
    let mut j = tokens.len() as isize - 2;
    while j > que_pos_signed
        && matches_pos_tag_regex(tokens[j as usize], &TRAILING_DISLOCATION_TAGS)
    {
        j -= 1;
    }
    let trailing_dislocation = j < tokens.len() as isize - 2
        && j > que_pos_signed
        && tokens[j as usize].token().token() == ",";

    let verb_group = verb_group_after_que(tokens, que_pos_signed, core_end)?;
    let first_verb_pos = verb_group.first_verb_index;
    let main_verb_pos = verb_group.last_verb_index;
    let verb3s = matches_pos_tag_regex(tokens[first_verb_pos as usize], &VERB_3S);
    let verb3p = matches_pos_tag_regex(tokens[first_verb_pos as usize], &VERB_3P);
    let verb2p = matches_pos_tag_regex(tokens[first_verb_pos as usize], &VERB_2P);
    let main_verb_lemma = reading_with_tag_regex(tokens[main_verb_pos as usize], &P_VERB)
        .and_then(|r| r.lemma())
        .map(str::to_string);

    let first_token_after_verb_pos = verb_group.last_index + 1;
    let first_postag_after_verb =
        if first_token_after_verb_pos >= 0 && first_token_after_verb_pos < tokens.len() as isize {
            reading_with_tag_regex(tokens[first_token_after_verb_pos as usize], &ANY_TAG)
                .and_then(|r| r.pos_tags().first().map(String::as_str))
                .unwrap_or("UNKNOWN")
        } else {
            ""
        };

    if completive_only
        && !(main_verb_pos >= 0
            && main_verb_pos + 1 < tokens.len() as isize
            && tokens[(main_verb_pos + 1) as usize].token().token() == "que"
            && main_verb_lemma
                .as_deref()
                .is_some_and(|l| COMPLETIVE_GAP_VERBS.contains(&l)))
    {
        return None;
    }

    let voler_dir = voler_dir_prediction(tokens, &verb_group, core_end);
    if let Some(voler_dir) = voler_dir {
        if is_si_no_question {
            return Some(false);
        }
        return Some(voler_dir);
    }

    let mut pronoms: Vec<String> = Vec::new();
    let mut i = first_verb_pos - verb_group.num_pronouns_before;
    while i < first_verb_pos {
        add_pronoun_token(tokens, i, &mut pronoms);
        i += 1;
    }
    let mut i = main_verb_pos + 1;
    while i <= verb_group.last_index {
        add_pronoun_token(tokens, i, &mut pronoms);
        i += 1;
    }
    let has_ho = pronoms.iter().any(|p| p == "ho");
    let mut has_accusative_pronoun = false;
    let mut has_dative_pronoun = false;
    let mut has_accusative_not_dative_pronoun = false;
    for p in &pronoms {
        let accusative = ACCUSATIVE_PRONOUNS.contains(&p.as_str());
        let dative = DATIVE_PRONOUNS.contains(&p.as_str());
        has_accusative_pronoun |= accusative;
        has_dative_pronoun |= dative;
        has_accusative_not_dative_pronoun |= accusative && !dative;
    }
    let pronom_str = if pronoms.is_empty() {
        String::new()
    } else {
        transform(&pronoms.join(" "), PronounPosition::Davant)
    };

    let complement_can_be_subject = (verb3s || verb3p || verb2p)
        && first_token_after_verb_pos > 0
        && first_token_after_verb_pos < tokens.len() as isize
        && postverbal_can_be_subject(tokens, first_token_after_verb_pos, verb3s, verb3p, verb2p);

    let mut complement_can_be_object = has_accusative_not_dative_pronoun;
    if first_postag_after_verb.starts_with('N')
        || (first_token_after_verb_pos > 0
            && first_token_after_verb_pos + 1 < tokens.len() as isize
            && (matches_pos_tag_regex(tokens[first_token_after_verb_pos as usize], &DET)
                || INTERROGATIVE_WORDS.contains(
                    &tokens[first_token_after_verb_pos as usize]
                        .token()
                        .token()
                        .to_lowercase()
                        .as_str(),
                )))
    {
        complement_can_be_object = true;
    }

    if main_verb_lemma.as_deref() == Some("veure") && has_accusative_pronoun {
        complement_can_be_object = true;
    }

    let mut is_exception_object = false;
    let mut is_exception_subject = false;
    if first_token_after_verb_pos > 0 && first_token_after_verb_pos + 1 < tokens.len() as isize {
        let readings_after_verb = tokens[first_token_after_verb_pos as usize];
        let readings_after_verb2 = tokens[(first_token_after_verb_pos + 1) as usize];
        let is_common_exception = readings_after_verb2.has_chunk("PTime")
            || readings_after_verb2.has_pos_tag("_loc_unavegada")
            || readings_after_verb2.has_pos_tag("_data_concreta")
            || readings_after_verb.has_lemma("tot");
        is_exception_object = !has_accusative_not_dative_pronoun && is_common_exception;
        is_exception_subject = is_common_exception;
    }
    complement_can_be_object = complement_can_be_object && !is_exception_object;
    if main_verb_lemma.as_deref() == Some("fer")
        && postverbal_starts_temporal_adjunct(tokens, first_token_after_verb_pos, core_end)
    {
        complement_can_be_object = false;
    }
    let complement_can_be_subject = complement_can_be_subject
        && !is_exception_subject
        && !is_another_subject
        && !trailing_dislocation;

    let mut is_que_subject = false;
    if verb3s && main_verb_pos > 1 {
        let que_found_pos = find_first_form(tokens, main_verb_pos + 1, "que");
        let nearby_que = que_found_pos > main_verb_pos && que_found_pos < main_verb_pos + 5;
        let fer_has_subject_que = nearby_que || has_accusative_not_dative_pronoun;
        let previous_verb_lemma =
            reading_with_tag_regex(tokens[(main_verb_pos - 1) as usize], &P_VERB)
                .and_then(|r| r.lemma());
        is_que_subject = fer_has_subject_que
            && (main_verb_lemma.as_deref() == Some("fer") || previous_verb_lemma == Some("fer"));
    }
    let mut is_que_object = false;
    let mut saturated_completive = false;
    let main_verb_finite_lemma =
        reading_with_tag_regex(tokens[main_verb_pos as usize], &FINITE_VERB)
            .and_then(|r| r.lemma());
    if main_verb_finite_lemma.is_some()
        && main_verb_pos + 1 < tokens.len() as isize
        && tokens[(main_verb_pos + 1) as usize].token().token() == "que"
        && main_verb_finite_lemma != Some("veure")
    {
        if main_verb_lemma
            .as_deref()
            .is_some_and(|l| COMPLETIVE_GAP_VERBS.contains(&l))
        {
            match completive_has_gap(tokens, main_verb_pos, core_end) {
                Some(false) => saturated_completive = true,
                _ => is_que_object = true,
            }
        } else {
            is_que_object = true;
        }
    }
    let is_fer = main_verb_lemma.as_deref() == Some("fer");
    let fer_por_mal_polar = !starts_with_accent
        && is_fer_por_mal_polar(
            tokens,
            main_verb_pos,
            first_token_after_verb_pos,
            core_end,
            has_dative_pronoun,
        );
    if fer_por_mal_polar || (is_fer && is_fer_por_que(tokens, first_token_after_verb_pos)) {
        is_que_subject = false;
        complement_can_be_object = true;
    } else if is_fer && is_fer_mal(tokens, first_token_after_verb_pos) {
        is_que_subject = true;
        complement_can_be_object = false;
    }

    let mut is_intransitive_verb = (main_verb_lemma.as_deref().is_some_and(is_intransitive)
        && main_verb_lemma
            .as_deref()
            .is_none_or(|l| !COPULAR_VERBS.contains(&l)))
        || (main_verb_lemma.as_deref() == Some("ser") && pronom_str == "hi");
    if main_verb_lemma.as_deref() == Some("passar") || main_verb_lemma.as_deref() == Some("agradar")
    {
        is_intransitive_verb = true;
        is_que_subject = !complement_can_be_object;
    } else if is_intransitive_verb {
        is_que_subject = false;
    }
    is_que_subject = is_que_subject && !is_another_subject;

    if starts_with_accent
        && main_verb_lemma.as_deref() == Some("fer")
        && is_fer_mal(tokens, first_token_after_verb_pos)
    {
        is_intransitive_verb = false;
        is_que_subject = true;
        complement_can_be_object = false;
    }

    let mut predicts = is_que_subject
        || is_que_object
        || (!is_intransitive_verb
            && ((!complement_can_be_object && !has_ho) || complement_can_be_subject));

    if saturated_completive && !is_que_subject {
        predicts = false;
    }
    if is_si_no_question && !is_que_subject && !is_que_object {
        predicts = false;
    }
    Some(predicts)
}

static ANY_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(".*").unwrap());

fn voler_dir_prediction(
    tokens: &[&AnalyzedTokenReadings],
    verb_group: &VerbGroupInfo,
    core_end: isize,
) -> Option<bool> {
    if verb_group.last_verb_index <= verb_group.first_verb_index {
        return None;
    }
    let last_verb_lemma =
        reading_with_tag_regex(tokens[verb_group.last_verb_index as usize], &P_VERB)
            .and_then(|r| r.lemma())?;
    if last_verb_lemma != "dir" {
        return None;
    }
    let mut has_voler = false;
    let mut i = verb_group.first_verb_index;
    while i <= verb_group.last_verb_index {
        if reading_with_tag_regex(tokens[i as usize], &P_VERB).and_then(|r| r.lemma())
            == Some("voler")
        {
            has_voler = true;
        }
        i += 1;
    }
    if !has_voler {
        return None;
    }
    let len = tokens.len() as isize;
    let mut object_pos = verb_group.last_index + 1;
    while object_pos < core_end
        && object_pos < len
        && matches_pos_tag_regex(tokens[object_pos as usize], &DET)
    {
        object_pos += 1;
    }
    if object_pos < core_end && object_pos < len {
        let form = tokens[object_pos as usize].token().token().to_lowercase();
        if form == "res"
            || form == "quelcom"
            || form == "alguna"
            || form == "algun"
            || form == "cap"
        {
            return Some(false);
        }
    }
    Some(true)
}

fn completive_has_gap(
    tokens: &[&AnalyzedTokenReadings],
    main_verb_pos: isize,
    core_end: isize,
) -> Option<bool> {
    let que_pos = main_verb_pos + 1;
    let emb = verb_group_after_que(tokens, que_pos, core_end)?;
    let emb_lemma = reading_with_tag_regex(tokens[emb.last_verb_index as usize], &P_VERB)
        .and_then(|r| r.lemma())?;

    let mut emb_pron: Vec<String> = Vec::new();
    let mut k = emb.first_verb_index - emb.num_pronouns_before;
    while k < emb.first_verb_index {
        add_pronoun_token(tokens, k, &mut emb_pron);
        k += 1;
    }
    let mut k = emb.last_verb_index + 1;
    while k <= emb.last_index {
        add_pronoun_token(tokens, k, &mut emb_pron);
        k += 1;
    }
    for p in &emb_pron {
        if (ACCUSATIVE_PRONOUNS.contains(&p.as_str()) && !DATIVE_PRONOUNS.contains(&p.as_str()))
            || p == "ho"
            || p == "en"
        {
            return Some(false);
        }
    }

    let mut overt_subject = false;
    let mut k = que_pos + 1;
    while k < emb.first_verb_index && k < tokens.len() as isize {
        if matches_pos_tag_regex(tokens[k as usize], &OVERT_SUBJECT_TAGS) {
            overt_subject = true;
        }
        k += 1;
    }

    let copular = COPULAR_VERBS.contains(&emb_lemma);
    let intransitive = is_intransitive(emb_lemma) && !copular;
    let content_pos = first_argument_token_after(tokens, emb.last_index + 1, core_end);

    if copular {
        if content_pos < 0 {
            return Some(true);
        }
        let c_form = tokens[content_pos as usize].token().token().to_lowercase();
        if NEUTER_ATTRIBUTE_WORDS.contains(&c_form.as_str())
            || INTERROGATIVE_WORDS.contains(&c_form.as_str())
        {
            return Some(true);
        }
        if matches_pos_tag_regex(tokens[content_pos as usize], &FULL_ATTRIBUTE_TAGS) {
            return Some(false);
        }
        return Some(true);
    }

    if content_pos < 0 {
        return Some(!(overt_subject && intransitive));
    }

    let c_form = tokens[content_pos as usize].token().token().to_lowercase();
    if matches_pos_tag_regex(tokens[content_pos as usize], &NOMINAL_CONTENT_TAGS)
        && !INTERROGATIVE_WORDS.contains(&c_form.as_str())
    {
        let verb3s = matches_pos_tag_regex(tokens[emb.first_verb_index as usize], &VERB_3S);
        let verb3p = matches_pos_tag_regex(tokens[emb.first_verb_index as usize], &VERB_3P);
        let verb2p = matches_pos_tag_regex(tokens[emb.first_verb_index as usize], &VERB_2P);
        return Some(postverbal_can_be_subject(
            tokens,
            content_pos,
            verb3s,
            verb3p,
            verb2p,
        ));
    }

    Some(!intransitive)
}

fn first_argument_token_after(
    tokens: &[&AnalyzedTokenReadings],
    start: isize,
    end: isize,
) -> isize {
    let len = tokens.len() as isize;
    let mut i = start;
    while i < end && i < len {
        let form = tokens[i as usize].token().token();
        if form == "?" || form == "," {
            return -1;
        }
        if matches_pos_tag_regex(tokens[i as usize], &ADVERB_TAGS) {
            i += 1;
            continue;
        }
        return i;
    }
    -1
}

fn postverbal_can_be_subject(
    tokens: &[&AnalyzedTokenReadings],
    start: isize,
    verb3s: bool,
    verb3p: bool,
    verb2p: bool,
) -> bool {
    let verb_p = verb3p || verb2p;
    let mut i = start;
    while i < tokens.len() as isize && i <= start + 4 {
        let form = tokens[i as usize].token().token();
        if form == "?" || form == "," {
            break;
        }
        if reading_with_tag_regex(tokens[i as usize], &NP_TAGS).is_some() {
            return true;
        }
        if let Some(common_noun) = reading_with_tag_regex(tokens[i as usize], &NC_TAGS) {
            let lemma = common_noun.lemma().unwrap_or("");
            if NEVER_SUBJECT_NOUNS.contains(&lemma) {
                return false;
            }
            if !(ANIMATE_SUBJECT_NOUNS.contains(&lemma)
                || tokens[i as usize].has_pos_tag("NCCN000"))
            {
                return false;
            }
            let noun_singular = matches_pos_tag_regex(tokens[i as usize], &NC_SINGULAR);
            let noun_plural = matches_pos_tag_regex(tokens[i as usize], &NC_PLURAL);
            return (verb3s && noun_singular)
                || (verb_p && noun_plural)
                || (!noun_singular && !noun_plural);
        }
        i += 1;
    }
    false
}

fn is_fer_mal(tokens: &[&AnalyzedTokenReadings], start: isize) -> bool {
    if start <= 0 || start >= tokens.len() as isize {
        return false;
    }
    let first = tokens[start as usize].token().token().to_lowercase();
    if first == "cap" || first == "algun" || first == "alguna" || first == "un" || first == "una" {
        return false;
    }
    tokens[start as usize].has_lemma("mal")
        || (tokens[start as usize].has_lemma("por") && !next_token_is(tokens, start, "que"))
}

fn is_fer_por_mal_polar(
    tokens: &[&AnalyzedTokenReadings],
    main_verb_pos: isize,
    start: isize,
    end: isize,
    has_dative_pronoun: bool,
) -> bool {
    let len = tokens.len() as isize;
    if main_verb_pos <= 0
        || start <= 0
        || start >= len
        || end <= start
        || !tokens[main_verb_pos as usize].has_lemma("fer")
    {
        return false;
    }
    let noun_pos = first_lexical_token(tokens, start, end);
    if noun_pos < 0 {
        return false;
    }
    if !(tokens[noun_pos as usize].has_lemma("por") || tokens[noun_pos as usize].has_lemma("mal")) {
        return false;
    }
    if tokens[noun_pos as usize].has_lemma("por") && next_token_is(tokens, noun_pos, "que") {
        return false;
    }
    has_dative_pronoun
        || has_infinitive_after_noun(tokens, noun_pos, end)
        || has_subject_after_fer_mal(tokens, noun_pos, end)
        || has_auxiliary_before_fer(tokens, main_verb_pos)
}

fn first_lexical_token(tokens: &[&AnalyzedTokenReadings], start: isize, end: isize) -> isize {
    let len = tokens.len() as isize;
    let mut i = start;
    while i < end && i < len {
        let token = tokens[i as usize].token().token();
        if token == "," || token == "?" {
            break;
        }
        if eq_ignore_case(token, "pas")
            || eq_ignore_case(token, "gaire")
            || eq_ignore_case(token, "més")
        {
            i += 1;
            continue;
        }
        return i;
    }
    -1
}

fn has_subject_after_fer_mal(
    tokens: &[&AnalyzedTokenReadings],
    noun_pos: isize,
    end: isize,
) -> bool {
    let len = tokens.len() as isize;
    let mut i = noun_pos + 1;
    while i < end && i < len && i <= noun_pos + 4 {
        if tokens[i as usize].token().token() == "," || tokens[i as usize].token().token() == "?" {
            break;
        }
        if matches_pos_tag_regex(tokens[i as usize], &SUBJECT_PRONOUN_TAGS) {
            return true;
        }
        i += 1;
    }
    false
}

fn has_infinitive_after_noun(
    tokens: &[&AnalyzedTokenReadings],
    noun_pos: isize,
    end: isize,
) -> bool {
    let len = tokens.len() as isize;
    let mut i = noun_pos + 1;
    while i < end && i < len && i <= noun_pos + 3 {
        if tokens[i as usize].token().token() == "," || tokens[i as usize].token().token() == "?" {
            break;
        }
        if matches_pos_tag_regex(tokens[i as usize], &INFINITIVE_TAGS) {
            return true;
        }
        i += 1;
    }
    false
}

fn has_auxiliary_before_fer(tokens: &[&AnalyzedTokenReadings], verb_pos: isize) -> bool {
    let mut i = verb_pos - 1;
    while i > 0 && i >= verb_pos - 4 {
        if tokens[i as usize].token().token() == "," || tokens[i as usize].token().token() == "?" {
            break;
        }
        let lemma = reading_with_tag_regex(tokens[i as usize], &P_VERB).and_then(|r| r.lemma());
        if lemma == Some("voler")
            || lemma == Some("poder")
            || lemma == Some("deure")
            || lemma == Some("haver")
        {
            return true;
        }
        i -= 1;
    }
    false
}

fn is_fer_por_que(tokens: &[&AnalyzedTokenReadings], start: isize) -> bool {
    start > 0
        && start < tokens.len() as isize
        && tokens[start as usize].has_lemma("por")
        && next_token_is(tokens, start, "que")
}

fn next_token_is(tokens: &[&AnalyzedTokenReadings], start: isize, token: &str) -> bool {
    start + 1 < tokens.len() as isize
        && eq_ignore_case(tokens[(start + 1) as usize].token().token(), token)
}

fn postverbal_starts_temporal_adjunct(
    tokens: &[&AnalyzedTokenReadings],
    start: isize,
    end: isize,
) -> bool {
    let len = tokens.len() as isize;
    if start <= 0 || start >= len || end <= start {
        return false;
    }
    let mut has_determiner = false;
    let mut i = start;
    while i < end && i <= start + 3 {
        if matches_pos_tag_regex(tokens[i as usize], &DET) {
            has_determiner = true;
            i += 1;
            continue;
        }
        let lemma = reading_with_tag_regex(tokens[i as usize], &NC_TAGS).and_then(|r| r.lemma());
        return lemma.is_some_and(|l| {
            TEMPORAL_ADJUNCT_NOUNS.contains(&l) && (has_determiner || WEEKDAY_NOUNS.contains(&l))
        });
    }
    false
}

fn add_pronoun_token(tokens: &[&AnalyzedTokenReadings], index: isize, pronoms: &mut Vec<String>) {
    if index < 0 || index >= tokens.len() as isize {
        return;
    }
    if let Some(pronoun) = reading_with_tag_regex(tokens[index as usize], &PRONOM) {
        pronoms.push(transform(pronoun.token(), PronounPosition::Normalized));
    }
}

fn find_first_form(tokens: &[&AnalyzedTokenReadings], start: isize, form: &str) -> isize {
    let mut i = start.max(0);
    while i < tokens.len() as isize {
        if eq_ignore_case(tokens[i as usize].token().token(), form) {
            return i;
        }
        i += 1;
    }
    -1
}

fn verb_group_after_que(
    tokens: &[&AnalyzedTokenReadings],
    que_idx: isize,
    core_end: isize,
) -> Option<VerbGroupInfo> {
    let verb_synthesizer = VerbSynthesizer::new(tokens, que_idx + 1);
    if verb_synthesizer.is_undefined() || verb_synthesizer.first_verb_index() >= core_end {
        return None;
    }
    let first_verb = verb_synthesizer.first_verb_index();
    let next_que = find_first_form(tokens, que_idx + 1, "que");
    if next_que > que_idx && next_que < first_verb {
        return None;
    }
    let boundary = if next_que > first_verb && next_que < core_end {
        next_que
    } else {
        core_end
    };
    let last_index = verb_synthesizer.last_index().min(boundary - 1);
    let mut last_verb = verb_synthesizer.last_verb_index().min(last_index);
    while last_verb >= first_verb
        && reading_with_tag_regex(tokens[last_verb as usize], &P_VERB).is_none()
    {
        last_verb -= 1;
    }
    if last_verb < first_verb {
        return None;
    }
    Some(VerbGroupInfo {
        first_verb_index: first_verb,
        last_verb_index: last_verb,
        last_index,
        num_pronouns_before: verb_synthesizer.num_pronouns_before(),
    })
}

pub fn find_que(tokens: &[&AnalyzedTokenReadings]) -> Option<usize> {
    tokens.iter().position(|t| {
        eq_ignore_case(t.token().token(), "que") || eq_ignore_case(t.token().token(), "què")
    })
}

fn find_question_mark(tokens: &[&AnalyzedTokenReadings], from_idx: usize) -> isize {
    let mut i = from_idx + 1;
    while i < tokens.len() {
        if tokens[i].token().token() == "?" {
            return i as isize;
        }
        i += 1;
    }
    -1
}

fn strip_confirmation_tag(
    tokens: &[&AnalyzedTokenReadings],
    que_idx: usize,
    question_idx: isize,
) -> isize {
    let last_core = question_idx - 1;
    let comma_idx = last_core - 1;
    if comma_idx > que_idx as isize
        && tokens[comma_idx as usize].token().token() == ","
        && CONFIRMATION_TAGS.contains(
            &tokens[last_core as usize]
                .token()
                .token()
                .to_lowercase()
                .as_str(),
        )
    {
        return comma_idx;
    }
    question_idx
}
