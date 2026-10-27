pub use og_rules::fr_suppress::*;

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{RuleMatch, SuggestedReplacement};

    fn base_rm(replacement: &str) -> RuleMatch {
        RuleMatch::new(
            "Les deux parties de ce mot peuvent s'unir.",
            4,
            9,
            og_core::RuleMatchRule::new("FR_SPLIT_WORDS_HYPHEN", "d"),
            og_core::RuleMatchContext::new("", 0, 0),
        )
        .with_replacements(vec![SuggestedReplacement::new(replacement)])
    }

    #[test]
    fn french_dictionary_misspelling_pins() {
        assert!(is_misspelled("pianist"));
        assert!(!is_misspelled("pianiste"));
        assert!(is_misspelled("postprécédent"));
        assert!(is_misspelled("surfaceèdu"));
        assert!(is_misspelled("murêne"));
        assert!(!is_misspelled("là"));
        assert!(!is_misspelled("où"));
        assert!(!is_misspelled("guerre"));
        assert!(!is_misspelled(""));
        assert!(!is_misspelled("."));
    }

    #[test]
    fn replacements_are_misspelled_per_french_token() {
        assert!(!replacement_is_misspelled("j'arrive"));
        assert!(!replacement_is_misspelled("D'enfants"));
        assert!(!replacement_is_misspelled("J'ai"));
        assert!(!replacement_is_misspelled("l'opulence"));
        assert!(!replacement_is_misspelled("m'ennuyer"));
        assert!(replacement_is_misspelled("pianist"));
        assert!(replacement_is_misspelled("surfaceèdu"));
        assert!(!replacement_is_misspelled("le carré"));
    }

    #[test]
    fn accept_rule_match_drops_match_when_every_suggestion_is_misspelled() {
        assert!(accept_rule_match(base_rm("pianist"), "suppressMatch:true", &[]).is_none());
    }

    #[test]
    fn accept_rule_match_keeps_valid_suggestions_and_match() {
        let kept =
            accept_rule_match(base_rm("pianiste"), "suppressMatch:true", &[]).expect("match kept");
        let values: Vec<&str> = kept.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(values, vec!["pianiste"]);
    }

    #[test]
    fn accept_rule_match_keeps_match_without_suggestions_when_suppress_match_false() {
        let kept =
            accept_rule_match(base_rm("pianist"), "suppressMatch:false", &[]).expect("match kept");
        assert!(kept.replacements().is_empty());
    }

    #[test]
    fn accept_rule_match_without_args_suppresses_by_default() {
        assert!(accept_rule_match(base_rm("pianist"), "", &[]).is_none());
    }

    #[test]
    fn accept_rule_match_drops_only_the_misspelled_suggestion() {
        let rm = base_rm("x").with_replacements(vec![
            SuggestedReplacement::new("pianist"),
            SuggestedReplacement::new("pianiste"),
        ]);
        let kept = accept_rule_match(rm, "suppressMatch:true", &[]).expect("match kept");
        let values: Vec<&str> = kept.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(values, vec!["pianiste"]);
    }

    #[test]
    fn portable_walkers_resolve_the_lt_module_layout() {
        let dir = fr_resource_dir().expect("fr resource dir");
        assert_eq!(
            Some(dir.clone()),
            crate::fr::test_support::fr_resource_dir()
        );
        assert!(dir.join("french.dict").exists());
        assert!(dir.join("hunspell/spelling.txt").exists());
    }

    #[test]
    fn portable_postag_matching_reads_the_pipeline_tagger_readings() {
        let tagger = crate::fr::tagger::FrenchTagger::shared_arc().expect("fr tagger");
        for word in ["guerre", "arrive", "l‘opulence", "pianiste", "j‘arrive"] {
            let tags: Vec<String> = og_tagger::Tagger::tag(tagger.as_ref(), &[word])
                .into_iter()
                .next()
                .map(|atr| {
                    atr.into_readings()
                        .iter()
                        .flat_map(|r| r.pos_tags().to_vec())
                        .collect()
                })
                .unwrap_or_default();
            for tag in &tags {
                assert!(replacement_matches_postag(word, tag), "{word} {tag}");
            }
        }
    }
}
