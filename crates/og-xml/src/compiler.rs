use crate::types::*;
use crate::parser::*;

pub struct XmlCompiler;

impl XmlCompiler {
    pub fn new() -> Self {
        Self
    }

    /// Anchor a regex pattern for full-string matching (Java String.matches() behavior).
    /// Wraps the pattern with `^(?:...)$` unless already anchored.
    fn anchor_regex(pattern: &str) -> String {
        let trimmed = pattern.trim();
        if trimmed.starts_with('^') && trimmed.ends_with('$') {
            return trimmed.to_string();
        }
        format!("^(?:{})$", trimmed)
    }

    pub fn compile_file(&self, xml: &str) -> Result<CompiledRuleSet, XmlParseError> {
        let parser = XmlRuleParser::new();
        let rule_file = parser.parse(xml)?;

        let mut compiled = CompiledRuleSet {
            rules: Vec::new(),
            categories: rule_file.categories.clone(),
        };

        for rule in &rule_file.rules {
            compiled.rules.push(self.compile_rule(rule));
        }

        for group in &rule_file.rule_groups {
            for (i, rule) in group.rules.iter().enumerate() {
                let mut compiled_rule = self.compile_rule(rule);
                // If rule doesn't have its own ID, inherit the group's ID
                if compiled_rule.id.is_empty() {
                    if let Some(ref group_id) = group.id {
                        compiled_rule.id = group_id.clone();
                        // Add sub-id based on position within group
                        compiled_rule.sub_id = Some(format!("{}", i + 1));
                    }
                }
                // Inherit group-level issue_type if rule doesn't have its own
                if compiled_rule.issue_type == "grammar" {
                    if let Some(ref git) = group.issue_type {
                        compiled_rule.issue_type = git.clone();
                    }
                }
                compiled.rules.push(compiled_rule);
            }
        }

        Ok(compiled)
    }

    fn compile_rule(&self, rule: &XmlRule) -> CompiledRule {
        CompiledRule {
            id: rule.id.clone(),
            sub_id: rule.sub_id.clone(),
            name: rule.name.clone(),
            description: rule.description.clone(),
            category: rule.category.clone(),
            antipatterns: rule.antipatterns.iter().map(|ap| {
                let pat_cs = ap.case_sensitive;
                CompiledPattern {
                    tokens: ap.tokens.iter().map(|t| {
                        let mut ct = self.compile_token(t);
                        if pat_cs { ct.case_sensitive = true; }
                        ct
                    }).collect(),
                    elements: ap.elements.iter().map(|e| {
                        match e {
                            XmlPatternElement::Token(t) => {
                                let mut ct = self.compile_token(t);
                                if pat_cs { ct.case_sensitive = true; }
                                CompiledPatternElement::Token(ct)
                            }
                            XmlPatternElement::OrGroup(g) => {
                                CompiledPatternElement::OrGroup(
                                    g.alternatives.iter().map(|t| {
                                        let mut ct = self.compile_token(t);
                                        if pat_cs { ct.case_sensitive = true; }
                                        ct
                                    }).collect()
                                )
                            }
                            XmlPatternElement::AndGroup(g) => {
                                CompiledPatternElement::AndGroup(
                                    g.constraints.iter().map(|t| {
                                        let mut ct = self.compile_token(t);
                                        if pat_cs { ct.case_sensitive = true; }
                                        ct
                                    }).collect()
                                )
                            }
                        }
                    }).collect(),
                    case_sensitive: pat_cs,
                    marker_start: None,
                    marker_end: None,
                }
            }).collect(),
            pattern: {
                let pat_cs = rule.pattern.case_sensitive;
                CompiledPattern {
                    tokens: rule.pattern.tokens.iter().map(|t| {
                        let mut ct = self.compile_token(t);
                        if pat_cs { ct.case_sensitive = true; }
                        ct
                    }).collect(),
                    elements: rule.pattern.elements.iter().map(|e| {
                        match e {
                            XmlPatternElement::Token(t) => {
                                let mut ct = self.compile_token(t);
                                if pat_cs { ct.case_sensitive = true; }
                                CompiledPatternElement::Token(ct)
                            }
                            XmlPatternElement::OrGroup(g) => {
                                CompiledPatternElement::OrGroup(
                                    g.alternatives.iter().map(|t| {
                                        let mut ct = self.compile_token(t);
                                        if pat_cs { ct.case_sensitive = true; }
                                        ct
                                    }).collect()
                                )
                            }
                            XmlPatternElement::AndGroup(g) => {
                                CompiledPatternElement::AndGroup(
                                    g.constraints.iter().map(|t| {
                                        let mut ct = self.compile_token(t);
                                        if pat_cs { ct.case_sensitive = true; }
                                        ct
                                    }).collect()
                                )
                            }
                        }
                    }).collect(),
                    case_sensitive: pat_cs,
                    marker_start: rule.pattern.marker_start,
                    marker_end: rule.pattern.marker_end,
                }
            },
            message: rule.message.clone(),
            short_message: rule.short_message.clone(),
            suggestions: rule.suggestions.iter().map(|s| CompiledSuggestion {
                text: s.text.clone(),
                parts: s.parts.clone(),
            }).collect(),
            examples: rule.examples.clone(),
            url: rule.url.clone(),
            default_on: rule.default_on,
            deprecated: rule.deprecated,
            filter: rule.filter.clone(),
            issue_type: rule.issue_type.clone()
                .or_else(|| rule.category.issue_type.clone())
                .unwrap_or_else(|| "grammar".to_string()),
        }
    }

