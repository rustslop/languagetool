use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use og_core::{AnalyzedSentence, RuleMatch};
use og_rules::language_filter::{LanguageFilterCall, LanguageFilterHook};

use super::adjust_pronouns::accept_rule_match as adjust_pronouns_accept;
use super::adjust_verb_suggestions::{
    AdjustArgs, AdjustServices, accept_rule_match as adjust_verb_accept,
};
use super::ca_adapt::adapt_suggestion_ca;
use super::convert_gender_number::{ConvertArgs, accept_rule_match as convert_accept};
use super::en_no_infinitiu::accept_rule_match as en_no_infinitiu_accept;
use super::oblidarse_suggestions::accept_rule_match as oblidarse_accept;
use super::portar_gerundi::accept_rule_match as portar_gerundi_accept;
use super::portar_temps::accept_rule_match as adjust_portar_temps_accept;
use super::possessius_redundants::accept_rule_match as possessius_accept;
use super::que_inici::accept_rule_match as que_inici_accept;

pub struct CatalanPatternFilterHook {
    tagger: Option<Arc<crate::ca::tagger::CatalanTagger>>,
    ca_synth: Option<Arc<super::ca_synthesizer::CaSynthesizer>>,
}

fn arg<'a>(args: &'a HashMap<&'a str, String>, key: &str) -> &'a str {
    args.get(key).map(String::as_str).unwrap_or("")
}

fn offset_aligned_sentence_text(sentence: &AnalyzedSentence) -> Cow<'_, str> {
    let base = sentence.start();
    if base == 0 {
        return Cow::Borrowed(sentence.text());
    }
    Cow::Owned(format!("{}{}", " ".repeat(base), sentence.text()))
}

impl CatalanPatternFilterHook {
    pub fn new(
        tagger: Option<Arc<crate::ca::tagger::CatalanTagger>>,
        synthesizer: Option<Arc<og_rules::postponed_adjective::LangSynthesizer>>,
    ) -> Self {
        let ca_synth = synthesizer.as_ref().map(|synth| {
            let dir = crate::ca::tagger::ca_resource_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("ca-resource-missing"));
            Arc::new(super::ca_synthesizer::CaSynthesizer::build(
                &dir,
                synth.clone(),
            ))
        });
        Self { tagger, ca_synth }
    }

    fn tag(&self, word: &str) -> Vec<og_core::AnalyzedTokenReadings> {
        self.tagger
            .as_ref()
            .map(|tagger| og_tagger::Tagger::tag(tagger.as_ref(), &[word]))
            .unwrap_or_default()
    }

    fn synth_regexp(&self, lemma: &str, postag: &str) -> Vec<String> {
        self.ca_synth
            .as_ref()
            .map(|synth| synth.synthesize_regexp(lemma, postag))
            .unwrap_or_default()
    }

    fn synth_plain(&self, lemma: &str, postag: &str) -> Vec<String> {
        self.ca_synth
            .as_ref()
            .map(|synth| synth.synthesize_plain(lemma, postag))
            .unwrap_or_default()
    }
}

