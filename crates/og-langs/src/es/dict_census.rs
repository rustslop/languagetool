use og_tagger::morfologik::MorfologikDictionary;
use std::path::PathBuf;

#[test]
fn pinned_dict_gaps_block_the_eight_direct_ok_residue_rules() {
    let _release_guard = crate::release_guard();
    let pipeline = crate::es::test_support::SpanishPipeline::shared();
    let tagger = pipeline.tagger_ref();
    let primera = tagger.lookup("primera");
    assert!(
        primera.contains(&("1", "AO0FS0")),
        "ordinal surface carries the numeric-lemma reading: {primera:?}"
    );
    assert!(
        !primera
            .iter()
            .any(|(lemma, tag)| *lemma == "primero" && tag.starts_with("AO")),
        "AGREEMENT_ORDINALS needs a word-lemma AO reading for inflected=yes (PatternToken.getTestToken tests the reading lemma); the pinned dict has none: {primera:?}"
    );
    let vigesimo = tagger.lookup("vigésimo");
    assert!(
        vigesimo.iter().all(|(lemma, _)| *lemma == "20"),
        "ordinal decenas readings also carry numeric lemmas: {vigesimo:?}"
    );
    for (word, rule) in [
        ("medio", "AGREEMENT_DET_NOUN"),
        ("dos", "HACE_UN_MES_ATRAS"),
    ] {
        let readings = tagger.lookup(word);
        assert!(
            readings.iter().all(|(_, tag)| !tag.starts_with('D')),
            "{rule} needs a D[^RN].* determiner reading for '{word}'; the pinned dict (and the fetched 460k snapshot) has none: {readings:?}"
        );
    }
}

#[test]
fn pinned_dict_era_numeral_readings_block_mi_tilde_and_el_menos() {
    let _release_guard = crate::release_guard();
    let pipeline = crate::es::test_support::SpanishPipeline::shared();
    let tagger = pipeline.tagger_ref();
    let mil = tagger.lookup("mil");
    assert!(
        mil.contains(&("mil", "AQ0CP0")) && mil.iter().all(|(_, tag)| !tag.starts_with("DN")),
        "MI_TILDE antipattern 2 (grammar.xml:9140-9144) suppresses 'a mi mil' unless 'mil' carries a DN.* reading exempted by its <exception postag=\"DN.*\">; the pinned dict has only {mil:?}"
    );
    for word in ["tres", "cuatro"] {
        let readings = tagger.lookup(word);
        assert!(
            readings.iter().all(|(_, tag)| !tag.starts_with("DN")),
            "EL_MENOS sub-rules need a D[IN].* reading on '{word}' so the optional determiner element can consume it before vez|veces; the pinned dict has only {readings:?}"
        );
    }
    let pasada = tagger.lookup("pasada");
    assert!(
        pasada.iter().all(|(lemma, _)| *lemma != "pasado"),
        "AGREEMENT_POSTPONED_ADJ's day-of-week antipattern exempts inflected forms of lemma 'pasado' (grammar.xml entities.ent:18); without a pasado-lemma reading on 'pasada' the exception cannot fire and the match is suppressed: {pasada:?}"
    );
}

#[test]
#[ignore]
fn probe_dict_era_reading_evidence() {
    let _release_guard = crate::release_guard();
    let pipeline = crate::es::test_support::SpanishPipeline::shared();
    let tagger = pipeline.tagger_ref();
    for w in [
        "mil",
        "tres",
        "dos",
        "cuatro",
        "pasada",
        "anime",
        "motriz",
        "maquina",
        "productos",
        "musgo",
        "penca",
        "tsunami",
    ] {
        eprintln!("{w}: {:?}", tagger.lookup(w));
    }
}

