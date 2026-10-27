use og_core::analyzed::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_core::checker::Disambiguator;
use og_xml::compiler::{
    CompiledException, CompiledPattern, CompiledPatternElement, CompiledPatternToken, PosMatcher,
    XmlCompiler,
};
use og_xml::disambig_parser::DisambigXmlParser;
use og_xml::disambig_types::{CompiledDisambigRule, DisambigAction, DisambigRule};

enum FirstTokenGate {
    None,
    Text { text: String, case_sensitive: bool },
    LiteralSet(std::collections::HashSet<String>),
    OrLiteralSet(std::collections::HashSet<String>),
    PostagPlain { tag: String, negate_pos: bool },
}

pub struct XmlDisambiguator {
    rules: Vec<CompiledDisambigRule>,
    unifications: Vec<(String, Vec<(String, PosMatcher)>)>,
    action_regexes: Vec<DisambigActionRegex>,
    gates: Vec<FirstTokenGate>,
    first_literal: Vec<Option<FirstLiteralFilter>>,
    first_regex: Vec<Option<FirstRegexFilter>>,
    partial_postag: Vec<Option<PartialPosTagFilterSpec>>,
    rule_index: DisambigRuleIndex,
    english_tagger: Option<std::sync::Arc<crate::english_tagger::EnglishTagger>>,
}

struct DisambigRuleIndex {
    by_token: std::collections::HashMap<String, Vec<u32>>,
    unclassified: Vec<u32>,
}

fn expand_literal_regexp(pattern: &str) -> Option<Vec<String>> {
    let chars: Vec<char> = pattern.chars().collect();
    let (values, pos) = expand_regexp_alternation(&chars, 0)?;
    if pos != chars.len() || values.is_empty() || values.len() > 256 {
        return None;
    }
    let mut out = values;
    out.sort_unstable();
    out.dedup();
    Some(out)
}

fn expand_regexp_alternation(chars: &[char], mut pos: usize) -> Option<(Vec<String>, usize)> {
    let mut branches = Vec::new();
    loop {
        let (branch, next) = expand_regexp_concat(chars, pos)?;
        branches.push(branch);
        pos = next;
        if pos < chars.len() && chars[pos] == '|' {
            pos += 1;
            continue;
        }
        break;
    }
    let mut values = Vec::new();
    for branch in branches {
        values.extend(branch);
    }
    Some((values, pos))
}

fn expand_regexp_concat(chars: &[char], mut pos: usize) -> Option<(Vec<String>, usize)> {
    let mut current = vec![String::new()];
    while pos < chars.len() {
        let c = chars[pos];
        if c == '|' || c == ')' {
            break;
        }
        let mut part_values: Vec<String>;
        if c == '(' {
            let mut open_at = pos + 1;
            if open_at + 1 < chars.len() && chars[open_at] == '?' && chars[open_at + 1] == ':' {
                open_at += 2;
            }
            let (inner, next) = expand_regexp_alternation(chars, open_at)?;
            if next >= chars.len() || chars[next] != ')' {
                return None;
            }
            part_values = inner;
            pos = next + 1;
        } else if c == '\\' {
            let next = pos + 1;
            if next >= chars.len() || chars[next].is_alphanumeric() {
                return None;
            }
            part_values = vec![chars[next].to_string()];
            pos = next + 1;
        } else if c.is_alphanumeric() || " -_'/,;:!?&%#@\"".contains(c) {
            part_values = vec![c.to_string()];
            pos += 1;
        } else {
            return None;
        }
        if pos < chars.len() && chars[pos] == '?' {
            part_values.insert(0, String::new());
            pos += 1;
        }
        let mut merged = Vec::with_capacity(current.len() * part_values.len());
        for prefix in &current {
            for part in &part_values {
                merged.push(format!("{prefix}{part}"));
            }
        }
        if merged.len() > 256 {
            return None;
        }
        current = merged;
    }
    Some((current, pos))
}

impl DisambigRuleIndex {
    fn build(rules: &[CompiledDisambigRule]) -> Self {
        let mut by_token: std::collections::HashMap<String, Vec<u32>> =
            std::collections::HashMap::new();
        let mut unclassified = Vec::new();
        for (idx, rule) in rules.iter().enumerate() {
            let idx = idx as u32;
            let mut best: Option<Vec<String>> = None;
            if !rule.pattern.has_structured_elements {
                for token in &rule.pattern.tokens {
                    let Some(values) = mandatory_literal_values(token) else {
                        continue;
                    };
                    if best.as_ref().is_none_or(|b| values.len() < b.len()) {
                        best = Some(values);
                    }
                }
            }
            match best {
                Some(values) => {
                    for value in values {
                        by_token.entry(value).or_default().push(idx);
                    }
                }
                None => unclassified.push(idx),
            }
        }
        Self {
            by_token,
            unclassified,
        }
    }

    fn rules_for_sentence(
        &self,
        lower_texts: &std::collections::HashSet<String>,
        limit: u32,
    ) -> Vec<u32> {
        let mut selected: Vec<u32> = self
            .unclassified
            .iter()
            .copied()
            .filter(|&idx| idx < limit)
            .collect();
        if !self.by_token.is_empty() {
            for (value, indices) in &self.by_token {
                if lower_texts.contains(value) {
                    selected.extend(indices.iter().copied().filter(|&idx| idx < limit));
                }
            }
            selected.sort_unstable();
            selected.dedup();
        }
        selected
    }
}

fn mandatory_literal_values(token: &CompiledPatternToken) -> Option<Vec<String>> {
    let text = token.text.as_deref()?;
    if text.is_empty()
        || token.match_no.is_some()
        || token.min == Some(0)
        || token.inflected
        || token.negate
        || token.negate_pos
        || token.chunk.is_some()
    {
        return None;
    }
    let raw: Vec<String> = if token.is_regexp {
        expand_literal_regexp(text)?
    } else {
        vec![text.to_string()]
    };
    Some(raw.into_iter().map(|v| v.to_lowercase()).collect())
}

struct PartialPosTagFilterSpec {
    no: usize,
    token_re: regex::Regex,
    postag_re: regex::Regex,
    negate_pos: bool,
    two_groups: bool,
    prefix: String,
    suffix: String,
}

struct FirstRegexFilter {
    re: og_xml::compiler::SharedRegex,
    inflected: bool,
}

struct FirstLiteralFilter {
    text: String,
    case_sensitive: bool,
    inflected: bool,
}

enum DisambigActionRegex {
    Remove(Vec<RemoveWordMatcher>),
    RemovePostag(
        Option<og_xml::compiler::SharedRegex>,
        Option<og_xml::compiler::SharedFancyRegex>,
    ),
    Filter(
        Option<og_xml::compiler::SharedRegex>,
        Option<og_xml::compiler::SharedFancyRegex>,
    ),
    Match {
        anchored: Option<og_xml::compiler::SharedRegex>,
        anchored_fancy: Option<og_xml::compiler::SharedFancyRegex>,
        raw: Option<og_xml::compiler::SharedRegex>,
        raw_fancy: Option<og_xml::compiler::SharedFancyRegex>,
    },
    None,
}

struct RemoveWordMatcher {
    regex: Option<og_xml::compiler::SharedRegex>,
    literal: Option<String>,
    lemma: Option<String>,
}

impl XmlDisambiguator {
    // Java's IsEnglishWordFilter resolves en-US only when the en module is on the classpath;
    // de/fr/es/nl/pt/ca modules don't depend on it, so their pipelines run the filter inert.
    pub fn from_xml(xml: &str) -> Result<Self, String> {
        Self::from_xml_with_english(xml, None)
    }

    pub fn from_xml_with_english(
        xml: &str,
        english_tagger: Option<std::sync::Arc<crate::english_tagger::EnglishTagger>>,
    ) -> Result<Self, String> {
        let parser = DisambigXmlParser::new();
        let rule_set = parser.parse(xml).map_err(|e| e.to_string())?;

        let compiler = XmlCompiler::new();
        let compiled_rules: Vec<CompiledDisambigRule> = rule_set
            .rules
            .into_iter()
            .map(|r| compile_disambig_rule(&compiler, r))
            .collect();

        let mut unifications: Vec<(String, Vec<(String, PosMatcher)>)> = Vec::new();
        for uni in &rule_set.unifications {
            let mut compiled = Vec::new();
            for eq in &uni.equivalences {
                if let Some(re) = PosMatcher::new_anchored(&format!("^(?:{})$", eq.postag)) {
                    compiled.push((eq.eq_type.clone(), re));
                }
            }
            unifications.push((uni.feature.clone(), compiled));
        }

        let is_regex_pos = |pos: &str| {
            pos.contains('.')
                || pos.contains('*')
                || pos.contains('+')
                || pos.contains('[')
                || pos.contains('(')
                || pos.contains('|')
        };
        let action_regexes = compiled_rules
            .iter()
            .map(|rule| match &rule.disambig {
                DisambigAction::Remove(wds) => DisambigActionRegex::Remove(
                    wds.iter()
                        .map(|w| {
                            let literal = w.pos.clone().filter(|pos| !is_regex_pos(pos));
                            let regex = match (&w.pos, &literal) {
                                (Some(pos), None) => og_xml::compiler::shared_cached_regex(pos),
                                _ => None,
                            };
                            RemoveWordMatcher {
                                regex,
                                literal,
                                lemma: w.lemma.clone(),
                            }
                        })
                        .collect(),
                ),
                DisambigAction::RemovePostag(postag) => {
                    let anchored = format!("^(?:{})$", postag);
                    DisambigActionRegex::RemovePostag(
                        og_xml::compiler::shared_cached_regex(&anchored),
                        og_xml::compiler::shared_cached_fancy_regex(&anchored),
                    )
                }
                DisambigAction::Filter { postag } => {
                    let anchored = format!("^(?:{postag})$");
                    DisambigActionRegex::Filter(
                        og_xml::compiler::shared_cached_regex(&anchored),
                        og_xml::compiler::shared_cached_fancy_regex(&anchored),
                    )
                }
                DisambigAction::Match(m) => {
                    let Some(postag) = m.postag.as_deref() else {
                        return DisambigActionRegex::Match {
                            anchored: None,
                            anchored_fancy: None,
                            raw: None,
                            raw_fancy: None,
                        };
                    };
                    let anchored = format!("^(?:{postag})$");
                    DisambigActionRegex::Match {
                        anchored: og_xml::compiler::shared_cached_regex(&anchored),
                        anchored_fancy: og_xml::compiler::shared_cached_fancy_regex(&anchored),
                        raw: og_xml::compiler::shared_cached_regex(postag),
                        raw_fancy: og_xml::compiler::shared_cached_fancy_regex(postag),
                    }
                }
                _ => DisambigActionRegex::None,
            })
            .collect();

        let gates = compiled_rules.iter().map(build_first_token_gate).collect();

        let first_literal = {
            let rules = &compiled_rules;
            (0..rules.len())
                .map(|i| {
                    let first = rules[i].pattern.tokens.first()?;
                    if rules[i].pattern.has_structured_elements
                        || first.negate
                        || first.min == Some(0)
                        || first.compiled_regexp.is_some()
                    {
                        return None;
                    }
                    let text = first.text.as_ref()?;
                    Some(FirstLiteralFilter {
                        text: text.clone(),
                        case_sensitive: first.case_sensitive,
                        inflected: first.inflected,
                    })
                })
                .collect()
        };

        let first_regex: Vec<Option<FirstRegexFilter>> = {
            let rules = &compiled_rules;
            (0..rules.len())
                .map(|i| {
                    let first = rules[i].pattern.tokens.first()?;
                    if rules[i].pattern.has_structured_elements
                        || first.negate
                        || first.min == Some(0)
                        || first.text.is_some()
                    {
                        return None;
                    }
                    let re = first.compiled_regexp.as_ref()?;
                    let re_to_use = if !first.case_sensitive {
                        first.compiled_regexp_ci.as_ref().unwrap_or(re)
                    } else {
                        re
                    };
                    Some(FirstRegexFilter {
                        re: re_to_use.clone(),
                        inflected: first.inflected,
                    })
                })
                .collect()
        };

        let partial_postag = compiled_rules
            .iter()
            .map(|rule| {
                if rule.filter_class.as_deref() != Some(EN_PARTIAL_POSTAG_FILTER_CLASS) {
                    return None;
                }
                compile_partial_postag_filter(rule.filter_args.as_deref().unwrap_or(""))
            })
            .collect();

        let rule_index = DisambigRuleIndex::build(&compiled_rules);

        Ok(Self {
            rules: compiled_rules,
            unifications,
            action_regexes,
            gates,
            first_literal,
            first_regex,
            partial_postag,
            rule_index,
            english_tagger,
        })
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    pub fn rule_ids(&self) -> Vec<&str> {
        self.rules.iter().filter_map(|r| r.id.as_deref()).collect()
    }
}

fn parse_literal_alternation(pattern: &str) -> Option<Vec<String>> {
    let trimmed = pattern
        .strip_prefix("^(?:")
        .and_then(|p| p.strip_suffix(")$"))
        .unwrap_or(pattern);
    let mut literals = Vec::new();
    for part in trimmed.split('|') {
        if part.is_empty() {
            return None;
        }
        for ch in part.chars() {
            if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' && ch != '\'' && ch != '/' {
                return None;
            }
        }
        literals.push(part.to_string());
    }
    Some(literals)
}

fn first_element_token(
    rule: &CompiledDisambigRule,
) -> Option<&og_xml::compiler::CompiledPatternToken> {
    match rule.pattern.elements.first() {
        Some(og_xml::compiler::CompiledPatternElement::Token(t)) => Some(t),
        Some(og_xml::compiler::CompiledPatternElement::OrGroup(alts)) => alts.first(),
        Some(og_xml::compiler::CompiledPatternElement::AndGroup(cs)) => cs.first(),
        None => rule.pattern.tokens.first(),
    }
}

fn build_first_token_gate(rule: &CompiledDisambigRule) -> FirstTokenGate {
    if let Some(og_xml::compiler::CompiledPatternElement::OrGroup(group)) =
        rule.pattern.elements.first()
    {
        let mut set = std::collections::HashSet::new();
        let mut usable = !group.is_empty();
        for alt in group {
            if alt.negate
                || alt.min == Some(0)
                || alt.inflected
                || alt.compiled_regexp.is_some()
                || alt.text.as_deref().is_none_or(str::is_empty)
            {
                usable = false;
                break;
            }
            set.insert(alt.text.clone().unwrap_or_default().to_lowercase());
        }
        if usable {
            return FirstTokenGate::OrLiteralSet(set);
        }
        return FirstTokenGate::None;
    }
    let Some(first) = first_element_token(rule) else {
        return FirstTokenGate::None;
    };
    if first.min == Some(0) || first.negate {
        return FirstTokenGate::None;
    }
    if let Some(re) = first.compiled_regexp.as_ref()
        && first.text.is_none()
        && !first.inflected
    {
        if let Some(literals) = parse_literal_alternation(re.as_str()) {
            let set = if first.case_sensitive {
                literals.into_iter().collect()
            } else {
                literals
                    .iter()
                    .map(|l| l.to_lowercase())
                    .collect::<std::collections::HashSet<_>>()
            };
            return FirstTokenGate::LiteralSet(set);
        }
        return FirstTokenGate::None;
    }
    if let Some(text) = first.text.as_ref()
        && first.compiled_regexp.is_none()
        && !first.inflected
    {
        return FirstTokenGate::Text {
            text: if first.case_sensitive {
                text.clone()
            } else {
                text.to_lowercase()
            },
            case_sensitive: first.case_sensitive,
        };
    }
    if let Some(postag) = first.postag.as_ref()
        && first.compiled_postag_regexp.is_none()
        && first.text.is_none()
        && first.compiled_regexp.is_none()
        && postag != "UNKNOWN"
    {
        return FirstTokenGate::PostagPlain {
            tag: postag.clone(),
            negate_pos: first.negate_pos,
        };
    }
    FirstTokenGate::None
}

impl Disambiguator for XmlDisambiguator {
    fn disambiguate(&self, sentence: &mut AnalyzedSentence) {
        self.apply_rules_through(sentence, self.rules.len());
    }
}

impl XmlDisambiguator {
    pub fn rule_id(&self, rule_idx: usize) -> Option<&str> {
        self.rules.get(rule_idx).and_then(|r| r.id.as_deref())
    }

    pub fn rule_examples(
        &self,
        rule_idx: usize,
    ) -> (&[String], &[og_xml::disambig_types::DisambigExample]) {
        match self.rules.get(rule_idx) {
            Some(rule) => (&rule.untouched_examples, &rule.examples),
            None => (&[], &[]),
        }
    }

    pub fn apply_rules_through(&self, sentence: &mut AnalyzedSentence, through: usize) {
        let limit = through.min(self.rules.len());
        let mut env = SentenceEnv::build(sentence);
        let candidates = self
            .rule_index
            .rules_for_sentence(&env.lower_texts, limit as u32);
        let mut memo = RegexMemo::default();
        for rule_idx in candidates {
            if self.apply_rule_with_env(sentence, rule_idx as usize, &env, &mut memo) {
                env = SentenceEnv::build(sentence);
                memo = RegexMemo::default();
            }
        }
    }

    pub fn rule_debug(&self, rule_idx: usize) -> String {
        match self.rules.get(rule_idx) {
            Some(rule) => format!("{:?} {:?}", rule.id, rule.disambig),
            None => String::new(),
        }
    }

    pub fn debug_match_rule(
        &self,
        sentence: &mut AnalyzedSentence,
        rule_idx: usize,
    ) -> Vec<Vec<usize>> {
        let Some(rule) = self.rules.get(rule_idx) else {
            return Vec::new();
        };
        let tokens: Vec<&AnalyzedTokenReadings> =
            sentence.non_whitespace_tokens().into_iter().collect();
        let mut out = Vec::new();
        for start in 0..tokens.len() {
            if let Some(result) = match_pattern(&rule.pattern, &tokens, start) {
                out.push(result.matched_positions.clone());
            }
        }
        out
    }

    pub fn rule_pattern_debug(&self, rule_idx: usize) -> String {
        let Some(rule) = self.rules.get(rule_idx) else {
            return String::new();
        };
        rule.pattern
            .tokens
            .iter()
            .map(|t| {
                format!(
                    "{:?}/{:?}@{:?}",
                    t.text,
                    t.postag,
                    t.compiled_regexp.as_ref().map(|r| r.as_str())
                )
            })
            .collect::<Vec<_>>()
            .join(" ; ")
    }

    pub fn first_index_of_rule_id(&self, rule_id: &str) -> Option<usize> {
        self.rules
            .iter()
            .position(|r| r.id.as_deref() == Some(rule_id))
    }

    pub fn apply_rule(&self, sentence: &mut AnalyzedSentence, rule_idx: usize) {
        let env = SentenceEnv::build(sentence);
        let mut memo = RegexMemo::default();
        self.apply_rule_with_env(sentence, rule_idx, &env, &mut memo);
    }

