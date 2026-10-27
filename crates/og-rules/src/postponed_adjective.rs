use og_core::{AnalyzedToken, AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};
use og_tagger::morfologik::MorfologikDictionary;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

const MAX_LEVELS: usize = 4;

fn full_match(pattern: &str) -> Regex {
    Regex::new(&format!("^(?:{pattern})$")).expect("valid postag pattern")
}

fn reading_matches(reading: &AnalyzedToken, re: &Regex) -> bool {
    let tags = reading.pos_tags();
    if tags.is_empty() {
        re.is_match("UNKNOWN")
    } else {
        tags.iter().any(|t| re.is_match(t))
    }
}

fn mp(token: Option<&AnalyzedTokenReadings>, re: &Regex) -> bool {
    token.is_some_and(|t| t.readings().iter().any(|r| reading_matches(r, re)))
}

fn mr(text: &str, re: &Regex) -> bool {
    re.is_match(text)
}

fn matching_reading<'a>(token: &'a AnalyzedTokenReadings, re: &Regex) -> Option<&'a AnalyzedToken> {
    token.readings().iter().find(|r| reading_matches(r, re))
}

#[derive(Default)]
struct Apparitions {
    adverb: bool,
    conjunction: bool,
    punctuation: bool,
    infinitive: bool,
}

#[derive(Default)]
struct Levels {
    nt: [u32; MAX_LEVELS],
    nms: [u32; MAX_LEVELS],
    nfs: [u32; MAX_LEVELS],
    nmp: [u32; MAX_LEVELS],
    nmn: [u32; MAX_LEVELS],
    nfp: [u32; MAX_LEVELS],
    ncs: [u32; MAX_LEVELS],
    ncp: [u32; MAX_LEVELS],
    dms: [u32; MAX_LEVELS],
    dfs: [u32; MAX_LEVELS],
    dmp: [u32; MAX_LEVELS],
    dfp: [u32; MAX_LEVELS],
}

struct TagSet {
    nom: Regex,
    nom_ms: Regex,
    nom_fs: Regex,
    nom_mp: Regex,
    nom_mn: Regex,
    nom_fp: Regex,
    nom_cs: Regex,
    nom_cp: Regex,
    nom_det: Regex,
    gn_any: Regex,
    gn_ms_tag: Regex,
    gn_fs_tag: Regex,
    gn_mp_tag: Regex,
    gn_fp_tag: Regex,
    gn_cs_tag: Regex,
    gn_cp_tag: Regex,
    det: Regex,
    det_cs: Regex,
    det_ms: Regex,
    det_fs: Regex,
    det_mp: Regex,
    det_fp: Regex,
    gn_ms: Regex,
    gn_fs: Regex,
    gn_mp: Regex,
    gn_fp: Regex,
    gn_cp: Regex,
    gn_cs: Regex,
    adjectiu: Regex,
    adjectiu_ms: Regex,
    adjectiu_fs: Regex,
    adjectiu_mp: Regex,
    adjectiu_fp: Regex,
    adjectiu_cp: Regex,
    adjectiu_cs: Regex,
    adjectiu_s: Regex,
    adjectiu_p: Regex,
    adverbi: Regex,
    conjuncio: Regex,
    puntuacio: Regex,
    loc_adv: Regex,
    adverbis_acceptats: Regex,
    coord_ioni: Regex,
    keep_count: Regex,
    keep_count2: Regex,
    stop_count: Regex,
    preposicions: Regex,
    prepo_nivell: Regex,
    verb: Regex,
    gv: Regex,
    synth_common_plural: Regex,
    synth_cs: Regex,
    synth_cp: Regex,
    synth_ms: Regex,
    synth_fs: Regex,
    synth_mp: Regex,
    synth_fp: Regex,
}

fn es_tags() -> &'static TagSet {
    static TAGS: OnceLock<TagSet> = OnceLock::new();
    TAGS.get_or_init(|| TagSet {
        nom: full_match("N.*"),
        nom_ms: full_match("N.MS.*|PI0MS000"),
        nom_fs: full_match("N.FS.*|PI0FS000"),
        nom_mp: full_match("N.MP.*"),
        nom_mn: full_match("N.MN.*"),
        nom_fp: full_match("N.FP.*"),
        nom_cs: full_match("N.CS.*"),
        nom_cp: full_match("N.CP.*"),
        nom_det: full_match("N.*|D[NDA0I].*|PI0[MF]S000"),
        gn_any: full_match("_GN_.*"),
        gn_ms_tag: full_match("_GN_MS"),
        gn_fs_tag: full_match("_GN_FS"),
        gn_mp_tag: full_match("_GN_MP"),
        gn_fp_tag: full_match("_GN_FP"),
        gn_cs_tag: full_match("_GN_[MF]S"),
        gn_cp_tag: full_match("_GN_[MF]P"),
        det: full_match("D[NDA0IP].*"),
        det_cs: full_match("D[NDA0IP]0CS0"),
        det_ms: full_match("D[NDA0IP]0MS0"),
        det_fs: full_match("D[NDA0IP]0FS0"),
        det_mp: full_match("D[NDA0IP]0MP0"),
        det_fp: full_match("D[NDA0IP]0FP0"),
        gn_ms: full_match(
            "N.[MC][SN].*|A..[MC][SN].*|V.P..SM.?|PX.MS.*|D[NDA0I]0MS0|PI0MS000",
        ),
        gn_fs: full_match(
            "N.[FC][SN].*|A..[FC][SN].*|V.P..SF.?|PX.FS.*|D[NDA0I]0FS0|PI0FS000",
        ),
        gn_mp: full_match(
            "N.[MC][PN].*|A..[MC][PN].*|V.P..PM.?|PX.MP.*|D[NDA0I]0MP0",
        ),
        gn_fp: full_match(
            "N.[FC][PN].*|A..[FC][PN].*|V.P..PF.?|PX.FP.*|D[NDA0I]0FP0",
        ),
        gn_cp: full_match("N.[FMC][PN].*|A..[FMC][PN].*|D[NDA0I]0[FM]P0"),
        gn_cs: full_match("N.[FMC][SN].*|A..[FMC][SN].*|D[NDA0I]0[FM]S0||PI0[MFC]S000"),
        adjectiu: full_match("AQ.*|V.P.*|PX.*|.*LOC_ADJ.*"),
        adjectiu_ms: full_match("A..[MC][SN].*|V.P..SM.?|PX.MS.*"),
        adjectiu_fs: full_match("A..[FC][SN].*|V.P..SF.?|PX.FS.*"),
        adjectiu_mp: full_match("A..[MC][PN].*|V.P..PM.?|PX.MP.*"),
        adjectiu_fp: full_match("A..[FC][PN].*|V.P..PF.?|PX.FP.*"),
        adjectiu_cp: full_match("A..C[PN].*"),
        adjectiu_cs: full_match("A..C[SN].*"),
        adjectiu_s: full_match("A...[SN].*|V.P..S..?|PX..S.*"),
        adjectiu_p: full_match("A...[PN].*|V.P..P..?|PX..P.*"),
        adverbi: full_match("R.|.*LOC_ADV.*"),
        conjuncio: full_match("C.|.*LOC_CONJ.*"),
        puntuacio: full_match("_PUNCT"),
        loc_adv: full_match(".*LOC_ADV.*"),
        adverbis_acceptats: full_match("RG_before"),
        coord_ioni: full_match("y|e|o|u|ni"),
        keep_count: full_match(
            "A.*|N.*|D[NAIDP].*|SPS.*|SP:DA|.*LOC_ADV.*|V.P.*|_PUNCT.*|.*LOC_ADJ.*|PX.*|PI0.S000|UNKNOWN|V.N.{4}",
        ),
        keep_count2: full_match(",|y|e|o|ni|u"),
        stop_count: full_match(";|lo"),
        preposicions: full_match("SP.*"),
        prepo_nivell: full_match("de|del|en|sobre|a|entre|por|con|sin|contra|para"),
        verb: full_match("V.[^P].*|_GV_"),
        gv: full_match("_GV_"),
        synth_cs: full_match("A..CP."),
        synth_cp: full_match("A..CS."),
        synth_common_plural: full_match("A...P.|V.P..P.|PX..P.*"),
        synth_ms: full_match("A..MS.|V.P..SM|PX.MS.*"),
        synth_fs: full_match("A..FS.|V.P..SF|PX.FS.*"),
        synth_mp: full_match("A..MP.|V.P..PM|PX.MP.*"),
        synth_fp: full_match("A..FP.|V.P..PF|PX.FP.*"),
    })
}

struct FrTagSet {
    base: TagSet,
    det_cp: Regex,
    gn_mn: Regex,
    gn_fn: Regex,
    gn_mn_tag: Regex,
    gn_fn_tag: Regex,
    adjectiu_mn: Regex,
    adjectiu_fn: Regex,
    adjectiu_m: Regex,
    infinitive: Regex,
}

fn fr_tags() -> &'static FrTagSet {
    static TAGS: OnceLock<FrTagSet> = OnceLock::new();
    TAGS.get_or_init(|| FrTagSet {
        base: TagSet {
            nom: full_match("[NZ] .*"),
            nom_ms: full_match("[NZ] m s"),
            nom_fs: full_match("[NZ] f s"),
            nom_mp: full_match("[NZ] m p"),
            nom_mn: full_match("[NZ] m sp"),
            nom_fp: full_match("[NZ] f p"),
            nom_cs: full_match("[NZ] e s"),
            nom_cp: full_match("[NZ] e sp"),
            nom_det: full_match("[NZ] .*|(P\\+)?D .*"),
            gn_any: full_match("_GN_.*"),
            gn_ms_tag: full_match("_GN_MS"),
            gn_fs_tag: full_match("_GN_FS"),
            gn_mp_tag: full_match("_GN_MP"),
            gn_fp_tag: full_match("_GN_FP"),
            gn_cs_tag: full_match("_GN_[MF]S"),
            gn_cp_tag: full_match("_GN_[MF]P"),
            det: full_match("(P\\+)?D .*"),
            det_cs: full_match("(P\\+)?D e s"),
            det_ms: full_match("(P\\+)?D m s"),
            det_fs: full_match("(P\\+)?D f s"),
            det_mp: full_match("(P\\+)?D m p"),
            det_fp: full_match("(P\\+)?D f p"),
            gn_ms: full_match("[NZ] [me] (s|sp)|J [me] (s|sp)|V ppa m s|(P\\+)?D m (s|sp)"),
            gn_fs: full_match("[NZ] [fe] (s|sp)|J [fe] (s|sp)|V ppa f s|(P\\+)?D f (s|sp)"),
            gn_mp: full_match("[NZ] [me] (p|sp)|J [me] (p|sp)|V ppa m p|(P\\+)?D m (p|sp)"),
            gn_fp: full_match("[NZ] [fe] (p|sp)|J [fe] (p|sp)|V ppa f p|(P\\+)?D f (p|sp)"),
            gn_cp: full_match("[NZ] [fme] (p|sp)|J [fme] (p|sp)|(P\\+)?D [fme] (p|sp)"),
            gn_cs: full_match("[NZ] [fme] (s|sp)|J [fme] (s|sp)|(P\\+)?D [fme] (s|sp)"),
            adjectiu: full_match("J .*|V ppa .*|PX.*"),
            adjectiu_ms: full_match("J [me] (s|sp)|V ppa m s"),
            adjectiu_fs: full_match("J [fe] (s|sp)|V ppa f s"),
            adjectiu_mp: full_match("J [me] (p|sp)|V ppa m p"),
            adjectiu_fp: full_match("J [fe] (p|sp)|V ppa f p"),
            adjectiu_cp: full_match("J e (p|sp)"),
            adjectiu_cs: full_match("J e (s|sp)"),
            adjectiu_s: full_match("J .* (s|sp)|V ppa . s"),
            adjectiu_p: full_match("J .* (p|sp)|V ppa . p"),
            adverbi: full_match("A"),
            conjuncio: full_match("C .*"),
            puntuacio: full_match("_PUNCT"),
            loc_adv: full_match("A"),
            adverbis_acceptats: full_match("A"),
            coord_ioni: full_match("et|ou|ni"),
            keep_count: full_match(
                "Y|J .*|N .*|D .*|P.*|V ppa .*|M nonfin|UNKNOWN|Z.*|V.* inf|V ppr",
            ),
            keep_count2: full_match(",|et|ou|ni"),
            stop_count: full_match("[;:\\(\\)\\[\\]–—―‒]"),
            preposicions: full_match("P.*"),
            prepo_nivell: full_match(
                "d'|de|des|du|à|au|aux|en|dans|sur|entre|par|pour|avec|sans|contre|comme",
            ),
            verb: full_match("V.* (inf|ind|sub|con|ppr|imp).*"),
            gv: full_match("_GV_"),
            synth_cs: full_match("J e p"),
            synth_cp: full_match("J e s"),
            synth_common_plural: full_match("J . p|V ppa . p"),
            synth_ms: full_match("J [me] sp?|V ppa m s"),
            synth_fs: full_match("J [fe] sp?|V ppa f s"),
            synth_mp: full_match("J [me] s?p|V ppa m p"),
            synth_fp: full_match("J [fe] s?p|V ppa f p"),
        },
        det_cp: full_match("(P\\+)?D e p"),
        gn_mn: full_match("[NZ] [me] (s|p|sp)|J [me] (s|p|sp)|(P\\+)?D [me] (s|p|sp)"),
        gn_fn: full_match("[NZ] [fe] (s|p|sp)|J [fe] (s|p|sp)|(P\\+)?D [fe] (s|p|sp)"),
        gn_mn_tag: full_match("_GN_M[SP]"),
        gn_fn_tag: full_match("_GN_F[SP]"),
        adjectiu_mn: full_match("J m sp"),
        adjectiu_fn: full_match("J f sp"),
        adjectiu_m: full_match("J [me] .*|V ppa [me] .*"),
        infinitive: full_match("V.* inf"),
    })
}

