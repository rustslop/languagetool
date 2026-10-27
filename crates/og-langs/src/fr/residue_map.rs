use std::collections::HashMap;
use std::hash::Hash;
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Family {
    TokenizerDecimalRun,
    TokenizerPercentWordChar,
    TokenizerLeadingHyphen,
    RawPosPreDisambiguation,
    SuppressMisspelledSuggestionSpeller,
    CommaSuggestionSentStartOffset,
    ScopeNextExceptionReadingZero,
    MatchNoZeroTextInterpolation,
    ApostropheNormalizationAsymmetry,
    ChunkInteriorLeadingSpacePos,
    UnifySkipAntipattern,
    AndGroupAntipatternParse,
    Differential,
}

impl Family {
    fn java_anchor(self) -> &'static str {
        match self {
            Self::TokenizerDecimalRun => {
                "FrenchWordTokenizer.java:133-134 DECIMAL_POINT replaceAll joins only the first non-overlapping digit.digit per scan position"
            }
            Self::TokenizerPercentWordChar => {
                "FrenchWordTokenizer.java:37 wordCharacters class contains %"
            }
            Self::TokenizerLeadingHyphen => {
                "FrenchWordTokenizer.java:159-162 strips leading hyphens into standalone '-' tokens before pattern split and dictionary lookup; FrenchWordTokenizerTest.java:92 [-, Oui,  , -, l', homme, .]"
            }
            Self::RawPosPreDisambiguation => {
                "PatternRuleHandler.java:299 raw_pos=yes; PatternRuleMatcher.java:88-90 matches on getPreDisambiguationTokensWithoutWhitespace(); AnalyzedSentence.java:59 keeps the pre-disambiguation copy"
            }
            Self::SuppressMisspelledSuggestionSpeller => {
                "XMLRuleHandler.java:377 + PatternRuleHandler.java:374-383 mark suppress_misspelled matches; MatchState.java:307-318 replace unknown suggestion parts with <mistake/>; PatternRuleMatcher.java:47-51 + :186-190 drop the match"
            }
            Self::CommaSuggestionSentStartOffset => {
                "PatternRuleMatcher.java:177-182 fromPos = prev.getStartPos() + prev.getToken().length() with the Java SENT_START token whose text is empty (JLanguageTool.java:1813)"
            }
            Self::ScopeNextExceptionReadingZero => {
                "AbstractPatternRulePerformer.java:184-190: the current element's scope-next exception is tested only when prevSkipNext == 0 and only against reading 0 of the next token"
            }
            Self::MatchNoZeroTextInterpolation => {
                "PatternToken.java:502-515 doCompile replaces the \\\\N placeholder inside the token string with the referenced token text (style.xml ECRITURE_INCLUSIVE <token><match no=\"0\"/>e</token>)"
            }
            Self::ApostropheNormalizationAsymmetry => {
                "Java pattern matching normalizes no apostrophes (no U+2018 handling in PatternToken.java or StringMatcher.java); the class [´‘′`] matches the raw token '‘'"
            }
            Self::ChunkInteriorLeadingSpacePos => {
                "MultiWordChunker.java:379-405 removePreviousTags rewrites every token of a chunk with the clean tag (getNextPosTag :449-458 returns 'A' for 'A'), so all four tokens of 'de plus en plus' carry exactly A"
            }
            Self::UnifySkipAntipattern => {
                "grammar.xml:30276-30294 OU sub-rule antipattern: unify person+number over [V.* skip=-1] ... [V.*] with the pronouns in unify-ignore aligns Je/vous/écrirai/ou/vous/téléphonerai sequentially and the two 1 s verbs unify, suppressing the rule"
            }
            Self::AndGroupAntipatternParse => {
                "grammar.xml:10022-10032 AGREEMENT_PARTICULAR antipattern [JN][C?][P?][D?][N s?p|N f .][and V ppr + J m s][punct] aligns niveau/d'/énergie/croissant/. and suppresses the rule"
            }
            Self::Differential => {
                "poursoi: 'poursoi' is a dict word (poursoi/N m s in both engines - FrenchTagger.java:100-101 nulls only non-dict words, pinned by FrenchTaggerTest.java:54 'nonexistingword'->[null]null), the sole blocker is <token postag=\"[DP].*\"/> against the sentence-final '.' whose readings are M fin,M fin,SENT_END (dict + JLanguageTool.java:1842-1850) and PatternToken.java:352-362 rejects that exactly like Rust does; upstream master ships the identical rule+example and its GitHub workflows run no tests, so only a JVM trace can show what Java does. nuance lilas: the fp is ADJ_ADJ_SENT_END sub-rule 5 (grammar.xml:14621) matching [J m sp]lilas/[et]/[A]très/[J f p]parfumées/'.' on readings identical in both engines (roses filtered to J e p by RP-ETRE_ADJ_AMBIG disambiguation.xml:8681-8690 in both); all 66 antipatterns (58 group-shared + 8 local) were aligned by hand and none matches - every near-miss dies on 'très' (A) sitting between 'et' and the final J, or on the sentence-final '.' where a trailing P/punct is required"
            }
        }
    }

    fn rust_target(self) -> &'static str {
        match self {
            Self::TokenizerDecimalRun => {
                "og-tokenizer/src/french_word_tokenizer.rs scan_word_end :291-295 chains digit.digit joins greedily; Java DECIMAL_POINT replaceAll resumes after the consumed digit.digit triple, so '32.1.1985' tokenizes ['32.1','.','1985'] - gate the branch so a dot directly after a consumed triple cannot re-anchor"
            }
            Self::TokenizerPercentWordChar => {
                "og-tokenizer/src/french_word_tokenizer.rs is_fr_word_char :52-58 is missing '%' (Java wordCharacters has °%‰‱), so '8%'/'51%' split"
            }
            Self::TokenizerLeadingHyphen => {
                "og-tokenizer/src/french_word_tokenizer.rs split_word_token lacks the Java tokenize() strip of leading hyphens into standalone '-' tokens before pattern matching, so '-l\\u{2019}homme' stays fused ['-', 'l\\u{2019}homme'] instead of Java ['-','-','l\\u{2019}','homme']"
            }
            Self::RawPosPreDisambiguation => {
                "og-xml/src/parser.rs (compile raw_pos) + og-core AnalyzedSentence (pre-disambiguation copy) + og-rules/src/pattern_rule.rs (token view selection)"
            }
            Self::SuppressMisspelledSuggestionSpeller => {
                "the filter routing landed (og-rules/src/fr_suppress.rs + the pattern_rule.rs:2814-2815 dispatch arm); remaining work is og-rules/src/pattern_rule.rs raw_match_alternatives/suggestion_alternatives - the MatchState.java:307-318 pass that rewrites unknown suggestion parts to <mistake/> before accept_rule_match nulls the match"
            }
            Self::CommaSuggestionSentStartOffset => {
                "og-rules/src/pattern_rule.rs:627 and :639 (use prev.token().end(), not start + text length)"
            }
            Self::ScopeNextExceptionReadingZero => "og-rules/src/pattern_rule.rs:1352-1357",
            Self::MatchNoZeroTextInterpolation => {
                "og-xml/src/parser.rs (keep the in-text match placeholder) + og-rules/src/pattern_rule.rs backreference_matches"
            }
            Self::ApostropheNormalizationAsymmetry => {
                "og-xml/src/unifier.rs:8-13 + og-rules/src/pattern_rule.rs:2164-2188; not shim-able from fr: U+2018 maps to the typewriter apostrophe the Java class deliberately excludes, so normalizing the pattern side would create false positives on correct examples"
            }
            Self::ChunkInteriorLeadingSpacePos => {
                "og-tagger multiword chunker remove-previous-tags path: the fr dump shows chunk tokens 1 and 3 ('de', 'en') carrying pos ' A' with a leading space while tokens 2 and 4 carry 'A', so exact postag=\"A\" elements (IL_DEVIENT_QUOI grammar.xml:125137) never match them"
            }
            Self::UnifySkipAntipattern => {
                "og-rules/src/pattern_rule.rs antipattern evaluation: the standalone suppressor (probe in fr/diagnose.rs fr_diagnose_suppressor_patterns) never matches, so unify + skip=-1 + unify-ignore inside antipatterns is not evaluated (antipattern_unify_satisfied pattern_rule.rs:2399 exists but this shape stays dead)"
            }
            Self::AndGroupAntipatternParse => {
                "og-xml/src/parser.rs: the standalone equivalent of the antipattern fails to COMPILE with IllFormed(MismatchedEndTag { expected: \"token\", found: \"pattern\" }) on 4/4 runs (optionals + and-group + text-bearing token), and a neighbor shape flipped across runs - parse is shape-sensitive and nondeterministic for these constructs"
            }
            Self::Differential => {
                "unresolved: poursoi = the required [DP].* next-token postag at sentence end (if a JVM trace shows Java matching, the fix site is og-rules pattern_rule.rs end-of-sentence semantics for a required next-token element; if Java fails it too, the example is upstream-broken and the residue becomes accepted-excluded material); nuance lilas = whichever Java-side mechanism suppresses ADJ_ADJ_SENT_END sub 5 - the 66 antipatterns carry no static candidate, so the trace must name it"
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TokenizerDecimalRun => "tokenizer decimal run",
            Self::TokenizerPercentWordChar => "tokenizer % word char",
            Self::TokenizerLeadingHyphen => "tokenizer leading hyphen",
            Self::RawPosPreDisambiguation => "raw_pos pre-disambiguation",
            Self::SuppressMisspelledSuggestionSpeller => "suppress_misspelled suggestion speller",
            Self::CommaSuggestionSentStartOffset => "comma-suggestion SENT_START offset",
            Self::ScopeNextExceptionReadingZero => "scope-next exception reading-0",
            Self::MatchNoZeroTextInterpolation => "<match no=\"0\"/> text interpolation",
            Self::ApostropheNormalizationAsymmetry => "apostrophe normalization asymmetry",
            Self::ChunkInteriorLeadingSpacePos => "chunk-interior leading-space pos",
            Self::UnifySkipAntipattern => "unify+skip antipattern",
            Self::AndGroupAntipatternParse => "and-group antipattern parse",
            Self::Differential => "differential",
        }
    }
}