    fn apply_rule_with_env(
        &self,
        sentence: &mut AnalyzedSentence,
        rule_idx: usize,
        env: &SentenceEnv,
        memo: &mut RegexMemo,
    ) -> bool {
        let rule = match self.rules.get(rule_idx) {
            Some(rule) => rule,
            None => return false,
        };
        {
            if rule.pattern.tokens.is_empty() && rule.pattern.elements.is_empty() {
                return false;
            }
            let mut pending_ignore_spelling: Vec<(usize, usize)> = Vec::new();
            let mut pending_actions: Vec<(usize, &DisambigAction, &DisambigActionRegex)> =
                Vec::new();
            let mut pending_unify: Vec<(usize, AnalyzedToken, Vec<AnalyzedToken>)> = Vec::new();
            let mut pending_wd_actions: Vec<(Vec<usize>, &DisambigAction, &DisambigActionRegex)> =
                Vec::new();
            let mut pending_filterall: Vec<(usize, &CompiledPatternToken)> = Vec::new();
            let mut pending_immunize: Vec<usize> = Vec::new();

            {
                let tokens: Vec<&AnalyzedTokenReadings> = env.token_refs(sentence);

                let Some(first) = first_element_token(rule) else {
                    return false;
                };
                match &self.gates[rule_idx] {
                    FirstTokenGate::Text {
                        text,
                        case_sensitive,
                    } => {
                        let can_match = if *case_sensitive {
                            env.raw_texts.contains(text.as_str())
                        } else {
                            env.lower_texts.contains(text.as_str())
                                || env.raw_texts.contains(text.as_str())
                        };
                        if !can_match {
                            return false;
                        }
                    }
                    FirstTokenGate::LiteralSet(set) => {
                        let any = if first.case_sensitive {
                            env.raw_texts.iter().any(|t| set.contains(t.as_str()))
                        } else {
                            env.lower_texts.iter().any(|l| set.contains(l.as_str()))
                        };
                        if !any {
                            return false;
                        }
                    }
                    FirstTokenGate::OrLiteralSet(set) => {
                        if !env.lower_texts.iter().any(|l| set.contains(l.as_str())) {
                            return false;
                        }
                    }
                    FirstTokenGate::PostagPlain { tag, negate_pos } => {
                        let any = env.pos_tags.iter().any(|set| set.contains(tag.as_str()));
                        if any == *negate_pos {
                            return false;
                        }
                    }
                    FirstTokenGate::None => {}
                }

                for start_idx in 0..tokens.len() {
                    if let FirstTokenGate::PostagPlain { tag, negate_pos } = &self.gates[rule_idx]
                        && env.pos_tags[start_idx].contains(tag.as_str()) == *negate_pos
                    {
                        continue;
                    }
                    if let Some(re) = first.compiled_postag_regexp.as_ref()
                        && first.text.is_none()
                        && first.compiled_regexp.is_none()
                    {
                        let key = std::sync::Arc::as_ptr(re) as usize;
                        let entry = memo.postag.entry(key).or_insert_with(|| PostagRegexMemo {
                            unknown_ok: re.is_match("UNKNOWN"),
                            results: Vec::new(),
                            evaluated: Vec::new(),
                        });
                        if entry.evaluated.len() < tokens.len() {
                            entry.evaluated.resize(tokens.len(), false);
                            entry.results.resize(tokens.len(), false);
                        }
                        if !entry.evaluated[start_idx] {
                            let unknown_ok = entry.unknown_ok;
                            entry.results[start_idx] =
                                tokens[start_idx].readings().iter().any(|r| {
                                    r.pos_tags().iter().any(|t| re.is_match(t))
                                        || (unknown_ok && reading_tagless(r))
                                });
                            entry.evaluated[start_idx] = true;
                        }
                        let hit = entry.results[start_idx];
                        if !hit && !first.negate_pos {
                            continue;
                        }
                        if hit && first.negate_pos {
                            continue;
                        }
                    }
                    let mut first_preverified = false;
                    if let Some(f) = self.first_literal[rule_idx].as_ref() {
                        let t = tokens[start_idx];
                        let surface = if f.case_sensitive {
                            t.token().token() == f.text
                        } else {
                            og_xml::unifier::eq_ignore_case(t.token().token(), &f.text)
                        };
                        let ok = if f.inflected {
                            inflected_text_matches(
                                t.readings(),
                                t.token().token(),
                                &f.text,
                                f.case_sensitive,
                            )
                        } else {
                            surface
                        };
                        if !ok {
                            continue;
                        }
                    }
                    if let Some(f) = self.first_regex[rule_idx].as_ref() {
                        let key = (std::sync::Arc::as_ptr(&f.re) as usize, f.inflected);
                        let entry = memo.first_match.entry(key).or_default();
                        if entry.evaluated.len() < tokens.len() {
                            entry.evaluated.resize(tokens.len(), false);
                            entry.results.resize(tokens.len(), false);
                        }
                        if !entry.evaluated[start_idx] {
                            let tok = tokens[start_idx];
                            entry.results[start_idx] = if f.inflected {
                                tok.readings().iter().any(|r| {
                                    f.re.is_match(r.lemma().unwrap_or(tok.token().token()))
                                }) || tok.readings().is_empty()
                                    && f.re.is_match(tok.token().token())
                            } else {
                                f.re.is_match(tok.token().token())
                            };
                            entry.evaluated[start_idx] = true;
                        }
                        if !entry.results[start_idx] {
                            continue;
                        }
                        first_preverified = true;
                    }
                    let effective_pattern = &rule.pattern;
                    let mp_result = if first_preverified
                        && !effective_pattern.case_sensitive
                        && !effective_pattern.has_structured_elements
                    {
                        match_tokens_preverified(&effective_pattern.tokens, &tokens, start_idx)
                    } else {
                        match_pattern(effective_pattern, &tokens, start_idx)
                    };
                    if let Some(match_result) = mp_result {
                        let rule_first = match_result
                            .matched_positions
                            .first()
                            .copied()
                            .unwrap_or(usize::MAX);
                        let rule_last = match_result.matched_positions.last().copied().unwrap_or(0);
                        let mut blocked = false;
                        for ap in &rule.antipatterns {
                            if antipattern_matches(ap, &tokens, rule_first, rule_last) {
                                blocked = true;
                                break;
                            }
                        }
                        if blocked {
                            continue;
                        }

                        if rule.filter_class.as_deref() == Some(ENGLISH_WORD_FILTER_CLASS)
                            && !self.passes_english_word_filter(rule, &tokens, &match_result)
                        {
                            continue;
                        }

                        if rule.filter_class.as_deref() == Some(EN_PARTIAL_POSTAG_FILTER_CLASS)
                            && !self.passes_partial_postag_filter(rule_idx, &tokens, &match_result)
                        {
                            continue;
                        }

                        let mut unify_survivors: Option<Vec<(usize, Vec<u32>)>> = None;
                        if let Some(ref spec) = rule.unify {
                            let feature_list: Vec<&str> = if spec.features.is_empty() {
                                vec![spec.feature.as_str()]
                            } else {
                                spec.features.iter().map(String::as_str).collect()
                            };
                            let mut features: Vec<(String, Vec<&str>)> =
                                Vec::with_capacity(feature_list.len());
                            let mut eq_matchers: std::collections::HashMap<
                                (&str, &str),
                                &PosMatcher,
                            > = std::collections::HashMap::new();
                            let mut all_found = true;
                            for f in &feature_list {
                                match self.unifications.iter().find(|(name, _)| name == f) {
                                    Some((_, eqs)) => {
                                        for (ty, re) in eqs {
                                            eq_matchers.insert((f, ty.as_str()), re);
                                        }
                                        features.push((
                                            (*f).to_string(),
                                            eqs.iter().map(|(ty, _)| ty.as_str()).collect(),
                                        ));
                                    }
                                    None => {
                                        all_found = false;
                                        break;
                                    }
                                }
                            }
                            if !all_found {
                                continue;
                            }
                            let has_structured = rule.pattern.has_structured_elements;
                            let (start, end) = if has_structured {
                                (spec.elem_start, spec.elem_end)
                            } else {
                                (spec.token_start, spec.token_end)
                            };
                            if end <= start {
                                if spec.negate {
                                    unify_survivors = None;
                                } else {
                                    continue;
                                }
                            } else {
                                let (slot_start, slot_end) = slot_range_for_elements(
                                    &match_result.element_counts,
                                    start,
                                    end,
                                );
                                let clipped_end =
                                    slot_end.min(match_result.matched_positions.len());
                                let clipped_start = slot_start.min(clipped_end);
                                let indexed_unify: Vec<(usize, usize)> = match_result
                                    .matched_positions[clipped_start..clipped_end]
                                    .iter()
                                    .enumerate()
                                    .map(|(slot, &p)| {
                                        (
                                            p,
                                            element_for_slot(
                                                &match_result.element_counts,
                                                slot + clipped_start,
                                            ),
                                        )
                                    })
                                    .collect();
                                if indexed_unify.len() != end - start && !spec.negate {
                                    continue;
                                }
                                let mut plans: Vec<og_xml::unifier::UnifyElemPlan> =
                                    Vec::with_capacity(end - start);
                                let mut fed_positions: Vec<usize> = Vec::with_capacity(end - start);
                                for &(pos, pattern_idx) in &indexed_unify {
                                    let token = tokens[pos];
                                    let neutral = if has_structured {
                                        match rule.pattern.elements.get(pattern_idx) {
                                            Some(
                                                og_xml::compiler::CompiledPatternElement::Token(t),
                                            ) => t.unify_neutral,
                                            _ => false,
                                        }
                                    } else {
                                        rule.pattern
                                            .tokens
                                            .get(pattern_idx)
                                            .is_some_and(|t| t.unify_neutral)
                                    };
                                    if neutral {
                                        plans.push(og_xml::unifier::UnifyElemPlan {
                                            elem_idx: pattern_idx,
                                            neutral_readings: vec![token.readings().len()],
                                            reading_sets: Vec::new(),
                                        });
                                        fed_positions.push(pos);
                                        continue;
                                    }
                                    let criteria: Vec<&CompiledPatternToken> =
                                        if has_structured {
                                            match rule.pattern.elements.get(pattern_idx) {
                                            Some(
                                                og_xml::compiler::CompiledPatternElement::Token(t),
                                            ) => vec![t.as_ref()],
                                            Some(og_xml::compiler::CompiledPatternElement::AndGroup(
                                                cs,
                                            )) => match cs.first() {
                                                Some(head) => vec![head],
                                                None => continue,
                                            },
                                            Some(og_xml::compiler::CompiledPatternElement::OrGroup(
                                                alts,
                                            )) => alts.iter().collect(),
                                            None => continue,
                                        }
                                        } else {
                                            match rule.pattern.tokens.get(pattern_idx) {
                                                Some(t) => vec![t],
                                                None => continue,
                                            }
                                        };
                                    let surface = og_xml::unifier::surface_text(token);
                                    let untagged = og_xml::unifier::has_no_tag_reading(token);
                                    let mut all_variants: Vec<
                                        Vec<Vec<og_xml::unifier::UnifyReading<'_>>>,
                                    > = Vec::with_capacity(criteria.len());
                                    for criterion in &criteria {
                                        let mut reading_tags: Vec<
                                            og_xml::unifier::UnifyReading<'_>,
                                        > = Vec::with_capacity(token.readings().len());
                                        for (ri, reading) in token.readings().iter().enumerate() {
                                            if og_xml::unifier::reading_matches_element(
                                                criterion, surface, reading, untagged,
                                            ) {
                                                reading_tags.push(og_xml::unifier::UnifyReading {
                                                    tags: reading
                                                        .pos_tags()
                                                        .iter()
                                                        .map(String::as_str)
                                                        .collect(),
                                                    id: ri as u32,
                                                });
                                            }
                                        }
                                        if !reading_tags.is_empty() {
                                            all_variants.push(vec![reading_tags]);
                                        }
                                    }
                                    if all_variants.is_empty() {
                                        continue;
                                    }
                                    plans.push(og_xml::unifier::UnifyElemPlan {
                                        elem_idx: pattern_idx,
                                        neutral_readings: Vec::new(),
                                        reading_sets: all_variants,
                                    });
                                    fed_positions.push(pos);
                                }
                                let mut uni = og_xml::unifier::Unifier::new(features, eq_matchers);
                                if !og_xml::unifier::eval_unify_plans(
                                    &plans,
                                    0,
                                    end - 1,
                                    spec.negate,
                                    &mut uni,
                                ) {
                                    continue;
                                }
                                if matches!(rule.disambig, DisambigAction::Unify) && !spec.negate {
                                    if fed_positions.len() != end - start {
                                        continue;
                                    }
                                    let Some(survivors) = uni.unified_readings() else {
                                        continue;
                                    };
                                    unify_survivors = Some(
                                        fed_positions
                                            .into_iter()
                                            .zip(survivors)
                                            .filter(|(_, ids)| !ids.is_empty())
                                            .collect(),
                                    );
                                }
                            }
                        }

                        let has_marker = matches!(
                            (rule.pattern.marker_start, rule.pattern.marker_end),
                            (Some(_), Some(_))
                        );
                        let elem_bounds = if rule.pattern.has_structured_elements {
                            (rule.pattern.marker_elem_start, rule.pattern.marker_elem_end)
                        } else {
                            (None, None)
                        };
                        let effective_marker = if let (Some(ms), Some(me)) = elem_bounds {
                            (Some(ms), Some(me))
                        } else {
                            (rule.pattern.marker_start, rule.pattern.marker_end)
                        };
                        let counts = &match_result.element_counts;
                        let indexed_targets: Vec<(usize, usize)> = if let (Some(ms), Some(me)) =
                            effective_marker
                        {
                            let (slot_start, slot_end) = slot_range_for_elements(counts, ms, me);
                            let end = slot_end.min(match_result.matched_positions.len());
                            let start = slot_start.min(end);
                            match_result.matched_positions[start..end]
                                .iter()
                                .enumerate()
                                .map(|(slot, &pos)| (pos, element_for_slot(counts, slot + start)))
                                .collect()
                        } else {
                            match_result
                                .matched_positions
                                .iter()
                                .enumerate()
                                .map(|(slot, &pos)| (pos, element_for_slot(counts, slot)))
                                .collect()
                        };
                        let target_indices: Vec<usize> = if has_marker {
                            indexed_targets.iter().map(|&(pos, _)| pos).collect()
                        } else {
                            indexed_targets
                                .first()
                                .map(|&(pos, _)| vec![pos])
                                .unwrap_or_default()
                        };

                        if matches!(rule.disambig, DisambigAction::FilterAll) {
                            for (pos, pattern_idx) in indexed_targets {
                                let outer = if rule.pattern.has_structured_elements {
                                    match rule.pattern.elements.get(pattern_idx) {
                                        Some(og_xml::compiler::CompiledPatternElement::Token(
                                            t,
                                        )) => Some(t.as_ref()),
                                        Some(
                                            og_xml::compiler::CompiledPatternElement::AndGroup(cs),
                                        ) => cs.first(),
                                        Some(
                                            og_xml::compiler::CompiledPatternElement::OrGroup(alts),
                                        ) => alts.first(),
                                        None => None,
                                    }
                                } else {
                                    rule.pattern.tokens.get(pattern_idx)
                                };
                                if let Some(tok) = outer {
                                    pending_filterall.push((pos, tok));
                                }
                            }
                        } else if matches!(rule.disambig, DisambigAction::Immunize) {
                            let counts = &match_result.element_counts;
                            let (span_start_elem, span_end_elem) =
                                if let (Some(ms), Some(me)) = effective_marker {
                                    (ms, me)
                                } else {
                                    (0, counts.len())
                                };
                            if span_end_elem > span_start_elem
                                && counts[span_start_elem..span_end_elem]
                                    .iter()
                                    .all(|c| *c > 0)
                            {
                                let start_pos = if has_marker {
                                    let slot_start: usize = counts[..span_start_elem].iter().sum();
                                    match_result.matched_positions
                                        [slot_start + counts[span_start_elem] - 1]
                                } else {
                                    match_result
                                        .matched_positions
                                        .first()
                                        .copied()
                                        .unwrap_or(usize::MAX)
                                };
                                if start_pos != usize::MAX {
                                    pending_immunize.extend(
                                        (0..span_end_elem - span_start_elem).map(|i| start_pos + i),
                                    );
                                }
                            }
                        } else if matches!(rule.disambig, DisambigAction::RemovePostag(_))
                            | matches!(rule.disambig, DisambigAction::Match(_))
                        {
                            if let Some(&first) = target_indices.first() {
                                pending_actions.push((
                                    first,
                                    &rule.disambig,
                                    &self.action_regexes[rule_idx],
                                ));
                            }
                        } else if matches!(rule.disambig, DisambigAction::IgnoreSpelling) {
                            if let (Some(&(first, _)), Some(&(last, _))) =
                                (indexed_targets.first(), indexed_targets.last())
                            {
                                pending_ignore_spelling.push((first, last));
                            }
                        } else if matches!(rule.disambig, DisambigAction::Unify)
                            && let Some(ref survivors) = unify_survivors
                        {
                            let mut entries: Vec<(usize, AnalyzedToken, Vec<AnalyzedToken>)> =
                                Vec::with_capacity(survivors.len());
                            for (pos, ids) in survivors.iter() {
                                let atr = tokens[*pos];
                                if ids.len() == atr.readings().len() {
                                    continue;
                                }
                                let base = atr.token().clone();
                                let kept: Vec<AnalyzedToken> = ids
                                    .iter()
                                    .filter_map(|&id| atr.readings().get(id as usize).cloned())
                                    .collect();
                                if kept.is_empty() {
                                    continue;
                                }
                                entries.push((*pos, base, kept));
                            }
                            pending_unify.extend(entries);
                        } else if let Some(wds_len) = match &rule.disambig {
                            DisambigAction::Replace(wds)
                            | DisambigAction::Add(wds)
                            | DisambigAction::Remove(wds)
                                if !wds.is_empty() =>
                            {
                                Some(wds.len())
                            }
                            _ => None,
                        } {
                            let counts = &match_result.element_counts;
                            let (span_start_elem, span_end_elem) =
                                if let (Some(ms), Some(me)) = effective_marker {
                                    (ms, me)
                                } else {
                                    (0, counts.len())
                                };
                            if span_end_elem > span_start_elem
                                && counts[span_start_elem..span_end_elem]
                                    .iter()
                                    .all(|c| *c > 0)
                                && wds_len == span_end_elem - span_start_elem
                            {
                                let start_pos = if has_marker {
                                    let slot_start: usize = counts[..span_start_elem].iter().sum();
                                    match_result.matched_positions
                                        [slot_start + counts[span_start_elem] - 1]
                                } else {
                                    match_result
                                        .matched_positions
                                        .first()
                                        .copied()
                                        .unwrap_or(usize::MAX)
                                };
                                let targets: Vec<usize> =
                                    (0..wds_len).map(|i| start_pos + i).collect();
                                pending_wd_actions.push((
                                    targets,
                                    &rule.disambig,
                                    &self.action_regexes[rule_idx],
                                ));
                            }
                        } else {
                            for token_idx in target_indices {
                                pending_actions.push((
                                    token_idx,
                                    &rule.disambig,
                                    &self.action_regexes[rule_idx],
                                ));
                            }
                        }
                    }
                }
            }

            let mutated = !(pending_actions.is_empty()
                && pending_wd_actions.is_empty()
                && pending_filterall.is_empty()
                && pending_ignore_spelling.is_empty()
                && pending_immunize.is_empty()
                && pending_unify.is_empty());

            for (non_ws_idx, action, matcher) in pending_actions {
                apply_disambiguation(sentence.tokens_mut(), non_ws_idx, action, matcher);
            }

            for (targets, action, matcher) in pending_wd_actions {
                match action {
                    DisambigAction::Replace(wds) => {
                        apply_replace_wds(sentence.tokens_mut(), &targets, wds);
                    }
                    DisambigAction::Add(wds) => {
                        apply_add_wds(sentence.tokens_mut(), &targets, wds);
                    }
                    DisambigAction::Remove(_) => {
                        let DisambigActionRegex::Remove(matchers) = matcher else {
                            continue;
                        };
                        apply_remove_wds(sentence.tokens_mut(), &targets, matchers);
                    }
                    _ => {}
                }
            }

            for (non_ws_idx, pattern_token) in pending_filterall {
                apply_filterall(sentence.tokens_mut(), non_ws_idx, pattern_token);
            }

            let tokens = sentence.tokens_mut();
            let nw: Vec<usize> = (0..tokens.len())
                .filter(|&i| !tokens[i].is_whitespace())
                .collect();
            for (first, last) in pending_ignore_spelling {
                if let (Some(&start), Some(&end)) = (nw.get(first), nw.get(last)) {
                    for token in &mut tokens[start..=end] {
                        token.ignore_spelling();
                    }
                }
            }
            for non_ws_idx in pending_immunize {
                if let Some(&real_idx) = nw.get(non_ws_idx) {
                    tokens[real_idx].immunize();
                }
            }
            for (non_ws_idx, base, kept) in pending_unify {
                if let Some(&real_idx) = nw.get(non_ws_idx) {
                    let chunks = tokens[real_idx].chunks().to_vec();
                    let ignored_by_speller = tokens[real_idx].is_ignored_by_speller();
                    let was_immunized = tokens[real_idx].is_immunized();
                    tokens[real_idx] = AnalyzedTokenReadings::new(base).with_readings(kept);
                    tokens[real_idx].set_chunks(chunks);
                    if ignored_by_speller {
                        tokens[real_idx].ignore_spelling();
                    }
                    if was_immunized {
                        tokens[real_idx].immunize();
                    }
                }
            }
            mutated
        }
    }
}

#[derive(Default)]
struct RegexMemo {
    postag: std::collections::HashMap<usize, PostagRegexMemo>,
    first_match: std::collections::HashMap<(usize, bool), FirstRegexMemo>,
}

struct PostagRegexMemo {
    unknown_ok: bool,
    results: Vec<bool>,
    evaluated: Vec<bool>,
}

#[derive(Default)]
struct FirstRegexMemo {
    results: Vec<bool>,
    evaluated: Vec<bool>,
}

struct SentenceEnv {
    nw: Vec<usize>,
    raw_texts: std::collections::HashSet<String>,
    lower_texts: std::collections::HashSet<String>,
    pos_tags: Vec<std::collections::HashSet<String>>,
}

impl SentenceEnv {
    fn build(sentence: &AnalyzedSentence) -> Self {
        let tokens = sentence.tokens();
        let nw: Vec<usize> = (0..tokens.len())
            .filter(|&i| !tokens[i].is_whitespace())
            .collect();
        let raw_texts = nw
            .iter()
            .map(|&i| tokens[i].token().token().to_string())
            .collect();
        let lower_texts = nw
            .iter()
            .map(|&i| tokens[i].token().token().to_lowercase())
            .collect();
        let pos_tags = nw
            .iter()
            .map(|&i| {
                tokens[i]
                    .readings()
                    .iter()
                    .flat_map(|r| r.pos_tags().iter().cloned())
                    .collect()
            })
            .collect();
        Self {
            nw,
            raw_texts,
            lower_texts,
            pos_tags,
        }
    }

    fn token_refs<'a>(&self, sentence: &'a AnalyzedSentence) -> Vec<&'a AnalyzedTokenReadings> {
        let tokens = sentence.tokens();
        self.nw.iter().map(|&i| &tokens[i]).collect()
    }
}

struct MatchResult {
    matched_positions: Vec<usize>,
    element_counts: Vec<usize>,
}

fn reference_text_matches(
    pt: &og_xml::compiler::CompiledPatternToken,
    tokens: &[&AnalyzedTokenReadings],
    token_idx: usize,
    matched_so_far: &[usize],
) -> bool {
    let Some(no) = pt.match_no else {
        return true;
    };
    if no < 0 {
        return false;
    }
    let Some(&ref_idx) = matched_so_far.get(no as usize) else {
        return false;
    };
    let Some(ref_tok) = tokens.get(ref_idx) else {
        return false;
    };
    let surface = tokens[token_idx].token().token();
    let matches = if pt.case_sensitive {
        surface == ref_tok.token().token()
    } else {
        og_xml::unifier::eq_ignore_case(surface, ref_tok.token().token())
    };
    if pt.negate { !matches } else { matches }
}

fn element_for_slot(counts: &[usize], slot: usize) -> usize {
    let mut acc = 0usize;
    for (i, &c) in counts.iter().enumerate() {
        if c == 0 {
            continue;
        }
        if acc == slot {
            return i;
        }
        acc += 1;
    }
    counts.len().saturating_sub(1)
}

fn slot_range_for_elements(counts: &[usize], start_elem: usize, end_elem: usize) -> (usize, usize) {
    let sum_to = |n: usize| -> usize { counts.iter().take(n).sum() };
    (sum_to(start_elem), sum_to(end_elem))
}

fn match_pattern(
    pattern: &CompiledPattern,
    tokens: &[&AnalyzedTokenReadings],
    start_idx: usize,
) -> Option<MatchResult> {
    let elements = &pattern.elements;
    let has_structured = pattern.has_structured_elements;

    if has_structured {
        match_elements(elements, tokens, start_idx, &[])
    } else {
        match_tokens(&pattern.tokens, tokens, start_idx)
    }
}

fn compile_disambig_element(
    compiler: &XmlCompiler,
    element: &og_xml::types::XmlPatternElement,
    pattern_case_sensitive: bool,
) -> og_xml::compiler::CompiledPatternElement {
    use og_xml::compiler::CompiledPatternElement;
    use og_xml::types::XmlPatternElement;
    match element {
        XmlPatternElement::Token(t) => CompiledPatternElement::Token(Box::new(
            compiler.compile_token_with_default(t, pattern_case_sensitive),
        )),
        XmlPatternElement::OrGroup(g) => CompiledPatternElement::OrGroup(
            g.alternatives
                .iter()
                .map(|t| compiler.compile_token_with_default(t, pattern_case_sensitive))
                .collect(),
        ),
        XmlPatternElement::AndGroup(g) => CompiledPatternElement::AndGroup(
            g.constraints
                .iter()
                .map(|t| compiler.compile_token_with_default(t, pattern_case_sensitive))
                .collect(),
        ),
    }
}

fn match_tokens(
    pattern_tokens: &[CompiledPatternToken],
    tokens: &[&AnalyzedTokenReadings],
    start_idx: usize,
) -> Option<MatchResult> {
    match_tokens_recursive(pattern_tokens, tokens, start_idx, 0, false, &[])
}

fn match_tokens_preverified(
    pattern_tokens: &[CompiledPatternToken],
    tokens: &[&AnalyzedTokenReadings],
    start_idx: usize,
) -> Option<MatchResult> {
    match_tokens_recursive(pattern_tokens, tokens, start_idx, 0, true, &[])
}

fn match_tokens_recursive(
    pattern_tokens: &[CompiledPatternToken],
    tokens: &[&AnalyzedTokenReadings],
    token_idx: usize,
    pattern_idx: usize,
    first_text_verified: bool,
    matched_so_far: &[usize],
) -> Option<MatchResult> {
    if pattern_idx >= pattern_tokens.len() {
        return Some(MatchResult {
            matched_positions: Vec::new(),
            element_counts: Vec::new(),
        });
    }
    if token_idx >= tokens.len() {
        let all_optional = pattern_tokens[pattern_idx..]
            .iter()
            .all(|pt| pt.min.unwrap_or(1) == 0);
        if all_optional {
            return Some(MatchResult {
                matched_positions: Vec::new(),
                element_counts: vec![0; pattern_tokens.len() - pattern_idx],
            });
        }
        return None;
    }

    let pt = &pattern_tokens[pattern_idx];

    if pt.min.unwrap_or(1) == 0
        && later_token_matches_at(
            pattern_tokens,
            tokens,
            token_idx,
            pattern_idx,
            matched_so_far,
        )
    {
        let mut result = match_tokens_recursive(
            pattern_tokens,
            tokens,
            token_idx,
            pattern_idx + 1,
            first_text_verified && pattern_idx == 0,
            matched_so_far,
        )?;
        result.element_counts.insert(0, 0);
        return Some(result);
    }

    let first_matched = if pattern_idx == 0 && first_text_verified {
        token_matches_text_verified(pt, tokens, token_idx)
    } else {
        token_matches(pt, tokens, token_idx)
    } && reference_text_matches(pt, tokens, token_idx, matched_so_far);

    if !first_matched {
        return None;
    }

    let scope_next: Vec<&CompiledException> = pt
        .exceptions
        .iter()
        .filter(|exc| exc.scope.as_deref() == Some("next"))
        .collect();
    let scope_next_vetoes = |next_token: usize, count: usize| {
        !scope_next.is_empty()
            && ((token_idx + count)..=next_token.min(tokens.len().saturating_sub(1))).any(|q| {
                scope_next
                    .iter()
                    .any(|exc| exception_matches(exc, tokens[q], space_before_at(tokens, q)))
            })
    };
    let max_occurrences = match pt.max {
        None | Some(1) | Some(0) => 1,
        Some(-1) => usize::MAX,
        Some(m) => m as usize,
    };
    let remaining = pattern_tokens.len() - pattern_idx - 1;
    let guard = tokens.len() as isize - remaining as isize;
    let mut count = 1usize;
    while count < max_occurrences
        && ((token_idx + count) as isize) < guard
        && token_matches(pt, tokens, token_idx + count)
        && reference_text_matches(pt, tokens, token_idx + count, matched_so_far)
    {
        count += 1;
    }
    let scan_limit = if pt.skip < 0 {
        tokens.len()
    } else {
        pt.skip as usize
    };
    for dist in 0..=scan_limit {
        let next_token = token_idx + count + dist;
        if next_token > tokens.len() {
            break;
        }
        if scope_next_vetoes(next_token, count) {
            continue;
        }
        let mut extended: Vec<usize> = matched_so_far.to_vec();
        extended.extend(token_idx..token_idx + count);
        if !next_token_commits_at(
            pattern_tokens,
            tokens,
            next_token,
            pattern_idx + 1,
            &extended,
        ) {
            continue;
        }
        if let Some(mut result) = match_tokens_recursive(
            pattern_tokens,
            tokens,
            next_token,
            pattern_idx + 1,
            false,
            &extended,
        ) {
            for p in (token_idx..token_idx + count).rev() {
                result.matched_positions.insert(0, p);
            }
            result.element_counts.insert(0, count);
            return Some(result);
        }
        return None;
    }

    None
}

