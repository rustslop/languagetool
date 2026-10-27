use og_core::{AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn resolve_args<'a>(args: &'a str, matched_texts: &[String]) -> HashMap<&'a str, String> {
    let mut result = HashMap::new();
    for part in args.split_whitespace() {
        let Some((key, val)) = part.split_once(':') else {
            continue;
        };
        let resolved = if let Some(rest) = val.strip_prefix('\\')
            && let Ok(idx) = rest.parse::<usize>()
            && idx > 0
            && idx <= matched_texts.len()
        {
            matched_texts[idx - 1].clone()
        } else {
            val.to_string()
        };
        result.insert(key, resolved);
    }
    result
}

pub fn pattern_token_range<'a>(
    tokens: &'a [&'a AnalyzedTokenReadings],
    matched_positions: &[usize],
) -> &'a [&'a AnalyzedTokenReadings] {
    match (matched_positions.first(), matched_positions.last()) {
        (Some(&first), Some(&last)) if first <= last && last < tokens.len() => {
            &tokens[first..=last]
        }
        _ => &[],
    }
}

fn is_all_uppercase(s: &str) -> bool {
    s.chars().all(|c| !c.is_alphabetic() || !c.is_lowercase())
}

fn is_capitalized_word(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => {
            chars.all(|c| !c.is_alphabetic() || c.is_lowercase())
        }
        _ => false,
    }
}

fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn has_diacritics(s: &str) -> bool {
    s.chars().any(|c| c.is_alphabetic() && !c.is_ascii())
}

fn full_match(pattern: &str, value: &str) -> bool {
    full_matcher(pattern).is_match(value)
}

fn full_matcher(pattern: &str) -> regex::Regex {
    static CACHE: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap();
    regex::Regex::clone(
        guard
            .entry(pattern.to_string())
            .or_insert_with(|| Regex::new(&format!("^(?:{pattern})$")).unwrap()),
    )
}

fn readings_match_pos_tag_regex(atr: &AnalyzedTokenReadings, pos_tag_regex: &str) -> bool {
    atr.readings()
        .iter()
        .flat_map(|r| r.pos_tags().iter())
        .any(|t| full_match(pos_tag_regex, t))
}

fn confusion_pairs() -> &'static HashMap<String, Vec<(String, String)>> {
    static PAIRS: OnceLock<HashMap<String, Vec<(String, String)>>> = OnceLock::new();
    PAIRS.get_or_init(|| {
        let mut map: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let Some(dir) = crate::es::es_rules_dir() else {
            return map;
        };
        let Ok(text) = std::fs::read_to_string(dir.join("confusion_pairs.txt")) else {
            return map;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split(';').collect();
            if parts.len() != 3 {
                continue;
            }
            map.entry(parts[0].to_string())
                .or_default()
                .push((parts[1].to_string(), parts[2].to_string()));
        }
        map
    })
}

pub fn confusion_check_filter(
    args: &str,
    matched_texts: &[String],
    matched_positions: &[usize],
    tokens: &[&AnalyzedTokenReadings],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let pattern_tokens = pattern_token_range(tokens, matched_positions);
    let parsed = resolve_args(args, matched_texts);
    let postag = parsed.get("postag")?;
    let original_form = parsed.get("form")?;
    let is_all_upper = is_all_uppercase(original_form);
    let is_capitalized = is_capitalized_word(original_form);
    let form = original_form.to_lowercase();
    let gendernumber_from = parsed
        .get("gendernumberFrom")
        .and_then(|v| v.parse::<usize>().ok());
    let desired: Option<&str> = gendernumber_from.and_then(|i| {
        let atr = pattern_tokens.get(i.checked_sub(1)?)?;
        if readings_match_pos_tag_regex(atr, "[NAPD].+MS.*|V.P..SM") {
            Some("NC[MC][SN]000|A..[MC][SN].|V.P..SM")
        } else if readings_match_pos_tag_regex(atr, "[NAPD].+MP.*|V.P..PM") {
            Some("NC[MC][PN]000|A..[MC][PN].|V.P..PM")
        } else if readings_match_pos_tag_regex(atr, "[NAPD].+FS.*|V.P..SF") {
            Some("NC[FC][SN]000|A..[FC][SN].|V.P..SF")
        } else if readings_match_pos_tag_regex(atr, "[NAPD].+FP.*|V.P..PF") {
            Some("NC[FC][PN]000|A..[FC][PN].|V.P..PF")
        } else if readings_match_pos_tag_regex(atr, "[NAPD].+CP.*|V.P..P.") {
            Some("NC[MFC][PN]000|A..[MFC][PN].|V.P..P.")
        } else if readings_match_pos_tag_regex(atr, "[NAPD].+CS.*|V.P..S.") {
            Some("NC[MFC][SN]000|A..[MFC][SN].|V.P..S.")
        } else {
            None
        }
    });

    let pairs = confusion_pairs();
    let mut replacement: Option<String> = None;
    if let Some(entries) = pairs.get(&form)
        && entries.iter().any(|(_, tag)| full_match(postag, tag))
    {
        if let Some(desired) = desired {
            if entries
                .first()
                .is_some_and(|(_, tag)| full_match(desired, tag))
            {
                replacement = entries.first().map(|(word, _)| word.clone());
            }
        } else if gendernumber_from.is_none() {
            replacement = entries.first().map(|(word, _)| word.clone());
        }
    }
    let mut replacement = replacement?;
    let mut message = rm.message().to_string();
    if !has_diacritics(&replacement) || has_diacritics(&form) {
        message = message.replace("se escribe con tilde", "se escribe de otra manera");
    }
    rm.set_message(message);
    if is_all_upper {
        replacement = replacement.to_uppercase();
    }
    if is_capitalized {
        replacement = uppercase_first_char(&replacement);
    }
    let template = rm
        .replacements()
        .first()
        .map(|r| r.value().to_string())
        .unwrap_or_else(|| "{suggestion}".to_string());
    let suggestion = template
        .replace("{suggestion}", &replacement)
        .replace("{Suggestion}", &uppercase_first_char(&replacement))
        .replace("{SUGGESTION}", &replacement.to_uppercase());
    rm.set_replacements(vec![SuggestedReplacement::new(suggestion)]);
    Some(rm)
}

const OPENING_QUOTES: [char; 8] = ['«', '“', '"', '‘', '\'', '„', '¿', '¡'];

fn is_punctuation_or_symbol(token: &str) -> bool {
    token
        .chars()
        .all(|c| !c.is_alphanumeric() && !c.is_whitespace())
}

fn is_opening_quote(token: &str) -> bool {
    token.chars().count() == 1 && OPENING_QUOTES.contains(&token.chars().next().unwrap())
}

fn has_space_before(tokens: &[&AnalyzedTokenReadings], i: usize) -> bool {
    match i.checked_sub(1) {
        Some(prev) => tokens[prev].token().end() < tokens[i].token().start(),
        None => tokens.first().is_some_and(|t| t.token().start() > 0),
    }
}

pub fn add_commas_filter(
    tokens: &[&AnalyzedTokenReadings],
    sentence_base: usize,
    sentence_text: &str,
    mut rm: RuleMatch,
    args: &str,
) -> Option<RuleMatch> {
    let suggest_semicolon = args
        .split_whitespace()
        .any(|part| part.eq_ignore_ascii_case("suggestSemicolon:true"));
    let from = rm.offset();
    let to = rm.offset() + rm.length();
    let rel = |abs: usize| abs - sentence_base;
    let mut postag_from = 1usize;
    while postag_from < tokens.len() && tokens[postag_from].token().start() < from {
        postag_from += 1;
    }
    let mut postag_to = postag_from;
    while postag_to < tokens.len() && tokens[postag_to].token().end() < to {
        postag_to += 1;
    }
    let before_ok = postag_from == 1
        || is_punctuation_or_symbol(tokens[postag_from - 1].token().token())
        || is_capitalized_word(tokens[postag_from].token().token());
    let after_ok = tokens.get(postag_to + 1).is_some_and(|n| {
        is_punctuation_or_symbol(n.token().token())
            && !(has_space_before(tokens, postag_to + 1) && is_opening_quote(n.token().token()))
    });
    if before_ok && after_ok {
        return None;
    }
    let matched = &sentence_text[rel(from)..rel(to)];
    if suggest_semicolon
        && postag_from >= 1
        && postag_from - 1 < tokens.len()
        && tokens[postag_from - 1].token().token() == ","
        && !after_ok
    {
        let start = tokens[postag_from - 1].token().start();
        let end = tokens[postag_to].token().end();
        rm.set_offset_length(start, end - start);
        rm.set_replacements(vec![
            SuggestedReplacement::new(format!("; {matched},")),
            SuggestedReplacement::new(format!(", {matched},")),
        ]);
        return Some(rm);
    }
    if before_ok && !after_ok {
        let start = tokens[postag_to].token().start();
        rm.set_offset_length(start, to - start);
        rm.set_replacements(vec![SuggestedReplacement::new(format!(
            "{},",
            tokens[postag_to].token().token()
        ))]);
        return Some(rm);
    }
    if !before_ok && after_ok {
        let mut start = tokens[postag_from].token().start();
        if has_space_before(tokens, postag_from) {
            start = start.saturating_sub(1);
        }
        let end = tokens[postag_from].token().end();
        rm.set_offset_length(start, end - start);
        rm.set_replacements(vec![SuggestedReplacement::new(format!(
            ", {}",
            tokens[postag_from].token().token()
        ))]);
        return Some(rm);
    }
    let mut start = tokens[postag_from].token().start();
    if has_space_before(tokens, postag_from) {
        start = start.saturating_sub(1);
    }
    let end = tokens[postag_to].token().end();
    rm.set_offset_length(start, end - start);
    rm.set_replacements(vec![SuggestedReplacement::new(format!(", {matched},"))]);
    Some(rm)
}

fn es_contractions_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b([Aa]|[Dd]e) e(l)\b").unwrap())
}

pub fn adapt_suggestion_es(replacement: &str) -> String {
    es_contractions_re()
        .replace_all(replacement, "${1}${2}")
        .into_owned()
}

pub fn adapt_suggestions_filter(mut rm: RuleMatch) -> Option<RuleMatch> {
    let adapted: Vec<SuggestedReplacement> = rm
        .replacements()
        .iter()
        .map(|r| SuggestedReplacement::new(adapt_suggestion_es(r.value())))
        .collect();
    rm.set_replacements(adapted);
    Some(rm)
}

fn marker_or_index(
    spec: &str,
    pattern_tokens: &[&AnalyzedTokenReadings],
    rm_from: usize,
) -> Option<usize> {
    if let Some(suffix) = spec.strip_prefix("marker") {
        let mut i = 0usize;
        while i < pattern_tokens.len() && pattern_tokens[i].token().start() < rm_from {
            i += 1;
        }
        i += 1;
        let extra: usize = suffix.parse().unwrap_or(0);
        let one_based = i + extra;
        if one_based < 1 || one_based > pattern_tokens.len() {
            return None;
        }
        Some(one_based - 1)
    } else {
        let n: usize = spec.parse().ok()?;
        if n < 1 || n > pattern_tokens.len() {
            return None;
        }
        Some(n - 1)
    }
}

fn select_reading<'a>(atr: &'a AnalyzedTokenReadings, select: &str) -> (Option<&'a str>, &'a str) {
    const UNKNOWN: &str = "UNKNOWN";
    for reading in atr.readings() {
        for tag in reading.pos_tags() {
            if full_match(select, tag) {
                return (reading.lemma(), tag);
            }
        }
    }
    match atr.readings().first() {
        Some(reading) => (
            reading.lemma(),
            reading
                .pos_tags()
                .first()
                .map(String::as_str)
                .unwrap_or(UNKNOWN),
        ),
        None => (None, UNKNOWN),
    }
}

