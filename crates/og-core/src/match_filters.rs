use crate::rule_match::RuleMatch;

const NEGATIVE_CONSTANT: i32 = i32::MIN + 10_000;

const ERRORS_CORRECTED_ALL_AT_ONCE: &[&str] = &[
    "FALSCHES_ANFUEHRUNGSZEICHEN",
    "TYPOGRAFISCHE_ANFUEHRUNGSZEICHEN",
    "TYPOGRAFISCHE_ANFUEHRUNGSZEICHEN_CH",
    "TYPOGRAFISCHE_ANFUEHRUNGSZEICHEN_2",
    "TYPOGRAFISCHE_APOSTROPHE",
    "EN_QUOTES",
    "WRONG_APOSTROPHE",
    "PARAGRAF_LEERZEICHEN",
    "CONSISTENCYRULE_JE_F",
    "CONSISTENCYRULE_JE_M",
    "COMILLAS_TIPOGRAFICAS",
    "COMETES_TIPOGRAFIQUES",
    "TYPOGRAFISCHE_AANHALINGSTEKENS",
    "PT_SMART_QUOTES",
    "GL_SMART_QUOTES",
];

pub trait LanguageMatchFilter: Send + Sync {
    fn priority_for_id(&self, _id: &str) -> i32 {
        base_priority_for_id(_id)
    }

    fn rule_priority(&self, rule_match: &RuleMatch) -> i32 {
        let rule_priority = self.priority_for_id(rule_match.rule().id());
        if rule_priority != 0 {
            return rule_priority;
        }
        let category_priority = self.priority_for_id(rule_match.rule().category().id().as_str());
        if category_priority != 0 {
            return category_priority;
        }
        0
    }

    fn filter_rule_matches(&self, _matches: &mut [RuleMatch], _text: &str) {}
}

pub struct BaseLanguageMatchFilter;

impl LanguageMatchFilter for BaseLanguageMatchFilter {}

pub fn base_priority_for_id(id: &str) -> i32 {
    if id.eq_ignore_ascii_case("TOO_LONG_SENTENCE") {
        return -101;
    }
    if id == "REPETITIONS_STYLE" {
        return -55;
    }
    if id.contains("STYLE") {
        return -50;
    }
    0
}

pub fn same_rule_group_filter(matches: &mut Vec<RuleMatch>) {
    matches.sort_by_key(|m| m.offset());
    let mut keep = vec![true; matches.len()];
    let mut i = 0;
    while i < matches.len() {
        let anchor = &matches[i];
        let mut j = i + 1;
        while j < matches.len()
            && overlaps(anchor, &matches[j])
            && anchor.rule().id() == matches[j].rule().id()
        {
            keep[j] = false;
            j += 1;
        }
        i = j;
    }
    compact(matches, &keep);
}

fn compact(matches: &mut Vec<RuleMatch>, keep: &[bool]) {
    let mut write = 0;
    for (read, &kept) in keep.iter().enumerate() {
        if kept {
            matches.swap(read, write);
            write += 1;
        }
    }
    matches.truncate(write);
}

fn overlaps(a: &RuleMatch, b: &RuleMatch) -> bool {
    a.offset() <= b.offset() + b.length() && a.offset() + a.length() >= b.offset()
}

