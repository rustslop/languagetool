use std::collections::HashSet;
use std::path::Path;

use og_core::{AnalyzedToken, AnalyzedTokenReadings};

use crate::arabic_tagger::{
    ArabicTagger, get_conjunction_prefix, get_definite_prefix, get_jar_prefix, has_conjunction,
    has_jar, has_pronoun, is_attached, is_definite, merge_pos_tag, set_flag, set_procletic_flags,
};
use crate::morfologik::{SynthDictionary, shared_synth_dictionary};

pub struct ArabicSynthesizer {
    dictionary: Option<std::sync::Arc<SynthDictionary>>,
}

impl ArabicSynthesizer {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        Self {
            dictionary: shared_synth_dictionary(resource_dir, "arabic_synth"),
        }
    }

    pub fn has_dictionary(&self) -> bool {
        self.dictionary.is_some()
    }

    pub fn synthesize(&self, lemma: &str, postag: &str) -> Vec<String> {
        let Some(dictionary) = &self.dictionary else {
            return Vec::new();
        };
        dictionary
            .lookup(&format!("{lemma}|{postag}"))
            .iter()
            .map(|stem| correct_stem(stem, postag))
            .collect()
    }

    pub fn inflect_lemma_like(
        &self,
        tagger: &ArabicTagger,
        target_lemma: &str,
        source_token: &AnalyzedToken,
    ) -> Vec<String> {
        let token_read_list: AnalyzedTokenReadings = tagger.tag_word(target_lemma);
        let has_lemma = token_read_list
            .readings()
            .iter()
            .any(|r| r.lemma() == Some(target_lemma));
        if !has_lemma {
            return vec![format!("[{target_lemma}]")];
        }

        let source_postag = source_token.pos_tags().first().cloned().unwrap_or_default();
        let prefix = tagger.get_proclitic(source_token);
        let suffix = tagger.get_enclitic(source_token);

        let mut wordlist = Vec::new();
        for current_token in token_read_list.readings() {
            if current_token.lemma() != Some(target_lemma) {
                continue;
            }
            let postag_lemma = current_token
                .pos_tags()
                .first()
                .cloned()
                .unwrap_or_default();
            let merged = merge_pos_tag(&source_postag, &postag_lemma);
            let word = format!("{prefix}{target_lemma}");
            wordlist.extend(self.set_enclitic_multiple(
                tagger,
                &word,
                &merged,
                target_lemma,
                &suffix,
            ));
        }
        let mut seen = HashSet::new();
        wordlist
            .into_iter()
            .filter(|w| seen.insert(w.clone()))
            .collect()
    }

    fn set_enclitic_multiple(
        &self,
        tagger: &ArabicTagger,
        word: &str,
        postag: &str,
        lemma: &str,
        suffix: &str,
    ) -> Vec<String> {
        let default_wordlist = vec![format!("({word})")];
        if postag.is_empty() {
            return default_wordlist;
        }
        let flag = if suffix.is_empty() { '-' } else { 'H' };
        let token = AnalyzedToken::new(word, 0, word.len())
            .with_lemma(lemma.to_string())
            .with_pos_tags(vec![postag.to_string()]);
        let procletic = tagger.get_proclitic(&token);
        let mut new_postag = set_flag(postag, "PRONOUN", flag);
        new_postag = set_procletic_flags(&new_postag);

        let new_words = self.synthesize(lemma, &new_postag);
        let mut wordlist = Vec::new();
        if !new_words.is_empty() {
            for stem in new_words {
                let new_word = if has_pronoun(&new_postag) && flag == 'H' {
                    if stem.ends_with('\u{64a}') {
                        if suffix == "\u{64a}" {
                            format!("{procletic}{stem}")
                        } else {
                            String::new()
                        }
                    } else if stem.ends_with('\u{647}') {
                        format!(
                            "{procletic}{}{suffix}",
                            stem.strip_suffix('\u{647}').unwrap_or(&stem)
                        )
                    } else {
                        format!("{procletic}{stem}{suffix}")
                    }
                } else {
                    format!("{procletic}{stem}")
                };
                if !new_word.is_empty() {
                    wordlist.push(new_word);
                }
            }
        } else {
            wordlist.push(format!("({word})"));
        }
        if wordlist.is_empty() {
            return default_wordlist;
        }
        wordlist
    }
}

fn correct_stem(stem: &str, postag: &str) -> String {
    let mut corrected = stem.to_string();
    if is_attached(postag) {
        corrected = corrected
            .strip_suffix('\u{647}')
            .unwrap_or(&corrected)
            .to_string();
    }
    if is_definite(postag) {
        corrected = format!("{}{corrected}", get_definite_prefix(postag));
    }
    if has_jar(postag) {
        corrected = format!("{}{corrected}", get_jar_prefix(postag));
    }
    if has_conjunction(postag) {
        corrected = format!("{}{corrected}", get_conjunction_prefix(postag));
    }
    corrected
}

pub fn correct_tag(postag: &str) -> String {
    let tag = crate::arabic_tagger::set_conjunction(postag, "-");
    let tag = set_definite(&tag, "-");
    crate::arabic_tagger::unify_pronoun_tag(tag)
}

