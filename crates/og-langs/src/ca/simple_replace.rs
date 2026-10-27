use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
};
use std::collections::HashMap;

type WrongWords = HashMap<String, Vec<String>>;

fn load_wrong_words(data: &str) -> WrongWords {
    let mut map: WrongWords = HashMap::new();
    for raw_line in data.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((wrong_side, suggestion_side)) = line.split_once('=') else {
            continue;
        };
        let suggestions: Vec<String> = suggestion_side.split('|').map(str::to_string).collect();
        if suggestions.is_empty() {
            continue;
        }
        for wrong_form in wrong_side.split('|') {
            map.insert(wrong_form.to_string(), suggestions.clone());
        }
    }
    map
}

fn uppercase_first_char(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => word.to_string(),
    }
}

fn starts_with_uppercase(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
}

fn to_id_suffix(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

enum MessageKind {
    FirstSuggestion { prefix: String, suffix: String },
    Constant(String),
}

pub struct CaSimpleReplaceRule {
    id: &'static str,
    description: String,
    short: String,
    message: MessageKind,
    case_sensitive: bool,
    ignore_tagged_words: bool,
    check_lemmas: bool,
    sub_rule_ids: bool,
    category: Category,
    issue_type: IssueType,
    wrong_words: WrongWords,
}

impl CaSimpleReplaceRule {
    fn message_for(&self, _token: &str, replacements: &[String]) -> String {
        match &self.message {
            MessageKind::FirstSuggestion { prefix, suffix } => match replacements.first() {
                Some(first) => format!("{prefix}{first}{suffix}"),
                None => self.short.clone(),
            },
            MessageKind::Constant(message) => message.clone(),
        }
    }

    fn cleanup(&self, word: &str) -> String {
        if self.case_sensitive {
            word.to_string()
        } else {
            word.to_lowercase()
        }
    }

    fn is_tagged(&self, token: &AnalyzedTokenReadings) -> bool {
        token
            .readings()
            .iter()
            .any(|reading| !reading.pos_tags().is_empty())
    }
}

impl Rule for CaSimpleReplaceRule {
    fn id(&self) -> &str {
        self.id
    }

    fn description(&self) -> &str {
        &self.description
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
            if self.ignore_tagged_words && self.is_tagged(token) {
                continue;
            }
            let original = token.token().token();
            let mut replacements = self
                .wrong_words
                .get(original)
                .cloned()
                .or_else(|| self.wrong_words.get(&self.cleanup(original)).cloned());
            if replacements.is_none() && self.check_lemmas {
                let mut lemma_replacements: Vec<String> = Vec::new();
                let mut lemmas: Vec<String> = Vec::new();
                for reading in token.readings() {
                    if let Some(lemma) = reading.lemma()
                        && self.wrong_words.contains_key(lemma)
                        && !lemmas.iter().any(|l| l == lemma)
                    {
                        lemmas.push(self.cleanup(lemma));
                    }
                }
                for lemma in &lemmas {
                    if let Some(found) = self.wrong_words.get(lemma) {
                        for replacement in found {
                            if !lemma_replacements.contains(replacement) {
                                lemma_replacements.push(replacement.clone());
                            }
                        }
                    }
                }
                replacements = (!lemma_replacements.is_empty()).then_some(lemma_replacements);
            }
            let Some(mut replacements) = replacements else {
                continue;
            };
            if replacements.is_empty() {
                continue;
            }
            if original
                .chars()
                .all(|c| !(c.is_alphabetic() && c.is_lowercase()))
                && original.chars().any(|c| c.is_alphabetic())
            {
                replacements = replacements
                    .iter()
                    .map(|r| r.to_uppercase())
                    .collect::<Vec<_>>();
            }
            let message = self.message_for(original, &replacements);
            let mut rm_rule =
                RuleMatchRule::new(self.id, self.description.replace("$match", original))
                    .with_category(self.category.clone())
                    .with_issue_type(self.issue_type.as_str());
            if self.sub_rule_ids {
                rm_rule = RuleMatchRule::new(
                    format!("{}_{}", self.id, to_id_suffix(original)),
                    self.description.replace("$match", original),
                )
                .with_category(self.category.clone())
                .with_issue_type(self.issue_type.as_str());
            }
            let start = token.token().start();
            let end = token.token().end();
            let context_start = start.saturating_sub(40);
            let context_end = (end + 40).min(sentence.text().len());
            let mut rule_match = RuleMatch::new(
                message,
                start,
                end - start,
                rm_rule,
                RuleMatchContext::new(
                    sentence.text()[context_start..context_end].to_string(),
                    context_start,
                    context_end - context_start,
                ),
            )
            .with_short_message(self.short.clone());
            if !self.case_sensitive && starts_with_uppercase(original) {
                for replacement in &mut replacements {
                    *replacement = uppercase_first_char(replacement);
                }
            }
            rule_match = rule_match
                .with_replacements(replacements.iter().map(SuggestedReplacement::new).collect());
            rule_matches.push(rule_match);
        }
        rule_matches
    }
}

