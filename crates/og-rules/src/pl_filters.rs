use og_core::RuleMatch;

pub fn decade_spelling_filter(
    args: &str,
    matched_texts: &[String],
    mut m: RuleMatch,
) -> Option<RuleMatch> {
    let lata = filter_arg(args, "lata", matched_texts)?;
    let chars: Vec<char> = lata.chars().collect();
    if chars.len() < 2 {
        return None;
    }
    let century: String = chars[..2].iter().collect();
    let cent: i32 = century.parse().ok()?;
    let decade: String = chars[2..].iter().collect();
    let wiek = roman_numeral(cent + 1);
    let message = m
        .message()
        .replace("{dekada}", &decade)
        .replace("{wiek}", &wiek);
    for replacement in m.replacements_mut() {
        let value = replacement
            .value()
            .replace("{dekada}", &decade)
            .replace("{wiek}", &wiek);
        replacement.set_value(value);
    }
    m.set_message(message);
    Some(m)
}

pub fn shortened_year_range_filter(
    args: &str,
    matched_texts: &[String],
    m: RuleMatch,
) -> Option<RuleMatch> {
    let x = filter_arg(args, "x", matched_texts)?;
    let y = filter_arg(args, "y", matched_texts)?;
    let x_val: i32 = x.parse().ok()?;
    let chars: Vec<char> = x.chars().collect();
    if chars.len() < 2 {
        return None;
    }
    let century: String = chars[..2].iter().collect();
    let y_val: i32 = format!("{century}{y}").parse().ok()?;
    (x_val >= y_val).then_some(m)
}

fn filter_arg<'a>(args: &str, key: &str, matched_texts: &'a [String]) -> Option<&'a str> {
    for part in args.split_whitespace() {
        let Some((k, val)) = part.split_once(':') else {
            continue;
        };
        if k != key {
            continue;
        }
        let idx = val
            .strip_prefix('\\')
            .and_then(|rest| rest.parse::<usize>().ok())?;
        if idx == 0 || idx > matched_texts.len() {
            return None;
        }
        return Some(matched_texts[idx - 1].as_str());
    }
    None
}

fn roman_numeral(mut num: i32) -> String {
    const NUMBERS: [i32; 13] = [1000, 900, 500, 400, 100, 90, 50, 40, 10, 9, 5, 4, 1];
    const LETTERS: [&str; 13] = [
        "M", "CM", "D", "CD", "C", "XC", "L", "XL", "X", "IX", "V", "IV", "I",
    ];
    let mut roman = String::new();
    for (i, &value) in NUMBERS.iter().enumerate() {
        while num >= value {
            roman.push_str(LETTERS[i]);
            num -= value;
        }
    }
    roman
}

#[cfg(test)]
mod tests {
    use super::*;

    fn match_with(message: &str, replacements: &[&str]) -> RuleMatch {
        let mut m = RuleMatch::new(
            message,
            0,
            1,
            og_core::RuleMatchRule::new("DATA_DEKADY", "test"),
            og_core::RuleMatchContext::new("test", 0, 1),
        );
        m.set_replacements(
            replacements
                .iter()
                .map(|r| og_core::SuggestedReplacement::new(*r))
                .collect(),
        );
        m
    }

    fn texts(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn decade_spelling_fills_the_placeholders_like_java() {
        let lata_texts = texts(&["lata", "1840"]);
        let m = decade_spelling_filter(
            "lata:\\2",
            &lata_texts,
            match_with("Poprawnie: {dekada}. {wiek} w.", &["{dekada}. {wiek} w."]),
        )
        .expect("match kept");
        assert_eq!(m.message(), "Poprawnie: 40. XIX w.");
        assert_eq!(
            m.replacements()
                .iter()
                .map(|r| r.value().to_string())
                .collect::<Vec<_>>(),
            vec!["40. XIX w.".to_string()]
        );
        let m = decade_spelling_filter(
            "lata:\\2",
            &texts(&["lata", "1990"]),
            match_with("{dekada}. {wiek}", &["{dekada}. {wiek}"]),
        )
        .expect("kept");
        assert_eq!(m.message(), "90. XX");
    }

    #[test]
    fn decade_spelling_drops_the_match_where_java_throws() {
        let base = || match_with("{dekada}", &[]);
        assert!(decade_spelling_filter("lata:\\2", &texts(&["a", "9"]), base()).is_none());
        assert!(decade_spelling_filter("lata:\\2", &texts(&["a", "ab"]), base()).is_none());
        assert!(decade_spelling_filter("lata:\\2", &texts(&["a", "1x40"]), base()).is_none());
        assert!(decade_spelling_filter("lata:\\2", &texts(&["a"]), base()).is_none());
    }

    #[test]
    fn shortened_year_range_keeps_only_inverted_or_equal_ranges() {
        let args = "x:\\2 y:\\4";
        let m = || match_with("x", &[]);
        assert!(
            shortened_year_range_filter(args, &texts(&["w", "1855", "i", "40"]), m()).is_some()
        );
        assert!(
            shortened_year_range_filter(args, &texts(&["w", "1915", "i", "15"]), m()).is_some()
        );
        assert!(
            shortened_year_range_filter(args, &texts(&["w", "1855", "i", "60"]), m()).is_none()
        );
        assert!(
            shortened_year_range_filter(args, &texts(&["w", "1998", "i", "05"]), m()).is_some()
        );
        assert!(
            shortened_year_range_filter(args, &texts(&["w", "1998", "i", "99"]), m()).is_none()
        );
        assert!(shortened_year_range_filter(args, &texts(&["w", "x", "i", "1"]), m()).is_none());
    }

    #[test]
    fn filter_arg_resolves_backreferences_only() {
        let texts = texts(&["a", "c"]);
        assert_eq!(filter_arg("lata:\\1", "lata", &texts), Some("a"));
        assert_eq!(filter_arg("lata:\\2", "lata", &texts), Some("c"));
        assert_eq!(filter_arg("lata:\\3", "lata", &texts), None);
        assert_eq!(filter_arg("lata:\\0", "lata", &texts), None);
        assert_eq!(filter_arg("x:\\1 y:\\2", "y", &texts), Some("c"));
        assert_eq!(filter_arg("x:\\1", "missing", &texts), None);
    }
}