pub fn clean_overlapping_filter(
    matches: Vec<RuleMatch>,
    text: &str,
    filter: &dyn LanguageMatchFilter,
) -> Vec<RuleMatch> {
    let mut clean: Vec<RuleMatch> = Vec::with_capacity(matches.len());
    let mut iter = matches.into_iter();
    let Some(mut prev) = iter.next() else {
        return clean;
    };
    for m in iter {
        let mut is_duplicate_suggestion = false;
        if let (Some(suggestion), Some(prev_suggestion)) =
            (first_replacement(&m), first_replacement(&prev))
        {
            if m.offset() == prev.offset() + prev.length()
                && prev_suggestion.ends_with(',')
                && suggestion.starts_with(", ")
            {
                is_duplicate_suggestion = true;
            }
            if !is_duplicate_suggestion
                && java_index_of_space(suggestion) > 0
                && java_index_of_space(prev_suggestion) > 0
                && m.offset() == prev.offset() + prev.length() + 1
            {
                let parts = java_split_space(suggestion);
                let parts_prev = java_split_space(prev_suggestion);
                if parts_prev.len() > 1 && parts.len() > 1 && parts_prev[1] == parts[0] {
                    is_duplicate_suggestion = true;
                }
            }
        }

        if m.offset() >= prev.offset() + prev.length() && !is_duplicate_suggestion {
            clean.push(prev);
            prev = m;
            continue;
        }

        let mut current_priority = filter.rule_priority(&m);
        if m.rule().is_picky() && current_priority != i32::MIN {
            current_priority += NEGATIVE_CONSTANT;
        }
        let mut prev_priority = filter.rule_priority(&prev);
        if prev.rule().is_picky() && prev_priority != i32::MIN {
            prev_priority += NEGATIVE_CONSTANT;
        }
        let current_is_punctuation_only = is_punctuation_only_change(&m, text);
        let prev_is_punctuation_only = is_punctuation_only_change(&prev, text);
        if current_is_punctuation_only && prev_is_punctuation_only {
            let current_included_all_at_once =
                ERRORS_CORRECTED_ALL_AT_ONCE.contains(&m.rule().id());
            let prev_included_all_at_once =
                ERRORS_CORRECTED_ALL_AT_ONCE.contains(&prev.rule().id());
            if current_included_all_at_once != prev_included_all_at_once {
                if current_included_all_at_once {
                    if current_priority < prev_priority {
                        current_priority = prev_priority + 1;
                    }
                } else if prev_priority < current_priority {
                    prev_priority = current_priority + 1;
                }
            }
        }
        if current_priority == prev_priority {
            current_priority = m.length() as i32;
            prev_priority = prev.length() as i32;
        }
        if current_priority == prev_priority {
            current_priority += 1;
        }
        if current_priority > prev_priority {
            prev = m;
        }
    }
    clean.push(prev);
    clean
}

fn first_replacement(m: &RuleMatch) -> Option<&str> {
    m.replacements().first().map(|r| r.value())
}

fn java_index_of_space(s: &str) -> i32 {
    s.find(' ').map_or(-1, |i| i as i32)
}

fn java_split_space(s: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = s.split(' ').collect();
    while parts.last() == Some(&"") {
        parts.pop();
    }
    if parts.len() == 1 && parts[0].is_empty() {
        parts.clear();
    }
    parts
}

pub fn original_error_str<'a>(m: &'a RuleMatch, text: &'a str) -> &'a str {
    let end = m.offset() + m.length();
    if m.length() == 0
        || end > text.len()
        || !text.is_char_boundary(m.offset())
        || !text.is_char_boundary(end)
    {
        return "";
    }
    &text[m.offset()..end]
}

