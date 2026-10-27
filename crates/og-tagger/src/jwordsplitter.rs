use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::LazyLock;

const WORDS_GERMAN: &str = include_str!("../data/words_german.txt");
const EXCEPTIONS_GERMAN: &str = include_str!("../data/exceptions_german.txt");

const INTERFIXES: [&str; 3] = ["s-", "s", "-"];
const MIN_WORD_LEN: usize = 3;
const MAX_WORD_LEN: usize = 70;

fn dictionary() -> &'static HashSet<&'static str> {
    static WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
        let mut words: HashSet<&'static str> = WORDS_GERMAN
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| l.trim().to_lowercase().leak() as &'static str)
            .collect();
        for word in [
            "influencer",
            "katheterisierung",
            "rücklass",
            "abdichtung",
            "laptop",
            "verschattung",
            "paradeiser",
            "einreichung",
            "bestatter",
            "divergenz",
            "schrumpf",
            "degustation",
            "schaft",
            "abstreifer",
            "glühwürmchen",
            "aufwertung",
            "einhausung",
            "lackier",
            "zarge",
            "pluralisierung",
            "schanzen",
            "abscheide",
            "rangier",
            "temporal",
            "kartonage",
            "kartonagen",
            "rebellion",
            "binokular",
            "umverlegung",
            "umhausung",
            "überholung",
            "chloroplasten",
            "nachrangigkeit",
            "spital",
            "turnus",
            "teilnehmenden",
            "pensionisten",
            "graduierten",
            "beladung",
            "controller",
            "resilienz",
            "mitführ",
            "trauma",
            "abtau",
            "normung",
            "mikroskopie",
            "bitumen",
            "erfolglosigkeit",
            "pneumatik",
            "anlasser",
            "allozierung",
            "alphabetisierung",
            "aktuator",
            "akademisierung",
            "allergiker",
            "queer",
            "filament",
            "querung",
            "curling",
            "opioid",
            "booster",
            "schmuse",
            "thrombozyten",
            "dysfunktion",
            "storchen",
            "nasch",
            "esperanto",
            "passivierung",
            "radikalisierung",
            "erleuchtung",
            "verwalter",
            "verbiss",
            "ausleih",
            "rutsch",
            "kufen",
            "entferner",
            "debitoren",
            "terrakotta",
            "graffiti",
            "auffahr",
            "anmutung",
            "kritzel",
            "salami",
            "eukalyptus",
            "kreativ",
            "hochvolt",
            "trading",
            "extraktion",
            "verstetigung",
            "diagonal",
            "margen",
            "synonym",
            "aufbringung",
            "robustheit",
            "nachuntersuchung",
            "erstkommunion",
            "hauptstadt",
            "neustart",
            "polarisierung",
            "vollstreckbarkeit",
            "vollziehung",
            "kasko",
            "blitzableiter",
            "abschattungen",
            "kuscheltier",
            "gastro",
            "hortensien",
        ] {
            words.insert(word);
        }
        words
    });
    &WORDS
}

type ExceptionMap = HashMap<String, Vec<String>>;

fn parse_exception_lines(map: &mut ExceptionMap, content: &str) {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<String> = line
            .replace("/NS", "")
            .split('|')
            .map(str::to_string)
            .collect();
        let complete_word = line.replace('|', "");
        if complete_word.contains('/') {
            let real_word = complete_word.replace("/NS", "").to_lowercase();
            map.insert(real_word.clone(), parts.clone());
            map.insert(format!("{real_word}n"), append_to_last(&parts, "n"));
            map.insert(format!("{real_word}s"), append_to_last(&parts, "s"));
        } else {
            map.insert(complete_word.to_lowercase(), parts);
        }
    }
}

fn append_to_last(parts: &[String], suffix: &str) -> Vec<String> {
    let mut result = parts.to_vec();
    if let Some(last) = result.last_mut() {
        last.push_str(suffix);
    }
    result
}

