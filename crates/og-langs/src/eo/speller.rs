use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchKind,
    RuleMatchRule, SuggestedReplacement,
};
use og_spell::Dictionary;
use og_spell::suggestion::SuggestionEngine;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

pub const HUNSPELL_RULE_EO: &str = "HUNSPELL_RULE";

struct AffixRule {
    name: char,
    is_prefix: bool,
    strip: String,
    add: String,
    cont: Vec<char>,
}

pub struct EoHunspell {
    entries: HashMap<String, Vec<String>>,
    rules: Vec<AffixRule>,
    suffix_by_last: HashMap<char, Vec<usize>>,
    prefix_by_first: HashMap<char, Vec<usize>>,
    zero_add: Vec<usize>,
    need_affix: Option<char>,
}

enum CapType {
    NoCap,
    InitCap,
    AllCap,
    Huh,
}

fn cap_type(chars: &[char]) -> CapType {
    let upper: Vec<usize> = chars
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_uppercase())
        .map(|(i, _)| i)
        .collect();
    if upper.is_empty() {
        CapType::NoCap
    } else if upper.len() == chars.len() {
        CapType::AllCap
    } else if upper == vec![0] {
        CapType::InitCap
    } else {
        CapType::Huh
    }
}

fn lowered(chars: &[char]) -> Vec<char> {
    chars.iter().flat_map(|c| c.to_lowercase()).collect()
}

fn init_capped(chars: &[char]) -> Vec<char> {
    let mut out = lowered(chars);
    if let Some(first) = out.first_mut() {
        let mut it = first.to_uppercase();
        if let (Some(up), None) = (it.next(), it.next()) {
            *first = up;
        }
    }
    out
}

impl EoHunspell {
    pub fn load(aff_path: &Path, dic_path: &Path) -> Option<Self> {
        let aff = std::fs::read_to_string(aff_path).ok()?;
        let dic = std::fs::read_to_string(dic_path).ok()?;
        Self::parse(&aff, &dic)
    }

    fn parse(aff: &str, dic: &str) -> Option<Self> {
        let mut need_affix = None;
        let mut rules: Vec<AffixRule> = Vec::new();
        for raw in aff.lines() {
            if raw.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = raw.split_whitespace().collect();
            if fields.is_empty() {
                continue;
            }
            match fields[0] {
                "NEEDAFFIX" => {
                    need_affix = fields.get(1).and_then(|f| f.chars().next());
                }
                "PFX" | "SFX" => {
                    let Some(name) = fields.get(1).and_then(|f| f.chars().next()) else {
                        continue;
                    };
                    if fields.len() >= 4 && fields[3].parse::<usize>().is_ok() {
                        continue;
                    }
                    if fields.len() < 4 {
                        continue;
                    }
                    let is_prefix = fields[0] == "PFX";
                    let strip = if fields[2] == "0" {
                        String::new()
                    } else {
                        fields[2].to_string()
                    };
                    let (add, cont) = match fields[3].split_once('/') {
                        Some((a, f)) => (a.to_string(), f.chars().collect::<Vec<char>>()),
                        None => (fields[3].to_string(), Vec::new()),
                    };
                    let add = if add == "0" { String::new() } else { add };
                    rules.push(AffixRule {
                        name,
                        is_prefix,
                        strip,
                        add,
                        cont,
                    });
                }
                _ => {}
            }
        }
        let mut entries: HashMap<String, Vec<String>> = HashMap::new();
        for (idx, raw) in dic.lines().enumerate() {
            if idx == 0 {
                continue;
            }
            let word_part = raw.split('\t').next().unwrap_or("");
            let (word, flags) = match word_part.find('/') {
                Some(0) | None => (word_part, ""),
                Some(pos) => (&word_part[..pos], &word_part[pos + 1..]),
            };
            if word.is_empty() {
                continue;
            }
            entries
                .entry(word.to_string())
                .or_default()
                .push(flags.to_string());
        }
        if entries.is_empty() {
            return None;
        }
        let mut suffix_by_last: HashMap<char, Vec<usize>> = HashMap::new();
        let mut prefix_by_first: HashMap<char, Vec<usize>> = HashMap::new();
        let mut zero_add = Vec::new();
        for (idx, rule) in rules.iter().enumerate() {
            let indexed = if rule.is_prefix {
                rule.add.chars().next().map(|c| (true, c))
            } else {
                rule.add.chars().next_back().map(|c| (false, c))
            };
            match indexed {
                None => zero_add.push(idx),
                Some((true, first)) => prefix_by_first.entry(first).or_default().push(idx),
                Some((false, last)) => suffix_by_last.entry(last).or_default().push(idx),
            }
        }
        Some(Self {
            entries,
            rules,
            suffix_by_last,
            prefix_by_first,
            zero_add,
            need_affix,
        })
    }

