use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use std::collections::HashMap;
use std::sync::Arc;

pub struct GalicianSimpleReplaceRule {
    id: &'static str,
    description: &'static str,
    short: &'static str,
    message: &'static str,
    category: Category,
    issue_type: IssueType,
    entries: HashMap<String, Vec<String>>,
    synth: Option<Arc<og_tagger::morfologik::SynthDictionary>>,
}

fn shared_gl_synth() -> Option<Arc<og_tagger::morfologik::SynthDictionary>> {
    static SYNTH: og_core::releasable::Releasable<og_tagger::morfologik::SynthDictionary> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&SYNTH, || {
        super::tagger::gl_resource_dir()
            .and_then(|dir| og_tagger::morfologik::shared_synth_dictionary(&dir, "galician_synth"))
    })
}

impl GalicianSimpleReplaceRule {
    pub fn new(
        id: &'static str,
        description: &'static str,
        short: &'static str,
        message: &'static str,
    ) -> Self {
        Self {
            id,
            description,
            short,
            message,
            category: Category::new("MISC", "Miscelánea"),
            issue_type: IssueType::Misspelling,
            entries: HashMap::new(),
            synth: shared_gl_synth(),
        }
    }

    pub fn load_data(&mut self, data: &str) {
        self.entries = parse_replace_data(data);
    }

    fn lookup(&self, token: &str) -> Option<Vec<String>> {
        self.entries
            .get(token)
            .cloned()
            .or_else(|| self.entries.get(&token.to_lowercase()).cloned())
    }

    fn lemma_replacements(&self, readings: &AnalyzedTokenReadings) -> Option<Vec<String>> {
        let mut lemmas: Vec<String> = Vec::new();
        for reading in readings.readings() {
            if let Some(lemma) = reading.lemma()
                && self.entries.contains_key(lemma)
                && !lemmas.contains(&lemma.to_lowercase())
            {
                lemmas.push(lemma.to_lowercase());
            }
        }
        if lemmas.is_empty() {
            return None;
        }
        let mut possible: Vec<String> = Vec::new();
        for lemma in &lemmas {
            let Some(replacements) = self.entries.get(lemma) else {
                continue;
            };
            match &self.synth {
                Some(synth) => {
                    for replacement_lemma in replacements {
                        for reading in readings.readings() {
                            let forms = synth.lookup(&format!(
                                "{replacement_lemma}|{}",
                                reading.pos_tags().first().map(String::as_str).unwrap_or("")
                            ));
                            for form in forms {
                                if !possible.contains(&form) {
                                    possible.push(form);
                                }
                            }
                        }
                    }
                }
                None => {
                    for replacement in replacements {
                        if !possible.contains(replacement) {
                            possible.push(replacement.clone());
                        }
                    }
                }
            }
        }
        Some(possible)
    }
}

pub(crate) fn parse_replace_data(data: &str) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for line in data.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split('=').collect();
        if parts.len() != 2 || parts[1].trim().is_empty() {
            continue;
        }
        let replacements: Vec<String> = parts[1].split('|').map(str::to_string).collect();
        for wrong_form in parts[0].split('|') {
            map.insert(wrong_form.to_string(), replacements.clone());
        }
    }
    map
}

fn is_all_uppercase(s: &str) -> bool {
    s.chars().all(|c| !c.is_alphabetic() || c.is_uppercase())
}

fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => s.to_string(),
    }
}

pub(crate) fn context_window(
    sentence: &AnalyzedSentence,
    from: usize,
    to: usize,
    pad: usize,
) -> RuleMatchContext {
    let text = sentence.text();
    let text_len = sentence.start() + text.len();
    let mut ctx_start = from.saturating_sub(pad).max(sentence.start());
    while ctx_start > sentence.start() && !text.is_char_boundary(ctx_start - sentence.start()) {
        ctx_start -= 1;
    }
    let mut ctx_end = (to + pad).min(text_len);
    while ctx_end < text_len && !text.is_char_boundary(ctx_end - sentence.start()) {
        ctx_end += 1;
    }
    let local_start = ctx_start - sentence.start();
    let local_end = ctx_end - sentence.start();
    RuleMatchContext::new(
        text[local_start..local_end].to_string(),
        ctx_start,
        ctx_end - ctx_start,
    )
}

impl Rule for GalicianSimpleReplaceRule {
    fn id(&self) -> &str {
        self.id
    }

    fn description(&self) -> &str {
        self.description
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        self.issue_type.clone()
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        for token in sentence.non_whitespace_tokens() {
            if token.is_sentence_start() || token.is_immunized() || token.is_ignored_by_speller() {
                continue;
            }
            let original = token.token().token();
            let possible = match self.lookup(original) {
                Some(replacements) => Some(replacements),
                None => self.lemma_replacements(token),
            };
            let Some(possible) = possible else {
                continue;
            };
            let mut replacements: Vec<String> = if is_all_uppercase(original) {
                possible.iter().map(|s| s.to_uppercase()).collect()
            } else {
                possible
            };
            replacements.retain(|r| r != original);
            if replacements.is_empty() {
                continue;
            }
            let message = self
                .message
                .replacen("$suggestions", &replacements.join(", "), 1)
                .replacen("$match", original, 1);
            let from = token.token().start();
            let to = token.token().end();
            let rm_rule = RuleMatchRule::new(self.id, self.description)
                .with_category(self.category.clone())
                .with_issue_type(self.issue_type.as_str());
            let starts_with_uppercase = original
                .chars()
                .next()
                .is_some_and(|c| c.is_uppercase() && !is_all_uppercase(original));
            if starts_with_uppercase {
                for replacement in &mut replacements {
                    *replacement = uppercase_first_char(replacement);
                }
            }
            rule_matches.push(
                RuleMatch::new(
                    message,
                    from,
                    to - from,
                    rm_rule,
                    context_window(sentence, from, to, 40),
                )
                .with_short_message(self.short.to_string())
                .with_replacements(replacements.iter().map(SuggestedReplacement::new).collect())
                .with_sentence(sentence.text().to_string()),
            );
        }
        rule_matches
    }
}

