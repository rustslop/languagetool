use std::sync::LazyLock;

use og_core::{AnalyzedToken, AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};
use regex::Regex;

use super::apostophation::{
    all_need_same_apostrophation_ms, get_preposition_and_determiner, needs_apostrophation_ms,
};
use super::token_scan::{
    has_pos_tag_starting_with, preserve_case_word_by_word, reading_and_tag_matching,
};

static SPLIT_GENDER_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(N.|A..|V.P..|D..|PX.)(.)(.)(.*)$").unwrap());
static SPLIT_GENDER_NUMBER_NO_NOUN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(A..|V.P..|D..|PX.)(.)(.)(.*)$").unwrap());
static SPLIT_GENDER_NUMBER_ADJECTIVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(A..|V.P..|PX.)(.)(.)(.*)$").unwrap());
static POSTAG_EXCEPTIONS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(NP.*|AQ0CN0|SPS00|[CP].*)$").unwrap());
static FORMS_TO_IGNORE: [&str; 2] = ["mes", "las"];

pub struct ConvertArgs<'a> {
    pub gender: &'a str,
    pub number: &'a str,
    pub lemma_select: &'a str,
    pub new_lemma: &'a str,
    pub keep_original: bool,
}

struct GenderAndNumberSplit {
    prefix: String,
    suffix: String,
    gender: String,
    number: String,
}

fn split_gender_number(tag: &str) -> Option<GenderAndNumberSplit> {
    let caps = SPLIT_GENDER_NUMBER.captures(tag)?;
    let prefix = caps.get(1)?.as_str().to_string();
    let g2 = caps.get(2)?.as_str().to_string();
    let g3 = caps.get(3)?.as_str().to_string();
    let suffix = caps.get(4)?.as_str().to_string();
    if prefix.starts_with('V') {
        Some(GenderAndNumberSplit {
            prefix,
            suffix,
            gender: g3,
            number: g2,
        })
    } else {
        Some(GenderAndNumberSplit {
            prefix,
            suffix,
            gender: g2,
            number: g3,
        })
    }
}

fn split_of_reading(atr: &AnalyzedTokenReadings, re: &Regex) -> Option<GenderAndNumberSplit> {
    let (_, tag) = reading_and_tag_matching(atr, re)?;
    split_gender_number(tag)
}

fn synthesize_with_gender_and_number(
    atr: &AnalyzedToken,
    atr_tag: &str,
    gender: &str,
    number: &str,
    synth: &dyn Fn(&str, &str) -> Vec<String>,
) -> String {
    let mut parts = atr.token().splitn(2, ' ');
    let _word = parts.next().unwrap_or("");
    let remainder = parts.next().unwrap_or("");
    let split = split_gender_number(atr_tag);
    let Some(split) = split else {
        return String::new();
    };
    let (gender, number) = if split.prefix.starts_with('V') {
        (number, gender)
    } else {
        (gender, number)
    };
    let add_gender = if split.prefix.starts_with("DA") {
        ""
    } else {
        "C"
    };
    let pattern = format!(
        "{}[{}{}][{}N]{}",
        split.prefix, gender, add_gender, number, split.suffix
    );
    let synthesized = synth(atr.lemma().unwrap_or(""), &pattern);
    match synthesized.into_iter().next() {
        Some(form) if !remainder.is_empty() => format!("{form} {remainder}"),
        Some(form) => form,
        None => String::new(),
    }
}

