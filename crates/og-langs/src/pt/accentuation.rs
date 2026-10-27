use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

static PREPOSICAO_DE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("de|d[a|o]s?").expect("valid regex"));
static ARTIGO_O_MS: LazyLock<Regex> = LazyLock::new(|| Regex::new("o|O").expect("valid regex"));
static ARTIGO_O_FS: LazyLock<Regex> = LazyLock::new(|| Regex::new("a|A").expect("valid regex"));
static ARTIGO_O_MP: LazyLock<Regex> = LazyLock::new(|| Regex::new("as|As").expect("valid regex"));
static ARTIGO_O_FP: LazyLock<Regex> = LazyLock::new(|| Regex::new("os|Os").expect("valid regex"));
static DETERMINANTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("D[^R].*").expect("valid regex"));
static DETERMINANTE_MS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("D[^R].[MC][SN].*").expect("valid regex"));
static DETERMINANTE_FS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("D[^R].[FC][SN].*").expect("valid regex"));
static DETERMINANTE_MP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("D[^R].[MC][PN].*").expect("valid regex"));
static DETERMINANTE_FP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("D[^R].[FC][PN].*").expect("valid regex"));
static NOME_MS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("NC[MC][SN].*").expect("valid regex"));
static NOME_FS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("NC[FC][SN].*").expect("valid regex"));
static NOME_MP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("NC[MC][PN].*").expect("valid regex"));
static NOME_FP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("NC[FC][PN].*").expect("valid regex"));
static ADJETIVO_MS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("A..[MC][SN].*|V.P..SM.?|PX.MS.*").expect("valid regex"));
static ADJETIVO_FS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("A..[FC][SN].*|V.P..SF.?|PX.FS.*").expect("valid regex"));
static ADJETIVO_MP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("A..[MC][PN].*|V.P..PM.?|PX.MP.*").expect("valid regex"));
static ADJETIVO_FP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("A..[FC][PN].*|V.P..PF.?|PX.FP.*").expect("valid regex"));
static INFINITIVO: LazyLock<Regex> = LazyLock::new(|| Regex::new("V.N.*").expect("valid regex"));
static VERBO_CONJUGADO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("V.[^NGP].*|_GV_").expect("valid regex"));
static PARTICIPIO_MS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("V.P.*SM.?").expect("valid regex"));
static GRUPO_VERBAL: LazyLock<Regex> = LazyLock::new(|| Regex::new("_GV_").expect("valid regex"));
static VERBO_3S: LazyLock<Regex> = LazyLock::new(|| Regex::new("V...3S..?").expect("valid regex"));
static NOT_IN_PREV_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("V..*|PP.*|P0.*|V.P.*").expect("valid regex"));
static BEFORE_ADJECTIVE_MS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("SPS00|D[^R].[MC][SN].*|V.[^NGP].*|PX.*").expect("valid regex"));
static BEFORE_ADJECTIVE_FS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("SPS00|D[^R].[FC][SN].*|V.[^NGP].*|PX.*").expect("valid regex"));
static BEFORE_ADJECTIVE_MP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("SPS00|D[^R].[MC][PN].*|V.[^NGP].*|PX.*").expect("valid regex"));
static BEFORE_ADJECTIVE_FP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("SPS00|D[^R].[FC][PN].*|V.[^NGP].*|PX.*").expect("valid regex"));
static GN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(".*_GN_.*|<?/?N[CP].*").expect("valid regex"));
static EXCEPCOES_ANTES_DE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?i)forma|maneira|por|costat").expect("valid regex"));
static PRONOME_PESSOAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new("P0.{6}|PP3CN000|PP3NN000|PP3CP000|PP3CSD00").expect("valid regex")
});

pub struct PortugueseAccentuationCheckRule {
    nouns: HashMap<String, (String, String)>,
    adjectives: HashMap<String, (String, String)>,
}

fn load_words(data: &str) -> HashMap<String, (String, String)> {
    let mut map = HashMap::new();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() != 3 {
            continue;
        }
        map.insert(
            parts[0].to_string(),
            (parts[1].to_string(), parts[2].to_string()),
        );
    }
    map
}

fn matches_postag(atr: &AnalyzedTokenReadings, pattern: &Regex) -> bool {
    atr.readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| pattern.is_match(t)))
}

