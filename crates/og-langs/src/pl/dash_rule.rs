use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

struct DashCompoundData {
    variants: HashMap<String, String>,
    max_len: usize,
}

fn load_compounds(data: &str) -> DashCompoundData {
    let mut variants = HashMap::new();
    let mut max_len = 0;
    for line in data.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let word = if line.ends_with('+') || line.ends_with('?') {
            continue;
        } else if line.ends_with('*') || line.ends_with('$') {
            &line[..line.len() - 1]
        } else {
            line
        };
        for variant in [
            word.replace('-', "\u{2013}"),
            word.replace('-', "\u{2014}"),
            word.replace('-', " \u{2013} "),
            word.replace('-', " \u{2014} "),
        ] {
            max_len = max_len.max(variant.len());
            variants.insert(variant.clone(), variant);
        }
    }
    DashCompoundData { variants, max_len }
}

fn shared_compounds() -> Arc<DashCompoundData> {
    static CACHE: OnceLock<Arc<DashCompoundData>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            Arc::new(load_compounds(include_str!(
                "../../../../languagetool/languagetool-language-modules/pl/src/main/resources/org/languagetool/resource/pl/compounds.txt"
            )))
        })
        .clone()
}

pub struct PolishDashRule {
    compounds: Arc<DashCompoundData>,
}

impl PolishDashRule {
    pub fn new() -> Self {
        Self {
            compounds: shared_compounds(),
        }
    }

    fn candidate_hits<'t>(&self, text: &'t str) -> Vec<(usize, &'t str)> {
        let mut hits: Vec<(usize, usize)> = Vec::new();
        let mut cursor = 0usize;
        while let Some(rel) = text[cursor..]
            .find('\u{2013}')
            .or_else(|| text[cursor..].find('\u{2014}'))
        {
            let dash_at = cursor + rel;
            let min_start = dash_at.saturating_sub(self.compounds.max_len);
            let mut starts: Vec<usize> = Vec::new();
            for start in min_start..=dash_at {
                if !text.is_char_boundary(start) {
                    continue;
                }
                let preceded_ok = start == 0
                    || text[..start]
                        .chars()
                        .next_back()
                        .is_some_and(|c| !c.is_ascii_alphabetic());
                if preceded_ok {
                    starts.push(start);
                }
            }
            let max_end = (dash_at + self.compounds.max_len).min(text.len());
            let mut ends: Vec<usize> = Vec::new();
            for end in dash_at..=max_end {
                if !text.is_char_boundary(end) {
                    continue;
                }
                let followed_ok = end == text.len()
                    || text[end..]
                        .chars()
                        .next()
                        .is_some_and(|c| !c.is_ascii_alphabetic());
                if followed_ok {
                    ends.push(end);
                }
            }
            for start in &starts {
                for end in &ends {
                    if end > start
                        && text.get(*start..*end).is_some_and(|candidate| {
                            self.compounds.variants.contains_key(candidate)
                        })
                    {
                        hits.push((*start, *end));
                    }
                }
            }
            let advance = text[dash_at..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(3);
            cursor = dash_at + advance;
        }
        hits.sort_by(|a, b| {
            b.1.saturating_sub(b.0)
                .cmp(&a.1.saturating_sub(a.0))
                .then(b.0.cmp(&a.0))
        });
        hits.dedup();
        hits.into_iter()
            .filter_map(|(start, end)| text.get(start..end).map(|s| (start, s)))
            .collect()
    }
}

impl Default for PolishDashRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for PolishDashRule {
    fn id(&self) -> &str {
        "PL_DASH_RULE"
    }

    fn is_picky(&self) -> bool {
        true
    }

    fn description(&self) -> &str {
        "Sprawdza, czy wyrazy pisane z łącznikiem zapisano z myślnikami (np. „Lądek — Zdrój” zamiast „Lądek-Zdrój”)."
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("TYPOGRAPHY", "Typography")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("other".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let text = sentence.text();
        let mut matches = Vec::new();
        let mut used_starts: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for (begin, covered) in self.candidate_hits(text) {
            let end = begin + covered.len();
            if used_starts.contains(&begin) {
                continue;
            }
            let replacement = covered
                .replace(" \u{2013} ", "-")
                .replace(" \u{2014} ", "-")
                .replace(['\u{2013}', '\u{2014}'], "-");
            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str());
            let ctx_start = begin.saturating_sub(40);
            let ctx_end = (end + 40).min(text.len());
            matches.push(
                RuleMatch::new(
                    "Błędne użycie myślnika zamiast łącznika.",
                    begin,
                    end - begin,
                    rm_rule,
                    RuleMatchContext::new(
                        text[ctx_start..ctx_end].to_string(),
                        ctx_start,
                        ctx_end - ctx_start,
                    ),
                )
                .with_replacements(vec![SuggestedReplacement::new(replacement)])
                .with_sentence(text.to_string())
                .with_short_message("Myślnik")
                .with_context_for_sure_match(2),
            );
            used_starts.insert(begin);
        }
        matches.sort_by_key(|m| m.offset());
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_pins_compound_with_dashes_is_flagged_and_hyphen_suggested() {
        let rule = PolishDashRule::new();
        let text = "Witamy w Lądek – Zdrój!";
        let sentence =
            crate::pl::test_support::PolishPipeline::shared().analyze_range(text, 0, text.len());
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches.len() == 1,
            "{text}: {:?}",
            matches.iter().map(|m| m.offset()).collect::<Vec<_>>()
        );
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "Lądek-Zdrój")
        );
        assert!(
            &text[matches[0].offset()..matches[0].offset() + matches[0].length()]
                == "Lądek – Zdrój"
        );
    }

    #[test]
    fn java_pins_spaced_em_dash_variant() {
        let rule = PolishDashRule::new();
        let text = "Mieszkam w Lądek — Zdrój.";
        let sentence =
            crate::pl::test_support::PolishPipeline::shared().analyze_range(text, 0, text.len());
        let matches = rule.match_sentence(&sentence);
        assert!(
            matches
                .iter()
                .any(|m| &text[m.offset()..m.offset() + m.length()] == "Lądek — Zdrój"),
            "{text}: {matches:?}"
        );
    }

    #[test]
    fn java_pins_join_only_entries_are_not_dash_candidates() {
        let rule = PolishDashRule::new();
        let text = "Uczyli się nota bene — bez skutku.";
        let sentence =
            crate::pl::test_support::PolishPipeline::shared().analyze_range(text, 0, text.len());
        assert!(
            rule.match_sentence(&sentence).is_empty(),
            "nota-bene+ entries are skipped by loadCompoundFile"
        );
    }
}
