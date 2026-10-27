use crate::parser::*;
use crate::types::*;
use rayon::prelude::*;

const REGEX_CACHE_SHARDS: usize = 16;

pub type SharedRegex = std::sync::Arc<regex::Regex>;
pub type SharedFancyRegex = std::sync::Arc<fancy_regex::Regex>;
type RegexCacheShard = std::sync::Mutex<std::collections::HashMap<String, Option<SharedRegex>>>;
type FancyRegexCacheShard =
    std::sync::Mutex<std::collections::HashMap<String, Option<SharedFancyRegex>>>;

fn is_property_sequence(regexp: &str) -> bool {
    regexp.contains("\\p{Lu}") || regexp.contains("\\p{Ll}")
}

fn shard_vec<T>() -> Vec<T>
where
    T: Default,
{
    (0..REGEX_CACHE_SHARDS).map(|_| T::default()).collect()
}

fn shard_index(pattern: &str, shards: usize) -> usize {
    let mut h: u64 = 14_695_981_039_346_656_037;
    for b in pattern.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1_099_511_628_211);
    }
    (h % shards as u64) as usize
}

fn plain_regex(pattern: &str) -> Option<regex::Regex> {
    regex::Regex::new(pattern).ok()
}

fn plain_fancy(pattern: &str) -> Option<fancy_regex::Regex> {
    fancy_regex::Regex::new(pattern).ok()
}

pub fn shared_cached_regex(pattern: &str) -> Option<SharedRegex> {
    static PLAIN: std::sync::OnceLock<Vec<RegexCacheShard>> = std::sync::OnceLock::new();
    let shards = PLAIN.get_or_init(|| {
        register_regex_releaser(&PLAIN, || {
            PLAIN
                .get()
                .map(|v| v.iter().for_each(|s| s.lock().unwrap().clear()));
        });
        shard_vec()
    });
    lookup_or_build(shards, pattern, plain_regex)
}

pub fn shared_cached_regex_ci(pattern: &str) -> Option<SharedRegex> {
    static CI: std::sync::OnceLock<Vec<RegexCacheShard>> = std::sync::OnceLock::new();
    let shards = CI.get_or_init(|| {
        register_regex_releaser(&CI, || {
            CI.get()
                .map(|v| v.iter().for_each(|s| s.lock().unwrap().clear()));
        });
        shard_vec()
    });
    lookup_or_build(shards, pattern, crate::java_case_fold::compile_java_ci)
}

pub fn shared_cached_fancy_regex(pattern: &str) -> Option<SharedFancyRegex> {
    static FANCY: std::sync::OnceLock<Vec<FancyRegexCacheShard>> = std::sync::OnceLock::new();
    let shards = FANCY.get_or_init(|| {
        register_regex_releaser(&FANCY, || {
            FANCY
                .get()
                .map(|v| v.iter().for_each(|s| s.lock().unwrap().clear()));
        });
        shard_vec()
    });
    lookup_or_build(shards, pattern, plain_fancy)
}

fn register_regex_releaser<T>(cell: &'static std::sync::OnceLock<T>, clear: fn()) {
    og_core::releasable::register_releaser(
        cell as *const std::sync::OnceLock<T> as usize,
        std::sync::Arc::new(move |_| {
            clear();
            0
        }),
    );
}

fn lookup_or_build<T>(
    shards: &'static [std::sync::Mutex<
        std::collections::HashMap<String, Option<std::sync::Arc<T>>>,
    >],
    pattern: &str,
    build: fn(&str) -> Option<T>,
) -> Option<std::sync::Arc<T>>
where
    T: Send + Sync + 'static,
{
    let shard = &shards[shard_index(pattern, REGEX_CACHE_SHARDS)];
    if let Some(hit) = shard.lock().unwrap().get(pattern) {
        return hit.clone();
    }
    let built = build(pattern).map(std::sync::Arc::new);
    shard
        .lock()
        .unwrap()
        .insert(pattern.to_string(), built.clone());
    built
}

pub struct XmlCompiler;

impl XmlCompiler {
    pub fn new() -> Self {
        Self
    }

    fn cached_regex(&self, pattern: &str) -> Option<SharedRegex> {
        shared_cached_regex(pattern)
    }

    fn cached_regex_ci(&self, pattern: &str) -> Option<SharedRegex> {
        shared_cached_regex_ci(pattern)
    }

    fn cached_fancy_regex(&self, pattern: &str) -> Option<SharedFancyRegex> {
        shared_cached_fancy_regex(pattern)
    }

    fn anchor_regex(pattern: &str) -> String {
        let trimmed = pattern.trim();
        if trimmed.starts_with('^') && trimmed.ends_with('$') {
            return trimmed.to_string();
        }
        format!("^(?:{})$", trimmed)
    }
    pub fn compile_file(&self, xml: &str) -> Result<CompiledRuleSet, XmlParseError> {
        self.compile_file_with_base(xml, None)
    }

    pub fn compile_file_with_base(
        &self,
        xml: &str,
        base_dir: Option<&std::path::Path>,
    ) -> Result<CompiledRuleSet, XmlParseError> {
        let parser = XmlRuleParser::new();
        let rule_file = parser.parse_with_base(xml, base_dir)?;

        let mut compiled = CompiledRuleSet {
            rules: Vec::new(),
            categories: rule_file.categories.clone(),
            unifications: rule_file
                .unifications
                .iter()
                .map(|uni| CompiledUnification {
                    feature: uni.feature.clone(),
                    equivalences: uni
                        .equivalences
                        .iter()
                        .filter_map(|eq| {
                            let pattern = if eq.exact {
                                format!(r"\A(?:{})\z", regex::escape(&eq.postag))
                            } else {
                                format!(r"\A(?:{})\z", eq.postag)
                            };
                            PosMatcher::new_anchored(&pattern).map(|re| (eq.eq_type.clone(), re))
                        })
                        .collect(),
                })
                .collect(),
        };

        let mut compiled_rules: Vec<CompiledRule> = rule_file
            .rules
            .par_iter()
            .map(|rule| self.compile_rule(rule))
            .collect();

        let grouped: Vec<&XmlRule> = rule_file
            .rule_groups
            .iter()
            .flat_map(|group| group.rules.iter())
            .collect();
        let grouped_compiled: Vec<CompiledRule> = grouped
            .par_iter()
            .map(|rule| self.compile_rule(rule))
            .collect();
        let mut grouped_iter = grouped_compiled.into_iter();
        for group in &rule_file.rule_groups {
            for (i, mut compiled_rule) in grouped_iter.by_ref().take(group.rules.len()).enumerate()
            {
                if compiled_rule.id.is_empty()
                    && let Some(ref group_id) = group.id
                {
                    compiled_rule.id = group_id.clone();
                    if compiled_rule.sub_id.is_none() {
                        compiled_rule.sub_id = Some(format!("{}", i + 1));
                    }
                }
                if group.default_on == Some(false) {
                    compiled_rule.default_on = false;
                }
                if compiled_rule.issue_type == "grammar"
                    && let Some(ref git) = group.issue_type
                {
                    compiled_rule.issue_type = git.clone();
                }
                compiled_rules.push(compiled_rule);
            }
        }
        for rule in &mut compiled_rules {
            rule.unifications = compiled.unifications.clone();
        }
        compiled.rules = compiled_rules;

        Ok(compiled)
    }

