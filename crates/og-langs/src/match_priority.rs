use og_core::RuleMatch;
use og_core::match_filters::{
    BaseLanguageMatchFilter, LanguageMatchFilter, base_priority_for_id, original_error_str,
};
use std::sync::Arc;

fn en_priority_table(id: &str) -> Option<i32> {
    Some(match id {
        "I_E" => 10,
        "CHILDISH_LANGUAGE" => 8,
        "RUDE_SARCASTIC" => 6,
        "FOR_NOUN_SAKE" => 6,
        "YEAR_OLD_HYPHEN" => 6,
        "MISSING_HYPHEN" => 5,
        "WRONG_APOSTROPHE" => 5,
        "YOU_GOOD" => 3,
        "DOS_AND_DONTS" => 3,
        "IF_YOU_FURTHER_QUESTIONS" => 3,
        "ABBREVIATION_PUNCTUATION" => 2,
        "READ_ONLY_ACCESS_HYPHEN" => 2,
        "MAKE_OR_BREAK_HYPHEN" => 2,
        "LINKED_IN" => 2,
        "T_HE" => 1,
        "I_A_M" => 1,
        "ACCESS_EXCESS" => 1,
        "PRP_ABLE_TO" => 1,
        "WEE_WE" => 1,
        "CAN_MISSPELLING" => 1,
        "FOR_THE_MOST_PART2" => 1,
        "FACE_TO_FACE_HYPHEN" => 1,
        "RUN_ON" => 1,
        "ON_THE_LOOK_OUT" => 1,
        "APOSTROPHE_IN_DAYS" => 1,
        "SAFE_GUARD_COMPOUND" => 1,
        "EVEN_HANDED_HYPHEN" => 1,
        "GET_TOGETHER_HYPHEN" => 1,
        "GOT_HERE" => 1,
        "PICTURE_PERFECT_HYPHEN" => 1,
        "SEEM_SEEN" => 1,
        "SAVE_SAFE" => 1,
        "FEDEX" => 2,
        "DROP_DEAD_HYPHEN" => 1,
        "HEAR_HERE" => 1,
        "THE_FRENCH" => 1,
        "A_HEADS_UP" => 1,
        "UNITES_UNITED" => 1,
        "THIS_MISSING_VERB" => 1,
        "YOURE" => 1,
        "LIFE_COMPOUNDS" => 1,
        "DRIVE_THROUGH_HYPHEN" => 1,
        "CAUSE_COURSE" => 1,
        "THANK_YOUR" => 1,
        "AN_AND" => 1,
        "HER_S" => 1,
        "ONE_TO_MANY_HYPHEN" => 1,
        "COVID_19" => 1,
        "RATHER_NOT_VB" => 1,
        "PIECE_COMPOUNDS" => 1,
        "OTHER_WISE_COMPOUND" => 1,
        "ON_EXCEL" => 1,
        "ALL_NN" => 1,
        "SHOW_COMPOUNDS" => 1,
        "PRP_AREA" => 1,
        "IF_VB_PCT" => 1,
        "CAUSE_BECAUSE" => 1,
        "MAY_MANY" => 1,
        "BOUT_TO" => 1,
        "HAVE_HAVE" => 1,
        "LUV" => 1,
        "DAT" => 1,
        "MAC_OS" => 1,
        "BESTEST" => 1,
        "OFF_OF" => 1,
        "SHELL_COMPOUNDS" => 1,
        "HANDS_ON_HYPHEN" => 1,
        "PROFITS_WARNINGS" => 1,
        "QUIET_QUITE" => 1,
        "A_OK" => 1,
        "I_A" => 1,
        "PRP_NO_VB" => 1,
        "GAVE_HAVE" => 1,
        "THERE_FORE" => 1,
        "FOLLOW_UP" => 1,
        "IT_SOMETHING" => 1,
        "NO_KNOW" => 1,
        "WILL_BASED_ON" => 1,
        "DON_T_AREN_T" => 1,
        "WILL_BECOMING" => 1,
        "WOULD_NEVER_VBN" => 1,
        "MONEY_BACK_HYPHEN" => 1,
        "WORLDS_BEST" => 1,
        "STEP_COMPOUNDS" => 1,
        "WON_T_TO" => 1,
        "WAN_T" => 1,
        "THE_US" => 1,
        "THE_IT" => 1,
        "THANK_YOU_MUCH" => 1,
        "TO_DO_HYPHEN" => 1,
        "A_NUMBER_NNS" => 1,
        "A_HUNDREDS" => 1,
        "NOW_A_DAYS" => 1,
        "COUPLE_OF_TIMES" => 1,
        "A_WINDOWS" => 1,
        "A_SCISSOR" => 1,
        "A_SNICKERS" => 1,
        "A_NNS_BEST_NN" => 1,
        "BACHELORS" => 1,
        "WERE_WEAR" => 1,
        "NEITHER_NOR" => 1,
        "FOR_AWHILE" => 1,
        "A_BUT" => 1,
        "BORN_IN" => 1,
        "DO_TO" => 1,
        "CURIOS_CURIOUS" => 1,
        "INCORRECT_POSSESSIVE_APOSTROPHE" => 1,
        "THIS_YEARS_POSSESSIVE_APOSTROPHE" => 1,
        "SPURIOUS_APOSTROPHE" => 1,
        "BE_NOT_BE_JJ" => 1,
        "IN_THIS_REGARDS" => 1,
        "IT_SEAMS" => 1,
        "NO_WHERE" => 1,
        "APOSTROPHE_VS_QUOTE" => 1,
        "ALL_OF_SUDDEN" => 1,
        "COMMA_PERIOD" => 1,
        "COMMA_CLOSING_PARENTHESIS" => 1,
        "ELLIPSIS" => 1,
        "HERE_HEAR" => 1,
        "MISSING_POSS_APOS" => 1,
        "DO_HE_VERB" => 1,
        "LIGATURES" => 1,
        "APPSTORE" => 1,
        "INCORRECT_CONTRACTIONS" => 1,
        "DONT_T" => 1,
        "WHATS_APP" => 1,
        "NON_STANDARD_COMMA" => 1,
        "NON_ENGLISH_CHARACTER_IN_A_WORD" => 1,
        "WONT_CONTRACTION" => 1,
        "THAN_THANK" => 1,
        "IT_IF" => 1,
        "FINE_TUNE_COMPOUNDS" => 1,
        "WHAT_IS_YOU" => 1,
        "SUPPOSE_TO" => 1,
        "CONFUSION_GONG_GOING" => 1,
        "SEEN_SEEM" => 1,
        "PROFANITY" => 1,
        "PROFANITY_XML" => 1,
        "GOOD_FLUCK" => 2,
        "PROFANITY_TYPOS" => 2,
        "THE_THEM" => 1,
        "THERE_THEIR" => 1,
        "TO_WORRIED_ABOUT" => 1,
        "IT_IS_DEPENDING_ON" => 1,
        "TO_NIGHT_TO_DAY" => 1,
        "IRREGARDLESS" => 1,
        "MD_APOSTROPHE_VB" => 1,
        "ULTRA_HYPHEN" => 1,
        "THINK_BELIEVE_THAT" => 1,
        "HAS_TO_APPROVED_BY" => 1,
        "MD_VBD" => -1,
        "PRP_PRP" => -1,
        "IS_LIKELY_TO_BE" => -1,
        "WANNA" => 1,
        "LOOK_FORWARD_TO" => 1,
        "LOOK_SLIKE" => 1,
        "A3FT" => 1,
        "HYPHEN_TO_EN" => 1,
        "EVERY_NOW_AND_THEN" => 0,
        "EN_DIACRITICS_REPLACE_ORTHOGRAPHY" => -1,
        "MISSING_COMMA_BETWEEN_DAY_AND_YEAR" => -1,
        "FASTLY" => -1,
        "WHO_NOUN" => -1,
        "ANYWAYS" => -1,
        "MISSING_GENITIVE" => -1,
        "EN_UNPAIRED_BRACKETS" => -1,
        "WAKED_UP" => -1,
        "NEEDS_FIXED" => -1,
        "SENT_START_NNP_COMMA" => -1,
        "SENT_START_NN_DT" => -1,
        "DT_PDT" => -1,
        "MD_VB_AND_NOTVB" => -1,
        "BLACK_SEA" => -1,
        "A_TO" => -1,
        "MANY_NN" => -1,
        "WE_BE" => -1,
        "A_LOT_OF_NN" => -1,
        "REPETITIONS_STYLE" => -51,
        "ORDER_OF_WORDS_WITH_NOT" => -1,
        "ADVERB_WORD_ORDER_10_TEMP" => 1,
        "ADVERB_WORD_ORDER" => -1,
        "HAVE_VB_DT" => -1,
        "MD_PRP" => -1,
        "IT_IS_2" => -1,
        "A_RB_NN" => -1,
        "DT_RB_IN" => -1,
        "VERB_NOUN_CONFUSION" => -1,
        "NOUN_VERB_CONFUSION" => -1,
        "PLURAL_VERB_AFTER_THIS" => -1,
        "BE_RB_BE" => -1,
        "IT_ITS" => -1,
        "ENGLISH_WORD_REPEAT_RULE" => -1,
        "DT_JJ_NO_NOUN" => -1,
        "AGREEMENT_SENT_START" => -1,
        "PREPOSITION_VERB" => -1,
        "EN_A_VS_AN" => -1,
        "CD_NN" => -1,
        "CD_NNU" => -1,
        "ATD_VERBS_TO_COLLOCATION" => -1,
        "ORDINAL_NUMBER_MISSING_ORDINAL_INDICATOR" => -1,
        "ADVERB_OR_HYPHENATED_ADJECTIVE" => -1,
        "GOING_TO_VBD" => -1,
        "MISSING_PREPOSITION" => -1,
        "CHARACTER_APOSTROPHE_WORD" => -1,
        "SINGLE_CHARACTER" => -1,
        "BE_TO_VBG" => -1,
        "NON3PRS_VERB" => -1,
        "DT_NN_VBG" => -1,
        "NNS_THAT_ARE_JJ" => -1,
        "DID_FOUND_AMBIGUOUS" => -1,
        "BE_I_BE_GERUND" => -1,
        "VBZ_VBD" => -1,
        "SUPERLATIVE_THAN" => -1,
        "UNLIKELY_OPENING_PUNCTUATION" => -1,
        "MD_DT_JJ" => -1,
        "I_IF" => -1,
        "NOUNPHRASE_VB_RB_DT" => -1,
        "SENT_START_NN_NN_VB" => -1,
        "VB_A_JJ_NNS" => -1,
        "DUPLICATION_OF_IS_VBZ" => -1,
        "METRIC_UNITS_EN_IMPERIAL" => -1,
        "IF_THEN_COMMA" => -1,
        "COMMA_COMPOUND_SENTENCE" => -1,
        "COMMA_COMPOUND_SENTENCE_2" => -1,
        "BE_VBG_BE" => -1,
        "PRP_VB_VB" => -1,
        "FOR_ANY_CLARIFICATIONS" => -1,
        "PLEASE_LET_ME_KNOW" => -1,
        "UNNECESSARY_CAPITALIZATION" => -1,
        "CONFUSION_OF_A_JJ_NNP_NNS_PRP" => -1,
        "PLURALITY_CONFUSION_OF_NNS_OF_NN" => -1,
        "NP_TO_IS" => -1,
        "REPEATED_VERBS" => -1,
        "NNP_COMMA_QUESTION" => -2,
        "THE_CC" => -2,
        "PRP_VBG" => -2,
        "CANT_JJ" => -2,
        "WOULD_A" => -2,
        "I_AM_VB" => -2,
        "VBP_VBP" => -2,
        "GONNA_TEMP" => -3,
        "A_INFINITIVE" => -3,
        "INDIAN_ENGLISH" => -3,
        "DO_PRP_NOTVB" => -3,
        "GONNA" => -4,
        "WHATCHA" => -4,
        "DONTCHA" => -4,
        "GOTCHA" => -4,
        "OUTTA" => -4,
        "Y_ALL" => -4,
        "GIMME" => -4,
        "LEMME" => -4,
        "ID_CASING" => -4,
        "MORFOLOGIK_RULE_EN_US" => -10,
        "MORFOLOGIK_RULE_EN_GB" => -10,
        "MORFOLOGIK_RULE_EN_CA" => -10,
        "MORFOLOGIK_RULE_EN_ZA" => -10,
        "MORFOLOGIK_RULE_EN_NZ" => -10,
        "MORFOLOGIK_RULE_EN_AU" => -10,
        "UPPERCASE_SENTENCE_START" => -11,
        "MD_PRP_QUESTION_MARK" => -11,
        "PRP_RB_NO_VB" => -12,
        "MD_JJ" => -12,
        "HE_VERB_AGR" => -12,
        "MD_BASEFORM" => -12,
        "IT_VBZ" => -12,
        "PRP_THE" => -12,
        "PRP_JJ" => -12,
        "SINGULAR_NOUN_VERB_AGREEMENT" => -12,
        "SINGULAR_AGREEMENT_SENT_START" => -12,
        "VB_TO_NN_DT" => -12,
        "SUBJECTVERBAGREEMENT_2" => -12,
        "THE_SENT_END" => -12,
        "DT_NN_ARE_AME" => -12,
        "COLLECTIVE_NOUN_VERB_AGREEMENT_VBP" => -12,
        "SUBJECT_VERB_AGREEMENT" => -12,
        "VERB_APOSTROPHE_S" => -12,
        "WHERE_MD_VB" => -12,
        "SENT_START_PRPS_JJ_NN_VBP" => -12,
        "TO_AFTER_MODAL_VERBS" => -12,
        "SINGULAR_NOUN_ADV_AGREEMENT" => -12,
        "BE_VBP_IN" => -12,
        "BE_VBG_NN" => -12,
        "THE_NNS_NN_IS" => -12,
        "IF_DT_NN_VBZ" => -12,
        "PRP_MD_NN" => -12,
        "HAVE_PART_AGREEMENT" => -13,
        "BEEN_PART_AGREEMENT" => -13,
        "BE_WITH_WRONG_VERB_FORM" => -14,
        "TWO_CONNECTED_MODAL_VERBS" => -15,
        "PRP_NO_ADVERB_VERB" => -15,
        "MISSING_TO_BETWEEN_BE_AND_VB" => -15,
        "IN_DT_IN" => -15,
        "MISSING_SUBJECT" => -15,
        "HAVE_TO_NOTVB" => -15,
        "PLEASE_DO_NOT_THE_CAT" => -15,
        "VB_TO_JJ" => -15,
        "CC_PRP_ARTICLE" => -15,
        "BE_MD" => -20,
        "POSSESSIVE_APOSTROPHE" => -10,
        "WANT_TO_NN" => -25,
        "QUESTION_WITHOUT_VERB" => -25,
        "PRP_VB" => -25,
        "PRP_VB_NN" => -25,
        "BE_NN" => -26,
        "BE_VB_OR_NN" => -26,
        "DO_DT_NN_BE" => -26,
        "PRONOUN_NOUN" => -26,
        "ETC_PERIOD" => -49,
        "COULD_YOU_NOT_NEEDED" => -49,
        "SENTENCE_FRAGMENT" => -51,
        "SEEMS_TO_BE" => -51,
        "MD_NN" => -60,
        "I_THINK_FEEL" => -60,
        "KNOW_AWARE_REDO" => -60,
        "EN_REDUNDANCY_REPLACE" => -510,
        "EN_PLAIN_ENGLISH_REPLACE" => -511,
        "REP_PASSIVE_VOICE" => -599,
        "FOUR_NN" => -599,
        "THREE_NN" => -600,
        "SENT_START_NUM" => -600,
        "PASSIVE_VOICE" => -600,
        "EG_NO_COMMA" => -600,
        "IE_NO_COMMA" => -600,
        "REASON_WHY" => -600,
        "ALL_UPPERCASE" => -1000,
        _ => return None,
    })
}
fn de_priority_table(id: &str) -> Option<i32> {
    Some(match id {
        "DE_PROHIBITED_PHRASE" => 11,
        "WRONG_SPELLING_PREMIUM_INTERNAL" => 10,
        "OLD_SPELLING_RULE" => 10,
        "DE_COMPOUNDS" => 10,
        "E_MAIL_SIGNATUR" => 10,
        "TELEFON_NR" => 10,
        "IRGEND_COMPOUND" => 10,
        "DA_DURCH" => 2,
        "BEI_GOOGLE" => 2,
        "EINE_ORIGINAL_RECHNUNG_TEST" => 2,
        "VON_SEITEN_RECOMMENDATION" => 2,
        "AUFFORDERUNG_SIE" => 2,
        "WEIS_ICH" => 2,
        "VONSTATTEN_GEHEN" => 2,
        "VERWECHSLUNG_MIR_DIR_MIR_DIE" => 1,
        "ERNEUERBARE_ENERGIEN" => 1,
        "DRIVE_IN" => 1,
        "DIES_MAL" => 1,
        "AN_STATT" => 1,
        "VOR_BEI" => 1,
        "SUB_VER_KLEIN" => 1,
        "ALLES_GUTE" => 1,
        "NEUN_NEUEN" => 1,
        "VERWANDET_VERWANDTE" => 1,
        "IN_DEUTSCHE_SPRACHE" => 1,
        "SCHMIERE_STEHEN" => 1,
        "UEBER_EIN_MANGEL" => 1,
        "SEIT_LAENGEREN" => 1,
        "WIR_GEFUEHL" => 1,
        "VORHER_NACHHER_BILD" => 1,
        "SEIT_KLEIN_AUF" => 1,
        "SEIT_GEBURT_AN" => 1,
        "WO_VON" => 1,
        "ICH_BIN_STAND_JETZT_KOMMA" => 1,
        "EIN_LOGGEN" => 1,
        "ZU_GENÜGE" => 1,
        "SEIT_BEKANNT_WERDEN" => 1,
        "IMPFLICHT" => 1,
        "NULL_KOMMA_NICHTS" => 1,
        "ZWEI_AN_HALB" => 1,
        "BLUETOOTH_LAUTSPRECHER" => 1,
        "KOENNT_ICH" => 1,
        "WIR_HABE" => 1,
        "DAS_IST_GLAUBE_ICH_EGAL" => 1,
        "ICH_KOENNT" => 1,
        "HAT_DU" => 1,
        "HAST_DICH" => 1,
        "GRUNDE" => 1,
        "EIN_FACH" => 1,
        "WOGEN_SUBST" => 1,
        "SO_WIES_IST" => 1,
        "SICH_SICHT" => 1,
        "MIT_VERANTWORTLICH" => 1,
        "VOR_LACHEN" => 1,
        "TOUREN_SUBST" => 1,
        "AUF_DRÄNGEN" => 1,
        "AUF_ZACK" => 1,
        "UNTER_DRUCK" => 1,
        "ZUCCHINIS" => 1,
        "PASSWORTE" => 1,
        "ANGL_PA_ED_UNANGEMESSEN" => 1,
        "ANFUEHRUNGSZEICHEN_DE_AT" => 1,
        "ANFUEHRUNGSZEICHEN_CH_FR" => 1,
        "EMAIL" => 1,
        "IM_STICH_LASSEN" => 1,
        "ZULANGE" => 1,
        "ROCK_N_ROLL" => 1,
        "JOE_BIDEN" => 1,
        "RESOURCE_RESSOURCE" => 1,
        "ANS_OHNE_APOSTROPH" => 1,
        "DIESEN_JAHRES" => 1,
        "TAG_EIN_TAG_AUS" => 1,
        "WERT_SEIN" => 1,
        "EBEN_FALLS" => 1,
        "DA_DRAUS" => 1,
        "AUSSER_ORDENTLICH" => 1,
        "IN_UND_AUSWENDIG" => 1,
        "HIER_MIT" => 1,
        "HIER_FUER" => 1,
        "MIT_REISSEN" => 1,
        "JEDEN_FALLS" => 1,
        "MOEGLICHER_WEISE_ETC" => 1,
        "UST_ID" => 1,
        "INS_FITNESS" => 1,
        "MIT_UNTER" => 1,
        "SEIT_VS_SEID" => 1,
        "ZU_KOMMEN_LASSEN" => 1,
        "ZU_SCHICKEN_LASSEN" => 1,
        "IM_UM" => 1,
        "EINEN_VERSUCH_WERT" => 1,
        "DASS_DAS_PA2_DAS_PROIND" => 1,
        "AUF_BITTEN" => 1,
        "MEINET_WEGEN" => 1,
        "FUER_INSBESONDERE" => 1,
        "COVID_19" => 1,
        "DA_VOR" => 1,
        "DAS_WUENSCHE_ICH" => 1,
        "KLEINSCHREIBUNG_MAL" => 1,
        "VERINF_DAS_DASS_SUB" => 1,
        "IM_ALTER" => 1,
        "DAS_ALTER" => 1,
        "VER_INF_PKT_VER_INF" => 1,
        "DASS_MIT_VERB" => 1,
        "AB_TEST" => 1,
        "BZGL_ABK" => 1,
        "DURCH_WACHSEN" => 1,
        "ICH_WARTE" => 1,
        "RUNDUM_SORGLOS_PAKET" => 1,
        "MIT_FREUNDLICHEN_GRUESSE" => 1,
        "OK" => 1,
        "EINE_ORIGINAL_RECHNUNG" => 1,
        "VALENZ_TEST" => 1,
        "WAEHRUNGSANGABEN_CHF" => 1,
        "FALSCHES_ANFUEHRUNGSZEICHEN" => -1,
        "VER_KOMMA_PRO_RIN" => -1,
        "VER_INF_VER_INF" => -1,
        "DE_COMPOUND_COHERENCY" => -1,
        "GEFEATURED" => -1,
        "NUMBER_SUB" => -1,
        "MFG" => -1,
        "VER123_VERAUXMOD" => -1,
        "DE_AGREEMENT" => -1,
        "DE_AGREEMENT2" => -1,
        "KOMMA_NEBEN_UND_HAUPTSATZ" => -1,
        "FALSCHES_RELATIVPRONOMEN" => -1,
        "AKZENT_STATT_APOSTROPH" => -1,
        "BEENDE_IST_SENTEND" => -1,
        "VER_ADJ_ZU_SCHLAFEN" => -1,
        "LT_RECOMMENDATION" => -1,
        "MIO_PUNKT" => -1,
        "AUSLASSUNGSPUNKTE_LEERZEICHEN" => -1,
        "IM_ERSCHEINUNG_SPELLING_RULE" => -1,
        "SPACE_BEFORE_OG" => -1,
        "VERSEHENTLICHERWEISE" => -1,
        "VERMOD_SKIP_VER_PKT" => -1,
        "N_NETTER_TYP" => -1,
        "EINZELBUCHSTABE_PREMIUM" => -2,
        "ART_IND_ADJ_SUB" => -2,
        "KATARI" => -2,
        "SCHOENE_WETTER" => -2,
        "MEIN_KLEIN_HAUS" => -2,
        "UNPAIRED_BRACKETS" => -2,
        "DE_UNPAIRED_QUOTES" => -2,
        "ICH_GLAUBE_FUER_EUCH" => -2,
        "OBJECT_AGREEMENT" => -2,
        "ICH_INF_PREMIUM" => -2,
        "MEHRERE_WOCHE_PREMIUM" => -2,
        "DOPPELTER_NOMINATIV" => -2,
        "KUDAMM" => -2,
        "ALTERNATIVEN_FUER_ANGLIZISMEN" => -2,
        "ANGLIZISMUS_INTERNAL" => -2,
        "DOPPELUNG_VER_MOD_AUX" => -2,
        "AERZTEN_INNEN" => -2,
        "ANGLIZISMEN" => -2,
        "ANGLIZISMUS_PA_MIT_ED" => -2,
        "MEINSTE" => -2,
        "ZAHL_IM_WORT" => -2,
        "ICH_LIEBS" => -2,
        "WENNS_UND_ABERS" => -2,
        "ABERS_SATZANFANG_SPELLING_RULE" => -2,
        "VERNEB" => -2,
        "ZAHL_IM_WORT_SPELLING_RULE" => -2,
        "GERMAN_SPELLER_RULE" => -3,
        "AUSTRIAN_GERMAN_SPELLER_RULE" => -3,
        "SWISS_GERMAN_SPELLER_RULE" => -3,
        "DE_VERBAGREEMENT" => -4,
        "UPPERCASE_SENTENCE_START" => -4,
        "PUNKT_ENDE_DIREKTE_REDE" => -4,
        "LEERZEICHEN_NACH_VOR_ANFUEHRUNGSZEICHEN" => -4,
        "ZEICHENSETZUNG_DIREKTE_REDE" => -4,
        "GROSSSCHREIBUNG_WOERTLICHER_REDE" => -4,
        "IM_IHM_SPELLING_RULE" => -4,
        "IN_UNKNOWNKLEIN_VER" => -4,
        "SEHR_GEEHRTER_NAME" => -4,
        "DE_PHRASE_REPETITION" => -4,
        "FRAGEZEICHEN_NACH_DIREKTER_REDE" => -4,
        "PUNCTUATION_PARAGRAPH_END" => -4,
        "F_ANSTATT_PH_2" => -4,
        "DAS_WETTER_IST" => -5,
        "VEREIZ_VERINF_PKT" => -5,
        "WER_STARK_SCHWITZ" => -5,
        "VERBEN_PRAEFIX_AUS" => -5,
        "ANFUEHRUNG_VERSCHACHTELT" => -5,
        "SATZBAU_AN_DEN_KOMMT" => -5,
        "SUBJECT_VERB_AGREEMENT" => -5,
        "SAGT_SAGT" => -9,
        "PUNKT_ENDE_ABSATZ" => -10,
        "KOMMA_VOR_RELATIVSATZ" => -10,
        "VON_LEBENSLAEUFE_SPELLING_RULE" => -12,
        "VER_WER_VER_3" => -12,
        "PA_WAS" => -12,
        "ICH_GEHE_DU_BLEIBST" => -12,
        "PROPERNOMSIN_VERIMPSIN" => -12,
        "DIESE_HABE_ER_BELEIDIGTE" => -12,
        "VER123_VERAUXMOD_TEST1" => -12,
        "ZUSAMMENGESETZTE_VERBEN" => -12,
        "PRP_VER_PRGK" => -13,
        "COMMA_IN_FRONT_RELATIVE_CLAUSE" => -13,
        "SAGT_RUFT" => -13,
        "KANNST_WERDEN" => -13,
        "KOMMA_ZWISCHEN_HAUPT_UND_NEBENSATZ_2" => -14,
        "MAN_SIEHT_SEHR_SCHOEN" => -14,
        "BEI_VERB" => -14,
        "MODALVERB_FLEKT_VERB" => -14,
        "DATIV_NACH_PRP" => -14,
        "DAT_ODER_AKK_NACH_PRP" => -14,
        "SENT_START_SIN_PLU" => -14,
        "SENT_START_PLU_SIN" => -14,
        "VER_INFNOMEN" => -14,
        "GERMAN_WORD_REPEAT_RULE" => -15,
        "TOO_LONG_PARAGRAPH" => -15,
        "ALL_UPPERCASE" => -15,
        "NUR_LEDIGLICH" => -16,
        "STYLE_KEINE_AHNUNG" => -16,
        "COMMA_BEHIND_RELATIVE_CLAUSE" => -52,
        "DOPPELUNG_MODALVERB" => -52,
        "VER_DOPPELUNG" => -52,
        "DEF_ARTIKEL_INDEF_ADJ" => -52,
        "PRP_ADJ_AGREEMENT" => -52,
        "SIE_WOLLTEN_SIND" => -52,
        "ART_ADJ_SOL" => -52,
        "WURDEN_WORDEN_1" => -52,
        "WAR_WAHR" => -52,
        "KOMMA_ZWISCHEN_HAUPT_UND_NEBENSATZ" => -53,
        "VERB_IST" => -53,
        "WAR_WERDEN" => -53,
        "INF_VER_MOD_SPELLING_RULE" => -53,
        "DOPPELTES_VERB" => -53,
        "VERB_FEM_SUBST" => -54,
        "SUBJUNKTION_KOMMA_2" => -54,
        "DOPPELUNG_GLEICHES_VERB" => -55,
        "FEHLENDES_NOMEN" => -60,
        "REPETITIONS_STYLE" => -60,
        "GERMAN_WORD_REPEAT_BEGINNING_RULE" => -61,
        "TYPOGRAPHY" => -14,
        "COLLOQUIALISMS" => -15,
        "STYLE" => -15,
        "REDUNDANCY" => -15,
        "GENDER_NEUTRALITY" => -15,
        _ => return None,
    })
}
fn es_priority_table(id: &str) -> Option<i32> {
    Some(match id {
        "ES_SIMPLE_REPLACE_MULTIWORDS" => 50,
        "LOS_MAPUCHE" => 50,
        "TE_TILDE" => 50,
        "DE_TILDE" => 50,
        "PLURAL_SEPARADO" => 50,
        "PERSONAJES_FAMOSOS" => 50,
        "NO_SEPARADO" => 40,
        "PARTICIPIO_MS" => 40,
        "VERBO_MODAL_INFINITIVO" => 40,
        "EL_NO_TILDE" => 40,
        "SE_CREO" => 35,
        "POR_CIERTO" => 30,
        "DEGREE_CHAR" => 30,
        "LO_LOS" => 30,
        "ETCETERA" => 30,
        "P_EJ" => 30,
        "SE_CREO2" => 25,
        "ESPACIO_DESPUES_DE_PUNTO" => 25,
        "AGREEMENT_ADJ_NOUN_AREA" => 30,
        "PRONOMBRE_SIN_VERBO" => 20,
        "AGREEMENT_DET_ABREV" => 25,
        "MUCHO_NF" => 25,
        "AGREEMENT_DET_NOUN_EXCEPTIONS" => 25,
        "TYPOGRAPHY" => 20,
        "PRIMER_PRIMERA" => 20,
        "AGREEMENT_DET_NOUN" => 15,
        "AGREEMENT_DET_ADJ" => 10,
        "CONFUSION_ES_SE" => 20,
        "HALLA_HAYA" => 10,
        "VALLA_VAYA" => 10,
        "SI_AFIRMACION" => 10,
        "TE_TILDE2" => 10,
        "SEPARADO" => 1,
        "ES_SPLIT_WORDS" => -10,
        "U_NO" => -10,
        "EL_TILDE" => -10,
        "SINGLE_CHARACTER" => -15,
        "TOO_LONG_PARAGRAPH" => -15,
        "PREP_VERB" => -20,
        "SUBJUNTIVO_FUTURO" => -30,
        "SUBJUNTIVO_PASADO" => -30,
        "SUBJUNTIVO_PASADO2" => -30,
        "AGREEMENT_ADJ_NOUN" => -30,
        "AGREEMENT_PARTICIPLE_NOUN" => -30,
        "AGREEMENT_POSTPONED_ADJ" => -30,
        "MULTI_ADJ" => -30,
        "SUBJUNTIVO_INCORRECTO" => -40,
        "COMMA_SINO" => -40,
        "COMMA_SINO2" => -40,
        "VOSEO" => -40,
        "REPETITIONS_STYLE" => -50,
        "MORFOLOGIK_RULE_ES" => -100,
        "PHRASE_REPETITION" => -150,
        "SPANISH_WORD_REPEAT_RULE" => -150,
        "UPPERCASE_SENTENCE_START" => -200,
        "ES_QUESTION_MARK" => -250,
        _ => return None,
    })
}
fn fr_priority_table(id: &str) -> Option<i32> {
    Some(match id {
        "AGREEMENT_EXCEPTIONS" => 100,
        "EXPRESSIONS_VU" => 100,
        "SA_CA_SE" => 100,
        "SIL_VOUS_PLAIT" => 100,
        "QUASI_NOM" => 100,
        "MA" => 100,
        "SON_SONT" => 100,
        "JE_TES" => 100,
        "A_INFINITIF" => 100,
        "ON_ONT" => 100,
        "LEURS_LEUR" => 100,
        "DU_DU" => 100,
        "ACCORD_CHAQUE" => 100,
        "J_N2" => 100,
        "CEST_A_DIRE" => 100,
        "FAIRE_VPPA" => 100,
        "D_N_E_OU_E" => 100,
        "GENS_ACCORD" => 100,
        "VIRGULE_EXPRESSIONS_FIGEES" => 100,
        "TRAIT_UNION" => 100,
        "PLURIEL_AL2" => 100,
        "FR_SPLIT_WORDS_HYPHEN" => 100,
        "PAS_DE_TRAIT_UNION" => 50,
        "SOCIOCULTUREL" => 40,
        "A_VERBE_INFINITIF" => 20,
        "DE_OU_DES" => 20,
        "EMPLOI_EMPLOIE" => 20,
        "VOIR_VOIRE" => 20,
        "D_VPPA" => 20,
        "EST_CE_QUE" => 20,
        "CONFUSION_PARLEZ_PARLER" => 10,
        "AGREEMENT_TOUT_LE" => 10,
        "ESPACE_UNITES" => 10,
        "BYTES" => 10,
        "Y_A" => 10,
        "COTE" => 10,
        "PEUTETRE" => 10,
        "A_A_ACCENT" => 10,
        "A_ACCENT_A" => 10,
        "A_A_ACCENT2" => 10,
        "A_ACCENT" => 10,
        "JE_M_APPEL" => 10,
        "ACCORD_R_PERS_VERBE" => 10,
        "JE_SUI" => 10,
        "R_VAVOIR_VINF" => 10,
        "AN_EN" => 10,
        "APOS_M" => 10,
        "ACCORD_PLURIEL_ORDINAUX" => 10,
        "SUJET_AUXILIAIRE" => 10,
        "ADJ_ADJ_SENT_END" => 10,
        "OU_PAS" => 10,
        "PLACE_DE_LA_VIRGULE" => 10,
        "SE_CE" => -10,
        "PAS_DE_SOUCIS" => 10,
        "J_N" => -10,
        "TE_NV" => -20,
        "TE_NV2" => -10,
        "INTERROGATIVE_DIRECTE" => -10,
        "V_J_A_R" => -10,
        "TRES_TRES_ADJ" => -10,
        "IMP_PRON" => -10,
        "TOO_LONG_PARAGRAPH" => -15,
        "PREP_VERBECONJUGUE" => -20,
        "LA_LA2" => -20,
        "FRENCH_WORD_REPEAT_RULE" => -20,
        "PAS_DE_VERBE_APRES_POSSESSIF_DEMONSTRATIF" => -20,
        "VIRGULE_VERBE" => -20,
        "VERBES_FAMILIERS" => -25,
        "VERB_PRONOUN" => -50,
        "IL_VERBE" => -50,
        "A_LE" => -50,
        "ILS_VERBE" => -50,
        "AGREEMENT_POSTPONED_ADJ" => -50,
        "MULTI_ADJ" => -50,
        "PARENTHESES" => -50,
        "REP_ESSENTIEL" => -50,
        "CONFUSION_AL_LA" => -50,
        "LE_COVID" => -60,
        "FR_SPELLING_RULE" => -100,
        "VIRG_INF" => -100,
        "ET_SENT_START" => -151,
        "MAIS_SENT_START" => -151,
        "EN_CE_QUI_CONCERNE" => -152,
        "EN_MEME_TEMPS" => -152,
        "ET_AUSSI" => -152,
        "MAIS_AUSSI" => -152,
        "ELISION" => -200,
        "POINT" => -200,
        "REPETITIONS_STYLE" => -250,
        "POINTS_SUSPENSIONS_SPACE" => -250,
        "UPPERCASE_SENTENCE_START" => -300,
        "FRENCH_WHITESPACE_STRICT" => -350,
        "TOUT_MAJUSCULES" => -400,
        "VIRG_NON_TROUVEE" => -400,
        "POINTS_2" => -400,
        "MOTS_INCOMP" => -400,
        "FRENCH_WHITESPACE" => -400,
        "MOT_TRAIT_MOT" => -400,
        "FRENCH_WORD_REPEAT_BEGINNING_RULE" => -350,
        _ => return None,
    })
}
fn nl_priority_table(id: &str) -> Option<i32> {
    Some(match id {
        "TOO_LONG_SENTENCE" => -1,
        "SINT_X" => 3,
        "ET_AL" => 1,
        "N_PERSOONS" => 1,
        "HOOFDLETTERS_OVERBODIG_A" => 1,
        "VERSCHILLENDE_AANHALINGSTEKENS" => 1,
        "STAM_ZONDER_IK" => -1,
        "KOMMA_ONTBR" => -1,
        "KOMMA_KOMMA" => -1,
        "HET_FIETS" => -2,
        "JIJ_JOU_JOUW" => -2,
        "JOU_JOUW" => -3,
        "BE" => -3,
        "DOUBLE_PUNCTUATION" => -3,
        "EINDE_ZIN_ONVERWACHT" => -5,
        "TOO_LONG_PARAGRAPH" => -15,
        "ERG_LANG_WOORD" => -20,
        "DE_ONVERWACHT" => -20,
        _ => return None,
    })
}
fn pt_priority_table(id: &str) -> Option<i32> {
    Some(match id {
        "FRAGMENT_TWO_ARTICLES" => 50,
        "DEGREE_MINUTES_SECONDS" => 30,
        "INTERJECTIONS_PUNTUATION" => 20,
        "CONFUSION_POR_PÔR_V2" => 10,
        "PARONYM_POLITICA_523" => 10,
        "PARONYM_PRONUNCIA_262" => 10,
        "PARONYM_CRITICA_397" => 10,
        "PARONYM_INICIO_169" => 10,
        "LP_PARONYMS" => 10,
        "PARONYM_MUSICO_499_bis" => 10,
        "NA_NÃO" => 10,
        "VERB_COMMA_CONJUNCTION" => 10,
        "HOMOPHONE_AS_CARD" => 5,
        "TODOS_FOLLOWED_BY_NOUN_PLURAL" => 3,
        "TODOS_FOLLOWED_BY_NOUN_SINGULAR" => 2,
        "AUSENCIA_VIRGULA" => 1,
        "EMAIL" => 1,
        "UNPAIRED_BRACKETS" => -5,
        "PROFANITY" => -6,
        "PT_BARBARISMS_REPLACE" => -10,
        "BARBARISMS_PT_PT_V4" => -10,
        "PT_PT_SIMPLE_REPLACE" => -11,
        "PT_REDUNDANCY_REPLACE" => -12,
        "PT_WORDINESS_REPLACE" => -13,
        "PT_CLICHE_REPLACE" => -17,
        "INTERNET_ABBREVIATIONS" => -24,
        "CHILDISH_LANGUAGE" => -25,
        "ARCHAISMS" => -26,
        "INFORMALITIES" => -27,
        "BIASED_OPINION_WORDS" => -31,
        "PT_AGREEMENT_REPLACE" => -35,
        "CONTA_TO" => -44,
        "PT_DIACRITICS_REPLACE" => -45,
        "DIACRITICS" => -45,
        "PT_COMPOUNDS_POST_REFORM" => 1,
        "PORTUGUESE_OLD_SPELLING_INTERNAL" => -9,
        "AUX_VERBO" => -45,
        "ENSINO_A_DISTANCIA" => -45,
        "OQ_O_QUE_ORTHOGRAPHY" => -45,
        "PT_ENGLISH_CONTRACTION_ORTHOGRAPHY" => -45,
        "EMAIL_SEM_HIFEN" => -45,
        "PRETERITO_PERFEITO" => -51,
        "PT_BR_SIMPLE_REPLACE" => -51,
        "CRASE_CONFUSION" => -54,
        "NAO_MILITARES_CIVIS" => -54,
        "NA_QUELE" => -54,
        "NOTAS_FICAIS" => -54,
        "GENERAL_VERB_AGREEMENT_ERRORS" => -55,
        "GENERAL_NUMBER_AGREEMENT_ERRORS" => -56,
        "GENERAL_GENDER_NUMBER_AGREEMENT_ERRORS" => -56,
        "FINAL_STOPS" => -75,
        "EU_NÓS_REMOVAL" => -90,
        "COLOCAÇÃO_ADVÉRBIO" => -90,
        "FAZER_USO_DE-USAR-RECORRER" => -90,
        "FORMAL_T-V_DISTINCTION" => -100,
        "FORMAL_T-V_DISTINCTION_ALL" => -101,
        "REPEATED_WORDS" => -210,
        "PT_WIKIPEDIA_COMMON_ERRORS" => -500,
        "UPPERCASE_SENTENCE_START" => -600,
        "FILLER_WORDS_PT" => -990,
        "READABILITY_RULE_SIMPLE_PT" => -1100,
        "READABILITY_RULE_DIFFICULT_PT" => -1101,
        "UNKNOWN_WORD" => -2000,
        _ => return None,
    })
}

