use std::path::PathBuf;
use std::sync::Arc;

use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::CompiledRule;
use og_xml::compiler::CompiledRuleSet;
use og_xml::compiler::XmlCompiler;

pub(crate) fn grammar_path() -> Option<PathBuf> {
    crate::fr::test_support::fr_rules_dir().map(|dir| dir.join("grammar.xml"))
}

pub(crate) fn grammar_available() -> bool {
    grammar_path().is_some_and(|path| path.exists())
}

pub(crate) fn read_grammar() -> String {
    let path = grammar_path().expect("fr grammar.xml path");
    std::fs::read_to_string(&path).expect("fr grammar.xml")
}

pub(crate) fn compile_grammar(xml: &str) -> CompiledRuleSet {
    let base = crate::fr::test_support::fr_rules_dir();
    let normalized = crate::fr::normalize_fr_rules_xml(xml);
    XmlCompiler::new()
        .compile_file_with_base(&normalized, base.as_deref())
        .expect("compile fr grammar.xml")
}

pub(crate) fn compiled_rules() -> Arc<CompiledRuleSet> {
    static GRAMMAR: og_core::releasable::Releasable<CompiledRuleSet> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&GRAMMAR, || {
        Some(Arc::new(compile_grammar(&read_grammar())))
    })
    .expect("fr refresh grammar")
}

pub(crate) fn pattern_engine() -> Arc<PatternRuleEngine> {
    static ENGINE: og_core::releasable::Releasable<PatternRuleEngine> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&ENGINE, || {
        Some(Arc::new(crate::fr::example_suite::build_pattern_engine()))
    })
    .expect("fr refresh pattern engine")
}

pub(crate) fn find_rule<'a>(
    rule_set: &'a CompiledRuleSet,
    id: &str,
    sub_id: &str,
) -> &'a CompiledRule {
    rule_set
        .rules
        .iter()
        .find(|rule| rule.id == id && rule.sub_id.as_deref() == Some(sub_id))
        .unwrap_or_else(|| panic!("{id}[{sub_id}] in grammar.xml"))
}

pub(crate) fn rule_match_spans(rule: &CompiledRule, text: &str) -> Vec<(usize, usize, String)> {
    let engine = pattern_engine();
    let pipeline = crate::fr::test_support::FrenchPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    let mut spans = Vec::new();
    for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
        for rule_match in engine.match_rule(rule, &sentence) {
            let start = rule_match.offset() + sentence_start;
            spans.push((
                start,
                start + rule_match.length(),
                rule_match
                    .replacements()
                    .first()
                    .map(|replacement| replacement.value().to_string())
                    .unwrap_or_default(),
            ));
        }
    }
    spans
}

pub(crate) fn rule_is_untagged_at(text: &str, surface: &str) -> bool {
    let pipeline = crate::fr::test_support::FrenchPipeline::shared();
    pipeline
        .analyze_text(text)
        .iter()
        .flat_map(|sentence| sentence.tokens().iter())
        .any(|token| token.token().token() == surface && og_xml::unifier::has_no_tag_reading(token))
}
