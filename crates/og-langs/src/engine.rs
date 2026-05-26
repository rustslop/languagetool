use og_core::checker::{SentenceTokenizer, WordTokenizer};
use og_core::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings,
    CheckRequest, CheckResult, Checker, Language, RuleMatch,
    SentenceRange,
    rule::{Rule, TextLevelRule}, Category, IssueType,
};
use og_xml::compiler::{XmlCompiler, CompiledRule};
use og_rules::pattern_rule::PatternRuleEngine;
use og_tagger::EnglishTagger;
use og_tagger::XmlDisambiguator;
use og_spell::{Dictionary, SpellingCheckRule};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A rule loaded from an XML grammar file, wrapping the compiled rule
/// and delegating to PatternRuleEngine for matching.
pub struct XmlPatternRule {
    compiled: CompiledRule,
    engine: PatternRuleEngine,
}

impl XmlPatternRule {
    pub fn new(compiled: CompiledRule) -> Self {
        Self {
            compiled,
            engine: PatternRuleEngine::new(),
        }
    }
}

impl Rule for XmlPatternRule {
    fn id(&self) -> &str {
        &self.compiled.id
    }

    fn description(&self) -> &str {
        &self.compiled.name
    }

    fn is_default_on(&self) -> bool {
        self.compiled.default_on
    }

    fn category(&self) -> Category {
        Category::new(&self.compiled.category.id, &self.compiled.category.name)
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        self.engine.match_rule(&self.compiled, sentence)
    }
}

/// Language engine that loads XML rules for a specific language
/// and provides a full checking pipeline.
pub struct LanguageEngine {
    language: Language,
    rules: Vec<Arc<dyn Rule>>,
    text_level_rules: Vec<Arc<dyn TextLevelRule>>,
    tagger: Option<Arc<dyn og_core::checker::Tagger>>,
    disambiguator: Option<Arc<dyn og_core::checker::Disambiguator>>,
    checker: Checker,
}

impl LanguageEngine {
    pub fn new(language: Language) -> Self {
        let checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(SentenceSplitterBridge))
            .with_word_tokenizer(Arc::new(WordTokenizerBridge));

