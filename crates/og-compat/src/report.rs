use crate::compare::{ComparisonResult, ComparisonStatus};

pub struct CompatReport {
    pub results: Vec<ComparisonResult>,
}

impl CompatReport {
    pub fn new(results: Vec<ComparisonResult>) -> Self {
        Self { results }
    }

    pub fn pass_count(&self) -> usize {
        self.results.iter().filter(|r| r.status == ComparisonStatus::Pass).count()
    }

    pub fn fail_count(&self) -> usize {
        self.results.iter().filter(|r| r.status != ComparisonStatus::Pass).count()
    }

    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.status == ComparisonStatus::Pass)
    }

    pub fn format_report(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!("Compatibility Report: {} pass, {} fail\n", self.pass_count(), self.fail_count()));
        report.push_str(&"-".repeat(60).to_string());
        report.push('\n');

        for result in &self.results {
            let status_str = match result.status {
                ComparisonStatus::Pass => "PASS",
                ComparisonStatus::OffsetMismatch => "FAIL: offset mismatch",
                ComparisonStatus::ReplacementMismatch => "FAIL: replacement mismatch",
                ComparisonStatus::MessageMismatch => "FAIL: message mismatch",
                ComparisonStatus::MissingInRust => "FAIL: Java found, Rust missed",
                ComparisonStatus::ExtraInRust => "FAIL: Rust found extra",
                ComparisonStatus::CategoryMismatch => "FAIL: category mismatch",
                ComparisonStatus::LengthMismatch => "FAIL: length mismatch",
            };
            report.push_str(&format!("{} [{}] {}\n", status_str, result.rule_id, result.details));
        }

        report
    }
}
