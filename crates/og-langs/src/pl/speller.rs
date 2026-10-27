use og_core::rule::Rule;
use og_core::{AnalyzedSentence, RuleMatch, SuggestedReplacement};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

pub const MORFOLOGIK_RULE_PL_PL: &str = "MORFOLOGIK_RULE_PL_PL";

const TOKENIZING_PREFIXES: [&str; 4] = ["Quasi", "quasi", "Niby", "niby"];

const NON_SPLITTING_PREFIXES: [&str; 29] = [
    "arcy", "neo", "pre", "anty", "eks", "bez", "beze", "ekstra", "hiper", "infra", "kontr",
    "maksi", "midi", "między", "mini", "nad", "nade", "około", "ponad", "post", "pro", "przeciw",
    "pseudo", "super", "śród", "ultra", "wice", "wokół", "wokoło",
];

const BANNED_SUFFIXES: [&str; 12] = [
    "ami", "ach", "e", "ego", "em", "emu", "ie", "im", "m", "om", "owie", "owi",
];

struct ExactOracle {
    arena: Vec<u8>,
    offsets: Vec<u32>,
}

impl ExactOracle {
    fn from_words(words: Vec<String>) -> Self {
        let mut sorted: Vec<Box<str>> = words
            .into_iter()
            .filter(|w| !w.is_empty())
            .map(String::into_boxed_str)
            .collect();
        sorted.sort_unstable();
        sorted.dedup();
        let mut arena: Vec<u8> = Vec::with_capacity(sorted.iter().map(|w| w.len()).sum());
        let mut offsets: Vec<u32> = Vec::with_capacity(sorted.len() + 1);
        for word in &sorted {
            offsets.push(arena.len() as u32);
            arena.extend_from_slice(word.as_bytes());
        }
        offsets.push(arena.len() as u32);
        Self { arena, offsets }
    }

    fn word_at(&self, idx: usize) -> &str {
        let start = self.offsets[idx] as usize;
        let end = self.offsets[idx + 1] as usize;
        std::str::from_utf8(&self.arena[start..end]).unwrap_or_default()
    }

    fn contains(&self, word: &str) -> bool {
        let n = self.offsets.len() - 1;
        let mut lo = 0usize;
        let mut hi = n;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let candidate = self.word_at(mid);
            match candidate.cmp(word) {
                std::cmp::Ordering::Equal => return true,
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        false
    }

    fn contains_java_fold(&self, word: &str) -> bool {
        if self.contains(word) {
            return true;
        }
        let lower = word.to_lowercase();
        lower != word && self.contains(&lower)
    }

    fn len(&self) -> usize {
        self.offsets.len() - 1
    }
}

fn load_pl_pl_words(resource_dir: &std::path::Path) -> Option<Vec<String>> {
    let fsa = og_tagger::morfologik::FsaWordSet::load(
        &resource_dir.join("hunspell/pl_PL.dict"),
        &resource_dir.join("hunspell/pl_PL.info"),
    )?;
    Some(fsa.word_list())
}

fn shared_speller_oracle() -> Option<Arc<ExactOracle>> {
    static CACHE: og_core::releasable::Releasable<ExactOracle> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let resource_dir = super::tagger::pl_resource_dir()?;
        let mut words = load_pl_pl_words(&resource_dir)?;
        if let Ok(extra) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt")) {
            for line in extra.lines() {
                let word = line.split(['\t', ';']).next().unwrap_or("").trim();
                if !word.is_empty() && !word.starts_with('#') {
                    words.push(word.to_string());
                }
            }
        }
        Some(Arc::new(ExactOracle::from_words(words)))
    })
}

fn shared_ignore_words() -> HashSet<String> {
    let mut ignore = HashSet::new();
    if let Some(resource_dir) = super::tagger::pl_resource_dir()
        && let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/ignore.txt"))
    {
        for line in words.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                ignore.insert(word.to_string());
            }
        }
    }
    ignore
}

fn shared_prohibited_words() -> HashSet<String> {
    let mut prohibited = HashSet::new();
    if let Some(resource_dir) = super::tagger::pl_resource_dir()
        && let Ok(words) = std::fs::read_to_string(resource_dir.join("hunspell/prohibit.txt"))
    {
        for line in words.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                prohibited.insert(word.to_string());
            }
        }
    }
    prohibited
}

pub struct PolishSpellerRule {
    oracle: Arc<ExactOracle>,
    tagger: Arc<super::tagger::PolishTagger>,
    prohibited: HashSet<String>,
    ignore_words: HashSet<String>,
    banned_suffixes: HashSet<&'static str>,
    accepted_compounds: Mutex<HashSet<String>>,
}

impl PolishSpellerRule {
    pub fn new() -> Self {
        Self {
            oracle: shared_speller_oracle()
                .unwrap_or_else(|| Arc::new(ExactOracle::from_words(Vec::new()))),
            tagger: super::tagger::PolishTagger::shared_arc()
                .expect("pl resource dir with polish.dict"),
            prohibited: shared_prohibited_words(),
            ignore_words: shared_ignore_words(),
            banned_suffixes: BANNED_SUFFIXES.into_iter().collect(),
            accepted_compounds: Mutex::new(HashSet::new()),
        }
    }

