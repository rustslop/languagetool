use crate::es::test_support::SpanishPipeline;
use og_rules::pattern_rule::PatternRuleEngine;
use og_xml::compiler::XmlCompiler;
use std::path::PathBuf;

fn grammar_path() -> PathBuf {
    PathBuf::from(
        "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/grammar.xml",
    )
}

pub fn dump(text: &str) {
    let pipeline = SpanishPipeline::shared();
    for (sentence, start) in pipeline.analyze_offset_text(text) {
        eprintln!("sentence @{start}: [{}]", sentence.text());
        for t in sentence.tokens() {
            let readings: Vec<String> = t
                .readings()
                .iter()
                .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                .collect();
            let chunks = t.chunks();
            let chunk_str = if chunks.is_empty() {
                String::new()
            } else {
                format!(" chunks={:?}", chunks)
            };
            eprintln!(
                "  [{}..{}] {:?}: {}{}",
                t.token().start(),
                t.token().end(),
                t.token().token(),
                readings.join(", "),
                chunk_str
            );
        }
    }
}

pub fn run_rule(rule_id: &str, text: &str) {
    let xml = std::fs::read_to_string(grammar_path()).expect("grammar.xml");
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, grammar_path().parent())
        .expect("compile");
    let engine = PatternRuleEngine::new();
    let pipeline = SpanishPipeline::shared();
    let mut found = false;
    for rule in &rule_set.rules {
        if rule.id != rule_id {
            continue;
        }
        found = true;
        for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
            for m in engine.match_rule(rule, &sentence) {
                eprintln!(
                    "match @{}..{} (sentence@{sentence_start}): {:?}",
                    m.offset() + sentence_start,
                    m.offset() + sentence_start + m.length(),
                    &text[m.offset() + sentence_start
                        ..(m.offset() + sentence_start + m.length()).min(text.len())]
                );
            }
        }
    }
    if !found {
        eprintln!("no rule with id {rule_id}");
    }
}

pub fn staged_sentence(text: &str) -> og_core::AnalyzedSentence {
    let pipeline = SpanishPipeline::shared();
    let s = text;
    let mut sentence = og_core::AnalyzedSentence::new(s, 0, s.len());
    let mut tokens = Vec::new();
    tokens.push(og_core::AnalyzedTokenReadings::new(
        og_core::AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
    ));
    use og_tokenizer::WordTokenizer as _;
    let tokenizer = crate::es::word_tokenizer::SpanishWordTokenizer::new();
    tokens.extend(tokenizer.tokenize(s).into_iter().map(|t| {
        og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
            t.text(),
            t.start(),
            t.end(),
        ))
    }));
    sentence.set_tokens(tokens);
    let token_texts: Vec<String> = sentence
        .tokens()
        .iter()
        .skip(1)
        .map(|t| t.token().token().to_string())
        .collect();
    let refs: Vec<&str> = token_texts.iter().map(|s| s.as_str()).collect();
    let tagged = og_tagger::Tagger::tag(pipeline.tagger_ref(), &refs);
    let tokens_mut = sentence.tokens_mut();
    for (i, atr) in tagged.into_iter().enumerate() {
        let token_idx = i + 1;
        if token_idx < tokens_mut.len() && !atr.readings().is_empty() {
            let readings = atr.into_readings();
            let merged = og_core::AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
                tokens_mut[token_idx].token().token(),
                tokens_mut[token_idx].token().start(),
                tokens_mut[token_idx].token().end(),
            ))
            .with_readings(readings);
            if !merged.readings().is_empty() {
                tokens_mut[token_idx] = merged;
            }
        }
    }
    sentence
}

