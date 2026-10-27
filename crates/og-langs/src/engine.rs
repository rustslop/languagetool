use og_core::checker::{SentenceTokenizer, WordTokenizer};
use og_core::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, Category, CheckRequest, CheckResult,
    Checker, IssueType, Language, RuleMatch, SentenceRange,
    rule::{Rule, TextLevelRule},
};
use og_rules::pattern_rule::PatternRuleEngine;
use og_spell::SpellingCheckRule;
use og_tagger::{
    ChainedDisambiguator, EnglishTagger, MultiWordChunkerDisambiguator, XmlDisambiguator,
};
use og_xml::compiler::{CompiledRule, XmlCompiler};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct XmlPatternRule {
    compiled: CompiledRule,
    engine: PatternRuleEngine,
    category: Category,
}

impl XmlPatternRule {
    pub fn new(compiled: CompiledRule) -> Self {
        Self::with_spelling(compiled, None)
    }

    pub fn with_spelling(
        compiled: CompiledRule,
        spelling: Option<Arc<og_rules::spelling_support::SpellingSupport>>,
    ) -> Self {
        Self::with_resources(compiled, spelling, None, None)
    }

    pub fn with_resources(
        compiled: CompiledRule,
        spelling: Option<Arc<og_rules::spelling_support::SpellingSupport>>,
        synthesizer: Option<Arc<og_rules::postponed_adjective::LangSynthesizer>>,
        suggestion_synth: Option<Arc<og_rules::postponed_adjective::LangSynthesizer>>,
    ) -> Self {
        let category = Category::new(&compiled.category.id, &compiled.category.name);
        let mut engine = PatternRuleEngine::new();
        if let Some(spelling) = spelling {
            engine = engine.with_spelling_support(spelling);
        }
        if let Some(synthesizer) = synthesizer {
            engine = engine.with_synthesizer(synthesizer);
        }
        if let Some(suggestion_synth) = suggestion_synth {
            engine = engine.with_suggestion_synthesizer(suggestion_synth);
        }
        Self {
            compiled,
            engine,
            category,
        }
    }
}

impl Rule for XmlPatternRule {
    fn id(&self) -> &str {
        &self.compiled.id
    }

    #[allow(clippy::misnamed_getters)]
    fn description(&self) -> &str {
        &self.compiled.name
    }

    fn is_default_on(&self) -> bool {
        self.compiled.default_on
    }

    fn is_picky(&self) -> bool {
        self.compiled.picky
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        self.engine.match_rule(&self.compiled, sentence)
    }
}

pub struct LanguageEngine {
    language: Language,
    rules: Vec<Arc<dyn Rule>>,
    text_level_rules: Vec<Arc<dyn TextLevelRule>>,
    tagger: Option<Arc<dyn og_core::checker::Tagger>>,
    disambiguator: Option<Arc<dyn og_core::checker::Disambiguator>>,
    post_chunker: Option<Arc<dyn og_core::checker::Chunker>>,
    spelling_support: Option<Arc<og_rules::spelling_support::SpellingSupport>>,
    german_speller: Option<Arc<og_spell::GermanSpeller>>,
    synthesizer: Option<Arc<og_rules::postponed_adjective::LangSynthesizer>>,
    suggestion_synth: Option<Arc<og_rules::postponed_adjective::LangSynthesizer>>,
    checker: Checker,
    language_filter: Arc<dyn og_core::match_filters::LanguageMatchFilter>,
    l2_rules: std::sync::Mutex<HashMap<String, Vec<Arc<dyn Rule>>>>,
    l2_checkers: std::sync::Mutex<HashMap<String, Checker>>,
}

impl LanguageEngine {
    pub fn new(language: Language) -> Self {
        let checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(SentenceSplitterBridge(
                language.short_code().to_string(),
            )))
            .with_word_tokenizer(Arc::new(WordTokenizerBridge::for_code(
                language.short_code(),
            )));

        let language_filter = crate::match_priority::for_code(language.short_code());
        Self {
            language,
            rules: Vec::new(),
            text_level_rules: Vec::new(),
            tagger: None,
            disambiguator: None,
            post_chunker: None,
            spelling_support: None,
            german_speller: None,
            synthesizer: None,
            suggestion_synth: None,
            checker,
            language_filter,
            l2_rules: std::sync::Mutex::new(HashMap::new()),
            l2_checkers: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn shared_for_code(code: &str) -> Option<Arc<Self>> {
        static CELLS: std::sync::OnceLock<
            std::sync::Mutex<
                HashMap<String, &'static og_core::releasable::Releasable<LanguageEngine>>,
            >,
        > = std::sync::OnceLock::new();
        let cell = {
            let cells = CELLS.get_or_init(Default::default);
            let mut guard = cells.lock().unwrap();
            *guard
                .entry(code.to_string())
                .or_insert_with(|| Box::leak(Box::new(og_core::releasable::Releasable::new())))
        };
        if let Some(hit) = og_core::releasable::shared_probe(cell) {
            return Some(hit);
        }
        let _heavy = og_core::releasable::heavy_guard();
        og_core::releasable::shared_get_or_build(cell, || {
            trim_released_engine_pages();
            match code {
                "en" => Some(Self::english()),
                "de" => Some(Self::german()),
                other => Self::for_code_inner(other),
            }
            .map(Arc::new)
        })
    }

    pub fn english() -> Self {
        let tagger = match find_lt_resource_dir() {
            Some(lt_root) => {
                let resource_dir =
                    lt_root.join("en/src/main/resources/org/languagetool/resource/en");
                if resource_dir.exists() {
                    EnglishTagger::from_resource_dir(&resource_dir)
                } else {
                    EnglishTagger::new()
                }
            }
            None => EnglishTagger::new(),
        };

        let tagger_arc: Arc<dyn og_core::checker::Tagger> = Arc::new(tagger);

        let mut engine = Self {
            language: Language::English,
            rules: Vec::new(),
            text_level_rules: Vec::new(),
            tagger: Some(tagger_arc),
            disambiguator: None,
            post_chunker: None,
            spelling_support: None,
            german_speller: None,
            synthesizer: None,
            suggestion_synth: None,
            checker: Checker::new()
                .with_sentence_tokenizer(Arc::new(SentenceSplitterBridge("en".to_string())))
                .with_word_tokenizer(Arc::new(WordTokenizerBridge::Default(true))),
            language_filter: crate::match_priority::for_code("en"),
            l2_rules: std::sync::Mutex::new(HashMap::new()),
            l2_checkers: std::sync::Mutex::new(HashMap::new()),
        };

        let mut spelling_support: Option<Arc<og_rules::spelling_support::SpellingSupport>> = None;
        if let Some(lt_root) = find_lt_resource_dir() {
            let resource_dir = lt_root.join("en/src/main/resources/org/languagetool/resource/en");
            og_tagger::chunker::load_opennlp_chunker(&resource_dir);
            if let Some(synth) =
                og_rules::postponed_adjective::LangSynthesizer::for_language(&resource_dir, "en")
            {
                engine.suggestion_synth = Some(Arc::new(synth));
            }
            if let Some(support) = find_lt_core_resource_dir().and_then(|core_resource_dir| {
                og_rules::spelling_support::SpellingSupport::dictionary_from_dir(&resource_dir).map(
                    |dict| {
                        og_rules::spelling_support::SpellingSupport::new_for_language(
                            std::sync::Arc::new(dict),
                            &core_resource_dir,
                            &resource_dir,
                            "en",
                        )
                    },
                )
            }) {
                spelling_support = Some(Arc::new(support));
            }
            engine.spelling_support = spelling_support.clone();

            let rules_dir = lt_root.join("en/src/main/resources/org/languagetool/rules/en");
            let grammar_path = grammar_xml_path("en", &lt_root);
            if grammar_path.exists()
                && let Ok(count) = engine.load_xml_file(&grammar_path)
            {
                eprintln!("Loaded {} XML rules for English", count);
            }
            for file in ["style.xml", "grammar_custom.xml"] {
                let path = rules_dir.join(file);
                if !path.exists() {
                    continue;
                }
                match engine.load_xml_file(&path) {
                    Ok(count) => eprintln!("Loaded {} rules from {file}", count),
                    Err(e) => eprintln!("Warning: failed to load {file}: {e}"),
                }
            }
            for file in [
                "en-US/grammar.xml",
                "en-US/style.xml",
                "en-US/grammar-premium.xml",
            ] {
                let path = rules_dir.join(file);
                if !path.exists() {
                    continue;
                }
                match engine.load_xml_file(&path) {
                    Ok(count) => eprintln!("Loaded {} rules from {file}", count),
                    Err(e) => eprintln!("Warning: failed to load {file}: {e}"),
                }
            }

            let disambig_path = lt_root
                .join("en/src/main/resources/org/languagetool/resource/en/disambiguation.xml");
            if disambig_path.exists()
                && let Ok(xml) = std::fs::read_to_string(&disambig_path)
            {
                match XmlDisambiguator::from_xml_with_english(
                    &xml,
                    og_tagger::english_tagger::shared_english_tagger(),
                ) {
                    Ok(disambiguator) => {
                        eprintln!(
                            "Loaded {} disambiguation rules for English",
                            disambiguator.rule_count()
                        );
                        let mut chain: Vec<Arc<dyn og_core::checker::Disambiguator>> =
                            vec![Arc::new(disambiguator)];
                        if let Some(multiword) =
                            MultiWordChunkerDisambiguator::from_resource_dir(&resource_dir)
                        {
                            chain.insert(0, Arc::new(multiword));
                        }
                        if let Some(core_resource_dir) = find_lt_core_resource_dir()
                            && let Some(global) =
                                MultiWordChunkerDisambiguator::spelling_global(&core_resource_dir)
                        {
                            chain.insert(0, Arc::new(global));
                        }
                        engine.disambiguator = Some(Arc::new(ChainedDisambiguator::new(chain)));
                    }
                    Err(e) => eprintln!("Warning: Failed to load disambiguation rules: {}", e),
                }
            }

            let det_a_path =
                lt_root.join("en/src/main/resources/org/languagetool/rules/en/det_a.txt");
            let det_an_path =
                lt_root.join("en/src/main/resources/org/languagetool/rules/en/det_an.txt");
            let mut native_rules: Vec<Arc<dyn Rule>> = Vec::new();
            if det_a_path.exists()
                && det_an_path.exists()
                && let (Ok(det_a), Ok(det_an)) = (
                    std::fs::read_to_string(&det_a_path),
                    std::fs::read_to_string(&det_an_path),
                )
            {
                let mut avsan = crate::en::AvsAnRule::new();
                avsan.load_data(&det_a, &det_an);
                native_rules.push(Arc::new(avsan));
            }

            native_rules.push(Arc::new(crate::en::SimpleReplaceRule::english_default()));
            native_rules.push(Arc::new(
                crate::en::SimpleReplaceRule::english_redundancies(),
            ));
            native_rules.push(Arc::new(crate::en::SimpleReplaceRule::english_profanity()));
            native_rules.push(Arc::new(crate::en::SimpleReplaceRule::american_replace()));
            native_rules.push(Arc::new(crate::en::SimpleReplaceRule::british_replace()));
            native_rules.push(Arc::new(crate::en::SimpleReplaceRule::new_zealand_replace()));

            native_rules.push(Arc::new(
                og_rules::native_rules::DoublePunctuationRule::new(),
            ));
            native_rules.push(Arc::new(
                og_rules::native_rules::UppercaseSentenceStartRule::new(),
            ));
            native_rules.push(Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()));

            native_rules.push(Arc::new(crate::en::ContractionSpellingRule::new()));
            native_rules.push(Arc::new(crate::en::SpecificCaseRule::new()));
            native_rules.push(Arc::new(crate::en::DiacriticsRule::new()));
            native_rules.push(Arc::new(crate::en::PlainEnglishRule::new()));
            native_rules.push(Arc::new(crate::en::CompoundRule::new()));
            native_rules.push(Arc::new(crate::en::DashRule::new()));
            native_rules.push(Arc::new(crate::en::WrongWordInContextRule::new()));
            native_rules.push(Arc::new(crate::en::EnglishWordRepeatRule::new()));
            native_rules.push(Arc::new(
                crate::en::unit_conversion::UnitConversionRule::new(),
            ));

            let hunspell_dir =
                lt_root.join("en/src/main/resources/org/languagetool/resource/en/hunspell");

            let dict = variant_dictionary(&hunspell_dir, "US");
            eprintln!("Loaded {} words from Morfologik dictionary", dict.len());

            let mut ignore_words = HashSet::new();
            if let Ok(words) = std::fs::read_to_string(hunspell_dir.join("ignore.txt")) {
                for line in words.lines() {
                    let word = line.trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        ignore_words.insert(word.to_lowercase());
                    }
                }
            }

            if !dict.is_empty() {
                let spell_rule = SpellingCheckRule::new_shared(
                    Arc::new(dict),
                    "MORFOLOGIK_RULE_EN_US",
                    HashSet::new(),
                )
                .with_ignore_words(ignore_words);
                eprintln!("Loaded spellchecker with {} words", spell_rule.dict_size());
                native_rules.push(Arc::new(spell_rule));
            }
            engine.add_rules(native_rules);
        }

        let mut extra_text_level: Vec<Arc<dyn TextLevelRule>> = vec![Arc::new(
            crate::en::repeated_words::EnglishRepeatedWordsRule::new(),
        )];
        let text_level_rules: [Arc<dyn TextLevelRule>; 13] = [
            Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
            Arc::new(og_rules::text_level_rules::SentenceWhitespaceRule::new()),
            Arc::new(og_rules::text_level_rules::GenericUnpairedBracketsRule::new()),
            Arc::new(og_rules::text_level_rules::GenericUnpairedQuotesRule::english()),
            Arc::new(og_rules::text_level_rules::LongSentenceRule::new(40)),
            Arc::new(og_rules::text_level_rules::WordRepeatBeginningRule::new()),
            Arc::new(crate::en::ConsistentApostrophesRule::new()),
            Arc::new(crate::en::WordCoherencyRule::new()),
            Arc::new(og_rules::text_level_rules::WhiteSpaceBeforeParagraphEndRule::new()),
            Arc::new(og_rules::text_level_rules::EmptyLineRule::new()),
            Arc::new(crate::en::StyleFrequencyRule::adjective()),
            Arc::new(crate::en::StyleFrequencyRule::noun()),
            Arc::new(crate::en::StyleFrequencyRule::verb()),
        ];
        engine.add_text_level_rules(text_level_rules);
        engine.add_text_level_rules(extra_text_level.drain(..));
        engine.add_rule(Arc::new(
            og_rules::text_level_rules::WhiteSpaceAtBeginOfParagraphRule::new(),
        ));

        if let Some(model) = og_languagemodel::DiskLanguageModel::shared_for_language("en") {
            engine.add_rule(Arc::new(
                crate::upper_case_ngram::EnglishUpperCaseNgramRule::new(model.clone()),
            ));
            engine.add_rule(Arc::new(
                crate::confusion_probability::ConfusionRule::new_english(model.clone()),
            ));
            engine.add_rule(Arc::new(
                crate::en::ngram_probability::EnglishNgramProbabilityRule::new(model),
            ));
        }

