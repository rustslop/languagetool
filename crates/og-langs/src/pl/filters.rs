use og_core::RuleMatch;

pub fn is_undispatched_pl_filter(class: &str) -> bool {
    class == "org.languagetool.rules.pl.DecadeSpellingFilter"
        || class == "org.languagetool.rules.ShortenedYearRangeChecker"
}

pub fn apply_undispatched_pl_filter(
    class: &str,
    args: &str,
    element_texts: &[Option<String>],
    m: RuleMatch,
) -> Option<RuleMatch> {
    if class == "org.languagetool.rules.pl.DecadeSpellingFilter" {
        decade_spelling(&filter_arg(args, "lata", element_texts)?, m)
    } else {
        shortened_year_range(
            &filter_arg(args, "x", element_texts)?,
            &filter_arg(args, "y", element_texts)?,
            m,
        )
    }
}

fn filter_arg(args: &str, key: &str, element_texts: &[Option<String>]) -> Option<String> {
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
        if idx == 0 || idx > element_texts.len() {
            return None;
        }
        return element_texts[idx - 1].clone();
    }
    None
}

fn decade_spelling(lata: &str, mut m: RuleMatch) -> Option<RuleMatch> {
    let chars: Vec<char> = lata.chars().collect();
    if chars.len() < 2 {
        return None;
    }
    let century: String = chars[..2].iter().collect();
    let cent: u32 = century.parse().ok()?;
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

fn shortened_year_range(x: &str, y: &str, m: RuleMatch) -> Option<RuleMatch> {
    let x_val: i32 = x.parse().ok()?;
    let chars: Vec<char> = x.chars().collect();
    if chars.len() < 2 {
        return None;
    }
    let century: String = chars[..2].iter().collect();
    let y_val: i32 = format!("{century}{y}").parse().ok()?;
    (x_val >= y_val).then_some(m)
}

fn roman_numeral(mut num: u32) -> String {
    const NUMBERS: [u32; 13] = [1000, 900, 500, 400, 100, 90, 50, 40, 10, 9, 5, 4, 1];
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

    #[test]
    fn decade_spelling_fills_the_placeholders_like_java() {
        let m = decade_spelling(
            "1840",
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
        let m = decade_spelling(
            "1990",
            match_with("{dekada}. {wiek}", &["{dekada}. {wiek}"]),
        )
        .expect("kept");
        assert_eq!(m.message(), "90. XX");
        let m = decade_spelling(
            "1900",
            match_with("pierwsza dekada {wiek} w.", &["pierwsza dekada {wiek} w."]),
        )
        .expect("kept");
        assert_eq!(m.message(), "pierwsza dekada XX w.");
    }

    #[test]
    fn decade_spelling_drops_the_match_where_java_throws() {
        assert!(decade_spelling("9", match_with("{dekada}", &[])).is_none());
        assert!(decade_spelling("ab", match_with("{dekada}", &[])).is_none());
        assert!(decade_spelling("1x40", match_with("{dekada}", &[])).is_none());
    }

    #[test]
    fn roman_numeral_covers_the_java_table() {
        assert_eq!(roman_numeral(12), "XII");
        assert_eq!(roman_numeral(13), "XIII");
        assert_eq!(roman_numeral(15), "XV");
        assert_eq!(roman_numeral(17), "XVII");
        assert_eq!(roman_numeral(19), "XIX");
        assert_eq!(roman_numeral(20), "XX");
        assert_eq!(roman_numeral(1), "I");
        assert_eq!(roman_numeral(100), "C");
    }

    #[test]
    fn shortened_year_range_keeps_only_inverted_or_equal_ranges() {
        let m = match_with("x", &[]);
        assert!(shortened_year_range("1855", "40", m.clone()).is_some());
        assert!(shortened_year_range("1915", "15", m.clone()).is_some());
        assert!(shortened_year_range("1855", "60", m.clone()).is_none());
        assert!(shortened_year_range("1998", "05", m.clone()).is_some());
        assert!(shortened_year_range("1998", "99", m.clone()).is_none());
        assert!(shortened_year_range("x", "1", m).is_none());
    }

    #[test]
    fn filter_arg_resolves_backreferences_only() {
        let texts = vec![Some("a".to_string()), None, Some("c".to_string())];
        assert_eq!(
            filter_arg("lata:\\1", "lata", &texts),
            Some("a".to_string())
        );
        assert_eq!(filter_arg("lata:\\2", "lata", &texts), None);
        assert_eq!(filter_arg("lata:\\4", "lata", &texts), None);
        assert_eq!(filter_arg("lata:\\0", "lata", &texts), None);
        assert_eq!(
            filter_arg("x:\\1 y:\\3", "y", &texts),
            Some("c".to_string())
        );
        assert_eq!(filter_arg("x:\\1", "missing", &texts), None);
    }
}
