use super::agreement::{PosType, has_reading_of_type, immunized_indices};
use super::helpers::{
    has_pos_tag_starting_with, is_all_uppercase, lowercase_first_char, matches_pos_tag_regex,
    starts_with_uppercase, uppercase_first_char,
};
use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement,
};
use std::collections::HashSet;
use std::sync::{Arc, LazyLock};

const UPPERCASE_MESSAGE: &str =
    "Außer am Satzanfang werden nur Nomen und Eigennamen großgeschrieben.";
const LOWERCASE_MESSAGE: &str =
    "Falls es sich um ein substantiviertes Verb handelt, wird es großgeschrieben.";
const COLON_MESSAGE: &str = "Folgt dem Doppelpunkt weder ein Substantiv noch eine wörtliche Rede oder ein vollständiger Hauptsatz, schreibt man klein weiter.";

static NUMERALS_EN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"\A(?:[a-z]|[0-9]+|(m{0,4}(c[md]|d?c{0,3})(x[cl]|l?x{0,3})(i[xv]|v?i{0,3})))\z",
    )
    .unwrap()
});
static TWO_UPPERCASE_CHARS: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\A[A-ZÖÄÜ][A-ZÖÄÜ][a-zöäüß-]+\z").unwrap());
static VERHALTEN: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\A.+verhalten\z").unwrap());
static IRGEND_ETC: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\A(?:irgendwelche|irgendwas|irgendein|weniger?|einiger?|mehr|aufs)\z")
        .unwrap()
});
static VER_MOD_AUX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\AVER:(?:MOD|AUX):[1-3]:.*\z").unwrap());
static ALLE_NM: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\AAlle[nm]\z").unwrap());
static IN_INNEN: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\A(?:in|innen)\z").unwrap());
static SCHAF_PATTERN: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\A.{3,}schaf(?:s|en)?\z").unwrap());
static SCHAFE_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\A(?:Alpenschaf|Berberschaf|Bergschaf|Blauschaf|Brillenschaf|Dachsteinschaf|Deichschaf|Dickhornschaf|Feinwollschaf|Fettschwanzschaf|Fleischschaf|Fuchsschaf|Glücksschaf|Hausschaf|Jungschaf|Karakulschaf|Klonschaf|Merinoschaf|Milchschaf|Mondschaf|Nutzschaf|Rhönschaf|Riesenwildschaf|Schaukelschaf|Schneeschaf|Steinschaf|Steppenschaf|Superschaf|Waldschaf|Weideschaf|Wildschaf|Wollschaf|Zackelschaf|Zuchtschaf|Zwergblauschaf)(?:s|en)?\z").unwrap()
});
static START_WITH_SPIEL: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\A(?:Spielzugs?|Spielzugangs?|Spielzuganges|Spielzugbuchs?|Spielzugbüchern?|Spielzuges|Spielzugverluste?|Spielzugverluste[ns])\z").unwrap()
});
static END_WITH_SCHAFTE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\A[A-ZÖÄÜ][a-zöäß-]+schafte\z").unwrap());

const NOUN_INDICATORS: [&str; 6] = ["das", "sein", "mein", "dein", "euer", "unser"];

const SENTENCE_START_EXCEPTIONS: [&str; 11] =
    ["(", "\"", "'", "‘", "„", "«", "»", "‚", ".", "!", "?"];

const UNDEFINED_QUANTIFIERS: [&str; 5] = ["viel", "nichts", "nix", "wenig", "allerlei"];

const INTERROGATIVE_PARTICLES: [&str; 9] = [
    "was", "wodurch", "wofür", "womit", "woran", "worauf", "woraus", "wovon", "wie",
];

const POSSESSIVE_INDICATORS: [&str; 6] = ["einer", "eines", "der", "des", "dieser", "dieses"];

const DAS_VERB_EXCEPTIONS: [&str; 7] = ["nur", "sogar", "auch", "die", "alle", "viele", "zu"];

const COLON_QUESTION_WORDS: [&str; 9] = [
    "warum", "wieso", "weshalb", "wer", "was", "wann", "wo", "wie", "wozu",
];

const COLON_QUESTION_CONJUNCTIONS: [&str; 4] = ["und", "oder", "aber", "denn"];

const LANGUAGE_NAMES: [&str; 84] = [
    "Angelsächsisch",
    "Afrikanisch",
    "Albanisch",
    "Altarabisch",
    "Altchinesisch",
    "Altgriechisch",
    "Althochdeutsch",
    "Altpersisch",
    "Amerikanisch",
    "Arabisch",
    "Armenisch",
    "Bairisch",
    "Baskisch",
    "Bengalisch",
    "Bulgarisch",
    "Chinesisch",
    "Dänisch",
    "Deutsch",
    "Englisch",
    "Estnisch",
    "Finnisch",
    "Französisch",
    "Frühneuhochdeutsch",
    "Germanisch",
    "Georgisch",
    "Griechisch",
    "Hebräisch",
    "Hocharabisch",
    "Hochchinesisch",
    "Hochdeutsch",
    "Holländisch",
    "Indonesisch",
    "Irisch",
    "Isländisch",
    "Italienisch",
    "Japanisch",
    "Jiddisch",
    "Jugoslawisch",
    "Kantonesisch",
    "Katalanisch",
    "Klingonisch",
    "Koreanisch",
    "Kroatisch",
    "Kurdisch",
    "Lateinisch",
    "Lettisch",
    "Litauisch",
    "Luxemburgisch",
    "Mittelhochdeutsch",
    "Mongolisch",
    "Neuhochdeutsch",
    "Niederländisch",
    "Norwegisch",
    "Persisch",
    "Plattdeutsch",
    "Polnisch",
    "Portugiesisch",
    "Rätoromanisch",
    "Rumänisch",
    "Russisch",
    "Sächsisch",
    "Schwäbisch",
    "Schwedisch",
    "Schweizerisch",
    "Serbisch",
    "Serbokroatisch",
    "Slawisch",
    "Slowakisch",
    "Slowenisch",
    "Spanisch",
    "Syrisch",
    "Tamilisch",
    "Tibetisch",
    "Tschechisch",
    "Tschetschenisch",
    "Türkisch",
    "Turkmenisch",
    "Uigurisch",
    "Ukrainisch",
    "Ungarisch",
    "Usbekisch",
    "Vietnamesisch",
    "Walisisch",
    "Weißrussisch",
];

static CASE_ANTI_PATTERNS: LazyLock<Option<Vec<crate::engine::XmlPatternRule>>> =
    LazyLock::new(|| {
        let compiler = og_xml::XmlCompiler::new();
        match compiler.compile_file(include_str!("case_antipatterns.xml")) {
            Ok(set) => Some(
                set.rules
                    .into_iter()
                    .map(crate::engine::XmlPatternRule::new)
                    .collect(),
            ),
            Err(_) => None,
        }
    });

