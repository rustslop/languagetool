use std::collections::HashMap;
use std::sync::OnceLock;

use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};

enum SuggestionMode {
    JoinOnly,
    HyphenOnly,
    Both,
}

struct CompoundEntry {
    parts: Vec<String>,
    hyphenated: String,
    joined: String,
    mode: SuggestionMode,
}

/// Lazily-built lookup structure over [`CompoundRule::entries`].
///
/// All lowered parts live in one contiguous `lower_arena`, delimited by
/// `part_spans` (global part index -> byte span); `part_starts` slices those
/// spans per entry. `by_first_part` maps the lowercased first part to the
/// `(head, tail)` of a chain of entry indices sharing it, threaded through
/// `next_same_first` in original entry order, so the match loop can jump
/// straight to the only entries that can possibly match at a token position
/// and still see them in the order a linear scan would. Everything is a
/// handful of flat allocations: no per-part boxes, no per-key bucket vectors.
struct CompoundIndex {
    lower_arena: String,
    /// Byte spans into `lower_arena` for every part of every entry, in entry
    /// order (`part_starts[e]..part_starts[e+1]` belong to entry `e`).
    part_spans: Vec<(u32, u32)>,
    /// First `part_spans` index per entry, plus a trailing sentinel.
    part_starts: Vec<u32>,
    by_first_part: HashMap<Box<str>, (u32, u32)>,
    /// Intrusive next link per entry within its `by_first_part` chain;
    /// `u32::MAX` terminates a chain.
    next_same_first: Vec<u32>,
}

impl CompoundIndex {
    fn part_count(&self, entry: u32) -> usize {
        (self.part_starts[entry as usize + 1] - self.part_starts[entry as usize]) as usize
    }

    fn part(&self, entry: u32, j: u32) -> &str {
        let (start, end) = self.part_spans[self.part_starts[entry as usize] as usize + j as usize];
        &self.lower_arena[start as usize..end as usize]
    }
}

/// Appends the lowercase form of `s` to `buf`, allocating only when the
/// std-identical fallback runs.
///
/// U+03A3 (GREEK CAPITAL SIGMA, UTF-8 `CE A3`) is the only character whose
/// lowercase mapping depends on surrounding context (SpecialCasing.txt
/// `Final_Sigma`), which `str::to_lowercase` applies — e.g. `"ΣΟΦΟΣ"` lowers
/// to `"σοφος"` with a final ς — and char-wise lowering cannot reproduce it.
/// Any string that could contain it falls back to `str::to_lowercase`
/// itself, so the result is byte-identical to std for every input; all other
/// input takes the allocation-free char-wise map.
/// `test_push_lowercase_matches_std` pins this equivalence.
fn push_lowercase_into(buf: &mut String, s: &str) {
    if s.as_bytes().contains(&0xCE) {
        buf.push_str(&s.to_lowercase());
        return;
    }
    for c in s.chars() {
        for lc in c.to_lowercase() {
            buf.push(lc);
        }
    }
}

pub struct CompoundRule {
    id: String,
    description: String,
    with_hyphen_msg: String,
    without_hyphen_msg: String,
    either_msg: String,
    url: String,
    entries: Vec<CompoundEntry>,
    max_parts: usize,
    /// Built on the first `match_sentence` call, never at construction, so
    /// engine assembly pays nothing for it.
    index: OnceLock<CompoundIndex>,
}

impl CompoundRule {
    pub fn new() -> Self {
        Self::for_language(
            "EN_COMPOUNDS",
            "Hyphenated words",
            "This word is normally spelled with a hyphen.",
            "This word is normally spelled as one.",
            "This word is normally spelled as one or with a hyphen.",
            include_str!("../../../../languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/compounds.txt"),
        )
        .with_url("https://languagetool.org/insights/post/hyphen/")
    }

    pub fn for_language(
        id: &str,
        description: &str,
        with_hyphen_msg: &str,
        without_hyphen_msg: &str,
        either_msg: &str,
        data: &str,
    ) -> Self {
        let mut rule = Self {
            id: id.to_string(),
            description: description.to_string(),
            with_hyphen_msg: with_hyphen_msg.to_string(),
            without_hyphen_msg: without_hyphen_msg.to_string(),
            either_msg: either_msg.to_string(),
            url: String::new(),
            entries: Vec::new(),
            max_parts: 0,
            index: OnceLock::new(),
        };
        rule.load_data(data);
        rule
    }