pub fn dump_stages(text: &str) {
    use og_core::checker::Disambiguator as _;
    let resource_dir = crate::es::es_resource_dir().expect("es dir");
    let core = crate::engine::find_lt_core_resource_dir().expect("core dir");
    let show = |label: &str, s: &og_core::AnalyzedSentence| {
        eprintln!("== {label} ==");
        for t in s.tokens() {
            if t.token().token().trim().is_empty() {
                continue;
            }
            let readings: Vec<String> = t
                .readings()
                .iter()
                .map(|r| format!("{}/{}", r.lemma().unwrap_or("-"), r.pos_tags().join("|")))
                .collect();
            eprintln!("  {:?}: {}", t.token().token(), readings.join(", "));
        }
    };
    let mut s = staged_sentence(text);
    if let Some(last) = s.tokens_mut().last_mut() {
        last.add_sent_end();
    }
    show("tagged", &s);
    let global = og_tagger::MultiWordChunkerDisambiguator::from_params(
        &core.join("spelling_global.txt"),
        og_tagger::ChunkerParams {
            allow_first_capitalized: false,
            allow_all_uppercase: true,
            allow_titlecase: false,
            default_tag: Some("NPCN000"),
            remove_previous_tags: false,
            add_ignore_spelling: false,
        },
    );
    if let Some(g) = global {
        g.disambiguate(&mut s);
        show("spelling_global", &s);
    }
    let multi = og_tagger::MultiWordChunkerDisambiguator::from_params(
        &resource_dir.join("multiwords.txt"),
        og_tagger::ChunkerParams {
            allow_first_capitalized: true,
            allow_all_uppercase: true,
            allow_titlecase: false,
            default_tag: None,
            remove_previous_tags: true,
            add_ignore_spelling: false,
        },
    );
    if let Some(m) = multi {
        m.disambiguate(&mut s);
        show("multiwords", &s);
    }
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).unwrap();
    let d = og_tagger::XmlDisambiguator::from_xml(&xml).unwrap();
    d.disambiguate(&mut s);
    show("xml", &s);
}

#[test]
#[ignore]
fn probe_gn_readings() {
    let _release_guard = crate::release_guard();
    dump("África es bonito.");
    dump("Los dragones son guay.");
    dump("La tres reglas son estas.");
    dump("vigésimo primera");
}

#[test]
#[ignore]
fn probe_direct_ok_failures() {
    let _release_guard = crate::release_guard();
    dump("hace dos años atrás");
    dump("hace dos semanas y media atrás");
    dump("Alguien explíqueme que sucede aquí.");
    dump("El otro de Inglaterra.");
    dump("medio millones de euros");
}

#[test]
#[ignore]
fn probe_synth_lookups() {
    let _release_guard = crate::release_guard();
    use crate::es::es_resource_dir;
    let dir = es_resource_dir().expect("es dir");
    let dict = og_tagger::morfologik::MorfologikDictionary::load(
        &dir.join("spanish.dict"),
        &dir.join("spanish.info"),
    )
    .expect("dict");
    for w in [
        "estos",
        "este",
        "guapa",
        "guapo",
        "vigésima",
        "primera",
        "África",
        "guay",
    ] {
        eprintln!("lookup({w:?}) = {:?}", dict.lookup(w));
    }
    let synth =
        og_rules::postponed_adjective::LangSynthesizer::for_language(&dir, "es").expect("synth");
    for (lemma, tag) in [
        ("este", "DI0[CM]P0"),
        ("este", "DI0MP0"),
        ("guapo", "A.*"),
        ("guapo", "AQ0FS0"),
        ("20", "AO0FS0"),
    ] {
        let re = regex::Regex::new(tag).unwrap();
        eprintln!(
            "synthesize({lemma:?}, {tag:?}) = {:?}",
            synth.synthesize(lemma, &re)
        );
    }
}

#[test]
fn tokenizer_pins_java_spanish_word_tokenizer_test() {
    let _release_guard = crate::release_guard();
    use og_tokenizer::WordTokenizer as _;
    let t = crate::es::word_tokenizer::SpanishWordTokenizer::new();
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("*test+", vec!["*", "test", "+"]),
        ("e-mails", vec!["e-mails"]),
        ("$100", vec!["$100"]),
        ("$1.000", vec!["$1.000"]),
        ("$1,400.50", vec!["$1,400.50"]),
        ("1,400.50$", vec!["1,400.50$"]),
        ("Ven ‒dijo.", vec!["Ven", " ", "‒", "dijo", "."]),
        (
            "11.as Jornadas de Estudio",
            vec!["11.as", " ", "Jornadas", " ", "de", " ", "Estudio"],
        ),
    ];
    for (text, expected) in cases {
        let tokens = t.tokenize(text);
        let got: Vec<&str> = tokens.iter().map(|x| x.text()).collect();
        assert_eq!(&got, &expected, "tokenizing {text:?}");
    }
    for (text, want_len) in [
        ("1.º", 1usize),
        ("Es la 21.ª y el 45.º", 11),
        ("Es la 21.a y el 45.o", 11),
    ] {
        let n = t.tokenize(text).len();
        assert_eq!(n, want_len, "token count for {text:?}");
    }
    let toks = t.tokenize("best-seller sars-cov-2");
    let texts: Vec<&str> = toks.iter().map(|x| x.text()).collect();
    assert_eq!(texts, vec!["best-seller", " ", "sars-cov-2"]);
}