    fn compile_rule(&self, rule: &XmlRule) -> CompiledRule {
        CompiledRule {
            id: rule.id.clone(),
            sub_id: rule.sub_id.clone(),
            name: rule.name.clone(),
            description: rule.description.clone(),
            source_file: None,
            picky: rule.tags.iter().any(|t| t == "picky"),
            category: rule.category.clone(),
            antipatterns: rule
                .antipatterns
                .iter()
                .map(|ap| {
                    let pat_cs = ap.case_sensitive;
                    CompiledPattern {
                        marker_elem_start: ap.marker_elem_start,
                        marker_elem_end: ap.marker_elem_end,
                        unify_groups: ap.unify_groups.clone(),
                        tokens: ap
                            .tokens
                            .iter()
                            .map(|t| self.compile_token_with_default(t, pat_cs))
                            .collect(),
                        elements: ap
                            .elements
                            .iter()
                            .map(|e| match e {
                                XmlPatternElement::Token(t) => CompiledPatternElement::Token(
                                    Box::new(self.compile_token_with_default(t, pat_cs)),
                                ),
                                XmlPatternElement::OrGroup(g) => CompiledPatternElement::OrGroup(
                                    g.alternatives
                                        .iter()
                                        .map(|t| self.compile_token_with_default(t, pat_cs))
                                        .collect(),
                                ),
                                XmlPatternElement::AndGroup(g) => CompiledPatternElement::AndGroup(
                                    g.constraints
                                        .iter()
                                        .map(|t| self.compile_token_with_default(t, pat_cs))
                                        .collect(),
                                ),
                            })
                            .collect(),
                        case_sensitive: pat_cs,
                        marker_start: None,
                        marker_end: None,
                        has_structured_elements: ap.elements.iter().any(|e| {
                            matches!(
                                e,
                                XmlPatternElement::OrGroup(_) | XmlPatternElement::AndGroup(_)
                            )
                        }),
                        raw_pos: ap.raw_pos,
                    }
                })
                .collect(),
            pattern: {
                let pat_cs = rule.pattern.case_sensitive;
                CompiledPattern {
                    marker_elem_start: rule.pattern.marker_elem_start,
                    marker_elem_end: rule.pattern.marker_elem_end,
                    unify_groups: rule.pattern.unify_groups.clone(),
                    tokens: rule
                        .pattern
                        .tokens
                        .iter()
                        .map(|t| self.compile_token_with_default(t, pat_cs))
                        .collect(),
                    elements: rule
                        .pattern
                        .elements
                        .iter()
                        .map(|e| match e {
                            XmlPatternElement::Token(t) => CompiledPatternElement::Token(Box::new(
                                self.compile_token_with_default(t, pat_cs),
                            )),
                            XmlPatternElement::OrGroup(g) => CompiledPatternElement::OrGroup(
                                g.alternatives
                                    .iter()
                                    .map(|t| self.compile_token_with_default(t, pat_cs))
                                    .collect(),
                            ),
                            XmlPatternElement::AndGroup(g) => CompiledPatternElement::AndGroup(
                                g.constraints
                                    .iter()
                                    .map(|t| self.compile_token_with_default(t, pat_cs))
                                    .collect(),
                            ),
                        })
                        .collect(),
                    case_sensitive: pat_cs,
                    marker_start: rule.pattern.marker_start,
                    marker_end: rule.pattern.marker_end,
                    has_structured_elements: rule.pattern.elements.iter().any(|e| {
                        matches!(
                            e,
                            XmlPatternElement::OrGroup(_) | XmlPatternElement::AndGroup(_)
                        )
                    }),
                    raw_pos: rule.pattern.raw_pos,
                }
            },
            message: rule.message.clone(),
            short_message: rule.short_message.clone(),
            regexp: rule.rule_regexp.as_ref().map(|rr| {
                let pattern = if rr.exact {
                    rr.text.clone()
                } else {
                    replace_spaces_in_regex(&rr.text)
                };
                let pattern = split_variable_lookbehind(&pattern);
                let (regex, fancy_regex) = if rr.case_sensitive {
                    let plain = self.cached_regex(&pattern);
                    let fancy = if plain.is_none() {
                        self.cached_fancy_regex(&pattern)
                    } else {
                        None
                    };
                    (plain, fancy)
                } else {
                    let plain = self.cached_regex_ci(&pattern);
                    let fancy = if plain.is_none() {
                        self.cached_fancy_regex(&format!("(?i){pattern}"))
                    } else {
                        None
                    };
                    (plain, fancy)
                };
                CompiledRuleRegexp {
                    regex,
                    fancy_regex,
                    mark: rr.mark,
                }
            }),
            suggestions: rule
                .suggestions
                .iter()
                .map(|s| CompiledSuggestion {
                    text: s.text.clone(),
                    parts: s.parts.clone(),
                    suppress_misspelled: s.suppress_misspelled,
                })
                .collect(),
            examples: rule.examples.clone(),
            unifications: Vec::new(),
            url: rule.url.clone(),
            default_on: rule.default_on,
            deprecated: rule.deprecated,
            filter: rule.filter.clone(),
            issue_type: rule
                .issue_type
                .clone()
                .or_else(|| rule.category.issue_type.clone())
                .unwrap_or_else(|| "grammar".to_string()),
            suppress_misspelled: rule.suppress_misspelled,
        }
    }

    pub fn compile_token(&self, token: &XmlPatternToken) -> CompiledPatternToken {
        self.compile_token_with_default(token, false)
    }

    pub fn compile_token_with_default(
        &self,
        token: &XmlPatternToken,
        pattern_case_sensitive: bool,
    ) -> CompiledPatternToken {
        let effective_case_sensitive = if token.case_sensitive_set {
            token.case_sensitive
        } else {
            pattern_case_sensitive
        };
        let resolved: XmlPatternToken = {
            let mut t = token.clone();
            t.case_sensitive = effective_case_sensitive;
            t
        };
        let token = &resolved;
        let is_reference = token.match_no.is_some_and(|no| {
            token
                .text
                .as_deref()
                .is_some_and(|t| t.contains(&format!("\\{no}")))
        });
        let raw_pattern = if token.regexp.as_deref() == Some("yes") {
            token.text.clone()
        } else {
            token.regexp.clone()
        };
        let decoded_pattern = raw_pattern.as_deref().map(decode_surrogate_pairs);
        let compiled_re = if is_reference {
            None
        } else {
            decoded_pattern.as_ref().and_then(|p| {
                let anchored = Self::anchor_regex(p);
                self.cached_regex(&anchored)
            })
        };
        let compiled_re_ci = if is_reference {
            None
        } else {
            decoded_pattern
                .as_ref()
                .filter(|p| !is_property_sequence(p))
                .and_then(|p| {
                    let anchored = Self::anchor_regex(p);
                    self.cached_regex_ci(&anchored)
                })
        };
        let postag_source = if token.postag_regexp.as_deref() == Some("yes") {
            token.postag.clone()
        } else {
            token.postag_regexp.clone()
        };
        let postag_re = postag_source
            .as_ref()
            .and_then(|t| self.cached_regex(&Self::anchor_regex(t)));
        let postag_fancy = if postag_re.is_none() {
            postag_source
                .as_ref()
                .and_then(|t| self.cached_fancy_regex(&Self::anchor_regex(t)))
        } else {
            None
        };

        let text_value = if is_reference {
            token.text.clone()
        } else if token.regexp.as_deref() == Some("yes") {
            None
        } else {
            token.text.clone()
        };

        CompiledPatternToken {
            unify_neutral: token.unify_neutral,
            is_regexp: token.regexp.as_deref() == Some("yes"),
            text: text_value,
            compiled_regexp: compiled_re.clone(),
            compiled_regexp_ci: compiled_re_ci,
            compiled_regexp_fancy: if is_reference {
                None
            } else {
                self.compile_fancy(compiled_re.is_none(), token)
            },
            postag: token.postag.clone(),
            compiled_postag_regexp: postag_re,
            compiled_postag_regexp_fancy: postag_fancy,
            negate: token.negate,
            negate_pos: token.negate_pos,
            case_sensitive: token.case_sensitive,
            inflected: token.inflected,
            min: token.min,
            max: token.max,
            skip: token.skip,
            exceptions: token
                .exceptions
                .iter()
                .map(|e| self.compile_exception_with_inherited(e, effective_case_sensitive))
                .collect(),
            space_before: token.space_before.clone(),
            chunk: token.chunk.clone(),
            chunk_re: token.chunk_re.as_ref().and_then(|r| {
                let anchored = Self::anchor_regex(r);
                self.cached_regex(&anchored)
            }),
            match_no: token.match_no,
            match_no_case_conversion: token.match_no_case_conversion.clone(),
            match_no_postag: token.match_no_postag.clone(),
            match_no_postag_regexp: token.match_no_postag_regexp,
            match_no_postag_replace: token.match_no_postag_replace.clone(),
            match_no_setpos: token.match_no_setpos,
            match_no_regexp_replace: token.match_no_regexp_replace.clone(),
            compiled_match_no_regex: token
                .match_no_regexp_match
                .as_deref()
                .and_then(|p| self.cached_regex(p)),
        }
    }