fn exceptions() -> &'static ExceptionMap {
    static MAP: LazyLock<ExceptionMap> = LazyLock::new(|| {
        let mut map: ExceptionMap = HashMap::default();
        parse_exception_lines(&mut map, EXCEPTIONS_GERMAN);
        for (word, parts) in TOKENIZER_EXCEPTIONS {
            map.insert(
                word.to_lowercase(),
                parts.iter().map(|p| p.to_string()).collect(),
            );
        }
        map
    });
    &MAP
}

const TOKENIZER_EXCEPTIONS: &[(&str, &[&str])] = &[
    ("Sportshorts", &["Sport", "shorts"]),
    ("Absolventen", &["Absolventen"]),
    ("Acetat", &["Acetat"]),
    ("Alkoholabstinenz", &["Alkohol", "abstinenz"]),
    ("Androgen", &["Androgen"]),
    ("Auberginen", &["Auberginen"]),
    ("Auckland", &["Auckland"]),
    ("Boston", &["Boston"]),
    ("Brandenburg", &["Brandenburg"]),
    ("Broadcast", &["Broadcast"]),
    ("Buchsbaum", &["Buchsbaum"]),
    ("Chiemsee", &["Chiemsee"]),
    ("Coffein", &["Coffein"]),
    ("Drohnen", &["Drohnen"]),
    ("Eiben", &["Eiben"]),
    ("Eingroschen", &["Eingroschen"]),
    ("Einkomponenten", &["Einkomponenten"]),
    ("Elster", &["Elster"]),
    ("Engineering", &["Engineering"]),
    ("Factoring", &["Factoring"]),
    ("Flexodruck", &["Flexo", "druck"]),
    ("Graviton", &["Graviton"]),
    ("Göttinnen", &["Göttinnen"]),
    ("Hallesche", &["Hallesche"]),
    ("Hinspiel", &["Hinspiel"]),
    ("Homogen", &["Homogen"]),
    ("Kolleggen", &["Kolleggen"]),
    ("Karstadt", &["Karstadt"]),
    ("Kartier", &["Kartier"]),
    ("Kaukasus", &["Kaukasus"]),
    ("Knoblauch", &["Knoblauch"]),
    ("Kollagen", &["Kollagen"]),
    ("Kommerz", &["Kommerz"]),
    ("Mentoring", &["Mentoring"]),
    ("Monarchen", &["Monarchen"]),
    ("Oligarchen", &["Oligarchen"]),
    ("Optimal", &["Optimal"]),
    ("Saunieren", &["Saunieren"]),
    ("Schiessen", &["Schiessen"]),
    ("Spielgeleier", &["Spielgeleier"]),
    ("Halleschen", &["Halleschen"]),
    ("Reinigungstab", &["Reinigungs", "tab"]),
    ("Reinigungstabs", &["Reinigungs", "tabs"]),
    ("Tauschwerte", &["Tausch", "werte"]),
    ("Tauschwertes", &["Tausch", "wertes"]),
    ("Kinderspielen", &["Kinder", "spielen"]),
    ("Buchhaltungstrick", &["Buchhaltungs", "trick"]),
    ("Buchhaltungstricks", &["Buchhaltungs", "tricks"]),
    ("Haushaltstrick", &["Haushalts", "trick"]),
    ("Haushaltstricks", &["Haushalts", "tricks"]),
    ("Verkaufstrick", &["Verkaufs", "trick"]),
    ("Verkaufstricks", &["Verkaufs", "tricks"]),
    ("Ablenkungstrick", &["Ablenkungs", "trick"]),
    ("Ablenkungstricks", &["Ablenkungs", "tricks"]),
    ("Manipulationstrick", &["Manipulations", "trick"]),
    ("Manipulationstricks", &["Manipulations", "tricks"]),
    ("Erziehungstrick", &["Erziehungs", "trick"]),
    ("Erziehungstricks", &["Erziehungs", "tricks"]),
    ("Messetage", &["Messe", "tage"]),
    ("Messetagen", &["Messe", "tagen"]),
    ("karamelligen", &["karamelligen"]),
    ("Häkelnadel", &["Häkel", "nadel"]),
    ("Häkelnadeln", &["Häkel", "nadeln"]),
    ("Freiberg", &["Freiberg"]),
    ("Abtestat", &["Abtestat"]),
    ("Abtestaten", &["Abtestaten"]),
    ("Freibergs", &["Freibergs"]),
    ("Kreuzberg", &["Kreuzberg"]),
    ("Kreuzbergs", &["Kreuzbergs"]),
    ("Digitalisierung", &["Digitalisierung"]),
    ("Abtrocknung", &["Abtrocknung"]),
    ("Erlösung", &["Erlösung"]),
    ("Feuerung", &["Feuerung"]),
    ("Aktivierung", &["Aktivierung"]),
    ("Protokollierung", &["Protokollierung"]),
    ("Budgetierung", &["Budgetierung"]),
    ("Faltung", &["Faltung"]),
    ("Anhäufung", &["Anhäufung"]),
    ("Aufkohlung", &["Aufkohlung"]),
    ("Festigung", &["Festigung"]),
    ("Allerheiligen", &["Allerheiligen"]),
    ("Druckerpressen", &["Drucker", "pressen"]),
    ("Habitat", &["Habitat"]),
    ("Augarten", &["Augarten"]),
    ("Auszeit", &["Auszeit"]),
    ("Bewegtbild", &["Bewegt", "bild"]),
    ("Bigband", &["Bigband"]),
    ("Bisexuelle", &["Bisexuelle"]),
    ("Bisexuellen", &["Bisexuellen"]),
    ("Bunsenbrenner", &["Bunsenbrenner"]),
    ("Carbon", &["Carbon"]),
    ("Carsharing", &["Carsharing"]),
    ("Castor", &["Castor"]),
    ("Catering", &["Catering"]),
    ("Cholesterin", &["Cholesterin"]),
    ("Damast", &["Damast"]),
    ("Dispositiv", &["Dispositiv"]),
    ("Emittent", &["Emittent"]),
    ("Emittenten", &["Emittenten"]),
    ("Express", &["Express"]),
    ("Fairness", &["Fairness"]),
    ("Fiberglas", &["Fiberglas"]),
    ("Globus", &["Globus"]),
    ("Göttinnen", &["Göttinnen"]),
    ("Illustration", &["Illustration"]),
    ("Muttertag", &["Muttertag"]),
    ("Muttertags", &["Muttertags"]),
    ("Patriarchen", &["Patriarchen"]),
    ("Phosgen", &["Phosgen"]),
    ("Vatertag", &["Vatertag"]),
    ("Vatertags", &["Vatertags"]),
    ("Vaterland", &["Vaterland"]),
    ("Vaterlands", &["Vaterlands"]),
    ("Wehrmacht", &["Wehrmacht"]),
    ("Wehrmachts", &["Wehrmachts"]),
];

