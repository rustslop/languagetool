use crate::dictionary::Dictionary;
use crate::suggestion::SuggestionEngine;
use og_core::checker::Tagger;
use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchKind, RuleMatchRule, SuggestedReplacement,
};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

const MAX_TOKEN_LENGTH: usize = 200;
const MIN_WORD_LENGTH: usize = 5;
const MAX_WORD_LENGTH: usize = 40;
const SWISS: &str = "de-CH";

struct Patterns {
    missing_adj: Regex,
    adj_suffix: Regex,
    schaf: Regex,
    schafe: Regex,
    start_with_spiel: Regex,
    end_with_schafte: Regex,
    compound_typos: Regex,
    compound_end_typos: Regex,
    infix_s_suffixes: Regex,
    wechselnumerus: Regex,
    wechselinfix: Regex,
    confused_prefixes: Regex,
    needs_to_be_plural: Regex,
    subnomplufem_exceptions: Regex,
    person_suffixes: Regex,
    invalid_comp_part_1: Regex,
    invalid_comp_part_2: Regex,
    subinf_singular_object: Regex,
    arbeit_comp: Regex,
    bach_comp: Regex,
    bad_comp: Regex,
    link_comp: Regex,
    links_comp: Regex,
    recht_comp: Regex,
    rechts_comp: Regex,
    verband_comp: Regex,
    verbands_comp: Regex,
    wider_comp: Regex,
    wochentag_comp: Regex,
    wochentage: Regex,
    wochentage_s: Regex,
    welten_comp: Regex,
    woerter_comp: Regex,
    cities_exceptions: Regex,
    direction: Regex,
    first_upper_case: Regex,
    special_case: Regex,
    special_case_with_s: Regex,
    special_case_third: Regex,
    gender_neutral_sin: Regex,
    gender_neutral_plu: Regex,
    gender_neutral_slash_hyphen: Regex,
    gender_star_special_chrs: Regex,
    gender_star_slash_hyphen: Regex,
    is_needing_fugen_s: Regex,
    camel_case: Regex,
    eig_country_or_region: Regex,
    directional_adjective: Regex,
    elative_prefix: Regex,
    mitarbeitende: Regex,
}

fn anchored(pattern: &str) -> Regex {
    Regex::new(&format!("^(?:{pattern})$")).expect("valid regex")
}

impl Patterns {
    fn new() -> Patterns {
        let adj_suffix = "(affin|basiert|konform|widrig|fähig|haltig|bedingt|gerecht|würdig|relevant|übergreifend|tauglich|untauglich|artig|bezogen|orientiert|fremd|liebend|hassend|bildend|hemmend|abhängig|zentriert|förmig|mäßig|pflichtig|ähnlich|spezifisch|verträglich|technisch|typisch|frei|arm|freundlich|feindlich|gemäß|neutral|seitig|begeistert|geeignet|ungeeignet|berechtigt|sicher|süchtig|resistent|verachtend|schädigend)";
        Patterns {
            missing_adj: anchored(&format!("[a-zöäüß]{{3,25}}{adj_suffix}(er|es|en|em|e)?")),
            adj_suffix: Regex::new(&format!("{adj_suffix}(er|es|en|em|e)?")).expect("valid regex"),
            schaf: anchored(".{3,}schaf(s|en)?"),
            schafe: anchored("(Alpenschaf|Berberschaf|Bergschaf|Blauschaf|Brillenschaf|Dachsteinschaf|Deichschaf|Dickhornschaf|Feinwollschaf|Fettschwanzschaf|Fleischschaf|Fuchsschaf|Glücksschaf|Hausschaf|Jungschaf|Karakulschaf|Klonschaf|Merinoschaf|Milchschaf|Mondschaf|Nutzschaf|Rhönschaf|Riesenwildschaf|Schaukelschaf|Schneeschaf|Steinschaf|Steppenschaf|Superschaf|Waldschaf|Weideschaf|Wildschaf|Wollschaf|Zackelschaf|Zuchtschaf|Zwergblauschaf)(s|en)?"),
            start_with_spiel: anchored("Spielzugs?|Spielzugangs?|Spielzuganges|Spielzugbuchs?|Spielzugbüchern?|Spielzuges|Spielzugverluste?|Spielzugverluste[ns]"),
            end_with_schafte: anchored("[A-ZÖÄÜ][a-zöäß-]+schafte"),
            compound_typos: anchored("([Ee]mail|[Ii]reland|[Kk]reissaal|[Mm]akeup|[Ss]tandart).*"),
            compound_end_typos: anchored(".*(gruße|schaf(s|en)?)"),
            infix_s_suffixes: anchored(".*(heit|(s|[^c]t|x)ion|ität|keit|ling|ung|schaft|tum)"),
            wechselnumerus: anchored("wort|welt"),
            wechselinfix: anchored("(arbeit|dienstag|donnerstag|freitag|montag|mittwoch|link|recht|samstag|sonntag|verband)s?"),
            confused_prefixes: anchored("bach|bade?|wi(e)?der"),
            needs_to_be_plural: anchored("absolvent(in)?|adressat|aktie|antenne|apache|arbeitnehmer(in)?|ärztin|assistent(in)?|astronom(in)?|asylant(in)?|autor(in)?|azteke|bakterie|ballade|bauer|billion|bisexuelle|blume|bonze|börse|bot(e|in)|buche|bürg(e|in)|bürokrat(in)?|chrysantheme|dän(e|in)?|debatte|debitor(in)?|decke|diakon(in)?|diktator(in)?|direktor(in)?|doktorand(in)?|domäne|dozent(in)?|drohne|druid(e|in)?|düne|ehre|eibe|elefant|elektron|ellipse|emittent(in)?|elfe|elle|enge|erbse|eremit|erde|erste|esche|exot(e|in)?|expert(e|in)?|extremist(in)?|fabrikant(in)?|falke|fassade|farbe|fasan|favorit(in)?|felge|ferien|figur|fluor|frage|franz(ose|ösin)|frau|frisur|förde|galle|gatt(e|in)?|gerät|gepard|gezeit|gigant|gilde|göttin|griech(e|in)?|halt|heid(e|in)?|herde|historie|hölle|höhle|hose|hugenott(e|in)?|hund|hündin|immigrant(in)?|investor(in)?|irokes(e|in)|islamist(in)?|jesuit(in)?|jungfer|jungfrau|junggesell(e|in)|juror(in)?|kadett|kante|kaskade|kathode|katholik(in)?|katze|kette|kid|klasse|kirche|klaue|klient(in)?|klinge|knappe|koeffizient|kojote|komet|kommentator(in)?|komödie|kompliz(e|in)|konkurrent(in)?|konfirmand(in)?|konsonant|kontrahent(in)?|krake|kralle|kranke|krähe|kraut|krippe|kurd(e|in)|kuriosität|kurve|kusine|küste|laie|laterne|laute|legende|lehne|leise|lektor(in)?|leopard|lerche|leserin|lieferant(in)?|lippe|loge|lotse|länge|läuse|löwe|lücke|luke|made|mädel|maske|maßnahme|matriarchin|menge|mensch|metapher|methode|metropole|miene|miete|migrant(in)?|million|mitte|maus|moderator(in)?|monarch(in)?|mongol(e|in)|mormone|mücke|mühle|musikant(in)?|mysterium|nerv|niederlage|nixe|nonne|note|obdachlose|ode|organist|panne|papagei|parzelle|pastor(in)?|pate|patient|patriarch(in)?|petze|pfadfinderin|pfanne|pfaffe|pfau|pfeife|platte|polle|pomade|pomeranze|posse|praktikant(in)?|prinz(essin)?|prise|produzent(in)?|prominente|prophet(in)?|prototyp|prälat|psychopath(in)?|puppe|pädophile|pygmäe|rabe|radikale|rakete|rampe|ranke|rassist(in)?|rate|raupe|rendite|repressalie|rest|riese|rinde|rind|robbe|robe|romanist|rose|ross|route|nummer|runde|russ(e|in)?|röhre|rübe|salbe|schabe|schale|scheide|schelle|schenke|schere|sphäre|dicke|kröte|schauspielerin|schimpans(e|in)|schlampe|schlange|schluchte|schmiere|schnake|schnalle|schneide|schnelle|schokolade|schotte|schurke|schwabe|schwalbe|schwede|schwule|seele|seide|seite|senator(in)?|serb(e|in)?|serie|silbe|skulptur|sonne|sorge|sorte|spanne|sparte|spatz|sperre|spitze|sproße|spule|stalaktit|steppe|straße|streife|studie|stunde|stütze|tabelle|therapeut(in)?|tinte|tote|toilette|torte|traube|treffe|treppe|truhe|träne|tunte|tüte|tyrann|urne|utensil|vandal(e|in)|vasall(in)?|vene|versicherte|verwandte|veteran(in)?|virtuose|vorname|waffe|wanne|ware|watte|wehe|welle|welpe|wiese|wirtin|zar(in)?|zentrum|zutat"),
            subnomplufem_exceptions: anchored(".+(atie|lyse|metrie|sophie|omie)|absage|accessoire|allergie|analogie|anästhesie|anatomie|anomalie|archäologie|aufnahme|balance|barriere|batterie|creme|deponie|dürre|einlage|energie|folklore|franchise|gemeinde|glu[ck]ose|gülle|hanse|hefe|hirse|infanterie|kolonie|kontrolle|lounge|massage|mathe|melodie|nässe|parfümerie|pharmazie|pflege|pipeline|poesie|psychotherapie|regie|renaissance|säure|single|sprache|spree|stärke|suche|teilnahme|theorie|therapie|vanille|wiederaufnahme|wende"),
            person_suffixes: anchored(".+([dlt]ant(in)?|arch(in)?|graf(in)?|gog(e|in)|([bdfghnoptuvxyz]|he|([inrt]a|o)(l)|(for|ga|i)m|[^f]r)(ist)(in)?|krat(in)?|man(e|in)|naut(in)?|olog(e|in)|(d|[pt]i|r)ent(in)?|seur(in)?|soph(in)?|tit(in)?|.+[^b]urg(in)?)"),
            invalid_comp_part_1: anchored("adresse|ahmen|kamp|kontrolle|leuchte|norden|osten|perspektive|schule|sprache|stelle|suche|süden|westen"),
            invalid_comp_part_2: anchored("kamp"),
            subinf_singular_object: anchored("putzen|rauchen|sein|spielen"),
            arbeit_comp: anchored("(gebe|nehme)(r(s|n|innen|in)?|nde[mnr]?)"),
            bach_comp: anchored("aue|bett|biograph(ie|in)?|chor|forelle|gesellschaft|kantate|lauf|niederung|stelze|tal|verein"),
            bad_comp: anchored("accessoire|architektur|besitzer(in)?|design|eintritt|fenster|größe|grund|kollektion|konzept|lösung|maß|möbel|nutzer(in)?|regal|spezialist(in)?|spiegel|straße|tür|utensil"),
            link_comp: anchored("element|inhalt|liste|portal|text|titel|tracking|verzeichnis"),
            links_comp: anchored("abbieger(in)?|abweichler(in)?|anwalt|anwältin|anwaltschaft|ausfall|auslage|ausleger(in)?|au(ss|ß)en|bündnis|drall|drehung|extremer?|extremis(t|tin|mus)|faschis(t|tin|mus)|fraktion|galopp|gewinde|händ(er|erin|igkeit)|hörnchen|innen|intellektueller?|katholizis(t|tin|mus)|koalition|konter|kurs|kurve|lastigkeit|lenker|nationalis(t|tin|mus)|opposition|orientierung|partei|populis(t|tin|mus)|radikal(e|er|ismus|ist|istin)|regierung|ruck|rutsch|schnitt|schuss|schwenk(ung)?|sektierer(in)?|steuerung|terror(t|tin|ismus)|verbinder(in)?|verkehr|wendung|wichser"),
            recht_comp: anchored("bank|eck|fertigung|gläubigkeit|haber|haberei|leitung|losigkeit|mäßigkeit|winkligkeit|zeitigkeit"),
            rechts_comp: anchored("abbieger(in)?|abteilung|akt|akte|angelegenheit|ansicht|anspruch|anwalt|anwalts|anwaltschaft|anwendung|anwältin|auffassung|aufsicht|auskunft|ausleger(in)?|ausschuss|au(ss|ß)en|begehren|begriff|behelf|beistand|berater|beratung|bereich|beschwerde|beugung|beziehung|brecher|bruch|dienst|drall|durchsetzung|empfinden|entwicklung|setzung|experte|experten|extremer?|extremis(t|tin|mus)|fall|fehler|folge|form|fortbildung|frage|fähigkeit|gebiet|gebieten|gelehrte|gelehrter|geschichte|geschäft|gewinde|gleichheit|grund|grundlage|grundsatz|gründen|gut|gutachten|gültigkeit|güter|handlung|hilfe|händ(er|erin|igkeit)|hängigkeit|inhaber|institut|katholizis(t|tin|mus)|klick|konformität|kraft|kreis|kurve|lage|lastigkeit|lehre|lenker|medizin|mediziner|meinung|missbrauch|mittel|mitteln|mängel|nachfolge|nachfolger|nachfolgerin|nationalis(t|tin|mus)|natur|norm|ordnung|persönlichkeit|pflege|pfleger|pflicht|philosophie|politik|populis(t|tin|mus)|position|praxis|problem|quelle|radikal(e|er|ismus|ist|istin)|rahmen|rat|ratgeber(in)?|ruck|rutsch|sache|sachen|satz|schutz|sicherheit|sinn|sprache|soziologie|sprechung|staat|staatlichkeit|stand|status|stellung|streit|streitigkeit|system|terroris(t|tin|ismus)|texte|texter|thema|theorie|tipp|titel|träger|unsicherheit|verfolgung|vergleichung|verhältnis|verkehr|verletzung|verletzungen|verordnung|verstoß|verständnis|verteidiger|verteidigung|vertreter|vertretung|vorschrift|wahl|weg|wesen|widrigkeit|wirksamkeit|wirkung|wissenschaft|wissenschaften|wissenschaftler|zug|änderung"),
            verband_comp: anchored("klammer|kasten|kiste|mull|material|päckchen|platz|raum|schere|zeug|zimmer"),
            verbands_comp: anchored("chef(in)?|flug|funktionär(in)?|kasse|klage|leben|leitung|leiter(in)?|ligist(in)|material|päckchen|präsident(in)?|presse|spiel|vertreter(in)|vorsitzender?|vorstand|wechsel|zeichen|zeit(schrift|ung)"),
            wider_comp: anchored("glanz|hall|haken|klage|klang|lager|rechtliche|ruf|spiel|spruch|sehen|stand|streit|wille"),
            wochentag_comp: anchored("abend|mittag|morgen|nachmittag|vormittag"),
            wochentage: anchored("Montag|Dienstag|Mittwoch|Donnerstag|Freitag|Samstag|Sonntag"),
            wochentage_s: anchored("(Montag|Dienstag|Mittwoch|Donnerstag|Freitag|Samstag|Sonntag)s"),
            welten_comp: anchored("Brand|Bummler(in)?|Drama|Wende"),
            woerter_comp: anchored("Buch|Liste|Verzeichnis"),
            cities_exceptions: anchored("bahnhof|flughafen|haupt"),
            direction: anchored("nord|ost|süd|west"),
            first_upper_case: anchored("[A-ZÖÄÜ][a-zöäüß-]+"),
            special_case: anchored(".{3,25}(tum|ing|ling|heit|keit|schaft|ung|ion|tät|at|um)"),
            special_case_with_s: anchored(".{3,25}(tum|ing|ling|heit|keit|schaft|ung|ion|tät|at|um)s"),
            special_case_third: anchored("[A-ZÖÄÜ][a-zöäüß]{2,}(ei|öl)"),
            gender_neutral_sin: anchored(".*[*:_/]in"),
            gender_neutral_plu: anchored(".*[*:_/]in.+"),
            gender_neutral_slash_hyphen: anchored(".*/-in.+"),
            gender_star_special_chrs: Regex::new("[*:_/]in").expect("valid regex"),
            gender_star_slash_hyphen: Regex::new("/-in").expect("valid regex"),
            is_needing_fugen_s: anchored(".*(tum|ling|ion|tät|keit|schaft|sicht|ung|en)"),
            camel_case: anchored(".*(\\p{Ll}\\p{Lu}|\\p{Lu}{2,}\\p{Ll}).*"),
            eig_country_or_region: Regex::new(".+(COU|GEB|STD|WAT)").expect("valid regex"),
            directional_adjective: anchored(".+ische?[mnrs]?"),
            elative_prefix: Regex::new("^(bitter|dunkel|erz|extra|früh|gemein|grund|hyper|lau|mega|minder|stock|super|tod|ultra|ur|voll)").expect("valid regex"),
            mitarbeitende: Regex::new("mitarbeitenden?").expect("valid regex"),
        }
    }
}

