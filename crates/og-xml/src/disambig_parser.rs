use crate::disambig_types::*;
use crate::types::*;
use quick_xml::Reader;
use quick_xml::events::BytesStart;
use quick_xml::events::Event;

fn exception_from_attrs(e: &BytesStart<'_>) -> XmlException {
    let mut exc = XmlException::default();
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = crate::decode_attr_value(&attr.value);
        match key.as_str() {
            "regexp" => exc.regexp = Some(val),
            "postag" => exc.postag = Some(val.trim().to_string()),
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
    exc
}

#[allow(clippy::too_many_arguments)]
fn attach_exception(
    exc: XmlException,
    current_token: &mut Option<XmlPatternToken>,
    antipattern_token: &mut Option<XmlPatternToken>,
    in_or_group: bool,
    current_or_group: &mut Option<XmlOrGroup>,
    in_and_group: bool,
    current_and_group: &mut Option<XmlAndGroup>,
    in_antipattern: bool,
    current_antipattern: &mut Option<XmlPattern>,
    current_pattern: &mut Option<XmlPattern>,
) {
    if current_token.is_some() {
        current_token.as_mut().unwrap().exceptions.push(exc);
    } else if in_antipattern && antipattern_token.is_some() {
        antipattern_token.as_mut().unwrap().exceptions.push(exc);
    } else if in_or_group {
        if let Some(g) = current_or_group.as_mut()
            && let Some(last) = g.alternatives.last_mut()
        {
            last.exceptions.push(exc);
        }
    } else if in_and_group {
        if let Some(g) = current_and_group.as_mut()
            && let Some(last) = g.constraints.last_mut()
        {
            last.exceptions.push(exc);
        }
    } else if in_antipattern {
        if let Some(ap) = current_antipattern.as_mut()
            && let Some(last) = ap.tokens.last_mut()
        {
            last.exceptions.push(exc);
        }
    } else if let Some(p) = current_pattern.as_mut()
        && let Some(last) = p.tokens.last_mut()
    {
        last.exceptions.push(exc);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DisambigParseError {
    #[error("XML parse error: {0}")]
    Parse(#[from] quick_xml::Error),
    #[error("Invalid rule: {0}")]
    InvalidRule(String),
}

#[derive(Default)]
pub struct DisambigXmlParser;

impl DisambigXmlParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, xml: &str) -> Result<DisambigRuleSet, DisambigParseError> {
        let expanded = Self::expand_entities(xml);
        let cleaned = Self::strip_doctype(&expanded);

        let mut reader = Reader::from_str(&cleaned);
        reader.config_mut().trim_text(false);

        let mut rule_set = DisambigRuleSet::default();
        let mut current_rule: Option<DisambigRule> = None;
        let mut group_antipatterns: Vec<XmlPattern> = Vec::new();
        let mut current_pattern: Option<XmlPattern> = None;
        let mut current_token: Option<XmlPatternToken> = None;
        let mut current_group_id: Option<String> = None;
        let mut current_group_name: Option<String> = None;
        let mut in_antipattern = false;
        let mut current_antipattern: Option<XmlPattern> = None;
        let mut antipattern_token: Option<XmlPatternToken> = None;
        let mut in_pattern_marker = false;
        let mut pattern_marker_start: Option<usize> = None;
        let mut pattern_marker_elem_start: Option<usize> = None;
        let mut antipattern_marker_elem_start: Option<usize> = None;
        let mut in_exception = false;
        let mut pending_filter_class: Option<String> = None;
        let mut pending_filter_args: Option<String> = None;
        let mut current_exception: Option<XmlException> = None;
        let mut exception_text = String::new();
        let mut in_or_group = false;
        let mut current_or_group: Option<XmlOrGroup> = None;
        let mut in_and_group = false;
        let mut current_and_group: Option<XmlAndGroup> = None;
        let mut current_text = String::new();
        let mut current_unification: Option<UnificationFeature> = None;
        let mut unify_feature: Option<String> = None;
        let mut unify_features: Vec<String> = Vec::new();
        let mut unify_start: Option<usize> = None;
        let mut unify_elem_start: Option<usize> = None;
        let mut unify_negate = false;
        let mut in_unify_ignore = false;
        let mut buf = Vec::new();
        let mut in_wd = false;
        let mut current_wd_text = String::new();
        let mut current_wd: DisambigWord = DisambigWord::default();
        let mut current_disambig: Option<DisambigAction> = None;
        let mut disambig_wds: Vec<DisambigWord> = Vec::new();
        let mut in_disambig_element = false;
        let mut disambig_match: Option<DisambigMatch> = None;
        let mut in_disambig_match = false;
        let mut disambig_match_text = String::new();
        let mut in_example = false;
        let mut example_kind = String::new();
        let mut example_input: Option<String> = None;
        let mut example_output: Option<String> = None;
        let mut example_raw = String::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let local_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match local_name.as_str() {
                        "rule" => {
                            let mut rule = DisambigRule::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "id" => rule.id = Some(val),
                                    "name" => rule.name = Some(val),
                                    _ => {}
                                }
                            }
                            if let Some(id) = current_group_id.clone() {
                                rule.id = rule.id.or(Some(id));
                            }
                            if let Some(name) = current_group_name.clone() {
                                rule.name = rule.name.or(Some(name));
                            }
                            current_rule = Some(rule);
                            pending_filter_class = None;
                            pending_filter_args = None;
                        }
                        "rulegroup" => {
                            current_group_id = None;
                            current_group_name = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "id" => current_group_id = Some(val),
                                    "name" => current_group_name = Some(val),
                                    _ => {}
                                }
                            }
                        }
                        "filter" => {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "class" => pending_filter_class = Some(val),
                                    "args" => pending_filter_args = Some(val),
                                    _ => {}
                                }
                            }
                        }
                        "unification" => {
                            let mut feature = String::new();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "feature" {
                                    feature = val;
                                }
                            }
                            current_unification = Some(UnificationFeature {
                                feature,
                                equivalences: Vec::new(),
                            });
                        }
                        "equivalence" => {
                            let mut eq_type = String::new();
                            let mut postag = String::new();
                            let mut exact = false;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "type" => eq_type = val,
                                    "postag" => postag = val.trim().to_string(),
                                    "postag_regexp" => exact = val != "yes",
                                    _ => {}
                                }
                            }
                            if let Some(ref mut uni) = current_unification {
                                uni.equivalences.push(UnificationEquivalence {
                                    eq_type,
                                    postag,
                                    exact,
                                });
                            }
                        }
                        "unify" => {
                            if let Some(p) = current_pattern.as_ref() {
                                unify_start = Some(p.tokens.len());
                                unify_elem_start = Some(p.elements.len());
                            }
                            unify_feature = None;
                            unify_features.clear();
                            unify_negate = false;
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
                        "feature" if current_pattern.is_some() => {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "id" {
                                    unify_feature = Some(val.clone());
                                    if !unify_features.contains(&val) {
                                        unify_features.push(val);
                                    }
                                }
                            }
                        }
                        "pattern" => {
                            let mut pattern = XmlPattern::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "case_sensitive" {
                                    pattern.case_sensitive = val == "yes";
                                }
                            }
                            current_pattern = Some(pattern);
                            pattern_marker_start = None;
                        }
                        "antipattern" => {
                            in_antipattern = true;
                            let mut antipattern = XmlPattern::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "case_sensitive" {
                                    antipattern.case_sensitive = val == "yes";
                                }
                            }
                            current_antipattern = Some(antipattern);
                        }
                        "example" if !in_antipattern && current_unification.is_none() => {
                            in_example = true;
                            example_kind.clear();
                            example_input = None;
                            example_output = None;
                            example_raw.clear();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "type" => example_kind = val,
                                    "inputform" => example_input = Some(val),
                                    "outputform" => example_output = Some(val),
                                    _ => {}
                                }
                            }
                        }
                        "marker" if in_example => {
                            example_raw.push_str("<marker>");
                        }
                        "marker" if current_pattern.is_some() => {
                            in_pattern_marker = true;
                            pattern_marker_start = current_pattern.as_ref().map(|p| p.tokens.len());
                            pattern_marker_elem_start =
                                current_pattern.as_ref().map(|p| p.elements.len());
                        }
                        "marker" if current_antipattern.is_some() => {
                            in_pattern_marker = true;
                            antipattern_marker_elem_start =
                                current_antipattern.as_ref().map(|p| p.elements.len());
                        }
                        "exception" => {
                            let exc = exception_from_attrs(&e);
                            in_exception = true;
                            current_exception = Some(exc);
                            exception_text.clear();
                        }
                        "token" => {
                            let mut token = XmlPatternToken::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "regexp" => token.regexp = Some(val),
                                    "postag" => token.postag = Some(val.trim().to_string()),
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
                            if in_antipattern && !in_or_group && !in_and_group {
                                antipattern_token = Some(token);
                            } else {
                                current_token = Some(token);
                            }
                        }
                        "or" => {
                            in_or_group = true;
                            current_or_group = Some(XmlOrGroup::default());
                        }
                        "match" if current_token.is_some() || antipattern_token.is_some() => {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "no" {
                                    let no = val.parse::<i32>().ok();
                                    if let Some(ref mut token) = current_token {
                                        token.match_no = no;
                                    } else if let Some(ref mut token) = antipattern_token {
                                        token.match_no = no;
                                    }
                                }
                                if key == "postag" {
                                    if let Some(ref mut token) = current_token {
                                        token.match_no_postag = Some(val);
                                    } else if let Some(ref mut token) = antipattern_token {
                                        token.match_no_postag = Some(val);
                                    }
                                }
                            }
                        }
                        "and" => {
                            in_and_group = true;
                            current_and_group = Some(XmlAndGroup::default());
                        }
                        "disambig" => {
                            let mut action: Option<String> = None;
                            let mut postag: Option<String> = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "action" => action = Some(val),
                                    "postag" => postag = Some(val),
                                    _ => {}
                                }
                            }
                            disambig_wds.clear();
                            in_disambig_element = true;
                            disambig_match = None;
                            disambig_match_text.clear();
                            current_disambig = match action.as_deref() {
                                Some("replace") => {
                                    if let Some(postag) = postag {
                                        Some(DisambigAction::SetPos(postag))
                                    } else {
                                        Some(DisambigAction::Replace(Vec::new()))
                                    }
                                }
                                Some("remove") => {
                                    if let Some(postag) = postag {
                                        Some(DisambigAction::RemovePostag(postag))
                                    } else {
                                        Some(DisambigAction::Remove(Vec::new()))
                                    }
                                }
                                Some("add") => Some(DisambigAction::Add(Vec::new())),
                                Some("filter") => {
                                    let postag = postag.unwrap_or_default();
                                    Some(DisambigAction::Filter { postag })
                                }
                                Some("filterall") => Some(DisambigAction::FilterAll),
                                Some("ignore_spelling") => Some(DisambigAction::IgnoreSpelling),
                                Some("unify") => Some(DisambigAction::Unify),
                                Some("immunize") => Some(DisambigAction::Immunize),
                                None if postag.is_some() => {
                                    Some(DisambigAction::SetPos(postag.unwrap()))
                                }
                                _ => None,
                            };
                        }
                        "match" if in_disambig_element => {
                            in_disambig_match = true;
                            disambig_match_text.clear();
                            let mut m = DisambigMatch::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "postag" => m.postag = Some(val),
                                    "postag_regexp" => m.postag_regexp = val == "yes",
                                    "postag_replace" => m.postag_replace = Some(val),
                                    _ => {}
                                }
                            }
                            disambig_match = Some(m);
                        }
                        "wd" => {
                            in_wd = true;
                            current_wd = DisambigWord::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "pos" => current_wd.pos = Some(val),
                                    "lemma" => current_wd.lemma = Some(val),
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Empty(e)) => {
                    let local_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match local_name.as_str() {
                        "filter" => {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "class" => pending_filter_class = Some(val),
                                    "args" => pending_filter_args = Some(val),
                                    _ => {}
                                }
                            }
                        }
                        "match"
                            if current_token.is_some()
                                || antipattern_token.is_some()
                                || in_disambig_element =>
                        {
                            let mut m = DisambigMatch::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if in_disambig_element {
                                    match key.as_str() {
                                        "postag" => m.postag = Some(val),
                                        "postag_regexp" => m.postag_regexp = val == "yes",
                                        "postag_replace" => m.postag_replace = Some(val),
                                        _ => {}
                                    }
                                    continue;
                                }
                                if key == "no" {
                                    let no = val.parse::<i32>().ok();
                                    if let Some(ref mut token) = current_token {
                                        token.match_no = no;
                                    } else if let Some(ref mut token) = antipattern_token {
                                        token.match_no = no;
                                    }
                                }
                                if key == "postag"
                                    && let Some(ref mut token) = current_token
                                {
                                    token.match_no_postag = Some(val);
                                }
                            }
                            if in_disambig_element {
                                disambig_match = Some(m);
                            }
                        }
                        "exception" => {
                            let exc = exception_from_attrs(&e);
                            attach_exception(
                                exc,
                                &mut current_token,
                                &mut antipattern_token,
                                in_or_group,
                                &mut current_or_group,
                                in_and_group,
                                &mut current_and_group,
                                in_antipattern,
                                &mut current_antipattern,
                                &mut current_pattern,
                            );
                        }
                        "feature" => {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "id" {
                                    unify_feature = Some(val.clone());
                                    if !unify_features.contains(&val) {
                                        unify_features.push(val);
                                    }
                                }
                            }
                        }
                        "token" if current_unification.is_some() => {
                            let mut postag = String::new();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                if key == "postag" {
                                    postag = val;
                                }
                            }
                            if let Some(uni) = current_unification.as_mut()
                                && let Some(eq) = uni.equivalences.last_mut()
                            {
                                eq.postag = postag;
                            }
                        }
                        "token" => {
                            let mut token = XmlPatternToken::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "regexp" => token.regexp = Some(val),
                                    "postag" => token.postag = Some(val.trim().to_string()),
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
                            if in_unify_ignore {
                                token.unify_neutral = true;
                            }
                            if in_or_group {
                                if let Some(ref mut g) = current_or_group {
                                    g.alternatives.push(token);
                                }
                            } else if in_and_group {
                                if let Some(ref mut g) = current_and_group {
                                    g.constraints.push(token);
                                }
                            } else if in_antipattern {
                                if let Some(ref mut ap) = current_antipattern {
                                    for copy in token.min_expansion() {
                                        ap.tokens.push(copy.clone());
                                        ap.elements.push(XmlPatternElement::Token(copy));
                                    }
                                }
                            } else if let Some(ref mut p) = current_pattern {
                                for copy in token.min_expansion() {
                                    p.tokens.push(copy.clone());
                                    p.elements.push(XmlPatternElement::Token(copy));
                                }
                            }
                        }
                        "wd" => {
                            let mut wd = DisambigWord::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "pos" => wd.pos = Some(val),
                                    "lemma" => wd.lemma = Some(val),
                                    _ => {}
                                }
                            }
                            disambig_wds.push(wd);
                        }
                        "disambig" => {
                            let mut action: Option<String> = None;
                            let mut postag: Option<String> = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = crate::decode_attr_value(&attr.value);
                                match key.as_str() {
                                    "action" => action = Some(val),
                                    "postag" => postag = Some(val),
                                    _ => {}
                                }
                            }
                            current_disambig = match action.as_deref() {
                                Some("replace") => {
                                    if let Some(postag) = postag {
                                        Some(DisambigAction::SetPos(postag))
                                    } else {
                                        Some(DisambigAction::Replace(Vec::new()))
                                    }
                                }
                                Some("remove") => {
                                    if let Some(postag) = postag {
                                        Some(DisambigAction::RemovePostag(postag))
                                    } else {
                                        Some(DisambigAction::Remove(Vec::new()))
                                    }
                                }
                                Some("add") => Some(DisambigAction::Add(Vec::new())),
                                Some("filter") => {
                                    let postag = postag.unwrap_or_default();
                                    Some(DisambigAction::Filter { postag })
                                }
                                Some("filterall") => Some(DisambigAction::FilterAll),
                                Some("ignore_spelling") => Some(DisambigAction::IgnoreSpelling),
                                Some("unify") => Some(DisambigAction::Unify),
                                Some("immunize") => Some(DisambigAction::Immunize),
                                None if postag.is_some() => {
                                    Some(DisambigAction::SetPos(postag.unwrap()))
                                }
                                _ => None,
                            };
                        }
                        _ => {}
                    }
                }
                Ok(Event::Text(e)) => {
                    let text = e
                        .unescape()
                        .map(|c| c.to_string())
                        .unwrap_or_else(|_| String::from_utf8_lossy(e.as_ref()).to_string());
                    if in_example {
                        example_raw.push_str(&text);
                    } else if in_disambig_match {
                        disambig_match_text.push_str(&text);
                    } else if in_wd {
                        current_wd_text.push_str(&text);
                    }
                    if in_wd {
                    } else if in_exception {
                        exception_text.push_str(&text);
                    } else if current_token.is_some() || antipattern_token.is_some() {
                        current_text.push_str(&text);
                    }
                }
                Ok(Event::End(e)) => {
                    let local_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match local_name.as_str() {
                        "rule" => {
                            if let Some(mut rule) = current_rule.take() {
                                if let Some(da) = current_disambig.take() {
                                    rule.disambig = da;
                                }
                                rule.filter_class = pending_filter_class.take();
                                rule.filter_args = pending_filter_args.take();
                                rule.antipatterns
                                    .splice(0..0, group_antipatterns.iter().cloned());
                                rule_set.rules.push(rule);
                            }
                            disambig_wds.clear();
                        }
                        "rulegroup" => {
                            current_group_id = None;
                            current_group_name = None;
                            group_antipatterns.clear();
                        }
                        "pattern" => {
                            if let Some(pattern) = current_pattern.take()
                                && let Some(ref mut rule) = current_rule
                            {
                                rule.pattern = pattern;
                            }
                            in_pattern_marker = false;
                            pattern_marker_start = None;
                        }
                        "antipattern" => {
                            if let Some(ap) = current_antipattern.take() {
                                if let Some(ref mut rule) = current_rule {
                                    rule.antipatterns.push(ap);
                                } else {
                                    group_antipatterns.push(ap);
                                }
                            }
                            in_antipattern = false;
                            antipattern_marker_elem_start = None;
                        }
                        "unify" => {
                            if let Some(start) = unify_start
                                && let Some(elem_start) = unify_elem_start
                                && let Some(p) = current_pattern.as_ref()
                                && let Some(rule) = current_rule.as_mut()
                            {
                                let features = if unify_features.is_empty() {
                                    vec![unify_feature.clone().unwrap_or_default()]
                                } else {
                                    unify_features.clone()
                                };
                                rule.unify = Some(UnifySpec {
                                    feature: unify_feature.clone().unwrap_or_default(),
                                    features,
                                    negate: unify_negate,
                                    token_start: start,
                                    token_end: p.tokens.len(),
                                    elem_start,
                                    elem_end: p.elements.len(),
                                });
                            }
                            unify_start = None;
                            unify_elem_start = None;
                            unify_feature = None;
                            unify_features.clear();
                            unify_negate = false;
                        }
                        "unify-ignore" => {
                            in_unify_ignore = false;
                        }
                        "unification" => {
                            if let Some(uni) = current_unification.take() {
                                rule_set.unifications.push(uni);
                            }
                        }
                        "marker" if in_example => {
                            example_raw.push_str("</marker>");
                        }
                        "example" if in_example => {
                            in_example = false;
                            if let Some(rule) = current_rule.as_mut() {
                                match example_kind.as_str() {
                                    "untouched" => {
                                        rule.untouched_examples
                                            .push(example_raw.trim().to_string());
                                    }
                                    "ambiguous" => {
                                        rule.examples.push(DisambigExample {
                                            text: example_raw.trim().to_string(),
                                            input_form: example_input.take(),
                                            output_form: example_output.take(),
                                        });
                                    }
                                    _ => {}
                                }
                            }
                            example_raw.clear();
                        }
                        "marker" if current_pattern.is_some() && in_pattern_marker => {
                            in_pattern_marker = false;
                            if let Some(ref mut p) = current_pattern {
                                p.marker_start = pattern_marker_start;
                                p.marker_end = Some(p.tokens.len());
                                p.marker_elem_start = pattern_marker_elem_start;
                                p.marker_elem_end = Some(p.elements.len());
                            }
                        }
                        "marker" if current_antipattern.is_some() && in_pattern_marker => {
                            in_pattern_marker = false;
                            if let Some(ref mut ap) = current_antipattern {
                                ap.marker_elem_start = antipattern_marker_elem_start;
                                ap.marker_elem_end = Some(ap.elements.len());
                            }
                        }
                        "exception" if in_exception => {
                            in_exception = false;
                            let exc = current_exception.take();
                            if let Some(mut exc) = exc {
                                if !exception_text.is_empty() {
                                    exc.text = Some(exception_text.trim().to_string());
                                }
                                attach_exception(
                                    exc,
                                    &mut current_token,
                                    &mut antipattern_token,
                                    in_or_group,
                                    &mut current_or_group,
                                    in_and_group,
                                    &mut current_and_group,
                                    in_antipattern,
                                    &mut current_antipattern,
                                    &mut current_pattern,
                                );
                            }
                            exception_text.clear();
                        }
                        "token" => {
                            let token = current_token.take().or(antipattern_token.take());
                            if let Some(mut token) = token {
                                let trimmed = current_text.trim();
                                if !trimmed.is_empty() {
                                    token.text = Some(trimmed.to_string());
                                }
                                current_text.clear();
                                if in_unify_ignore {
                                    token.unify_neutral = true;
                                }
                                if current_unification.is_some() && current_pattern.is_none() {
                                    if let Some(uni) = current_unification.as_mut()
                                        && let Some(eq) = uni.equivalences.last_mut()
                                    {
                                        eq.postag = token.postag.clone().unwrap_or_default();
                                    }
                                } else if in_or_group {
                                    if let Some(ref mut g) = current_or_group {
                                        g.alternatives.push(token);
                                    }
                                } else if in_and_group {
                                    if let Some(ref mut g) = current_and_group {
                                        g.constraints.push(token);
                                    }
                                } else if in_antipattern {
                                    if let Some(ref mut ap) = current_antipattern {
                                        for copy in token.min_expansion() {
                                            ap.tokens.push(copy.clone());
                                            ap.elements.push(XmlPatternElement::Token(copy));
                                        }
                                    }
                                } else if let Some(ref mut p) = current_pattern {
                                    for copy in token.min_expansion() {
                                        p.tokens.push(copy.clone());
                                        p.elements.push(XmlPatternElement::Token(copy));
                                    }
                                }
                            }
                        }
                        "or" => {
                            if let Some(g) = current_or_group.take() {
                                if in_antipattern {
                                    if let Some(ref mut ap) = current_antipattern {
                                        ap.elements.push(XmlPatternElement::OrGroup(g));
                                    }
                                } else if let Some(ref mut p) = current_pattern {
                                    p.elements.push(XmlPatternElement::OrGroup(g));
                                }
                            }
                            in_or_group = false;
                        }
                        "and" => {
                            if let Some(g) = current_and_group.take() {
                                if in_antipattern {
                                    if let Some(ref mut ap) = current_antipattern {
                                        ap.elements.push(XmlPatternElement::AndGroup(g));
                                    }
                                } else if let Some(ref mut p) = current_pattern {
                                    p.elements.push(XmlPatternElement::AndGroup(g));
                                }
                            }
                            in_and_group = false;
                        }
                        "match" if in_disambig_match => {
                            in_disambig_match = false;
                            let lemma = disambig_match_text.trim().to_string();
                            if !lemma.is_empty()
                                && let Some(ref mut m) = disambig_match
                            {
                                m.lemma = Some(lemma);
                                m.postag_regexp = true;
                            }
                        }
                        "disambig" => {
                            in_disambig_element = false;
                            if let Some(
                                DisambigAction::Replace(wds)
                                | DisambigAction::Remove(wds)
                                | DisambigAction::Add(wds),
                            ) = current_disambig.as_mut()
                            {
                                wds.append(&mut disambig_wds);
                            }
                            disambig_wds.clear();
                            let has_wds = match current_disambig.as_ref() {
                                Some(
                                    DisambigAction::Replace(w)
                                    | DisambigAction::Remove(w)
                                    | DisambigAction::Add(w),
                                ) => !w.is_empty(),
                                _ => false,
                            };
                            let reaches_match_element = matches!(
                                current_disambig,
                                Some(DisambigAction::Filter { .. })
                                    | Some(DisambigAction::Replace(_))
                                    | Some(DisambigAction::Remove(_))
                                    | Some(DisambigAction::RemovePostag(_))
                                    | Some(DisambigAction::Add(_))
                                    | Some(DisambigAction::SetPos(_))
                            );
                            if let Some(m) = disambig_match.take()
                                && reaches_match_element
                                && !has_wds
                            {
                                current_disambig = Some(DisambigAction::Match(m));
                            }
                        }
                        "wd" if in_wd => {
                            in_wd = false;
                            current_wd.text = current_wd_text.trim().to_string();
                            current_wd_text.clear();
                            disambig_wds.push(current_wd.clone());
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(DisambigParseError::Parse(e));
                }
                _ => {}
            }
            buf.clear();
        }

        Ok(rule_set)
    }

    fn strip_doctype(xml: &str) -> String {
        if let Some(start) = xml.find("<!DOCTYPE")
            && let Some(end) = xml[start..].find("]>")
        {
            return xml[..start].to_string() + &xml[start + end + 2..];
        }
        xml.to_string()
    }

    fn strip_comments(xml: &str) -> String {
        let mut result = String::with_capacity(xml.len());
        let mut rest = xml;
        while let Some(start) = rest.find("<!--") {
            result.push_str(&rest[..start]);
            match rest[start + 4..].find("-->") {
                Some(end) => {
                    for ch in rest[start..start + 4 + end + 3].chars() {
                        if ch == '\n' {
                            result.push('\n');
                        }
                    }
                    rest = &rest[start + 4 + end + 3..];
                }
                None => {
                    rest = "";
                }
            }
        }
        result.push_str(rest);
        result
    }

    fn expand_entities(xml: &str) -> String {
        let mut result = xml.to_string();

        let uncommented = Self::strip_comments(xml);
        let entity_pattern = regex::Regex::new(r#"<!ENTITY\s+(\w+)\s+"([^"]*)">"#).unwrap();
        for cap in entity_pattern.captures_iter(&uncommented) {
            let name = &cap[1];
            let value = &cap[2];
            result = result.replace(&format!("&{};", name), value);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_antipattern_marker_span() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="pl">
    <rule id="ANTI_M" name="Antipattern with marker">
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
        <disambig action="filter" postag="NN"/>
    </rule>
    <rule id="ANTI_PLAIN" name="Antipattern without marker">
        <antipattern>
            <token>foo</token>
        </antipattern>
        <pattern>
            <token>foo</token>
        </pattern>
        <disambig action="filter" postag="NN"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        let marker_anti = &result.rules[0].antipatterns[0];
        assert_eq!(marker_anti.elements.len(), 3);
        assert_eq!(marker_anti.marker_elem_start, Some(1));
        assert_eq!(marker_anti.marker_elem_end, Some(2));
        let plain_anti = &result.rules[1].antipatterns[0];
        assert_eq!(plain_anti.marker_elem_start, None);
        assert_eq!(plain_anti.marker_elem_end, None);
    }

    #[test]
    fn test_exceptions_attach_and_or_groups_collect_alternatives() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="EXC_ATTACH" name="exception attachments">
        <antipattern>
            <token>anti</token>
            <token>anti2<exception postag="XYZ"/></token>
        </antipattern>
        <pattern>
            <token>plain<exception postag="ABC"/></token>
            <or>
                <token>either</token>
                <token>other</token>
            </or>
        </pattern>
        <disambig action="filter" postag="NN"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        let anti = &result.rules[0].antipatterns[0];
        assert_eq!(anti.tokens.len(), 2);
        assert_eq!(anti.tokens[1].exceptions[0].postag.as_deref(), Some("XYZ"));

        let pattern = &result.rules[0].pattern;
        assert_eq!(pattern.tokens[0].text.as_deref(), Some("plain"));
        assert_eq!(pattern.tokens[0].exceptions.len(), 1);
        assert_eq!(
            pattern.tokens[0].exceptions[0].postag.as_deref(),
            Some("ABC")
        );
        match &pattern.elements[1] {
            XmlPatternElement::OrGroup(g) => {
                assert_eq!(g.alternatives.len(), 2);
                assert_eq!(g.alternatives[0].text.as_deref(), Some("either"));
                assert_eq!(g.alternatives[1].text.as_deref(), Some("other"));
            }
            other => panic!("expected or group, got {other:?}"),
        }
    }

    #[test]
    fn test_commented_out_entity_definition_is_ignored() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE rules [
    <!--<!ENTITY verb_forms "ran|runs">-->
    <!ENTITY verb_forms "go|went">
]>
<rules lang="en">
    <rule id="ENTITY_COMMENT" name="Commented entity">
        <pattern>
            <token regexp="yes">&verb_forms;</token>
        </pattern>
        <disambig postag="VB"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules.len(), 1);
        let token_text = &result.rules[0].pattern.tokens[0].text;
        assert_eq!(token_text.as_deref(), Some("go|went"));
    }

    #[test]
    fn test_parse_simple_disambig_rule() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="TEST" name="Test rule">
        <pattern>
            <token>hello</token>
        </pattern>
        <disambig postag="UH"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules.len(), 1);
        assert_eq!(result.rules[0].id.as_deref(), Some("TEST"));
        assert_eq!(result.rules[0].name.as_deref(), Some("Test rule"));
        assert_eq!(result.rules[0].pattern.tokens.len(), 1);
        match &result.rules[0].disambig {
            DisambigAction::SetPos(pos) => assert_eq!(pos, "UH"),
            other => panic!("Expected SetPos, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_replace_action() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="REPLACE" name="Replace test">
        <pattern>
            <token>ca</token>
            <token spacebefore="no">n't</token>
        </pattern>
        <disambig action="replace"><wd lemma="can" pos="MD"/></disambig>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules.len(), 1);
        match &result.rules[0].disambig {
            DisambigAction::Replace(wds) => {
                assert_eq!(wds.len(), 1);
                assert_eq!(wds[0].pos.as_deref(), Some("MD"));
                assert_eq!(wds[0].lemma.as_deref(), Some("can"));
            }
            other => panic!("Expected Replace, got {:?}", other),
        }
        assert_eq!(
            result.rules[0].pattern.tokens[1].space_before.as_deref(),
            Some("no")
        );
    }

    #[test]
    fn test_parse_remove_action() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="REM" name="Remove test">
        <pattern>
            <token>or</token>
        </pattern>
        <disambig action="remove"><wd pos="JJ"/></disambig>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        match &result.rules[0].disambig {
            DisambigAction::Remove(wds) => {
                assert_eq!(wds.len(), 1);
                assert_eq!(wds[0].pos.as_deref(), Some("JJ"));
            }
            other => panic!("Expected Remove, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_filter_action() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="FILT" name="Filter test">
        <pattern>
            <token postag="V.*" postag_regexp="yes"/>
            <token spacebefore="no">n't</token>
        </pattern>
        <disambig action="filter" postag="V.*"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        match &result.rules[0].disambig {
            DisambigAction::Filter { postag } => {
                assert_eq!(postag, "V.*");
            }
            other => panic!("Expected Filter, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_marker_in_pattern() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="MARK" name="Marker test">
        <pattern>
            <token>who</token>
            <marker>
                <token>ai</token>
            </marker>
            <token>n't</token>
        </pattern>
        <disambig action="replace"><wd lemma="be" pos="VBZ"/></disambig>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].pattern.tokens.len(), 3);
        assert_eq!(result.rules[0].pattern.marker_start, Some(1));
        assert_eq!(result.rules[0].pattern.marker_end, Some(2));
    }

    #[test]
    fn test_parse_antipattern() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="ANTI" name="Antipattern test">
        <antipattern>
            <token>foo</token>
            <token>bar</token>
        </antipattern>
        <pattern>
            <token>baz</token>
        </pattern>
        <disambig postag="NN"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].antipatterns.len(), 1);
        assert_eq!(result.rules[0].antipatterns[0].tokens.len(), 2);
    }

    #[test]
    fn test_parse_rulegroup() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rulegroup id="GRP" name="Group">
        <rule name="Rule 1">
            <pattern><token>foo</token></pattern>
            <disambig postag="NN"/>
        </rule>
        <rule name="Rule 2">
            <pattern><token>bar</token></pattern>
            <disambig postag="VB"/>
        </rule>
    </rulegroup>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules.len(), 2);
    }

    #[test]
    fn test_parse_doctype_entities() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE rules [
    <!ENTITY months "January|February|March">
]>
<rules lang="en">
    <rule id="ENT" name="Entity test">
        <pattern>
            <token regexp="yes">&months;</token>
        </pattern>
        <disambig postag="NNP"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules.len(), 1);
        let token = &result.rules[0].pattern.tokens[0];
        assert_eq!(token.text.as_deref(), Some("January|February|March"));
        assert_eq!(token.regexp.as_deref(), Some("yes"));
    }

    #[test]
    fn test_parse_ignore_spelling() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="SPELL" name="Spelling test">
        <pattern>
            <token>something</token>
        </pattern>
        <disambig action="ignore_spelling"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        match &result.rules[0].disambig {
            DisambigAction::IgnoreSpelling => {}
            other => panic!("Expected IgnoreSpelling, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_and_group() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="AND" name="And test">
        <pattern>
            <and>
                <token inflected="yes">install</token>
                <token inflected="yes">instal</token>
            </and>
        </pattern>
        <disambig action="remove"><wd lemma="instal"></wd></disambig>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.rules[0].pattern.elements.len(), 1);
        match &result.rules[0].pattern.elements[0] {
            XmlPatternElement::AndGroup(g) => {
                assert_eq!(g.constraints.len(), 2);
            }
            other => panic!("Expected AndGroup, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_add_multiple_wds() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="MULTI" name="Multi wd">
        <pattern>
            <token>,</token>
        </pattern>
        <disambig action="add"><wd pos="PCT"/></disambig>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        match &result.rules[0].disambig {
            DisambigAction::Add(wds) => {
                assert_eq!(wds.len(), 1);
                assert_eq!(wds[0].pos.as_deref(), Some("PCT"));
            }
            other => panic!("Expected Add, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_regexp_token() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="RE" name="Regexp">
        <pattern>
            <token regexp="yes">\d+</token>
        </pattern>
        <disambig postag="CD"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        let token = &result.rules[0].pattern.tokens[0];
        assert_eq!(token.regexp.as_deref(), Some("yes"));
        assert_eq!(token.text.as_deref(), Some("\\d+"));
    }

    #[test]
    fn test_parse_case_sensitive_pattern() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="CS" name="Case Sensitive">
        <pattern case_sensitive="yes">
            <token>or</token>
        </pattern>
        <disambig action="remove"><wd pos="JJ"/></disambig>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        assert!(result.rules[0].pattern.case_sensitive);
    }
}

#[cfg(test)]
#[test]
fn paired_elements_and_edge_attrs_are_parsed() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <!-- a comment
    spanning lines with <!ENTITY decoy "x"> inside -->
    <rulegroup id="GRP" name="Group">
        <antipattern case_sensitive="yes">
            <marker><token>anti</token></marker>
        </antipattern>
        <rule foo="unknown">
            <filter class="DemoFilter" args="a b"></filter>
            <pattern case_sensitive="yes">
                <unify-ignore><token>skip</token></unify-ignore>
                <token case_sensitive="yes" spacebefore="yes" chunk="B-NP" chunk_re="B-.*">word<match no="1" postag="NN"/></token>
                <token>exc<exception case_sensitive="yes" bogus="x" scope="next">BAD</exception></token>
                <or><token>either</token><exception postag="XYZ"/></or>
                <and><token>both</token><exception postag="ZYX"/></and>
                <marker><token>marked</token></marker>
            </pattern>
            <disambig action="filterall"/>
            <example type="ambiguous" inputform="in" outputform="out" bogus="y">Some <marker>example</marker> text.</example>
            <example type="untouched">Untouched text.</example>
        </rule>
    </rulegroup>
</rules>"#;

    let parser = DisambigXmlParser::new();
    let result = parser.parse(xml).unwrap();

    assert_eq!(result.rules.len(), 1);
    let rule = &result.rules[0];
    assert_eq!(rule.id.as_deref(), Some("GRP"));
    assert_eq!(rule.filter_class.as_deref(), Some("DemoFilter"));
    assert_eq!(rule.filter_args.as_deref(), Some("a b"));
    assert_eq!(rule.antipatterns.len(), 1);
    assert!(rule.antipatterns[0].case_sensitive);
    assert_eq!(rule.antipatterns[0].marker_elem_start, Some(0));
    assert_eq!(rule.antipatterns[0].marker_elem_end, Some(1));
    assert!(rule.pattern.case_sensitive);

    let skip = &rule.pattern.tokens[0];
    assert_eq!(skip.text.as_deref(), Some("skip"));
    assert!(skip.unify_neutral);

    let token = &rule.pattern.tokens[1];
    assert!(token.case_sensitive);
    assert!(token.case_sensitive_set);
    assert_eq!(token.space_before.as_deref(), Some("yes"));
    assert_eq!(token.chunk.as_deref(), Some("B-NP"));
    assert_eq!(token.chunk_re.as_deref(), Some("B-.*"));
    assert_eq!(token.match_no, Some(1));
    assert_eq!(token.match_no_postag.as_deref(), Some("NN"));

    let exc_token = &rule.pattern.tokens[2];
    assert_eq!(exc_token.exceptions.len(), 1);
    let exc = &exc_token.exceptions[0];
    assert!(exc.case_sensitive);
    assert!(exc.case_sensitive_set);
    assert_eq!(exc.scope.as_deref(), Some("next"));
    assert_eq!(exc.text.as_deref(), Some("BAD"));

    let or_group = rule
        .pattern
        .elements
        .iter()
        .find_map(|e| match e {
            XmlPatternElement::OrGroup(g) => Some(g),
            _ => None,
        })
        .expect("or group");
    assert_eq!(or_group.alternatives.len(), 1);
    assert_eq!(
        or_group.alternatives[0].exceptions[0].postag.as_deref(),
        Some("XYZ")
    );
    let and_group = rule
        .pattern
        .elements
        .iter()
        .find_map(|e| match e {
            XmlPatternElement::AndGroup(g) => Some(g),
            _ => None,
        })
        .expect("and group");
    assert_eq!(and_group.constraints.len(), 1);
    assert_eq!(
        and_group.constraints[0].exceptions[0].postag.as_deref(),
        Some("ZYX")
    );
    assert!(rule.pattern.marker_elem_start.is_some());

    assert_eq!(rule.examples.len(), 1);
    assert_eq!(rule.examples[0].input_form.as_deref(), Some("in"));
    assert_eq!(rule.examples[0].output_form.as_deref(), Some("out"));
    assert!(rule.examples[0].text.contains("<marker>"));
    assert_eq!(rule.untouched_examples, vec!["Untouched text.".to_string()]);
    assert!(matches!(rule.disambig, DisambigAction::FilterAll));
}

#[test]
fn disambig_action_variants_and_unification_tokens_are_parsed() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <unification feature="number">
        <equivalence type="sg" postag="NN" postag_regexp="no" bogus="z"><token postag="NN"/></equivalence>
        <equivalence type="pl"><token postag="NNS">words</token></equivalence>
    </unification>
    <rule id="SETPOS" name="setpos">
        <pattern><token>a</token></pattern>
        <disambig action="replace" postag="NN"/>
    </rule>
    <rule id="RMDIR" name="rmdir">
        <pattern><token>b</token></pattern>
        <disambig action="remove" postag="NN"/>
    </rule>
    <rule id="ADD" name="add">
        <pattern><token>c</token></pattern>
        <disambig action="add"><wd pos="VB" bogus="q">run</wd><wd pos="NN"/></disambig>
    </rule>
    <rule id="MATCHED" name="matched">
        <pattern><token>d</token></pattern>
        <disambig action="filter" postag="NN"><match postag="NN" postag_regexp="yes" postag_replace="VB">lemma</match></disambig>
    </rule>
    <rule id="IGNORE" name="ignore">
        <pattern><token>e</token></pattern>
        <disambig action="ignore_spelling"/>
    </rule>
    <rule id="UNIFY" name="unify">
        <pattern><token>f</token></pattern>
        <disambig action="unify"/>
    </rule>
    <rule id="IMMUNE" name="immune">
        <pattern><token>g</token></pattern>
        <disambig action="immunize"/>
    </rule>
    <rule id="BARE" name="bare">
        <pattern><token>h</token></pattern>
        <disambig postag="UH"/>
    </rule>
    <rule id="EMPTYACT" name="emptyact">
        <pattern><token>i</token></pattern>
        <disambig action="replace"/>
    </rule>
</rules>"#;

    let parser = DisambigXmlParser::new();
    let result = parser.parse(xml).unwrap();

    assert_eq!(result.unifications.len(), 1);
    let uni = &result.unifications[0];
    assert_eq!(uni.feature, "number");
    assert_eq!(uni.equivalences.len(), 2);
    assert_eq!(uni.equivalences[0].eq_type, "sg");
    assert!(uni.equivalences[0].exact);
    assert_eq!(uni.equivalences[0].postag, "NN");
    assert_eq!(uni.equivalences[1].postag, "NNS");

    let by_id = |id: &str| {
        result
            .rules
            .iter()
            .find(|r| r.id.as_deref() == Some(id))
            .unwrap()
    };
    assert!(matches!(&by_id("SETPOS").disambig, DisambigAction::SetPos(p) if p == "NN"));
    assert!(matches!(&by_id("RMDIR").disambig, DisambigAction::RemovePostag(p) if p == "NN"));
    match &by_id("ADD").disambig {
        DisambigAction::Add(wds) => {
            assert_eq!(wds.len(), 2);
            assert_eq!(wds[0].pos.as_deref(), Some("VB"));
            assert_eq!(wds[0].text, "run");
            assert_eq!(wds[1].text, "");
        }
        other => panic!("Expected Add, got {:?}", other),
    }
    match &by_id("MATCHED").disambig {
        DisambigAction::Match(m) => {
            assert_eq!(m.postag.as_deref(), Some("NN"));
            assert!(m.postag_regexp);
            assert_eq!(m.postag_replace.as_deref(), Some("VB"));
            assert_eq!(m.lemma.as_deref(), Some("lemma"));
        }
        other => panic!("Expected Match, got {:?}", other),
    }
    assert!(matches!(
        by_id("IGNORE").disambig,
        DisambigAction::IgnoreSpelling
    ));
    assert!(matches!(by_id("UNIFY").disambig, DisambigAction::Unify));
    assert!(matches!(by_id("IMMUNE").disambig, DisambigAction::Immunize));
    assert!(matches!(&by_id("BARE").disambig, DisambigAction::SetPos(p) if p == "UH"));
    assert!(matches!(&by_id("EMPTYACT").disambig, DisambigAction::Replace(wds) if wds.is_empty()));
}