fn later_token_matches_at(
    pattern_tokens: &[CompiledPatternToken],
    tokens: &[&AnalyzedTokenReadings],
    token_idx: usize,
    pattern_idx: usize,
    matched_so_far: &[usize],
) -> bool {
    for pt in &pattern_tokens[pattern_idx + 1..] {
        if token_matches(pt, tokens, token_idx)
            && reference_text_matches(pt, tokens, token_idx, matched_so_far)
        {
            return true;
        }
        if pt.min.unwrap_or(1) > 0 {
            return false;
        }
    }
    false
}

fn next_token_commits_at(
    pattern_tokens: &[CompiledPatternToken],
    tokens: &[&AnalyzedTokenReadings],
    token_idx: usize,
    pattern_idx: usize,
    matched_so_far: &[usize],
) -> bool {
    let Some(pt) = pattern_tokens.get(pattern_idx) else {
        return true;
    };
    if token_idx >= tokens.len() {
        return pattern_tokens[pattern_idx..]
            .iter()
            .all(|pt| pt.min.unwrap_or(1) == 0);
    }
    if pt.min.unwrap_or(1) == 0
        && later_token_matches_at(
            pattern_tokens,
            tokens,
            token_idx,
            pattern_idx,
            matched_so_far,
        )
    {
        return true;
    }
    token_matches(pt, tokens, token_idx)
        && reference_text_matches(pt, tokens, token_idx, matched_so_far)
}

fn token_matches_text_verified(
    pt: &CompiledPatternToken,
    tokens: &[&AnalyzedTokenReadings],
    token_idx: usize,
) -> bool {
    if token_idx >= tokens.len() {
        return false;
    }
    let token = tokens[token_idx];
    if token.is_immunized() {
        return false;
    }

    if token_idx > 0 {
        let prev = tokens[token_idx - 1];
        let spaced = token.token().start() != prev.token().end();
        match pt.space_before.as_deref() {
            Some("no") if spaced => return false,
            Some("yes") if !spaced => return false,
            _ => {}
        }
    }

    let readings = token.readings();
    let pos_ok = if readings.is_empty() {
        pt.postag.is_none()
    } else {
        readings.iter().any(|r| reading_pos_matches_disambig(pt, r))
    };
    if !pos_ok {
        return false;
    }

    if token_idx > 0 {
        let prev = tokens[token_idx - 1];
        for exc in &pt.exceptions {
            if exc.scope.as_deref() == Some("previous")
                && exception_matches(exc, prev, space_before_at(tokens, token_idx - 1))
            {
                return false;
            }
        }
    }

    token_matches_tail(
        pt,
        token,
        token_idx > 0 && {
            let prev = tokens[token_idx - 1];
            token.token().start() != prev.token().end()
        },
    )
}

fn token_matches_tail(
    pt: &CompiledPatternToken,
    token: &AnalyzedTokenReadings,
    token_space_before: bool,
) -> bool {
    if let Some(ref chunk) = pt.chunk
        && !token.has_chunk(chunk)
    {
        return false;
    }
    if let Some(ref chunk_re) = pt.chunk_re
        && !token.has_chunk_matching(chunk_re)
    {
        return false;
    }

    for exc in &pt.exceptions {
        if exc.scope.as_deref() == Some("next") || exc.scope.as_deref() == Some("previous") {
            continue;
        }
        if exception_matches(exc, token, token_space_before) {
            return false;
        }
    }

    true
}

fn exception_reading_text_matches(
    exc: &CompiledException,
    reading: &og_core::AnalyzedToken,
    surface: &str,
) -> bool {
    let Some(ref exc_text) = exc.text else {
        return true;
    };
    if let Some(ref re) = exc.compiled_regexp {
        let test = if exc.inflected {
            reading.lemma().unwrap_or(surface)
        } else {
            surface
        };
        return re.is_match(test);
    }
    if exc.inflected {
        let test = reading.lemma().unwrap_or(surface);
        let matched = if exc.case_sensitive {
            test == exc_text
        } else {
            og_xml::unifier::eq_ignore_case(test, exc_text)
        };
        return matched;
    }
    if exc.case_sensitive {
        surface == exc_text
    } else {
        og_xml::unifier::eq_ignore_case(surface, exc_text)
    }
}

fn exception_reading_pos_matches(exc: &CompiledException, r: &og_core::AnalyzedToken) -> bool {
    let Some(ref postag) = exc.postag else {
        return true;
    };
    if exc.compiled_postag_regexp.is_some() || exc.compiled_postag_regexp_fancy.is_some() {
        let plain = exc
            .compiled_postag_regexp
            .as_ref()
            .is_some_and(|re| r.pos_tags().iter().any(|t| re.is_match(t)));
        let fancy = exc
            .compiled_postag_regexp_fancy
            .as_ref()
            .is_some_and(|re| r.pos_tags().iter().any(|t| re.is_match(t).unwrap_or(false)));
        let unknown = exc
            .compiled_postag_regexp
            .as_ref()
            .is_some_and(|re| re.is_match("UNKNOWN"))
            || exc
                .compiled_postag_regexp_fancy
                .as_ref()
                .is_some_and(|re| re.is_match("UNKNOWN").unwrap_or(false));
        return (plain || fancy || (unknown && reading_tagless(r))) != exc.negate_pos;
    }
    if postag == "UNKNOWN" {
        return reading_tagless(r) != exc.negate_pos;
    }
    r.pos_tags().iter().any(|t| t == postag) != exc.negate_pos
}

fn exception_matches(
    exc: &CompiledException,
    token: &AnalyzedTokenReadings,
    token_space_before: bool,
) -> bool {
    if exc
        .space_before
        .is_some_and(|required| required != token_space_before)
    {
        return false;
    }
    let text = token.token().token();
    let readings = token.readings();
    let per_reading = || {
        if readings.is_empty() {
            let synthetic = og_core::AnalyzedToken::new(text, 0, text.len());
            return exception_reading_text_matches(exc, &synthetic, text)
                && exception_reading_pos_matches(exc, &synthetic);
        }
        readings.iter().any(|r| {
            exception_reading_text_matches(exc, r, text) && exception_reading_pos_matches(exc, r)
        })
    };
    let has_text_constraint = exc.text.is_some();
    let has_pos_constraint = exc.postag.is_some();
    let exc_matches = if has_text_constraint {
        per_reading()
    } else if has_pos_constraint {
        readings
            .iter()
            .any(|r| exception_reading_pos_matches(exc, r))
    } else {
        false
    };
    if exc.negate {
        !exc_matches
    } else {
        exc_matches
    }
}

fn match_elements(
    elements: &[CompiledPatternElement],
    tokens: &[&AnalyzedTokenReadings],
    start_idx: usize,
    prior: &[usize],
) -> Option<MatchResult> {
    let mut positions: Vec<usize> = Vec::new();
    let mut counts = Vec::new();
    let mut token_idx = start_idx;
    let matched_before = prior.to_vec();

    for (elem_idx, elem) in elements.iter().enumerate() {
        if token_idx >= tokens.len() {
            let all_optional = elements[elem_idx..].iter().all(|e| match e {
                CompiledPatternElement::Token(pt) => pt.min.unwrap_or(1) == 0,
                CompiledPatternElement::OrGroup(alts) => {
                    alts.first().is_some_and(|a| a.min.unwrap_or(1) == 0)
                }
                CompiledPatternElement::AndGroup(cs) => {
                    cs.first().is_some_and(|c| c.min.unwrap_or(1) == 0)
                }
            });
            if all_optional {
                counts.extend(std::iter::repeat_n(0, elements.len() - elem_idx));
                return Some(MatchResult {
                    matched_positions: positions,
                    element_counts: counts,
                });
            }
            return None;
        }

        if element_min(elem).unwrap_or(1) == 0
            && later_element_matches_at(elements, tokens, token_idx, elem_idx, &matched_before)
        {
            counts.push(0);
            continue;
        }
        let (matched, rep, min) = match elem {
            CompiledPatternElement::Token(pt) => (
                token_matches(pt, tokens, token_idx)
                    && reference_text_matches(pt, tokens, token_idx, &matched_before),
                Some(pt.as_ref()),
                pt.min,
            ),
            CompiledPatternElement::OrGroup(alternatives) => (
                alternatives.iter().any(|alt| {
                    token_matches(alt, tokens, token_idx)
                        && reference_text_matches(alt, tokens, token_idx, &matched_before)
                }),
                alternatives.first(),
                alternatives.first().and_then(|a| a.min),
            ),
            CompiledPatternElement::AndGroup(constraints) => (
                constraints.iter().all(|c| {
                    token_matches(c, tokens, token_idx)
                        && reference_text_matches(c, tokens, token_idx, &matched_before)
                }),
                constraints.first(),
                constraints.first().and_then(|c| c.min),
            ),
        };
        if !matched {
            if min.unwrap_or(1) == 0 {
                counts.push(0);
                continue;
            }
            return None;
        }
        let rep = rep.unwrap_or_else(|| unreachable!("matched element has a representative"));
        let scope_next: Vec<&CompiledException> = rep
            .exceptions
            .iter()
            .filter(|exc| exc.scope.as_deref() == Some("next"))
            .collect();
        let max_occurrences = match rep.max {
            None | Some(1) | Some(0) => 1,
            Some(-1) => usize::MAX,
            Some(m) => m as usize,
        };
        let remaining = elements.len() - elem_idx - 1;
        let guard = tokens.len() as isize - remaining as isize;
        let mut count = 1usize;
        while count < max_occurrences
            && ((token_idx + count) as isize) < guard
            && element_matches_full(elem, tokens, token_idx + count, &matched_before)
        {
            count += 1;
        }
        if rep.skip == 0 && scope_next.is_empty() {
            positions.extend(token_idx..token_idx + count);
            counts.push(count);
            token_idx += count;
            continue;
        }
        let max_skip = if rep.skip < 0 {
            tokens.len()
        } else {
            rep.skip as usize
        };
        for off in 0..=max_skip {
            let next = token_idx + count + off;
            if next > tokens.len() {
                break;
            }
            if !scope_next.is_empty()
                && ((token_idx + count)..=next.min(tokens.len().saturating_sub(1))).any(|q| {
                    scope_next
                        .iter()
                        .any(|exc| exception_matches(exc, tokens[q], space_before_at(tokens, q)))
                })
            {
                continue;
            }
            let mut next_prior = matched_before.clone();
            next_prior.extend(positions.iter().copied());
            next_prior.extend(token_idx..token_idx + count);
            if !element_commits_at(&elements[elem_idx + 1..], tokens, next, 0, &next_prior) {
                continue;
            }
            if let Some(rest) = match_elements(&elements[elem_idx + 1..], tokens, next, &next_prior)
            {
                let mut result = MatchResult {
                    matched_positions: positions.clone(),
                    element_counts: counts.clone(),
                };
                result
                    .matched_positions
                    .extend(token_idx..token_idx + count);
                result.element_counts.push(count);
                result.matched_positions.extend(rest.matched_positions);
                result.element_counts.extend(rest.element_counts);
                return Some(result);
            }
            return None;
        }
        return None;
    }

    Some(MatchResult {
        matched_positions: positions,
        element_counts: counts,
    })
}

fn element_min(element: &CompiledPatternElement) -> Option<i32> {
    match element {
        CompiledPatternElement::Token(pt) => pt.min,
        CompiledPatternElement::OrGroup(alts) => alts.first().and_then(|a| a.min),
        CompiledPatternElement::AndGroup(cs) => cs.first().and_then(|c| c.min),
    }
}

fn element_matches_full(
    element: &CompiledPatternElement,
    tokens: &[&AnalyzedTokenReadings],
    pos: usize,
    prior: &[usize],
) -> bool {
    match element {
        CompiledPatternElement::Token(pt) => {
            token_matches(pt, tokens, pos) && reference_text_matches(pt, tokens, pos, prior)
        }
        CompiledPatternElement::OrGroup(alts) => alts.iter().any(|alt| {
            token_matches(alt, tokens, pos) && reference_text_matches(alt, tokens, pos, prior)
        }),
        CompiledPatternElement::AndGroup(cs) => cs.iter().all(|c| {
            token_matches(c, tokens, pos) && reference_text_matches(c, tokens, pos, prior)
        }),
    }
}

fn later_element_matches_at(
    elements: &[CompiledPatternElement],
    tokens: &[&AnalyzedTokenReadings],
    pos: usize,
    elem_idx: usize,
    prior: &[usize],
) -> bool {
    for el in &elements[elem_idx + 1..] {
        if element_matches_full(el, tokens, pos, prior) {
            return true;
        }
        if element_min(el).unwrap_or(1) > 0 {
            return false;
        }
    }
    false
}

fn element_commits_at(
    elements: &[CompiledPatternElement],
    tokens: &[&AnalyzedTokenReadings],
    pos: usize,
    elem_idx: usize,
    prior: &[usize],
) -> bool {
    let Some(element) = elements.get(elem_idx) else {
        return true;
    };
    if pos >= tokens.len() {
        return elements[elem_idx..]
            .iter()
            .all(|e| element_min(e).unwrap_or(1) == 0);
    }
    if element_min(element).unwrap_or(1) == 0
        && later_element_matches_at(elements, tokens, pos, elem_idx, prior)
    {
        return true;
    }
    element_matches_full(element, tokens, pos, prior)
}

fn space_before_at(tokens: &[&AnalyzedTokenReadings], idx: usize) -> bool {
    match (idx.checked_sub(1), tokens.get(idx)) {
        (Some(prev), Some(tok)) if prev < tokens.len() => {
            tok.token().start() != tokens[prev].token().end()
        }
        _ => false,
    }
}

fn inflected_text_matches(
    readings: &[og_core::AnalyzedToken],
    surface: &str,
    pattern_text: &str,
    case_sensitive: bool,
) -> bool {
    if readings.is_empty() {
        return if case_sensitive {
            surface == pattern_text
        } else {
            og_xml::unifier::eq_ignore_case(surface, pattern_text)
        };
    }
    readings.iter().any(|r| {
        let test = r.lemma().unwrap_or(surface);
        if case_sensitive {
            test == pattern_text
        } else {
            og_xml::unifier::eq_ignore_case(test, pattern_text)
        }
    })
}

fn reading_text_matches(
    pt: &CompiledPatternToken,
    reading: &og_core::AnalyzedToken,
    surface: &str,
) -> bool {
    if let Some(ref pattern_text) = pt.text {
        let test = if pt.inflected {
            reading.lemma().unwrap_or(surface)
        } else {
            surface
        };
        let matched = if pt.case_sensitive {
            test == pattern_text
        } else {
            og_xml::unifier::eq_ignore_case(test, pattern_text)
        };
        return matched != pt.negate;
    }
    if let Some(ref re) = pt.compiled_regexp {
        let re_to_use = if !pt.case_sensitive {
            pt.compiled_regexp_ci.as_ref().unwrap_or(re)
        } else {
            re
        };
        let test = if pt.inflected {
            reading.lemma().unwrap_or(surface)
        } else {
            surface
        };
        return re_to_use.is_match(test) != pt.negate;
    }
    if pt.is_regexp
        && let Some(ref re) = pt.compiled_regexp_fancy
    {
        let test = if pt.inflected {
            reading.lemma().unwrap_or(surface)
        } else {
            surface
        };
        return re.is_match(test).unwrap_or(false) != pt.negate;
    }
    !pt.negate
}

fn reading_pos_matches_disambig(pt: &CompiledPatternToken, r: &og_core::AnalyzedToken) -> bool {
    let Some(ref postag) = pt.postag else {
        return true;
    };
    if pt.compiled_postag_regexp.is_some() || pt.compiled_postag_regexp_fancy.is_some() {
        let plain = pt
            .compiled_postag_regexp
            .as_ref()
            .is_some_and(|re| r.pos_tags().iter().any(|t| re.is_match(t)));
        let fancy = pt
            .compiled_postag_regexp_fancy
            .as_ref()
            .is_some_and(|re| r.pos_tags().iter().any(|t| re.is_match(t).unwrap_or(false)));
        let unknown = pt
            .compiled_postag_regexp
            .as_ref()
            .is_some_and(|re| re.is_match("UNKNOWN"))
            || pt
                .compiled_postag_regexp_fancy
                .as_ref()
                .is_some_and(|re| re.is_match("UNKNOWN").unwrap_or(false));
        return (plain || fancy || (unknown && reading_tagless(r))) != pt.negate_pos;
    }
    if postag == "UNKNOWN" {
        return reading_tagless(r) != pt.negate_pos;
    }
    r.pos_tags().iter().any(|t| t == postag) != pt.negate_pos
}

fn token_readings_match(pt: &CompiledPatternToken, token: &AnalyzedTokenReadings) -> bool {
    let surface = token.token().token();
    let readings = token.readings();
    if readings.is_empty() {
        return pt.postag.is_none()
            && reading_text_matches(
                pt,
                &og_core::AnalyzedToken::new(surface, 0, surface.len()),
                surface,
            );
    }
    readings
        .iter()
        .any(|r| reading_text_matches(pt, r, surface) && reading_pos_matches_disambig(pt, r))
}

fn token_matches(
    pt: &CompiledPatternToken,
    tokens: &[&AnalyzedTokenReadings],
    token_idx: usize,
) -> bool {
    if token_idx >= tokens.len() {
        return false;
    }
    let token = tokens[token_idx];
    if token.is_immunized() {
        return false;
    }

    if token_idx > 0 {
        let prev = tokens[token_idx - 1];
        let spaced = token.token().start() != prev.token().end();
        match pt.space_before.as_deref() {
            Some("no") if spaced => return false,
            Some("yes") if !spaced => return false,
            _ => {}
        }
    }

    if !token_readings_match(pt, token) {
        return false;
    }

    if token_idx > 0 {
        let prev = tokens[token_idx - 1];
        for exc in &pt.exceptions {
            if exc.scope.as_deref() == Some("previous")
                && exception_matches(exc, prev, space_before_at(tokens, token_idx - 1))
            {
                return false;
            }
        }
    }

    token_matches_tail(
        pt,
        token,
        token_idx > 0 && {
            let prev = tokens[token_idx - 1];
            token.token().start() != prev.token().end()
        },
    )
}

fn antipattern_matches(
    antipattern: &CompiledPattern,
    tokens: &[&AnalyzedTokenReadings],
    rule_first: usize,
    rule_last: usize,
) -> bool {
    for start in 0..tokens.len() {
        if let Some(result) = match_pattern(antipattern, tokens, start) {
            let mut anti_first = result
                .matched_positions
                .first()
                .copied()
                .unwrap_or(usize::MAX);
            let mut anti_last = result.matched_positions.last().copied().unwrap_or(0);
            if let (Some(ms), Some(me)) =
                (antipattern.marker_elem_start, antipattern.marker_elem_end)
                && let Some((mk_first, mk_last)) = marker_position_span(&result, ms, me)
            {
                anti_first = mk_first;
                anti_last = mk_last;
            }
            if anti_first <= rule_last && anti_last >= rule_first {
                return true;
            }
        }
    }
    false
}

fn marker_position_span(
    result: &MatchResult,
    marker_elem_start: usize,
    marker_elem_end: usize,
) -> Option<(usize, usize)> {
    let sum_to = |n: usize| -> usize { result.element_counts.iter().take(n).sum() };
    let first_idx = sum_to(marker_elem_start);
    let last_idx = sum_to(marker_elem_end).saturating_sub(1);
    let first = *result.matched_positions.get(first_idx)?;
    let last = *result.matched_positions.get(last_idx)?;
    (first <= last).then_some((first, last))
}

impl XmlDisambiguator {
    fn passes_english_word_filter(
        &self,
        rule: &CompiledDisambigRule,
        tokens: &[&AnalyzedTokenReadings],
        match_result: &MatchResult,
    ) -> bool {
        let Some(tagger) = self.english_tagger.as_ref() else {
            return false;
        };
        let Some(args) = rule.filter_args.as_deref() else {
            return false;
        };
        let arg_value = |key: &str| -> Option<String> {
            args.split_whitespace().find_map(|kv| {
                let (k, v) = kv.split_once(':')?;
                (k == key).then(|| v.to_string())
            })
        };
        let Some(form_positions) = arg_value("formPositions") else {
            return false;
        };
        let forms: Vec<usize> = form_positions
            .split(',')
            .filter_map(|p| p.trim().parse::<usize>().ok())
            .collect();
        if forms.is_empty() {
            return false;
        }
        let postags: Vec<String> = arg_value("postags")
            .map(|v| v.split(',').map(|p| p.trim().to_string()).collect())
            .unwrap_or_default();
        if !postags.is_empty() && postags.len() != forms.len() {
            return false;
        }
        for (i, one_based) in forms.iter().enumerate() {
            let slot = match_result
                .element_counts
                .iter()
                .take(one_based.saturating_sub(1))
                .sum::<usize>();
            let Some(&token_idx) = match_result.matched_positions.get(slot) else {
                return false;
            };
            let Some(token) = tokens.get(token_idx) else {
                return false;
            };
            let readings = tagger.tag_word(token.token().token(), 0, 0);
            if readings.is_empty() {
                return false;
            }
            if let Some(expected) = postags.get(i) {
                let Ok(re) = regex::Regex::new(&format!("^(?:{expected})$")) else {
                    return false;
                };
                if !readings
                    .iter()
                    .any(|r| r.pos_tags().iter().any(|t| re.is_match(t)))
                {
                    return false;
                }
            }
        }
        true
    }

    fn passes_partial_postag_filter(
        &self,
        rule_idx: usize,
        tokens: &[&AnalyzedTokenReadings],
        match_result: &MatchResult,
    ) -> bool {
        let Some(spec) = self.partial_postag[rule_idx].as_ref() else {
            return false;
        };
        let Some(tagger) = self.english_tagger.as_ref() else {
            return false;
        };
        let expected_groups = if spec.two_groups { 2 } else { 1 };
        if spec.token_re.captures_len() - 1 != expected_groups {
            return false;
        }
        let Some(&first) = match_result.matched_positions.first() else {
            return false;
        };
        let last = match_result
            .matched_positions
            .last()
            .copied()
            .unwrap_or(first);
        let Some(token) = tokens
            .get(first..=last)
            .and_then(|slice| slice.get(spec.no.checked_sub(1)?))
        else {
            return false;
        };
        let token_text = format!("{}{}{}", spec.prefix, token.token().token(), spec.suffix);
        let Some(captures) = spec.token_re.captures(&token_text) else {
            return false;
        };
        let mut partial_token = captures
            .get(1)
            .map_or(String::new(), |m| m.as_str().to_string());
        if expected_groups == 2 {
            partial_token.push_str(captures.get(2).map_or("", |m| m.as_str()));
        }
        let readings = tagger.tag_word(&partial_token, 0, 0);
        let mut postag_count = 0usize;
        for reading in &readings {
            for pos in reading.pos_tags() {
                if spec.negate_pos {
                    postag_count += 1;
                    if spec.postag_re.is_match(pos) {
                        return false;
                    }
                } else if spec.postag_re.is_match(pos) {
                    return true;
                }
            }
        }
        postag_count > 0 && spec.negate_pos
    }
}

