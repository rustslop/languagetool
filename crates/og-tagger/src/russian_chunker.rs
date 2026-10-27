use crate::openregex::PhraseTag;
use crate::openregex::Token;
use crate::openregex::apply;
use crate::openregex::build_expanded;
use crate::openregex::lru_cache::BoundedMap;
use crate::openregex::sentence_forms;
use og_core::AnalyzedSentence;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::OnceLock;

const FILTER_TAGS: [&str; 8] = [
    "PP",
    "NPP",
    "NPS",
    "MayMissingYO",
    "VP",
    "SBAR",
    "ADJP",
    "DPT",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum PhraseType {
    Np,
    Npp,
    Vp,
    Sbar,
    Adjp,
    Dpt,
}

impl PhraseType {
    fn tag(self) -> PhraseTag {
        match self {
            PhraseType::Np => PhraseTag::Bi {
                start: "B-NP",
                inner: "I-NP",
            },
            PhraseType::Npp => PhraseTag::Bi {
                start: "B-NP-plural",
                inner: "I-NP-plural",
            },
            PhraseType::Vp => PhraseTag::Bi {
                start: "B-VP",
                inner: "I-VP",
            },
            PhraseType::Adjp => PhraseTag::Bi {
                start: "B-ADJP",
                inner: "I-ADJP",
            },
            PhraseType::Dpt => PhraseTag::Bi {
                start: "B-DPT",
                inner: "I-DPT",
            },
            PhraseType::Sbar => PhraseTag::Flat("SBAR"),
        }
    }
}

fn expand(expr: &str) -> String {
    expr.replace("<NP>", "<chunk=B-NP> <chunk=I-NP>*")
        .replace("<VP>", "<chunk=B-VP> <chunk=I-VP>*")
        .replace("<ADJP>", "<chunk=B-ADJP> <chunk=I-ADJP>*")
        .replace("<DPT>", "<chunk=B-DPT> <chunk=I-DPT>*")
}

fn build(expr: &str, phrase: PhraseType, overwrite: bool) -> crate::openregex::Rule {
    let expanded = expand(expr);
    build_expanded(&expanded, phrase.tag(), overwrite, &FILTER_TAGS, Vec::new())
}

fn rules1() -> &'static [crate::openregex::Rule] {
    static RULES: OnceLock<Vec<crate::openregex::Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        vec![
            build(
                r"<posre='NN:(Name|Fam|Patr):.*'> <posre='NN:(Name|Fam|Patr):.*'>+ ",
                PhraseType::Np,
                true,
            ),
            build(
                r"<posre='NN:Fam:.*'> <regexCS=[А-ЯЁ]> <.> <regexCS=[А-ЯЁ]> <.> ",
                PhraseType::Np,
                true,
            ),
            build(
                r"<regexCS=[А-ЯЁ]> <.> <regexCS=[А-ЯЁ]> <.> <posre='NN:Fam:.*'> ",
                PhraseType::Np,
                true,
            ),
            build(
                r"<posre='VB:.*:.*' & !posre='NN:.*'>* ",
                PhraseType::Vp,
                false,
            ),
            build(r"<если>", PhraseType::Sbar, false),
            build(r"<поэтому>", PhraseType::Sbar, false),
            build(
                r"<posre='ADJ:Posit:.*:.*'> <posre='NN:(Anim|Inanim):.*' & !posre='NN:(Anim|Inanim):.*:(R|D|T|P)'> ",
                PhraseType::Np,
                true,
            ),
            build(
                r"<posre='ADJ:Posit:.*:.*'> <posre='NN:(Anim|Inanim):.*' & !posre='NN:(Anim|Inanim):.*:(R|D|T|P)'> <posre='NN:(Anim|Inanim):.*'> ",
                PhraseType::Np,
                true,
            ),
            build(
                r"<posre='ADJ:Posit:.*:.*'> <posre='NN:(Anim|Inanim):.*' & !posre='NN:(Anim|Inanim):.*:(Nom|V)'> <posre='NN:(Anim|Inanim):.*:(Nom|V)' & !posre='NN:(Anim|Inanim):.*:(R|D|T|P)'> ",
                PhraseType::Adjp,
                true,
            ),
            build(
                r"<posre='DPT:.*:.*' & !pos='PREP'> ",
                PhraseType::Dpt,
                false,
            ),
            build(
                r"<posre='DPT:.*:.*' & !pos='PREP'> <posre='NN:.*:.*:(R|D|T|P)'> ",
                PhraseType::Dpt,
                true,
            ),
            build(
                r"<posre='DPT:.*:.*' & !pos='PREP'> <posre='PREP'> <posre='NN:.*:.*:(R|D|T|P)'> ",
                PhraseType::Dpt,
                true,
            ),
            build(r"<posre='PT:.*:.*'> ", PhraseType::Adjp, false),
            build(
                r"<posre='PT:.*:.*'> <pos='ADV'> ",
                PhraseType::Adjp,
                true,
            ),
            build(
                r"<posre='PT:.*:.*'> <posre='NN:.*:.*:(R|D|T|P)'> ",
                PhraseType::Adjp,
                true,
            ),
            build(
                r"<posre='PT:.*:.*'> <posre='PREP'> <posre='NN:.*:.*:(R|D|T|P|V)'> ",
                PhraseType::Adjp,
                true,
            ),
            build(
                r"<posre='PT:.*:.*'> <posre='PREP'> <posre='ADJ:.*:.*:(R|D|T|P|V)'> <posre='NN:.*:.*:(R|D|T|P|V)'> ",
                PhraseType::Adjp,
                true,
            ),
            build(
                r"<posre='PT:.*:.*'> <posre='NN:(Anim|Inanim):.*' & !posre='NN:(Anim|Inanim):.*:(Nom|V)'> <posre='NN:(Anim|Inanim):.*:(Nom|V)' & !posre='NN:(Anim|Inanim):.*:(R|D|T|P)'> ",
                PhraseType::Adjp,
                true,
            ),
            build(
                r"<posre='PT:.*:.*'> <posre='PNN:.*' & !posre='PNN:.*:Nom:.*'> <posre='NN:(Anim|Inanim):.*:(Nom|V)' & !posre='NN:(Anim|Inanim):.*:(R|D|T|P)'> ",
                PhraseType::Adjp,
                true,
            ),
            build(
                r"<posre='PT:.*:.*'> <posre='ADJ:.*:.*'> ",
                PhraseType::Adjp,
                false,
            ),
            build(r"<тов>", PhraseType::Np, false),
        ]
    })
}

