/// Rule-based English chunker.
/// Assigns IOB-style chunk tags (B-NP, I-NP, E-NP, B-VP, I-VP, B-PP, etc.)
/// based on POS tag sequences. Based on LanguageTool's EnglishChunker.

/// Chunk tag for a token
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkTag {
    None,
    BNP(String),  // B-NP, B-NP-singular, B-NP-plural
    INP(String),  // I-NP, I-NP-singular, I-NP-plural
    ENP(String),  // E-NP, E-NP-singular, E-NP-plural
    BVP,
    IVP,
    EVP,
    BPP,
    IPP,
    EPP,
    BADVP,
    IADVP,
    EADVP,
    BADJP,
    IADJP,
    EADJP,
    BSBAR,
    ISBAR,
    ESBAR,
    BPRT,
}

impl ChunkTag {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ChunkTag::None => None,
            ChunkTag::BNP(s) => Some(s.as_str()),
            ChunkTag::INP(s) => Some(s.as_str()),
            ChunkTag::ENP(s) => Some(s.as_str()),
            ChunkTag::BVP => Some("B-VP"),
            ChunkTag::IVP => Some("I-VP"),
            ChunkTag::EVP => Some("E-VP"),
            ChunkTag::BPP => Some("B-PP"),
            ChunkTag::IPP => Some("I-PP"),
            ChunkTag::EPP => Some("E-PP"),
            ChunkTag::BADVP => Some("B-ADVP"),
            ChunkTag::IADVP => Some("I-ADVP"),
            ChunkTag::EADVP => Some("E-ADVP"),
            ChunkTag::BADJP => Some("B-ADJP"),
            ChunkTag::IADJP => Some("I-ADJP"),
            ChunkTag::EADJP => Some("E-ADJP"),
            ChunkTag::BSBAR => Some("B-SBAR"),
            ChunkTag::ISBAR => Some("I-SBAR"),
            ChunkTag::ESBAR => Some("E-SBAR"),
            ChunkTag::BPRT => Some("B-PRT"),
        }
    }
}

/// Chunk a sequence of POS-tagged tokens.
/// Returns chunk tags for each token (skip index 0 = SENT_START).
pub fn chunk_tokens(pos_tags: &[&str], tokens: &[&str]) -> Vec<Option<String>> {
    let n = pos_tags.len();
    let mut chunks: Vec<Option<String>> = vec![None; n];

    if n <= 1 {
        return chunks;
    }

    let mut i = 1; // Skip SENT_START at index 0
    while i < n {
        let tag = pos_tags[i];
        let _word = tokens.get(i).map(|s| *s).unwrap_or("");

        // Skip punctuation and sentence boundaries
        if is_punct_tag(tag) {
            i += 1;
            continue;
        }

        // Try matching chunk patterns in priority order
        if let Some(consumed) = try_np_chunk(&pos_tags[i..], &tokens[i..], &mut chunks[i..]) {
            i += consumed;
        } else if let Some(consumed) = try_vp_chunk(&pos_tags[i..], &tokens[i..], &mut chunks[i..]) {
            i += consumed;
        } else if let Some(consumed) = try_pp_chunk(&pos_tags[i..], &mut chunks[i..]) {
            i += consumed;
        } else if let Some(consumed) = try_advp_chunk(&pos_tags[i..], &mut chunks[i..]) {
            i += consumed;
        } else if let Some(consumed) = try_adjp_chunk(&pos_tags[i..], &mut chunks[i..]) {
            i += consumed;
        } else {
            i += 1;
        }
    }

    chunks
}

fn is_punct_tag(tag: &str) -> bool {
    matches!(tag, "SENT_START" | "SENT_END" | "," | ":" | "." | "-LRB-" | "-RRB-" | "PCT")
}