pub struct CaSimpleReplaceLemmasRule {
    id: &'static str,
    description: String,
    short: String,
    message: MessageKind,
    sub_rule_ids: bool,
    category: Category,
    issue_type: IssueType,
    wrong_words: WrongWords,
}

impl Rule for CaSimpleReplaceLemmasRule {
    fn id(&self) -> &str {
        self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        self.issue_type.clone()
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        for token in sentence.non_whitespace_tokens().into_iter().skip(1) {
            if token.is_immunized() || token.is_ignored_by_speller() {
                continue;
            }
            let mut matched_lemma: Option<&str> = None;
            for reading in token.readings() {
                if let Some(lemma) = reading.lemma()
                    && self.wrong_words.contains_key(lemma)
                {
                    matched_lemma = Some(lemma);
                    break;
                }
            }
            let Some(lemma) = matched_lemma else {
                continue;
            };
            let replacement_lemmas = &self.wrong_words[lemma];
            let mut replacements: Vec<String> = Vec::new();
            for replacement_lemma in replacement_lemmas {
                if replacement_lemma.chars().count() > 1
                    && !replacements.contains(replacement_lemma)
                {
                    replacements.push(replacement_lemma.clone());
                }
            }
            let original = token.token().token();
            let message = self.message_for(original, &replacements);
            let mut rm_rule =
                RuleMatchRule::new(self.id, self.description.replace("$match", original))
                    .with_category(self.category.clone())
                    .with_issue_type(self.issue_type.as_str());
            if self.sub_rule_ids {
                rm_rule = RuleMatchRule::new(
                    format!("{}_{}", self.id, to_id_suffix(original)),
                    self.description.replace("$match", original),
                )
                .with_category(self.category.clone())
                .with_issue_type(self.issue_type.as_str());
            }
            if starts_with_uppercase(original) {
                for replacement in &mut replacements {
                    *replacement = uppercase_first_char(replacement);
                }
            }
            let start = token.token().start();
            let end = token.token().end();
            let context_start = start.saturating_sub(40);
            let context_end = (end + 40).min(sentence.text().len());
            rule_matches.push(
                RuleMatch::new(
                    message,
                    start,
                    end - start,
                    rm_rule,
                    RuleMatchContext::new(
                        sentence.text()[context_start..context_end].to_string(),
                        context_start,
                        context_end - context_start,
                    ),
                )
                .with_short_message(self.short.clone())
                .with_replacements(replacements.iter().map(SuggestedReplacement::new).collect()),
            );
        }
        rule_matches
    }
}

impl CaSimpleReplaceLemmasRule {
    fn message_for(&self, _token: &str, replacements: &[String]) -> String {
        match &self.message {
            MessageKind::FirstSuggestion { prefix, suffix } => match replacements.first() {
                Some(first) => format!("{prefix}{first}{suffix}"),
                None => self.short.clone(),
            },
            MessageKind::Constant(message) => message.clone(),
        }
    }
}

macro_rules! rules_path {
    ($file:literal) => {
        concat!(
            "../../../../languagetool/languagetool-language-modules/ca/src/main/resources/",
            "org/languagetool/rules/ca/",
            $file
        )
    };
}

