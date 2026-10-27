use og_test_runner::runner::TestRunner;
use og_xml::compiler::{
    CompiledException, CompiledPattern, CompiledPatternElement, CompiledPatternToken, CompiledRule,
    XmlCompiler,
};
use og_xml::types::XmlExampleType;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Bucket {
    BaselineFlipped,
    ChunkDecisive,
    PostagDecisiveZeroReading,
    PostagDecisiveDictWord,
    SurfaceDecisive,
    Unmapped,
}

impl Bucket {
    fn as_str(self) -> &'static str {
        match self {
            Bucket::BaselineFlipped => "baseline_flipped",
            Bucket::ChunkDecisive => "chunk_decisive",
            Bucket::PostagDecisiveZeroReading => "postag_decisive_zero_reading",
            Bucket::PostagDecisiveDictWord => "postag_decisive_dict_word",
            Bucket::SurfaceDecisive => "surface_decisive",
            Bucket::Unmapped => "unmapped",
        }
    }
}

fn strip_token_chunks(tok: &mut CompiledPatternToken) {
    tok.chunk = None;
    tok.chunk_re = None;
}

fn strip_pattern_chunks(pat: &mut CompiledPattern) {
    for tok in pat.tokens.iter_mut() {
        strip_token_chunks(tok);
    }
    for elem in pat.elements.iter_mut() {
        match elem {
            CompiledPatternElement::Token(t) => strip_token_chunks(t),
            CompiledPatternElement::OrGroup(ts) | CompiledPatternElement::AndGroup(ts) => {
                for t in ts.iter_mut() {
                    strip_token_chunks(t);
                }
            }
        }
    }
}

fn strip_token_postags(tok: &mut CompiledPatternToken) {
    tok.postag = None;
    tok.compiled_postag_regexp = None;
    tok.compiled_postag_regexp_fancy = None;
    tok.negate_pos = false;
    for e in tok.exceptions.iter_mut() {
        strip_exception_postags(e);
    }
}

fn strip_exception_postags(exc: &mut CompiledException) {
    exc.postag = None;
    exc.compiled_postag_regexp = None;
    exc.compiled_postag_regexp_fancy = None;
    exc.negate_pos = false;
}

fn strip_pattern_postags(pat: &mut CompiledPattern) {
    for tok in pat.tokens.iter_mut() {
        strip_token_postags(tok);
    }
    for elem in pat.elements.iter_mut() {
        match elem {
            CompiledPatternElement::Token(t) => strip_token_postags(t),
            CompiledPatternElement::OrGroup(ts) | CompiledPatternElement::AndGroup(ts) => {
                for t in ts.iter_mut() {
                    strip_token_postags(t);
                }
            }
        }
    }
}

fn strip_rule_chunks(rule: &CompiledRule) -> CompiledRule {
    let mut r = rule.clone();
    strip_pattern_chunks(&mut r.pattern);
    for ap in r.antipatterns.iter_mut() {
        strip_pattern_chunks(ap);
    }
    r
}

fn strip_rule_postags(rule: &CompiledRule) -> CompiledRule {
    let mut r = rule.clone();
    strip_pattern_postags(&mut r.pattern);
    for ap in r.antipatterns.iter_mut() {
        strip_pattern_postags(ap);
    }
    r
}

fn extract_marker(text: &str) -> (String, Option<(usize, usize)>) {
    const START: &str = "<marker>";
    const END: &str = "</marker>";
    match (text.find(START), text.find(END)) {
        (Some(s), Some(e)) => {
            let clean = text.replace(START, "").replace(END, "");
            (clean, Some((s, e - START.len())))
        }
        _ => (text.to_string(), None),
    }
}

fn example_passes(
    runner: &TestRunner,
    rule: &CompiledRule,
    example: &og_xml::types::XmlExample,
) -> bool {
    let (_clean, marker) = extract_marker(&example.text);
    let matches = runner.match_rule_on_text(rule, &example.text).2;
    match example.example_type {
        XmlExampleType::Correct => matches.is_empty(),
        XmlExampleType::Incorrect => {
            if matches.is_empty() {
                false
            } else if let Some((m_start, m_end)) = marker {
                let first = &matches[0];
                let ms = first.offset();
                let me = ms + first.length();
                ms <= m_start && me >= m_end
            } else {
                true
            }
        }
        XmlExampleType::TriggersError => true,
    }
}

fn zero_reading_tokens(runner: &TestRunner, example: &og_xml::types::XmlExample) -> usize {
    let (clean, _) = extract_marker(&example.text);
    let sentence = runner.debug_sentence(&clean);
    sentence
        .tokens()
        .iter()
        .filter(|t| !t.is_whitespace() && t.readings().is_empty())
        .count()
}

fn parse_line(line: &str) -> Option<(String, Option<String>)> {
    let rest = line.strip_prefix("FAIL: [")?;
    let rule_id = rest.split(']').next()?.to_string();
    let text = rest
        .split(": '")
        .nth(1)
        .map(|t| t.strip_suffix('\'').unwrap_or(t).to_string());
    Some((rule_id, text))
}

