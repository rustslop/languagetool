#[cfg(test)]
pub(crate) mod dict_census;
#[cfg(test)]
pub(crate) mod disambig_suite;
pub mod filters;
pub mod question_mark;
pub mod repeated_words;
pub mod replace_verbs;
#[cfg(test)]
pub(crate) mod residue_analysis;
pub mod simple_replace;
pub mod speller;
#[cfg(test)]
pub(crate) mod suppress;
pub mod tagger;
#[cfg(test)]
pub(crate) mod trace;
pub mod unpaired_brackets;
pub mod word_repeat;
pub mod word_repeat_beginning;
pub mod word_tokenizer;

use og_core::rule::{Rule, TextLevelRule};
use std::path::PathBuf;
use std::sync::Arc;

pub fn es_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("es/src/main/resources/org/languagetool/resource/es"))
}

pub fn es_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("es/src/main/resources/org/languagetool/rules/es"))
}

pub struct SpanishWrongWordInContextRule {
    inner: crate::en::wrong_word_context::WrongWordInContextRule,
}

impl SpanishWrongWordInContextRule {
    pub fn new() -> Self {
        Self {
            inner: crate::en::wrong_word_context::WrongWordInContextRule::for_language(
                "SPANISH_WRONG_WORD_IN_CONTEXT",
                "Confusión según el contexto: $match",
                include_str!(
                    "../../../../languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/wrongWordInContext.txt"
                ),
                "¿Quería decir <suggestion>$SUGGESTION</suggestion> en vez de \"$WRONGWORD\"?",
                "¿Quería decir <suggestion>$SUGGESTION</suggestion> ($EXPLANATION_SUGGESTION) en vez de \"$WRONGWORD\" ($EXPLANATION_WRONGWORD)?",
            ),
        }
    }
}

impl Default for SpanishWrongWordInContextRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for SpanishWrongWordInContextRule {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn is_default_on(&self) -> bool {
        self.inner.is_default_on()
    }

    fn category(&self) -> og_core::Category {
        og_core::Category::new("CONFUSED_WORDS", "Confusiones")
    }

    fn issue_type(&self) -> og_core::IssueType {
        og_core::IssueType::Grammar
    }

    fn match_sentence(&self, sentence: &og_core::AnalyzedSentence) -> Vec<og_core::RuleMatch> {
        self.inner.match_sentence(sentence)
    }
}

pub struct SpanishCompoundRule {
    inner: crate::en::compound::CompoundRule,
}

impl SpanishCompoundRule {
    pub fn new() -> Self {
        Self {
            inner: crate::en::compound::CompoundRule::for_language(
                "ES_COMPOUNDS",
                "Palabras compuestas con guion: $match",
                "Se escribe con un guion.",
                "Se escribe junto sin espacio ni guion.",
                "Se escribe junto o con un guion.",
                include_str!(
                    "../../../../languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/resource/es/compounds.txt"
                ),
            ),
        }
    }
}

impl Default for SpanishCompoundRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for SpanishCompoundRule {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn is_default_on(&self) -> bool {
        self.inner.is_default_on()
    }

    fn category(&self) -> og_core::Category {
        og_core::Category::new("COMPOUNDING", "Palabras compuestas")
    }

    fn issue_type(&self) -> og_core::IssueType {
        og_core::IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &og_core::AnalyzedSentence) -> Vec<og_core::RuleMatch> {
        self.inner.match_sentence(sentence)
    }
}

pub type SpanishNativeRules = (Vec<Arc<dyn Rule>>, Vec<Arc<dyn TextLevelRule>>);

pub fn native_rules() -> SpanishNativeRules {
    let sentence_rules: Vec<Arc<dyn Rule>> = vec![
        Arc::new(simple_replace::SimpleReplaceRule::spanish()),
        Arc::new(SpanishWrongWordInContextRule::new()),
        Arc::new(SpanishCompoundRule::new()),
        Arc::new(simple_replace::SimpleReplaceRule::wikipedia()),
        Arc::new(replace_verbs::SimpleReplaceVerbsRule::new()),
        Arc::new(word_repeat::SpanishWordRepeatRule::new()),
        Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
    ];
    let text_level_rules: Vec<Arc<dyn TextLevelRule>> = vec![
        Arc::new(question_mark::QuestionMarkRule::new()),
        Arc::new(unpaired_brackets::SpanishUnpairedBracketsRule::new()),
        Arc::new(word_repeat_beginning::SpanishWordRepeatBeginningRule::new()),
        Arc::new(repeated_words::SpanishRepeatedWordsRule::new()),
        Arc::new(og_rules::text_level_rules::LongSentenceRule::new(60)),
        Arc::new(og_rules::text_level_rules::LongParagraphRule::new(220)),
    ];
    (sentence_rules, text_level_rules)
}