fn exception_split(word: &str) -> Option<Vec<String>> {
    let result = exceptions().get(&word.to_lowercase())?;
    let joined: String = result.concat();
    if word.to_lowercase() == joined.to_lowercase() {
        let mut offset = 0;
        let pieces: Option<Vec<String>> = result
            .iter()
            .map(|p| {
                let piece = word.get(offset..offset + p.len())?.to_string();
                offset += p.len();
                Some(piece)
            })
            .collect();
        return pieces.or_else(|| Some(result.clone()));
    }
    Some(result.clone())
}

fn is_simple_word(part: &str) -> bool {
    part.chars().count() >= MIN_WORD_LEN && dictionary().contains(part.to_lowercase().as_str())
}

fn find_interfix(lc_word: &str) -> Option<&'static str> {
    INTERFIXES.iter().copied().find(|i| lc_word.ends_with(i))
}

fn ends_with_interfix(lc_word: &str) -> bool {
    INTERFIXES.iter().any(|i| lc_word.ends_with(i))
}

fn split(word: &str, allow_interfix_removal: bool, strict: bool) -> Option<Vec<String>> {
    if let Some(parts) = exception_split(word) {
        return Some(parts);
    }
    let lc_word = word.to_lowercase();
    let removable_interfix = find_interfix(&lc_word);
    let word_without_interfix = match removable_interfix {
        Some(i) => &word[..word.len() - i.len()],
        None => word,
    };
    let can_interfix_be_removed = removable_interfix.is_some() && allow_interfix_removal;

    if is_simple_word(word) {
        Some(vec![word.to_string()])
    } else if can_interfix_be_removed && is_simple_word(word_without_interfix) {
        Some(vec![
            word_without_interfix.to_string(),
            removable_interfix.expect("checked").to_string(),
        ])
    } else {
        let mut parts = split_from_right(word, strict);
        if parts.is_none() && is_simple_word(word) {
            parts = Some(vec![word.to_string()]);
        } else if let Some(ref mut p) = parts
            && is_simple_word(word)
            && !p.iter().any(|x| x == word)
        {
            p.push(word.to_string());
        }
        if parts.is_none()
            && ends_with_interfix(&lc_word)
            && let Some(mut inner) = split_from_right(word_without_interfix, strict)
        {
            if let Some(interfix) = removable_interfix {
                inner.push(interfix.to_string());
            }
            parts = Some(inner);
        }
        parts
    }
}