    pub fn compile_element(&self, element: &XmlPatternElement) -> CompiledPatternElement {
        match element {
            XmlPatternElement::Token(t) => {
                CompiledPatternElement::Token(Box::new(self.compile_token(t)))
            }
            XmlPatternElement::OrGroup(g) => CompiledPatternElement::OrGroup(
                g.alternatives
                    .iter()
                    .map(|t| self.compile_token(t))
                    .collect(),
            ),
            XmlPatternElement::AndGroup(g) => CompiledPatternElement::AndGroup(
                g.constraints
                    .iter()
                    .map(|t| self.compile_token(t))
                    .collect(),
            ),
        }
    }

    fn compile_exception_with_inherited(
        &self,
        exc: &XmlException,
        inherited_case_sensitive: bool,
    ) -> CompiledException {
        let effective_case_sensitive = if exc.case_sensitive_set {
            exc.case_sensitive
        } else {
            inherited_case_sensitive
        };
        let exception_regex_source = if exc.regexp.as_deref() == Some("yes") {
            exc.text.as_deref()
        } else {
            exc.regexp.as_deref()
        };
        let compiled_re = exception_regex_source.and_then(|t| {
            let anchored = Self::anchor_regex(t);
            if effective_case_sensitive || is_property_sequence(t) {
                self.cached_regex(&anchored)
            } else {
                self.cached_regex_ci(&anchored)
            }
        });
        let compiled_postag_re = if exc.postag_regexp.as_deref() == Some("yes") {
            exc.postag
                .as_ref()
                .and_then(|t| self.cached_regex(&Self::anchor_regex(t)))
        } else {
            None
        };
        let compiled_postag_fancy = if compiled_postag_re.is_none() {
            exc.postag
                .as_ref()
                .filter(|_| exc.postag_regexp.as_deref() == Some("yes"))
                .and_then(|t| self.cached_fancy_regex(&Self::anchor_regex(t)))
        } else {
            None
        };
        CompiledException {
            text: exc.text.clone(),
            compiled_regexp: compiled_re.clone(),
            compiled_regexp_fancy: self.compile_fancy_exception(compiled_re.is_none(), exc),
            postag: exc.postag.clone(),
            compiled_postag_regexp: compiled_postag_re,
            compiled_postag_regexp_fancy: compiled_postag_fancy,
            negate: exc.negate,
            negate_pos: exc.negate_pos,
            inflected: exc.inflected,
            case_sensitive: effective_case_sensitive,
            scope: exc.scope.clone(),
            space_before: exc.space_before,
        }
    }

    fn compile_fancy(
        &self,
        needed: bool,
        token: &crate::types::XmlPatternToken,
    ) -> Option<SharedFancyRegex> {
        if !needed {
            return None;
        }
        let pattern = if token.regexp.as_deref() == Some("yes") {
            token.text.clone()
        } else {
            token.regexp.clone()
        }?;
        let anchored = Self::anchor_regex(&pattern);
        let final_pattern = if token.case_sensitive {
            anchored
        } else {
            format!("(?i){anchored}")
        };
        self.cached_fancy_regex(&final_pattern)
    }

    fn compile_fancy_exception(
        &self,
        needed: bool,
        exc: &XmlException,
    ) -> Option<SharedFancyRegex> {
        if !needed {
            return None;
        }
        let pattern = if exc.regexp.as_deref() == Some("yes") {
            exc.text.clone()
        } else {
            exc.regexp.clone()
        }?;
        let anchored = Self::anchor_regex(&pattern);
        let final_pattern = if exc.case_sensitive {
            anchored
        } else {
            format!("(?i){anchored}")
        };
        self.cached_fancy_regex(&final_pattern)
    }
}

const BACKSLASH: u8 = 0x5C;
const BACKSLASH_STR: &str = "\\";

const MAX_LOOKBEHIND_ALTERNATIVES: usize = 64;
const MAX_LOOKBEHIND_REPEAT: usize = 6;

fn parse_lookbehind_atom(rest: &str) -> Option<(String, usize)> {
    let bytes = rest.as_bytes();
    let first = *bytes.first()?;
    let body_len = if first == b'[' {
        let mut k = 1;
        while k < bytes.len() {
            if bytes[k] == b'\\' {
                k += 1;
            } else if bytes[k] == b']' {
                break;
            }
            k += 1;
        }
        if k >= bytes.len() {
            return None;
        }
        k + 1
    } else if first == b'\\' {
        if bytes.len() < 2 {
            return None;
        }
        2
    } else if !matches!(
        first,
        b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'*' | b'+' | b'.' | b'|' | b'^' | b'$'
    ) {
        rest.chars().next()?.len_utf8()
    } else {
        return None;
    };
    Some((rest[..body_len].to_string(), body_len))
}

fn expand_bounded_negative_lookbehind(inner: &str) -> Option<String> {
    let mut atoms: Vec<(String, usize, usize)> = Vec::new();
    let bytes = inner.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let (body, len) = parse_lookbehind_atom(&inner[i..])?;
        i += len;
        let (min, max) = if i < bytes.len() && bytes[i] == b'?' {
            i += 1;
            (0, 1)
        } else if i < bytes.len() && (bytes[i] == b'*' || bytes[i] == b'+') {
            return None;
        } else if i < bytes.len() && bytes[i] == b'{' {
            let close = inner[i..].find('}')? + i;
            let spec = &inner[i + 1..close];
            let (lo, hi) = if let Some((l, r)) = spec.split_once(',') {
                let lo: usize = l.parse().ok()?;
                if r.is_empty() {
                    return None;
                }
                (lo, r.parse().ok()?)
            } else {
                let v: usize = spec.parse().ok()?;
                (v, v)
            };
            if hi < lo || hi > MAX_LOOKBEHIND_REPEAT {
                return None;
            }
            i = close + 1;
            (lo, hi)
        } else {
            (1, 1)
        };
        atoms.push((body, min, max));
    }
    if atoms.is_empty() {
        return None;
    }
    let mut total = 1usize;
    let mut has_empty = true;
    for (_, min, max) in &atoms {
        has_empty &= *min == 0;
        if max != min {
            total = total.saturating_mul(max - min + 1);
        }
    }
    if has_empty || !(2..=MAX_LOOKBEHIND_ALTERNATIVES).contains(&total) {
        return None;
    }
    let mut alternatives: Vec<String> = vec![String::new()];
    for (body, min, max) in &atoms {
        let mut next = Vec::with_capacity(alternatives.len() * (max - min + 1));
        for alt in &alternatives {
            for count in *min..=*max {
                let mut candidate = alt.clone();
                for _ in 0..count {
                    candidate.push_str(body);
                }
                next.push(candidate);
            }
        }
        alternatives = next;
        if alternatives.len() > MAX_LOOKBEHIND_ALTERNATIVES {
            return None;
        }
    }
    Some(alternatives.join("|"))
}