fn set_definite(postag: &str, flag: &str) -> String {
    let myflag = match flag {
        "ال" | "L" | "لل" | "D" => 'L',
        "-" | "" => '-',
        _ => return postag.to_string(),
    };
    if crate::arabic_tagger::is_unattached_noun(postag) {
        set_flag(postag, "PRONOUN", myflag)
    } else {
        postag.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arabic_tagger::get_flag;

    fn resource_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/resource/ar",
        )
    }

    #[test]
    fn correct_tag_clears_clitics_and_unifies_pronouns() {
        assert_eq!(correct_tag("NJ-;F3--;W-L"), "NJ-;F3--;---");
        assert_eq!(correct_tag("NJ-;F3I;-KL"), "NJ-;F3I;--L");
        assert_eq!(correct_tag("V41;M3H-paU;W-H"), "V41;M3H-paU;--H");
        assert_eq!(correct_tag("PART;---;---"), "PART;---;---");
    }

    #[test]
    fn missing_dictionary_synthesizes_nothing() {
        let synth = ArabicSynthesizer::from_resource_dir(std::path::Path::new("/nonexistent"));
        assert!(!synth.has_dictionary());
        assert!(synth.synthesize("آب", "NJ-;F1--;--H").is_empty());
    }

    #[test]
    fn inflecting_an_unknown_lemma_returns_the_bracketed_lemma() {
        let synth = ArabicSynthesizer::from_resource_dir(&resource_dir());
        let tagger = ArabicTagger::from_resource_dir(&resource_dir());
        let source = AnalyzedToken::new("كتاب", 0, 0)
            .with_lemma("كتاب".to_string())
            .with_pos_tags(vec!["NJ-;F3--;--H".to_string()]);
        assert_eq!(
            synth.inflect_lemma_like(&tagger, "قققةق", &source),
            vec!["[قققةق]".to_string()]
        );
    }

    #[test]
    fn inflect_lemma_like_shapes_pronoun_and_definite_forms() {
        let synth = ArabicSynthesizer::from_resource_dir(&resource_dir());
        let tagger = ArabicTagger::from_resource_dir(&resource_dir()).with_new_style_pronoun_tag();
        let mk = |word: &str, tag: &str| {
            AnalyzedToken::new(word, 0, 0)
                .with_lemma(word.to_string())
                .with_pos_tags(vec![tag.to_string()])
        };

        let forms = synth.inflect_lemma_like(&tagger, "كتاب", &mk("كتابهم", "NJ-;F3--;--H"));
        assert_eq!(forms, vec!["كتاباتهم", "كتابتهم", "كتابياتهم"]);

        let forms = synth.inflect_lemma_like(&tagger, "كتاب", &mk("كتابه", "NJ-;F3--;--H"));
        assert_eq!(forms, vec!["كتاباته", "كتابته", "كتابياته"]);

        let forms = synth.inflect_lemma_like(&tagger, "كتاب", &mk("الكتاب", "NJ-;F3--;--L"));
        assert_eq!(
            forms,
            vec![
                "الكتابات",
                "الكتابة",
                "الكتابتان",
                "الكتابيات",
                "الكتابيتان"
            ]
        );

        let forms = synth.inflect_lemma_like(&tagger, "كتاب", &mk("وبالكتاب", "NJ-;F3I;-KL"));
        assert_eq!(forms, vec!["(وبكتاب)"]);
    }

    #[test]
    fn synthesize_returns_corrected_stem_for_attached_tag() {
        let synth = ArabicSynthesizer::from_resource_dir(&resource_dir());
        if !synth.has_dictionary() {
            return;
        }
        let forms = synth.synthesize("آب", "NJ-;F1--;--H");
        assert!(
            forms.iter().any(|f| f == "آبته" || f == "آبتي"),
            "attached pronoun form expected, got {forms:?}"
        );
    }

    #[test]
    fn inflect_lemma_like_uses_source_affixes() {
        let synth = ArabicSynthesizer::from_resource_dir(&resource_dir());
        if !synth.has_dictionary() {
            return;
        }
        let tagger = ArabicTagger::from_resource_dir(&resource_dir()).with_new_style_pronoun_tag();
        let source = AnalyzedToken::new("أبحاث", 0, 0)
            .with_lemma("أبحاث".to_string())
            .with_pos_tags(vec!["NM-;-1--;---".to_string()]);
        let forms = synth.inflect_lemma_like(&tagger, "بحوث", &source);
        assert!(
            !forms.is_empty(),
            "expected at least one inflected suggestion"
        );
    }

    #[test]
    fn flag_helpers_match_java_positions() {
        assert!(is_attached("NA-;-1A-;--H"));
        assert!(!is_attached("NA-;-1A-;---"));
        assert!(is_definite("NA-;-1A-;--L"));
        assert_eq!(get_definite_prefix("NA-;-1A-;--L"), "ال");
        assert_eq!(get_jar_prefix("NA-;-1A-;-K-"), "ك");
        assert_eq!(get_conjunction_prefix("VW1;M3H-faU;WS-"), "و");
        assert_eq!(
            merge_pos_tag("VW1;--------;---", "V31;--------;---"),
            "V31;--------;---"
        );
        assert_eq!(get_flag("NA-;-1A-;-K-", "JAR"), 'K');
    }
}