    fn with_url(mut self, url: &str) -> Self {
        self.url = url.to_string();
        self
    }

    fn load_data(&mut self, data: &str) {
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let (word_part, mode) = if let Some(w) = line.strip_suffix('+') {
                (w, SuggestionMode::JoinOnly)
            } else if let Some(w) = line.strip_suffix('*') {
                (w, SuggestionMode::HyphenOnly)
            } else if let Some(w) = line.strip_suffix('?') {
                (w, SuggestionMode::JoinOnly)
            } else if let Some(w) = line.strip_suffix('$') {
                (w, SuggestionMode::Both)
            } else {
                (line, SuggestionMode::Both)
            };

            let parts: Vec<String> = word_part.split('-').map(String::from).collect();
            if parts.len() < 2 {
                continue;
            }

            let hyphenated = word_part.to_string();
            let joined = parts.join("");

            let part_count = parts.len();
            if part_count > self.max_parts {
                self.max_parts = part_count;
            }

            self.entries.push(CompoundEntry {
                parts,
                hyphenated,
                joined,
                mode,
            });
        }
    }
}

impl Default for CompoundRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for CompoundRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("COMPOUNDING", "Compounding")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        let n = tokens.len();
        let text = sentence.text();

        let index = self.index();

        // Lowercase each token exactly once into a flat arena; every lookup
        // and part comparison below is a borrow into this buffer. The scan
        // oracle below lowercased both sides of every one of the ~8.9k entry
        // comparisons instead.
        let mut lower_buf = String::with_capacity(text.len());
        let mut lower_spans = Vec::with_capacity(n);
        for token in tokens.iter() {
            let start = lower_buf.len();
            push_lowercase_into(&mut lower_buf, token.token().token());
            lower_spans.push((start, lower_buf.len()));
        }

        for i in 0..n {
            let (first_start, first_end) = lower_spans[i];
            // An entry can only fully match at `i` if its first part equals
            // `tokens[i]` case-insensitively, so the chain (which keeps
            // original entry order) contains every candidate in the same
            // order the linear scan would have considered them.
            let Some(&(mut entry_idx, _)) =
                index.by_first_part.get(&lower_buf[first_start..first_end])
            else {
                continue;
            };

            loop {
                let part_count = index.part_count(entry_idx);
                if i + part_count <= n {
                    let mut matched = true;
                    for j in 1..part_count {
                        let (start, end) = lower_spans[i + j];
                        if &lower_buf[start..end] != index.part(entry_idx, j as u32) {
                            matched = false;
                            break;
                        }
                    }
                    if matched {
                        matches.push(self.build_match(&self.entries[entry_idx as usize], &tokens, i, text));
                        break;
                    }
                }

                match index.next_same_first[entry_idx as usize] {
                    u32::MAX => break,
                    next => entry_idx = next,
                }
            }
        }

        matches
    }
}

impl CompoundRule {
    fn index(&self) -> &CompoundIndex {
        self.index.get_or_init(|| {
            let mut lower_arena = String::new();
            let mut part_spans: Vec<(u32, u32)> = Vec::new();
            let mut part_starts: Vec<u32> = Vec::with_capacity(self.entries.len() + 1);
            let mut by_first_part: HashMap<Box<str>, (u32, u32)> =
                HashMap::with_capacity(self.entries.len());
            let mut next_same_first: Vec<u32> = vec![u32::MAX; self.entries.len()];

            for (idx, entry) in self.entries.iter().enumerate() {
                part_starts.push(part_spans.len() as u32);
                for part in &entry.parts {
                    let start = lower_arena.len() as u32;
                    push_lowercase_into(&mut lower_arena, part);
                    part_spans.push((start, lower_arena.len() as u32));
                }

                // Append `idx` to its first-part chain, keeping chains in
                // entry order.
                let first = part_spans[part_starts[idx] as usize];
                let key = &lower_arena[first.0 as usize..first.1 as usize];
                match by_first_part.get_mut(key) {
                    Some(&mut (_, ref mut tail)) => {
                        next_same_first[*tail as usize] = idx as u32;
                        *tail = idx as u32;
                    }
                    None => {
                        by_first_part.insert(key.to_owned().into_boxed_str(), (idx as u32, idx as u32));
                    }
                }
            }
            part_starts.push(part_spans.len() as u32);

            CompoundIndex {
                lower_arena,
                part_spans,
                part_starts,
                by_first_part,
                next_same_first,
            }
        })
    }

