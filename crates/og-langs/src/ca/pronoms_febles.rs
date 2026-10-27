use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum PronounPosition {
    Davant,
    DavantApos,
    Darrere,
    DarrereApos,
    DarrereNoguionetNoapos,
    DarreAposNoguionetNoapos,
    Normalized,
}

const PRONOMS_FEBLES: &[&[&str; 7]] = &[
    &["el", "l'", "-lo", "'l", "lo", "l", "el"],
    &[
        "els el", "els l'", "-los-el", "'ls-el", "losel", "lsel", "els el",
    ],
    &[
        "els els", "els els", "-los-els", "'ls-els", "losels", "lsels", "els els",
    ],
    &[
        "els en", "els n'", "-los-en", "'ls-en", "losen", "lsen", "els en",
    ],
    &[
        "els hi", "els hi", "-los-hi", "'ls-hi", "loshi", "lshi", "els hi",
    ],
    &[
        "els ho", "els ho", "-los-ho", "'ls-ho", "losho", "lsho", "els ho",
    ],
    &[
        "els la", "els l'", "-los-la", "'ls-la", "losla", "lsla", "els la",
    ],
    &[
        "els les", "els les", "-los-les", "'ls-les", "losles", "lsles", "els les",
    ],
    &["els", "els", "-los", "'ls", "los", "ls", "els"],
    &["em", "m'", "-me", "'m", "me", "m", "em"],
    &["en", "n'", "-ne", "'n", "ne", "n", "en"],
    &[
        "ens el", "ens l'", "-nos-el", "'ns-el", "nosel", "nsel", "ens el",
    ],
    &[
        "ens els", "ens els", "-nos-els", "'ns-els", "nosels", "nsels", "ens els",
    ],
    &[
        "ens en", "ens n'", "-nos-en", "'ns-en", "nosen", "nsen", "ens en",
    ],
    &[
        "ens hi", "ens hi", "-nos-hi", "'ns-hi", "noshi", "nshi", "ens hi",
    ],
    &[
        "ens ho", "ens ho", "-nos-ho", "'ns-ho", "nosho", "nsho", "ens ho",
    ],
    &[
        "ens la", "ens l'", "-nos-la", "'ns-la", "nosla", "nsla", "ens la",
    ],
    &[
        "ens les", "ens les", "-nos-les", "'ns-les", "nosles", "nsles", "ens les",
    ],
    &[
        "ens li", "ens li", "-nos-li", "'ns-li", "nosli", "nsli", "ens li",
    ],
    &["ens", "ens", "-nos", "'ns", "nos", "ns", "ens"],
    &["es", "s'", "-se", "'s", "se", "s", "es"],
    &["et", "t'", "-te", "'t", "te", "t", "et"],
    &["hi", "hi", "-hi", "-hi", "hi", "hi", "hi"],
    &["ho", "ho", "-ho", "-ho", "ho", "ho", "ho"],
    &["l'en", "el n'", "-l'en", "-l'en", "len", "len", "el en"],
    &["l'hi", "l'hi", "-l'hi", "-l'hi", "lhi", "lhi", "el hi"],
    &[
        "la hi", "la hi", "-la-hi", "-la-hi", "lahi", "lahi", "la hi",
    ],
    &["la", "l'", "-la", "-la", "la", "la", "la"],
    &["la'n", "la n'", "-la'n", "-la'n", "lan", "lan", "la en"],
    &[
        "les en", "les n'", "-les-en", "-les-en", "lesen", "lesen", "les en",
    ],
    &[
        "les hi", "les hi", "-les-hi", "-les-hi", "leshi", "leshi", "les hi",
    ],
    &["les", "les", "-les", "-les", "les", "les", "les"],
    &[
        "li hi", "li hi", "-li-hi", "-li-hi", "lihi", "lihi", "li hi",
    ],
    &[
        "li ho", "li ho", "-li-ho", "-li-ho", "liho", "liho", "li ho",
    ],
    &[
        "li la", "li l'", "-li-la", "-li-la", "lila", "lila", "li la",
    ],
    &[
        "li les", "li les", "-li-les", "-li-les", "liles", "liles", "li les",
    ],
    &["li", "li", "-li", "-li", "li", "li", "li"],
    &["li'l", "li l'", "-li'l", "-li'l", "lil", "lil", "li el"],
    &[
        "li'ls", "li'ls", "-li'ls", "-li'ls", "lils", "lils", "li els",
    ],
    &["li'n", "li n'", "-li'n", "-li'n", "lin", "lin", "li en"],
    &["m'hi", "m'hi", "-m'hi", "-m'hi", "mhi", "mhi", "em hi"],
    &["m'ho", "m'ho", "-m'ho", "-m'ho", "mho", "mho", "em ho"],
    &[
        "me la", "me l'", "-me-la", "-me-la", "mela", "mela", "em la",
    ],
    &[
        "me les", "me les", "-me-les", "-me-les", "meles", "meles", "em les",
    ],
    &[
        "me li", "me li", "-me-li", "-me-li", "meli", "meli", "em li",
    ],
    &["me'l", "me l'", "-me'l", "-me'l", "mel", "mel", "em el"],
    &[
        "me'ls", "me'ls", "-me'ls", "-me'ls", "mels", "mels", "em els",
    ],
    &["me'n", "me n'", "-me'n", "-me'n", "men", "men", "em en"],
    &["n'hi", "n'hi", "-n'hi", "-n'hi", "nhi", "nhi", "en hi"],
    &["s'hi", "s'hi", "-s'hi", "-s'hi", "shi", "shi", "es hi"],
    &["s'ho", "s'ho", "-s'ho", "-s'ho", "sho", "sho", "es ho"],
    &[
        "se la", "se l'", "-se-la", "-se-la", "sela", "sela", "es la",
    ],
    &[
        "se les", "se les", "-se-les", "-se-les", "seles", "seles", "es les",
    ],
    &[
        "se li", "se li", "-se-li", "-se-li", "seli", "seli", "es li",
    ],
    &[
        "se us", "se us", "-se-us", "-se-us", "seus", "seus", "es us",
    ],
    &[
        "se vos", "se vos", "-se-vos", "-se-vos", "sevos", "sevos", "es vos",
    ],
    &["se'l", "se l'", "-se'l", "-se'l", "sel", "sel", "es el"],
    &[
        "se'ls", "se'ls", "-se'ls", "-se'ls", "sels", "sels", "es els",
    ],
    &["se'm", "se m'", "-se'm", "-se'm", "sem", "sem", "es em"],
    &["se'n", "se n'", "-se'n", "-se'n", "sen", "sen", "es en"],
    &[
        "se'ns", "se'ns", "-se'ns", "-se'ns", "sens", "sens", "es ens",
    ],
    &["se't", "se t'", "-se't", "-se't", "set", "set", "es et"],
    &["t'hi", "t'hi", "-t'hi", "-t'hi", "thi", "thi", "et hi"],
    &["t'ho", "t'ho", "-t'ho", "-t'ho", "tho", "tho", "et ho"],
    &[
        "te la", "te l'", "-te-la", "-te-la", "tela", "tela", "et la",
    ],
    &[
        "te les", "te les", "-te-les", "-te-les", "teles", "teles", "et les",
    ],
    &[
        "te li", "te li", "-te-li", "-te-li", "teli", "teli", "et li",
    ],
    &["te'l", "te l'", "-te'l", "-te'l", "tel", "tel", "et el"],
    &[
        "te'ls", "te'ls", "-te'ls", "-te'ls", "tels", "tels", "et els",
    ],
    &["te'm", "te m'", "-te'm", "-te'm", "tem", "tem", "et em"],
    &["te'n", "te n'", "-te'n", "-te'n", "ten", "ten", "et en"],
    &[
        "te'ns", "te'ns", "-te'ns", "-te'ns", "tens", "tens", "et ens",
    ],
    &[
        "us el", "us l'", "-vos-el", "-us-el", "vosel", "usel", "us el",
    ],
    &[
        "us els", "us els", "-vos-els", "-us-els", "vosels", "usels", "us els",
    ],
    &[
        "us em", "us m'", "-vos-em", "-us-em", "vosem", "usem", "us em",
    ],
    &[
        "us en", "us n'", "-vos-en", "-us-en", "vosen", "usen", "us en",
    ],
    &[
        "us ens", "us ens", "-vos-ens", "-us-ens", "vosens", "usens", "us ens",
    ],
    &[
        "us hi", "us hi", "-vos-hi", "-us-hi", "voshi", "ushi", "us hi",
    ],
    &[
        "us ho", "us ho", "-vos-ho", "-us-ho", "vosho", "usho", "us ho",
    ],
    &[
        "us la", "us l'", "-vos-la", "-us-la", "vosla", "usla", "us la",
    ],
    &[
        "us les", "us les", "-vos-les", "-us-les", "vosles", "usles", "us les",
    ],
    &[
        "us li", "us li", "-vos-li", "-us-li", "vosli", "usli", "us li",
    ],
    &["us", "us", "-vos", "-us", "vos", "us", "us"],
    &[
        "se me'n", "se me n'", "-se-me'n", "-se-me'n", "semen", "semen", "es em en",
    ],
    &[
        "se te'n", "se te n'", "-se-te'n", "-se-te'n", "seten", "seten", "es et en",
    ],
    &[
        "se li'n", "se li n'", "-se-li'n", "-se-li'n", "selin", "selin", "es li en",
    ],
    &[
        "se'ns en",
        "se'ns n'",
        "-se'ns-en",
        "-se'ns-en",
        "sensen",
        "sensen",
        "es ens en",
    ],
    &[
        "se us en",
        "se us n'",
        "-se-us-en",
        "-se-us-en",
        "seusen",
        "seusen",
        "es us en",
    ],
    &[
        "se vos en",
        "se vos n'",
        "-se-vos-en",
        "-se-vos-en",
        "sevosen",
        "sevosen",
        "es vos en",
    ],
    &[
        "se'ls en",
        "se'ls n'",
        "-se'ls-en",
        "-se'ls-en",
        "selsen",
        "selsen",
        "es els en",
    ],
];
const INCORRECT_ORDERS: &[(&str, &str)] = &[
    ("me se", "se'm"),
    ("me s'", "se m'"),
    ("te se", "se't"),
    ("te s'", "se t'"),
    ("li se", "se li"),
    ("li s'", "se li"),
    ("mi", "m'hi"),
    ("si", "s'hi"),
    ("nosi", "-nos-hi"),
    ("losi", "-los-hi"),
    ("lis", "els"),
    ("m'en", "me'n"),
    ("t'en", "te'n"),
    ("s'en", "se'n"),
];

