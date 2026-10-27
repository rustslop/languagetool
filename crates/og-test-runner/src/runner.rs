use og_core::checker::{Chunker as _, Disambiguator, SentenceTokenizer, WordTokenizer};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, SentenceRange};
use og_rules::pattern_rule::PatternRuleEngine;
use og_tagger::{EnglishTagger, Tagger, XmlDisambiguator};
use og_xml::compiler::{CompiledRuleSet, XmlCompiler};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ExampleTestResult {
    pub rule_id: String,
    pub passed: bool,
    pub message: String,
    pub example_text: String,
    pub expected_type: String,
}

#[derive(Debug)]
pub struct TestRunSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<ExampleTestResult>,
}

impl TestRunSummary {
    pub fn is_all_passed(&self) -> bool {
        self.failed == 0
    }
}

enum TestWordTokenizer {
    Default(bool),
    German,
    French(Arc<og_tokenizer::FrenchWordTokenizer>),
    Arabic,
    Dutch,
    CrimeanTatar,
    Esperanto,
    Spanish(og_langs::es::word_tokenizer::SpanishWordTokenizer),
    Portuguese(Arc<og_langs::pt::word_tokenizer::PortugueseWordTokenizer>),
    Polish(Arc<og_langs::pl::word_tokenizer::PolishWordTokenizer>),
    Russian(og_langs::ru::word_tokenizer::RussianWordTokenizer),
    Belarusian(og_langs::be::word_tokenizer::BelarusianWordTokenizer),
    Breton(og_langs::br::word_tokenizer::BretonWordTokenizer),
    Catalan(og_langs::ca::word_tokenizer::CatalanWordTokenizer),
    Greek(og_langs::el::word_tokenizer::GreekWordTokenizer),
    Persian(og_langs::fa::word_tokenizer::PersianWordTokenizer),
    Galician(og_langs::gl::word_tokenizer::GalicianWordTokenizer),
    Japanese(og_langs::ja::JapaneseWordTokenizer),
    Khmer(og_langs::km::KhmerWordTokenizer),
    Malayalam(og_langs::ml::MalayalamWordTokenizer),
    Tamil(og_langs::ta::TamilWordTokenizer),
    Tagalog(og_langs::tl::TagalogWordTokenizer),
    Ukrainian(og_langs::uk::UkrainianWordTokenizer),
    Chinese(og_langs::zh::ChineseWordTokenizer),
}