pub fn spanish() -> crate::engine::LanguageEngine {
    let mut engine =
        crate::engine::LanguageEngine::for_code("es").expect("es is a registered generic language");
    let (sentence_rules, text_level_rules) = native_rules();
    engine.add_rules(sentence_rules);
    engine.add_text_level_rules(text_level_rules);
    engine
}

pub fn build_es_disambiguator(
    resource_dir: &std::path::Path,
) -> Option<std::sync::Arc<dyn og_core::checker::Disambiguator>> {
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).ok()?;
    let expanded =
        og_xml::parser::XmlRuleParser::expand_entities_with_base(&xml, Some(resource_dir));
    let xml_disambiguator =
        std::sync::Arc::new(og_tagger::XmlDisambiguator::from_xml(&expanded).ok()?);
    let core = crate::engine::find_lt_core_resource_dir()?;
    let global = og_tagger::MultiWordChunkerDisambiguator::from_params(
        &core.join("spelling_global.txt"),
        og_tagger::ChunkerParams {
            allow_first_capitalized: false,
            allow_all_uppercase: true,
            allow_titlecase: false,
            default_tag: Some("NPCN000"),
            remove_previous_tags: false,
            add_ignore_spelling: false,
        },
    )?;
    let multiwords = og_tagger::MultiWordChunkerDisambiguator::from_params(
        &resource_dir.join("multiwords.txt"),
        og_tagger::ChunkerParams {
            allow_first_capitalized: true,
            allow_all_uppercase: true,
            allow_titlecase: false,
            default_tag: None,
            remove_previous_tags: true,
            add_ignore_spelling: false,
        },
    )?;
    let cut = xml_disambiguator
        .first_index_of_rule_id("PERIFRASIS_VERBALES")
        .unwrap_or(xml_disambiguator.rule_count());
    let global_xml = std::fs::read_to_string(core.join("disambiguation-global.xml")).ok()?;
    let global_expanded =
        og_xml::parser::XmlRuleParser::expand_entities_with_base(&global_xml, Some(&core));
    let global_disambiguator = og_tagger::XmlDisambiguator::from_xml(&global_expanded).ok()?;
    Some(std::sync::Arc::new(EsDisambiguatorChain {
        global: std::sync::Arc::new(global),
        multiwords: std::sync::Arc::new(multiwords),
        xml: xml_disambiguator,
        xml_cut: cut,
        global_xml: std::sync::Arc::new(global_disambiguator),
    }))
}

fn append_gv_chunks(
    xml: &og_tagger::XmlDisambiguator,
    sentence: &mut og_core::AnalyzedSentence,
    from_rule: usize,
) {
    let mut gv_tokens: Vec<usize> = Vec::new();
    for rule_idx in from_rule..xml.rule_count() {
        for positions in xml.debug_match_rule(sentence, rule_idx) {
            for p in positions {
                if !gv_tokens.contains(&p) {
                    gv_tokens.push(p);
                }
            }
        }
    }
    if gv_tokens.is_empty() {
        return;
    }
    gv_tokens.sort_unstable();
    let mut nonws_seen = 0usize;
    for token in sentence.tokens_mut().iter_mut() {
        if token.is_whitespace() {
            continue;
        }
        if gv_tokens.binary_search(&nonws_seen).is_ok() && !token.has_chunk("GV") {
            let mut chunks = token.chunks().to_vec();
            chunks.push("GV".to_string());
            token.set_chunks(chunks);
        }
        nonws_seen += 1;
    }
}

struct EsDisambiguatorChain {
    global: std::sync::Arc<og_tagger::MultiWordChunkerDisambiguator>,
    multiwords: std::sync::Arc<og_tagger::MultiWordChunkerDisambiguator>,
    xml: std::sync::Arc<og_tagger::XmlDisambiguator>,
    xml_cut: usize,
    global_xml: std::sync::Arc<og_tagger::XmlDisambiguator>,
}

impl og_core::checker::Disambiguator for EsDisambiguatorChain {
    fn disambiguate(&self, sentence: &mut og_core::AnalyzedSentence) {
        self.global.disambiguate(sentence);
        self.multiwords.disambiguate(sentence);
        self.xml.apply_rules_through(sentence, self.xml_cut);
        append_gv_chunks(&self.xml, sentence, self.xml_cut);
        self.global_xml.disambiguate(sentence);
    }
}

