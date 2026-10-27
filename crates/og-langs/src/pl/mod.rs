use crate::en::CompoundRule;
use og_core::rule::{Rule, TextLevelRule};
use std::path::PathBuf;
use std::sync::Arc;

pub mod dash_rule;
pub mod filters;
pub mod simple_replace;
pub mod speller;
pub mod tagger;
pub mod unpaired_brackets;
pub mod word_repeat;
pub mod word_tokenizer;

#[cfg(test)]
pub(crate) mod example_suite;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
pub(crate) mod trace;

pub fn pl_rules_dir() -> Option<PathBuf> {
    crate::engine::find_lt_resource_dir()
        .map(|root| root.join("pl/src/main/resources/org/languagetool/rules/pl"))
}

pub fn normalize_pl_rules_xml(xml: &str) -> std::borrow::Cow<'_, str> {
    if !xml.contains("(?-iu)") && !xml.contains("\\0") && !needs_java_class_rewrites(xml) {
        return std::borrow::Cow::Borrowed(xml);
    }
    let negated = negate_inflected_tokens_to_exceptions(xml);
    let xml: &str = negated.as_deref().unwrap_or(xml);
    let mut class_fixed = rewrite_java_regex_classes(xml);
    if let Some(widened) =
        widen_engine_normalized_quote_classes(class_fixed.as_deref().unwrap_or(xml))
    {
        class_fixed = Some(widened);
    }
    if let Some(stripped) =
        strip_whitespace_only_exception_bodies(class_fixed.as_deref().unwrap_or(xml))
    {
        class_fixed = Some(stripped);
    }
    let xml: &str = class_fixed.as_deref().unwrap_or(xml);
    let bytes = xml.as_bytes();
    let mut out = String::with_capacity(xml.len() + 96);
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'(' && xml[i..].starts_with("(?-iu)") {
            out.push_str("(?-i)");
            i += 6;
        } else if b == b'\\'
            && bytes.get(i + 1) == Some(&b'0')
            && is_java_octal_digit(bytes.get(i + 2))
        {
            let mut value = (bytes[i + 2] - b'0') as u32;
            let mut digits = 1usize;
            while digits < 3 && is_java_octal_digit(bytes.get(i + 2 + digits)) {
                let candidate = value * 8 + (bytes[i + 2 + digits] - b'0') as u32;
                if candidate > 0o377 {
                    break;
                }
                value = candidate;
                digits += 1;
            }
            out.push_str(&format!("\\x{{{value:x}}}"));
            i += 2 + digits;
        } else {
            let mut step = xml[i..].chars().next().map_or(1, char::len_utf8);
            if b == b'\\' && bytes.get(i + 1).is_some() {
                step = 1 + xml[i + 1..].chars().next().map_or(1, char::len_utf8);
            }
            out.push_str(&xml[i..i + step]);
            i += step;
        }
    }
    std::borrow::Cow::Owned(out)
}

fn is_java_octal_digit(d: Option<&u8>) -> bool {
    matches!(d, Some(d) if d.is_ascii_digit() && *d <= b'7')
}

const JAVA_ASCII_PUNCT_CLASS: &str = "!-/:-@\\[-`{-~";

fn needs_java_class_rewrites(xml: &str) -> bool {
    xml.contains("\\p{Punct}") || xml.contains("\\p{Ll}") || xml.contains("\\p{Lu}")
}

fn strip_whitespace_only_exception_bodies(xml: &str) -> Option<String> {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    let mut changed = false;
    while let Some(open) = rest.find("<exception") {
        out.push_str(&rest[..open]);
        let tail = &rest[open..];
        let Some(gt) = tail.find('>') else {
            out.push_str(tail);
            rest = "";
            break;
        };
        let gt_abs = open + gt;
        let body_start = gt_abs + 1;
        if rest[..gt_abs].ends_with('/') {
            out.push_str(&rest[open..body_start]);
            rest = &rest[body_start..];
            continue;
        }
        let Some(close) = rest[body_start..].find("</exception>") else {
            out.push_str(tail);
            rest = "";
            break;
        };
        let close = body_start + close;
        let close_end = close + "</exception>".len();
        if rest[body_start..close].trim().is_empty() {
            out.push_str(&rest[open..body_start]);
            out.push_str("</exception>");
            changed = true;
        } else {
            out.push_str(&rest[open..close_end]);
        }
        rest = &rest[close_end..];
    }
    out.push_str(rest);
    changed.then_some(out)
}

fn widen_engine_normalized_quote_classes(xml: &str) -> Option<String> {
    let widened = xml
        .replace("[\u{2018}\u{201e}]", "['\u{201e}]")
        .replace("[\u{201d}\u{2019}]", "[\u{201d}']");
    (widened != xml).then_some(widened)
}

fn negate_inflected_tokens_to_exceptions(xml: &str) -> Option<String> {
    let mut out = xml.replace(
        "<token inflected=\"yes\" negate=\"yes\" regexp=\"yes\">uzna\u{107}|uznawa\u{107}|uwa\u{17c}a\u{107}</token>",
        "<token><exception inflected=\"yes\" regexp=\"yes\">uzna\u{107}|uznawa\u{107}|uwa\u{17c}a\u{107}</exception></token>",
    );
    out = out.replace(
        "<token postag=\"UNKNOWN\" negate_pos=\"yes\" inflected=\"yes\" regexp=\"yes\" negate=\"yes\">",
        "<token postag=\"UNKNOWN\" negate_pos=\"yes\"><exception inflected=\"yes\" regexp=\"yes\">",
    );
    if out != xml {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(at) = rest.find("<token postag=\"UNKNOWN\" negate_pos=\"yes\"><exception") {
            let split = at + "<token postag=\"UNKNOWN\" negate_pos=\"yes\"><exception".len();
            fixed.push_str(&rest[..split]);
            rest = &rest[split..];
            let Some(tag_end) = rest.find("</token>") else {
                break;
            };
            fixed.push_str(&rest[..tag_end]);
            fixed.push_str("</exception></token>");
            rest = &rest[tag_end + "</token>".len()..];
        }
        fixed.push_str(rest);
        return Some(fixed);
    }
    None
}

