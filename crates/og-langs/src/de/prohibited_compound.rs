use crate::de::helpers::has_pos_tag_starting_with;
use crate::de::helpers::is_pos_tag_unknown;
use crate::de::helpers::is_tagged;
use crate::de::helpers::lowercase_first_char;
use crate::de::helpers::starts_with_uppercase;
use crate::de::helpers::uppercase_first_char;
use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use og_languagemodel::LanguageModel;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::OnceLock;

#[derive(Clone)]
struct Pair {
    part1: String,
    part1_desc: Option<String>,
    part2: String,
    part2_desc: Option<String>,
}

impl Pair {
    fn new(part1: &str, part1_desc: Option<&str>, part2: &str, part2_desc: Option<&str>) -> Self {
        Self {
            part1: part1.to_string(),
            part1_desc: part1_desc.map(str::to_string),
            part2: part2.to_string(),
            part2_desc: part2_desc.map(str::to_string),
        }
    }
}

const IGNORE_WORDS: [&str; 2] = ["Die", "De"];

const LOWERCASE_PAIRS: [(&str, Option<&str>, &str, Option<&str>); 79] = [
    (
        "knoten",
        Some("Verschlingung von Fäden"),
        "konten",
        Some("Plural von 'Konto'"),
    ),
    (
        "schaf",
        Some("Tier"),
        "schaft",
        Some("'-schaft' (Element zur Wortbildung, z. B. 'Freundschaft')"),
    ),
    (
        "schafen",
        Some("Dativ Plural von 'Schaf'"),
        "schaften",
        Some("'-schaften' (Element zur Wortbildung, z. B. 'Freundschaften')"),
    ),
    (
        "alpen",
        Some("Hochgebirge in Mittel- und Südeuropa"),
        "alben",
        Some("Plural von 'Album'"),
    ),
    (
        "pillen",
        Some("Tabletten"),
        "pullen",
        Some("Plural von 'Pulle' (Flasche)"),
    ),
    ("tauben", Some("Vogelart"), "trauben", Some("Obstsorte")),
    (
        "panel",
        Some("ausgewählte Personengruppe"),
        "paneel",
        Some("Platte für Wand- und Deckenverkleidungen"),
    ),
    (
        "nabe",
        Some("Mittelteil eines Rades"),
        "narbe",
        Some("verheilende Wunde"),
    ),
    (
        "first",
        Some("höchste Kante an einem geneigten Dach"),
        "frist",
        Some("spätester Zeitpunkt"),
    ),
    (
        "koma",
        Some("Zustand tiefer Bewusstlosigkeit"),
        "komma",
        Some("Satzzeichen"),
    ),
    (
        "korn",
        Some("Getreide sowie dessen Frucht"),
        "kron",
        Some("Vorsilbe z.B. in 'Kronkorken'"),
    ),
    (
        "bauten",
        Some("Form von 'Bau' (Bauwerk, Haus, ...)"),
        "beuten",
        Some("Form von 'Beute'"),
    ),
    (
        "file",
        Some("engl. 'Datei'"),
        "filet",
        Some("ein Stück Fleisch oder Fisch"),
    ),
    (
        "zecke",
        Some("blutsaugender Parasit"),
        "zwecke",
        Some("Dativ von 'Zweck' (Ziel)"),
    ),
    (
        "frucht",
        Some("Teil einer Pflanze; Obst"),
        "furcht",
        Some("Angst"),
    ),
    (
        "rate",
        Some("Verhältnis zwischen zwei Größen"),
        "ratte",
        Some("Nagetier"),
    ),
    (
        "posten",
        Some("Arbeitsplatz, Wachposten"),
        "posen",
        Some("Pose: betonte Körperhaltung"),
    ),
    (
        "himmel",
        Some("Bereich über der Erde"),
        "hummel",
        Some("Insekt"),
    ),
    (
        "server",
        Some("Computer"),
        "servier",
        Some("zu 'servieren'"),
    ),
    ("ziege", Some("Tier"), "ziegel", Some("Ziegelstein")),
    (
        "robe",
        Some("Kleidungsstück"),
        "probe",
        Some("Test, Kontrolle"),
    ),
    (
        "mode",
        Some("Kleidung"),
        "monde",
        Some("Begleiter eines Planeten"),
    ),
    (
        "eigen",
        Some("'selbst', z.B. 'Eigenzitat'"),
        "eingen",
        Some("Möglicher Tippfehler"),
    ),
    (
        "stümpfe",
        Some("Rest eines Körpergliedes"),
        "strümpfe",
        Some("Bekleidungsstück für den Fuß"),
    ),
    (
        "gelände",
        Some("Gebiet"),
        "geländer",
        Some("Konstruktion zum Festhalten entlang von Treppen"),
    ),
    (
        "tropen",
        Some("feuchtwarme Gebiete am Äquator"),
        "tropfen",
        Some("kleine Menge Flüssigkeit"),
    ),
    (
        "enge",
        Some("Mangel an Platz"),
        "menge",
        Some("Anzahl an Einheiten"),
    ),
    (
        "ritt",
        Some("Reiten"),
        "tritt",
        Some("Aufsetzen eines Fußes"),
    ),
    ("beine", Some("Körperteil"), "biene", Some("Insekt")),
    ("rebe", Some("Weinrebe"), "reibe", Some("Küchenreibe")),
    (
        "ass",
        Some("Spielkarte"),
        "pass",
        Some("Reisepass; Übergang durch ein Gebirge"),
    ),
    (
        "türmer",
        Some("Turmwächter"),
        "türme",
        Some("Plural von 'Turm' (Bauwerk)"),
    ),
    (
        "soge",
        Some("ziehende Strömungen"),
        "sorge",
        Some("bedrückendes Gefühl"),
    ),
    (
        "panne",
        Some("technischer Defekt"),
        "spanne",
        Some("Zeitraum"),
    ),
    (
        "elfer",
        Some("Elfmeter"),
        "helfer",
        Some("Person, die hilft"),
    ),
    (
        "gase",
        Some("Plural von 'Gas' (Aggregatzustand)"),
        "gasse",
        Some("kleine Straße"),
    ),
    (
        "ekel",
        Some("Abscheu"),
        "enkel",
        Some("Kind eines eigenen Kindes"),
    ),
    (
        "reis",
        Some("Nahrungsmittel"),
        "reise",
        Some("Ausflug/Fahrt"),
    ),
    (
        "hüte",
        Some("Kopfbedeckungen"),
        "häute",
        Some("Plural von 'Haut'"),
    ),
    ("werbereich", None, "erbereich", None),
    (
        "lage",
        Some("Position"),
        "alge",
        Some("im Wasser lebende Organismen"),
    ),
    ("sphäre", Some("Kugel"), "spähreh", None),
    (
        "schenke",
        Some("Gastwirtschaft (auch: Schänke)"),
        "schenkel",
        Some("Ober- und Unterschenkel"),
    ),
    ("mai", Some("Monat nach April"), "mail", Some("E-Mail")),
    (
        "pump",
        Some("'auf Pump': umgangssprachlich für 'auf Kredit'"),
        "pumpe",
        Some("Gerät zur Beförderung von Flüssigkeiten"),
    ),
    (
        "mitte",
        Some("zentral"),
        "mittel",
        Some("Methode, um etwas zu erreichen"),
    ),
    (
        "fein",
        Some("feinkörnig, genau, gut"),
        "feind",
        Some("Gegner"),
    ),
    (
        "traum",
        Some("Erleben während des Schlafes"),
        "trauma",
        Some("Verletzung"),
    ),
    (
        "name",
        Some("Bezeichnung (z.B. 'Vorname')"),
        "nahme",
        Some("zu 'nehmen' (z.B. 'Teilnahme')"),
    ),
    (
        "bart",
        Some("Haarbewuchs im Gesicht"),
        "dart",
        Some("Wurfpfeil"),
    ),
    ("hart", Some("fest"), "dart", Some("Wurfpfeil")),
    (
        "speiche",
        Some("Verbindung zwischen Nabe und Felge beim Rad"),
        "speicher",
        Some("Lagerraum"),
    ),
    (
        "kart",
        Some("Gokart (Fahrzeug)"),
        "karte",
        Some("Fahrkarte, Postkarte, Landkarte, ..."),
    ),
    (
        "karts",
        Some("Kart = Gokart (Fahrzeug)"),
        "karte",
        Some("Fahrkarte, Postkarte, Landkarte, ..."),
    ),
    (
        "kiefer",
        Some("knöcherner Teil des Schädels"),
        "kiefern",
        Some("Kieferngewächse (Baum)"),
    ),
    (
        "gel",
        Some("dickflüssige Masse"),
        "geld",
        Some("Zahlungsmittel"),
    ),
    (
        "kamp",
        Some("Flurname für ein Stück Land"),
        "kampf",
        Some("Auseinandersetzung"),
    ),
    ("läuse", Some("Insekt"), "läufe", Some("Bewegungsart")),
    ("läusen", Some("Insekt"), "läufen", Some("Bewegungsart")),
    (
        "ruck",
        Some("plötzliche Bewegung"),
        "druck",
        Some("Belastung"),
    ),
    (
        "brüste",
        Some("Plural von Brust"),
        "bürste",
        Some("Gerät mit Borsten, z.B. zum Reinigen"),
    ),
    (
        "attraktion",
        Some("Sehenswürdigkeit"),
        "akttaktion",
        Some("vermutlicher Tippfehler"),
    ),
    ("turn", Some("zu 'turnen'"), "turm", Some("hohes Bauwerk")),
    (
        "mit",
        Some("Präposition"),
        "miet",
        Some("zu 'Miete' (Überlassung gegen Bezahlung)"),
    ),
    (
        "bart",
        Some("Behaarung im Gesicht"),
        "brat",
        Some("zu 'braten', z.B. 'Bratkartoffel'"),
    ),
    (
        "uhr",
        Some("Instrument zur Zeitmessung"),
        "ur",
        Some("ursprünglich"),
    ),
    (
        "abschluss",
        Some("Ende"),
        "abschuss",
        Some("Vorgang des Abschießens, z.B. mit einer Waffe"),
    ),
    (
        "brache",
        Some("verlassenes Grundstück"),
        "branche",
        Some("Wirtschaftszweig"),
    ),
    (
        "wieder",
        Some("erneut, wiederholt, nochmal (Wiederholung, Wiedervorlage, ...)"),
        "wider",
        Some("gegen, entgegen (Widerwille, Widerstand, Widerspruch, ...)"),
    ),
    (
        "leer",
        Some("ohne Inhalt"),
        "lehr",
        Some("bezogen auf Ausbildung und Wissen"),
    ),
    (
        "gewerbe",
        Some("wirtschaftliche Tätigkeit"),
        "gewebe",
        Some("gewebter Stoff; Verbund ähnlicher Zellen"),
    ),
    (
        "klima",
        Some("langfristige Wetterzustände"),
        "lima",
        Some("Hauptstadt von Peru"),
    ),
    (
        "modell",
        Some("vereinfachtes Abbild der Wirklichkeit"),
        "model",
        Some("Fotomodell"),
    ),
    (
        "häufigkeit",
        Some("Anzahl von Ereignissen"),
        "häutigkeit",
        Some("z.B. in Dunkelhäutigkeit"),
    ),
    (
        "hin",
        Some("in Richtung"),
        "hirn",
        Some("Gehirn, Denkapparat"),
    ),
    (
        "verklärung",
        Some("Beschönigung, Darstellung in einem besseren Licht"),
        "erklärung",
        Some("Darstellung, Erläuterung"),
    ),
    ("punk", Some("Jugendkultur"), "punkt", Some("Satzzeichen")),
    (
        "haft",
        Some("Freiheitsentzug"),
        "schaft",
        Some("-schaft (Element zur Wortbildung)"),
    ),
    (
        "stande",
        Some("zu 'Stand'"),
        "stange",
        Some("länglicher Gegenstand"),
    ),
];

