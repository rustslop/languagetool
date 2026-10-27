use og_core::{
    AnalyzedSentence, Category, IssueType, Rule, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};
use og_languagemodel::LanguageModel;
use std::collections::HashMap;
use std::sync::Arc;

const MIN_COVERAGE: f32 = 0.5;
const MIN_PROB: f64 = 0.0;

#[derive(Clone)]
struct ConfusionString {
    string: String,
    description: Option<String>,
}

#[derive(Clone)]
struct ConfusionPair {
    terms: Vec<ConfusionString>,
    uppercase_first_char_terms: Vec<ConfusionString>,
    factor: i64,
    bidirectional: bool,
}

pub struct ConfusionRule {
    id_prefix: String,
    word_to_pairs: HashMap<String, Vec<ConfusionPair>>,
    model: Arc<dyn LanguageModel>,
}

fn load_confusion_pairs(data: &str) -> HashMap<String, Vec<ConfusionPair>> {
    let mut map: HashMap<String, Vec<ConfusionPair>> = HashMap::new();
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let without_comment = line.split('#').next().unwrap_or("").trim();
        let bidirectional = !without_comment.contains("->");
        let split: Vec<String> = without_comment
            .split(';')
            .flat_map(|seg| seg.split("->"))
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        if split.len() < 3 {
            continue;
        }
        let Ok(factor) = split[split.len() - 1].replace(',', "").parse::<i64>() else {
            continue;
        };
        let mut terms = Vec::new();
        for part in &split[..split.len() - 1] {
            let sub: Vec<&str> = part.split('|').collect();
            terms.push(ConfusionString {
                string: sub[0].trim().to_string(),
                description: sub.get(1).map(|d| d.trim().to_string()),
            });
        }
        if terms.len() != 2 {
            continue;
        }
        let uppercase_terms: Vec<ConfusionString> = terms
            .iter()
            .map(|t| ConfusionString {
                string: uppercase_first(&t.string),
                description: t.description.clone(),
            })
            .collect();
        let pair = ConfusionPair {
            terms: terms.clone(),
            uppercase_first_char_terms: uppercase_terms,
            factor,
            bidirectional,
        };
        for term in &terms {
            map.entry(term.string.clone())
                .or_default()
                .push(pair.clone());
        }
    }
    map
}

fn uppercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn lowercase_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn is_real_word(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| c.is_alphabetic())
}

struct GoogleTokens {
    tokens: Vec<String>,
    starts: Vec<usize>,
    ends: Vec<usize>,
}

fn google_tokens(text: &str, add_start: bool) -> GoogleTokens {
    let mut tokens = Vec::new();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    if add_start {
        tokens.push("_START_".to_string());
        starts.push(0);
        ends.push(0);
    }
    let mut offset = 0usize;
    for part in text.split(|c: char| c.is_whitespace()) {
        let part_start = offset + text[offset..].find(part).unwrap_or(0);
        let cleaned: String = part.chars().filter(|c| !c.is_whitespace()).collect();
        let token = if cleaned == "\u{2019}" {
            "'".to_string()
        } else {
            cleaned
        };
        if !token.is_empty() {
            tokens.push(token);
            starts.push(part_start);
            ends.push(part_start + part.len());
        }
        offset = part_start + part.len();
    }
    GoogleTokens {
        tokens,
        starts,
        ends,
    }
}

fn context(
    tokens: &GoogleTokens,
    pos: usize,
    replacement: &[&str],
    to_left: usize,
    to_right: usize,
) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    let mut added = 0usize;
    let mut i = 1usize;
    while added < to_left {
        if pos < i {
            result.clear();
            break;
        }
        let t = &tokens.tokens[pos - i];
        if t.chars().all(|c| c.is_whitespace()) {
            i += 1;
            continue;
        }
        result.insert(0, t.clone());
        added += 1;
        i += 1;
    }
    for r in replacement {
        result.push((*r).to_string());
    }
    let mut added = 0usize;
    let mut i = 1usize;
    while added < to_right {
        let idx = pos + i;
        if idx >= tokens.tokens.len() {
            result.push("_END_".to_string());
            break;
        }
        let t = &tokens.tokens[idx];
        if t.chars().all(|c| c.is_whitespace()) {
            i += 1;
            continue;
        }
        result.push(t.clone());
        added += 1;
        i += 1;
    }
    result
}