#[test]
fn sentence_splits_pin_java_spanish_sentence_tokenizer_test() {
    let _release_guard = crate::release_guard();
    use og_tokenizer::SentenceTokenizer as _;
    let splitter = og_tokenizer::srx::srx_splitter("es").expect("es srx");
    let cases: Vec<(&str, Vec<&str>)> = vec![
        (
            "Esto es una frase. Esto es otra frase.",
            vec!["Esto es una frase. ", "Esto es otra frase."],
        ),
        (
            "¿Nos vamos? Hay que irse.",
            vec!["¿Nos vamos? ", "Hay que irse."],
        ),
        ("1. Artículo primero", vec!["1. Artículo primero"]),
        ("Entonces... apareció él.", vec!["Entonces... apareció él."]),
        (
            "A la atención de A. Comes.",
            vec!["A la atención de A. Comes."],
        ),
        ("las Sras. diputadas", vec!["las Sras. diputadas"]),
        ("El Dr. Joan no vendrá.", vec!["El Dr. Joan no vendrá."]),
        (
            "En EE. UU. por los DD. HH. después de los JJ. OO.",
            vec!["En EE. UU. por los DD. HH. después de los JJ. OO."],
        ),
        (
            "Me voy a EE. UU. Buen viaje.",
            vec!["Me voy a EE. UU. ", "Buen viaje."],
        ),
        (
            "Cebolla, ajo, calabaza, etc. Compramos en fruitería.",
            vec!["Cebolla, ajo, calabaza, etc. ", "Compramos en fruitería."],
        ),
        (
            "Son de 1 g. Han sido acondicionadas.",
            vec!["Son de 1 g. ", "Han sido acondicionadas."],
        ),
        (
            "El acto será a las 15.30 h. de la tarde.",
            vec!["El acto será a las 15.30 h. de la tarde."],
        ),
        (
            "cuando G.Oueddei se convierte en líder",
            vec!["cuando G.Oueddei se convierte en líder"],
        ),
        (
            "El jesuita alemán J.E. Nithard",
            vec!["El jesuita alemán J.E. Nithard"],
        ),
        (
            "Art. 2.1: Estarán obligados...",
            vec!["Art. 2.1: Estarán obligados..."],
        ),
        (
            "Hola, Albert: Me puedes decir tu correo?",
            vec!["Hola, Albert: ", "Me puedes decir tu correo?"],
        ),
        (
            "Mi profesora trabaja los lun., mié. y vie.; los juev. y los dom., no.",
            vec!["Mi profesora trabaja los lun., mié. y vie.; los juev. y los dom., no."],
        ),
    ];
    for (text, expected) in cases {
        let got: Vec<String> = splitter
            .split(text)
            .iter()
            .map(|s| text[s.start()..s.end()].to_string())
            .collect();
        assert_eq!(got, expected, "splitting {text:?}");
    }
}

#[test]
fn disambiguator_expands_external_entities() {
    let _release_guard = crate::release_guard();
    let pipeline = crate::es::test_support::SpanishPipeline::shared();
    let sentence = &pipeline.analyze_text("Me junto con él en el patio.")[0];
    let junto = sentence
        .tokens()
        .iter()
        .find(|t| t.token().token() == "junto")
        .expect("junto token");
    assert!(
        junto
            .readings()
            .iter()
            .flat_map(|r| r.pos_tags().iter())
            .any(|t| t.starts_with("VM")),
        "junto_a must be vetoed after a clitic pronoun: {:?}",
        junto.readings()
    );
}