const EN_PARTIAL_POSTAG_FILTER_CLASS: &str =
    "org.languagetool.rules.en.NoDisambiguationEnglishPartialPosTagFilter";

const ENGLISH_WORD_FILTER_CLASS: &str = "org.languagetool.rules.IsEnglishWordFilter";

fn compile_partial_postag_filter(args: &str) -> Option<PartialPosTagFilterSpec> {
    let mut no: Option<usize> = None;
    let mut regexp: Option<&str> = None;
    let mut postag_regexp: Option<&str> = None;
    let mut negate_pos = false;
    let mut two_groups = false;
    let mut prefix = String::new();
    let mut suffix = String::new();
    for part in args.split_whitespace() {
        let Some((key, val)) = part.split_once(':') else {
            continue;
        };
        match key {
            "no" => no = val.parse::<usize>().ok(),
            "regexp" => regexp = Some(val),
            "postag_regexp" => postag_regexp = Some(val),
            "negate_pos" => negate_pos = true,
            "two_groups_regexp" => two_groups = true,
            "prefix" => prefix = val.to_string(),
            "suffix" => suffix = val.to_string(),
            _ => {}
        }
    }
    let token_re = regex::Regex::new(&format!("^(?:{})$", regexp?)).ok()?;
    let postag_re = regex::Regex::new(&format!("^(?:{})$", postag_regexp?)).ok()?;
    Some(PartialPosTagFilterSpec {
        no: no?,
        token_re,
        postag_re,
        negate_pos,
        two_groups,
        prefix,
        suffix,
    })
}

fn apply_filterall(
    tokens: &mut [AnalyzedTokenReadings],
    token_idx: usize,
    pattern_token: &CompiledPatternToken,
) {
    let Some(postag) = pattern_token.postag.as_deref() else {
        return;
    };
    let re = pattern_token.compiled_postag_regexp.clone().or_else(|| {
        regex::Regex::new(&format!("^(?:{})$", postag))
            .ok()
            .map(std::sync::Arc::new)
    });
    let Some(re) = re else {
        return;
    };
    let mut real_idx: Option<usize> = None;
    let mut non_ws_count = 0;
    for (i, t) in tokens.iter().enumerate() {
        if !t.is_whitespace() {
            if non_ws_count == token_idx {
                real_idx = Some(i);
                break;
            }
            non_ws_count += 1;
        }
    }
    let Some(real_idx) = real_idx else {
        return;
    };
    let prior_chunks = tokens[real_idx].chunks().to_vec();
    let ignored_by_speller = tokens[real_idx].is_ignored_by_speller();
    let was_immunized = tokens[real_idx].is_immunized();
    let base = tokens[real_idx].token().clone();
    let had_sent_end = current_had_sent_end(&[], &tokens[real_idx]);
    let current_readings = tokens[real_idx].readings().to_vec();
    let kept: Vec<AnalyzedToken> = current_readings
        .into_iter()
        .filter(|r| r.pos_tags().iter().any(|t| re.is_match(t)))
        .collect();
    if kept.is_empty() {
        return;
    }
    let mut updated = AnalyzedTokenReadings::new(base).with_readings(kept);
    restore_sent_end_if_lost(&mut updated, had_sent_end);
    updated.set_chunks(prior_chunks);
    if ignored_by_speller {
        updated.ignore_spelling();
    }
    if was_immunized {
        updated.immunize();
    }
    tokens[real_idx] = updated;
}

fn reading_tagless(r: &og_core::AnalyzedToken) -> bool {
    r.pos_tags()
        .iter()
        .all(|t| t == "SENT_END" || t == "PARA_END")
}

fn current_had_sent_end(filtered: &[AnalyzedToken], token: &AnalyzedTokenReadings) -> bool {
    let still_present = filtered
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t == "SENT_END"));
    !still_present
        && token
            .readings()
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t == "SENT_END"))
}

fn restore_sent_end_if_lost(updated: &mut AnalyzedTokenReadings, had_sent_end: bool) {
    if had_sent_end {
        updated.add_sent_end();
    }
}

fn match_new_token(
    readings: &[AnalyzedToken],
    token: &str,
    pos_tag: &str,
    base: &og_core::AnalyzedToken,
) -> Vec<AnalyzedToken> {
    let mut lemma: Option<String> = None;
    for r in readings {
        let Some(tag) = r.pos_tags().first() else {
            continue;
        };
        if tag == pos_tag && r.lemma().is_some() {
            lemma = r.lemma().map(str::to_string);
        }
        if lemma.is_none() {
            lemma = readings.first().and_then(|f| f.lemma()).map(str::to_string);
        }
    }
    readings
        .iter()
        .filter(|r| !r.pos_tags().is_empty())
        .map(|_| {
            let mut at = AnalyzedToken::new(token, base.start(), base.end());
            at.set_pos_tags(vec![pos_tag.to_string()]);
            at.set_lemma(lemma.clone());
            at
        })
        .collect()
}

fn non_ws_to_real(tokens: &[AnalyzedTokenReadings], token_idx: usize) -> Option<usize> {
    let mut non_ws_count = 0;
    for (i, t) in tokens.iter().enumerate() {
        if !t.is_whitespace() {
            if non_ws_count == token_idx {
                return Some(i);
            }
            non_ws_count += 1;
        }
    }
    None
}

fn apply_replace_wds(
    tokens: &mut [AnalyzedTokenReadings],
    targets: &[usize],
    wds: &[og_xml::disambig_types::DisambigWord],
) {
    for (i, w) in wds.iter().enumerate() {
        let Some(&non_ws) = targets.get(i) else {
            return;
        };
        let Some(real_idx) = non_ws_to_real(tokens, non_ws) else {
            return;
        };
        let prior_chunks = tokens[real_idx].chunks().to_vec();
        let ignored_by_speller = tokens[real_idx].is_ignored_by_speller();
        let was_immunized = tokens[real_idx].is_immunized();
        let base = tokens[real_idx].token().clone();
        let had_sent_end = current_had_sent_end(&[], &tokens[real_idx]);
        let token_text = if w.text.is_empty() {
            base.token().to_string()
        } else {
            w.text.clone()
        };
        let mut reading = AnalyzedToken::new(&token_text, base.start(), base.end());
        if let Some(pos) = &w.pos {
            reading.set_pos_tags(vec![pos.clone()]);
        }
        reading.set_lemma(w.lemma.clone().or(Some(token_text)));
        let mut updated = AnalyzedTokenReadings::new(base).with_readings(vec![reading]);
        restore_sent_end_if_lost(&mut updated, had_sent_end);
        updated.set_chunks(prior_chunks);
        if ignored_by_speller {
            updated.ignore_spelling();
        }
        if was_immunized {
            updated.immunize();
        }
        tokens[real_idx] = updated;
    }
}

fn apply_add_wds(
    tokens: &mut [AnalyzedTokenReadings],
    targets: &[usize],
    wds: &[og_xml::disambig_types::DisambigWord],
) {
    for (i, w) in wds.iter().enumerate() {
        let Some(&non_ws) = targets.get(i) else {
            return;
        };
        let Some(real_idx) = non_ws_to_real(tokens, non_ws) else {
            return;
        };
        let prior_chunks = tokens[real_idx].chunks().to_vec();
        let ignored_by_speller = tokens[real_idx].is_ignored_by_speller();
        let was_immunized = tokens[real_idx].is_immunized();
        let base = tokens[real_idx].token().clone();
        let token_text = if w.text.is_empty() {
            base.token().to_string()
        } else {
            w.text.clone()
        };
        let mut new_reading = AnalyzedToken::new(&token_text, base.start(), base.end());
        if let Some(pos) = &w.pos {
            new_reading.set_pos_tags(vec![pos.clone()]);
        }
        new_reading.set_lemma(w.lemma.clone().or(Some(token_text)));
        let exists = tokens[real_idx].readings().iter().any(|r| {
            r.token() == new_reading.token()
                && r.pos_tags() == new_reading.pos_tags()
                && r.lemma() == new_reading.lemma()
        });
        if exists {
            continue;
        }
        let current_readings = tokens[real_idx].readings().to_vec();
        let mut new_readings: Vec<AnalyzedToken> = Vec::with_capacity(current_readings.len() + 1);
        if let Some((last, head)) = current_readings.split_last() {
            new_readings.extend_from_slice(head);
            if !last.pos_tags().is_empty() {
                new_readings.push(last.clone());
            }
        }
        new_readings.push(new_reading);
        let mut updated = AnalyzedTokenReadings::new(base).with_readings(new_readings);
        updated.set_chunks(prior_chunks);
        if ignored_by_speller {
            updated.ignore_spelling();
        }
        if was_immunized {
            updated.immunize();
        }
        tokens[real_idx] = updated;
    }
}

fn apply_remove_wds(
    tokens: &mut [AnalyzedTokenReadings],
    targets: &[usize],
    matchers: &[RemoveWordMatcher],
) {
    for (i, w) in matchers.iter().enumerate() {
        let Some(&non_ws) = targets.get(i) else {
            return;
        };
        let Some(real_idx) = non_ws_to_real(tokens, non_ws) else {
            return;
        };
        let prior_chunks = tokens[real_idx].chunks().to_vec();
        let ignored_by_speller = tokens[real_idx].is_ignored_by_speller();
        let was_immunized = tokens[real_idx].is_immunized();
        let base = tokens[real_idx].token().clone();
        let current_readings = tokens[real_idx].readings().to_vec();
        let mut filtered: Vec<AnalyzedToken> = Vec::with_capacity(current_readings.len());
        for r in &current_readings {
            let pos_match = match (&w.regex, &w.literal) {
                (Some(re), _) => r.pos_tags().iter().any(|t| re.is_match(t)),
                (None, Some(pos)) => r.pos_tags().iter().any(|t| t == pos),
                (None, None) => false,
            };
            let lemma_match = w
                .lemma
                .as_ref()
                .is_none_or(|lemma| r.lemma() == Some(lemma.as_str()));
            if !(pos_match && lemma_match) {
                filtered.push(r.clone());
            }
        }
        let had_sent_end = current_had_sent_end(&filtered, &tokens[real_idx]);
        if filtered.is_empty() {
            filtered.push(AnalyzedToken::new(base.token(), base.start(), base.end()));
        }
        let mut updated = AnalyzedTokenReadings::new(base).with_readings(filtered);
        restore_sent_end_if_lost(&mut updated, had_sent_end);
        updated.set_chunks(prior_chunks);
        if ignored_by_speller {
            updated.ignore_spelling();
        }
        if was_immunized {
            updated.immunize();
        }
        tokens[real_idx] = updated;
    }
}

fn apply_disambiguation(
    tokens: &mut [AnalyzedTokenReadings],
    token_idx: usize,
    action: &DisambigAction,
    matcher: &DisambigActionRegex,
) {
    let Some(real_idx) = non_ws_to_real(tokens, token_idx) else {
        return;
    };
    let prior_chunks = tokens[real_idx].chunks().to_vec();
    let ignored_by_speller = tokens[real_idx].is_ignored_by_speller();
    let was_immunized = tokens[real_idx].is_immunized();
    match action {
        DisambigAction::SetPos(pos) => {
            let base = tokens[real_idx].token().clone();
            let had_sent_end = current_had_sent_end(&[], &tokens[real_idx]);
            let mut surviving_lemma: Option<&str> = None;
            for r in tokens[real_idx].readings().iter() {
                if r.pos_tags().iter().any(|t| t == pos) && r.lemma().is_some() {
                    surviving_lemma = r.lemma();
                }
            }
            let lemma = surviving_lemma
                .filter(|l| !l.is_empty())
                .or_else(|| tokens[real_idx].readings().first().and_then(|r| r.lemma()));
            let mut new_reading = AnalyzedToken::new(base.token(), base.start(), base.end());
            new_reading.set_pos_tags(vec![pos.clone()]);
            new_reading.set_lemma(lemma.map(str::to_string));
            let mut updated = AnalyzedTokenReadings::new(base).with_readings(vec![new_reading]);
            restore_sent_end_if_lost(&mut updated, had_sent_end);
            updated.set_chunks(prior_chunks.clone());
            if ignored_by_speller {
                updated.ignore_spelling();
            }
            if was_immunized {
                updated.immunize();
            }
            tokens[real_idx] = updated;
        }
        DisambigAction::Replace(wds) => {
            let base = tokens[real_idx].token().clone();
            let new_readings: Vec<AnalyzedToken> = wds
                .iter()
                .map(|w| {
                    let token_text = if w.text.is_empty() {
                        base.token().to_string()
                    } else {
                        w.text.clone()
                    };
                    let mut reading = AnalyzedToken::new(&token_text, base.start(), base.end());
                    if let Some(pos) = &w.pos {
                        reading.set_pos_tags(vec![pos.clone()]);
                    }
                    reading.set_lemma(Some(w.lemma.clone().unwrap_or(token_text)));
                    reading
                })
                .collect();
            if !new_readings.is_empty() {
                let mut updated = AnalyzedTokenReadings::new(base).with_readings(new_readings);
                updated.set_chunks(prior_chunks.clone());
                if ignored_by_speller {
                    updated.ignore_spelling();
                }
                if was_immunized {
                    updated.immunize();
                }
                tokens[real_idx] = updated;
            }
        }
        DisambigAction::Remove(_) => {
            let DisambigActionRegex::Remove(matchers) = matcher else {
                return;
            };
            let base = tokens[real_idx].token().clone();
            let current_readings = tokens[real_idx].readings().to_vec();
            let filtered: Vec<AnalyzedToken> = current_readings
                .into_iter()
                .filter(|r| {
                    !matchers.iter().any(|w| {
                        let pos_match = match (&w.regex, &w.literal) {
                            (Some(re), _) => r.pos_tags().iter().any(|t| re.is_match(t)),
                            (None, Some(pos)) => r.pos_tags().iter().any(|t| t == pos),
                            (None, None) => false,
                        };
                        let lemma_match = w
                            .lemma
                            .as_ref()
                            .is_none_or(|lemma| r.lemma() == Some(lemma.as_str()));
                        pos_match && lemma_match
                    })
                })
                .collect();
            let mut filtered = filtered;
            let had_sent_end = current_had_sent_end(&filtered, &tokens[real_idx]);
            if filtered.is_empty() {
                filtered.push(AnalyzedToken::new(base.token(), base.start(), base.end()));
            }
            let mut updated = AnalyzedTokenReadings::new(base).with_readings(filtered);
            restore_sent_end_if_lost(&mut updated, had_sent_end);
            updated.set_chunks(prior_chunks.clone());
            if ignored_by_speller {
                updated.ignore_spelling();
            }
            if was_immunized {
                updated.immunize();
            }
            tokens[real_idx] = updated;
        }
        DisambigAction::RemovePostag(_) => {
            let DisambigActionRegex::RemovePostag(re, re_fancy) = matcher else {
                return;
            };
            if re.is_none() && re_fancy.is_none() {
                return;
            }
            let tag_matches = |t: &str| -> bool {
                re.as_ref().is_some_and(|re| re.is_match(t))
                    || re_fancy
                        .as_ref()
                        .is_some_and(|re| re.is_match(t).unwrap_or(false))
            };
            let base = tokens[real_idx].token().clone();
            let current_readings = tokens[real_idx].readings().to_vec();
            let mut filtered: Vec<AnalyzedToken> = current_readings
                .into_iter()
                .filter(|r| !r.pos_tags().iter().any(|t| tag_matches(t)))
                .collect();
            let had_sent_end = current_had_sent_end(&filtered, &tokens[real_idx]);
            if filtered.is_empty() {
                filtered.push(AnalyzedToken::new(base.token(), base.start(), base.end()));
            }
            let mut updated = AnalyzedTokenReadings::new(base).with_readings(filtered);
            restore_sent_end_if_lost(&mut updated, had_sent_end);
            updated.set_chunks(prior_chunks.clone());
            if ignored_by_speller {
                updated.ignore_spelling();
            }
            if was_immunized {
                updated.immunize();
            }
            tokens[real_idx] = updated;
        }
        DisambigAction::Add(wds) => {
            if wds.is_empty() {
                return;
            }
            let current_readings = tokens[real_idx].readings().to_vec();
            let base = tokens[real_idx].token().clone();
            let mut new_readings: Vec<AnalyzedToken> =
                Vec::with_capacity(current_readings.len() + wds.len());
            if let Some((last, head)) = current_readings.split_last() {
                new_readings.extend_from_slice(head);
                if !last.pos_tags().is_empty() {
                    new_readings.push(last.clone());
                }
            }
            for w in wds {
                let mut reading = AnalyzedToken::new(base.token(), base.start(), base.end());
                if let Some(pos) = &w.pos {
                    reading.set_pos_tags(vec![pos.clone()]);
                }
                if let Some(lemma) = &w.lemma {
                    reading.set_lemma(Some(lemma.clone()));
                } else {
                    reading.set_lemma(base.lemma().map(|l| l.to_string()));
                }
                new_readings.push(reading);
            }
            let mut updated = AnalyzedTokenReadings::new(base).with_readings(new_readings);
            updated.set_chunks(prior_chunks.clone());
            if ignored_by_speller {
                updated.ignore_spelling();
            }
            if was_immunized {
                updated.immunize();
            }
            tokens[real_idx] = updated;
        }
        DisambigAction::Filter { .. } => {
            let DisambigActionRegex::Filter(re, re_fancy) = matcher else {
                return;
            };
            if re.is_none() && re_fancy.is_none() {
                return;
            }
            let tag_matches = |t: &str| -> bool {
                re.as_ref().is_some_and(|re| re.is_match(t))
                    || re_fancy
                        .as_ref()
                        .is_some_and(|re| re.is_match(t).unwrap_or(false))
            };
            let base = tokens[real_idx].token().clone();
            let current_readings = tokens[real_idx].readings().to_vec();
            let filtered: Vec<AnalyzedToken> = current_readings
                .into_iter()
                .filter(|r| r.pos_tags().iter().any(|t| tag_matches(t)))
                .collect();
            let had_sent_end = current_had_sent_end(&filtered, &tokens[real_idx]);
            if !filtered.is_empty() {
                let mut updated = AnalyzedTokenReadings::new(base).with_readings(filtered);
                restore_sent_end_if_lost(&mut updated, had_sent_end);
                updated.set_chunks(prior_chunks.clone());
                if ignored_by_speller {
                    updated.ignore_spelling();
                }
                if was_immunized {
                    updated.immunize();
                }
                tokens[real_idx] = updated;
            }
        }
        DisambigAction::Match(spec) => {
            let DisambigActionRegex::Match {
                anchored,
                anchored_fancy,
                raw,
                raw_fancy,
            } = matcher
            else {
                return;
            };
            let atr = &tokens[real_idx];
            let base = atr.token().clone();
            let had_sent_end = current_had_sent_end(&[], atr);
            let probe_token = atr
                .readings()
                .first()
                .map(|r| r.token().to_string())
                .unwrap_or_else(|| base.token().to_string());
            let working: Option<Vec<AnalyzedToken>> = spec.lemma.as_deref().map(|lemma| {
                let survivors: Vec<AnalyzedToken> = atr
                    .readings()
                    .iter()
                    .filter(|r| {
                        let lemma_ok = r.lemma() == Some(lemma);
                        let pos_ok = match spec.postag.as_deref() {
                            Some(probe_pos) => r.pos_tags().first().is_some_and(|t| t == probe_pos),
                            None => true,
                        };
                        r.token() == probe_token && lemma_ok && pos_ok
                    })
                    .cloned()
                    .collect();
                if survivors.is_empty() {
                    vec![AnalyzedToken::new(base.token(), base.start(), base.end())]
                } else {
                    survivors
                }
            });
            if let Some(postag) = spec.postag.as_deref() {
                let source: &[AnalyzedToken] = working.as_deref().unwrap_or_else(|| atr.readings());
                let Some(first) = source.first() else {
                    return;
                };
                let token_text = first.token().to_string();
                let whole_match = |tag: &str| -> bool {
                    anchored.as_ref().is_some_and(|re| re.is_match(tag))
                        || anchored_fancy
                            .as_ref()
                            .is_some_and(|re| re.is_match(tag).unwrap_or(false))
                };
                let rewrite = |tag: &str| -> String {
                    match spec.postag_replace.as_deref() {
                        Some(rep) => {
                            if let Some(re) = raw.as_ref() {
                                re.replace_all(tag, rep).into_owned()
                            } else if let Some(re) = raw_fancy.as_ref() {
                                re.replace_all(tag, rep).into_owned()
                            } else {
                                tag.to_string()
                            }
                        }
                        None => tag.to_string(),
                    }
                };
                let mut kept: Vec<AnalyzedToken> = Vec::new();
                if spec.postag_regexp {
                    for r in source {
                        let Some(tag) = r.pos_tags().first() else {
                            continue;
                        };
                        if whole_match(tag) {
                            let mut at = AnalyzedToken::new(&token_text, base.start(), base.end());
                            at.set_pos_tags(vec![rewrite(tag)]);
                            at.set_lemma(r.lemma().map(str::to_string));
                            kept.push(at);
                        }
                    }
                    if kept.is_empty() {
                        kept = match_new_token(source, &token_text, postag, &base);
                    }
                } else {
                    kept = match_new_token(source, &token_text, postag, &base);
                }
                if !kept.is_empty() {
                    let mut updated = AnalyzedTokenReadings::new(base).with_readings(kept);
                    restore_sent_end_if_lost(&mut updated, had_sent_end);
                    updated.set_chunks(prior_chunks.clone());
                    if ignored_by_speller {
                        updated.ignore_spelling();
                    }
                    if was_immunized {
                        updated.immunize();
                    }
                    tokens[real_idx] = updated;
                }
            } else if let Some(working) = working {
                let mut updated = AnalyzedTokenReadings::new(base).with_readings(working);
                restore_sent_end_if_lost(&mut updated, had_sent_end);
                updated.set_chunks(prior_chunks.clone());
                if ignored_by_speller {
                    updated.ignore_spelling();
                }
                if was_immunized {
                    updated.immunize();
                }
                tokens[real_idx] = updated;
            }
        }
        DisambigAction::FilterAll => {
            let base = tokens[real_idx].token().clone();
            let empty_reading = AnalyzedToken::new(base.token(), base.start(), base.end());
            let mut updated = AnalyzedTokenReadings::new(base).with_readings(vec![empty_reading]);
            updated.set_chunks(prior_chunks.clone());
            if ignored_by_speller {
                updated.ignore_spelling();
            }
            if was_immunized {
                updated.immunize();
            }
            tokens[real_idx] = updated;
        }
        DisambigAction::IgnoreSpelling => {}
        DisambigAction::Unify => {}
        DisambigAction::Immunize => {}
    }
}