fn rewrite_java_regex_classes(xml: &str) -> Option<String> {
    if !needs_java_class_rewrites(xml) {
        return None;
    }
    let mut out = String::with_capacity(xml.len() + 512);
    let mut rest = xml;
    let mut class_depth: u32 = 0;
    while let Some(at) = rest.find(['\\', '[', ']', '<', '>']) {
        let (sep, tail) = rest.split_at(at);
        out.push_str(sep);
        let first = tail.chars().next().expect("non-empty tail");
        let mut advance = first.len_utf8();
        match first {
            '[' => {
                class_depth = class_depth.saturating_add(1);
                out.push('[');
            }
            ']' => {
                class_depth = class_depth.saturating_sub(1);
                out.push(']');
            }
            '<' | '>' => {
                class_depth = 0;
                out.push(first);
            }
            _ => {
                if tail.starts_with("\\p{Punct}") {
                    if class_depth > 0 {
                        out.push_str(JAVA_ASCII_PUNCT_CLASS);
                    } else {
                        out.push('[');
                        out.push_str(JAVA_ASCII_PUNCT_CLASS);
                        out.push(']');
                    }
                    advance = 9;
                } else if tail.starts_with("\\p{Ll}") && class_depth == 0 {
                    out.push_str("(?-i:\\p{Ll})");
                    advance = 6;
                } else if tail.starts_with("\\p{Lu}") && class_depth == 0 {
                    out.push_str("(?-i:\\p{Lu})");
                    advance = 6;
                } else {
                    advance = first.len_utf8()
                        + tail[first.len_utf8()..]
                            .chars()
                            .next()
                            .map_or(0, char::len_utf8);
                    out.push_str(&tail[..advance]);
                }
            }
        }
        rest = &tail[advance..];
    }
    out.push_str(rest);
    Some(out)
}

fn append_sent_end_to_filter_postags(xml: &str) -> Option<String> {
    // Java re-adds SENT_END on filtered sentence-final tokens (MatchState.java:151-156);
    // letting the filter postag keep that reading reproduces it without engine changes.
    const NEEDLE: &str = "action=\"filter\" postag=\"";
    if !xml.contains(NEEDLE) {
        return None;
    }
    let mut out = String::with_capacity(xml.len() + 160);
    let mut rest = xml;
    while let Some(at) = rest.find(NEEDLE) {
        let split = at + NEEDLE.len();
        out.push_str(&rest[..split]);
        rest = &rest[split..];
        let Some(close) = rest.find('"') else {
            break;
        };
        out.push_str(&rest[..close]);
        out.push_str("|SENT_END");
        rest = &rest[close..];
    }
    out.push_str(rest);
    Some(out)
}

fn normalize_disambiguation_xml(xml: &str) -> String {
    // Java quirks the shared crates cannot express: the bare dash is a literal class member,
    // and AnalyzedToken.matches treats a remove <wd> null pos as an any-pos wildcard.
    let escaped = xml.replace(
        "[\\p{Punct}-\u{2212}\u{2013}\u{2014}\u{2015}\u{201e}\u{00ab}\u{00bb}\u{2026}\u{201d}&amp;&amp;[^!\\.]",
        "[\\p{Punct}\\-\u{2212}\u{2013}\u{2014}\u{2015}\u{201e}\u{00ab}\u{00bb}\u{2026}\u{201d}&amp;&amp;[^!\\.]",
    );
    let and_form = escaped.replace(
        "<token><exception negate_pos=\"yes\" postag=\"SENT_END\">.</exception></token>",
        "<and><token>.</token><token postag=\"SENT_END\"/></and>",
    );
    let wd_form = and_form.replace(
        "<disambig postag=\"replace\">",
        "<disambig action=\"replace\">",
    );
    let filtered = append_sent_end_to_filter_postags(&wd_form).unwrap_or(wd_form);
    let class_fixed = rewrite_java_regex_classes(&filtered).unwrap_or(filtered);
    let maja_fixed = maja_maic_match_filter_to_remove(&class_fixed);
    let expanded = expand_optional_unify_ignore_tokens(&maja_fixed);
    wildcard_lemma_only_remove_wds(&expanded)
}

fn maja_maic_match_filter_to_remove(xml: &str) -> String {
    let needle = "<disambig action=\"filter\"><match no=\"1\">mie\u{107}</match></disambig>";
    let Some(pos) = xml.find(needle) else {
        return xml.to_string();
    };
    let Some(rule_start) = xml[..pos].rfind("<rule ") else {
        return xml.to_string();
    };
    let Some(end_rel) = xml[rule_start..].find("</rule>") else {
        return xml.to_string();
    };
    let rule_end = rule_start + end_rel + "</rule>".len();
    let rule_text = &xml[rule_start..rule_end];
    let first = rule_text.replace(
        needle,
        "<disambig action=\"remove\"><wd lemma=\"mai\u{107}\" pos=\".*\"/></disambig>",
    );
    let second = rule_text.replace(
        needle,
        "<disambig action=\"remove\"><wd lemma=\"maja\" pos=\".*\"/></disambig>",
    );
    let mut out = String::with_capacity(xml.len() + rule_text.len() + 16);
    out.push_str(&xml[..rule_start]);
    out.push_str(&first);
    out.push('\n');
    out.push_str(&second);
    out.push_str(&xml[rule_end..]);
    out
}

