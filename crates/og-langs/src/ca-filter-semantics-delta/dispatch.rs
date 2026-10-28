use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use og_core::{AnalyzedSentence, RuleMatch};
use og_rules::language_filter::{LanguageFilterCall, LanguageFilterHook};

use super::convert_gender_number::{ConvertArgs, accept_rule_match as convert_accept};
use super::en_no_infinitiu::accept_rule_match as en_no_infinitiu_accept;
use super::portar_gerundi::accept_rule_match as portar_gerundi_accept;
use super::possessius_redundants::accept_rule_match as possessius_accept;
use super::que_inici::accept_rule_match as que_inici_accept;

pub struct CatalanPatternFilterHook {
    tagger: Option<Arc<crate::ca::tagger::CatalanTagger>>,
    synthesizer: Option<Arc<og_rules::postponed_adjective::LangSynthesizer>>,
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
        Self {
            tagger,
            synthesizer,
        }
    }

    fn tag(&self, word: &str) -> Vec<og_core::AnalyzedTokenReadings> {
        self.tagger
            .as_ref()
            .map(|tagger| og_tagger::Tagger::tag(tagger.as_ref(), &[word]))
            .unwrap_or_default()
    }

    fn synth(&self, lemma: &str, postag: &str) -> Vec<String> {
        self.synthesizer
            .as_ref()
            .map(|synth| synth.synthesize_exact(lemma, postag))
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
                    &|lemma, postag| self.synth(lemma, postag),
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
                    &|lemma, postag| self.synth(lemma, postag),
                    &|s| s.to_string(),
                )
            }
            "org.languagetool.rules.ca.PortarGerundiSuggestionsFilter" => portar_gerundi_accept(
                &rm,
                rm.offset(),
                call.tokens,
                arg(call.args, "newLemma"),
                &|lemma, postag| self.synth(lemma, postag),
            ),
            "org.languagetool.rules.ca.PossessiusRedundantsFilter" => {
                let pattern_token_pos = call.matched_positions.first().copied().unwrap_or(0);
                let mut rm = rm;
                possessius_accept(&mut rm, pattern_token_pos, call.tokens).map(|()| rm)
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
}
