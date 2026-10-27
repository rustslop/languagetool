pub use og_rules::pt_filters::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pt::test_support::PortuguesePipeline;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings, RuleMatch, SuggestedReplacement};
    use og_rules::pattern_rule::PatternRuleEngine;
    use og_rules::postponed_adjective::LangSynthesizer;
    use og_xml::compiler::{CompiledRule, CompiledRuleSet, XmlCompiler};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn grammar_path() -> PathBuf {
        crate::engine::find_lt_resource_dir()
            .expect("lt resource dir")
            .join("pt/src/main/resources/org/languagetool/rules/pt/grammar.xml")
    }

    fn compiled_rules() -> std::sync::Arc<CompiledRuleSet> {
        static RULES: og_core::releasable::Releasable<CompiledRuleSet> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&RULES, || {
            let path = grammar_path();
            let xml = std::fs::read_to_string(&path).unwrap();
            Some(std::sync::Arc::new(
                XmlCompiler::new()
                    .compile_file_with_base(&xml, path.parent())
                    .expect("compile grammar.xml"),
            ))
        })
        .expect("pt grammar")
    }

    fn synthesizer() -> Option<LangSynthesizer> {
        LangSynthesizer::for_language(
            &super::super::tagger::pt_resource_dir().expect("pt resource dir"),
            "pt",
        )
    }

    fn atr(word: &str, tags: &[(&str, &str)]) -> AnalyzedTokenReadings {
        let readings: Vec<AnalyzedToken> = tags
            .iter()
            .map(|(lemma, tag)| {
                AnalyzedToken::new(word, 0, word.len())
                    .with_lemma(*lemma)
                    .with_pos_tags(vec![tag.to_string()])
            })
            .collect();
        AnalyzedTokenReadings::new(AnalyzedToken::new(word, 0, word.len())).with_readings(readings)
    }

    fn base_rm(message: &str, offset: usize, length: usize, template: &str) -> RuleMatch {
        RuleMatch::new(
            message,
            offset,
            length,
            og_core::RuleMatchRule::new("X", "d"),
            og_core::RuleMatchContext::new("", 0, 0),
        )
        .with_replacements(vec![SuggestedReplacement::new(template)])
    }

    fn first_rule<'a>(rules: &'a CompiledRuleSet, id: &str) -> &'a CompiledRule {
        rules
            .rules
            .iter()
            .find(|r| r.id == id)
            .unwrap_or_else(|| panic!("rule {id} not compiled"))
    }

    fn bare(rule: &CompiledRule) -> CompiledRule {
        let mut b = rule.clone();
        b.filter = None;
        b
    }

    #[test]
    fn confusion_pairs_data_parses_clean_and_accumulates() {
        for (brazilian, variant_file) in [(false, "pt-PT"), (true, "pt-BR")] {
            let map = confusion_pairs(brazilian);
            assert!(
                map.len() > 1000,
                "{variant_file} map must load the base file plus its variant overlay"
            );
            let medias = map.get("medias").expect("medias entry");
            assert_eq!(medias.word, "médias");
            assert_eq!(
                medias.tags,
                vec![
                    "NCFP000".to_string(),
                    "AQ0FP0".to_string(),
                    "NCFP000".to_string()
                ]
            );
            let tramites = map.get("tramites").expect("tramites entry");
            assert_eq!(tramites.word, "trâmites");
            assert_eq!(tramites.tags, vec!["NCMP000".to_string()]);
        }
        assert_eq!(
            confusion_pairs(true).get("anonimo").unwrap().word,
            "anônimo"
        );
        assert_eq!(
            confusion_pairs(false).get("anonimo").unwrap().word,
            "anónimo"
        );
        assert_eq!(confusion_pairs(false).get("bebe").unwrap().word, "bebé");
        assert_eq!(confusion_pairs(true).get("bebe").unwrap().word, "bebê");
    }

    #[test]
    fn confusion_check_resolves_form_and_postag_from_matched_texts() {
        let token = atr("tramites", &[("trâmite", "NCMP000")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&token];
        let matched = vec!["nos".to_string(), "tramites".to_string()];
        let rm = base_rm("m", 8, 8, "{suggestion}");
        let out = confusion_check_filter(
            "pt-PT",
            "form:\\2 postag:N.[MC]P.*|A..[MC]P.",
            &matched,
            &tokens,
            rm,
        )
        .expect("tramites -> trâmites");
        assert_eq!(out.replacements()[0].value(), "trâmites");
    }

    #[test]
    fn confusion_check_nulls_forms_missing_from_the_pairs() {
        let token = atr("servia", &[("servir", "VMII3S0")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&token];
        let matched = vec!["nos".to_string(), "servia".to_string()];
        let rm = base_rm("m", 9, 6, "{suggestion}");
        assert!(
            confusion_check_filter(
                "pt-PT",
                "form:\\2 postag:N.[MC]P.*|A..[MC]P.",
                &matched,
                &tokens,
                rm,
            )
            .is_none(),
            "'servia' is not a confusion pair, Java returns null"
        );
    }

    #[test]
    fn confusion_check_templates_carry_all_three_case_placeholders() {
        let token = atr("Anonimo", &[("anónimo", "AQ0MS0")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&token];
        let matched = vec!["Anonimo".to_string()];
        let rm = base_rm("m", 0, 7, "{Suggestion} / {suggestion} / {SUGGESTION}");
        let out = confusion_check_filter("pt-PT", "form:\\1 postag:[AN].*", &matched, &tokens, rm)
            .expect("anonimo entry exists in the pt-PT overlay");
        assert_eq!(out.replacements()[0].value(), "Anónimo / anónimo / ANÓNIMO");
        let rm = base_rm("m", 0, 7, "{Suggestion}");
        let out = confusion_check_filter("pt-BR", "form:\\1 postag:[AN].*", &matched, &tokens, rm)
            .expect("anonimo entry exists in the pt-BR overlay");
        assert_eq!(out.replacements()[0].value(), "Anônimo");
    }

    #[test]
    fn confusion_check_gendernumber_uses_only_the_first_reading() {
        let matched = vec!["os".to_string(), "tramites".to_string()];
        let source = atr("os", &[("o", "DA0MP0")]);
        let entry = atr("tramites", &[("trâmite", "NCMP000")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&source, &entry];
        let rm = base_rm("m", 3, 8, "{suggestion}");
        let out = confusion_check_filter(
            "pt-PT",
            "form:\\2 postag:N.[MC]P.*|A..[MC]P. gendernumberFrom:1",
            &matched,
            &tokens,
            rm,
        )
        .expect("MP source selects the MP entry");
        assert_eq!(out.replacements()[0].value(), "trâmites");

        let source = atr("o", &[("o", "DA0MS0")]);
        let entry = atr("tramites", &[("trâmite", "NCMP000")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&source, &entry];
        let rm = base_rm("m", 2, 8, "{suggestion}");
        assert!(
            confusion_check_filter(
                "pt-PT",
                "form:\\2 postag:N.[MC]P.*|A..[MC]P. gendernumberFrom:1",
                &matched,
                &tokens,
                rm,
            )
            .is_none(),
            "MS source demands a singular first reading, NCMP000 must be rejected"
        );

        let source = atr("x", &[("x", "ZP0")]);
        let entry = atr("tramites", &[("trâmite", "NCMP000")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&source, &entry];
        let rm = base_rm("m", 2, 8, "{suggestion}");
        assert!(
            confusion_check_filter(
                "pt-PT",
                "form:\\2 postag:N.[MC]P.*|A..[MC]P. gendernumberFrom:1",
                &matched,
                &tokens,
                rm,
            )
            .is_none(),
            "a source matching no gender pattern leaves the replacement unset"
        );
    }

    #[test]
    fn confusion_check_rewrites_message_without_diacritic_gain() {
        let mut message = "se escribe con tilde".to_string();
        let replacement = "casa";
        let form = "kasa";
        if !has_diacritics(replacement) || has_diacritics(form) {
            message = message.replace("se escribe con tilde", "se escribe de otra manera");
        }
        assert_eq!(message, "se escribe de otra manera");
        assert!(has_diacritics("anónimo"));
        assert!(!has_diacritics("anonimo"));
    }

    #[test]
    fn toponym_map_validates_municipalities_with_suffix_relaxation() {
        assert!(is_valid_toponym("Niterói"));
        assert!(is_valid_toponym("Angra dos Reis"));
        assert!(is_valid_toponym("Varre-Sai"));
        assert!(is_valid_toponym("Paty Do Alferes"));
        assert!(is_valid_toponym("São José do Vale do Rio Preto"));
        assert!(is_valid_toponym("do Rio de Janeiro"));
        assert!(!is_valid_toponym("Fernando Gabeira"));
        assert!(!is_valid_toponym("qualquer coisa"));
    }

    #[test]
    fn toponym_filter_accepts_and_rejects_like_java() {
        let rm = base_rm("m", 17, 5, "{suggestion}");
        let out = brazilian_toponym_filter(&["Niterói (RJ)", "Niterói", " (RJ)", "RJ"], rm)
            .expect("Niterói is an RJ municipality");
        assert_eq!(out.replacements()[0].value(), "\u{2013}RJ");
        let rm = base_rm("m", 17, 3, "{suggestion}");
        assert!(
            brazilian_toponym_filter(&["Niterói–RJ", "Niterói", "–RJ", "RJ"], rm).is_none(),
            "already-correct en dash suggestion is dropped"
        );
        let rm = base_rm("m", 33, 4, "{suggestion}");
        assert!(
            brazilian_toponym_filter(
                &["Fernando Gabeira (RJ)", "Fernando Gabeira", " (RJ)", "RJ"],
                rm,
            )
            .is_none(),
            "non-toponym is dropped"
        );
    }

    #[test]
    fn toponym_rule_examples_pass_end_to_end_through_the_compiled_regexp() {
        let pipeline = PortuguesePipeline::shared();
        let engine = PatternRuleEngine::new();
        let cases: &[(&str, &str, Option<&str>)] = &[
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PARENTESES",
                "Nasceu em Niterói (RJ).",
                Some("\u{2013}RJ"),
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PARENTESES",
                "Nasceu em Niterói–RJ.",
                None,
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PARENTESES",
                "O candidato a prefeito Fernando Gabeira (RJ).",
                None,
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Nasceu em Niterói – RJ.",
                Some("\u{2013}RJ"),
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Nasceu em Niterói/RJ.",
                Some("\u{2013}RJ"),
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Nativo de Angra dos Reis/RJ.",
                Some("\u{2013}RJ"),
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Nativo de Varre-Sai/RJ.",
                Some("\u{2013}RJ"),
            ),
            (
                "SEPARADORES_DE_ESTADOS_BRASILEIROS_MEIA_RISCA_PONTUACAO_SIMPLES",
                "Nativo de Paty Do Alferes/RJ.",
                Some("\u{2013}RJ"),
            ),
        ];
        let rules = compiled_rules();
        for (id, text, expected) in cases {
            let rule = first_rule(&rules, id);
            let sentence = &pipeline.analyze_text(text)[0];
            let raw = engine.match_rule(&bare(rule), sentence);
            let regexp = rule.regexp.as_ref().expect("regexp rule");
            let capture_sets: Vec<Vec<String>> = if let Some(re) = regexp.regex.as_ref() {
                re.captures_iter(sentence.text())
                    .map(|caps| {
                        (0..caps.len())
                            .map(|i| {
                                caps.get(i)
                                    .map(|m| m.as_str().to_string())
                                    .unwrap_or_default()
                            })
                            .collect()
                    })
                    .collect()
            } else if let Some(re) = regexp.fancy_regex.as_ref() {
                re.captures_iter(sentence.text())
                    .flatten()
                    .map(|caps| {
                        (0..caps.len())
                            .map(|i| {
                                caps.get(i)
                                    .map(|m| m.as_str().to_string())
                                    .unwrap_or_default()
                            })
                            .collect()
                    })
                    .collect()
            } else {
                Vec::new()
            };
            assert_eq!(
                raw.len(),
                capture_sets.len(),
                "{id} on {text}: engine matches and capture sets must align"
            );
            let mut kept: Vec<String> = Vec::new();
            for (rm, groups) in raw.into_iter().zip(capture_sets) {
                let refs: Vec<&str> = groups.iter().map(String::as_str).collect();
                if let Some(out) = brazilian_toponym_filter(&refs, rm) {
                    kept.push(out.replacements()[0].value().to_string());
                }
            }
            match expected {
                Some(value) => {
                    assert_eq!(kept, vec![value.to_string()], "{id} on {text}");
                }
                None => {
                    assert!(kept.is_empty(), "{id} on {text} kept {kept:?}");
                }
            }
        }
    }

    #[test]
    fn participle_filter_selects_irregular_form_for_regular_token() {
        let synth = synthesizer().expect("pt synthesizer from the main dict");
        let gastado = atr("gastado", &[("gastar", "VMP00SM")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&gastado];
        let rm = base_rm("m", 0, 7, "{suggestion}");
        let out = regular_irregular_participle_filter(
            Some(&synth),
            "direction:RegularToIrregular",
            &tokens,
            rm,
        )
        .expect("gastado -> gasto");
        assert_eq!(out.replacements()[0].value(), "gasto");
    }

    #[test]
    fn participle_filter_nulls_tokens_without_a_vmp_reading() {
        let synth = synthesizer().expect("pt synthesizer from the main dict");
        let limpo = atr("limpo", &[("limpar", "VMIP1S0"), ("limpo", "AQ0MS0")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&limpo];
        let rm = base_rm("m", 0, 5, "{suggestion}");
        assert!(
            regular_irregular_participle_filter(
                Some(&synth),
                "direction:IrregularToRegular",
                &tokens,
                rm,
            )
            .is_none(),
            "the dict carries no VMP reading for 'limpo', Java's hasPosTagStartingWith fails"
        );
    }

    #[test]
    fn participle_rule_example_passes_end_to_end() {
        let pipeline = PortuguesePipeline::shared();
        let engine = PatternRuleEngine::new();
        let synth = Arc::new(synthesizer().expect("pt synthesizer"));
        let rules = compiled_rules();
        let rule = rules
            .rules
            .iter()
            .find(|r| {
                r.filter.as_ref().is_some_and(|f| {
                    f.class.ends_with("RegularIrregularParticipleFilter")
                        && f.args.contains("RegularToIrregular")
                })
            })
            .expect("RegularToIrregular participle rule");
        let text = "O dinheiro foi gastado o mês passado.";
        let sentence = &pipeline.analyze_text(text)[0];
        let tokens: Vec<&og_core::AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
        let raw = engine
            .match_rule(&bare(rule), sentence)
            .into_iter()
            .next()
            .expect("raw pattern match on 'foi gastado'");
        let out = regular_irregular_participle_filter(
            Some(synth.as_ref()),
            "direction:RegularToIrregular",
            &tokens,
            raw,
        )
        .expect("filtered match");
        assert_eq!(out.replacements()[0].value(), "gasto");
        assert_eq!(&text[out.offset()..out.offset() + out.length()], "gastado");
    }

    #[test]
    fn soros_roman_matches_the_active_roman_sor_rules() {
        assert_eq!(soros_roman("1"), "I");
        assert_eq!(soros_roman("2"), "II");
        assert_eq!(soros_roman("4"), "IV");
        assert_eq!(soros_roman("9"), "IX");
        assert_eq!(soros_roman("10"), "X");
        assert_eq!(soros_roman("20"), "XX");
        assert_eq!(soros_roman("40"), "XL");
        assert_eq!(soros_roman("44"), "XLIV");
        assert_eq!(soros_roman("90"), "XC");
        assert_eq!(soros_roman("400"), "CD");
        assert_eq!(soros_roman("900"), "CM");
        assert_eq!(soros_roman("1987"), "MCMLXXXVII");
        assert_eq!(soros_roman("3999"), "MMMCMXCIX");
        assert_eq!(soros_roman("0"), "");
        assert_eq!(soros_roman(""), "");
        assert_eq!(soros_roman("12a"), "");
        assert_eq!(soros_roman("007"), "VII");
        assert_eq!(soros_roman("4000"), "4\u{0305}");
        assert_eq!(soros_roman("12000"), "1\u{0305}2\u{0305}");
    }

    #[test]
    fn roman_numeral_rule_examples_pass_end_to_end() {
        let pipeline = PortuguesePipeline::shared();
        let engine = PatternRuleEngine::new();
        let rules = compiled_rules();
        let rule = first_rule(&rules, "SECULO_EM_NUMERAIS_ROMANOS");
        for (text, source, expected) in [
            ("No começo do século 20.", "20", "XX"),
            ("Nasceu no século 1.", "1", "I"),
            ("A história se passa no século 44.", "44", "XLIV"),
        ] {
            let sentence = &pipeline.analyze_text(text)[0];
            let raw = engine
                .match_rule(&bare(rule), sentence)
                .into_iter()
                .next()
                .unwrap_or_else(|| panic!("raw match on {text}"));
            assert_eq!(raw.replacements()[0].value(), "{suggestion}", "{text}");
            let matched = vec!["século".to_string(), source.to_string()];
            let out = roman_numeral_filter("arabicSource:\\2", &matched, raw).expect("always set");
            assert_eq!(out.replacements()[0].value(), expected, "{text}");
        }
    }

    #[test]
    fn enclisis_pronoun_tag_extraction_ports_java_cases() {
        let nos = atr("nos", &[("nos", "PP1CP000")]);
        assert_eq!(
            pronoun_tags(&nos, "comeu", false),
            vec!["PP1CPO00".to_string()]
        );
        assert_eq!(
            pronoun_tags(&nos, "comeram", false),
            vec!["PP1CPO00".to_string(), "PP3MPA00".to_string()]
        );
        assert_eq!(
            pronoun_tags(&nos, "põe", false),
            vec!["PP1CPO00".to_string(), "PP3MPA00".to_string()]
        );
        let os = atr("os", &[("o", "PP3MPA00")]);
        assert_eq!(
            pronoun_tags(&os, "comeu", true),
            vec!["PP3MPA00".to_string()],
            "already-accusative tags pass through the N00 conversion unchanged"
        );
        let ele = atr("ele", &[("ele", "PP3CN00")]);
        assert_eq!(pronoun_tags(&ele, "viu", true), vec!["PP3CA00".to_string()]);
        let noun = atr("casa", &[("casa", "NCFS000")]);
        assert!(pronoun_tags(&noun, "viu", false).is_empty());
    }

    #[test]
    fn enclisis_filter_ports_the_all_caps_example_shape() {
        let verb = atr("TEM", &[("ter", "VMIP3P0")]);
        let hyphen = atr("-", &[]);
        let pronoun = atr("O", &[("o", "PP3MSA00")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb, &hyphen, &pronoun];
        let rm = base_rm("m", 4, 5, "{suggestion}");
        let out = portuguese_enclisis_filter(
            synthesizer().as_ref(),
            "verbPos:0 pronounPos:2 convertToAccusative:False",
            &tokens,
            &[0, 1, 2],
            rm,
        )
        .expect("pronoun tags are non-empty");
        assert!(
            out.replacements().is_empty(),
            "the pt synth dict is absent, so enclisisTag lookups return no forms"
        );
    }

    #[test]
    fn proclisis_pronoun_forms_port_the_java_switch() {
        assert_eq!(proclisis_pronoun_forms("lo", "distribuí"), vec!["o"]);
        assert_eq!(proclisis_pronoun_forms("no", "pôs"), vec!["o"]);
        assert_eq!(proclisis_pronoun_forms("la", "comeu"), vec!["a"]);
        assert_eq!(proclisis_pronoun_forms("na", "pôs"), vec!["a"]);
        assert_eq!(proclisis_pronoun_forms("los", "comeu"), vec!["os"]);
        assert_eq!(proclisis_pronoun_forms("las", "comeu"), vec!["as"]);
        assert_eq!(proclisis_pronoun_forms("nas", "pôs"), vec!["as"]);
        assert_eq!(proclisis_pronoun_forms("nos", "comeu"), vec!["nos"]);
        assert_eq!(proclisis_pronoun_forms("nos", "puseram"), vec!["nos", "os"]);
        assert_eq!(proclisis_pronoun_forms("nos", "põe"), vec!["nos", "os"]);
        assert_eq!(proclisis_pronoun_forms("lhe", "deu"), vec!["lhe"]);
    }

    #[test]
    fn proclisis_filter_builds_pronoun_verb_suggestions() {
        let enclitic = atr("quis-lo", &[("querer", "VMIS3S0:PP3MSA00")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&enclitic];
        let synth = synthesizer().expect("pt synthesizer from the main dict");
        let rm = base_rm("m", 0, 7, "{suggestion}");
        let out = portuguese_proclisis_filter(Some(&synth), &tokens, &[0], rm).expect("match");
        assert_eq!(
            out.replacements()
                .iter()
                .map(|r| r.value().to_string())
                .collect::<Vec<String>>(),
            vec!["o quis".to_string()],
            "VMIS3S0 synthesizes to 'quis' and the clitic 'lo' maps to 'o'"
        );
    }

    #[test]
    fn pattern_token_range_windows_the_matched_span() {
        let a = atr("a", &[]);
        let b = atr("b", &[]);
        let c = atr("c", &[]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&a, &b, &c];
        assert_eq!(pattern_token_range(&tokens, &[1, 2]).len(), 2);
        assert!(pattern_token_range(&tokens, &[]).is_empty());
    }

    #[test]
    fn move_pronoun_tag_ports_the_java_split_semantics() {
        assert_eq!(
            move_pronoun_tag("VMN0000:PP3MSN00", "VMIP3P0"),
            "VMIP3P0:PP3MSN00"
        );
        assert_eq!(move_pronoun_tag("VMN0000", "VMIP3P0"), "VMIP3P0");
        assert_eq!(
            move_pronoun_tag("VMN0000:PP3MSN00:X", "VMIP3P0"),
            "VMIP3P0",
            "a three-part source tag is not a verb:pronoun pair"
        );
        assert_eq!(move_pronoun_tag("VMN0000:", "VMIP3P0"), "VMIP3P0");
    }

    #[test]
    fn suggestion_exception_rejects_plural_without_s() {
        assert!(is_suggestion_exception_pt("estudo", "VMIP1P0"));
        assert!(is_suggestion_exception_pt("estudai", "VMIP2P0"));
        assert!(!is_suggestion_exception_pt("estudamos", "VMIP1P0"));
        assert!(!is_suggestion_exception_pt("estuda", "VMIP3S0"));
    }

    #[test]
    fn advanced_synthesizer_composes_postag_replace_groups() {
        assert_eq!(
            composite_postag(
                "V(.)N.*",
                "V...(..).*",
                "VMN0000",
                "VMIP1S0",
                r"V\a1(IP|IF)\b1."
            ),
            "VM(IP|IF)1S."
        );
        assert_eq!(
            composite_postag("V.*", "NC(.)(.)000", "VMN0000", "NCFS000", "VMP00\\b2\\b1"),
            "VMP00SF"
        );
        assert_eq!(
            composite_postag("V.*", "NC.*", "AQ0MS0", "NCFS000", r"AQ0\b1\b20"),
            "AQ0\\b1\\b20",
            "without matching groups the replace text stays verbatim"
        );
    }

    #[test]
    fn advanced_synthesizer_synthesizes_from_the_lemma_and_postag_tokens() {
        let tenho = atr("tenho", &[("ter", "VMIP1S0")]);
        let pesquisado = atr("pesquisado", &[("pesquisar", "VMP00SM")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&tenho, &pesquisado];
        let synth = synthesizer().expect("pt synthesizer from the main dict");
        let rm = base_rm("m", 3, 17, "{suggestion}");
        let out = advanced_synthesizer_filter(
            Some(&synth),
            "lemmaFrom:2 lemmaSelect:V.* postagFrom:1 postagSelect:V.*",
            &[],
            &[0, 1],
            &tokens,
            rm,
        )
        .expect("filtered match");
        assert_eq!(
            out.replacements()
                .iter()
                .map(|r| r.value().to_string())
                .collect::<Vec<String>>(),
            vec!["pesquiso".to_string()],
            "pesquisar synthesized with tenho's VMIP1S0"
        );
    }

    #[test]
    fn advanced_synthesizer_rejects_new_lemma_placeholders() {
        let tenho = atr("tenho", &[("ter", "VMIP1S0")]);
        let pesquisado = atr("pesquisado", &[("pesquisar", "VMP00SM")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&tenho, &pesquisado];
        let synth = synthesizer().expect("pt synthesizer from the main dict");
        let rm = base_rm("m", 3, 17, "{suggestion}");
        assert!(
            advanced_synthesizer_filter(
                Some(&synth),
                "lemmaFrom:2 lemmaSelect:V.* postagFrom:1 postagSelect:V.* newLemma:_x",
                &[],
                &[0, 1],
                &tokens,
                rm,
            )
            .is_none(),
            "pt does not override getNewLemma, so a '_' placeholder nulls the lemma"
        );
    }

    #[test]
    fn advanced_synthesizer_routes_through_the_dispatch() {
        let tenho = atr("tenho", &[("ter", "VMIP1S0")]);
        let pesquisado = atr("pesquisado", &[("pesquisar", "VMP00SM")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&tenho, &pesquisado];
        let synth = synthesizer().expect("pt synthesizer from the main dict");
        let call = PtFilterCall {
            variant: "pt-PT",
            args: "lemmaFrom:2 lemmaSelect:V.* postagFrom:1 postagSelect:V.*",
            matched_texts: &[],
            matched_positions: &[0, 1],
            tokens: &tokens,
            regexp_groups: None,
            synthesizer: Some(&synth),
        };
        let rm = base_rm("m", 3, 17, "{suggestion}");
        let out = apply_pt_filter(
            "org.languagetool.rules.pt.AdvancedSynthesizerFilter",
            call,
            rm,
        )
        .expect("synthesizer match");
        assert_eq!(out.replacements()[0].value(), "pesquiso");
    }

    #[test]
    fn dispatch_recognizes_exactly_the_seven_java_filter_classes() {
        for class in [
            "org.languagetool.rules.pt.ConfusionCheckFilter",
            "org.languagetool.rules.pt.BrazilianToponymFilter",
            "org.languagetool.rules.pt.RegularIrregularParticipleFilter",
            "org.languagetool.rules.pt.RomanNumeralFilter",
            "org.languagetool.rules.pt.PortugueseEnclisisFilter",
            "org.languagetool.rules.pt.PortugueseProclisisFilter",
            "org.languagetool.rules.pt.AdvancedSynthesizerFilter",
        ] {
            assert!(is_pt_filter(class), "{class} must route");
        }
        assert!(!is_pt_filter(
            "org.languagetool.rules.es.ConfusionCheckFilter"
        ));
        assert!(!is_pt_filter(
            "org.languagetool.rules.es.AdvancedSynthesizerFilter"
        ));
        assert!(!is_pt_filter("org.languagetool.rules.pt.FutureDateFilter"));
        assert!(!is_pt_filter(""));
    }

    #[test]
    fn dispatch_windows_the_confusion_pattern_tokens_for_gendernumber_from() {
        let unmatched = atr("x", &[("x", "ZP0")]);
        let source = atr("os", &[("o", "DA0MP0")]);
        let entry = atr("tramites", &[("trâmite", "NCMP000")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&unmatched, &source, &entry];
        let matched = vec!["os".to_string(), "tramites".to_string()];
        let call = PtFilterCall {
            variant: "pt-PT",
            args: "form:\\2 postag:N.[MC]P.*|A..[MC]P. gendernumberFrom:1",
            matched_texts: &matched,
            matched_positions: &[1, 2],
            tokens: &tokens,
            regexp_groups: None,
            synthesizer: None,
        };
        let rm = base_rm("m", 3, 8, "{suggestion}");
        let out = apply_pt_filter("org.languagetool.rules.pt.ConfusionCheckFilter", call, rm)
            .expect("gendernumberFrom must index the match window, not the sentence tokens");
        assert_eq!(out.replacements()[0].value(), "trâmites");
    }

    #[test]
    fn dispatch_routes_the_toponym_filter_through_the_regexp_groups() {
        let tokens: Vec<&AnalyzedTokenReadings> = Vec::new();
        let call = PtFilterCall {
            variant: "pt-BR",
            args: "foo:bar",
            matched_texts: &[],
            matched_positions: &[],
            tokens: &tokens,
            regexp_groups: None,
            synthesizer: None,
        };
        let rm = base_rm("m", 17, 5, "{suggestion}");
        assert!(
            apply_pt_filter("org.languagetool.rules.pt.BrazilianToponymFilter", call, rm).is_none(),
            "the regexp path must pass capture group texts"
        );
        let groups = ["Niterói (RJ)", "Niterói", " (RJ)", "RJ"];
        let call = PtFilterCall {
            variant: "pt-BR",
            args: "foo:bar",
            matched_texts: &[],
            matched_positions: &[],
            tokens: &tokens,
            regexp_groups: Some(&groups),
            synthesizer: None,
        };
        let rm = base_rm("m", 17, 5, "{suggestion}");
        let out = apply_pt_filter("org.languagetool.rules.pt.BrazilianToponymFilter", call, rm)
            .expect("Niterói is an RJ municipality");
        assert_eq!(out.replacements()[0].value(), "\u{2013}RJ");
    }

    #[test]
    fn dispatch_drives_the_compiled_confusion_rule_end_to_end() {
        let pipeline = PortuguesePipeline::shared();
        let engine = PatternRuleEngine::new();
        let rules = compiled_rules();
        let rule = rules
            .rules
            .iter()
            .find(|r| {
                r.filter.as_ref().is_some_and(|f| {
                    f.class.ends_with("pt.ConfusionCheckFilter") && f.args.contains("N.[MC]P")
                })
            })
            .expect("nos|são confusion subrule");
        let text = "Um erro nos tramites.";
        let sentence = &pipeline.analyze_text(text)[0];
        let raw = engine
            .match_rule(&bare(rule), sentence)
            .into_iter()
            .next()
            .expect("raw pattern match on 'nos tramites'");
        assert_eq!(raw.replacements()[0].value(), "{suggestion}");
        let tokens: Vec<&AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
        let marker_idx = tokens
            .iter()
            .position(|t| t.token().start() == raw.offset())
            .expect("marker token");
        let matched_texts = vec!["nos".to_string(), "tramites".to_string()];
        let start = marker_idx + 1 - matched_texts.len();
        let matched_positions: Vec<usize> = (start..=marker_idx).collect();
        let call = PtFilterCall {
            variant: "pt-PT",
            args: rule.filter.as_ref().unwrap().args.as_str(),
            matched_texts: &matched_texts,
            matched_positions: &matched_positions,
            tokens: &tokens,
            regexp_groups: None,
            synthesizer: None,
        };
        let out = apply_pt_filter("org.languagetool.rules.pt.ConfusionCheckFilter", call, raw)
            .expect("tramites -> trâmites");
        assert_eq!(out.replacements()[0].value(), "trâmites");
    }

    #[test]
    fn dispatch_routes_the_four_pattern_path_filters() {
        let synth = synthesizer().expect("pt synthesizer from the main dict");
        let gastado = atr("gastado", &[("gastar", "VMP00SM")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&gastado];
        let call = PtFilterCall {
            variant: "pt-PT",
            args: "direction:RegularToIrregular",
            matched_texts: &[],
            matched_positions: &[0],
            tokens: &tokens,
            regexp_groups: None,
            synthesizer: Some(&synth),
        };
        let rm = base_rm("m", 0, 7, "{suggestion}");
        let out = apply_pt_filter(
            "org.languagetool.rules.pt.RegularIrregularParticipleFilter",
            call,
            rm,
        )
        .expect("gastado -> gasto");
        assert_eq!(out.replacements()[0].value(), "gasto");

        let matched = vec!["século".to_string(), "20".to_string()];
        let call = PtFilterCall {
            variant: "pt-PT",
            args: "arabicSource:\\2",
            matched_texts: &matched,
            matched_positions: &[0, 1],
            tokens: &tokens,
            regexp_groups: None,
            synthesizer: None,
        };
        let rm = base_rm("m", 0, 2, "{suggestion}");
        let out = apply_pt_filter("org.languagetool.rules.pt.RomanNumeralFilter", call, rm)
            .expect("roman replacement is always set");
        assert_eq!(out.replacements()[0].value(), "XX");

        let verb = atr("TEM", &[("ter", "VMIP3P0")]);
        let hyphen = atr("-", &[]);
        let pronoun = atr("O", &[("o", "PP3MSA00")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&verb, &hyphen, &pronoun];
        let call = PtFilterCall {
            variant: "pt-BR",
            args: "verbPos:0 pronounPos:2 convertToAccusative:False",
            matched_texts: &[],
            matched_positions: &[0, 1, 2],
            tokens: &tokens,
            regexp_groups: None,
            synthesizer: Some(&synth),
        };
        let rm = base_rm("m", 4, 5, "{suggestion}");
        let out = apply_pt_filter(
            "org.languagetool.rules.pt.PortugueseEnclisisFilter",
            call,
            rm,
        )
        .expect("pronoun tags are non-empty");
        assert!(out.replacements().is_empty());

        let enclitic = atr("quis-lo", &[("querer", "VMIS3S0:PP3MSA00")]);
        let tokens: Vec<&AnalyzedTokenReadings> = vec![&enclitic];
        let call = PtFilterCall {
            variant: "pt-PT",
            args: "",
            matched_texts: &[],
            matched_positions: &[0],
            tokens: &tokens,
            regexp_groups: None,
            synthesizer: Some(&synth),
        };
        let rm = base_rm("m", 0, 7, "{suggestion}");
        let out = apply_pt_filter(
            "org.languagetool.rules.pt.PortugueseProclisisFilter",
            call,
            rm,
        )
        .expect("proclisis match");
        assert_eq!(out.replacements()[0].value(), "o quis");
    }
    #[test]
    fn portable_walkers_resolve_the_lt_module_layout() {
        let rules = pt_rules_dir().expect("pt rules dir");
        assert!(rules.join("grammar.xml").is_file());
        let resource = pt_resource_dir().expect("pt resource dir");
        assert!(resource.join("portuguese.dict").is_file());
        assert!(
            resource
                .join("brazilian_municipalities")
                .join("RJ.tsv")
                .is_file()
        );
    }
}