#[test]
fn perifrasis_verbal_addchunk_rules_do_not_strip_interim_readings() {
    let _release_guard = crate::release_guard();
    let pipeline = crate::es::test_support::SpanishPipeline::shared();
    let sentence = &pipeline.analyze_text("Nos hemos ido desarrollado")[0];
    let ido = sentence
        .tokens()
        .iter()
        .find(|t| t.token().token() == "ido")
        .expect("ido token");
    assert!(
        ido.readings()
            .iter()
            .flat_map(|r| r.pos_tags().iter())
            .any(|t| t == "_reflexive"),
        "reflexive_verb interim tag must survive: {:?}",
        ido.readings()
    );
}
#[test]
#[ignore]
fn probe_comillas_apostrofo_quote_equality_divergence() {
    let _release_guard = crate::release_guard();
    run_rule(
        "COMILLAS_APOSTROFO",
        "Ha ilustrado ‘El Hobbit’, ‘El Señor de los Anillos‘ y ‘Los hijos de Húrin’.",
    );
    run_rule(
        "COMILLAS_APOSTROFO",
        "Los historiales clínicos (‘medical records’)",
    );
}

#[test]
#[ignore]
fn probe_postponed_antipattern_suppression() {
    let _release_guard = crate::release_guard();
    let xml = std::fs::read_to_string(grammar_path()).expect("grammar.xml");
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, grammar_path().parent())
        .expect("compile");
    for r in rule_set
        .rules
        .iter()
        .filter(|r| r.id == "AGREEMENT_POSTPONED_ADJ")
    {
        eprintln!(
            "sub-rule {:?}: antipatterns={} examples={} first_example={:?}",
            r.sub_id,
            r.antipatterns.len(),
            r.examples.len(),
            r.examples.first().map(|e| e.text.clone())
        );
    }
    let big = rule_set
        .rules
        .iter()
        .find(|r| r.id == "AGREEMENT_POSTPONED_ADJ" && r.sub_id.as_deref() == Some("3"))
        .expect("big postponed sub-rule 3");
    eprintln!(
        "sub-rule 3: antipatterns={} pattern_tokens={}",
        big.antipatterns.len(),
        big.pattern.tokens.len()
    );
    let engine = PatternRuleEngine::new();
    let pipeline = SpanishPipeline::shared();
    let text = "El domingo pasada hubo corrida de toros de lujo.";
    let count_with = |antis: &[og_xml::compiler::CompiledPattern]| -> usize {
        let mut probe = big.clone();
        probe.antipatterns = antis.to_vec();
        let mut total = 0;
        for (sentence, _) in pipeline.analyze_offset_text(text) {
            total += engine.match_rule(&probe, &sentence).len();
        }
        total
    };
    eprintln!("all: {}", count_with(&big.antipatterns));
    let mut lo = 0usize;
    let mut hi = big.antipatterns.len();
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let subset = big.antipatterns[lo..mid].to_vec();
        if count_with(&subset) == 0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    eprintln!("suppressing antipattern index in sub-rule: {lo}");
    let anti = &big.antipatterns[lo];
    for (i, tok) in anti.tokens.iter().enumerate() {
        eprintln!("  anti[{i}] = {:?}", tok);
    }
    eprintln!("  structured={}", anti.has_structured_elements);
    for (i, el) in anti.elements.iter().enumerate() {
        eprintln!("  anti-element[{i}] = {:?}", el);
    }
}