static EXCEPTION_PHRASES: LazyLock<Vec<Vec<regex::Regex>>> = LazyLock::new(|| {
    let mut phrases: HashSet<String> = HashSet::new();
    if let Some(lt_root) = crate::engine::find_lt_resource_dir() {
        let dir = lt_root.join("de/src/main/resources/org/languagetool/resource/de");
        for file in ["eigennamen_gross.txt", "case_rule_exceptions.txt"] {
            if let Ok(text) = std::fs::read_to_string(dir.join(file)) {
                for line in text.lines() {
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    phrases.insert(line.to_string());
                }
            }
        }
    }
    phrases
        .into_iter()
        .map(|phrase| {
            phrase
                .split(' ')
                .filter(|p| !p.is_empty())
                .map(|p| regex::Regex::new(&format!(r"\A(?:{p})\z")).unwrap())
                .collect()
        })
        .collect()
});
const EXCEPTIONS: [&str; 667] = [
    "Bedienstete",
    "Bediensteter",
    "Feierwütiger",
    "Feierwütige",
    "Feierwütigen",
    "Dritten",
    "Berufstätige",
    "Berufstätigen",
    "Erwerbstätige",
    "Erwerbstätigen",
    "Tatverdächtige",
    "Tatverdächtigen",
    "Konsumierende",
    "Konsumierenden",
    "Verliebter",
    "Verliebte",
    "Beängstigendes",
    "Oppositioneller",
    "Oppositionelle",
    "Verantwortlicher",
    "Verantwortliche",
    "Verantwortlichen",
    "Beschuldigte",
    "Beschuldigten",
    "Beklagte",
    "Beklagten",
    "Befragte",
    "Befragten",
    "Hingerichtete",
    "Lehrende",
    "Lehrender",
    "Vertrauter",
    "Out",
    "Packet",
    "Paket",
    "Adult",
    "Apart",
    "Universal",
    "Multinational",
    "Additional",
    "Smart",
    "Adverse",
    "Different",
    "Light",
    "Legal",
    "Computational",
    "Holder",
    "Just",
    "Lost",
    "Fundamental",
    "Quick",
    "Infernal",
    "Fit",
    "Fair",
    "Viral",
    "Tough",
    "Indoor",
    "Superb",
    "Resilient",
    "Hexagonal",
    "Responsive",
    "Anno",
    "Mo",
    "Di",
    "Mi",
    "Do",
    "Di. und Do. um 18 Uhr",
    "Fr",
    "Fr. Dr. Müller",
    "Sa",
    "Gr",
    "Gr. 12",
    "Mag",
    "Mag. Helke Müller",
    "Nov",
    "Diss",
    "Invalide",
    "Invalider",
    "Invaliden",
    "Sehende",
    "Sehender",
    "Sehenden",
    "Schutzheilige",
    "Schutzheiliger",
    "Schutzheiligen",
    "Lila",
    "Langzeitarbeitslose",
    "Langzeitarbeitslosen",
    "Langzeitarbeitsloser",
    "Linksintellektuelle",
    "Linksintellektueller",
    "Linksintellektuellen",
    "Beschuldigte",
    "Beschuldigten",
    "Drogenabhängige",
    "Drogenabhängiger",
    "Drogenabhängiger",
    "Drogenabhängigen",
    "Asylsuchender",
    "Asylsuchende",
    "Asylsuchenden",
    "Landtagsabgeordnete",
    "Landtagsabgeordneter",
    "Landtagsabgeordneten",
    "Stadtverordnete",
    "Stadtverordneter",
    "Stadtverordneten",
    "Veränderliche",
    "Veränderlicher",
    "Veränderlichen",
    "Werbetreibende",
    "Werbetreibender",
    "Werbetreibenden",
    "Verletzter",
    "Verletzten",
    "Werktätige",
    "Werktätiger",
    "Werktätigen",
    "Getestete",
    "Getesteten",
    "Genesene",
    "Genesenen",
    "Geimpfte",
    "Geboosterte",
    "Ungeimpfte",
    "Geimpften",
    "Geboosterten",
    "Ungeimpften",
    "Geflüchtete",
    "Geflüchteten",
    "Projektbeteiligte",
    "Projektbeteiligten",
    "Heranwachsende",
    "Heranwachsenden",
    "Interessierte",
    "Interessierten",
    "Infizierte",
    "Infizierten",
    "Gehörlose",
    "Gehörlosen",
    "Drücke",
    "Klecks",
    "Quatsch",
    "Speis",
    "Flash",
    "Suhl",
    "Müh",
    "Bims",
    "Wisch",
    "Außenputz",
    "Rinderhack",
    "Hack",
    "Schlitz",
    "Frevler",
    "Zementputz",
    "Hurst",
    "Bombardier",
    "Kraus",
    "Strunz",
    "Bell",
    "Melk",
    "Klopp",
    "Walz",
    "Schiel",
    "Dusch",
    "Penn",
    "Dörr",
    "Kies",
    "Koks",
    "Dell",
    "Wall",
    "Beige",
    "Zoom",
    "Perl",
    "Parallele",
    "Parallelen",
    "Rutsch",
    "Spar",
    "Merz",
    "Gefahren",
    "Minderjährige",
    "Minderjähriger",
    "Minderjährigen",
    "Scheinselbstständige",
    "Bundestagsabgeordneter",
    "Bundestagsabgeordneten",
    "Bundestagsabgeordnete",
    "Reichstagsabgeordneter",
    "Reichstagsabgeordneten",
    "Reichstagsabgeordnete",
    "Medienschaffende",
    "Medienschaffenden",
    "Medienschaffender",
    "Lehrende",
    "Lehrenden",
    "Vertretene",
    "Vertretenen",
    "Vorstandsvorsitzender",
    "Vorstandsvorsitzenden",
    "Vorstandsvorsitzende",
    "Demonstrierende",
    "Demonstrierenden",
    "Marketingtreibende",
    "Marketingtreibender",
    "Marketingtreibenden",
    "Strafgefangenen",
    "Strafgefangener",
    "Strafgefangene",
    "Pädophile",
    "Pädophiler",
    "Pädophilen",
    "Lehrbeauftragte",
    "Lehrbeauftragter",
    "Lehrbeauftragten",
    "Erkrankte",
    "Erkrankter",
    "Erkrankten",
    "Eigner",
    "Polizeibeamten",
    "Polizeibeamter",
    "Polizeibeamte",
    "Kriegsversehrte",
    "Kriegsversehrter",
    "Kriegsversehrten",
    "Demenzkranke",
    "Demenzkranker",
    "Demenzkranken",
    "Parteivorsitzende",
    "Parteivorsitzender",
    "Parteivorsitzenden",
    "Kriegsgefangene",
    "Kriegsgefangener",
    "Kriegsgefangenen",
    "Ehrenvorsitzende",
    "Ehrenvorsitzender",
    "Ehrenvorsitzenden",
    "Oberkommandierende",
    "Oberkommandierender",
    "Oberkommandierenden",
    "Werbungtreibende",
    "Werbungtreibenden",
    "Mitangeklagte",
    "Schuhfilz",
    "Mix",
    "Rahm",
    "Flansch",
    "WhatsApp",
    "Verschleiß",
    "Schutzsuchende",
    "Schutzsuchenden",
    "Versicherte",
    "Versicherten",
    "Cyberkriminelle",
    "Cyberkriminellen",
    "Kriminelle",
    "Kriminellen",
    "Auszubildenden",
    "Auszubildende",
    "Auszubildender",
    "Lernende",
    "Lernender",
    "Lernenden",
    "Teilnehmende",
    "Teilnehmenden",
    "Radfahrende",
    "Radfahrenden",
    "Autofahrende",
    "Autofahrenden",
    "Auszubildene",
    "Auszubildenen",
    "Absolvierende",
    "Absolvierenden",
    "Einheimische",
    "Einheimischen",
    "Einheimischer",
    "Wehrbeauftragter",
    "Wehrbeauftragte",
    "Wehrbeauftragten",
    "Wehrbeauftragtem",
    "Prozessbevollmächtigter",
    "Prozessbevollmächtigte",
    "Prozessbevollmächtigten",
    "Prozessbevollmächtigtem",
    "Bundesbeamte",
    "Bundesbeamter",
    "Bundesbeamten",
    "Bundesbeamtem",
    "Datenschutzbeauftragter",
    "Datenschutzbeauftragte",
    "Datenschutzbeauftragten",
    "Datenschutzbeauftragtem",
    "Steuerbevollmächtigte",
    "Steuerbevollmächtigter",
    "Steuerbevollmächtigten",
    "Steuerbevollmächtigtem",
    "Suchtkranken",
    "Suchtkranke",
    "Suchtkranker",
    "Filmschaffende",
    "Filmschaffender",
    "Filmschaffenden",
    "Filmschaffendem",
    "Arbeitssuchende",
    "Arbeitssuchender",
    "Arbeitssuchenden",
    "Arbeitssuchendem",
    "Bausachverständige",
    "Bausachverständiger",
    "Bausachverständigen",
    "Bausachverständigem",
    "Heurige",
    "Ratsuchende",
    "Ratsuchender",
    "Ratsuchenden",
    "Verwundete",
    "Verwundeter",
    "Verwundeten",
    "Vollzugsbeamte",
    "Vollzugsbeamter",
    "Vollzugsbeamten",
    "Schutzbefohlene",
    "Schutzbefohlener",
    "Schutzbefohlenen",
    "Verfahrensbeteiligte",
    "Verfahrensbeteiligter",
    "Verfahrensbeteiligten",
    "Kolonialbeamte",
    "Kolonialbeamter",
    "Kolonialbeamten",
    "Verwaltungsbeamte",
    "Verwaltungsbeamter",
    "Verwaltungsbeamten",
    "Verdächtige",
    "Verdächtiger",
    "Verdächtigen",
    "Leichtverletzte",
    "Leichtverletzten",
    "Leichtverletzte",
    "Dozierende",
    "Dozierenden",
    "Studierende",
    "Studierender",
    "Studierenden",
    "Suchbegriffen",
    "Plattdeutsch",
    "Wallet",
    "Str",
    "Priest",
    "Simple",
    "Legend",
    "Golden",
    "Forward",
    "Unverzagt",
    "Auszubildende",
    "Auszubildender",
    "Gelehrte",
    "Gelehrter",
    "Gelehrten",
    "Vorstehende",
    "Vorstehender",
    "Mitwirkende",
    "Mitwirkender",
    "Mitwirkenden",
    "Tabellenletzte",
    "Tabellenletzter",
    "Familienangehörige",
    "Familienangehöriger",
    "Zeitreisende",
    "Zeitreisender",
    "Zeitreisenden",
    "Erwerbstätige",
    "Erwerbstätigen",
    "Erwerbstätiger",
    "Selbstständige",
    "Selbstständigen",
    "Selbstständiger",
    "Selbständige",
    "Selbständigen",
    "Selbständiger",
    "Genaueres",
    "Äußersten",
    "Dienstreisender",
    "Verletzte",
    "Vermisste",
    "Äußeres",
    "Abseits",
    "Unschuldige",
    "Unschuldiger",
    "Unschuldigen",
    "Mitarbeitende",
    "Mitarbeitender",
    "Mitarbeitenden",
    "Beschäftigter",
    "Beschäftigte",
    "Beschäftigten",
    "Beschäftigtem",
    "Bekannter",
    "Bekannte",
    "Bevollmächtigte",
    "Bevollmächtigter",
    "Bevollmächtigten",
    "Brecht",
    "Tel",
    "Unschuldiger",
    "Vorgesetzter",
    "Abs",
    "Klappe",
    "Vorfahre",
    "Mittler",
    "Hr",
    "Schwarz",
    "Genese",
    "Rosa",
    "Auftrieb",
    "Zuschnitt",
    "Geschossen",
    "Vortrieb",
    "Abtrieb",
    "Gesandter",
    "Durchfahrt",
    "Durchgriff",
    "Überfahrt",
    "Zeche",
    "Sparte",
    "Sparten",
    "Heiliger",
    "Reisender",
    "Pest",
    "Schwinge",
    "Verlies",
    "Nachfolge",
    "Stift",
    "Belange",
    "Geistlicher",
    "Google",
    "Hu",
    "Jenseits",
    "Abends",
    "Stimmberechtigte",
    "Stimmberechtigten",
    "Stimmberechtigter",
    "Alleinerziehende",
    "Alleinerziehenden",
    "Alleinerziehender",
    "Abgeordneter",
    "Abgeordnete",
    "Abgeordneten",
    "Angestellter",
    "Angestellte",
    "Angestellten",
    "Armeeangehörige",
    "Armeeangehörigen",
    "Armeeangehöriger",
    "Liberaler",
    "Abriss",
    "Ahne",
    "Ähnlichem",
    "Ähnliches",
    "Er hat Ähnliches erlebt",
    "Allerlei",
    "Anklang",
    "Verlobter",
    "Anstrich",
    "Armes",
    "Ausdrücke",
    "Auswüchsen",
    "Bände",
    "Bänden",
    "Beauftragter",
    "Belange",
    "Biss",
    "De",
    "De Morgan",
    "Diesseits",
    "im Diesseits",
    "Dr",
    "Durcheinander",
    "Eindrücke",
    "Erwachsener",
    "Familienangehörige",
    "Brüder und solche Familienangehörige, die...",
    "Flöße",
    "Folgendes",
    "Fort",
    "Fraß",
    "Fristende",
    "Frevel",
    "Genüge",
    "Gefallen",
    "Gläubige",
    "Gläubiger",
    "Gläubigen",
    "Hechte",
    "Herzöge",
    "Herzögen",
    "Hinfahrt",
    "Hilfsstoff",
    "Hilfsstoffe",
    "Hundert",
    "Zehntausend",
    "Hunderttausend",
    "Hyperwallet",
    "Ihnen",
    "Ihrerseits",
    "Ihr",
    "Ihre",
    "Ihrem",
    "Ihren",
    "Ihrer",
    "Ihres",
    "Infrarot",
    "Jenseits",
    "Jugendliche",
    "Jugendlichen",
    "Jugendlicher",
    "Jünger",
    "Kant",
    "Klaue",
    "Konditional",
    "Krähe",
    "Kurzem",
    "Landwirtschaft",
    "Langem",
    "Längerem",
    "Lausitz",
    "Le",
    "Le Monde",
    "Lehrlingsunterweisung",
    "Leichter",
    "Letzt",
    "Letzt",
    "zu guter Letzt",
    "Letztere",
    "Letzterer",
    "Letzteres",
    "Link",
    "Links",
    "Löhne",
    "Luden",
    "Milk",
    "melken",
    "Mitfahrt",
    "Mr",
    "Mrd",
    "Mrs",
    "Nachfrage",
    "Nachts",
    "des Nachts",
    "eines Nachts",
    "Nachspann",
    "Nähte",
    "Nähten",
    "Narkoseverfahren",
    "Neuem",
    "Neugeborene",
    "Neugeborenen",
    "Neugeborenes",
    "Nr",
    "Nutze",
    "Obdachlose",
    "Obdachloser",
    "Obdachlosen",
    "Oder",
    "Ohrfeige",
    "Patsche",
    "Pfiffe",
    "Pfiffen",
    "Press",
    "Prof",
    "Puste",
    "Sachverständiger",
    "Sankt",
    "Schaulustige",
    "Schaulustigen",
    "Schaulustiger",
    "Scheine",
    "Scheiße",
    "Schuft",
    "Schufte",
    "Schuld",
    "Schwangere",
    "Schwangeren",
    "Schwärme",
    "Schwarzes",
    "Sie",
    "Skype",
    "Spitz",
    "Spott",
    "St",
    "Stereotyp",
    "Störe",
    "Tausend",
    "Tischende",
    "Toter",
    "Übrigen",
    "im Übrigen",
    "Unentschieden",
    "Unvorhergesehenes",
    "etwas Unvorhergesehenes",
    "Verantwortlicher",
    "Verlass",
    "Verwandter",
    "Verstorbenen",
    "Verstorbene",
    "Vielfache",
    "Vielfaches",
    "Vorsitzender",
    "Fraktionsvorsitzender",
    "Verletzte",
    "Verletzten",
    "Walt",
    "Weitem",
    "Weiteres",
    "Wohlen",
    "Wicht",
    "Wichtiges",
    "Wider",
    "das Für und Wider",
    "Wild",
    "Zeche",
    "Zusage",
    "Zwinge",
    "Zirkusrund",
    "Tertiär",
    "Erster",
    "er wurde Erster im Langlauf",
    "Zweiter",
    "Dritter",
    "Vierter",
    "Fünfter",
    "Sechster",
    "Siebter",
    "Achter",
    "Neunter",
    "Erste",
    "sie wurde Erste im Langlauf",
    "Zweite",
    "Dritte",
    "Vierte",
    "Fünfte",
    "Sechste",
    "Siebte",
    "Achte",
    "Neunte",
    "Dein",
    "Deine",
    "Deinem",
    "Deinen",
    "Deiner",
    "Deines",
    "Deinerseits",
    "Dich",
    "Dir",
    "Du",
    "Euch",
    "Euer",
    "Eure",
    "Euern",
    "Eurem",
    "Euren",
    "Eures",
    "Eueren",
    "Euerem",
    "Eueres",
    "Euerer",
    "Eurerseits",
    "Euerseits",
];

