//! Statistics, the metrics line, peak RSS and argument parsing.

use std::fmt::Write as _;

/// Untimed runs before every median: they page data in and fill the lazy
/// caches (the speller's sorted word list, the tagger's per-word caches).
pub const WARMUPS: usize = 2;

/// Timed runs per median unless `--iters` overrides it.
pub const DEFAULT_ITERS: usize = 15;

pub const USAGE: &str = "\
usage: og-bench [--json] [--iters N]

Builds the English LanguageEngine, times the fixed corpora and a fixed list
of n-gram lookups, and prints exactly one JSON line on stdout:
  {\"build_ms\":N,\"clean_ms\":N,\"err_ms\":N,\"multi_ms\":N,\"ngram_lookup_us\":N,\"peak_rss_mb\":N}
Progress and diagnostics go to stderr.

  --json       JSON is the only output format; accepted for explicitness
  --iters N    timed runs per median (default 15, each after 2 warmups)
";

/// What the command line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Run { iters: usize },
    Help,
}

/// Parses the arguments after the program name.
pub fn parse_args<I, S>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iters = DEFAULT_ITERS;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let arg = arg.as_ref();
        let value = match arg {
            "--json" => continue,
            "-h" | "--help" => return Ok(Command::Help),
            "--iters" => args
                .next()
                .ok_or("--iters needs a value")?
                .as_ref()
                .to_owned(),
            _ => match arg.strip_prefix("--iters=") {
                Some(value) => value.to_owned(),
                None => return Err(format!("unknown argument {arg:?}")),
            },
        };
        iters = match value.parse::<usize>() {
            Ok(n) if n > 0 => n,
            _ => return Err(format!("--iters needs a positive integer, got {value:?}")),
        };
    }
    Ok(Command::Run { iters })
}

/// The median of `samples` (the mean of the middle two for an even count);
/// NaN when empty.
pub fn median(samples: &[f64]) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    match sorted.len() {
        0 => f64::NAN,
        len if len % 2 == 1 => sorted[mid],
        _ => (sorted[mid - 1] + sorted[mid]) / 2.0,
    }
}

/// One bench run's results.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Wall time of one `LanguageEngine::english()` call, the first build in
    /// the process.
    pub build_ms: f64,
    /// Median wall time of `check` on each corpus.
    pub clean_ms: f64,
    pub err_ms: f64,
    pub multi_ms: f64,
    /// Median latency of a single `get_count` over the fixed key list.
    pub ngram_lookup_us: f64,
    /// VmHWM at the end of the run, in MiB (kB / 1024).
    pub peak_rss_mb: f64,
}

impl Metrics {
    /// The metrics as one JSON object without a trailing newline. Values
    /// have three decimals; a value that is not finite becomes `null` so
    /// the line stays valid JSON.
    pub fn to_json_line(&self) -> String {
        let fields = [
            ("build_ms", self.build_ms),
            ("clean_ms", self.clean_ms),
            ("err_ms", self.err_ms),
            ("multi_ms", self.multi_ms),
            ("ngram_lookup_us", self.ngram_lookup_us),
            ("peak_rss_mb", self.peak_rss_mb),
        ];
        let mut line = String::from("{");
        for (index, (key, value)) in fields.into_iter().enumerate() {
            if index > 0 {
                line.push(',');
            }
            let _ = write!(line, "\"{key}\":");
            if value.is_finite() {
                let _ = write!(line, "{value:.3}");
            } else {
                line.push_str("null");
            }
        }
        line.push('}');
        line
    }
}

/// The `VmHWM` (peak resident set) value of a `/proc/<pid>/status` text, in kB.
pub fn vm_hwm_kib(status: &str) -> Option<u64> {
    let rest = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?;
    rest.trim().strip_suffix("kB")?.trim().parse().ok()
}

/// This process's peak resident set so far, in MiB, from `/proc/self/status`.
pub fn peak_rss_mb() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    vm_hwm_kib(&status).map(|kib| kib as f64 / 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_default_to_fifteen_iterations() {
        let none: [&str; 0] = [];
        assert_eq!(parse_args(none), Ok(Command::Run { iters: 15 }));
        assert_eq!(parse_args(["--json"]), Ok(Command::Run { iters: 15 }));
    }

    #[test]
    fn args_accept_an_iters_override_in_both_spellings() {
        assert_eq!(
            parse_args(["--json", "--iters", "5"]),
            Ok(Command::Run { iters: 5 })
        );
        assert_eq!(parse_args(["--iters=7"]), Ok(Command::Run { iters: 7 }));
        assert_eq!(parse_args(["--help"]), Ok(Command::Help));
    }

    #[test]
    fn args_reject_bad_values_and_unknown_flags() {
        assert!(parse_args(["--iters"]).is_err());
        assert!(parse_args(["--iters", "0"]).is_err());
        assert!(parse_args(["--iters", "x"]).is_err());
        assert!(parse_args(["--iters=-3"]).is_err());
        assert!(parse_args(["--fast"]).is_err());
    }

    #[test]
    fn median_takes_the_middle_or_the_mean_of_the_middle_two() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
        assert_eq!(median(&[7.5]), 7.5);
        assert!(median(&[]).is_nan());
    }

    #[test]
    fn metrics_line_has_the_specified_keys_in_order() {
        let metrics = Metrics {
            build_ms: 5123.4567,
            clean_ms: 12.0,
            err_ms: 0.5,
            multi_ms: 100.25,
            ngram_lookup_us: 42.0001,
            peak_rss_mb: 812.0,
        };
        assert_eq!(
            metrics.to_json_line(),
            "{\"build_ms\":5123.457,\"clean_ms\":12.000,\"err_ms\":0.500,\"multi_ms\":100.250,\
             \"ngram_lookup_us\":42.000,\"peak_rss_mb\":812.000}"
        );
        let broken = Metrics {
            peak_rss_mb: f64::NAN,
            ngram_lookup_us: f64::INFINITY,
            ..metrics
        };
        assert!(
            broken
                .to_json_line()
                .ends_with("\"ngram_lookup_us\":null,\"peak_rss_mb\":null}")
        );
    }

    #[test]
    fn vm_hwm_is_parsed_from_a_status_text() {
        let status = "Name:\tog-bench\nVmPeak:\t 2000 kB\nVmHWM:\t  830464 kB\nVmRSS:\t 1 kB\n";
        assert_eq!(vm_hwm_kib(status), Some(830_464));
        assert_eq!(vm_hwm_kib("Name:\tx\n"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn peak_rss_of_this_process_is_readable() {
        let mb = peak_rss_mb().expect("VmHWM in /proc/self/status");
        assert!(mb > 0.0, "{mb}");
    }
}
