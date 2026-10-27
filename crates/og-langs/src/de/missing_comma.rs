use super::agreement::immunized_indices;
use super::helpers::{has_pos_tag_starting_with, is_pos_tag_unknown, matches_pos_tag_regex};
use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use std::collections::HashSet;
use std::sync::LazyLock;

static MARKS_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        "\\A[,;.:?\u{2022}!\u{2013}\u{2014}\u{2019}'\"\u{201E}\u{201C}\u{201D}\u{2026}\u{00BB}\u{00AB}\u{201A}\u{2018}\u{203A}\u{2039}()/\\[\\]]\\z",
    )
    .unwrap()
});
static PRONOUN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\A(d(e[mnr]|ie|as|e([nr]|ss)en)|welche[mrs]?|wessen|was)\z").unwrap()
});
static ZAL_ETC_PATTERN: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\A(ZAL|AD[JV]|ART|SUB|PRO:POS|PRP).*\z").unwrap());
static GENDER_PART: [&str; 4] = ["SIN:FEM", "SIN:MAS", "SIN:NEU", "PLU"];

pub static MISSING_COMMA_ANTI_PATTERNS: LazyLock<Option<Vec<crate::engine::XmlPatternRule>>> =
    LazyLock::new(|| {
        let compiler = og_xml::XmlCompiler::new();
        match compiler.compile_file(include_str!("missing_comma_antipatterns.xml")) {
            Ok(set) => Some(
                set.rules
                    .into_iter()
                    .map(crate::engine::XmlPatternRule::new)
                    .collect(),
            ),
            Err(_) => None,
        }
    });

fn is_separator(token: &str) -> bool {
    MARKS_REGEX.is_match(token) || token == "und" || token == "oder"
}

fn next_separator(tokens: &[&AnalyzedTokenReadings], start: usize) -> usize {
    for (i, t) in tokens.iter().enumerate().skip(start) {
        if is_separator(t.token().token()) {
            return i;
        }
    }
    tokens.len() - 1
}

fn is_prp(token: &AnalyzedTokenReadings, immunized: &HashSet<usize>, idx: usize) -> bool {
    has_pos_tag_starting_with(token, "PRP:") && !immunized.contains(&idx)
}

fn is_verb(tokens: &[&AnalyzedTokenReadings], n: usize, immunized: &HashSet<usize>) -> bool {
    matches_pos_tag_regex(tokens[n], "(VER:[1-3]:|VER:.*:[1-3]:).*")
        && !matches_any(tokens[n], &ZAL_ETC_PATTERN)
        && (!has_pos_tag_starting_with(tokens[n], "VER:INF:")
            || tokens
                .get(n.wrapping_sub(1))
                .is_none_or(|t| t.token().token() != "zu"))
        && !immunized.contains(&n)
}

fn matches_any(token: &AnalyzedTokenReadings, re: &regex::Regex) -> bool {
    token
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| re.is_match(t)))
}

fn is_any_verb(tokens: &[&AnalyzedTokenReadings], n: usize) -> bool {
    has_pos_tag_starting_with(tokens[n], "VER:")
        || (n + 1 < tokens.len()
            && ((tokens[n].token().token() == "zu"
                && has_pos_tag_starting_with(tokens[n + 1], "VER:INF:"))
                || (tokens[n].has_pos_tag("NEG")
                    && has_pos_tag_starting_with(tokens[n + 1], "VER:"))))
}

fn verb_pos(
    tokens: &[&AnalyzedTokenReadings],
    start: usize,
    end: usize,
    immunized: &HashSet<usize>,
) -> Vec<usize> {
    let mut verbs = Vec::new();
    for i in start..end {
        if is_verb(tokens, i, immunized) {
            if has_pos_tag_starting_with(tokens[i], "PA") {
                let gender = get_gender(tokens[i]);
                let s_str = format!("(ADJ|PA[12]):.*{gender}.*");
                let mut j = i + 1;
                while j < end && matches_pos_tag_regex(tokens[j], &s_str) {
                    j += 1;
                }
                let sub_str = format!("(SUB|EIG):.*{gender}.*");
                let no_sub = tokens
                    .get(j)
                    .is_some_and(|t| !matches_pos_tag_regex(t, &sub_str) && !is_pos_tag_unknown(t));
                if no_sub {
                    verbs.push(i);
                }
            } else {
                verbs.push(i);
            }
        }
    }
    verbs
}

fn is_kon_unt(token: &AnalyzedTokenReadings) -> bool {
    token.has_pos_tag("KON:UNT")
        || ["wer", "wo", "wohin"]
            .iter()
            .any(|w| w.eq_ignore_ascii_case(token.token().token()))
}

