use super::replace::GaSimpleReplaceRule;
use super::utils::{eclipse, is_vowel, lenite};
use og_core::IssueType;
use std::collections::HashMap;
use std::sync::LazyLock;

const DATIVE_PLURALS_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/ga/src/main/resources/org/languagetool/rules/ga/dative-plurals.txt"
);

struct DativePluralsEntry {
    form: String,
    form_modern: Option<String>,
    replacement: String,
    equivalent: Option<String>,
}

impl DativePluralsEntry {
    fn standard(&self) -> &str {
        self.equivalent.as_deref().unwrap_or(&self.replacement)
    }
}

fn split_modern(field: &str) -> (String, Option<String>) {
    match field.split_once(':') {
        Some((base, modern)) => (base.to_string(), Some(modern.to_string())),
        None => (field.to_string(), None),
    }
}

fn load_entries(data: &str) -> Vec<DativePluralsEntry> {
    let mut out = Vec::new();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() != 4 {
            panic!("Incorrect number of fields: {line}");
        }
        let (form, form_modern) = split_modern(parts[0]);
        let (repl, equivalent) = split_modern(parts[3]);
        out.push(DativePluralsEntry {
            form,
            form_modern,
            replacement: repl,
            equivalent,
        });
    }
    out
}

fn insert_mutated(out: &mut HashMap<String, String>, form: &str, standard: &str) {
    out.insert(form.to_string(), standard.to_string());
    let lenited_form = lenite(form);
    let lenited_repl = lenite(standard);
    if lenited_form != form {
        out.insert(lenited_form, lenited_repl);
    }
    let eclipsed_form = eclipse(form);
    let eclipsed_repl = eclipse(standard);
    if eclipsed_form != form {
        out.insert(eclipsed_form, eclipsed_repl);
    }
    if form.chars().next().is_some_and(is_vowel) {
        out.insert(format!("h{form}"), format!("h{standard}"));
        out.insert(format!("h-{form}"), format!("h{standard}"));
    }
}

static SIMPLE_REPLACEMENTS: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let mut out = HashMap::new();
    for entry in load_entries(DATIVE_PLURALS_TXT) {
        let standard = entry.standard().to_string();
        insert_mutated(&mut out, &entry.form, &standard);
        if let Some(modern) = &entry.form_modern {
            insert_mutated(&mut out, modern, &standard);
        }
    }
    out
});

pub fn dative_plural_standard_replace_rule() -> GaSimpleReplaceRule {
    GaSimpleReplaceRule::new(
        "GA_DATIVE_PLURALS_STD",
        "Tuiseal tabharthach iolra",
        "Tabharthach iolra",
        "Is litriú réamhchaighdeánach (tabharthach iolra) é \"$match\"; an \"$suggestions\" a bhí i gceist agat?",
    )
    .with_issue_type(IssueType::Grammar)
    .with_map(
        SIMPLE_REPLACEMENTS
            .iter()
            .map(|(k, v)| (k.clone(), vec![v.clone()]))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::rule::Rule as _;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut pos = 0usize;
        for word in text.split_whitespace() {
            let start = pos + text[pos..].find(word).expect("word offset");
            let end = start + word.len();
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                word, start, end,
            )));
            pos = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    fn single_match(text: &str) -> og_core::RuleMatch {
        let matches = dative_plural_standard_replace_rule().match_sentence(&make_sentence(text));
        assert_eq!(matches.len(), 1);
        matches.into_iter().next().unwrap()
    }

    #[test]
    fn maps_the_prestandard_dative_plural_to_the_standard_form() {
        let text = "as na beachaibh sin.";
        let m = single_match(text);
        assert_eq!(m.offset(), text.find("beachaibh").unwrap());
        assert_eq!(m.replacements()[0].value(), "beacha");
        assert_eq!(
            m.message(),
            "Is litriú réamhchaighdeánach (tabharthach iolra) é \"beachaibh\"; an \"beacha\" a bhí i gceist agat?"
        );
        assert_eq!(m.rule().id(), "GA_DATIVE_PLURALS_STD");
    }

    #[test]
    fn lenited_and_eclipsed_variants_map_to_mutated_standards() {
        assert_eq!(
            single_match("as na bheachaibh sin.").replacements()[0].value(),
            "bheacha"
        );
        assert_eq!(
            single_match("as na mbeachaibh sin.").replacements()[0].value(),
            "mbeacha"
        );
    }

    #[test]
    fn vowel_initial_forms_gain_the_h_prefix() {
        assert_eq!(
            single_match("as na háiteachaibh sin.").replacements()[0].value(),
            "háiteanna"
        );
        assert_eq!(
            single_match("as na h-áiteachaibh sin.").replacements()[0].value(),
            "háiteanna"
        );
    }

    #[test]
    fn modern_spelling_variant_maps_to_the_same_standard() {
        assert_eq!(
            single_match("as na buaibh sin.").replacements()[0].value(),
            "ba"
        );
    }

    #[test]
    fn standard_dative_plural_is_not_flagged() {
        let rule = dative_plural_standard_replace_rule();
        assert_eq!(
            rule.match_sentence(&make_sentence("as na beacha sin."))
                .len(),
            0
        );
    }
}
