use std::path::PathBuf;
use std::sync::Arc;

use og_core::AnalyzedSentence;
use og_core::AnalyzedToken;
use og_core::AnalyzedTokenReadings;
use og_rules::pattern_rule::PatternRuleEngine;
use og_tokenizer::SentenceTokenizer as _;
use og_tokenizer::WordTokenizer as _;
use og_xml::compiler::CompiledRuleSet;
use og_xml::compiler::XmlCompiler;

pub(crate) fn grammar_path() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir().map(|root| crate::engine::grammar_xml_path("en", &root))
}

pub(crate) fn grammar_available() -> bool {
    grammar_path().is_some_and(|path| path.exists())
}

pub(crate) fn compiled_rules() -> Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        let path = grammar_path()?;
        let xml = std::fs::read_to_string(&path).ok()?;
        Some(Arc::new(
            XmlCompiler::new()
                .compile_file_with_base(&xml, path.parent())
                .ok()?,
        ))
    })
    .expect("en refresh grammar")
}

pub(crate) fn pattern_engine() -> Arc<PatternRuleEngine> {
    static ENGINE: og_core::releasable::Releasable<PatternRuleEngine> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&ENGINE, || {
        let resource_dir = crate::engine::find_lt_resource_dir()?
            .join("en/src/main/resources/org/languagetool/resource/en");
        let core_resource_dir = crate::engine::find_lt_core_resource_dir()?;
        let support = og_rules::spelling_support::SpellingSupport::dictionary_from_dir(
            &resource_dir,
        )
        .map(|dict| {
            og_rules::spelling_support::SpellingSupport::new_for_language(
                Arc::new(dict),
                &core_resource_dir,
                &resource_dir,
                "en",
            )
        });
        let mut engine = match support {
            Some(support) => PatternRuleEngine::new().with_spelling_support(Arc::new(support)),
            None => PatternRuleEngine::new(),
        };
        if let Some(synth) =
            og_rules::postponed_adjective::LangSynthesizer::for_language(&resource_dir, "en")
        {
            engine = engine.with_synthesizer(Arc::new(synth));
        }
        if let Some(synth) =
            og_rules::postponed_adjective::LangSynthesizer::for_language(&resource_dir, "en")
        {
            engine = engine.with_suggestion_synthesizer(Arc::new(synth));
        }
        Some(Arc::new(engine))
    })
    .expect("en refresh pattern engine")
}