    pub fn compile_token(&self, token: &XmlPatternToken) -> CompiledPatternToken {
        // When regexp="yes", the token text IS the regex pattern.
        // Java's String.matches() requires full-string match, so anchor with ^(?:...)$
        let compiled_re = if token.regexp.as_deref() == Some("yes") {
            token.text.as_ref().and_then(|t| {
                let anchored = Self::anchor_regex(t);
                regex::Regex::new(&anchored).ok()
            })
        } else {
            token.regexp.as_ref().and_then(|r| {
                let anchored = Self::anchor_regex(r);
                regex::Regex::new(&anchored).ok()
            })
        };
        // Pre-compile case-insensitive version for non-case-sensitive regex matching
        let compiled_re_ci = if token.regexp.as_deref() == Some("yes") {
            token.text.as_ref().and_then(|t| {
                let anchored = Self::anchor_regex(t);
                regex::RegexBuilder::new(&anchored).case_insensitive(true).build().ok()
            })
        } else {
            token.regexp.as_ref().and_then(|r| {
                let anchored = Self::anchor_regex(r);
                regex::RegexBuilder::new(&anchored).case_insensitive(true).build().ok()
            })
        };
        let postag_re = if token.postag_regexp.as_deref() == Some("yes") {
            token.postag.as_ref().and_then(|t| regex::Regex::new(t).ok())
        } else {
            token.postag_regexp.as_ref().and_then(|r| regex::Regex::new(r).ok())
        };

        // When regexp="yes", text is the pattern, not a literal match
        let text_value = if token.regexp.as_deref() == Some("yes") {
            None // Don't use as literal text match
        } else {
            token.text.clone()
        };

        CompiledPatternToken {
            text: text_value,
            compiled_regexp: compiled_re,
            compiled_regexp_ci: compiled_re_ci,
            postag: token.postag.clone(),
            compiled_postag_regexp: postag_re,
            negate: token.negate,
            negate_pos: token.negate_pos,
            case_sensitive: token.case_sensitive,
            inflected: token.inflected,
            min: token.min,
            max: token.max,
            skip: token.skip,
            exceptions: token.exceptions.iter().map(|e| self.compile_exception(e)).collect(),
            space_before: token.space_before.clone(),
            chunk: token.chunk.clone(),
            chunk_re: token.chunk_re.as_ref().and_then(|r| {
                let anchored = Self::anchor_regex(r);
                regex::Regex::new(&anchored).ok()
            }),
            match_no: token.match_no,
        }
    }

    pub fn compile_element(&self, element: &XmlPatternElement) -> CompiledPatternElement {
        match element {
            XmlPatternElement::Token(t) => CompiledPatternElement::Token(self.compile_token(t)),
            XmlPatternElement::OrGroup(g) => {
                CompiledPatternElement::OrGroup(
                    g.alternatives.iter().map(|t| self.compile_token(t)).collect()
                )
            }
            XmlPatternElement::AndGroup(g) => {
                CompiledPatternElement::AndGroup(
                    g.constraints.iter().map(|t| self.compile_token(t)).collect()
                )
            }
        }
    }

    fn compile_exception(&self, exc: &XmlException) -> CompiledException {
        let compiled_re = if exc.regexp.as_deref() == Some("yes") {
            exc.text.as_ref().and_then(|t| {
                let anchored = Self::anchor_regex(t);
                regex::Regex::new(&anchored).ok()
            })
        } else {
            exc.regexp.as_ref().and_then(|r| {
                let anchored = Self::anchor_regex(r);
                regex::Regex::new(&anchored).ok()
            })
        };
        let compiled_postag_re = if exc.postag_regexp.as_deref() == Some("yes") {
            exc.postag.as_ref().and_then(|t| regex::Regex::new(t).ok())
        } else {
            None
        };
        CompiledException {
            text: exc.text.clone(),
            compiled_regexp: compiled_re,
            postag: exc.postag.clone(),
            compiled_postag_regexp: compiled_postag_re,
            negate: exc.negate,
            negate_pos: exc.negate_pos,
            inflected: exc.inflected,
            case_sensitive: exc.case_sensitive,
            scope: exc.scope.clone(),
        }
    }
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
}

#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub id: String,
    pub sub_id: Option<String>,
    pub name: String,
    pub description: String,
    pub category: XmlCategory,
    pub antipatterns: Vec<CompiledPattern>,
    pub pattern: CompiledPattern,
    pub message: String,
    pub short_message: Option<String>,
    pub suggestions: Vec<CompiledSuggestion>,
    pub examples: Vec<XmlExample>,
    pub url: Option<String>,
    pub default_on: bool,
    pub deprecated: bool,
    pub filter: Option<crate::types::XmlFilter>,
    pub issue_type: String,
}