const BLACKLIST_REGEX: &[&str] = &[
    r"Lande(basis|basen|region|gebiets?|gebieten?|regionen|betriebs?|betrieben?|offizieren?|bereichs?|bereichen?|einrichtung|einrichtungen|massen?|plans?|versuchs?|versuchen?)",
    r"Model(vertrags?|verträgen?|erfahrung|erfahrungen|szene|welt)",
    r"(Raum|Surf|Jazz|Herbst|Gymnastik|Normal)schuhen?",
    r"preis",
    r"reisähnlich(e|e[nmrs])?",
    r"neugestartet(e|e[nmrs])?",
    r"reisender",
    r"[a-zöäüß]+sender",
    r"gra(ph|f)ische?",
    r"gra(ph|f)ische[rsnm]",
    r"gra(ph|f)s?$",
    r"gra(ph|f)en",
    r"gra(ph|f)in",
    r"gra(ph|f)ik",
    r"gra(ph|f)ie",
    r"Gra(ph|f)its?",
    r".+gra(ph|f)its?",
];

struct SearchIndex {
    pairs: Vec<Pair>,
    by_key: HashMap<Box<str>, Vec<u32>>,
    blacklist_regex: Vec<regex::Regex>,
}

fn build_search_index() -> SearchIndex {
    let mut pairs: Vec<Pair> = Vec::new();
    for (p1, d1, p2, d2) in LOWERCASE_PAIRS {
        pairs.push(Pair::new(p1, d1, p2, d2));
        let uc1 = uppercase_first_char(p1);
        let uc2 = uppercase_first_char(p2);
        if uc1 != p1 || uc2 != p2 {
            pairs.push(Pair::new(&uc1, d1, &uc2, d2));
        }
    }
    for (s1, d1, s2, d2) in
        crate::confusion_probability::confusion_term_pairs("de", &["confusion_sets.txt"])
    {
        let all_upper = starts_with_uppercase(&s1)
            && starts_with_uppercase(&s2)
            && !IGNORE_WORDS.contains(&s1.as_str())
            && !IGNORE_WORDS.contains(&s2.as_str());
        if !all_upper {
            continue;
        }
        pairs.push(Pair::new(&s1, d1.as_deref(), &s2, d2.as_deref()));
        pairs.push(Pair::new(
            &lowercase_first_char(&s1),
            d1.as_deref(),
            &lowercase_first_char(&s2),
            d2.as_deref(),
        ));
    }
    let mut by_key: HashMap<Box<str>, Vec<u32>> = HashMap::new();
    for (idx, pair) in pairs.iter().enumerate() {
        by_key
            .entry(pair.part1.as_str().into())
            .or_default()
            .push(idx as u32);
        by_key
            .entry(pair.part2.as_str().into())
            .or_default()
            .push(idx as u32);
    }
    let blacklist_regex = BLACKLIST_REGEX
        .iter()
        .filter_map(|p| regex::Regex::new(p).ok())
        .collect();
    SearchIndex {
        pairs,
        by_key,
        blacklist_regex,
    }
}