fn matches_data_postag(entry: &(String, String), pattern: &Regex) -> bool {
    pattern.is_match(&entry.1)
}

impl PortugueseAccentuationCheckRule {
    pub fn new() -> Self {
        Self {
            nouns: load_words(include_str!(
                "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/verbos_sem_acento_nomes_com_acento.txt"
            )),
            adjectives: load_words(include_str!(
                "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/verbos_sem_acento_adj_com_acento.txt"
            )),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn replacement_for(&self, tokens: &[&AnalyzedTokenReadings], i: usize) -> Option<String> {
        let token = if i == 1 {
            tokens[i].token().token().to_lowercase()
        } else {
            tokens[i].token().token().to_string()
        };
        let prev_token = tokens[i - 1].token().token();
        let prev_prev_token = if i > 2 {
            tokens[i - 2].token().token()
        } else {
            ""
        };
        let next_token = tokens.get(i + 1).map(|t| t.token().token()).unwrap_or("");
        let next_next_token = tokens.get(i + 2).map(|t| t.token().token()).unwrap_or("");

        let noun_entry = self.nouns.get(&token).cloned();
        let adj_entry = self.adjectives.get(&token).cloned();
        let mut replacement: Option<String> = None;

        if let Some(entry) = noun_entry.clone()
            && !matches_postag(tokens[i], &GN)
            && ((tokens[i - 1].has_pos_tag("SPS00")
                && !tokens[i - 1].has_pos_tag("RG")
                && !matches_postag(tokens[i - 1], &DETERMINANTE)
                && !matches_postag(tokens[i], &INFINITIVO))
                || (matches_postag(tokens[i - 1], &DETERMINANTE_MS)
                    && matches_data_postag(&entry, &NOME_MS))
                || (matches_postag(tokens[i - 1], &DETERMINANTE_MP)
                    && matches_data_postag(&entry, &NOME_MP))
                || (matches_postag(tokens[i - 1], &DETERMINANTE_FS)
                    && matches_data_postag(&entry, &NOME_FS))
                || (matches_postag(tokens[i - 1], &DETERMINANTE_FP)
                    && matches_data_postag(&entry, &NOME_FP))
                || (i > 2
                    && matches_postag(tokens[i - 2], &VERBO_CONJUGADO)
                    && ((matches_postag(tokens[i - 1], &DETERMINANTE_MS)
                        && matches_data_postag(&entry, &NOME_MS))
                        || (matches_postag(tokens[i - 1], &DETERMINANTE_MP)
                            && matches_data_postag(&entry, &NOME_MP))
                        || (matches_postag(tokens[i - 1], &DETERMINANTE_FS)
                            && matches_data_postag(&entry, &NOME_FS))
                        || (matches_postag(tokens[i - 1], &DETERMINANTE_FP)
                            && matches_data_postag(&entry, &NOME_FP))))
                || (i > 2
                    && matches_postag(tokens[i - 2], &VERBO_CONJUGADO)
                    && ((ARTIGO_O_MS.is_match(prev_token)
                        && matches_data_postag(&entry, &NOME_MS))
                        || (ARTIGO_O_MP.is_match(prev_token)
                            && matches_data_postag(&entry, &NOME_MP))
                        || (ARTIGO_O_FS.is_match(prev_token)
                            && matches_data_postag(&entry, &NOME_FS))
                        || (ARTIGO_O_FP.is_match(prev_token)
                            && matches_data_postag(&entry, &NOME_FP))))
                || (!matches_postag(tokens[i], &PARTICIPIO_MS)
                    && !matches_postag(tokens[i - 1], &NOT_IN_PREV_TOKEN)
                    && i < tokens.len() - 2
                    && !matches_postag(tokens[i + 2], &INFINITIVO)
                    && !EXCEPCOES_ANTES_DE.is_match(next_next_token)
                    && !tokens[i - 1].has_pos_tag("RG"))
                || ((ARTIGO_O_MS.is_match(prev_token) && matches_data_postag(&entry, &NOME_MS))
                    || (ARTIGO_O_FS.is_match(prev_token) && matches_data_postag(&entry, &NOME_FS))
                    || (ARTIGO_O_MP.is_match(prev_token) && matches_data_postag(&entry, &NOME_MP))
                    || (ARTIGO_O_FP.is_match(prev_token) && matches_data_postag(&entry, &NOME_FP)))
                    && PREPOSICAO_DE.is_match(next_token)
                || (i < tokens.len() - 1
                    && ((matches_data_postag(&entry, &NOME_MS)
                        && matches_postag(tokens[i + 1], &ADJETIVO_MS))
                        || (matches_data_postag(&entry, &NOME_FS)
                            && matches_postag(tokens[i + 1], &ADJETIVO_FS))
                        || (matches_data_postag(&entry, &NOME_MP)
                            && matches_postag(tokens[i + 1], &ADJETIVO_MP))
                        || (matches_data_postag(&entry, &NOME_FP)
                            && matches_postag(tokens[i + 1], &ADJETIVO_FP))))
                || ((matches_data_postag(&entry, &NOME_MS)
                    && matches_postag(tokens[i - 1], &ADJETIVO_MS)
                    && !matches_postag(tokens[i], &VERBO_3S)
                    && !matches_postag(tokens[i], &GRUPO_VERBAL))
                    || (matches_data_postag(&entry, &NOME_FS)
                        && matches_postag(tokens[i - 1], &ADJETIVO_FS)
                        && !matches_postag(tokens[i], &VERBO_3S))
                    || (matches_data_postag(&entry, &NOME_MP)
                        && matches_postag(tokens[i - 1], &ADJETIVO_MP))
                    || (matches_data_postag(&entry, &NOME_FP)
                        && matches_postag(tokens[i - 1], &ADJETIVO_FP)))
                || (next_token == "que"
                    && i > 2
                    && ((matches_data_postag(&entry, &NOME_MS)
                        && matches_postag(tokens[i - 1], &ADJETIVO_MS)
                        && matches_postag(tokens[i - 2], &DETERMINANTE_MS))
                        || (matches_data_postag(&entry, &NOME_FS)
                            && matches_postag(tokens[i - 1], &ADJETIVO_FS)
                            && matches_postag(tokens[i - 2], &DETERMINANTE_FS))
                        || (matches_data_postag(&entry, &NOME_MP)
                            && matches_postag(tokens[i - 1], &ADJETIVO_MP)
                            && matches_postag(tokens[i - 2], &DETERMINANTE_MP))
                        || (matches_data_postag(&entry, &NOME_FP)
                            && matches_postag(tokens[i - 1], &ADJETIVO_FP)
                            && matches_postag(tokens[i - 2], &DETERMINANTE_FP))))
                || (next_token == "que"
                    && ((ARTIGO_O_MS.is_match(prev_token)
                        && matches_data_postag(&entry, &NOME_MS))
                        || (ARTIGO_O_FS.is_match(prev_token)
                            && matches_data_postag(&entry, &NOME_FS))
                        || (ARTIGO_O_MP.is_match(prev_token)
                            && matches_data_postag(&entry, &NOME_MP))
                        || (ARTIGO_O_FP.is_match(prev_token)
                            && matches_data_postag(&entry, &NOME_FP))))
                || (i > 2
                    && tokens[i - 2].has_pos_tag("SPS00")
                    && !tokens[i - 2].has_pos_tag("RG")
                    && (matches_data_postag(&entry, &NOME_MS)
                        && matches_postag(tokens[i - 1], &ADJETIVO_MS)
                        || matches_data_postag(&entry, &NOME_FS)
                            && matches_postag(tokens[i - 1], &ADJETIVO_FS)
                        || matches_data_postag(&entry, &NOME_MP)
                            && matches_postag(tokens[i - 1], &ADJETIVO_MP)
                        || matches_data_postag(&entry, &NOME_FP)
                            && matches_postag(tokens[i - 1], &ADJETIVO_FP))))
        {
            replacement = Some(entry.0.clone());
        }

        if let Some(entry) = adj_entry
            && !matches_postag(tokens[i], &GN)
            && ((matches_data_postag(&entry, &ADJETIVO_MS)
                && matches_postag(tokens[i - 1], &NOME_MS)
                && !tokens[i - 1].has_pos_tag("_GN_FS")
                && matches_postag(tokens[i], &VERBO_CONJUGADO)
                && !matches_postag(tokens[i], &VERBO_3S))
                || (matches_data_postag(&entry, &ADJETIVO_FS)
                    && prev_prev_token.eq_ignore_ascii_case("de")
                    && (prev_token == "maneira" || prev_token == "forma"))
                || (matches_data_postag(&entry, &ADJETIVO_MP)
                    && matches_postag(tokens[i - 1], &NOME_MP))
                || (matches_data_postag(&entry, &ADJETIVO_FP)
                    && matches_postag(tokens[i - 1], &NOME_FP))
                || (i < tokens.len() - 1
                    && prev_token != "que"
                    && !matches_postag(tokens[i - 1], &NOT_IN_PREV_TOKEN)
                    && ((matches_data_postag(&entry, &ADJETIVO_MS)
                        && matches_postag(tokens[i + 1], &NOME_MS)
                        && matches_postag(tokens[i - 1], &BEFORE_ADJECTIVE_MS))
                        || (matches_data_postag(&entry, &ADJETIVO_FS)
                            && matches_postag(tokens[i + 1], &NOME_FS)
                            && matches_postag(tokens[i - 1], &BEFORE_ADJECTIVE_FS))
                        || (matches_data_postag(&entry, &ADJETIVO_MP)
                            && matches_postag(tokens[i + 1], &NOME_MP)
                            && matches_postag(tokens[i - 1], &BEFORE_ADJECTIVE_MP))
                        || (matches_data_postag(&entry, &ADJETIVO_FP)
                            && matches_postag(tokens[i + 1], &NOME_FP)
                            && matches_postag(tokens[i - 1], &BEFORE_ADJECTIVE_FP))))
                || (i < tokens.len() - 1
                    && ((matches_data_postag(&entry, &ADJETIVO_MS)
                        && matches_postag(tokens[i + 1], &NOME_MS)
                        && ARTIGO_O_MS.is_match(prev_token))
                        || (matches_data_postag(&entry, &ADJETIVO_FS)
                            && matches_postag(tokens[i + 1], &NOME_FS)
                            && ARTIGO_O_FS.is_match(prev_token))
                        || (matches_data_postag(&entry, &ADJETIVO_MP)
                            && matches_postag(tokens[i + 1], &NOME_MP)
                            && ARTIGO_O_MP.is_match(prev_token))
                        || (matches_data_postag(&entry, &ADJETIVO_FP)
                            && matches_postag(tokens[i + 1], &NOME_FP)
                            && ARTIGO_O_FP.is_match(prev_token)))))
        {
            replacement = Some(entry.0.clone());
        }
        replacement
    }
}

impl Default for PortugueseAccentuationCheckRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for PortugueseAccentuationCheckRule {
    fn id(&self) -> &str {
        "ACCENTUATION_CHECK_PT"
    }

    fn description(&self) -> &str {
        "Confusão com acentos gráficos"
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("CONFUSED_WORDS", "Confused Words")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        for i in 1..tokens.len() {
            let raw = tokens[i].token().token();
            let token = if i == 1 {
                raw.to_lowercase()
            } else {
                raw.to_string()
            };
            if token.is_empty() {
                continue;
            }
            if !self.nouns.contains_key(&token) && !self.adjectives.contains_key(&token) {
                continue;
            }
            if matches_postag(tokens[i - 1], &PRONOME_PESSOAL)
                && !tokens[i - 1].token().token().starts_with('-')
            {
                continue;
            }
            if let Some(replacement) = self.replacement_for(&tokens, i) {
                let from = tokens[i].token().start();
                let to = tokens[i].token().end();
                let ctx_start = from.saturating_sub(40);
                let ctx_end = std::cmp::min(to + 40, sentence.text().len());
                let context_text = sentence.text()[ctx_start..ctx_end].to_string();
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                rule_matches.push(
                    RuleMatch::new(
                        "Se é um nome ou um adjectivo, tem acento.",
                        from,
                        to - from,
                        rm_rule,
                        RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                    )
                    .with_short_message("Falta um acento")
                    .with_replacements(vec![SuggestedReplacement::new(&replacement)])
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
    use crate::pt::test_util::make_sentence;
    use og_core::AnalyzedToken;

    fn with_tags(sentence: &mut AnalyzedSentence, tags: &[&str]) {
        let mut next = 0usize;
        let tokens = sentence.tokens_mut();
        for tag in tags {
            while next < tokens.len() && tokens[next].token().token().trim().is_empty() {
                next += 1;
            }
            if next >= tokens.len() {
                break;
            }
            let text = tokens[next].token().token().to_string();
            let start = tokens[next].token().start();
            let end = tokens[next].token().end();
            tokens[next]
                .readings_mut()
                .push(AnalyzedToken::new(text, start, end).with_pos_tags(vec![tag.to_string()]));
            next += 1;
        }
    }

    #[test]
    fn data_files_load() {
        let rule = PortugueseAccentuationCheckRule::new();
        assert!(rule.nouns.contains_key("abobada"));
        assert_eq!(rule.nouns["abobada"].0, "abóbada");
        assert!(rule.adjectives.contains_key("acarida"));
    }

    #[test]
    fn noun_after_determiner_gets_accent() {
        let rule = PortugueseAccentuationCheckRule::new();
        let mut sentence = make_sentence("A abobada desabou.");
        with_tags(&mut sentence, &["", "DA0FS0", "VMIS3S0", "", ""]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "abóbada");
    }

    #[test]
    fn pronoun_subject_stays_quiet() {
        let rule = PortugueseAccentuationCheckRule::new();
        let mut sentence = make_sentence("Ele abobada ontem.");
        with_tags(&mut sentence, &["", "P010P000", "VMIS3S0", "RG", ""]);
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn rule_is_default_off_like_java() {
        assert!(!PortugueseAccentuationCheckRule::new().is_default_on());
    }

    #[test]
    fn noun_arms_fire_across_the_context_patterns() {
        let rule = PortugueseAccentuationCheckRule::new();
        for (text, tags, expected) in [
            (
                "Gosto de abobada.",
                &["", "VMIP1S0", "SPS00", "", ""][..],
                "abóbada",
            ),
            ("o abrenuncio", &["", "DA0MS0", ""][..], "abrenúncio"),
            (
                "Ele comeu a abobada logo.",
                &["", "PP3CS000", "VMIS3S0", "DA0FS0", "", "RG", ""],
                "abóbada",
            ),
            (
                "Ele comeu a abobada logo.",
                &["", "PP3CS000", "VMIS3S0", "", "", "RG", ""],
                "abóbada",
            ),
            (
                "A abobada de caiu.",
                &["", "DA0FS0", "", "SPS00", "VMIS3S0", ""],
                "abóbada",
            ),
            (
                "A abobada boa caiu.",
                &["", "DA0FS0", "", "AQ0FS0", "VMIS3S0", ""],
                "abóbada",
            ),
            (
                "A boa abobada caiu.",
                &["", "DA0FS0", "AQ0FS0", "", "VMIS3S0", ""],
                "abóbada",
            ),
            (
                "A abobada que caiu.",
                &["", "DA0FS0", "", "PR0CN000", "VMIS3S0", ""],
                "abóbada",
            ),
            (
                "de boa abobada",
                &["", "SPS00", "AQ0FS0", ""][..],
                "abóbada",
            ),
            (
                "comprou e abobada agora.",
                &["", "VMIS3S0", "CC", "", "RG", ""],
                "abóbada",
            ),
            (
                "Abobada boa caiu.",
                &["", "", "AQ0FS0", "VMIS3S0", ""],
                "abóbada",
            ),
        ] {
            let mut sentence = make_sentence(text);
            with_tags(&mut sentence, tags);
            let matches = rule.match_sentence(&sentence);
            assert_eq!(
                matches.len(),
                1,
                "{text}: expected one accentuation match, got {matches:?}"
            );
            assert_eq!(
                matches[0].replacements()[0].value(),
                expected,
                "{text}: wrong replacement"
            );
        }
    }

    #[test]
    fn noun_arms_stay_quiet_for_pronoun_subjects_and_verb_readings() {
        let rule = PortugueseAccentuationCheckRule::new();
        for (text, tags) in [(
            "Ele abobada ontem.",
            &["", "PP3CS000", "VMIS3S0", "RG", ""][..],
        )] {
            let mut sentence = make_sentence(text);
            with_tags(&mut sentence, tags);
            assert!(
                rule.match_sentence(&sentence).is_empty(),
                "{text}: must stay unflagged"
            );
        }
    }

    #[test]
    fn adjective_arms_fire_across_the_context_patterns() {
        let rule = PortugueseAccentuationCheckRule::new();
        for (text, tags, expected) in [
            ("canto acarido", &["", "NCMS000", "VMIS1S0"][..], "acárido"),
            (
                "de maneira acarida",
                &["", "SPS00", "NCFS000", ""][..],
                "acárida",
            ),
            (
                "em acarida casa",
                &["", "SPS00", "", "NCFS000"][..],
                "acárida",
            ),
            (
                "a acarida casa",
                &["", "DA0FS0", "", "NCFS000"][..],
                "acárida",
            ),
        ] {
            let mut sentence = make_sentence(text);
            with_tags(&mut sentence, tags);
            let matches = rule.match_sentence(&sentence);
            assert_eq!(
                matches.len(),
                1,
                "{text}: expected one accentuation match, got {matches:?}"
            );
            assert_eq!(
                matches[0].replacements()[0].value(),
                expected,
                "{text}: wrong replacement"
            );
        }
    }

    #[test]
    fn adjective_arms_stay_quiet_after_que_and_before_infinitives() {
        let rule = PortugueseAccentuationCheckRule::new();
        for (text, tags) in [
            ("que acarida casa", &["", "PR0CN000", "", "NCFS000"][..]),
            ("para acarida agora", &["", "SPS00", "", "RG", ""][..]),
        ] {
            let mut sentence = make_sentence(text);
            with_tags(&mut sentence, tags);
            assert!(
                rule.match_sentence(&sentence).is_empty(),
                "{text}: must stay unflagged"
            );
        }
    }

    #[test]
    fn masculine_noun_arms_fire_after_verbs_and_articles() {
        let rule = PortugueseAccentuationCheckRule::new();
        for (text, tags, expected) in [
            (
                "Ele comeu este abrenuncio.",
                &["", "PP3CS000", "VMIS3S0", "DD0MS00", "", ""][..],
                "abrenúncio",
            ),
            (
                "Ele comeu o abrenuncio.",
                &["", "PP3CS000", "VMIS3S0", "", "", ""][..],
                "abrenúncio",
            ),
            ("o abrenuncio de", &["", "", "", "SPS00"][..], "abrenúncio"),
            (
                "o abrenuncio baixo caiu.",
                &["", "", "", "AQ0MS0", "VMIS3S0", ""],
                "abrenúncio",
            ),
            (
                "O baixo abrenuncio caiu.",
                &["", "AQ0MS0", "", "VMIS3S0", ""],
                "abrenúncio",
            ),
            (
                "O abrenuncio que caiu.",
                &["", "", "", "PR0CN000", "VMIS3S0", ""],
                "abrenúncio",
            ),
            (
                "este baixo abrenuncio que caiu.",
                &["", "DD0MS00", "AQ0MS0", "", "PR0CN000", "VMIS3S0", ""],
                "abrenúncio",
            ),
            (
                "de baixo abrenuncio",
                &["", "SPS00", "AQ0MS0", ""][..],
                "abrenúncio",
            ),
        ] {
            let mut sentence = make_sentence(text);
            with_tags(&mut sentence, tags);
            let matches = rule.match_sentence(&sentence);
            assert_eq!(
                matches.len(),
                1,
                "{text}: expected one accentuation match, got {matches:?}"
            );
            assert_eq!(
                matches[0].replacements()[0].value(),
                expected,
                "{text}: wrong replacement"
            );
        }
    }

    #[test]
    fn masculine_adjective_arms_fire_before_and_after_nouns() {
        let rule = PortugueseAccentuationCheckRule::new();
        for (text, tags, expected) in [
            (
                "em acarido prato",
                &["", "SPS00", "", "NCMS000"][..],
                "acárido",
            ),
            ("o acarido prato", &["", "", "", "NCMS000"][..], "acárido"),
        ] {
            let mut sentence = make_sentence(text);
            with_tags(&mut sentence, tags);
            let matches = rule.match_sentence(&sentence);
            assert_eq!(
                matches.len(),
                1,
                "{text}: expected one accentuation match, got {matches:?}"
            );
            assert_eq!(
                matches[0].replacements()[0].value(),
                expected,
                "{text}: wrong replacement"
            );
        }
    }

    #[test]
    fn default_constructor_loads_the_same_data() {
        let rule = PortugueseAccentuationCheckRule::default();
        assert!(rule.nouns.contains_key("abrenuncio"));
        assert!(rule.adjectives.contains_key("acarido"));
    }
}