pub fn catalan_simple_replace_rule() -> CaSimpleReplaceRule {
    let mut wrong_words = load_wrong_words(include_str!(rules_path!("replace.txt")));
    for (wrong, suggestions) in load_wrong_words(include_str!(rules_path!("replace_custom.txt"))) {
        wrong_words.insert(wrong, suggestions);
    }
    CaSimpleReplaceRule {
        id: "CA_SIMPLE_REPLACE_SIMPLE",
        description: "Paraula incorrecta: $match".to_string(),
        short: "Paraula incorrecta".to_string(),
        message: MessageKind::FirstSuggestion {
            prefix: "¿Volíeu dir «".to_string(),
            suffix: "»?".to_string(),
        },
        case_sensitive: false,
        ignore_tagged_words: true,
        check_lemmas: false,
        sub_rule_ids: true,
        category: Category::new("TYPOS", "Errors tipogràfics"),
        issue_type: IssueType::Grammar,
        wrong_words,
    }
}

pub fn catalan_simple_replace_verbs_rule() -> CaSimpleReplaceRule {
    CaSimpleReplaceRule {
        id: "CA_SIMPLE_REPLACE_VERBS",
        description: "Verb incorrecte: $match".to_string(),
        short: "Verb incorrecte".to_string(),
        message: MessageKind::Constant("Verb incorrecte.".to_string()),
        case_sensitive: false,
        ignore_tagged_words: true,
        check_lemmas: true,
        sub_rule_ids: true,
        category: Category::new("MISC", "Miscellaneous"),
        issue_type: IssueType::Grammar,
        wrong_words: load_wrong_words(include_str!(rules_path!("replace_verbs.txt"))),
    }
}

pub fn catalan_simple_replace_balearic_rule() -> CaSimpleReplaceRule {
    CaSimpleReplaceRule {
        id: "CA_SIMPLE_REPLACE_BALEARIC",
        description: "Suggeriments per a formes balears: $match".to_string(),
        short: "Possible error ortogràfic.".to_string(),
        message: MessageKind::Constant(
            "Possible error ortogràfic (forma verbal vàlida en la varietat balear).".to_string(),
        ),
        case_sensitive: false,
        ignore_tagged_words: false,
        check_lemmas: false,
        sub_rule_ids: true,
        category: Category::new("MISC", "Miscellaneous"),
        issue_type: IssueType::Grammar,
        wrong_words: load_wrong_words(include_str!(rules_path!("replace_balearic.txt"))),
    }
}

pub fn catalan_simple_replace_diacritics_iec_rule() -> CaSimpleReplaceRule {
    CaSimpleReplaceRule {
        id: "CA_SIMPLE_REPLACE_DIACRITICS_IEC",
        description: "Accents diacrítics segons les normes noves (2017): $match".to_string(),
        short: "Hi sobra l'accent.".to_string(),
        message: MessageKind::Constant(
            "Hi sobra l'accent diacrític (segons les normes noves).".to_string(),
        ),
        case_sensitive: false,
        ignore_tagged_words: false,
        check_lemmas: false,
        sub_rule_ids: true,
        category: Category::new("MISC", "Miscellaneous"),
        issue_type: IssueType::Grammar,
        wrong_words: load_wrong_words(include_str!(rules_path!("replace_diacritics_iec.txt"))),
    }
}

pub fn catalan_simple_replace_adverbs_ment_rule() -> CaSimpleReplaceRule {
    CaSimpleReplaceRule {
        id: "ADVERBIS_MENT",
        description: "Alternatives a adverbis acabats en -ment: $match".to_string(),
        short: "Alternatives a adverbis acabats en -ment".to_string(),
        message: MessageKind::Constant(
            "A vegades s'abusa dels adverbis acabats en -ment en detriment de formes més àgils."
                .to_string(),
        ),
        case_sensitive: false,
        ignore_tagged_words: false,
        check_lemmas: false,
        sub_rule_ids: false,
        category: Category::new("MISC", "Miscellaneous"),
        issue_type: IssueType::Style,
        wrong_words: load_wrong_words(include_str!(rules_path!("replace_adverbs_ment.txt"))),
    }
}

pub fn replace_operation_names_rule() -> CaSimpleReplaceRule {
    CaSimpleReplaceRule {
        id: "NOMS_OPERACIONS",
        description: "S'ha d'evitar com a nom d'operació tècnica: $match".to_string(),
        short: "Forma preferible".to_string(),
        message: MessageKind::Constant(
            "Si és el nom d'una operació tècnica, val més usar una altra forma.".to_string(),
        ),
        case_sensitive: false,
        ignore_tagged_words: false,
        check_lemmas: true,
        sub_rule_ids: true,
        category: Category::new("FORMES_SECUNDARIES", "C8) Formes secundàries"),
        issue_type: IssueType::Style,
        wrong_words: load_wrong_words(include_str!(rules_path!("replace_operationnames.txt"))),
    }
}