const GRAMMAR_RESIDUES: &[(&str, &str, Family)] = &[
    (
        "MOTS_MAUX",
        "Antidote, le remède à tous vos mots.",
        Family::ScopeNextExceptionReadingZero,
    ),
    (
        "MOTS_MAUX",
        "Docteur délivrez moi de mes mots.",
        Family::ScopeNextExceptionReadingZero,
    ),
    (
        "IL_DEVIENT_QUOI",
        "Il devient de plus en plus.",
        Family::ChunkInteriorLeadingSpacePos,
    ),
    (
        "ALORS",
        "alors il faut partir",
        Family::CommaSuggestionSentStartOffset,
    ),
    (
        "DANS_CE_CAS",
        "dans ce cas il est certain",
        Family::CommaSuggestionSentStartOffset,
    ),
    (
        "AGREEMENT_PARTICULAR",
        "Sa configuration électronique est par ordre de niveau d'énergie croissant.",
        Family::AndGroupAntipatternParse,
    ),
    (
        "ADJ_ADJ_SENT_END",
        "Quand Ddaykha étudiait à l'université, elle n'avait ni ami ni amie.",
        Family::MatchNoZeroTextInterpolation,
    ),
    (
        "ADJ_ADJ_SENT_END",
        "Ses grandes fleurs doubles sont roses à nuance lilas et très parfumées.",
        Family::Differential,
    ),
    (
        "PEUT_ETRE",
        "Le canon peut tourner sur 360 degrés et peut-être élevé à un angle de 90 degrés.",
        Family::ScopeNextExceptionReadingZero,
    ),
    (
        "PEUT_PEUX_PEU",
        "Quelqu\"un peut -il m'éclairer et si possible en justifiant.",
        Family::TokenizerLeadingHyphen,
    ),
    (
        "OU",
        "Je vous écrirai ou vous téléphonerai la semaine prochaine.",
        Family::UnifySkipAntipattern,
    ),
    (
        "CE_SE",
        "Il faut tout ce dire.",
        Family::RawPosPreDisambiguation,
    ),
    (
        "CE_SE",
        "C'est lui qui ce cache sous la table.",
        Family::RawPosPreDisambiguation,
    ),
    (
        "CE_SE",
        "Cela ce passe sous la table.",
        Family::RawPosPreDisambiguation,
    ),
    (
        "CE_SE",
        "Qu'attent-il pour ce lancer ?",
        Family::RawPosPreDisambiguation,
    ),
    (
        "CE_SE",
        "Il ne le fait pas pour ce repentir.",
        Family::RawPosPreDisambiguation,
    ),
    (
        "CE_SE",
        "Il a décidé de ce lever tôt.",
        Family::RawPosPreDisambiguation,
    ),
    (
        "VOIX_VOIE",
        "Le candidat a obtenu 51% des voies.",
        Family::TokenizerPercentWordChar,
    ),
    (
        "DU_DU",
        "C'est trop, tu n'aurais pas du savoir !",
        Family::RawPosPreDisambiguation,
    ),
    (
        "MOTS_COLLES_OU_SEPARES",
        "Il le fait poursoi.",
        Family::Differential,
    ),
    (
        "D_N_QUE_R_AVOIR_PPA",
        "Voici la piste qu'aurait négligé les enqêteurs.",
        Family::ScopeNextExceptionReadingZero,
    ),
    (
        "PRONSUJ_NONVERBE",
        "Je ne sé compris.",
        Family::ScopeNextExceptionReadingZero,
    ),
    (
        "ETRE_VPPA_OU_ADJ",
        "De leur rencontre est donc né ce lieu.",
        Family::ScopeNextExceptionReadingZero,
    ),
    (
        "ACCORD_SUJET_VERBE",
        "De quels sites parles -tu?",
        Family::TokenizerLeadingHyphen,
    ),
    (
        "PREFIXES_NON_SEPARES",
        "Pour revenir au post précédent.",
        Family::SuppressMisspelledSuggestionSpeller,
    ),
    (
        "PREFIXES_NON_SEPARES",
        "Miguel Diaz-Canel est porté sur les fonts néo-post-castristes.",
        Family::SuppressMisspelledSuggestionSpeller,
    ),
    (
        "PREFIXES_NON_SEPARES",
        "Elle poursuit les séries et aligne sur la post saison 14,3 points",
        Family::SuppressMisspelledSuggestionSpeller,
    ),
    (
        "ESPACE_POURCENT",
        "Une croissance de 8%.",
        Family::TokenizerPercentWordChar,
    ),
    (
        "ESPACE_POURCENT",
        "Une croissance de 8,5%.",
        Family::TokenizerPercentWordChar,
    ),
    (
        "PAS_DE_TRAIT_UNION",
        "Le néo-impressionnisme",
        Family::SuppressMisspelledSuggestionSpeller,
    ),
    (
        "TRAIT_UNION_ESPACE",
        "Vas -tu les regarder à la télévision ?",
        Family::TokenizerLeadingHyphen,
    ),
    (
        "APOS_INCORRECT",
        "D‘enfants.",
        Family::ApostropheNormalizationAsymmetry,
    ),
    (
        "TIRET",
        "-il est arrivé à l'hôtel hier.",
        Family::TokenizerLeadingHyphen,
    ),
    (
        "DATE",
        "Il est né le 32.1.1985.",
        Family::TokenizerDecimalRun,
    ),
    (
        "DATE",
        "Il est né le 300.1.1985.",
        Family::TokenizerDecimalRun,
    ),
    (
        "DATE",
        "Il est né le 3000.1.1985.",
        Family::TokenizerDecimalRun,
    ),
    (
        "DATE",
        "Elle est née le 31.4.1989.",
        Family::TokenizerDecimalRun,
    ),
    (
        "DATE",
        "Elle est née le 30.2.1900.",
        Family::TokenizerDecimalRun,
    ),
    (
        "DATE",
        "Elle est née le 31.2.1900.",
        Family::TokenizerDecimalRun,
    ),
    (
        "DATE",
        "Elle est née le 29.2.1900.",
        Family::TokenizerDecimalRun,
    ),
    ("DATE", "Le 29.2.2100.", Family::TokenizerDecimalRun),
];