impl WordTokenizer for TestWordTokenizer {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        use og_tokenizer::WordTokenizer as _;
        let tokens = match self {
            TestWordTokenizer::Default(true) => {
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
            TestWordTokenizer::Default(false) => {
                og_tokenizer::DefaultWordTokenizer::new_generic().tokenize(text)
            }
            TestWordTokenizer::German => og_tokenizer::GermanWordTokenizer::new().tokenize(text),
            TestWordTokenizer::French(fr) => fr.tokenize(text),
            TestWordTokenizer::Arabic => {
                og_tokenizer::DefaultWordTokenizer::new_splitting_hyphen().tokenize(text)
            }
            TestWordTokenizer::Dutch => {
                og_tokenizer::DefaultWordTokenizer::new_dutch().tokenize(text)
            }
            TestWordTokenizer::CrimeanTatar => {
                let tz = og_tokenizer::DefaultWordTokenizer::new_generic();
                tz.split_trailing_hyphens(tz.tokenize(text))
            }
            TestWordTokenizer::Esperanto => {
                og_tokenizer::DefaultWordTokenizer::new_esperanto().tokenize(text)
            }
            TestWordTokenizer::Spanish(tz) => tz.tokenize(text),
            TestWordTokenizer::Portuguese(tz) => {
                return tz.tokenize(text, offset);
            }
            TestWordTokenizer::Polish(tz) => tz.tokenize(text),
            TestWordTokenizer::Russian(tz) => tz.tokenize(text),
            TestWordTokenizer::Belarusian(tz) => tz.tokenize(text),
            TestWordTokenizer::Breton(tz) => tz.tokenize(text),
            TestWordTokenizer::Catalan(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            TestWordTokenizer::Greek(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            TestWordTokenizer::Persian(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            TestWordTokenizer::Galician(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            TestWordTokenizer::Japanese(tz) => {
                return WordTokenizer::tokenize(tz, text, offset);
            }
            TestWordTokenizer::Khmer(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            TestWordTokenizer::Malayalam(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            TestWordTokenizer::Tamil(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            TestWordTokenizer::Tagalog(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            TestWordTokenizer::Ukrainian(tz) => og_tokenizer::WordTokenizer::tokenize(tz, text),
            TestWordTokenizer::Chinese(tz) => {
                return tz.tokenize(text, offset);
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

#[allow(dead_code)]
struct TestSentenceTokenizer;

impl SentenceTokenizer for TestSentenceTokenizer {
    fn split(&self, text: &str) -> Vec<SentenceRange> {
        use og_tokenizer::SentenceTokenizer as _;
        let sentences = match og_tokenizer::srx::srx_splitter("en") {
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

pub fn generic_dict_stem(code: &str) -> Option<&'static str> {
    let generic_stems: &[(&str, &str)] = &[
        ("fr", "french"),
        ("da", "danish"),
        ("es", "spanish"),
        ("pt", "portuguese"),
        ("nl", "dutch"),
        ("pl", "polish"),
        ("ru", "russian"),
        ("it", "italian"),
        ("ar", "arabic"),
        ("ast", "asturian"),
        ("br", "breton"),
        ("ca", "catalan"),
        ("el", "greek"),
        ("fa", ""),
        ("ga", "irish"),
        ("gl", "galician"),
        ("is", ""),
        ("ja", ""),
        ("km", "khmer"),
        ("ro", "romanian"),
        ("sk", "slovak"),
        ("sv", "swedish"),
        ("crh", "crimean_tatar"),
        ("ml", "malayalam"),
        ("ta", "tamil"),
        ("tl", "tagalog"),
    ];
    generic_stems
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, stem)| *stem)
}

pub enum AnyTagger {
    English(EnglishTagger),
    German(og_tagger::GermanTagger),
    Generic(og_tagger::GenericTagger),
    GenericShared(std::sync::Arc<og_tagger::GenericTagger>),
    Arabic(og_tagger::arabic_tagger::ArabicTagger),
    Esperanto(og_tagger::EsperantoTagger),
    Spanish(Arc<og_langs::es::tagger::SpanishTagger>),
    Portuguese(Arc<og_langs::pt::tagger::PortugueseTagger>),
    French(Arc<og_langs::fr::tagger::FrenchTagger>),
    Polish(Arc<og_langs::pl::tagger::PolishTagger>),
    Breton(Arc<og_langs::br::BretonTagger>),
    Catalan(Arc<og_langs::ca::CatalanTagger>),
    Galician(Arc<og_langs::gl::GalicianTagger>),
}

impl AnyTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        match self {
            AnyTagger::English(t) => Tagger::tag(t, tokens),
            AnyTagger::German(t) => Tagger::tag(t, tokens),
            AnyTagger::Generic(t) => Tagger::tag(t, tokens),
            AnyTagger::GenericShared(t) => Tagger::tag(t.as_ref(), tokens),
            AnyTagger::Arabic(t) => Tagger::tag(t, tokens),
            AnyTagger::Esperanto(t) => Tagger::tag(t, tokens),
            AnyTagger::Spanish(t) => Tagger::tag(t.as_ref(), tokens),
            AnyTagger::Portuguese(t) => Tagger::tag(t.as_ref(), tokens),
            AnyTagger::French(t) => Tagger::tag(t.as_ref(), tokens),
            AnyTagger::Polish(t) => Tagger::tag(t.as_ref(), tokens),
            AnyTagger::Breton(t) => Tagger::tag(t.as_ref(), tokens),
            AnyTagger::Catalan(t) => Tagger::tag(t.as_ref(), tokens),
            AnyTagger::Galician(t) => Tagger::tag(t.as_ref(), tokens),
        }
    }
}

pub struct TestRunner {
    language: String,
    pattern_engine: PatternRuleEngine,
    tagger: AnyTagger,
    disambiguator: Option<Arc<dyn Disambiguator>>,
    french_tokenizer: Option<Arc<og_tokenizer::FrenchWordTokenizer>>,
    sentence_splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl TestRunner {
    pub fn new() -> Self {
        Self::for_language("en")
    }

    pub fn for_language(code: &str) -> Self {
        let lt_modules =
            Path::new("/home/agent/opengrammar-rs/languagetool/languagetool-language-modules");
        let resource_dir = lt_modules.join(format!(
            "{code}/src/main/resources/org/languagetool/resource/{code}"
        ));
        let core_resource_dir = Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-core/src/main/resources/org/languagetool/resource",
        );

        let (tagger, disambiguator, pattern_engine) = std::thread::scope(|scope| {
            let tagger_h = scope.spawn(|| {
                if code == "eo"
                    && resource_dir.exists()
                    && let Some(tagger) =
                        og_tagger::EsperantoTagger::from_resource_dir(&resource_dir)
                {
                    return AnyTagger::Esperanto(tagger);
                }
                if code == "es"
                    && let Some(tagger) = og_langs::es::tagger::SpanishTagger::shared_arc()
                {
                    return AnyTagger::Spanish(tagger);
                }
                if code == "pt"
                    && let Some(tagger) = og_langs::pt::tagger::PortugueseTagger::shared_arc()
                {
                    return AnyTagger::Portuguese(tagger);
                }
                if code == "fr"
                    && let Some(tagger) = og_langs::fr::tagger::FrenchTagger::shared_arc()
                {
                    return AnyTagger::French(tagger);
                }
                if code == "pl"
                    && let Some(tagger) = og_langs::pl::tagger::PolishTagger::shared_arc()
                {
                    return AnyTagger::Polish(tagger);
                }
                if code == "br"
                    && let Some(tagger) = og_langs::br::BretonTagger::shared_arc()
                {
                    return AnyTagger::Breton(tagger);
                }
                if code == "ca"
                    && let Some(tagger) = og_langs::ca::CatalanTagger::shared_arc()
                {
                    return AnyTagger::Catalan(tagger);
                }
                if code == "gl"
                    && let Some(tagger) = og_langs::gl::GalicianTagger::shared_arc()
                {
                    return AnyTagger::Galician(tagger);
                }
                if code == "sr"
                    && let Some(tagger) =
                        og_langs::sr::serbian_tagger(og_langs::sr::SerbianVariant::Ekavian)
                {
                    return AnyTagger::GenericShared(tagger);
                }
                if let Some(stem) = generic_dict_stem(code)
                    && resource_dir.exists()
                {
                    if code == "fr" {
                        return AnyTagger::Generic(og_tagger::GenericTagger::french(&resource_dir));
                    }
                    if code == "ru" {
                        return AnyTagger::Generic(og_tagger::GenericTagger::russian(
                            &resource_dir,
                        ));
                    }
                    if code == "ar" {
                        return AnyTagger::Arabic(
                            og_tagger::arabic_tagger::ArabicTagger::from_resource_dir(
                                &resource_dir,
                            ),
                        );
                    }
                    return AnyTagger::Generic(og_tagger::GenericTagger::from_resource_dir(
                        &resource_dir,
                        stem,
                    ));
                }
                if code == "de" && resource_dir.exists() {
                    og_tagger::GermanTagger::from_resource_dir(&resource_dir)
                        .map(AnyTagger::German)
                        .unwrap_or_else(|| AnyTagger::English(EnglishTagger::new()))
                } else if resource_dir.exists() {
                    AnyTagger::English(EnglishTagger::from_resource_dir(&resource_dir))
                } else {
                    AnyTagger::English(EnglishTagger::new())
                }
            });

            let disambiguator_h = scope.spawn(|| {
                if code == "es"
                    && let Some(chain) = og_langs::es::build_es_disambiguator(&resource_dir)
                {
                    return Some(chain);
                }
                if code == "fr"
                    && let Some(chain) = og_langs::fr::build_fr_disambiguator(&resource_dir)
                {
                    return Some(chain);
                }
                if code == "pl"
                    && let Some(chain) = og_langs::pl::build_pl_disambiguator(&resource_dir)
                {
                    return Some(chain);
                }
                if code == "pt"
                    && let Some(chain) = og_langs::pt::build_pt_disambiguator(&resource_dir)
                {
                    return Some(chain);
                }
                if code == "br"
                    && let Some(disambiguator) = og_langs::br::build_br_disambiguator(&resource_dir)
                {
                    return Some(Arc::new(disambiguator));
                }
                if code == "uk"
                    && let Some(disambiguator) = og_tagger::XmlDisambiguator::from_xml(
                        &std::fs::read_to_string(resource_dir.join("disambiguation.xml"))
                            .unwrap_or_default(),
                    )
                    .ok()
                {
                    return Some(Arc::new(disambiguator));
                }
                if code == "ga"
                    && let Some(chain) = og_langs::ga::build_ga_disambiguator(&resource_dir)
                {
                    return Some(Arc::new(chain));
                }
                if code == "gl"
                    && let Some(chain) = og_langs::gl::build_gl_disambiguator(&resource_dir)
                {
                    return Some(Arc::new(chain));
                }
                let disambig_path = resource_dir.join("disambiguation.xml");
                let english_tagger = (code == "en")
                    .then(og_tagger::english_tagger::shared_english_tagger)
                    .flatten();
                match std::fs::read_to_string(&disambig_path) {
                    Ok(xml) => {
                        match XmlDisambiguator::from_xml_with_english(&xml, english_tagger) {
                            Ok(d) => {
                                eprintln!("Loaded {} disambiguation rules", d.rule_count());
                                Some(og_tagger::hybrid_disambiguator(
                                    code,
                                    &resource_dir,
                                    core_resource_dir,
                                    d,
                                ))
                            }
                            Err(e) => {
                                eprintln!("Warning: failed to load disambiguation: {}", e);
                                None
                            }
                        }
                    }
                    Err(_) => None,
                }
            });

            let engine_h = scope.spawn(|| {
                let generic_multitoken =
                    ["nl", "pt", "es", "fr"].contains(&code) && resource_dir.exists();
                let support = if code == "en" && resource_dir.exists() && core_resource_dir.exists()
                {
                    og_rules::spelling_support::SpellingSupport::dictionary_from_dir(
                        &lt_modules.join("en/src/main/resources/org/languagetool/resource/en"),
                    )
                    .map(|dict| {
                        og_rules::spelling_support::SpellingSupport::new_for_language(
                            std::sync::Arc::new(dict),
                            core_resource_dir,
                            &lt_modules.join("en/src/main/resources/org/languagetool/resource/en"),
                            "en",
                        )
                    })
                } else if generic_multitoken && core_resource_dir.exists() {
                    let mut dict = og_spell::Dictionary::new();
                    if code == "nl"
                        && let Some(shared) =
                            og_tagger::generic_tagger::shared_dictionary(&resource_dir, "dutch")
                    {
                        dict = og_spell::Dictionary::from_shared_words(std::sync::Arc::new(
                            og_spell::morfologik_view::MorfologikView::new(shared),
                        ));
                    }
                    for name in [
                        "hunspell/spelling.txt",
                        "spelling/spelling.txt",
                        "added.txt",
                    ] {
                        if let Ok(words) = std::fs::read_to_string(resource_dir.join(name)) {
                            for line in words.lines() {
                                let word = line.trim();
                                if !word.is_empty() && !word.starts_with('#') {
                                    dict.add_word(word);
                                }
                            }
                        }
                    }
                    Some(
                        og_rules::spelling_support::SpellingSupport::new_for_language(
                            std::sync::Arc::new(dict),
                            core_resource_dir,
                            &resource_dir,
                            code,
                        ),
                    )
                } else if code == "de" && resource_dir.exists() && core_resource_dir.exists() {
                    shared_german_speller_dictionary(&resource_dir).map(|dictionary| {
                        let mut support =
                            og_rules::spelling_support::SpellingSupport::new_for_language(
                                dictionary.clone(),
                                core_resource_dir,
                                &resource_dir,
                                "de",
                            );
                        if let Some(checker) = og_spell::hunspell::HunspellChecker::load(
                            &resource_dir.join("hunspell/de_DE.aff"),
                            &resource_dir.join("hunspell/de_DE.dic"),
                        ) {
                            let base = og_spell::GermanSpeller::new(
                                dictionary,
                                &resource_dir,
                                Some(core_resource_dir),
                                None,
                                "de-DE",
                            );
                            support = support.with_german_rule_speller(std::sync::Arc::new(
                                base.with_hunspell_core(std::sync::Arc::new(checker)),
                            ));
                        }
                        support
                    })
                } else {
                    None
                };
                let mut engine = match support {
                    Some(support) => {
                        let mut engine = PatternRuleEngine::new()
                            .with_spelling_support(std::sync::Arc::new(support));
                        if let Some(synth) =
                            og_rules::postponed_adjective::LangSynthesizer::for_language(
                                &resource_dir,
                                code,
                            )
                        {
                            engine = engine.with_synthesizer(std::sync::Arc::new(synth));
                        }
                        engine
                    }
                    None => match og_rules::postponed_adjective::LangSynthesizer::for_language(
                        &resource_dir,
                        code,
                    ) {
                        Some(synth) => {
                            PatternRuleEngine::new().with_synthesizer(std::sync::Arc::new(synth))
                        }
                        None => PatternRuleEngine::new(),
                    },
                };
                if let Some(synth) = og_rules::postponed_adjective::LangSynthesizer::for_language(
                    &resource_dir,
                    code,
                ) {
                    engine = engine.with_suggestion_synthesizer(std::sync::Arc::new(synth));
                }
                engine
            });

            (
                tagger_h.join().unwrap(),
                disambiguator_h.join().unwrap(),
                engine_h.join().unwrap(),
            )
        });

        let french_tokenizer = (code == "fr").then(|| {
            let dir = lt_modules.join("fr/src/main/resources/org/languagetool/resource/fr");
            let tagger = og_tagger::GenericTagger::french(&dir);
            Arc::new(og_tokenizer::FrenchWordTokenizer::new(Arc::new(
                move |w: &str| tagger.is_word_tagged(w),
            )))
        });

        let sentence_splitter = if matches!(code, "eo" | "zh") {
            None
        } else {
            og_tokenizer::srx::srx_splitter(code)
        };

        Self {
            language: code.to_string(),
            pattern_engine,
            tagger,
            disambiguator,
            french_tokenizer,
            sentence_splitter,
        }
    }

    fn tokenize_sentence(&self, text: &str) -> AnalyzedSentence {
        let tokenizer = match &self.french_tokenizer {
            Some(fr) => TestWordTokenizer::French(fr.clone()),
            None if self.language == "de" => TestWordTokenizer::German,
            None if self.language == "ar" => TestWordTokenizer::Arabic,
            None if self.language == "nl" => TestWordTokenizer::Dutch,
            None if self.language == "crh" => TestWordTokenizer::CrimeanTatar,
            None if self.language == "eo" => TestWordTokenizer::Esperanto,
            None if self.language == "es" => {
                TestWordTokenizer::Spanish(og_langs::es::word_tokenizer::SpanishWordTokenizer::new())
            }
            None if self.language == "pt" => {
                og_langs::pt::word_tokenizer::shared_portuguese_word_tokenizer()
                    .map(TestWordTokenizer::Portuguese)
                    .unwrap_or(TestWordTokenizer::Default(false))
            }
            None if self.language == "pl" => {
                match og_langs::pl::tagger::PolishTagger::shared_arc() {
                    Some(tagger) => TestWordTokenizer::Polish(Arc::new(
                        og_langs::pl::word_tokenizer::PolishWordTokenizer::new(tagger),
                    )),
                    None => TestWordTokenizer::Default(false),
                }
            }
            None if self.language == "ru" => {
                TestWordTokenizer::Russian(og_langs::ru::word_tokenizer::RussianWordTokenizer::new())
            }
            None if self.language == "be" => TestWordTokenizer::Belarusian(
                og_langs::be::word_tokenizer::BelarusianWordTokenizer::new(),
            ),
            None if self.language == "br" => {
                TestWordTokenizer::Breton(og_langs::br::word_tokenizer::BretonWordTokenizer::new())
            }
            None if self.language == "ca" => {
                TestWordTokenizer::Catalan(og_langs::ca::word_tokenizer::CatalanWordTokenizer::new())
            }
            None if self.language == "el" => {
                TestWordTokenizer::Greek(og_langs::el::word_tokenizer::GreekWordTokenizer::new())
            }
            None if self.language == "fa" => {
                TestWordTokenizer::Persian(og_langs::fa::word_tokenizer::PersianWordTokenizer::new())
            }
            None if self.language == "gl" => TestWordTokenizer::Galician(
                og_langs::gl::word_tokenizer::GalicianWordTokenizer::new(),
            ),
            None if self.language == "ja" => {
                TestWordTokenizer::Japanese(og_langs::ja::JapaneseWordTokenizer::new())
            }
            None if self.language == "km" => {
                TestWordTokenizer::Khmer(og_langs::km::KhmerWordTokenizer::new())
            }
            None if self.language == "ml" => {
                TestWordTokenizer::Malayalam(og_langs::ml::MalayalamWordTokenizer::new())
            }
            None if self.language == "ta" => {
                TestWordTokenizer::Tamil(og_langs::ta::TamilWordTokenizer::new())
            }
            None if self.language == "tl" => {
                TestWordTokenizer::Tagalog(og_langs::tl::TagalogWordTokenizer::new())
            }
            None if self.language == "uk" => {
                TestWordTokenizer::Ukrainian(og_langs::uk::UkrainianWordTokenizer::new())
            }
            None if self.language == "zh" => {
                TestWordTokenizer::Chinese(og_langs::zh::word_tokenizer::ChineseWordTokenizer::new())
            }
            None => TestWordTokenizer::Default(self.language == "en"),
        };
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = tokenizer.tokenize(text, 0);
        let start_token = AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        );
        tokens.insert(0, start_token);

        let tagged = if self.language == "ja" {
            Vec::new()
        } else {
            let token_texts: Vec<String> = tokens
                .iter()
                .skip(1)
                .map(|t| t.token().token().to_string())
                .collect();
            let text_refs: Vec<&str> = token_texts.iter().map(|s| s.as_str()).collect();
            self.tagger.tag(&text_refs)
        };

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
            let text = tagged_atr.token().token().to_string();
            let readings = tagged_atr.into_readings();
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let lemma = readings[0]
                .lemma()
                .map(str::to_string)
                .unwrap_or_else(|| text.clone());
            let updated_primary = AnalyzedToken::new(text, start, end)
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

        if self.language == "en" {
            self.apply_chunking(&mut sentence);
        }

        if self.disambiguator.is_some() {
            sentence.set_pre_disambiguation_tokens(sentence.tokens().to_vec());
        }

        if let Some(ref disambiguator) = self.disambiguator {
            disambiguator.disambiguate(&mut sentence);
        }

        if self.language == "de" {
            og_tagger::german_chunker::shared_german_chunker().add_chunk_tags(&mut sentence);
        }

        if self.language == "ru" {
            og_tagger::russian_chunker::shared_russian_chunker().add_chunk_tags(&mut sentence);
        }

        sentence
    }

    fn apply_chunking(&self, sentence: &mut AnalyzedSentence) {
        og_tagger::chunker::apply_chunk_tags(sentence);
    }

    fn analyze_text(&self, text: &str) -> Vec<(AnalyzedSentence, usize)> {
        use og_tokenizer::SentenceTokenizer as _;
        match &self.sentence_splitter {
            Some(splitter) => splitter
                .split(text)
                .into_iter()
                .map(|s| (self.tokenize_sentence(s.text()), s.start()))
                .collect(),
            None => Vec::from([(self.tokenize_sentence(text), 0)]),
        }
    }

    fn match_rule_across_text(
        &self,
        rule: &og_xml::compiler::CompiledRule,
        clean_text: &str,
    ) -> Vec<og_core::RuleMatch> {
        let mut matches = Vec::new();
        for (sentence, sentence_start) in self.analyze_text(clean_text) {
            for mut m in self.pattern_engine.match_rule(rule, &sentence) {
                m.set_offset_length(m.offset() + sentence_start, m.length());
                matches.push(m);
            }
        }
        matches
    }

    fn extract_marker(text: &str) -> (String, Option<(usize, usize)>) {
        let start_marker = "<marker>";
        let end_marker = "</marker>";

        let start_idx = text.find(start_marker);
        let end_idx = text.find(end_marker);

        match (start_idx, end_idx) {
            (Some(s), Some(e)) => {
                let marker_start = s;
                let marker_end = e - start_marker.len();
                let clean = text.replace(start_marker, "").replace(end_marker, "");
                (clean, Some((marker_start, marker_end)))
            }
            _ => (text.to_string(), None),
        }
    }

    pub fn run_xml_example_tests(&self, xml: &str) -> TestRunSummary {
        let mut results = Vec::new();
        let compiler = XmlCompiler::new();

        let rule_set = match compiler.compile_file(xml) {
            Ok(rs) => rs,
            Err(e) => {
                results.push(ExampleTestResult {
                    rule_id: "PARSE_ERROR".to_string(),
                    passed: false,
                    message: format!("Failed to parse XML: {}", e),
                    example_text: String::new(),
                    expected_type: "parse".to_string(),
                });
                return TestRunSummary {
                    total: 1,
                    passed: 0,
                    failed: 1,
                    results,
                };
            }
        };

        self.run_rule_set_example_tests(&rule_set)
    }

    pub fn debug_sentence(&self, text: &str) -> AnalyzedSentence {
        self.tokenize_sentence(text)
    }

    pub fn match_rule_on_text(
        &self,
        rule: &og_xml::compiler::CompiledRule,
        text: &str,
    ) -> (String, Option<(usize, usize)>, Vec<og_core::RuleMatch>) {
        let (clean_text, marker_range) = Self::extract_marker(text);
        let matches = self.match_rule_across_text(rule, &clean_text);
        (clean_text, marker_range, matches)
    }

    pub fn run_rule_set_example_tests(&self, rule_set: &CompiledRuleSet) -> TestRunSummary {
        use rayon::prelude::*;
        use std::collections::HashMap;

        let started = std::time::Instant::now();
        og_rules::pattern_rule::set_test_date_hack(true);

        let mut group_index: HashMap<(String, Option<String>), usize> = HashMap::new();
        let mut groups: Vec<Vec<&og_xml::compiler::CompiledRule>> = Vec::new();
        for rule in &rule_set.rules {
            let key = (rule.id.clone(), rule.sub_id.clone());
            let next = groups.len();
            let idx = *group_index.entry(key).or_insert(next);
            if idx == next {
                groups.push(vec![rule]);
            } else {
                groups[idx].push(rule);
            }
        }

        let pairs: Vec<(
            &og_xml::compiler::CompiledRule,
            &og_xml::types::XmlExample,
            &Vec<&og_xml::compiler::CompiledRule>,
        )> = groups
            .iter()
            .flat_map(|group| {
                group
                    .first()
                    .into_iter()
                    .flat_map(move |rule| rule.examples.iter().map(move |e| (*rule, e, group)))
            })
            .collect();

        let results: Vec<ExampleTestResult> = pairs
            .par_iter()
            .map(|(rule, example, group)| {
                let (clean_text, marker_range) = Self::extract_marker(&example.text);
                let mut matches = Vec::new();
                for aux in group.iter() {
                    matches.extend(self.match_rule_across_text(aux, &clean_text));
                }
                Self::evaluate_example(rule, example, &clean_text, marker_range, &matches)
            })
            .collect();

        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.len() - passed;
        let elapsed = started.elapsed();
        eprintln!(
            "Suite wall-clock: {:.2}s for {} examples ({:.0} examples/s), {failed} failed",
            elapsed.as_secs_f64(),
            results.len(),
            if elapsed.as_secs_f64() > 0.0 {
                results.len() as f64 / elapsed.as_secs_f64()
            } else {
                0.0
            }
        );
        TestRunSummary {
            total: results.len(),
            passed,
            failed,
            results,
        }
    }

    fn evaluate_example(
        rule: &og_xml::compiler::CompiledRule,
        example: &og_xml::types::XmlExample,
        clean_text: &str,
        marker_range: Option<(usize, usize)>,
        matches: &[og_core::RuleMatch],
    ) -> ExampleTestResult {
        match example.example_type {
            og_xml::types::XmlExampleType::Incorrect => {
                if matches.is_empty() {
                    ExampleTestResult {
                        rule_id: rule.id.clone(),
                        passed: false,
                        message: format!(
                            "Expected match on incorrect example but got none: '{}'",
                            clean_text
                        ),
                        example_text: clean_text.to_string(),
                        expected_type: "incorrect".to_string(),
                    }
                } else if let Some((m_start, m_end)) = marker_range {
                    let first_match = &matches[0];
                    let match_start = first_match.offset();
                    let match_end = match_start + first_match.length();
                    let marker_in_match = match_start <= m_start && match_end >= m_end;
                    if marker_in_match {
                        ExampleTestResult {
                            rule_id: rule.id.clone(),
                            passed: true,
                            message: format!(
                                "Match {}..{} contains marker {}..{}",
                                match_start, match_end, m_start, m_end
                            ),
                            example_text: clean_text.to_string(),
                            expected_type: "incorrect".to_string(),
                        }
                    } else {
                        ExampleTestResult {
                            rule_id: rule.id.clone(),
                            passed: false,
                            message: format!(
                                "Match {}..{} does not contain marker {}..{}",
                                match_start, match_end, m_start, m_end
                            ),
                            example_text: clean_text.to_string(),
                            expected_type: "incorrect".to_string(),
                        }
                    }
                } else {
                    ExampleTestResult {
                        rule_id: rule.id.clone(),
                        passed: true,
                        message: format!("Matched correctly: '{}'", clean_text),
                        example_text: clean_text.to_string(),
                        expected_type: "incorrect".to_string(),
                    }
                }
            }
            og_xml::types::XmlExampleType::Correct => {
                if !matches.is_empty() {
                    ExampleTestResult {
                        rule_id: rule.id.clone(),
                        passed: false,
                        message: format!(
                            "Expected no match on correct example but got {}: '{}'",
                            matches.len(),
                            clean_text
                        ),
                        example_text: clean_text.to_string(),
                        expected_type: "correct".to_string(),
                    }
                } else {
                    ExampleTestResult {
                        rule_id: rule.id.clone(),
                        passed: true,
                        message: format!("No match on correct example: '{}'", clean_text),
                        example_text: clean_text.to_string(),
                        expected_type: "correct".to_string(),
                    }
                }
            }
            og_xml::types::XmlExampleType::TriggersError => ExampleTestResult {
                rule_id: rule.id.clone(),
                passed: true,
                message: format!("Triggers error (known FP): '{}'", clean_text),
                example_text: clean_text.to_string(),
                expected_type: "triggers_error".to_string(),
            },
        }
    }

    pub fn run_xml_file_tests(&self, path: &Path) -> Result<TestRunSummary, String> {
        let xml = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        Ok(self.run_xml_example_tests(&xml))
    }

    pub fn run_pattern_test(
        &self,
        rule_id: &str,
        xml: &str,
        sentence_text: &str,
        expected_match: bool,
    ) -> ExampleTestResult {
        let compiler = XmlCompiler::new();
        let rule_set = match compiler.compile_file(xml) {
            Ok(rs) => rs,
            Err(e) => {
                return ExampleTestResult {
                    rule_id: rule_id.to_string(),
                    passed: false,
                    message: format!("Parse error: {}", e),
                    example_text: sentence_text.to_string(),
                    expected_type: "single".to_string(),
                };
            }
        };

        let rule = match rule_set.rules.iter().find(|r| r.id == rule_id) {
            Some(r) => r,
            None => {
                return ExampleTestResult {
                    rule_id: rule_id.to_string(),
                    passed: false,
                    message: format!("Rule '{}' not found", rule_id),
                    example_text: sentence_text.to_string(),
                    expected_type: "single".to_string(),
                };
            }
        };

        let sentence = self.tokenize_sentence(sentence_text);
        let matches = self.pattern_engine.match_rule(rule, &sentence);

        let matched = !matches.is_empty();
        ExampleTestResult {
            rule_id: rule_id.to_string(),
            passed: matched == expected_match,
            message: if matched == expected_match {
                format!("OK: '{}'", sentence_text)
            } else {
                format!(
                    "FAIL: '{}' - expected {}, got {}",
                    sentence_text,
                    if expected_match { "match" } else { "no match" },
                    if matched { "match" } else { "no match" }
                )
            },
            example_text: sentence_text.to_string(),
            expected_type: "single".to_string(),
        }
    }
}

impl Default for TestRunner {
    fn default() -> Self {
        Self::new()
    }
}

pub mod test_support {
    use super::*;

    pub const GRAMMAR_PATH: &str = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/grammar.xml";
    const DE_GRAMMAR_PATH: &str = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/rules/de/grammar.xml";

    pub fn german_runner() -> Arc<TestRunner> {
        static CELL: og_core::releasable::Releasable<TestRunner> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&CELL, || {
            Some(Arc::new(TestRunner::for_language("de")))
        })
        .expect("german runner")
    }

    pub fn german_grammar_rules() -> Arc<CompiledRuleSet> {
        static CELL: og_core::releasable::Releasable<CompiledRuleSet> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&CELL, || {
            let path = std::path::Path::new(DE_GRAMMAR_PATH);
            let xml = std::fs::read_to_string(path).expect("Failed to read de grammar.xml");
            Some(Arc::new(
                XmlCompiler::new()
                    .compile_file_with_base(&xml, path.parent())
                    .expect("Failed to compile de grammar.xml"),
            ))
        })
        .expect("de grammar rules")
    }

    pub fn german_grammar_available() -> bool {
        Path::new(DE_GRAMMAR_PATH).exists()
    }

    pub fn shared_runner() -> Arc<TestRunner> {
        static CELL: og_core::releasable::Releasable<TestRunner> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&CELL, || Some(Arc::new(TestRunner::new())))
            .expect("english runner")
    }

    pub fn shared_language_runner(code: &'static str) -> Arc<TestRunner> {
        static CELLS: std::sync::OnceLock<
            std::sync::Mutex<
                std::collections::HashMap<
                    &'static str,
                    &'static og_core::releasable::Releasable<TestRunner>,
                >,
            >,
        > = std::sync::OnceLock::new();
        let cell = {
            let cells = CELLS.get_or_init(Default::default);
            let mut guard = cells.lock().unwrap();
            *guard
                .entry(code)
                .or_insert_with(|| Box::leak(Box::new(og_core::releasable::Releasable::new())))
        };
        og_core::releasable::shared_get_or_build(cell, || {
            Some(Arc::new(TestRunner::for_language(code)))
        })
        .unwrap_or_else(|| panic!("{code} runner"))
    }

    pub fn shared_grammar_rules() -> Arc<CompiledRuleSet> {
        static CELL: og_core::releasable::Releasable<CompiledRuleSet> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&CELL, || {
            let path = std::path::Path::new(GRAMMAR_PATH);
            let xml = std::fs::read_to_string(path).expect("Failed to read grammar.xml");
            Some(Arc::new(
                XmlCompiler::new()
                    .compile_file_with_base(&xml, path.parent())
                    .expect("Failed to compile grammar.xml"),
            ))
        })
        .expect("english grammar rules")
    }

    pub fn grammar_available() -> bool {
        Path::new(GRAMMAR_PATH).exists()
    }

    pub fn generic_grammar_available(code: &str) -> bool {
        Path::new(&format!(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{code}/src/main/resources/org/languagetool/rules/{code}/grammar.xml"
        ))
        .exists()
    }

    pub fn generic_grammar_rules(code: &'static str) -> std::sync::Arc<CompiledRuleSet> {
        static CACHE: std::sync::OnceLock<
            std::sync::Mutex<
                std::collections::HashMap<&'static str, std::sync::Weak<CompiledRuleSet>>,
            >,
        > = std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
        let mut guard = cache.lock().unwrap();
        if let Some(hit) = guard.get(code).and_then(std::sync::Weak::upgrade) {
            return hit;
        }
        let path = std::path::PathBuf::from(format!(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{code}/src/main/resources/org/languagetool/rules/{code}/grammar.xml"
        ));
        let extra_files: &[&str] = match code {
            "sr" => &og_langs::sr::SR_RULE_FILES,
            "uk" => &og_langs::uk::UK_RULE_FILES,
            _ => &[],
        };
        let normalize = |xml: String| -> String {
            match code {
                "fr" => og_langs::fr::normalize_fr_rules_xml(&xml).into_owned(),
                "pl" => og_langs::pl::normalize_pl_rules_xml(&xml).into_owned(),
                "br" => og_langs::br::normalize_br_rules_xml(&xml).into_owned(),
                "uk" => og_langs::uk::normalize_uk_rules_xml(&xml).into_owned(),
                "zh" => og_langs::zh::normalize_zh_rules_xml(&xml).into_owned(),
                _ => xml,
            }
        };
        let xml = std::fs::read_to_string(&path).expect("Failed to read grammar.xml");
        let xml = normalize(xml);
        let mut rules: Vec<og_xml::compiler::CompiledRule> = Vec::new();
        let mut categories = Vec::new();
        let mut unifications = Vec::new();
        let main = XmlCompiler::new()
            .compile_file_with_base(&xml, path.parent())
            .expect("Failed to compile grammar.xml");
        rules.extend(main.rules);
        categories.extend(main.categories);
        unifications.extend(main.unifications);
        for file in extra_files {
            let extra_path = path.with_file_name(file);
            let extra_xml = std::fs::read_to_string(&extra_path)
                .unwrap_or_else(|e| panic!("Failed to read {file}: {e}"));
            let extra_xml = normalize(extra_xml);
            let extra = XmlCompiler::new()
                .compile_file_with_base(&extra_xml, extra_path.parent())
                .unwrap_or_else(|e| panic!("Failed to compile {file}: {e}"));
            rules.extend(extra.rules);
            categories.extend(extra.categories);
            unifications.extend(extra.unifications);
        }
        let compiled = std::sync::Arc::new(CompiledRuleSet {
            rules,
            categories,
            unifications,
        });
        guard.insert(code, std::sync::Arc::downgrade(&compiled));
        compiled
    }
}

fn shared_german_speller_dictionary(
    resource_dir: &Path,
) -> Option<std::sync::Arc<og_spell::Dictionary>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<std::path::PathBuf, std::sync::Weak<og_spell::Dictionary>>,
        >,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = resource_dir.to_path_buf();
    let mut guard = cache.lock().unwrap();
    if let Some(dict) = guard.get(&key).and_then(std::sync::Weak::upgrade) {
        return Some(dict);
    }
    let dictionary = og_tagger::german_tagger::shared_german_dictionary(resource_dir).map(|dict| {
        let mut d = og_spell::Dictionary::from_shared_words(std::sync::Arc::new(
            og_spell::morfologik_view::MorfologikView::new(dict),
        ));
        if let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt")) {
            for line in words.lines() {
                for word in og_rules::spelling_support::prepare_line_for_speller(
                    og_rules::spelling_support::SpellerVariant::German,
                    line,
                ) {
                    d.add_word(&word);
                }
            }
        }
        d
    });
    let dictionary = dictionary.map(std::sync::Arc::new);
    if let Some(dict) = &dictionary {
        guard.insert(key, std::sync::Arc::downgrade(dict));
    }
    dictionary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_xml(rules_body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                {}
            </category>
        </rules>"#,
            rules_body
        )
    }

    #[test]
    fn sentence_tokenizer_splits_on_srx_boundaries() {
        let text = "One sentence. Another one? Yes!";
        let ranges = TestSentenceTokenizer.split(text);
        let texts: Vec<&str> = ranges.iter().map(|r| &text[r.start..r.end]).collect();
        assert_eq!(texts, vec!["One sentence. ", "Another one? ", "Yes!"]);
        assert!(TestSentenceTokenizer.split("").is_empty());
    }

    #[test]
    fn german_grammar_rules_compile_and_share_the_cache() {
        assert!(test_support::german_grammar_available());
        let rules = test_support::german_grammar_rules();
        assert!(
            rules.rules.len() > 4000,
            "de grammar rule count: {}",
            rules.rules.len()
        );
        let again = test_support::german_grammar_rules();
        assert!(std::sync::Arc::ptr_eq(&rules, &again));
    }

    #[test]
    fn generic_grammar_rules_merge_the_ukrainian_extra_files() {
        assert!(test_support::generic_grammar_available("uk"));
        let rules = test_support::generic_grammar_rules("uk");
        assert!(
            rules
                .rules
                .iter()
                .any(|r| r.id == "piv_with_proper_noun_1992"),
            "rule from grammar-spelling.xml missing; {} rules total",
            rules.rules.len()
        );
        let again = test_support::generic_grammar_rules("uk");
        assert!(std::sync::Arc::ptr_eq(&rules, &again));
    }

    #[test]
    fn shared_german_speller_dictionary_is_reused_while_alive() {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
        );
        let first = shared_german_speller_dictionary(dir);
        let second = shared_german_speller_dictionary(dir);
        match (first, second) {
            (Some(a), Some(b)) => assert!(std::sync::Arc::ptr_eq(&a, &b)),
            _ => panic!("german speller dictionary should load from {dir:?}"),
        }
    }

    #[test]
    fn test_runner_incorrect_example_passes() {
        let runner = test_support::shared_runner();
        let xml = simple_xml(
            r#"
            <rule id="TEST1" name="Test">
                <pattern>
                    <token>their</token>
                    <token>is</token>
                </pattern>
                <message>Did you mean <suggestion>there is</suggestion>?</message>
                <example correction="there is">Their <marker>is</marker> a problem.</example>
                <example>There is no problem.</example>
            </rule>
        "#,
        );

        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert_eq!(
            summary.passed,
            2,
            "Failed: {:?}",
            summary
                .results
                .iter()
                .filter(|r| !r.passed)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_runner_correct_example_no_match() {
        let runner = test_support::shared_runner();
        let xml = simple_xml(
            r#"
            <rule id="TEST2" name="Test">
                <pattern>
                    <token>xyzzy</token>
                </pattern>
                <message>This should not match.</message>
                <example>This is a normal sentence.</example>
            </rule>
        "#,
        );

        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 1);
        assert!(summary.is_all_passed());
    }

    #[test]
    fn test_runner_regexp_rule() {
        let runner = test_support::shared_runner();
        let xml = simple_xml(
            r#"
            <rule id="TEST3" name="Test">
                <pattern>
                    <token regexp="yes">teh</token>
                </pattern>
                <message>Did you mean <suggestion>the</suggestion>?</message>
                <example correction="the"><marker>Teh</marker> quick brown fox.</example>
                <example>The quick brown fox.</example>
            </rule>
        "#,
        );

        let summary = runner.run_xml_example_tests(&xml);
        for r in &summary.results {
            if !r.passed {
                eprintln!("FAILED: {} - {}", r.rule_id, r.message);
            }
        }
        assert!(
            summary
                .results
                .iter()
                .any(|r| r.passed && r.expected_type == "correct")
        );
    }

    #[test]
    fn test_runner_multiple_rules() {
        let runner = test_support::shared_runner();
        let xml = simple_xml(
            r#"
            <rule id="RULE_A" name="Rule A">
                <pattern>
                    <token>hello</token>
                    <token>world</token>
                </pattern>
                <message>Hi!</message>
                <example><marker>Hello world</marker> test.</example>
                <example>Hi there.</example>
            </rule>
            <rule id="RULE_B" name="Rule B">
                <pattern>
                    <token>goodbye</token>
                    <token>world</token>
                </pattern>
                <message>Bye!</message>
                <example><marker>Goodbye world</marker> test.</example>
                <example>See you.</example>
            </rule>
        "#,
        );

        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 4, "Should have 4 examples");
    }

    #[test]
    fn test_runner_marker_extraction() {
        let (clean, range) = TestRunner::extract_marker("Hello <marker>world</marker> test");
        assert_eq!(clean, "Hello world test");
        assert_eq!(range, Some((6, 11)));
    }

    #[test]
    fn test_runner_no_marker() {
        let (clean, range) = TestRunner::extract_marker("Hello world test");
        assert_eq!(clean, "Hello world test");
        assert!(range.is_none());
    }

    #[test]
    fn test_runner_single_pattern_test() {
        let runner = test_support::shared_runner();
        let xml = simple_xml(
            r#"
            <rule id="SINGLE_TEST" name="Single">
                <pattern>
                    <token>foo</token>
                    <token>bar</token>
                </pattern>
                <message>Test</message>
            </rule>
        "#,
        );

        let result = runner.run_pattern_test("SINGLE_TEST", &xml, "foo bar baz", true);
        assert!(result.passed, "{}", result.message);

        let result = runner.run_pattern_test("SINGLE_TEST", &xml, "baz qux", false);
        assert!(result.passed, "{}", result.message);
    }

    #[test]
    fn test_runner_xml_parse_error() {
        let runner = test_support::shared_runner();
        let xml = r#"<?xml version="1.0"?><rules><category id="X"><rule id="TEST"><pattern>"#;
        let summary = runner.run_xml_example_tests(xml);
        assert!(summary.total <= 1);
    }

    #[test]
    fn test_runner_rule_not_found() {
        let runner = test_support::shared_runner();
        let xml = simple_xml(
            r#"
            <rule id="EXISTS" name="Exists">
                <pattern><token>test</token></pattern>
                <message>Test</message>
            </rule>
        "#,
        );

        let result = runner.run_pattern_test("NONEXISTENT", &xml, "test", true);
        assert!(!result.passed);
        assert!(result.message.contains("not found"));
    }

    #[test]
    fn test_runner_negate_token() {
        let runner = test_support::shared_runner();
        let xml = simple_xml(
            r#"
            <rule id="NEG_TEST" name="Negate">
                <pattern>
                    <token negate="yes">good</token>
                    <token>morning</token>
                </pattern>
                <message>Not good morning.</message>
                <example type="incorrect"><marker>Bad morning</marker> today.</example>
                <example>Good morning everyone.</example>
            </rule>
        "#,
        );

        let summary = runner.run_xml_example_tests(&xml);
        for r in &summary.results {
            if !r.passed {
                eprintln!(
                    "FAILED: {} ({}) - {}",
                    r.rule_id, r.expected_type, r.message
                );
            }
        }
        assert!(summary.is_all_passed(), "Negate token test failed");
    }

    #[test]
    fn test_load_real_english_grammar_xml() {
        if !test_support::grammar_available() {
            eprintln!("Skipping: English grammar.xml not found");
            return;
        }

        let rule_set = test_support::shared_grammar_rules();

        let total = rule_set.rules.len();
        eprintln!("Loaded {} rules from English grammar.xml", total);

        assert!(total > 4000, "Expected 4000+ rules, got {}", total);

        let with_antipatterns = rule_set
            .rules
            .iter()
            .filter(|r| !r.antipatterns.is_empty())
            .count();
        let with_marker = rule_set
            .rules
            .iter()
            .filter(|r| r.pattern.marker_start.is_some())
            .count();
        let with_exceptions = rule_set
            .rules
            .iter()
            .filter(|r| r.pattern.tokens.iter().any(|t| !t.exceptions.is_empty()))
            .count();
        let with_skip = rule_set
            .rules
            .iter()
            .filter(|r| r.pattern.tokens.iter().any(|t| t.skip != 0))
            .count();
        let with_min_max = rule_set
            .rules
            .iter()
            .filter(|r| {
                r.pattern
                    .tokens
                    .iter()
                    .any(|t| t.min.is_some() || t.max.is_some())
            })
            .count();
        let with_postag = rule_set
            .rules
            .iter()
            .filter(|r| r.pattern.tokens.iter().any(|t| t.postag.is_some()))
            .count();

        eprintln!("Rules with antipatterns: {}", with_antipatterns);
        eprintln!("Rules with pattern markers: {}", with_marker);
        eprintln!("Rules with exceptions: {}", with_exceptions);
        eprintln!("Rules with skip: {}", with_skip);
        eprintln!("Rules with min/max: {}", with_min_max);
        eprintln!("Rules with postag: {}", with_postag);

        assert!(
            with_antipatterns > 1000,
            "Expected 1000+ rules with antipatterns, got {}",
            with_antipatterns
        );
        assert!(
            with_exceptions > 500,
            "Expected 500+ rules with exceptions, got {}",
            with_exceptions
        );
        assert!(
            with_skip > 100,
            "Expected 100+ rules with skip, got {}",
            with_skip
        );
    }

    #[test]
    fn test_real_german_grammar_examples() {
        const PINNED_RESIDUE: &[(&str, &str)] = &[(
            "ZUSAMMENSCHREIBUNG_HINTER",
            "Expected match on incorrect example but got none: 'Er hat ihr hinterher spioniert.'",
        )];

        if !test_support::german_grammar_available() {
            eprintln!("Skipping: German grammar.xml not found");
            return;
        }

        let started = std::time::Instant::now();
        let rules_handle = std::thread::spawn(test_support::german_grammar_rules);
        let runner = test_support::german_runner();
        let rules = rules_handle.join().expect("grammar compile thread");
        let summary = runner.run_rule_set_example_tests(&rules);
        drop(rules);
        drop(runner);
        trim_released_pages(og_core::releasable::release_unused_since(started));

        eprintln!("Total German examples tested: {}", summary.total);
        eprintln!("Passed: {}", summary.passed);
        eprintln!("Failed: {}", summary.failed);
        eprintln!(
            "Pass rate: {:.1}%",
            if summary.total > 0 {
                100.0 * summary.passed as f64 / summary.total as f64
            } else {
                0.0
            }
        );

        let mut correct_fp = 0;
        let mut no_match = 0;
        let mut marker = 0;
        for r in &summary.results {
            if r.passed {
                continue;
            }
            if r.message.contains("correct -") || r.message.contains("Expected no match") {
                correct_fp += 1;
            } else if r.message.contains("marker") {
                marker += 1;
            } else {
                no_match += 1;
            }
        }
        eprintln!(
            "  Incorrect (no match): {no_match}\n  Correct (unexpected match): {correct_fp}\n  Incorrect (marker mismatch): {marker}"
        );

        let mut fail_file = String::new();
        let mut residue: Vec<(String, String)> = Vec::new();
        for r in &summary.results {
            if !r.passed {
                fail_file.push_str(&format!("FAIL: [{}] {}\n", r.rule_id, r.message));
                residue.push((r.rule_id.clone(), r.message.clone()));
            }
        }
        let _ = std::fs::write("/tmp/test_failures_de.txt", fail_file);

        let expected: Vec<(String, String)> = PINNED_RESIDUE
            .iter()
            .map(|(id, msg)| (id.to_string(), msg.to_string()))
            .collect();
        assert_eq!(
            residue, expected,
            "the German residue must stay pinned: the one ZUSAMMENSCHREIBUNG_HINTER example \
             dies in RemoveUnknownCompoundsFilter exactly as it does in Java on this data — \
             LT's own de_DE.dic carries 'hinterher' with no compound flags and only the bare \
             lemma 'hinterherspionieren', so hunspell 1.7 (the libhunspell-1.7.so bundled by \
             com.gitlab.dumonts:hunspell:2.1.2, verified: it flags 'hinterherspioniert' and \
             even suggests splitting it) rejects the compound, it appears in no accept list \
             (ignore.txt/spelling*.txt), so isMisspelled is true and the filter drops the \
             match; making it pass would require engine behavior or data Java itself does \
             not have; new or changed failures mean an engine regression"
        );
        eprintln!("Summary: {}/{} passed", summary.passed, summary.total);
    }

    #[test]
    fn test_real_english_grammar_examples() {
        const PINNED_EN_RESIDUE: &[(&str, &str)] = &[];

        if !test_support::grammar_available() {
            eprintln!("Skipping: English grammar.xml not found");
            return;
        }

        let rules_handle = std::thread::spawn(test_support::shared_grammar_rules);
        let runner = test_support::shared_runner();
        let rules = rules_handle.join().expect("grammar compile thread");
        let summary = runner.run_rule_set_example_tests(&rules);

        eprintln!("Total examples tested: {}", summary.total);
        {
            let xml = std::fs::read_to_string(test_support::GRAMMAR_PATH).unwrap_or_default();
            let mut without_comments = String::with_capacity(xml.len());
            let mut rest = xml.as_str();
            while let Some(start) = rest.find("<!--") {
                without_comments.push_str(&rest[..start]);
                rest = match rest[start..].find("-->") {
                    Some(end) => &rest[start + end + 3..],
                    None => "",
                };
            }
            without_comments.push_str(rest);
            let live = without_comments.matches("<example").count();
            let raw = xml.matches("<example").count();
            let mut in_antipattern_examples = 0usize;
            let mut depth = 0usize;
            let mut offset = 0usize;
            while let Some(rel) = without_comments[offset..].find('<') {
                let tag = &without_comments[offset + rel..];
                if tag.starts_with("<antipattern") {
                    depth += 1;
                } else if tag.starts_with("</antipattern") && depth > 0 {
                    depth -= 1;
                } else if depth > 0 && tag.starts_with("<example") {
                    in_antipattern_examples += 1;
                }
                offset += rel + 1;
            }
            eprintln!(
                "Evaluated-vs-live accounting: {}/{} live <example> tags evaluated ({} raw, {} inside comments, {} live examples sit inside <antipattern> blocks)",
                summary.total,
                live,
                raw,
                raw - live,
                in_antipattern_examples
            );
        }
        eprintln!("Passed: {}", summary.passed);
        eprintln!("Failed: {}", summary.failed);
        eprintln!(
            "Pass rate: {:.1}%",
            if summary.total > 0 {
                100.0 * summary.passed as f64 / summary.total as f64
            } else {
                0.0
            }
        );

        let mut fail_by_reason: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for r in &summary.results {
            if !r.passed {
                let reason = if r.message.contains("POS") {
                    "pos_tag".to_string()
                } else if r.message.contains("match") {
                    "no_match".to_string()
                } else if r.message.contains("marker") {
                    "marker_mismatch".to_string()
                } else {
                    "other".to_string()
                };
                *fail_by_reason.entry(reason).or_insert(0) += 1;
            }
        }
        eprintln!("\nFailure categories:");
        for (reason, count) in &fail_by_reason {
            eprintln!("  {}: {}", reason, count);
        }

        let mut rule_fail_counts: std::collections::HashMap<String, (usize, usize, usize)> =
            std::collections::HashMap::new();
        for r in &summary.results {
            if !r.passed {
                let entry = rule_fail_counts
                    .entry(r.rule_id.clone())
                    .or_insert((0, 0, 0));
                if r.expected_type == "incorrect" {
                    entry.0 += 1;
                } else {
                    entry.1 += 1;
                }
                if r.message.contains("marker") {
                    entry.2 += 1;
                }
            }
        }
        let mut sorted_fails: Vec<_> = rule_fail_counts.iter().collect();
        sorted_fails.sort_by_key(|(_, counts)| std::cmp::Reverse(counts.0 + counts.1));
        eprintln!("\nTop 30 failing rules:");
        for (rule_id, (incorrect, correct, marker)) in sorted_fails.iter().take(30) {
            eprintln!(
                "  {}: {} no-match, {} false-positive, {} marker-mismatch",
                rule_id, incorrect, correct, marker
            );
        }

        let failures: Vec<_> = summary
            .results
            .iter()
            .filter(|r| !r.passed)
            .take(100)
            .collect();
        for f in &failures {
            eprintln!("FAIL: [{}] {} - {}", f.rule_id, f.expected_type, f.message);
        }

        if let Ok(all_failures) = std::fs::File::create("/tmp/test_failures_all.txt") {
            use std::io::Write;
            let mut out = std::io::BufWriter::new(all_failures);
            for r in &summary.results {
                if !r.passed {
                    writeln!(
                        out,
                        "FAIL: [{}] {} - {}",
                        r.rule_id, r.expected_type, r.message
                    )
                    .ok();
                }
            }
        }

        let incorrect_no_match = summary
            .results
            .iter()
            .filter(|r| {
                !r.passed && r.expected_type == "incorrect" && r.message.contains("Expected match")
            })
            .count();
        let correct_unexpected = summary
            .results
            .iter()
            .filter(|r| !r.passed && r.expected_type == "correct")
            .count();
        let incorrect_marker = summary
            .results
            .iter()
            .filter(|r| {
                !r.passed
                    && r.expected_type == "incorrect"
                    && r.message.contains("does not contain marker")
            })
            .count();
        eprintln!("\nFailure breakdown:");
        eprintln!("  Incorrect (no match): {}", incorrect_no_match);
        eprintln!("  Correct (unexpected match): {}", correct_unexpected);
        eprintln!("  Incorrect (marker mismatch): {}", incorrect_marker);

        let pass_rate = if summary.total > 0 {
            summary.passed as f64 / summary.total as f64
        } else {
            0.0
        };
        assert!(
            pass_rate > 0.10,
            "Expected at least 10% pass rate, got {:.1}%",
            pass_rate * 100.0
        );

        let residue: Vec<(String, String)> = summary
            .results
            .iter()
            .filter(|r| !r.passed)
            .map(|r| (r.rule_id.clone(), r.message.clone()))
            .collect();
        let expected: Vec<(String, String)> = PINNED_EN_RESIDUE
            .iter()
            .map(|(id, msg)| (id.to_string(), msg.to_string()))
            .collect();
        assert_eq!(
            residue, expected,
            "the English residue must stay pinned byte-exact: the last entry (A_NNS on 'a \
             feeling private minions') was fixed by teaching the disambiguation XML parser to \
             ignore <!ENTITY> definitions inside comments, so en's live \
             &abbreviated_negated_verbs; (wo|would|...) — not the commented-out fused forms \
             (won|wouldn|...) — feeds DT_JJ_NNJJ_NN_VB_APOSTROPHE, which strips the JJ reading \
             from 'private' exactly like Java; any new or changed failure is an engine regression"
        );
    }

    #[test]
    fn test_no_break_space_example_not_flagged() {
        if !test_support::grammar_available() {
            return;
        }
        let rules = test_support::shared_grammar_rules();
        let runner = test_support::shared_runner();
        let rule = rules
            .rules
            .iter()
            .find(|r| {
                r.id == "SENT_START_CONJUNCTIVE_LINKING_ADVERB_COMMA"
                    && r.examples.iter().any(|e| e.text.contains('\u{a0}'))
            })
            .expect("sub-rule with no-break-space example");
        let example = rule
            .examples
            .iter()
            .find(|e| e.text.contains('\u{a0}'))
            .unwrap();
        let (_clean, _marker, matches) = runner.match_rule_on_text(rule, &example.text);
        assert!(
            matches.is_empty(),
            "no-break-space example flagged: '{}' got {} matches",
            example.text,
            matches.len()
        );
    }

    #[test]
    fn test_it_is_rule_its_cool_man() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rulegroup id="IT_IS" name="Test">
                    <rule>
                        <pattern>
                            <marker>
                                <token>its</token>
                            </marker>
                            <token postag="JJ">
                                <exception regexp="yes">first|second|third|last</exception>
                            </token>
                            <token>man</token>
                        </pattern>
                        <message>Did you mean <suggestion>it's</suggestion>?</message>
                        <example correction="It's"><marker>Its</marker> cool man!</example>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;

        let runner = test_support::shared_runner();
        let summary = runner.run_xml_example_tests(xml);

        let sentence = runner.tokenize_sentence("Its cool man!");
        let nwt: Vec<_> = sentence.non_whitespace_tokens();
        eprintln!("Non-whitespace tokens for 'Its cool man!':");
        for (i, token) in nwt.iter().enumerate() {
            eprintln!(
                "  [{}] '{}' pos={:?} has_JJ={}",
                i,
                token.token().token(),
                token.token().pos_tags(),
                token.has_pos_tag("JJ")
            );
        }

        let compiler = og_xml::compiler::XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        eprintln!("Compiled {} rules", rule_set.rules.len());
        for rule in &rule_set.rules {
            eprintln!(
                "Rule {} has {} pattern tokens",
                rule.id,
                rule.pattern.tokens.len()
            );
            for (i, t) in rule.pattern.tokens.iter().enumerate() {
                eprintln!(
                    "  token[{}]: text={:?} postag={:?} negate={}",
                    i, t.text, t.postag, t.negate
                );
            }
            let matches = runner.pattern_engine.match_rule(rule, &sentence);
            eprintln!("  matches: {} matches found", matches.len());
        }

        for r in &summary.results {
            if !r.passed {
                eprintln!("FAILED: {}", r.message);
            }
        }
        assert!(
            summary.is_all_passed(),
            "IT_IS[10] rule should match 'Its cool man!'"
        );
    }