fn main() {
    let code = std::env::args().nth(1).unwrap_or_else(|| "en".to_string());
    let artifact_path = std::env::args()
        .nth(2)
        .unwrap_or_else(|| format!("/tmp/test_failures_{code}.txt"));
    let grammar_path = format!(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/{code}/src/main/resources/org/languagetool/rules/{code}/grammar.xml"
    );

    let xml = std::fs::read_to_string(&grammar_path)
        .unwrap_or_else(|e| panic!("grammar {grammar_path}: {e}"));
    let base = std::path::Path::new(&grammar_path)
        .parent()
        .map(std::path::Path::to_path_buf);
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, base.as_deref())
        .unwrap_or_else(|e| panic!("compile: {e}"));
    let runner = TestRunner::for_language(&code);
    og_rules::pattern_rule::set_test_date_hack(true);

    let artifact =
        std::fs::read_to_string(&artifact_path).unwrap_or_else(|e| panic!("artifact: {e}"));

    let mut counts: std::collections::HashMap<Bucket, usize> = std::collections::HashMap::new();
    let mut by_rule: std::collections::HashMap<String, std::collections::HashMap<Bucket, usize>> =
        std::collections::HashMap::new();
    let mut lines_out: Vec<String> = Vec::new();

    for line in artifact.lines() {
        let Some((rule_id, text)) = parse_line(line) else {
            continue;
        };
        let candidate_rules: Vec<&CompiledRule> =
            rule_set.rules.iter().filter(|r| r.id == rule_id).collect();
        let mut pairs: Vec<(&CompiledRule, &og_xml::types::XmlExample)> = Vec::new();
        for r in &candidate_rules {
            for e in r.examples.iter() {
                let clean = e.text.replace("<marker>", "").replace("</marker>", "");
                match &text {
                    Some(t) => {
                        if clean == *t || clean == t.replace('\u{a0}', " ") || e.text == *t {
                            pairs.push((r, e));
                        }
                    }
                    None => pairs.push((r, e)),
                }
            }
        }
        if pairs.is_empty() {
            *counts.entry(Bucket::Unmapped).or_default() += 1;
            lines_out.push(format!("{} \t[{}]", line, Bucket::Unmapped.as_str()));
            continue;
        }
        let Some(&(rule, example)) = pairs.iter().find(|(r, e)| {
            !example_passes(&runner, r, e) || e.example_type == XmlExampleType::TriggersError
        }) else {
            *counts.entry(Bucket::BaselineFlipped).or_default() += 1;
            lines_out.push(format!("{} \t[{}]", line, Bucket::BaselineFlipped.as_str()));
            continue;
        };

        let bucket = if example_passes(&runner, &strip_rule_chunks(rule), example) {
            Bucket::ChunkDecisive
        } else if example_passes(&runner, &strip_rule_postags(rule), example) {
            let zero = zero_reading_tokens(&runner, example);
            if zero > 0 {
                Bucket::PostagDecisiveZeroReading
            } else {
                Bucket::PostagDecisiveDictWord
            }
        } else {
            Bucket::SurfaceDecisive
        };

        *counts.entry(bucket).or_default() += 1;
        *by_rule
            .entry(rule_id)
            .or_default()
            .entry(bucket)
            .or_default() += 1;
        lines_out.push(format!("{} \t[{}]", line, bucket.as_str()));
    }

    let mut report = String::new();
    report.push_str(&format!(
        "residue split for {code} (artifact {artifact_path}, grammar {grammar_path})\n"
    ));
    report.push_str(
        "buckets: chunk_decisive = outcome flips when chunk/chunk_re constraints are neutralized\n",
    );
    report.push_str(
        "          postag_decisive_* = outcome flips only when POS constraints are neutralized\n",
    );
    report.push_str("          postag_decisive_zero_reading = ... and the example has a token with no readings at all\n");
    report.push_str("          surface_decisive = outcome independent of POS/chunk evidence\n");
    report.push_str("          baseline_flipped = artifact line no longer reproduces\n");
    report.push('\n');
    for b in [
        Bucket::ChunkDecisive,
        Bucket::PostagDecisiveZeroReading,
        Bucket::PostagDecisiveDictWord,
        Bucket::SurfaceDecisive,
        Bucket::BaselineFlipped,
        Bucket::Unmapped,
    ] {
        report.push_str(&format!(
            "{:<32} {}\n",
            b.as_str(),
            counts.get(&b).copied().unwrap_or(0)
        ));
    }
    let total: usize = counts.values().sum();
    report.push_str(&format!("{:<32} {}\n", "TOTAL", total));
    report.push_str("\nby rule (top):\n");
    let mut rule_rows: Vec<_> = by_rule
        .iter()
        .map(|(id, bc)| {
            let n: usize = bc.values().sum();
            (id, bc, n)
        })
        .collect();
    rule_rows.sort_by_key(|(_, _, n)| std::cmp::Reverse(*n));
    for (id, bc, n) in rule_rows.iter().take(40) {
        let parts: Vec<String> = bc
            .iter()
            .map(|(b, c)| format!("{}={}", b.as_str(), c))
            .collect();
        report.push_str(&format!("  {id} ({n}): {}\n", parts.join(" ")));
    }
    report.push_str("\nper-line:\n");
    report.push_str(&lines_out.join("\n"));
    report.push('\n');

    let out_path = format!("/tmp/residue_split_{code}.txt");
    std::fs::write(&out_path, report).unwrap_or_else(|e| panic!("write {out_path}: {e}"));
    println!("wrote {out_path}");
    for b in [
        Bucket::ChunkDecisive,
        Bucket::PostagDecisiveZeroReading,
        Bucket::PostagDecisiveDictWord,
        Bucket::SurfaceDecisive,
        Bucket::BaselineFlipped,
        Bucket::Unmapped,
    ] {
        println!(
            "{:<32} {}",
            b.as_str(),
            counts.get(&b).copied().unwrap_or(0)
        );
    }
}