static SEARCH_INDEX: LazyLock<SearchIndex> = LazyLock::new(build_search_index);

fn blacklist() -> &'static HashSet<String> {
    static BLACKLIST: OnceLock<HashSet<String>> = OnceLock::new();
    BLACKLIST.get_or_init(|| {
        let mut words = HashSet::new();
        if let Some(root) = crate::engine::find_lt_resource_dir() {
            let path = root
                .join("de/src/main/resources/org/languagetool/resource/de/compound_exceptions.txt");
            if let Ok(data) = std::fs::read_to_string(path) {
                for line in data.lines() {
                    if line.starts_with('#') {
                        continue;
                    }
                    let word = line.split('#').next().unwrap_or("").trim();
                    if !word.is_empty() {
                        words.insert(word.to_string());
                    }
                }
            }
        }
        words
    })
}

fn herrn_frau() -> &'static regex::Regex {
    static RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^(?:Herrn?|Frau|Dr|Prof|Mag|Hr|Fr|Mr|Mrs|Ms|Fräulein)$").unwrap()
    });
    &RE
}

fn to_id(input: &str) -> String {
    let upper = input.to_uppercase().replace(' ', "_").replace('\'', "_Q_");
    let upper = upper
        .replace('Ä', "AE")
        .replace('Ü', "UE")
        .replace('Ö', "OE");
    upper
        .chars()
        .map(|c| {
            let is_allowed = c.is_ascii_uppercase()
                || ('\u{00C0}'..='\u{00D6}').contains(&c)
                || ('\u{00D8}'..='\u{00DE}').contains(&c);
            if is_allowed { c } else { '_' }
        })
        .collect()
}