fn composite_postag(
    lemma_select: &str,
    postag_select: &str,
    original_postag: &str,
    desired_postag: &str,
    postag_replace: &str,
) -> String {
    let a = full_matcher(lemma_select).captures(original_postag);
    let b = full_matcher(postag_select).captures(desired_postag);
    let mut result = postag_replace.to_string();
    if let (Some(a), Some(b)) = (a, b) {
        for i in 1..=a.len() {
            if let Some(g) = a.get(i) {
                result = result.replace(&format!(r"\a{i}"), g.as_str());
            }
        }
        for i in 1..=b.len() {
            if let Some(g) = b.get(i) {
                result = result.replace(&format!(r"\b{i}"), g.as_str());
            }
        }
    }
    result
}

pub fn advanced_synthesizer_filter(
    synthesizer: Option<&og_rules::postponed_adjective::LangSynthesizer>,
    args: &str,
    matched_texts: &[String],
    matched_positions: &[usize],
    tokens: &[&AnalyzedTokenReadings],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let (&first, &last) = (matched_positions.first()?, matched_positions.last()?);
    if first > last || last >= tokens.len() {
        return Some(rm);
    }
    let pattern_tokens = &tokens[first..=last];
    let parsed = resolve_args(args, matched_texts);
    let (Some(postag_select), Some(lemma_select), Some(postag_from_str), Some(lemma_from_str)) = (
        parsed.get("postagSelect"),
        parsed.get("lemmaSelect"),
        parsed.get("postagFrom"),
        parsed.get("lemmaFrom"),
    ) else {
        return Some(rm);
    };
    let new_lemma = parsed.get("newLemma").cloned().unwrap_or_default();
    let postag_replace = parsed.get("postagReplace").cloned();
    let lemma_from = marker_or_index(lemma_from_str, pattern_tokens, rm.offset())?;
    let postag_from = marker_or_index(postag_from_str, pattern_tokens, rm.offset())?;
    let (lemma, original_postag) = {
        let (l, t) = select_reading(pattern_tokens[lemma_from], lemma_select);
        (l.map(str::to_string), t.to_string())
    };
    let desired_postag = select_reading(pattern_tokens[postag_from], postag_select)
        .1
        .to_string();
    let mut desired_lemma = lemma;
    if !new_lemma.is_empty() {
        if new_lemma.starts_with('_') {
            desired_lemma = None;
        } else {
            desired_lemma = Some(new_lemma);
        }
    }
    let desired_lemma = desired_lemma?;
    let desired_postag = match postag_replace {
        Some(replace) => composite_postag(
            lemma_select,
            postag_select,
            &original_postag,
            &desired_postag,
            &replace,
        ),
        None => desired_postag,
    };
    let surface = pattern_tokens[lemma_from].token().token();
    let is_word_capitalized = is_capitalized_word(surface);
    let is_word_all_upper = is_all_uppercase(surface);
    let Some(synthesizer) = synthesizer else {
        return Some(rm);
    };
    let tag_regex = match Regex::new(&desired_postag) {
        Ok(re) => re,
        Err(_) => return Some(rm),
    };
    let replacements = synthesizer.synthesize(&desired_lemma, &tag_regex);
    if replacements.is_empty() {
        return Some(rm);
    }
    let mut list: Vec<String> = Vec::new();
    let mut suggestion_used = false;
    for r in rm.replacements().iter().map(|r| r.value().to_string()) {
        for nr in &replacements {
            if r.contains("{suggestion}")
                || r.contains("{Suggestion}")
                || r.contains("{SUGGESTION}")
            {
                suggestion_used = true;
            }
            let mut nr = nr.clone();
            if is_word_capitalized {
                nr = uppercase_first_char(&nr);
            }
            if is_word_all_upper {
                nr = nr.to_uppercase();
            }
            let complete = r
                .replace("{suggestion}", &nr)
                .replace("{Suggestion}", &uppercase_first_char(&nr))
                .replace("{SUGGESTION}", &nr.to_uppercase());
            if !list.contains(&complete) {
                list.push(complete);
            }
        }
    }
    if !suggestion_used {
        list.extend(replacements.iter().cloned());
    }
    let adapted: Vec<SuggestedReplacement> = list
        .iter()
        .map(|r| SuggestedReplacement::new(adapt_suggestion_es(r)))
        .collect();
    rm.set_replacements(adapted);
    Some(rm)
}

fn equal_without_diacritics(s: &str, t: &str) -> bool {
    let a: Vec<char> = s.chars().map(crate::es::speller::nfd_base).collect();
    let b: Vec<char> = t.chars().map(crate::es::speller::nfd_base).collect();
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.to_lowercase().eq(y.to_lowercase()))
}

pub fn find_suggestions_filter(
    args: &str,
    matched_texts: &[String],
    matched_positions: &[usize],
    tokens: &[&AnalyzedTokenReadings],
    sentence_text: &str,
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    const MAX_SUGGESTIONS: usize = 10;
    let pattern_tokens = pattern_token_range(tokens, matched_positions);
    let parsed = resolve_args(args, matched_texts);
    let (Some(word_from), Some(desired_postag)) =
        (parsed.get("wordFrom"), parsed.get("desiredPostag"))
    else {
        return Some(rm);
    };
    let priority_postag = parsed.get("priorityPostag");
    let remove_suggestions_regexp = parsed.get("removeSuggestionsRegexp");
    let suppress_match = parsed
        .get("suppressMatch")
        .is_some_and(|v| v.eq_ignore_ascii_case("true"));
    let diacritics_mode = parsed.get("Mode").is_some_and(|m| m == "diacritics");
    let Some(tagger) = crate::es::tagger::SpanishTagger::shared() else {
        return Some(rm);
    };

    let original_error_str = {
        let from = rm.offset().min(sentence_text.len());
        let to = (rm.offset() + rm.length()).min(sentence_text.len());
        sentence_text.get(from..to).unwrap_or("").to_string()
    };
    let word = if word_from == "inmarker" {
        original_error_str.replace(' ', "")
    } else {
        let index = marker_or_index(word_from, pattern_tokens, rm.offset())?;
        pattern_tokens
            .get(index)
            .map(|atr| atr.token().token().to_string())?
    };

    let is_word_capitalized = is_capitalized_word(&word);
    let is_word_all_upper = is_all_uppercase(&word);
    let original_tagged = og_tagger::Tagger::tag(tagger.as_ref(), &[word.as_str()]);
    if diacritics_mode
        && original_tagged
            .iter()
            .any(|atr| readings_match_pos_tag_regex(atr, desired_postag))
    {
        return None;
    }

    let mut replacements: Vec<String> = Vec::new();
    let mut used_priority_pos = 0usize;
    for suggestion in crate::es::speller::find_similar_words(&word) {
        if replacements.len() >= 2 * MAX_SUGGESTIONS {
            break;
        }
        let suggestion = suggestion.as_str();
        let analyzed = og_tagger::Tagger::tag(tagger.as_ref(), &[suggestion]);
        for atr in &analyzed {
            if suggestion != word && readings_match_pos_tag_regex(atr, desired_postag) {
                let not_present = replacements
                    .iter()
                    .all(|r| r != suggestion && r != &suggestion.to_lowercase());
                if not_present
                    && (!diacritics_mode || equal_without_diacritics(suggestion, &word))
                    && remove_suggestions_regexp
                        .as_ref()
                        .is_none_or(|pattern| !full_match(pattern, suggestion))
                {
                    let mut replacement = suggestion.to_string();
                    if is_word_all_upper {
                        replacement = replacement.to_uppercase();
                    }
                    if is_word_capitalized {
                        replacement = uppercase_first_char(&replacement);
                    }
                    if priority_postag.is_some_and(|p| readings_match_pos_tag_regex(atr, p)) {
                        replacements.insert(used_priority_pos, replacement);
                        used_priority_pos += 1;
                    } else {
                        replacements.push(replacement);
                    }
                }
            }
        }
    }

    let match_contains_some_finished_suggestion = rm
        .replacements()
        .iter()
        .any(|k| !k.value().to_lowercase().contains("{suggestion}"));
    if diacritics_mode && replacements.is_empty() && !match_contains_some_finished_suggestion {
        return None;
    }
    if replacements.is_empty() && suppress_match && !match_contains_some_finished_suggestion {
        return None;
    }

    let mut definitive: Vec<String> = Vec::new();
    let mut replacements_used = false;
    for s in rm.replacements().iter().map(|r| r.value().to_string()) {
        if s.contains("{suggestion}") || s.contains("{Suggestion}") || s.contains("{SUGGESTION}") {
            replacements_used = true;
            for s2 in &replacements {
                if definitive.len() >= MAX_SUGGESTIONS {
                    break;
                }
                if s.contains("{suggestion}") {
                    let candidate = s.replace("{suggestion}", s2);
                    if !definitive.contains(&candidate) {
                        definitive.push(candidate);
                    }
                } else if s.contains("{Suggestion}") {
                    let candidate = s.replace("{Suggestion}", &uppercase_first_char(s2));
                    if !definitive.contains(&candidate) {
                        definitive.push(candidate);
                    }
                } else {
                    let candidate = s.replace("{SUGGESTION}", &s2.to_uppercase());
                    if !definitive.contains(&candidate) {
                        definitive.push(candidate);
                    }
                }
            }
        } else if !definitive.contains(&s) {
            definitive.push(s);
        }
    }
    if !replacements_used {
        for replacement in &replacements {
            if definitive.len() >= MAX_SUGGESTIONS {
                break;
            }
            if !definitive.contains(replacement) {
                definitive.push(replacement.clone());
            }
        }
    }
    if let Some(pos) = definitive.iter().position(|d| d == &original_error_str) {
        definitive.remove(pos);
    }
    let distinct: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        definitive
            .into_iter()
            .filter(|d| seen.insert(d.clone()))
            .collect()
    };
    rm.set_replacements(
        distinct
            .into_iter()
            .map(SuggestedReplacement::new)
            .collect(),
    );
    Some(rm)
}

pub fn spanish_suppress_misspelled_filter(
    mut rm: RuleMatch,
    args: &str,
    matched_texts: &[String],
) -> Option<RuleMatch> {
    let parsed = resolve_args(args, matched_texts);
    let suppress_match = parsed
        .get("suppressMatch")
        .is_none_or(|v| !v.eq_ignore_ascii_case("false"));
    let suppress_postag = parsed.get("SuppressPostag").cloned();
    let filter_postag = parsed.get("FilterPostag").cloned();
    let tagged = if suppress_postag.is_some() || filter_postag.is_some() {
        crate::es::tagger::SpanishTagger::shared().map(|tagger| {
            let refs: Vec<&str> = rm.replacements().iter().map(|r| r.value()).collect();
            og_tagger::Tagger::tag(tagger.as_ref(), &refs)
        })
    } else {
        None
    };
    let mut kept: Vec<SuggestedReplacement> = Vec::new();
    for (i, repl) in rm.replacements().iter().enumerate() {
        let value = repl.value();
        if crate::es::speller::replacement_is_misspelled(value) {
            continue;
        }
        if let Some(sp) = suppress_postag.as_deref()
            && tagged
                .as_ref()
                .and_then(|t| t.get(i))
                .is_some_and(|atr| readings_match_pos_tag_regex(atr, sp))
        {
            continue;
        }
        if let Some(fp) = filter_postag.as_deref()
            && tagged
                .as_ref()
                .and_then(|t| t.get(i))
                .is_some_and(|atr| !readings_match_pos_tag_regex(atr, fp))
        {
            continue;
        }
        kept.push(repl.clone());
    }
    if kept.is_empty() && suppress_match {
        return None;
    }
    rm.set_replacements(kept);
    Some(rm)
}

