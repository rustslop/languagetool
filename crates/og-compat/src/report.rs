use crate::compare::{ComparisonResult, ComparisonStatus};

pub struct CompatReport {
    pub results: Vec<ComparisonResult>,
}

impl CompatReport {
    pub fn new(results: Vec<ComparisonResult>) -> Self {
        Self { results }
    }

    pub fn pass_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.status == ComparisonStatus::Pass)
            .count()
    }

    pub fn fail_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.status != ComparisonStatus::Pass)
            .count()
    }

    pub fn all_passed(&self) -> bool {
        self.results
            .iter()
            .all(|r| r.status == ComparisonStatus::Pass)
    }

    pub fn format_report(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!(
            "Compatibility Report: {} pass, {} fail\n",
            self.pass_count(),
            self.fail_count()
        ));
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
            report.push_str(&format!(
                "{} [{}] {}\n",
                status_str, result.rule_id, result.details
            ));
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(rule_id: &str, status: ComparisonStatus, details: &str) -> ComparisonResult {
        ComparisonResult {
            rule_id: rule_id.to_string(),
            status,
            details: details.to_string(),
        }
    }

    #[test]
    fn counts_split_passes_from_failures() {
        let report = CompatReport::new(vec![
            result("R1", ComparisonStatus::Pass, "ok"),
            result("R2", ComparisonStatus::OffsetMismatch, "off"),
            result("R3", ComparisonStatus::Pass, "ok"),
        ]);
        assert_eq!(report.pass_count(), 2);
        assert_eq!(report.fail_count(), 1);
        assert!(!report.all_passed());
    }

    #[test]
    fn all_passed_accepts_only_passes() {
        assert!(CompatReport::new(vec![]).all_passed());
        assert!(CompatReport::new(vec![result("R1", ComparisonStatus::Pass, "ok")]).all_passed());
        assert!(
            !CompatReport::new(vec![result("R1", ComparisonStatus::MessageMismatch, "msg")])
                .all_passed()
        );
    }

    #[test]
    fn empty_report_formats_header_and_rule_line_only() {
        let text = CompatReport::new(vec![]).format_report();
        assert_eq!(
            text,
            format!("Compatibility Report: 0 pass, 0 fail\n{}\n", "-".repeat(60))
        );
    }

    #[test]
    fn format_report_labels_every_status_arm() {
        let report = CompatReport::new(vec![
            result("A", ComparisonStatus::Pass, "d0"),
            result("B", ComparisonStatus::OffsetMismatch, "d1"),
            result("C", ComparisonStatus::ReplacementMismatch, "d2"),
            result("D", ComparisonStatus::MessageMismatch, "d3"),
            result("E", ComparisonStatus::MissingInRust, "d4"),
            result("F", ComparisonStatus::ExtraInRust, "d5"),
            result("G", ComparisonStatus::CategoryMismatch, "d6"),
            result("H", ComparisonStatus::LengthMismatch, "d7"),
        ]);
        let text = report.format_report();
        assert_eq!(
            text,
            concat!(
                "Compatibility Report: 1 pass, 7 fail\n",
                "------------------------------------------------------------\n",
                "PASS [A] d0\n",
                "FAIL: offset mismatch [B] d1\n",
                "FAIL: replacement mismatch [C] d2\n",
                "FAIL: message mismatch [D] d3\n",
                "FAIL: Java found, Rust missed [E] d4\n",
                "FAIL: Rust found extra [F] d5\n",
                "FAIL: category mismatch [G] d6\n",
                "FAIL: length mismatch [H] d7\n",
            )
        );
    }
}
