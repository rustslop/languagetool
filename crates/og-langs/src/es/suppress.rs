use crate::es::test_support::SpanishPipeline;
use og_core::AnalyzedTokenReadings;
use og_core::rule_match::{RuleMatch, SuggestedReplacement};
use og_xml::compiler::{CompiledPatternToken, CompiledRule};
use og_xml::types::{SuggestionPart, XmlMatch};

pub(crate) fn apply_suppress_misspelled(
    rule: &CompiledRule,
    tokens: &[&AnalyzedTokenReadings],
    rm: RuleMatch,
) -> Option<RuleMatch> {
    let flagged =
        rule.suppress_misspelled || rule.suggestions.iter().any(|s| s.suppress_misspelled);
    if !flagged || rule.pattern.has_structured_elements {
        return Some(rm);
    }
    let mut rm = rm;
    let mut junk: Vec<String> = Vec::new();
    if let Some(positions) = element_token_positions(rule, tokens, &rm) {
        for suggestion in &rule.suggestions {
            if !(suggestion.suppress_misspelled || rule.suppress_misspelled) {
                continue;
            }
            let [SuggestionPart::Match(m)] = suggestion.parts.as_slice() else {
                continue;
            };
            if m.postag.is_some() {
                continue;
            }
            let Some(&Some(pos)) = positions.get(m.no as usize - 1) else {
                continue;
            };
            let Some(token) = tokens.get(pos) else {
                continue;
            };
            if let Some(word) = raw_match_string(m, token)
                && unknown_to_spanish_tagger(&word)
            {
                junk.push(word);
            }
        }
    }
    if junk.is_empty() {
        return Some(rm);
    }
    let mut replacements: Vec<SuggestedReplacement> = rm.replacements().to_vec();
    for word in &junk {
        if let Some(idx) = replacements.iter().position(|r| r.value() == word.as_str()) {
            replacements.remove(idx);
        }
    }
    if rule.suppress_misspelled && replacements.is_empty() {
        return None;
    }
    rm.set_replacements(replacements);
    Some(rm)
}

fn element_token_positions(
    rule: &CompiledRule,
    tokens: &[&AnalyzedTokenReadings],
    rm: &RuleMatch,
) -> Option<Vec<Option<usize>>> {
    let end = rm.offset() + rm.length();
    let mut span = None;
    let mut last = 0usize;
    for (i, t) in tokens.iter().enumerate() {
        if t.token().start() < end && t.token().end() > rm.offset() {
            if span.is_none() {
                span = Some(i);
            }
            last = i;
        }
    }
    let first = span?;
    let mut cursor = first;
    let mut out: Vec<Option<usize>> = Vec::with_capacity(rule.pattern.tokens.len());
    for token in &rule.pattern.tokens {
        if token.skip != 0 || token.max.unwrap_or(1) != 1 {
            return None;
        }
        let satisfied = cursor <= last && token_satisfied(token, tokens[cursor]);
        if satisfied {
            out.push(Some(cursor));
            cursor += 1;
        } else if token.min == Some(0) {
            out.push(None);
        } else {
            return None;
        }
    }
    if cursor != last + 1 {
        return None;
    }
    Some(out)
}

fn token_satisfied(token: &CompiledPatternToken, atr: &AnalyzedTokenReadings) -> bool {
    let surface = atr.token().token();
    let text_ok = match &token.text {
        Some(text) => {
            let matched = if let Some(re) = &token.compiled_regexp_fancy {
                re.is_match(surface).unwrap_or(false)
            } else if token.case_sensitive {
                token
                    .compiled_regexp
                    .as_ref()
                    .is_some_and(|re| re.is_match(surface))
                    || surface == text
            } else {
                token
                    .compiled_regexp_ci
                    .as_ref()
                    .or(token.compiled_regexp.as_ref())
                    .is_some_and(|re| re.is_match(surface))
                    || surface.eq_ignore_ascii_case(text)
            };
            if token.negate { !matched } else { matched }
        }
        None => true,
    };
    let postag_ok = match &token.postag {
        Some(postag) => {
            let untagged = atr.readings().iter().all(|r| {
                r.pos_tags()
                    .iter()
                    .all(|t| t == "SENT_END" || t == "PARAGRAPH_END")
            });
            let unknown_ok = untagged
                && (if let Some(re) = &token.compiled_postag_regexp_fancy {
                    re.is_match("UNKNOWN").unwrap_or(false)
                } else if let Some(re) = &token.compiled_postag_regexp {
                    re.is_match("UNKNOWN")
                } else {
                    postag == "UNKNOWN"
                });
            let matched = unknown_ok
                || atr
                    .readings()
                    .iter()
                    .flat_map(|r| r.pos_tags().iter())
                    .any(|t| {
                        if let Some(re) = &token.compiled_postag_regexp_fancy {
                            re.is_match(t).unwrap_or(false)
                        } else if let Some(re) = &token.compiled_postag_regexp {
                            re.is_match(t)
                        } else {
                            t == postag
                        }
                    });
            if token.negate_pos { !matched } else { matched }
        }
        None => true,
    };
    text_ok && postag_ok
}

