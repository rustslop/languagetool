use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchRule,
    rule::Rule,
};
use std::collections::HashSet;
use std::sync::LazyLock;

const PEOPLE_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/ga/src/main/resources/org/languagetool/rules/ga/people.txt"
);

static DAOINE: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    for line in PEOPLE_TXT.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(stripped) = line.strip_prefix('*') {
            set.insert(stripped.to_string());
        } else {
            set.insert(line.to_lowercase());
        }
    }
    set
});

const NUMBER_REPLACEMENTS: [(&str, &str); 9] = [
    ("dhá", "beirt"),
    ("trí", "triúr"),
    ("ceathair", "ceathrar"),
    ("cúig", "cúigear"),
    ("sé", "seisear"),
    ("seacht", "seachtar"),
    ("ocht", "ochtar"),
    ("naoi", "naonúr"),
    ("deich", "deichniúr"),
];

fn eq_ignore_case(a: &str, b: &str) -> bool {
    let mut ai = a.chars().flat_map(char::to_uppercase);
    let mut bi = b.chars().flat_map(char::to_uppercase);
    loop {
        match (ai.next(), bi.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) if x == y => {}
            _ => return false,
        }
    }
}

pub struct DhaNoBeirtRule {
    description: String,
    category: Category,
}

impl DhaNoBeirtRule {
    pub fn new() -> Self {
        Self {
            description: "'dhá' nó 'beirt'".to_string(),
            category: Category::new("MISC", "Miscellaneous"),
        }
    }

    fn is_number(token: &str) -> bool {
        NUMBER_REPLACEMENTS
            .iter()
            .any(|(num, _)| eq_ignore_case(num, token))
    }

    fn number_replacement(token: &str) -> Option<&'static str> {
        NUMBER_REPLACEMENTS
            .iter()
            .find(|(num, _)| *num == token)
            .map(|(_, repl)| *repl)
    }

    fn is_person(token: &AnalyzedTokenReadings) -> bool {
        if DAOINE.contains(&token.token().token().to_lowercase()) {
            return true;
        }
        token
            .readings()
            .iter()
            .filter_map(|r| r.lemma())
            .any(|lemma| DAOINE.contains(lemma))
    }

    fn push_match(
        &self,
        sentence: &AnalyzedSentence,
        token: &AnalyzedTokenReadings,
        message: String,
        matches: &mut Vec<RuleMatch>,
    ) {
        let from = token.token().start();
        let to = token.token().end();
        let rm_rule = RuleMatchRule::new("GA_DHA_NO_BEIRT", &self.description)
            .with_category(self.category.clone())
            .with_issue_type(IssueType::Misspelling.as_str());
        matches.push(
            RuleMatch::new(
                message,
                from,
                to - from,
                rm_rule,
                super::context_window(sentence, from, to, 40),
            )
            .with_short_message("Uimhir phearsanta".to_string())
            .with_sentence(sentence.text().to_string()),
        );
    }
}