fn split_variable_lookbehind(pattern: &str) -> String {
    let bytes = pattern.as_bytes();
    let mut out = String::with_capacity(pattern.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'('
            && pattern[i..].starts_with("(?<!")
            && bytes.get(i + 4).copied().is_some_and(|b| b != b'?')
        {
            let mut depth = 1usize;
            let mut j = i + 4;
            while j < bytes.len() && depth > 0 {
                if bytes[j] == BACKSLASH {
                    j += 1;
                } else if bytes[j] == b'[' {
                    while j < bytes.len() && bytes[j] != b']' {
                        if bytes[j] == BACKSLASH {
                            j += 1;
                        }
                        j += 1;
                    }
                } else if bytes[j] == b'(' {
                    depth += 1;
                } else if bytes[j] == b')' {
                    depth -= 1;
                }
                j += 1;
            }
            let inner = &pattern[i + 4..j.saturating_sub(1)];
            let (effective_base, wrapper_captures) =
                if inner.starts_with('(') && inner.ends_with(')') && group_spans_inner(inner) {
                    (&inner[1..inner.len() - 1], count_capturing_groups(inner))
                } else {
                    (inner, 0)
                };
            let expanded_storage;
            let effective: &str = match expand_bounded_negative_lookbehind(effective_base) {
                Some(expanded) => {
                    expanded_storage = expanded;
                    expanded_storage.as_str()
                }
                None => effective_base,
            };
            let parts = split_top_level_alternatives(effective);
            if parts.len() > 1 && wrapper_captures <= 1 {
                let split_form: String = parts
                    .iter()
                    .map(|part| format!("(?<!{})", part.trim()))
                    .collect();
                if wrapper_captures == 1 {
                    out.push('(');
                    out.push_str(&split_form);
                    out.push(')');
                } else {
                    out.push_str(&split_form);
                }
                i = j;
                continue;
            }
        }
        let ch = pattern[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn group_spans_inner(inner: &str) -> bool {
    let bytes = inner.as_bytes();
    if bytes.first() != Some(&b'(') {
        return false;
    }
    let mut depth = 0usize;
    let mut k = 0usize;
    while k < bytes.len() {
        if bytes[k] == BACKSLASH {
            k += 1;
        } else if bytes[k] == b'[' {
            while k < bytes.len() && bytes[k] != b']' {
                if bytes[k] == BACKSLASH {
                    k += 1;
                }
                k += 1;
            }
        } else if bytes[k] == b'(' {
            depth += 1;
        } else if bytes[k] == b')' {
            depth -= 1;
            if depth == 0 {
                return k == bytes.len() - 1;
            }
        }
        k += 1;
    }
    false
}

fn count_capturing_groups(pattern: &str) -> usize {
    let bytes = pattern.as_bytes();
    let mut count = 0usize;
    let mut k = 0usize;
    while k < bytes.len() {
        if bytes[k] == BACKSLASH {
            k += 1;
        } else if bytes[k] == b'[' {
            while k < bytes.len() && bytes[k] != b']' {
                if bytes[k] == BACKSLASH {
                    k += 1;
                }
                k += 1;
            }
        } else if bytes[k] == b'(' && !pattern[k..].starts_with("(?") {
            count += 1;
        }
        k += 1;
    }
    count
}

fn split_top_level_alternatives(inner: &str) -> Vec<&str> {
    let bytes = inner.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut k = 0usize;
    while k < bytes.len() {
        if bytes[k] == BACKSLASH {
            k += 1;
        } else if bytes[k] == b'[' {
            while k < bytes.len() && bytes[k] != b']' {
                if bytes[k] == BACKSLASH {
                    k += 1;
                }
                k += 1;
            }
        } else if bytes[k] == b'(' {
            depth += 1;
        } else if bytes[k] == b')' {
            depth -= 1;
        } else if bytes[k] == b'|' && depth == 0 {
            parts.push(&inner[start..k]);
            start = k + 1;
        }
        k += 1;
    }
    parts.push(&inner[start..]);
    parts
}

fn decode_surrogate_pairs(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let bytes = pattern.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == BACKSLASH
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'u'
            && i + 12 <= bytes.len()
        {
            let hi = u32::from_str_radix(&pattern[i + 2..i + 6], 16)
                .ok()
                .filter(|v| (0xD800..0xDC00).contains(v));
            let lo_next = pattern.get(i + 6..i + 12).and_then(|rest| {
                if rest.starts_with(BACKSLASH_STR) && rest.as_bytes().get(1) == Some(&b'u') {
                    u32::from_str_radix(&rest[2..6], 16)
                        .ok()
                        .filter(|v| (0xDC00..0xE000).contains(v))
                } else {
                    None
                }
            });
            if let (Some(hi), Some(lo)) = (hi, lo_next) {
                let combined = 0x1_0000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                out.push_str(&format!("\\u{{{combined:X}}}"));
                i += 12;
                continue;
            }
        }
        if bytes[i] == BACKSLASH
            && i + 1 < bytes.len()
            && bytes[i + 1] >= 0x80
            && pattern.is_char_boundary(i + 1)
        {
            let len = pattern[i + 1..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&pattern[i + 1..i + 1 + len]);
            i += 1 + len;
            continue;
        }
        let len = pattern[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&pattern[i..i + len]);
        i += len;
    }
    out
}

fn replace_spaces_in_regex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_bracket = false;
    for c in s.chars() {
        if c == '[' {
            in_bracket = true;
        } else if c == ']' {
            in_bracket = false;
        }
        if c == ' ' && !in_bracket {
            out.push_str("(?:[\\s\u{00A0}\u{202F}]+)");
        } else {
            out.push(c);
        }
    }
    out
}

impl Default for XmlCompiler {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct CompiledRuleSet {
    pub rules: Vec<CompiledRule>,
    pub categories: Vec<XmlCategory>,
    pub unifications: Vec<CompiledUnification>,
}

#[derive(Debug, Clone)]
pub struct CompiledUnification {
    pub feature: String,
    pub equivalences: Vec<(String, PosMatcher)>,
}

#[derive(Debug, Clone)]
pub enum PosMatcher {
    Plain(SharedRegex),
    Fancy(SharedFancyRegex),
}

impl PosMatcher {
    pub fn new_anchored(pattern: &str) -> Option<Self> {
        regex::Regex::new(pattern)
            .ok()
            .map(|re| PosMatcher::Plain(std::sync::Arc::new(re)))
            .or_else(|| {
                fancy_regex::Regex::new(pattern)
                    .ok()
                    .map(|re| PosMatcher::Fancy(std::sync::Arc::new(re)))
            })
    }

    pub fn is_match(&self, text: &str) -> bool {
        match self {
            PosMatcher::Plain(re) => re.is_match(text),
            PosMatcher::Fancy(re) => re.is_match(text).unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub id: String,
    pub sub_id: Option<String>,
    pub name: String,
    pub description: String,
    pub source_file: Option<String>,
    pub category: XmlCategory,
    pub antipatterns: Vec<CompiledPattern>,
    pub pattern: CompiledPattern,
    pub regexp: Option<CompiledRuleRegexp>,
    pub message: String,
    pub short_message: Option<String>,
    pub suggestions: Vec<CompiledSuggestion>,
    pub examples: Vec<XmlExample>,
    pub unifications: Vec<CompiledUnification>,
    pub url: Option<String>,
    pub default_on: bool,
    pub deprecated: bool,
    pub filter: Option<crate::types::XmlFilter>,
    pub issue_type: String,
    pub suppress_misspelled: bool,
    pub picky: bool,
}

#[derive(Debug, Clone)]
pub struct CompiledRuleRegexp {
    pub regex: Option<SharedRegex>,
    pub fancy_regex: Option<SharedFancyRegex>,
    pub mark: usize,
}

#[derive(Debug, Clone)]
pub enum CompiledPatternElement {
    Token(Box<CompiledPatternToken>),
    OrGroup(Vec<CompiledPatternToken>),
    AndGroup(Vec<CompiledPatternToken>),
}

#[derive(Debug, Clone)]
pub struct CompiledPattern {
    pub tokens: Vec<CompiledPatternToken>,
    pub elements: Vec<CompiledPatternElement>,
    pub case_sensitive: bool,
    pub marker_start: Option<usize>,
    pub marker_end: Option<usize>,
    pub marker_elem_start: Option<usize>,
    pub marker_elem_end: Option<usize>,
    pub has_structured_elements: bool,
    pub unify_groups: Vec<crate::types::XmlUnifyGroup>,
    pub raw_pos: bool,
}

#[derive(Debug, Clone)]
pub struct CompiledPatternToken {
    pub unify_neutral: bool,
    pub is_regexp: bool,
    pub text: Option<String>,
    pub compiled_regexp: Option<SharedRegex>,
    pub compiled_regexp_ci: Option<SharedRegex>,
    pub compiled_regexp_fancy: Option<SharedFancyRegex>,
    pub postag: Option<String>,
    pub compiled_postag_regexp: Option<SharedRegex>,
    pub compiled_postag_regexp_fancy: Option<SharedFancyRegex>,
    pub negate: bool,
    pub negate_pos: bool,
    pub case_sensitive: bool,
    pub inflected: bool,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub skip: i32,
    pub exceptions: Vec<CompiledException>,
    pub space_before: Option<String>,
    pub chunk: Option<String>,
    pub chunk_re: Option<SharedRegex>,
    pub match_no: Option<i32>,
    pub match_no_case_conversion: Option<String>,
    pub match_no_postag: Option<String>,
    pub match_no_postag_regexp: bool,
    pub match_no_postag_replace: Option<String>,
    pub match_no_setpos: bool,
    pub match_no_regexp_replace: Option<String>,
    pub compiled_match_no_regex: Option<SharedRegex>,
}

#[derive(Debug, Clone)]
pub struct CompiledException {
    pub text: Option<String>,
    pub compiled_regexp: Option<SharedRegex>,
    pub compiled_regexp_fancy: Option<SharedFancyRegex>,
    pub postag: Option<String>,
    pub compiled_postag_regexp: Option<SharedRegex>,
    pub compiled_postag_regexp_fancy: Option<SharedFancyRegex>,
    pub negate: bool,
    pub negate_pos: bool,
    pub inflected: bool,
    pub case_sensitive: bool,
    pub scope: Option<String>,
    pub space_before: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct CompiledSuggestion {
    pub text: String,
    pub parts: Vec<SuggestionPart>,
    pub suppress_misspelled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_negative_lookbehind_expands_to_fixed_width_conjunction() {
        let expanded = expand_bounded_negative_lookbehind("[A-Z\\$]-?[0-9\\.]{0,5}");
        assert_eq!(
            expanded.as_deref(),
            Some(
                "[A-Z\\$]|[A-Z\\$][0-9\\.]|[A-Z\\$][0-9\\.][0-9\\.]|[A-Z\\$][0-9\\.][0-9\\.][0-9\\.]|[A-Z\\$][0-9\\.][0-9\\.][0-9\\.][0-9\\.]|[A-Z\\$][0-9\\.][0-9\\.][0-9\\.][0-9\\.][0-9\\.]|[A-Z\\$]-|[A-Z\\$]-[0-9\\.]|[A-Z\\$]-[0-9\\.][0-9\\.]|[A-Z\\$]-[0-9\\.][0-9\\.][0-9\\.]|[A-Z\\$]-[0-9\\.][0-9\\.][0-9\\.][0-9\\.]|[A-Z\\$]-[0-9\\.][0-9\\.][0-9\\.][0-9\\.][0-9\\.]"
            )
        );
        assert_eq!(expand_bounded_negative_lookbehind("[A-Z]{2}"), None);
        assert_eq!(expand_bounded_negative_lookbehind("a*"), None);
        assert_eq!(expand_bounded_negative_lookbehind("a|b"), None);
        assert_eq!(expand_bounded_negative_lookbehind("a?"), None);
    }

    #[test]
    fn lookbehind_rule_regexp_compiles_and_matches_like_java() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rule id="LB" name="lb">
                <regexp case_sensitive='yes'>(?&lt;![A-Z\$]-?[0-9\.]{0,5})[0-9]{1,5}m</regexp>
                <message>m</message>
            </rule>
        </rules>"#;
        let rule_set = XmlCompiler::new().compile_file(xml).unwrap();
        let re = rule_set.rules[0].regexp.as_ref().unwrap();
        assert!(re.regex.is_some() || re.fancy_regex.is_some());
        let engine = re
            .fancy_regex
            .as_ref()
            .or(re.regex.as_ref().map(|_| unreachable!()));
        let f = engine.unwrap();
        assert!(f.is_match("needs $350m now").is_ok_and(|m| !m));
        assert!(f.is_match("about 350m users").is_ok_and(|m| m));
    }

    #[test]
    fn test_compile_simple_rule() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST" name="Test">
                    <pattern>
                        <token>hello</token>
                        <token>world</token>
                    </pattern>
                    <message>Use <suggestion>goodbye</suggestion></message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules.len(), 1);
        assert_eq!(result.rules[0].pattern.tokens.len(), 2);
        assert_eq!(result.categories.len(), 1);
        assert_eq!(result.rules[0].id, "TEST");
        assert_eq!(result.rules[0].name, "Test");
        assert_eq!(result.rules[0].message, "Use goodbye");
        assert_eq!(result.rules[0].suggestions.len(), 1);
        assert_eq!(result.rules[0].suggestions[0].text, "goodbye");
    }

    #[test]
    fn test_compile_literal_tokens_have_text() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="LIT" name="Literal">
                    <pattern>
                        <token>foo</token>
                        <token>bar</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(
            result.rules[0].pattern.tokens[0].text.as_deref(),
            Some("foo")
        );
        assert_eq!(
            result.rules[0].pattern.tokens[1].text.as_deref(),
            Some("bar")
        );
        assert!(result.rules[0].pattern.tokens[0].compiled_regexp.is_none());
        assert!(result.rules[0].pattern.tokens[1].compiled_regexp.is_none());
    }

    #[test]
    fn test_compile_regexp_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="RE" name="Regexp">
                    <pattern>
                        <token regexp="yes">foo|bar</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert!(token.text.is_none());
        assert!(token.compiled_regexp.is_some());
        assert!(token.compiled_regexp.as_ref().unwrap().is_match("foo"));
        assert!(token.compiled_regexp.as_ref().unwrap().is_match("bar"));
        assert!(!token.compiled_regexp.as_ref().unwrap().is_match("baz"));
    }

    #[test]
    fn test_compile_regexp_token_digits() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="DIGIT" name="Digit">
                    <pattern>
                        <token regexp="yes">\d+</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert!(token.compiled_regexp.is_some());
        assert!(token.compiled_regexp.as_ref().unwrap().is_match("123"));
        assert!(!token.compiled_regexp.as_ref().unwrap().is_match("abc"));
    }

    #[test]
    fn test_compile_negate_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="NEG" name="Negate">
                    <pattern>
                        <token negate="yes">bad</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert!(result.rules[0].pattern.tokens[0].negate);
        assert_eq!(
            result.rules[0].pattern.tokens[0].text.as_deref(),
            Some("bad")
        );
    }

    #[test]
    fn test_compile_case_sensitive_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="CS" name="Case sensitive">
                    <pattern>
                        <token case_sensitive="yes">Hello</token>
                        <token case_sensitive="no">world</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert!(result.rules[0].pattern.tokens[0].case_sensitive);
        assert!(!result.rules[0].pattern.tokens[1].case_sensitive);
    }

    #[test]
    fn test_compile_min_max_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="MM" name="MinMax">
                    <pattern>
                        <token min="0" max="3">optional</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules[0].pattern.tokens[0].min, Some(0));
        assert_eq!(result.rules[0].pattern.tokens[0].max, Some(3));
    }

    #[test]
    fn test_compile_min_two_expands_to_repeated_tokens() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="MM2" name="MinTwo">
                    <pattern>
                        <token>de</token>
                        <marker>
                            <token min="2" skip="1">x</token>
                        </marker>
                        <token>y</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules[0].pattern.tokens.len(), 4);
        assert_eq!(
            result.rules[0].pattern.tokens[0].text.as_deref(),
            Some("de")
        );
        assert_eq!(result.rules[0].pattern.tokens[1].text.as_deref(), Some("x"));
        assert_eq!(result.rules[0].pattern.tokens[2].text.as_deref(), Some("x"));
        assert_eq!(result.rules[0].pattern.tokens[3].text.as_deref(), Some("y"));
        assert_eq!(result.rules[0].pattern.tokens[1].min, None);
        assert_eq!(result.rules[0].pattern.tokens[1].skip, 1);
        assert_eq!(result.rules[0].pattern.marker_start, Some(1));
        assert_eq!(result.rules[0].pattern.marker_end, Some(3));
        assert_eq!(result.rules[0].pattern.elements.len(), 4);
    }

    #[test]
    fn test_compile_min_two_in_antipattern_expands() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="APM" name="AntipatternMin">
                    <antipattern>
                        <token min="3">-</token>
                    </antipattern>
                    <pattern>
                        <token>a</token>
                        <token>b</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        let ap = &result.rules[0].antipatterns[0];
        assert_eq!(ap.tokens.len(), 3);
        assert_eq!(
            ap.tokens
                .iter()
                .filter(|t| t.text.as_deref() == Some("-"))
                .count(),
            3
        );
        assert_eq!(ap.elements.len(), 3);
    }

    #[test]
    fn test_phrase_siblings_in_rulegroup_share_sub_id() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <phrases>
                <phrase id="TIMEWORD">
                    <token>yesterday</token>
                </phrase>
                <phrase id="LASTWEEK">
                    <token>last</token>
                    <token>week</token>
                </phrase>
                <phrase id="ALL_PAST">
                    <includephrases>
                        <phraseref idref="TIMEWORD"/>
                        <phraseref idref="LASTWEEK"/>
                    </includephrases>
                </phrase>
            </phrases>
            <category id="GRAMMAR" name="Grammar">
                <rulegroup id="GRP" name="Group">
                    <rule name="One">
                        <pattern>
                            <phraseref idref="ALL_PAST"/>
                            <token>went</token>
                        </pattern>
                        <message>Msg</message>
                        <example correction=""><marker>yesterday went</marker></example>
                    </rule>
                    <rule name="Two">
                        <pattern>
                            <token>other</token>
                        </pattern>
                        <message>Msg2</message>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules.len(), 3);
        assert_eq!(result.rules[0].id, "GRP");
        assert_eq!(result.rules[0].sub_id.as_deref(), Some("1"));
        assert_eq!(result.rules[0].pattern.tokens.len(), 2);
        assert_eq!(
            result.rules[1].sub_id.as_deref(),
            Some("1"),
            "phrase sibling must share the original rule's subId like createRules"
        );
        assert_eq!(result.rules[1].pattern.tokens.len(), 3);
        assert_eq!(result.rules[2].sub_id.as_deref(), Some("2"));
    }

    #[test]
    fn test_compile_skip_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="SKIP" name="Skip">
                    <pattern>
                        <token skip="5">first</token>
                        <token>last</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules[0].pattern.tokens[0].skip, 5);
        assert_eq!(result.rules[0].pattern.tokens[1].skip, 0);
    }

    #[test]
    fn test_compile_postag_constraint() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="POSTAG" name="Postag">
                    <pattern>
                        <token postag="JJ">big</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(
            result.rules[0].pattern.tokens[0].postag.as_deref(),
            Some("JJ")
        );
        assert!(
            result.rules[0].pattern.tokens[0]
                .compiled_postag_regexp
                .is_none()
        );
    }

    #[test]
    fn unification_equivalence_with_lookahead_falls_back_to_fancy() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="pl">
            <unification feature="number">
                <equivalence type="other">
                    <token postag="subst:(?!sg).*" postag_regexp="yes"/>
                </equivalence>
            </unification>
            <category id="GRAMMAR" name="Grammar">
                <rule id="UNI" name="unify lookahead">
                    <pattern>
                        <token>word</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();
        assert_eq!(result.unifications.len(), 1);
        let equivalences = &result.unifications[0].equivalences;
        assert_eq!(
            equivalences.len(),
            1,
            "lookahead equivalence must not be dropped"
        );
        let (_, matcher) = &equivalences[0];
        assert!(matcher.is_match("subst:pl"));
        assert!(!matcher.is_match("subst:sg"));
    }

    #[test]
    fn test_compile_postag_regexp_constraint() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="POSTRE" name="Postag regexp">
                    <pattern>
                        <token postag="NN.*" postag_regexp="yes">word</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert_eq!(token.postag.as_deref(), Some("NN.*"));
        assert!(token.compiled_postag_regexp.is_some());
        assert!(
            token
                .compiled_postag_regexp
                .as_ref()
                .unwrap()
                .is_match("NN")
        );
        assert!(
            token
                .compiled_postag_regexp
                .as_ref()
                .unwrap()
                .is_match("NNS")
        );
        assert!(
            !token
                .compiled_postag_regexp
                .as_ref()
                .unwrap()
                .is_match("VB")
        );
    }

    #[test]
    fn test_compile_exception_on_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EXC" name="Exception">
                    <pattern>
                        <token>word<exception regexp="skip_\w+" negate="yes" scope="next"/></token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert_eq!(token.exceptions.len(), 1);
        let exc = &token.exceptions[0];
        assert!(exc.negate);
        assert_eq!(exc.scope.as_deref(), Some("next"));
        assert!(exc.compiled_regexp.is_some());
        assert!(exc.compiled_regexp.as_ref().unwrap().is_match("skip_this"));
    }

    #[test]
    fn test_compile_multiple_suggestions() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="MULTI" name="Multi suggestion">
                    <pattern><token>wrong</token></pattern>
                    <message>Use <suggestion>opt1</suggestion> or <suggestion>opt2</suggestion> or <suggestion>opt3</suggestion>.</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules[0].suggestions.len(), 3);
        assert_eq!(result.rules[0].suggestions[0].text, "opt1");
        assert_eq!(result.rules[0].suggestions[1].text, "opt2");
        assert_eq!(result.rules[0].suggestions[2].text, "opt3");
    }

    #[test]
    fn test_compile_rule_group() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rulegroup id="GRP" name="Group">
                    <rule id="GR1" name="Grouped 1">
                        <pattern><token>alpha</token></pattern>
                        <message>M1</message>
                    </rule>
                    <rule id="GR2" name="Grouped 2">
                        <pattern><token>beta</token></pattern>
                        <message>M2</message>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules.len(), 2);
        assert_eq!(result.rules[0].id, "GR1");
        assert_eq!(
            result.rules[0].pattern.tokens[0].text.as_deref(),
            Some("alpha")
        );
        assert_eq!(result.rules[1].id, "GR2");
        assert_eq!(
            result.rules[1].pattern.tokens[0].text.as_deref(),
            Some("beta")
        );
    }

    #[test]
    fn test_compile_default_on_off() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="ON_RULE" name="On" default="on">
                    <pattern><token>a</token></pattern>
                    <message>M1</message>
                </rule>
                <rule id="OFF_RULE" name="Off" default="off">
                    <pattern><token>b</token></pattern>
                    <message>M2</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert!(result.rules[0].default_on);
        assert!(!result.rules[1].default_on);
    }

    #[test]
    fn test_compile_rulegroup_default_off_propagates_to_members() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rulegroup id="GRP_OFF" name="Off Group" default="off">
                    <rule name="A"><token>foo</token><message>bar</message></rule>
                    <rule name="B"><token>baz</token><message>qux</message></rule>
                </rulegroup>
                <rulegroup id="GRP_ON" name="On Group" default="on">
                    <rule name="C"><token>foo</token><message>bar</message></rule>
                </rulegroup>
            </category>
        </rules>"#;
        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();
        let off: Vec<&str> = result
            .rules
            .iter()
            .filter(|r| r.id == "GRP_OFF")
            .map(|r| if r.default_on { "on" } else { "off" })
            .collect();
        assert_eq!(off, vec!["off", "off"]);
        assert!(
            result
                .rules
                .iter()
                .any(|r| r.id == "GRP_ON" && r.default_on)
        );
    }

    #[test]
    fn test_compile_deprecated_rule() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="DEP" name="Deprecated" deprecated="yes">
                    <pattern><token>old</token></pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert!(result.rules[0].deprecated);
    }

    #[test]
    fn test_compile_category_preserved() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="STYLE" name="Style" description="Style issues" default="on">
                <rule id="STYLE_R" name="Style rule">
                    <pattern><token>test</token></pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.categories.len(), 1);
        assert_eq!(result.categories[0].id, "STYLE");
        assert_eq!(result.categories[0].name, "Style");
        assert_eq!(result.rules[0].category.id, "STYLE");
    }

    #[test]
    fn test_compile_pattern_case_sensitive() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="PC" name="Pattern case">
                    <pattern case_sensitive="yes">
                        <token>Test</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert!(result.rules[0].pattern.case_sensitive);
    }

    #[test]
    fn test_compile_examples_preserved() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EX" name="Examples">
                    <pattern><token>error</token></pattern>
                    <message>Fix <suggestion>correct</suggestion></message>
                    <example type="incorrect"><marker>error</marker> here.</example>
                    <example type="correct">correct here.</example>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules[0].examples.len(), 2);
        assert_eq!(
            result.rules[0].examples[0].example_type,
            XmlExampleType::Incorrect
        );
        assert_eq!(
            result.rules[0].examples[1].example_type,
            XmlExampleType::Correct
        );
    }

    #[test]
    fn test_compile_mixed_rules_and_groups() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="STANDALONE" name="Standalone">
                    <pattern><token>solo</token></pattern>
                    <message>Solo</message>
                </rule>
                <rulegroup id="GRP" name="Group">
                    <rule id="G1" name="G1">
                        <pattern><token>g1</token></pattern>
                        <message>G1</message>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules.len(), 2);
        assert_eq!(result.rules[0].id, "STANDALONE");
        assert_eq!(result.rules[1].id, "G1");
    }

    #[test]
    fn test_compile_inflected_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="INFL" name="Inflected">
                    <pattern>
                        <token inflected="yes">run</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert!(result.rules[0].pattern.tokens[0].inflected);
    }

    #[test]
    fn antipattern_examples_are_not_rule_examples() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="de">
            <category id="GRAMMAR" name="Grammar">
                <rule id="ANTI_EX" name="Antipattern examples">
                    <antipattern>
                        <token>für</token>
                        <example correction="">Für die italienische Mafia ist das heiliges Gebiet.</example>
                    </antipattern>
                    <pattern>
                        <token>das</token>
                        <token postag="ADJ:NOM:SIN:NEU:IND" postag_regexp="yes"/>
                    </pattern>
                    <message>Msg</message>
                    <example correction="X">Das <marker>heiliges</marker> Gebiet.</example>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules[0].examples.len(), 1);
        assert_eq!(
            result.rules[0].examples[0].text,
            "Das <marker>heiliges</marker> Gebiet."
        );
    }

    #[test]
    fn test_compile_self_closing_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="SELF" name="Self close">
                    <pattern>
                        <token negate="yes"/>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert!(token.negate);
        assert!(token.text.is_none());
        assert!(token.compiled_regexp.is_none());
    }

    #[test]
    fn test_compile_empty_pattern() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EMPTY" name="Empty">
                    <pattern></pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules[0].pattern.tokens.len(), 0);
    }

    #[test]
    fn test_compile_multiple_categories() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="CAT1" name="First">
                <rule id="R1" name="R1">
                    <pattern><token>a</token></pattern>
                    <message>M1</message>
                </rule>
            </category>
            <category id="CAT2" name="Second">
                <rule id="R2" name="R2">
                    <pattern><token>b</token></pattern>
                    <message>M2</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.categories.len(), 2);
        assert_eq!(result.rules.len(), 2);
        assert_eq!(result.rules[0].category.id, "CAT1");
        assert_eq!(result.rules[1].category.id, "CAT2");
    }

    #[test]
    fn test_compile_exception_with_postag() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EXC_POST" name="Exc postag">
                    <pattern>
                        <token>word<exception postag="NN" negate="yes" inflected="yes"/></token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        let exc = &result.rules[0].pattern.tokens[0].exceptions[0];
        assert_eq!(exc.postag.as_deref(), Some("NN"));
        assert!(exc.negate);
        assert!(exc.inflected);
    }

    #[test]
    fn test_compile_complex_regexp_alternation() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="ALT" name="Alternation">
                    <pattern>
                        <token regexp="yes">(?:their|there|they're)</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert!(token.text.is_none());
        let re = token.compiled_regexp.as_ref().unwrap();
        assert!(re.is_match("their"));
        assert!(re.is_match("there"));
        assert!(re.is_match("they're"));
        assert!(!re.is_match("thier"));
    }

    #[test]
    fn test_compile_multiple_exceptions() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="MULTI_EXC" name="Multi exc">
                    <pattern>
                        <token>base<exception negate="yes"/><exception scope="next"/></token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert_eq!(token.exceptions.len(), 2);
        assert!(token.exceptions[0].negate);
        assert_eq!(token.exceptions[1].scope.as_deref(), Some("next"));
    }

    #[test]
    fn test_compile_no_rules_file() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="EMPTY" name="Empty">
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules.len(), 0);
        assert_eq!(result.categories.len(), 1);
    }

    #[test]
    fn test_compile_postag_regexp_matches() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="POSTRE2" name="Postag regexp 2">
                    <pattern>
                        <token postag="^VB(Z|S)?$" postag_regexp="yes">verb</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        let re = token.compiled_postag_regexp.as_ref().unwrap();
        assert!(re.is_match("VB"));
        assert!(re.is_match("VBZ"));
        assert!(re.is_match("VBS"));
        assert!(!re.is_match("VBD"));
    }

    #[test]
    fn test_compile_all_token_attrs() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="ALL" name="All attrs">
                    <pattern>
                        <token regexp="yes" negate="yes" case_sensitive="yes" inflected="yes" postag="NN.*" postag_regexp="yes" min="1" max="3" skip="2">test_pattern</token>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert!(token.text.is_none());
        assert!(token.compiled_regexp.is_some());
        assert!(token.compiled_postag_regexp.is_some());
        assert!(token.negate);
        assert!(token.case_sensitive);
        assert!(token.inflected);
        assert_eq!(token.min, Some(1));
        assert_eq!(token.max, Some(3));
        assert_eq!(token.skip, 2);
        assert_eq!(token.postag.as_deref(), Some("NN.*"));
    }

    #[test]
    fn test_antipattern_token_text_capture() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rulegroup id="TESTGROUP" name="Test Group">
                    <antipattern>
                        <token inflected="yes">be</token>
                        <token regexp="yes">romeo|matt|mike</token>
                    </antipattern>
                    <rule id="TEST" name="Test">
                        <pattern>
                            <token>'m|'re</token>
                            <token>been</token>
                        </pattern>
                        <message>Msg</message>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;

        let compiler = XmlCompiler::new();
        let result = compiler.compile_file(xml).unwrap();

        assert_eq!(result.rules.len(), 1);
        let rule = &result.rules[0];
        assert!(
            !rule.antipatterns.is_empty(),
            "Rule should have inherited antipatterns from group"
        );

        let ap = &rule.antipatterns[0];
        assert_eq!(ap.tokens.len(), 2, "Antipattern should have 2 tokens");

        let tok0 = &ap.tokens[0];
        assert_eq!(
            tok0.text.as_deref(),
            Some("be"),
            "Token 0 should have text 'be'"
        );
        assert!(tok0.inflected, "Token 0 should be inflected");

        let tok1 = &ap.tokens[1];
        assert!(
            tok1.compiled_regexp.is_some(),
            "Token 1 should have compiled_regexp (text={:?}, regexp_attr={:?})",
            tok1.text,
            "yes"
        );
        assert!(tok1.compiled_regexp.as_ref().unwrap().is_match("romeo"));
        assert!(tok1.compiled_regexp.as_ref().unwrap().is_match("matt"));
        assert!(!tok1.compiled_regexp.as_ref().unwrap().is_match("other"));
    }
    #[test]
    fn test_postag_regexes_match_fully_like_java() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="FULLMATCH" name="Test">
                    <pattern>
                        <token postag="NN" postag_regexp="yes"/>
                    </pattern>
                    <message>Fix</message>
                </rule>
            </category>
        </rules>"#;
        let rule_set = XmlCompiler::new().compile_file(xml).unwrap();
        let token = &rule_set.rules[0].pattern.tokens[0];
        let re = token.compiled_postag_regexp.as_ref().unwrap();
        assert!(re.is_match("NN"));
        assert!(
            !re.is_match("NN:U"),
            "postag NN must not match NN:U (Java uses matches())"
        );
        assert!(!re.is_match("NNS"));
    }
}

