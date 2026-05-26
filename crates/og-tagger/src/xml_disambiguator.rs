use og_core::analyzed::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use og_core::checker::Disambiguator;
use og_xml::compiler::{CompiledPattern, CompiledPatternElement, CompiledPatternToken, XmlCompiler};
use og_xml::disambig_parser::DisambigXmlParser;
use og_xml::disambig_types::{CompiledDisambigRule, DisambigAction, DisambigRule};

/// XML-based disambiguator that loads disambiguation rules and applies them
/// to refine POS tag assignments.
pub struct XmlDisambiguator {
    rules: Vec<CompiledDisambigRule>,
}

impl XmlDisambiguator {
    /// Create a new disambiguator from disambiguation XML content
    pub fn from_xml(xml: &str) -> Result<Self, String> {
        let parser = DisambigXmlParser::new();
        let rule_set = parser.parse(xml).map_err(|e| e.to_string())?;

        let compiler = XmlCompiler::new();
        let compiled_rules: Vec<CompiledDisambigRule> = rule_set
            .rules
            .into_iter()
            .map(|r| compile_disambig_rule(&compiler, r))
            .collect();

        Ok(Self {
            rules: compiled_rules,
        })
    }

    /// Get the number of loaded rules
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

impl Disambiguator for XmlDisambiguator {
    fn disambiguate(&self, sentence: &mut AnalyzedSentence) {
        // Collect all match operations first, then apply them.
        // This avoids holding immutable borrows while mutating.
        let mut pending_actions: Vec<(usize, &DisambigAction)> = Vec::new();

        for rule in &self.rules {
            if rule.pattern.tokens.is_empty() {
                continue;
            }

            let tokens: Vec<&AnalyzedTokenReadings> = sentence.non_whitespace_tokens();

            // Try to match at each position
            for start_idx in 0..tokens.len() {
                if let Some(match_result) = match_pattern(&rule.pattern, &tokens, start_idx) {
                    // Check antipatterns
                    let mut blocked = false;
                    for ap in &rule.antipatterns {
                        if antipattern_matches(ap, &tokens, start_idx) {
                            blocked = true;
                            break;
                        }
                    }
                    if blocked {
                        continue;
                    }

                    // Determine which token(s) to apply the disambiguation to
                    let target_indices = if let (Some(ms), Some(me)) =
                        (rule.pattern.marker_start, rule.pattern.marker_end)
                    {
                        // Apply only to marked tokens
                        match_result
                            .matched_positions
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| *i >= ms && *i < me)
                            .map(|(_, &pos)| pos)
                            .collect()
                    } else {
                        // Apply to first matched token (LT default behavior)
                        match_result
                            .matched_positions
                            .first()
                            .map(|&p| vec![p])
                            .unwrap_or_default()
                    };

                    for token_idx in target_indices {
                        pending_actions.push((token_idx, &rule.disambig));
                    }
                }
            }
        }

        // Apply all pending actions
        for (non_ws_idx, action) in pending_actions {
            apply_disambiguation(sentence.tokens_mut(), non_ws_idx, action);
        }
    }
}

struct MatchResult {
    matched_positions: Vec<usize>,
}

/// Match a compiled pattern against tokens starting at start_idx
fn match_pattern(
    pattern: &CompiledPattern,
    tokens: &[&AnalyzedTokenReadings],
    start_idx: usize,
) -> Option<MatchResult> {
    let elements = &pattern.elements;
    let has_structured = elements
        .iter()
        .any(|e| matches!(e, CompiledPatternElement::OrGroup(_) | CompiledPatternElement::AndGroup(_)));

    if has_structured {
        match_elements(elements, tokens, start_idx)
    } else {
        match_tokens(&pattern.tokens, tokens, start_idx)
    }
}

/// Match flat token list with proper backtracking for optional tokens
fn match_tokens(
    pattern_tokens: &[CompiledPatternToken],
    tokens: &[&AnalyzedTokenReadings],
    start_idx: usize,
) -> Option<MatchResult> {
    match_tokens_recursive(pattern_tokens, tokens, start_idx, 0)
}