#[test]
#[ignore]
fn compare_failing_words_across_spanish_dicts() {
    let _release_guard = crate::release_guard();
    let resource_dir = crate::es::es_resource_dir().expect("es resource dir");
    let on_disk = MorfologikDictionary::load(
        &resource_dir.join("spanish.dict"),
        &resource_dir.join("spanish.info"),
    )
    .expect("on-disk spanish.dict");
    let old = MorfologikDictionary::load(
        &PathBuf::from("/tmp/old_spanish.dict"),
        &PathBuf::from("/tmp/old_spanish.info"),
    );
    let fetched = MorfologikDictionary::load(
        &PathBuf::from("/tmp/current_spanish.dict"),
        &PathBuf::from("/tmp/current_spanish.info"),
    );
    if old.is_none() {
        eprintln!("no /tmp/old_spanish.dict; skipping comparison");
        return;
    }
    let old = old.unwrap();

    let words = std::fs::read_to_string("/tmp/es_fail_words.txt").expect("word list");
    let mut only_old = 0usize;
    let mut only_new = 0usize;
    let mut in_both = 0usize;
    let mut in_neither = 0usize;
    let mut old_better: Vec<String> = Vec::new();
    let mut new_better: Vec<String> = Vec::new();
    let mut neither: Vec<String> = Vec::new();
    for w in words.lines() {
        let w = w.trim();
        if w.is_empty() {
            continue;
        }
        let lower = w.to_lowercase();
        let has = |d: &MorfologikDictionary| {
            let cap = {
                let mut chars = lower.chars();
                chars
                    .next()
                    .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default()
            };
            !d.lookup(w).is_empty()
                || (!lower.eq(w) && !d.lookup(&lower).is_empty())
                || (w.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
                    && !d.lookup(&cap).is_empty())
        };
        let in_old = has(&old);
        let in_new = has(&on_disk);
        match (in_old, in_new) {
            (true, true) => in_both += 1,
            (true, false) => {
                only_old += 1;
                old_better.push(w.to_string());
            }
            (false, true) => {
                only_new += 1;
                new_better.push(w.to_string());
            }
            (false, false) => {
                in_neither += 1;
                neither.push(w.to_string());
            }
        }
    }
    eprintln!(
        "both: {in_both}, only-old: {only_old}, only-fetched: {only_new}, neither: {in_neither}"
    );
    eprintln!("words only in last-tracked 1MB dict: {old_better:?}");
    eprintln!("words only in fetched dict: {new_better:?}");
    eprintln!("words in neither dict: {neither:?}");
    for w in ["del", "al", "dos", "joven", "casa", "podés", "Jorge"] {
        let fmt = |d: &MorfologikDictionary| {
            d.lookup(w)
                .iter()
                .map(|(l, t)| format!("{l}/{t}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let fetched_fmt = fetched
            .as_ref()
            .map(|d| format!("[{}]", fmt(d)))
            .unwrap_or_else(|| "ABSENT".to_string());
        eprintln!(
            "{w}: last-tracked=[{}] fetched460k={fetched_fmt}",
            fmt(&old)
        );
    }
}

#[test]
#[ignore]
fn probe_suppress_misspelled_suggestion_words() {
    let _release_guard = crate::release_guard();
    let resource_dir = crate::es::es_resource_dir().expect("es resource dir");
    let dict = MorfologikDictionary::load(
        &resource_dir.join("spanish.dict"),
        &resource_dir.join("spanish.info"),
    )
    .expect("spanish.dict");
    for w in [
        "nanotecnología",
        "revictimización",
        "cofundador",
        "intersectorial",
        "sacaleches",
        "betalactámicos",
        "antirruso",
        "antirrusas",
        "cazarecompensas",
        "postrevolucionario",
        "cibercomercio",
        "prodemócrata",
        "prerebajas",
        "macroredada",
        "neurobiología",
        "astroturismo",
        "excolonias",
        "hidroalcohólico",
        "coexpositores",
        "mayahablantes",
        "cubrebocas",
        "microrracismo",
        "pseudorreferéndum",
        "seudorreferéndum",
        "nacionalpopulismo",
        "nacionalpopulistas",
        "reactiva",
        "autocuración",
        "rebemol",
        "conhablantes",
    ] {
        eprintln!("{w}: {}", !dict.lookup(w).is_empty());
    }
}

#[test]
#[ignore]
fn probe_style_failure_words() {
    let _release_guard = crate::release_guard();
    let resource_dir = crate::es::es_resource_dir().expect("es resource dir");
    let dict = MorfologikDictionary::load(
        &resource_dir.join("spanish.dict"),
        &resource_dir.join("spanish.info"),
    )
    .expect("spanish.dict");
    for w in [
        "complejizó",
        "complejizar",
        "Cantaba",
        "desafinó",
        "cantó",
        "casas",
        "casa",
        "rojas",
        "árbol",
        "rojo",
    ] {
        eprintln!("{w}: {}", !dict.lookup(w).is_empty());
    }
}
