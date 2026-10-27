use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use std::collections::{HashMap, VecDeque};

pub const BRETON_TOPO: &str = "BR_TOPO";

const TOPO_DESCRIPTION: &str = "anvioù-lec’h e brezhoneg";
const TOPO_SHORT: &str = "anvioù lec’h";
const TOPO_SUGGESTION: &str = " zo un anv lec’h gallek. Ha fellout a rae deoc’h skrivañ ";
const TOPO_SUGGESTIONS_SEPARATOR: &str = " pe ";

pub struct TopoReplaceRule {
    maps: Vec<HashMap<String, String>>,
}

fn load_maps(data: &str) -> Vec<HashMap<String, String>> {
    use og_tokenizer::WordTokenizer as _;
    let tokenizer = super::word_tokenizer::BretonWordTokenizer::new();
    let mut maps: Vec<HashMap<String, String>> = Vec::new();
    for raw_line in data.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('=');
        let wrong_side = parts.next().unwrap_or("");
        let Some(correct) = parts.next() else {
            continue;
        };
        if parts.next().is_some() {
            continue;
        }
        for wrong_form in wrong_side.split('|') {
            let word_count = tokenizer
                .tokenize(wrong_form)
                .iter()
                .filter(|token| !token.text().trim().is_empty())
                .count();
            if word_count == 0 {
                continue;
            }
            while maps.len() < word_count {
                maps.push(HashMap::new());
            }
            maps[word_count - 1].insert(wrong_form.to_string(), correct.to_string());
        }
    }
    maps
}

fn context(sentence: &AnalyzedSentence, start: usize, end: usize) -> RuleMatchContext {
    let full_text = sentence.text();
    let rel_start = start.saturating_sub(sentence.start());
    let rel_end = (rel_start + (end - start)).min(full_text.len());
    let mut context_start = rel_start.saturating_sub(40);
    while context_start < full_text.len() && !full_text.is_char_boundary(context_start) {
        context_start += 1;
    }
    let mut context_end = std::cmp::min(rel_end + 40, full_text.len());
    while context_end > context_start && !full_text.is_char_boundary(context_end) {
        context_end -= 1;
    }
    RuleMatchContext::new(
        full_text[context_start..context_end].to_string(),
        context_start,
        context_end - context_start,
    )
}

impl TopoReplaceRule {
    pub fn new() -> Self {
        Self {
            maps: load_maps(include_str!(
                "../../../../languagetool/languagetool-language-modules/br/src/main/resources/org/languagetool/rules/br/topo.txt"
            )),
        }
    }

    pub fn get_wrong_words(&self) -> &[HashMap<String, String>] {
        &self.maps
    }
}

impl Default for TopoReplaceRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for TopoReplaceRule {
    fn id(&self) -> &str {
        BRETON_TOPO
    }