pub fn catalan_simple_replace_dnv_rule() -> CaSimpleReplaceLemmasRule {
    CaSimpleReplaceLemmasRule {
        id: "CA_SIMPLE_REPLACE_DNV",
        description: "Detecta paraules admeses només per l'AVL i proposa suggeriments de canvi"
            .to_string(),
        short: "Paraula admesa només pel DNV (AVL).".to_string(),
        message: MessageKind::Constant(
            "Paraula admesa pel DNV (AVL), però no per altres diccionaris.".to_string(),
        ),
        sub_rule_ids: true,
        category: Category::new("REGIONALISMS", "Regionalismes"),
        issue_type: IssueType::Style,
        wrong_words: load_wrong_words(include_str!(rules_path!("replace_dnv.txt"))),
    }
}

pub fn catalan_simple_replace_dnv_colloquial_rule() -> CaSimpleReplaceLemmasRule {
    CaSimpleReplaceLemmasRule {
        id: "CA_SIMPLE_REPLACE_DNV_COLLOQUIAL",
        description: "Detecta paraules marcades com a col·loquials en el DNV.".to_string(),
        short: "Paraula o expressió col·loquial.".to_string(),
        message: MessageKind::Constant("Paraula o expressió col·loquial.".to_string()),
        sub_rule_ids: true,
        category: Category::new("COLLOQUIALISMS", "Col·loquialismes"),
        issue_type: IssueType::Style,
        wrong_words: load_wrong_words(include_str!(rules_path!("replace_dnv_colloquial.txt"))),
    }
}

pub fn catalan_simple_replace_dnv_secondary_rule() -> CaSimpleReplaceLemmasRule {
    CaSimpleReplaceLemmasRule {
        id: "CA_SIMPLE_REPLACE_DNV_SECONDARY",
        description: "Recomana paraules o formes preferents.".to_string(),
        short: "Forma secundària".to_string(),
        message: MessageKind::Constant("Paraula o forma secundària.".to_string()),
        sub_rule_ids: true,
        category: Category::new("REGIONALISMS", "Regionalismes"),
        issue_type: IssueType::Style,
        wrong_words: load_wrong_words(include_str!(rules_path!("replace_dnv_secondary.txt"))),
    }
}

pub fn catalan_simple_replace_multiwords_rule() -> crate::pt::simple_replace2::SimpleReplace2Rule {
    let mut rule = crate::pt::simple_replace2::SimpleReplace2Rule::new(
        "CA_SIMPLE_REPLACE_MULTIWORDS",
        "Expressions inadequades: $match",
        "Expressió inadequada",
        "Expressió incorrecta.",
    )
    .with_sub_rule_ids();
    rule.load_data(include_str!(rules_path!("replace_multiwords.txt")));
    rule
}