        Self {
            language,
            rules: Vec::new(),
            text_level_rules: Vec::new(),
            tagger: None,
            disambiguator: None,
            checker,
        }
    }

    /// Create an English LanguageEngine with all available rules
    pub fn english() -> Self {
        let mut tagger = EnglishTagger::new();

        // Load external data into the tagger
        if let Some(lt_root) = find_lt_resource_dir() {
            let dict_path = lt_root.join("en/src/main/resources/org/languagetool/resource/en/dict_decoded.txt");
            if dict_path.exists() {
                if let Ok(data) = std::fs::read_to_string(&dict_path) {
                    let count = tagger.load_fsa_dictionary(&data);
                    eprintln!("Loaded {} FSA dictionary entries", count);
                }
            }
            let added_path = lt_root.join("en/src/main/resources/org/languagetool/resource/en/added.txt");
            if added_path.exists() {
                if let Ok(data) = std::fs::read_to_string(&added_path) {
                    tagger.load_added(&data);
                }
            }
            let uncountable_path = lt_root.join("en/src/main/resources/org/languagetool/resource/en/uncountable.txt");
            if uncountable_path.exists() {
                if let Ok(data) = std::fs::read_to_string(&uncountable_path) {
                    tagger.load_uncountable(&data);
                }
            }
            let partlycountable_path = lt_root.join("en/src/main/resources/org/languagetool/resource/en/partlycountable.txt");
            if partlycountable_path.exists() {
                if let Ok(data) = std::fs::read_to_string(&partlycountable_path) {
                    tagger.load_partlycountable(&data);
                }
            }
        }

        let tagger_arc: Arc<dyn og_core::checker::Tagger> = Arc::new(tagger);

        let mut engine = Self {
            language: Language::English,
            rules: Vec::new(),
            text_level_rules: Vec::new(),
            tagger: Some(tagger_arc),
            disambiguator: None,
            checker: Checker::new()
                .with_sentence_tokenizer(Arc::new(SentenceSplitterBridge))
                .with_word_tokenizer(Arc::new(WordTokenizerBridge)),
        };

        // Load grammar.xml and disambiguation.xml
        if let Some(lt_root) = find_lt_resource_dir() {
            let grammar_path = grammar_xml_path("en", &lt_root);
            if grammar_path.exists() {
                if let Ok(count) = engine.load_xml_file(&grammar_path) {
                    eprintln!("Loaded {} XML rules for English", count);
                }
            }

            // Load disambiguation.xml
            let disambig_path = lt_root.join("en/src/main/resources/org/languagetool/resource/en/disambiguation.xml");
            if disambig_path.exists() {
                if let Ok(xml) = std::fs::read_to_string(&disambig_path) {
                    match XmlDisambiguator::from_xml(&xml) {
                        Ok(disambiguator) => {
                            eprintln!("Loaded {} disambiguation rules for English", disambiguator.rule_count());
                            engine.disambiguator = Some(Arc::new(disambiguator));
                        }
                        Err(e) => eprintln!("Warning: Failed to load disambiguation rules: {}", e),
                    }
                }
            }

            // Load AvsAnRule with data files
            let det_a_path = lt_root.join("en/src/main/resources/org/languagetool/rules/en/det_a.txt");
            let det_an_path = lt_root.join("en/src/main/resources/org/languagetool/rules/en/det_an.txt");
            if det_a_path.exists() && det_an_path.exists() {
                if let (Ok(det_a), Ok(det_an)) = (
                    std::fs::read_to_string(&det_a_path),
                    std::fs::read_to_string(&det_an_path),
                ) {
                    let mut avsan = crate::en::AvsAnRule::new();
                    avsan.load_data(&det_a, &det_an);
                    engine.add_rule(Arc::new(avsan));
                }
            }

            // Add SimpleReplaceRule with data
            engine.add_rule(Arc::new(crate::en::SimpleReplaceRule::english_default()));

            // Load spellchecker with English word lists
            let mut dict = Dictionary::new();
            let hunspell_dir = lt_root.join("en/src/main/resources/org/languagetool/resource/en/hunspell");
            let resource_dir = lt_root.join("en/src/main/resources/org/languagetool/resource/en");

            // Load common words
            if let Ok(words) = std::fs::read_to_string(resource_dir.join("common_words.txt")) {
                for line in words.lines() {
                    let word = line.trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        dict.add_word(word);
                    }
                }
            }

            // Load spelling dictionary
            if let Ok(words) = std::fs::read_to_string(hunspell_dir.join("spelling.txt")) {
                for line in words.lines() {
                    let word = line.trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        dict.add_word(word);
                    }
                }
            }

            // Load spelling_merged (merged dictionary)
            if let Ok(words) = std::fs::read_to_string(hunspell_dir.join("spelling_merged.txt")) {
                for line in words.lines() {
                    let word = line.trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        dict.add_word(word);
                    }
                }
            }

            // Load ignore words (words the spellchecker should skip)
            let mut ignore_words = HashSet::new();
            if let Ok(words) = std::fs::read_to_string(hunspell_dir.join("ignore.txt")) {
                for line in words.lines() {
                    let word = line.trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        ignore_words.insert(word.to_lowercase());
                    }
                }
            }

            if dict.len() > 0 {
                let spell_rule = SpellingCheckRule::new(dict)
                    .with_ignore_words(ignore_words);
                eprintln!("Loaded spellchecker with {} words", spell_rule.dict_size());
                engine.add_rule(Arc::new(spell_rule));
            }
        }

        engine.rebuild_checker();
        engine
    }

    /// Load grammar rules from an XML file
    pub fn load_xml_rules(&mut self, xml: &str) -> Result<usize, String> {
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml)
            .map_err(|e| format!("XML compilation error: {}", e))?;

        let count = rule_set.rules.len();
        for compiled_rule in rule_set.rules {
            let rule = Arc::new(XmlPatternRule::new(compiled_rule));
            self.rules.push(rule);
        }

        // Rebuild checker with updated rules
        self.rebuild_checker();
        Ok(count)
    }

    /// Load grammar rules from a file path
    pub fn load_xml_file(&mut self, path: &Path) -> Result<usize, String> {
        let xml = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        self.load_xml_rules(&xml)
    }

    /// Add a native Rust rule
    pub fn add_rule(&mut self, rule: Arc<dyn Rule>) {
        self.rules.push(rule);
        self.rebuild_checker();
    }

    /// Add a text-level rule
    pub fn add_text_level_rule(&mut self, rule: Arc<dyn TextLevelRule>) {
        self.text_level_rules.push(rule);
        self.rebuild_checker();
    }

    fn rebuild_checker(&mut self) {
        let mut checker = Checker::new()
            .with_sentence_tokenizer(Arc::new(SentenceSplitterBridge))
            .with_word_tokenizer(Arc::new(WordTokenizerBridge))
            .set_rules(self.rules.clone())
            .set_text_level_rules(self.text_level_rules.clone());

        if let Some(ref tagger) = self.tagger {
            checker = checker.with_tagger(tagger.clone());
        }

        if let Some(ref disambiguator) = self.disambiguator {
            checker = checker.with_disambiguator(disambiguator.clone());
        }

        self.checker = checker;
    }

    /// Check text and return matches
    pub fn check(&self, text: &str) -> CheckResult {
        let request = CheckRequest::new(text, self.language.clone());
        self.checker.check(&request)
    }

    /// Check text with custom request options
    pub fn check_with_request(&self, request: &CheckRequest) -> CheckResult {
        self.checker.check(request)
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    pub fn language(&self) -> &Language {
        &self.language
    }
}