    /// Shared match emission for the indexed path and the scan oracle, so
    /// both construct identical [`RuleMatch`]es from a hit.
    fn build_match(
        &self,
        entry: &CompoundEntry,
        tokens: &[&AnalyzedTokenReadings],
        i: usize,
        text: &str,
    ) -> RuleMatch {
        let part_count = entry.parts.len();
        let match_start = tokens[i].token().start();
        let match_end = tokens[i + part_count - 1].token().end();
        let match_length = match_end - match_start;

        let matched_text = &text[match_start..match_end];

        let suggestions: Vec<SuggestedReplacement> = match &entry.mode {
            SuggestionMode::JoinOnly => {
                vec![SuggestedReplacement::new(preserve_case_multi(
                    matched_text,
                    &entry.joined,
                ))]
            }
            SuggestionMode::HyphenOnly => {
                vec![SuggestedReplacement::new(preserve_case_multi(
                    matched_text,
                    &entry.hyphenated,
                ))]
            }
            SuggestionMode::Both => {
                let joined = preserve_case_multi(matched_text, &entry.joined);
                let hyphenated = preserve_case_multi(matched_text, &entry.hyphenated);
                let mut s = vec![SuggestedReplacement::new(&hyphenated)];
                if joined != hyphenated {
                    s.push(SuggestedReplacement::new(&joined));
                }
                s
            }
        };

        let msg = match &entry.mode {
            SuggestionMode::JoinOnly => &self.without_hyphen_msg,
            SuggestionMode::HyphenOnly => &self.with_hyphen_msg,
            SuggestionMode::Both => &self.either_msg,
        };

        let mut rm_rule = RuleMatchRule::new(self.id(), self.description())
            .with_category(self.category())
            .with_issue_type(self.issue_type().as_str());
        if !self.url.is_empty() {
            rm_rule = rm_rule.with_urls(vec![self.url.to_string()]);
        }

        let ctx_start = match_start.saturating_sub(40);
        let ctx_end = std::cmp::min(match_end + 40, text.len());
        let context_text = text[ctx_start..ctx_end].to_string();

        RuleMatch::new(
            msg,
            match_start,
            match_length,
            rm_rule,
            RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
        )
        .with_replacements(suggestions)
        .with_sentence(text.to_string())
    }

    /// The original linear-scan algorithm, kept as the permanent correctness
    /// oracle for the indexed [`Rule::match_sentence`] path: identical
    /// per-entry `to_lowercase` comparisons over all entries in order.
    #[cfg_attr(not(test), allow(dead_code))]
    fn match_sentence_scan(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut matches = Vec::new();
        let n = tokens.len();
        let text = sentence.text();

        for i in 0..n {
            for entry in &self.entries {
                let part_count = entry.parts.len();
                if i + part_count > n {
                    continue;
                }

                let mut matched = true;
                for (j, part) in entry.parts.iter().enumerate() {
                    let token_text = tokens[i + j].token().token();
                    if token_text.to_lowercase() != part.to_lowercase() {
                        matched = false;
                        break;
                    }
                }

                if !matched {
                    continue;
                }

                matches.push(self.build_match(entry, &tokens, i, text));
                break;
            }
        }

        matches
    }
}