#[test]
fn parse_lookbehind_atom_handles_classes_escapes_and_unicode() {
    assert_eq!(
        parse_lookbehind_atom("[a-z]\\."),
        Some(("[a-z]".to_string(), 5))
    );
    assert_eq!(parse_lookbehind_atom("[a"), None);
    assert_eq!(parse_lookbehind_atom("\\d+"), Some(("\\d".to_string(), 2)));
    assert_eq!(parse_lookbehind_atom("\\"), None);
    assert_eq!(parse_lookbehind_atom("üx"), Some(("ü".to_string(), 2)));
    assert_eq!(parse_lookbehind_atom("("), None);
    assert_eq!(parse_lookbehind_atom(""), None);
}

#[test]
fn expand_bounded_negative_lookbehind_rejects_unbounded_and_empty_forms() {
    assert_eq!(expand_bounded_negative_lookbehind("a{2,}"), None);
    assert_eq!(expand_bounded_negative_lookbehind("a{2,9}"), None);
    assert_eq!(expand_bounded_negative_lookbehind(""), None);
    assert_eq!(expand_bounded_negative_lookbehind("a{0,2}"), None);
    assert_eq!(
        expand_bounded_negative_lookbehind("[0-9]{1,2}"),
        Some("[0-9]|[0-9][0-9]".to_string())
    );
}

