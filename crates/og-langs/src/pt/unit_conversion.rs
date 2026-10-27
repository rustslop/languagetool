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
const POUND: Unit = Unit::new(Dim::Mass, 0.45359237);
const OUNCE: Unit = Unit::new(Dim::Mass, 0.45359237 / 12.0);
const M: Unit = Unit::new(Dim::Length, 1.0);
const FEET: Unit = Unit::new(Dim::Length, 0.3048);
const YARD: Unit = Unit::new(Dim::Length, 0.3048 * 3.0);
const INCH: Unit = Unit::new(Dim::Length, 0.3048 / 12.0);
const MILE: Unit = Unit::new(Dim::Length, 0.3048 * 5280.0);
const M2: Unit = Unit::new(Dim::Area, 1.0);
const FEET2: Unit = Unit::new(Dim::Area, 0.3048 * 0.3048);
const INCH2: Unit = Unit::new(Dim::Area, (0.3048 / 12.0) * (0.3048 / 12.0));
const YARD2: Unit = Unit::new(Dim::Area, (0.3048 * 3.0) * (0.3048 * 3.0));
const M3: Unit = Unit::new(Dim::Volume, 1.0);
const FEET3: Unit = Unit::new(Dim::Volume, 0.3048 * 0.3048 * 0.3048);
const INCH3: Unit = Unit::new(
    Dim::Volume,
    (0.3048 / 12.0) * (0.3048 / 12.0) * (0.3048 / 12.0),
);
const YARD3: Unit = Unit::new(
    Dim::Volume,
    (0.3048 * 3.0) * (0.3048 * 3.0) * (0.3048 * 3.0),
);
const LITRE: Unit = Unit::new(Dim::Volume, 1e-3);
const CELSIUS: Unit = Unit::new(Dim::Temperature, 1.0);
const FAHRENHEIT: Unit = Unit::with_offset(Dim::Temperature, 5.0 / 9.0, -32.0 * 5.0 / 9.0);
const KM_PER_H: Unit = Unit::new(Dim::Speed, 1.0 / 3.6);
const MPH: Unit = Unit::new(Dim::Speed, 0.44704);

struct UnitDef {
    pattern: &'static str,
    unit: Unit,
    symbol: &'static str,
    metric: bool,
}