fn match_tokens_recursive(
    pattern_tokens: &[CompiledPatternToken],
    tokens: &[&AnalyzedTokenReadings],
    token_idx: usize,
    pattern_idx: usize,
) -> Option<MatchResult> {
    if pattern_idx >= pattern_tokens.len() {
        return Some(MatchResult {
            matched_positions: Vec::new(),
        });
    }
    if token_idx >= tokens.len() {
        // Check if remaining patterns are all optional
        let all_optional = pattern_tokens[pattern_idx..].iter().all(|pt| pt.min.unwrap_or(1) == 0);
        if all_optional {
            return Some(MatchResult {
                matched_positions: Vec::new(),
            });
        }
        return None;
    }

    let pt = &pattern_tokens[pattern_idx];
    let is_optional = pt.min.unwrap_or(1) == 0;

    // Try matching the token at current position
    if token_matches(pt, tokens, token_idx, token_idx > 0) {
        let skip = if pt.skip > 0 { pt.skip as usize } else { 0 };
        let next_token = token_idx + 1 + skip;

        if let Some(mut result) = match_tokens_recursive(pattern_tokens, tokens, next_token, pattern_idx + 1) {
            result.matched_positions.insert(0, token_idx);
            return Some(result);
        }
    }

    // If optional, try skipping this pattern token
    if is_optional {
        return match_tokens_recursive(pattern_tokens, tokens, token_idx, pattern_idx + 1);
    }

    None
}