#[test]
fn split_variable_lookbehind_distributes_over_alternatives() {
    assert_eq!(split_variable_lookbehind("(?<!a|b)c"), "(?<!a)(?<!b)c");
    assert_eq!(split_variable_lookbehind("(?<!(a|b))c"), "((?<!a)(?<!b))c");
    assert_eq!(split_variable_lookbehind("(?<!\\d)x"), "(?<!\\d)x");
    assert_eq!(split_variable_lookbehind("(?<!(ab))x"), "(?<!(ab))x");
    assert_eq!(split_variable_lookbehind("plain"), "plain");
}

#[test]
fn lookbehind_helpers_classify_groups_and_alternatives() {
    assert!(group_spans_inner("(a|b)"));
    assert!(!group_spans_inner("a(b)"));
    assert!(!group_spans_inner("(a)b"));
    assert_eq!(count_capturing_groups("(a|(b))"), 2);
    assert_eq!(count_capturing_groups("(?:a)"), 0);
    assert_eq!(
        split_top_level_alternatives("a|b(c|d)|e"),
        vec!["a", "b(c|d)", "e"]
    );
}

#[test]
fn decode_surrogate_pairs_and_space_replacement_follow_java() {
    assert_eq!(decode_surrogate_pairs("\\uD83D\\uDE00"), "\\u{1F600}");
    assert_eq!(decode_surrogate_pairs("\\uD83Dnope"), "\\uD83Dnope");
    assert_eq!(decode_surrogate_pairs("\\ü"), "ü");
    assert_eq!(decode_surrogate_pairs("a"), "a");
    assert_eq!(
        replace_spaces_in_regex("a b[c d]"),
        "a(?:[\\s\u{00A0}\u{202F}]+)b[c d]"
    );
}

