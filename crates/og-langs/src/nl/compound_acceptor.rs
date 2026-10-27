use og_spell::Dictionary;
use std::collections::HashSet;
use std::sync::Arc;

const MAX_WORD_SIZE: usize = 35;

const NO_S: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl/compound_acceptor/no_s.txt"
);
const NEEDS_S: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl/compound_acceptor/needs_s.txt"
);
const DIRECTIONS: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl/compound_acceptor/directions.txt"
);
const ALWAYS_NEEDS_S: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl/compound_acceptor/always_needs_s.txt"
);
const ALWAYS_NEEDS_HYPHEN: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl/compound_acceptor/always_needs_hyphen.txt"
);
const PART1_EXCEPTIONS: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl/compound_acceptor/part1_exceptions.txt"
);
const PART2_EXCEPTIONS: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl/compound_acceptor/part2_exceptions.txt"
);
const ACRONYM_EXCEPTIONS: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl/compound_acceptor/acronym_exceptions.txt"
);

const ALWAYS_NEEDS_HET: &[&str] = &["patroon", "punt", "gemaal", "weer", "kussen", "deel"];
const ALWAYS_NEEDS_DE: &[&str] = &["keten", "boor", "dans"];
const ALWAYS_NEEDS_MRV: &[&str] = &["pies", "koeken", "heden"];

const COLLIDING_VOWELS: &[&str] = &[
    "aa", "ae", "ai", "au", "ee", "ée", "ei", "éi", "eu", "éu", "ie", "ii", "ij", "oe", "oi", "oo",
    "ou", "ui", "uu",
];

pub(crate) fn always_needs_het(word: &str) -> bool {
    ALWAYS_NEEDS_HET.contains(&word)
}

pub(crate) fn always_needs_de(word: &str) -> bool {
    ALWAYS_NEEDS_DE.contains(&word)
}

pub(crate) fn always_needs_mrv(word: &str) -> bool {
    ALWAYS_NEEDS_MRV.contains(&word)
}

pub(crate) fn compound_part2_is_pinned(word: &str) -> bool {
    always_needs_het(word) || always_needs_de(word) || always_needs_mrv(word)
}

fn load_words(data: &str) -> HashSet<&'static str> {
    data.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let leaked: &'static str = og_tagger::morfologik::intern_string(line.to_string());
            leaked
        })
        .collect()
}

fn acronym_stem_length(part1: &str, uppercase_only: bool) -> Option<usize> {
    let stem = part1.strip_suffix('-')?;
    let mut count = 0usize;
    for c in stem.chars() {
        if !c.is_ascii_alphabetic() || (uppercase_only && !c.is_ascii_uppercase()) {
            return None;
        }
        count += 1;
    }
    (2..=4).contains(&count).then_some(count)
}

fn special_acronym_ok(word: &str, exceptions: &HashSet<&str>) -> bool {
    acronym_stem_length(word, false).is_some() && {
        let stem = word.strip_suffix('-').unwrap_or("");
        exceptions.contains(stem)
    }
}

fn uppercase_acronym_ok(word: &str, exceptions: &HashSet<&str>) -> bool {
    acronym_stem_length(word, true).is_some() && {
        let stem = word.strip_suffix('-').unwrap_or("");
        let upper = stem.to_uppercase();
        !exceptions.iter().any(|e| e.to_uppercase() == upper)
    }
}

fn is_normal_case(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {
            chars.all(|c| c.is_ascii_lowercase() || c == 'é')
        }
        _ => false,
    }
}

fn has_colliding_vowels(part1: &str, part2: &str) -> bool {
    let Some(char1) = part1.chars().last() else {
        return false;
    };
    let Some(char2) = part2.chars().next() else {
        return false;
    };
    let mut vowels = String::new();
    vowels.push(char1);
    vowels.push(char2);
    COLLIDING_VOWELS.contains(&vowels.to_lowercase().as_str())
}

pub struct CompoundAcceptor {
    no_s: HashSet<&'static str>,
    needs_s: HashSet<&'static str>,
    geographical_directions: HashSet<&'static str>,
    always_needs_s: HashSet<&'static str>,
    always_needs_hyphen: HashSet<&'static str>,
    part1_exceptions: HashSet<&'static str>,
    part2_exceptions: HashSet<&'static str>,
    acronym_exceptions: HashSet<&'static str>,
    lookup: Arc<super::tagger::WordLookup>,
    spelling: Option<Arc<Dictionary>>,
}

pub fn shared_compound_acceptor() -> Arc<CompoundAcceptor> {
    static CACHE: og_core::releasable::Releasable<CompoundAcceptor> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || Some(Arc::new(CompoundAcceptor::new())))
        .expect("compound acceptor")
}

impl CompoundAcceptor {
    fn new() -> Self {
        Self {
            no_s: load_words(NO_S),
            needs_s: load_words(NEEDS_S),
            geographical_directions: load_words(DIRECTIONS),
            always_needs_s: load_words(ALWAYS_NEEDS_S),
            always_needs_hyphen: load_words(ALWAYS_NEEDS_HYPHEN),
            part1_exceptions: load_words(PART1_EXCEPTIONS),
            part2_exceptions: load_words(PART2_EXCEPTIONS),
            acronym_exceptions: load_words(ACRONYM_EXCEPTIONS),
            lookup: super::tagger::WordLookup::shared(),
            spelling: super::speller::shared_speller_dictionary(),
        }
    }

