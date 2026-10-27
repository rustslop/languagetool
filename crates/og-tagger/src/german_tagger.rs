use crate::Tagger;
use crate::morfologik::MorfologikDictionary;
use og_core::AnalyzedToken;
use og_core::AnalyzedTokenReadings;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;

type Readings = Vec<(&'static str, &'static str)>;
type ReadingMap = HashMap<Box<str>, Readings>;

const PARTIZIP2CONTAINS1PLUPRA: &[&str] = &[
    "blasen", "fahren", "fallen", "fangen", "fressen", "geben", "halten", "kommen", "laden",
    "lassen", "laufen", "lesen", "messen", "raten", "schlafen", "schlagen", "sehen", "tragen",
    "treten",
];

const PARTIZIP2CONTAINS1PLUPRT: &[&str] = &[
    "bieten",
    "bleiben",
    "fliegen",
    "fließen",
    "heben",
    "leiden",
    "meiden",
    "scheiden",
    "schließen",
    "schreiben",
    "stehen",
    "steigen",
    "streiten",
    "treiben",
    "weisen",
    "ziehen",
];

const POSTAGSPARTIZIPENDINGE: &[&str] = &[
    "AKK:PLU:FEM:GRU:SOL:VER",
    "AKK:PLU:MAS:GRU:SOL:VER",
    "AKK:PLU:NEU:GRU:SOL:VER",
    "AKK:SIN:FEM:GRU:DEF:VER",
    "AKK:SIN:FEM:GRU:IND:VER",
    "AKK:SIN:FEM:GRU:SOL:VER",
    "AKK:SIN:NEU:GRU:DEF:VER",
    "NOM:PLU:FEM:GRU:SOL:VER",
    "NOM:PLU:MAS:GRU:SOL:VER",
    "NOM:PLU:NEU:GRU:SOL:VER",
    "NOM:SIN:FEM:GRU:DEF:VER",
    "NOM:SIN:FEM:GRU:IND:VER",
    "NOM:SIN:FEM:GRU:SOL:VER",
    "NOM:SIN:MAS:GRU:DEF:VER",
    "NOM:SIN:NEU:GRU:DEF:VER",
];

const POSTAGSPARTIZIPENDINGEM: &[&str] = &["DAT:SIN:MAS:GRU:SOL:VER", "DAT:SIN:NEU:GRU:SOL:VER"];

const POSTAGSPARTIZIPENDINGEN: &[&str] = &[
    "AKK:PLU:FEM:GRU:DEF:VER",
    "AKK:PLU:FEM:GRU:IND:VER",
    "AKK:PLU:MAS:GRU:DEF:VER",
    "AKK:PLU:MAS:GRU:IND:VER",
    "AKK:PLU:NEU:GRU:DEF:VER",
    "AKK:PLU:NEU:GRU:IND:VER",
    "AKK:SIN:MAS:GRU:DEF:VER",
    "AKK:SIN:MAS:GRU:IND:VER",
    "AKK:SIN:MAS:GRU:SOL:VER",
    "DAT:PLU:FEM:GRU:DEF:VER",
    "DAT:PLU:FEM:GRU:IND:VER",
    "DAT:PLU:FEM:GRU:SOL:VER",
    "DAT:PLU:MAS:GRU:DEF:VER",
    "DAT:PLU:MAS:GRU:IND:VER",
    "DAT:PLU:MAS:GRU:SOL:VER",
    "DAT:PLU:NEU:GRU:DEF:VER",
    "DAT:PLU:NEU:GRU:IND:VER",
    "DAT:PLU:NEU:GRU:SOL:VER",
    "DAT:SIN:FEM:GRU:DEF:VER",
    "DAT:SIN:FEM:GRU:IND:VER",
    "DAT:SIN:MAS:GRU:DEF:VER",
    "DAT:SIN:MAS:GRU:IND:VER",
    "DAT:SIN:NEU:GRU:DEF:VER",
    "DAT:SIN:NEU:GRU:IND:VER",
    "GEN:PLU:FEM:GRU:DEF:VER",
    "GEN:PLU:FEM:GRU:IND:VER",
    "GEN:PLU:MAS:GRU:DEF:VER",
    "GEN:PLU:MAS:GRU:IND:VER",
    "GEN:PLU:NEU:GRU:DEF:VER",
    "GEN:PLU:NEU:GRU:IND:VER",
    "GEN:SIN:FEM:GRU:DEF:VER",
    "GEN:SIN:FEM:GRU:IND:VER",
    "GEN:SIN:MAS:GRU:DEF:VER",
    "GEN:SIN:MAS:GRU:IND:VER",
    "GEN:SIN:MAS:GRU:SOL:VER",
    "GEN:SIN:NEU:GRU:DEF:VER",
    "GEN:SIN:NEU:GRU:IND:VER",
    "GEN:SIN:NEU:GRU:SOL:VER",
    "NOM:PLU:FEM:GRU:DEF:VER",
    "NOM:PLU:FEM:GRU:IND:VER",
    "NOM:PLU:MAS:GRU:DEF:VER",
    "NOM:PLU:MAS:GRU:IND:VER",
    "NOM:PLU:NEU:GRU:DEF:VER",
    "NOM:PLU:NEU:GRU:IND:VER",
];

const POSTAGSPARTIZIPENDINGER: &[&str] = &[
    "DAT:SIN:FEM:GRU:SOL:VER",
    "GEN:PLU:FEM:GRU:SOL:VER",
    "GEN:PLU:MAS:GRU:SOL:VER",
    "GEN:PLU:NEU:GRU:SOL:VER",
    "GEN:SIN:FEM:GRU:SOL:VER",
    "NOM:SIN:MAS:GRU:IND:VER",
    "NOM:SIN:MAS:GRU:SOL:VER",
    "DAT:SIN:FEM:GRU:SOL:VER",
    "GEN:PLU:FEM:GRU:SOL:VER",
    "GEN:PLU:MAS:GRU:SOL:VER",
    "GEN:PLU:NEU:GRU:SOL:VER",
    "GEN:SIN:FEM:GRU:SOL:VER",
    "NOM:SIN:MAS:GRU:IND:VER",
    "NOM:SIN:MAS:GRU:SOL:VER",
];

const POSTAGSPARTIZIPENDINGES: &[&str] = &[
    "AKK:SIN:NEU:GRU:IND:VER",
    "AKK:SIN:NEU:GRU:SOL:VER",
    "NOM:SIN:NEU:GRU:IND:VER",
    "NOM:SIN:NEU:GRU:SOL:VER",
];

const NOT_A_VERB: &[&str] = &[
    "angebot",
    "anteil",
    "aufenthalt",
    "ausdruck",
    "auswärtsspiel",
    "beispiel",
    "bereich",
    "besondere",
    "daring",
    "einfach",
    "einfachst",
    "endkasten",
    "freibetrag",
    "grautöne",
    "grüntöne",
    "großherzöge",
    "großteil",
    "hochhaus",
    "klarerweise",
    "maßnahme",
    "mitglieder",
    "nachricht",
    "nebenfach",
    "niederlage",
    "nothing",
    "notscheid",
    "preisver",
    "reinweiß",
    "schwarzweiß",
    "schwarzgrau",
    "schwarzgrün",
    "schwarztöne",
    "unbesiegt",
    "unmenge",
    "unrat",
    "unver",
    "verrückterweise",
    "versonnen",
    "vorlieb",
    "vorteil",
    "warmweiß",
    "wohldefiniert",
    "wohlergehen",
    "wohlgemerkt",
    "zuende",
    "zuhause",
    "zumal",
    "zuver",
    "darauf",
    "einmal",
    "kleinkram",
    "hochsicher",
    "ehering",
    "freitag",
    "großmeister",
    "handwerk",
    "herpes",
    "nachfolger",
];