pub fn spanish_number_in_word_filter(
    args: &str,
    matched_texts: &[String],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let parsed = resolve_args(args, matched_texts);
    let Some(word) = parsed.get("word") else {
        return Some(rm);
    };
    let word_replacing_zero_o = word.replace('0', "o");
    let word_without_number: String = word.chars().filter(|c| !c.is_ascii_digit()).collect();
    let mut replacements: Vec<String> = Vec::new();
    if !crate::es::speller::is_misspelled(&word_replacing_zero_o) && word != &word_replacing_zero_o
    {
        replacements.push(word_replacing_zero_o);
    }
    if !crate::es::speller::is_misspelled(&word_without_number) {
        replacements.push(word_without_number.clone());
    }
    if replacements.is_empty() {
        replacements = crate::es::speller::find_similar_words(&word_without_number);
    }
    if replacements.is_empty() {
        return None;
    }
    rm.set_replacements(
        replacements
            .into_iter()
            .map(SuggestedReplacement::new)
            .collect(),
    );
    Some(rm)
}

fn pac_re(pat: &'static str) -> &'static Regex {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap();
    if let Some(re) = guard.get(pat) {
        return re;
    }
    let re: &'static Regex = Box::leak(Regex::new(pat).expect("pac regex").into());
    guard.insert(pat, re);
    re
}

fn pac_pos(token: &AnalyzedTokenReadings, pat: &'static str) -> bool {
    let re = pac_re(pat);
    token.readings().iter().any(|r| {
        r.pos_tags().iter().any(|t| re.is_match(t))
            || r.pos_tags().is_empty() && re.is_match("UNKNOWN")
    })
}

fn pac_text(token: &AnalyzedTokenReadings, pat: &'static str) -> bool {
    pac_re(pat).is_match(token.token().token())
}

fn pac_reading<'a>(
    token: &'a AnalyzedTokenReadings,
    pat: &'static str,
) -> Option<&'a og_core::AnalyzedToken> {
    let re = pac_re(pat);
    token.readings().iter().find(|r| {
        r.pos_tags().iter().any(|t| re.is_match(t))
            || r.pos_tags().is_empty() && re.is_match("UNKNOWN")
    })
}

const PAC_NOM: &str = r"^N.*$";
const PAC_NOM_MS: &str = r"^(?:N.MS.*|PI0MS000)$";
const PAC_NOM_FS: &str = r"^(?:N.FS.*|PI0FS000)$";
const PAC_NOM_MP: &str = r"^N.MP.*$";
const PAC_NOM_MN: &str = r"^N.MN.*$";
const PAC_NOM_FP: &str = r"^N.FP.*$";
const PAC_NOM_CS: &str = r"^N.CS.*$";
const PAC_NOM_CP: &str = r"^N.CP.*$";
const PAC_NOM_DET: &str = r"^(?:N.*|D[NDA0I].*|PI0[MF]S000)$";
const PAC_GN_ANY: &str = r"^_GN_.*$";
const PAC_GN_MS: &str = r"^_GN_MS$";
const PAC_GN_FS: &str = r"^_GN_FS$";
const PAC_GN_MP: &str = r"^_GN_MP$";
const PAC_GN_FP: &str = r"^_GN_FP$";
const PAC_GN_CS: &str = r"^_GN_[MF]S$";
const PAC_GN_CP: &str = r"^_GN_[MF]P$";
const PAC_DET_ANY: &str = r"^D[NDA0IP].*$";
const PAC_DET_CS: &str = r"^D[NDA0IP]0CS0$";
const PAC_DET_MS: &str = r"^D[NDA0IP]0MS0$";
const PAC_DET_FS: &str = r"^D[NDA0IP]0FS0$";
const PAC_DET_MP: &str = r"^D[NDA0IP]0MP0$";
const PAC_DET_FP: &str = r"^D[NDA0IP]0FP0$";
const PAC_SUBST_MS: &str =
    r"^(?:N.[MC][SN].*|A..[MC][SN].*|V.P..SM.?|PX.MS.*|D[NDA0I]0MS0|PI0MS000)$";
const PAC_SUBST_FS: &str =
    r"^(?:N.[FC][SN].*|A..[FC][SN].*|V.P..SF.?|PX.FS.*|D[NDA0I]0FS0|PI0FS000)$";
const PAC_SUBST_MP: &str = r"^(?:N.[MC][PN].*|A..[MC][PN].*|V.P..PM.?|PX.MP.*|D[NDA0I]0MP0)$";
const PAC_SUBST_FP: &str = r"^(?:N.[FC][PN].*|A..[FC][PN].*|V.P..PF.?|PX.FP.*|D[NDA0I]0FP0)$";
const PAC_SUBST_CP: &str = r"^(?:N.[FMC][PN].*|A..[FMC][PN].*|D[NDA0I]0[FM]P0)$";
const PAC_SUBST_CS: &str = r"^(?:N.[FMC][SN].*|A..[FC][SN].*|D[NDA0I]0[FM]S0||PI0[MFC]S000)$";
const PAC_ADJ_ANY: &str = r"^(?:AQ.*|V.P.*|PX.*|.*LOC_ADJ.*)$";
const PAC_ADJ_MS: &str = r"^(?:A..[MC][SN].*|V.P..SM.?|PX.MS.*)$";
const PAC_ADJ_FS: &str = r"^(?:A..[FC][SN].*|V.P..SF.?|PX.FS.*)$";
const PAC_ADJ_MP: &str = r"^(?:A..[MC][PN].*|V.P..PM.?|PX.MP.*)$";
const PAC_ADJ_FP: &str = r"^(?:A..[FC][PN].*|V.P..PF.?|PX.FP.*)$";
const PAC_ADJ_CP: &str = r"^A..C[PN].*$";
const PAC_ADJ_CS: &str = r"^A..C[SN].*$";
const PAC_ADJ_S: &str = r"^(?:A...[SN].*|V.P..S..?|PX..S.*)$";
const PAC_ADJ_P: &str = r"^(?:A...[PN].*|V.P..P..?|PX..P.*)$";
const PAC_ADVERBI: &str = r"^(?:R.|.*LOC_ADV.*)$";
const PAC_CONJUNCIO: &str = r"^(?:C.|.*LOC_CONJ.*)$";
const PAC_PUNCT: &str = r"^_PUNCT$";
const PAC_LOC_ADV: &str = r"^.*LOC_ADV.*$";
const PAC_ADVERBIS_ACCEPTATS: &str = r"^RG_before$";
const PAC_KEEP_COUNT: &str = r"^(?:A.*|N.*|D[NAIDP].*|SPS.*|SP:DA|.*LOC_ADV.*|V.P.*|_PUNCT.*|.*LOC_ADJ.*|PX.*|PI0.S000|UNKNOWN|V.N.{4})$";
const PAC_KEEP_COUNT2: &str = r"^(?:,|y|e|o|ni|u)$";
const PAC_STOP_COUNT: &str = r"^(?:;|lo)$";
const PAC_PREPOSICIONS: &str = r"^SP.*$";
const PAC_PREP_CANVI_NIVELL: &str = r"^(?:de|del|en|sobre|a|entre|por|con|sin|contra|para)$";
const PAC_COORDINACIO_IONI: &str = r"^(?:y|e|o|u|ni)$";
const PAC_VERB: &str = r"^(?:V.[^P].*|_GV_)$";
const PAC_GV: &str = r"^_GV_$";

struct PacState {
    adverb: bool,
    conjunction: bool,
    punctuation: bool,
}

