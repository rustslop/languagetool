use std::sync::Arc;

use og_compat::{GoldenCase, GoldenMatch, GoldenRunner};

fn cases() -> Vec<GoldenCase> {
    vec![
        GoldenCase::new(
            "ccs-verb-guard-allows-match",
            "BOD meeting is next week and we all have to jump for it.",
        )
        .expect_with("COMMA_COMPOUND_SENTENCE", 24, 4, vec![", and"]),
        GoldenCase::new(
            "ccs-marker-includes-leading-whitespace",
            "He gave the picture to his daughter and she was so surprised that she forgot to say \"thank you\".",
        )
        .expect_with("COMMA_COMPOUND_SENTENCE", 35, 4, vec![", and"]),
        GoldenCase::new(
            "it-is-2-chunk-antipattern-suppresses",
            "Its more than 4,000 employees, located in 60 offices around the world, are ready.",
        ),
        GoldenCase::new(
            "prp-have-vb-single-token-np-carries-e-chunk",
            "He has uses a space before the colon.",
        )
        .expect("PRP_HAVE_VB", 6, 5)
        .presence_only_marker(),
        GoldenCase::new(
            "correct-node-js-extension-not-flagged",
            "The team maintains node.JS services for the API gateway.",
        ),
        GoldenCase::new(
            "correct-gov-abbreviation-name-not-passive",
            "Gov. Gray Davis signed the legislation in Sacramento last week.",
        ),
        GoldenCase::new(
            "correct-plain-sentence-no-matches",
            "The children walked slowly towards the old harbour before sunset.",
        ),
        GoldenCase::new(
            "contraction-typo-coud-t-flagged",
            "They coud't come to the party yesterday.",
        )
        .presence_only_expect("COUD_T"),
        GoldenCase::new(
            "correct-date-past-not-future-flagged",
            "Wir haben am 7.8.2010 den Kunden informiert.",
        ),
        GoldenCase::new(
            "unit-conversion-suggests-metric",
            "The trail is 5 miles long.",
        )
        .presence_only_expect("METRIC_UNITS_EN_GENERAL"),
        GoldenCase::new(
            "fp-guard-hippa-lowercase-adjacent",
            "The hippa compliant server stores patient data securely.",
        ),
        GoldenCase::new(
            "fp-guard-hippa-proper-name",
            "She works at the Hippa institute in Vienna.",
        ),
        GoldenCase::new(
            "fp-guard-gov-abbreviation-passive",
            "Gov. Gray Davis signed the legislation in Sacramento last week.",
        ),
        GoldenCase::new(
            "fp-guard-node-js-extension",
            "We use node.JS for the backend services.",
        ),
        GoldenCase::new(
            "fp-guard-node-js-prose",
            "This chapter covers node.JS deployment in production systems.",
        ),
        GoldenCase::new(
            "fp-guard-usa-abbrev",
            "The U.S.A. has fifty states.",
        ),
        GoldenCase::new(
            "fp-guard-dr-abbreviation",
            "Dr. Smith arrived early.",
        ),
        GoldenCase::new(
            "fp-guard-mr-abbreviation",
            "I saw Mr. Thompson at the store.",
        ),
        GoldenCase::new(
            "fp-guard-sen-abbreviation",
            "The result was announced by Sen. Kennedy yesterday afternoon.",
        ),
        GoldenCase::new(
            "fp-guard-third-street",
            "Their house is on the third street to the left of the bank.",
        ),
        GoldenCase::new(
            "fp-guard-committee-proposal",
            "It was a surprise to no one that the committee approved the proposal unanimously.",
        ),
        GoldenCase::new(
            "fp-guard-data-trend",
            "The data shows that the trend is continuing into the next quarter.",
        ),
        GoldenCase::new(
            "fp-guard-include-skipped-suggestion",
            "Please note that we will revert the change.",
        ),
        GoldenCase::new(
            "fp-guard-possessive-apostrophe-grant-relief",
            "If the indicators are revised upward, as most expect, that would grant some customers relief from the rate hike but could necessitate future ones to generate enough revenue for power purchases.",
        ),
        GoldenCase::new(
            "fp-guard-possessive-apostrophe-clearance",
            "Military officials must sometimes grant the passengers clearance to come ashore.",
        ),
        GoldenCase::new(
            "fp-guard-possessive-apostrophe-depicting",
            "The following is a graph depicting the effects ocean acidification has had on marine wildlife.",
        ),
        GoldenCase::new(
            "fp-guard-prp-have-vb-space-before-colon",
            "He has uses a space before the colon: like this.",
        ),
        GoldenCase::new(
            "typewriter-apostrophe-fires-on-curly-apostrophe",
            "An actress’s role",
        )
        .expect_with("TYPEWRITER_APOSTROPHE", 10, 4, vec!["'s"]),
        GoldenCase::new(
            "prp-vbg-optional-element-at-tail-bound",
            "What else she doing?",
        )
        .expect_with("PRP_VBG", 0, 13, vec!["What else is she", "What else she is"]),
    ]
}

#[test]
fn golden_parity_cases_pass() {
    let build_start = std::time::Instant::now();
    let runner = GoldenRunner::english();
    let build_elapsed = build_start.elapsed();
    let run_start = std::time::Instant::now();
    let report = runner.run(&cases());
    let run_elapsed = run_start.elapsed();
    eprintln!(
        "engine-build: {} ms ({} rules), golden-run: {} ms",
        build_elapsed.as_millis(),
        runner.rule_count(),
        run_elapsed.as_millis()
    );
    let failures: Vec<String> = report
        .results
        .iter()
        .filter(|r| r.status != og_compat::ComparisonStatus::Pass)
        .map(|r| format!("{} ({:?}): {}", r.rule_id, r.status, r.details))
        .collect();
    assert!(
        report.all_passed(),
        "{} of {} checks failed:\n{}",
        failures.len(),
        report.results.len(),
        failures.join("\n")
    );
}

#[test]
fn golden_runner_from_engine_enforces_message_fragments() {
    let runner = GoldenRunner::from_engine(Arc::new(og_langs::engine::LanguageEngine::english()));
    assert!(runner.rule_count() > 0);

    let mut present = GoldenCase::new(
        "typewriter-apostrophe-message-fragment",
        "An actress’s role",
    );
    present.matches.push(
        GoldenMatch::new("TYPEWRITER_APOSTROPHE", 10, 4)
            .with_replacements(vec!["'s"])
            .with_message("typewriter apostrophe"),
    );

    let mut absent = GoldenCase::new(
        "message-fragment-absent",
        "BOD meeting is next week and we all have to jump for it.",
    );
    absent.matches.push(
        GoldenMatch::new("COMMA_COMPOUND_SENTENCE", 24, 4)
            .with_replacements(vec![", and"])
            .with_message("ZZZ absent fragment ZZZ"),
    );

    let report = runner.run(&[present, absent]);
    assert!(
        report
            .results
            .iter()
            .any(|r| r.status == og_compat::ComparisonStatus::Pass)
    );
    let mismatch = report
        .results
        .iter()
        .find(|r| r.status == og_compat::ComparisonStatus::MessageMismatch)
        .expect("absent fragment reported as message mismatch");
    assert_eq!(mismatch.rule_id, "COMMA_COMPOUND_SENTENCE");
    assert_eq!(
        mismatch.details,
        format!(
            "expected message containing {:?}",
            "ZZZ absent fragment ZZZ"
        )
    );
    assert!(!report.all_passed());
}