const PREFIXES_SEPARABLE: &[&str] = &[
    "gegeneinander",
    "durcheinander",
    "nebeneinander",
    "übereinander",
    "aufeinander",
    "auseinander",
    "beieinander",
    "aneinander",
    "ineinander",
    "zueinander",
    "gegenüber",
    "beisammen",
    "gegenüber",
    "hernieder",
    "rückwärts",
    "wiederauf",
    "wiederein",
    "wiederher",
    "zufrieden",
    "zwangsvor",
    "entgegen",
    "hinunter",
    "abhanden",
    "aufrecht",
    "aufwärts",
    "auswärts",
    "beiseite",
    "danieder",
    "drauflos",
    "einwärts",
    "herunter",
    "hindurch",
    "verrückt",
    "vorwärts",
    "zunichte",
    "zusammen",
    "zwangsum",
    "zwischen",
    "abseits",
    "abwärts",
    "entlang",
    "hinfort",
    "ähnlich",
    "daneben",
    "general",
    "herüber",
    "hierher",
    "hierhin",
    "hinüber",
    "schwarz",
    "trocken",
    "überein",
    "vorlieb",
    "vorüber",
    "wichtig",
    "zurecht",
    "zuwider",
    "hinweg",
    "allein",
    "besser",
    "daheim",
    "doppel",
    "feinst",
    "fertig",
    "herauf",
    "heraus",
    "herbei",
    "hinauf",
    "hinaus",
    "hinein",
    "kaputt",
    "kennen",
    "kürzer",
    "mittag",
    "nieder",
    "runter",
    "sicher",
    "sitzen",
    "voraus",
    "vorbei",
    "vorweg",
    "weiter",
    "wieder",
    "zugute",
    "zurück",
    "zwangs",
    "abend",
    "blank",
    "brust",
    "dahin",
    "davon",
    "drauf",
    "drein",
    "durch",
    "einig",
    "empor",
    "grund",
    "herum",
    "höher",
    "klein",
    "knapp",
    "krank",
    "krumm",
    "kugel",
    "näher",
    "neben",
    "offen",
    "preis",
    "rüber",
    "ruhig",
    "statt",
    "still",
    "übrig",
    "umher",
    "unter",
    "voran",
    "zweck",
    "acht",
    "drei",
    "fehl",
    "feil",
    "fort",
    "frei",
    "groß",
    "hand",
    "hart",
    "heim",
    "hier",
    "hoch",
    "klar",
    "lahm",
    "miss",
    "nach",
    "nahe",
    "quer",
    "rauf",
    "raus",
    "rein",
    "rück",
    "satt",
    "stoß",
    "teil",
    "über",
    "voll",
    "wach",
    "wahr",
    "warm",
    "wert",
    "wohl",
    "auf",
    "aus",
    "bei",
    "ehe",
    "ein",
    "eis",
    "end",
    "her",
    "hin",
    "los",
    "maß",
    "mit",
    "out",
    "ran",
    "rum",
    "tot",
    "vor",
    "weg",
    "weh",
    "ab",
    "an",
    "da",
    "um",
    "zu",
];
const PREFIXES_NON_SEPARABLE: &[&str] = &[
    "be", "emp", "ent", "er", "hinter", "miss", "un", "ver", "zer",
];
const PREFIXES_VERBS: &[&str] = &[
    "gegeneinander",
    "durcheinander",
    "nebeneinander",
    "übereinander",
    "aufeinander",
    "auseinander",
    "beieinander",
    "aneinander",
    "ineinander",
    "zueinander",
    "gegenüber",
    "beisammen",
    "gegenüber",
    "hernieder",
    "rückwärts",
    "wiederauf",
    "wiederein",
    "wiederher",
    "zufrieden",
    "zwangsvor",
    "entgegen",
    "hinunter",
    "abhanden",
    "aufrecht",
    "aufwärts",
    "auswärts",
    "beiseite",
    "danieder",
    "drauflos",
    "einwärts",
    "herunter",
    "hindurch",
    "verrückt",
    "vorwärts",
    "zunichte",
    "zusammen",
    "zwangsum",
    "zwischen",
    "abseits",
    "abwärts",
    "entlang",
    "hinfort",
    "ähnlich",
    "daneben",
    "general",
    "herüber",
    "hierher",
    "hierhin",
    "hinüber",
    "schwarz",
    "trocken",
    "überein",
    "vorlieb",
    "vorüber",
    "wichtig",
    "zurecht",
    "zuwider",
    "hinweg",
    "hinter",
    "allein",
    "besser",
    "daheim",
    "doppel",
    "feinst",
    "fertig",
    "herauf",
    "heraus",
    "herbei",
    "hinauf",
    "hinaus",
    "hinein",
    "kaputt",
    "kennen",
    "kürzer",
    "mittag",
    "nieder",
    "runter",
    "sicher",
    "sitzen",
    "voraus",
    "vorbei",
    "vorweg",
    "weiter",
    "wieder",
    "zugute",
    "zurück",
    "zwangs",
    "abend",
    "blank",
    "brust",
    "dahin",
    "davon",
    "drauf",
    "drein",
    "durch",
    "einig",
    "empor",
    "grund",
    "herum",
    "höher",
    "klein",
    "knapp",
    "krank",
    "krumm",
    "kugel",
    "näher",
    "neben",
    "offen",
    "preis",
    "rüber",
    "ruhig",
    "statt",
    "still",
    "übrig",
    "umher",
    "unter",
    "voran",
    "zweck",
    "miss",
    "acht",
    "drei",
    "fehl",
    "feil",
    "fort",
    "frei",
    "groß",
    "hand",
    "hart",
    "heim",
    "hier",
    "hoch",
    "klar",
    "lahm",
    "miss",
    "nach",
    "nahe",
    "quer",
    "rauf",
    "raus",
    "rein",
    "rück",
    "satt",
    "stoß",
    "teil",
    "über",
    "voll",
    "wach",
    "wahr",
    "warm",
    "wert",
    "wohl",
    "emp",
    "ent",
    "ver",
    "zer",
    "auf",
    "aus",
    "bei",
    "ehe",
    "ein",
    "eis",
    "end",
    "her",
    "hin",
    "los",
    "maß",
    "mit",
    "out",
    "ran",
    "rum",
    "tot",
    "vor",
    "weg",
    "weh",
    "be",
    "er",
    "un",
    "ab",
    "an",
    "da",
    "um",
    "zu",
];
const COMPOUND_VERB_PREFIXES: &[&str] = &[
    "ab",
    "abend",
    "abhanden",
    "acht",
    "ähnlich",
    "allein",
    "an",
    "auf",
    "aufeinander",
    "aufrecht",
    "aufwärts",
    "aus",
    "auseinander",
    "auswärts",
    "bei",
    "beieinander",
    "beisammen",
    "beiseite",
    "besser",
    "blank",
    "brust",
    "da",
    "daheim",
    "dahin",
    "daneben",
    "danieder",
    "davon",
    "doppel",
    "drauflos",
    "drei",
    "drein",
    "durch",
    "durcheinander",
    "ehe",
    "ein",
    "einig",
    "einwärts",
    "eis",
    "empor",
    "end",
    "fehl",
    "feil",
    "feinst",
    "fort",
    "frei",
    "gegenüber",
    "general",
    "groß",
    "grund",
    "hand",
    "hart",
    "heim",
    "her",
    "herauf",
    "heraus",
    "herbei",
    "hernieder",
    "herüber",
    "herum",
    "herunter",
    "hier",
    "hierher",
    "hierhin",
    "hin",
    "hinauf",
    "hinaus",
    "hindurch",
    "hinein",
    "hinüber",
    "hoch",
    "höher",
    "ineinander",
    "kaputt",
    "kennen",
    "klar",
    "klein",
    "knapp",
    "krank",
    "krumm",
    "kugel",
    "kürzer",
    "lahm",
    "los",
    "maß",
    "miss",
    "mit",
    "mittag",
    "nach",
    "nahe",
    "näher",
    "neben",
    "nebeneinander",
    "nieder",
    "offen",
    "out",
    "preis",
    "quer",
    "ran",
    "rauf",
    "raus",
    "rein",
    "rüber",
    "rück",
    "rückwärts",
    "ruhig",
    "rum",
    "runter",
    "satt",
    "schwarz",
    "sicher",
    "sitzen",
    "statt",
    "still",
    "stoß",
    "teil",
    "tot",
    "trocken",
    "über",
    "überein",
    "übereinander",
    "übrig",
    "um",
    "umher",
    "unter",
    "verrückt",
    "voll",
    "vor",
    "voran",
    "voraus",
    "vorbei",
    "vorlieb",
    "vorüber",
    "vorwärts",
    "vorweg",
    "wach",
    "wahr",
    "warm",
    "weg",
    "weh",
    "weiter",
    "wert",
    "wichtig",
    "wieder",
    "wiederauf",
    "wiederein",
    "wiederher",
    "wohl",
    "zu",
    "zueinander",
    "zufrieden",
    "zugute",
    "zunichte",
    "zurecht",
    "zurück",
    "zusammen",
    "zuwider",
    "zwangs",
    "zwangsum",
    "zwangsvor",
    "zweck",
    "zwischen",
];
const ADJ_PREFIX_STRIP: &[&str] = &[
    "bitter", "dunkel", "erz", "extra", "früh", "gemein", "grund", "hyper", "lau", "mega",
    "minder", "stock", "super", "tod", "ultra", "u[nr]", "voll",
];
const ADJ_PREFIX_GATE: &[&str] = &[
    "bitter", "dunkel", "erz", "extra", "früh", "gemein", "hyper", "lau", "mega", "minder",
    "stock", "super", "tod", "ultra", "un", "ur",
];
const TAGS_FOR_WEISE: &[&str] = &[
    "ADJ:AKK:PLU:FEM:GRU:SOL",
    "ADJ:AKK:PLU:MAS:GRU:SOL",
    "ADJ:AKK:PLU:NEU:GRU:SOL",
    "ADJ:AKK:SIN:FEM:GRU:DEF",
    "ADJ:AKK:SIN:FEM:GRU:IND",
    "ADJ:AKK:SIN:FEM:GRU:SOL",
    "ADJ:AKK:SIN:NEU:GRU:DEF",
    "ADJ:NOM:PLU:FEM:GRU:SOL",
    "ADJ:NOM:PLU:MAS:GRU:SOL",
    "ADJ:NOM:PLU:NEU:GRU:SOL",
    "ADJ:NOM:SIN:FEM:GRU:DEF",
    "ADJ:NOM:SIN:FEM:GRU:IND",
    "ADJ:NOM:SIN:FEM:GRU:SOL",
    "ADJ:NOM:SIN:MAS:GRU:DEF",
    "ADJ:NOM:SIN:NEU:GRU:DEF",
    "ADJ:PRD:GRU",
];
const NOUN_TAG_EXPANSION_EXCEPTIONS: &[&str] = &["Wegstrecken"];
const NO_IMPERATIVE_SWAP: &[&str] = &["gar", "mal", "null", "trotz"];
const TAGS_FOR_ADJ: &[&str] = &["ADJ:PRD:GRU"];
const TAGS_FOR_ADJ_E: &[&str] = &[
    "ADJ:AKK:PLU:FEM:GRU:SOL",
    "ADJ:AKK:PLU:MAS:GRU:SOL",
    "ADJ:AKK:PLU:NEU:GRU:SOL",
    "ADJ:AKK:SIN:FEM:GRU:DEF",
    "ADJ:AKK:SIN:FEM:GRU:IND",
    "ADJ:AKK:SIN:FEM:GRU:SOL",
    "ADJ:AKK:SIN:NEU:GRU:DEF",
    "ADJ:NOM:PLU:FEM:GRU:SOL",
    "ADJ:NOM:PLU:MAS:GRU:SOL",
    "ADJ:NOM:PLU:NEU:GRU:SOL",
    "ADJ:NOM:SIN:FEM:GRU:DEF",
    "ADJ:NOM:SIN:FEM:GRU:IND",
    "ADJ:NOM:SIN:FEM:GRU:SOL",
    "ADJ:NOM:SIN:MAS:GRU:DEF",
    "ADJ:NOM:SIN:NEU:GRU:DEF",
];
const TAGS_FOR_ADJ_EN: &[&str] = &[
    "ADJ:AKK:PLU:FEM:GRU:DEF",
    "ADJ:AKK:PLU:FEM:GRU:IND",
    "ADJ:AKK:PLU:MAS:GRU:DEF",
    "ADJ:AKK:PLU:MAS:GRU:IND",
    "ADJ:AKK:PLU:NEU:GRU:DEF",
    "ADJ:AKK:PLU:NEU:GRU:IND",
    "ADJ:AKK:SIN:MAS:GRU:DEF",
    "ADJ:AKK:SIN:MAS:GRU:IND",
    "ADJ:AKK:SIN:MAS:GRU:SOL",
    "ADJ:DAT:PLU:FEM:GRU:DEF",
    "ADJ:DAT:PLU:FEM:GRU:IND",
    "ADJ:DAT:PLU:FEM:GRU:SOL",
    "ADJ:DAT:PLU:MAS:GRU:DEF",
    "ADJ:DAT:PLU:MAS:GRU:IND",
    "ADJ:DAT:PLU:MAS:GRU:SOL",
    "ADJ:DAT:PLU:NEU:GRU:DEF",
    "ADJ:DAT:PLU:NEU:GRU:IND",
    "ADJ:DAT:PLU:NEU:GRU:SOL",
    "ADJ:DAT:SIN:FEM:GRU:DEF",
    "ADJ:DAT:SIN:FEM:GRU:IND",
    "ADJ:DAT:SIN:MAS:GRU:DEF",
    "ADJ:DAT:SIN:MAS:GRU:IND",
    "ADJ:DAT:SIN:NEU:GRU:DEF",
    "ADJ:DAT:SIN:NEU:GRU:IND",
    "ADJ:GEN:PLU:FEM:GRU:DEF",
    "ADJ:GEN:PLU:FEM:GRU:IND",
    "ADJ:GEN:PLU:MAS:GRU:DEF",
    "ADJ:GEN:PLU:MAS:GRU:IND",
    "ADJ:GEN:PLU:NEU:GRU:DEF",
    "ADJ:GEN:PLU:NEU:GRU:IND",
    "ADJ:GEN:SIN:FEM:GRU:DEF",
    "ADJ:GEN:SIN:FEM:GRU:IND",
    "ADJ:GEN:SIN:MAS:GRU:DEF",
    "ADJ:GEN:SIN:MAS:GRU:IND",
    "ADJ:GEN:SIN:MAS:GRU:SOL",
    "ADJ:GEN:SIN:NEU:GRU:DEF",
    "ADJ:GEN:SIN:NEU:GRU:IND",
    "ADJ:GEN:SIN:NEU:GRU:SOL",
    "ADJ:NOM:PLU:FEM:GRU:DEF",
    "ADJ:NOM:PLU:FEM:GRU:IND",
    "ADJ:NOM:PLU:MAS:GRU:DEF",
    "ADJ:NOM:PLU:MAS:GRU:IND",
    "ADJ:NOM:PLU:NEU:GRU:DEF",
    "ADJ:NOM:PLU:NEU:GRU:IND",
];
const TAGS_FOR_ADJ_ER: &[&str] = &[
    "ADJ:DAT:SIN:FEM:GRU:SOL",
    "ADJ:GEN:PLU:FEM:GRU:SOL",
    "ADJ:GEN:PLU:MAS:GRU:SOL",
    "ADJ:GEN:PLU:NEU:GRU:SOL",
    "ADJ:GEN:SIN:FEM:GRU:SOL",
    "ADJ:NOM:SIN:MAS:GRU:IND",
    "ADJ:NOM:SIN:MAS:GRU:SOL",
];
const TAGS_FOR_ADJ_EM: &[&str] = &["ADJ:DAT:SIN:MAS:GRU:SOL", "ADJ:DAT:SIN:NEU:GRU:SOL"];
const TAGS_FOR_ADJ_ES: &[&str] = &[
    "ADJ:AKK:SIN:NEU:GRU:IND",
    "ADJ:AKK:SIN:NEU:GRU:SOL",
    "ADJ:NOM:SIN:NEU:GRU:IND",
    "ADJ:NOM:SIN:NEU:GRU:SOL",
];

type SentTokens<'a> = &'a [(&'a str, usize, usize)];

struct Occurrence<'a> {
    word: &'a str,
    start: usize,
    end: usize,
}

struct TokenCtx {
    first_word: bool,
    prev_word: Option<String>,
    pos: usize,
}

impl TokenCtx {
    fn new() -> Self {
        Self {
            first_word: true,
            prev_word: None,
            pos: 0,
        }
    }
}

struct PrefixInfixVerb {
    prefix: &'static str,
    infix: &'static str,
    verb_baseform: &'static str,
}

struct NominalizedVerb {
    prefix: &'static str,
    verb_baseform: &'static str,
}

struct AdjInfo {
    baseform: &'static str,
    tag: &'static str,
}

#[derive(Default)]
struct ExpansionInfos {
    verb_infos: HashMap<&'static str, PrefixInfixVerb>,
    nom_verb_infos: HashMap<&'static str, NominalizedVerb>,
    nom_gen_verb_infos: HashMap<&'static str, NominalizedVerb>,
    adj_infos: HashMap<&'static str, Vec<AdjInfo>>,
}

pub struct GermanTagger {
    dictionary: Arc<MorfologikDictionary>,
    manual: ReadingMap,
    removals: ReadingMap,
    expansion: OnceLock<Arc<ExpansionInfos>>,
    resource_dir: Option<std::path::PathBuf>,
}

pub fn shared_german_dictionary(dir: &Path) -> Option<Arc<MorfologikDictionary>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<std::path::PathBuf, std::sync::Weak<MorfologikDictionary>>,
        >,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = dir.join("german.dict");
    let mut guard = cache.lock().unwrap();
    if let Some(dict) = guard.get(&key).and_then(std::sync::Weak::upgrade) {
        return Some(dict);
    }
    let dictionary = Arc::new(MorfologikDictionary::load(
        &dir.join("german.dict"),
        &dir.join("german.info"),
    )?);
    guard.insert(key, std::sync::Arc::downgrade(&dictionary));
    Some(dictionary)
}

fn load_manual_map(dir: &Path, files: &[&str]) -> ReadingMap {
    let mut map: ReadingMap = HashMap::new();
    for file in files {
        let Ok(content) = std::fs::read_to_string(dir.join(file)) else {
            continue;
        };
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let (Some(form), Some(lemma), Some(tag)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let form_leaked: &'static str = Box::leak(form.to_string().into_boxed_str());
            let lemma_leaked: &'static str = Box::leak(lemma.to_string().into_boxed_str());
            let tag_leaked: &'static str = Box::leak(tag.trim().to_string().into_boxed_str());
            map.entry(form_leaked.into())
                .or_default()
                .push((lemma_leaked, tag_leaked));
        }
    }
    map
}

fn to_pa2(tags: &[&str]) -> Vec<&'static str> {
    tags.iter()
        .map(|t| {
            Box::leak(format!("{}:VER", t.replace("ADJ:", "PA2:")).into_boxed_str()) as &'static str
        })
        .collect()
}

fn fill_adj_infos(
    word: &'static str,
    suffix: &str,
    tags: &[&'static str],
    adj_infos: &mut HashMap<&'static str, Vec<AdjInfo>>,
) {
    let fullform: &'static str = Box::leak(format!("{word}{suffix}").into_boxed_str());
    let list: Vec<AdjInfo> = tags
        .iter()
        .map(|t| AdjInfo {
            baseform: word,
            tag: t,
        })
        .collect();
    adj_infos.insert(fullform, list);
}

fn synthesize_verb_forms(dictionary: &MorfologikDictionary, lemma: &str) -> Vec<&'static str> {
    let mut forms: Vec<&'static str> = Vec::new();
    let lc_lemma = starts_lowercase(lemma);
    if let Some(words) = dictionary.verb_forms_by_lemma().get(lemma) {
        for word in words {
            let leaked: &'static str = Box::leak(word.clone().into_boxed_str());
            if leaked.contains('ß') {
                continue;
            }
            if starts_lowercase(leaked) == lc_lemma && !forms.contains(&leaked) {
                forms.push(leaked);
            }
        }
    }
    forms
}