/// Match structured elements (with or/and groups)
fn match_elements(
    elements: &[CompiledPatternElement],
    tokens: &[&AnalyzedTokenReadings],
    start_idx: usize,
) -> Option<MatchResult> {
    let mut positions = Vec::new();
    let mut token_idx = start_idx;

    for elem in elements {
        if token_idx >= tokens.len() {
            return None;
        }

        match elem {
            CompiledPatternElement::Token(pt) => {
                if token_matches(pt, tokens, token_idx, token_idx > start_idx) {
                    positions.push(token_idx);
                    token_idx += 1;
                } else if pt.min.unwrap_or(1) == 0 {
                    // Optional
                    positions.push(token_idx);
                    // Don't advance token_idx
                } else {
                    return None;
                }
            }
            CompiledPatternElement::OrGroup(alternatives) => {
                let mut matched = false;
                for alt in alternatives {
                    if token_matches(alt, tokens, token_idx, token_idx > start_idx) {
                        positions.push(token_idx);
                        token_idx += 1;
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    return None;
                }
            }
            CompiledPatternElement::AndGroup(constraints) => {
                let mut all_match = true;
                for c in constraints {
                    if !token_matches(c, tokens, token_idx, token_idx > start_idx) {
                        all_match = false;
                        break;
                    }
                }
                if all_match {
                    positions.push(token_idx);
                    token_idx += 1;
                } else {
                    return None;
                }
            }
        }
    }

    Some(MatchResult {
        matched_positions: positions,
    })
}

/// Check if a pattern token matches a specific token position
fn token_matches(
    pt: &CompiledPatternToken,
    tokens: &[&AnalyzedTokenReadings],
    token_idx: usize,
    has_previous: bool,
) -> bool {
    if token_idx >= tokens.len() {
        return false;
    }
    let token = tokens[token_idx];
    let text = token.token().token();

    // Check spacebefore="no"
    if pt.space_before.as_deref() == Some("no") && has_previous && token_idx > 0 {
        let prev = tokens[token_idx - 1];
        if token.token().start() != prev.token().end() {
            return false;
        }
    }

    // Check text match
    let text_matches = if let Some(ref pattern_text) = pt.text {
        let surface_matches = if pt.case_sensitive {
            text == pattern_text
        } else {
            text.eq_ignore_ascii_case(pattern_text)
        };

        if pt.inflected {
            // inflected="yes": match surface form or any reading's lemma
            let lemma_matches = surface_matches || token.readings().iter().any(|r| {
                if let Some(lemma) = r.lemma() {
                    if pt.case_sensitive {
                        lemma == pattern_text
                    } else {
                        lemma.eq_ignore_ascii_case(pattern_text)
                    }
                } else {
                    false
                }
            });
            if pt.negate { !lemma_matches } else { lemma_matches }
        } else {
            if pt.negate { !surface_matches } else { surface_matches }
        }
    } else if let Some(ref re) = pt.compiled_regexp {
        let re_to_use = if !pt.case_sensitive {
            pt.compiled_regexp_ci.as_ref().unwrap_or(re)
        } else {
            re
        };
        if pt.negate {
            !re_to_use.is_match(text)
        } else {
            re_to_use.is_match(text)
        }
    } else {
        // No text constraint - matches any token (unless negate)
        !pt.negate
    };

    if !text_matches {
        return false;
    }

    // Check POS tag constraint
    let pos_matches = if let Some(ref postag) = pt.postag {
        if let Some(ref re) = pt.compiled_postag_regexp {
            let has_matching = token.readings().iter().any(|r| {
                r.pos_tags().iter().any(|t| re.is_match(t))
            });
            if pt.negate_pos { !has_matching } else { has_matching }
        } else {
            let has_matching = token.readings().iter().any(|r| {
                r.pos_tags().iter().any(|t| t == postag)
            });
            if pt.negate_pos { !has_matching } else { has_matching }
        }
    } else {
        true
    };

    if !pos_matches {
        return false;
    }

    // Check chunk constraint
    if let Some(ref chunk) = pt.chunk {
        let token_chunk = token.chunk().unwrap_or("");
        if token_chunk != chunk {
            return false;
        }
    }
    if let Some(ref chunk_re) = pt.chunk_re {
        let token_chunk = token.chunk().unwrap_or("");
        if !chunk_re.is_match(token_chunk) {
            return false;
        }
    }

    // Check exceptions
    for exc in &pt.exceptions {
        let text_exc = if let Some(ref exc_text) = exc.text {
            if let Some(ref re) = exc.compiled_regexp {
                re.is_match(text)
            } else {
                text.eq_ignore_ascii_case(exc_text)
            }
        } else {
            false
        };
        let pos_exc = if let Some(ref postag) = exc.postag {
            if let Some(ref re) = exc.compiled_postag_regexp {
                token.readings().iter().any(|r| {
                    r.pos_tags().iter().any(|t| re.is_match(t))
                })
            } else {
                token.readings().iter().any(|r| {
                    r.pos_tags().iter().any(|t| t == postag)
                })
            }
        } else {
            false
        };

        // An exception matches if its text matches (if present) AND its POS matches (if present, respecting negate_pos)
        let has_text_constraint = exc.text.is_some();
        let has_pos_constraint = exc.postag.is_some();
        let pos_ok = if exc.negate_pos { !pos_exc } else { pos_exc };
        let exc_matches = if has_text_constraint && has_pos_constraint {
            text_exc && pos_ok
        } else if has_text_constraint {
            text_exc
        } else if has_pos_constraint {
            pos_ok
        } else {
            false
        };

        let exc_active = if exc.negate { !exc_matches } else { exc_matches };
        if exc_active {
            return false;
        }
    }

    true
}

/// Check if an antipattern matches starting from any position
fn antipattern_matches(
    antipattern: &CompiledPattern,
    tokens: &[&AnalyzedTokenReadings],
    _rule_start: usize,
) -> bool {
    // Antipattern can match anywhere in the sentence
    for start in 0..tokens.len() {
        if match_pattern(antipattern, tokens, start).is_some() {
            return true;
        }
    }
    false
}

/// Apply a disambiguation action to a specific token
fn apply_disambiguation(
    tokens: &mut Vec<AnalyzedTokenReadings>,
    token_idx: usize,
    action: &DisambigAction,
) {
    // Get the non-whitespace index
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
    let real_idx = match real_idx {
        Some(idx) => idx,
        None => return,
    };

    match action {
        DisambigAction::SetPos(pos) => {
            // Replace all readings with a single POS tag
            let base = tokens[real_idx].token().clone();
            let mut new_reading = AnalyzedToken::new(base.token(), base.start(), base.end());
            new_reading.set_pos_tags(vec![pos.clone()]);
            new_reading.set_lemma(base.lemma().map(|l| l.to_string()));
            tokens[real_idx] = AnalyzedTokenReadings::new(base).with_readings(vec![new_reading]);
        }
        DisambigAction::Replace(wds) => {
            // Replace readings with specified word(s)
            let base = tokens[real_idx].token().clone();
            let new_readings: Vec<AnalyzedToken> = wds
                .iter()
                .map(|w| {
                    let mut reading = AnalyzedToken::new(base.token(), base.start(), base.end());
                    if let Some(pos) = &w.pos {
                        reading.set_pos_tags(vec![pos.clone()]);
                    }
                    if let Some(lemma) = &w.lemma {
                        reading.set_lemma(Some(lemma.clone()));
                    } else {
                        reading.set_lemma(base.lemma().map(|l| l.to_string()));
                    }
                    reading
                })
                .collect();
            if !new_readings.is_empty() {
                tokens[real_idx] = AnalyzedTokenReadings::new(base).with_readings(new_readings);
            }
        }
        DisambigAction::Remove(wds) => {
            // Remove readings matching the specified criteria
            let base = tokens[real_idx].token().clone();
            let current_readings = tokens[real_idx].readings().to_vec();
            let filtered: Vec<AnalyzedToken> = current_readings
                .into_iter()
                .filter(|r| {
                    !wds.iter().any(|w| {
                        let pos_match = w.pos.as_ref().map_or(false, |pos| {
                            // Support regex patterns (e.g., "PRP.*")
                            let is_regex = pos.contains('.') || pos.contains('*') || pos.contains('+')
                                || pos.contains('[') || pos.contains('(') || pos.contains('|');
                            if is_regex {
                                if let Ok(re) = regex::Regex::new(pos) {
                                    r.pos_tags().iter().any(|t| re.is_match(t))
                                } else {
                                    r.pos_tags().contains(pos)
                                }
                            } else {
                                r.pos_tags().contains(pos)
                            }
                        });
                        let lemma_match = w.lemma.as_ref().map_or(true, |lemma| {
                            r.lemma() == Some(lemma.as_str())
                        });
                        pos_match && lemma_match
                    })
                })
                .collect();
            if !filtered.is_empty() {
                tokens[real_idx] = AnalyzedTokenReadings::new(base).with_readings(filtered);
            }
        }
        DisambigAction::Add(wds) => {
            // Add new readings to existing ones
            let current_readings = tokens[real_idx].readings().to_vec();
            let base = tokens[real_idx].token().clone();
            let mut new_readings = current_readings;
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
            tokens[real_idx] = AnalyzedTokenReadings::new(base).with_readings(new_readings);
        }
        DisambigAction::Filter { postag } => {
            // Keep only readings that match the postag regex
            let re = regex::Regex::new(postag);
            if let Ok(re) = re {
                let base = tokens[real_idx].token().clone();
                let current_readings = tokens[real_idx].readings().to_vec();
                let filtered: Vec<AnalyzedToken> = current_readings
                    .into_iter()
                    .filter(|r| r.pos_tags().iter().any(|t| re.is_match(t)))
                    .collect();
                if !filtered.is_empty() {
                    tokens[real_idx] =
                        AnalyzedTokenReadings::new(base).with_readings(filtered);
                }
            }
        }
        DisambigAction::FilterAll => {
            // Remove all readings - token becomes unknown
            let base = tokens[real_idx].token().clone();
            let empty_reading = AnalyzedToken::new(base.token(), base.start(), base.end());
            tokens[real_idx] = AnalyzedTokenReadings::new(base).with_readings(vec![empty_reading]);
        }
        DisambigAction::IgnoreSpelling => {
            // Mark token to be ignored by spell checker
        }
        DisambigAction::Unify => {
            // Unification - complex feature matching, not yet implemented
        }
    }
}

fn compile_disambig_rule(compiler: &XmlCompiler, rule: DisambigRule) -> CompiledDisambigRule {
    CompiledDisambigRule {
        id: rule.id,
        name: rule.name,
        pattern: CompiledPattern {
            tokens: rule.pattern.tokens.iter().map(|t| compiler.compile_token(t)).collect(),
            elements: rule.pattern.elements.iter().map(|e| compiler.compile_element(e)).collect(),
            case_sensitive: rule.pattern.case_sensitive,
            marker_start: rule.pattern.marker_start,
            marker_end: rule.pattern.marker_end,
        },
        antipatterns: rule
            .antipatterns
            .iter()
            .map(|ap| CompiledPattern {
                tokens: ap.tokens.iter().map(|t| compiler.compile_token(t)).collect(),
                elements: ap.elements.iter().map(|e| compiler.compile_element(e)).collect(),
                case_sensitive: ap.case_sensitive,
                marker_start: None,
                marker_end: None,
            })
            .collect(),
        disambig: rule.disambig,
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
            // Create separate readings for each POS tag (like the real tagger)
            let readings: Vec<AnalyzedToken> = pos_tags
                .into_iter()
                .map(|pos| {
                    AnalyzedToken::new(text, start, end)
                        .with_pos_tags(vec![pos.to_string()])
                })
                .collect();
            analyzed.push(AnalyzedTokenReadings::new(base).with_readings(readings));
            offset = end + 1; // space
        }
        let total_len = offset.saturating_sub(1);
        let mut sentence = AnalyzedSentence::new("test", 0, total_len);
        sentence.set_tokens(analyzed);
        sentence
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
        // Should have only CD tag now
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

        // Create sentence with adjacent tokens for spacebefore="no"
        let mut sentence = AnalyzedSentence::new("ca n't", 0, 5);
        let mut tokens = Vec::new();
        let t1 = AnalyzedToken::new("ca", 0, 2).with_pos_tags(vec!["NN".to_string()]);
        tokens.push(AnalyzedTokenReadings::new(t1));
        let t2 = AnalyzedToken::new("n't", 2, 5).with_pos_tags(vec!["RB".to_string()]); // Adjacent - no space
        tokens.push(AnalyzedTokenReadings::new(t2));
        sentence.set_tokens(tokens);

        disambig.disambiguate(&mut sentence);

        // Only the "ca" token (first, inside marker) should be affected
        let first = &sentence.tokens()[0];
        assert!(first.has_pos_tag("MD"));
        assert!(!first.has_pos_tag("NN"));
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
        let readings2 = vec![
            AnalyzedToken::new("n't", 2, 5).with_pos_tags(vec!["RB".to_string()]),
        ];
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
        // Should be unchanged
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

        // Test that "January" matches (entity expanded)
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
</rules>"#;

        let disambig = XmlDisambiguator::from_xml(xml).unwrap();

        // Sentence WITHOUT the antipattern token - should disambiguate
        let mut sentence1 = make_sentence(vec![("test", vec!["NN", "VB"])]);
        disambig.disambiguate(&mut sentence1);
        assert!(sentence1.tokens()[0].has_pos_tag("NN"));
        assert!(!sentence1.tokens()[0].has_pos_tag("VB"));

        // Sentence WITH the antipattern token - should NOT disambiguate
        let mut sentence2 = make_sentence(vec![("nope", vec!["RB"]), ("test", vec!["NN", "VB"])]);
        disambig.disambiguate(&mut sentence2);
        assert!(sentence2.tokens()[1].has_pos_tag("NN"));
        assert!(sentence2.tokens()[1].has_pos_tag("VB")); // Still has both
    }

    #[test]
    fn test_load_real_english_disambiguation() {
        let path = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/disambiguation.xml";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: disambiguation.xml not found");
            return;
        }
        let xml = std::fs::read_to_string(path).unwrap();
        match XmlDisambiguator::from_xml(&xml) {
            Ok(d) => {
                println!("Loaded {} disambiguation rules", d.rule_count());
                assert!(d.rule_count() > 100, "Expected at least 100 disambiguation rules");
            }
            Err(e) => panic!("Failed to load disambiguation.xml: {}", e),
        }
    }
}