#[test]
fn paired_disambig_actions_feature_and_token_match_are_parsed() {
    let xml = r#"<rules lang="de">
    <rule id="FA" name="fa">
        <pattern><token>a</token></pattern>
        <disambig action="filterall"></disambig>
    </rule>
    <rule id="IG" name="ig">
        <pattern><token>b</token></pattern>
        <disambig action="ignore_spelling"></disambig>
    </rule>
    <rule id="UN" name="un">
        <pattern><token>c</token></pattern>
        <disambig action="unify"></disambig>
    </rule>
    <rule id="IM" name="im">
        <pattern><token>d</token></pattern>
        <disambig action="immunize"></disambig>
    </rule>
    <rule id="SP" name="sp">
        <pattern><token>e</token></pattern>
        <disambig postag="NN"></disambig>
    </rule>
    <rule id="FM" name="fm">
        <pattern>
            <unify>
                <feature id="gender"></feature>
                <token>f</token>
            </unify>
            <token>wort<match no="1" postag="NN"></match></token>
        </pattern>
        <disambig action="filter" postag="NN"></disambig>
    </rule>
</rules>"#;

    let result = DisambigXmlParser::new().parse(xml).unwrap();

    let by_id = |id: &str| {
        result
            .rules
            .iter()
            .find(|r| r.id.as_deref() == Some(id))
            .unwrap()
    };
    assert!(matches!(by_id("FA").disambig, DisambigAction::FilterAll));
    assert!(matches!(
        by_id("IG").disambig,
        DisambigAction::IgnoreSpelling
    ));
    assert!(matches!(by_id("UN").disambig, DisambigAction::Unify));
    assert!(matches!(by_id("IM").disambig, DisambigAction::Immunize));
    assert!(matches!(&by_id("SP").disambig, DisambigAction::SetPos(p) if p == "NN"));
    assert!(matches!(
        &by_id("FM").disambig,
        DisambigAction::Filter { postag } if postag == "NN"
    ));

    let fm = by_id("FM");
    let unify = fm.unify.as_ref().expect("unify");
    assert_eq!(unify.features, vec!["gender"]);
    let matched = fm.pattern.tokens.last().unwrap();
    assert_eq!(matched.match_no, Some(1));
    assert_eq!(matched.match_no_postag.as_deref(), Some("NN"));
}

