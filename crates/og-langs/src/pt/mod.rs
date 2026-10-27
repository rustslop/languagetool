pub mod accentuation;
pub mod compound;
pub mod dash;
pub mod filler_words;
pub mod filters;
pub mod readability;
pub mod simple_replace;
pub mod simple_replace2;
pub mod tagger;
pub mod unit_conversion;
pub mod word_repeat;
pub mod word_tokenizer;

#[cfg(test)]
pub(crate) mod example_suite;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
pub(crate) mod trace;

use crate::en::{WordCoherencyRule, WrongWordInContextRule};
use og_core::rule::{Rule, TextLevelRule};
use std::path::Path;
use std::sync::Arc;

struct PtDisambiguatorChain {
    hybrid: Arc<dyn og_core::checker::Disambiguator>,
    global_xml: Arc<og_tagger::XmlDisambiguator>,
}

impl og_core::checker::Disambiguator for PtDisambiguatorChain {
    fn disambiguate(&self, sentence: &mut og_core::AnalyzedSentence) {
        self.hybrid.disambiguate(sentence);
        self.global_xml.disambiguate(sentence);
    }
}

pub fn build_pt_disambiguator(
    resource_dir: &Path,
) -> Option<Arc<dyn og_core::checker::Disambiguator>> {
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).ok()?;
    let expanded =
        og_xml::parser::XmlRuleParser::expand_entities_with_base(&xml, Some(resource_dir));
    let xml_disambiguator = og_tagger::XmlDisambiguator::from_xml(&expanded).ok()?;
    let core = crate::engine::find_lt_core_resource_dir()?;
    let hybrid = og_tagger::hybrid_disambiguator("pt", resource_dir, &core, xml_disambiguator);
    let global_xml = std::fs::read_to_string(core.join("disambiguation-global.xml")).ok()?;
    let global_expanded =
        og_xml::parser::XmlRuleParser::expand_entities_with_base(&global_xml, Some(&core));
    let global = Arc::new(og_tagger::XmlDisambiguator::from_xml(&global_expanded).ok()?);
    Some(Arc::new(PtDisambiguatorChain {
        hybrid,
        global_xml: global,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PtVariant {
    Angola,
    Brazil,
    Mozambique,
    Portugal,
}

pub fn variant_for_code(code: &str) -> Option<PtVariant> {
    match code {
        "pt-AO" => Some(PtVariant::Angola),
        "pt-BR" => Some(PtVariant::Brazil),
        "pt-MZ" => Some(PtVariant::Mozambique),
        "pt-PT" => Some(PtVariant::Portugal),
        _ => None,
    }
}

pub fn variant_rules_dir(variant: PtVariant) -> &'static str {
    match variant {
        PtVariant::Angola => "pt-AO",
        PtVariant::Brazil => "pt-BR",
        PtVariant::Mozambique => "pt-MZ",
        PtVariant::Portugal => "pt-PT",
    }
}

pub fn portuguese_paragraph_repeat_beginning_rule()
-> crate::paragraph_repeat_beginning::ParagraphRepeatBeginningRule {
    crate::paragraph_repeat_beginning::ParagraphRepeatBeginningRule::new(
        "Início de parágrafo repetido",
        "O mesmo começo que o parágrafo anterior",
        "Estilo",
    )
}

pub fn portuguese_word_coherency_rule() -> WordCoherencyRule {
    WordCoherencyRule::for_language(
        "PT_WORD_COHERENCY",
        "Consistência de palavras com grafias múltiplas",
        include_str!("../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/coherency.txt"),
    )
    .with_message_template(
        "Não deve utilizar formas distintas de palavras com dupla grafia no mesmo texto. Escolha entre '{word1}' e '{word2}'.",
    )
}

pub fn portuguese_wrong_word_in_context_rule() -> WrongWordInContextRule {
    WrongWordInContextRule::for_language(
        "PORTUGUESE_WRONG_WORD_IN_CONTEXT",
        "Confusão de palavra dentro do contexto (p.ex. infligir/infringir, etc.)",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/rules/pt/wrongWordInContext.txt"
        ),
        "Pretende dizer <suggestion>$SUGGESTION</suggestion> em vez de $WRONGWORD?",
        "Considere <suggestion>$SUGGESTION</suggestion>, i.e. $EXPLANATION_SUGGESTION, em vez de '$WRONGWORD', i.e. $EXPLANATION_WRONGWORD?",
    )
}

pub struct PortugueseNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn shared_native_rules() -> PortugueseNativeRules {
    PortugueseNativeRules {
        rules: vec![
            Arc::new(compound::PortugueseCompoundRule::post_reform()),
            Arc::new(compound::PortugueseCompoundRule::colour_hyphenation()),
            Arc::new(simple_replace::orthography_replace_rule()),
            Arc::new(simple_replace::portuguese_replace_rule()),
            Arc::new(simple_replace2::barbarisms_base()),
            Arc::new(simple_replace2::cliche_base()),
            Arc::new(simple_replace2::redundancy_base()),
            Arc::new(simple_replace2::wordiness_base()),
            Arc::new(simple_replace2::wikipedia_base()),
            Arc::new(word_repeat::PortugueseWordRepeatRule::new()),
            Arc::new(accentuation::PortugueseAccentuationCheckRule::new()),
            Arc::new(simple_replace2::diacritics_rule()),
            Arc::new(portuguese_wrong_word_in_context_rule()),
            Arc::new(unit_conversion::PortugueseUnitConversionRule::new()),
            Arc::new(simple_replace::english_contraction_rule()),
        ],
        text_level_rules: vec![
            Arc::new(portuguese_paragraph_repeat_beginning_rule()),
            Arc::new(filler_words::PortugueseFillerWordsRule::new()),
            Arc::new(word_repeat::PortugueseWordRepeatBeginningRule::new()),
            Arc::new(portuguese_word_coherency_rule()),
            Arc::new(readability::PortugueseReadabilityRule::new(true)),
            Arc::new(readability::PortugueseReadabilityRule::new(false)),
        ],
    }
}

pub fn variant_native_rules(variant: PtVariant) -> PortugueseNativeRules {
    if matches!(variant, PtVariant::Angola | PtVariant::Mozambique) {
        return PortugueseNativeRules {
            rules: vec![
                Arc::new(compound::PortugueseCompoundRule::pre_reform()),
                Arc::new(dash::PreReformPortugueseDashRule::new()),
            ],
            text_level_rules: Vec::new(),
        };
    }
    let (replace, barbarisms, archaisms, cliche, redundancy, wordiness, wikipedia) = match variant {
        PtVariant::Portugal | PtVariant::Angola | PtVariant::Mozambique => (
            simple_replace2::portugal_replace_rule(),
            simple_replace2::barbarisms_portugal(),
            simple_replace2::archaisms_portugal(),
            simple_replace2::cliche_portugal(),
            simple_replace2::redundancy_portugal(),
            simple_replace2::wordiness_portugal(),
            simple_replace2::wikipedia_portugal(),
        ),
        PtVariant::Brazil => (
            simple_replace2::brazilian_replace_rule(),
            simple_replace2::barbarisms_brazil(),
            simple_replace2::archaisms_brazil(),
            simple_replace2::cliche_brazil(),
            simple_replace2::redundancy_brazil(),
            simple_replace2::wordiness_brazil(),
            simple_replace2::wikipedia_brazil(),
        ),
    };
    let agreement = (variant == PtVariant::Portugal).then(simple_replace::agreement_replace_rule);
    PortugueseNativeRules {
        rules: std::iter::once(
            Arc::new(compound::PortugueseCompoundRule::post_reform()) as Arc<dyn Rule>
        )
        .chain(std::iter::once(
            Arc::new(dash::PostReformPortugueseDashRule::new()) as Arc<dyn Rule>,
        ))
        .chain(agreement.map(|r| Arc::new(r) as Arc<dyn Rule>))
        .chain([
            Arc::new(replace) as Arc<dyn Rule>,
            Arc::new(barbarisms),
            Arc::new(archaisms),
            Arc::new(cliche),
            Arc::new(redundancy),
            Arc::new(wordiness),
            Arc::new(wikipedia),
        ])
        .collect(),
        text_level_rules: Vec::new(),
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    pub(crate) fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0usize;
        while offset < text.len() {
            let next = text[offset..]
                .chars()
                .next()
                .expect("non-empty slice has a char");
            let end = if next.is_whitespace() {
                text[offset..]
                    .char_indices()
                    .find(|(_, c)| !c.is_whitespace())
                    .map(|(i, _)| offset + i)
                    .unwrap_or(text.len())
            } else if next.is_alphanumeric() || next == '-' || next == '\u{00E9}' {
                text[offset..]
                    .char_indices()
                    .find(|(_, c)| !(c.is_alphanumeric() || *c == '-'))
                    .map(|(i, _)| offset + i)
                    .unwrap_or(text.len())
            } else {
                offset + next.len_utf8()
            };
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                &text[offset..end],
                offset,
                end,
            )));
            offset = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_for_code_maps_codes() {
        assert_eq!(variant_for_code("pt-BR"), Some(PtVariant::Brazil));
        assert_eq!(variant_for_code("pt-PT"), Some(PtVariant::Portugal));
        assert_eq!(variant_for_code("pt-AO"), Some(PtVariant::Angola));
        assert_eq!(variant_for_code("pt-MZ"), Some(PtVariant::Mozambique));
        assert_eq!(variant_for_code("pt"), None);
        assert_eq!(variant_rules_dir(PtVariant::Portugal), "pt-PT");
        assert_eq!(variant_rules_dir(PtVariant::Mozambique), "pt-MZ");
    }

    #[test]
    fn shared_native_rules_carry_java_ids() {
        let rules = shared_native_rules();
        let ids: Vec<&str> = rules.rules.iter().map(|r| r.id()).collect();
        for expected in [
            "PT_COMPOUNDS_POST_REFORM",
            "PT_COLOUR_HYPHENATION",
            "PT_SIMPLE_REPLACE_ORTHOGRAPHY",
            "PT_SIMPLE_REPLACE",
            "PT_BARBARISMS_REPLACE",
            "PT_CLICHE_REPLACE",
            "PT_REDUNDANCY_REPLACE",
            "PT_WORDINESS_REPLACE",
            "PT_WIKIPEDIA_COMMON_ERRORS",
            "PORTUGUESE_WORD_REPEAT_RULE",
            "ACCENTUATION_CHECK_PT",
            "PT_DIACRITICS_REPLACE",
            "UNIDADES_METRICAS",
            "PORTUGUESE_WRONG_WORD_IN_CONTEXT",
            "PT_ENGLISH_CONTRACTION_ORTHOGRAPHY",
        ] {
            assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
        }
        let text_ids: Vec<&str> = rules.text_level_rules.iter().map(|r| r.id()).collect();
        for expected in [
            "PARAGRAPH_REPEAT_BEGINNING_RULE",
            "FILLER_WORDS_PT",
            "PORTUGUESE_WORD_REPEAT_BEGINNING_RULE",
            "PT_WORD_COHERENCY",
            "READABILITY_RULE_SIMPLE_PT",
            "READABILITY_RULE_DIFFICULT_PT",
        ] {
            assert!(text_ids.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn paragraph_repeat_beginning_rule_carries_pt_bundle_strings() {
        let rule = portuguese_paragraph_repeat_beginning_rule();
        assert_eq!(rule.id(), "PARAGRAPH_REPEAT_BEGINNING_RULE");
        assert!(!rule.is_default_on());
        assert_eq!(rule.description(), "Início de parágrafo repetido");
        assert_eq!(rule.category().name(), "Estilo");
    }

    #[test]
    fn portugal_variant_rules_carry_java_ids() {
        let rules = variant_native_rules(PtVariant::Portugal);
        let ids: Vec<&str> = rules.rules.iter().map(|r| r.id()).collect();
        for expected in [
            "PT_POSAO_DASH_RULE",
            "PT_AGREEMENT_REPLACE",
            "PT_PT_SIMPLE_REPLACE",
            "PT_ARCHAISMS_REPLACE",
        ] {
            assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
        }
    }

    #[test]
    fn brazil_variant_rules_carry_java_ids() {
        let rules = variant_native_rules(PtVariant::Brazil);
        let ids: Vec<&str> = rules.rules.iter().map(|r| r.id()).collect();
        for expected in ["PT_BR_SIMPLE_REPLACE", "PT_ARCHAISMS_REPLACE"] {
            assert!(ids.contains(&expected));
        }
        assert!(!ids.contains(&"PT_AGREEMENT_REPLACE"));
    }

    #[test]
    fn pre_reform_variant_rules_carry_java_ids() {
        for variant in [PtVariant::Angola, PtVariant::Mozambique] {
            let rules = variant_native_rules(variant);
            let ids: Vec<&str> = rules.rules.iter().map(|r| r.id()).collect();
            assert_eq!(
                ids,
                vec!["PT_COMPOUNDS_PRE_REFORM", "PT_PREAO_DASH_RULE"],
                "AngolaPortuguese/MozambiquePortuguese add exactly the two pre-reform rules over the shared base: {ids:?}"
            );
            assert!(rules.text_level_rules.is_empty());
        }
        assert!(
            variant_native_rules(PtVariant::Portugal)
                .rules
                .iter()
                .all(|r| r.id() != "PT_COMPOUNDS_PRE_REFORM")
        );
    }

    #[test]
    fn coherency_rule_matches_java_example() {
        let rule = portuguese_word_coherency_rule();
        let s1 = test_util::make_sentence("Foi um período duradouro.");
        let s2 = test_util::make_sentence("Tão marcante e duradoiro dificilmente será esquecido.");
        let text = format!("{} {}", s1.text(), s2.text());
        let matches = rule.match_text(&text, &[s1, s2]);
        assert!(!matches.is_empty(), "duradouro/duradoiro must be flagged");
    }

    #[test]
    fn wrong_word_context_matches_java_example() {
        let rule = portuguese_wrong_word_in_context_rule();
        let matches = rule.match_sentence(&test_util::make_sentence(
            "O acidente infringiu grandes danos.",
        ));
        assert!(
            matches
                .iter()
                .any(|m| m.replacements().iter().any(|r| r.value() == "infligiu")),
            "expected infligiu suggestion, got {matches:?}"
        );
    }
}