fn is_punctuation_only_change(m: &RuleMatch, text: &str) -> bool {
    let Some(replacement) = first_replacement(m) else {
        return false;
    };
    let original = original_error_str(m, text);
    if original.is_empty() || replacement == original {
        return false;
    }
    let normalized_original: String = original.chars().filter(|c| c.is_alphanumeric()).collect();
    let normalized_replacement: String = replacement
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    normalized_original == normalized_replacement
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Category;
    use crate::rule_match::{RuleMatchContext, RuleMatchRule};

    struct PriorityFilter;

    impl LanguageMatchFilter for PriorityFilter {
        fn priority_for_id(&self, id: &str) -> i32 {
            match id {
                "P3_RULE" => 3,
                "P2_RULE" | "COMMA_HIGH_PRIORITY" | "MISSING_THE_HIGH_PRIORITY" => 2,
                "P1_RULE"
                | "P1_RULE_B"
                | "COMMA_LOW_PRIORITY"
                | "COMMA_LOW_PRIORITY2"
                | "MISSING_THE_LOW_PRIORITY" => 1,
                _ => base_priority_for_id(id),
            }
        }
    }

    fn m(id: &str, from: usize, to: usize, replacement: Option<&str>) -> RuleMatch {
        let rule = RuleMatchRule::new(id, "desc");
        let mut rule_match = RuleMatch::new(
            "msg",
            from,
            to - from,
            rule,
            RuleMatchContext::new("ctx", from, to - from),
        );
        if let Some(replacement) = replacement {
            rule_match =
                rule_match.with_replacements(vec![crate::rule_match::SuggestedReplacement::new(
                    replacement,
                )]);
        }
        rule_match
    }

    fn clean(matches: Vec<RuleMatch>) -> Vec<RuleMatch> {
        clean_overlapping_filter(matches, "0123456789 123456789 123456789", &PriorityFilter)
    }

    #[test]
    fn same_rule_group_removes_overlapping_same_id_matches() {
        let mut matches = vec![m("id1", 10, 20, None), m("id1", 15, 25, None)];
        same_rule_group_filter(&mut matches);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn same_rule_group_keeps_non_overlapping_same_id_matches() {
        let mut matches = vec![m("id1", 10, 20, None), m("id1", 21, 25, None)];
        same_rule_group_filter(&mut matches);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn same_rule_group_keeps_overlapping_different_id_matches() {
        let mut matches = vec![m("id1", 10, 20, None), m("id2", 15, 25, None)];
        same_rule_group_filter(&mut matches);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn same_rule_group_sorts_by_offset() {
        let mut matches = vec![m("id1", 15, 25, None), m("id2", 0, 10, None)];
        same_rule_group_filter(&mut matches);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[1].offset(), 15);
    }

    #[test]
    fn clean_empty_input() {
        assert_eq!(clean(Vec::new()).len(), 0);
    }

    #[test]
    fn clean_keeps_non_overlapping_matches() {
        let result = clean(vec![m("P1_RULE", 0, 10, None), m("P1_RULE", 11, 20, None)]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn clean_keeps_juxtaposed_matches() {
        let result = clean(vec![m("P1_RULE", 0, 10, None), m("P1_RULE", 10, 20, None)]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn clean_equal_priority_takes_longest() {
        let result = clean(vec![m("P1_RULE", 0, 10, None), m("P1_RULE_B", 9, 20, None)]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rule().id(), "P1_RULE_B");
    }

    #[test]
    fn clean_equal_priority_equal_length_takes_latest() {
        let result = clean(vec![m("P1_RULE_B", 0, 10, None), m("P1_RULE", 9, 20, None)]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rule().id(), "P1_RULE");
    }

    #[test]
    fn clean_higher_priority_wins_from_either_side() {
        let result = clean(vec![m("P1_RULE", 0, 10, None), m("P2_RULE", 9, 20, None)]);
        assert_eq!(result[0].rule().id(), "P2_RULE");
        let result = clean(vec![m("P2_RULE", 0, 10, None), m("P1_RULE", 9, 20, None)]);
        assert_eq!(result[0].rule().id(), "P2_RULE");
    }

    #[test]
    fn clean_picky_rule_loses_despite_higher_priority() {
        let mut picky = m("P2_RULE", 0, 10, None);
        picky.set_picky(true);
        let result = clean(vec![picky, m("P1_RULE", 9, 20, None)]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rule().id(), "P1_RULE");
        let mut picky = m("P2_RULE", 9, 20, None);
        picky.set_picky(true);
        let result = clean(vec![m("P1_RULE", 0, 10, None), picky]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rule().id(), "P1_RULE");
    }

    #[test]
    fn clean_juxtaposed_comma_duplicate() {
        let result = clean(vec![
            m("COMMA_LOW_PRIORITY", 5, 10, Some("right,")),
            m("COMMA_HIGH_PRIORITY", 10, 15, Some(", left")),
        ]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rule().id(), "COMMA_HIGH_PRIORITY");

        let result = clean(vec![
            m("COMMA_HIGH_PRIORITY", 5, 10, Some("right,")),
            m("COMMA_LOW_PRIORITY", 10, 15, Some(", left")),
        ]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rule().id(), "COMMA_HIGH_PRIORITY");

        let result = clean(vec![
            m("COMMA_LOW_PRIORITY2", 5, 10, Some("right,")),
            m("COMMA_LOW_PRIORITY", 10, 15, Some(", left")),
        ]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rule().id(), "COMMA_LOW_PRIORITY");
    }

    #[test]
    fn clean_duplicate_suggestion_same_position() {
        let result = clean(vec![
            m("MISSING_THE_HIGH_PRIORITY", 5, 10, Some("of the")),
            m("MISSING_THE_LOW_PRIORITY", 11, 15, Some("the provisions")),
        ]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rule().id(), "MISSING_THE_HIGH_PRIORITY");
    }

    #[test]
    fn punctuation_only_all_at_once_preference() {
        struct PunctFilter;
        impl LanguageMatchFilter for PunctFilter {
            fn priority_for_id(&self, id: &str) -> i32 {
                if id == "PUNCT_OTHER" {
                    1
                } else {
                    base_priority_for_id(id)
                }
            }
        }
        let matches = vec![
            m("PUNCT_OTHER", 5, 10, Some("foo")),
            m("EN_QUOTES", 5, 10, Some("‘foo’")),
        ];
        let result = clean_overlapping_filter(matches, "aaaa 'foo' bbbb", &PunctFilter);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rule().id(), "EN_QUOTES");
    }

    #[test]
    fn java_split_space_matches_java_semantics() {
        assert_eq!(java_split_space("a b"), vec!["a", "b"]);
        assert_eq!(java_split_space("a  b"), vec!["a", "", "b"]);
        assert_eq!(java_split_space("a "), vec!["a"]);
        assert_eq!(java_split_space(" a"), vec!["", "a"]);
        assert!(java_split_space(" ").is_empty());
    }

    #[test]
    fn base_priority_defaults() {
        assert_eq!(base_priority_for_id("TOO_LONG_SENTENCE"), -101);
        assert_eq!(base_priority_for_id("too_long_sentence"), -101);
        assert_eq!(base_priority_for_id("REPETITIONS_STYLE"), -55);
        assert_eq!(base_priority_for_id("TYPOGRAPHY_STYLE"), -50);
        assert_eq!(base_priority_for_id("SOME_RULE"), 0);
    }

    #[test]
    fn rule_priority_prefers_rule_id_over_category() {
        let filter = BaseLanguageMatchFilter;
        let by_id = RuleMatch::new(
            "m",
            0,
            1,
            RuleMatchRule::new("REPETITIONS_STYLE", "d")
                .with_category(Category::new("TOO_LONG_SENTENCE", "Long")),
            RuleMatchContext::new("c", 0, 1),
        );
        assert_eq!(filter.rule_priority(&by_id), -55);
        let by_category = RuleMatch::new(
            "m",
            0,
            1,
            RuleMatchRule::new("PLAIN_RULE", "d").with_category(Category::new("MY_STYLE", "Style")),
            RuleMatchContext::new("c", 0, 1),
        );
        assert_eq!(filter.rule_priority(&by_category), -50);
        let plain = m("PLAIN_RULE", 0, 1, None);
        assert_eq!(filter.rule_priority(&plain), 0);
    }

    #[test]
    fn base_filter_rule_matches_is_a_no_op() {
        let mut matches = vec![m("A", 0, 5, None), m("B", 6, 9, None)];
        BaseLanguageMatchFilter.filter_rule_matches(&mut matches, "text");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].offset(), 0);
    }

    #[test]
    fn original_error_str_slices_the_matched_span() {
        let text = "0123456789 123456789 123456789";
        assert_eq!(original_error_str(&m("R", 2, 5, None), text), "234");
        assert_eq!(original_error_str(&m("R", 0, 30, None), text), text);
    }

    #[test]
    fn original_error_str_guards_degenerate_spans() {
        let text = "0123456789";
        assert_eq!(original_error_str(&m("R", 3, 3, None), text), "");
        assert_eq!(original_error_str(&m("R", 8, 15, None), text), "");
    }

    #[test]
    fn original_error_str_rejects_non_char_boundaries() {
        let text = "héllo wörld";
        assert_eq!(original_error_str(&m("R", 2, 3, None), text), "");
        assert_eq!(original_error_str(&m("R", 0, 2, None), text), "");
        assert_eq!(original_error_str(&m("R", 1, 4, None), text), "él");
        assert_eq!(original_error_str(&m("R", 0, 1, None), text), "h");
    }
}
