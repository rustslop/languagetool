use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, TextLevelRule,
};

const ADD_ADVERBS: [&str; 5] = [
    "Asimismo",
    "Igualmente",
    "Además",
    "También",
    "Adicionalmente",
];
const CONTRAST_CONJ: [&str; 3] = ["Pero", "Empero", "Mas"];
const EMPHASIS_ADVERBS: [&str; 4] = [
    "Obviamente",
    "Claramente",
    "Absolutamente",
    "Definitivamente",
];
const EXPLAIN_ADVERBS: [&str; 4] = [
    "Específicamente",
    "Concretamente",
    "Particularmente",
    "Precisamente",
];
const PERSONAL_PRONOUNS: [&str; 12] = [
    "yo",
    "tú",
    "él",
    "ella",
    "nosostros",
    "nosotras",
    "vosotros",
    "vosotras",
    "ellos",
    "ellas",
    "usted",
    "ustedes",
];
const ADD_EXPRESSIONS: [&str; 1] = ["Así mismo"];
const CONTRAST_EXPRESSIONS: [&str; 3] = ["Aun así", "Por otra parte", "Sin embargo"];
const EXCEPTIONS_START: [&str; 12] = [
    "el",
    "la",
    "los",
    "las",
    "punto",
    "artículo",
    "módulo",
    "parte",
    "sesión",
    "unidad",
    "tema",
    "n",
];
const SENTENCE_EXCEPTIONS: [&str; 4] = ["por un", "por otro", "por otra", "por una"];

fn is_adverb(atr: &AnalyzedTokenReadings) -> bool {
    if atr.has_pos_tag("RG") || atr.has_pos_tag("LOC_ADV") {
        return true;
    }
    let tok = atr.token().token();
    ADD_ADVERBS.contains(&tok)
        || CONTRAST_CONJ.contains(&tok)
        || EMPHASIS_ADVERBS.contains(&tok)
        || EXPLAIN_ADVERBS.contains(&tok)
}

fn suggestions_for(token: &str) -> Vec<String> {
    let lower_tok = token.to_lowercase();
    if PERSONAL_PRONOUNS.contains(&lower_tok.as_str()) {
        return vec![
            format!("Además, {lower_tok}"),
            format!("Igualmente, {lower_tok}"),
            format!("No solo eso, sino que {lower_tok}"),
        ];
    }
    if ADD_ADVERBS.contains(&token) {
        let mut out: Vec<String> = ADD_ADVERBS
            .iter()
            .filter(|adv| **adv != token)
            .map(|s| s.to_string())
            .collect();
        out.extend(ADD_EXPRESSIONS.iter().map(|s| s.to_string()));
        return out;
    }
    if CONTRAST_CONJ.contains(&token) {
        return CONTRAST_EXPRESSIONS.iter().map(|s| s.to_string()).collect();
    }
    if EMPHASIS_ADVERBS.contains(&token) {
        return EMPHASIS_ADVERBS
            .iter()
            .filter(|adv| **adv != token)
            .map(|s| s.to_string())
            .collect();
    }
    if EXPLAIN_ADVERBS.contains(&token) {
        return EXPLAIN_ADVERBS
            .iter()
            .filter(|adv| **adv != token)
            .map(|s| s.to_string())
            .collect();
    }
    Vec::new()
}

pub struct SpanishWordRepeatBeginningRule;

impl SpanishWordRepeatBeginningRule {
    pub fn new() -> Self {
        Self
    }

    fn is_exception(&self, token: &str) -> bool {
        matches!(token, ":" | "–" | "-" | "✔️" | "➡️" | "—" | "⭐️" | "⚠️")
            || token.chars().next().is_some_and(|c| c.is_numeric())
            || EXCEPTIONS_START.contains(&token.to_lowercase().as_str())
    }