#[test]
fn regex_cache_release_clears_cached_compilations() {
    let first = shared_cached_regex("og-xml-release-probe-[0-9]+").unwrap();
    og_core::releasable::release_all();
    let second = shared_cached_regex("og-xml-release-probe-[0-9]+").unwrap();
    assert!(!std::sync::Arc::ptr_eq(&first, &second));
    assert!(second.is_match("og-xml-release-probe-42"));
}

#[test]
fn compiler_default_matches_new() {
    let xml = r#"<rules lang="en"><rule id="D" name="d"><pattern><token>t</token></pattern><message>m</message></rule></rules>"#;
    let rs = XmlCompiler::default().compile_file(xml).unwrap();
    assert_eq!(rs.rules.len(), 1);
}

#[cfg(test)]
mod surrogate_tests {
    use super::*;

    #[test]
    fn surrogate_pair_ranges_decode_and_compile() {
        let pat =
            "[\\ud83c\\udc00-\\ud83c\\udfff]+|[\\ud83d\\udc00-\\ud83d\\udfff]+|[\\u2600-\\u27ff]+";
        let decoded = decode_surrogate_pairs(pat);
        let re = regex::Regex::new(&format!("^(?:{decoded})$")).expect("must compile");
        assert!(!re.is_match("ich"));
        assert!(re.is_match("\u{1F600}"));
        assert!(re.is_match("\u{2600}"));
    }