const STYLE_RESIDUES: &[(&str, &str, Family)] = &[(
    "ECRITURE_INCLUSIVE",
    "Pour l'étudiant ou étudiante.",
    Family::MatchNoZeroTextInterpolation,
)];

pub fn families_summary() -> Vec<(&'static str, usize, usize)> {
    let mut counts: HashMap<Family, (usize, usize)> = HashMap::new();
    for (_, _, family) in GRAMMAR_RESIDUES {
        counts.entry(*family).or_default().0 += 1;
    }
    for (_, _, family) in STYLE_RESIDUES {
        counts.entry(*family).or_default().1 += 1;
    }
    let mut out: Vec<_> = counts
        .into_iter()
        .map(|(family, (grammar, style))| (family.label(), grammar, style))
        .collect();
    out.sort_unstable();
    out
}

fn fr_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("fr/src/main/resources/org/languagetool/rules/fr"))
}

fn rule_file(rule_id: &str) -> Option<String> {
    for file in ["grammar.xml", "style.xml"] {
        let xml = std::fs::read_to_string(fr_rules_dir()?.join(file)).ok()?;
        let needle = format!("id=\"{rule_id}\"");
        if xml.contains(&needle) {
            return Some(xml);
        }
    }
    None
}

fn compiled_rule_count() -> usize {
    static COUNT: OnceLock<usize> = OnceLock::new();
    *COUNT.get_or_init(|| {
        let mut total = 0usize;
        for file in ["grammar.xml", "style.xml"] {
            let Some(path) = fr_rules_dir().map(|dir| dir.join(file)) else {
                continue;
            };
            let Ok(xml) = std::fs::read_to_string(&path) else {
                continue;
            };
            let xml = crate::fr::normalize_fr_rules_xml(&xml);
            let Ok(set) =
                og_xml::compiler::XmlCompiler::new().compile_file_with_base(&xml, path.parent())
            else {
                continue;
            };
            total += set.rules.len();
        }
        total
    })
}

