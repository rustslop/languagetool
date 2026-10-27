use og_core::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, Category, IssueType, RuleMatch,
    RuleMatchContext, RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use og_tagger::GenericTagger;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, OnceLock};

static DIGIT_RE: OnceLock<regex::Regex> = OnceLock::new();

const MAX_TERMS: usize = 5;
const COMPOUNDS_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/fr/src/main/resources/org/languagetool/resource/fr/compounds.txt"
);

struct CompoundRuleData {
    incorrect: HashSet<Box<str>>,
    joined: HashSet<Box<str>>,
    joined_lower: HashSet<Box<str>>,
    dash: HashSet<Box<str>>,
    has_digit_patterns: bool,
}

impl CompoundRuleData {
    fn load(data: &str) -> Self {
        let mut out = Self {
            incorrect: HashSet::new(),
            joined: HashSet::new(),
            joined_lower: HashSet::new(),
            dash: HashSet::new(),
            has_digit_patterns: false,
        };
        for line in data.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut exp = line.replace('-', " ");
            if exp.ends_with('+') {
                exp.truncate(exp.len() - 1);
                out.joined.insert(Box::from(exp.to_lowercase()));
            } else if exp.ends_with('*') {
                exp.truncate(exp.len() - 1);
                out.dash.insert(Box::from(exp.to_lowercase()));
            } else if exp.ends_with('?') {
                exp.truncate(exp.len() - 1);
                let lc = exp.to_lowercase();
                out.joined.insert(Box::from(lc.clone()));
                out.joined_lower.insert(Box::from(lc));
            } else if exp.ends_with('$') {
                exp.truncate(exp.len() - 1);
                let lc = exp.to_lowercase();
                out.joined.insert(Box::from(lc.clone()));
                out.joined_lower.insert(Box::from(lc.clone()));
                out.dash.insert(Box::from(lc));
            } else {
                let lc = exp.to_lowercase();
                out.joined.insert(Box::from(lc.clone()));
                out.dash.insert(Box::from(lc));
            }
            if exp.contains("\\d") {
                out.has_digit_patterns = true;
            }
            out.incorrect.insert(Box::from(exp.to_lowercase()));
        }
        out
    }
}

pub struct CompoundRule {
    id: &'static str,
    description: &'static str,
    with_hyphen_msg: &'static str,
    without_hyphen_msg: &'static str,
    either_msg: &'static str,
    short_msg: &'static str,
    tagger: Option<Arc<GenericTagger>>,
    data: CompoundRuleData,
}

fn whitespace_collapse(s: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\s+").unwrap())
        .replace_all(s, " ")
        .into_owned()
}

fn normalize(s: &str) -> String {
    whitespace_collapse(&s.trim().replace(" - ", " ").replace('-', " "))
}

fn is_all_uppercase(s: &str) -> bool {
    !s.chars().any(char::is_lowercase)
}

fn is_not_all_uppercase(s: &str) -> bool {
    s.split(' ')
        .all(|part| part == "-" || !is_all_uppercase(part))
}

fn uncapitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn merge_compound(s: &str, uncapitalize_mid: bool) -> String {
    let mut out = String::new();
    for (k, part) in s.replace('-', " ").split(' ').enumerate() {
        if k == 0 {
            out.push_str(part);
        } else if uncapitalize_mid {
            out.push_str(&uncapitalize_first(part));
        } else {
            out.push_str(part);
        }
    }
    out
}

fn collapse_dashes(s: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"--+").unwrap())
        .replace_all(s, "-")
        .into_owned()
}