/// Try to match a noun phrase starting at position 0.
fn try_np_chunk(pos_tags: &[&str], tokens: &[&str], chunks: &mut [Option<String>]) -> Option<usize> {
    let n = pos_tags.len();
    let mut j = 0;
    let mut has_noun = false;
    let mut is_plural = false;
    let mut started = false;

    // Optional determiner at the start
    if j < n && is_np_determiner(pos_tags[j]) {
        j += 1;
        started = true;
    }

    // Optional pre-determiners (all, both, half)
    if !started && j < n && is_predet(tokens.get(j).map(|s| *s).unwrap_or("")) {
        j += 1;
        started = true;
        // Then optional determiner
        if j < n && is_np_determiner(pos_tags[j]) {
            j += 1;
        }
    }

    // Optional adjective phrase / adjectival modifiers
    while j < n && is_adjectival(pos_tags[j]) {
        j += 1;
        started = true;
    }

    // Optional adverb + adjective
    while j < n && pos_tags[j] == "RB" && j + 1 < n && is_adjectival(pos_tags[j + 1]) {
        j += 2;
        started = true;
    }

    // Head noun(s) - the core of the NP
    if j < n && is_noun_tag(pos_tags[j]) {
        is_plural = pos_tags[j] == "NNS" || pos_tags[j] == "NNPS";
        has_noun = true;
        j += 1;
        started = true;
        // Additional nouns (compound nouns)
        while j < n && is_noun_tag(pos_tags[j]) {
            if pos_tags[j] == "NNS" || pos_tags[j] == "NNPS" {
                is_plural = true;
            }
            j += 1;
        }
        // Optional possessive 's after noun (e.g., "John's car")
        // This ends the current NP; the possessum will be a new NP
        if j < n && pos_tags[j] == "POS" {
            j += 1; // include 's in this NP as E-NP
        }
    }

    // Also handle NPs that are just PRP (pronouns)
    if !started && j < n && pos_tags[j] == "PRP" {
        // Check for possessive 's following the pronoun (e.g., "it's features")
        if j + 1 < n && pos_tags[j + 1] == "POS" {
            chunks[0] = Some("B-NP-singular".to_string());
            chunks[1] = Some("E-NP-singular".to_string());
            return Some(2);
        }
        chunks[0] = Some("B-NP-singular".to_string());
        return Some(1);
    }

    if !started && j < n && pos_tags[j] == "PRP$" {
        // Possessive pronoun starts an NP
        j += 1;
        started = true;
        // Continue with optional JJ + NN
        while j < n && is_adjectival(pos_tags[j]) {
            j += 1;
        }
        if j < n && is_noun_tag(pos_tags[j]) {
            is_plural = pos_tags[j] == "NNS" || pos_tags[j] == "NNPS";
            has_noun = true;
            j += 1;
        }
    }

    if !has_noun && !started {
        return None;
    }

    // Need at least one content word (noun or adjective)
    if !has_noun && j < 2 {
        return None;
    }

    let number_suffix = if is_plural { "-plural" } else { "-singular" };

    if j > 0 {
        if j == 1 {
            // Single-token NP gets both B-NP and E-NP tags
            chunks[0] = Some(format!("E-NP{}", number_suffix));
        } else {
            chunks[0] = Some(format!("B-NP{}", number_suffix));
            for k in 1..j - 1 {
                chunks[k] = Some(format!("I-NP{}", number_suffix));
            }
            chunks[j - 1] = Some(format!("E-NP{}", number_suffix));
        }
        return Some(j);
    }

    None
}

fn is_np_determiner(tag: &str) -> bool {
    tag == "DT" || tag == "EX" || tag == "CD" || tag == "WDT"
}

fn is_predet(word: &str) -> bool {
    matches!(word.to_lowercase().as_str(), "all" | "both" | "half" | "such" | "many" | "some" | "few")
}

fn is_adjectival(tag: &str) -> bool {
    matches!(tag, "JJ" | "JJR" | "JJS" | "VBN" | "VBG" | "ORD")
}

fn is_noun_tag(tag: &str) -> bool {
    tag.starts_with("NN") || tag == "FW"
}

