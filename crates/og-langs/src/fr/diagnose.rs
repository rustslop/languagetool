#![cfg(test)]

use crate::fr::example_suite::build_pattern_engine;
use crate::fr::test_support::FrenchPipeline;
use og_tokenizer::WordTokenizer as _;
use og_xml::compiler::XmlCompiler;

fn tokenizer_probe() {
    let pipeline = FrenchPipeline::shared();
    for text in [
        "Il est né le 32.1.1985.",
        "Il est né le 31.4.1989.",
        "Une croissance de 8%.",
        "Une croissance de 8,5%.",
        "-il est arrivé à l'hôtel hier.",
        "D‘enfants.",
        "Vas -tu les regarder à la télévision ?",
        "S’il vous -plaît, dessine-moi un mouton.",
        "S’il-vous plaît, dessine-moi un mouton.",
        "J'utilise google.com.",
        "La surface du mur ne s'abîme.",
        "The pia- nist",
        "alors il faut partir",
        "dans ce cas il est certain",
        "Le néo-impressionnisme",
        "Miguel Diaz-Canel est porté sur les fonts néo-post-castristes.",
        "Pour revenir au post précédent.",
    ] {
        let tokens: Vec<String> = pipeline
            .tokenizer_ref()
            .tokenize(text)
            .iter()
            .map(|t| format!("{:?}", t.text()))
            .collect();
        println!("TOKENS {text:?} => {}", tokens.join(" "));
    }
}

fn tagger_probe() {
    let tagged = og_tagger::Tagger::tag(
        FrenchPipeline::shared().tagger_ref(),
        &[
            "du", "savoir", "dire", "lever", "repentir", "lancer", "voies",
        ],
    );
    for atr in tagged {
        let readings: Vec<String> = atr
            .readings()
            .iter()
            .map(|r| format!("{}/{}", r.lemma().unwrap_or("null"), r.pos_tags().join(" ")))
            .collect();
        println!("TAG {:?} => {}", atr.token().token(), readings.join(" | "));
    }
}

fn readings_probe() {
    let pipeline = FrenchPipeline::shared();
    for text in [
        "Je n’aime guerre le sport.",
        "Il n’y en a guerre.",
        "C'est lui qui ce cache sous la table.",
        "Il a décidé de ce lever tôt.",
        "Il s’en suivit de grands troubles.",
        "Le développement qui s’en est suivi fut remarquable.",
        "J’ai pas compris.",
        "Je ne sé compris.",
        "C’est arrosoir contient 15 litres.",
        "Qu’est ce que tu dis ?",
        "Demander jusqu’à ce qu’il me répond.",
        "J’aimerai bien travailler ici.",
        "C'est trop, tu n'aurais pas du savoir !",
        "Voici la piste qu'aurait négligé les enqêteurs.",
        "Il devient de plus en plus.",
        "La défaite s’en suivit.",
        "Il le fait poursoi.",
        "Il se peut qu’il soi en retard.",
        "Dominique peux venir.",
        "Nous l'avons mangés.",
        "Un orage semble annoncée.",
        "Il parle d’avantage de toi.",
        "Quand j'étais petit, je jouais au football.",
        "Je suis d'abord allé au laboratoire.",
    ] {
        println!("READINGS {text:?}");
        for sentence in pipeline.analyze_text(text) {
            for t in sentence.tokens() {
                if t.is_whitespace() || t.token().token() == "<S>" {
                    continue;
                }
                let readings: Vec<String> = t
                    .readings()
                    .iter()
                    .map(|r| format!("{}/{}", r.lemma().unwrap_or("null"), r.pos_tags().join(" ")))
                    .collect();
                let chunks = t.chunks().join(",");
                println!(
                    "  {:?} [{}..{}] = {} | chunks: {}",
                    t.token().token(),
                    t.token().start(),
                    t.token().end(),
                    readings.join(" | "),
                    chunks
                );
            }
        }
    }
}

