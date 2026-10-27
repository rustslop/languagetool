use crate::postponed_adjective::LangSynthesizer;
use og_core::{AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};
use og_xml::compiler::CompiledRule;
use og_xml::types::XmlFilter;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub fn resolve_args<'a>(args: &'a str, matched_texts: &[String]) -> HashMap<&'a str, String> {
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

pub fn raw_arg<'a>(args: &'a str, key: &str) -> Option<&'a str> {
    args.split_whitespace()
        .find_map(|part| part.strip_prefix(key)?.strip_prefix(':'))
}

pub fn pattern_token_range<'a>(
    tokens: &'a [&'a AnalyzedTokenReadings],
    matched_positions: &[usize],
) -> &'a [&'a AnalyzedTokenReadings] {
    match (matched_positions.first(), matched_positions.last()) {
        (Some(&first), Some(&last)) if last < tokens.len() => &tokens[first..=last],
        _ => &[],
    }
}

pub fn is_all_uppercase(s: &str) -> bool {
    s.chars().all(|c| !c.is_alphabetic() || !c.is_lowercase())
}

pub fn is_capitalized_word(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => {
            chars.all(|c| !c.is_alphabetic() || c.is_lowercase())
        }
        _ => false,
    }
}

pub fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn has_diacritics(s: &str) -> bool {
    s.chars().any(|c| c.is_alphabetic() && !c.is_ascii())
}

pub fn full_match(pattern: &str, value: &str) -> bool {
    full_matcher(pattern).is_match(value)
}

pub fn full_matcher(pattern: &str) -> Regex {
    static CACHE: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap();
    regex::Regex::clone(
        guard
            .entry(pattern.to_string())
            .or_insert_with(|| Regex::new(&format!("^(?:{pattern})$")).unwrap()),
    )
}

pub fn readings_match_pos_tag_regex(atr: &AnalyzedTokenReadings, pos_tag_regex: &str) -> bool {
    atr.readings()
        .iter()
        .flat_map(|r| r.pos_tags().iter())
        .any(|t| full_match(pos_tag_regex, t))
}

pub fn lt_modules_root() -> Option<PathBuf> {
    [
        PathBuf::from("languagetool/languagetool-language-modules"),
        PathBuf::from("../languagetool/languagetool-language-modules"),
        PathBuf::from("../languagetool-language-modules"),
        PathBuf::from("../../languagetool-language-modules"),
        PathBuf::from("/home/agent/opengrammar-rs/languagetool/languagetool-language-modules"),
    ]
    .into_iter()
    .find(|p| p.exists())
    .and_then(|p| p.canonicalize().ok())
}

pub fn pt_rules_dir() -> Option<PathBuf> {
    lt_modules_root().map(|root| root.join("pt/src/main/resources/org/languagetool/rules/pt"))
}

pub fn pt_resource_dir() -> Option<PathBuf> {
    lt_modules_root().map(|root| root.join("pt/src/main/resources/org/languagetool/resource/pt"))
}

pub struct ConfusionEntry {
    pub word: String,
    pub tags: Vec<String>,
}

pub fn confusion_pairs(brazilian: bool) -> &'static HashMap<String, ConfusionEntry> {
    static PT: OnceLock<HashMap<String, ConfusionEntry>> = OnceLock::new();
    static BR: OnceLock<HashMap<String, ConfusionEntry>> = OnceLock::new();
    let load = |variant_file: &str| {
        let mut map: HashMap<String, ConfusionEntry> = HashMap::new();
        let Some(dir) = pt_rules_dir() else {
            return map;
        };
        for name in ["confusion_pairs.txt", variant_file] {
            let Ok(text) = std::fs::read_to_string(dir.join(name)) else {
                continue;
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
                match map.get_mut(parts[0]) {
                    Some(entry) => entry.tags.push(parts[2].to_string()),
                    None => {
                        map.insert(
                            parts[0].to_string(),
                            ConfusionEntry {
                                word: parts[1].to_string(),
                                tags: vec![parts[2].to_string()],
                            },
                        );
                    }
                }
            }
        }
        map
    };
    if brazilian {
        BR.get_or_init(|| load("pt-BR/confusion_pairs.txt"))
    } else {
        PT.get_or_init(|| load("pt-PT/confusion_pairs.txt"))
    }
}

pub fn confusion_check_filter(
    variant: &str,
    args: &str,
    matched_texts: &[String],
    pattern_tokens: &[&AnalyzedTokenReadings],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let parsed = resolve_args(args, matched_texts);
    let postag = parsed.get("postag")?;
    let form = parsed.get("form")?.to_lowercase();
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

    let relevant_words = confusion_pairs(variant == "pt-BR");
    let mut replacement: Option<&str> = None;
    if let Some(entry) = relevant_words.get(&form)
        && entry.tags.iter().any(|t| full_match(postag, t))
    {
        if let Some(desired) = desired {
            if entry.tags.first().is_some_and(|t| full_match(desired, t)) {
                replacement = Some(entry.word.as_str());
            }
        } else if gendernumber_from.is_none() {
            replacement = Some(entry.word.as_str());
        }
    }
    let replacement = replacement?.to_string();
    let mut message = rm.message().to_string();
    if !has_diacritics(&replacement) || has_diacritics(&form) {
        message = message.replace("se escribe con tilde", "se escribe de otra manera");
    }
    rm.set_message(message);
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

pub const BRAZILIAN_STATES: [&str; 27] = [
    "AC", "AL", "AP", "AM", "BA", "CE", "DF", "ES", "GO", "MA", "MT", "MS", "MG", "PA", "PB", "PR",
    "PE", "PI", "RJ", "RN", "RS", "RO", "RR", "SC", "SP", "SE", "TO",
];

pub fn toponyms() -> &'static HashSet<String> {
    static TOPONYMS: OnceLock<HashSet<String>> = OnceLock::new();
    TOPONYMS.get_or_init(|| {
        let mut set = HashSet::new();
        let Some(resource_dir) = pt_resource_dir() else {
            return set;
        };
        let dir = resource_dir.join("brazilian_municipalities");
        for state in BRAZILIAN_STATES {
            let Ok(text) = std::fs::read_to_string(dir.join(format!("{state}.tsv"))) else {
                continue;
            };
            for line in text.lines() {
                set.insert(line.replace('-', " ").to_lowercase());
            }
        }
        set
    })
}

pub fn is_valid_toponym(toponym: &str) -> bool {
    let normalised = toponym.replace('-', " ").to_lowercase();
    let parts: Vec<&str> = normalised.split(' ').collect();
    let all = toponyms();
    (0..parts.len()).any(|i| all.contains(&parts[i..].join(" ")))
}

pub fn brazilian_toponym_filter(groups: &[&str], mut rm: RuleMatch) -> Option<RuleMatch> {
    let toponym = groups.get(1).copied().unwrap_or_default();
    let underlined = groups.get(2).copied().unwrap_or_default();
    let state = groups.get(3).copied().unwrap_or_default();
    let suggestion = format!("\u{2013}{state}");
    if suggestion == underlined {
        return None;
    }
    if !is_valid_toponym(toponym) {
        return None;
    }
    rm.set_replacements(vec![SuggestedReplacement::new(suggestion)]);
    Some(rm)
}

pub fn is_regular_participle(p: &str) -> bool {
    let lp = p.to_lowercase();
    ["do", "dos", "da", "das"]
        .iter()
        .any(|end| lp.ends_with(end))
}

