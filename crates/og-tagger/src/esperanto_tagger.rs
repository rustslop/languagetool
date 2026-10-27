use std::collections::{HashMap, HashSet};
use std::path::Path;

use og_core::{AnalyzedToken, AnalyzedTokenReadings};
use regex::Regex;
use std::sync::OnceLock;

pub struct EsperantoTagger {
    manual: HashMap<String, Vec<(String, String)>>,
    transitive: HashSet<String>,
    intransitive: HashSet<String>,
    non_participle: HashSet<String>,
}

fn load_words(text: &str) -> HashSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn parse_manual_tagger(text: &str) -> HashMap<String, Vec<(String, String)>> {
    let mut separator = "\t";
    let mut map: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if let Some(sep) = line.trim().strip_prefix("#separatorRegExp=") {
            separator = sep;
            continue;
        }
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let stripped = line.split('#').next().unwrap_or("").trim();
        let parts: Vec<&str> = stripped.split(separator).collect();
        if parts.len() != 3 {
            continue;
        }
        map.entry(parts[0].to_string())
            .or_default()
            .push((parts[1].to_string(), parts[2].trim().to_string()));
    }
    map
}

fn x_system_to_unicode(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c1 = chars[i];
        let c2 = chars.get(i + 1).copied().unwrap_or(' ');
        if c2 == 'x' {
            match c1 {
                'c' => {
                    out.push('ĉ');
                    i += 1;
                }
                'g' => {
                    out.push('ĝ');
                    i += 1;
                }
                'h' => {
                    out.push('ĥ');
                    i += 1;
                }
                'j' => {
                    out.push('ĵ');
                    i += 1;
                }
                's' => {
                    out.push('ŝ');
                    i += 1;
                }
                'u' => {
                    out.push('ŭ');
                    i += 1;
                }
                _ => out.push(c1),
            }
        } else {
            out.push(c1);
        }
        i += 1;
    }
    out
}

struct Patterns {
    verb: Regex,
    prefix: Regex,
    suffix: Regex,
    participle: Regex,
    tabelvorto: Regex,
    tabelvorto_adverb: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        verb: Regex::new("(..+)(as|os|is|us|u|i)$").unwrap(),
        prefix: Regex::new("^(?:mal|mis|ek|re|fi|ne)(.*)").unwrap(),
        suffix: Regex::new("(.*)((?:ad|aĉ|eg|et)i)$").unwrap(),
        participle: Regex::new("((..+)([aio])(n?)t)([aoe])(j?)(n?)$").unwrap(),
        tabelvorto: Regex::new("^(i|ti|ki|ĉi|neni)(?:(?:([uoae])(j?)(n?))|(am|al|es|el|om))$")
            .unwrap(),
        tabelvorto_adverb: Regex::new("^(?:ti|i|ĉi|neni)(?:am|om|el|e)$").unwrap(),
    })
}

impl EsperantoTagger {
    pub fn from_resource_dir(resource_dir: &Path) -> Option<Self> {
        let manual_text = std::fs::read_to_string(resource_dir.join("manual-tagger.txt")).ok()?;
        let rules_dir = resource_dir
            .ancestors()
            .nth(2)
            .map(|p| p.join("rules/eo"))?;
        let transitive = std::fs::read_to_string(rules_dir.join("verb-tr.txt"))
            .map(|t| load_words(&t))
            .unwrap_or_default();
        let intransitive = std::fs::read_to_string(rules_dir.join("verb-ntr.txt"))
            .map(|t| load_words(&t))
            .unwrap_or_default();
        let non_participle = std::fs::read_to_string(rules_dir.join("root-ant-at.txt"))
            .map(|t| load_words(&t))
            .unwrap_or_default();
        Some(Self {
            manual: parse_manual_tagger(&manual_text),
            transitive,
            intransitive,
            non_participle,
        })
    }

