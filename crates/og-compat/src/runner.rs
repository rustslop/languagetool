use crate::compare::{ComparisonResult, ComparisonStatus, JavaMatch};
use crate::report::CompatReport;
use og_core::CheckRequest;
use og_langs::engine::LanguageEngine;
use std::sync::Arc;

pub struct GoldenMatch {
    pub rule_id: &'static str,
    pub offset: usize,
    pub length: usize,
    pub replacements: Vec<&'static str>,
    pub message_contains: Option<&'static str>,
    pub presence_only: bool,
}

impl GoldenMatch {
    pub fn new(rule_id: &'static str, offset: usize, length: usize) -> Self {
        Self {
            rule_id,
            offset,
            length,
            replacements: Vec::new(),
            message_contains: None,
            presence_only: false,
        }
    }

    pub fn presence_only(mut self) -> Self {
        self.presence_only = true;
        self
    }

    pub fn with_replacements(mut self, replacements: Vec<&'static str>) -> Self {
        self.replacements = replacements;
        self
    }

    pub fn with_message(mut self, fragment: &'static str) -> Self {
        self.message_contains = Some(fragment);
        self
    }
}

pub struct GoldenCase {
    pub name: &'static str,
    pub text: &'static str,
    pub matches: Vec<GoldenMatch>,
}

impl GoldenCase {
    pub fn new(name: &'static str, text: &'static str) -> Self {
        Self {
            name,
            text,
            matches: Vec::new(),
        }
    }

    pub fn expect(mut self, rule_id: &'static str, offset: usize, length: usize) -> Self {
        self.matches.push(GoldenMatch::new(rule_id, offset, length));
        self
    }

    pub fn presence_only_marker(mut self) -> Self {
        if let Some(last) = self.matches.last_mut() {
            last.presence_only = true;
        }
        self
    }

    pub fn presence_only_expect(mut self, rule_id: &'static str) -> Self {
        self.matches
            .push(GoldenMatch::new(rule_id, usize::MAX, 0).presence_only());
        self
    }

    pub fn expect_with(
        mut self,
        rule_id: &'static str,
        offset: usize,
        length: usize,
        replacements: Vec<&'static str>,
    ) -> Self {
        self.matches
            .push(GoldenMatch::new(rule_id, offset, length).with_replacements(replacements));
        self
    }
}

pub struct GoldenRunner {
    engine: Arc<LanguageEngine>,
    picky: bool,
}

impl GoldenRunner {
    pub fn english() -> Self {
        Self {
            engine: Arc::new(LanguageEngine::english()),
            picky: true,
        }
    }

    pub fn rule_count(&self) -> usize {
        self.engine.rule_count()
    }

    pub fn from_engine(engine: Arc<LanguageEngine>) -> Self {
        Self {
            engine,
            picky: true,
        }
    }

    pub fn run(&self, cases: &[GoldenCase]) -> CompatReport {
        let mut results = Vec::new();
        for case in cases {
            let mut request = self.request(case.text);
            if !case.matches.is_empty() {
                request.enabled_rules = case
                    .matches
                    .iter()
                    .map(|m| m.rule_id.to_string())
                    .collect::<Vec<_>>();
            }
            let rust_result = self.engine.check_with_request(&request);
            let rust_matches = rust_result.matches;
            let java_matches: Vec<JavaMatch> = case
                .matches
                .iter()
                .map(|expected| {
                    if expected.presence_only
                        && let Some(found) = rust_matches
                            .iter()
                            .find(|m| m.rule().id() == expected.rule_id)
                    {
                        return JavaMatch {
                            rule_id: expected.rule_id.to_string(),
                            message: String::new(),
                            offset: found.offset(),
                            length: found.length(),
                            replacements: found
                                .replacements()
                                .iter()
                                .map(|r| r.value().to_string())
                                .collect(),
                            category: String::new(),
                            issue_type: String::new(),
                        };
                    }
                    JavaMatch {
                        rule_id: expected.rule_id.to_string(),
                        message: expected.message_contains.unwrap_or_default().to_string(),
                        offset: expected.offset,
                        length: expected.length,
                        replacements: expected
                            .replacements
                            .iter()
                            .map(|r| r.to_string())
                            .collect(),
                        category: String::new(),
                        issue_type: String::new(),
                    }
                })
                .collect();
            let expected_rules: Vec<&str> = case.matches.iter().map(|m| m.rule_id).collect();
            let mut case_results: Vec<ComparisonResult> =
                crate::compare::compare_results(&rust_matches, &java_matches)
                    .into_iter()
                    .filter(|r| {
                        r.status != ComparisonStatus::ExtraInRust
                            || expected_rules.contains(&r.rule_id.as_str())
                    })
                    .collect();
            for expected in &case.matches {
                if let Some(fragment) = expected.message_contains
                    && !rust_matches.iter().any(|m| {
                        m.rule().id() == expected.rule_id && m.message().contains(fragment)
                    })
                {
                    case_results.push(ComparisonResult {
                        rule_id: expected.rule_id.to_string(),
                        status: ComparisonStatus::MessageMismatch,
                        details: format!("expected message containing {fragment:?}"),
                    });
                }
            }
            results.extend(case_results);
        }
        CompatReport::new(results)
    }

    fn request(&self, text: &str) -> CheckRequest {
        let mut request = CheckRequest::new(text, self.engine.language().clone());
        request.picky = self.picky;
        request.run_all_rules = true;
        request
    }
}