/// Try to match a verb phrase.
/// Groups auxiliaries (be/have/do forms + modals) + adverbs + main verb into a single VP chunk.
/// Matches Java LT's OpenNLP chunker behavior.
fn try_vp_chunk(pos_tags: &[&str], tokens: &[&str], chunks: &mut [Option<String>]) -> Option<usize> {
    let n = pos_tags.len();
    let mut j = 0;

    // Optional modals
    while j < n && pos_tags[j] == "MD" {
        j += 1;
    }

    // Optional auxiliary verbs (be/have/do forms) with optional negation
    // Pattern: (VBZ/VBP/VBD/VBN/VBG of be/have/do) optionally followed by "not/n't"
    while j < n {
        if is_verb_tag(pos_tags[j]) && is_aux_verb(tokens.get(j).map(|s| *s).unwrap_or("")) {
            j += 1;
            // Optional negation after auxiliary
            if j < n && is_negation(tokens.get(j).map(|s| *s).unwrap_or("")) {
                j += 1;
            }
        } else {
            break;
        }
    }

    // Optional adverb between aux and main verb
    if j < n && pos_tags[j] == "RB" {
        if j + 1 < n && is_verb_tag(pos_tags[j + 1]) {
            j += 1;
        }
    }

    // Main verb - but not if the previous aux was have/has/had and this looks NP-initial
    if j < n && is_verb_tag(pos_tags[j]) {
        // After "have/has/had", stop if next token is adjectival or noun-like
        // (e.g., "has open" → open should start NP, not be VP main verb)
        let prev_is_have_aux = if j > 0 {
            let prev_tok = tokens.get(j - 1).map(|s| *s).unwrap_or("");
            matches!(prev_tok.to_lowercase().as_str(), "have" | "has" | "had" | "'ve" | "'s")
                && pos_tags[j - 1] != "MD" // only if prev was the have-aux, not modal
        } else {
            false
        };
        if prev_is_have_aux && (is_adjectival(pos_tags[j]) || is_noun_tag(pos_tags[j])) {
            // Don't consume this as main verb; end VP at the aux
        } else {
            j += 1;
        }
    } else if j > 0 {
        // Had auxiliaries but no main verb - still a VP (e.g., "can not")
        // But only if we had at least one modal
    } else {
        return None;
    }

    // Chain consecutive verbs including base-form (VB), participles (VBN), gerunds (VBG)
    // Also handle adverbs between verbs
    while j < n {
        if is_verb_tag(pos_tags[j]) {
            j += 1;
        } else if pos_tags[j] == "RB" && j + 1 < n && is_verb_tag(pos_tags[j + 1]) {
            j += 1;
        } else {
            break;
        }
    }

    if j == 0 {
        return None;
    }

    // Assign chunk tags - VP uses only B-/I- tags (no E- tags, matching Java LT)
    if j == 1 {
        chunks[0] = Some("B-VP".to_string());
    } else {
        chunks[0] = Some("B-VP".to_string());
        for k in 1..j {
            chunks[k] = Some("I-VP".to_string());
        }
    }
    Some(j)
}

fn is_aux_verb(word: &str) -> bool {
    matches!(word.to_lowercase().as_str(),
        "be" | "am" | "is" | "are" | "was" | "were" | "been" | "being" |
        "have" | "has" | "had" | "having" |
        "do" | "does" | "did" | "doing" | "done"
    )
}

fn is_negation(word: &str) -> bool {
    matches!(word.to_lowercase().as_str(), "not" | "n't" | "nt" | "never")
}

#[allow(dead_code)]
fn is_auxiliary(tag: &str) -> bool {
    tag == "MD" || tag == "EX"
}

fn is_verb_tag(tag: &str) -> bool {
    matches!(tag, "VB" | "VBD" | "VBG" | "VBN" | "VBP" | "VBZ")
}

/// Try to match a prepositional phrase.
fn try_pp_chunk(pos_tags: &[&str], chunks: &mut [Option<String>]) -> Option<usize> {
    if pos_tags.is_empty() || pos_tags[0] != "IN" && pos_tags[0] != "TO" {
        return None;
    }
    // PP = preposition + optional NP
    // Just tag the preposition for now
    chunks[0] = Some("B-PP".to_string());
    Some(1)
}

/// Try to match an adverb phrase.
fn try_advp_chunk(pos_tags: &[&str], chunks: &mut [Option<String>]) -> Option<usize> {
    if pos_tags.is_empty() || pos_tags[0] != "RB" {
        return None;
    }
    let mut j = 1;
    while j < pos_tags.len() && pos_tags[j] == "RB" {
        j += 1;
    }
    if j == 1 {
        chunks[0] = Some("B-ADVP".to_string());
    } else {
        chunks[0] = Some("B-ADVP".to_string());
        for k in 1..j {
            chunks[k] = Some("I-ADVP".to_string());
        }
    }
    Some(j)
}