#[test]
#[ignore]
fn probe_minimal_postponed_element() {
    let _release_guard = crate::release_guard();
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rules lang="es">
    <rule id="T1">
        <pattern>
            <token postag="AQ.*|V.P.*|PX.*" postag_regexp="yes"><exception postag="_GN_.*|ignore_concordance|AQ.CN.|_GV_" postag_regexp="yes"/><exception regexp="yes">primero|juntos|juntas|dado|dada|dados|dadas|incluso|valgo|valga</exception><exception regexp="yes">\p{Lu}.*</exception></token>
        </pattern>
    </rule>
    <rule id="T2">
        <pattern>
            <token postag="AQ.*|V.P.*|PX.*" postag_regexp="yes"/>
        </pattern>
    </rule>
    <rule id="T3">
        <pattern>
            <token postag="V.P.*" postag_regexp="yes"/>
        </pattern>
    </rule>
</rules>
"#;
    let path = std::path::PathBuf::from("/tmp/es_t1_probe.xml");
    std::fs::write(&path, xml).unwrap();
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(xml, path.parent())
        .expect("compile");
    let engine = PatternRuleEngine::new();
    let pipeline = SpanishPipeline::shared();
    for text in [
        "El domingo pasada hubo corrida.",
        "El domingo pasada hubo corrida de toros de lujo.",
        "La casa del pueblo blancas.",
        "Abandonaron el país transcurridas diez semanas.",
    ] {
        for rule in &rule_set.rules {
            let mut hits = 0;
            for (sentence, _) in pipeline.analyze_offset_text(text) {
                for m in engine.match_rule(rule, &sentence) {
                    eprintln!(
                        "{} on {:?} @{}..{}",
                        rule.id,
                        text,
                        m.offset(),
                        m.offset() + m.length()
                    );
                    hits += 1;
                }
            }
            if hits == 0 {
                eprintln!("{} on {:?}: NO MATCH", rule.id, text);
            }
        }
    }
}

#[test]
#[ignore]
fn probe_raw_dict_lookups() {
    let _release_guard = crate::release_guard();
    use crate::es::es_resource_dir;
    let dir = es_resource_dir().expect("es dir");
    let dict = og_tagger::morfologik::MorfologikDictionary::load(
        &dir.join("spanish.dict"),
        &dir.join("spanish.info"),
    )
    .expect("dict");
    for w in [
        "maquina",
        "maquinar",
        "practicas",
        "Entrega",
        "productos",
        "penca",
        "centrales",
        "tsunami",
        "anime",
        "pasada",
        "pasado",
        "motriz",
    ] {
        eprintln!("lookup({w:?}) = {:?}", dict.lookup(w));
    }
    if let Some(shared) = og_tagger::generic_tagger::shared_dictionary(
        &crate::es::es_resource_dir().expect("es dir"),
        "spanish",
    ) {
        for w in ["maquina", "máquina", "maquinas"] {
            eprintln!("shared lookup({w:?}) = {:?}", shared.lookup(w));
        }
    }
    let tagger = crate::es::tagger::SpanishTagger::shared().expect("tagger");
    for w in ["maquina", "máquina"] {
        eprintln!("tagger lookup({w:?}) = {:?}", tagger.lookup(w));
    }
}

#[test]
#[ignore]
fn probe_all_tagged_no_match_families() {
    let _release_guard = crate::release_guard();
    run_rule("MI_TILDE", "Y a mi solo mil.");
    dump("Y a mi solo mil.");
    run_rule("MI_TILDE", "Y a mi mil.");
    dump("Y a mi mil.");
    run_rule("EL_MENOS", "Lo hizo el menos tres veces.");
    dump("Lo hizo el menos tres veces.");
    run_rule("EL_MENOS", "Yo tengo el menos cuatro.");
    run_rule("HACIA_TILDE", "Hacia dos años que no se veían.");
    dump("Hacia dos años que no se veían.");
    run_rule(
        "DIACRITICS_VERB_N_ADJ",
        "La extraña maquina produjo lo necesario.",
    );
    dump("La extraña maquina produjo lo necesario.");
    run_rule("AGREEMENT_POSTPONED_ADJ", "Entrega productos.");
    dump("Entrega productos.");
    run_rule("AGREEMENT_POSTPONED_ADJ", "El impulso motriz.");
    dump("El impulso motriz.");
    run_rule(
        "AGREEMENT_POSTPONED_ADJ",
        "El domingo pasada hubo corrida de toros de lujo.",
    );
    dump("El domingo pasada hubo corrida de toros de lujo.");
    run_rule("AGREEMENT_POSTPONED_ADJ", "La casa del pueblo blancas.");
    dump("La casa del pueblo blancas.");
    run_rule(
        "COMILLAS_APOSTROFO",
        "Ha ilustrado ‘El Hobbit’, ‘El Señor de los Anillos‘ y ‘Los hijos de Húrin’.",
    );
    dump("Ha ilustrado ‘El Hobbit’, ‘El Señor de los Anillos‘ y ‘Los hijos de Húrin’.");
}