    pub fn accept_compound(&self, word: &str) -> bool {
        if word.chars().count() > MAX_WORD_SIZE {
            return false;
        }
        self.split_word(word).is_some()
    }

    pub fn get_parts(&self, word: &str) -> Option<(String, String)> {
        self.split_word(word)
    }

    fn split_word(&self, word: &str) -> Option<(String, String)> {
        let char_count = word.chars().count();
        if char_count > MAX_WORD_SIZE {
            return None;
        }
        let boundaries: Vec<usize> = word
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(word.len()))
            .collect();
        for &split in boundaries.iter().take(char_count.saturating_sub(3)).skip(3) {
            let (part1, part2) = (&word[..split], &word[split..]);
            if part1 != part2 && self.accept_compound_parts(part1, part2) {
                return Some((part1.to_string(), part2.to_string()));
            }
        }
        None
    }

    fn accept_compound_parts(&self, part1: &str, part2: &str) -> bool {
        let part1lc = part1.to_lowercase();
        if part1.ends_with('s')
            && !self.part1_exceptions.contains(&part1[..part1.len() - 1])
            && !self.always_needs_s.contains(part1)
            && !self.no_s.contains(part1)
            && !part1.contains('-')
        {
            for suffix in &self.always_needs_s {
                if part1lc.ends_with(suffix) {
                    return self.is_noun(part2)
                        && self.is_existing_word(&part1lc[..part1lc.len() - 1])
                        && self.spelling_ok(part2);
                }
            }
            return self.needs_s.contains(part1lc.as_str())
                && self.is_noun(part2)
                && self.spelling_ok(&part1[..part1.len() - 1])
                && self.spelling_ok(part2);
        }
        if self.geographical_directions.contains(part1) {
            return self.is_geographical_compound(part2);
        }
        if part1.ends_with('-') {
            return (self.acronym_ok(part1) || self.always_needs_hyphen.contains(part1lc.as_str()))
                && self.spelling_ok(part2);
        }
        if let Some(stripped) = part2.strip_prefix('-') {
            return self.no_s.contains(part1lc.as_str())
                && self.is_noun(stripped)
                && self.spelling_ok(part1)
                && self.spelling_ok(stripped)
                && has_colliding_vowels(part1, stripped);
        }
        (self.no_s.contains(part1lc.as_str()) || self.part1_exceptions.contains(part1lc.as_str()))
            && self.is_noun(part2)
            && self.spelling_ok(part1)
            && !has_colliding_vowels(part1, part2)
    }

    fn is_noun(&self, word: &str) -> bool {
        self.lookup
            .lookup(word)
            .iter()
            .any(|(_, tag)| tag.starts_with("ZNW") && !self.part2_exceptions.contains(word))
    }

    fn is_existing_word(&self, word: &str) -> bool {
        !self.lookup.lookup(word).is_empty()
    }

    fn is_geographical_compound(&self, word: &str) -> bool {
        self.lookup
            .lookup(word)
            .iter()
            .any(|(_, tag)| tag.starts_with("ENM:LOC"))
    }

    fn spelling_ok(&self, word: &str) -> bool {
        if !is_normal_case(word) {
            return false;
        }
        match &self.spelling {
            Some(dict) => dict.contains(&word.to_lowercase()),
            None => true,
        }
    }

    fn acronym_ok(&self, non_compound: &str) -> bool {
        if acronym_stem_length(non_compound, true).is_some() {
            uppercase_acronym_ok(non_compound, &self.acronym_exceptions)
        } else {
            special_acronym_ok(non_compound, &self.acronym_exceptions)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acceptor() -> &'static CompoundAcceptor {
        static ACCEPTOR: std::sync::OnceLock<CompoundAcceptor> = std::sync::OnceLock::new();
        ACCEPTOR.get_or_init(CompoundAcceptor::new)
    }

    #[test]
    fn java_pins_accepted_and_rejected_compounds() {
        assert!(acceptor().accept_compound("oorlogsbeker"));
        assert!(acceptor().accept_compound("beoordelingsgeschiedenis"));
        assert!(acceptor().accept_compound("IRA-akkoord"));
        assert!(!acceptor().accept_compound("beoordelinggeschiedenis"));
        assert!(!acceptor().accept_compound("gastkritiekgeschiedenis"));
        assert!(!acceptor().accept_compound("haarhalfbergnacht"));
        assert!(!acceptor().accept_compound("vriendenvrienden"));
        assert!(!acceptor().accept_compound("politieeenheid"));
    }

    #[test]
    fn get_parts_returns_the_first_accepted_split() {
        assert_eq!(
            acceptor().get_parts("beoordelingsgeschiedenis"),
            Some(("beoordelings".to_string(), "geschiedenis".to_string()))
        );
        assert_eq!(
            acceptor().get_parts("Zuidoost-Gouda"),
            Some(("Zuidoost-".to_string(), "Gouda".to_string()))
        );
        assert_eq!(
            acceptor().get_parts("havermout"),
            Some(("haver".to_string(), "mout".to_string()))
        );
    }

    #[test]
    fn acronym_exceptions_block_accepted_acronyms() {
        let exceptions = load_words(ACRONYM_EXCEPTIONS);
        assert!(uppercase_acronym_ok("IRA-", &exceptions));
        assert!(!uppercase_acronym_ok("AIDS-", &exceptions));
    }
}