fn unit_defs() -> Vec<UnitDef> {
    let e = |pattern: &'static str, unit: Unit, symbol: &'static str, metric: bool| UnitDef {
        pattern,
        unit,
        symbol,
        metric,
    };
    vec![
        e("kg", KG, "kg", true),
        e("g", Unit::new(Dim::Mass, 1e-3), "g", true),
        e("t", Unit::new(Dim::Mass, 1e3), "t", true),
        e("lb", POUND, "lb", false),
        e("mi", MILE, "mi", false),
        e("yd", YARD, "yd", false),
        e("(?:ft|′|')(?!(\\w|\\d))", FEET, "ft", false),
        e("(?:inch|″)(?!(\\w|\\d))", INCH, "inch", false),
        e("(?:km/h|kmh)", KM_PER_H, "km/h", true),
        e("(?:mph)", MPH, "mph", false),
        e("km", Unit::new(Dim::Length, 1e3), "km", true),
        e("m", M, "m", true),
        e("cm", Unit::new(Dim::Length, 1e-2), "cm", true),
        e("mm", Unit::new(Dim::Length, 1e-3), "mm", true),
        e("µm", Unit::new(Dim::Length, 1e-6), "µm", true),
        e("nm", Unit::new(Dim::Length, 1e-9), "nm", true),
        e("m(?:\\^2|2|²)", M2, "m²", true),
        e("ha", Unit::new(Dim::Area, 1e4), "ha", true),
        e("a", Unit::new(Dim::Area, 1e2), "a", true),
        e("km(?:\\^2|2|²)", Unit::new(Dim::Area, 1e6), "km²", true),
        e("cm(?:\\^2|2|²)", Unit::new(Dim::Area, 1e-4), "cm²", true),
        e("mm(?:\\^2|2|²)", Unit::new(Dim::Area, 1e-6), "mm²", true),
        e("µm(?:\\^2|2|²)", Unit::new(Dim::Area, 1e-12), "µm²", true),
        e("nm(?:\\^2|2|²)", Unit::new(Dim::Area, 1e-18), "nm²", true),
        e("(?:sq|square) (?:in(?:ch)?|inches)", INCH2, "sq in", false),
        e("(?:inches|in|inch) (?:\\^2|2|²)", INCH2, "in²", false),
        e("(?:sq|square) (?:ft|feet|foot)", FEET2, "sq ft", false),
        e("sf", FEET2, "sf", false),
        e("ft(?:\\^2|2|²)", FEET2, "ft²", false),
        e("(?:sq|square) (?:yds?|yards?)", YARD2, "sq yd", false),
        e("(?:yards?|yds?)(?:\\^2|2|²)", YARD2, "yd²", false),
        e("m(?:\\^3|3|³)", M3, "m³", true),
        e("km(?:\\^3|3|³)", Unit::new(Dim::Volume, 1e9), "km³", true),
        e("cm(?:\\^3|3|³)", Unit::new(Dim::Volume, 1e-6), "cm³", true),
        e("mm(?:\\^3|3|³)", Unit::new(Dim::Volume, 1e-9), "mm³", true),
        e("µm(?:\\^3|3|³)", Unit::new(Dim::Volume, 1e-18), "µm³", true),
        e("nm(?:\\^3|3|³)", Unit::new(Dim::Volume, 1e-27), "nm³", true),
        e("(?:cubic|cu) (?:feet|ft|foot)", FEET3, "cubic feet", false),
        e("(?:feet|ft|foot)(?:\\^3|3|³)", FEET3, "ft³", false),
        e(
            "(?:cubic|cu) (?:inch|in|inches)",
            INCH3,
            "cubic inch",
            false,
        ),
        e("(?:inch|in)(?:\\^3|3|³)", INCH3, "inch³", false),
        e("(?:cubic|cu) (?:yards?|yds?)", YARD3, "cubic yard", false),
        e("(?:yard|yd)(?:\\^3|3|³)", YARD3, "yard³", false),
        e("l", LITRE, "l", true),
        e("ml", Unit::new(Dim::Volume, 1e-3), "ml", true),
        e("°F", FAHRENHEIT, "°F", false),
        e("°C", CELSIUS, "°C", true),
        e("(qui|ki)lo(grama)?", KG, "quilogramas", true),
        e("grama", Unit::new(Dim::Mass, 1e-3), "gramas", true),
        e("toneladas?", Unit::new(Dim::Mass, 1e3), "toneladas", true),
        e("libras?", POUND, "libras", false),
        e("onças?", OUNCE, "onças", false),
        e("milhas?", MILE, "milhas", false),
        e("jardas?", YARD, "jardas", false),
        e("pés?", FEET, "pés", false),
        e("polegadas?", INCH, "polegadas", false),
        e(
            "(qu|k)ilômetros? por hora",
            KM_PER_H,
            "quilômetros por hora",
            true,
        ),
        e("milhas? por hora", MPH, "milhas por hora", false),
        e("metros?", M, "metros", true),
        e(
            "(qu|k)ilômetros?",
            Unit::new(Dim::Length, 1e3),
            "quilômetros",
            true,
        ),
        e(
            "decímetros?",
            Unit::new(Dim::Length, 1e-1),
            "decímetros",
            false,
        ),
        e(
            "centímetros?",
            Unit::new(Dim::Length, 1e-2),
            "centímetros",
            true,
        ),
        e(
            "milímetros?",
            Unit::new(Dim::Length, 1e-3),
            "milímetros",
            true,
        ),
        e(
            "micrômetros?",
            Unit::new(Dim::Length, 1e-6),
            "micrômetros",
            true,
        ),
        e(
            "nanômetros?",
            Unit::new(Dim::Length, 1e-9),
            "nanômetros",
            true,
        ),
        e(
            "picômetros?",
            Unit::new(Dim::Length, 1e-12),
            "picômetros",
            true,
        ),
        e(
            "fentômetros?",
            Unit::new(Dim::Length, 1e-15),
            "fentômetros",
            true,
        ),
        e("metros? quadrados?", M2, "metros quadrados", true),
        e("hectar(es)?", Unit::new(Dim::Area, 1e4), "hectares", true),
        e("ares?", Unit::new(Dim::Area, 1e2), "ares", true),
        e(
            "(k|qui)ilômetros? quadrados?",
            Unit::new(Dim::Area, 1e6),
            "quilômetros quadrados",
            true,
        ),
        e(
            "decímetros? quadrados?",
            Unit::new(Dim::Area, 1e-2),
            "decímetros quadrados",
            false,
        ),
        e(
            "centímetros? quadrados?",
            Unit::new(Dim::Area, 1e-4),
            "centímetros quadrados",
            true,
        ),
        e(
            "milímetros? quadrados?",
            Unit::new(Dim::Area, 1e-6),
            "milímetros quadrados",
            true,
        ),
        e(
            "micrômetros? quadrados?",
            Unit::new(Dim::Area, 1e-12),
            "micrômetros quadrados",
            true,
        ),
        e(
            "nanômetros? quadrados?",
            Unit::new(Dim::Area, 1e-18),
            "nanômetros quadrados",
            true,
        ),
        e("metros? cúbicos?", M3, "metros cúbicos", true),
        e(
            "(k|qu)ilômetros? cúbicos?",
            Unit::new(Dim::Volume, 1e9),
            "quilômetros cúbicos",
            true,
        ),
        e(
            "decímetros? cúbicos?",
            Unit::new(Dim::Volume, 1e-3),
            "decímetros cúbicos",
            false,
        ),
        e(
            "centímetros? cúbicos?",
            Unit::new(Dim::Volume, 1e-6),
            "centímetros cúbicos",
            true,
        ),
        e(
            "milímetros? cúbicos?",
            Unit::new(Dim::Volume, 1e-9),
            "milímetros cúbicos",
            true,
        ),
        e(
            "micrômetros? cúbicos?",
            Unit::new(Dim::Volume, 1e-18),
            "micrômetros cúbicos",
            true,
        ),
        e(
            "nanômetros? cúbicos?",
            Unit::new(Dim::Volume, 1e-27),
            "nanômetros cúbicos",
            true,
        ),
        e("litros?", LITRE, "litros", true),
        e(
            "mililitros?",
            Unit::new(Dim::Volume, 1e-3),
            "mililitros",
            true,
        ),
        e(
            "(?:Graus)? Fahrenheit",
            FAHRENHEIT,
            "graus Fahrenheit",
            false,
        ),
        e(
            "(?:Graus)? (Celsi[ou]s|[cC]entígrados?)",
            CELSIUS,
            "graus Celsius",
            true,
        ),
    ]
}