fn add_verb_line(line: &str, infos: &mut ExpansionInfos, dictionary: &MorfologikDictionary) {
    if !line.contains('_') || line.ends_with("_in") {
        return;
    }
    let cleaned = match line.find('#') {
        Some(idx) => &line[..idx],
        None => line,
    }
    .trim();
    let mut parts = cleaned.split('_');
    let (Some(prefix), Some(verb_baseform)) = (parts.next(), parts.next()) else {
        return;
    };
    let prefix: &'static str = Box::leak(prefix.to_string().into_boxed_str());
    let verb_baseform: &'static str = Box::leak(verb_baseform.to_string().into_boxed_str());
    for form in synthesize_verb_forms(dictionary, verb_baseform) {
        let key: &'static str = Box::leak(format!("{prefix}{form}").into_boxed_str());
        infos.verb_infos.insert(
            key,
            PrefixInfixVerb {
                prefix,
                infix: "",
                verb_baseform,
            },
        );
    }
    let zu_key: &'static str = Box::leak(format!("{prefix}zu{verb_baseform}").into_boxed_str());
    infos.verb_infos.insert(
        zu_key,
        PrefixInfixVerb {
            prefix,
            infix: "zu",
            verb_baseform,
        },
    );
    let upper_prefix: &'static str = Box::leak(uppercase_first(prefix).into_boxed_str());
    let nom_key: &'static str =
        Box::leak(format!("{upper_prefix}{verb_baseform}").into_boxed_str());
    infos.nom_verb_infos.insert(
        nom_key,
        NominalizedVerb {
            prefix: upper_prefix,
            verb_baseform,
        },
    );
    let gen_key: &'static str = Box::leak(format!("{nom_key}s").into_boxed_str());
    infos.nom_gen_verb_infos.insert(
        gen_key,
        NominalizedVerb {
            prefix: upper_prefix,
            verb_baseform,
        },
    );
}

impl GermanTagger {
    pub fn from_resource_dir(dir: &Path) -> Option<Self> {
        let dictionary = shared_german_dictionary(dir)?;
        let manual = load_manual_map(dir, &["added.txt", "added_custom.txt"]);
        let removals = load_manual_map(dir, &["removed.txt", "removed_custom.txt"]);
        Some(Self {
            dictionary,
            manual,
            removals,
            expansion: OnceLock::new(),
            resource_dir: Some(dir.to_path_buf()),
        })
    }

    pub fn load_manual_data(&mut self, added: &str, removed: &str) {
        append_manual_lines(&mut self.manual, added);
        append_manual_lines(&mut self.removals, removed);
    }

    pub fn entry_count(&self) -> usize {
        self.dictionary.len()
    }

    fn word_tag(&self, word: &str) -> Readings {
        let mut readings: Readings = self.manual.get(word).cloned().unwrap_or_default();
        readings.extend(self.dictionary.lookup(word));
        if let Some(removals) = self.removals.get(word) {
            readings.retain(|(lemma, tag)| {
                !removals
                    .iter()
                    .any(|(r_lemma, r_tag)| r_lemma == lemma && r_tag == tag)
            });
        }
        readings
    }