pub struct EnglishMatchFilter;

impl LanguageMatchFilter for EnglishMatchFilter {
    fn priority_for_id(&self, id: &str) -> i32 {
        if let Some(priority) = en_priority_table(id) {
            return priority;
        }
        if id.starts_with("EN_COMPOUNDS_") {
            return 2;
        }
        if id == "PRP_VBZ" {
            return -2;
        }
        if id.starts_with("CONFUSION_RULE_") {
            return -20;
        }
        if id == "EN_UPPER_CASE_NGRAM" {
            return -12;
        }
        if id.starts_with("EN_MULTITOKEN_SPELLING_") {
            return -9;
        }
        if id.starts_with("EN_GB_SIMPLE_REPLACE") {
            return -5;
        }
        if id.starts_with("EN_US_SIMPLE_REPLACE") {
            return -5;
        }
        if id == "QB_EN_OXFORD" {
            return -51;
        }
        if id.starts_with("EN_SIMPLE_REPLACE")
            && (id.ends_with("GRAMME") || id.ends_with("GRAMMES"))
        {
            return -49;
        }
        base_priority_for_id(id)
    }

    fn filter_rule_matches(&self, matches: &mut [RuleMatch], text: &str) {
        for rule_match in matches.iter_mut() {
            let error_str = original_error_str(rule_match, text).to_string();
            let starts_with_apostrophe = error_str.starts_with('\'');
            let starts_with_nt = error_str.starts_with("n't");
            if error_str.chars().count() > 2 {
                let mut new_replacements = Vec::with_capacity(rule_match.replacements().len());
                for replacement in rule_match.replacements() {
                    let mut value = replacement.value().to_string();
                    if starts_with_apostrophe
                        && !value.starts_with('\'')
                        && !value.starts_with('\u{2019}')
                        && !value.starts_with(' ')
                    {
                        value.insert(0, ' ');
                    }
                    if starts_with_nt
                        && !value.starts_with("n't")
                        && !value.starts_with("n\u{2019}t")
                    {
                        value.insert(0, ' ');
                    }
                    if new_replacements.iter().any(
                        |r: &og_core::rule_match::SuggestedReplacement| r.value() == value.as_str(),
                    ) {
                        continue;
                    }
                    let mut new_replacement = replacement.clone();
                    new_replacement.set_value(value);
                    new_replacements.push(new_replacement);
                }
                *rule_match.replacements_mut() = new_replacements;
            }
            let id = rule_match.rule().id();
            if id.starts_with("EN_SIMPLE_REPLACE")
                && (id.ends_with("GRAMME") || id.ends_with("GRAMMES"))
            {
                rule_match.rule_mut().set_issue_type("locale-violation");
            }
        }
    }
}