        engine.rebuild_checker();
        engine
    }

    pub fn german() -> Self {
        let profile = std::env::var("OG_ENGINE_PROFILE").is_ok();
        let mut stage_ms: Vec<(&'static str, f64)> = Vec::new();
        let mut mark = std::time::Instant::now();
        let mut lap =
            |profile: bool, stage_ms: &mut Vec<(&'static str, f64)>, name: &'static str| {
                if profile {
                    stage_ms.push((name, mark.elapsed().as_secs_f64() * 1e3));
                }
                mark = std::time::Instant::now();
            };

        let mut engine = Self::new(Language::German);

        if let Some(lt_root) = find_lt_resource_dir() {
            let resource_dir = lt_root.join("de/src/main/resources/org/languagetool/resource/de");
            if let Some(synth) =
                og_rules::postponed_adjective::LangSynthesizer::for_language(&resource_dir, "de")
            {
                engine.suggestion_synth = Some(Arc::new(synth));
            }
        }
        lap(profile, &mut stage_ms, "synth");

        let mut shared_dict: Option<Arc<og_spell::Dictionary>> = None;
        let mut german_resource_dir: Option<PathBuf> = None;
        let mut german_tagger: Option<Arc<og_tagger::GermanTagger>> = None;
        let mut german_speller: Option<Arc<og_spell::GermanSpeller>> = None;
        if let Some(lt_root) = find_lt_resource_dir() {
            let resource_dir = lt_root.join("de/src/main/resources/org/languagetool/resource/de");
            german_resource_dir = Some(resource_dir.clone());
            if let Some(tagger) = og_tagger::GermanTagger::from_resource_dir(&resource_dir) {
                let tagger = Arc::new(tagger);
                german_tagger = Some(tagger.clone());
                engine.tagger = Some(tagger as Arc<dyn og_core::checker::Tagger>);
            }
            let disambig_path = resource_dir.join("disambiguation.xml");
            if disambig_path.exists()
                && let Ok(xml) = std::fs::read_to_string(&disambig_path)
                && let Ok(disambiguator) = XmlDisambiguator::from_xml(&xml)
            {
                engine.disambiguator = Some(Arc::new(disambiguator));
            }
            lap(profile, &mut stage_ms, "tagger+disambig");

            if let Some(dict_arc) = shared_german_speller_dictionary(&resource_dir) {
                shared_dict = Some(dict_arc.clone());
                let speller = shared_german_speller(
                    &dict_arc,
                    &resource_dir,
                    find_lt_core_resource_dir().as_deref(),
                    engine.tagger.clone(),
                );
                if let Some(core_resource_dir) = find_lt_core_resource_dir() {
                    engine.spelling_support = shared_german_spelling_support(
                        &dict_arc,
                        &core_resource_dir,
                        &resource_dir,
                        speller.as_ref(),
                    );
                }
                if let Some(speller) = speller {
                    german_speller = Some(speller.clone());
                    engine.german_speller = Some(speller.clone());
                    engine.add_rule(Arc::new(og_spell::GermanSpellerRule::new(
                        speller,
                        "MORFOLOGIK_RULE_DE_DE",
                    )));
                }
                engine.add_rule(Arc::new(crate::de::CompoundInfinitivRule::new(Some(
                    dict_arc,
                ))));
            }
            lap(profile, &mut stage_ms, "speller_dict+support+speller_rules");

            let rules_dir = lt_root.join("de/src/main/resources/org/languagetool/rules/de");
            for file in ["grammar.xml", "style.xml", "grammar_custom.xml"] {
                let path = rules_dir.join(file);
                if !path.exists() {
                    continue;
                }
                match engine.load_xml_file(&path) {
                    Ok(count) => eprintln!("Loaded {} rules from de/{file}", count),
                    Err(e) => eprintln!("Warning: failed to load de/{file}: {e}"),
                }
            }
        }
        lap(profile, &mut stage_ms, "xml_rule_loads");

        engine.add_rule(Arc::new(crate::de::SimpleReplaceRule::new()));
        engine.add_rule(Arc::new(
            og_rules::native_rules::UppercaseSentenceStartRule::new(),
        ));
        let synth_dictionary = german_resource_dir
            .as_deref()
            .and_then(og_tagger::german_tagger::shared_german_dictionary);
        engine.add_rule(Arc::new(crate::de::AgreementRule::new(
            engine.tagger.clone(),
            engine.disambiguator.clone(),
            shared_dict,
            synth_dictionary.clone(),
        )));
        engine.add_rule(Arc::new(crate::de::AgreementRule2::new(
            synth_dictionary.clone(),
        )));
        engine.add_rule(Arc::new(crate::de::CaseRule::new(
            german_tagger.clone(),
            engine.spelling_support.clone(),
        )));
        engine.add_rule(Arc::new(crate::de::GermanDashRule));
        engine.add_text_level_rule(Arc::new(crate::de::VerbAgreementRule::new(
            synth_dictionary.clone(),
        )));
        engine.add_rule(Arc::new(crate::de::SubjectVerbAgreementRule::new(
            german_tagger.clone(),
        )));
        engine.add_rule(Arc::new(crate::de::OldSpellingRule::new("DE")));
        engine.add_rule(Arc::new(crate::de::MissingVerbRule::new(
            engine.tagger.clone(),
        )));
        engine.add_rule(Arc::new(crate::de::GermanWordRepeatRule::new()));
        engine.add_rule(Arc::new(
            crate::de::MissingCommaRelativeClauseRule::in_front(),
        ));
        engine.add_rule(Arc::new(crate::de::MissingCommaRelativeClauseRule::behind()));
        engine.add_rule(Arc::new(crate::de::GermanWrongWordInContextRule));
        engine.add_rule(Arc::new(crate::de::WiederVsWiderRule));
        engine.add_rule(Arc::new(crate::de::RedundantModalOrAuxiliaryVerb));
        engine.add_rule(Arc::new(crate::de::GermanUnitConversionRule::new()));
        engine.add_text_level_rule(Arc::new(crate::de::GermanWordCoherencyRule));
        engine.add_text_level_rule(Arc::new(crate::de::CompoundCoherencyRule));
        engine.add_text_level_rule(Arc::new(crate::de::DuUpperLowerCaseRule));
        engine.add_text_level_rule(Arc::new(crate::de::SimilarNameRule));
        engine.add_text_level_rule(Arc::new(crate::de::GermanParagraphRepeatBeginningRule));
        engine.add_text_level_rule(Arc::new(crate::de::GermanRepeatedWordsRule));
        engine.add_text_level_rule(Arc::new(crate::de::GermanReadabilityRule::new(true)));
        engine.add_text_level_rule(Arc::new(crate::de::GermanReadabilityRule::new(false)));
        engine.add_text_level_rule(Arc::new(crate::de::GermanStyleRepeatedWordRule::new()));
        engine.add_text_level_rule(Arc::new(crate::de::StyleTooOftenUsedWordRule::verb()));
        engine.add_text_level_rule(Arc::new(crate::de::StyleTooOftenUsedWordRule::noun()));
        engine.add_text_level_rule(Arc::new(crate::de::StyleTooOftenUsedWordRule::adjective()));
        engine.add_text_level_rule(Arc::new(crate::de::StatisticSentenceStyleRule::new(
            crate::de::SentenceCondition::Passive,
        )));
        engine.add_text_level_rule(Arc::new(crate::de::StatisticSentenceStyleRule::new(
            crate::de::SentenceCondition::ModalVerb,
        )));
        engine.add_text_level_rule(Arc::new(crate::de::StatisticSentenceStyleRule::new(
            crate::de::SentenceCondition::Man,
        )));
        engine.add_text_level_rule(Arc::new(crate::de::StatisticSentenceStyleRule::new(
            crate::de::SentenceCondition::ConjunctionAtBegin,
        )));
        engine.add_text_level_rule(Arc::new(crate::de::StatisticWordStyleRule::new(
            crate::de::WordCondition::FillerWords,
        )));
        engine.add_text_level_rule(Arc::new(crate::de::StatisticWordStyleRule::new(
            crate::de::WordCondition::NonSignificantVerbs,
        )));
        engine.add_text_level_rule(Arc::new(crate::de::StatisticWordStyleRule::new(
            crate::de::WordCondition::UnnecessaryPhrases,
        )));

        engine.add_rule(Arc::new(crate::de::german_double_punctuation_rule()));
        engine.add_rule(Arc::new(
            og_rules::native_rules::CommaWhitespaceRule::new().with_domain_after_dot_exception(),
        ));
        engine.add_text_level_rule(Arc::new(
            og_rules::text_level_rules::MultipleWhitespaceRule::new(),
        ));
        engine.add_text_level_rule(Arc::new(
            og_rules::text_level_rules::SentenceWhitespaceRule::new(),
        ));
        engine.add_text_level_rule(Arc::new(
            og_rules::text_level_rules::GenericUnpairedBracketsRule::new(),
        ));
        engine.add_text_level_rule(Arc::new(
            og_rules::text_level_rules::GenericUnpairedQuotesRule::new()
                .with_id("DE_UNPAIRED_QUOTES")
                .with_quote_symbols(vec![
                    ("\u{201E}".to_string(), "\u{201C}".to_string()),
                    ("\u{00BB}".to_string(), "\u{00AB}".to_string()),
                    ("\u{00AB}".to_string(), "\u{00BB}".to_string()),
                    ("\"".to_string(), "\"".to_string()),
                    ("'".to_string(), "'".to_string()),
                    ("\u{201A}".to_string(), "\u{2018}".to_string()),
                    ("\u{203A}".to_string(), "\u{2039}".to_string()),
                    ("\u{2039}".to_string(), "\u{203A}".to_string()),
                ]),
        ));
        engine.add_text_level_rule(Arc::new(
            og_rules::text_level_rules::WhiteSpaceBeforeParagraphEndRule::new(),
        ));
        engine.add_rule(Arc::new(
            og_rules::text_level_rules::WhiteSpaceAtBeginOfParagraphRule::new(),
        ));
        engine.add_text_level_rule(Arc::new(og_rules::text_level_rules::EmptyLineRule::new()));
        engine.add_text_level_rule(Arc::new(crate::de::GermanLongParagraphRule::new()));
        engine.add_text_level_rule(Arc::new(
            crate::de::GermanPunctuationMarkAtParagraphEnd::new(),
        ));
        engine.add_text_level_rule(Arc::new(og_rules::text_level_rules::LongSentenceRule::new(
            40,
        )));
        engine.add_text_level_rule(Arc::new(crate::de::GermanWordRepeatBeginningRule::new()));
        engine.add_text_level_rule(Arc::new(crate::de::StyleRepeatedVeryShortSentences::new()));
        engine.add_text_level_rule(Arc::new(crate::de::StyleRepeatedSentenceBeginning::new()));

        engine.post_chunker = Some(og_tagger::german_chunker::shared_german_chunker());

        if let Some(model) = og_languagemodel::DiskLanguageModel::shared_for_language("de") {
            engine.add_rule(Arc::new(
                crate::upper_case_ngram::GermanUpperCaseNgramRule::new(model.clone()),
            ));
            engine.add_rule(Arc::new(
                crate::confusion_probability::ConfusionRule::new_german(model.clone()),
            ));
            engine.add_rule(Arc::new(crate::de::ProhibitedCompoundRule::new(
                model,
                german_speller.clone(),
            )));
        }

        lap(profile, &mut stage_ms, "native_rules");
        engine.rebuild_checker();
        lap(profile, &mut stage_ms, "rebuild_checker");
        if profile {
            let total: f64 = stage_ms.iter().map(|(_, ms)| *ms).sum();
            let stages: Vec<String> = stage_ms
                .iter()
                .map(|(name, ms)| format!("{name}={ms:.0}"))
                .collect();
            eprintln!("german() build total_ms={total:.0} {}", stages.join(" "));
        }
        engine
    }

    pub fn german_variant(&self, code: &str) -> Self {
        let variant_dir = match code {
            "de-AT" => "de-DE-AT",
            "de-CH" => "de-CH",
            _ => "de-DE-AT",
        };
        let mut rules: Vec<Arc<dyn Rule>> = self.rules.clone();
        if let Some(lt_root) = find_lt_resource_dir() {
            let path = lt_root.join(format!(
                "de/src/main/resources/org/languagetool/rules/de/{variant_dir}/grammar.xml"
            ));
            if path.exists() {
                let compiler = XmlCompiler::new();
                if let Ok(xml) = std::fs::read_to_string(&path)
                    && let Ok(rule_set) = compiler.compile_file(&xml)
                {
                    rules.extend(rule_set.rules.into_iter().map(|compiled| {
                        Arc::new(XmlPatternRule::with_spelling(
                            compiled,
                            self.spelling_support.clone(),
                        )) as Arc<dyn Rule>
                    }));
                }
            }
        }
        let country = code.rsplit('-').next().unwrap_or("DE").to_string();
        rules.retain(|r| r.id() != "OLD_SPELLING_RULE");
        rules.push(Arc::new(crate::de::OldSpellingRule::new(&country)));
        let base_resource_dir = find_lt_resource_dir()
            .map(|root| root.join("de/src/main/resources/org/languagetool/resource/de"));
        let mut dict = base_resource_dir
            .as_deref()
            .and_then(shared_german_speller_dictionary)
            .map(|dict| (*dict).clone())
            .unwrap_or_else(og_spell::Dictionary::new);
        let mut variant_ignore_words: Vec<String> = Vec::new();
        let mut variant_hunspell: Option<Arc<og_spell::hunspell::HunspellChecker>> = None;
        if let Some(lt_root) = find_lt_resource_dir() {
            let hunspell_dir =
                lt_root.join("de/src/main/resources/org/languagetool/resource/de/hunspell");
            for file in ["spelling.txt", &format!("spelling-de-{country}.txt")] {
                if let Ok(words) = std::fs::read_to_string(hunspell_dir.join(file)) {
                    for line in words.lines() {
                        for word in og_rules::spelling_support::prepare_line_for_speller(
                            og_rules::spelling_support::SpellerVariant::German,
                            line,
                        ) {
                            dict.add_word(&word);
                            if file.starts_with("spelling-de-") && !word.is_empty() {
                                variant_ignore_words.push(word);
                            }
                        }
                    }
                }
            }
            let variant_dic = hunspell_dir.join(format!("de_{country}.dic"));
            let variant_aff = hunspell_dir.join(format!("de_{country}.aff"));
            if variant_dic.exists() && variant_aff.exists() {
                if let Ok(content) = std::fs::read_to_string(&variant_dic) {
                    for line in content.lines() {
                        let word = line.split('/').next().unwrap_or("").trim();
                        if !word.is_empty() && !word.starts_with('#') {
                            dict.add_word(word);
                        }
                    }
                }
                variant_hunspell =
                    og_spell::hunspell::HunspellChecker::load(&variant_aff, &variant_dic)
                        .map(std::sync::Arc::new);
            }
        }
        if !dict.is_empty() {
            let id = match country.as_str() {
                "AT" => "AUSTRIAN_GERMAN_SPELLER_RULE",
                "CH" => "SWISS_GERMAN_SPELLER_RULE",
                _ => "MORFOLOGIK_RULE_DE_DE",
            };
            rules.retain(|r| r.id() != "MORFOLOGIK_RULE_DE_DE");
            let variant_code = format!("de-{country}");
            if let Some(speller) = &self.german_speller {
                let mut variant_speller = speller.with_shared_data(Arc::new(dict), &variant_code);
                variant_speller = variant_speller.with_extra_ignore_words(variant_ignore_words);
                if let Some(checker) = variant_hunspell {
                    variant_speller = variant_speller.with_hunspell_core(checker);
                }
                rules.push(Arc::new(og_spell::GermanSpellerRule::new(
                    Arc::new(variant_speller),
                    id,
                )));
            } else {
                rules.push(Arc::new(og_spell::SpellingCheckRule::new_shared(
                    Arc::new(dict),
                    id,
                    HashSet::new(),
                )));
            }
        }
        let mut engine = Self::new(Language::from_code(code).unwrap_or(Language::German));
        engine.rules = rules;
        engine.text_level_rules = self.text_level_rules.clone();
        engine.tagger = self.tagger.clone();
        engine.disambiguator = self.disambiguator.clone();
        engine.spelling_support = self.spelling_support.clone();
        engine.german_speller = self.german_speller.clone();
        engine.synthesizer = self.synthesizer.clone();
        engine.suggestion_synth = self.suggestion_synth.clone();
        engine.post_chunker = self.post_chunker.clone();
        engine.rebuild_checker();
        engine
    }

    pub fn portuguese_variant(&self, code: &str) -> Self {
        let variant = crate::pt::variant_for_code(code).unwrap_or(crate::pt::PtVariant::Portugal);
        let variant_dir = crate::pt::variant_rules_dir(variant);
        let mut rules: Vec<Arc<dyn Rule>> = self.rules.clone();
        if let Some(lt_root) = find_lt_resource_dir() {
            let rules_dir = lt_root.join(format!(
                "pt/src/main/resources/org/languagetool/rules/pt/{variant_dir}"
            ));
            for file in ["grammar.xml", "style.xml"] {
                let path = rules_dir.join(file);
                if !path.exists() {
                    continue;
                }
                if let Ok(xml) = std::fs::read_to_string(&path) {
                    let compiler = XmlCompiler::new();
                    if let Ok(rule_set) = compiler.compile_file(&xml) {
                        rules.extend(rule_set.rules.into_iter().map(|compiled| {
                            Arc::new(XmlPatternRule::with_spelling(
                                compiled,
                                self.spelling_support.clone(),
                            )) as Arc<dyn Rule>
                        }));
                    }
                }
            }
        }
        let native = crate::pt::variant_native_rules(variant);
        rules.extend(native.rules);
        let mut text_level_rules = self.text_level_rules.clone();
        text_level_rules.extend(native.text_level_rules);
        let mut engine = Self::new(Language::from_code(code).unwrap_or(Language::PortuguesePt));
        engine.rules = rules;
        engine.text_level_rules = text_level_rules;
        engine.tagger = self.tagger.clone();
        engine.disambiguator = self.disambiguator.clone();
        engine.spelling_support = self.spelling_support.clone();
        engine.synthesizer = self.synthesizer.clone();
        engine.suggestion_synth = self.suggestion_synth.clone();
        engine.post_chunker = self.post_chunker.clone();
        engine.rebuild_checker();
        engine
    }

    pub fn for_code(code: &str) -> Option<Self> {
        let _heavy = og_core::releasable::heavy_guard();
        let _guard = engine_build_guard();
        Self::for_code_inner(code)
    }

    fn for_code_inner(code: &str) -> Option<Self> {
        let language = Language::from_code(code)?;
        let short = language.short_code().to_string();
        let config = match short.as_str() {
            "da" => ("danish", "HUNSPELL_RULE"),
            "ca" => ("catalan", "MORFOLOGIK_RULE_CA_ES"),
            "fr" => ("french", "FR_SPELLING_RULE"),
            "es" => ("spanish", "MORFOLOGIK_RULE_ES"),
            "pt" => ("portuguese", "MORFOLOGIK_RULE_PT_PT"),
            "nl" => ("dutch", "MORFOLOGIK_RULE_NL_NL"),
            "pl" => ("polish", "MORFOLOGIK_RULE_PL_PL"),
            "ru" => ("russian", "MORFOLOGIK_RULE_RU_RU"),
            "it" => ("italian", "MORFOLOGIK_RULE_IT_IT"),
            "ar" => ("arabic", "HUNSPELL_RULE_AR"),
            "ast" => ("asturian", "MORFOLOGIK_RULE_AST"),
            "be" => ("", "MORFOLOGIK_RULE_BE_BY"),
            "crh" => ("crimean_tatar", "MORFOLOGIK_RULE_CRH_UA"),
            "br" => ("breton", "MORFOLOGIK_RULE_BR_FR"),
            "el" => ("greek", "MORFOLOGIK_RULE_EL_GR"),
            "gl" => ("galician", "HUNSPELL_RULE"),
            "ro" => ("romanian", "MORFOLOGIK_RULE_RO_RO"),
            "sk" => ("slovak", "MORFOLOGIK_RULE_SK_SK"),
            "sv" => ("swedish", "HUNSPELL_RULE"),
            "eo" => ("", "HUNSPELL_RULE_EO"),
            "fa" => ("", ""),
            "ga" => ("irish", "MORFOLOGIK_RULE_GA_IE"),
            "is" => ("", ""),
            "ja" => ("", ""),
            "km" => ("khmer", "HUNSPELL_RULE"),
            "lt" => ("", "MORFOLOGIK_RULE_LT_LT"),
            "ml" => ("malayalam", "MORFOLOGIK_RULE_ML_IN"),
            "sl" => ("", "MORFOLOGIK_RULE_SL_SI"),
            "sr" => ("", "MORFOLOGIK_RULE_SR_EKAVIAN"),
            "ta" => ("tamil", ""),
            "tl" => ("tagalog", crate::tl::MORFOLOGIK_RULE_TL),
            "uk" => ("ukrainian", crate::uk::MORFOLOGIK_RULE_UK_UA),
            "zh" => ("", ""),
            _ => return None,
        };

        let mut engine = Self::new(language);

        let Some(lt_root) = find_lt_resource_dir() else {
            return Some(engine);
        };
        let resource_dir = lt_root.join(format!(
            "{short}/src/main/resources/org/languagetool/resource/{short}"
        ));
        if short == "eo"
            && let Some(tagger) = og_tagger::EsperantoTagger::from_resource_dir(&resource_dir)
        {
            engine.tagger = Some(Arc::new(tagger));
        }
        let tagger = if short == "fr" {
            og_tagger::GenericTagger::french(&resource_dir)
        } else if short == "ru" {
            og_tagger::GenericTagger::russian(&resource_dir)
        } else {
            og_tagger::GenericTagger::from_resource_dir(&resource_dir, config.0)
        };
        let has_dictionary = tagger.has_dictionary();
        if short == "ar" && has_dictionary {
            let resource = resource_dir.clone();
            engine.tagger = Some(Arc::new(
                og_tagger::arabic_tagger::ArabicTagger::from_resource_dir(&resource),
            ));
        }
        let has_manual = resource_dir.join("added.txt").exists();
        let generic_tagger_arc: Option<Arc<og_tagger::GenericTagger>> =
            if engine.tagger.is_none() && (has_dictionary || has_manual) {
                Some(Arc::new(tagger))
            } else {
                None
            };
        if let Some(arc) = generic_tagger_arc.clone() {
            engine.tagger = Some(arc);
        }
        let fr_native_tagger = (short == "fr")
            .then(|| generic_tagger_arc.clone())
            .flatten();
        if short == "ru" {
            engine.post_chunker = Some(og_tagger::russian_chunker::shared_russian_chunker());
        }
        if short == "ca" {
            engine.post_chunker = Some(Arc::new(crate::ca::gv_chunker::CatalanGVChunker));
        }
        let disambig_path = resource_dir.join("disambiguation.xml");
        if short == "es"
            && let Some(chain) = crate::es::build_es_disambiguator(&resource_dir)
        {
            engine.disambiguator = Some(chain);
        } else if short == "fr"
            && let Some(chain) = crate::fr::build_fr_disambiguator(&resource_dir)
        {
            engine.disambiguator = Some(chain);
        } else if short == "pt"
            && let Some(chain) = crate::pt::build_pt_disambiguator(&resource_dir)
        {
            engine.disambiguator = Some(chain);
        } else if short == "pl"
            && let Some(chain) = crate::pl::build_pl_disambiguator(&resource_dir)
        {
            engine.disambiguator = Some(chain);
        } else if short == "br"
            && let Some(disambiguator) = crate::br::build_br_disambiguator(&resource_dir)
        {
            engine.disambiguator = Some(Arc::new(disambiguator));
        } else if short == "ga"
            && let Some(chain) = crate::ga::build_ga_disambiguator(&resource_dir)
        {
            engine.disambiguator = Some(Arc::new(chain));
        } else if short == "gl"
            && let Some(chain) = crate::gl::build_gl_disambiguator(&resource_dir)
        {
            engine.disambiguator = Some(Arc::new(chain));
        } else if disambig_path.exists()
            && let Ok(xml) = std::fs::read_to_string(&disambig_path)
            && let Ok(disambiguator) = XmlDisambiguator::from_xml(&xml)
            && let Some(core_resource_dir) = find_lt_core_resource_dir()
        {
            engine.disambiguator = Some(og_tagger::hybrid_disambiguator(
                &short,
                &resource_dir,
                &core_resource_dir,
                disambiguator,
            ));
        }

        let mut dict = wordlist_dictionary(&resource_dir, config.0);
        if let Ok(extra) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt")) {
            for line in extra.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }
        let mut ignore_words = HashSet::new();
        if let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/ignore.txt")) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    ignore_words.insert(word.to_lowercase());
                }
            }
        }
        let mut prohibit = HashSet::new();
        if let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/prohibit.txt")) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    prohibit.insert(word.to_string());
                }
            }
        }
        if short == "nl" {
            if let (Some(core_resource_dir), Some(dictionary)) = (
                find_lt_core_resource_dir(),
                crate::nl::speller::shared_speller_dictionary(),
            ) {
                engine.spelling_support = Some(Arc::new(
                    og_rules::spelling_support::SpellingSupport::new_for_language(
                        dictionary,
                        &core_resource_dir,
                        &resource_dir,
                        &short,
                    ),
                ));
            }
        } else if !dict.is_empty() && short != "eo" && short != "ga" {
            let dict = Arc::new(dict);
            if short != "pl"
                && short != "ast"
                && short != "br"
                && short != "da"
                && short != "ml"
                && short != "ta"
                && short != "tl"
            {
                engine.add_rule(Arc::new(
                    og_spell::SpellingCheckRule::new_shared(dict.clone(), config.1, HashSet::new())
                        .with_ignore_words(ignore_words),
                ));
            }
            if let Some(core_resource_dir) = find_lt_core_resource_dir() {
                engine.spelling_support = Some(Arc::new(
                    og_rules::spelling_support::SpellingSupport::new_for_language(
                        dict,
                        &core_resource_dir,
                        &resource_dir,
                        &short,
                    ),
                ));
            }
        }
        engine.synthesizer =
            og_rules::postponed_adjective::LangSynthesizer::for_language(&resource_dir, &short)
                .map(Arc::new);
        engine.suggestion_synth = engine.synthesizer.clone();

        let rules_dir = lt_root.join(format!(
            "{short}/src/main/resources/org/languagetool/rules/{short}"
        ));
        for file in ["grammar.xml", "style.xml"] {
            let path = rules_dir.join(file);
            if !path.exists() {
                continue;
            }
            let outcome = if short == "fr" {
                engine.load_xml_file_normalized(&path, crate::fr::normalize_fr_rules_xml)
            } else if short == "pl" {
                engine.load_xml_file_normalized(&path, crate::pl::normalize_pl_rules_xml)
            } else if short == "br" {
                engine.load_xml_file_normalized(&path, crate::br::normalize_br_rules_xml)
            } else if short == "lt" {
                engine.load_xml_file_normalized(&path, crate::lt::normalize_lt_rules_xml)
            } else if short == "sv" {
                engine.load_xml_file_normalized(&path, crate::sv::normalize_sv_rules_xml)
            } else if short == "uk" {
                engine.load_xml_file_normalized(&path, crate::uk::normalize_uk_rules_xml)
            } else if short == "zh" {
                engine.load_xml_file_normalized(&path, crate::zh::normalize_zh_rules_xml)
            } else {
                engine.load_xml_file(&path)
            };
            match outcome {
                Ok(count) => eprintln!("Loaded {} rules from {short}/{file}", count),
                Err(e) => eprintln!("Warning: failed to load {short}/{file}: {e}"),
            }
        }
        if short == "nl"
            && let Some(variant_path) = crate::nl::variant_grammar_path()
        {
            match engine.load_xml_file(&variant_path) {
                Ok(count) => eprintln!("Loaded {count} rules from nl/nl-NL/grammar.xml"),
                Err(e) => eprintln!("Warning: failed to load nl/nl-NL/grammar.xml: {e}"),
            }
        }
        if short == "sk" {
            for path in crate::sk::sk_extra_rule_paths(&lt_root) {
                match engine.load_xml_file_normalized(&path, crate::sk::normalize_sk_rules_xml) {
                    Ok(count) => eprintln!("Loaded {count} rules from sk/grammar-typography.xml"),
                    Err(e) => eprintln!("Warning: failed to load sk/grammar-typography.xml: {e}"),
                }
            }
        }
        if short == "sr" {
            for path in crate::sr::sr_extra_rule_paths(&lt_root) {
                match engine.load_xml_file(&path) {
                    Ok(count) => eprintln!(
                        "Loaded {count} rules from sr/{}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                    Err(e) => eprintln!(
                        "Warning: failed to load sr/{}: {e}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                }
            }
        }
        if short == "uk" {
            for path in crate::uk::uk_extra_rule_paths(&lt_root) {
                let file = path.file_name().unwrap_or_default().to_string_lossy();
                match engine.load_xml_file_normalized(&path, crate::uk::normalize_uk_rules_xml) {
                    Ok(count) => eprintln!("Loaded {count} rules from uk/{file}"),
                    Err(e) => eprintln!("Warning: failed to load uk/{file}: {e}"),
                }
            }
        }

        if short != "nl" && short != "pl" && short != "ru" && short != "ar" && short != "uk" {
            engine.add_rules([
                Arc::new(og_rules::native_rules::DoublePunctuationRule::new()) as Arc<dyn Rule>,
                Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            ]);
        } else if short == "ru" {
            engine.add_rule(Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()));
        }
        let brackets_rule: Option<Arc<dyn TextLevelRule>> = if short == "es"
            || short == "be"
            || short == "br"
            || short == "nl"
            || short == "pl"
            || short == "ru"
            || short == "ta"
            || short == "uk"
            || short == "zh"
        {
            None
        } else if short == "pt" || short == "da" || short == "ga" {
            Some(Arc::new(
                og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
                    vec!["[", "(", "{", "\"", "“"],
                    vec!["]", ")", "}", "\"", "”"],
                ),
            ))
        } else if short == "el" {
            Some(Arc::new(
                og_rules::text_level_rules::GenericUnpairedBracketsRule::new()
                    .with_id("EL_UNPAIRED_BRACKETS")
                    .with_symbols(
                        vec!["[", "(", "{", "“", "\"", "«"],
                        vec!["]", ")", "}", "”", "\"", "»"],
                    ),
            ))
        } else if short == "ca" {
            Some(Arc::new(crate::ca::catalan_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>)
        } else if short == "it" {
            Some(Arc::new(
                og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
                    vec!["[", "(", "{", "\u{bb}", "\u{ab}"],
                    vec!["]", ")", "}", "\u{ab}", "\u{bb}"],
                ),
            ))
        } else if short == "ar" {
            Some(Arc::new(
                og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
                    vec!["[", "(", "{", "«", "﴾", "\"", "'"],
                    vec!["]", ")", "}", "»", "﴿", "\"", "'"],
                ),
            ))
        } else if short == "sk" {
            Some(Arc::new(
                og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
                    vec!["[", "(", "{", "„", "»", "«", "\""],
                    vec!["]", ")", "}", "“", "«", "»", "\""],
                ),
            ))
        } else if short == "gl" {
            Some(Arc::new(crate::gl::galician_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>)
        } else if short == "is" {
            Some(Arc::new(crate::is::icelandic_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>)
        } else if short == "km" {
            Some(Arc::new(crate::km::khmer_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>)
        } else if short == "lt" {
            Some(Arc::new(crate::lt::lithuanian_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>)
        } else if short == "ml" {
            Some(Arc::new(crate::ml::malayalam_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>)
        } else if short == "ro" {
            Some(Arc::new(crate::ro::romanian_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>)
        } else if short == "sl" {
            Some(Arc::new(crate::sl::slovenian_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>)
        } else if short == "sr" {
            Some(Arc::new(crate::sr::serbian_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>)
        } else if short == "sv" {
            Some(Arc::new(crate::sv::swedish_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>)
        } else if short == "tl" {
            Some(Arc::new(crate::tl::tagalog_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>)
        } else {
            Some(Arc::new(
                og_rules::text_level_rules::GenericUnpairedBracketsRule::new(),
            ))
        };
        let mut generic_text_level_rules: Vec<Arc<dyn TextLevelRule>> =
            if short == "nl" || short == "pl" || short == "uk" {
                Vec::new()
            } else {
                vec![
                    Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new())
                        as Arc<dyn TextLevelRule>,
                    Arc::new(og_rules::text_level_rules::SentenceWhitespaceRule::new()),
                ]
            };
        generic_text_level_rules.extend(brackets_rule);
        engine.add_text_level_rules(generic_text_level_rules);

        match short.as_str() {
            "fr" => {
                if let Some(tagger) = crate::fr::tagger::FrenchTagger::shared_arc() {
                    engine.tagger = Some(tagger);
                }
                engine.add_rule(
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new())
                        as Arc<dyn Rule>,
                );
                engine.add_text_level_rules([
                    Arc::new(og_rules::text_level_rules::LongSentenceRule::new(40))
                        as Arc<dyn TextLevelRule>,
                    Arc::new(og_rules::text_level_rules::LongParagraphRule::new(220)),
                ]);
                engine.add_rule(Arc::new(crate::fr::french_compound_rule(fr_native_tagger)));
                engine.add_rule(Arc::new(crate::fr::french_question_whitespace_strict_rule()));
                engine.add_rule(Arc::new(crate::fr::french_question_whitespace_rule()));
                engine.add_rule(Arc::new(crate::fr::french_simple_replace_rule()));
                engine.add_text_level_rule(Arc::new(crate::fr::french_repeated_words_rule(
                    engine.synthesizer.clone(),
                )));
            }
            "es" => {
                if let Some(tagger) = crate::es::tagger::SpanishTagger::shared_arc() {
                    engine.tagger = Some(tagger);
                }
                let (sentence_rules, text_level_rules) = crate::es::native_rules();
                engine.add_rules(sentence_rules);
                engine.add_text_level_rules(text_level_rules);
                if let Some(model) = og_languagemodel::DiskLanguageModel::shared_for_language("es")
                {
                    engine.add_rule(Arc::new(
                        crate::confusion_probability::ConfusionRule::new_spanish(model),
                    ));
                }
            }
            "pt" => {
                if let Some(tagger) = crate::pt::tagger::PortugueseTagger::shared_arc() {
                    engine.tagger = Some(tagger);
                }
                engine.add_rule(
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new())
                        as Arc<dyn Rule>,
                );
                engine.add_rule(Arc::new(
                    og_rules::text_level_rules::WhiteSpaceAtBeginOfParagraphRule::new(),
                ) as Arc<dyn Rule>);
                engine.add_text_level_rules([
                    Arc::new(og_rules::text_level_rules::LongSentenceRule::new(50))
                        as Arc<dyn TextLevelRule>,
                    Arc::new(og_rules::text_level_rules::LongParagraphRule::new(220)),
                    Arc::new(og_rules::text_level_rules::WhiteSpaceBeforeParagraphEndRule::new()),
                    Arc::new(og_rules::text_level_rules::EmptyLineRule::new()),
                    Arc::new(
                        og_rules::text_level_rules::PunctuationMarkAtParagraphEnd2Rule::new()
                            .with_message("Adicione pontuação final ao fim do parágrafo."),
                    ),
                ]);
                let native = crate::pt::shared_native_rules();
                engine.add_rules(native.rules);
                engine.add_text_level_rules(native.text_level_rules);
            }
            "nl" => {
                if let Some(tagger) = crate::nl::tagger::DutchTagger::shared_arc() {
                    engine.tagger = Some(tagger as Arc<dyn og_core::checker::Tagger>);
                }
                let native = crate::nl::dutch_native_rules();
                engine.add_rules(native.rules);
                engine.add_text_level_rules(native.text_level_rules);
            }
            "pl" => {
                if let Some(tagger) = crate::pl::tagger::PolishTagger::shared_arc() {
                    engine.tagger = Some(tagger as Arc<dyn og_core::checker::Tagger>);
                }
                let native = crate::pl::polish_native_rules();
                engine.add_rules(native.rules);
                engine.add_text_level_rules(native.text_level_rules);
            }
            "it" => {
                engine.add_rules([
                    Arc::new(og_rules::native_rules::WhitespaceBeforePunctuationRule::new())
                        as Arc<dyn Rule>,
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
                    Arc::new(crate::it::ItalianWordRepeatRule::new()),
                ]);
                if let Some(model) = og_languagemodel::DiskLanguageModel::shared_for_language("it")
                {
                    engine.add_rule(Arc::new(
                        crate::confusion_probability::ConfusionRule::new_italian(model),
                    ));
                }
            }
            "ast" => {
                engine.add_rules([
                    Arc::new(crate::ast::AsturianSpellerRule::new()) as Arc<dyn Rule>,
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
                ]);
            }
            "da" => {
                engine.add_rules([
                    Arc::new(crate::da::DanishSpellerRule::new()) as Arc<dyn Rule>,
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
                ]);
            }
            "be" => {
                engine.add_rule(
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new())
                        as Arc<dyn Rule>,
                );
                engine.add_rule(Arc::new(
                    og_rules::text_level_rules::WhiteSpaceAtBeginOfParagraphRule::new(),
                ) as Arc<dyn Rule>);
                engine.add_text_level_rules([
                    Arc::new(og_rules::text_level_rules::LongSentenceRule::new(50))
                        as Arc<dyn TextLevelRule>,
                    Arc::new(og_rules::text_level_rules::LongParagraphRule::new(220)),
                    Arc::new(crate::be::belarusian_paragraph_repeat_beginning_rule())
                        as Arc<dyn TextLevelRule>,
                    Arc::new(og_rules::text_level_rules::WhiteSpaceBeforeParagraphEndRule::new()),
                    Arc::new(og_rules::text_level_rules::PunctuationMarkAtParagraphEnd2Rule::new()),
                ]);
                engine.add_rule(Arc::new(crate::be::belarusian_simple_replace_rule()));
                engine.add_rule(Arc::new(crate::be::belarusian_specific_case_rule()));
            }
            "br" => {
                if let Some(tagger) = crate::br::BretonTagger::shared_arc() {
                    engine.tagger = Some(tagger);
                }
                engine.add_rules([
                    crate::br::BretonSpellerRule::shared_arc() as Arc<dyn Rule>,
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
                    Arc::new(crate::br::breton_topo_replace_rule()),
                ]);
            }
            "el" => {
                engine.add_rule(
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new())
                        as Arc<dyn Rule>,
                );
                engine.add_text_level_rules([Arc::new(
                    og_rules::text_level_rules::LongSentenceRule::new(50),
                ) as Arc<dyn TextLevelRule>]);
                let natives = crate::el::greek_native_rules();
                engine.add_rules(natives.rules);
                engine.add_text_level_rules(natives.text_level_rules);
            }
            "eo" => {
                engine.add_rules([
                    Arc::new(crate::eo::EsperantoSpellerRule::new()) as Arc<dyn Rule>,
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
                    Arc::new(og_rules::native_rules::WordRepeatRule::new()),
                ]);
            }
            "fa" => {
                engine.add_text_level_rule(Arc::new(
                    og_rules::text_level_rules::LongSentenceRule::new(50),
                ) as Arc<dyn TextLevelRule>);
                let native = crate::fa::persian_native_rules();
                engine.add_rules(native.rules);
                engine.add_text_level_rules(native.text_level_rules);
            }
            "ga" => {
                engine.add_rule(
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new())
                        as Arc<dyn Rule>,
                );
                engine.add_text_level_rules([
                    Arc::new(og_rules::text_level_rules::LongSentenceRule::new(50))
                        as Arc<dyn TextLevelRule>,
                    Arc::new(og_rules::text_level_rules::LongParagraphRule::new(220)),
                ]);
                let native = crate::ga::irish_native_rules();
                engine.add_rules(native.rules);
                engine.add_text_level_rules(native.text_level_rules);
            }
            "gl" => {
                if let Some(tagger) = crate::gl::GalicianTagger::shared_arc() {
                    engine.tagger = Some(tagger);
                }
                let native = crate::gl::galician_native_rules();
                engine.add_rules(native.rules);
                engine.add_text_level_rules(native.text_level_rules);
            }
            "is" => {
                engine.add_rules([
                    Arc::new(crate::is::IcelandicSpellerRule::new()) as Arc<dyn Rule>,
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
                    Arc::new(og_rules::native_rules::WordRepeatRule::new()),
                ]);
            }
            "km" => {
                engine.add_rules([
                    Arc::new(crate::km::khmer_simple_replace_rule()) as Arc<dyn Rule>,
                    Arc::new(crate::km::KhmerWordRepeatRule::new()),
                    Arc::new(crate::km::KhmerSpaceBeforeRule::new()),
                ]);
            }
            "lt" => {
                engine.add_rule(
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new())
                        as Arc<dyn Rule>,
                );
            }
            "ml" => {
                engine.add_rules([
                    Arc::new(crate::ml::MalayalamSpellerRule::new()) as Arc<dyn Rule>,
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
                    Arc::new(og_rules::native_rules::WordRepeatRule::new()),
                ]);
            }
            "sk" => {
                let native = crate::sk::slovak_native_rules();
                engine.add_rules(native.rules);
            }
            "sl" => {
                engine.add_rules([
                    Arc::new(crate::sl::slovenian_speller_rule()) as Arc<dyn Rule>,
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
                    Arc::new(og_rules::native_rules::WordRepeatRule::new()),
                ]);
            }
            "sr" => {
                if let Some(tagger) = crate::sr::serbian_tagger(crate::sr::SerbianVariant::Ekavian)
                {
                    engine.tagger = Some(tagger);
                }
                engine.add_rules([
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new())
                        as Arc<dyn Rule>,
                    Arc::new(og_rules::native_rules::WordRepeatRule::new()),
                    Arc::new(crate::sr::sr_speller_rule(
                        crate::sr::SerbianVariant::Ekavian,
                    )),
                    Arc::new(crate::sr::sr_grammar_replace_rule(
                        crate::sr::SerbianVariant::Ekavian,
                    )),
                    Arc::new(crate::sr::sr_style_replace_rule(
                        crate::sr::SerbianVariant::Ekavian,
                    )),
                ]);
            }
            "ca" => {
                if let Some(tagger) = crate::ca::CatalanTagger::shared_arc() {
                    engine.tagger = Some(tagger);
                }
                engine.add_rule(
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new())
                        as Arc<dyn Rule>,
                );
                let native = crate::ca::catalan_native_rules();
                engine.add_rules(native.rules);
                engine.add_text_level_rule(Arc::new(
                    og_rules::text_level_rules::LongSentenceRule::new(60),
                ));
                engine.add_text_level_rules(native.text_level_rules);
            }
            "ru" => {
                engine.add_rules([
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new())
                        as Arc<dyn Rule>,
                    Arc::new(crate::ru::russian_simple_word_repeat_rule()),
                    Arc::new(crate::ru::RussianWordRepeatRule::new()),
                    Arc::new(crate::ru::russian_simple_replace_rule()),
                    Arc::new(crate::ru::russian_compound_rule()),
                    Arc::new(crate::ru::RussianVerbConjugationRule::new()),
                    Arc::new(crate::ru::russian_specific_case_rule()),
                    Arc::new(crate::ru::RussianDashRule::new()),
                ]);
                engine.add_text_level_rules([
                    Arc::new(crate::ru::russian_unpaired_brackets_rule()) as Arc<dyn TextLevelRule>,
                    Arc::new(crate::ru::russian_word_coherency_rule()),
                    Arc::new(crate::ru::russian_word_root_repeat_rule()),
                    Arc::new(crate::ru::RussianFillerWordsRule::new()),
                    Arc::new(og_rules::text_level_rules::LongSentenceRule::new(50)),
                    Arc::new(
                        og_rules::text_level_rules::LongParagraphRule::new(220)
                            .with_description("Удобочитаемость: абзац длиной 220 слов")
                            .with_message("Абзац длиной 220 слов, разбейте его на части."),
                    ),
                    Arc::new(
                        og_rules::text_level_rules::PunctuationMarkAtParagraphEnd2Rule::new()
                            .with_message("Добавьте знак пунктуации в конце абзаца."),
                    ),
                    Arc::new(og_rules::text_level_rules::WhiteSpaceBeforeParagraphEndRule::new()),
                    Arc::new(crate::ru::russian_paragraph_repeat_beginning_rule()),
                ]);
                if let Some(yo) = crate::ru::RussianYoSpellerRule::from_resource_dir(&resource_dir)
                {
                    engine.add_rule(Arc::new(yo) as Arc<dyn Rule>);
                }
                engine.add_rule(Arc::new(
                    og_rules::text_level_rules::WhiteSpaceAtBeginOfParagraphRule::new(),
                ) as Arc<dyn Rule>);
                if let Some(model) = og_languagemodel::DiskLanguageModel::shared_for_language("ru")
                {
                    engine.add_rule(Arc::new(
                        crate::confusion_probability::ConfusionRule::new_russian(model),
                    ));
                }
            }
            "ro" => {
                engine.add_rules([
                    Arc::new(crate::ro::romanian_replace_rule()) as Arc<dyn Rule>,
                    Arc::new(crate::ro::romanian_compound_rule()),
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
                    Arc::new(og_rules::native_rules::WordRepeatRule::new()),
                ]);
                engine.add_text_level_rule(Arc::new(
                    crate::ro::romanian_word_repeat_beginning_rule(),
                ));
            }
            "crh" => {
                engine.add_rule(
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new())
                        as Arc<dyn Rule>,
                );
                engine.add_text_level_rule(Arc::new(
                    og_rules::text_level_rules::WhiteSpaceBeforeParagraphEndRule::new(),
                ));
                engine.add_rule(Arc::new(
                    og_rules::text_level_rules::WhiteSpaceAtBeginOfParagraphRule::new(),
                ) as Arc<dyn Rule>);
            }
            "sv" => {
                engine.add_rules([
                    Arc::new(crate::sv::swedish_compound_rule()) as Arc<dyn Rule>,
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
                    Arc::new(og_rules::native_rules::WordRepeatRule::new()),
                ]);
                engine.add_text_level_rule(Arc::new(crate::sv::swedish_word_coherency_rule()));
                engine.add_text_level_rules([
                    Arc::new(og_rules::text_level_rules::LongSentenceRule::new(40))
                        as Arc<dyn TextLevelRule>,
                    Arc::new(og_rules::text_level_rules::LongParagraphRule::new(150)),
                ]);
            }
            "ta" => {
                engine.add_text_level_rule(Arc::new(
                    og_rules::text_level_rules::LongSentenceRule::new(50),
                ) as Arc<dyn TextLevelRule>);
            }
            "tl" => {
                engine.add_rules([
                    Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new())
                        as Arc<dyn Rule>,
                    Arc::new(crate::tl::tagalog_speller_rule()),
                ]);
            }
            "uk" => {
                let native = crate::uk::ukrainian_native_rules();
                engine.add_rules(native.rules);
                engine.add_text_level_rules(native.text_level_rules);
            }
            "ar" => {
                engine.add_rules([
                    Arc::new(crate::ar::arabic_base_comma_whitespace_rule()) as Arc<dyn Rule>,
                    Arc::new(crate::ar::arabic_double_punctuation_rule()),
                    Arc::new(crate::ar::arabic_comma_whitespace_rule()),
                    Arc::new(crate::ar::arabic_question_mark_whitespace_rule()),
                    Arc::new(crate::ar::arabic_semicolon_whitespace_rule()),
                    Arc::new(crate::ar::arabic_word_repeat_rule()),
                    Arc::new(crate::ar::arabic_darja_rule()),
                    Arc::new(crate::ar::arabic_diacritics_rule()),
                    Arc::new(crate::ar::arabic_simple_replace_rule()),
                    Arc::new(crate::ar::arabic_homophones_rule()),
                    Arc::new(crate::ar::arabic_redundancy_rule()),
                    Arc::new(crate::ar::arabic_wordiness_rule()),
                    Arc::new(crate::ar::arabic_wrong_word_in_context_rule()),
                    Arc::new(crate::ar::trans_verb::ArabicTransVerbRule::from_resource_dir(
                        &resource_dir,
                    )),
                    Arc::new(
                        crate::ar::inflected_one_word::ArabicInflectedOneWordReplaceRule::from_resource_dir(
                            &resource_dir,
                        ),
                    ),
                ]);
                engine.add_text_level_rules([
                    Arc::new(og_rules::text_level_rules::LongSentenceRule::new(50))
                        as Arc<dyn TextLevelRule>,
                    Arc::new(crate::ar::arabic_word_coherency_rule()),
                ]);
                if let Some(model) = og_languagemodel::DiskLanguageModel::shared_for_language("ar")
                {
                    engine.add_rule(Arc::new(
                        crate::confusion_probability::ConfusionRule::new_arabic(model),
                    ));
                }
            }
            _ => {}
        }

        engine.rebuild_checker();
        Some(engine)
    }

    pub fn load_xml_rules(&mut self, xml: &str) -> Result<usize, String> {
        self.load_xml_rules_with_source(xml, None)
    }

    pub fn load_xml_file(&mut self, path: &Path) -> Result<usize, String> {
        let xml = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        let source_file = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string);
        self.load_xml_rules_with_base(&xml, source_file.as_deref(), path.parent())
    }

    pub(crate) fn load_xml_file_normalized(
        &mut self,
        path: &Path,
        normalize: for<'a> fn(&'a str) -> std::borrow::Cow<'a, str>,
    ) -> Result<usize, String> {
        let xml = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        let xml = normalize(&xml);
        let source_file = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string);
        self.load_xml_rules_with_base(&xml, source_file.as_deref(), path.parent())
    }

    fn load_xml_rules_with_source(
        &mut self,
        xml: &str,
        source_file: Option<&str>,
    ) -> Result<usize, String> {
        self.load_xml_rules_with_base(xml, source_file, None)
    }

    fn load_xml_rules_with_base(
        &mut self,
        xml: &str,
        source_file: Option<&str>,
        base_dir: Option<&Path>,
    ) -> Result<usize, String> {
        let compiler = XmlCompiler::new();
        let rule_set = compiler
            .compile_file_with_base(xml, base_dir)
            .map_err(|e| format!("XML compilation error: {}", e))?;

        let count = rule_set.rules.len();
        for mut compiled_rule in rule_set.rules {
            compiled_rule.source_file = source_file.map(str::to_string);
            let rule = Arc::new(XmlPatternRule::with_resources(
                compiled_rule,
                self.spelling_support.clone(),
                self.synthesizer.clone(),
                self.suggestion_synth.clone(),
            ));
            self.rules.push(rule);
        }

        self.rebuild_checker();
        Ok(count)
    }

    pub fn add_rule(&mut self, rule: Arc<dyn Rule>) {
        self.rules.push(rule);
        self.rebuild_checker();
    }

    pub fn add_rules(&mut self, rules: impl IntoIterator<Item = Arc<dyn Rule>>) {
        self.rules.extend(rules);
        self.rebuild_checker();
    }

    pub fn add_text_level_rule(&mut self, rule: Arc<dyn TextLevelRule>) {
        self.text_level_rules.push(rule);
        self.rebuild_checker();
    }

    pub fn add_text_level_rules(
        &mut self,
        rules: impl IntoIterator<Item = Arc<dyn TextLevelRule>>,
    ) {
        self.text_level_rules.extend(rules);
        self.rebuild_checker();
    }

    fn rebuild_checker(&mut self) {
        let mut checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(SentenceSplitterBridge(
                self.language.short_code().to_string(),
            )))
            .with_word_tokenizer(Arc::new(WordTokenizerBridge::for_code(
                self.language.short_code(),
            )))
            .with_language_filter(self.language_filter.clone())
            .set_rules(self.rules.clone())
            .set_text_level_rules(self.text_level_rules.clone());

        if let Some(ref tagger) = self.tagger {
            checker = checker
                .with_tagger(tagger.clone())
                .with_chunker(Arc::new(og_tagger::chunker::PostDisambiguationChunker));
        }

        if let Some(ref disambiguator) = self.disambiguator {
            checker = checker.with_disambiguator(disambiguator.clone());
        }

        if let Some(ref chunker) = self.post_chunker {
            checker = checker.with_post_chunker(chunker.clone());
        }

        self.checker = checker;
    }

    pub fn check(&self, text: &str) -> CheckResult {
        let request = CheckRequest::new(text, self.language.clone());
        self.checker.check(&request)
    }

    pub fn check_with_request(&self, request: &CheckRequest) -> CheckResult {
        let short_code = match request.mother_tongue.as_ref() {
            Some(mt) => mt.short_code().to_string(),
            None => return self.checker.check(request),
        };
        {
            let cache = self.l2_checkers.lock().unwrap();
            if let Some(checker) = cache.get(&short_code) {
                return checker.check(request);
            }
        }
        let l2 = self.l2_rules(&short_code);
        if l2.is_empty() {
            return self.checker.check(request);
        }
        let mut rules = self.rules.clone();
        rules.extend(l2);
        let checker = self.build_checker(rules);
        let result = checker.check(request);
        let mut cache = self.l2_checkers.lock().unwrap();
        cache.insert(short_code, checker);
        result
    }

    fn l2_rules(&self, mother_tongue_code: &str) -> Vec<Arc<dyn Rule>> {
        {
            let cache = self.l2_rules.lock().unwrap();
            if let Some(rules) = cache.get(mother_tongue_code) {
                return rules.clone();
            }
        }
        let loaded = self.load_l2_rules(mother_tongue_code);
        let mut cache = self.l2_rules.lock().unwrap();
        cache.insert(mother_tongue_code.to_string(), loaded.clone());
        loaded
    }

    fn load_l2_rules(&self, mother_tongue_code: &str) -> Vec<Arc<dyn Rule>> {
        let Some(lt_root) = find_lt_resource_dir() else {
            return Vec::new();
        };
        let module = self.language.short_code();
        let path = lt_root
            .join(format!(
                "{}/src/main/resources/org/languagetool/rules/{}",
                module, module
            ))
            .join(format!("grammar-l2-{}.xml", mother_tongue_code));
        if !path.exists() {
            return Vec::new();
        }
        let compiler = XmlCompiler::new();
        let xml = match std::fs::read_to_string(&path) {
            Ok(xml) => xml,
            Err(_) => return Vec::new(),
        };
        let rule_set = match compiler.compile_file(&xml) {
            Ok(rule_set) => rule_set,
            Err(_) => return Vec::new(),
        };
        let mut loaded: Vec<Arc<dyn Rule>> = rule_set
            .rules
            .into_iter()
            .map(|compiled| {
                Arc::new(XmlPatternRule::with_spelling(
                    compiled,
                    self.spelling_support.clone(),
                )) as Arc<dyn Rule>
            })
            .collect();
        if self.language.short_code() == "en"
            && let Some(model) = og_languagemodel::DiskLanguageModel::shared_for_language("en")
        {
            loaded.push(Arc::new(
                crate::confusion_probability::ConfusionRule::new_english_for_l2(
                    model,
                    mother_tongue_code,
                ),
            ));
        }
        loaded
    }

    fn build_checker(&self, rules: Vec<Arc<dyn Rule>>) -> Checker {
        let mut checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(SentenceSplitterBridge(
                self.language.short_code().to_string(),
            )))
            .with_word_tokenizer(Arc::new(WordTokenizerBridge::for_code(
                self.language.short_code(),
            )))
            .with_language_filter(self.language_filter.clone())
            .set_rules(rules)
            .set_text_level_rules(self.text_level_rules.clone());

        if let Some(ref tagger) = self.tagger {
            checker = checker
                .with_tagger(tagger.clone())
                .with_chunker(Arc::new(og_tagger::chunker::PostDisambiguationChunker));
        }
        if let Some(ref disambiguator) = self.disambiguator {
            checker = checker.with_disambiguator(disambiguator.clone());
        }
        checker
    }

    pub fn variant(&self, code: &str) -> Self {
        let country = code.rsplit('-').next().unwrap_or("US").to_uppercase();
        let mut extra_words = HashSet::new();
        let mut ignore_words = HashSet::new();
        let mut shared_dict: Option<Arc<og_spell::Dictionary>> = None;
        if let Some(lt_root) = find_lt_resource_dir() {
            let hunspell_dir =
                lt_root.join("en/src/main/resources/org/languagetool/resource/en/hunspell");
            if let Ok(words) =
                std::fs::read_to_string(hunspell_dir.join(format!("spelling_en-{country}.txt")))
            {
                for line in words.lines() {
                    let word = line.trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        extra_words.insert(word.to_string());
                    }
                }
            }
            if let Ok(words) = std::fs::read_to_string(hunspell_dir.join("ignore.txt")) {
                for line in words.lines() {
                    let word = line.trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        ignore_words.insert(word.to_lowercase());
                    }
                }
            }
            shared_dict = Some(Arc::new(variant_dictionary(&hunspell_dir, &country)));
        }

        let mut rules: Vec<Arc<dyn Rule>> = self
            .rules
            .iter()
            .filter(|r| !r.id().starts_with("MORFOLOGIK_RULE_EN_"))
            .cloned()
            .collect();
        if let Some(lt_root) = find_lt_resource_dir() {
            let variant_dir = lt_root.join("en/src/main/resources/org/languagetool/rules/en");
            for file in [format!("{code}/grammar.xml"), format!("{code}/style.xml")] {
                let path = variant_dir.join(file);
                if !path.exists() {
                    continue;
                }
                let compiler = XmlCompiler::new();
                if let Ok(xml) = std::fs::read_to_string(&path)
                    && let Ok(rule_set) = compiler.compile_file(&xml)
                {
                    rules.extend(rule_set.rules.into_iter().map(|compiled| {
                        Arc::new(XmlPatternRule::with_spelling(
                            compiled,
                            self.spelling_support.clone(),
                        )) as Arc<dyn Rule>
                    }));
                }
            }
        }
        if let Some(dict) = shared_dict {
            let id = format!("MORFOLOGIK_RULE_EN_{country}");
            let spell_rule = SpellingCheckRule::new_shared(dict, id, extra_words)
                .with_ignore_words(ignore_words);
            rules.push(Arc::new(spell_rule));
        }

        let mut engine =
            Self::new(Language::from_code(code).unwrap_or_else(|| self.language.clone()));
        engine.rules = rules;
        engine.text_level_rules = self.text_level_rules.clone();
        engine.tagger = self.tagger.clone();
        engine.disambiguator = self.disambiguator.clone();
        engine.spelling_support = self.spelling_support.clone();
        engine.rebuild_checker();
        engine
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    pub fn rule_ids(&self) -> Vec<&str> {
        self.rules.iter().map(|r| r.id()).collect()
    }

    pub fn text_level_rule_ids(&self) -> Vec<&str> {
        self.text_level_rules.iter().map(|r| r.id()).collect()
    }

    pub fn language(&self) -> &Language {
        &self.language
    }
}