    fn expansions(&self) -> &Arc<ExpansionInfos> {
        if let Some(cached) = self.expansion.get() {
            return cached;
        }
        static SHARED: og_core::releasable::Releasable<
            std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Arc<ExpansionInfos>>>,
        > = og_core::releasable::Releasable::new();
        let shared = og_core::releasable::shared_get_or_build(&SHARED, || {
            Some(std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )))
        })
        .expect("expansion cache");
        let key = self
            .resource_dir
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("/nonexistent"));
        let built = {
            let mut guard = shared.lock().unwrap();
            Arc::clone(
                guard
                    .entry(key)
                    .or_insert_with(|| Arc::new(self.build_expansion_infos())),
            )
        };
        let _ = self.expansion.set(built);
        self.expansion.get().expect("expansion just set")
    }

    fn build_expansion_infos(&self) -> ExpansionInfos {
        let mut infos = ExpansionInfos::default();
        let Some(dir) = &self.resource_dir else {
            return infos;
        };
        let Ok(spelling) = std::fs::read_to_string(dir.join("hunspell/spelling.txt")) else {
            return infos;
        };
        let adj_tag_sets: [(&[&'static str], &str); 6] = [
            (TAGS_FOR_ADJ, ""),
            (TAGS_FOR_ADJ_E, "e"),
            (TAGS_FOR_ADJ_EN, "en"),
            (TAGS_FOR_ADJ_ER, "er"),
            (TAGS_FOR_ADJ_EM, "em"),
            (TAGS_FOR_ADJ_ES, "es"),
        ];
        let pa2_sets: [Vec<&'static str>; 6] = [
            to_pa2(TAGS_FOR_ADJ),
            to_pa2(TAGS_FOR_ADJ_E),
            to_pa2(TAGS_FOR_ADJ_EN),
            to_pa2(TAGS_FOR_ADJ_ER),
            to_pa2(TAGS_FOR_ADJ_EM),
            to_pa2(TAGS_FOR_ADJ_ES),
        ];
        for line_original in spelling.lines() {
            if line_original.is_empty() || line_original.starts_with('#') {
                continue;
            }
            let line = match line_original.find('#') {
                Some(idx) => line_original[..idx].trim(),
                None => line_original.trim(),
            };
            if line.is_empty() || line.ends_with("/PA") || line.ends_with("/AP") {
                continue;
            }
            let base = line.split('/').next().unwrap_or(line);
            if line.ends_with("/P") {
                let word: &'static str = Box::leak(base.to_string().into_boxed_str());
                for (i, (_, suffix)) in adj_tag_sets.iter().enumerate() {
                    fill_adj_infos(word, suffix, &pa2_sets[i], &mut infos.adj_infos);
                }
            } else if line.ends_with("/A") && !line.ends_with("ste/A") && !line.ends_with("er/A") {
                let word: &'static str = Box::leak(base.to_string().into_boxed_str());
                for (tags, suffix) in adj_tag_sets.iter() {
                    fill_adj_infos(word, suffix, tags, &mut infos.adj_infos);
                }
            } else {
                add_verb_line(line, &mut infos, &self.dictionary);
            }
        }
        infos
    }

    pub fn tag_word(&self, surface: &str, start: usize, end: usize) -> Vec<AnalyzedToken> {
        let tokens: Vec<(&str, usize, usize)> = vec![(surface, start, end)];
        let mut ctx = TokenCtx::new();
        self.analyze_token(surface, start, end, &tokens, 0, &mut ctx)
    }

    pub fn lookup(&self, word: &str) -> Option<Vec<AnalyzedToken>> {
        let tokens: Vec<(&str, usize, usize)> = vec![(word, 0, word.len())];
        let mut ctx = TokenCtx {
            first_word: false,
            prev_word: None,
            pos: usize::MAX,
        };
        let readings = self.analyze_token(word, 0, word.len(), &tokens, 0, &mut ctx);
        if readings.first().is_some_and(|r| r.pos_tags().is_empty()) {
            return None;
        }
        Some(readings)
    }

    fn analyze_token(
        &self,
        word: &str,
        start: usize,
        end: usize,
        tokens: SentTokens,
        token_idx: usize,
        ctx: &mut TokenCtx,
    ) -> Vec<AnalyzedToken> {
        let mut tagger_tokens: Option<Readings> = None;
        if token_idx + 2 < tokens.len() {
            let mid = tokens[token_idx + 1].0;
            let next = tokens[token_idx + 2].0;
            if matches!(mid, "*" | ":" | "_" | "/") {
                if matches_after_asterisk(next) {
                    let mut combined = self.word_tag(word);
                    combined.extend(self.word_tag(&format!("{word}{next}")));
                    tagger_tokens = Some(combined);
                } else if innen_pattern1(next) {
                    let last_part = next.rsplit_once('-').map_or(next, |(_, last)| last);
                    tagger_tokens = Some(self.word_tag(last_part));
                } else if innen_pattern2(next)
                    && let Some(idx) = next.rfind("innen")
                {
                    let tail = uppercase_first(&next[idx + "innen".len()..]);
                    tagger_tokens = Some(self.word_tag(&tail));
                }
            }
        }
        let mut tagger_tokens = tagger_tokens.unwrap_or_else(|| self.word_tag(word));

        let lower = word.to_lowercase();
        if (ctx.first_word || ctx.prev_word.as_deref() == Some(":")) && tagger_tokens.is_empty() {
            tagger_tokens = self.word_tag(&lower);
            ctx.first_word = !is_alphanumeric(word);
        } else if ctx.pos == 0 {
            tagger_tokens.extend(self.word_tag(&lower));
        } else if ctx.pos > 1
            && tagger_tokens.is_empty()
            && let Some(idx) = tokens.iter().position(|t| t.0 == word).filter(|i| *i > 2)
            && tokens[idx - 1].0 == "„"
            && tokens[idx - 3].0 == ":"
        {
            tagger_tokens.extend(self.word_tag(&lower));
        }

        let mut readings: Vec<AnalyzedToken> = Vec::new();
        if !tagger_tokens.is_empty() {
            readings.extend(
                tagger_tokens
                    .iter()
                    .map(|(l, t)| token_from(word, start, end, l, t)),
            );
            let word_lower = word.to_lowercase();
            if !starts_with_any(&word_lower, PREFIXES_SEPARABLE)
                && !contains_any(&word_lower, NOT_A_VERB)
                && (word == title_case(word) || word == word_lower)
            {
                let (lst_prt, frst_prt) = if starts_with_any(&word_lower, PREFIXES_NON_SEPARABLE) {
                    let lst = strip_prefix_re(&word_lower, PREFIXES_NON_SEPARABLE);
                    let frst = word.strip_suffix(lst.as_str()).unwrap_or("").to_string();
                    (lst, frst)
                } else {
                    (word.to_string(), String::new())
                };
                let verbs = self.word_tag(&lst_prt);
                let at_start = tokens.iter().position(|t| t.0 == word) == Some(0)
                    || word == lowercase_first(word);
                if at_start && !NO_IMPERATIVE_SWAP.contains(&lst_prt.as_str()) {
                    for (lemma, tag) in &verbs {
                        if tag.starts_with("VER:IMP:SIN:SFT")
                            && !readings_contain_tag(&readings, "VER:1:SIN:PRÄ:SFT")
                        {
                            readings.push(token_from(
                                word,
                                start,
                                end,
                                &format!("{}{}", frst_prt.to_lowercase(), lemma),
                                "VER:1:SIN:PRÄ:SFT",
                            ));
                        }
                        if tag.starts_with("VER:1:SIN:PRÄ:SFT")
                            && !readings_contain_tag(&readings, "VER:IMP:SIN:SFT")
                        {
                            readings.push(token_from(
                                word,
                                start,
                                end,
                                &format!("{}{}", frst_prt.to_lowercase(), lemma),
                                "VER:IMP:SIN:SFT",
                            ));
                        }
                    }
                }
            }
        } else {
            self.expand_unknown(
                Occurrence { word, start, end },
                tokens,
                token_idx,
                ctx,
                &mut readings,
            );
        }
        if readings.is_empty() {
            readings.push(no_info_token(word, start, end));
        }
        readings
    }

    fn expand_unknown(
        &self,
        at: Occurrence<'_>,
        tokens: SentTokens,
        token_idx: usize,
        ctx: &mut TokenCtx,
        readings: &mut Vec<AnalyzedToken>,
    ) {
        let Occurrence { word, start, end } = at;
        let expansions = self.expansions();
        let add_noun_tags = !NOUN_TAG_EXPANSION_EXCEPTIONS.contains(&word);
        let verb_info = expansions.verb_infos.get(word);
        let nom_verb_info = expansions.nom_verb_infos.get(word);
        let nom_gen_verb_info = expansions.nom_gen_verb_infos.get(word);
        let adj_infos = expansions.adj_infos.get(word);

        if let Some(info) = verb_info.filter(|i| starts_lowercase(i.prefix)) {
            let no_prefix_form = &word[info.prefix.len() + info.infix.len()..];
            let tags = self.word_tag(no_prefix_form);
            let mut is_sft = false;
            for (lemma, tag) in &tags {
                if (tag.starts_with("VER:") || tag.starts_with("PA1:") || tag.starts_with("PA2:"))
                    && !tag.starts_with("VER:MOD")
                    && !tag.starts_with("VER:AUX")
                {
                    let flektion = &tag[tag.len().saturating_sub(3)..];
                    if starts_with_any(info.prefix, PREFIXES_SEPARABLE)
                        && !contains_any(word, NOT_A_VERB)
                    {
                        let at_start = tokens.iter().position(|t| t.0 == word) == Some(0)
                            || word == lowercase_first(word);
                        if (tag.starts_with("VER:1")
                            || tag.starts_with("VER:2")
                            || tag.starts_with("VER:3"))
                            && at_start
                        {
                            readings.push(token_from(
                                word,
                                start,
                                end,
                                &format!("{}{}", info.prefix, lemma),
                                &format!("{tag}:NEB"),
                            ));
                        } else if !tag.starts_with("VER:IMP") {
                            readings.push(token_from(
                                word,
                                start,
                                end,
                                &format!("{}{}", info.prefix, lemma),
                                tag,
                            ));
                        } else if tag.starts_with("VER:IMP:SIN")
                            && (flektion == "SFT" || !has_i_then_more(word))
                        {
                            readings.push(token_from(
                                word,
                                start,
                                end,
                                &format!("{}{}", info.prefix, lemma),
                                &format!("VER:1:SIN:PRÄ:{flektion}:NEB"),
                            ));
                        }
                    } else if starts_with_any(info.prefix, PREFIXES_NON_SEPARABLE)
                        && !contains_any(word, NOT_A_VERB)
                    {
                        if tag.starts_with("VER:IMP:SIN") || tag.starts_with("VER:1:SIN:PRÄ") {
                            if flektion == "SFT" || !has_i_then_more(word) {
                                readings.push(token_from(
                                    word,
                                    start,
                                    end,
                                    &format!("{}{}", info.prefix, lemma),
                                    &format!("VER:IMP:SIN{flektion}"),
                                ));
                                readings.push(token_from(
                                    word,
                                    start,
                                    end,
                                    &format!("{}{}", info.prefix, lemma),
                                    &format!("VER:1:SIN:PRÄ:{flektion}"),
                                ));
                            }
                        } else {
                            readings.push(token_from(
                                word,
                                start,
                                end,
                                &format!("{}{}", info.prefix, lemma),
                                tag,
                            ));
                        }
                    }
                    if tag.contains(":SFT") {
                        is_sft = true;
                    }
                }
            }
            if info.infix == "zu" {
                readings.clear();
                let form = if is_sft { "SFT" } else { "NON" };
                readings.push(token_from(
                    word,
                    start,
                    end,
                    &format!("{}{}", info.prefix, info.verb_baseform),
                    &format!("VER:EIZ:{form}"),
                ));
            }
        } else if let Some(info) = nom_verb_info.filter(|_| add_noun_tags) {
            let lemma = format!("{}{}", info.prefix, info.verb_baseform);
            for tag in [
                "SUB:NOM:SIN:NEU:INF",
                "SUB:AKK:SIN:NEU:INF",
                "SUB:DAT:SIN:NEU:INF",
            ] {
                readings.push(token_from(word, start, end, &lemma, tag));
            }
        } else if let Some(info) = nom_gen_verb_info.filter(|_| add_noun_tags) {
            let lemma = format!("{}{}", info.prefix, info.verb_baseform);
            readings.push(token_from(word, start, end, &lemma, "SUB:GEN:SIN:NEU:INF"));
        } else if let Some(adj) = adj_infos {
            for info in adj {
                readings.push(token_from(word, start, end, info.baseform, info.tag));
            }
        } else if self.is_weise_exception(word) {
            for tag in TAGS_FOR_WEISE {
                readings.push(token_from(word, start, end, word, tag));
            }
        } else if !word.trim().is_empty() && mitarbeitenden_pattern(word) {
            if let Some(idx) = word.find("mitarbeitende") {
                let first_part = &word[..idx];
                let last_part = &word[idx..];
                let tagged = self.word_tag(&uppercase_first(last_part));
                for (_lemma, tag) in &tagged {
                    readings.push(token_from(
                        word,
                        start,
                        end,
                        &format!("{first_part}mitarbeitende"),
                        tag,
                    ));
                }
            }
        } else if !word.trim().is_empty() {
            let compound_parts = self.compound_tokenize(word);
            if compound_parts.len() <= 1 {
                let imperative = self.get_imperative_form(word, start, end, tokens, ctx);
                if !imperative.is_empty() {
                    readings.extend(imperative);
                } else {
                    let substantivated = self.get_substantivated_forms(word, start, end, tokens);
                    if !substantivated.is_empty() {
                        readings.extend(substantivated);
                    } else {
                        self.expand_unknown_simple(word, start, end, tokens, readings);
                    }
                }
            } else if !is_domain(tokens, token_idx) {
                self.expand_compound_last_part(word, start, end, tokens, &compound_parts, readings);
            }
        }
    }

    fn expand_unknown_simple(
        &self,
        word: &str,
        start: usize,
        end: usize,
        tokens: SentTokens,
        readings: &mut Vec<AnalyzedToken>,
    ) {
        if starts_with_any(word, ADJ_PREFIX_GATE)
            && let Some(last_part) = strip_adj_prefix(word)
            && last_part.chars().count() > 3
        {
            let first_part = word.strip_suffix(last_part.as_str()).unwrap_or("");
            let tagged = self.word_tag(&last_part);
            for (lemma, tag) in &tagged {
                if !(first_part.chars().count() == 2 && tag.starts_with("VER")) {
                    readings.push(token_from(
                        word,
                        start,
                        end,
                        &format!("{first_part}{lemma}"),
                        tag,
                    ));
                }
            }
        }
        if space_split_count(word) == 1 && !word.chars().next().is_some_and(|c| c.is_numeric()) {
            let sanitized = self.sanitize_word(word);
            let word_stem = &word[..word.len() - sanitized.len()];
            let compounded = self.compound_tokenize(&sanitized);
            let mut lookup_word = compounded
                .last()
                .cloned()
                .unwrap_or_else(|| sanitized.clone());
            if compounded.len() > 1 {
                lookup_word = uppercase_first(&lookup_word);
            }
            let mut linked = self.add_stem(&self.word_tag(&lookup_word), word_stem);
            if word.contains('-')
                && linked.is_empty()
                && self.matches_uppercase_adjective(&lookup_word)
            {
                let lowered = lowercase_first(&lookup_word);
                linked = self.word_tag(&lowered);
            }
            if linked.is_empty() {
                let word_lower = word.to_lowercase();
                if starts_with_any(&word_lower, PREFIXES_VERBS)
                    && !contains_any(&word_lower, NOT_A_VERB)
                    && (word == title_case(word) || word == word_lower)
                {
                    let last_part = strip_prefix_re(&word_lower, PREFIXES_VERBS);
                    if last_part.chars().count() > 2 {
                        let first_part = word.strip_suffix(last_part.as_str()).unwrap_or("");
                        self.expand_separable_verb(
                            Occurrence { word, start, end },
                            tokens,
                            first_part,
                            &last_part,
                            readings,
                        );
                    }
                } else {
                    readings.push(no_info_token(word, start, end));
                }
            } else if starts_uppercase(word) {
                for (lemma, tag) in &linked {
                    readings.push(token_from(word, start, end, lemma, tag));
                }
            } else {
                for (lemma, tag) in &linked {
                    if tag.starts_with("VER:IMP") {
                        continue;
                    }
                    let mut full_lemma = String::new();
                    for (i, part) in compounded[..compounded.len() - 1].iter().enumerate() {
                        if i == 0 {
                            full_lemma.push_str(part);
                        } else {
                            full_lemma.push_str(&lowercase_first(part));
                        }
                    }
                    full_lemma.push_str(&lowercase_first(lemma));
                    readings.push(token_from(word, start, end, &full_lemma, tag));
                }
            }
        } else {
            readings.push(no_info_token(word, start, end));
        }
    }

    fn expand_separable_verb(
        &self,
        at: Occurrence<'_>,
        tokens: SentTokens,
        first_part: &str,
        last_part: &str,
        readings: &mut Vec<AnalyzedToken>,
    ) {
        let Occurrence { word, start, end } = at;
        if let Some(infinitiv) = last_part.strip_prefix("zu") {
            let infs = self.word_tag(infinitiv);
            for (lemma, tag) in &infs {
                if tag.starts_with("VER:INF") {
                    let pstg = tag.replacen("INF", "EIZ", 1);
                    readings.push(token_from(
                        word,
                        start,
                        end,
                        &format!("{first_part}{lemma}"),
                        &pstg,
                    ));
                }
            }
        }
        let at_start =
            tokens.iter().position(|t| t.0 == word) == Some(0) || word == word.to_lowercase();
        let first_lower = first_part.to_lowercase();
        let tagged_words = self.word_tag(last_part);
        for (lemma, tag) in &tagged_words {
            if tag.starts_with("VER")
                && !tag.starts_with("VER:PA")
                && !tag.starts_with("VER:AUX")
                && !tag.starts_with("VER:MOD")
                && first_part != "un"
            {
                if tag.starts_with("VER:INF") {
                    if word == uppercase_first(word) {
                        readings.push(token_from(word, start, end, word, "SUB:NOM:SIN:NEU:INF"));
                        readings.push(token_from(word, start, end, word, "SUB:DAT:SIN:NEU:INF"));
                        readings.push(token_from(word, start, end, word, "SUB:AKK:SIN:NEU:INF"));
                        if tokens.iter().position(|t| t.0 == word) == Some(0) {
                            readings.push(token_from(
                                word,
                                start,
                                end,
                                &format!("{first_lower}{lemma}"),
                                tag,
                            ));
                        }
                    } else {
                        readings.push(token_from(
                            word,
                            start,
                            end,
                            &format!("{first_lower}{lemma}"),
                            tag,
                        ));
                    }
                } else if tag.starts_with("VER:IMP") {
                    let flekt = &tag[tag.len().saturating_sub(3)..];
                    if at_start && !PREFIXES_SEPARABLE.contains(&first_lower.as_str()) {
                        readings.push(token_from(
                            word,
                            start,
                            end,
                            &format!("{first_lower}{lemma}"),
                            tag,
                        ));
                        if tag.starts_with("VER:IMP:SIN") {
                            readings.push(token_from(
                                word,
                                start,
                                end,
                                &format!("{first_part}{lemma}"),
                                &format!("VER:1:SIN:PRÄ:{flekt}"),
                            ));
                        }
                    } else if at_start && (flekt == "SFT" || !has_i_then_more(word)) {
                        readings.push(token_from(
                            word,
                            start,
                            end,
                            &format!("{first_part}{lemma}"),
                            &format!("VER:1:SIN:PRÄ:{flekt}:NEB"),
                        ));
                    }
                } else if PREFIXES_SEPARABLE.contains(&first_lower.as_str()) && at_start {
                    let new_tag = if tag.ends_with(":NEB") {
                        tag.to_string()
                    } else {
                        format!("{tag}:NEB")
                    };
                    readings.push(token_from(
                        word,
                        start,
                        end,
                        &format!("{first_lower}{lemma}"),
                        &new_tag,
                    ));
                    if (first_part == "durch" || first_part == "um")
                        && tag.starts_with("VER:3:SIN:PRÄ")
                    {
                        if tag.starts_with("VER:3:SIN:PRÄ:SFT") {
                            readings.push(token_from(
                                word,
                                start,
                                end,
                                &format!("{first_part}{lemma}"),
                                "VER:PA2:SFT",
                            ));
                        } else {
                            readings.push(token_from(
                                word,
                                start,
                                end,
                                &format!("{first_part}{lemma}"),
                                "VER:PA2:NON",
                            ));
                        }
                        readings.push(token_from(word, start, end, word, "PA2:PRD:GRU:VER"));
                    }
                } else if PREFIXES_NON_SEPARABLE.contains(&first_lower.as_str()) && at_start {
                    readings.push(token_from(
                        word,
                        start,
                        end,
                        &format!("{first_lower}{lemma}"),
                        tag,
                    ));
                    if tag.starts_with("VER:3:SIN:PRÄ:SFT")
                        || (tag.starts_with("VER:1:PLU:PRÄ:NON")
                            && PARTIZIP2CONTAINS1PLUPRA.iter().any(|v| lemma.contains(v)))
                        || (tag.starts_with("VER:1:PLU:PRT:NON")
                            && PARTIZIP2CONTAINS1PLUPRT.iter().any(|v| lemma.contains(v)))
                    {
                        if first_part != "un" {
                            let fl = &tag[tag.len().saturating_sub(3)..];
                            readings.push(token_from(
                                word,
                                start,
                                end,
                                &format!("{first_part}{lemma}"),
                                &format!("VER:PA2:{fl}"),
                            ));
                        }
                        readings.push(token_from(word, start, end, word, "PA2:PRD:GRU:VER"));
                    }
                }
            } else if (tag.starts_with("PA") || tag.starts_with("VER:PA"))
                && at_start
                && !(first_part == "un" && tag.starts_with("VER:PA"))
            {
                readings.push(token_from(
                    word,
                    start,
                    end,
                    &format!("{first_lower}{lemma}"),
                    tag,
                ));
            }
        }
        let partizip_suffixes: [(&str, &[&str]); 5] = [
            ("e", POSTAGSPARTIZIPENDINGE),
            ("em", POSTAGSPARTIZIPENDINGEM),
            ("en", POSTAGSPARTIZIPENDINGEN),
            ("er", POSTAGSPARTIZIPENDINGER),
            ("es", POSTAGSPARTIZIPENDINGES),
        ];
        for (suffix, tags) in partizip_suffixes {
            let Some(stripped) = last_part.strip_suffix(suffix) else {
                continue;
            };
            let tagged_middle = self.word_tag(stripped);
            for (lemma, tag) in &tagged_middle {
                if tag.starts_with("VER:3:SIN:PRÄ:SFT") && at_start && !lemma.is_empty() {
                    let stem = &word[..word.len() - suffix.len()];
                    for pos_tag in tags {
                        readings.push(token_from(
                            word,
                            start,
                            end,
                            stem,
                            &format!("PA2:{pos_tag}"),
                        ));
                    }
                }
            }
        }
    }

    fn expand_compound_last_part(
        &self,
        word: &str,
        start: usize,
        end: usize,
        tokens: SentTokens,
        compound_parts: &[String],
        readings: &mut Vec<AnalyzedToken>,
    ) {
        let Some(last) = compound_parts.last() else {
            readings.push(no_info_token(word, start, end));
            return;
        };
        let mut last_part = last.clone();
        if starts_uppercase(word)
            && !["freie", "freier", "freien", "freies", "freiem"]
                .iter()
                .any(|f| last_part.contains(f))
        {
            last_part = uppercase_first(&last_part);
        }
        let part_tagger_tokens = self.word_tag(&last_part);
        if part_tagger_tokens.is_empty() {
            readings.push(no_info_token(word, start, end));
            return;
        }
        let first_part = compound_parts[0].as_str();
        if COMPOUND_VERB_PREFIXES.contains(&first_part) {
            let at_start =
                tokens.iter().position(|t| t.0 == word) == Some(0) || word == lowercase_first(word);
            for (lemma, tag) in &part_tagger_tokens {
                if (tag.starts_with("VER:1")
                    || tag.starts_with("VER:2")
                    || tag.starts_with("VER:3"))
                    && at_start
                {
                    let new_tag = if tag.ends_with("NEB") {
                        tag.to_string()
                    } else {
                        format!("{tag}:NEB")
                    };
                    readings.push(token_from(
                        word,
                        start,
                        end,
                        &format!("{first_part}{lemma}"),
                        &new_tag,
                    ));
                } else if !tag.starts_with("VER:IMP") {
                    readings.push(token_from(
                        word,
                        start,
                        end,
                        &format!("{first_part}{lemma}"),
                        tag,
                    ));
                }
            }
        } else {
            let mut lemma = String::new();
            for (i, part) in compound_parts[..compound_parts.len() - 1]
                .iter()
                .enumerate()
            {
                if i == 0 {
                    lemma.push_str(part);
                } else {
                    lemma.push_str(&lowercase_first(part));
                }
            }
            for (l, tag) in &part_tagger_tokens {
                if tag.contains("VER") {
                    continue;
                }
                let full_lemma = format!("{}{}", lemma, lowercase_first(l));
                readings.push(token_from(word, start, end, &full_lemma, tag));
            }
        }
    }

    fn get_imperative_form(
        &self,
        word: &str,
        start: usize,
        end: usize,
        tokens: SentTokens,
        ctx: &TokenCtx,
    ) -> Vec<AnalyzedToken> {
        let idx = tokens.iter().position(|t| t.0 == word);
        let mut previous_word = "";
        if let Some(mut i) = idx {
            while i > 0 {
                i -= 1;
                previous_word = tokens[i].0;
                if !previous_word.chars().all(|c| c.is_whitespace()) {
                    break;
                }
            }
        }
        let prev_ok = [
            "ich", "er", "es", "sie", "bitte", "aber", "nun", "jetzt", "„",
        ]
        .iter()
        .any(|p| p.eq_ignore_ascii_case(previous_word));
        if !(prev_ok || ctx.pos == 0 && tokens.len() > 1) {
            return Vec::new();
        }
        let w = if ctx.pos == 0 || previous_word == "„" {
            word.to_lowercase()
        } else {
            word.to_string()
        };
        let tagged_with_e = self.word_tag(&format!("{w}e"));
        for (lemma, tag) in &tagged_with_e {
            if tag.starts_with("VER:IMP:SIN") {
                let removed = self
                    .removals
                    .get(w.as_str())
                    .is_some_and(|r| r.iter().any(|(rl, rt)| rl == lemma && rt == tag));
                if !removed {
                    return vec![token_from(word, start, end, lemma, tag)];
                }
                break;
            }
        }
        Vec::new()
    }

    fn get_substantivated_forms(
        &self,
        word: &str,
        start: usize,
        end: usize,
        tokens: SentTokens,
    ) -> Vec<AnalyzedToken> {
        if !word.ends_with("er") {
            return Vec::new();
        }
        let stem = &word[..word.len() - 2];
        if stem.chars().count() >= 4 && stem.chars().all(|c| c.is_ascii_digit()) {
            return all_adj_gru_tags()
                .iter()
                .map(|t| token_from(word, start, end, word, t))
                .collect();
        }
        let lower_tags = self.word_tag(&word.to_lowercase());
        if lower_tags.iter().any(|(_, t)| t.starts_with("ADV")) {
            return Vec::new();
        }
        let idx = tokens.iter().position(|t| t.0 == word);
        if let Some(mut i) = idx {
            i += 1;
            while i < tokens.len() {
                let next_word = tokens[i].0;
                if next_word.chars().all(|c| c.is_whitespace()) {
                    i += 1;
                    continue;
                }
                if next_word.chars().next().is_some_and(|c| c.is_uppercase()) || next_word == "als"
                {
                    return Vec::new();
                }
                break;
            }
        }
        let female_form = &word[..word.len() - 1];
        let tagged_female = self.word_tag(female_form);
        if tagged_female
            .iter()
            .any(|(_, t)| *t == "SUB:NOM:SIN:FEM:ADJ")
        {
            return vec![
                token_from(word, start, end, word, "SUB:NOM:SIN:MAS:ADJ"),
                token_from(word, start, end, word, "SUB:GEN:PLU:MAS:ADJ"),
            ];
        }
        Vec::new()
    }

    fn is_weise_exception(&self, word: &str) -> bool {
        word.strip_suffix("erweise").is_some_and(|stem| {
            self.word_tag(stem)
                .iter()
                .any(|(_, t)| t.starts_with("ADJ"))
        })
    }

    fn matches_uppercase_adjective(&self, word: &str) -> bool {
        let lowered = lowercase_first(word);
        self.word_tag(&lowered)
            .first()
            .is_some_and(|(_, t)| t.starts_with("ADJ"))
    }

    fn sanitize_word(&self, word: &str) -> String {
        if word.ends_with('-') {
            return word.to_string();
        }
        let split: Vec<&str> = word.split('-').collect();
        let base = if split.len() > 1 && !split[split.len() - 1].trim().is_empty() {
            split[split.len() - 1]
        } else {
            word
        };
        let compounded = self.compound_tokenize(base);
        let last_part = if compounded.len() > 1 && starts_uppercase(word) {
            uppercase_last(&compounded)
        } else {
            compounded
                .last()
                .cloned()
                .unwrap_or_else(|| base.to_string())
        };
        let tagged = self.word_tag(&last_part);
        if !tagged.is_empty()
            && (tagged[0].1.starts_with("SUB")
                || tagged[0].1.starts_with("ADJ")
                || self.matches_uppercase_adjective(&last_part))
        {
            last_part
        } else {
            word.to_string()
        }
    }

    fn add_stem(&self, tagged: &Readings, stem: &str) -> Readings {
        if stem.is_empty() {
            return tagged.to_vec();
        }
        tagged
            .iter()
            .map(|(lemma, tag)| {
                let new_lemma = if !stem.ends_with('-') && tag.starts_with("SUB") {
                    lemma.to_lowercase()
                } else {
                    lemma.to_string()
                };
                (
                    Box::leak(format!("{stem}{new_lemma}").into_boxed_str()) as &'static str,
                    *tag,
                )
            })
            .collect()
    }

    fn compound_tokenize(&self, word: &str) -> Vec<String> {
        if word.is_empty() {
            return Vec::new();
        }
        crate::jwordsplitter::tokenize(word)
    }

    fn analyze(&self, tokens: &[(&str, usize, usize)]) -> Vec<Vec<AnalyzedToken>> {
        let mut ctx = TokenCtx::new();
        let mut out = Vec::with_capacity(tokens.len());
        for (idx, (word, start, end)) in tokens.iter().enumerate() {
            let readings = self.analyze_token(word, *start, *end, tokens, idx, &mut ctx);
            ctx.pos += word.chars().count();
            ctx.prev_word = Some(word.to_string());
            out.push(readings);
        }
        out
    }
}

fn append_manual_lines(map: &mut ReadingMap, content: &str) {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let (Some(form), Some(lemma), Some(tag)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let form_leaked: &'static str = Box::leak(form.to_string().into_boxed_str());
        let lemma_leaked: &'static str = Box::leak(lemma.to_string().into_boxed_str());
        let tag_leaked: &'static str = Box::leak(tag.trim().to_string().into_boxed_str());
        map.entry(form_leaked.into())
            .or_default()
            .push((lemma_leaked, tag_leaked));
    }
}

fn token_from(surface: &str, start: usize, end: usize, lemma: &str, tag: &str) -> AnalyzedToken {
    AnalyzedToken::new(surface, start, end)
        .with_lemma(lemma.to_string())
        .with_pos_tags(vec![tag.to_string()])
}

fn no_info_token(surface: &str, start: usize, end: usize) -> AnalyzedToken {
    AnalyzedToken::new(surface, start, end)
}

fn readings_contain_tag(readings: &[AnalyzedToken], needle: &str) -> bool {
    readings
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.starts_with(needle)))
}