    fn is_sentence_exception(&self, sentence: &AnalyzedSentence) -> bool {
        let s = sentence.text().to_lowercase();
        SENTENCE_EXCEPTIONS.iter().any(|e| s.starts_with(e))
    }
}

impl Default for SpanishWordRepeatBeginningRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for SpanishWordRepeatBeginningRule {
    fn id(&self) -> &str {
        "SPANISH_WORD_REPEAT_BEGINNING_RULE"
    }

    fn description(&self) -> &str {
        "Dos frases consecutivas empiezan con el mismo adverbio o pronombre"
    }

    fn is_picky(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("REPETITIONS_STYLE", "Repeticiones")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut last_token = String::new();
        let mut before_last_token = String::new();
        let mut matches = Vec::new();
        let mut pos = 0usize;
        let mut prev_sentence_ended_like_sentence = false;
        for sentence in sentences {
            if self.is_sentence_exception(sentence) {
                prev_sentence_ended_like_sentence = false;
                pos += sentence.text().len();
                continue;
            }
            let tokens = sentence.non_whitespace_tokens();
            let mut token = String::new();
            if tokens.len() > 3 {
                let analyzed_token = tokens[1];
                token = analyzed_token.token().token().to_string();
                let is_word = token.chars().count() != 1
                    || token.chars().next().is_some_and(char::is_alphabetic);
                let tok2 = tokens[2].token().token();
                let tok3 = tokens[3].token().token();
                if is_word
                    && last_token == token
                    && !self.is_exception(&token)
                    && !self.is_exception(tok2)
                    && !self.is_exception(tok3)
                    && prev_sentence_ended_like_sentence
                {
                    let short_msg = if is_adverb(analyzed_token) {
                        "Dos frases consecutivas empiezan con el mismo adverbio o pronombre."
                    } else if before_last_token == token {
                        "Tres frases seguidas empiezan con la misma palabra."
                    } else {
                        ""
                    };
                    if !short_msg.is_empty() {
                        let msg = format!(
                            "{short_msg} Puede usar un sinónimo para hacer más interesante el texto."
                        );
                        let start_pos = pos + analyzed_token.token().start();
                        let length = token.len();
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_picky(true)
                            .with_category(self.category())
                            .with_issue_type(self.issue_type().as_str());
                        let mut rm = RuleMatch::new(
                            msg,
                            start_pos,
                            length,
                            rm_rule,
                            RuleMatchContext::new(token.clone(), start_pos, length),
                        )
                        .with_sentence(sentence.text().to_string());
                        let suggestions = suggestions_for(&token);
                        if !suggestions.is_empty() {
                            rm = rm.with_replacements(
                                suggestions
                                    .iter()
                                    .map(|s| SuggestedReplacement::new(s.clone()))
                                    .collect(),
                            );
                        }
                        matches.push(rm);
                    }
                }
            }
            before_last_token = last_token.clone();
            last_token = token;
            pos += sentence.text().len();
            let trimmed = sentence.text().trim();
            prev_sentence_ended_like_sentence =
                trimmed.ends_with('.') || trimmed.ends_with('?') || trimmed.ends_with('!');
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::es::test_support::analyze_spanish_sentences;

    #[test]
    fn flags_repeated_asimismo_beginning() {
        let rule = SpanishWordRepeatBeginningRule::new();
        let text =
            "Asimismo, la calle es casi toda residencial. Asimismo, lleva el nombre de un poeta.";
        let sentences = analyze_spanish_sentences(text);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value().contains("Además"))
        );
    }

    #[test]
    fn no_flag_for_distinct_beginnings() {
        let rule = SpanishWordRepeatBeginningRule::new();
        let text = "La casa es grande. El coche es azul.";
        let sentences = analyze_spanish_sentences(text);
        let matches = rule.match_text(text, &sentences);
        assert!(matches.is_empty());
    }

    fn run_rule(text: &str) -> Vec<RuleMatch> {
        let rule = SpanishWordRepeatBeginningRule::default();
        let sentences = analyze_spanish_sentences(text);
        rule.match_text(text, &sentences)
    }

