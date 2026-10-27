#[derive(Debug, Clone, PartialEq)]
pub enum ChunkTag {
    None,
    BNP(String),
    INP(String),
    ENP(String),
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

pub struct PostDisambiguationChunker;

impl og_core::checker::Chunker for PostDisambiguationChunker {
    fn add_chunk_tags(&self, sentence: &mut og_core::AnalyzedSentence) {
        apply_chunk_tags(sentence);
    }
}

pub fn load_opennlp_chunker(resource_dir: &std::path::Path) {
    crate::opennlp::OpenNlpEnglishChunker::install(resource_dir);
}

pub fn apply_chunk_tags(sentence: &mut og_core::AnalyzedSentence) {
    if let Some(chunker) = crate::opennlp::OpenNlpEnglishChunker::shared() {
        chunker.add_chunk_tags(sentence);
        return;
    }
    apply_heuristic_chunk_tags(sentence);
}

fn apply_heuristic_chunk_tags(sentence: &mut og_core::AnalyzedSentence) {
    let mut nw_indices: Vec<usize> = Vec::new();
    let mut all_tags: Vec<Vec<String>> = Vec::new();
    let mut texts: Vec<&str> = Vec::new();

    for (i, t) in sentence.tokens().iter().enumerate() {
        if t.is_whitespace() {
            continue;
        }
        let tags = t.token().pos_tags();
        let boundary_only =
            tags.iter().all(|t| t == "SENT_START" || t == "SENT_END") || t.token().token() == "<S>";
        if boundary_only {
            continue;
        }
        nw_indices.push(i);
        all_tags.push(tags.to_vec());
        texts.push(t.token().token());
    }

    let pos_tags = select_chunker_tags(&all_tags, &texts);

    let mut chunker_pos: Vec<&str> = vec!["SENT_START"];
    let mut chunker_texts: Vec<&str> = vec!["<S>"];
    chunker_pos.extend(pos_tags.iter().copied());
    chunker_texts.extend(texts.iter().copied());

    let chunks = chunk_tokens(&chunker_pos, &chunker_texts);

    let tokens = sentence.tokens_mut();
    for (chunk_idx, orig_idx) in nw_indices.iter().enumerate() {
        if let Some(Some(chunk_tag)) = chunks.get(chunk_idx + 1) {
            tokens[*orig_idx]
                .replace_chunk_tags_preserving_apostrophe_flag(vec![chunk_tag.clone()]);
        }
    }
}

pub fn select_chunker_tags<'a>(all_tags: &'a [Vec<String>], texts: &[&str]) -> Vec<&'a str> {
    let mut selected: Vec<&str> = Vec::with_capacity(all_tags.len());
    for i in 0..all_tags.len() {
        selected.push(select_chunker_tag(all_tags, texts, &selected, i));
    }
    selected
}