#[cfg(test)]
pub(crate) mod example_suite;

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use og_core::checker::Disambiguator;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
    use og_tokenizer::WordTokenizer as _;
    use std::sync::Arc;

    pub struct SpanishPipeline {
        tokenizer: word_tokenizer::SpanishWordTokenizer,
        tagger: Arc<tagger::SpanishTagger>,
        disambiguator: Option<Arc<dyn Disambiguator>>,
        splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
    }

    impl SpanishPipeline {
        pub fn shared() -> Arc<SpanishPipeline> {
            static PIPE: og_core::releasable::Releasable<SpanishPipeline> =
                og_core::releasable::Releasable::new();
            og_core::releasable::shared_get_or_build(&PIPE, || {
                let resource_dir =
                    es_resource_dir().expect("es resource dir with spanish.dict present");
                let tagger = Arc::new(tagger::SpanishTagger::from_resource_dir(&resource_dir));
                let disambiguator = build_es_disambiguator(&resource_dir);
                Some(Arc::new(SpanishPipeline {
                    tokenizer: word_tokenizer::SpanishWordTokenizer::new(),
                    tagger,
                    disambiguator,
                    splitter: og_tokenizer::srx::srx_splitter("es"),
                }))
            })
            .expect("es pipeline")
        }

        pub fn tagger_ref(&self) -> &tagger::SpanishTagger {
            &self.tagger
        }

        pub fn sentence_ranges(&self, text: &str) -> Vec<(usize, usize)> {
            use og_tokenizer::SentenceTokenizer as _;
            match &self.splitter {
                Some(srx) => srx
                    .split(text)
                    .iter()
                    .map(|s| (s.start(), s.end()))
                    .collect(),
                None => vec![(0, text.len())],
            }
        }

        pub fn analyze_range(&self, text: &str, start: usize, end: usize) -> AnalyzedSentence {
            let sentence_text = &text[start..end];
            let mut sentence = AnalyzedSentence::new(sentence_text, 0, sentence_text.len());
            let mut tokens = Vec::new();
            let start_token = AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            );
            tokens.push(start_token);
            tokens.extend(self.tokenizer.tokenize(sentence_text).into_iter().map(|t| {
                AnalyzedTokenReadings::new(AnalyzedToken::new(t.text(), t.start(), t.end()))
            }));
            sentence.set_tokens(tokens);
            let token_texts: Vec<String> = sentence
                .tokens()
                .iter()
                .skip(1)
                .map(|t| t.token().token().to_string())
                .collect();
            let refs: Vec<&str> = token_texts.iter().map(|s| s.as_str()).collect();
            let tagged = og_tagger::Tagger::tag(self.tagger.as_ref(), &refs);
            let tokens_mut = sentence.tokens_mut();
            for (i, atr) in tagged.into_iter().enumerate() {
                let token_idx = i + 1;
                if token_idx < tokens_mut.len() && !atr.readings().is_empty() {
                    let chunks = atr.chunks().to_vec();
                    let readings = atr.into_readings();
                    let all_tags: Vec<String> = readings
                        .iter()
                        .flat_map(|r| r.pos_tags().to_vec())
                        .collect();
                    let lemma = readings[0]
                        .lemma()
                        .unwrap_or(tokens_mut[token_idx].token().token())
                        .to_string();
                    let mut merged = AnalyzedTokenReadings::new(
                        AnalyzedToken::new(
                            tokens_mut[token_idx].token().token(),
                            tokens_mut[token_idx].token().start(),
                            tokens_mut[token_idx].token().end(),
                        )
                        .with_pos_tags(all_tags)
                        .with_lemma(lemma),
                    )
                    .with_readings(readings);
                    if !chunks.is_empty() {
                        merged.set_chunks(chunks);
                    }
                    tokens_mut[token_idx] = merged;
                }
            }
            let tokens = sentence.tokens_mut();
            for token in tokens.iter_mut().rev() {
                if !token.is_whitespace()
                    && !token.token().pos_tags().contains(&"SENT_START".to_string())
                {
                    if !token.has_pos_tag("SENT_END") {
                        token.add_sent_end();
                    }
                    break;
                }
            }
            if let Some(disambiguator) = &self.disambiguator {
                disambiguator.disambiguate(&mut sentence);
            }
            sentence
        }

        pub fn analyze_text(&self, text: &str) -> Vec<AnalyzedSentence> {
            self.sentence_ranges(text)
                .into_iter()
                .map(|(s, e)| self.analyze_range(text, s, e))
                .collect()
        }

        pub fn analyze_offset_text(&self, text: &str) -> Vec<(AnalyzedSentence, usize)> {
            self.sentence_ranges(text)
                .into_iter()
                .map(|(s, e)| (self.analyze_range(text, s, e), s))
                .collect()
        }
    }

    pub fn analyze_spanish(text: &str) -> Vec<AnalyzedSentence> {
        SpanishPipeline::shared().analyze_text(text)
    }

    pub fn analyze_spanish_sentences(text: &str) -> Vec<AnalyzedSentence> {
        SpanishPipeline::shared().analyze_text(text)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::SpanishPipeline;

    #[test]
    fn srx_rules_resolve_for_spanish() {
        let splitter = og_tokenizer::srx::srx_splitter("es");
        assert!(splitter.is_some(), "es SRX rules must resolve");
        use og_tokenizer::SentenceTokenizer as _;
        let splitter = splitter.unwrap();
        let sentences = splitter.split("Veni. Adiós.");
        assert_eq!(sentences.len(), 2);
        let sentences = splitter.split("1. El título sigue aquí.");
        assert_eq!(
            sentences.len(),
            1,
            "es 'Punto primero' rule keeps enumerations whole"
        );
    }

    #[test]
    fn tagger_pins_java_spanish_tagger_test() {
        let pipeline = SpanishPipeline::shared();
        let cases: &[(&str, &str)] = &[
            ("Soy", "ser VSIP1S0"),
            ("CASA", "casa NCFS000"),
            ("superincómodas", "superincómodo AQ0FP0"),
            ("autoeducan", "autoeducar VMIP3P0"),
            ("autorretratarán", "autorretratar VMIF3P0"),
            ("económico-sociales", "económico-social AQ0CP0"),
        ];
        for (word, expected) in cases {
            let tagged = og_tagger::Tagger::tag(pipeline.tagger_ref(), &[word]);
            let actual = tagged
                .first()
                .and_then(|atr| atr.readings().first())
                .map(|r| {
                    format!(
                        "{} {}",
                        r.lemma().unwrap_or("null"),
                        r.pos_tags().first().map(String::as_str).unwrap_or("null")
                    )
                })
                .unwrap_or_else(|| "null null".to_string());
            assert_eq!(&actual, expected, "tagging {word}");
        }
    }

    #[test]
    fn tagger_pins_java_untagged_and_case_behavior() {
        let pipeline = SpanishPipeline::shared();
        let tagged = og_tagger::Tagger::tag(pipeline.tagger_ref(), &["blablabla"]);
        assert!(
            tagged[0].readings().is_empty()
                || tagged[0].readings().iter().all(|r| r.pos_tags().is_empty()),
            "blablabla must stay untagged"
        );
        let tagged = og_tagger::Tagger::tag(pipeline.tagger_ref(), &["autoralentizan"]);
        assert!(
            tagged[0].readings().is_empty(),
            "autoralentizan has no valid prefix analysis (Java: [null]null)"
        );
        let tagged = og_tagger::Tagger::tag(pipeline.tagger_ref(), &["CaSa"]);
        assert!(
            tagged[0].readings().is_empty(),
            "mixed-case words get no lowercase fallback (SpanishTagger.tag isMixedCase guard)"
        );
    }

    #[test]
    fn tagger_emits_current_colon_scheme_tags() {
        let pipeline = SpanishPipeline::shared();
        let tagged = og_tagger::Tagger::tag(pipeline.tagger_ref(), &["dímelo", "del"]);
        let dimelo_tags: Vec<String> = tagged[0]
            .readings()
            .iter()
            .flat_map(|r| r.pos_tags().to_vec())
            .collect();
        assert!(
            dimelo_tags
                .iter()
                .any(|t| t.starts_with("VMM02S0:PP1CS") && t.ends_with("PP3MSA00")),
            "Java pin SpanishTaggerTest dímelo/[decir]VMM02S0:PP1CS000:PP3MSA00 (chain joined by ':'), got {dimelo_tags:?}"
        );
        let del_tags: Vec<String> = tagged[1]
            .readings()
            .iter()
            .flat_map(|r| r.pos_tags().to_vec())
            .collect();
        assert!(
            del_tags.iter().any(|t| t == "SP:DA"),
            "current grammar.xml expects SP:DA for 'del', got {del_tags:?}"
        );
    }
}
