use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement, rule::Rule,
};
use std::sync::LazyLock;

fn dash_variants(data: &str) -> Vec<String> {
    let mut variants = Vec::new();
    for line in data.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.ends_with('+') || line.ends_with('?') {
            continue;
        }
        let word = line
            .strip_suffix('*')
            .or_else(|| line.strip_suffix('$'))
            .unwrap_or(line);
        for (from, to) in [("-", "–"), ("-", "—"), ("-", " – "), ("-", " — ")] {
            variants.push(word.replace(from, to));
        }
    }
    variants
}

static DASH_VARIANTS: LazyLock<Vec<String>> = LazyLock::new(|| {
    dash_variants(include_str!(
        "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/resource/pt/post-reform-compounds.txt"
    ))
});

static PRE_REFORM_DASH_VARIANTS: LazyLock<Vec<String>> = LazyLock::new(|| {
    dash_variants(include_str!(
        "../../../../languagetool/languagetool-language-modules/pt/src/main/resources/org/languagetool/resource/pt/pre-reform-compounds.txt"
    ))
});

fn is_boundary(s: char) -> bool {
    !s.is_ascii_alphabetic()
        && !matches!(
            s,
            'Â' | 'â'
                | 'Ã'
                | 'ã'
                | 'Ç'
                | 'ç'
                | 'Ê'
                | 'ê'
                | 'Ó'
                | 'ó'
                | 'Ô'
                | 'ô'
                | 'Õ'
                | 'õ'
                | 'ü'
        )
}

pub struct PostReformPortugueseDashRule;

impl PostReformPortugueseDashRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PostReformPortugueseDashRule {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PreReformPortugueseDashRule;

impl PreReformPortugueseDashRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PreReformPortugueseDashRule {
    fn default() -> Self {
        Self::new()
    }
}

fn dash_matches(id: &str, variants: &[String], sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
    let text = sentence.text();
    if !text.contains('–') && !text.contains('—') {
        return Vec::new();
    }
    let mut matches = Vec::new();
    let mut start_positions: Vec<usize> = Vec::new();
    let mut hits: Vec<(usize, usize)> = variants
        .iter()
        .flat_map(|variant| {
            text.match_indices(variant)
                .map(move |(i, _)| (i, variant.len()))
        })
        .collect();
    hits.sort_by_key(|&(begin, _)| std::cmp::Reverse(begin));
    for (begin, len) in hits {
        if start_positions.contains(&begin) {
            continue;
        }
        if begin > 0 {
            let prev = text[..begin].chars().next_back().unwrap_or(' ');
            if !is_boundary(prev) {
                continue;
            }
        }
        let end = begin + len;
        if end < text.len() {
            let next = text[end..].chars().next().unwrap_or(' ');
            if !is_boundary(next) {
                continue;
            }
        }
        let covered = &text[begin..end];
        let suggestion = covered
            .replace(" – ", "-")
            .replace(" — ", "-")
            .replace(['–', '—'], "-");
        let rm_rule = RuleMatchRule::new(id, "Travessões no lugar de hífens")
            .with_category(Category::new("TYPOGRAPHY", "Typography"))
            .with_issue_type(IssueType::Typographical.as_str());
        let ctx_start = begin.saturating_sub(40);
        let ctx_end = std::cmp::min(end + 40, text.len());
        let context_text = text[ctx_start..ctx_end].to_string();
        matches.push(
            RuleMatch::new(
                "Um travessão foi utilizado em vez de um hífen.",
                begin,
                end - begin,
                rm_rule,
                RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
            )
            .with_replacements(vec![SuggestedReplacement::new(&suggestion)])
            .with_sentence(text.to_string()),
        );
        start_positions.push(begin);
    }
    matches
}

impl Rule for PostReformPortugueseDashRule {
    fn id(&self) -> &str {
        "PT_POSAO_DASH_RULE"
    }

    fn description(&self) -> &str {
        "Travessões no lugar de hífens"
    }