const SUBST_VERBEN_EXCEPTIONS: [&str; 37] = [
    "hinziehen",
    "helfen",
    "lassen",
    "passieren",
    "haben",
    "passiert",
    "beschränkt",
    "wiederholt",
    "scheinen",
    "klar",
    "heißen",
    "einen",
    "gehören",
    "bedeutet",
    "ermöglicht",
    "funktioniert",
    "sollen",
    "werden",
    "dürfen",
    "müssen",
    "so",
    "ist",
    "können",
    "mein",
    "sein",
    "muss",
    "muß",
    "wollen",
    "habe",
    "ein",
    "tun",
    "bestätigt",
    "bestätigte",
    "bestätigten",
    "bekommen",
    "sauer",
    "bedeuten",
];

pub struct CaseRule {
    tagger: Option<Arc<og_tagger::GermanTagger>>,
    speller: Option<Arc<og_rules::spelling_support::SpellingSupport>>,
}

struct LowercaseContext<'a> {
    token: &'a str,
    prev_token_is_das: bool,
    next_token_is_personal_or_reflexive_pronoun: bool,
    immunized: bool,
}

struct UppercaseContext<'a> {
    analyzed_token: &'a AnalyzedTokenReadings,
    token: &'a str,
    lowercase_readings: Option<&'a [AnalyzedToken]>,
    immunized: &'a HashSet<usize>,
}

impl CaseRule {
    pub fn new(
        tagger: Option<Arc<og_tagger::GermanTagger>>,
        speller: Option<Arc<og_rules::spelling_support::SpellingSupport>>,
    ) -> Self {
        Self { tagger, speller }
    }

    fn lookup(&self, word: &str) -> Option<Vec<AnalyzedToken>> {
        self.tagger.as_deref().and_then(|t| t.lookup(word))
    }

    fn is_misspelled(&self, word: &str) -> bool {
        if SCHAF_PATTERN.is_match(word) && !SCHAFE_PATTERN.is_match(word) {
            let variant = if let Some(stem) = word.strip_suffix("schaf") {
                format!("{stem}schaft")
            } else if let Some(stem) = word.strip_suffix("schafs") {
                format!("{stem}schaft")
            } else if let Some(stem) = word.strip_suffix("schafen") {
                format!("{stem}schaften")
            } else {
                word.to_string()
            };
            if !self.base_is_misspelled(&variant) {
                return true;
            }
        }
        if word.starts_with("Spielzug") && !START_WITH_SPIEL.is_match(word) {
            return true;
        }
        if word.starts_with("Standart")
            && word != "Standarte"
            && word != "Standarten"
            && !word.starts_with("Standartenträger")
            && !word.starts_with("Standartenführer")
        {
            return true;
        }
        if word.ends_with("schafte") && END_WITH_SCHAFTE.is_match(word) {
            return true;
        }
        self.base_is_misspelled(word)
    }

    fn base_is_misspelled(&self, word: &str) -> bool {
        self.speller
            .as_deref()
            .is_some_and(|s| s.is_misspelled(word))
    }

