use crate::types::*;
use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum XmlParseError {
    #[error("XML parse error: {0}")]
    Parse(#[from] quick_xml::Error),
    #[error("Invalid rule: {0}")]
    InvalidRule(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

type PhraseVariants = Vec<(Vec<XmlPatternToken>, Vec<XmlPatternElement>)>;

struct PatternRefUse {
    idref: String,
    split_tok: usize,
    split_elem: usize,
}

fn renumber_phrase_refs(tokens: &mut [XmlPatternToken], phrase_ranges: &[(usize, usize)]) {
    for (idx, token) in tokens.iter_mut().enumerate() {
        if idx == 0 || token.match_no.is_none() {
            continue;
        }
        let in_phrase = phrase_ranges
            .iter()
            .any(|&(start, len)| idx >= start && idx < start + len);
        if in_phrase {
            let old_no = token.match_no.unwrap_or(0);
            let new_no = old_no + idx as i32 - 1;
            token.match_no = Some(new_no);
            if let Some(text) = token.text.as_mut() {
                *text = text.replace(&format!("\\{old_no}"), &format!("\\{new_no}"));
            }
        }
    }
}

pub struct XmlRuleParser;

impl XmlRuleParser {
    pub fn new() -> Self {
        Self
    }

    #[allow(unused_assignments, unused_variables)]
    pub fn parse(&self, xml: &str) -> Result<XmlRuleFile, XmlParseError> {
        self.parse_with_base(xml, None)
    }

    #[allow(unused_assignments, unused_variables)]
    pub fn parse_with_base(
        &self,
        xml: &str,
        base_dir: Option<&std::path::Path>,
    ) -> Result<XmlRuleFile, XmlParseError> {
        let expanded = Self::expand_entities_with_base(xml, base_dir);
        let cleaned = Self::strip_doctype(&expanded);

        let mut reader = Reader::from_str(&cleaned);
        reader.config_mut().trim_text(false);

        let mut file = XmlRuleFile::default();
        let mut current_category: Option<XmlCategory> = None;
        let mut current_rule: Option<XmlRule> = None;
        let mut current_pattern: Option<XmlPattern> = None;
        let mut unify_start: Option<usize> = None;
        let mut unify_elem_start: Option<usize> = None;
        let mut unify_negate = false;
        let mut unify_features: Vec<(String, Vec<String>)> = Vec::new();
        let mut unify_feature_name = String::new();
        let mut unify_feature_types: Vec<String> = Vec::new();
        let mut in_unify_ignore = false;
        let mut unifications: Vec<crate::disambig_types::UnificationFeature> = Vec::new();
        let mut current_unification: Option<crate::disambig_types::UnificationFeature> = None;
        let mut current_equivalence: Option<crate::disambig_types::UnificationEquivalence> = None;
        let mut current_token: Option<XmlPatternToken> = None;
        let mut current_group: Option<XmlRuleGroup> = None;
        let mut group_sub_id: u32 = 0;
        let mut in_message = false;
        let mut in_suggestion = false;
        let mut suggestion_text = String::new();
        let mut suggestion_parts: Vec<SuggestionPart> = Vec::new();
        let mut message_text = String::new();
        let mut message_suppress = false;
        let mut suggestion_suppress = false;
        let mut in_example = false;
        let mut example_type = XmlExampleType::Correct;
        let mut example_text = String::new();
        let mut current_text = String::new();
        let mut buf = Vec::new();
        let mut in_antipattern = false;
        let mut current_antipattern: Option<XmlPattern> = None;
        let mut antipattern_token: Option<XmlPatternToken> = None;
        let mut antipattern_text = String::new();
        let mut in_pattern_marker = false;
        let mut pattern_marker_start: Option<usize> = None;
        let mut pattern_marker_end: Option<usize> = None;
        let mut pattern_marker_elem_start: Option<usize> = None;
        let mut pattern_marker_elem_end: Option<usize> = None;
        let mut antipattern_marker_elem_start: Option<usize> = None;
        let mut antipattern_marker_elem_end: Option<usize> = None;
        let mut in_phrases = false;
        let mut phrase_id = String::new();
        let mut phrase_run_tokens: Vec<XmlPatternToken> = Vec::new();
        let mut phrase_run_elements: Vec<XmlPatternElement> = Vec::new();
        let mut phrase_variants: PhraseVariants = Vec::new();
        let mut phrase_after_ref = false;
        let mut phrase_map: HashMap<String, PhraseVariants> = HashMap::new();
        let mut pattern_ref_uses: Vec<PatternRefUse> = Vec::new();
        let mut in_exception = false;
        let mut current_exception: Option<XmlException> = None;
        let mut exception_text = String::new();
        let mut in_or_group = false;
        let mut current_or_group: Option<XmlOrGroup> = None;
        let mut in_and_group = false;
        let mut current_and_group: Option<XmlAndGroup> = None;
        let mut in_short = false;
        let mut short_text = String::new();
        let mut in_url = false;
        let mut url_text = String::new();
        let mut example_correction: Option<String> = None;
        let mut current_rule_regexp: Option<XmlRuleRegexp> = None;
        let mut rule_regexp_text = String::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let local_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match local_name.as_str() {
                        "category" => {
                            let mut cat = XmlCategory::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "id" => cat.id = val,
                                    "name" => cat.name = val,
                                    "description" => cat.description = Some(val),
                                    "default" => cat.default_on = val == "on",
                                    "type" => cat.issue_type = Some(val),
                                    _ => {}
                                }
                            }
                            current_category = Some(cat);
                        }
                        "rulegroup" => {
                            let mut group = XmlRuleGroup::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "id" => group.id = Some(val),
                                    "name" => group.name = Some(val),
                                    "default" => group.default_on = Some(val == "on"),
                                    "type" => group.issue_type = Some(val),
                                    "tags" => {
                                        group.tags =
                                            val.split_whitespace().map(str::to_string).collect()
                                    }
                                    _ => {}
                                }
                            }
                            current_group = Some(group);
                            group_sub_id = 0;
                        }
                        "rule" => {
                            let mut rule = XmlRule {
                                default_on: true,
                                ..Default::default()
                            };
                            if current_group.is_some() {
                                group_sub_id += 1;
                            }
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "id" => rule.id = val,
                                    "name" => rule.name = val,
                                    "description" => rule.description = val,
                                    "sub_id" => rule.sub_id = Some(val),
                                    "default" => rule.default_on = val == "on",
                                    "deprecated" => rule.deprecated = val == "yes",
                                    "type" => rule.issue_type = Some(val),
                                    "tags" => {
                                        rule.tags =
                                            val.split_whitespace().map(str::to_string).collect()
                                    }
                                    _ => {}
                                }
                            }
                            if let Some(cat) = &current_category {
                                rule.category = cat.clone();
                            }
                            current_rule = Some(rule);
                            pattern_ref_uses.clear();
                        }
                        "pattern" => {
                            let mut pattern = XmlPattern::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key.as_str() == "case_sensitive" {
                                    pattern.case_sensitive = val == "yes"
                                } else if key.as_str() == "raw_pos" {
                                    pattern.raw_pos = val == "yes";
                                }
                            }
                            current_pattern = Some(pattern);
                            pattern_marker_start = None;
                        }
                        "regexp"
                            if current_rule.is_some()
                                && current_pattern.is_none()
                                && current_token.is_none()
                                && current_antipattern.is_none() =>
                        {
                            let mut rule_regexp = XmlRuleRegexp::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "case_sensitive" => rule_regexp.case_sensitive = val == "yes",
                                    "type" => rule_regexp.exact = val == "exact",
                                    "mark" => rule_regexp.mark = val.parse().unwrap_or(0),
                                    _ => {}
                                }
                            }
                            current_rule_regexp = Some(rule_regexp);
                            rule_regexp_text.clear();
                        }
                        "antipattern" => {
                            in_antipattern = true;
                            let mut ap = XmlPattern::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "case_sensitive" {
                                    ap.case_sensitive = val == "yes";
                                }
                            }
                            current_antipattern = Some(ap);
                            antipattern_text.clear();
                        }
                        "phrases" => {
                            in_phrases = true;
                        }
                        "phrase" if in_phrases => {
                            phrase_id.clear();
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"id" {
                                    phrase_id = crate::decode_attr_value(&attr.value);
                                }
                            }
                            phrase_run_tokens.clear();
                            phrase_run_elements.clear();
                            phrase_variants.clear();
                            phrase_after_ref = false;
                        }
                        "phraseref" => {
                            let mut idref = String::new();
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"idref" {
                                    idref = crate::decode_attr_value(&attr.value);
                                }
                            }
                            if in_phrases {
                                if let Some(alts) = phrase_map.get(&idref) {
                                    for alt in alts {
                                        if phrase_run_tokens.is_empty() {
                                            phrase_variants.push((alt.0.clone(), alt.1.clone()));
                                        } else {
                                            let mut tokens = phrase_run_tokens.clone();
                                            tokens.extend(alt.0.iter().cloned());
                                            let mut elements = phrase_run_elements.clone();
                                            elements.extend(alt.1.iter().cloned());
                                            phrase_variants.push((tokens, elements));
                                        }
                                    }
                                    phrase_after_ref = true;
                                }
                            } else if let Some(alts) = phrase_map.get(&idref)
                                && let (ref t0, ref e0) = alts[0]
                            {
                                if let Some(ref mut pattern) = current_pattern {
                                    pattern_ref_uses.push(PatternRefUse {
                                        idref: idref.clone(),
                                        split_tok: pattern.tokens.len(),
                                        split_elem: pattern.elements.len(),
                                    });
                                    pattern.tokens.extend(t0.iter().cloned());
                                    pattern.elements.extend(e0.iter().cloned());
                                } else if let Some(ref mut ap) = current_antipattern {
                                    ap.tokens.extend(t0.iter().cloned());
                                    ap.elements.extend(e0.iter().cloned());
                                }
                            }
                        }
                        "exception" => {
                            let mut exc = XmlException::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "regexp" => exc.regexp = Some(val),
                                    "postag" => exc.postag = Some(val),
                                    "postag_regexp" => exc.postag_regexp = Some(val),
                                    "negate" => exc.negate = val == "yes",
                                    "negate_pos" => exc.negate_pos = val == "yes",
                                    "inflected" => exc.inflected = val == "yes",
                                    "case_sensitive" => {
                                        exc.case_sensitive = val == "yes";
                                        exc.case_sensitive_set = true;
                                    }
                                    "scope" => exc.scope = Some(val),
                                    "spacebefore" if val != "ignore" => {
                                        exc.space_before = Some(val == "yes");
                                    }
                                    _ => {}
                                }
                            }
                            in_exception = true;
                            current_exception = Some(exc);
                            exception_text.clear();
                        }
                        "token" if current_unification.is_some() => {
                            let mut token = XmlPatternToken::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "postag" => token.postag = Some(val),
                                    "postag_regexp" => token.postag_regexp = Some(val),
                                    "negate_pos" => token.negate_pos = val == "yes",
                                    _ => {}
                                }
                            }
                            if let Some(eq) = &mut current_equivalence {
                                eq.postag = token.postag.clone().unwrap_or_default();
                                eq.exact = token.postag_regexp.as_deref() != Some("yes");
                            }
                        }
                        "token" => {
                            let mut token = XmlPatternToken::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "regexp" => token.regexp = Some(val),
                                    "postag" => token.postag = Some(val),
                                    "postag_regexp" => token.postag_regexp = Some(val),
                                    "negate" => token.negate = val == "yes",
                                    "negate_pos" => token.negate_pos = val == "yes",
                                    "case_sensitive" => {
                                        token.case_sensitive = val == "yes";
                                        token.case_sensitive_set = true;
                                    }
                                    "inflected" => token.inflected = val == "yes",
                                    "min" => token.min = val.parse().ok(),
                                    "max" => token.max = val.parse().ok(),
                                    "skip" => token.skip = val.parse().unwrap_or(0),
                                    "spacebefore" => token.space_before = Some(val),
                                    "chunk" => token.chunk = Some(val),
                                    "chunk_re" => token.chunk_re = Some(val),
                                    _ => {}
                                }
                            }
                            current_text.clear();
                            if in_phrases && phrase_after_ref {
                                phrase_run_tokens.clear();
                                phrase_run_elements.clear();
                                phrase_after_ref = false;
                            }
                            current_token = Some(token);
                        }
                        "message" => {
                            in_message = true;
                            message_text.clear();
                            message_suppress = false;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                if key == "suppress_misspelled" {
                                    message_suppress = crate::decode_attr_value(&attr.value)
                                        .eq_ignore_ascii_case("yes");
                                }
                            }
                        }
                        "short" => {
                            in_short = true;
                            short_text.clear();
                        }
                        "url" => {
                            in_url = true;
                            url_text.clear();
                        }
                        "suggestion" => {
                            in_suggestion = true;
                            suggestion_text.clear();
                            suggestion_parts.clear();
                            suggestion_suppress = false;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                if key == "suppress_misspelled" {
                                    suggestion_suppress = crate::decode_attr_value(&attr.value)
                                        .eq_ignore_ascii_case("yes");
                                }
                            }
                        }
                        "match" if in_suggestion => {
                            let mut m = XmlMatch::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "no" => m.no = val.parse().unwrap_or(1),
                                    "case_conversion" => m.case_conversion = Some(val),
                                    "regexp_match" => m.regexp_match = Some(val),
                                    "regexp_replace" => m.regexp_replace = Some(val),
                                    "include_inflected" => m.include_inflected = val == "yes",
                                    "include_skipped" => m.include_skipped = Some(val),
                                    "postag" => m.postag = Some(val),
                                    "postag_regexp" => m.postag_regexp = val == "yes",
                                    "postag_replace" => m.postag_replace = Some(val),
                                    _ => {}
                                }
                            }
                            suggestion_parts.push(SuggestionPart::Match(m));
                        }
                        "match" if current_token.is_some() => {
                            let mut marker_no: Option<String> = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "no"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no = val.parse().ok();
                                    if token.match_no.is_some() {
                                        marker_no = Some(val.clone());
                                    }
                                }
                                if key == "postag"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_postag = Some(val.clone());
                                }
                                if key == "postag_regexp"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_postag_regexp = val == "yes";
                                }
                                if key == "postag_replace"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_postag_replace = Some(val.clone());
                                }
                                if key == "setpos"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_setpos = val == "yes";
                                }
                                if key == "regexp_match"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_regexp_match = Some(val.clone());
                                }
                                if key == "regexp_replace"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_regexp_replace = Some(val.clone());
                                }
                                if key == "case_conversion"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_case_conversion = Some(val.clone());
                                }
                            }
                            if let Some(no) = marker_no {
                                current_text.push('\\');
                                current_text.push_str(&no);
                            }
                        }
                        "example" => {
                            let mut ex_type_str: Option<String> = None;
                            example_correction = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "type" => ex_type_str = Some(val),
                                    "correction" => example_correction = Some(val),
                                    _ => {}
                                }
                            }
                            example_type = match ex_type_str.as_deref() {
                                Some("incorrect") => XmlExampleType::Incorrect,
                                Some("triggers_error") => XmlExampleType::TriggersError,
                                _ if example_correction.is_some() => XmlExampleType::Incorrect,
                                _ => XmlExampleType::Correct,
                            };
                            in_example = true;
                            example_text.clear();
                        }
                        "marker" => {
                            if in_example {
                                example_text.push_str("<marker>");
                            } else if current_pattern.is_some() || current_antipattern.is_some() {
                                in_pattern_marker = true;
                                if let Some(ref pattern) = current_pattern {
                                    pattern_marker_start = Some(pattern.tokens.len());
                                    pattern_marker_elem_start = Some(pattern.elements.len());
                                } else if let Some(ref ap) = current_antipattern {
                                    antipattern_marker_elem_start = Some(ap.elements.len());
                                }
                            }
                        }
                        "unification" => {
                            let mut feature = crate::disambig_types::UnificationFeature::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "feature" {
                                    feature.feature = val;
                                }
                            }
                            current_unification = Some(feature);
                        }
                        "equivalence" => {
                            let mut eq = crate::disambig_types::UnificationEquivalence::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "type" => eq.eq_type = val,
                                    "postag" => eq.postag = val,
                                    "postag_regexp" => eq.exact = val != "yes",
                                    _ => {}
                                }
                            }
                            current_equivalence = Some(eq);
                        }
                        "unify" => {
                            if let Some(pattern) = &mut current_pattern {
                                unify_start = Some(pattern.tokens.len());
                                unify_elem_start = Some(pattern.elements.len());
                            } else if let Some(ap) = &mut current_antipattern {
                                unify_start = Some(ap.tokens.len());
                                unify_elem_start = Some(ap.elements.len());
                            }
                            unify_negate = false;
                            unify_features.clear();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "negate" {
                                    unify_negate = val == "yes";
                                }
                            }
                        }
                        "unify-ignore" => {
                            in_unify_ignore = true;
                        }
                        "feature" if unify_start.is_some() => {
                            unify_feature_name.clear();
                            unify_feature_types.clear();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "id" {
                                    unify_feature_name = val;
                                }
                            }
                        }
                        "type" if unify_start.is_some() => {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "id" {
                                    unify_feature_types.push(val);
                                }
                            }
                        }
                        "or" => {
                            in_or_group = true;
                            current_or_group = Some(XmlOrGroup::default());
                        }
                        "and" => {
                            in_and_group = true;
                            current_and_group = Some(XmlAndGroup::default());
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(e)) => {
                    let local_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match local_name.as_str() {
                        "feature" if unify_start.is_some() => {
                            let mut id = String::new();
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"id" {
                                    id = crate::decode_attr_value(&attr.value);
                                }
                            }
                            if !id.is_empty() {
                                unify_features.push((id, Vec::new()));
                            }
                        }
                        "type" if unify_start.is_some() => {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"id" {
                                    unify_feature_types.push(crate::decode_attr_value(&attr.value));
                                }
                            }
                        }
                        "phraseref" => {
                            let mut idref = String::new();
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"idref" {
                                    idref = crate::decode_attr_value(&attr.value);
                                }
                            }
                            if in_phrases {
                                if let Some(alts) = phrase_map.get(&idref) {
                                    for alt in alts {
                                        if phrase_run_tokens.is_empty() {
                                            phrase_variants.push((alt.0.clone(), alt.1.clone()));
                                        } else {
                                            let mut tokens = phrase_run_tokens.clone();
                                            tokens.extend(alt.0.iter().cloned());
                                            let mut elements = phrase_run_elements.clone();
                                            elements.extend(alt.1.iter().cloned());
                                            phrase_variants.push((tokens, elements));
                                        }
                                    }
                                    phrase_after_ref = true;
                                }
                            } else if let Some(alts) = phrase_map.get(&idref)
                                && let (ref t0, ref e0) = alts[0]
                            {
                                if let Some(ref mut pattern) = current_pattern {
                                    pattern_ref_uses.push(PatternRefUse {
                                        idref: idref.clone(),
                                        split_tok: pattern.tokens.len(),
                                        split_elem: pattern.elements.len(),
                                    });
                                    pattern.tokens.extend(t0.iter().cloned());
                                    pattern.elements.extend(e0.iter().cloned());
                                } else if let Some(ref mut ap) = current_antipattern {
                                    ap.tokens.extend(t0.iter().cloned());
                                    ap.elements.extend(e0.iter().cloned());
                                }
                            }
                        }
                        "token" if current_unification.is_some() => {
                            let mut token = XmlPatternToken::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "postag" => token.postag = Some(val),
                                    "postag_regexp" => token.postag_regexp = Some(val),
                                    "negate_pos" => token.negate_pos = val == "yes",
                                    _ => {}
                                }
                            }
                            if let Some(eq) = &mut current_equivalence {
                                eq.postag = token.postag.clone().unwrap_or_default();
                                eq.exact = token.postag_regexp.as_deref() != Some("yes");
                            }
                        }
                        "token" => {
                            let mut token = XmlPatternToken::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "regexp" => token.regexp = Some(val),
                                    "negate" => token.negate = val == "yes",
                                    "negate_pos" => token.negate_pos = val == "yes",
                                    "case_sensitive" => {
                                        token.case_sensitive = val == "yes";
                                        token.case_sensitive_set = true;
                                    }
                                    "inflected" => token.inflected = val == "yes",
                                    "postag" => token.postag = Some(val),
                                    "postag_regexp" => token.postag_regexp = Some(val),
                                    "min" => token.min = val.parse().ok(),
                                    "max" => token.max = val.parse().ok(),
                                    "skip" => token.skip = val.parse().unwrap_or(0),
                                    "spacebefore" => token.space_before = Some(val),
                                    "chunk" => token.chunk = Some(val),
                                    "chunk_re" => token.chunk_re = Some(val),
                                    _ => {}
                                }
                            }
                            if in_phrases && phrase_after_ref {
                                phrase_run_tokens.clear();
                                phrase_run_elements.clear();
                                phrase_after_ref = false;
                            }
                            if in_or_group {
                                if let Some(ref mut or_group) = current_or_group {
                                    if in_unify_ignore {
                                        token.unify_neutral = true;
                                    }
                                    or_group.alternatives.push(token);
                                }
                            } else if in_and_group {
                                if let Some(ref mut and_group) = current_and_group {
                                    if in_unify_ignore {
                                        token.unify_neutral = true;
                                    }
                                    and_group.constraints.push(token);
                                }
                            } else if in_antipattern {
                                let trimmed = antipattern_text.trim();
                                if !trimmed.is_empty() {
                                    token.text = Some(trimmed.to_string());
                                }
                                if in_unify_ignore {
                                    token.unify_neutral = true;
                                }
                                if let Some(ref mut ap) = current_antipattern {
                                    for copy in token.min_expansion() {
                                        ap.tokens.push(copy.clone());
                                        ap.elements.push(XmlPatternElement::Token(copy));
                                    }
                                }
                                antipattern_text.clear();
                            } else if in_phrases {
                                for copy in token.min_expansion() {
                                    phrase_run_tokens.push(copy.clone());
                                    phrase_run_elements.push(XmlPatternElement::Token(copy));
                                }
                            } else if let Some(pattern) = &mut current_pattern {
                                let trimmed = current_text.trim();
                                if !trimmed.is_empty() {
                                    token.text = Some(trimmed.to_string());
                                }
                                if in_unify_ignore {
                                    token.unify_neutral = true;
                                }
                                for copy in token.min_expansion() {
                                    pattern.tokens.push(copy.clone());
                                    pattern.elements.push(XmlPatternElement::Token(copy));
                                }
                            }
                            current_text.clear();
                        }
                        "exception" => {
                            let mut exc = XmlException::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "regexp" => exc.regexp = Some(val),
                                    "postag" => exc.postag = Some(val),
                                    "postag_regexp" => exc.postag_regexp = Some(val),
                                    "negate" => exc.negate = val == "yes",
                                    "negate_pos" => exc.negate_pos = val == "yes",
                                    "inflected" => exc.inflected = val == "yes",
                                    "case_sensitive" => {
                                        exc.case_sensitive = val == "yes";
                                        exc.case_sensitive_set = true;
                                    }
                                    "scope" => exc.scope = Some(val),
                                    "spacebefore" if val != "ignore" => {
                                        exc.space_before = Some(val == "yes");
                                    }
                                    _ => {}
                                }
                            }
                            if let Some(ref mut token) = current_token {
                                token.exceptions.push(exc);
                            } else if let Some(ref mut ap_token) = antipattern_token {
                                ap_token.exceptions.push(exc);
                            }
                        }
                        "short" => {
                            in_short = true;
                            short_text.clear();
                        }
                        "match" if in_suggestion => {
                            let mut m = XmlMatch::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "no" => m.no = val.parse().unwrap_or(1),
                                    "case_conversion" => m.case_conversion = Some(val),
                                    "regexp_match" => m.regexp_match = Some(val),
                                    "regexp_replace" => m.regexp_replace = Some(val),
                                    "include_inflected" => m.include_inflected = val == "yes",
                                    "include_skipped" => m.include_skipped = Some(val),
                                    "postag" => m.postag = Some(val),
                                    "postag_regexp" => m.postag_regexp = val == "yes",
                                    "postag_replace" => m.postag_replace = Some(val),
                                    _ => {}
                                }
                            }
                            suggestion_parts.push(SuggestionPart::Match(m));
                        }
                        "match" if current_token.is_some() => {
                            let mut marker_no: Option<String> = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "no"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no = val.parse().ok();
                                    if token.match_no.is_some() {
                                        marker_no = Some(val.clone());
                                    }
                                }
                                if key == "postag"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_postag = Some(val.clone());
                                }
                                if key == "postag_regexp"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_postag_regexp = val == "yes";
                                }
                                if key == "postag_replace"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_postag_replace = Some(val.clone());
                                }
                                if key == "setpos"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_setpos = val == "yes";
                                }
                                if key == "regexp_match"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_regexp_match = Some(val.clone());
                                }
                                if key == "regexp_replace"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_regexp_replace = Some(val.clone());
                                }
                                if key == "case_conversion"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_case_conversion = Some(val.clone());
                                }
                            }
                            if let Some(no) = marker_no {
                                current_text.push('\\');
                                current_text.push_str(&no);
                            }
                        }
                        "url" => {}
                        "antipattern" => {}
                        "regexp"
                            if current_rule.is_some()
                                && current_pattern.is_none()
                                && current_token.is_none()
                                && current_antipattern.is_none() =>
                        {
                            let mut rule_regexp = XmlRuleRegexp::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "case_sensitive" => rule_regexp.case_sensitive = val == "yes",
                                    "type" => rule_regexp.exact = val == "exact",
                                    "mark" => rule_regexp.mark = val.parse().unwrap_or(0),
                                    _ => {}
                                }
                            }
                            current_rule_regexp = Some(rule_regexp);
                            rule_regexp_text.clear();
                        }
                        "filter" => {
                            let mut filter = XmlFilter::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "class" => filter.class = val,
                                    "args" => filter.args = val,
                                    _ => {}
                                }
                            }
                            if let Some(rule) = &mut current_rule {
                                rule.filter = Some(filter);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(e)) => {
                    let text = e
                        .unescape()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|_| String::from_utf8_lossy(e.as_ref()).to_string());
                    if in_short {
                        short_text.push_str(&text);
                    } else if in_url {
                        url_text.push_str(&text);
                    } else if in_suggestion {
                        suggestion_text.push_str(&text);
                        suggestion_parts.push(SuggestionPart::Text(text.clone()));
                        if in_message {
                            message_text.push_str(&text);
                        }
                    } else if in_message {
                        message_text.push_str(&text);
                    } else if in_example {
                        example_text.push_str(&text);
                    } else if in_exception {
                        exception_text.push_str(&text);
                    } else if current_rule_regexp.is_some() {
                        rule_regexp_text.push_str(&text);
                    } else if current_token.is_some() {
                        current_text.push_str(&text);
                    } else if in_antipattern {
                        antipattern_text.push_str(&text);
                    }
                }
                Ok(Event::End(e)) => {
                    let local_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match local_name.as_str() {
                        "category" => {
                            if let Some(cat) = current_category.take() {
                                file.categories.push(cat);
                            }
                        }
                        "rulegroup" => {
                            if let Some(mut group) = current_group.take() {
                                let group_antipatterns = group.antipatterns.clone();
                                let group_tags = std::mem::take(&mut group.tags);
                                for rule in &mut group.rules {
                                    let mut combined = group_antipatterns.clone();
                                    combined.append(&mut rule.antipatterns);
                                    rule.antipatterns = combined;
                                    for tag in &group_tags {
                                        if !rule.tags.iter().any(|t| t == tag) {
                                            rule.tags.push(tag.clone());
                                        }
                                    }
                                }
                                group.tags = group_tags;
                                file.rule_groups.push(group);
                            }
                        }
                        "rule" => {
                            if let Some(mut rule) = current_rule.take() {
                                if let Some(mut rule_regexp) = current_rule_regexp.take() {
                                    rule_regexp.text = rule_regexp_text.trim().to_string();
                                    rule.rule_regexp = Some(rule_regexp);
                                }
                                rule_regexp_text.clear();
                                if current_group.is_some() && rule.sub_id.is_none() {
                                    rule.sub_id = Some(group_sub_id.to_string());
                                }
                                let mut variants = Vec::new();
                                if !pattern_ref_uses.is_empty() {
                                    let mut base_ranges: Vec<(usize, usize)> = Vec::new();
                                    for use_ in &pattern_ref_uses {
                                        if let Some(alts) = phrase_map.get(&use_.idref)
                                            && let Some((alt0_tokens, _)) = alts.first()
                                        {
                                            base_ranges.push((use_.split_tok, alt0_tokens.len()));
                                        }
                                    }
                                    renumber_phrase_refs(&mut rule.pattern.tokens, &base_ranges);
                                    for use_ in &pattern_ref_uses {
                                        let Some(alts) = phrase_map.get(&use_.idref) else {
                                            continue;
                                        };
                                        let Some((alt0_tokens, alt0_elements)) = alts.first()
                                        else {
                                            continue;
                                        };
                                        for (alt_tokens, alt_elements) in alts.iter().skip(1) {
                                            let mut tokens =
                                                rule.pattern.tokens[..use_.split_tok].to_vec();
                                            tokens.extend(alt_tokens.iter().cloned());
                                            tokens.extend_from_slice(
                                                &rule.pattern.tokens
                                                    [use_.split_tok + alt0_tokens.len()..],
                                            );
                                            let mut elements =
                                                rule.pattern.elements[..use_.split_elem].to_vec();
                                            elements.extend(alt_elements.iter().cloned());
                                            elements.extend_from_slice(
                                                &rule.pattern.elements
                                                    [use_.split_elem + alt0_elements.len()..],
                                            );
                                            let tok_delta = alt_tokens.len() as isize
                                                - alt0_tokens.len() as isize;
                                            let elem_delta = alt_elements.len() as isize
                                                - alt0_elements.len() as isize;
                                            let shift =
                                                |idx: usize, split: usize, delta: isize| -> usize {
                                                    if idx > split {
                                                        (idx as isize + delta).max(0) as usize
                                                    } else {
                                                        idx
                                                    }
                                                };
                                            let mut cloned = rule.clone();
                                            cloned.pattern.marker_start = cloned
                                                .pattern
                                                .marker_start
                                                .map(|i| shift(i, use_.split_tok, tok_delta));
                                            cloned.pattern.marker_end = cloned
                                                .pattern
                                                .marker_end
                                                .map(|i| shift(i, use_.split_tok, tok_delta));
                                            cloned.pattern.marker_elem_start = cloned
                                                .pattern
                                                .marker_elem_start
                                                .map(|i| shift(i, use_.split_elem, elem_delta));
                                            cloned.pattern.marker_elem_end = cloned
                                                .pattern
                                                .marker_elem_end
                                                .map(|i| shift(i, use_.split_elem, elem_delta));
                                            for uni in &mut cloned.pattern.unify_groups {
                                                uni.token_start = shift(
                                                    uni.token_start,
                                                    use_.split_tok,
                                                    tok_delta,
                                                );
                                                uni.token_end =
                                                    shift(uni.token_end, use_.split_tok, tok_delta);
                                                uni.elem_start = shift(
                                                    uni.elem_start,
                                                    use_.split_elem,
                                                    elem_delta,
                                                );
                                                uni.elem_end = shift(
                                                    uni.elem_end,
                                                    use_.split_elem,
                                                    elem_delta,
                                                );
                                            }
                                            renumber_phrase_refs(
                                                &mut tokens,
                                                &[(use_.split_tok, alt_tokens.len())],
                                            );
                                            cloned.pattern.tokens = tokens;
                                            cloned.pattern.elements = elements;
                                            variants.push(cloned);
                                        }
                                    }
                                }
                                pattern_ref_uses.clear();
                                if let Some(group) = &mut current_group {
                                    group.rules.push(rule);
                                    group.rules.append(&mut variants);
                                } else {
                                    file.rules.push(rule);
                                    file.rules.append(&mut variants);
                                }
                            }
                        }
                        "phrases" => {
                            in_phrases = false;
                        }
                        "phrase" if in_phrases => {
                            let mut alts = std::mem::take(&mut phrase_variants);
                            if alts.is_empty() {
                                alts.push((
                                    std::mem::take(&mut phrase_run_tokens),
                                    std::mem::take(&mut phrase_run_elements),
                                ));
                            } else {
                                for alt in &mut alts {
                                    alt.0.extend(phrase_run_tokens.iter().cloned());
                                    alt.1.extend(phrase_run_elements.iter().cloned());
                                }
                                phrase_run_tokens.clear();
                                phrase_run_elements.clear();
                            }
                            if !phrase_id.is_empty() {
                                phrase_map.insert(phrase_id.clone(), alts);
                            }
                            phrase_after_ref = false;
                        }
                        "phraseref" => {}
                        "pattern" => {
                            if let Some(mut pattern) = current_pattern.take() {
                                if let Some(start) = pattern_marker_start {
                                    pattern.marker_start = Some(start);
                                    pattern.marker_end = pattern_marker_end;
                                    pattern.marker_elem_start = pattern_marker_elem_start;
                                    pattern.marker_elem_end = pattern_marker_elem_end;
                                }
                                if let Some(rule) = &mut current_rule {
                                    rule.pattern = pattern;
                                }
                            }
                            in_pattern_marker = false;
                            pattern_marker_start = None;
                            pattern_marker_end = None;
                            pattern_marker_elem_start = None;
                            pattern_marker_elem_end = None;
                        }
                        "antipattern" => {
                            in_antipattern = false;
                            if let Some(mut ap) = current_antipattern.take() {
                                ap.marker_elem_start = antipattern_marker_elem_start;
                                ap.marker_elem_end = antipattern_marker_elem_end;
                                if let Some(rule) = &mut current_rule {
                                    rule.antipatterns.push(ap);
                                } else if let Some(group) = &mut current_group {
                                    group.antipatterns.push(ap);
                                }
                            }
                            antipattern_marker_elem_start = None;
                            antipattern_marker_elem_end = None;
                        }
                        "exception" => {
                            in_exception = false;
                            if let Some(mut exc) = current_exception.take() {
                                if !exception_text.is_empty() {
                                    exc.text = Some(exception_text.trim().to_string());
                                }
                                if let Some(ref mut token) = current_token {
                                    token.exceptions.push(exc);
                                }
                            }
                        }
                        "token" => {
                            if let Some(mut token) = current_token.take() {
                                let trimmed = current_text.trim();
                                if !trimmed.is_empty() {
                                    token.text = Some(trimmed.to_string());
                                }
                                if current_unification.is_some() && current_pattern.is_none() {
                                    if let Some(uni) = current_unification.as_mut()
                                        && let Some(eq) = uni.equivalences.last_mut()
                                    {
                                        eq.postag = token.postag.clone().unwrap_or_default();
                                        eq.exact = token.postag_regexp.as_deref() != Some("yes");
                                    }
                                } else if in_or_group {
                                    if let Some(ref mut or_group) = current_or_group {
                                        if in_unify_ignore {
                                            token.unify_neutral = true;
                                        }
                                        or_group.alternatives.push(token);
                                    }
                                } else if in_and_group {
                                    if let Some(ref mut and_group) = current_and_group {
                                        if in_unify_ignore {
                                            token.unify_neutral = true;
                                        }
                                        and_group.constraints.push(token);
                                    }
                                } else if in_antipattern {
                                    if in_unify_ignore {
                                        token.unify_neutral = true;
                                    }
                                    if let Some(ref mut ap) = current_antipattern {
                                        for copy in token.min_expansion() {
                                            ap.tokens.push(copy.clone());
                                            ap.elements.push(XmlPatternElement::Token(copy));
                                        }
                                    }
                                } else if in_phrases {
                                    for copy in token.min_expansion() {
                                        phrase_run_tokens.push(copy.clone());
                                        phrase_run_elements.push(XmlPatternElement::Token(copy));
                                    }
                                } else if let Some(pattern) = &mut current_pattern {
                                    if in_unify_ignore {
                                        token.unify_neutral = true;
                                    }
                                    for copy in token.min_expansion() {
                                        pattern.tokens.push(copy.clone());
                                        pattern.elements.push(XmlPatternElement::Token(copy));
                                    }
                                }
                            }
                            current_text.clear();
                        }
                        "message" => {
                            in_message = false;
                            if let Some(rule) = &mut current_rule {
                                rule.message = message_text.trim().to_string();
                                rule.suppress_misspelled = message_suppress;
                            }
                        }
                        "suggestion" => {
                            in_suggestion = false;
                            if let Some(rule) = &mut current_rule {
                                rule.suggestions.push(XmlSuggestion {
                                    text: suggestion_text.trim().to_string(),
                                    parts: suggestion_parts.clone(),
                                    suppress_misspelled: suggestion_suppress,
                                });
                            }
                            suggestion_parts.clear();
                        }
                        "short" => {
                            in_short = false;
                            if let Some(rule) = &mut current_rule {
                                let trimmed = short_text.trim();
                                if !trimmed.is_empty() {
                                    rule.short_message = Some(trimmed.to_string());
                                }
                            }
                        }
                        "url" => {
                            in_url = false;
                            if let Some(rule) = &mut current_rule {
                                let trimmed = url_text.trim();
                                if !trimmed.is_empty() {
                                    rule.url = Some(trimmed.to_string());
                                }
                            }
                        }
                        "example" => {
                            in_example = false;
                            if !in_antipattern {
                                let final_type = example_type.clone();
                                if let Some(rule) = &mut current_rule {
                                    let corrections = example_correction
                                        .as_ref()
                                        .map(|c| {
                                            c.split('|').map(|s| s.trim().to_string()).collect()
                                        })
                                        .unwrap_or_default();
                                    rule.examples.push(XmlExample {
                                        example_type: final_type,
                                        text: example_text.trim().to_string(),
                                        corrections,
                                    });
                                }
                            }
                            example_correction = None;
                        }
                        "marker" => {
                            if in_example {
                                example_text.push_str("</marker>");
                            } else if current_pattern.is_some() {
                                in_pattern_marker = false;
                                if let Some(ref pattern) = current_pattern {
                                    pattern_marker_end = Some(pattern.tokens.len());
                                    pattern_marker_elem_end = Some(pattern.elements.len());
                                }
                            } else if current_antipattern.is_some() {
                                in_pattern_marker = false;
                                if let Some(ref ap) = current_antipattern {
                                    antipattern_marker_elem_end = Some(ap.elements.len());
                                }
                            }
                        }
                        "unification" => {
                            if let Some(uni) = current_unification.take() {
                                unifications.push(uni);
                            }
                            current_equivalence = None;
                        }
                        "equivalence" => {
                            if let Some(eq) = current_equivalence.take()
                                && let Some(uni) = &mut current_unification
                            {
                                uni.equivalences.push(eq);
                            }
                        }
                        "unify-ignore" => {
                            in_unify_ignore = false;
                        }
                        "feature" if unify_start.is_some() => {
                            if !unify_feature_name.is_empty() {
                                unify_features.push((
                                    unify_feature_name.clone(),
                                    unify_feature_types.clone(),
                                ));
                            }
                            unify_feature_name.clear();
                            unify_feature_types.clear();
                        }
                        "unify" => {
                            if let Some(start) = unify_start.take() {
                                let elem_start = unify_elem_start.take().unwrap_or(start);
                                let group = crate::types::XmlUnifyGroup {
                                    negate: unify_negate,
                                    features: unify_features.clone(),
                                    token_start: start,
                                    token_end: 0,
                                    elem_start,
                                    elem_end: 0,
                                };
                                if let Some(pattern) = &mut current_pattern {
                                    pattern.unify_groups.push(crate::types::XmlUnifyGroup {
                                        token_end: pattern.tokens.len(),
                                        elem_end: pattern.elements.len(),
                                        ..group
                                    });
                                } else if let Some(ap) = &mut current_antipattern {
                                    ap.unify_groups.push(crate::types::XmlUnifyGroup {
                                        token_end: ap.tokens.len(),
                                        elem_end: ap.elements.len(),
                                        ..group
                                    });
                                }
                            }
                            unify_features.clear();
                            unify_negate = false;
                        }
                        "or" => {
                            in_or_group = false;
                            if let Some(or_group) = current_or_group.take() {
                                let element = XmlPatternElement::OrGroup(or_group);
                                if in_antipattern {
                                    if let Some(ref mut ap) = current_antipattern {
                                        ap.elements.push(element);
                                    }
                                } else if let Some(ref mut pattern) = current_pattern {
                                    pattern.elements.push(element);
                                }
                            }
                        }
                        "and" => {
                            in_and_group = false;
                            if let Some(and_group) = current_and_group.take() {
                                let element = XmlPatternElement::AndGroup(and_group);
                                if in_antipattern {
                                    if let Some(ref mut ap) = current_antipattern {
                                        ap.elements.push(element);
                                    }
                                } else if let Some(ref mut pattern) = current_pattern {
                                    pattern.elements.push(element);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(XmlParseError::Parse(e)),
                _ => {}
            }
            buf.clear();
        }

        file.unifications = unifications;
        Ok(file)
    }

    fn strip_doctype(xml: &str) -> String {
        if let Some(start) = xml.find("<!DOCTYPE")
            && let Some(end) = xml[start..].find("]>")
        {
            return xml[..start].to_string() + &xml[start + end + 2..];
        }
        xml.to_string()
    }

    pub fn expand_entities(xml: &str) -> String {
        Self::expand_entities_with_base(xml, None)
    }

    pub fn expand_entities_with_base(xml: &str, base_dir: Option<&std::path::Path>) -> String {
        let mut xml = xml.to_string();
        if let Some(base) = base_dir {
            xml = Self::include_external_entities(&xml, base, 0);
        }
        Self::expand_internal_entities(&xml)
    }

    fn include_external_entities(xml: &str, base: &std::path::Path, depth: usize) -> String {
        let decl = regex::Regex::new(r#"<!ENTITY\s+%\s+(\w+)\s+SYSTEM\s+"([^"]+)"\s*>"#).unwrap();
        let mut params: Vec<(String, std::path::PathBuf)> = Vec::new();
        for cap in decl.captures_iter(xml) {
            let rel = cap[2].replace('\\', "/");
            let path = base.join(&rel);
            if path.exists() {
                params.push((cap[1].to_string(), path));
            }
        }
        if params.is_empty() || depth > 8 {
            return xml.to_string();
        }
        let mut out = xml.to_string();
        for (name, path) in params {
            let marker = format!("%{name};");
            if !out.contains(&marker) {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                let expanded = Self::include_external_entities(&content, base, depth + 1);
                out = out.replace(&marker, &expanded);
            }
        }
        out
    }

    fn expand_internal_entities(xml: &str) -> String {
        let double_q = regex::Regex::new(r#"<!ENTITY\s+(\w+)\s+"([^"]*)"\s*>"#).unwrap();
        let single_q = regex::Regex::new(r#"<!ENTITY\s+(\w+)\s+'([^']*)'\s*>"#).unwrap();
        let mut entities: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for cap in double_q.captures_iter(xml) {
            entities.insert(cap[1].to_string(), cap[2].to_string());
        }
        for cap in single_q.captures_iter(xml) {
            entities.insert(cap[1].to_string(), cap[2].to_string());
        }

        fn valid_entity_name(name: &str) -> bool {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
        }

        fn expand_value(
            value: &str,
            entities: &std::collections::HashMap<String, String>,
            memo: &mut std::collections::HashMap<String, String>,
            depth: usize,
        ) -> String {
            if depth > 16 || !value.contains('&') {
                return value.to_string();
            }
            let mut out = String::with_capacity(value.len());
            let mut rest = value;
            while let Some(pos) = rest.find('&') {
                out.push_str(&rest[..pos]);
                let after = &rest[pos..];
                let Some(semi) = after.find(';') else {
                    out.push_str(after);
                    return out;
                };
                let name = &after[1..semi];
                if !valid_entity_name(name) {
                    out.push('&');
                    rest = &after[1..];
                    continue;
                }
                if let Some(expanded) = memo.get(name) {
                    out.push_str(expanded);
                } else if let Some(nested) = entities.get(name) {
                    let expanded = expand_value(nested, entities, memo, depth + 1);
                    memo.insert(name.to_string(), expanded.clone());
                    out.push_str(&expanded);
                } else {
                    out.push_str(&after[..=semi]);
                }
                rest = &after[semi + 1..];
            }
            out.push_str(rest);
            out
        }

        let mut memo: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut result = String::with_capacity(xml.len() + 128 * 1024);
        let mut rest = xml;
        while let Some(pos) = rest.find('&') {
            result.push_str(&rest[..pos]);
            let after = &rest[pos..];
            let Some(semi) = after.find(';') else {
                result.push_str(after);
                break;
            };
            let name = &after[1..semi];
            if !valid_entity_name(name) {
                result.push('&');
                rest = &after[1..];
                continue;
            }
            if let Some(expanded) = memo.get(name) {
                result.push_str(expanded);
            } else if let Some(nested) = entities.get(name) {
                let expanded = expand_value(nested, &entities, &mut memo, 0);
                memo.insert(name.to_string(), expanded.clone());
                result.push_str(&expanded);
            } else {
                result.push_str(&after[..=semi]);
            }
            rest = &after[semi + 1..];
        }
        result.push_str(rest);
        result
    }
}

impl Default for XmlRuleParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_internal_entities_allows_whitespace_before_gt() {
        let xml = "<!ENTITY nombre_CASA_CADA \"casa|cada\" >\n&nombre_CASA_CADA;";
        assert_eq!(
            XmlRuleParser::expand_entities(xml),
            "<!ENTITY nombre_CASA_CADA \"casa|cada\" >\ncasa|cada"
        );
        let xml = "<!ENTITY tight \"a|b\">\n&tight;";
        assert_eq!(
            XmlRuleParser::expand_entities(xml),
            "<!ENTITY tight \"a|b\">\na|b"
        );
        let xml = "<!ENTITY single 'x|y' >\n&single;";
        assert_eq!(
            XmlRuleParser::expand_entities(xml),
            "<!ENTITY single 'x|y' >\nx|y"
        );
    }

    #[test]
    fn test_parse_phrases_and_phraseref_expansion() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="pl">
            <phrases>
                <phrase id="ALT1">
                    <token>foo</token>
                </phrase>
                <phrase id="ALT2">
                    <token>bar</token>
                    <token>baz</token>
                </phrase>
                <phrase id="COMBINED">
                    <phraseref idref="ALT1"></phraseref>
                    <phraseref idref="ALT2"/>
                </phrase>
            </phrases>
            <category id="GRAMMAR" name="Grammar">
                <rule id="REF_RULE" name="Ref rule">
                    <pattern>
                        <token>start</token>
                        <phraseref idref="COMBINED"></phraseref>
                        <token>end</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules.len(), 2);
        let texts: Vec<Vec<Option<&str>>> = result
            .rules
            .iter()
            .map(|r| r.pattern.tokens.iter().map(|t| t.text.as_deref()).collect())
            .collect();
        assert_eq!(
            texts,
            vec![
                vec![Some("start"), Some("foo"), Some("end")],
                vec![Some("start"), Some("bar"), Some("baz"), Some("end")],
            ]
        );
        assert_eq!(result.rules[0].id, "REF_RULE");
        assert_eq!(result.rules[1].id, "REF_RULE");
    }

    #[test]
    fn test_parse_phases_marker_bounds_survive_ref_expansion() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="pl">
            <phrases>
                <phrase id="ONE">
                    <token regexp="yes">a|b</token>
                </phrase>
                <phrase id="TWO">
                    <token>c</token>
                    <token>d</token>
                </phrase>
                <phrase id="EITHER">
                    <phraseref idref="ONE"></phraseref>
                    <phraseref idref="TWO"></phraseref>
                </phrase>
            </phrases>
            <category id="GRAMMAR" name="Grammar">
                <rule id="MARKER_REF" name="Marker ref">
                    <pattern>
                        <token>start</token>
                        <marker>
                            <phraseref idref="EITHER"></phraseref>
                        </marker>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules.len(), 2);
        let first = &result.rules[0];
        assert_eq!(first.pattern.tokens.len(), 2);
        assert_eq!(first.pattern.marker_elem_start, Some(1));
        assert_eq!(first.pattern.marker_elem_end, Some(2));
        let second = &result.rules[1];
        assert_eq!(second.pattern.tokens.len(), 3);
        assert_eq!(second.pattern.marker_elem_start, Some(1));
        assert_eq!(second.pattern.marker_elem_end, Some(3));
    }

    #[test]
    fn test_parse_antipattern_marker_span() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="MARKER_ANTI" name="Marker antipattern">
                    <antipattern>
                        <token>foo</token>
                        <marker>
                            <token>bar</token>
                        </marker>
                        <token>baz</token>
                    </antipattern>
                    <pattern>
                        <marker>
                            <token>bar</token>
                        </marker>
                    </pattern>
                    <message>Test</message>
                </rule>
                <rule id="NO_MARKER_ANTI" name="No marker antipattern">
                    <antipattern>
                        <token>foo</token>
                    </antipattern>
                    <pattern>
                        <token>foo</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let marker_anti = &result.rules[0].antipatterns[0];
        assert_eq!(marker_anti.elements.len(), 3);
        assert_eq!(marker_anti.marker_elem_start, Some(1));
        assert_eq!(marker_anti.marker_elem_end, Some(2));
        let no_marker_anti = &result.rules[1].antipatterns[0];
        assert_eq!(no_marker_anti.marker_elem_start, None);
        assert_eq!(no_marker_anti.marker_elem_end, None);
    }

    #[test]
    fn test_parse_simple_rule() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="TEST_RULE" name="Test rule">
                    <pattern>
                        <token>foo</token>
                        <token>bar</token>
                    </pattern>
                    <message>Use <suggestion>baz</suggestion> instead.</message>
                    <example type="incorrect">This is <marker>foo bar</marker> test.</example>
                    <example type="correct">This is baz test.</example>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.categories.len(), 1);
        assert_eq!(result.categories[0].id, "GRAMMAR");
        assert_eq!(result.rules.len(), 1);
        assert_eq!(result.rules[0].id, "TEST_RULE");
        assert_eq!(result.rules[0].pattern.tokens.len(), 2);
        assert_eq!(
            result.rules[0].pattern.tokens[0].text.as_deref(),
            Some("foo")
        );
        assert_eq!(
            result.rules[0].pattern.tokens[1].text.as_deref(),
            Some("bar")
        );
        assert_eq!(result.rules[0].message, "Use baz instead.");
    }

    #[test]
    fn test_parse_regexp_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="MISC" name="Misc">
                <rule id="REGEX_TEST" name="Regex test">
                    <pattern>
                        <token regexp="yes">foo|bar</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();
        assert_eq!(
            result.rules[0].pattern.tokens[0].regexp.as_deref(),
            Some("yes")
        );
        assert_eq!(
            result.rules[0].pattern.tokens[0].text.as_deref(),
            Some("foo|bar")
        );
    }

    #[test]
    fn test_parse_negate_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="MISC" name="Misc">
                <rule id="NEG_TEST" name="Negate test">
                    <pattern>
                        <token negate="yes">bad</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();
        assert!(result.rules[0].pattern.tokens[0].negate);
    }

    #[test]
    fn test_parse_multiple_rules() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="RULE_ONE" name="First rule">
                    <pattern>
                        <token>apple</token>
                    </pattern>
                    <message>Message one</message>
                </rule>
                <rule id="RULE_TWO" name="Second rule">
                    <pattern>
                        <token>banana</token>
                        <token>cherry</token>
                    </pattern>
                    <message>Message two</message>
                </rule>
                <rule id="RULE_THREE" name="Third rule">
                    <pattern>
                        <token>date</token>
                    </pattern>
                    <message>Message three</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.categories.len(), 1);
        assert_eq!(result.rules.len(), 3);

        assert_eq!(result.rules[0].id, "RULE_ONE");
        assert_eq!(result.rules[0].name, "First rule");
        assert_eq!(result.rules[0].pattern.tokens.len(), 1);
        assert_eq!(
            result.rules[0].pattern.tokens[0].text.as_deref(),
            Some("apple")
        );
        assert_eq!(result.rules[0].message, "Message one");

        assert_eq!(result.rules[1].id, "RULE_TWO");
        assert_eq!(result.rules[1].name, "Second rule");
        assert_eq!(result.rules[1].pattern.tokens.len(), 2);
        assert_eq!(
            result.rules[1].pattern.tokens[0].text.as_deref(),
            Some("banana")
        );
        assert_eq!(
            result.rules[1].pattern.tokens[1].text.as_deref(),
            Some("cherry")
        );
        assert_eq!(result.rules[1].message, "Message two");

        assert_eq!(result.rules[2].id, "RULE_THREE");
        assert_eq!(result.rules[2].name, "Third rule");
        assert_eq!(result.rules[2].pattern.tokens.len(), 1);
        assert_eq!(
            result.rules[2].pattern.tokens[0].text.as_deref(),
            Some("date")
        );
        assert_eq!(result.rules[2].message, "Message three");
    }

    #[test]
    fn test_parse_regexp_token_variations() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="MISC" name="Misc">
                <rule id="REGEX_TEST" name="Regex test">
                    <pattern>
                        <token regexp="yes">foo|bar|baz</token>
                        <token regexp="yes">\d+</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let tokens = &result.rules[0].pattern.tokens;
        assert_eq!(tokens[0].regexp.as_deref(), Some("yes"));
        assert_eq!(tokens[0].text.as_deref(), Some("foo|bar|baz"));
        assert_eq!(tokens[1].regexp.as_deref(), Some("yes"));
        assert_eq!(tokens[1].text.as_deref(), Some("\\d+"));
    }

    #[test]
    fn test_parse_negate_token_variations() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="MISC" name="Misc">
                <rule id="NEG_TEST" name="Negate test">
                    <pattern>
                        <token negate="yes">bad</token>
                        <token negate="no">good</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert!(result.rules[0].pattern.tokens[0].negate);
        assert!(!result.rules[0].pattern.tokens[1].negate);
        assert_eq!(
            result.rules[0].pattern.tokens[1].text.as_deref(),
            Some("good")
        );
    }

    #[test]
    fn test_parse_case_sensitive_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="MISC" name="Misc">
                <rule id="CASE_TEST" name="Case test">
                    <pattern>
                        <token case_sensitive="yes">Hello</token>
                        <token case_sensitive="no">world</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert!(result.rules[0].pattern.tokens[0].case_sensitive);
        assert!(!result.rules[0].pattern.tokens[1].case_sensitive);
    }

    #[test]
    fn test_parse_token_with_exception() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EXC_TEST" name="Exception test">
                    <pattern>
                        <token>color</token>
                        <token>blind</token>
                    </pattern>
                    <message>Use <suggestion>colour</suggestion> instead.</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].suggestions.len(), 1);
        assert_eq!(result.rules[0].suggestions[0].text, "colour");
    }

    #[test]
    fn test_parse_exception_on_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EXC_RULE" name="Exception rule">
                    <pattern>
                        <token regexp="yes" negate="yes">test<exception regexp="yes" negate="yes" scope="next"/></token>
                    </pattern>
                    <message>Test message</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert_eq!(token.exceptions.len(), 1);

        let exc = &token.exceptions[0];
        assert_eq!(exc.regexp.as_deref(), Some("yes"));
        assert!(exc.negate);
        assert_eq!(exc.scope.as_deref(), Some("next"));
    }

    #[test]
    fn test_parse_exception_spacebefore() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EXC_SB" name="Exception spacebefore">
                    <pattern>
                        <token>foo<exception scope="next" spacebefore="no">bar</exception></token>
                        <token>baz<exception spacebefore="yes">quux</exception></token>
                        <token>one<exception spacebefore="ignore">two</exception></token>
                        <token>three<exception>four</exception></token>
                        <token>five<exception spacebefore="no"/></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();
        let tokens = &result.rules[0].pattern.tokens;
        assert_eq!(tokens[0].exceptions[0].space_before, Some(false));
        assert_eq!(tokens[1].exceptions[0].space_before, Some(true));
        assert_eq!(tokens[2].exceptions[0].space_before, None);
        assert_eq!(tokens[3].exceptions[0].space_before, None);
        assert_eq!(tokens[4].exceptions[0].space_before, Some(false));
    }

    #[test]
    fn test_parse_exception_with_postag() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EXC_POSTAG" name="Exception postag">
                    <pattern>
                        <token>word<exception postag="NN" postag_regexp="yes" inflected="yes"/></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let exc = &result.rules[0].pattern.tokens[0].exceptions[0];
        assert_eq!(exc.postag.as_deref(), Some("NN"));
        assert_eq!(exc.postag_regexp.as_deref(), Some("yes"));
        assert!(exc.inflected);
    }

    #[test]
    fn test_parse_exception_inline_text_accumulation() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EXC_ACC" name="Exception accumulation">
                    <pattern>
                        <token>base<exception negate="yes">exc_text</exception></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert_eq!(token.text.as_deref(), Some("base"));
        assert_eq!(token.exceptions.len(), 1);
        assert!(token.exceptions[0].negate);
        assert_eq!(token.exceptions[0].text.as_deref(), Some("exc_text"));
    }

    #[test]
    fn test_parse_multiple_suggestions() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="MULTI_SUG" name="Multi suggestion">
                    <pattern>
                        <token>wrong</token>
                    </pattern>
                    <message>Use <suggestion>option1</suggestion> or <suggestion>option2</suggestion> or <suggestion>option3</suggestion>.</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].suggestions.len(), 3);
        assert_eq!(result.rules[0].suggestions[0].text, "option1");
        assert_eq!(result.rules[0].suggestions[1].text, "option2");
        assert_eq!(result.rules[0].suggestions[2].text, "option3");
        assert_eq!(
            result.rules[0].message,
            "Use option1 or option2 or option3."
        );
    }

    #[test]
    fn test_parse_min_max_on_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="MINMAX_TEST" name="Min max test">
                    <pattern>
                        <token min="0" max="3">optional</token>
                        <token min="1" max="5">ranged</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let tokens = &result.rules[0].pattern.tokens;
        assert_eq!(tokens[0].min, Some(0));
        assert_eq!(tokens[0].max, Some(3));
        assert_eq!(tokens[0].text.as_deref(), Some("optional"));
        assert_eq!(tokens[1].min, Some(1));
        assert_eq!(tokens[1].max, Some(5));
        assert_eq!(tokens[1].text.as_deref(), Some("ranged"));
    }

    #[test]
    fn test_parse_skip_on_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="SKIP_TEST" name="Skip test">
                    <pattern>
                        <token skip="2">first</token>
                        <token>last</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].pattern.tokens[0].skip, 2);
        assert_eq!(
            result.rules[0].pattern.tokens[0].text.as_deref(),
            Some("first")
        );
        assert_eq!(result.rules[0].pattern.tokens[1].skip, 0);
    }

    #[test]
    fn test_parse_rule_group() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rulegroup id="GROUP_A" name="Group A" default="on">
                    <rule id="RULE_G1" name="Grouped rule 1">
                        <pattern>
                            <token>alpha</token>
                        </pattern>
                        <message>Alpha message</message>
                    </rule>
                    <rule id="RULE_G2" name="Grouped rule 2">
                        <pattern>
                            <token>beta</token>
                        </pattern>
                        <message>Beta message</message>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rule_groups.len(), 1);
        let group = &result.rule_groups[0];
        assert_eq!(group.id.as_deref(), Some("GROUP_A"));
        assert_eq!(group.name.as_deref(), Some("Group A"));
        assert_eq!(group.default_on, Some(true));
        assert_eq!(group.rules.len(), 2);

        assert_eq!(group.rules[0].id, "RULE_G1");
        assert_eq!(group.rules[0].name, "Grouped rule 1");
        assert_eq!(
            group.rules[0].pattern.tokens[0].text.as_deref(),
            Some("alpha")
        );
        assert_eq!(group.rules[0].message, "Alpha message");

        assert_eq!(group.rules[1].id, "RULE_G2");
        assert_eq!(group.rules[1].name, "Grouped rule 2");
        assert_eq!(
            group.rules[1].pattern.tokens[0].text.as_deref(),
            Some("beta")
        );
        assert_eq!(group.rules[1].message, "Beta message");
    }

    #[test]
    fn test_parse_rule_group_without_attrs() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rulegroup>
                    <rule id="RULE_IN_GROUP" name="In group">
                        <pattern>
                            <token>word</token>
                        </pattern>
                        <message>Msg</message>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rule_groups.len(), 1);
        let group = &result.rule_groups[0];
        assert_eq!(group.id, None);
        assert_eq!(group.name, None);
        assert_eq!(group.default_on, None);
        assert_eq!(group.rules.len(), 1);
    }

    #[test]
    fn test_parse_multiple_rule_groups() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rulegroup id="GRP1" name="First Group">
                    <rule id="R1" name="R1">
                        <pattern><token>one</token></pattern>
                        <message>M1</message>
                    </rule>
                </rulegroup>
                <rulegroup id="GRP2" name="Second Group">
                    <rule id="R2" name="R2">
                        <pattern><token>two</token></pattern>
                        <message>M2</message>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rule_groups.len(), 2);
        assert_eq!(result.rule_groups[0].id.as_deref(), Some("GRP1"));
        assert_eq!(result.rule_groups[1].id.as_deref(), Some("GRP2"));
    }

    #[test]
    fn test_parse_category_with_all_attrs() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="STYLE" name="Style" description="Stylistic issues" default="on">
                <rule id="STYLE_RULE" name="Style rule">
                    <pattern>
                        <token>very</token>
                        <token>unique</token>
                    </pattern>
                    <message>Avoid this.</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.categories.len(), 1);
        let cat = &result.categories[0];
        assert_eq!(cat.id, "STYLE");
        assert_eq!(cat.name, "Style");
        assert_eq!(cat.description.as_deref(), Some("Stylistic issues"));
        assert!(cat.default_on);
    }

    #[test]
    fn test_parse_category_default_off() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="TYPO" name="Typo" default="off">
                <rule id="TYPO_RULE" name="Typo rule">
                    <pattern><token>misteak</token></pattern>
                    <message>Typo</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert!(!result.categories[0].default_on);
    }

    #[test]
    fn test_parse_category_no_default_attr() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="MISC" name="Misc">
                <rule id="MISC_RULE" name="Misc rule">
                    <pattern><token>word</token></pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert!(!result.categories[0].default_on);
    }

    #[test]
    fn test_parse_example_correct() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EX_CORRECT" name="Correct example">
                    <pattern>
                        <token>wrong</token>
                    </pattern>
                    <message>Use <suggestion>right</suggestion></message>
                    <example type="correct">This is right.</example>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].examples.len(), 1);
        assert_eq!(
            result.rules[0].examples[0].example_type,
            XmlExampleType::Correct
        );
        assert_eq!(result.rules[0].examples[0].text, "This is right.");
    }

    #[test]
    fn test_parse_example_incorrect_with_marker() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EX_INCORRECT" name="Incorrect example">
                    <pattern>
                        <token>wrong</token>
                    </pattern>
                    <message>Use <suggestion>right</suggestion></message>
                    <example type="incorrect">This is <marker>wrong</marker> here.</example>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].examples.len(), 1);
        assert_eq!(
            result.rules[0].examples[0].example_type,
            XmlExampleType::Incorrect
        );
        assert_eq!(
            result.rules[0].examples[0].text,
            "This is <marker>wrong</marker> here."
        );
    }

    #[test]
    fn test_parse_example_marker_alone_stays_correct() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EX_IMPL" name="Implicit incorrect">
                    <pattern>
                        <token>error</token>
                    </pattern>
                    <message>Fix it</message>
                    <example>This has <marker>error</marker> in it.</example>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(
            result.rules[0].examples[0].example_type,
            XmlExampleType::Correct
        );
    }

    #[test]
    fn test_parse_example_no_type_correct_default() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EX_NODEF" name="No type">
                    <pattern><token>word</token></pattern>
                    <message>Msg</message>
                    <example>A plain sentence.</example>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(
            result.rules[0].examples[0].example_type,
            XmlExampleType::Correct
        );
        assert_eq!(result.rules[0].examples[0].text, "A plain sentence.");
    }

    #[test]
    fn test_parse_rule_default_on() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="ON_RULE" name="On rule" default="on">
                    <pattern><token>test</token></pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert!(result.rules[0].default_on);
    }

    #[test]
    fn test_parse_rule_default_off() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="OFF_RULE" name="Off rule" default="off">
                    <pattern><token>test</token></pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert!(!result.rules[0].default_on);
    }

    #[test]
    fn test_parse_rule_no_default_attr() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="NO_DEF_RULE" name="No default attr">
                    <pattern><token>test</token></pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert!(result.rules[0].default_on);
    }

    #[test]
    fn test_parse_rule_deprecated() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="DEP_RULE" name="Deprecated rule" deprecated="yes">
                    <pattern><token>old</token></pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert!(result.rules[0].deprecated);
    }

    #[test]
    fn test_parse_multiple_pattern_tokens() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="MULTI_TOK" name="Multi token">
                    <pattern>
                        <token>I</token>
                        <token>am</token>
                        <token regexp="yes">go(?:ing|ne)</token>
                        <token negate="yes">to</token>
                        <token case_sensitive="yes">Store</token>
                    </pattern>
                    <message>Check</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let tokens = &result.rules[0].pattern.tokens;
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0].text.as_deref(), Some("I"));
        assert_eq!(tokens[1].text.as_deref(), Some("am"));
        assert_eq!(tokens[2].regexp.as_deref(), Some("yes"));
        assert_eq!(tokens[2].text.as_deref(), Some("go(?:ing|ne)"));
        assert!(tokens[3].negate);
        assert_eq!(tokens[3].text.as_deref(), Some("to"));
        assert!(tokens[4].case_sensitive);
        assert_eq!(tokens[4].text.as_deref(), Some("Store"));
    }

    #[test]
    fn test_parse_empty_pattern() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EMPTY_PAT" name="Empty pattern">
                    <pattern>
                    </pattern>
                    <message>Empty</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].pattern.tokens.len(), 0);
    }

    #[test]
    fn test_parse_postag_constraint() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="POSTAG_TEST" name="Postag test">
                    <pattern>
                        <token postag="JJ">big</token>
                        <token postag="NN">dog</token>
                    </pattern>
                    <message>Check</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let tokens = &result.rules[0].pattern.tokens;
        assert_eq!(tokens[0].postag.as_deref(), Some("JJ"));
        assert_eq!(tokens[0].text.as_deref(), Some("big"));
        assert_eq!(tokens[1].postag.as_deref(), Some("NN"));
        assert_eq!(tokens[1].text.as_deref(), Some("dog"));
    }

    #[test]
    fn test_parse_postag_regexp_constraint() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="POSTAG_RE_TEST" name="Postag regexp test">
                    <pattern>
                        <token postag="NN.*" postag_regexp="yes">word</token>
                    </pattern>
                    <message>Check</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert_eq!(token.postag.as_deref(), Some("NN.*"));
        assert_eq!(token.postag_regexp.as_deref(), Some("yes"));
    }

    #[test]
    fn test_parse_inflected_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="INFL_TEST" name="Inflected test">
                    <pattern>
                        <token inflected="yes">run</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert!(result.rules[0].pattern.tokens[0].inflected);
        assert_eq!(
            result.rules[0].pattern.tokens[0].text.as_deref(),
            Some("run")
        );
    }

    #[test]
    fn test_parse_pattern_case_sensitive() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="PAT_CASE" name="Pattern case">
                    <pattern case_sensitive="yes">
                        <token>Hello</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert!(result.rules[0].pattern.case_sensitive);
    }

    #[test]
    fn test_parse_self_closing_token() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="SELF_CLOSE" name="Self closing">
                    <pattern>
                        <token negate="yes"/>
                        <token>word</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let tokens = &result.rules[0].pattern.tokens;
        assert_eq!(tokens.len(), 2);
        assert!(tokens[0].negate);
        assert_eq!(tokens[0].text, None);
        assert_eq!(tokens[1].text.as_deref(), Some("word"));
    }

    #[test]
    fn test_parse_multiple_examples() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="MULTI_EX" name="Multiple examples">
                    <pattern><token>error</token></pattern>
                    <message>Fix <suggestion>correct</suggestion></message>
                    <example type="incorrect"><marker>error</marker> one.</example>
                    <example type="incorrect"><marker>error</marker> two.</example>
                    <example type="correct">correct one.</example>
                    <example type="correct">correct two.</example>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].examples.len(), 4);
        assert_eq!(
            result.rules[0].examples[0].example_type,
            XmlExampleType::Incorrect
        );
        assert_eq!(
            result.rules[0].examples[1].example_type,
            XmlExampleType::Incorrect
        );
        assert_eq!(
            result.rules[0].examples[2].example_type,
            XmlExampleType::Correct
        );
        assert_eq!(
            result.rules[0].examples[3].example_type,
            XmlExampleType::Correct
        );
    }

    #[test]
    fn test_parse_mixed_rules_and_groups() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="STANDALONE" name="Standalone">
                    <pattern><token>standalone</token></pattern>
                    <message>Standalone msg</message>
                </rule>
                <rulegroup id="GRP" name="Group">
                    <rule id="GROUPED_1" name="Grouped 1">
                        <pattern><token>grouped1</token></pattern>
                        <message>G1 msg</message>
                    </rule>
                    <rule id="GROUPED_2" name="Grouped 2">
                        <pattern><token>grouped2</token></pattern>
                        <message>G2 msg</message>
                    </rule>
                </rulegroup>
                <rule id="ANOTHER_STANDALONE" name="Another standalone">
                    <pattern><token>another</token></pattern>
                    <message>Another msg</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules.len(), 2);
        assert_eq!(result.rule_groups.len(), 1);
        assert_eq!(result.rules[0].id, "STANDALONE");
        assert_eq!(result.rules[1].id, "ANOTHER_STANDALONE");
        assert_eq!(result.rule_groups[0].rules.len(), 2);
    }

    #[test]
    fn test_parse_multiple_categories() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar" default="on">
                <rule id="GRAM_RULE" name="Grammar rule">
                    <pattern><token>gram</token></pattern>
                    <message>Grammar msg</message>
                </rule>
            </category>
            <category id="STYLE" name="Style" default="off">
                <rule id="STYLE_RULE" name="Style rule">
                    <pattern><token>style</token></pattern>
                    <message>Style msg</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.categories.len(), 2);
        assert_eq!(result.categories[0].id, "GRAMMAR");
        assert!(result.categories[0].default_on);
        assert_eq!(result.categories[1].id, "STYLE");
        assert!(!result.categories[1].default_on);
        assert_eq!(result.rules.len(), 2);
    }

    #[test]
    fn test_parse_category_propagation_to_rule() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="CONFUSION" name="Confusion Words">
                <rule id="CONFUSE_RULE" name="Confusion">
                    <pattern><token>their</token></pattern>
                    <message>Check</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].category.id, "CONFUSION");
        assert_eq!(result.rules[0].category.name, "Confusion Words");
    }

    #[test]
    fn test_parse_token_with_all_attrs() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="ALL_ATTR" name="All attrs">
                    <pattern>
                        <token regexp="yes" negate="yes" case_sensitive="yes" inflected="yes" postag="VB.*" postag_regexp="yes" min="0" max="2" skip="1">test_pattern</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert_eq!(token.regexp.as_deref(), Some("yes"));
        assert!(token.negate);
        assert!(token.case_sensitive);
        assert!(token.inflected);
        assert_eq!(token.postag.as_deref(), Some("VB.*"));
        assert_eq!(token.postag_regexp.as_deref(), Some("yes"));
        assert_eq!(token.min, Some(0));
        assert_eq!(token.max, Some(2));
        assert_eq!(token.skip, 1);
        assert_eq!(token.text.as_deref(), Some("test_pattern"));
    }

    #[test]
    fn test_parse_empty_message() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="EMPTY_MSG" name="Empty message">
                    <pattern><token>word</token></pattern>
                    <message></message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].message, "");
    }

    #[test]
    fn test_parse_rule_with_sub_id() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="PARENT_RULE" name="Parent">
                    <pattern><token>word</token></pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].sub_id, None);
    }

    #[test]
    fn test_parse_exception_multiple() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rule id="MULTI_EXC" name="Multi exception">
                    <pattern>
                        <token>base<exception negate="yes"/><exception scope="next"/></token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert_eq!(token.text.as_deref(), Some("base"));
        assert_eq!(token.exceptions.len(), 2);
        assert!(token.exceptions[0].negate);
        assert_eq!(token.exceptions[1].scope.as_deref(), Some("next"));
    }

    #[test]
    fn test_parse_group_default_off() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rulegroup id="OFF_GRP" name="Off Group" default="off">
                    <rule id="OFF_GRP_RULE" name="Off Grouped Rule">
                        <pattern><token>word</token></pattern>
                        <message>Msg</message>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rule_groups[0].default_on, Some(false));
    }

    #[test]
    fn test_parse_no_rules() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="EMPTY" name="Empty">
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.categories.len(), 1);
        assert_eq!(result.rules.len(), 0);
        assert_eq!(result.rule_groups.len(), 0);
    }

    #[test]
    fn test_parse_rule_inherits_category() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="CAT_A" name="Category A" description="Desc A">
                <rule id="RULE_CAT" name="Cat rule">
                    <pattern><token>test</token></pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let rule = &result.rules[0];
        assert_eq!(rule.category.id, "CAT_A");
        assert_eq!(rule.category.name, "Category A");
        assert_eq!(rule.category.description.as_deref(), Some("Desc A"));
    }

    #[test]
    fn test_parse_grouped_rule_inherits_category() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="CAT_B" name="Category B">
                <rulegroup id="RG" name="RG">
                    <rule id="GRP_RULE_CAT" name="Grouped cat rule">
                        <pattern><token>test</token></pattern>
                        <message>Msg</message>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let rule = &result.rule_groups[0].rules[0];
        assert_eq!(rule.category.id, "CAT_B");
        assert_eq!(rule.category.name, "Category B");
    }

    #[test]
    fn test_parse_self_closing_unify_feature() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="ru">
            <category id="CAT_C" name="Category C">
                <rule id="UNIFY_SC" name="Unify self-closing feature">
                    <pattern>
                        <unify negate="yes">
                            <feature id="case"/>
                            <token postag="ADJ:.*" postag_regexp="yes"/>
                            <token postag="NN:.*" postag_regexp="yes"/>
                        </unify>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let groups = &result.rules[0].pattern.unify_groups;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].features, vec![("case".to_string(), Vec::new())]);
        assert!(groups[0].negate);
    }

    #[test]
    fn test_parse_self_closing_unify_type_inside_open_feature() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="pl">
            <category id="CAT_D" name="Category D">
                <rule id="UNIFY_SC_TYPE" name="Unify self-closing type">
                    <pattern>
                        <unify>
                            <feature id="number"><type id="sg"/></feature>
                            <token postag="NN:.*" postag_regexp="yes"/>
                            <token postag="NN:.*" postag_regexp="yes"/>
                        </unify>
                    </pattern>
                    <message>Msg</message>
                </rule>
            </category>
        </rules>"#;

        let parser = XmlRuleParser::new();
        let result = parser.parse(xml).unwrap();

        let groups = &result.rules[0].pattern.unify_groups;
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].features,
            vec![("number".to_string(), vec!["sg".to_string()])]
        );
    }

    #[test]
    fn test_parse_rule_metadata_elements_and_attrs() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar" type="grammar" description="Grammar issues">
                <rule id="META" name="Meta" type="typographical" tags="prio:1 beta">
                    <pattern>
                        <token>foo</token>
                    </pattern>
                    <message>Test</message>
                    <short>Short msg</short>
                    <url>https://example.org/rule</url>
                    <filter class="ExampleFilter" args="arg1 arg2"/>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        assert_eq!(result.categories.len(), 1);
        assert_eq!(result.categories[0].issue_type.as_deref(), Some("grammar"));
        assert_eq!(
            result.categories[0].description.as_deref(),
            Some("Grammar issues")
        );
        let rule = &result.rules[0];
        assert_eq!(rule.issue_type.as_deref(), Some("typographical"));
        assert_eq!(rule.tags, vec!["prio:1".to_string(), "beta".to_string()]);
        assert_eq!(rule.short_message.as_deref(), Some("Short msg"));
        assert_eq!(rule.url.as_deref(), Some("https://example.org/rule"));
        let filter = rule.filter.as_ref().expect("filter");
        assert_eq!(filter.class, "ExampleFilter");
        assert_eq!(filter.args, "arg1 arg2");
    }

    #[test]
    fn test_parse_rulegroup_type_tags_and_inheritance() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <rulegroup id="GRP" name="Group" type="colloquialisms" tags="alpha">
                <rule id="A" name="A">
                    <pattern><token>a</token></pattern>
                    <message>m</message>
                </rule>
                <rule id="B" name="B" tags="beta">
                    <pattern><token>b</token></pattern>
                    <message>m</message>
                </rule>
            </rulegroup>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        assert_eq!(result.rule_groups.len(), 1);
        let group = &result.rule_groups[0];
        assert_eq!(group.id.as_deref(), Some("GRP"));
        assert_eq!(group.issue_type.as_deref(), Some("colloquialisms"));
        assert_eq!(group.tags, vec!["alpha".to_string()]);
        assert_eq!(group.rules[0].tags, vec!["alpha".to_string()]);
        assert_eq!(
            group.rules[1].tags,
            vec!["beta".to_string(), "alpha".to_string()]
        );
    }

    #[test]
    fn test_parse_rule_level_regexp_and_token_edge_attrs() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="C" name="c">
                <rule id="RE" name="Re">
                    <regexp case_sensitive="yes" type="exact" mark="1">\bfoo\b</regexp>
                    <message>Test</message>
                </rule>
                <rule id="ATTRS" name="Attrs">
                    <pattern raw_pos="yes">
                        <token spacebefore="yes" chunk="B-NP" chunk_re="B-.*">word</token>
                        <token min="2" max="3" skip="-1" postag="NN" regexp="no">next</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        let rr = result.rules[0].rule_regexp.as_ref().expect("rule regexp");
        assert_eq!(rr.text, r"\bfoo\b");
        assert!(rr.case_sensitive);
        assert!(rr.exact);
        assert_eq!(rr.mark, 1);
        let rule = &result.rules[1];
        assert!(rule.pattern.raw_pos);
        let first = &rule.pattern.tokens[0];
        assert_eq!(first.space_before.as_deref(), Some("yes"));
        assert_eq!(first.chunk.as_deref(), Some("B-NP"));
        assert_eq!(first.chunk_re.as_deref(), Some("B-.*"));
        let second = &rule.pattern.tokens[1];
        assert_eq!(second.text.as_deref(), Some("next"));
        assert_eq!(second.min, None);
        assert_eq!(second.max, Some(3));
        assert_eq!(second.skip, -1);
        assert_eq!(rule.pattern.tokens.len(), 3);
        assert_eq!(rule.pattern.tokens[2].text.as_deref(), Some("next"));
    }

    #[test]
    fn test_parse_match_elements_in_token_and_suggestion() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="C" name="c">
                <rule id="SUG" name="Sug">
                    <pattern>
                        <token>foo<match no="1" postag="JJ" postag_regexp="yes" postag_replace="NN" setpos="yes" regexp_match="f(o+)" regexp_replace="$1" case_conversion="startlower"/></token>
                    </pattern>
                    <message suppress_misspelled="yes">Use <match no="1" case_conversion="startupper" include_inflected="yes" include_skipped="follow" postag="NN" postag_regexp="yes" postag_replace="NNP" regexp_match=".*" regexp_replace="x"/> instead.</message>
                    <suggestion suppress_misspelled="yes"><match no="2"/> bar</suggestion>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        let rule = &result.rules[0];
        assert!(rule.suppress_misspelled);
        let token = &rule.pattern.tokens[0];
        assert_eq!(token.text.as_deref(), Some("foo\\1"));
        assert_eq!(token.match_no, Some(1));
        assert_eq!(token.match_no_postag.as_deref(), Some("JJ"));
        assert!(token.match_no_postag_regexp);
        assert_eq!(token.match_no_postag_replace.as_deref(), Some("NN"));
        assert!(token.match_no_setpos);
        assert_eq!(token.match_no_regexp_match.as_deref(), Some("f(o+)"));
        assert_eq!(token.match_no_regexp_replace.as_deref(), Some("$1"));
        assert_eq!(
            token.match_no_case_conversion.as_deref(),
            Some("startlower")
        );
        assert_eq!(rule.suggestions.len(), 1);
        assert!(rule.suggestions[0].suppress_misspelled);
        assert!(matches!(
            &rule.suggestions[0].parts[0],
            SuggestionPart::Match(m) if m.no == 2
        ));
        assert!(matches!(
            &rule.suggestions[0].parts[1],
            SuggestionPart::Text(t) if t == " bar"
        ));
    }

    #[test]
    fn test_parse_self_closing_token_attributes() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="C" name="c">
                <rule id="EMPTYTOK" name="Empty token">
                    <pattern>
                        <token foo="bar"></token>
                        <token regexp="yes" negate="yes" negate_pos="yes" case_sensitive="yes" inflected="yes" postag="NN" postag_regexp="yes" min="1" max="2" skip="1" spacebefore="yes" chunk="B-NP" chunk_re="B-.*" foo="x">word</token>
                    </pattern>
                    <message>Test</message>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[1];
        assert_eq!(token.text.as_deref(), Some("word"));
        assert!(token.regexp.as_deref() == Some("yes"));
        assert!(token.negate);
        assert!(token.negate_pos);
        assert!(token.case_sensitive && token.case_sensitive_set);
        assert!(token.inflected);
        assert_eq!(token.postag.as_deref(), Some("NN"));
        assert_eq!(token.postag_regexp.as_deref(), Some("yes"));
        assert_eq!(token.min, Some(1));
        assert_eq!(token.max, Some(2));
        assert_eq!(token.skip, 1);
        assert_eq!(token.space_before.as_deref(), Some("yes"));
        assert_eq!(token.chunk.as_deref(), Some("B-NP"));
        assert_eq!(token.chunk_re.as_deref(), Some("B-.*"));
    }

    #[test]
    fn test_parse_match_elements_with_explicit_close_tags() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="C" name="c">
                <rule id="MSTART" name="M">
                    <pattern>
                        <token>foo<match no="1" postag="JJ" postag_regexp="yes" postag_replace="NN" setpos="yes" regexp_match="f(o+)" regexp_replace="$1" case_conversion="startlower"></match></token>
                    </pattern>
                    <message>Test</message>
                    <suggestion suppress_misspelled="yes"><match no="2" case_conversion="startupper" include_inflected="yes" include_skipped="follow" postag="NN" postag_regexp="yes" postag_replace="NNP" regexp_match=".*" regexp_replace="x"></match> bar</suggestion>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        let rule = &result.rules[0];
        let token = &rule.pattern.tokens[0];
        assert_eq!(token.text.as_deref(), Some("foo\\1"));
        assert_eq!(token.match_no, Some(1));
        assert_eq!(token.match_no_postag.as_deref(), Some("JJ"));
        assert!(token.match_no_postag_regexp);
        assert_eq!(token.match_no_postag_replace.as_deref(), Some("NN"));
        assert!(token.match_no_setpos);
        assert_eq!(token.match_no_regexp_match.as_deref(), Some("f(o+)"));
        assert_eq!(token.match_no_regexp_replace.as_deref(), Some("$1"));
        assert_eq!(
            token.match_no_case_conversion.as_deref(),
            Some("startlower")
        );
        assert_eq!(rule.suggestions.len(), 1);
        assert!(rule.suggestions[0].suppress_misspelled);
        let SuggestionPart::Match(m) = &rule.suggestions[0].parts[0] else {
            panic!("expected match part");
        };
        assert_eq!(m.no, 2);
        assert_eq!(m.case_conversion.as_deref(), Some("startupper"));
        assert!(m.include_inflected);
        assert_eq!(m.include_skipped.as_deref(), Some("follow"));
        assert_eq!(m.postag.as_deref(), Some("NN"));
        assert!(m.postag_regexp);
        assert_eq!(m.postag_replace.as_deref(), Some("NNP"));
        assert_eq!(m.regexp_match.as_deref(), Some(".*"));
        assert_eq!(m.regexp_replace.as_deref(), Some("x"));
        assert!(matches!(
            &rule.suggestions[0].parts[1],
            SuggestionPart::Text(t) if t == " bar"
        ));
    }

    #[test]
    fn test_parse_example_correction_attr() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="C" name="c">
                <rule id="EX" name="Ex">
                    <pattern><token>foo</token></pattern>
                    <message>Test</message>
                    <example type="incorrect" correction="bar|baz">A foo sentence.</example>
                    <example correction="qux">Another foo.</example>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        let examples = &result.rules[0].examples;
        assert_eq!(examples.len(), 2);
        assert_eq!(
            examples[0].corrections,
            vec!["bar".to_string(), "baz".to_string()]
        );
        assert_eq!(examples[1].example_type, XmlExampleType::Incorrect);
        assert_eq!(examples[1].corrections, vec!["qux".to_string()]);
    }

    #[test]
    fn test_parse_unification_equivalences_with_token_postags() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="pl">
            <unification feature="gender">
                <equivalence type="masc" postag="NN:.*" postag_regexp="no">
                    <token postag="NN:.*"/>
                </equivalence>
                <equivalence type="fem">
                    <token postag="NN" postag_regexp="yes"> word </token>
                </equivalence>
            </unification>
            <category id="C" name="c">
                <rule id="U" name="u">
                    <pattern><token>t</token></pattern>
                    <message>m</message>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        assert_eq!(result.unifications.len(), 1);
        let uni = &result.unifications[0];
        assert_eq!(uni.feature, "gender");
        assert_eq!(uni.equivalences.len(), 2);
        assert_eq!(uni.equivalences[0].eq_type, "masc");
        assert_eq!(uni.equivalences[0].postag, "NN:.*");
        assert!(uni.equivalences[0].exact);
        assert_eq!(uni.equivalences[1].eq_type, "fem");
        assert_eq!(uni.equivalences[1].postag, "NN");
        assert!(!uni.equivalences[1].exact);
    }

    #[test]
    fn test_parse_unification_token_negate_pos_and_phrase_run_ref() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="pl">
            <unification feature="gender">
                <equivalence type="other">
                    <token postag="NN" postag_regexp="yes" negate_pos="yes"/>
                </equivalence>
            </unification>
            <phrases>
                <phrase id="ONE"><token>a</token></phrase>
                <phrase id="PRE_REF"><token>pre</token><phraseref idref="ONE"></phraseref></phrase>
            </phrases>
            <category id="C" name="c">
                <rule id="U" name="u">
                    <pattern>
                        <token>t</token>
                        <phraseref idref="PRE_REF"/>
                    </pattern>
                    <message>m</message>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        let uni = &result.unifications[0];
        assert_eq!(uni.equivalences[0].eq_type, "other");
        assert_eq!(uni.equivalences[0].postag, "NN");
        assert!(!uni.equivalences[0].exact);

        let texts: Vec<Option<&str>> = result.rules[0]
            .pattern
            .tokens
            .iter()
            .map(|t| t.text.as_deref())
            .collect();
        assert_eq!(texts, vec![Some("t"), Some("pre"), Some("a"), Some("pre")]);
    }

    #[test]
    fn test_parse_unify_negate_features_or_and_ignore_groups() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="C" name="c">
                <rule id="UNI" name="uni">
                    <pattern>
                        <unify negate="yes">
                            <feature id="gender"><type id="masc"/><type id="neut"/></feature>
                            <feature id="number"><type id="sg"/></feature>
                            <or>
                                <token>a</token>
                                <token>b</token>
                            </or>
                            <and>
                                <token postag="NN"/>
                                <token postag="VB"/>
                            </and>
                            <unify-ignore><token>skip</token></unify-ignore>
                            <token>c</token>
                        </unify>
                    </pattern>
                    <message>m</message>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();
        let pattern = &result.rules[0].pattern;

        assert_eq!(pattern.unify_groups.len(), 1);
        let group = &pattern.unify_groups[0];
        assert!(group.negate);
        assert_eq!(
            group.features,
            vec![
                (
                    "gender".to_string(),
                    vec!["masc".to_string(), "neut".to_string()]
                ),
                ("number".to_string(), vec!["sg".to_string()])
            ]
        );
        let or_group = pattern
            .elements
            .iter()
            .find_map(|e| match e {
                XmlPatternElement::OrGroup(g) if g.alternatives.len() == 2 => Some(g),
                _ => None,
            })
            .expect("or group");
        let or_texts: Vec<Option<&str>> = or_group
            .alternatives
            .iter()
            .map(|t| t.text.as_deref())
            .collect();
        assert_eq!(or_texts, vec![Some("a"), Some("b")]);
        assert!(pattern.elements.iter().any(|e| matches!(
            e,
            XmlPatternElement::AndGroup(g) if g.constraints.len() == 2
        )));
        assert!(
            pattern
                .tokens
                .iter()
                .any(|t| t.text.as_deref() == Some("skip") && t.unify_neutral)
        );
        assert!(
            pattern
                .tokens
                .iter()
                .any(|t| t.text.as_deref() == Some("c"))
        );
    }

    #[test]
    fn test_parse_unify_and_groups_inside_antipattern() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="C" name="c">
                <rule id="AP" name="ap">
                    <antipattern>
                        <unify negate="yes">
                            <feature id="gender"><type id="fem"/></feature>
                            <token>foo</token>
                        </unify>
                        <or><token>a</token></or>
                        <and><token>b</token></and>
                        <token>tail</token>
                        <token postag="NN"/>
                    </antipattern>
                    <pattern><token>bar</token></pattern>
                    <message>m</message>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();
        let rule = &result.rules[0];

        assert_eq!(rule.antipatterns.len(), 1);
        let ap = &rule.antipatterns[0];
        assert_eq!(ap.unify_groups.len(), 1);
        assert!(ap.unify_groups[0].negate);
        assert_eq!(
            ap.unify_groups[0].features,
            vec![("gender".to_string(), vec!["fem".to_string()])]
        );
        assert!(ap.elements.iter().any(|e| matches!(
            e,
            XmlPatternElement::OrGroup(g) if g.alternatives.len() == 1
        )));
        assert!(ap.elements.iter().any(|e| matches!(
            e,
            XmlPatternElement::AndGroup(g) if g.constraints.len() == 1
        )));
        let texts: Vec<Option<&str>> = ap.tokens.iter().map(|t| t.text.as_deref()).collect();
        assert!(texts.contains(&Some("foo")));
        assert!(texts.contains(&Some("tail")));
        assert!(ap.tokens.iter().any(|t| t.postag.as_deref() == Some("NN")));
    }

    #[test]
    fn test_parse_phrase_merge_and_antipattern_phraseref() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <phrases>
                <phrase id="ONE"><token>a</token></phrase>
                <phrase id="MIX"><token>pre</token><phraseref idref="ONE"/></phrase>
                <phrase id="EITHER"><phraseref idref="ONE"></phraseref><phraseref idref="MIX"/></phrase>
            </phrases>
            <category id="C" name="c">
                <rule id="R" name="r">
                    <antipattern><phraseref idref="ONE"/></antipattern>
                    <antipattern><phraseref idref="MIX"/></antipattern>
                    <pattern>
                        <token>lead</token>
                        <phraseref idref="EITHER"/>
                        <token>tail</token>
                    </pattern>
                    <message>m</message>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        assert_eq!(result.rules.len(), 2);
        let texts: Vec<Vec<Option<&str>>> = result
            .rules
            .iter()
            .map(|r| r.pattern.tokens.iter().map(|t| t.text.as_deref()).collect())
            .collect();
        assert_eq!(
            texts,
            vec![
                vec![Some("lead"), Some("a"), Some("tail")],
                vec![
                    Some("lead"),
                    Some("pre"),
                    Some("a"),
                    Some("pre"),
                    Some("tail")
                ],
            ]
        );
        for rule in &result.rules {
            assert_eq!(rule.antipatterns.len(), 2);
            let ap_texts: Vec<Option<&str>> = rule.antipatterns[0]
                .tokens
                .iter()
                .map(|t| t.text.as_deref())
                .collect();
            assert_eq!(ap_texts, vec![Some("a")]);
            let mix_texts: Vec<Option<&str>> = rule.antipatterns[1]
                .tokens
                .iter()
                .map(|t| t.text.as_deref())
                .collect();
            assert_eq!(mix_texts, vec![Some("pre"), Some("a"), Some("pre")]);
        }
    }

    #[test]
    fn test_parse_phraseref_renumbers_matches_and_shifts_unify() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <phrases>
                <phrase id="A">
                    <token>z</token>
                    <token>y<match no="2"/></token>
                </phrase>
                <phrase id="C"><token>w</token></phrase>
                <phrase id="ALT"><phraseref idref="A"></phraseref><phraseref idref="C"/></phrase>
            </phrases>
            <category id="C2" name="c2">
                <rule id="RN" name="rn">
                    <pattern>
                        <token>lead</token>
                        <phraseref idref="ALT"/>
                        <unify>
                            <feature id="gender"><type id="masc"/></feature>
                            <token>x<match no="1"/></token>
                        </unify>
                    </pattern>
                    <message>m</message>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        assert_eq!(result.rules.len(), 2);
        let base = &result.rules[0];
        let base_texts: Vec<Option<&str>> = base
            .pattern
            .tokens
            .iter()
            .map(|t| t.text.as_deref())
            .collect();
        assert_eq!(
            base_texts,
            vec![Some("lead"), Some("z"), Some("y\\3"), Some("x\\1")]
        );
        let y = &base.pattern.tokens[2];
        assert_eq!(y.match_no, Some(3));
        assert_eq!(y.text.as_deref(), Some("y\\3"));
        assert_eq!(base.pattern.unify_groups[0].token_start, 3);
        assert_eq!(base.pattern.unify_groups[0].token_end, 4);

        let clone = &result.rules[1];
        let clone_texts: Vec<Option<&str>> = clone
            .pattern
            .tokens
            .iter()
            .map(|t| t.text.as_deref())
            .collect();
        assert_eq!(clone_texts, vec![Some("lead"), Some("w"), Some("x\\1")]);
        assert_eq!(clone.pattern.unify_groups[0].token_start, 2);
        assert_eq!(clone.pattern.unify_groups[0].token_end, 3);
    }

    #[test]
    fn test_parse_strips_doctype_and_expands_internal_entities() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE rules [\n<!ENTITY greet \"hello\">\n]>\n<rules lang=\"en\"><category id=\"C\" name=\"c\"><rule id=\"D\" name=\"d\"><pattern><token>&greet;</token></pattern><message>m</message><url/><weird/></rule></category></rules>";

        let result = XmlRuleParser::default().parse(xml).unwrap();

        assert_eq!(
            result.rules[0].pattern.tokens[0].text.as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn test_expand_internal_entities_nested_invalid_and_unknown() {
        let expanded = XmlRuleParser::expand_entities(
            "<!ENTITY a \"x&b;\">\n<!ENTITY b \"y\">\n&a;&a;&b;&unknown;",
        );
        assert_eq!(
            expanded,
            "<!ENTITY a \"xy\">\n<!ENTITY b \"y\">\nxyxyy&unknown;"
        );
        assert_eq!(
            XmlRuleParser::expand_entities("&bad name; more"),
            "&bad name; more"
        );
        assert_eq!(XmlRuleParser::expand_entities("a & b"), "a & ba & b");

        let cyclic = XmlRuleParser::expand_entities("<!ENTITY cyc \"z&cyc;end\">\n&cyc;");
        let usage = cyclic.rsplit('\n').next().unwrap();
        assert!(usage.starts_with('z'));
        assert!(usage.matches('z').count() >= 16);
        assert!(usage.ends_with("end"));
    }

    #[test]
    fn test_expand_external_entities_with_base_dir() {
        let dir = std::env::temp_dir().join("og_xml_entity_tests");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ent.dtd"), "<!ENTITY foo \"bar\">").unwrap();
        std::fs::write(
            dir.join("self.dtd"),
            "<!ENTITY % self SYSTEM \"self.dtd\">\n%self;",
        )
        .unwrap();

        let with_marker = "<!ENTITY % ext SYSTEM \"ent.dtd\">\n%ext;\n&foo;";
        assert_eq!(
            XmlRuleParser::expand_entities_with_base(with_marker, Some(&dir)),
            "<!ENTITY % ext SYSTEM \"ent.dtd\">\n<!ENTITY foo \"bar\">\nbar"
        );
        assert_eq!(
            XmlRuleParser::expand_entities_with_base(with_marker, None),
            with_marker
        );

        let unused = "<!ENTITY % ext SYSTEM \"ent.dtd\">\n&foo;";
        assert_eq!(
            XmlRuleParser::expand_entities_with_base(unused, Some(&dir)),
            unused
        );

        let self_ref = "<!ENTITY % ext SYSTEM \"self.dtd\">\n%ext;";
        let expanded = XmlRuleParser::expand_entities_with_base(self_ref, Some(&dir));
        assert!(expanded.matches("%self;").count() >= 1);
        assert!(expanded.matches("<!ENTITY").count() >= 8);
    }

    #[test]
    fn test_parse_reports_reader_errors() {
        assert!(XmlRuleParser::new().parse("<rules").is_err());
        assert!(
            XmlRuleParser::new()
                .parse("<rules lang=\"en\"></rules>")
                .is_ok()
        );
    }

    #[test]
    fn test_parse_rule_level_regexp_antipattern_and_token_attrs() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <phrases>
                <phrase id="HEAD">
                    <token>alpha</token>
                </phrase>
                <phrase id="MIX">
                    <phraseref idref="HEAD"></phraseref>
                    <token>tail</token>
                    <token spacebefore="yes" chunk="I-NP"/>
                </phrase>
            </phrases>
            <unification feature="gender">
                <equivalence type="masc">
                    <token postag="NN:M"></token>
                </equivalence>
            </unification>
            <category id="C" name="c">
                <rule id="R0" name="r0">
                    <regexp case_sensitive="yes" type="exact" mark="1">warn</regexp>
                </rule>
                <rule id="R0B" name="r0b">
                    <regexp case_sensitive="no" type="hello" mark="3"/>
                </rule>
                <rule id="R1" name="r">
                    <antipattern case_sensitive="yes">
                        <token>beta</token>
                        <phraseref idref="MIX"/>
                    </antipattern>
                    <pattern>
                        <token spacebefore="yes" chunk="B-NP" chunk_re="B-.*" skip="1" min="1" max="2"/>
                        <token>word</token>
                    </pattern>
                    <message>Use <suggestion><match no="1" case_conversion="startlower" regexp_match="a" regexp_replace="b" include_inflected="yes" include_skipped="yes" postag_replace="NN"/></suggestion>.</message>
                </rule>
                <rule id="R2" name="r2">
                    <pattern>
                        <unify>
                            <feature id="gender"><type id="masc"></type></feature>
                            <token>a</token>
                            <token>b</token>
                        </unify>
                    </pattern>
                    <message>m</message>
                </rule>
            </category>
        </rules>"#;

        let result = XmlRuleParser::new().parse(xml).unwrap();

        let r0 = &result.rules[0];
        let rule_regexp = r0.rule_regexp.as_ref().expect("rule regexp");
        assert_eq!(rule_regexp.text, "warn");
        assert!(rule_regexp.case_sensitive);
        assert!(rule_regexp.exact);
        assert_eq!(rule_regexp.mark, 1);

        let r0b = &result.rules[1];
        let empty_regexp = r0b.rule_regexp.as_ref().expect("self-closing rule regexp");
        assert_eq!(empty_regexp.text, "");
        assert!(!empty_regexp.case_sensitive);
        assert!(!empty_regexp.exact);
        assert_eq!(empty_regexp.mark, 3);

        let r1 = &result.rules[2];

        let anti = &r1.antipatterns[0];
        assert!(anti.case_sensitive);
        let anti_texts: Vec<Option<&str>> = anti.tokens.iter().map(|t| t.text.as_deref()).collect();
        assert_eq!(
            anti_texts,
            vec![Some("beta"), Some("alpha"), Some("tail"), None]
        );

        let token = &r1.pattern.tokens[0];
        assert_eq!(token.space_before.as_deref(), Some("yes"));
        assert_eq!(token.chunk.as_deref(), Some("B-NP"));
        assert_eq!(token.chunk_re.as_deref(), Some("B-.*"));
        assert_eq!(token.skip, 1);

        assert_eq!(result.unifications[0].equivalences[0].postag, "NN:M");

        let parts = &r1.suggestions[0].parts;
        let matched = parts
            .iter()
            .find_map(|p| match p {
                SuggestionPart::Match(m) => Some(m),
                _ => None,
            })
            .expect("match part in suggestion");
        assert_eq!(matched.case_conversion.as_deref(), Some("startlower"));
        assert_eq!(matched.regexp_match.as_deref(), Some("a"));
        assert_eq!(matched.regexp_replace.as_deref(), Some("b"));
        assert!(matched.include_inflected);
        assert_eq!(matched.include_skipped.as_deref(), Some("yes"));
        assert_eq!(matched.postag_replace.as_deref(), Some("NN"));

        let r2 = &result.rules[3];
        assert!(!r2.pattern.unify_groups.is_empty());
    }
}