pub struct GermanMatchFilter;

impl LanguageMatchFilter for GermanMatchFilter {
    fn priority_for_id(&self, id: &str) -> i32 {
        if let Some(priority) = de_priority_table(id) {
            return priority;
        }
        if id.starts_with("DE_PROHIBITED_COMPOUNDS_")
            || id.starts_with("DE_PROHIBITED_COMPOUNDS_PREMIUM_")
        {
            return -4;
        }
        if id.starts_with("DE_MULTITOKEN_SPELLING") {
            return -2;
        }
        if id.starts_with("CONFUSION_RULE_") {
            return -1;
        }
        base_priority_for_id(id)
    }
}

pub struct SpanishMatchFilter;

impl LanguageMatchFilter for SpanishMatchFilter {
    fn priority_for_id(&self, id: &str) -> i32 {
        if id == "CONFUSIONS2" || id == "RARE_WORDS" {
            return 50;
        }
        if id == "MISSPELLING" || id == "CONFUSIONS" || id == "INCORRECT_EXPRESSIONS" {
            return 40;
        }
        if id == "DIACRITICS" {
            return 30;
        }
        if id.starts_with("ES_SIMPLE_REPLACE_SIMPLE") {
            return 30;
        }
        if id.starts_with("ES_COMPOUNDS") {
            return 50;
        }
        if let Some(priority) = es_priority_table(id) {
            return priority;
        }
        if id.starts_with("ES_MULTITOKEN_SPELLING") {
            return -95;
        }
        base_priority_for_id(id)
    }
}