    fn description(&self) -> &str {
        TOPO_DESCRIPTION
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Uncategorized
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let capacity = self.maps.len();
        let tokens: Vec<&AnalyzedTokenReadings> = sentence
            .tokens()
            .iter()
            .filter(|token| !token.is_whitespace())
            .collect();
        let mut rule_matches = Vec::new();
        let mut window: VecDeque<&AnalyzedTokenReadings> = VecDeque::with_capacity(capacity);
        for token in tokens.iter().skip(1) {
            if capacity > 0 && window.len() == capacity {
                window.pop_front();
            }
            window.push_back(token);
            let window_refs: Vec<&AnalyzedTokenReadings> = window.iter().copied().collect();
            let len = window_refs.len();
            for start in 0..len {
                if window_refs[start].is_immunized() {
                    continue;
                }
                let mut crt = String::new();
                for (k, window_token) in window_refs.iter().enumerate().skip(start) {
                    if k > start && window_token.token().start() > window_refs[k - 1].token().end()
                    {
                        crt.push(' ');
                    }
                    crt.push_str(window_token.token().token());
                }
                let Some(correct) = self
                    .maps
                    .get(len - start - 1)
                    .and_then(|map| map.get(crt.as_str()))
                else {
                    continue;
                };
                let replacements: Vec<&str> = correct.split('|').collect();
                let mut message = format!("{crt}{TOPO_SUGGESTION}");
                let last = replacements.len().saturating_sub(1);
                for (k, replacement) in replacements.iter().enumerate() {
                    if k > 0 {
                        message.push_str(if k == last {
                            TOPO_SUGGESTIONS_SEPARATOR
                        } else {
                            ", "
                        });
                    }
                    message.push_str(&format!("<suggestion>{replacement}</suggestion>"));
                }
                message.push('?');
                let from_pos = window_refs[start].token().start();
                let to_pos = window_refs[len - 1].token().end();
                let rule = RuleMatchRule::new(BRETON_TOPO, TOPO_DESCRIPTION)
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                rule_matches.push(
                    RuleMatch::new(
                        message,
                        from_pos,
                        to_pos - from_pos,
                        rule,
                        context(sentence, from_pos, to_pos),
                    )
                    .with_short_message(TOPO_SHORT.to_string())
                    .with_replacements(
                        replacements
                            .iter()
                            .map(|r| SuggestedReplacement::new(*r))
                            .collect(),
                    )
                    .with_sentence(sentence.text().to_string()),
                );
                break;
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        use og_tokenizer::WordTokenizer as _;
        let mut tokens = vec![AnalyzedTokenReadings::new(
            og_core::AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.extend(
            super::super::word_tokenizer::BretonWordTokenizer::new()
                .tokenize(text)
                .into_iter()
                .map(|t| {
                    AnalyzedTokenReadings::new(og_core::AnalyzedToken::new(
                        t.text(),
                        t.start(),
                        t.end(),
                    ))
                }),
        );
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn loads_the_java_rule_shape() {
        let rule = TopoReplaceRule::new();
        assert_eq!(rule.id(), "BR_TOPO");
        assert!(!rule.get_wrong_words().is_empty());
        assert!(
            rule.get_wrong_words()[0]
                .get("France")
                .is_some_and(|v| v == "Frañs|bro-C’hall")
        );
    }

    #[test]
    fn java_pin_france_a_zo_ur_vro_matches_once() {
        let rule = TopoReplaceRule::new();
        let matches = rule.match_sentence(&sentence("France a zo ur vro."));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].offset() + matches[0].length(), "France".len());
        assert_eq!(
            matches[0].message(),
            "France zo un anv lec’h gallek. Ha fellout a rae deoc’h skrivañ <suggestion>Frañs</suggestion> pe <suggestion>bro-C’hall</suggestion>?"
        );
        let replacements: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(replacements, vec!["Frañs", "bro-C’hall"]);
        assert_eq!(matches[0].short_message().unwrap_or_default(), TOPO_SHORT);
    }

    #[test]
    fn java_pin_france_3_channel_name() {
        let rule = TopoReplaceRule::new();
        let sentence =
            crate::br::example_suite::disambiguated_sentence("France 3 a zo ur chadenn skinwel.");
        let france = sentence
            .tokens()
            .iter()
            .find(|t| t.token().token() == "France")
            .expect("France token");
        assert!(
            france.is_immunized(),
            "disambiguation.xml's FRANCE rulegroup must immunize 'France' before the \
             channel words, which is how Java's TopoReplaceRuleTest gets 0 matches"
        );
        assert_eq!(
            rule.match_sentence(&sentence).len(),
            0,
            "the immunized first token must be skipped like TopoReplaceRule.java's \
             prevTokensList.get(len - crtWordCount).isImmunized() check"
        );
        let plain = crate::br::example_suite::disambiguated_sentence("France a zo ur vro.");
        let france = plain
            .tokens()
            .iter()
            .find(|t| t.token().token() == "France")
            .expect("France token");
        assert!(!france.is_immunized());
        assert_eq!(rule.match_sentence(&plain).len(), 1);
    }

    #[test]
    fn java_pin_preceded_france_stays_immunized() {
        let rule = TopoReplaceRule::new();
        let sentence =
            crate::br::example_suite::disambiguated_sentence("Radio France a zo ur skingomz.");
        let france = sentence
            .tokens()
            .iter()
            .find(|t| t.token().token() == "France")
            .expect("France token");
        assert!(
            france.is_immunized(),
            "the FRANCE rulegroup's second rule immunizes 'France' after \
             Air|Anatole|Radio|de; got {:?}",
            france.readings()
        );
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn case_sensitive_lookup_keeps_lowercase_france_quiet() {
        let rule = TopoReplaceRule::new();
        assert!(
            rule.match_sentence(&sentence("france a zo ur vro."))
                .is_empty()
        );
    }

    #[test]
    fn hyphenated_toponyms_match_whole_tokens() {
        let rule = TopoReplaceRule::new();
        let matches = rule.match_sentence(&sentence("Aigrefeuille-sur-Maine a zo brav."));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(
            matches[0].offset() + matches[0].length(),
            "Aigrefeuille-sur-Maine".len()
        );
        assert_eq!(matches[0].replacements()[0].value(), "Kelenneg-ar-Mewan");
    }
}