    #[test]
    fn java_escape_of_non_ascii_punctuation_is_unescaped() {
        let pat = "[,:!;_\\.\\?\\-\\–…\\(\\[]|and|or|\\+|&|\\u2063";
        let decoded = decode_surrogate_pairs(pat);
        assert_eq!(decoded, "[,:!;_\\.\\?\\-–…\\(\\[]|and|or|\\+|&|\\u2063");
        let re = regex::Regex::new(&format!("^(?:{decoded})$")).expect("must compile");
        assert!(re.is_match("!"));
        assert!(re.is_match("\u{2063}"));
        assert!(!re.is_match("word"));
    }

    #[test]
    fn emoji_token_via_compiler_matches_only_emoji() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="de">
<category id="MISC" name="Misc">
<rule id="E" name="e">
<pattern>
<token regexp="yes">[\ud83c\udc00-\ud83c\udfff]+|[\ud83d\udc00-\ud83d\udfff]+|[\u2600-\u27ff]+</token>
</pattern>
<message>m</message>
</rule>
</category>
</rules>"#;
        let rs = XmlCompiler::new().compile_file(xml).unwrap();
        let tok = &rs.rules[0].pattern.tokens[0];
        assert!(tok.compiled_regexp.is_some(), "decoded regex must compile");
        let re = tok.compiled_regexp.as_ref().unwrap();
        assert!(!re.is_match("ich"));
        assert!(re.is_match("\u{1F600}"));
    }
    #[test]
    fn group_span_and_capturing_group_walks_handle_escapes_and_classes() {
        assert!(group_spans_inner("(a|b)"));
        assert!(!group_spans_inner("(a)b"));
        assert!(group_spans_inner("(a[b)]c)"));
        assert!(group_spans_inner("(\\)x)"));
        assert!(!group_spans_inner("(a(b)c)d"));
        assert_eq!(count_capturing_groups("a[b(c]"), 0);
        assert_eq!(count_capturing_groups("\\(a\\)"), 0);
        assert_eq!(count_capturing_groups("(a)[b](c)"), 2);
    }

    #[test]
    fn compile_element_compiles_token_or_and_and_groups() {
        let tok = |text: &str| {
            let mut t = XmlPatternToken::default();
            t.text = Some(text.to_string());
            t
        };
        let compiler = XmlCompiler::new();
        assert!(matches!(
            compiler.compile_element(&XmlPatternElement::Token(tok("a"))),
            CompiledPatternElement::Token(_)
        ));
        let or = compiler.compile_element(&XmlPatternElement::OrGroup(XmlOrGroup {
            alternatives: vec![tok("a"), tok("b")],
        }));
        assert!(matches!(or, CompiledPatternElement::OrGroup(_)));
        let and = compiler.compile_element(&XmlPatternElement::AndGroup(XmlAndGroup {
            constraints: vec![tok("c")],
        }));
        assert!(matches!(and, CompiledPatternElement::AndGroup(_)));
    }
}