fn compile_disambig_rule(compiler: &XmlCompiler, rule: DisambigRule) -> CompiledDisambigRule {
    CompiledDisambigRule {
        id: rule.id,
        name: rule.name,
        filter_class: rule.filter_class,
        filter_args: rule.filter_args,
        untouched_examples: rule.untouched_examples,
        examples: rule.examples,
        pattern: CompiledPattern {
            marker_elem_start: rule.pattern.marker_elem_start,
            marker_elem_end: rule.pattern.marker_elem_end,
            unify_groups: Vec::new(),
            tokens: rule
                .pattern
                .tokens
                .iter()
                .map(|t| compiler.compile_token_with_default(t, rule.pattern.case_sensitive))
                .collect(),
            elements: rule
                .pattern
                .elements
                .iter()
                .map(|e| compile_disambig_element(compiler, e, rule.pattern.case_sensitive))
                .collect(),
            case_sensitive: rule.pattern.case_sensitive,
            marker_start: rule.pattern.marker_start,
            marker_end: rule.pattern.marker_end,
            has_structured_elements: rule.pattern.elements.iter().any(|e| {
                matches!(
                    e,
                    og_xml::types::XmlPatternElement::OrGroup(_)
                        | og_xml::types::XmlPatternElement::AndGroup(_)
                )
            }),
            raw_pos: rule.pattern.raw_pos,
        },
        antipatterns: rule
            .antipatterns
            .iter()
            .map(|ap| CompiledPattern {
                marker_elem_start: ap.marker_elem_start,
                marker_elem_end: ap.marker_elem_end,
                unify_groups: Vec::new(),
                tokens: ap
                    .tokens
                    .iter()
                    .map(|t| compiler.compile_token_with_default(t, ap.case_sensitive))
                    .collect(),
                elements: ap
                    .elements
                    .iter()
                    .map(|e| compile_disambig_element(compiler, e, ap.case_sensitive))
                    .collect(),
                case_sensitive: ap.case_sensitive,
                marker_start: None,
                marker_end: None,
                has_structured_elements: ap.elements.iter().any(|e| {
                    matches!(
                        e,
                        og_xml::types::XmlPatternElement::OrGroup(_)
                            | og_xml::types::XmlPatternElement::AndGroup(_)
                    )
                }),
                raw_pos: ap.raw_pos,
            })
            .collect(),
        disambig: rule.disambig,
        unify: rule.unify,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::analyzed::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    fn make_sentence(tokens: Vec<(&str, Vec<&str>)>) -> AnalyzedSentence {
        let mut offset = 0;
        let mut analyzed = Vec::new();
        for (text, pos_tags) in tokens {
            let start = offset;
            let end = offset + text.len();
            let base = AnalyzedToken::new(text, start, end);
            let readings: Vec<AnalyzedToken> = pos_tags
                .into_iter()
                .map(|pos| {
                    AnalyzedToken::new(text, start, end).with_pos_tags(vec![pos.to_string()])
                })
                .collect();
            analyzed.push(AnalyzedTokenReadings::new(base).with_readings(readings));
            offset = end + 1;
        }
        let total_len = offset.saturating_sub(1);
        let mut sentence = AnalyzedSentence::new("test", 0, total_len);
        sentence.set_tokens(analyzed);
        sentence
    }

    fn make_sentence_with_lemmas(tokens: Vec<(&str, &str, Vec<&str>)>) -> AnalyzedSentence {
        let mut offset = 0;
        let mut analyzed = Vec::new();
        for (text, lemma, pos_tags) in tokens {
            let start = offset;
            let end = offset + text.len();
            let base = AnalyzedToken::new(text, start, end);
            let readings: Vec<AnalyzedToken> = pos_tags
                .into_iter()
                .map(|pos| {
                    AnalyzedToken::new(text, start, end)
                        .with_pos_tags(vec![pos.to_string()])
                        .with_lemma(lemma)
                })
                .collect();
            analyzed.push(AnalyzedTokenReadings::new(base).with_readings(readings));
            offset = end + 1;
        }
        let total_len = offset.saturating_sub(1);
        let mut sentence = AnalyzedSentence::new("test", 0, total_len);
        sentence.set_tokens(analyzed);
        sentence
    }

    #[test]
    fn unknown_postag_matches_sent_end_reading() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="SENT_END_UNSET" name="clear tagless final token">
        <pattern>
            <marker><token postag="UNKNOWN"/></marker>
        </pattern>
        <disambig postag="NN"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![("word", vec!["NN"])]);
        sentence.tokens_mut().last_mut().unwrap().add_sent_end();
        disambig.disambiguate(&mut sentence);
        let last = sentence.tokens().last().unwrap();
        let tags: Vec<String> = last
            .readings()
            .iter()
            .map(|r| r.pos_tags().join("|"))
            .collect();
        assert_eq!(
            tags,
            vec!["NN".to_string(), "SENT_END".to_string()],
            "UNKNOWN must match the SENT_END-tagged reading and the set-pos replace must keep the sentence end"
        );
    }

    #[test]
    fn set_pos_uses_last_matching_lemma_and_first_reading_fallback() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="BEST_JJS" name="best as adjective">
        <pattern>
            <marker><token>best</token></marker>
            <token postag="IN"/>
        </pattern>
        <disambig postag="JJS"/>
    </rule>
    <rule id="DT_VBN_JJ" name="DT + VBN + JJ">
        <pattern>
            <marker><token>known</token></marker>
            <token postag="IN"/>
        </pattern>
        <disambig postag="JJ"/>
    </rule>
</rules>"#;
        let readings_for = |word: &str, start: usize, specs: &[(&str, &str)]| {
            specs
                .iter()
                .map(|&(lemma, pos)| {
                    AnalyzedToken::new(word, start, start + word.len())
                        .with_pos_tags(vec![pos.to_string()])
                        .with_lemma(lemma)
                })
                .collect::<Vec<AnalyzedToken>>()
        };
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut both = AnalyzedSentence::new("test", 0, 8);
        both.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("best", 0, 4)).with_readings(
                readings_for(
                    "best",
                    0,
                    &[("best", "NN:U"), ("good", "JJS"), ("well", "JJS")],
                ),
            ),
            AnalyzedTokenReadings::new(AnalyzedToken::new("for", 5, 8))
                .with_readings(readings_for("for", 5, &[("for", "IN")])),
        ]);
        disambig.disambiguate(&mut both);
        let best = &both.tokens()[0];
        assert_eq!(
            (
                best.readings()[0].lemma(),
                best.readings()[0].pos_tags().join("|")
            ),
            (Some("well"), "JJS".to_string()),
            "the single-pos replace keeps the LAST matching reading's lemma"
        );

        let mut none = AnalyzedSentence::new("test", 0, 9);
        none.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("known", 0, 5)).with_readings(
                readings_for("known", 0, &[("know", "VBN"), ("known", "NN")]),
            ),
            AnalyzedTokenReadings::new(AnalyzedToken::new("for", 6, 9))
                .with_readings(readings_for("for", 6, &[("for", "IN")])),
        ]);
        disambig.disambiguate(&mut none);
        let known = &none.tokens()[0];
        assert_eq!(
            (
                known.readings()[0].lemma(),
                known.readings()[0].pos_tags().join("|")
            ),
            (Some("know"), "JJ".to_string()),
            "with no matching reading the first reading's lemma is used"
        );
    }

    #[test]
    fn inflected_first_element_is_not_gated_on_surface_text() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="GO_APE" name="go ape => NN:UN">
      <pattern>
      <token inflected="yes">go</token>
      <marker><token>ape</token></marker>
      </pattern>
      <disambig postag="NN:UN"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence_with_lemmas(vec![
            ("goes", "go", vec!["NNS", "VBZ"]),
            ("ape", "ape", vec!["NN", "VB", "VBP"]),
        ]);
        disambig.disambiguate(&mut sentence);
        let ape = &sentence.tokens()[1];
        assert_eq!(
            ape.readings()
                .iter()
                .map(|r| r.pos_tags().join("|"))
                .collect::<Vec<_>>(),
            vec!["NN:UN".to_string()],
            "a rule led by an inflected element must fire on inflected surface forms"
        );
    }

    #[test]
    fn negate_pos_exception_fires_on_any_non_matching_reading() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">
    <rule id="PRP_NOM" name="Remove NOM after pure preposition">
        <pattern>
            <token postag_regexp="yes" postag="PRP.+">
                <exception postag="PRP.+" postag_regexp="yes" negate_pos="yes"/>
            </token>
            <marker>
                <and>
                    <token postag_regexp="yes" postag=".+NOM:.+"/>
                    <token postag_regexp="yes" postag=".+(AKK|DAT|GEN):.+"/>
                </and>
            </marker>
        </pattern>
        <disambig action="remove" postag=".+:NOM:.+"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut mixed = make_sentence(vec![
            ("mit", vec!["PRP:TMP+MOD+CAU:DAT", "ZUS"]),
            ("Wasser", vec!["SUB:NOM:SIN:NEU", "SUB:AKK:SIN:NEU"]),
        ]);
        disambig.disambiguate(&mut mixed);
        assert!(
            mixed.tokens()[1].has_pos_tag("SUB:NOM:SIN:NEU"),
            "a preposition with a non-PRP reading must be excluded by the negate_pos exception"
        );
        let mut pure = make_sentence(vec![
            ("für", vec!["PRP:AKK"]),
            ("Wasser", vec!["SUB:NOM:SIN:NEU", "SUB:AKK:SIN:NEU"]),
        ]);
        disambig.disambiguate(&mut pure);
        assert!(
            !pure.tokens()[1].has_pos_tag("SUB:NOM:SIN:NEU"),
            "a pure preposition still triggers the NOM removal"
        );
    }

    #[test]
    fn and_group_leading_pattern_gates_on_its_first_member() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="VBZ_BEFORE_DT_NN" name="verb/noun + det + noun => verb">
        <pattern>
            <marker>
                <and>
                    <token postag="NNS"/>
                    <token postag="VBZ" />
                </and>
            </marker>
            <token postag="DT"><exception>that</exception></token>
            <token postag="NN(:UN?)?" postag_regexp="yes"><exception postag_regexp="yes" postag="VB.*"/></token>
        </pattern>
        <disambig postag="VBZ"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("father", vec!["NN"]),
            ("sings", vec!["NNS", "VBZ"]),
            ("a", vec!["DT"]),
            ("song", vec!["NN:UN"]),
        ]);
        disambig.disambiguate(&mut sentence);
        let tok = sentence
            .tokens()
            .iter()
            .find(|t| t.token().token() == "sings")
            .unwrap();
        assert_eq!(
            tok.readings()
                .iter()
                .map(|r| r.pos_tags().join("|"))
                .collect::<Vec<_>>(),
            vec!["VBZ"],
            "a pattern whose first element is an and-group must gate starts on the group's first member, not the following element"
        );
    }

    #[test]
    fn or_group_leading_pattern_gates_on_its_literal_set() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="PET_ACTION" name="pet actions">
        <pattern>
            <or>
                <token>cat</token>
                <token>dog</token>
            </or>
            <marker><token postag="VBZ"/></marker>
        </pattern>
        <disambig postag="ANIMAL"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        assert_eq!(disambig.rule_ids(), vec!["PET_ACTION"]);
        let readings_of = |sentence: &AnalyzedSentence, word: &str| {
            sentence
                .tokens()
                .iter()
                .find(|t| t.token().token() == word)
                .map(|t| {
                    t.readings()
                        .iter()
                        .map(|r| r.pos_tags().join("|"))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        for subject in ["cat", "dog"] {
            let mut sentence =
                make_sentence(vec![(subject, vec!["NN"]), ("sleeps", vec!["VBZ", "NNS"])]);
            disambig.disambiguate(&mut sentence);
            assert_eq!(
                readings_of(&sentence, "sleeps"),
                vec!["ANIMAL"],
                "'{subject}' must pass the or-literal first-token gate so the marker token is replaced"
            );
            assert_eq!(
                readings_of(&sentence, subject),
                vec!["NN"],
                "the token outside the marker keeps its readings"
            );
        }
        let mut other = make_sentence(vec![("bird", vec!["NN"]), ("sleeps", vec!["VBZ", "NNS"])]);
        disambig.disambiguate(&mut other);
        assert_eq!(
            readings_of(&other, "sleeps"),
            vec!["VBZ", "NNS"],
            "a subject outside the or-group must be gated out before any matching"
        );
    }

    #[test]
    fn case_sensitive_antipattern_and_multi_wd_replace() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="STATUS_QUO" name="status quo">
        <antipattern case_sensitive="yes">
            <token>Status</token>
            <token>Quo</token>
        </antipattern>
        <pattern>
            <token>status</token>
            <token>quo</token>
        </pattern>
        <disambig action="replace"><wd pos="NN"/><wd pos="NN"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut lower = make_sentence(vec![
            ("the", vec!["DT"]),
            ("status", vec!["NN", "NN:UN"]),
            ("quo", vec![]),
        ]);
        disambig.disambiguate(&mut lower);
        let tags = |s: &AnalyzedSentence, word: &str| {
            s.tokens()
                .iter()
                .find(|t| t.token().token() == word)
                .map(|t| {
                    t.readings()
                        .iter()
                        .map(|r| r.pos_tags().join("|"))
                        .collect::<Vec<_>>()
                })
        };
        assert_eq!(
            tags(&lower, "status"),
            Some(vec!["NN".to_string()]),
            "wd[0] replaces only the first matched token"
        );
        assert_eq!(
            tags(&lower, "quo"),
            Some(vec!["NN".to_string()]),
            "wd[1] replaces only the second matched token, not both wds on one token"
        );
        let mut upper = make_sentence(vec![
            ("the", vec!["DT"]),
            ("Status", vec!["NN", "NN:UN"]),
            ("Quo", vec![]),
        ]);
        disambig.disambiguate(&mut upper);
        assert_eq!(
            tags(&upper, "Status"),
            Some(vec!["NN".to_string(), "NN:UN".to_string()]),
            "a case_sensitive antipattern must block the mixed-case match"
        );
    }

    #[test]
    fn empty_regexp_token_has_no_text_constraint() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="EMPTY_REGEXP_WILDCARD">
        <antipattern>
            <token regexp="yes" postag="VB" />
        </antipattern>
        <pattern>
            <marker>
                <token regexp="yes" postag="NN.*" postag_regexp="yes" />
            </marker>
        </pattern>
        <disambig postag="VB"/>
    </rule>
</rules>"#;
        let disambiguator = XmlDisambiguator::from_xml(xml).unwrap();
        let mut blocked = make_sentence(vec![
            ("<S>", vec!["SENT_START"]),
            ("word", vec!["NN", "VB"]),
            (".", vec!["SENT_END"]),
        ]);
        disambiguator.disambiguate(&mut blocked);
        assert!(
            blocked.tokens()[1].readings().len() == 2,
            "the marker element matches the token via postag alone, and the same empty-regexp shape with postag=VB in the antipattern overlaps and vetoes the rule (Java sets TEST_STRING_MASK only for non-empty patterns)"
        );
        let mut free = make_sentence(vec![
            ("<S>", vec!["SENT_START"]),
            ("word", vec!["NN"]),
            (".", vec!["SENT_END"]),
        ]);
        disambiguator.disambiguate(&mut free);
        assert!(
            free.tokens()[1].readings().len() == 1 && free.tokens()[1].has_pos_tag("VB"),
            "without a VB reading the antipattern does not overlap, and the marker element still matches on postag alone so the replace runs"
        );
    }

    #[test]
    fn max_occurrence_element_matches_multiple_tokens() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="VBN_VBD_LIKE" name="two middle tokens">
        <pattern>
            <token>has</token>
            <token min="0" max="2" postag="PRP|NN.*|RB" postag_regexp="yes" />
            <marker>
                <and>
                    <token postag="VBD"/>
                    <token postag="VBN"/>
                </and>
            </marker>
        </pattern>
        <disambig postag="VBN"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("She", vec!["PRP"]),
            ("has", vec!["VBZ"]),
            ("not", vec!["RB"]),
            ("bee", vec!["NN"]),
            ("injured", vec!["VBD", "VBN", "JJ"]),
        ]);
        disambig.disambiguate(&mut sentence);
        let tok = sentence
            .tokens()
            .iter()
            .find(|t| t.token().token() == "injured")
            .unwrap();
        assert_eq!(
            tok.readings()
                .iter()
                .map(|r| r.pos_tags().join("|"))
                .collect::<Vec<_>>(),
            vec!["VBN"],
            "an element with max=2 must match two consecutive tokens so the and-group on the next token applies"
        );

        let mut one = make_sentence(vec![
            ("She", vec!["PRP"]),
            ("has", vec!["VBZ"]),
            ("not", vec!["RB"]),
            ("injured", vec!["VBD", "VBN", "JJ"]),
        ]);
        disambig.disambiguate(&mut one);
        let tok = one
            .tokens()
            .iter()
            .find(|t| t.token().token() == "injured")
            .unwrap();
        assert_eq!(
            tok.readings()
                .iter()
                .map(|r| r.pos_tags().join("|"))
                .collect::<Vec<_>>(),
            vec!["VBN"]
        );

        let mut zero = make_sentence(vec![
            ("She", vec!["PRP"]),
            ("has", vec!["VBZ"]),
            ("injured", vec!["VBD", "VBN", "JJ"]),
        ]);
        disambig.disambiguate(&mut zero);
        let tok = zero
            .tokens()
            .iter()
            .find(|t| t.token().token() == "injured")
            .unwrap();
        assert_eq!(
            tok.readings()
                .iter()
                .map(|r| r.pos_tags().join("|"))
                .collect::<Vec<_>>(),
            vec!["VBN"]
        );
    }

    #[test]
    fn test_set_pos_disambiguation() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="NUM" name="Tag numbers">
        <pattern>
            <token regexp="yes">\d+</token>
        </pattern>
        <disambig postag="CD"/>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![("10", vec!["CD", "NN"])]);
        disambig.disambiguate(&mut sentence);

        let token = &sentence.tokens()[0];
        assert!(token.has_pos_tag("CD"));
        assert!(!token.has_pos_tag("NN"));
    }

    #[test]
    fn test_remove_disambiguation() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="OR_JJ" name="Remove JJ from or">
        <pattern case_sensitive="yes">
            <token>or</token>
        </pattern>
        <disambig action="remove"><wd pos="JJ"/></disambig>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![("or", vec!["CC", "JJ", "NN:U"])]);
        disambig.disambiguate(&mut sentence);

        let token = &sentence.tokens()[0];
        assert!(token.has_pos_tag("CC"));
        assert!(!token.has_pos_tag("JJ"));
        assert!(token.has_pos_tag("NN:U"));
    }

    #[test]
    fn test_replace_disambiguation() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="CA" name="Replace ca">
        <pattern>
            <marker>
                <token>ca</token>
            </marker>
            <token spacebefore="no">n't</token>
        </pattern>
        <disambig action="replace"><wd lemma="can" pos="MD"/></disambig>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();

        let mut sentence = AnalyzedSentence::new("ca n't", 0, 5);
        let mut tokens = Vec::new();
        let t1 = AnalyzedToken::new("ca", 0, 2).with_pos_tags(vec!["NN".to_string()]);
        tokens.push(AnalyzedTokenReadings::new(t1));
        let t2 = AnalyzedToken::new("n't", 2, 5).with_pos_tags(vec!["RB".to_string()]);
        tokens.push(AnalyzedTokenReadings::new(t2));
        sentence.set_tokens(tokens);

        disambig.disambiguate(&mut sentence);

        let first = &sentence.tokens()[0];
        assert!(first.has_pos_tag("MD"));
        assert!(!first.has_pos_tag("NN"));
    }

    #[test]
    fn test_ignore_spelling_marks_full_match_range() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="CRON" name="ignore cron job">
        <pattern>
            <token>cron</token>
            <token regexp="yes">jobs?</token>
        </pattern>
        <disambig action="ignore_spelling"/>
    </rule>
    <rule id="CA" name="ignore ca in ca n't">
        <pattern>
            <marker>
                <token>ca</token>
            </marker>
            <token spacebefore="no">n't</token>
        </pattern>
        <disambig action="ignore_spelling"/>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();

        let mut sentence = AnalyzedSentence::new("cron job", 0, 8);
        let tokens = vec![
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("cron", 0, 4).with_pos_tags(vec!["NN".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("job", 5, 8).with_pos_tags(vec!["NN".to_string()]),
            ),
        ];
        sentence.set_tokens(tokens);
        disambig.disambiguate(&mut sentence);
        assert!(sentence.tokens()[0].is_ignored_by_speller());
        assert!(sentence.tokens()[1].is_ignored_by_speller());
        assert!(sentence.tokens()[0].has_pos_tag("NN"));

        let mut sentence = AnalyzedSentence::new("ca n't", 0, 5);
        let tokens = vec![
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("ca", 0, 2).with_pos_tags(vec!["NN".to_string()]),
            ),
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("n't", 2, 5).with_pos_tags(vec!["RB".to_string()]),
            ),
        ];
        sentence.set_tokens(tokens);
        disambig.disambiguate(&mut sentence);
        assert!(sentence.tokens()[0].is_ignored_by_speller());
        assert!(!sentence.tokens()[1].is_ignored_by_speller());
    }

    #[test]
    fn test_add_disambiguation() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="PUNCT" name="Add PCT">
        <pattern>
            <token regexp="yes">[\.,;:…!\?]</token>
        </pattern>
        <disambig action="add"><wd pos="PCT"/></disambig>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![(".", vec!["."])]);
        disambig.disambiguate(&mut sentence);

        let token = &sentence.tokens()[0];
        assert!(token.has_pos_tag("."));
        assert!(token.has_pos_tag("PCT"));
    }

    fn readings_of(token: &AnalyzedTokenReadings) -> Vec<(Option<&str>, String)> {
        token
            .readings()
            .iter()
            .map(|r| (r.lemma(), r.pos_tags().join("|")))
            .collect()
    }

    fn le_sentence() -> AnalyzedSentence {
        make_sentence_with_lemmas(vec![
            ("le", "le", vec!["D f s", "R pers obj 3 s"]),
            ("chat", "chat", vec!["NC s m"]),
        ])
    }

    #[test]
    fn match_action_filters_readings_by_a_regexp_postag() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_OBJ" name="le as object pronoun">
        <pattern>
            <marker><token>le</token></marker>
        </pattern>
        <disambig action="replace"><match postag="R.*" postag_regexp="yes"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = le_sentence();
        disambig.disambiguate(&mut sentence);
        assert_eq!(
            readings_of(&sentence.tokens()[0]),
            vec![(Some("le"), "R pers obj 3 s".to_string())],
            "only the pronoun reading survives the regexp match filter"
        );
        assert_eq!(
            readings_of(&sentence.tokens()[1]).len(),
            1,
            "unmatched tokens are untouched"
        );
    }

    #[test]
    fn match_action_rewrites_tags_with_postag_replace() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_REWRITE" name="rewrite the pronoun tag">
        <pattern>
            <marker><token>le</token></marker>
        </pattern>
        <disambig action="replace"><match postag="R (.+)" postag_regexp="yes" postag_replace="PRON $1"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = le_sentence();
        disambig.disambiguate(&mut sentence);
        assert_eq!(
            readings_of(&sentence.tokens()[0]),
            vec![(Some("le"), "PRON pers obj 3 s".to_string())],
            "the matched tag is rewritten with the replacement"
        );
    }

    #[test]
    fn match_action_with_a_literal_postag_builds_a_new_reading() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_LITERAL" name="literal postag match">
        <pattern>
            <marker><token>le</token></marker>
        </pattern>
        <disambig action="replace"><match postag="PRON"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = le_sentence();
        disambig.disambiguate(&mut sentence);
        assert_eq!(
            readings_of(&sentence.tokens()[0]),
            vec![
                (Some("le"), "PRON".to_string()),
                (Some("le"), "PRON".to_string())
            ],
            "a literal postag assigns the tag to every non-empty reading with the first lemma"
        );

        let mut readings = vec![
            AnalyzedToken::new("le", 0, 2)
                .with_pos_tags(vec!["D f s".to_string()])
                .with_lemma("der"),
            AnalyzedToken::new("le", 0, 2)
                .with_pos_tags(vec!["PRON".to_string()])
                .with_lemma("leobj"),
            AnalyzedToken::new("le", 0, 2),
        ];
        let mut custom = AnalyzedSentence::new("test", 0, 5);
        custom.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("le", 0, 2)).with_readings(readings),
            AnalyzedTokenReadings::new(AnalyzedToken::new("x", 3, 4)).with_readings(vec![
                AnalyzedToken::new("x", 3, 4).with_pos_tags(vec!["NC".to_string()]),
            ]),
        ]);
        disambig.disambiguate(&mut custom);
        assert_eq!(
            readings_of(&custom.tokens()[0]),
            vec![
                (Some("leobj"), "PRON".to_string()),
                (Some("leobj"), "PRON".to_string())
            ],
            "the reading matching the literal postag donates its lemma and tagless readings are skipped"
        );
    }

    #[test]
    fn match_action_with_an_unmatched_regexp_falls_back_to_a_new_reading() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_MISS" name="regexp match miss">
        <pattern>
            <marker><token>le</token></marker>
        </pattern>
        <disambig action="replace"><match postag="Z.*" postag_regexp="yes"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = le_sentence();
        disambig.disambiguate(&mut sentence);
        assert_eq!(
            readings_of(&sentence.tokens()[0]),
            vec![
                (Some("le"), "Z.*".to_string()),
                (Some("le"), "Z.*".to_string())
            ],
            "an unmatched regexp falls back to the literal postag text"
        );
    }

    #[test]
    fn match_action_with_lemma_text_keeps_only_that_lemma() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_LEMMA" name="lemma-selected match">
        <pattern>
            <marker><token>le</token></marker>
        </pattern>
        <disambig action="replace"><match>le</match></disambig>
    </rule>
    <rule id="LE_LEMMA_MISS" name="lemma-selected match miss">
        <pattern>
            <marker><token>chat</token></marker>
        </pattern>
        <disambig action="replace"><match>rien</match></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence_with_lemmas(vec![
            ("le", "le", vec!["D f s", "R pers obj 3 s"]),
            ("chat", "chat", vec!["NC s m"]),
        ]);
        disambig.disambiguate(&mut sentence);
        assert_eq!(
            readings_of(&sentence.tokens()[1]),
            vec![(None, String::new())],
            "a lemma with no surviving reading collapses to a plain reading"
        );
    }

    #[test]
    fn match_action_preserves_speller_and_immunize_flags() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_FLAGS" name="flags preserved">
        <pattern>
            <marker><token>le</token></marker>
        </pattern>
        <disambig action="replace"><match postag="R.*" postag_regexp="yes"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = le_sentence();
        sentence.tokens_mut()[0].ignore_spelling();
        disambig.disambiguate(&mut sentence);
        let token = &sentence.tokens()[0];
        assert!(token.is_ignored_by_speller());
        assert_eq!(
            readings_of(token),
            vec![(Some("le"), "R pers obj 3 s".to_string())]
        );
    }

    #[test]
    fn add_wd_action_applies_per_token_across_the_marker() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="ADD_WDS" name="add one reading per token">
        <pattern>
            <marker>
                <token>good</token>
                <token>morning</token>
            </marker>
        </pattern>
        <disambig action="add"><wd pos="X1"/><wd pos="X2">morgen</wd></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![("good", vec!["JJ"]), ("morning", vec!["NN"])]);
        sentence.tokens_mut()[1].ignore_spelling();
        disambig.disambiguate(&mut sentence);
        assert!(sentence.tokens()[0].has_pos_tag("JJ"));
        assert!(sentence.tokens()[0].has_pos_tag("X1"));
        assert!(!sentence.tokens()[0].has_pos_tag("X2"));
        assert!(sentence.tokens()[1].has_pos_tag("NN"));
        assert!(sentence.tokens()[1].has_pos_tag("X2"));
        assert!(!sentence.tokens()[1].has_pos_tag("X1"));
        assert!(sentence.tokens()[1].is_ignored_by_speller());
        assert_eq!(
            sentence.tokens()[1].readings().last().unwrap().token(),
            "morgen",
            "a wd with text content adds a reading carrying that text"
        );

        disambig.disambiguate(&mut sentence);
        let count = sentence.tokens()[1]
            .readings()
            .iter()
            .filter(|r| r.pos_tags().first().is_some_and(|t| t == "X2"))
            .count();
        assert_eq!(
            count, 1,
            "adding the identical reading a second time is a no-op"
        );
    }

    #[test]
    fn remove_wd_action_keeps_the_flags_and_the_last_reading() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="RM_WD" name="remove one reading">
        <pattern>
            <marker><token>run</token></marker>
        </pattern>
        <disambig action="remove"><wd pos="NN" lemma="run"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence_with_lemmas(vec![
            ("run", "run", vec!["VB", "NN"]),
            (".", ".", vec!["."]),
        ]);
        sentence.tokens_mut()[0].ignore_spelling();
        disambig.disambiguate(&mut sentence);
        let token = &sentence.tokens()[0];
        assert_eq!(readings_of(token), vec![(Some("run"), "VB".to_string())]);
        assert!(token.is_ignored_by_speller());
    }

    #[test]
    fn set_pos_action_preserves_speller_and_immunize_flags() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="SETPOS_FLAGS" name="set-pos flags">
        <pattern>
            <marker><token>run</token></marker>
        </pattern>
        <disambig postag="VB"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence_with_lemmas(vec![("run", "run", vec!["VB", "NN"])]);
        sentence.tokens_mut()[0].ignore_spelling();
        disambig.disambiguate(&mut sentence);
        let token = &sentence.tokens()[0];
        assert!(token.is_ignored_by_speller());
        assert_eq!(readings_of(token), vec![(Some("run"), "VB".to_string())]);
    }

    #[test]
    fn test_filter_disambiguation() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="VFILT" name="Filter verb">
        <pattern>
            <marker>
                <token postag="V.*" postag_regexp="yes"/>
            </marker>
            <token spacebefore="no">n't</token>
        </pattern>
        <disambig action="filter" postag="V.*"/>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();

        let mut sentence = AnalyzedSentence::new("don't", 0, 5);
        let mut tokens = Vec::new();
        let base1 = AnalyzedToken::new("do", 0, 2);
        let readings1 = vec![
            AnalyzedToken::new("do", 0, 2).with_pos_tags(vec!["VBP".to_string()]),
            AnalyzedToken::new("do", 0, 2).with_pos_tags(vec!["NN".to_string()]),
        ];
        tokens.push(AnalyzedTokenReadings::new(base1).with_readings(readings1));
        let base2 = AnalyzedToken::new("n't", 2, 5);
        let readings2 = vec![AnalyzedToken::new("n't", 2, 5).with_pos_tags(vec!["RB".to_string()])];
        tokens.push(AnalyzedTokenReadings::new(base2).with_readings(readings2));
        sentence.set_tokens(tokens);

        disambig.disambiguate(&mut sentence);

        let first = &sentence.tokens()[0];
        assert!(first.has_pos_tag("VBP"));
        assert!(!first.has_pos_tag("NN"));
    }

    #[test]
    fn test_no_match_no_change() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="NUM" name="Tag numbers">
        <pattern>
            <token regexp="yes">\d+</token>
        </pattern>
        <disambig postag="CD"/>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![("hello", vec!["UH", "NN"])]);
        disambig.disambiguate(&mut sentence);

        let token = &sentence.tokens()[0];
        assert!(token.has_pos_tag("UH"));
        assert!(token.has_pos_tag("NN"));
    }

    #[test]
    fn test_parse_real_disambiguation_header() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE rules [
    <!ENTITY months "January|February|March|April|May|Ju(ne|ly)|August|September|October|November|December">
]>
<rules lang="en">
    <rule id="MONTHS" name="Months">
        <pattern>
            <token regexp="yes">&months;</token>
        </pattern>
        <disambig postag="NNP"/>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        assert_eq!(disambig.rule_count(), 1);

        let mut sentence = make_sentence(vec![("January", vec!["NNP", "NN"])]);
        disambig.disambiguate(&mut sentence);
        let token = &sentence.tokens()[0];
        assert!(token.has_pos_tag("NNP"));
        assert!(!token.has_pos_tag("NN"));
    }

    #[test]
    fn test_rule_count() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="R1" name="Rule 1">
        <pattern><token>a</token></pattern>
        <disambig postag="DT"/>
    </rule>
    <rule id="R2" name="Rule 2">
        <pattern><token>b</token></pattern>
        <disambig postag="NN"/>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        assert_eq!(disambig.rule_count(), 2);
    }

    #[test]
    fn test_and_group_disambiguation() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="AND" name="And group">
        <pattern>
            <and>
                <token inflected="yes">install</token>
                <token inflected="yes">instal</token>
            </and>
        </pattern>
        <disambig action="remove"><wd lemma="instal"/></disambig>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        assert_eq!(disambig.rule_count(), 1);
    }

    #[test]
    fn test_antipattern_blocks_disambiguation() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="ANTI" name="With antipattern">
        <antipattern>
            <token>nope</token>
        </antipattern>
        <pattern>
            <token>test</token>
        </pattern>
        <disambig postag="NN"/>
    </rule>
    <rule id="ANTI2" name="With overlapping antipattern">
        <antipattern>
            <token regexp="yes">pro.+</token>
        </antipattern>
        <pattern>
            <marker>
                <token>probe</token>
            </marker>
            <token>nope</token>
        </pattern>
        <disambig action="remove"><wd pos="VB"/></disambig>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();

        let mut sentence1 = make_sentence(vec![("test", vec!["NN", "VB"])]);
        disambig.disambiguate(&mut sentence1);
        assert!(sentence1.tokens()[0].has_pos_tag("NN"));
        assert!(!sentence1.tokens()[0].has_pos_tag("VB"));

        let mut sentence2 = make_sentence(vec![("nope", vec!["RB"]), ("test", vec!["NN", "VB"])]);
        disambig.disambiguate(&mut sentence2);
        assert!(sentence2.tokens()[1].has_pos_tag("NN"));
        assert!(!sentence2.tokens()[1].has_pos_tag("VB"));

        let mut sentence3 = make_sentence(vec![("probe", vec!["NN", "VB"]), ("nope", vec!["RB"])]);
        disambig.disambiguate(&mut sentence3);
        assert!(sentence3.tokens()[0].has_pos_tag("VB"));
    }

    #[test]
    fn test_load_real_english_disambiguation() {
        let path = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/disambiguation.xml";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: disambiguation.xml not found");
            return;
        }
        let xml = std::fs::read_to_string(path).unwrap();
        match XmlDisambiguator::from_xml(&xml) {
            Ok(d) => {
                println!("Loaded {} disambiguation rules", d.rule_count());
                assert!(
                    d.rule_count() > 100,
                    "Expected at least 100 disambiguation rules"
                );
            }
            Err(e) => panic!("Failed to load disambiguation.xml: {}", e),
        }
    }
    #[test]
    fn real_italian_disambiguation_io_verb() {
        let path = "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/it/src/main/resources/org/languagetool/resource/it/disambiguation.xml";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: Italian disambiguation.xml not found");
            return;
        }
        let xml = std::fs::read_to_string(path).unwrap();
        let disambiguator = XmlDisambiguator::from_xml(&xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("Io", vec!["PRO-PERS-1-F-S", "PRO-PERS-1-M-S"]),
            (
                "le",
                vec!["PRO-PERS-CLI-3-F-P", "PRO-PERS-CLI-3-F-S", "ART-F:p"],
            ),
            ("ritrovo", vec!["NOUN-M:s", "VER:ind+pres+1+s"]),
        ]);
        disambiguator.disambiguate(&mut sentence);
        let ritrovo = &sentence.tokens()[2];
        assert!(
            !ritrovo.has_pos_tag("NOUN-M:s"),
            "real it disambiguation must drop the featureless noun reading"
        );
    }

    #[test]
    fn italian_io_verb_unify_drops_featureless_readings() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="it">
    <unification feature="number">
        <equivalence type="sg"><token postag="NOUN.*s|ADJ.*\+s|VER:.*\+s|PRO-.*-S" postag_regexp="yes"/></equivalence>
        <equivalence type="pl"><token postag="NOUN.*p|ADJ.*\+p|VER:.*\+p|PRO-.*-P" postag_regexp="yes"/></equivalence>
    </unification>
    <unification feature="persona">
        <equivalence type="first"><token postag=".*[-\+]1[-\+].*" postag_regexp="yes"/></equivalence>
        <equivalence type="third"><token postag=".*[-\+]3[-\+].*" postag_regexp="yes"/></equivalence>
    </unification>
    <rulegroup id="IO_VERB" name="io + verb">
        <rule>
            <pattern>
                <marker>
                    <unify>
                        <feature id="number"/><feature id="persona"/>
                        <token postag="PRO-PERS-[123].*" postag_regexp="yes"></token>
                        <unify-ignore><token postag="PRO-PERS-CLI.*" postag_regexp="yes"/></unify-ignore>
                        <token postag="VER:.*" postag_regexp="yes"></token>
                    </unify>
                </marker>
            </pattern>
            <disambig action="unify"/>
        </rule>
    </rulegroup>