fn matches_after_asterisk(s: &str) -> bool {
    matches!(s, "in" | "innen" | "r" | "e")
}

fn innen_pattern1(s: &str) -> bool {
    let rest = s
        .strip_prefix("innen")
        .or_else(|| (s.len() >= 2 && (s.starts_with("in"))).then(|| &s[2..]));
    let Some(rest) = rest else {
        return false;
    };
    let Some(after_dash) = rest.strip_prefix('-') else {
        return false;
    };
    let mut chars = after_dash.chars();
    if !chars.next().is_some_and(|c| c.is_uppercase()) {
        return false;
    }
    after_dash.chars().all(|c| c.is_alphabetic() || c == '-')
}

fn innen_pattern2(s: &str) -> bool {
    let Some(idx) = s.find("innen") else {
        return false;
    };
    let after = &s[idx + 5..];
    !after.is_empty() && after.chars().all(|c| c.is_lowercase() || c == '-')
}

fn mitarbeitenden_pattern(word: &str) -> bool {
    let stem = if let Some(s) = word.strip_suffix("mitarbeitende") {
        s
    } else if let Some(s) = word.strip_suffix("mitarbeitenden") {
        s
    } else {
        return false;
    };
    let stem_chars: Vec<char> = stem.chars().collect();
    if stem_chars.len() < 2 || stem_chars.len() > 25 {
        return false;
    }
    stem_chars.first().is_some_and(|c| c.is_uppercase())
        && stem_chars[1..]
            .iter()
            .all(|c| c.is_lowercase() || *c == 'ß' || *c == 'ä' || *c == 'ö' || *c == 'ü')
}

fn is_domain(tokens: SentTokens, token_idx: usize) -> bool {
    token_idx + 2 < tokens.len()
        && tokens[token_idx + 1].0 == "."
        && ["com", "net", "org", "de", "at", "ch", "fr", "uk", "gov"]
            .contains(&tokens[token_idx + 2].0)
}

fn starts_with_any(word: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| word.starts_with(p))
}

fn contains_any(word: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| word.contains(n))
}

fn strip_prefix_re(word: &str, prefixes: &[&str]) -> String {
    for p in prefixes {
        if let Some(stripped) = word.strip_prefix(p) {
            return stripped.to_string();
        }
    }
    word.to_string()
}

fn strip_adj_prefix(word: &str) -> Option<String> {
    for p in ADJ_PREFIX_STRIP {
        let key = if *p == "u[nr]" { "un" } else { p };
        if let Some(stripped) = word.strip_prefix(key) {
            return Some(stripped.to_string());
        }
    }
    word.strip_prefix("ur").map(str::to_string)
}

fn has_i_then_more(word: &str) -> bool {
    let chars: Vec<char> = word.chars().collect();
    (0..chars.len().saturating_sub(1)).any(|i| chars[i] == 'i')
}

fn title_case(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn uppercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn uppercase_last(parts: &[String]) -> String {
    parts.last().map(|p| uppercase_first(p)).unwrap_or_default()
}

fn lowercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn starts_uppercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_uppercase())
}

fn starts_lowercase(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_lowercase())
}

fn is_alphanumeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric())
}

fn space_split_count(word: &str) -> usize {
    word.split(' ').filter(|p| !p.is_empty()).count()
}

fn all_adj_gru_tags() -> &'static [&'static str] {
    static TAGS: OnceLock<Vec<&'static str>> = OnceLock::new();
    TAGS.get_or_init(|| {
        let mut out = Vec::new();
        for case in ["NOM", "AKK", "GEN", "DAT"] {
            for num in ["PLU", "SIN"] {
                for gender in ["MAS", "FEM", "NEU"] {
                    for det in ["DEF", "IND", "SOL"] {
                        out.push(Box::leak(
                            format!("ADJ:{case}:{num}:{gender}:GRU:{det}").into_boxed_str(),
                        ) as &'static str);
                    }
                }
            }
        }
        out
    })
}

impl Tagger for GermanTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let mut with_offsets: Vec<(&str, usize, usize)> = Vec::with_capacity(tokens.len());
        let mut byte_offset = 0usize;
        for t in tokens {
            with_offsets.push((t, byte_offset, byte_offset + t.len()));
            byte_offset += t.len();
        }
        let results = self.analyze(&with_offsets);
        results
            .into_iter()
            .zip(with_offsets)
            .map(|(readings, (text, start, end))| {
                let all_tags: Vec<String> = readings
                    .iter()
                    .flat_map(|r| r.pos_tags().to_vec())
                    .collect();
                let lemma = readings
                    .first()
                    .and_then(|r| r.lemma().map(str::to_string))
                    .unwrap_or_else(|| text.to_string());
                let primary = AnalyzedToken::new(text, start, end)
                    .with_pos_tags(all_tags)
                    .with_lemma(lemma);
                AnalyzedTokenReadings::new(primary).with_readings(readings)
            })
            .collect()
    }
}