pub(crate) struct EnRefreshPipeline {
    tokenizer: Arc<og_tokenizer::DefaultWordTokenizer>,
    tagger: Arc<og_tagger::EnglishTagger>,
    disambiguator: Option<Arc<dyn og_core::checker::Disambiguator>>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl EnRefreshPipeline {
    pub(crate) fn shared() -> Arc<EnRefreshPipeline> {
        static PIPE: og_core::releasable::Releasable<EnRefreshPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir = crate::engine::find_lt_resource_dir()?
                .join("en/src/main/resources/org/languagetool/resource/en");
            let core_resource_dir = crate::engine::find_lt_core_resource_dir()?;
            let tagger = og_tagger::english_tagger::shared_english_tagger()?;
            og_tagger::chunker::load_opennlp_chunker(&resource_dir);
            let disambiguator = std::fs::read_to_string(resource_dir.join("disambiguation.xml"))
                .ok()
                .and_then(|xml| {
                    og_tagger::XmlDisambiguator::from_xml_with_english(
                        &xml,
                        og_tagger::english_tagger::shared_english_tagger(),
                    )
                    .ok()
                })
                .map(|disambiguator| {
                    og_tagger::hybrid_disambiguator(
                        "en",
                        &resource_dir,
                        &core_resource_dir,
                        disambiguator,
                    )
                });
            let tokenizer = Arc::new(og_tokenizer::DefaultWordTokenizer::new_english(Arc::new({
                let tagger = tagger.clone();
                move |word: &str| tagger.is_word_tagged(word)
            })));
            Some(Arc::new(EnRefreshPipeline {
                tokenizer,
                tagger,
                disambiguator,
                splitter: og_tokenizer::srx::srx_splitter("en"),
            }))
        })
        .expect("en refresh pipeline")
    }

    pub(crate) fn analyze_sentence(&self, text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens: Vec<AnalyzedTokenReadings> = self
            .tokenizer
            .split_underscores(self.tokenizer.tokenize(text))
            .into_iter()
            .map(|token| {
                AnalyzedTokenReadings::new(AnalyzedToken::new(
                    token.text(),
                    token.start(),
                    token.end(),
                ))
            })
            .collect();
        tokens.insert(
            0,
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
        );

        let token_texts: Vec<String> = tokens
            .iter()
            .skip(1)
            .map(|token| token.token().token().to_string())
            .collect();
        let refs: Vec<&str> = token_texts.iter().map(String::as_str).collect();
        let tagged = og_tagger::Tagger::tag(self.tagger.as_ref(), &refs);
        for (i, tagged_atr) in tagged.into_iter().enumerate() {
            let token_idx = i + 1;
            if token_idx >= tokens.len() {
                continue;
            }
            let start = tokens[token_idx].token().start();
            let end = tokens[token_idx].token().end();
            if tagged_atr.readings().is_empty() {
                let primary = AnalyzedToken::new(tagged_atr.token().token(), start, end);
                let mut next = AnalyzedTokenReadings::new(primary);
                if !tagged_atr.chunks().is_empty() {
                    next.set_chunks(tagged_atr.chunks().to_vec());
                }
                tokens[token_idx] = next;
                continue;
            }
            let tagged_chunks = tagged_atr.chunks().to_vec();
            let surface = tagged_atr.token().token().to_string();
            let readings = tagged_atr.into_readings();
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|reading| reading.pos_tags().to_vec())
                .collect();
            let lemma = readings[0]
                .lemma()
                .map(str::to_string)
                .unwrap_or_else(|| surface.clone());
            let updated_primary = AnalyzedToken::new(surface, start, end)
                .with_pos_tags(all_tags)
                .with_lemma(lemma);
            let mut atr = AnalyzedTokenReadings::new(updated_primary).with_readings(readings);
            if !tagged_chunks.is_empty() {
                atr.set_chunks(tagged_chunks);
            }
            tokens[token_idx] = atr;
        }
        sentence.set_tokens(tokens);

        let tokens = sentence.tokens_mut();
        for i in (0..tokens.len()).rev() {
            if !tokens[i].is_whitespace()
                && !tokens[i]
                    .token()
                    .pos_tags()
                    .contains(&"SENT_START".to_string())
            {
                if !tokens[i].has_pos_tag("SENT_END") {
                    tokens[i].add_sent_end();
                }
                break;
            }
        }

        og_tagger::chunker::apply_chunk_tags(&mut sentence);

        if self.disambiguator.is_some() {
            sentence.set_pre_disambiguation_tokens(sentence.tokens().to_vec());
        }
        if let Some(disambiguator) = &self.disambiguator {
            disambiguator.disambiguate(&mut sentence);
        }
        sentence
    }

    pub(crate) fn analyze_offset_text(&self, text: &str) -> Vec<(AnalyzedSentence, usize)> {
        let Some(splitter) = &self.splitter else {
            return vec![(self.analyze_sentence(text), 0)];
        };
        splitter
            .split(text)
            .into_iter()
            .map(|span| (self.analyze_sentence(span.text()), span.start()))
            .collect()
    }
}

pub(crate) fn strip_marker(text: &str) -> (String, Option<(usize, usize)>) {
    match (text.find("<marker>"), text.find("</marker>")) {
        (Some(start), Some(end)) => {
            let clean = text.replace("<marker>", "").replace("</marker>", "");
            (clean, Some((start, end - "<marker>".len())))
        }
        _ => (text.to_string(), None),
    }
}