#[test]
#[ignore]
fn probe_mydia01_fp() {
    let _release_guard = crate::release_guard();
    run_rule(
        "MYDIA01",
        "Sin embargo, si capitulo, es en verdad por pura debilidad;",
    );
    dump("Sin embargo, si capitulo, es en verdad por pura debilidad;");
}

#[test]
#[ignore]
fn probe_diacritics02_no_match() {
    let _release_guard = crate::release_guard();
    run_rule("DIACRITICS_02", "Maquina de pelar patatas.");
    dump_stages("Maquina de pelar patatas.");
}

#[test]
#[ignore]
fn probe_el_tilde_fp() {
    let _release_guard = crate::release_guard();
    run_rule(
        "EL_TILDE",
        "ni siquiera el anime era lo suficientemente expresivo",
    );
    dump("ni siquiera el anime era lo suficientemente expresivo");
}

#[test]
#[ignore]
fn probe_que_tilde2_fp() {
    let _release_guard = crate::release_guard();
    run_rule(
        "QUE_TILDE2",
        "con la salvedad de que dicha comunicación de datos este amparada en una obligación legal",
    );
    dump(
        "con la salvedad de que dicha comunicación de datos este amparada en una obligación legal",
    );
}

#[test]
#[ignore]
fn probe_no_separado_fp() {
    let _release_guard = crate::release_guard();
    run_rule("NO_SEPARADO", "en la tonalidad de re bemol menor.");
    run_rule("NO_SEPARADO", "Se re activa.");
    dump("en la tonalidad de re bemol menor.");
}

#[test]
#[ignore]
fn probe_hace_un_mes_atras_no_match() {
    let _release_guard = crate::release_guard();
    run_rule("HACE_UN_MES_ATRAS", "hace dos años atrás");
    dump("hace dos años atrás");
}

#[test]
#[ignore]
fn probe_agreement_ordinals_no_match() {
    let _release_guard = crate::release_guard();
    run_rule("AGREEMENT_ORDINALS", "vigésimo primera");
    run_rule("AGREEMENT_ORDINALS", "vigésimo primeros");
    dump("vigésimo primera");
    dump("vigésimo primeros");
    use crate::es::es_resource_dir;
    let dir = es_resource_dir().expect("es dir");
    let dict = og_tagger::morfologik::MorfologikDictionary::load(
        &dir.join("spanish.dict"),
        &dir.join("spanish.info"),
    )
    .expect("dict");
    for w in ["vigésimo", "vigesimo", "primera", "primeros", "décimo"] {
        eprintln!("lookup({w:?}) = {:?}", dict.lookup(w));
    }
    if let Some(fetched) = og_tagger::morfologik::MorfologikDictionary::load(
        &std::path::PathBuf::from("/tmp/current_spanish.dict"),
        &std::path::PathBuf::from("/tmp/current_spanish.info"),
    ) {
        for w in ["vigésimo", "primera", "primeros", "décimo"] {
            eprintln!("fetched lookup({w:?}) = {:?}", fetched.lookup(w));
        }
    }
}

#[test]
#[ignore]
fn probe_agreement_det_noun_medio_no_match() {
    let _release_guard = crate::release_guard();
    run_rule("AGREEMENT_DET_NOUN", "medio millones de euros");
    dump("medio millones de euros");
    use crate::es::es_resource_dir;
    let dir = es_resource_dir().expect("es dir");
    let dict = og_tagger::morfologik::MorfologikDictionary::load(
        &dir.join("spanish.dict"),
        &dir.join("spanish.info"),
    )
    .expect("dict");
    for w in ["medio", "millones", "millón"] {
        eprintln!("lookup({w:?}) = {:?}", dict.lookup(w));
    }
    if let Some(fetched) = og_tagger::morfologik::MorfologikDictionary::load(
        &std::path::PathBuf::from("/tmp/current_spanish.dict"),
        &std::path::PathBuf::from("/tmp/current_spanish.info"),
    ) {
        for w in ["medio", "millones", "millón", "del", "dos"] {
            eprintln!("fetched lookup({w:?}) = {:?}", fetched.lookup(w));
        }
    }
}