pub fn dump() {
    eprintln!(
        "fr residue map: {} grammar + {} style",
        GRAMMAR_RESIDUES.len(),
        STYLE_RESIDUES.len()
    );
    for (label, grammar, style) in families_summary() {
        eprintln!("  {label:<42} grammar {grammar:>2}  style {style:>2}");
    }
    for (rule_id, text, family) in GRAMMAR_RESIDUES.iter().chain(STYLE_RESIDUES.iter()) {
        eprintln!(
            "  [{rule_id}] {text:?} => {} | {} | {}",
            family.label(),
            family.java_anchor(),
            family.rust_target()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn dump_residue_map() {
        let _release_guard = crate::release_guard();
        super::dump();
    }

    #[test]
    fn residue_counts_match_the_suites() {
        assert_eq!(GRAMMAR_RESIDUES.len(), 41, "grammar.xml residue inventory");
        assert_eq!(STYLE_RESIDUES.len(), 1, "style.xml residue inventory");
        let differential: Vec<_> = GRAMMAR_RESIDUES
            .iter()
            .chain(STYLE_RESIDUES.iter())
            .filter(|(_, _, f)| *f == Family::Differential)
            .map(|(id, text, _)| (*id, *text))
            .collect();
        assert_eq!(
            differential.len(),
            2,
            "unresolved differential residues (ADJ_ADJ_SENT_END 'nuance lilas': exactly sub-rule 5 \
             grammar.xml:14621 fires, span 48..73 over lilas/et/très/parfumées, on readings \
             identical in both engines - roses is J e p in both after RP-ETRE_ADJ_AMBIG \
             disambiguation.xml:8681-8690 - and none of the 66 antipatterns (58 group-shared + \
             8 local) aligns by static reading; MOTS_COLLES_OU_SEPARES 'poursoi': 'poursoi' is a \
             dict word poursoi/N m s in both engines, sub-rule 30 grammar.xml:47526-47536 matches \
             11..18 whenever a real [DP] token follows, and the only blocker is <token \
             postag=\"[DP].*\"/> against the sentence-final '.' = M fin,M fin,SENT_END, which \
             PatternToken.java:352-362 rejects in Java exactly as in Rust - both need a JVM \
             differential to name the Java-side behavior): {differential:?}"
        );
    }

    #[test]
    fn il_devient_quoi_chunk_tokens_carry_leading_space_pos() {
        let sentence = &crate::fr::test_support::analyze_french("Il devient de plus en plus.")[0];
        let surface_pos: Vec<(&str, Vec<String>)> = sentence
            .non_whitespace_tokens()
            .iter()
            .filter(|t| matches!(t.token().token(), "de" | "plus" | "en"))
            .map(|t| {
                (
                    t.token().token(),
                    t.readings()
                        .iter()
                        .map(|r| r.pos_tags().join(" "))
                        .collect(),
                )
            })
            .collect();
        let expected: Vec<(&str, Vec<String>)> = [
            ("de", vec![" A".to_string()]),
            ("plus", vec!["A".to_string()]),
            ("en", vec![" A".to_string()]),
            ("plus", vec!["A".to_string()]),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            surface_pos, expected,
            "Java MultiWordChunker.java:379-405 removePreviousTags + getNextPosTag :449-458 give \
             every token of the chunk the clean tag 'A'; the Rust chunker emits ' A' (leading \
             space) on chunk tokens 1 and 3, so the exact postag=\"A\" element of IL_DEVIENT_QUOI \
             (grammar.xml:125137) can never match them - this pin flips when the og-tagger \
             chunker normalizes the interior tag"
        );
    }

    fn compile_inline(xml: &str) -> Result<og_xml::compiler::CompiledRuleSet, String> {
        let dir = fr_rules_dir();
        og_xml::compiler::XmlCompiler::new()
            .compile_file_with_base(xml, dir.as_deref())
            .map_err(|err| format!("{err:?}"))
    }

    fn sub_rule(rule_id: &str, sub_id: &str) -> og_xml::compiler::CompiledRule {
        let dir = fr_rules_dir().expect("fr rules dir");
        let raw = std::fs::read_to_string(dir.join("grammar.xml")).expect("grammar.xml");
        let xml = crate::fr::normalize_fr_rules_xml(&raw);
        let set = og_xml::compiler::XmlCompiler::new()
            .compile_file_with_base(&xml, Some(&dir))
            .expect("compile grammar.xml");
        set.rules
            .into_iter()
            .find(|r| r.id == rule_id && r.sub_id.as_deref() == Some(sub_id))
            .unwrap_or_else(|| panic!("{rule_id}[{sub_id}] in grammar.xml"))
    }

    fn rule_match_spans(rule: &og_xml::compiler::CompiledRule, text: &str) -> Vec<(usize, usize)> {
        let engine = crate::fr::example_suite::build_pattern_engine();
        let pipeline = crate::fr::test_support::FrenchPipeline::shared();
        og_rules::pattern_rule::set_test_date_hack(true);
        let mut out = Vec::new();
        for (sentence, sentence_start) in pipeline.analyze_offset_text(text) {
            for m in engine.match_rule(rule, &sentence) {
                out.push((
                    m.offset() + sentence_start,
                    m.offset() + sentence_start + m.length(),
                ));
            }
        }
        out
    }

    const UNIFICATION_DEFS: &str = r#"
    <unification feature="number">
        <equivalence type="sg"><token postag=".* sp?" postag_regexp="yes"/></equivalence>
        <equivalence type="pl"><token postag=".* s?p" postag_regexp="yes"/></equivalence>
    </unification>
    <unification feature="person">
        <equivalence type="p1"><token postag=".* 1 .*" postag_regexp="yes"/></equivalence>
        <equivalence type="p2"><token postag=".* 2 .*" postag_regexp="yes"/></equivalence>
        <equivalence type="p3"><token postag=".* 3 .*" postag_regexp="yes"/></equivalence>
    </unification>"#;

    fn wrap_pattern(id: &str, pattern: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rules lang=\"fr\">{UNIFICATION_DEFS}\n\
             <rule id=\"{id}\">{pattern}<message>probe</message></rule>\n</rules>"
        )
    }

    #[test]
    fn ou_suppressor_unify_skip_shape_stays_dead_in_rust() {
        let pattern = r#"<pattern>
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
            </pattern>"#;
        let set = compile_inline(&wrap_pattern("PROBE_OU", pattern)).expect("compiles");
        let engine = crate::fr::example_suite::build_pattern_engine();
        let pipeline = crate::fr::test_support::FrenchPipeline::shared();
        og_rules::pattern_rule::set_test_date_hack(true);
        let text = "Je vous écrirai ou vous téléphonerai la semaine prochaine.";
        let mut matches = 0usize;
        for (sentence, _) in pipeline.analyze_offset_text(text) {
            for rule in &set.rules {
                matches += engine.match_rule(rule, &sentence).len();
            }
        }
        assert_eq!(
            matches, 0,
            "this is the grammar.xml:30276-30294 OU antipattern body: in Java it aligns \
             Je(ignored R suj)/vous(ignored R obj)/écrirai(V,unified 1 s)/ou(ignored)/\
             vous(ignored R suj)/téléphonerai(V,unified 1 s) and person=1+number=s unify, \
             suppressing the OU sub-rule that false-positives in Rust - the pin flips when \
             og-rules evaluates unify + skip=-1 + unify-ignore inside antipatterns"
        );
    }

    #[test]
    fn agreement_particular_suppressor_shape_fails_to_compile() {
        let pattern = r#"<pattern>
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
            </pattern>"#;
        let result = compile_inline(&wrap_pattern("PROBE_AGREEMENT", pattern));
        assert!(
            result.is_err(),
            "this is the grammar.xml:10022-10032 AGREEMENT_PARTICULAR antipattern body: in \
             Java it aligns niveau(N)/d'(P)/énergie(N f s)/croissant(and V ppr + J m s)/'.' \
             and suppresses the sub-rule that false-positives in Rust; the Rust og-xml parser \
             currently rejects the shape with IllFormed(MismatchedEndTag) and a neighbor shape \
             flipped across runs - the pin flips when the parser compiles optionals + and-group \
             + text-bearing token deterministically"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("MismatchedEndTag"),
            "expected the MismatchedEndTag signature, got: {err}"
        );
    }

    #[test]
    fn adj_adj_matchno0_suppressor_shape_now_fires_like_java() {
        let pattern = r#"<pattern>
                <token postag="J m s"/>
                <token regexp="yes">et|ni|ou</token>
                <token><match no="0"/>e</token>
            </pattern>"#;
        let set = compile_inline(&wrap_pattern("PROBE_MATCHNO0", pattern))
            .expect("the match-no-0 shape compiles");
        let engine = crate::fr::example_suite::build_pattern_engine();
        let pipeline = crate::fr::test_support::FrenchPipeline::shared();
        og_rules::pattern_rule::set_test_date_hack(true);
        let text = "Quand Ddaykha étudiait à l'université, elle n'avait ni ami ni amie.";
        let mut matches = 0usize;
        for (sentence, _) in pipeline.analyze_offset_text(text) {
            for rule in &set.rules {
                matches += engine.match_rule(rule, &sentence).len();
            }
        }
        assert_eq!(
            matches, 1,
            "this is the grammar.xml:13897-13901 ADJ_ADJ_SENT_END rulegroup antipattern: in \
             Java <match no=\"0\"/> interpolates the referenced token text so 'ami' + 'ni' + \
             'amie' (= <match no=\"0\"/>e) suppresses the sub-rule - the in-token <match> \
             references port (191a205) closed the gap this pin used to document"
        );
    }

    #[test]
    fn ou_differential_second_vous_keeps_suj_reading_in_rust() {
        let sentence = &crate::fr::test_support::analyze_french(
            "Je vous écrirai ou vous téléphonerai la semaine prochaine.",
        )[0];
        let tokens = sentence.non_whitespace_tokens();
        let vous = tokens
            .iter()
            .find(|t| t.token().token() == "vous" && t.token().start() == 20)
            .expect("second 'vous'");
        assert!(
            vous.has_pos_tag("R pers suj 2 p") && vous.has_pos_tag("R pers obj 2 p"),
            "the OU owning rule needs the suj reading here; Rust keeps suj+obj after \
             disambiguation, so the FP is either a Java disambiguator rule the Rust chain \
             misses or a combined postag+regexp+(?-i) element divergence"
        );
    }

    #[test]
    fn poursoi_sub30_matches_with_dp_follower_but_not_sentence_end() {
        let rule = sub_rule("MOTS_COLLES_OU_SEPARES", "30");
        assert_eq!(
            rule_match_spans(&rule, "Il le fait poursoi le chemin."),
            vec![(11, 18)],
            "sub-rule 30 (grammar.xml:47526-47536) is <token case_sensitive=\"yes\">poursoi</token> \
             + <token postag=\"[DP].*\" postag_regexp=\"yes\"/>: with 'le' (D f s) following, the \
             case-sensitive text match, the scope=previous 's'' exception and the postag element \
             all work, and 'poursoi' itself is a dict word (poursoi/N m s) in both engines - the \
             round-21 FrenchTagger.java:100 null-tagging theory is retired (that path fires only \
             for non-dict words, pinned by FrenchTaggerTest.java:54)"
        );
        assert!(
            rule_match_spans(&rule, "Il le fait poursoi.").is_empty(),
            "the sole blocker is the sentence-final '.': its readings are M fin,M fin,SENT_END \
             (dict + JLanguageTool.java:1842-1850) and PatternToken.java:352-362 rejects them for \
             the exact [DP].* postag exactly like the Rust engine does - by static reading Java \
             cannot match this example either; the pin flips only if the shared matcher gains \
             sentence-end semantics that satisfy a required next-token postag"
        );
    }

    #[test]
    fn adj_adj_sub5_nuance_lilas_fp_carries_only_dead_antipatterns() {
        let rule = sub_rule("ADJ_ADJ_SENT_END", "5");
        assert_eq!(
            rule.antipatterns.len(),
            66,
            "58 rulegroup-shared antipatterns (grammar.xml:13699-14181) must be attached to \
             sub-rule 5 ahead of its 8 local ones (parser.rs:883-887 group antipattern \
             inheritance, Java AbstractPatternRule.setAntiPatterns)"
        );
        assert_eq!(
            rule_match_spans(
                &rule,
                "Ses grandes fleurs doubles sont roses à nuance lilas et très parfumées."
            ),
            vec![(48, 73)],
            "the fp is exactly sub-rule 5 (grammar.xml:14621) aligning [J m sp]lilas / et / \
             [A]très / [J f p]parfumées / '.' - readings identical in both engines, roses being \
             J e p in both after RP-ETRE_ADJ_AMBIG (disambiguation.xml:8681-8690); every one of \
             the 66 antipatterns misses by static alignment (the near-misses die on 'très' \
             between 'et' and the final J, or on the sentence-final '.' where a trailing \
             P/punct/match-interpolation is required) - the pin flips when the Java-side \
             suppressor named by a JVM trace is implemented in the shared engine"
        );
    }

    #[test]
    fn il_devient_quoi_readings_are_all_present() {
        let sentence = &crate::fr::test_support::analyze_french("Il devient de plus en plus.")[0];
        let tokens = sentence.non_whitespace_tokens();
        let devient = tokens
            .iter()
            .find(|t| t.token().token() == "devient")
            .expect("devient");
        assert!(devient.has_pos_tag("V ind pres 3 s"));
        for surface in ["de", "plus", "en", "plus"] {
            let token = tokens
                .iter()
                .find(|t| t.token().token() == surface)
                .unwrap_or_else(|| panic!("{surface} token"));
            assert!(
                token.has_pos_tag("A") || token.has_pos_tag(" A"),
                "{surface} must carry the chunked 'de plus en plus' adverb reading"
            );
        }
        let end = tokens
            .iter()
            .find(|t| t.token().token() == ".")
            .expect("final dot");
        assert!(end.has_pos_tag("SENT_END"));
    }

    #[test]
    fn every_residue_rule_exists_and_example_text_is_real() {
        let dir = fr_rules_dir().expect("fr rules dir");
        for file in ["grammar.xml", "style.xml"] {
            std::fs::read_to_string(dir.join(file)).expect(file);
        }
        for (rule_id, text, _) in GRAMMAR_RESIDUES.iter().chain(STYLE_RESIDUES.iter()) {
            let xml = rule_file(rule_id).unwrap_or_else(|| panic!("rule {rule_id} not found"));
            let flat = xml.replace("<marker>", "").replace("</marker>", "");
            let mut end = text.len().min(24);
            while !text.is_char_boundary(end) {
                end += 1;
            }
            let clean_needle = &text[..end];
            assert!(
                flat.contains(clean_needle),
                "example fragment {clean_needle:?} of {rule_id} not found in rules xml"
            );
        }
    }

    #[test]
    fn residue_rules_are_all_present_in_the_compiled_sets() {
        let dir = fr_rules_dir().expect("fr rules dir");
        let mut ids: Vec<String> = Vec::new();
        for file in ["grammar.xml", "style.xml"] {
            let raw = std::fs::read_to_string(dir.join(file)).expect(file);
            let xml = crate::fr::normalize_fr_rules_xml(&raw);
            let set = og_xml::compiler::XmlCompiler::new()
                .compile_file_with_base(&xml, Some(&dir))
                .expect(file);
            ids.extend(set.rules.iter().map(|r| r.id.clone()));
        }
        for (rule_id, _, _) in GRAMMAR_RESIDUES.iter().chain(STYLE_RESIDUES.iter()) {
            assert!(
                ids.iter().any(|id| id == rule_id),
                "residue rule {rule_id} missing from compiled sets"
            );
        }
        assert!(compiled_rule_count() > 0);
    }

    #[test]
    fn java_empty_flag_group_is_rejected_by_the_rust_regex_engine() {
        let dangling = std::str::from_utf8(b"(?-)[A-Z]").unwrap();
        assert!(regex::Regex::new(dangling).is_err());
        assert!(regex::Regex::new("(?-i)[A-Z]").is_ok());
    }

    #[test]
    fn normalize_fr_rules_xml_strips_the_java_noop_flag_group() {
        let xml = rule_file("LA_LA2").expect("LA_LA2 in grammar.xml");
        assert!(xml.contains("(?-i)[A-Z].*|(?-)[A-Z]"));
        let normalized = crate::fr::normalize_fr_rules_xml(&xml);
        let normalized = normalized.as_ref();
        assert!(!normalized.contains("(?-)[A-Z]"));
        assert!(
            normalized.contains("(?-i)[A-Z].*|[A-Z]"),
            "the (?-i) inline flag must survive while the empty (?-) group is dropped"
        );
    }

    #[test]
    fn normalize_fr_rules_xml_joins_multiline_token_alternatives() {
        let xml =
            std::fs::read_to_string(fr_rules_dir().expect("fr rules dir").join("grammar.xml"))
                .expect("grammar.xml");
        assert!(xml.contains("frais\n"));
        let normalized = crate::fr::normalize_fr_rules_xml(&xml);
        let normalized = normalized.as_ref();
        assert!(
            normalized.contains("frais|"),
            "the FAIRE_PLUS_NOM_INCORRECT exception alternative 'frais\\n|froid' must be joined"
        );
    }

    #[test]
    fn apos_incorrect_exception_class_covers_all_four_apostrophe_variants() {
        let xml = rule_file("APOS_INCORRECT").expect("APOS_INCORRECT in grammar.xml");
        assert!(xml.contains("[´‘′`]"));
        let class = regex::Regex::new("[´‘′`]").expect("literal class compiles");
        for variant in ['´', '‘', '′', '`'] {
            assert!(class.is_match(&variant.to_string()), "{variant:?} in class");
        }
        let folded = regex::Regex::new("(?i)[´‘′`]").expect("ci class compiles");
        for variant in ['´', '‘', '′', '`'] {
            assert!(
                folded.is_match(&variant.to_string()),
                "case-insensitive class must still match {variant:?}"
            );
        }
        assert_eq!(
            og_xml::unifier::normalize_apostrophes("‘"),
            "'",
            "the engine rewrites U+2018 on the token side while the pattern class keeps it"
        );
    }

    #[test]
    fn faire_plus_exception_regexp_still_matches_its_listed_words() {
        let dir = fr_rules_dir().expect("fr rules dir");
        let raw = std::fs::read_to_string(dir.join("grammar.xml")).expect("grammar.xml");
        let xml = crate::fr::normalize_fr_rules_xml(&raw);
        let set = og_xml::compiler::XmlCompiler::new()
            .compile_file_with_base(&xml, Some(&dir))
            .expect("compile grammar.xml");
        let rule = set
            .rules
            .iter()
            .find(|r| r.id == "FAIRE_PLUS_NOM_INCORRECT")
            .expect("FAIRE_PLUS_NOM_INCORRECT");
        let exception = rule
            .pattern
            .tokens
            .iter()
            .flat_map(|t| t.exceptions.iter())
            .find(|e| e.text.as_deref().is_some_and(|t| t.contains("abstraction")))
            .expect("word-list exception");
        assert!(
            exception.compiled_regexp.is_some() || exception.compiled_regexp_fancy.is_some(),
            "giant word-list exception regexp failed to compile in both engines"
        );
        let text = exception.text.as_deref().expect("exception text");
        assert!(
            text.contains("frais|"),
            "alternatives must be joined: {text:?}"
        );
        assert!(
            !text.contains(['\n', '\t', '\r']),
            "token text must be whitespace-normalized"
        );
        let anchored = regex::Regex::new(&format!("^(?:{text})$")).expect("recompile exception");
        assert!(anchored.is_match("frais"));
    }

    #[test]
    #[ignore]
    fn trace_faire_plus_exception_compile_paths() {
        let _release_guard = crate::release_guard();
        let dir = fr_rules_dir().expect("fr rules dir");
        let xml = std::fs::read_to_string(dir.join("grammar.xml")).expect("grammar.xml");
        let start = xml.find("abstraction").expect("word list");
        let end = xml[start..].find("</exception>").expect("exception end");
        let list = &xml[start..start + end];
        let trimmed = list.trim();
        let anchored = format!("^(?:{trimmed})$");
        let plain = regex::Regex::new(&anchored);
        eprintln!(
            "plain compile: {}",
            plain.as_ref().is_ok_and(|re| re.is_match("frais"))
        );
        let folded = og_xml::java_case_fold::compile_java_ci(&anchored);
        eprintln!(
            "java-ci compile: {}",
            folded.as_ref().is_some_and(|re| re.is_match("frais"))
        );
        let ci = regex::Regex::new(&format!("(?i){anchored}"));
        eprintln!(
            "(?i) compile: {}",
            ci.as_ref().is_ok_and(|re| re.is_match("frais"))
        );
        if let Some(re) = folded {
            let hits: Vec<bool> = ["frais", "abstraction", "confiance", "usage", "violence"]
                .iter()
                .map(|w| re.is_match(w))
                .collect();
            eprintln!("java-ci per-word: {hits:?}");
        }
        eprintln!("list head: {:?}", &list[..80.min(list.len())]);
    }
}