fn get3gram_probability(
    model: &dyn LanguageModel,
    tokens: &GoogleTokens,
    pos: usize,
    term: &str,
) -> f64 {
    let left = context(tokens, pos, &[term], 0, 2);
    let middle = context(tokens, pos, &[term], 1, 1);
    let right = context(tokens, pos, &[term], 2, 0);
    let pl = model.get_probability(&left.iter().map(String::as_str).collect::<Vec<_>>());
    let pm = model.get_probability(&middle.iter().map(String::as_str).collect::<Vec<_>>());
    let pr = model.get_probability(&right.iter().map(String::as_str).collect::<Vec<_>>());
    if pl.coverage < MIN_COVERAGE && pm.coverage < MIN_COVERAGE && pr.coverage < MIN_COVERAGE {
        return 0.0;
    }
    pl.prob * pm.prob * pr.prob
}

fn clean_id(id: &str) -> String {
    id.to_uppercase()
        .replace('Ä', "AE")
        .replace('Ü', "UE")
        .replace('Ö', "OE")
}

impl ConfusionRule {
    pub fn new_english(model: Arc<dyn LanguageModel>) -> Self {
        let data = confusion_data("en", &["confusion_sets.txt"]);
        Self {
            id_prefix: "EN_CONFUSION_RULE".to_string(),
            word_to_pairs: load_confusion_pairs(&data),
            model,
        }
    }

    pub fn new_german(model: Arc<dyn LanguageModel>) -> Self {
        let data = confusion_data("de", &["confusion_sets.txt"]);
        Self {
            id_prefix: "DE_CONFUSION_RULE".to_string(),
            word_to_pairs: load_confusion_pairs(&data),
            model,
        }
    }

    pub fn new_spanish(model: Arc<dyn LanguageModel>) -> Self {
        let data = confusion_data("es", &["confusion_sets.txt"]);
        Self {
            id_prefix: "ES_CONFUSION_RULE".to_string(),
            word_to_pairs: load_confusion_pairs(&data),
            model,
        }
    }

    pub fn new_russian(model: Arc<dyn LanguageModel>) -> Self {
        let data = confusion_data("ru", &["confusion_sets.txt"]);
        Self {
            id_prefix: "CONFUSION_RULE".to_string(),
            word_to_pairs: load_confusion_pairs(&data),
            model,
        }
    }

    pub fn new_italian(model: Arc<dyn LanguageModel>) -> Self {
        let data = confusion_data("it", &["confusion_sets.txt"]);
        Self {
            id_prefix: "CONFUSION_RULE".to_string(),
            word_to_pairs: load_confusion_pairs(&data),
            model,
        }
    }

    pub fn new_arabic(model: Arc<dyn LanguageModel>) -> Self {
        let data = confusion_data("ar", &["confusion_sets.txt"]);
        Self {
            id_prefix: "CONFUSION_RULE".to_string(),
            word_to_pairs: load_confusion_pairs(&data),
            model,
        }
    }

    pub fn new_english_for_l2(model: Arc<dyn LanguageModel>, mother_tongue: &str) -> Self {
        let data = confusion_data("en", &[&format!("confusion_sets_l2_{mother_tongue}.txt")]);
        Self {
            id_prefix: format!("EN_FOR_{mother_tongue}_SPEAKERS_FALSE_FRIENDS").to_uppercase(),
            word_to_pairs: load_confusion_pairs(&data),
            model,
        }
    }

    pub fn pair_count(&self) -> usize {
        self.word_to_pairs.len()
    }
}

fn confusion_data(short_code: &str, files: &[&str]) -> String {
    let mut data = String::new();
    if let Some(root) = crate::engine::find_lt_resource_dir() {
        for file in files {
            let path = root.join(format!(
                "{short_code}/src/main/resources/org/languagetool/resource/{short_code}/{file}"
            ));
            if let Ok(content) = std::fs::read_to_string(&path) {
                data.push_str(&content);
                data.push('\n');
            }
        }
    }
    data
}

