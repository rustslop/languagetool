use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const MULTIPART_COMPOUNDS: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/nl/src/main/resources/org/languagetool/resource/nl/multipartcompounds.txt"
);

const SPELLED_WORDS: &str = "abc|adv|aed|apk|b2b|bh|bhv|bso|btw|bv|cao|cd|cfk|ckv|cv|dc|dj|dtp|dvd|fte|gft|ggo|ggz|gm|gmo|gps|gsm|hbo|hd|hiv|hr|hrm|hst|ic|ivf|kmo|lcd|lp|lpg|lsd|mbo|mdf|mkb|mms|msn|mt|ngo|nv|ob|ov|ozb|p2p|pc|pcb|pdf|pk|pps|pr|pvc|roc|rvs|sms|tbc|tbs|tl|tv|uv|vbo|vj|vmbo|vsbo|vwo|wc|wo|xtc|zzp";

const VOWEL_CONNECTIONS: &[&str] = &[
    "aa", "ae", "ai", "au", "ee", "ée", "ei", "éi", "eu", "éu", "ie", "ii", "ij", "oe", "oi", "oo",
    "ou", "ui", "uu",
];

fn spelled_words() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| SPELLED_WORDS.split('|').collect())
}

fn hyphen_chars_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(&format!(r"(^|.+-)?({SPELLED_WORDS})")).unwrap())
}

fn chars_hyphen_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(&format!(r"({SPELLED_WORDS})(-.+|$)?")).unwrap())
}

fn ends_in_hyphen_and_char(compound: &str) -> bool {
    let mut chars = compound.chars().rev();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && matches!(chars.next(), Some('-'))
        && chars.next().is_some()
}

fn starts_with_char_and_hyphen(word: &str) -> bool {
    let mut chars = word.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && matches!(chars.next(), Some('-'))
        && chars.next().is_some()
}

pub(crate) fn glue_parts(parts: &[&str]) -> String {
    let spelled = spelled_words();
    let mut compound = parts.first().copied().unwrap_or_default().to_string();
    for word2 in parts.iter().skip(1) {
        if compound.chars().count() > 2 || spelled.contains(compound.as_str()) {
            let Some(last) = compound.chars().last() else {
                continue;
            };
            let Some(first) = word2.chars().next() else {
                continue;
            };
            let mut connection = String::new();
            connection.push(last);
            connection.push(first);
            let vowel_collision = VOWEL_CONNECTIONS.iter().any(|v| connection.contains(v));
            if vowel_collision
                || (first.is_uppercase() && last.is_lowercase())
                || (last.is_uppercase() && first.is_lowercase())
                || (last.is_uppercase() && first.is_uppercase())
                || compound.chars().last().is_some_and(|c| c.is_ascii_digit())
                || word2.chars().next().is_some_and(|c| c.is_ascii_digit())
                || hyphen_chars_re().is_match(&compound)
                || chars_hyphen_re().is_match(word2)
                || ends_in_hyphen_and_char(&compound)
                || starts_with_char_and_hyphen(word2)
            {
                compound.push('-');
                compound.push_str(word2);
            } else {
                compound.push_str(word2);
            }
        } else {
            compound.push_str(word2);
        }
    }
    compound
}

fn generate_variants(so_far: &str, words: &[&str], result: &mut HashSet<String>) {
    match words {
        [last] => {
            if so_far.contains(' ') {
                result.insert(format!("{so_far}{last}"));
            }
            result.insert(format!("{so_far} {last}"));
        }
        [head, rest @ ..] => {
            generate_variants(&format!("{so_far}{head}"), rest, result);
            if !so_far.is_empty() {
                generate_variants(&format!("{so_far} {head}"), rest, result);
            }
        }
        [] => {}
    }
}

struct SpaceInCompoundData {
    variants_by_first_word: HashMap<String, Vec<&'static str>>,
    normalized_to_message: HashMap<String, String>,
}

fn shared_data() -> &'static SpaceInCompoundData {
    static DATA: OnceLock<SpaceInCompoundData> = OnceLock::new();
    DATA.get_or_init(|| {
        let mut variants: HashSet<String> = HashSet::new();
        let mut normalized_to_message: HashMap<String, String> = HashMap::new();
        for line in MULTIPART_COMPOUNDS.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let line = line.trim_end_matches('\r');
            let parts: Vec<&str> = line.split('|').collect();
            let word_parts = parts[0];
            let words: Vec<&str> = word_parts.split(' ').collect();
            generate_variants("", &words, &mut variants);
            let mut message = format!("Waarschijnlijk bedoelt u: {}", glue_parts(&words));
            if parts.len() == 2 {
                message.push_str(&format!(" ({})", parts[1]));
            }
            normalized_to_message.insert(glue_parts(&words), message);
        }
        let mut variants_by_first_word: HashMap<String, Vec<&'static str>> = HashMap::new();
        for variant in variants {
            let first_word = variant.split(' ').next().unwrap_or_default().to_string();
            variants_by_first_word
                .entry(first_word)
                .or_default()
                .push(og_tagger::morfologik::intern_string(variant));
        }
        SpaceInCompoundData {
            variants_by_first_word,
            normalized_to_message,
        }
    })
}

pub struct SpaceInCompoundRule {
    description: String,
}

pub fn space_in_compound_rule() -> SpaceInCompoundRule {
    SpaceInCompoundRule {
        description: "Detecteert spatiefouten".to_string(),
    }
}

fn is_ascii_letter(c: char) -> bool {
    c.is_ascii_alphabetic()
}

fn to_id(input: &str) -> String {
    input
        .to_uppercase()
        .trim()
        .replace(' ', "_")
        .replace('\'', "_Q_")
}