fn combined_exception_split(left_part: &str, right_part: &str) -> Option<Vec<String>> {
    if let Some(right_parts) = exceptions().get(&right_part.to_lowercase()) {
        let mut parts = vec![left_part.to_string()];
        parts.extend(right_parts.iter().cloned());
        return Some(parts);
    }
    let left_parts = exceptions().get(&left_part.to_lowercase())?;
    let mut parts = left_parts.clone();
    parts.push(right_part.to_string());
    Some(parts)
}

fn split_from_right(word: &str, strict: bool) -> Option<Vec<String>> {
    let chars: Vec<char> = word.chars().collect();
    let mut parts: Option<Vec<String>> = None;
    let mut i = chars.len().saturating_sub(MIN_WORD_LEN);
    while i >= MIN_WORD_LEN {
        let left_part: String = chars[..i].iter().collect();
        let right_part: String = chars[i..].iter().collect();
        if is_simple_word(&right_part) {
            if let Some(mut left_parts) = split(&left_part, true, strict) {
                left_parts.push(right_part);
                parts = Some(left_parts);
            } else if !strict {
                parts = Some(vec![left_part, right_part]);
            }
        } else if !strict {
            if let Some(exception_parts) = combined_exception_split(&left_part, &right_part) {
                parts = Some(exception_parts);
            } else if is_simple_word(&left_part) {
                parts = Some(vec![left_part, right_part]);
            }
        }
        i -= 1;
    }
    parts
}

fn disambiguate(parts: Vec<String>) -> Vec<String> {
    let mut new_parts = parts;
    let last_idx = new_parts.len().saturating_sub(1);
    if new_parts.len() > 1 {
        let last_part = new_parts[last_idx].clone();
        if last_part == "samt" || last_part == "samts" || last_part == "samtes" {
            let prev = new_parts[last_idx - 1].clone();
            new_parts[last_idx - 1] = format!("{prev}s");
            new_parts[last_idx] = last_part
                .strip_prefix('s')
                .unwrap_or(&last_part)
                .to_string();
            return new_parts;
        }
    }
    if new_parts.is_empty() {
        return new_parts;
    }
    let mut i = new_parts.len() - 1;
    while i >= 2 {
        let part = new_parts[i].clone();
        let prev_part = new_parts[i - 1].clone();
        let prev_prev_part = new_parts[i - 2].clone();
        if prev_part == "s" {
            let part_is_word = dictionary().contains(format!("s{part}").to_lowercase().as_str());
            if !part_is_word || prev_prev_part == "Verhalten" {
                new_parts[i - 2] = format!("{prev_prev_part}s");
                new_parts.remove(i - 1);
            }
        }
        i -= 1;
    }
    new_parts
}