pub(crate) fn confusion_term_pairs(
    short_code: &str,
    files: &[&str],
) -> Vec<(String, Option<String>, String, Option<String>)> {
    let map = load_confusion_pairs(&confusion_data(short_code, files));
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for pairs in map.values() {
        for pair in pairs {
            if pair.terms.len() != 2 {
                continue;
            }
            let key = (pair.terms[0].string.clone(), pair.terms[1].string.clone());
            if seen.insert(key) {
                out.push((
                    pair.terms[0].string.clone(),
                    pair.terms[0].description.clone(),
                    pair.terms[1].string.clone(),
                    pair.terms[1].description.clone(),
                ));
            }
        }
    }
    out
}

impl Rule for ConfusionRule {
    fn id(&self) -> &str {
        &self.id_prefix
    }

    fn description(&self) -> &str {
        "Words that can easily be confused (using ngram statistics)"
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Possible Typo")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let text = sentence.text();
        let gt = google_tokens(text, true);
        if gt.tokens.len() == 2 {
            return Vec::new();
        }
        let mut matches = Vec::new();
        let mut real_word_before = false;
        for pos in 0..gt.tokens.len() {
            let token = &gt.tokens[pos];
            let mut uppercase = false;
            let mut pairs: Option<&Vec<ConfusionPair>> = self.word_to_pairs.get(token);
            if pairs.is_none()
                && !real_word_before
                && token.chars().next().is_some_and(|c| c.is_uppercase())
                && is_real_word(token)
            {
                pairs = self.word_to_pairs.get(&lowercase_first(token));
                uppercase = true;
            }
            if is_real_word(token) {
                real_word_before = true;
            }
            let Some(pairs) = pairs else {
                continue;
            };
            for pair in pairs {
                let set = if uppercase {
                    &pair.uppercase_first_char_terms
                } else {
                    &pair.terms
                };
                let other = set
                    .iter()
                    .find(|s| !s.string.eq_ignore_ascii_case(token))
                    .cloned();
                let Some(other) = other else {
                    continue;
                };
                let p1 = get3gram_probability(self.model.as_ref(), &gt, pos, token);
                let p2 = get3gram_probability(self.model.as_ref(), &gt, pos, &other.string);
                let better = p2 >= MIN_PROB
                    && p1 <= f64::MAX / pair.factor as f64
                    && p2 > p1 * pair.factor as f64;
                if !better {
                    continue;
                }
                if !pair.bidirectional && other.string.eq_ignore_ascii_case(&set[0].string) {
                    continue;
                }
                if pos > 0
                    && gt.tokens[pos - 1] == "_START_"
                    && gt.tokens.get(pos + 1).is_some_and(|t| is_real_word(t))
                {
                    continue;
                }
                let from_text = set
                    .iter()
                    .find(|s| s.string.eq_ignore_ascii_case(token))
                    .cloned()
                    .unwrap_or_else(|| set[0].clone());
                let message = match (
                    from_text.description.as_deref(),
                    other.description.as_deref(),
                ) {
                    (Some(d1), Some(d2)) => format!(
                        "Statistics suggest '{}' ({}), not '{}' ({}).",
                        other.string, d2, from_text.string, d1
                    ),
                    (Some(d1), None) => format!(
                        "Statistics suggest '{}', not '{}' ({}).",
                        other.string, from_text.string, d1
                    ),
                    (None, Some(d2)) => format!(
                        "Statistics suggest '{}' ({}), not '{}'.",
                        other.string, d2, from_text.string
                    ),
                    (None, None) => format!(
                        "Statistics suggest '{}', not '{}'.",
                        other.string, from_text.string
                    ),
                };
                let term1 = clean_id(&pair.terms[0].string);
                let term2 = clean_id(&pair.terms[1].string);
                let rm_rule = RuleMatchRule::new(
                    format!("{}_{}_{}", self.id_prefix, term1, term2),
                    format!("{} vs. {}", pair.terms[0].string, pair.terms[1].string),
                )
                .with_category(self.category())
                .with_issue_type("misspelling");
                let start = gt.starts[pos];
                let end = gt.ends[pos];
                let ctx_start = start.saturating_sub(40).min(text.len());
                let ctx_end = (end + 40).min(text.len());
                matches.push(
                    RuleMatch::new(
                        &message,
                        start,
                        end.saturating_sub(start),
                        rm_rule,
                        RuleMatchContext::new(
                            text[ctx_start..ctx_end].to_string(),
                            ctx_start,
                            ctx_end - ctx_start,
                        ),
                    )
                    .with_replacements(vec![SuggestedReplacement::new(&other.string)]),
                );
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_languagemodel::DiskLanguageModel;

    fn model() -> Option<Arc<DiskLanguageModel>> {
        let dir = std::path::PathBuf::from("/home/agent/opengrammar-rs/ngram-data/extracted/en");
        if !dir.join("2grams.tsv").exists() {
            return None;
        }
        DiskLanguageModel::from_dir(&dir).map(Arc::new)
    }

    #[test]
    fn confusion_rule_runs_against_real_model() {
        let Some(m) = model() else {
            eprintln!("Skipping: en model data not present");
            return;
        };
        let rule = ConfusionRule::new_english(m);
        assert!(rule.pair_count() > 100, "confusion sets must load");
        let sentence = AnalyzedSentence::new("I except your offer.", 0, 20);
        let matches = rule.match_sentence(&sentence);
        assert!(matches.len() <= 1, "no crash: {matches:?}");
    }

    #[test]
    fn false_friend_rule_loads_l2_data() {
        let dir = std::path::PathBuf::from("/home/agent/opengrammar-rs/ngram-data/extracted/en");
        if !dir.join("2grams.tsv").exists() {
            eprintln!("Skipping: en model data not present");
            return;
        }
        let Some(m) = DiskLanguageModel::from_dir(&dir) else {
            panic!("model must load");
        };
        let rule = ConfusionRule::new_english_for_l2(Arc::new(m), "de");
        assert!(
            rule.pair_count() > 100,
            "l2 german confusion sets must load, got {}",
            rule.pair_count()
        );
    }

    #[test]
    fn german_confusion_rule_loads() {
        let dir = std::path::PathBuf::from("/home/agent/opengrammar-rs/ngram-data/extracted/de");
        if !dir.join("2grams.tsv").exists() {
            eprintln!("Skipping: de model data not present");
            return;
        }
        let Some(m) = DiskLanguageModel::from_dir(&dir) else {
            panic!("de model must load");
        };
        let rule = ConfusionRule::new_german(Arc::new(m));
        assert!(rule.pair_count() >= 70, "german confusion sets must load");
    }

    struct TableModel {
        table: HashMap<String, (f64, f32)>,
    }

    impl LanguageModel for TableModel {
        fn get_count(&self, _tokens: &[&str]) -> u64 {
            0
        }
        fn get_total_token_count(&self) -> u64 {
            0
        }
        fn get_probability(&self, context: &[&str]) -> og_languagemodel::Probability {
            match self.table.get(&context.join(" ")) {
                Some((prob, coverage)) => og_languagemodel::Probability {
                    prob: *prob,
                    coverage: *coverage,
                    total_count: 0,
                },
                None => og_languagemodel::Probability {
                    prob: 0.0,
                    coverage: 1.0,
                    total_count: 0,
                },
            }
        }
    }

    fn table(entries: &[(&str, f64, f32)]) -> Arc<TableModel> {
        Arc::new(TableModel {
            table: entries
                .iter()
                .map(|(k, p, c)| ((*k).to_string(), (*p, *c)))
                .collect(),
        })
    }

    fn rule_from(data: &str, model: Arc<dyn LanguageModel>) -> ConfusionRule {
        ConfusionRule {
            id_prefix: "TEST_CONFUSION".to_string(),
            word_to_pairs: load_confusion_pairs(data),
            model,
        }
    }

    fn except_accept_model() -> Arc<TableModel> {
        table(&[
            ("except money. _END_", 0.01, 1.0),
            ("I except money.", 0.01, 1.0),
            ("_START_ I except", 0.01, 1.0),
            ("accept money. _END_", 0.9, 1.0),
            ("I accept money.", 0.9, 1.0),
            ("_START_ I accept", 0.9, 1.0),
        ])
    }

    fn sentence(text: &str) -> AnalyzedSentence {
        AnalyzedSentence::new(text, 0, text.len())
    }

    #[test]
    fn parses_directions_descriptions_and_factors() {
        let data = "one; two; 3\nuni -> uno; 10\ndesc|first; other|second; 2\n\
                    skipme\nbad; factor\na; b; x\none; two; three; 4\ncomma; separated; 1,000\n";
        let map = load_confusion_pairs(data);
        assert_eq!(map["one"].len(), 1);
        let pair = &map["one"][0];
        assert_eq!(pair.factor, 3);
        assert!(pair.bidirectional);
        assert_eq!(pair.terms[0].string, "one");
        assert_eq!(pair.terms[0].description, None);
        assert_eq!(pair.uppercase_first_char_terms[0].string, "One");

        assert_eq!(map["uni"].len(), 1);
        assert!(!map["uni"][0].bidirectional);
        assert_eq!(map["uni"][0].factor, 10);
        assert_eq!(map["uno"][0].terms[1].string, "uno");

        let described = &map["desc"][0];
        assert_eq!(described.terms[0].description.as_deref(), Some("first"));
        assert_eq!(described.terms[1].description.as_deref(), Some("second"));

        assert_eq!(map["comma"][0].factor, 1000);
        for missing in ["skipme", "bad", "a", "b", "x"] {
            assert!(!map.contains_key(missing), "{missing} must be skipped");
        }
    }

    #[test]
    fn suggests_the_statistically_preferred_term() {
        let rule = rule_from("except; accept; 10\n", except_accept_model());
        let matches = rule.match_sentence(&sentence("I except money."));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 2);
        assert_eq!(matches[0].length(), 6);
        assert_eq!(
            matches[0].message(),
            "Statistics suggest 'accept', not 'except'."
        );
        assert_eq!(matches[0].replacements()[0].value(), "accept");
        assert_eq!(matches[0].rule().id(), "TEST_CONFUSION_EXCEPT_ACCEPT");
        assert_eq!(matches[0].rule().description(), "except vs. accept");
        assert_eq!(matches[0].rule().category().id().as_str(), "TYPOS");
    }

    #[test]
    fn messages_carry_available_descriptions() {
        for (data, expected) in [
            (
                "except|preposition; accept; 10\n",
                "Statistics suggest 'accept', not 'except' (preposition).",
            ),
            (
                "except; accept|verb; 10\n",
                "Statistics suggest 'accept' (verb), not 'except'.",
            ),
            (
                "except|preposition; accept|verb; 10\n",
                "Statistics suggest 'accept' (verb), not 'except' (preposition).",
            ),
        ] {
            let rule = rule_from(data, except_accept_model());
            let matches = rule.match_sentence(&sentence("I except money."));
            assert_eq!(matches.len(), 1, "data: {data}");
            assert_eq!(matches[0].message(), expected);
        }
    }

    #[test]
    fn unidirectional_pairs_only_flag_the_left_side() {
        let model = table(&[
            ("foo the bar.", 0.001, 1.0),
            ("We foo the", 0.001, 1.0),
            ("_START_ We foo", 0.001, 1.0),
            ("bar the bar.", 0.9, 1.0),
            ("We bar the", 0.9, 1.0),
            ("_START_ We bar", 0.9, 1.0),
        ]);
        let rule = rule_from("foo -> bar; 2\n", model);
        let matches = rule.match_sentence(&sentence("We foo the bar."));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "bar");
        assert!(matches[0].message().contains("'bar', not 'foo'"));

        let reversed = rule.match_sentence(&sentence("We bar the foo."));
        assert!(reversed.is_empty(), "the right side never points back");
    }

    #[test]
    fn sentence_start_uppercase_uses_the_capitalized_set() {
        let model = table(&[
            ("Except , money.", 0.01, 1.0),
            ("_START_ Except ,", 0.01, 1.0),
            ("Except", 0.01, 1.0),
            ("Accept , money.", 0.9, 1.0),
            ("_START_ Accept ,", 0.9, 1.0),
            ("Accept", 0.9, 1.0),
        ]);
        let rule = rule_from("except; accept; 10\n", model);
        let matches = rule.match_sentence(&sentence("Except , money."));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].length(), 6);
        assert_eq!(matches[0].replacements()[0].value(), "Accept");
    }

    #[test]
    fn sentence_start_guard_skips_words_followed_by_real_words() {
        let rule = rule_from("except; accept; 10\n", except_accept_model());
        let matches = rule.match_sentence(&sentence("Except money."));
        assert!(matches.is_empty());
    }

    #[test]
    fn uncovered_contexts_yield_zero_probability() {
        let model = table(&[
            ("aaa it .", 0.5, 0.3),
            ("We aaa it", 0.5, 0.3),
            ("_START_ We aaa", 0.5, 0.3),
            ("bbb it .", 0.9, 0.3),
            ("We bbb it", 0.9, 0.3),
            ("_START_ We bbb", 0.9, 0.3),
        ]);
        let rule = rule_from("aaa; bbb; 5\n", model);
        assert!(rule.match_sentence(&sentence("We aaa it.")).is_empty());
    }

    #[test]
    fn single_word_sentences_return_early() {
        let rule = rule_from("hello; hi; 10\n", table(&[]));
        assert!(rule.match_sentence(&sentence("hello")).is_empty());
    }

    #[test]
    fn curly_apostrophe_is_normalized_to_the_ascii_quote() {
        let model = table(&[
            ("' s _END_", 0.01, 1.0),
            ("it ' s", 0.01, 1.0),
            ("_START_ it '", 0.01, 1.0),
            ("re s _END_", 0.9, 1.0),
            ("it re s", 0.9, 1.0),
            ("_START_ it re", 0.9, 1.0),
        ]);
        let rule = rule_from("'; re; 10\n", model);
        let matches = rule.match_sentence(&sentence("it ’ s"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 3);
        assert_eq!(matches[0].length(), 3);
        assert_eq!(matches[0].replacements()[0].value(), "re");
    }

    #[test]
    fn helpers_fold_case_and_clean_umlaubt_ids() {
        assert_eq!(uppercase_first("über"), "Über");
        assert_eq!(uppercase_first(""), "");
        assert_eq!(lowercase_first("ÜBER"), "üBER");
        assert_eq!(lowercase_first(""), "");
        assert_eq!(clean_id("äöü"), "AEOEUE");
        assert!(is_real_word("über"));
        assert!(!is_real_word("über."));
    }

    #[test]
    fn language_constructors_load_their_confusion_sets() {
        let model: Arc<dyn LanguageModel> = table(&[]);
        let en = ConfusionRule::new_english(model.clone());
        assert!(en.pair_count() > 100);
        assert_eq!(en.id(), "EN_CONFUSION_RULE");
        assert_eq!(en.category().id().as_str(), "TYPOS");
        assert_eq!(en.issue_type(), IssueType::Misspelling);
        assert!(en.is_default_on());

        let de = ConfusionRule::new_german(model.clone());
        assert!(de.pair_count() >= 70);
        assert_eq!(de.id(), "DE_CONFUSION_RULE");

        let es = ConfusionRule::new_spanish(model.clone());
        assert!(es.pair_count() >= 1);
        assert_eq!(es.id(), "ES_CONFUSION_RULE");

        let ru = ConfusionRule::new_russian(model.clone());
        assert!(ru.pair_count() >= 1);
        assert_eq!(ru.id(), "CONFUSION_RULE");

        assert_eq!(ConfusionRule::new_italian(model.clone()).pair_count(), 0);
        assert_eq!(ConfusionRule::new_arabic(model.clone()).pair_count(), 0);

        let l2 = ConfusionRule::new_english_for_l2(model, "de");
        assert!(l2.pair_count() > 0);
        assert_eq!(l2.id(), "EN_FOR_DE_SPEAKERS_FALSE_FRIENDS");
    }

    #[test]
    fn term_pairs_expose_deduplicated_tuple_descriptions() {
        let pairs = confusion_term_pairs("en", &["confusion_sets.txt"]);
        assert!(!pairs.is_empty());
        for (a, da, b, db) in &pairs {
            assert!(!a.is_empty() && !b.is_empty());
            assert_ne!(a, b);
            let _ = (da, db);
        }
        let mut sorted = pairs.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), pairs.len(), "pairs must be deduplicated");
    }
}