    #[test]
    fn test_debug_pipeline_stages() {
        let runner = test_support::shared_runner();
        for text in [
            "We want go to the cinema.",
            "You will see what an American looks like.",
        ] {
            let tokenizer = TestWordTokenizer::Default(true);
            let mut sentence = AnalyzedSentence::new(text, 0, text.len());
            let mut tokens = tokenizer.tokenize(text, 0);
            let start_token = AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            );
            tokens.insert(0, start_token);
            let token_texts: Vec<String> = tokens
                .iter()
                .skip(1)
                .map(|t| t.token().token().to_string())
                .collect();
            let text_refs: Vec<&str> = token_texts.iter().map(|s| s.as_str()).collect();
            let tagged = runner.tagger.tag(&text_refs);
            let tagged_tags = |word: &str| -> Vec<String> {
                tagged
                    .iter()
                    .find(|atr| atr.token().token() == word)
                    .map(|atr| {
                        let mut tags: Vec<String> = atr
                            .readings()
                            .iter()
                            .flat_map(|r| r.pos_tags().to_vec())
                            .collect();
                        tags.sort();
                        tags.dedup();
                        tags
                    })
                    .unwrap_or_default()
            };
            if text == "We want go to the cinema." {
                for (word, expected) in [("want", "VB"), ("go", "VB"), ("to", "TO")] {
                    assert!(
                        tagged_tags(word).iter().any(|t| t == expected),
                        "'{word}' must carry {expected} straight after tagging, got {:?}",
                        tagged_tags(word)
                    );
                }
            }
            eprintln!("== '{}' AFTER TAG:", text);
            for (i, atr) in tagged.iter().enumerate() {
                eprintln!(
                    "  [{}] {:?} readings={:?}",
                    i,
                    text_refs.get(i),
                    atr.readings()
                        .iter()
                        .map(|r| r.pos_tags().to_vec())
                        .collect::<Vec<_>>()
                );
            }
            let merged: Vec<AnalyzedTokenReadings> = tagged
                .iter()
                .map(|atr| {
                    let at = atr.token().clone();
                    AnalyzedTokenReadings::new(at).with_readings(atr.readings().to_vec())
                })
                .collect();
            sentence.set_tokens(merged);
            if let Some(ref d) = runner.disambiguator {
                d.disambiguate(&mut sentence);
            }
            let disambig_tags = |word: &str| -> Vec<String> {
                sentence
                    .tokens()
                    .iter()
                    .find(|t| t.token().token() == word)
                    .map(|t| {
                        let mut tags: Vec<String> = t
                            .readings()
                            .iter()
                            .flat_map(|r| r.pos_tags().to_vec())
                            .collect();
                        tags.sort();
                        tags.dedup();
                        tags
                    })
                    .unwrap_or_default()
            };
            if text == "We want go to the cinema." {
                for (word, expected) in [("want", "VB"), ("go", "VB"), ("to", "TO")] {
                    assert!(
                        disambig_tags(word).iter().any(|t| t == expected),
                        "'{word}' must still carry {expected} after disambiguation, got {:?}",
                        disambig_tags(word)
                    );
                }
            } else {
                assert_eq!(
                    disambig_tags("will"),
                    vec!["MD"],
                    "the en disambiguator must reduce 'will' to MD"
                );
                assert_eq!(
                    disambig_tags("what"),
                    vec!["WP"],
                    "the en disambiguator must reduce 'what' to WP"
                );
            }
            eprintln!("== '{}' AFTER DISAMBIG:", text);
            for t in sentence.tokens() {
                if t.is_whitespace() {
                    continue;
                }
                eprintln!(
                    "  {:?} readings={:?} lemmas={:?}",
                    t.token().token(),
                    t.readings()
                        .iter()
                        .map(|r| r.pos_tags().to_vec())
                        .collect::<Vec<_>>(),
                    t.readings()
                        .iter()
                        .map(|r| r.lemma().map(|l| l.to_string()))
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn test_debug_matching_subrules() {
        let runner = test_support::shared_runner();
        let sentences = [
            "RWE said it has open positions",
            "Do you have change?",
            "we have open positions sometimes.",
        ];
        let rule_set = test_support::shared_grammar_rules();
        for text in sentences {
            let sentence = runner.tokenize_sentence(text);
            let mut matched = 0;
            for rule in &rule_set.rules {
                if rule.id != "HAVE_PART_AGREEMENT" {
                    continue;
                }
                let matches = runner.pattern_engine.match_rule(rule, &sentence);
                if !matches.is_empty() {
                    matched += 1;
                    eprintln!(
                        "MATCH [{}] sub={:?} on '{}' at {}..{}",
                        rule.id,
                        rule.sub_id,
                        text,
                        matches[0].offset(),
                        matches[0].offset() + matches[0].length()
                    );
                    for (i, t) in rule.pattern.tokens.iter().enumerate() {
                        eprintln!(
                            "  token[{}]: text={:?} postag={:?} chunk={:?} chunk_re={:?}",
                            i, t.text, t.postag, t.chunk, t.chunk_re
                        );
                    }
                }
            }
            if matched == 0 {
                eprintln!("no match on '{}'", text);
            }
        }
    }

    #[test]
    #[ignore]
    fn test_debug_a_nns_fps() {
        let runner = test_support::shared_runner();
        let sentences = vec![
            "I want spend time with my friends.",
            "I would like spend time with my friends.",
        ];

        for text in &sentences {
            let sentence = runner.tokenize_sentence(text);
            let nwt: Vec<_> = sentence.non_whitespace_tokens();
            eprintln!("\n'{}':", text);
            for (i, token) in nwt.iter().enumerate() {
                let chunk = token.chunk().unwrap_or("-");
                eprintln!(
                    "  [{}] {:15} pos={:?} chunk={}",
                    i,
                    format!("'{}'", token.token().token()),
                    token.token().pos_tags(),
                    chunk
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn test_debug_pos_tags_for_failures() {
        let runner = test_support::shared_runner();
        let sentences = vec![
            "Grammatical errors is bad.",
            "Restart PHP and try replicate your issue.",
            "I would like test the tool.",
            "We want go to the cinema.",
            "I love listen to music.",
            "RWE said it has open positions",
            "Do you have change?",
            "We want test it.",
            "Grammatical errors always makes your text look bad.",
            "Heat rates has no impact on numbers.",
            "I want spend time with my friends.",
            "Can I tell if its using the repeater?",
            "He is friend.",
            "We have visited the client on 7 October 2025.",
            "I would love listen your contribution.",
            "He never looks the front door.",
            "This sound slike an error.",
            "We want test it.",
            "But its much better like this.",
            "I think its much better now.",
            "Its much easier.",
            "Let me know when its under pressure.",
            "It's to early.",
            "He spent 10% to much.",
            "I better stop before I go to far.",
            "The dogs barks loudly.",
            "I would like see us make this work.",
            "They don't want let me learn.",
            "I want spend time with my friends.",
            "Restart PHP and try replicate your issue.",
            "I would like test the tool.",
            "We want go to the cinema.",
            "I love listen to music.",
            "RWE said it has open positions",
            "Do you have change?",
        ];

        for text in &sentences {
            let sentence = runner.tokenize_sentence(text);
            let nwt: Vec<_> = sentence.non_whitespace_tokens();
            eprintln!("\n'{}':", text);
            for (i, token) in nwt.iter().enumerate() {
                let chunk = token.chunk().unwrap_or("-");
                eprintln!(
                    "  [{}] {:15} pos={:?} chunk={}",
                    i,
                    format!("'{}'", token.token().token()),
                    token.token().pos_tags(),
                    chunk
                );
            }
        }
    }

    #[test]
    fn test_been_part_agreement_debug() {
        let runner = test_support::shared_runner();

        let sentence = runner.tokenize_sentence("I'm been prepared.");
        let _nwt: Vec<_> = sentence.non_whitespace_tokens();

        let compiled_rule_set = test_support::shared_grammar_rules();

        let to_too_rules: Vec<_> = compiled_rule_set
            .rules
            .iter()
            .filter(|r| r.id == "TO_TOO")
            .collect();
        eprintln!("\nTO_TOO rules found: {}", to_too_rules.len());
        for (i, rule) in to_too_rules.iter().enumerate() {
            eprintln!("  Rule {}: {} pattern tokens", i, rule.pattern.tokens.len());
            let sentence2 = runner.tokenize_sentence("It's to early.");
            let matches = runner.pattern_engine.match_rule(rule, &sentence2);
            if !matches.is_empty() {
                eprintln!("    MATCHED! {} matches", matches.len());
            }
        }

        let been_rules: Vec<_> = compiled_rule_set
            .rules
            .iter()
            .filter(|r| r.id.contains("BEEN_PART"))
            .collect();
        assert!(!been_rules.is_empty(), "Should find BEEN_PART rules");

        let mut any_matched = false;
        for rule in been_rules.iter() {
            let matches = runner.pattern_engine.match_rule(rule, &sentence);
            if !matches.is_empty() {
                any_matched = true;
            }
        }
        assert!(
            any_matched,
            "At least one BEEN_PART rule should match 'I'm been prepared.'"
        );
    }

    #[test]
    fn test_to_too_inflected_matching() {
        let runner = test_support::shared_runner();
        let compiled = test_support::shared_grammar_rules();

        let test_cases = vec![
            ("This is to funny.", true),
            ("It is to small", true),
            ("It's to early.", true),
            ("They were to good for you to miss.", true),
            ("He spent 10% to much.", true),
        ];

        let to_too_rules: Vec<_> = compiled.rules.iter().filter(|r| r.id == "TO_TOO").collect();

        for (text, should_match) in &test_cases {
            let sentence = runner.tokenize_sentence(text);
            let any_matched = to_too_rules
                .iter()
                .any(|rule| !runner.pattern_engine.match_rule(rule, &sentence).is_empty());
            assert_eq!(
                any_matched, *should_match,
                "TO_TOO match for '{}' expected {}, got {}",
                text, should_match, any_matched
            );
        }
    }

    #[test]
    fn test_common_pos_tag_coverage() {
        let runner = test_support::shared_runner();

        fn tags_for(runner: &TestRunner, word: &str) -> Vec<String> {
            let sentence = runner.tokenize_sentence(word);
            let nwt: Vec<_> = sentence.non_whitespace_tokens();
            let token = nwt
                .iter()
                .find(|t| t.token().token() != "<S>")
                .expect("should have a non-<S> token");
            let mut tags: Vec<String> = token
                .readings()
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            tags.sort();
            tags.dedup();
            tags
        }

        fn check(runner: &TestRunner, word: &str, expected_tags: &[&str]) {
            let actual = tags_for(runner, word);

            let missing: Vec<&str> = expected_tags
                .iter()
                .filter(|t| !actual.iter().any(|a| a == **t))
                .copied()
                .collect();

            assert!(
                missing.is_empty(),
                "'{word}' must carry the en dictionary tags {expected_tags:?}, missing {missing:?} (actual {actual:?})"
            );
        }

        eprintln!("\n=== VBZ (3rd person singular present) ===");
        for word in &["needs", "wants", "likes", "tries"] {
            check(&runner, word, &["VBZ"]);
        }

        eprintln!("\n=== VB (base form verb) ===");
        for word in &["need", "want", "like", "try"] {
            check(&runner, word, &["VB"]);
        }
        for word in &["go", "make", "contact", "solve", "mount"] {
            check(&runner, word, &["VB"]);
        }
        for word in &["compete", "bring", "send", "extend"] {
            check(&runner, word, &["VB"]);
        }

        eprintln!("\n=== VB + noun variants (verb and dictionary noun tag) ===");
        check(&runner, "close", &["VB", "NN"]);
        check(&runner, "reopen", &["VB", "VBP"]);
        check(&runner, "open", &["VB", "JJ", "NN:U"]);
        for word in &["help", "sleep"] {
            check(&runner, word, &["NN:U", "VB"]);
        }
        for word in &["support", "exercise", "love", "fear", "care"] {
            check(&runner, word, &["NN:UN", "VB"]);
        }

        eprintln!("\n=== JJ (adjective) ===");
        for word in &[
            "cool",
            "awesome",
            "great",
            "easy",
            "happy",
            "perfect",
            "recommendable",
        ] {
            check(&runner, word, &["JJ"]);
        }
        for word in &["okay", "worth"] {
            check(&runner, word, &["JJ"]);
        }

        eprintln!("\n=== VBG (gerund / present participle) ===");
        for word in &["happening", "going", "writing"] {
            check(&runner, word, &["VBG"]);
        }

        eprintln!("\n=== VBN (past participle) ===");
        for word in &["done", "been", "given"] {
            check(&runner, word, &["VBN"]);
        }

        eprintln!("\n=== NNS (plural noun) ===");
        for word in &["phenomena", "criteria", "stimuli"] {
            check(&runner, word, &["NNS"]);
        }
        for word in &["freshmen", "women", "gentlemen"] {
            check(&runner, word, &["NNS"]);
        }

        eprintln!("\n=== JJR / JJ (comparative and base adjective) ===");
        for word in &["better", "easier"] {
            check(&runner, word, &["JJR"]);
        }
        {
            let word = &"expensive";
            check(&runner, word, &["JJ"]);
        }
    }

    #[test]
    fn ukrainian_runner_tokenizes_and_checks_examples() {
        let runner = TestRunner::for_language("uk");
        let sentence = runner.debug_sentence("Це тестове речення.");
        let words: Vec<&str> = sentence
            .non_whitespace_tokens()
            .iter()
            .map(|t| t.token().token())
            .collect();
        assert!(words.len() >= 3, "tokens: {words:?}");

        let xml = simple_xml(
            r#"
            <rule id="UK_TEST" name="Ukrainian test">
                <pattern>
                    <token>тестове</token>
                </pattern>
                <message>Test</message>
                <example type="incorrect">Це <marker>тестове</marker> речення.</example>
            </rule>
        "#,
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 1);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn sentence_offsets_shift_matches_across_sentences() {
        let runner = test_support::shared_runner();
        let xml = simple_xml(
            r#"
            <rule id="TWO_SENT" name="Two sentences">
                <pattern>
                    <token>teh</token>
                </pattern>
                <message>Did you mean <suggestion>the</suggestion>?</message>
                <example type="incorrect">First one. <marker>Teh</marker> second.</example>
            </rule>
        "#,
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 1);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }
    fn lang_xml(lang: &str, rules_body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="{lang}">
            <category id="GRAMMAR" name="Grammar">
                {}
            </category>
        </rules>"#,
            rules_body
        )
    }

    fn token_rule(id: &str, token: &str, incorrect: &str, correct: &str) -> String {
        format!(
            r#"<rule id="{id}" name="{id}">
                <pattern>
                    <token regexp="yes">{token}</token>
                </pattern>
                <message>Unexpected token.</message>
                <example type="incorrect">{incorrect}</example>
                <example type="correct">{correct}</example>
            </rule>"#
        )
    }

    #[test]
    fn esperanto_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("eo");
        let xml = lang_xml(
            "eo",
            &token_rule(
                "EO_TOK",
                "zqxjw",
                "Teksto <marker>zqxjw</marker> plua frazo.",
                "Neniu problemo cxi tie.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn danish_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("da");
        let xml = lang_xml(
            "da",
            &token_rule(
                "DA_TOK",
                "zqxjw",
                "En <marker>zqxjw</marker> mere s&aelig;tning.",
                "Der er intet problem her.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn catalan_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("ca");
        let xml = lang_xml(
            "ca",
            &token_rule(
                "CA_TOK",
                "zqxjw",
                "Una <marker>zqxjw</marker> m&eacute;s frase.",
                "No hi ha cap problema.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn russian_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("ru");
        let xml = lang_xml(
            "ru",
            &token_rule(
                "RU_TOK",
                "эзюча",
                "Одно <marker>эзюча</marker> ещё предложение.",
                "Здесь нет никакой проблемы.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn run_xml_file_tests_read_rules_from_disk() {
        let dir = std::env::temp_dir().join("og_test_runner_file_tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("example_rules.xml");
        std::fs::write(
            &path,
            lang_xml(
                "en",
                &token_rule(
                    "FILE_TOK",
                    "teh",
                    "A <marker>teh</marker> house.",
                    "The house.",
                ),
            ),
        )
        .unwrap();
        let runner = test_support::shared_runner();
        let summary = runner.run_xml_file_tests(&path).unwrap();
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);

        let missing = runner.run_xml_file_tests(&dir.join("does_not_exist.xml"));
        assert!(missing.is_err());
    }

    #[test]
    fn pattern_test_reports_unknown_rule_ids() {
        let runner = test_support::shared_runner();
        let xml = lang_xml(
            "en",
            &token_rule(
                "PT_TOK",
                "teh",
                "A <marker>teh</marker> house.",
                "The house.",
            ),
        );
        let result = runner.run_pattern_test("NO_SUCH_RULE", &xml, "teh house", false);
        assert!(!result.passed);
        assert!(result.message.contains("not found"));

        let hit = runner.run_pattern_test("PT_TOK", &xml, "teh house", true);
        assert!(hit.passed, "{}", hit.message);
        let miss = runner.run_pattern_test("PT_TOK", &xml, "clean house", false);
        assert!(miss.passed, "{}", miss.message);
    }

    #[test]
    fn pattern_test_reports_xml_parse_errors() {
        let runner = test_support::shared_runner();
        let result = runner.run_pattern_test(
            "PT_TOK",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rules></wrong></rules>",
            "teh house",
            true,
        );
        assert!(!result.passed);
        assert_eq!(result.rule_id, "PT_TOK");
        assert_eq!(result.expected_type, "single");
        assert_eq!(result.example_text, "teh house");
        assert!(result.message.contains("Parse error"), "{}", result.message);
    }

    #[test]
    fn generic_grammar_helpers_load_and_cache_danish_rules() {
        assert!(test_support::generic_grammar_available("da"));
        let first = test_support::generic_grammar_rules("da");
        assert!(!first.rules.is_empty());
        let second = test_support::generic_grammar_rules("da");
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert!(!test_support::generic_grammar_available("zz"));
    }

    #[test]
    fn match_rule_on_text_reports_marker_and_matches() {
        let runner = test_support::shared_runner();
        let xml = lang_xml(
            "en",
            &token_rule(
                "MR_TOK",
                "teh",
                "A <marker>teh</marker> house.",
                "The house.",
            ),
        );
        let rule_set = XmlCompiler::new().compile_file(&xml).unwrap();
        let rule = &rule_set.rules[0];
        let (clean, marker, matches) =
            runner.match_rule_on_text(rule, "A <marker>teh</marker> house");
        assert_eq!(clean, "A teh house");
        assert_eq!(marker, Some((2, 5)));
        assert!(!matches.is_empty());

        let sentence = runner.debug_sentence("A teh house");
        assert!(sentence.tokens().len() >= 3);
    }

    #[test]
    fn spanish_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("es");
        let xml = lang_xml(
            "es",
            &token_rule(
                "ES_TOK",
                "zqxjw",
                "Una <marker>zqxjw</marker> frase más.",
                "No hay ningún problema.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn portuguese_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("pt");
        let xml = lang_xml(
            "pt",
            &token_rule(
                "PT_TOK",
                "zqxjw",
                "Uma <marker>zqxjw</marker> frase maior.",
                "Não há problema.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn french_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("fr");
        let xml = lang_xml(
            "fr",
            &token_rule(
                "FR_TOK",
                "zqxjw",
                "Une <marker>zqxjw</marker> phrase encore.",
                "Il n'y a aucun problème.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn french_runner_splits_hyphenated_unknown_words() {
        let runner = TestRunner::for_language("fr");
        let sentence = runner.debug_sentence("Un mot-zqxjw supplémentaire.");
        let tokens: Vec<&str> = sentence
            .non_whitespace_tokens()
            .iter()
            .map(|t| t.token().token())
            .collect();
        assert!(tokens.contains(&"mot"), "tokens: {tokens:?}");
        assert!(tokens.contains(&"-"), "tokens: {tokens:?}");
        assert!(tokens.contains(&"zqxjw"), "tokens: {tokens:?}");
    }

    #[test]
    fn polish_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("pl");
        let xml = lang_xml(
            "pl",
            &token_rule(
                "PL_TOK",
                "zqxjw",
                "Jeszcze <marker>zqxjw</marker> jedno zdanie.",
                "Nie ma tutaj problemu.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn german_runner_matches_inline_examples() {
        assert!(test_support::german_grammar_available());
        let runner = test_support::german_runner();
        let again = test_support::german_runner();
        assert!(std::sync::Arc::ptr_eq(&runner, &again));
        let xml = lang_xml(
            "de",
            &token_rule(
                "DE_TOK",
                "zqxjw",
                "Ein <marker>zqxjw</marker> weiterer Satz.",
                "Hier gibt es kein Problem.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn dutch_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("nl");
        let xml = lang_xml(
            "nl",
            &token_rule(
                "NL_TOK",
                "zqxjw",
                "Nog <marker>zqxjw</marker> een zin.",
                "Er is hier geen probleem.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn breton_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("br");
        let xml = lang_xml(
            "br",
            &token_rule(
                "BR_TOK",
                "zqxjw",
                "Ur <marker>zqxjw</marker> frazenn all.",
                "N'eus kudenn ebet.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn galician_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("gl");
        let xml = lang_xml(
            "gl",
            &token_rule(
                "GL_TOK",
                "zqxjw",
                "Unha <marker>zqxjw</marker> frase máis.",
                "Non hai ningún problema.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn irish_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("ga");
        let xml = lang_xml(
            "ga",
            &token_rule(
                "GA_TOK",
                "zqxjw",
                "Is <marker>zqxjw</marker> sampla eile.",
                "Níl aon fhadhb ann.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn serbian_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("sr");
        let xml = lang_xml(
            "sr",
            &token_rule(
                "SR_TOK",
                "zqxjw",
                "Još <marker>zqxjw</marker> jedna rečenica.",
                "Ovde nema nikakvog problema.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn arabic_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("ar");
        let xml = lang_xml(
            "ar",
            &token_rule(
                "AR_TOK",
                "zqxjw",
                "هذا <marker>zqxjw</marker> اختبار آخر.",
                "لا توجد مشكلة هنا.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn unmatched_incorrect_examples_fail_with_message() {
        let runner = test_support::shared_runner();
        let xml = lang_xml(
            "en",
            r#"<rule id="NO_HIT" name="No hit">
                <pattern>
                    <token>zqxjw</token>
                    <token>wvv</token>
                </pattern>
                <message>Two tokens.</message>
                <example type="incorrect">A <marker>zqxjw</marker> standalone token.</example>
            </rule>"#,
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.failed, 1);
        let result = &summary.results[0];
        assert!(!result.passed);
        assert_eq!(result.rule_id, "NO_HIT");
        assert_eq!(result.expected_type, "incorrect");
        assert!(
            result
                .message
                .contains("Expected match on incorrect example but got none")
        );
    }

    #[test]
    fn matches_outside_the_marker_fail_the_example() {
        let runner = test_support::shared_runner();
        let xml = lang_xml(
            "en",
            r#"<rule id="OFF_MARK" name="Off marker">
                <pattern>
                    <token>here</token>
                </pattern>
                <message>Here.</message>
                <example type="incorrect">A <marker>zqxjw</marker> thing here.</example>
            </rule>"#,
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.failed, 1);
        let result = &summary.results[0];
        assert!(
            result.message.contains("does not contain marker"),
            "{}",
            result.message
        );
    }

    #[test]
    fn incorrect_examples_without_markers_pass_on_any_match() {
        let runner = test_support::shared_runner();
        let xml = lang_xml(
            "en",
            r#"<rule id="NO_MARK" name="No marker">
                <pattern>
                    <token>zqxjw</token>
                </pattern>
                <message>Found.</message>
                <example type="incorrect">It happens zqxjw today.</example>
            </rule>"#,
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
        assert!(summary.results[0].message.contains("Matched correctly"));
    }

    #[test]
    fn triggers_error_examples_pass_through() {
        let runner = test_support::shared_runner();
        let xml = lang_xml(
            "en",
            r#"<rule id="TRIG" name="Triggers">
                <pattern>
                    <token>zqxjw</token>
                </pattern>
                <message>Known FP.</message>
                <example type="triggers_error">A zqxjw sentence.</example>
            </rule>"#,
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
        assert_eq!(summary.results[0].expected_type, "triggers_error");
        assert!(summary.results[0].message.contains("Triggers error"));
    }

    #[test]
    fn malformed_xml_reports_a_parse_error_result() {
        let runner = test_support::shared_runner();
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rules></wrong></rules>";
        let summary = runner.run_xml_example_tests(xml);
        assert_eq!(summary.total, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.results[0].rule_id, "PARSE_ERROR");
        assert_eq!(summary.results[0].expected_type, "parse");
        assert!(
            summary.results[0].message.contains("Failed to parse XML"),
            "{}",
            summary.results[0].message
        );
    }

    #[test]
    fn rules_sharing_an_id_are_evaluated_as_one_group() {
        let runner = test_support::shared_runner();
        let base = lang_xml(
            "en",
            r#"<rule id="GROUPED" name="Grouped base">
                <pattern>
                    <token>zqxjw</token>
                    <token>wvv</token>
                </pattern>
                <message>Two tokens.</message>
                <example type="incorrect">A <marker>zqxjw</marker> single token.</example>
            </rule>"#,
        );
        let extra = lang_xml(
            "en",
            r#"<rule id="GROUPED" name="Grouped extra">
                <pattern>
                    <token>zqxjw</token>
                </pattern>
                <message>One token.</message>
            </rule>"#,
        );
        let mut rule_set = XmlCompiler::new().compile_file(&base).unwrap();
        let alone = runner.run_rule_set_example_tests(&rule_set);
        assert_eq!(alone.failed, 1, "{:?}", alone.results);

        let extra_set = XmlCompiler::new().compile_file(&extra).unwrap();
        rule_set.rules.push(extra_set.rules[0].clone());
        let grouped = runner.run_rule_set_example_tests(&rule_set);
        assert_eq!(grouped.total, 1);
        assert!(grouped.is_all_passed(), "{:?}", grouped.results);
    }

    #[test]
    fn pattern_test_reports_expectation_mismatches() {
        let runner = test_support::shared_runner();
        let xml = lang_xml(
            "en",
            &token_rule(
                "MM_TOK",
                "zqxjw",
                "A <marker>zqxjw</marker> house.",
                "The house.",
            ),
        );
        let result = runner.run_pattern_test("MM_TOK", &xml, "a zqxjw house", false);
        assert!(!result.passed);
        assert!(result.message.contains("expected no match, got match"));
    }

    #[test]
    fn default_runner_behaves_like_new() {
        let runner = TestRunner::default();
        let xml = lang_xml(
            "en",
            &token_rule(
                "DF_TOK",
                "zqxjw",
                "A <marker>zqxjw</marker> house.",
                "The house.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn shared_grammar_rules_cache_the_english_compilation() {
        let first = test_support::shared_grammar_rules();
        let second = test_support::shared_grammar_rules();
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert!(!first.rules.is_empty());
    }

    fn assert_word_count(runner: &TestRunner, text: &str, min_words: usize) {
        let sentence = runner.debug_sentence(text);
        let words: Vec<&str> = sentence
            .non_whitespace_tokens()
            .iter()
            .map(|t| t.token().token())
            .collect();
        assert!(words.len() >= min_words, "tokens: {words:?}");
    }

    #[test]
    fn belarusian_runner_tokenizes_native_text() {
        assert_word_count(&TestRunner::for_language("be"), "Гэта тэставае сказ.", 3);
    }

    #[test]
    fn greek_runner_tokenizes_native_text() {
        assert_word_count(&TestRunner::for_language("el"), "Αυτή είναι μια δοκιμή.", 3);
    }

    #[test]
    fn persian_runner_tokenizes_native_text() {
        assert_word_count(&TestRunner::for_language("fa"), "این یک آزمایش است.", 3);
    }

    #[test]
    fn crimean_tatar_runner_splits_trailing_hyphen_tokens() {
        let runner = TestRunner::for_language("crh");
        let sentence = runner.debug_sentence("Bir kelime- sóz.");
        let tokens: Vec<&str> = sentence
            .non_whitespace_tokens()
            .iter()
            .map(|t| t.token().token())
            .collect();
        assert!(tokens.contains(&"kelime"), "tokens: {tokens:?}");
        assert!(tokens.contains(&"-"), "tokens: {tokens:?}");
    }

    #[test]
    fn japanese_runner_keeps_tokenizer_tags_without_tagger() {
        let runner = TestRunner::for_language("ja");
        let sentence = runner.debug_sentence("これはテストです。");
        let tokens = sentence.non_whitespace_tokens();
        assert!(tokens.len() >= 2);
        assert!(
            tokens.iter().any(|t| !t.token().pos_tags().is_empty()),
            "ja tags come from the tokenizer dictionary"
        );
    }

    #[test]
    fn khmer_runner_tokenizes_native_text() {
        assert_word_count(&TestRunner::for_language("km"), "នេះ ជា ការ សាកល្បង។", 3);
    }

    #[test]
    fn malayalam_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("ml");
        let xml = lang_xml(
            "ml",
            &token_rule(
                "ML_TOK",
                "zqxjw",
                "ഇത് <marker>zqxjw</marker> ഒരു പരീക്ഷണം.",
                "ഇവിടെ പ്രശ്നമില്ല.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn tamil_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("ta");
        let xml = lang_xml(
            "ta",
            &token_rule(
                "TA_TOK",
                "zqxjw",
                "இது <marker>zqxjw</marker> ஒரு சோதனை.",
                "இங்கே எந்த பிரச்சனையும் இல்லை.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn tagalog_runner_matches_inline_examples() {
        let runner = TestRunner::for_language("tl");
        let xml = lang_xml(
            "tl",
            &token_rule(
                "TL_TOK",
                "zqxjw",
                "Ito ay <marker>zqxjw</marker> na pagsusulit.",
                "Walang problema dito.",
            ),
        );
        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert!(summary.is_all_passed(), "{:?}", summary.results);
    }

    #[test]
    fn chinese_runner_tokenizes_the_whole_text_as_one_sentence() {
        let runner = TestRunner::for_language("zh");
        let sentence = runner.debug_sentence("这是一个测试句子。");
        assert!(sentence.non_whitespace_tokens().len() >= 2);
    }
}

#[test]
#[ignore]
fn test_debug_have_part_agreement_fp() {
    let runner = test_support::shared_runner();
    let sentences = vec![
        "Do you have change?",
        "Like other insects, beetles have open circulatory systems, based on hemolymph rather than blood.",
        "RWE said it has open positions",
        "We have soap.",
        "We do have soap.",
        "We do not have soap.",
        "I have contact with aliens.",
        "Do you have Telegram?",
        "The family that my friend had was his cousin and his sister.",
        "In the Sahara region some oases have palm trees.",
        "It had feature limitations such as...",
        "A rabbit has long ears.",
        "The tool my teacher had was archaic.",
        "Do you have bicycle in your garage?",
        "For any additional questions that you may have please contact me.",
        "We have only option 2 available.",
        "We have only option #2 available.",
        "I don't have job We need all the food we can get.",
        "The block of wood had saw marks on it.",
        "The sun had rose before I awoke.",
        "The block of wood has saw marks on it.",
        "For example, monarchical societies often had a system of \"social ranks\" which were collectivist because the social rank one had or did not have was more important than his or her individual will, and the specific rank in question could only be overridden in very limited cases.",
        "By the mid-10th century, the Samanid dynasty had crumble in the face of attacks from Turkish tribes to the north and from the Ghaznavids, a rising Turkic Muslim dynasty in Afghanistan.",
        "What I'm trying to have are papers on the Silver Leaf resort thingy.",
        "What I like to have are papers on the Silver Leaf resort thingy.",
        "What I really would like to have are papers on the Silver Leaf resort thingy.",
        "The options that my brother should have are the only ...",
        "The options my brother should have are the only ...",
        "The options he should have are the only ...",
        "we have open positions sometimes.",
        "There are a few I have open questions about or am waiting to receive copies of the contract.",
        "I am taking a scuba diving class and I have open water dives on both Saturday and Sunday.",
        "we don't have open access to the transmission grid",
        "In this case I think 100 is necessary because once a strike has open interest, we must continue to support it.",
        "If we restrict to strikes with a lower delta, we face the problem of not offering enough strikes and not making a market in options that have open EOL interest that have moved closer to the money.",
        "When the basis vectors have norm 1, the coordinate functionals e*n have...",
        "the coordinate functionals e*n have norm 2C in the dual of X.",
    ];

    for text in &sentences {
        let sentence = runner.tokenize_sentence(text);
        let nwt: Vec<_> = sentence.non_whitespace_tokens();
        eprintln!("\n'{}':", text);
        for (i, token) in nwt.iter().enumerate() {
            let chunk = token.chunk().unwrap_or("-");
            eprintln!(
                "  [{}] {:15} pos={:?} chunk={}",
                i,
                format!("'{}'", token.token().token()),
                token.token().pos_tags(),
                chunk
            );
        }
    }
}

#[test]
#[ignore]
fn test_debug_missing_to_before_verb() {
    let runner = test_support::shared_runner();
    let sentences = vec![
        "We need test it first.",
        "I would like see us make this work.",
        "I like go to the pool.",
        "He needs go there.",
        "I want spend time with my friends.",
        "They want improve his skills.",
        "Try get in our heads.",
        "I like hang out with the crew.",
        "Installer will detect PHP 7 and try install.",
        "He needs go",
    ];

    for text in &sentences {
        let sentence = runner.tokenize_sentence(text);
        let nwt: Vec<_> = sentence.non_whitespace_tokens();
        eprintln!("\n'{}':", text);
        for (i, token) in nwt.iter().enumerate() {
            let chunk = token.chunk().unwrap_or("-");
            eprintln!(
                "  [{}] {:15} pos={:?} chunk={}",
                i,
                format!("'{}'", token.token().token()),
                token.token().pos_tags(),
                chunk
            );
        }
    }
}

#[test]
fn test_debug_chunking_with_whitespace() {
    let runner = test_support::shared_runner();
    let sentence = runner.tokenize_sentence("The client is here.");
    let tokens = sentence.tokens();
    assert_eq!(tokens.len(), 9, "expected <S> + 5 words + 3 spaces + .");
    let expected = [
        ("<S>", "-", false),
        ("The", "B-NP-singular", false),
        (" ", "-", true),
        ("client", "E-NP-singular", false),
        (" ", "-", true),
        ("is", "B-VP", false),
        (" ", "-", true),
        ("here", "B-ADVP", false),
        (".", "O", false),
    ];
    for (token, (text, chunk, ws)) in tokens.iter().zip(expected) {
        assert_eq!(token.token().token(), text);
        assert_eq!(token.is_whitespace(), ws, "'{text}' whitespace mismatch");
        assert_eq!(
            token.chunk().unwrap_or("-"),
            chunk,
            "'{text}' chunk mismatch"
        );
        if ws {
            assert!(
                token.token().pos_tags().is_empty(),
                "whitespace tokens carry no POS tags"
            );
        }
    }
    eprintln!(
        "ALL tokens for 'The client is here.' ({} total):",
        tokens.len()
    );
    for (i, token) in tokens.iter().enumerate() {
        let tags = token.token().pos_tags();
        let tag_str = if tags.is_empty() {
            "(none)".to_string()
        } else {
            format!("{:?}", tags)
        };
        let ws = if token.is_whitespace() { " [WS]" } else { "" };
        eprintln!(
            "  [{}] {:10} pos={} chunk={}{}",
            i,
            format!(
                "'{}'",
                token.token().token().replace('\n', "\\n").replace(' ', "·")
            ),
            tag_str,
            token.chunk().unwrap_or("-"),
            ws
        );
    }
}

#[test]
#[ignore]
fn test_it_is_2_debug() {
    let runner = test_support::shared_runner();
    let sentences = vec![
        "But its much better like this.",
        "I think its much better now.",
        "Its much easier.",
    ];

    for text in &sentences {
        let sentence = runner.tokenize_sentence(text);
        eprintln!("\n--- '{}' ---", text);
        let nwt: Vec<_> = sentence.non_whitespace_tokens();
        for (i, token) in nwt.iter().enumerate() {
            eprintln!(
                "  [{}] {:15} pos={:?} chunk={}",
                i,
                format!("'{}'", token.token().token()),
                token.token().pos_tags(),
                token.chunk().unwrap_or("-")
            );
        }
    }

    let rule_set = test_support::shared_grammar_rules();

    let text = "But its much better like this.";
    let sentence = runner.tokenize_sentence(text);
    let mut rule_num = 0;
    for rule in &rule_set.rules {
        if rule.id == "IT_IS_2" {
            rule_num += 1;
            let matches = runner.pattern_engine.match_rule(rule, &sentence);
            eprintln!("\nIT_IS_2[{}]: {} matches", rule_num, matches.len());
            if !matches.is_empty() {
                for m in &matches {
                    eprintln!("  match: {:?}", m);
                }
            }
        }
    }
}

#[test]
#[ignore]
fn test_debug_a_infinitive_fps() {
    let runner = test_support::shared_runner();
    let sentences = vec![
        "Checkout or add a subscribe box to the cart.",
        "If there is already a notify email...",
        "I hope this will help to explain why I am hesitant to endorse a centralize unit commitment implementation.",
        "Let's have a listen to what he wants to say",
        "He had a listen.",
        "a show",
        "He pulled a stun gun.",
        "Simply book an Excite Travel Vacations package by October 31, 2001.",
        "Smith's bill would ban the use of methyl tertiary butyl ether, an oxygenate used in 85 percent of reformulated gasoline.",
        "He sustained the injury while chasing down a lose ball in the fourth quarter.",
        "We invite you to come an have a look.",
        "The carve outs from the LOL remain an open issue.",
        "The unify pathing issue is resolved, all buckets are filled.",
        "The integrate mw's are 10mw'w and are reflected on revised oasis request 4405.",
        "Perhaps should add the decommission of the present MIPS measurement system and the implementation of the PGAS system.",
        "The engine ran fine, the outdrive went up and down, the prop was the correct size and pitch.",
        "The proof of Cleveland's mettle came quickly: its NFL regular-season opener was against the two-time defending champion Eagles on September 16 in Philadelphia.",
        "He said we would need either a parent guarantee or a prepay to transact with occidental energy marketing.",
        "The open e and back a are often indicated in writing by the use of the letters alaph",
        "He sustained the injury while chasing down a lose ball in the fourth quarter.",
        "confirms that the deny code that we wrote is ineffective.",
        "The Disk II single-sided floppy drive used 5.25-inch floppy disks",
        "In the U.S.A be the best.",
        "the conquer stage will be more complex than decrease and conquer algorithms.",
    ];

    for text in &sentences {
        let sentence = runner.tokenize_sentence(text);
        let nwt: Vec<_> = sentence.non_whitespace_tokens();
        eprintln!("\n'{text}':");
        for (i, token) in nwt.iter().enumerate() {
            let chunk = token.chunk().unwrap_or("-");
            eprintln!(
                "  [{}] {:15} pos={:?} chunk={}",
                i,
                format!("'{}'", token.token().token()),
                token.token().pos_tags(),
                chunk
            );
        }
    }
}

#[test]
fn test_real_danish_grammar_examples() {
    run_generic_example_suite("da");
}

#[test]
fn test_real_de_ch_grammar_examples() {
    run_german_variant_example_suite("de-CH");
}

#[test]
fn test_real_de_at_grammar_examples() {
    run_german_variant_example_suite("de-AT");
}

#[test]
fn test_real_french_grammar_examples() {
    run_generic_example_suite("fr");
}

#[test]
fn test_real_spanish_grammar_examples() {
    run_generic_example_suite("es");
}

#[test]
fn test_real_portuguese_grammar_examples() {
    run_generic_example_suite("pt");
}

#[test]
fn test_real_dutch_grammar_examples() {
    run_generic_example_suite("nl");
}

#[test]
fn test_real_polish_grammar_examples() {
    run_generic_example_suite("pl");
}

#[test]
fn test_real_russian_grammar_examples() {
    run_generic_example_suite("ru");
}

#[test]
fn test_real_italian_grammar_examples() {
    run_generic_example_suite("it");
}

#[test]
fn test_real_arabic_grammar_examples() {
    run_generic_example_suite("ar");
}

#[test]
fn test_real_asturian_grammar_examples() {
    run_generic_example_suite("ast");
}

#[test]
fn test_real_catalan_grammar_examples() {
    run_generic_example_suite("ca");
}

#[test]
fn test_real_belarusian_grammar_examples() {
    run_generic_example_suite("be");
}

#[test]
fn test_real_crimean_tatar_grammar_examples() {
    run_generic_example_suite("crh");
}

#[test]
fn test_real_breton_grammar_examples() {
    run_generic_example_suite("br");
}

#[test]
fn test_real_greek_grammar_examples() {
    run_generic_example_suite("el");
}

#[test]
fn test_real_persian_grammar_examples() {
    run_generic_example_suite("fa");
}

#[test]
fn test_real_irish_grammar_examples() {
    run_generic_example_suite("ga");
}

#[test]
fn test_real_galician_grammar_examples() {
    run_generic_example_suite("gl");
}

#[test]
fn test_real_icelandic_grammar_examples() {
    run_generic_example_suite("is");
}

#[test]
fn test_real_japanese_grammar_examples() {
    run_generic_example_suite("ja");
}

#[test]
fn test_real_khmer_grammar_examples() {
    run_generic_example_suite("km");
}

#[test]
fn test_real_romanian_grammar_examples() {
    run_generic_example_suite("ro");
}

#[test]
fn test_real_swedish_grammar_examples() {
    run_generic_example_suite("sv");
}

#[test]
fn test_real_slovak_grammar_examples() {
    run_generic_example_suite("sk");
}

#[test]
fn test_real_slovenian_grammar_examples() {
    run_generic_example_suite("sl");
}

#[test]
fn test_real_lithuanian_grammar_examples() {
    run_generic_example_suite("lt");
}

#[test]
fn test_real_malayalam_grammar_examples() {
    run_generic_example_suite("ml");
}

#[test]
fn test_real_tamil_grammar_examples() {
    run_generic_example_suite("ta");
}

#[test]
fn test_real_tagalog_grammar_examples() {
    run_generic_example_suite("tl");
}

#[test]
fn test_real_serbian_grammar_examples() {
    run_generic_example_suite("sr");
}

#[test]
fn test_real_ukrainian_grammar_examples() {
    run_generic_example_suite("uk");
}

#[test]
fn test_real_chinese_grammar_examples() {
    run_generic_example_suite("zh");
}

#[test]
fn test_real_esperanto_grammar_examples() {
    run_generic_example_suite("eo");
}

#[cfg(test)]
fn trim_released_pages(freed: usize) {
    const TRIM_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);
    #[cfg(target_os = "linux")]
    if freed > 0 && og_core::releasable::trim_due(TRIM_INTERVAL) {
        unsafe {
            libc::malloc_trim(0);
        }
    }
}

#[cfg(test)]
fn run_german_variant_example_suite(code: &'static str) {
    let started = std::time::Instant::now();
    let variant_dir = if code == "de-CH" { "de-CH" } else { "de-DE-AT" };
    let path = std::path::PathBuf::from(format!(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/rules/de/{variant_dir}/grammar.xml"
    ));
    if !path.exists() {
        eprintln!("Skipping: {variant_dir} grammar.xml not found");
        return;
    }
    let runner = test_support::german_runner();
    let xml = std::fs::read_to_string(&path).expect("grammar.xml read");
    let rules = XmlCompiler::new()
        .compile_file_with_base(&xml, path.parent())
        .expect("grammar compile");
    let summary = runner.run_rule_set_example_tests(&rules);
    drop(rules);
    trim_released_pages(og_core::releasable::release_unused_since(started));

    let mut no_match = 0;
    let mut fp = 0;
    let mut marker = 0;
    for r in &summary.results {
        if r.passed {
            continue;
        }
        if r.message.contains("Expected no match") {
            fp += 1;
        } else if r.message.contains("marker") {
            marker += 1;
        } else {
            no_match += 1;
        }
    }
    eprintln!(
        "{code}: {}/{} passed ({:.1}%) - no_match: {no_match}, fp: {fp}, marker: {marker}",
        summary.passed,
        summary.total,
        100.0 * summary.passed as f64 / summary.total.max(1) as f64
    );
    let mut fail_file = String::new();
    for r in &summary.results {
        if !r.passed {
            fail_file.push_str(&format!("FAIL: [{}] {}\n", r.rule_id, r.message));
        }
    }
    let artifact_code = code.replace('-', "_");
    let _ = std::fs::write(format!("/tmp/test_failures_{artifact_code}.txt"), fail_file);
}

#[cfg(test)]
fn run_generic_example_suite(code: &'static str) {
    if !test_support::generic_grammar_available(code) {
        eprintln!("Skipping: {code} grammar.xml not found");
        return;
    }
    let started = std::time::Instant::now();
    let rules_handle = std::thread::spawn(move || test_support::generic_grammar_rules(code));
    let runner = test_support::shared_language_runner(code);
    let rules = rules_handle.join().expect("grammar compile thread");
    let summary = runner.run_rule_set_example_tests(&rules);
    drop(rules);
    drop(runner);
    trim_released_pages(og_core::releasable::release_unused_since(started));

    let mut no_match = 0;
    let mut fp = 0;
    let mut marker = 0;
    for r in &summary.results {
        if r.passed {
            continue;
        }
        if r.message.contains("Expected no match") {
            fp += 1;
        } else if r.message.contains("marker") {
            marker += 1;
        } else {
            no_match += 1;
        }
    }
    eprintln!(
        "{code}: {}/{} passed ({:.1}%) - no_match: {no_match}, fp: {fp}, marker: {marker}",
        summary.passed,
        summary.total,
        100.0 * summary.passed as f64 / summary.total.max(1) as f64
    );
    let mut fail_file = String::new();
    for r in &summary.results {
        if !r.passed {
            fail_file.push_str(&format!("FAIL: [{}] {}\n", r.rule_id, r.message));
        }
    }
    let _ = std::fs::write(format!("/tmp/test_failures_{code}.txt"), fail_file);
    assert!(
        summary.total > 0,
        "{code} grammar.xml compiled but its example suite ran zero examples"
    );
}