fn rules2() -> &'static [crate::openregex::Rule] {
    static RULES: OnceLock<Vec<crate::openregex::Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        vec![
            build(
                r"<posre=NN:Name:.*> <и> <posre=NN:Name:.*>",
                PhraseType::Npp,
                true,
            ),
            build(
                r"<posre=NN:Name:.*> <или> <posre=NN:Name:.*>",
                PhraseType::Npp,
                true,
            ),
            build(
                r"<не> <posre='VB:.*:.*' & !posre='NN:.*'>* ",
                PhraseType::Vp,
                false,
            ),
        ]
    })
}

pub struct RussianChunker;

fn chunkable(atr: &og_core::AnalyzedTokenReadings) -> bool {
    !atr.is_whitespace() && !atr.chunks().iter().any(|c| c == "MayMissingYO")
}

impl RussianChunker {
    pub fn new() -> Self {
        Self
    }

    fn compute_chunks(&self, sentence: &AnalyzedSentence) -> Vec<Vec<String>> {
        let forms: HashSet<String> = sentence_forms(sentence);
        let mut tokens: Vec<Token> = sentence
            .tokens()
            .iter()
            .filter(|t| chunkable(t))
            .map(|t| Token::new(t, vec!["O".to_string()]))
            .collect();
        for rule in rules1() {
            apply(rule, &mut tokens, &forms);
        }
        for rule in rules2() {
            apply(rule, &mut tokens, &forms);
        }
        tokens.into_iter().map(|t| t.chunks).collect()
    }
}

impl Default for RussianChunker {
    fn default() -> Self {
        Self::new()
    }
}

impl og_core::checker::Chunker for RussianChunker {
    fn add_chunk_tags(&self, sentence: &mut AnalyzedSentence) {
        static CACHE: std::sync::OnceLock<std::sync::Mutex<BoundedMap>> =
            std::sync::OnceLock::new();
        let cache_cell = CACHE.get_or_init(|| std::sync::Mutex::new(BoundedMap::new()));
        let mut key = String::new();
        for atr in sentence.tokens() {
            if !chunkable(atr) {
                continue;
            }
            key.push_str(atr.token().token());
            key.push('\u{1}');
            let reading = atr.token();
            for tag in reading.pos_tags() {
                key.push_str(tag);
                key.push('\u{2}');
            }
            for chunk in atr.chunks() {
                key.push_str(chunk);
                key.push('\u{3}');
            }
            key.push('\u{4}');
        }
        if key.is_empty() {
            return;
        }
        let assigned: Vec<Vec<String>> = {
            let mut cache = cache_cell.lock().unwrap();
            if let Some(hit) = cache.get(&key) {
                hit.clone()
            } else {
                let computed = self.compute_chunks(sentence);
                cache.put(key, computed.clone());
                computed
            }
        };
        let sentence_tokens = sentence.tokens_mut();
        let mut idx = 0;
        for token in sentence_tokens.iter_mut() {
            if !chunkable(token) {
                continue;
            }
            if let Some(chunks) = assigned.get(idx) {
                token.set_chunks(chunks.clone());
            }
            idx += 1;
        }
    }
}