    pub fn shared() -> Arc<PolishSpellerRule> {
        static CACHE: og_core::releasable::Releasable<PolishSpellerRule> =
            og_core::releasable::Releasable::new();
        og_core::releasable::shared_get_or_build(&CACHE, || {
            Some(Arc::new(PolishSpellerRule::new()))
        })
        .expect("polish speller rule")
    }

    pub fn is_known_word(&self, word: &str) -> bool {
        if !word.chars().any(|c| c.is_alphabetic()) {
            return true;
        }
        if self.ignore_words.contains(word) || self.ignore_words.contains(&word.to_lowercase()) {
            return true;
        }
        if word.chars().count() == 1 && word.chars().next().is_some_and(char::is_alphabetic) {
            return true;
        }
        if word.contains('/')
            || word.contains('@')
            || word.starts_with("http")
            || word.starts_with("www.")
        {
            return true;
        }
        if word.chars().any(|c| c.is_ascii_digit()) {
            return true;
        }
        self.oracle.contains_java_fold(word)
    }

    pub fn oracle_size(&self) -> usize {
        self.oracle.len()
    }

    fn has_tokenizing_prefix(word: &str) -> bool {
        TOKENIZING_PREFIXES
            .iter()
            .any(|p| word.contains(&format!("{p}-")))
    }

    fn quasi_niby_pieces(word: &str) -> Vec<(usize, &str)> {
        let mut pieces: Vec<(usize, &str)> = Vec::new();
        let mut index = 0usize;
        while index < word.len() {
            let Some(rel) = TOKENIZING_PREFIXES
                .iter()
                .filter_map(|p| word[index..].find(&format!("{p}-")))
                .min()
            else {
                break;
            };
            let hit = index + rel;
            if hit > index {
                pieces.push((index, &word[index..hit]));
            }
            index = hit
                + word[hit..]
                    .find('-')
                    .map(|i| hit + i + 1)
                    .unwrap_or(word.len());
        }
        if index < word.len() {
            pieces.push((index, &word[index..]));
        }
        pieces
    }

    fn is_not_compound(&self, word: &str) -> bool {
        if self.accepted_compounds.lock().unwrap().contains(word) {
            return false;
        }
        let mut probably_correct = false;
        let char_positions: Vec<usize> = word.char_indices().map(|(i, _)| i).collect();
        for &i in char_positions.iter().skip(2) {
            let first = &word[..i];
            let second = &word[i..];
            let first_lower = first.to_lowercase();
            if NON_SPLITTING_PREFIXES.contains(&first_lower.as_str())
                && self.is_known_word(second)
                && second.len() > first.len()
            {
                probably_correct = true;
                break;
            }
            let tagged = og_tagger::Tagger::tag(self.tagger.as_ref(), &[first, second]);
            let first_adja = tagged[0]
                .readings()
                .iter()
                .any(|r| r.pos_tags().first().is_some_and(|t| t == "adja"));
            let first_num_not_adv = tagged[0]
                .readings()
                .iter()
                .any(|r| r.pos_tags().first().is_some_and(|t| t.starts_with("num:")))
                && !tagged[0]
                    .readings()
                    .iter()
                    .any(|r| r.pos_tags().first().is_some_and(|t| t.starts_with("adv")));
            let second_adj = tagged[1]
                .readings()
                .iter()
                .any(|r| r.pos_tags().first().is_some_and(|t| t.starts_with("adj:")));
            if (first_adja || first_num_not_adv) && second_adj {
                probably_correct = true;
                break;
            }
        }
        if probably_correct {
            self.accepted_compounds
                .lock()
                .unwrap()
                .insert(word.to_string());
        }
        !probably_correct
    }

    fn prune_suggestions(&self, suggestions: &[SuggestedReplacement]) -> Vec<SuggestedReplacement> {
        suggestions
            .iter()
            .filter(|s| {
                let parts: Vec<&str> = s.value().split(' ').collect();
                !(parts.len() > 1 && self.banned_suffixes.contains(parts[1]))
            })
            .cloned()
            .collect()
    }

    fn build_match(
        &self,
        sentence: &AnalyzedSentence,
        text: &str,
        start: usize,
        word: &str,
    ) -> RuleMatch {
        let rel_start = start.saturating_sub(sentence.start());
        let rel_end = rel_start + word.len();
        let mut ctx_start = rel_start.saturating_sub(40);
        while ctx_start < text.len() && !text.is_char_boundary(ctx_start) {
            ctx_start += 1;
        }
        let mut ctx_end = (rel_end + 40).min(text.len());
        while ctx_end > ctx_start && !text.is_char_boundary(ctx_end) {
            ctx_end = text.len().min(ctx_end + 1);
        }
        let rm_rule = og_core::RuleMatchRule::new(self.id(), self.description())
            .with_category(self.category())
            .with_issue_type(self.issue_type().as_str());
        RuleMatch::new(
            format!("Possible spelling mistake found: '{word}'"),
            start,
            word.len(),
            rm_rule,
            og_core::RuleMatchContext::new(
                text[ctx_start..ctx_end].to_string(),
                ctx_start,
                ctx_end - ctx_start,
            ),
        )
        .with_sentence(text.to_string())
        .with_replacements(
            self.prune_suggestions(&[SuggestedReplacement::new(word.to_lowercase())]),
        )
    }
}