fn shared_german_spelling_support(
    dict: &Arc<og_spell::Dictionary>,
    core_resource_dir: &Path,
    resource_dir: &Path,
    base_speller: Option<&Arc<og_spell::GermanSpeller>>,
) -> Option<Arc<og_rules::spelling_support::SpellingSupport>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<
                PathBuf,
                std::sync::Weak<og_rules::spelling_support::SpellingSupport>,
            >,
        >,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut guard = cache.lock().unwrap();
    if let Some(hit) = guard.get(resource_dir).and_then(std::sync::Weak::upgrade) {
        return Some(hit);
    }
    let mut support = og_rules::spelling_support::SpellingSupport::new_for_language(
        dict.clone(),
        core_resource_dir,
        resource_dir,
        "de",
    );
    if let Some(base) = base_speller
        && let Some(filter_speller) = shared_german_rule_speller(base, dict, resource_dir)
    {
        support = support.with_german_rule_speller(filter_speller);
    }
    let support = std::sync::Arc::new(support);
    guard.insert(resource_dir.to_path_buf(), Arc::downgrade(&support));
    Some(support)
}

fn shared_german_rule_speller(
    base: &Arc<og_spell::GermanSpeller>,
    dict: &Arc<og_spell::Dictionary>,
    resource_dir: &Path,
) -> Option<Arc<og_spell::GermanSpeller>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<PathBuf, std::sync::Weak<og_spell::GermanSpeller>>,
        >,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut guard = cache.lock().unwrap();
    if let Some(hit) = guard.get(resource_dir).and_then(std::sync::Weak::upgrade) {
        return Some(hit);
    }
    let country = if resource_dir.join("hunspell/de_DE.aff").exists() {
        "de-DE"
    } else if resource_dir.join("hunspell/de_CH.aff").exists() {
        "de-CH"
    } else {
        "de-AT"
    };
    let stem = country.replace('-', "_");
    let checker = og_spell::hunspell::HunspellChecker::load(
        &resource_dir.join(format!("hunspell/{stem}.aff")),
        &resource_dir.join(format!("hunspell/{stem}.dic")),
    )?;
    let speller = std::sync::Arc::new(
        base.with_shared_data(dict.clone(), country)
            .with_hunspell_core(std::sync::Arc::new(checker)),
    );
    guard.insert(resource_dir.to_path_buf(), Arc::downgrade(&speller));
    Some(speller)
}