impl PacState {
    fn new() -> Self {
        Self {
            adverb: false,
            conjunction: false,
            punctuation: false,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn update(&mut self, token: &AnalyzedTokenReadings) {
        if pac_pos(token, PAC_NOM) || pac_pos(token, PAC_ADJ_ANY) {
            self.reset();
            return;
        }
        self.adverb |= pac_pos(token, PAC_ADVERBI);
        self.conjunction |= pac_pos(token, PAC_CONJUNCIO);
        self.punctuation |= pac_pos(token, PAC_PUNCT) || token.token().token() == ",";
    }
}

fn pac_keep_counting(state: &PacState, token: &AnalyzedTokenReadings) -> bool {
    if (pac_pos(token, PAC_PUNCT) || state.conjunction || state.adverb)
        && (state.punctuation || state.conjunction)
        && (state.punctuation || state.adverb)
    {
        return false;
    }
    (pac_pos(token, PAC_KEEP_COUNT)
        || pac_text(token, PAC_KEEP_COUNT2)
        || pac_pos(token, PAC_ADVERBIS_ACCEPTATS))
        && !pac_text(token, PAC_STOP_COUNT)
        && (!pac_pos(token, PAC_GV) || pac_pos(token, PAC_GN_ANY))
}

pub fn postponed_adjective_concordance_filter(
    args: &str,
    matched_positions: &[usize],
    tokens: &[&AnalyzedTokenReadings],
    synthesizer: Option<&og_rules::postponed_adjective::LangSynthesizer>,
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let add_comma = args
        .split_whitespace()
        .any(|part| part.eq_ignore_ascii_case("addComma:true"));
    let i = *matched_positions.first()?;
    if i == 0 || i >= tokens.len() {
        return None;
    }
    let max_levels = 4usize;
    let mut is_plural = true;
    let mut is_prev_noun = false;
    let mut can_be_ms = false;
    let mut can_be_fs = false;
    let mut can_be_mp = false;
    let mut can_be_fp = false;
    let mut can_be_p = false;
    let mut c_nt = [0usize; 4];
    let mut c_nms = [0usize; 4];
    let mut c_nfs = [0usize; 4];
    let mut c_nmp = [0usize; 4];
    let mut c_nmn = [0usize; 4];
    let mut c_nfp = [0usize; 4];
    let mut c_ncs = [0usize; 4];
    let mut c_ncp = [0usize; 4];
    let mut c_dms = [0usize; 4];
    let mut c_dfs = [0usize; 4];
    let mut c_dmp = [0usize; 4];
    let mut c_dfp = [0usize; 4];
    let mut c_n = [0usize; 4];
    let mut c_d = [0usize; 4];
    let mut level = 0usize;
    let mut state = PacState::new();
    let mut j = 1usize;
    while i > j && level < max_levels && pac_keep_counting(&state, tokens[i - j]) {
        let cur = i - j;
        if !is_prev_noun
            && (pac_pos(tokens[cur], PAC_NOM)
                || (cur > 0
                    && !pac_pos(tokens[cur], PAC_NOM)
                    && pac_pos(tokens[cur], PAC_ADJ_ANY)
                    && pac_pos(tokens[cur - 1], PAC_DET_ANY)))
        {
            if pac_pos(tokens[cur], PAC_GN_MS) {
                c_nms[level] += 1;
                can_be_ms = true;
            }
            if pac_pos(tokens[cur], PAC_GN_FS) {
                c_nfs[level] += 1;
                can_be_fs = true;
            }
            if pac_pos(tokens[cur], PAC_GN_MP) {
                c_nmp[level] += 1;
                can_be_mp = true;
            }
            if pac_pos(tokens[cur], PAC_GN_FP) {
                c_nfp[level] += 1;
                can_be_fp = true;
            }
        }
        if !pac_pos(tokens[cur], PAC_GN_ANY) {
            if pac_pos(tokens[cur], PAC_NOM_MS) {
                c_nms[level] += 1;
                can_be_ms = true;
            } else if pac_pos(tokens[cur], PAC_NOM_FS) {
                c_nfs[level] += 1;
                can_be_fs = true;
            } else if pac_pos(tokens[cur], PAC_NOM_MP) {
                c_nmp[level] += 1;
                can_be_mp = true;
            } else if pac_pos(tokens[cur], PAC_NOM_MN) {
                c_nmn[level] += 1;
                can_be_ms = true;
                can_be_mp = true;
            } else if pac_pos(tokens[cur], PAC_NOM_FP) {
                c_nfp[level] += 1;
                can_be_fp = true;
            } else if pac_pos(tokens[cur], PAC_NOM_CS) {
                c_ncs[level] += 1;
                can_be_ms = true;
                can_be_fs = true;
            } else if pac_pos(tokens[cur], PAC_NOM_CP) {
                c_ncp[level] += 1;
                can_be_fp = true;
                can_be_mp = true;
            }
        }
        if pac_pos(tokens[cur], PAC_NOM) {
            c_nt[level] += 1;
            is_prev_noun = true;
        } else {
            is_prev_noun = false;
        }
        if pac_pos(tokens[cur], PAC_DET_CS) {
            if cur + 1 < tokens.len() && pac_pos(tokens[cur + 1], PAC_NOM_MS) {
                c_dms[level] += 1;
                can_be_ms = true;
            }
            if cur + 1 < tokens.len() && pac_pos(tokens[cur + 1], PAC_NOM_FS) {
                c_dfs[level] += 1;
                can_be_fs = true;
            }
        }
        if !pac_pos(tokens[cur], PAC_ADVERBI) {
            if pac_pos(tokens[cur], PAC_DET_MS) {
                c_dms[level] += 1;
                can_be_ms = true;
            }
            if pac_pos(tokens[cur], PAC_DET_FS) {
                c_dfs[level] += 1;
                can_be_fs = true;
            }
            if pac_pos(tokens[cur], PAC_DET_MP) {
                c_dmp[level] += 1;
                can_be_mp = true;
            }
            if pac_pos(tokens[cur], PAC_DET_FP) {
                c_dfp[level] += 1;
                can_be_fp = true;
            }
        }
        if cur > 0
            && pac_text(tokens[cur], PAC_PREP_CANVI_NIVELL)
            && !pac_text(tokens[cur - 1], PAC_COORDINACIO_IONI)
            && cur + 1 < tokens.len()
            && !pac_pos(tokens[cur + 1], PAC_ADVERBI)
        {
            level += 1;
        }
        if level > 0 && pac_text(tokens[cur], PAC_COORDINACIO_IONI) {
            let mut k = 1usize;
            while k < 4 && cur > k && cur - k > 0 {
                let back = cur - k;
                if !((pac_pos(tokens[back], PAC_KEEP_COUNT)
                    || pac_text(tokens[back], PAC_KEEP_COUNT2)
                    || pac_pos(tokens[back], PAC_ADVERBIS_ACCEPTATS))
                    && !pac_text(tokens[back], PAC_STOP_COUNT))
                {
                    break;
                }
                if pac_pos(tokens[back], PAC_PREPOSICIONS) {
                    j += k;
                    break;
                }
                k += 1;
            }
        }
        state.update(tokens[i - j]);
        j += 1;
    }
    level += 1;
    if level > max_levels {
        level = max_levels;
    }
    let mut c_ntotal = 0usize;
    let mut c_dtotal = 0usize;
    let mut j2 = 0usize;
    while j2 < level {
        c_n[j2] = c_nms[j2] + c_nfs[j2] + c_nmp[j2] + c_nfp[j2] + c_ncs[j2] + c_ncp[j2] + c_nmn[j2];
        c_d[j2] = c_dms[j2] + c_dfs[j2] + c_dmp[j2] + c_dfp[j2];
        c_ntotal += c_n[j2];
        c_dtotal += c_d[j2];
        if pac_pos(tokens[i], PAC_ADJ_MP)
            && (c_n[j2] > 1 || c_d[j2] > 1)
            && (c_nms[j2] + c_nmn[j2] + c_nmp[j2] + c_ncs[j2] + c_ncp[j2] + c_dms[j2] + c_dmp[j2])
                > 0
            && (c_nfs[j2] + c_nfp[j2] <= c_nt[j2])
        {
            return None;
        }
        if pac_pos(tokens[i], PAC_ADJ_FP)
            && (c_n[j2] > 1 || c_d[j2] > 1)
            && ((c_nms[j2] + c_nmp[j2] + c_nmn[j2] + c_dms[j2] + c_dmp[j2]) == 0
                || (c_nt[j2] > 0 && c_nfs[j2] + c_nfp[j2] >= c_nt[j2]))
        {
            return None;
        }
        if c_n[j2] + c_d[j2] > 0 {
            is_plural = is_plural && c_d[j2] > 1;
            can_be_p = can_be_p || c_n[j2] > 1;
        }
        j2 += 1;
    }
    if i > 2 && c_nmp[0] + c_nfp[0] + c_ncp[0] > 0 && tokens[i - 2].token().token() == "," {
        is_plural = true;
    }
    if c_ntotal == 0 && c_dtotal == 0 {
        return None;
    }
    let (subst, adj_pat, gn_pat) = if pac_pos(tokens[i], PAC_ADJ_CS) {
        (PAC_SUBST_CS, PAC_ADJ_S, PAC_GN_CS)
    } else if pac_pos(tokens[i], PAC_ADJ_CP) {
        (PAC_SUBST_CP, PAC_ADJ_P, PAC_GN_CP)
    } else if pac_pos(tokens[i], PAC_ADJ_MS) {
        (PAC_SUBST_MS, PAC_ADJ_MS, PAC_GN_MS)
    } else if pac_pos(tokens[i], PAC_ADJ_FS) {
        (PAC_SUBST_FS, PAC_ADJ_FS, PAC_GN_FS)
    } else if pac_pos(tokens[i], PAC_ADJ_MP) {
        (PAC_SUBST_MP, PAC_ADJ_MP, PAC_GN_MP)
    } else if pac_pos(tokens[i], PAC_ADJ_FP) {
        (PAC_SUBST_FP, PAC_ADJ_FP, PAC_GN_FP)
    } else {
        return None;
    };
    let mut j3 = 1usize;
    let mut keep = true;
    while i > j3 && keep {
        let cur = i - j3;
        if pac_pos(tokens[cur], PAC_NOM_DET) && pac_pos(tokens[cur], gn_pat) {
            return None;
        }
        if !pac_pos(tokens[cur], PAC_GN_ANY) && pac_pos(tokens[cur], subst) {
            return None;
        }
        keep = !pac_pos(tokens[cur], PAC_NOM_DET);
        j3 += 1;
    }
    let prev_ok = (pac_pos(tokens[i - 1], PAC_NOM) && !pac_pos(tokens[i - 1], subst))
        || (pac_pos(tokens[i - 1], PAC_ADJ_ANY) && !pac_pos(tokens[i - 1], gn_pat))
        || (pac_pos(tokens[i - 1], PAC_ADJ_ANY) && !pac_pos(tokens[i - 1], adj_pat))
        || (i > 2
            && pac_pos(tokens[i - 1], PAC_ADVERBIS_ACCEPTATS)
            && !pac_pos(tokens[i - 2], PAC_VERB)
            && !pac_pos(tokens[i - 2], PAC_PREPOSICIONS))
        || (i > 3
            && pac_pos(tokens[i - 1], PAC_LOC_ADV)
            && pac_pos(tokens[i - 2], PAC_LOC_ADV)
            && !pac_pos(tokens[i - 3], PAC_VERB)
            && !pac_pos(tokens[i - 3], PAC_PREPOSICIONS));
    if !prev_ok {
        return None;
    }
    if !(is_plural && pac_pos(tokens[i], PAC_ADJ_S)) {
        let mut j4 = 1usize;
        state.reset();
        while i > j4 && pac_keep_counting(&state, tokens[i - j4]) && (level > 1 || j4 < 4) {
            let cur = i - j4;
            if (!pac_pos(tokens[cur], PAC_GN_ANY)
                && pac_pos(tokens[cur], PAC_NOM_DET)
                && pac_pos(tokens[cur], subst))
                || pac_pos(tokens[cur], gn_pat)
            {
                return None;
            }
            state.update(tokens[cur]);
            j4 += 1;
        }
    }
    let synth = synthesizer?;
    let target = tokens[i].token().token();
    let mut suggestions: Vec<String> = Vec::new();
    if let Some(at) = pac_reading(tokens[i], PAC_ADJ_CS)
        && let Some(lemma) = at.lemma()
    {
        suggestions.extend(synth.synthesize(lemma, pac_re(r"^A..CP.$")));
    }
    if suggestions.is_empty()
        && let Some(at) = pac_reading(tokens[i], PAC_ADJ_CP)
        && let Some(lemma) = at.lemma()
    {
        suggestions.extend(synth.synthesize(lemma, pac_re(r"^A..CS.$")));
    }
    if suggestions.is_empty()
        && is_plural
        && let Some(at) = pac_reading(tokens[i], PAC_ADJ_P)
        && let Some(lemma) = at.lemma()
    {
        suggestions.extend(synth.synthesize(lemma, pac_re(r"^(?:A...P.|V.P..P.|PX..P.*)$")));
    }
    if let Some(at) = pac_reading(tokens[i], PAC_ADJ_ANY)
        && let Some(lemma) = at.lemma()
        && suggestions.is_empty()
    {
        if can_be_ms && !is_plural {
            suggestions.extend(synth.synthesize(lemma, pac_re(r"^(?:A..MS.|V.P..SM|PX.MS.*)$")));
        }
        if can_be_fs && !is_plural {
            suggestions.extend(synth.synthesize(lemma, pac_re(r"^(?:A..FS.|V.P..SF|PX.FS.*)$")));
        }
        if can_be_mp {
            suggestions.extend(synth.synthesize(lemma, pac_re(r"^(?:A..MP.|V.P..PM|PX.MP.*)$")));
        }
        if can_be_fp {
            suggestions.extend(synth.synthesize(lemma, pac_re(r"^(?:A..FP.|V.P..PF|PX.FP.*)$")));
        }
        if can_be_ms && (is_plural || can_be_p) {
            suggestions.extend(synth.synthesize(lemma, pac_re(r"^(?:A..MP.|V.P..PM|PX.MP.*)$")));
        }
        if can_be_fs && !can_be_ms && (is_plural || can_be_p) {
            suggestions.extend(synth.synthesize(lemma, pac_re(r"^(?:A..FP.|V.P..PF|PX.FP.*)$")));
        }
    }
    let lower_target = target.to_lowercase();
    suggestions.retain(|s| *s != lower_target);
    if suggestions.is_empty() {
        return None;
    }
    let mut definitive: Vec<String> = Vec::new();
    if add_comma {
        definitive.push(format!(", {target}"));
        for s in &suggestions {
            definitive.push(format!(" {s}"));
        }
        // Java shifts the match to tokenStart-1 keeping the end; our span may start on the space
        let new_start = tokens[i].token().start().saturating_sub(1);
        let new_len = (rm.offset() + rm.length()).saturating_sub(new_start);
        rm.set_offset_length(new_start, new_len);
    } else {
        for s in suggestions {
            if !definitive.contains(&s) {
                definitive.push(s);
            }
        }
    }
    rm.set_replacements(
        definitive
            .into_iter()
            .map(SuggestedReplacement::new)
            .collect(),
    );
    Some(rm)
}

pub fn is_es_filter(class: &str) -> bool {
    matches!(
        class,
        "org.languagetool.rules.es.ConfusionCheckFilter"
            | "org.languagetool.rules.AddCommasFilter"
            | "org.languagetool.rules.AdaptSuggestionsFilter"
            | "org.languagetool.rules.es.AdvancedSynthesizerFilter"
            | "org.languagetool.rules.es.FindSuggestionsFilter"
            | "org.languagetool.rules.es.SpanishSuppressMisspelledSuggestionsFilter"
            | "org.languagetool.rules.es.SpanishNumberInWordFilter"
            | "org.languagetool.rules.es.PostponedAdjectiveConcordanceFilter"
    )
}

pub struct EsFilterCall<'a> {
    pub args: &'a str,
    pub matched_texts: &'a [String],
    pub matched_positions: &'a [usize],
    pub tokens: &'a [&'a AnalyzedTokenReadings],
    pub sentence_text: &'a str,
    pub synthesizer: Option<&'a og_rules::postponed_adjective::LangSynthesizer>,
}