fn has_potential_subclause(
    tokens: &[&AnalyzedTokenReadings],
    start: usize,
    end: usize,
    immunized: &HashSet<usize>,
) -> isize {
    let verbs = verb_pos(tokens, start, end, immunized);
    if verbs.len() == 1 && end + 2 < tokens.len() && verbs[0] == end - 1 {
        let next_end = next_separator(tokens, end + 1);
        let next_verbs = verb_pos(tokens, end + 1, next_end, immunized);
        if is_kon_unt(tokens[start]) {
            if next_verbs.len() > 1 || (next_verbs.len() == 1 && next_verbs[0] == end - 1) {
                return verbs[0] as isize;
            }
        } else if !next_verbs.is_empty() {
            return verbs[0] as isize;
        }
        return -1;
    }
    if verbs.len() == 2 {
        if matches_pos_tag_regex(tokens[verbs[0]], "VER:(MOD|AUX):.*")
            && has_pos_tag_starting_with(tokens[verbs[1]], "VER:INF:")
        {
            return verbs[0] as isize;
        }
        if has_pos_tag_starting_with(tokens[verbs[0]], "VER:AUX:")
            && has_pos_tag_starting_with(tokens[verbs[1]], "VER:PA2:")
        {
            return -1;
        }
        if end == tokens.len() - 1
            && verbs[0] == end - 2
            && has_pos_tag_starting_with(tokens[verbs[0]], "VER:INF:")
            && has_pos_tag_starting_with(tokens[verbs[1]], "VER:MOD:")
        {
            return -1;
        }
    }
    if verbs.len() == 3
        && has_pos_tag_starting_with(tokens[verbs[0]], "VER:MOD:")
        && ((matches_pos_tag_regex(tokens[verbs[2] - 1], "VER:(INF|PA2):.*")
            && has_pos_tag_starting_with(tokens[verbs[2]], "VER:INF:"))
            || (tokens[verbs[1] - 1].token().token() == "weder"
                && has_pos_tag_starting_with(tokens[verbs[1]], "VER:INF:")
                && tokens[verbs[2] - 1].token().token() == "noch"
                && has_pos_tag_starting_with(tokens[verbs[1]], "VER:INF:")))
    {
        return -1;
    }
    if verbs.len() > 1 {
        return verbs[verbs.len() - 1] as isize;
    }
    -1
}

fn is_pronoun(tokens: &[&AnalyzedTokenReadings], n: usize) -> bool {
    PRONOUN.is_match(tokens[n].token().token())
        && tokens
            .get(n.wrapping_sub(1))
            .is_none_or(|t| t.token().token() != "sowie")
}

fn get_gender(token: &AnalyzedTokenReadings) -> String {
    let mut ret = String::new();
    let mut n_matches = 0;
    for (part, re) in GENDER_PART.iter().zip([
        r"\A.*:SIN:FEM.*\z",
        r"\A.*:SIN:MAS.*\z",
        r"\A.*:SIN:NEU.*\z",
        r"\A.*:PLU.*\z",
    ]) {
        if matches_pos_tag_regex(token, re) {
            if n_matches > 0 {
                ret.push('|');
            }
            ret.push_str(part);
            n_matches += 1;
        }
    }
    if n_matches > 1 {
        format!("({ret})")
    } else {
        ret
    }
}

fn matches_gender_with(
    gender: &str,
    tokens: &[&AnalyzedTokenReadings],
    from: usize,
    to: usize,
) -> bool {
    let m_str = if gender.is_empty() {
        "PRO:DEM:.*SIN:NEU.*".to_string()
    } else {
        format!("(SUB|EIG):.*{gender}.*")
    };
    for i in (from..to).rev() {
        if matches_pos_tag_regex(tokens[i], &m_str)
            && (i != 1 || !has_pos_tag_starting_with(tokens[i], "VER:"))
        {
            return true;
        }
    }
    false
}

fn is_article_without_sub(gender: &str, tokens: &[&AnalyzedTokenReadings], n: usize) -> bool {
    if gender.is_empty() {
        return false;
    }
    has_pos_tag_starting_with(tokens[n], "VER:")
        && n > 0
        && matches_pos_tag_regex(tokens[n - 1], &format!("(ADJ|PA[12]|PRO:POS):.*{gender}.*"))
}

fn skip_sub(tokens: &[&AnalyzedTokenReadings], n: usize, to: usize) -> isize {
    let gender = get_gender(tokens[n]);
    let s_str = format!("(SUB|EIG):.*{gender}.*");
    for (i, token) in tokens.iter().enumerate().take(to).skip(n + 1) {
        if matches_pos_tag_regex(token, &s_str) {
            return i as isize;
        }
    }
    -1
}