    fn lookup_has(&self, chars: &[char], name: char) -> bool {
        self.entries
            .get(chars.iter().collect::<String>().as_str())
            .is_some_and(|homonyms| homonyms.iter().any(|flags| flags.contains(name)))
    }

    fn standalone_ok(&self, chars: &[char]) -> bool {
        self.entries
            .get(chars.iter().collect::<String>().as_str())
            .is_some_and(|homonyms| homonyms.iter().any(|flags| !self.has_need_affix(flags)))
    }

    fn has_need_affix(&self, flags: &str) -> bool {
        self.need_affix.is_some_and(|n| flags.contains(n))
    }

    fn rule_stem(&self, idx: usize, chars: &[char]) -> Option<Vec<char>> {
        let rule = &self.rules[idx];
        let add: Vec<char> = rule.add.chars().collect();
        let strip: Vec<char> = rule.strip.chars().collect();
        let mut stem = Vec::with_capacity(chars.len() + strip.len());
        if rule.is_prefix {
            if !chars.starts_with(&add) {
                return None;
            }
            stem.extend_from_slice(&strip);
            stem.extend_from_slice(&chars[add.len()..]);
        } else {
            if add.is_empty() {
                stem.extend_from_slice(chars);
            } else {
                if !chars.ends_with(&add) {
                    return None;
                }
                stem.extend_from_slice(&chars[..chars.len() - add.len()]);
            }
            stem.extend_from_slice(&strip);
        }
        Some(stem)
    }

    fn reachable_has(&self, chars: &[char], name: char, depth: u32) -> bool {
        if depth == 0 {
            return false;
        }
        if self.lookup_has(chars, name) {
            return true;
        }
        let mut candidates: Vec<usize> = Vec::new();
        if let Some(last) = chars.last()
            && let Some(list) = self.suffix_by_last.get(last)
        {
            candidates.extend_from_slice(list);
        }
        if let Some(first) = chars.first()
            && let Some(list) = self.prefix_by_first.get(first)
        {
            candidates.extend_from_slice(list);
        }
        candidates.extend_from_slice(&self.zero_add);
        for idx in candidates {
            let rule = &self.rules[idx];
            let Some(stem) = self.rule_stem(idx, chars) else {
                continue;
            };
            if rule.cont.contains(&name) && self.reachable_has(&stem, rule.name, depth - 1) {
                return true;
            }
            // hunspell cross-product: an affix with empty cont classes lets the
            // opposite-direction affix draw its license from the deeper stem.
            if rule.cont.is_empty()
                && self.reachable_has(&stem, rule.name, depth - 1)
                && self.reachable_has(&stem, name, depth - 1)
            {
                return true;
            }
        }
        false
    }

    fn check(&self, chars: &[char]) -> bool {
        if chars.is_empty() {
            return true;
        }
        if self.standalone_ok(chars) {
            return true;
        }
        let mut candidates: Vec<usize> = Vec::new();
        if let Some(last) = chars.last()
            && let Some(list) = self.suffix_by_last.get(last)
        {
            candidates.extend_from_slice(list);
        }
        if let Some(first) = chars.first()
            && let Some(list) = self.prefix_by_first.get(first)
        {
            candidates.extend_from_slice(list);
        }
        candidates.extend_from_slice(&self.zero_add);
        for idx in candidates {
            let rule = &self.rules[idx];
            if rule.cont.contains(&self.need_affix.unwrap_or('\0')) {
                continue;
            }
            let Some(stem) = self.rule_stem(idx, chars) else {
                continue;
            };
            if self.reachable_has(&stem, rule.name, 4) {
                return true;
            }
        }
        false
    }

