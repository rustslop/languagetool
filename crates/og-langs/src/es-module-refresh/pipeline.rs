use std::path::PathBuf;
use std::sync::Arc;

use og_core::AnalyzedSentence;
use og_core::AnalyzedToken;
use og_core::AnalyzedTokenReadings;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::CompiledRule;
use og_xml::compiler::CompiledRuleSet;
use og_xml::compiler::XmlCompiler;

pub(crate) fn grammar_path() -> Option<PathBuf> {
    crate::es::es_rules_dir().map(|dir| dir.join("grammar.xml"))
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
    .expect("es refresh grammar")
}

pub(crate) fn compile_fragment(xml: &str) -> Vec<CompiledRule> {
    XmlCompiler::new()
        .compile_file(xml)
        .expect("compile es refresh fragment")
        .rules
}

pub(crate) fn celebres_group(rule_set: &CompiledRuleSet) -> Vec<&CompiledRule> {
    rule_set
        .rules
        .iter()
        .filter(|r| r.id == "CELEBRE")
        .collect()
}

pub(crate) fn celebres_quote_sub_rule(rule_set: &CompiledRuleSet) -> &CompiledRule {
    rule_set
        .rules
        .iter()
        .find(|r| {
            r.id == "CELEBRE"
                && r.pattern
                    .tokens
                    .iter()
                    .any(|t| t.space_before.as_deref() == Some("yes") && t.is_regexp)
        })
        .expect("the refreshed CELEBRE quote sub-rule")
}

pub(crate) fn es_synth() -> Option<Arc<og_rules::postponed_adjective::LangSynthesizer>> {
    static CACHE: og_core::releasable::Releasable<og_rules::postponed_adjective::LangSynthesizer> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        crate::es::es_resource_dir()
            .and_then(|dir| {
                og_rules::postponed_adjective::LangSynthesizer::for_language(&dir, "es")
            })
            .map(Arc::new)
    })
}

pub(crate) fn pattern_engine() -> Arc<PatternRuleEngine> {
    static ENGINE: og_core::releasable::Releasable<PatternRuleEngine> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&ENGINE, || {
        let resource_dir = crate::es::es_resource_dir()?;
        let core_resource_dir = crate::engine::find_lt_core_resource_dir()?;
        let mut dict = og_spell::Dictionary::new();
        for name in ["hunspell/spelling.txt", "added.txt"] {
            if let Ok(words) = std::fs::read_to_string(resource_dir.join(name)) {
                for line in words.lines() {
                    let word = line.trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        dict.add_word(word);
                    }
                }
            }
        }
        let support = og_rules::spelling_support::SpellingSupport::new_for_language(
            Arc::new(dict),
            &core_resource_dir,
            &resource_dir,
            "es",
        );
        let synth = es_synth();
        let mut engine = PatternRuleEngine::new().with_spelling_support(Arc::new(support));
        if let Some(synth) = synth {
            engine = engine
                .with_synthesizer(synth.clone())
                .with_suggestion_synthesizer(synth);
        }
        Some(Arc::new(engine))
    })
    .expect("es refresh pattern engine")
}

pub(crate) struct RefreshPipeline {
    tokenizer: crate::es::word_tokenizer::SpanishWordTokenizer,
    tagger: Arc<crate::es::tagger::SpanishTagger>,
    disambiguator: Option<Arc<dyn og_core::checker::Disambiguator>>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl RefreshPipeline {
    pub(crate) fn shared() -> Arc<RefreshPipeline> {
        static PIPE: og_core::releasable::Releasable<RefreshPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir =
                crate::es::es_resource_dir().expect("es resource dir with spanish.dict present");
            let tagger = Arc::new(crate::es::tagger::SpanishTagger::from_resource_dir(
                &resource_dir,
            ));
            let disambiguator = crate::es::build_es_disambiguator(&resource_dir);
            Some(Arc::new(RefreshPipeline {
                tokenizer: crate::es::word_tokenizer::SpanishWordTokenizer::new(),
                tagger,
                disambiguator,
                splitter: og_tokenizer::srx::srx_splitter("es"),
            }))
        })
        .expect("es refresh pipeline")
    }

    pub(crate) fn analyze_range(&self, text: &str, start: usize, end: usize) -> AnalyzedSentence {
        use og_tokenizer::WordTokenizer as _;
        let sentence_text = &text[start..end];
        let mut sentence = AnalyzedSentence::new(sentence_text, 0, sentence_text.len());
        let mut tokens = Vec::new();
        let start_token = AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        );
        tokens.push(start_token);
        tokens.extend(
            self.tokenizer.tokenize(sentence_text).into_iter().map(|t| {
                AnalyzedTokenReadings::new(AnalyzedToken::new(t.text(), t.start(), t.end()))
            }),
        );
        sentence.set_tokens(tokens);
        let token_texts: Vec<String> = sentence
            .tokens()
            .iter()
            .skip(1)
            .map(|t| t.token().token().to_string())
            .collect();
        let refs: Vec<&str> = token_texts.iter().map(String::as_str).collect();
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
        if self.disambiguator.is_some() {
            sentence.set_pre_disambiguation_tokens(sentence.tokens().to_vec());
        }
        if let Some(disambiguator) = &self.disambiguator {
            disambiguator.disambiguate(&mut sentence);
        }
        sentence
    }

    pub(crate) fn analyze_offset_text(&self, text: &str) -> Vec<(AnalyzedSentence, usize)> {
        use og_tokenizer::SentenceTokenizer as _;
        match &self.splitter {
            Some(srx) => srx
                .split(text)
                .into_iter()
                .map(|s| (self.analyze_range(text, s.start(), s.end()), s.start()))
                .collect(),
            None => vec![(self.analyze_range(text, 0, text.len()), 0)],
        }
    }
}

pub(crate) fn strip_marker(text: &str) -> (String, Option<(usize, usize)>) {
    match (text.find("<marker>"), text.find("</marker>")) {
        (Some(s), Some(e)) => {
            let clean = text.replace("<marker>", "").replace("</marker>", "");
            (clean, Some((s, e - "<marker>".len())))
        }
        _ => (text.to_string(), None),
    }
}