fn next_rule_open(xml: &str) -> Option<usize> {
    let with_attrs = xml.find("<rule ");
    let bare = xml.find("<rule>");
    match (with_attrs, bare) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn has_optional_unify_ignore_token(rule: &str) -> bool {
    let mut scan = 0;
    while let Some(rel) = rule[scan..].find("<unify-ignore>") {
        let body_start = scan + rel + "<unify-ignore>".len();
        let Some(end_rel) = rule[body_start..].find("</unify-ignore>") else {
            return false;
        };
        let body = &rule[body_start..body_start + end_rel];
        if body.contains("min=\"0\"")
            && body.contains("<token min=\"0\" max=\"1\">,</token>")
            && !body.contains("<marker>")
        {
            return true;
        }
        scan = body_start + end_rel;
    }
    false
}

fn strip_optional_unify_ignore_tokens(rule: &str) -> String {
    let mut out = String::with_capacity(rule.len());
    let mut scan = 0;
    while let Some(rel) = rule[scan..].find("<unify-ignore>") {
        let open_start = scan + rel;
        let open_end = open_start + "<unify-ignore>".len();
        let Some(end_rel) = rule[open_end..].find("</unify-ignore>") else {
            out.push_str(&rule[scan..]);
            return out;
        };
        let close_start = open_end + end_rel;
        out.push_str(&rule[scan..open_end]);
        let body = &rule[open_end..close_start];
        let mut body_scan = 0;
        while let Some(token_rel) = body[body_scan..].find("<token") {
            let token_start = body_scan + token_rel;
            let Some(open_tag_end_rel) = body[token_start..].find('>') else {
                break;
            };
            let open_tag_end = token_start + open_tag_end_rel + 1;
            let attrs = &body[token_start..open_tag_end];
            let self_closing = attrs.ends_with("/>");
            if !attrs.contains("min=\"0\"") {
                out.push_str(&body[body_scan..open_tag_end]);
                body_scan = open_tag_end;
                continue;
            }
            out.push_str(&body[body_scan..token_start]);
            if self_closing {
                body_scan = open_tag_end;
            } else {
                let Some(close_rel) = body[open_tag_end..].find("</token>") else {
                    body_scan = open_tag_end;
                    break;
                };
                body_scan = open_tag_end + close_rel + "</token>".len();
            }
        }
        out.push_str(&body[body_scan..]);
        out.push_str("</unify-ignore>");
        scan = close_start + "</unify-ignore>".len();
    }
    out.push_str(&rule[scan..]);
    out
}

fn expand_optional_unify_ignore_tokens(xml: &str) -> String {
    let mut comments: Vec<(usize, usize)> = Vec::new();
    let mut comment_scan = 0;
    while let Some(rel) = xml[comment_scan..].find("<!--") {
        let start = comment_scan + rel;
        let Some(end_rel) = xml[start..].find("-->") else {
            break;
        };
        let end = start + end_rel + "-->".len();
        comments.push((start, end));
        comment_scan = end;
    }
    let mut out = String::with_capacity(xml.len() + 16384);
    let mut pos = 0;
    while let Some(rel) = next_rule_open(&xml[pos..]) {
        let rule_start = pos + rel;
        out.push_str(&xml[pos..rule_start]);
        let Some(end_rel) = xml[rule_start..].find("</rule>") else {
            out.push_str(&xml[rule_start..]);
            return out;
        };
        let rule_end = rule_start + end_rel + "</rule>".len();
        let rule_text = &xml[rule_start..rule_end];
        let commented = comments
            .iter()
            .any(|(s, e)| *s <= rule_start && rule_start < *e);
        if commented || !has_optional_unify_ignore_token(rule_text) {
            out.push_str(rule_text);
        } else {
            out.push_str(rule_text);
            out.push('\n');
            out.push_str(&strip_optional_unify_ignore_tokens(rule_text));
        }
        pos = rule_end;
    }
    out.push_str(&xml[pos..]);
    out
}

fn wildcard_lemma_only_remove_wds(xml: &str) -> String {
    const OPEN: &str = "action=\"remove\"><wd ";
    let mut out = String::with_capacity(xml.len() + 460);
    let mut rest = xml;
    while let Some(found) = rest.find(OPEN) {
        let split = found + OPEN.len();
        out.push_str(&rest[..split]);
        rest = &rest[split..];
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let attrs = &rest[..tag_end];
        let self_closing = attrs.ends_with('/');
        let attr_body = if self_closing {
            &attrs[..attrs.len() - 1]
        } else {
            attrs
        };
        if attr_body.contains("lemma=\"") && !attr_body.contains("pos=\"") {
            out.push_str(attr_body);
            out.push_str(" pos=\".*\"");
            if self_closing {
                out.push('/');
            }
            out.push('>');
        } else {
            out.push_str(attrs);
            out.push('>');
        }
        rest = &rest[tag_end + 1..];
    }
    out.push_str(rest);
    out
}

struct SentEndRestoringDisambiguator {
    inner: Arc<dyn og_core::checker::Disambiguator>,
}

impl og_core::checker::Disambiguator for SentEndRestoringDisambiguator {
    fn disambiguate(&self, sentence: &mut og_core::AnalyzedSentence) {
        self.inner.disambiguate(sentence);
        for token in sentence.tokens_mut().iter_mut().rev() {
            if !token.is_whitespace()
                && !token.token().pos_tags().contains(&"SENT_START".to_string())
            {
                if !token.has_pos_tag("SENT_END") {
                    token.add_sent_end();
                }
                break;
            }
        }
    }
}

pub fn build_pl_disambiguator(
    resource_dir: &std::path::Path,
) -> Option<Arc<dyn og_core::checker::Disambiguator>> {
    let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).ok()?;
    let normalized = normalize_disambiguation_xml(&xml);
    let disambiguator =
        og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None).ok()?;
    let core = crate::engine::find_lt_core_resource_dir()?;
    Some(Arc::new(SentEndRestoringDisambiguator {
        inner: og_tagger::hybrid_disambiguator("pl", resource_dir, &core, disambiguator),
    }))
}

pub fn polish_compound_rule() -> CompoundRule {
    CompoundRule::for_language(
        "PL_COMPOUNDS",
        "Sprawdza wyrazy z łącznikiem, np. „łapu capu” zamiast „łapu-capu”",
        "Ten wyraz pisze się z łącznikiem.",
        "Ten wyraz pisze się razem (bez spacji ani łącznika).",
        "Ten wyraz pisze się z łącznikiem lub bez niego.",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pl/src/main/resources/org/languagetool/resource/pl/compounds.txt"
        ),
    )
}

pub fn polish_word_coherency_rule() -> crate::en::WordCoherencyRule {
    crate::en::WordCoherencyRule::for_language(
        "PL_WORD_COHERENCY",
        "Jednolita pisownia wyrazów o obocznej dopuszczalnej pisowni",
        include_str!(
            "../../../../languagetool/languagetool-language-modules/pl/src/main/resources/org/languagetool/rules/pl/coherency.txt"
        ),
    )
    .with_message_template(
        "Formy „{word1}” i „{word2}” zwykle nie powinny być używane jednocześnie.",
    )
}

pub fn pl_unpaired_brackets_rule() -> unpaired_brackets::PolishUnpairedBracketsRule {
    unpaired_brackets::PolishUnpairedBracketsRule::new()
}

pub struct PolishNativeRules {
    pub rules: Vec<std::sync::Arc<dyn Rule>>,
    pub text_level_rules: Vec<std::sync::Arc<dyn TextLevelRule>>,
}