pub struct FrenchMatchFilter;

impl LanguageMatchFilter for FrenchMatchFilter {
    fn priority_for_id(&self, id: &str) -> i32 {
        if let Some(priority) = fr_priority_table(id) {
            return priority;
        }
        if id.starts_with("FR_COMPOUNDS") {
            return 500;
        }
        if id == "CAT_TYPOGRAPHIE" || id == "CAT_TOURS_CRITIQUES" || id == "CAT_HOMONYMES_PARONYMES"
        {
            return 20;
        }
        if id == "SON" {
            return -5;
        }
        if id.starts_with("CAR") {
            return -50;
        }
        if id == "CONFUSION_RULE_PREMIUM" {
            return -50;
        }
        if id.starts_with("FR_MULTITOKEN_SPELLING_") {
            return -90;
        }
        if id.starts_with("FR_SIMPLE_REPLACE") {
            return 150;
        }
        if id.starts_with("grammalecte_") {
            return -150;
        }
        base_priority_for_id(id)
    }
}

pub struct DutchMatchFilter;

impl LanguageMatchFilter for DutchMatchFilter {
    fn priority_for_id(&self, id: &str) -> i32 {
        if id.starts_with("NL_SIMPLE_REPLACE") || id.starts_with("NL_SPACE_IN_COMPOUND") {
            return 1;
        }
        if let Some(priority) = nl_priority_table(id) {
            return priority;
        }
        if id.starts_with("AI_NL_HYDRA_LEO") {
            if id.starts_with("AI_NL_HYDRA_LEO_MISSING_COMMA") {
                return -51;
            }
            return -5;
        }
        base_priority_for_id(id)
    }
}

