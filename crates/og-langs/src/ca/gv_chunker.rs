use og_core::{AnalyzedSentence, AnalyzedTokenReadings};

enum TokenSpec {
    Lemma(&'static [&'static str]),
    Postag(&'static str),
    PostagAndLemma(&'static str, &'static [&'static str]),
    PostagNotPostag(&'static str, &'static str),
    Text(&'static [&'static str]),
    PostagText(&'static str, &'static [&'static str]),
}

struct GvRule {
    tokens: &'static [TokenSpec],
}

fn has_any_lemma(atr: &AnalyzedTokenReadings, lemmas: &[&str]) -> bool {
    lemmas
        .iter()
        .any(|l| atr.readings().iter().any(|r| r.lemma() == Some(*l)))
}

fn is_text(atr: &AnalyzedTokenReadings, alternatives: &[&str]) -> bool {
    let surface = atr.token().token().to_lowercase();
    alternatives.iter().any(|a| *a == surface)
}

fn spec_matches(spec: &TokenSpec, atr: &AnalyzedTokenReadings) -> bool {
    match spec {
        TokenSpec::Lemma(lemmas) => has_any_lemma(atr, lemmas),
        TokenSpec::Postag(postag) => atr.has_pos_tag_matching(postag),
        TokenSpec::PostagAndLemma(postag, lemmas) => {
            atr.has_pos_tag_matching(postag) && has_any_lemma(atr, lemmas)
        }
        TokenSpec::PostagNotPostag(postag, excluded) => {
            atr.has_pos_tag_matching(postag) && !atr.has_pos_tag_matching(excluded)
        }
        TokenSpec::Text(alternatives) => is_text(atr, alternatives),
        TokenSpec::PostagText(postag, alternatives) => {
            atr.has_pos_tag_matching(postag) && is_text(atr, alternatives)
        }
    }
}

const RULE_HAVER_DE_PARAR: &[TokenSpec] = &[
    TokenSpec::Lemma(&["haver", "deixar", "parar"]),
    TokenSpec::Text(&["de", "d'"]),
    TokenSpec::Postag("V.N.*"),
];
const RULE_HAVER_PARTICIPI: &[TokenSpec] = &[
    TokenSpec::Lemma(&["haver"]),
    TokenSpec::PostagNotPostag("V.P.*", "[NA].*"),
];
const RULE_CALDRE: &[TokenSpec] = &[
    TokenSpec::PostagAndLemma("V...3S..", &["caldre"]),
    TokenSpec::Postag("V.N.*"),
];
const RULE_ANAR_INF: &[TokenSpec] = &[
    TokenSpec::PostagAndLemma("VA.*", &["anar"]),
    TokenSpec::Postag("V.N.*"),
];
const RULE_GERUNDI: &[TokenSpec] = &[
    TokenSpec::PostagAndLemma(
        "V.*",
        &[
            "continuar",
            "seguir",
            "estar",
            "anar",
            "prosseguir",
            "acabar",
            "dur",
            "portar",
        ],
    ),
    TokenSpec::Postag("V.G.*"),
];
const RULE_MODAL_INFINITIU: &[TokenSpec] = &[
    TokenSpec::PostagAndLemma(
        "V.*",
        &[
            "poder", "voler", "desitjar", "deure", "saber", "pensar", "fer", "tocar",
        ],
    ),
    TokenSpec::Postag("V.N.*"),
];
const RULE_TORNAR_A: &[TokenSpec] = &[
    TokenSpec::PostagAndLemma(
        "V.*",
        &["tornar", "començar", "anar", "arribar", "aplegar", "venir"],
    ),
    TokenSpec::Text(&["a"]),
    TokenSpec::Postag("V.N.*"),
];
const RULE_ARRIBAR_MAI_A: &[TokenSpec] = &[
    TokenSpec::PostagAndLemma("V.*", &["arribar", "aplegar"]),
    TokenSpec::Text(&["mai"]),
    TokenSpec::Text(&["a"]),
    TokenSpec::Postag("V.N.*"),
];
const RULE_ACABAR_PER: &[TokenSpec] = &[
    TokenSpec::PostagAndLemma("V.*", &["acabar"]),
    TokenSpec::Text(&["per", "de", "d'"]),
    TokenSpec::Postag("V.N.*"),
];
const RULE_DEIXAR_FER: &[TokenSpec] = &[
    TokenSpec::PostagAndLemma("V.*", &["deixar"]),
    TokenSpec::PostagText("V.N.*", &["fer", "passar", "anar"]),
];

const GV_RULES: &[GvRule] = &[
    GvRule {
        tokens: RULE_HAVER_DE_PARAR,
    },
    GvRule {
        tokens: RULE_HAVER_PARTICIPI,
    },
    GvRule {
        tokens: RULE_CALDRE,
    },
    GvRule {
        tokens: RULE_ANAR_INF,
    },
    GvRule {
        tokens: RULE_GERUNDI,
    },
    GvRule {
        tokens: RULE_MODAL_INFINITIU,
    },
    GvRule {
        tokens: RULE_TORNAR_A,
    },
    GvRule {
        tokens: RULE_ARRIBAR_MAI_A,
    },
    GvRule {
        tokens: RULE_ACABAR_PER,
    },
    GvRule {
        tokens: RULE_DEIXAR_FER,
    },
];

pub fn add_gv_chunks(sentence: &mut AnalyzedSentence) {
    let non_whitespace: Vec<usize> = sentence
        .tokens()
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.is_whitespace() && !t.is_sentence_start())
        .map(|(i, _)| i)
        .collect();
    let mut gv_flags = vec![false; non_whitespace.len()];
    for rule in GV_RULES {
        let len = rule.tokens.len();
        if non_whitespace.len() < len {
            continue;
        }
        for start in 0..=non_whitespace.len() - len {
            let all_match = (0..len).all(|k| {
                let atr = &sentence.tokens()[non_whitespace[start + k]];
                spec_matches(&rule.tokens[k], atr)
            });
            if all_match {
                for k in 0..len {
                    gv_flags[start + k] = true;
                }
            }
        }
    }
    let tokens = sentence.tokens_mut();
    for (gv, orig_idx) in gv_flags.iter().zip(non_whitespace.iter()) {
        if *gv && !tokens[*orig_idx].has_chunk("GV") {
            let mut chunks = tokens[*orig_idx].chunks().to_vec();
            chunks.push("GV".to_string());
            tokens[*orig_idx].set_chunks(chunks);
        }
    }
}

pub struct CatalanGVChunker;

impl og_core::checker::Chunker for CatalanGVChunker {
    fn add_chunk_tags(&self, sentence: &mut AnalyzedSentence) {
        add_gv_chunks(sentence);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::AnalyzedToken;

    fn token(word: &str, lemma: &str, tag: &str, offset: &mut usize) -> AnalyzedTokenReadings {
        let start = *offset;
        let end = start + word.len();
        *offset = end + 1;
        AnalyzedTokenReadings::new(AnalyzedToken::new(word, start, end)).with_readings(vec![
            AnalyzedToken::new(word, start, end)
                .with_lemma(lemma.to_string())
                .with_pos_tags(vec![tag.to_string()]),
        ])
    }

    fn sentence_from(tokens: Vec<AnalyzedTokenReadings>, text: &str) -> AnalyzedSentence {
        let mut s = AnalyzedSentence::new(text, 0, text.len());
        s.set_tokens(tokens);
        s
    }

    #[test]
    fn haver_de_infinitiu_gets_gv_on_all_three_tokens() {
        let mut offset = 0;
        let tokens = vec![
            token("ha", "haver", "VMIP3S00", &mut offset),
            token("de", "de", "SPS00", &mut offset),
            token("fer", "fer", "VMN0000", &mut offset),
        ];
        let mut sentence = sentence_from(tokens, "ha de fer");
        add_gv_chunks(&mut sentence);
        assert!(sentence.tokens().iter().all(|t| t.has_chunk("GV")));
    }

    #[test]
    fn plain_nouns_get_no_chunk() {
        let mut offset = 0;
        let tokens = vec![
            token("la", "el", "DA0FS0", &mut offset),
            token("casa", "casa", "NCFS000", &mut offset),
        ];
        let mut sentence = sentence_from(tokens, "la casa");
        add_gv_chunks(&mut sentence);
        assert!(sentence.tokens().iter().all(|t| !t.has_chunk("GV")));
    }

    #[test]
    fn anar_a_infinitiu_marks_the_perifrasi() {
        let mut offset = 0;
        let tokens = vec![
            token("va", "anar", "VMIP3S00", &mut offset),
            token("a", "anar", "VMN0000", &mut offset),
            token("veure", "veure", "VMN0000", &mut offset),
        ];
        let mut sentence = sentence_from(tokens, "va a veure");
        add_gv_chunks(&mut sentence);
        assert!(sentence.tokens().iter().all(|t| t.has_chunk("GV")));
    }

    #[test]
    fn noun_reading_blocks_the_participle_rule() {
        let mut offset = 0;
        let ha = token("ha", "haver", "VMIP3S00", &mut offset);
        let start = offset;
        let end = start + "dat".len();
        let dat =
            AnalyzedTokenReadings::new(AnalyzedToken::new("dat", start, end)).with_readings(vec![
                AnalyzedToken::new("dat", start, end)
                    .with_lemma("dar".to_string())
                    .with_pos_tags(vec!["VMP00SF".to_string()]),
                AnalyzedToken::new("dat", start, end)
                    .with_lemma("data".to_string())
                    .with_pos_tags(vec!["NCFS000".to_string()]),
            ]);
        let mut sentence = sentence_from(vec![ha, dat], "ha dat");
        add_gv_chunks(&mut sentence);
        assert!(
            !sentence.tokens()[1].has_chunk("GV"),
            "the [NA].* exception must block the perifrasi"
        );
    }
}