pub fn select_chunker_tag<'a>(
    all_tags: &'a [Vec<String>],
    texts: &[&str],
    selected_so_far: &[&str],
    i: usize,
) -> &'a str {
    let tags = &all_tags[i];
    if tags.is_empty() {
        return "";
    }
    if tags.len() == 1 {
        return tags[0].as_str();
    }

    if tags.iter().any(|t| t == "CC")
        && matches!(
            texts[i].to_lowercase().as_str(),
            "and" | "or" | "but" | "nor" | "yet" | "for" | "so"
        )
    {
        return "CC";
    }

    let prev_is_adjective = i > 0
        && selected_so_far
            .get(i - 1)
            .is_some_and(|t| t.starts_with("JJ"));
    if prev_is_adjective && tags.iter().any(|t| t.starts_with("NN")) {
        return tags
            .iter()
            .find(|t| t.starts_with("NN"))
            .map(|s| s.as_str())
            .unwrap_or(tags[0].as_str());
    }

    let selected_is = |k: usize| selected_so_far.get(k).copied();
    let after_modal = i >= 1 && selected_is(i - 1) == Some("MD");
    let after_adverb_over_modal =
        i >= 2 && selected_is(i - 1) == Some("RB") && selected_is(i - 2) == Some("MD");
    let after_to = i >= 1 && selected_is(i - 1) == Some("TO");
    if (after_modal || after_adverb_over_modal || after_to)
        && tags.iter().any(|t| t.starts_with("VB"))
    {
        return tags
            .iter()
            .find(|t| t.starts_with("VB"))
            .map(|s| s.as_str())
            .unwrap_or(tags[0].as_str());
    }

    let after_coordinator = i > 0
        && all_tags[i - 1].iter().any(|t| t == "CC")
        && matches!(
            texts
                .get(i - 1)
                .copied()
                .unwrap_or("")
                .to_lowercase()
                .as_str(),
            "and" | "or" | "but" | "nor" | "yet" | "so"
        );
    if after_coordinator && tags.iter().any(|t| t == "VB") {
        let next_has_vb =
            i + 1 < all_tags.len() && all_tags[i + 1].iter().any(|t| t.starts_with("VB"));
        let conjunct_has_vb = (i.saturating_sub(4)..i.saturating_sub(1))
            .any(|k| all_tags[k].iter().any(|t| t.starts_with("VB")));
        if next_has_vb || conjunct_has_vb {
            return "VB";
        }
    }

    let has_vb = tags.iter().any(|t| t.starts_with("VB"));
    let has_jj = tags.iter().any(|t| t.starts_with("JJ"));
    let has_nn = tags.iter().any(|t| t.starts_with("NN"));
    let prev_is_verb = i > 0
        && all_tags[i - 1]
            .iter()
            .any(|t| t.starts_with("VB") || t == "MD");
    let prev_is_have = i > 0
        && prev_is_verb
        && matches!(
            texts
                .get(i - 1)
                .copied()
                .unwrap_or("")
                .to_lowercase()
                .as_str(),
            "have" | "has" | "had" | "'ve" | "'s"
        );
    if prev_is_have && has_jj {
        return tags
            .iter()
            .find(|t| t.starts_with("JJ"))
            .map(|s| s.as_str())
            .unwrap_or(tags[0].as_str());
    }
    if prev_is_have && has_nn {
        return tags
            .iter()
            .find(|t| t.starts_with("NN"))
            .map(|s| s.as_str())
            .unwrap_or(tags[0].as_str());
    }
    let prev_is_det = i > 0
        && all_tags[i - 1]
            .iter()
            .any(|t| t == "DT" || t == "PRP$" || t == "CD" || t == "WDT");
    if prev_is_det && has_nn && has_vb {
        return tags
            .iter()
            .find(|t| t.starts_with("NN"))
            .map(|s| s.as_str())
            .unwrap_or(tags[0].as_str());
    }
    if prev_is_verb && has_vb && has_nn && !prev_is_have {
        return tags
            .iter()
            .find(|t| t.starts_with("VB"))
            .map(|s| s.as_str())
            .unwrap_or(tags[0].as_str());
    }
    if prev_is_verb && has_vb && has_jj && !prev_is_have {
        return tags
            .iter()
            .find(|t| t.starts_with("VB"))
            .map(|s| s.as_str())
            .unwrap_or(tags[0].as_str());
    }
    let prev_is_prp = i > 0
        && all_tags[i - 1]
            .iter()
            .any(|t| t == "PRP" || t.starts_with("PRP_S") || t.starts_with("PRP_O"));
    if prev_is_prp {
        let has_md = tags.iter().any(|t| t == "MD");
        if has_vb && has_nn {
            return tags
                .iter()
                .find(|t| t.starts_with("VB"))
                .map(|s| s.as_str())
                .unwrap_or(tags[0].as_str());
        }
        if has_md && has_nn {
            return "MD";
        }
    }
    if matches!(
        texts[i].to_lowercase().as_str(),
        "am" | "is"
            | "are"
            | "was"
            | "were"
            | "be"
            | "been"
            | "being"
            | "have"
            | "has"
            | "had"
            | "do"
            | "does"
            | "did"
            | "doing"
            | "done"
    ) && has_vb
    {
        return tags
            .iter()
            .find(|t| t.starts_with("VB"))
            .map(|s| s.as_str())
            .unwrap_or(tags[0].as_str());
    }

    tags[0].as_str()
}