fn has_partial_pos_tag(token: &AnalyzedTokenReadings, needle: &str) -> bool {
    token
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.contains(needle)))
}

fn remove_hyphens_and_adapt_case(word: &str) -> Option<String> {
    let parts: Vec<&str> = word.split('-').collect();
    if parts.len() > 1 {
        let mut out = String::new();
        for (i, part) in parts.iter().enumerate() {
            if part.chars().count() <= 1 {
                return None;
            }
            if i == 0 {
                out.push_str(part);
            } else {
                out.push_str(&lowercase_first_char(part));
            }
        }
        return Some(out);
    }
    None
}

pub struct ProhibitedCompoundRule {
    model: Arc<dyn LanguageModel>,
    speller: Option<Arc<og_spell::GermanSpeller>>,
}

impl ProhibitedCompoundRule {
    pub fn new(
        model: Arc<dyn LanguageModel>,
        speller: Option<Arc<og_spell::GermanSpeller>>,
    ) -> Self {
        Self { model, speller }
    }

    fn is_misspelled(&self, word: &str) -> bool {
        let Some(speller) = &self.speller else {
            return false;
        };
        speller.is_misspelled_strict(word)
    }

    fn candidate_pairs(&self, word_part: &str) -> Vec<u32> {
        let mut hits: Vec<u32> = Vec::new();
        for (key, indices) in &SEARCH_INDEX.by_key {
            if word_part.contains(key.as_ref()) {
                hits.extend_from_slice(indices);
            }
        }
        hits.sort_unstable();
        hits.dedup();
        hits
    }

