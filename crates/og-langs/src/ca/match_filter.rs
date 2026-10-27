use og_core::match_filters::{LanguageMatchFilter, base_priority_for_id};

pub struct CatalanMatchFilter;

impl LanguageMatchFilter for CatalanMatchFilter {
    fn priority_for_id(&self, id: &str) -> i32 {
        catalan_priority_for_id(id).unwrap_or_else(|| base_priority_for_id(id))
    }
}

pub fn catalan_priority_for_id(id: &str) -> Option<i32> {
    match id {
        "CONFUSIONS2" | "DEU_NI_DO" => Some(80),
        "FER_LOGIN" | "L_OK" => Some(70),
        "INCORRECT_EXPRESSIONS"
        | "PERSONATGES_FAMOSOS"
        | "CONEIXO_CONEC"
        | "COMETES_INCORRECTES"
        | "OFERTAR_OFERIR" => Some(50),
        "PREGUEM_DISCULPIN" => Some(45),
        "DESDE_UN"
        | "CONEIXET"
        | "CONEIXENTS"
        | "MOTS_NO_SEPARATS"
        | "REPETEAD_ELEMENTS"
        | "ESPERANT_US_AGRADI"
        | "LO_NEUTRE"
        | "ESPAIS_SOBRANTS"
        | "PER_A_QUE_PERQUE"
        | "PRONOMS_FEBLES_COMBINACIONS_SE" => Some(40),
        "ELA_GEMINADA"
        | "TENIR_QUE"
        | "CONFUSIONS_PRONOMS_FEBLES"
        | "COMMA_PERO1"
        | "PASSAR_SE"
        | "PRONOMS_FEBLES_TEMPS_VERBAL" => Some(35),
        "OBLIDARSE"
        | "CA_SPLIT_WORDS"
        | "ET_AL"
        | "PRONOMS_FEBLES_COLLOQUIALS"
        | "CONCORDANCES_CASOS_PARTICULARS"
        | "GERUNDI_PERD_T"
        | "CONFUSIONS"
        | "PRONOMS_FEBLES_DARRERE_VERB"
        | "VERBS_NO_INCOATIUS"
        | "ARRIBAN_ARRIBANT"
        | "PERO_PERO"
        | "PUNT_LLETRA" => Some(30),
        "REEMPRENDRE" | "INCORRECT_WORDS_IN_CONTEXT" => Some(28),
        "PRONOMS_FEBLES_SOLTS2" => Some(26),
        "ES_UNKNOWN" | "HAVER_SENSE_HAC" | "HA_A" | "PASSAT_PERIFRASTIC" | "PREPOSITIONS" => {
            Some(25)
        }
        "CONFUSIONS_ACCENT"
        | "CONFUSIO_PASSAT_INFINITIU"
        | "DIACRITICS"
        | "COMMA_ENTRE_DALTRES"
        | "CAP_GENS"
        | "MOTS_SENSE_GUIONETS"
        | "ORDINALS"
        | "SUPER"
        | "PRONOM_FEBLE_HI" => Some(20),
        "HAVER_PARTICIPI_HAVER_IMPERSONAL" | "SE_LI_VA_FER_CALLAR" | "CA_REMOTE_RULE" => Some(15),
        "CONCORDANCES_NUMERALS_DUES"
        | "POSTULARSE"
        | "FALTA_CONDICIONAL"
        | "ACCENTUATION_CHECK"
        | "CONCORDANCA_GRIS"
        | "SELS_EN_VA_DE_LES_MANS"
        | "A_PER"
        | "CONCORDANCES_NUMERALS"
        | "COMMA_IJ"
        | "AVIS"
        | "CAP_ELS_CAP_ALS"
        | "CASING"
        | "DOS_ARTICLES"
        | "MOTS_GUIONET"
        | "SELS_EN_VA"
        | "RECENT"
        | "CONCORDANCES_NOUNS_PRIORITY"
        | "PREFIXOS_SENSE_GUIONET_EN_DICCIONARI"
        | "ZERO_O"
        | "URL"
        | "EL_FAN_AGENOLLAR" => Some(10),
        "CONCORDANCES_DET_NOM"
        | "CONCORDANCES_DET_ADJ"
        | "CONCORDANCES_DET_POSSESSIU"
        | "DET_GN"
        | "SPELLING"
        | "APOSTROF_ANYS"
        | "VENIR_NO_REFLEXIU"
        | "DEUS_SEUS"
        | "SON_BONIC"
        | "ACCENTUACIO"
        | "FIDEUA"
        | "L_NO_APOSTROFA"
        | "EN_NO_INFINITIU_CAUSAL_REMOTE"
        | "L_D_N_NO_S_APOSTROFEN"
        | "AMB_EM" => Some(5),
        "CONTRACCIONS" => Some(0),
        "CASING_START" => Some(-5),
        "CA_WORD_COHERENCY"
        | "CA_WORD_COHERENCY_VALENCIA"
        | "QUAN_PREPOSICIO"
        | "ARTICLE_TOPONIM_MIN"
        | "PEL_QUE"
        | "COMMA_LOCUTION"
        | "REGIONAL_VERBS"
        | "UN_ALTRE_DISTRIBUTIVES"
        | "PRONOMS_FEBLES_SOLTS"
        | "CONCORDANCA_PRONOMS_CATCHALL" => Some(-10),
        "AGREEMENT_POSTPONED_ADJ" => Some(-15),
        "FALTA_COMA_FRASE_CONDICIONAL" | "ESPAIS_QUE_FALTEN_PUNTUACIO" | "VERBS_NOMSPROPIS" => {
            Some(-20)
        }
        "VERBS_PRONOMINALS" => Some(-25),
        "PORTO_LLEGINT" => Some(-30),
        "PORTA_UNA_HORA" | "MAJOR_MES_GRAN0" => Some(-40),
        "REPETITIONS_STYLE" | "MUNDAR" => Some(-50),
        "NOMBRES_ROMANS" | "CA_SPLIT_LONG_SENTENCE" => Some(-90),
        "TASCAS_TASQUES" | "PREPOSICIONS_MINUSCULA" | "SUGGERIMENTS_LE" => Some(-97),
        "MORFOLOGIK_RULE_CA_ES" => Some(-100),
        "EXIGEIX_ACCENTUACIO_VALENCIANA" => Some(-120),
        "PHRASE_REPETITION" | "SUBSTANTIUS_JUNTS" => Some(-150),
        "REPETITION_ADJ_N_ADJ" => Some(-155),
        "FALTA_ELEMENT_ENTRE_VERBS" | "PUNT_FINAL" | "PUNCTUATION_PARAGRAPH_END" => Some(-200),
        "CA_END_PARAGRAPH_PUNCTUATION" | "DICENDI_QUE" => Some(-250),
        "UPPERCASE_SENTENCE_START" | "MAJUSCULA_IMPROBABLE" | "ELA_GEMINADA_WIKI" => Some(-300),
        _ => multitoken_or_replace_prefix(id),
    }
}