impl og_core::checker::Tagger for GermanTagger {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]) {
        let with_offsets: Vec<(&str, usize, usize)> = tokens
            .iter()
            .skip(1)
            .map(|atr| {
                let t = atr.token();
                (t.token(), t.start(), t.end())
            })
            .collect();
        let results = self.analyze(&with_offsets);
        for (i, readings) in results.into_iter().enumerate() {
            let token_idx = i + 1;
            if token_idx >= tokens.len() {
                break;
            }
            let primary = tokens[token_idx].token().clone();
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let lemma = readings
                .first()
                .and_then(|r| r.lemma().map(str::to_string))
                .unwrap_or_else(|| primary.token().to_string());
            let updated = AnalyzedToken::new(primary.token(), primary.start(), primary.end())
                .with_pos_tags(all_tags)
                .with_lemma(lemma);
            tokens[token_idx] = AnalyzedTokenReadings::new(updated).with_readings(readings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource_dir() -> Option<std::path::PathBuf> {
        [
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
            "languagetool/languagetool-language-modules/de/src/main/resources/org/languagetool/resource/de",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join("german.dict").exists())
    }

    fn tagger() -> GermanTagger {
        let dir = resource_dir().expect("german resources");
        GermanTagger::from_resource_dir(&dir).expect("tagger loads")
    }

    #[test]
    fn tags_german_nouns_with_case_readings() {
        let tagger = tagger();
        let result = Tagger::tag(&tagger, &["Haus"]);
        assert!(
            result[0].has_pos_tag("SUB:NOM:SIN:NEU"),
            "{:?}",
            result[0].all_pos_tags()
        );
        assert!(result[0].has_pos_tag("SUB:AKK:SIN:NEU"));
    }

    #[test]
    fn in_place_tagging_fills_readings_and_preserves_offsets() {
        let tagger = tagger();
        let words = [("Der", 0usize, 3usize), ("Hund", 4, 8), ("schläft", 9, 16)];
        let start = AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        );
        let mut tokens = vec![start];
        tokens.extend(words.iter().map(|(text, start, end)| {
            AnalyzedTokenReadings::new(AnalyzedToken::new(*text, *start, *end))
        }));
        og_core::checker::Tagger::tag(&tagger, &mut tokens);
        assert_eq!(tokens[0].token().token(), "<S>");
        assert!(
            tokens[0].token().pos_tags() == ["SENT_START"],
            "{:?}",
            tokens[0].token().pos_tags()
        );
        for (i, (text, start, end)) in words.iter().enumerate() {
            let atr = &tokens[i + 1];
            assert_eq!(atr.token().token(), *text);
            assert_eq!(atr.token().start(), *start);
            assert_eq!(atr.token().end(), *end);
        }
        assert!(
            tokens[1].has_pos_tag("ART:DEF:NOM:SIN:MAS"),
            "{:?}",
            tokens[1].all_pos_tags()
        );
        assert!(
            tokens[2].has_pos_tag("SUB:NOM:SIN:MAS"),
            "{:?}",
            tokens[2].all_pos_tags()
        );
        let by_value = Tagger::tag(&tagger, &["Der", "Hund", "schläft"]);
        for (in_place, by_value) in tokens[1..].iter().zip(by_value.iter()) {
            assert_eq!(in_place.all_pos_tags(), by_value.all_pos_tags());
        }
    }

    #[test]
    fn capitalized_sentence_start_falls_back_to_lowercase() {
        let tagger = tagger();
        let result = Tagger::tag(&tagger, &["Der", "Hund", "schläft"]);
        assert!(
            result[0].has_pos_tag("ART:DEF:NOM:SIN:MAS"),
            "{:?}",
            result[0].all_pos_tags()
        );
        assert!(
            result[1].has_pos_tag("SUB:NOM:SIN:MAS"),
            "{:?}",
            result[1].all_pos_tags()
        );
        assert!(
            result[2].has_pos_tag("VER:3:SIN:PRÄ:NON"),
            "{:?}",
            result[2].all_pos_tags()
        );
    }

    #[test]
    fn mid_sentence_uppercase_does_not_get_lowercase_readings() {
        let tagger = tagger();
        let result = Tagger::tag(&tagger, &["Er", "spielte", "Lach"]);
        let lach = result[2].readings();
        assert!(
            lach.iter().all(|r| r.pos_tags().is_empty()),
            "Lach should stay untagged mid-sentence: {lach:?}"
        );
    }

    #[test]
    fn alternative_imperative_forms_match_java() {
        let tagger = tagger();
        let result = Tagger::tag(&tagger, &["Geh", "bitte", "!"]);
        assert!(
            result[0]
                .all_pos_tags()
                .iter()
                .any(|t| t.starts_with("VER:IMP:SIN")),
            "{:?}",
            result[0].all_pos_tags()
        );
        let result = Tagger::tag(&tagger, &["Er", "geh", "jetzt"]);
        assert!(
            result[1]
                .all_pos_tags()
                .iter()
                .any(|t| t.starts_with("VER:IMP:SIN")),
            "{:?}",
            result[1].all_pos_tags()
        );
        let result = Tagger::tag(&tagger, &["zehnfach"]);
        assert!(
            result[0].readings().iter().all(|r| r.pos_tags().is_empty()),
            "a single imperative-looking token must stay untagged: {:?}",
            result[0].all_pos_tags()
        );
    }

    #[test]
    fn ddd_er_numeral_gets_all_adj_gru_tags() {
        let tagger = tagger();
        let result = Tagger::tag(&tagger, &["Den", "2019er", "Wert", "hatten", "sie"]);
        let tags = result[1].all_pos_tags();
        assert!(
            tags.iter().any(|t| *t == "ADJ:AKK:SIN:NEU:GRU:IND"),
            "{tags:?}"
        );
        assert!(
            tags.iter().any(|t| *t == "ADJ:NOM:SIN:MAS:GRU:SOL"),
            "{tags:?}"
        );
        assert_eq!(tags.len(), 72, "{tags:?}");
    }

    #[test]
    fn innen_gap_compound_variants_take_the_tail_reading() {
        let tagger = tagger();
        let result = Tagger::tag(&tagger, &["Werkstudent", ":", "innen-Zielgruppe"]);
        assert!(
            result[0]
                .readings()
                .iter()
                .any(|r| r.lemma() == Some("Zielgruppe")),
            "{:?}",
            result[0].readings()
        );
        let result = Tagger::tag(&tagger, &["Werkstudent", ":", "innenzielgruppe"]);
        assert!(
            result[0]
                .readings()
                .iter()
                .any(|r| r.lemma() == Some("Zielgruppe")),
            "{:?}",
            result[0].readings()
        );
    }

    #[test]
    fn substantivated_adjective_gets_sub_reading() {
        let tagger = tagger();
        let result = Tagger::tag(
            &tagger,
            &["Der", "Verrückter", "ist", "wieder", "unterwegs"],
        );
        assert!(
            result[1].has_pos_tag("SUB:NOM:SIN:MAS:ADJ"),
            "{:?}",
            result[1].all_pos_tags()
        );
        let result = Tagger::tag(
            &tagger,
            &[
                "Er",
                "wollte",
                "dem",
                "Mann",
                "helfen",
                ",",
                "doch",
                "der",
                "Verrückter",
                "schüttelte",
            ],
        );
        assert!(
            result[8].has_pos_tag("SUB:NOM:SIN:MAS:ADJ"),
            "{:?}",
            result[8].all_pos_tags()
        );
        let result = Tagger::tag(
            &tagger,
            &[
                "Die",
                "Sitzung",
                ",",
                "der",
                "Abgeordneter",
                "Müller",
                "fernblieb",
            ],
        );
        assert!(
            !result[4].has_pos_tag("SUB:NOM:SIN:MAS:ADJ"),
            "uppercase follower must suppress substantivation: {:?}",
            result[4].all_pos_tags()
        );
    }

    #[test]
    fn unknown_compounds_get_last_part_pos() {
        let tagger = tagger();
        let result = Tagger::tag(&tagger, &["Grundprinzipien"]);
        assert!(
            result[0]
                .readings()
                .iter()
                .any(|r| r.pos_tags().iter().any(|t| t.starts_with("SUB:DAT:PLU"))),
            "{:?}",
            result[0].readings()
        );
        let result = Tagger::tag(&tagger, &["Telefontarife"]);
        assert!(
            result[0]
                .readings()
                .iter()
                .any(|r| r.pos_tags().iter().any(|t| t.starts_with("SUB:"))),
            "{:?}",
            result[0].readings()
        );
    }

    fn has_partial_tag(atr: &og_core::AnalyzedTokenReadings, partial: &str) -> bool {
        atr.readings()
            .iter()
            .any(|r| r.pos_tags().iter().any(|t| t.contains(partial)))
    }

    fn tags_of(atr: &og_core::AnalyzedTokenReadings) -> Vec<(String, String)> {
        atr.readings()
            .iter()
            .flat_map(|r| {
                let lemma = r.lemma().unwrap_or_default().to_string();
                r.pos_tags().iter().map(move |t| (lemma.clone(), t.clone()))
            })
            .collect()
    }

    #[test]
    fn adjectives_from_spelling_txt_expand_pa2_readings() {
        let tagger = tagger();
        let readings = tags_of(
            &Tagger::tag(&tagger, &["abgemindert"])
                .into_iter()
                .next()
                .unwrap(),
        );
        assert_eq!(
            readings,
            vec![("abgemindert".to_string(), "PA2:PRD:GRU:VER".to_string())]
        );

        let es = tags_of(
            &Tagger::tag(&tagger, &["abgemindertes"])
                .into_iter()
                .next()
                .unwrap(),
        );
        assert_eq!(es.len(), 4);
        assert!(es.iter().all(|(lemma, _)| lemma == "abgemindert"));
        assert!(
            es.iter()
                .all(|(_, tag)| tag.starts_with("PA2:") && tag.contains(":SIN:NEU:GRU:"))
        );

        assert!(tagger.lookup("fünftjüngste").is_none());

        let meist = tags_of(
            &Tagger::tag(&tagger, &["meistgewünschtes"])
                .into_iter()
                .next()
                .unwrap(),
        );
        assert_eq!(meist.len(), 4);
        assert!(meist.iter().all(|(l, _)| l == "meistgewünscht"));
        assert!(
            meist
                .iter()
                .all(|(_, t)| t.starts_with("ADJ:") && t.contains(":SIN:NEU:GRU:"))
        );

        let ter = tags_of(
            &Tagger::tag(&tagger, &["meistgewünschter"])
                .into_iter()
                .next()
                .unwrap(),
        );
        assert_eq!(ter.len(), 7);

        let plain = tags_of(
            &Tagger::tag(&tagger, &["meistgewünscht"])
                .into_iter()
                .next()
                .unwrap(),
        );
        assert_eq!(
            plain,
            vec![("meistgewünscht".to_string(), "ADJ:PRD:GRU".to_string())]
        );
    }

    #[test]
    fn dash_compound_lemma_is_joined() {
        let tagger = tagger();
        let readings = tagger.lookup("Zahn-Arzt-Verband").expect("readings");
        assert!(
            readings
                .iter()
                .any(|r| r.lemma() == Some("Zahnarztverband"))
        );
    }

    #[test]
    fn gender_gap_tokens_carry_feminine_plurals() {
        let tagger = tagger();
        for joiner in ["*", "_", ":", "/"] {
            let result = Tagger::tag(&tagger, &["viele", "Freund", joiner, "innen"]);
            assert!(
                has_partial_tag(&result[1], ":PLU:FEM"),
                "joiner {joiner}: {:?}",
                tags_of(&result[1])
            );
        }
        let result = Tagger::tag(&tagger, &["jede", "*", "r", "Mitarbeiter", "*", "in"]);
        assert!(has_partial_tag(&result[0], "PRO:IND:NOM:SIN:FEM"));
        assert!(has_partial_tag(&result[0], "PRO:IND:NOM:SIN:MAS"));
        assert!(has_partial_tag(&result[3], "SUB:NOM:SIN:FEM"));
        assert!(has_partial_tag(&result[3], "SUB:NOM:SIN:MAS"));
    }

    #[test]
    fn domains_and_zehnfach_stay_untagged() {
        let tagger = tagger();
        let result = Tagger::tag(&tagger, &["bundestag", ".", "de"]);
        assert!(
            result[0].readings().iter().all(|r| r.pos_tags().is_empty()),
            "{:?}",
            tags_of(&result[0])
        );
        let result = Tagger::tag(&tagger, &["zehnfach"]);
        assert!(
            result[0].readings().iter().all(|r| r.pos_tags().is_empty()),
            "{:?}",
            tags_of(&result[0])
        );
    }

    #[test]
    fn lookup_answers_java_dictionary_cases() {
        let tagger = tagger();
        assert!(tagger.lookup("hauses").is_none());
        assert!(tagger.lookup("Groß").is_none());
        assert!(tagger.lookup("under").is_none());
        assert!(tagger.lookup("vanillig-karamelligen").is_none());

        let hauses = tagger.lookup("Hauses").expect("Hauses");
        assert_eq!(hauses.len(), 1);
        assert_eq!(hauses[0].lemma(), Some("Haus"));
        assert_eq!(hauses[0].pos_tags(), &["SUB:GEN:SIN:NEU".to_string()]);

        let zuge = tagger.lookup("Zuge").expect("Zuge");
        assert_eq!(zuge.len(), 1);
        assert!(zuge[0].has_pos_tag("SUB:DAT:SIN:MAS"));

        let tische = tagger.lookup("Tische").expect("Tische");
        assert_eq!(tische.len(), 4);
        assert!(tische.iter().all(|r| r.lemma() == Some("Tisch")));

        let verletzter = tagger.lookup("Verletzter").expect("Verletzter");
        assert_eq!(
            verletzter[0].pos_tags(),
            &["SUB:NOM:SIN:MAS:ADJ".to_string()]
        );
        assert_eq!(
            verletzter[1].pos_tags(),
            &["SUB:GEN:PLU:MAS:ADJ".to_string()]
        );

        let erzkatholisch = tagger.lookup("erzkatholisch").expect("erzkatholisch");
        assert_eq!(erzkatholisch[0].pos_tags(), &["ADJ:PRD:GRU".to_string()]);

        let unerbeten = tagger.lookup("unerbeten").expect("unerbeten");
        assert_eq!(unerbeten[0].pos_tags(), &["PA2:PRD:GRU:VER".to_string()]);

        let delisted = tagger.lookup("Donaudampfschiff").expect("Donaudampfschiff");
        assert_eq!(delisted.len(), 3);
        assert!(
            delisted
                .iter()
                .all(|r| r.lemma() == Some("Donaudampfschiff"))
        );

        let uebrig = tagger.lookup("übrigbleibst").expect("übrigbleibst");
        assert_eq!(uebrig.len(), 1);
        assert_eq!(uebrig[0].lemma(), Some("übrigbleiben"));
        assert_eq!(uebrig[0].pos_tags(), &["VER:2:SIN:PRÄ:NON:NEB".to_string()]);

        let it = tagger
            .lookup("IT-Dienstleistungsunternehmen")
            .expect("IT compound");
        assert!(it[0].pos_tags()[0].starts_with("SUB"));

        let entweder = tagger.lookup("Entweder-oder").expect("Entweder-oder");
        assert!(entweder[0].pos_tags()[0].starts_with("SUB"));

        let polizei = tagger
            .lookup("Polizeimitarbeitende")
            .expect("Polizeimitarbeitende");
        assert_eq!(polizei.len(), 5);
        let polizei_den = tagger
            .lookup("Polizeimitarbeitenden")
            .expect("Polizeimitarbeitenden");
        assert_eq!(polizei_den.len(), 19);
        let mut den_tags: Vec<&str> = polizei_den
            .iter()
            .flat_map(|r| r.pos_tags().iter().map(|t| t.as_str()))
            .collect();
        den_tags.sort_unstable();
        den_tags.dedup();
        assert_eq!(den_tags.len(), 19);
        assert!(
            den_tags
                .iter()
                .all(|t| t.starts_with("SUB:") && t.ends_with(":ADJ"))
        );
        assert!(den_tags.contains(&"SUB:AKK:SIN:MAS:ADJ"));
        assert!(den_tags.contains(&"SUB:DAT:SIN:NEU:ADJ"));
        assert!(den_tags.contains(&"SUB:NOM:PLU:NEU:ADJ"));
    }

    #[test]
    fn pre_reform_spellings_and_manually_added_words_stay() {
        let tagger = tagger();
        for word in ["Kuß", "Kuss", "Haß", "Hass"] {
            let readings = tagger.lookup(word).expect(word);
            assert_eq!(readings.len(), 3, "{word}");
            assert!(readings.iter().all(|r| r.lemma() == Some(word)));
        }
        let wichtigtuerinnen = Tagger::tag(&tagger, &["Wichtigtuerinnen"]);
        let tags = tags_of(&wichtigtuerinnen[0]);
        assert_eq!(tags.len(), 4);
        assert!(tags.iter().all(|(l, t)| l == "Wichtigtuerin"
            && t.starts_with("SUB:")
            && t.ends_with(":PLU:FEM")));
    }

    #[test]
    fn colon_triggers_lowercase_fallback_for_the_next_word() {
        let tagger = tagger();
        let result = Tagger::tag(
            &tagger,
            &["Er", "sagte", ":", "Als", "Erstes", "würde", "ich"],
        );
        assert_eq!(result.len(), 7);
        assert_eq!(result[3].token().token(), "Als");
        assert_eq!(result[3].readings().len(), 4, "{:?}", tags_of(&result[3]));
    }

    #[test]
    fn weise_exception_matches_java() {
        let tagger = tagger();
        for word in [
            "überweise",
            "verweise",
            "eimerweise",
            "meterweise",
            "literweise",
            "blätterweise",
            "erweise",
        ] {
            assert!(!tagger.is_weise_exception(word), "{word}");
        }
        assert!(tagger.is_weise_exception("lustigerweise"));
        assert!(tagger.is_weise_exception("idealerweise"));
    }

    #[test]
    fn prefix_verbs_from_spelling_txt_match_java_counts() {
        let tagger = tagger();
        let cases: &[(&str, usize, &[(&str, &str)])] = &[
            (
                "herausfallen",
                5,
                &[
                    ("herausfallen", "VER:1:PLU:KJ1:NON:NEB"),
                    ("herausfallen", "VER:1:PLU:PRÄ:NON:NEB"),
                    ("herausfallen", "VER:3:PLU:KJ1:NON:NEB"),
                    ("herausfallen", "VER:3:PLU:PRÄ:NON:NEB"),
                    ("herausfallen", "VER:INF:NON"),
                ],
            ),
            (
                "herumgeben",
                5,
                &[
                    ("herumgeben", "VER:1:PLU:KJ1:NON:NEB"),
                    ("herumgeben", "VER:1:PLU:PRÄ:NON:NEB"),
                    ("herumgeben", "VER:3:PLU:KJ1:NON:NEB"),
                    ("herumgeben", "VER:3:PLU:PRÄ:NON:NEB"),
                    ("herumgeben", "VER:INF:NON"),
                ],
            ),
            (
                "herumgab",
                2,
                &[
                    ("herumgeben", "VER:1:SIN:PRT:NON:NEB"),
                    ("herumgeben", "VER:3:SIN:PRT:NON:NEB"),
                ],
            ),
            (
                "zurückgeschickt",
                2,
                &[
                    ("zurückschicken", "VER:PA2:SFT"),
                    ("zurückgeschickt", "PA2:PRD:GRU:VER"),
                ],
            ),
            ("abzuschicken", 1, &[("abschicken", "VER:EIZ:SFT")]),
            ("reinzunehmen", 1, &[("reinnehmen", "VER:EIZ:NON")]),
            (
                "Mitmanagen",
                3,
                &[
                    ("Mitmanagen", "SUB:NOM:SIN:NEU:INF"),
                    ("Mitmanagen", "SUB:AKK:SIN:NEU:INF"),
                    ("Mitmanagen", "SUB:DAT:SIN:NEU:INF"),
                ],
            ),
            ("Mitmanagens", 1, &[("Mitmanagen", "SUB:GEN:SIN:NEU:INF")]),
        ];
        for (word, count, required) in cases {
            let result = Tagger::tag(&tagger, &[word]);
            let tags = tags_of(&result[0]);
            assert_eq!(tags.len(), *count, "{word}: {tags:?}");
            for (lemma, tag) in *required {
                assert!(
                    tags.iter().any(|(l, t)| l == lemma && t == tag),
                    "{word} misses {lemma}/{tag}: {tags:?}"
                );
            }
            assert!(
                tags.iter().all(|(_, t)| !t.starts_with("ADJ:")),
                "{word} must not gain ADJ readings: {tags:?}"
            );
        }

        let wegstrecken = Tagger::tag(&tagger, &["Wegstrecken"]);
        let tags = tags_of(&wegstrecken[0]);
        assert_eq!(tags.len(), 7, "{tags:?}");
        assert!(!tags.iter().any(|(_, t)| t == "SUB:GEN:SIN:NEU:INF"));
    }

    #[test]
    fn separable_prefix_verbs_match_java_counts() {
        let tagger = tagger();
        let nachguckst = Tagger::tag(&tagger, &["nachguckst"]);
        let tags = tags_of(&nachguckst[0]);
        assert_eq!(tags.len(), 1, "{tags:?}");
        assert_eq!(
            tags[0],
            (
                "nachgucken".to_string(),
                "VER:2:SIN:PRÄ:SFT:NEB".to_string()
            )
        );

        let nachgucke = Tagger::tag(&tagger, &["nachgucke"]);
        let tags = tags_of(&nachgucke[0]);
        assert_eq!(tags.len(), 4, "{tags:?}");
        assert!(tags.iter().any(|(l, t)| l == "nachgucken"
            && (t == "VER:1:SIN:PRÄ:SFT:NEB"
                || t == "VER:1:SIN:KJ1:SFT:NEB"
                || t == "VER:3:SIN:KJ1:SFT:NEB")));

        let nachzugucken = Tagger::tag(&tagger, &["nachzugucken"]);
        let tags = tags_of(&nachzugucken[0]);
        assert_eq!(tags.len(), 1, "{tags:?}");
        assert_eq!(
            tags[0],
            ("nachgucken".to_string(), "VER:EIZ:SFT".to_string())
        );

        let nachguckend = Tagger::tag(&tagger, &["nachguckend"]);
        let tags = tags_of(&nachguckend[0]);
        assert_eq!(tags.len(), 2, "{tags:?}");
        assert!(
            tags.iter()
                .any(|(l, t)| l == "nachgucken" && t == "VER:PA1:SFT")
        );
        assert!(
            tags.iter()
                .any(|(l, t)| l == "nachguckend" && t == "PA1:PRD:GRU:VER")
        );
    }

    #[test]
    fn non_separable_prefix_verbs_match_java_counts() {
        let tagger = tagger();

        let vergaeren = Tagger::tag(&tagger, &["vergären"]);
        let tags = tags_of(&vergaeren[0]);
        assert_eq!(tags.len(), 15, "{tags:?}");
        for expected in [
            "VER:1:PLU:PRÄ:NON",
            "VER:1:PLU:KJ1:NON",
            "VER:3:PLU:PRÄ:NON",
            "VER:3:PLU:KJ1:NON",
            "VER:INF:NON",
            "VER:1:PLU:PRÄ:SFT",
            "VER:1:PLU:KJ1:SFT",
            "VER:3:PLU:PRÄ:SFT",
            "VER:3:PLU:KJ1:SFT",
            "VER:INF:SFT",
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == "vergären" && t == expected),
                "{expected} missing: {tags:?}"
            );
        }