    fn values_of(matches: &[RuleMatch]) -> Vec<String> {
        matches[0]
            .replacements()
            .iter()
            .map(|r| r.value().to_string())
            .collect()
    }

    #[test]
    fn repeated_pero_suggests_the_contrast_expressions() {
        let matches = run_rule(
            "Pero gatos corren rápido. Pero perros corretean rápido. Pero niños corren rápido.",
        );
        assert_eq!(matches.len(), 2, "{matches:?}");
        assert_eq!(
            values_of(&matches),
            vec!["Aun así", "Por otra parte", "Sin embargo"]
        );
    }

    #[test]
    fn repeated_emphasis_adverbs_drop_the_repeated_one() {
        let matches = run_rule(
            "Obviamente gatos corren rápido. Obviamente perros corretean rápido. Obviamente niños corren rápido.",
        );
        assert_eq!(matches.len(), 2, "{matches:?}");
        assert_eq!(
            values_of(&matches),
            vec!["Claramente", "Absolutamente", "Definitivamente"]
        );
    }

    #[test]
    fn repeated_explain_adverbs_drop_the_repeated_one() {
        let matches = run_rule(
            "Específicamente gatos corren rápido. Específicamente perros corretean rápido. Específicamente niños corren rápido.",
        );
        assert_eq!(matches.len(), 2, "{matches:?}");
        assert_eq!(
            values_of(&matches),
            vec!["Concretamente", "Particularmente", "Precisamente"]
        );
    }

    #[test]
    fn repeated_pronouns_get_ademas_igualmente_y_no_solo_eso() {
        let matches = run_rule("Él es alto. Él es fuerte. Él es listo.");
        assert_eq!(
            matches.len(),
            1,
            "pronouns flag from the third sentence like plain words: {matches:?}"
        );
        assert_eq!(
            values_of(&matches),
            vec![
                "Además, él".to_string(),
                "Igualmente, él".to_string(),
                "No solo eso, sino que él".to_string(),
            ]
        );
    }

    #[test]
    fn repeated_plain_words_flag_only_from_the_third_sentence() {
        let two = run_rule("La casa es grande. La calle es azul.");
        assert!(two.is_empty(), "two plain repeats stay quiet: {two:?}");

        let three =
            run_rule("Casas grandes hay muchas. Casas azules hay pocas. Casas nuevas hay aquí.");
        assert_eq!(three.len(), 1, "{three:?}");
        assert!(
            three[0].message().contains("Tres frases seguidas"),
            "{}",
            three[0].message()
        );
        assert!(three[0].replacements().is_empty());
    }

    #[test]
    fn por_un_sentences_reset_the_repeat_tracking() {
        let matches = run_rule(
            "Por un lado, la casa es grande. Por un otro lado, la calle es azul. Por una parte, el cielo es azul.",
        );
        assert!(
            matches.is_empty(),
            "sentence-level exceptions skip: {matches:?}"
        );
    }

    #[test]
    fn exception_starters_and_numbers_stay_quiet() {
        let matches = run_rule("El gato duerme mucho. El perro come mucho. El pájaro canta mucho.");
        assert!(
            matches.is_empty(),
            "'el' is an exception starter: {matches:?}"
        );
        let numeric =
            run_rule("2023 fue un año seco. 2024 fue un año húmedo. 2025 fue un año frío.");
        assert!(
            numeric.is_empty(),
            "numeric starts are exceptions: {numeric:?}"
        );
    }

    #[test]
    fn single_letter_and_symbol_tokens_are_exceptions() {
        let rule = SpanishWordRepeatBeginningRule::new();
        assert!(rule.is_exception(":"));
        assert!(rule.is_exception("3"));
        assert!(rule.is_exception("Punto"));
        assert!(!rule.is_exception("casa"));
    }
}
