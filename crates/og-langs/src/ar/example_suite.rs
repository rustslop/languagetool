use og_core::checker::Disambiguator;
use og_core::rule::{Rule, TextLevelRule};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn ar_resource_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("ar/src/main/resources/org/languagetool/resource/ar"))
}

pub(crate) struct ArabicPipeline {
    tagger: Arc<og_tagger::arabic_tagger::ArabicTagger>,
    disambiguator: Option<Arc<dyn Disambiguator>>,
    splitter: Option<og_tokenizer::srx::SrxSentenceSplitter>,
}

impl ArabicPipeline {
    pub(crate) fn shared() -> std::sync::Arc<ArabicPipeline> {
        static PIPE: og_core::releasable::Releasable<ArabicPipeline> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&PIPE, || {
            let resource_dir =
                ar_resource_dir().expect("ar resource dir with arabic.dict + disambiguation.xml");
            let tagger = Arc::new(og_tagger::arabic_tagger::ArabicTagger::from_resource_dir(
                &resource_dir,
            ));
            let disambiguator = std::fs::read_to_string(resource_dir.join("disambiguation.xml"))
                .ok()
                .and_then(|xml| og_tagger::XmlDisambiguator::from_xml_with_english(&xml, None).ok())
                .and_then(|d| {
                    crate::engine::find_lt_core_resource_dir()
                        .map(|core| og_tagger::hybrid_disambiguator("ar", &resource_dir, &core, d))
                });
            Some(std::sync::Arc::new(ArabicPipeline {
                tagger,
                disambiguator,
                splitter: og_tokenizer::srx::srx_splitter("ar"),
            }))
        })
        .expect("ar pipeline")
    }

    pub(crate) fn tagger(&self) -> &og_tagger::arabic_tagger::ArabicTagger {
        self.tagger.as_ref()
    }

    pub(crate) fn analyze(&self, text: &str) -> Vec<AnalyzedSentence> {
        let ranges: Vec<(usize, usize)> = {
            use og_tokenizer::SentenceTokenizer as _;
            match &self.splitter {
                Some(srx) => srx
                    .split(text)
                    .iter()
                    .map(|s| (s.start(), s.end()))
                    .collect(),
                None => vec![(0, text.len())],
            }
        };
        ranges
            .into_iter()
            .map(|(start, end)| self.analyze_range(text, start, end))
            .collect()
    }

    fn analyze_range(&self, text: &str, start: usize, end: usize) -> AnalyzedSentence {
        let sentence_text = &text[start..end];
        let mut sentence = AnalyzedSentence::new(sentence_text, start, end);
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", start, start).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.extend({
            use og_tokenizer::WordTokenizer as _;
            og_tokenizer::DefaultWordTokenizer::new_splitting_hyphen()
                .tokenize(sentence_text)
                .into_iter()
                .map(|t| {
                    AnalyzedTokenReadings::new(AnalyzedToken::new(
                        t.text(),
                        t.start() + start,
                        t.end() + start,
                    ))
                })
        });
        sentence.set_tokens(tokens);
        let token_texts: Vec<String> = sentence
            .tokens()
            .iter()
            .skip(1)
            .map(|t| t.token().token().to_string())
            .collect();
        let text_refs: Vec<&str> = token_texts.iter().map(String::as_str).collect();
        let tagged = og_tagger::Tagger::tag(self.tagger.as_ref(), &text_refs);
        let tokens_mut = sentence.tokens_mut();
        for (i, tagged_atr) in tagged.into_iter().enumerate() {
            let token_idx = i + 1;
            if token_idx >= tokens_mut.len() {
                continue;
            }
            let start = tokens_mut[token_idx].token().start();
            let end = tokens_mut[token_idx].token().end();
            if tagged_atr.readings().is_empty() {
                let primary = AnalyzedToken::new(tagged_atr.token().token(), start, end);
                tokens_mut[token_idx] = AnalyzedTokenReadings::new(primary);
                continue;
            }
            let text = tagged_atr.token().token().to_string();
            let readings = tagged_atr.into_readings();
            let all_tags: Vec<String> = readings
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            let lemma = readings[0]
                .lemma()
                .map(str::to_string)
                .unwrap_or_else(|| text.clone());
            let updated_primary = AnalyzedToken::new(text, start, end)
                .with_pos_tags(all_tags)
                .with_lemma(lemma);
            tokens_mut[token_idx] =
                AnalyzedTokenReadings::new(updated_primary).with_readings(readings);
        }
        let final_tokens = sentence.tokens_mut();
        for i in (0..final_tokens.len()).rev() {
            if !final_tokens[i].is_whitespace() && !final_tokens[i].has_pos_tag("SENT_START") {
                if !final_tokens[i].has_pos_tag("SENT_END") {
                    final_tokens[i].add_sent_end();
                }
                break;
            }
        }
        if let Some(disambiguator) = &self.disambiguator {
            disambiguator.disambiguate(&mut sentence);
        }
        sentence
    }
}