#[allow(dead_code)]
static APOSTROPHE_NEEDED: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new("(?i)^h?[aeiou\u{00e0}\u{00e8}\u{00e9}\u{00ed}\u{00f2}\u{00f3}\u{00fa}].*$")
        .unwrap()
});
static APOSTROPHE_NEEDED_END: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("(?i)^.*[aei]$").unwrap());

pub(crate) fn transform(input_pronom: &str, pronoun_pos: PronounPosition) -> String {
    let normalized = incorrect_orders_rewrite(input_pronom.trim());
    let group = PRONOMS_FEBLES
        .iter()
        .find(|row| row.iter().any(|entry| eq_ignore_case(&normalized, entry)));
    let Some(group) = group else {
        return String::new();
    };
    let pronom = group[pronoun_pos as usize];
    let mut out = pronom.to_string();
    if (pronoun_pos == PronounPosition::Davant || pronoun_pos == PronounPosition::DavantApos)
        && !out.ends_with('\'')
    {
        out.push(' ');
    }
    out
}

fn incorrect_orders_rewrite(input: &str) -> String {
    let lower = input.to_lowercase();
    for (wrong, fixed) in INCORRECT_ORDERS {
        if *wrong == lower.as_str() {
            return fixed.to_string();
        }
    }
    lower
}

fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

#[allow(dead_code)]
pub(crate) fn transform_davant(input_pronom: &str, next_word: &str) -> String {
    let next_lower = next_word.to_lowercase();
    if APOSTROPHE_NEEDED.is_match(&next_lower) {
        return transform(input_pronom, PronounPosition::DavantApos);
    }
    let pronom = transform(input_pronom, PronounPosition::Davant);
    if pronom == "es "
        && (next_lower.starts_with('s')
            || next_lower.starts_with("ce")
            || next_lower.starts_with("ci"))
    {
        return "se ".to_string();
    }
    pronom
}

pub(crate) fn transform_darrere(input_pronom: &str, previous_word: &str) -> String {
    if APOSTROPHE_NEEDED_END.is_match(previous_word) {
        transform(input_pronom, PronounPosition::DarrereApos)
    } else {
        transform(input_pronom, PronounPosition::Darrere)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darrere_forms_come_from_the_table() {
        assert_eq!(transform_darrere("n'hi", "ha"), "-n'hi");
        assert_eq!(transform_darrere("n'hi", "anat"), "-n'hi");
        assert_eq!(transform("n'hi", PronounPosition::Darrere), "-n'hi");
        assert_eq!(transform("n'hi", PronounPosition::DarrereApos), "-n'hi");
        assert_eq!(transform("n'hi", PronounPosition::Davant), "n'hi ");
        assert_eq!(transform("n'hi", PronounPosition::Normalized), "en hi");
        assert_eq!(transform("l'", PronounPosition::Darrere), "-lo");
        assert_eq!(transform("els", PronounPosition::Darrere), "-los");
    }

    #[test]
    fn davant_forms_pick_the_apostrophe_variant_before_vowels() {
        assert_eq!(transform_davant("en", "anar"), "n'");
        assert_eq!(transform_davant("en", "menjar"), "en ");
        assert_eq!(transform_davant("es", "seguir"), "se ");
        assert_eq!(transform_davant("es", "cendre"), "se ");
    }

    #[test]
    fn unknown_pronoms_transform_to_the_empty_string() {
        assert_eq!(transform("-t", PronounPosition::Darrere), "");
        assert_eq!(transform("xyz", PronounPosition::Davant), "");
    }

    #[test]
    fn incorrect_orders_are_normalized_before_lookup() {
        assert_eq!(transform("mi", PronounPosition::Davant), "m'hi ");
        assert_eq!(transform("m'en", PronounPosition::Darrere), "-me'n");
    }
}