pub fn tokenize(word: &str) -> Vec<String> {
    tokenize_with_mode(word, true)
}

pub fn tokenize_with_mode(word: &str, strict: bool) -> Vec<String> {
    if word.chars().count() > MAX_WORD_LEN {
        return vec![word.to_string()];
    }
    let trimmed = word.trim();
    if let Some(exc) = exception_split(trimmed) {
        return exc;
    }
    let Some(parts) = split(trimmed, false, strict) else {
        return vec![trimmed.to_string()];
    };
    let mut parts = disambiguate(parts);
    for part in &mut parts {
        if let Some(stripped) = part.strip_prefix('-') {
            *part = stripped.to_string();
        }
        if let Some(stripped) = part.strip_suffix('-') {
            *part = stripped.to_string();
        }
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_word_is_not_split() {
        assert_eq!(tokenize("Abgeordneter"), vec!["Abgeordneter"]);
        assert_eq!(tokenize("hoch"), vec!["hoch"]);
    }

    #[test]
    fn unknown_word_without_valid_split_stays_whole() {
        assert_eq!(tokenize("Obdachloser"), vec!["Obdachloser"]);
        assert_eq!(tokenize("Wicklerer"), vec!["Wicklerer"]);
    }

    #[test]
    fn compounds_split_into_known_parts() {
        let parts = tokenize("Kirschbaum");
        assert_eq!(parts.len(), 2);
        assert!(is_simple_word(&parts[0]) && is_simple_word(&parts[1]));
        assert_eq!(parts.concat(), "Kirschbaum");
    }

    #[test]
    fn exception_split_wins() {
        assert_eq!(tokenize("Pilotsendung"), vec!["Pilot", "sendung"]);
        assert_eq!(tokenize("Buchhaltungstrick"), vec!["Buchhaltungs", "trick"]);
    }

    #[test]
    fn non_strict_mode_falls_back_to_partial_splits() {
        assert_eq!(tokenize_with_mode("xyzhaus", false), vec!["xyz", "haus"]);
        assert_eq!(tokenize_with_mode("xyzhaus", true), vec!["xyzhaus"]);
        assert_eq!(
            tokenize_with_mode("Buchhaltungstrick", false),
            vec!["Buchhaltungs", "trick"]
        );
    }

    #[test]
    fn non_strict_mode_uses_exception_pieces_for_unknown_sides() {
        assert_eq!(
            tokenize_with_mode("stubepilotsendung", false),
            vec!["stube", "Pilot", "sendung"]
        );
        assert_eq!(
            tokenize_with_mode("xyzcomicstrip", false),
            vec!["xyz", "Comic", "strip"]
        );
        assert_eq!(
            tokenize_with_mode("Bruttorentenniveauxyz", false),
            vec!["Brut", "torentenniveauxyz"]
        );
    }

    #[test]
    fn samt_compounds_move_the_linking_s_to_the_left_part() {
        assert_eq!(tokenize("baumsamt"), vec!["baums", "amt"]);
        assert_eq!(tokenize("Haussamtes"), vec!["Hauss", "amtes"]);
        assert_eq!(tokenize("Süßigkeitensamts"), vec!["Süßigkeitens", "amts"]);
    }

    #[test]
    fn trailing_hyphen_becomes_its_own_stripped_piece() {
        assert_eq!(tokenize("Obdachlosen-"), vec!["Obdach", "losen", ""]);
    }

    #[test]
    fn oversized_words_are_returned_whole() {
        let long = "a".repeat(71);
        assert_eq!(tokenize(&long), vec![long.clone()]);
        assert_eq!(tokenize_with_mode(&long, false), vec![long]);
    }
}
