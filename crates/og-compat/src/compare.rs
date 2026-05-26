use og_core::RuleMatch;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaMatch {
    pub rule_id: String,
    pub message: String,
    pub offset: usize,
    pub length: usize,
    pub replacements: Vec<String>,
    pub category: String,
    pub issue_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaCheckResult {
    pub matches: Vec<JavaMatch>,
}

impl JavaCheckResult {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonStatus {
    Pass,
    OffsetMismatch,
    LengthMismatch,
    ReplacementMismatch,
    MessageMismatch,
    MissingInRust,
    ExtraInRust,
    CategoryMismatch,
}

#[derive(Debug, Clone)]
pub struct ComparisonResult {
    pub rule_id: String,
    pub status: ComparisonStatus,
    pub details: String,
}

/// Compare Rust RuleMatch results against Java LT results.
/// Uses fuzzy matching: matches by (rule_id, approximate offset) to handle
/// minor offset differences from tokenization.
pub fn compare_results(
    rust_matches: &[RuleMatch],
    java_matches: &[JavaMatch],
) -> Vec<ComparisonResult> {
    let mut results = Vec::new();
    let mut rust_matched: Vec<bool> = vec![false; rust_matches.len()];

    for java_match in java_matches {
        // Find closest rust match by rule_id + offset
        let best_rust = rust_matches.iter().enumerate()
            .filter(|(_, rm)| !rust_matched[rm_idx(rust_matches, rm)])
            .filter(|(_, rm)| rm.rule().id() == java_match.rule_id)
            .min_by_key(|(_, rm)| {
                (rm.offset() as i64 - java_match.offset as i64).abs() as usize
            });

        match best_rust {
            Some((idx, rm)) => {
                rust_matched[idx] = true;
                let mut match_ok = true;

                if rm.offset() != java_match.offset {
                    results.push(ComparisonResult {
                        rule_id: java_match.rule_id.clone(),
                        status: ComparisonStatus::OffsetMismatch,
                        details: format!(
                            "Java: offset={}, Rust: offset={}",
                            java_match.offset, rm.offset()
                        ),
                    });
                    match_ok = false;
                }

                if rm.length() != java_match.length {
                    results.push(ComparisonResult {
                        rule_id: java_match.rule_id.clone(),
                        status: ComparisonStatus::LengthMismatch,
                        details: format!(
                            "Java: len={}, Rust: len={}",
                            java_match.length, rm.length()
                        ),
                    });
                    match_ok = false;
                }

                let rust_replacements: Vec<&str> = rm.replacements().iter().map(|r| r.value()).collect();
                if rust_replacements != java_match.replacements {
                    results.push(ComparisonResult {
                        rule_id: java_match.rule_id.clone(),
                        status: ComparisonStatus::ReplacementMismatch,
                        details: format!(
                            "Java: {:?}, Rust: {:?}",
                            java_match.replacements, rust_replacements
                        ),
                    });
                    match_ok = false;
                }

                if match_ok {
                    results.push(ComparisonResult {
                        rule_id: java_match.rule_id.clone(),
                        status: ComparisonStatus::Pass,
                        details: "same matches".to_string(),
                    });
                }
            }
            None => {
                results.push(ComparisonResult {
                    rule_id: java_match.rule_id.clone(),
                    status: ComparisonStatus::MissingInRust,
                    details: format!(
                        "Java found '{}' at offset={}, Rust missed it",
                        java_match.rule_id, java_match.offset
                    ),
                });
            }
        }
    }

    // Check for extra matches in Rust
    for (idx, rust_match) in rust_matches.iter().enumerate() {
        if !rust_matched[idx] {
            results.push(ComparisonResult {
                rule_id: rust_match.rule().id().to_string(),
                status: ComparisonStatus::ExtraInRust,
                details: format!(
                    "Rust found '{}' at offset={}, not in Java",
                    rust_match.rule().id(),
                    rust_match.offset()
                ),
            });
        }
    }

    // If both are empty, that's a pass
    if java_matches.is_empty() && rust_matches.is_empty() {
        results.push(ComparisonResult {
            rule_id: "(none)".to_string(),
            status: ComparisonStatus::Pass,
            details: "no matches in either".to_string(),
        });
    }

    results
}

fn rm_idx(matches: &[RuleMatch], target: &RuleMatch) -> usize {
    // Get index by pointer comparison
    matches.iter().position(|m| std::ptr::eq(m, target)).unwrap_or(0)
}

/// Compare two JSON check results (for golden file comparison)
pub fn compare_json_results(rust_json: &str, java_json: &str) -> Vec<ComparisonResult> {
    let rust_result: serde_json::Value = match serde_json::from_str(rust_json) {
        Ok(v) => v,
        Err(e) => {
            return vec![ComparisonResult {
                rule_id: "PARSE_ERROR".to_string(),
                status: ComparisonStatus::MissingInRust,
                details: format!("Failed to parse Rust JSON: {}", e),
            }];
        }
    };

    let java_java_matches: Vec<JavaMatch> = match serde_json::from_str(java_json) {
        Ok(v) => v,
        Err(e) => {
            return vec![ComparisonResult {
                rule_id: "PARSE_ERROR".to_string(),
                status: ComparisonStatus::ExtraInRust,
                details: format!("Failed to parse Java JSON: {}", e),
            }];
        }
    };

    // Extract matches from Rust JSON
    let rust_matches_raw = match rust_result.get("matches") {
        Some(v) => v,
        None => {
            return vec![ComparisonResult {
                rule_id: "FORMAT_ERROR".to_string(),
                status: ComparisonStatus::MissingInRust,
                details: "Rust JSON missing 'matches' field".to_string(),
            }];
        }
    };

    // Convert to JavaMatch format for comparison
    let rust_as_java: Vec<JavaMatch> = rust_matches_raw.as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|m| {
            Some(JavaMatch {
                rule_id: m.get("rule")?.get("id")?.as_str()?.to_string(),
                message: m.get("message")?.as_str()?.to_string(),
                offset: m.get("offset")?.as_u64()? as usize,
                length: m.get("length")?.as_u64()? as usize,
                replacements: m.get("replacements")
                    .and_then(|r| r.as_array())
                    .map(|arr| arr.iter()
                        .filter_map(|v| v.get("value")?.as_str().map(String::from))
                        .collect())
                    .unwrap_or_default(),
                category: m.get("rule")?.get("category")?.get("id")?.as_str()?.to_string(),
                issue_type: String::new(),
            })
        })
        .collect();

    // Since we can't create RuleMatch from JSON easily, compare the two JavaMatch vecs directly
    compare_java_match_lists(&rust_as_java, &java_java_matches)
}

fn compare_java_match_lists(rust: &[JavaMatch], java: &[JavaMatch]) -> Vec<ComparisonResult> {
    let mut results = Vec::new();
    let mut rust_matched: Vec<bool> = vec![false; rust.len()];

    for java_match in java {
        let best = rust.iter().enumerate()
            .filter(|(i, _)| !rust_matched[*i])
            .filter(|(_, r)| r.rule_id == java_match.rule_id)
            .min_by_key(|(_, r)| (r.offset as i64 - java_match.offset as i64).abs() as usize);

        match best {
            Some((idx, rm)) => {
                rust_matched[idx] = true;
                let mut match_ok = true;

                if rm.offset != java_match.offset {
                    results.push(ComparisonResult {
                        rule_id: java_match.rule_id.clone(),
                        status: ComparisonStatus::OffsetMismatch,
                        details: format!("Java: offset={}, Rust: offset={}", java_match.offset, rm.offset),
                    });
                    match_ok = false;
                }

                if rm.length != java_match.length {
                    results.push(ComparisonResult {
                        rule_id: java_match.rule_id.clone(),
                        status: ComparisonStatus::LengthMismatch,
                        details: format!("Java: len={}, Rust: len={}", java_match.length, rm.length),
                    });
                    match_ok = false;
                }

                if rm.replacements != java_match.replacements {
                    results.push(ComparisonResult {
                        rule_id: java_match.rule_id.clone(),
                        status: ComparisonStatus::ReplacementMismatch,
                        details: format!("Java: {:?}, Rust: {:?}", java_match.replacements, rm.replacements),
                    });
                    match_ok = false;
                }

                if match_ok {
                    results.push(ComparisonResult {
                        rule_id: java_match.rule_id.clone(),
                        status: ComparisonStatus::Pass,
                        details: "same matches".to_string(),
                    });
                }
            }
            None => {
                results.push(ComparisonResult {
                    rule_id: java_match.rule_id.clone(),
                    status: ComparisonStatus::MissingInRust,
                    details: format!("Java found '{}' at offset={}, Rust missed it", java_match.rule_id, java_match.offset),
                });
            }
        }
    }

    for (idx, rm) in rust.iter().enumerate() {
        if !rust_matched[idx] {
            results.push(ComparisonResult {
                rule_id: rm.rule_id.clone(),
                status: ComparisonStatus::ExtraInRust,
                details: format!("Rust found '{}' at offset={}, not in Java", rm.rule_id, rm.offset),
            });
        }
    }

    if java.is_empty() && rust.is_empty() {
        results.push(ComparisonResult {
            rule_id: "(none)".to_string(),
            status: ComparisonStatus::Pass,
            details: "no matches in either".to_string(),
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_java_match(rule_id: &str, offset: usize, length: usize, replacements: Vec<&str>) -> JavaMatch {
        JavaMatch {
            rule_id: rule_id.to_string(),
            message: String::new(),
            offset,
            length,
            replacements: replacements.into_iter().map(String::from).collect(),
            category: "GRAMMAR".to_string(),
            issue_type: "grammar".to_string(),
        }
    }

    #[test]
    fn test_compare_empty() {
        let results = compare_results(&[], &[]);
        assert!(results.iter().all(|r| r.status == ComparisonStatus::Pass));
    }

    #[test]
    fn test_compare_java_match_list_empty() {
        let results = compare_java_match_lists(&[], &[]);
        assert!(results.iter().all(|r| r.status == ComparisonStatus::Pass));
    }

    #[test]
    fn test_compare_java_found_rust_missed() {
        let java = vec![make_java_match("RULE1", 5, 3, vec!["fix"])];
        let results = compare_java_match_lists(&[], &java);
        assert!(results.iter().any(|r| r.status == ComparisonStatus::MissingInRust));
    }

    #[test]
    fn test_compare_rust_found_extra() {
        let rust = vec![make_java_match("RULE1", 5, 3, vec!["fix"])];
        let results = compare_java_match_lists(&rust, &[]);
        assert!(results.iter().any(|r| r.status == ComparisonStatus::ExtraInRust));
    }

    #[test]
    fn test_compare_identical_matches() {
        let matches = vec![
            make_java_match("RULE1", 0, 5, vec!["hello"]),
            make_java_match("RULE2", 10, 3, vec!["fix"]),
        ];
        let results = compare_java_match_lists(&matches, &matches);
        assert!(results.iter().all(|r| r.status == ComparisonStatus::Pass));
    }

    #[test]
    fn test_compare_offset_mismatch() {
        let java = vec![make_java_match("RULE1", 5, 3, vec!["fix"])];
        let rust = vec![make_java_match("RULE1", 6, 3, vec!["fix"])];
        let results = compare_java_match_lists(&rust, &java);
        assert!(results.iter().any(|r| r.status == ComparisonStatus::OffsetMismatch));
    }

    #[test]
    fn test_compare_length_mismatch() {
        let java = vec![make_java_match("RULE1", 5, 3, vec!["fix"])];
        let rust = vec![make_java_match("RULE1", 5, 4, vec!["fix"])];
        let results = compare_java_match_lists(&rust, &java);
        assert!(results.iter().any(|r| r.status == ComparisonStatus::LengthMismatch));
    }

    #[test]
    fn test_compare_replacement_mismatch() {
        let java = vec![make_java_match("RULE1", 5, 3, vec!["fix1"])];
        let rust = vec![make_java_match("RULE1", 5, 3, vec!["fix2"])];
        let results = compare_java_match_lists(&rust, &java);
        assert!(results.iter().any(|r| r.status == ComparisonStatus::ReplacementMismatch));
    }

    #[test]
    fn test_compare_multiple_rules() {
        let java = vec![
            make_java_match("RULE1", 0, 5, vec!["hello"]),
            make_java_match("RULE2", 10, 3, vec!["fix"]),
            make_java_match("RULE3", 20, 4, vec!["word"]),
        ];
        let rust = vec![
            make_java_match("RULE1", 0, 5, vec!["hello"]),
            make_java_match("RULE3", 20, 4, vec!["word"]),
        ];
        let results = compare_java_match_lists(&rust, &java);
        assert!(results.iter().any(|r| r.status == ComparisonStatus::Pass && r.rule_id == "RULE1"));
        assert!(results.iter().any(|r| r.status == ComparisonStatus::MissingInRust && r.rule_id == "RULE2"));
    }
}