    fn get_matches(
        &self,
        sentence: &AnalyzedSentence,
        readings: &AnalyzedTokenReadings,
        parts_start_pos: usize,
        word_part: &str,
        to_pos_correction: usize,
        rule_matches: &mut Vec<RuleMatch>,
    ) -> usize {
        let word_len = word_part.chars().count();
        let gated = (is_tagged(readings)
            && !has_partial_pos_tag(readings, "SUB")
            && !has_pos_tag_starting_with(readings, "EIG:"))
            || word_len <= 6;
        if gated {
            return parts_start_pos + word_len + 1;
        }
        struct Weighted {
            weight: u64,
            rule_id: String,
            description: String,
            message: String,
            offset: usize,
            length: usize,
            replacement: String,
        }
        let mut weighted: Vec<Weighted> = Vec::new();
        for idx in self.candidate_pairs(word_part) {
            let pair = &SEARCH_INDEX.pairs[idx as usize];
            let variant = if let Some(pos) = word_part.find(&pair.part1) {
                let mut v = String::new();
                v.push_str(&word_part[..pos]);
                v.push_str(&pair.part2);
                v.push_str(&word_part[pos + pair.part1.len()..]);
                v
            } else if let Some(pos) = word_part.find(&pair.part2) {
                let mut v = String::new();
                v.push_str(&word_part[..pos]);
                v.push_str(&pair.part1);
                v.push_str(&word_part[pos + pair.part2.len()..]);
                v
            } else {
                continue;
            };
            let word_count = self.model.get_count(&[word_part]);
            let variant_count = self.model.get_count(&[&variant]);
            if variant_count > 0
                && word_count == 0
                && !blacklist().contains(word_part)
                && !self.is_misspelled(&variant)
                && !SEARCH_INDEX
                    .blacklist_regex
                    .iter()
                    .any(|re| re.is_match(word_part))
            {
                let message = match (&pair.part1_desc, &pair.part2_desc) {
                    (Some(d1), Some(d2)) => format!(
                        "Möglicher Tippfehler. {}: {}, {}: {}",
                        uppercase_first_char(&pair.part1),
                        d1,
                        uppercase_first_char(&pair.part2),
                        d2
                    ),
                    _ => format!("Möglicher Tippfehler: {}/{}", pair.part1, pair.part2),
                };
                let from_pos = readings.token().start() + parts_start_pos;
                let to_pos = from_pos + word_part.len() + to_pos_correction;
                let rule_id = to_id(&format!(
                    "DE_PROHIBITED_COMPOUNDS_{}_{}",
                    pair.part1, pair.part2
                ));
                let description = format!(
                    "Markiert wahrscheinlich falsche Komposita mit Teilwort '{}' statt '{}' und umgekehrt",
                    uppercase_first_char(&pair.part1),
                    uppercase_first_char(&pair.part2)
                );
                weighted.push(Weighted {
                    weight: variant_count,
                    rule_id,
                    description,
                    message,
                    offset: from_pos,
                    length: to_pos - from_pos,
                    replacement: variant,
                });
            }
        }
        if let Some(best) = weighted.into_iter().max_by_key(|w| w.weight) {
            let rm_rule = RuleMatchRule::new(best.rule_id, best.description)
                .with_category(Category::new("TYPOS", "Mögliche Tippfehler"))
                .with_issue_type(IssueType::Misspelling.as_str());
            let context_start = best.offset.saturating_sub(30);
            let context_end = std::cmp::min(best.offset + best.length + 30, sentence.text().len());
            let context_text = sentence
                .text()
                .get(context_start..context_end)
                .unwrap_or("")
                .to_string();
            rule_matches.push(
                RuleMatch::new(
                    best.message,
                    best.offset,
                    best.length,
                    rm_rule,
                    RuleMatchContext::new(context_text, context_start, context_end - context_start),
                )
                .with_sentence(sentence.text().to_string())
                .with_replacements(vec![SuggestedReplacement::new(best.replacement)]),
            );
        }
        parts_start_pos + word_len + 1
    }
}

