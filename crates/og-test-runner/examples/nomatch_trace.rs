use std::collections::BTreeMap;

use og_test_runner::runner::TestRunner;
use og_test_runner::runner::test_support;

const NO_MATCH_PREFIX: &str = "Expected match on incorrect example but got none: '";

fn parse_failures(path: &str) -> Vec<(String, String)> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    content
        .lines()
        .filter_map(|line| {
            if !line.starts_with("FAIL: [") {
                return None;
            }
            let id_end = line.find(']')?;
            let rule_id = line[7..id_end].to_string();
            let pos = line.find(NO_MATCH_PREFIX)? + NO_MATCH_PREFIX.len();
            let text = line[pos..].strip_suffix('\'')?;
            Some((rule_id, text.to_string()))
        })
        .collect()
}

fn main() {
    let code: &'static str = Box::leak(
        std::env::args()
            .nth(1)
            .expect("usage: nomatch_trace <lang>")
            .into_boxed_str(),
    );
    let path = format!("/tmp/test_failures_{code}.txt");
    let failures = parse_failures(&path);
    if failures.is_empty() {
        eprintln!("no no-match failures parsed from {path}");
        return;
    }

    let runner = TestRunner::for_language(code);
    let rules_handle = std::thread::spawn(move || test_support::generic_grammar_rules(code));
    let rules = rules_handle.join().expect("grammar compile thread");

    let mut cause_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut untraced = 0usize;
    let mut matched_anyway = 0usize;
    let mut deepest_by_rule: BTreeMap<String, String> = BTreeMap::new();

    og_rules::pattern_trace::set_enabled(true);
    for (rule_id, text) in &failures {
        let sub_rules: Vec<_> = rules.rules.iter().filter(|r| &r.id == rule_id).collect();
        if sub_rules.is_empty() {
            untraced += 1;
            deepest_by_rule
                .entry(rule_id.clone())
                .or_insert_with(|| "RULE-ID-NOT-FOUND".to_string());
            continue;
        }
        let mut reported = false;
        for rule in sub_rules {
            og_rules::pattern_trace::clear();
            let (_, _, matches) = runner.match_rule_on_text(rule, text);
            if !matches.is_empty() {
                matched_anyway += 1;
                continue;
            }
            let events = og_rules::pattern_trace::take();
            if events.is_empty() {
                continue;
            }
            let max_pat = events.iter().map(|e| e.pattern_idx).max().unwrap_or(0);
            let deepest = events
                .iter()
                .filter(|e| e.pattern_idx == max_pat)
                .min_by_key(|e| e.token_idx)
                .expect("events non-empty");
            *cause_counts.entry(deepest.cause.clone()).or_insert(0) += 1;
            deepest_by_rule.entry(rule_id.clone()).or_insert_with(|| {
                format!(
                    "elem#{} tok={}: {}",
                    deepest.pattern_idx, deepest.token, deepest.cause
                )
            });
            reported = true;
        }
        if !reported {
            untraced += 1;
        }
    }
    og_rules::pattern_trace::set_enabled(false);

    println!(
        "=== {code}: {} no-match failures traced ===",
        failures.len()
    );
    println!("matched-anyway: {matched_anyway}, untraced: {untraced}");
    println!("--- deepest-failure causes (aggregate) ---");
    let mut causes: Vec<_> = cause_counts.into_iter().collect();
    causes.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (cause, n) in causes.iter().take(50) {
        println!("{n:5}  {cause}");
    }
    println!("--- per-rule deepest failure (first 80 rules) ---");
    for (rule_id, cause) in deepest_by_rule.iter().take(80) {
        println!("[{rule_id}] {cause}");
    }
}