impl Default for PolishSpellerRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for PolishSpellerRule {
    fn id(&self) -> &str {
        MORFOLOGIK_RULE_PL_PL
    }

    fn description(&self) -> &str {
        "Possible spelling mistake"
    }

    fn category(&self) -> og_core::Category {
        og_core::Category::new("TYPOS", "Possible Typo")
            .with_description("Possible spelling mistake")
    }

    fn issue_type(&self) -> og_core::IssueType {
        og_core::IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let text = sentence.text();
        let mut out = Vec::new();
        for token in sentence.non_whitespace_tokens() {
            let word = token.token().token();
            if token.has_pos_tag("SENT_START") || token.is_sentence_start() {
                continue;
            }
            if token.is_ignored_by_speller() || token.is_immunized() {
                continue;
            }
            if !word.chars().any(|c| c.is_alphabetic()) {
                continue;
            }
            if word.contains('.') && word.chars().filter(|c| *c == '.').count() > 1 {
                continue;
            }
            let hyphen_tokenizing = word.contains('-') && Self::has_tokenizing_prefix(word);
            if hyphen_tokenizing {
                for (rel, piece) in Self::quasi_niby_pieces(word) {
                    if piece.is_empty() || self.is_known_word(piece) {
                        continue;
                    }
                    if self.prohibited.contains(piece) || self.is_not_compound(piece) {
                        out.push(self.build_match(
                            sentence,
                            text,
                            token.token().start() + rel,
                            piece,
                        ));
                    }
                }
                continue;
            }
            if self.prohibited.contains(word)
                || !self.is_known_word(word) && self.is_not_compound(word)
            {
                out.push(self.build_match(sentence, text, token.token().start(), word));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches_for(text: &str) -> Vec<RuleMatch> {
        let sentence =
            crate::pl::test_support::PolishPipeline::shared().analyze_range(text, 0, text.len());
        PolishSpellerRule::shared().match_sentence(&sentence)
    }

    #[test]
    fn java_pins_example_pair_flags_the_misspelling() {
        let matches = matches_for("To jest zdanie z bledem.");
        assert!(
            matches
                .iter()
                .any(|m| m.length() == "bledem".len() && m.offset() > 10),
            "{matches:?}"
        );
        assert!(matches_for("To jest zdanie z b\u{142}\u{119}dem.").is_empty());
    }

    #[test]
    fn java_pins_compound_adjectives_are_not_flagged() {
        assert!(matches_for("To jest bia\u{142}ozielony dach.").is_empty());
    }

    #[test]
    fn java_pins_non_splitting_prefixes_are_not_flagged() {
        assert!(matches_for("To jest supernowoczesny sprz\u{119}t.").is_empty());
    }

    #[test]
    fn java_pins_quasi_and_niby_pieces_are_checked_separately() {
        assert!(
            matches_for("On uprawia quasi-nauk\u{119}.").is_empty(),
            "the known stem after quasi- is not flagged as a whole word"
        );
        let matches = matches_for("On uprawia quasi-naukw\u{119}.");
        assert!(
            matches.iter().any(|m| m.length() == "naukw\u{119}".len()),
            "the unknown piece after quasi- is flagged at its own span: {matches:?}"
        );
    }

    #[test]
    fn oracle_seeds_from_the_hunspell_pl_pl_fsa_and_exact_case() {
        let rule = PolishSpellerRule::shared();
        assert!(
            rule.oracle_size() > 100_000,
            "pl_PL.hunspell FSA must load: {}",
            rule.oracle_size()
        );
        assert!(rule.is_known_word("kobieta"));
        assert!(rule.is_known_word("b\u{142}\u{119}dem".to_string().as_str()));
        assert!(!rule.is_known_word("bledem"));
        assert!(
            rule.is_known_word("Bledem"),
            "Bledem is in pl_PL as its capitalized form"
        );
    }

    #[test]
    fn java_pins_banned_suffixes_are_pruned_from_suggestions() {
        let rule = PolishSpellerRule::shared();
        let suggestions: Vec<SuggestedReplacement> = ["dobry em", "kobieta", "z\u{142}y pies"]
            .iter()
            .map(|s| SuggestedReplacement::new(*s))
            .collect();
        let pruned = rule.prune_suggestions(&suggestions);
        let values: Vec<&str> = pruned.iter().map(|s| s.value()).collect();
        assert!(values.contains(&"kobieta"), "{values:?}");
        assert!(
            !values.iter().any(|v| v.ends_with(" em")),
            "the non-word suffix em is pruned: {values:?}"
        );
    }
}
