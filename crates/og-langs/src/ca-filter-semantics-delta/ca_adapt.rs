use std::sync::LazyLock;

use regex::Regex;

use super::token_scan::{is_capitalized_word, preserve_case, uppercase_first_char};

static CA_CONTRACTIONS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b([Aa]|[DdPp]e)r? e(ls?)\b").unwrap());
static CA_CONTRACTIONS2: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b([Dd])[' ]+(els?)\b").unwrap());
static CA_APOSTROPHES1: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b([LDNSTMldnstm]['’]) ").unwrap());
static CA_APOSTROPHES2: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b([mtlsn])['’]([^1haeiouáàèéíòóúA-ZÀÈÉÍÒÓÚ“«"])"#).unwrap());
static CA_APOSTROPHES3: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\be?([mtsldn])e? (h[aeio]|h?[aeiouàèéíòóú][a-zàèéíòóúïüç])").unwrap()
});
static CA_APOSTROPHES4: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(l)a ([aeoàúèéí][^ ])").unwrap());
static CA_APOSTROPHES5: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b([mts]e) (['’])").unwrap());
static CA_APOSTROPHES6: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bs'e(ns|ls)\b").unwrap());
static CA_APOSTROPHES7: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(de|a)l (h?[aeoàúèéí][^ ])").unwrap());
static CA_APOSTROPHES8: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b([MTLSN])['’]([^1haeiouáàèéíòóúA-ZÀÈÉÍÒÓÚ“«"])"#).unwrap());
static CA_APOSTROPHES9: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b([Dd])['’]([^1haeiouáàèéíòóúA-ZÀÈÉÍÒÓÚ“«"])"#).unwrap());
static CA_REMOVE_SPACES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(a|de|pe) (ls?)\b").unwrap());

pub fn adapt_suggestion_ca(s: &str, original_error_str: &str) -> String {
    let capitalized = is_capitalized_word(s);
    let mut s = s
        .replace("gens traça", "gens de traça")
        .replace("gens facilitat", "gens de facilitat");
    s = CA_CONTRACTIONS.replace_all(&s, "${1}${2}").into_owned();
    s = CA_CONTRACTIONS2.replace_all(&s, "${1}${2}").into_owned();
    s = CA_APOSTROPHES1.replace_all(&s, "${1}").into_owned();
    s = CA_APOSTROPHES2.replace_all(&s, "e${1} ${2}").into_owned();
    if !s.contains("en el") && !s.contains("-se") {
        s = CA_APOSTROPHES3.replace_all(&s, "${1}'${2}").into_owned();
    }
    s = CA_APOSTROPHES4.replace_all(&s, "${1}'${2}").into_owned();
    s = CA_APOSTROPHES5.replace_all(&s, "${1}${2}").into_owned();
    s = CA_APOSTROPHES6.replace_all(&s, "se'${1}").into_owned();
    s = CA_APOSTROPHES7.replace_all(&s, "${1} l'${2}").into_owned();
    s = CA_APOSTROPHES8
        .replace_all(&s, |c: &regex::Captures| {
            format!("E{} {}", c[1].to_lowercase(), &c[2])
        })
        .into_owned();
    s = CA_APOSTROPHES9.replace_all(&s, "${1}e ${2}").into_owned();
    s = CA_REMOVE_SPACES
        .replace_all(&s, |c: &regex::Captures| {
            let whole = c.get(0).unwrap();
            if next_char_is_apostrophe(&s[whole.end()..]) {
                whole.as_str().to_string()
            } else {
                format!("{}{}", &c[1], &c[2])
            }
        })
        .into_owned();
    if capitalized {
        s = uppercase_first_char(&s);
    }
    s = s.replace(" ,", ",");
    preserve_case(&s, original_error_str)
}

fn next_char_is_apostrophe(rest: &str) -> bool {
    rest.chars().next().is_some_and(|c| c == '\'' || c == '’')
}
