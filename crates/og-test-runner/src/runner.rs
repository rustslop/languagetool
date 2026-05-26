use og_core::checker::{SentenceTokenizer, WordTokenizer, Disambiguator};
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, SentenceRange};
use og_xml::compiler::XmlCompiler;
use og_rules::pattern_rule::PatternRuleEngine;
use og_tagger::{EnglishTagger, Tagger, XmlDisambiguator};
use std::path::Path;

/// Result of running a single example test
#[derive(Debug, Clone)]
pub struct ExampleTestResult {
    pub rule_id: String,
    pub passed: bool,
    pub message: String,
    pub example_text: String,
    pub expected_type: String,
}

/// Summary of running all example tests for a grammar file
#[derive(Debug)]
pub struct TestRunSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<ExampleTestResult>,
}

impl TestRunSummary {
    pub fn is_all_passed(&self) -> bool {
        self.failed == 0
    }
}

/// Bridge tokenizer for test runner
struct TestWordTokenizer;

impl WordTokenizer for TestWordTokenizer {
    fn tokenize(&self, text: &str, offset: usize) -> Vec<AnalyzedTokenReadings> {
        let tokenizer = og_tokenizer::DefaultWordTokenizer::new();
        use og_tokenizer::WordTokenizer as _;
        let tokens = tokenizer.tokenize(text);
        tokens.into_iter().map(|t| {
            let at = AnalyzedToken::new(t.text(), t.start() + offset, t.end() + offset);
            AnalyzedTokenReadings::new(at)
        }).collect()
    }
}

struct TestSentenceTokenizer;

impl SentenceTokenizer for TestSentenceTokenizer {
    fn split(&self, text: &str) -> Vec<SentenceRange> {
        let splitter = og_tokenizer::DefaultSentenceSplitter::new();
        use og_tokenizer::SentenceTokenizer as _;
        let sentences = splitter.split(text);
        sentences.into_iter().map(|s| SentenceRange {
            start: s.start(),
            end: s.end(),
        }).collect()
    }
}

pub struct TestRunner {
    pattern_engine: PatternRuleEngine,
    tagger: EnglishTagger,
    disambiguator: Option<XmlDisambiguator>,
}