fn shared_german_speller(
    dict: &Arc<og_spell::Dictionary>,
    resource_dir: &Path,
    core_resource_dir: Option<&Path>,
    tagger: Option<Arc<dyn og_core::checker::Tagger>>,
) -> Option<Arc<og_spell::GermanSpeller>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<PathBuf, std::sync::Weak<og_spell::GermanSpeller>>,
        >,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut guard = cache.lock().unwrap();
    if let Some(hit) = guard.get(resource_dir).and_then(std::sync::Weak::upgrade) {
        return Some(hit);
    }
    let speller = Arc::new(og_spell::GermanSpeller::new(
        dict.clone(),
        resource_dir,
        core_resource_dir,
        tagger,
        "de-DE",
    ));
    guard.insert(resource_dir.to_path_buf(), Arc::downgrade(&speller));
    Some(speller)
}

fn shared_german_speller_dictionary(resource_dir: &Path) -> Option<Arc<og_spell::Dictionary>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, std::sync::Weak<og_spell::Dictionary>>>,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut guard = cache.lock().unwrap();
    if let Some(dict) = guard.get(resource_dir).and_then(std::sync::Weak::upgrade) {
        return Some(dict);
    }
    let shared = og_tagger::german_tagger::shared_german_dictionary(resource_dir);
    let mut dict = match shared.as_ref() {
        Some(shared) => og_spell::Dictionary::from_shared_words(Arc::new(
            og_spell::morfologik_view::MorfologikView::new(shared.clone()),
        )),
        None => og_spell::Dictionary::new(),
    };
    if let Ok(extra) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt")) {
        for line in extra.lines() {
            for word in og_rules::spelling_support::prepare_line_for_speller(
                og_rules::spelling_support::SpellerVariant::German,
                line,
            ) {
                dict.add_word(&word);
            }
        }
    }
    for word in og_rules::spelling_support::german_hunspell_words(resource_dir) {
        dict.add_word(&word);
    }
    if shared.is_none() && dict.is_empty() {
        return None;
    }
    let dict = Arc::new(dict);
    guard.insert(resource_dir.to_path_buf(), Arc::downgrade(&dict));
    Some(dict)
}

fn wordlist_dictionary(resource_dir: &Path, dict_stem: &str) -> og_spell::Dictionary {
    match og_tagger::generic_tagger::shared_dictionary(resource_dir, dict_stem) {
        Some(shared) => og_spell::Dictionary::from_shared_words(Arc::new(
            og_spell::morfologik_view::MorfologikView::new(shared),
        )),
        None => og_spell::Dictionary::new(),
    }
}

fn variant_dictionary(hunspell_dir: &Path, country: &str) -> og_spell::Dictionary {
    let mut dict = og_spell::Dictionary::new();
    let mut sources = vec![
        format!("en_{country}.words.txt"),
        "spelling.txt".to_string(),
    ];
    sources.push(format!("spelling_en-{country}.txt"));
    for source in sources {
        if let Ok(words) = std::fs::read_to_string(hunspell_dir.join(source)) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    dict.add_word(word);
                }
            }
        }
    }
    dict
}

struct SentenceSplitterBridge(String);

impl SentenceTokenizer for SentenceSplitterBridge {
    fn split(&self, text: &str) -> Vec<SentenceRange> {
        use og_tokenizer::SentenceTokenizer as _;
        let sentences = match og_tokenizer::srx::srx_splitter(&self.0) {
            Some(srx) => srx.split(text),
            None => og_tokenizer::DefaultSentenceSplitter::new().split(text),
        };
        sentences
            .into_iter()
            .map(|s| SentenceRange {
                start: s.start(),
                end: s.end(),
            })
            .collect()
    }
}