#[test]
fn reader_errors_are_reported() {
    assert!(DisambigXmlParser::new().parse("<rules").is_err());
}

#[cfg(test)]
mod self_closing_exception_tests {
    use super::*;

    #[test]
    fn exception_postag_is_parsed_inside_pattern_token() {
        let xml = r##"<rules lang="fr">
    <rule name="nom" id="NOM">
        <pattern>
            <token postag="SENT_START"/>
            <token regexp="yes">Les?|La</token>
            <marker>
                <token postag="[NJZK].*" postag_regexp="yes">
                    <exception postag="V ppr"/></token>
            </marker>
        </pattern>
        <disambig action="filter" postag="[NJZKD].*"/>
    </rule>
</rules>"##;
        let rs = DisambigXmlParser::new().parse(xml).unwrap();
        let toks = &rs.rules[0].pattern.tokens;
        let marked = toks
            .iter()
            .find(|t| t.postag.as_deref() == Some("[NJZK].*"))
            .expect("marked token with [NJZK].* postag");
        assert_eq!(marked.exceptions.len(), 1);
        assert_eq!(marked.exceptions[0].postag.as_deref(), Some("V ppr"));
    }

    #[test]
    fn self_closing_exception_in_antipattern_is_parsed() {
        let xml = r##"<rules lang="fr">
    <rule id="R" name="r">
        <antipattern>
            <token>a</token>
            <token postag="N.*" postag_regexp="yes"><exception postag="V ppr"/></token>
        </antipattern>
        <pattern>
            <token>b</token>
        </pattern>
        <disambig action="filter" postag="N"/>
    </rule>
</rules>"##;
        let rs = DisambigXmlParser::new().parse(xml).unwrap();
        let ap = &rs.rules[0].antipatterns[0];
        assert_eq!(ap.tokens[1].exceptions.len(), 1);
        assert_eq!(ap.tokens[1].exceptions[0].postag.as_deref(), Some("V ppr"));
    }