impl TestRunner {
    pub fn new() -> Self {
        let mut tagger = EnglishTagger::new();

        // Load added.txt if available
        let added_path = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/added.txt";
        if Path::new(added_path).exists() {
            if let Ok(data) = std::fs::read_to_string(added_path) {
                tagger.load_added(&data);
            }
        }

        // Load uncountable nouns (-> NN:U)
        let uncountable_path = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/uncountable.txt";
        if Path::new(uncountable_path).exists() {
            if let Ok(data) = std::fs::read_to_string(uncountable_path) {
                tagger.load_uncountable(&data);
            }
        }

        // Load partly-countable nouns (-> NN:UN)
        let partlycountable_path = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/partlycountable.txt";
        if Path::new(partlycountable_path).exists() {
            if let Ok(data) = std::fs::read_to_string(partlycountable_path) {
                tagger.load_partlycountable(&data);
            }
        }

        // Load FSA dictionary for more accurate POS tagging (only for unknown words)
        let dict_path = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/dict_decoded.txt";
        if Path::new(dict_path).exists() {
            if let Ok(data) = std::fs::read_to_string(dict_path) {
                let count = tagger.load_fsa_dictionary(&data);
                eprintln!("Loaded {} FSA dictionary entries", count);
            }
        }

        // Load disambiguation rules if available
        let disambiguator = {
            let disambig_path = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/resource/en/disambiguation.xml";
            if Path::new(disambig_path).exists() {
                if let Ok(xml) = std::fs::read_to_string(disambig_path) {
                    match XmlDisambiguator::from_xml(&xml) {
                        Ok(d) => {
                            eprintln!("Loaded {} disambiguation rules", d.rule_count());
                            Some(d)
                        }
                        Err(e) => {
                            eprintln!("Warning: failed to load disambiguation: {}", e);
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        Self {
            pattern_engine: PatternRuleEngine::new(),
            tagger,
            disambiguator,
        }
    }

    /// Tokenize text into an AnalyzedSentence using the real tokenizer + POS tagger + disambiguator
    fn tokenize_sentence(&self, text: &str) -> AnalyzedSentence {
        let tokenizer = TestWordTokenizer;
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = tokenizer.tokenize(text, 0);
        // Add SENT_START token
        let start_token = AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()])
        );
        tokens.insert(0, start_token);

        // Apply POS tagger to non-SENT_START tokens
        let token_texts: Vec<String> = tokens.iter().skip(1).map(|t| t.token().token().to_string()).collect();
        let text_refs: Vec<&str> = token_texts.iter().map(|s| s.as_str()).collect();
        let tagged = Tagger::tag(&self.tagger, &text_refs);

        // Merge POS tags back into tokens
        for (i, tagged_atr) in tagged.into_iter().enumerate() {
            let token_idx = i + 1; // Skip SENT_START
            if token_idx < tokens.len() {
                let readings = tagged_atr.readings().to_vec();
                if !readings.is_empty() {
                    let primary = tokens[token_idx].token().clone();
                    let all_tags: Vec<String> = readings.iter()
                        .flat_map(|r| r.pos_tags().to_vec())
                        .collect();
                    let updated_primary = AnalyzedToken::new(
                        primary.token(),
                        primary.start(),
                        primary.end()
                    )
                    .with_pos_tags(all_tags)
                    .with_lemma(readings[0].lemma().unwrap_or(primary.token()).to_string());

                    tokens[token_idx] = AnalyzedTokenReadings::new(updated_primary).with_readings(readings);
                }
            }
        }

        sentence.set_tokens(tokens);

        // Add SENT_END tag to last non-whitespace token (like Java's setSentEnd())
        // In Java LT, every sentence's last non-whitespace token gets a SENT_END reading added
        let tokens = sentence.tokens_mut();
        for i in (0..tokens.len()).rev() {
            if !tokens[i].is_whitespace() && !tokens[i].token().pos_tags().contains(&"SENT_START".to_string()) {
                if !tokens[i].has_pos_tag("SENT_END") {
                    tokens[i].add_sent_end();
                }
                break;
            }
        }

        // Apply chunking BEFORE disambiguation (chunk info is needed by disambig rules)
        self.apply_chunking(&mut sentence);

        // Apply disambiguation rules
        if let Some(ref disambiguator) = self.disambiguator {
            disambiguator.disambiguate(&mut sentence);
        }

        // Apply contextual disambiguation heuristics
        self.apply_contextual_disambiguation(&mut sentence);

        // Re-chunk after disambiguation (POS tags may have changed)
        self.apply_chunking(&mut sentence);

        sentence
    }

    /// Apply contextual disambiguation heuristics to reduce false positives.
    /// These are simple rules that remove unlikely POS tags based on surrounding context.
    fn apply_contextual_disambiguation(&self, sentence: &mut AnalyzedSentence) {
        let tokens = sentence.tokens();
        let n = tokens.len();

        // Build list of non-whitespace token indices
        let nw: Vec<usize> = (0..n).filter(|&i| !tokens[i].is_whitespace()).collect();
        if nw.len() < 3 { return; }

        // Build parallel arrays of text and tags for non-ws tokens
        let nw_text: Vec<&str> = nw.iter().map(|&i| tokens[i].token().token()).collect();
        let nw_has_tag: Vec<Vec<String>> = nw.iter().map(|&i| {
            let mut tags: Vec<String> = Vec::new();
            for r in tokens[i].readings() {
                for t in r.pos_tags() {
                    if !tags.contains(&t.to_string()) {
                        tags.push(t.to_string());
                    }
                }
            }
            tags
        }).collect();

        let mut removals: Vec<(usize, String)> = Vec::new(); // (nw_index, tag_to_remove)

        for i in 0..nw.len() {
            let text = nw_text[i];
            let lower = text.to_lowercase();
            let tags = &nw_has_tag[i];

            // Rule 1: After "do/does/did" used as auxiliary (has VB/VBZ/VBD tag),
            // remove VBZ/VB from following word if it also has NN/NNS.
            // e.g., "I do flips" → "flips" should be NNS, not VBZ
            if i >= 1 {
                let prev_lower = nw_text[i-1].to_lowercase();
                let prev_is_do_verb = (prev_lower == "do" || prev_lower == "does" || prev_lower == "did")
                    && nw_has_tag[i-1].iter().any(|t| t.starts_with("VB"));
                if prev_is_do_verb {
                    if tags.contains(&"VBZ".to_string()) && tags.iter().any(|t| t.starts_with("NN")) {
                        removals.push((i, "VBZ".to_string()));
                    }
                    if tags.contains(&"VB".to_string()) && tags.iter().any(|t| t.starts_with("NN"))
                        && !tags.contains(&"NN:UN".to_string()) && !tags.contains(&"NN:U".to_string()) {
                        removals.push((i, "VB".to_string()));
                    }
                }
                // Also handle "do not flips" pattern
                if i >= 2 && nw_text[i-1].eq_ignore_ascii_case("not") {
                    let do_idx = i - 2;
                    let do_lower = nw_text[do_idx].to_lowercase();
                    let do_is_verb = (do_lower == "do" || do_lower == "does" || do_lower == "did")
                        && nw_has_tag[do_idx].iter().any(|t| t.starts_with("VB"));
                    if do_is_verb {
                        if tags.contains(&"VBZ".to_string()) && tags.iter().any(|t| t.starts_with("NN")) {
                            removals.push((i, "VBZ".to_string()));
                        }
                    }
                }
            }

            // Rule 2: "does" as plural of "doe" (deer) - "The does grazed"
            if lower == "does" && tags.contains(&"VBZ".to_string()) && tags.contains(&"NNS".to_string()) {
                if i >= 1 {
                    let prev_lower = nw_text[i-1].to_lowercase();
                    if prev_lower == "the" || prev_lower == "these" || prev_lower == "those" {
                        if i + 1 < nw.len() {
                            let next_tags = &nw_has_tag[i+1];
                            if next_tags.iter().any(|t| t == "VBD" || t == "VBN") {
                                removals.push((i, "VBZ".to_string()));
                            }
                        }
                    }
                }
            }

            // Rule 3: After determiner (DT/PRP$), remove VBZ/VBP from noun-like words
            // e.g., "the dreams" → "dreams" should be NNS
            // But DON'T remove VB (needed for "a need" etc.)
            if i >= 1 {
                let prev_tags = &nw_has_tag[i-1];
                let prev_is_det = prev_tags.iter().any(|t| t == "DT" || t == "PRP$" || t == "PRP_S");
                if prev_is_det {
                    if tags.contains(&"VBZ".to_string()) && tags.iter().any(|t| t.starts_with("NN")) {
                        removals.push((i, "VBZ".to_string()));
                    }
                }
            }

            // Rule 4: After subject pronouns (I/we/you/they/he/she/it) or proper nouns,
            // remove IN from words that also have VB (like/love/want/need/hope/try)
            // e.g., "I like ice cream" → "like" should be VB, not IN
            if i >= 1 {
                let prev_tags = &nw_has_tag[i-1];
                let prev_is_subject = prev_tags.iter().any(|t| t == "PRP")
                    || prev_tags.iter().any(|t| t.starts_with("NNP"));
                if prev_is_subject {
                    if tags.contains(&"IN".to_string()) && tags.contains(&"VB".to_string()) {
                        // Check if it's a verb that can also be preposition (like, love, etc.)
                        let verb_preps = ["like", "love", "want", "need", "hope", "try"];
                        if verb_preps.contains(&lower.as_str()) {
                            removals.push((i, "IN".to_string()));
                        }
                    }
                }
            }

            // Rule 5: After "does/do/did + n't/not", remove IN from following verb-preps
            // e.g., "doesn't like" → "like" should be VB, not IN
            if i >= 2 {
                let prev2_lower = nw_text[i-2].to_lowercase();
                let prev1_lower = nw_text[i-1].to_lowercase();
                let is_do_nt = (prev2_lower == "does" || prev2_lower == "do" || prev2_lower == "did")
                    && (prev1_lower == "n't" || prev1_lower == "not");
                if is_do_nt {
                    if tags.contains(&"IN".to_string()) && tags.contains(&"VB".to_string()) {
                        removals.push((i, "IN".to_string()));
                    }
                }
            }

            // Rule 6: After modal (MD), remove IN from verb-preps
            // e.g., "will like" → "like" should be VB, not IN
            if i >= 1 {
                let prev_tags = &nw_has_tag[i-1];
                if prev_tags.iter().any(|t| t == "MD") {
                    if tags.contains(&"IN".to_string()) && tags.contains(&"VB".to_string()) {
                        removals.push((i, "IN".to_string()));
                    }
                }
            }

            // Rule 7: After "to" (IN/TO), remove IN from verb-preps
            // e.g., "to like" → "like" should be VB, not IN
            if i >= 1 {
                let prev_tags = &nw_has_tag[i-1];
                let prev_is_to = prev_tags.iter().any(|t| t == "TO" || t == "IN")
                    && nw_text[i-1].eq_ignore_ascii_case("to");
                if prev_is_to {
                    if tags.contains(&"IN".to_string()) && tags.contains(&"VB".to_string()) {
                        removals.push((i, "IN".to_string()));
                    }
                }
            }

            // Rule 8: After DT/PRP$ + singular NN, remove VBP/VBZ from following word if it has NN
            // e.g., "the bus stop" → "stop" should be NN, not VBP
            // But DON'T apply when prev is plural (NNS) — "The dogs barks" needs VBZ kept
            // Reduces SINGULAR_AGREEMENT_SENT_START false positives
            if i >= 2 {
                let prev2_tags = &nw_has_tag[i-2];
                let prev1_tags = &nw_has_tag[i-1];
                let prev2_is_det = prev2_tags.iter().any(|t| t == "DT" || t == "PRP$" || t == "PRP_S");
                let prev1_is_singular_noun = prev1_tags.iter().any(|t| t.starts_with("NN"))
                    && !prev1_tags.iter().any(|t| t == "NNS");
                if prev2_is_det && prev1_is_singular_noun {
                    if tags.iter().any(|t| t.starts_with("NN")) {
                        if tags.contains(&"VBP".to_string()) {
                            removals.push((i, "VBP".to_string()));
                        }
                        if tags.contains(&"VBZ".to_string()) {
                            removals.push((i, "VBZ".to_string()));
                        }
                    }
                }
            }

            // Rule 9: After DT + NN (directly adjacent), remove VBP/VBZ
            // e.g., "the dreams" → "dreams" should be NNS
            if i >= 1 {
                let prev_tags = &nw_has_tag[i-1];
                let prev_is_det = prev_tags.iter().any(|t| t == "DT" || t == "PRP$" || t == "PRP_S");
                if prev_is_det {
                    if tags.iter().any(|t| t.starts_with("NN")) {
                        if tags.contains(&"VBZ".to_string()) {
                            removals.push((i, "VBZ".to_string()));
                        }
                    }
                }
            }

            // Rule 10: After "have/has/had", prefer VBN over VBD for following word
            // e.g., "has become" → "become" should be VBN, not VBD
            // This prevents HAVE_PART_AGREEMENT from matching valid perfect tenses
            if i >= 1 {
                let prev_lower = nw_text[i-1].to_lowercase();
                let prev_is_have = (prev_lower == "have" || prev_lower == "has" || prev_lower == "had"
                    || prev_lower == "'ve" || prev_lower == "'s")
                    && nw_has_tag[i-1].iter().any(|t| t.starts_with("VB"));
                if prev_is_have {
                    // If word has VBN, remove VBD to prefer perfect tense reading
                    if tags.contains(&"VBN".to_string()) && tags.contains(&"VBD".to_string()) {
                        removals.push((i, "VBD".to_string()));
                    }
                }
            }

            // Rule 11: After "many/more/most/few/several/some", remove VBZ from NN words
            // e.g., "many dreams" → "dreams" should be NNS
            if i >= 1 {
                let prev_lower = nw_text[i-1].to_lowercase();
                let prev_is_quant = ["many", "more", "most", "few", "several", "some",
                    "these", "those", "various", "numerous"].contains(&prev_lower.as_str());
                if prev_is_quant {
                    if tags.iter().any(|t| t.starts_with("NN")) && tags.contains(&"VBZ".to_string()) {
                        removals.push((i, "VBZ".to_string()));
                    }
                }
            }

            // Rule 12: After PRP subject (he/she/it/they/we/I/you), remove NN/NNS from VB words
            // when followed by a non-verb. This helps "He walk" match HE_VERB_AGR.
            if i >= 1 && i + 1 < nw.len() {
                let prev_tags = &nw_has_tag[i-1];
                let prev_is_subject = prev_tags.iter().any(|t| t == "PRP");
                if prev_is_subject {
                    if tags.contains(&"VB".to_string()) && tags.iter().any(|t| t.starts_with("NN")) {
                        let next_tags = &nw_has_tag[i+1];
                        let next_is_verb = next_tags.iter().any(|t| t.starts_with("VB") || t.starts_with("MD"));
                        let next_is_det = next_tags.iter().any(|t| t == "DT" || t == "PRP$" || t == "IN");
                        if !next_is_verb && !next_is_det && nw_text[i+1] != "to" {
                            for nn_tag in ["NN", "NNS"].iter() {
                                if tags.contains(&nn_tag.to_string()) {
                                    removals.push((i, nn_tag.to_string()));
                                }
                            }
                        }
                    }
                }
            }

            // Rule 13: After "to" (TO tag), remove NN/NNS from following word if it has VB
            // e.g., "to walk" → "walk" should be VB, not NN
            if i >= 1 {
                let prev_tags = &nw_has_tag[i-1];
                let prev_is_to = prev_tags.iter().any(|t| t == "TO")
                    && nw_text[i-1].eq_ignore_ascii_case("to");
                if prev_is_to {
                    if tags.contains(&"VB".to_string()) {
                        for nn_tag in ["NN", "NNS"].iter() {
                            if tags.contains(&nn_tag.to_string()) {
                                removals.push((i, nn_tag.to_string()));
                            }
                        }
                    }
                }
            }

            // Rule 14: After modal (MD), remove NN/NNS from following word if it has VB
            // e.g., "will walk" → "walk" should be VB, not NN
            if i >= 1 {
                let prev_tags = &nw_has_tag[i-1];
                if prev_tags.iter().any(|t| t == "MD") {
                    if tags.contains(&"VB".to_string()) {
                        for nn_tag in ["NN", "NNS"].iter() {
                            if tags.contains(&nn_tag.to_string()) {
                                removals.push((i, nn_tag.to_string()));
                            }
                        }
                    }
                }
            }

            // Rule 15: After "have/has/had" + RB?, remove NN from VBN/VBD words
            // e.g., "has run" → "run" should be VBN, not NN
            if i >= 1 {
                let prev_lower = nw_text[i-1].to_lowercase();
                let prev_is_have = (prev_lower == "have" || prev_lower == "has" || prev_lower == "had"
                    || prev_lower == "'ve" || prev_lower == "'s")
                    && nw_has_tag[i-1].iter().any(|t| t.starts_with("VB"));
                let prev_is_have_or_rb = prev_is_have
                    || (i >= 2 && {
                        let pp_lower = nw_text[i-2].to_lowercase();
                        let pp_is_have = (pp_lower == "have" || pp_lower == "has" || pp_lower == "had"
                            || pp_lower == "'ve" || pp_lower == "'s")
                            && nw_has_tag[i-2].iter().any(|t| t.starts_with("VB"));
                        pp_is_have && nw_has_tag[i-1].iter().any(|t| t == "RB")
                    });
                if prev_is_have_or_rb {
                    if tags.contains(&"VBN".to_string()) || tags.contains(&"VBD".to_string()) {
                        for nn_tag in ["NN", "NNS"].iter() {
                            if tags.contains(&nn_tag.to_string()) {
                                removals.push((i, nn_tag.to_string()));
                            }
                        }
                    }
                }
            }

            // Rule 16: After "have/has/had", if word has JJ and VBD/VBN, and next word is a
            // noun/determiner, remove VBD/VBN to prefer JJ reading.
            // e.g., "have open positions" → "open" is JJ, not VBN
            if i >= 1 && i + 1 < nw.len() {
                let prev_lower = nw_text[i-1].to_lowercase();
                let prev_is_have = (prev_lower == "have" || prev_lower == "has" || prev_lower == "had"
                    || prev_lower == "'ve" || prev_lower == "'s")
                    && nw_has_tag[i-1].iter().any(|t| t.starts_with("VB"));
                if prev_is_have {
                    let has_jj = tags.contains(&"JJ".to_string());
                    let has_vbn = tags.contains(&"VBN".to_string());
                    let has_vbd = tags.contains(&"VBD".to_string());
                    if has_jj && (has_vbn || has_vbd) {
                        let next_tags = &nw_has_tag[i+1];
                        let next_is_noun_or_det = next_tags.iter().any(|t|
                            t.starts_with("NN") || t == "DT" || t == "PRP$" || t == "PRP_S");
                        if next_is_noun_or_det {
                            if has_vbn { removals.push((i, "VBN".to_string())); }
                            if has_vbd { removals.push((i, "VBD".to_string())); }
                        }
                    }
                }
            }

            // Rule 17: After "the/a/an" + JJ, remove VBZ/VBP from following word if it has NN
            // e.g., "the live music" → "music" should be NN, not VBP
            if i >= 2 {
                let prev2_tags = &nw_has_tag[i-2];
                let prev1_tags = &nw_has_tag[i-1];
                let prev2_is_det = prev2_tags.iter().any(|t| t == "DT");
                let prev1_is_adj = prev1_tags.iter().any(|t| t.starts_with("JJ"));
                if prev2_is_det && prev1_is_adj {
                    if tags.iter().any(|t| t.starts_with("NN")) {
                        if tags.contains(&"VBZ".to_string()) {
                            removals.push((i, "VBZ".to_string()));
                        }
                        if tags.contains(&"VBP".to_string()) {
                            removals.push((i, "VBP".to_string()));
                        }
                    }
                }
            }

            // Rule 19: Before a determiner (DT), remove JJR/JJS from nouns
            // "the better results" → "better" is JJR, "results" is NNS not JJR
            if i + 1 < nw.len() {
                let next_tags = &nw_has_tag[i+1];
                let next_is_det = next_tags.iter().any(|t| t == "DT");
                if next_is_det && tags.iter().any(|t| t.starts_with("NN")) {
                    if tags.contains(&"JJR".to_string()) {
                        removals.push((i, "JJR".to_string()));
                    }
                    if tags.contains(&"JJS".to_string()) {
                        removals.push((i, "JJS".to_string()));
                    }
                }
            }

            // Rule 20: After IN (preposition), if word has NN and VB, prefer NN
            // "in the park" → "park" is NN, not VB
            if i >= 2 && i < nw.len() {
                let prev2_tags = &nw_has_tag[i-2];
                let prev1_tags = &nw_has_tag[i-1];
                let prev2_is_in = prev2_tags.iter().any(|t| t == "IN");
                let prev1_is_det = prev1_tags.iter().any(|t| t == "DT" || t == "PRP$");
                if prev2_is_in && prev1_is_det {
                    if tags.iter().any(|t| t.starts_with("NN")) {
                        removals.push((i, "VB".to_string()));
                        removals.push((i, "VBP".to_string()));
                        removals.push((i, "VBZ".to_string()));
                    }
                }
            }

            // Rule 21: After "a/an" (DT), remove VB from words that also have NN
            // "a need" → "need" is NN, not VB. Fixes A_INFINITIVE FPs.
            if i >= 1 {
                let prev_tags = &nw_has_tag[i-1];
                let prev_text_lower = nw_text[i-1].to_lowercase();
                let prev_is_a_an = prev_text_lower == "a" || prev_text_lower == "an";
                if prev_is_a_an && prev_tags.iter().any(|t| t == "DT") {
                    if tags.iter().any(|t| t.starts_with("NN")) {
                        if tags.contains(&"VB".to_string()) {
                            removals.push((i, "VB".to_string()));
                        }
                        if tags.contains(&"VBP".to_string()) {
                            removals.push((i, "VBP".to_string()));
                        }
                    }
                }
            }

            // Rule 22: After "many/much/more/most/few/so/as/how/too" + "a/an", remove VB from NN words
            // This helps MUCH_COUNTABLE, MANY_NN FPs
            // Already handled by existing rules partially

            // Rule 23: At sentence start (after SENT_START), if word has VB and NN,
            // prefer NN for words that are commonly nouns.
            // "Animals can..." → "Animals" is NNS, not VB
            // But "Walk the dog" → "Walk" should be VB
            // So only apply if next word is NOT a verb/modifier
            if i == 0 && tags.contains(&"VB".to_string()) && tags.iter().any(|t| t.starts_with("NN")) {
                if nw.len() > 1 {
                    let next_tags = &nw_has_tag[1];
                    // If next word is DT, IN, or noun, current word is likely a noun
                    let next_suggests_noun_subject = next_tags.iter().any(|t|
                        t == "DT" || t == "IN" || t.starts_with("NN") || t == "CC" || t == "MD");
                    if next_suggests_noun_subject {
                        removals.push((i, "VB".to_string()));
                        removals.push((i, "VBP".to_string()));
                    }
                }
            }

            // Rule 24: After sentence-end period + SENT_START (start of new sentence),
            // if word has VB and NN, and next word is a verb, prefer VB.
            // "People think..." → "People" could be NNS or VBZ, but with VB think next, prefer NNS.
            // Actually handled above by Rule 23.
            // Instead: if current word has VBZ and NN, and prev is a verb/comma, prefer NN.
            // "He knows people are..." → "people" should be NNS not VBZ

            // Rule 25: After comma, if word has VBZ/VBP and NN, prefer NN
            // "In winter, animals migrate" → "animals" is NNS
            if i >= 1 {
                let prev_text_lower = nw_text[i-1].to_lowercase();
                if prev_text_lower == "," || prev_text_lower == ":" || prev_text_lower == ";" {
                    if tags.iter().any(|t| t.starts_with("NN")) {
                        if tags.contains(&"VBZ".to_string()) {
                            removals.push((i, "VBZ".to_string()));
                        }
                        if tags.contains(&"VBP".to_string()) {
                            removals.push((i, "VBP".to_string()));
                        }
                    }
                }
            }

            // Rule 26: After "the/these/those" + JJ, remove VB from following word if NN
            // "the open position" → "position" is NN, not VB
            if i >= 2 {
                let prev2_text = nw_text[i-2].to_lowercase();
                let prev2_is_det = ["the", "these", "those", "this", "that", "some", "any", "every"].contains(&prev2_text.as_str());
                let prev1_is_adj = nw_has_tag[i-1].iter().any(|t| t.starts_with("JJ"));
                if prev2_is_det && prev1_is_adj {
                    if tags.iter().any(|t| t.starts_with("NN")) {
                        removals.push((i, "VB".to_string()));
                        removals.push((i, "VBP".to_string()));
                        removals.push((i, "VBZ".to_string()));
                    }
                }
            }

            // Rule 27: Before "to" (TO tag), remove VB from words that also have NN
            // "key to success" → "key" is NN, not VB/JJ
            // But don't apply after subject pronouns: "I go to school" → "go" is VB
            if i + 1 < nw.len() {
                let next_text_lower = nw_text[i+1].to_lowercase();
                let next_is_to = next_text_lower == "to" && nw_has_tag[i+1].iter().any(|t| t == "TO");
                if next_is_to {
                    let prev_is_subject = i >= 1 && nw_has_tag[i-1].iter().any(|t|
                        t == "PRP" || t.starts_with("PRP_S") || t.starts_with("PRP_O"));
                    if !prev_is_subject {
                        if tags.iter().any(|t| t.starts_with("NN")) {
                            removals.push((i, "VB".to_string()));
                            removals.push((i, "VBP".to_string()));
                        }
                    }
                }
            }

            // Rule 28: Before CC (and/or/but) + noun, remove VB from words that have NN
            // "time and effort" → "time" is NN, not VB
            if i + 2 < nw.len() {
                let next_is_cc = nw_has_tag[i+1].iter().any(|t| t == "CC");
                let next2_is_nn = nw_has_tag[i+2].iter().any(|t| t.starts_with("NN"));
                if next_is_cc && next2_is_nn {
                    if tags.iter().any(|t| t.starts_with("NN")) {
                        removals.push((i, "VB".to_string()));
                        removals.push((i, "VBP".to_string()));
                        removals.push((i, "VBZ".to_string()));
                    }
                }
            }

        }

        // Apply removals
        if removals.is_empty() { return; }

        let tokens_mut = sentence.tokens_mut();
        for (nw_idx, tag_to_remove) in &removals {
            if let Some(&real_idx) = nw.get(*nw_idx) {
                let token = &mut tokens_mut[real_idx];
                for reading in token.readings_mut() {
                    reading.set_pos_tags(
                        reading.pos_tags().iter()
                            .filter(|t| *t != tag_to_remove)
                            .cloned()
                            .collect()
                    );
                }
                let base = token.token_mut();
                base.set_pos_tags(
                    base.pos_tags().iter()
                        .filter(|t| *t != tag_to_remove)
                        .cloned()
                        .collect()
                );
            }
        }
    }

    /// Apply chunk tags to tokens based on their POS tags.
    /// Operates only on non-whitespace tokens to avoid whitespace breaking up chunks.
    fn apply_chunking(&self, sentence: &mut AnalyzedSentence) {
        let tokens = sentence.tokens();

        // Build index mapping: non-whitespace token positions
        let mut nw_indices: Vec<usize> = Vec::new();
        let mut all_tags: Vec<Vec<String>> = Vec::new(); // all POS tags per token
        let mut token_texts: Vec<&str> = Vec::new();

        for (i, t) in tokens.iter().enumerate() {
            if t.is_whitespace() { continue; }
            let tags = t.token().pos_tags();
            if tags.iter().any(|t| t == "SENT_START" || t == "SENT_END") { continue; }
            nw_indices.push(i);
            all_tags.push(tags.to_vec());
            token_texts.push(t.token().token());
        }

        // Select best POS tag per token using context heuristics
        let pos_tags: Vec<&str> = all_tags.iter().enumerate().map(|(i, tags)| {
            if tags.is_empty() { return ""; }
            if tags.len() == 1 { return tags[0].as_str(); }

            let has_vb = tags.iter().any(|t| t.starts_with("VB"));
            let has_jj = tags.iter().any(|t| t.starts_with("JJ"));
            let has_nn = tags.iter().any(|t| t.starts_with("NN") || t == "FW");

            // Look back to find the effective previous verb context
            // Skip over negation (n't/not/RB) to find the real preceding POS
            let prev_is_verb = if i > 0 {
                all_tags[i - 1].iter().any(|t| t.starts_with("VB") || t == "MD")
            } else {
                false
            };
            // After "do n't" / "does n't" etc, the negation RB is prev, but verb is 2 back
            let prev_2_is_verb = if i > 1 {
                let prev_text = token_texts.get(i - 1).map(|s| *s).unwrap_or("");
                let is_negation = matches!(prev_text.to_lowercase().as_str(), "n't" | "not" | "never")
                    || all_tags[i - 1].iter().any(|t| t == "RB");
                if is_negation {
                    all_tags[i - 2].iter().any(|t| t.starts_with("VB") || t == "MD")
                } else {
                    false
                }
            } else {
                false
            };
            let effective_prev_is_verb = prev_is_verb || prev_2_is_verb;

            let prev_is_det = if i > 0 {
                all_tags[i - 1].iter().any(|t| t == "DT" || t == "PRP$" || t == "CD" || t == "WDT")
            } else {
                false
            };

            // After "have/has/had", prefer adjective/noun over verb for ambiguous words
            // (e.g., "has open positions" → open should be JJ, not VB)
            // But ONLY when have is immediately before (not separated by not/n't)
            // "have not getting" → getting should stay VBG, not become NN
            let prev_is_have = if i > 0 {
                let prev_text = token_texts.get(i - 1).map(|s| *s).unwrap_or("");
                prev_is_verb && matches!(prev_text.to_lowercase().as_str(),
                    "have" | "has" | "had" | "'ve" | "'s")
            } else {
                false
            };
            // Don't apply have-heuristic when separated by negation
            let effective_prev_is_have = prev_is_have;

            if effective_prev_is_have && has_jj {
                return tags.iter().find(|t| t.starts_with("JJ")).map(|s| s.as_str()).unwrap_or(tags[0].as_str());
            }
            if effective_prev_is_have && has_nn {
                return tags.iter().find(|t| t.starts_with("NN")).map(|s| s.as_str()).unwrap_or(tags[0].as_str());
            }

            // After a determiner, prefer noun tags over verb tags
            if prev_is_det && has_nn && has_vb {
                return tags.iter().find(|t| t.starts_with("NN")).map(|s| s.as_str()).unwrap_or(tags[0].as_str());
            }

            // After a verb (including after "do n't"), prefer verb tags over noun tags
            // But ONLY for specific verbs that commonly take bare infinitives
            // E.g., "want test" → test should be VB not NN
            // E.g., "don't want" → want should be VB not NN:UN
            if effective_prev_is_verb && has_vb && has_nn && !effective_prev_is_have {
                return tags.iter().find(|t| t.starts_with("VB")).map(|s| s.as_str()).unwrap_or(tags[0].as_str());
            }

            // After a verb (including negated), prefer VB over JJ for ambiguous words
            // E.g., "would like" → like should be VB not JJ
            // But NOT after have/has/had (where adj is more likely, e.g., "have hard copy")
            if effective_prev_is_verb && has_vb && has_jj && !effective_prev_is_have {
                return tags.iter().find(|t| t.starts_with("VB")).map(|s| s.as_str()).unwrap_or(tags[0].as_str());
            }

            // After a subject pronoun (PRP), prefer verb/modals over noun tags
            // E.g., "We want" → want should be VB not NN:UN
            // E.g., "We would" → would should be MD not NN
            let prev_is_prp = if i > 0 {
                all_tags[i - 1].iter().any(|t| t == "PRP" || t.starts_with("PRP_S") || t.starts_with("PRP_O"))
            } else {
                false
            };
            if prev_is_prp {
                let has_md = tags.iter().any(|t| t == "MD");
                if has_vb && has_nn {
                    return tags.iter().find(|t| t.starts_with("VB")).map(|s| s.as_str()).unwrap_or(tags[0].as_str());
                }
                if has_md && has_nn {
                    return "MD";
                }
            }

            tags[0].as_str()
        }).collect();

        // Prepend a fake SENT_START for the chunker
        let mut all_pos = vec!["SENT_START"];
        let mut all_texts = vec!["<S>"];
        all_pos.extend(pos_tags.iter().map(|s| *s));
        all_texts.extend(token_texts.iter().map(|s| *s));

        let chunks = og_tagger::chunker::chunk_tokens(&all_pos, &all_texts);

        // Map chunks back to original token indices (skip index 0 = SENT_START)
        let tokens = sentence.tokens_mut();
        for (chunk_idx, orig_idx) in nw_indices.iter().enumerate() {
            let chunk_array_idx = chunk_idx + 1; // +1 for SENT_START
            if let Some(Some(chunk_tag)) = chunks.get(chunk_array_idx) {
                tokens[*orig_idx].set_chunk(Some(chunk_tag.clone()));
            }
        }
    }

    /// Strip <marker> tags from example text and return (clean_text, marker_start, marker_end)
    fn extract_marker(text: &str) -> (String, Option<(usize, usize)>) {
        let start_marker = "<marker>";
        let end_marker = "</marker>";

        let start_idx = text.find(start_marker);
        let end_idx = text.find(end_marker);

        match (start_idx, end_idx) {
            (Some(s), Some(e)) => {
                let marker_start = s;
                let marker_end = e - start_marker.len();
                let clean = text[..s].to_string() + &text[s + start_marker.len()..e] + &text[e + end_marker.len()..];
                (clean, Some((marker_start, marker_end)))
            }
            _ => (text.to_string(), None),
        }
    }

    /// Run all example tests from an XML grammar file
    pub fn run_xml_example_tests(&self, xml: &str) -> TestRunSummary {
        let mut results = Vec::new();
        let compiler = XmlCompiler::new();

        let rule_set = match compiler.compile_file(xml) {
            Ok(rs) => rs,
            Err(e) => {
                results.push(ExampleTestResult {
                    rule_id: "PARSE_ERROR".to_string(),
                    passed: false,
                    message: format!("Failed to parse XML: {}", e),
                    example_text: String::new(),
                    expected_type: "parse".to_string(),
                });
                return TestRunSummary {
                    total: 1,
                    passed: 0,
                    failed: 1,
                    results,
                };
            }
        };

        for rule in &rule_set.rules {
            for example in &rule.examples {
                let (clean_text, marker_range) = Self::extract_marker(&example.text);

                let sentence = self.tokenize_sentence(&clean_text);
                let matches = self.pattern_engine.match_rule(rule, &sentence);

                match example.example_type {
                    og_xml::types::XmlExampleType::Incorrect => {
                        if matches.is_empty() {
                            results.push(ExampleTestResult {
                                rule_id: rule.id.clone(),
                                passed: false,
                                message: format!(
                                    "Expected match on incorrect example but got none: '{}'",
                                    clean_text
                                ),
                                example_text: clean_text,
                                expected_type: "incorrect".to_string(),
                            });
                        } else if let Some((m_start, m_end)) = marker_range {
                            // Verify that the marker is within or overlaps the match
                            let first_match = &matches[0];
                            let match_start = first_match.offset();
                            let match_end = match_start + first_match.length();
                            // Match should contain the marker
                            let marker_in_match = match_start <= m_start && match_end >= m_end;
                            if marker_in_match {
                                results.push(ExampleTestResult {
                                    rule_id: rule.id.clone(),
                                    passed: true,
                                    message: format!("Match {}..{} contains marker {}..{}", match_start, match_end, m_start, m_end),
                                    example_text: clean_text,
                                    expected_type: "incorrect".to_string(),
                                });
                            } else {
                                results.push(ExampleTestResult {
                                    rule_id: rule.id.clone(),
                                    passed: false,
                                    message: format!(
                                        "Match {}..{} does not contain marker {}..{}",
                                        match_start, match_end, m_start, m_end
                                    ),
                                    example_text: clean_text,
                                    expected_type: "incorrect".to_string(),
                                });
                            }
                        } else {
                            results.push(ExampleTestResult {
                                rule_id: rule.id.clone(),
                                passed: true,
                                message: format!("Matched correctly: '{}'", clean_text),
                                example_text: clean_text,
                                expected_type: "incorrect".to_string(),
                            });
                        }
                    }
                    og_xml::types::XmlExampleType::Correct => {
                        if !matches.is_empty() {
                            results.push(ExampleTestResult {
                                rule_id: rule.id.clone(),
                                passed: false,
                                message: format!(
                                    "Expected no match on correct example but got {}: '{}'",
                                    matches.len(), clean_text
                                ),
                                example_text: clean_text,
                                expected_type: "correct".to_string(),
                            });
                        } else {
                            results.push(ExampleTestResult {
                                rule_id: rule.id.clone(),
                                passed: true,
                                message: format!("No match on correct example: '{}'", clean_text),
                                example_text: clean_text,
                                expected_type: "correct".to_string(),
                            });
                        }
                    }
                }
            }
        }

        let passed = results.iter().filter(|r| r.passed).count();
        let failed = results.len() - passed;
        TestRunSummary {
            total: results.len(),
            passed,
            failed,
            results,
        }
    }

    /// Run example tests on an XML file from disk
    pub fn run_xml_file_tests(&self, path: &Path) -> Result<TestRunSummary, String> {
        let xml = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        Ok(self.run_xml_example_tests(&xml))
    }

    /// Run a single pattern test
    pub fn run_pattern_test(
        &self,
        rule_id: &str,
        xml: &str,
        sentence_text: &str,
        expected_match: bool,
    ) -> ExampleTestResult {
        let compiler = XmlCompiler::new();
        let rule_set = match compiler.compile_file(xml) {
            Ok(rs) => rs,
            Err(e) => {
                return ExampleTestResult {
                    rule_id: rule_id.to_string(),
                    passed: false,
                    message: format!("Parse error: {}", e),
                    example_text: sentence_text.to_string(),
                    expected_type: "single".to_string(),
                };
            }
        };

        let rule = match rule_set.rules.iter().find(|r| r.id == rule_id) {
            Some(r) => r,
            None => {
                return ExampleTestResult {
                    rule_id: rule_id.to_string(),
                    passed: false,
                    message: format!("Rule '{}' not found", rule_id),
                    example_text: sentence_text.to_string(),
                    expected_type: "single".to_string(),
                };
            }
        };

        let sentence = self.tokenize_sentence(sentence_text);
        let matches = self.pattern_engine.match_rule(rule, &sentence);

        let matched = !matches.is_empty();
        ExampleTestResult {
            rule_id: rule_id.to_string(),
            passed: matched == expected_match,
            message: if matched == expected_match {
                format!("OK: '{}'", sentence_text)
            } else {
                format!(
                    "FAIL: '{}' - expected {}, got {}",
                    sentence_text,
                    if expected_match { "match" } else { "no match" },
                    if matched { "match" } else { "no match" }
                )
            },
            example_text: sentence_text.to_string(),
            expected_type: "single".to_string(),
        }
    }
}

impl Default for TestRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_xml(rules_body: &str) -> String {
        format!(r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                {}
            </category>
        </rules>"#, rules_body)
    }

    #[test]
    fn test_runner_incorrect_example_passes() {
        let runner = TestRunner::new();
        let xml = simple_xml(r#"
            <rule id="TEST1" name="Test">
                <pattern>
                    <token>their</token>
                    <token>is</token>
                </pattern>
                <message>Did you mean <suggestion>there is</suggestion>?</message>
                <example correction="there is">Their <marker>is</marker> a problem.</example>
                <example>There is no problem.</example>
            </rule>
        "#);

        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 2);
        assert_eq!(summary.passed, 2, "Failed: {:?}", summary.results.iter().filter(|r| !r.passed).collect::<Vec<_>>());
    }

    #[test]
    fn test_runner_correct_example_no_match() {
        let runner = TestRunner::new();
        let xml = simple_xml(r#"
            <rule id="TEST2" name="Test">
                <pattern>
                    <token>xyzzy</token>
                </pattern>
                <message>This should not match.</message>
                <example>This is a normal sentence.</example>
            </rule>
        "#);

        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 1);
        assert!(summary.is_all_passed());
    }

    #[test]
    fn test_runner_regexp_rule() {
        let runner = TestRunner::new();
        let xml = simple_xml(r#"
            <rule id="TEST3" name="Test">
                <pattern>
                    <token regexp="yes">teh</token>
                </pattern>
                <message>Did you mean <suggestion>the</suggestion>?</message>
                <example correction="the"><marker>Teh</marker> quick brown fox.</example>
                <example>The quick brown fox.</example>
            </rule>
        "#);

        let summary = runner.run_xml_example_tests(&xml);
        // The regexp rule should match "Teh"
        for r in &summary.results {
            if !r.passed {
                eprintln!("FAILED: {} - {}", r.rule_id, r.message);
            }
        }
        // At minimum, the correct example should pass (no match)
        assert!(summary.results.iter().any(|r| r.passed && r.expected_type == "correct"));
    }

    #[test]
    fn test_runner_multiple_rules() {
        let runner = TestRunner::new();
        let xml = simple_xml(r#"
            <rule id="RULE_A" name="Rule A">
                <pattern>
                    <token>hello</token>
                    <token>world</token>
                </pattern>
                <message>Hi!</message>
                <example><marker>Hello world</marker> test.</example>
                <example>Hi there.</example>
            </rule>
            <rule id="RULE_B" name="Rule B">
                <pattern>
                    <token>goodbye</token>
                    <token>world</token>
                </pattern>
                <message>Bye!</message>
                <example><marker>Goodbye world</marker> test.</example>
                <example>See you.</example>
            </rule>
        "#);

        let summary = runner.run_xml_example_tests(&xml);
        assert_eq!(summary.total, 4, "Should have 4 examples");
    }

    #[test]
    fn test_runner_marker_extraction() {
        let (clean, range) = TestRunner::extract_marker("Hello <marker>world</marker> test");
        assert_eq!(clean, "Hello world test");
        assert_eq!(range, Some((6, 11)));
    }

    #[test]
    fn test_runner_no_marker() {
        let (clean, range) = TestRunner::extract_marker("Hello world test");
        assert_eq!(clean, "Hello world test");
        assert!(range.is_none());
    }

    #[test]
    fn test_runner_single_pattern_test() {
        let runner = TestRunner::new();
        let xml = simple_xml(r#"
            <rule id="SINGLE_TEST" name="Single">
                <pattern>
                    <token>foo</token>
                    <token>bar</token>
                </pattern>
                <message>Test</message>
            </rule>
        "#);

        let result = runner.run_pattern_test("SINGLE_TEST", &xml, "foo bar baz", true);
        assert!(result.passed, "{}", result.message);

        let result = runner.run_pattern_test("SINGLE_TEST", &xml, "baz qux", false);
        assert!(result.passed, "{}", result.message);
    }

    #[test]
    fn test_runner_xml_parse_error() {
        let runner = TestRunner::new();
        // Use XML that's structurally broken to trigger parse error
        let xml = r#"<?xml version="1.0"?><rules><category id="X"><rule id="TEST"><pattern>"#;
        // This should still parse (quick-xml is lenient) or produce empty rules
        let summary = runner.run_xml_example_tests(xml);
        // Either we get a parse error or empty results - both are acceptable
        assert!(summary.total <= 1);
    }

    #[test]
    fn test_runner_rule_not_found() {
        let runner = TestRunner::new();
        let xml = simple_xml(r#"
            <rule id="EXISTS" name="Exists">
                <pattern><token>test</token></pattern>
                <message>Test</message>
            </rule>
        "#);

        let result = runner.run_pattern_test("NONEXISTENT", &xml, "test", true);
        assert!(!result.passed);
        assert!(result.message.contains("not found"));
    }

    #[test]
    fn test_runner_negate_token() {
        let runner = TestRunner::new();
        let xml = simple_xml(r#"
            <rule id="NEG_TEST" name="Negate">
                <pattern>
                    <token negate="yes">good</token>
                    <token>morning</token>
                </pattern>
                <message>Not good morning.</message>
                <example><marker>Bad morning</marker> today.</example>
                <example>Good morning everyone.</example>
            </rule>
        "#);

        let summary = runner.run_xml_example_tests(&xml);
        for r in &summary.results {
            if !r.passed {
                eprintln!("FAILED: {} ({}) - {}", r.rule_id, r.expected_type, r.message);
            }
        }
        assert!(summary.is_all_passed(), "Negate token test failed");
    }

    #[test]
    fn test_load_real_english_grammar_xml() {
        let grammar_path = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/grammar.xml";
        if !Path::new(grammar_path).exists() {
            eprintln!("Skipping: English grammar.xml not found at {}", grammar_path);
            return;
        }

        let xml = std::fs::read_to_string(grammar_path).expect("Failed to read grammar.xml");
        let compiler = og_xml::compiler::XmlCompiler::new();
        let rule_set = compiler.compile_file(&xml).expect("Failed to compile grammar.xml");

        let total = rule_set.rules.len();
        eprintln!("Loaded {} rules from English grammar.xml", total);

        // We expect thousands of rules
        assert!(total > 4000, "Expected 4000+ rules, got {}", total);

        // Count rules with various features
        let with_antipatterns = rule_set.rules.iter().filter(|r| !r.antipatterns.is_empty()).count();
        let with_marker = rule_set.rules.iter().filter(|r| r.pattern.marker_start.is_some()).count();
        let with_exceptions = rule_set.rules.iter()
            .filter(|r| r.pattern.tokens.iter().any(|t| !t.exceptions.is_empty())).count();
        let with_skip = rule_set.rules.iter()
            .filter(|r| r.pattern.tokens.iter().any(|t| t.skip != 0)).count();
        let with_min_max = rule_set.rules.iter()
            .filter(|r| r.pattern.tokens.iter().any(|t| t.min.is_some() || t.max.is_some())).count();
        let with_postag = rule_set.rules.iter()
            .filter(|r| r.pattern.tokens.iter().any(|t| t.postag.is_some())).count();

        eprintln!("Rules with antipatterns: {}", with_antipatterns);
        eprintln!("Rules with pattern markers: {}", with_marker);
        eprintln!("Rules with exceptions: {}", with_exceptions);
        eprintln!("Rules with skip: {}", with_skip);
        eprintln!("Rules with min/max: {}", with_min_max);
        eprintln!("Rules with postag: {}", with_postag);

        // Sanity checks on feature counts
        assert!(with_antipatterns > 1000, "Expected 1000+ rules with antipatterns, got {}", with_antipatterns);
        assert!(with_exceptions > 500, "Expected 500+ rules with exceptions, got {}", with_exceptions);
        assert!(with_skip > 100, "Expected 100+ rules with skip, got {}", with_skip);
    }

    #[test]
    fn test_real_english_grammar_examples() {
        let grammar_path = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/grammar.xml";
        if !Path::new(grammar_path).exists() {
            eprintln!("Skipping: English grammar.xml not found");
            return;
        }

        let xml = std::fs::read_to_string(grammar_path).expect("Failed to read grammar.xml");
        let runner = TestRunner::new();
        let summary = runner.run_xml_example_tests(&xml);

        eprintln!("Total examples tested: {}", summary.total);
        eprintln!("Passed: {}", summary.passed);
        eprintln!("Failed: {}", summary.failed);
        eprintln!("Pass rate: {:.1}%", if summary.total > 0 { 100.0 * summary.passed as f64 / summary.total as f64 } else { 0.0 });

        // Categorize failures
        let mut fail_by_reason: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for r in &summary.results {
            if !r.passed {
                let reason = if r.message.contains("POS") { "pos_tag".to_string() }
                    else if r.message.contains("match") { "no_match".to_string() }
                    else if r.message.contains("marker") { "marker_mismatch".to_string() }
                    else { "other".to_string() };
                *fail_by_reason.entry(reason).or_insert(0) += 1;
            }
        }
        eprintln!("\nFailure categories:");
        for (reason, count) in &fail_by_reason {
            eprintln!("  {}: {}", reason, count);
        }

        // Print top failing rule IDs by count
        let mut rule_fail_counts: std::collections::HashMap<String, (usize, usize, usize)> = std::collections::HashMap::new();
        for r in &summary.results {
            if !r.passed {
                let entry = rule_fail_counts.entry(r.rule_id.clone()).or_insert((0, 0, 0));
                if r.expected_type == "incorrect" { entry.0 += 1; }
                else { entry.1 += 1; }
                if r.message.contains("marker") { entry.2 += 1; }
            }
        }
        let mut sorted_fails: Vec<_> = rule_fail_counts.iter().collect();
        sorted_fails.sort_by(|a, b| (b.1.0 + b.1.1).cmp(&(a.1.0 + a.1.1)));
        eprintln!("\nTop 30 failing rules:");
        for (rule_id, (incorrect, correct, marker)) in sorted_fails.iter().take(30) {
            eprintln!("  {}: {} no-match, {} false-positive, {} marker-mismatch", rule_id, incorrect, correct, marker);
        }

        // Print some failing examples for debugging
        let failures: Vec<_> = summary.results.iter().filter(|r| !r.passed).take(100).collect();
        for f in &failures {
            eprintln!("FAIL: [{}] {} - {}", f.rule_id, f.expected_type, f.message);
        }

        // Save all failures to file for analysis
        if let Ok(all_failures) = std::fs::File::create("/tmp/test_failures_all.txt") {
            use std::io::Write;
            let mut out = std::io::BufWriter::new(all_failures);
            for r in &summary.results {
                if !r.passed {
                    writeln!(out, "FAIL: [{}] {} - {}", r.rule_id, r.expected_type, r.message).ok();
                }
            }
        }

        // Show breakdown of failures by expected type
        let incorrect_no_match = summary.results.iter()
            .filter(|r| !r.passed && r.expected_type == "incorrect" && r.message.contains("Expected match"))
            .count();
        let correct_unexpected = summary.results.iter()
            .filter(|r| !r.passed && r.expected_type == "correct")
            .count();
        let incorrect_marker = summary.results.iter()
            .filter(|r| !r.passed && r.expected_type == "incorrect" && r.message.contains("does not contain marker"))
            .count();
        eprintln!("\nFailure breakdown:");
        eprintln!("  Incorrect (no match): {}", incorrect_no_match);
        eprintln!("  Correct (unexpected match): {}", correct_unexpected);
        eprintln!("  Incorrect (marker mismatch): {}", incorrect_marker);

        // We don't require 100% pass rate yet (many rules need POS tags, filters, etc.)
        // But we should have a reasonable pass rate
        let pass_rate = if summary.total > 0 { summary.passed as f64 / summary.total as f64 } else { 0.0 };
        assert!(pass_rate > 0.10, "Expected at least 10% pass rate, got {:.1}%", pass_rate * 100.0);
    }

    #[test]
    fn test_it_is_rule_its_cool_man() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rules lang="en">
            <category id="GRAMMAR" name="Grammar">
                <rulegroup id="IT_IS" name="Test">
                    <rule>
                        <pattern>
                            <marker>
                                <token>its</token>
                            </marker>
                            <token postag="JJ">
                                <exception regexp="yes">first|second|third|last</exception>
                            </token>
                            <token>man</token>
                        </pattern>
                        <message>Did you mean <suggestion>it's</suggestion>?</message>
                        <example correction="It's"><marker>Its</marker> cool man!</example>
                    </rule>
                </rulegroup>
            </category>
        </rules>"#;

        let runner = TestRunner::new();
        let summary = runner.run_xml_example_tests(xml);

        // Debug: print token POS tags
        let sentence = runner.tokenize_sentence("Its cool man!");
        let nwt: Vec<_> = sentence.non_whitespace_tokens();
        eprintln!("Non-whitespace tokens for 'Its cool man!':");
        for (i, token) in nwt.iter().enumerate() {
            eprintln!("  [{}] '{}' pos={:?} has_JJ={}",
                i, token.token().token(),
                token.token().pos_tags(),
                token.has_pos_tag("JJ"));
        }

        // Try matching manually
        let compiler = og_xml::compiler::XmlCompiler::new();
        let rule_set = compiler.compile_file(xml).unwrap();
        eprintln!("Compiled {} rules", rule_set.rules.len());
        for rule in &rule_set.rules {
            eprintln!("Rule {} has {} pattern tokens", rule.id, rule.pattern.tokens.len());
            for (i, t) in rule.pattern.tokens.iter().enumerate() {
                eprintln!("  token[{}]: text={:?} postag={:?} negate={}", i, t.text, t.postag, t.negate);
            }
            let matches = runner.pattern_engine.match_rule(rule, &sentence);
            eprintln!("  matches: {} matches found", matches.len());
        }

        for r in &summary.results {
            if !r.passed {
                eprintln!("FAILED: {}", r.message);
            }
        }
        assert!(summary.is_all_passed(), "IT_IS[10] rule should match 'Its cool man!'");
    }

    #[test]
    fn test_debug_a_nns_fps() {
        let runner = TestRunner::new();
        let sentences = vec![
            "I want spend time with my friends.",
            "I would like spend time with my friends.",
        ];

        for text in &sentences {
            let sentence = runner.tokenize_sentence(text);
            let nwt: Vec<_> = sentence.non_whitespace_tokens();
            eprintln!("\n'{}':", text);
            for (i, token) in nwt.iter().enumerate() {
                let chunk = token.chunk().unwrap_or("-");
                eprintln!("  [{}] {:15} pos={:?} chunk={}",
                    i, format!("'{}'", token.token().token()),
                    token.token().pos_tags(), chunk);
            }
        }
    }

    #[test]
    fn test_debug_pos_tags_for_failures() {
        let runner = TestRunner::new();
        let sentences = vec![
            "I want spend time with my friends.",
            "Can I tell if its using the repeater?",
            "He is friend.",
            "We have visited the client on 7 October 2025.",
            "I would love listen your contribution.",
            "He never looks the front door.",
            "This sound slike an error.",
            "We want test it.",
            "But its much better like this.",
            "I think its much better now.",
            "Its much easier.",
            "Let me know when its under pressure.",
            "It's to early.",
            "He spent 10% to much.",
            "I better stop before I go to far.",
            "The dogs barks loudly.",
            "I would like see us make this work.",
            "They don't want let me learn.",
            "I want spend time with my friends.",
            "Restart PHP and try replicate your issue.",
        ];

        for text in &sentences {
            let sentence = runner.tokenize_sentence(text);
            let nwt: Vec<_> = sentence.non_whitespace_tokens();
            eprintln!("\n'{}':", text);
            for (i, token) in nwt.iter().enumerate() {
                let chunk = token.chunk().unwrap_or("-");
                eprintln!("  [{}] {:15} pos={:?} chunk={}",
                    i, format!("'{}'", token.token().token()),
                    token.token().pos_tags(), chunk);
            }
        }
    }

    /// Diagnostic test that checks POS tags for specific words commonly needed by
    #[test]
    fn test_been_part_agreement_debug() {
        let runner = &TestRunner::new();

        // Test: "I'm been prepared." should trigger BEEN_PART_AGREEMENT
        let sentence = runner.tokenize_sentence("I'm been prepared.");
        let nwt: Vec<_> = sentence.non_whitespace_tokens();

        // Load grammar rules and try matching
        let grammar_path = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/grammar.xml";
        let xml = std::fs::read_to_string(grammar_path).unwrap();
        let compiled_rule_set = og_xml::compiler::XmlCompiler::new().compile_file(&xml).unwrap();

        // Debug TO_TOO matching
        let to_too_rules: Vec<_> = compiled_rule_set.rules.iter()
            .filter(|r| r.id == "TO_TOO")
            .collect();
        eprintln!("\nTO_TOO rules found: {}", to_too_rules.len());
        for (i, rule) in to_too_rules.iter().enumerate() {
            eprintln!("  Rule {}: {} pattern tokens", i, rule.pattern.tokens.len());
            let sentence2 = runner.tokenize_sentence("It's to early.");
            let matches = runner.pattern_engine.match_rule(rule, &sentence2);
            if !matches.is_empty() {
                eprintln!("    MATCHED! {} matches", matches.len());
            }
        }

        let been_rules: Vec<_> = compiled_rule_set.rules.iter()
            .filter(|r| r.id.contains("BEEN_PART"))
            .collect();
        assert!(!been_rules.is_empty(), "Should find BEEN_PART rules");

        let mut any_matched = false;
        for rule in been_rules.iter() {
            let matches = runner.pattern_engine.match_rule(rule, &sentence);
            if !matches.is_empty() {
                any_matched = true;
            }
        }
        assert!(any_matched, "At least one BEEN_PART rule should match 'I'm been prepared.'");
    }

    #[test]
    fn test_to_too_inflected_matching() {
        let runner = &TestRunner::new();
        let grammar_path = "/home/agent/languagetool/languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/grammar.xml";
        let xml = std::fs::read_to_string(grammar_path).unwrap();
        let compiled = og_xml::compiler::XmlCompiler::new().compile_file(&xml).unwrap();

        // These sentences use "is/were/been" (inflected forms of "be") + "to" + adjective
        // They should all match TO_TOO rules that use <token inflected="yes">be</token>
        let test_cases = vec![
            ("This is to funny.", true),
            ("It is to small", true),
            ("It's to early.", true),
            ("They were to good for you to miss.", true),
            ("He spent 10% to much.", true),
        ];

        let to_too_rules: Vec<_> = compiled.rules.iter()
            .filter(|r| r.id == "TO_TOO")
            .collect();

        for (text, should_match) in &test_cases {
            let sentence = runner.tokenize_sentence(text);
            let any_matched = to_too_rules.iter().any(|rule| {
                !runner.pattern_engine.match_rule(rule, &sentence).is_empty()
            });
            assert_eq!(any_matched, *should_match,
                "TO_TOO match for '{}' expected {}, got {}",
                text, should_match, any_matched);
        }
    }

    /// failing grammar rules. Prints a comparison of actual vs expected tags so
    /// you can quickly spot which words the tagger is missing or mis-tagging.
    ///
    /// Run with: cargo test -p og-test-runner test_common_pos_tag_coverage -- --nocapture
    #[test]
    fn test_common_pos_tag_coverage() {
        let runner = &TestRunner::new();

        /// Helper: tag a single isolated word and return the sorted list of POS
        /// tags that the full pipeline assigns (including disambiguation).
        fn tags_for(runner: &TestRunner, word: &str) -> Vec<String> {
            let sentence = runner.tokenize_sentence(word);
            let nwt: Vec<_> = sentence.non_whitespace_tokens();
            // Skip the synthetic <S> token and take the first real token's readings
            let token = nwt.iter()
                .find(|t| t.token().token() != "<S>")
                .expect("should have a non-<S> token");
            let mut tags: Vec<String> = token.readings()
                .iter()
                .flat_map(|r| r.pos_tags().to_vec())
                .collect();
            tags.sort();
            tags.dedup();
            tags
        }

        /// Helper: assert and print diagnostics for one word.
        fn check(
            runner: &TestRunner,
            word: &str,
            expected_tags: &[&str],
        ) -> Vec<String> {
            let actual = tags_for(runner, word);

            let mut missing: Vec<&str> = expected_tags
                .iter()
                .filter(|t| !actual.iter().any(|a| a == **t))
                .copied()
                .collect();
            missing.sort();

            let status = if missing.is_empty() { "OK" } else { "MISSING" };
            eprintln!(
                "  {:20} expected={:?}  actual={:?}  {}",
                format!("'{}'", word),
                expected_tags,
                actual,
                status,
            );
            if !missing.is_empty() {
                eprintln!("                         missing tags: {:?}", missing);
            }

            actual
        }

        // -----------------------------------------------------------------------
        // 3rd person singular present verbs  (VBZ)
        // -----------------------------------------------------------------------
        eprintln!("\n=== VBZ (3rd person singular present) ===");
        for word in &["needs", "wants", "likes", "tries"] {
            check(runner, word, &["VBZ"]);
        }

        // -----------------------------------------------------------------------
        // Base form verbs  (VB)
        // -----------------------------------------------------------------------
        eprintln!("\n=== VB (base form verb) ===");
        for word in &["need", "want", "like", "try"] {
            check(runner, word, &["VB"]);
        }
        for word in &["go", "make", "contact", "solve", "mount"] {
            check(runner, word, &["VB"]);
        }
        for word in &["compete", "bring", "send", "extend"] {
            check(runner, word, &["VB"]);
        }

        // -----------------------------------------------------------------------
        // Verbs that are also nouns  (VB + NN)
        // -----------------------------------------------------------------------
        eprintln!("\n=== VB + NN (verb and noun) ===");
        for word in &["open", "reopen", "close"] {
            check(runner, word, &["VB", "NN"]);
        }
        for word in &["help", "support", "sleep", "exercise"] {
            check(runner, word, &["NN", "VB"]);
        }
        for word in &["love", "fear", "care"] {
            check(runner, word, &["VB", "NN"]);
        }

        // -----------------------------------------------------------------------
        // Adjectives  (JJ)
        // -----------------------------------------------------------------------
        eprintln!("\n=== JJ (adjective) ===");
        for word in &["cool", "awesome", "great", "easy", "happy", "perfect", "recommendable"] {
            check(runner, word, &["JJ"]);
        }
        for word in &["okay", "worth"] {
            check(runner, word, &["JJ"]);
        }

        // -----------------------------------------------------------------------
        // Gerund / present participle  (VBG)
        // -----------------------------------------------------------------------
        eprintln!("\n=== VBG (gerund / present participle) ===");
        for word in &["happening", "going", "writing"] {
            check(runner, word, &["VBG"]);
        }

        // -----------------------------------------------------------------------
        // Past participle  (VBN)
        // -----------------------------------------------------------------------
        eprintln!("\n=== VBN (past participle) ===");
        for word in &["done", "been", "given"] {
            check(runner, word, &["VBN"]);
        }

        // -----------------------------------------------------------------------
        // Irregular plurals  (NNS)
        // -----------------------------------------------------------------------
        eprintln!("\n=== NNS (plural noun) ===");
        for word in &["phenomena", "criteria", "stimuli"] {
            check(runner, word, &["NNS"]);
        }
        for word in &["freshmen", "women", "gentlemen"] {
            check(runner, word, &["NNS"]);
        }

        // -----------------------------------------------------------------------
        // Comparative / base adjectives  (JJR / JJ)
        // -----------------------------------------------------------------------
        eprintln!("\n=== JJR / JJ (comparative and base adjective) ===");
        for word in &["better", "easier"] {
            check(runner, word, &["JJR"]);
        }
        for word in &["expensive"] {
            check(runner, word, &["JJ"]);
        }
    }
}

    #[test]
    fn test_debug_have_part_agreement_fp() {
        let runner = TestRunner::new();
        let sentences = vec![
            "Do you have change?",
            "Like other insects, beetles have open circulatory systems, based on hemolymph rather than blood.",
            "RWE said it has open positions",
            "We have soap.",
            "We do have soap.",
            "We do not have soap.",
            "I have contact with aliens.",
            "Do you have Telegram?",
            "The family that my friend had was his cousin and his sister.",
            "In the Sahara region some oases have palm trees.",
            "It had feature limitations such as...",
            "A rabbit has long ears.",
            "The tool my teacher had was archaic.",
            "Do you have bicycle in your garage?",
            "For any additional questions that you may have please contact me.",
            "We have only option 2 available.",
            "We have only option #2 available.",
            "I don't have job We need all the food we can get.",
            "The block of wood had saw marks on it.",
            "The sun had rose before I awoke.",
            "The block of wood has saw marks on it.",
            "For example, monarchical societies often had a system of \"social ranks\" which were collectivist because the social rank one had or did not have was more important than his or her individual will, and the specific rank in question could only be overridden in very limited cases.",
            "By the mid-10th century, the Samanid dynasty had crumble in the face of attacks from Turkish tribes to the north and from the Ghaznavids, a rising Turkic Muslim dynasty in Afghanistan.",
            "What I'm trying to have are papers on the Silver Leaf resort thingy.",
            "What I like to have are papers on the Silver Leaf resort thingy.",
            "What I really would like to have are papers on the Silver Leaf resort thingy.",
            "The options that my brother should have are the only ...",
            "The options my brother should have are the only ...",
            "The options he should have are the only ...",
            "we have open positions sometimes.",
            "There are a few I have open questions about or am waiting to receive copies of the contract.",
            "I am taking a scuba diving class and I have open water dives on both Saturday and Sunday.",
            "we don't have open access to the transmission grid",
            "In this case I think 100 is necessary because once a strike has open interest, we must continue to support it.",
            "If we restrict to strikes with a lower delta, we face the problem of not offering enough strikes and not making a market in options that have open EOL interest that have moved closer to the money.",
            "When the basis vectors have norm 1, the coordinate functionals e*n have...",
            "the coordinate functionals e*n have norm 2C in the dual of X.",
        ];

        for text in &sentences {
            let sentence = runner.tokenize_sentence(text);
            let nwt: Vec<_> = sentence.non_whitespace_tokens();
            eprintln!("\n'{}':", text);
            for (i, token) in nwt.iter().enumerate() {
                let chunk = token.chunk().unwrap_or("-");
                eprintln!("  [{}] {:15} pos={:?} chunk={}",
                    i, format!("'{}'", token.token().token()),
                    token.token().pos_tags(), chunk);
            }
        }
    }

    #[test]
    fn test_debug_missing_to_before_verb() {
        let runner = TestRunner::new();
        let sentences = vec![
            "We need test it first.",
            "I would like see us make this work.",
            "I like go to the pool.",
            "He needs go there.",
            "I want spend time with my friends.",
            "They want improve his skills.",
            "Try get in our heads.",
            "I like hang out with the crew.",
            "Installer will detect PHP 7 and try install.",
            "He needs go",
        ];

        for text in &sentences {
            let sentence = runner.tokenize_sentence(text);
            let nwt: Vec<_> = sentence.non_whitespace_tokens();
            eprintln!("\n'{}':", text);
            for (i, token) in nwt.iter().enumerate() {
                let chunk = token.chunk().unwrap_or("-");
                eprintln!("  [{}] {:15} pos={:?} chunk={}",
                    i, format!("'{}'", token.token().token()),
                    token.token().pos_tags(), chunk);
            }
        }
    }

    #[test]
    fn test_debug_chunking_with_whitespace() {
        let runner = TestRunner::new();
        let sentence = runner.tokenize_sentence("The client is here.");
        let tokens = sentence.tokens();
        eprintln!("ALL tokens for 'The client is here.' ({} total):", tokens.len());
        for (i, token) in tokens.iter().enumerate() {
            let tags = token.token().pos_tags();
            let tag_str = if tags.is_empty() { "(none)".to_string() } else { format!("{:?}", tags) };
            let ws = if token.is_whitespace() { " [WS]" } else { "" };
            eprintln!("  [{}] {:10} pos={} chunk={}{}",
                i, format!("'{}'", token.token().token().replace('\n', "\\n").replace(' ', "·")),
                tag_str,
                token.chunk().unwrap_or("-"),
                ws);
        }
    }

    #[test]
    fn test_debug_a_infinitive_fps() {
        let runner = TestRunner::new();
        let sentences = vec![
            "Checkout or add a subscribe box to the cart.",
            "If there is already a notify email...",
            "I hope this will help to explain why I am hesitant to endorse a centralize unit commitment implementation.",
            "Let's have a listen to what he wants to say",
            "He had a listen.",
            "a show",
            "He pulled a stun gun.",
            "Simply book an Excite Travel Vacations package by October 31, 2001.",
            "Smith's bill would ban the use of methyl tertiary butyl ether, an oxygenate used in 85 percent of reformulated gasoline.",
            "He sustained the injury while chasing down a lose ball in the fourth quarter.",
            "We invite you to come an have a look.",
            "The carve outs from the LOL remain an open issue.",
            "The unify pathing issue is resolved, all buckets are filled.",
            "The integrate mw's are 10mw'w and are reflected on revised oasis request 4405.",
            "Perhaps should add the decommission of the present MIPS measurement system and the implementation of the PGAS system.",
            "The engine ran fine, the outdrive went up and down, the prop was the correct size and pitch.",
            "The proof of Cleveland's mettle came quickly: its NFL regular-season opener was against the two-time defending champion Eagles on September 16 in Philadelphia.",
            "He said we would need either a parent guarantee or a prepay to transact with occidental energy marketing.",
            "The open e and back a are often indicated in writing by the use of the letters alaph",
            "He sustained the injury while chasing down a lose ball in the fourth quarter.",
            "confirms that the deny code that we wrote is ineffective.",
            "The Disk II single-sided floppy drive used 5.25-inch floppy disks",
            "In the U.S.A be the best.",
            "the conquer stage will be more complex than decrease and conquer algorithms.",
        ];

        for text in &sentences {
            let sentence = runner.tokenize_sentence(text);
            let nwt: Vec<_> = sentence.non_whitespace_tokens();
            eprintln!("\n'{}':", text);
            for (i, token) in nwt.iter().enumerate() {
                let chunk = token.chunk().unwrap_or("-");
                eprintln!("  [{}] {:15} pos={:?} chunk={}",
                    i, format!("'{}'", token.token().token()),
                    token.token().pos_tags(), chunk);
            }
        }
    }