    fn has_noun_reading(&self, readings: Option<&AnalyzedTokenReadings>) -> bool {
        let Some(readings) = readings else {
            return false;
        };
        if has_pos_tag_starting_with(readings, "ABK") && has_partial_pos_tag(readings, "SUB") {
            return true;
        }
        let token = readings.token().token().replace('\u{AD}', "");
        if let Some(all) = self.lookup(&token) {
            for reading in &all {
                for tag in reading.pos_tags() {
                    if tag.contains("SUB:") && !tag.contains(":ADJ") {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn is_number(&self, token: &str) -> bool {
        if !token.is_empty() && token.chars().all(|c| c.is_numeric()) {
            return true;
        }
        self.lookup(&lowercase_first_char(token))
            .is_some_and(|readings| {
                readings
                    .iter()
                    .any(|r| r.pos_tags().iter().any(|t| t == "ZAL"))
            })
    }

    fn is_potential_uppercase_error(
        &self,
        pos: usize,
        tokens: &[&AnalyzedTokenReadings],
        lowercase_readings: Option<&[AnalyzedToken]>,
        is_preceded_by_modal_or_auxiliary: bool,
    ) -> bool {
        if pos <= 1 {
            return false;
        }
        if tokens[pos - 1].token().token() == "zu"
            && !matches_pos_tag_regex(tokens[pos], ".*(NEU|MAS|FEM)$")
            && lowercase_readings.is_some_and(|lc| {
                lc.iter()
                    .any(|r| r.pos_tags().iter().any(|t| t.starts_with("VER:INF")))
            })
        {
            return true;
        }
        if VERHALTEN.is_match(tokens[pos].token().token()) {
            return false;
        }
        let mut is_potential_error = pos + 3 < tokens.len()
            && tokens[pos + 1].token().token() == ","
            && INTERROGATIVE_PARTICLES.contains(&tokens[pos + 2].token().token())
            && has_pos_tag_starting_with(tokens[pos - 1], "VER:MOD")
            && !tokens[pos - 1].has_lemma("mögen")
            && tokens[pos + 3].token().token() != "zum";
        if !is_potential_error
            && lowercase_readings.is_some_and(|lc| {
                lc.iter().any(|r| {
                    r.pos_tags()
                        .iter()
                        .any(|t| t.contains("SUB:NOM:SIN:NEU:INF") || t.contains("SUB:DAT:PLU:"))
                })
            })
            && (tokens[pos - 1].token().token() == "zu"
                || has_partial_tag_opt(
                    Some(tokens[pos - 1]),
                    &["SUB", "EIG", "VER:AUX:3:", "ADV:TMP", "ABK"],
                ))
        {
            let lc = lowercase_readings.unwrap();
            is_potential_error |= lc
                .iter()
                .any(|r| r.pos_tags().iter().any(|t| t == "PA2:PRD:GRU:VER"))
                && !has_pos_tag_starting_with(tokens[pos - 1], "VER:AUX:3")
                && !lc
                    .iter()
                    .any(|r| r.pos_tags().iter().any(|t| t == "VER:3:PLU:PRT:NON"));
            is_potential_error |= (pos + 2 >= tokens.len()
                || tokens[pos + 1].token().token() == ",")
                && (tokens[pos - 1].token().token() == "zu" || is_preceded_by_modal_or_auxiliary)
                && tokens[pos].token().token().starts_with("Über")
                && lc.iter().any(|r| {
                    r.pos_tags()
                        .iter()
                        .any(|t| t.contains("VER:INF:") || t.contains("PA2:PRD:GRU:VER"))
                });
        }
        is_potential_error
    }

    fn potentially_add_lowercase_match(
        &self,
        rule_matches: &mut Vec<RuleMatch>,
        token_readings: &AnalyzedTokenReadings,
        ctx: LowercaseContext<'_>,
        sentence: &AnalyzedSentence,
    ) {
        let LowercaseContext {
            token,
            prev_token_is_das,
            next_token_is_personal_or_reflexive_pronoun,
            immunized,
        } = ctx;
        if prev_token_is_das
            && !next_token_is_personal_or_reflexive_pronoun
            && token.chars().next().is_some_and(|c| c.is_lowercase())
            && !SUBST_VERBEN_EXCEPTIONS.contains(&token)
            && has_pos_tag_starting_with(token_readings, "VER:INF")
            && !token_readings.is_ignored_by_speller()
            && !immunized
        {
            add_rule_match(
                rule_matches,
                sentence,
                LOWERCASE_MESSAGE,
                token_readings,
                &uppercase_first_char(token_readings.token().token()),
                self_urls(),
            );
        }
    }

    fn potentially_add_uppercase_match(
        &self,
        rule_matches: &mut Vec<RuleMatch>,
        tokens: &[&AnalyzedTokenReadings],
        i: usize,
        ctx: UppercaseContext<'_>,
        sentence: &AnalyzedSentence,
    ) {
        let UppercaseContext {
            analyzed_token,
            token,
            lowercase_readings,
            immunized,
        } = ctx;
        if i == 0 {
            return;
        }
        let is_upper_first = token.chars().next().is_some_and(|c| c.is_uppercase());
        let lc_word = lowercase_first_char(token);
        if is_upper_first
            && token.chars().count() > 1
            && !analyzed_token.is_ignored_by_speller()
            && !immunized.contains(&i)
            && !SENTENCE_START_EXCEPTIONS.contains(&tokens[i - 1].token().token())
            && !EXCEPTIONS.contains(&token)
            && !is_all_uppercase(token)
            && !self.is_language(i, tokens, token)
            && !self.is_probably_city(i, tokens, token)
            && !has_reading_of_type(analyzed_token, PosType::ProperNoun)
            && !analyzed_token.is_sentence_end()
            && !is_ellipsis(i, tokens)
            && !self.is_numbering(i, tokens)
            && !self.is_nominalization(i, tokens, token, lowercase_readings)
            && !self.is_adverb_and_nominalization(i, tokens)
            && !self.is_special_case(i, tokens)
            && !self.is_adjective_as_noun(i, tokens, lowercase_readings)
            && !self.is_singular_imperative(lowercase_readings, tokens[i])
            && !self.is_exception_phrase(i, tokens)
            && !(i == 2 && tokens[i - 1].token().token() == "\u{201C}")
            && !is_case_typo(token)
            && !followed_by_gender_gap(tokens, i)
            && !is_noun_with_verb_reading(tokens[i])
            && !is_invisible_separator(i.wrapping_sub(1), tokens)
            && !self.is_misspelled(&lc_word)
        {
            if tokens[i - 1].token().token() == ":" {
                if is_question_equivalent_after_colon(i, tokens) {
                    return;
                }
                let subarray = &tokens[..i];
                let is_verb_following = is_verb_following(i, tokens, lowercase_readings);
                if !(is_verb_following || count_pos_tag_starting_with(subarray, "VER") == 0) {
                    add_rule_match(
                        rule_matches,
                        sentence,
                        COLON_MESSAGE,
                        tokens[i],
                        &lc_word,
                        self_urls(),
                    );
                }
                return;
            }
            add_rule_match(
                rule_matches,
                sentence,
                UPPERCASE_MESSAGE,
                tokens[i],
                &lc_word,
                self_urls(),
            );
        }
    }

    fn is_language(&self, i: usize, tokens: &[&AnalyzedTokenReadings], token: &str) -> bool {
        let stripped = token.strip_suffix('n').unwrap_or(token);
        let stripped = stripped.strip_suffix('e').unwrap_or(stripped);
        let maybe_language = (token.ends_with("sch") && LANGUAGE_NAMES.contains(&token))
            || LANGUAGE_NAMES.contains(&stripped);
        let next_readings = tokens.get(i + 1).copied();
        let prev_is_auf = tokens
            .get(i.wrapping_sub(1))
            .is_some_and(|t| t.token().token() == "auf");
        maybe_language && (!self.has_noun_reading(next_readings) || prev_is_auf)
    }

    fn is_probably_city(&self, i: usize, tokens: &[&AnalyzedTokenReadings], token: &str) -> bool {
        if matches!(token, "Klein" | "Groß" | "Neu") {
            let Some(next) = tokens.get(i + 1) else {
                return false;
            };
            let is_tagged = next
                .readings()
                .iter()
                .any(|r| r.pos_tags().iter().any(|t| !t.is_empty()));
            return !is_tagged || has_pos_tag_starting_with(next, "EIG");
        }
        false
    }

    fn is_nominalization(
        &self,
        i: usize,
        tokens: &[&AnalyzedTokenReadings],
        token: &str,
        lowercase_readings: Option<&[AnalyzedToken]>,
    ) -> bool {
        let next_readings = tokens.get(i + 1).copied();
        if !(!starts_with_uppercase(token)
            || self.is_number(token)
            || self.has_noun_reading(next_readings)
            || next_readings.is_some_and(|n| is_numeric(n.token().token()))
            || ALLE_NM.is_match(token))
        {
            if lowercase_readings.is_some_and(|lc| {
                lc.iter()
                    .any(|r| r.pos_tags().iter().any(|t| t == "PRP:LOK+TMP+CAU:DAT+AKK"))
            }) {
                return false;
            }
            let prev_token = tokens.get(i.wrapping_sub(1)).copied();
            let prev_prev_token = tokens.get(i.wrapping_sub(2)).copied();
            let prev_prev_prev_token = tokens.get(i.wrapping_sub(3)).copied();
            let prev_token_str = prev_token.map(|t| t.token().token()).unwrap_or_default();
            if (matches!(prev_token_str, "und" | "oder" | "beziehungsweise")
                && prev_prev_token.is_some()
                && has_partial_pos_tag(tokens[i], "SUB")
                && has_partial_pos_tag(tokens[i], ":ADJ"))
                || (prev_prev_token.is_some_and(|t| has_partial_pos_tag(t, "SUB"))
                    && !self.has_noun_reading(next_readings)
                    && lowercase_readings.is_some_and(|lc| {
                        lc.iter()
                            .any(|r| r.pos_tags().iter().any(|t| t.contains("ADJ")))
                    })
                    && prev_token_str != ",")
            {
                return true;
            }
            if lowercase_readings.is_some_and(|lc| {
                lc.iter()
                    .any(|r| r.pos_tags().iter().any(|t| t == "PA1:PRD:GRU:VER"))
            }) {
                return false;
            }
            return ((prev_token.is_some()
                && IRGEND_ETC.is_match(prev_token_str)
                && has_partial_pos_tag(tokens[i], "SUB"))
                || self.is_number(prev_token_str))
                || (has_partial_tag_opt(prev_token, &["ART", "PRO:"])
                    && !(((i < 4 && tokens.len() > 4)
                        || prev_token.is_some_and(|t| t.readings().len() == 1)
                        || prev_prev_token.is_some_and(|t| t.has_lemma("sein")))
                        && prev_token
                            .is_some_and(|t| has_pos_tag_starting_with(t, "PRO:PER:NOM:")))
                    && !prev_token.is_some_and(|t| has_partial_pos_tag(t, ":STD")))
                || (has_partial_tag_opt(prev_prev_prev_token, &["ART"])
                    && has_partial_tag_opt(prev_prev_token, &["PRP"])
                    && has_partial_tag_opt(prev_token, &["SUB"]))
                || (has_partial_tag_opt(prev_prev_token, &["PRO:", "PRP"])
                    && has_partial_tag_opt(prev_token, &["ADJ", "ADV", "PA2", "PA1"]))
                || (has_partial_tag_opt(prev_prev_prev_token, &["PRO:", "PRP"])
                    && has_partial_tag_opt(prev_prev_token, &["ADJ", "ADV"])
                    && has_partial_tag_opt(prev_token, &["ADJ", "ADV", "PA2"]))
                || (has_pos_tag_starting_with(tokens[i], "SUB:")
                    && has_partial_tag_opt(prev_token, &["GEN"])
                    && !has_partial_tag_opt(next_readings, &["PKT"]));
        }
        false
    }

    fn is_adverb_and_nominalization(&self, i: usize, tokens: &[&AnalyzedTokenReadings]) -> bool {
        let prev_prev_token = if i > 1 {
            tokens[i - 2].token().token()
        } else {
            ""
        };
        let prev_token = tokens.get(i.wrapping_sub(1)).copied();
        let token = tokens[i].token().token();
        let next_readings = tokens.get(i + 1).copied();
        prev_prev_token.eq_ignore_ascii_case("das")
            && has_partial_tag_opt(prev_token, &["ADV"])
            && starts_with_uppercase(token)
            && !self.has_noun_reading(next_readings)
    }

    fn is_special_case(&self, i: usize, tokens: &[&AnalyzedTokenReadings]) -> bool {
        let prev_token = if i > 1 {
            tokens[i - 1].token().token()
        } else {
            ""
        };
        let token = tokens[i].token().token();
        let next_readings = tokens.get(i + 1).copied();
        prev_token.eq_ignore_ascii_case("im")
            && token == "Allgemeinen"
            && !self.has_noun_reading(next_readings)
    }

    fn is_adjective_as_noun(
        &self,
        i: usize,
        tokens: &[&AnalyzedTokenReadings],
        lowercase_readings: Option<&[AnalyzedToken]>,
    ) -> bool {
        let prev_token = tokens.get(i.wrapping_sub(1)).copied();
        let next_readings = tokens.get(i + 1).copied();
        let mut prev_lowercase_readings: Option<Vec<AnalyzedToken>> = None;
        if i > 1
            && SENTENCE_START_EXCEPTIONS.contains(&tokens[i - 2].token().token())
            && let Some(prev) = prev_token
        {
            let word = prev.token().token().to_lowercase();
            prev_lowercase_readings = self.lookup(&word);
        }

        let is_possibly_followed_by_infinitive =
            next_readings.is_some_and(|n| n.token().token() == "zu");
        let is_followed_by_infinitive = next_readings
            .is_some_and(|n| !is_possibly_followed_by_infinitive && has_partial_pos_tag(n, "EIZ"));
        let is_followed_by_possessive_indicator =
            next_readings.is_some_and(|n| POSSESSIVE_INDICATORS.contains(&n.token().token()));

        let is_undef_quantifier = prev_token.is_some_and(|p| {
            UNDEFINED_QUANTIFIERS.contains(&p.token().token().to_lowercase().as_str())
        });
        let is_prev_determiner = prev_token.is_some_and(|p| {
            (has_partial_tag_opt(Some(p), &["ART", "PRP", "ZAL"])
                || prev_lowercase_readings
                    .as_deref()
                    .is_some_and(|lc| has_partial_tag_opt_lc(Some(lc), &["ART", "PRP", "ZAL"])))
                && !has_partial_pos_tag(p, ":STD")
        });
        let is_preceded_by_verb = prev_token.is_some_and(|p| {
            matches_pos_tag_regex(p, "VER:(MOD:|AUX:)?[1-3]:.*") && !p.has_lemma("sein")
        });
        if !(is_prev_determiner
            || is_undef_quantifier
            || is_possibly_followed_by_infinitive
            || is_followed_by_infinitive)
            && !(is_preceded_by_verb
                && lowercase_readings.is_some()
                && has_partial_tag_opt_lc(lowercase_readings, &["ADJ:", "PA"])
                && next_readings
                    .is_some_and(|n| !matches!(n.token().token(), "und" | "oder" | ",")))
            && !(is_followed_by_possessive_indicator
                && has_partial_tag_opt_lc(lowercase_readings, &["ADJ", "VER"]))
            && !(prev_token.is_some_and(|p| p.has_pos_tag("KON:UNT"))
                && !self.has_noun_reading(next_readings)
                && next_readings.is_some_and(|n| !n.has_pos_tag("KON:NEB")))
        {
            let prev_prev_token =
                if i > 1 && prev_token.is_some_and(|p| has_partial_pos_tag(p, "ADJ")) {
                    tokens.get(i - 2).copied()
                } else {
                    None
                };
            if !is_preceded_by_verb
                && let Some(lc) = lowercase_readings
                && let Some(prev_token) = prev_token
            {
                if has_partial_pos_tag(prev_token, "SUB:")
                    && lc.iter().any(|r| {
                        r.pos_tags().iter().any(|t| {
                            t.starts_with("ADJ:GEN:PLU:MAS:GRU:SOL")
                                || t.starts_with("PA2:GEN:PLU:MAS:GRU:SOL")
                        })
                    })
                {
                    return next_readings.is_some_and(|n| !has_partial_pos_tag(n, "SUB:"));
                } else if next_readings.is_some_and(|n| {
                    n.readings().len() == 1
                        && has_pos_tag_starting_with(prev_token, "PRO:PER:NOM:")
                        && n.has_pos_tag("ADJ:PRD:GRU")
                }) {
                    return true;
                }
            }
            if !has_partial_tag_opt(prev_prev_token, &["ART", "PRP", "ZAL"]) {
                return false;
            }
        }

        for reading in tokens[i].readings() {
            for pos_tag in reading.pos_tags() {
                let tag_empty = pos_tag.is_empty();
                if (tag_empty || pos_tag.contains("ADJ"))
                    && !self.has_noun_reading(next_readings)
                    && !next_readings.is_some_and(|n| is_numeric(n.token().token()))
                {
                    if tag_empty
                        && has_partial_tag_opt_lc(
                            lowercase_readings,
                            &[
                                "PRP:LOK",
                                "PA2:PRD:GRU:VER",
                                "PA1:PRD:GRU:VER",
                                "ADJ:PRD:KOM",
                                "ADV:TMP",
                            ],
                        )
                    {
                        continue;
                    }
                    return true;
                }
            }
        }
        false
    }

    fn is_singular_imperative(
        &self,
        lowercase_readings: Option<&[AnalyzedToken]>,
        token: &AnalyzedTokenReadings,
    ) -> bool {
        lowercase_readings.is_some_and(|lc| {
            lc.iter()
                .any(|r| r.pos_tags().iter().any(|t| t.starts_with("VER:IMP:SIN")))
        }) && token.token().token() != "Ein"
            && token.token().token() != "Eine"
    }

    fn is_exception_phrase(&self, i: usize, tokens: &[&AnalyzedTokenReadings]) -> bool {
        for patterns in EXCEPTION_PHRASES.iter() {
            for (j, pattern) in patterns.iter().enumerate() {
                if pattern.is_match(tokens[i].token().token()) {
                    let start_index = i as isize - j as isize;
                    if compare_lists(
                        tokens,
                        start_index,
                        start_index + patterns.len() as isize - 1,
                        patterns,
                    ) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn is_numbering(&self, i: usize, tokens: &[&AnalyzedTokenReadings]) -> bool {
        i >= 2
            && matches!(tokens[i - 1].token().token(), ")" | "]")
            && NUMERALS_EN.is_match(tokens[i - 2].token().token())
            && !(i > 3
                && tokens[i - 3].token().token() == "("
                && has_pos_tag_starting_with(tokens[i - 4], "SUB:"))
    }
}

fn self_urls() -> Vec<String> {
    vec![
        "https://dict.leo.org/grammatik/deutsch/Rechtschreibung/Regeln/Gross-klein/index.html"
            .to_string(),
    ]
}

fn add_rule_match(
    rule_matches: &mut Vec<RuleMatch>,
    sentence: &AnalyzedSentence,
    msg: &str,
    token_readings: &AnalyzedTokenReadings,
    fixed_word: &str,
    urls: Vec<String>,
) {
    let from = token_readings.token().start();
    let to = token_readings.token().end();
    let rm_rule = RuleMatchRule::new(
        "DE_CASE",
        "Großschreibung von Nomen und substantivierten Verben",
    )
    .with_category(Category::new("CASING", "Groß-/Kleinschreibung"))
    .with_issue_type("casing")
    .with_urls(urls);
    rule_matches.push(
        RuleMatch::new(
            msg,
            from,
            to - from,
            rm_rule,
            RuleMatchContext::new(sentence.text().to_string(), from, to - from),
        )
        .with_sentence(sentence.text().to_string())
        .with_replacements(vec![SuggestedReplacement::new(fixed_word)]),
    );
}

fn has_partial_pos_tag(token: &AnalyzedTokenReadings, needle: &str) -> bool {
    token
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.contains(needle)))
}

fn has_partial_tag_opt(token: Option<&AnalyzedTokenReadings>, pos_tags: &[&str]) -> bool {
    token.is_some_and(|t| pos_tags.iter().any(|p| has_partial_pos_tag(t, p)))
}

fn has_partial_tag_opt_lc(readings: Option<&[AnalyzedToken]>, pos_tags: &[&str]) -> bool {
    readings.is_some_and(|lc| {
        lc.iter().any(|r| {
            r.pos_tags()
                .iter()
                .any(|t| pos_tags.iter().any(|p| t.contains(p)))
        })
    })
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_numeric())
}

fn count_pos_tag_starting_with(tokens: &[&AnalyzedTokenReadings], prefix: &str) -> usize {
    tokens
        .iter()
        .filter(|t| has_pos_tag_starting_with(t, prefix))
        .count()
}

fn is_salutation(token: &str) -> bool {
    matches!(token, "Herr" | "Hr" | "Herrn" | "Frau" | "Fr" | "Fräulein")
}

fn is_company(token: &str) -> bool {
    matches!(
        token,
        "Firma" | "Familie" | "Unternehmen" | "Firmen" | "Bäckerei" | "Metzgerei" | "Fa"
    )
}

fn is_prev_probably_relative_pronoun(tokens: &[&AnalyzedTokenReadings], i: usize) -> bool {
    i >= 3
        && tokens[i - 1].token().token() == "das"
        && tokens[i - 2].token().token() == ","
        && matches_pos_tag_regex(tokens[i - 3], "SUB:...:SIN:NEU")
}

fn is_followed_by_relative_or_subordinate_clause(
    i: usize,
    tokens: &[&AnalyzedTokenReadings],
) -> bool {
    if i + 4 < tokens.len() {
        return tokens[i + 1].token().token() == ","
            && (INTERROGATIVE_PARTICLES.contains(&tokens[i + 2].token().token())
                || tokens[i + 2].has_pos_tag("KON:UNT"));
    }
    false
}

fn is_case_typo(token: &str) -> bool {
    TWO_UPPERCASE_CHARS.is_match(token)
}

fn followed_by_gender_gap(tokens: &[&AnalyzedTokenReadings], i: usize) -> bool {
    i + 2 < tokens.len()
        && tokens[i + 1].token().token() == ":"
        && IN_INNEN.is_match(tokens[i + 2].token().token())
}

fn is_noun_with_verb_reading(token: &AnalyzedTokenReadings) -> bool {
    has_pos_tag_starting_with(token, "SUB") && has_pos_tag_starting_with(token, "VER:INF")
}

fn is_invisible_separator(i: usize, tokens: &[&AnalyzedTokenReadings]) -> bool {
    tokens
        .get(i)
        .is_some_and(|t| t.token().token().starts_with('\u{2063}'))
}

fn is_ellipsis(i: usize, tokens: &[&AnalyzedTokenReadings]) -> bool {
    matches!(tokens[i - 1].token().token(), "]" | ")")
        && ((i == 4 && tokens[i - 2].token().token() == "…")
            || (i == 6 && tokens[i - 2].token().token() == "."))
}

fn is_verb_following(
    i: usize,
    tokens: &[&AnalyzedTokenReadings],
    lowercase_readings: Option<&[AnalyzedToken]>,
) -> bool {
    let first_is_verb = match lowercase_readings {
        Some(lc) => lc
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t.starts_with("VER:"))),
        None => has_pos_tag_starting_with(tokens[i], "VER:"),
    };
    let mut count = usize::from(first_is_verb);
    for t in &tokens[i + 1..] {
        if has_pos_tag_starting_with(t, "VER:") {
            count += 1;
        }
    }
    count != 0
}

fn is_colon_question_word(word: &str) -> bool {
    COLON_QUESTION_WORDS
        .iter()
        .any(|w| w.eq_ignore_ascii_case(word))
}

fn is_question_equivalent_after_colon(i: usize, tokens: &[&AnalyzedTokenReadings]) -> bool {
    if i + 1 < tokens.len() {
        let word = tokens[i].token().token();
        let next = tokens[i + 1].token().token();
        if is_colon_question_word(word) && next == "?" {
            return true;
        }
        if COLON_QUESTION_CONJUNCTIONS
            .iter()
            .any(|c| c.eq_ignore_ascii_case(word))
            && i + 2 < tokens.len()
            && is_colon_question_word(tokens[i + 1].token().token())
            && tokens[i + 2].token().token() == "?"
        {
            return true;
        }
    }
    false
}

fn compare_lists(
    tokens: &[&AnalyzedTokenReadings],
    start_index: isize,
    end_index: isize,
    patterns: &[regex::Regex],
) -> bool {
    if start_index < 0 {
        return false;
    }
    let mut i = 0usize;
    let mut j = start_index;
    while j <= end_index {
        if i >= patterns.len()
            || j >= tokens.len() as isize
            || !patterns[i].is_match(tokens[j as usize].token().token())
        {
            return false;
        }
        i += 1;
        j += 1;
    }
    true
}

impl Rule for CaseRule {
    fn id(&self) -> &str {
        "DE_CASE"
    }

    fn description(&self) -> &str {
        "Großschreibung von Nomen und substantivierten Verben"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("CASING", "Groß-/Kleinschreibung")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("casing".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let immunized: HashSet<usize> = CASE_ANTI_PATTERNS
            .as_ref()
            .map(|rules| immunized_indices(rules, sentence))
            .unwrap_or_default();
        let mut rule_matches = Vec::new();
        let mut prev_token_is_das = false;
        let mut is_preceded_by_modal_or_auxiliary = false;
        for i in 0..tokens.len() {
            if first_pos_tag(tokens[i]).is_some_and(|t| t == "SENT_START") {
                continue;
            }
            if i == 1 {
                prev_token_is_das =
                    NOUN_INDICATORS.contains(&tokens[1].token().token().to_lowercase().as_str());
                continue;
            }
            if i > 0
                && (is_salutation(tokens[i - 1].token().token())
                    || is_company(tokens[i - 1].token().token()))
            {
                continue;
            }
            if i > 2
                && NUMERALS_EN.is_match(tokens[i - 1].token().token())
                && tokens[i - 2].token().token() == "."
                && NUMERALS_EN.is_match(tokens[i - 3].token().token())
            {
                continue;
            }

            let analyzed_token = tokens[i];
            let token = analyzed_token.token().token();
            let is_baseform =
                !analyzed_token.readings().is_empty() && analyzed_token.has_lemma(token);
            if (first_pos_tag(analyzed_token).is_none()
                || has_reading_of_type(analyzed_token, PosType::Verb))
                && is_baseform
            {
                let mut next_token_is_personal_or_reflexive_pronoun = false;
                if i + 1 < tokens.len() {
                    let next = tokens[i + 1];
                    next_token_is_personal_or_reflexive_pronoun =
                        has_partial_pos_tag(next, "PRO:PER")
                            || matches!(next.token().token(), "sich" | "Sie");
                    if next.has_pos_tag("PKT") {
                        continue;
                    }
                    if prev_token_is_das
                        && (DAS_VERB_EXCEPTIONS.contains(&next.token().token())
                            || is_followed_by_relative_or_subordinate_clause(i, &tokens))
                        || (i > 1
                            && has_partial_tag_opt(
                                tokens.get(i - 2).copied(),
                                &["VER:AUX", "VER:MOD"],
                            ))
                    {
                        continue;
                    }
                }
                if is_prev_probably_relative_pronoun(&tokens, i)
                    || (prev_token_is_das && count_pos_tag_starting_with(&tokens, "VER") == 1)
                {
                    continue;
                }
                self.potentially_add_lowercase_match(
                    &mut rule_matches,
                    tokens[i],
                    LowercaseContext {
                        token,
                        prev_token_is_das,
                        next_token_is_personal_or_reflexive_pronoun,
                        immunized: immunized.contains(&i),
                    },
                    sentence,
                );
            }
            prev_token_is_das = NOUN_INDICATORS.contains(&token.to_lowercase().as_str());
            if analyzed_token
                .readings()
                .iter()
                .any(|r| r.pos_tags().iter().any(|t| VER_MOD_AUX.is_match(t)))
            {
                is_preceded_by_modal_or_auxiliary = true;
            }
            let lowercase_readings = self.lookup(&token.to_lowercase());
            if self.has_noun_reading(Some(analyzed_token)) {
                if !self.is_potential_uppercase_error(
                    i,
                    &tokens,
                    lowercase_readings.as_deref(),
                    is_preceded_by_modal_or_auxiliary,
                ) {
                    continue;
                }
            } else if has_pos_tag_starting_with(analyzed_token, "SUB:")
                && i + 1 < tokens.len()
                && tokens[i + 1]
                    .token()
                    .token()
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_lowercase())
                && matches_pos_tag_regex(tokens[i + 1], "(VER:[123]:|PA2).+")
            {
                continue;
            }
            if first_pos_tag(analyzed_token).is_none() && lowercase_readings.is_none() {
                continue;
            }
            if first_pos_tag(analyzed_token).is_none()
                && let Some(lc) = lowercase_readings.as_deref()
                && (first_pos_tag_lc(lc).is_none() || token.ends_with("innen"))
            {
                continue;
            }
            self.potentially_add_uppercase_match(
                &mut rule_matches,
                &tokens,
                i,
                UppercaseContext {
                    analyzed_token,
                    token,
                    lowercase_readings: lowercase_readings.as_deref(),
                    immunized: &immunized,
                },
                sentence,
            );
        }
        rule_matches
    }
}

fn first_pos_tag(token: &AnalyzedTokenReadings) -> Option<&str> {
    token
        .readings()
        .first()
        .and_then(|r| r.pos_tags().first())
        .map(String::as_str)
}

fn first_pos_tag_lc(readings: &[AnalyzedToken]) -> Option<&str> {
    readings
        .first()
        .and_then(|r| r.pos_tags().first())
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use crate::engine::LanguageEngine;

    fn shared_engine() -> std::sync::Arc<LanguageEngine> {
        crate::engine::LanguageEngine::shared_for_code("de").expect("german engine")
    }

    fn case_matches(text: &str) -> Vec<(String, usize, String)> {
        shared_engine()
            .check(text)
            .matches
            .iter()
            .filter(|m| m.rule().id() == "DE_CASE")
            .map(|m| {
                (
                    m.message().to_string(),
                    m.offset(),
                    m.replacements()
                        .first()
                        .map(|r| r.value().to_string())
                        .unwrap_or_default(),
                )
            })
            .collect()
    }

    #[test]
    fn java_good_sentences_stay_clean() {
        use rayon::prelude::*;
        let offenders: Vec<String> = [
            "(Dauer, Raum, Anwesende)",
            "Es gibt wenige Befragte.",
            "Es gibt mehr Befragte, die das machen würden.",
            "Das ist eine Abkehr von Gottes Geboten.",
            "Dem Hund Futter geben",
            "So könnte es auch den Handwerksbetrieben gehen, die ausbilden und deren Ausbildung dann Industriebetrieben zugutekäme.",
            "Die Firma Drosch hat nicht pünktlich geliefert.",
            "Das schlägt nicht so zu Buche.",
            "Dirk Hetzel ist ein Name.",
            "Aber sie tat es, sodass unsere Klasse das sehen und fotografieren konnte.",
            "Sein Verhalten war okay.",
            "Hier ein Satz. 'Ein Zitat.'",
            "Hier ein Satz. «Ein Zitat.»",
            "Hier ein Satz. »Ein Zitat.«",
            "Hier ein Satz. (Noch einer.)",
            "Hier geht es nach Tel Aviv.",
            "Unser Jüngster ist da.",
            "Alles Erfundene ist wahr.",
            "Sie hat immer ihr Bestes getan.",
            "Er wird etwas Verrücktes träumen.",
            "Er wird etwas schön Verrücktes träumen.",
            "Er wird etwas ganz schön Verrücktes träumen.",
            "Mit aufgewühltem Innerem.",
            "Mit völlig aufgewühltem Innerem.",
            "Tom ist etwas über dreißig.",
            "Diese Angriffe bleiben im Verborgenen.",
            "Ihr sollt mich das wissen lassen.",
            "Wenn er mich das rechtzeitig wissen lässt, gerne.",
            "Und sein völlig aufgewühltes Inneres erzählte von den Geschehnissen.",
            "Ein Kaninchen, das zaubern kann.",
            "Keine Ahnung, wie ich das prüfen sollte.",
            "Und dann noch Strafrechtsdogmatikerinnen.",
            "Er kann ihr das bieten, was sie verdient.",
            "Das fragen sich mittlerweile viele.",
            "Ich habe gehofft, dass du das sagen würdest.",
            "Eigentlich hätte ich das wissen müssen.",
            "Mir tut es wirklich leid, Ihnen das sagen zu müssen.",
            "Der Wettkampf endete im Unentschieden.",
            "Er versuchte, Neues zu tun.",
            "Du musst das wissen, damit du die Prüfung bestehst",
            "Er fragte, ob das gelingen wird.",
            "Er mag Obst, wie zum Beispel Apfelsinen.",
            "Er will die Ausgaben für Umweltschutz und Soziales kürzen.",
            "Die Musicalverfilmung „Die Schöne und das Biest“ bricht mehrere Rekorde.",
            "Joachim Sauer lobte Johannes Rau.",
            "Im Falle des Menschen ist dessen wirkendes Wollen gegeben.",
            "Szenario: 1) Zwei Galaxien verschmelzen.",
            "Existieren Außerirdische im Universum?",
            "Tom vollbringt Außerordentliches.",
            "Er führt Böses im Schilde.",
            "Es gab Überlebende.",
            "'Wir werden das stoppen.'",
            "Wahre Liebe muss das aushalten.",
            "Du kannst das machen.",
            "Vor dem Aus stehen.",
            "Ich Armer!",
            "Hallo Malte,",
            "Parks Vertraute Choi Soon Sil ist zu drei Jahren Haft verurteilt worden.",
            "Bei einer Veranstaltung Rechtsextremer passierte es.",
            "Eine Gruppe Betrunkener singt.",
            "Bei Betreten des Hauses.",
            "Das Aus für Italien ist bitter.",
            "Das Aus kam unerwartet.",
            "Gibt es die Schuhe auch in Gr. 43?",
            "Weil er Unmündige sexuell missbraucht haben soll, wurde ein Lehrer verhaftet.",
            "Tausende Gläubige kamen.",
            "Es kamen Tausende Gläubige.",
            "Das schließen Forscher aus den gefundenen Spuren.",
            "Wieder Verletzter bei Unfall",
            "‚Dieser Satz.‘ Hier kommt der nächste Satz.",
            "Dabei werden im Wesentlichen zwei Prinzipien verwendet:",
            "Er fragte, ob das gelingen oder scheitern wird.",
            "Einen Tag nach Bekanntwerden des Skandals",
            "Das machen eher die Erwachsenen.",
            "Das ist ihr Zuhause.",
            "Das ist Sandras Zuhause.",
            "Das machen eher wohlhabende Leute.",
            "Das schaffen moderne E-Autos locker.",
            "Das verwalten User.",
            "Man kann das generalisieren",
            "Aber wie wir das machen und sicher gestalten, darauf konzentriert sich unsere Arbeit.",
            "Assistenzsysteme warnen rechtzeitig vor Gefahren.",
            "Jeremy Schulte rannte um sein Leben.",
            "Er spricht Sunnitisch & Schiitisch.",
            "Er sagte, Geradliniges und Krummliniges sei unvergleichbar.",
            "Dort erfahren sie Kurioses und Erstaunliches zum Zusammenspiel von Mensch und Natur.",
            "Dabei unterscheidet die Shareware zwischen Privatem und Dienstlichem bei Fahrten ebenso wie bei Autos.",
            "Besucher erwartet Handegefertigtes, Leckeres und Informatives rund um den Hund.",
            "Der Unterschied zwischen Vorstellbarem und Machbarem war niemals geringer.",
            "Das war Fiete Lang.",
            "Wenn du an das glaubst, was du tust, kannst du Großes erreichen.",
            "Dann hat er Großes erreicht.",
            "Dann hat er Großes geleistet.",
            "Das Thema Datenaustauschverfahren ist mir wichtig.",
            "Ist das eine Frage ? Müsste das nicht anders sein?",
            "Das ist ein Satz !!! Das auch.",
            "Der russische Erdölmagnat Emanuel Nobel, der Erbauer des ersten Dieselmotorschiffes.",
            "Zur Versöhnung: Jüdische Gläubige sollen beten.",
            "Fast im Stundentakt wurden neue Infizierte gemeldet.",
            "Bert Van Den Brink",
            "“In den meisten Bundesländern werden solche Studien per se nicht durchgeführt.”",
            "A) Das Haus",
            "Rabi und Polykarp Kusch an der Columbia-Universität",
            "Man geht davon aus, dass es sich dabei nicht um Reinigungsverhalten handelt.",
            "Wenn dort oft Gefahren lauern.",
            "3b) Den Bereich absichern",
            "@booba Da der Holger keine Zeit hat ...",
            "Es gibt infizierte Ärzt*innen.",
            "🙂 Übrigens finde ich dein neues Ordnungssystem richtig genial!",
            "Ein 10,4 Ah Lithium-Akku",
            "14:15 Uhr SpVgg Westheim",
            "Unser Wärmestrom-Tarif WärmeKompakt im Detail",
            "Autohaus Dornig GmbH",
            "Hans Pries GmbH",
            "Der Kund*innenservice war auch sehr kulant und persönlich.",
            ":D Auf dieses Frl.",
            "@b_fischer Der Bonussemester-Antrag oder der Widerspruch?",
            "Das Gedicht “Der Panther”.",
            "Klar, dass wir das brauchen.",
            "Bei der Fülle an Vorgaben kann das schnell vergessen werden.",
            "Liebe Kund:in",
            "Wir sollten das mal labeln.",
            "Teil 1: Der unaufhaltsame Aufstieg Bonapartes",
            "Der Absatz bestimmt, in welchem Maße diese Daten Dritten zugänglich gemacht werden.",
            "Der TN spricht Russisch - Muttersprache",
            "Ich musste das Video mehrmals stoppen, um mir über das Gesagte Gedanken zu machen.",
            "Während Besagtes Probleme verursachte.",
            "Während der Befragte Geschichten erzählte.",
            "Während ein Befragter Geschichten erzählte.",
            "... für welche ein Befragter Geld ausgegeben hat.",
            "Während die Befragte Geld verdiente.",
            "Sind dem Zahlungspflichtigen Kosten entstanden?",
            "Jetzt, wo Protestierende und Politiker sich streiten",
            "Die Nacht, die Liebe, dazu der Wein — zu nichts Gutem Ratgeber sein.",
            "Warum tun die Menschen Böses?",
            "Und das Vergangene Revue passieren lassen",
            "Seither ist das Französische Amtssprache in Frankreich.",
            "Für die Betreute Kontoauszüge holen.",
            "Das verstehen Deutsche halt nicht.",
            "12:00 - 13:00 Gemeinsames Mittagessen",
            "Meld dich, wenn du Großes vorhast.",
            "Der Aphorismus will nicht Dumme gescheit, sondern Gescheite nachdenklich machen.",
            "Während des Hochwassers den Eingeschlossenen Wasser und Nahrung bringen",
            "Aus dem Stein der Weisen macht ein Dummer Schotter.",
            "Auf dem Weg zu ihnen begegnet der Halbwüchsige Revolverhelden und Indianern.",
            "▶︎ Dies ist ein Test",
            "▶ Dies ist ein Test",
            "* Dies ist ein Test",
            "- Dies ist ein Test",
            "• Dies ist ein Test",
            ":-) Dies ist ein Test",
            ";-) Dies ist ein Test",
            "Das fahrende Auto.",
            "Das können wir so machen.",
            "Denn das Fahren ist einfach.",
            "Das Fahren ist einfach.",
            "Das Gehen fällt mir leicht.",
            "Das Ernten der Kartoffeln ist mühsam.",
            "Entschuldige das späte Weiterleiten.",
            "Ich liebe das Lesen.",
            "Das Betreten des Rasens ist verboten.",
            "Das haben wir aus eigenem Antrieb getan.",
            "Das haben wir.",
            "Das haben wir schon.",
            "Das lesen sie doch sicher in einer Minute durch.",
            "Das lesen Sie doch sicher in einer Minute durch!",
            "Formationswasser, das oxidiert war.",
            "Um das herauszubekommen diskutieren zwei Experten.",
            "Die Mannschaft ist eine gelungene Mischung aus alten Haudegen und jungen Wilden.",
            "Er hält eine Handbreit Abstand.",
            "Das ist das Debakel und Aus für Podolski.",
            "Ein Highlight für Klein und Groß!",
            "Der schwedische Psychologe Dan Katz, Autor von 'Angst kocht auch nur mit Wasser', sieht in der Corona-Krise dennoch nicht nur Negatives.",
        ]
        .into_par_iter()
        .filter_map(|text| {
            let matches = case_matches(text);
            (!matches.is_empty()).then(|| format!("{text}: got {matches:?}"))
        })
        .collect();
        assert!(offenders.is_empty(), "{}", offenders.join("\n"));
    }

    #[test]
    fn java_bad_sentences_flag_exactly_once() {
        use rayon::prelude::*;
        let offenders: Vec<String> = [
            "Während des Hochwassers den Eingeschlossenen Menschen Nahrung bringen",
            "Während Gefragte Menschen antworteten.",
            "Ich brauche eine Gratis App die Ohne WLAN.",
            "Alle Kommunikationsmedien die Meinem Widersacher dienen werden.",
            "Ich wünsche dir Alles Liebe.",
            "Das Auto Meines Vaters wird in Italien produziert.",
            "Ich sehe da keine Absolute Schranke.",
            "Manns und Fontanes Gesammelten Werken.",
            "Und das Neue Haus.",
            "Das sind die Die Lehrer.",
            "Und das Gesagte Wort.",
            "Und die Gesagten Wörter.",
            "Und meine Erzählte Geschichte.",
            "Und diese Erzählten Geschichten.",
            "Und eine Neue Zeit.",
            "Das machen der Töne ist schwierig.",
            "Sie Vertraute niemandem.",
            "Beten Lernt man in Nöten.",
            "Er sagte, Geradliniges und krummliniges sei unvergleichbar.",
            "Er sagte, ein Geradliniges und Krummliniges Konzept ist nicht tragbar.",
            "Ä Was?",
            "… die preiswerte Variante unserer Topseller im Bereich Alternativ Mehle.",
            "Hi und Herzlich willkommen auf meiner Seite.",
            "Er ist Groß.",
            "Die Zahl ging auf Über 1.000 zurück.",
            "Er sammelt Große und kleine Tassen.",
            "Er sammelt Große, mittlere und kleine Tassen.",
            "Dann will sie mit London Über das Referendum verhandeln.",
            "Sie kann sich täglich Über vieles freuen.",
            "Der Vater (51) Fuhr nach Rom.",
            "Er müsse Überlegen, wie er das Problem löst.",
            "Er sagte, dass er Über einen Stein stolperte.",
            "Unser warten wird sich lohnen.",
            "Tom kann mit fast Allem umgehen.",
            "Dabei Übersah er sie.",
            "Das sind 10 Millionen Euro, Gleichzeitig und zusätzlich.",
            "Sie werden im Allgemeinen Fall gefasst.",
            "Das ist das Dümmste Kind.",
            "Er sagt, dass Geistliche Würdenträger davon betroffen sind.",
            "Er sagt, dass Geistliche und weltliche Würdenträger davon betroffen sind.",
            "Er ist begeistert Von der Fülle.",
            "Er wohnt Über einer Garage.",
            "Die Anderen 90 Prozent waren krank.",
            "Die Ausgewählten Leute werden gut betreut.",
            "Er war dort Im März 2000.",
            "Er war dort Im Jahr 96.",
            "Das ist es: Kein Satz.",
            "Wen magst du lieber: Die Giants oder die Dragons?",
            "Ich frage mich: Warum Das so ist.",
            "Das wirklich Wichtige Verfahren ist dies:",
            "Die Schöne Tür",
            "Das Blaue Auto.",
            "Ein Einfacher Satz zum Testen.",
            "Eine Einfache Frage zum Testen?",
            "Er kam Früher als sonst.",
            "Er rennt Schneller als ich.",
            "Das Winseln Stört.",
            "Sein verhalten war okay.",
            "Dem Norwegischen Ingenieur gelingt das gut.",
            "Das dabei erlernte und Erlebte Wissen ist sehr nützlich.",
            "Diese Regelung wurde als Überholt bezeichnet.",
            "Die Dolmetscherin und Der Vorleser gehen spazieren.",
            "Das sagen haben hier viele.",
            "Bis Bald!",
            "Das ist Eine Schreibweise.",
            "Das ist Ein Mann.",
            "Sie erhalten bald unsere Neuesten Insights.",
            "Das fahren ist einfach.",
            "Denn das fahren ist einfach.",
            "Denn das laufen ist einfach.",
            "Denn das essen ist einfach.",
            "Denn das gehen ist einfach.",
            "Das Große Auto wurde gewaschen.",
            "Ich habe ein Neues Fahrrad.",
        ]
        .into_par_iter()
        .filter_map(|text| {
            let matches = case_matches(text);
            (matches.len() != 1).then(|| {
                format!(
                    "{text}: Java CaseRuleTest expects exactly one DE_CASE match, got {matches:?}"
                )
            })
        })
        .collect();
        assert!(offenders.is_empty(), "{}", offenders.join("\n"));
    }

    #[test]
    fn java_divergences_pin_the_current_residue() {
        for text in [
            "Als Erstes würde ich sofort die Struktur ändern.",
            "Er sagte: Als Erstes würde ich sofort die Struktur ändern.",
            "Das wird Scholz' engster Vertraute Wolfgang Schmidt übernehmen.",
            "Das ist ein zwingendes Muss.",
        ] {
            let matches = case_matches(text);
            assert_eq!(
                matches.len(),
                1,
                "{text}: Java CaseRuleTest accepts these; the Rust guard chain still flags them"
            );
        }
        for text in [
            "…  jahrzehntelangen Mitstreitern und vielen Freunden aus Nah und Fern.",
            "Der Brief wird am Mittwoch in Brüssel Übergeben.",
            "Damit sollen sie die Versorgung in der Region Übernehmen.",
            "Die Unfallursache scheint geklärt, ein Lichtsignal wurde Überfahren.",
            "Der Lenker hatte die Höchstgeschwindigkeit um 76 km/h Überschritten.",
        ] {
            let matches = case_matches(text);
            assert!(
                matches.is_empty(),
                "{text}: Java CaseRuleTest flags the 'Über' participle / leading-ellipsis cases; \
                 the Rust tagger readings do not trip is_potential_uppercase_error here"
            );
        }
    }

    #[test]
    fn flags_mid_sentence_adjective() {
        let matches = case_matches("Ich habe Heute keine Zeit.");
        assert!(
            matches
                .iter()
                .any(|(msg, _, sugg)| msg.contains("großgeschrieben") && sugg == "heute"),
            "got {matches:?}"
        );
    }

    #[test]
    fn flags_substantivized_verb_lowercase() {
        let matches = case_matches("Das laufen fällt mir leicht.");
        assert!(
            matches
                .iter()
                .any(|(msg, _, sugg)| msg.contains("substantiviertes Verb") && sugg == "Laufen"),
            "got {matches:?}"
        );
    }

    #[test]
    fn accepts_correct_sentences() {
        for text in [
            "Das Laufen fällt mir leicht.",
            "Ein einfacher Satz zum Testen.",
            "Heute spricht Frau Stieg.",
            "3.1 Technische Dokumentation",
            "Er arbeitet im Bereich Präsidiales.",
            "Anmeldung bis Fr. 1.12.",
            "Eine Gruppe Aufständischer verwüstete die Bar.",
            "Das Winseln stört.",
            "Hier ein Satz. \"Ein Zitat.\"",
        ] {
            let matches = case_matches(text);
            assert!(matches.is_empty(), "{text}: got {matches:?}");
        }
    }
}