const FR_EXCEPTIONS_EGRAVE: [&str; 3] = ["burkinabè", "koinè", "épistémè"];

fn is_exception_fr(word: &str) -> bool {
    if word.starts_with("qq") {
        return true;
    }
    if word.ends_with('è') {
        let lower = word.to_lowercase();
        return !FR_EXCEPTIONS_EGRAVE.contains(&lower.as_str());
    }
    false
}

fn occurs_after_start(haystack: &str, needle: &str) -> bool {
    haystack
        .match_indices(needle)
        .any(|(i, _)| i > 0 && haystack.is_char_boundary(i))
}

fn pl_is_negated(source_postag: Option<&str>, pos_tag: &str) -> bool {
    occurs_after_start(pos_tag, ":neg")
        || source_postag.is_some_and(|p| occurs_after_start(p, ":neg"))
            && !occurs_after_start(pos_tag, "com")
            && !occurs_after_start(pos_tag, "sup")
}

fn replace_first_str(haystack: &str, from: &str, to: &str) -> String {
    match haystack.find(from) {
        Some(i) => format!("{}{}{}", &haystack[..i], to, &haystack[i + from.len()..]),
        None => haystack.to_string(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SynthLang {
    German,
    Spanish,
    French,
    English,
    Portuguese,
    Polish,
    Galician,
    Romanian,
    Slovak,
    Italian,
    Swedish,
}

pub struct LangSynthesizer {
    dictionary: Option<Arc<MorfologikDictionary>>,
    seek: Option<Arc<og_tagger::morfologik::SynthDictionary>>,
    manual: Vec<(Box<str>, Box<str>, Box<str>)>,
    removals: Vec<(Box<str>, Box<str>, Box<str>)>,
    removals2: Vec<(Box<str>, Box<str>, Box<str>)>,
    lang: SynthLang,
    article: crate::en_article::ArticleData,
    possible_tags: Vec<Box<str>>,
}

const GERMAN_REMOVE: &[&str] = &[
    "unsren",
    "unsrem",
    "unsres",
    "unsre",
    "unsern",
    "unserm",
    "unsrer",
    "angepaßt",
    "beschloß",
    "biß",
    "entschloß",
    "ergoß",
    "faßt",
    "genoß",
    "paßt",
    "paßte",
    "preßt",
    "preßte",
    "riß",
    "schloß",
    "streßtest",
    "vergißt",
    "verlaß",
    "verläßt",
    "vermiß",
    "vermißt",
    "wißt",
    "wußtest",
    "wüßtest",
];

fn shared_decoded_dictionary(path: &Path) -> Option<Arc<MorfologikDictionary>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, std::sync::Weak<MorfologikDictionary>>>,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut guard = cache.lock().unwrap();
    if let Some(dict) = guard.get(path).and_then(std::sync::Weak::upgrade) {
        return Some(dict);
    }
    let data = std::fs::read_to_string(path).ok()?;
    let dictionary = Arc::new(MorfologikDictionary::from_decoded_text(&data));
    guard.insert(path.to_path_buf(), std::sync::Arc::downgrade(&dictionary));
    Some(dictionary)
}

impl LangSynthesizer {
    pub fn for_language(resource_dir: &Path, code: &str) -> Option<Self> {
        let (dictionary, seek, lang): (
            Option<Arc<MorfologikDictionary>>,
            Option<Arc<og_tagger::morfologik::SynthDictionary>>,
            SynthLang,
        ) = match code {
            "de" => (
                og_tagger::german_tagger::shared_german_dictionary(resource_dir),
                None,
                SynthLang::German,
            ),
            "es" => (
                og_tagger::generic_tagger::shared_dictionary(resource_dir, "spanish"),
                None,
                SynthLang::Spanish,
            ),
            "fr" => (
                og_tagger::generic_tagger::shared_dictionary(resource_dir, "french"),
                None,
                SynthLang::French,
            ),
            "pt" => (
                og_tagger::generic_tagger::shared_dictionary(resource_dir, "portuguese"),
                None,
                SynthLang::Portuguese,
            ),
            "pl" => (
                og_tagger::generic_tagger::shared_dictionary(resource_dir, "polish_synth"),
                None,
                SynthLang::Polish,
            ),
            "en" => {
                let dict = shared_decoded_dictionary(&resource_dir.join("dict_decoded.txt"));
                (dict, None, SynthLang::English)
            }
            "gl" => (
                None,
                og_tagger::morfologik::shared_synth_dictionary(resource_dir, "galician_synth"),
                SynthLang::Galician,
            ),
            "ro" => (
                None,
                og_tagger::morfologik::shared_synth_dictionary(resource_dir, "romanian_synth"),
                SynthLang::Romanian,
            ),
            "sk" => (
                None,
                og_tagger::morfologik::shared_synth_dictionary(resource_dir, "slovak_synth"),
                SynthLang::Slovak,
            ),
            "it" => (
                None,
                og_tagger::morfologik::shared_synth_dictionary(resource_dir, "italian_synth"),
                SynthLang::Italian,
            ),
            "sv" => (
                None,
                og_tagger::morfologik::shared_synth_dictionary(resource_dir, "swedish_synth"),
                SynthLang::Swedish,
            ),
            _ => return None,
        };
        let manual = std::fs::read_to_string(resource_dir.join("added.txt"))
            .map(|t| parse_manual(&t))
            .unwrap_or_default();
        let removals = std::fs::read_to_string(resource_dir.join("removed.txt"))
            .map(|t| parse_manual(&t))
            .unwrap_or_default();
        let removals2 = std::fs::read_to_string(resource_dir.join("do-not-synthesize.txt"))
            .map(|t| parse_manual(&t))
            .unwrap_or_default();
        let article = if lang == SynthLang::English {
            let rules_dir = resource_dir.join("../../rules/en");
            if rules_dir.join("det_a.txt").is_file() {
                crate::en_article::ArticleData::from_rules_dir(&rules_dir)
            } else {
                crate::en_article::ArticleData::empty()
            }
        } else {
            crate::en_article::ArticleData::empty()
        };
        let possible_tags = match lang {
            SynthLang::Polish => Some("polish_tags.txt"),
            SynthLang::Galician => Some("galician_tags.txt"),
            SynthLang::Romanian => Some("romanian_tags.txt"),
            SynthLang::Slovak => Some("slovak_tags.txt"),
            SynthLang::Italian => Some("italian_tags.txt"),
            SynthLang::Swedish => Some("swedish_synth.dict_tags.txt"),
            _ => None,
        };
        let possible_tags = possible_tags
            .and_then(|name| std::fs::read_to_string(resource_dir.join(name)).ok())
            .map(|t| {
                t.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .map(Box::from)
                    .collect::<Vec<Box<str>>>()
            })
            .unwrap_or_default();
        let mut possible_tags = possible_tags;
        for (_, tag, _) in &manual {
            if !possible_tags.iter().any(|t| t == tag) {
                possible_tags.push(tag.clone());
            }
        }
        Some(Self {
            dictionary,
            seek,
            manual,
            removals,
            removals2,
            lang,
            article,
            possible_tags,
        })
    }

    fn is_german(&self) -> bool {
        self.lang == SynthLang::German
    }

    fn is_french(&self) -> bool {
        self.lang == SynthLang::French
    }

    fn is_english(&self) -> bool {
        self.lang == SynthLang::English
    }

    fn is_spanish(&self) -> bool {
        self.lang == SynthLang::Spanish
    }

    pub fn synthesize(&self, lemma: &str, tag: &Regex) -> Vec<String> {
        let mut results = self.lookup_forms(lemma, tag);
        if self.is_german() {
            results.retain(|r| !GERMAN_REMOVE.contains(&r.as_str()));
        }
        results
    }

    fn german_lookup_ok(&self, lemma: &str, form: &str) -> bool {
        if !self.is_german() {
            return true;
        }
        let lc_lemma = lemma.chars().next().is_some_and(|c| c.is_lowercase());
        let lc_form = form.chars().next().is_some_and(|c| c.is_lowercase());
        lc_lemma == lc_form || lemma == "mein" || lemma == "ich" && !GERMAN_REMOVE.contains(&form)
    }

    fn keep_form(&self, form: &str) -> bool {
        if self.is_french() {
            return !is_exception_fr(form);
        }
        if self.is_english() {
            return !is_exception_en(form);
        }
        true
    }

    fn split_es_lemma<'a>(&self, lemma: &'a str, postag: &str) -> (&'a str, &'a str) {
        if self.is_spanish()
            && postag.starts_with('V')
            && let Some((head, tail)) = lemma.split_once(' ')
        {
            (head, tail)
        } else {
            (lemma, "")
        }
    }

    pub fn is_determiner_tag(&self, postag: &str) -> bool {
        self.is_english() && (postag == "+DT" || postag == "+INDT")
    }

    pub fn determiner_forms(&self, token: &str, postag: &str) -> Vec<String> {
        if postag == "+DT" {
            return vec![
                self.article.suggest_a_or_an(token),
                format!(
                    "the {}",
                    crate::en_article::lowercase_first_char_if_capitalized(token)
                ),
            ];
        }
        vec![self.article.suggest_a_or_an(token)]
    }

    pub fn synthesize_exact(&self, lemma: &str, postag: &str) -> Vec<String> {
        self.synthesize_exact_for(lemma, None, postag)
    }

    pub fn synthesize_exact_for(
        &self,
        lemma: &str,
        source_postag: Option<&str>,
        postag: &str,
    ) -> Vec<String> {
        if self.lang == SynthLang::Polish {
            if postag.find('+').is_some_and(|i| i > 0) {
                return self.pl_synthesize_regexp(lemma, source_postag, postag);
            }
            let negated = pl_is_negated(source_postag, postag);
            let mut results = Vec::new();
            self.pl_word_forms(lemma, postag, negated, &mut results);
            return results;
        }
        let (head, tail) = self.split_es_lemma(lemma, postag);
        let mut results = self.lookup_exact(head, postag);
        if results.is_empty() && self.is_german() {
            results = self.compound_forms_matching(head, |t| t == postag);
        } else if self.is_german() {
            results.retain(|r| !GERMAN_REMOVE.contains(&r.as_str()));
        }
        if !tail.is_empty() {
            for form in &mut results {
                *form = format!("{form} {tail}");
            }
        }
        results
    }

    fn pl_word_forms(&self, lemma: &str, postag: &str, negated: bool, out: &mut Vec<String>) {
        if negated {
            let replaced = replace_first_str(postag, ":neg", ":aff");
            for form in self.lookup_exact(lemma, &replaced) {
                out.push(format!("nie{form}"));
            }
        } else {
            out.extend(self.lookup_exact(lemma, postag));
        }
    }

    fn pl_synthesize_regexp(
        &self,
        lemma: &str,
        source_postag: Option<&str>,
        pos_tag: &str,
    ) -> Vec<String> {
        let negated = pl_is_negated(source_postag, pos_tag);
        let mut pattern = pos_tag.to_string();
        if negated {
            pattern = pattern.replace(":neg", ":aff?");
        }
        pattern = pattern.replace('+', "|");
        let Some(re) = og_xml::compiler::shared_cached_regex(&format!("^(?:{pattern})$")) else {
            return Vec::new();
        };
        let mut results = Vec::new();
        for tag in &self.possible_tags {
            if re.is_match(tag) {
                self.pl_word_forms(lemma, tag, negated, &mut results);
            }
        }
        results.sort();
        results.dedup();
        results
    }

    pub fn target_postag<'a>(&self, pos_tags: &[&'a str]) -> Option<&'a str> {
        if pos_tags.is_empty() {
            return None;
        }
        if !self.is_spanish() {
            return pos_tags.last().copied();
        }
        let mut sorted: Vec<&str> = pos_tags.to_vec();
        sorted.sort_by(|a, b| {
            if *a == "VMIP3S0" && *b == "VMM02S0" {
                std::cmp::Ordering::Greater
            } else if *a == "VMM02S0" && *b == "VMIP3S0" {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        });
        sorted.pop()
    }

    pub fn synthesize_full(&self, lemma: &str, target: &str) -> Vec<String> {
        self.synthesize_full_for(lemma, None, target)
    }

    pub fn synthesize_full_for(
        &self,
        lemma: &str,
        source_postag: Option<&str>,
        target: &str,
    ) -> Vec<String> {
        if self.lang == SynthLang::Polish {
            return self.pl_synthesize_regexp(lemma, source_postag, target);
        }
        let Some(re) = og_xml::compiler::shared_cached_regex(&format!("^(?:{target})$")) else {
            return Vec::new();
        };
        let (head, tail) = self.split_es_lemma(lemma, target);
        let mut results = self.synthesize(head, re.as_ref());
        if results.is_empty() && self.is_german() {
            results = self.compound_forms_matching(head, |t| re.is_match(t));
        }
        if !tail.is_empty() {
            for form in &mut results {
                *form = format!("{form} {tail}");
            }
        }
        results.sort();
        results.dedup();
        results
    }

    fn compound_parts(&self, lemma: &str) -> Option<(String, String, &'static str, bool)> {
        let mut parts = og_tagger::jwordsplitter::tokenize(lemma);
        if parts.is_empty() {
            return None;
        }
        let mut maybe_hyphen = "";
        if parts.len() == 1 {
            let hyphen_split: Vec<String> = lemma.split('-').map(str::to_string).collect();
            if hyphen_split.len() > 1 {
                maybe_hyphen = "-";
                parts = hyphen_split;
            }
        }
        let (last_raw, first_parts) = parts.split_last()?;
        let first_part = first_parts.join(maybe_hyphen);
        let last_part = uppercase_first_char(last_raw);
        let uppercase_last =
            !maybe_hyphen.is_empty() && last_raw.chars().next().is_some_and(char::is_uppercase);
        Some((last_part, first_part, maybe_hyphen, uppercase_last))
    }

    fn compound_forms_matching(
        &self,
        lemma: &str,
        matches_tag: impl Fn(&str) -> bool,
    ) -> Vec<String> {
        let Some((last_part, first_part, maybe_hyphen, uppercase_last)) =
            self.compound_parts(lemma)
        else {
            return Vec::new();
        };
        let mut results = Vec::new();
        for form in self.lookup_matching(&last_part, &matches_tag) {
            let tail = if uppercase_last {
                form
            } else {
                lowercase_first_char(&form)
            };
            results.push(format!("{first_part}{maybe_hyphen}{tail}"));
        }
        results
    }

    fn lookup_matching(&self, lemma: &str, matches_tag: &impl Fn(&str) -> bool) -> Vec<String> {
        let mut results = Vec::new();
        if let Some(dict) = &self.dictionary {
            let entries = dict
                .synth_forms_by_lemma()
                .unwrap_or_else(|| dict.forms_by_lemma());
            if let Some(entries) = entries.get(lemma) {
                let mut sorted: Vec<(&str, &str)> =
                    entries.iter().map(|(t, form)| (&**t, &**form)).collect();
                sorted.sort_unstable();
                for (t, form) in sorted {
                    if matches_tag(t) && self.german_lookup_ok(lemma, form) {
                        results.push(form.to_string());
                    }
                }
            }
        }
        self.extend_manual(lemma, &mut results, matches_tag);
        self.apply_removals(lemma, &mut results, matches_tag);
        results.retain(|r| self.keep_form(r));
        results
    }

    fn lookup_forms(&self, lemma: &str, tag: &Regex) -> Vec<String> {
        if let Some(dict) = &self.seek {
            let mut results = Vec::new();
            for t in &self.possible_tags {
                if tag.is_match(t) {
                    results.extend(dict.lookup(&format!("{lemma}|{t}")));
                }
            }
            self.extend_manual(lemma, &mut results, &|t| tag.is_match(t));
            self.apply_removals(lemma, &mut results, &|t| tag.is_match(t));
            results.retain(|r| self.keep_form(r));
            return results;
        }
        self.lookup_matching(lemma, &|t| tag.is_match(t))
    }

    fn lookup_exact(&self, lemma: &str, postag: &str) -> Vec<String> {
        if let Some(dict) = &self.seek {
            let mut results = dict.lookup(&format!("{lemma}|{postag}"));
            self.extend_manual(lemma, &mut results, &|t| t == postag);
            self.apply_removals(lemma, &mut results, &|t| t == postag);
            results.retain(|r| self.keep_form(r));
            return results;
        }
        self.lookup_matching(lemma, &|t| t == postag)
    }

    fn extend_manual(
        &self,
        lemma: &str,
        results: &mut Vec<String>,
        matches_tag: &impl Fn(&str) -> bool,
    ) {
        for (l, t, form) in &self.manual {
            if l.as_ref() == lemma && matches_tag(t) && self.german_lookup_ok(lemma, form) {
                results.push(form.to_string());
            }
        }
    }

    fn apply_removals(
        &self,
        lemma: &str,
        results: &mut Vec<String>,
        matches_tag: &impl Fn(&str) -> bool,
    ) {
        for (l, t, form) in self.removals.iter().chain(self.removals2.iter()) {
            if l.as_ref() == lemma && matches_tag(t) {
                results.retain(|r| r.as_str() != form.as_ref());
            }
        }
    }
}