fn raw_match_string(m: &XmlMatch, token: &AnalyzedTokenReadings) -> Option<String> {
    let mut text = token.token().token().to_string();
    if let Some(match_re) = &m.regexp_match {
        let re = regex::Regex::new(match_re).ok()?;
        let repl = java_replacement(m.regexp_replace.as_deref().unwrap_or(""));
        text = re.replace_all(&text, repl.as_ref()).into_owned();
    } else {
        let repl = m.regexp_replace.as_deref()?;
        text = java_replacement(repl).into_owned();
    }
    if let Some(ref cc) = m.case_conversion {
        text = apply_case_conversion(&text, cc);
    }
    Some(text)
}

fn java_replacement(repl: &str) -> std::borrow::Cow<'_, str> {
    if !repl.contains('$') {
        return std::borrow::Cow::Borrowed(repl);
    }
    let mut out = String::with_capacity(repl.len() + 4);
    let mut chars = repl.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let mut digits = String::new();
        while let Some(&d) = chars.peek() {
            if d.is_ascii_digit() {
                digits.push(d);
                chars.next();
            } else {
                break;
            }
        }
        if digits.is_empty() {
            out.push('$');
            continue;
        }
        match chars.peek() {
            Some(&n) if n.is_alphanumeric() || n == '_' => {
                out.push_str("${");
                out.push_str(&digits);
                out.push('}');
            }
            _ => {
                out.push('$');
                out.push_str(&digits);
            }
        }
    }
    std::borrow::Cow::Owned(out)
}