fn skip_to_sub(gender: &str, tokens: &[&AnalyzedTokenReadings], n: usize, to: usize) -> isize {
    let pa_str = format!("PA[12]:.*{gender}.*");
    if tokens
        .get(n + 1)
        .is_some_and(|t| matches_pos_tag_regex(t, &pa_str))
    {
        return (n + 1) as isize;
    }
    let adj_str = format!("(ADJ|PA[12]):.*{gender}.*");
    let mut i = n + 1;
    while i < to {
        if matches_pos_tag_regex(tokens[i], &adj_str) || is_pos_tag_unknown(tokens[i]) {
            return i as isize;
        }
        if has_pos_tag_starting_with(tokens[i], "ART") {
            let skip = skip_sub(tokens, i, to);
            if skip < 0 {
                return skip;
            }
            i = skip as usize;
        }
        i += 1;
    }
    -1
}

fn is_article(gender: &str, tokens: &[&AnalyzedTokenReadings], from: usize, to: usize) -> bool {
    if gender.is_empty() {
        return false;
    }
    let s_sub = format!("(SUB|EIG):.*{gender}.*");
    let s_adj =
        format!("(ZAL|PRP:|KON:|ADV:|ADJ:PRD:|(ADJ|PA[12]|PRO:(POS|DEM|IND)):.*{gender}).*");
    let mut i = from + 1;
    while i < to {
        if matches_pos_tag_regex(tokens[i], &s_sub) || is_pos_tag_unknown(tokens[i]) {
            return true;
        }
        if has_pos_tag_starting_with(tokens[i], "ART") || !matches_pos_tag_regex(tokens[i], &s_adj)
        {
            if is_article_without_sub(gender, tokens, i) {
                return true;
            }
            let skip_to = skip_to_sub(gender, tokens, i, to);
            if skip_to > 0 {
                i = skip_to as usize;
            } else {
                return false;
            }
        }
        i += 1;
    }
    to < tokens.len() && is_article_without_sub(gender, tokens, to)
}

fn missed_comma_in_front(
    tokens: &[&AnalyzedTokenReadings],
    start: usize,
    _end: usize,
    last_verb: usize,
    immunized: &HashSet<usize>,
) -> isize {
    for i in start..last_verb.saturating_sub(1) {
        if immunized.contains(&i) {
            continue;
        }
        if is_pronoun(tokens, i) {
            let gender = get_gender(tokens[i]);
            if !is_any_verb(tokens, i + 1)
                && matches_gender_with(&gender, tokens, start, i)
                && !is_article(&gender, tokens, i, last_verb)
            {
                return i as isize;
            }
        }
    }
    -1
}

fn is_two_combined_verbs(first: &AnalyzedTokenReadings, second: &AnalyzedTokenReadings) -> bool {
    matches_pos_tag_regex(first, "(VER:.*INF|.*PA[12]:).*")
        && has_pos_tag_starting_with(second, "VER:")
}

fn is_three_combined_verbs(tokens: &[&AnalyzedTokenReadings], first: usize, last: usize) -> bool {
    matches_pos_tag_regex(tokens[first], "VER:(AUX|INF|PA[12]).*")
        && matches_pos_tag_regex(tokens[first + 1], "VER:(.*INF|PA[12]).*")
        && matches_pos_tag_regex(tokens[last], "VER:(MOD|AUX).*")
}

fn is_four_combined_verbs(tokens: &[&AnalyzedTokenReadings], first: usize, last: usize) -> bool {
    has_partial(tokens[first], "KJ2")
        && has_partial(tokens[first + 1], "PA2")
        && matches_pos_tag_regex(tokens[first + 2], "VER:(.*INF|PA[12]).*")
        && matches_pos_tag_regex(tokens[last], "VER:(MOD|AUX).*")
}

fn has_partial(token: &AnalyzedTokenReadings, needle: &str) -> bool {
    token
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.contains(needle)))
}

fn is_par(token: &AnalyzedTokenReadings) -> bool {
    has_pos_tag_starting_with(token, "PA2:") || has_pos_tag_starting_with(token, "VER:PA2")
}

fn is_infinitiv_zu(tokens: &[&AnalyzedTokenReadings], last: usize) -> bool {
    last > 0
        && tokens[last - 1].token().token() == "zu"
        && matches_pos_tag_regex(tokens[last], "VER:.*INF.*")
}