</rules>"#;

        let disambiguator = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("Io", vec!["PRO-PERS-1-F-S"]),
            ("le", vec!["PRO-PERS-CLI-3-F-P", "ART-F:p"]),
            ("ritrovo", vec!["NOUN-M:s", "VER:ind+pres+1+s"]),
        ]);
        disambiguator.disambiguate(&mut sentence);
        let ritrovo = &sentence.tokens()[2];
        assert!(
            ritrovo.has_pos_tag("VER:ind+pres+1+s"),
            "verb reading must survive unify"
        );
        assert!(
            !ritrovo.has_pos_tag("NOUN-M:s"),
            "unify must drop the featureless noun reading like Java checkNext"
        );
    }

    #[test]
    fn antipattern_veto_requires_span_overlap_like_keep_by_disambig() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">
    <unification feature="number">
        <equivalence type="sg"><token postag=".*:SIN:.*" postag_regexp="yes"/></equivalence>
        <equivalence type="pl"><token postag=".*:PLU:.*" postag_regexp="yes"/></equivalence>
    </unification>
    <unification feature="case">
        <equivalence type="nom"><token postag=".*:NOM:.*" postag_regexp="yes"/></equivalence>
        <equivalence type="akk"><token postag=".*:AKK:.*" postag_regexp="yes"/></equivalence>
    </unification>
    <unification feature="gender">
        <equivalence type="mas"><token postag=".*:MAS.*" postag_regexp="yes"/></equivalence>
        <equivalence type="fem"><token postag=".*:FEM.*" postag_regexp="yes"/></equivalence>
    </unification>
    <rule id="UNIFY_ADJ_SUB" name="NP unify 3b">
        <antipattern>
            <token postag="ART:.*" postag_regexp="yes"/>
            <token postag="ADJ:.*" postag_regexp="yes"/>
            <token postag="SUB:.*" postag_regexp="yes"/>
        </antipattern>
        <pattern>
            <unify>
                <feature id="number"/><feature id="case"/><feature id="gender"/>
                <token postag="ADJ:.*" postag_regexp="yes"/>
                <token postag="SUB:.*" postag_regexp="yes"/>
            </unify>
        </pattern>
        <disambig action="unify"/>
    </rule>
</rules>"#;

        let disambiguator = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            (
                "Herzliche",
                vec![
                    "SUB:NOM:SIN:MAS:ADJ",
                    "ADJ:NOM:SIN:MAS:GRU:DEF",
                    "ADJ:NOM:SIN:FEM:GRU:DEF",
                ],
            ),
            ("Dank", vec!["SUB:NOM:SIN:MAS", "SUB:AKK:SIN:MAS"]),
            ("die", vec!["ART:NOM:SIN:FEM"]),
            (
                "schnelle",
                vec!["ADJ:NOM:SIN:FEM:GRU:DEF", "ADJ:NOM:PLU:FEM:GRU:SOL"],
            ),
            ("Hilfe", vec!["SUB:NOM:SIN:FEM", "SUB:AKK:SIN:FEM"]),
        ]);
        disambiguator.disambiguate(&mut sentence);
        let herzliche = &sentence.tokens()[0];
        assert!(
            !herzliche.has_pos_tag("SUB:NOM:SIN:MAS:ADJ"),
            "antipattern matching only 'die schnelle Hilfe' must not block unify on 'Herzliche Dank'"
        );
        assert!(
            herzliche.has_pos_tag("ADJ:NOM:SIN:MAS:GRU:DEF"),
            "unified ADJ reading must survive"
        );
        assert!(
            !herzliche.has_pos_tag("ADJ:NOM:SIN:FEM:GRU:DEF"),
            "non-unifying ADJ reading must be dropped"
        );
        let schnelle = &sentence.tokens()[3];
        assert!(
            schnelle.has_pos_tag("ADJ:NOM:PLU:FEM:GRU:SOL"),
            "overlapping antipattern must still block unify on 'schnelle Hilfe'"
        );
    }

    #[test]
    fn and_group_skip_gives_next_element_window_and_scope_next_vetoes() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">
    <rule id="KEIN_1_SIN_PRAE" name="Entferne 1:SIN:PRÄ ohne ich">
        <pattern>
            <token skip="-1" postag_regexp="yes" postag="(PKT|SENT_START)">
                <exception scope="next">ich</exception>
            </token>
            <marker>
                <and>
                    <token skip="-1" postag_regexp="yes" postag="VER:1:SIN:PRÄ.*">
                        <exception scope="next">ich</exception>
                    </token>
                    <token postag_regexp="yes" postag="VER:IMP:SIN.*"/>
                </and>
            </marker>
            <token postag="(PKT|SENT_END)" postag_regexp="yes"/>
        </pattern>
        <disambig action="remove" postag="VER:1:SIN:PRÄ.+"/>
    </rule>
</rules>"#;

        let disambiguator = XmlDisambiguator::from_xml(xml).unwrap();
        let mut removed = make_sentence(vec![
            ("<S>", vec!["SENT_START"]),
            ("Stimmt", vec!["VER:3:SIN:PRÄ:SFT"]),
            ("es", vec!["PRO:PER:NOM:SIN:ALG:3"]),
            ("dass", vec!["KON:NEB"]),
            ("du", vec!["PRO:PER:NOM:SIN:ALG:2"]),
            ("das", vec!["ART:DEF:AKK:SIN:NEU"]),
            ("Auto", vec!["SUB:AKK:SIN:NEU"]),
            ("reparier", vec!["VER:IMP:SIN:SFT", "VER:1:SIN:PRÄ:SFT"]),
            ("hast", vec!["VER:AUX:2:SIN:PRÄ"]),
            ("?", vec!["SENT_END"]),
        ]);
        disambiguator.disambiguate(&mut removed);
        let reparier = &removed.tokens()[7];
        assert!(
            !reparier.has_pos_tag("VER:1:SIN:PRÄ:SFT"),
            "and-group skip must let the final SENT_END element match past 'hast'"
        );
        assert!(reparier.has_pos_tag("VER:IMP:SIN:SFT"));

        let mut untouched = make_sentence(vec![
            ("<S>", vec!["SENT_START"]),
            ("Später", vec!["ADV"]),
            ("gehe", vec!["VER:1:SIN:PRÄ:NON", "VER:IMP:SIN"]),
            ("ich", vec!["PRO:PER:NOM:SIN:ALG:1"]),
            ("essen", vec!["VER:INF"]),
            (".", vec!["SENT_END"]),
        ]);
        disambiguator.disambiguate(&mut untouched);
        assert!(
            untouched.tokens()[2].has_pos_tag("VER:1:SIN:PRÄ:NON"),
            "scope=next exception on 'ich' must veto the removal"
        );
    }

    #[test]
    fn skip_means_up_to_n_tokens_not_exactly_n() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">
    <rule id="SKIP_ANTI" name="antipattern with skip">
        <antipattern>
            <token skip="2">ich</token>
            <token>geb</token>
        </antipattern>
        <pattern>
            <marker>
                <token>geb</token>
            </marker>
        </pattern>
        <disambig action="remove" postag="VER:.+"/>
    </rule>
</rules>"#;

        let disambiguator = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![("Ich", vec!["PRO"]), ("geb", vec!["VER:1", "SUB"])]);

        let geb = sentence.tokens_mut()[1].token().clone();
        sentence.tokens_mut()[1] = AnalyzedTokenReadings::new(geb).with_readings(vec![
            AnalyzedToken::new("geb", 4, 7).with_pos_tags(vec!["VER:1".to_string()]),
            AnalyzedToken::new("geb", 4, 7).with_pos_tags(vec!["SUB".to_string()]),
        ]);
        disambiguator.disambiguate(&mut sentence);
        assert!(
            sentence.tokens()[1].has_pos_tag("VER:1"),
            "skip=2 antipattern must match at distance 1 and block the removal"
        );

        let mut far = make_sentence(vec![
            ("Ich", vec!["PRO"]),
            ("x", vec!["ADV"]),
            ("y", vec!["ADV"]),
            ("z", vec!["ADV"]),
            ("geb", vec!["VER:1", "SUB"]),
        ]);
        let geb = far.tokens_mut()[4].token().clone();
        far.tokens_mut()[4] = AnalyzedTokenReadings::new(geb).with_readings(vec![
            AnalyzedToken::new("geb", 8, 11).with_pos_tags(vec!["VER:1".to_string()]),
            AnalyzedToken::new("geb", 8, 11).with_pos_tags(vec!["SUB".to_string()]),
        ]);
        disambiguator.disambiguate(&mut far);
        assert!(
            !far.tokens()[3].has_pos_tag("VER:1"),
            "beyond the skip range the antipattern must not block the removal"
        );
    }

    #[test]
    fn test_remove_postag_strips_matching_readings() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="HAVE_ADJ" name="have + adjective keeps only non-verb readings">
        <pattern>
            <token regexp="yes" postag="VB[ZPG]?" postag_regexp="yes" inflected="yes">be|have</token>
            <marker>
                <token postag="JJ.*" postag_regexp="yes"/>
            </marker>
        </pattern>
        <disambig action="remove" postag="VB.*"/>
    </rule>