pub struct PortugueseMatchFilter;

impl LanguageMatchFilter for PortugueseMatchFilter {
    fn priority_for_id(&self, id: &str) -> i32 {
        if id.starts_with("MORFOLOGIK_RULE") {
            return -50;
        }
        if id.starts_with("PT_SIMPLE_REPLACE_ORTHOGRAPHY") {
            return -49;
        }
        if id.starts_with("PT_MULTITOKEN_SPELLING") {
            return -48;
        }
        if id.starts_with("ACENTUA\u{00c7}\u{00c3}O_VOGAL_\u{00ca}NCLISE") {
            return -51;
        }
        if id.starts_with("COLOCACAO_PRONOMINAL_COM_ATRATOR") {
            return -52;
        }
        if let Some(priority) = pt_priority_table(id) {
            return priority;
        }
        base_priority_for_id(id)
    }
}

pub struct PolishMatchFilter;

impl LanguageMatchFilter for PolishMatchFilter {
    fn priority_for_id(&self, id: &str) -> i32 {
        if id == "ZDANIA_ZLOZONE" {
            return -1;
        }
        base_priority_for_id(id)
    }
}

pub struct BelarusianMatchFilter;

impl LanguageMatchFilter for BelarusianMatchFilter {
    fn priority_for_id(&self, id: &str) -> i32 {
        match id {
            "RUSSIAN_SIMPLE_REPLACE_RULE" => 10,
            "BELARUSIAN_SPECIFIC_CASE" => 9,
            "Word_root_repeat" => -1,
            "PUNCT_DPT_2" => -2,
            "TOO_LONG_PARAGRAPH" => -15,
            _ => base_priority_for_id(id),
        }
    }
}