    fn is_picky(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("TYPOGRAPHY", "Typography")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Typographical
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        dash_matches(self.id(), &DASH_VARIANTS, sentence)
    }
}

impl Rule for PreReformPortugueseDashRule {
    fn id(&self) -> &str {
        "PT_PREAO_DASH_RULE"
    }

    fn description(&self) -> &str {
        "Travessões no lugar de hífens"
    }

    fn is_picky(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("TYPOGRAPHY", "Typography")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Typographical
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        dash_matches(self.id(), &PRE_REFORM_DASH_VARIANTS, sentence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pt::test_util::make_sentence;

    #[test]
    fn suggests_hyphen_for_en_dash_compound() {
        let rule = PostReformPortugueseDashRule::new();
        let sentence = make_sentence("Vamos analisar os decretos–lei.");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "decretos-lei");
    }

    #[test]
    fn spaced_em_dash_also_matched() {
        let rule = PostReformPortugueseDashRule::new();
        let sentence = make_sentence("Os guarda — chuvas molharam.");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches[0].replacements()[0].value(), "guarda-chuvas");
    }

    #[test]
    fn plain_hyphen_not_matched() {
        let rule = PostReformPortugueseDashRule::new();
        let sentence = make_sentence("Vamos analisar os decretos-lei.");
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn substring_of_word_not_matched() {
        let rule = PostReformPortugueseDashRule::new();
        let sentence = make_sentence("xguarda–chuvas grande.");
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn pre_reform_rule_matches_the_pre_reform_dataset() {
        let rule = PreReformPortugueseDashRule::new();
        assert_eq!(rule.id(), "PT_PREAO_DASH_RULE");
        let sentence = make_sentence("Foi um fato a–propósito.");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "a-propósito");
        assert!(
            PostReformPortugueseDashRule::new()
                .match_sentence(&sentence)
                .is_empty(),
            "'a-propósito' is pre-reform-only data and must not fire the post-reform rule"
        );
    }

    #[test]
    fn shared_start_with_a_nested_dataset_word_reports_once() {
        let rule = PostReformPortugueseDashRule::new();
        for (text, covered, replacement) in [
            (
                "\u{e1}gua\u{2013}marinha\u{2013}m\u{e9}dia \u{e9} bonita.",
                "\u{e1}gua\u{2013}marinha\u{2013}m\u{e9}dia",
                "\u{e1}gua-marinha-m\u{e9}dia",
            ),
            (
                "\u{c1}gua\u{2014}marinha\u{2014}m\u{e9}dia aqui.",
                "\u{c1}gua\u{2014}marinha\u{2014}m\u{e9}dia",
                "\u{c1}gua-marinha-m\u{e9}dia",
            ),
        ] {
            let sentence = make_sentence(text);
            let matches = rule.match_sentence(&sentence);
            assert_eq!(matches.len(), 1, "{text}: {matches:?}");
            assert_eq!(
                &text[matches[0].offset()..matches[0].offset() + matches[0].length()],
                covered
            );
            assert_eq!(matches[0].replacements()[0].value(), replacement);
        }
    }

    #[test]
    fn metadata_pins_both_rule_registrations() {
        let post = PostReformPortugueseDashRule::default();
        assert_eq!(post.id(), "PT_POSAO_DASH_RULE");
        assert_eq!(post.description(), "Travessões no lugar de hífens");
        assert_eq!(post.category().id().as_str(), "TYPOGRAPHY");
        assert!(matches!(post.issue_type(), IssueType::Typographical));
        assert!(post.is_picky());

        let pre = PreReformPortugueseDashRule::default();
        assert_eq!(pre.id(), "PT_PREAO_DASH_RULE");
        assert_eq!(pre.description(), "Travessões no lugar de hífens");
        assert_eq!(pre.category().id().as_str(), "TYPOGRAPHY");
        assert!(matches!(pre.issue_type(), IssueType::Typographical));
        assert!(pre.is_picky());
    }
}