static PATTERNS: LazyLock<Patterns> = LazyLock::new(Patterns::new);

fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn lowercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn starts_with_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_uppercase)
}

fn starts_with_lowercase(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_lowercase)
}

fn is_all_uppercase(s: &str) -> bool {
    !s.is_empty() && s.chars().all(char::is_uppercase)
}

fn is_all_lowercase(s: &str) -> bool {
    !s.is_empty() && s.chars().all(char::is_lowercase)
}

fn cut_off_dot(s: &str) -> &str {
    s.strip_suffix('.').unwrap_or(s)
}

fn remove_trailing_s(part: &str) -> &str {
    part.strip_suffix('s').unwrap_or(part)
}

fn remove_trailing_hyphen(part: &str) -> &str {
    part.strip_suffix('-').unwrap_or(part)
}

fn remove_trailing_s_and_hyphen(part: &str) -> &str {
    remove_trailing_s(remove_trailing_hyphen(part))
}

fn is_ascii_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn gender_star_find(s: &str) -> Option<usize> {
    for (idx, _) in s.char_indices() {
        let rest = &s[idx..];
        if rest.starts_with("In") && s[..idx].chars().next_back().is_some_and(is_ascii_word_char) {
            return Some(idx);
        }
        if rest.starts_with("*in") || rest.starts_with(":in") || rest.starts_with("_in") {
            return Some(idx);
        }
        if rest.starts_with("/-in") {
            return Some(idx);
        }
    }
    None
}

fn gender_star_match_len(s: &str, idx: usize) -> usize {
    let rest = &s[idx..];
    if rest.starts_with("/-in") {
        4
    } else if rest.starts_with("*in") || rest.starts_with(":in") || rest.starts_with("_in") {
        3
    } else {
        2
    }
}

fn replace_first_gender_star(s: &str) -> String {
    match gender_star_find(s) {
        Some(idx) => {
            let len = gender_star_match_len(s, idx);
            format!("{}in{}", &s[..idx], &s[idx + len..])
        }
        None => s.to_string(),
    }
}

fn has_binnen_i(s: &str) -> bool {
    s.char_indices().any(|(idx, _)| {
        s[idx..].starts_with("In") && s[..idx].chars().next_back().is_some_and(is_ascii_word_char)
    })
}

fn replace_first_binnen_i(s: &str) -> String {
    for (idx, _) in s.char_indices() {
        if s[idx..].starts_with("In")
            && s[..idx].chars().next_back().is_some_and(is_ascii_word_char)
        {
            return format!("{}in{}", &s[..idx], &s[idx + "In".len()..]);
        }
    }
    s.to_string()
}

#[derive(Clone)]
struct GermanSharedData {
    tagger: Option<Arc<dyn Tagger>>,
    ignore_words: HashSet<String>,
    ignore_words_ch: HashSet<String>,
    words_to_be_ignored_in_compounds: HashSet<String>,
    word_starts_to_be_prohibited: Vec<String>,
    word_endings_to_be_prohibited: Vec<String>,
    words_to_be_prohibited: HashSet<String>,
    words_needing_infix_s: HashSet<String>,
    verb_stems: HashSet<String>,
    verb_prefixes: HashSet<String>,
    other_prefixes: HashSet<String>,
    old_spelling: HashSet<String>,
}

fn clean_tags_and_escape_chars(s: &str) -> String {
    let cut = s.split('#').next().unwrap_or("");
    cut.replace('\\', "").trim().to_string()
}

fn add_expanded(result: &mut Vec<String>, word: String) {
    if !word.is_empty() && !result.contains(&word) {
        result.push(word);
    }
}

fn expand_line(line: &str) -> Vec<String> {
    let flag_idx = line.find('/');
    let is_flag_line = !line.starts_with('#') && flag_idx.is_some_and(|i| i > 0);
    let underscore_idx = line.find('_');
    let is_underscore_line = !line.starts_with('#') && underscore_idx.is_some_and(|i| i > 0);
    if is_underscore_line {
        let cleaned = clean_tags_and_escape_chars(line);
        let parts: Vec<&str> = cleaned.split('_').collect();
        if parts.len() != 2 || parts[0].contains('/') || parts[1].contains('/') {
            return vec![cleaned];
        }
        let (first, second) = (parts[0], parts[1]);
        let mut result = Vec::new();
        if second == "in" {
            for joiner in ["_in", "_innen", "*in", "*innen", ":in", ":innen"] {
                result.push(format!("{first}{joiner}"));
            }
        } else {
            result.push(format!("{first}{second}"));
            result.push(format!("{first}zu{second}"));
            result.push(format!("{}{second}s", uppercase_first_char(first)));
        }
        return result;
    }
    if is_flag_line {
        let cleaned = clean_tags_and_escape_chars(line);
        let parts: Vec<&str> = cleaned.split('/').collect();
        if parts.len() != 2 {
            return vec![cleaned];
        }
        let (word, suffix) = (parts[0], parts[1]);
        let mut result = Vec::new();
        for c in suffix.chars() {
            match c {
                'S' => {
                    add_expanded(&mut result, word.to_string());
                    add_expanded(&mut result, format!("{word}s"));
                }
                'N' => {
                    add_expanded(&mut result, word.to_string());
                    add_expanded(&mut result, format!("{word}n"));
                }
                'E' => {
                    add_expanded(&mut result, word.to_string());
                    add_expanded(&mut result, format!("{word}e"));
                }
                'F' => {
                    add_expanded(&mut result, word.to_string());
                    add_expanded(&mut result, format!("{word}in"));
                }
                'T' => {
                    add_expanded(&mut result, word.to_string());
                    if let Some(stripped) =
                        word.strip_suffix("straße").or(word.strip_suffix("strasse"))
                    {
                        add_expanded(&mut result, format!("{stripped}str."));
                    } else if let Some(stripped) =
                        word.strip_suffix("Straße").or(word.strip_suffix("Strasse"))
                    {
                        add_expanded(&mut result, format!("{stripped}Str."));
                    }
                }
                'A' | 'P' => {
                    add_expanded(&mut result, word.to_string());
                    if word.ends_with('e') {
                        for ending in ["r", "s", "n", "m"] {
                            add_expanded(&mut result, format!("{word}{ending}"));
                        }
                    } else {
                        for ending in ["e", "er", "es", "en", "em"] {
                            add_expanded(&mut result, format!("{word}{ending}"));
                        }
                    }
                }
                _ => {}
            }
        }
        return result;
    }
    vec![clean_tags_and_escape_chars(line)]
}