    fn find_transitivity(&self, verb: &str) -> &'static str {
        if verb.ends_with("iĝi") {
            return "nt";
        } else if verb.ends_with("igi") {
            return if verb == "memmortigi" { "nt" } else { "tr" };
        }
        let p = patterns();
        let mut verb = verb.to_string();
        loop {
            let is_transitive = self.transitive.contains(verb.as_str());
            let is_intransitive = self.intransitive.contains(verb.as_str());
            if is_transitive {
                return if is_intransitive { "tn" } else { "tr" };
            } else if is_intransitive {
                return "nt";
            }
            if let Some(next) = p
                .prefix
                .captures(verb.as_str())
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
            {
                verb = next;
                continue;
            }
            if let Some(stem) = p.suffix.captures(verb.as_str()).and_then(|c| c.get(1)) {
                verb = format!("{}i", stem.as_str());
                continue;
            }
            break;
        }
        "xx"
    }

    fn tag_word(&self, word: &str, start: usize, end: usize) -> Vec<AnalyzedToken> {
        let mut l: Vec<AnalyzedToken> = Vec::new();
        let char_count = word.chars().count();
        if char_count > 50 {
            l.push(AnalyzedToken::new(word, start, end));
        } else if char_count > 1 {
            let l_word = x_system_to_unicode(&word.to_lowercase());
            if let Some(manual_tags) = self.manual.get(&l_word)
                && !manual_tags.is_empty()
            {
                for (lemma, tag) in manual_tags {
                    l.push(
                        AnalyzedToken::new(word, start, end)
                            .with_lemma(lemma.clone())
                            .with_pos_tags(vec![tag.clone()]),
                    );
                }
            } else {
                let p = patterns();
                if let Some(c) = p.tabelvorto.captures(&l_word) {
                    let type1 = c
                        .get(1)
                        .and_then(|m| m.as_str().chars().next())
                        .map(|ch| ch.to_lowercase().to_string())
                        .unwrap_or_default();
                    let type2 = c.get(2).map(|m| m.as_str().to_string());
                    let pl = c.get(3).map(|m| m.as_str().to_string());
                    let acc = c.get(4).map(|m| m.as_str().to_string());
                    let type3 = c.get(5).map(|m| m.as_str().to_string());
                    let accusative = match acc.as_deref() {
                        None => "xxx".to_string(),
                        Some(g) if g.eq_ignore_ascii_case("n") => "akz".to_string(),
                        Some(_) => "nak".to_string(),
                    };
                    let plural = match pl.as_deref() {
                        None => " pn ".to_string(),
                        Some(g) if g.eq_ignore_ascii_case("j") => " pl ".to_string(),
                        Some(_) => " np ".to_string(),
                    };
                    let ty = type2.or(type3).map(|t| t.to_lowercase());
                    l.push(
                        AnalyzedToken::new(word, start, end).with_pos_tags(vec![format!(
                            "T {}{}{} {}",
                            accusative,
                            plural,
                            type1,
                            ty.as_deref().unwrap_or_default()
                        )]),
                    );
                    if p.tabelvorto_adverb.is_match(&l_word) {
                        l.push(
                            AnalyzedToken::new(word, start, end)
                                .with_lemma(l_word.clone())
                                .with_pos_tags(vec!["E nak".to_string()]),
                        );
                    }
                } else if l_word.ends_with('o') {
                    l.push(tagged(word, start, end, "O nak np", &l_word));
                } else if l_word.len() >= 2 && l_word.ends_with('\'') {
                    l.push(tagged(
                        word,
                        start,
                        end,
                        "O nak np",
                        &format!("{}o", &l_word[..l_word.len() - 1]),
                    ));
                } else if l_word.ends_with("oj") {
                    l.push(tagged(
                        word,
                        start,
                        end,
                        "O nak pl",
                        &l_word[..l_word.len() - 1],
                    ));
                } else if l_word.ends_with("on") {
                    l.push(tagged(
                        word,
                        start,
                        end,
                        "O akz np",
                        &l_word[..l_word.len() - 1],
                    ));
                } else if l_word.ends_with("ojn") {
                    l.push(tagged(
                        word,
                        start,
                        end,
                        "O akz pl",
                        &l_word[..l_word.len() - 2],
                    ));
                } else if l_word.ends_with('a') {
                    l.push(tagged(word, start, end, "A nak np", &l_word));
                } else if l_word.ends_with("aj") {
                    l.push(tagged(
                        word,
                        start,
                        end,
                        "A nak pl",
                        &l_word[..l_word.len() - 1],
                    ));
                } else if l_word.ends_with("an") {
                    l.push(tagged(
                        word,
                        start,
                        end,
                        "A akz np",
                        &l_word[..l_word.len() - 1],
                    ));
                } else if l_word.ends_with("ajn") {
                    l.push(tagged(
                        word,
                        start,
                        end,
                        "A akz pl",
                        &l_word[..l_word.len() - 2],
                    ));
                } else if l_word.ends_with('e') {
                    l.push(tagged(word, start, end, "E nak", &l_word));
                } else if l_word.ends_with("en") {
                    l.push(tagged(
                        word,
                        start,
                        end,
                        "E akz",
                        &l_word[..l_word.len() - 1],
                    ));
                } else if let Some(c) = p.verb.captures(&l_word) {
                    let verb = format!("{}i", &c[1]);
                    let tense = c[2].to_string();
                    let transitive = self.find_transitivity(&verb);
                    l.push(
                        AnalyzedToken::new(word, start, end)
                            .with_lemma(verb)
                            .with_pos_tags(vec![format!("V {transitive} {tense}")]),
                    );
                } else {
                    l.push(AnalyzedToken::new(word, start, end));
                }

                if let Some(c) = p.participle.captures(&l_word)
                    && !self
                        .non_participle
                        .contains(c.get(1).map_or("", |m| m.as_str()))
                {
                    let verb = format!("{}i", &c[2]);
                    let aio = c[3].to_string();
                    let ant_at = if &c[4] == "n" { "n" } else { "-" };
                    let aoe = c[5].to_string();
                    let plural = if &c[6] == "j" { "pl" } else { "np" };
                    let accusative = if &c[7] == "n" { "akz" } else { "nak" };
                    let transitive = self.find_transitivity(&verb);
                    l.push(
                        AnalyzedToken::new(word, start, end)
                            .with_lemma(verb)
                            .with_pos_tags(vec![format!(
                                "C {accusative} {plural} {transitive} {aio} {ant_at} {aoe}"
                            )]),
                    );
                }
            }
        } else {
            l.push(AnalyzedToken::new(word, start, end));
        }
        l
    }
}