fn is_two_plus_combined_verbs(
    tokens: &[&AnalyzedTokenReadings],
    first: usize,
    last: usize,
) -> bool {
    matches_pos_tag_regex(tokens[first], ".*PA[12]:.*")
        && matches_pos_tag_regex(tokens[last - 1], "VER:.*INF.*")
}

fn is_kon_after_verb(tokens: &[&AnalyzedTokenReadings], start: usize, end: usize) -> bool {
    if matches_pos_tag_regex(tokens[start], "VER:(MOD|AUX).*")
        && matches_pos_tag_regex(tokens[start + 1], "(KON|PRP).*")
    {
        if start + 3 == end {
            return true;
        }
        for token in &tokens[start + 2..end] {
            if matches_pos_tag_regex(token, "(SUB|PRO:PER).*") {
                return true;
            }
        }
    }
    false
}

fn is_special_pair(tokens: &[&AnalyzedTokenReadings], first: usize, second: usize) -> bool {
    if first + 3 >= second
        && matches_pos_tag_regex(tokens[first], "VER:.*INF.*")
        && matches!(tokens[first + 1].token().token(), "als" | "noch")
        && matches_pos_tag_regex(tokens[first + 2], "VER:.*INF.*")
    {
        if first + 2 == second {
            return true;
        }
        return is_two_combined_verbs(tokens[second - 1], tokens[second]);
    }
    false
}

fn is_perfect(tokens: &[&AnalyzedTokenReadings], first: usize, second: usize) -> bool {
    has_pos_tag_starting_with(tokens[first], "VER:AUX:")
        && matches_pos_tag_regex(tokens[second], "VER:.*(INF|PA2).*")
}

fn is_special_inf(
    tokens: &[&AnalyzedTokenReadings],
    first: usize,
    second: usize,
    start: usize,
) -> bool {
    if !has_pos_tag_starting_with(tokens[first], "VER:INF") {
        return false;
    }
    for i in ((start + 1)..first).rev() {
        if has_pos_tag_starting_with(tokens[i], "ART") {
            let i = skip_sub(tokens, i, second);
            return i > 0;
        }
    }
    false
}

fn is_separator_or_inf(tokens: &[&AnalyzedTokenReadings], n: usize) -> bool {
    is_separator(tokens[n].token().token())
        || has_pos_tag_starting_with(tokens[n], "VER:INF")
        || (tokens.len() > n + 1
            && tokens[n].token().token() == "zu"
            && matches_pos_tag_regex(tokens[n + 1], "VER:.*INF.*"))
}

fn get_comma_behind(
    tokens: &[&AnalyzedTokenReadings],
    verbs: &[usize],
    start: usize,
    end: usize,
) -> isize {
    if verbs.len() == 1 {
        if tokens
            .get(verbs[0] + 1)
            .is_some_and(|t| is_separator(t.token().token()))
        {
            return -1;
        }
        return verbs[0] as isize;
    } else if verbs.len() == 2 {
        if is_special_pair(tokens, verbs[0], verbs[1]) {
            if is_separator_or_inf(tokens, verbs[1] + 1) {
                return -1;
            }
            return verbs[1] as isize;
        } else if verbs[0] + 1 == verbs[1] {
            if is_two_combined_verbs(tokens[verbs[0]], tokens[verbs[1]]) {
                if is_separator_or_inf(tokens, verbs[1] + 1)
                    || is_kon_after_verb(tokens, verbs[1], end)
                {
                    return -1;
                }
                return verbs[1] as isize;
            }
        } else if verbs[0] + 2 == verbs[1]
            && is_three_combined_verbs(tokens, verbs[0], verbs[1])
            && is_separator_or_inf(tokens, verbs[1] + 1)
        {
            return -1;
        } else if verbs[0] + 2 == verbs[1] && is_three_combined_verbs(tokens, verbs[0], verbs[1]) {
            return verbs[1] as isize;
        }
        if is_par(tokens[verbs[0]])
            || is_perfect(tokens, verbs[0], verbs[1])
            || is_infinitiv_zu(tokens, verbs[1])
            || is_special_inf(tokens, verbs[0], verbs[1], start)
        {
            if is_separator_or_inf(tokens, verbs[1] + 1) {
                return -1;
            }
            return verbs[1] as isize;
        }
    } else if verbs.len() == 3 {
        if is_two_plus_combined_verbs(tokens, verbs[0], verbs[2]) {
            if is_separator_or_inf(tokens, verbs[2] + 1) {
                return -1;
            }
            return verbs[2] as isize;
        } else if verbs[0] + 2 == verbs[2] {
            if verbs[0] + 1 == verbs[1]
                && is_three_combined_verbs(tokens, verbs[0], verbs[2])
                && is_separator_or_inf(tokens, verbs[2] + 1)
            {
                return -1;
            }
            if verbs[0] + 1 == verbs[1] && is_three_combined_verbs(tokens, verbs[0], verbs[2]) {
                return verbs[2] as isize;
            }
        } else if (verbs[0] + 3 == verbs[2] && is_four_combined_verbs(tokens, verbs[0], verbs[2]))
            || (has_pos_tag_starting_with(tokens[verbs[2]], "VER:MOD:")
                && is_special_pair(tokens, verbs[0], verbs[1]))
        {
            if is_separator_or_inf(tokens, verbs[2] + 1) {
                return -1;
            }
            return verbs[2] as isize;
        }
        if matches_pos_tag_regex(tokens[verbs[1]], "VER:.*INF.*")
            && is_perfect(tokens, verbs[0], verbs[2])
        {
            if is_separator_or_inf(tokens, verbs[2] + 1) {
                return -1;
            }
            return verbs[1] as isize;
        }
    }
    verbs[0] as isize
}