fn load_word_file(path: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn load_set_file(path: &Path, set: &mut HashSet<String>) {
    for line in load_word_file(path) {
        set.insert(line.trim().split(';').next().unwrap_or("").to_string());
    }
}

fn shared_german_data(resource_dir: &Path, data: GermanSharedData) -> Arc<GermanSharedData> {
    static CACHE: LazyLock<Mutex<HashMap<std::path::PathBuf, std::sync::Weak<GermanSharedData>>>> =
        LazyLock::new(Default::default);
    let mut cache = CACHE.lock().unwrap();
    if let Some(existing) = cache.get(resource_dir).and_then(std::sync::Weak::upgrade) {
        return existing;
    }
    let shared = Arc::new(data);
    cache.insert(
        resource_dir.to_path_buf(),
        std::sync::Arc::downgrade(&shared),
    );
    shared
}

pub struct GermanSpeller {
    dictionary: Arc<Dictionary>,
    suggestion_engine: SuggestionEngine,
    shared: Arc<GermanSharedData>,
    swiss: bool,
    hunspell: Option<Arc<crate::hunspell::HunspellChecker>>,
    cache: Mutex<HashMap<String, bool>>,
}

impl GermanSpeller {
    pub fn new(
        dictionary: Arc<Dictionary>,
        resource_dir: &Path,
        core_resource_dir: Option<&Path>,
        tagger: Option<Arc<dyn Tagger>>,
        variant: &str,
    ) -> Self {
        let mut ignore_words: HashSet<String> = HashSet::new();
        let mut ignore_words_ch: HashSet<String> = HashSet::new();
        let mut ignored_in_compounds: HashSet<String> = HashSet::new();
        let mut prohibit_starts: Vec<String> = Vec::new();
        let mut prohibit_ends: Vec<String> = Vec::new();
        let mut prohibit_exact: HashSet<String> = HashSet::new();

        let hunspell_dir = resource_dir.join("hunspell");
        let mut ignore_sources = vec![
            hunspell_dir.join("ignore.txt"),
            hunspell_dir.join("spelling.txt"),
            hunspell_dir.join("spelling_custom.txt"),
        ];
        if let Some(core) = core_resource_dir {
            ignore_sources.push(core.join("spelling_global.txt"));
        }
        for source in &ignore_sources {
            for line in load_word_file(source) {
                let swiss_line = line.replace('ß', "ss");
                let line = if variant == SWISS {
                    swiss_line.clone()
                } else {
                    line
                };
                if let Some(stripped) = line.strip_suffix("-*") {
                    ignored_in_compounds.insert(stripped.to_string());
                    continue;
                }
                for word in expand_line(&line) {
                    ignore_words.insert(word.clone());
                    if variant != SWISS {
                        let ch_form = if word.contains('ß') {
                            word.replace('ß', "ss")
                        } else {
                            word.clone()
                        };
                        ignore_words_ch.insert(ch_form);
                    }
                }
            }
        }

        for source in [
            hunspell_dir.join("prohibit.txt"),
            hunspell_dir.join("prohibit_custom.txt"),
        ] {
            for line in load_word_file(&source) {
                let words = expand_line(&line);
                if words.len() == 1 && words[0].ends_with(".*") {
                    prohibit_starts.push(words[0][..words[0].len() - 2].to_string());
                } else if words.first().is_some_and(|w| w.starts_with(".*")) {
                    for word in words {
                        prohibit_ends.push(word[2..].to_string());
                    }
                } else {
                    prohibit_exact.extend(words);
                }
            }
        }

        let mut words_needing_infix_s = HashSet::new();
        let mut verb_stems = HashSet::new();
        let mut verb_prefixes = HashSet::new();
        let mut other_prefixes = HashSet::new();
        let mut old_spelling = HashSet::new();
        load_set_file(
            &resource_dir.join("words_infix_s.txt"),
            &mut words_needing_infix_s,
        );
        load_set_file(&resource_dir.join("verb_stems.txt"), &mut verb_stems);
        load_set_file(&resource_dir.join("verb_prefixes.txt"), &mut verb_prefixes);
        load_set_file(
            &resource_dir.join("other_prefixes.txt"),
            &mut other_prefixes,
        );
        load_set_file(&resource_dir.join("alt_neu.csv"), &mut old_spelling);

        let suggestion_engine = SuggestionEngine::new(std::sync::Arc::clone(&dictionary));
        Self {
            dictionary,
            suggestion_engine,
            shared: shared_german_data(
                resource_dir,
                GermanSharedData {
                    tagger,
                    ignore_words,
                    ignore_words_ch,
                    words_to_be_ignored_in_compounds: ignored_in_compounds,
                    word_starts_to_be_prohibited: prohibit_starts,
                    word_endings_to_be_prohibited: prohibit_ends,
                    words_to_be_prohibited: prohibit_exact,
                    words_needing_infix_s,
                    verb_stems,
                    verb_prefixes,
                    other_prefixes,
                    old_spelling,
                },
            ),
            swiss: variant == SWISS,
            hunspell: None,
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_hunspell_core(mut self, checker: Arc<crate::hunspell::HunspellChecker>) -> Self {
        self.hunspell = Some(checker);
        self
    }

    pub fn with_shared_data(&self, dictionary: Arc<Dictionary>, variant: &str) -> Self {
        let suggestion_engine = SuggestionEngine::new(std::sync::Arc::clone(&dictionary));
        Self {
            dictionary,
            suggestion_engine,
            shared: self.shared.clone(),
            swiss: variant == SWISS,
            hunspell: self.hunspell.clone(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_extra_ignore_words(&self, extra: Vec<String>) -> Self {
        let mut data = (*self.shared).clone();
        for word in extra {
            let ch_form = if word.contains('ß') {
                word.replace('ß', "ss")
            } else {
                word.clone()
            };
            data.ignore_words.insert(word);
            data.ignore_words_ch.insert(ch_form);
        }
        Self {
            dictionary: self.dictionary.clone(),
            suggestion_engine: SuggestionEngine::new(Arc::clone(&self.dictionary)),
            shared: Arc::new(data),
            swiss: self.swiss,
            hunspell: self.hunspell.clone(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn dictionary(&self) -> &Arc<Dictionary> {
        &self.dictionary
    }

    fn active_ignore_words(&self) -> &HashSet<String> {
        if self.swiss {
            &self.shared.ignore_words_ch
        } else {
            &self.shared.ignore_words
        }
    }

    pub fn suggestions(&self, word: &str, max: usize) -> Vec<String> {
        self.suggestion_engine.suggest(word, max)
    }

    fn is_in_ignored_set(&self, word: &str) -> bool {
        self.active_ignore_words().contains(word)
    }

    fn is_ignored_no_case(&self, word: &str) -> bool {
        self.is_in_ignored_set(word)
            || (PATTERNS.first_upper_case.is_match(word)
                && self.is_in_ignored_set(&word.to_lowercase()))
            || word.chars().count() <= 1
    }

    fn ignore_word_base(&self, word: &str) -> bool {
        if word.chars().count() > MAX_TOKEN_LENGTH {
            return true;
        }
        if !word.chars().any(|c| c.is_alphabetic()) {
            return true;
        }
        if word.ends_with('.') && !self.is_in_ignored_set(word) {
            return self.is_ignored_no_case(cut_off_dot(word));
        }
        self.is_ignored_no_case(word)
    }

    pub fn filter_is_misspelled(&self, word: &str) -> bool {
        self.special_cases_misspelled(word) || self.hunspell_rule_is_misspelled(word)
    }

    fn hunspell_rule_is_misspelled(&self, word: &str) -> bool {
        let is_alphabetic = if word.chars().count() == 1 {
            word.chars().next().is_some_and(char::is_alphabetic)
        } else {
            true
        };
        (is_alphabetic && word != "--" && !self.spell(word) && !self.ignore_word_base(word))
            || self.is_prohibited(cut_off_dot(word))
    }

    fn is_prohibited(&self, word: &str) -> bool {
        self.shared.words_to_be_prohibited.contains(word)
            || self
                .shared
                .word_starts_to_be_prohibited
                .iter()
                .any(|w| word.starts_with(w.as_str()))
            || self
                .shared
                .word_endings_to_be_prohibited
                .iter()
                .any(|w| word.ends_with(w.as_str()))
    }

    fn spell(&self, word: &str) -> bool {
        match &self.hunspell {
            Some(checker) => checker.spell(word),
            None => self.dictionary.contains(word),
        }
    }

    fn special_cases_misspelled(&self, word: &str) -> bool {
        if PATTERNS.schaf.is_match(word) && !PATTERNS.schafe.is_match(word) {
            let variant = word
                .strip_suffix("schaf")
                .map(|w| format!("{w}schaft"))
                .or_else(|| word.strip_suffix("schafs").map(|w| format!("{w}schaft")))
                .or_else(|| word.strip_suffix("schafen").map(|w| format!("{w}schaften")))
                .unwrap_or_else(|| word.to_string());
            if !self.base_is_misspelled(&variant) {
                return true;
            }
        }
        if word.starts_with("Spielzug") && !PATTERNS.start_with_spiel.is_match(word) {
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
        word.ends_with("schafte") && PATTERNS.end_with_schafte.is_match(word)
    }

    fn hunspell_core_misspelled(&self, word: &str) -> bool {
        let is_alphabetic = if word.chars().count() == 1 {
            word.chars().next().is_some_and(char::is_alphabetic)
        } else {
            true
        };
        is_alphabetic && word != "--" && !self.spell(word) && !self.ignore_word_base(word)
    }

    fn hunspell_is_misspelled(&self, word: &str) -> bool {
        self.hunspell_core_misspelled(word) || self.is_prohibited(cut_off_dot(word))
    }

    fn base_is_misspelled(&self, word: &str) -> bool {
        self.special_cases_misspelled(word) || self.hunspell_is_misspelled(word)
    }

    pub fn is_misspelled(&self, word: &str) -> bool {
        if let Some(hit) = self.cache.lock().unwrap().get(word) {
            return *hit;
        }
        let result =
            self.base_is_misspelled(word) && !self.ignore_potentially_misspelled_word(word);
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= 8192 {
            cache.clear();
        }
        cache.insert(word.to_string(), result);
        result
    }

    pub fn is_misspelled_strict(&self, word: &str) -> bool {
        self.special_cases_misspelled(word)
            || self.is_prohibited(cut_off_dot(word))
            || (self.hunspell_core_misspelled(word) && !self.compound_parts_spellable(word))
    }

    fn compound_parts_spellable(&self, word: &str) -> bool {
        if word.contains('-') {
            return false;
        }
        let parts = og_tagger::jwordsplitter::tokenize(word);
        parts.len() >= 2
            && parts.iter().all(|part| {
                let capitalized = uppercase_first_char(part);
                self.spell(part)
                    || self.spell(&part.to_lowercase())
                    || (capitalized != *part && self.spell(&capitalized))
            })
    }

    fn tag_word(&self, word: &str) -> Vec<AnalyzedToken> {
        let Some(tagger) = &self.shared.tagger else {
            return Vec::new();
        };
        let mut tokens = vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("", 0, 0)),
            AnalyzedTokenReadings::new(AnalyzedToken::new(word, 0, word.len())),
        ];
        tagger.tag(&mut tokens);
        tokens
            .into_iter()
            .nth(1)
            .map(|atr| atr.into_readings())
            .unwrap_or_default()
    }

    fn any_tag<F: Fn(&str) -> bool>(&self, word: &str, accepts: F) -> bool {
        self.tag_word(word)
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| accepts(t)))
    }

    fn is_noun(&self, word: &str) -> bool {
        self.any_tag(word, |t| t.starts_with("SUB:"))
    }

    fn is_adjective(&self, word: &str) -> bool {
        self.any_tag(word, |t| t.starts_with("ADJ:"))
    }

    fn is_only_noun(&self, word: &str) -> bool {
        let readings = self.tag_word(word);
        !readings.is_empty()
            && readings.iter().all(|r| {
                !r.pos_tags().is_empty() && r.pos_tags().iter().all(|t| t.starts_with("SUB:"))
            })
    }

    fn is_noun_nom(&self, word: &str) -> bool {
        self.any_tag(word, |t| t.starts_with("SUB:NOM"))
    }

    fn is_noun_nom_sin(&self, word: &str) -> bool {
        self.any_tag(word, |t| t.starts_with("SUB:NOM:SIN"))
    }

    fn is_noun_nom_plu(&self, word: &str) -> bool {
        self.any_tag(word, |t| t.starts_with("SUB:NOM:PLU"))
    }

    fn is_country_or_region_nom_sin(&self, word: &str) -> bool {
        self.any_tag(word, |t| {
            t.starts_with("EIG:NOM:SIN") && PATTERNS.eig_country_or_region.is_match(t)
        })
    }

    fn is_sub_ver_inf(&self, word: &str) -> bool {
        self.any_tag(word, |t| t.starts_with("SUB:") && t.ends_with(":INF"))
    }

    fn find_lemma_for_noun(&self, word: &str) -> String {
        let readings = self.tag_word(&uppercase_first_char(word));
        let mut lemma = String::new();
        let first_lemma = readings.first().and_then(|r| r.lemma().map(str::to_string));
        for reading in &readings {
            if reading.pos_tags().iter().any(|t| t.starts_with("SUB")) {
                lemma = first_lemma.clone().unwrap_or_default();
            }
        }
        lemma
    }

    fn is_verb_stem(&self, word: &str) -> bool {
        self.shared.verb_stems.contains(&lowercase_first_char(word))
    }

    fn is_verb_prefix(&self, word: &str) -> bool {
        self.shared
            .verb_prefixes
            .contains(&lowercase_first_char(word))
    }

    fn is_other_prefix(&self, word: &str) -> bool {
        self.shared
            .other_prefixes
            .contains(&lowercase_first_char(word))
    }

    fn needs_infix_s(&self, word: &str) -> bool {
        if PATTERNS.infix_s_suffixes.is_match(word)
            && self.any_tag(word, |t| t.starts_with("SUB:NOM:SIN:FEM"))
        {
            return true;
        }
        self.shared.words_needing_infix_s.contains(word)
    }

    fn needs_to_be_plural(&self, lemma: &str) -> bool {
        if PATTERNS.needs_to_be_plural.is_match(lemma) {
            return true;
        }
        if self.any_tag(&uppercase_first_char(lemma), |t| {
            t.starts_with("SUB:NOM:SIN:FE")
        }) && lemma.ends_with('e')
            && !PATTERNS.subnomplufem_exceptions.is_match(lemma)
        {
            return true;
        }
        PATTERNS.person_suffixes.is_match(lemma)
    }

    fn starts_with_ignored_word(&self, word: &str) -> usize {
        if word.chars().count() < 4 {
            return 0;
        }
        let mut offsets: Vec<usize> = word.char_indices().map(|(i, _)| i).collect();
        offsets.push(word.len());
        for len in (1..offsets.len()).rev() {
            let candidate = &word[..offsets[len]];
            if self.active_ignore_words().contains(candidate) {
                return candidate.chars().count();
            }
        }
        0
    }

    fn is_needing_fugen_s(&self, word: &str) -> bool {
        PATTERNS.is_needing_fugen_s.is_match(word)
    }

    fn ignore_elative(&self, word: &str) -> bool {
        if [
            "bitter", "dunkel", "erz", "extra", "früh", "gemein", "hyper", "lau", "mega", "minder",
            "stock", "super", "tod", "ultra", "ur",
        ]
        .iter()
        .any(|p| word.starts_with(p))
        {
            let last_part = PATTERNS.elative_prefix.replace(word, "").to_string();
            return last_part.chars().count() >= 3 && !self.base_is_misspelled(&last_part);
        }
        false
    }

    fn is_ignored_word_or_compound(&self, w: &str) -> bool {
        self.is_in_ignored_set(w) || self.shared.words_to_be_ignored_in_compounds.contains(w)
    }

    fn ignore_compound_with_ignored_word(&self, word: &str) -> bool {
        if !starts_with_uppercase(word)
            && !["nord", "west", "ost", "süd"]
                .iter()
                .any(|p| word.starts_with(p))
        {
            return false;
        }
        let parts: Vec<&str> = word.split('-').collect();
        if parts.len() < 2 {
            let mut end = self.starts_with_ignored_word(word);
            if end < 3 {
                if word.starts_with("ost") || word.starts_with("süd") {
                    end = 3;
                } else if word.starts_with("west") || word.starts_with("nord") {
                    end = 4;
                } else {
                    return false;
                }
            }
            let ignored_word = &word[..end];
            let partial_word = cut_off_dot(&word[end..]);
            let is_noun = self.is_noun(partial_word);
            let mut is_uppercase_noun = false;
            if !is_noun && !starts_with_uppercase(partial_word) {
                is_uppercase_noun = self.is_noun(&uppercase_first_char(partial_word));
            }
            let is_direction = PATTERNS.direction.is_match(ignored_word);
            let is_adjective = self.is_adjective(ignored_word);
            let is_directional_adjective = is_direction
                && (is_adjective || PATTERNS.directional_adjective.is_match(partial_word));
            let is_candidate = (is_directional_adjective || is_noun || is_uppercase_noun)
                && !is_all_uppercase(ignored_word)
                && (is_all_lowercase(partial_word) || ignored_word.ends_with('-'));
            let need_fugen_s = self.is_needing_fugen_s(ignored_word);
            if is_candidate && partial_word.chars().count() > 2 {
                let partial = if need_fugen_s {
                    partial_word.strip_prefix('s').unwrap_or(partial_word)
                } else {
                    partial_word
                };
                return self.spell(partial) || self.spell(&uppercase_first_char(partial));
            }
            return false;
        }
        let mut has_ignored_word = false;
        let mut to_spell_check: Vec<&str> = Vec::new();
        let strip_first = &word[parts[0].len() + 1..];
        let strip_last = &word[..word.len() - parts[parts.len() - 1].len() - 1];
        if self.is_ignored_word_or_compound(strip_first) {
            has_ignored_word = true;
            if !self.is_ignored_word_or_compound(parts[0]) {
                to_spell_check.push(parts[0]);
            }
        } else if self.is_ignored_word_or_compound(strip_last) {
            has_ignored_word = true;
            if !self.is_ignored_word_or_compound(parts[parts.len() - 1]) {
                to_spell_check.push(parts[parts.len() - 1]);
            }
        } else {
            for part in &parts {
                if self.is_ignored_word_or_compound(part) {
                    has_ignored_word = true;
                } else {
                    to_spell_check.push(part);
                }
            }
        }
        if has_ignored_word {
            for w in &to_spell_check {
                if !self.spell(w) {
                    return false;
                }
            }
        }
        has_ignored_word
    }

    fn ignore_by_hanging_hyphen(&self, words: &[&str], idx: usize) -> bool {
        let word = words[idx];
        let next_word = words[idx + 1..]
            .iter()
            .copied()
            .find(|w| {
                !(w.ends_with('-')
                    || matches!(
                        *w,
                        "," | "/"
                            | "&"
                            | "und"
                            | "oder"
                            | "bzw."
                            | "beziehungsweise"
                            | "sowie"
                            | "statt"
                    )
                    || w.trim().is_empty())
            })
            .map(cut_off_dot);
        let Some(next_word) = next_word else {
            return false;
        };
        let is_compound = og_tagger::jwordsplitter::tokenize(next_word).len() > 1
            || next_word.find('-').is_some_and(|i| i > 0)
            || PATTERNS.special_case_third.is_match(next_word);
        if is_compound {
            let word = remove_trailing_hyphen(word);
            let mut misspelled = !self.spell(word);
            if misspelled && self.is_ignored_word_or_compound(word) {
                misspelled = false;
            } else if misspelled
                && word.ends_with('s')
                && self.is_needing_fugen_s(remove_trailing_s(word))
            {
                misspelled = !self.spell(remove_trailing_s(word));
            }
            return !misspelled;
        }
        false
    }

    pub fn ignore_word_in_sentence(&self, words: &[&str], idx: usize) -> bool {
        let word = words[idx];
        if word.chars().count() > MAX_TOKEN_LENGTH {
            return true;
        }
        let ignore = self.ignore_word_base(word);
        let ignore_uncapitalized =
            !ignore && idx == 0 && self.ignore_word_base(&lowercase_first_char(word));
        let mut ignore_by_hyphen = false;
        if !ignore && !ignore_uncapitalized && word.contains('-') && word.ends_with('-') {
            ignore_by_hyphen = self.ignore_by_hanging_hyphen(words, idx);
        }
        let ignore_hyphenated_compound =
            !ignore_by_hyphen && self.ignore_compound_with_ignored_word(word);
        if PATTERNS.missing_adj.is_match(word) {
            let first_part = uppercase_first_char(&PATTERNS.adj_suffix.replace(word, ""));
            if self.base_is_misspelled(word)
                && ((!self.base_is_misspelled(&first_part)
                    && !PATTERNS.special_case.is_match(&first_part)
                    && self.is_only_noun(&first_part)
                    && !self.base_is_misspelled(&format!("{first_part}test")))
                    || (first_part.ends_with('s')
                        && !self.base_is_misspelled(remove_trailing_s(&first_part))
                        && PATTERNS.special_case_with_s.is_match(&first_part)
                        && self.is_only_noun(remove_trailing_s(&first_part))
                        && !self.base_is_misspelled(&format!("{first_part}test"))))
            {
                return true;
            }
        }
        if word.ends_with("mitarbeitende") || word.ends_with("mitarbeitenden") {
            let replaced = PATTERNS
                .mitarbeitende
                .replace(word, "mitarbeiter")
                .to_string();
            if self.spell(&replaced) {
                return true;
            }
        }
        ignore
            || ignore_uncapitalized
            || ignore_by_hyphen
            || ignore_hyphenated_compound
            || self.ignore_elative(word)
    }

    fn is_valid_word_length(&self, word: &str) -> bool {
        let len = word.chars().count();
        len <= MIN_WORD_LENGTH || len >= MAX_WORD_LENGTH
    }

    fn is_probably_typo(&self, word: &str) -> bool {
        PATTERNS.compound_typos.is_match(word) || PATTERNS.compound_end_typos.is_match(word)
    }

    fn is_valid_part_length(parts: &[String]) -> bool {
        if parts.len() == 2 {
            return parts[0].chars().count() >= 3 && parts[1].chars().count() >= 4;
        }
        if parts.len() == 3 {
            return parts[0].chars().count() >= 3
                && parts[1].chars().count() >= 4
                && parts[2].chars().count() >= 4;
        }
        false
    }

    fn is_old_spelling(&self, parts: &[String]) -> bool {
        for part in parts {
            let cleaned = remove_trailing_s_and_hyphen(part);
            if part.ends_with('s') {
                if self
                    .shared
                    .old_spelling
                    .contains(&uppercase_first_char(part))
                    || self
                        .shared
                        .old_spelling
                        .contains(&uppercase_first_char(cleaned))
                    || self
                        .shared
                        .old_spelling
                        .contains(&lowercase_first_char(part))
                    || self
                        .shared
                        .old_spelling
                        .contains(&lowercase_first_char(cleaned))
                {
                    return true;
                }
            } else if self
                .shared
                .old_spelling
                .contains(&uppercase_first_char(cleaned))
                || self
                    .shared
                    .old_spelling
                    .contains(&lowercase_first_char(cleaned))
            {
                return true;
            }
        }
        false
    }

    fn is_valid_gender_neutral_word(&self, parts: &[String], word: &str) -> bool {
        let mut start = 0usize;
        for part in parts {
            if start + part.len() > word.len() {
                return true;
            }
            let mut end = start + part.len();
            let Some(to_check) = word.get(start..end) else {
                return true;
            };
            if to_check.starts_with('I') && start > 0 {
                return false;
            }
            if has_binnen_i(to_check)
                && (self.base_is_misspelled(&replace_first_binnen_i(to_check))
                    || (!to_check.ends_with("In") && !to_check.ends_with("Innen")))
            {
                return false;
            }
            if PATTERNS.gender_neutral_sin.is_match(to_check) {
                let replaced = PATTERNS
                    .gender_star_special_chrs
                    .replace(to_check, "in")
                    .to_string();
                if self.base_is_misspelled(&replaced) {
                    return false;
                }
                end += 1;
            }
            if PATTERNS.gender_neutral_plu.is_match(to_check) && end < word.len() {
                end += 1;
                if let Some(extended) = word.get(start..end) {
                    let replaced = PATTERNS
                        .gender_star_special_chrs
                        .replace(extended, "in")
                        .to_string();
                    if self.base_is_misspelled(&replaced)
                        || (!extended.ends_with("in") && !extended.ends_with("innen"))
                    {
                        return false;
                    }
                }
            }
            if PATTERNS.gender_neutral_slash_hyphen.is_match(to_check) && end + 1 < word.len() {
                end += 2;
                if let Some(extended) = word.get(start..end) {
                    let replaced = PATTERNS
                        .gender_star_slash_hyphen
                        .replace(extended, "in")
                        .to_string();
                    if self.base_is_misspelled(&replaced)
                        || (!extended.ends_with("in") && !extended.ends_with("innen"))
                    {
                        return false;
                    }
                }
            }
            start = end;
        }
        true
    }

    fn avoid_infix_s_as_single_token(parts: Vec<String>) -> Vec<String> {
        let mut parts = parts;
        let mut idx = parts.len();
        while idx > 0 {
            idx -= 1;
            if parts[idx] == "s" && idx > 0 {
                let s = parts.remove(idx);
                parts[idx - 1].push_str(&s);
            }
        }
        parts
    }

    fn split_parts_by_hyphen(parts: Vec<String>) -> Vec<String> {
        let mut result: Vec<String> = Vec::with_capacity(parts.len());
        for part in parts {
            if part.contains('-') {
                result.extend(part.split('-').map(str::to_string));
            } else {
                result.push(part);
            }
        }
        result
    }

    fn restore_removed_hyphens(parts: Vec<String>, word: &str) -> Vec<String> {
        let hyphen_positions: Vec<usize> = word
            .char_indices()
            .filter(|(_, c)| *c == '-')
            .map(|(i, _)| i)
            .collect();
        let mut result = Vec::with_capacity(parts.len());
        let mut current_pos = 0usize;
        for mut token in parts {
            for hyphen_pos in &hyphen_positions {
                if *hyphen_pos >= current_pos && *hyphen_pos == current_pos + token.len() {
                    token.push('-');
                    break;
                }
            }
            current_pos += token.len();
            result.push(token);
        }
        result
    }

    pub fn ignore_potentially_misspelled_word(&self, word: &str) -> bool {
        let word_no_dot = cut_off_dot(word);
        if self.is_valid_word_length(word)
            || starts_with_lowercase(word)
            || self.is_prohibited(word)
            || self.is_prohibited(word_no_dot)
        {
            return false;
        }
        if self.is_probably_typo(word) {
            return false;
        }
        let word_no_dot_org = word_no_dot;
        let word_no_dot = replace_first_gender_star(word_no_dot);
        if PATTERNS.camel_case.is_match(&word_no_dot) {
            return false;
        }
        let mut parts = og_tagger::jwordsplitter::tokenize(&word_no_dot);
        if parts.len() == 1 {
            parts = og_tagger::jwordsplitter::tokenize_with_mode(&word_no_dot, false);
        }
        let parts = Self::avoid_infix_s_as_single_token(parts);
        if word_no_dot.contains('-') {
            let split_by_hyphen: Vec<&str> = word_no_dot.split('-').collect();
            let last_part = split_by_hyphen[split_by_hyphen.len() - 1];
            if !self.is_noun(last_part) && self.is_noun(&uppercase_first_char(last_part)) {
                return false;
            }
            for w in &split_by_hyphen {
                if self.base_is_misspelled(w)
                    && self.base_is_misspelled(remove_trailing_s_and_hyphen(w))
                {
                    return false;
                }
            }
            let parts =
                Self::restore_removed_hyphens(Self::split_parts_by_hyphen(parts), &word_no_dot);
            return self.check_split_parts(parts, &word_no_dot, word_no_dot_org);
        }
        self.check_split_parts(parts, &word_no_dot, word_no_dot_org)
    }

    fn check_split_parts(
        &self,
        parts: Vec<String>,
        word_no_dot: &str,
        word_no_dot_org: &str,
    ) -> bool {
        let _ = word_no_dot;
        if !Self::is_valid_part_length(&parts) {
            return false;
        }
        if parts.len() >= 2 && self.is_old_spelling(&parts) {
            return false;
        }
        if gender_star_find(word_no_dot_org).is_some()
            && !self.is_valid_gender_neutral_word(&parts, word_no_dot_org)
        {
            return false;
        }
        if parts.len() == 2 {
            self.process_two_part_compounds(&parts[0], &parts[1])
        } else if parts.len() == 3 {
            self.process_three_part_compound(&parts)
        } else {
            false
        }
    }

    fn process_two_part_compounds(&self, part1: &str, part2: &str) -> bool {
        let part1upcased = uppercase_first_char(part1);
        let part2upcased = uppercase_first_char(part2);
        let part1_without_hyphen = remove_trailing_hyphen(part1);
        let part2upcased_is_noun = self.is_noun(&part2upcased);
        let part2upcased_is_mispelled = self.base_is_misspelled(&part2upcased);

        if PATTERNS
            .invalid_comp_part_1
            .is_match(&lowercase_first_char(part1_without_hyphen))
        {
            return false;
        }
        if PATTERNS
            .invalid_comp_part_2
            .is_match(&lowercase_first_char(part2))
        {
            return false;
        }

        let mut part1_without_infix_s = part1upcased.clone();
        let mut part1_lemma = self.find_lemma_for_noun(part1_without_hyphen);
        if part1_lemma.is_empty() && part1_without_hyphen.ends_with('s') {
            part1_lemma = self.find_lemma_for_noun(remove_trailing_s_and_hyphen(part1));
            part1_without_infix_s = remove_trailing_s(&part1upcased).to_string();
        }

        if self.is_noun_nom_plu(&part1_without_infix_s)
            && !self.is_noun_nom_sin(&part1_without_infix_s)
            && !self.is_country_or_region_nom_sin(&part1_without_infix_s)
            && (!self.is_sub_ver_inf(&part2upcased)
                || (self.is_sub_ver_inf(&part2upcased)
                    && PATTERNS
                        .subinf_singular_object
                        .is_match(&lowercase_first_char(part2))))
            && !self.needs_to_be_plural(&lowercase_first_char(&part1_lemma))
            && !PATTERNS
                .wechselnumerus
                .is_match(&lowercase_first_char(&part1_lemma))
            && !PATTERNS
                .wechselinfix
                .is_match(&lowercase_first_char(remove_trailing_s(&part1_lemma)))
        {
            return false;
        }
        if self.needs_to_be_plural(&lowercase_first_char(&part1_lemma))
            && self.is_noun_nom_sin(&part1_without_infix_s)
        {
            return false;
        }
        if PATTERNS
            .wechselnumerus
            .is_match(&lowercase_first_char(&part1_lemma))
            && !self.check_plural_for_part1_part2_combination(part1, part2)
        {
            return false;
        }
        if PATTERNS.wechselinfix.is_match(&lowercase_first_char(part1)) {
            return self.check_infix_s_for_part1_part2_combination(part1, part2);
        }
        if PATTERNS
            .confused_prefixes
            .is_match(&lowercase_first_char(part1))
            && !part2upcased_is_mispelled
            && part2upcased_is_noun
        {
            return self.check_confusion_for_part1_part2_combination(part1, part2);
        }
        if part2upcased_is_noun
            && !part2upcased_is_mispelled
            && part1_without_hyphen.ends_with('s')
            && (self.is_noun_nom(&part1upcased) || self.is_verb_stem(part1))
            && !self.needs_infix_s(remove_trailing_s(&part1upcased))
        {
            return true;
        }
        if part2upcased_is_noun
            && !part2upcased_is_mispelled
            && part1_without_hyphen.ends_with('s')
            && self.is_noun_nom(remove_trailing_s_and_hyphen(&part1upcased))
            && self.needs_infix_s(remove_trailing_s_and_hyphen(&part1upcased))
        {
            return true;
        }
        if part2upcased_is_noun
            && !part2upcased_is_mispelled
            && !part1_without_hyphen.ends_with('s')
            && (self.is_noun_nom(&part1upcased) || self.is_verb_stem(part1))
            && !self.needs_infix_s(&part1upcased)
        {
            return true;
        }
        if part2upcased_is_noun
            && !part2upcased_is_mispelled
            && ((is_all_uppercase(remove_trailing_s_and_hyphen(part1))
                && !self.base_is_misspelled(remove_trailing_s_and_hyphen(part1)))
                || self.is_other_prefix(part1))
        {
            return true;
        }
        if part2upcased_is_noun
            && !part2upcased_is_mispelled
            && self.is_country_or_region_nom_sin(part1)
            && !PATTERNS
                .cities_exceptions
                .is_match(&lowercase_first_char(part2))
        {
            return true;
        }
        false
    }

    fn process_three_part_compound(&self, parts: &[String]) -> bool {
        let part1 = parts[0].as_str();
        let part2 = parts[1].as_str();
        let part3 = parts[2].as_str();
        let compound1 = format!("{part1}{part2}");
        let compound2 = format!("{}{part3}", uppercase_first_char(part2));
        if self.is_noun(&compound1) && self.is_noun(&compound2) {
            return self.process_two_part_compounds(part1, remove_trailing_hyphen(part2))
                && self.process_two_part_compounds(part2, part3);
        }
        if compound1.ends_with('s') || compound1.ends_with("s-") {
            let part2_no_infix = remove_trailing_s_and_hyphen(part2);
            return self.process_two_part_compounds(part1, part2_no_infix)
                && self.process_two_part_compounds(part2, part3);
        }
        if self.is_verb_prefix(part1) && self.is_verb_stem(part2) && self.is_noun(&compound2) {
            return true;
        }
        if self.is_noun_nom_sin(part1) && self.is_verb_stem(part2) && self.is_noun(&compound2) {
            return true;
        }
        if self.is_noun_nom(part1) && self.is_other_prefix(part2) && self.is_noun(&compound2) {
            return true;
        }
        if self.is_other_prefix(part1) && self.is_verb_stem(part2) && self.is_noun(&compound2) {
            return true;
        }
        false
    }

    fn check_infix_s_for_part1_part2_combination(&self, part1: &str, part2: &str) -> bool {
        let mut part2_lemma = self.find_lemma_for_noun(remove_trailing_hyphen(part2));
        if part2_lemma.is_empty() && remove_trailing_hyphen(part2).ends_with('s') {
            part2_lemma = self.find_lemma_for_noun(remove_trailing_s_and_hyphen(part2));
        }
        let part2_lemma_lc = lowercase_first_char(&part2_lemma);
        if part1 == "Arbeit" && PATTERNS.arbeit_comp.is_match(part2) {
            return true;
        }
        if part1 == "Arbeits" && !PATTERNS.arbeit_comp.is_match(part2) {
            return true;
        }
        if part1 == "Link" && PATTERNS.link_comp.is_match(&part2_lemma_lc) {
            return true;
        }
        if part1 == "Links" && PATTERNS.links_comp.is_match(&part2_lemma_lc) {
            return true;
        }
        if part1 == "Recht" && PATTERNS.recht_comp.is_match(&part2_lemma_lc) {
            return true;
        }
        if part1 == "Rechts" && PATTERNS.rechts_comp.is_match(&part2_lemma_lc) {
            return true;
        }
        if part1 == "Verband" && PATTERNS.verband_comp.is_match(&part2_lemma_lc) {
            return true;
        }
        if part1 == "Verbands" && PATTERNS.verbands_comp.is_match(&part2_lemma_lc) {
            return true;
        }
        if PATTERNS.wochentage.is_match(part1) && PATTERNS.wochentag_comp.is_match(&part2_lemma_lc)
        {
            return true;
        }
        if PATTERNS.wochentage_s.is_match(part1)
            && !PATTERNS.wochentag_comp.is_match(&part2_lemma_lc)
        {
            return true;
        }
        false
    }

    fn check_confusion_for_part1_part2_combination(&self, part1: &str, part2: &str) -> bool {
        let part2_lemma = self.find_lemma_for_noun(remove_trailing_hyphen(part2));
        let part2_lemma_lc = lowercase_first_char(&part2_lemma);
        if part1 == "Bad" && PATTERNS.bach_comp.is_match(&part2_lemma_lc) {
            return true;
        }
        if part1 == "Bad" && PATTERNS.bad_comp.is_match(&part2_lemma_lc) {
            return true;
        }
        if part1 == "Bade" && !PATTERNS.bad_comp.is_match(&part2_lemma_lc) {
            return true;
        }
        if part1 == "Wider" && PATTERNS.wider_comp.is_match(&part2_lemma_lc) {
            return true;
        }
        if part1 == "Wieder" && !PATTERNS.wider_comp.is_match(&part2_lemma_lc) {
            return true;
        }
        false
    }

    fn check_plural_for_part1_part2_combination(&self, part1: &str, part2: &str) -> bool {
        let part2_lemma = self.find_lemma_for_noun(remove_trailing_hyphen(part2));
        if part1 == "Welt" && !PATTERNS.welten_comp.is_match(&part2_lemma) {
            return true;
        }
        if part1 == "Welten" && PATTERNS.welten_comp.is_match(&part2_lemma) {
            return true;
        }
        if part1 == "Wort" && !PATTERNS.woerter_comp.is_match(&part2_lemma) {
            return true;
        }
        if part1 == "Wörter" && PATTERNS.woerter_comp.is_match(&part2_lemma) {
            return true;
        }
        false
    }
}

pub struct GermanSpellerRule {
    id: String,
    speller: Arc<GermanSpeller>,
    category: Category,
}

impl GermanSpellerRule {
    pub fn new(speller: Arc<GermanSpeller>, id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            speller,
            category: Category::new("TYPOS", "Possible Typo")
                .with_description("Possible spelling mistake"),
        }
    }

    pub fn speller(&self) -> &Arc<GermanSpeller> {
        &self.speller
    }
}

impl Rule for GermanSpellerRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Possible spelling mistake"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let tokens = sentence.tokens();
        let mut word_tokens: Vec<&str> = Vec::with_capacity(tokens.len());
        let mut token_index: Vec<usize> = Vec::with_capacity(tokens.len());
        for (i, token_readings) in tokens.iter().enumerate() {
            if token_readings.is_whitespace() {
                continue;
            }
            let text = token_readings.token().token();
            if text.is_empty() || text == "<S>" {
                continue;
            }
            word_tokens.push(text);
            token_index.push(i);
        }

        for (idx, &i) in token_index.iter().enumerate() {
            let token_readings = &tokens[i];
            if token_readings.is_ignored_by_speller() || token_readings.is_immunized() {
                continue;
            }
            let text = token_readings.token().token();
            if !text.chars().any(|c| c.is_alphabetic()) {
                continue;
            }
            if text.contains('.') && text.chars().filter(|c| *c == '.').count() > 1 {
                continue;
            }
            if text.chars().any(|c| c.is_ascii_digit())
                || text.contains('/')
                || text.contains('@')
                || text.starts_with("http")
                || text.starts_with("www.")
            {
                continue;
            }
            if self.speller.ignore_word_in_sentence(&word_tokens, idx)
                && !self.speller.is_prohibited(cut_off_dot(text))
            {
                continue;
            }
            if !self.speller.is_misspelled(text) {
                continue;
            }

            let start = token_readings.token().start();
            let end = token_readings.token().end();
            let full_text = sentence.text();
            let rel_start = start.saturating_sub(sentence.start());
            let rel_end = end.saturating_sub(sentence.start()).min(full_text.len());
            let mut context_start = rel_start.saturating_sub(40);
            while context_start < full_text.len() && !full_text.is_char_boundary(context_start) {
                context_start += 1;
            }
            let mut context_end = std::cmp::min(rel_end + 40, full_text.len());
            while context_end > context_start && !full_text.is_char_boundary(context_end) {
                context_end += 1;
            }
            let context_text = full_text[context_start..context_end].to_string();
            let suggestions = self.speller.suggestions(text, 5);
            let replacements: Vec<SuggestedReplacement> = suggestions
                .into_iter()
                .map(SuggestedReplacement::new)
                .collect();
            let rule = RuleMatchRule::new(&self.id, self.description())
                .with_category(self.category.clone())
                .with_issue_type(IssueType::Misspelling.as_str());
            matches.push(
                RuleMatch::new(
                    format!("Possible spelling mistake found: '{}'", text),
                    start,
                    end - start,
                    rule,
                    RuleMatchContext::new(context_text, context_start, context_end - context_start),
                )
                .with_sentence(sentence.text().to_string())
                .with_replacements(replacements)
                .with_match_type(RuleMatchKind::UnknownWord),
            );
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn resource_dir() -> Option<PathBuf> {
        [
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
            "languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.join("german.dict").exists())
    }

    fn shared_dictionary() -> Option<&'static Arc<Dictionary>> {
        static DICT: LazyLock<Option<Arc<Dictionary>>> = LazyLock::new(|| {
            let dir = resource_dir()?;
            let mut dict = Dictionary::from_shared_words(Arc::new(
                crate::morfologik_view::MorfologikView::new(
                    og_tagger::german_tagger::shared_german_dictionary(&dir)?,
                ),
            ));
            if let Ok(words) = std::fs::read_to_string(dir.join("hunspell/de_DE.dic")) {
                for line in words.lines() {
                    let word = line.split('/').next().unwrap_or("").trim();
                    if !word.is_empty() && !word.starts_with('#') {
                        dict.add_word(word);
                    }
                }
            }
            Some(Arc::new(dict))
        });
        DICT.as_ref()
    }

    fn speller_arc() -> Option<&'static Arc<GermanSpeller>> {
        static SPELLER: LazyLock<Option<Arc<GermanSpeller>>> = LazyLock::new(|| {
            let dir = resource_dir()?;
            let tagger = og_tagger::GermanTagger::from_resource_dir(&dir)?;
            Some(Arc::new(GermanSpeller::new(
                Arc::clone(shared_dictionary()?),
                &dir,
                None,
                Some(Arc::new(tagger)),
                "de-DE",
            )))
        });
        SPELLER.as_ref()
    }

    fn swiss_speller() -> Option<&'static GermanSpeller> {
        static SPELLER: LazyLock<Option<GermanSpeller>> = LazyLock::new(|| {
            let dir = resource_dir()?;
            Some(GermanSpeller::new(
                Arc::clone(shared_dictionary()?),
                &dir,
                None,
                None,
                "de-CH",
            ))
        });
        SPELLER.as_ref()
    }

    fn hunspell_speller() -> Option<&'static GermanSpeller> {
        static SPELLER: LazyLock<Option<GermanSpeller>> = LazyLock::new(|| {
            let dir = resource_dir()?;
            let hunspell_dir = dir.join("hunspell");
            let checker = crate::hunspell::HunspellChecker::load(
                &hunspell_dir.join("de_DE.aff"),
                &hunspell_dir.join("de_DE.dic"),
            )?;
            Some(
                speller_arc()?
                    .with_extra_ignore_words(Vec::new())
                    .with_hunspell_core(Arc::new(checker)),
            )
        });
        SPELLER.as_ref()
    }

    fn taggerless_speller() -> Option<&'static GermanSpeller> {
        static SPELLER: LazyLock<Option<GermanSpeller>> = LazyLock::new(|| {
            let dir = resource_dir()?;
            Some(GermanSpeller::new(
                Arc::clone(shared_dictionary()?),
                &dir,
                None,
                None,
                "de-DE",
            ))
        });
        SPELLER.as_ref()
    }

    fn speller() -> Option<&'static GermanSpeller> {
        speller_arc().map(|arc| arc.as_ref())
    }

    #[test]
    fn valid_compounds_are_accepted() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in ["Leerzeile", "Arbeitszimmer", "Kirschbaum"] {
            assert!(
                !s.is_misspelled_strict(word) || !s.is_misspelled(word),
                "'{word}' must be accepted as a compound (strict={}, combined={})",
                s.is_misspelled_strict(word),
                s.is_misspelled(word)
            );
            assert!(!s.is_misspelled(word), "'{word}' must not be flagged");
        }
    }

    #[test]
    fn invalid_compound_and_typos_stay_misspelled() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(
            s.is_misspelled_strict("Leerzeilx"),
            "plain typo stays flagged"
        );
        assert!(
            s.is_misspelled("Arbeitamt"),
            "'Arbeit+amt' has a too-short second part and must stay flagged"
        );
    }

    #[test]
    fn elative_and_hyphenated_compound_acceptance() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        let words = ["Stil-", "und", "Grammatikprüfung"];
        assert!(
            s.ignore_word_in_sentence(&words, 0),
            "hanging hyphen before compound must be accepted"
        );
        assert!(
            s.ignore_elative("superschnell"),
            "elative must be accepted: {}",
            s.is_misspelled("superschnell")
        );
    }

    #[test]
    fn standart_special_case_flags() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.is_misspelled_strict("Standartfehler"));
        assert!(!s.is_misspelled_strict("Standarte"));
    }

    #[test]
    fn prohibited_words_are_flagged_even_if_known() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.is_prohibited("Abiturabschuss") || s.is_misspelled_strict("Abiturabschuss"));
    }

    #[test]
    fn gender_star_replace_first_matches_java() {
        assert_eq!(replace_first_gender_star("Expert*innen"), "Expertinnen");
        assert_eq!(replace_first_gender_star("Kund_innen"), "Kundinnen");
        assert_eq!(
            replace_first_gender_star("ExpertInnen"),
            "Expertinnen",
            "'t' is an ascii word char so the Binnen-I alternative matches"
        );
        assert_eq!(replace_first_binnen_i("KundInnen"), "Kundinnen");
        assert_eq!(replace_first_gender_star("Fahrer/-innen"), "Fahrerinnen");
    }

    #[test]
    fn line_expansion_covers_flags_and_gender() {
        assert_eq!(
            expand_line("Osama/S"),
            vec!["Osama".to_string(), "Osamas".to_string()]
        );
        assert_eq!(expand_line("Kund_in").len(), 6);
        assert_eq!(
            expand_line("weg_delegieren"),
            vec![
                "wegdelegieren".to_string(),
                "wegzudelegieren".to_string(),
                "Wegdelegierens".to_string()
            ]
        );
        assert_eq!(expand_line("Foo Bar # note"), vec!["Foo Bar".to_string()]);
    }

    #[test]
    fn swiss_variant_swaps_the_active_ignore_set() {
        let Some(de) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        let Some(ch) = swiss_speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(de.is_misspelled("Badstrasse"));
        assert!(!ch.is_misspelled("Badstrasse"));
        assert!(!de.is_misspelled("Badstraße"));

        let re_shared = ch.with_shared_data(Arc::clone(shared_dictionary().unwrap()), "de-DE");
        assert!(re_shared.is_misspelled("Badstrasse"));
    }

    #[test]
    fn extra_ignore_words_add_both_sharp_s_forms() {
        let Some(de) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        let Some(ch) = swiss_speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(de.is_misspelled("Qxßweg"));
        let extended = de.with_extra_ignore_words(vec!["Qxßweg".to_string()]);
        assert!(!extended.is_misspelled("Qxßweg"));
        assert!(extended.is_misspelled("Qxssweg"));

        assert!(ch.is_misspelled("Qxssweg"));
        let extended_ch = ch.with_extra_ignore_words(vec!["Qxßweg".to_string()]);
        assert!(!extended_ch.is_misspelled("Qxssweg"));
        assert!(extended_ch.is_misspelled("Qxßweg"));
    }

    #[test]
    fn with_hunspell_core_routes_spell_through_hunspell() {
        let Some(s) = speller_arc() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        let Some(dir) = resource_dir() else {
            return;
        };
        let hunspell_dir = dir.join("hunspell");
        let Some(checker) = crate::hunspell::HunspellChecker::load(
            &hunspell_dir.join("de_DE.aff"),
            &hunspell_dir.join("de_DE.dic"),
        ) else {
            eprintln!("Skipping: hunspell resources not present");
            return;
        };
        let with_core = s
            .with_extra_ignore_words(Vec::new())
            .with_hunspell_core(Arc::new(checker));
        assert!(!with_core.is_misspelled("hocheffizientes"));
        assert!(!with_core.is_misspelled("Haus"));
        assert!(with_core.is_misspelled("Hausx"));
        assert!(with_core.dictionary().contains("Haus"));
        assert!(!with_core.suggestions("Hausx", 3).is_empty());
    }

    #[test]
    fn ignore_word_in_sentence_gates() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        let oversized = "x".repeat(MAX_TOKEN_LENGTH + 10);
        assert!(s.ignore_word_in_sentence(&[&oversized], 0));
        assert!(s.ignore_word_in_sentence(&["--"], 0));
        assert!(s.ignore_word_in_sentence(&["a"], 0));
        assert!(s.ignore_word_in_sentence(&["Badstraße."], 0));
        assert!(!s.ignore_word_in_sentence(&["Qxqweg"], 0));

        let extended = s.with_extra_ignore_words(vec!["qxqweg".to_string()]);
        assert!(extended.ignore_word_in_sentence(&["Qxqweg"], 0));
    }

    #[test]
    fn hanging_hyphen_uses_the_next_compound() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.ignore_word_in_sentence(&["Meinungs-", "Meinungsfreiheit"], 0));
        assert!(!s.ignore_word_in_sentence(&["Stil-", "und", ","], 0));
        assert!(!s.ignore_word_in_sentence(&["Stil-"], 0));
    }

    #[test]
    fn hyphenated_compounds_with_ignored_parts() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.ignore_word_in_sentence(&["Big-Data-Analysis"], 0));
        assert!(s.ignore_word_in_sentence(&["Haus-Einpseudowortmitßfürlanguagetooltests"], 0));
        assert!(!s.ignore_word_in_sentence(&["Bla-Einpseudowortmitßfürlanguagetooltests"], 0));
    }

    #[test]
    fn direction_prefixed_compounds_stay_accepted() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in ["Ostwald", "Nordbahn", "Südbahn", "Westwind"] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
    }

    #[test]
    fn java_compound_families_behave_like_the_reference() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        let accepted = [
            "Montagmorgen",
            "Weltenbrand",
            "Wortschatz",
            "Wörterbuch",
            "Arbeitnehmer",
            "Arbeitsamt",
            "Linkliste",
            "Linksabbieger",
            "Rechtsanwalt",
            "Verbandskasse",
            "Badezimmer",
            "Wiedergabe",
            "Widerspruch",
            "Badforelle",
            "Sonnenblumenkern",
            "Bundeskanzler",
        ];
        for word in accepted {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
        for word in ["Montagsmorgen", "Weltbrand"] {
            assert!(s.is_misspelled(word), "'{word}' must stay flagged");
        }
    }

    #[test]
    fn gender_star_and_camel_case_words() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(!s.is_misspelled("Expert*innen"));
        assert!(!s.is_misspelled("Fahrerinnen"));
        assert!(s.is_misspelled("Kund*innen"));
        assert!(s.is_misspelled("FooBar"));
    }

    #[test]
    fn adjective_suffix_and_mitarbeitende_words_stay_accepted() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in ["arbeitsrelevant", "umweltrelevant", "Mitarbeitende"] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
    }

    #[test]
    fn is_misspelled_cache_survives_eviction() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for i in 0..(8192 + 16) {
            assert!(s.is_misspelled(&format!("q{i}x")));
        }
        assert!(s.is_misspelled("Weltbrand"));
        assert!(!s.is_misspelled("Weltenbrand"));
    }

    #[test]
    fn taggerless_speller_keeps_the_core_gates() {
        let Some(s) = taggerless_speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.is_misspelled("Weltbrand"));
        assert!(s.is_misspelled("Montagsmorgen"));
        assert!(!s.is_misspelled("Weltenbrand"));
    }

    #[test]
    fn strict_mode_flags_hyphenated_unknown_compounds() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.is_misspelled_strict("Bundes-Arbeitgeber"));
        assert!(!s.is_misspelled_strict("Weltenbrand"));
    }

    #[test]
    fn german_rule_match_sentence_gates_and_metadata() {
        use og_core::AnalyzedTokenReadings;
        let Some(s) = speller_arc() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        let rule = GermanSpellerRule::new(Arc::clone(s), "DE_HUNSPELL_RULE");
        assert_eq!(rule.id(), "DE_HUNSPELL_RULE");
        assert_eq!(rule.description(), "Possible spelling mistake");
        assert_eq!(rule.category().name(), "Possible Typo");
        assert_eq!(rule.issue_type(), IssueType::Misspelling);
        assert!(Arc::ptr_eq(rule.speller(), s));

        let mut sentence = AnalyzedSentence::new("Hallo wrldx.", 0, "Hallo wrldx.".len());
        let mut immunized = AnalyzedTokenReadings::new(AnalyzedToken::new("wrldx", 6, 11));
        immunized.immunize();
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("Hallo", 0, 5)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("<S>", 0, 0)),
            AnalyzedTokenReadings::new(AnalyzedToken::new(" ", 0, 0)),
            immunized,
            AnalyzedTokenReadings::new(AnalyzedToken::new("http://x", 0, 0)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("e.g.", 0, 0)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("abc12", 0, 0)),
        ]);
        assert!(rule.match_sentence(&sentence).is_empty());

        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("Hallo", 0, 5)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("wrldx", 6, 11)),
        ]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].message().contains("wrldx"));
        assert_eq!(matches[0].rule().id(), "DE_HUNSPELL_RULE");
    }

    #[test]
    fn schaf_special_cases_flag_the_plain_variants() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in ["Bauernschaf", "Bauernschafts", "Bauernschafen"] {
            assert!(s.is_misspelled(word), "'{word}' must stay flagged");
        }
        assert!(!s.is_misspelled("Wildschafs"));
    }

    #[test]
    fn old_spelling_compound_parts_stay_flagged() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.is_misspelled("Abflußrohr"));
    }

    #[test]
    fn gendered_forms_split_between_valid_and_invalid() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(!s.is_misspelled("Expert*in"));
        assert!(s.is_misspelled("Fahrer/-innen"));
        assert!(s.is_misspelled("Leser_in"));
        assert!(s.is_misspelled("Leser:in"));
    }

    #[test]
    fn gender_star_plural_forms_split_on_the_part_base() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(
            !s.is_misspelled("Autofahrerinnen"),
            "the plain compound is a valid word"
        );
        assert!(
            !s.is_misspelled("Expert*innen"),
            "a star plural over a standalone suffix is accepted"
        );
        assert!(
            s.is_misspelled("Autofahrer*innen"),
            "the split part 'fahrerinnen' is not a word, so the star plural is rejected"
        );
    }

    #[test]
    fn binnen_i_forms_inside_compounds_are_rejected() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(!s.is_misspelled("Autofahrerinnen"));
        assert!(
            s.is_misspelled("AutofahrerIn"),
            "the Binnen-I form of the compound is rejected"
        );
    }

    #[test]
    fn hyphenated_and_three_part_families() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.is_misspelled("Bundes-Kanzler"));
        for word in [
            "Bundeskanzler",
            "Bundeskanzlerin",
            "Sonnenblumenöl",
            "Griechenlandorchester",
        ] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
    }

    #[test]
    fn filter_is_misspelled_mirrors_the_hunspell_rule() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.filter_is_misspelled("Hausx"));
        assert!(!s.filter_is_misspelled("Haus"));
        assert!(!s.filter_is_misspelled("--"));
        assert!(!s.filter_is_misspelled("5"));
    }

    struct TempResourceDir {
        dir: PathBuf,
        core: PathBuf,
    }

    impl Drop for TempResourceDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
            let _ = std::fs::remove_dir_all(&self.core);
        }
    }

    #[test]
    fn synthetic_resource_dir_drives_ignore_and_prohibit_sets() {
        let base = std::env::temp_dir().join(format!("og-spell-de-res-{}", std::process::id()));
        let dir = base.join("de");
        let core = base.join("core");
        std::fs::create_dir_all(dir.join("hunspell")).unwrap();
        std::fs::create_dir_all(&core).unwrap();
        std::fs::write(
            dir.join("hunspell/ignore.txt"),
            "Ignoriert\nGewiß\nWeißwort-*\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("hunspell/prohibit.txt"),
            "Anfang.*\n.*ende\nVerboten\n",
        )
        .unwrap();
        std::fs::write(core.join("spelling_global.txt"), "Globalignoriert\n").unwrap();
        let resources = TempResourceDir {
            dir: dir.clone(),
            core: core.clone(),
        };

        let dict = Arc::new(Dictionary::new());
        let de = GermanSpeller::new(
            Arc::clone(&dict),
            &resources.dir,
            Some(&resources.core),
            None,
            "de-DE",
        );
        let ch = GermanSpeller::new(
            Arc::clone(&dict),
            &resources.dir,
            Some(&resources.core),
            None,
            "de-CH",
        );

        assert!(!de.is_misspelled("Ignoriert"));
        assert!(!de.is_misspelled("Globalignoriert"));
        assert!(de.is_misspelled("Gewiss"));
        assert!(!de.is_misspelled("Gewiß"));
        assert!(!ch.is_misspelled("Gewiss"));
        assert!(ch.is_misspelled("Gewiß"));

        assert!(de.is_misspelled("Anfangswort"));
        assert!(de.is_misspelled("Wortende"));
        assert!(de.is_misspelled("Verboten"));
        assert!(de.is_misspelled("Unverboten"));
    }

    #[test]
    fn line_expansion_handles_malformed_lines() {
        assert_eq!(expand_line("Foo_Bar_Baz"), vec!["Foo_Bar_Baz".to_string()]);
        assert_eq!(expand_line("a/b/c"), vec!["a/b/c".to_string()]);
        assert_eq!(
            expand_line("Hauptstraße/T"),
            vec!["Hauptstraße".to_string(), "Hauptstr.".to_string()]
        );
        assert_eq!(replace_first_binnen_i("Kunden"), "Kunden");
        assert!(
            expand_line("Wort/Q").is_empty(),
            "unknown flag chars expand to nothing"
        );
    }

    #[test]
    fn direction_prefixed_lowercase_words_are_ignored_in_sentences() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.is_misspelled("ostbahn"), "unknown as a plain word");
        assert!(s.ignore_word_in_sentence(&["ostbahn"], 0));
        assert!(s.ignore_word_in_sentence(&["nordbahn"], 0));
        assert!(!s.ignore_word_in_sentence(&["qxbahn"], 0));
    }

    #[test]
    fn schaf_special_case_flags_when_the_schaft_form_is_valid() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(
            s.is_misspelled("Mannschaf"),
            "'Mannschaft' is valid, so the plain form is flagged"
        );
        assert!(!s.is_misspelled("Mannschaft"));
        assert!(
            !s.is_misspelled("Wildschaf"),
            "listed Schaf compounds stay accepted"
        );
    }

    #[test]
    fn spielzug_special_case_flags_unknown_derivations() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.is_misspelled("Spielzugtorte"));
        assert!(!s.is_misspelled("Spielzug"));
    }

    #[test]
    fn hyphen_compound_with_ignored_middle_part_is_ignored() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.ignore_word_in_sentence(&["Haus-Hadamard-Haus"], 0));
        assert!(!s.ignore_word_in_sentence(&["Haus-Qxadard-Haus"], 0));
    }

    #[test]
    fn hanging_hyphen_covers_ignored_and_fugen_s_words() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.ignore_word_in_sentence(&["Hadamard-", "und", "Schwarz-Rot"], 0));
        assert!(s.ignore_word_in_sentence(&["Bundes-", "und", "Schwarz-Rot-Gold"], 0));
        assert!(!s.ignore_word_in_sentence(&["Qxschafts-", "und", "Schwarz-Rot"], 0));
    }

    #[test]
    fn hyphen_gates_flag_misspelled_and_lowercase_parts() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(
            s.is_misspelled("Bundes-kanzler"),
            "lowercase part only spells as a noun when capitalized"
        );
        assert!(s.is_misspelled("Qxq-Analysis"));
        assert!(s.is_misspelled("Bundes-qxqwort"));
        assert!(
            s.is_misspelled("Haus-Schwarz-Rot"),
            "hyphen-keeping parts are split again before the compound walk"
        );
    }

    #[test]
    fn old_spelling_s_final_parts_stay_flagged() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.is_misspelled("Känguruhsherde"));
        assert!(!s.is_misspelled("Kängurus"));
    }

    #[test]
    fn wechsel_infix_families_accept_the_listed_second_parts() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in [
            "Verbandskasse",
            "Linkportale",
            "Rechtbank",
            "Verbandraum",
            "Dienstagvormittag",
            "Dienstagsrunde",
            "Linksverkehr",
            "Arbeitnehmendem",
        ] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
    }

    #[test]
    fn confused_prefix_families_split_between_bach_bad_and_wider() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in [
            "Badforelle",
            "Badregal",
            "Badezimmer",
            "Widerglanz",
            "Wiederecho",
        ] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
    }

    #[test]
    fn wechsel_numerus_families_accept_the_matching_plural_forms() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in ["Weltfrieden", "Weltenwende", "Wörterliste", "Wortschatz"] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
        assert!(s.is_misspelled("Wörterschatz"));
    }

    #[test]
    fn fugen_s_arms_accept_schoenheits_and_kassen_compounds() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in ["Schönheitswasser", "Schönheitsfarm", "Kassenbuch"] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
    }

    #[test]
    fn plural_lemma_arms_flag_singular_part_one_compounds() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(
            s.is_misspelled("Häuserrauchen"),
            "SUB:INF object keeps the plural gate"
        );
        assert!(s.is_misspelled("Häuserqasse"));
    }

    #[test]
    fn three_part_verb_families_are_accepted_by_the_spell_rule() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in ["Ausbiegemöglichkeit", "Kennenlernmöglichkeit"] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
    }

    #[test]
    fn missing_adj_gate_accepts_testable_noun_stems_with_hunspell() {
        let Some(s) = hunspell_speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in [
            "mannschaftsrelevant",
            "waldrelevant",
            "dorfbedingt",
            "handlungsartig",
        ] {
            assert!(
                s.ignore_word_in_sentence(&[word], 0),
                "'{word}' passes the missing-adjective gate"
            );
        }
        assert!(!s.ignore_word_in_sentence(&["arbeitartig"], 0));
    }

    #[test]
    fn mitarbeitende_gate_replaces_the_suffix_with_mitarbeiter() {
        let Some(s) = hunspell_speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(!s.is_misspelled("Teammitarbeitende"));
        assert!(s.ignore_word_in_sentence(&["Teammitarbeitende"], 0));
    }

    #[test]
    fn gender_neutral_walk_covers_star_slash_and_binnen_i_forms() {
        let Some(s) = hunspell_speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in [
            "Bundeskanzler*innen",
            "Fahrer/-innen",
            "Kund*innen",
            "Leser/-in",
            "Professor/-innen",
            "Schwarz*weiß",
            "Schwarz*weiss",
        ] {
            assert!(s.is_misspelled(word), "'{word}' must stay flagged");
        }
        assert!(
            !s.is_misspelled("BundeskanzlerInnen"),
            "spellable Binnen-I form passes the gender-neutral walk"
        );
    }

    #[test]
    fn gender_neutral_plural_suffixes_match_the_java_pins() {
        let Some(s) = hunspell_speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in [
            "Jurist:innenausbildung",
            "Jurist*innenausbildung",
            "Jurist_innenausbildung",
        ] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
        for word in [
            "Jurist:innenausbildunk",
            "Jurist*innenausbildunk",
            "Jurist_innenausbildunk",
        ] {
            assert!(s.is_misspelled(word), "'{word}' must stay flagged");
        }
        for word in ["SEO-Expert/-innen", "Arbeitnehmer/-innenschutzgesetz"] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
        for word in ["SEO-Expert*in", "SEO-Expert:in", "SEO-Expert_in"] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
        for word in ["SEO-Expert*annen", "SEO-Expert:innenwahrsheinlich"] {
            assert!(s.is_misspelled(word), "'{word}' must stay flagged");
        }
        for word in [
            "SEO-Expert*in",
            "SEO-Expert:in",
            "SEO-Expert_in",
            "SEO-Expert*innen",
            "SEO-Expert:innen",
            "SEO-Expert_innen",
            "SEO-Expert/-innen",
            "Arbeitnehmer*innenschutzgesetz",
            "Arbeitnehmer:innenschutzgesetz",
            "Arbeitnehmer_innenschutzgesetz",
            "Arbeitnehmer/-innenschutzgesetz",
        ] {
            assert!(
                s.ignore_potentially_misspelled_word(word),
                "'{word}' must be ignored as potentially misspelled"
            );
        }
    }

    #[test]
    fn oversized_and_single_char_words_skip_the_hunspell_gates() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        let oversized = "a".repeat(MAX_TOKEN_LENGTH + 10);
        assert!(!s.filter_is_misspelled(&oversized));
        assert!(!s.is_misspelled("a"));
        assert!(!s.is_misspelled_strict("a"));
    }

    #[test]
    fn compound_parts_spellable_falls_back_to_the_capitalized_form() {
        let base = std::env::temp_dir().join(format!("og-spell-de-cap-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        struct TempDir(std::path::PathBuf);
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let guard = TempDir(base.clone());

        let mut dict = Dictionary::from_shared_words(Arc::new(
            crate::morfologik_view::MorfologikView::new(Arc::new(
                og_tagger::morfologik::MorfologikDictionary::from_decoded_text(
                    "Kanzler\tKanzler\tSUB:NOM:SIN:MAS\n",
                ),
            )),
        ));
        dict.add_word("Bundes");
        let s = GermanSpeller::new(Arc::new(dict), &guard.0, None, None, "de-DE");

        assert!(
            s.is_misspelled("Bundeskanzler"),
            "the compound itself is unknown"
        );
        assert!(
            !s.is_misspelled_strict("Bundeskanzler"),
            "'kanzler' only becomes spellable through its capitalized form"
        );
    }

    #[test]
    fn match_sentence_snaps_the_context_to_char_boundaries() {
        use og_core::AnalyzedTokenReadings;
        let Some(s) = speller_arc() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        let rule = GermanSpellerRule::new(Arc::clone(s), "DE_HUNSPELL_RULE");

        let prefix: String = "ä".repeat(21);
        let suffix: String = "ä".repeat(30);
        let text = format!("{prefix} wrldx {suffix}.");
        let word_start = prefix.len() + 1;
        let word_end = word_start + "wrldx".len();
        let mut sentence = AnalyzedSentence::new(&text, 0, text.len());
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new(&prefix, 0, prefix.len())),
            AnalyzedTokenReadings::new(AnalyzedToken::new("wrldx", word_start, word_end)),
        ]);

        let matches = rule.match_sentence(&sentence);
        let wrldx_match = matches
            .iter()
            .find(|m| m.message().contains("wrldx"))
            .expect("wrldx is flagged");
        let context = wrldx_match.context();
        let context_start = context.offset();
        let context_text = context.text();
        assert!(
            text.is_char_boundary(context_start),
            "context start snapped to a char boundary"
        );
        assert!(
            text.is_char_boundary(context_start + context_text.len()),
            "context end snapped to a char boundary"
        );
        assert!(context_text.contains("wrldx"));
    }

    #[test]
    fn short_words_and_ignored_prefixes_drive_the_compound_ignore_walk() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(
            !s.ignore_word_in_sentence(&["Hau"], 0),
            "short words return no prefix"
        );
        let extended = s.with_extra_ignore_words(vec!["Qxvw".to_string(), "Qxung".to_string()]);
        assert!(extended.ignore_word_in_sentence(&["Qxvwort"], 0));
        assert!(!extended.ignore_word_in_sentence(&["Qxungstadt"], 0));
    }

    #[test]
    fn hanging_hyphen_next_word_compound_kinds() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(
            s.ignore_word_in_sentence(&["Meinungs-", "und", "Kanzlei"], 0),
            "Kanzlei counts as a compound via the -ei ending"
        );
        assert!(
            !s.ignore_word_in_sentence(&["Stil-", "und", "Haus"], 0),
            "a plain noun is not a compound continuation"
        );
    }

    #[test]
    fn lowercase_mitarbeitende_is_ignored_through_the_suffix_swap() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(s.ignore_word_in_sentence(&["mitarbeitende"], 0));
    }

    #[test]
    fn binnen_i_gate_rejects_unspellable_capitalized_inner_forms() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(
            s.is_misspelled("BundeskanzlerInnen"),
            "'kanzlerinnen' alone is not spellable, so the Binnen-I form stays flagged"
        );
        assert!(
            s.is_misspelled("Qxberg*stein"),
            "star split stays consistent"
        );
        assert!(
            s.is_misspelled("AktienIndex"),
            "a part starting with the Binnen-I is rejected"
        );
    }

    #[test]
    fn invalid_compound_parts_and_lemma_retries_stay_flagged() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(
            s.is_misspelled("Westenhimmel"),
            "'westen' is an invalid first part"
        );
        assert!(
            s.is_misspelled("Winterkamp"),
            "'kamp' is an invalid second part"
        );
        assert!(
            s.is_misspelled("Kassenxyz"),
            "singular part one needs a plural"
        );
        assert!(
            s.is_misspelled("Linksqxqworts"),
            "unknown s-final second part retries the lemma without the s"
        );
    }

    #[test]
    fn verb_stem_and_uppercase_prefixes_accept_their_compounds() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(
            !s.is_misspelled("Abblasmuster"),
            "verb stem plus s joins a noun"
        );
        assert!(
            !s.is_misspelled("DAX-Wert"),
            "all-uppercase first part is known"
        );
    }

    #[test]
    fn three_part_families_cover_s_link_other_prefixes_and_verb_stems() {
        let Some(s) = speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        for word in [
            "Linksverkehrsregel",
            "Rechtsprachgebrauch",
            "Schwarz-Rot-Gold",
            "Weltenwende",
        ] {
            assert!(!s.is_misspelled(word), "'{word}' must be accepted");
        }
        assert!(
            s.is_misspelled("Bundesverkehrsamt"),
            "s-linked three-part split still fails on the short tail"
        );
        assert!(s.is_misspelled("Wilhelmkamp"));
        assert!(
            s.is_misspelled("Bachforelle"),
            "plain 'Bach' matches no confusion family"
        );
    }

    #[test]
    fn taggerless_speller_accepts_the_kennenlernen_family() {
        let Some(s) = taggerless_speller() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        assert!(!s.is_misspelled("Sozialkennenlernen"));
    }

    #[test]
    fn match_sentence_skips_ignored_and_non_word_tokens() {
        use og_core::AnalyzedTokenReadings;
        let Some(s) = speller_arc() else {
            eprintln!("Skipping: German resources not present");
            return;
        };
        let rule = GermanSpellerRule::new(Arc::clone(s), "DE_HUNSPELL_RULE");
        let mut sentence = AnalyzedSentence::new("123 ostbahn", 0, "123 ostbahn".len());
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(AnalyzedToken::new("123", 0, 3)),
            AnalyzedTokenReadings::new(AnalyzedToken::new(" ", 3, 4)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("ostbahn", 4, 11)),
        ]);
        assert!(
            rule.match_sentence(&sentence).is_empty(),
            "digit tokens and sentence-ignored direction words produce no match"
        );
    }
}