enum WordTokenizerBridge {
    Default(bool),
    German,
    French(Arc<og_tokenizer::FrenchWordTokenizer>),
    Arabic,
    Dutch,
    CrimeanTatar,
    Esperanto,
    Spanish(crate::es::word_tokenizer::SpanishWordTokenizer),
    Portuguese(Arc<crate::pt::word_tokenizer::PortugueseWordTokenizer>),
    Polish(Arc<crate::pl::word_tokenizer::PolishWordTokenizer>),
    Russian(crate::ru::word_tokenizer::RussianWordTokenizer),
    Belarusian(crate::be::word_tokenizer::BelarusianWordTokenizer),
    Breton(crate::br::word_tokenizer::BretonWordTokenizer),
    Catalan(crate::ca::word_tokenizer::CatalanWordTokenizer),
    Greek(crate::el::word_tokenizer::GreekWordTokenizer),
    Persian(crate::fa::word_tokenizer::PersianWordTokenizer),
    Galician(crate::gl::word_tokenizer::GalicianWordTokenizer),
    Japanese(crate::ja::JapaneseWordTokenizer),
    Khmer(crate::km::KhmerWordTokenizer),
    Malayalam(crate::ml::MalayalamWordTokenizer),
    Romanian(crate::ro::RomanianWordTokenizer),
    Tamil(crate::ta::TamilWordTokenizer),
    Tagalog(crate::tl::TagalogWordTokenizer),
    Ukrainian(crate::uk::UkrainianWordTokenizer),
    Chinese(crate::zh::ChineseWordTokenizer),
}

impl WordTokenizerBridge {
    fn for_code(code: &str) -> Self {
        if code == "en" {
            return Self::Default(true);
        }
        if code == "de" {
            return Self::German;
        }
        if code == "ar" {
            return Self::Arabic;
        }
        if code == "nl" {
            return Self::Dutch;
        }
        if code == "crh" {
            return Self::CrimeanTatar;
        }
        if code == "eo" {
            return Self::Esperanto;
        }
        if code == "es" {
            return Self::Spanish(crate::es::word_tokenizer::SpanishWordTokenizer::new());
        }
        if code == "pt" {
            return crate::pt::word_tokenizer::shared_portuguese_word_tokenizer()
                .map(Self::Portuguese)
                .unwrap_or(Self::Default(false));
        }
        if code == "pl"
            && let Some(tagger) = crate::pl::tagger::PolishTagger::shared_arc()
        {
            return Self::Polish(Arc::new(
                crate::pl::word_tokenizer::PolishWordTokenizer::new(tagger),
            ));
        }
        if code == "ru" {
            return Self::Russian(crate::ru::word_tokenizer::RussianWordTokenizer::new());
        }
        if code == "be" {
            return Self::Belarusian(crate::be::word_tokenizer::BelarusianWordTokenizer::new());
        }
        if code == "br" {
            return Self::Breton(crate::br::word_tokenizer::BretonWordTokenizer::new());
        }
        if code == "ca" {
            return Self::Catalan(crate::ca::word_tokenizer::CatalanWordTokenizer::new());
        }
        if code == "el" {
            return Self::Greek(crate::el::word_tokenizer::GreekWordTokenizer::new());
        }
        if code == "fa" {
            return Self::Persian(crate::fa::word_tokenizer::PersianWordTokenizer::new());
        }
        if code == "gl" {
            return Self::Galician(crate::gl::word_tokenizer::GalicianWordTokenizer::new());
        }
        if code == "ja" {
            return Self::Japanese(crate::ja::JapaneseWordTokenizer::new());
        }
        if code == "km" {
            return Self::Khmer(crate::km::KhmerWordTokenizer::new());
        }
        if code == "ml" {
            return Self::Malayalam(crate::ml::MalayalamWordTokenizer::new());
        }
        if code == "ro" {
            return Self::Romanian(crate::ro::RomanianWordTokenizer::new());
        }
        if code == "ta" {
            return Self::Tamil(crate::ta::TamilWordTokenizer::new());
        }
        if code == "tl" {
            return Self::Tagalog(crate::tl::TagalogWordTokenizer::new());
        }
        if code == "uk" {
            return Self::Ukrainian(crate::uk::UkrainianWordTokenizer::new());
        }
        if code == "zh" {
            return Self::Chinese(crate::zh::ChineseWordTokenizer::new());
        }
        if code != "fr" {
            return Self::Default(false);
        }
        static FR: std::sync::OnceLock<Option<Arc<og_tokenizer::FrenchWordTokenizer>>> =
            std::sync::OnceLock::new();
        let cached = FR.get_or_init(|| {
            let tagger = find_lt_resource_dir().map(|lt_root| {
                let dir = lt_root.join("fr/src/main/resources/org/languagetool/resource/fr");
                Arc::new(og_tagger::GenericTagger::french(&dir))
            })?;
            Some(Arc::new(og_tokenizer::FrenchWordTokenizer::new(Arc::new(
                move |w: &str| tagger.is_word_tagged(w),
            ))))
        });
        match cached {
            Some(fr) => Self::French(fr.clone()),
            None => Self::Default(false),
        }
    }
}