/// A compiled pattern element - either a single token, an or-group, or an and-group
#[derive(Debug, Clone)]
pub enum CompiledPatternElement {
    Token(CompiledPatternToken),
    OrGroup(Vec<CompiledPatternToken>),
    AndGroup(Vec<CompiledPatternToken>),
}

#[derive(Debug, Clone)]
pub struct CompiledPattern {
    pub tokens: Vec<CompiledPatternToken>,
    /// Structured elements including or/and groups
    pub elements: Vec<CompiledPatternElement>,
    pub case_sensitive: bool,
    /// Index of first token inside <marker>, if any
    pub marker_start: Option<usize>,
    /// Index after last token inside <marker>, if any
    pub marker_end: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct CompiledPatternToken {
    pub text: Option<String>,
    pub compiled_regexp: Option<regex::Regex>,
    /// Case-insensitive version of compiled_regexp (pre-compiled for performance)
    pub compiled_regexp_ci: Option<regex::Regex>,
    pub postag: Option<String>,
    pub compiled_postag_regexp: Option<regex::Regex>,
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
    pub chunk_re: Option<regex::Regex>,
    /// Backreference: match the same text as pattern token N (1-based).
    pub match_no: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct CompiledException {
    pub text: Option<String>,
    pub compiled_regexp: Option<regex::Regex>,
    pub postag: Option<String>,
    pub compiled_postag_regexp: Option<regex::Regex>,
    pub negate: bool,
    pub negate_pos: bool,
    pub inflected: bool,
    pub case_sensitive: bool,
    pub scope: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompiledSuggestion {
    pub text: String,
    pub parts: Vec<SuggestionPart>,
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // Non-regexp tokens preserve text as literal match
        assert_eq!(result.rules[0].pattern.tokens[0].text.as_deref(), Some("foo"));
        assert_eq!(result.rules[0].pattern.tokens[1].text.as_deref(), Some("bar"));
        // No compiled regexp for literal tokens
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
        // When regexp="yes", text is not used as literal match
        assert!(token.text.is_none());
        // But compiled_regexp should be present
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
        assert_eq!(result.rules[0].pattern.tokens[0].text.as_deref(), Some("bad"));
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

        assert_eq!(result.rules[0].pattern.tokens[0].postag.as_deref(), Some("JJ"));
        // No compiled postag regexp when postag_regexp is not "yes"
        assert!(result.rules[0].pattern.tokens[0].compiled_postag_regexp.is_none());
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
        assert!(token.compiled_postag_regexp.as_ref().unwrap().is_match("NN"));
        assert!(token.compiled_postag_regexp.as_ref().unwrap().is_match("NNS"));
        assert!(!token.compiled_postag_regexp.as_ref().unwrap().is_match("VB"));
    }

    #[test]
    fn test_compile_exception_on_token() {
        // Uses self-closing exception to avoid text accumulation issues
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

        // Grouped rules should appear in compiled.rules
        assert_eq!(result.rules.len(), 2);
        assert_eq!(result.rules[0].id, "GR1");
        assert_eq!(result.rules[0].pattern.tokens[0].text.as_deref(), Some("alpha"));
        assert_eq!(result.rules[1].id, "GR2");
        assert_eq!(result.rules[1].pattern.tokens[0].text.as_deref(), Some("beta"));
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
        // Rule also carries category
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
        assert_eq!(result.rules[0].examples[0].example_type, XmlExampleType::Incorrect);
        assert_eq!(result.rules[0].examples[1].example_type, XmlExampleType::Correct);
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

        // Both standalone rules and grouped rules appear in compiled.rules
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
        // Uses self-closing exception to avoid text accumulation issues
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
        // Uses self-closing exceptions to avoid text accumulation issues
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
        assert!(token.text.is_none()); // regexp="yes" means text is pattern, not literal
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
        // Test that antipattern tokens with text content are properly captured
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
        // Rule should inherit group-level antipatterns
        assert!(!rule.antipatterns.is_empty(), "Rule should have inherited antipatterns from group");

        let ap = &rule.antipatterns[0];
        assert_eq!(ap.tokens.len(), 2, "Antipattern should have 2 tokens");

        // Token 0: inflected="yes" be
        let tok0 = &ap.tokens[0];
        assert_eq!(tok0.text.as_deref(), Some("be"), "Token 0 should have text 'be'");
        assert!(tok0.inflected, "Token 0 should be inflected");

        // Token 1: regexp="yes" romeo|matt|mike
        let tok1 = &ap.tokens[1];
        // With regexp="yes", text is None (used as pattern) but compiled_regexp should be Some
        assert!(tok1.compiled_regexp.is_some(),
            "Token 1 should have compiled_regexp (text={:?}, regexp_attr={:?})",
            tok1.text, "yes");
        assert!(tok1.compiled_regexp.as_ref().unwrap().is_match("romeo"));
        assert!(tok1.compiled_regexp.as_ref().unwrap().is_match("matt"));
        assert!(!tok1.compiled_regexp.as_ref().unwrap().is_match("other"));
    }
}