const WORDS_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/gl/src/main/resources/org/languagetool/rules/gl/words.txt"
);
const SPANISH_TXT: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/gl/src/main/resources/org/languagetool/rules/gl/spanish.txt"
);

pub fn simple_replace_rule() -> GalicianSimpleReplaceRule {
    let mut rule = GalicianSimpleReplaceRule::new(
        "GL_SIMPLE_REPLACE",
        "Corrección de erros léxicos (barbarismos).",
        "Erros léxicos",
        "'$match' non existe en galego. Talvez quería vostede dicir: $suggestions.",
    );
    rule.load_data(WORDS_TXT);
    rule
}

pub fn cast_words_rule() -> GalicianSimpleReplaceRule {
    let mut rule = GalicianSimpleReplaceRule::new(
        "GL_CAST_WORDS",
        "Corrección de erros léxicos (castelanismos).",
        "Castelanismos léxicos",
        "'$match' é un castelanismo. Empregue no seu sitio: $suggestions.",
    );
    rule.load_data(SPANISH_TXT);
    rule
}
#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings};

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut pos = 0usize;
        for word in text.split_whitespace() {
            let start = pos + text[pos..].find(word).expect("word offset");
            let end = start + word.len();
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                word, start, end,
            )));
            pos = end;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    fn make_lemma_sentence(token: &str, lemma: &str, tag: &str) -> AnalyzedSentence {
        let text = format!("un {token} aquí.");
        let start = text.find(token).expect("token offset");
        let end = start + token.len();
        let mut sentence = AnalyzedSentence::new(text.clone(), 0, text.len());
        let reading = AnalyzedToken::new(token, start, end)
            .with_lemma(lemma)
            .with_pos_tags(vec![tag.to_string()]);
        let atr = AnalyzedTokenReadings::new(AnalyzedToken::new(token, start, end))
            .with_readings(vec![reading]);
        sentence.set_tokens(vec![
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
            atr,
        ]);
        sentence
    }

    fn values(m: &og_core::RuleMatch) -> Vec<&str> {
        m.replacements().iter().map(|r| r.value()).collect()
    }

    #[test]
    fn parse_replace_data_skips_empty_replacements_and_comments() {
        let map = parse_replace_data("# c\n\na|b=c|d\nbad\nempty=\n");
        assert_eq!(map.get("a"), Some(&vec!["c".to_string(), "d".to_string()]));
        assert_eq!(map.get("b"), Some(&vec!["c".to_string(), "d".to_string()]));
        assert!(!map.contains_key("bad"));
        assert!(!map.contains_key("empty"));
    }

    #[test]
    fn simple_replace_rule_suggests_the_galician_form() {
        let text = "un achádego recente.";
        let matches = simple_replace_rule().match_sentence(&make_sentence(text));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), text.find("achádego").unwrap());
        assert_eq!(values(&matches[0]), vec!["achado"]);
        assert_eq!(
            matches[0].message(),
            "'achádego' non existe en galego. Talvez quería vostede dicir: achado."
        );
        assert_eq!(matches[0].rule().id(), "GL_SIMPLE_REPLACE");
    }

    #[test]
    fn cast_words_rule_flags_the_spanesian_loan() {
        let matches = cast_words_rule().match_sentence(&make_sentence("na acera nova"));
        assert_eq!(matches.len(), 1);
        assert_eq!(values(&matches[0]), vec!["beirarrúa"]);
        assert_eq!(
            matches[0].message(),
            "'acera' é un castelanismo. Empregue no seu sitio: beirarrúa."
        );
    }

    #[test]
    fn case_variants_adjust_the_suggestion() {
        let rule = cast_words_rule();
        let matches = rule.match_sentence(&make_sentence("na Acera nova"));
        assert_eq!(values(&matches[0]), vec!["Beirarrúa"]);
        let matches = rule.match_sentence(&make_sentence("na ACERA nova"));
        assert_eq!(values(&matches[0]), vec!["BEIRARRÚA"]);
    }

    #[test]
    fn lemma_lookup_synthesizes_the_inflected_form() {
        let rule = cast_words_rule();
        let matches = rule.match_sentence(&make_lemma_sentence("beirarrúas", "acera", "NCFS000"));
        assert_eq!(matches.len(), 1);
        assert_eq!(values(&matches[0]), vec!["beirarrúa"]);
    }

    #[test]
    fn lemma_lookup_without_a_synthesizable_tag_yields_no_match() {
        let rule = cast_words_rule();
        assert_eq!(
            rule.match_sentence(&make_lemma_sentence("beirarrúas", "acera", "VMIP3S0"))
                .len(),
            0
        );
    }
}
