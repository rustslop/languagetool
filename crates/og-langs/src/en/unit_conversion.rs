use og_core::{
    AnalyzedSentence, Category, IssueType, Rule, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};

const MAX_SUGGESTIONS: usize = 5;
const ROUNDING_DELTA: f64 = 0.05;
const DELTA: f64 = 1e-2;
const NUMBER_REGEX: &str = r"(-?\b[0-9]{1,32}[0-9,.]{0,32})";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dim {
    Length,
    Mass,
    Volume,
    Temperature,
    Speed,
    Area,
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Unit {
    dim: Dim,
    factor: f64,
    offset: f64,
}

impl Unit {
    const fn new(dim: Dim, factor: f64) -> Self {
        Self {
            dim,
            factor,
            offset: 0.0,
        }
    }

    const fn with_offset(dim: Dim, factor: f64, offset: f64) -> Self {
        Self {
            dim,
            factor,
            offset,
        }
    }

    fn scale_to_base(self, value: f64) -> f64 {
        value * self.factor + self.offset
    }

    fn scale_from_base(self, value: f64) -> f64 {
        (value - self.offset) / self.factor
    }
}

const KG: Unit = Unit::new(Dim::Mass, 1.0);
const M: Unit = Unit::new(Dim::Length, 1.0);
const M2: Unit = Unit::new(Dim::Area, 1.0);
const M3: Unit = Unit::new(Dim::Volume, 1.0);
const CELSIUS: Unit = Unit::new(Dim::Temperature, 1.0);
const POUND: Unit = Unit::new(Dim::Mass, 0.45359237);
const OUNCE: Unit = Unit::new(Dim::Mass, 0.45359237 / 12.0);
const FEET: Unit = Unit::new(Dim::Length, 0.3048);
const YARD: Unit = Unit::new(Dim::Length, 0.3048 * 3.0);
const INCH: Unit = Unit::new(Dim::Length, 0.3048 / 12.0);
const MILE: Unit = Unit::new(Dim::Length, 0.3048 * 5280.0);
const FAHRENHEIT: Unit = Unit::with_offset(Dim::Temperature, 5.0 / 9.0, -160.0 / 9.0);
const LITRE: Unit = Unit::new(Dim::Volume, 1e-3);
const KM_PER_H: Unit = Unit::new(Dim::Speed, 1.0 / 3.6);

struct UnitEntry {
    pattern: String,
    unit: Unit,
    symbol: &'static str,
    metric: bool,
}

fn unit_entries() -> Vec<UnitEntry> {
    let mut v: Vec<UnitEntry> = vec![
        entry("kg", KG, "kg", true),
        entry("g", Unit::new(Dim::Mass, 1e-3), "g", true),
        entry("t", Unit::new(Dim::Mass, 1e3), "t", true),
        entry("lb", POUND, "lb", false),
        entry("mi", MILE, "mi", false),
        entry("yd", YARD, "yd", false),
        entry("(?:ft|′|')(?!(\\w|\\d))", FEET, "ft", false),
        entry("(?:inch|″)(?!(\\w|\\d))", INCH, "inch", false),
        entry("(?:km/h|kmh)", KM_PER_H, "km/h", true),
        entry("mph", Unit::new(Dim::Speed, 0.44704), "mph", false),
        entry("km", Unit::new(Dim::Length, 1e3), "km", true),
        entry("m", M, "m", true),
        entry("cm", Unit::new(Dim::Length, 1e-2), "cm", true),
        entry("mm", Unit::new(Dim::Length, 1e-3), "mm", true),
        entry("µm", Unit::new(Dim::Length, 1e-6), "µm", true),
        entry("nm", Unit::new(Dim::Length, 1e-9), "nm", true),
        entry("m(?:\\^2|2|²)", M2, "m²", true),
        entry("ha", Unit::new(Dim::Area, 1e4), "ha", true),
        entry("a", Unit::new(Dim::Area, 1e2), "a", true),
        entry("km(?:\\^2|2|²)", Unit::new(Dim::Area, 1e6), "km²", true),
        entry("cm(?:\\^2|2|²)", Unit::new(Dim::Area, 1e-4), "cm²", true),
        entry("mm(?:\\^2|2|²)", Unit::new(Dim::Area, 1e-6), "mm²", true),
        entry("m(?:\\^3|3|³)", M3, "m³", true),
        entry("cm(?:\\^3|3|³)", Unit::new(Dim::Volume, 1e-6), "cm³", true),
        entry("mm(?:\\^3|3|³)", Unit::new(Dim::Volume, 1e-9), "mm³", true),
        entry(
            "(?:cubic|cu) (?:feet|ft|foot)",
            Unit::new(Dim::Volume, 0.3048f64.powi(3)),
            "cubic feet",
            false,
        ),
        entry(
            "(?:feet|ft|foot)(?:\\^3|3|³)",
            Unit::new(Dim::Volume, 0.3048f64.powi(3)),
            "ft³",
            false,
        ),
        entry(
            "(?:cubic|cu) (?:inch|in|inches)",
            Unit::new(Dim::Volume, (0.3048 / 12.0f64).powi(3)),
            "cubic inch",
            false,
        ),
        entry(
            "(?:inch|in)(?:\\^3|3|³)",
            Unit::new(Dim::Volume, (0.3048 / 12.0f64).powi(3)),
            "inch³",
            false,
        ),
        entry(
            "(?:cubic|cu) (?:yards?|yds?)",
            Unit::new(Dim::Volume, (0.3048 * 3.0f64).powi(3)),
            "cubic yard",
            false,
        ),
        entry(
            "(?:yard|yd)(?:\\^3|3|³)",
            Unit::new(Dim::Volume, (0.3048 * 3.0f64).powi(3)),
            "yard³",
            false,
        ),
        entry("l", LITRE, "l", true),
        entry("ml", Unit::new(Dim::Volume, 1e-6), "ml", true),
        entry("°F", FAHRENHEIT, "°F", false),
        entry("°C", CELSIUS, "°C", true),
        entry(
            "miles per hour",
            Unit::new(Dim::Speed, 0.44704),
            "miles per hour",
            false,
        ),
        entry("kilograms?", KG, "kilogram", true),
        entry("grams?", Unit::new(Dim::Mass, 1e-3), "gram", true),
        entry("tons?", Unit::new(Dim::Mass, 1e3), "ton", true),
        entry("pounds?", POUND, "pounds", false),
        entry("ounces?", OUNCE, "ounces", false),
        entry("feet", FEET, "feet", false),
        entry("miles?", MILE, "miles", false),
        entry("yards?", YARD, "yards", false),
        entry("inch(es)?", INCH, "inches", false),
        entry(
            "(?:degrees?)? Fahrenheit",
            FAHRENHEIT,
            "degree Fahrenheit",
            false,
        ),
        entry("(?:degrees?)? Celsius", CELSIUS, "degree Celsius", true),
    ];
    v.retain(|e| !e.metric || !matches!(e.symbol, "a"));
    v
}

fn entry(pattern: &str, unit: Unit, symbol: &'static str, metric: bool) -> UnitEntry {
    UnitEntry {
        pattern: pattern.to_string(),
        unit,
        symbol,
        metric,
    }
}

pub struct UnitConversionRule {
    compiled: Vec<(regex::Regex, Unit, &'static str, bool)>,
    converted_re: regex::Regex,
    anti_res: Vec<regex::Regex>,
}

impl UnitConversionRule {
    pub fn new() -> Self {
        let compiled = unit_entries()
            .into_iter()
            .filter_map(|e| {
                let full = format!("{NUMBER_REGEX}[\\s\\u{{00A0}}]{{0,5}}{}\\b", e.pattern);
                regex::Regex::new(&full)
                    .ok()
                    .map(|re| (re, e.unit, e.symbol, e.metric))
            })
            .collect();
        let converted_re =
            regex::Regex::new(&format!(r"\s*\((?:ca. )?{NUMBER_REGEX}\s*([^)]+)\s*\)")).unwrap();
        let anti_res = [
            r"\s?\d+'\d\d\d\s?",
            r"\d+[-‐–]\d+",
            r"\d+/\d+",
            r"\d+:\d+",
            r"\d+⁄\d+",
        ]
        .iter()
        .filter_map(|p| regex::Regex::new(p).ok())
        .collect();
        Self {
            compiled,
            converted_re,
            anti_res,
        }
    }

    fn metric_equivalents(&self, value: f64, unit: Unit) -> Vec<(Unit, f64, &'static str)> {
        if self
            .compiled
            .iter()
            .any(|(_, candidate, _, metric)| *metric && *candidate == unit)
        {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (_, candidate, symbol, metric) in &self.compiled {
            if !metric || *candidate == unit || candidate.dim != unit.dim {
                continue;
            }
            let converted = candidate.scale_from_base(unit.scale_to_base(value));
            out.push((*candidate, converted, *symbol));
            if out.len() > 16 {
                break;
            }
        }
        out.sort_by(|a, b| {
            naturalness(a.1)
                .partial_cmp(&naturalness(b.1))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.dedup_by(|a, b| a.0 == b.0 && a.2 == b.2);
        out
    }

    fn format_measurement(&self, value: f64, unit: Unit) -> Vec<String> {
        let mut formatted: Vec<String> = Vec::new();
        for (_, converted, symbol) in self.metric_equivalents(value, unit) {
            let rounded = converted.round();
            if formatted.len() > MAX_SUGGESTIONS {
                break;
            }
            if (converted - rounded).abs() / converted.abs() < ROUNDING_DELTA && rounded != 0.0 {
                let s = format!("ca. {} {}", format_number(rounded), symbol);
                if !formatted.contains(&s) {
                    formatted.push(s);
                }
            }
            let formatted_number = format_number(converted);
            let s = format!("{formatted_number} {symbol}");
            if !formatted.contains(&s) && formatted_number != "0" {
                formatted.push(s);
            }
        }
        formatted
    }
}

fn char_boundary_after(text: &str, mut pos: usize) -> usize {
    while pos < text.len() && !text.is_char_boundary(pos) {
        pos += 1;
    }
    pos
}

fn char_boundary_at(text: &str, mut pos: usize) -> usize {
    while pos > 0 && !text.is_char_boundary(pos) {
        pos += 1;
        if pos >= text.len() {
            return text.len();
        }
    }
    pos
}

fn naturalness(number: f64) -> f64 {
    let abs = number.abs();
    if abs < 1.0 {
        1.0 / (abs * abs * 2.0)
    } else if abs > 1000.0 {
        abs / 1000.0
    } else {
        abs
    }
}

fn format_number(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    let mut s = format!("{rounded:.2}");
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    s
}

fn parse_number(raw: &str) -> Option<f64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if cleaned.chars().filter(|c| *c == '.').count() > 1 {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

impl Rule for UnitConversionRule {
    fn id(&self) -> &str {
        "METRIC_UNITS_EN_GENERAL"
    }

    fn is_picky(&self) -> bool {
        true
    }

    fn description(&self) -> &str {
        "Suggests or checks conversion of units to their metric equivalents."
    }

    fn category(&self) -> Category {
        Category::new("STYLE", "Style")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let text = sentence.text();
        let mut matches = Vec::new();
        let mut ignore_ranges: Vec<(usize, usize)> = Vec::new();

        let mut ordered: Vec<&(regex::Regex, Unit, &'static str, bool)> =
            self.compiled.iter().collect();
        ordered.sort_by_key(|(_, _, _, metric)| !metric);

        for (re, unit, _, _) in &ordered {
            for mat in re.find_iter(text) {
                let ignored = ignore_ranges
                    .iter()
                    .any(|(s, e)| mat.start() >= *s && mat.end() <= *e);
                if ignored {
                    continue;
                }
                let anti = self.anti_res.iter().any(|anti| anti.is_match(mat.as_str()));
                if anti {
                    continue;
                }
                ignore_ranges.push((mat.start(), mat.end()));

                let number_text = re
                    .captures(mat.as_str())
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let mut number_text = number_text.as_str();
                if number_text.starts_with('-')
                    && regex::Regex::new(&format!("{NUMBER_REGEX}$"))
                        .ok()
                        .is_some_and(|re| re.is_match(&text[..mat.start()]))
                {
                    number_text = &number_text[1..];
                }
                let Some(value) = parse_number(number_text) else {
                    continue;
                };

                let converted_in_text = self
                    .converted_re
                    .find(&text[mat.end()..])
                    .filter(|m| m.start() == 0);
                let converted = self.format_measurement(value, *unit);

                match converted_in_text {
                    None => {
                        let Some(converted) = (!converted.is_empty()).then_some(converted) else {
                            continue;
                        };
                        let suggestions: Vec<SuggestedReplacement> = converted
                            .iter()
                            .map(|c| SuggestedReplacement::new(format!("{} ({c})", mat.as_str())))
                            .collect();
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_picky(true)
                            .with_category(self.category())
                            .with_issue_type("style");
                        let ctx_start = char_boundary_after(text, mat.start().saturating_sub(40));
                        let ctx_end = char_boundary_at(text, (mat.end() + 40).min(text.len()));
                        matches.push(
                            RuleMatch::new(
                                "Writing for an international audience? Consider adding the metric equivalent.",
                                mat.start(),
                                mat.end() - mat.start(),
                                rm_rule,
                                RuleMatchContext::new(
                                    text[ctx_start..ctx_end].to_string(),
                                    ctx_start,
                                    ctx_end - ctx_start,
                                ),
                            )
                            .with_replacements(suggestions)
                            .with_short_message("Add metric equivalent?"),
                        );
                    }
                    Some(converted_mat) => {
                        let abs_start = mat.end() + converted_mat.start();
                        let abs_end = mat.end() + converted_mat.end();
                        ignore_ranges.push((abs_start, abs_end));
                        let inner = converted_mat
                            .as_str()
                            .trim()
                            .trim_start_matches('(')
                            .trim_end_matches(')')
                            .to_string();
                        if converted.contains(&inner) {
                            continue;
                        }
                        let number_in = self
                            .converted_re
                            .captures(converted_mat.as_str())
                            .and_then(|c| c.get(1))
                            .and_then(|m| parse_number(m.as_str()));
                        let Some(value_in) = number_in else {
                            continue;
                        };
                        if let Some((_, computed, _)) =
                            self.metric_equivalents(value, *unit).first().copied()
                            && (value_in - computed).abs() >= DELTA
                        {
                            let suggestions: Vec<SuggestedReplacement> = converted
                                .iter()
                                .map(|c| {
                                    SuggestedReplacement::new(format!("{} ({c})", mat.as_str()))
                                })
                                .collect();
                            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                                .with_picky(true)
                                .with_category(self.category())
                                .with_issue_type("style");
                            let ctx_start =
                                char_boundary_after(text, mat.start().saturating_sub(40));
                            let ctx_end = char_boundary_at(text, (abs_end + 40).min(text.len()));
                            matches.push(
                                    RuleMatch::new(
                                        "This unit conversion doesn't seem right. Do you want to correct it automatically?",
                                        mat.start(),
                                        abs_end - mat.start(),
                                        rm_rule,
                                        RuleMatchContext::new(
                                            text[ctx_start..ctx_end].to_string(),
                                            ctx_start,
                                            ctx_end - ctx_start,
                                        ),
                                    )
                                    .with_replacements(suggestions)
                                    .with_short_message("Incorrect unit conversion. Correct it?"),
                                );
                        }
                    }
                }
            }
        }
        matches
    }
}

impl Default for UnitConversionRule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence(text: &str) -> AnalyzedSentence {
        AnalyzedSentence::new(text, 0, text.len())
    }

    #[test]
    fn suggests_metric_equivalent_for_miles() {
        let rule = UnitConversionRule::new();
        let matches = rule.match_sentence(&sentence("The trail is 5 miles long."));
        assert_eq!(matches.len(), 1);
        let replacements: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(
            replacements.iter().any(|r| r.starts_with("5 miles (")),
            "{replacements:?}"
        );
    }

    #[test]
    fn accepts_correct_conversion_in_text() {
        let rule = UnitConversionRule::new();
        let matches = rule.match_sentence(&sentence("The trail is 5 miles (8.05 km) long."));
        assert!(matches.is_empty(), "{matches:?}");
    }

    #[test]
    fn flags_wrong_conversion() {
        let rule = UnitConversionRule::new();
        let matches = rule.match_sentence(&sentence("The trail is 5 miles (3 km) long."));
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn metric_units_are_not_flagged() {
        let rule = UnitConversionRule::new();
        assert!(
            rule.match_sentence(&sentence("The trail is 8 km long."))
                .is_empty()
        );
    }

    #[test]
    fn fahrenheit_converts_to_celsius() {
        let rule = UnitConversionRule::new();
        let matches = rule.match_sentence(&sentence("It was 100 Fahrenheit outside."));
        assert_eq!(matches.len(), 1);
        let replacements: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(
            replacements.iter().any(|r| r.contains("37.78")),
            "{replacements:?}"
        );
    }
}