pub(crate) fn java_reading_set(atr: &AnalyzedTokenReadings) -> Vec<String> {
    let mut set: Vec<String> = atr
        .readings()
        .iter()
        .map(|r| {
            format!(
                "{}/[{}]{}",
                atr.token().token(),
                r.lemma().unwrap_or("null"),
                r.pos_tags().first().map(String::as_str).unwrap_or("null")
            )
        })
        .collect();
    set.sort();
    set.dedup();
    set
}

pub(crate) fn sentence_matches(rule: &dyn Rule, text: &str) -> Vec<og_core::RuleMatch> {
    ArabicPipeline::shared()
        .analyze(text)
        .iter()
        .flat_map(|sentence| rule.match_sentence(sentence))
        .collect()
}

pub(crate) fn text_level_matches(rule: &dyn TextLevelRule, text: &str) -> Vec<og_core::RuleMatch> {
    let sentences = ArabicPipeline::shared().analyze(text);
    rule.match_text(text, &sentences)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn java_tag_line(input: &str) -> String {
        let pipeline = ArabicPipeline::shared();
        let no_whitespace: Vec<String> = {
            use og_tokenizer::WordTokenizer as _;
            og_tokenizer::DefaultWordTokenizer::new_splitting_hyphen()
                .tokenize(input)
                .into_iter()
                .filter(|t| !t.is_whitespace())
                .map(|t| t.text().to_string())
                .collect()
        };
        let refs: Vec<&str> = no_whitespace.iter().map(String::as_str).collect();
        let tagged = og_tagger::Tagger::tag(pipeline.tagger(), &refs);
        tagged
            .iter()
            .map(java_reading_set)
            .map(|set| set.join("|"))
            .collect::<Vec<_>>()
            .join(" -- ")
    }

    fn resource_dir() -> std::path::PathBuf {
        ar_resource_dir().expect("ar resource dir")
    }

    #[test]
    fn tagger_readings_match_arabic_tagger_test() {
        assert_eq!(
            java_tag_line("الخياريتان"),
            "الخياريتان/[خيار]NA-;F3--;--L|الخياريتان/[خيار]NJ-;F2--;--L|الخياريتان/[خيار]NJ-;F3--;--L"
        );
        assert_eq!(
            java_tag_line("السماء زرقاء"),
            "السماء/[سماء]NJ-;F1--;--L|السماء/[سماء]NJ-;F1A-;--L|السماء/[سماء]NJ-;F1I-;--L|السماء/[سماء]NJ-;F1U-;--L -- \
             زرقاء/[زرقاء]NA-;F1--;---|زرقاء/[زرقاء]NA-;F1A-;---|زرقاء/[زرقاء]NA-;F1I-;---|زرقاء/[زرقاء]NA-;F1U-;---"
        );
        assert_eq!(java_tag_line("العباره"), "العباره/[null]null");
        assert_eq!(
            java_tag_line("والبلاد"),
            "والبلاد/[بلاد]NJ-;F3--;W-L|والبلاد/[بلاد]NJ-;F3A-;W-L|والبلاد/[بلاد]NJ-;F3I-;W-L|والبلاد/[بلاد]NJ-;F3U-;W-L|والبلاد/[بلاد]NJ-;M1--;W-L|والبلاد/[بلاد]NJ-;M1A-;W-L|والبلاد/[بلاد]NJ-;M1I-;W-L|والبلاد/[بلاد]NJ-;M1U-;W-L"
        );
        assert_eq!(
            java_tag_line("بلادهما"),
            "بلادهما/[بلاد]NJ-;F3--;--H|بلادهما/[بلاد]NJ-;F3A-;--H|بلادهما/[بلاد]NJ-;F3I-;--H|بلادهما/[بلاد]NJ-;F3U-;--H|بلادهما/[بلاد]NJ-;M1--;--H|بلادهما/[بلاد]NJ-;M1A-;--H|بلادهما/[بلاد]NJ-;M1I-;--H|بلادهما/[بلاد]NJ-;M1U-;--H"
        );
        assert_eq!(
            java_tag_line("كبلاد"),
            "كبلاد/[بلاد]NJ-;F3--;-K-|كبلاد/[بلاد]NJ-;F3I-;-K-|كبلاد/[بلاد]NJ-;M1--;-K-|كبلاد/[بلاد]NJ-;M1I-;-K-"
        );
        assert_eq!(
            java_tag_line("وكالبلاد"),
            "وكالبلاد/[بلاد]NJ-;F3--;WKL|وكالبلاد/[بلاد]NJ-;F3I-;WKL|وكالبلاد/[بلاد]NJ-;M1--;WKL|وكالبلاد/[بلاد]NJ-;M1I-;WKL"
        );
        assert_eq!(
            java_tag_line("فاستعملها"),
            "فاستعملها/[اِسْتَعْمَلَ]V61;M1H-pa-;W-H|فاستعملها/[اِسْتَعْمَلَ]V61;M1Y-i--;W-H"
        );
        assert_eq!(
            java_tag_line("سيعملون"),
            "سيعملون/[أَعْمَلَ]V41;M3H-faU;-S-|سيعملون/[أَعْمَلَ]V41;M3H-fpU;-S-|سيعملون/[عَمِلَ]V31;M3H-faU;-S-|سيعملون/[عَمِلَ]V31;M3H-fpU;-S-|سيعملون/[عَمَّلَ]V41;M3H-faU;-S-|سيعملون/[عَمَّلَ]V41;M3H-fpU;-S-"
        );
        assert_eq!(
            java_tag_line("فسيعملون"),
            "فسيعملون/[أَعْمَلَ]V41;M3H-faU;WS-|فسيعملون/[أَعْمَلَ]V41;M3H-fpU;WS-|فسيعملون/[عَمِلَ]V31;M3H-faU;WS-|فسيعملون/[عَمِلَ]V31;M3H-fpU;WS-|فسيعملون/[عَمَّلَ]V41;M3H-faU;WS-|فسيعملون/[عَمَّلَ]V41;M3H-fpU;WS-"
        );
        assert_eq!(
            java_tag_line("كتاب"),
            "كتاب/[كتاب]NA-;-3--;---|كتاب/[كتاب]NA-;-3A-;---|كتاب/[كتاب]NA-;-3I-;---|كتاب/[كتاب]NA-;-3U-;---|كتاب/[كتاب]NJ-;M1--;---|كتاب/[كتاب]NJ-;M1A-;---|كتاب/[كتاب]NJ-;M1I-;---|كتاب/[كتاب]NJ-;M1U-;---|كتاب/[كتاب]NM-;M1--;---|كتاب/[كتاب]NM-;M1A-;---|كتاب/[كتاب]NM-;M1I-;---|كتاب/[كتاب]NM-;M1U-;---"
        );
        assert_eq!(
            java_tag_line("للبلاد"),
            "للبلاد/[بلاد]NJ-;F3--;-LL|للبلاد/[بلاد]NJ-;F3I-;-LL|للبلاد/[بلاد]NJ-;M1--;-LL|للبلاد/[بلاد]NJ-;M1I-;-LL"
        );
        assert_eq!(
            java_tag_line("للاعب"),
            "للاعب/[لاعب]NA-;M1--;-L-|للاعب/[لاعب]NA-;M1--;-LL|للاعب/[لاعب]NA-;M1I-;-L-|للاعب/[لاعب]NA-;M1I-;-LL|للاعب/[لَاعَبَ]V41;M1H-pa-;-L-|للاعب/[لَاعَبَ]V41;M1Y-i--;-L-"
        );
        assert_eq!(
            java_tag_line("ببلاد"),
            "ببلاد/[بلاد]NJ-;F3--;-B-|ببلاد/[بلاد]NJ-;F3I-;-B-|ببلاد/[بلاد]NJ-;M1--;-B-|ببلاد/[بلاد]NJ-;M1I-;-B-"
        );
    }

    #[test]
    fn synthesizer_matches_arabic_synthesizer_test() {
        let synth =
            og_tagger::arabic_synthesizer::ArabicSynthesizer::from_resource_dir(&resource_dir());
        assert_eq!(
            synth.synthesize("خيار", "NJ-;F2--;---"),
            vec!["خيارتان".to_string(), "خياريتان".to_string()]
        );
        assert_eq!(
            synth.synthesize("بلاد", "NJ-;F3A-;--H"),
            vec![
                "بلادت".to_string(),
                "بلادتي".to_string(),
                "بلاد".to_string(),
                "بلادي".to_string()
            ]
        );
        assert_eq!(
            synth.synthesize("اِسْتَعْمَلَ", "V61;M3Y-pa-;--H"),
            vec!["استعملتمو".to_string()]
        );
        assert_eq!(
            synth.synthesize("اِسْتَكْمَلَ", "V61;M3Y-pa-;---"),
            vec!["استكملتم".to_string()]
        );
        assert_eq!(
            synth.synthesize("اِسْتَمَعَ", "V51;M3Y-pa-;---"),
            vec!["استمعتم".to_string()]
        );
    }

    #[test]
    fn srx_matches_arabic_srx_sentence_tokenizer_test() {
        for text in [
            "مشوار التعلم طويل.",
            "هل ستنام الليلة؟",
            "قُل: توْأمٌ، وتوْأمانِ: وقلْ: هذانِ توْأمانِ.. ",
            "قلْ: هذِهِ توْأمُ «هذا»، (وقلْ: هذِهِ توْأمةُ هذا)، وقلْ: هذانِ توْأمٌ!",
        ] {
            assert_eq!(
                ArabicPipeline::shared().analyze(text).len(),
                1,
                "must not split {text}"
            );
        }
    }

    #[test]
    fn comma_whitespace_counts_match_java_test() {
        let rule = super::super::arabic_comma_whitespace_rule();
        for (text, expected) in [
            ("هذه جملة تجريبية.", 0),
            ("هذه, هي, جملة التجربة.", 0),
            ("قل (كيت وكيت) تجربة!.", 0),
            ("تكلف €2,45.", 0),
            ("ثمنها 50,- يورو", 0),
            ("جملة مع علامات الحذف ...", 0),
            ("هذه صورة: .5 وهي صحيحة.", 0),
            ("هذه $1,000,000.", 0),
            ("هذه 1,5.", 0),
            ("هذا ,,فحص''.", 0),
            ("نفّذ ./validate.sh لفحص الملف.", 0),
            ("هذه,\u{00A0}حقا,\u{00A0}فراغ غير فاصل.", 0),
            ("هذه،جملة للتجربة.", 1),
            ("هذه ، جملة للتجربة.", 1),
            ("هذه ،تجربة جملة.", 2),
            ("،هذه جملة للتجربة.", 2),
        ] {
            assert_eq!(
                sentence_matches(&rule, text).len(),
                expected,
                "count for {text:?}"
            );
        }
    }

    #[test]
    fn question_mark_whitespace_counts_match_java_test() {
        let rule = super::super::arabic_question_mark_whitespace_rule();
        for (text, expected) in [
            ("أهذه تجربة؟", 0),
            ("This is a test sentence?", 0),
            ("أهذه تجربة?", 0),
            ("This is a test sentence؟", 0),
            ("أهذه تجربة ؟", 1),
        ] {
            assert_eq!(
                sentence_matches(&rule, text).len(),
                expected,
                "count for {text:?}"
            );
        }
    }

    #[test]
    fn semicolon_whitespace_counts_match_java_test() {
        let rule = super::super::arabic_semicolon_whitespace_rule();
        for (text, expected) in [
            ("This is a test sentence؛", 0),
            ("أهذه تجربة؛", 0),
            ("أهذه تجربة ؛", 1),
        ] {
            assert_eq!(
                sentence_matches(&rule, text).len(),
                expected,
                "count for {text:?}"
            );
        }
    }

    #[test]
    fn darja_rule_matches_java_test() {
        let rule = super::super::arabic_darja_rule();
        assert!(sentence_matches(&rule, "إن شاء").is_empty());
        let matches = sentence_matches(&rule, "طرشي");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "فلفل حلو");
        assert_eq!(sentence_matches(&rule, "فايدة").len(), 1);
    }

    #[test]
    fn diacritics_rule_matches_java_test() {
        let rule = super::super::arabic_diacritics_rule();
        assert_eq!(sentence_matches(&rule, "تجربة").len(), 1);
    }

    #[test]
    fn simple_replace_rule_matches_java_test() {
        let rule = super::super::arabic_simple_replace_rule();
        assert!(sentence_matches(&rule, "عبد الله").is_empty());
        let matches = sentence_matches(&rule, "عبدالله");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "عبد الله");
        assert_eq!(sentence_matches(&rule, "يافطة").len(), 1);
        assert_eq!(sentence_matches(&rule, "المائة").len(), 1);
        assert_eq!(sentence_matches(&rule, "الذى").len(), 1);
    }

    #[test]
    fn homophones_rule_matches_java_test() {
        let rule = super::super::arabic_homophones_rule();
        assert_eq!(sentence_matches(&rule, "ضن").len(), 1);
        assert_eq!(sentence_matches(&rule, "حاضر").len(), 1);
        assert_eq!(sentence_matches(&rule, "حض").len(), 1);
    }

    #[test]
    fn redundancy_rule_matches_java_test() {
        let rule = super::super::arabic_redundancy_rule();
        let matches = sentence_matches(&rule, "سوف لن");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "لن");
    }

    #[test]
    fn wordiness_rule_matches_java_test() {
        let rule = super::super::arabic_wordiness_rule();
        assert_eq!(sentence_matches(&rule, "وأخيرا وليس آخرا").len(), 1);
    }

    #[test]
    fn word_repeat_rule_matches_java_example_and_ignores() {
        let rule = super::super::arabic_word_repeat_rule();
        let text = "هذا فقط فقط مثال.";
        let matches = sentence_matches(&rule, text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), "هذا ".len());
        assert_eq!(matches[0].length(), "فقط فقط".len());
        assert_eq!(
            &text[matches[0].offset()..matches[0].offset() + matches[0].length()],
            "فقط فقط"
        );
        assert_eq!(matches[0].replacements()[0].value(), "فقط");
        assert!(sentence_matches(&rule, "نفذت التعليمات خطوة خطوة.").is_empty());
        assert!(sentence_matches(&rule, "امش رويدا رويدا").is_empty());
    }

    #[test]
    fn wrong_word_in_context_rule_is_inert_like_java() {
        let rule = super::super::arabic_wrong_word_in_context_rule();
        assert!(sentence_matches(&rule, "من سوء الضن بالله ترك الأمر بالمعروف.").is_empty());
    }

    #[test]
    fn java_embedded_examples_pin_exact_texts() {
        let wordiness = super::super::arabic_wordiness_rule();
        let matches = sentence_matches(&wordiness, "هناك خطأ في العبارة");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "في العبارة خطأ");

        let simple_replace = super::super::arabic_simple_replace_rule();
        let matches = sentence_matches(&simple_replace, "الى");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "إلى");

        let trans_verb =
            super::super::trans_verb::ArabicTransVerbRule::from_resource_dir(&resource_dir());
        assert!(
            sentence_matches(&trans_verb, "The train arrived a hour ago.").is_empty(),
            "Java's embedded example is an English leftover no Arabic verb-data word matches"
        );
    }

    #[test]
    fn replace_rules_match_phrases_beyond_three_tokens() {
        let wordiness = super::super::arabic_wordiness_rule();
        let matches = sentence_matches(&wordiness, "هناك مشكلة في الكمبيوتر");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "في الحاسوب مشكلة");

        let simple_replace = super::super::arabic_simple_replace_rule();
        let matches = sentence_matches(&simple_replace, "أشهد ألا إله إلا الله");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacements()[0].value(),
            "أشهد أن لا إله إلا الله"
        );
        let matches = sentence_matches(&simple_replace, "الرضى من أجل النعم على الإنسان");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "الر\u{651}\u{650}ضا");
    }

    #[test]
    fn replace_rules_keep_only_the_longest_match() {
        let simple_replace = super::super::arabic_simple_replace_rule();
        let text = "فترة طويلة";
        let matches = sentence_matches(&simple_replace, text);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            &text[matches[0].offset()..matches[0].offset() + matches[0].length()],
            text
        );
        assert_eq!(matches[0].replacements()[0].value(), "مدة");

        let diacritics = super::super::arabic_diacritics_rule();
        let matches = sentence_matches(&diacritics, "هضبة الجولان");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "هضبة الجَوْلان");
    }

    #[test]
    fn replace_rules_render_java_messages() {
        let simple_replace = super::super::arabic_simple_replace_rule();
        let matches = sentence_matches(&simple_replace, "أسطحة");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].message(),
            "قل <suggestion>سطوح</suggestion> أو  <suggestion>أسْطح</suggestion>"
        );
        assert_eq!(matches[0].short_message(), Some("خطأ، يفضل أن  يقال:"));

        let with_tab = sentence_matches(&simple_replace, "فترة");
        assert_eq!(with_tab.len(), 1);
        assert_eq!(
            with_tab[0].message(),
            "الفترة في اللغة العربية الضعف والسكون وانكسار الحدة والنشاط، وتطلق على ما بين نبيين"
        );
    }

    #[test]
    fn word_coherency_rule_matches_java_example() {
        let rule = super::super::arabic_word_coherency_rule();
        let text = "وزارة الشؤون الخارجية تهتم  بكل شئون العالم.";
        let matches = text_level_matches(&rule, text);
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(&text[m.offset()..m.offset() + m.length()], "شئون");
        assert_eq!(m.replacements()[0].value(), "شؤون");
        assert!(m.message().contains("('شئون' و 'شؤون')"));
    }

    #[test]
    fn trans_verb_rule_matches_java_test() {
        let rule =
            super::super::trans_verb::ArabicTransVerbRule::from_resource_dir(&resource_dir());
        {
            let text = "كان أَفَاضَ في الحديث";
            assert!(sentence_matches(&rule, text).is_empty(), "{text}");
        }
        for text in [
            "كان أفاض من الحديث",
            "لقد أفاضت من الحديث",
            "لقد أفاضت الحديث",
            "كان أفاضها الحديث",
            "إذ استعجل الأمر",
        ] {
            assert_eq!(sentence_matches(&rule, text).len(), 1, "{text}");
        }
    }

    #[test]
    fn inflected_one_word_rule_matches_java_test() {
        let rule =
            super::super::inflected_one_word::ArabicInflectedOneWordReplaceRule::from_resource_dir(
                &resource_dir(),
            );
        for text in ["أجريت بحوثا في المخبر", "وجعل لكم من أزواجكم بنين وحفدة"]
        {
            assert!(sentence_matches(&rule, text).is_empty(), "{text}");
        }
        for text in ["أجريت أبحاثا في المخبر", "وجعل لكم من أزواجكم بنين وأحفاد"]
        {
            assert!(
                !sentence_matches(&rule, text).is_empty(),
                "expected match on {text}"
            );
        }
    }

    #[test]
    fn double_punctuation_rule_matches_java_example() {
        let rule = super::super::arabic_double_punctuation_rule();
        assert_eq!(sentence_matches(&rule, "لقد.. نجحنا.").len(), 1);
        assert!(sentence_matches(&rule, "لقد. نجحنا.").is_empty());
    }

    #[test]
    fn second_sentence_matches_carry_global_offsets() {
        let first = "جملة أولى تماما.";
        let rule = super::super::arabic_word_repeat_rule();
        let matches = sentence_matches(&rule, &format!("{first} هذا فقط فقط مثال."));
        assert_eq!(matches.len(), 1);
        assert!(matches[0].offset() > first.len());
        assert!(matches[0].context().text().contains("فقط"));

        let comma_rule = super::super::arabic_comma_whitespace_rule();
        let matches = sentence_matches(&comma_rule, &format!("{first} هذه ،تجربة."));
        assert!(!matches.is_empty());
        assert!(matches.iter().all(|m| m.offset() >= first.len()));

        let trans_rule =
            super::super::trans_verb::ArabicTransVerbRule::from_resource_dir(&resource_dir());
        let matches = sentence_matches(&trans_rule, &format!("{first} كان أفاض من الحديث"));
        assert_eq!(matches.len(), 1);
        assert!(matches[0].offset() > first.len());
        assert!(matches[0].context().text().contains("أفاض"));
    }
}
