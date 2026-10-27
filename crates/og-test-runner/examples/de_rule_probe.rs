use og_test_runner::runner::TestRunner;
use og_xml::compiler::{CompiledPatternElement, XmlCompiler};

fn tok_digest(t: &og_xml::compiler::CompiledPatternToken) -> String {
    let mut s = String::new();
    if let Some(text) = &t.text {
        s.push_str(&format!("{text:?}"));
        if t.is_regexp {
            s.insert(0, '~');
        }
    }
    if let Some(pos) = &t.postag {
        s.push_str(&format!(
            "@{}{}",
            pos,
            if t.compiled_postag_regexp.is_some() || t.compiled_postag_regexp_fancy.is_some() {
                "~"
            } else {
                ""
            }
        ));
    }
    if t.negate {
        s.insert(0, '!');
    }
    if t.negate_pos {
        s.push_str("!pos");
    }
    if t.case_sensitive {
        s.push_str("/cs");
    }
    if t.inflected {
        s.push_str("/infl");
    }
    if let Some(min) = t.min {
        s.push_str(&format!("/min{min}"));
    }
    if let Some(max) = t.max {
        s.push_str(&format!("/max{}", max));
    }
    if t.skip != 0 {
        s.push_str(&format!("/skip{}", t.skip));
    }
    if let Some(space) = &t.space_before {
        s.push_str(&format!("/sp={space}"));
    }
    if let Some(chunk) = &t.chunk {
        s.push_str(&format!("/chunk{chunk}"));
    }
    if !t.exceptions.is_empty() {
        s.push_str(&format!("/exc{}", t.exceptions.len()));
    }
    s
}

fn elem_digest(e: &CompiledPatternElement) -> String {
    match e {
        CompiledPatternElement::Token(t) => tok_digest(t),
        CompiledPatternElement::OrGroup(g) => format!(
            "OR[{}]",
            g.iter().map(tok_digest).collect::<Vec<_>>().join(",")
        ),
        CompiledPatternElement::AndGroup(g) => format!(
            "AND[{}]",
            g.iter().map(tok_digest).collect::<Vec<_>>().join(",")
        ),
    }
}

fn main() {
    let rule_id = std::env::args().nth(1).expect("rule id");
    let text = std::env::args().nth(2).expect("text");
    let grammar_path = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/rules/de/grammar.xml";
    let xml = std::fs::read_to_string(grammar_path).expect("grammar.xml");
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, std::path::Path::new(grammar_path).parent())
        .expect("compile");
    let runner = TestRunner::for_language("de");
    for (n, rule) in rule_set
        .rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.id == rule_id)
    {
        let pattern_digest = rule
            .pattern
            .elements
            .iter()
            .map(elem_digest)
            .collect::<Vec<_>>()
            .join(" ");
        println!("=== [#{n}] sub={:?} {}", rule.sub_id, pattern_digest);
        let matches = runner.match_rule_on_text(rule, &text).2;
        println!("  matches: {}", matches.len());
        for m in &matches {
            println!("    {} len {} {:?}", m.offset(), m.length(), m.message());
        }
    }
}
