use og_core::rule::{Rule, TextLevelRule};
use og_core::{Category, IssueType};
use og_tagger::ChainedDisambiguator;
use std::path::Path;
use std::sync::Arc;

pub mod replace;
pub mod tagger;
pub mod word_tokenizer;

#[cfg(test)]
mod example_suite;

pub use replace::{cast_words_rule, simple_replace_rule};
pub use tagger::GalicianTagger;
pub use word_tokenizer::GalicianWordTokenizer;

pub fn build_gl_disambiguator(resource_dir: &Path) -> Option<ChainedDisambiguator> {
    let multiwords = og_tagger::MultiWordChunkerDisambiguator::from_params(
        &resource_dir.join("multiwords.txt"),
        og_tagger::ChunkerParams {
            allow_first_capitalized: false,
            allow_all_uppercase: false,
            allow_titlecase: false,
            default_tag: None,
            remove_previous_tags: false,
            add_ignore_spelling: false,
        },
    );
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).ok()?;
    let expanded =
        og_xml::parser::XmlRuleParser::expand_entities_with_base(&xml, Some(resource_dir));
    let xml = og_tagger::XmlDisambiguator::from_xml(&expanded).ok()?;
    let mut stages: Vec<Arc<dyn og_core::checker::Disambiguator>> = Vec::new();
    if let Some(chunker) = multiwords {
        stages.push(Arc::new(chunker));
    }
    stages.push(Arc::new(xml));
    Some(ChainedDisambiguator::new(stages))
}

pub fn galician_unpaired_brackets_rule() -> og_rules::text_level_rules::GenericUnpairedBracketsRule
{
    og_rules::text_level_rules::GenericUnpairedBracketsRule::new().with_symbols(
        vec![
            "[", "(", "{", "\u{201C}", "\u{00AB}", "\u{00BB}", "\u{2018}", "\"", "'",
        ],
        vec![
            "]", ")", "}", "\u{201D}", "\u{00BB}", "\u{00AB}", "\u{2019}", "\"", "'",
        ],
    )
}

pub fn galician_paragraph_repeat_beginning_rule()
-> crate::paragraph_repeat_beginning::ParagraphRepeatBeginningRule {
    crate::paragraph_repeat_beginning::ParagraphRepeatBeginningRule::new(
        "O mesmo comezo do parágrafo",
        "O mesmo comezo como derradeiro parágrafo",
        "Estilo",
    )
}

pub fn galician_redundancy_rule() -> crate::pt::simple_replace2::SimpleReplace2Rule {
    let mut rule = crate::pt::simple_replace2::SimpleReplace2Rule::new(
        "GL_REDUNDANCY_REPLACE",
        "1. Pleonasmos e redundancias",
        "Pleonasmo",
        "'$match' é un pleonasmo. É preferible dicir $suggestions",
    )
    .with_separator(" ou ")
    .with_category(Category::new("REDUNDANCY", "Redundancia"))
    .with_issue_type(IssueType::Style)
    .with_url("https://gl.wikipedia.org/wiki/Pleonasmo");
    rule.load_data(include_str!(
        "../../../../languagetool/languagetool-language-modules/gl/src/main/resources/org/languagetool/rules/gl/redundancies.txt"
    ));
    rule
}

pub fn galician_wordiness_rule() -> crate::pt::simple_replace2::SimpleReplace2Rule {
    let mut rule = crate::pt::simple_replace2::SimpleReplace2Rule::new(
        "GL_WORDINESS_REPLACE",
        "2. Expresións prolixas",
        "Expresión prolixa",
        "'$match' é unha expresión innecesariamente complexa. É preferíbel dicir $suggestions",
    )
    .with_separator(" ou ")
    .with_category(Category::new("REDUNDANCY", "Redundancia"))
    .with_issue_type(IssueType::Style);
    rule.load_data(include_str!(
        "../../../../languagetool/languagetool-language-modules/gl/src/main/resources/org/languagetool/rules/gl/wordiness.txt"
    ));
    rule
}

pub fn galician_barbarisms_rule() -> crate::pt::simple_replace2::SimpleReplace2Rule {
    let mut rule = crate::pt::simple_replace2::SimpleReplace2Rule::new(
        "GL_BARBARISM_REPLACE",
        "Palabras de orixe estranxeira evitábeis",
        "Xenismo",
        "'$match' é un xenismo. É preferíbel dicir $suggestions",
    )
    .with_separator(" ou ")
    .with_category(Category::new("STYLE", "Estilo"))
    .with_issue_type(IssueType::Other("localeViolation".to_string()))
    .with_url("https://gl.wikipedia.org/wiki/Xenismo");
    rule.load_data(include_str!(
        "../../../../languagetool/languagetool-language-modules/gl/src/main/resources/org/languagetool/rules/gl/barbarisms.txt"
    ));
    rule
}