    #[test]
    fn unification_block_and_unify_disambig_rule_are_parsed() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">
    <unification feature="gender">
        <equivalence type="mas">
            <token postag=".*:(MAS|NOG).*" postag_regexp="yes"/>
        </equivalence>
        <equivalence type="fem">
            <token postag=".*:(FEM|NOG).*" postag_regexp="yes"/>
        </equivalence>
        <equivalence type="pl" postag=".*:PLU.*" postag_regexp="yes"></equivalence>
    </unification>
    <rule id="UNI_DET" name="NP unify">
        <pattern>
            <unify>
                <feature id="gender"/>
                <token regexp="yes">der|die|das</token>
                <token postag="SUB:.*" postag_regexp="yes"/>
            </unify>
        </pattern>
        <disambig action="unify"/>
        <example type="untouched">Der Wagen</example>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.unifications.len(), 1);
        let uni = &result.unifications[0];
        assert_eq!(uni.feature, "gender");
        assert_eq!(uni.equivalences.len(), 3);
        assert_eq!(uni.equivalences[0].eq_type, "mas");
        assert_eq!(uni.equivalences[0].postag, ".*:(MAS|NOG).*");
        assert!(!uni.equivalences[0].exact);
        assert_eq!(uni.equivalences[2].eq_type, "pl");
        assert_eq!(uni.equivalences[2].postag, ".*:PLU.*");
        assert!(!uni.equivalences[2].exact);

        let rule = &result.rules[0];
        let unify = rule.unify.as_ref().unwrap();
        assert_eq!(unify.features, vec!["gender"]);
        assert!(!unify.negate);
        assert_eq!(unify.token_start, 0);
        assert_eq!(unify.token_end, 2);
        assert_eq!(unify.elem_start, 0);
        assert_eq!(unify.elem_end, 2);
        assert!(matches!(rule.disambig, DisambigAction::Unify));
        assert_eq!(rule.untouched_examples, vec!["Der Wagen"]);
    }

    #[test]
    fn unify_negate_and_multiple_features_are_parsed() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">
    <rule id="UNI_NEG" name="negated unify">
        <pattern>
            <unify negate="yes">
                <feature id="gender"/><feature id="number"/>
                <token>der</token>
                <token>Wagen</token>
            </unify>
            <token>.</token>
        </pattern>
        <disambig action="filterall"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        let rule = &result.rules[0];
        let unify = rule.unify.as_ref().unwrap();
        assert_eq!(unify.features, vec!["gender", "number"]);
        assert!(unify.negate);
        assert_eq!(unify.token_start, 0);
        assert_eq!(unify.token_end, 2);
        assert_eq!(unify.elem_start, 0);
        assert_eq!(unify.elem_end, 2);
        assert_eq!(rule.pattern.tokens.len(), 3);
        assert!(matches!(rule.disambig, DisambigAction::FilterAll));
    }

    #[test]
    fn filter_class_and_args_attach_to_the_current_rule() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="de">
    <rule id="EN_FILTER" name="English filter rule">
        <pattern>
            <token regexp="yes">test|case</token>
            <token regexp="yes">[a-z]+</token>
        </pattern>
        <filter class="org.languagetool.rules.IsEnglishWordFilter" args="formPositions:1,2"/>
        <disambig action="add">
            <wd pos="_english_ignore_"/>
            <wd pos="_english_ignore_"/>
        </disambig>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        let rule = &result.rules[0];
        assert_eq!(
            rule.filter_class.as_deref(),
            Some("org.languagetool.rules.IsEnglishWordFilter")
        );
        assert_eq!(rule.filter_args.as_deref(), Some("formPositions:1,2"));
        match &rule.disambig {
            DisambigAction::Add(wds) => {
                assert_eq!(wds.len(), 2);
                assert_eq!(wds[0].pos.as_deref(), Some("_english_ignore_"));
            }
            other => panic!("Expected Add, got {:?}", other),
        }
    }

    #[test]
    fn filter_disambig_with_match_lemma_becomes_a_match_action() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="pl">
    <rule id="MAC_MATCH" name="match lemma">
        <pattern>
            <token>mają</token>
        </pattern>
        <disambig action="filter"><match no="1">mieć</match></disambig>
        <example type="ambiguous" inputform="mają[maić/verb:fin]" outputform="mają[mieć/verb:fin]">Oni <marker>mają</marker> robaki.</example>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        let rule = &result.rules[0];
        match &rule.disambig {
            DisambigAction::Match(m) => {
                assert_eq!(m.lemma.as_deref(), Some("mieć"));
                assert!(m.postag.is_none());
                assert!(m.postag_regexp);
            }
            other => panic!("Expected Match, got {:?}", other),
        }
        assert_eq!(rule.examples.len(), 1);
        let example = &rule.examples[0];
        assert_eq!(example.text, "Oni <marker>mają</marker> robaki.");
        assert_eq!(example.input_form.as_deref(), Some("mają[maić/verb:fin]"));
        assert_eq!(example.output_form.as_deref(), Some("mają[mieć/verb:fin]"));
    }

    #[test]
    fn filter_disambig_with_postag_match_keeps_the_regexp_flag() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="fr">
    <rule id="FR_MATCH" name="postag match">
        <pattern>
            <token>des</token>
            <token>trucs</token>
        </pattern>
        <disambig action="filter"><match no="2" postag="[ZNJ].*" postag_regexp="yes"/></disambig>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        let rule = &result.rules[0];
        match &rule.disambig {
            DisambigAction::Match(m) => {
                assert_eq!(m.postag.as_deref(), Some("[ZNJ].*"));
                assert!(m.postag_regexp);
                assert!(m.lemma.is_none());
            }
            other => panic!("Expected Match, got {:?}", other),
        }
    }

    #[test]
    fn and_group_tokens_and_self_closing_remove_postag_are_parsed() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="ca">
    <rule id="FOT_N" name="fot nominal">
        <pattern>
            <and>
                <token postag="N.*" postag_regexp="yes" regexp="yes">fot|fots|bon|ros</token>
                <token postag="N.*|SENT_END" postag_regexp="yes" negate_pos="yes"/>
            </and>
        </pattern>
        <disambig action="remove" postag="N.*"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        let rule = &result.rules[0];
        assert_eq!(rule.pattern.elements.len(), 1);
        match &rule.pattern.elements[0] {
            XmlPatternElement::AndGroup(g) => {
                assert_eq!(g.constraints.len(), 2);
                assert!(g.constraints[1].negate_pos);
            }
            other => panic!("Expected AndGroup, got {:?}", other),
        }
        match &rule.disambig {
            DisambigAction::RemovePostag(pos) => assert_eq!(pos, "N.*"),
            other => panic!("Expected RemovePostag, got {:?}", other),
        }
    }

    #[test]
    fn or_group_alternatives_and_post_token_exceptions_are_parsed() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="ca">
    <rule id="OR_GROUP" name="or group">
        <pattern>
            <or>
                <token>alpha</token>
                <token>beta</token>
                <exception postag="XYZ" spacebefore="yes"/>
            </or>
        </pattern>
        <disambig action="filterall"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        let rule = &result.rules[0];
        match &rule.pattern.elements[0] {
            XmlPatternElement::OrGroup(g) => {
                assert_eq!(g.alternatives.len(), 2);
                let exc = &g.alternatives[1].exceptions[0];
                assert_eq!(exc.postag.as_deref(), Some("XYZ"));
                assert_eq!(exc.space_before, Some(true));
            }
            other => panic!("Expected OrGroup, got {:?}", other),
        }
        assert!(matches!(rule.disambig, DisambigAction::FilterAll));
    }

    #[test]
    fn trailing_exceptions_attach_to_the_last_pattern_token() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="TRAILING_EXC" name="exception after the token closes">
        <pattern>
            <token>plain</token>
            <exception postag="ABC" negate_pos="yes"/>
        </pattern>
        <disambig action="filter" postag="NN"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        let pattern = &result.rules[0].pattern;
        assert_eq!(pattern.tokens.len(), 1);
        assert_eq!(pattern.tokens[0].text.as_deref(), Some("plain"));
        assert_eq!(pattern.tokens[0].exceptions.len(), 1);
        assert_eq!(
            pattern.tokens[0].exceptions[0].postag.as_deref(),
            Some("ABC")
        );
        assert!(pattern.tokens[0].exceptions[0].negate_pos);
    }

    #[test]
    fn trailing_exceptions_attach_to_the_last_antipattern_token() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <rule id="ANTI_TRAILING_EXC" name="antipattern exception after the token closes">
        <antipattern>
            <token>anti</token>
            <exception postag="XYZ"/>
        </antipattern>
        <pattern>
            <token>plain</token>
        </pattern>
        <disambig action="filter" postag="NN"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        let anti = &result.rules[0].antipatterns[0];
        assert_eq!(anti.tokens.len(), 1);
        assert_eq!(anti.tokens[0].text.as_deref(), Some("anti"));
        assert_eq!(anti.tokens[0].exceptions.len(), 1);
        assert_eq!(anti.tokens[0].exceptions[0].postag.as_deref(), Some("XYZ"));
    }

    #[test]
    fn paired_filter_and_unification_elements_are_parsed() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<rules lang="en">
    <unification feature="number"></unification>
    <rule id="PAIRED_FILTER" name="filter with content">
        <pattern>
            <token>plain</token>
        </pattern>
        <filter class="sometClass" args="someArgs"></filter>
        <disambig action="filter" postag="NN"/>
    </rule>
</rules>"#;

        let parser = DisambigXmlParser::new();
        let result = parser.parse(xml).unwrap();

        assert_eq!(result.unifications.len(), 1);
        assert_eq!(result.unifications[0].feature, "number");
        assert_eq!(result.rules[0].filter_class.as_deref(), Some("sometClass"));
        assert_eq!(result.rules[0].filter_args.as_deref(), Some("someArgs"));
    }
}