fn missed_comma_behind(
    tokens: &[&AnalyzedTokenReadings],
    in_front: usize,
    start: usize,
    end: usize,
    immunized: &HashSet<usize>,
) -> isize {
    for i in start..end {
        if is_pronoun(tokens, i) {
            let verbs = verb_pos(tokens, i, end, immunized);
            if !verbs.is_empty() {
                let gender = get_gender(tokens[i]);
                if !is_any_verb(tokens, i + 1)
                    && matches_gender_with(&gender, tokens, in_front, i - 1)
                    && !is_article(&gender, tokens, i, verbs[verbs.len() - 1])
                {
                    return get_comma_behind(tokens, &verbs, i, end);
                }
            }
        }
    }
    -1
}

fn get_sin_or_plu_of_pro(token: &AnalyzedTokenReadings) -> Option<String> {
    if !has_partial(token, "PRO:PER:") && !has_pos_tag_starting_with(token, "IND:") {
        return None;
    }
    let mut ret = String::new();
    let mut n_matches = 0;
    if matches_pos_tag_regex(token, r".*:SIN.*") {
        ret.push_str("SIN");
        n_matches += 1;
    }
    if matches_pos_tag_regex(token, r".*:PLU.*") {
        if !ret.is_empty() {
            ret.push('|');
        }
        ret.push_str("PLU");
        n_matches += 1;
    }
    if n_matches > 1 {
        Some(format!("({ret})"))
    } else {
        Some(ret)
    }
}

fn is_verb_pro_pair(tokens: &[&AnalyzedTokenReadings], n: usize) -> bool {
    let Some(sin_or_plu) = tokens.get(n + 1).and_then(|t| get_sin_or_plu_of_pro(t)) else {
        return false;
    };
    let re = regex::Regex::new(&format!(r"\AVER:.*{sin_or_plu}.*\z")).unwrap();
    tokens[n]
        .readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| re.is_match(t)))
}

pub struct MissingCommaRelativeClauseRule {
    behind: bool,
}

impl MissingCommaRelativeClauseRule {
    pub fn in_front() -> Self {
        Self { behind: false }
    }

    pub fn behind() -> Self {
        Self { behind: true }
    }

    fn rule_meta(&self) -> RuleMatchRule {
        RuleMatchRule::new(self.id(), self.description())
            .with_category(Category::new("HILFESTELLUNG_KOMMASETZUNG", "Kommasetzung"))
            .with_issue_type(IssueType::Punctuation.as_str())
    }
}

impl Rule for MissingCommaRelativeClauseRule {
    fn id(&self) -> &str {
        if self.behind {
            "COMMA_BEHIND_RELATIVE_CLAUSE"
        } else {
            "COMMA_IN_FRONT_RELATIVE_CLAUSE"
        }
    }

