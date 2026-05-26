use crate::disambig_types::*;
use crate::types::*;
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, thiserror::Error)]
pub enum DisambigParseError {
    #[error("XML parse error: {0}")]
    Parse(#[from] quick_xml::Error),
    #[error("Invalid rule: {0}")]
    InvalidRule(String),
}

pub struct DisambigXmlParser;

impl DisambigXmlParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse disambiguation XML, expanding DOCTYPE entities manually.
    pub fn parse(&self, xml: &str) -> Result<DisambigRuleSet, DisambigParseError> {
        // quick-xml doesn't expand DOCTYPE entities, so we need to handle them manually.
        // Step 1: Expand entities (replace &name; with values from DOCTYPE)
        // Step 2: Strip DOCTYPE declaration
        // Step 3: Handle comments inside DOCTYPE (which may contain problematic chars)
        let expanded = Self::expand_entities(xml);
        let cleaned = Self::strip_doctype(&expanded);

        let mut reader = Reader::from_str(&cleaned);
        reader.config_mut().trim_text(false);

        let mut rule_set = DisambigRuleSet::default();
        let mut current_rule: Option<DisambigRule> = None;
        let mut current_pattern: Option<XmlPattern> = None;
        let mut current_token: Option<XmlPatternToken> = None;
        let mut current_group_id: Option<String> = None;
        let mut current_group_name: Option<String> = None;
        let mut in_antipattern = false;
        let mut current_antipattern: Option<XmlPattern> = None;
        let mut antipattern_token: Option<XmlPatternToken> = None;
        let mut in_pattern_marker = false;
        let mut pattern_marker_start: Option<usize> = None;
        let mut in_exception = false;
        let mut current_exception: Option<XmlException> = None;
        let mut exception_text = String::new();
        let mut in_or_group = false;
        let mut current_or_group: Option<XmlOrGroup> = None;
        let mut in_and_group = false;
        let mut current_and_group: Option<XmlAndGroup> = None;
        let mut current_text = String::new();
        let mut buf = Vec::new();
        let mut in_wd = false;
        let mut current_wd: DisambigWord = DisambigWord::default();
        let mut current_disambig: Option<DisambigAction> = None;
        let mut disambig_wds: Vec<DisambigWord> = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let local_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match local_name.as_str() {
                        "rule" => {
                            let mut rule = DisambigRule::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "id" => rule.id = Some(val),
                                    "name" => rule.name = Some(val),
                                    _ => {}
                                }
                            }
                            if let Some(id) = current_group_id.take() {
                                rule.id = rule.id.or(Some(id));
                            }
                            if let Some(name) = current_group_name.take() {
                                rule.name = rule.name.or(Some(name));
                            }
                            current_rule = Some(rule);
                        }
                        "rulegroup" => {
                            current_group_id = None;
                            current_group_name = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "id" => current_group_id = Some(val),
                                    "name" => current_group_name = Some(val),
                                    _ => {}
                                }
                            }
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
                            current_antipattern = Some(XmlPattern::default());
                        }
                        "marker" if current_pattern.is_some() => {
                            in_pattern_marker = true;
                            pattern_marker_start = if let Some(ref p) = current_pattern {
                                Some(p.tokens.len())
                            } else {
                                None
                            };
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
                                    _ => {}
                                }
                            }
                            current_text.clear();
                            if in_or_group {
                                current_token = Some(token);
                            } else if in_and_group {
                                current_token = Some(token);
                            } else if in_antipattern {
                                antipattern_token = Some(token);
                            } else {
                                current_token = Some(token);
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
                        "disambig" => {
                            let mut action: Option<String> = None;
                            let mut postag: Option<String> = None;
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "action" => action = Some(val),
                                    "postag" => postag = Some(val),
                                    _ => {}
                                }
                            }
                            disambig_wds.clear();
                            current_disambig = match action.as_deref() {
                                Some("replace") => Some(DisambigAction::Replace(Vec::new())),
                                Some("remove") => Some(DisambigAction::Remove(Vec::new())),
                                Some("add") => Some(DisambigAction::Add(Vec::new())),
                                Some("filter") => {
                                    let postag = postag.unwrap_or_default();
                                    Some(DisambigAction::Filter { postag })
                                }
                                Some("filterall") => Some(DisambigAction::FilterAll),
                                Some("ignore_spelling") => Some(DisambigAction::IgnoreSpelling),
                                Some("unify") => Some(DisambigAction::Unify),
                                None if postag.is_some() => {
                                    Some(DisambigAction::SetPos(postag.unwrap()))
                                }
                                _ => None,
                            };
                        }
                        "wd" => {
                            in_wd = true;
                            current_wd = DisambigWord::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
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
                                    _ => {}
                                }
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
                                    ap.tokens.push(token.clone());
                                    ap.elements.push(XmlPatternElement::Token(token));
                                }
                            } else if let Some(ref mut p) = current_pattern {
                                p.tokens.push(token.clone());
                                p.elements.push(XmlPatternElement::Token(token));
                            }
                        }
                        "wd" => {
                            let mut wd = DisambigWord::default();
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                                let val = String::from_utf8_lossy(&attr.value).to_string();
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
                                let val = String::from_utf8_lossy(&attr.value).to_string();
                                match key.as_str() {
                                    "action" => action = Some(val),
                                    "postag" => postag = Some(val),
                                    _ => {}
                                }
                            }
                            current_disambig = match action.as_deref() {
                                Some("replace") => Some(DisambigAction::Replace(Vec::new())),
                                Some("remove") => Some(DisambigAction::Remove(Vec::new())),
                                Some("add") => Some(DisambigAction::Add(Vec::new())),
                                Some("filter") => {
                                    let postag = postag.unwrap_or_default();
                                    Some(DisambigAction::Filter { postag })
                                }
                                Some("filterall") => Some(DisambigAction::FilterAll),
                                Some("ignore_spelling") => Some(DisambigAction::IgnoreSpelling),
                                Some("unify") => Some(DisambigAction::Unify),
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
                    let text = e.unescape().unwrap_or_default().to_string();
                    if in_wd {
                        // <wd> can have text content (the lemma) but we already
                        // capture lemma from attribute
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
                                rule_set.rules.push(rule);
                            }
                            disambig_wds.clear();
                        }
                        "rulegroup" => {
                            current_group_id = None;
                            current_group_name = None;
                        }
                        "pattern" => {
                            if let Some(pattern) = current_pattern.take() {
                                if let Some(ref mut rule) = current_rule {
                                    rule.pattern = pattern;
                                }
                            }
                            in_pattern_marker = false;
                            pattern_marker_start = None;
                        }
                        "antipattern" => {
                            if let Some(ap) = current_antipattern.take() {
                                if let Some(ref mut rule) = current_rule {
                                    rule.antipatterns.push(ap);
                                }
                            }
                            in_antipattern = false;
                        }
                        "marker" if current_pattern.is_some() && in_pattern_marker => {
                            in_pattern_marker = false;
                            if let Some(ref mut p) = current_pattern {
                                p.marker_start = pattern_marker_start;
                                p.marker_end = Some(p.tokens.len());
                            }
                        }
                        "exception" => {
                            if in_exception {
                                in_exception = false;
                                let exc = current_exception.take();
                                if let Some(mut exc) = exc {
                                    if !exception_text.is_empty() {
                                        exc.text = Some(exception_text.trim().to_string());
                                    }
                                    if in_or_group {
                                        if let Some(ref mut g) = current_or_group {
                                            if let Some(last) = g.alternatives.last_mut() {
                                                last.exceptions.push(exc);
                                            }
                                        }
                                    } else if in_and_group {
                                        if let Some(ref mut g) = current_and_group {
                                            if let Some(last) = g.constraints.last_mut() {
                                                last.exceptions.push(exc);
                                            }
                                        }
                                    } else if in_antipattern {
                                        if let Some(ref mut ap) = current_antipattern {
                                            if let Some(last) = ap.tokens.last_mut() {
                                                last.exceptions.push(exc);
                                            }
                                        }
                                    } else if let Some(ref mut p) = current_pattern {
                                        if let Some(last) = p.tokens.last_mut() {
                                            last.exceptions.push(exc);
                                        }
                                    }
                                }
                                exception_text.clear();
                            }
                        }
                        "token" => {
                            // Handle both regular tokens and antipattern tokens
                            let token = current_token.take().or(antipattern_token.take());
                            if let Some(mut token) = token {
                                // Get text content from non-self-closing token
                                if !current_text.is_empty() {
                                    token.text = Some(current_text.trim().to_string());
                                }
                                current_text.clear();

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
                                        ap.tokens.push(token.clone());
                                        ap.elements.push(XmlPatternElement::Token(token));
                                    }
                                } else if let Some(ref mut p) = current_pattern {
                                    p.tokens.push(token.clone());
                                    p.elements.push(XmlPatternElement::Token(token));
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
                        "disambig" => {
                            // Store collected <wd> elements into the current disambig action
                            if let Some(ref mut da) = current_disambig {
                                match da {
                                    DisambigAction::Replace(wds) => {
                                        wds.extend(disambig_wds.drain(..));
                                    }
                                    DisambigAction::Remove(wds) => {
                                        wds.extend(disambig_wds.drain(..));
                                    }
                                    DisambigAction::Add(wds) => {
                                        wds.extend(disambig_wds.drain(..));
                                    }
                                    _ => {}
                                }
                            }
                            disambig_wds.clear();
                        }
                        "wd" => {
                            if in_wd {
                                in_wd = false;
                                disambig_wds.push(current_wd.clone());
                            }
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

    /// Strip DOCTYPE declaration from XML (quick-xml can't parse it).
    /// DOCTYPE internal subset ends with `]>` — we need to find that specifically.
    fn strip_doctype(xml: &str) -> String {
        if let Some(start) = xml.find("<!DOCTYPE") {
            // Find the end of the DOCTYPE internal subset: "]>"
            if let Some(end) = xml[start..].find("]>") {
                return xml[..start].to_string() + &xml[start + end + 2..];
            }
        }
        xml.to_string()
    }

    /// Expand commonly used entities in the disambiguation XML
    fn expand_entities(xml: &str) -> String {
        let mut result = xml.to_string();

        // Extract entity definitions from DOCTYPE
        let entity_pattern = regex::Regex::new(r#"<!ENTITY\s+(\w+)\s+"([^"]*)">"#).unwrap();
        for cap in entity_pattern.captures_iter(xml) {
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
        assert_eq!(result.rules[0].pattern.tokens[1].space_before.as_deref(), Some("no"));
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
        // Entity should have been expanded in the token text
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