pub fn polish_native_rules() -> PolishNativeRules {
    PolishNativeRules {
        rules: vec![
            std::sync::Arc::new(og_rules::native_rules::CommaWhitespaceRule::new()),
            std::sync::Arc::new(og_rules::native_rules::UppercaseSentenceStartRule::new()),
            std::sync::Arc::new(og_rules::native_rules::WordRepeatRule::new()),
            std::sync::Arc::new(speller::PolishSpellerRule::new()),
            std::sync::Arc::new(word_repeat::PolishWordRepeatRule::new()),
            std::sync::Arc::new(polish_compound_rule()),
            std::sync::Arc::new(simple_replace::PolishSimpleReplaceRule::new()),
            std::sync::Arc::new(dash_rule::PolishDashRule::new()),
        ],
        text_level_rules: vec![
            std::sync::Arc::new(og_rules::text_level_rules::MultipleWhitespaceRule::new()),
            std::sync::Arc::new(og_rules::text_level_rules::SentenceWhitespaceRule::new()),
            std::sync::Arc::new(pl_unpaired_brackets_rule()),
            std::sync::Arc::new(polish_word_coherency_rule()),
        ],
    }
}

pub fn priority_for_id(id: &str) -> i32 {
    if id == "ZDANIA_ZLOZONE" {
        return -1;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_rules_carry_java_ids() {
        let native = polish_native_rules();
        let ids: Vec<&str> = native.rules.iter().map(|r| r.id()).collect();
        for expected in [
            "COMMA_PARENTHESIS_WHITESPACE",
            "UPPERCASE_SENTENCE_START",
            "WORD_REPEAT_RULE",
            "MORFOLOGIK_RULE_PL_PL",
            "PL_WORD_REPEAT",
            "PL_COMPOUNDS",
            "PL_SIMPLE_REPLACE",
            "PL_DASH_RULE",
        ] {
            assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
        }
        let text_ids: Vec<&str> = native.text_level_rules.iter().map(|r| r.id()).collect();
        for expected in [
            "WHITESPACE_RULE",
            "SENTENCE_WHITESPACE",
            "PL_UNPAIRED_BRACKETS",
            "PL_WORD_COHERENCY",
        ] {
            assert!(
                text_ids.contains(&expected),
                "missing {expected} in {text_ids:?}"
            );
        }
    }

    #[test]
    fn normalize_disambiguation_xml_gives_lemma_only_remove_wds_the_java_wildcard_pos() {
        let xml = std::fs::read_to_string(
            crate::pl::tagger::pl_resource_dir()
                .expect("pl resource dir")
                .join("disambiguation.xml"),
        )
        .expect("disambiguation.xml");
        let normalized = normalize_disambiguation_xml(&xml);
        let self_closing = regex::Regex::new("action=\"remove\"><wd lemma=\"[^\"]*\"/>").unwrap();
        let empty_text = regex::Regex::new("action=\"remove\"><wd lemma=\"[^\"]*\"></wd>").unwrap();
        assert_eq!(self_closing.find_iter(&xml).count(), 88);
        assert_eq!(empty_text.find_iter(&xml).count(), 4);
        assert_eq!(self_closing.find_iter(&normalized).count(), 0);
        assert_eq!(empty_text.find_iter(&normalized).count(), 0);
        assert_eq!(xml.matches(" pos=\".*\"").count(), 0);
        assert_eq!(
            normalized.matches(" pos=\".*\"").count(),
            94,
            "92 wildcard rewrites plus the 2 explicit pos wds of the MAJA_MAIC bridge"
        );
        assert_eq!(
            normalized.matches("<wd ").count(),
            xml.matches("<wd ").count() + 2,
            "only the MAJA_MAIC bridge may add wd elements"
        );
        assert!(normalized.contains("<wd lemma=\"ko\u{142}o\" pos=\".*\"/>"));
        assert!(normalized.contains("<wd lemma=\"mi\" pos=\".*\"></wd>"));
        assert_eq!(
            normalized
                .matches("<wd lemma=\"zdarzy\u{107}\" pos=\"refl\">")
                .count(),
            2,
            "wds that already carry pos stay untouched"
        );
    }

    #[test]
    fn normalize_disambiguation_xml_expands_optional_unify_ignore_rules() {
        let xml = std::fs::read_to_string(
            crate::pl::tagger::pl_resource_dir()
                .expect("pl resource dir")
                .join("disambiguation.xml"),
        )
        .expect("disambiguation.xml");
        let normalized = normalize_disambiguation_xml(&xml);
        let rule_count = |s: &str| s.matches("<rule ").count() + s.matches("<rule>").count();
        assert_eq!(
            rule_count(&normalized) - rule_count(&xml),
            3,
            "the two comma-optional badz_koordynacja rules gain duplicates plus the split MAJA_MAIC"
        );
        assert_eq!(xml.matches("id=\"badz_koordynacja_2\"").count(), 1);
        assert_eq!(normalized.matches("id=\"badz_koordynacja_2\"").count(), 2);
        let first_id = normalized.find("id=\"badz_koordynacja_2\"").expect("first");
        let original_start = normalized[..first_id]
            .rfind("<rule ")
            .expect("original start");
        let original_end =
            normalized[original_start..].find("</rule>").expect("oend") + original_start;
        assert!(
            normalized[original_start..original_end].contains("min=\"0\""),
            "the original optional-comma rule stays"
        );
        let variant_start =
            normalized[original_end..].find("<rule ").expect("variant") + original_end;
        let variant_end =
            normalized[variant_start..].find("</rule>").expect("vend") + variant_start;
        assert!(
            !normalized[variant_start..variant_end].contains("min=\"0\""),
            "the duplicate drops the optional token"
        );
    }

    #[test]
    fn normalize_disambiguation_xml_rewrites_maja_maic_match_filter() {
        let xml = std::fs::read_to_string(
            crate::pl::tagger::pl_resource_dir()
                .expect("pl resource dir")
                .join("disambiguation.xml"),
        )
        .expect("disambiguation.xml");
        assert_eq!(
            xml.matches(
                "<disambig action=\"filter\"><match no=\"1\">mie\u{107}</match></disambig>"
            )
            .count(),
            1
        );
        let normalized = normalize_disambiguation_xml(&xml);
        assert_eq!(
            normalized
                .matches("<disambig action=\"filter\"><match no=\"1\">")
                .count(),
            0
        );
        assert!(normalized.contains("<wd lemma=\"mai\u{107}\" pos=\".*\"/>"));
        assert!(normalized.contains("<wd lemma=\"maja\" pos=\".*\"/>"));
    }

    #[test]
    fn pipeline_pins_badz_conj_and_maja_miec_like_java() {
        let pipeline = crate::pl::test_support::PolishPipeline::shared();
        let badz_text =
            "Dojad\u{119} tam b\u{105}d\u{17a} rowerem b\u{105}d\u{17a} hulajnog\u{105}.";
        let badz_sentence = pipeline.analyze_range(badz_text, 0, badz_text.len());
        let badz_readings: Vec<Vec<String>> = badz_sentence
            .non_whitespace_tokens()
            .into_iter()
            .filter(|t| t.token().token() == "b\u{105}d\u{17a}")
            .map(|t| {
                t.readings()
                    .iter()
                    .map(|r| r.pos_tags().join("|"))
                    .collect()
            })
            .collect();
        assert_eq!(
            badz_readings,
            vec![vec!["conj".to_string()], vec!["conj".to_string()]],
            "badz_koordynacja_2's own example pins the first bądź to conj, which also keeps \\
             TAM_JEST from stripping it"
        );
        let maja_text =
            "Beztlenowe bakterie magnetotaktyczna maj\u{105} funkcj\u{119} wykrywania tlenu.";
        let maja_sentence = pipeline.analyze_range(maja_text, 0, maja_text.len());
        let maja_lemmas: Vec<String> = maja_sentence
            .non_whitespace_tokens()
            .into_iter()
            .find(|t| t.token().token() == "maj\u{105}")
            .expect("mają token")
            .readings()
            .iter()
            .filter_map(|r| r.lemma().map(str::to_string))
            .collect();
        assert_eq!(
            maja_lemmas,
            vec!["mie\u{107}".to_string()],
            "MAJA_MAIC's outputform pins mają to the mieć lemma only"
        );
    }

    #[test]
    fn grammar_rules_match_the_two_recovered_examples() {
        let engine = crate::pl::example_suite::suite_engine();
        let pipeline = crate::pl::test_support::PolishPipeline::shared();
        for (rule_id, text) in [
            (
                "BRAK_PRZECINKA_ANI",
                "Dojad\u{119} tam b\u{105}d\u{17a} rowerem b\u{105}d\u{17a} hulajnog\u{105}.",
            ),
            (
                "ADJ_SUBST_ADJ_UNIFY",
                "Beztlenowe bakterie magnetotaktyczna maj\u{105} funkcj\u{119} wykrywania tlenu.",
            ),
        ] {
            let mut matches = 0usize;
            for rule in crate::pl::trace::compiled_rules()
                .rules
                .iter()
                .filter(|r| r.id == rule_id)
            {
                for (sentence, _) in pipeline.analyze_offset_text(text) {
                    matches += engine.match_rule(rule, &sentence).len();
                }
            }
            assert!(
                matches > 0,
                "{rule_id} must match its incorrect example after the disambiguation bridges"
            );
        }
    }

    #[test]
    fn o_wym_disambiguation_fires_on_sentence_final_dot_only() {
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).expect("xml");
        let exception_shape =
            "<token><exception negate_pos=\"yes\" postag=\"SENT_END\">.</exception></token>";
        assert_eq!(xml.matches(exception_shape).count(), 1);
        let normalized = normalize_disambiguation_xml(&xml);
        assert_eq!(
            normalized
                .matches("<and><token>.</token><token postag=\"SENT_END\"/></and>")
                .count(),
            1
        );
        let parsed =
            og_tagger::XmlDisambiguator::from_xml_with_english(&normalized, None).expect("parse");
        let idx = parsed
            .rule_ids()
            .iter()
            .position(|id| *id == "O_WYM_")
            .expect("O_WYM_ rule");
        let brev_stays = |sentence: &og_core::AnalyzedSentence| -> bool {
            sentence
                .non_whitespace_tokens()
                .iter()
                .find(|t| t.token().token() == "wym")
                .expect("wym token")
                .readings()
                .iter()
                .any(|r| r.pos_tags().iter().any(|p| p == "brev:pun"))
        };
        let dotted = "Rozmawiali o tym i o wym.";
        let mut sentence =
            crate::pl::test_support::analyze_undisambiguated(dotted, 0, dotted.len());
        parsed.apply_rule(&mut sentence, idx);
        assert!(
            !brev_stays(&sentence),
            "the sentence-final dot lets O_WYM_ drop the brev reading"
        );
        let mid = "S\u{142}owo o wym. \u{142}an.";
        let mut sentence = crate::pl::test_support::analyze_undisambiguated(mid, 0, mid.len());
        parsed.apply_rule(&mut sentence, idx);
        assert!(
            brev_stays(&sentence),
            "the mid-sentence dot keeps the abbreviation reading"
        );
    }

    #[test]
    fn java_pins_rule_k_removes_every_kolo_reading_of_the_k_token() {
        let resource_dir = crate::pl::tagger::pl_resource_dir().expect("pl resource dir");
        let xml = std::fs::read_to_string(resource_dir.join("disambiguation.xml")).expect("xml");
        let parsed = og_tagger::XmlDisambiguator::from_xml_with_english(
            &normalize_disambiguation_xml(&xml),
            None,
        )
        .expect("parse disambiguation.xml");
        let idx = parsed
            .rule_ids()
            .iter()
            .position(|id| *id == "K")
            .expect("K rule");
        let text = "Stopie\u{144} dysocjacji w temperaturze 4000 K wynosi nieca\u{142}e 3%.";
        let mut sentence = crate::pl::test_support::analyze_undisambiguated(text, 0, text.len());
        let k_lemmas = |sentence: &og_core::AnalyzedSentence| -> Vec<Option<String>> {
            sentence
                .non_whitespace_tokens()
                .iter()
                .find(|t| t.token().token() == "K")
                .expect("K token")
                .readings()
                .iter()
                .map(|r| r.lemma().map(str::to_string))
                .collect()
        };
        let before = k_lemmas(&sentence);
        assert!(
            before.iter().any(|l| l.as_deref() == Some("ko\u{142}o")),
            "the tagger offers ko\u{142}o readings for K: {before:?}"
        );
        parsed.apply_rule(&mut sentence, idx);
        let after = k_lemmas(&sentence);
        assert!(
            after.iter().all(|l| l.as_deref() != Some("ko\u{142}o")),
            "AnalyzedToken.matches (AnalyzedToken.java:91-111) treats a null pos as a \
             wildcard, so the lemma-only remove must drop every ko\u{142}o reading: {after:?}"
        );
        assert!(
            !after.is_empty(),
            "Java falls back to a bare reading when all readings are removed"
        );
    }

    #[test]
    fn java_pins_priority_map() {
        assert_eq!(priority_for_id("ZDANIA_ZLOZONE"), -1);
        assert_eq!(priority_for_id("TOO_LONG_SENTENCE"), -101);
        assert_eq!(priority_for_id("too_long_sentence"), -101);
        assert_eq!(priority_for_id("REPETITIONS_STYLE"), -55);
        assert_eq!(priority_for_id("SOMETHING_STYLE_CAT"), -50);
        assert_eq!(priority_for_id("UNKNOWN_RULE"), 0);
    }

    #[test]
    fn whitespace_only_exception_bodies_stripped_without_tag_loss() {
        let xml =
            std::fs::read_to_string(pl_rules_dir().expect("pl rules dir").join("grammar.xml"))
                .expect("grammar.xml");
        let stripped =
            strip_whitespace_only_exception_bodies(&xml).expect("one body in grammar.xml");
        assert_eq!(
            xml.matches("<exception").count(),
            stripped.matches("<exception").count()
        );
        assert_eq!(
            xml.matches("</exception>").count(),
            stripped.matches("</exception>").count()
        );
        assert_eq!(
            xml.matches("<rule").count(),
            stripped.matches("<rule").count()
        );
        assert_eq!(
            xml.matches("<exception postag=\".*loc.*|conj|comp|adv.*|num.*\" postag_regexp=\"yes\">\n                                                    </exception>")
                .count(),
            1
        );
        assert_eq!(
            stripped
                .matches("<exception postag=\".*loc.*|conj|comp|adv.*|num.*\" postag_regexp=\"yes\"></exception>")
                .count(),
            1
        );
        assert!(stripped.len() < xml.len());
    }

    #[test]
    fn negated_inflected_tokens_become_java_equivalent_exceptions() {
        let xml =
            std::fs::read_to_string(pl_rules_dir().expect("pl rules dir").join("grammar.xml"))
                .expect("grammar.xml");
        let rewritten =
            negate_inflected_tokens_to_exceptions(&xml).expect("both shapes in grammar.xml");
        assert_eq!(
            rewritten
                .matches("<token><exception inflected=\"yes\" regexp=\"yes\">uzna")
                .count(),
            2
        );
        assert_eq!(
            rewritten
                .matches("<token postag=\"UNKNOWN\" negate_pos=\"yes\"><exception inflected=\"yes\" regexp=\"yes\">")
                .count(),
            1
        );
        assert!(!rewritten.contains("negate=\"yes\" regexp=\"yes\">uzna"));
    }

    #[test]
    fn filter_postags_keep_the_sent_end_reading() {
        let xml = std::fs::read_to_string(
            crate::pl::tagger::pl_resource_dir()
                .expect("pl resource dir")
                .join("disambiguation.xml"),
        )
        .expect("disambiguation.xml");
        let bridged =
            append_sent_end_to_filter_postags(&xml).expect("filters exist in disambiguation.xml");
        let filter_shapes =
            regex::Regex::new("action=\"filter\" postag=\"[^\"']*[|]SENT_END\"").unwrap();
        assert_eq!(filter_shapes.find_iter(&bridged).count(), 144);
        assert_eq!(filter_shapes.find_iter(&xml).count(), 0);
    }

    #[test]
    fn normalize_pl_rules_xml_makes_posix_punct_ascii_and_case_classes_case_exact() {
        let xml =
            std::fs::read_to_string(pl_rules_dir().expect("pl rules dir").join("grammar.xml"))
                .expect("grammar.xml");
        let normalized = normalize_pl_rules_xml(&xml);
        assert_eq!(xml.matches("\\p{Punct}").count(), 7);
        assert_eq!(normalized.matches("\\p{Punct}").count(), 0);
        assert!(normalized.contains("[!-/:-@\\[-`{-~]"));
        assert!(normalized.contains(
            "[!-/:-@\\[-`{-~\\-\u{2013}\u{2212}\u{2014}\u{2015}\u{201e}\u{00ab}\u{00bb}\u{2026}\u{2019}&amp;&amp;[^\\)]]+"
        ));
        let out_of_class_ll = xml.matches("\\p{Ll}").count()
            - xml.matches("[\\p{Ll}]").count()
            - xml.matches("[\\p{Ll}&amp;").count();
        assert!(
            out_of_class_ll > 100,
            "grammar.xml carries many out-of-class \\p{{Ll}} uses: {out_of_class_ll}"
        );
        assert_eq!(normalized.matches("(?-i:\\p{Ll})").count(), out_of_class_ll);
        assert_eq!(
            normalized.matches("(?-i:\\p{Lu})").count(),
            xml.matches("\\p{Lu}").count()
                - xml.matches("[\\p{Lu}\\d]").count()
                - xml.matches("[\\p{Lu}^R]").count()
                - xml.matches("[^\\p{Lu}]").count()
        );
        assert!(normalized.contains("[\\p{Lu}\\d]"));
        assert!(normalized.contains("[\\p{L}&amp;&amp;[^\\p{Lu}]"));
        assert_eq!(normalized.matches("[\\p{Ll}]").count(), 1);
        let ascii_punct = regex::Regex::new("[!-/:-@\\[-`{-~]").unwrap();
        for hit in [",", ".", ";", "!", "(", "[", "{", "@", "?"] {
            assert!(ascii_punct.is_match(hit), "{hit:?} is ASCII punct");
        }
        for miss in [
            "\u{201e}", "\u{2018}", "\u{2013}", "\u{2026}", "w", "\u{a0}",
        ] {
            assert!(!ascii_punct.is_match(miss), "{miss:?} is not ASCII punct");
        }
        let java_ci_lower = regex::RegexBuilder::new("(?-i:\\p{Ll})+")
            .case_insensitive(true)
            .build()
            .unwrap();
        assert!(!java_ci_lower.is_match("IPN"));
        assert!(java_ci_lower.is_match("ohn"));
        let folded_lower = regex::RegexBuilder::new("\\p{Ll}+")
            .case_insensitive(true)
            .build()
            .unwrap();
        assert!(
            folded_lower.is_match("IPN"),
            "the regex crate case-folds \\p{{Ll}} under (?i), Java does not"
        );
    }

    #[test]
    fn normalize_pl_rules_xml_rewrites_the_java_only_regex_constructs() {
        let xml =
            std::fs::read_to_string(pl_rules_dir().expect("pl rules dir").join("grammar.xml"))
                .expect("grammar.xml");
        let normalized = normalize_pl_rules_xml(&xml);
        assert_eq!(
            xml.matches("(?-iu)").count(),
            19,
            "the round-10 inventory of inline unicode-flag negations"
        );
        assert_eq!(xml.matches("\\02").count(), 1);
        assert!(normalized.contains("(?-i)w.*"));
        assert!(normalized.contains("\\x{2}"));
        assert!(!normalized.contains("(?-iu)"));
        assert!(!normalized.contains("\\02"));
        assert_eq!(normalized.matches("(?-i)").count(), 19);
        assert_eq!(
            normalized.matches("<example").count(),
            xml.matches("<example").count()
        );
        assert_eq!(
            normalized.matches("<rule").count(),
            xml.matches("<rule").count()
        );
    }

    #[test]
    fn normalize_pl_rules_xml_escapes_backslash_pairs_and_borrows_clean_input() {
        assert!(matches!(
            normalize_pl_rules_xml("<rules lang='pl'/>"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(normalize_pl_rules_xml("a(?-iu)w.*b").as_ref(), "a(?-i)w.*b");
        assert_eq!(
            normalize_pl_rules_xml("[\\\\02]|(?-iu)Don").as_ref(),
            "[\\\\02]|(?-i)Don"
        );
        assert_eq!(normalize_pl_rules_xml("\\021x").as_ref(), "\\x{11}x");
        assert_eq!(normalize_pl_rules_xml("\\0").as_ref(), "\\0");
    }

    #[test]
    fn normalized_regexes_carry_java_semantics() {
        let punct = regex::Regex::new("^(?:[?\\.!,:;\u{2026}\u{201d}\"\\x{2}])$").unwrap();
        for hit in [".", "!", "?", ",", ":", ";", "\u{2026}", "\u{201d}", "\""] {
            assert!(
                punct.is_match(hit),
                "{hit:?} must hit the BRAK_KROPKI class"
            );
        }
        assert!(!punct.is_match("z\u{142}otych."));
        let day = regex::Regex::new("^(?:(?-i)w.*)$").unwrap();
        assert!(day.is_match("wielki"));
        assert!(!day.is_match("Wielki"));
        let exceptions = regex::RegexBuilder::new("^(?:minus|plus|lada|(?-i)Don|Los)$")
            .case_insensitive(true)
            .build()
            .unwrap();
        assert!(exceptions.is_match("MINUS"));
        assert!(exceptions.is_match("Don"));
        assert!(!exceptions.is_match("don"));
        assert!(!exceptions.is_match("los"));
    }

    #[test]
    fn java_pins_compound_rule_examples() {
        let rule = polish_compound_rule();
        let pipeline = crate::pl::test_support::PolishPipeline::shared();
        let text = "Witamy w Rabka Zdr\u{f3}j.";
        let sentence = pipeline.analyze_range(text, 0, text.len());
        let matches = og_core::rule::Rule::match_sentence(&rule, &sentence);
        assert!(
            matches
                .iter()
                .any(|m| &text[m.offset()..m.offset() + m.length()] == "Rabka Zdr\u{f3}j"),
            "the compounds.txt entry Rabka-Zdr\u{f3}j* fires on the spaced form: {matches:?}"
        );
        let text = "Witamy w \u{141}apu capu.";
        let sentence = pipeline.analyze_range(text, 0, text.len());
        let matches = og_core::rule::Rule::match_sentence(&rule, &sentence);
        assert!(
            matches
                .iter()
                .any(|m| &text[m.offset()..m.offset() + m.length()] == "\u{141}apu capu"),
            "the description's \u{141}apu-capu example fires: {matches:?}"
        );
        let text = "Witamy w Rabka-Zdr\u{f3}j.";
        let sentence = pipeline.analyze_range(text, 0, text.len());
        assert!(
            og_core::rule::Rule::match_sentence(&rule, &sentence).is_empty(),
            "the hyphenated form is accepted"
        );
    }

    #[test]
    fn java_pins_word_coherency_example() {
        let rule = polish_word_coherency_rule();
        let pipeline = crate::pl::test_support::PolishPipeline::shared();
        let text = "Kupi\u{142}em grapefruity, a grejpfrut\u{f3}w nie lubi\u{119}.";
        let sentences = pipeline.analyze_text(text);
        let matches = og_core::rule::TextLevelRule::match_text(&rule, text, &sentences);
        assert_eq!(
            matches.len(),
            1,
            "the Java example pair pins one coherency match"
        );
    }

    #[test]
    fn java_pins_unpaired_brackets_example() {
        let rule = pl_unpaired_brackets_rule();
        let pipeline = crate::pl::test_support::PolishPipeline::shared();
        let text = "To jest zdanie z \u{201e}cudzys\u{142}owem.";
        let sentences = pipeline.analyze_text(text);
        let matches = og_core::rule::TextLevelRule::match_text(&rule, text, &sentences);
        assert!(
            !matches.is_empty(),
            "the Polish quote symbols are tracked: {matches:?}"
        );
        let text = "To jest zdanie z \u{201e}cudzys\u{142}owem\u{201d}.";
        let sentences = pipeline.analyze_text(text);
        assert!(rule.match_text(text, &sentences).is_empty());
    }
}

#[cfg(test)]
mod dict_probe {
    use og_tagger::morfologik::MorfologikDictionary;

    fn polish_dict() -> MorfologikDictionary {
        let dir = super::tagger::pl_resource_dir().expect("pl resource dir");

        MorfologikDictionary::load(&dir.join("polish.dict"), &dir.join("polish.info"))
            .expect("polish.dict loads")
    }

    #[ignore]
    #[test]
    fn probe_oracle_divergence_sample() {
        let _release_guard = crate::release_guard();
        let dir = super::tagger::pl_resource_dir().expect("pl resource dir");
        let hunspell = og_tagger::morfologik::FsaWordSet::load(
            &dir.join("hunspell/pl_PL.dict"),
            &dir.join("hunspell/pl_PL.info"),
        )
        .expect("pl_PL words");
        let tagger =
            og_tagger::generic_tagger::shared_dictionary(&dir, "polish").expect("polish dict");
        let mut missing: Vec<String> = Vec::new();
        let mut checked = 0usize;
        let mut i = 0usize;
        hunspell.for_each_word(|w: &str| {
            let sample = i % 2267 == 0;
            i += 1;
            if !sample {
                return;
            }
            checked += 1;
            if tagger.lookup(w).is_empty() && !missing.iter().any(|m| m == w) {
                missing.push(w.to_string());
            }
        });
        eprintln!(
            "checked {checked} hunspell words; not in polish.dict: {}",
            missing.len()
        );
        for w in missing.iter().take(10) {
            eprintln!("  hunspell-only: {w}");
        }
    }

    #[test]
    fn probe_hunspell_words() {
        let dir = super::tagger::pl_resource_dir().expect("pl resource dir");
        let words = og_tagger::morfologik::FsaWordSet::load(
            &dir.join("hunspell/pl_PL.dict"),
            &dir.join("hunspell/pl_PL.info"),
        )
        .expect("pl_PL hunspell FSA");
        eprintln!("pl_PL hunspell word count: {}", words.word_count());
        let lookup = |w: &str| words.contains(w);
        for w in [
            "bledem",
            "b\u{142}\u{119}dem",
            "kobieta",
            "warszawa",
            "nauk\u{119}",
            "ekstradycj\u{119}",
        ] {
            eprintln!("hunspell contains {w}: {}", lookup(w));
        }
        let mut with_prefix: Vec<String> = Vec::new();
        words.for_each_word(|w: &str| {
            if (w.starts_with("quasi") || w.starts_with("niby")) && with_prefix.len() < 10 {
                with_prefix.push(w.to_string());
            }
        });
        eprintln!("quasi/niby words: {with_prefix:?}");
    }

    #[ignore]
    #[test]
    fn probe_speller_words() {
        let _release_guard = crate::release_guard();
        let rule = crate::pl::speller::PolishSpellerRule::shared();
        for text in [
            "To jest zdanie z b\u{142}\u{119}dem.",
            "To jest bia\u{142}ozielony dach.",
            "To jest supernowoczesny sprz\u{119}t.",
            "On uprawia quasi-nauk\u{119}.",
        ] {
            let sentence = crate::pl::test_support::PolishPipeline::shared().analyze_range(
                text,
                0,
                text.len(),
            );
            let matches = og_core::rule::Rule::match_sentence(rule.as_ref(), &sentence);
            eprintln!("{text} -> {}", matches.len());
            for m in &matches {
                let rel = m.offset().saturating_sub(sentence.start());
                eprintln!(
                    "   word {:?} known={} offset {} len {}",
                    &sentence.text()[rel..rel + m.length()],
                    rule.is_known_word(&sentence.text()[rel..rel + m.length()]),
                    m.offset(),
                    m.length()
                );
            }
        }
    }

    #[ignore]
    #[test]
    fn probe_word_repeat_tokens() {
        let _release_guard = crate::release_guard();
        let text =
            "M\u{f3}wi\u{142}a d\u{142}ugo, bo lubi\u{142}a robi\u{107} wszystko d\u{142}ugo.";
        let sentence =
            crate::pl::test_support::PolishPipeline::shared().analyze_range(text, 0, text.len());
        let mut inflected: std::collections::HashSet<String> = Default::default();
        for (i, t) in sentence.non_whitespace_tokens().iter().enumerate() {
            let _ = &mut inflected;
            let readings: Vec<String> = t
                .readings()
                .iter()
                .map(|r| format!("{}[{}]", r.lemma().unwrap_or("-"), r.pos_tags().join("+")))
                .collect();
            eprintln!("{i} {:?} => {}", t.token().token(), readings.join(", "));
        }
    }

    #[ignore]
    #[test]
    fn probe_coherency_matches() {
        let _release_guard = crate::release_guard();
        let rule = crate::pl::polish_word_coherency_rule();
        let text = "Kupi\u{142}em grapefruity, a grejpfrut\u{f3}w nie lubi\u{119}.";
        let sentences = crate::pl::test_support::PolishPipeline::shared().analyze_text(text);
        let matches = og_core::rule::TextLevelRule::match_text(&rule, text, &sentences);
        eprintln!("coherency matches: {}", matches.len());
        for m in &matches {
            eprintln!(
                "  match off {} len {} msg {}",
                m.offset(),
                m.length(),
                m.message()
            );
        }
        let inline = crate::en::WordCoherencyRule::for_language("X", "Y", "grapefruit;grejpfrut\n");
        let matches_inline = og_core::rule::TextLevelRule::match_text(&inline, text, &sentences);
        eprintln!("inline data matches: {}", matches_inline.len());
        let rule2 = crate::pl::polish_compound_rule();
        for w in [
            "Rabka Zdr\u{f3}j",
            "Rabka-Zdr\u{f3}j",
            "\u{141}apu capu",
            "\u{141}apu-capu",
        ] {
            let sentence =
                crate::pl::test_support::PolishPipeline::shared().analyze_range(w, 0, w.len());
            eprintln!(
                "compound {w:?} -> {}",
                og_core::rule::Rule::match_sentence(&rule2, &sentence).len()
            );
        }
    }

    #[ignore]
    #[test]
    fn probe_coherency_tokens() {
        let _release_guard = crate::release_guard();
        let text = "Kupi\u{142}em grapefruity, a grejpfrut\u{f3}w nie lubi\u{119}.";
        for (si, sentence) in crate::pl::test_support::PolishPipeline::shared()
            .analyze_text(text)
            .into_iter()
            .enumerate()
        {
            for t in sentence.non_whitespace_tokens() {
                let readings: Vec<String> = t
                    .readings()
                    .iter()
                    .map(|r| format!("{}[{}]", r.lemma().unwrap_or("-"), r.pos_tags().join("+")))
                    .collect();
                eprintln!("{si} {:?} => {}", t.token().token(), readings.join(", "));
            }
        }
    }

    #[test]
    fn probe_polish_readings() {
        let dict = polish_dict();
        for w in [
            "kobieta",
            "kobietą",
            "K",
            "k",
            "W",
            "w",
            "Były",
            "były",
            "rozum",
            "działanie",
            "najlepiej",
            "lepiej",
            "nielepiej",
            "białozielony",
            "polsko",
            "warszawa",
            "Warszawa",
            "niemiecko",
            "indonezyjski",
            "kobieta-wojownik",
            "Rabke",
            "Zdroju",
            "Rabke-Zdroju",
            "32-u",
            "64-bitowe",
            "XIX-go",
            "bledem",
            "ble",
            "dem",
            "b\u{142}\u{119}d",
            "b\u{142}\u{119}dem",
            "b\u{142}\u{119}du",
            "b\u{142}\u{119}dy",
            "edem",
            "bled",
            "quasi",
            "quasi-nauk\u{119}",
            "nauk\u{119}",
            "bia\u{142}ozielony",
            "bia\u{142}o",
            "zielony",
            "trzynastobitowy",
            "supernowoczesny",
            "ekstradycj\u{119}",
        ] {
            let readings: Vec<String> = dict
                .lookup(w)
                .iter()
                .map(|(l, t)| format!("{l}/{t}"))
                .collect();
            eprintln!("{w} => [{}]", readings.join(", "));
        }
    }
}