impl WordTokenizer for WordTokenizerBridge {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        use og_tokenizer::WordTokenizer as _;
        let tokens = match self {
            Self::Default(true) => {
                static EN: std::sync::OnceLock<std::sync::Arc<og_tokenizer::DefaultWordTokenizer>> =
                    std::sync::OnceLock::new();
                let tokenizer =
                    EN.get_or_init(
                        || match og_tagger::english_tagger::shared_english_tagger() {
                            Some(tagger) => std::sync::Arc::new(
                                og_tokenizer::DefaultWordTokenizer::new_english(
                                    std::sync::Arc::new(move |w: &str| tagger.is_word_tagged(w)),
                                ),
                            ),
                            None => std::sync::Arc::new(og_tokenizer::DefaultWordTokenizer::new()),
                        },
                    );
                tokenizer.split_underscores(tokenizer.tokenize(text))
            }
            Self::Default(false) => {
                og_tokenizer::DefaultWordTokenizer::new_generic().tokenize(text)
            }
            Self::German => og_tokenizer::GermanWordTokenizer::new().tokenize(text),
            Self::French(fr) => fr.tokenize(text),
            Self::Arabic => {
                og_tokenizer::DefaultWordTokenizer::new_splitting_hyphen().tokenize(text)
            }
            Self::Dutch => og_tokenizer::DefaultWordTokenizer::new_dutch().tokenize(text),
            Self::CrimeanTatar => {
                let tz = og_tokenizer::DefaultWordTokenizer::new_generic();
                tz.split_trailing_hyphens(tz.tokenize(text))
            }
            Self::Esperanto => og_tokenizer::DefaultWordTokenizer::new_esperanto().tokenize(text),
            Self::Spanish(tz) => tz.tokenize(text),
            Self::Portuguese(tz) => {
                return tz.tokenize(text, offset);
            }
            Self::Polish(tz) => tz.tokenize(text),
            Self::Russian(tz) => tz.tokenize(text),
            Self::Belarusian(tz) => tz.tokenize(text),
            Self::Breton(tz) => tz.tokenize(text),
            Self::Catalan(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            Self::Greek(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            Self::Persian(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            Self::Galician(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            Self::Japanese(tz) => {
                return og_core::checker::WordTokenizer::tokenize(tz, text, offset);
            }
            Self::Khmer(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            Self::Malayalam(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            Self::Romanian(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            Self::Tamil(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            Self::Tagalog(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            Self::Ukrainian(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            Self::Chinese(tz) => {
                return og_core::checker::WordTokenizer::tokenize(tz, text, offset);
            }
        };
        tokens
            .into_iter()
            .map(|t| {
                let at = AnalyzedToken::new(t.text(), t.start() + offset, t.end() + offset);
                AnalyzedTokenReadings::new(at)
            })
            .collect()
    }
}

#[cfg(target_os = "linux")]
fn trim_released_engine_pages() {
    const TRIM_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);
    if og_core::releasable::trim_due(TRIM_INTERVAL) {
        unsafe { libc::malloc_trim(0) };
    }
}

#[cfg(not(target_os = "linux"))]
fn trim_released_engine_pages() {}

pub(crate) fn engine_build_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn find_lt_core_resource_dir() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(
            "languagetool/languagetool-core/src/main/resources/org/languagetool/resource",
        ),
        PathBuf::from(
            "../languagetool/languagetool-core/src/main/resources/org/languagetool/resource",
        ),
        PathBuf::from(
            "/home/agent/opengrammar-rs/languagetool/languagetool-core/src/main/resources/org/languagetool/resource",
        ),
    ];
    for candidate in &candidates {
        if candidate.exists() {
            return candidate.canonicalize().ok();
        }
    }
    None
}

pub fn find_lt_resource_dir() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("languagetool/languagetool-language-modules"),
        PathBuf::from("../languagetool/languagetool-language-modules"),
        PathBuf::from("../languagetool-language-modules"),
        PathBuf::from("../../languagetool-language-modules"),
        PathBuf::from("/home/agent/opengrammar-rs/languagetool/languagetool-language-modules"),
    ];
    for candidate in &candidates {
        if candidate.exists() {
            return candidate.canonicalize().ok();
        }
    }
    None
}

pub fn grammar_xml_path(lang_code: &str, lt_root: &Path) -> PathBuf {
    let module_name = lang_code.split('-').next().unwrap_or(lang_code);
    lt_root
        .join(module_name)
        .join("src/main/resources/org/languagetool/rules")
        .join(module_name)
        .join("grammar.xml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn shared_english() -> Arc<LanguageEngine> {
        LanguageEngine::shared_for_code("en").expect("english engine")
    }

    fn shared_german() -> Arc<LanguageEngine> {
        LanguageEngine::shared_for_code("de").expect("german engine")
    }

    fn build_engine(code: &str) -> Option<std::sync::Arc<LanguageEngine>> {
        LanguageEngine::shared_for_code(code)
    }

    #[test]
    fn matches_in_later_sentences_keep_absolute_offsets() {
        let _release_guard = crate::release_guard();
        let engine = shared_german();
        let text = "Das ist ein vollkommen korrekter Satz. Dies ist ein fhcsal.";
        let typo_at = text.find("fhcsal").expect("test text contains typo");
        let result = engine.check(text);
        let hit = result
            .matches
            .iter()
            .find(|m| m.length() == "fhcsal".len() && m.offset() == typo_at);
        assert!(
            hit.is_some(),
            "expected a match at byte {typo_at} for the typo in the second sentence, got {:?}",
            result
                .matches
                .iter()
                .map(|m| (m.rule().id().to_string(), m.offset(), m.length()))
                .collect::<Vec<_>>()
        );
        let hit = hit.expect("checked above");
        assert_eq!(&text[hit.offset()..hit.offset() + hit.length()], "fhcsal");
    }

    #[test]
    fn mother_tongue_adds_l2_rules_to_the_check() {
        let _release_guard = crate::release_guard();
        let base = shared_english();
        let plain = base.rules.len();
        let mut request = CheckRequest::new("Test", Language::English);
        request.mother_tongue = Some(Language::German);
        let _ = base.check_with_request(&request);
        let cached = base
            .l2_rules
            .lock()
            .unwrap()
            .get("de")
            .cloned()
            .unwrap_or_default();
        assert!(
            !cached.is_empty(),
            "expected grammar-l2-de.xml rules for motherTongue=de"
        );
        assert!(cached.len() <= plain || plain > 0);
    }

    #[test]
    fn paragraph_rules_fire_through_the_srx_pipeline() {
        let _release_guard = crate::release_guard();
        let engine = shared_english();
        let mut request = CheckRequest::new("Hello world. \n\nNext paragraph", Language::English);
        request.enabled_rules = vec![
            "WHITESPACE_PARAGRAPH".to_string(),
            "WHITESPACE_PARAGRAPH_BEGIN".to_string(),
        ];
        request.picky = true;
        let result = engine.check_with_request(&request);
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"WHITESPACE_PARAGRAPH"),
            "expected WHITESPACE_PARAGRAPH in {ids:?}"
        );
    }

    #[test]
    fn danish_engine_registers_with_tagger_and_rules() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("da").expect("Danish engine must be registered via for_code");
        assert!(
            engine.rule_count() > 10,
            "expected Danish rules to load, got {}",
            engine.rule_count()
        );
        assert!(
            engine.tagger.is_some(),
            "expected the Danish morfologik tagger to load"
        );
        let result = engine.check("Dette er en perfect sætning.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"HUNSPELL_RULE"),
            "expected Danish speller to fire on a typo, got {ids:?}"
        );
    }

    #[test]
    fn greek_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("el").expect("Greek engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "UPPERCASE_SENTENCE_START",
            "DOUBLE_PUNCTUATION",
            "COMMA_PARENTHESIS_WHITESPACE",
            "WORD_REPEAT_RULE",
            "GREEK_HOMONYMS_REPLACE",
            "EL_SPECIFIC_CASE",
            "GREEK_ORTHOGRAPHY_NUMERAL_STRESS",
            "EL_REDUNDANCY_REPLACE",
            "MORFOLOGIK_RULE_EL_GR",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Greek natives, got {ids:?}"
            );
        }
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "EL_UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
            "GREEK_WORD_REPEAT_BEGINNING_RULE",
            "TOO_LONG_SENTENCE",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Greek text-level natives, got {text_level_ids:?}"
            );
        }
        assert!(
            engine.tagger.is_some(),
            "expected the Greek morfologik tagger to load"
        );

        let result = engine.check("\u{3b1}\u{3c5}\u{3c4}\u{3ae} \u{3b5}\u{3af}\u{3bd}\u{3b1}\u{3b9} \u{3bc}\u{3b9}\u{3b1} \u{3b4}\u{3bf}\u{3ba}\u{3b9}\u{3bc}\u{3ae}.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"UPPERCASE_SENTENCE_START"),
            "expected UPPERCASE_SENTENCE_START on lowercase start, got {ids:?}"
        );
    }

    #[test]
    fn esperanto_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("eo").expect("Esperanto engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "HUNSPELL_RULE",
            "UPPERCASE_SENTENCE_START",
            "WORD_REPEAT_RULE",
            "DOUBLE_PUNCTUATION",
            "COMMA_PARENTHESIS_WHITESPACE",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Esperanto natives, got {ids:?}"
            );
        }
        assert_eq!(
            ids.iter().filter(|id| **id == "HUNSPELL_RULE").count(),
            1,
            "the eo arm's EsperantoSpellerRule must be the only hunspell speller, like \
             Esperanto.java:85: {ids:?}"
        );
        assert!(
            !ids.contains(&"HUNSPELL_RULE_EO"),
            "the generic wordlist speller must stay off for eo (Esperanto.java has no morfologik \
             dict): {ids:?}"
        );
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Esperanto text-level natives, got {text_level_ids:?}"
            );
        }
        assert!(
            engine.tagger.is_some(),
            "expected the EsperantoTagger to load"
        );

        let text = "La Mondaj Ludoj de 2013 estis plur-sporta evento.";
        let result = engine.check(text);
        let hunspell_spans: Vec<&str> = result
            .matches
            .iter()
            .filter(|m| m.rule().id() == "HUNSPELL_RULE")
            .map(|m| &text[m.offset()..m.offset() + m.length()])
            .collect();
        assert_eq!(
            hunspell_spans,
            vec!["plur-sporta"],
            "EsperantoTest.java:41-44 expects exactly the compound 'plur-sporta' flagged, got \
             all ids {:?}",
            result
                .matches
                .iter()
                .map(|m| m.rule().id())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn persian_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("fa").expect("Persian engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "PERSIAN_COMMA_PARENTHESIS_WHITESPACE",
            "PERSIAN_DOUBLE_PUNCTUATION",
            "PERSIAN_WORD_REPEAT_RULE",
            "FA_SIMPLE_REPLACE",
            "FA_SPACE_BEFORE_CONJUNCTION",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Persian natives, got {ids:?}"
            );
        }
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
            "TOO_LONG_SENTENCE",
            "PERSIAN_WORD_REPEAT_BEGINNING_RULE",
            "FA_WORD_COHERENCY",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Persian text-level natives, got {text_level_ids:?}"
            );
        }
        assert!(
            engine.tagger.is_none(),
            "Persian.java defines no tagger and fa carries no dict: {ids:?}"
        );
        assert!(
            !ids.iter()
                .any(|id| id.starts_with("MORFOLOGIK_RULE") || id.contains("HUNSPELL")),
            "Persian.java:81-94 installs no speller, fa has no hunspell dir: {ids:?}"
        );

        let text = "این کار برای برای تو بود.";
        let result = engine.check(text);
        let spans: Vec<&str> = result
            .matches
            .iter()
            .filter(|m| m.rule().id() == "PERSIAN_WORD_REPEAT_RULE")
            .map(|m| &text[m.offset()..m.offset() + m.length()])
            .collect();
        assert_eq!(
            spans,
            vec!["برای برای"],
            "PersianWordRepeatRule.java:50-51 flags the doubled word with both tokens, got \
             all ids {:?}",
            result
                .matches
                .iter()
                .map(|m| m.rule().id())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn irish_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("ga").expect("Irish engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "UPPERCASE_SENTENCE_START",
            "WORD_REPEAT_RULE",
            "WHITESPACE_PARAGRAPH_BEGIN",
            "GA_LOGAINM",
            "GA_PEOPLE",
            "GA_SPASANNA",
            "GA_COMPOUNDS",
            "GA_PRESTANDARD_REPLACE",
            "GA_REPLACE",
            "GA_FGB_EQ_REPLACE",
            "GA_ENGLISH_HOMOPHONE",
            "GA_DHA_NO_BEIRT",
            "GA_DATIVE_PLURALS_STD",
            "GA_SPECIFIC_CASE",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Irish natives, got {ids:?}"
            );
        }
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
            "TOO_LONG_SENTENCE",
            "TOO_LONG_PARAGRAPH",
            "WHITESPACE_PARAGRAPH",
            "PARAGRAPH_REPEAT_BEGINNING_RULE",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Irish text-level natives, got {text_level_ids:?}"
            );
        }
        assert!(
            engine.tagger.is_some(),
            "the GenericTagger manual added.txt path must install while irish.dict is absent"
        );
        assert!(
            !ids.contains(&"MORFOLOGIK_RULE_GA_IE"),
            "Irish.java:106 MorfologikIrishSpellerRule needs irish.dict which is absent, and \
             the ga spelling.txt wordlist (single letters) must not become a speller: {ids:?}"
        );

        let result = engine.check("Seo abairt cheart.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            !ids.iter()
                .any(|id| id.starts_with("MORFOLOGIK_RULE") || id.contains("HUNSPELL")),
            "a correct Irish sentence must stay unflagged by spellers, got {ids:?}"
        );
    }

    #[test]
    fn galician_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("gl").expect("Galician engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "HUNSPELL_RULE",
            "UPPERCASE_SENTENCE_START",
            "WHITESPACE_PARAGRAPH_BEGIN",
            "GL_SIMPLE_REPLACE",
            "GL_CAST_WORDS",
            "GL_REDUNDANCY_REPLACE",
            "GL_WORDINESS_REPLACE",
            "GL_BARBARISM_REPLACE",
            "GL_WIKIPEDIA_COMMON_ERRORS",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Galician natives, got {ids:?}"
            );
        }
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
            "TOO_LONG_SENTENCE",
            "TOO_LONG_PARAGRAPH",
            "WHITESPACE_PARAGRAPH",
            "EMPTY_LINE",
            "PARAGRAPH_REPEAT_BEGINNING_RULE",
            "PUNCTUATION_PARAGRAPH_END2",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Galician text-level natives, got {text_level_ids:?}"
            );
        }
        assert!(
            engine.tagger.is_some(),
            "GalicianTagger must replace the generic tagger so the gl tag() quirks apply"
        );

        let result = engine.check("O problema é acuciante.");
        let cast = result
            .matches
            .iter()
            .find(|m| m.rule().id() == "GL_CAST_WORDS")
            .expect("GL_CAST_WORDS must flag the castelanismo 'acuciante'");
        assert!(
            cast.replacements().iter().any(|r| r.value() == "urxente"),
            "got {:?}",
            cast.replacements()
        );
        let result = engine.check("O gato comeu qxzxv.");
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.rule().id() == "HUNSPELL_RULE"),
            "the wordlist speller must fire with Java's HunspellRule id, got {:?}",
            result
                .matches
                .iter()
                .map(|m| m.rule().id())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn icelandic_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("is").expect("Icelandic engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "HUNSPELL_NO_SUGGEST_RULE",
            "UPPERCASE_SENTENCE_START",
            "WORD_REPEAT_RULE",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Icelandic natives, got {ids:?}"
            );
        }
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Icelandic text-level natives, got {text_level_ids:?}"
            );
        }
        assert!(
            engine.tagger.is_none(),
            "Icelandic.java:63-65 uses DemoTagger and is carries no dict/added.txt: {ids:?}"
        );
        assert_eq!(
            ids.iter()
                .filter(|id| **id == "HUNSPELL_NO_SUGGEST_RULE")
                .count(),
            1,
            "Icelandic.java:74-82 installs exactly one speller, got {ids:?}"
        );
        assert!(
            !ids.contains(&"HUNSPELL_RULE"),
            "the is hunspell/spelling.txt is comment-only so the wordlist-speller block must be \
             skipped like fa, got {ids:?}"
        );

        let result = engine.check("Þetta er qvist setning.");
        let speller = result
            .matches
            .iter()
            .find(|m| m.rule().id() == "HUNSPELL_NO_SUGGEST_RULE")
            .expect("the hunspell speller must flag the non-word 'qvist'");
        assert!(
            speller.replacements().is_empty(),
            "HunspellNoSuggestionRule.java:73-76 never offers replacements, got {:?}",
            speller.replacements()
        );
        let result = engine.check("Hann sagði \"þetta er gott.");
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.rule().id() == "UNPAIRED_BRACKETS"),
            "the is brackets arm must track Java's 5 default symbol pairs incl. quotes, got {:?}",
            result
                .matches
                .iter()
                .map(|m| m.rule().id())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn japanese_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("ja").expect("Japanese engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in ["COMMA_PARENTHESIS_WHITESPACE", "DOUBLE_PUNCTUATION"] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Japanese natives, got {ids:?}"
            );
        }
        assert!(
            engine.rule_count() > 700,
            "the ja grammar.xml must load its 735 rules, got {}",
            engine.rule_count()
        );
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Japanese text-level natives, got {text_level_ids:?}"
            );
        }
        for id in ["DOUBLE_PUNCTUATION", "WHITESPACE_RULE"] {
            assert_eq!(
                ids.iter()
                    .chain(text_level_ids.iter())
                    .filter(|i| **i == id)
                    .count(),
                1,
                "Japanese.java:61-66 installs DoublePunctuation + MultipleWhitespace once: the \
                 generic path provides them, so japanese_native_rules() must not be installed \
                 twice, got {ids:?} + {text_level_ids:?}"
            );
        }
        assert!(
            engine.tagger.is_none(),
            "ja carries no resource dir: no dict/added.txt exists and the fused \
             WordTokenizerBridge::Japanese arm supplies the gosen readings"
        );

        let result = engine.check("したがって。");
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.rule().id() == "SITAGATTE_END"),
            "SITAGATTE_END = [したがって][SENT_END] must fire through the engine pipeline, got {:?}",
            result
                .matches
                .iter()
                .map(|m| m.rule().id())
                .collect::<Vec<_>>()
        );
        let text = "これわ文章を入力して'Check Text'をクリックすると、誤記を探すことができる。";
        let result = engine.check(text);
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"KOREWA"),
            "the JapaneseTest demo text must fire KOREWA on これわ, got {ids:?}"
        );
    }

    #[test]
    fn khmer_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("km").expect("Khmer engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "HUNSPELL_RULE",
            "KM_SIMPLE_REPLACE",
            "KM_WORD_REPEAT_RULE",
            "KM_SPACE_BEFORE_CONJUNCTION",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Khmer natives, got {ids:?}"
            );
        }
        assert!(
            !ids.contains(&"MORFOLOGIK_RULE"),
            "Java KhmerHunspellRule carries the plain HUNSPELL_RULE id, got {ids:?}"
        );
        let text_level_ids = engine.text_level_rule_ids();
        for expected in ["UNPAIRED_BRACKETS", "KM_UNPAIRED_BRACKETS"] {
            assert_eq!(
                text_level_ids.iter().filter(|i| **i == expected).count(),
                if expected == "KM_UNPAIRED_BRACKETS" {
                    1
                } else {
                    0
                },
                "the km brackets arm must install KhmerUnpairedBracketsRule instead of the \
                 generic [ ( {{ tracker, got {text_level_ids:?}"
            );
        }
        assert!(
            engine.tagger.is_some(),
            "KhmerTagger = BaseTagger over khmer.dict must load through the generic path"
        );

        let text = "ឃ្លា\u{200B}នេះ\u{200B}បង្ហាញ\u{200B}ពី\u{200B}ពី\u{200B}កំហុស\u{200B}វេយ្យាករណ៍ ដើម្បី\u{200B}បញ្ជាក់\u{200B}ពី\u{200B}ប្រសិទ្ធភាព\u{200B}របស់\u{200B}កម្មវិធី សំរាប់។";
        let result = engine.check(text);
        let replace = result
            .matches
            .iter()
            .find(|m| m.rule().id() == "KM_SIMPLE_REPLACE")
            .expect("KM_SIMPLE_REPLACE must flag the coherency.txt key 'សំរាប់'");
        assert!(
            replace.replacements().iter().any(|r| r.value() == "សម្រាប់"),
            "coherency.txt:1260 maps សំរាប់ to សម្រាប់, got {:?}",
            replace.replacements()
        );
    }

    #[test]
    fn lithuanian_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("lt").expect("Lithuanian engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "UPPERCASE_SENTENCE_START",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Lithuanian natives, got {ids:?}"
            );
        }
        assert!(
            !ids.contains(&"MORFOLOGIK_RULE_LT_LT"),
            "the checkout ships no lt dictionary (Java itself throws 'Cannot find dictionary \
             file'), so no speller may be installed, got {ids:?}"
        );
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Lithuanian text-level natives, got {text_level_ids:?}"
            );
        }
        for id in [
            "DOUBLE_PUNCTUATION",
            "COMMA_PARENTHESIS_WHITESPACE",
            "WHITESPACE_RULE",
        ] {
            assert_eq!(
                ids.iter()
                    .chain(text_level_ids.iter())
                    .filter(|i| **i == id)
                    .count(),
                1,
                "Lithuanian.java:72-81 installs each shared native exactly once: the generic \
                 path provides them, got {ids:?} + {text_level_ids:?}"
            );
        }
        assert!(
            engine.tagger.is_none(),
            "lt carries DemoTagger parity: no lt dictionary exists in the checkout"
        );
        assert!(
            ids.contains(&"BRAK_PRZECINKA_ZE"),
            "the lt grammar.xml must load through normalize_lt_rules_xml, got {ids:?}"
        );

        let text = "Jaroslavas pajuto kad jo geriausias draugas yra Mantas.";
        let result = engine.check(text);
        let hit = result
            .matches
            .iter()
            .find(|m| m.rule().id() == "BRAK_PRZECINKA_ZE")
            .expect("BRAK_PRZECINKA_ZE must fire on 'pajuto kad' through the engine pipeline");
        assert!(
            text[hit.offset()..hit.offset() + hit.length()].contains("pajuto kad"),
            "the match must cover the missing-comma span, got offset {} length {}",
            hit.offset(),
            hit.length()
        );
    }

    #[test]
    fn malayalam_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("ml").expect("Malayalam engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "MORFOLOGIK_RULE_ML_IN",
            "UPPERCASE_SENTENCE_START",
            "WORD_REPEAT_RULE",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Malayalam natives, got {ids:?}"
            );
        }
        assert_eq!(
            ids.iter()
                .filter(|i| **i == "MORFOLOGIK_RULE_ML_IN")
                .count(),
            1,
            "the generic wordlist speller must be suppressed for ml (it would draw on the tiny \
             malayalam.dict POS wordlist instead of hunspell/ml_IN.dict): only the module \
             MalayalamSpellerRule may install MORFOLOGIK_RULE_ML_IN, got {ids:?}"
        );
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Malayalam text-level natives, got {text_level_ids:?}"
            );
        }
        assert!(
            engine.tagger.is_some(),
            "MalayalamTagger = BaseTagger over malayalam.dict must load through the generic path"
        );

        let result = engine.check("Aagohw.");
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.rule().id() == "MORFOLOGIK_RULE_ML_IN"),
            "the ml_IN.dict speller must flag 'Aagohw' (MorfologikMalayalamSpellerRuleTest), got \
             {:?}",
            result
                .matches
                .iter()
                .map(|m| m.rule().id())
                .collect::<Vec<_>>()
        );
        let text = "ഞാന്\u{200d} അവളെ ഒരു പുസ്തകം നല്\u{200d}കി.";
        let result = engine.check(text);
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"അവളെ"),
            "the ml grammar rule അവളെ must fire on its own example through the engine pipeline, \
             got {ids:?}"
        );
    }

    #[test]
    fn slovak_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("sk").expect("Slovak engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "MORFOLOGIK_RULE_SK_SK",
            "UPPERCASE_SENTENCE_START",
            "WORD_REPEAT_RULE",
            "SK_COMPOUNDS",
            "POCITACOVE_UVODZOVKY",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Slovak rules, got {ids:?}"
            );
        }
        for id in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "MORFOLOGIK_RULE_SK_SK",
            "WHITESPACE_RULE",
            "UNPAIRED_BRACKETS",
        ] {
            assert_eq!(
                ids.iter()
                    .chain(engine.text_level_rule_ids().iter())
                    .filter(|i| **i == id)
                    .count(),
                1,
                "Slovak.java:85-99 installs each native exactly once: the generic path provides \
                 the shared five, slovak_native_rules() only the missing three, got {ids:?}"
            );
        }
        assert!(
            engine.tagger.is_some(),
            "SlovakTagger = BaseTagger over slovak.dict must load through the generic path"
        );

        let result = engine.check("toto je veta.");
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.rule().id() == "UPPERCASE_SENTENCE_START"),
            "UPPERCASE_SENTENCE_START must fire on a lowercase sentence start, got {:?}",
            result
                .matches
                .iter()
                .map(|m| m.rule().id())
                .collect::<Vec<_>>()
        );
        let text = "Toto je česko slovenský projekt.";
        let result = engine.check(text);
        let compound = result
            .matches
            .iter()
            .find(|m| m.rule().id() == "SK_COMPOUNDS")
            .expect("SK_COMPOUNDS must fire on 'česko slovenský'");
        assert!(
            compound
                .replacements()
                .iter()
                .any(|r| r.value() == "česko-slovenský"),
            "the spaced form must suggest the hyphenated form, got {:?}",
            compound.replacements()
        );
        let result = engine.check("Rozlíšenie obrazovky 800x600.");
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.rule().id().starts_with("ZNAK_NASOBENIA")),
            "grammar-typography.xml must load through sk_extra_rule_paths + the BOM normalizer: \
             the ZNAK_NASOBENIA group fires on '800x600', got {:?}",
            result
                .matches
                .iter()
                .map(|m| m.rule().id())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn slovenian_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("sl").expect("Slovenian engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "MORFOLOGIK_RULE_SL_SI",
            "UPPERCASE_SENTENCE_START",
            "WORD_REPEAT_RULE",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Slovenian natives, got {ids:?}"
            );
        }
        for id in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "MORFOLOGIK_RULE_SL_SI",
            "UPPERCASE_SENTENCE_START",
            "WORD_REPEAT_RULE",
            "UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
        ] {
            assert_eq!(
                ids.iter()
                    .chain(engine.text_level_rule_ids().iter())
                    .filter(|i| **i == id)
                    .count(),
                1,
                "Slovenian.java:65-76 installs each native exactly once: the generic path \
                 provides the shared ones, the sl arm only the missing speller + case + repeat, \
                 got {ids:?} + {:?}",
                engine.text_level_rule_ids()
            );
        }
        assert!(
            engine.tagger.is_none(),
            "sl ships no POS .dict, so the tagger stays None = Java's DemoTagger parity"
        );

        let text = "Kupil je npr. jajca, moko in mleko.";
        let result = engine.check(text);
        let flagged: Vec<(&str, &str)> = result
            .matches
            .iter()
            .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
            .collect();
        assert_eq!(
            flagged,
            vec![("MORFOLOGIK_RULE_SL_SI", "npr")],
            "JLanguageToolTest.testSlovenian expects exactly 1 match: the hunspell/sl_SI.dict \
             speller flags 'npr' (the dict stores 'npr.') while the Slovenian SRX ruleset keeps \
             the sentence unsplit"
        );
    }

    #[test]
    fn serbian_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("sr").expect("Serbian engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "UPPERCASE_SENTENCE_START",
            "WORD_REPEAT_RULE",
            "MORFOLOGIK_RULE_SR_EKAVIAN",
            "SR_EKAVIAN_SIMPLE_GRAMMAR_REPLACE_RULE",
            "SR_EKAVIAN_SIMPLE_STYLE_REPLACE_RULE",
            "INVALID_DATE",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Serbian rules (INVALID_DATE proves grammar-logical.xml \
                 loads through sr_extra_rule_paths), got {ids:?}"
            );
        }
        for id in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "MORFOLOGIK_RULE_SR_EKAVIAN",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
            "UNPAIRED_BRACKETS",
        ] {
            assert_eq!(
                ids.iter()
                    .chain(engine.text_level_rule_ids().iter())
                    .filter(|i| **i == id)
                    .count(),
                1,
                "Serbian.java:142-167 installs each native exactly once: the generic path \
                 provides the shared ones, the sr arm only the missing five, got {ids:?}"
            );
        }
        assert!(
            !ids.contains(&"MORFOLOGIK_RULE_SR_JEKAVIAN"),
            "the base Serbian language is ekavian (getDefaultLanguageVariant), so no jekavian \
             rule ids may leak in, got {ids:?}"
        );
        assert!(
            engine.tagger.is_some(),
            "EkavianTagger = BaseTagger over dictionary/ekavian/serbian.dict must be installed by \
             the sr arm (the dict lives in a variant subdirectory the generic stem cannot reach)"
        );
        assert!(
            engine.disambiguator.is_some(),
            "resource/sr/disambiguation.xml must load through the generic disambiguation branch"
        );

        let text = "То је било 31.11. Син је вишљи од оца. „Наводник";
        let result = engine.check(text);
        let flagged: Vec<(&str, &str)> = result
            .matches
            .iter()
            .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
            .collect();
        assert_eq!(
            flagged,
            vec![
                ("INVALID_DATE", "31.11."),
                ("SR_EKAVIAN_SIMPLE_GRAMMAR_REPLACE_RULE", "вишљи"),
                ("MORFOLOGIK_RULE_SR_EKAVIAN", "оца"),
                ("UNPAIRED_BRACKETS", "„"),
            ],
            "the INVALID_DATE rule from grammar-logical.xml, the ekavian grammar-replace native, \
             the ekavian speller and the Serbian unpaired-bracket symbols must all fire through \
             the engine pipeline"
        );
        let replace = result
            .matches
            .iter()
            .find(|m| m.rule().id() == "SR_EKAVIAN_SIMPLE_GRAMMAR_REPLACE_RULE")
            .expect("grammar replace match");
        assert!(
            replace.replacements().iter().any(|r| r.value() == "виши"),
            "SimpleGrammarEkavianReplaceRuleTest pins вишљи -> виши, got {:?}",
            replace.replacements()
        );
    }

    #[test]
    fn tamil_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("ta").expect("Tamil engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in ["COMMA_PARENTHESIS_WHITESPACE", "DOUBLE_PUNCTUATION"] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Tamil natives, got {ids:?}"
            );
        }
        assert!(
            ids.iter()
                .all(|i| !i.contains("SPELL") && !i.contains("MORFOLOGIK")),
            "Tamil.java:66-74 installs no speller, so no spelling rule id may appear, got {ids:?}"
        );
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "WHITESPACE_RULE",
            "TOO_LONG_SENTENCE",
            "SENTENCE_WHITESPACE",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Tamil text-level natives, got {text_level_ids:?}"
            );
        }
        assert!(
            !text_level_ids.contains(&"UNPAIRED_BRACKETS"),
            "Tamil.java carries no unpaired-brackets rule, got {text_level_ids:?}"
        );
        assert!(
            engine.tagger.is_some(),
            "TamilTagger = BaseTagger over tamil.dict must load through the generic path"
        );

        let text = "இந்த பெட்டியில் உங்கள் உரையை ஒட்டி சரிவர சோதிக்கிறதா என பாருங்கள். 'லேங்குவேஜ் டூல்' சில இலக்கணப் பிழைகளைச் சரியாக கண்டுபிடிக்கும். பல பிழைகளைப் பிடிக்க தடுமாறலாம்.";
        let result = engine.check(text);
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert_eq!(
            ids,
            vec![
                "Ends_in_A_1",
                "Ends_in_A_3",
                "Ends_in_A_3",
                "Ends_in_A_4",
                "Ends_in_A_2",
            ],
            "TamilTest.java:35-37 testDemoText pins exactly these five matches in this order"
        );
    }

    #[test]
    fn tagalog_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("tl").expect("Tagalog engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "DOUBLE_PUNCTUATION",
            "UPPERCASE_SENTENCE_START",
            "MORFOLOGIK_RULE_TL",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Tagalog natives (Tagalog.java:84-94), got {ids:?}"
            );
        }
        assert_eq!(
            ids.iter().filter(|id| **id == "MORFOLOGIK_RULE_TL").count(),
            1,
            "the tl natives arm installs the module speller over hunspell/tl_PH.dict, so the \
             generic wordlist speller over tagalog.dict must stay off (the pl/ast/br/da/ml/ta \
             exclusion), got {ids:?}"
        );
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "UNPAIRED_BRACKETS",
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Tagalog text-level natives, got {text_level_ids:?}"
            );
        }
        assert!(
            engine.tagger.is_some(),
            "TagalogTagger = BaseTagger over tagalog.dict must load through the generic path \
             (the postag rules need it)"
        );

        let text = "Ang LanguageTool ay maganda gamit sa araw-araw. Ang talatang ito ay nagpapakita ng ng kakayahan ng LanguageTool at hinahalimbawa kung paano ito gamitin. Litaw rin sa talatang ito na may mga bagaybagay na hindii pa kayang itama nng LanguageTool.";
        let result = engine.check(text);
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert_eq!(
            ids,
            vec![
                "ADJECTIVE-V_COMMON_NOUN",
                "NG_NG",
                "MORFOLOGIK_RULE_TL",
                "R_WORDS",
                "MORFOLOGIK_RULE_TL",
                "MORFOLOGIK_RULE_TL",
                "MORFOLOGIK_RULE_TL",
            ],
            "TagalogTest.java:33-37 testDemoText pins exactly these seven matches in this order"
        );
    }

    #[test]
    fn ukrainian_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("uk").expect("Ukrainian engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "UKRAINIAN_WORD_REPEAT_RULE",
            "DASH",
            "UK_HIDDEN_CHARS",
            "UK_MIXED_ALPHABETS",
            "UK_SIMPLE_REPLACE_SOFT",
            "UK_SIMPLE_REPLACE_RENAMED",
            "UK_SIMPLE_REPLACE",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Ukrainian natives (Ukrainian.java:138-179 minus the \
                 dictionary-gated entries), got {ids:?}"
            );
        }
        assert!(
            !ids.contains(&"DOUBLE_PUNCTUATION"),
            "Ukrainian.java:162 comments DoublePunctuationRule out, so uk must leave the generic \
             DoublePunctuation/CommaWhitespace block, got {ids:?}"
        );
        assert_eq!(
            ids.iter()
                .filter(|id| **id == "COMMA_PARENTHESIS_WHITESPACE")
                .count(),
            1,
            "the uk natives arm supplies UkrainianCommaWhitespaceRule, so the generic block would \
             duplicate the id, got {ids:?}"
        );
        assert!(
            !ids.contains(&"MORFOLOGIK_RULE_UK_UA"),
            "no ukrainian.dict and no hunspell/uk_UA.dict exist in this tree (the external \
             ua.net.nlp:morfologik-ukrainian-lt artifact), so no speller may install, got {ids:?}"
        );
        let text_level_ids = engine.text_level_rule_ids();
        for expected in ["UPPERCASE_SENTENCE_START", "WHITESPACE_RULE"] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Ukrainian text-level natives, got {text_level_ids:?}"
            );
        }
        assert!(
            !text_level_ids.contains(&"SENTENCE_WHITESPACE"),
            "Ukrainian.java:138-179 installs no SentenceWhitespaceRule and the natives already \
             carry MultipleWhitespace, so uk must leave the generic text-level block, got \
             {text_level_ids:?}"
        );
        assert!(
            !text_level_ids.contains(&"UNPAIRED_BRACKETS"),
            "Ukrainian.java installs no unpaired-brackets rule, got {text_level_ids:?}"
        );
        assert!(
            engine.rule_count() > 100,
            "expected the five grammar-*.xml extras to load, got {}",
            engine.rule_count()
        );

        let text = "УВАГА! Внизу наведено приклад тексту з помилками, які допоможе виправити LanguageTool. Будь-ласка, вставте тутт ваш текст, або перевірте цей текст на предмет помилок. Знайти всі помилки для LanguageTool є не по силах з багатьох причин але дещо він вам все таки підкаже. Порівняно з засобами перевірки орфографії LanguageTool також змайде граматичні та стильові проблеми. LanguageTool — ваш самий кращий помічник.";
        let result = engine.check(text);
        let flagged: Vec<(&str, &str)> = result
            .matches
            .iter()
            .map(|m| (m.rule().id(), &text[m.offset()..m.offset() + m.length()]))
            .collect();
        assert_eq!(
            flagged,
            vec![
                ("UK_SIMPLE_REPLACE", "Будь-ласка"),
                ("PO_VS_OTHER", "по силах"),
                ("comma_before_but", "причин але"),
                ("WORDS_WITH_DASH", "все таки"),
            ],
            "UkrainianTest.testLanguage oracles that survive without the POS dictionary: \
             UK_SIMPLE_REPLACE proving the natives arm, comma_before_but proving \
             grammar-grammar.xml loads, WORDS_WITH_DASH proving the extras load"
        );
    }

    #[test]
    fn chinese_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("zh").expect("Chinese engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in ["DOUBLE_PUNCTUATION", "COMMA_PARENTHESIS_WHITESPACE"] {
            assert!(
                ids.contains(&expected),
                "expected {expected} from the generic path (Chinese.java:82-87 installs \
                 DoublePunctuation + MultipleWhitespace, both covered generically - the ja \
                 no-double-install pattern), got {ids:?}"
            );
        }
        assert!(
            ids.iter()
                .all(|i| !i.contains("SPELL") && !i.contains("MORFOLOGIK")),
            "Chinese.java installs no speller and resource/zh carries no .dict, got {ids:?}"
        );
        let text_level_ids = engine.text_level_rule_ids();
        for expected in ["WHITESPACE_RULE", "SENTENCE_WHITESPACE"] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Chinese text-level natives, got {text_level_ids:?}"
            );
        }
        assert!(
            !text_level_ids.contains(&"UNPAIRED_BRACKETS"),
            "Chinese.java installs no unpaired-brackets rule, got {text_level_ids:?}"
        );
        assert!(
            engine.tagger.is_none(),
            "ChineseTagger is folded into ChineseWordTokenizer and resource/zh has no \
             .dict/added.txt, so the engine tagger stays None like ja"
        );
        assert!(
            engine.rule_count() > 600,
            "expected grammar.xml to load through the BOM normalizer, got {}",
            engine.rule_count()
        );

        let text = "将文本粘贴在此，或者检测以下文本：我和她去看了二部电影。";
        let result = engine.check(text);
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert_eq!(
            ids,
            vec!["wa5"],
            "ChineseTest.testLanguage expects exactly the wa5 match on '二部' (suggests 两部)"
        );
        let flagged = &text
            [result.matches[0].offset()..result.matches[0].offset() + result.matches[0].length()];
        assert_eq!(flagged, "二部");
        assert!(
            result.matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "两部"),
            "wa5 must suggest '两部': {:?}",
            result.matches[0].replacements()
        );
    }

    #[test]
    fn romanian_engine_registers_with_tagger_and_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("ro").expect("Romanian engine must be registered via for_code");
        assert!(
            engine.rule_count() > 10,
            "expected Romanian rules to load, got {}",
            engine.rule_count()
        );
        for expected in [
            "UPPERCASE_SENTENCE_START",
            "WORD_REPEAT_RULE",
            "RO_SIMPLE_REPLACE",
            "RO_COMPOUND",
            "MORFOLOGIK_RULE_RO_RO",
        ] {
            assert!(
                engine.rule_ids().contains(&expected),
                "expected {expected} in Romanian rules (Romanian.java:76-92 getRelevantRules), \
                 got {:?}",
                engine.rule_ids()
            );
        }
        assert!(
            engine
                .text_level_rule_ids()
                .contains(&"ROMANIAN_WORD_REPEAT_BEGINNING_RULE"),
            "the ro natives arm must add the text-level RomanianWordRepeatBeginningRule, got {:?}",
            engine.text_level_rule_ids()
        );
        assert!(
            engine.tagger.is_some(),
            "expected the Romanian morfologik tagger to load"
        );
        let result = engine.check("Nu am fost acasã.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"MORFOLOGIK_RULE_RO_RO"),
            "expected the Romanian speller to fire on a typo, got {ids:?}"
        );
    }

    #[test]
    fn swedish_engine_registers_with_tagger_and_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("sv").expect("Swedish engine must be registered via for_code");
        assert!(
            engine.rule_count() > 10,
            "expected Swedish rules to load, got {}",
            engine.rule_count()
        );
        assert!(
            engine.tagger.is_some(),
            "expected the Swedish morfologik tagger to load"
        );
        let result = engine.check("Han arbetar med datorrz.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"HUNSPELL_RULE"),
            "expected the Swedish speller to fire on a typo, got {ids:?}"
        );
    }

    #[test]
    fn russian_engine_registers_tagger_normalization_and_chunker() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("ru").expect("Russian engine must be registered via for_code");
        assert!(
            engine.rule_count() > 10,
            "expected Russian rules to load, got {}",
            engine.rule_count()
        );
        assert!(
            engine.tagger.is_some(),
            "expected the Russian morfologik tagger to load"
        );
        assert!(
            engine.post_chunker.is_some(),
            "expected the Russian post-disambiguation chunker"
        );
        let result = engine.check("Крепкий стол.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            !ids.contains(&"Unify_Adj_NN_case"),
            "correct ADJ+NN pair must not be flagged, got {ids:?}"
        );
    }

    #[test]
    fn french_engine_registers_native_rules() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("fr").expect("French engine must be registered via for_code");
        assert!(
            engine.rule_count() > 10,
            "expected French rules to load, got {}",
            engine.rule_count()
        );
        assert!(
            engine.tagger.is_some(),
            "expected the French morfologik tagger to load"
        );
        let result = engine.check("le quel est le bon ?");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.iter().any(|id| id.starts_with("FR_COMPOUNDS")),
            "expected FR_COMPOUNDS to fire on 'Congo Brazzaville', got {ids:?}"
        );
    }

    #[test]
    fn spanish_engine_registers_native_rules_and_tagger() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("es").expect("Spanish engine must be registered via for_code");
        assert!(
            engine.rule_count() > 10,
            "expected Spanish rules to load, got {}",
            engine.rule_count()
        );
        assert!(
            engine.tagger.is_some(),
            "expected the Spanish tagger to load"
        );
        let ids: Vec<String> = engine
            .text_level_rules
            .iter()
            .map(|r| r.id().to_string())
            .collect();
        assert!(
            ids.contains(&"ES_UNPAIRED_BRACKETS".to_string()),
            "expected SpanishUnpairedBracketsRule instead of the generic bracket rule, got {ids:?}"
        );
        assert!(
            !ids.contains(&"UNPAIRED_BRACKETS".to_string()),
            "the generic bracket rule must not be registered for es, got {ids:?}"
        );
        let result = engine.check("Qué pasa?");
        let match_ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            match_ids.contains(&"ES_QUESTION_MARK"),
            "expected ES_QUESTION_MARK to fire, got {match_ids:?}"
        );
    }

    #[test]
    fn portuguese_engine_registers_native_rules() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("pt").expect("pt engine must be registered via for_code");
        let ids: Vec<String> = engine.rules.iter().map(|r| r.id().to_string()).collect();
        for expected in ["PT_COMPOUNDS_POST_REFORM", "PT_BARBARISMS_REPLACE"] {
            assert!(
                ids.contains(&expected.to_string()),
                "missing {expected} in pt engine"
            );
        }
        let result = engine.check("Esta casa é velha. foi construida em 1950.");
        let match_ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            match_ids.contains(&"UPPERCASE_SENTENCE_START"),
            "expected UPPERCASE_SENTENCE_START to fire, got {match_ids:?}"
        );
    }

    #[test]
    fn portuguese_variant_engines_add_variant_rules() {
        let _release_guard = crate::release_guard();
        let base = build_engine("pt").expect("pt base engine");
        let pt_pt = base.portuguese_variant("pt-PT");
        let pt_br = base.portuguese_variant("pt-BR");
        assert!(pt_pt.rule_count() > base.rule_count());
        assert!(pt_br.rule_count() > base.rule_count());
        let pt_pt_ids: Vec<String> = pt_pt.rules.iter().map(|r| r.id().to_string()).collect();
        let pt_br_ids: Vec<String> = pt_br.rules.iter().map(|r| r.id().to_string()).collect();
        assert!(
            pt_pt_ids.contains(&"PT_AGREEMENT_REPLACE".to_string()),
            "pt-PT variant must carry PT_AGREEMENT_REPLACE"
        );
        assert!(
            !pt_br_ids.contains(&"PT_AGREEMENT_REPLACE".to_string()),
            "pt-BR variant must not carry PT_AGREEMENT_REPLACE"
        );
        assert_eq!(pt_pt.language.code(), "pt-PT");
        assert_eq!(pt_br.language.code(), "pt-BR");
    }

    #[test]
    fn portuguese_engine_disambiguates_through_the_global_xml() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("pt").expect("pt engine must be registered via for_code");
        assert!(
            engine.disambiguator.is_some(),
            "the pt arm must install the entity-expanded disambiguation.xml chain plus \
             disambiguation-global.xml (PortugueseHybridDisambiguator.java:47)"
        );
        let mut request = og_core::CheckRequest::new(
            "google.com.br é um excelente recurso.",
            og_core::Language::PortuguesePt,
        );
        request.enabled_rules = vec!["URL_INICIAL".to_string()];
        let result = engine.check_with_request(&request);
        let url_matches: Vec<&og_core::RuleMatch> = result
            .matches
            .iter()
            .filter(|m| m.rule().id() == "URL_INICIAL")
            .collect();
        assert_eq!(
            url_matches.len(),
            1,
            "URL_INICIAL (grammar.xml:16565) only fires when the GLOBAL_URL add-rules of \
             disambiguation-global.xml tag bare domains with _IS_URL readings"
        );
        assert_eq!((url_matches[0].offset(), url_matches[0].length()), (0, 13));
    }

    #[test]
    fn portuguese_pre_reform_variants_register_native_rules() {
        let _release_guard = crate::release_guard();
        let base = build_engine("pt").expect("pt base engine");
        for (code, expected_language) in [
            ("pt-AO", og_core::Language::PortugueseAo),
            ("pt-MZ", og_core::Language::PortugueseMz),
        ] {
            let variant = base.portuguese_variant(code);
            let ids: Vec<String> = variant.rules.iter().map(|r| r.id().to_string()).collect();
            assert!(
                ids.contains(&"PT_COMPOUNDS_PRE_REFORM".to_string()),
                "{code} must carry PreReformPortugueseCompoundRule (AngolaPortuguese.java:55)"
            );
            assert!(
                ids.contains(&"PT_PREAO_DASH_RULE".to_string()),
                "{code} must carry PreReformPortugueseDashRule (AngolaPortuguese.java:56)"
            );
            assert_eq!(variant.language, expected_language);
        }
    }

    #[test]
    fn dutch_engine_registers_native_rules_and_tagger() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("nl").expect("Dutch engine must be registered via for_code");
        assert!(
            engine.rule_count() > 10,
            "expected Dutch rules to load, got {}",
            engine.rule_count()
        );
        assert!(engine.tagger.is_some(), "expected the Dutch tagger to load");
        let ids: Vec<String> = engine.rules.iter().map(|r| r.id().to_string()).collect();
        for expected in [
            "MORFOLOGIK_RULE_NL_NL",
            "NL_COMPOUNDS",
            "NL_SIMPLE_REPLACE",
            "DUTCH_WRONG_WORD_IN_CONTEXT",
            "ALLEEN_NL",
        ] {
            assert!(
                ids.contains(&expected.to_string()),
                "missing {expected} in nl engine"
            );
        }
        assert_eq!(
            ids.iter()
                .filter(|id| id.as_str() == "DOUBLE_PUNCTUATION")
                .count(),
            1,
            "the generic and Dutch native sets must not duplicate DOUBLE_PUNCTUATION"
        );
        let text_level_ids: Vec<String> = engine
            .text_level_rules
            .iter()
            .map(|r| r.id().to_string())
            .collect();
        assert!(
            text_level_ids.contains(&"NL_WORD_COHERENCY".to_string()),
            "missing NL_WORD_COHERENCY in nl engine text-level rules"
        );
        let result = engine.check("FoobarWrongxx");
        let match_ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            match_ids.contains(&"MORFOLOGIK_RULE_NL_NL"),
            "expected the Dutch speller to fire, got {match_ids:?}"
        );
        let result = engine.check("organogram, organigram");
        let match_ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            match_ids.contains(&"NL_WORD_COHERENCY"),
            "expected NL_WORD_COHERENCY to fire on mixed spelling, got {match_ids:?}"
        );
    }

    #[test]
    fn polish_engine_registers_native_rules_and_tagger() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("pl").expect("Polish engine must be registered via for_code");
        assert!(
            engine.rule_count() > 10,
            "expected Polish rules to load, got {}",
            engine.rule_count()
        );
        assert!(
            engine.tagger.is_some(),
            "expected the Polish tagger to load"
        );
        let ids: Vec<String> = engine.rules.iter().map(|r| r.id().to_string()).collect();
        for expected in [
            "MORFOLOGIK_RULE_PL_PL",
            "PL_COMPOUNDS",
            "PL_SIMPLE_REPLACE",
            "PL_DASH_RULE",
            "PL_WORD_REPEAT",
        ] {
            assert!(
                ids.contains(&expected.to_string()),
                "missing {expected} in pl engine"
            );
        }
        assert_eq!(
            ids.iter()
                .filter(|id| id.as_str() == "MORFOLOGIK_RULE_PL_PL")
                .count(),
            1,
            "the generic speller block and the Polish native set must not duplicate MORFOLOGIK_RULE_PL_PL"
        );
        assert_eq!(
            ids.iter()
                .filter(|id| id.as_str() == "DOUBLE_PUNCTUATION")
                .count(),
            2,
            "DOUBLE_PUNCTUATION must come only from grammar.xml's 2-sub-rule rulegroup; Java Polish.getRelevantRules (Polish.java:91-107) adds no native DoublePunctuationRule"
        );
        let text_level_ids: Vec<String> = engine
            .text_level_rules
            .iter()
            .map(|r| r.id().to_string())
            .collect();
        for expected in ["PL_UNPAIRED_BRACKETS", "PL_WORD_COHERENCY"] {
            assert!(
                text_level_ids.contains(&expected.to_string()),
                "missing {expected} in pl engine text-level rules"
            );
        }
        let result = engine.check("To jest zdanie z bledem.");
        let match_ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            match_ids.contains(&"MORFOLOGIK_RULE_PL_PL"),
            "expected the Polish speller to fire, got {match_ids:?}"
        );
        let result = engine.check("Kupi\u{142}em grapefruity, a grejpfrut\u{f3}w nie lubi\u{119}.");
        let match_ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            match_ids.contains(&"PL_WORD_COHERENCY"),
            "expected PL_WORD_COHERENCY to fire on mixed spelling, got {match_ids:?}"
        );
    }

    #[test]
    fn spelling_global_chunker_shields_speller_and_replace_rules() {
        let _release_guard = crate::release_guard();
        let engine = shared_english();
        for text in [
            "There is Givenchy here.",
            "There is Frito-Lay here.",
            "We met Emmanuel Macron yesterday.",
        ] {
            let mut request = CheckRequest::new(text, Language::English);
            request.enabled_rules = vec!["MORFOLOGIK_RULE_EN_US".to_string()];
            let result = engine.check_with_request(&request);
            let hits: Vec<usize> = result.matches.iter().map(|m| m.offset()).collect();
            assert!(
                hits.is_empty(),
                "spelling_global entry must not be flagged for {text:?}, got {hits:?}"
            );
        }
    }

    #[test]
    fn italian_engine_wires_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("it").expect("Italian engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "WHITESPACE_PUNCTUATION",
            "UPPERCASE_SENTENCE_START",
            "ITALIAN_WORD_REPEAT_RULE",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Italian natives, got {ids:?}"
            );
        }

        let result = engine.check("questa \u{e8} una prova : con due punti");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"WHITESPACE_PUNCTUATION"),
            "expected WHITESPACE_PUNCTUATION on space before colon, got {ids:?}"
        );
        assert!(
            ids.contains(&"UPPERCASE_SENTENCE_START"),
            "expected UPPERCASE_SENTENCE_START on lowercase start, got {ids:?}"
        );

        let result = engine.check("Il lessico si andava via via modificando.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            !ids.contains(&"ITALIAN_WORD_REPEAT_RULE"),
            "'via via' must be ignored, got {ids:?}"
        );

        assert!(
            engine.synthesizer.is_some(),
            "Italian.java:113-115 wires ItalianSynthesizer.INSTANCE over italian_synth.dict"
        );

        let result = engine.check("Lu\u{ec} 7 ottobre 2014");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"DATE_WEEKDAY"),
            "Java flags 'Lu\u{ec}' (startsWith lu -> Monday vs actual Tuesday), got {ids:?}"
        );
    }

    #[test]
    fn asturian_engine_wires_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("ast").expect("Asturian engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "UPPERCASE_SENTENCE_START",
            "DOUBLE_PUNCTUATION",
            "COMMA_PARENTHESIS_WHITESPACE",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Asturian natives, got {ids:?}"
            );
        }
        assert_eq!(
            ids.iter()
                .filter(|id| **id == crate::ast::MORFOLOGIK_RULE_AST)
                .count(),
            1,
            "the ast arm's AsturianSpellerRule and the generic speller block must not both \
             register MORFOLOGIK_RULE_AST"
        );
        let text_level_ids = engine.text_level_rule_ids();
        assert!(
            text_level_ids.contains(&"UNPAIRED_BRACKETS")
                && text_level_ids.contains(&"WHITESPACE_RULE"),
            "expected brackets and whitespace text rules, got {text_level_ids:?}"
        );

        let result = engine.check("la casa ye grande.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"UPPERCASE_SENTENCE_START"),
            "expected UPPERCASE_SENTENCE_START on lowercase start, got {ids:?}"
        );

        let result = engine.check("\u{bf}Tas avec\u{e1}u colos correutores gramaticales?");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&crate::ast::MORFOLOGIK_RULE_AST),
            "expected the ast_ES.dict speller to flag 'avec\u{e1}u', got {ids:?}"
        );
    }

    #[test]
    fn catalan_engine_wires_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("ca").expect("Catalan engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "UPPERCASE_SENTENCE_START",
            "DOUBLE_PUNCTUATION",
            "COMMA_PARENTHESIS_WHITESPACE",
            "CATALAN_WORD_REPEAT_RULE",
            "CATALAN_WRONG_WORD_IN_CONTEXT",
            "CA_SIMPLE_REPLACE_SIMPLE",
            "CA_SIMPLE_REPLACE_MULTIWORDS",
            "PRONOMS_FEBLES_DUPLICATS",
            "CA_CHECKCASE",
            "ADVERBIS_MENT",
            "CA_COMPOUNDS",
            "CA_SIMPLE_REPLACE_DNV",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Catalan natives, got {ids:?}"
            );
        }
        assert!(
            !ids.contains(&"MORFOLOGIK_RULE_CA_ES"),
            "no morfologik dict for ca in this tree, the wordlist speller must stay off: {ids:?}"
        );
        let text_level_ids = engine.text_level_rule_ids();
        for expected in [
            "UNPAIRED_BRACKETS",
            "SENTENCE_WHITESPACE",
            "TOO_LONG_SENTENCE",
            "CA_UNPAIRED_QUESTION",
            "CA_UNPAIRED_EXCLAMATION",
            "CATALAN_WORD_REPEAT_BEGINNING_RULE",
            "CA_WORD_COHERENCY",
            "CA_REMOTE_RULE",
            "CA_SPLIT_LONG_SENTENCE",
            "PUNCTUATION_PARAGRAPH_END",
            "IGNORE_PROPER_NOUNS",
        ] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Catalan text-level natives, got {text_level_ids:?}"
            );
        }
        let result = engine.check("és.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert_eq!(
            ids,
            vec!["TOTS_ELS_DIACRITICS"],
            "Catalan.java:513 gives UPPERCASE_SENTENCE_START -300 while TOTS_ELS_DIACRITICS \
             falls to the base 0, so the CatalanMatchFilter's overlap resolution keeps the \
             diacritics rule exactly like CleanOverlappingFilter.java:90-99 - this locks the \
             ca match-filter wiring end-to-end through the real engine"
        );
        let result = engine.check("la casa és gran.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert_eq!(
            ids,
            vec!["CA_SPLIT_WORDS"],
            "with the manual added.txt dictionary every common word stays UNKNOWN so \
             CA_MULTITOKEN_SPELLING_FOUR fires pre-filter exactly like the module pipeline, \
             but Catalan.java:303 gives CA_SPLIT_WORDS 30 over the -95 of \
             Catalan.java:522-523, so the ca overlap resolution keeps only CA_SPLIT_WORDS: \
             {ids:?}"
        );
        let result = engine.check("Aix\u{f2} \u{e9}s (impossible. ");
        let brackets: Vec<_> = result
            .matches
            .iter()
            .filter(|m| m.rule().id() == "UNPAIRED_BRACKETS")
            .collect();
        assert_eq!(
            brackets.len(),
            1,
            "Catalan.java:118 registers CatalanUnpairedBracketsRule: {:?}",
            result.matches
        );
        assert!(
            brackets[0].message().contains("S\u{ed}mbol sense parella"),
            "the ca rule carries the Catalan message, not the generic engine one: {}",
            brackets[0].message()
        );
        let result = engine.check("\u{bb}Aix\u{f2} em porta a demanar-t'ho.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            !ids.contains(&"UNPAIRED_BRACKETS"),
            "the ca i==1 leading-closed-guillemet exemption holds in the engine frame (the old \
             shared crossed-guillemet set fired it): {ids:?}"
        );
    }

    #[test]
    fn catalan_engine_tokenizer_mounts_the_catalan_splitter() {
        let _release_guard = crate::release_guard();
        let texts = |code: &str| {
            WordTokenizerBridge::for_code(code)
                .tokenize("Emporta-te'ls a l'observatori dels mars", 0)
                .into_iter()
                .map(|t| t.token().token().to_string())
                .collect::<Vec<_>>()
        };
        let catalan = texts("ca");
        assert_eq!(
            catalan,
            vec![
                "Emporta",
                "-te",
                "'ls",
                " ",
                "a",
                " ",
                "l'",
                "observatori",
                " ",
                "de",
                "ls",
                " ",
                "mars"
            ],
            "CatalanWordTokenizerTest.java:65 expects the closed clitic class whole"
        );
        let generic = texts("xx");
        assert_ne!(catalan, generic);
        assert_eq!(generic[0], "Emporta-te");
        assert_eq!(generic[10], "dels");
    }

    #[test]
    fn belarusian_engine_wires_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("be").expect("Belarusian engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "UPPERCASE_SENTENCE_START",
            "DOUBLE_PUNCTUATION",
            "COMMA_PARENTHESIS_WHITESPACE",
            "BE_SIMPLE_REPLACE",
            "BE_SPECIFIC_CASE",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Belarusian natives, got {ids:?}"
            );
        }
        assert!(
            !engine.text_level_rule_ids().contains(&"UNPAIRED_BRACKETS"),
            "Belarusian.java registers no unpaired-brackets rule"
        );

        let result = engine.check("вар'ят вялікі.");
        let hit = result
            .matches
            .iter()
            .find(|m| m.rule().id() == "UPPERCASE_SENTENCE_START")
            .expect("UPPERCASE_SENTENCE_START must fire on the lowercase start");
        assert_eq!(hit.offset(), 0);
        assert_eq!(hit.length(), "вар'ят".len());
        assert_eq!(hit.replacements()[0].value(), "Вар'ят");
    }

    #[test]
    fn breton_engine_wires_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("br").expect("Breton engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "MORFOLOGIK_RULE_BR_FR",
            "UPPERCASE_SENTENCE_START",
            "DOUBLE_PUNCTUATION",
            "COMMA_PARENTHESIS_WHITESPACE",
            "BR_TOPO",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Breton natives, got {ids:?}"
            );
        }
        assert_eq!(
            ids.iter()
                .filter(|id| **id == "MORFOLOGIK_RULE_BR_FR")
                .count(),
            1,
            "the generic wordlist speller must stay excluded for br: {ids:?}"
        );
        let text_level_ids = engine.text_level_rule_ids();
        assert!(
            text_level_ids.contains(&"WHITESPACE_RULE")
                && text_level_ids.contains(&"SENTENCE_WHITESPACE"),
            "expected the shared whitespace text rules, got {text_level_ids:?}"
        );
        assert!(
            !text_level_ids.contains(&"UNPAIRED_BRACKETS"),
            "Breton.java registers no unpaired-brackets rule"
        );

        let result = engine.check("c’hlañvoc’h eo.");
        let hit = result
            .matches
            .iter()
            .find(|m| m.rule().id() == "UPPERCASE_SENTENCE_START")
            .expect("UPPERCASE_SENTENCE_START must fire on the lowercase start");
        assert_eq!(hit.offset(), 0);
        assert_eq!(hit.length(), "c’hlañvoc’h".len());
        assert_eq!(hit.replacements()[0].value(), "C’hlañvoc’h");
    }

    #[test]
    fn arabic_engine_wires_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine = build_engine("ar").expect("Arabic engine must be registered via for_code");
        let ids = engine.rule_ids();
        for expected in [
            "ARABIC_DOUBLE_PUNCTUATION",
            "ARABIC_COMMA_PARENTHESIS_WHITESPACE",
            "ARABIC_QM_WHITESPACE",
            "ARABIC_SC_WHITESPACE",
            "COMMA_PARENTHESIS_WHITESPACE",
            "ARABIC_WORD_REPEAT_RULE",
            "AR_DARJA_REPLACE",
            "AR_DIACRITICS_REPLACE",
            "AR_SIMPLE_REPLACE",
            "AR_HOMOPHONES_REPLACE",
            "AR_REDUNDANCY_REPLACE",
            "AR_WORDINESS_REPLACE",
            "ARABIC_WRONG_WORD_IN_CONTEXT",
            "AR_VERB_TRANSITIVE_IINDIRECT",
            "AR_INFLECTED_ONE_WORD",
        ] {
            assert!(
                ids.contains(&expected),
                "expected {expected} in Arabic natives, got {ids:?}"
            );
        }
        assert!(
            !ids.contains(&"DOUBLE_PUNCTUATION"),
            "Arabic.java:103-127 registers only ArabicDoublePunctuationRule, not the shared base, got {ids:?}"
        );
        let text_level_ids = engine.text_level_rule_ids();
        for expected in ["AR_WORD_COHERENCY", "TOO_LONG_SENTENCE"] {
            assert!(
                text_level_ids.contains(&expected),
                "expected {expected} in Arabic text-level rules, got {text_level_ids:?}"
            );
        }

        let result = engine.check("لقد.. نجحنا.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"ARABIC_DOUBLE_PUNCTUATION"),
            "expected ARABIC_DOUBLE_PUNCTUATION on '..', got {ids:?}"
        );

        let result = engine.check("نعم ؟ لقد نجحنا.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"ARABIC_QM_WHITESPACE"),
            "expected ARABIC_QM_WHITESPACE on space before '؟', got {ids:?}"
        );

        let result = engine.check("نفذت التعليمات خطوة خطوة.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            !ids.contains(&"ARABIC_WORD_REPEAT_RULE"),
            "'خطوة خطوة' must be ignored, got {ids:?}"
        );

        let result = engine.check("قال « ثم أغلق القوس");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"ARABIC_SC_WHITESPACE") && !ids.contains(&"UNPAIRED_BRACKETS"),
            "space around the guillemet must surface the quotes-whitespace match and clean_overlapping_filter must drop the shorter bracket match, got {ids:?}"
        );

        let result = engine.check("قال«ثم أغلق القوس");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            ids.contains(&"UNPAIRED_BRACKETS"),
            "unpaired Arabic guillemet without surrounding whitespace must be flagged, got {ids:?}"
        );
    }

    #[test]
    fn german_engine_loads_rules_and_checks_text() {
        let _release_guard = crate::release_guard();
        let engine = shared_german();
        assert!(
            engine.rule_count() > 100,
            "expected German rules to load, got {}",
            engine.rule_count()
        );
        let result = engine.check("Der Haus ist gross.");
        let ids: Vec<&str> = result.matches.iter().map(|m| m.rule().id()).collect();
        assert!(
            !ids.is_empty(),
            "expected German rules to fire on 'Der Haus ist gross.', got none"
        );
    }

    #[test]
    fn variant_dictionaries_follow_java_hunspell_differences() {
        let _release_guard = crate::release_guard();
        let lt_root = match find_lt_resource_dir() {
            Some(p) => p,
            None => return,
        };
        let hunspell_dir =
            lt_root.join("en/src/main/resources/org/languagetool/resource/en/hunspell");
        let us = variant_dictionary(&hunspell_dir, "US");
        let gb = variant_dictionary(&hunspell_dir, "GB");
        assert!(us.contains("color"));
        assert!(!us.contains("colour"));
        assert!(gb.contains("colour"));
        assert!(us.len() > 100_000 && gb.len() > 100_000);
    }

    #[test]
    fn german_variants_carry_their_own_morfologik_rule_and_variant_grammar() {
        let _release_guard = crate::release_guard();
        let base = shared_german();
        let base_ids: Vec<String> = base.rules.iter().map(|r| r.id().to_string()).collect();
        assert!(base_ids.iter().any(|i| i == "MORFOLOGIK_RULE_DE_DE"));

        let ch = base.german_variant("de-CH");
        let ch_ids: Vec<String> = ch.rules.iter().map(|r| r.id().to_string()).collect();
        assert!(
            ch_ids.iter().any(|i| i == "SWISS_GERMAN_SPELLER_RULE"),
            "de-CH must swap in its own speller rule"
        );
        assert!(!ch_ids.iter().any(|i| i == "MORFOLOGIK_RULE_DE_DE"));
        assert!(
            ch_ids.iter().any(|i| i == "GRUESSE_CH_SPELLING_RULE"),
            "de-CH must load de-CH/grammar.xml rules"
        );
        assert!(
            !ch_ids.iter().any(|i| i == "GRUESSE_SPELLING_RULE"),
            "de-DE-AT rules must not leak into de-CH"
        );
        assert_eq!(ch.language().code(), "de-CH");

        let at = base.german_variant("de-AT");
        let at_ids: Vec<String> = at.rules.iter().map(|r| r.id().to_string()).collect();
        assert!(
            at_ids.iter().any(|i| i == "AUSTRIAN_GERMAN_SPELLER_RULE"),
            "de-AT must swap in its own speller rule"
        );
        assert!(
            at_ids.iter().any(|i| i == "GRUESSE_SPELLING_RULE"),
            "de-AT must load de-DE-AT/grammar.xml rules"
        );
        assert!(
            !at_ids.iter().any(|i| i == "GRUESSE_CH_SPELLING_RULE"),
            "de-CH rules must not leak into de-AT"
        );
        assert_eq!(at.language().code(), "de-AT");
    }

    #[test]
    fn german_variant_spellers_cover_the_java_wordlist_oracles() {
        let _release_guard = crate::release_guard();
        let base = shared_german();

        let at = base.german_variant("de-AT");
        let at_speller_matches = |text: &str| -> Vec<(String, Vec<String>)> {
            at.check(text)
                .matches
                .iter()
                .filter(|m| m.rule().id() == "AUSTRIAN_GERMAN_SPELLER_RULE")
                .map(|m| {
                    (
                        text[m.offset()..m.offset() + m.length()].to_string(),
                        m.replacements()
                            .iter()
                            .map(|r| r.value().to_string())
                            .collect(),
                    )
                })
                .collect()
        };
        assert!(
            at_speller_matches("Shopbewertung").is_empty(),
            "Shopbewertung comes from the de_AT hunspell dict"
        );
        assert!(
            at_speller_matches("Wahlzuckerl").is_empty()
                && at_speller_matches("Wahlzuckerls").is_empty(),
            "Wahlzuckerl/NS comes from spelling-de-AT.txt"
        );
        assert_eq!(
            at_speller_matches("aifhdlidflifs").len(),
            1,
            "gibberish must be flagged"
        );

        let ch = base.german_variant("de-CH");
        let ch_speller_matches = |text: &str| -> Vec<(String, Vec<String>)> {
            ch.check(text)
                .matches
                .iter()
                .filter(|m| m.rule().id() == "SWISS_GERMAN_SPELLER_RULE")
                .map(|m| {
                    (
                        text[m.offset()..m.offset() + m.length()].to_string(),
                        m.replacements()
                            .iter()
                            .map(|r| r.value().to_string())
                            .collect(),
                    )
                })
                .collect()
        };
        for accepted in ["Shopbewertung", "Abwart", "Abwarts"] {
            assert!(
                ch_speller_matches(accepted).is_empty(),
                "{accepted} must be accepted (spelling.txt / spelling-de-CH.txt)"
            );
        }
        assert_eq!(
            ch_speller_matches("aifhdlidflifs").len(),
            1,
            "gibberish must be flagged"
        );
        let trottinettens = ch_speller_matches("Trottinettens");
        assert_eq!(trottinettens.len(), 1, "Trottinettens is not in any list");
        assert_eq!(
            trottinettens[0].1.first().map(String::as_str),
            Some("Trottinetten"),
            "first suggestion must be Trottinetten (Trottinette/N in spelling-de-CH.txt)"
        );
    }

    #[test]
    fn regional_variants_carry_their_own_morfologik_rule() {
        let _release_guard = crate::release_guard();
        let base = shared_english();
        let ids: Vec<String> = base.rules.iter().map(|r| r.id().to_string()).collect();
        assert!(ids.iter().any(|i| i == "MORFOLOGIK_RULE_EN_US"));
        assert!(!ids.iter().any(|i| i == "MORFOLOGIK_RULE_EN_GB"));

        let gb = base.variant("en-GB");
        let gb_ids: Vec<String> = gb.rules.iter().map(|r| r.id().to_string()).collect();
        assert!(gb_ids.iter().any(|i| i == "MORFOLOGIK_RULE_EN_GB"));
        assert!(!gb_ids.iter().any(|i| i == "MORFOLOGIK_RULE_EN_US"));
        assert_eq!(gb.language().code(), "en-GB");
    }

    #[test]
    fn test_language_engine_basic() {
        let mut engine = LanguageEngine::new(Language::English);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST_RULE" name="Test">
                    <pattern>
                        <token>their</token>
                        <token>is</token>
                    </pattern>
                    <message>Did you mean <suggestion>there is</suggestion>?</message>
                </rule>
            </category>
        </rules>"#;

        let count = engine.load_xml_rules(xml).unwrap();
        assert_eq!(count, 1);
        assert_eq!(engine.rule_count(), 1);

        let result = engine.check("their is a problem");
        assert!(!result.matches.is_empty(), "Expected at least one match");
        assert_eq!(result.matches[0].rule().id(), "TEST_RULE");
    }

    #[test]
    fn test_language_engine_no_match() {
        let mut engine = LanguageEngine::new(Language::English);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="NO_MATCH_RULE" name="No match test">
                    <pattern>
                        <token>xyzzy</token>
                    </pattern>
                    <message>This should not match normal text.</message>
                </rule>
            </category>
        </rules>"#;

        engine.load_xml_rules(xml).unwrap();
        let result = engine.check("This is a normal sentence.");
        assert!(result.matches.is_empty());
    }

    #[test]
    fn test_language_engine_multiple_rules() {
        let mut engine = LanguageEngine::new(Language::English);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="RULE_A" name="Rule A">
                    <pattern>
                        <token regexp="yes">teh</token>
                    </pattern>
                    <message>Did you mean <suggestion>the</suggestion>?</message>
                </rule>
                <rule id="RULE_B" name="Rule B">
                    <pattern>
                        <token>their</token>
                        <token>is</token>
                    </pattern>
                    <message>Did you mean <suggestion>there is</suggestion>?</message>
                </rule>
            </category>
        </rules>"#;

        let count = engine.load_xml_rules(xml).unwrap();
        assert_eq!(count, 2);

        let result = engine.check("teh their is");
        assert!(
            result.matches.len() >= 2,
            "Expected at least 2 matches, got {}",
            result.matches.len()
        );
    }

    #[test]
    fn test_language_engine_with_native_rules() {
        use og_rules::native_rules::WordRepeatRule;
        let mut engine = LanguageEngine::new(Language::English);
        engine.add_rule(Arc::new(WordRepeatRule::new()));

        let result = engine.check("the the quick brown fox");
        assert!(!result.matches.is_empty(), "Expected word repeat detection");
    }

    #[test]
    fn test_load_real_english_grammar() {
        let _release_guard = crate::release_guard();
        let lt_root = match find_lt_resource_dir() {
            Some(p) => p,
            None => {
                eprintln!("Skipping: LT resource dir not found");
                return;
            }
        };

        let grammar_path = grammar_xml_path("en", &lt_root);
        if !grammar_path.exists() {
            eprintln!(
                "Skipping: English grammar.xml not found at {:?}",
                grammar_path
            );
            return;
        }

        let engine = shared_english();
        let count = engine.rule_count();
        println!("Loaded {} rules from English grammar.xml", count);
        assert!(
            count > 4000,
            "Expected the real English grammar, got {count}"
        );

        let result = engine.check("This is a test.");
        assert_eq!(result.language.code, "en-US");
    }

    #[test]
    fn test_engine_sentence_splitting() {
        let mut engine = LanguageEngine::new(Language::English);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="CAPPED" name="Sentence start">
                    <pattern>
                        <token>hello</token>
                    </pattern>
                    <message>Capitalize.</message>
                </rule>
            </category>
        </rules>"#;
        engine.load_xml_rules(xml).unwrap();

        let result = engine.check("Well hello there. And hello again.");
        assert!(!result.matches.is_empty());
    }
    #[test]
    fn xml_rules_loaded_from_file_carry_source_file() {
        let mut engine = LanguageEngine::new(Language::English);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="SRC_FILE_RULE" name="Test">
                    <pattern>
                        <token>tehest</token>
                    </pattern>
                    <message>Did you mean <suggestion>test</suggestion>?</message>
                </rule>
            </category>
        </rules>"#;
        let path = std::env::temp_dir().join("og-engine-source-file-test-grammar.xml");
        std::fs::write(&path, xml).unwrap();
        let count = engine.load_xml_file(&path).unwrap();
        assert_eq!(count, 1);

        let result = engine.check("tehest");
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].rule().id(), "SRC_FILE_RULE");
        assert_eq!(
            result.matches[0].rule().source_file(),
            Some("og-engine-source-file-test-grammar.xml")
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn crimean_tatar_engine_registers_java_relevant_natives() {
        let _release_guard = crate::release_guard();
        let engine =
            build_engine("crh").expect("Crimean Tatar engine must be registered via for_code");
        for expected in ["UPPERCASE_SENTENCE_START", "WHITESPACE_PARAGRAPH_BEGIN"] {
            assert!(
                engine.rule_ids().contains(&expected),
                "expected {expected} in Crimean Tatar rules (CrimeanTatar.java getRelevantRules), got {:?}",
                engine.rule_ids()
            );
        }
        assert!(
            engine
                .text_level_rule_ids()
                .contains(&"WHITESPACE_PARAGRAPH"),
            "the crh arm must add the text-level WhiteSpaceBeforeParagraphEndRule, got {:?}",
            engine.text_level_rule_ids()
        );
    }
}