impl LanguageFilterHook for CatalanPatternFilterHook {
    fn apply_filter(&self, call: LanguageFilterCall<'_>, rm: RuleMatch) -> Option<RuleMatch> {
        match call.class {
            "org.languagetool.rules.ca.QueIniciFilter" => {
                let mode = call.args.get("mode").map(String::as_str);
                if que_inici_accept(rm.rule().id(), mode, call.tokens) {
                    Some(rm)
                } else {
                    None
                }
            }
            "org.languagetool.rules.ca.ConvertToGenderAndNumberFilter" => {
                let args = ConvertArgs {
                    gender: arg(call.args, "gender"),
                    number: arg(call.args, "number"),
                    lemma_select: arg(call.args, "lemmaSelect"),
                    new_lemma: arg(call.args, "newLemma"),
                    keep_original: arg(call.args, "keepOriginal").eq_ignore_ascii_case("true"),
                };
                let sentence_text = offset_aligned_sentence_text(call.sentence);
                convert_accept(
                    &rm,
                    &sentence_text,
                    rm.offset(),
                    call.tokens,
                    &args,
                    &|word| self.tag(word),
                    &|lemma, postag| self.synth_regexp(lemma, postag),
                )
            }
            "org.languagetool.rules.ca.EnNoInfinitiuSuggestionFilter" => {
                let sentence_text = offset_aligned_sentence_text(call.sentence);
                en_no_infinitiu_accept(
                    &rm,
                    &sentence_text,
                    rm.offset(),
                    rm.offset() + rm.length(),
                    call.tokens,
                    &|lemma, postag| self.synth_plain(lemma, postag),
                    &|s| adapt_suggestion_ca(s, ""),
                )
            }
            "org.languagetool.rules.ca.PortarTempsSuggestionsFilter" => {
                adjust_portar_temps_accept(&rm, rm.offset(), call.tokens, &|lemma, postag| {
                    self.synth_regexp(lemma, postag)
                })
            }
            "org.languagetool.rules.ca.PortarGerundiSuggestionsFilter" => portar_gerundi_accept(
                &rm,
                rm.offset(),
                call.tokens,
                arg(call.args, "newLemma"),
                &|lemma, postag| self.synth_regexp(lemma, postag),
            ),
            "org.languagetool.rules.AdaptSuggestionsFilter" => {
                let sentence_text = offset_aligned_sentence_text(call.sentence);
                let original = sentence_text
                    .get(rm.offset()..rm.offset() + rm.length())
                    .unwrap_or("")
                    .to_string();
                let adapted: Vec<og_core::SuggestedReplacement> = rm
                    .replacements()
                    .iter()
                    .map(|r| {
                        og_core::SuggestedReplacement::new(&adapt_suggestion_ca(
                            r.value(),
                            &original,
                        ))
                    })
                    .collect();
                let mut out = rm;
                out.set_replacements(adapted);
                Some(out)
            }
            "org.languagetool.rules.ca.AdjustPronounsFilter" => {
                let services = AdjustServices {
                    tagger: &|word| self.tag(word),
                    synth: &|lemma, postag| self.synth_regexp(lemma, postag),
                };
                let _ = &services;
                adjust_pronouns_accept(
                    &rm,
                    rm.offset(),
                    call.tokens,
                    arg(call.args, "actions"),
                    arg(call.args, "newLemma"),
                    arg(call.args, "newOnlyLemma"),
                    &|lemma, postag| self.synth_regexp(lemma, postag),
                )
            }
            "org.languagetool.rules.ca.AdjustVerbSuggestionsFilter" => {
                let sentence_text = offset_aligned_sentence_text(call.sentence);
                let services = AdjustServices {
                    tagger: &|word| self.tag(word),
                    synth: &|lemma, postag| self.synth_plain(lemma, postag),
                };
                adjust_verb_accept(
                    &rm,
                    &sentence_text,
                    rm.offset(),
                    rm.offset() + rm.length(),
                    &AdjustArgs {
                        actions: arg(call.args, "actions"),
                        number_from_next_words: arg(call.args, "numberFromNextWords")
                            .eq_ignore_ascii_case("true"),
                        force_number: arg(call.args, "forceNumber"),
                    },
                    call.tokens,
                    &services,
                )
            }
            "org.languagetool.rules.ca.OblidarseSugestionsFilter" => {
                oblidarse_accept(&rm, rm.offset(), call.tokens, &|lemma, postag| {
                    self.synth_plain(lemma, postag)
                })
            }
            "org.languagetool.rules.ca.PossessiusRedundantsFilter" => {
                let pattern_token_pos = call.matched_positions.first().copied().unwrap_or(0);
                let mut rm = rm;
                possessius_accept(&mut rm, pattern_token_pos, call.tokens).map(|()| rm)
            }
            "org.languagetool.rules.ca.DiacriticsCheckFilter" => {
                super::diacritics_check::accept_rule_match(
                    &rm,
                    call.args,
                    call.tokens,
                    call.matched_positions,
                )
            }
            _ => Some(rm),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_classes_stay_untouched() {
        let hook = CatalanPatternFilterHook::new(None, None);
        let rm = RuleMatch::new(
            "m",
            0,
            1,
            og_core::RuleMatchRule::new("R", "d"),
            og_core::RuleMatchContext::new("c", 0, 1),
        );
        let out = hook
            .apply_filter(
                LanguageFilterCall {
                    class: "org.languagetool.rules.some.OtherFilter",
                    args: &HashMap::new(),
                    matched_texts: &[],
                    matched_positions: &[],
                    element_positions: &[],
                    tokens: &[],
                    sentence: &AnalyzedSentence::new("a", 0, 1),
                },
                rm,
            )
            .expect("kept");
        assert_eq!(out.rule().id(), "R");
    }

    #[test]
    fn adjust_verb_and_oblidarse_route_into_the_module() {
        let hook = CatalanPatternFilterHook::new(None, None);
        for class in [
            "org.languagetool.rules.ca.AdjustVerbSuggestionsFilter",
            "org.languagetool.rules.ca.OblidarseSugestionsFilter",
        ] {
            let rm = RuleMatch::new(
                "m",
                0,
                1,
                og_core::RuleMatchRule::new("R", "d"),
                og_core::RuleMatchContext::new("c", 0, 1),
            );
            let out = hook.apply_filter(
                LanguageFilterCall {
                    class,
                    args: &HashMap::new(),
                    matched_texts: &[],
                    matched_positions: &[],
                    element_positions: &[],
                    tokens: &[],
                    sentence: &AnalyzedSentence::new("a", 0, 1),
                },
                rm,
            );
            assert!(
                out.is_none(),
                "{class} must reach the ported filter, not the default keep"
            );
        }
    }
}