pub fn chunk_tokens(pos_tags: &[&str], tokens: &[&str]) -> Vec<Option<String>> {
    let n = pos_tags.len();
    let mut chunks: Vec<Option<String>> = vec![None; n];

    if n <= 1 {
        return chunks;
    }

    let mut i = 1;
    while i < n {
        let tag = pos_tags[i];
        let _word = tokens.get(i).copied().unwrap_or("");

        if is_punct_tag(tag) {
            i += 1;
            continue;
        }

        if let Some(consumed) = try_np_chunk(&pos_tags[i..], &tokens[i..], &mut chunks[i..]) {
            i += consumed;
        } else if let Some(consumed) = try_vp_chunk(&pos_tags[i..], &tokens[i..], &mut chunks[i..])
        {
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
    matches!(
        tag,
        "SENT_START" | "SENT_END" | "," | ":" | "." | "-LRB-" | "-RRB-" | "PCT"
    )
}

fn try_np_chunk(
    pos_tags: &[&str],
    tokens: &[&str],
    chunks: &mut [Option<String>],
) -> Option<usize> {
    let n = pos_tags.len();
    let mut j = 0;
    let mut has_noun = false;
    let mut is_plural = false;
    let mut started = false;

    if j < n && is_np_determiner(pos_tags[j]) {
        j += 1;
        started = true;
    }

    if !started && j < n && is_predet(tokens.get(j).copied().unwrap_or("")) {
        j += 1;
        started = true;
        if j < n && is_np_determiner(pos_tags[j]) {
            j += 1;
        }
    }

    while j < n && is_adjectival(pos_tags[j]) {
        j += 1;
        started = true;
    }

    while j < n && pos_tags[j] == "RB" && j + 1 < n && is_adjectival(pos_tags[j + 1]) {
        j += 2;
        started = true;
    }

    if j < n && is_noun_tag(pos_tags[j]) {
        has_noun = true;
        j += 1;
        started = true;
        while j < n && is_noun_tag(pos_tags[j]) {
            j += 1;
        }
        is_plural = pos_tags[j - 1] == "NNS" || pos_tags[j - 1] == "NNPS";
        if j < n && pos_tags[j] == "POS" {
            j += 1;
        }
    }

    if !started && j < n && pos_tags[j] == "PRP" {
        if j + 1 < n && pos_tags[j + 1] == "POS" {
            chunks[0] = Some("B-NP-singular".to_string());
            chunks[1] = Some("E-NP-singular".to_string());
            return Some(2);
        }
        chunks[0] = Some("B-NP-singular".to_string());
        return Some(1);
    }

    if !started && j < n && pos_tags[j] == "PRP$" {
        j += 1;
        started = true;
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

    if !has_noun && j < 2 {
        return None;
    }

    let number_suffix = if is_plural { "-plural" } else { "-singular" };

    if j > 0 {
        if j == 1 {
            chunks[0] = Some(format!("E-NP{}", number_suffix));
        } else {
            chunks[0] = Some(format!("B-NP{}", number_suffix));
            for chunk in chunks.iter_mut().take(j - 1).skip(1) {
                *chunk = Some(format!("I-NP{}", number_suffix));
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
    matches!(
        word.to_lowercase().as_str(),
        "all" | "both" | "half" | "such" | "many" | "some" | "few"
    )
}

fn is_adjectival(tag: &str) -> bool {
    matches!(tag, "JJ" | "JJR" | "JJS" | "VBN" | "VBG" | "ORD")
}

fn is_noun_tag(tag: &str) -> bool {
    tag.starts_with("NN") || tag == "FW"
}

fn try_vp_chunk(
    pos_tags: &[&str],
    tokens: &[&str],
    chunks: &mut [Option<String>],
) -> Option<usize> {
    let n = pos_tags.len();
    let mut j = 0;

    while j < n && pos_tags[j] == "MD" {
        j += 1;
    }

    while j < n {
        if is_verb_tag(pos_tags[j]) && is_aux_verb(tokens.get(j).copied().unwrap_or("")) {
            j += 1;
            if j < n && is_negation(tokens.get(j).copied().unwrap_or("")) {
                j += 1;
            }
        } else {
            break;
        }
    }

    if j < n && pos_tags[j] == "RB" && j + 1 < n && is_verb_tag(pos_tags[j + 1]) {
        j += 1;
    }

    if j < n && is_verb_tag(pos_tags[j]) {
        let prev_is_have_aux = if j > 0 {
            let prev_tok = tokens.get(j - 1).copied().unwrap_or("");
            matches!(
                prev_tok.to_lowercase().as_str(),
                "have" | "has" | "had" | "'ve" | "'s"
            ) && pos_tags[j - 1] != "MD"
        } else {
            false
        };
        if prev_is_have_aux && (is_adjectival(pos_tags[j]) || is_noun_tag(pos_tags[j])) {
        } else {
            j += 1;
        }
    } else if j > 0 {
    } else {
        return None;
    }

    while j < n {
        if is_verb_tag(pos_tags[j])
            || (pos_tags[j] == "RB" && j + 1 < n && is_verb_tag(pos_tags[j + 1]))
        {
            j += 1;
        } else {
            break;
        }
    }

    if j == 0 {
        return None;
    }

    if j == 1 {
        chunks[0] = Some("B-VP".to_string());
    } else {
        chunks[0] = Some("B-VP".to_string());
        for chunk in chunks.iter_mut().take(j).skip(1) {
            *chunk = Some("I-VP".to_string());
        }
    }
    Some(j)
}

fn is_aux_verb(word: &str) -> bool {
    matches!(
        word.to_lowercase().as_str(),
        "be" | "am"
            | "is"
            | "are"
            | "was"
            | "were"
            | "been"
            | "being"
            | "have"
            | "has"
            | "had"
            | "having"
            | "do"
            | "does"
            | "did"
            | "doing"
            | "done"
    )
}

fn is_negation(word: &str) -> bool {
    matches!(word.to_lowercase().as_str(), "not" | "n't" | "nt" | "never")
}

fn is_verb_tag(tag: &str) -> bool {
    matches!(tag, "VB" | "VBD" | "VBG" | "VBN" | "VBP" | "VBZ")
}

fn try_pp_chunk(pos_tags: &[&str], chunks: &mut [Option<String>]) -> Option<usize> {
    if pos_tags.is_empty() || pos_tags[0] != "IN" && pos_tags[0] != "TO" {
        return None;
    }
    chunks[0] = Some("B-PP".to_string());
    Some(1)
}

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
        for chunk in chunks.iter_mut().take(j).skip(1) {
            *chunk = Some("I-ADVP".to_string());
        }
    }
    Some(j)
}

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
        let pos_tags = ["SENT_START", "PRP", "VB", "VB", "PRP"];
        let tokens = ["<S>", "We", "want", "test", "it"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[2].as_deref(), Some("B-VP"));
        assert_eq!(chunks[3].as_deref(), Some("I-VP"));
    }

    #[test]
    fn test_basic_np_chunking() {
        let pos_tags = ["SENT_START", "DT", "JJ", "NN", "VBZ"];
        let tokens = ["<S>", "The", "quick", "fox", "jumps"];
        let chunks = chunk_tokens(&pos_tags, &tokens);

        assert_eq!(chunks[1], Some("B-NP-singular".to_string()));
        assert_eq!(chunks[2], Some("I-NP-singular".to_string()));
        assert_eq!(chunks[3], Some("E-NP-singular".to_string()));
    }

    #[test]
    fn test_vp_chunking() {
        let pos_tags = ["SENT_START", "NN", "MD", "VB"];
        let tokens = ["<S>", "He", "can", "run"];
        let chunks = chunk_tokens(&pos_tags, &tokens);

        assert!(chunks[2].as_deref() == Some("B-VP"));
        assert!(chunks[3].as_deref() == Some("I-VP"));
    }

    #[test]
    fn test_aux_vp_chunking() {
        let pos_tags = ["SENT_START", "PRP", "VBZ", "VBN", "VBG"];
        let tokens = ["<S>", "He", "has", "been", "walking"];
        let chunks = chunk_tokens(&pos_tags, &tokens);

        assert_eq!(chunks[1].as_deref(), Some("B-NP-singular"));
        assert_eq!(chunks[2].as_deref(), Some("B-VP"));
        assert_eq!(chunks[3].as_deref(), Some("I-VP"));
        assert_eq!(chunks[4].as_deref(), Some("I-VP"));
    }

    #[test]
    fn test_pp_chunking() {
        let pos_tags = ["SENT_START", "NN", "IN", "DT", "NN"];
        let tokens = ["<S>", "cat", "on", "the", "mat"];
        let chunks = chunk_tokens(&pos_tags, &tokens);

        assert_eq!(chunks[2], Some("B-PP".to_string()));
    }

    #[test]
    fn test_np_plurality_follows_head_noun() {
        let pos = ["SENT_START", "DT", "NNS", "NN"];
        let tokens = ["<S>", "an", "integrations", "expert"];
        let chunks = chunk_tokens(&pos, &tokens);
        assert_eq!(chunks[2].as_deref(), Some("I-NP-singular"));
        assert_eq!(chunks[3].as_deref(), Some("E-NP-singular"));
    }

    #[test]
    fn test_plural_np() {
        let pos_tags = ["SENT_START", "DT", "NNS", "VBP"];
        let tokens = ["<S>", "The", "dogs", "run"];
        let chunks = chunk_tokens(&pos_tags, &tokens);

        assert_eq!(chunks[1], Some("B-NP-plural".to_string()));
        assert_eq!(chunks[2], Some("E-NP-plural".to_string()));
    }

    #[test]
    fn chunk_tag_as_str_round_trips_every_variant() {
        assert_eq!(ChunkTag::None.as_str(), None);
        assert_eq!(ChunkTag::BNP("x".into()).as_str(), Some("x"));
        assert_eq!(ChunkTag::INP("y".into()).as_str(), Some("y"));
        assert_eq!(ChunkTag::ENP("z".into()).as_str(), Some("z"));
        assert_eq!(ChunkTag::BVP.as_str(), Some("B-VP"));
        assert_eq!(ChunkTag::IVP.as_str(), Some("I-VP"));
        assert_eq!(ChunkTag::EVP.as_str(), Some("E-VP"));
        assert_eq!(ChunkTag::BPP.as_str(), Some("B-PP"));
        assert_eq!(ChunkTag::IPP.as_str(), Some("I-PP"));
        assert_eq!(ChunkTag::EPP.as_str(), Some("E-PP"));
        assert_eq!(ChunkTag::BADVP.as_str(), Some("B-ADVP"));
        assert_eq!(ChunkTag::IADVP.as_str(), Some("I-ADVP"));
        assert_eq!(ChunkTag::EADVP.as_str(), Some("E-ADVP"));
        assert_eq!(ChunkTag::BADJP.as_str(), Some("B-ADJP"));
        assert_eq!(ChunkTag::IADJP.as_str(), Some("I-ADJP"));
        assert_eq!(ChunkTag::EADJP.as_str(), Some("E-ADJP"));
        assert_eq!(ChunkTag::BSBAR.as_str(), Some("B-SBAR"));
        assert_eq!(ChunkTag::ISBAR.as_str(), Some("I-SBAR"));
        assert_eq!(ChunkTag::ESBAR.as_str(), Some("E-SBAR"));
        assert_eq!(ChunkTag::BPRT.as_str(), Some("B-PRT"));
    }

    #[test]
    fn empty_tag_list_chunks_to_nothing() {
        assert_eq!(chunk_tokens(&["DT"], &["the"]), vec![None]);
    }

    #[test]
    fn punctuation_tags_are_skipped_without_consuming_chunks() {
        let pos_tags = ["SENT_START", "NN", ",", "VBZ"];
        let tokens = ["<S>", "cat", ",", "sleeps"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[2], None);
        assert_eq!(chunks[1].as_deref(), Some("E-NP-singular"));
        assert_eq!(chunks[3].as_deref(), Some("B-VP"));
    }

    #[test]
    fn predeterminer_and_determiner_join_the_np() {
        let pos_tags = ["SENT_START", "NN", "DT", "NNS"];
        let tokens = ["<S>", "all", "the", "dogs"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[1].as_deref(), Some("B-NP-plural"));
        assert_eq!(chunks[2].as_deref(), Some("I-NP-plural"));
        assert_eq!(chunks[3].as_deref(), Some("E-NP-plural"));
    }

    #[test]
    fn adverb_before_adjective_stays_inside_np() {
        let pos_tags = ["SENT_START", "DT", "RB", "JJ", "NN"];
        let tokens = ["<S>", "a", "very", "quick", "fox"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[1].as_deref(), Some("B-NP-singular"));
        assert_eq!(chunks[2].as_deref(), Some("I-NP-singular"));
        assert_eq!(chunks[3].as_deref(), Some("I-NP-singular"));
        assert_eq!(chunks[4].as_deref(), Some("E-NP-singular"));
    }

    #[test]
    fn possessive_pronoun_is_a_singular_np() {
        let pos_tags = ["SENT_START", "PRP$", "JJ", "NNS"];
        let tokens = ["<S>", "their", "big", "dogs"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[1].as_deref(), Some("B-NP-plural"));
        assert_eq!(chunks[2].as_deref(), Some("I-NP-plural"));
        assert_eq!(chunks[3].as_deref(), Some("E-NP-plural"));
    }

    #[test]
    fn possessive_marker_after_pronoun_ends_the_np() {
        let pos_tags = ["SENT_START", "PRP", "POS", "VBZ"];
        let tokens = ["<S>", "it", "'s", "rains"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[1].as_deref(), Some("B-NP-singular"));
        assert_eq!(chunks[2].as_deref(), Some("E-NP-singular"));
    }

    #[test]
    fn lone_pronoun_is_a_one_token_np() {
        let pos_tags = ["SENT_START", "PRP", "VBZ"];
        let tokens = ["<S>", "he", "runs"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[1].as_deref(), Some("B-NP-singular"));
        assert_eq!(chunks[2].as_deref(), Some("B-VP"));
    }

    #[test]
    fn possessive_marker_after_noun_joins_the_np() {
        let pos_tags = ["SENT_START", "NN", "POS", "NN"];
        let tokens = ["<S>", "dog", "'s", "bone"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[1].as_deref(), Some("B-NP-singular"));
        assert_eq!(chunks[2].as_deref(), Some("E-NP-singular"));
        assert_eq!(chunks[3].as_deref(), Some("E-NP-singular"));
    }

    #[test]
    fn negation_between_aux_and_verb_stays_in_vp() {
        let pos_tags = ["SENT_START", "PRP", "VBZ", "RB", "VBP"];
        let tokens = ["<S>", "he", "has", "not", "run"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[2].as_deref(), Some("B-VP"));
        assert_eq!(chunks[3].as_deref(), Some("I-VP"));
        assert_eq!(chunks[4].as_deref(), Some("I-VP"));
    }

    #[test]
    fn modal_then_adverb_then_verb_forms_one_vp() {
        let pos_tags = ["SENT_START", "PRP", "MD", "RB", "VB"];
        let tokens = ["<S>", "he", "can", "not", "go"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[2].as_deref(), Some("B-VP"));
        assert_eq!(chunks[3].as_deref(), Some("I-VP"));
        assert_eq!(chunks[4].as_deref(), Some("I-VP"));
    }

    #[test]
    fn to_infinitive_marker_is_a_pp() {
        let pos_tags = ["SENT_START", "VB", "TO", "VB"];
        let tokens = ["<S>", "want", "to", "sleep"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[1].as_deref(), Some("B-VP"));
        assert_eq!(chunks[2].as_deref(), Some("B-PP"));
        assert_eq!(chunks[3].as_deref(), Some("B-VP"));
    }

    #[test]
    fn consecutive_adverbs_chunk_into_one_advp() {
        let pos_tags = ["SENT_START", "VBD", "RB", "RB"];
        let tokens = ["<S>", "ran", "very", "fast"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[1].as_deref(), Some("B-VP"));
        assert_eq!(chunks[2].as_deref(), Some("B-ADVP"));
        assert_eq!(chunks[3].as_deref(), Some("I-ADVP"));
    }

    #[test]
    fn stranded_adjective_chunks_into_adjp() {
        let pos_tags = ["SENT_START", "VBP", "JJ"];
        let tokens = ["<S>", "look", "great"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[2].as_deref(), Some("B-ADJP"));
    }

    #[test]
    fn unknown_tag_falls_through_without_a_chunk() {
        let pos_tags = ["SENT_START", "UH", "NN"];
        let tokens = ["<S>", "oh", "cat"];
        let chunks = chunk_tokens(&pos_tags, &tokens);
        assert_eq!(chunks[1], None);
        assert_eq!(chunks[2].as_deref(), Some("E-NP-singular"));
    }

    #[test]
    fn heuristic_chunk_tags_skip_whitespace_and_sentence_markers() {
        use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

        let parts: [(&str, &[&str]); 7] = [
            ("<S>", &[]),
            ("The", &["DT"]),
            (" ", &[]),
            ("quick", &["JJ"]),
            ("fox", &["NN"]),
            ("jumps", &["VBZ"]),
            (".", &["."]),
        ];
        let mut pos = 0usize;
        let mut tokens = Vec::new();
        for (text, tags) in parts {
            let mut at = AnalyzedToken::new(text, pos, pos + text.len());
            if !tags.is_empty() {
                at = at.with_pos_tags(tags.iter().map(|t| t.to_string()).collect());
            }
            tokens.push(AnalyzedTokenReadings::new(at));
            pos += text.len();
        }
        let mut sentence = AnalyzedSentence::new("<S>The quick fox jumps.", 0, pos);
        sentence.set_tokens(tokens);

        apply_heuristic_chunk_tags(&mut sentence);

        let chunks: Vec<&[String]> = sentence.tokens().iter().map(|t| t.chunks()).collect();
        assert!(chunks[0].is_empty(), "<S> marker: {chunks:?}");
        assert!(chunks[2].is_empty(), "whitespace: {chunks:?}");
        assert_eq!(chunks[1], &["B-NP-singular".to_string()]);
        assert_eq!(chunks[3], &["I-NP-singular".to_string()]);
        assert_eq!(chunks[4], &["E-NP-singular".to_string()]);
        assert_eq!(chunks[5], &["B-VP".to_string()]);
        assert!(chunks[6].is_empty(), "punct: {chunks:?}");
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    fn tags(list: &[&[&str]]) -> Vec<Vec<String>> {
        list.iter()
            .map(|t| t.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    #[test]
    fn coordinator_selects_cc() {
        let all = tags(&[
            &["NN", "VB", "VBP"],
            &["NNP"],
            &["NN", "CC"],
            &["NN", "VB", "VBP"],
        ]);
        let texts = ["Restart", "PHP", "and", "try"];
        assert_eq!(select_chunker_tags(&all, &texts)[2], "CC");
    }

    #[test]
    fn noun_after_adjective_wins_over_verb() {
        let all = tags(&[&["JJ", "NN:U", "VB", "VBP"], &["NNS", "VBZ"]]);
        let texts = ["open", "positions"];
        assert_eq!(select_chunker_tags(&all, &texts)[1], "NNS");
    }

    #[test]
    fn verb_coordination_after_cc_prefers_verb() {
        let all = tags(&[
            &["NN", "VB", "VBP"],
            &["NNP"],
            &["NN", "CC"],
            &["NN", "VB", "VBP"],
        ]);
        let texts = ["Restart", "PHP", "and", "try"];
        assert_eq!(select_chunker_tags(&all, &texts)[3], "VB");
    }

    #[test]
    fn noun_without_verbal_context_after_cc_stays_noun() {
        let all = tags(&[&["NN"], &["NN", "CC"], &["NN", "VB"], &["NN"]]);
        let texts = ["salt", "and", "pepper", "area"];
        assert_eq!(select_chunker_tags(&all, &texts)[2], "NN");
    }

    #[test]
    fn empty_candidate_tags_select_the_empty_string() {
        assert_eq!(select_chunker_tag(&[vec![]], &["x"], &[], 0), "");
    }

    #[test]
    fn verb_selected_after_modal_adverb_and_to() {
        let all = tags(&[&["MD"], &["VB", "NN"]]);
        assert_eq!(select_chunker_tag(&all, &["can", "go"], &["MD"], 1), "VB");

        let all = tags(&[&["MD"], &["RB"], &["VB", "NN"]]);
        assert_eq!(
            select_chunker_tag(&all, &["can", "not", "go"], &["MD", "RB"], 2),
            "VB"
        );

        let all = tags(&[&["TO"], &["VB", "NN"]]);
        assert_eq!(select_chunker_tag(&all, &["to", "go"], &["TO"], 1), "VB");
    }

    #[test]
    fn adjective_or_noun_selected_after_have_aux() {
        let all = tags(&[&["VBZ"], &["JJ", "NN"]]);
        assert_eq!(
            select_chunker_tag(&all, &["has", "gone"], &["VBZ"], 1),
            "JJ"
        );

        let all = tags(&[&["VBZ"], &["NN", "VB"]]);
        assert_eq!(
            select_chunker_tag(&all, &["has", "rest"], &["VBZ"], 1),
            "NN"
        );
    }

    #[test]
    fn noun_selected_after_determiner() {
        let all = tags(&[&["DT"], &["NN", "VB"]]);
        assert_eq!(
            select_chunker_tag(&all, &["the", "plant"], &["DT"], 1),
            "NN"
        );

        let all = tags(&[&["PRP$"], &["NN", "VB"]]);
        assert_eq!(
            select_chunker_tag(&all, &["my", "plant"], &["PRP$"], 1),
            "NN"
        );

        let all = tags(&[&["CD"], &["NN", "VB"]]);
        assert_eq!(
            select_chunker_tag(&all, &["two", "plant"], &["CD"], 1),
            "NN"
        );
    }

    #[test]
    fn verb_selected_after_plain_verb() {
        let all = tags(&[&["VBP"], &["VB", "NN"]]);
        assert_eq!(select_chunker_tag(&all, &["see", "go"], &["VBP"], 1), "VB");

        let all = tags(&[&["VBP"], &["VB", "JJ"]]);
        assert_eq!(select_chunker_tag(&all, &["see", "go"], &["VBP"], 1), "VB");
    }

    #[test]
    fn verb_or_modal_selected_after_pronoun() {
        let all = tags(&[&["PRP"], &["VB", "NN"]]);
        assert_eq!(
            select_chunker_tag(&all, &["they", "plant"], &["PRP"], 1),
            "VB"
        );

        let all = tags(&[&["PRP"], &["MD", "NN"]]);
        assert_eq!(
            select_chunker_tag(&all, &["they", "must"], &["PRP"], 1),
            "MD"
        );
    }

    #[test]
    fn be_verb_surface_picks_the_verb_reading() {
        let all = tags(&[&["EX"], &["VB", "NN"]]);
        assert_eq!(
            select_chunker_tag(&all, &["There", "been"], &["EX"], 1),
            "VB"
        );
    }

    #[test]
    fn multi_reading_word_without_context_keeps_first_tag() {
        let all = tags(&[&["DT"], &["NN", "VB"]]);
        assert_eq!(
            select_chunker_tag(&all, &["the", "plant"], &["NN"], 1),
            "NN"
        );
    }
}