const MSG_CHECK: &str = "Esta conversão não parece estar precisa. Gostaria de corrigi-la?";
const MSG_SUGGESTION: &str = "Deseja adicionar automaticamente uma conversão ao sistema métrico?";
const MSG_CHECK_UNKNOWN_UNIT: &str = "A unidade usada nesta conversão não foi reconhecida.";
const MSG_UNIT_MISMATCH: &str = "Estas unidades de medida não são compatíveis.";
const SHORT_CHECK: &str = "Conversão incorreta. Corrigir?";
const SHORT_SUGGESTION: &str = "Adicionar conversão ao sistema métrico?";
const SHORT_CHECK_UNKNOWN_UNIT: &str = "Unidade desconhecida.";
const SHORT_UNIT_MISMATCH: &str = "Unidade incompatível.";

enum Message {
    Check,
    Suggestion,
    CheckUnknownUnit,
    UnitMismatch,
}

impl Message {
    fn text(&self) -> &'static str {
        match self {
            Message::Check => MSG_CHECK,
            Message::Suggestion => MSG_SUGGESTION,
            Message::CheckUnknownUnit => MSG_CHECK_UNKNOWN_UNIT,
            Message::UnitMismatch => MSG_UNIT_MISMATCH,
        }
    }

    fn short(&self) -> &'static str {
        match self {
            Message::Check => SHORT_CHECK,
            Message::Suggestion => SHORT_SUGGESTION,
            Message::CheckUnknownUnit => SHORT_CHECK_UNKNOWN_UNIT,
            Message::UnitMismatch => SHORT_UNIT_MISMATCH,
        }
    }
}

