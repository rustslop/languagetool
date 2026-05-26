use crate::types::*;
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, thiserror::Error)]
pub enum XmlParseError {
    #[error("XML parse error: {0}")]
    Parse(#[from] quick_xml::Error),
    #[error("Invalid rule: {0}")]
    InvalidRule(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct XmlRuleParser;

impl XmlRuleParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, xml: &str) -> Result<XmlRuleFile, XmlParseError> {
        let expanded = Self::expand_entities(xml);
        let cleaned = Self::strip_doctype(&expanded);

        let mut reader = Reader::from_str(&cleaned);
        reader.config_mut().trim_text(false);

        let mut file = XmlRuleFile::default();
        let mut current_category: Option<XmlCategory> = None;
        let mut current_rule: Option<XmlRule> = None;
        let mut current_pattern: Option<XmlPattern> = None;
        let mut current_token: Option<XmlPatternToken> = None;
        let mut current_group: Option<XmlRuleGroup> = None;
        let mut in_message = false;
        let mut in_suggestion = false;
        let mut suggestion_text = String::new();
        let mut suggestion_parts: Vec<SuggestionPart> = Vec::new();
        let mut message_text = String::new();
        let mut in_example = false;
        let mut example_type = XmlExampleType::Correct;
        let mut example_text = String::new();
        let mut current_text = String::new();
        let mut buf = Vec::new();
        // Antipattern state
        let mut in_antipattern = false;
        let mut current_antipattern: Option<XmlPattern> = None;
        let mut antipattern_token: Option<XmlPatternToken> = None;
        let mut antipattern_text = String::new();
        // Marker-in-pattern state
        let mut in_pattern_marker = false;
        let mut pattern_marker_start: Option<usize> = None;
        let mut pattern_marker_end: Option<usize> = None;
        // Non-self-closing exception state
        let mut in_exception = false;
        let mut current_exception: Option<XmlException> = None;
        let mut exception_text = String::new();
        // Or/And group state
        let mut in_or_group = false;
        let mut current_or_group: Option<XmlOrGroup> = None;
        let mut in_and_group = false;
        let mut current_and_group: Option<XmlAndGroup> = None;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let local_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match local_name.as_str() {
                        "category" => {
                            let mut cat = XmlCategory::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "id" => cat.id = val,
                                    "name" => cat.name = val,
                                    "description" => cat.description = Some(val),
                                    "default" => cat.default_on = val == "on",
                                    _ => {}
                                }
                            }
                            current_category = Some(cat);
                        }
                        "rulegroup" => {
                            let mut group = XmlRuleGroup::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "id" => group.id = Some(val),
                                    "name" => group.name = Some(val),
                                    "default" => group.default_on = Some(val == "on"),
                                    _ => {}
                                }
                            }
                            current_group = Some(group);
                        }
                        "rule" => {
                            let mut rule = XmlRule {
                                default_on: true,
                                ..Default::default()
                            };
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "id" => rule.id = val,
                                    "name" => rule.name = val,
                                    "default" => rule.default_on = val == "on",
                                    "deprecated" => rule.deprecated = val == "yes",
                                    _ => {}
                                }
                            }
                            if let Some(cat) = &current_category {
                                rule.category = cat.clone();
                            }
                            current_rule = Some(rule);
                        }
                        "pattern" => {
                            let mut pattern = XmlPattern::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "case_sensitive" => pattern.case_sensitive = val == "yes",
                                    _ => {}
                                }
                            }
                            current_pattern = Some(pattern);
                            pattern_marker_start = None;
                        }
                        "antipattern" => {
                            in_antipattern = true;
                            let mut ap = XmlPattern::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == "case_sensitive" {
                                    ap.case_sensitive = val == "yes";
                                }
                            }
                            current_antipattern = Some(ap);
                            antipattern_text.clear();
                        }
                        "exception" => {
                            // Non-self-closing exception with text content
                            let mut exc = XmlException::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "regexp" => exc.regexp = Some(val),
                                    "postag" => exc.postag = Some(val),
                                    "postag_regexp" => exc.postag_regexp = Some(val),
                                    "negate" => exc.negate = val == "yes",
                                    "negate_pos" => exc.negate_pos = val == "yes",
                                    "inflected" => exc.inflected = val == "yes",
                                    "case_sensitive" => exc.case_sensitive = val == "yes",
                                    "scope" => exc.scope = Some(val),
                                    _ => {}
                                }
                            }
                            in_exception = true;
                            current_exception = Some(exc);
                            exception_text.clear();
                        }
                        "token" => {
                            let mut token = XmlPatternToken::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "regexp" => token.regexp = Some(val),
                                    "postag" => token.postag = Some(val),
                                    "postag_regexp" => token.postag_regexp = Some(val),
                                    "negate" => token.negate = val == "yes",
                                    "negate_pos" => token.negate_pos = val == "yes",
                                    "case_sensitive" => token.case_sensitive = val == "yes",
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
                            current_token = Some(token);
                        }
                        "message" => {
                            in_message = true;
                            message_text.clear();
                        }
                        "suggestion" => {
                            in_suggestion = true;
                            suggestion_text.clear();
                            suggestion_parts.clear();
                        }
                        "match" if in_suggestion => {
                            // <match no="1" case_conversion="startupper">text</match>
                            let mut m = XmlMatch::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "no" => m.no = val.parse().unwrap_or(1),
                                    "case_conversion" => m.case_conversion = Some(val),
                                    "regexp_match" => m.regexp_match = Some(val),
                                    "regexp_replace" => m.regexp_replace = Some(val),
                                    "include_inflected" => m.include_inflected = val == "yes",
                                    _ => {}
                                }
                            }
                            suggestion_parts.push(SuggestionPart::Match(m));
                        }
                        "match" if current_token.is_some() => {
                            // <match no="1"> inside a <token> - backreference in pattern
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == "no" {
                                    if let Some(ref mut token) = current_token {
                                        token.match_no = val.parse().ok();
                                    }
                                }
                            }
                        }
                        "example" => {
                            let mut has_correction = false;
                            let ex_type = e.attributes().flatten().find_map(|attr| {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == "type" {
                                    Some(val)
                                } else if key == "correction" {
                                    has_correction = true;
                                    None
                                } else {
                                    None
                                }
                            });
                            example_type = match ex_type.as_deref() {
                                Some("incorrect") => XmlExampleType::Incorrect,
                                Some("triggers_error") => XmlExampleType::TriggersError,
                                _ if has_correction => XmlExampleType::Incorrect,
                                _ => XmlExampleType::Correct,
                            };
                            in_example = true;
                            example_text.clear();
                        }
                        "marker" => {
                            // Inside example - preserve <marker> tags for position extraction
                            if in_example {
                                example_text.push_str("<marker>");
                            } else if current_pattern.is_some() || current_antipattern.is_some() {
                                // Inside pattern - track which tokens are in the marker
                                in_pattern_marker = true;
                                if let Some(ref pattern) = current_pattern {
                                    pattern_marker_start = Some(pattern.tokens.len());
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
                        "token" => {
                            let mut token = XmlPatternToken::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "regexp" => token.regexp = Some(val),
                                    "negate" => token.negate = val == "yes",
                                    "negate_pos" => token.negate_pos = val == "yes",
                                    "case_sensitive" => token.case_sensitive = val == "yes",
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
                            if in_or_group {
                                if let Some(ref mut or_group) = current_or_group {
                                    or_group.alternatives.push(token);
                                }
                            } else if in_and_group {
                                if let Some(ref mut and_group) = current_and_group {
                                    and_group.constraints.push(token);
                                }
                            } else if in_antipattern {
                                // For self-closing tokens in antipatterns, antipattern_text
                                // contains only whitespace between tokens — don't assign it.
                                // Only assign non-whitespace text content.
                                let trimmed = antipattern_text.trim();
                                if !trimmed.is_empty() {
                                    token.text = Some(trimmed.to_string());
                                }
                                if let Some(ref mut ap) = current_antipattern {
                                    ap.tokens.push(token.clone());
                                    ap.elements.push(XmlPatternElement::Token(token));
                                }
                                antipattern_text.clear();
                            } else if let Some(pattern) = &mut current_pattern {
                                let trimmed = current_text.trim();
                                if !trimmed.is_empty() {
                                    token.text = Some(trimmed.to_string());
                                }
                                pattern.tokens.push(token.clone());
                                pattern.elements.push(XmlPatternElement::Token(token));
                            }
                            current_text.clear();
                        }
                        "exception" => {
                            let mut exc = XmlException::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "regexp" => exc.regexp = Some(val),
                                    "postag" => exc.postag = Some(val),
                                    "postag_regexp" => exc.postag_regexp = Some(val),
                                    "negate" => exc.negate = val == "yes",
                                    "negate_pos" => exc.negate_pos = val == "yes",
                                    "inflected" => exc.inflected = val == "yes",
                                    "case_sensitive" => exc.case_sensitive = val == "yes",
                                    "scope" => exc.scope = Some(val),
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
                            // Short message - read text content
                        }
                        "match" if in_suggestion => {
                            // <match no="1" case_conversion="startupper" /> inside suggestion
                            let mut m = XmlMatch::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "no" => m.no = val.parse().unwrap_or(1),
                                    "case_conversion" => m.case_conversion = Some(val),
                                    "regexp_match" => m.regexp_match = Some(val),
                                    "regexp_replace" => m.regexp_replace = Some(val),
                                    "include_inflected" => m.include_inflected = val == "yes",
                                    _ => {}
                                }
                            }
                            suggestion_parts.push(SuggestionPart::Match(m));
                        }
                        "match" if current_token.is_some() => {
                            // <match no="1" /> inside a <token> - backreference in pattern
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                if key == "no" {
                                    if let Some(ref mut token) = current_token {
                                        token.match_no = val.parse().ok();
                                    }
                                }
                            }
                        }
                        "url" => {
                            // URL element
                        }
                        "antipattern" => {
                            // Antipattern
                        }
                        "filter" => {
                            let mut filter = XmlFilter::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
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
                    let text = e.unescape().unwrap_or_default().to_string();
                    if in_suggestion {
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
                            if let Some(group) = current_group.take() {
                                file.rule_groups.push(group);
                            }
                        }
                        "rule" => {
                            if let Some(mut rule) = current_rule.take() {
                                if let Some(group) = &mut current_group {
                                    // Inherit group-level antipatterns (prepend so rule-level ones take precedence)
                                    let mut combined = group.antipatterns.clone();
                                    combined.extend(rule.antipatterns.drain(..));
                                    rule.antipatterns = combined;
                                    group.rules.push(rule);
                                } else {
                                    file.rules.push(rule);
                                }
                            }
                        }
                        "pattern" => {
                            if let Some(mut pattern) = current_pattern.take() {
                                // Set marker bounds if a marker was found inside the pattern
                                if let Some(start) = pattern_marker_start {
                                    pattern.marker_start = Some(start);
                                    pattern.marker_end = pattern_marker_end;
                                }
                                if let Some(rule) = &mut current_rule {
                                    rule.pattern = pattern;
                                }
                            }
                            in_pattern_marker = false;
                            pattern_marker_start = None;
                            pattern_marker_end = None;
                        }
                        "antipattern" => {
                            in_antipattern = false;
                            if let Some(ap) = current_antipattern.take() {
                                if let Some(rule) = &mut current_rule {
                                    rule.antipatterns.push(ap);
                                } else if let Some(group) = &mut current_group {
                                    // Store rulegroup-level antipatterns for inheritance
                                    group.antipatterns.push(ap);
                                }
                            }
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
                                if in_or_group {
                                    if let Some(ref mut or_group) = current_or_group {
                                        or_group.alternatives.push(token);
                                    }
                                } else if in_and_group {
                                    if let Some(ref mut and_group) = current_and_group {
                                        and_group.constraints.push(token);
                                    }
                                } else if in_antipattern {
                                    if let Some(ref mut ap) = current_antipattern {
                                        ap.tokens.push(token.clone());
                                        ap.elements.push(XmlPatternElement::Token(token));
                                    }
                                } else if let Some(pattern) = &mut current_pattern {
                                    pattern.tokens.push(token.clone());
                                    pattern.elements.push(XmlPatternElement::Token(token));
                                }
                            }
                            current_text.clear();
                        }
                        "message" => {
                            in_message = false;
                            if let Some(rule) = &mut current_rule {
                                rule.message = message_text.trim().to_string();
                            }
                        }
                        "suggestion" => {
                            in_suggestion = false;
                            if let Some(rule) = &mut current_rule {
                                rule.suggestions.push(XmlSuggestion {
                                    text: suggestion_text.trim().to_string(),
                                    parts: suggestion_parts.clone(),
                                });
                            }
                            suggestion_parts.clear();
                        }
                        "example" => {
                            in_example = false;
                            if let Some(rule) = &mut current_rule {
                                rule.examples.push(XmlExample {
                                    example_type: example_type.clone(),
                                    text: example_text.trim().to_string(),
                                    corrections: Vec::new(),
                                });
                            }
                        }
                        "marker" => {
                            if in_example {
                                example_text.push_str("</marker>");
                            } else if current_pattern.is_some() {
                                in_pattern_marker = false;
                                if let Some(ref pattern) = current_pattern {
                                    pattern_marker_end = Some(pattern.tokens.len());
                                }
                            }
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

        Ok(file)
    }

    fn strip_doctype(xml: &str) -> String {
        if let Some(start) = xml.find("<!DOCTYPE") {
            if let Some(end) = xml[start..].find("]>") {
                return xml[..start].to_string() + &xml[start + end + 2..];
            }
        }
        xml.to_string()
    }

    fn expand_entities(xml: &str) -> String {
        let mut result = xml.to_string();
        // Extract all entity definitions
        let double_q = regex::Regex::new(r#"<!ENTITY\s+(\w+)\s+"([^"]*)">"#).unwrap();
        let single_q = regex::Regex::new(r#"<!ENTITY\s+(\w+)\s+'([^']*)'>"#).unwrap();
        let mut entities: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for cap in double_q.captures_iter(xml) {
            entities.insert(cap[1].to_string(), cap[2].to_string());
        }
        for cap in single_q.captures_iter(xml) {
            entities.insert(cap[1].to_string(), cap[2].to_string());
        }

        // Iteratively expand entities until no more changes (handles nested entities)
        loop {
            let mut changed = false;
            for (name, value) in &entities {
                let placeholder = format!("&{};", name);
                if result.contains(&placeholder) {
                    result = result.replace(&placeholder, value);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
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
        assert_eq!(result.rules[0].pattern.tokens[0].text.as_deref(), Some("foo"));
        assert_eq!(result.rules[0].pattern.tokens[1].text.as_deref(), Some("bar"));
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
        assert_eq!(result.rules[0].pattern.tokens[0].regexp.as_deref(), Some("yes"));
        assert_eq!(result.rules[0].pattern.tokens[0].text.as_deref(), Some("foo|bar"));
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
        assert_eq!(result.rules[0].pattern.tokens[0].text.as_deref(), Some("apple"));
        assert_eq!(result.rules[0].message, "Message one");

        assert_eq!(result.rules[1].id, "RULE_TWO");
        assert_eq!(result.rules[1].name, "Second rule");
        assert_eq!(result.rules[1].pattern.tokens.len(), 2);
        assert_eq!(result.rules[1].pattern.tokens[0].text.as_deref(), Some("banana"));
        assert_eq!(result.rules[1].pattern.tokens[1].text.as_deref(), Some("cherry"));
        assert_eq!(result.rules[1].message, "Message two");

        assert_eq!(result.rules[2].id, "RULE_THREE");
        assert_eq!(result.rules[2].name, "Third rule");
        assert_eq!(result.rules[2].pattern.tokens.len(), 1);
        assert_eq!(result.rules[2].pattern.tokens[0].text.as_deref(), Some("date"));
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
        assert_eq!(result.rules[0].pattern.tokens[1].text.as_deref(), Some("good"));
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
        // Self-closing exception inside a non-empty token element.
        // Uses <token>test</token> with a self-closing <exception/> to avoid
        // the parser accumulating all child text into the token's text field.
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
        // Note: with inline non-self-closing exceptions, the token text accumulates
        // all child text. Here we assert what the parser actually produces.
        assert_eq!(token.exceptions.len(), 1);

        let exc = &token.exceptions[0];
        assert_eq!(exc.regexp.as_deref(), Some("yes"));
        assert!(exc.negate);
        assert_eq!(exc.scope.as_deref(), Some("next"));
    }

    #[test]
    fn test_parse_exception_with_postag() {
        // Self-closing exception to verify postag/postag_regexp/inflected attrs
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
        // Non-self-closing exceptions with text are now properly parsed.
        // The token text includes only the text before the exception.
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
        // Token text is "base" (text before the exception only)
        assert_eq!(token.text.as_deref(), Some("base"));
        // Non-self-closing exceptions with text are now properly captured
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
        assert_eq!(result.rules[0].message, "Use option1 or option2 or option3.");
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
        assert_eq!(result.rules[0].pattern.tokens[0].text.as_deref(), Some("first"));
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
        assert_eq!(group.rules[0].pattern.tokens[0].text.as_deref(), Some("alpha"));
        assert_eq!(group.rules[0].message, "Alpha message");

        assert_eq!(group.rules[1].id, "RULE_G2");
        assert_eq!(group.rules[1].name, "Grouped rule 2");
        assert_eq!(group.rules[1].pattern.tokens[0].text.as_deref(), Some("beta"));
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
        assert_eq!(result.rules[0].examples[0].example_type, XmlExampleType::Correct);
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
        assert_eq!(result.rules[0].examples[0].example_type, XmlExampleType::Incorrect);
        assert_eq!(result.rules[0].examples[0].text, "This is <marker>wrong</marker> here.");
    }

    #[test]
    fn test_parse_example_implicit_incorrect_from_marker() {
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

        // Examples with <marker> tags are implicitly incorrect
        assert_eq!(result.rules[0].examples[0].example_type, XmlExampleType::Incorrect);
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

        assert_eq!(result.rules[0].examples[0].example_type, XmlExampleType::Correct);
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

        // Rules default to true when no default attr is present
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
        assert_eq!(result.rules[0].pattern.tokens[0].text.as_deref(), Some("run"));
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
        assert_eq!(result.rules[0].examples[0].example_type, XmlExampleType::Incorrect);
        assert_eq!(result.rules[0].examples[1].example_type, XmlExampleType::Incorrect);
        assert_eq!(result.rules[0].examples[2].example_type, XmlExampleType::Correct);
        assert_eq!(result.rules[0].examples[3].example_type, XmlExampleType::Correct);
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
        assert!(result.categories[0].default_on); // explicit on
        assert_eq!(result.categories[1].id, "STYLE");
        assert!(!result.categories[1].default_on); // explicit off
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

        // sub_id defaults to None when not in XML attributes
        assert_eq!(result.rules[0].sub_id, None);
    }

    #[test]
    fn test_parse_exception_multiple() {
        // Uses self-closing exceptions to avoid text accumulation issues
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
}