fn apply_case_conversion(text: &str, mode: &str) -> String {
    match mode {
        "startlower" => {
            let mut chars = text.chars();
            match chars.next() {
                Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
        "startupper" => {
            let mut chars = text.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
        "alllower" => text.to_lowercase(),
        "allupper" => text.to_uppercase(),
        _ => text.to_string(),
    }
}

fn unknown_to_spanish_tagger(word: &str) -> bool {
    let tagged = og_tagger::Tagger::tag(SpanishPipeline::shared().tagger_ref(), &[word]);
    tagged.first().is_none_or(|atr| {
        atr.readings().is_empty() || atr.readings().iter().all(|r| r.pos_tags().is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::es::test_support::SpanishPipeline;
    use og_rules::pattern_rule::PatternRuleEngine;
    use og_xml::compiler::XmlCompiler;

    fn compiled_rules(id: &str) -> Vec<og_xml::compiler::CompiledRule> {
        let path = std::path::PathBuf::from(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/grammar.xml",
        );
        let xml = std::fs::read_to_string(&path).expect("grammar.xml");
        let rule_set = XmlCompiler::new()
            .compile_file_with_base(&xml, path.parent())
            .expect("compile grammar.xml");
        rule_set.rules.into_iter().filter(|r| r.id == id).collect()
    }

    fn surviving_matches(id: &str, text: &str) -> Vec<RuleMatch> {
        let pipeline = SpanishPipeline::shared();
        let engine = PatternRuleEngine::new();
        let mut out = Vec::new();
        for rule in compiled_rules(id) {
            for sentence in pipeline.analyze_text(text) {
                let tokens: Vec<&AnalyzedTokenReadings> = sentence.non_whitespace_tokens();
                for m in engine.match_rule(&rule, &sentence) {
                    if let Some(m) = apply_suppress_misspelled(&rule, &tokens, m) {
                        out.push(m);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn suppress_drops_no_separado_when_joined_word_is_unknown() {
        let matches = surviving_matches("NO_SEPARADO", "en la tonalidad de re bemol menor.");
        assert!(
            matches.is_empty(),
            "'rebemol' is unknown to the tagger so the whole match must be dropped, got {:?}",
            matches
                .iter()
                .map(|m| m.replacements().len())
                .collect::<Vec<_>>()
        );
        let matches = surviving_matches("NO_SEPARADO", "Cuenta con hablantes en todo el mundo.");
        assert!(
            matches.is_empty(),
            "'conhablantes' is unknown so 'Cuenta con hablantes' must not match"
        );
        let matches = surviving_matches(
            "NO_SEPARADO",
            "Quien caza dos liebres al mismo tiempo, no caza ninguna.",
        );
        assert!(matches.is_empty());
    }

    #[test]
    fn suppress_keeps_no_separado_when_joined_word_is_known() {
        let matches = surviving_matches("NO_SEPARADO", "Se re activa.");
        assert!(
            matches
                .iter()
                .any(|m| m.replacements().iter().any(|r| r.value() == "reactiva")),
            "'reactiva' is a real word so the match must survive, got {:?}",
            matches.len()
        );
    }

    #[test]
    fn suppress_drops_r_rr_on_unknown_accented_replacement() {
        let matches = surviving_matches("R_RR", "Autorízase el ingreso de agente.");
        assert!(matches.is_empty());
        let matches = surviving_matches("R_RR", "Se autoretrató.");
        assert!(
            matches
                .iter()
                .any(|m| m.replacements().iter().any(|r| r.value() == "autorretrató")),
            "'autorretrató' is known so the match must survive"
        );
    }

    #[test]
    fn unflagged_subrules_pass_matches_through_untouched() {
        let matches = surviving_matches("NO_SEPARADO", "Son social demócratas.");
        assert!(
            matches.iter().any(|m| m
                .replacements()
                .iter()
                .any(|r| r.value() == "socialdemócratas")),
            "the social subrule carries no suppress flag so its suggestion must survive: {:?}",
            matches.len()
        );
    }

    #[test]
    fn regexp_suggestion_survives_when_the_joined_word_is_known() {
        let matches = surviving_matches("NO_SEPARADO", "La tele visión.");
        assert!(
            matches
                .iter()
                .any(|m| m.replacements().iter().any(|r| r.value() == "televisión")),
            "'televisión' is a real word so the regexp-built suggestion must survive"
        );
    }

    #[test]
    fn regexp_suggestion_strips_unknown_joins_from_the_replacements() {
        let matches = surviving_matches("NO_SEPARADO", "El tele pate.");
        assert!(
            !matches.is_empty(),
            "the tele subrule must still flag 'tele pate'"
        );
        assert!(
            matches
                .iter()
                .all(|m| !m.replacements().iter().any(|r| r.value() == "telepate")),
            "'telepate' is unknown to the tagger so it must be stripped from every match"
        );
    }

    #[test]
    fn case_conversion_modes_follow_the_java_visitor() {
        assert_eq!(apply_case_conversion("Berlin", "startlower"), "berlin");
        assert_eq!(apply_case_conversion("", "startlower"), "");
        assert_eq!(apply_case_conversion("berlín", "startupper"), "Berlín");
        assert_eq!(apply_case_conversion("", "startupper"), "");
        assert_eq!(apply_case_conversion("SociAL", "alllower"), "social");
        assert_eq!(apply_case_conversion("demócrata", "allupper"), "DEMÓCRATA");
        assert_eq!(apply_case_conversion("Igual", "unknownmode"), "Igual");
    }

    #[test]
    fn java_replacement_quotes_group_refs_followed_by_name_chars() {
        assert_eq!(java_replacement("tele$1").as_ref(), "tele$1");
        assert_eq!(java_replacement("$1x").as_ref(), "${1}x");
        assert_eq!(java_replacement("$12b").as_ref(), "${12}b");
        assert_eq!(java_replacement("a$").as_ref(), "a$");
        assert_eq!(java_replacement("$").as_ref(), "$");
        assert_eq!(java_replacement("sinrefs").as_ref(), "sinrefs");
    }

    #[test]
    fn raw_match_string_applies_replace_and_case_conversion() {
        let token = AnalyzedTokenReadings::new(og_core::AnalyzedToken::new("trabajo", 0, 7));
        let m = |regexp_match: Option<&str>, regexp_replace: Option<&str>, cc: Option<&str>| {
            let mut m = XmlMatch::default();
            m.no = 3;
            m.regexp_match = regexp_match.map(str::to_string);
            m.regexp_replace = regexp_replace.map(str::to_string);
            m.case_conversion = cc.map(str::to_string);
            m
        };
        assert_eq!(
            raw_match_string(&m(Some("(.+)"), Some("tele$1"), None), &token).as_deref(),
            Some("teletrabajo")
        );
        assert_eq!(
            raw_match_string(&m(None, Some("tel"), None), &token).as_deref(),
            Some("tel")
        );
        assert_eq!(
            raw_match_string(&m(Some("(t.*)"), Some("$1"), Some("startupper")), &token).as_deref(),
            Some("Trabajo")
        );
        assert_eq!(
            raw_match_string(&m(None, None, Some("allupper")), &token),
            None,
            "without any regexp field there is nothing to build the word from"
        );
    }
}