pub fn shared_russian_chunker() -> Arc<RussianChunker> {
    static SHARED: std::sync::OnceLock<Arc<RussianChunker>> = std::sync::OnceLock::new();
    SHARED.get_or_init(|| Arc::new(RussianChunker)).clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::AnalyzedToken;
    use og_core::AnalyzedTokenReadings;
    use og_core::checker::Chunker as _;

    fn sentence(parts: &[(&str, &[&str], &[&str])]) -> AnalyzedSentence {
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for (text, tags, chunks) in parts {
            let readings: Vec<AnalyzedToken> = tags
                .iter()
                .map(|t| {
                    AnalyzedToken::new(*text, offset, offset + text.len())
                        .with_pos_tags(vec![t.to_string()])
                })
                .collect();
            let primary = AnalyzedToken::new(*text, offset, offset + text.len())
                .with_pos_tags(tags.iter().map(|t| t.to_string()).collect());
            let mut atr = AnalyzedTokenReadings::new(primary).with_readings(readings);
            atr.set_chunks(chunks.iter().map(|c| c.to_string()).collect());
            tokens.push(atr);
            offset += text.len() + 1;
        }
        let mut s = AnalyzedSentence::new("", 0, offset);
        s.set_tokens(tokens);
        s
    }

    fn chunk_strings(s: &AnalyzedSentence) -> Vec<String> {
        s.tokens()
            .iter()
            .filter(|t| !t.is_whitespace())
            .map(|t| t.chunks().join("|"))
            .collect()
    }

    #[test]
    fn unmatched_tokens_keep_o() {
        let mut s = sentence(&[
            ("дом", &["NN:Inanim:Masc:Sin:Nom"], &[]),
            (".", &["SENT"], &[]),
        ]);
        RussianChunker::new().add_chunk_tags(&mut s);
        let chunks = chunk_strings(&s);
        assert_eq!(chunks[1], "O", "{chunks:?}");
        assert_eq!(chunks[2], "O", "{chunks:?}");
    }

    #[test]
    fn may_missing_yo_tokens_are_skipped() {
        let mut s = sentence(&[
            ("дереве", &["NN:Inanim:Neut:Sin:P"], &["MayMissingYO"]),
            ("упал", &["VB:Act:Past:Masc:Sing"], &[]),
        ]);
        RussianChunker::new().add_chunk_tags(&mut s);
        let chunks = chunk_strings(&s);
        assert_eq!(chunks[1], "MayMissingYO", "{chunks:?}");
        assert!(chunks[2].contains("B-VP"), "{chunks:?}");
    }

    #[test]
    fn adjective_noun_phrase_gets_np() {
        let mut s = sentence(&[
            ("крепкий", &["ADJ:Posit:Masc:Sin:Nom"], &[]),
            ("стол", &["NN:Inanim:Masc:Sin:Nom"], &[]),
        ]);
        RussianChunker::new().add_chunk_tags(&mut s);
        let chunks = chunk_strings(&s);
        assert!(chunks[1].contains("B-NP"), "{chunks:?}");
        assert!(chunks[2].contains("I-NP"), "{chunks:?}");
    }

    #[test]
    fn participle_gets_adjp() {
        let mut s = sentence(&[
            ("казавшийся", &["PT:Past:Masc:Sing:Acc"], &[]),
            ("смуглым", &["NN:Inanim:Masc:Sin:T"], &[]),
            ("человек", &["NN:Anim:Masc:Sin:Nom"], &[]),
        ]);
        RussianChunker::new().add_chunk_tags(&mut s);
        let chunks = chunk_strings(&s);
        assert!(chunks.iter().any(|c| c.contains("ADJP")), "{chunks:?}");
    }

    #[test]
    fn name_conjunction_gets_plural_np() {
        let mut s = sentence(&[
            ("Маша", &["NN:Name:Fem:Sin:Nom"], &[]),
            ("и", &["CONJ"], &[]),
            ("Миша", &["NN:Name:Masc:Sin:Nom"], &[]),
        ]);
        RussianChunker::new().add_chunk_tags(&mut s);
        let chunks = chunk_strings(&s);
        assert!(chunks[1].contains("B-NP-plural"), "{chunks:?}");
        assert!(chunks[3].contains("I-NP-plural"), "{chunks:?}");
    }

    #[test]
    fn subordinating_conjunction_gets_sbar() {
        let mut s = sentence(&[("если", &["CONJ"], &[])]);
        RussianChunker::new().add_chunk_tags(&mut s);
        let chunks = chunk_strings(&s);
        assert!(chunks[1].contains("SBAR"), "{chunks:?}");
    }

    #[test]
    fn all_rules_build() {
        assert_eq!(rules1().len(), 21);
        assert_eq!(rules2().len(), 3);
    }
}