pub struct PortugueseUnitConversionRule {
    patterns: Vec<(regex::Regex, Unit)>,
    symbols: Vec<(Unit, Vec<&'static str>)>,
    metric_units: Vec<Unit>,
    converted_re: regex::Regex,
    feet_inch_re: regex::Regex,
    feet_inch_shape_re: regex::Regex,
    number_range_part_re: regex::Regex,
    anti_res: Vec<regex::Regex>,
}

impl PortugueseUnitConversionRule {
    pub fn new() -> Self {
        let mut patterns = Vec::new();
        let mut symbols: Vec<(Unit, Vec<&'static str>)> = Vec::new();
        let mut metric_units = Vec::new();
        for def in unit_defs() {
            let full = format!("{NUMBER_REGEX}[\\s\\u{{00A0}}]{{0,5}}{}\\b", def.pattern);
            if let Ok(re) = regex::Regex::new(&full) {
                patterns.push((re, def.unit));
            }
            match symbols.iter_mut().find(|(u, _)| *u == def.unit) {
                Some((_, list)) => list.push(def.symbol),
                None => symbols.push((def.unit, vec![def.symbol])),
            }
            if def.metric && !metric_units.contains(&def.unit) {
                metric_units.push(def.unit);
            }
        }
        let converted_re =
            regex::Regex::new(r"\s*\((?:ca. )?(-?[0-9]{1,32}[0-9,.]{0,32})\s*([^)]+)\s*\)")
                .unwrap();
        let feet_inch_re = regex::Regex::new(r#"(\d+)(?:ft|′|')\s*(\d+)\s*(?:in|"|″)?"#).unwrap();
        let feet_inch_shape_re = regex::Regex::new(r"^\(\d+ (?:feet|ft) \d+ inch\)$").unwrap();
        let number_range_part_re = regex::Regex::new(&format!("{NUMBER_REGEX}$")).unwrap();
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
            patterns,
            symbols,
            metric_units,
            converted_re,
            feet_inch_re,
            feet_inch_shape_re,
            number_range_part_re,
            anti_res,
        }
    }

    fn symbols_of(&self, unit: Unit) -> &[&'static str] {
        self.symbols
            .iter()
            .find(|(u, _)| *u == unit)
            .map(|(_, list)| list.as_slice())
            .unwrap_or(&[])
    }

    fn is_metric(&self, unit: Unit) -> bool {
        self.metric_units.contains(&unit)
    }

    fn metric_equivalents(&self, value: f64, unit: Unit) -> Option<Vec<(Unit, f64)>> {
        if self.is_metric(unit) {
            return None;
        }
        let mut out: Vec<(Unit, f64)> = Vec::new();
        for metric in &self.metric_units {
            if metric.dim == unit.dim {
                out.push((*metric, metric.scale_from_base(unit.scale_to_base(value))));
            }
        }
        if out.is_empty() {
            return None;
        }
        out.sort_by(|a, b| {
            naturalness(a.1)
                .partial_cmp(&naturalness(b.1))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Some(out)
    }

    fn format_conversions(&self, conversions: &[(Unit, f64)]) -> Vec<String> {
        let mut formatted: Vec<String> = Vec::new();
        for (unit, converted) in conversions {
            let rounded = converted.round();
            for symbol in self.symbols_of(*unit) {
                if formatted.len() > MAX_SUGGESTIONS {
                    break;
                }
                if (converted - rounded).abs() / converted.abs() < ROUNDING_DELTA && rounded != 0.0
                {
                    let s = format!("aprox. {} {}", format_number(rounded), symbol);
                    if !formatted.contains(&s) {
                        formatted.push(s);
                    }
                }
                let formatted_number = format_number(*converted);
                let s = format!("{formatted_number} {symbol}");
                if !formatted.contains(&s) && formatted_number != "0" {
                    formatted.push(s);
                }
            }
        }
        formatted
    }

    fn format_measurement(&self, value: f64, unit: Unit) -> Option<Vec<String>> {
        self.metric_equivalents(value, unit)
            .map(|equivalents| self.format_conversions(&equivalents))
            .filter(|formatted| !formatted.is_empty())
    }

    fn special_matches<'t>(&'t self, text: &'t str) -> Vec<(usize, regex::Captures<'t>)> {
        let mut out = Vec::new();
        for caps in self.feet_inch_re.captures_iter(text) {
            let digits_start = caps.get(1).map(|m| m.start()).unwrap_or(0);
            let mut prev = text[..digits_start].char_indices().rev();
            let Some((_, first)) = prev.next() else {
                continue;
            };
            if first.is_whitespace() {
                let anchor = match prev.next() {
                    Some((_, second)) => second,
                    None => continue,
                };
                if is_java_digit(anchor) || anchor == 'º' || anchor == '°' {
                    continue;
                }
                out.push((digits_start - first.len_utf8(), caps));
            } else {
                if is_java_digit(first) || first == 'º' || first == '°' {
                    continue;
                }
                out.push((digits_start, caps));
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn try_conversion(
        &self,
        text: &str,
        start: usize,
        end: usize,
        number_text: Option<&str>,
        custom_value: Option<f64>,
        unit: Unit,
        matches: &mut Vec<RuleMatch>,
        ignore_ranges: &mut Vec<(usize, usize)>,
    ) {
        ignore_ranges.push((start, end));
        let converted_mat = self
            .converted_re
            .find(&text[end..])
            .filter(|m| m.start() == 0);

        let value = match custom_value {
            Some(v) => Some(v),
            None => number_text.and_then(|raw| {
                let mut raw = raw;
                if raw.starts_with('-') && self.number_range_part_re.is_match(&text[..start]) {
                    raw = &raw[1..];
                }
                parse_number(raw)
            }),
        };
        let Some(value) = value else {
            return;
        };
        let converted = self.format_measurement(value, unit);
        let matched_text = &text[start..end];

        let Some(cm) = converted_mat else {
            let Some(converted) = converted else {
                return;
            };
            let suggestions = wrap_suggestions(matched_text, &converted);
            matches.push(self.build_match(Message::Suggestion, start, end, text, suggestions));
            return;
        };

        let abs_start = end + cm.start();
        let abs_end = end + cm.end();
        ignore_ranges.push((abs_start, abs_end));
        let converted_str = cm.as_str();
        let trimmed = converted_str.trim();
        let Some(inner) = trimmed
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .map(str::to_string)
        else {
            return;
        };
        if let Some(converted) = converted.as_ref()
            && converted.contains(&inner)
        {
            return;
        }
        let caps = self
            .converted_re
            .captures(converted_str)
            .expect("matched conversion has captures");
        let converted_unit = self
            .patterns
            .iter()
            .find(|(re, _)| re.is_match(trimmed))
            .map(|(_, u)| *u);
        let Some(converted_unit) = converted_unit else {
            if let Some(converted) = converted {
                let g1 = caps.get(1).map(|m| end + m.start()).unwrap_or(abs_start);
                let g2 = caps.get(2).map(|m| end + m.end()).unwrap_or(abs_end);
                let suggestions = bare_suggestions(&converted);
                matches.push(self.build_match(
                    Message::CheckUnknownUnit,
                    g1,
                    g2,
                    text,
                    suggestions,
                ));
            }
            return;
        };
        if converted_unit == unit {
            return;
        }
        let Some(value_in) = caps.get(1).and_then(|m| parse_number(m.as_str())) else {
            return;
        };
        if self.feet_inch_shape_re.is_match(trimmed) {
            return;
        }
        match converted {
            None => {
                if unit.dim != converted_unit.dim {
                    matches.push(self.build_match(
                        Message::UnitMismatch,
                        start,
                        abs_end,
                        text,
                        Vec::new(),
                    ));
                    return;
                }
                let unit_converted = converted_unit.scale_from_base(unit.scale_to_base(value));
                if (unit_converted - value_in).abs() > DELTA {
                    let reverse = self.format_conversions(&[(converted_unit, unit_converted)]);
                    if reverse.contains(&inner) {
                        return;
                    }
                    let g1 = caps.get(1).map(|m| end + m.start()).unwrap_or(abs_start);
                    let g2 = caps.get(1).map(|m| end + m.end()).unwrap_or(abs_end);
                    let suggestions = bare_suggestions(&reverse);
                    matches.push(self.build_match(Message::Check, g1, g2, text, suggestions));
                }
            }
            Some(converted) => {
                let Some(equivalents) = self.metric_equivalents(value, unit) else {
                    return;
                };
                let Some((metric_unit, metric_value)) = equivalents.first().copied() else {
                    return;
                };
                if !(converted_unit == metric_unit && (value_in - metric_value).abs() < DELTA) {
                    let suggestions = wrap_suggestions(matched_text, &converted);
                    matches.push(self.build_match(
                        Message::Check,
                        start,
                        abs_end,
                        text,
                        suggestions,
                    ));
                }
            }
        }
    }

    fn build_match(
        &self,
        message: Message,
        start: usize,
        end: usize,
        text: &str,
        suggestions: Vec<SuggestedReplacement>,
    ) -> RuleMatch {
        let rm_rule = RuleMatchRule::new(self.id(), self.description())
            .with_category(self.category())
            .with_issue_type(self.issue_type().as_str());
        let ctx_start = char_boundary_after(text, start.saturating_sub(40));
        let ctx_end = char_boundary_at(text, (end + 40).min(text.len()));
        RuleMatch::new(
            message.text(),
            start,
            end - start,
            rm_rule,
            RuleMatchContext::new(
                text[ctx_start..ctx_end].to_string(),
                ctx_start,
                ctx_end - ctx_start,
            ),
        )
        .with_replacements(suggestions)
        .with_short_message(message.short())
    }
}

impl Default for PortugueseUnitConversionRule {
    fn default() -> Self {
        Self::new()
    }
}

fn is_java_digit(c: char) -> bool {
    c.is_numeric()
}

fn wrap_suggestions(original: &str, converted: &[String]) -> Vec<SuggestedReplacement> {
    converted
        .iter()
        .map(|c| SuggestedReplacement::new(format!("{original} ({c})")))
        .collect()
}

fn bare_suggestions(converted: &[String]) -> Vec<SuggestedReplacement> {
    converted
        .iter()
        .map(|c| SuggestedReplacement::new(c.clone()))
        .collect()
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
    } else if abs < 100.0 {
        abs - 50.0
    } else {
        abs * abs
    }
}

fn format_number(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    let s = format!("{rounded:.2}");
    let (sign, digits) = match s.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", s.as_str()),
    };
    let (int_part, frac_part) = match digits.split_once('.') {
        Some((int, frac)) => (int, frac),
        None => (digits, ""),
    };
    let mut grouped = String::new();
    let int_digits = int_part.as_bytes();
    for (i, b) in int_digits.iter().enumerate() {
        if i > 0 && (int_digits.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(*b as char);
    }
    let frac_trimmed = frac_part.trim_end_matches('0');
    if frac_trimmed.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped},{frac_trimmed}")
    }
}

fn parse_number(raw: &str) -> Option<f64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .collect();
    let commas = cleaned.chars().filter(|c| *c == ',').count();
    // Java's GERMANY NumberFormat treats '.' as grouping, so a bare dot is dropped
    let normalized = if commas > 1 {
        return None;
    } else if commas == 1 {
        cleaned.replace('.', "").replace(',', ".")
    } else {
        cleaned.replace('.', "")
    };
    normalized.parse::<f64>().ok()
}

impl Rule for PortugueseUnitConversionRule {
    fn id(&self) -> &str {
        "UNIDADES_METRICAS"
    }