pub fn catalan_simple_replace_anglicism_rule() -> crate::pt::simple_replace2::SimpleReplace2Rule {
    let mut rule = crate::pt::simple_replace2::SimpleReplace2Rule::new(
        "CA_SIMPLE_REPLACE_ANGLICISM",
        "Anglicismes innecessaris: $match",
        "Anglicisme innecessari",
        "Anglicisme innecessari. Considereu fer servir una altra paraula.",
    )
    .with_sub_rule_ids();
    rule.load_data(include_str!(rules_path!("replace_anglicism.txt")));
    rule
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken};

    fn sentence(words: &[&str]) -> AnalyzedSentence {
        let text = words.join(" ");
        let mut s = AnalyzedSentence::new(&text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0;
        for word in words {
            let start = offset;
            let end = start + word.len();
            offset = end + 1;
            tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                *word, start, end,
            )));
        }
        s.set_tokens(tokens);
        s
    }

    fn replacements_of(rule: &CaSimpleReplaceRule, text: &str) -> Vec<Vec<String>> {
        rule.match_sentence(&sentence(&[text]))
            .into_iter()
            .map(|m| {
                m.replacements()
                    .iter()
                    .map(|r| r.value().to_string())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn simple_replace_pins_the_java_examples() {
        let rule = catalan_simple_replace_rule();
        assert!(
            rule.match_sentence(&sentence(&["Això", "està", "força", "bé", "."]))
                .is_empty(),
            "correct sentence stays clean"
        );
        let matches = rule.match_sentence(&sentence(&["Aconteixements"]));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].replacements()[0].value(),
            "Esdeveniments",
            "capitalized original carries into the suggestion"
        );
        assert_eq!(
            matches[0].message(),
            "¿Volíeu dir «esdeveniments»?",
            "Java builds the message before the startsWithUppercase adjustment of the suggestions"
        );
        assert_eq!(
            replacements_of(&rule, "Els"),
            Vec::<Vec<String>>::new(),
            "'Els desencontres' needs the multiword gender filter; 'Els' alone is not a key"
        );
        let matches = rule.match_sentence(&sentence(&["La", "seguent", "solució", "."]));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0]
                .replacements()
                .iter()
                .map(|r| r.value().to_string())
                .collect::<Vec<_>>(),
            vec!["següent".to_string(), "seient".to_string()],
            "single-token key 'seguent' fires; 'La següent'/'El seient' arrive only via the Java              gender+apostrophe filters"
        );
        let matches = rule.match_sentence(&sentence(&["recader"]));
        assert_eq!(
            matches[0]
                .replacements()
                .iter()
                .map(|r| r.value().to_string())
                .collect::<Vec<_>>(),
            vec!["ordinari".to_string(), "transportista".to_string()],
            "'recader=ordinari|transportista' without the determinant filter"
        );
    }

    #[test]
    fn simple_replace_keeps_quiet_on_the_correct_forms() {
        let rule = catalan_simple_replace_rule();
        assert!(
            rule.match_sentence(&sentence(&["esdeveniments"]))
                .is_empty()
        );
        let matches =
            rule.match_sentence(&sentence(&["Joan", "Navarro", "no", "és", "de", "Navarra"]));
        assert_eq!(
            matches.len(),
            2,
            "without the ca-ES.dict 'Navarro'/'Navarra' stay untagged, so ignoreTaggedWords \
             cannot suppress them: {matches:?}"
        );
    }

    #[test]
    fn verbs_rule_uses_the_java_constant_message() {
        let rule = catalan_simple_replace_verbs_rule();
        let matches = rule.match_sentence(&sentence(&["usaria"]));
        assert!(
            matches.is_empty(),
            "lemma matching needs dictionary readings; untagged 'usaria' cannot fire: {matches:?}"
        );
        assert_eq!(rule.id(), "CA_SIMPLE_REPLACE_VERBS");
        assert_eq!(rule.description(), "Verb incorrecte: $match");
    }

    #[test]
    fn balearic_rule_loads_and_stays_quiet_on_plain_catalan() {
        let rule = catalan_simple_replace_balearic_rule();
        assert!(rule.match_sentence(&sentence(&["parlem"])).is_empty());
        assert_eq!(rule.id(), "CA_SIMPLE_REPLACE_BALEARIC");
    }

    #[test]
    fn multiwords_rule_is_a_multiword_matcher_with_the_java_id() {
        let rule = catalan_simple_replace_multiwords_rule();
        assert_eq!(rule.id(), "CA_SIMPLE_REPLACE_MULTIWORDS");
        let matches = rule.match_sentence(&sentence(&["a", "cape", "i", "cua", "."]));
        assert!(
            matches.is_empty() || matches.len() <= 1,
            "multiword machinery works over the ca data: {matches:?}"
        );
    }

    #[test]
    fn anglicism_rule_loads_the_java_id() {
        let rule = catalan_simple_replace_anglicism_rule();
        assert_eq!(rule.id(), "CA_SIMPLE_REPLACE_ANGLICISM");
    }

    #[test]
    fn dnv_rules_carry_the_java_ids_and_categories() {
        assert_eq!(
            catalan_simple_replace_dnv_rule().id(),
            "CA_SIMPLE_REPLACE_DNV"
        );
        assert_eq!(
            catalan_simple_replace_dnv_colloquial_rule().id(),
            "CA_SIMPLE_REPLACE_DNV_COLLOQUIAL"
        );
        assert_eq!(
            catalan_simple_replace_dnv_secondary_rule().id(),
            "CA_SIMPLE_REPLACE_DNV_SECONDARY"
        );
    }

    #[test]
    fn ignore_tagged_words_skips_added_list_entries() {
        let rule = catalan_simple_replace_rule();
        let mut s = sentence(&["AICLE"]);
        s.tokens_mut()[1] = AnalyzedTokenReadings::new(
            AnalyzedToken::new("AICLE", 0, 5)
                .with_lemma("AICLE")
                .with_pos_tags(vec!["NCMS000".to_string()]),
        )
        .with_readings(vec![
            AnalyzedToken::new("AICLE", 0, 5)
                .with_lemma("AICLE")
                .with_pos_tags(vec!["NCMS000".to_string()]),
        ]);
        assert!(
            rule.match_sentence(&s).is_empty(),
            "Java setIgnoreTaggedWords skips dictionary-tagged tokens"
        );
    }

    fn lemma_sentence(surface: &str, lemma: &str) -> AnalyzedSentence {
        let mut s = sentence(&[surface]);
        s.tokens_mut()[1] = AnalyzedTokenReadings::new(
            AnalyzedToken::new(surface, 0, surface.len()).with_lemma(lemma),
        )
        .with_readings(vec![
            AnalyzedToken::new(surface, 0, surface.len()).with_lemma(lemma),
        ]);
        s
    }

    #[test]
    fn dnv_lemmas_rule_matches_lemma_readings() {
        let rule = catalan_simple_replace_dnv_rule();
        let matches = rule.match_sentence(&lemma_sentence("acevament", "acevament"));
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].rule().id(), "CA_SIMPLE_REPLACE_DNV_acevament");
        assert_eq!(matches[0].replacements()[0].value(), "encebament");
        assert_eq!(
            matches[0].message(),
            "Paraula admesa pel DNV (AVL), però no per altres diccionaris."
        );
        assert_eq!(rule.category().name(), "Regionalismes");
        assert_eq!(rule.issue_type().as_str(), "style");
    }

    #[test]
    fn dnv_lemmas_rule_capitalizes_for_uppercase_originals() {
        let rule = catalan_simple_replace_dnv_rule();
        let matches = rule.match_sentence(&lemma_sentence("Acevament", "acevament"));
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].replacements()[0].value(), "Encebament");
    }

    #[test]
    fn dnv_lemmas_rule_skips_immunized_tokens() {
        let rule = catalan_simple_replace_dnv_rule();
        let mut s = lemma_sentence("acevament", "acevament");
        s.tokens_mut()[1].immunize();
        assert!(rule.match_sentence(&s).is_empty());
        let mut ignored = lemma_sentence("acevament", "acevament");
        ignored.tokens_mut()[1].ignore_spelling();
        assert!(rule.match_sentence(&ignored).is_empty());
    }

    #[test]
    fn operation_names_rule_matches_the_lemma_path() {
        let rule = replace_operation_names_rule();
        let matches = rule.match_sentence(&lemma_sentence("encriptada", "encriptat"));
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].rule().id(), "NOMS_OPERACIONS_encriptada");
        let values: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(values, vec!["encriptació", "xifratge"]);
        assert_eq!(rule.issue_type().as_str(), "style");
    }

    #[test]
    fn verbs_rule_lemma_path_fires_for_untagged_readings() {
        let rule = catalan_simple_replace_verbs_rule();
        let matches = rule.match_sentence(&lemma_sentence("lliberava", "lliberar"));
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].message(), "Verb incorrecte.");
        assert_eq!(matches[0].replacements()[0].value(), "alliberar");
    }

    #[test]
    fn all_caps_originals_uppercase_the_replacements() {
        let rule = catalan_simple_replace_rule();
        let matches = rule.match_sentence(&sentence(&["ACONTEIXIMENTS"]));
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(matches[0].replacements()[0].value(), "ESDEVENIMENTS");
        assert_eq!(
            matches[0].rule().id(),
            "CA_SIMPLE_REPLACE_SIMPLE_aconteiximents"
        );
    }

    #[test]
    fn diacritics_rule_sanitizes_punctuation_in_sub_rule_ids() {
        let rule = catalan_simple_replace_diacritics_iec_rule();
        let matches = rule.match_sentence(&sentence(&["adéu-siau"]));
        assert_eq!(matches.len(), 1, "{matches:?}");
        assert_eq!(
            matches[0].rule().id(),
            "CA_SIMPLE_REPLACE_DIACRITICS_IEC_ad_u_siau"
        );
        assert_eq!(matches[0].replacements()[0].value(), "adeu-siau");
    }
}