pub fn galician_wikipedia_rule() -> crate::pt::simple_replace2::SimpleReplace2Rule {
    let mut rule = crate::pt::simple_replace2::SimpleReplace2Rule::new(
        "GL_WIKIPEDIA_COMMON_ERRORS",
        "Erros frecuentes nos artigos da Wikipedia",
        "Erro gramatical ou de normativa",
        "'$match' é un erro. Considere utilizar $suggestions",
    )
    .with_separator(" ou ")
    .with_category(Category::new(
        "WIKIPEDIA",
        "Regras específicas da Wikipedia",
    ))
    .with_issue_type(IssueType::Grammar)
    .with_url(
        "https://gl.wikipedia.org/wiki/Wikipedia:Erros_de_ortograf%C3%ADa_e_desviaci%C3%B3ns",
    );
    rule.load_data(include_str!(
        "../../../../languagetool/languagetool-language-modules/gl/src/main/resources/org/languagetool/rules/gl/wikipedia.txt"
    ));
    rule
}

pub struct GalicianNativeRules {
    pub rules: Vec<Arc<dyn Rule>>,
    pub text_level_rules: Vec<Arc<dyn TextLevelRule>>,
}

pub fn galician_native_rules() -> GalicianNativeRules {
    GalicianNativeRules {
        rules: vec![
            Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            Arc::new(og_rules::text_level_rules::WhiteSpaceAtBeginOfParagraphRule::new()),
            Arc::new(simple_replace_rule()),
            Arc::new(cast_words_rule()),
            Arc::new(galician_redundancy_rule()),
            Arc::new(galician_wordiness_rule()),
            Arc::new(galician_barbarisms_rule()),
            Arc::new(galician_wikipedia_rule()),
        ],
        text_level_rules: vec![
            Arc::new(og_rules::text_level_rules::LongSentenceRule::new(50)),
            Arc::new(
                og_rules::text_level_rules::LongParagraphRule::new(220)
                    .with_description("Lexibilidade: parágrafo de máis de 220 palabras")
                    .with_message(
                        "O parágrafo sobrepasa as 220 palabras de longo. Considere unha revisión.",
                    ),
            ),
            Arc::new(og_rules::text_level_rules::WhiteSpaceBeforeParagraphEndRule::new()),
            Arc::new(og_rules::text_level_rules::EmptyLineRule::new()),
            Arc::new(galician_paragraph_repeat_beginning_rule()),
            Arc::new(
                og_rules::text_level_rules::PunctuationMarkAtParagraphEnd2Rule::new()
                    .with_message("Engada un signo de puntuación ao final do parágrafo."),
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natives_use_java_ids_in_relevant_rules_order() {
        let natives = galician_native_rules();
        let ids: Vec<&str> = natives.rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            ids,
            vec![
                "UPPERCASE_SENTENCE_START",
                "WHITESPACE_PARAGRAPH_BEGIN",
                "GL_SIMPLE_REPLACE",
                "GL_CAST_WORDS",
                "GL_REDUNDANCY_REPLACE",
                "GL_WORDINESS_REPLACE",
                "GL_BARBARISM_REPLACE",
                "GL_WIKIPEDIA_COMMON_ERRORS",
            ],
            "the module owns the gl-specific half of Galician.java:99-127 plus the shared \
             natives the generic engine path does not install; CommaWhitespace, \
             DoublePunctuation, UNPAIRED_BRACKETS, the HUNSPELL_RULE speller, \
             MultipleWhitespace and SentenceWhitespace stay with the shared engine path"
        );
        let text_ids: Vec<&str> = natives.text_level_rules.iter().map(|r| r.id()).collect();
        assert_eq!(
            text_ids,
            vec![
                "TOO_LONG_SENTENCE",
                "TOO_LONG_PARAGRAPH",
                "WHITESPACE_PARAGRAPH",
                "EMPTY_LINE",
                "PARAGRAPH_REPEAT_BEGINNING_RULE",
                "PUNCTUATION_PARAGRAPH_END2",
            ],
            "the shared PunctuationMarkAtParagraphEnd2Rule keeps its PUNCTUATION_PARAGRAPH_END2 \
             id; Java's PunctuationMarkAtParagraphEnd carries PUNCTUATION_PARAGRAPH_END"
        );
    }

    #[test]
    fn unpaired_brackets_use_the_galician_symbol_pairs() {
        let rule = galician_unpaired_brackets_rule();
        assert_eq!(rule.id(), "UNPAIRED_BRACKETS");
    }

    #[test]
    fn paragraph_repeat_carries_the_gl_bundle_strings() {
        let rule = galician_paragraph_repeat_beginning_rule();
        assert_eq!(rule.id(), "PARAGRAPH_REPEAT_BEGINNING_RULE");
        assert!(!rule.is_default_on());
        assert_eq!(rule.description(), "O mesmo comezo do parágrafo");
    }
}