fn multitoken_or_replace_prefix(id: &str) -> Option<i32> {
    if id.starts_with("CA_MULTITOKEN_SPELLING") {
        return Some(-95);
    }
    if id.starts_with("CA_SIMPLE_REPLACE_MULTIWORDS") {
        return Some(70);
    }
    if id.starts_with("CA_SIMPLE_REPLACE_ANGLICISM") {
        return Some(65);
    }
    if id.starts_with("CA_SIMPLE_REPLACE_BALEARIC") {
        return Some(60);
    }
    if id.starts_with("CA_SIMPLE_REPLACE_VERBS") {
        return Some(28);
    }
    if id.starts_with("CA_COMPOUNDS") {
        return Some(50);
    }
    if id.starts_with("CA_SIMPLE_REPLACE_DIACRITICS_IEC") {
        return Some(0);
    }
    if id.starts_with("CA_SIMPLE_REPLACE") {
        return Some(30);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const JAVA_TABLE: &[(&str, i32)] = &[
        ("CONFUSIONS2", 80),
        ("DEU_NI_DO", 80),
        ("FER_LOGIN", 70),
        ("L_OK", 70),
        ("INCORRECT_EXPRESSIONS", 50),
        ("PERSONATGES_FAMOSOS", 50),
        ("CONEIXO_CONEC", 50),
        ("COMETES_INCORRECTES", 50),
        ("OFERTAR_OFERIR", 50),
        ("PREGUEM_DISCULPIN", 45),
        ("DESDE_UN", 40),
        ("CONEIXET", 40),
        ("CONEIXENTS", 40),
        ("MOTS_NO_SEPARATS", 40),
        ("REPETEAD_ELEMENTS", 40),
        ("ESPERANT_US_AGRADI", 40),
        ("LO_NEUTRE", 40),
        ("ESPAIS_SOBRANTS", 40),
        ("PER_A_QUE_PERQUE", 40),
        ("PRONOMS_FEBLES_COMBINACIONS_SE", 40),
        ("ELA_GEMINADA", 35),
        ("TENIR_QUE", 35),
        ("CONFUSIONS_PRONOMS_FEBLES", 35),
        ("COMMA_PERO1", 35),
        ("PASSAR_SE", 35),
        ("PRONOMS_FEBLES_TEMPS_VERBAL", 35),
        ("OBLIDARSE", 30),
        ("CA_SPLIT_WORDS", 30),
        ("ET_AL", 30),
        ("PRONOMS_FEBLES_COLLOQUIALS", 30),
        ("CONCORDANCES_CASOS_PARTICULARS", 30),
        ("GERUNDI_PERD_T", 30),
        ("CONFUSIONS", 30),
        ("PRONOMS_FEBLES_DARRERE_VERB", 30),
        ("VERBS_NO_INCOATIUS", 30),
        ("ARRIBAN_ARRIBANT", 30),
        ("PERO_PERO", 30),
        ("PUNT_LLETRA", 30),
        ("REEMPRENDRE", 28),
        ("INCORRECT_WORDS_IN_CONTEXT", 28),
        ("PRONOMS_FEBLES_SOLTS2", 26),
        ("ES_UNKNOWN", 25),
        ("HAVER_SENSE_HAC", 25),
        ("HA_A", 25),
        ("PASSAT_PERIFRASTIC", 25),
        ("PREPOSITIONS", 25),
        ("CONFUSIONS_ACCENT", 20),
        ("CONFUSIO_PASSAT_INFINITIU", 20),
        ("DIACRITICS", 20),
        ("COMMA_ENTRE_DALTRES", 20),
        ("CAP_GENS", 20),
        ("MOTS_SENSE_GUIONETS", 20),
        ("ORDINALS", 20),
        ("SUPER", 20),
        ("PRONOM_FEBLE_HI", 20),
        ("HAVER_PARTICIPI_HAVER_IMPERSONAL", 15),
        ("SE_LI_VA_FER_CALLAR", 15),
        ("CA_REMOTE_RULE", 15),
        ("CONCORDANCES_NUMERALS_DUES", 10),
        ("POSTULARSE", 10),
        ("FALTA_CONDICIONAL", 10),
        ("ACCENTUATION_CHECK", 10),
        ("CONCORDANCA_GRIS", 10),
        ("SELS_EN_VA_DE_LES_MANS", 10),
        ("A_PER", 10),
        ("CONCORDANCES_NUMERALS", 10),
        ("COMMA_IJ", 10),
        ("AVIS", 10),
        ("CAP_ELS_CAP_ALS", 10),
        ("CASING", 10),
        ("DOS_ARTICLES", 10),
        ("MOTS_GUIONET", 10),
        ("SELS_EN_VA", 10),
        ("RECENT", 10),
        ("CONCORDANCES_NOUNS_PRIORITY", 10),
        ("PREFIXOS_SENSE_GUIONET_EN_DICCIONARI", 10),
        ("ZERO_O", 10),
        ("URL", 10),
        ("EL_FAN_AGENOLLAR", 10),
        ("CONCORDANCES_DET_NOM", 5),
        ("CONCORDANCES_DET_ADJ", 5),
        ("CONCORDANCES_DET_POSSESSIU", 5),
        ("DET_GN", 5),
        ("SPELLING", 5),
        ("APOSTROF_ANYS", 5),
        ("VENIR_NO_REFLEXIU", 5),
        ("DEUS_SEUS", 5),
        ("SON_BONIC", 5),
        ("ACCENTUACIO", 5),
        ("FIDEUA", 5),
        ("L_NO_APOSTROFA", 5),
        ("EN_NO_INFINITIU_CAUSAL_REMOTE", 5),
        ("L_D_N_NO_S_APOSTROFEN", 5),
        ("AMB_EM", 5),
        ("CONTRACCIONS", 0),
        ("CASING_START", -5),
        ("CA_WORD_COHERENCY", -10),
        ("CA_WORD_COHERENCY_VALENCIA", -10),
        ("QUAN_PREPOSICIO", -10),
        ("ARTICLE_TOPONIM_MIN", -10),
        ("PEL_QUE", -10),
        ("COMMA_LOCUTION", -10),
        ("REGIONAL_VERBS", -10),
        ("UN_ALTRE_DISTRIBUTIVES", -10),
        ("PRONOMS_FEBLES_SOLTS", -10),
        ("CONCORDANCA_PRONOMS_CATCHALL", -10),
        ("AGREEMENT_POSTPONED_ADJ", -15),
        ("FALTA_COMA_FRASE_CONDICIONAL", -20),
        ("ESPAIS_QUE_FALTEN_PUNTUACIO", -20),
        ("VERBS_NOMSPROPIS", -20),
        ("VERBS_PRONOMINALS", -25),
        ("PORTO_LLEGINT", -30),
        ("PORTA_UNA_HORA", -40),
        ("MAJOR_MES_GRAN0", -40),
        ("REPETITIONS_STYLE", -50),
        ("MUNDAR", -50),
        ("NOMBRES_ROMANS", -90),
        ("CA_SPLIT_LONG_SENTENCE", -90),
        ("TASCAS_TASQUES", -97),
        ("PREPOSICIONS_MINUSCULA", -97),
        ("SUGGERIMENTS_LE", -97),
        ("MORFOLOGIK_RULE_CA_ES", -100),
        ("EXIGEIX_ACCENTUACIO_VALENCIANA", -120),
        ("PHRASE_REPETITION", -150),
        ("SUBSTANTIUS_JUNTS", -150),
        ("REPETITION_ADJ_N_ADJ", -155),
        ("FALTA_ELEMENT_ENTRE_VERBS", -200),
        ("PUNT_FINAL", -200),
        ("PUNCTUATION_PARAGRAPH_END", -200),
        ("CA_END_PARAGRAPH_PUNCTUATION", -250),
        ("DICENDI_QUE", -250),
        ("UPPERCASE_SENTENCE_START", -300),
        ("MAJUSCULA_IMPROBABLE", -300),
        ("ELA_GEMINADA_WIKI", -300),
    ];

    #[test]
    fn table_size_pins_the_java_switch() {
        assert_eq!(JAVA_TABLE.len(), 134);
    }

    #[test]
    fn ca_priorities_decide_the_surviving_overlap() {
        use og_core::match_filters::{BaseLanguageMatchFilter, clean_overlapping_filter};
        use og_core::rule_match::{RuleMatch, RuleMatchContext, RuleMatchRule};
        let make = |id: &str| {
            RuleMatch::new(
                "msg",
                0,
                5,
                RuleMatchRule::new(id, "desc"),
                RuleMatchContext::new("un texte", 0, 5),
            )
        };
        let matches = vec![make("DEU_NI_DO"), make("MUNDAR")];
        let ca_kept = clean_overlapping_filter(matches, "un texte", &CatalanMatchFilter);
        assert_eq!(
            ca_kept.iter().map(|m| m.rule().id()).collect::<Vec<_>>(),
            vec!["DEU_NI_DO"],
            "80 beats -50 under the ca table"
        );
        let matches = vec![make("DEU_NI_DO"), make("MUNDAR")];
        let base_kept = clean_overlapping_filter(matches, "un texte", &BaseLanguageMatchFilter);
        assert_eq!(
            base_kept.iter().map(|m| m.rule().id()).collect::<Vec<_>>(),
            vec!["MUNDAR"],
            "the base filter ties at 0/0 and keeps the later match"
        );
    }

    #[test]
    fn exact_table_matches_java_case_for_case() {
        for (id, priority) in JAVA_TABLE {
            assert_eq!(
                catalan_priority_for_id(id),
                Some(*priority),
                "wrong priority for {id}"
            );
        }
        let mut ids: Vec<&str> = JAVA_TABLE.iter().map(|(id, _)| *id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), JAVA_TABLE.len(), "duplicate ids in JAVA_TABLE");
    }

    #[test]
    fn starts_with_chains_apply_after_the_switch() {
        assert_eq!(
            catalan_priority_for_id("CA_MULTITOKEN_SPELLING_FOUR"),
            Some(-95)
        );
        assert_eq!(catalan_priority_for_id("CA_MULTITOKEN_SPELLING"), Some(-95));
        assert_eq!(
            catalan_priority_for_id("CA_SIMPLE_REPLACE_MULTIWORDS"),
            Some(70)
        );
        assert_eq!(
            catalan_priority_for_id("CA_SIMPLE_REPLACE_ANGLICISM"),
            Some(65)
        );
        assert_eq!(
            catalan_priority_for_id("CA_SIMPLE_REPLACE_BALEARIC"),
            Some(60)
        );
        assert_eq!(catalan_priority_for_id("CA_SIMPLE_REPLACE_VERBS"), Some(28));
        assert_eq!(catalan_priority_for_id("CA_COMPOUNDS"), Some(50));
        assert_eq!(
            catalan_priority_for_id("CA_SIMPLE_REPLACE_DIACRITICS_IEC"),
            Some(0)
        );
        assert_eq!(
            catalan_priority_for_id("CA_SIMPLE_REPLACE_SIMPLE"),
            Some(30)
        );
        assert_eq!(catalan_priority_for_id("CA_SIMPLE_REPLACE_DNV"), Some(30));
    }

    #[test]
    fn unknown_ids_fall_through_to_the_language_base() {
        assert_eq!(catalan_priority_for_id("SOME_CA_RULE"), None);
        let filter = CatalanMatchFilter;
        assert_eq!(filter.priority_for_id("SOME_CA_RULE"), 0);
        assert_eq!(filter.priority_for_id("TOO_LONG_SENTENCE"), -101);
        assert_eq!(filter.priority_for_id("too_long_sentence"), -101);
        assert_eq!(filter.priority_for_id("TYPOGRAPHY_STYLE"), -50);
    }

    #[test]
    fn repetitions_style_override_beats_the_base_value() {
        let filter = CatalanMatchFilter;
        assert_eq!(filter.priority_for_id("REPETITIONS_STYLE"), -50);
        assert_eq!(filter.priority_for_id("MUNDAR"), -50);
    }
}