const EN_EXCEPTIONS: &[&str] = &[
    "ne'er",
    "e'er",
    "o'er",
    "ol'",
    "ma'am",
    "n't",
    "informations",
];

fn is_exception_en(word: &str) -> bool {
    word.starts_with('\'') || EN_EXCEPTIONS.contains(&word)
}

fn uppercase_first_char(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn lowercase_first_char(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn parse_manual(text: &str) -> Vec<(Box<str>, Box<str>, Box<str>)> {
    let mut entries = Vec::new();
    let mut separator = "\t";
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if let Some(sep) = line.strip_prefix("#separatorRegExp=") {
            separator = sep;
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.split('#').next().unwrap_or("").trim();
        let mut parts = line.splitn(3, separator);
        if let (Some(form), Some(lemma), Some(tag)) = (parts.next(), parts.next(), parts.next()) {
            entries.push((lemma.into(), tag.trim().into(), form.into()));
        }
    }
    entries
}

fn arg_is_true(args: &str, key: &str) -> bool {
    args.split_whitespace().any(|part| {
        part.strip_prefix(&format!("{key}:"))
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    })
}

fn dedup_preserving_order(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::with_capacity(values.len());
    values
        .into_iter()
        .filter(|v| seen.insert(v.clone()))
        .collect()
}

pub(crate) fn accept_es(
    synthesizer: Option<&LangSynthesizer>,
    args: &str,
    matched_positions: &[usize],
    tokens: &[&AnalyzedTokenReadings],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let t = es_tags();
    let add_comma = arg_is_true(args, "addComma");
    let i = *matched_positions.first()?;
    let token_at = |idx: usize| tokens.get(idx).copied();
    let adjective = token_at(i);

    let mut is_plural = true;
    let mut is_prev_noun = false;
    let mut can_be_ms = false;
    let mut can_be_fs = false;
    let mut can_be_mp = false;
    let mut can_be_fp = false;
    let mut can_be_p = false;
    let mut lv = Levels::default();
    let mut apparitions = Apparitions::default();
    let mut level = 0usize;
    let mut j = 1usize;

    while i > j && keep_counting_es(token_at(i - j), &apparitions, t) && level < MAX_LEVELS {
        let cur = i - j;
        let current = token_at(cur);
        if !is_prev_noun {
            if mp(current, &t.nom)
                || (cur > 1
                    && !mp(current, &t.nom)
                    && mp(current, &t.adjectiu)
                    && mp(token_at(cur - 1), &t.det))
            {
                if mp(current, &t.gn_ms_tag) {
                    lv.nms[level] += 1;
                    can_be_ms = true;
                }
                if mp(current, &t.gn_fs_tag) {
                    lv.nfs[level] += 1;
                    can_be_fs = true;
                }
                if mp(current, &t.gn_mp_tag) {
                    lv.nmp[level] += 1;
                    can_be_mp = true;
                }
                if mp(current, &t.gn_fp_tag) {
                    lv.nfp[level] += 1;
                    can_be_fp = true;
                }
            }
            if !mp(current, &t.gn_any) {
                if mp(current, &t.nom_ms) {
                    lv.nms[level] += 1;
                    can_be_ms = true;
                } else if mp(current, &t.nom_fs) {
                    lv.nfs[level] += 1;
                    can_be_fs = true;
                } else if mp(current, &t.nom_mp) {
                    lv.nmp[level] += 1;
                    can_be_mp = true;
                } else if mp(current, &t.nom_mn) {
                    lv.nmn[level] += 1;
                    can_be_ms = true;
                    can_be_mp = true;
                } else if mp(current, &t.nom_fp) {
                    lv.nfp[level] += 1;
                    can_be_fp = true;
                } else if mp(current, &t.nom_cs) {
                    lv.ncs[level] += 1;
                    can_be_ms = true;
                    can_be_fs = true;
                } else if mp(current, &t.nom_cp) {
                    lv.ncp[level] += 1;
                    can_be_fp = true;
                    can_be_mp = true;
                }
            }
        }
        if mp(current, &t.nom) {
            lv.nt[level] += 1;
            is_prev_noun = true;
        } else {
            is_prev_noun = false;
        }
        if mp(current, &t.det_cs) {
            if mp(token_at(cur + 1), &t.nom_ms) {
                lv.dms[level] += 1;
                can_be_ms = true;
            }
            if mp(token_at(cur + 1), &t.nom_fs) {
                lv.dfs[level] += 1;
                can_be_fs = true;
            }
        }
        if !mp(current, &t.adverbi) {
            if mp(current, &t.det_ms) {
                lv.dms[level] += 1;
                can_be_ms = true;
            }
            if mp(current, &t.det_fs) {
                lv.dfs[level] += 1;
                can_be_fs = true;
            }
            if mp(current, &t.det_mp) {
                lv.dmp[level] += 1;
                can_be_mp = true;
            }
            if mp(current, &t.det_fp) {
                lv.dfp[level] += 1;
                can_be_fp = true;
            }
        }
        if i > j {
            let text = current.map(|c| c.token().token()).unwrap_or_default();
            if mr(text, &t.prepo_nivell)
                && !mr(
                    token_at(cur - 1)
                        .map(|c| c.token().token())
                        .unwrap_or_default(),
                    &t.coord_ioni,
                )
                && !mp(token_at(cur + 1), &t.adverbi)
            {
                level += 1;
            }
        }
        if level > 0
            && mr(
                token_at(cur).map(|c| c.token().token()).unwrap_or_default(),
                &t.coord_ioni,
            )
        {
            let mut k = 1usize;
            while k < 4 && i > j + k {
                let back = token_at(i - j - k);
                let back_text = back.map(|c| c.token().token()).unwrap_or_default();
                if !((mp(back, &t.keep_count)
                    || mr(back_text, &t.keep_count2)
                    || mp(back, &t.adverbis_acceptats))
                    && !mr(back_text, &t.stop_count))
                {
                    break;
                }
                if mp(back, &t.preposicions) {
                    j += k;
                    break;
                }
                k += 1;
            }
        }
        if let Some(current) = token_at(i - j) {
            update_apparitions_es(current, &mut apparitions, t);
        }
        j += 1;
    }

    level += 1;
    if level > MAX_LEVELS {
        level = MAX_LEVELS;
    }
    let mut c_n = [0u32; MAX_LEVELS];
    let mut c_d = [0u32; MAX_LEVELS];
    let mut c_ntotal = 0u32;
    let mut c_dtotal = 0u32;
    for jj in 0..level {
        c_n[jj] = lv.nms[jj]
            + lv.nfs[jj]
            + lv.nmp[jj]
            + lv.nfp[jj]
            + lv.ncs[jj]
            + lv.ncp[jj]
            + lv.nmn[jj];
        c_d[jj] = lv.dms[jj] + lv.dfs[jj] + lv.dmp[jj] + lv.dfp[jj];
        c_ntotal += c_n[jj];
        c_dtotal += c_d[jj];
        if mp(adjective, &t.adjectiu_mp)
            && (c_n[jj] > 1 || c_d[jj] > 1)
            && (lv.nms[jj]
                + lv.nmn[jj]
                + lv.nmp[jj]
                + lv.ncs[jj]
                + lv.ncp[jj]
                + lv.dms[jj]
                + lv.dmp[jj])
                > 0
            && lv.nfs[jj] + lv.nfp[jj] <= lv.nt[jj]
        {
            return None;
        }
        if mp(adjective, &t.adjectiu_fp)
            && (c_n[jj] > 1 || c_d[jj] > 1)
            && ((lv.nms[jj] + lv.nmp[jj] + lv.nmn[jj] + lv.dms[jj] + lv.dmp[jj]) == 0
                || (lv.nt[jj] > 0 && lv.nfs[jj] + lv.nfp[jj] >= lv.nt[jj]))
        {
            return None;
        }
        if c_n[jj] + c_d[jj] > 0 {
            is_plural = is_plural && c_d[jj] > 1;
            can_be_p = can_be_p || c_n[jj] > 1;
        }
    }
    is_plural = is_plural
        || (i > 2
            && lv.nmp[0] + lv.nfp[0] + lv.ncp[0] > 0
            && token_at(i - 2).map(|c| c.token().token()) == Some(","));

    if c_ntotal == 0 && c_dtotal == 0 {
        return None;
    }

    let (subst, adj, gn): (&Regex, &Regex, &Regex) = if mp(adjective, &t.adjectiu_cs) {
        (&t.gn_cs, &t.adjectiu_s, &t.gn_cs_tag)
    } else if mp(adjective, &t.adjectiu_cp) {
        (&t.gn_cp, &t.adjectiu_p, &t.gn_cp_tag)
    } else if mp(adjective, &t.adjectiu_ms) {
        (&t.gn_ms, &t.adjectiu_ms, &t.gn_ms_tag)
    } else if mp(adjective, &t.adjectiu_fs) {
        (&t.gn_fs, &t.adjectiu_fs, &t.gn_fs_tag)
    } else if mp(adjective, &t.adjectiu_mp) {
        (&t.gn_mp, &t.adjectiu_mp, &t.gn_mp_tag)
    } else if mp(adjective, &t.adjectiu_fp) {
        (&t.gn_fp, &t.adjectiu_fp, &t.gn_fp_tag)
    } else {
        return None;
    };

    let mut j = 1usize;
    let mut keep = true;
    while i > j && keep {
        let cur = i - j;
        let current = token_at(cur);
        if (mp(current, &t.nom_det) && mp(current, gn))
            || (!mp(current, &t.gn_any) && mp(current, subst))
        {
            return None;
        }
        keep = !mp(current, &t.nom_det);
        j += 1;
    }

    let prev = token_at(i - 1);
    let necessary = (mp(prev, &t.nom) && !mp(prev, subst))
        || (mp(prev, &t.adjectiu) && !mp(prev, gn))
        || (mp(prev, &t.adjectiu) && !mp(prev, adj))
        || (i > 2
            && mp(prev, &t.adverbis_acceptats)
            && !mp(token_at(i - 2), &t.verb)
            && !mp(token_at(i - 2), &t.preposicions))
        || (i > 3
            && mp(prev, &t.loc_adv)
            && mp(token_at(i - 2), &t.loc_adv)
            && !mp(token_at(i - 3), &t.verb)
            && !mp(token_at(i - 3), &t.preposicions));
    if !necessary {
        return None;
    }

    if !(is_plural && mp(adjective, &t.adjectiu_s)) {
        let mut j = 1usize;
        apparitions = Apparitions::default();
        while i > j && keep_counting_es(token_at(i - j), &apparitions, t) && (level > 1 || j < 4) {
            let cur = i - j;
            let current = token_at(cur);
            if (!mp(current, &t.gn_any) && mp(current, &t.nom_det) && mp(current, subst))
                || mp(current, gn)
            {
                return None;
            }
            if let Some(current) = current {
                update_apparitions_es(current, &mut apparitions, t);
            }
            j += 1;
        }
    }

    let Some(synth) = synthesizer else {
        return Some(rm);
    };
    let Some(adjective) = adjective else {
        return Some(rm);
    };
    let mut suggestions: Vec<String> = Vec::new();
    if let Some(lemma) = matching_reading(adjective, &t.adjectiu_cs).and_then(|r| r.lemma()) {
        suggestions.extend(synth.synthesize(lemma, &t.synth_cs));
    }
    if suggestions.is_empty()
        && let Some(lemma) = matching_reading(adjective, &t.adjectiu_cp).and_then(|r| r.lemma())
    {
        suggestions.extend(synth.synthesize(lemma, &t.synth_cp));
    }
    if suggestions.is_empty()
        && is_plural
        && let Some(lemma) = matching_reading(adjective, &t.adjectiu_p).and_then(|r| r.lemma())
    {
        suggestions.extend(synth.synthesize(lemma, &t.synth_common_plural));
    }
    if suggestions.is_empty()
        && let Some(at) = matching_reading(adjective, &t.adjectiu)
        && let Some(lemma) = at.lemma()
    {
        if can_be_ms && !is_plural {
            suggestions.extend(synth.synthesize(lemma, &t.synth_ms));
        }
        if can_be_fs && !is_plural {
            suggestions.extend(synth.synthesize(lemma, &t.synth_fs));
        }
        if can_be_mp {
            suggestions.extend(synth.synthesize(lemma, &t.synth_mp));
        }
        if can_be_fp {
            suggestions.extend(synth.synthesize(lemma, &t.synth_fp));
        }
        if can_be_ms && (is_plural || can_be_p) {
            suggestions.extend(synth.synthesize(lemma, &t.synth_mp));
        }
        if can_be_fs && !can_be_ms && (is_plural || can_be_p) {
            suggestions.extend(synth.synthesize(lemma, &t.synth_fp));
        }
    }
    let surface = adjective.token().token();
    let lower = surface.to_lowercase();
    if let Some(pos) = suggestions.iter().position(|s| *s == lower) {
        suggestions.remove(pos);
    }
    if suggestions.is_empty() {
        return None;
    }
    let definitive = if add_comma {
        rm.set_offset_length(rm.offset().saturating_sub(1), rm.length() + 1);
        let mut out = Vec::with_capacity(suggestions.len() + 1);
        out.push(format!(", {surface}"));
        out.extend(suggestions.iter().map(|s| format!(" {s}")));
        out
    } else {
        suggestions
    };
    rm.set_replacements(
        dedup_preserving_order(definitive)
            .into_iter()
            .map(SuggestedReplacement::new)
            .collect(),
    );
    Some(rm)
}

fn keep_counting_es(
    token: Option<&AnalyzedTokenReadings>,
    apparitions: &Apparitions,
    t: &TagSet,
) -> bool {
    let Some(a) = token else {
        return false;
    };
    let text = a.token().token();
    if (mp(Some(a), &t.puntuacio) || apparitions.conjunction || apparitions.adverb)
        && (apparitions.punctuation || apparitions.conjunction)
        && (apparitions.punctuation || apparitions.adverb)
    {
        return false;
    }
    (mp(Some(a), &t.keep_count) || mr(text, &t.keep_count2) || mp(Some(a), &t.adverbis_acceptats))
        && !mr(text, &t.stop_count)
        && (!mp(Some(a), &t.gv) || mp(Some(a), &t.gn_any))
}

fn update_apparitions_es(token: &AnalyzedTokenReadings, apparitions: &mut Apparitions, t: &TagSet) {
    if mp(Some(token), &t.nom) || mp(Some(token), &t.adjectiu) {
        *apparitions = Apparitions::default();
        return;
    }
    apparitions.adverb |= mp(Some(token), &t.adverbi);
    apparitions.conjunction |= mp(Some(token), &t.conjuncio);
    apparitions.punctuation |= mp(Some(token), &t.puntuacio) || token.token().token() == ",";
}

pub(crate) fn accept_fr(
    synthesizer: Option<&LangSynthesizer>,
    _args: &str,
    matched_positions: &[usize],
    tokens: &[&AnalyzedTokenReadings],
    mut rm: RuleMatch,
) -> Option<RuleMatch> {
    let t = fr_tags();
    let b = &t.base;
    let i = *matched_positions.first()?;
    let token_at = |idx: usize| tokens.get(idx).copied();
    let adjective = token_at(i);

    let mut is_plural = true;
    let mut is_prev_noun = false;
    let mut can_be_ms = false;
    let mut can_be_fs = false;
    let mut can_be_mp = false;
    let mut can_be_fp = false;
    let mut can_be_p = false;
    let mut lv = Levels::default();
    let mut apparitions = Apparitions::default();
    let mut level = 0usize;
    let mut j = 1usize;

    while i > j && keep_counting_fr(token_at(i - j), &apparitions, t) && level < MAX_LEVELS {
        let cur = i - j;
        let current = token_at(cur);
        if !is_prev_noun {
            if mp(current, &b.nom)
                || (cur > 1
                    && !mp(current, &b.nom)
                    && mp(current, &b.adjectiu)
                    && mp(token_at(cur - 1), &b.det))
            {
                if mp(current, &b.gn_ms_tag) {
                    lv.nms[level] += 1;
                    can_be_ms = true;
                }
                if mp(current, &b.gn_fs_tag) {
                    lv.nfs[level] += 1;
                    can_be_fs = true;
                }
                if mp(current, &b.gn_mp_tag) {
                    lv.nmp[level] += 1;
                    can_be_mp = true;
                }
                if mp(current, &b.gn_fp_tag) {
                    lv.nfp[level] += 1;
                    can_be_fp = true;
                }
            }
            if !mp(current, &b.gn_any) {
                if mp(current, &b.nom_ms) {
                    lv.nms[level] += 1;
                    can_be_ms = true;
                } else if mp(current, &b.nom_fs) {
                    lv.nfs[level] += 1;
                    can_be_fs = true;
                } else if mp(current, &b.nom_mp) {
                    lv.nmp[level] += 1;
                    can_be_mp = true;
                } else if mp(current, &b.nom_mn) {
                    lv.nmn[level] += 1;
                    can_be_ms = true;
                    can_be_mp = true;
                } else if mp(current, &b.nom_fp) {
                    lv.nfp[level] += 1;
                    can_be_fp = true;
                } else if mp(current, &b.nom_cs) {
                    lv.ncs[level] += 1;
                    can_be_ms = true;
                    can_be_fs = true;
                } else if mp(current, &b.nom_cp) {
                    lv.ncp[level] += 1;
                    can_be_fp = true;
                    can_be_mp = true;
                }
            }
        }
        if mp(current, &b.nom) {
            lv.nt[level] += 1;
            is_prev_noun = true;
        } else {
            is_prev_noun = false;
        }
        if mp(current, &b.det_cs) {
            if mp(token_at(cur + 1), &b.nom_ms) {
                lv.dms[level] += 1;
                can_be_ms = true;
            }
            if mp(token_at(cur + 1), &b.nom_fs) {
                lv.dfs[level] += 1;
                can_be_fs = true;
            }
        }
        if mp(current, &t.det_cp) {
            if mp(token_at(cur + 1), &b.nom_mp) {
                lv.dms[level] += 1;
                can_be_mp = true;
            }
            if mp(token_at(cur + 1), &b.nom_fp) {
                lv.dfs[level] += 1;
                can_be_fp = true;
            }
        }
        if !mp(current, &b.adverbi) {
            if mp(current, &b.det_ms) {
                lv.dms[level] += 1;
                can_be_ms = true;
            }
            if mp(current, &b.det_fs) {
                lv.dfs[level] += 1;
                can_be_fs = true;
            }
            if mp(current, &b.det_mp) {
                lv.dmp[level] += 1;
                can_be_mp = true;
            }
            if mp(current, &b.det_fp) {
                lv.dfp[level] += 1;
                can_be_fp = true;
            }
        }
        if i > j + 1 {
            let text = current.map(|c| c.token().token()).unwrap_or_default();
            if (mr(text, &b.prepo_nivell)
                && mp(current, &b.preposicions)
                && !mp(current, &b.conjuncio)
                && !mr(
                    token_at(cur - 1)
                        .map(|c| c.token().token())
                        .unwrap_or_default(),
                    &b.coord_ioni,
                )
                && !mp(token_at(cur + 1), &b.adverbi))
                || (text.eq_ignore_ascii_case("d'")
                    && token_at(cur + 1)
                        .map(|c| c.token().token().eq_ignore_ascii_case("environ"))
                        .unwrap_or(false))
            {
                level += 1;
            }
        }
        j = update_j_value_fr(tokens, i, j, t);
        if let Some(current) = token_at(i - j) {
            update_apparitions_fr(current, &mut apparitions, t);
        }
        j += 1;
    }

    level += 1;
    if level > MAX_LEVELS {
        level = MAX_LEVELS;
    }
    let mut c_n = [0u32; MAX_LEVELS];
    let mut c_d = [0u32; MAX_LEVELS];
    let mut c_ntotal = 0u32;
    let mut c_dtotal = 0u32;
    for jj in 0..level {
        c_n[jj] = lv.nms[jj]
            + lv.nfs[jj]
            + lv.nmp[jj]
            + lv.nfp[jj]
            + lv.ncs[jj]
            + lv.ncp[jj]
            + lv.nmn[jj];
        c_d[jj] = lv.dms[jj] + lv.dfs[jj] + lv.dmp[jj] + lv.dfp[jj];
        c_ntotal += c_n[jj];
        c_dtotal += c_d[jj];
        if mp(adjective, &b.adjectiu_mp)
            && (c_n[jj] > 1 || c_d[jj] > 1)
            && (lv.nms[jj]
                + lv.nmn[jj]
                + lv.nmp[jj]
                + lv.ncs[jj]
                + lv.ncp[jj]
                + lv.dms[jj]
                + lv.dmp[jj])
                > 0
            && lv.nfs[jj] + lv.nfp[jj] <= lv.nt[jj]
        {
            return None;
        }
        if mp(adjective, &b.adjectiu_fp)
            && (c_n[jj] > 1 || c_d[jj] > 1)
            && ((lv.nms[jj] + lv.nmp[jj] + lv.nmn[jj] + lv.dms[jj] + lv.dmp[jj]) == 0
                || (lv.nt[jj] > 0 && lv.nfs[jj] + lv.nfp[jj] >= lv.nt[jj]))
        {
            return None;
        }
        if c_n[jj] + c_d[jj] > 0 {
            is_plural = is_plural && c_d[jj] > 1 && level > 1;
            can_be_p = can_be_p || c_n[jj] > 1;
        }
    }
    is_plural = is_plural
        || (i > 2
            && lv.nmp[0] + lv.nfp[0] + lv.ncp[0] > 0
            && token_at(i - 2).map(|c| c.token().token()) == Some(","));

    if c_ntotal == 0 && c_dtotal == 0 {
        return None;
    }

    let (subst, adj, gn): (&Regex, &Regex, &Regex) = if mp(adjective, &b.adjectiu_cs) {
        (&b.gn_cs, &b.adjectiu_s, &b.gn_cs_tag)
    } else if mp(adjective, &b.adjectiu_cp) {
        (&b.gn_cp, &b.adjectiu_p, &b.gn_cp_tag)
    } else if mp(adjective, &t.adjectiu_mn) {
        (&t.gn_mn, &t.adjectiu_m, &t.gn_mn_tag)
    } else if mp(adjective, &t.adjectiu_fn) {
        (&t.gn_fn, &t.adjectiu_fn, &t.gn_fn_tag)
    } else if mp(adjective, &b.adjectiu_ms) {
        (&b.gn_ms, &b.adjectiu_ms, &b.gn_ms_tag)
    } else if mp(adjective, &b.adjectiu_fs) {
        (&b.gn_fs, &b.adjectiu_fs, &b.gn_fs_tag)
    } else if mp(adjective, &b.adjectiu_mp) {
        (&b.gn_mp, &b.adjectiu_mp, &b.gn_mp_tag)
    } else if mp(adjective, &b.adjectiu_fp) {
        (&b.gn_fp, &b.adjectiu_fp, &b.gn_fp_tag)
    } else {
        return None;
    };

    let mut j = 1usize;
    let mut keep = true;
    while i > j && keep {
        let cur = i - j;
        let current = token_at(cur);
        if (mp(current, &b.nom_det) && mp(current, gn))
            || (!mp(current, &b.gn_any) && mp(current, subst))
        {
            return None;
        }
        keep = !mp(current, &b.nom_det);
        j += 1;
    }

    let prev = token_at(i - 1);
    let necessary = (mp(prev, &b.nom) && !mp(prev, subst))
        || (mp(prev, &b.gn_any) && !mp(prev, gn))
        || (mp(prev, &b.adjectiu) && !mp(prev, adj))
        || (i > 2
            && mp(prev, &b.adverbis_acceptats)
            && !mp(token_at(i - 2), &b.verb)
            && !mp(token_at(i - 2), &b.preposicions))
        || (i > 3
            && mp(prev, &b.loc_adv)
            && mp(token_at(i - 2), &b.loc_adv)
            && !mp(token_at(i - 3), &b.verb)
            && !mp(token_at(i - 3), &b.preposicions));
    if !necessary {
        return None;
    }

    if !(is_plural && mp(adjective, &b.adjectiu_s)) {
        let mut j = 1usize;
        apparitions = Apparitions::default();
        while i > j && keep_counting_fr(token_at(i - j), &apparitions, t) && (level > 1 || j < 4) {
            let cur = i - j;
            let current = token_at(cur);
            if (!mp(current, &b.gn_any) && mp(current, &b.nom_det) && mp(current, subst))
                || mp(current, gn)
            {
                return None;
            }
            j = update_j_value_fr(tokens, i, j, t);
            if let Some(current) = token_at(i - j) {
                update_apparitions_fr(current, &mut apparitions, t);
            }
            j += 1;
        }
    }

    let Some(synth) = synthesizer else {
        return Some(rm);
    };
    let adjective = adjective?;
    let mut suggestions: Vec<String> = Vec::new();
    if let Some(lemma) = matching_reading(adjective, &b.adjectiu_cs).and_then(|r| r.lemma()) {
        suggestions.extend(synth.synthesize(lemma, &b.synth_cs));
    }
    if suggestions.is_empty()
        && let Some(lemma) = matching_reading(adjective, &b.adjectiu_cp).and_then(|r| r.lemma())
    {
        suggestions.extend(synth.synthesize(lemma, &b.synth_cp));
    }
    if suggestions.is_empty()
        && is_plural
        && let Some(lemma) = matching_reading(adjective, &b.adjectiu_p).and_then(|r| r.lemma())
    {
        suggestions.extend(synth.synthesize(lemma, &b.synth_common_plural));
    }
    if suggestions.is_empty()
        && let Some(at) = matching_reading(adjective, &b.adjectiu)
        && let Some(lemma) = at.lemma()
    {
        if can_be_ms && !is_plural {
            suggestions.extend(synth.synthesize(lemma, &b.synth_ms));
        }
        if can_be_fs && !is_plural {
            suggestions.extend(synth.synthesize(lemma, &b.synth_fs));
        }
        if can_be_mp {
            suggestions.extend(synth.synthesize(lemma, &b.synth_mp));
        }
        if can_be_fp {
            suggestions.extend(synth.synthesize(lemma, &b.synth_fp));
        }
        if can_be_ms && (is_plural || can_be_p) {
            suggestions.extend(synth.synthesize(lemma, &b.synth_mp));
        }
        if can_be_fs && !can_be_ms && (is_plural || can_be_p) {
            suggestions.extend(synth.synthesize(lemma, &b.synth_fp));
        }
    }
    suggestions = dedup_preserving_order(suggestions);
    let lower = adjective.token().token().to_lowercase();
    if let Some(pos) = suggestions.iter().position(|s| *s == lower) {
        suggestions.remove(pos);
    }
    if suggestions.is_empty() {
        return None;
    }
    rm.set_replacements(
        suggestions
            .into_iter()
            .map(SuggestedReplacement::new)
            .collect(),
    );
    Some(rm)
}

fn update_j_value_fr(tokens: &[&AnalyzedTokenReadings], i: usize, j: usize, t: &FrTagSet) -> usize {
    let cur = i - j;
    let Some(current) = tokens.get(cur).copied() else {
        return j;
    };
    if mr(current.token().token(), &t.base.coord_ioni)
        && cur > 1
        && cur + 1 < tokens.len()
        && mp(tokens.get(cur - 1).copied(), &t.base.det)
        && tokens[cur + 1].token().token() == "plus"
    {
        return j + 1;
    }
    j
}

fn keep_counting_fr(
    token: Option<&AnalyzedTokenReadings>,
    apparitions: &Apparitions,
    t: &FrTagSet,
) -> bool {
    let Some(a) = token else {
        return false;
    };
    let b = &t.base;
    let text = a.token().token();
    if mr(text, &b.prepo_nivell) {
        return true;
    }
    if text == "." {
        return true;
    }
    if ((mp(Some(a), &b.puntuacio) || apparitions.conjunction || apparitions.adverb)
        && (apparitions.punctuation || apparitions.conjunction)
        && (apparitions.punctuation || apparitions.adverb))
        || (apparitions.infinitive && (mr(text, &b.coord_ioni) || apparitions.adverb))
    {
        return false;
    }
    (mp(Some(a), &b.keep_count) || mr(text, &b.keep_count2) || mp(Some(a), &b.adverbis_acceptats))
        && !mr(text, &b.stop_count)
        && (!mp(Some(a), &b.gv) || mp(Some(a), &b.gn_any))
}

fn update_apparitions_fr(
    token: &AnalyzedTokenReadings,
    apparitions: &mut Apparitions,
    t: &FrTagSet,
) {
    let b = &t.base;
    apparitions.conjunction |= mp(Some(token), &b.conjuncio);
    if token.token().token() == "com" {
        return;
    }
    if mp(Some(token), &b.nom) || mp(Some(token), &b.adjectiu) {
        *apparitions = Apparitions::default();
        return;
    }
    apparitions.adverb |= mp(Some(token), &b.adverbi);
    apparitions.punctuation |= mp(Some(token), &b.puntuacio) || token.token().token() == ",";
    apparitions.infinitive |= mp(Some(token), &t.infinitive);
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{RuleMatchContext, RuleMatchRule};

    fn pl_synth() -> Option<LangSynthesizer> {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/pl/src/main/resources/org/languagetool/resource/pl",
        );
        LangSynthesizer::for_language(dir, "pl")
    }

    #[test]
    fn pl_synthesizes_forms_from_polish_synth_dict() {
        let Some(synth) = pl_synth() else {
            return;
        };
        assert_eq!(
            synth.synthesize_exact_for("zrobić", None, "ger:sg:nom.acc:n2:perf:aff:refl.nonrefl"),
            vec!["zrobienie".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for(
                "zrobić",
                Some("ger:sg:nom.acc:n2:perf:neg:refl.nonrefl"),
                "ger:sg:nom.acc:n2:perf:neg:refl.nonrefl"
            ),
            vec!["niezrobienie".to_string()]
        );
        assert_eq!(
            synth.synthesize_full_for("zrobić", None, "ger:sg:nom\\.acc:n2:perf:aff:refl.nonrefl"),
            vec!["zrobienie".to_string()]
        );
        assert_eq!(
            synth.synthesize_full_for(
                "zrobić",
                Some("ger:sg:nom.acc:n2:perf:neg:refl.nonrefl"),
                "ger:sg:nom\\.acc:n2:perf:neg:refl.nonrefl"
            ),
            vec!["niezrobienie".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("dobry", None, "adj:sg:acc:m3:com"),
            vec!["lepszy".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for(
                "dobry",
                Some("adj:sg:nom:m3:pos"),
                "adj:sg:nom:m3:pos+adj:sg:acc:m3:pos"
            ),
            vec!["dobry".to_string()]
        );
    }

    fn tag(text: &str, tags: &[&str]) -> AnalyzedTokenReadings {
        let readings = if tags.is_empty() {
            vec![AnalyzedToken::new(text, 0, text.len())]
        } else {
            tags.iter()
                .map(|t| AnalyzedToken::new(text, 0, text.len()).with_pos_tags(vec![t.to_string()]))
                .collect()
        };
        AnalyzedTokenReadings::new(AnalyzedToken::new(text, 0, text.len())).with_readings(readings)
    }

    #[test]
    fn es_patterns_compile_and_match() {
        let t = es_tags();
        assert!(mp(Some(&tag("casa", &["NCFS000"])), &t.nom));
        assert!(mp(Some(&tag("casa", &["NCFS000"])), &t.nom_fs));
        assert!(mp(Some(&tag("blancas", &["AQ0FP0"])), &t.adjectiu_fp));
        assert!(mp(Some(&tag("del", &["SP+DA"])), &t.preposicions));
        assert!(mp(Some(&tag("x", &[])), &t.keep_count));
        assert!(mr("del", &t.prepo_nivell));
        assert!(!mr("el", &t.prepo_nivell));
        assert!(mr("ni", &t.coord_ioni));
        assert!(mp(Some(&tag("x", &["_GN_MS"])), &t.gn_any));
    }

    #[test]
    fn fr_patterns_compile_and_match() {
        let t = fr_tags();
        let b = &t.base;
        assert!(mp(Some(&tag("maison", &["N f s"])), &b.nom));
        assert!(mp(Some(&tag("jolies", &["J f p"])), &b.adjectiu));
        assert!(mp(Some(&tag("jolies", &["J f p"])), &b.adjectiu_fp));
        assert!(mp(Some(&tag("joli", &["J m sp"])), &t.adjectiu_mn));
        assert!(mp(Some(&tag("les", &["D e p"])), &t.det_cp));
        assert!(mp(Some(&tag("manger", &["V inf"])), &t.infinitive));
        assert!(mp(Some(&tag("manger", &["V inf"])), &b.verb));
        assert!(!mp(Some(&tag("joli", &["V ppa m s"])), &b.verb));
        assert!(mr("et", &b.coord_ioni));
        assert!(mr("d'", &b.prepo_nivell));
        assert!(mr(";", &b.stop_count));
        assert!(mr(":", &b.stop_count));
        assert!(mr("–", &b.stop_count));
        assert!(!mr(".", &b.stop_count));
    }

    #[test]
    fn fr_exception_filtering() {
        assert!(is_exception_fr("qqch"));
        assert!(is_exception_fr("manifestè"));
        assert!(!is_exception_fr("burkinabè"));
        assert!(!is_exception_fr("épistémè"));
        assert!(!is_exception_fr("jolis"));
    }

    #[test]
    fn manual_parsing_supports_declared_separator() {
        let entries = parse_manual("#separatorRegExp=;\nQuick;Quick;Z m sp\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0.as_ref(), "Quick");
        assert_eq!(entries[0].1.as_ref(), "Z m sp");
        assert_eq!(entries[0].2.as_ref(), "Quick");
    }

    #[test]
    fn arg_parsing_requires_exact_key() {
        assert!(arg_is_true("addComma:true", "addComma"));
        assert!(arg_is_true("key:value addComma:TRUE", "addComma"));
        assert!(!arg_is_true("key:value", "addComma"));
        assert!(!arg_is_true("addComma:false", "addComma"));
    }

    #[test]
    fn dedup_keeps_first_occurrence_order() {
        let out = dedup_preserving_order(vec!["a".into(), "b".into(), "a".into()]);
        assert_eq!(out, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn ro_synthesizes_forms_from_romanian_synth_dict() {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ro/src/main/resources/org/languagetool/resource/ro",
        );
        let Some(synth) = LangSynthesizer::for_language(dir, "ro") else {
            return;
        };
        assert_eq!(
            synth.synthesize_exact_for("alege", None, "V000000f00"),
            vec!["alege".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("alege", None, "V0p2000cz0"),
            vec!["alegeți".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("fi", None, "V000000f0f"),
            vec!["fi".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("fi", None, "V0p2000izf"),
            vec!["sunteți".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("fi", None, "V0p3000izf"),
            vec!["sunt".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("merge", None, "V0p1000im0"),
            vec!["merseserăm".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("merge", None, "V0s1000im0"),
            vec!["mersesem".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("legătură", None, "Sfs3aac000"),
            vec!["legătura".to_string()]
        );
    }

    #[test]
    fn ro_synthesizes_regex_tags_over_possible_tags() {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ro/src/main/resources/org/languagetool/resource/ro",
        );
        let Some(synth) = LangSynthesizer::for_language(dir, "ro") else {
            return;
        };
        let re = Regex::new("V0p2000izf|V0p3000izf").unwrap();
        assert_eq!(
            synth.synthesize("fi", &re),
            vec!["sunteți".to_string(), "sunt".to_string()]
        );
    }

    #[test]
    fn gl_synthesizes_forms_from_galician_synth_dict() {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/gl/src/main/resources/org/languagetool/resource/gl",
        );
        let Some(synth) = LangSynthesizer::for_language(dir, "gl") else {
            return;
        };
        assert_eq!(
            synth.synthesize_exact_for("cantar", None, "VMIP1S0"),
            vec!["canto".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("cantar", None, "VMIP3S0"),
            vec!["canta".to_string()]
        );
    }

    #[test]
    fn sk_synthesizes_forms_from_slovak_synth_dict() {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/sk/src/main/resources/org/languagetool/resource/sk",
        );
        let Some(synth) = LangSynthesizer::for_language(dir, "sk") else {
            return;
        };
        assert_eq!(
            synth.synthesize_exact_for("blablabla", None, "blablabla"),
            Vec::<String>::new()
        );
        assert_eq!(
            synth.synthesize_exact_for("časopis", None, "SSis2"),
            vec!["časopisu".to_string()]
        );
        let re = Regex::new("SS.*").unwrap();
        assert_eq!(
            synth.synthesize("časopis", &re),
            vec![
                "časopisy".to_string(),
                "časopisov".to_string(),
                "časopisom".to_string(),
                "časopisy".to_string(),
                "časopisy".to_string(),
                "časopisoch".to_string(),
                "časopismi".to_string(),
                "časopis".to_string(),
                "časopisu".to_string(),
                "časopisu".to_string(),
                "časopis".to_string(),
                "časopis".to_string(),
                "časopise".to_string(),
                "časopisom".to_string(),
            ]
        );
    }

    #[test]
    fn it_synthesizes_forms_from_italian_synth_dict() {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/it/src/main/resources/org/languagetool/resource/it",
        );
        let Some(synth) = LangSynthesizer::for_language(dir, "it") else {
            return;
        };
        assert_eq!(
            synth.synthesize_exact_for("blablabla", None, "blablabla"),
            Vec::<String>::new()
        );
        assert_eq!(
            synth.synthesize_exact_for("richiedere", None, "VER:impr+pres+2+s"),
            vec!["richiedi".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("esultare", None, "VER:part+past+s+f"),
            vec!["esultata".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("libro", None, "NOUN-M:p"),
            vec!["libri".to_string()]
        );
    }

    #[test]
    fn sv_synthesizes_forms_from_swedish_synth_dict() {
        let dir = std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/sv/src/main/resources/org/languagetool/resource/sv",
        );
        let Some(synth) = LangSynthesizer::for_language(dir, "sv") else {
            return;
        };
        assert_eq!(
            synth.synthesize_exact_for("blablabla", None, "blablabla"),
            Vec::<String>::new()
        );
        assert_eq!(
            synth.synthesize_exact_for("nyckel", None, "NN:BF:SIN:GEN:UTR"),
            vec!["nyckelns".to_string()]
        );
        assert_eq!(
            synth.synthesize_exact_for("FN", None, "PM:GEN:ACR"),
            vec!["FN:s".to_string()]
        );
        let re = Regex::new("JJ:S:BF:.*").unwrap();
        assert_eq!(
            synth.synthesize("stor", &re),
            vec!["störste".to_string(), "största".to_string()]
        );
    }
    fn lang_dir(module: &str) -> std::path::PathBuf {
        std::path::Path::new(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules",
        )
        .join(module)
        .join("src/main/resources/org/languagetool/resource")
        .join(module)
    }

    fn tagged(text: &str, lemma: &str, tags: &[&str]) -> AnalyzedTokenReadings {
        let readings = tags
            .iter()
            .map(|t| {
                AnalyzedToken::new(text, 0, text.len())
                    .with_lemma(lemma)
                    .with_pos_tags(vec![t.to_string()])
            })
            .collect();
        AnalyzedTokenReadings::new(AnalyzedToken::new(text, 0, text.len())).with_readings(readings)
    }

    fn rm_for(text: &str) -> RuleMatch {
        RuleMatch::new(
            "m",
            0,
            4,
            RuleMatchRule::new("X", "d"),
            RuleMatchContext::new(text.to_string(), 0, 4),
        )
    }

    fn values_of(rm: &RuleMatch) -> Vec<String> {
        rm.replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect()
    }

    #[test]
    fn es_postponed_adjective_concordance_matches_java_examples() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let pueblo = vec![
            tagged("La", "el", &["DA0FS0"]),
            tagged("casa", "casa", &["NCFS000"]),
            tagged("del", "del", &["SPS00"]),
            tagged("pueblo", "pueblo", &["NCMS000"]),
            tagged("blancas", "blanco", &["AQ0FP0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = pueblo.iter().collect();
        let out = accept_es(Some(&synth), "key:value", &[4], &refs, rm_for("x")).expect("kept");
        assert_eq!(
            values_of(&out),
            vec!["blanco".to_string(), "blanca".to_string()]
        );
        let casas = vec![
            tagged("Son", "ser", &["VMIP3P0"]),
            tagged("casas", "casa", &["NCFP000"]),
            tagged("rojos", "rojo", &["AQ0MP0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = casas.iter().collect();
        let out = accept_es(Some(&synth), "key:value", &[2], &refs, rm_for("x")).expect("kept");
        assert_eq!(values_of(&out), vec!["rojas".to_string()]);
        let lujosa = vec![
            tagged("en", "en", &["SPS00"]),
            tagged("un", "uno", &["DI0MS0"]),
            tagged("restaurante", "restaurante", &["NCMS000"]),
            tagged("lujosa", "lujoso", &["AQ0FS0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = lujosa.iter().collect();
        let out = accept_es(Some(&synth), "key:value", &[3], &refs, rm_for("x")).expect("kept");
        assert_eq!(values_of(&out), vec!["lujoso".to_string()]);
    }

    #[test]
    fn es_add_comma_prepends_the_comma_suggestion() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let tokens = vec![
            tagged("Disculpe", "disculpar", &["VMSP3S0"]),
            tagged("el", "el", &["DA0MS0"]),
            tagged("retraso", "retraso", &["NCMS000"]),
            tagged("maestra", "maestro", &["AQ0FS0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let mut rm = rm_for("Disculpe el retraso maestra");
        rm.set_replacements(vec![SuggestedReplacement::new("{suggestion}")]);
        let out = accept_es(Some(&synth), "addComma:true", &[3], &refs, rm).expect("kept");
        assert_eq!(
            values_of(&out),
            vec![", maestra".to_string(), " maestro".to_string()]
        );
    }

    #[test]
    fn es_without_positions_or_synthesizer_keeps_or_drops_the_match() {
        let tokens = vec![tagged("casa", "casa", &["NCFS000"])];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        assert!(accept_es(None, "key:value", &[], &refs, rm_for("x")).is_none());
        assert!(accept_es(None, "key:value", &[0], &refs, rm_for("x")).is_none());
        let lujosa = vec![
            tagged("en", "en", &["SPS00"]),
            tagged("un", "uno", &["DI0MS0"]),
            tagged("restaurante", "restaurante", &["NCMS000"]),
            tagged("lujosa", "lujoso", &["AQ0FS0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = lujosa.iter().collect();
        let out = accept_es(None, "key:value", &[3], &refs, rm_for("x"))
            .expect("kept unchanged without a synthesizer");
        assert!(out.replacements().is_empty());
    }

    #[test]
    fn fr_postponed_adjective_concordance_matches_java_examples() {
        let dir = lang_dir("fr");
        let Some(synth) = LangSynthesizer::for_language(&dir, "fr") else {
            return;
        };
        let maison = vec![
            tagged("la", "le", &["D f s"]),
            tagged("maison", "maison", &["N f s"]),
            tagged("bleu", "bleu", &["J m s"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = maison.iter().collect();
        let out = accept_fr(Some(&synth), "key:value", &[2], &refs, rm_for("x")).expect("kept");
        assert_eq!(values_of(&out), vec!["bleue".to_string()]);
        assert!(accept_fr(Some(&synth), "key:value", &[], &refs, rm_for("x")).is_none());
    }

    #[test]
    fn lang_synthesizer_covers_german_and_manual_paths() {
        let dir = lang_dir("de");
        let Some(synth) = LangSynthesizer::for_language(&dir, "de") else {
            return;
        };
        let plural = Regex::new("^(?:SUB:AKK:PLU:NEU)$").unwrap();
        assert!(
            synth
                .synthesize("Haus", &plural)
                .contains(&"Häuser".to_string())
        );
        let exact = synth.synthesize_exact_for("Haus", None, "SUB:AKK:PLU:NEU");
        assert_eq!(exact, vec!["Häuser".to_string()]);
        let compound = synth.synthesize_exact_for("Bundesautobahn", None, "SUB:NOM:SIN:FEM");
        assert_eq!(
            compound,
            vec!["Bundesautobahn".to_string()],
            "compound fallback recombines the split parts"
        );
        assert!(synth.target_postag(&[]).is_none());
        let last = synth.target_postag(&["A", "B"]);
        assert_eq!(last, Some("B"));
        assert!(!synth.is_determiner_tag("DT"));
        assert!(synth.determiner_forms("house", "+INDT").len() == 1);
        let es = LangSynthesizer::for_language(&lang_dir("es"), "es").expect("es synthesizer");
        assert_eq!(
            es.target_postag(&["VMIP3S0", "VMM02S0"]),
            Some("VMIP3S0"),
            "spanish keeps the indicative reading for imperatives"
        );
    }

    #[test]
    fn lang_synthesizer_english_uses_the_decoded_dict() {
        let dir = lang_dir("en");
        let Some(synth) = LangSynthesizer::for_language(&dir, "en") else {
            return;
        };
        assert!(synth.is_determiner_tag("+DT"));
        let forms = synth.determiner_forms("university", "+DT");
        assert!(forms.contains(&"a university".to_string()), "{forms:?}");
        let re = Regex::new("^(?:VBD)$").unwrap();
        let went = synth.synthesize("go", &re);
        assert!(went.contains(&"went".to_string()), "{went:?}");
        assert!(
            !went.iter().any(|f| f.starts_with('\'')),
            "exceptions filtered"
        );
    }

    #[test]
    fn spanish_lemma_split_handles_clitic_leading_verbs() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let forms = synth.synthesize_exact_for("dar lo", None, "VMIP3S0");
        assert!(
            forms.iter().all(|f| f.ends_with(" lo")),
            "tail is appended: {forms:?}"
        );
    }

    #[test]
    fn es_concordance_walk_counts_determiners_and_common_nouns() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let cases: Vec<(Vec<AnalyzedTokenReadings>, usize, Option<Vec<&str>>)> = vec![
            (
                vec![
                    tagged("un", "uno", &["DD0CS0"]),
                    tagged("libro", "libro", &["NCMS000"]),
                    tagged("blanca", "blanco", &["AQ0FS0"]),
                ],
                2,
                Some(vec!["blanco"]),
            ),
            (
                vec![
                    tagged("la", "el", &["DD0CS0"]),
                    tagged("casa", "casa", &["NCFS000"]),
                    tagged("blanco", "blanco", &["AQ0MS0"]),
                ],
                2,
                Some(vec!["blanca"]),
            ),
            (
                vec![
                    tagged("los", "el", &["DD0MP0"]),
                    tagged("libros", "libro", &["NCMP000"]),
                    tagged("blanca", "blanco", &["AQ0FS0"]),
                ],
                2,
                Some(vec!["blancos"]),
            ),
            (
                vec![
                    tagged("las", "el", &["DD0FP0"]),
                    tagged("casas", "casa", &["NCFP000"]),
                    tagged("blanco", "blanco", &["AQ0MS0"]),
                ],
                2,
                Some(vec!["blancas"]),
            ),
            (
                vec![
                    tagged("el", "el", &["UNKNOWN"]),
                    tagged("agua", "agua", &["NCMN000"]),
                    tagged("blanca", "blanco", &["AQ0FS0"]),
                ],
                2,
                Some(vec!["blanco", "blancos"]),
            ),
            (
                vec![
                    tagged("el", "el", &["UNKNOWN"]),
                    tagged("arte", "arte", &["NCCS000"]),
                    tagged("blancos", "blanco", &["AQ0MP0"]),
                ],
                2,
                Some(vec!["blanco", "blanca"]),
            ),
            (
                vec![
                    tagged("el", "el", &["UNKNOWN"]),
                    tagged("meta", "meta", &["NCCP000"]),
                    tagged("blanco", "blanco", &["AQ0MS0"]),
                ],
                2,
                Some(vec!["blancos", "blancas"]),
            ),
            (
                vec![
                    tagged("x", "x", &["UNKNOWN"]),
                    tagged("casas", "casa", &["NCFP000"]),
                    tagged("de", "de", &["SPS00"]),
                    tagged("libro", "libro", &["NCMS000"]),
                    tagged("blanca", "blanco", &["AQ0FS0"]),
                ],
                4,
                Some(vec!["blanco", "blancas"]),
            ),
        ];
        for (tokens, adj_pos, expected) in cases {
            let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
            let out = accept_es(Some(&synth), "key:value", &[adj_pos], &refs, rm_for("x"));
            match (out, expected) {
                (Some(rm), Some(want)) => assert_eq!(values_of(&rm), want),
                (None, None) => {}
                (got, want) => panic!("mismatch: got {got:?} want {want:?}"),
            }
        }
    }

    #[test]
    fn es_concordance_walk_uses_gendered_determiners_and_levels() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let cases: Vec<(Vec<AnalyzedTokenReadings>, usize, Option<Vec<&str>>)> = vec![
            (
                vec![
                    tagged("x", "x", &["UNKNOWN"]),
                    tagged("un", "uno", &["DD0CS0"]),
                    tagged("libro", "libro", &["NCMS000"]),
                    tagged("blanca", "blanco", &["AQ0FS0"]),
                ],
                3,
                Some(vec!["blanco"]),
            ),
            (
                vec![
                    tagged("x", "x", &["UNKNOWN"]),
                    tagged("una", "uno", &["DD0CS0"]),
                    tagged("casa", "casa", &["NCFS000"]),
                    tagged("blanco", "blanco", &["AQ0MS0"]),
                ],
                3,
                Some(vec!["blanca"]),
            ),
            (
                vec![
                    tagged("x", "x", &["UNKNOWN"]),
                    tagged("el", "el", &["DD0MS0"]),
                    tagged("libro", "libro", &["NCMS000"]),
                    tagged("blanca", "blanco", &["AQ0FS0"]),
                ],
                3,
                Some(vec!["blanco"]),
            ),
            (
                vec![
                    tagged("x", "x", &["UNKNOWN"]),
                    tagged("la", "el", &["DD0FS0"]),
                    tagged("casa", "casa", &["NCFS000"]),
                    tagged("blanco", "blanco", &["AQ0MS0"]),
                ],
                3,
                Some(vec!["blanca"]),
            ),
            (
                vec![
                    tagged("x", "x", &["UNKNOWN"]),
                    tagged("los", "el", &["DD0MP0"]),
                    tagged("libros", "libro", &["NCMP000"]),
                    tagged("blanca", "blanco", &["AQ0FS0"]),
                ],
                3,
                Some(vec!["blancos"]),
            ),
            (
                vec![
                    tagged("x", "x", &["UNKNOWN"]),
                    tagged("las", "el", &["DD0FP0"]),
                    tagged("casas", "casa", &["NCFP000"]),
                    tagged("blanco", "blanco", &["AQ0MS0"]),
                ],
                3,
                Some(vec!["blancas"]),
            ),
            (
                vec![
                    tagged("x", "x", &["UNKNOWN"]),
                    tagged("de", "de", &["SPS00"]),
                    tagged("casa", "casa", &["NCFS000"]),
                    tagged("blanco", "blanco", &["AQ0MS0"]),
                ],
                3,
                Some(vec!["blanca"]),
            ),
            (
                vec![
                    tagged("x", "x", &["UNKNOWN"]),
                    tagged("x", "x", &["UNKNOWN"]),
                    tagged("y", "y", &["UNKNOWN"]),
                    tagged("x", "x", &["UNKNOWN"]),
                    tagged("de", "de", &["SPS00"]),
                    tagged("libro", "libro", &["NCMS000"]),
                    tagged("blanca", "blanco", &["AQ0FS0"]),
                ],
                6,
                Some(vec!["blanco"]),
            ),
        ];
        for (tokens, adj_pos, expected) in cases {
            let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
            let out = accept_es(Some(&synth), "key:value", &[adj_pos], &refs, rm_for("x"));
            match (out, expected) {
                (Some(rm), Some(want)) => assert_eq!(values_of(&rm), want),
                (None, None) => {}
                (got, want) => panic!("mismatch: got {got:?} want {want:?}"),
            }
        }
    }

    #[test]
    fn es_concordance_rejects_agreeing_and_empty_contexts() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let cases: Vec<Vec<AnalyzedTokenReadings>> = vec![
            vec![
                tagged("x", "x", &["UNKNOWN"]),
                tagged("libro", "libro", &["NCMS000"]),
                tagged("la", "el", &["DA0FS0"]),
                tagged("mesa", "mesa", &["NCFS000"]),
                tagged("blancos", "blanco", &["AQ0MP0"]),
            ],
            vec![
                tagged("x", "x", &["UNKNOWN"]),
                tagged("mesa", "mesa", &["NCFS000"]),
                tagged("la", "el", &["DA0FS0"]),
                tagged("mesa", "mesa", &["NCFS000"]),
                tagged("blancas", "blanco", &["AQ0FP0"]),
            ],
            vec![
                tagged("corre", "correr", &["VMIP3S0"]),
                tagged("blanca", "blanco", &["AQ0FS0"]),
            ],
            vec![
                tagged("casa", "casa", &["NCFS000"]),
                tagged("blanca", "blanco", &["AQ0FS0"]),
            ],
        ];
        for tokens in cases {
            let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
            let adj = refs.len() - 1;
            match accept_es(Some(&synth), "key:value", &[adj], &refs, rm_for("x")) {
                None => {}
                Some(rm) => panic!(
                    "expected rejection for {:?} got {:?}",
                    refs.iter().map(|t| t.token().token()).collect::<Vec<_>>(),
                    values_of(&rm)
                ),
            }
        }
    }

    #[test]
    fn es_concordance_counts_common_gender_determiners_and_prepositions() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let cases: Vec<(Vec<AnalyzedTokenReadings>, usize, Option<Vec<&str>>)> = vec![
            (
                vec![
                    tagged("el", "el", &["DD0CS0"]),
                    tagged("agua", "agua", &["NCMS000", "NCFS000"]),
                    tagged("blancos", "blanco", &["AQ0MP0"]),
                ],
                2,
                Some(vec!["blanco"]),
            ),
            (
                vec![
                    tagged("el", "el", &["DD0CS0"]),
                    tagged("arma", "arma", &["NCFS000"]),
                    tagged("blancos", "blanco", &["AQ0MP0"]),
                ],
                2,
                Some(vec!["blanca"]),
            ),
            (
                vec![
                    tagged("las", "el", &["DD0FP0"]),
                    tagged("casas", "casa", &["NCFP000"]),
                    tagged("de", "de", &["SPS00"]),
                    tagged("la", "el", &["DD0FS0"]),
                    tagged("ciudad", "ciudad", &["NCFS000"]),
                    tagged("blancos", "blanco", &["AQ0MP0"]),
                ],
                5,
                Some(vec!["blanca", "blancas"]),
            ),
        ];
        for (tokens, adj_pos, expected) in cases {
            let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
            let out = accept_es(Some(&synth), "key:value", &[adj_pos], &refs, rm_for("x"));
            match (out, expected) {
                (Some(rm), Some(want)) => assert_eq!(values_of(&rm), want),
                (None, None) => {}
                (got, want) => panic!("mismatch: got {got:?} want {want:?}"),
            }
        }
    }

    #[test]
    fn es_concordance_uses_comma_adverbs_and_coordination() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let comma_run = vec![
            tagged("cosa", "cosa", &["NCFS000"]),
            tagged(",", ",", &["_PUNCT"]),
            tagged("libros", "libro", &["NCMP000"]),
            tagged("blancas", "blanco", &["AQ0FP0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = comma_run.iter().collect();
        match accept_es(Some(&synth), "key:value", &[3], &refs, rm_for("x")) {
            Some(rm) => assert_eq!(values_of(&rm), vec!["blancos".to_string()]),
            None => panic!(
                "comma case rejected: {:?}",
                refs.iter().map(|t| t.token().token()).collect::<Vec<_>>()
            ),
        }

        let adverbial = vec![
            tagged("x", "x", &["UNKNOWN"]),
            tagged("libro", "libro", &["NCMS000"]),
            tagged("muy", "muy", &["RG_before"]),
            tagged("blancas", "blanco", &["AQ0FS0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = adverbial.iter().collect();
        let out = accept_es(Some(&synth), "key:value", &[3], &refs, rm_for("x")).expect("kept");
        assert_eq!(values_of(&out), vec!["blanco".to_string()]);

        let locative = vec![
            tagged("x", "x", &["UNKNOWN"]),
            tagged("libro", "libro", &["NCMS000"]),
            tagged("muy", "muy", &["RG_LOC_ADV"]),
            tagged("bien", "bien", &["RG_LOC_ADV"]),
            tagged("negra", "negro", &["AQ0FS0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = locative.iter().collect();
        let out = accept_es(Some(&synth), "key:value", &[4], &refs, rm_for("x")).expect("kept");
        assert_eq!(values_of(&out), vec!["negro".to_string()]);

        let stop = vec![
            tagged("x", "x", &["UNKNOWN"]),
            tagged("libro", "libro", &["NCMS000"]),
            tagged("y", "y", &["CC"]),
            tagged(",", ",", &["_PUNCT"]),
            tagged("muy", "muy", &["RG"]),
            tagged("blanca", "blanco", &["AQ0FS0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = stop.iter().collect();
        assert!(accept_es(Some(&synth), "key:value", &[5], &refs, rm_for("x")).is_none());

        let jump = vec![
            tagged("casa", "casa", &["NCFS000"]),
            tagged("de", "de", &["SPS00"]),
            tagged("libro", "libro", &["NCMS000"]),
            tagged("y", "y", &["CC"]),
            tagged("muy", "muy", &["RG"]),
            tagged("de", "de", &["SPS00"]),
            tagged("piso", "piso", &["NCMS000"]),
            tagged("blanca", "blanco", &["AQ0FS0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = jump.iter().collect();
        let out = accept_es(Some(&synth), "key:value", &[7], &refs, rm_for("x")).expect("kept");
        assert_eq!(
            values_of(&out),
            vec!["blanco".to_string()],
            "the coordination jump skips the coordinated phrase"
        );
    }

    #[test]
    fn fr_concordance_walk_counts_determiners_and_levels() {
        let dir = lang_dir("fr");
        let Some(synth) = LangSynthesizer::for_language(&dir, "fr") else {
            return;
        };
        let cases: Vec<(Vec<AnalyzedTokenReadings>, usize, Option<Vec<&str>>)> = vec![
            (
                vec![
                    tagged("un", "un", &["D e s"]),
                    tagged("ami", "ami", &["N m s"]),
                    tagged("bleue", "bleu", &["J f s"]),
                ],
                2,
                Some(vec!["bleu"]),
            ),
            (
                vec![
                    tagged("la", "le", &["D f s"]),
                    tagged("maison", "maison", &["N f s"]),
                    tagged("de", "de", &["P"]),
                    tagged("la", "le", &["D f s"]),
                    tagged("porte", "porte", &["N f s"]),
                    tagged("bleu", "bleu", &["J m s"]),
                ],
                5,
                Some(vec!["bleue"]),
            ),
        ];
        for (tokens, adj_pos, expected) in cases {
            let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
            let out = accept_fr(Some(&synth), "key:value", &[adj_pos], &refs, rm_for("x"));
            match (out, expected) {
                (Some(rm), Some(want)) => assert_eq!(values_of(&rm), want),
                (None, None) => {}
                (got, want) => panic!("mismatch: got {got:?} want {want:?}"),
            }
        }

        let rejects = vec![
            vec![
                tagged("x", "x", &["UNKNOWN"]),
                tagged("ami", "ami", &["N m s"]),
                tagged("le", "le", &["D m s"]),
                tagged("table", "table", &["N f s"]),
                tagged("bleus", "bleu", &["J m p"]),
            ],
            vec![
                tagged("court", "courir", &["V ind pres 3s"]),
                tagged("bleue", "bleu", &["J f s"]),
            ],
        ];
        for tokens in rejects {
            let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
            let adj = refs.len() - 1;
            assert!(
                accept_fr(Some(&synth), "key:value", &[adj], &refs, rm_for("x")).is_none(),
                "expected rejection"
            );
        }
    }

    #[test]
    fn fr_concordance_walk_covers_the_remaining_gender_and_case_arms() {
        let dir = lang_dir("fr");
        let Some(synth) = LangSynthesizer::for_language(&dir, "fr") else {
            return;
        };
        let cases: Vec<(Vec<AnalyzedTokenReadings>, Option<Vec<&str>>)> = vec![
            (
                vec![
                    tagged("il", "il", &["PRO"]),
                    tagged("ami", "ami", &["N m s", "_GN_MS"]),
                    tagged("bleue", "bleu", &["J f s"]),
                ],
                Some(vec!["bleu"]),
            ),
            (
                vec![
                    tagged("il", "il", &["PRO"]),
                    tagged("camarade", "camarade", &["N m sp"]),
                    tagged("bleue", "bleu", &["J f s"]),
                ],
                Some(vec!["bleu", "bleus"]),
            ),
            (
                vec![
                    tagged("madame", "madame", &["N f s"]),
                    tagged(",", ",", &["_PUNCT"]),
                    tagged("enfant", "enfant", &["N e s"]),
                    tagged("bleu", "bleu", &["J m s"]),
                ],
                None,
            ),
            (
                vec![
                    tagged("madame", "madame", &["N f s"]),
                    tagged(",", ",", &["_PUNCT"]),
                    tagged("enfants", "enfant", &["N e sp"]),
                    tagged("bleu", "bleu", &["J m s"]),
                ],
                None,
            ),
            (
                vec![
                    tagged("il", "il", &["PRO"]),
                    tagged("la", "le", &["D e s"]),
                    tagged("porte", "porte", &["N f s"]),
                    tagged("bleu", "bleu", &["J m s"]),
                ],
                Some(vec!["bleue"]),
            ),
            (
                vec![
                    tagged("il", "il", &["PRO"]),
                    tagged("les", "le", &["D e p"]),
                    tagged("amis", "ami", &["N m p"]),
                    tagged("bleue", "bleu", &["J f s"]),
                ],
                Some(vec!["bleus"]),
            ),
            (
                vec![
                    tagged("il", "il", &["PRO"]),
                    tagged("le", "le", &["D m s"]),
                    tagged("garçon", "garçon", &["N m s"]),
                    tagged("bleue", "bleu", &["J f s"]),
                ],
                Some(vec!["bleu"]),
            ),
            (
                vec![
                    tagged("il", "il", &["PRO"]),
                    tagged("les", "le", &["D f p"]),
                    tagged("maisons", "maison", &["N f p"]),
                    tagged("bleu", "bleu", &["J m s"]),
                ],
                Some(vec!["bleues"]),
            ),
            (
                vec![
                    tagged("madame", "madame", &["N f s"]),
                    tagged(",", ",", &["_PUNCT"]),
                    tagged("amis", "ami", &["N m p"]),
                    tagged("bleu", "bleu", &["J m s"]),
                ],
                Some(vec!["bleus"]),
            ),
        ];
        for (tokens, expected) in cases {
            let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
            let adj = refs.len() - 1;
            let out = accept_fr(Some(&synth), "key:value", &[adj], &refs, rm_for("x"));
            match (out, expected) {
                (Some(rm), Some(want)) => assert_eq!(
                    values_of(&rm),
                    want,
                    "tokens: {:?}",
                    tokens.iter().map(|t| t.token().token()).collect::<Vec<_>>()
                ),
                (None, None) => {}
                (got, want) => panic!(
                    "mismatch: got {got:?} want {want:?} for {:?}",
                    tokens.iter().map(|t| t.token().token()).collect::<Vec<_>>()
                ),
            }
        }
    }

    #[test]
    fn fr_concordance_level_preposition_and_coordination_jumps() {
        let dir = lang_dir("fr");
        let Some(synth) = LangSynthesizer::for_language(&dir, "fr") else {
            return;
        };
        let d_environ = vec![
            tagged("lit", "lit", &["N m s"]),
            tagged("et", "et", &["C c"]),
            tagged("d'", "de", &["P"]),
            tagged("environ", "environ", &["N m s"]),
            tagged("bleue", "bleu", &["J f s"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = d_environ.iter().collect();
        let out = accept_fr(Some(&synth), "key:value", &[4], &refs, rm_for("x")).expect("kept");
        assert_eq!(
            values_of(&out),
            vec!["bleu".to_string()],
            "the d'environ exception opens a new level even behind a coordination"
        );

        let deux_ou_plus = vec![
            tagged("deux", "deux", &["D e p"]),
            tagged("ou", "ou", &["C c"]),
            tagged("plus", "plus", &["A"]),
            tagged("bleue", "bleu", &["J f s"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = deux_ou_plus.iter().collect();
        assert!(
            accept_fr(Some(&synth), "key:value", &[3], &refs, rm_for("x")).is_none(),
            "'deux ou plus' must not count the numeral as a concordance determiner"
        );

        let sans_synth = vec![
            tagged("il", "il", &["PRO"]),
            tagged("maison", "maison", &["N f s"]),
            tagged("bleu", "bleu", &["J m s"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = sans_synth.iter().collect();
        let kept = accept_fr(None, "key:value", &[2], &refs, rm_for("x")).expect("kept");
        assert!(
            kept.replacements().is_empty(),
            "without a synthesizer the match passes through untouched"
        );
    }

    #[test]
    fn es_coordination_walk_jumps_back_to_the_preposition() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let tokens = vec![
            tagged("La", "el", &["DA0FS0"]),
            tagged("de", "de", &["SPS00"]),
            tagged(",", ",", &["Fx"]),
            tagged("y", "y", &["CC"]),
            tagged("casa", "casa", &["NCFS000"]),
            tagged("de", "de", &["SPS00"]),
            tagged("ciudad", "ciudad", &["NCFS000"]),
            tagged("blancas", "blanco", &["AQ0FP0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let out = accept_es(Some(&synth), "key:value", &[7], &refs, rm_for("x")).expect("kept");
        assert_eq!(values_of(&out), vec!["blanca".to_string()]);
    }

    #[test]
    fn es_coordination_walk_stops_at_a_finite_verb() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let tokens = vec![
            tagged("El", "el", &["DA0MS0"]),
            tagged("corre", "correr", &["VMIP3P0"]),
            tagged("y", "y", &["CC"]),
            tagged("casa", "casa", &["NCFS000"]),
            tagged("de", "de", &["SPS00"]),
            tagged("pueblo", "pueblo", &["NCMS000"]),
            tagged("blancas", "blanco", &["AQ0FP0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let out = accept_es(Some(&synth), "key:value", &[6], &refs, rm_for("x")).expect("kept");
        assert_eq!(
            values_of(&out),
            vec!["blanco".to_string(), "blanca".to_string()]
        );
    }

    #[test]
    fn es_concordance_clamps_the_level_at_max_levels() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let mut tokens = vec![tagged("La", "el", &["DA0FS0"])];
        for _ in 0..4 {
            tokens.push(tagged("de", "de", &["SPS00"]));
            tokens.push(tagged("pueblo", "pueblo", &["NCMS000"]));
        }
        tokens.push(tagged("blancas", "blanco", &["AQ0FP0"]));
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let out = accept_es(
            Some(&synth),
            "key:value",
            &[refs.len() - 1],
            &refs,
            rm_for("x"),
        )
        .expect("kept");
        assert_eq!(values_of(&out), vec!["blanco".to_string()]);
    }

    #[test]
    fn es_unmatched_adjective_shape_is_rejected() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let tokens = vec![
            tagged("La", "el", &["DA0FS0"]),
            tagged("casa", "casa", &["NCFS000"]),
            tagged("corre", "correr", &["VMIP3S0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        assert!(
            accept_es(Some(&synth), "key:value", &[2], &refs, rm_for("x")).is_none(),
            "only the six gender/number adjective shapes can be checked"
        );
    }

    #[test]
    fn es_unknown_lemma_yields_no_suggestions() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let tokens = vec![
            tagged("En", "en", &["SPS00"]),
            tagged("en", "en", &["SPS00"]),
            tagged("un", "uno", &["DI0MS0"]),
            tagged("restaurante", "restaurante", &["NCMS000"]),
            tagged("lujosa", "zzzzzzzz", &["AQ0FS0"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        assert!(
            accept_es(Some(&synth), "key:value", &[4], &refs, rm_for("x")).is_none(),
            "an adjective the synthesizer cannot inflect produces no rule match"
        );
    }

    #[test]
    fn es_out_of_range_match_position_rejects() {
        let tokens = vec![tagged("casa", "casa", &["NCFS000"])];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        assert!(accept_es(None, "key:value", &[7], &refs, rm_for("x")).is_none());
    }

    #[test]
    fn fr_common_gender_plural_adjective_uses_the_plural_synthesis() {
        let dir = lang_dir("fr");
        let Some(synth) = LangSynthesizer::for_language(&dir, "fr") else {
            return;
        };
        let tokens = vec![
            tagged("Les", "le", &["D e p"]),
            tagged("chats", "chat", &["N m p"]),
            tagged(".", ".", &["N m p"]),
            tagged(",", ",", &["R"]),
            tagged("joli", "joli", &["J m s"]),
            tagged("bleues", "bleu", &["J f p"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let out = accept_fr(Some(&synth), "key:value", &[5], &refs, rm_for("x")).expect("kept");
        assert_eq!(values_of(&out), vec!["bleus".to_string()]);
    }

    #[test]
    fn fr_feminine_singular_plural_adjective_selects_the_fn_pattern() {
        let dir = lang_dir("fr");
        let Some(synth) = LangSynthesizer::for_language(&dir, "fr") else {
            return;
        };
        let tokens = vec![
            tagged("Le", "le", &["D m s"]),
            tagged("chat", "chat", &["N m s"]),
            tagged("jolie", "joli", &["J f sp"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let out = accept_fr(Some(&synth), "key:value", &[2], &refs, rm_for("x")).expect("kept");
        assert_eq!(values_of(&out), vec!["joli".to_string()]);
    }

    #[test]
    fn fr_deux_ou_plus_jumps_past_the_numeral_determiner() {
        let dir = lang_dir("fr");
        let Some(synth) = LangSynthesizer::for_language(&dir, "fr") else {
            return;
        };
        let tokens = vec![
            tagged("Les", "le", &["D f p"]),
            tagged("deux", "deux", &["D f p"]),
            tagged("ou", "ou", &["C c"]),
            tagged("plus", "plus", &["A"]),
            tagged("grandes", "grand", &["J f p"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        assert!(
            accept_fr(Some(&synth), "key:value", &[4], &refs, rm_for("x")).is_none(),
            "the 'deux ou plus' jump must skip the numeral so nothing is counted"
        );
    }

    #[test]
    fn fr_com_token_updates_no_apparitions() {
        let tokens = vec![
            tagged("Le", "le", &["D m s"]),
            tagged("com", "com", &["D m p"]),
            tagged("restaurant", "restaurant", &["N m s"]),
            tagged("jolie", "zzzzzzzz", &["J f s"]),
        ];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        let kept = accept_fr(None, "key:value", &[3], &refs, rm_for("x")).expect("kept");
        assert!(kept.replacements().is_empty());
    }

    #[test]
    fn fr_out_of_range_match_position_rejects() {
        let tokens = vec![tagged("maison", "maison", &["N f s"])];
        let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
        assert!(accept_fr(None, "key:value", &[9], &refs, rm_for("x")).is_none());
    }

    #[test]
    fn synthesizer_regex_tail_and_postag_edges() {
        let Some(synth) = pl_synth() else {
            return;
        };
        assert_eq!(
            synth.synthesize_full("zrobić", "ger:sg:nom\\.acc:n2:perf:aff:refl.nonrefl"),
            vec!["zrobienie".to_string()]
        );
        assert!(
            synth
                .synthesize_full_for("zrobić", None, "adj:[")
                .is_empty()
        );
        let es = LangSynthesizer::for_language(&lang_dir("es"), "es").expect("es synthesizer");
        let forms = es.synthesize_full_for("dar lo", None, "VMIP3S0");
        assert!(
            forms.iter().all(|f| f.ends_with(" lo")),
            "tail is appended: {forms:?}"
        );
        assert_eq!(es.target_postag(&["VMM02S0", "VMIP3S0"]), Some("VMIP3S0"));
        assert_eq!(
            es.target_postag(&["AQ0MS0", "VMIP3S0", "VMM02S0"]),
            Some("VMIP3S0")
        );
        assert!(es.synthesize_full_for("casa", None, "[").is_empty());
        assert_eq!(uppercase_first_char(""), "");
        assert_eq!(lowercase_first_char(""), "");
    }

    #[test]
    fn german_hyphen_compound_falls_back_to_the_hyphen_split() {
        let dir = lang_dir("de");
        let Some(synth) = LangSynthesizer::for_language(&dir, "de") else {
            return;
        };
        let forms = synth.synthesize_full_for("Xyzqwe-Ding", None, "SUB:NOM:SIN:NEU");
        assert_eq!(
            forms,
            vec!["Xyzqwe-Ding".to_string()],
            "the hyphen split recombines with the original uppercase part"
        );
    }

    #[test]
    fn es_concordance_counts_disambiguated_gn_readings() {
        let dir = lang_dir("es");
        let Some(synth) = LangSynthesizer::for_language(&dir, "es") else {
            return;
        };
        let cases: Vec<(Vec<AnalyzedTokenReadings>, Vec<&str>)> = vec![
            (
                vec![
                    tagged("La", "el", &["DA0FS0"]),
                    tagged("casa", "casa", &["NCFS000"]),
                    tagged("la", "el", &["DA0FS0"]),
                    tagged("verde", "verde", &["AQ0FS0", "_GN_FS"]),
                    tagged("blancos", "blanco", &["AQ0MP0"]),
                ],
                vec!["blanca", "blancas"],
            ),
            (
                vec![
                    tagged("El", "el", &["DA0MS0"]),
                    tagged("el", "el", &["DA0MS0"]),
                    tagged("verdes", "verde", &["NCMS000", "_GN_MP"]),
                    tagged("blanca", "blanco", &["AQ0FS0"]),
                ],
                vec!["blanco", "blancos"],
            ),
            (
                vec![
                    tagged("El", "el", &["DA0MS0"]),
                    tagged("pueblo", "pueblo", &["NCMS000"]),
                    tagged("el", "el", &["DA0MS0"]),
                    tagged("verdes", "verde", &["AQ0FP0", "_GN_FP"]),
                    tagged("blanca", "blanco", &["AQ0FS0"]),
                ],
                vec!["blanco", "blancas", "blancos"],
            ),
        ];
        for (tokens, want) in cases {
            let refs: Vec<&AnalyzedTokenReadings> = tokens.iter().collect();
            let out = accept_es(
                Some(&synth),
                "key:value",
                &[refs.len() - 1],
                &refs,
                rm_for("x"),
            )
            .expect("kept");
            assert_eq!(
                values_of(&out),
                want,
                "tokens: {:?}",
                tokens.iter().map(|t| t.token().token()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn english_synthesizer_without_a_rules_dir_uses_empty_article_data() {
        let dir = std::env::temp_dir().join(format!("og-rules-en-synth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("dict_decoded.txt"), "went\tgo\tVBD\n").unwrap();
        let synth = LangSynthesizer::for_language(&dir, "en").expect("synthesizer from dict alone");
        assert!(synth.is_determiner_tag("+DT"));
        let re = Regex::new("^(?:VBD)$").unwrap();
        let went = synth.synthesize("go", &re);
        assert_eq!(went, vec!["went".to_string()]);
        assert_eq!(synth.determiner_forms("university", "+INDT").len(), 1);
        drop(synth);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