fn preserve_case_multi(original: &str, replacement: &str) -> String {
    let is_all_upper = original
        .chars()
        .all(|c| !c.is_alphabetic() || c.is_uppercase());
    let is_title = original
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
        && original
            .chars()
            .skip(1)
            .all(|c| !c.is_alphabetic() || c.is_lowercase());

    if is_all_upper {
        replacement.to_uppercase()
    } else if is_title {
        let mut r = replacement.chars();
        match r.next() {
            Some(first) => format!("{}{}", first.to_uppercase(), r.as_str()),
            None => replacement.to_string(),
        }
    } else {
        replacement.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut byte_pos = 0;
        for part in text.split(|c: char| c.is_whitespace()) {
            if part.is_empty() {
                continue;
            }
            if let Some(offset) = text[byte_pos..].find(part) {
                let start = byte_pos + offset;
                let end = start + part.len();
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    part, start, end,
                )));
                byte_pos = end;
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn test_compound_part_time() {
        let rule = CompoundRule::new();
        let sentence = make_sentence("I have a part time job");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Should detect 'part time' as compound");
        let repl: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(
            repl.iter().any(|r| r.contains("part-time")),
            "Should suggest 'part-time', got {:?}",
            repl
        );
    }

    #[test]
    fn test_compound_already_hyphenated() {
        let rule = CompoundRule::new();
        let sentence = make_sentence("I have a part-time job");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "Already hyphenated should not trigger");
    }

    #[test]
    fn test_compound_no_match() {
        let rule = CompoundRule::new();
        let sentence = make_sentence("the quick brown fox");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_push_lowercase_matches_std() {
        // Final-sigma contexts (all contain U+03A3 and so take the std
        // fallback): ς only when preceded by a cased letter with only
        // case-ignorables between, and not followed by cased/ignorable then
        // cased. Expanding mappings: İ (U+0130) lowercases to two chars; ß
        // never expands under to_lowercase.
        let cases = [
            "",
            "hello",
            "PART-TIME",
            "Part Time",
            "İstanbul",
            "ß",
            "ΣΟΦΟΣ",
            "Σ",
            "ΑΣ",
            "ΑΣΑ",
            "A.Σ",
            "ΑΣ.",
            "a'Σ",
            "ΑΣ 1",
            "ǅaΣ",
            "äöü",
            "ABC İİ ẞ",
            "ǅungla",
            "~-42#",
            "ΣΟΦΟΣ part time Σ",
        ];
        for case in cases {
            let mut buf = String::new();
            push_lowercase_into(&mut buf, case);
            assert_eq!(buf, case.to_lowercase(), "mismatch for {case:?}");
        }

        // Sweep every scalar value below plane 14 so the allocation-free
        // char-wise path is verified against std char by char.
        for cp in 0..0x30000u32 {
            let Some(c) = char::from_u32(cp) else { continue };
            let s = c.to_string();
            let mut buf = String::new();
            push_lowercase_into(&mut buf, &s);
            assert_eq!(buf, s.to_lowercase(), "mismatch for U+{cp:04X}");
        }
    }

    #[test]
    fn test_index_is_lazy() {
        let rule = CompoundRule::new();
        assert!(
            rule.index.get().is_none(),
            "engine assembly must not pay the index build"
        );
        let _ = rule.match_sentence(&make_sentence("hello world"));
        assert!(rule.index.get().is_some());
    }

    #[test]
    fn test_index_covers_every_entry_once() {
        let rule = CompoundRule::new();
        let _ = rule.match_sentence(&make_sentence("part time"));
        let index = rule.index.get().expect("index built after first match");

        // Walk every chain: together they must cover each entry exactly
        // once, in strictly increasing (original entry) order, and each
        // entry's chain key must be the lowered first part of that entry.
        let mut seen: Vec<u32> = Vec::with_capacity(rule.entries.len());
        for (key, &(head, tail)) in &index.by_first_part {
            let mut chain = Vec::new();
            let mut cur = head;
            loop {
                chain.push(cur);
                match index.next_same_first[cur as usize] {
                    u32::MAX => break,
                    next => cur = next,
                }
            }
            assert_eq!(*chain.last().unwrap(), tail, "chain for {key:?} must end at tail");
            assert!(
                chain.windows(2).all(|w| w[0] < w[1]),
                "chain out of order for {key:?}: {chain:?}"
            );
            for &e in &chain {
                assert_eq!(index.part(e, 0), key.as_ref());
            }
            seen.extend(chain);
        }
        seen.sort_unstable();
        let expected: Vec<u32> = (0..rule.entries.len() as u32).collect();
        assert_eq!(seen, expected, "every entry indexed exactly once");

        // The arena holds each part's exact lowercase form.
        assert_eq!(index.part_starts.len(), rule.entries.len() + 1);
        for (e, entry) in rule.entries.iter().enumerate() {
            assert_eq!(index.part_count(e as u32), entry.parts.len());
            for (j, part) in entry.parts.iter().enumerate() {
                assert_eq!(index.part(e as u32, j as u32), part.to_lowercase());
            }
        }
    }

    /// Manual timing evidence, not a CI gate: run with
    /// `--ignored --nocapture`. Uses the og-bench corpora verbatim so the
    /// compound rule's share of each metric is measurable without building
    /// the engine.
    #[test]
    #[ignore = "timing evidence only: cargo test -- --ignored --nocapture"]
    fn timing_indexed_vs_scan_on_bench_corpora() {
        use std::hint::black_box;
        use std::time::Instant;

        let clean = "The children walked to school together this morning. \
                     Their teacher was waiting for them at the gate.";
        let err = "Their is a probleme with this sentense. \
                   Teh guick brown fox jumpd over the lazy dogg. \
                   I has tree cats and they runs fast.";
        let multi = "Last summer we drove to the coast for a long weekend. \
                     The weather was warm and the sea was calm. \
                     Their was a small restaurant near the harbor that served fresh fish. \
                     We ordered the soup, and it tasted wonderfull. \
                     My brother wanted to go swimming, but there towels were still in the car. \
                     In the afternoon we walked along the beach and colected shells. \
                     The children built a sandcastle with a tall tower. \
                     Later, a sudden storm forced us to run back to the hotel. \
                     I think there going to remember that trip for years. \
                     The next morning the sky was clear again. \
                     We packed our bags and drove home before lunch. \
                     It was the best holiday we have had in a long time.";

        let rule = CompoundRule::new();
        for _ in 0..2 {
            black_box(rule.match_sentence(&make_sentence("warmup")));
        }
        let idx = rule.index.get().expect("index built");
        eprintln!(
            "compound-index entries={} unique_first_parts={} parts={} arena_bytes={}",
            rule.entries.len(),
            idx.by_first_part.len(),
            idx.part_spans.len(),
            idx.lower_arena.len()
        );
        for (name, text) in [("clean", clean), ("err", err), ("multi", multi)] {
            let sentences: Vec<AnalyzedSentence> = text
                .split_terminator(". ")
                .filter(|s| !s.is_empty())
                .map(make_sentence)
                .collect();
            let iters = 10;

            for _ in 0..2 {
                for s in &sentences {
                    black_box(rule.match_sentence(s));
                }
            }
            let start = Instant::now();
            for _ in 0..iters {
                for s in &sentences {
                    black_box(rule.match_sentence(s));
                }
            }
            let indexed_ms = start.elapsed().as_secs_f64() * 1000.0 / iters as f64;

            for s in &sentences {
                black_box(rule.match_sentence_scan(s));
            }
            let start = Instant::now();
            for _ in 0..iters {
                for s in &sentences {
                    black_box(rule.match_sentence_scan(s));
                }
            }
            let scan_ms = start.elapsed().as_secs_f64() * 1000.0 / iters as f64;

            eprintln!(
                "compound-timing corpus={name} sentences={} indexed={indexed_ms:.3}ms scan={scan_ms:.3}ms ratio={:.1}x",
                sentences.len(),
                scan_ms / indexed_ms.max(f64::EPSILON),
            );
        }
    }

    /// Manual end-to-end evidence, not a CI gate: run with
    /// `--ignored --nocapture`. Times `engine.check` on the og-bench corpora
    /// with and without EN_COMPOUNDS (interleaved, medians), so the compound
    /// rule's share of the clean/err metrics is visible in the real pipeline.
    #[test]
    #[ignore = "timing evidence only: cargo test -- --ignored --nocapture"]
    fn timing_engine_check_compound_share() {
        use crate::engine::LanguageEngine;
        use og_core::{CheckRequest, Language};
        use std::time::Instant;

        let _release_guard = crate::release_guard();
        let engine = LanguageEngine::shared_for_code("en").expect("shared English engine");

        let clean = "The children walked to school together this morning. \
                     Their teacher was waiting for them at the gate.";
        let err = "Their is a probleme with this sentense. \
                   Teh guick brown fox jumpd over the lazy dogg. \
                   I has tree cats and they runs fast.";

        let iters = 15;
        // Proof the bench engine actually runs EN_COMPOUNDS: a triggering
        // text must produce its match through the full pipeline.
        {
            let trigger = engine.check("I have a part time job.");
            let compound_hits: Vec<&og_core::RuleMatch> = trigger
                .matches
                .iter()
                .filter(|m| m.rule().id() == "EN_COMPOUNDS")
                .collect();
            eprintln!(
                "compound-share trigger matches={} compound_matches={}",
                trigger.matches.len(),
                compound_hits.len()
            );
            assert!(
                compound_hits
                    .iter()
                    .any(|m| m.replacements().iter().any(|r| r.value().contains("part-time"))),
                "EN_COMPOUNDS must fire through the engine, got {:?}",
                trigger.matches
            );
        }

        for (name, text) in [("clean", clean), ("err", err)] {
            let mut full = Vec::with_capacity(iters);
            let mut diffs = Vec::with_capacity(iters);
            for run in 0..(2 + iters) {
                let mut disabled = CheckRequest::new(text, Language::English);
                disabled.disabled_rules = vec!["EN_COMPOUNDS".to_string()];
                let plain = CheckRequest::new(text, Language::English);

                // Interleave the variants so machine drift hits both alike;
                // the per-pair difference cancels drift for the share too.
                let t = Instant::now();
                let with = engine.check_with_request(&plain);
                let with_ms = t.elapsed().as_secs_f64() * 1000.0;
                let t = Instant::now();
                let without = engine.check_with_request(&disabled);
                let without_ms = t.elapsed().as_secs_f64() * 1000.0;
                if run == 0 {
                    let compound_hits = with
                        .matches
                        .iter()
                        .filter(|m| m.rule().id() == "EN_COMPOUNDS")
                        .count();
                    eprintln!(
                        "compound-share corpus={name} matches={} compound_matches={compound_hits}",
                        with.matches.len()
                    );
                    assert_eq!(
                        format!("{:?}", with.matches),
                        format!("{:?}", without.matches),
                        "disabling EN_COMPOUNDS must change nothing on clean/err corpora"
                    );
                    continue;
                }
                full.push(with_ms);
                diffs.push(with_ms - without_ms);
            }
            full.sort_by(f64::total_cmp);
            diffs.sort_by(f64::total_cmp);
            let med = |v: &[f64]| v[v.len() / 2];
            eprintln!(
                "compound-share corpus={name} full_median={:.3}ms share_median_of_pair_diffs={:.3}ms",
                med(&full),
                med(&diffs),
            );
        }
    }

    #[test]
    fn test_indexed_matches_scan_oracle() {
        let rule = CompoundRule::new();
        // Real entries from compounds.txt: part-time* (HyphenOnly, 2 parts),
        // run-of-the-mill* / cock-of-the-rock* / up-to-the-minute* (4 parts),
        // time-table? (JoinOnly), plus case variants, boundary truncations,
        // duplicate-first-part buckets ("part-", "time-"), multiple hits in
        // one sentence, and non-ASCII lookups that must not match.
        let corpus = [
            "I have a part time job",
            "A Part Time job",
            "a PART TIME job",
            "I have a part-time job",
            "the quick brown fox jumps over the lazy dog",
            "it was a run of the mill solution",
            "a RUN OF THE MILL solution",
            "a Run Of The Mill solution",
            "the cock of the rock sat there",
            "up to the minute news arrived",
            "check the time table now",
            "the time tables are old",
            "we only ran of the",
            "at the very end: part",
            "the part time job and the run of the mill story",
            "part part part time",
            "time time table",
            "párt time job",
            "a PART TİME job",
            "a part​ time job",
            "",
            "   ",
            "word",
        ];
        for text in corpus {
            let sentence = make_sentence(text);
            let indexed = rule.match_sentence(&sentence);
            let scanned = rule.match_sentence_scan(&sentence);
            assert_eq!(
                format!("{indexed:?}"),
                format!("{scanned:?}"),
                "indexed path diverged from scan oracle for {text:?}"
            );
        }
    }
}
