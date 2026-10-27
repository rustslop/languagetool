use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
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
const FEET: Unit = Unit::new(Dim::Length, 0.3048);
const YARD: Unit = Unit::new(Dim::Length, 0.3048 * 3.0);
const INCH: Unit = Unit::new(Dim::Length, 0.0254);
const MILE: Unit = Unit::new(Dim::Length, 1609.344);
const FAHRENHEIT: Unit = Unit::with_offset(Dim::Temperature, 5.0 / 9.0, -32.0 * 5.0 / 9.0);
const LITRE: Unit = Unit::new(Dim::Volume, 1e-3);
const KM_PER_H: Unit = Unit::new(Dim::Speed, 1.0 / 3.6);
const MPH: Unit = Unit::new(Dim::Speed, 0.44704);

struct UnitEntry {
    pattern: &'static str,
    unit: Unit,
    symbol: &'static str,
    metric: bool,
}

fn unit_entries() -> Vec<UnitEntry> {
    vec![
        UnitEntry {
            pattern: "Kilo(gramm)?",
            unit: KG,
            symbol: "Kilogramm",
            metric: true,
        },
        UnitEntry {
            pattern: "Gramm",
            unit: Unit::new(Dim::Mass, 1e-3),
            symbol: "Gramm",
            metric: true,
        },
        UnitEntry {
            pattern: "Tonnen?",
            unit: Unit::new(Dim::Mass, 1e3),
            symbol: "Tonnen",
            metric: true,
        },
        UnitEntry {
            pattern: "Pfund",
            unit: POUND,
            symbol: "Pfund",
            metric: false,
        },
        UnitEntry {
            pattern: "Meilen?",
            unit: MILE,
            symbol: "Meile",
            metric: false,
        },
        UnitEntry {
            pattern: "Yard",
            unit: YARD,
            symbol: "Yard",
            metric: false,
        },
        UnitEntry {
            pattern: "Fuß",
            unit: FEET,
            symbol: "Fuß",
            metric: false,
        },
        UnitEntry {
            pattern: "Zoll",
            unit: INCH,
            symbol: "Zoll",
            metric: false,
        },
        UnitEntry {
            pattern: "(Kilometer pro Stunde|Stundenkilometer)",
            unit: KM_PER_H,
            symbol: "Kilometer pro Stunde",
            metric: true,
        },
        UnitEntry {
            pattern: "Meilen pro Stunde",
            unit: MPH,
            symbol: "Meilen pro Stunde",
            metric: false,
        },
        UnitEntry {
            pattern: "Meter",
            unit: M,
            symbol: "Meter",
            metric: true,
        },
        UnitEntry {
            pattern: "Kilometer",
            unit: Unit::new(Dim::Length, 1e3),
            symbol: "Kilometer",
            metric: true,
        },
        UnitEntry {
            pattern: "Zentimeter",
            unit: Unit::new(Dim::Length, 1e-2),
            symbol: "Zentimeter",
            metric: true,
        },
        UnitEntry {
            pattern: "Millimeter",
            unit: Unit::new(Dim::Length, 1e-3),
            symbol: "Millimeter",
            metric: true,
        },
        UnitEntry {
            pattern: "Mikrometer",
            unit: Unit::new(Dim::Length, 1e-6),
            symbol: "Mikrometer",
            metric: true,
        },
        UnitEntry {
            pattern: "Nanometer",
            unit: Unit::new(Dim::Length, 1e-9),
            symbol: "Nanometer",
            metric: true,
        },
        UnitEntry {
            pattern: "Pikometer",
            unit: Unit::new(Dim::Length, 1e-12),
            symbol: "Pikometer",
            metric: true,
        },
        UnitEntry {
            pattern: "Femtometer",
            unit: Unit::new(Dim::Length, 1e-15),
            symbol: "Femtometer",
            metric: true,
        },
        UnitEntry {
            pattern: "Quadratmeter",
            unit: M2,
            symbol: "Quadratmeter",
            metric: true,
        },
        UnitEntry {
            pattern: "Hektar",
            unit: Unit::new(Dim::Area, 1e4),
            symbol: "Hektar",
            metric: true,
        },
        UnitEntry {
            pattern: "Ar\\b",
            unit: Unit::new(Dim::Area, 1e2),
            symbol: "Ar",
            metric: true,
        },
        UnitEntry {
            pattern: "Quadratkilometer",
            unit: Unit::new(Dim::Area, 1e6),
            symbol: "Quadratkilometer",
            metric: true,
        },
        UnitEntry {
            pattern: "Quadratzentimeter",
            unit: Unit::new(Dim::Area, 1e-4),
            symbol: "Quadratzentimeter",
            metric: true,
        },
        UnitEntry {
            pattern: "Quadratmillimeter",
            unit: Unit::new(Dim::Area, 1e-6),
            symbol: "Quadratmillimeter",
            metric: true,
        },
        UnitEntry {
            pattern: "Quadratmikrometer",
            unit: Unit::new(Dim::Area, 1e-12),
            symbol: "Quadratmikrometer",
            metric: true,
        },
        UnitEntry {
            pattern: "Quadratnanometer",
            unit: Unit::new(Dim::Area, 1e-18),
            symbol: "Quadratnanometer",
            metric: true,
        },
        UnitEntry {
            pattern: "Kubikmeter",
            unit: M3,
            symbol: "Kubikmeter",
            metric: true,
        },
        UnitEntry {
            pattern: "Kubikkilometer",
            unit: Unit::new(Dim::Volume, 1e9),
            symbol: "Kubikkilometer",
            metric: true,
        },
        UnitEntry {
            pattern: "Kubikzentimeter",
            unit: Unit::new(Dim::Volume, 1e-6),
            symbol: "Kubikzentimeter",
            metric: true,
        },
        UnitEntry {
            pattern: "Kubikmillimeter",
            unit: Unit::new(Dim::Volume, 1e-9),
            symbol: "Kubikmillimeter",
            metric: true,
        },
        UnitEntry {
            pattern: "Kubikmikrometer",
            unit: Unit::new(Dim::Volume, 1e-18),
            symbol: "Kubikmikrometer",
            metric: true,
        },
        UnitEntry {
            pattern: "Kubiknanometer",
            unit: Unit::new(Dim::Volume, 1e-27),
            symbol: "Kubiknanometer",
            metric: true,
        },
        UnitEntry {
            pattern: "Liter",
            unit: LITRE,
            symbol: "Liter",
            metric: true,
        },
        UnitEntry {
            pattern: "Milliliter",
            unit: Unit::new(Dim::Volume, 1e-6),
            symbol: "Milliliter",
            metric: true,
        },
        UnitEntry {
            pattern: "(?:Grad)? Fahrenheit",
            unit: FAHRENHEIT,
            symbol: "Grad Fahrenheit",
            metric: false,
        },
        UnitEntry {
            pattern: "(?:Grad)? Celsius",
            unit: CELSIUS,
            symbol: "Grad Celsius",
            metric: true,
        },
    ]
}