impl Rule for ProhibitedCompoundRule {
    fn id(&self) -> &str {
        "DE_PROHIBITED_COMPOUNDS"
    }

    fn description(&self) -> &str {
        "Markiert wahrscheinlich falsche Komposita wie 'Lehrzeile', wenn 'Leerzeile' häufiger vorkommt."
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Mögliche Tippfehler")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let tokens = sentence.non_whitespace_tokens();
        let mut prev: Option<&AnalyzedTokenReadings> = None;
        for readings in tokens {
            let word = readings.token().token();
            if let Some(prev_readings) = prev {
                if has_partial_pos_tag(prev_readings, "EIG:")
                    && starts_with_uppercase(word)
                    && (has_partial_pos_tag(readings, "EIG:") || is_pos_tag_unknown(readings))
                {
                    continue;
                }
                if herrn_frau().is_match(prev_readings.token().token()) {
                    continue;
                }
            }
            let mut parts_start_pos = 0usize;
            for part in word.split('-') {
                parts_start_pos = self.get_matches(
                    sentence,
                    readings,
                    parts_start_pos,
                    part,
                    0,
                    &mut rule_matches,
                );
            }
            if let Some(no_hyphens) = remove_hyphens_and_adapt_case(word) {
                let correction = word.chars().count() - no_hyphens.chars().count();
                self.get_matches(
                    sentence,
                    readings,
                    0,
                    &no_hyphens,
                    correction,
                    &mut rule_matches,
                );
            }
            prev = Some(readings);
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_matches(text: &str) -> Vec<(String, usize, String)> {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let mut request = og_core::CheckRequest::new(text, og_core::Language::German);
        request.enabled_rules = vec!["DE_PROHIBITED_COMPOUNDS".to_string()];
        engine
            .check_with_request(&request)
            .matches
            .iter()
            .filter(|m| m.rule().id().starts_with("DE_PROHIBITED_COMPOUNDS"))
            .map(|m| {
                (
                    m.rule().id().to_string(),
                    m.offset(),
                    m.replacements()
                        .first()
                        .map(|r| r.value().to_string())
                        .unwrap_or_default(),
                )
            })
            .collect()
    }

    fn real_model_available() -> bool {
        og_languagemodel::DiskLanguageModel::shared_for_language("de").is_some()
    }

    #[test]
    fn fires_on_lehrzeile_with_real_model() {
        if !real_model_available() {
            eprintln!("Skipping: extracted German ngram data not present");
            return;
        }
        let matches = engine_matches("Da steht eine Lehrzeile zu viel.");
        assert!(
            matches
                .iter()
                .any(|(id, _, sugg)| id == "DE_PROHIBITED_COMPOUNDS_LEER_LEHR"
                    && sugg == "Leerzeile"),
            "expected Lehrzeile -> Leerzeile, got {matches:?}"
        );
    }

    #[test]
    fn does_not_fire_on_correct_leerzeile() {
        if !real_model_available() {
            eprintln!("Skipping: extracted German ngram data not present");
            return;
        }
        let matches = engine_matches("Da steht eine Leerzeile zu viel.");
        assert!(
            matches.is_empty(),
            "Leerzeile must not be flagged: {matches:?}"
        );
    }

    #[test]
    fn to_id_matches_java_normalization() {
        assert_eq!(
            to_id("DE_PROHIBITED_COMPOUNDS_wieder_wider"),
            "DE_PROHIBITED_COMPOUNDS_WIEDER_WIDER"
        );
        assert_eq!(to_id("häu_fig"), "HAEU_FIG");
    }

    #[test]
    fn remove_hyphens_adapts_case() {
        assert_eq!(
            remove_hyphens_and_adapt_case("S-Bahn"),
            None,
            "single-letter part must abort"
        );
        assert_eq!(
            remove_hyphens_and_adapt_case("GmbH-Hauptsitz"),
            Some("GmbHhauptsitz".to_string())
        );
        assert_eq!(remove_hyphens_and_adapt_case("Bahnhof"), None);
    }
}