</rules>"#;

        let disambiguator = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("it", vec!["PRP"]),
            ("has", vec!["VBZ"]),
            ("open", vec!["JJ", "NN:U", "VB", "VBP"]),
            ("positions", vec!["NNS"]),
        ]);
        let has = sentence.tokens_mut()[1].token().clone();
        sentence.tokens_mut()[1] = AnalyzedTokenReadings::new(has).with_readings(vec![
            AnalyzedToken::new("has", 3, 6)
                .with_pos_tags(vec!["VBZ".to_string()])
                .with_lemma("have".to_string()),
        ]);
        disambiguator.disambiguate(&mut sentence);
        let token = &sentence.tokens()[2];
        assert!(token.has_pos_tag("JJ"), "JJ must remain");
        assert!(token.has_pos_tag("NN:U"), "NN:U must remain");
        assert!(!token.has_pos_tag("VB"), "VB must be removed");
        assert!(!token.has_pos_tag("VBP"), "VBP must be removed");
    }
    #[test]
    fn test_immunize_action_marks_all_matched_tokens() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">
    <rule id="LOS_ANGELES" name="Los Angeles">
        <pattern>
            <token>Los</token>
            <token>Angeles</token>
        </pattern>
        <disambig action="immunize"/>
    </rule>
    <rule id="AFTER" name="filter after immunize">
        <pattern>
            <token>Angeles</token>
        </pattern>
        <disambig action="replace"><wd pos="NN"/></disambig>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![("Los", vec!["NE"]), ("Angeles", vec!["NE"])]);

        assert!(!sentence.tokens()[0].is_immunized());
        assert!(!sentence.tokens()[1].is_immunized());

        disambig.disambiguate(&mut sentence);

        assert!(sentence.tokens()[0].is_immunized());
        assert!(sentence.tokens()[1].is_immunized());
        assert!(
            sentence.tokens()[1].has_pos_tag("NE"),
            "immunized token must be invisible to later rules"
        );
    }

    #[test]
    fn immunize_marker_span_counts_elements_not_skip_bridges() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">
    <rule id="BRIDGE_IN_MARKER" name="bridge inside marker">
        <pattern>
            <marker>
                <token skip="1">A</token>
                <token>X</token>
            </marker>
        </pattern>
        <disambig action="immunize"/>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("A", vec!["NN"]),
            ("b", vec!["NN"]),
            ("X", vec!["NN"]),
        ]);

        disambig.disambiguate(&mut sentence);

        assert!(sentence.tokens()[0].is_immunized(), "marker start A");
        assert!(
            sentence.tokens()[1].is_immunized(),
            "the bridge token inside the element-count run is immunized"
        );
        assert!(
            !sentence.tokens()[2].is_immunized(),
            "Java immunizes through matchingTokensWithCorrection (A,b,X); ours stops after the element count"
        );
    }

    #[test]
    fn immunize_marker_at_first_element_starts_at_first_matched_token() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">
    <rule id="MARKER_FIRST" name="marker opens the pattern">
        <pattern>
            <marker>
                <token>X</token>
                <token>Y</token>
            </marker>
            <token>Z</token>
        </pattern>
        <disambig action="immunize"/>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("X", vec!["NN"]),
            ("Y", vec!["NN"]),
            ("Z", vec!["NN"]),
        ]);

        disambig.disambiguate(&mut sentence);

        assert!(sentence.tokens()[0].is_immunized());
        assert!(sentence.tokens()[1].is_immunized());
        assert!(
            !sentence.tokens()[2].is_immunized(),
            "spc=0: span covers the marker elements only, the trailing element stays visible"
        );
    }

    #[test]
    fn immunize_skipped_optional_element_blocks_the_whole_span() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">
    <rule id="OPTIONAL_IN_MARKER" name="optional element skipped inside marker">
        <pattern>
            <marker>
                <token>X</token>
                <token min="0">Q</token>
                <token>Y</token>
            </marker>
        </pattern>
        <disambig action="immunize"/>
    </rule>
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![("X", vec!["NN"]), ("Y", vec!["NN"])]);

        disambig.disambiguate(&mut sentence);

        assert!(
            !sentence.tokens()[0].is_immunized() && !sentence.tokens()[1].is_immunized(),
            "the all-counts-positive gate vetoes the whole span when an optional element matched nothing"
        );

        let mut present = make_sentence(vec![
            ("X", vec!["NN"]),
            ("Q", vec!["NN"]),
            ("Y", vec!["NN"]),
        ]);
        disambig.disambiguate(&mut present);
        assert!(
            present.tokens()[0].is_immunized()
                && present.tokens()[1].is_immunized()
                && present.tokens()[2].is_immunized(),
            "positive control: the same rule immunizes the full span when no element is skipped"
        );
    }

    #[test]
    fn test_filter_action_requires_full_postag_match() {
        let exact = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="FILT" name="Filter exact">
        <pattern>
            <token>run</token>
        </pattern>
        <disambig action="filter" postag="NN"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(exact).unwrap();
        let mut sentence = make_sentence(vec![("run", vec!["NN", "VB", "NNP"])]);
        disambig.disambiguate(&mut sentence);
        let token = &sentence.tokens()[0];
        assert!(token.has_pos_tag("NN"));
        assert!(!token.has_pos_tag("VB"), "partial matches must be dropped");
        assert!(!token.has_pos_tag("NNP"), "postag N must not match NNP");

        let no_match = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="FILT" name="Filter no full match">
        <pattern>
            <token>run</token>
        </pattern>
        <disambig action="filter" postag="V"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(no_match).unwrap();
        let mut sentence = make_sentence(vec![("run", vec!["NN", "VB"])]);
        disambig.disambiguate(&mut sentence);
        let token = &sentence.tokens()[0];
        assert!(
            token.has_pos_tag("NN") && token.has_pos_tag("VB"),
            "no full match means the filter is a no-op"
        );
    }

    #[test]
    fn test_unify_action_filters_readings_by_number_feature() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <unification feature="number">
        <equivalence type="sg">
            <token postag="NNP?(:UN?)?|VB[ZD]|SENT_END|PCT" postag_regexp="yes"/>
        </equivalence>
        <equivalence type="pl">
            <token postag="NNP?S|VB[PD]|SENT_END|PCT" postag_regexp="yes"/>
        </equivalence>
    </unification>
    <rule id="ENGLISH_NN" name="adjective noun">
        <pattern>
            <token postag_regexp="yes" postag="DT|JJ|PRP\$"/>
            <token postag_regexp="yes" postag="JJ.*|VBG"><exception postag="RB"/><exception regexp="yes">being|one</exception></token>
            <marker>
                <unify>
                    <feature id="number"/>
                    <and>
                        <token postag="NN.*" postag_regexp="yes"/>
                        <token postag="JJ.?" postag_regexp="yes"/>
                    </and>
                    <token postag_regexp="yes" postag="SENT_END|VB."><exception postag_regexp="yes" postag="NN.*|VBN"/></token>
                </unify>
            </marker>
        </pattern>
        <disambig action="unify"/>
    </rule>
</rules>"#;

        let disambiguator = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("my", vec!["PRP$"]),
            ("bad", vec!["JJ"]),
            ("English", vec!["JJ", "NNP"]),
            (".", vec!["SENT_END", "PCT"]),
        ]);
        disambiguator.disambiguate(&mut sentence);
        let token = &sentence.tokens()[2];
        assert!(!token.has_pos_tag("JJ"), "JJ must be dropped by unify");
        assert!(token.has_pos_tag("NNP"), "NNP must be kept");

        let mut plural = make_sentence(vec![
            ("These", vec!["DT"]),
            ("real", vec!["JJ"]),
            ("English", vec!["JJ", "NNP"]),
            ("are", vec!["VBP"]),
            ("nice", vec!["JJ"]),
            (".", vec!["PCT"]),
        ]);
        disambiguator.disambiguate(&mut plural);
        let token = &plural.tokens()[2];
        assert!(token.has_pos_tag("JJ"), "untouched example keeps JJ");
        assert!(token.has_pos_tag("NNP"), "untouched example keeps NNP");
    }

    const DE_UNIFICATIONS: &str = r#"
    <unification feature="number">
        <equivalence type="sg"><token postag=".*:SIN:.*" postag_regexp="yes"/></equivalence>
        <equivalence type="pl"><token postag=".*:PLU:.*" postag_regexp="yes"/></equivalence>
    </unification>
    <unification feature="case">
        <equivalence type="nom"><token postag=".*:NOM:.*" postag_regexp="yes"/></equivalence>
        <equivalence type="akk"><token postag=".*:AKK:.*" postag_regexp="yes"/></equivalence>
        <equivalence type="dat"><token postag=".*:DAT:.*" postag_regexp="yes"/></equivalence>
        <equivalence type="gen"><token postag=".*:GEN:.*" postag_regexp="yes"/></equivalence>
    </unification>
    <unification feature="gender">
        <equivalence type="mas"><token postag=".*:(MAS|NOG).*" postag_regexp="yes"/></equivalence>
        <equivalence type="fem"><token postag=".*:(FEM|NOG).*" postag_regexp="yes"/></equivalence>
        <equivalence type="neu"><token postag=".*:(NEU|NOG).*" postag_regexp="yes"/></equivalence>
    </unification>"#;

    fn det_noun_rule(unify_attrs: &str, extra: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">{unifications}
    <rule id="UNIFY_DET_SUB" name="det sub">
        <pattern>
            <unify{unify_attrs}>
                <feature id="number"/><feature id="case"/><feature id="gender"/>
                <token regexp="yes">der|die|das</token>
                <token postag="SUB:.*" postag_regexp="yes"/>
            </unify>
        </pattern>
        {extra}
    </rule>
</rules>"#,
            unifications = DE_UNIFICATIONS,
            unify_attrs = unify_attrs,
            extra = extra
        )
    }

    #[test]
    fn test_unify_threads_all_declared_features_per_reading() {
        let disambiguator =
            XmlDisambiguator::from_xml(&det_noun_rule("", r#"<disambig action="unify"/>"#))
                .unwrap();
        let mut sentence = make_sentence(vec![
            ("Der", vec!["ART:NOM:SIN:MAS", "ART:GEN:PLU:NEU"]),
            ("Haus", vec!["SUB:NOM:SIN:NEU", "SUB:NOM:SIN:MAS"]),
        ]);
        disambiguator.disambiguate(&mut sentence);
        let der = &sentence.tokens()[0];
        assert_eq!(der.readings().len(), 1);
        assert!(der.has_pos_tag("ART:NOM:SIN:MAS"));
        let haus = &sentence.tokens()[1];
        assert_eq!(haus.readings().len(), 1);
        assert!(haus.has_pos_tag("SUB:NOM:SIN:MAS"));
    }

    #[test]
    fn test_unify_leaves_readings_untouched_when_sequence_does_not_unify() {
        let disambiguator =
            XmlDisambiguator::from_xml(&det_noun_rule("", r#"<disambig action="unify"/>"#))
                .unwrap();
        let mut sentence = make_sentence(vec![
            ("Der", vec!["ART:NOM:SIN:MAS"]),
            ("Harz", vec!["SUB:AKK:SIN:NEU", "SUB:NOM:SIN:NEU"]),
        ]);
        disambiguator.disambiguate(&mut sentence);
        assert_eq!(sentence.tokens()[0].readings().len(), 1);
        assert_eq!(sentence.tokens()[1].readings().len(), 2);
    }

    #[test]
    fn test_unify_ignore_position_keeps_all_its_readings() {
        let xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">{unifications}
    <rule id="UNIFY_NEUTRAL" name="neutral">
        <pattern>
            <unify>
                <feature id="number"/>
                <token postag="ART:.*" postag_regexp="yes"/>
                <unify-ignore>
                    <token>und</token>
                </unify-ignore>
                <token postag="SUB:.*" postag_regexp="yes"/>
            </unify>
        </pattern>
        <disambig action="unify"/>
    </rule>
</rules>"#,
            unifications = DE_UNIFICATIONS
        );
        let disambiguator = XmlDisambiguator::from_xml(&xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("die", vec!["ART:NOM:SIN:MAS", "ART:NOM:PLU:MAS"]),
            ("und", vec!["KON", "SUB:NOM:PLU:NEU"]),
            ("Haus", vec!["SUB:NOM:SIN:NEU"]),
        ]);
        disambiguator.disambiguate(&mut sentence);
        assert!(sentence.tokens()[0].has_pos_tag("ART:NOM:SIN:MAS"));
        assert!(
            !sentence.tokens()[0].has_pos_tag("ART:NOM:PLU:MAS"),
            "the plural article finds no singular partner"
        );
        assert_eq!(
            sentence.tokens()[1].readings().len(),
            2,
            "neutral positions keep every reading"
        );
        assert!(sentence.tokens()[2].has_pos_tag("SUB:NOM:SIN:NEU"));
    }

    #[test]
    fn test_negated_unify_blocks_the_rule_when_the_sequence_unifies() {
        let disambiguator = XmlDisambiguator::from_xml(&det_noun_rule(
            r#" negate="yes""#,
            r#"<disambig action="immunize"/>"#,
        ))
        .unwrap();
        let mut unified = make_sentence(vec![
            ("Der", vec!["ART:NOM:SIN:MAS"]),
            ("Haus", vec!["SUB:NOM:SIN:MAS"]),
        ]);
        disambiguator.disambiguate(&mut unified);
        assert!(
            !unified.tokens()[0].is_immunized(),
            "unifying sequence must fail a negated unify rule"
        );

        let mut split = make_sentence(vec![
            ("Der", vec!["ART:NOM:SIN:MAS"]),
            ("Harz", vec!["SUB:NOM:SIN:NEU"]),
        ]);
        disambiguator.disambiguate(&mut split);
        assert!(split.tokens()[0].is_immunized());
    }

    #[test]
    fn pattern_token_backreference_requires_same_surface() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="STEP_X_STEP">
        <pattern>
            <marker>
                <token postag="NN" regexp="yes">step|bit</token>
                <token>by</token>
                <token postag="NN"><match no="0"/></token>
            </marker>
        </pattern>
        <disambig action="filterall"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut same = make_sentence(vec![
            ("step", vec!["NN", "VB"]),
            ("by", vec!["IN"]),
            ("step", vec!["NN", "VB"]),
        ]);
        disambig.disambiguate(&mut same);
        assert_eq!(
            same.tokens()[0].readings().len(),
            1,
            "filterall applies when the referenced surface matches"
        );
        let mut other = make_sentence(vec![
            ("step", vec!["NN", "VB"]),
            ("by", vec!["IN"]),
            ("bit", vec!["NN", "VB"]),
        ]);
        disambig.disambiguate(&mut other);
        assert_eq!(
            other.tokens()[0].readings().len(),
            2,
            "a different surface must not match the backreference"
        );
    }

    #[test]
    fn chunk_re_constraints_are_honored_and_marker_targets_first_marked_token() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="DO_NN_VB" name="does Peter have (VB) time">
        <pattern>
            <token postag="SENT_START|PCT|CC" postag_regexp="yes" />
            <token regexp="yes" min="0">why|when|where|who|which|what|how</token>
            <token inflected="yes" regexp="yes">do|will|can|should|may|must</token>
            <token min="0">n't</token>
            <token chunk_re="B-NP.*" min="0" />
            <token chunk_re="E-NP.*" />
            <marker>
                <token postag="VB" chunk_re=".-VP" />
            </marker>
        </pattern>
        <disambig action="filter" postag="VB" />
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("<S>", vec!["SENT_START"]),
            ("Do", vec!["VB", "VBP"]),
            ("you", vec!["PRP"]),
            ("have", vec!["NN", "VB", "VBP"]),
            ("bicycle", vec!["NN", "VB", "VBP"]),
            (".", vec!["."]),
        ]);
        let chunks: [&[&str]; 6] = [
            &[],
            &[],
            &["B-NP-singular", "E-NP-singular"],
            &["B-VP"],
            &["B-NP-singular", "E-NP-singular"],
            &[],
        ];
        for (t, ch) in sentence.tokens_mut().iter_mut().zip(chunks) {
            t.set_chunks(ch.iter().map(|c| c.to_string()).collect());
        }
        disambig.disambiguate(&mut sentence);
        let have = &sentence.tokens()[3];
        assert_eq!(
            have.readings()
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect::<Vec<_>>(),
            vec!["VB".to_string()],
            "the filter must hit the marked 'have', not the following token"
        );
        let bicycle = &sentence.tokens()[4];
        assert!(
            bicycle.has_pos_tag("NN"),
            "bicycle must keep its NN reading"
        );
    }

    fn digit_hyphenated_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="DIGIT_HYPHENATED_ADJECTIVES" name="hyphenated adjectives">
        <pattern>
            <token regexp="yes">[0-9]+-[a-z]+<exception postag="JJ"/></token>
        </pattern>
        <filter class="org.languagetool.rules.en.NoDisambiguationEnglishPartialPosTagFilter"
            args="no:1 regexp:[0-9]+-([a-z]+|[A-Z]+) postag_regexp:NN(:UN?)?"/>
        <disambig action="add"><wd pos="JJ"/></disambig>
    </rule>
</rules>"#
    }

    fn en_resource_dir() -> Option<std::sync::Arc<crate::english_tagger::EnglishTagger>> {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en",
        );
        if !dir.exists() {
            return None;
        }
        Some(std::sync::Arc::new(
            crate::english_tagger::EnglishTagger::from_resource_dir(dir),
        ))
    }

    #[test]
    fn partial_postag_filter_blocks_mixed_case_and_plural_stems() {
        let Some(tagger) = en_resource_dir() else {
            return;
        };
        let disambig =
            XmlDisambiguator::from_xml_with_english(digit_hyphenated_xml(), Some(tagger)).unwrap();
        for token in ["4-Hour", "500-pages"] {
            let mut sentence = make_sentence(vec![(token, vec!["CD"])]);
            disambig.disambiguate(&mut sentence);
            assert!(
                !sentence.tokens()[0].has_pos_tag("JJ"),
                "{token} must stay untouched: the case-sensitive filter regexp cannot match the mixed-case stem, and a plural stem is NNS"
            );
        }
    }

    #[test]
    fn partial_postag_filter_admits_singular_noun_stems() {
        let Some(tagger) = en_resource_dir() else {
            return;
        };
        let disambig =
            XmlDisambiguator::from_xml_with_english(digit_hyphenated_xml(), Some(tagger)).unwrap();
        let mut sentence = make_sentence(vec![("500-page", vec!["CD"])]);
        disambig.disambiguate(&mut sentence);
        assert!(
            sentence.tokens()[0].has_pos_tag("JJ"),
            "the 'page' stem tags NN so the JJ reading must be added"
        );
    }

    #[test]
    fn partial_postag_filter_stays_inert_without_a_supplied_tagger() {
        if en_resource_dir().is_none() {
            return;
        }
        let disambig = XmlDisambiguator::from_xml(digit_hyphenated_xml()).unwrap();
        let mut sentence = make_sentence(vec![("500-page", vec!["CD"])]);
        disambig.disambiguate(&mut sentence);
        assert!(
            !sentence.tokens()[0].has_pos_tag("JJ"),
            "from_xml supplies no English tagger, so the filter rejects like Java's null-tagger modules"
        );
    }

    fn english_word_filter_xml(args: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="ca">
    <rule id="ANGLICISME" name="english words keep an unknown reading">
        <pattern>
            <token>for</token>
            <marker><token regexp="yes">[a-z]+</token></marker>
        </pattern>
        <filter class="org.languagetool.rules.IsEnglishWordFilter" args="{args}"/>
        <disambig action="add"><wd pos="EN"/></disambig>
    </rule>
</rules>"#
        )
    }

    #[test]
    fn english_word_filter_admits_tagged_english_forms_with_a_supplied_tagger() {
        if en_resource_dir().is_none() {
            return;
        }
        let disambig = XmlDisambiguator::from_xml_with_english(
            &english_word_filter_xml("formPositions:2"),
            crate::english_tagger::shared_english_tagger(),
        )
        .expect("compile IsEnglishWordFilter rule");
        let mut english = make_sentence(vec![("for", vec!["SP"]), ("example", vec!["NC"])]);
        disambig.disambiguate(&mut english);
        assert!(
            english.tokens()[1].has_pos_tag("EN"),
            "an English-dictionary word at form position 2 passes IsEnglishWordFilter"
        );
        let mut foreign = make_sentence(vec![("for", vec!["SP"]), ("qzxvbk", vec!["NC"])]);
        disambig.disambiguate(&mut foreign);
        assert!(
            !foreign.tokens()[1].has_pos_tag("EN"),
            "a word the English tagger cannot tag must block the rule like Java's wordIsTagged"
        );
    }

    #[test]
    fn english_word_filter_postags_requires_full_tag_match_per_form() {
        if en_resource_dir().is_none() {
            return;
        }
        let disambig = XmlDisambiguator::from_xml_with_english(
            &english_word_filter_xml("formPositions:2 postags:VB"),
            crate::english_tagger::shared_english_tagger(),
        )
        .unwrap();
        let mut verb = make_sentence(vec![("for", vec!["SP"]), ("run", vec!["NC"])]);
        disambig.disambiguate(&mut verb);
        assert!(
            verb.tokens()[1].has_pos_tag("EN"),
            "'run' tags VB so it passes the postags:VB constraint"
        );
        let mut noun = make_sentence(vec![("for", vec!["SP"]), ("furniture", vec!["NC"])]);
        disambig.disambiguate(&mut noun);
        assert!(
            !noun.tokens()[1].has_pos_tag("EN"),
            "'furniture' tags NN:U only, so postags:VB must reject the match like matchesPosTagRegex"
        );
    }

    #[test]
    fn match_lemma_filter_keeps_only_readings_with_that_lemma() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="pl">
    <rule id="MAJA_MAIC" name="mają to nie maić">
        <pattern>
            <token>mają</token>
        </pattern>
        <disambig action="filter"><match no="1">mieć</match></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = AnalyzedSentence::new("mają", 0, "mają".len());
        let readings = vec![
            AnalyzedToken::new("mają", 0, 4)
                .with_pos_tags(vec!["verb:fin:pl:ter:imperf:refl.nonrefl".to_string()])
                .with_lemma("maić"),
            AnalyzedToken::new("mają", 0, 4)
                .with_pos_tags(vec!["subst:sg:inst:f".to_string()])
                .with_lemma("maja"),
            AnalyzedToken::new("mają", 0, 4)
                .with_pos_tags(vec!["verb:fin:pl:ter:imperf:refl.nonrefl".to_string()])
                .with_lemma("mieć"),
        ];
        let maja =
            AnalyzedTokenReadings::new(AnalyzedToken::new("mają", 0, 4)).with_readings(readings);
        sentence.set_tokens(vec![maja]);
        disambig.disambiguate(&mut sentence);
        let maja = &sentence.tokens()[0];
        let kept: Vec<String> = maja
            .readings()
            .iter()
            .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("+")))
            .collect();
        assert_eq!(
            kept,
            vec!["mieć/verb:fin:pl:ter:imperf:refl.nonrefl".to_string()],
            "the MAJA_MAIC example outputform keeps only the mieć reading"
        );
    }

    #[test]
    fn match_lemma_filter_falls_back_to_a_tagless_token_when_nothing_survives() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="pl">
    <rule id="MAJA_MAIC" name="mają to nie maić">
        <pattern>
            <token>mają</token>
        </pattern>
        <disambig action="filter"><match no="1">nie_ma_takiego_lematu</match></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![("mają", vec!["subst:sg:inst:f"])]);
        disambig.disambiguate(&mut sentence);
        let maja = &sentence.tokens()[0];
        assert_eq!(
            maja.readings().len(),
            1,
            "the drained token keeps exactly one reading"
        );
        assert!(
            maja.readings()[0].pos_tags().is_empty(),
            "the surviving reading carries no POS tag"
        );
        assert_eq!(maja.token().token(), "mają");
    }

    #[test]
    fn match_postag_regexp_replace_rewrites_each_matching_reading() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="eo">
    <rule id="VIVI" name="vivi vivon">
        <pattern>
            <marker><token inflected="yes" regexp="yes">viv(?:e[tg])?i</token></marker>
            <token regexp="yes">viv(?:e[gt])?on</token>
        </pattern>
        <disambig action="replace"><match no="1" postag_regexp="yes" postag="V nt (.*)" postag_replace="V tr $1"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence_with_lemmas(vec![
            ("vivis", "vivi", vec!["V nt 3 p", "V nt 1 s"]),
            ("vivon", "vivon", vec!["N akz"]),
        ]);
        disambig.disambiguate(&mut sentence);
        let vivis = &sentence.tokens()[0];
        let rewritten: Vec<String> = vivis
            .readings()
            .iter()
            .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("+")))
            .collect();
        assert_eq!(
            rewritten,
            vec!["vivi/V tr 3 p".to_string(), "vivi/V tr 1 s".to_string()],
            "each V nt reading is rewritten to V tr with its group kept and lemma preserved"
        );
        let vivon = &sentence.tokens()[1];
        assert_eq!(
            vivon.readings().len(),
            1,
            "the action must only touch the marked token"
        );
        assert!(vivon.has_pos_tag("N akz"));
    }

    #[test]
    fn match_postag_lookahead_falls_back_to_fancy_regex() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="eo">
    <rule id="VIVI_FANCY" name="lookahead postag">
        <pattern>
            <marker><token>vivis</token></marker>
        </pattern>
        <disambig action="replace"><match no="1" postag_regexp="yes" postag="V nt 3 p(?=)" postag_replace="V tr"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence_with_lemmas(vec![("vivis", "vivi", vec!["V nt 3 p"])]);
        disambig.disambiguate(&mut sentence);
        let vivis = &sentence.tokens()[0];
        let rewritten: Vec<String> = vivis
            .readings()
            .iter()
            .map(|r| r.pos_tags().join("+"))
            .collect();
        assert_eq!(
            rewritten,
            vec!["V tr".to_string()],
            "the empty lookahead postag must match and rewrite through the fancy-regex fallback"
        );
    }

    #[test]
    fn match_postag_on_a_readingless_token_changes_nothing() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="eo">
    <rule id="VIVI_EMPTY" name="empty readings">
        <pattern>
            <marker><token>vivis</token></marker>
        </pattern>
        <disambig action="replace"><match no="1" postag_regexp="yes" postag="V nt" postag_replace="V tr"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = AnalyzedSentence::new("vivis", 0, 5);
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("vivis", 0, 5)).with_readings(vec![]),
        ]);
        disambig.disambiguate(&mut sentence);
        assert!(
            sentence.tokens()[0].readings().is_empty(),
            "a token without readings must pass through untouched"
        );
    }

    #[test]
    fn remove_wd_restores_sentence_end_and_keeps_chunks() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="TRAILING_JJ" name="drop trailing JJ">
        <pattern>
            <marker><token postag="JJ"/></marker>
        </pattern>
        <disambig action="remove"><wd pos="JJ"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![("quick", vec!["JJ", "NN"])]);
        {
            let last = sentence.tokens_mut().last_mut().unwrap();
            last.set_chunks(vec!["B-NP".to_string()]);
            last.add_sent_end();
        }
        disambig.disambiguate(&mut sentence);
        let quick = &sentence.tokens()[0];
        assert!(quick.has_pos_tag("NN"));
        assert!(!quick.has_pos_tag("JJ"));
        assert!(
            quick.has_pos_tag("SENT_END"),
            "removing the JJ reading must not drop the sentence-end marker"
        );
        assert_eq!(
            quick.chunks().to_vec(),
            vec!["B-NP".to_string()],
            "the rewrite must carry over chunk tags"
        );
    }

    #[test]
    fn replace_wd_keeps_the_ignored_by_speller_flag() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="br">
    <rule id="DA_E" name="da as verb particle">
        <pattern>
            <marker><token>da</token></marker>
            <token>gonnad</token>
        </pattern>
        <disambig action="replace"><wd pos="L e"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = AnalyzedSentence::new("da gonnad", 0, 9);
        let da_readings = vec![
            AnalyzedToken::new("da", 0, 2).with_pos_tags(vec!["P".to_string()]),
            AnalyzedToken::new("da", 0, 2).with_pos_tags(vec!["D e sp".to_string()]),
        ];
        let gonnad_readings =
            vec![AnalyzedToken::new("gonnad", 3, 9).with_pos_tags(vec!["V ng".to_string()])];
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("da", 0, 2)).with_readings(da_readings),
            AnalyzedTokenReadings::new(AnalyzedToken::new("gonnad", 3, 9))
                .with_readings(gonnad_readings),
        ]);
        sentence.tokens_mut().first_mut().unwrap().ignore_spelling();
        disambig.disambiguate(&mut sentence);
        let da = &sentence.tokens()[0];
        assert!(da.has_pos_tag("L e"));
        assert!(!da.has_pos_tag("P"));
        assert!(
            da.is_ignored_by_speller(),
            "replaced readings keep the ignored-by-speller flag"
        );
    }

    #[test]
    fn add_wd_skips_reading_that_already_exists() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="PCT_AGAIN" name="add PCT again">
        <pattern>
            <token>.</token>
        </pattern>
        <disambig action="add"><wd pos="PCT"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence_with_lemmas(vec![(".", ".", vec![".", "PCT"])]);
        disambig.disambiguate(&mut sentence);
        let dot = &sentence.tokens()[0];
        let pct_count = dot
            .readings()
            .iter()
            .filter(|r| r.pos_tags().first() == Some(&"PCT".to_string()))
            .count();
        assert_eq!(
            pct_count, 1,
            "the already-present PCT reading must not be duplicated"
        );
        assert!(dot.has_pos_tag("."));
    }

    fn spaced_dots_sentence() -> AnalyzedSentence {
        let spec = [
            ("is", 0usize, 2usize),
            (".", 2, 3),
            (".", 4, 5),
            (".", 6, 7),
        ];
        let analyzed = spec
            .iter()
            .map(|(text, start, end)| {
                let base = AnalyzedToken::new(*text, *start, *end);
                AnalyzedTokenReadings::new(base)
                    .with_readings(vec![AnalyzedToken::new(*text, *start, *end)])
            })
            .collect();
        let mut sentence = AnalyzedSentence::new("is. . .", 0, 7);
        sentence.set_tokens(analyzed);
        sentence
    }

    #[test]
    fn spacebefore_yes_rejects_glued_leading_token() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="IMMUNIZE_ELLIPSIS" name="immunize ellipsis">
        <pattern>
            <token spacebefore="yes">.</token>
            <token spacebefore="yes">.</token>
            <token spacebefore="yes">.</token>
        </pattern>
        <disambig action="immunize"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = spaced_dots_sentence();
        disambig.disambiguate(&mut sentence);
        assert!(
            !sentence.tokens()[1].is_immunized(),
            "the first dot is glued to 'is' so spacebefore=yes must fail and the rule must not immunize"
        );
    }

    #[test]
    fn spacebefore_yes_immunizes_fully_spaced_dots() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="IMMUNIZE_ELLIPSIS" name="immunize ellipsis">
        <pattern>
            <token spacebefore="yes">.</token>
            <token spacebefore="yes">.</token>
            <token spacebefore="yes">.</token>
        </pattern>
        <disambig action="immunize"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = spaced_dots_sentence();
        {
            let tokens = sentence.tokens_mut();
            tokens[1] = AnalyzedTokenReadings::new(AnalyzedToken::new(".", 8, 9))
                .with_readings(vec![AnalyzedToken::new(".", 8, 9)]);
        }
        disambig.disambiguate(&mut sentence);
        assert!(
            sentence.tokens()[1].is_immunized()
                && sentence.tokens()[2].is_immunized()
                && sentence.tokens()[3].is_immunized(),
            "three fully spaced dots must be immunized like Java's IMMUNIZE_ELLIPSIS"
        );
    }

    #[test]
    fn filterall_filters_marker_positions_by_element_postag_including_and_groups() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="DO_not_VBNN_VB" name="do/did/does + not + verb/noun -> VB">
        <pattern>
            <marker>
                <token postag="VB.*" postag_regexp="yes" regexp="yes">do(es)?|did</token>
                <token>not</token>
                <and>
                    <token postag="VB"/>
                    <token postag_regexp="yes" postag="JJ|NN.*"/>
                </and>
            </marker>
        </pattern>
        <disambig action="filterall"/>
    </rule>