impl Rule for SpaceInCompoundRule {
    fn id(&self) -> &str {
        "NL_SPACE_IN_COMPOUND"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Diversen")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let data = shared_data();
        let text = sentence.text();
        let mut matches = Vec::new();
        let mut offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        offsets.push(text.len());
        let mut offset_idx = 0usize;
        while offset_idx + 1 < offsets.len() {
            let start = offsets[offset_idx];
            offset_idx += 1;
            if start > 0 && is_ascii_letter(text[..start].chars().next_back().unwrap_or(' ')) {
                continue;
            }
            if text[start..].starts_with(' ') {
                continue;
            }
            let word_end = text[start..]
                .find(' ')
                .map(|i| start + i)
                .unwrap_or(text.len());
            let first_word = &text[start..word_end];
            if let Some(variants) = data.variants_by_first_word.get(first_word) {
                for variant in variants {
                    let Some(end) = start.checked_add(variant.len()) else {
                        continue;
                    };
                    if end > text.len()
                        || !text.is_char_boundary(end)
                        || &text[start..end] != *variant
                    {
                        continue;
                    }
                    if end < text.len() && text[end..].chars().next().is_some_and(is_ascii_letter) {
                        continue;
                    }
                    let covered = &text[start..end];
                    let covered_parts: Vec<&str> = covered.split(' ').collect();
                    let covered_no_spaces = glue_parts(&covered_parts);
                    let Some(message) = data.normalized_to_message.get(&covered_no_spaces) else {
                        continue;
                    };
                    let rule_id = to_id(&format!("{}_{}", self.id(), covered));
                    let rm_rule = RuleMatchRule::new(rule_id, self.description.clone())
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str());
                    let mut ctx_start = start.saturating_sub(40);
                    while !text.is_char_boundary(ctx_start) {
                        ctx_start -= 1;
                    }
                    let mut ctx_end = (end + 40).min(text.len());
                    while !text.is_char_boundary(ctx_end) {
                        ctx_end += 1;
                    }
                    matches.push(
                        RuleMatch::new(
                            message.clone(),
                            start,
                            end - start,
                            rm_rule,
                            RuleMatchContext::new(
                                text[ctx_start..ctx_end].to_string(),
                                ctx_start,
                                ctx_end - ctx_start,
                            ),
                        )
                        .with_sentence(text.to_string())
                        .with_replacements(vec![SuggestedReplacement::new(&covered_no_spaces)]),
                    );
                }
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nl::test_support::DutchPipeline;

    fn sentence(text: &str) -> AnalyzedSentence {
        let pipeline = DutchPipeline::shared();
        pipeline.analyze_range(text, 0, text.len())
    }

    fn rule() -> SpaceInCompoundRule {
        space_in_compound_rule()
    }

    #[test]
    fn java_pins_good_sentences() {
        assert!(
            rule()
                .match_sentence(&sentence("langeafstandloper"))
                .is_empty()
        );
        assert!(
            rule()
                .match_sentence(&sentence(
                    "Dat zie je nu weer met de zogenaamde oudelullendagen die in heel andere tijden met gulle hand in cao’s werden uitgereikt aan werknemers van vijftig jaar en ouder."
                ))
                .is_empty()
        );
        assert!(
            rule()
                .match_sentence(&sentence(
                    "...jk aan voor de middelbare school tijdens de aanmeldw..."
                ))
                .is_empty()
        );
    }

    #[test]
    fn java_pins_matching_sentences() {
        assert_eq!(
            rule()
                .match_sentence(&sentence(
                    "...jk aan voor de middelbare school tijdens de..., lange afstand loper"
                ))
                .len(),
            1
        );
        assert_eq!(
            rule()
                .match_sentence(&sentence("lange afstand loper"))
                .len(),
            1
        );
        assert_eq!(
            rule().match_sentence(&sentence("langeafstand loper")).len(),
            1
        );
        assert_eq!(
            rule().match_sentence(&sentence("lange afstandloper")).len(),
            1
        );
    }

    #[test]
    fn suggestion_glues_the_parts() {
        let matches = rule().match_sentence(&sentence("lange afstand loper"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "langeafstandloper");
        assert!(
            matches[0]
                .rule()
                .id()
                .contains("NL_SPACE_IN_COMPOUND_LANGE")
        );
    }

    #[test]
    fn java_pins_generate_variants() {
        let mut res = Vec::new();
        let mut set = HashSet::new();
        generate_variants("", &["a", "b"], &mut set);
        res.extend(set.drain().collect::<Vec<_>>());
        assert_eq!(res, vec!["a b".to_string()]);

        let mut set = HashSet::new();
        generate_variants("", &["a", "b", "c"], &mut set);
        let mut res: Vec<String> = set.into_iter().collect();
        res.sort_unstable();
        assert_eq!(
            res,
            vec!["a b c".to_string(), "a bc".to_string(), "ab c".to_string()]
        );

        let mut set = HashSet::new();
        generate_variants("", &["a", "b", "c", "d"], &mut set);
        assert_eq!(set.len(), 7);
    }

    #[test]
    fn java_pins_glue_parts() {
        assert_eq!(
            glue_parts(&["lange", "afstand", "loper"]),
            "langeafstandloper"
        );
        assert_eq!(
            glue_parts(&["tweede", "album", "syndroom"]),
            "tweedealbumsyndroom"
        );
        assert_eq!(glue_parts(&["cd", "speler"]), "cd-speler");
        assert_eq!(glue_parts(&["tv", "meubel"]), "tv-meubel");
        assert_eq!(glue_parts(&["b2b", "netwerk"]), "b2b-netwerk");
        assert_eq!(glue_parts(&["100", "jaar"]), "100-jaar");
        assert_eq!(glue_parts(&["aa", "weer"]), "aaweer");
    }
}