/// Try to match an adjective phrase.
fn try_adjp_chunk(pos_tags: &[&str], chunks: &mut [Option<String>]) -> Option<usize> {
    if pos_tags.is_empty() || !is_adjectival(pos_tags[0]) {
        return None;
    }
    chunks[0] = Some("B-ADJP".to_string());
    Some(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vp_chaining() {
        // "We want test it" - want(B-VP) + test(I-VP) should be one VP
        let pos_tags = vec!["SENT_START", "PRP", "VB", "VB", "PRP"];
        let tokens = vec!["<S>", "We", "want", "test", "it"];
        let chunks = chunk_tokens(
            &pos_tags.iter().map(|s| *s).collect::<Vec<_>>(),
            &tokens.iter().map(|s| *s).collect::<Vec<_>>(),
        );
        // VP uses only B-/I- tags (no E- tags)
        assert_eq!(chunks[2].as_deref(), Some("B-VP")); // want
        assert_eq!(chunks[3].as_deref(), Some("I-VP")); // test
    }

    #[test]
    fn test_basic_np_chunking() {
        let pos_tags = vec!["SENT_START", "DT", "JJ", "NN", "VBZ"];
        let tokens = vec!["<S>", "The", "quick", "fox", "jumps"];
        let chunks = chunk_tokens(
            &pos_tags.iter().map(|s| *s).collect::<Vec<_>>(),
            &tokens.iter().map(|s| *s).collect::<Vec<_>>(),
        );

        assert_eq!(chunks[1], Some("B-NP-singular".to_string())); // The
        assert_eq!(chunks[2], Some("I-NP-singular".to_string())); // quick
        assert_eq!(chunks[3], Some("E-NP-singular".to_string())); // fox
    }

    #[test]
    fn test_vp_chunking() {
        let pos_tags = vec!["SENT_START", "NN", "MD", "VB"];
        let tokens = vec!["<S>", "He", "can", "run"];
        let chunks = chunk_tokens(
            &pos_tags.iter().map(|s| *s).collect::<Vec<_>>(),
            &tokens.iter().map(|s| *s).collect::<Vec<_>>(),
        );

        assert!(chunks[2].as_deref() == Some("B-VP"));
        assert!(chunks[3].as_deref() == Some("I-VP"));
    }

    #[test]
    fn test_aux_vp_chunking() {
        // "He has been walking" - should be one VP chunk
        let pos_tags = vec!["SENT_START", "PRP", "VBZ", "VBN", "VBG"];
        let tokens = vec!["<S>", "He", "has", "been", "walking"];
        let chunks = chunk_tokens(
            &pos_tags.iter().map(|s| *s).collect::<Vec<_>>(),
            &tokens.iter().map(|s| *s).collect::<Vec<_>>(),
        );

        assert_eq!(chunks[1].as_deref(), Some("B-NP-singular")); // He (PRP = NP)
        assert_eq!(chunks[2].as_deref(), Some("B-VP")); // has
        assert_eq!(chunks[3].as_deref(), Some("I-VP")); // been
        assert_eq!(chunks[4].as_deref(), Some("I-VP")); // walking
    }

    #[test]
    fn test_pp_chunking() {
        let pos_tags = vec!["SENT_START", "NN", "IN", "DT", "NN"];
        let tokens = vec!["<S>", "cat", "on", "the", "mat"];
        let chunks = chunk_tokens(
            &pos_tags.iter().map(|s| *s).collect::<Vec<_>>(),
            &tokens.iter().map(|s| *s).collect::<Vec<_>>(),
        );

        assert_eq!(chunks[2], Some("B-PP".to_string())); // on
    }

    #[test]
    fn test_plural_np() {
        let pos_tags = vec!["SENT_START", "DT", "NNS", "VBP"];
        let tokens = vec!["<S>", "The", "dogs", "run"];
        let chunks = chunk_tokens(
            &pos_tags.iter().map(|s| *s).collect::<Vec<_>>(),
            &tokens.iter().map(|s| *s).collect::<Vec<_>>(),
        );

        assert_eq!(chunks[1], Some("B-NP-plural".to_string())); // The
        assert_eq!(chunks[2], Some("E-NP-plural".to_string())); // dogs
    }
}