    pub fn spell(&self, word: &str) -> bool {
        let chars: Vec<char> = word.chars().collect();
        match cap_type(&chars) {
            CapType::NoCap | CapType::Huh => self.check(&chars),
            CapType::InitCap => {
                let lower = lowered(&chars);
                self.check(&chars) || self.check(&lower)
            }
            CapType::AllCap => {
                let lower = lowered(&chars);
                let init = init_capped(&chars);
                self.check(&chars) || self.check(&init) || self.check(&lower)
            }
        }
    }
}

pub struct EsperantoSpellerRule {
    hunspell: Option<Arc<EoHunspell>>,
    dictionary: Arc<Dictionary>,
    suggestion_engine: SuggestionEngine,
    ignored: HashSet<String>,
    category: Category,
}

pub fn eo_resource_dir() -> Option<std::path::PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("eo/src/main/resources/org/languagetool/resource/eo"))
}

fn shared_hunspell() -> Option<Arc<EoHunspell>> {
    static CACHE: og_core::releasable::Releasable<EoHunspell> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&CACHE, || {
        let dir = eo_resource_dir()?;
        EoHunspell::load(&dir.join("hunspell/eo.aff"), &dir.join("hunspell/eo.dic")).map(Arc::new)
    })
}

fn speller_dictionary(resource_dir: &Path) -> Dictionary {
    let mut dict = Dictionary::new();
    if let Ok(extra) = std::fs::read_to_string(resource_dir.join("hunspell/spelling.txt")) {
        for line in extra.lines() {
            let word = line.trim();
            if !word.is_empty() && !word.starts_with('#') {
                dict.add_word(word);
            }
        }
    }
    dict
}

fn ignored_words(resource_dir: &Path) -> HashSet<String> {
    let mut ignored = HashSet::new();
    for file in ["hunspell/ignore.txt", "hunspell/spelling.txt"] {
        if let Ok(words) = std::fs::read_to_string(resource_dir.join(file)) {
            for line in words.lines() {
                let word = line.trim();
                if !word.is_empty() && !word.starts_with('#') {
                    ignored.insert(word.to_string());
                }
            }
        }
    }
    ignored
}

fn is_mixed_case(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first.is_lowercase() {
        return word.chars().any(|c| c.is_uppercase());
    }
    chars.any(|c| c.is_uppercase())
}

impl EsperantoSpellerRule {
    pub fn new() -> Self {
        let resource_dir = eo_resource_dir().unwrap_or_default();
        let dictionary = Arc::new(speller_dictionary(&resource_dir));
        Self {
            hunspell: shared_hunspell(),
            suggestion_engine: SuggestionEngine::new(std::sync::Arc::clone(&dictionary)),
            ignored: ignored_words(&resource_dir),
            dictionary,
            category: Category::new("TYPOS", "Possible Typo")
                .with_description("Ebla mistajpa\u{135}o"),
        }
    }

    pub fn dictionary_size(&self) -> usize {
        self.dictionary.len()
    }

    fn is_ignored(&self, word: &str) -> bool {
        self.ignored.contains(word)
            || (!is_mixed_case(word) && self.ignored.contains(&word.to_lowercase()))
    }

    fn is_known_word(&self, word: &str) -> bool {
        if !word.chars().any(|c| c.is_alphabetic()) {
            return true;
        }
        if word.chars().count() == 1 {
            return true;
        }
        if self.is_ignored(word) {
            return true;
        }
        if word.contains('/') || word.contains('@') {
            return true;
        }
        let lowercase = word.to_lowercase();
        if self.dictionary.contains(word) || self.dictionary.contains(&lowercase) {
            return true;
        }
        match &self.hunspell {
            Some(hunspell) => hunspell.spell(word) || hunspell.spell(&lowercase),
            None => false,
        }
    }
}

impl Default for EsperantoSpellerRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for EsperantoSpellerRule {
    fn id(&self) -> &str {
        HUNSPELL_RULE_EO
    }

