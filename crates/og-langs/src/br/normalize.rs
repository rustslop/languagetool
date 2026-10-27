use std::borrow::Cow;

fn attribute_value_is_yes(tag: &str, attr: &str) -> bool {
    let mut i = 0usize;
    let bytes = tag.as_bytes();
    while let Some(start) = tag[i..].find(attr) {
        let at = i + start;
        let before_ok = at == 0 || bytes[at - 1].is_ascii_whitespace() || bytes[at - 1] == b'/';
        let mut j = at + attr.len();
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if before_ok && bytes.get(j) == Some(&b'=') {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if bytes.get(j) == Some(&b'"') || bytes.get(j) == Some(&b'\'') {
                let quote = bytes[j];
                let value_start = j + 1;
                let value_end = tag[value_start..]
                    .find(quote as char)
                    .map(|p| value_start + p)
                    .unwrap_or(tag.len());
                return tag[value_start..value_end].eq_ignore_ascii_case("yes");
            }
        }
        i = at + attr.len();
    }
    false
}

// The engine normalizes U+2019 on the test side of exception/token-regexp
// matching but not on patterns, fast paths, or inflected lemmas.
pub fn normalize_br_rules_xml(xml: &str) -> Cow<'_, str> {
    if !xml.contains('\u{2019}') && !xml.contains('\u{2018}') {
        return Cow::Borrowed(xml);
    }
    let mut out = String::with_capacity(xml.len());
    let bytes = xml.as_bytes();
    let mut i = 0usize;
    let mut stack: Vec<(String, bool)> = Vec::new();
    let mut in_comment = false;
    while i < bytes.len() {
        if in_comment {
            if xml[i..].starts_with("-->") {
                in_comment = false;
                out.push_str("-->");
                i += 3;
            } else {
                let step = utf8_step(&xml[i..]);
                out.push_str(&xml[i..i + step]);
                i += step;
            }
            continue;
        }
        let b = bytes[i];
        if b == b'<' && xml[i..].starts_with("<!--") {
            in_comment = true;
            out.push_str("<!--");
            i += 4;
            continue;
        }
        if b == b'<' {
            let end = xml[i..].find('>').map(|p| i + p).unwrap_or(xml.len());
            let tag_content = &xml[i + 1..end.min(xml.len())];
            let name = tag_content
                .trim_start_matches('/')
                .split(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
                .next()
                .unwrap_or("")
                .to_string();
            let is_close = tag_content.starts_with('/');
            let is_self_closing = tag_content.ends_with('/');
            if is_close {
                stack.pop();
            } else if !is_self_closing && !name.is_empty() {
                let normalize = match name.as_str() {
                    "exception" => true,
                    "token" => {
                        attribute_value_is_yes(tag_content, "regexp")
                            && !attribute_value_is_yes(tag_content, "inflected")
                    }
                    _ => false,
                };
                stack.push((name, normalize));
            }
            out.push_str(&xml[i..end.min(xml.len())]);
            if end < xml.len() {
                out.push('>');
            }
            i = end + 1;
            continue;
        }
        let step = utf8_step(&xml[i..]);
        let ch = &xml[i..i + step];
        let normalize_text = stack.last().is_some_and(|(_, normalize)| *normalize);
        if normalize_text && (ch == "\u{2019}" || ch == "\u{2018}") {
            out.push('\'');
        } else {
            out.push_str(ch);
        }
        i += step;
    }
    Cow::Owned(out)
}

fn utf8_step(s: &str) -> usize {
    s.chars().next().map_or(1, char::len_utf8)
}

// Java defaults an action-less <disambig> to REPLACE; the shared parser only
// defaults when a postag attribute is present, so bare elements are dropped.
pub fn normalize_br_disambiguation_xml(xml: &str) -> Cow<'_, str> {
    if !xml.contains("<disambig>") {
        return Cow::Borrowed(xml);
    }
    Cow::Owned(xml.replace("<disambig>", "<disambig action=\"replace\">"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_token_bodies_keep_the_typographic_apostrophe() {
        let xml = "<rules><rule><pattern><token>pelec’h</token></pattern></rule></rules>";
        assert!(normalize_br_rules_xml(xml).contains("pelec’h"));
    }

    #[test]
    fn regexp_token_bodies_get_the_ascii_apostrophe() {
        let xml =
            "<rules><rule><pattern><token regexp=\"yes\">c’h.*</token></pattern></rule></rules>";
        assert!(normalize_br_rules_xml(xml).contains("c'h.*"));
    }

    #[test]
    fn exception_bodies_normalize_even_when_inflected() {
        let xml = "<rules><rule><pattern><token>c’hwi</token><exception inflected=\"yes\" regexp=\"yes\">(g|c’h).*</exception></pattern></rule></rules>";
        let out = normalize_br_rules_xml(xml);
        assert!(out.contains("c’hwi"));
        assert!(out.contains("(g|c'h).*"));
    }

    #[test]
    fn inflected_regexp_token_bodies_keep_the_typographic_apostrophe() {
        let xml = "<rules><rule><pattern><token regexp=\"yes\" inflected=\"yes\">k.*|marc’h</token></pattern></rule></rules>";
        assert!(normalize_br_rules_xml(xml).contains("marc’h"));
    }

    #[test]
    fn messages_examples_and_comments_stay_untouched() {
        let xml = "<!-- c’h comment --><rules><rule><message>Ur c’hemmadur fall</message><example correction=\"c’hiez\">Hor <marker>kiez</marker></example></rule></rules>";
        let out = normalize_br_rules_xml(xml);
        assert!(out.contains("Ur c’hemmadur fall"));
        assert!(out.contains("correction=\"c’hiez\""));
        assert!(out.contains("c’h comment"));
    }

    #[test]
    fn nested_match_inside_token_keeps_its_attributes() {
        let xml = "<rules><rule><pattern><token postag=\"J cmp.*\" postag_regexp=\"yes\" regexp=\"yes\"><match no=\"0\" regexp_match=\"^.(.*)\" regexp_replace=\"(k|c’h)$1oc’h\"/></token></pattern></rule></rules>";
        let out = normalize_br_rules_xml(xml);
        assert!(out.contains("regexp_replace=\"(k|c’h)$1oc’h\""));
    }

    #[test]
    fn left_single_quote_is_normalized_like_the_right_one() {
        let xml =
            "<rules><rule><pattern><token regexp=\"yes\">gwerc‘h</token></pattern></rule></rules>";
        assert!(normalize_br_rules_xml(xml).contains("gwerc'h"));
    }

    #[test]
    fn action_less_disambig_elements_become_replaces() {
        let xml = "<rules><rule><pattern><marker><token>e</token></marker></pattern><disambig><wd lemma=\"bezañ\" pos=\"X DA_LAVARET_EO\"/></disambig></rule></rules>";
        assert!(normalize_br_disambiguation_xml(xml).contains("<disambig action=\"replace\">"));
    }

    #[test]
    fn disambig_actions_stay_untouched() {
        let xml = "<rules><rule><pattern><marker><token>eo</token></marker></pattern><disambig action=\"add\"><wd lemma=\"bezañ\" pos=\"X\"/></disambig></rule></rules>";
        assert!(normalize_br_disambiguation_xml(xml).contains("<disambig action=\"add\">"));
    }
}