fn tagged(word: &str, start: usize, end: usize, tag: &str, lemma: &str) -> AnalyzedToken {
    AnalyzedToken::new(word, start, end)
        .with_lemma(lemma.to_string())
        .with_pos_tags(vec![tag.to_string()])
}

impl crate::Tagger for EsperantoTagger {
    fn tag(&self, tokens: &[&str]) -> Vec<AnalyzedTokenReadings> {
        let mut results = Vec::with_capacity(tokens.len());
        let mut byte_offset = 0;
        for word in tokens {
            let start = byte_offset;
            let end = start + word.len();
            let readings = self.tag_word(word, start, end);
            let primary = AnalyzedToken::new(*word, start, end);
            results.push(AnalyzedTokenReadings::new(primary).with_readings(readings));
            byte_offset = end;
        }
        results
    }
}

impl og_core::checker::Tagger for EsperantoTagger {
    fn tag(&self, tokens: &mut [AnalyzedTokenReadings]) {
        for atr in tokens.iter_mut().skip(1) {
            let primary = atr.token();
            let surface = primary.token().to_string();
            let start = primary.start();
            let end = primary.end();
            let readings = self.tag_word(&surface, start, end);
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let lemma = readings
                .first()
                .and_then(|r| r.lemma().map(str::to_string))
                .unwrap_or_else(|| surface.clone());
            let updated = AnalyzedToken::new(surface, start, end)
                .with_pos_tags(all_tags)
                .with_lemma(lemma);
            *atr = AnalyzedTokenReadings::new(updated).with_readings(readings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::checker::Tagger as _;

    fn eo_resource_dir() -> Option<std::path::PathBuf> {
        [
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/eo/src/main/resources/org/languagetool/resource/eo",
            "languagetool/languagetool-language-modules/eo/src/main/resources/org/languagetool/resource/eo",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join("manual-tagger.txt").exists())
    }

    fn readings(tagger: &EsperantoTagger, text: &str) -> Vec<String> {
        let mut tokens: Vec<AnalyzedTokenReadings> =
            vec![AnalyzedTokenReadings::new(AnalyzedToken::new("<S>", 0, 0))];
        let mut offset = 0;
        for word in text.split(' ') {
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                word,
                offset,
                offset + word.len(),
            )));
            offset += word.len() + 1;
        }
        tagger.tag(&mut tokens);
        tokens
            .iter()
            .skip(1)
            .map(|t| {
                t.readings()
                    .iter()
                    .map(|r| {
                        format!(
                            "{}/[{}]{}",
                            t.token().token(),
                            r.lemma().unwrap_or("null"),
                            r.pos_tags().join(" ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect()
    }

    #[test]
    fn esperanto_tagger_matches_java_test_expectations() {
        let Some(dir) = eo_resource_dir() else {
            return;
        };
        let tagger = EsperantoTagger::from_resource_dir(&dir).expect("eo resources");

        assert_eq!(
            readings(&tagger, "Tio estas simpla testo"),
            vec![
                "Tio/[null]T nak np t o",
                "estas/[esti]V nt as",
                "simpla/[simpla]A nak np",
                "testo/[testo]O nak np",
            ]
        );
        assert_eq!(
            readings(&tagger, "Mi malsategas"),
            vec!["Mi/[mi]R nak np", "malsategas/[malsategi]V nt as"]
        );
        assert_eq!(
            readings(&tagger, "Li malŝategas sin"),
            vec![
                "Li/[li]R nak np",
                "malŝategas/[malŝategi]V tr as",
                "sin/[si]R akz np"
            ]
        );
    }

    #[test]
    fn esperanto_tagger_x_system_pangram_matches_java() {
        let Some(dir) = eo_resource_dir() else {
            return;
        };
        let tagger = EsperantoTagger::from_resource_dir(&dir).expect("eo resources");
        assert_eq!(
            readings(
                &tagger,
                "Sxajnas ke sagaca monahxo lauxtvocxe rifuzadis pregxi sur herbajxo"
            ),
            vec![
                "Sxajnas/[ŝajni]V nt as",
                "ke/[ke]_",
                "sagaca/[sagaca]A nak np",
                "monahxo/[monaĥo]O nak np",
                "lauxtvocxe/[laŭtvoĉe]E nak",
                "rifuzadis/[rifuzadi]V tr is",
                "pregxi/[preĝi]V nt i",
                "sur/[sur]P kak",
                "herbajxo/[herbaĵo]O nak np",
            ]
        );
    }

    #[test]
    fn x_system_conversion_handles_all_digraphs() {
        assert_eq!(x_system_to_unicode("jxauxdo"), "ĵaŭdo");
        assert_eq!(x_system_to_unicode("gx"), "ĝ");
        assert_eq!(x_system_to_unicode("hx"), "ĥ");
        assert_eq!(x_system_to_unicode("sx"), "ŝ");
        assert_eq!(x_system_to_unicode("ax"), "ax");
    }
}

#[cfg(test)]
mod suffix_tests {
    use super::*;

    fn tagger() -> EsperantoTagger {
        let dir = [
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/eo/src/main/resources/org/languagetool/resource/eo",
            "languagetool/languagetool-language-modules/eo/src/main/resources/org/languagetool/resource/eo",
        ]
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.join("manual-tagger.txt").exists())
        .expect("eo resources");
        EsperantoTagger::from_resource_dir(&dir).expect("tagger loads")
    }

    fn readings(tagger: &EsperantoTagger, word: &str) -> Vec<(String, String)> {
        tagger
            .tag_word(word, 0, word.len())
            .iter()
            .map(|r| {
                (
                    r.lemma().unwrap_or_default().to_string(),
                    r.pos_tags().join(" "),
                )
            })
            .collect()
    }

    #[test]
    fn noun_adjective_adverb_endings_carry_number_and_case() {
        let t = tagger();
        assert_eq!(
            readings(&t, "bel'"),
            vec![("belo".to_string(), "O nak np".to_string())]
        );
        assert_eq!(
            readings(&t, "testoj"),
            vec![("testo".to_string(), "O nak pl".to_string())]
        );
        assert_eq!(
            readings(&t, "teston"),
            vec![("testo".to_string(), "O akz np".to_string())]
        );
        assert_eq!(
            readings(&t, "testojn"),
            vec![("testo".to_string(), "O akz pl".to_string())]
        );
        assert_eq!(
            readings(&t, "simplaj"),
            vec![("simpla".to_string(), "A nak pl".to_string())]
        );
        assert_eq!(
            readings(&t, "simplan"),
            vec![("simpla".to_string(), "A akz np".to_string())]
        );
        assert_eq!(
            readings(&t, "simplajn"),
            vec![("simpla".to_string(), "A akz pl".to_string())]
        );
        assert_eq!(
            readings(&t, "rapiden"),
            vec![("rapide".to_string(), "E akz".to_string())]
        );
    }

    #[test]
    fn transitivity_rules_strip_prefixes_and_suffixes() {
        let t = tagger();
        assert_eq!(
            readings(&t, "sidiĝi"),
            vec![("sidiĝi".to_string(), "V nt i".to_string())]
        );
        assert_eq!(
            readings(&t, "farigi"),
            vec![("farigi".to_string(), "V tr i".to_string())]
        );
        assert_eq!(
            readings(&t, "memmortigi"),
            vec![("memmortigi".to_string(), "V nt i".to_string())]
        );
        assert_eq!(
            readings(&t, "maldormi"),
            vec![("maldormi".to_string(), "V nt i".to_string())]
        );
        assert_eq!(readings(&t, "zzz"), vec![(String::new(), String::new())]);
    }

    #[test]
    fn tabelvortoj_get_correlatives_and_adverb_readings() {
        let t = tagger();
        let tia = readings(&t, "tiuj");
        assert!(
            tia.contains(&(String::new(), "T nak pl t u".to_string())),
            "{tia:?}"
        );
        let tin = readings(&t, "tiun");
        assert!(
            tin.contains(&(String::new(), "T akz np t u".to_string())),
            "{tin:?}"
        );
        let tiam = readings(&t, "tiam");
        assert!(
            tiam.contains(&(String::new(), "T xxx pn t am".to_string())),
            "{tiam:?}"
        );
        assert!(
            tiam.contains(&("tiam".to_string(), "E nak".to_string())),
            "{tiam:?}"
        );
    }

    #[test]
    fn participles_add_c_readings_unless_the_root_is_listed() {
        let t = tagger();
        let laboranta = readings(&t, "laboranta");
        assert_eq!(
            laboranta,
            vec![
                ("laboranta".to_string(), "A nak np".to_string()),
                ("labori".to_string(), "C nak np nt a n a".to_string()),
            ]
        );
        let abata = readings(&t, "abata");
        assert_eq!(abata, vec![("abata".to_string(), "A nak np".to_string())]);
    }

    #[test]
    fn overlong_words_stay_untagged_and_slice_tagging_keeps_offsets() {
        let t = tagger();
        let long = "a".repeat(51);
        assert_eq!(readings(&t, &long), vec![(String::new(), String::new())]);

        let out = crate::Tagger::tag(&t, &["testo", "nekonata"]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].token().token(), "testo");
        assert_eq!(out[0].token().start(), 0);
        assert_eq!(out[0].token().end(), 5);
        assert_eq!(out[1].token().start(), 5);
        assert_eq!(out[1].token().end(), 13);
    }
}