pub fn apply_es_filter(class: &str, call: EsFilterCall<'_>, rm: RuleMatch) -> Option<RuleMatch> {
    match class {
        "org.languagetool.rules.es.ConfusionCheckFilter" => confusion_check_filter(
            call.args,
            call.matched_texts,
            call.matched_positions,
            call.tokens,
            rm,
        ),
        "org.languagetool.rules.AddCommasFilter" => {
            add_commas_filter(call.tokens, 0, call.sentence_text, rm, call.args)
        }
        "org.languagetool.rules.AdaptSuggestionsFilter" => adapt_suggestions_filter(rm),
        "org.languagetool.rules.es.AdvancedSynthesizerFilter" => advanced_synthesizer_filter(
            call.synthesizer,
            call.args,
            call.matched_texts,
            call.matched_positions,
            call.tokens,
            rm,
        ),
        "org.languagetool.rules.es.FindSuggestionsFilter" => find_suggestions_filter(
            call.args,
            call.matched_texts,
            call.matched_positions,
            call.tokens,
            call.sentence_text,
            rm,
        ),
        "org.languagetool.rules.es.SpanishSuppressMisspelledSuggestionsFilter" => {
            spanish_suppress_misspelled_filter(rm, call.args, call.matched_texts)
        }
        "org.languagetool.rules.es.SpanishNumberInWordFilter" => {
            spanish_number_in_word_filter(call.args, call.matched_texts, rm)
        }
        "org.languagetool.rules.es.PostponedAdjectiveConcordanceFilter" => {
            postponed_adjective_concordance_filter(
                call.args,
                call.matched_positions,
                call.tokens,
                call.synthesizer,
                rm,
            )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::es::test_support::SpanishPipeline;
    use og_rules::pattern_rule::PatternRuleEngine;
    use og_xml::compiler::XmlCompiler;
    use std::path::PathBuf;

    fn compiled_rules() -> std::sync::Arc<og_xml::compiler::CompiledRuleSet> {
        static RULES: og_core::releasable::Releasable<og_xml::compiler::CompiledRuleSet> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&RULES, || {
            let path = PathBuf::from(
                "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/grammar.xml",
            );
            let xml = std::fs::read_to_string(&path).unwrap();
            Some(std::sync::Arc::new(
                XmlCompiler::new()
                    .compile_file_with_base(&xml, path.parent())
                    .expect("compile grammar.xml"),
            ))
        })
        .expect("es grammar")
    }

    fn atr(word: &str, tags: &[(&str, &str)]) -> AnalyzedTokenReadings {
        atr_at(word, 0, word.len(), tags)
    }

    fn atr_at(
        word: &str,
        start: usize,
        end: usize,
        tags: &[(&str, &str)],
    ) -> AnalyzedTokenReadings {
        let mut out = AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(word, start, end));
        let readings: Vec<og_core::AnalyzedToken> = tags
            .iter()
            .map(|(lemma, tag)| {
                og_core::AnalyzedToken::new(word, start, end)
                    .with_lemma(*lemma)
                    .with_pos_tags(vec![tag.to_string()])
            })
            .collect();
        *out.readings_mut() = readings;
        out
    }

    fn sent_start() -> AnalyzedTokenReadings {
        AnalyzedTokenReadings::new(
            og_core::AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )
    }

    fn es_synth() -> Option<std::sync::Arc<og_rules::postponed_adjective::LangSynthesizer>> {
        crate::es::es_resource_dir()
            .and_then(|dir| {
                og_rules::postponed_adjective::LangSynthesizer::for_language(&dir, "es")
            })
            .map(std::sync::Arc::new)
    }

    fn base_rm(message: &str, offset: usize, length: usize, template: &str) -> RuleMatch {
        RuleMatch::new(
            message,
            offset,
            length,
            og_core::RuleMatchRule::new("X", "d"),
            og_core::RuleMatchContext::new("", 0, 0),
        )
        .with_replacements(vec![SuggestedReplacement::new(template)])
    }

    #[test]
    fn confusion_check_ports_mydia02_shape() {
        let base = atr("Maquina", &[("maquinar", "VMIP3S0"), ("maca", "NCFS00D")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&base];
        let matched = vec!["Maquina".to_string()];
        let rm = base_rm(
            "Si es adjetivo o nombre, se escribe con tilde.",
            0,
            7,
            "{suggestion}",
        );
        let out = confusion_check_filter("form:\\1 postag:[AN].*", &matched, &[0], &tokens, rm)
            .expect("maquina -> máquina");
        assert_eq!(out.replacements()[0].value(), "Máquina");
        assert_eq!(
            out.message(),
            "Si es adjetivo o nombre, se escribe con tilde."
        );
    }

    #[test]
    fn confusion_check_rejects_wrong_gender() {
        let base = atr("practicas", &[("x", "AQ0MP0")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&base];
        let matched = vec!["practicas".to_string()];
        let rm = base_rm("m", 0, 9, "{suggestion}");
        assert!(pairs_contain("practicas", "prácticas", "NCFP000"));
        let out = confusion_check_filter(
            "form:\\1 postag:[AN].* gendernumberFrom:1",
            &matched,
            &[0],
            &tokens,
            rm,
        );
        assert!(
            out.is_none(),
            "first reading NCFP000 must not match MS pattern"
        );
    }

    fn pairs_contain(form: &str, word: &str, tag: &str) -> bool {
        confusion_pairs()
            .get(form)
            .is_some_and(|e| e.contains(&(word.to_string(), tag.to_string())))
    }

    #[test]
    fn confusion_check_rewrites_message_without_diacritic_gain() {
        let base = atr("ralla", &[("rallar", "VMIP3S0")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&base];
        let matched = vec!["ralla".to_string()];
        let rm = base_rm("m se escribe con tilde x", 0, 5, "{suggestion}");
        let out = confusion_check_filter("form:\\1 postag:[AN].*", &matched, &[0], &tokens, rm)
            .expect("ralla -> raya");
        assert_eq!(out.message(), "m se escribe de otra manera x");
        assert_eq!(out.replacements()[0].value(), "raya");
    }

    #[test]
    fn add_commas_ports_marker_examples() {
        let pipeline = SpanishPipeline::shared();
        let engine = PatternRuleEngine::new();
        let rule_set = compiled_rules();
        for (text, expect_start, expect_len, expect_first) in [
            (
                "Es por lo tanto lo que hay que hacer.",
                2usize,
                13usize,
                ", por lo tanto,",
            ),
            ("Y por otro lado no logró ganar.", 1, 14, ", por otro lado,"),
        ] {
            let sentence = &pipeline.analyze_text(text)[0];
            let tokens: Vec<&og_core::AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
            let mut best: Option<(usize, usize, String)> = None;
            for rule in rule_set
                .rules
                .iter()
                .filter(|r| r.id == "COMMA_MARCADOR_DISCURSIVO")
            {
                for raw in engine.match_rule(rule, sentence) {
                    if let Some(out) = add_commas_filter(
                        &tokens,
                        sentence.start(),
                        sentence.text(),
                        raw,
                        "none:none",
                    ) {
                        let candidate = (
                            out.offset(),
                            out.length(),
                            out.replacements()[0].value().to_string(),
                        );
                        if candidate.2 == expect_first {
                            best = Some(candidate);
                        }
                    }
                }
            }
            let (start, len, first) =
                best.unwrap_or_else(|| panic!("no repositioned match for {text}"));
            assert_eq!((start, len), (expect_start, expect_len), "{text}");
            assert_eq!(first, expect_first, "{text}");
        }
    }

    #[test]
    fn add_commas_ports_semicolon_variant() {
        let pipeline = SpanishPipeline::shared();
        let engine = PatternRuleEngine::new();
        let rule_set = compiled_rules();
        let text = "Me gusta mucho este, sin embargo prefiero aquel.";
        let rule = rule_set
            .rules
            .iter()
            .filter(|r| r.id == "COMMA_MARCADOR_DISCURSIVO")
            .find(|r| {
                r.filter
                    .as_ref()
                    .is_some_and(|f| f.args.contains("suggestSemicolon"))
            })
            .expect("semicolon subrule");
        let sentence = &pipeline.analyze_text(text)[0];
        let tokens: Vec<&og_core::AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
        let raw = engine
            .match_rule(rule, sentence)
            .into_iter()
            .next()
            .expect("raw match");
        let out = add_commas_filter(
            &tokens,
            sentence.start(),
            sentence.text(),
            raw,
            "suggestSemicolon:true",
        )
        .expect("repositioned match");
        assert_eq!((out.offset(), out.length()), (19, 13));
        assert_eq!(out.replacements()[0].value(), "; sin embargo,");
        assert_eq!(out.replacements()[1].value(), ", sin embargo,");
    }

    #[test]
    fn add_commas_treats_inverted_exclamation_as_opening_quote() {
        assert!(is_opening_quote("¡"));
        let start = sent_start();
        let va = atr_at("va", 0, 2, &[("ir", "VMIP3S0")]);
        let tanto = atr_at("tanto", 3, 8, &[("tanto", "RG")]);
        let opening = atr_at("¡", 9, 10, &[("¡", "_PUNCT")]);
        let alto = atr_at("alto", 11, 15, &[("alto", "AQ0MS0")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&start, &va, &tanto, &opening, &alto];
        let rm = base_rm("m", 3, 5, "x");
        let out = add_commas_filter(&tokens, 0, "va tanto ¡alto!", rm, "none:none")
            .expect("repositioned match");
        assert_eq!((out.offset(), out.length()), (2, 6));
        assert_eq!(out.replacements()[0].value(), ", tanto,");
    }

    #[test]
    fn adapt_suggestions_contracts_spanish_contractions() {
        assert_eq!(adapt_suggestion_es("a el"), "al");
        assert_eq!(adapt_suggestion_es("de el perro"), "del perro");
        assert_eq!(adapt_suggestion_es("A el"), "Al");
        assert_eq!(adapt_suggestion_es("aleatorio"), "aleatorio");
        let rm = base_rm("m", 0, 5, "a el");
        let out = adapt_suggestions_filter(rm).expect("always kept");
        assert_eq!(out.replacements()[0].value(), "al");
    }

    #[test]
    fn advanced_synthesizer_ports_agreement_ordinals_shape() {
        let synth = es_synth();
        let start = sent_start();
        let t1 = atr_at("vigésimo", 0, 8, &[("20", "AO0MS0")]);
        let t2 = atr_at("primera", 9, 16, &[("primero", "AO0FS0")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&start, &t1, &t2];
        let mut rm = base_rm("m", 0, 16, "x");
        rm.set_replacements(vec![
            SuggestedReplacement::new("vigesimoprimera"),
            SuggestedReplacement::new("{suggestion} primera"),
        ]);
        let out = advanced_synthesizer_filter(
            synth.as_deref(),
            "lemmaFrom:1 lemmaSelect:AO.* postagFrom:2 postagSelect:AO.*",
            &["vigésimo".to_string(), "primera".to_string()],
            &[1, 2],
            &tokens,
            rm,
        )
        .expect("match kept");
        let values: Vec<String> = out
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        assert!(
            values.contains(&"vigésima primera".to_string()),
            "synthesized ordinal must fill the template: {values:?}"
        );
    }

    #[test]
    fn advanced_synthesizer_composes_postag_replace_groups() {
        let synth = es_synth();
        let start = sent_start();
        let det = atr_at("la", 0, 2, &[("el", "DA0FS0")]);
        let noun = atr_at("tres", 3, 7, &[("3", "_GN_FP")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&start, &det, &noun];
        let mut rm = base_rm("m", 0, 7, "{suggestion}");
        rm.set_replacements(vec![SuggestedReplacement::new("{suggestion}")]);
        let out = advanced_synthesizer_filter(
            synth.as_deref(),
            "lemmaFrom:1 lemmaSelect:D(..)..(.) postagFrom:2 postagSelect:_GN_(.)(.) postagReplace:D\\a1[C\\b1]\\b2\\a2",
            &["la".to_string(), "tres".to_string()],
            &[1, 2],
            &tokens,
            rm,
        )
        .expect("match kept");
        let values: Vec<String> = out
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        assert_eq!(values, vec!["las".to_string()]);
    }

    #[test]
    fn advanced_synthesizer_resolves_marker_and_capitalization() {
        let synth = es_synth();
        let start = sent_start();
        let alta = atr_at("alta", 0, 4, &[("alto", "AQ0FS0")]);
        let guapa = atr_at("Guapa", 5, 10, &[("guapo", "AQ0MS0")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&start, &alta, &guapa];
        let mut rm = base_rm("m", 5, 5, "{suggestion}");
        rm.set_replacements(vec![SuggestedReplacement::new("{suggestion}")]);
        let out = advanced_synthesizer_filter(
            synth.as_deref(),
            "lemmaFrom:marker lemmaSelect:A.* postagFrom:1 postagSelect:A.*",
            &["alta".to_string(), "Guapa".to_string()],
            &[1, 2],
            &tokens,
            rm,
        )
        .expect("match kept");
        let values: Vec<String> = out
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        assert_eq!(
            values,
            vec!["Guapa".to_string()],
            "lemma from marker token 'Guapa' synthesized with postag AQ0FS0 from token 1, capitalized"
        );
    }

    #[test]
    fn advanced_synthesizer_passes_through_without_synth_hits() {
        let synth = es_synth();
        let start = sent_start();
        let t1 = atr_at("xyzzy", 0, 6, &[("xyzzy", "ZZ0MS0")]);
        let t2 = atr_at("plugh", 7, 12, &[("plugh", "ZZ0FS0")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&start, &t1, &t2];
        let mut rm = base_rm("m", 0, 12, "{suggestion}");
        rm.set_replacements(vec![SuggestedReplacement::new("{suggestion}")]);
        let out = advanced_synthesizer_filter(
            synth.as_deref(),
            "lemmaFrom:1 lemmaSelect:ZZ.* postagFrom:2 postagSelect:ZZ.*",
            &["xyzzy".to_string(), "plugh".to_string()],
            &[1, 2],
            &tokens,
            rm,
        )
        .expect("match kept");
        assert_eq!(out.replacements()[0].value(), "{suggestion}");
    }

    fn bare_rm(message: &str, offset: usize, length: usize) -> RuleMatch {
        RuleMatch::new(
            message,
            offset,
            length,
            og_core::RuleMatchRule::new("X", "d"),
            og_core::RuleMatchContext::new("", 0, 0),
        )
    }

    #[test]
    fn find_suggestions_pins_the_inmarker_apostrophe_example() {
        let pipeline = SpanishPipeline::shared();
        let engine = PatternRuleEngine::new();
        let rule_set = compiled_rules();
        let text = "Est`n aquí.";
        let sentence = &pipeline.analyze_text(text)[0];
        let tokens: Vec<&og_core::AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
        let rule = rule_set
            .rules
            .iter()
            .filter(|r| r.id == "APOSTROFO_ACENTO")
            .find(|r| {
                r.filter
                    .as_ref()
                    .is_some_and(|f| f.args.contains("wordFrom:inmarker"))
            })
            .expect("inmarker subrule");
        let raw = engine
            .match_rule(rule, sentence)
            .into_iter()
            .next()
            .expect("raw match on Est`n");
        assert!(
            raw.replacements().is_empty(),
            "the rule has no <suggestion> element"
        );
        let matched_positions = vec![1usize, 2, 3];
        let matched_texts: Vec<String> = matched_positions
            .iter()
            .map(|&i| tokens[i].token().token().to_string())
            .collect();
        let out = find_suggestions_filter(
            "wordFrom:inmarker desiredPostag:.* suppressMatch:true",
            &matched_texts,
            &matched_positions,
            &tokens,
            sentence.text(),
            raw,
        )
        .expect("match kept with suggestions");
        let values: Vec<String> = out
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        assert!(
            values.starts_with(&["Están".to_string(), "Estén".to_string()]),
            "correction pair must lead the suggestions: {values:?}"
        );
    }

    #[test]
    fn find_suggestions_suppresses_when_no_dictionary_candidate() {
        let start = sent_start();
        let word = atr_at("O’neill", 0, 7, &[]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&start, &word];
        let rm = bare_rm("Posible error de ortografía.", 1, 6);
        let out = find_suggestions_filter(
            "wordFrom:inmarker desiredPostag:.* suppressMatch:true",
            &["O’neill".to_string()],
            &[1],
            &tokens,
            "O’neill",
            rm,
        );
        assert!(out.is_none(), "no candidates in the dict means no match");
    }

    #[test]
    fn suppress_misspelled_drops_joined_and_keeps_known_suggestions() {
        let rm = base_rm("m", 0, 5, "imporntate");
        assert!(spanish_suppress_misspelled_filter(rm, "suppressMatch:true", &[]).is_none());
        let rm = base_rm("m", 0, 5, "importante");
        let kept = spanish_suppress_misspelled_filter(rm, "suppressMatch:true", &[])
            .expect("known suggestion kept");
        assert_eq!(kept.replacements()[0].value(), "importante");
        let rm = base_rm("m", 0, 5, "x").with_replacements(vec![
            SuggestedReplacement::new("imporntate"),
            SuggestedReplacement::new("importante"),
        ]);
        let kept = spanish_suppress_misspelled_filter(rm, "suppressMatch:true", &[])
            .expect("match kept while a suggestion survives");
        assert_eq!(kept.replacements().len(), 1);
        assert_eq!(kept.replacements()[0].value(), "importante");
    }

    #[test]
    fn number_in_word_pins_the_buen0s_example() {
        let pipeline = SpanishPipeline::shared();
        let engine = PatternRuleEngine::new();
        let rule_set = compiled_rules();
        let text = "¡Buen0s días!";
        let sentence = &pipeline.analyze_text(text)[0];
        let rule = rule_set
            .rules
            .iter()
            .find(|r| r.id == "NUMBERS_IN_WORDS")
            .expect("NUMBERS_IN_WORDS rule");
        let raw = engine
            .match_rule(rule, sentence)
            .into_iter()
            .next()
            .expect("raw match on Buen0s");
        let out = spanish_number_in_word_filter("word:\\1", &["Buen0s".to_string()], raw)
            .expect("match kept");
        let values: Vec<String> = out
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        assert_eq!(values, vec!["Buenos".to_string()]);
    }

    #[test]
    fn dispatch_recognizes_exactly_the_eight_es_filter_classes() {
        for class in [
            "org.languagetool.rules.es.ConfusionCheckFilter",
            "org.languagetool.rules.AddCommasFilter",
            "org.languagetool.rules.AdaptSuggestionsFilter",
            "org.languagetool.rules.es.AdvancedSynthesizerFilter",
            "org.languagetool.rules.es.FindSuggestionsFilter",
            "org.languagetool.rules.es.SpanishSuppressMisspelledSuggestionsFilter",
            "org.languagetool.rules.es.SpanishNumberInWordFilter",
            "org.languagetool.rules.es.PostponedAdjectiveConcordanceFilter",
        ] {
            assert!(is_es_filter(class), "{class} must route");
        }
        assert!(!is_es_filter(
            "org.languagetool.rules.pt.ConfusionCheckFilter"
        ));
        assert!(!is_es_filter(
            "org.languagetool.rules.fr.FindSuggestionsFilter"
        ));
        assert!(!is_es_filter("org.languagetool.rules.ca.DateCheckFilter"));
        assert!(!is_es_filter(""));
    }

    #[test]
    fn dispatch_routes_the_three_new_filters() {
        let start = sent_start();
        let word = atr_at("Buen0s", 0, 6, &[("Buen0s", "UNKNOWN")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&start, &word];
        let rm = base_rm("Posible error ortográfico.", 1, 5, "x");
        let out = apply_es_filter(
            "org.languagetool.rules.es.SpanishNumberInWordFilter",
            EsFilterCall {
                args: "word:\\1",
                matched_texts: &["Buen0s".to_string()],
                matched_positions: &[1],
                tokens: &tokens,
                sentence_text: "Buen0s",
                synthesizer: None,
            },
            rm,
        )
        .expect("match kept");
        assert_eq!(out.replacements()[0].value(), "Buenos");

        let rm = base_rm("m", 0, 5, "imporntate");
        let out = apply_es_filter(
            "org.languagetool.rules.es.SpanishSuppressMisspelledSuggestionsFilter",
            EsFilterCall {
                args: "suppressMatch:true",
                matched_texts: &[],
                matched_positions: &[],
                tokens: &tokens,
                sentence_text: "imporntate",
                synthesizer: None,
            },
            rm,
        );
        assert!(out.is_none());

        let word = atr_at("Est`n", 0, 5, &[]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&start, &word];
        let rm = bare_rm("Posible error de ortografía.", 0, 5);
        let out = apply_es_filter(
            "org.languagetool.rules.es.FindSuggestionsFilter",
            EsFilterCall {
                args: "wordFrom:inmarker desiredPostag:.* suppressMatch:true",
                matched_texts: &["Est`n".to_string()],
                matched_positions: &[1],
                tokens: &tokens,
                sentence_text: "Est`n",
                synthesizer: None,
            },
            rm,
        )
        .expect("match kept");
        let values: Vec<String> = out
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        assert!(
            values.starts_with(&["Están".to_string(), "Estén".to_string()]),
            "correction pair must lead the suggestions: {values:?}"
        );
    }

    fn position_of(
        tokens: &[&AnalyzedTokenReadings],
        offset: usize,
        length: usize,
    ) -> Option<usize> {
        tokens
            .iter()
            .position(|t| offset < t.token().end() && t.token().start() < offset + length)
    }

    fn postponed_rule<'a>(
        rule_set: &'a og_xml::compiler::CompiledRuleSet,
        sub_id: &str,
    ) -> &'a og_xml::compiler::CompiledRule {
        rule_set
            .rules
            .iter()
            .find(|r| {
                r.id == "AGREEMENT_POSTPONED_ADJ"
                    && r.sub_id.as_deref() == Some(sub_id)
                    && r.filter
                        .as_ref()
                        .is_some_and(|f| f.class.ends_with("PostponedAdjectiveConcordanceFilter"))
            })
            .unwrap_or_else(|| panic!("postponed sub-rule {sub_id} carrying the filter"))
    }

    fn run_postponed(text: &str, sub_id: &str) -> Vec<(usize, usize, Vec<String>)> {
        let pipeline = SpanishPipeline::shared();
        let rule_set = compiled_rules();
        let rule = postponed_rule(&rule_set, sub_id);
        let synth = crate::es::es_resource_dir().and_then(|dir| {
            og_rules::postponed_adjective::LangSynthesizer::for_language(&dir, "es")
                .map(std::sync::Arc::new)
        });
        let mut engine = PatternRuleEngine::new();
        if let Some(ref synth) = synth {
            engine = engine
                .with_synthesizer(synth.clone())
                .with_suggestion_synthesizer(synth.clone());
        }
        let mut out = Vec::new();
        for sentence in pipeline.analyze_text(text) {
            let tokens: Vec<&og_core::AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
            for raw in engine.match_rule(rule, &sentence) {
                eprintln!(
                    "raw match @{}..{} on {text:?}",
                    raw.offset(),
                    raw.offset() + raw.length()
                );
                let Some(pos) = position_of(&tokens, raw.offset(), raw.length()) else {
                    continue;
                };
                eprintln!(
                    "token {:?} pos {pos} readings {:?}",
                    tokens[pos].token().token(),
                    tokens[pos]
                        .readings()
                        .iter()
                        .map(|r| (r.lemma(), r.pos_tags()))
                        .collect::<Vec<_>>()
                );
                let texts = vec![tokens[pos].token().token().to_string()];
                let call = EsFilterCall {
                    args: rule.filter.as_ref().map(|f| f.args.as_str()).unwrap_or(""),
                    matched_texts: &texts,
                    matched_positions: &[pos],
                    tokens: &tokens,
                    sentence_text: sentence.text(),
                    synthesizer: synth.as_deref(),
                };
                if let Some(m) = apply_es_filter(
                    "org.languagetool.rules.es.PostponedAdjectiveConcordanceFilter",
                    call,
                    raw,
                ) {
                    let values = m
                        .replacements()
                        .iter()
                        .map(|r| r.value().to_string())
                        .collect();
                    out.push((m.offset(), m.length(), values));
                }
            }
        }
        out
    }

    #[test]
    fn postponed_adj_keeps_entrega_productos_only_via_dict_invented_forms() {
        let out = run_postponed("Entrega productos.", "3");
        assert_eq!(out.len(), 1, "one kept match: {out:?}");
        assert_eq!(out[0].2, vec!["producta".to_string()]);
    }

    #[test]
    fn postponed_adj_keeps_musgo_only_via_dict_invented_forms() {
        let out = run_postponed("Una piedra rodante junta musgo.", "3");
        assert_eq!(out.len(), 1, "one kept match on musgo: {out:?}");
        assert_eq!(out[0].0, 25);
        assert_eq!(out[0].2, vec!["musga".to_string(), "musgosa".to_string()]);
    }

    #[test]
    fn postponed_adj_filter_suggests_blanco_blanca() {
        let out = run_postponed("La casa del pueblo blancas.", "3");
        assert_eq!(out.len(), 1, "exactly one kept match: {out:?}");
        assert_eq!(out[0].0, 19);
        assert_eq!(out[0].1, 7);
        assert_eq!(out[0].2, vec!["blanco".to_string(), "blanca".to_string()]);
    }

    #[test]
    #[ignore]
    fn postponed_adj_probe_remaining_fps() {
        let _release_guard = crate::release_guard();
        for text in [
            "Se dan los ciervos, gamos, corzos, muflones y la cabra montés.",
            "Los alumnos de 4º de la ESO de Carmelitas premiados en el concurso europeo.",
            "Abandonaron el país transcurridas diez semanas.",
            "Abandonaron el país transcurridos más de diez meses.",
            "Una piedra rodante no junta musgo.",
            "Muestra resultados.",
            "El impulso motriz.",
            "El domingo pasada hubo corrida de toros de lujo.",
            "Análisis clínica.",
            "Son casas rojos.",
            "Estas son las sillas blancos de las que te hablé.",
            "Un loro puede imitar el habla humano.",
            "El domingo pasada hubo corrida.",
        ] {
            eprintln!("{text:?} -> {:?}", run_postponed(text, "3"));
        }
    }

    fn run_concordance(tokens: Vec<AnalyzedTokenReadings>) -> Option<RuleMatch> {
        let i = tokens.len() - 1;
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let rm = base_rm("m", 0, 5, "{suggestion}");
        postponed_adjective_concordance_filter("", &[i], &refs, es_synth().as_deref(), rm)
    }

    fn replacements_of(rm: RuleMatch) -> Vec<String> {
        rm.replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect()
    }

    #[test]
    fn es_filter_registry_recognizes_the_java_filter_classes() {
        for class in [
            "org.languagetool.rules.es.ConfusionCheckFilter",
            "org.languagetool.rules.AddCommasFilter",
            "org.languagetool.rules.AdaptSuggestionsFilter",
            "org.languagetool.rules.es.AdvancedSynthesizerFilter",
            "org.languagetool.rules.es.FindSuggestionsFilter",
            "org.languagetool.rules.es.SpanishSuppressMisspelledSuggestionsFilter",
            "org.languagetool.rules.es.SpanishNumberInWordFilter",
            "org.languagetool.rules.es.PostponedAdjectiveConcordanceFilter",
        ] {
            assert!(
                is_es_filter(class),
                "{class} must route to the es filter table"
            );
        }
        assert!(!is_es_filter("org.languagetool.rules.es.UnknownFilter"));
    }

    #[test]
    fn postponed_adj_pipeline_walks_coordination_and_preposition_levels() {
        let out = run_postponed(
            "Se dan los ciervos, gamos, corzos, muflones y la cabra montés.",
            "3",
        );
        assert_eq!(
            out,
            vec![(55, 7, vec!["monteses".to_string(), "montesas".to_string()])]
        );
        let out = run_postponed(
            "Los alumnos de 4º de la ESO de Carmelitas premiados en el concurso europeo.",
            "3",
        );
        assert_eq!(
            out,
            vec![(
                43,
                9,
                vec!["premiadoras".to_string(), "premiadas".to_string()]
            )]
        );
        let out = run_postponed("Abandonaron el país transcurridas diez semanas.", "3");
        assert_eq!(out, vec![(21, 13, vec!["transcurrido".to_string()])]);
        let out = run_postponed("Son casas rojos.", "3");
        assert_eq!(out, vec![(10, 5, vec!["rojas".to_string()])]);
        let out = run_postponed("Análisis clínica.", "3");
        assert_eq!(
            out,
            vec![(10, 8, vec!["clínico".to_string(), "clínicos".to_string()])]
        );
    }

    #[test]
    fn postponed_adj_coordination_scans_back_to_the_preposition() {
        let out = run_concordance(vec![
            sent_start(),
            atr("de", &[("de", "SPS00")]),
            atr("la", &[("la", "DA0FS0")]),
            atr("mesa", &[("mesa", "NCFS000")]),
            atr("y", &[("y", "CC")]),
            atr("casa", &[("casa", "NCFS000")]),
            atr("de", &[("de", "SPS00")]),
            atr("la", &[("la", "DA0FS0")]),
            atr("silla", &[("silla", "NCFS000")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ])
        .expect("the feminine singular context must yield a suggestion");
        assert_eq!(replacements_of(out), vec!["blanca".to_string()]);
        let out = run_concordance(vec![
            sent_start(),
            atr("de", &[("de", "SPS00")]),
            atr("la", &[("la", "DA0FS0")]),
            atr("corre", &[("correr", "VMIP3S0")]),
            atr("y", &[("y", "CC")]),
            atr("de", &[("de", "SPS00")]),
            atr("la", &[("la", "DA0FS0")]),
            atr("silla", &[("silla", "NCFS000")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ])
        .expect("a verb behind the coordination must only cut the scan short");
        assert_eq!(replacements_of(out), vec!["blanca".to_string()]);
        let out = run_concordance(vec![
            sent_start(),
            atr("de", &[("de", "SPS00")]),
            atr("la", &[("la", "DA0FS0")]),
            atr("mesa", &[("mesa", "NCFS000")]),
            atr("y", &[("y", "CC")]),
            atr("y", &[("y", "CC")]),
            atr("de", &[("de", "SPS00")]),
            atr("la", &[("la", "DA0FS0")]),
            atr("silla", &[("silla", "NCFS000")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ]);
        assert!(
            out.is_none(),
            "a preposition behind a coordination opens no new level, so the doubled \
             feminine counts make the feminine plural agree"
        );
        let out = run_concordance(vec![
            sent_start(),
            atr("y", &[("y", "CC")]),
            atr("y", &[("y", "CC")]),
            atr("mesa", &[("mesa", "NCFS000")]),
            atr("de", &[("de", "SPS00")]),
            atr("y", &[("y", "CC")]),
            atr("silla", &[("silla", "NCFS000")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ])
        .expect("a coordination kept alive only by its text must still count");
        assert_eq!(replacements_of(out), vec!["blanca".to_string()]);
        let out = run_concordance(vec![
            sent_start(),
            atr("corre", &[("correr", "VMIP3S0")]),
            atr("y", &[("y", "CC")]),
            atr("mesa", &[("mesa", "NCFS000")]),
            atr("de", &[("de", "SPS00")]),
            atr("y", &[("y", "CC")]),
            atr("silla", &[("silla", "NCFS000")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ])
        .expect("a verb behind the scanned coordination must only cut the scan short");
        assert_eq!(replacements_of(out), vec!["blanca".to_string()]);
    }

    #[test]
    fn postponed_adj_plural_adjectives_survive_coordinated_singulars_only() {
        let out = run_concordance(vec![
            sent_start(),
            atr("los", &[("el", "DA0MP0")]),
            atr("coches", &[("coche", "NCMP000")]),
            atr("y", &[("y", "CC")]),
            atr("barcos", &[("barco", "NCMP000")]),
            atr("rojos", &[("rojo", "AQ0MP0")]),
        ]);
        assert!(
            out.is_none(),
            "a masculine plural after two masculine plurals agrees"
        );
        let out = run_concordance(vec![
            sent_start(),
            atr("las", &[("el", "DA0FP0")]),
            atr("casas", &[("casa", "NCFP000")]),
            atr("y", &[("y", "CC")]),
            atr("sillas", &[("silla", "NCFP000")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ]);
        assert!(
            out.is_none(),
            "a feminine plural after two feminine plurals agrees"
        );
        let out = run_concordance(vec![
            sent_start(),
            atr("el", &[("el", "DA0MS0")]),
            atr("casas", &[("casa", "NCFS000")]),
            atr("y", &[("y", "CC")]),
            atr("mesa", &[("mesa", "NCFS000")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ]);
        assert!(
            out.is_none(),
            "feminine nouns covering every counted noun still block the feminine plural"
        );
    }

    #[test]
    fn postponed_adj_comma_before_plural_noun_forces_the_plural_form() {
        let out = run_concordance(vec![
            sent_start(),
            atr("los", &[("el", "DA0MP0")]),
            atr("pájaros", &[("pájaro", "NCMP000")]),
            atr(",", &[(",", "_PUNCT")]),
            atr("muy", &[("muy", "RG_before")]),
            atr("rápido", &[("rápido", "AQ0MS0")]),
        ])
        .expect("the comma after a plural noun must force the plural suggestion");
        assert_eq!(replacements_of(out), vec!["rápidos".to_string()]);
    }

    #[test]
    fn postponed_adj_stays_silent_without_counted_nouns_or_determiners() {
        let out = run_concordance(vec![
            sent_start(),
            atr("corre", &[("correr", "VMIP3S0")]),
            atr("rápido", &[("rápido", "AQ0MS0")]),
        ]);
        assert!(
            out.is_none(),
            "a verb stops the walk before anything is counted"
        );
    }

    #[test]
    fn postponed_adj_common_gender_tags_count_both_genders() {
        let out = run_concordance(vec![
            sent_start(),
            atr("el", &[("el", "DD0CS0")]),
            atr("problema", &[("problema", "NCMS000")]),
            atr("fácil", &[("fácil", "AQ0CS0")]),
        ]);
        assert!(
            out.is_none(),
            "the agreeing common-gender pair must veto the match"
        );
        let out = run_concordance(vec![
            sent_start(),
            atr("la", &[("el", "DD0CS0")]),
            atr("casa", &[("casa", "NCFS000")]),
            atr("fácil", &[("fácil", "AQ0CS0")]),
        ]);
        assert!(out.is_none());
        let out = run_concordance(vec![
            sent_start(),
            atr("la", &[("el", "DA0FS0")]),
            atr("artistas", &[("artista", "NCCS000")]),
            atr("fácil", &[("fácil", "AQ0CS0")]),
        ]);
        assert!(
            out.is_none(),
            "the N.CS noun matches the common singular substitution"
        );
    }

    #[test]
    fn postponed_adj_common_gender_adjectives_synthesize_the_other_number() {
        let out = run_concordance(vec![
            sent_start(),
            atr("el", &[("el", "DA0MS0")]),
            atr("problema", &[("problema", "NCMS000")]),
            atr("fáciles", &[("fácil", "AQ0CP0")]),
        ])
        .expect("a common plural adjective after a singular noun must suggest the singular");
        assert_eq!(replacements_of(out), vec!["fácil".to_string()]);
        let out = run_concordance(vec![
            sent_start(),
            atr("los", &[("el", "DA0MP0")]),
            atr("problemas", &[("problema", "NCCP000")]),
            atr("fácil", &[("fácil", "AQ0CS0")]),
        ])
        .expect("a common singular adjective after a plural noun must suggest the plural");
        assert_eq!(replacements_of(out), vec!["fáciles".to_string()]);
    }

    #[test]
    fn postponed_adj_requires_an_adjective_reading_at_the_match() {
        let out = run_concordance(vec![
            sent_start(),
            atr("la", &[("el", "DA0FS0")]),
            atr("casa", &[("casa", "NCFS000")]),
            atr("casas", &[("casa", "NCFP000")]),
        ]);
        assert!(
            out.is_none(),
            "without an adjective reading no substitution class exists"
        );
    }

    #[test]
    fn postponed_adj_gn_marked_readings_veto_the_match() {
        let out = run_concordance(vec![
            sent_start(),
            atr("la", &[("el", "DA0FS0")]),
            atr("sillas", &[("silla", "NCFS000"), ("silla", "_GN_FP")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ]);
        assert!(
            out.is_none(),
            "a noun already marked as feminine plural agrees"
        );
        let out = run_concordance(vec![
            sent_start(),
            atr("los", &[("el", "DA0MP0")]),
            atr("coches", &[("coche", "NCMP000"), ("coche", "_GN_MP")]),
            atr("rojos", &[("rojo", "AQ0MP0")]),
        ]);
        assert!(
            out.is_none(),
            "a noun already marked as masculine plural agrees"
        );
        let out = run_concordance(vec![
            sent_start(),
            atr("la", &[("el", "DA0FS0"), ("el", "_GN_FP")]),
            atr("sillas", &[("silla", "NCFS000")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ]);
        assert!(
            out.is_none(),
            "a determiner closing a feminine plural group agrees"
        );
    }

    #[test]
    fn postponed_adj_adjective_predecessor_uses_its_own_gender() {
        let out = run_concordance(vec![
            sent_start(),
            atr("la", &[("el", "DA0FS0")]),
            atr("bueno", &[("bueno", "AQ0MS0")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ])
        .expect("a masculine adjective predecessor must yield the feminine singular");
        assert_eq!(replacements_of(out), vec!["blanca".to_string()]);
        let out = run_concordance(vec![
            sent_start(),
            atr("la", &[("el", "DA0FS0")]),
            atr("bueno", &[("bueno", "AQ0MS0"), ("bueno", "_GN_FP")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ]);
        assert!(
            out.is_none(),
            "a predecessor inside a feminine plural nominal group agrees"
        );
    }

    #[test]
    fn postponed_adj_double_loc_adverb_predecessor_passes_the_gate() {
        let out = run_concordance(vec![
            sent_start(),
            atr("problema", &[("problema", "NCMS000")]),
            atr("cerca", &[("cerca", "LOC_ADV")]),
            atr("arriba", &[("arriba", "LOC_ADV")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ])
        .expect("two locative adverbs must satisfy the predecessor gate");
        assert_eq!(replacements_of(out), vec!["blanco".to_string()]);
    }

    #[test]
    fn postponed_adj_determiner_predecessor_fails_the_gate() {
        let out = run_concordance(vec![
            sent_start(),
            atr("casa", &[("casa", "NCFS000")]),
            atr("la", &[("el", "DA0FS0")]),
            atr("blancas", &[("blanco", "AQ0FP0")]),
        ]);
        assert!(
            out.is_none(),
            "a determiner predecessor is not a valid context"
        );
    }

    #[test]
    fn postponed_adj_plurional_context_synthesizes_the_plural_across_levels() {
        let mut tokens = vec![sent_start()];
        for _ in 0..5 {
            tokens.push(atr("de", &[("de", "SPS00")]));
            tokens.push(atr("la", &[("el", "DA0FS0")]));
            tokens.push(atr("casa", &[("casa", "NCFS000")]));
        }
        tokens.push(atr("blancas", &[("blanco", "AQ0FP0")]));
        let out = run_concordance(tokens).expect("the clamped level walk must still suggest");
        assert_eq!(replacements_of(out), vec!["blanca".to_string()]);
    }

    #[test]
    fn postponed_adj_plural_flag_synthesizes_the_other_plural_gender() {
        let out = run_concordance(vec![
            sent_start(),
            atr("las", &[("el", "DA0FP0")]),
            atr("casas", &[("casa", "NCFP000")]),
            atr(",", &[(",", "_PUNCT")]),
            atr("muy", &[("muy", "RG_before")]),
            atr("rojos", &[("rojo", "AQ0MP0")]),
        ])
        .expect("the plural flag must synthesize the plural forms");
        assert_eq!(replacements_of(out), vec!["rojas".to_string()]);
        let out = run_concordance(vec![
            sent_start(),
            atr("las", &[("el", "DA0FP0")]),
            atr("casas", &[("casa", "NCFP000")]),
            atr(",", &[(",", "_PUNCT")]),
            atr("muy", &[("muy", "RG_before")]),
            atr("grandes", &[("grande", "AQ0MP0")]),
        ]);
        assert!(
            out.is_none(),
            "an invariable adjective resynthesizes only itself and is dropped"
        );
    }

    #[test]
    fn postponed_adj_plural_nouns_force_the_plural_of_a_singular_adjective() {
        let out = run_concordance(vec![
            sent_start(),
            atr("las", &[("el", "DA0FP0")]),
            atr("casas", &[("casa", "NCFP000")]),
            atr("roja", &[("rojo", "AQ0FS0")]),
        ])
        .expect("a feminine plural context must suggest the plural form");
        assert_eq!(replacements_of(out), vec!["rojas".to_string()]);
        let out = run_concordance(vec![
            sent_start(),
            atr("la", &[("el", "DA0FS0")]),
            atr("mesas", &[("mesa", "NCFP000")]),
            atr("y", &[("y", "CC")]),
            atr("casas", &[("casa", "NCFP000")]),
            atr("blanca", &[("blanco", "AQ0FS0")]),
        ])
        .expect("the coordinated plural nouns must outrank the singular article");
        assert_eq!(replacements_of(out), vec!["blancas".to_string()]);
    }

    #[test]
    fn postponed_adj_mnemonic_noun_outside_the_short_window_opens_the_plural() {
        let out = run_concordance(vec![
            sent_start(),
            atr("artista", &[("artista", "NCMN000")]),
            atr("casas", &[("casa", "NCFP000")]),
            atr("mesas", &[("mesa", "NCFP000")]),
            atr("sillas", &[("silla", "NCFP000")]),
            atr("rojo", &[("rojo", "AQ0MS0")]),
        ])
        .expect("the mnemonic noun keeps the masculine options open");
        assert_eq!(
            replacements_of(out),
            vec!["rojos".to_string(), "rojas".to_string()]
        );
    }

    #[test]
    fn postponed_adj_add_comma_shifts_offset_and_prefixes_token() {
        let out = run_postponed("Disculpe el retraso maestra.", "2");
        assert_eq!(out.len(), 1, "exactly one kept match: {out:?}");
        assert_eq!((out[0].0, out[0].1), (19, 8));
        assert_eq!(out[0].2.first().map(String::as_str), Some(", maestra"));
        assert!(
            out[0].2.iter().any(|s| s == " maestro"),
            "synthesized masculine form must follow: {out:?}"
        );
    }

    #[test]
    fn advanced_synthesizer_passthrough_arms_keep_or_drop_the_match() {
        let tokens: Vec<AnalyzedTokenReadings> = vec![
            sent_start(),
            atr("administración", &[("administración", "NCFS000")]),
            atr("lenta", &[("lento", "AQ0FS0")]),
            atr("y", &[("y", "CC")]),
            atr("corrupto", &[("corrupto", "AQ0MS0")]),
        ];
        let tokens: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let args = "lemmaFrom:4 lemmaSelect:A.* postagFrom:2 postagSelect:A.*";
        let matched_texts = vec!["corrupto".to_string()];

        let rm = base_rm("m", 0, 7, "{suggestion}");
        let out = advanced_synthesizer_filter(None, args, &matched_texts, &[4, 2], &tokens, rm);
        let out = out.expect("out-of-order positions keep the match");
        assert_eq!(out.replacements()[0].value(), "{suggestion}");

        let rm = base_rm("m", 0, 7, "{suggestion}");
        let out =
            advanced_synthesizer_filter(None, "lemmaFrom:4", &matched_texts, &[4], &tokens, rm);
        let out = out.expect("missing args keep the match untouched");
        assert_eq!(out.replacements()[0].value(), "{suggestion}");

        let rm = base_rm("m", 0, 7, "{suggestion}");
        let out = advanced_synthesizer_filter(
            None,
            "lemmaFrom:4 lemmaSelect:A.* postagFrom:2 postagSelect:A.* newLemma:_",
            &matched_texts,
            &[1, 4],
            &tokens,
            rm,
        );
        assert!(
            out.is_none(),
            "newLemma underscore suppresses the match: {out:?}"
        );
    }

    #[test]
    fn advanced_synthesizer_rewrites_the_suggestion_placeholder() {
        let synth = es_synth();
        let args = "lemmaFrom:4 lemmaSelect:A.* postagFrom:2 postagSelect:A.*";
        let matched_texts = vec!["corrupto".to_string()];

        let tokens: Vec<AnalyzedTokenReadings> = vec![
            sent_start(),
            atr("administración", &[("administración", "NCFS000")]),
            atr("lenta", &[("lento", "AQ0FS0")]),
            atr("y", &[("y", "CC")]),
            atr("corrupto", &[("corrupto", "AQ0MS0")]),
        ];
        let tokens: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let rm = base_rm("m", 0, 7, "{suggestion}");
        let out = advanced_synthesizer_filter(
            synth.as_deref(),
            args,
            &matched_texts,
            &[1, 4],
            &tokens,
            rm,
        )
        .expect("synthesized match kept");
        let values: Vec<String> = out
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        assert_eq!(values, vec!["corrupta".to_string()]);

        let tokens: Vec<AnalyzedTokenReadings> = vec![
            sent_start(),
            atr("administración", &[("administración", "NCFS000")]),
            atr("lenta", &[("lento", "AQ0FS0")]),
            atr("y", &[("y", "CC")]),
            atr("Corrupto", &[("corrupto", "AQ0MS0")]),
        ];
        let tokens: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let rm = base_rm("m", 0, 7, "{suggestion}");
        let out = advanced_synthesizer_filter(
            synth.as_deref(),
            args,
            &matched_texts,
            &[1, 4],
            &tokens,
            rm,
        )
        .expect("capitalized match kept");
        assert_eq!(out.replacements()[0].value(), "Corrupta");

        let tokens: Vec<AnalyzedTokenReadings> = vec![
            sent_start(),
            atr("administración", &[("administración", "NCFS000")]),
            atr("lenta", &[("lento", "AQ0FS0")]),
            atr("y", &[("y", "CC")]),
            atr("CORRUPTO", &[("corrupto", "AQ0MS0")]),
        ];
        let tokens: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let rm = base_rm("m", 0, 7, "{suggestion}");
        let out = advanced_synthesizer_filter(
            synth.as_deref(),
            args,
            &matched_texts,
            &[1, 4],
            &tokens,
            rm,
        )
        .expect("uppercase match kept");
        assert_eq!(out.replacements()[0].value(), "CORRUPTA");
    }

    #[test]
    fn find_suggestions_diacritics_mode_suppresses_matching_readings() {
        let sentence = "Yo publico eso.";
        let rm = base_rm("m", 3, 7, "x");
        let out = find_suggestions_filter(
            "wordFrom:inmarker desiredPostag:V.* Mode:diacritics",
            &[],
            &[],
            &[],
            sentence,
            rm,
        );
        assert!(
            out.is_none(),
            "verb reading present so the match is dropped"
        );
    }
}