    fn description(&self) -> &str {
        "Ebla mistajpa\u{135}o"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        for token_readings in sentence.tokens() {
            if token_readings.is_whitespace() {
                continue;
            }
            let text = token_readings.token().token();
            if text == "<S>" || token_readings.is_sentence_start() {
                continue;
            }
            if token_readings.is_ignored_by_speller() || token_readings.is_immunized() {
                continue;
            }
            if !text.chars().any(|c| c.is_alphabetic()) {
                continue;
            }
            if text.contains('.') && text.chars().filter(|c| *c == '.').count() > 1 {
                continue;
            }
            if self.is_known_word(text) {
                continue;
            }
            let start = token_readings.token().start();
            let end = token_readings.token().end();
            let suggestions = self.suggestion_engine.suggest(text, 5);
            let replacements: Vec<SuggestedReplacement> = suggestions
                .into_iter()
                .map(SuggestedReplacement::new)
                .collect();
            let rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category.clone())
                .with_issue_type(IssueType::Misspelling.as_str());
            matches.push(
                RuleMatch::new(
                    "Ebla mistajpa\u{135}o trovita",
                    start,
                    end - start,
                    rule,
                    RuleMatchContext::new(sentence.text().to_string(), 0, sentence.text().len()),
                )
                .with_short_message("Mistajpa\u{135}o")
                .with_sentence(sentence.text().to_string())
                .with_replacements(replacements)
                .with_match_type(RuleMatchKind::UnknownWord),
            );
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> Option<std::path::PathBuf> {
        eo_resource_dir().filter(|d| d.join("hunspell/eo.dic").exists())
    }

    fn known(rule: &EsperantoSpellerRule, words: &[&str]) {
        for word in words {
            assert!(rule.is_known_word(word), "'{word}' must be known");
        }
    }

    #[test]
    fn hunspell_dictionary_loads() {
        let Some(_) = dir() else {
            return;
        };
        let hunspell = shared_hunspell();
        assert!(hunspell.is_some(), "eo.aff + eo.dic must load");
        assert!(hunspell.unwrap().spell("esperanto"));
    }

    #[test]
    fn zero_add_affix_chains_license_the_inflected_forms() {
        let Some(_) = dir() else {
            return;
        };
        let hunspell = shared_hunspell().unwrap();
        assert!(!hunspell.spell("est"));
        for word in [
            "estas",
            "estis",
            "estos",
            "estus",
            "estu",
            "esti",
            "mondo",
            "mondaj",
            "mondon",
            "mondojn",
            "ludoj",
            "evento",
            "granda",
            "malgranda",
            "malgrandaj",
            "mondego",
            "algluu",
            "kontrolu",
            "kontrolendan",
            "atentigas",
            "rimarkis",
            "ekzemplon",
        ] {
            assert!(
                hunspell.spell(word),
                "'{word}' must be accepted via affix chains"
            );
        }
        assert!(!hunspell.spell("mondfnord"));
        assert!(!hunspell.spell("plur-sporta"));
        assert!(!hunspell.spell("erraroj"));
        assert!(!hunspell.spell("ankaux"));
    }

    #[test]
    fn everyday_words_are_known_and_x_system_is_not() {
        let Some(_) = dir() else {
            return;
        };
        let rule = EsperantoSpellerRule::new();
        known(
            &rule,
            &[
                "La",
                "Mondaj",
                "Ludoj",
                "de",
                "estis",
                "evento",
                "Algluu",
                "vian",
                "kontrolendan",
                "tekston",
                "\u{109}i",
                "tie",
                "rimarkis",
                "gramatikaj",
                "eraro",
                "frazo",
                "atentigas",
                "pri",
                "literumaj",
                "kiel",
            ],
        );
        assert!(!rule.is_known_word("erraroj"));
        assert!(!rule.is_known_word("plur-sporta"));
    }

    #[test]
    fn spelling_txt_words_are_accepted() {
        let Some(_) = dir() else {
            return;
        };
        let rule = EsperantoSpellerRule::new();
        known(&rule, &["Lingvoilo", "Vikivortarojn", "mojosa", "dank'"]);
    }

    #[test]
    fn java_test_sentence_yields_the_single_hunspell_match() {
        let Some(_) = dir() else {
            return;
        };
        let text = "La Mondaj Ludoj de 2013 estis plur-sporta evento...";
        let sentence = crate::eo::example_suite::eo_pipeline().analyze_range(text, 0, text.len());
        let rule = EsperantoSpellerRule::new();
        let spans: Vec<(usize, usize)> = rule
            .match_sentence(&sentence)
            .iter()
            .map(|m| (m.offset(), m.length()))
            .collect();
        let words: Vec<&str> = spans.iter().map(|(o, l)| &text[*o..*o + *l]).collect();
        assert_eq!(
            words,
            vec!["plur-sporta"],
            "EsperantoTest.java:41-44 expects exactly one HUNSPELL_RULE match on this sentence"
        );
    }
}