fn rule_match_probe() {
    let xml = std::fs::read_to_string(
        crate::fr::test_support::fr_rules_dir()
            .expect("fr rules dir")
            .join("grammar.xml"),
    )
    .expect("grammar.xml");
    let rule_set = XmlCompiler::new()
        .compile_file_with_base(&xml, crate::fr::test_support::fr_rules_dir().as_deref())
        .expect("compile grammar.xml");
    let engine = build_pattern_engine();
    let pipeline = FrenchPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);

    let probes: &[(&str, &str)] = &[
        ("DATE", "Il est né le 32.1.1985."),
        ("DATE", "Elle est née le 29.2.1900."),
        ("ESPACE_POURCENT", "Une croissance de 8%."),
        ("ALORS", "alors il faut partir"),
        ("DANS_CE_CAS", "dans ce cas il est certain"),
        ("FR_SPLIT_WORDS_HYPHEN", "The pia- nist"),
        ("PREFIXES_NON_SEPARES", "Pour revenir au post précédent."),
        ("CE_SE", "C'est lui qui ce cache sous la table."),
        ("GUERE_GUERRE", "Je n’aime guerre le sport."),
        ("LA_LA2", "La personne photographiée est à la fois :"),
        (
            "OU",
            "Je vous écrirai ou vous téléphonerai la semaine prochaine.",
        ),
        ("PAS_DE_TRAIT_UNION", "Le néo-impressionnisme"),
        ("CONF_N_V", "J'utilise google.com."),
        ("APOS_ESPACE", "La surface du mur ne s'abîme."),
        ("MOTS_MAUX", "Antidote, le remède à tous vos mots."),
        ("MOTS_MAUX", "Docteur délivrez moi de mes mots."),
        ("DU_DU", "C'est trop, tu n'aurais pas du savoir !"),
        ("CE_SE", "Il a décidé de ce lever tôt."),
        ("ACCORD_SUJET_VERBE", "Dominique peux venir."),
        ("ACCORD_SUJET_VERBE", "L'enfant dis merci."),
        ("ETRE_VPPA_OU_ADJ", "Un orage semble annoncée."),
        ("AUX_AVOIR", "Nous l'avons mangés."),
    ];
    for (id, text) in probes {
        println!("MATCH {id} on {text:?}");
        let mut any = false;
        for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
            for rule in &rule_set.rules {
                if rule.id != *id {
                    continue;
                }
                let fr_filter = rule.filter.as_ref().filter(|f| {
                    f.class == crate::fr::suppress_misspelled::FRENCH_SUPPRESS_MISSPELLED_FILTER_CLASS
                });
                for m in engine.match_rule(rule, &sentence) {
                    let Some(m) = (match fr_filter {
                        Some(filter) => {
                            crate::fr::suppress_misspelled::accept_rule_match(m, &filter.args, &[])
                        }
                        None => Some(m),
                    }) else {
                        continue;
                    };
                    any = true;
                    println!(
                        "  sub {:?} => {}..{} msg {:?} sugg {:?}",
                        rule.sub_id,
                        m.offset() + sentence_start,
                        m.offset() + sentence_start + m.length(),
                        m.message(),
                        m.replacements()
                            .iter()
                            .map(|r| r.value().to_string())
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
        if !any {
            println!("  (no match)");
        }
    }
}

fn disambig_bisect_probe() {
    disambig_bisect(
        "Il a décidé de ce lever tôt.",
        &["ce", "lever", "cache"],
        &["ce", "lever"],
    );
    disambig_bisect(
        "Je vous écrirai ou vous téléphonerai la semaine prochaine.",
        &["vous", "écrirai", "téléphonerai"],
        &["vous", "écrirai", "téléphonerai"],
    );
    disambig_bisect(
        "Ses grandes fleurs doubles sont roses à nuance lilas et très parfumées.",
        &["roses"],
        &["roses"],
    );
}

fn disambig_bisect(text: &str, show_tokens: &[&str], watch_tokens: &[&str]) {
    use og_core::checker::Disambiguator as _;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
    use og_tagger::multiword_chunker::{ChunkerParams, MultiWordChunkerDisambiguator};
    use og_tokenizer::WordTokenizer as _;

    let resource_dir = crate::fr::test_support::fr_resource_dir().expect("fr resource dir");
    let core = crate::engine::find_lt_core_resource_dir().expect("core resource dir");
    let pipeline = FrenchPipeline::shared();
    let mut sentence = {
        let mut s = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.extend(
            pipeline
                .tokenizer_ref()
                .tokenize(text)
                .into_iter()
                .map(|t| {
                    AnalyzedTokenReadings::new(AnalyzedToken::new(t.text(), t.start(), t.end()))
                }),
        );
        s.set_tokens(tokens);
        let refs: Vec<&str> = s
            .tokens()
            .iter()
            .skip(1)
            .map(|t| t.token().token())
            .collect();
        let tagged = og_tagger::Tagger::tag(pipeline.tagger_ref(), &refs);
        let tokens_mut = s.tokens_mut();
        for (i, atr) in tagged.into_iter().enumerate() {
            let idx = i + 1;
            let start = tokens_mut[idx].token().start();
            let end = tokens_mut[idx].token().end();
            let chunks = atr.chunks().to_vec();
            let token_text = atr.token().token().to_string();
            let readings = atr.into_readings();
            if readings.is_empty() {
                continue;
            }
            let mut merged = AnalyzedTokenReadings::new(
                AnalyzedToken::new(token_text, start, end)
                    .with_pos_tags(
                        readings
                            .iter()
                            .flat_map(|r| r.pos_tags().to_vec())
                            .collect::<Vec<_>>(),
                    )
                    .with_lemma(readings[0].lemma().unwrap_or_default().to_string()),
            )
            .with_readings(readings);
            if !chunks.is_empty() {
                merged.set_chunks(chunks);
            }
            tokens_mut[idx] = merged;
        }
        s
    };
    let show = |stage: &str, s: &AnalyzedSentence| {
        for t in s.tokens() {
            if show_tokens.contains(&t.token().token()) {
                println!(
                    "{stage} {:?}@{} => {}",
                    t.token().token(),
                    t.token().start(),
                    t.readings()
                        .iter()
                        .map(|r| format!(
                            "{}/{}",
                            r.lemma().unwrap_or("null"),
                            r.pos_tags().join(" ")
                        ))
                        .collect::<Vec<_>>()
                        .join(" | ")
                );
            }
        }
    };
    println!("BISECT {text:?}");
    show("TAGGED", &sentence);
    let global = MultiWordChunkerDisambiguator::from_params(
        &core.join("spelling_global.txt"),
        ChunkerParams {
            allow_first_capitalized: false,
            allow_all_uppercase: true,
            allow_titlecase: false,
            default_tag: Some("_NONE_"),
            remove_previous_tags: false,
            add_ignore_spelling: true,
        },
    )
    .expect("spelling_global chunker");
    global.disambiguate(&mut sentence);
    show("GLOBAL", &sentence);
    let chunker = MultiWordChunkerDisambiguator::from_params(
        &resource_dir.join("multiwords.txt"),
        ChunkerParams {
            allow_first_capitalized: true,
            allow_all_uppercase: true,
            allow_titlecase: false,
            default_tag: None,
            remove_previous_tags: true,
            add_ignore_spelling: false,
        },
    )
    .expect("multiwords chunker");
    chunker.disambiguate(&mut sentence);
    show("CHUNKER", &sentence);
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).expect("xml");
    let disambiguator = og_tagger::XmlDisambiguator::from_xml(&xml).expect("parse");
    let snapshot = |s: &AnalyzedSentence| {
        String::new()
            + &s.tokens()
                .iter()
                .filter(|t| watch_tokens.contains(&t.token().token()))
                .map(|t| {
                    format!(
                        "{}@{}={}",
                        t.token().token(),
                        t.token().start(),
                        t.readings()
                            .iter()
                            .map(|r| r.pos_tags().first().map_or("?".to_string(), |p| p.clone()))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect::<Vec<_>>()
                .join(" ; ")
    };
    let mut prev = snapshot(&sentence);
    for rule_idx in 0..disambiguator.rule_count() {
        disambiguator.apply_rules_through(&mut sentence, rule_idx + 1);
        let now = snapshot(&sentence);
        if now != prev {
            println!(
                "CHANGED by rule {rule_idx} id={:?}: {now}",
                disambiguator.rule_id(rule_idx)
            );
            prev = now;
        }
    }
    println!();
}

fn tagger_only_readings(words: &[&str]) -> Vec<String> {
    og_tagger::Tagger::tag(FrenchPipeline::shared().tagger_ref(), words)
        .into_iter()
        .map(|atr| {
            format!(
                "{}={}",
                atr.token().token(),
                atr.readings()
                    .iter()
                    .map(|r| format!("{}/{}", r.lemma().unwrap_or("null"), r.pos_tags().join(" ")))
                    .collect::<Vec<_>>()
                    .join("|")
            )
        })
        .collect()
}

fn dump_token_readings(text: &str, focus: &[&str]) {
    let pipeline = FrenchPipeline::shared();
    for sentence in pipeline.analyze_text(text) {
        for t in sentence.non_whitespace_tokens() {
            if focus.contains(&t.token().token()) {
                println!(
                    "  {:?} => {}",
                    t.token().token(),
                    t.readings()
                        .iter()
                        .map(|r| format!(
                            "{}/{}",
                            r.lemma().unwrap_or("null"),
                            r.pos_tags().join(" ")
                        ))
                        .collect::<Vec<_>>()
                        .join(" | ")
                );
            }
        }
    }
}

fn adj_adj_fps_probe() {
    println!("TAGGER-ONLY:");
    for line in tagger_only_readings(&["roses", "croissant", "vous", "poursoi"]) {
        println!("  {line}");
    }
    println!("PIPELINE 'Ses grandes fleurs doubles sont roses à nuance lilas et très parfumées.':");
    dump_token_readings(
        "Ses grandes fleurs doubles sont roses à nuance lilas et très parfumées.",
        &["roses", "nuance", "lilas", "parfumées"],
    );
    println!(
        "PIPELINE 'Sa configuration électronique est par ordre de niveau d'énergie croissant.':"
    );
    dump_token_readings(
        "Sa configuration électronique est par ordre de niveau d'énergie croissant.",
        &["croissant", "niveau"],
    );
    println!("PIPELINE 'Je vous écrirai ou vous téléphonerai la semaine prochaine.':");
    dump_token_readings(
        "Je vous écrirai ou vous téléphonerai la semaine prochaine.",
        &["vous", "écrirai", "téléphonerai"],
    );
    println!("PIPELINE 'Il le fait poursoi.':");
    dump_token_readings("Il le fait poursoi.", &["poursoi", "."]);
}

fn suppressor_pattern_probe() {
    let unification_defs = r#"
    <unification feature="number">
        <equivalence type="sg"><token postag=".* sp?" postag_regexp="yes"/></equivalence>
        <equivalence type="pl"><token postag=".* s?p" postag_regexp="yes"/></equivalence>
    </unification>
    <unification feature="person">
        <equivalence type="p1"><token postag=".* 1 .*" postag_regexp="yes"/></equivalence>
        <equivalence type="p2"><token postag=".* 2 .*" postag_regexp="yes"/></equivalence>
        <equivalence type="p3"><token postag=".* 3 .*" postag_regexp="yes"/></equivalence>
    </unification>"#;
    let probes: &[(&str, &str, &str)] = &[
        (
            "PROBE_OU_UNIFY_SKIP",
            r#"<pattern>
                <unify>
                    <feature id="person"/>
                    <feature id="number"/>
                    <unify-ignore>
                        <token postag="R pers suj.*" postag_regexp="yes"/>
                        <token postag="R pers obj.*" postag_regexp="yes" min="0" max="3"/>
                        <token inflected="yes" min="0" max="1" regexp="yes">ne|en</token>
                    </unify-ignore>
                    <token postag="V.*" postag_regexp="yes" skip="-1"/>
                    <unify-ignore>
                        <token>ou</token>
                        <token postag="R pers suj.*" postag_regexp="yes"/>
                        <token postag="R pers obj.*" postag_regexp="yes" min="0" max="3"/>
                        <token inflected="yes" min="0" max="1">ne</token>
                    </unify-ignore>
                    <token postag="V.*" postag_regexp="yes"/>
                </unify>
            </pattern>"#,
            "Je vous écrirai ou vous téléphonerai la semaine prochaine.",
        ),
        (
            "PROBE_AGREEMENT_AND_OPTIONALS",
            r#"<pattern>
                <token postag="[JN].*" postag_regexp="yes"/>
                <token postag="C.*" postag_regexp="yes" min="0" max="1"/>
                <token postag="P.*" postag_regexp="yes" min="0" max="1"/>
                <token postag="D.*" postag_regexp="yes" min="0" max="1"/>
                <token postag="N.* s?p|N f ." postag_regexp="yes"/>
                <and>
                    <token postag="V ppr"/>
                    <token postag="J m s"/>
                </and>
                <token regexp="yes">[,\.\;\:\!\?]"/>
            </pattern>"#,
            "Sa configuration électronique est par ordre de niveau d'énergie croissant.",
        ),
        (
            "PROBE_MATCHNO0_ANTIPATTERN",
            r#"<pattern>
                <token postag="J m s"/>
                <token regexp="yes">et|ni|ou</token>
                <token><match no="0"/>e</token>
            </pattern>"#,
            "Quand Ddaykha étudiait à l'université, elle n'avait ni ami ni amie.",
        ),
    ];
    let isolation: &[(&str, &str)] = &[
        (
            "ISO_AND_ONLY",
            r#"<pattern><and><token postag="V ppr"/><token postag="J m s"/></and></pattern>"#,
        ),
        (
            "ISO_AND_PLUS_PUNCT",
            r#"<pattern><and><token postag="V ppr"/><token postag="J m s"/></and><token regexp="yes">[,.]</token></pattern>"#,
        ),
        (
            "ISO_OPTIONALS_THEN_AND",
            r#"<pattern><token postag="C.*" postag_regexp="yes" min="0" max="1"/><token postag="N.*" postag_regexp="yes"/><and><token postag="V ppr"/><token postag="J m s"/></and></pattern>"#,
        ),
        (
            "ISO_TEXT_REGEX_ONLY",
            r#"<pattern><token postag="J m s"/><token regexp="yes">[,\.\;\:\!\?]</token></pattern>"#,
        ),
        (
            "ISO_PIPE_POSTAG",
            r#"<pattern><token postag="N.* s?p|N f ." postag_regexp="yes"/></pattern>"#,
        ),
        (
            "ISO_FULL_NO_PUNCT",
            r#"<pattern><token postag="[JN].*" postag_regexp="yes"/><token postag="C.*" postag_regexp="yes" min="0" max="1"/><token postag="P.*" postag_regexp="yes" min="0" max="1"/><token postag="D.*" postag_regexp="yes" min="0" max="1"/><token postag="N.* s?p|N f ." postag_regexp="yes"/><and><token postag="V ppr"/><token postag="J m s"/></and></pattern>"#,
        ),
        (
            "ISO_FULL_NO_AND",
            r#"<pattern><token postag="[JN].*" postag_regexp="yes"/><token postag="C.*" postag_regexp="yes" min="0" max="1"/><token postag="P.*" postag_regexp="yes" min="0" max="1"/><token postag="D.*" postag_regexp="yes" min="0" max="1"/><token postag="N.* s?p|N f ." postag_regexp="yes"/><token regexp="yes">[,\.\;\:\!\?]</token></pattern>"#,
        ),
        (
            "ISO_ONE_OPT_PLUS_TEXT",
            r#"<pattern><token postag="C.*" postag_regexp="yes" min="0" max="1"/><token regexp="yes">[,\.\;\:\!\?]</token></pattern>"#,
        ),
        (
            "ISO_TWO_OPT_PLUS_TEXT",
            r#"<pattern><token postag="C.*" postag_regexp="yes" min="0" max="1"/><token postag="P.*" postag_regexp="yes" min="0" max="1"/><token regexp="yes">[,\.\;\:\!\?]</token></pattern>"#,
        ),
        (
            "ISO_TWO_OPT_PLUS_PLAINTEXT",
            r#"<pattern><token postag="C.*" postag_regexp="yes" min="0" max="1"/><token postag="P.*" postag_regexp="yes" min="0" max="1"/><token>abc</token></pattern>"#,
        ),
    ];
    for (id, pattern) in isolation {
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rules lang=\"fr\">{unification_defs}\n\
             <rule id=\"{id}\">{pattern}<message>iso probe</message></rule>\n</rules>"
        );
        match XmlCompiler::new()
            .compile_file_with_base(&xml, crate::fr::test_support::fr_rules_dir().as_deref())
        {
            Ok(_) => println!("ISOLATION {id} OK"),
            Err(err) => println!("ISOLATION {id} ERR: {err:?}"),
        }
    }
    let engine = build_pattern_engine();
    let pipeline = FrenchPipeline::shared();
    og_rules::pattern_rule::set_test_date_hack(true);
    for (id, pattern, text) in probes {
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rules lang=\"fr\">{unification_defs}\n\
             <rule id=\"{id}\">{pattern}<message>suppressor probe</message></rule>\n</rules>"
        );
        let compiled = XmlCompiler::new()
            .compile_file_with_base(&xml, crate::fr::test_support::fr_rules_dir().as_deref());
        let rule_set = match compiled {
            Ok(set) => set,
            Err(err) => {
                println!("SUPPRESSOR {id} COMPILE ERROR: {err:?}");
                continue;
            }
        };
        let mut hits = 0usize;
        for rule in &rule_set.rules {
            for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
                for m in engine.match_rule(rule, &sentence) {
                    hits += 1;
                    println!(
                        "SUPPRESSOR {id} MATCH {}..{} '{}'",
                        m.offset() + sentence_start,
                        m.offset() + sentence_start + m.length(),
                        &text[(m.offset() + sentence_start).min(text.len())
                            ..(m.offset() + sentence_start + m.length()).min(text.len())]
                    );
                }
            }
        }
        if hits == 0 {
            println!("SUPPRESSOR {id} NO MATCH on {text:?}");
        }
    }
}

#[test]
#[ignore]
fn fr_diagnose_tokenizer() {
    let _release_guard = crate::release_guard();
    tokenizer_probe();
}

#[test]
#[ignore]
fn fr_diagnose_suppressor_patterns() {
    let _release_guard = crate::release_guard();
    suppressor_pattern_probe();
}

#[test]
#[ignore]
fn fr_diagnose_adj_adj_fps() {
    let _release_guard = crate::release_guard();
    adj_adj_fps_probe();
}

#[test]
#[ignore]
fn fr_diagnose_tagger() {
    let _release_guard = crate::release_guard();
    tagger_probe();
}

#[test]
#[ignore]
fn fr_diagnose_offsets() {
    let _release_guard = crate::release_guard();
    let pipeline = FrenchPipeline::shared();
    for text in ["alors il faut partir", "dans ce cas il est certain"] {
        println!("SPLIT {text:?} => {:?}", pipeline.sentence_ranges(text));
        for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
            println!(
                "  sentence {}..{} start {} text {:?}",
                sentence.start(),
                sentence.end(),
                sentence_start,
                sentence.text()
            );
            for (i, t) in sentence.tokens().iter().enumerate() {
                println!(
                    "    {i} {:?} [{}..{}]",
                    t.token().token(),
                    t.token().start(),
                    t.token().end()
                );
            }
        }
    }
}

#[test]
#[ignore]
fn fr_diagnose_disambig_bisect() {
    let _release_guard = crate::release_guard();
    disambig_bisect_probe();
}

#[test]
#[ignore]
fn fr_diagnose_readings() {
    let _release_guard = crate::release_guard();
    readings_probe();
}

#[test]
#[ignore]
fn fr_diagnose_rule_matches() {
    let _release_guard = crate::release_guard();
    rule_match_probe();
}