pub fn accept_rule_match(
    rm: &RuleMatch,
    sentence_text: &str,
    from_pos: usize,
    tokens: &[&AnalyzedTokenReadings],
    args: &ConvertArgs,
    tagger: &dyn Fn(&str) -> Vec<AnalyzedTokenReadings>,
    synth: &dyn Fn(&str, &str) -> Vec<String>,
) -> Option<RuleMatch> {
    let mut pos_word = 0usize;
    while pos_word < tokens.len()
        && (tokens[pos_word].token().start() < from_pos || tokens[pos_word].is_sentence_start())
    {
        pos_word += 1;
    }
    let lemma_select_re = Regex::new(&format!("^(?:{})$", args.lemma_select)).ok()?;
    if pos_word >= tokens.len() {
        return None;
    }

    let mut suggestions: Vec<String> = Vec::new();
    let mut atr_noun_list: Vec<AnalyzedToken> = Vec::new();
    let atr_noun_orig = reading_and_tag_matching(tokens[pos_word], &lemma_select_re)
        .map(|(reading, _)| reading.clone());
    let original_replacements: Vec<String> = rm
        .replacements()
        .iter()
        .map(|r| r.value().to_string())
        .collect();
    if !args.new_lemma.is_empty()
        && let Some(orig) = atr_noun_orig.as_ref()
    {
        atr_noun_list.push(
            AnalyzedToken::new(orig.token(), orig.start(), orig.end())
                .with_lemma(args.new_lemma)
                .with_pos_tags(orig.pos_tags().to_vec()),
        );
    } else if !original_replacements.is_empty() {
        let split_noun_orig_postag = atr_noun_orig
            .as_ref()
            .and_then(|at| split_gender_number(at.pos_tags().first()?));
        for suggestion in &original_replacements {
            let mut parts = suggestion.splitn(2, ' ');
            let word = parts.next().unwrap_or("");
            let remainder = parts.next().unwrap_or("");
            let atrs = tagger(word);
            let Some(first) = atrs.first() else {
                suggestions = original_replacements.clone();
                atr_noun_list.clear();
                break;
            };
            let matched = reading_and_tag_matching(first, &SPLIT_GENDER_NUMBER);
            if matched.is_none() || reading_and_tag_matching(first, &POSTAG_EXCEPTIONS).is_some() {
                suggestions = original_replacements.clone();
                atr_noun_list.clear();
                break;
            }
            let (at, at_tag) = matched.unwrap();
            let split_postag = split_gender_number(at_tag)?;
            let mut new_postag = split_postag.prefix.clone();
            let number = if !args.number.is_empty() {
                args.number.to_string()
            } else if split_noun_orig_postag
                .as_ref()
                .is_some_and(|s| s.number != "N")
            {
                split_noun_orig_postag.as_ref().unwrap().number.clone()
            } else {
                split_postag.number.clone()
            };
            let gender = if !args.gender.is_empty() {
                args.gender.to_string()
            } else {
                split_postag.gender.clone()
            };
            if split_postag.prefix.starts_with('V') {
                new_postag.push_str(&format!("{number}{gender}"));
            } else {
                new_postag.push_str(&format!("{gender}{number}"));
            }
            new_postag.push_str(&split_postag.suffix);
            let complete_form = if remainder.is_empty() {
                at.lemma().unwrap_or("").to_string()
            } else {
                format!("{word} {remainder}")
            };
            atr_noun_list.push(
                AnalyzedToken::new(complete_form, at.start(), at.end())
                    .with_lemma(at.lemma().unwrap_or(""))
                    .with_pos_tags(vec![new_postag]),
            );
        }
    } else {
        atr_noun_list.push(atr_noun_orig?);
    }
    let mut start_pos = pos_word;
    let mut end_pos = pos_word;

    let atr_refs: Vec<&AnalyzedToken> = atr_noun_list.iter().collect();
    let all_need_same =
        all_need_same_apostrophation_ms(tokens[pos_word].token().token(), &atr_refs);

    for atr_noun in &atr_noun_list {
        start_pos = pos_word;
        end_pos = pos_word;
        let atr_tag = atr_noun.pos_tags().first().map(String::as_str)?;
        let split_postag = split_gender_number(atr_tag)?;
        let mut desired_gender_str = if !args.gender.is_empty() {
            args.gender.to_string()
        } else {
            split_postag.gender.clone()
        };
        let mut desired_number_str = if !args.number.is_empty() {
            args.number.to_string()
        } else {
            split_postag.number.clone()
        };
        if desired_gender_str == "C"
            && pos_word > 1
            && let Some(split2) = split_of_reading(tokens[pos_word - 1], &SPLIT_GENDER_NUMBER)
            && (split2.gender == "F" || split2.gender == "M")
        {
            desired_gender_str = split2.gender;
        }
        if desired_gender_str == "C"
            && pos_word + 1 < tokens.len()
            && let Some(split2) = split_of_reading(tokens[pos_word + 1], &SPLIT_GENDER_NUMBER)
            && (split2.gender == "F" || split2.gender == "M")
        {
            desired_gender_str = split2.gender;
        }
        if desired_number_str == "N"
            && pos_word > 1
            && let Some(split2) = split_of_reading(tokens[pos_word - 1], &SPLIT_GENDER_NUMBER)
            && (split2.number == "S" || split2.number == "P")
        {
            desired_number_str = split2.number;
        }
        if desired_number_str == "N"
            && pos_word + 1 < tokens.len()
            && let Some(split2) = split_of_reading(tokens[pos_word + 1], &SPLIT_GENDER_NUMBER)
            && (split2.number == "S" || split2.number == "P")
        {
            desired_number_str = split2.number;
        }

        if desired_gender_str.contains(&split_postag.gender) {
            desired_gender_str = format!(
                "{}{}",
                split_postag.gender,
                desired_gender_str.replace(&split_postag.gender, "")
            );
        }
        if desired_number_str.contains(&split_postag.number) {
            desired_number_str = format!(
                "{}{}",
                split_postag.number,
                desired_number_str.replace(&split_postag.number, "")
            );
        }
        for gender_ch in desired_gender_str.chars() {
            for number_ch in desired_number_str.chars() {
                let desired_gender = gender_ch.to_string();
                let desired_number = number_ch.to_string();
                let mut suggestion_builder = String::new();
                let mut ignore_this_suggestion = false;
                if !args.keep_original {
                    let s = synthesize_with_gender_and_number(
                        atr_noun,
                        atr_tag,
                        &desired_gender,
                        &desired_number,
                        synth,
                    );
                    if s.is_empty() {
                        ignore_this_suggestion = true;
                    }
                    suggestion_builder.push_str(&s);
                } else {
                    suggestion_builder.push_str(atr_noun.token());
                }
                let mut stop = false;
                let mut i = pos_word as isize;
                let mut preposition_to_add = String::new();
                let mut add_determiner = false;
                let mut added_demonstrative = false;
                let mut conditional_added_string = String::new();
                let mut add_tot = String::new();
                while !stop && i > 1 {
                    i -= 1;
                    let idx = i as usize;
                    let mut atr =
                        reading_and_tag_matching(tokens[idx], &SPLIT_GENDER_NUMBER_NO_NOUN);
                    if (!has_pos_tag_starting_with(tokens[idx], "D")
                        && (tokens[idx].has_pos_tag("_perfet")
                            || tokens[idx].has_pos_tag("_GV_")
                            || tokens[idx].has_chunk("GV")))
                        || FORMS_TO_IGNORE
                            .contains(&tokens[idx].token().token().to_lowercase().as_str())
                    {
                        atr = None;
                    }
                    let atr_ok = atr.filter(|(reading, _)| {
                        !reading.pos_tags().is_empty() && reading.lemma().is_some()
                    });
                    if let Some((reading, tag)) = atr_ok {
                        let lemma = reading.lemma().unwrap_or("");
                        if tag.starts_with("DA") {
                            suggestion_builder.insert_str(0, &conditional_added_string);
                            conditional_added_string.clear();
                            add_determiner = true;
                            start_pos = idx;
                        } else if !add_determiner && !added_demonstrative {
                            let mut s = synthesize_with_gender_and_number(
                                reading,
                                tag,
                                &desired_gender,
                                &desired_number,
                                synth,
                            );
                            if s.is_empty() {
                                ignore_this_suggestion = true;
                            }
                            if s == "bo" {
                                s = "bon".to_string();
                            }
                            suggestion_builder.insert_str(0, &conditional_added_string);
                            conditional_added_string.clear();
                            if (idx + 1) < tokens.len()
                                && tokens[idx + 1].token().start() > tokens[idx].token().end()
                            {
                                suggestion_builder.insert(0, ' ');
                            }
                            suggestion_builder.insert_str(0, &s);
                            start_pos = idx;
                            if tag.starts_with("DD") {
                                added_demonstrative = true;
                            }
                            if tag.starts_with('D')
                                && !tag.starts_with("DN")
                                && !added_demonstrative
                                && !lemma.eq_ignore_ascii_case("quant")
                            {
                                stop = true;
                            }
                        } else {
                            if lemma == "tot" {
                                let s = synthesize_with_gender_and_number(
                                    reading,
                                    tag,
                                    &desired_gender,
                                    &desired_number,
                                    synth,
                                );
                                if !s.is_empty() {
                                    add_tot = format!("{s} ");
                                    start_pos = idx;
                                }
                            }
                            stop = true;
                        }
                    } else if tokens[idx].has_pos_tag("SPS00")
                        || tokens[idx].has_pos_tag("LOC_PREP")
                    {
                        if add_determiner {
                            let preposition = tokens[idx].token().token().to_lowercase();
                            let preposition = match preposition.as_str() {
                                "pe" => "per".to_string(),
                                "d'" => "de".to_string(),
                                p => p.to_string(),
                            };
                            if preposition == "a" || preposition == "de" || preposition == "per" {
                                preposition_to_add = preposition;
                                start_pos = idx;
                            }
                        } else if !all_need_same
                            && (tokens[idx].token().token().eq_ignore_ascii_case("d'")
                                || tokens[idx].token().token().eq_ignore_ascii_case("de"))
                        {
                            start_pos = idx;
                            preposition_to_add = "de".to_string();
                        }
                        stop = true;
                    } else if tokens[idx].has_pos_tag("_PUNCT_CONT")
                        || tokens[idx].has_pos_tag("CC")
                    {
                        if pos_word - idx == 1
                            || (i > 1
                                && tokens[idx].token().token().eq_ignore_ascii_case("i")
                                && tokens[idx - 1].token().token().eq_ignore_ascii_case("tot"))
                        {
                            stop = true;
                        } else {
                            conditional_added_string
                                .insert_str(0, &format!("{} ", tokens[idx].token().token()));
                        }
                    } else if has_pos_tag_starting_with(tokens[idx], "RG")
                        && (i <= 1
                            || reading_and_tag_matching(tokens[idx - 1], &SPLIT_GENDER_NUMBER)
                                .is_some())
                        && (i >= tokens.len() as isize - 1
                            || reading_and_tag_matching(tokens[idx + 1], &SPLIT_GENDER_NUMBER)
                                .is_some())
                    {
                        conditional_added_string
                            .insert_str(0, &format!("{} ", tokens[idx].token().token()));
                    } else {
                        stop = true;
                    }
                }
                stop = false;
                let mut i = pos_word as isize;
                conditional_added_string.clear();
                let mut is_there_conjunction = false;
                while !stop && i < tokens.len() as isize - 1 {
                    i += 1;
                    let idx = i as usize;
                    if FORMS_TO_IGNORE
                        .contains(&tokens[idx].token().token().to_lowercase().as_str())
                    {
                        break;
                    }
                    let mut atr =
                        reading_and_tag_matching(tokens[idx], &SPLIT_GENDER_NUMBER_ADJECTIVE);
                    if is_there_conjunction && has_pos_tag_starting_with(tokens[idx], "NC") {
                        atr = None;
                    }
                    if let Some((reading, tag)) = atr {
                        let s = synthesize_with_gender_and_number(
                            reading,
                            tag,
                            &desired_gender,
                            &desired_number,
                            synth,
                        );
                        if s.is_empty() {
                            ignore_this_suggestion = true;
                        }
                        suggestion_builder.push_str(&conditional_added_string);
                        conditional_added_string.clear();
                        suggestion_builder.push(' ');
                        suggestion_builder.push_str(&s);
                        end_pos = idx;
                    } else if has_pos_tag_starting_with(tokens[idx], "RG") {
                        conditional_added_string
                            .push_str(&format!(" {}", tokens[idx].token().token()));
                    } else if tokens[idx].has_pos_tag("CC") {
                        is_there_conjunction = true;
                        conditional_added_string
                            .push_str(&format!(" {}", tokens[idx].token().token()));
                    } else if tokens[idx].has_pos_tag("_PUNCT_CONT") {
                        conditional_added_string.push_str(tokens[idx].token().token());
                    } else {
                        stop = true;
                    }
                }
                if add_determiner {
                    let det = get_preposition_and_determiner(
                        &suggestion_builder,
                        &format!("{desired_gender}{desired_number}"),
                        &preposition_to_add,
                    )
                    .unwrap_or("");
                    suggestion_builder.insert_str(0, det);
                } else if !preposition_to_add.is_empty() {
                    if preposition_to_add == "de" && needs_apostrophation_ms(&suggestion_builder) {
                        preposition_to_add = "d'".to_string();
                    } else {
                        preposition_to_add.push(' ');
                    }
                    suggestion_builder.insert_str(0, &preposition_to_add);
                }
                suggestion_builder.insert_str(0, &add_tot);
                let original_span = &sentence_text
                    [tokens[start_pos].token().start()..tokens[end_pos].token().end()];
                let suggestion = preserve_case_word_by_word(&suggestion_builder, original_span);
                if end_pos == pos_word
                    && start_pos == pos_word
                    && tokens[pos_word].token().token() == suggestion
                {
                    continue;
                }
                if !ignore_this_suggestion {
                    suggestions.push(suggestion);
                }
            }
        }
    }
    if suggestions.is_empty() {
        return None;
    }
    let original_str =
        &sentence_text[tokens[start_pos].token().start()..tokens[end_pos].token().end()];
    if suggestions.iter().any(|s| s == original_str) {
        return None;
    }
    let mut out = rm.clone();
    out.set_offset_length(
        tokens[start_pos].token().start(),
        tokens[end_pos].token().end() - tokens[start_pos].token().start(),
    );
    out.set_replacements(
        suggestions
            .iter()
            .map(|s| SuggestedReplacement::new(s.clone()))
            .collect(),
    );
    Some(out)
}
