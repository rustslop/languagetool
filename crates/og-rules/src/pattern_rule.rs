use og_core::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use og_xml::compiler::CompiledRule;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

fn java_replacement(repl: &str) -> Cow<'_, str> {
    if !repl.contains('$') {
        return Cow::Borrowed(repl);
    }
    let mut out = String::with_capacity(repl.len() + 4);
    let mut chars = repl.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let mut digits = String::new();
        while let Some(&d) = chars.peek() {
            if d.is_ascii_digit() {
                digits.push(d);
                chars.next();
            } else {
                break;
            }
        }
        if digits.is_empty() {
            out.push('$');
            continue;
        }
        match chars.peek() {
            Some(&n) if n.is_alphanumeric() || n == '_' => {
                out.push_str("${");
                out.push_str(&digits);
                out.push('}');
            }
            _ => {
                out.push('$');
                out.push_str(&digits);
            }
        }
    }
    Cow::Owned(out)
}

fn whole_match_regex(pattern: &str, is_regexp: bool) -> Option<og_xml::compiler::SharedRegex> {
    let body = if is_regexp {
        pattern.to_string()
    } else {
        regex::escape(pattern)
    };
    og_xml::compiler::shared_cached_regex(&format!("^(?:{body})$"))
}

fn seg_has_lower_dot_lower(seg: &str) -> bool {
    let b = seg.as_bytes();
    for i in 1..b.len().saturating_sub(1) {
        if b[i] == b'.' && b[i - 1].is_ascii_lowercase() && b[i + 1].is_ascii_lowercase() {
            return true;
        }
    }
    false
}

fn pl_pos_tag_correction(tag: &str) -> String {
    if !tag.contains('.') {
        return tag.to_string();
    }
    let mut hit = false;
    let out: Vec<String> = tag
        .split(':')
        .map(|seg| {
            if seg_has_lower_dot_lower(seg) {
                hit = true;
                format!("(.*{}.*)", seg.replace('.', ".*|.*"))
            } else {
                seg.to_string()
            }
        })
        .collect();
    if hit { out.join(":") } else { tag.to_string() }
}

fn apply_case_conversion(text: &str, conversion: &str) -> String {
    match conversion {
        "startlower" => {
            let mut chars = text.chars();
            match chars.next() {
                Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
        "startupper" => {
            let mut chars = text.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
        "alllower" => text.to_lowercase(),
        "allupper" => text.to_uppercase(),
        _ => text.to_string(),
    }
}

pub struct PatternRuleEngine {
    word_index: HashMap<String, Vec<usize>>,
    catch_all_rule_indices: Vec<usize>,
    spelling: Option<std::sync::Arc<crate::spelling_support::SpellingSupport>>,
    synthesizer: Option<std::sync::Arc<crate::postponed_adjective::LangSynthesizer>>,
    suggestion_synth: Option<std::sync::Arc<crate::postponed_adjective::LangSynthesizer>>,
}

impl PatternRuleEngine {
    pub fn new() -> Self {
        Self {
            word_index: HashMap::new(),
            catch_all_rule_indices: Vec::new(),
            spelling: None,
            synthesizer: None,
            suggestion_synth: None,
        }
    }

    pub fn with_spelling_support(
        mut self,
        spelling: std::sync::Arc<crate::spelling_support::SpellingSupport>,
    ) -> Self {
        self.spelling = Some(spelling);
        self
    }

    pub fn with_synthesizer(
        mut self,
        synthesizer: std::sync::Arc<crate::postponed_adjective::LangSynthesizer>,
    ) -> Self {
        self.synthesizer = Some(synthesizer);
        self
    }

    pub fn with_suggestion_synthesizer(
        mut self,
        synthesizer: std::sync::Arc<crate::postponed_adjective::LangSynthesizer>,
    ) -> Self {
        self.suggestion_synth = Some(synthesizer);
        self
    }

    pub fn build_index(&mut self, rules: &[&CompiledRule]) {
        self.word_index.clear();
        self.catch_all_rule_indices.clear();

        for (rule_idx, rule) in rules.iter().enumerate() {
            if rule.pattern.tokens.is_empty() {
                continue;
            }
            let first_token = &rule.pattern.tokens[0];
            if let Some(ref text) = first_token.text
                && !first_token.negate
                && first_token.compiled_regexp.is_none()
            {
                let key = text.to_lowercase();
                self.word_index.entry(key).or_default().push(rule_idx);
                continue;
            }
            self.catch_all_rule_indices.push(rule_idx);
        }
    }

    pub fn match_indexed_rules(
        &self,
        rules: &[&CompiledRule],
        sentence: &AnalyzedSentence,
    ) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();

        let mut candidate = vec![false; rules.len()];
        for token in &tokens {
            let token_lower = token.token().token().to_lowercase();
            if let Some(rule_indices) = self.word_index.get(&token_lower) {
                for &rule_idx in rule_indices {
                    candidate[rule_idx] = true;
                }
            }
        }
        for &rule_idx in &self.catch_all_rule_indices {
            candidate[rule_idx] = true;
        }

        let mut all_matches = Vec::new();
        for (rule_idx, is_candidate) in candidate.iter().enumerate() {
            if !is_candidate {
                continue;
            }
            let rule = rules[rule_idx];
            let index_key = rule
                .pattern
                .tokens
                .first()
                .filter(|t| !t.negate && t.compiled_regexp.is_none())
                .and_then(|t| t.text.as_ref())
                .map(|text| text.to_lowercase());
            let mut matches = Vec::new();
            for token_idx in 0..tokens.len() {
                if let Some(ref key) = index_key
                    && tokens[token_idx].token().token().to_lowercase() != *key
                {
                    continue;
                }
                if let Some(rm) = self.try_match_at(rule, &tokens, token_idx, sentence) {
                    matches.push(rm);
                }
            }
            all_matches.extend(self.check_for_antipatterns(rule, &tokens, sentence, matches));
        }

        all_matches
    }

    pub fn match_indexed_rules_parallel(
        &self,
        rules: &[&CompiledRule],
        sentences: &[AnalyzedSentence],
    ) -> Vec<RuleMatch> {
        use rayon::prelude::*;

        sentences
            .par_iter()
            .flat_map(|sentence| self.match_indexed_rules(rules, sentence))
            .collect()
    }
}

impl PatternRuleEngine {
    pub fn match_rule(&self, rule: &CompiledRule, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        if let Some(ref regexp) = rule.regexp {
            return self.match_regexp_rule(rule, regexp, sentence);
        }
        let tokens = if rule.pattern.raw_pos {
            sentence.pre_disambiguation_non_whitespace_tokens()
        } else {
            sentence.non_whitespace_tokens()
        };
        let matches = self.collect_rule_matches(rule, &tokens, sentence);
        self.check_for_antipatterns(rule, &tokens, sentence, matches)
    }

    fn check_for_antipatterns(
        &self,
        rule: &CompiledRule,
        tokens: &[&AnalyzedTokenReadings],
        sentence: &AnalyzedSentence,
        matches: Vec<RuleMatch>,
    ) -> Vec<RuleMatch> {
        if matches.is_empty() || rule.antipatterns.is_empty() {
            return matches;
        }
        let spans = self.antipattern_immunize_spans(rule, tokens);
        if spans.is_empty() {
            return matches;
        }
        let mut immunized: Vec<AnalyzedTokenReadings> =
            tokens.iter().map(|t| (*t).clone()).collect();
        for &(first, last) in &spans {
            let last = last.min(immunized.len().saturating_sub(1));
            for token in &mut immunized[first..=last] {
                token.immunize();
            }
        }
        let refs: Vec<&AnalyzedTokenReadings> = immunized.iter().collect();
        self.collect_rule_matches(rule, &refs, sentence)
    }

    fn antipattern_immunize_spans(
        &self,
        rule: &CompiledRule,
        tokens: &[&AnalyzedTokenReadings],
    ) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        for antipattern in rule.antipatterns.iter() {
            if antipattern.tokens.is_empty() && antipattern.elements.is_empty() {
                continue;
            }
            let has_structured = antipattern.has_structured_elements;
            let first_literal = self.antipattern_first_literal(antipattern);
            for pos in 0..tokens.len() {
                if let Some((lit, cs)) = &first_literal {
                    let text = normalize_apostrophes(tokens[pos].token().token());
                    let hit = if *cs {
                        text == lit.as_str()
                    } else {
                        eq_ignore_case(&text, lit)
                    };
                    if !hit {
                        continue;
                    }
                }
                let result = if has_structured {
                    self.match_pattern_elements(&antipattern.elements, tokens, pos)
                } else {
                    self.match_pattern_tokens(&antipattern.tokens, tokens, pos)
                };
                if let Some(anti_result) = result
                    && antipattern_unify_satisfied(
                        &rule.unifications,
                        antipattern,
                        &anti_result.matched_positions,
                        &anti_result.pattern_indices,
                        &anti_result.matched_texts,
                        tokens,
                    )
                {
                    let whole_first = anti_result
                        .matched_positions
                        .first()
                        .copied()
                        .unwrap_or(pos);
                    let whole_last = anti_result.matched_positions.last().copied().unwrap_or(pos);
                    let marker_span = (|| {
                        let (ms, me) =
                            (antipattern.marker_elem_start?, antipattern.marker_elem_end?);
                        let first = anti_result.position_for_pattern(ms)?;
                        let me_pat = if me > 0 { me - 1 } else { 0 };
                        let last = anti_result.last_position_for_pattern(me_pat)?;
                        Some((first, last))
                    })();
                    let (first, last) = marker_span.unwrap_or((whole_first, whole_last));
                    spans.push((first, last));
                }
            }
        }
        spans
    }

    fn collect_rule_matches(
        &self,
        rule: &CompiledRule,
        tokens: &[&AnalyzedTokenReadings],
        sentence: &AnalyzedSentence,
    ) -> Vec<RuleMatch> {
        let mut matches = Vec::new();

        if rule.pattern.tokens.is_empty() && rule.pattern.elements.is_empty() {
            return matches;
        }

        let has_structured = rule.pattern.has_structured_elements;

        let first_element_is_group = has_structured
            && matches!(
                rule.pattern.elements.first(),
                Some(og_xml::compiler::CompiledPatternElement::OrGroup(_))
                    | Some(og_xml::compiler::CompiledPatternElement::AndGroup(_))
            );

        if first_element_is_group {
            for start_idx in 0..tokens.len() {
                if let Some(rule_match) = self.try_match_at(rule, tokens, start_idx, sentence) {
                    matches.push(rule_match);
                }
            }
            return matches;
        }

        let first_token = &rule.pattern.tokens[0];

        let start_positions: Vec<usize> = if let Some(ref text) = first_token.text {
            if !first_token.negate
                && first_token.compiled_regexp.is_none()
                && first_token.min != Some(0)
            {
                tokens
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| {
                        let surface_matches = if first_token.case_sensitive {
                            t.token().token() == text
                        } else {
                            eq_ignore_case(t.token().token(), text)
                        };
                        if surface_matches {
                            true
                        } else if first_token.inflected {
                            t.readings().iter().any(|r| {
                                if let Some(lemma) = r.lemma() {
                                    if first_token.case_sensitive {
                                        lemma == text
                                    } else {
                                        eq_ignore_case(lemma, text)
                                    }
                                } else {
                                    false
                                }
                            })
                        } else {
                            false
                        }
                    })
                    .map(|(i, _)| i)
                    .collect()
            } else {
                (0..tokens.len()).collect()
            }
        } else {
            (0..tokens.len()).collect()
        };

        for start_idx in start_positions {
            if let Some(rule_match) = self.try_match_at(rule, tokens, start_idx, sentence) {
                matches.push(rule_match);
            }
        }

        matches
    }

    pub fn match_rules(
        &self,
        rules: &[&CompiledRule],
        sentence: &AnalyzedSentence,
    ) -> Vec<RuleMatch> {
        let mut engine = Self::new();
        engine.build_index(rules);
        engine.match_indexed_rules(rules, sentence)
    }

    fn match_regexp_rule(
        &self,
        rule: &CompiledRule,
        regexp: &og_xml::compiler::CompiledRuleRegexp,
        sentence: &AnalyzedSentence,
    ) -> Vec<RuleMatch> {
        const MAX_SENT_LENGTH: usize = 2000;
        let text = sentence.text();
        let base = sentence.start();
        let mut matches = Vec::new();
        if text.len() > MAX_SENT_LENGTH {
            return matches;
        }

        let captured: Vec<Vec<Option<(usize, usize)>>> = if let Some(ref re) = regexp.regex {
            re.captures_iter(text)
                .map(|caps| {
                    (0..caps.len())
                        .map(|i| caps.get(i).map(|m| (m.start(), m.end())))
                        .collect()
                })
                .collect()
        } else if let Some(ref re) = regexp.fancy_regex {
            re.captures_iter(text)
                .flatten()
                .map(|caps| {
                    (0..caps.len())
                        .map(|i| caps.get(i).map(|m| (m.start(), m.end())))
                        .collect()
                })
                .collect()
        } else {
            return matches;
        };

        for groups in captured {
            let Some((match_start, match_end)) = groups
                .get(regexp.mark)
                .and_then(|g| *g)
                .or_else(|| groups.first().and_then(|g| *g))
            else {
                continue;
            };
            if match_end < match_start {
                continue;
            }

            if let Some(ref filter) = rule.filter
                && filter.class.ends_with("RegexAntiPatternFilter")
                && !regex_antipattern_allows(filter.args.as_str(), text, match_start, match_end)
            {
                continue;
            }

            let group_texts: Vec<&str> = groups
                .iter()
                .map(|g| g.map(|(s, e)| &text[s..e]).unwrap_or(""))
                .collect();

            let message = resolve_regexp_message(&rule.message, &group_texts);

            let replacements: Vec<SuggestedReplacement> = rule
                .suggestions
                .iter()
                .map(|s| {
                    if !s.parts.is_empty() {
                        let mut resolved = String::new();
                        for part in &s.parts {
                            match part {
                                og_xml::types::SuggestionPart::Text(t) => {
                                    resolved.push_str(&resolve_regexp_message(t, &group_texts));
                                }
                                og_xml::types::SuggestionPart::Match(m) => {
                                    let idx = m.no.unsigned_abs() as usize;
                                    let mut value =
                                        group_texts.get(idx).copied().unwrap_or("").to_string();
                                    if let Some(ref rm) = m.regexp_match
                                        && let Some(re) = og_xml::compiler::shared_cached_regex(rm)
                                    {
                                        let replace = m.regexp_replace.as_deref().unwrap_or("");
                                        value = re
                                            .replace_all(&value, java_replacement(replace).as_ref())
                                            .into_owned();
                                    }
                                    if let Some(ref cc) = m.case_conversion {
                                        value = apply_case_conversion(&value, cc);
                                    }
                                    resolved.push_str(&value);
                                }
                            }
                        }
                        SuggestedReplacement::new(&resolved)
                    } else {
                        SuggestedReplacement::new(resolve_regexp_message(&s.text, &group_texts))
                    }
                })
                .collect();

            let match_length = match_end - match_start;
            let context_start = {
                let mut pos = match_start.saturating_sub(40);
                while !text.is_char_boundary(pos) && pos < text.len() {
                    pos += 1;
                }
                pos
            };
            let context_end = {
                let mut pos = std::cmp::min(match_end + 40, text.len());
                while !text.is_char_boundary(pos) && pos > 0 {
                    pos -= 1;
                }
                pos
            };
            let context_text = text[context_start..context_end].to_string();

            let rm_rule = RuleMatchRule::new(&rule.id, &rule.name)
                .with_category(og_core::Category::new(
                    &rule.category.id,
                    &rule.category.name,
                ))
                .with_issue_type(&rule.issue_type);
            let rm_rule = if let Some(ref sub_id) = rule.sub_id {
                rm_rule.with_sub_id(sub_id)
            } else {
                rm_rule
            };
            let rm_rule = if let Some(ref url) = rule.url {
                rm_rule.with_urls(vec![url.clone()])
            } else {
                rm_rule
            };
            let rm_rule = if let Some(ref source_file) = rule.source_file {
                rm_rule.with_source_file(source_file)
            } else {
                rm_rule
            };
            let match_kind = rm_rule.pattern_match_kind();

            let mut rm = RuleMatch::new(
                &message,
                base + match_start,
                match_length,
                rm_rule,
                RuleMatchContext::new(
                    context_text,
                    base + context_start,
                    context_end - context_start,
                ),
            )
            .with_replacements(replacements)
            .with_match_type(match_kind)
            .with_sentence(sentence.text().to_string());
            if let Some(ref short) = rule.short_message {
                rm = rm.with_short_message(short);
            }
            if let Some(ref filter) = rule.filter
                && crate::pt_filters::is_pt_filter(&filter.class)
            {
                let tokens = sentence.non_whitespace_tokens();
                let span_end = rm.offset() + rm.length();
                let matched_positions: Vec<usize> = tokens
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.token().start() < span_end && t.token().end() > rm.offset())
                    .map(|(i, _)| i)
                    .collect();
                let matched_texts: Vec<String> = matched_positions
                    .iter()
                    .map(|&i| tokens[i].token().token().to_string())
                    .collect();
                let Some(rm) = crate::pt_filters::apply_engine_pt_filter_with_groups(
                    filter,
                    &group_texts,
                    crate::pt_filters::PtFilterCall {
                        variant: "pt-PT",
                        args: &filter.args,
                        matched_texts: &matched_texts,
                        matched_positions: &matched_positions,
                        tokens: &tokens,
                        regexp_groups: None,
                        synthesizer: self.synthesizer.as_deref(),
                    },
                    rm,
                ) else {
                    continue;
                };
                matches.push(rm);
                continue;
            }
            matches.push(rm);
        }

        matches
    }

    fn try_match_at(
        &self,
        rule: &CompiledRule,
        tokens: &[&AnalyzedTokenReadings],
        start_idx: usize,
        sentence: &AnalyzedSentence,
    ) -> Option<RuleMatch> {
        let pattern_tokens = &rule.pattern.tokens;
        let elements = &rule.pattern.elements;

        let has_structured = rule.pattern.has_structured_elements;

        let match_result = if has_structured {
            self.match_pattern_elements(elements, tokens, start_idx)?
        } else {
            self.match_pattern_tokens(pattern_tokens, tokens, start_idx)?
        };

        if !rule.pattern.unify_groups.is_empty()
            && !unify_groups_satisfied(rule, &match_result, tokens)
        {
            return None;
        }

        let text = sentence.text();
        let base = sentence.start();
        let marker_indices = if rule.pattern.has_structured_elements {
            (rule.pattern.marker_elem_start, rule.pattern.marker_elem_end)
        } else {
            (rule.pattern.marker_start, rule.pattern.marker_end)
        };
        let (match_start, match_end) = if let (Some(ms), Some(me)) = marker_indices {
            let ms_tok_idx = match_result.position_for_pattern(ms)?;
            let me_pat = if me > 0 { me - 1 } else { 0 };
            let me_tok_idx = match_result.last_position_for_pattern(me_pat)?;
            let start_tok = tokens.get(ms_tok_idx)?;
            let end_tok = tokens.get(me_tok_idx)?;
            let mut start = start_tok.token().start() - base;
            if ms_tok_idx >= 1 && comma_leading_suggestion(rule, &match_result.matched_texts) {
                let prev = tokens[ms_tok_idx - 1];
                start = prev.token().start() + prev.token().token().len() - base;
            }
            let end = end_tok.token().end() - base;
            (start, end)
        } else {
            let first_start = match_result.first_start?;
            let mut start = first_start - base;
            let first_idx = tokens
                .iter()
                .position(|t| t.token().start() == first_start)?;
            if first_idx >= 1 && comma_leading_suggestion(rule, &match_result.matched_texts) {
                let prev = tokens[first_idx - 1];
                start = prev.token().start() + prev.token().token().len() - base;
            }
            let end = match_result.last_end? - base;
            (start, end)
        };

        if match_end < match_start {
            return None;
        }

        let match_length = match_end - match_start;

        let context_start = {
            let mut pos = match_start.saturating_sub(40);
            while !text.is_char_boundary(pos) && pos < text.len() {
                pos += 1;
            }
            pos
        };
        let context_end = {
            let mut pos = std::cmp::min(match_end + 40, text.len());
            while !text.is_char_boundary(pos) && pos > 0 {
                pos -= 1;
            }
            pos
        };
        let context_text = text[context_start..context_end].to_string();

        let mut replacements: Vec<SuggestedReplacement> = Vec::new();
        for s in &rule.suggestions {
            let alternatives = self.suggestion_alternatives(s, tokens, &match_result);
            let strippable = rule.suppress_misspelled || s.suppress_misspelled;
            let kept: Vec<String> = if strippable {
                alternatives
                    .into_iter()
                    .filter(|a| !is_suppress_junk(a))
                    .collect()
            } else {
                alternatives
            };
            replacements.extend(
                kept.into_iter()
                    .map(|text| SuggestedReplacement::new(&text)),
            );
        }
        if rule.suppress_misspelled && replacements.is_empty() {
            return None;
        }

        let rm_rule = RuleMatchRule::new(&rule.id, &rule.name)
            .with_category(og_core::Category::new(
                &rule.category.id,
                &rule.category.name,
            ))
            .with_issue_type(&rule.issue_type);
        let rm_rule = if let Some(ref sub_id) = rule.sub_id {
            rm_rule.with_sub_id(sub_id)
        } else {
            rm_rule
        };
        let rm_rule = if let Some(ref url) = rule.url {
            rm_rule.with_urls(vec![url.clone()])
        } else {
            rm_rule
        };
        let rm_rule = if let Some(ref source_file) = rule.source_file {
            rm_rule.with_source_file(source_file)
        } else {
            rm_rule
        };
        let match_kind = rm_rule.pattern_match_kind();

        let mut rm = RuleMatch::new(
            &rule.message,
            base + match_start,
            match_length,
            rm_rule,
            RuleMatchContext::new(
                context_text,
                base + context_start,
                context_end - context_start,
            ),
        )
        .with_replacements(replacements)
        .with_match_type(match_kind)
        .with_sentence(sentence.text().to_string());
        if let Some(ref short) = rule.short_message {
            rm = rm.with_short_message(short);
        }

        if let Some(ref filter) = rule.filter {
            let element_positions: Vec<usize> = (0..rule.pattern.elements.len())
                .filter_map(|elem| match_result.last_position_for_pattern(elem))
                .collect();
            return self.apply_filter(
                FilterContext {
                    rule,
                    matched_texts: &match_result.matched_texts,
                    matched_positions: &match_result.matched_positions,
                    element_positions: &element_positions,
                    tokens,
                    sentence,
                },
                filter,
                rm,
            );
        }

        Some(rm)
    }

    fn match_pattern_tokens(
        &self,
        pattern_tokens: &[og_xml::compiler::CompiledPatternToken],
        tokens: &[&AnalyzedTokenReadings],
        start_idx: usize,
    ) -> Option<PatternMatchResult> {
        self.match_pattern_recursive(pattern_tokens, tokens, start_idx, 0, &[], None)
    }

    fn suggestion_alternatives(
        &self,
        s: &og_xml::compiler::CompiledSuggestion,
        tokens: &[&AnalyzedTokenReadings],
        match_result: &PatternMatchResult,
    ) -> Vec<String> {
        if s.parts.is_empty() {
            return vec![self.resolve_backreferences(&s.text, &match_result.matched_texts)];
        }
        let mut acc: Vec<String> = vec![String::new()];
        for part in &s.parts {
            let alternatives: Vec<String> = match part {
                og_xml::types::SuggestionPart::Text(t) => {
                    vec![self.resolve_backreferences(t, &match_result.matched_texts)]
                }
                og_xml::types::SuggestionPart::Match(m) => {
                    self.match_alternatives(m, tokens, match_result)
                }
            };
            if alternatives.is_empty() {
                return Vec::new();
            }
            let mut next = Vec::with_capacity(acc.len() * alternatives.len());
            for prefix in &acc {
                for alt in &alternatives {
                    next.push(format!("{prefix}{alt}"));
                }
            }
            acc = next;
        }
        acc
    }

    fn match_alternatives(
        &self,
        m: &og_xml::types::XmlMatch,
        tokens: &[&AnalyzedTokenReadings],
        match_result: &PatternMatchResult,
    ) -> Vec<String> {
        let token_idx = (m.no as usize).saturating_sub(1);
        let (Some(pos), true) = (
            match_result.position_for_pattern(token_idx),
            m.postag.is_some() && self.suggestion_synth.is_some(),
        ) else {
            return self.raw_match_alternatives(m, tokens, match_result, token_idx);
        };
        let Some(token) = tokens.get(pos) else {
            return self.raw_match_alternatives(m, tokens, match_result, token_idx);
        };
        let mut forms = self.postag_forms(m, token);
        if let Some(ref mode) = m.include_skipped
            && mode != "none"
        {
            let suffix = skipped_suffix(
                tokens,
                &match_result.matched_positions,
                token_idx,
                mode == "following",
            );
            for form in &mut forms {
                form.push_str(&suffix);
            }
        }
        forms
    }

    fn raw_match_alternatives(
        &self,
        m: &og_xml::types::XmlMatch,
        tokens: &[&AnalyzedTokenReadings],
        match_result: &PatternMatchResult,
        token_idx: usize,
    ) -> Vec<String> {
        let Some(pos) = match_result.position_for_pattern(token_idx) else {
            return vec![String::new()];
        };
        let Some(token) = tokens.get(pos) else {
            return vec![String::new()];
        };
        let mut text = token.token().token().to_string();
        if let Some(ref rm) = m.regexp_match
            && let Ok(re) = regex::Regex::new(rm)
        {
            let replace = m.regexp_replace.as_deref().unwrap_or("");
            text = re
                .replace_all(&text, java_replacement(replace).as_ref())
                .to_string();
        }
        if let Some(ref cc) = m.case_conversion {
            text = apply_case_conversion(&text, cc);
        }
        if let Some(ref mode) = m.include_skipped
            && mode != "none"
        {
            text.push_str(&skipped_suffix(
                tokens,
                &match_result.matched_positions,
                token_idx,
                mode == "following",
            ));
        }
        vec![text]
    }

    fn postag_forms(
        &self,
        m: &og_xml::types::XmlMatch,
        token: &AnalyzedTokenReadings,
    ) -> Vec<String> {
        let Some(synth) = self.suggestion_synth.as_deref() else {
            return Vec::new();
        };
        let token_str = token.token().token();
        let Some(postag) = m.postag.as_deref() else {
            return Vec::new();
        };
        if !m.postag_regexp {
            let mut word_forms: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            if synth.is_determiner_tag(postag) {
                word_forms.extend(synth.determiner_forms(token_str, postag));
            } else {
                for reading in token.readings() {
                    if let Some(lemma) = reading.lemma() {
                        word_forms.extend(synth.synthesize_exact_for(
                            lemma,
                            reading.pos_tags().first().map(String::as_str),
                            postag,
                        ));
                    }
                }
            }
            let mut forms: Vec<String> = if word_forms.is_empty() {
                vec![format!("({token_str})")]
            } else {
                word_forms.into_iter().collect()
            };
            if let Some(ref cc) = m.case_conversion {
                for form in &mut forms {
                    *form = apply_case_conversion(form, cc);
                }
            }
            return forms;
        }
        let (Ok(re), Ok(re_full)) = (
            regex::Regex::new(postag),
            regex::Regex::new(&format!("^(?:{postag})$")),
        ) else {
            return vec![format!("({token_str})")];
        };
        let readings = token.readings();
        let mut word_forms: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut one_form = false;
        for reading in readings {
            if reading.lemma().is_some() {
                continue;
            }
            let tags = reading.pos_tags();
            if tags.is_empty() {
                word_forms.insert(token_str.to_string());
                one_form = true;
            } else if tags
                .iter()
                .any(|t| t == "SENT_START" || t == "SENT_END" || t == "PARAGRAPH_END")
            {
                if !one_form {
                    word_forms.insert(token_str.to_string());
                }
                one_form = true;
            } else {
                one_form = false;
            }
        }
        let mut pos_tags: Vec<&str> = readings
            .iter()
            .flat_map(|r| r.pos_tags().iter().map(String::as_str))
            .filter(|t| re_full.is_match(t))
            .collect();
        let mut target = synth.target_postag(&pos_tags).unwrap_or(postag).to_string();
        if let Some(replace) = m.postag_replace.as_deref() {
            if pos_tags.is_empty() {
                pos_tags.push(postag);
            }
            target = pos_tags
                .iter()
                .map(|t| {
                    re.replace_all(t, java_replacement(replace).as_ref())
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("|");
        }
        if !one_form {
            for reading in readings {
                if let Some(lemma) = reading.lemma() {
                    word_forms.extend(synth.synthesize_full_for(
                        lemma,
                        reading.pos_tags().first().map(String::as_str),
                        &target,
                    ));
                }
            }
        }
        let mut forms: Vec<String> = if word_forms.is_empty() {
            vec![format!("({token_str})")]
        } else {
            word_forms.into_iter().collect()
        };
        if let Some(ref cc) = m.case_conversion {
            for form in &mut forms {
                *form = apply_case_conversion(form, cc);
            }
        }
        forms
    }

    fn match_pattern_elements(
        &self,
        elements: &[og_xml::compiler::CompiledPatternElement],
        tokens: &[&AnalyzedTokenReadings],
        start_idx: usize,
    ) -> Option<PatternMatchResult> {
        self.match_elements_recursive(elements, tokens, start_idx, 0, &[], None)
    }

    fn backreference_matches(
        &self,
        pattern_token: &og_xml::compiler::CompiledPatternToken,
        token: &AnalyzedTokenReadings,
        matched: &[MatchedTraceEntry],
        tokens: &[&AnalyzedTokenReadings],
    ) -> Option<bool> {
        let match_no = pattern_token.match_no?;
        let ref_idx = match_no as usize;
        let entry = matched.get(ref_idx)?;
        let ref_text = entry.text.clone();
        let marker = format!("\\{match_no}");
        let compose = |substitute: &str| -> String {
            let base = pattern_token.text.as_deref().unwrap_or(marker.as_str());
            base.replace(&marker, substitute)
        };
        let eval_targets = |targets: &[String]| -> bool {
            if pattern_token.is_regexp {
                targets.iter().any(|t| {
                    let anchored = format!("^(?:{t})$");
                    let re = if pattern_token.case_sensitive {
                        og_xml::compiler::shared_cached_regex(&anchored)
                    } else {
                        og_xml::compiler::shared_cached_regex_ci(&anchored)
                    };
                    let surface = surface_text(token);
                    re.is_some_and(|re| {
                        if pattern_token.inflected {
                            token
                                .readings()
                                .iter()
                                .any(|r| re.is_match(r.lemma().unwrap_or(surface)))
                        } else {
                            re.is_match(surface)
                        }
                    })
                })
            } else if pattern_token.inflected {
                let surface = surface_text(token);
                token.readings().iter().any(|r| {
                    let test = normalize_apostrophes(r.lemma().unwrap_or(surface));
                    targets.iter().any(|t| {
                        let t = normalize_apostrophes(t);
                        if pattern_token.case_sensitive {
                            test == t
                        } else {
                            eq_ignore_case(test.as_ref(), t.as_ref())
                        }
                    })
                })
            } else {
                let token_text = normalize_apostrophes(surface_text(token));
                targets.iter().any(|t| {
                    let t = normalize_apostrophes(t);
                    if pattern_token.case_sensitive {
                        token_text == t
                    } else {
                        eq_ignore_case(&token_text, &t)
                    }
                })
            }
        };
        if pattern_token.match_no_setpos {
            let ref_token = tokens.get(entry.token_idx)?;
            let pos_pattern = pattern_token.match_no_postag.as_deref()?;
            let filter = whole_match_regex(pos_pattern, pattern_token.match_no_postag_regexp)?;
            let mut tags: Vec<&str> = Vec::new();
            for reading in ref_token.readings() {
                for tag in reading.pos_tags() {
                    if !tag.is_empty() && filter.is_match(tag) {
                        tags.push(tag);
                    }
                }
            }
            let target = match pattern_token.match_no_postag_replace.as_deref() {
                Some(replace) => {
                    let unanchored_body = if pattern_token.match_no_postag_regexp {
                        pos_pattern.to_string()
                    } else {
                        regex::escape(pos_pattern)
                    };
                    let unanchored = og_xml::compiler::shared_cached_regex(&unanchored_body)?;
                    let mut replaced: Vec<String> = if tags.is_empty() {
                        vec![pos_pattern.to_string()]
                    } else {
                        tags.iter().map(|t| t.to_string()).collect()
                    };
                    for t in &mut replaced {
                        *t = unanchored
                            .replace_all(t, java_replacement(replace).as_ref())
                            .into_owned();
                        *t = pl_pos_tag_correction(t);
                    }
                    replaced.join("|")
                }
                None => tags.last().copied().unwrap_or(pos_pattern).to_string(),
            };
            let target_re = whole_match_regex(&target, true)?;
            let mut hit = token.readings().iter().any(|r| {
                r.pos_tags()
                    .iter()
                    .any(|t| !t.is_empty() && target_re.is_match(t))
            });
            if hit {
                let residual = pattern_token
                    .text
                    .as_deref()
                    .map(|t| t.replace(&marker, ""))
                    .unwrap_or_default();
                if !residual.is_empty() {
                    let residual_norm = normalize_apostrophes(&residual);
                    let token_text = normalize_apostrophes(surface_text(token));
                    hit = if pattern_token.case_sensitive {
                        token_text == residual_norm
                    } else {
                        eq_ignore_case(&token_text, &residual_norm)
                    };
                }
            }
            return Some(if pattern_token.negate { !hit } else { hit });
        }
        if let Some(re) = pattern_token.compiled_match_no_regex.as_ref() {
            let replace = pattern_token
                .match_no_regexp_replace
                .as_deref()
                .unwrap_or("");
            let transformed = re
                .replace_all(&ref_text, java_replacement(replace).as_ref())
                .into_owned();
            let transformed = match pattern_token.match_no_case_conversion.as_deref() {
                Some(cc) => apply_case_conversion(&transformed, cc),
                None => transformed,
            };
            let composed = compose(&transformed);
            let matches = eval_targets(&[composed]);
            return Some(if pattern_token.negate {
                !matches
            } else {
                matches
            });
        }
        if let Some(postag) = pattern_token.match_no_postag.as_deref() {
            let ref_token = tokens.get(entry.token_idx)?;
            let synth = self
                .synthesizer
                .as_deref()
                .or(self.suggestion_synth.as_deref());
            let Some(synth) = synth else {
                let raw = match pattern_token.match_no_case_conversion.as_deref() {
                    Some(cc) => apply_case_conversion(&entry.text, cc),
                    None => entry.text.clone(),
                };
                let composed = compose(&raw);
                let matches = eval_targets(&[composed]);
                return Some(if pattern_token.negate {
                    !matches
                } else {
                    matches
                });
            };
            let mut forms: Vec<String> = Vec::new();
            for reading in ref_token.readings() {
                if let Some(lemma) = reading.lemma() {
                    for form in synth.synthesize_exact(lemma, postag) {
                        if !forms.contains(&form) {
                            forms.push(form);
                        }
                    }
                }
            }
            if forms.is_empty() {
                forms.push(ref_text.clone());
            }
            if let Some(cc) = pattern_token.match_no_case_conversion.as_deref() {
                for form in &mut forms {
                    *form = apply_case_conversion(form, cc);
                }
            }
            let forms: Vec<String> = forms.iter().map(|f| compose(f)).collect();
            let matches = eval_targets(&forms);
            return Some(if pattern_token.negate {
                !matches
            } else {
                matches
            });
        }
        let case_converted = match pattern_token.match_no_case_conversion.as_deref() {
            Some(cc) => apply_case_conversion(&ref_text, cc),
            None => ref_text,
        };
        let composed = compose(&case_converted);
        let matches = eval_targets(&[composed]);
        if pattern_token.negate {
            Some(!matches)
        } else {
            Some(matches)
        }
    }

    fn match_elements_recursive(
        &self,
        elements: &[og_xml::compiler::CompiledPatternElement],
        tokens: &[&AnalyzedTokenReadings],
        token_idx: usize,
        element_idx: usize,
        matched: &[MatchedTraceEntry],
        prevailing: Option<i32>,
    ) -> Option<PatternMatchResult> {
        if element_idx >= elements.len() {
            return Some(PatternMatchResult::new());
        }

        if token_idx >= tokens.len() {
            crate::pattern_trace::record(
                element_idx,
                usize::MAX,
                "<EOS>",
                "ran-out-of-tokens".to_string(),
            );
            return None;
        }

        let prev_skip = prevailing.unwrap_or_else(|| element_prev_skip(elements, element_idx));
        let element = &elements[element_idx];
        match element {
            og_xml::compiler::CompiledPatternElement::Token(pattern_token) => self
                .match_token_element(
                    pattern_token,
                    elements,
                    tokens,
                    token_idx,
                    element_idx,
                    prev_skip,
                    matched,
                ),
            og_xml::compiler::CompiledPatternElement::OrGroup(alternatives) => {
                for alt in alternatives {
                    if let Some(result) = self.match_token_element(
                        alt,
                        elements,
                        tokens,
                        token_idx,
                        element_idx,
                        prev_skip,
                        matched,
                    ) {
                        return Some(result);
                    }
                }
                if crate::pattern_trace::enabled()
                    && let Some(first) = alternatives.first()
                {
                    self.trace_element_failure(
                        first,
                        tokens[token_idx],
                        token_idx,
                        tokens,
                        matched,
                        element_idx,
                    );
                }
                None
            }
            og_xml::compiler::CompiledPatternElement::AndGroup(constraints) => {
                let group_skip = constraints.first().map(|c| c.skip).unwrap_or(0);
                let max_group_skip = if group_skip < 0 {
                    tokens.len()
                } else {
                    group_skip as usize
                };
                let all_match = constraints.iter().all(|c| {
                    if let Some(br) =
                        self.backreference_matches(c, tokens[token_idx], matched, tokens)
                    {
                        br
                    } else {
                        self.token_matches(c, tokens[token_idx], token_idx, prev_skip, tokens)
                    }
                });
                if all_match {
                    let mut new_mt = matched.to_vec();
                    new_mt.push(MatchedTraceEntry {
                        text: tokens[token_idx].token().token().to_string(),
                        token_idx,
                    });
                    for gap in 0..=max_group_skip {
                        let after = token_idx + 1 + gap;
                        if after > tokens.len() {
                            break;
                        }
                        if !self.next_element_commits_at(
                            elements,
                            tokens,
                            after,
                            element_idx + 1,
                            &new_mt,
                        ) {
                            continue;
                        }
                        if let Some(mut result) = self.match_elements_recursive(
                            elements,
                            tokens,
                            after,
                            element_idx + 1,
                            &new_mt,
                            None,
                        ) {
                            result.push_front(
                                token_idx,
                                element_idx,
                                tokens[token_idx].token().token().to_string(),
                            );
                            result.extend_span(
                                tokens[token_idx].token().start(),
                                tokens[token_idx].token().end(),
                            );
                            return Some(result);
                        }
                        return None;
                    }
                }
                if crate::pattern_trace::enabled()
                    && !all_match
                    && let Some(failed) = constraints.iter().find(|c| {
                        let br_failed = matches!(
                            self.backreference_matches(c, tokens[token_idx], matched, tokens),
                            Some(false)
                        );
                        br_failed
                            || !self.token_matches(
                                c,
                                tokens[token_idx],
                                token_idx,
                                prev_skip,
                                tokens,
                            )
                    })
                {
                    self.trace_element_failure(
                        failed,
                        tokens[token_idx],
                        token_idx,
                        tokens,
                        matched,
                        element_idx,
                    );
                }
                None
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn match_token_element(
        &self,
        pattern_token: &og_xml::compiler::CompiledPatternToken,
        elements: &[og_xml::compiler::CompiledPatternElement],
        tokens: &[&AnalyzedTokenReadings],
        token_idx: usize,
        element_idx: usize,
        prev_skip: i32,
        matched: &[MatchedTraceEntry],
    ) -> Option<PatternMatchResult> {
        let skip = pattern_token.skip;
        let min = pattern_token.min;
        let max = pattern_token.max;
        let max_skip = if skip < 0 {
            tokens.len()
        } else {
            skip as usize
        };

        let token_ok = |pos: usize, mt: &[MatchedTraceEntry]| -> bool {
            if let Some(br) = self.backreference_matches(pattern_token, tokens[pos], mt, tokens) {
                br && self.token_matches(pattern_token, tokens[pos], pos, prev_skip, tokens)
            } else {
                self.token_matches(pattern_token, tokens[pos], pos, prev_skip, tokens)
            }
        };

        if min == Some(0) {
            if prev_skip == 0 && self.token_scope_next_arm2_blocks(pattern_token, tokens, token_idx)
            {
                return None;
            }
            if self.later_element_matches_at(
                elements,
                tokens,
                token_idx,
                element_idx,
                matched,
                prev_skip,
            ) {
                let mut result = self.match_elements_recursive(
                    elements,
                    tokens,
                    token_idx,
                    element_idx + 1,
                    matched,
                    Some(prev_skip),
                )?;
                result.push_front(token_idx, element_idx, String::new());
                return Some(result);
            }
        }

        if min.is_some() || max.is_some() {
            if !token_ok(token_idx, matched) {
                if crate::pattern_trace::enabled() {
                    self.trace_element_failure(
                        pattern_token,
                        tokens[token_idx],
                        token_idx,
                        tokens,
                        matched,
                        element_idx,
                    );
                }
                return None;
            }
            let min_count = min.unwrap_or(1) as usize;
            let max_count =
                max.map(|m| m as usize)
                    .unwrap_or(if min_count > 0 { min_count } else { 1 });
            let mut matched_count = 0;
            let mut cur = token_idx;
            let remaining_elems = elements.len() - element_idx - 1;
            while matched_count < max_count
                && (matched_count == 0 || cur < tokens.len().saturating_sub(remaining_elems))
            {
                if token_ok(cur, matched) {
                    matched_count += 1;
                    cur += 1;
                } else {
                    break;
                }
            }
            let count_start = if min_count == 0 { 1 } else { min_count };
            if matched_count < count_start {
                crate::pattern_trace::record(
                    element_idx,
                    token_idx,
                    tokens[token_idx].token().token(),
                    "min-count-not-reached".to_string(),
                );
                return None;
            }
            let count = matched_count;
            let mut new_mt = matched.to_vec();
            for (i, t) in tokens
                .iter()
                .enumerate()
                .take(token_idx + count)
                .skip(token_idx)
            {
                new_mt.push(MatchedTraceEntry {
                    text: t.token().token().to_string(),
                    token_idx: i,
                });
            }
            let has_next_element = element_idx + 1 < elements.len();
            for gap in 0..=max_skip {
                let after = token_idx + count + gap;
                if after > tokens.len() {
                    break;
                }
                if has_next_element
                    && self.scope_next_arm1_blocks(
                        pattern_token,
                        skip,
                        tokens,
                        token_idx + count,
                        after,
                    )
                {
                    return None;
                }
                if !self.next_element_commits_at(elements, tokens, after, element_idx + 1, &new_mt)
                {
                    continue;
                }
                if let Some(mut result) = self.match_elements_recursive(
                    elements,
                    tokens,
                    after,
                    element_idx + 1,
                    &new_mt,
                    None,
                ) {
                    for i in (0..count).rev() {
                        result.push_front(
                            token_idx + i,
                            element_idx,
                            tokens[token_idx + i].token().token().to_string(),
                        );
                    }
                    result.extend_span(
                        tokens[token_idx].token().start(),
                        tokens[token_idx + count - 1].token().end(),
                    );
                    return Some(result);
                }
                return None;
            }
            return None;
        }

        if token_ok(token_idx, matched) {
            let mut new_mt = matched.to_vec();
            new_mt.push(MatchedTraceEntry {
                text: tokens[token_idx].token().token().to_string(),
                token_idx,
            });
            let has_next_element = element_idx + 1 < elements.len();
            for gap in 0..=max_skip {
                let after = token_idx + 1 + gap;
                if after > tokens.len() {
                    break;
                }
                if has_next_element
                    && self.scope_next_arm1_blocks(
                        pattern_token,
                        skip,
                        tokens,
                        token_idx + 1,
                        after,
                    )
                {
                    return None;
                }
                if !self.next_element_commits_at(elements, tokens, after, element_idx + 1, &new_mt)
                {
                    continue;
                }
                if let Some(mut result) = self.match_elements_recursive(
                    elements,
                    tokens,
                    after,
                    element_idx + 1,
                    &new_mt,
                    None,
                ) {
                    result.push_front(
                        token_idx,
                        element_idx,
                        tokens[token_idx].token().token().to_string(),
                    );
                    result.extend_span(
                        tokens[token_idx].token().start(),
                        tokens[token_idx].token().end(),
                    );
                    return Some(result);
                }
                return None;
            }
        }
        if crate::pattern_trace::enabled() {
            self.trace_element_failure(
                pattern_token,
                tokens[token_idx],
                token_idx,
                tokens,
                matched,
                element_idx,
            );
        }
        None
    }

    fn match_pattern_recursive(
        &self,
        pattern_tokens: &[og_xml::compiler::CompiledPatternToken],
        tokens: &[&AnalyzedTokenReadings],
        token_idx: usize,
        pattern_idx: usize,
        matched: &[MatchedTraceEntry],
        prevailing: Option<i32>,
    ) -> Option<PatternMatchResult> {
        if pattern_idx >= pattern_tokens.len() {
            return Some(PatternMatchResult::new());
        }

        if token_idx >= tokens.len() {
            crate::pattern_trace::record(
                pattern_idx,
                usize::MAX,
                "<EOS>",
                "ran-out-of-tokens".to_string(),
            );
            return None;
        }

        let pattern_token = &pattern_tokens[pattern_idx];
        let text_token = tokens[token_idx];
        let prev_skip = prevailing.unwrap_or(if pattern_idx == 0 {
            0
        } else {
            pattern_tokens[pattern_idx - 1].skip
        });

        if pattern_token.min == Some(0) {
            if prev_skip == 0 && self.token_scope_next_arm2_blocks(pattern_token, tokens, token_idx)
            {
                return None;
            }
            if self.later_token_matches_at(
                pattern_tokens,
                tokens,
                token_idx,
                pattern_idx,
                matched,
                prev_skip,
            ) {
                let mut result = self.match_pattern_recursive(
                    pattern_tokens,
                    tokens,
                    token_idx,
                    pattern_idx + 1,
                    matched,
                    Some(prev_skip),
                )?;
                result.push_front(token_idx, pattern_idx, String::new());
                return Some(result);
            }
        }

        let matches = if let Some(br) =
            self.backreference_matches(pattern_token, text_token, matched, tokens)
        {
            br && self.token_matches(pattern_token, text_token, token_idx, prev_skip, tokens)
        } else {
            self.token_matches(pattern_token, text_token, token_idx, prev_skip, tokens)
        };

        if !matches && crate::pattern_trace::enabled() {
            self.trace_element_failure(
                pattern_token,
                text_token,
                token_idx,
                tokens,
                matched,
                pattern_idx,
            );
        }

        if matches {
            let token_start = text_token.token().start();
            let token_end = text_token.token().end();
            let token_text = text_token.token().token().to_string();

            let min_count = pattern_token.min.unwrap_or(1) as usize;
            let max_count = pattern_token.max.map(|m| m as usize).unwrap_or(1);
            let mut repeat_count = 1;
            let mut last_token_idx = token_idx;
            let remaining_elems = pattern_tokens.len() - pattern_idx - 1;

            while repeat_count < max_count
                && last_token_idx + 1 < tokens.len().saturating_sub(remaining_elems)
            {
                let next_matches = if let Some(br) = self.backreference_matches(
                    pattern_token,
                    tokens[last_token_idx + 1],
                    matched,
                    tokens,
                ) {
                    br && self.token_matches(
                        pattern_token,
                        tokens[last_token_idx + 1],
                        last_token_idx + 1,
                        prev_skip,
                        tokens,
                    )
                } else {
                    self.token_matches(
                        pattern_token,
                        tokens[last_token_idx + 1],
                        last_token_idx + 1,
                        prev_skip,
                        tokens,
                    )
                };
                if next_matches {
                    last_token_idx += 1;
                    repeat_count += 1;
                } else {
                    break;
                }
            }

            if repeat_count < min_count {
                crate::pattern_trace::record(
                    pattern_idx,
                    token_idx,
                    text_token.token().token(),
                    "min-count-not-reached".to_string(),
                );
                return None;
            }

            let skip = pattern_token.skip;
            let max_skip = if skip < 0 {
                tokens.len()
            } else {
                skip as usize
            };

            let mut new_mt = matched.to_vec();
            for (i, t) in tokens
                .iter()
                .enumerate()
                .take(last_token_idx.min(tokens.len() - 1) + 1)
                .skip(token_idx)
            {
                if i == token_idx {
                    new_mt.push(MatchedTraceEntry {
                        text: token_text.clone(),
                        token_idx: i,
                    });
                } else {
                    new_mt.push(MatchedTraceEntry {
                        text: t.token().token().to_string(),
                        token_idx: i,
                    });
                }
            }

            let has_next_pattern = pattern_idx + 1 < pattern_tokens.len();
            for next_offset in 0..=max_skip {
                let next_token_idx = last_token_idx + 1 + next_offset;
                if next_token_idx > tokens.len() && next_offset > 0 {
                    break;
                }
                if has_next_pattern
                    && self.scope_next_arm1_blocks(
                        pattern_token,
                        skip,
                        tokens,
                        last_token_idx + 1,
                        next_token_idx,
                    )
                {
                    return None;
                }
                if !self.next_token_commits_at(
                    pattern_tokens,
                    tokens,
                    next_token_idx,
                    pattern_idx + 1,
                    &new_mt,
                ) {
                    continue;
                }
                if let Some(mut rest) = self.match_pattern_recursive(
                    pattern_tokens,
                    tokens,
                    next_token_idx,
                    pattern_idx + 1,
                    &new_mt,
                    None,
                ) {
                    let last_end = if last_token_idx < tokens.len() {
                        tokens[last_token_idx].token().end()
                    } else {
                        token_end
                    };

                    let mut positions = Vec::new();
                    let mut pat_indices = Vec::new();
                    let mut texts = Vec::new();
                    for (i, t) in tokens
                        .iter()
                        .enumerate()
                        .take(last_token_idx.min(tokens.len() - 1) + 1)
                        .skip(token_idx)
                    {
                        positions.push(i);
                        pat_indices.push(pattern_idx);
                        if i == token_idx {
                            texts.push(token_text.clone());
                        } else {
                            texts.push(t.token().token().to_string());
                        }
                    }

                    rest.matched_positions.splice(0..0, positions);
                    rest.pattern_indices.splice(0..0, pat_indices);
                    rest.matched_texts.splice(0..0, texts);
                    rest.first_start = Some(match rest.first_start {
                        Some(s) => std::cmp::min(s, token_start),
                        None => token_start,
                    });
                    rest.last_end = Some(match rest.last_end {
                        Some(e) => std::cmp::max(e, last_end),
                        None => last_end,
                    });
                    return Some(rest);
                }
                return None;
            }

            if pattern_idx + 1 >= pattern_tokens.len() {
                let last_end = if last_token_idx < tokens.len() {
                    tokens[last_token_idx].token().end()
                } else {
                    token_end
                };
                let mut positions = Vec::new();
                let mut pat_indices = Vec::new();
                let mut texts = Vec::new();
                for (i, t) in tokens
                    .iter()
                    .enumerate()
                    .take(last_token_idx.min(tokens.len() - 1) + 1)
                    .skip(token_idx)
                {
                    positions.push(i);
                    pat_indices.push(pattern_idx);
                    if i == token_idx {
                        texts.push(token_text.clone());
                    } else {
                        texts.push(t.token().token().to_string());
                    }
                }
                return Some(PatternMatchResult {
                    first_start: Some(token_start),
                    last_end: Some(last_end),
                    matched_positions: positions,
                    pattern_indices: pat_indices,
                    matched_texts: texts,
                });
            }

            None
        } else {
            None
        }
    }

    fn antipattern_first_literal(
        &self,
        antipattern: &og_xml::compiler::CompiledPattern,
    ) -> Option<(String, bool)> {
        if antipattern.has_structured_elements {
            return None;
        }
        let first = antipattern.tokens.first()?;
        if first.negate
            || first.inflected
            || first.min == Some(0)
            || first.skip != 0
            || first.compiled_regexp.is_some()
            || first.match_no.is_some()
        {
            return None;
        }
        let text = first.text.as_ref()?;
        let norm = normalize_apostrophes(text);
        Some((norm.into_owned(), first.case_sensitive))
    }

    fn resolve_backreferences(&self, text: &str, matched_texts: &[String]) -> String {
        let mut result = String::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '\\' && i + 1 < chars.len() {
                let mut num_str = String::new();
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    num_str.push(chars[j]);
                    j += 1;
                }
                if !num_str.is_empty() {
                    if let Ok(idx) = num_str.parse::<usize>() {
                        if idx > 0 && idx <= matched_texts.len() {
                            result.push_str(&matched_texts[idx - 1]);
                        } else if idx == 0 && !matched_texts.is_empty() {
                            result.push_str(&matched_texts.join(" "));
                        }
                    }
                    i = j;
                    continue;
                }
            }
            result.push(chars[i]);
            i += 1;
        }

        result
    }

    fn pattern_token_matches_at(
        &self,
        pattern_token: &og_xml::compiler::CompiledPatternToken,
        token: &AnalyzedTokenReadings,
        token_idx: usize,
        prev_skip: i32,
        tokens: &[&AnalyzedTokenReadings],
        matched: &[MatchedTraceEntry],
    ) -> bool {
        if let Some(br) = self.backreference_matches(pattern_token, token, matched, tokens) {
            br && self.token_matches(pattern_token, token, token_idx, prev_skip, tokens)
        } else {
            self.token_matches(pattern_token, token, token_idx, prev_skip, tokens)
        }
    }

    fn next_token_commits_at(
        &self,
        pattern_tokens: &[og_xml::compiler::CompiledPatternToken],
        tokens: &[&AnalyzedTokenReadings],
        token_idx: usize,
        pattern_idx: usize,
        matched: &[MatchedTraceEntry],
    ) -> bool {
        let Some(pattern_token) = pattern_tokens.get(pattern_idx) else {
            return true;
        };
        let Some(token) = tokens.get(token_idx) else {
            return false;
        };
        let prev_skip = if pattern_idx == 0 {
            0
        } else {
            pattern_tokens[pattern_idx - 1].skip
        };
        if pattern_token.min == Some(0)
            && self.later_token_matches_at(
                pattern_tokens,
                tokens,
                token_idx,
                pattern_idx,
                matched,
                prev_skip,
            )
        {
            return true;
        }
        let commits = self.pattern_token_matches_at(
            pattern_token,
            token,
            token_idx,
            prev_skip,
            tokens,
            matched,
        );
        if !commits && crate::pattern_trace::enabled() {
            self.trace_element_failure(
                pattern_token,
                token,
                token_idx,
                tokens,
                matched,
                pattern_idx,
            );
        }
        commits
    }

    fn next_element_commits_at(
        &self,
        elements: &[og_xml::compiler::CompiledPatternElement],
        tokens: &[&AnalyzedTokenReadings],
        token_idx: usize,
        element_idx: usize,
        matched: &[MatchedTraceEntry],
    ) -> bool {
        let Some(element) = elements.get(element_idx) else {
            return true;
        };
        let Some(token) = tokens.get(token_idx) else {
            return false;
        };
        let prev_skip = element_prev_skip(elements, element_idx);
        if let og_xml::compiler::CompiledPatternElement::Token(t) = element
            && t.min == Some(0)
            && self.later_element_matches_at(
                elements,
                tokens,
                token_idx,
                element_idx,
                matched,
                prev_skip,
            )
        {
            return true;
        }
        let commits =
            self.element_matches_at(element, token, token_idx, prev_skip, tokens, matched);
        if !commits
            && crate::pattern_trace::enabled()
            && let og_xml::compiler::CompiledPatternElement::Token(t) = element
        {
            self.trace_element_failure(t, token, token_idx, tokens, matched, element_idx);
        }
        commits
    }

    fn element_matches_at(
        &self,
        element: &og_xml::compiler::CompiledPatternElement,
        token: &AnalyzedTokenReadings,
        token_idx: usize,
        prev_skip: i32,
        tokens: &[&AnalyzedTokenReadings],
        matched: &[MatchedTraceEntry],
    ) -> bool {
        match element {
            og_xml::compiler::CompiledPatternElement::Token(t) => {
                self.pattern_token_matches_at(t, token, token_idx, prev_skip, tokens, matched)
            }
            og_xml::compiler::CompiledPatternElement::OrGroup(alts) => alts.iter().any(|a| {
                self.pattern_token_matches_at(a, token, token_idx, prev_skip, tokens, matched)
            }),
            og_xml::compiler::CompiledPatternElement::AndGroup(constraints) => {
                constraints.iter().all(|c| {
                    self.pattern_token_matches_at(c, token, token_idx, prev_skip, tokens, matched)
                })
            }
        }
    }

    fn later_token_matches_at(
        &self,
        pattern_tokens: &[og_xml::compiler::CompiledPatternToken],
        tokens: &[&AnalyzedTokenReadings],
        token_idx: usize,
        pattern_idx: usize,
        matched: &[MatchedTraceEntry],
        prevailing: i32,
    ) -> bool {
        if self.scope_next_arm1_blocks(
            &pattern_tokens[pattern_idx],
            prevailing,
            tokens,
            token_idx,
            token_idx,
        ) {
            return false;
        }
        for tok in &pattern_tokens[pattern_idx + 1..] {
            if self.pattern_token_matches_at(
                tok,
                tokens[token_idx],
                token_idx,
                prevailing,
                tokens,
                matched,
            ) {
                return true;
            }
            if tok.min.unwrap_or(1) > 0 {
                return false;
            }
        }
        false
    }

    fn later_element_matches_at(
        &self,
        elements: &[og_xml::compiler::CompiledPatternElement],
        tokens: &[&AnalyzedTokenReadings],
        token_idx: usize,
        element_idx: usize,
        matched: &[MatchedTraceEntry],
        prevailing: i32,
    ) -> bool {
        if let og_xml::compiler::CompiledPatternElement::Token(skipped) = &elements[element_idx]
            && self.scope_next_arm1_blocks(skipped, prevailing, tokens, token_idx, token_idx)
        {
            return false;
        }
        for element in &elements[element_idx + 1..] {
            let matched = self.element_matches_at(
                element,
                tokens[token_idx],
                token_idx,
                prevailing,
                tokens,
                matched,
            );
            if matched {
                return true;
            }
            let optional = matches!(element, og_xml::compiler::CompiledPatternElement::Token(t) if t.min.unwrap_or(1) == 0);
            if !optional {
                return false;
            }
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn trace_element_failure(
        &self,
        pattern: &og_xml::compiler::CompiledPatternToken,
        token: &AnalyzedTokenReadings,
        token_idx: usize,
        tokens: &[&AnalyzedTokenReadings],
        matched: &[MatchedTraceEntry],
        pattern_idx: usize,
    ) {
        let surface = token.token().token();
        let cause = if token.is_immunized() {
            "immunized".to_string()
        } else if pattern.match_no.is_some()
            && self
                .backreference_matches(pattern, token, matched, tokens)
                .is_some_and(|br| !br)
        {
            format!("backref-no={:?} mismatch", pattern.match_no)
        } else {
            let has_string = pattern.text.is_some()
                || pattern.compiled_regexp.is_some()
                || pattern.compiled_regexp_fancy.is_some();
            if has_string && !element_surface_text_matches(pattern, token) {
                if pattern.inflected {
                    format!("inflected-lemma!={:?}", requirement_text(pattern))
                } else {
                    format!("surface!={:?}", requirement_text(pattern))
                }
            } else if !element_matches_readings(pattern, token) {
                if pattern.negate && pattern.postag.is_none() {
                    format!("negated token matched surface {surface:?}")
                } else {
                    format!(
                        "postag={}{:?} got=[{}]",
                        if pattern.negate_pos { "!" } else { "" },
                        pattern.postag.as_deref().unwrap_or(""),
                        token
                            .readings()
                            .iter()
                            .take(4)
                            .map(|r| {
                                format!("{}:{}", r.lemma().unwrap_or(""), r.pos_tags().join("|"))
                            })
                            .collect::<Vec<_>>()
                            .join("; ")
                    )
                }
            } else if let Some(ref chunk) = pattern.chunk {
                format!("chunk!={chunk:?}")
            } else if pattern.chunk_re.is_some() {
                "chunk_re mismatch".to_string()
            } else if let Some(ref sb) = pattern.space_before {
                format!("space_before={sb}")
            } else if let Some(vetoed) = pattern.exceptions.iter().find_map(|exc| {
                let exc_matches = match exc.scope.as_deref() {
                    Some("next") => tokens
                        .get(token_idx + 1)
                        .is_some_and(|next| exception_matches_reading0(exc, next, true)),
                    Some("previous") => token_idx
                        .checked_sub(1)
                        .and_then(|prev| tokens.get(prev))
                        .is_some_and(|prev| self.exception_matches(exc, prev, true)),
                    _ => self.exception_matches(exc, token, has_space_before(tokens, token_idx)),
                };
                exc_matches.then(|| {
                    format!(
                        "exception scope={:?} text={:?} postag={:?} negate={}",
                        exc.scope,
                        exc.text.as_deref().unwrap_or(""),
                        exc.postag.as_deref().unwrap_or(""),
                        exc.negate
                    )
                })
            }) {
                vetoed
            } else {
                "unclassified".to_string()
            }
        };
        crate::pattern_trace::record(pattern_idx, token_idx, surface, cause);
    }

    fn token_matches(
        &self,
        pattern: &og_xml::compiler::CompiledPatternToken,
        token: &AnalyzedTokenReadings,
        token_idx: usize,
        prev_skip: i32,
        tokens: &[&AnalyzedTokenReadings],
    ) -> bool {
        if token.is_immunized() {
            return false;
        }
        if !element_matches_readings(pattern, token) {
            return false;
        }

        if let Some(ref chunk) = pattern.chunk
            && !token.has_chunk(chunk)
        {
            return false;
        }
        if let Some(ref chunk_re) = pattern.chunk_re
            && !token.has_chunk_matching(chunk_re)
        {
            return false;
        }

        if let Some(ref sb) = pattern.space_before
            && token_idx > 0
        {
            let prev_end = tokens[token_idx - 1].token().end();
            let cur_start = token.token().start();
            if sb == "no" {
                if cur_start != prev_end {
                    return false;
                }
            } else if sb == "yes" && cur_start == prev_end {
                return false;
            }
        }

        for exc in &pattern.exceptions {
            let exc_matches = match exc.scope.as_deref() {
                Some("next") if prev_skip == 0 => {
                    self.token_scope_next_arm2_blocks(pattern, tokens, token_idx)
                }
                Some("next") => false,
                Some("previous") => {
                    if token_idx > 0 {
                        self.exception_matches(
                            exc,
                            tokens[token_idx - 1],
                            has_space_before(tokens, token_idx - 1),
                        )
                    } else {
                        false
                    }
                }
                _ => self.exception_matches(exc, token, has_space_before(tokens, token_idx)),
            };

            if exc_matches {
                return false;
            }
        }

        true
    }

    fn exception_matches(
        &self,
        exc: &og_xml::compiler::CompiledException,
        token: &AnalyzedTokenReadings,
        token_space_before: bool,
    ) -> bool {
        let readings = token.readings();
        if exc
            .space_before
            .is_some_and(|required| required != token_space_before)
        {
            return false;
        }
        let has_string = exc.compiled_regexp.is_some()
            || exc.compiled_regexp_fancy.is_some()
            || exc.text.is_some();
        if !has_string && exc.postag.is_none() {
            return false;
        }
        let untagged = has_no_tag_reading(token);
        if !has_string {
            return !exc.negate
                && readings
                    .iter()
                    .any(|r| exception_reading_pos_ok(exc, r, untagged));
        }
        let surface = surface_text(token);
        readings
            .iter()
            .any(|r| reading_matches_exception(exc, surface, r, untagged))
    }

    fn scope_next_arm1_blocks(
        &self,
        pattern_token: &og_xml::compiler::CompiledPatternToken,
        skip: i32,
        tokens: &[&AnalyzedTokenReadings],
        matched_end: usize,
        candidate: usize,
    ) -> bool {
        if candidate >= tokens.len() {
            return false;
        }
        let tokens_len = tokens.len() as i64;
        let next_pos = matched_end as i64;
        let effective_skip = if i64::from(skip) + next_pos >= tokens_len || skip < 0 {
            (tokens_len - next_pos - 1) as i32
        } else {
            skip
        };
        if effective_skip <= 0 {
            return false;
        }
        pattern_token.exceptions.iter().any(|exc| {
            exc.scope.as_deref() == Some("next")
                && self.exception_matches(
                    exc,
                    tokens[candidate],
                    has_space_before(tokens, candidate),
                )
        })
    }

    fn token_scope_next_arm2_blocks(
        &self,
        pattern_token: &og_xml::compiler::CompiledPatternToken,
        tokens: &[&AnalyzedTokenReadings],
        token_idx: usize,
    ) -> bool {
        let Some(next) = tokens.get(token_idx + 1) else {
            return false;
        };
        pattern_token.exceptions.iter().any(|exc| {
            exc.scope.as_deref() == Some("next")
                && exception_matches_reading0(exc, next, has_space_before(tokens, token_idx + 1))
        })
    }
}

fn element_prev_skip(
    elements: &[og_xml::compiler::CompiledPatternElement],
    element_idx: usize,
) -> i32 {
    if element_idx == 0 {
        return 0;
    }
    match &elements[element_idx - 1] {
        og_xml::compiler::CompiledPatternElement::Token(t) => t.skip,
        og_xml::compiler::CompiledPatternElement::OrGroup(alts) => {
            alts.iter().map(|a| a.skip).max().unwrap_or(0)
        }
        og_xml::compiler::CompiledPatternElement::AndGroup(c) => {
            c.first().map(|c| c.skip).unwrap_or(0)
        }
    }
}

fn exception_matches_reading0(
    exc: &og_xml::compiler::CompiledException,
    token: &AnalyzedTokenReadings,
    token_space_before: bool,
) -> bool {
    if exc
        .space_before
        .is_some_and(|required| required != token_space_before)
    {
        return false;
    }
    let has_string =
        exc.compiled_regexp.is_some() || exc.compiled_regexp_fancy.is_some() || exc.text.is_some();
    if !has_string && exc.postag.is_none() {
        return false;
    }
    let Some(reading0) = token.readings().first() else {
        return false;
    };
    let untagged = has_no_tag_reading(token);
    if !has_string {
        return !exc.negate && exception_reading_pos_ok(exc, reading0, untagged);
    }
    let surface = surface_text(token);
    reading_matches_exception(exc, surface, reading0, untagged)
}

fn exception_reading_pos_ok(
    exc: &og_xml::compiler::CompiledException,
    reading: &og_core::AnalyzedToken,
    token_untagged: bool,
) -> bool {
    match exc.postag.as_deref() {
        Some(postag) => {
            reading_pos_matches(
                postag,
                exc.compiled_postag_regexp.as_ref(),
                exc.compiled_postag_regexp_fancy.as_ref(),
                reading.pos_tags(),
                token_untagged,
            ) != exc.negate_pos
        }
        None => true,
    }
}

fn exception_text_matches(exc: &og_xml::compiler::CompiledException, test: &str) -> bool {
    let t = normalize_apostrophes(test);
    if let Some(re) = exc.compiled_regexp.as_ref() {
        re.is_match(t.as_ref())
    } else if let Some(re) = exc.compiled_regexp_fancy.as_ref() {
        re.is_match(t.as_ref()).unwrap_or(false)
    } else if let Some(text) = exc.text.as_deref() {
        let norm_text = normalize_apostrophes(text);
        if exc.case_sensitive {
            t == norm_text
        } else {
            eq_ignore_case(t.as_ref(), norm_text.as_ref())
        }
    } else {
        true
    }
}

fn reading_matches_exception(
    exc: &og_xml::compiler::CompiledException,
    surface: &str,
    reading: &og_core::AnalyzedToken,
    token_untagged: bool,
) -> bool {
    let test = if exc.inflected {
        reading.lemma().unwrap_or(surface)
    } else {
        surface
    };
    let text_ok = exception_text_matches(exc, test);
    (text_ok != exc.negate) && exception_reading_pos_ok(exc, reading, token_untagged)
}

fn reading_pos_ok(
    pattern: &og_xml::compiler::CompiledPatternToken,
    reading: &og_core::AnalyzedToken,
    token_untagged: bool,
) -> bool {
    match pattern.postag.as_deref() {
        Some(postag) => {
            reading_pos_matches(
                postag,
                pattern.compiled_postag_regexp.as_ref(),
                pattern.compiled_postag_regexp_fancy.as_ref(),
                reading.pos_tags(),
                token_untagged,
            ) != pattern.negate_pos
        }
        None => true,
    }
}

fn requirement_text(pattern: &og_xml::compiler::CompiledPatternToken) -> String {
    if let Some(ref t) = pattern.text {
        t.clone()
    } else if let Some(ref re) = pattern.compiled_regexp {
        re.as_str().to_string()
    } else if let Some(ref re) = pattern.compiled_regexp_fancy {
        re.as_str().to_string()
    } else {
        String::new()
    }
}

fn element_surface_text_matches(
    pattern: &og_xml::compiler::CompiledPatternToken,
    token: &AnalyzedTokenReadings,
) -> bool {
    let raw_text = surface_text(token);
    let token_text = normalize_apostrophes(raw_text);
    if let Some(ref text) = pattern.text {
        let norm_text = normalize_apostrophes(text);
        if pattern.case_sensitive {
            token_text == norm_text
        } else {
            eq_ignore_case(&token_text, &norm_text)
        }
    } else {
        let regex_hit = |target: &str| {
            if let Some(ref re) = pattern.compiled_regexp_fancy {
                re.is_match(target).unwrap_or(false)
            } else if let Some(ref re) = pattern.compiled_regexp {
                if !pattern.case_sensitive
                    && let Some(ref ci_re) = pattern.compiled_regexp_ci
                {
                    ci_re.is_match(target)
                } else {
                    re.is_match(target)
                }
            } else {
                true
            }
        };
        regex_hit(raw_text) || token_text.as_ref() != raw_text && regex_hit(token_text.as_ref())
    }
}

fn element_matches_readings(
    pattern: &og_xml::compiler::CompiledPatternToken,
    token: &AnalyzedTokenReadings,
) -> bool {
    let string_is_reference = pattern
        .match_no
        .is_some_and(|no| match pattern.text.as_deref() {
            Some(text) => text.contains(&format!("\\{no}")),
            None => false,
        });
    let has_string = !string_is_reference
        && (pattern.text.is_some()
            || pattern.compiled_regexp.is_some()
            || pattern.compiled_regexp_fancy.is_some());
    let untagged = has_no_tag_reading(token);
    let any_pos_ok = |readings: &[og_core::AnalyzedToken]| {
        if readings.is_empty() {
            pattern.postag.is_none()
        } else {
            readings
                .iter()
                .any(|r| reading_pos_ok(pattern, r, untagged))
        }
    };
    if !has_string {
        if pattern.match_no.is_some() {
            return any_pos_ok(token.readings());
        }
        return !pattern.negate && any_pos_ok(token.readings());
    }
    if pattern.inflected {
        let surface = surface_text(token);
        if token
            .readings()
            .iter()
            .any(|r| reading_matches_element(pattern, surface, r, untagged))
        {
            return true;
        }
        return element_surface_text_matches(pattern, token) != pattern.negate
            && any_pos_ok(token.readings());
    }
    element_surface_text_matches(pattern, token) != pattern.negate && any_pos_ok(token.readings())
}

fn has_space_before(tokens: &[&AnalyzedTokenReadings], token_idx: usize) -> bool {
    token_idx > 0 && tokens[token_idx].token().start() != tokens[token_idx - 1].token().end()
}

#[derive(Clone)]
struct MatchedTraceEntry {
    text: String,
    token_idx: usize,
}

struct PatternMatchResult {
    first_start: Option<usize>,
    last_end: Option<usize>,
    matched_positions: Vec<usize>,
    pattern_indices: Vec<usize>,
    matched_texts: Vec<String>,
}

impl PatternMatchResult {
    fn new() -> Self {
        Self {
            first_start: None,
            last_end: None,
            matched_positions: Vec::new(),
            pattern_indices: Vec::new(),
            matched_texts: Vec::new(),
        }
    }

    fn push_front(&mut self, pos: usize, pattern_idx: usize, text: String) {
        self.matched_positions.insert(0, pos);
        self.pattern_indices.insert(0, pattern_idx);
        self.matched_texts.insert(0, text);
    }

    fn extend_span(&mut self, start: usize, end: usize) {
        self.first_start = Some(match self.first_start {
            Some(s) => std::cmp::min(s, start),
            None => start,
        });
        self.last_end = Some(match self.last_end {
            Some(e) => std::cmp::max(e, end),
            None => end,
        });
    }

    fn position_for_pattern(&self, pattern_idx: usize) -> Option<usize> {
        self.pattern_indices
            .iter()
            .position(|&p| p == pattern_idx)
            .and_then(|i| self.matched_positions.get(i).copied())
    }

    fn last_position_for_pattern(&self, pattern_idx: usize) -> Option<usize> {
        self.pattern_indices
            .iter()
            .rposition(|&p| p == pattern_idx)
            .and_then(|i| self.matched_positions.get(i).copied())
    }
}

fn is_suppress_junk(suggestion: &str) -> bool {
    static JUNK: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    JUNK.get_or_init(|| {
        regex::Regex::new(r"^[^<>()]*(?:\([^<>()]*\)|<mistake/>)[^<>()]*$").unwrap()
    })
    .is_match(suggestion)
}

fn comma_leading_suggestion(rule: &CompiledRule, matched_texts: &[String]) -> bool {
    if rule.message.contains("<suggestion>,") {
        return true;
    }
    rule.suggestions.iter().any(|s| {
        let mut leading = String::new();
        for part in &s.parts {
            match part {
                og_xml::types::SuggestionPart::Text(t) => leading.push_str(t),
                og_xml::types::SuggestionPart::Match(m) => {
                    leading.push_str(m.regexp_replace.as_deref().unwrap_or("x"));
                }
            }
            if !leading.is_empty() {
                break;
            }
        }
        let resolved = resolve_leading_backrefs(&leading, matched_texts);
        resolved.starts_with(',')
    })
}

fn resolve_leading_backrefs(text: &str, matched_texts: &[String]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
            let mut num = String::new();
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                num.push(chars[j]);
                j += 1;
            }
            if let Ok(idx) = num.parse::<usize>()
                && idx > 0
                && idx <= matched_texts.len()
            {
                result.push_str(&matched_texts[idx - 1]);
            }
            i = j;
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

fn unify_groups_satisfied(
    rule: &CompiledRule,
    match_result: &PatternMatchResult,
    tokens: &[&AnalyzedTokenReadings],
) -> bool {
    for group in &rule.pattern.unify_groups {
        if !unify_group_matches(
            &rule.unifications,
            &rule.pattern,
            group,
            &match_result.matched_positions,
            &match_result.pattern_indices,
            &match_result.matched_texts,
            tokens,
        ) {
            return false;
        }
    }
    true
}

fn antipattern_unify_satisfied(
    unifications: &[og_xml::compiler::CompiledUnification],
    antipattern: &og_xml::compiler::CompiledPattern,
    matched_positions: &[usize],
    pattern_indices: &[usize],
    matched_texts: &[String],
    tokens: &[&AnalyzedTokenReadings],
) -> bool {
    for group in &antipattern.unify_groups {
        if !unify_group_matches(
            unifications,
            antipattern,
            group,
            matched_positions,
            pattern_indices,
            matched_texts,
            tokens,
        ) {
            return false;
        }
    }
    true
}

use og_xml::unifier::{
    Unifier, UnifyElemPlan, UnifyReading, eq_ignore_case, eval_unify_plans, has_no_tag_reading,
    normalize_apostrophes, reading_matches_element, reading_pos_matches, surface_text,
};

fn unify_group_matches(
    unifications: &[og_xml::compiler::CompiledUnification],
    rule_pattern: &og_xml::compiler::CompiledPattern,
    group: &og_xml::types::XmlUnifyGroup,
    matched_positions: &[usize],
    pattern_indices: &[usize],
    matched_texts: &[String],
    tokens: &[&AnalyzedTokenReadings],
) -> bool {
    if group.elem_end <= group.elem_start {
        return group.negate;
    }
    let last_elem = group.elem_end - 1;
    let mut plans: Vec<UnifyElemPlan<'_>> = Vec::with_capacity(group.elem_end - group.elem_start);
    for elem_idx in group.elem_start..group.elem_end {
        let Some(element) = rule_pattern.elements.get(elem_idx) else {
            continue;
        };
        let criteria: Vec<&og_xml::compiler::CompiledPatternToken> = match element {
            og_xml::compiler::CompiledPatternElement::Token(t) => vec![t.as_ref()],
            og_xml::compiler::CompiledPatternElement::AndGroup(c) => match c.first() {
                Some(head) => vec![head],
                None => continue,
            },
            og_xml::compiler::CompiledPatternElement::OrGroup(alts) => alts.iter().collect(),
        };
        let optional = matches!(
            element,
            og_xml::compiler::CompiledPatternElement::Token(t) if t.min == Some(0)
        );
        let entries: Vec<usize> = pattern_indices
            .iter()
            .enumerate()
            .filter(|&(_, &p)| p == elem_idx)
            .filter(|&(i, _)| !(optional && matched_texts.get(i).is_some_and(|t| t.is_empty())))
            .filter_map(|(i, _)| matched_positions.get(i).copied())
            .collect();
        if criteria.first().is_some_and(|c| c.unify_neutral) {
            plans.push(UnifyElemPlan {
                elem_idx,
                neutral_readings: entries
                    .iter()
                    .filter_map(|&p| tokens.get(p).map(|t| t.readings().len()))
                    .collect(),
                reading_sets: Vec::new(),
            });
            continue;
        }
        let mut all_variants: Vec<Vec<Vec<UnifyReading<'_>>>> = Vec::with_capacity(criteria.len());
        for criterion in &criteria {
            let mut reading_sets: Vec<Vec<UnifyReading<'_>>> = Vec::with_capacity(entries.len());
            for &sentence_token in &entries {
                let Some(token) = tokens.get(sentence_token) else {
                    continue;
                };
                let surface = surface_text(token);
                let untagged = has_no_tag_reading(token);
                let mut reading_tags: Vec<UnifyReading<'_>> =
                    Vec::with_capacity(token.readings().len());
                for (ri, reading) in token.readings().iter().enumerate() {
                    if reading_matches_element(criterion, surface, reading, untagged) {
                        reading_tags.push(UnifyReading {
                            tags: reading.pos_tags().iter().map(String::as_str).collect(),
                            id: ri as u32,
                        });
                    }
                }
                if !reading_tags.is_empty() {
                    reading_sets.push(reading_tags);
                }
            }
            all_variants.push(reading_sets);
        }
        all_variants.retain(|v| !v.is_empty());
        if all_variants.is_empty() {
            continue;
        }
        plans.push(UnifyElemPlan {
            elem_idx,
            neutral_readings: Vec::new(),
            reading_sets: all_variants,
        });
    }
    let mut uni = Unifier::from_unifications(unifications, group);
    eval_unify_plans(&plans, 0, last_elem, group.negate, &mut uni)
}

fn uppercase_first_char(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        out.push_str(&first.to_uppercase().to_string());
        out.push_str(chars.as_str());
    }
    out
}

fn is_sagt(w: &str) -> bool {
    matches!(w, "Sagt" | "sagt" | "Sag" | "sag")
}

fn is_denke_etc(w: &str) -> bool {
    matches!(
        w,
        "denke" | "dachte" | "glaube" | "schätze" | "vermute" | "behaupte"
    )
}

fn is_bei_fuer_mit(w: &str) -> bool {
    matches!(w, "bei" | "für" | "mit")
}

fn is_dir_etc(w: &str) -> bool {
    let lower: String = w.chars().flat_map(|c| c.to_lowercase()).collect();
    matches!(lower.as_str(), "dir" | "dich" | "euer" | "euch")
}

fn is_der_etc(tokens: &[&AnalyzedTokenReadings]) -> bool {
    tokens
        .get(1)
        .map(|t| {
            matches!(
                t.token().token(),
                "der"
                    | "die"
                    | "das"
                    | "seine"
                    | "ihre"
                    | "deine"
                    | "unsere"
                    | "meine"
                    | "folgender"
                    | "dieser"
            )
        })
        .unwrap_or(false)
}

fn skipped_suffix(
    tokens: &[&AnalyzedTokenReadings],
    matched_positions: &[usize],
    pattern_idx: usize,
    following: bool,
) -> String {
    let Some(&token_idx) = matched_positions.get(pattern_idx) else {
        return String::new();
    };
    let end = matched_positions
        .get(pattern_idx + 1)
        .copied()
        .unwrap_or(tokens.len());
    let mut out = String::new();
    for k in (token_idx + 1)..end {
        let Some(token) = tokens.get(k) else {
            break;
        };
        if out.is_empty() {
            if !following {
                out.push(' ');
            }
        } else if tokens
            .get(k - 1)
            .is_some_and(|t| t.token().end() < token.token().start())
        {
            out.push(' ');
        }
        out.push_str(token.token().token());
    }
    out
}

fn resolve_regexp_message(text: &str, groups: &[&str]) -> String {
    let mut result = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            let idx = (bytes[i + 1] - b'0') as usize;
            if let Some(value) = groups.get(idx) {
                result.push_str(value);
            }
            i += 2;
            continue;
        }
        let ch_len = text[i..].chars().next().map_or(1, char::len_utf8);
        result.push_str(&text[i..i + ch_len]);
        i += ch_len;
    }
    result
}

fn regex_antipattern_allows(args: &str, text: &str, from: usize, to: usize) -> bool {
    let Some((_, patterns)) = args.split_once("antipatterns:") else {
        return true;
    };
    for pattern in patterns.split('|') {
        let Ok(re) = regex::Regex::new(pattern) else {
            continue;
        };
        for mat in re.find_iter(text) {
            let overlap = (mat.start() <= to && mat.end() >= to)
                || (mat.start() <= from && mat.end() >= from);
            if overlap {
                return false;
            }
        }
    }
    true
}

fn equal_without_diacritics(a: &str, b: &str) -> bool {
    let fold = |s: &str| {
        s.chars()
            .flat_map(|c| c.to_lowercase())
            .map(|c| match c {
                'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
                'é' | 'è' | 'ê' | 'ë' => 'e',
                'í' | 'ì' | 'î' | 'ï' => 'i',
                'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
                'ú' | 'ù' | 'û' | 'ü' => 'u',
                'ñ' => 'n',
                'ç' => 'c',
                'ý' | 'ÿ' => 'y',
                other => other,
            })
            .collect::<String>()
    };
    fold(a) == fold(b)
}

impl Default for PatternRuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

struct FilterContext<'a> {
    rule: &'a CompiledRule,
    matched_texts: &'a [String],
    matched_positions: &'a [usize],
    element_positions: &'a [usize],
    tokens: &'a [&'a AnalyzedTokenReadings],
    sentence: &'a AnalyzedSentence,
}

impl PatternRuleEngine {
    fn apply_filter(
        &self,
        ctx: FilterContext<'_>,
        filter: &og_xml::types::XmlFilter,
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let FilterContext {
            rule,
            matched_texts,
            matched_positions,
            element_positions,
            tokens,
            sentence,
        } = ctx;
        match filter.class.as_str() {
            "org.languagetool.rules.en.FutureDateFilter" => {
                if self.apply_future_date_filter(&filter.args, matched_texts, DateLang::En) {
                    Some(rm)
                } else {
                    None
                }
            }
            "org.languagetool.rules.de.FutureDateFilter" => {
                if self.apply_future_date_filter(&filter.args, matched_texts, DateLang::De) {
                    Some(rm)
                } else {
                    None
                }
            }
            "org.languagetool.rules.it.FutureDateFilter" => {
                if self.apply_future_date_filter(&filter.args, matched_texts, DateLang::It) {
                    Some(rm)
                } else {
                    None
                }
            }
            "org.languagetool.rules.pt.FutureDateFilter" => {
                if self.apply_future_date_filter(&filter.args, matched_texts, DateLang::Pt) {
                    Some(rm)
                } else {
                    None
                }
            }
            "org.languagetool.rules.ru.FutureDateFilter" => {
                if self.apply_future_date_filter(&filter.args, matched_texts, DateLang::Ru) {
                    Some(rm)
                } else {
                    None
                }
            }
            "org.languagetool.rules.ru.INNNumberFilter" => {
                let parsed = self.resolve_java_filter_args(&filter.args, matched_texts);
                match parsed.get("inn") {
                    Some(inn) if inn_is_valid(inn) => None,
                    Some(_) => Some(rm),
                    None => None,
                }
            }
            "org.languagetool.rules.ru.RussianPartialPosTagFilter" => {
                let parsed = self.resolve_java_filter_args(&filter.args, matched_texts);
                if russian_partial_tag_satisfies(&parsed, matched_texts) {
                    Some(rm)
                } else {
                    None
                }
            }
            "org.languagetool.rules.it.DateCheckFilter"
            | "org.languagetool.rules.pl.DateCheckFilter"
            | "org.languagetool.rules.ru.DateCheckFilter"
            | "org.languagetool.rules.br.DateCheckFilter"
            | "org.languagetool.rules.eo.DateCheckFilter"
            | "org.languagetool.rules.ar.filters.ArabicDateCheckFilter" => {
                let lang = match filter.class.as_str() {
                    c if c.contains(".it.") => DateLang::It,
                    c if c.contains(".pl.") => DateLang::Pl,
                    c if c.contains(".ru.") => DateLang::Ru,
                    c if c.contains(".br.") => DateLang::Br,
                    c if c.contains(".eo.") => DateLang::Eo,
                    c if c.contains("Arabic") => DateLang::Ar,
                    _ => DateLang::En,
                };
                self.apply_date_check_filter(&filter.args, matched_texts, lang, rm)
            }
            "org.languagetool.rules.en.DateCheckFilter"
            | "org.languagetool.rules.de.DateCheckFilter"
            | "org.languagetool.rules.fr.DateCheckFilter"
            | "org.languagetool.rules.es.DateCheckFilter"
            | "org.languagetool.rules.pt.DateCheckFilter"
            | "org.languagetool.rules.nl.DateCheckFilter"
            | "org.languagetool.rules.ca.DateCheckFilter" => {
                let lang = match filter.class.as_str() {
                    c if c.contains(".de.") => DateLang::De,
                    c if c.contains(".fr.") => DateLang::Fr,
                    c if c.contains(".es.") => DateLang::Es,
                    c if c.contains(".pt.") => DateLang::Pt,
                    c if c.contains(".nl.") => DateLang::Nl,
                    c if c.contains(".ca.") => DateLang::Ca,
                    _ => DateLang::En,
                };
                self.apply_date_check_with_suggestions_filter(
                    lang,
                    &filter.args,
                    element_positions,
                    tokens,
                    sentence.start(),
                    rm,
                )
            }
            "org.languagetool.rules.en.NewYearDateFilter"
            | "org.languagetool.rules.de.NewYearDateFilter"
            | "org.languagetool.rules.fr.NewYearDateFilter"
            | "org.languagetool.rules.es.NewYearDateFilter"
            | "org.languagetool.rules.pt.NewYearDateFilter" => {
                let lang = match filter.class.as_str() {
                    c if c.contains(".de.") => DateLang::De,
                    c if c.contains(".fr.") => DateLang::Fr,
                    c if c.contains(".es.") => DateLang::Es,
                    c if c.contains(".pt.") => DateLang::Pt,
                    _ => DateLang::En,
                };
                self.apply_new_year_date_filter(&filter.args, matched_texts, lang, rm)
            }
            "org.languagetool.rules.en.YMDNewYearDateFilter"
            | "org.languagetool.rules.de.YMDNewYearDateFilter"
            | "org.languagetool.rules.pt.YMDNewYearDateFilter" => {
                let lang = match filter.class.as_str() {
                    c if c.contains(".de.") => DateLang::De,
                    c if c.contains(".pt.") => DateLang::Pt,
                    _ => DateLang::En,
                };
                self.apply_ymd_new_year_date_filter(&filter.args, matched_texts, lang, rm)
            }
            "org.languagetool.rules.de.RecentYearFilter" => {
                if self.apply_recent_year_filter(&filter.args, matched_texts) {
                    Some(rm)
                } else {
                    None
                }
            }
            "org.languagetool.rules.de.InsertCommaFilter" => {
                Some(self.apply_insert_comma_filter(matched_texts, matched_positions, tokens, rm))
            }
            "org.languagetool.rules.de.RemoveUnknownCompoundsFilter" => {
                self.apply_remove_unknown_compounds_filter(&filter.args, matched_texts, rm)
            }
            "org.languagetool.rules.de.PotentialCompoundFilter" => {
                self.apply_potential_compound_filter(&filter.args, matched_texts, rm)
            }
            "org.languagetool.rules.de.ValidWordFilter" => {
                self.apply_valid_word_filter(&filter.args, matched_texts, rm)
            }
            "org.languagetool.rules.en.EnglishSuppressMisspelledSuggestionsFilter"
            | "org.languagetool.rules.nl.DutchSuppressMisspelledSuggestionsFilter" => {
                self.apply_suppress_misspelled_suggestions_filter(&filter.args, matched_texts, rm)
            }
            "org.languagetool.rules.fr.FrenchSuppressMisspelledSuggestionsFilter" => {
                crate::fr_suppress::accept_rule_match(rm, &filter.args, matched_texts)
            }
            "org.languagetool.rules.ru.RussianSuppressMisspelledSuggestionsFilter" => {
                crate::ru_filters::accept_rule_match(rm, &filter.args, matched_texts)
            }
            "org.languagetool.rules.ru.AdvancedSynthesizerFilter" => {
                let russian_synth = crate::ru_filters::RussianSynthesizer::shared();
                crate::ru_filters::advanced_synthesizer_filter(
                    russian_synth.as_deref(),
                    &filter.args,
                    matched_texts,
                    matched_positions,
                    tokens,
                    rm,
                )
            }
            "org.languagetool.rules.en.EnglishNumberInWordFilter"
            | "org.languagetool.rules.de.GermanNumberInWordFilter" => {
                self.apply_number_in_word_filter(&filter.args, matched_texts, rm)
            }
            "org.languagetool.rules.en.FindSuggestionsFilter" => {
                self.apply_find_suggestions_filter(&filter.args, matched_texts, rm)
            }
            "org.languagetool.rules.en.AdverbFilter" => {
                self.apply_adverb_filter(&filter.args, matched_texts, rm)
            }
            "org.languagetool.rules.en.OrdinalSuffixFilter" => self.apply_ordinal_suffix_filter(rm),
            "org.languagetool.rules.UnderlineSpacesFilter" => {
                self.apply_underline_spaces_filter(&filter.args, sentence, rm)
            }
            "org.languagetool.rules.spelling.multitoken.MultitokenSpellerFilter" => {
                self.apply_multitoken_speller_filter(matched_positions, tokens, sentence, rm)
            }
            "org.languagetool.rules.DateRangeChecker" => {
                if self.apply_date_range_filter(&filter.args, matched_texts) {
                    Some(rm)
                } else {
                    None
                }
            }
            "org.languagetool.rules.pl.DecadeSpellingFilter" => {
                crate::pl_filters::decade_spelling_filter(&filter.args, matched_texts, rm)
            }
            "org.languagetool.rules.ShortenedYearRangeChecker" => {
                crate::pl_filters::shortened_year_range_filter(&filter.args, matched_texts, rm)
            }
            "org.languagetool.rules.patterns.RegexAntiPatternFilter" => {
                if self.apply_regex_antipattern_filter(&filter.args, matched_texts) {
                    Some(rm)
                } else {
                    None
                }
            }
            "org.languagetool.rules.patterns.ApostropheTypeFilter" => {
                if self.apply_apostrophe_type_filter(&filter.args, matched_positions, tokens, &rm) {
                    Some(rm)
                } else {
                    None
                }
            }
            "org.languagetool.rules.es.PostponedAdjectiveConcordanceFilter" => {
                crate::postponed_adjective::accept_es(
                    self.synthesizer.as_deref(),
                    &filter.args,
                    matched_positions,
                    tokens,
                    rm,
                )
            }
            "org.languagetool.rules.fr.PostponedAdjectiveConcordanceFilter" => {
                crate::postponed_adjective::accept_fr(
                    self.synthesizer.as_deref(),
                    &filter.args,
                    matched_positions,
                    tokens,
                    rm,
                )
            }
            c if crate::pt_filters::is_pt_filter(c) => crate::pt_filters::apply_engine_pt_filter(
                rule,
                filter,
                sentence.text(),
                rm,
                crate::pt_filters::PtFilterCall {
                    variant: "pt-PT",
                    args: &filter.args,
                    matched_texts,
                    matched_positions,
                    tokens,
                    regexp_groups: None,
                    synthesizer: self.synthesizer.as_deref(),
                },
            ),
            _ => Some(rm),
        }
    }

    fn parse_filter_args(&self, args: &str, matched_texts: &[String]) -> HashMap<String, String> {
        let mut result = HashMap::new();
        for part in args.split_whitespace() {
            if let Some((key, val)) = part.split_once(':') {
                let resolved = if let Some(rest) = val.strip_prefix('\\') {
                    if let Ok(idx) = rest.parse::<usize>() {
                        if idx > 0 && idx <= matched_texts.len() {
                            matched_texts[idx - 1].clone()
                        } else {
                            val.to_string()
                        }
                    } else {
                        val.to_string()
                    }
                } else if let Ok(idx) = val.parse::<usize>() {
                    if idx > 0 && idx <= matched_texts.len() {
                        matched_texts[idx - 1].clone()
                    } else {
                        val.to_string()
                    }
                } else {
                    val.to_string()
                };
                result.insert(key.to_string(), resolved);
            }
        }
        result
    }

    fn resolve_java_filter_args<'a>(
        &self,
        args: &'a str,
        matched_texts: &[String],
    ) -> HashMap<&'a str, String> {
        let mut result = HashMap::new();
        for part in args.split_whitespace() {
            let Some((key, val)) = part.split_once(':') else {
                continue;
            };
            let resolved = if let Some(rest) = val.strip_prefix('\\')
                && let Ok(idx) = rest.parse::<usize>()
                && idx > 0
                && idx <= matched_texts.len()
            {
                matched_texts[idx - 1].clone()
            } else {
                val.to_string()
            };
            result.insert(key, resolved);
        }
        result
    }

    fn parse_filter_date(
        &self,
        parsed: &HashMap<&str, String>,
        lang: DateLang,
        require_year: bool,
    ) -> Option<(u32, u32, u32)> {
        let year = if let Some(y) = parsed.get("year") {
            y.parse::<u32>().ok()?
        } else if require_year {
            return None;
        } else {
            logic_current_year()
        };
        let month = if let Some(m) = parsed.get("month") {
            if is_numeric_str(m) {
                m.parse::<u32>().ok()?
            } else {
                parse_month_for(lang, &trim_special_chars(m))
            }
        } else if require_year {
            return None;
        } else {
            0
        };
        let day = if let Some(d) = parsed.get("day") {
            parse_day_of_month_for(lang, d)
        } else if require_year {
            return None;
        } else {
            1
        };
        if month == 0 || month > 12 || day > days_in_month(year, month) {
            return None;
        }
        Some(day_zero_rolls_back(year, month, day))
    }

    fn apply_future_date_filter(
        &self,
        args: &str,
        matched_texts: &[String],
        lang: DateLang,
    ) -> bool {
        let parsed = self.resolve_java_filter_args(args, matched_texts);
        let Some((year, month, day)) = self.parse_filter_date(&parsed, lang, true) else {
            return false;
        };
        (year, month, day) > current_date()
    }

    fn apply_date_check_filter(
        &self,
        args: &str,
        matched_texts: &[String],
        lang: DateLang,
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let parsed = self.resolve_java_filter_args(args, matched_texts);
        let week_day_str = parsed.get("weekDay")?.replace('\u{AD}', "");
        let claimed = parse_weekday_for(lang, &week_day_str);
        if claimed == 0 {
            return None;
        }
        let (year, month, day) = self.parse_filter_date(&parsed, lang, false)?;
        let actual = compute_weekday(year, month, day);
        if claimed == actual {
            return None;
        }
        let real_name = localized_day_name(lang, actual);
        let claimed_name = localized_day_name(lang, claimed);
        let current_year = real_current_year().to_string();
        let mut rm = rm;
        let message = rm
            .message()
            .replace("{realDay}", real_name)
            .replace("{day}", claimed_name)
            .replace("{currentYear}", &current_year);
        rm.set_message(message);
        for replacement in rm.replacements_mut() {
            let value = replacement.value().to_string();
            replacement.set_value(
                value
                    .replace("{realDay}", real_name)
                    .replace("{day}", claimed_name)
                    .replace("{currentYear}", &current_year),
            );
        }
        Some(rm)
    }

    fn apply_new_year_date_filter(
        &self,
        args: &str,
        matched_texts: &[String],
        lang: DateLang,
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let parsed = self.resolve_java_filter_args(args, matched_texts);
        self.accept_new_year_date(&parsed, lang, rm)
    }

    fn accept_new_year_date(
        &self,
        parsed: &HashMap<&str, String>,
        lang: DateLang,
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let (year, month, _day) = self.parse_filter_date(parsed, lang, true)?;
        if !logic_is_january() || month == 12 || year + 1 != logic_current_year() {
            return None;
        }
        let real_year = logic_current_year().to_string();
        let year_str = year.to_string();
        let mut rm = rm;
        let message = rm
            .message()
            .replace("{year}", &year_str)
            .replace("{realYear}", &real_year);
        rm.set_message(message);
        for replacement in rm.replacements_mut() {
            let value = replacement.value().to_string();
            replacement.set_value(
                value
                    .replace("{year}", &year_str)
                    .replace("{realYear}", &real_year),
            );
        }
        Some(rm)
    }

    fn apply_ymd_new_year_date_filter(
        &self,
        args: &str,
        matched_texts: &[String],
        lang: DateLang,
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let mut parsed = self.resolve_java_filter_args(args, matched_texts);
        let date = parsed.get("date")?.clone();
        let parts: Vec<&str> = date.split('-').collect();
        if parts.len() != 3 {
            return None;
        }
        let year = parts[0].parse::<u32>().ok()?;
        let real_date = format!("{}-{}-{}", year + 1, parts[1], parts[2]);
        parsed.insert("year", parts[0].to_string());
        parsed.insert("month", parts[1].to_string());
        parsed.insert("day", parts[2].to_string());
        let mut rm = rm;
        let message = rm.message().replace("{realDate}", &real_date);
        rm.set_message(message);
        for replacement in rm.replacements_mut() {
            let value = replacement.value().to_string();
            replacement.set_value(value.replace("{realDate}", &real_date));
        }
        self.accept_new_year_date(&parsed, lang, rm)
    }

    fn apply_date_check_with_suggestions_filter(
        &self,
        lang: DateLang,
        args: &str,
        element_positions: &[usize],
        tokens: &[&AnalyzedTokenReadings],
        base: usize,
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let raw: HashMap<&str, &str> = args
            .split_whitespace()
            .filter_map(|part| part.split_once(':'))
            .collect();
        let arg_pos = |key: &str| -> Option<usize> {
            let value: usize = raw.get(key)?.parse().ok()?;
            if value == 0 || value > element_positions.len() {
                return None;
            }
            Some(element_positions[value - 1])
        };
        let week_day_pos = arg_pos("weekDay")?;
        let mut is_full_date_token = false;
        let (day_pos, _month_pos, year_pos, day_str, month_str, year_str): (
            usize,
            usize,
            Option<usize>,
            String,
            String,
            Option<String>,
        ) = if let Some(full_pos) = arg_pos("date") {
            let parts: Vec<&str> = tokens[full_pos].token().token().split('-').collect();
            if parts.len() != 3 {
                return None;
            }
            is_full_date_token = true;
            (
                full_pos,
                full_pos,
                Some(full_pos),
                parts[2].to_string(),
                parts[1].to_string(),
                Some(parts[0].to_string()),
            )
        } else {
            let d = arg_pos("day")?;
            let m = arg_pos("month")?;
            let y = arg_pos("year");
            (
                d,
                m,
                y,
                tokens[d].token().token().to_string(),
                tokens[m].token().token().to_string(),
                y.map(|p| tokens[p].token().token().to_string()),
            )
        };
        let week_day_str = tokens[week_day_pos].token().token().replace('\u{AD}', "");
        let claimed = parse_weekday_for(lang, &week_day_str);
        if claimed == 0 {
            return None;
        }
        let day = parse_day_of_month_for(lang, &day_str);
        let month = if is_numeric_str(&month_str) {
            month_str.parse::<u32>().ok()?
        } else {
            parse_month_for(lang, &trim_special_chars(&month_str))
        };
        let year = match year_str {
            Some(ref y) => y.parse::<u32>().ok()?,
            None => logic_current_year(),
        };
        if month == 0 || month > 12 || day > days_in_month(year, month) {
            return None;
        }
        let (year, month, day) = day_zero_rolls_back(year, month, day);
        let actual = compute_weekday(year, month, day);
        if claimed == actual {
            return None;
        }
        let logic_year = logic_current_year();
        if claimed == compute_weekday(logic_year, month, day) {
            let year_pos = year_pos?;
            let mut rm = rm;
            let message =
                wrong_year_message(lang).replace("{currentYear}", &logic_year.to_string());
            rm.set_message(message);
            let suggestion = if is_full_date_token {
                format!("{logic_year}-{month_str}-{day_str}")
            } else {
                logic_year.to_string()
            };
            let start = base + tokens[year_pos].token().start();
            let end = base + tokens[year_pos].token().end();
            rm.set_offset_length(start, end - start);
            rm.set_replacements(vec![SuggestedReplacement::new(suggestion)]);
            return Some(rm);
        }
        let start_tok = week_day_pos.min(day_pos);
        let end_tok = week_day_pos.max(day_pos);
        let real_name = localized_day_name(lang, actual);
        let claimed_name = localized_day_name(lang, claimed);
        let current_year = real_current_year().to_string();
        let mut rm = rm;
        let message = rm
            .message()
            .replace("{realDay}", real_name)
            .replace("{day}", claimed_name)
            .replace("{currentYear}", &current_year);
        rm.set_message(message);
        let whitespace_before =
            |j: usize| j > 0 && tokens[j].token().start() > tokens[j - 1].token().end();
        let mut first_suggestion = String::new();
        let mut first = true;
        for (j, tok) in tokens.iter().enumerate().take(end_tok + 1).skip(start_tok) {
            if !first && whitespace_before(j) {
                first_suggestion.push(' ');
            }
            first = false;
            if j == week_day_pos {
                first_suggestion.push_str(&preserve_case(real_name, &week_day_str));
            } else {
                first_suggestion.push_str(tok.token().token());
            }
        }
        let mut replacements = vec![SuggestedReplacement::new(adjust_suggestion(
            lang,
            &first_suggestion,
        ))];
        let mut corrected_day: Option<u32> = None;
        for difference in 1..7u32 {
            if day > difference && compute_weekday(year, month, day - difference) == claimed {
                corrected_day = Some(day - difference);
                break;
            }
            if day + difference < 32 {
                if day + difference > days_in_month(year, month) {
                    return None;
                }
                if compute_weekday(year, month, day + difference) == claimed {
                    corrected_day = Some(day + difference);
                    break;
                }
            }
        }
        if let Some(corrected) = corrected_day {
            let corrected_str = corrected.to_string();
            let mut second_suggestion = String::new();
            let mut first = true;
            for (j, tok) in tokens.iter().enumerate().take(end_tok + 1).skip(start_tok) {
                if !first && whitespace_before(j) {
                    second_suggestion.push(' ');
                }
                first = false;
                if j == day_pos {
                    if is_full_date_token {
                        second_suggestion.push_str(&format!(
                            "{}-{month_str}-{corrected_str}",
                            year_str.as_deref().unwrap_or("")
                        ));
                    } else {
                        second_suggestion.push_str(&day_str_like_original(
                            lang,
                            &corrected_str,
                            &day_str,
                        ));
                    }
                } else {
                    second_suggestion.push_str(tok.token().token());
                }
            }
            if !second_suggestion.is_empty() {
                replacements.push(SuggestedReplacement::new(adjust_suggestion(
                    lang,
                    &second_suggestion,
                )));
            }
        }
        rm.set_replacements(replacements);
        let start = base + tokens[start_tok].token().start();
        let end = base + tokens[end_tok].token().end();
        rm.set_offset_length(start, end - start);
        Some(rm)
    }

    fn apply_recent_year_filter(&self, args: &str, matched_texts: &[String]) -> bool {
        let parsed = self.parse_filter_args(args, matched_texts);
        let this_year = current_date().0;
        let max_year = this_year.saturating_sub(
            parsed
                .get("maxYearsBack")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0),
        );
        let year = parsed
            .get("year")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        year < this_year && year >= max_year
    }

    fn apply_remove_unknown_compounds_filter(
        &self,
        args: &str,
        matched_texts: &[String],
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let Some(spelling) = self.spelling.as_ref() else {
            return Some(rm);
        };
        let parsed = self.parse_filter_args(args, matched_texts);
        let part1 = parsed.get("part1").cloned().unwrap_or_default();
        let part2 = parsed.get("part2").cloned().unwrap_or_default();
        let compound = format!("{}{}", part1, part2.to_lowercase());
        if spelling.is_misspelled(&compound) {
            None
        } else {
            Some(rm)
        }
    }

    fn apply_valid_word_filter(
        &self,
        args: &str,
        matched_texts: &[String],
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let Some(spelling) = self.spelling.as_ref() else {
            return Some(rm);
        };
        let parsed = self.parse_filter_args(args, matched_texts);
        let word1 = parsed.get("word1").cloned().unwrap_or_default();
        let word2 = parsed.get("word2").cloned().unwrap_or_default();
        let joined = format!("{word1}{word2}");
        let joined_lower = format!("{}{}", word1, word2.to_lowercase());
        if !spelling.is_misspelled(&joined) || !spelling.is_misspelled(&joined_lower) {
            None
        } else {
            Some(rm)
        }
    }

    fn apply_insert_comma_filter(
        &self,
        _matched_texts: &[String],
        matched_positions: &[usize],
        tokens: &[&AnalyzedTokenReadings],
        rm: RuleMatch,
    ) -> RuleMatch {
        let tagger = crate::de_comma_tagger::shared();
        let mut suggestions: Vec<String> = Vec::new();
        for replacement in rm.replacements().iter().map(|r| r.value().to_string()) {
            let parts: Vec<&str> = replacement.split_whitespace().collect();
            match parts.len() {
                2 => suggestions.push(format!("{}, {}", parts[0], parts[1])),
                3 => {
                    let t1 = tagger.tags_of(parts[0]);
                    let t2 = tagger.tags_of(parts[1]);
                    let t3 = tagger.tags_of(parts[2]);
                    let has =
                        |tags: &[String], prefix: &str| tags.iter().any(|t| t.starts_with(prefix));
                    if has(&t1, "VER:") && has(&t2, "PRO:PER:") {
                        suggestions.push(format!("{}, {} {}", parts[0], parts[1], parts[2]));
                    } else if is_sagt(parts[0]) && parts[1] == "mal" && has(&t3, "VER:") {
                        suggestions.push(format!("{} {}, {}", parts[0], parts[1], parts[2]));
                    } else if has(&t1, "VER:") && has(&t2, "ADV:") && has(&t3, "VER:") {
                        suggestions.push(format!("{}, {} {}", parts[0], parts[1], parts[2]));
                    }
                }
                4..=7 => {
                    let pattern_token_pos = matched_positions.first().copied().unwrap_or(0);
                    let t1 = tagger.tags_of(parts[0]);
                    let t2 = tagger.tags_of(parts[1]);
                    let t3 = tagger.tags_of(parts[2]);
                    let t4 = tagger.tags_of(parts[3]);
                    let rest1 = parts[1..].join(" ");
                    let has =
                        |tags: &[String], prefix: &str| tags.iter().any(|t| t.starts_with(prefix));
                    let pos0_verbs = tokens
                        .first()
                        .map(|t| {
                            t.readings()
                                .iter()
                                .any(|r| r.pos_tags().iter().any(|g| g.starts_with("VER:")))
                        })
                        .unwrap_or(false);
                    let sent_start_adv = tokens
                        .get(1)
                        .map(|t| {
                            t.readings()
                                .iter()
                                .any(|r| r.pos_tags().iter().any(|g| g.starts_with("ADV:")))
                        })
                        .unwrap_or(false);
                    if pattern_token_pos <= 2 || (pattern_token_pos == 3 && sent_start_adv) {
                        let rest2 = parts[2..].join(" ");
                        if parts.len() == 5
                            && has(&t1, "VER:")
                            && has(&t2, "ART:")
                            && has(&t3, "SUB:")
                            && has(&tagger.tags_of(parts[3]), "SUB:")
                            && has(&tagger.tags_of(parts[4]), "VER:")
                        {
                            suggestions.push(format!(
                                "{} {} {} {},",
                                parts[0], parts[1], parts[2], parts[3]
                            ));
                        } else if (parts.len() == 4 && pos0_verbs && is_der_etc(tokens))
                            || (has(&t1, "VER:") && has(&t2, "PRO:POS:") && has(&t3, "SUB:"))
                        {
                            suggestions.push(format!("{}, {}", parts[0], rest1));
                        } else if has(&t1, "VER:") && has(&t2, "PRO:PER:") && has(&t3, "ADV:INR") {
                            suggestions.push(format!("{} {}, {}", parts[0], parts[1], rest2));
                        } else if (has(&t1, "VER:") && has(&t2, "PRO:POS:") && has(&t3, "ADJ:"))
                            || (is_denke_etc(parts[0]) && has(&t2, "PRO:DEM:") && has(&t3, "SUB:"))
                            || (pattern_token_pos == 1
                                && is_bei_fuer_mit(parts[1])
                                && is_dir_etc(parts[2])
                                && has(&t4, "VER:"))
                        {
                            suggestions.push(format!("{}, {}", parts[0], rest1));
                        }
                    }
                }
                _ => {}
            }
        }
        rm.with_replacements(
            suggestions
                .into_iter()
                .map(og_core::SuggestedReplacement::new)
                .collect(),
        )
    }

    fn apply_potential_compound_filter(
        &self,
        args: &str,
        matched_texts: &[String],
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let Some(spelling) = self.spelling.as_ref() else {
            return Some(rm);
        };
        let parsed = self.parse_filter_args(args, matched_texts);
        let part1 = parsed.get("part1").cloned().unwrap_or_default();
        let part2 = parsed.get("part2").cloned().unwrap_or_default();

        let is_mixed_case = |s: &str| {
            s.chars().any(|c| c.is_uppercase())
                && s.chars().any(|c| c.is_lowercase())
                && !s.chars().all(|c| c.is_uppercase())
        };
        let is_all_upper = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_uppercase());

        let part2lowercase = if !is_mixed_case(&part2) && !is_all_upper(&part2) {
            part2.to_lowercase()
        } else {
            part2.clone()
        };
        let part2capitalized = if !is_mixed_case(&part2) && !is_all_upper(&part2) {
            uppercase_first_char(&part2.to_lowercase())
        } else {
            part2.clone()
        };
        let part1capitalized = if !is_mixed_case(&part1) && !is_all_upper(&part1) {
            uppercase_first_char(&part1.to_lowercase())
        } else {
            part1.clone()
        };

        let joined_word = format!("{part1capitalized}{part2lowercase}");
        let hyphenated_word = format!("{part1capitalized}-{part2capitalized}");

        let mut replacements: Vec<String> = Vec::new();
        if !spelling.is_misspelled(&joined_word) {
            if joined_word.chars().count() > 20 {
                replacements.push(hyphenated_word);
            }
            replacements.push(joined_word);
        } else {
            replacements.push(hyphenated_word);
        }
        if replacements.is_empty() {
            return None;
        }
        Some(
            rm.with_replacements(
                replacements
                    .into_iter()
                    .map(SuggestedReplacement::new)
                    .collect(),
            ),
        )
    }

    fn apply_date_range_filter(&self, args: &str, matched_texts: &[String]) -> bool {
        let parsed = self.parse_filter_args(args, matched_texts);
        let x = parsed
            .get("x")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        let y = parsed
            .get("y")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0);
        x >= y
    }

    fn apply_regex_antipattern_filter(&self, args: &str, matched_texts: &[String]) -> bool {
        let parsed = self.parse_filter_args(args, matched_texts);
        if let Some(patterns) = parsed.get("antipatterns") {
            for pattern in patterns.split('|') {
                if let Ok(re) = regex::Regex::new(pattern) {
                    for text in matched_texts {
                        if re.is_match(text) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    fn apply_apostrophe_type_filter(
        &self,
        args: &str,
        matched_positions: &[usize],
        tokens: &[&AnalyzedTokenReadings],
        rm: &RuleMatch,
    ) -> bool {
        let parsed = self.parse_filter_args(args, &[]);
        let wants_typographic = parsed
            .get("hasTypographicalApostrophe")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let Some(word_from) = parsed.get("wordFrom") else {
            return true;
        };
        let token_idx = if word_from == "marker" {
            matched_positions.iter().copied().find(|&pos| {
                tokens
                    .get(pos)
                    .is_some_and(|t| t.token().start() >= rm.offset())
            })
        } else {
            match word_from.parse::<usize>() {
                Ok(n) if n >= 1 => matched_positions.get(n - 1).copied(),
                _ => None,
            }
        };
        match token_idx.and_then(|pos| tokens.get(pos)) {
            Some(atr) => atr.has_typographic_apostrophe() == wants_typographic,
            None => false,
        }
    }

    fn apply_number_in_word_filter(
        &self,
        args: &str,
        matched_texts: &[String],
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let Some(spelling) = self.spelling.as_ref() else {
            return Some(rm);
        };
        let parsed = self.parse_filter_args(args, matched_texts);
        let Some(word) = parsed.get("word") else {
            return Some(rm);
        };
        let word_replacing_zero_o = word.replace('0', "o");
        let word_without_number: String = word.chars().filter(|c| !c.is_ascii_digit()).collect();

        let mut replacements: Vec<String> = Vec::new();
        if !spelling.is_misspelled(&word_replacing_zero_o) && word != &word_replacing_zero_o {
            replacements.push(word_replacing_zero_o);
        }
        if !spelling.is_misspelled(&word_without_number) {
            replacements.push(word_without_number.clone());
        }
        if replacements.is_empty() {
            replacements = spelling.suggestions(&word_without_number, 10);
        }
        if replacements.is_empty() {
            return None;
        }
        Some(
            rm.with_replacements(
                replacements
                    .into_iter()
                    .map(SuggestedReplacement::new)
                    .collect(),
            ),
        )
    }

    fn apply_find_suggestions_filter(
        &self,
        args: &str,
        matched_texts: &[String],
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        const MAX_SUGGESTIONS: usize = 10;
        let Some(spelling) = self.spelling.as_ref() else {
            return Some(rm);
        };
        let parsed = self.parse_filter_args(args, matched_texts);
        let Some(word_from) = parsed.get("wordFrom") else {
            return Some(rm);
        };
        let Some(desired_postag) = parsed.get("desiredPostag") else {
            return Some(rm);
        };
        let priority_postag = parsed.get("priorityPostag");
        let remove_regexp = parsed.get("removeSuggestionsRegexp");
        let suppress_match = parsed
            .get("suppressMatch")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));
        let diacritics_mode = parsed.get("Mode").is_some_and(|m| m == "diacritics");

        let word = if word_from == "marker" {
            matched_texts.last().cloned().unwrap_or_default()
        } else {
            word_from.clone()
        };
        if word.is_empty() {
            return Some(rm);
        }

        let desired_re = regex::Regex::new(&format!("^(?:{desired_postag})$")).ok();
        let priority_re =
            priority_postag.and_then(|p| regex::Regex::new(&format!("^(?:{p})$")).ok());
        let remove_re = remove_regexp.and_then(|r| regex::Regex::new(&format!("^(?:{r})$")).ok());

        let matches_desired = |tags: &[&str]| match &desired_re {
            Some(re) => tags.iter().any(|t| re.is_match(t)),
            None => tags.iter().any(|t| t == desired_postag),
        };

        let original_readings = spelling.tag(&word);
        let original_tags: Vec<&str> = original_readings
            .iter()
            .flat_map(|t| t.pos_tags().iter().map(String::as_str).collect::<Vec<_>>())
            .collect();
        if matches_desired(&original_tags) && diacritics_mode {
            return None;
        }

        let is_word_capitalized = word.chars().next().is_some_and(|c| c.is_uppercase())
            && !word.chars().skip(1).any(|c| c.is_uppercase());
        let is_word_all_upper = !word.chars().any(|c| c.is_lowercase());

        let mut replacements: Vec<String> = Vec::new();
        let mut used_priority_pos = 0;
        for suggestion in spelling.suggestions(&word.to_lowercase(), 2 * MAX_SUGGESTIONS) {
            if replacements.len() >= 2 * MAX_SUGGESTIONS {
                break;
            }
            if suggestion == word {
                continue;
            }
            let clean = suggestion.to_lowercase();
            let suggestion_readings = spelling.tag(&clean);
            let suggestion_tags: Vec<&str> = suggestion_readings
                .iter()
                .flat_map(|t| t.pos_tags().iter().map(String::as_str).collect::<Vec<_>>())
                .collect();
            if !matches_desired(&suggestion_tags) {
                continue;
            }
            if diacritics_mode && !equal_without_diacritics(&suggestion, &word) {
                continue;
            }
            if remove_re
                .as_ref()
                .is_some_and(|re| re.is_match(&suggestion))
            {
                continue;
            }
            if replacements.contains(&suggestion) || replacements.contains(&clean) {
                continue;
            }
            let mut replacement = suggestion;
            if is_word_all_upper {
                replacement = replacement.to_uppercase();
            }
            if is_word_capitalized {
                replacement = uppercase_first_char(&replacement);
            }
            if priority_re
                .as_ref()
                .is_some_and(|re| suggestion_tags.iter().any(|t| re.is_match(t)))
            {
                replacements.insert(used_priority_pos, replacement);
                used_priority_pos += 1;
            } else {
                replacements.push(replacement);
            }
        }

        let existing: Vec<String> = rm
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect();
        let match_has_finished_suggestion = existing
            .iter()
            .any(|s| !s.to_lowercase().contains("{suggestion}"));
        if replacements.is_empty() && diacritics_mode && !match_has_finished_suggestion {
            return None;
        }
        if replacements.is_empty() && suppress_match && !match_has_finished_suggestion {
            return None;
        }

        let mut definitive: Vec<String> = Vec::new();
        let mut replacements_used = false;
        for s in &existing {
            if s.contains("{suggestion}")
                || s.contains("{Suggestion}")
                || s.contains("{SUGGESTION}")
            {
                replacements_used = true;
                for s2 in &replacements {
                    if definitive.len() >= MAX_SUGGESTIONS {
                        break;
                    }
                    let candidate = if s.contains("{suggestion}") {
                        s.replace("{suggestion}", s2)
                    } else if s.contains("{Suggestion}") {
                        s.replace("{Suggestion}", &uppercase_first_char(s2))
                    } else {
                        s.replace("{SUGGESTION}", &s2.to_uppercase())
                    };
                    if !definitive.contains(&candidate) {
                        definitive.push(candidate);
                    }
                }
            } else if !definitive.contains(s) {
                definitive.push(s.clone());
            }
        }
        if !replacements_used {
            for replacement in replacements {
                if definitive.len() >= MAX_SUGGESTIONS {
                    break;
                }
                if !definitive.contains(&replacement) {
                    definitive.push(replacement);
                }
            }
        }
        Some(
            rm.with_replacements(
                definitive
                    .into_iter()
                    .map(SuggestedReplacement::new)
                    .collect(),
            ),
        )
    }

    fn apply_suppress_misspelled_suggestions_filter(
        &self,
        args: &str,
        matched_texts: &[String],
        mut rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let parsed = self.parse_filter_args(args, matched_texts);
        let suppress_match = parsed
            .get("suppressMatch")
            .is_none_or(|v| !v.eq_ignore_ascii_case("false"));
        let suppress_postag = parsed.get("SuppressPostag").cloned();
        let filter_postag = parsed.get("FilterPostag").cloned();
        let postag_matches = |word: &str, pattern: &str| -> bool {
            let atr = AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(word, 0, 0))
                .with_readings(
                    self.spelling
                        .as_ref()
                        .map(|s| s.tag(word))
                        .unwrap_or_default(),
                );
            atr.has_pos_tag_matching(pattern)
        };
        let mut kept: Vec<og_core::SuggestedReplacement> = Vec::new();
        for repl in rm.replacements() {
            let value = repl.value();
            let misspelled = match self.spelling.as_ref() {
                Some(spelling) => value.split_whitespace().any(|w| spelling.is_misspelled(w)),
                None => false,
            };
            if misspelled {
                continue;
            }
            if let Some(sp) = suppress_postag.as_deref()
                && postag_matches(value, sp)
            {
                continue;
            }
            if let Some(fp) = filter_postag.as_deref()
                && !postag_matches(value, fp)
            {
                continue;
            }
            kept.push(repl.clone());
        }
        if kept.is_empty() && suppress_match {
            None
        } else {
            rm.set_replacements(kept);
            Some(rm)
        }
    }

    fn apply_multitoken_speller_filter(
        &self,
        matched_positions: &[usize],
        tokens: &[&AnalyzedTokenReadings],
        sentence: &AnalyzedSentence,
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let Some(spelling) = self.spelling.as_ref() else {
            return Some(rm);
        };
        let text = sentence.text();
        let start = rm.offset();
        let end = start + rm.length();
        let underlined_error = text.get(start..end).unwrap_or_default();
        if underlined_error.is_empty() {
            return None;
        }
        let are_tokens_accepted_by_speller = spelling.multitoken_checks_tokens_with_speller()
            && !underlined_error
                .split_whitespace()
                .any(|w| spelling.is_misspelled(w));
        let replacements =
            spelling.multitoken_suggestions(underlined_error, are_tokens_accepted_by_speller);
        if replacements.is_empty() {
            return None;
        }
        let replacements = if underlined_error.chars().count() > 4
            && !underlined_error.chars().any(|c| c.is_lowercase())
        {
            let upper: Vec<String> = replacements
                .iter()
                .map(|r| r.to_uppercase())
                .filter(|r| *r != underlined_error)
                .collect();
            let mut deduped: Vec<String> = Vec::with_capacity(upper.len());
            for r in upper {
                if !deduped.contains(&r) {
                    deduped.push(r);
                }
            }
            deduped
        } else {
            let pattern_token_pos = matched_positions.first().copied().unwrap_or(0);
            let mut words_start_pos = 1;
            while words_start_pos < tokens.len() {
                let token_text = tokens[words_start_pos].token().token();
                let is_punct_or_non_word = !token_text.chars().any(|c| c.is_alphanumeric());
                if is_punct_or_non_word {
                    words_start_pos += 1;
                } else {
                    break;
                }
            }
            if pattern_token_pos == words_start_pos {
                let mut capitalized: Vec<String> = Vec::with_capacity(replacements.len());
                for r in replacements {
                    let new_replacement = if r == r.to_lowercase() {
                        uppercase_first_char(&r)
                    } else {
                        r
                    };
                    if !capitalized.contains(&new_replacement)
                        && new_replacement != *underlined_error
                    {
                        capitalized.push(new_replacement);
                    }
                }
                capitalized
            } else {
                replacements
            }
        };
        if replacements.is_empty() {
            return None;
        }
        Some(
            rm.with_replacements(
                replacements
                    .into_iter()
                    .map(SuggestedReplacement::new)
                    .collect(),
            ),
        )
    }

    fn apply_ordinal_suffix_filter(&self, mut rm: RuleMatch) -> Option<RuleMatch> {
        let replacements = rm.replacements().to_vec();
        if let Some(first) = replacements.first() {
            let digits: String = first
                .value()
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            if digits.is_empty() {
                return Some(rm);
            }
            let suffix =
                if digits.ends_with("11") || digits.ends_with("12") || digits.ends_with("13") {
                    "th"
                } else if digits.ends_with('1') {
                    "st"
                } else if digits.ends_with('2') {
                    "nd"
                } else if digits.ends_with('3') {
                    "rd"
                } else {
                    "th"
                };
            let new_suggestion = format!("{}{}", digits, suffix);
            let new_replacements = vec![SuggestedReplacement::new(&new_suggestion)];
            rm = rm.with_replacements(new_replacements);
        }
        Some(rm)
    }

    fn apply_underline_spaces_filter(
        &self,
        args: &str,
        sentence: &AnalyzedSentence,
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let parsed = self.parse_filter_args(args, &[]);
        let mode = parsed
            .get("underlineSpaces")
            .map(|s| s.as_str())
            .unwrap_or("both");
        let text = sentence.text();
        let mut offset = rm.offset();
        let mut end = rm.offset() + rm.length();

        if (mode == "before" || mode == "both")
            && offset > 0
            && text
                .as_bytes()
                .get(offset - 1)
                .map(|&b| b.is_ascii_whitespace())
                .unwrap_or(false)
        {
            offset -= 1;
        }
        if (mode == "after" || mode == "both")
            && end + 1 < text.len()
            && text
                .as_bytes()
                .get(end)
                .map(|&b| b.is_ascii_whitespace())
                .unwrap_or(false)
        {
            end += 1;
        }

        Some(rm.with_offset(offset).with_length(end - offset))
    }

    fn apply_adverb_filter(
        &self,
        args: &str,
        matched_texts: &[String],
        rm: RuleMatch,
    ) -> Option<RuleMatch> {
        let parsed = self.parse_filter_args(args, matched_texts);
        let adverb = parsed.get("adverb").map(|s| s.as_str()).unwrap_or("");
        let noun = parsed.get("noun").map(|s| s.as_str()).unwrap_or("");

        let adjective = adverb_to_adjective(adverb);
        if let Some(adj) = adjective
            && adj != adverb
        {
            let new_suggestion = format!("{} {}", adj, noun);
            return Some(rm.with_replacements(vec![SuggestedReplacement::new(&new_suggestion)]));
        }
        Some(rm)
    }
}

fn adverb_to_adjective(adverb: &str) -> Option<&'static str> {
    Some(match adverb {
        "well" => "good",
        "fast" => "fast",
        "hard" => "hard",
        "late" => "late",
        "early" => "early",
        "daily" => "daily",
        "straight" => "straight",
        "simply" => "simple",
        "cheaply" => "cheap",
        "quickly" => "quick",
        "slowly" => "slow",
        "easily" => "easy",
        "angrily" => "angry",
        "happily" => "happy",
        "luckily" => "lucky",
        "terribly" => "terrible",
        "tragically" => "tragic",
        "economically" => "economic",
        "greatly" => "great",
        "highly" => "high",
        "generally" => "general",
        "differently" => "different",
        "rightly" => "right",
        "largely" => "large",
        "really" => "real",
        "directly" => "direct",
        "clearly" => "clear",
        "merely" => "mere",
        "exactly" => "exact",
        "recently" => "recent",
        "rapidly" => "rapid",
        "suddenly" => "sudden",
        "extremely" => "extreme",
        "properly" => "proper",
        "politically" => "political",
        "probably" => "probable",
        "successfully" => "successful",
        "unusually" => "unusual",
        "obviously" => "obvious",
        "currently" => "current",
        "fully" => "full",
        "accidentally" => "accidental",
        "automatically" => "automatic",
        "completely" => "complete",
        "accurately" => "accurate",
        "necessarily" => "necessary",
        "temporarily" => "temporary",
        "significantly" => "significant",
        "hastily" => "hasty",
        "immediately" => "immediate",
        "rarely" => "rare",
        "totally" => "total",
        "literally" => "literal",
        "gently" => "gentle",
        "finally" => "final",
        "increasingly" => "increasing",
        "considerably" => "considerable",
        "effectively" => "effective",
        "briefly" => "brief",
        "physically" => "physical",
        "enthusiastically" => "enthusiastic",
        "incredibly" => "incredible",
        "permanently" => "permanent",
        "entirely" => "entire",
        "surely" => "sure",
        "positively" => "positive",
        "negatively" => "negative",
        "relatively" => "relative",
        "absolutely" => "absolute",
        "socially" => "social",
        "solely" => "sole",
        "fortunately" => "fortunate",
        "unfortunately" => "unfortunate",
        "ideally" => "ideal",
        "privately" => "private",
        "unreasonably" => "unreasonable",
        "personally" => "personal",
        "basically" => "basic",
        "definitely" => "definite",
        "potentially" => "potential",
        "manually" => "manual",
        "continuously" => "continuous",
        "sadly" => "sad",
        "eventually" => "eventual",
        "possibly" => "possible",
        "visually" => "visual",
        "usually" => "usual",
        "normally" => "normal",
        "steadily" => "steady",
        "actively" => "active",
        "statistically" => "statistical",
        "culturally" => "cultural",
        "vividly" => "vivid",
        "partially" => "partial",
        "seriously" => "serious",
        "verbally" => "verbal",
        "shortly" => "short",
        "mildly" => "mild",
        "secretly" => "secret",
        "especially" => "especial",
        "specially" => "special",
        "previously" => "previous",
        "traditionally" => "traditional",
        "individually" => "individual",
        "carefully" => "careful",
        "essentially" => "essential",
        "originally" => "original",
        "alarmingly" => "alarming",
        "newly" => "new",
        "structurally" => "structural",
        "globally" => "global",
        "seamlessly" => "seamless",
        "sustainably" => "sustainable",
        "coldly" => "cold",
        "densely" => "dense",
        "calmly" => "calm",
        "widely" => "wide",
        "heavily" => "heavy",
        "honestly" => "honest",
        "desperately" => "desperate",
        "apparently" => "apparent",
        "uniquely" => "unique",
        "critically" => "critical",
        "equally" => "equal",
        "strongly" => "strong",
        "thoroughly" => "thorough",
        "technically" => "technical",
        "swiftly" => "swift",
        "occasionally" => "occasional",
        "specifically" => "specific",
        "subtly" => "subtle",
        "actually" => "actual",
        "particularly" => "particular",
        "nicely" => "nice",
        "progressively" => "progressive",
        "genuinely" => "genuine",
        "deeply" => "deep",
        "spiritually" => "spiritual",
        "purely" => "pure",
        "oddly" => "odd",
        "professionally" => "professional",
        "consistently" => "consistent",
        "truly" => "true",
        "commonly" => "common",
        "safely" => "safe",
        "internally" => "internal",
        "annually" => "annual",
        "brightly" => "bright",
        "officially" => "official",
        "perfectly" => "perfect",
        "overly" => "over",
        "brilliantly" => "brilliant",
        "exclusively" => "exclusive",
        "commercially" => "commercial",
        "weirdly" => "weird",
        "routinely" => "routine",
        "naturally" => "natural",
        "lightly" => "light",
        "primarily" => "primary",
        "peacefully" => "peaceful",
        "thankfully" => "thankful",
        "reliably" => "reliable",
        "hugely" => "huge",
        "strictly" => "strict",
        "morally" => "moral",
        "voluntarily" => "voluntary",
        "typically" => "typical",
        "playfully" => "playful",
        "wonderfully" => "wonderful",
        "emotionally" => "emotional",
        "efficiently" => "efficient",
        "mentally" => "mental",
        "credibly" => "credible",
        "rashly" => "rash",
        "financially" => "financial",
        "urgently" => "urgent",
        "historically" => "historical",
        "tightly" => "tight",
        "greedily" => "greedy",
        "fluently" => "fluent",
        "ordinarily" => "ordinary",
        "inevitably" => "inevitable",
        "partly" => "partial",
        "initially" => "initial",
        "justly" => "just",
        "plausibly" => "plausible",
        "massively" => "massive",
        "approximately" => "approximate",
        "extraordinarily" => "extraordinary",
        "warmly" => "warm",
        "nearly" => "near",
        "virtually" => "virtual",
        "regularly" => "regular",
        "deliberately" => "deliberate",
        "reasonably" => "reasonable",
        "similarly" => "similar",
        "flexibly" => "flexible",
        "softly" => "soft",
        "responsibly" => "responsible",
        "sweetly" => "sweet",
        "comfortably" => "comfortable",
        "uncomfortably" => "uncomfortable",
        "undoubtedly" => "undoubted",
        "humanly" => "human",
        "intrinsically" => "intrinsic",
        "substantially" => "substantial",
        "suspiciously" => "suspicious",
        "loudly" => "loud",
        "moderately" => "moderate",
        "gravely" => "grave",
        "digitally" => "digital",
        "finely" => "fine",
        "respectfully" => "respectful",
        "diagonally" => "diagonal",
        "additionally" => "additional",
        "sexually" => "sexual",
        "remarkably" => "remarkable",
        "acutely" => "acute",
        "linearly" => "linear",
        "unbelievably" => "unbelievable",
        "merrily" => "merry",
        "concretely" => "concrete",
        "intuitively" => "intuitive",
        "powerfully" => "powerful",
        "fairly" => "fair",
        "wholly" => "whole",
        "keenly" => "keen",
        "unconsciously" => "unconscious",
        "consciously" => "conscious",
        "humanely" => "humane",
        "rudely" => "rude",
        "incorrectly" => "incorrect",
        "correctly" => "correct",
        "mistakenly" => "mistaken",
        "wrongly" => "wrong",
        "willingly" => "willing",
        "hopefully" => "hopeful",
        "truthfully" => "truthful",
        "attractively" => "attractive",
        "supposedly" => "supposed",
        "imperfectly" => "imperfect",
        "wildly" => "wild",
        "hotly" => "hot",
        "genetically" => "genetic",
        "distinctly" => "distinct",
        "medically" => "medical",
        "certainly" => "certain",
        "beautifully" => "beautiful",
        "firmly" => "firm",
        "gradually" => "gradual",
        "grossly" => "gross",
        "memorably" => "memorable",
        "strangely" => "strange",
        "harshly" => "harsh",
        "proudly" => "proud",
        "politely" => "polite",
        "ethically" => "ethical",
        "noticeably" => "noticeable",
        "mainly" => "main",
        "popularly" => "popular",
        "improperly" => "improper",
        "cruelly" => "cruel",
        "optimally" => "optimal",
        "surprisingly" => "surprising",
        "severely" => "severe",
        "frequently" => "frequent",
        "rationally" => "rational",
        "separately" => "separate",
        "smoothly" => "smooth",
        "indirectly" => "indirect",
        "scientifically" => "scientific",
        "publicly" => "public",
        "tenderly" => "tender",
        "arguably" => "arguable",
        "horribly" => "horrible",
        "figuratively" => "figurative",
        "constantly" => "constant",
        "reluctantly" => "reluctant",
        "externally" => "external",
        "intellectually" => "intellectual",
        "dramatically" => "dramatic",
        "freshly" => "fresh",
        "casually" => "casual",
        "enormously" => "enormous",
        "alternatively" => "alternative",
        "gladly" => "glad",
        "bitterly" => "bitter",
        "faintly" => "faint",
        "humbly" => "humble",
        "promptly" => "prompt",
        "neatly" => "neat",
        "insanely" => "insane",
        "firstly" => "first",
        "philosophically" => "philosophical",
        _ => return None,
    })
}

static TEST_DATE_HACK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_test_date_hack(on: bool) {
    TEST_DATE_HACK.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn test_date_hack() -> bool {
    TEST_DATE_HACK.load(std::sync::atomic::Ordering::Relaxed)
}

fn current_date() -> (u32, u32, u32) {
    if test_date_hack() {
        return (2014, 1, 1);
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let z = days as i64 + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32, m as u32, d as u32)
}

fn parse_month(s: &str) -> u32 {
    let lower = s.to_lowercase();
    match lower.as_str() {
        "1" | "01" => 1,
        "2" | "02" => 2,
        "3" | "03" => 3,
        "4" | "04" => 4,
        "5" | "05" => 5,
        "6" | "06" => 6,
        "7" | "07" => 7,
        "8" | "08" => 8,
        "9" | "09" => 9,
        "10" => 10,
        "11" => 11,
        "12" => 12,
        n if n.starts_with("jan") => 1,
        n if n.starts_with("feb") => 2,
        n if n.starts_with("mar") => 3,
        n if n.starts_with("apr") => 4,
        n if n.starts_with("may") => 5,
        n if n.starts_with("jun") => 6,
        n if n.starts_with("jul") => 7,
        n if n.starts_with("aug") => 8,
        n if n.starts_with("sep") => 9,
        n if n.starts_with("oct") => 10,
        n if n.starts_with("nov") => 11,
        n if n.starts_with("dec") => 12,
        _ => 0,
    }
}

fn parse_month_german(s: &str) -> u32 {
    let lower = s.to_lowercase();
    match lower.as_str() {
        "1" | "01" => 1,
        "2" | "02" => 2,
        "3" | "03" => 3,
        "4" | "04" => 4,
        "5" | "05" => 5,
        "6" | "06" => 6,
        "7" | "07" => 7,
        "8" | "08" => 8,
        "9" | "09" => 9,
        "10" => 10,
        "11" => 11,
        "12" => 12,
        n if n.starts_with("jän") || n.starts_with("jan") => 1,
        n if n.starts_with("feb") => 2,
        n if n.starts_with("mär") => 3,
        n if n.starts_with("apr") => 4,
        n if n.starts_with("mai") => 5,
        n if n.starts_with("jun") => 6,
        n if n.starts_with("jul") => 7,
        n if n.starts_with("aug") => 8,
        n if n.starts_with("sep") => 9,
        n if n.starts_with("okt") => 10,
        n if n.starts_with("nov") => 11,
        n if n.starts_with("dez") => 12,
        _ => 0,
    }
}

fn parse_weekday(s: &str) -> u32 {
    let lower = s.to_lowercase();
    let prefix = if lower.len() >= 2 {
        &lower[..2]
    } else {
        &lower
    };
    match prefix {
        "su" => 1,
        "mo" => 2,
        "tu" => 3,
        "we" => 4,
        "th" => 5,
        "fr" => 6,
        "sa" => 7,
        _ => 0,
    }
}

fn parse_weekday_german(s: &str) -> u32 {
    let lower = s.to_lowercase();
    if lower.starts_with("sonnabend") {
        return 7;
    }
    let prefix = if lower.len() >= 2 {
        &lower[..2]
    } else {
        &lower
    };
    match prefix {
        "so" => 1,
        "mo" => 2,
        "di" => 3,
        "mi" => 4,
        "do" => 5,
        "fr" => 6,
        "sa" => 7,
        _ => 0,
    }
}

fn parse_month_italian(s: &str) -> u32 {
    let lower = s.to_lowercase();
    match lower.as_str() {
        "1" | "01" => 1,
        "2" | "02" => 2,
        "3" | "03" => 3,
        "4" | "04" => 4,
        "5" | "05" => 5,
        "6" | "06" => 6,
        "7" | "07" => 7,
        "8" | "08" => 8,
        "9" | "09" => 9,
        "10" => 10,
        "11" => 11,
        "12" => 12,
        n if n.starts_with("gen") => 1,
        n if n.starts_with("feb") => 2,
        n if n.starts_with("mar") => 3,
        n if n.starts_with("apr") => 4,
        n if n.starts_with("mag") => 5,
        n if n.starts_with("giu") => 6,
        n if n.starts_with("lug") => 7,
        n if n.starts_with("ago") => 8,
        n if n.starts_with("set") => 9,
        n if n.starts_with("ott") => 10,
        n if n.starts_with("nov") => 11,
        n if n.starts_with("dic") => 12,
        _ => 0,
    }
}

fn parse_weekday_italian(s: &str) -> u32 {
    let lower = s.to_lowercase();
    match lower.as_str() {
        n if n.starts_with("do") => 1,
        n if n.starts_with("lu") => 2,
        n if n.starts_with("ma") => 3,
        n if n.starts_with("me") => 4,
        n if n.starts_with("gi") => 5,
        n if n.starts_with("ve") => 6,
        n if n.starts_with("sa") => 7,
        _ => 0,
    }
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn day_zero_rolls_back(year: u32, month: u32, day: u32) -> (u32, u32, u32) {
    if day > 0 {
        return (year, month, day);
    }
    if month == 1 {
        (year.saturating_sub(1), 12, 31)
    } else {
        (year, month - 1, days_in_month(year, month - 1))
    }
}

fn russian_resource_dir() -> Option<std::path::PathBuf> {
    [
        std::path::PathBuf::from(
            "languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru",
        ),
        std::path::PathBuf::from(
            "../languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru",
        ),
        std::path::PathBuf::from(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ru/src/main/resources/org/languagetool/resource/ru",
        ),
    ]
    .into_iter()
    .find(|p| p.join("russian.dict").exists())
}

struct RussianTagCache {
    tagger: Option<og_tagger::GenericTagger>,
    disambiguator: Option<og_tagger::XmlDisambiguator>,
    cache: std::sync::Mutex<HashMap<String, Arc<Vec<AnalyzedTokenReadings>>>>,
}

fn shared_russian_tag_cache() -> &'static RussianTagCache {
    static SHARED: OnceLock<RussianTagCache> = OnceLock::new();
    SHARED.get_or_init(|| {
        let dir = russian_resource_dir();
        let tagger = dir
            .as_ref()
            .map(|dir| og_tagger::GenericTagger::russian(dir));
        let disambiguator = dir.and_then(|dir| {
            std::fs::read_to_string(dir.join("disambiguation.xml"))
                .ok()
                .and_then(|xml| og_tagger::XmlDisambiguator::from_xml(&xml).ok())
        });
        RussianTagCache {
            tagger,
            disambiguator,
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    })
}

fn russian_tag_word(word: &str) -> Arc<Vec<AnalyzedTokenReadings>> {
    let shared = shared_russian_tag_cache();
    if let Some(hit) = shared.cache.lock().unwrap().get(word) {
        return Arc::clone(hit);
    }
    let tagged = shared
        .tagger
        .as_ref()
        .map(|tagger| {
            use og_tagger::Tagger as _;
            let readings = tagger.tag(&[word]);
            if let Some(disambiguator) = shared.disambiguator.as_ref() {
                let mut atr = AnalyzedTokenReadings::new(AnalyzedToken::new(word, 0, word.len()));
                if let Some(first) = readings.first() {
                    atr = first.clone();
                }
                let mut sentence = AnalyzedSentence::new(word, 0, word.len());
                sentence.set_tokens(vec![atr]);
                use og_core::checker::Disambiguator as _;
                disambiguator.disambiguate(&mut sentence);
                sentence.tokens().to_vec()
            } else {
                readings
            }
        })
        .unwrap_or_default();
    let arc = Arc::new(tagged);
    shared
        .cache
        .lock()
        .unwrap()
        .insert(word.to_string(), Arc::clone(&arc));
    arc
}

fn russian_partial_tag_satisfies(parsed: &HashMap<&str, String>, matched_texts: &[String]) -> bool {
    let Ok(token_pos) = parsed
        .get("no")
        .map(String::as_str)
        .unwrap_or_default()
        .parse::<usize>()
    else {
        return false;
    };
    let Some(regexp) = parsed.get("regexp") else {
        return false;
    };
    let Some(required) = parsed.get("postag_regexp") else {
        return false;
    };
    let negate_pos = parsed.contains_key("negate_pos");
    let two_groups = parsed.contains_key("two_groups_regexp");
    let prefix = parsed.get("prefix").map(String::as_str).unwrap_or("");
    let suffix = parsed.get("suffix").map(String::as_str).unwrap_or("");
    let Some(surface) = matched_texts.get(token_pos.wrapping_sub(1)) else {
        return false;
    };
    let token = format!("{prefix}{surface}{suffix}");
    let Ok(re) = regex::Regex::new(&format!("\\A(?:{regexp})\\z")) else {
        return false;
    };
    let Some(caps) = re.captures(&token) else {
        return false;
    };
    let mut partial = caps
        .get(1)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    if two_groups && let Some(second) = caps.get(2) {
        partial.push_str(second.as_str());
    }
    let Ok(required_re) = regex::Regex::new(required) else {
        return false;
    };
    let tags = russian_tag_word(&partial);
    let mut postag_count = 0usize;
    for tag in tags.iter() {
        for reading in tag.readings() {
            for pos in reading.pos_tags() {
                if pos.is_empty() {
                    continue;
                }
                if negate_pos {
                    postag_count += 1;
                    if required_re.is_match(pos) {
                        return false;
                    }
                } else if required_re.is_match(pos) {
                    return true;
                }
            }
        }
    }
    if postag_count == 0 { false } else { negate_pos }
}

fn inn_is_valid(inn: &str) -> bool {
    if !inn.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let d: Vec<u32> = inn.chars().filter_map(|c| c.to_digit(10)).collect();
    match d.len() {
        10 => {
            let kz = (2 * d[0]
                + 4 * d[1]
                + 10 * d[2]
                + 3 * d[3]
                + 5 * d[4]
                + 9 * d[5]
                + 4 * d[6]
                + 6 * d[7]
                + 8 * d[8])
                % 11;
            let kz = if kz > 9 { kz - 10 } else { kz };
            d[9] == kz
        }
        12 => {
            let kz1 = (7 * d[0]
                + 2 * d[1]
                + 4 * d[2]
                + 10 * d[3]
                + 3 * d[4]
                + 5 * d[5]
                + 9 * d[6]
                + 4 * d[7]
                + 6 * d[8]
                + 8 * d[9])
                % 11;
            let kz2 = (3 * d[0]
                + 7 * d[1]
                + 2 * d[2]
                + 4 * d[3]
                + 10 * d[4]
                + 3 * d[5]
                + 5 * d[6]
                + 9 * d[7]
                + 4 * d[8]
                + 6 * d[9]
                + 8 * d[10])
                % 11;
            let kz1 = if kz1 > 9 { kz1 - 10 } else { kz1 };
            let kz2 = if kz2 > 9 { kz2 - 10 } else { kz2 };
            d[10] == kz1 && d[11] == kz2
        }
        _ => true,
    }
}

fn compute_weekday(year: u32, month: u32, day: u32) -> u32 {
    let (y, m) = if month < 3 {
        (year - 1, month + 12)
    } else {
        (year, month)
    };
    let h = (day + (13 * (m + 1)) / 5 + y + y / 4 - y / 100 + y / 400) % 7;
    match h {
        0 => 7,
        1 => 1,
        n => n,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DateLang {
    En,
    De,
    It,
    Fr,
    Es,
    Pt,
    Nl,
    Pl,
    Ru,
    Ar,
    Br,
    Eo,
    Ca,
}

fn trim_special_chars(s: &str) -> String {
    static NOT_KEPT: OnceLock<regex::Regex> = OnceLock::new();
    NOT_KEPT
        .get_or_init(|| regex::Regex::new(r"[^\p{Alphabetic}\p{Nd}\s\p{P}]").unwrap())
        .replace_all(s, "")
        .into_owned()
}

fn parse_month_for(lang: DateLang, s: &str) -> u32 {
    match lang {
        DateLang::En => parse_month(s),
        DateLang::De => parse_month_german(s),
        DateLang::It => parse_month_italian(s),
        DateLang::Fr => parse_month_french(s),
        DateLang::Es => parse_month_spanish(s),
        DateLang::Pt => parse_month_portuguese(s),
        DateLang::Nl => parse_month_dutch(s),
        DateLang::Pl => parse_month_polish(s),
        DateLang::Ru => parse_month_russian(s),
        DateLang::Ar => parse_month_arabic(s),
        DateLang::Br => parse_month_breton(s),
        DateLang::Eo => parse_month_esperanto(s),
        DateLang::Ca => parse_month_catalan(s),
    }
}

fn parse_month_french(s: &str) -> u32 {
    let mon = s.to_lowercase();
    let n = mon.as_str();
    if n.starts_with("jan") {
        1
    } else if n.starts_with("fév") || n.starts_with("fev") {
        2
    } else if n.starts_with("mar") {
        3
    } else if n.starts_with("avr") {
        4
    } else if n.starts_with("mai") {
        5
    } else if n.starts_with("juin") {
        6
    } else if n.starts_with("juil") {
        7
    } else if n.starts_with("aou") || n.starts_with("aoû") {
        8
    } else if n.starts_with("sep") {
        9
    } else if n.starts_with("oct") {
        10
    } else if n.starts_with("nov") {
        11
    } else if n.starts_with("déc") || n.starts_with("dec") {
        12
    } else {
        0
    }
}

fn parse_month_spanish(s: &str) -> u32 {
    let mon = s.to_lowercase();
    let n = mon.as_str();
    if n.starts_with("en") {
        1
    } else if n.starts_with("fe") {
        2
    } else if n.starts_with("mar") || n.starts_with("mzo") {
        3
    } else if n.starts_with("ab") {
        4
    } else if n.starts_with("may") || n.starts_with("my") {
        5
    } else if n.starts_with("jun") || n == "jn" {
        6
    } else if n.starts_with("jul") || n == "jl" {
        7
    } else if n.starts_with("ag") {
        8
    } else if n.starts_with("se") {
        9
    } else if n.starts_with("oc") {
        10
    } else if n.starts_with("no") {
        11
    } else if n.starts_with("di") {
        12
    } else {
        0
    }
}

fn parse_month_portuguese(s: &str) -> u32 {
    let mon = trim_special_chars(s).to_lowercase();
    let n = mon.as_str();
    if n.starts_with("jan") {
        1
    } else if n.starts_with("fev") {
        2
    } else if n.starts_with("mar") {
        3
    } else if n.starts_with("abr") {
        4
    } else if n.starts_with("mai") {
        5
    } else if n.starts_with("jun") {
        6
    } else if n.starts_with("jul") {
        7
    } else if n.starts_with("ago") {
        8
    } else if n.starts_with("set") {
        9
    } else if n.starts_with("out") {
        10
    } else if n.starts_with("nov") {
        11
    } else if n.starts_with("dez") {
        12
    } else {
        0
    }
}

fn parse_month_dutch(s: &str) -> u32 {
    let mon = s.to_lowercase();
    let n = mon.as_str();
    if n.starts_with("jan") {
        1
    } else if n.starts_with("feb") {
        2
    } else if n.starts_with("maa") || n.starts_with("mrt") || n.starts_with("mar") {
        3
    } else if n.starts_with("apr") {
        4
    } else if n.starts_with("mei") {
        5
    } else if n.starts_with("jun") {
        6
    } else if n.starts_with("jul") {
        7
    } else if n.starts_with("aug") {
        8
    } else if n.starts_with("sep") {
        9
    } else if n.starts_with("okt") || n.starts_with("oct") {
        10
    } else if n.starts_with("nov") {
        11
    } else if n.starts_with("dec") {
        12
    } else {
        0
    }
}

fn parse_month_polish(s: &str) -> u32 {
    let mon = s.to_lowercase();
    match (mon.as_str(), s) {
        ("stycznia", _) | (_, "I") => 1,
        ("lutego", _) | (_, "II") => 2,
        ("marca", _) | (_, "III") => 3,
        ("kwietnia", _) | (_, "IV") => 4,
        ("maja", _) | (_, "V") => 5,
        ("czerwca", _) | (_, "VI") => 6,
        ("lipca", _) | (_, "VII") => 7,
        ("sierpnia", _) | (_, "VIII") => 8,
        ("września", _) | (_, "IX") => 9,
        ("października", _) | (_, "X") => 10,
        ("listopada", _) | (_, "XI") => 11,
        ("grudnia", _) | (_, "XII") => 12,
        _ => 0,
    }
}

fn parse_month_russian(s: &str) -> u32 {
    let mon = s.to_lowercase();
    match (mon.as_str(), s) {
        ("январь", _) | ("января", _) | ("янв", _) | (_, "I") => 1,
        ("февраль", _) | ("февраля", _) | ("фев", _) | (_, "II") => 2,
        ("март", _) | ("марта", _) | ("мар", _) | (_, "III") => 3,
        ("апрель", _) | ("апреля", _) | ("апр", _) | (_, "IV") => 4,
        ("май", _) | ("мая", _) | (_, "V") => 5,
        ("июнь", _) | ("июня", _) | ("ин", _) | (_, "VI") => 6,
        ("июль", _) | ("июля", _) | ("ил", _) | (_, "VII") => 7,
        ("август", _) | ("августа", _) | ("авг", _) | (_, "VIII") => 8,
        ("сентябрь", _) | ("сентября", _) | ("сен", _) | (_, "IX") => 9,
        ("октябрь", _) | ("октября", _) | ("окт", _) | (_, "X") => 10,
        ("ноябрь", _) | ("ноября", _) | ("ноя", _) | (_, "XI") => 11,
        ("декабрь", _) | ("декабря", _) | ("дек", _) | (_, "XII") => 12,
        _ => 0,
    }
}

fn parse_month_arabic(s: &str) -> u32 {
    let mon = trim_special_chars(s);
    match mon.as_str() {
        "كانون الثاني" | "كانون ثاني" | "يناير" | "جانفي" | "جانفييه" => {
            1
        }
        "شباط" | "فبراير" | "فيفري" => 2,
        "آذار" | "مارس" => 3,
        "نيسان" | "أبريل" | "أفريل" => 4,
        "أيار" | "مايو" | "ماي" => 5,
        "حزيران" | "يونيو" | "جوان" => 6,
        "تموز" | "يوليو" | "جويلية" => 7,
        "آب" | "أغسطس" | "أوت" => 8,
        "أيلول" | "سبتمبر" => 9,
        "تشرين الأول" | "أكتوبر" => 10,
        "تشرين الثاني" | "تشرين ثاني" | "نوفمبر" => 11,
        "كانون الأول" | "كانون أول" | "ديسمبر" => 12,
        _ => 0,
    }
}

fn parse_month_breton(s: &str) -> u32 {
    let mon = s.to_lowercase();
    match mon.as_str() {
        "genver" => 1,
        "c’hwevrer" => 2,
        "meurzh" => 3,
        "ebrel" => 4,
        "mae" => 5,
        "mezheven" | "even" => 6,
        "gouere" | "gouhere" => 7,
        "eost" => 8,
        "gwengolo" => 9,
        "here" => 10,
        "du" => 11,
        "kerzu" => 12,
        _ => 0,
    }
}

fn parse_weekday_for(lang: DateLang, s: &str) -> u32 {
    match lang {
        DateLang::En => parse_weekday(s),
        DateLang::De => parse_weekday_german(s),
        DateLang::It => parse_weekday_italian(s),
        DateLang::Fr => parse_weekday_french(s),
        DateLang::Es => parse_weekday_spanish(s),
        DateLang::Pt => parse_weekday_portuguese(s),
        DateLang::Nl => parse_weekday_dutch(s),
        DateLang::Pl => parse_weekday_polish(s),
        DateLang::Ru => parse_weekday_russian(s),
        DateLang::Ar => parse_weekday_arabic(s),
        DateLang::Br => parse_weekday_breton(s),
        DateLang::Eo => parse_weekday_esperanto(s),
        DateLang::Ca => parse_weekday_catalan(s),
    }
}

fn parse_weekday_french(s: &str) -> u32 {
    let day = s.to_lowercase();
    let n = day.as_str();
    if n.starts_with("dim") {
        1
    } else if n.starts_with("lun") {
        2
    } else if n.starts_with("mar") {
        3
    } else if n.starts_with("mer") {
        4
    } else if n.starts_with("jeu") {
        5
    } else if n.starts_with("ven") {
        6
    } else if n.starts_with("sam") {
        7
    } else {
        0
    }
}

fn parse_weekday_spanish(s: &str) -> u32 {
    match s.to_lowercase().as_str() {
        "do" | "domingo" => 1,
        "lu" | "lunes" => 2,
        "ma" | "martes" => 3,
        "mi" | "miércoles" => 4,
        "ju" | "jueves" => 5,
        "vi" | "viernes" => 6,
        "sa" | "sábado" => 7,
        _ => 0,
    }
}

fn parse_weekday_portuguese(s: &str) -> u32 {
    let day = trim_special_chars(s).to_lowercase();
    let n = day.as_str();
    if n.starts_with("dom") {
        1
    } else if n.starts_with("seg") {
        2
    } else if n.starts_with("ter") {
        3
    } else if n.starts_with("qua") {
        4
    } else if n.starts_with("qui") {
        5
    } else if n.starts_with("sex") {
        6
    } else if n.starts_with("sáb") {
        7
    } else {
        0
    }
}

fn parse_weekday_dutch(s: &str) -> u32 {
    match s.to_lowercase().as_str() {
        "zo" | "zondag" => 1,
        "ma" | "maandag" => 2,
        "di" | "dinsdag" => 3,
        "wo" | "woensdag" => 4,
        "do" | "donderdag" => 5,
        "vr" | "vrijdag" => 6,
        "za" | "zaterdag" => 7,
        _ => 0,
    }
}

fn parse_weekday_polish(s: &str) -> u32 {
    let day = s.to_lowercase();
    let n = day.as_str();
    if n.starts_with("pon") {
        2
    } else if n.starts_with("wt") {
        3
    } else if n.starts_with("śr") {
        4
    } else if n.starts_with("czw") {
        5
    } else if n == "pt" || n.starts_with("piątk") || n == "piątek" {
        6
    } else if n.starts_with("sob") {
        7
    } else if n.starts_with("niedz") {
        1
    } else {
        0
    }
}

fn parse_weekday_russian(s: &str) -> u32 {
    let day = s.to_lowercase();
    let n = day.as_str();
    if n.starts_with("пн") || n == "понедельник" {
        2
    } else if n.starts_with("вт") {
        3
    } else if n.starts_with("ср") {
        4
    } else if n.starts_with("чт") || n == "четверг" {
        5
    } else if n == "пт" || n.starts_with("пятниц") {
        6
    } else if n.starts_with("сб") || n.starts_with("суббот") {
        7
    } else if n.starts_with("вс") || n == "воскресенье" {
        1
    } else {
        0
    }
}

fn parse_weekday_arabic(s: &str) -> u32 {
    match s {
        "الأحد" => 1,
        "الإثنين" | "الاثنين" => 2,
        "الثلاثاء" => 3,
        "الأربعاء" => 4,
        "الخميس" => 5,
        "الجمعة" => 6,
        "السبت" => 7,
        _ => 0,
    }
}

fn parse_weekday_breton(s: &str) -> u32 {
    let day = s.to_lowercase();
    let n = day.as_str();
    if n.ends_with("sul") {
        1
    } else if n.ends_with("lun") {
        2
    } else if n.ends_with("meurzh") {
        3
    } else if n.ends_with("merc’her") {
        4
    } else if n == "yaou" || n == "diriaou" {
        5
    } else if n.ends_with("gwener") {
        6
    } else if n.ends_with("sadorn") {
        7
    } else {
        0
    }
}

fn localized_day_name(lang: DateLang, weekday: u32) -> &'static str {
    let names = match lang {
        DateLang::En => [
            "",
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
        ],
        DateLang::De => [
            "",
            "Sonntag",
            "Montag",
            "Dienstag",
            "Mittwoch",
            "Donnerstag",
            "Freitag",
            "Samstag",
        ],
        DateLang::It => [
            "",
            "domenica",
            "lunedì",
            "martedì",
            "mercoledì",
            "giovedì",
            "venerdì",
            "sabato",
        ],
        DateLang::Fr => [
            "", "dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi",
        ],
        DateLang::Es => [
            "",
            "domingo",
            "lunes",
            "martes",
            "miércoles",
            "jueves",
            "viernes",
            "sábado",
        ],
        DateLang::Pt => [
            "",
            "domingo",
            "segunda-feira",
            "terça-feira",
            "quarta-feira",
            "quinta-feira",
            "sexta-feira",
            "sábado",
        ],
        DateLang::Nl => [
            "",
            "zondag",
            "maandag",
            "dinsdag",
            "woensdag",
            "donderdag",
            "vrijdag",
            "zaterdag",
        ],
        DateLang::Pl => [
            "",
            "niedziela",
            "poniedziałek",
            "wtorek",
            "środa",
            "czwartek",
            "piątek",
            "sobota",
        ],
        DateLang::Ru => [
            "",
            "воскресенье",
            "понедельник",
            "вторник",
            "среда",
            "четверг",
            "пятница",
            "суббота",
        ],
        DateLang::Ar => [
            "",
            "الأحد",
            "الإثنين",
            "الثلاثاء",
            "الأربعاء",
            "الخميس",
            "الجمعة",
            "السبت",
        ],
        DateLang::Br => [
            "",
            "Sul",
            "Lun",
            "Meurzh",
            "Merc’her",
            "Yaou",
            "Gwener",
            "Sadorn",
        ],
        DateLang::Eo => [
            "", "dimanĉo", "lundo", "mardo", "merkredo", "jaŭdo", "vendredo", "sabato",
        ],
        DateLang::Ca => [
            "",
            "diumenge",
            "dilluns",
            "dimarts",
            "dimecres",
            "dijous",
            "divendres",
            "dissabte",
        ],
    };
    names.get(weekday as usize).copied().unwrap_or("")
}

fn parse_day_of_month_for(lang: DateLang, s: &str) -> u32 {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        return digits.parse().unwrap_or(0);
    }
    if lang == DateLang::Br {
        return parse_day_of_month_breton(s);
    }
    if lang == DateLang::Eo {
        return parse_day_of_month_esperanto(s);
    }
    0
}

fn parse_day_of_month_esperanto(s: &str) -> u32 {
    let mut day = s.to_lowercase();
    if day.ends_with('n') {
        day.pop();
    }
    let mut n = 0u32;
    if let Some(rest) = day.strip_prefix("tridek") {
        n = 30;
        day = rest.to_string();
    } else if let Some(rest) = day.strip_prefix("dudek") {
        n = 20;
        day = rest.to_string();
    } else if let Some(rest) = day.strip_prefix("dek") {
        n = 10;
        day = rest.to_string();
    }
    if n > 0 && day.starts_with('-') {
        day.remove(0);
    }
    match day.as_str() {
        "unua" => n + 1,
        "dua" => n + 2,
        "tria" => n + 3,
        "kvara" => n + 4,
        "kvina" => n + 5,
        "sesa" => n + 6,
        "sepa" => n + 7,
        "oka" => n + 8,
        "naŭa" | "nauxa" | "naua" => n + 9,
        _ => n,
    }
}

fn parse_weekday_esperanto(s: &str) -> u32 {
    let day = s.to_lowercase();
    let n = day.as_str();
    if n.starts_with("dim") {
        1
    } else if n.starts_with("lun") {
        2
    } else if n.starts_with("mar") {
        3
    } else if n.starts_with("mer") {
        4
    } else if n.starts_with("ĵaŭ")
        || n.starts_with("jau")
        || n.starts_with("jhau")
        || n.starts_with("jxau")
    {
        5
    } else if n.starts_with("ven") {
        6
    } else if n.starts_with("sab") {
        7
    } else {
        0
    }
}

fn parse_month_esperanto(s: &str) -> u32 {
    let mon = s.to_lowercase();
    let n = mon.as_str();
    if n.starts_with("jan") {
        1
    } else if n.starts_with("feb") {
        2
    } else if n.starts_with("mar") {
        3
    } else if n.starts_with("apr") {
        4
    } else if n.starts_with("maj") {
        5
    } else if n.starts_with("jun") {
        6
    } else if n.starts_with("jul") {
        7
    } else if n.starts_with("aŭg") {
        8
    } else if n.starts_with("sep") {
        9
    } else if n.starts_with("okt") {
        10
    } else if n.starts_with("nov") {
        11
    } else if n.starts_with("dec") {
        12
    } else {
        0
    }
}

fn parse_day_of_month_breton(s: &str) -> u32 {
    let mut day = s.to_lowercase();
    if day.starts_with('t') {
        day.replace_range(0..1, "d");
    }
    if day.starts_with('p') {
        day.replace_range(0..1, "b");
    }
    let day = day.strip_suffix("vet").unwrap_or(&day);
    match day {
        "c’hentañ" | "unan" => 1,
        "daou" | "eil" => 2,
        "dri" | "drede" | "deir" => 3,
        "bevar" => 4,
        "bemp" | "bem" => 5,
        "c’hwerc’h" => 6,
        "seizh" => 7,
        "eizh" => 8,
        "nav" | "na" => 9,
        "dek" => 10,
        "unnek" => 11,
        "daouzek" => 12,
        "drizek" => 13,
        "bevarzek" => 14,
        "bemzek" => 15,
        "c’hwezek" => 16,
        "seitek" => 17,
        "driwec’h" => 18,
        "naontek" => 19,
        "ugent" => 20,
        "dregont" => 30,
        _ => 0,
    }
}

fn parse_weekday_catalan(s: &str) -> u32 {
    match s.to_lowercase().as_str() {
        "dg" | "diumenge" => 1,
        "dl" | "dilluns" => 2,
        "dt" | "dimarts" => 3,
        "dc" | "dimecres" => 4,
        "dj" | "dijous" => 5,
        "dv" | "divendres" => 6,
        "ds" | "dissabte" => 7,
        _ => 0,
    }
}

fn parse_month_catalan(s: &str) -> u32 {
    let mon = s.to_lowercase();
    let n = mon.as_str();
    if n.starts_with("gen") {
        1
    } else if n.starts_with("febr") {
        2
    } else if n.starts_with("mar\u{e7}") {
        3
    } else if n.starts_with("abr") {
        4
    } else if n.starts_with("maig") {
        5
    } else if n.starts_with("juny") {
        6
    } else if n.starts_with("jul") {
        7
    } else if n.starts_with("ag") {
        8
    } else if n.starts_with("set") {
        9
    } else if n.starts_with("oct") {
        10
    } else if n.starts_with("nov") {
        11
    } else if n.starts_with("des") {
        12
    } else {
        0
    }
}

fn wrong_year_message(lang: DateLang) -> &'static str {
    match lang {
        DateLang::En => "This date is wrong. Did you mean \"{currentYear}\"?",
        DateLang::De => {
            "Dieses Datum stimmt nicht mit dem Tag überein. Meinten Sie \"{currentYear}\"?"
        }
        DateLang::Fr => {
            "Cette date est incorrecte. Faites-vous référence à l'année \"{currentYear}\" ?"
        }
        DateLang::Es => "Este fecha no es correcta. ¿Se refería al año \"{currentYear}\"?",
        DateLang::Pt => {
            "Esta data está incorreta. Você está se referindo ao ano \"{currentYear}\"?"
        }
        DateLang::Nl => "Deze datum is onjuist. Bedoelt u misschien \"{currentYear}\"?",
        DateLang::Ca => "Aquesta data no és correcta. ¿Us referiu a l'any \"{currentYear}\"?",
        _ => "",
    }
}

fn day_str_like_original(lang: DateLang, day: &str, original: &str) -> String {
    if lang != DateLang::En || is_numeric_str(original) {
        return day.to_string();
    }
    let Ok(number) = day.parse::<u32>() else {
        return day.to_string();
    };
    if (11..=13).contains(&number) {
        return format!("{number}th");
    }
    match number % 10 {
        1 => format!("{number}st"),
        2 => format!("{number}nd"),
        3 => format!("{number}rd"),
        _ => format!("{number}th"),
    }
}

fn adjust_suggestion(lang: DateLang, sugg: &str) -> String {
    if lang != DateLang::De {
        return sugg.to_string();
    }
    let dot_comma_pos = sugg.find(".,");
    if let Some(pos) = dot_comma_pos
        && pos > 5
        && pos < 12
    {
        return sugg.replace(".,", ",");
    }
    let comma_pos = sugg.find(',');
    if dot_comma_pos.is_none()
        && let Some(pos) = comma_pos
        && pos > 0
        && pos < 5
    {
        return sugg.replace(",", ".,");
    }
    sugg.to_string()
}

fn real_current_year() -> u32 {
    current_date().0
}

fn logic_current_year() -> u32 {
    if test_date_hack() {
        2014
    } else {
        current_date().0
    }
}

fn logic_is_january() -> bool {
    if test_date_hack() {
        return true;
    }
    current_date().1 == 1
}

fn preserve_case(input: &str, model: &str) -> String {
    if model.is_empty() {
        return input.to_string();
    }
    let is_capitalized = model
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
        && model
            .chars()
            .skip(1)
            .all(|c| !c.is_alphabetic() || c.is_lowercase());
    if is_capitalized {
        let lower: String = input.to_lowercase();
        let mut chars = lower.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    } else if model.chars().all(|c| !c.is_lowercase()) {
        input.to_uppercase()
    } else {
        input.to_string()
    }
}

fn is_numeric_str(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_xml::compiler::XmlCompiler;

    #[test]
    fn esperanto_date_filter_parsers_match_java() {
        assert_eq!(parse_weekday_esperanto("dimanĉon"), 1);
        assert_eq!(parse_weekday_esperanto("lundon"), 2);
        assert_eq!(parse_weekday_esperanto("mardon"), 3);
        assert_eq!(parse_weekday_esperanto("mer"), 4);
        assert_eq!(parse_weekday_esperanto("ĵaŭdon"), 5);
        assert_eq!(parse_weekday_esperanto("jauxdon"), 5);
        assert_eq!(parse_weekday_esperanto("jxaudon"), 5);
        assert_eq!(parse_weekday_esperanto("vendredon"), 6);
        assert_eq!(parse_weekday_esperanto("sabaton"), 7);
        assert_eq!(parse_weekday_esperanto("flaugundo"), 0);

        assert_eq!(parse_month_esperanto("Aŭgusto"), 8);
        assert_eq!(parse_month_esperanto("Oktobro"), 10);
        assert_eq!(parse_month_esperanto("jan"), 1);
        assert_eq!(parse_month_esperanto("decembro"), 12);
        assert_eq!(parse_month_esperanto("februaro"), 2);
        assert_eq!(parse_month_esperanto("marto"), 3);
        assert_eq!(parse_month_esperanto("aprilo"), 4);
        assert_eq!(parse_month_esperanto("majo"), 5);
        assert_eq!(parse_month_esperanto("junio"), 6);
        assert_eq!(parse_month_esperanto("julio"), 7);
        assert_eq!(parse_month_esperanto("septembro"), 9);
        assert_eq!(parse_month_esperanto("novembro"), 11);
        assert_eq!(parse_month_esperanto("x"), 0);

        assert_eq!(parse_day_of_month_for(DateLang::Eo, "28-an"), 28);
        assert_eq!(parse_day_of_month_for(DateLang::Eo, "duan"), 2);
        assert_eq!(parse_day_of_month_for(DateLang::Eo, "dudek-duan"), 22);
        assert_eq!(parse_day_of_month_for(DateLang::Eo, "tridek-unuan"), 31);
        assert_eq!(parse_day_of_month_for(DateLang::Eo, "dek"), 10);
        assert_eq!(parse_day_of_month_for(DateLang::Eo, "naŭan"), 9);
        assert_eq!(parse_day_of_month_for(DateLang::Eo, "nauxan"), 9);

        assert_eq!(compute_weekday(2014, 8, 28), 5);
        assert_eq!(localized_day_name(DateLang::Eo, 5), "jaŭdo");
        assert_eq!(localized_day_name(DateLang::Eo, 1), "dimanĉo");
    }

    #[test]
    fn italian_date_filter_parsers_match_java() {
        assert_eq!(parse_weekday_italian("Domenica"), 1);
        assert_eq!(parse_weekday_italian("Luned\u{ec}"), 2);
        assert_eq!(parse_weekday_italian("lun"), 2);
        assert_eq!(parse_weekday_italian("Lun"), 2);
        assert_eq!(parse_weekday_italian("Lu\u{ec}"), 2);
        assert_eq!(parse_weekday_italian("lu"), 2);
        assert_eq!(parse_weekday_italian("marted\u{ec}"), 3);
        assert_eq!(parse_weekday_italian("ma"), 3);
        assert_eq!(parse_weekday_italian("mercoled\u{ec}"), 4);
        assert_eq!(parse_weekday_italian("me"), 4);
        assert_eq!(parse_weekday_italian("gioved\u{ec}"), 5);
        assert_eq!(parse_weekday_italian("gi"), 5);
        assert_eq!(parse_weekday_italian("venerd\u{ec}"), 6);
        assert_eq!(parse_weekday_italian("ve"), 6);
        assert_eq!(parse_weekday_italian("sabato"), 7);
        assert_eq!(parse_weekday_italian("sa"), 7);
        assert_eq!(parse_weekday_italian("do"), 1);
        assert_eq!(parse_weekday_italian("Dom."), 1);
        assert_eq!(parse_weekday_italian("x"), 0);
        assert_eq!(parse_weekday_italian(""), 0);

        assert_eq!(parse_month_italian("Ottobre"), 10);
        assert_eq!(parse_month_italian("ott"), 10);
        assert_eq!(parse_month_italian("settembre"), 9);
        assert_eq!(parse_month_italian("dic"), 12);
        assert_eq!(parse_month_italian("07"), 7);
    }

    #[test]
    fn catalan_date_filter_parsers_match_java() {
        assert_eq!(parse_weekday_catalan("Diumenge"), 1);
        assert_eq!(parse_weekday_catalan("dg"), 1);
        assert_eq!(parse_weekday_catalan("Dilluns"), 2);
        assert_eq!(parse_weekday_catalan("dl"), 2);
        assert_eq!(parse_weekday_catalan("Dimarts"), 3);
        assert_eq!(parse_weekday_catalan("dt"), 3);
        assert_eq!(parse_weekday_catalan("Dimecres"), 4);
        assert_eq!(parse_weekday_catalan("dc"), 4);
        assert_eq!(parse_weekday_catalan("Dijous"), 5);
        assert_eq!(parse_weekday_catalan("dj"), 5);
        assert_eq!(parse_weekday_catalan("Divendres"), 6);
        assert_eq!(parse_weekday_catalan("dv"), 6);
        assert_eq!(parse_weekday_catalan("Dissabte"), 7);
        assert_eq!(parse_weekday_catalan("ds"), 7);
        assert_eq!(parse_weekday_catalan("dium"), 0);
        assert_eq!(parse_weekday_catalan("divendres "), 0);
        assert_eq!(parse_weekday_catalan("x"), 0);

        assert_eq!(parse_month_catalan("Octubre"), 10);
        assert_eq!(parse_month_catalan("oct"), 10);
        assert_eq!(parse_month_catalan("d'agost"), 0);
        assert_eq!(parse_month_catalan("agost"), 8);
        assert_eq!(parse_month_catalan("ag"), 8);
        assert_eq!(parse_month_catalan("juliol"), 7);
        assert_eq!(parse_month_catalan("jul"), 7);
        assert_eq!(parse_month_catalan("setembre"), 9);
        assert_eq!(parse_month_catalan("desembre"), 12);
        assert_eq!(parse_month_catalan("mar\u{e7}"), 3);
        assert_eq!(parse_month_catalan("mar"), 0);
        assert_eq!(parse_month_catalan("x"), 0);
        assert_eq!(parse_month_catalan("gener"), 1);
        assert_eq!(parse_month_catalan("febrer"), 2);
        assert_eq!(parse_month_catalan("abril"), 4);
        assert_eq!(parse_month_catalan("maig"), 5);
        assert_eq!(parse_month_catalan("juny"), 6);
        assert_eq!(parse_month_catalan("novembre"), 11);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "x"), 0);

        assert_eq!(compute_weekday(2014, 10, 7), 3);
        assert_eq!(localized_day_name(DateLang::Ca, 3), "dimarts");
        assert_eq!(localized_day_name(DateLang::Ca, 6), "divendres");
        assert_eq!(
            wrong_year_message(DateLang::Ca),
            "Aquesta data no \u{e9}s correcta. \u{bf}Us referiu a l'any \"{currentYear}\"?"
        );
    }

    #[test]
    fn java_replacement_disambiguates_group_ref_from_literal() {
        assert_eq!(java_replacement("$1P$2").as_ref(), "${1}P$2");
        assert_eq!(java_replacement("$1").as_ref(), "$1");
        assert_eq!(java_replacement("no refs").as_ref(), "no refs");
        assert_eq!(java_replacement("$12x").as_ref(), "${12}x");
        let re = regex::Regex::new("(A...|NC.|V.P..)S(.*)").unwrap();
        assert_eq!(
            re.replace_all("NCFS000", java_replacement("$1P$2").as_ref())
                .as_ref(),
            "NCFP000"
        );
    }

    #[test]
    fn french_date_filter_parsers_match_java() {
        assert_eq!(parse_month_for(DateLang::Fr, "janvier"), 1);
        assert_eq!(parse_month_for(DateLang::Fr, "Février"), 2);
        assert_eq!(parse_month_for(DateLang::Fr, "fevr"), 2);
        assert_eq!(parse_month_for(DateLang::Fr, "mars"), 3);
        assert_eq!(parse_month_for(DateLang::Fr, "avril"), 4);
        assert_eq!(parse_month_for(DateLang::Fr, "mai"), 5);
        assert_eq!(parse_month_for(DateLang::Fr, "juin"), 6);
        assert_eq!(parse_month_for(DateLang::Fr, "juillet"), 7);
        assert_eq!(parse_month_for(DateLang::Fr, "aout"), 8);
        assert_eq!(parse_month_for(DateLang::Fr, "août"), 8);
        assert_eq!(parse_month_for(DateLang::Fr, "septembre"), 9);
        assert_eq!(parse_month_for(DateLang::Fr, "octobre"), 10);
        assert_eq!(parse_month_for(DateLang::Fr, "novembre"), 11);
        assert_eq!(parse_month_for(DateLang::Fr, "décembre"), 12);
        assert_eq!(parse_month_for(DateLang::Fr, "decembre"), 12);
        assert_eq!(parse_month_for(DateLang::Fr, "jui"), 0);
        assert_eq!(parse_month_french("juil"), 7);
        assert_eq!(parse_month_french("xyz"), 0);

        assert_eq!(parse_weekday_for(DateLang::Fr, "dimanche"), 1);
        assert_eq!(parse_weekday_for(DateLang::Fr, "lundi"), 2);
        assert_eq!(parse_weekday_for(DateLang::Fr, "mardi"), 3);
        assert_eq!(parse_weekday_for(DateLang::Fr, "mercredi"), 4);
        assert_eq!(parse_weekday_for(DateLang::Fr, "jeudi"), 5);
        assert_eq!(parse_weekday_for(DateLang::Fr, "vendredi"), 6);
        assert_eq!(parse_weekday_for(DateLang::Fr, "samedi"), 7);
        assert_eq!(parse_weekday_french("octave"), 0);
    }

    #[test]
    fn spanish_date_filter_parsers_match_java() {
        assert_eq!(parse_month_for(DateLang::Es, "enero"), 1);
        assert_eq!(parse_month_for(DateLang::Es, "febrero"), 2);
        assert_eq!(parse_month_for(DateLang::Es, "marzo"), 3);
        assert_eq!(parse_month_for(DateLang::Es, "mzo"), 3);
        assert_eq!(parse_month_for(DateLang::Es, "abril"), 4);
        assert_eq!(parse_month_for(DateLang::Es, "mayo"), 5);
        assert_eq!(parse_month_for(DateLang::Es, "my"), 5);
        assert_eq!(parse_month_for(DateLang::Es, "junio"), 6);
        assert_eq!(parse_month_for(DateLang::Es, "jn"), 6);
        assert_eq!(parse_month_for(DateLang::Es, "julio"), 7);
        assert_eq!(parse_month_for(DateLang::Es, "jl"), 7);
        assert_eq!(parse_month_for(DateLang::Es, "agosto"), 8);
        assert_eq!(parse_month_for(DateLang::Es, "septiembre"), 9);
        assert_eq!(parse_month_for(DateLang::Es, "octubre"), 10);
        assert_eq!(parse_month_for(DateLang::Es, "noviembre"), 11);
        assert_eq!(parse_month_for(DateLang::Es, "diciembre"), 12);
        assert_eq!(parse_month_spanish("jun"), 6);
        assert_eq!(parse_month_spanish("jul"), 7);
        assert_eq!(parse_month_spanish("ju"), 0);

        assert_eq!(parse_weekday_for(DateLang::Es, "do"), 1);
        assert_eq!(parse_weekday_for(DateLang::Es, "domingo"), 1);
        assert_eq!(parse_weekday_for(DateLang::Es, "lu"), 2);
        assert_eq!(parse_weekday_for(DateLang::Es, "lunes"), 2);
        assert_eq!(parse_weekday_for(DateLang::Es, "ma"), 3);
        assert_eq!(parse_weekday_for(DateLang::Es, "martes"), 3);
        assert_eq!(parse_weekday_for(DateLang::Es, "mi"), 4);
        assert_eq!(parse_weekday_for(DateLang::Es, "miércoles"), 4);
        assert_eq!(parse_weekday_for(DateLang::Es, "ju"), 5);
        assert_eq!(parse_weekday_for(DateLang::Es, "jueves"), 5);
        assert_eq!(parse_weekday_for(DateLang::Es, "vi"), 6);
        assert_eq!(parse_weekday_for(DateLang::Es, "viernes"), 6);
        assert_eq!(parse_weekday_for(DateLang::Es, "sa"), 7);
        assert_eq!(parse_weekday_for(DateLang::Es, "sábado"), 7);
        assert_eq!(parse_weekday_spanish("xd"), 0);
    }

    #[test]
    fn portuguese_date_filter_parsers_match_java() {
        assert_eq!(parse_month_for(DateLang::Pt, "janeiro"), 1);
        assert_eq!(parse_month_for(DateLang::Pt, "fevereiro"), 2);
        assert_eq!(parse_month_for(DateLang::Pt, "março"), 3);
        assert_eq!(parse_month_for(DateLang::Pt, "abril"), 4);
        assert_eq!(parse_month_for(DateLang::Pt, "maio"), 5);
        assert_eq!(parse_month_for(DateLang::Pt, "junho"), 6);
        assert_eq!(parse_month_for(DateLang::Pt, "julho"), 7);
        assert_eq!(parse_month_for(DateLang::Pt, "agosto"), 8);
        assert_eq!(parse_month_for(DateLang::Pt, "setembro"), 9);
        assert_eq!(parse_month_for(DateLang::Pt, "outubro"), 10);
        assert_eq!(parse_month_for(DateLang::Pt, "novembro"), 11);
        assert_eq!(parse_month_for(DateLang::Pt, "dezembro"), 12);
        assert_eq!(
            parse_month_portuguese("x\u{AD}yz"),
            0,
            "soft hyphen trimmed"
        );
        assert_eq!(parse_month_portuguese("nada"), 0);

        assert_eq!(parse_weekday_for(DateLang::Pt, "domingo"), 1);
        assert_eq!(parse_weekday_for(DateLang::Pt, "segunda"), 2);
        assert_eq!(parse_weekday_for(DateLang::Pt, "terça"), 3);
        assert_eq!(parse_weekday_for(DateLang::Pt, "quarta"), 4);
        assert_eq!(parse_weekday_for(DateLang::Pt, "quinta"), 5);
        assert_eq!(parse_weekday_for(DateLang::Pt, "sexta"), 6);
        assert_eq!(parse_weekday_for(DateLang::Pt, "sábado"), 7);
        assert_eq!(parse_weekday_portuguese("folded"), 0);
    }

    #[test]
    fn dutch_date_filter_parsers_match_java() {
        assert_eq!(parse_month_for(DateLang::Nl, "januari"), 1);
        assert_eq!(parse_month_for(DateLang::Nl, "februari"), 2);
        assert_eq!(parse_month_for(DateLang::Nl, "maart"), 3);
        assert_eq!(parse_month_for(DateLang::Nl, "mrt"), 3);
        assert_eq!(parse_month_for(DateLang::Nl, "maa"), 3);
        assert_eq!(parse_month_for(DateLang::Nl, "april"), 4);
        assert_eq!(parse_month_for(DateLang::Nl, "mei"), 5);
        assert_eq!(parse_month_for(DateLang::Nl, "juni"), 6);
        assert_eq!(parse_month_for(DateLang::Nl, "juli"), 7);
        assert_eq!(parse_month_for(DateLang::Nl, "augustus"), 8);
        assert_eq!(parse_month_for(DateLang::Nl, "september"), 9);
        assert_eq!(parse_month_for(DateLang::Nl, "oktober"), 10);
        assert_eq!(parse_month_dutch("oct"), 10);
        assert_eq!(parse_month_for(DateLang::Nl, "november"), 11);
        assert_eq!(parse_month_for(DateLang::Nl, "december"), 12);
        assert_eq!(parse_month_dutch("mrt"), 3);
        assert_eq!(parse_month_dutch("geen"), 0);

        assert_eq!(parse_weekday_for(DateLang::Nl, "zo"), 1);
        assert_eq!(parse_weekday_for(DateLang::Nl, "zondag"), 1);
        assert_eq!(parse_weekday_for(DateLang::Nl, "ma"), 2);
        assert_eq!(parse_weekday_for(DateLang::Nl, "maandag"), 2);
        assert_eq!(parse_weekday_for(DateLang::Nl, "di"), 3);
        assert_eq!(parse_weekday_for(DateLang::Nl, "dinsdag"), 3);
        assert_eq!(parse_weekday_for(DateLang::Nl, "wo"), 4);
        assert_eq!(parse_weekday_for(DateLang::Nl, "woensdag"), 4);
        assert_eq!(parse_weekday_for(DateLang::Nl, "do"), 5);
        assert_eq!(parse_weekday_for(DateLang::Nl, "donderdag"), 5);
        assert_eq!(parse_weekday_for(DateLang::Nl, "vr"), 6);
        assert_eq!(parse_weekday_for(DateLang::Nl, "vrijdag"), 6);
        assert_eq!(parse_weekday_for(DateLang::Nl, "za"), 7);
        assert_eq!(parse_weekday_for(DateLang::Nl, "zaterdag"), 7);
        assert_eq!(parse_weekday_dutch("zomaar"), 0);
    }

    #[test]
    fn polish_date_filter_parsers_match_java() {
        for (name, roman, num) in [
            ("stycznia", "I", 1),
            ("lutego", "II", 2),
            ("marca", "III", 3),
            ("kwietnia", "IV", 4),
            ("maja", "V", 5),
            ("czerwca", "VI", 6),
            ("lipca", "VII", 7),
            ("sierpnia", "VIII", 8),
            ("września", "IX", 9),
            ("października", "X", 10),
            ("listopada", "XI", 11),
            ("grudnia", "XII", 12),
        ] {
            assert_eq!(parse_month_for(DateLang::Pl, name), num);
            assert_eq!(parse_month_for(DateLang::Pl, roman), num, "{roman}");
        }
        assert_eq!(parse_month_polish("STYCZNIA"), 1);
        assert_eq!(parse_month_polish("v"), 0);
        assert_eq!(parse_month_polish("nic"), 0);

        assert_eq!(parse_weekday_for(DateLang::Pl, "poniedziałek"), 2);
        assert_eq!(parse_weekday_for(DateLang::Pl, "wtorek"), 3);
        assert_eq!(parse_weekday_for(DateLang::Pl, "środa"), 4);
        assert_eq!(parse_weekday_for(DateLang::Pl, "czwartek"), 5);
        assert_eq!(parse_weekday_for(DateLang::Pl, "piątek"), 6);
        assert_eq!(parse_weekday_for(DateLang::Pl, "sobota"), 7);
        assert_eq!(parse_weekday_for(DateLang::Pl, "niedziela"), 1);
        assert_eq!(parse_weekday_polish("piątku"), 6);
        assert_eq!(parse_weekday_polish("xd"), 0);
    }

    #[test]
    fn russian_date_filter_parsers_match_java() {
        for (name, num) in [
            ("январь", 1),
            ("января", 1),
            ("февраль", 2),
            ("февраля", 2),
            ("март", 3),
            ("марта", 3),
            ("апрель", 4),
            ("апреля", 4),
            ("май", 5),
            ("мая", 5),
            ("июнь", 6),
            ("июня", 6),
            ("июль", 7),
            ("июля", 7),
            ("август", 8),
            ("августа", 8),
            ("сентябрь", 9),
            ("сентября", 9),
            ("октябрь", 10),
            ("октября", 10),
            ("ноябрь", 11),
            ("ноября", 11),
            ("декабрь", 12),
            ("декабря", 12),
        ] {
            assert_eq!(parse_month_for(DateLang::Ru, name), num);
        }
        assert_eq!(parse_month_for(DateLang::Ru, "IX"), 9);
        assert_eq!(parse_month_russian("январе"), 0);
        assert_eq!(parse_month_russian("нет"), 0);

        assert_eq!(parse_weekday_for(DateLang::Ru, "понедельник"), 2);
        assert_eq!(parse_weekday_for(DateLang::Ru, "пн"), 2);
        assert_eq!(parse_weekday_for(DateLang::Ru, "вт"), 3);
        assert_eq!(parse_weekday_for(DateLang::Ru, "ср"), 4);
        assert_eq!(parse_weekday_for(DateLang::Ru, "четверг"), 5);
        assert_eq!(parse_weekday_for(DateLang::Ru, "чт"), 5);
        assert_eq!(parse_weekday_for(DateLang::Ru, "пт"), 6);
        assert_eq!(parse_weekday_for(DateLang::Ru, "пятница"), 6);
        assert_eq!(parse_weekday_for(DateLang::Ru, "сб"), 7);
        assert_eq!(parse_weekday_for(DateLang::Ru, "суббота"), 7);
        assert_eq!(parse_weekday_for(DateLang::Ru, "вс"), 1);
        assert_eq!(parse_weekday_for(DateLang::Ru, "воскресенье"), 1);
        assert_eq!(parse_weekday_russian("нет"), 0);
    }

    #[test]
    fn arabic_date_filter_parsers_match_java() {
        for (name, num) in [
            ("كانون الثاني", 1),
            ("كانون ثاني", 1),
            ("يناير", 1),
            ("جانفي", 1),
            ("جانفييه", 1),
            ("شباط", 2),
            ("فبراير", 2),
            ("فيفري", 2),
            ("آذار", 3),
            ("مارس", 3),
            ("نيسان", 4),
            ("أبريل", 4),
            ("أفريل", 4),
            ("أيار", 5),
            ("مايو", 5),
            ("ماي", 5),
            ("حزيران", 6),
            ("يونيو", 6),
            ("جوان", 6),
            ("تموز", 7),
            ("يوليو", 7),
            ("جويلية", 7),
            ("آب", 8),
            ("أغسطس", 8),
            ("أوت", 8),
            ("أيلول", 9),
            ("سبتمبر", 9),
            ("تشرين الأول", 10),
            ("أكتوبر", 10),
            ("تشرين الثاني", 11),
            ("تشرين ثاني", 11),
            ("نوفمبر", 11),
            ("كانون الأول", 12),
            ("كانون أول", 12),
            ("ديسمبر", 12),
        ] {
            assert_eq!(parse_month_for(DateLang::Ar, name), num);
        }
        assert_eq!(parse_month_arabic("ليس"), 0);

        assert_eq!(parse_weekday_for(DateLang::Ar, "الأحد"), 1);
        assert_eq!(parse_weekday_for(DateLang::Ar, "الإثنين"), 2);
        assert_eq!(parse_weekday_for(DateLang::Ar, "الاثنين"), 2);
        assert_eq!(parse_weekday_for(DateLang::Ar, "الثلاثاء"), 3);
        assert_eq!(parse_weekday_for(DateLang::Ar, "الأربعاء"), 4);
        assert_eq!(parse_weekday_for(DateLang::Ar, "الخميس"), 5);
        assert_eq!(parse_weekday_for(DateLang::Ar, "الجمعة"), 6);
        assert_eq!(parse_weekday_for(DateLang::Ar, "السبت"), 7);
        assert_eq!(parse_weekday_arabic("لا"), 0);
    }

    #[test]
    fn breton_date_filter_parsers_match_java() {
        for (name, num) in [
            ("genver", 1),
            ("c’hwevrer", 2),
            ("meurzh", 3),
            ("ebrel", 4),
            ("mae", 5),
            ("mezheven", 6),
            ("even", 6),
            ("gouere", 7),
            ("gouhere", 7),
            ("eost", 8),
            ("gwengolo", 9),
            ("here", 10),
            ("du", 11),
            ("kerzu", 12),
        ] {
            assert_eq!(parse_month_for(DateLang::Br, name), num);
        }
        assert_eq!(parse_month_breton("ket"), 0);

        assert_eq!(parse_weekday_for(DateLang::Br, "Sul"), 1);
        assert_eq!(parse_weekday_for(DateLang::Br, "Disul"), 1);
        assert_eq!(parse_weekday_for(DateLang::Br, "Lun"), 2);
        assert_eq!(parse_weekday_for(DateLang::Br, "Dilun"), 2);
        assert_eq!(parse_weekday_for(DateLang::Br, "Meurzh"), 3);
        assert_eq!(parse_weekday_for(DateLang::Br, "Merc’her"), 4);
        assert_eq!(parse_weekday_for(DateLang::Br, "Yaou"), 5);
        assert_eq!(parse_weekday_for(DateLang::Br, "diriaou"), 5);
        assert_eq!(parse_weekday_for(DateLang::Br, "Gwener"), 6);
        assert_eq!(parse_weekday_for(DateLang::Br, "Sadorn"), 7);
        assert_eq!(parse_weekday_breton("ket"), 0);

        assert_eq!(parse_day_of_month_for(DateLang::Br, "7"), 7);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "unan"), 1);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "c’hentañ"), 1);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "eil"), 2);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "drede"), 3);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "tri"), 3);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "pevar"), 4);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "pemp"), 5);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "c’hwerc’h"), 6);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "seizh"), 7);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "eizh"), 8);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "nav"), 9);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "dek"), 10);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "unnek"), 11);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "daouzek"), 12);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "drizek"), 13);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "bevarzek"), 14);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "pemzek"), 15);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "c’hwezek"), 16);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "seitek"), 17);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "driwec’h"), 18);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "naontek"), 19);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "ugent"), 20);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "tregont"), 30);
        assert_eq!(parse_day_of_month_for(DateLang::Br, "tregontvet"), 30);
        assert_eq!(parse_day_of_month_for(DateLang::En, "x"), 0);
        assert_eq!(parse_day_of_month_for(DateLang::De, "vier"), 0);
    }

    #[test]
    fn english_german_and_italian_date_filter_parsers_match_java() {
        assert_eq!(parse_month_for(DateLang::En, "September"), 9);
        assert_eq!(parse_month_for(DateLang::En, "sep"), 9);
        assert_eq!(parse_month_for(DateLang::En, "12"), 12);
        assert_eq!(parse_month_for(DateLang::De, "September"), 9);
        assert_eq!(parse_month_for(DateLang::De, "sep"), 9);
        assert_eq!(parse_month_for(DateLang::It, "settembre"), 9);
        assert_eq!(parse_month_for(DateLang::Eo, "aŭgusto"), 8);
        assert_eq!(parse_month_for(DateLang::Ca, "octubre"), 10);

        assert_eq!(parse_weekday_for(DateLang::En, "Sunday"), 1);
        assert_eq!(parse_weekday_for(DateLang::En, "monday"), 2);
        assert_eq!(parse_weekday_for(DateLang::De, "Sonntag"), 1);
        assert_eq!(parse_weekday_for(DateLang::De, "Sonnabend"), 7);
        assert_eq!(parse_weekday_for(DateLang::De, "Dienstag"), 3);
        assert_eq!(parse_weekday_german("M"), 0);
        assert_eq!(parse_weekday_for(DateLang::It, "domenica"), 1);
        assert_eq!(parse_weekday_for(DateLang::Es, "lunes"), 2);
        assert_eq!(parse_weekday_for(DateLang::Eo, "lundo"), 2);
        assert_eq!(parse_weekday_for(DateLang::Ca, "dilluns"), 2);
    }

    #[test]
    fn localized_day_names_and_wrong_year_messages_match_java() {
        assert_eq!(localized_day_name(DateLang::En, 1), "Sunday");
        assert_eq!(localized_day_name(DateLang::En, 7), "Saturday");
        assert_eq!(localized_day_name(DateLang::De, 1), "Sonntag");
        assert_eq!(localized_day_name(DateLang::De, 3), "Dienstag");
        assert_eq!(localized_day_name(DateLang::It, 2), "lunedì");
        assert_eq!(localized_day_name(DateLang::Fr, 4), "mercredi");
        assert_eq!(localized_day_name(DateLang::Es, 5), "jueves");
        assert_eq!(localized_day_name(DateLang::Pt, 6), "sexta-feira");
        assert_eq!(localized_day_name(DateLang::Nl, 7), "zaterdag");
        assert_eq!(localized_day_name(DateLang::Pl, 2), "poniedziałek");
        assert_eq!(localized_day_name(DateLang::Ru, 3), "вторник");
        assert_eq!(localized_day_name(DateLang::Ar, 4), "الأربعاء");
        assert_eq!(localized_day_name(DateLang::Br, 5), "Yaou");
        assert_eq!(localized_day_name(DateLang::Eo, 6), "vendredo");
        assert_eq!(localized_day_name(DateLang::Ca, 7), "dissabte");
        assert_eq!(localized_day_name(DateLang::En, 0), "");
        assert_eq!(localized_day_name(DateLang::En, 8), "");

        assert_eq!(
            wrong_year_message(DateLang::En),
            "This date is wrong. Did you mean \"{currentYear}\"?"
        );
        assert_eq!(
            wrong_year_message(DateLang::De),
            "Dieses Datum stimmt nicht mit dem Tag überein. Meinten Sie \"{currentYear}\"?"
        );
        assert_eq!(
            wrong_year_message(DateLang::Fr),
            "Cette date est incorrecte. Faites-vous référence à l'année \"{currentYear}\" ?"
        );
        assert_eq!(
            wrong_year_message(DateLang::Es),
            "Este fecha no es correcta. ¿Se refería al año \"{currentYear}\"?"
        );
        assert_eq!(
            wrong_year_message(DateLang::Pt),
            "Esta data está incorreta. Você está se referindo ao ano \"{currentYear}\"?"
        );
        assert_eq!(
            wrong_year_message(DateLang::Nl),
            "Deze datum is onjuist. Bedoelt u misschien \"{currentYear}\"?"
        );
        assert_eq!(wrong_year_message(DateLang::It), "");
        assert_eq!(wrong_year_message(DateLang::Ru), "");
    }

    #[test]
    fn suggestion_helpers_match_java_date_filter_behavior() {
        assert_eq!(day_str_like_original(DateLang::En, "1", "1st"), "1st");
        assert_eq!(day_str_like_original(DateLang::En, "2", "2nd"), "2nd");
        assert_eq!(day_str_like_original(DateLang::En, "3", "3rd"), "3rd");
        assert_eq!(day_str_like_original(DateLang::En, "4", "4th"), "4th");
        assert_eq!(day_str_like_original(DateLang::En, "11", "11th"), "11th");
        assert_eq!(day_str_like_original(DateLang::En, "12", "12th"), "12th");
        assert_eq!(day_str_like_original(DateLang::En, "13", "13th"), "13th");
        assert_eq!(day_str_like_original(DateLang::En, "21", "21st"), "21st");
        assert_eq!(day_str_like_original(DateLang::En, "23", "23rd"), "23rd");
        assert_eq!(day_str_like_original(DateLang::En, "5", "5"), "5");
        assert_eq!(day_str_like_original(DateLang::En, "x", "5th"), "x");
        assert_eq!(day_str_like_original(DateLang::De, "1", "ersten"), "1");
        assert_eq!(day_str_like_original(DateLang::En, "7", "7"), "7");

        assert_eq!(
            adjust_suggestion(DateLang::De, "Mittwoch., 5"),
            "Mittwoch, 5"
        );
        assert_eq!(adjust_suggestion(DateLang::De, "Di, 3"), "Di., 3");
        assert_eq!(
            adjust_suggestion(DateLang::De, "Mo 5., Oktober"),
            "Mo 5., Oktober"
        );
        assert_eq!(
            adjust_suggestion(DateLang::De, "Montag, 5. Oktober"),
            "Montag, 5. Oktober"
        );
        assert_eq!(adjust_suggestion(DateLang::De, ",anfang"), ",anfang");
        assert_eq!(
            adjust_suggestion(DateLang::De, "no comma at all"),
            "no comma at all"
        );
        assert_eq!(adjust_suggestion(DateLang::En, "Monday, 5."), "Monday, 5.");

        assert_eq!(preserve_case("jeudi", "Vendredi"), "Jeudi");
        assert_eq!(preserve_case("jeudi", "VENDREDI"), "JEUDI");
        assert_eq!(preserve_case("jeudi", "vendredi"), "jeudi");
        assert_eq!(preserve_case("jeudi", ""), "jeudi");
        assert_eq!(preserve_case("van", "McVan"), "van");
    }

    pub(super) fn compile_and_match(xml: &str, sentence_text: &str) -> Vec<RuleMatch> {
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();

        let sentence = make_analyzed_sentence(sentence_text);
        let mut matches = Vec::new();
        for rule in &rule_set.rules {
            matches.extend(engine.match_rule(rule, &sentence));
        }
        matches
    }

    #[test]
    fn phraseref_setpos_match_constrains_by_referenced_reading() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="pl">
            <phrases>
                <phrase id="ALT_A">
                    <token>dummy</token>
                </phrase>
                <phrase id="ACC_MN">
                    <token postag=".*:(sg|pl):.*(acc|nom).*[\.:][mn].*" postag_regexp="yes"></token>
                    <token postag_regexp="yes"><match no="0" postag=".*:(sg|pl).*[\.:](acc|nom).*" postag_regexp="yes" postag_replace=".*:$1.*[.\:](acc|nom).*" setpos="yes"></match><exception postag=".*inst.*|.*loc.*" postag_regexp="yes"/></token>
                </phrase>
                <phrase id="ACC">
                    <includephrases>
                        <phraseref idref="ACC_MN"></phraseref>
                        <phraseref idref="ALT_A"></phraseref>
                    </includephrases>
                </phrase>
            </phrases>
            <category id="GRAMMAR" name="Grammar">
                <rule id="SETPOS_RULE" name="setpos">
                    <pattern>
                        <token>opatrzyć</token>
                        <token>w</token>
                        <phraseref idref="ACC"></phraseref>
                    </pattern>
                    <message>m</message>
                </rule>
            </category>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        assert_eq!(rule_set.rules.len(), 2);

        let mut sentence = make_analyzed_sentence("opatrzyć w osobiste godło .");
        set_reading(&mut sentence, 2, "osobisty", "adj:sg:acc:n1.n2:pos");
        set_reading(&mut sentence, 3, "godło", "subst:sg:acc:n2");
        let engine = PatternRuleEngine::new();
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "variant with setpos agreement must fire on acc...acc"
        );

        let mut mismatch = make_analyzed_sentence("opatrzyć w osobistej godle .");
        set_reading(&mut mismatch, 2, "osobisty", "adj:sg:loc:n1.n2:pos");
        set_reading(&mut mismatch, 3, "godło", "subst:sg:loc:n2");
        let engine = PatternRuleEngine::new();
        let nomatch: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &mismatch))
            .collect();
        assert_eq!(nomatch.len(), 0, "loc readings must not fire the rule");
    }

    #[test]
    fn antipattern_marker_immunizes_only_marker_span() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="M_ANTI" name="marker antipattern">
                <antipattern>
                    <token>foo</token>
                    <marker>
                        <token>bar</token>
                    </marker>
                </antipattern>
                <pattern>
                    <marker>
                        <token>foo</token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let matches = compile_and_match(xml, "foo bar");
        assert_eq!(
            matches.len(),
            1,
            "Java immunizes only the marker token 'bar' (startPositionCorrection=1), so 'foo' stays matchable"
        );
    }

    #[test]
    fn antipattern_match_reference_applies_case_conversion() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="M_CASE" name="antipattern with case-converted backreference">
                <antipattern case_sensitive="yes">
                    <token>as</token>
                    <token regexp="yes">[a-z]+</token>
                    <token>as</token>
                    <token><match no="1" case_conversion="startupper"/></token>
                    <token regexp="yes">[A-Z][a-z]+</token>
                </antipattern>
                <pattern>
                    <token>as</token>
                    <marker>
                        <token>big</token>
                        <token>as</token>
                        <token>big</token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        assert_eq!(
            compile_and_match(xml, "He needed a bell as big as Big Ben.").len(),
            0,
            "Java resolveReference compiles the reference with the match's case conversion, \
             so startupper('big')='Big' immunizes the 'as big as Big Ben' antipattern"
        );
        assert_eq!(
            compile_and_match(xml, "He needed a bell as big as big Ben.").len(),
            1
        );
    }

    #[test]
    fn nl_date_check_filter_rejects_consistent_and_suggests_for_mismatch() {
        set_test_date_hack(true);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="nl">
            <rule id="T_NL_DATE" name="datum">
                <pattern>
                    <token regexp="yes">maandag|dinsdag|woensdag|donderdag|vrijdag|zaterdag|zondag</token>
                    <token regexp="yes">\d\d?</token>
                    <token regexp="yes">oktober|september</token>
                    <token regexp="yes">\d\d\d\d</token>
                </pattern>
                <filter class="org.languagetool.rules.nl.DateCheckFilter" args="year:4 month:3 day:2 weekDay:1"/>
                <message>\2 \3 \4 is geen {day}, maar een {realDay}.</message>
            </rule>
        </rules>"#;
        let matches = compile_and_match(xml, "Maandag 7 oktober 2014.");
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(
            m.message(),
            "\\2 \\3 \\4 is geen maandag, maar een dinsdag."
        );
        let reps: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(reps, vec!["Dinsdag 7", "Maandag 6"]);

        assert_eq!(compile_and_match(xml, "Dinsdag 7 oktober 2014.").len(), 0);
    }

    #[test]
    fn nl_date_check_filter_wrong_year_moves_match_to_year_token() {
        set_test_date_hack(true);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="nl">
            <rule id="T_NL_YEAR" name="datum">
                <pattern>
                    <token regexp="yes">maandag|dinsdag|zondag</token>
                    <token regexp="yes">\d\d?</token>
                    <token>september</token>
                    <token regexp="yes">\d\d\d\d</token>
                </pattern>
                <filter class="org.languagetool.rules.nl.DateCheckFilter" args="year:4 month:3 day:2 weekDay:1"/>
                <message>\2 \3 \4 is geen {day}.</message>
            </rule>
        </rules>"#;
        let text = "Zondag 28 september 2013.";
        let matches = compile_and_match(xml, text);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(&text[m.offset()..m.offset() + m.length()], "2013");
        assert_eq!(
            m.message(),
            "Deze datum is onjuist. Bedoelt u misschien \"2014\"?"
        );
        let reps: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(reps, vec!["2014"]);
    }

    #[test]
    fn es_new_year_date_filter_uses_spanish_months() {
        set_test_date_hack(true);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="es">
            <rule id="T_ES_NEWYEAR" name="nuevo año">
                <pattern>
                    <token regexp="yes">\d\d?</token>
                    <token>de</token>
                    <token regexp="yes">octubre|oct|diciembre</token>
                    <token>de</token>
                    <marker><token regexp="yes">\d\d\d\d</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.es.NewYearDateFilter" args="year:\5 month:\3 day:\1"/>
                <message>Ha empezado un año nuevo. ¿Quería decir <suggestion>{realYear}</suggestion>?</message>
            </rule>
        </rules>"#;
        assert_eq!(compile_and_match(xml, "7 de octubre de 2013").len(), 1);
        assert_eq!(compile_and_match(xml, "7 de octubre de 2014").len(), 0);
        assert_eq!(compile_and_match(xml, "7 de diciembre de 2013").len(), 0);
        assert_eq!(compile_and_match(xml, "7 de oct de 2013").len(), 1);
    }

    #[test]
    fn pl_date_check_filter_parses_polish_months_and_weekdays() {
        set_test_date_hack(true);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="pl">
            <rule id="T_PL_DATE" name="data">
                <pattern>
                    <token regexp="yes">poniedziałek|wtorek|środa</token>
                    <token regexp="yes">\d\d?</token>
                    <token>października</token>
                    <token regexp="yes">\d\d\d\d</token>
                </pattern>
                <filter class="org.languagetool.rules.pl.DateCheckFilter" args="year:\4 month:\3 day:\2 weekDay:\1"/>
                <message>Dzień \2 \3 \4 to nie \1.</message>
            </rule>
        </rules>"#;
        assert_eq!(
            compile_and_match(xml, "poniedziałek 7 października 2014").len(),
            1
        );
        assert_eq!(
            compile_and_match(xml, "wtorek 7 października 2014").len(),
            0
        );
    }

    #[test]
    fn ar_date_check_filter_parses_arabic_months_and_weekdays() {
        set_test_date_hack(true);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="ar">
            <rule id="T_AR_DATE" name="تاريخ">
                <pattern>
                    <token regexp="yes">الخميس|الجمعة</token>
                    <token regexp="yes">\d\d?</token>
                    <token>مارس</token>
                    <token regexp="yes">\d\d\d\d</token>
                </pattern>
                <filter class="org.languagetool.rules.ar.filters.ArabicDateCheckFilter" args="year:\4 month:\3 day:\2 weekDay:\1"/>
                <message>لا يوافق يوم \1 بل هو يوم {realDay}.</message>
            </rule>
        </rules>"#;
        let matches = compile_and_match(xml, "الخميس 25 مارس 2022");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message(), "لا يوافق يوم \\1 بل هو يوم الجمعة.");
        assert_eq!(compile_and_match(xml, "الجمعة 25 مارس 2022").len(), 0);
    }

    #[test]
    fn pt_future_date_filter_treats_plain_month_as_literal_number() {
        set_test_date_hack(true);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="pt">
            <rule id="T_PT_FUTURE" name="data futura">
                <pattern>
                    <token>de</token>
                    <token regexp="yes">\d\d?</token>
                    <token>dezembro</token>
                    <token regexp="yes">\d\d\d\d</token>
                </pattern>
                <filter class="org.languagetool.rules.pt.FutureDateFilter" args="year:\4 month:2 day:\2"/>
                <message>data futura</message>
            </rule>
        </rules>"#;
        assert_eq!(compile_and_match(xml, "de 1 dezembro 2014").len(), 1);
        assert_eq!(compile_and_match(xml, "de 1 dezembro 2013").len(), 0);
    }

    #[test]
    fn future_date_filter_accepts_localized_month_names() {
        set_test_date_hack(true);
        for (class, lang, month) in [
            ("org.languagetool.rules.en.FutureDateFilter", "en", "March"),
            ("org.languagetool.rules.de.FutureDateFilter", "de", "März"),
            ("org.languagetool.rules.it.FutureDateFilter", "it", "marzo"),
            ("org.languagetool.rules.ru.FutureDateFilter", "ru", "марта"),
        ] {
            let xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="{lang}">
            <rule id="T_FUTURE_{lang}" name="future date">
                <pattern>
                    <token>am</token>
                    <token regexp="yes">\d\d?</token>
                    <token>{month}</token>
                    <token regexp="yes">\d\d\d\d</token>
                </pattern>
                <filter class="{class}" args="year:\4 month:\3 day:\2"/>
                <message>future date</message>
            </rule>
        </rules>"#
            );
            let future = format!("am 5 {month} 2015 .");
            let past = format!("am 5 {month} 2013 .");
            assert_eq!(
                compile_and_match(&xml, &future).len(),
                1,
                "{class} keeps a date after the pinned current date"
            );
            assert_eq!(
                compile_and_match(&xml, &past).len(),
                0,
                "{class} drops a date before the pinned current date"
            );
        }
    }

    #[test]
    fn inn_number_filter_keeps_only_invalid_inns() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="ru">
            <rule id="INN" name="inn">
                <pattern>
                    <marker><token regexp="yes">\d{10,12}</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.ru.INNNumberFilter" args="inn:\1"/>
                <message>m</message>
            </rule>
        </rules>"#;
        assert_eq!(compile_and_match(xml, "ИНН 7830023695 здесь").len(), 0);
        assert_eq!(compile_and_match(xml, "ИНН 7830023690 здесь").len(), 1);
        let no_arg = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="ru">
            <rule id="INN2" name="inn2">
                <pattern>
                    <marker><token regexp="yes">\d{10,12}</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.ru.INNNumberFilter"/>
                <message>m</message>
            </rule>
        </rules>"#;
        assert_eq!(compile_and_match(no_arg, "ИНН 7830023690 здесь").len(), 0);
    }

    #[test]
    fn russian_partial_pos_tag_filter_dispatch_checks_the_tagged_form() {
        let xml = |postag: &str| {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="ru">
            <rule id="RU_PARTIAL" name="partial">
                <pattern>
                    <token>стол</token>
                </pattern>
                <filter class="org.languagetool.rules.ru.RussianPartialPosTagFilter" args="no:1 regexp:(.+) postag_regexp:{postag}"/>
                <message>m</message>
            </rule>
        </rules>"#
            )
        };
        assert_eq!(compile_and_match(&xml("NN.*"), "это стол тут").len(), 1);
        assert_eq!(compile_and_match(&xml("VB.*"), "это стол тут").len(), 0);
    }

    #[test]
    fn russian_suppress_misspelled_filter_dispatch_drops_misspelled_suggestions() {
        let xml = |suggestion: &str| {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="ru">
            <rule id="RU_SUPPRESS" name="suppress">
                <pattern>
                    <marker><token>каждя</token></marker>
                </pattern>
                <suggestion>{suggestion}</suggestion>
                <filter class="org.languagetool.rules.ru.RussianSuppressMisspelledSuggestionsFilter" args="suppressMatch:true"/>
                <message>m</message>
            </rule>
        </rules>"#
            )
        };
        assert_eq!(compile_and_match(&xml("каждя"), "каждя раз").len(), 0);
        let matches = compile_and_match(&xml("каждый"), "каждя раз");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "каждый");
    }

    #[test]
    fn filter_args_fall_back_to_literals_when_backrefs_do_not_resolve() {
        set_test_date_hack(true);
        for args in ["year:\\9 maxYearsBack:10", "year:2010 maxYearsBack:\\x"] {
            let xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="RY" name="ry">
                <pattern>
                    <token regexp="yes">\d{{4}}</token>
                </pattern>
                <message>m</message>
                <filter class="org.languagetool.rules.de.RecentYearFilter" args="{args}"/>
            </rule>
        </rules>"#
            );
            assert_eq!(
                compile_and_match(&xml, "Im Jahr 2010.").len(),
                0,
                "args {args} cannot resolve to a recent year"
            );
        }
    }

    #[test]
    fn ru_date_check_filter_parses_russian_months_and_weekdays() {
        set_test_date_hack(true);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="ru">
            <rule id="T_RU_DATE" name="дата">
                <pattern>
                    <token regexp="yes">понедельник|вторник</token>
                    <token min="0" max="1">,</token>
                    <token regexp="yes">\d\d?</token>
                    <token>октября</token>
                    <token regexp="yes">\d\d\d\d</token>
                </pattern>
                <filter class="org.languagetool.rules.ru.DateCheckFilter" args="year:\5 month:\4 day:\3 weekDay:\1"/>
                <message>Является {realDay}.</message>
            </rule>
        </rules>"#;
        assert_eq!(
            compile_and_match(xml, "понедельник, 7 октября 2014").len(),
            1
        );
        assert_eq!(
            compile_and_match(xml, "понедельник 7 октября 2014").len(),
            1
        );
        assert_eq!(compile_and_match(xml, "вторник 7 октября 2014").len(), 0);
    }

    #[test]
    fn br_date_check_filter_parses_breton_day_words() {
        set_test_date_hack(true);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="br">
            <rule id="T_BR_DATE" name="deiziad">
                <pattern>
                    <token regexp="yes">Dilun|Dimerc’her</token>
                    <token regexp="yes">\d\d?|eil</token>
                    <token>a</token>
                    <token>viz</token>
                    <token>gwengolo</token>
                    <token regexp="yes">\d\d\d\d</token>
                </pattern>
                <filter class="org.languagetool.rules.br.DateCheckFilter" args="year:\6 month:\5 day:\2 weekDay:\1"/>
                <message>Deiz fall.</message>
            </rule>
        </rules>"#;
        assert_eq!(
            compile_and_match(xml, "Dilun 2 a viz gwengolo 2014").len(),
            1
        );
        assert_eq!(
            compile_and_match(xml, "Dilun eil a viz gwengolo 2014").len(),
            1
        );
        assert_eq!(
            compile_and_match(xml, "Dimeurzh 2 a viz gwengolo 2014").len(),
            0
        );
    }

    #[test]
    fn fr_date_check_filter_suggests_weekday_and_day() {
        set_test_date_hack(true);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="fr">
            <rule id="T_FR_DATE" name="date">
                <pattern>
                    <token regexp="yes">lundi|mardi|mercredi|jeudi|vendredi</token>
                    <token regexp="yes">\d\d?</token>
                    <token>août</token>
                    <token regexp="yes">\d\d\d\d</token>
                </pattern>
                <filter class="org.languagetool.rules.fr.DateCheckFilter" args="year:4 month:3 day:2 weekDay:1"/>
                <message>n’est pas un {day} mais un {realDay}.</message>
            </rule>
        </rules>"#;
        let text = "Vendredi 28 août 2014.";
        let matches = compile_and_match(xml, text);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(&text[m.offset()..m.offset() + m.length()], "Vendredi 28");
        assert_eq!(m.message(), "n’est pas un vendredi mais un jeudi.");
        let reps: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(reps, vec!["Jeudi 28", "Vendredi 29"]);
    }

    fn set_reading(sentence: &mut og_core::AnalyzedSentence, i: usize, lemma: &str, tag: &str) {
        let tokens = sentence.tokens_mut();
        let atr = &mut tokens[i];
        let token = atr.token().clone();
        let reading = og_core::AnalyzedToken::new(token.token(), token.start(), token.end())
            .with_lemma(lemma.to_string())
            .with_pos_tags(vec![tag.to_string()]);
        *atr = og_core::AnalyzedTokenReadings::new(token).with_readings(vec![reading]);
    }

    struct TableTagger;

    impl og_tagger::Tagger for TableTagger {
        fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
            tokens
                .iter()
                .map(|&word| {
                    let tags: &[&str] = match word.to_lowercase().as_str() {
                        "good" => &["JJ"],
                        "gold" => &["NN", "JJ"],
                        "god" => &["NN"],
                        "well" => &["RB"],
                        "cafe" => &["RB"],
                        "café" => &["JJ"],
                        _ => &[],
                    };
                    let readings = tags
                        .iter()
                        .map(|t| {
                            og_core::AnalyzedToken::new(word, 0, word.len())
                                .with_lemma(word)
                                .with_pos_tags(vec![t.to_string()])
                        })
                        .collect();
                    AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(word, 0, word.len()))
                        .with_readings(readings)
                })
                .collect()
        }
    }

    fn engine_with_words(words: &[&str], multitoken: &str) -> PatternRuleEngine {
        use crate::spelling_support::SpellingSupport;
        let mut dict = og_spell::Dictionary::new();
        for w in words {
            dict.add_word(w);
        }
        let support =
            SpellingSupport::new(dict, multitoken).with_tagger(std::sync::Arc::new(TableTagger));
        PatternRuleEngine::new().with_spelling_support(std::sync::Arc::new(support))
    }

    fn match_with_engine(
        engine: &PatternRuleEngine,
        xml: &str,
        sentence_text: &str,
    ) -> Vec<RuleMatch> {
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let sentence = make_analyzed_sentence(sentence_text);
        let mut matches = Vec::new();
        for rule in &rule_set.rules {
            matches.extend(engine.match_rule(rule, &sentence));
        }
        matches
    }

    #[test]
    fn find_suggestions_filter_fills_the_suggestion_placeholder() {
        let engine = engine_with_words(&["good", "gold", "god"], "");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="FIND_SUGG" name="find suggestions">
                <pattern>
                    <token>the</token>
                    <marker><token regexp="yes">[a-z]+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.FindSuggestionsFilter" args="wordFrom:marker desiredPostag:JJ"/>
                <message>m</message>
                <suggestion>{suggestion}</suggestion>
            </rule>
        </rules>"#;
        let matches = match_with_engine(&engine, xml, "I like the gokd one.");
        assert_eq!(matches.len(), 1);
        let mut reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        reps.sort_unstable();
        assert_eq!(reps, vec!["gold", "good"]);
    }

    #[test]
    fn find_suggestions_filter_priority_postag_moves_matches_first() {
        let engine = engine_with_words(&["good", "gold", "god"], "");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="FIND_SUGG_PRIO" name="find suggestions priority">
                <pattern>
                    <token>the</token>
                    <marker><token regexp="yes">[a-z]+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.FindSuggestionsFilter" args="wordFrom:marker desiredPostag:JJ priorityPostag:NN"/>
                <message>m</message>
                <suggestion>{suggestion}</suggestion>
            </rule>
        </rules>"#;
        let matches = match_with_engine(&engine, xml, "I like the gokd one.");
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["gold", "good"]);
    }

    #[test]
    fn find_suggestions_filter_preserves_capitalization_and_filters_regexp() {
        let engine = engine_with_words(&["good", "gold", "god"], "");
        let base = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="FIND_SUGG_CAP" name="find suggestions capitalization">
                <pattern>
                    <token>the</token>
                    <marker><token regexp="yes">[a-z]+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.FindSuggestionsFilter" args="{args}"/>
                <message>m</message>
                <suggestion>{SUGGESTION}</suggestion>
            </rule>
        </rules>"#;
        let xml = base.replace("{args}", "wordFrom:marker desiredPostag:JJ");
        let matches = match_with_engine(&engine, &xml, "I like the Gokd one.");
        let mut reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        reps.sort_unstable();
        assert_eq!(reps, vec!["GOLD", "GOOD"]);

        let base = base.replace("{SUGGESTION}", "{Suggestion}");
        let xml = base.replace(
            "{args}",
            "wordFrom:marker desiredPostag:JJ removeSuggestionsRegexp:gold",
        );
        let matches = match_with_engine(&engine, &xml, "I like the gokd one.");
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["Good"]);
    }

    #[test]
    fn find_suggestions_filter_diacritics_mode_suppresses_or_suggests() {
        let engine = engine_with_words(&["cafe", "gold"], "");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="FIND_SUGG_DIA" name="find suggestions diacritics">
                <pattern>
                    <token>the</token>
                    <marker><token regexp="yes">[A-Za-zà-ÿ]+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.FindSuggestionsFilter" args="wordFrom:marker desiredPostag:RB Mode:diacritics"/>
                <message>m</message>
                <suggestion>{suggestion}</suggestion>
            </rule>
        </rules>"#;
        let matches = match_with_engine(&engine, xml, "The café tastes good.");
        assert_eq!(matches.len(), 1);
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["cafe"]);
        assert_eq!(
            match_with_engine(&engine, xml, "The caffe tastes good.").len(),
            0,
            "no diacritics-only suggestion removes the match"
        );

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="FIND_SUGG_DIA_OK" name="find suggestions diacritics ok">
                <pattern>
                    <token>the</token>
                    <marker><token regexp="yes">[A-Za-zà-ÿ]+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.FindSuggestionsFilter" args="wordFrom:marker desiredPostag:JJ Mode:diacritics"/>
                <message>m</message>
                <suggestion>{suggestion}</suggestion>
            </rule>
        </rules>"#;
        assert_eq!(
            match_with_engine(&engine, xml, "The café tastes good.").len(),
            0,
            "word already has the desired postag, diacritics mode removes the match"
        );

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="FIND_SUGG_SUP" name="find suggestions suppress">
                <pattern>
                    <token>the</token>
                    <marker><token regexp="yes">[a-z]+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.FindSuggestionsFilter" args="wordFrom:marker desiredPostag:VB suppressMatch:true"/>
                <message>m</message>
                <suggestion>{suggestion}</suggestion>
            </rule>
        </rules>"#;
        assert_eq!(
            match_with_engine(&engine, xml, "I like the gokd one.").len(),
            0,
            "no verb suggestions and suppressMatch drops the match"
        );
    }

    #[test]
    fn find_suggestions_filter_keeps_match_without_required_args() {
        let engine = engine_with_words(&["good"], "");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="FIND_SUGG_ARGS" name="find suggestions args">
                <pattern>
                    <token>the</token>
                    <marker><token regexp="yes">[a-z]+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.FindSuggestionsFilter" args="desiredPostag:JJ"/>
                <message>m</message>
                <suggestion>{suggestion}</suggestion>
            </rule>
        </rules>"#;
        let matches = match_with_engine(&engine, xml, "I like the gokd one.");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "{suggestion}");
    }

    #[test]
    fn number_in_word_filter_suggests_zero_o_and_digit_free_forms() {
        let engine = engine_with_words(&["good", "god", "well"], "");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="NUM_IN_WORD" name="number in word">
                <pattern>
                    <token>the</token>
                    <marker><token regexp="yes">[A-Za-z0-9]+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.EnglishNumberInWordFilter" args="word:\2"/>
                <message>m</message>
            </rule>
        </rules>"#;
        let matches = match_with_engine(&engine, xml, "the go0d morning!");
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["good", "god"]);

        assert!(
            match_with_engine(&engine, xml, "the q9z morning.").is_empty(),
            "no replacements and no nearby words drops the match"
        );
    }

    #[test]
    fn multitoken_speller_filter_suggests_the_known_phrase() {
        let engine = engine_with_words(
            &[
                "good", "god", "the", "dark", "knight", "rises", "rocky", "mountain",
            ],
            "The Dark Knight Rises\nRocky Mountain National Park # comment\n",
        );
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="MULTITOKEN" name="multitoken">
                <pattern>
                    <marker>
                        <token>Thé</token>
                        <token>Dákk</token>
                        <token>Kínght</token>
                        <token>Ríses</token>
                    </marker>
                </pattern>
                <filter class="org.languagetool.rules.spelling.multitoken.MultitokenSpellerFilter"/>
                <message>m</message>
            </rule>
        </rules>"#;
        let text = "I said , Thé Dákk Kínght Ríses .";
        let matches = match_with_engine(&engine, xml, text);
        assert_eq!(matches.len(), 1);
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["The Dark Knight Rises"]);
        let from = matches[0].offset();
        let to = from + matches[0].length();
        assert_eq!(&text[from..to], "Thé Dákk Kínght Ríses");
    }

    #[test]
    fn multitoken_speller_filter_uppercases_suggestions_for_all_caps_errors() {
        let engine = engine_with_words(
            &[
                "good", "god", "the", "dark", "knight", "rises", "rocky", "mountain",
            ],
            "The Dark Knight Rises\nRocky Mountain National Park # comment\n",
        );
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="MULTITOKEN" name="multitoken">
                <pattern>
                    <marker>
                        <token>THÉ</token>
                        <token>DÁKK</token>
                        <token>KÍNGHT</token>
                        <token>RÍSES</token>
                    </marker>
                </pattern>
                <filter class="org.languagetool.rules.spelling.multitoken.MultitokenSpellerFilter"/>
                <message>m</message>
            </rule>
        </rules>"#;
        let text = "I said , THÉ DÁKK KÍNGHT RÍSES .";
        let matches = match_with_engine(&engine, xml, text);
        assert_eq!(matches.len(), 1);
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["THE DARK KNIGHT RISES"]);
    }

    #[test]
    fn multitoken_speller_filter_capitalizes_suggestions_at_sentence_start() {
        let engine = engine_with_words(&["the", "dark", "knight", "rises"], "dark knight rises\n");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="MULTITOKEN" name="multitoken">
                <pattern>
                    <marker>
                        <token>dákk</token>
                        <token>kínght</token>
                        <token>ríses</token>
                    </marker>
                </pattern>
                <filter class="org.languagetool.rules.spelling.multitoken.MultitokenSpellerFilter"/>
                <message>m</message>
            </rule>
        </rules>"#;
        let text = ", dákk kínght ríses .";
        let matches = match_with_engine(&engine, xml, text);
        assert_eq!(matches.len(), 1);
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["Dark knight rises"]);
    }

    #[test]
    fn ordinal_suffix_filter_builds_the_english_ordinals() {
        let engine = PatternRuleEngine::new();
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="ORDINAL" name="ordinal">
                <pattern>
                    <marker><token regexp="yes">\d+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.OrdinalSuffixFilter"/>
                <message>m</message>
                <suggestion>\1th</suggestion>
            </rule>
        </rules>"#;
        for (text, expected) in [
            ("Place 1.", "1st"),
            ("Place 2.", "2nd"),
            ("Place 3.", "3rd"),
            ("Place 4.", "4th"),
            ("Place 11.", "11th"),
            ("Place 12.", "12th"),
            ("Place 13.", "13th"),
            ("Place 21.", "21st"),
            ("Place 111.", "111th"),
        ] {
            let matches = match_with_engine(&engine, xml, text);
            assert_eq!(matches.len(), 1, "{text}");
            assert_eq!(matches[0].replacements()[0].value(), expected, "{text}");
        }
    }

    #[test]
    fn adverb_filter_swaps_the_adverb_for_the_adjective() {
        let engine = PatternRuleEngine::new();
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="ADVERB" name="adverb">
                <pattern>
                    <marker>
                        <token>well</token>
                        <token>performance</token>
                    </marker>
                </pattern>
                <filter class="org.languagetool.rules.en.AdverbFilter" args="adverb:\1 noun:\2"/>
                <message>m</message>
            </rule>
        </rules>"#;
        let matches = match_with_engine(&engine, xml, "a well performance");
        assert_eq!(matches.len(), 1);
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["good performance"]);

        let unchanged = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="ADVERB_FAST" name="adverb fast">
                <pattern>
                    <marker>
                        <token>fast</token>
                        <token>car</token>
                    </marker>
                </pattern>
                <filter class="org.languagetool.rules.en.AdverbFilter" args="adverb:\1 noun:\2"/>
                <message>m</message>
                <suggestion>fast car</suggestion>
            </rule>
        </rules>"#;
        let matches = match_with_engine(&engine, unchanged, "a fast car");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacements()[0].value(),
            "fast car",
            "'fast' is its own adjective, the suggestion stays"
        );
    }

    #[test]
    fn underline_spaces_filter_extends_the_match_around_whitespace() {
        let engine = PatternRuleEngine::new();
        let base = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="UNDERLINE" name="underline">
                <pattern>
                    <marker><token regexp="yes">_+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.UnderlineSpacesFilter" args="underlineSpaces:{mode}"/>
                <message>m</message>
            </rule>
        </rules>"#;
        let text = "a _ b";
        for (mode, from, len) in [
            ("both", 1, " _ ".len()),
            ("before", 1, " _".len()),
            ("after", 2, "_ ".len()),
        ] {
            let xml = base.replace("{mode}", mode);
            let matches = match_with_engine(&engine, &xml, text);
            assert_eq!(matches.len(), 1, "{mode}");
            assert_eq!(matches[0].offset(), from, "{mode}");
            assert_eq!(matches[0].length(), len, "{mode}");
        }
        let no_args = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="UNDERLINE_DEFAULT" name="underline default">
                <pattern>
                    <marker><token regexp="yes">_+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.UnderlineSpacesFilter"/>
                <message>m</message>
            </rule>
        </rules>"#;
        let matches = match_with_engine(&engine, no_args, text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 1);
        assert_eq!(matches[0].length(), " _ ".len());
    }

    #[test]
    fn suppress_misspelled_suggestions_filter_keeps_only_spellable_ones() {
        let engine = engine_with_words(&["good", "well"], "");
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="SUPPRESS_MISSPELLED" name="suppress">
                <pattern>
                    <token>the</token>
                    <marker><token regexp="yes">[a-z]+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.EnglishSuppressMisspelledSuggestionsFilter"/>
                <message>m</message>
                <suggestion>godd</suggestion>
                <suggestion>good</suggestion>
            </rule>
        </rules>"#;
        let matches = match_with_engine(&engine, xml, "the godd one");
        assert_eq!(matches.len(), 1);
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["good"]);

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="SUPPRESS_ALL" name="suppress all">
                <pattern>
                    <token>the</token>
                    <marker><token regexp="yes">[a-z]+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.EnglishSuppressMisspelledSuggestionsFilter"/>
                <message>m</message>
                <suggestion>godd</suggestion>
            </rule>
        </rules>"#;
        assert!(
            match_with_engine(&engine, xml, "the godd one").is_empty(),
            "all suggestions misspelled drops the match by default"
        );

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="SUPPRESS_TAG" name="suppress by tag">
                <pattern>
                    <token>the</token>
                    <marker><token regexp="yes">[a-z]+</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.EnglishSuppressMisspelledSuggestionsFilter" args="suppressMatch:false FilterPostag:RB"/>
                <message>m</message>
                <suggestion>good</suggestion>
                <suggestion>well</suggestion>
            </rule>
        </rules>"#;
        let matches = match_with_engine(&engine, xml, "the good one");
        assert_eq!(matches.len(), 1);
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["well"]);
    }

    pub(super) fn make_analyzed_sentence(text: &str) -> og_core::AnalyzedSentence {
        use og_core::{AnalyzedToken, AnalyzedTokenReadings};
        let mut sentence = og_core::AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut pos = 0;
        let chars: Vec<char> = text.chars().collect();
        while pos < chars.len() {
            while pos < chars.len() && chars[pos].is_whitespace() {
                pos += 1;
            }
            if pos >= chars.len() {
                break;
            }
            let start_byte = text[..text
                .char_indices()
                .nth(pos)
                .map(|(i, _)| i)
                .unwrap_or(text.len())]
                .len();
            if chars[pos].is_alphanumeric() || chars[pos] == '\'' {
                let word_start = pos;
                while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '\'') {
                    pos += 1;
                }
                let word: String = chars[word_start..pos].iter().collect();
                let end_byte = text[..text
                    .char_indices()
                    .nth(pos)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len())]
                    .len();
                let at = AnalyzedToken::new(&word, start_byte, end_byte);
                tokens.push(AnalyzedTokenReadings::new(at));
            } else {
                let ch = chars[pos].to_string();
                pos += 1;
                let end_byte = text[..text
                    .char_indices()
                    .nth(pos)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len())]
                    .len();
                let at = AnalyzedToken::new(&ch, start_byte, end_byte);
                tokens.push(AnalyzedTokenReadings::new(at));
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn immunized_tokens_are_invisible_to_pattern_rules() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="IMMUNIZED_HIT" name="immunized hit">
                <pattern>
                    <token>Angeles</token>
                </pattern>
                <message>hit</message>
                <example correction=""><marker>Angeles</marker></example>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();

        let mut sentence = make_analyzed_sentence("Los Angeles");
        sentence.tokens_mut()[1].immunize();
        let matches = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        assert_eq!(matches, 0, "immunized token must not match");

        let mut sentence = make_analyzed_sentence("Los Angeles");
        sentence.tokens_mut()[0].immunize();
        let matches = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        assert_eq!(matches, 1, "only the immunized token is hidden");
    }

    #[test]
    fn rule_level_regexp_matches_text_and_resolves_suggestion_backrefs() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <category id="TYPOGRAPHY" name="Typografie">
                <rule id="REGEXP_RULE_TEST" name="Regexp rule">
                    <regexp case_sensitive="yes">Prof\.Dr\.</regexp>
                    <message>Die Abkürzung <suggestion>Prof. Dr.</suggestion> sollte durch ein Leerzeichen getrennt werden.</message>
                </rule>
            </category>
        </rules>"#;
        let matches = compile_and_match(xml, "Prof.Dr. med. Hans Müller");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].rule().id(), "REGEXP_RULE_TEST");
        assert_eq!(&matches[0].sentence().unwrap()[..14], "Prof.Dr. med. ");
        assert_eq!(matches[0].replacements()[0].value(), "Prof. Dr.");
    }

    #[test]
    fn rule_level_regexp_mark_group_and_message_backrefs() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="CURRENCY_REGEXP_TEST" name="Currency">
                    <regexp>(?&lt;![$€£])([\d,.]{1,10}\d)\s?([$€£])(?!\d)</regexp>
                    <message>The currency mark is usually put at the beginning of the number.</message>
                    <suggestion>\2\1</suggestion>
                </rule>
            </category>
        </rules>"#;
        let matches = compile_and_match(xml, "You owe me 7.25$ today.");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "$7.25");
        let from = matches[0].offset();
        let to = from + matches[0].length();
        assert_eq!(&"You owe me 7.25$ today."[from..to], "7.25$");
    }

    #[test]
    fn rule_level_regexp_smart_mode_tolerates_whitespace() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <category id="STYLE" name="Stil">
                <rule id="SMART_REGEXP_TEST" name="Smart">
                    <regexp>sehr (sehr) gut</regexp>
                    <message>Meinten Sie <suggestion><match no="1"/></suggestion>?</message>
                </rule>
            </category>
        </rules>"#;
        let matches = compile_and_match(xml, "Das ist ein sehr sehr  guter Text.");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "sehr");
    }

    #[test]
    fn rule_level_regexp_mark_attribute_selects_group_span() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <category id="STYLE" name="Stil">
                <rule id="MARK_REGEXP_TEST" name="Mark">
                    <regexp mark="1">\bZünglein an der (Wage)\b</regexp>
                    <message>Meinten Sie <suggestion>Waage</suggestion>?</message>
                </rule>
            </category>
        </rules>"#;
        let matches = compile_and_match(xml, "Er ist das Zünglein an der Wage.");
        assert_eq!(matches.len(), 1);
        let from = matches[0].offset();
        let to = from + matches[0].length();
        assert_eq!(&"Er ist das Zünglein an der Wage."[from..to], "Wage");
    }

    #[test]
    fn rule_level_regexp_missing_mark_group_falls_back_to_whole_match() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="ALT_GROUPS" name="alternation">
                <regexp mark="1">(zebra)|(cat)s</regexp>
                <message>m</message>
            </rule>
        </rules>"#;
        let matches = compile_and_match(xml, "The cats sleep.");
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(
            &"The cats sleep."[m.offset()..m.offset() + m.length()],
            "cats"
        );
    }

    #[test]
    fn rule_level_regexp_anti_pattern_filter_blocks_overlapping_matches() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="CURRENCY" name="Position of currency symbols">
                <regexp>([0-9.,]{0,10}[0-9]{1,10})\s?([$€£])</regexp>
                <filter class="org.languagetool.rules.patterns.RegexAntiPatternFilter" args="antipatterns:[0-9][-–—][0-9]{1,10}[$€£]"/>
                <message>Put the currency mark first.</message>
                <suggestion>$\1</suggestion>
            </rule>
        </rules>"#;
        assert_eq!(
            compile_and_match(xml, "You owe me 7.25$ today.").len(),
            1,
            "plain amount matches"
        );
        assert_eq!(
            compile_and_match(xml, "Ranges like 1-10$ are fine.").len(),
            0,
            "the antipattern covers the match and suppresses it"
        );
        assert_eq!(
            compile_and_match(xml, "The range 1-10 to 20$ is odd.").len(),
            1,
            "non-overlapping antipattern keeps the match"
        );
    }

    #[test]
    fn rule_level_regexp_suggestion_match_transforms_group_text() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="MATCH_TRANSFORM" name="match transforms">
                <regexp case_sensitive="yes">(profa|srta)\.</regexp>
                <message>Use <suggestion><match no="1" regexp_match="(profa|srta)" regexp_replace="Dra. "/><match no="1" case_conversion="startupper"/></suggestion>.</message>
            </rule>
        </rules>"#;
        let matches = compile_and_match(xml, "A profa. spoke.");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0]
                .replacements()
                .iter()
                .map(|r| r.value())
                .collect::<Vec<_>>(),
            vec!["Dra. Profa"]
        );
    }

    #[test]
    fn rule_level_regexp_context_snaps_to_char_boundaries() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BOUNDARY" name="boundary">
                <regexp>cat</regexp>
                <message>m</message>
            </rule>
        </rules>"#;
        let prefix = "é".repeat(30);
        let suffix = "é".repeat(30);
        let text = format!("{prefix} cat {suffix}");
        let matches = compile_and_match(&xml, &text);
        assert_eq!(matches.len(), 1);
        let context = matches[0].context();
        assert!(context.text().starts_with('é'), "{}", context.text());
        assert!(context.text().ends_with('é'), "{}", context.text());
        assert_eq!(context.offset() % 2, 0, "context start is on an é boundary");
    }

    #[test]
    fn rule_level_regexp_rule_metadata_reaches_the_match() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="TYPOGRAPHY" name="Typography">
                <rulegroup id="META_REGEXP" name="Metadata">
                    <rule id="META_REGEXP_FIRST">
                        <regexp>b+C</regexp>
                        <short>short msg</short>
                        <url>https://example.org/rule</url>
                        <message>m</message>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;
        let matches = compile_and_match(xml, "abBc");
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.rule().id(), "META_REGEXP_FIRST");
        assert_eq!(m.short_message(), Some("short msg"));
        assert_eq!(m.rule().category().id().as_str(), "TYPOGRAPHY");
        assert_eq!(m.rule().category().name(), "Typography");
    }

    #[test]
    fn pt_toponym_filter_runs_on_regexp_rules() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="pt">
            <rule id="TOPONYM_REGEXP" name="meia-risca">
                <regexp case_sensitive="yes" mark="2">(\p{Lu}\p{Ll}*)(\s?\((RJ)\))</regexp>
                <filter class="org.languagetool.rules.pt.BrazilianToponymFilter" args="foo:bar"/>
                <message>Use a meia-risca.</message>
                <suggestion>{suggestion}</suggestion>
            </rule>
        </rules>"#;
        let text = "Nasceu em Niterói (RJ) e morreu lá.";
        let matches = compile_and_match(xml, text);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(&text[m.offset()..m.offset() + m.length()], " (RJ)");
        let reps: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(reps, vec!["–RJ"]);
    }

    #[test]
    fn match_rules_builds_and_uses_the_word_index() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="INDEXED" name="indexed">
                <pattern>
                    <token>hello</token>
                    <token>world</token>
                </pattern>
                <message>m</message>
            </rule>
            <rule id="CATCH_ALL" name="catch all">
                <pattern>
                    <token regexp="yes">he[l]{2}o</token>
                    <token>world</token>
                </pattern>
                <message>m2</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let rules: Vec<&CompiledRule> = rule_set.rules.iter().collect();
        let engine = PatternRuleEngine::new();
        let sentence = make_analyzed_sentence("Hello World");
        let matches = engine.match_rules(&rules, &sentence);
        assert_eq!(
            matches.len(),
            2,
            "both the indexed and the catch-all rule fire"
        );
        let empty = engine.match_rules(&rules, &make_analyzed_sentence("nothing here"));
        assert!(empty.is_empty());
    }

    #[test]
    fn inflected_first_token_scans_positions_by_lemma() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="INFLECTED_FIRST" name="inflected first">
                <pattern>
                    <token inflected="yes">groß</token>
                    <token>Haus</token>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();

        let mut sentence = make_analyzed_sentence("ein großes Haus");
        set_reading(&mut sentence, 1, "groß", "ADJ");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "lemma 'groß' matches the inflected 'großes': {:?}",
            matches
                .iter()
                .map(|m| m.rule().id().to_string())
                .collect::<Vec<_>>()
        );

        let bare = make_analyzed_sentence("ein großes Haus");
        let none: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &bare))
            .collect();
        assert!(
            none.is_empty(),
            "without readings the lemma cannot be found"
        );
    }

    #[test]
    fn insert_comma_filter_ports_the_java_test_battery() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="KOMMA_ZWISCHEN" name="Komma zwischen Haupt- und Nebensatz">
                <pattern>
                    <marker>
                        <token>steht</token>
                        <token>parkt</token>
                    </marker>
                </pattern>
                <filter class="org.languagetool.rules.de.InsertCommaFilter" args="fake:1"/>
                <message>Komma fehlt.</message>
                <suggestion><match no="1"/> <match no="2"/></suggestion>
            </rule>
        </rules>"#;
        let matches = compile_and_match(
            xml,
            "Das Auto, das am Straßenrand steht parkt im Halteverbot.",
        );
        assert_eq!(matches.len(), 1);
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["steht, parkt"]);
    }

    #[test]
    fn insert_comma_filter_hoffe_bei_euch_matches_java() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="KOMMA_HOFFE" name="Komma nach Hoffe">
                <pattern>
                    <marker>
                        <token>bei</token>
                        <token>euch</token>
                        <token>ist</token>
                    </marker>
                </pattern>
                <filter class="org.languagetool.rules.de.InsertCommaFilter" args="fake:1"/>
                <message>Komma fehlt.</message>
                <suggestion>Hoffe <match no="1"/> <match no="2"/> <match no="3"/></suggestion>
            </rule>
        </rules>"#;
        let matches = compile_and_match(xml, "Hoffe bei euch ist alles gut.");
        assert_eq!(matches.len(), 1);
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["Hoffe, bei euch ist"]);
    }

    #[test]
    fn insert_comma_filter_covers_the_tagger_gates() {
        let cases = [
            (
                r#"<pattern><marker><token>hoffe</token><token>es</token><token>geht</token></marker></pattern>
                   <suggestion><match no="1"/> <match no="2"/> <match no="3"/></suggestion>"#,
                "Ich hoffe es geht Ihnen gut.",
                "hoffe, es geht",
            ),
            (
                r#"<pattern><marker><token>Sag</token><token>mal</token><token>hast</token></marker></pattern>
                   <suggestion><match no="1"/> <match no="2"/> <match no="3"/></suggestion>"#,
                "Sag mal hast du das gewusst.",
                "Sag mal, hast",
            ),
            (
                r#"<pattern><marker><token>denke</token><token>hier</token><token>kann</token></marker></pattern>
                   <suggestion><match no="1"/> <match no="2"/> <match no="3"/></suggestion>"#,
                "Ich denke hier kann aber auch mehr stehen.",
                "denke, hier kann",
            ),
            (
                r#"<pattern><marker><token>glaube</token><token>eure</token><token>Accounts</token><token>sind</token></marker></pattern>
                   <suggestion><match no="1"/> <match no="2"/> <match no="3"/> <match no="4"/></suggestion>"#,
                "Ich glaube eure Accounts sind noch aktiv.",
                "glaube, eure Accounts sind",
            ),
            (
                r#"<pattern><marker><token>glaube</token><token>eure</token><token>roten</token><token>Autos</token></marker></pattern>
                   <suggestion><match no="1"/> <match no="2"/> <match no="3"/> <match no="4"/></suggestion>"#,
                "Ich glaube eure roten Autos sind schnell.",
                "glaube, eure roten Autos",
            ),
            (
                r#"<pattern><marker><token>schätze</token><token>diese</token><token>Krawatte</token><token>passt</token></marker></pattern>
                   <suggestion><match no="1"/> <match no="2"/> <match no="3"/> <match no="4"/></suggestion>"#,
                "Ich schätze diese Krawatte passt gut.",
                "schätze, diese Krawatte passt",
            ),
            (
                r#"<pattern><marker><token>weißt</token><token>du</token><token>warum</token><token>das</token></marker></pattern>
                   <suggestion><match no="1"/> <match no="2"/> <match no="3"/> <match no="4"/></suggestion>"#,
                "Weißt du warum das so ist.",
                "Weißt du, warum das",
            ),
            (
                r#"<pattern><marker><token>Ist</token><token>der</token><token>Kunde</token><token>Verbraucher</token><token>gilt</token></marker></pattern>
                   <suggestion><match no="1"/> <match no="2"/> <match no="3"/> <match no="4"/> <match no="5"/></suggestion>"#,
                "Ist der Kunde Verbraucher gilt eine Frist.",
                "Ist der Kunde Verbraucher,",
            ),
        ];
        for (pattern, text, expected) in cases {
            let xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <rules lang="de">
                    <rule id="KOMMA_CASE" name="Komma">
                        {pattern}
                        <filter class="org.languagetool.rules.de.InsertCommaFilter" args="fake:1"/>
                        <message>Komma fehlt.</message>
                    </rule>
                </rules>"#
            );
            let matches = compile_and_match(&xml, text);
            assert_eq!(matches.len(), 1, "no match for {text} with {pattern}");
            let reps: Vec<&str> = matches[0]
                .replacements()
                .iter()
                .map(|r| r.value())
                .collect();
            assert_eq!(reps, vec![expected], "case {text}");
        }
    }

    #[test]
    fn insert_comma_filter_drops_unsupported_replacements() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="KOMMA_DROP" name="Komma drop">
                <pattern>
                    <marker>
                        <token>steht</token>
                        <token>im</token>
                        <token>Halteverbot</token>
                    </marker>
                </pattern>
                <filter class="org.languagetool.rules.de.InsertCommaFilter" args="fake:1"/>
                <message>Komma fehlt.</message>
                <suggestion><match no="1"/> <match no="2"/> <match no="3"/></suggestion>
            </rule>
        </rules>"#;
        let matches = compile_and_match(xml, "Das Auto steht im Halteverbot.");
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0].replacements().is_empty(),
            "no gate fires for VER+PRP+SUB"
        );
    }

    #[test]
    fn german_helper_predicates_match_the_java_patterns() {
        assert!(is_sagt("sagt"));
        assert!(is_sagt("Sagt"));
        assert!(is_sagt("sag"));
        assert!(is_sagt("Sag"));
        assert!(!is_sagt("sagen"));
        assert!(is_denke_etc("denke"));
        assert!(is_denke_etc("dachte"));
        assert!(is_denke_etc("glaube"));
        assert!(is_denke_etc("schätze"));
        assert!(is_denke_etc("vermute"));
        assert!(is_denke_etc("behaupte"));
        assert!(!is_denke_etc("denkst"));
        assert!(is_bei_fuer_mit("bei"));
        assert!(is_bei_fuer_mit("für"));
        assert!(is_bei_fuer_mit("mit"));
        assert!(!is_bei_fuer_mit("ohne"));
        assert!(is_dir_etc("dir"));
        assert!(is_dir_etc("Dir"));
        assert!(is_dir_etc("dich"));
        assert!(is_dir_etc("Dich"));
        assert!(is_dir_etc("euer"));
        assert!(is_dir_etc("Euch"));
        assert!(!is_dir_etc("eucheln"));
        let der_sentence = make_analyzed_sentence("meint die Karten");
        let refs: Vec<&AnalyzedTokenReadings> = der_sentence.tokens().iter().collect();
        assert!(is_der_etc(&refs));
        let folgender = make_analyzed_sentence("x folgender y");
        let refs: Vec<&AnalyzedTokenReadings> = folgender.tokens().iter().collect();
        assert!(is_der_etc(&refs));
        let keine = make_analyzed_sentence("nur ein Wort");
        let refs: Vec<&AnalyzedTokenReadings> = keine.tokens().iter().collect();
        assert!(!is_der_etc(&refs));
        assert!(!is_der_etc(&[]));
    }

    #[test]
    fn skipped_suffix_appends_tokens_between_pattern_matches() {
        let sentence = make_analyzed_sentence("steht da drüben parkt");
        let refs: Vec<&AnalyzedTokenReadings> = sentence.tokens().iter().collect();
        assert_eq!(skipped_suffix(&refs, &[0, 3], 0, true), "da drüben");
        assert_eq!(skipped_suffix(&refs, &[0, 3], 0, false), " da drüben");
        assert_eq!(skipped_suffix(&refs, &[0, 3], 1, true), "");
        assert_eq!(skipped_suffix(&refs, &[0], 5, true), "");
        assert_eq!(skipped_suffix(&refs, &[0, 99], 0, true), "da drüben parkt");
    }

    #[test]
    fn case_conversions_cover_the_java_set() {
        assert_eq!(apply_case_conversion("Berlin", "startlower"), "berlin");
        assert_eq!(apply_case_conversion("", "startlower"), "");
        assert_eq!(apply_case_conversion("berlin", "startupper"), "Berlin");
        assert_eq!(apply_case_conversion("", "startupper"), "");
        assert_eq!(apply_case_conversion("BerLin", "alllower"), "berlin");
        assert_eq!(apply_case_conversion("berlin", "allupper"), "BERLIN");
        assert_eq!(apply_case_conversion("berlin", "unknownmode"), "berlin");
        assert_eq!(apply_case_conversion("über", "startupper"), "Über");
    }

    #[test]
    fn diacritic_insensitive_equality_folds_accents_like_java() {
        assert!(equal_without_diacritics("café", "cafe"));
        assert!(equal_without_diacritics(
            "ÀÉÎÕ",
            "aeio".to_uppercase().as_str()
        ));
        assert!(equal_without_diacritics("señor", "senor"));
        assert!(equal_without_diacritics("ça", "ca"));
        assert!(!equal_without_diacritics("cafe", "cane"));
        assert!(!equal_without_diacritics("straße", "strase"));
    }

    fn synth_engine(module: &str) -> Option<PatternRuleEngine> {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules",
        )
        .join(module)
        .join("src/main/resources/org/languagetool/resource")
        .join(module);
        let synth = std::sync::Arc::new(crate::postponed_adjective::LangSynthesizer::for_language(
            &dir, module,
        )?);
        Some(
            PatternRuleEngine::default()
                .with_synthesizer(std::sync::Arc::clone(&synth))
                .with_suggestion_synthesizer(synth),
        )
    }

    #[test]
    fn en_determiner_suggestion_synthesizes_articles() {
        let Some(engine) = synth_engine("en") else {
            return;
        };
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="DT_MISSING" name="article missing">
                <pattern>
                    <token>is</token>
                    <marker><token>friend</token></marker>
                </pattern>
                <message>The article might be missing before the noun.</message>
                <suggestion><match no="2" postag="+DT"/></suggestion>
            </rule>
            <rule id="INDT_MISSING" name="indefinite article missing">
                <pattern>
                    <token>is</token>
                    <marker><token>friend</token></marker>
                </pattern>
                <message>The article might be missing before the noun.</message>
                <suggestion><match no="2" postag="+INDT"/></suggestion>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let sentence = make_analyzed_sentence("He is friend .");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(matches.len(), 2);
        let dt: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(dt, vec!["a friend", "the friend"]);
        let indt: Vec<&str> = matches[1]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(indt, vec!["a friend"]);
    }

    fn set_readings(
        sentence: &mut og_core::AnalyzedSentence,
        i: usize,
        readings: &[(Option<&str>, &str)],
    ) {
        let tokens = sentence.tokens_mut();
        let atr = &mut tokens[i];
        let token = atr.token().clone();
        let parsed: Vec<og_core::AnalyzedToken> = readings
            .iter()
            .map(|(lemma, tag)| {
                let mut at = og_core::AnalyzedToken::new(token.token(), token.start(), token.end())
                    .with_pos_tags(vec![tag.to_string()]);
                if let Some(lemma) = lemma {
                    at = at.with_lemma(lemma.to_string());
                }
                at
            })
            .collect();
        *atr = og_core::AnalyzedTokenReadings::new(token).with_readings(parsed);
    }

    fn en_suggestion_match(
        engine: &PatternRuleEngine,
        match_xml: &str,
        sentence: &og_core::AnalyzedSentence,
    ) -> Vec<String> {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="T" name="t">
                <pattern>
                    <token>is</token>
                    <marker><token regexp="yes">friends?</token></marker>
                </pattern>
                <message>m</message>
                <suggestion>{match_xml}</suggestion>
            </rule>
        </rules>"#
        );
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(&xml).unwrap();
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, sentence))
            .collect();
        matches
            .iter()
            .flat_map(|m| m.replacements().iter().map(|r| r.value().to_string()))
            .collect()
    }

    #[test]
    fn en_postag_suggestion_synthesizes_the_exact_form() {
        let Some(engine) = synth_engine("en") else {
            return;
        };
        let mut sentence = make_analyzed_sentence("He is friend .");
        set_reading(&mut sentence, 2, "friend", "NN");
        assert_eq!(
            en_suggestion_match(&engine, r#"<match no="2" postag="NNS"/>"#, &sentence),
            vec!["friends"]
        );
    }

    #[test]
    fn en_postag_suggestion_applies_case_conversion_to_synthesized_forms() {
        let Some(engine) = synth_engine("en") else {
            return;
        };
        let mut sentence = make_analyzed_sentence("He is friend .");
        set_reading(&mut sentence, 2, "friend", "NN");
        assert_eq!(
            en_suggestion_match(
                &engine,
                r#"<match no="2" postag="NNS" case_conversion="allupper"/>"#,
                &sentence
            ),
            vec!["FRIENDS"]
        );
    }

    #[test]
    fn invalid_postag_regexp_falls_back_to_the_parenthesized_token() {
        let Some(engine) = synth_engine("en") else {
            return;
        };
        let mut sentence = make_analyzed_sentence("He is friend .");
        set_reading(&mut sentence, 2, "friend", "NN");
        assert_eq!(
            en_suggestion_match(
                &engine,
                r#"<match no="2" postag="(" postag_regexp="yes"/>"#,
                &sentence
            ),
            vec!["(friend)"]
        );
    }

    #[test]
    fn tagless_readings_render_the_surface_form() {
        let Some(engine) = synth_engine("en") else {
            return;
        };
        let sentence = make_analyzed_sentence("He is friend .");
        assert_eq!(
            en_suggestion_match(
                &engine,
                r#"<match no="2" postag="NNS" postag_regexp="yes"/>"#,
                &sentence
            ),
            vec!["friend"]
        );
    }

    #[test]
    fn sentence_boundary_readings_render_the_surface_form_once() {
        let Some(engine) = synth_engine("en") else {
            return;
        };
        let mut sentence = make_analyzed_sentence("He is friend .");
        set_readings(&mut sentence, 2, &[(None, "SENT_END")]);
        assert_eq!(
            en_suggestion_match(
                &engine,
                r#"<match no="2" postag="NNS" postag_regexp="yes"/>"#,
                &sentence
            ),
            vec!["friend"]
        );
    }

    #[test]
    fn mixed_readings_synthesize_from_the_lemmatized_reading() {
        let Some(engine) = synth_engine("en") else {
            return;
        };
        let mut sentence = make_analyzed_sentence("He is friends .");
        set_readings(&mut sentence, 2, &[(None, "NNS"), (Some("friend"), "NN")]);
        assert_eq!(
            en_suggestion_match(
                &engine,
                r#"<match no="2" postag="NN.*" postag_regexp="yes"/>"#,
                &sentence
            ),
            vec!["friend"]
        );
    }

    #[test]
    fn postag_replace_without_matching_tags_replaces_the_pattern() {
        let Some(engine) = synth_engine("en") else {
            return;
        };
        let mut sentence = make_analyzed_sentence("He is friend .");
        set_reading(&mut sentence, 2, "friend", "NN");
        assert_eq!(
            en_suggestion_match(
                &engine,
                r#"<match no="2" postag="ZZ.*" postag_regexp="yes" postag_replace="QQ"/>"#,
                &sentence
            ),
            vec!["(friend)"]
        );
    }

    #[test]
    fn regexp_postag_suggestion_applies_case_conversion() {
        let Some(engine) = synth_engine("en") else {
            return;
        };
        let mut sentence = make_analyzed_sentence("He is friends .");
        set_reading(&mut sentence, 2, "friend", "NN");
        assert_eq!(
            en_suggestion_match(
                &engine,
                r#"<match no="2" postag="NN" postag_regexp="yes" case_conversion="allupper"/>"#,
                &sentence
            ),
            vec!["FRIEND"]
        );
    }

    #[test]
    fn de_dative_suggestion_synthesizes_the_matching_pronoun() {
        let Some(engine) = synth_engine("de") else {
            return;
        };
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="MIT_MICH" name="'mit mich'">
                <pattern>
                    <marker>
                        <token postag="PRP:.+:DAT" postag_regexp="yes"/>
                        <token postag="PRO:PER:AKK.*" postag_regexp="yes"/>
                    </marker>
                </pattern>
                <message>Nach '\1' steht der Dativ.</message>
                <suggestion>\1 <match no="2" postag="(.+)AKK(.*)" postag_regexp="yes" postag_replace="$1DAT$2"/></suggestion>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let mut sentence = make_analyzed_sentence("Du kommst mit mich .");
        set_reading(&mut sentence, 2, "mit", "PRP:VER:DAT");
        set_reading(&mut sentence, 3, "ich", "PRO:PER:AKK:SIN:ALG");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(matches.len(), 1, "pronoun after dative preposition matches");
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["mit mir"]);
    }

    #[test]
    fn de_comparative_suggestion_synthesizes_the_form() {
        let Some(engine) = synth_engine("de") else {
            return;
        };
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="JE_GROSS" name="'Je groß'">
                <pattern>
                    <token>Je</token>
                    <marker>
                        <token postag="ADJ:PRD:GRU" postag_regexp="yes" skip="-1"/>
                    </marker>
                    <token>desto</token>
                </pattern>
                <message>Meinten Sie die Komparativform?</message>
                <suggestion><match no="2" postag="ADJ:PRD:GRU" postag_replace="ADJ:PRD:KOM" postag_regexp="yes"/></suggestion>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let mut sentence = make_analyzed_sentence("Je groß , desto besser .");
        set_reading(&mut sentence, 1, "groß", "ADJ:PRD:GRU");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(matches.len(), 1);
        let reps: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(reps, vec!["größer"]);
    }

    #[test]
    fn suggestion_match_can_include_skipped_tokens() {
        let engine = PatternRuleEngine::default();
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="SKIPPED" name="skipped">
                <pattern>
                    <marker>
                        <token skip="-1">steht</token>
                        <token>parkt</token>
                    </marker>
                </pattern>
                <message>m</message>
                <suggestion><match include_skipped="all" no="1"/> <match no="2"/></suggestion>
            </rule>
            <rule id="SKIPPED_FOLLOWING" name="skipped following">
                <pattern>
                    <marker>
                        <token skip="-1">steht</token>
                        <token>parkt</token>
                    </marker>
                </pattern>
                <message>m</message>
                <suggestion><match include_skipped="following" no="1"/> <match no="2"/></suggestion>
            </rule>
            <rule id="SKIPPED_NONE" name="skipped none">
                <pattern>
                    <marker>
                        <token skip="-1">steht</token>
                        <token>parkt</token>
                    </marker>
                </pattern>
                <message>m</message>
                <suggestion><match include_skipped="none" no="1"/> <match no="2"/></suggestion>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let sentence = make_analyzed_sentence("Das Auto steht da drüben parkt im Halteverbot .");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(matches.len(), 3);
        let all: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(all, vec!["steht da drüben parkt"]);
        let following: Vec<&str> = matches[1]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(following, vec!["stehtda drüben parkt"]);
        let none: Vec<&str> = matches[2]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(none, vec!["steht parkt"]);
    }

    fn sentence_with_tokens(text: &str, pieces: &[&str]) -> og_core::AnalyzedSentence {
        use og_core::{AnalyzedToken, AnalyzedTokenReadings};
        let mut sentence = og_core::AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut pos = 0usize;
        for piece in pieces {
            let start = text[pos..].find(piece).map(|i| pos + i).unwrap_or(pos);
            let end = start + piece.len();
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                *piece, start, end,
            )));
            pos = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    fn match_tokens(xml: &str, text: &str, pieces: &[&str]) -> Vec<RuleMatch> {
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();
        let sentence = sentence_with_tokens(text, pieces);
        let mut matches = Vec::new();
        for rule in &rule_set.rules {
            matches.extend(engine.match_rule(rule, &sentence));
        }
        matches
    }

    #[test]
    fn ymd_new_year_date_filter_suggests_the_next_year_date() {
        set_test_date_hack(true);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="YMD_NEWYEAR" name="new year ymd">
                <pattern>
                    <marker><token regexp="yes">\d\d\d\d-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])</token></marker>
                </pattern>
                <filter class="org.languagetool.rules.en.YMDNewYearDateFilter" args="date:\1"/>
                <message>A new year has begun. Did you mean <suggestion>{realDate}</suggestion>?</message>
            </rule>
        </rules>"#;
        let matches = match_tokens(
            xml,
            "IGD Convention 2014 - 2013-10-31.",
            &["IGD", "Convention", "2014", "-", "2013-10-31", "."],
        );
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(
            m.message(),
            "A new year has begun. Did you mean 2014-10-31?"
        );
        let reps: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(reps, vec!["2014-10-31"]);
        assert_eq!(
            match_tokens(
                xml,
                "IGD Convention 2014 - 2014-10-31.",
                &["IGD", "Convention", "2014", "-", "2014-10-31", "."],
            )
            .len(),
            0,
            "same-year date is fine"
        );
        assert_eq!(
            match_tokens(
                xml,
                "IGD Convention 2014 - 2012-12-31.",
                &["IGD", "Convention", "2014", "-", "2012-12-31", "."],
            )
            .len(),
            0,
            "two years back is fine"
        );
    }

    #[test]
    fn en_date_check_filter_with_date_arg_uses_the_full_date_token() {
        set_test_date_hack(true);
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="EN_DATE_YMD" name="weekday vs ymd date">
                <pattern>
                    <marker>
                        <token regexp="yes">Monday|Friday</token>
                        <token>,</token>
                        <token regexp="yes">\d\d\d\d-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])</token>
                    </marker>
                </pattern>
                <filter class="org.languagetool.rules.en.DateCheckFilter" args="date:3 weekDay:1"/>
                <message>The date \3 (YYYY-MM-DD) is not a {day}, but a {realDay}.</message>
            </rule>
        </rules>"#;
        let text = "IGD Convention 2014 - Monday, 2014-10-31.";
        let matches = match_tokens(
            xml,
            text,
            &[
                "IGD",
                "Convention",
                "2014",
                "-",
                "Monday",
                ",",
                "2014-10-31",
                ".",
            ],
        );
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(
            m.message(),
            "The date \\3 (YYYY-MM-DD) is not a Monday, but a Friday."
        );
        let reps: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(reps, vec!["Friday, 2014-10-31", "Monday, 2014-10-27"]);
        assert_eq!(
            &text[m.offset()..m.offset() + m.length()],
            "Monday, 2014-10-31"
        );

        let text = "IGD Convention 2014 - Friday, 2014-10-31.";
        assert_eq!(
            match_tokens(
                xml,
                text,
                &[
                    "IGD",
                    "Convention",
                    "2014",
                    "-",
                    "Friday",
                    ",",
                    "2014-10-31",
                    "."
                ],
            )
            .len(),
            0,
            "consistent date"
        );

        let text = "Friday, 2013-08-15.";
        let matches = match_tokens(xml, text, &["Friday", ",", "2013-08-15", "."]);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.message(), "This date is wrong. Did you mean \"2014\"?");
        let reps: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(reps, vec!["2014-08-15"]);
        assert_eq!(&text[m.offset()..m.offset() + m.length()], "2013-08-15");
    }

    #[test]
    fn pattern_backreferences_cover_regex_replace_inflection_and_negation() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_REGEX" name="regex transformed backref">
                <pattern>
                    <marker>
                        <token regexp="yes">house7</token>
                        <token><match no="0" regexp_match="(\w+)7" regexp_replace="$1x"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        assert_eq!(compile_and_match(xml, "the house7 house.").len(), 0);
        assert_eq!(compile_and_match(xml, "the house7 housex.").len(), 1);

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_INFLECTED" name="inflected backref">
                <pattern>
                    <marker>
                        <token>run</token>
                        <token inflected="yes"><match no="0"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();
        let mut sentence = make_analyzed_sentence("they run running .");
        set_reading(&mut sentence, 2, "run", "VB");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "lemma 'run' satisfies the inflected backreference"
        );

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_NEGATE" name="negated backref">
                <pattern>
                    <marker>
                        <token>foo</token>
                        <token negate="yes"><match no="0"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        assert_eq!(compile_and_match(xml, "foo foo").len(), 0);
        assert_eq!(compile_and_match(xml, "foo bar").len(), 1);

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_CASE" name="case sensitive regexp backref">
                <pattern>
                    <marker>
                        <token regexp="yes">aa|bb</token>
                        <token regexp="yes" case_sensitive="yes"><match no="0"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        assert_eq!(compile_and_match(xml, "aa aa").len(), 1);
        assert_eq!(compile_and_match(xml, "aa AA").len(), 0);
    }

    #[test]
    fn pattern_backreference_postag_uses_the_raw_text_without_a_synthesizer() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_POSTAG_RAW" name="postag backref raw">
                <pattern>
                    <marker>
                        <token>Foo</token>
                        <token><match no="0" postag="VB" case_conversion="alllower"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        assert_eq!(
            compile_and_match(xml, "Foo foo").len(),
            1,
            "without a synthesizer the reference text is case-converted and compared raw"
        );
        assert_eq!(compile_and_match(xml, "Foo bar").len(), 0);
    }

    #[test]
    fn regexp_backreference_matches_case_insensitively_by_default() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_CI" name="case insensitive regexp backref">
                <pattern>
                    <marker>
                        <token regexp="yes">aa|bb</token>
                        <token regexp="yes"><match no="0"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        assert_eq!(compile_and_match(xml, "aa AA").len(), 1);
        assert_eq!(compile_and_match(xml, "aa CC").len(), 0);
    }

    #[test]
    fn inflected_regexp_backreference_matches_lemmas() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_INFLECTED_RE" name="inflected regexp backref">
                <pattern>
                    <marker>
                        <token>run</token>
                        <token inflected="yes" regexp="yes"><match no="0"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();

        let mut sentence = make_analyzed_sentence("they run running .");
        set_reading(&mut sentence, 2, "run", "VB");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(matches.len(), 1, "the lemma satisfies the regexp backref");

        let bare = make_analyzed_sentence("they run running .");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &bare))
            .collect();
        assert_eq!(
            matches.len(),
            0,
            "without a lemma the surface form does not satisfy the regexp backref"
        );
    }

    #[test]
    fn case_sensitive_inflected_backreference_compares_exact_case() {
        let cs_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_CS" name="case sensitive inflected backref">
                <pattern>
                    <marker>
                        <token>Run</token>
                        <token inflected="yes" case_sensitive="yes"><match no="0"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(cs_xml).unwrap();
        let engine = PatternRuleEngine::new();
        let mut sentence = make_analyzed_sentence("they Run run .");
        set_reading(&mut sentence, 2, "run", "VB");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(
            matches.len(),
            0,
            "case sensitive comparison rejects the lowercase lemma"
        );

        let ci_xml = cs_xml.replace(" case_sensitive=\"yes\"", "");
        let ci_rule_set = compiler.compile_file(&ci_xml).unwrap();
        let matches: Vec<_> = ci_rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn setpos_backreference_without_regexp_escapes_the_postag() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="SETPOS_LITERAL" name="setpos literal postag">
                <pattern>
                    <marker>
                        <token>dog</token>
                        <token><match no="0" postag="NN" setpos="yes" postag_replace="VB"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();

        let mut tagged = make_analyzed_sentence("the dog runs .");
        set_reading(&mut tagged, 1, "dog", "NN");
        set_reading(&mut tagged, 2, "runs", "VB");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &tagged))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "the literal postag is replaced and the target tag matched"
        );

        let mut untagged = make_analyzed_sentence("the dog runs .");
        set_reading(&mut untagged, 2, "runs", "VB");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &untagged))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "without reference tags the pattern itself is the replace base"
        );

        let mut wrong_target = make_analyzed_sentence("the dog runs .");
        set_reading(&mut wrong_target, 1, "dog", "NN");
        set_reading(&mut wrong_target, 2, "runs", "NN");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &wrong_target))
            .collect();
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn setpos_backreference_without_replace_targets_the_last_tag() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="SETPOS_LAST" name="setpos last tag">
                <pattern>
                    <marker>
                        <token>dog</token>
                        <token><match no="0" postag="N.*" postag_regexp="yes" setpos="yes"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();

        let mut sentence = make_analyzed_sentence("the dog barks .");
        set_reading(&mut sentence, 1, "dog", "NNP");
        set_reading(&mut sentence, 2, "barks", "NNP");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "the referenced reading's last tag becomes the target"
        );

        let mut other = make_analyzed_sentence("the dog barks .");
        set_reading(&mut other, 1, "dog", "NNP");
        set_reading(&mut other, 2, "barks", "NN");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &other))
            .collect();
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn setpos_backreference_residual_text_must_match_the_surface() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="SETPOS_RESIDUAL" name="setpos residual text">
                <pattern>
                    <marker>
                        <token>dog</token>
                        <token>x<match no="0" postag="N.*" postag_regexp="yes" setpos="yes"/>y</token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();

        let mut sentence = make_analyzed_sentence("the dog xy .");
        set_reading(&mut sentence, 1, "dog", "NNP");
        set_reading(&mut sentence, 2, "xy", "NNP");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "the residual text wraps the matched token"
        );

        let mut other_surface = make_analyzed_sentence("the dog zz .");
        set_reading(&mut other_surface, 1, "dog", "NNP");
        set_reading(&mut other_surface, 2, "zz", "NNP");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &other_surface))
            .collect();
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn regexp_match_backreference_applies_case_conversion() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_RE_CASE" name="regexp match with case conversion">
                <pattern>
                    <marker>
                        <token>house7</token>
                        <token case_sensitive="yes"><match no="0" regexp_match="(\w+)7" regexp_replace="$1x" case_conversion="allupper"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        assert_eq!(compile_and_match(xml, "the house7 HOUSEX .").len(), 1);
        assert_eq!(compile_and_match(xml, "the house7 housex .").len(), 0);
    }

    #[test]
    fn negated_regexp_match_backreference_inverts_the_result() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_RE_NEG" name="negated regexp match backref">
                <pattern>
                    <marker>
                        <token>house7</token>
                        <token negate="yes"><match no="0" regexp_match="(\w+)7" regexp_replace="$1x"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        assert_eq!(compile_and_match(xml, "the house7 housex .").len(), 0);
        assert_eq!(compile_and_match(xml, "the house7 other .").len(), 1);
    }

    #[test]
    fn postag_backreference_without_conversion_or_synthesizer_uses_the_reference_text() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_POSTAG_PLAIN" name="postag backref plain">
                <pattern>
                    <marker>
                        <token>Foo</token>
                        <token><match no="0" postag="VB"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        assert_eq!(compile_and_match(xml, "Foo Foo").len(), 1);
        assert_eq!(compile_and_match(xml, "Foo bar").len(), 0);

        let negated = xml.replace(
            "<token><match no=\"0\" postag=\"VB\"/></token>",
            "<token negate=\"yes\"><match no=\"0\" postag=\"VB\"/></token>",
        );
        assert_eq!(compile_and_match(&negated, "Foo Foo").len(), 0);
        assert_eq!(compile_and_match(&negated, "Foo bar").len(), 1);
    }

    #[test]
    fn postag_backreference_synthesizes_forms_with_the_engine_synthesizer() {
        let Some(engine) = synth_engine("en") else {
            return;
        };
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_POSTAG_SYNTH" name="postag backref synthesized">
                <pattern>
                    <marker>
                        <token>friend</token>
                        <token><match no="0" postag="NNS"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();

        let mut sentence = make_analyzed_sentence("He friend friends .");
        set_reading(&mut sentence, 1, "friend", "NN");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "the synthesized plural form satisfies the backreference"
        );

        let upper_xml = xml.replace(
            "postag=\"NNS\"",
            "postag=\"NNS\" case_conversion=\"allupper\"",
        );
        let upper_rule_set = compiler.compile_file(&upper_xml).unwrap();
        let mut upper = make_analyzed_sentence("He friend FRIENDS .");
        set_reading(&mut upper, 1, "friend", "NN");
        let matches: Vec<_> = upper_rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &upper))
            .collect();
        assert_eq!(matches.len(), 1, "case conversion applies to the forms");

        let negated_xml = xml.replace(
            "<token><match no=\"0\" postag=\"NNS\"/></token>",
            "<token negate=\"yes\"><match no=\"0\" postag=\"NNS\"/></token>",
        );
        let negated_rule_set = compiler.compile_file(&negated_xml).unwrap();
        let matches: Vec<_> = negated_rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(matches.len(), 0);

        let fallback_xml = xml.replace("postag=\"NNS\"", "postag=\"JJ\"");
        let fallback_rule_set = compiler.compile_file(&fallback_xml).unwrap();
        let mut fallback = make_analyzed_sentence("He friend friend .");
        set_reading(&mut fallback, 1, "friend", "NN");
        let matches: Vec<_> = fallback_rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &fallback))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "unsynthesizable postags fall back to the reference text"
        );
    }

    #[test]
    fn pattern_backreference_postag_synthesizes_with_the_engine_synthesizer() {
        let Some(engine) = synth_engine("en") else {
            return;
        };
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="BACKREF_POSTAG_SYNTH" name="postag backref synthesized">
                <pattern>
                    <marker>
                        <token>goes</token>
                        <token inflected="yes"><match no="1" postag="VB"/></token>
                    </marker>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let mut sentence = make_analyzed_sentence("goes went .");
        set_reading(&mut sentence, 0, "go", "VBZ");
        set_reading(&mut sentence, 1, "go", "VBD");
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "'went' is a VB form of the referenced lemma 'go'"
        );
    }

    #[test]
    fn pattern_trace_records_the_failure_causes() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="TRACE" name="trace">
                <pattern>
                    <token>hello</token>
                    <token>world</token>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();
        let sentence = make_analyzed_sentence("hello there friend");
        crate::pattern_trace::set_enabled(true);
        crate::pattern_trace::clear();
        let matches: Vec<_> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        let events = crate::pattern_trace::take();
        crate::pattern_trace::set_enabled(false);
        assert!(matches.is_empty());
        assert!(
            events
                .iter()
                .any(|e| e.cause == "surface!=\"world\"" && e.token == "there"),
            "events: {events:?}"
        );

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="TRACE_POSTAG" name="trace postag">
                <pattern>
                    <token postag="NN">hello</token>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let rule_set = compiler.compile_file(xml).unwrap();
        let sentence = make_analyzed_sentence("hello there");
        crate::pattern_trace::set_enabled(true);
        crate::pattern_trace::clear();
        let _ = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        let events = crate::pattern_trace::take();
        crate::pattern_trace::set_enabled(false);
        assert!(
            events
                .iter()
                .any(|e| e.cause.starts_with("postag=\"NN\"") && e.token == "hello"),
            "events: {events:?}"
        );

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="TRACE_NEGATE" name="trace negate">
                <pattern>
                    <token negate="yes">hello</token>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let rule_set = compiler.compile_file(xml).unwrap();
        let sentence = make_analyzed_sentence("hello world");
        crate::pattern_trace::set_enabled(true);
        crate::pattern_trace::clear();
        let _ = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        let events = crate::pattern_trace::take();
        crate::pattern_trace::set_enabled(false);
        assert!(
            events
                .iter()
                .any(|e| e.cause == "negated token matched surface \"hello\""),
            "events: {events:?}"
        );

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="TRACE_BACKREF" name="trace backref">
                <pattern>
                    <token>hello</token>
                    <token><match no="0"/></token>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let rule_set = compiler.compile_file(xml).unwrap();
        let sentence = make_analyzed_sentence("hello world");
        crate::pattern_trace::set_enabled(true);
        crate::pattern_trace::clear();
        let _ = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        let events = crate::pattern_trace::take();
        crate::pattern_trace::set_enabled(false);
        assert!(
            events
                .iter()
                .any(|e| e.cause == "backref-no=Some(0) mismatch"),
            "events: {events:?}"
        );

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="TRACE_IMMUNIZED" name="trace immunized">
                <pattern>
                    <token>hello</token>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let rule_set = compiler.compile_file(xml).unwrap();
        let mut sentence = make_analyzed_sentence("hello world");
        sentence.tokens_mut()[0].immunize();
        crate::pattern_trace::set_enabled(true);
        crate::pattern_trace::clear();
        let _ = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        let events = crate::pattern_trace::take();
        crate::pattern_trace::set_enabled(false);
        assert!(
            events.iter().any(|e| e.cause == "immunized"),
            "events: {events:?}"
        );
    }

    #[test]
    fn pattern_trace_records_or_group_and_exception_failures() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="TRACE_OR" name="trace or">
                <pattern>
                    <or>
                        <token>alpha</token>
                        <token>beta</token>
                    </or>
                    <token>gamma</token>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();
        let sentence = make_analyzed_sentence("alpha delta");
        crate::pattern_trace::set_enabled(true);
        crate::pattern_trace::clear();
        let _ = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        let events = crate::pattern_trace::take();
        crate::pattern_trace::set_enabled(false);
        assert!(
            events.iter().any(|e| e.cause == "surface!=\"gamma\""),
            "or-group then failing next token: {events:?}"
        );

        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="TRACE_EXC" name="trace exception">
                <pattern>
                    <token>foo<exception>foo</exception></token>
                </pattern>
                <message>m</message>
            </rule>
        </rules>"#;
        let rule_set = compiler.compile_file(xml).unwrap();
        let sentence = make_analyzed_sentence("foo bar");
        crate::pattern_trace::set_enabled(true);
        crate::pattern_trace::clear();
        let _ = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        let events = crate::pattern_trace::take();
        crate::pattern_trace::set_enabled(false);
        assert!(
            events
                .iter()
                .any(|e| e.cause.starts_with("exception scope=")),
            "events: {events:?}"
        );
    }

    #[test]
    fn test_simple_match() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>hello</token>
                        <token>world</token>
                    </pattern>
                    <message>Use <suggestion>goodbye world</suggestion></message>
                </rule>
            </category>
        </rules>"#;
        let matches = compile_and_match(xml, "hello world");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].message(), "Use goodbye world");
    }

    #[test]
    fn test_no_match() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>hello</token>
                        <token>world</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "foo bar");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_regexp_match() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token regexp="yes">hel+o</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "hello");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_negate_match() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token negate="yes">bad</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "good");
        assert_eq!(matches.len(), 1);

        let no_matches = compile_and_match(xml, "bad");
        assert!(no_matches.is_empty());
    }

    #[test]
    fn test_antipattern_blocks_match() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <antipattern>
                        <token>deep</token>
                        <token>clean</token>
                    </antipattern>
                    <pattern>
                        <token>clean</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "clean the house");
        assert_eq!(matches.len(), 1);

        let blocked = compile_and_match(xml, "deep clean");
        assert!(blocked.is_empty());
    }

    #[test]
    fn test_antipattern_with_inflected() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <antipattern>
                        <token>free</token>
                        <token inflected="yes">fall</token>
                    </antipattern>
                    <pattern>
                        <token>fall</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let blocked = compile_and_match(xml, "free fall");
        assert!(blocked.is_empty());
    }

    #[test]
    fn test_marker_limits_error_span() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>the</token>
                        <marker>
                            <token>bad</token>
                        </marker>
                        <token>word</token>
                    </pattern>
                    <message>Fix <suggestion>good</suggestion></message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "the bad word");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 4);
        assert_eq!(matches[0].length(), 3);
    }

    #[test]
    fn test_backreference_in_suggestion() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>foo</token>
                        <token>bar</token>
                    </pattern>
                    <message>Use <suggestion>\1 \2</suggestion></message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "foo bar");
        assert_eq!(matches.len(), 1);
        let repl: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(repl, vec!["foo bar"]);
    }

    #[test]
    fn test_backreference_with_literal() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>Ponzi</token>
                        <token regexp="yes">schemes?</token>
                    </pattern>
                    <message>Use <suggestion>Ponzi \2</suggestion></message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "Ponzi scheme");
        assert_eq!(matches.len(), 1);
        let repl: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(repl, vec!["Ponzi scheme"]);
    }

    #[test]
    fn test_skip_tokens() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token skip="2">start</token>
                        <token>end</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "start the end");
        assert_eq!(matches.len(), 1);

        let direct = compile_and_match(xml, "start end");
        assert_eq!(direct.len(), 1);
    }

    #[test]
    fn test_optional_token_java_semantics() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>a</token>
                        <token min="0">b</token>
                        <token>c</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        assert_eq!(compile_and_match(xml, "a c").len(), 1);
        assert_eq!(compile_and_match(xml, "a b c").len(), 1);
        assert_eq!(compile_and_match(xml, "a X c").len(), 0);
        assert_eq!(compile_and_match(xml, "a b b c").len(), 0);
        assert_eq!(compile_and_match(xml, "a").len(), 0);
        assert_eq!(compile_and_match(xml, "c a b").len(), 0);
    }

    #[test]
    fn test_optional_token_not_skipped_when_rest_fails() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>a</token>
                        <token min="0">b</token>
                        <token>c</token>
                        <token>d</token>
                        <token>e</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        assert_eq!(compile_and_match(xml, "a b c d e").len(), 1);
        assert_eq!(compile_and_match(xml, "a c d e").len(), 1);
        assert_eq!(compile_and_match(xml, "a d").len(), 0);
        assert_eq!(compile_and_match(xml, "a c b d").len(), 0);
        assert_eq!(compile_and_match(xml, "a c b d e").len(), 0);
    }

    #[test]
    fn test_optional_first_token_matches_when_skipped() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token min="0">please</token>
                        <token>help</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        assert_eq!(compile_and_match(xml, "help").len(), 1);
        assert_eq!(compile_and_match(xml, "please help").len(), 2);
        assert_eq!(compile_and_match(xml, "please").len(), 0);
    }

    #[test]
    fn test_optional_token_in_structured_pattern() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <or>
                            <token>start</token>
                            <token>begin</token>
                        </or>
                        <token min="0">maybe</token>
                        <token>end</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        assert_eq!(compile_and_match(xml, "start end").len(), 1);
        assert_eq!(compile_and_match(xml, "start maybe end").len(), 1);
        assert_eq!(compile_and_match(xml, "start X end").len(), 0);
        assert_eq!(compile_and_match(xml, "begin end").len(), 1);
    }

    #[test]
    fn test_optional_token_min_zero() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>hello</token>
                        <token min="0">the</token>
                        <token>world</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let with = compile_and_match(xml, "hello the world");
        assert_eq!(with.len(), 1);

        let without = compile_and_match(xml, "hello world");
        assert_eq!(without.len(), 1);
    }

    #[test]
    fn test_exception_scope_next() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>word<exception scope="next">end</exception></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "word start");
        assert_eq!(matches.len(), 1);

        let blocked = compile_and_match(xml, "word end");
        assert!(blocked.is_empty());
    }

    #[test]
    fn exception_spacebefore_no_only_kills_match_for_attached_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token regexp="yes">word|term<exception spacebefore="no">term</exception></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        assert_eq!(
            compile_and_match(xml, "word term").len(),
            2,
            "spaced 'term' must not trigger the spacebefore=no exception"
        );
        assert_eq!(
            compile_and_match(xml, "word-term").len(),
            1,
            "attached 'term' must be excluded by the spacebefore=no exception"
        );
    }

    #[test]
    fn exception_spacebefore_no_with_scope_next_gates_on_next_token_whitespace() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>-<exception scope="next" spacebefore="no">term</exception></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        assert!(
            compile_and_match(xml, "word-term").is_empty(),
            "exception applies when the next token is attached"
        );
        assert_eq!(
            compile_and_match(xml, "word - term").len(),
            1,
            "exception must stay inactive when the next token has whitespace before"
        );
    }

    #[test]
    fn optional_element_matches_even_at_the_token_tail_bound() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>what</token>
                        <token min="0" regexp="yes">else|ever</token>
                        <token>she</token>
                        <token>doing</token>
                        <token>?</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;
        assert_eq!(
            compile_and_match(xml, "What else she doing?").len(),
            1,
            "the optional element's first occurrence must ignore the remaining-elements bound"
        );
    }

    #[test]
    fn regexp_element_matches_the_raw_typographic_quote_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>its</token>
                        <token>that</token>
                        <token regexp="yes">[“”„]|‘</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;
        assert_eq!(
            compile_and_match(xml, "its that ‘collection’ element").len(),
            1,
            "regexes containing typographic quotes must match the raw token text"
        );
    }

    #[test]
    fn exception_text_and_postag_must_hold_on_one_reading() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>word<exception inflected="yes" regexp="yes" postag="NN">run[sd]?</exception></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();

        let mut sentence = make_analyzed_sentence("word");
        let tokens = sentence.tokens_mut();
        let atr = &mut tokens[0];
        let token = atr.token().clone();
        let run_reading = og_core::AnalyzedToken::new(token.token(), token.start(), token.end())
            .with_lemma("runs".to_string())
            .with_pos_tags(vec!["VB".to_string()]);
        let noun_reading = og_core::AnalyzedToken::new(token.token(), token.start(), token.end())
            .with_lemma("walk".to_string())
            .with_pos_tags(vec!["NN".to_string()]);
        *atr = og_core::AnalyzedTokenReadings::new(token)
            .with_readings(vec![run_reading, noun_reading]);

        let matches = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        assert_eq!(matches, 1);
    }

    #[test]
    fn exception_fires_when_one_reading_satisfies_text_and_postag() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>word<exception inflected="yes" regexp="yes" postag="NN">run[sd]?</exception></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();

        let mut sentence = make_analyzed_sentence("word");
        let tokens = sentence.tokens_mut();
        let atr = &mut tokens[0];
        let token = atr.token().clone();
        let run_reading = og_core::AnalyzedToken::new(token.token(), token.start(), token.end())
            .with_lemma("runs".to_string())
            .with_pos_tags(vec!["NN".to_string()]);
        let verb_reading = og_core::AnalyzedToken::new(token.token(), token.start(), token.end())
            .with_lemma("walk".to_string())
            .with_pos_tags(vec!["VB".to_string()]);
        *atr = og_core::AnalyzedTokenReadings::new(token)
            .with_readings(vec![run_reading, verb_reading]);

        let matches = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        assert_eq!(matches, 0);
    }

    #[test]
    fn exception_negated_postag_must_hold_on_one_reading() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>word<exception inflected="yes" regexp="yes" postag="NN" negate_pos="yes">run[sd]?</exception></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();

        let mut sentence = make_analyzed_sentence("word");
        let tokens = sentence.tokens_mut();
        let atr = &mut tokens[0];
        let token = atr.token().clone();
        let run_reading = og_core::AnalyzedToken::new(token.token(), token.start(), token.end())
            .with_lemma("runs".to_string())
            .with_pos_tags(vec!["NN".to_string()]);
        let verb_reading = og_core::AnalyzedToken::new(token.token(), token.start(), token.end())
            .with_lemma("walk".to_string())
            .with_pos_tags(vec!["VB".to_string()]);
        *atr = og_core::AnalyzedTokenReadings::new(token)
            .with_readings(vec![run_reading, verb_reading]);

        let matches = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        assert_eq!(matches, 1);
    }

    #[test]
    fn negated_inflected_exception_uses_lemma_not_surface() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>unit</token>
                        <token>i<exception scope="next" negate="yes" inflected="yes" postag="VB[DP]|MD" postag_regexp="yes">be</exception></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();

        let mut sentence = make_analyzed_sentence("unit i is");
        let tokens = sentence.tokens_mut();
        let atr = &mut tokens[2];
        let token = atr.token().clone();
        let reading = og_core::AnalyzedToken::new(token.token(), token.start(), token.end())
            .with_lemma("be".to_string())
            .with_pos_tags(vec!["VBZ".to_string()]);
        *atr = og_core::AnalyzedTokenReadings::new(token).with_readings(vec![reading]);

        let matches = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .count();
        assert_eq!(matches, 1);
    }

    #[test]
    fn inflected_falls_back_to_surface_for_composed_compound_lemma() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token regexp="yes">statische[snm]?</token>
                        <token regexp="yes" inflected="yes">Bundesamt|Landesamt</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let mut sentence = make_analyzed_sentence("Statische Bundesamt");
        let tokens = sentence.tokens_mut();
        let atr = &mut tokens[1];
        let token = atr.token().clone();
        let reading = og_core::AnalyzedToken::new(token.token(), token.start(), token.end())
            .with_lemma("Bundsamt".to_string())
            .with_pos_tags(vec!["SUB:NOM:SIN:MAS".to_string()]);
        *atr = og_core::AnalyzedTokenReadings::new(token).with_readings(vec![reading]);

        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();
        let matches = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_negate_without_content_never_matches() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>start</token>
                        <token negate="yes"/>
                        <token>end</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "start middle end");
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_negated_backreference_matches_other_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>foo</token>
                        <token negate="yes"><match no="0"/></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "foo bar");
        assert_eq!(matches.len(), 1);

        let same = compile_and_match(xml, "foo foo");
        assert_eq!(same.len(), 0);
    }

    #[test]
    fn test_multiple_antipatterns() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <antipattern>
                        <token>nordic</token>
                        <token>ski</token>
                    </antipattern>
                    <antipattern>
                        <token>water</token>
                        <token>ski</token>
                    </antipattern>
                    <pattern>
                        <token>ski</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "I like to ski");
        assert_eq!(matches.len(), 1);

        let blocked1 = compile_and_match(xml, "nordic ski");
        assert!(blocked1.is_empty());

        let blocked2 = compile_and_match(xml, "water ski");
        assert!(blocked2.is_empty());
    }

    #[test]
    fn test_antipattern_bridge_token_does_not_block() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <antipattern>
                        <token>bridge</token>
                    </antipattern>
                    <pattern>
                        <token skip="-1">alpha</token>
                        <token>omega</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "alpha bridge omega");
        assert_eq!(
            matches.len(),
            1,
            "antipattern on a skip-bridged token must not block the rule match"
        );
    }

    #[test]
    fn test_antipattern_span_includes_bridged_tokens() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <antipattern>
                        <token skip="-1">alpha</token>
                        <token>omega</token>
                    </antipattern>
                    <pattern>
                        <token>bridge</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let blocked = compile_and_match(xml, "alpha bridge omega");
        assert!(
            blocked.is_empty(),
            "antipattern immunization spans the bridged tokens, so the rematch cannot match"
        );
    }

    #[test]
    fn test_antipattern_match_outside_rule_range_keeps_match() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <antipattern>
                        <token>never</token>
                        <token>happens</token>
                    </antipattern>
                    <pattern>
                        <token>clean</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "clean the house");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_marker_with_multiple_tokens() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>I</token>
                        <marker>
                            <token>wants</token>
                            <token>a</token>
                        </marker>
                        <token>drink</token>
                    </pattern>
                    <message>Use <suggestion>I want a</suggestion></message>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "I wants a drink");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 2);
        assert_eq!(matches[0].length(), 7);
    }

    #[test]
    fn test_pattern_rule_match_type_follows_issue_type() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="STYLE_KIND" name="Test">
                    <pattern>
                        <token>teh</token>
                    </pattern>
                    <message>Did you mean <suggestion>the</suggestion>?</message>
                </rule>
            </category>
        </rules>"#;
        let compiler = og_xml::compiler::XmlCompiler::new();
        let mut rule_set = compiler.compile_file(xml).unwrap();
        let sentence = make_analyzed_sentence("teh word");

        let mut style_rule = rule_set.rules.remove(0);
        style_rule.issue_type = "style".to_string();
        let matches = PatternRuleEngine::new().match_rule(&style_rule, &sentence);
        assert_eq!(matches.len(), 1);
        let json = serde_json::to_value(&matches[0]).unwrap();
        assert_eq!(json["type"]["typeName"], "Hint");
        assert_eq!(json["rule"]["issueType"], "style");

        let mut grammar_rule = style_rule.clone();
        grammar_rule.issue_type = "grammar".to_string();
        let matches = PatternRuleEngine::new().match_rule(&grammar_rule, &sentence);
        let json = serde_json::to_value(&matches[0]).unwrap();
        assert_eq!(json["type"]["typeName"], "Other");
    }

    #[test]
    fn test_parallel_matching() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>error</token>
                    </pattern>
                    <message>Fix</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = og_xml::compiler::XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let rules: Vec<&og_xml::compiler::CompiledRule> = rule_set.rules.iter().collect();

        let mut engine = PatternRuleEngine::new();
        engine.build_index(&rules);

        let sentences: Vec<og_core::AnalyzedSentence> = vec![
            make_analyzed_sentence("this is an error here"),
            make_analyzed_sentence("no problems here"),
            make_analyzed_sentence("another error found"),
        ];

        let matches = engine.match_indexed_rules_parallel(&rules, &sentences);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_indexed_rules_performance() {
        let grammar_path = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/grammar.xml";
        if !std::path::Path::new(grammar_path).exists() {
            eprintln!("Skipping benchmark: grammar.xml not found");
            return;
        }

        let xml = std::fs::read_to_string(grammar_path).unwrap();
        let compiler = og_xml::compiler::XmlCompiler::new();
        let rule_set = compiler.compile_file(&xml).unwrap();
        let rules: Vec<&og_xml::compiler::CompiledRule> = rule_set.rules.iter().collect();

        let mut engine = PatternRuleEngine::new();
        engine.build_index(&rules);

        eprintln!(
            "Indexed {} rules, {} unique first-tokens, {} catch-all rules",
            rules.len(),
            engine.word_index.len(),
            engine.catch_all_rule_indices.len()
        );

        let sentence = make_analyzed_sentence("The quick brown fox jumps over the lazy dog.");

        let start = std::time::Instant::now();
        let matches = engine.match_indexed_rules(&rules, &sentence);
        let elapsed = start.elapsed();
        eprintln!(
            "Matched {} rules against test sentence in {:?}",
            rules.len(),
            elapsed
        );
        eprintln!("Found {} matches", matches.len());

        assert!(
            elapsed.as_millis() < 5000,
            "Matching took too long: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_ordinal_suffix_filter() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="ORDINAL_SUFFIX" name="Ordinal suffix">
                    <pattern>
                        <token regexp="yes">[0-9]+(st|nd|rd|th)</token>
                    </pattern>
                    <message>Did you mean <suggestion>\1</suggestion>?</message>
                    <filter class="org.languagetool.rules.en.OrdinalSuffixFilter"/>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "The 21nd of May");
        assert!(!matches.is_empty());
        let repl: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(repl.contains(&"21st"), "Expected '21st', got {:?}", repl);

        let matches = compile_and_match(xml, "The 22st of May");
        assert!(!matches.is_empty());
        let repl: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(repl.contains(&"22nd"), "Expected '22nd', got {:?}", repl);

        let matches = compile_and_match(xml, "The 11nd item");
        assert!(!matches.is_empty());
        let repl: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(repl.contains(&"11th"), "Expected '11th', got {:?}", repl);

        let matches = compile_and_match(xml, "The 3th item");
        assert!(!matches.is_empty());
        let repl: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(repl.contains(&"3rd"), "Expected '3rd', got {:?}", repl);
    }

    #[test]
    fn test_underline_spaces_filter_before() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="UNDERLINE_BEFORE" name="Underline before">
                    <pattern>
                        <token>hello</token>
                    </pattern>
                    <message>Test</message>
                    <filter class="org.languagetool.rules.UnderlineSpacesFilter" args="underlineSpaces:before"/>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "say hello world");
        assert!(!matches.is_empty());
        assert_eq!(
            matches[0].offset(),
            3,
            "Expected offset 3 (including preceding space)"
        );
    }

    #[test]
    fn test_underline_spaces_filter_after() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="UNDERLINE_AFTER" name="Underline after">
                    <pattern>
                        <token>hello</token>
                    </pattern>
                    <message>Test</message>
                    <filter class="org.languagetool.rules.UnderlineSpacesFilter" args="underlineSpaces:after"/>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "say hello world");
        assert!(!matches.is_empty());
        assert_eq!(
            matches[0].length(),
            6,
            "Expected length 6 (including trailing space)"
        );
    }

    #[test]
    fn test_adverb_filter() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="ADVERB_TEST" name="Adverb test">
                    <pattern>
                        <token>philosophically</token>
                        <token>question</token>
                    </pattern>
                    <message>Use adjective, not adverb.</message>
                    <suggestion>\1 \2</suggestion>
                    <filter class="org.languagetool.rules.en.AdverbFilter" args="adverb:\1 noun:\2"/>
                </rule>
            </category>
        </rules>"#;

        let matches = compile_and_match(xml, "a philosophically question here");
        assert!(!matches.is_empty());
        let repl: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(
            repl.contains(&"philosophical question"),
            "Expected 'philosophical question', got {:?}",
            repl
        );
    }
}

#[cfg(test)]
mod gap_semantics_tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
    use og_xml::compiler::XmlCompiler;

    fn sentence(words: &[&str]) -> AnalyzedSentence {
        sentence_with_untagged(words, &[])
    }

    fn sentence_with_untagged(words: &[&str], untagged: &[usize]) -> AnalyzedSentence {
        let text = words.join(" ");
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for (i, w) in words.iter().enumerate() {
            let atr = if untagged.contains(&i) {
                AnalyzedTokenReadings::new(AnalyzedToken::new(*w, offset, offset + w.len()))
            } else {
                let reading = AnalyzedToken::new(*w, offset, offset + w.len())
                    .with_lemma((*w).to_string())
                    .with_pos_tags(vec!["NN".to_string()]);
                let primary = AnalyzedToken::new(*w, offset, offset + w.len())
                    .with_pos_tags(vec!["NN".to_string()]);
                AnalyzedTokenReadings::new(primary).with_readings(vec![reading])
            };
            tokens.push(atr);
            offset += w.len() + 1;
        }
        let mut s = AnalyzedSentence::new(&text, 0, text.len());
        s.set_tokens(tokens);
        s
    }

    fn count_matches(pattern: &str, words: &[&str]) -> usize {
        let xml = format!(
            r#"<rules lang="de"><rule id="GAP" name="gap"><pattern>{pattern}</pattern><message>m</message></rule></rules>"#
        );
        let rules = XmlCompiler::new().compile_file(&xml).expect("compile");
        PatternRuleEngine::new()
            .match_rule(&rules.rules[0], &sentence(words))
            .len()
    }

    fn count_matches_untagged(pattern: &str, words: &[&str], untagged: &[usize]) -> usize {
        let xml = format!(
            r#"<rules lang="de"><rule id="GAP" name="gap"><pattern>{pattern}</pattern><message>m</message></rule></rules>"#
        );
        let rules = XmlCompiler::new().compile_file(&xml).expect("compile");
        PatternRuleEngine::new()
            .match_rule(&rules.rules[0], &sentence_with_untagged(words, untagged))
            .len()
    }

    #[test]
    fn untagged_token_matches_pos_free_elements_but_not_postag_elements() {
        let words = ["Ich", "geb", "'", "mir"];
        let string_pattern = r#"<token>geb</token><token>'</token><token>mir</token>"#;
        assert_eq!(
            count_matches_untagged(string_pattern, &words, &[2]),
            1,
            "Java tokens always carry a no-info reading, so POS-free string elements match"
        );
        let postag_pattern = r#"<token>geb</token><token postag="PKT"/><token>mir</token>"#;
        assert_eq!(count_matches_untagged(postag_pattern, &words, &[2]), 0);
        let wildcard = r#"<token>geb</token><token/><token>mir</token>"#;
        assert_eq!(count_matches_untagged(wildcard, &words, &[2]), 1);
    }

    #[test]
    fn greedy_gap_leaves_tokens_for_remaining_elements() {
        let pattern = r#"<token>start</token><token min="0" max="6"/><token>end</token>"#;
        assert_eq!(count_matches(pattern, &["start", "x", "y", "z", "end"]), 1);
    }

    #[test]
    fn greedy_gap_may_swallow_required_token() {
        let pattern = r#"<token>start</token><token min="0" max="6"/><token>end</token>"#;
        assert_eq!(
            count_matches(pattern, &["start", "x", "end", "y"]),
            0,
            "gap greedily consumes up to the tail bound, no backtracking"
        );
    }

    #[test]
    fn greedy_gap_then_optional_element_skips_in_place() {
        let pattern = r#"<token>start</token><token min="0" max="6"/><token min="0">nicht</token><token>end</token>"#;
        assert_eq!(
            count_matches(pattern, &["start", "x", "nicht", "end"]),
            1,
            "optional element consumed when present"
        );
        assert_eq!(
            count_matches(pattern, &["start", "x", "end"]),
            1,
            "optional element skipped when the next element matches in its place"
        );
        assert_eq!(
            count_matches(pattern, &["start", "x", "y", "z", "end"]),
            0,
            "optional-element skip does not slide forward past the tail bound"
        );
    }

    #[test]
    fn german_auf_dem_punkt_shape_matches() {
        let pattern = r#"<token>start</token><token min="0" max="6"/><token min="0">nicht</token><token>auf</token><token>dem</token><token>end</token>"#;
        assert_eq!(
            count_matches(
                pattern,
                &["start", "x", "y", "z", "w", "auf", "dem", "end", "!"]
            ),
            1
        );
    }
}

#[cfg(test)]
mod unify_tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
    use og_xml::compiler::XmlCompiler;

    fn compile(xml: &str) -> Vec<CompiledRule> {
        XmlCompiler::new().compile_file(xml).expect("compile").rules
    }

    fn sentence(parts: &[(&str, &[&str])]) -> AnalyzedSentence {
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for (text, tags) in parts {
            let readings: Vec<AnalyzedToken> = tags
                .iter()
                .map(|t| {
                    AnalyzedToken::new(*text, offset, offset + text.len())
                        .with_pos_tags(vec![t.to_string()])
                })
                .collect();
            let primary = AnalyzedToken::new(*text, offset, offset + text.len())
                .with_pos_tags(tags.iter().map(|t| t.to_string()).collect());
            tokens.push(AnalyzedTokenReadings::new(primary).with_readings(readings));
            offset += text.len() + 1;
        }
        let mut s = AnalyzedSentence::new("", 0, offset);
        s.set_tokens(tokens);
        s
    }

    fn run(xml: &str, parts: &[(&str, &[&str])]) -> usize {
        let rules = compile(xml);
        PatternRuleEngine::new()
            .match_rule(&rules[0], &sentence(parts))
            .len()
    }

    const DEFS: &str = r#"
    <unification feature="number">
        <equivalence type="sg"><token postag="NN.*:sg.*|VB.*:sg.*" postag_regexp="yes"/></equivalence>
        <equivalence type="pl"><token postag="NN.*:pl.*|VB.*:pl.*" postag_regexp="yes"/></equivalence>
    </unification>
    <unification feature="gender">
        <equivalence type="m"><token postag="(NN|VB).*:m.*" postag_regexp="yes"/></equivalence>
        <equivalence type="f"><token postag="(NN|VB).*:f.*" postag_regexp="yes"/></equivalence>
    </unification>"#;

    fn xml(unify_body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="pl">{DEFS}
    <category id="GRAMMAR" name="Grammar">
        <rule id="UNIFY_TEST" name="unify">
            <pattern>{unify_body}</pattern>
            <message>Unification.</message>
        </rule>
    </category>
</rules>"#
        )
    }

    #[test]
    fn bare_unify_enforces_all_declared_features() {
        let body = r#"<unify><token postag="VB.*" postag_regexp="yes"/><token postag="NN.*:pl.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(&x, &[("widzi", &["VB:sg:1:f"]), ("psy", &["NN:pl:m1"])]),
            0
        );
        assert_eq!(
            run(&x, &[("widz", &["VB:pl:1:f"]), ("psy", &["NN:pl:f1"])]),
            1
        );
    }

    #[test]
    fn self_closing_feature_limits_unification_to_that_feature() {
        let body = r#"<unify negate="yes"><feature id="number"/><token postag="VB.*" postag_regexp="yes"/><token postag="NN.*:pl.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(&x, &[("widz", &["VB:pl:1:f"]), ("psy", &["NN:pl:m1"])]),
            0
        );
        assert_eq!(
            run(&x, &[("widzi", &["VB:sg:1:f"]), ("psy", &["NN:pl:f1"])]),
            1
        );
    }

    #[test]
    fn unify_uses_any_reading_of_a_token() {
        let body = r#"<unify><token postag="VB.*" postag_regexp="yes"/><token postag="NN.*:pl.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(
                &x,
                &[
                    ("widzi", &["VB:sg:1:f", "VB:pl:3:f"]),
                    ("psy", &["NN:pl:f1"])
                ]
            ),
            1
        );
    }

    #[test]
    fn same_reading_must_satisfy_every_feature() {
        let body = r#"<unify><token postag="NN.*" postag_regexp="yes"/><token postag="NN.*:pl.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(
                &x,
                &[("dom", &["NN:sg:m", "NN:pl:f"]), ("psy", &["NN:pl:n1"])]
            ),
            0,
            "no single reading of 'dom' is both plural and gender-consistent"
        );
    }

    #[test]
    fn negated_unify_inverts_the_constraint() {
        let body = r#"<unify negate="yes"><token postag="VB.*" postag_regexp="yes"/><token postag="NN.*:pl.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(&x, &[("widzi", &["VB:sg:1:f"]), ("psy", &["NN:pl:m1"])]),
            1
        );
        assert_eq!(
            run(&x, &[("widz", &["VB:pl:1:f"]), ("psy", &["NN:pl:f1"])]),
            0
        );
    }

    #[test]
    fn unify_ignore_tokens_are_neutral() {
        let body = r#"<unify><token postag="VB.*" postag_regexp="yes"/><unify-ignore><token>psy</token></unify-ignore><token postag="NN.*:pl.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(
                &x,
                &[
                    ("widzi", &["VB:sg:1:f"]),
                    ("psy", &["NN:pl:m1"]),
                    ("psy", &["NN:pl:m1"])
                ]
            ),
            0,
            "the neutral token must not help the sg/pl mismatch"
        );
        assert_eq!(
            run(
                &x,
                &[
                    ("widz", &["VB:pl:1:f"]),
                    ("psy", &["NN:pl:f1"]),
                    ("psy", &["NN:pl:f1"])
                ]
            ),
            1
        );
    }

    #[test]
    fn feature_restriction_limits_the_unification() {
        let body = r#"<unify><feature id="number"><type id="sg"/></feature><token postag="NN.*" postag_regexp="yes"/><token postag="NN.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(run(&x, &[("dom", &["NN:sg:m"]), ("dom", &["NN:sg:f"])]), 1);
        assert_eq!(run(&x, &[("dom", &["NN:sg:m"]), ("psy", &["NN:pl:n1"])]), 0);
    }

    #[test]
    fn undefined_feature_fails_the_group() {
        let body = r#"<unify><feature id="casus"/><token postag="VB.*" postag_regexp="yes"/><token postag="NN.*:pl.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(&x, &[("widz", &["VB:pl:1"]), ("psy", &["NN:pl:n1"])]),
            0
        );
    }

    #[test]
    fn feature_pairs_must_come_from_one_reading_per_token() {
        let body = r#"<unify><token postag="NN.*" postag_regexp="yes"/><token postag="NN.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(
                &x,
                &[("dom", &["NN:sg:m", "NN:pl:f"]), ("lamperia", &["NN:sg:f"])]
            ),
            0,
            "sg from one reading and f from the other must not combine"
        );
    }

    #[test]
    fn branch_narrowing_survives_only_common_types() {
        let body = r#"<unify><token postag="NN.*" postag_regexp="yes"/><token postag="NN.*" postag_regexp="yes"/><token postag="NN.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(
                &x,
                &[
                    ("dom", &["NN:sg:m", "NN:pl:f"]),
                    ("lamperia", &["NN:pl:f"]),
                    ("kot", &["NN:sg:m"])
                ]
            ),
            0,
            "the middle token narrows both branches away"
        );
        assert_eq!(
            run(
                &x,
                &[
                    ("dom", &["NN:sg:m", "NN:pl:f"]),
                    ("lamperia", &["NN:pl:f"]),
                    ("lamperie", &["NN:pl:f"])
                ]
            ),
            1
        );
    }

    #[test]
    fn negated_single_token_group_never_matches() {
        let body = r#"<unify negate="yes"><token postag="NN.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(run(&x, &[("dom", &["NN:sg:m"])]), 0);
    }

    #[test]
    fn antipattern_unify_ignore_tokens_are_neutral() {
        let x = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="pl">{DEFS}
    <category id="GRAMMAR" name="Grammar">
        <rule id="UNIFY_AP" name="unify antipattern">
            <antipattern>
                <unify>
                    <token postag="NN.*" postag_regexp="yes"/>
                    <unify-ignore><token regexp="yes">[^&lt;]+</token></unify-ignore>
                    <token postag="NN.*" postag_regexp="yes"/>
                </unify>
            </antipattern>
            <pattern>
                <token postag="NN.*" postag_regexp="yes"/>
                <token postag="NN.*" postag_regexp="yes"/>
                <token postag="NN.*" postag_regexp="yes"/>
            </pattern>
            <message>Unification.</message>
        </rule>
    </category>
</rules>"#
        );
        let rules = compile(&x);
        assert!(
            rules[0].antipatterns[0]
                .tokens
                .iter()
                .any(|t| t.unify_neutral),
            "unify-ignore tokens inside antipatterns must be flagged neutral"
        );
        assert_eq!(
            run(
                &x,
                &[
                    ("dom", &["NN:sg:m"]),
                    ("iks", &["NN:pl:f"]),
                    ("domem", &["NN:sg:m"])
                ]
            ),
            0,
            "agreeing outer nouns must let the antipattern block the rule"
        );
        assert_eq!(
            run(
                &x,
                &[
                    ("dom", &["NN:sg:m"]),
                    ("iks", &["NN:pl:f"]),
                    ("psy", &["NN:pl:f"])
                ]
            ),
            1
        );
    }

    #[test]
    fn repeat_elements_feed_each_matched_token() {
        let body = r#"<unify negate="yes"><feature id="gender"/><token postag="NN.*" postag_regexp="yes" min="2" max="3"/><token postag="NN.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(
                &x,
                &[
                    ("a", &["NN:sg:m"]),
                    ("b", &["NN:sg:f"]),
                    ("c", &["NN:sg:m"]),
                    ("d", &["NN:sg:m"])
                ]
            ),
            2,
            "a disagreeing middle token empties the branch so the negated group passes; the tail bound leaves room for the last element, so both start positions match"
        );
        assert_eq!(
            run(
                &x,
                &[
                    ("a", &["NN:sg:m"]),
                    ("b", &["NN:sg:m"]),
                    ("c", &["NN:sg:m"]),
                    ("d", &["NN:sg:m"])
                ]
            ),
            0
        );
    }

    #[test]
    fn multi_match_elements_do_not_shift_unify_positions() {
        let x = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="pl">{DEFS}
    <category id="GRAMMAR" name="Grammar">
        <rule id="UNIFY_SHIFT" name="unify shift">
            <pattern>
                <token postag="DT.*" postag_regexp="yes" min="1" max="2"/>
                <unify>
                    <token postag="NN.*" postag_regexp="yes"/>
                    <token postag="NN.*" postag_regexp="yes"/>
                </unify>
            </pattern>
            <message>Unification.</message>
        </rule>
    </category>
</rules>"#
        );
        assert_eq!(
            run(
                &x,
                &[
                    ("te", &["DT:f:s"]),
                    ("te", &["DT:f:s"]),
                    ("dom", &["NN:sg:m"]),
                    ("domy", &["NN:sg:m"])
                ]
            ),
            2,
            "both start positions must unify the noun pair, not shifted tokens"
        );
    }

    fn sentence_readings(parts: &[(&str, &[(&str, &str)])]) -> AnalyzedSentence {
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for (text, readings) in parts {
            let readings: Vec<AnalyzedToken> = readings
                .iter()
                .map(|(lemma, tags)| {
                    AnalyzedToken::new(*text, offset, offset + text.len())
                        .with_lemma(*lemma)
                        .with_pos_tags(vec![tags.to_string()])
                })
                .collect();
            let primary = AnalyzedToken::new(*text, offset, offset + text.len()).with_pos_tags(
                readings
                    .iter()
                    .flat_map(|r| r.pos_tags().iter().cloned())
                    .collect(),
            );
            tokens.push(AnalyzedTokenReadings::new(primary).with_readings(readings));
            offset += text.len() + 1;
        }
        let mut s = AnalyzedSentence::new("", 0, offset);
        s.set_tokens(tokens);
        s
    }

    fn run_readings(xml: &str, parts: &[(&str, &[(&str, &str)])]) -> usize {
        let rules = compile(xml);
        PatternRuleEngine::new()
            .match_rule(&rules[0], &sentence_readings(parts))
            .len()
    }

    #[test]
    fn inflected_element_restricts_unify_readings_by_lemma() {
        let body = r#"<unify><feature id="number"/><token postag="NN.*" postag_regexp="yes"/><token postag="NN.*" postag_regexp="yes" inflected="yes">dom</token></unify>"#;
        let x = xml(body);
        assert_eq!(
            run_readings(
                &x,
                &[
                    ("psy", &[("", "NN:pl:f1")]),
                    ("domy", &[("dom", "NN:sg"), ("chat", "NN:pl:m1")])
                ]
            ),
            0,
            "only the lemma-matching reading may enter unification (Java filters readings through the full element matcher)"
        );
        assert_eq!(
            run_readings(
                &x,
                &[
                    ("psy", &[("", "NN:pl:f1")]),
                    ("domy", &[("dom", "NN:pl:m1"), ("chat", "NN:sg")])
                ]
            ),
            1
        );
    }

    #[test]
    fn and_group_between_unify_tokens_participates_with_head_criteria() {
        let body = r#"<unify><token postag="NN.*" postag_regexp="yes"/><and><token postag="VB.*" postag_regexp="yes"/><token postag="NN.*" postag_regexp="yes"/></and><token postag="NN.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run_readings(
                &x,
                &[
                    ("psy", &[("", "NN:pl:f1")]),
                    ("śpią", &[("śpić", "VB:pl:3:f"), ("śpi", "NN:pl:m1")]),
                    ("psy", &[("", "NN:pl:f1")])
                ]
            ),
            1,
            "the and-group head reading set must unify with the surrounding nouns"
        );
        assert_eq!(
            run_readings(
                &x,
                &[
                    ("psy", &[("", "NN:pl:f1")]),
                    ("śpią", &[("śpić", "VB:sg:3:f"), ("śpi", "NN:pl:m1")]),
                    ("psy", &[("", "NN:pl:f1")])
                ]
            ),
            0,
            "a disagreeing head reading must break the unification"
        );
    }

    #[test]
    fn skip_creates_gap_after_element_in_structured_patterns() {
        fn wrap(pattern: &str) -> String {
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><rules lang="de"><rule id="SKIPDBG" name="s">{pattern}<message>x</message></rule></rules>"#
            )
        }
        let tokens: &[(&str, &[&str])] = &[
            ("Momentan", &["ADJ:PRD:GRU"]),
            ("Besitze", &["SUB:AKK:PLU:MAS", "VER:1:SIN:PRÄ:NON"]),
            ("ich", &["PRO:PER:NOM:SIN:ALG"]),
        ];
        let x = wrap(
            r#"<pattern case_sensitive="yes"><token postag="SENT_START" skip="-1"/><marker><and><token postag="VER:1.*" postag_regexp="yes"/><token postag="SUB.*" postag_regexp="yes"/></and></marker></pattern>"#,
        );
        assert_eq!(
            run(&x, tokens),
            1,
            "the skip gap must open after the element carrying it, not widen its own search"
        );
        let x2 = wrap(
            r#"<pattern case_sensitive="yes"><token>Momentan</token><marker><and><token postag="VER:1.*" postag_regexp="yes"/><token postag="SUB.*" postag_regexp="yes"/></and></marker></pattern>"#,
        );
        assert_eq!(
            run(&x2, tokens),
            1,
            "adjacent structured patterns keep matching"
        );
        let x3 = wrap(
            r#"<pattern case_sensitive="yes"><token postag="VER:1.*" postag_regexp="yes"/><token skip="1">ich</token></pattern>"#,
        );
        assert_eq!(
            run(&x3, tokens),
            1,
            "one token may intervene between the skip carrier and the next element"
        );
        let x4 = wrap(
            r#"<pattern case_sensitive="yes"><token>Momentan</token><token postag="VER:1.*" postag_regexp="yes"/></pattern>"#,
        );
        assert_eq!(
            run(
                &x4,
                &[
                    ("Momentan", &["ADJ:PRD:GRU"]),
                    ("keine", &["PRO:IND:AKK:PLU:FEM:BEG"]),
                    ("Besitze", &["SUB:AKK:PLU:MAS", "VER:1:SIN:PRÄ:NON"]),
                ],
            ),
            0,
            "a token between elements without skip must not match"
        );
    }

    #[test]
    fn or_group_patterns_produce_forward_spans() {
        let x = xml(
            r#"<pattern><token postag="NN.*" postag_regexp="yes"/><or><token postag="VB.*:f.*" postag_regexp="yes"/><token postag="VB.*:m.*" postag_regexp="yes"/></or></pattern>"#,
        );
        assert_eq!(
            run(&x, &[("psy", &["NN:pl:f1"]), ("śpią", &["VB:pl:3:f"])]),
            1
        );
        let x3 = xml(
            r#"<pattern><token postag="NN.*" postag_regexp="yes"/><or><token>śpią</token><token>xxx</token></or></pattern>"#,
        );
        assert_eq!(
            run(&x3, &[("psy", &["NN:pl:f1"]), ("śpią", &["VB:pl:3:f"])]),
            1,
            "structured patterns must keep first_start <= last_end across elements"
        );
    }

    #[test]
    fn or_group_inside_unify_branches_per_alternative() {
        let body = r#"<unify><token postag="NN.*" postag_regexp="yes"/><or><token postag="VB.*:f.*" postag_regexp="yes"/><token postag="VB.*:m.*" postag_regexp="yes"/></or></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(&x, &[("psy", &["NN:pl:f1"]), ("śpią", &["VB:pl:3:f"])]),
            1
        );
        assert_eq!(
            run(&x, &[("psy", &["NN:pl:f1"]), ("śpią", &["VB:pl:3:m"])]),
            0,
            "no alternative reading set may unify with a feminine noun"
        );
    }

    #[test]
    fn skipped_optional_inside_unify_does_not_fail_the_group() {
        let body = r#"<unify><token postag="NN.*:pl.*" postag_regexp="yes" min="0"/><token postag="NN.*:pl.*" postag_regexp="yes"/></unify>"#;
        let x = xml(body);
        assert_eq!(
            run(&x, &[("psy", &["NN:pl:f1"]), ("psy", &["NN:pl:f1"])]),
            2,
            "a skipped optional unify element is silently ignored, not a group failure"
        );
    }
    #[test]
    fn date_parsers_cover_english_and_german_months_and_weekdays() {
        assert_eq!(parse_month("January"), 1);
        assert_eq!(parse_month("dec"), 12);
        assert_eq!(parse_month("09"), 9);
        assert_eq!(parse_month("12"), 12);
        assert_eq!(parse_month("xyz"), 0);
        assert_eq!(parse_month_german("Januar"), 1);
        assert_eq!(parse_month_german("Jänner"), 1);
        assert_eq!(parse_month_german("März"), 3);
        assert_eq!(parse_month_german("Mai"), 5);
        assert_eq!(parse_month_german("Oktober"), 10);
        assert_eq!(parse_month_german("Dezember"), 12);
        assert_eq!(parse_month_german("Sonnabend"), 0);
        assert_eq!(parse_weekday("Sunday"), 1);
        assert_eq!(parse_weekday("mon"), 2);
        assert_eq!(parse_weekday("Fri"), 6);
        assert_eq!(parse_weekday("x"), 0);
        assert_eq!(parse_weekday_german("Sonntag"), 1);
        assert_eq!(parse_weekday_german("Sonnabend"), 7);
        assert_eq!(parse_weekday_german("Dienstag"), 3);
        assert_eq!(parse_weekday_german("Freitag"), 6);
        assert_eq!(parse_weekday_german("xx"), 0);
    }

    #[test]
    fn month_lengths_and_day_zero_rollback_match_the_calendar() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2026, 10), 31);
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 13), 0);
        assert_eq!(day_zero_rolls_back(2026, 10, 0), (2026, 9, 30));
        assert_eq!(day_zero_rolls_back(2026, 3, 0), (2026, 2, 28));
        assert_eq!(day_zero_rolls_back(2024, 1, 0), (2023, 12, 31));
        assert_eq!(day_zero_rolls_back(2026, 10, 11), (2026, 10, 11));
        assert_eq!(compute_weekday(2024, 2, 29), 5);
        assert_eq!(compute_weekday(2000, 1, 1), 7);
        assert_eq!(compute_weekday(1999, 12, 31), 6);
        assert_eq!(compute_weekday(2026, 10, 11), 1);
    }

    #[test]
    fn inn_checksum_validates_ten_and_twelve_digit_numbers() {
        assert!(inn_is_valid("7830023695"));
        assert!(inn_is_valid("500100732570"));
        assert!(!inn_is_valid("7830023690"));
        assert!(!inn_is_valid("500100732571"));
        assert!(inn_is_valid("1234"));
        assert!(inn_is_valid("783002369a"));
    }

    #[test]
    fn test_date_hack_pins_current_date() {
        set_test_date_hack(true);
        assert_eq!(current_date(), (2014, 1, 1));
        assert_eq!(logic_current_year(), 2014);
        assert!(logic_is_january());
        set_test_date_hack(false);
        let (y, m, d) = current_date();
        assert!((2020..=2100).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d));
        assert_eq!(logic_current_year(), y);
        assert_eq!(logic_is_january(), m == 1);
    }

    #[test]
    fn russian_partial_tag_check_uses_the_shared_tagger() {
        let matched = vec!["стол".to_string()];
        fn make<'a>(args: &'a str) -> HashMap<&'a str, String> {
            args.split_whitespace()
                .filter_map(|part| part.split_once(':'))
                .map(|(k, v)| (k, v.to_string()))
                .collect()
        }
        let base = "no:1 regexp:(.+) postag_regexp:NN.*";
        assert!(russian_partial_tag_satisfies(&make(base), &matched));
        let negated = "no:1 regexp:(.+) postag_regexp:VB.* negate_pos:true";
        assert!(russian_partial_tag_satisfies(&make(negated), &matched));
        let wrong = "no:1 regexp:(.+) postag_regexp:VB.*";
        assert!(!russian_partial_tag_satisfies(&make(wrong), &matched));
        assert!(!russian_partial_tag_satisfies(
            &make("regexp:(.+)"),
            &matched
        ));
        assert!(!russian_partial_tag_satisfies(
            &make("no:1 regexp:(.+)"),
            &matched
        ));
        assert!(!russian_partial_tag_satisfies(
            &make("no:9 regexp:(.+) postag_regexp:NOUN"),
            &matched
        ));
        assert!(!russian_partial_tag_satisfies(
            &make("no:1 regexp:zzz( postag_regexp:NOUN"),
            &matched
        ));
        let two = "no:1 regexp:(с)(\\p{L}+) postag_regexp:NN.* two_groups_regexp:true";
        assert!(russian_partial_tag_satisfies(&make(two), &matched));
    }

    #[test]
    fn trim_special_chars_keeps_words_and_punctuation() {
        assert_eq!(trim_special_chars("слово"), "слово");
        assert_eq!(trim_special_chars("<слово>"), "слово");
        assert_eq!(trim_special_chars("«слово»"), "«слово»");
        assert_eq!(trim_special_chars("➊слово➊"), "слово");
    }
    use super::tests::{compile_and_match, make_analyzed_sentence};

    fn compile_and_match_with_dict(
        xml: &str,
        sentence_text: &str,
        words: &[&str],
    ) -> Vec<RuleMatch> {
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let support = crate::spelling_support::SpellingSupport::new(
            og_spell::Dictionary::from_words(words),
            "",
        );
        let engine = PatternRuleEngine::new().with_spelling_support(std::sync::Arc::new(support));
        let sentence = make_analyzed_sentence(sentence_text);
        rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect()
    }

    fn values_of(rm: &RuleMatch) -> Vec<String> {
        rm.replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect()
    }

    #[test]
    fn recent_year_filter_bounds_the_matched_year() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="RY" name="ry">
                <pattern>
                    <token regexp="yes">\d{4}</token>
                </pattern>
                <message>m</message>
                <filter class="org.languagetool.rules.de.RecentYearFilter" args="year:\1 maxYearsBack:10"/>
            </rule>
        </rules>"#;
        set_test_date_hack(true);
        assert!(!compile_and_match(xml, "Im Jahr 2010.").is_empty());
        assert!(compile_and_match(xml, "Im Jahr 2020.").is_empty());
        assert!(compile_and_match(xml, "Im Jahr 2000.").is_empty());
        set_test_date_hack(false);
    }

    #[test]
    fn date_range_checker_keeps_only_descending_ranges() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="DR" name="dr">
                <pattern>
                    <token regexp="yes">\d+</token>
                    <token>to</token>
                    <token regexp="yes">\d+</token>
                </pattern>
                <message>m</message>
                <filter class="org.languagetool.rules.DateRangeChecker" args="x:\1 y:\3"/>
            </rule>
        </rules>"#;
        assert!(!compile_and_match(xml, "from 5 to 3").is_empty());
        assert!(compile_and_match(xml, "from 3 to 5").is_empty());
    }

    #[test]
    fn regex_antipattern_filter_drops_matching_tokens() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="AP" name="ap">
                <pattern>
                    <token regexp="yes">aa|bb|cc</token>
                </pattern>
                <message>m</message>
                <filter class="org.languagetool.rules.patterns.RegexAntiPatternFilter" args="antipatterns:aa|bb"/>
            </rule>
        </rules>"#;
        assert!(compile_and_match(xml, "aa here").is_empty());
        assert!(compile_and_match(xml, "bb here").is_empty());
        assert!(!compile_and_match(xml, "cc here").is_empty());
    }

    #[test]
    fn remove_unknown_compounds_filter_checks_the_joined_form() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="RC" name="rc">
                <pattern>
                    <token>blassen</token>
                    <token>fall</token>
                </pattern>
                <message>m</message>
                <filter class="org.languagetool.rules.de.RemoveUnknownCompoundsFilter" args="part1:\1 part2:\2"/>
            </rule>
        </rules>"#;
        assert!(
            compile_and_match_with_dict(xml, "der blassen fall", &[]).is_empty(),
            "unknown compound suppresses the match"
        );
        let kept = compile_and_match_with_dict(
            xml,
            "der blassen fall",
            &["blassen", "fall", "blassenfall"],
        );
        assert!(!kept.is_empty(), "known compound keeps the match");
    }

    #[test]
    fn valid_word_filter_drops_matches_that_form_a_real_word() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="VW" name="vw">
                <pattern>
                    <token>Fahr</token>
                    <token>rad</token>
                </pattern>
                <message>m</message>
                <filter class="org.languagetool.rules.de.ValidWordFilter" args="word1:\1 word2:\2"/>
            </rule>
        </rules>"#;
        assert!(compile_and_match_with_dict(xml, "das Fahr rad", &["Fahrrad"]).is_empty());
        assert!(!compile_and_match_with_dict(xml, "das Fahr rad", &[]).is_empty());
    }

    #[test]
    fn potential_compound_filter_suggests_joined_or_hyphenated() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="PC" name="pc">
                <pattern>
                    <token>Fahr</token>
                    <token>rad</token>
                </pattern>
                <message>m</message>
                <filter class="org.languagetool.rules.de.PotentialCompoundFilter" args="part1:\1 part2:\2"/>
            </rule>
        </rules>"#;
        let matches = compile_and_match_with_dict(xml, "das Fahr rad", &["Fahrrad"]);
        assert_eq!(values_of(&matches[0]), vec!["Fahrrad".to_string()]);
        let matches = compile_and_match_with_dict(xml, "das Fahr rad", &[]);
        assert_eq!(values_of(&matches[0]), vec!["Fahr-Rad".to_string()]);
    }

    #[test]
    fn number_in_word_filter_replaces_digits_from_the_word() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="NW" name="nw">
                <pattern>
                    <token regexp="yes">w0rd</token>
                </pattern>
                <message>m</message>
                <filter class="org.languagetool.rules.en.EnglishNumberInWordFilter" args="word:\1"/>
            </rule>
        </rules>"#;
        let matches = compile_and_match_with_dict(xml, "the w0rd here", &["word"]);
        assert_eq!(values_of(&matches[0]), vec!["word".to_string()]);
        let matches = compile_and_match_with_dict(xml, "the w0rd here", &[]);
        assert!(
            matches.is_empty(),
            "no replacement available drops the match"
        );
    }

    #[test]
    fn suppress_misspelled_suggestions_filter_prunes_replacements() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="SS" name="ss">
                <pattern>
                    <token>xyzzy</token>
                </pattern>
                <message>m <suggestion>bonjou</suggestion> or <suggestion>xyzzy</suggestion></message>
                <filter class="org.languagetool.rules.en.EnglishSuppressMisspelledSuggestionsFilter"/>
            </rule>
        </rules>"#;
        let matches = compile_and_match_with_dict(xml, "the xyzzy here", &["xyzzy"]);
        assert_eq!(matches.len(), 1);
        assert_eq!(values_of(&matches[0]), vec!["xyzzy".to_string()]);
        let xml_all_bad = xml.replace(
            "<suggestion>xyzzy</suggestion>",
            "<suggestion>bonjou</suggestion>",
        );
        assert!(compile_and_match_with_dict(&xml_all_bad, "the xyzzy here", &["xyzzy"]).is_empty());
    }

    #[test]
    fn find_suggestions_filter_keeps_finished_suggestions_without_a_tagger() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="FS" name="fs">
                <pattern>
                    <token>xyzzy</token>
                </pattern>
                <message>m <suggestion>finished text</suggestion></message>
                <filter class="org.languagetool.rules.en.FindSuggestionsFilter" args="wordFrom:marker desiredPostag:NN"/>
            </rule>
        </rules>"#;
        let matches = compile_and_match_with_dict(xml, "the xyzzy here", &["xyzzy"]);
        assert_eq!(matches.len(), 1);
        let xml_no_args = xml.replace("wordFrom:marker desiredPostag:NN", "desiredPostag:NN");
        let matches = compile_and_match_with_dict(&xml_no_args, "the xyzzy here", &["xyzzy"]);
        assert_eq!(values_of(&matches[0]), vec!["finished text".to_string()]);
    }

    #[test]
    fn insert_comma_filter_commas_two_word_suggestions() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <rule id="IC" name="ic">
                <pattern>
                    <token>komm</token>
                    <token>geh</token>
                </pattern>
                <message>m <suggestion>komm geh</suggestion></message>
                <filter class="org.languagetool.rules.de.InsertCommaFilter"/>
            </rule>
        </rules>"#;
        let matches = compile_and_match(xml, "komm geh jetzt");
        assert_eq!(matches.len(), 1);
        assert_eq!(values_of(&matches[0]), vec!["komm, geh".to_string()]);
    }

    #[test]
    fn apostrophe_type_filter_gates_on_the_typographic_marker() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="AT" name="at">
                <pattern>
                    <token>don</token>
                </pattern>
                <message>m</message>
                <filter class="org.languagetool.rules.patterns.ApostropheTypeFilter" args="wordFrom:1 hasTypographicalApostrophe:true"/>
            </rule>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        let engine = PatternRuleEngine::new();
        let mut sentence = make_analyzed_sentence("don’t go");
        sentence.tokens_mut()[0].mark_typographic_apostrophe();
        let matches: Vec<RuleMatch> = rule_set
            .rules
            .iter()
            .flat_map(|r| engine.match_rule(r, &sentence))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "marked token satisfies the typographic gate"
        );
        let matches = compile_and_match(xml, "don’t go");
        assert!(matches.is_empty(), "plain ASCII token is dropped");
        let xml_plain = xml.replace(
            "hasTypographicalApostrophe:true",
            "hasTypographicalApostrophe:false",
        );
        let matches = compile_and_match(&xml_plain, "don’t go");
        assert_eq!(matches.len(), 1);
        let xml_oob = xml.replace("wordFrom:1", "wordFrom:9");
        assert!(compile_and_match(&xml_oob, "don’t go").is_empty());
        let xml_no_from = xml.replace("wordFrom:1 ", "");
        assert_eq!(compile_and_match(&xml_no_from, "don’t go").len(), 1);
    }
}