impl Default for DhaNoBeirtRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for DhaNoBeirtRule {
    fn id(&self) -> &str {
        "GA_DHA_NO_BEIRT"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        let mut prev_token_index = 0usize;
        let mut replacement: Option<&'static str> = None;
        let mut msg: Option<String> = None;
        for i in 1..tokens.len() {
            if Self::is_number(tokens[i].token().token())
                && i < tokens.len().saturating_sub(2)
                && Self::is_person(tokens[i + 1])
            {
                if eq_ignore_case(tokens[i].token().token(), "dhá") {
                    for j in i + 2..tokens.len() {
                        if eq_ignore_case(tokens[j].token().token(), "déag") {
                            replacement = Some("dháréag");
                            let message =
                                "Ba chóir duit <suggestion>dháréag</suggestion> a scríobh"
                                    .to_string();
                            self.push_match(
                                sentence,
                                tokens[prev_token_index + 1],
                                message,
                                &mut rule_matches,
                            );
                            self.push_match(
                                sentence,
                                tokens[j],
                                "Ba chóir duit \"déag\" a scriosadh.".to_string(),
                                &mut rule_matches,
                            );
                            msg = None;
                        }
                    }
                }
                if replacement.is_none() {
                    replacement = Self::number_replacement(tokens[i].token().token());
                    if msg.is_none() {
                        let repl = replacement.unwrap_or("null");
                        msg = Some(format!(
                            "Ba chóir duit <suggestion>{repl} {}</suggestion> a scríobh",
                            tokens[i + 1].token().token()
                        ));
                    }
                }
            }
            if let Some(message) = msg.take() {
                self.push_match(
                    sentence,
                    tokens[prev_token_index + 1],
                    message,
                    &mut rule_matches,
                );
            }
            prev_token_index = i;
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken};

    fn make_sentence(text: &str, lemmas: &[(&str, &str)]) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut pos = 0usize;
        for word in text.split_whitespace() {
            let start = pos + text[pos..].find(word).expect("word offset");
            let end = start + word.len();
            let lemma = lemmas
                .iter()
                .find(|(w, _)| *w == word)
                .map(|(_, lemma)| *lemma);
            let mut readings = AnalyzedTokenReadings::new(AnalyzedToken::new(word, start, end));
            if let Some(lemma) = lemma {
                readings = readings
                    .with_readings(vec![AnalyzedToken::new(word, start, end).with_lemma(lemma)]);
            }
            tokens.push(readings);
            pos = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn java_pin_dha_no_beirt_battery() {
        let rule = DhaNoBeirtRule::new();
        let person = &[("dheartháireacha", "deartháir")];
        assert_eq!(
            rule.match_sentence(&make_sentence("Seo abairt bheag.", &[]))
                .len(),
            0
        );
        assert_eq!(
            rule.match_sentence(&make_sentence("Tá beirt dheartháireacha agam.", person))
                .len(),
            0
        );
        assert_eq!(
            rule.match_sentence(&make_sentence("Tá dhá dheartháireacha agam.", person))
                .len(),
            1
        );
        assert_eq!(
            rule.match_sentence(&make_sentence("Seo dhá ab déag", &[]))
                .len(),
            2
        );
        assert_eq!(
            rule.match_sentence(&make_sentence(
                "Tá dhá dheartháireacha níos aosta déag agam.",
                person
            ))
            .len(),
            2
        );
    }

    #[test]
    fn suggests_the_personal_number_form_like_java() {
        let text = "Tá dhá dheartháireacha agam.";
        let matches = DhaNoBeirtRule::new()
            .match_sentence(&make_sentence(text, &[("dheartháireacha", "deartháir")]));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), text.find("dhá").unwrap());
        assert_eq!(matches[0].length(), "dhá".len());
        assert_eq!(
            matches[0].message(),
            "Ba chóir duit <suggestion>beirt dheartháireacha</suggestion> a scríobh"
        );
        assert_eq!(matches[0].short_message(), Some("Uimhir phearsanta"));
    }

    #[test]
    fn deag_after_dha_flags_both_tokens() {
        let text = "Seo dhá ab déag";
        let matches = DhaNoBeirtRule::new().match_sentence(&make_sentence(text, &[]));
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].offset(), text.find("dhá").unwrap());
        assert_eq!(
            matches[0].message(),
            "Ba chóir duit <suggestion>dháréag</suggestion> a scríobh"
        );
        assert_eq!(matches[1].offset(), text.find("déag").unwrap());
        assert_eq!(matches[1].message(), "Ba chóir duit \"déag\" a scriosadh.");
    }

    #[test]
    fn person_is_recognized_from_token_text_and_lemma() {
        let rule = DhaNoBeirtRule::new();
        let via_text = make_sentence("Tá trí ab anseo.", &[]);
        assert_eq!(rule.match_sentence(&via_text).len(), 1);
        let via_lemma = make_sentence(
            "Tá trí dheartháireacha agam.",
            &[("dheartháireacha", "deartháir")],
        );
        assert_eq!(rule.match_sentence(&via_lemma).len(), 1);
    }

    #[test]
    fn uppercase_number_falls_back_to_java_null_replacement() {
        let text = "Tá DHÁ dheartháireacha agam.";
        let matches = DhaNoBeirtRule::new()
            .match_sentence(&make_sentence(text, &[("dheartháireacha", "deartháir")]));
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0]
                .message()
                .contains("<suggestion>null dheartháireacha</suggestion>")
        );
    }

    #[test]
    fn number_without_person_or_near_end_is_ignored() {
        let rule = DhaNoBeirtRule::new();
        assert_eq!(
            rule.match_sentence(&make_sentence("Tá dhá chat agam.", &[]))
                .len(),
            0
        );
        assert_eq!(
            rule.match_sentence(&make_sentence("Bhí ceathair ann.", &[]))
                .len(),
            0
        );
    }
}