        let vergaeren_cap = Tagger::tag(&tagger, &["Vergären"]);
        let tags = tags_of(&vergaeren_cap[0]);
        assert_eq!(tags.len(), 24, "{tags:?}");
        for expected in [
            "SUB:NOM:SIN:NEU:INF",
            "SUB:DAT:SIN:NEU:INF",
            "SUB:AKK:SIN:NEU:INF",
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == "Vergären" && t == expected),
                "{expected} missing: {tags:?}"
            );
        }
        assert!(
            tags.iter()
                .any(|(l, t)| l == "vergären" && t == "VER:INF:SFT"),
            "{tags:?}"
        );

        let unbeglueckt = Tagger::tag(&tagger, &["unbeglückt"]);
        let tags = tags_of(&unbeglueckt[0]);
        assert_eq!(tags.len(), 2, "{tags:?}");
        assert!(
            tags.iter()
                .any(|(l, t)| l == "unbeglückt" && t == "PA2:PRD:GRU:VER"),
            "{tags:?}"
        );
        assert!(tags.iter().all(|(_, t)| !t.contains("VER:")), "{tags:?}");

        let erstritten = Tagger::tag(&tagger, &["erstritten"]);
        let tags = tags_of(&erstritten[0]);
        assert_eq!(tags.len(), 6, "{tags:?}");
        for (lemma, expected) in [
            ("erstreiten", "VER:1:PLU:PRT:NON"),
            ("erstreiten", "VER:PA2:NON"),
            ("erstritten", "PA2:PRD:GRU:VER"),
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == lemma && t == expected),
                "{lemma}/{expected} missing: {tags:?}"
            );
        }

        let bemessen = Tagger::tag(&tagger, &["bemessen"]);
        let tags = tags_of(&bemessen[0]);
        assert_eq!(tags.len(), 7, "{tags:?}");
        for (lemma, expected) in [
            ("bemessen", "VER:INF:NON"),
            ("bemessen", "VER:PA2:NON"),
            ("bemessen", "PA2:PRD:GRU:VER"),
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == lemma && t == expected),
                "{lemma}/{expected} missing: {tags:?}"
            );
        }

        let bemisst = Tagger::tag(&tagger, &["bemisst"]);
        let tags = tags_of(&bemisst[0]);
        assert_eq!(tags.len(), 8, "{tags:?}");
        assert!(
            !tags
                .iter()
                .any(|(l, t)| l == "bemessen" && t == "PA2:PRD:GRU:VER"),
            "{tags:?}"
        );

        let hinterlass = Tagger::tag(&tagger, &["hinterlass"]);
        let tags = tags_of(&hinterlass[0]);
        assert_eq!(tags.len(), 3, "{tags:?}");
        for expected in ["VER:IMP:SIN:NON", "VER:1:SIN:PRÄ:NON"] {
            assert!(
                tags.iter()
                    .any(|(l, t)| l == "hinterlassen" && t == expected),
                "{expected} missing: {tags:?}"
            );
        }
        assert!(tags.iter().all(|(_, t)| !t.contains("NEB")), "{tags:?}");
    }

    #[test]
    fn non_separable_prefix_imperative_and_praesens_forms() {
        let tagger = tagger();

        for (word, prefix_lemma) in [("verlasse", "verlassen"), ("behalte", "behalten")] {
            let result = Tagger::tag(&tagger, &[word]);
            let tags = tags_of(&result[0]);
            assert!(
                tags.iter()
                    .any(|(l, t)| l == prefix_lemma && t.starts_with("VER:IMP:SIN")),
                "{word} imperative missing: {tags:?}"
            );
            assert!(
                tags.iter()
                    .any(|(l, t)| l == prefix_lemma && t.starts_with("VER:1:SIN:PRÄ")),
                "{word} first person praesens missing: {tags:?}"
            );
        }
    }

    #[test]
    fn elided_imperative_e_form_tags_as_imperative() {
        let tagger = tagger();

        let result = Tagger::tag(&tagger, &["Er", "verlier", "jetzt"]);
        let tags = tags_of(&result[1]);
        assert!(
            tags.iter()
                .any(|(l, t)| l == "verlieren" && t.starts_with("VER:IMP:SIN")),
            "{tags:?}"
        );
    }

    #[test]
    fn modal_like_prefixes_stay_unmodified() {
        let tagger = tagger();

        let rauslassen = Tagger::tag(&tagger, &["rauslassen"]);
        let tags = tags_of(&rauslassen[0]);
        assert_eq!(tags.len(), 5, "{tags:?}");
        for expected in [
            "VER:1:PLU:PRÄ:NON:NEB",
            "VER:1:PLU:KJ1:NON:NEB",
            "VER:3:PLU:PRÄ:NON:NEB",
            "VER:3:PLU:KJ1:NON:NEB",
            "VER:INF:NON",
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == "rauslassen" && t == expected),
                "{expected} missing: {tags:?}"
            );
        }
        assert!(
            !tags
                .iter()
                .any(|(l, t)| l == "rauslassen" && t == "VER:1:PLU:PRÄ:NON"),
            "{tags:?}"
        );
        assert!(
            tags.iter()
                .all(|(l, t)| !(l == "rauslassen" && t.contains("VER:MOD"))),
            "{tags:?}"
        );

        let rauslass = Tagger::tag(&tagger, &["rauslass"]);
        let tags = tags_of(&rauslass[0]);
        assert_eq!(tags.len(), 2, "{tags:?}");
        assert!(
            tags.iter()
                .any(|(l, t)| l == "rauslassen" && t == "VER:1:SIN:PRÄ:NON:NEB"),
            "{tags:?}"
        );
        assert!(
            !tags
                .iter()
                .any(|(l, t)| l == "rauslassen" && t.starts_with("VER:IMP")),
            "{tags:?}"
        );

        let rauszulassen = Tagger::tag(&tagger, &["rauszulassen"]);
        let tags = tags_of(&rauszulassen[0]);
        assert_eq!(tags.len(), 1, "{tags:?}");
        assert_eq!(
            tags[0],
            ("rauslassen".to_string(), "VER:EIZ:NON".to_string())
        );

        let rausgelassen = Tagger::tag(&tagger, &["rausgelassen"]);
        let tags = tags_of(&rausgelassen[0]);
        assert_eq!(tags.len(), 2, "{tags:?}");
        for (lemma, expected) in [
            ("rauslassen", "VER:PA2:NON"),
            ("rausgelassen", "PA2:PRD:GRU:VER"),
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == lemma && t == expected),
                "{lemma}/{expected} missing: {tags:?}"
            );
        }
    }

    #[test]
    fn noun_like_words_do_not_get_verb_readings() {
        let tagger = tagger();
        for word in [
            "geschichte",
            "bereich",
            "beispiel",
            "keksdose",
            "aktienarten",
            "unmenge",
        ] {
            let result = Tagger::tag(&tagger, &[word]);
            let readings = result[0].readings();
            assert_eq!(readings.len(), 1, "{word}: {readings:?}");
            assert!(
                readings
                    .iter()
                    .all(|r| r.pos_tags().iter().all(|t| !t.contains("VER"))),
                "{word}: {readings:?}"
            );
        }

        let schwarzgrau = Tagger::tag(&tagger, &["schwarzgrau"]);
        let tags = tags_of(&schwarzgrau[0]);
        assert_eq!(tags.len(), 2, "{tags:?}");
        assert!(tags.iter().any(|(_, t)| t.contains("ADJ")), "{tags:?}");
        assert!(tags.iter().any(|(_, t)| t.contains("VER")), "{tags:?}");

        let entlang = Tagger::tag(&tagger, &["entlang"]);
        let tags = tags_of(&entlang[0]);
        assert_eq!(tags.len(), 4, "{tags:?}");
        assert!(tags.iter().any(|(_, t)| t.contains("PRP")), "{tags:?}");
        assert!(tags.iter().any(|(_, t)| t.contains("ZUS")), "{tags:?}");
        assert!(tags.iter().all(|(_, t)| !t.contains("VER:")), "{tags:?}");
    }

    #[test]
    fn verb_and_pa2_readings_match_java_counts() {
        let tagger = tagger();

        let erstickt = Tagger::tag(&tagger, &["erstickt"]);
        let tags = tags_of(&erstickt[0]);
        assert_eq!(tags.len(), 5, "{tags:?}");
        for (lemma, expected) in [
            ("ersticken", "VER:2:PLU:PRÄ:SFT"),
            ("ersticken", "VER:3:SIN:PRÄ:SFT"),
            ("ersticken", "VER:IMP:PLU:SFT"),
            ("ersticken", "VER:PA2:SFT"),
            ("erstickt", "PA2:PRD:GRU:VER"),
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == lemma && t == expected),
                "{lemma}/{expected} missing: {tags:?}"
            );
        }

        let erstickte = Tagger::tag(&tagger, &["erstickte"]);
        let tags = tags_of(&erstickte[0]);
        assert_eq!(tags.len(), 19, "{tags:?}");
        assert!(
            tags.iter()
                .any(|(l, t)| l == "erstickt" && t.starts_with("PA2")),
            "{tags:?}"
        );
        for expected in [
            "VER:1:SIN:KJ2:SFT",
            "VER:1:SIN:PRT:SFT",
            "VER:3:SIN:KJ2:SFT",
            "VER:3:SIN:PRT:SFT",
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == "ersticken" && t == expected),
                "{expected} missing: {tags:?}"
            );
        }
        assert!(tags.iter().all(|(_, t)| !t.contains("NEB")), "{tags:?}");

        let erstickend = Tagger::tag(&tagger, &["erstickend"]);
        let tags = tags_of(&erstickend[0]);
        assert_eq!(tags.len(), 2, "{tags:?}");
        for (lemma, expected) in [
            ("ersticken", "VER:PA1:SFT"),
            ("erstickend", "PA1:PRD:GRU:VER"),
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == lemma && t == expected),
                "{lemma}/{expected} missing: {tags:?}"
            );
        }

        let erstickender = Tagger::tag(&tagger, &["erstickender"]);
        let tags = tags_of(&erstickender[0]);
        assert_eq!(tags.len(), 7, "{tags:?}");
        assert!(
            tags.iter()
                .any(|(l, t)| l == "erstickend" && t == "PA1:DAT:SIN:FEM:GRU:SOL:VER"),
            "{tags:?}"
        );
    }

    #[test]
    fn manual_data_extends_and_removes_readings() {
        let dir = resource_dir().expect("german resources");
        let mut tagger = GermanTagger::from_resource_dir(&dir).expect("tagger loads");
        tagger.load_manual_data("neuwort\tneuwort\tSUB:NOM:SIN:NEU\n", "");
        let readings = tagger.lookup("neuwort").expect("manual addition");
        assert!(readings.iter().any(|r| r.has_pos_tag("SUB:NOM:SIN:NEU")));

        tagger.load_manual_data("", "Haus\tHaus\tSUB:NOM:SIN:NEU\n");
        let haus = tagger.lookup("Haus").expect("Haus");
        assert!(!haus.iter().any(|r| r.has_pos_tag("SUB:NOM:SIN:NEU")));
        assert!(haus.iter().any(|r| r.has_pos_tag("SUB:AKK:SIN:NEU")));
    }

    #[test]
    fn tag_word_and_entry_count_answer_dictionary_scale() {
        let dir = resource_dir().expect("german resources");
        let tagger = tagger();
        assert!(tagger.entry_count() > 100_000);
        let readings = tagger.tag_word("Haus", 0, 4);
        assert!(readings.iter().any(|r| r.has_pos_tag("SUB:NOM:SIN:NEU")));

        let first = shared_german_dictionary(&dir).expect("shared dict");
        let second = shared_german_dictionary(&dir).expect("shared dict again");
        assert!(std::sync::Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn generic_compound_walk_derives_partizip_ii_readings() {
        let tagger = tagger();
        let umsägt = Tagger::tag(&tagger, &["umsägt"]);
        let tags = tags_of(&umsägt[0]);
        assert!(
            tags.iter()
                .any(|(l, t)| l == "umsägen" && t == "VER:PA2:SFT"),
            "{tags:?}"
        );
        assert!(
            tags.iter()
                .any(|(l, t)| l == "umsägt" && t == "PA2:PRD:GRU:VER"),
            "{tags:?}"
        );

        let besägt = Tagger::tag(&tagger, &["besägt"]);
        let tags = tags_of(&besägt[0]);
        assert!(
            tags.iter()
                .any(|(l, t)| l == "besägen" && t == "VER:PA2:SFT"),
            "{tags:?}"
        );
        assert!(
            tags.iter()
                .any(|(l, t)| l == "besägt" && t == "PA2:PRD:GRU:VER"),
            "{tags:?}"
        );
        assert!(
            tags.iter().all(|(_, t)| !t.ends_with(":NEB")),
            "non-separable prefixes keep their finite readings without the subordinate-clause marker: {tags:?}"
        );

        let umsägt_capitalized = Tagger::tag(&tagger, &["Umsägt"]);
        let tags = tags_of(&umsägt_capitalized[0]);
        assert!(
            tags.iter()
                .any(|(l, t)| l == "Umsägen" && t == "VER:1:SIN:PRÄ:SFT:NEB"),
            "{tags:?}"
        );
        assert!(
            tags.iter()
                .all(|(_, t)| !t.starts_with("VER:PA2") && t != "PA2:PRD:GRU:VER"),
            "{tags:?}"
        );
    }

    #[test]
    fn generic_compound_walk_substantivizes_and_splits_zu_infinitives() {
        let tagger = tagger();
        let aufessen = Tagger::tag(&tagger, &["Aufessen"]);
        let tags = tags_of(&aufessen[0]);
        for tag in [
            "SUB:NOM:SIN:NEU:INF",
            "SUB:AKK:SIN:NEU:INF",
            "SUB:DAT:SIN:NEU:INF",
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == "Aufessen" && t == tag),
                "Aufessen misses {tag}: {tags:?}"
            );
        }
        assert!(
            tags.iter()
                .any(|(l, t)| l == "aufessen" && t == "VER:INF:NON"),
            "{tags:?}"
        );

        let lowercase = Tagger::tag(&tagger, &["aufessen"]);
        let tags = tags_of(&lowercase[0]);
        assert!(
            tags.iter()
                .any(|(l, t)| l == "aufessen" && t == "VER:INF:NON"),
            "{tags:?}"
        );
        assert!(
            !tags
                .iter()
                .any(|(_, t)| t.starts_with("SUB:") && t.ends_with(":INF")),
            "a lowercase infinitive compound is not substantivized: {tags:?}"
        );

        let anzuessen = Tagger::tag(&tagger, &["anzuessen"]);
        let tags = tags_of(&anzuessen[0]);
        assert!(
            tags.iter()
                .any(|(l, t)| l == "anessen" && t == "VER:EIZ:NON"),
            "{tags:?}"
        );

        let abfiedeln = Tagger::tag(&tagger, &["Abfiedeln"]);
        let tags = tags_of(&abfiedeln[0]);
        for tag in [
            "SUB:NOM:SIN:NEU:INF",
            "SUB:AKK:SIN:NEU:INF",
            "SUB:DAT:SIN:NEU:INF",
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == "Abfiedeln" && t == tag),
                "Abfiedeln misses {tag}: {tags:?}"
            );
        }
        assert!(
            tags.iter()
                .any(|(l, t)| l == "abfiedeln" && t == "VER:INF:SFT"),
            "{tags:?}"
        );
    }
    #[test]
    fn non_separable_prefix_expansions_copy_the_base_verb_readings() {
        let tagger = tagger();

        let verimpfst = Tagger::tag(&tagger, &["verimpfst"]);
        assert_eq!(
            tags_of(&verimpfst[0]),
            vec![("verimpfen".to_string(), "VER:2:SIN:PRÄ:SFT".to_string())],
            "a plain conjugated form copies its only base reading under the prefixed lemma"
        );

        let verimpfte = Tagger::tag(&tagger, &["verimpfte"]);
        let tags = tags_of(&verimpfte[0]);
        assert_eq!(tags.len(), 4, "{tags:?}");
        for expected in [
            "VER:1:SIN:KJ2:SFT",
            "VER:1:SIN:PRT:SFT",
            "VER:3:SIN:KJ2:SFT",
            "VER:3:SIN:PRT:SFT",
        ] {
            assert!(
                tags.iter().any(|(l, t)| l == "verimpfen" && t == expected),
                "{expected} missing: {tags:?}"
            );
        }

        let vermixt = Tagger::tag(&tagger, &["vermixt"]);
        let tags = tags_of(&vermixt[0]);
        assert_eq!(tags.len(), 4, "{tags:?}");
        assert!(
            tags.iter()
                .any(|(l, t)| l == "vermixen" && t == "VER:IMP:PLU:SFT"),
            "{tags:?}"
        );

        let entfolgt = Tagger::tag(&tagger, &["entfolgt"]);
        let tags = tags_of(&entfolgt[0]);
        assert_eq!(tags.len(), 3, "{tags:?}");
        assert!(
            tags.iter()
                .any(|(l, t)| l == "entfolgen" && t == "VER:3:SIN:PRÄ:SFT"),
            "{tags:?}"
        );

        let beklettert = Tagger::tag(&tagger, &["beklettert"]);
        assert!(
            tags_of(&beklettert[0])
                .iter()
                .any(|(l, t)| l == "beklettern" && t == "VER:2:PLU:PRÄ:SFT"),
            "the be- prefix forms the same copied readings"
        );
    }

    #[test]
    fn non_separable_prefix_imperative_swap_lacks_the_colon_like_java() {
        let tagger = tagger();
        let verimpfe = Tagger::tag(&tagger, &["verimpfe"]);
        let tags = tags_of(&verimpfe[0]);
        assert!(
            tags.iter()
                .any(|(l, t)| l == "verimpfen" && t == "VER:IMP:SINSFT"),
            "Java concatenates VER:IMP:SIN with the flektion without a separator: {tags:?}"
        );
        assert!(
            tags.iter()
                .any(|(l, t)| l == "verimpfen" && t == "VER:1:SIN:PRÄ:SFT"),
            "{tags:?}"
        );
    }
    #[test]
    fn weise_compounds_expand_to_the_full_sol_adjective_paradigm() {
        let tagger = tagger();
        let gelberweise = Tagger::tag(&tagger, &["gelberweise"]);
        let tags = tags_of(&gelberweise[0]);
        assert_eq!(tags.len(), 16, "{tags:?}");
        assert!(
            tags.iter()
                .all(|(l, t)| l == "gelberweise" && t.starts_with("ADJ:")),
            "{tags:?}"
        );
        for expected in [
            "ADJ:AKK:PLU:FEM:GRU:SOL",
            "ADJ:NOM:SIN:MAS:GRU:DEF",
            "ADJ:NOM:SIN:NEU:GRU:DEF",
            "ADJ:PRD:GRU",
        ] {
            assert!(
                tags.iter().any(|(_, t)| t == expected),
                "{expected} missing: {tags:?}"
            );
        }
        let dict_word = Tagger::tag(&tagger, &["lustigerweise"]);
        assert!(
            tags_of(&dict_word[0])
                .iter()
                .all(|(_, t)| t.starts_with("ADV:")),
            "dict-listed adverbs keep their dictionary readings"
        );
    }
}