    fn description(&self) -> &str {
        if self.behind {
            "Fehlendes Komma nach Relativsatz"
        } else {
            "Fehlendes Komma vor Relativsatz"
        }
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("HILFESTELLUNG_KOMMASETZUNG", "Kommasetzung")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Punctuation
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let tokens = sentence.non_whitespace_tokens();
        if tokens.len() <= 1 {
            return rule_matches;
        }
        let immunized: HashSet<usize> = MISSING_COMMA_ANTI_PATTERNS
            .as_ref()
            .map(|rules| immunized_indices(rules, sentence))
            .unwrap_or_default();
        let mut sub_start = 1usize;
        if is_separator(tokens[sub_start].token().token()) {
            sub_start += 1;
        }
        if self.behind {
            let mut sub_in_front = sub_start;
            sub_start = next_separator(&tokens, sub_in_front) + 1;
            while sub_start < tokens.len() {
                let sub_end = next_separator(&tokens, sub_start);
                let last_verb = has_potential_subclause(&tokens, sub_start, sub_end, &immunized);
                if last_verb > 0 {
                    let n_token =
                        missed_comma_behind(&tokens, sub_in_front, sub_start, sub_end, &immunized);
                    if n_token > 0 {
                        let n_token = n_token as usize;
                        if is_verb_pro_pair(&tokens, n_token)
                            && n_token > 0
                            && n_token + 1 < tokens.len()
                        {
                            let from = tokens[n_token - 1].token().start();
                            let to = tokens[n_token + 1].token().end();
                            let t_prev = tokens[n_token - 1].token().token();
                            let t = tokens[n_token].token().token();
                            let t_next = tokens[n_token + 1].token().token();
                            let rm = RuleMatch::new(
                                "Sollten Sie hier ein Komma einfügen oder zwei?",
                                from,
                                to - from,
                                self.rule_meta(),
                                RuleMatchContext::new(sentence.text().to_string(), from, to - from),
                            )
                            .with_sentence(sentence.text().to_string())
                            .with_replacements(vec![
                                SuggestedReplacement::new(format!("{t_prev}, {t} {t_next},")),
                                SuggestedReplacement::new(format!("{t_prev} {t} {t_next},")),
                                SuggestedReplacement::new(format!("{t_prev} {t}, {t_next}")),
                            ]);
                            rule_matches.push(rm);
                        } else if n_token + 1 < tokens.len() {
                            let from = tokens[n_token].token().start();
                            let to = tokens[n_token + 1].token().end();
                            let t = tokens[n_token].token().token();
                            let t_next = tokens[n_token + 1].token().token();
                            let rm = RuleMatch::new(
                                "Sollten Sie hier ein Komma einfügen?",
                                from,
                                to - from,
                                self.rule_meta(),
                                RuleMatchContext::new(sentence.text().to_string(), from, to - from),
                            )
                            .with_sentence(sentence.text().to_string())
                            .with_replacements(vec![
                                SuggestedReplacement::new(format!("{t}, {t_next}")),
                            ]);
                            rule_matches.push(rm);
                        }
                    }
                }
                sub_in_front = sub_start;
                sub_start = sub_end + 1;
            }
        } else {
            while sub_start < tokens.len() {
                let sub_end = next_separator(&tokens, sub_start);
                let last_verb = has_potential_subclause(&tokens, sub_start, sub_end, &immunized);
                if last_verb > 0 {
                    let n_token = missed_comma_in_front(
                        &tokens,
                        sub_start,
                        sub_end,
                        last_verb as usize,
                        &immunized,
                    );
                    if n_token > 0 {
                        let n_token = n_token as usize;
                        let start_token = if n_token > 0
                            && is_prp(tokens[n_token - 1], &immunized, n_token - 1)
                        {
                            n_token - 2
                        } else {
                            n_token.saturating_sub(1)
                        };
                        if start_token < tokens.len() {
                            let from = tokens[start_token].token().start();
                            let to = tokens[n_token].token().end();
                            let mut replacements = Vec::new();
                            if n_token > start_token + 1 {
                                replacements.push(SuggestedReplacement::new(format!(
                                    "{}, {} {}",
                                    tokens[start_token].token().token(),
                                    tokens[n_token - 1].token().token(),
                                    tokens[n_token].token().token()
                                )));
                            } else {
                                replacements.push(SuggestedReplacement::new(format!(
                                    "{}, {}",
                                    tokens[start_token].token().token(),
                                    tokens[n_token].token().token()
                                )));
                            }
                            let rm = RuleMatch::new(
                                "Sowohl angehängte als auch eingeschobene Relativsätze werden durch Kommas vom Hauptsatz getrennt.",
                                from,
                                to - from,
                                self.rule_meta(),
                                RuleMatchContext::new(sentence.text().to_string(), from, to - from),
                            )
                            .with_sentence(sentence.text().to_string())
                            .with_replacements(replacements);
                            rule_matches.push(rm);
                        }
                    }
                }
                sub_start = sub_end + 1;
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::LanguageEngine;

    fn shared_engine() -> std::sync::Arc<LanguageEngine> {
        crate::engine::LanguageEngine::shared_for_code("de").expect("german engine")
    }

    #[test]
    fn in_front_flags_missing_comma_before_relative_clause() {
        let engine = shared_engine();
        for text in [
            "Das Auto das am Straßenrand steht parkt im Halteverbot.",
            "Das Auto in dem der Mann sitzt parkt im Halteverbot.",
            "Die Frau die vor dem Auto steht hat schwarze Haare.",
        ] {
            let matches = engine.check(text);
            assert!(
                matches
                    .matches
                    .iter()
                    .any(|m| m.rule().id() == "COMMA_IN_FRONT_RELATIVE_CLAUSE"),
                "{text}: no COMMA_IN_FRONT_RELATIVE_CLAUSE match"
            );
        }
    }

    #[test]
    fn behind_flags_missing_comma_after_relative_clause() {
        let engine = shared_engine();
        {
            let text = "Die Frau, die vor dem Auto steht hat schwarze Haare.";
            let matches = engine.check(text);
            assert!(
                matches
                    .matches
                    .iter()
                    .any(|m| m.rule().id() == "COMMA_BEHIND_RELATIVE_CLAUSE"),
                "{text}: no COMMA_BEHIND_RELATIVE_CLAUSE match"
            );
        }
    }

    #[test]
    fn correct_sentences_not_flagged() {
        let engine = shared_engine();
        for text in [
            "Computer machen die Leute dumm.",
            "Selbst wenn das alles perfekt verlustfrei wäre, hätte ich nichts gewonnen.",
            "Die Unstimmigkeit zwischen den Geschichten der zwei Unfallbeteiligten war groß.",
        ] {
            let matches = engine.check(text);
            assert!(
                !matches
                    .matches
                    .iter()
                    .any(|m| m.rule().id().contains("RELATIVE_CLAUSE")),
                "{text}: got {:?}",
                matches
                    .matches
                    .iter()
                    .map(|m| (m.rule().id().to_string(), m.offset()))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn java_battery_behind_matches_the_java_spans() {
        for (text, expected) in [
            (
                "Das Auto, das am Straßenrand steht parkt im Halteverbot.",
                "steht parkt",
            ),
            (
                "Das Auto, in dem der Mann sitzt parkt im Halteverbot.",
                "sitzt parkt",
            ),
            (
                "Die Frau, die vor dem Auto steht hat schwarze Haare.",
                "steht hat",
            ),
            ("Alles, was ich habe ist ein Buch.", "habe ist"),
            (
                "In diesem Prozess sind aber Entwicklungsschritte ja integriert, die wir Psychiater glaube ich auch gut kennen.",
                "Psychiater glaube ich",
            ),
            (
                "Alles, was du für die Spieße brauchst sind Tortellini, getrocknete Tomaten, Cherrytomaten und Mozzarellakugeln.",
                "brauchst sind",
            ),
        ] {
            let spans = rule_spans(text, "COMMA_BEHIND_RELATIVE_CLAUSE");
            assert_eq!(
                spans,
                vec![expected],
                "{text}: Java MissingCommaRelativeClauseRuleTest behind-mode spans"
            );
        }
    }

    #[test]
    fn java_battery_behind_accepts_the_clean_sentences() {
        for text in [
            ".... das war eher so das, was alle im Browser deaktiviert hatten, weil man dachte, über Javascript wird irgendwie Schadsoftware eingeschleust.",
            "Ich habe einige Fehler begangen, die ich vermeiden hätte können sollen.",
            "Doch die Rolle, die Mohammed bin Salman in Riad spielt, ist zwiespältig.",
            "Laut Josef Peter Burg gehörte das Haus, aus dem er samt Familie geworfen wurde, Werner und Sigrid Bahlke, während er darin wohnte und darauf aufpasste.",
            "Darüber hinaus stellt die klassische Metaphysik eine Grundfrage, die sich etwa wie folgt formulieren lässt: Warum ist überhaupt Seiendes und nicht vielmehr Nichts?",
            "Wenn du alles, was du meinst nicht zu können, von anderen erledigen lässt, wirst du es niemals selbst lernen.",
            "Er hat einen Zeitraum durchlebt, in dem seine Gedanken verträumt auf den weiten Feldern der Mysterien umherirrten.",
            "Es ist die Wiederkehr der Panikmache, die der neue Nationalismus mit dem der Sprachreiniger verbindet und die Geschichte der Sprachreinigung zu einem Lehrstück macht.",
            "Gesuche können von Institutionen, Organisationen, Vereinen und Gruppierungen gestellt werden, die im Kanton Luzern domiziliert sind.",
            "Die Klausel kann zudem nur Gleichrang mit Verbindlichkeiten des Schuldners herstellen, die vom Gesetz in der Insolvenz nicht privilegiert sind, so dass die Klausel nichts an der gesetzlich vorgesehenen Rangfolge im Insolvenzverfahren ändert.",
            "Plan von Maßnahmen, mit denen das Ansteckungsrisiko während des Aufenthalts an einem Ort verringert werden soll",
            "Aus diesem Grund sind die Wörter nicht direkt übersetzt, stattdessen wird der Zustand oder die Situation beschrieben in der die Wörter benutzt werden.",
            "Kryptographisch sichere Verfahren sind dann solche, für die es keine bessere Methode zum Brechen der Sicherheit als das Faktorisieren einer großen Zahl gibt, insbesondere kann der private nicht aus dem öffentlichen Schlüssel errechnet werden.",
            "Wayne Jancik begrenzt seine One-Hit-Wonders in den USA auf die Billboard-Top-40-Pop-Hitparade mit einer „Ruhe-Periode“ von 5 Jahren, innerhalb derer kein weiterer Hit desselben Interpreten in die Top 40 gelangen darf.",
            "Nach dem Abzug russischer Truppen aus der Region Kiew sind in den ehemals besetzten und umkämpften ukrainischen Gebieten inzwischen Hunderte Leichen von Bewohnern gefunden worden.",
            "Bei der Installation werden Ihnen Sticker und Aufkleber zur Verfügung gestellt, die Sie anstelle der traditionellen Emojis verwenden können.",
            "»Alles, was wir dank dieses Projektes sehen werden, wird für uns neu sein«, so der renommierte Bienenforscher.",
        ] {
            let spans = rule_spans(text, "COMMA_BEHIND_RELATIVE_CLAUSE");
            assert!(
                spans.is_empty(),
                "{text}: unexpected behind-mode match {spans:?}"
            );
        }
    }

    fn rule_spans<'a>(text: &'a str, rule_id: &str) -> Vec<&'a str> {
        use og_core::checker::CheckRequest;
        let language = shared_engine().language().clone();
        let mut request = CheckRequest::new(text, language);
        request.enabled_rules.push(rule_id.to_string());
        shared_engine()
            .check_with_request(&request)
            .matches
            .iter()
            .filter(|m| m.rule().id() == rule_id)
            .map(|m| &text[m.offset()..m.offset() + m.length()])
            .collect()
    }

    #[test]
    fn java_battery_in_front_matches_the_java_spans() {
        for (text, expected) in [
            (
                "Das Auto das am Straßenrand steht parkt im Halteverbot.",
                "Auto das",
            ),
            (
                "Das Auto das am Straßenrand steht, parkt im Halteverbot.",
                "Auto das",
            ),
            (
                "Das Auto in dem der Mann sitzt, parkt im Halteverbot.",
                "Auto in dem",
            ),
            (
                "Das Auto in dem der Mann sitzt parkt im Halteverbot.",
                "Auto in dem",
            ),
            (
                "Die Frau die vor dem Auto steht hat schwarze Haare.",
                "Frau die",
            ),
            (
                "Die Frau die vor dem Auto steht, hat schwarze Haare.",
                "Frau die",
            ),
            ("Alles was ich habe, ist ein Buch.", "Alles was"),
        ] {
            let spans = rule_spans(text, "COMMA_IN_FRONT_RELATIVE_CLAUSE");
            assert_eq!(
                spans,
                vec![expected],
                "{text}: Java MissingCommaRelativeClauseRuleTest flags the article plus relative pronoun"
            );
        }
    }

    #[test]
    fn java_battery_in_front_accepts_the_clean_sentences() {
        for text in [
            "Computer machen die Leute dumm.",
            "Die Unstimmigkeit zwischen den Geschichten der zwei Unfallbeteiligten war groß.",
            "Ebenso darf keine schwerere Strafe als die zum Zeitpunkt der Begehung der strafbaren Handlung angedrohte Strafe verhängt werden.",
            "Als dritte Gruppe lassen sich Aminosäuren fassen, die der Organismus anstelle dieser in Proteine einbaut.",
            "Selbst wenn das alles perfekt verlustfrei wäre, hätte ich nichts gewonnen.",
            "Die Studenten, deren Urteil am stärksten von dem der Profis abwich, waren sich sicher, einen guten von einem schlechten unterscheiden zu können.",
            "Die Studenten, deren Urteil am stärksten durch das der Profis beeinflusst wurde, waren sich sicher, einen guten von einem schlechten unterscheiden zu können.",
        ] {
            assert!(
                rule_spans(text, "COMMA_IN_FRONT_RELATIVE_CLAUSE").is_empty(),
                "{text}: Java MissingCommaRelativeClauseRuleTest accepts this sentence"
            );
        }
    }
}
