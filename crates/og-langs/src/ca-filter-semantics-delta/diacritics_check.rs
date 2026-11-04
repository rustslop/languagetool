use std::collections::HashMap;
use std::sync::{Arc, LazyLock, OnceLock};

use og_core::{AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};
use regex::Regex;

use super::token_scan::reading_and_tag_matching;

struct ConfusionPairs {
    entries: HashMap<String, Vec<(String, String)>>,
}

impl ConfusionPairs {
    fn shared() -> Option<Arc<ConfusionPairs>> {
        static CACHE: OnceLock<Option<Arc<ConfusionPairs>>> = OnceLock::new();
        CACHE
            .get_or_init(|| {
                let path = crate::engine::find_lt_resource_dir()?
                    .join("ca/src/main/resources/org/languagetool/rules/ca/confusion_pairs.txt");
                let data = std::fs::read_to_string(path).ok()?;
                let mut entries: HashMap<String, Vec<(String, String)>> = HashMap::new();
                for line in data.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let parts: Vec<&str> = line.split(';').collect();
                    if parts.len() != 3 {
                        continue;
                    }
                    entries
                        .entry(parts[0].to_string())
                        .or_default()
                        .push((parts[1].to_string(), parts[2].to_string()));
                }
                Some(Arc::new(ConfusionPairs { entries }))
            })
            .clone()
    }
}

static MS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^NC[MC][SN]000$|^A..[MC][SN].$|^V.P..SM.$").unwrap());
static FS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^NC[FC][SN]000$|^A..[FC][SN].$|^V.P..SF.$").unwrap());
static MP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^NC[MC][PN]000$|^A..[MC][PN].$|^V.P..PM.$").unwrap());
static FP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^NC[FC][PN]000$|^A..[FC][PN].$|^V.P..PF.$").unwrap());
static CP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^NC[MFC][PN]000$|^A..[MFC][PN].$|^V.P..P..$").unwrap());
static CS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^NC[MFC][SN]000$|^A..[MFC][SN].$|^V.P..S..$").unwrap());

static SEL_MS: LazyLock<Regex> = LazyLock::new(|| Regex::new("^([NAPD].+MS.*|V.P..SM.)$").unwrap());
static SEL_MP: LazyLock<Regex> = LazyLock::new(|| Regex::new("^([NAPD].+MP.*|V.P..PM.)$").unwrap());
static SEL_FS: LazyLock<Regex> = LazyLock::new(|| Regex::new("^([NAPD].+FS.*|V.P..SF.)$").unwrap());
static SEL_FP: LazyLock<Regex> = LazyLock::new(|| Regex::new("^([NAPD].+FP.*|V.P..PF.)$").unwrap());
static SEL_CP: LazyLock<Regex> = LazyLock::new(|| Regex::new("^([NAPD].+CP.*|V.P..P..)$").unwrap());
static SEL_CS: LazyLock<Regex> = LazyLock::new(|| Regex::new("^([NAPD].+CS.*|V.P..S..)$").unwrap());

fn matches_full(pattern: &str, value: &str) -> bool {
    Regex::new(&format!("^(?:{pattern})$")).is_ok_and(|re| re.is_match(value))
}

fn select_gender_number_pattern(atr: &AnalyzedTokenReadings) -> Option<&'static Regex> {
    let selectors: [&Regex; 6] = [&SEL_MS, &SEL_MP, &SEL_FS, &SEL_FP, &SEL_CP, &SEL_CS];
    let targets: [&'static Regex; 6] = [&MS, &MP, &FS, &FP, &CP, &CS];
    for (selector, target) in selectors.iter().zip(targets) {
        if reading_and_tag_matching(atr, selector).is_some() {
            return Some(target);
        }
    }
    None
}

fn has_diacritics(word: &str) -> bool {
    word.chars().any(|c| c.is_alphabetic() && !c.is_ascii())
}

fn is_all_uppercase(word: &str) -> bool {
    word.chars()
        .all(|c| !(c.is_alphabetic() && c.is_lowercase()))
}

fn is_capitalized_word(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => {
            chars.all(|c| !c.is_alphabetic() || c.is_lowercase())
        }
        _ => false,
    }
}

fn uppercase_first_char(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => word.to_string(),
    }
}

pub fn accept_rule_match(
    rm: &RuleMatch,
    args: &HashMap<&str, String>,
    tokens: &[&AnalyzedTokenReadings],
    matched_positions: &[usize],
) -> Option<RuleMatch> {
    let pairs = ConfusionPairs::shared()?;
    let postag = args.get("postag")?;
    let original_form = args.get("form")?;
    let is_all_upper = is_all_uppercase(original_form);
    let is_capitalized = is_capitalized_word(original_form);
    let form = original_form.to_lowercase();

    let gendernumber_from = args.get("gendernumberFrom");
    let mut desired: Option<&Regex> = None;
    if let Some(source) = gendernumber_from {
        let i = source.parse::<usize>().ok()?;
        if i < 1 || i > tokens.len() {
            return None;
        }
        let base = matched_positions.first().copied().unwrap_or(0);
        let idx = (base + i - 1).min(tokens.len() - 1);
        desired = select_gender_number_pattern(tokens[idx]);
    }

    let readings = pairs.entries.get(&form)?;
    if !readings.iter().any(|(_, tag)| matches_full(postag, tag)) {
        return None;
    }
    let mut replacement;
    if let Some(desired) = desired {
        let (_, first_tag) = readings.first()?;
        if !desired.is_match(first_tag) {
            return None;
        }
        replacement = readings[0].0.clone();
    } else if gendernumber_from.is_none() {
        replacement = readings[0].0.clone();
    } else {
        return None;
    }

    let mut message = rm.message().to_string();
    if !(has_diacritics(&replacement) && !has_diacritics(&form)) {
        message = message.replace("s'escriu amb accent", "s'escriu d'una altra manera");
    }
    if is_all_upper {
        replacement = replacement.to_uppercase();
    }
    if is_capitalized {
        replacement = uppercase_first_char(&replacement);
    }
    let mut out = rm.clone();
    out.set_message(message);
    let suggestions = rm
        .replacements()
        .iter()
        .map(|repl| {
            let value = repl
                .value()
                .replace("{suggestion}", &replacement)
                .replace("{Suggestion}", &uppercase_first_char(&replacement))
                .replace("{SUGGESTION}", &replacement.to_uppercase());
            SuggestedReplacement::new(value)
        })
        .collect();
    out.set_replacements(suggestions);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs() -> Arc<ConfusionPairs> {
        ConfusionPairs::shared().expect("confusion_pairs.txt in the vendored ca rules dir")
    }

    #[test]
    fn confusion_pairs_load() {
        let pairs = pairs();
        let acid = pairs.entries.get("acid").expect("acid entry");
        assert_eq!(acid[0], ("àcid".to_string(), "AQ0MS0".to_string()));
        assert_eq!(acid[1], ("àcid".to_string(), "NCMS000".to_string()));
        assert_eq!(
            pairs.entries.get("acida").map(|v| v.as_slice()),
            Some(&[("àcida".to_string(), "AQ0FS0".to_string())][..])
        );
    }

    #[test]
    fn helper_predicates_match_the_java_string_tools() {
        assert!(is_all_uppercase("ACID"));
        assert!(!is_all_uppercase("Acid"));
        assert!(is_capitalized_word("Acid"));
        assert!(!is_capitalized_word("ACID"));
        assert!(!is_capitalized_word("acid"));
        assert!(has_diacritics("àcid"));
        assert!(!has_diacritics("acid"));
    }
}