pub struct GermanUnitConversionRule {
    compiled: Vec<(regex::Regex, Unit, &'static str, bool)>,
    converted_re: regex::Regex,
    anti_res: Vec<regex::Regex>,
}

impl GermanUnitConversionRule {
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
            r"Pfund Sterling",
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

impl Default for GermanUnitConversionRule {
    fn default() -> Self {
        Self::new()
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
    let s = format!("{rounded:.2}").replace('.', ",");
    let s = s.trim_end_matches('0').trim_end_matches(',');
    s.to_string()
}

fn parse_number(raw: &str) -> Option<f64> {
    let mut cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();
    if cleaned.chars().filter(|c| *c == ',').count() > 1 {
        cleaned = cleaned.chars().filter(|c| *c != '.').collect();
        cleaned = cleaned.replace(',', ".");
    } else if cleaned.contains(',') {
        cleaned = cleaned.replace('.', "");
        cleaned = cleaned.replace(',', ".");
    }
    cleaned.parse::<f64>().ok()
}

impl Rule for GermanUnitConversionRule {
    fn id(&self) -> &str {
        "EINHEITEN_METRISCH"
    }

    fn description(&self) -> &str {
        "Schlägt vor oder überprüft Angaben des metrischen Äquivalentes bei bestimmten Maßeinheiten."
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("STYLE", "Stil")
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
                            .with_category(self.category())
                            .with_issue_type("style");
                        let ctx_start = char_boundary_after(text, mat.start().saturating_sub(40));
                        let ctx_end = char_boundary_at(text, (mat.end() + 40).min(text.len()));
                        matches.push(
                            RuleMatch::new(
                                "Wollen Sie eine Umwandlung ins metrische System automatisch hinzufügen?",
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
                            .with_short_message("Metrisches Äquivalent hinzufügen?"),
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
                                .with_category(self.category())
                                .with_issue_type("style");
                            let ctx_start =
                                char_boundary_after(text, mat.start().saturating_sub(40));
                            let ctx_end = char_boundary_at(text, (abs_end + 40).min(text.len()));
                            matches.push(
                                    RuleMatch::new(
                                        "Diese Umrechnung scheint falsch zu sein. Wollen Sie sie automatisch korrigieren lassen?",
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
                                    .with_short_message("Falsche Umrechnung. Automatisch korrigieren?"),
                                );
                        }
                    }
                }
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence(text: &str) -> AnalyzedSentence {
        AnalyzedSentence::new(text, 0, text.len())
    }

    #[test]
    fn suggests_metric_equivalent_for_fuss() {
        let rule = GermanUnitConversionRule::new();
        let matches = rule.match_sentence(&sentence("Ich bin 6 Fuß groß."));
        assert_eq!(matches.len(), 1, "{matches:?}");
        let replacements: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert!(
            replacements.iter().any(|r| r.starts_with("6 Fuß (1,83")),
            "{replacements:?}"
        );
    }

    #[test]
    fn accepts_correct_conversion() {
        let rule = GermanUnitConversionRule::new();
        let matches = rule.match_sentence(&sentence("Ich bin 6 Fuß (1,83 m) groß."));
        assert!(matches.is_empty(), "{matches:?}");
    }

    #[test]
    fn flags_wrong_conversion() {
        let rule = GermanUnitConversionRule::new();
        let matches = rule.match_sentence(&sentence("Ich bin 6 Fuß (3 m) groß."));
        assert_eq!(matches.len(), 1, "{matches:?}");
    }

    #[test]
    fn metric_units_are_not_flagged() {
        let rule = GermanUnitConversionRule::new();
        assert!(
            rule.match_sentence(&sentence("Der Weg ist 8 Kilometer lang."))
                .is_empty()
        );
    }
}