pub struct RussianMatchFilter;

impl LanguageMatchFilter for RussianMatchFilter {
    fn priority_for_id(&self, id: &str) -> i32 {
        crate::ru::russian_priority_for_id(id).unwrap_or_else(|| base_priority_for_id(id))
    }
}

pub fn for_code(code: &str) -> Arc<dyn LanguageMatchFilter> {
    let short = code.split('-').next().unwrap_or(code);
    match short {
        "en" => Arc::new(EnglishMatchFilter),
        "de" => Arc::new(GermanMatchFilter),
        "es" => Arc::new(SpanishMatchFilter),
        "fr" => Arc::new(FrenchMatchFilter),
        "nl" => Arc::new(DutchMatchFilter),
        "pt" => Arc::new(PortugueseMatchFilter),
        "pl" => Arc::new(PolishMatchFilter),
        "be" => Arc::new(BelarusianMatchFilter),
        "ru" => Arc::new(RussianMatchFilter),
        "ca" => Arc::new(crate::ca::match_filter::CatalanMatchFilter),
        _ => Arc::new(BaseLanguageMatchFilter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{RuleMatchContext, RuleMatchKind, RuleMatchRule, SuggestedReplacement};

    #[test]
    fn english_priorities_match_java() {
        let filter = EnglishMatchFilter;
        assert_eq!(filter.priority_for_id("I_E"), 10);
        assert_eq!(filter.priority_for_id("EVERY_NOW_AND_THEN"), 0);
        assert_eq!(filter.priority_for_id("EN_COMPOUNDS_FOO"), 2);
        assert_eq!(filter.priority_for_id("SOME_UNKNOWN"), 0);
        assert_eq!(filter.priority_for_id("TOO_LONG_SENTENCE"), -101);
    }

    #[test]
    fn german_priorities_match_java() {
        let filter = GermanMatchFilter;
        assert_eq!(filter.priority_for_id("VERB_FEM_SUBST"), -54);
        assert_eq!(filter.priority_for_id("DE_PROHIBITED_COMPOUNDS_X"), -4);
        assert_eq!(filter.priority_for_id("TYPOGRAPHY"), -14);
    }

    #[test]
    fn spanish_prefixes_come_after_map() {
        let filter = SpanishMatchFilter;
        assert_eq!(filter.priority_for_id("CONFUSIONS2"), 50);
        assert_eq!(filter.priority_for_id("MORFOLOGIK_RULE_ES"), -100);
        assert_eq!(filter.priority_for_id("ES_MULTITOKEN_SPELLING_X"), -95);
    }

    #[test]
    fn dutch_prefixes_come_before_map() {
        let filter = DutchMatchFilter;
        assert_eq!(filter.priority_for_id("NL_SIMPLE_REPLACE_ANYTHING"), 1);
        assert_eq!(filter.priority_for_id("NL_SPACE_IN_COMPOUND_X"), 1);
    }

    #[test]
    fn portuguese_prefixes_come_before_map() {
        let filter = PortugueseMatchFilter;
        assert_eq!(filter.priority_for_id("MORFOLOGIK_RULE_PT_PT"), -50);
    }

    #[test]
    fn french_compound_priority() {
        let filter = FrenchMatchFilter;
        assert_eq!(filter.priority_for_id("FR_COMPOUNDS_X"), 500);
        assert_eq!(filter.priority_for_id("SON"), -5);
    }

    #[test]
    fn unknown_languages_use_base_filter() {
        let filter = for_code("ru");
        assert_eq!(filter.priority_for_id("ZDANIA_ZLOZONE"), 0);
        assert_eq!(filter.priority_for_id("TOO_LONG_SENTENCE"), -101);
    }

    #[test]
    fn variants_map_to_their_language() {
        assert_eq!(for_code("de-CH").priority_for_id("VERB_FEM_SUBST"), -54);
        assert_eq!(for_code("en-GB").priority_for_id("I_E"), 10);
    }

    #[test]
    fn portuguese_priorities_follow_pt_pt_variant() {
        let filter = for_code("pt");
        assert_eq!(filter.priority_for_id("PT_COMPOUNDS_POST_REFORM"), 1);
        assert_eq!(
            filter.priority_for_id("PORTUGUESE_OLD_SPELLING_INTERNAL"),
            -9
        );
    }

    #[test]
    fn polish_priorities_match_java() {
        let filter = for_code("pl");
        assert_eq!(filter.priority_for_id("ZDANIA_ZLOZONE"), -1);
        assert_eq!(filter.priority_for_id("TOO_LONG_SENTENCE"), -101);
        assert_eq!(filter.priority_for_id("REPETITIONS_STYLE"), -55);
        assert_eq!(filter.priority_for_id("SOME_STYLE_RULE"), -50);
        assert_eq!(filter.priority_for_id("UNKNOWN_PL_RULE"), 0);
    }

    #[test]
    fn catalan_filter_is_registered_for_code() {
        assert_eq!(for_code("ca").priority_for_id("DEU_NI_DO"), 80);
        assert_eq!(
            for_code("ca").priority_for_id("MORFOLOGIK_RULE_CA_ES"),
            -100
        );
        assert_eq!(for_code("ca").priority_for_id("SOME_CA_RULE"), 0);
        assert_eq!(
            for_code("ca-ES-valencia").priority_for_id("CA_SIMPLE_REPLACE_MULTIWORDS_X"),
            70
        );
    }

    #[test]
    fn english_prefix_arms_match_java() {
        let filter = EnglishMatchFilter;
        assert_eq!(filter.priority_for_id("PRP_VBZ"), -2);
        assert_eq!(filter.priority_for_id("CONFUSION_RULE_X"), -20);
        assert_eq!(filter.priority_for_id("EN_UPPER_CASE_NGRAM"), -12);
        assert_eq!(filter.priority_for_id("EN_MULTITOKEN_SPELLING_X"), -9);
        assert_eq!(filter.priority_for_id("EN_GB_SIMPLE_REPLACE_X"), -5);
        assert_eq!(filter.priority_for_id("EN_US_SIMPLE_REPLACE_X"), -5);
        assert_eq!(filter.priority_for_id("QB_EN_OXFORD"), -51);
        assert_eq!(filter.priority_for_id("EN_SIMPLE_REPLACE_KILOGRAMME"), -49);
        assert_eq!(filter.priority_for_id("EN_SIMPLE_REPLACE_PLURAL"), 0);
    }

    fn match_over(text: &str, error: &str, id: &str, replacements: &[&str]) -> RuleMatch {
        let offset = text.find(error).expect("error occurs in text");
        RuleMatch::new(
            "message",
            offset,
            error.len(),
            RuleMatchRule::new(id, "description"),
            RuleMatchContext::new(text, 0, text.len()),
        )
        .with_replacements(
            replacements
                .iter()
                .map(|r| SuggestedReplacement::new(*r))
                .collect(),
        )
    }

    fn replacement_values(matches: &[RuleMatch]) -> Vec<String> {
        matches[0]
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect()
    }

    #[test]
    fn english_apostrophe_errors_space_the_replacements() {
        let filter = EnglishMatchFilter;
        let mut matches = vec![match_over(
            "listen to 'em now",
            "'em",
            "SOME_RULE",
            &["them", "'em", "’em", " them"],
        )];
        filter.filter_rule_matches(&mut matches, "listen to 'em now");
        assert_eq!(replacement_values(&matches), vec![" them", "'em", "’em"]);
    }

    #[test]
    fn english_nt_errors_space_the_replacements() {
        let filter = EnglishMatchFilter;
        let mut matches = vec![match_over(
            "they do n't know",
            "n't",
            "SOME_RULE",
            &["not", "n’t", "n't", "not"],
        )];
        filter.filter_rule_matches(&mut matches, "they do n't know");
        assert_eq!(replacement_values(&matches), vec![" not", "n’t", "n't"]);
    }

    #[test]
    fn english_short_errors_keep_replacements_untouched() {
        let filter = EnglishMatchFilter;
        let mut matches = vec![match_over(
            "the dog's bone",
            "'s",
            "SOME_RULE",
            &["is", "has"],
        )];
        filter.filter_rule_matches(&mut matches, "the dog's bone");
        assert_eq!(replacement_values(&matches), vec!["is", "has"]);
    }

    #[test]
    fn english_gramme_rules_become_locale_violations() {
        let filter = EnglishMatchFilter;
        let mut matches = vec![match_over(
            "one kilogramme of flour",
            "kilogramme",
            "EN_SIMPLE_REPLACE_KILOGRAMME",
            &["kilogram"],
        )];
        assert_eq!(matches[0].rule().pattern_match_kind(), RuleMatchKind::Other);
        filter.filter_rule_matches(&mut matches, "one kilogramme of flour");
        assert_eq!(replacement_values(&matches), vec!["kilogram"]);
        assert_eq!(matches[0].rule().pattern_match_kind(), RuleMatchKind::Hint);
    }

    #[test]
    fn german_multitoken_and_confusion_prefix_arms_match_java() {
        let filter = GermanMatchFilter;
        assert_eq!(filter.priority_for_id("DE_MULTITOKEN_SPELLING_X"), -2);
        assert_eq!(filter.priority_for_id("CONFUSION_RULE_X"), -1);
    }

    #[test]
    fn spanish_map_entries_match_java() {
        let filter = SpanishMatchFilter;
        assert_eq!(filter.priority_for_id("MISSPELLING"), 40);
        assert_eq!(filter.priority_for_id("CONFUSIONS"), 40);
        assert_eq!(filter.priority_for_id("INCORRECT_EXPRESSIONS"), 40);
        assert_eq!(filter.priority_for_id("DIACRITICS"), 30);
        assert_eq!(filter.priority_for_id("ES_SIMPLE_REPLACE_SIMPLE_X"), 30);
        assert_eq!(filter.priority_for_id("ES_COMPOUNDS_X"), 50);
    }

    #[test]
    fn dutch_map_and_hydra_priorities_match_java() {
        let filter = DutchMatchFilter;
        assert_eq!(filter.priority_for_id("SINT_X"), 3);
        assert_eq!(filter.priority_for_id("ET_AL"), 1);
        assert_eq!(filter.priority_for_id("N_PERSOONS"), 1);
        assert_eq!(filter.priority_for_id("HOOFDLETTERS_OVERBODIG_A"), 1);
        assert_eq!(filter.priority_for_id("VERSCHILLENDE_AANHALINGSTEKENS"), 1);
        assert_eq!(filter.priority_for_id("STAM_ZONDER_IK"), -1);
        assert_eq!(filter.priority_for_id("KOMMA_ONTBR"), -1);
        assert_eq!(filter.priority_for_id("KOMMA_KOMMA"), -1);
        assert_eq!(filter.priority_for_id("HET_FIETS"), -2);
        assert_eq!(filter.priority_for_id("JIJ_JOU_JOUW"), -2);
        assert_eq!(filter.priority_for_id("JOU_JOUW"), -3);
        assert_eq!(filter.priority_for_id("BE"), -3);
        assert_eq!(filter.priority_for_id("DOUBLE_PUNCTUATION"), -3);
        assert_eq!(filter.priority_for_id("EINDE_ZIN_ONVERWACHT"), -5);
        assert_eq!(filter.priority_for_id("TOO_LONG_PARAGRAPH"), -15);
        assert_eq!(filter.priority_for_id("ERG_LANG_WOORD"), -20);
        assert_eq!(filter.priority_for_id("DE_ONVERWACHT"), -20);
        assert_eq!(filter.priority_for_id("TOO_LONG_SENTENCE"), -1);
        assert_eq!(filter.priority_for_id("AI_NL_HYDRA_LEO_X"), -5);
        assert_eq!(
            filter.priority_for_id("AI_NL_HYDRA_LEO_MISSING_COMMA_X"),
            -51
        );
    }

    #[test]
    fn portuguese_map_and_prefix_arms_match_java() {
        let filter = PortugueseMatchFilter;
        assert_eq!(
            filter.priority_for_id("PT_SIMPLE_REPLACE_ORTHOGRAPHY_X"),
            -49
        );
        assert_eq!(filter.priority_for_id("PT_MULTITOKEN_SPELLING_X"), -48);
        assert_eq!(filter.priority_for_id("ACENTUAÇÃO_VOGAL_ÊNCLISE_X"), -51);
        assert_eq!(
            filter.priority_for_id("COLOCACAO_PRONOMINAL_COM_ATRATOR_X"),
            -52
        );
        assert_eq!(filter.priority_for_id("FRAGMENT_TWO_ARTICLES"), 50);
        assert_eq!(filter.priority_for_id("DEGREE_MINUTES_SECONDS"), 30);
        assert_eq!(filter.priority_for_id("INTERJECTIONS_PUNTUATION"), 20);
        assert_eq!(filter.priority_for_id("AUSENCIA_VIRGULA"), 1);
        assert_eq!(filter.priority_for_id("UNPAIRED_BRACKETS"), -5);
        assert_eq!(filter.priority_for_id("PROFANITY"), -6);
        assert_eq!(filter.priority_for_id("PT_PT_SIMPLE_REPLACE"), -11);
        assert_eq!(filter.priority_for_id("PT_REDUNDANCY_REPLACE"), -12);
        assert_eq!(filter.priority_for_id("PT_CLICHE_REPLACE"), -17);
        assert_eq!(filter.priority_for_id("CHILDISH_LANGUAGE"), -25);
        assert_eq!(filter.priority_for_id("ARCHAISMS"), -26);
        assert_eq!(filter.priority_for_id("INFORMALITIES"), -27);
        assert_eq!(filter.priority_for_id("PT_AGREEMENT_REPLACE"), -35);
        assert_eq!(filter.priority_for_id("PRETERITO_PERFEITO"), -51);
        assert_eq!(filter.priority_for_id("PT_BR_SIMPLE_REPLACE"), -51);
        assert_eq!(filter.priority_for_id("CRASE_CONFUSION"), -54);
        assert_eq!(filter.priority_for_id("GENERAL_VERB_AGREEMENT_ERRORS"), -55);
        assert_eq!(filter.priority_for_id("REPEATED_WORDS"), -210);
        assert_eq!(filter.priority_for_id("PT_WIKIPEDIA_COMMON_ERRORS"), -500);
        assert_eq!(filter.priority_for_id("UPPERCASE_SENTENCE_START"), -600);
        assert_eq!(filter.priority_for_id("FILLER_WORDS_PT"), -990);
        assert_eq!(filter.priority_for_id("READABILITY_RULE_SIMPLE_PT"), -1100);
        assert_eq!(filter.priority_for_id("UNKNOWN_WORD"), -2000);
    }

    #[test]
    fn belarusian_priorities_match_java() {
        let filter = for_code("be");
        assert_eq!(filter.priority_for_id("RUSSIAN_SIMPLE_REPLACE_RULE"), 10);
        assert_eq!(filter.priority_for_id("BELARUSIAN_SPECIFIC_CASE"), 9);
        assert_eq!(filter.priority_for_id("Word_root_repeat"), -1);
        assert_eq!(filter.priority_for_id("PUNCT_DPT_2"), -2);
        assert_eq!(filter.priority_for_id("TOO_LONG_PARAGRAPH"), -15);
        assert_eq!(filter.priority_for_id("TOO_LONG_SENTENCE"), -101);
        assert_eq!(filter.priority_for_id("UNKNOWN_BE_RULE"), 0);
    }
}