</rules>"#;
        let disambiguator = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("<S>", vec!["SENT_START"]),
            ("I", vec!["PRP"]),
            ("did", vec!["VBD"]),
            ("not", vec!["RB"]),
            ("notice", vec!["NN:UN", "VB", "VBP"]),
            (".", vec!["SENT_END"]),
        ]);
        disambiguator.disambiguate(&mut sentence);
        let notice = &sentence.tokens()[4];
        assert!(
            notice.has_pos_tag("VB"),
            "the VB reading kept by the and-group's outer postag must survive"
        );
        assert!(
            !notice.has_pos_tag("NN:UN") && !notice.has_pos_tag("VBP"),
            "filterall must filter the and-group position with the first sub-token's postag (VB) like Java's PatternToken.getPOStag()"
        );
        assert!(
            sentence.tokens()[2].has_pos_tag("VBD"),
            "the regexp-postag element keeps its own matching readings"
        );
        assert!(
            sentence.tokens()[3].has_pos_tag("RB"),
            "an element without postag leaves its token untouched"
        );
    }

    #[test]
    fn scope_previous_exception_vetoes_the_text_verified_first_element() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="CAN_WILL_MD_2" name="can/will/must as a modal verb with adverb">
        <pattern>
            <marker><token regexp="yes">can|will|must<exception scope="previous" regexp="yes">be|am|is|was|are|were</exception></token></marker>
            <token postag="RB"/>
            <token postag="VB"><exception postag="MD"/></token>
        </pattern>
        <disambig postag="MD"/>
    </rule>
</rules>"#;
        let disambiguator = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("<S>", vec!["SENT_START"]),
            ("accession", vec!["NN"]),
            ("is", vec!["VBZ"]),
            ("will", vec!["MD", "NN:UN", "VB", "VBP"]),
            ("ultimately", vec!["RB"]),
            ("be", vec!["VB"]),
        ]);
        disambiguator.disambiguate(&mut sentence);
        assert!(
            sentence.tokens()[3].has_pos_tag("NN:UN"),
            "scope=previous exception on 'is' must veto the marker element even on the text-verified first-element path"
        );
        let mut modal = make_sentence(vec![
            ("<S>", vec!["SENT_START"]),
            ("This", vec!["DT"]),
            ("can", vec!["MD", "NN", "VB", "VBP"]),
            ("only", vec!["RB"]),
            ("be", vec!["VB"]),
        ]);
        disambiguator.disambiguate(&mut modal);
        assert!(
            modal.tokens()[2].readings().len() == 1 && modal.tokens()[2].has_pos_tag("MD"),
            "without the previous-token veto the rule still replaces with MD"
        );
    }

    #[test]
    fn spacebefore_is_enforced_on_the_match_initial_element() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="QUOT_M3" name="quote glued to the previous token stays untouched">
        <pattern>
            <marker>
                <and>
                    <token postag="''" spacebefore="yes">x</token>
                    <token postag="``" spacebefore="yes">x</token>
                </and>
            </marker>
        </pattern>
        <disambig postag="``"/>
    </rule>
</rules>"#;
        let disambiguator = XmlDisambiguator::from_xml(xml).unwrap();
        let build = |quote_start: usize| {
            let mut sentence = make_sentence(vec![("<S>", vec!["SENT_START"])]);
            let tokens = sentence.tokens_mut();
            tokens.push(
                AnalyzedTokenReadings::new(AnalyzedToken::new("a", 0, 1)).with_readings(vec![
                    AnalyzedToken::new("a", 0, 1).with_pos_tags(vec!["NN".to_string()]),
                ]),
            );
            tokens.push(
                AnalyzedTokenReadings::new(AnalyzedToken::new("x", quote_start, quote_start + 1))
                    .with_readings(vec![
                        AnalyzedToken::new("x", quote_start, quote_start + 1)
                            .with_pos_tags(vec!["''".to_string()]),
                        AnalyzedToken::new("x", quote_start, quote_start + 1)
                            .with_pos_tags(vec!["``".to_string()]),
                    ]),
            );
            sentence
        };
        let mut glued = build(1);
        disambiguator.disambiguate(&mut glued);
        assert!(
            glued.tokens()[2].readings().len() == 2,
            "spacebefore=yes must veto the match-initial element when the token is glued to the previous one, like PatternToken.isMatched which tests the whitespace flag on every element"
        );
        let mut spaced = build(2);
        disambiguator.disambiguate(&mut spaced);
        assert!(
            spaced.tokens()[2].readings().len() == 1 && spaced.tokens()[2].has_pos_tag("``"),
            "with whitespace before the token the spacebefore=yes element still matches"
        );
    }

    fn surface_sentence(text: &str) -> AnalyzedSentence {
        let mut offset = 0;
        let mut analyzed = Vec::new();
        for word in text.split_whitespace() {
            let start = offset;
            let end = offset + word.len();
            analyzed.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                word, start, end,
            )));
            offset = end + 1;
        }
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        sentence.set_tokens(analyzed);
        sentence
    }

    fn readings_signature(sentence: &AnalyzedSentence) -> Vec<String> {
        sentence
            .tokens()
            .iter()
            .map(|t| {
                let mut sig: Vec<String> = t
                    .readings()
                    .iter()
                    .flat_map(|r| r.pos_tags().to_vec())
                    .collect();
                sig.sort();
                sig
            })
            .flatten()
            .collect()
    }

    #[test]
    fn rule_selection_parity_on_real_disambiguation_files() {
        let cases = [
            (
                "pl",
                "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/pl/src/main/resources/org/languagetool/resource/pl/disambiguation.xml",
            ),
            (
                "ca",
                "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ca/src/main/resources/org/languagetool/resource/ca/disambiguation.xml",
            ),
            (
                "es",
                "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/resource/es/disambiguation.xml",
            ),
        ];
        for (code, path) in cases {
            let Ok(xml) = std::fs::read_to_string(path) else {
                eprintln!("Skipping {code}: disambiguation.xml not present");
                continue;
            };
            let disambiguator = XmlDisambiguator::from_xml(&xml).expect("parse disambiguation.xml");
            let mut checked = 0usize;
            for idx in 0..disambiguator.rule_count() {
                let (untouched, _) = disambiguator.rule_examples(idx);
                for text in untouched.iter().take(2) {
                    let mut selected = surface_sentence(text);
                    disambiguator.disambiguate(&mut selected);
                    let mut full = surface_sentence(text);
                    for rule_idx in 0..disambiguator.rule_count() {
                        disambiguator.apply_rule(&mut full, rule_idx);
                    }
                    assert_eq!(
                        readings_signature(&selected),
                        readings_signature(&full),
                        "{code} rule {idx}: selected rules must produce the readings of the full scan on '{text}'"
                    );
                    checked += 1;
                }
                if checked >= 120 {
                    break;
                }
            }
            assert!(checked > 0, "{code}: example sentences must be available");
        }
    }

    #[test]
    fn literal_rules_are_pruned_and_absent_literals_keep_readings() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="PRUNE_ME" name="literal first token">
        <pattern>
            <token>unprecedented</token>
            <token postag="NN"/>
        </pattern>
        <disambig postag="NN"/>
    </rule>
    <rule id="REGEXP_ALT" name="regexp alternation">
        <pattern>
            <token regexp="yes">alpha|beta</token>
        </pattern>
        <disambig action="remove" postag="VB"/>
    </rule>
</rules>"#;
        let disambiguator = XmlDisambiguator::from_xml(xml).unwrap();
        let mut absent = make_sentence(vec![("She", vec!["PRP"]), ("runs", vec!["VBZ", "NN"])]);
        disambiguator.disambiguate(&mut absent);
        assert_eq!(
            absent.tokens()[1].readings().len(),
            2,
            "no rule's literal values occur in the sentence, so no readings may change"
        );
        let mut present = make_sentence(vec![("the", vec!["DT"]), ("alpha", vec!["NN", "VB"])]);
        disambiguator.disambiguate(&mut present);
        assert_eq!(
            present.tokens()[1].readings().len(),
            1,
            "the regexp alternation rule must fire on 'alpha' and remove the VB reading"
        );
        assert!(present.tokens()[1].has_pos_tag("NN"));
    }

    #[test]
    fn expand_literal_regexp_covers_java_shapes() {
        assert_eq!(
            expand_literal_regexp("alpha|beta").unwrap(),
            vec!["alpha".to_string(), "beta".to_string()]
        );
        assert_eq!(
            expand_literal_regexp("colou?r").unwrap(),
            vec!["color".to_string(), "colour".to_string()]
        );
        assert_eq!(
            expand_literal_regexp("(?:foo|bar)b").unwrap(),
            vec!["barb".to_string(), "foob".to_string()]
        );
        assert_eq!(
            expand_literal_regexp(r"a\.b").unwrap(),
            vec!["a.b".to_string()]
        );
        assert!(expand_literal_regexp("a.*b").is_none());
        assert!(expand_literal_regexp("[abc]").is_none());
        assert!(expand_literal_regexp(r"\d+").is_none());
        assert!(expand_literal_regexp("(unclosed").is_none());
    }

    #[test]
    fn bare_actions_without_postag_wd_or_match_leave_readings_untouched() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="BARE_REPLACE" name="replace without payload">
        <pattern><marker><token>alpha</token></marker></pattern>
        <disambig action="replace"/>
    </rule>
    <rule id="BARE_REMOVE" name="remove without payload">
        <pattern><marker><token>beta</token></marker></pattern>
        <disambig action="remove"/>
    </rule>
    <rule id="BARE_ADD" name="add without payload">
        <pattern><marker><token>gamma</token></marker></pattern>
        <disambig action="add"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("alpha", vec!["NN", "VB"]),
            ("beta", vec!["NN", "VB"]),
            ("gamma", vec!["NN", "VB"]),
        ]);
        disambig.disambiguate(&mut sentence);
        for (idx, word) in ["alpha", "beta", "gamma"].iter().enumerate() {
            assert_eq!(
                readings_of(&sentence.tokens()[idx]),
                vec![(None, "NN".to_string()), (None, "VB".to_string())],
                "the bare action on '{word}' must not touch the readings"
            );
        }
    }

    #[test]
    fn malformed_action_postag_regexes_are_skipped() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="BAD_FILTER" name="filter with a broken regex">
        <pattern><marker><token>alpha</token></marker></pattern>
        <disambig action="filter" postag="("/>
    </rule>
    <rule id="BAD_REMOVE" name="remove with a broken regex">
        <pattern><marker><token>beta</token></marker></pattern>
        <disambig action="remove" postag="("/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = make_sentence(vec![
            ("alpha", vec!["NN", "VB"]),
            ("beta", vec!["NN", "VB"]),
        ]);
        disambig.disambiguate(&mut sentence);
        for idx in 0..2 {
            assert_eq!(
                sentence.tokens()[idx].readings().len(),
                2,
                "a postag that compiles as no regex at all must leave the readings alone"
            );
        }
    }

    #[test]
    fn immunized_tokens_are_skipped_by_disambiguation_rules() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_SET" name="set the pronoun tag">
        <pattern><marker><token>le</token></marker></pattern>
        <disambig postag="PRON"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = le_sentence();
        sentence.tokens_mut()[0].immunize();
        disambig.disambiguate(&mut sentence);
        let token = &sentence.tokens()[0];
        assert!(token.is_immunized());
        assert_eq!(
            readings_of(token),
            vec![
                (Some("le"), "D f s".to_string()),
                (Some("le"), "R pers obj 3 s".to_string())
            ],
            "an immunized token must not match any disambiguation rule, like PatternRuleMatcher"
        );
    }

    #[test]
    fn filter_action_keeps_the_speller_flag() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_FILTER" name="keep only the pronoun reading">
        <pattern><marker><token>le</token></marker></pattern>
        <disambig action="filter" postag="R.*"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = le_sentence();
        sentence.tokens_mut()[0].ignore_spelling();
        disambig.disambiguate(&mut sentence);
        assert!(sentence.tokens()[0].is_ignored_by_speller());
        assert_eq!(
            readings_of(&sentence.tokens()[0]),
            vec![(Some("le"), "R pers obj 3 s".to_string())]
        );
    }

    #[test]
    fn remove_postag_action_keeps_the_speller_flag() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_DROP_DET" name="drop the determiner reading">
        <pattern><marker><token>le</token></marker></pattern>
        <disambig action="remove" postag="D f s"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = le_sentence();
        sentence.tokens_mut()[0].ignore_spelling();
        disambig.disambiguate(&mut sentence);
        assert!(sentence.tokens()[0].is_ignored_by_speller());
        assert_eq!(
            readings_of(&sentence.tokens()[0]),
            vec![(Some("le"), "R pers obj 3 s".to_string())]
        );
    }

    #[test]
    fn match_action_lemma_text_keeps_the_speller_flag() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_LEMMA_FLAGS" name="lemma match keeps the flags">
        <pattern><marker><token>le</token></marker></pattern>
        <disambig action="replace"><match>le</match></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = AnalyzedSentence::new("test", 0, 5);
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("le", 0, 2)).with_readings(vec![
                AnalyzedToken::new("le", 0, 2)
                    .with_pos_tags(vec!["D f s".to_string()])
                    .with_lemma("le"),
                AnalyzedToken::new("le", 0, 2)
                    .with_pos_tags(vec!["D f p".to_string()])
                    .with_lemma("la"),
            ]),
            AnalyzedTokenReadings::new(AnalyzedToken::new("x", 3, 4)).with_readings(vec![
                AnalyzedToken::new("x", 3, 4).with_pos_tags(vec!["NC".to_string()]),
            ]),
        ]);
        sentence.tokens_mut()[0].ignore_spelling();
        disambig.disambiguate(&mut sentence);
        let token = &sentence.tokens()[0];
        assert!(token.is_ignored_by_speller());
        assert_eq!(
            readings_of(token),
            vec![(Some("le"), "D f s".to_string())],
            "only the lemma-matching reading survives and the speller flag carries over"
        );
    }

    #[test]
    fn match_action_regexp_postag_skips_tagless_readings() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_REGEXP_TAGLESS" name="regexp skips tagless readings">
        <pattern><marker><token>le</token></marker></pattern>
        <disambig action="replace"><match postag="R.*" postag_regexp="yes"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = AnalyzedSentence::new("test", 0, 5);
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("le", 0, 2)).with_readings(vec![
                AnalyzedToken::new("le", 0, 2)
                    .with_pos_tags(vec!["R pers obj 3 s".to_string()])
                    .with_lemma("le"),
                AnalyzedToken::new("le", 0, 2),
                AnalyzedToken::new("le", 0, 2)
                    .with_pos_tags(vec!["D f s".to_string()])
                    .with_lemma("le"),
            ]),
            AnalyzedTokenReadings::new(AnalyzedToken::new("x", 3, 4)).with_readings(vec![
                AnalyzedToken::new("x", 3, 4).with_pos_tags(vec!["NC".to_string()]),
            ]),
        ]);
        disambig.disambiguate(&mut sentence);
        assert_eq!(
            readings_of(&sentence.tokens()[0]),
            vec![(Some("le"), "R pers obj 3 s".to_string())],
            "the tagless middle reading is skipped instead of matching or panicking"
        );
    }

    #[test]
    fn match_action_lemma_text_with_a_postag_requires_both() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_LEMMA_POSTAG" name="lemma and postag must both hold">
        <pattern><marker><token>le</token></marker></pattern>
        <disambig action="replace"><match postag="D f s">le</match></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = AnalyzedSentence::new("test", 0, 5);
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("le", 0, 2)).with_readings(vec![
                AnalyzedToken::new("le", 0, 2)
                    .with_pos_tags(vec!["D f s".to_string()])
                    .with_lemma("le"),
                AnalyzedToken::new("le", 0, 2)
                    .with_pos_tags(vec!["D f p".to_string()])
                    .with_lemma("le"),
            ]),
            AnalyzedTokenReadings::new(AnalyzedToken::new("x", 3, 4)).with_readings(vec![
                AnalyzedToken::new("x", 3, 4).with_pos_tags(vec!["NC".to_string()]),
            ]),
        ]);
        disambig.disambiguate(&mut sentence);
        assert_eq!(
            readings_of(&sentence.tokens()[0]),
            vec![(Some("le"), "D f s".to_string())],
            "the lemma filter must also require the literal postag before the regexp pass"
        );
    }

    #[test]
    fn match_action_on_a_readingless_token_changes_nothing() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_EMPTY" name="match on a readingless token">
        <pattern><marker><token>le</token></marker></pattern>
        <disambig action="replace"><match postag="R.*" postag_regexp="yes"/></disambig>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = AnalyzedSentence::new("test", 0, 5);
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("le", 0, 2)).with_readings(Vec::new()),
            AnalyzedTokenReadings::new(AnalyzedToken::new("x", 3, 4)).with_readings(vec![
                AnalyzedToken::new("x", 3, 4).with_pos_tags(vec!["NC".to_string()]),
            ]),
        ]);
        disambig.disambiguate(&mut sentence);
        assert!(
            sentence.tokens()[0].readings().is_empty(),
            "a token without readings cannot donate a source reading, so it stays empty"
        );
    }

    #[test]
    fn debug_helpers_report_absent_rules() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="LE_ONLY" name="the only rule">
        <pattern><marker><token>le</token></marker></pattern>
        <disambig postag="PRON"/>
    </rule>
</rules>"#;
        let disambig = XmlDisambiguator::from_xml(xml).unwrap();
        let mut sentence = le_sentence();
        let before = readings_signature(&sentence);
        disambig.apply_rule(&mut sentence, 99);
        assert_eq!(
            readings_signature(&sentence),
            before,
            "an out-of-range rule index must be a no-op"
        );
        let (untouched, examples) = disambig.rule_examples(99);
        assert!(untouched.is_empty());
        assert!(examples.is_empty());
        assert_eq!(disambig.rule_debug(99), "");
        assert_eq!(disambig.first_index_of_rule_id("LE_ONLY"), Some(0));
        assert_eq!(disambig.first_index_of_rule_id("ABSENT"), None);
        assert!(
            disambig
                .rule_debug(0)
                .starts_with(&format!("{:?}", Some("LE_ONLY")))
        );
        let mut probe = le_sentence();
        assert_eq!(
            disambig.debug_match_rule(&mut probe, 0),
            vec![vec![0]],
            "the marker-only pattern matches the first token"
        );
        assert_eq!(
            disambig.debug_match_rule(&mut probe, 99),
            Vec::<Vec<usize>>::new()
        );
        let pattern_text = disambig.rule_pattern_debug(0);
        assert!(pattern_text.contains("\"le\""), "{pattern_text}");
        assert_eq!(disambig.rule_pattern_debug(99), "");
    }
}