/// Bridge from og-tokenizer to og-core's SentenceTokenizer trait
struct SentenceSplitterBridge;

impl SentenceTokenizer for SentenceSplitterBridge {
    fn split(&self, text: &str) -> Vec<SentenceRange> {
        let splitter = og_tokenizer::DefaultSentenceSplitter::new();
        use og_tokenizer::SentenceTokenizer as _;
        let sentences = splitter.split(text);
        sentences.into_iter().map(|s| SentenceRange {
            start: s.start(),
            end: s.end(),
        }).collect()
    }
}

/// Bridge from og-tokenizer to og-core's WordTokenizer trait
struct WordTokenizerBridge;

impl WordTokenizer for WordTokenizerBridge {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        let tokenizer = og_tokenizer::DefaultWordTokenizer::new();
        use og_tokenizer::WordTokenizer as _;
        let tokens = tokenizer.tokenize(text);
        tokens.into_iter().map(|t| {
            let at = AnalyzedToken::new(t.text(), t.start() + offset, t.end() + offset);
            AnalyzedTokenReadings::new(at)
        }).collect()
    }
}

/// Find the LanguageTool resource directory for a given language
pub fn find_lt_resource_dir() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("../languagetool-language-modules"),
        PathBuf::from("../../languagetool-language-modules"),
        PathBuf::from("/home/agent/languagetool/languagetool-language-modules"),
    ];
    for candidate in &candidates {
        if candidate.exists() {
            return Some(candidate.canonicalize().ok()?);
        }
    }
    None
}

/// Get the grammar.xml path for a language
pub fn grammar_xml_path(lang_code: &str, lt_root: &Path) -> PathBuf {
    // Map language code to module directory name
    let module_name = lang_code.split('-').next().unwrap_or(lang_code);
    lt_root.join(module_name)
        .join("src/main/resources/org/languagetool/rules")
        .join(module_name)
        .join("grammar.xml")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(result.matches.len() >= 2, "Expected at least 2 matches, got {}", result.matches.len());
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
        let lt_root = match find_lt_resource_dir() {
            Some(p) => p,
            None => {
                eprintln!("Skipping: LT resource dir not found");
                return;
            }
        };

        let grammar_path = grammar_xml_path("en", &lt_root);
        if !grammar_path.exists() {
            eprintln!("Skipping: English grammar.xml not found at {:?}", grammar_path);
            return;
        }

        let mut engine = LanguageEngine::new(Language::English);
        match engine.load_xml_file(&grammar_path) {
            Ok(count) => {
                println!("Loaded {} rules from English grammar.xml", count);
                assert!(count > 0, "Expected at least some rules");

                // Test with a simple sentence
                let result = engine.check("This is a test.");
                assert_eq!(result.language.code, "en-US");
            }
            Err(e) => {
                panic!("Failed to load English grammar.xml: {}", e);
            }
        }
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
        assert!(result.matches.len() >= 1);
    }
}