    fn description(&self) -> &str {
        "Sugere ou verifica informações equivalentes à métrica de unidades de medida específicas."
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("STYLE", "Estilo")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let text = sentence.text();
        let mut matches = Vec::new();
        let mut ignore_ranges: Vec<(usize, usize)> = Vec::new();

        for (start, caps) in self.special_matches(text) {
            let ignored = ignore_ranges
                .iter()
                .any(|(s, e)| start >= *s && caps.get(0).map(|m| m.end()).unwrap_or(0) <= *e);
            if ignored {
                continue;
            }
            let end = caps.get(0).map(|m| m.end()).unwrap_or(start);
            let feet = caps.get(1).and_then(|m| parse_number(m.as_str()));
            let inch = caps
                .get(2)
                .and_then(|m| parse_number(m.as_str()))
                .unwrap_or(0.0);
            let Some(feet) = feet else {
                continue;
            };
            self.try_conversion(
                text,
                start,
                end,
                None,
                Some(feet + inch / 12.0),
                FEET,
                &mut matches,
                &mut ignore_ranges,
            );
        }

        for is_metric in [true, false] {
            for (re, unit) in &self.patterns {
                if self.is_metric(*unit) != is_metric {
                    continue;
                }
                for caps in re.captures_iter(text) {
                    let Some(full) = caps.get(0) else {
                        continue;
                    };
                    let ignored = ignore_ranges
                        .iter()
                        .any(|(s, e)| full.start() >= *s && full.end() <= *e);
                    if ignored {
                        continue;
                    }
                    let number_text = caps.get(1).map(|m| m.as_str());
                    self.try_conversion(
                        text,
                        full.start(),
                        full.end(),
                        number_text,
                        None,
                        *unit,
                        &mut matches,
                        &mut ignore_ranges,
                    );
                }
            }
        }

        let mut by_start: Vec<RuleMatch> = Vec::new();
        for m in matches {
            match by_start
                .iter_mut()
                .find(|other| other.offset() == m.offset())
            {
                Some(other) => {
                    if m.offset() + m.length() > other.offset() + other.length() {
                        *other = m;
                    }
                }
                None => by_start.push(m),
            }
        }
        if !by_start.is_empty() {
            for anti in &self.anti_res {
                let mut pos = 0;
                while pos < text.len() {
                    let Some(found) = anti.find_at(text, pos) else {
                        break;
                    };
                    by_start.retain(|m| {
                        !(found.start() <= m.offset() && found.end() >= m.offset()
                            || found.start() <= m.offset() + m.length()
                                && found.end() >= m.offset() + m.length())
                    });
                    pos = found.end() + 1;
                }
            }
        }
        by_start.sort_by_key(|m| m.offset());
        by_start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence(text: &str) -> AnalyzedSentence {
        AnalyzedSentence::new(text, 0, text.len())
    }

    fn assert_matches(input: &str, expected: usize, converted: Option<&str>) {
        let rule = PortugueseUnitConversionRule::new();
        let matches = rule.match_sentence(&sentence(input));
        assert_eq!(
            matches.len(),
            expected,
            "input: {input:?}, got: {matches:?}"
        );
        if let (Some(converted), Some(first)) = (converted, matches.first()) {
            let hit = first
                .replacements()
                .iter()
                .any(|r| r.value().contains(converted));
            assert!(
                hit,
                "expected a suggestion containing {converted:?}, got {:?}",
                first.replacements()
            );
        }
    }

    #[test]
    fn suggests_metric_equivalent_for_pes() {
        assert_matches("Eu tenho 6 pés de altura.", 1, Some("1,83 metros"));
    }

    #[test]
    fn flags_wrong_conversion_for_pes() {
        assert_matches("Eu tenho 6 pés (2,02 m) de altura.", 1, Some("1,83 metros"));
    }

    #[test]
    fn accepts_correct_conversion_for_pes() {
        assert_matches("Eu tenho 6 pés (1,82 m) de altura.", 0, None);
    }

    #[test]
    fn converts_milhas_to_quilometros() {
        assert_matches(
            "A via tem 100 milhas de comprimento.",
            1,
            Some("160,93 quilômetros"),
        );
    }

    #[test]
    fn reverse_checks_metric_source_with_milhas_target() {
        assert_matches(
            "A via tem 10 km (20 milhas) de comprimento.",
            1,
            Some("6,21"),
        );
    }

    #[test]
    fn accepts_correct_milhas_conversion_after_km() {
        assert_matches("A via tem 10 km (6,21 milhas) de comprimento.", 0, None);
    }

    #[test]
    fn accepts_quilometros_conversion_after_milhas() {
        assert_matches(
            "A via tem 100 milhas (160,93 quilômetros) de comprimento.",
            0,
            None,
        );
    }

    #[test]
    fn converts_libras_with_german_grouping_parse() {
        assert_matches("A carga é de 10.000 libras.", 1, Some("4,54 toneladas"));
    }

    #[test]
    fn special_feet_inch_pattern_matches() {
        assert_matches("Isto tem 5'6\" de altura.", 1, Some("1,68 m"));
    }

    #[test]
    fn converts_sq_ft() {
        assert_matches(
            "O meu novo apartamento tem 500 sq ft de área.",
            1,
            Some("46,45 metros quadrados"),
        );
    }

    #[test]
    fn latitude_minutes_are_not_converted() {
        assert_matches(
            "Sendo a latitude 8º 32' 00\" e a longitude 39º 22' 49\".",
            0,
            None,
        );
        assert_matches(
            "Sendo a latitude 8º32'00\" e a longitude 39º22'49\".",
            0,
            None,
        );
    }

    #[test]
    fn same_unit_conversion_is_not_flagged() {
        assert_matches("Com altura de 4,8 cm (22,08 cm).", 0, None);
    }

    #[test]
    fn composite_squared_expression_is_not_flagged() {
        assert_matches("Com altura de 4,8 mm x 5,2 cm (2,5 cm²).", 0, None);
        assert_matches("Com altura de 4,8 mm × 5,2 cm (2,5 cm²).", 0, None);
    }

    #[test]
    fn id_and_messages_match_java() {
        let rule = PortugueseUnitConversionRule::new();
        assert_eq!(rule.id(), "UNIDADES_METRICAS");
        assert_eq!(
            rule.description(),
            "Sugere ou verifica informações equivalentes à métrica de unidades de medida específicas."
        );
    }

    #[test]
    fn converts_fahrenheit_using_the_offset_scale() {
        assert_matches(
            "A temperatura \u{e9} de 100 \u{b0}F.",
            1,
            Some("37,78 \u{b0}C"),
        );
        assert_matches(
            "A temperatura \u{e9} de 100 \u{b0}F (30 \u{b0}C).",
            1,
            Some("100 \u{b0}F (37,78 \u{b0}C)"),
        );
        assert_matches(
            "A temperatura \u{e9} de 100 \u{b0}F (37,78 \u{b0}C).",
            0,
            None,
        );
    }

    #[test]
    fn mismatched_dimension_in_parenthesis_is_flagged() {
        let rule = PortugueseUnitConversionRule::new();
        let text = "A \u{e1}gua congela a 0 \u{b0}C (32 milhas).";
        let matches = rule.match_sentence(&sentence(text));
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert!(
            matches[0]
                .message()
                .contains("unidades de medida n\u{e3}o s\u{e3}o compat\u{ed}veis"),
            "{}",
            matches[0].message()
        );
        assert!(matches[0].replacements().is_empty());
        assert!(
            matches[0]
                .short_message()
                .is_some_and(|s| s.contains("incompat\u{ed}vel"))
        );
    }

    #[test]
    fn unrecognized_unit_in_parenthesis_yields_check_unknown_unit() {
        let rule = PortugueseUnitConversionRule::new();
        let text = "Eu tenho 6 p\u{e9}s (2 bra\u{e7}as) de altura.";
        let matches = rule.match_sentence(&sentence(text));
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert!(
            matches[0].message().contains("n\u{e3}o foi reconhecida"),
            "{}",
            matches[0].message()
        );
        assert!(
            matches[0].replacements()[0].value().contains("1,83"),
            "the metric equivalents of the source unit must still be offered: {:?}",
            matches[0].replacements()
        );
    }

    #[test]
    fn two_conversions_in_one_sentence_each_keep_their_match() {
        let rule = PortugueseUnitConversionRule::new();
        let text = "A via tem 10 km (20 milhas) e 5 milhas (8 km) de comprimento.";
        let matches = rule.match_sentence(&sentence(text));
        let spans: Vec<(usize, &str)> = matches
            .iter()
            .map(|m| (m.offset(), m.short_message().unwrap_or_default()))
            .collect();
        assert_eq!(
            spans,
            vec![
                (17, "Convers\u{e3}o incorreta. Corrigir?"),
                (30, "Convers\u{e3}o incorreta. Corrigir?"),
            ],
            "{matches:?}"
        );
    }

    #[test]
    fn format_number_uses_german_locale_shape() {
        assert_eq!(format_number(1.8288), "1,83");
        assert_eq!(format_number(4.5359237), "4,54");
        assert_eq!(format_number(2.0), "2");
        assert_eq!(format_number(1609.344), "1.609,34");
        assert_eq!(format_number(-30.0), "-30");
    }

    #[test]
    fn parse_number_uses_german_locale_shape() {
        assert_eq!(parse_number("10.000"), Some(10000.0));
        assert_eq!(parse_number("2,02"), Some(2.02));
        assert_eq!(parse_number("1.234,56"), Some(1234.56));
        assert_eq!(parse_number("6"), Some(6.0));
    }
}