pub fn regular_irregular_participle_filter(
    synthesizer: Option<&LangSynthesizer>,
    args: &str,
    tokens: &[&AnalyzedTokenReadings],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let direction = raw_arg(args, "direction")?;
    let atr = tokens
        .iter()
        .copied()
        .find(|t| t.token().start() == rm.offset())?;
    if !atr
        .readings()
        .iter()
        .flat_map(|r| r.pos_tags().iter())
        .any(|t| t.starts_with("VMP"))
    {
        return None;
    }
    let mut selected: Option<&og_core::AnalyzedToken> = None;
    let mut desired_postag = String::new();
    for reading in atr.readings() {
        if let Some(tag) = reading.pos_tags().first()
            && tag.starts_with("VMP")
        {
            selected = Some(reading);
            desired_postag = tag.clone();
        }
    }
    let selected = selected?;
    let last = desired_postag.pop()?;
    if last == 'C' {
        desired_postag.push_str("[MC]");
    } else {
        desired_postag.push_str(&format!("[{last}C]"));
    }
    let synthesizer = synthesizer?;
    let lemma = selected.lemma().unwrap_or(selected.token());
    let participles = synthesizer.synthesize_full_for(lemma, None, &desired_postag);
    if participles.len() < 2 {
        return None;
    }
    let token_text = atr.token().token();
    let mut replacement: Option<String> = None;
    if direction.eq_ignore_ascii_case("RegularToIrregular") && is_regular_participle(token_text) {
        if !is_regular_participle(&participles[0]) {
            replacement = Some(participles[0].clone());
        } else if !is_regular_participle(&participles[1]) {
            replacement = Some(participles[1].clone());
        }
    } else if direction.eq_ignore_ascii_case("IrregularToRegular")
        && !is_regular_participle(token_text)
    {
        if is_regular_participle(&participles[0]) {
            replacement = Some(participles[0].clone());
        } else if is_regular_participle(&participles[1]) {
            replacement = Some(participles[1].clone());
        }
    }
    let replacement = replacement?;
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

pub fn soros_digit_shape(digit: u8, charset: [&str; 3]) -> String {
    let [c1, c2, c3] = charset;
    match digit {
        1 => c1.to_string(),
        2 => format!("{c1}{c1}"),
        3 => format!("{c1}{c1}{c1}"),
        4 => format!("{c1}{c2}"),
        5 => c2.to_string(),
        6 => format!("{c2}{c1}"),
        7 => format!("{c2}{c1}{c1}"),
        8 => format!("{c2}{c1}{c1}{c1}"),
        9 => format!("{c1}{c3}"),
        _ => String::new(),
    }
}

pub fn soros_overline(digits: &str) -> String {
    let mut out = String::new();
    for c in digits.chars() {
        out.push(c);
        out.push('\u{0305}');
    }
    out
}

pub fn soros_roman(input: &str) -> String {
    if input.is_empty() || !input.bytes().all(|b| b.is_ascii_digit()) {
        return String::new();
    }
    let digits = input.as_bytes();
    let Some(first) = digits.iter().position(|b| *b != b'0') else {
        return String::new();
    };
    run_digits(&digits[first..])
}

pub fn run_digits(digits: &[u8]) -> String {
    let n = digits.len();
    let d0 = digits[0] - b'0';
    match n {
        1 => soros_digit_shape(d0, ["I", "V", "X"]),
        2 => format!(
            "{}{}",
            soros_digit_shape(d0, ["X", "L", "C"]),
            run_digits(&digits[1..])
        ),
        3 => format!(
            "{}{}",
            soros_digit_shape(d0, ["C", "D", "M"]),
            run_digits(&digits[1..])
        ),
        4 if (1..=3).contains(&d0) => format!(
            "{}{}",
            soros_digit_shape(d0, ["M", "-", "-"]),
            run_digits(&digits[1..])
        ),
        7 if (1..=3).contains(&d0) => format!(
            "{}{}",
            soros_overline(&digits[..4].iter().map(|b| *b as char).collect::<String>()),
            run_digits(&digits[4..])
        ),
        _ => format!(
            "{}{}",
            soros_overline(
                &digits[..n - 3]
                    .iter()
                    .map(|b| *b as char)
                    .collect::<String>()
            ),
            run_digits(&digits[n - 3..])
        ),
    }
}

pub fn roman_numeral_filter(
    args: &str,
    matched_texts: &[String],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let parsed = resolve_args(args, matched_texts);
    let arabic_source = parsed.get("arabicSource").cloned().unwrap_or_default();
    rm.set_replacements(vec![SuggestedReplacement::new(soros_roman(&arabic_source))]);
    Some(rm)
}

pub fn convert_pronoun_to_accusative(pronoun_tag: &str) -> String {
    if let Some(prefix) = pronoun_tag.strip_suffix("N00") {
        format!("{prefix}A00")
    } else {
        pronoun_tag.to_string()
    }
}

pub fn pronoun_tags(
    pronoun_readings: &AnalyzedTokenReadings,
    verb_text: &str,
    convert_to_accusative: bool,
) -> Vec<String> {
    let mut tags = Vec::new();
    for reading in pronoun_readings.readings() {
        if reading.token() == "nos" {
            tags.push("PP1CPO00".to_string());
            if verb_text.ends_with('m') || verb_text.ends_with("ão") || verb_text.ends_with("õe")
            {
                tags.push("PP3MPA00".to_string());
            }
            break;
        }
        if let Some(tag) = reading.pos_tags().first()
            && tag.starts_with("PP")
        {
            let tag = if convert_to_accusative {
                convert_pronoun_to_accusative(tag)
            } else {
                tag.clone()
            };
            tags.push(tag);
        }
    }
    tags
}

pub fn portuguese_enclisis_filter(
    synthesizer: Option<&LangSynthesizer>,
    args: &str,
    tokens: &[&AnalyzedTokenReadings],
    matched_positions: &[usize],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let verb_pos = raw_arg(args, "verbPos").and_then(|v| v.parse::<usize>().ok())?;
    let pronoun_pos = raw_arg(args, "pronounPos").and_then(|v| v.parse::<usize>().ok())?;
    let convert_to_accusative =
        raw_arg(args, "convertToAccusative").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    let window = pattern_token_range(tokens, matched_positions);
    let verb = window.get(verb_pos).copied()?;
    let pronoun = window.get(pronoun_pos).copied()?;
    let tags = pronoun_tags(pronoun, verb.token().token(), convert_to_accusative);
    if tags.is_empty() {
        return None;
    }
    let mut suggestions: Vec<String> = Vec::new();
    if let Some(synthesizer) = synthesizer {
        let verb_text = verb.token().token();
        let title_case = is_capitalized_word(verb_text);
        let all_caps = is_all_uppercase(verb_text);
        for reading in verb.readings() {
            let Some(pos_tag) = reading.pos_tags().first() else {
                continue;
            };
            if !pos_tag.starts_with('V') {
                continue;
            }
            for pronoun_tag in &tags {
                let enclisis_tag = format!("{pos_tag}:{pronoun_tag}");
                let lemma = reading.lemma().unwrap_or(reading.token());
                for form in synthesizer.synthesize_exact_for(lemma, None, &enclisis_tag) {
                    let form = if title_case {
                        uppercase_first_char(&form)
                    } else if all_caps {
                        form.to_uppercase()
                    } else {
                        form
                    };
                    if !suggestions.contains(&form) {
                        suggestions.push(form);
                    }
                }
            }
            break;
        }
    }
    rm.set_replacements(
        suggestions
            .into_iter()
            .map(SuggestedReplacement::new)
            .collect(),
    );
    Some(rm)
}

pub fn proclisis_pronoun_forms(old_pronoun: &str, old_verb: &str) -> Vec<String> {
    match old_pronoun {
        "lo" | "no" => vec!["o".to_string()],
        "la" | "na" => vec!["a".to_string()],
        "los" => vec!["os".to_string()],
        "las" | "nas" => vec!["as".to_string()],
        "nos" => {
            let mut forms = vec!["nos".to_string()];
            if old_verb.ends_with('m') || old_verb.ends_with("ão") || old_verb.ends_with("õe") {
                forms.push("os".to_string());
            }
            forms
        }
        other => vec![other.to_string()],
    }
}

pub fn portuguese_proclisis_filter(
    synthesizer: Option<&LangSynthesizer>,
    tokens: &[&AnalyzedTokenReadings],
    matched_positions: &[usize],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let window = pattern_token_range(tokens, matched_positions);
    let enclitic = window.last().copied()?;
    let mut suggestions: Vec<String> = Vec::new();
    if let Some(synthesizer) = synthesizer {
        for reading in enclitic.readings() {
            let Some(pos_tag) = reading.pos_tags().first() else {
                continue;
            };
            if !pos_tag.starts_with('V') || !pos_tag.contains(':') {
                continue;
            }
            let verb_tag = pos_tag.split(':').next().unwrap_or_default();
            let lemma = reading.lemma().unwrap_or(reading.token());
            let Some(new_verb) = synthesizer
                .synthesize_exact_for(lemma, None, verb_tag)
                .into_iter()
                .next()
            else {
                continue;
            };
            let mut parts = reading.token().split('-');
            let (Some(old_verb), Some(old_pronoun)) = (parts.next(), parts.next()) else {
                continue;
            };
            for new_pronoun in proclisis_pronoun_forms(old_pronoun, old_verb) {
                let suggestion = format!("{new_pronoun} {new_verb}");
                if !suggestions.contains(&suggestion) {
                    suggestions.push(suggestion);
                }
            }
        }
    }
    rm.set_replacements(
        suggestions
            .into_iter()
            .map(SuggestedReplacement::new)
            .collect(),
    );
    Some(rm)
}

pub fn marker_or_index(
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

pub fn select_reading<'a>(
    atr: &'a AnalyzedTokenReadings,
    select: &str,
) -> (Option<&'a str>, &'a str) {
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

pub fn composite_postag(
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

pub fn move_pronoun_tag(source_tag: &str, destination_tag: &str) -> String {
    let mut parts: Vec<&str> = source_tag.split(':').collect();
    while parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    if parts.len() == 2 {
        format!(
            "{}:{}",
            destination_tag.split(':').next().unwrap_or_default(),
            parts[1]
        )
    } else {
        destination_tag.to_string()
    }
}

pub fn is_suggestion_exception_pt(token: &str, desired_postag: &str) -> bool {
    (desired_postag == "VMIP1P0" || desired_postag == "VMIP2P0") && !token.ends_with('s')
}

pub fn advanced_synthesizer_filter(
    synthesizer: Option<&LangSynthesizer>,
    args: &str,
    matched_texts: &[String],
    matched_positions: &[usize],
    tokens: &[&AnalyzedTokenReadings],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let pattern_tokens = pattern_token_range(tokens, matched_positions);
    if pattern_tokens.is_empty() {
        return Some(rm);
    }
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
    let desired_postag = match postag_replace.as_deref() {
        Some("keepPronoun") => move_pronoun_tag(&original_postag, &desired_postag),
        Some(replace) => composite_postag(
            lemma_select,
            postag_select,
            &original_postag,
            &desired_postag,
            replace,
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
            if is_suggestion_exception_pt(nr, &desired_postag) {
                continue;
            }
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
    rm.set_replacements(list.into_iter().map(SuggestedReplacement::new).collect());
    Some(rm)
}

pub fn is_pt_filter(class: &str) -> bool {
    matches!(
        class,
        "org.languagetool.rules.pt.ConfusionCheckFilter"
            | "org.languagetool.rules.pt.BrazilianToponymFilter"
            | "org.languagetool.rules.pt.RegularIrregularParticipleFilter"
            | "org.languagetool.rules.pt.RomanNumeralFilter"
            | "org.languagetool.rules.pt.PortugueseEnclisisFilter"
            | "org.languagetool.rules.pt.PortugueseProclisisFilter"
            | "org.languagetool.rules.pt.AdvancedSynthesizerFilter"
    )
}

pub struct PtFilterCall<'a> {
    pub variant: &'a str,
    pub args: &'a str,
    pub matched_texts: &'a [String],
    pub matched_positions: &'a [usize],
    pub tokens: &'a [&'a AnalyzedTokenReadings],
    pub regexp_groups: Option<&'a [&'a str]>,
    pub synthesizer: Option<&'a LangSynthesizer>,
}

pub fn apply_pt_filter(class: &str, call: PtFilterCall<'_>, rm: RuleMatch) -> Option<RuleMatch> {
    match class {
        "org.languagetool.rules.pt.ConfusionCheckFilter" => confusion_check_filter(
            call.variant,
            call.args,
            call.matched_texts,
            pattern_token_range(call.tokens, call.matched_positions),
            rm,
        ),
        "org.languagetool.rules.pt.BrazilianToponymFilter" => {
            brazilian_toponym_filter(call.regexp_groups?, rm)
        }
        "org.languagetool.rules.pt.RegularIrregularParticipleFilter" => {
            regular_irregular_participle_filter(call.synthesizer, call.args, call.tokens, rm)
        }
        "org.languagetool.rules.pt.RomanNumeralFilter" => {
            roman_numeral_filter(call.args, call.matched_texts, rm)
        }
        "org.languagetool.rules.pt.PortugueseEnclisisFilter" => portuguese_enclisis_filter(
            call.synthesizer,
            call.args,
            call.tokens,
            call.matched_positions,
            rm,
        ),
        "org.languagetool.rules.pt.PortugueseProclisisFilter" => {
            portuguese_proclisis_filter(call.synthesizer, call.tokens, call.matched_positions, rm)
        }
        "org.languagetool.rules.pt.AdvancedSynthesizerFilter" => advanced_synthesizer_filter(
            call.synthesizer,
            call.args,
            call.matched_texts,
            call.matched_positions,
            call.tokens,
            rm,
        ),
        _ => None,
    }
}

pub fn regexp_group_texts(
    regexp: &og_xml::compiler::CompiledRuleRegexp,
    text: &str,
    offset: usize,
    length: usize,
) -> Option<Vec<String>> {
    let end = offset + length;
    let captured: Vec<Vec<Option<(usize, usize)>>> = if let Some(ref re) = regexp.regex {
        re.captures_iter(text)
            .map(|caps| {
                (0..caps.len())
                    .map(|i| caps.get(i).map(|m| (m.start(), m.end())))
                    .collect()
            })
            .collect()
    } else if let Some(ref re) = regexp.fancy_regex {
        re.captures_iter(text)
            .flatten()
            .map(|caps| {
                (0..caps.len())
                    .map(|i| caps.get(i).map(|m| (m.start(), m.end())))
                    .collect()
            })
            .collect()
    } else {
        return None;
    };
    captured
        .into_iter()
        .find(|groups| {
            groups
                .get(regexp.mark)
                .and_then(|g| *g)
                .or_else(|| groups.first().and_then(|g| *g))
                .is_some_and(|(s, e)| s == offset && e == end)
        })
        .map(|groups| {
            groups
                .iter()
                .map(|g| g.map(|(s, e)| text[s..e].to_string()).unwrap_or_default())
                .collect()
        })
}

pub fn apply_engine_pt_filter(
    rule: &CompiledRule,
    filter: &XmlFilter,
    sentence_text: &str,
    rm: RuleMatch,
    call: PtFilterCall<'_>,
) -> Option<RuleMatch> {
    let group_texts = rule
        .regexp
        .as_ref()
        .and_then(|re| regexp_group_texts(re, sentence_text, rm.offset(), rm.length()));
    let group_refs: Vec<&str> = group_texts
        .iter()
        .flat_map(|v| v.iter().map(String::as_str))
        .collect();
    apply_engine_pt_filter_with_groups(filter, &group_refs, call, rm)
}

pub fn apply_engine_pt_filter_with_groups(
    filter: &XmlFilter,
    groups: &[&str],
    call: PtFilterCall<'_>,
    rm: RuleMatch,
) -> Option<RuleMatch> {
    let call = PtFilterCall {
        regexp_groups: (!groups.is_empty()).then_some(groups),
        ..call
    };
    apply_pt_filter(&filter.class, call, rm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedToken, RuleMatchContext, RuleMatchRule, SuggestedReplacement};
    use og_xml::compiler::{CompiledPattern, CompiledRuleRegexp};
    use std::io::Write;

    fn reading(token: &str, lemma: &str, postag: &str, start: usize) -> AnalyzedTokenReadings {
        AnalyzedTokenReadings::new(AnalyzedToken::new(token, start, start + token.len()))
            .with_readings(vec![
                AnalyzedToken::new(token, start, start + token.len())
                    .with_lemma(lemma)
                    .with_pos_tags(vec![postag.to_string()]),
            ])
    }

    fn match_at(offset: usize, length: usize, text: &str, template: &str) -> RuleMatch {
        let mut rm = RuleMatch::new(
            "msg",
            offset,
            length,
            RuleMatchRule::new("X", "d"),
            RuleMatchContext::new(text.to_string(), offset, length),
        );
        if !template.is_empty() {
            rm.set_replacements(vec![SuggestedReplacement::new(template)]);
        }
        rm
    }

    fn empty_call<'a>(tokens: &'a [&'a AnalyzedTokenReadings]) -> PtFilterCall<'a> {
        PtFilterCall {
            variant: "pt-PT",
            args: "",
            matched_texts: &[],
            matched_positions: &[],
            tokens,
            regexp_groups: None,
            synthesizer: None,
        }
    }

    fn temp_synth(entries: &str) -> LangSynthesizer {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("og-pt-synth-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = std::fs::File::create(dir.join("added.txt")).unwrap();
        f.write_all(entries.as_bytes()).unwrap();
        drop(f);
        LangSynthesizer::for_language(&dir, "pt").expect("temp pt synthesizer")
    }

    fn real_synth() -> Option<LangSynthesizer> {
        pt_resource_dir().and_then(|dir| LangSynthesizer::for_language(&dir, "pt"))
    }

    #[test]
    fn resolve_args_maps_backslash_indices_and_literals() {
        let matched = vec!["um".to_string(), "año".to_string()];
        let parsed = resolve_args("form:\\1 other:plain zero:\\0 oob:\\9", &matched);
        assert_eq!(parsed.get("form").map(String::as_str), Some("um"));
        assert_eq!(parsed.get("other").map(String::as_str), Some("plain"));
        assert_eq!(parsed.get("zero").map(String::as_str), Some("\\0"));
        assert_eq!(parsed.get("oob").map(String::as_str), Some("\\9"));
        let bare = resolve_args("nokey form:x", &[]);
        assert_eq!(bare.len(), 1);
        assert_eq!(bare.get("form").map(String::as_str), Some("x"));
    }

    #[test]
    fn raw_arg_finds_first_value_only() {
        assert_eq!(raw_arg("a:1 b:2 a:3", "a"), Some("1"));
        assert_eq!(raw_arg("a:1 b:2", "b"), Some("2"));
        assert_eq!(raw_arg("a:1", "c"), None);
    }

    #[test]
    fn pattern_token_range_slices_between_first_and_last_match() {
        let t1 = reading("a", "a", "A", 0);
        let t2 = reading("b", "b", "B", 2);
        let t3 = reading("c", "c", "C", 4);
        let tokens = [&t1, &t2, &t3];
        assert_eq!(pattern_token_range(&tokens, &[1, 2]).len(), 2);
        assert!(pattern_token_range(&tokens, &[]).is_empty());
        assert!(pattern_token_range(&tokens, &[5]).is_empty());
    }

    #[test]
    fn case_helpers_match_java_stringtools() {
        assert!(is_all_uppercase("ABC"));
        assert!(is_all_uppercase("ÁÉ"));
        assert!(!is_all_uppercase("AbC"));
        assert!(is_all_uppercase("123"));
        assert!(is_capitalized_word("Casa"));
        assert!(is_capitalized_word("Às"));
        assert!(!is_capitalized_word("casa"));
        assert!(!is_capitalized_word(""));
        assert!(!is_capitalized_word("CAsa"));
        assert_eq!(uppercase_first_char("casa"), "Casa");
        assert_eq!(uppercase_first_char("às"), "Às");
        assert_eq!(uppercase_first_char(""), "");
    }

    #[test]
    fn diacritics_and_full_match_are_anchored() {
        assert!(has_diacritics("ação"));
        assert!(!has_diacritics("acao"));
        assert!(!has_diacritics("123!"));
        assert!(full_match("NC.S000", "NCMS000"));
        assert!(!full_match("NC.S000", "xNCMS000x"));
        assert!(!full_match("NC.S000", "NCMS00"));
    }

    #[test]
    fn readings_match_pos_tag_regex_scans_all_readings() {
        let atr = AnalyzedTokenReadings::new(AnalyzedToken::new("x", 0, 1)).with_readings(vec![
            AnalyzedToken::new("x", 0, 1).with_pos_tags(vec!["DI0MS0".to_string()]),
            AnalyzedToken::new("x", 0, 1).with_pos_tags(vec!["NCMS000".to_string()]),
        ]);
        assert!(readings_match_pos_tag_regex(&atr, "NC.+"));
        assert!(!readings_match_pos_tag_regex(&atr, "V.+"));
    }

    #[test]
    fn confusion_pairs_load_variants_and_merge_duplicate_lines() {
        let pt = confusion_pairs(false);
        let br = confusion_pairs(true);
        assert!(!pt.is_empty());
        assert!(!br.is_empty());
        let entry = pt.get("abobora").expect("base-file entry");
        assert_eq!(entry.word, "abóbora");
        assert!(entry.tags.contains(&"NCFS000".to_string()));
        assert_eq!(pt.get("anonimo").unwrap().word, "anónimo");
        assert_eq!(br.get("anonimo").unwrap().word, "anônimo");
        let both = br.get("anonimo").expect("variant entry with two lines");
        assert_eq!(
            both.tags,
            vec!["AQ0MS0".to_string(), "NCMS000".to_string()],
            "repeated keys accumulate tags"
        );
    }

    #[test]
    fn confusion_check_filter_substitutes_the_confusion_word() {
        let owned = reading("abobora", "abobora", "NCFS000", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&owned];
        let matched = vec!["abobora".to_string()];
        let out = confusion_check_filter(
            "pt-PT",
            "postag:NCFS000 form:\\1",
            &matched,
            &tokens,
            match_at(0, 7, "abobora", "{suggestion} bonita"),
        )
        .expect("match kept");
        assert_eq!(out.replacements()[0].value(), "abóbora bonita");
        assert_eq!(out.message(), "msg");
    }

    #[test]
    fn confusion_check_filter_keeps_the_tilde_message_for_plain_accent_pairs() {
        let owned = reading("anonima", "anonima", "AQ0FS0", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&owned];
        let matched = vec!["anonima".to_string()];
        let mut rm = match_at(0, 7, "anonima", "{suggestion}");
        rm.set_message("se escribe con tilde");
        let out = confusion_check_filter("pt-PT", "postag:AQ0FS0 form:\\1", &matched, &tokens, rm)
            .unwrap();
        assert_eq!(out.replacements()[0].value(), "anónima");
        assert_eq!(out.message(), "se escribe con tilde");
    }

    #[test]
    fn confusion_check_filter_requires_postag_and_form_args() {
        let owned = reading("abobora", "abobora", "NCFS000", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&owned];
        let matched = vec!["abobora".to_string()];
        assert!(
            confusion_check_filter(
                "pt-PT",
                "postag:NCFS000",
                &matched,
                &tokens,
                match_at(0, 7, "abobora", "{suggestion}")
            )
            .is_none()
        );
        assert!(
            confusion_check_filter(
                "pt-PT",
                "postag:VMIP3S0 form:\\1",
                &matched,
                &tokens,
                match_at(0, 7, "abobora", "{suggestion}")
            )
            .is_none()
        );
    }

    #[test]
    fn confusion_check_filter_gender_number_agreement_gates_the_replacement() {
        let noun_fs = reading("casa", "casa", "NCFS000", 0);
        let matched = vec!["casa".to_string(), "abobora".to_string()];
        let args = "postag:NCFS000 form:\\2 gendernumberFrom:1";
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&noun_fs];
        let out = confusion_check_filter(
            "pt-PT",
            args,
            &matched,
            &tokens,
            match_at(0, 7, "abobora", "{suggestion}"),
        )
        .expect("FS agreement");
        assert_eq!(out.replacements()[0].value(), "abóbora");
        let adj_fs = reading("boa", "bom", "AQ0FS0", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&adj_fs];
        let out = confusion_check_filter(
            "pt-PT",
            args,
            &matched,
            &tokens,
            match_at(0, 7, "abobora", "{suggestion}"),
        )
        .expect("FS agreement via adjective");
        assert_eq!(out.replacements()[0].value(), "abóbora");
        let noun_mp = reading("bons", "bom", "AQ0MP0", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&noun_mp];
        assert!(
            confusion_check_filter(
                "pt-PT",
                args,
                &matched,
                &tokens,
                match_at(0, 7, "abobora", "{suggestion}")
            )
            .is_none()
        );
        assert!(
            confusion_check_filter(
                "pt-PT",
                "postag:NCFS000 form:\\1 gendernumberFrom:0",
                &matched,
                &tokens,
                match_at(0, 7, "abobora", "{suggestion}")
            )
            .is_none()
        );
    }

    #[test]
    fn toponym_lookup_uses_municipality_suffixes() {
        assert!(is_valid_toponym("São Paulo"));
        assert!(is_valid_toponym("são-paulo"));
        assert!(!is_valid_toponym("Zzzz Não Existe"));
    }

    #[test]
    fn brazilian_toponym_filter_matches_java_suggestion_gate() {
        let out = brazilian_toponym_filter(
            &["full", "São Paulo", "-SP", "SP"],
            match_at(0, 10, "São Paulo -SP", ""),
        )
        .expect("kept");
        assert_eq!(out.replacements()[0].value(), "–SP");
        assert!(
            brazilian_toponym_filter(
                &["full", "São Paulo", "–SP", "SP"],
                match_at(0, 10, "x", "")
            )
            .is_none()
        );
        assert!(
            brazilian_toponym_filter(
                &["full", "Zzz Não Existe", "-SP", "SP"],
                match_at(0, 10, "x", "")
            )
            .is_none()
        );
    }

    #[test]
    fn soros_digit_shape_covers_all_arabic_digits() {
        assert_eq!(soros_digit_shape(1, ["I", "V", "X"]), "I");
        assert_eq!(soros_digit_shape(2, ["I", "V", "X"]), "II");
        assert_eq!(soros_digit_shape(3, ["I", "V", "X"]), "III");
        assert_eq!(soros_digit_shape(4, ["I", "V", "X"]), "IV");
        assert_eq!(soros_digit_shape(5, ["I", "V", "X"]), "V");
        assert_eq!(soros_digit_shape(6, ["I", "V", "X"]), "VI");
        assert_eq!(soros_digit_shape(7, ["I", "V", "X"]), "VII");
        assert_eq!(soros_digit_shape(8, ["I", "V", "X"]), "VIII");
        assert_eq!(soros_digit_shape(9, ["I", "V", "X"]), "IX");
        assert_eq!(soros_digit_shape(0, ["I", "V", "X"]), "");
        assert_eq!(soros_digit_shape(4, ["M", "-", "-"]), "M-");
    }

    #[test]
    fn soros_overline_combines_per_character() {
        assert_eq!(soros_overline("4"), "4\u{0305}");
        assert_eq!(soros_overline("12"), "1\u{0305}2\u{0305}");
        assert_eq!(soros_overline(""), "");
    }

    #[test]
    fn soros_roman_converts_the_classical_range() {
        for (arabic, roman) in [
            ("1", "I"),
            ("2", "II"),
            ("3", "III"),
            ("4", "IV"),
            ("9", "IX"),
            ("14", "XIV"),
            ("40", "XL"),
            ("90", "XC"),
            ("249", "CCXLIX"),
            ("1999", "MCMXCIX"),
            ("3999", "MMMCMXCIX"),
        ] {
            assert_eq!(soros_roman(arabic), roman, "{arabic}");
        }
    }

    #[test]
    fn soros_roman_uses_overlines_beyond_3999_like_roman_sor() {
        assert_eq!(soros_roman("4000"), "4\u{0305}");
        assert_eq!(soros_roman("12345"), "1\u{0305}2\u{0305}CCCXLV");
        assert_eq!(
            soros_roman("1234567"),
            "1\u{0305}2\u{0305}3\u{0305}4\u{0305}DLXVII"
        );
        assert_eq!(soros_roman("0"), "");
        assert_eq!(soros_roman("007"), "VII");
        assert_eq!(soros_roman(""), "");
        assert_eq!(soros_roman("12a4"), "");
        assert_eq!(soros_roman("000"), "");
    }

    #[test]
    fn roman_numeral_filter_suggests_from_arabic_source() {
        let matched = vec!["xii".to_string(), "12".to_string()];
        let out = roman_numeral_filter("arabicSource:\\2", &matched, match_at(0, 3, "xii", ""))
            .expect("kept");
        assert_eq!(out.replacements()[0].value(), "XII");
        let out = roman_numeral_filter("", &[], match_at(0, 3, "xii", "")).expect("kept");
        assert_eq!(out.replacements()[0].value(), "");
    }

    #[test]
    fn pronoun_helpers_cover_accusative_conversion() {
        assert_eq!(convert_pronoun_to_accusative("PP1CPN00"), "PP1CPA00");
        assert_eq!(convert_pronoun_to_accusative("PP1CSO00"), "PP1CSO00");
        let nos = reading("nos", "nos", "PP1CPN00", 0);
        assert_eq!(pronoun_tags(&nos, "comemos", false), vec!["PP1CPO00"]);
        assert_eq!(
            pronoun_tags(&nos, "comeram", false),
            vec!["PP1CPO00", "PP3MPA00"]
        );
        assert_eq!(
            pronoun_tags(&nos, "põe", false),
            vec!["PP1CPO00", "PP3MPA00"]
        );
        let lo = reading("lo", "lo", "PP3MSN00", 0);
        assert_eq!(pronoun_tags(&lo, "comeu", true), vec!["PP3MSA00"]);
        let lhe = reading("lhe", "lhe", "PP3CSD0", 0);
        assert_eq!(pronoun_tags(&lhe, "x", false), vec!["PP3CSD0"]);
    }

    #[test]
    fn portuguese_enclisis_filter_synthesizes_contracted_forms() {
        let synth = temp_synth("come-o\tcomer\tVMIP3S0:PP3MSA00\n");
        let verb = reading("come", "comer", "VMIP3S0", 0);
        let pronoun = reading("o", "o", "PP3MSA00", 5);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb, &pronoun];
        let out = portuguese_enclisis_filter(
            Some(&synth),
            "verbPos:0 pronounPos:1 convertToAccusative:false",
            &tokens,
            &[0, 1],
            match_at(0, 6, "come o", ""),
        )
        .expect("kept");
        assert_eq!(out.replacements()[0].value(), "come-o");
    }

    #[test]
    fn portuguese_enclisis_filter_preserves_title_case_and_caps() {
        let synth = temp_synth("Como\tcomer\tVMIP1S0:PP1CPO00\nCOMO\tcomer\tVMIP1S0:PP1CPO00\n");
        let verb = reading("Como", "comer", "VMIP1S0", 0);
        let pronoun = reading("nos", "nos", "PP1CPN00", 5);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb, &pronoun];
        let out = portuguese_enclisis_filter(
            Some(&synth),
            "verbPos:0 pronounPos:1",
            &tokens,
            &[0, 1],
            match_at(0, 7, "Como nos", ""),
        )
        .unwrap();
        assert_eq!(out.replacements()[0].value(), "Como");
        let verb = reading("COMO", "comer", "VMIP1S0", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb, &pronoun];
        let out = portuguese_enclisis_filter(
            Some(&synth),
            "verbPos:0 pronounPos:1",
            &tokens,
            &[0, 1],
            match_at(0, 7, "COMO nos", ""),
        )
        .unwrap();
        assert_eq!(out.replacements()[0].value(), "COMO");
    }

    #[test]
    fn portuguese_enclisis_filter_rejects_pronounless_and_unargued_calls() {
        let verb = reading("come", "comer", "VMIP3S0", 0);
        let noun = reading("casa", "casa", "NCFS000", 5);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb, &noun];
        assert!(
            portuguese_enclisis_filter(
                None,
                "verbPos:0 pronounPos:1",
                &tokens,
                &[0, 1],
                match_at(0, 6, "come casa", "")
            )
            .is_none()
        );
        assert!(
            portuguese_enclisis_filter(
                None,
                "pronounPos:1",
                &tokens,
                &[0, 1],
                match_at(0, 6, "come casa", "")
            )
            .is_none()
        );
        assert!(
            portuguese_enclisis_filter(
                None,
                "verbPos:9 pronounPos:8",
                &tokens,
                &[0, 1],
                match_at(0, 6, "come casa", "")
            )
            .is_none()
        );
    }

    #[test]
    fn portuguese_enclisis_filter_without_synthesizer_clears_replacements() {
        let verb = reading("come", "comer", "VMIP3S0", 0);
        let pronoun = reading("o", "o", "PP3MSA00", 5);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb, &pronoun];
        let out = portuguese_enclisis_filter(
            None,
            "verbPos:0 pronounPos:1",
            &tokens,
            &[0, 1],
            match_at(0, 6, "come o", "{suggestion}"),
        )
        .expect("kept");
        assert!(out.replacements().is_empty());
    }

    #[test]
    fn portuguese_enclisis_filter_dedups_repeated_forms() {
        let synth =
            temp_synth("come-o\tcomer\tVMIP3S0:PP3MSA00\ncome-o\tcomer\tVMIP3S0:PP3MSA00\n");
        let verb = reading("come", "comer", "VMIP3S0", 0);
        let pronoun = reading("o", "o", "PP3MSA00", 5);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb, &pronoun];
        let out = portuguese_enclisis_filter(
            Some(&synth),
            "verbPos:0 pronounPos:1",
            &tokens,
            &[0, 1],
            match_at(0, 6, "come o", ""),
        )
        .expect("kept");
        assert_eq!(out.replacements().len(), 1);
    }

    #[test]
    fn proclisis_pronoun_forms_cover_the_java_switch() {
        assert_eq!(proclisis_pronoun_forms("lo", "x"), vec!["o"]);
        assert_eq!(proclisis_pronoun_forms("no", "x"), vec!["o"]);
        assert_eq!(proclisis_pronoun_forms("la", "x"), vec!["a"]);
        assert_eq!(proclisis_pronoun_forms("na", "x"), vec!["a"]);
        assert_eq!(proclisis_pronoun_forms("los", "x"), vec!["os"]);
        assert_eq!(proclisis_pronoun_forms("las", "x"), vec!["as"]);
        assert_eq!(proclisis_pronoun_forms("nas", "x"), vec!["as"]);
        assert_eq!(proclisis_pronoun_forms("nos", "x"), vec!["nos"]);
        assert_eq!(proclisis_pronoun_forms("nos", "comeram"), vec!["nos", "os"]);
        assert_eq!(proclisis_pronoun_forms("nos", "põe"), vec!["nos", "os"]);
        assert_eq!(proclisis_pronoun_forms("vos", "x"), vec!["vos"]);
    }

    #[test]
    fn portuguese_proclisis_filter_splits_the_enclitic_token() {
        let Some(synth) = real_synth() else {
            panic!("portuguese synthesizer must load");
        };
        let enclitic = reading("encontra-o", "encontrar", "VMIP3S0:PP3MSA00", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&enclitic];
        let out = portuguese_proclisis_filter(
            Some(&synth),
            &tokens,
            &[0],
            match_at(0, 10, "encontra-o", ""),
        )
        .expect("kept");
        let values: Vec<&str> = out.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(values, vec!["o encontra"]);
    }

    #[test]
    fn portuguese_proclisis_filter_skips_non_enclitic_readings() {
        let Some(synth) = real_synth() else {
            panic!("portuguese synthesizer must load");
        };
        let plain = reading("casa", "casa", "NCFS000", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&plain];
        let out = portuguese_proclisis_filter(
            Some(&synth),
            &tokens,
            &[0],
            match_at(0, 4, "casa", "{suggestion}"),
        )
        .expect("kept");
        assert!(out.replacements().is_empty());
        let hyphenless = reading("encontra", "encontrar", "VMIP3S0", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&hyphenless];
        let out = portuguese_proclisis_filter(
            Some(&synth),
            &tokens,
            &[0],
            match_at(0, 8, "encontra", ""),
        )
        .expect("kept");
        assert!(out.replacements().is_empty());
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&plain];
        let out = portuguese_proclisis_filter(None, &tokens, &[0], match_at(0, 4, "casa", ""))
            .expect("kept");
        assert!(out.replacements().is_empty());
    }

    #[test]
    fn marker_or_index_resolves_markers_and_bounded_indices() {
        let t0 = reading("a", "a", "A", 0);
        let t1 = reading("b", "b", "B", 2);
        let tokens = [&t0, &t1];
        assert_eq!(marker_or_index("1", &tokens, 0), Some(0));
        assert_eq!(marker_or_index("2", &tokens, 0), Some(1));
        assert_eq!(marker_or_index("0", &tokens, 0), None);
        assert_eq!(marker_or_index("3", &tokens, 0), None);
        assert_eq!(marker_or_index("x", &tokens, 0), None);
        assert_eq!(marker_or_index("marker", &tokens, 0), Some(0));
        assert_eq!(marker_or_index("marker1", &tokens, 0), Some(1));
        assert_eq!(marker_or_index("marker2", &tokens, 0), None);
        let t2 = reading("c", "c", "C", 4);
        let tokens = [&t0, &t1, &t2];
        assert_eq!(marker_or_index("marker", &tokens, 2), Some(1));
    }

    #[test]
    fn select_reading_prefers_matching_then_first_reading() {
        let atr = AnalyzedTokenReadings::new(AnalyzedToken::new("x", 0, 1)).with_readings(vec![
            AnalyzedToken::new("x", 0, 1)
                .with_lemma("l1")
                .with_pos_tags(vec!["AQ0MS0".to_string()]),
            AnalyzedToken::new("x", 0, 1)
                .with_lemma("l2")
                .with_pos_tags(vec!["NCMS000".to_string()]),
        ]);
        let (lemma, tag) = select_reading(&atr, "NC.+");
        assert_eq!(lemma, Some("l2"));
        assert_eq!(tag, "NCMS000");
        let (lemma, tag) = select_reading(&atr, "V.+");
        assert_eq!(lemma, Some("l1"));
        assert_eq!(tag, "AQ0MS0");
        let empty = AnalyzedTokenReadings::new(AnalyzedToken::new("x", 0, 1));
        let (lemma, tag) = select_reading(&empty, "NC.+");
        assert_eq!(lemma, None);
        assert_eq!(tag, "UNKNOWN");
    }

    #[test]
    fn composite_postag_splices_capture_groups() {
        let out = composite_postag(
            "(VMIP)(1)S0",
            "(NC)(F)S000",
            "VMIP1S0",
            "NCFS000",
            r"\a2-\b2",
        );
        assert_eq!(out, "1-F");
        assert_eq!(composite_postag("A", "B", "x", "y", "plain"), "plain");
    }

    #[test]
    fn move_pronoun_tag_keeps_the_pronoun_suffix() {
        assert_eq!(
            move_pronoun_tag("VMIP1S0:PP1CPO00", "VMIP1S0:PP1CSN00"),
            "VMIP1S0:PP1CPO00"
        );
        assert_eq!(
            move_pronoun_tag("VMIP1S0", "VMIP1S0:PP1CSN00"),
            "VMIP1S0:PP1CSN00"
        );
        assert_eq!(
            move_pronoun_tag("VMIP1S0:", "VMIP1S0:PP1CSN00"),
            "VMIP1S0:PP1CSN00"
        );
    }

    #[test]
    fn is_suggestion_exception_pt_gates_first_person_plurals() {
        assert!(is_suggestion_exception_pt("canta", "VMIP1P0"));
        assert!(!is_suggestion_exception_pt("cantamos", "VMIP1P0"));
        assert!(is_suggestion_exception_pt("canta", "VMIP2P0"));
        assert!(!is_suggestion_exception_pt("canta", "VMIP3S0"));
    }

    #[test]
    fn advanced_synthesizer_filter_agrees_the_adjective_with_the_noun() {
        let Some(synth) = real_synth() else {
            panic!("portuguese synthesizer must load");
        };
        let adj = reading("bonito", "bonito", "AQ0MS0", 0);
        let noun = reading("casa", "casa", "NCFS000", 7);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&adj, &noun];
        let matched = vec!["bonito".to_string(), "casa".to_string()];
        let args = "postagSelect:NC(F)S000 lemmaSelect:AQ0MS0 postagFrom:2 lemmaFrom:1 postagReplace:AQ0\\b1S0";
        let out = advanced_synthesizer_filter(
            Some(&synth),
            args,
            &matched,
            &[0, 1],
            &tokens,
            match_at(0, 6, "bonito casa", "{suggestion}"),
        )
        .expect("kept");
        let values: Vec<&str> = out.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(values, vec!["bonita"]);
    }

    #[test]
    fn advanced_synthesizer_filter_bails_out_on_missing_arguments_or_windows() {
        let adj = reading("bonito", "bonito", "AQ0MS0", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&adj];
        let matched = vec!["bonito".to_string()];
        let out = advanced_synthesizer_filter(
            None,
            "postagSelect:AQ0FS0",
            &matched,
            &[],
            &tokens,
            match_at(0, 6, "bonito", "{suggestion}"),
        )
        .unwrap();
        assert_eq!(out.replacements()[0].value(), "{suggestion}");
        assert!(
            advanced_synthesizer_filter(
                None,
                "postagSelect:AQ0FS0 lemmaSelect:AQ0MS0 lemmaFrom:9 postagFrom:1",
                &matched,
                &[0],
                &tokens,
                match_at(0, 6, "bonito", "{suggestion}")
            )
            .is_none()
        );
        assert!(
            advanced_synthesizer_filter(
                None,
                "postagSelect:AQ0FS0 lemmaSelect:AQ0MS0 lemmaFrom:1 postagFrom:9",
                &matched,
                &[0],
                &tokens,
                match_at(0, 6, "bonito", "{suggestion}")
            )
            .is_none()
        );
    }

    #[test]
    fn advanced_synthesizer_filter_replaces_the_lemma() {
        let synth = temp_synth("má\tmau\tAQ0FS0\n");
        let bom = reading("bom", "bom", "AQ0MS0", 0);
        let boa = reading("boa", "bom", "AQ0FS0", 4);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&bom, &boa];
        let matched = vec!["bom".to_string(), "boa".to_string()];
        let args = "postagSelect:AQ0FS0 lemmaSelect:AQ0MS0 postagFrom:2 lemmaFrom:1 newLemma:mau";
        let out = advanced_synthesizer_filter(
            Some(&synth),
            args,
            &matched,
            &[0, 1],
            &tokens,
            match_at(0, 3, "bom boa", "{Suggestion}!"),
        )
        .expect("kept");
        assert_eq!(out.replacements()[0].value(), "Má!");
    }

    #[test]
    fn advanced_synthesizer_filter_keep_pronoun_moves_the_pronoun_tag() {
        let synth = temp_synth("como-o\tcomer\tVMIP1S0:PP3MSA00\n");
        let enclisis = reading("como-o", "comer", "VMIP1S0:PP3MSA00", 0);
        let plain = reading("como", "comer", "VMIP1S0", 7);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&enclisis, &plain];
        let matched = vec!["como-o".to_string(), "como".to_string()];
        let args = "postagSelect:VMIP1S0 lemmaSelect:VMIP1S0:PP3MSA00 postagFrom:2 lemmaFrom:1 postagReplace:keepPronoun";
        let out = advanced_synthesizer_filter(
            Some(&synth),
            args,
            &matched,
            &[0, 1],
            &tokens,
            match_at(0, 6, "como-o como", "{suggestion}"),
        )
        .expect("kept");
        assert_eq!(out.replacements()[0].value(), "como-o");
    }

    #[test]
    fn advanced_synthesizer_filter_underscore_new_lemma_suppresses_synthesis() {
        let synth = temp_synth("forma\tnunca\tAQ0FS0\n");
        let tok = reading("x", "x", "AQ0MS0", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&tok];
        let matched = vec!["x".to_string()];
        let args = "postagSelect:AQ0MS0 lemmaSelect:AQ0MS0 postagFrom:1 lemmaFrom:1 newLemma:_ postagReplace:AQ0FS0";
        assert!(
            advanced_synthesizer_filter(
                Some(&synth),
                args,
                &matched,
                &[0],
                &tokens,
                match_at(0, 1, "x", "")
            )
            .is_none()
        );
    }

    #[test]
    fn advanced_synthesizer_filter_caps_the_suggestion_like_the_surface() {
        let synth = temp_synth("Boa\tbom\tAQ0FS0\n");
        let adj = reading("Boa", "bom", "AQ0FS0", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&adj];
        let matched = vec!["Boa".to_string()];
        let args = "postagSelect:AQ0FS0 lemmaSelect:AQ0FS0 postagFrom:1 lemmaFrom:1";
        let out = advanced_synthesizer_filter(
            Some(&synth),
            args,
            &matched,
            &[0],
            &tokens,
            match_at(0, 3, "Boa", "{suggestion}"),
        )
        .expect("kept");
        assert_eq!(out.replacements()[0].value(), "Boa");
    }

    #[test]
    fn advanced_synthesizer_filter_appends_bare_forms_without_template() {
        let synth = temp_synth("boa\tbom\tAQ0FS0\n");
        let adj = reading("bonito", "bom", "AQ0MS0", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&adj];
        let matched = vec!["bonito".to_string()];
        let args =
            "postagSelect:AQ0FS0 lemmaSelect:AQ0MS0 postagFrom:1 lemmaFrom:1 postagReplace:AQ0FS0";
        let out = advanced_synthesizer_filter(
            Some(&synth),
            args,
            &matched,
            &[0],
            &tokens,
            match_at(0, 6, "bonito", ""),
        )
        .expect("kept");
        let values: Vec<&str> = out.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(values, vec!["boa"]);
    }

    #[test]
    fn advanced_synthesizer_filter_skipped_exceptions_fall_back_to_raw_forms() {
        let synth = temp_synth("canta\tcantar\tVMIP1P0\n");
        let verb = reading("x", "cantar", "VMIP1S0", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb];
        let matched = vec!["x".to_string()];
        let args = "postagSelect:VMIP1S0 lemmaSelect:VMIP1S0 postagFrom:1 lemmaFrom:1 postagReplace:VMIP1P0";
        let out = advanced_synthesizer_filter(
            Some(&synth),
            args,
            &matched,
            &[0],
            &tokens,
            match_at(0, 1, "x", "{suggestion}"),
        )
        .expect("kept");
        let values: Vec<&str> = out.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(values, vec!["canta"]);
    }

    #[test]
    fn is_pt_filter_and_dispatch_route_every_class() {
        for class in [
            "org.languagetool.rules.pt.ConfusionCheckFilter",
            "org.languagetool.rules.pt.BrazilianToponymFilter",
            "org.languagetool.rules.pt.RegularIrregularParticipleFilter",
            "org.languagetool.rules.pt.RomanNumeralFilter",
            "org.languagetool.rules.pt.PortugueseEnclisisFilter",
            "org.languagetool.rules.pt.PortugueseProclisisFilter",
            "org.languagetool.rules.pt.AdvancedSynthesizerFilter",
        ] {
            assert!(is_pt_filter(class), "{class}");
        }
        assert!(!is_pt_filter("org.languagetool.rules.pt.FutureDateFilter"));
        let tokens: Vec<&AnalyzedTokenReadings> = vec![];
        assert!(
            apply_pt_filter(
                "no.such.Class",
                empty_call(&tokens),
                match_at(0, 1, "x", "")
            )
            .is_none()
        );
        assert!(
            apply_pt_filter(
                "org.languagetool.rules.pt.BrazilianToponymFilter",
                empty_call(&tokens),
                match_at(0, 1, "x", "")
            )
            .is_none()
        );
        let groups = ["full", "São Paulo", "-SP", "SP"];
        let out = apply_engine_pt_filter_with_groups(
            &XmlFilter {
                class: "org.languagetool.rules.pt.BrazilianToponymFilter".to_string(),
                args: String::new(),
            },
            &groups,
            empty_call(&tokens),
            match_at(0, 10, "São Paulo -SP", ""),
        )
        .expect("kept");
        assert_eq!(out.replacements()[0].value(), "–SP");
        let empty_groups: [&str; 0] = [];
        assert!(
            apply_engine_pt_filter_with_groups(
                &XmlFilter {
                    class: "org.languagetool.rules.pt.BrazilianToponymFilter".to_string(),
                    args: String::new(),
                },
                &empty_groups,
                empty_call(&tokens),
                match_at(0, 10, "São Paulo -SP", ""),
            )
            .is_none()
        );
    }

    #[test]
    fn regexp_group_texts_selects_the_group_matching_the_span() {
        let plain = CompiledRuleRegexp {
            regex: Some(std::sync::Arc::new(Regex::new(r"(\w+) (\w+)").unwrap())),
            fancy_regex: None,
            mark: 0,
        };
        assert_eq!(
            regexp_group_texts(&plain, "alpha beta", 0, 10),
            Some(vec![
                "alpha beta".to_string(),
                "alpha".to_string(),
                "beta".to_string()
            ])
        );
        let fancy = CompiledRuleRegexp {
            regex: None,
            fancy_regex: Some(std::sync::Arc::new(
                fancy_regex::Regex::new(r"(\w+) (\w+)").unwrap(),
            )),
            mark: 0,
        };
        assert_eq!(
            regexp_group_texts(&fancy, "alpha beta", 0, 10),
            Some(vec![
                "alpha beta".to_string(),
                "alpha".to_string(),
                "beta".to_string()
            ])
        );
        let neither = CompiledRuleRegexp {
            regex: None,
            fancy_regex: None,
            mark: 0,
        };
        assert_eq!(regexp_group_texts(&neither, "alpha beta", 0, 5), None);
        let marked = CompiledRuleRegexp {
            regex: Some(std::sync::Arc::new(Regex::new("(a)(lpha)").unwrap())),
            fancy_regex: None,
            mark: 1,
        };
        assert_eq!(
            regexp_group_texts(&marked, "alpha", 0, 1),
            Some(vec![
                "alpha".to_string(),
                "a".to_string(),
                "lpha".to_string()
            ])
        );
        assert_eq!(regexp_group_texts(&marked, "alpha", 1, 5), None);
    }

    #[test]
    fn apply_engine_pt_filter_extracts_groups_from_the_rule_regexp() {
        let mut rule = CompiledRule {
            id: "X".to_string(),
            sub_id: None,
            name: "X".to_string(),
            description: String::new(),
            source_file: None,
            category: Default::default(),
            antipatterns: Vec::new(),
            pattern: CompiledPattern {
                tokens: Vec::new(),
                elements: Vec::new(),
                case_sensitive: false,
                marker_start: None,
                marker_end: None,
                marker_elem_start: None,
                marker_elem_end: None,
                has_structured_elements: false,
                unify_groups: Vec::new(),
                raw_pos: false,
            },
            regexp: Some(CompiledRuleRegexp {
                regex: Some(std::sync::Arc::new(
                    Regex::new("(.*?)(–|-)([A-Z]{2})").unwrap(),
                )),
                fancy_regex: None,
                mark: 0,
            }),
            message: String::new(),
            short_message: None,
            suggestions: Vec::new(),
            examples: Vec::new(),
            unifications: Vec::new(),
            url: None,
            default_on: true,
            deprecated: false,
            filter: None,
            issue_type: String::new(),
            suppress_misspelled: false,
            picky: false,
        };
        let filter = XmlFilter {
            class: "org.languagetool.rules.pt.BrazilianToponymFilter".to_string(),
            args: String::new(),
        };
        let tokens: Vec<&AnalyzedTokenReadings> = vec![];
        let out = apply_engine_pt_filter(
            &rule,
            &filter,
            "São Paulo -SP",
            match_at(0, 10, "São Paulo -SP", ""),
            empty_call(&tokens),
        );
        assert!(out.is_none());
        rule.regexp = None;
        let out = apply_engine_pt_filter(
            &rule,
            &filter,
            "São Paulo -SP",
            match_at(0, 10, "São Paulo -SP", ""),
            empty_call(&tokens),
        );
        assert!(out.is_none());
    }

    #[test]
    fn regular_irregular_participle_filter_flips_directions() {
        let Some(synth) = real_synth() else {
            panic!("portuguese synthesizer must load");
        };
        let verb = reading("matada", "matar", "VMP00SF", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb];
        let out = regular_irregular_participle_filter(
            Some(&synth),
            "direction:RegularToIrregular",
            &tokens,
            match_at(0, 6, "matada", "{suggestion}"),
        )
        .expect("kept");
        assert_eq!(out.replacements()[0].value(), "morta");
        let verb = reading("morta", "matar", "VMP00SF", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb];
        let out = regular_irregular_participle_filter(
            Some(&synth),
            "direction:IrregularToRegular",
            &tokens,
            match_at(0, 5, "morta", "{suggestion}"),
        )
        .expect("kept");
        assert_eq!(out.replacements()[0].value(), "matada");
    }

    #[test]
    fn regular_irregular_participle_filter_rejects_mismatches() {
        let Some(synth) = real_synth() else {
            panic!("portuguese synthesizer must load");
        };
        let noun = reading("casa", "casa", "NCFS000", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&noun];
        assert!(
            regular_irregular_participle_filter(
                Some(&synth),
                "direction:RegularToIrregular",
                &tokens,
                match_at(0, 4, "casa", "")
            )
            .is_none()
        );
        let verb = reading("matada", "matar", "VMP00SF", 4);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb];
        assert!(
            regular_irregular_participle_filter(
                Some(&synth),
                "direction:RegularToIrregular",
                &tokens,
                match_at(0, 6, "x matada", "")
            )
            .is_none()
        );
        assert!(
            regular_irregular_participle_filter(
                Some(&synth),
                "direction:Sideways",
                &tokens,
                match_at(4, 6, "x matada", "")
            )
            .is_none()
        );
        assert!(
            regular_irregular_participle_filter(
                None,
                "direction:RegularToIrregular",
                &tokens,
                match_at(4, 6, "x matada", "")
            )
            .is_none()
        );
        let verb = reading("matada", "matar", "VMP00SF", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb];
        assert!(
            regular_irregular_participle_filter(
                Some(&synth),
                "direction:IrregularToRegular",
                &tokens,
                match_at(0, 6, "matada", "")
            )
            .is_none()
        );
        let verb = reading("morta", "matar", "VMP00SF", 0);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb];
        assert!(
            regular_irregular_participle_filter(
                Some(&synth),
                "direction:RegularToIrregular",
                &tokens,
                match_at(0, 5, "morta", "")
            )
            .is_none()
        );
    }

    #[test]
    fn lt_modules_root_resolves_the_checkout() {
        assert!(lt_modules_root().is_some());
        assert!(pt_rules_dir().is_some());
        assert!(pt_resource_dir().is_some());
    }
}