fn to_id_suffix(s: &str) -> String {
    s.to_uppercase()
        .replace(' ', "_")
        .replace('\'', "_Q_")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

impl CompoundRule {
    pub fn french(tagger: Option<Arc<GenericTagger>>) -> Self {
        Self {
            id: "FR_COMPOUNDS",
            description: "Mots avec trait d’union : $match",
            with_hyphen_msg: "Écrivez avec un trait d’union.",
            without_hyphen_msg: "Écrivez avec un mot seul sans espace ni trait d’union.",
            either_msg: "Écrivez avec un mot seul ou avec trait d’union.",
            short_msg: "Erreur de trait d’union",
            tagger,
            data: CompoundRuleData::load(COMPOUNDS_TXT),
        }
    }

    pub fn entry_count(&self) -> usize {
        self.data.incorrect.len()
    }

    fn is_misspelled(&self, word: &str) -> bool {
        match &self.tagger {
            Some(t) => !t.is_word_tagged(word),
            None => false,
        }
    }
}

impl Rule for CompoundRule {
    fn id(&self) -> &str {
        self.id
    }

    fn description(&self) -> &str {
        self.description
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Règles de base")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let full = sentence.tokens();
        let mut toks: Vec<(&AnalyzedTokenReadings, bool)> = Vec::with_capacity(full.len());
        for (i, t) in full.iter().enumerate() {
            if t.is_whitespace() {
                continue;
            }
            toks.push((t, i > 0 && full[i - 1].is_whitespace()));
        }
        let n = toks.len();
        let sent_start = sentence.start();
        let text = sentence.text();
        let mut prev_tokens: VecDeque<(&AnalyzedTokenReadings, bool)> =
            VecDeque::with_capacity(MAX_TERMS);
        let synthetic = AnalyzedTokenReadings::new(AnalyzedToken::new("", 0, 0));
        let mut rule_matches: Vec<RuleMatch> = Vec::new();
        let mut prev_from: Option<usize> = None;
        let digit_re = DIGIT_RE.get_or_init(|| regex::Regex::new(r"\d+").unwrap());
        for i in 0..n + MAX_TERMS {
            let current: Option<(&AnalyzedTokenReadings, bool)> = toks.get(i).copied();
            if i == 0 {
                if let Some(pair) = current {
                    prev_tokens.push_back(pair);
                }
                continue;
            }
            if current.is_some_and(|(t, _)| t.is_immunized()) {
                continue;
            }
            let mut strings: Vec<String> = Vec::with_capacity(MAX_TERMS);
            let mut origs: Vec<String> = Vec::with_capacity(MAX_TERMS);
            let mut string_end: HashMap<String, usize> = HashMap::with_capacity(MAX_TERMS * 2);
            let mut sb = String::new();
            let mut is_first_sent_start = false;
            for (j, (atr, ws_before)) in prev_tokens.iter().enumerate() {
                if *ws_before {
                    sb.push(' ');
                }
                sb.push_str(atr.token().token());
                if j == 0 {
                    is_first_sent_start = atr.has_pos_tag("SENT_START");
                }
                if j >= 1 || (j == 0 && !is_first_sent_start) {
                    let s = normalize(&sb);
                    string_end.entry(s.clone()).or_insert(j);
                    strings.push(s);
                    origs.push(sb.trim().to_string());
                }
            }
            for k in (0..strings.len()).rev() {
                let key = strings[k].to_lowercase();
                let contains_digits =
                    self.data.has_digit_patterns && strings[k].split(' ').any(is_numeric);
                let digit_key =
                    contains_digits.then(|| digit_re.replace_all(&key, "\\d+").into_owned());
                let hit = self.data.incorrect.contains(key.as_str())
                    || digit_key
                        .as_deref()
                        .is_some_and(|dk| self.data.incorrect.contains(dk));
                if !hit {
                    continue;
                }
                let orig = origs[k].as_str();
                if self.data.dash.contains(key.as_str()) && !orig.contains(' ') {
                    break;
                }
                let mut replacement: Vec<String> = Vec::new();
                let mut msg: &str = self.either_msg;
                if self.data.dash.contains(key.as_str())
                    || digit_key
                        .as_deref()
                        .is_some_and(|dk| self.data.incorrect.contains(dk))
                {
                    replacement.push(orig.replace(' ', "-"));
                    msg = self.with_hyphen_msg;
                }
                if is_not_all_uppercase(orig) && self.data.joined.contains(key.as_str()) {
                    replacement.push(merge_compound(
                        orig,
                        self.data.joined_lower.contains(key.as_str()),
                    ));
                    msg = self.without_hyphen_msg;
                }
                if strings[k]
                    .split(' ')
                    .next()
                    .is_some_and(|p| p.chars().count() == 1)
                {
                    replacement.clear();
                    replacement.push(orig.replace(' ', "-"));
                    msg = self.with_hyphen_msg;
                } else if replacement.is_empty() || replacement.len() == 2 {
                    msg = self.either_msg;
                }
                let end_j = string_end[&strings[k]];
                let first = prev_tokens
                    .front()
                    .map(|(t, _)| t.token().start())
                    .unwrap_or(0);
                let end = prev_tokens[end_j].0.token().end();
                let original_slice = &text[first - sent_start..end - sent_start];
                replacement = replacement
                    .into_iter()
                    .map(|r| collapse_dashes(&r))
                    .filter(|r| r != original_slice && !self.is_misspelled(r))
                    .collect();
                if replacement.is_empty() {
                    break;
                }
                if prev_from == Some(first) {
                    break;
                }
                prev_from = Some(first);
                let sub_id = format!("{}_{}", self.id, to_id_suffix(&strings[k]));
                let desc = self.description.replace("$match", orig);
                let rm_rule = RuleMatchRule::new(sub_id, desc)
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                let ctx_text = crate::fr::context_window(text, sent_start, first, end);
                rule_matches.push(
                    RuleMatch::new(
                        msg,
                        first,
                        end - first,
                        rm_rule,
                        RuleMatchContext::new(ctx_text, first, end - first),
                    )
                    .with_short_message(self.short_msg)
                    .with_replacements(replacement.iter().map(SuggestedReplacement::new).collect())
                    .with_sentence(text.to_string()),
                );
                break;
            }
            if prev_tokens.len() == MAX_TERMS {
                prev_tokens.pop_front();
            }
            if let Some(pair) = current {
                prev_tokens.push_back(pair);
            } else {
                prev_tokens.push_back((&synthetic, false));
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings};
    use og_tokenizer::FrenchWordTokenizer;
    use std::sync::Arc;

    use og_tokenizer::WordTokenizer as _;

    fn sentence_from(text: &str) -> AnalyzedSentence {
        let tokenizer = FrenchWordTokenizer::new(Arc::new(|w: &str| {
            matches!(
                w,
                "rendez-vous" | "peut-être" | "aujourd'hui" | "Jésus-Christ"
            )
        }));
        let mut tokens: Vec<AnalyzedTokenReadings> = tokenizer
            .tokenize(text)
            .iter()
            .map(|t| AnalyzedTokenReadings::new(AnalyzedToken::from_token(t)))
            .collect();
        tokens.insert(
            0,
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
        );
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        sentence.set_tokens(tokens);
        sentence
    }

    fn suggestion_lists(text: &str) -> Vec<Vec<String>> {
        CompoundRule::french(None)
            .match_sentence(&sentence_from(text))
            .into_iter()
            .map(|m| {
                m.replacements()
                    .iter()
                    .map(|r| r.value().to_string())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn correct_compounds_do_not_match() {
        let rule = CompoundRule::french(None);
        for text in [
            "Jésus-Christ",
            "Congo-Brazzaville",
            "vidéo-clip",
            "anglo-saxon",
        ] {
            assert!(
                rule.match_sentence(&sentence_from(text)).is_empty(),
                "{text}"
            );
        }
    }

    #[test]
    fn split_compounds_match_with_hyphen_suggestion() {
        assert_eq!(suggestion_lists("Jésus Christ"), vec![vec!["Jésus-Christ"]]);
        assert_eq!(
            suggestion_lists("Congo Brazzaville"),
            vec![vec!["Congo-Brazzaville"]]
        );
        assert_eq!(
            suggestion_lists("Congo- Brazzaville"),
            vec![vec!["Congo-Brazzaville"]]
        );
        assert_eq!(
            suggestion_lists("Congo -Brazzaville"),
            vec![vec!["Congo-Brazzaville"]]
        );
        assert_eq!(
            suggestion_lists("Congo - Brazzaville"),
            vec![vec!["Congo-Brazzaville"]]
        );
        assert_eq!(suggestion_lists("anglo saxon"), vec![vec!["anglo-saxon"]]);
        assert_eq!(suggestion_lists("anglo- saxon"), vec![vec!["anglo-saxon"]]);
        assert_eq!(suggestion_lists("anglo -saxon"), vec![vec!["anglo-saxon"]]);
        assert_eq!(suggestion_lists("anglo - saxon"), vec![vec!["anglo-saxon"]]);
    }

    #[test]
    fn joined_only_entry_suggests_one_word() {
        assert_eq!(
            suggestion_lists("rez-de chaussée"),
            vec![vec!["rez-de-chaussée"]]
        );
        assert_eq!(suggestion_lists("le - quel"), vec![vec!["lequel"]]);
        assert_eq!(suggestion_lists("le quel"), vec![vec!["lequel"]]);
        assert_eq!(suggestion_lists("le- quel"), vec![vec!["lequel"]]);
    }

    #[test]
    fn sub_rule_ids_carry_the_matched_compound() {
        let rule = CompoundRule::french(None);
        let matches = rule.match_sentence(&sentence_from("le quel"));
        assert_eq!(matches.len(), 1);
        assert!(matches[0].rule().id().starts_with("FR_COMPOUNDS_LE_QUEL"));
    }

    #[test]
    fn data_loads_expected_entries() {
        let rule = CompoundRule::french(None);
        assert!(rule.entry_count() > 1300);
        assert!(rule.data.incorrect.contains("rez de chaussée"));
        assert!(rule.data.joined.contains("le quel"));
        assert!(!rule.data.dash.contains("le quel"));
        assert!(rule.data.dash.contains("anglo saxon"));
        assert!(!rule.data.joined.contains("anglo saxon"));
    }

    #[test]
    fn java_class_example_pair_haut_rhin() {
        assert_eq!(suggestion_lists("Le Haut Rhin."), vec![vec!["Haut-Rhin"]]);
        assert_eq!(
            suggestion_lists("un anglo saxon ici."),
            vec![vec!["anglo-saxon"]]
        );
        let rule = CompoundRule::french(None);
        assert!(
            rule.match_sentence(&sentence_from("Le Haut-Rhin."))
                .is_empty()
        );
    }

    #[test]
    fn real_tagger_keeps_known_replacements_and_filters_unknown_ones() {
        let Some(lt_root) = crate::engine::find_lt_resource_dir() else {
            return;
        };
        let dir = lt_root.join("fr/src/main/resources/org/languagetool/resource/fr");
        if !dir.join("french.dict").exists() {
            return;
        }
        let tagger = Arc::new(GenericTagger::french(&dir));
        let rule = CompoundRule::french(Some(tagger));
        assert!(!rule.is_misspelled("lequel"));
        assert!(!rule.is_misspelled("anglo-saxon"));
        assert_eq!(suggestion_lists("le quel"), vec![vec!["lequel"]]);
        assert_eq!(suggestion_lists("anglo saxon"), vec![vec!["anglo-saxon"]]);
    }
}
