pub const CATALAN_WRONG_YEAR_MESSAGE: &str =
    "Aquesta data no és correcta. ¿Us referiu a l'any \"{currentYear}\"?";

const WEEKDAY_NAMES: [(&str, &str, u32); 7] = [
    ("dg", "diumenge", 1),
    ("dl", "dilluns", 2),
    ("dt", "dimarts", 3),
    ("dc", "dimecres", 4),
    ("dj", "dijous", 5),
    ("dv", "divendres", 6),
    ("ds", "dissabte", 7),
];

const MONTH_PREFIXES: [(&str, u32); 12] = [
    ("gen", 1),
    ("febr", 2),
    ("març", 3),
    ("abr", 4),
    ("maig", 5),
    ("juny", 6),
    ("jul", 7),
    ("ag", 8),
    ("set", 9),
    ("oct", 10),
    ("nov", 11),
    ("des", 12),
];

pub fn parse_weekday_catalan(day: &str) -> u32 {
    let lower = day.to_lowercase();
    WEEKDAY_NAMES
        .iter()
        .find(|(abbrev, full, _)| lower == *abbrev || lower == *full)
        .map_or(0, |(_, _, n)| *n)
}

pub fn day_name_catalan(weekday: u32) -> &'static str {
    match weekday {
        1 => "diumenge",
        2 => "dilluns",
        3 => "dimarts",
        4 => "dimecres",
        5 => "dijous",
        6 => "divendres",
        7 => "dissabte",
        _ => "",
    }
}

pub fn parse_month_catalan(month: &str) -> u32 {
    let lower = month.to_lowercase();
    MONTH_PREFIXES
        .iter()
        .find(|(prefix, _)| lower.starts_with(prefix))
        .map_or(0, |(_, n)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekday_table_covers_every_java_form_exactly() {
        for (abbrev, full, n) in WEEKDAY_NAMES {
            assert_eq!(parse_weekday_catalan(abbrev), n, "abbrev {abbrev}");
            assert_eq!(parse_weekday_catalan(full), n, "full {full}");
            assert_eq!(
                parse_weekday_catalan(&full.to_uppercase()),
                n,
                "uppercase {full}"
            );
            assert_eq!(day_name_catalan(n), full);
        }
        assert_eq!(
            parse_weekday_catalan("dium"),
            0,
            "DateFilterHelper.java:31-42 matches day names with equals, not startsWith - \
             'dium' throws RuntimeException in Java, which \
             AbstractDateCheckWithSuggestionsFilter.java:205-210 catches to drop the match"
        );
        assert_eq!(parse_weekday_catalan("divendres "), 0);
        assert_eq!(day_name_catalan(0), "");
        assert_eq!(day_name_catalan(8), "");
    }

    #[test]
    fn month_table_matches_the_java_prefix_chain() {
        let full_names = [
            ("gener", 1),
            ("febrer", 2),
            ("març", 3),
            ("abril", 4),
            ("maig", 5),
            ("juny", 6),
            ("juliol", 7),
            ("agost", 8),
            ("setembre", 9),
            ("octubre", 10),
            ("novembre", 11),
            ("desembre", 12),
        ];
        for (name, n) in full_names {
            assert_eq!(parse_month_catalan(name), n, "full {name}");
            assert_eq!(parse_month_catalan(&name.to_uppercase()), n, "upper {name}");
        }
        for (prefix, n) in MONTH_PREFIXES {
            assert_eq!(parse_month_catalan(prefix), n, "prefix {prefix}");
        }
        assert_eq!(
            parse_month_catalan("mar"),
            0,
            "DateFilterHelper.java:57 requires the cedilla: 'mar' is not a Catalan month"
        );
        assert_eq!(parse_month_catalan("x"), 0);
    }

    fn weekday_of(year: i32, month: u32, day: u32) -> u32 {
        let y = if month <= 2 { year - 1 } else { year };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let mp = ((month + 9) % 12) as i32;
        let doy = (153 * mp + 2) / 5 + day as i32 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146097 + doe - 719468;
        (days + 4).rem_euclid(7) as u32 + 1
    }

    #[test]
    fn weekday_arithmetic_agrees_with_java_calendar() {
        assert_eq!(weekday_of(1970, 1, 1), 5, "epoch is a Thursday");
        assert_eq!(weekday_of(2014, 10, 7), 3, "7 October 2014 is a dimarts");
        assert_eq!(weekday_of(2013, 8, 15), 5, "15 August 2013 is a dijous");
        assert_eq!(weekday_of(1912, 7, 12), 6, "12 July 1912 is a divendres");
    }

    #[test]
    fn ca_date_weekday_examples_drop_or_keep_like_java() {
        let drops = [
            ("Dimarts", 2014, 10, 7),
            ("Dimarts", 2014, 10, 7),
            ("divendres", 1912, 7, 12),
            ("Dimarts", 2014, 10, 7),
            ("Dimarts", 2014, 10, 7),
        ];
        for (claimed, y, m, d) in drops {
            assert_eq!(
                parse_weekday_catalan(claimed),
                weekday_of(y, m, d),
                "the five CA_DATE_WEEKDAY correct examples are true dates: \
                 AbstractDateCheckWithSuggestionsFilter.java:186-189 returns null and Java \
                 reports no match - the round-22 DateLang::Ca wiring drops them the same \
                 way (pinned in trace::round21_clitics_stay_whole_so_date_patterns_match)"
            );
        }
        let keeps = [
            ("Dilluns", 2014, 10, 7, "dimarts"),
            ("Divendres", 2013, 8, 15, "dijous"),
        ];
        for (claimed, y, m, d, real) in keeps {
            let claimed_n = parse_weekday_catalan(claimed);
            let actual_n = weekday_of(y, m, d);
            assert_ne!(claimed_n, actual_n);
            assert_eq!(day_name_catalan(actual_n), real);
        }
    }

    #[test]
    fn wrong_year_message_is_the_catalan_localization() {
        assert_eq!(
            CATALAN_WRONG_YEAR_MESSAGE,
            "Aquesta data no és correcta. ¿Us referiu a l'any \"{currentYear}\"?",
            "DateCheckFilter.java:33-35"
        );
    }
}
