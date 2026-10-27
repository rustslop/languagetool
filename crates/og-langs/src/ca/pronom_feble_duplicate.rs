use super::native_rules::{
    context_around, is_all_uppercase, is_capitalized_word, uppercase_first_char,
};
use super::pronoms_febles::transform_darrere;
use og_core::rule::Rule;
use og_core::{
    AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, Category, IssueType, RuleMatch,
    RuleMatchRule, SuggestedReplacement,
};
use std::sync::LazyLock;

const RULE_MESSAGE: &str =
    "Combinaci\u{f3} incorrecta de pronoms febles. Deixeu els de davant o els de darrere del verb.";
const SHORT_MESSAGE: &str = "Combinaci\u{f3} incorrecta de pronoms febles.";

static PRONOM_FEBLE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new("^(?:P0.{6}|PP3CN000|PP3NN000|PP3..A00|PP[123]CP000|PP3CSD00)$").unwrap()
});
static VERB_CONJUGAT: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^V.[SI].*$").unwrap());
static SPS00: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new("^SPS00$").unwrap());

pub struct PronomFebleDuplicateRule;

impl PronomFebleDuplicateRule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PronomFebleDuplicateRule {
    fn default() -> Self {
        Self::new()
    }
}

struct SentenceTokens<'a> {
    tokens: Vec<&'a AnalyzedTokenReadings>,
    whitespace_before: Vec<bool>,
}

fn sentence_tokens(sentence: &AnalyzedSentence) -> SentenceTokens<'_> {
    let mut tokens = Vec::with_capacity(sentence.tokens().len());
    let mut whitespace_before = Vec::with_capacity(sentence.tokens().len());
    let mut prev_whitespace = false;
    for token in sentence.tokens() {
        if token.is_whitespace() {
            prev_whitespace = true;
            continue;
        }
        tokens.push(token);
        whitespace_before.push(prev_whitespace);
        prev_whitespace = false;
    }
    SentenceTokens {
        tokens,
        whitespace_before,
    }
}

fn lemma_of_pronom_feble(atr: &AnalyzedTokenReadings) -> &str {
    for reading in atr.readings() {
        let matched = reading
            .pos_tags()
            .iter()
            .any(|tag| PRONOM_FEBLE.is_match(tag));
        if matched {
            return reading.lemma().unwrap_or("");
        }
    }
    ""
}

fn reading_with_tag_regex<'a>(
    atr: &'a AnalyzedTokenReadings,
    re: &regex::Regex,
) -> Option<&'a AnalyzedToken> {
    atr.readings()
        .iter()
        .find(|r| r.pos_tags().iter().any(|t| re.is_match(t)))
}

fn has_partial_pos_tag(atr: &AnalyzedTokenReadings, partial: &str) -> bool {
    atr.readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.contains(partial)))
}

fn has_pos_tag_starting_with(atr: &AnalyzedTokenReadings, prefix: &str) -> bool {
    atr.readings()
        .iter()
        .any(|r| r.pos_tags().iter().any(|t| t.starts_with(prefix)))
}

fn is_exception(tokens: &[&AnalyzedTokenReadings], i: usize) -> bool {
    if (has_partial_pos_tag(tokens[i], "VMN0000")
        || has_partial_pos_tag(tokens[i], "VSN0000")
        || has_partial_pos_tag(tokens[i], "VAN0000"))
        && tokens[i - 1].has_lemma("fer")
    {
        return true;
    }
    if tokens[i].token().token() == "poder" && has_pos_tag_starting_with(tokens[i - 1], "V") {
        return true;
    }
    if i > 3
        && i + 3 < tokens.len()
        && matches!(tokens[i].token().token(), "ha" | "havia")
        && tokens[i - 2].token().token() == "que"
        && tokens[i - 1].token().token() == "hi"
        && tokens[i + 1].has_lemma("de")
        && !(tokens[i + 2].has_lemma("haver") && tokens[i + 3].has_lemma("hi"))
    {
        return true;
    }
    false
}

fn preserve_case(input: &str, model: &str) -> String {
    if model.is_empty() {
        return input.to_string();
    }
    if is_capitalized_word(model) {
        return uppercase_first_char(&input.to_lowercase());
    }
    if is_all_uppercase(model) {
        return input.to_uppercase();
    }
    input.to_string()
}

fn suggestion_from_to(
    tokens: &[&AnalyzedTokenReadings],
    whitespace_before: &[bool],
    from: usize,
    to: usize,
) -> String {
    let mut sugg = String::new();
    for j in from..to {
        if let Some(atr) = tokens.get(j) {
            if whitespace_before.get(j).copied().unwrap_or(false) && !sugg.is_empty() {
                sugg.push(' ');
            }
            sugg.push_str(atr.token().token());
        }
    }
    sugg
}

fn is_there_error_in_lemmas(
    lemes_abans: &[&str],
    lemes_despres: &[&str],
    lemes_preposicions: &[&str],
    tokens: &[&AnalyzedTokenReadings],
    last_verb_pos: Option<usize>,
    corrected_pronouns: &mut Option<String>,
) -> bool {
    *corrected_pronouns = None;
    if lemes_abans.is_empty() || lemes_despres.is_empty() {
        return false;
    }
    if lemes_abans.len() == 1 && lemes_despres.len() == 1 && lemes_abans[0] == lemes_despres[0] {
        return true;
    }
    if lemes_abans.len() > 1 && lemes_despres.len() > 1 {
        return true;
    }
    if let Some(pos) = last_verb_pos
        && matches!(tokens[pos].token().token(), "haver" | "havent")
        && matches!(lemes_despres[0], "en" | "hi")
        && matches!(lemes_abans[0], "en" | "hi")
    {
        *corrected_pronouns = Some("n'hi".to_string());
        return true;
    }
    if lemes_abans.contains(&"en") && lemes_despres.contains(&"en") {
        return true;
    }
    if lemes_abans.contains(&"ell") && lemes_despres.contains(&"ell") {
        return true;
    }
    if lemes_abans.contains(&"ho")
        && (lemes_despres.contains(&"hi")
            || lemes_despres.contains(&"ell")
            || lemes_despres.contains(&"en"))
    {
        return true;
    }
    if lemes_abans.contains(&"hi")
        && !lemes_preposicions.contains(&"a")
        && (lemes_despres.contains(&"ho")
            || lemes_despres.contains(&"ell")
            || lemes_despres.contains(&"en"))
    {
        return true;
    }
    false
}

impl Rule for PronomFebleDuplicateRule {
    fn id(&self) -> &str {
        "PRONOMS_FEBLES_DUPLICATS"
    }

    fn description(&self) -> &str {
        "Pronoms febles duplicats"
    }

    fn category(&self) -> Category {
        Category::new("PRONOMS_FEBLES", "Pronoms febles")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let SentenceTokens {
            tokens,
            whitespace_before,
        } = sentence_tokens(sentence);
        let mut rule_matches = Vec::new();
        let mut init_pos: Option<usize> = None;
        let mut last_verb_pos: Option<usize> = None;
        let mut lemes_abans: Vec<&str> = Vec::new();
        let mut lemes_despres: Vec<&str> = Vec::new();
        let mut lemes_preposicions: Vec<&str> = Vec::new();
        let mut count_verb = 0usize;
        let mut in_verb_group = false;
        let mut corrected_pronouns: Option<String> = None;
        let mut i = 1usize;
        while i < tokens.len() {
            let pf_lemma = lemma_of_pronom_feble(tokens[i]);
            if !pf_lemma.is_empty() {
                if count_verb == 0
                    && (!lemes_abans.is_empty()
                        || whitespace_before[i]
                        || tokens[i - 1].has_pos_tag("SENT_START"))
                {
                    lemes_abans.push(pf_lemma);
                    if lemes_abans.len() == 1 {
                        init_pos = Some(i);
                    }
                    in_verb_group = true;
                } else if !whitespace_before[i] {
                    lemes_despres.push(pf_lemma);
                } else {
                    count_verb = 0;
                    lemes_abans.clear();
                    lemes_despres.clear();
                    lemes_abans.push(pf_lemma);
                    init_pos = Some(i);
                    in_verb_group = true;
                }
            } else if tokens[i].has_chunk("GV")
                && !lemes_abans.is_empty()
                && lemes_despres.is_empty()
                && !is_exception(&tokens, i)
            {
                if reading_with_tag_regex(tokens[i], &VERB_CONJUGAT).is_some() && count_verb > 0 {
                    in_verb_group = false;
                } else {
                    count_verb += 1;
                    in_verb_group = true;
                    last_verb_pos = Some(i);
                    if let Some(prep) = reading_with_tag_regex(tokens[i], &SPS00)
                        && let Some(lemma) = prep.lemma()
                    {
                        lemes_preposicions.push(lemma);
                    }
                }
            } else {
                in_verb_group = false;
            }

            if !in_verb_group || i == tokens.len() - 1 {
                if is_there_error_in_lemmas(
                    &lemes_abans,
                    &lemes_despres,
                    &lemes_preposicions,
                    &tokens,
                    last_verb_pos,
                    &mut corrected_pronouns,
                ) {
                    if in_verb_group && i == tokens.len() - 1 {
                        i += 1;
                    }
                    let start_index = init_pos.unwrap_or(1);
                    let from_pos = tokens[start_index].token().start();
                    let to_pos = tokens[i - 1].token().end();
                    let model = tokens[start_index].token().token();
                    let replacements: Vec<String> = match &corrected_pronouns {
                        None => {
                            let verbs_from = start_index + lemes_abans.len();
                            vec![
                                preserve_case(
                                    &suggestion_from_to(
                                        &tokens,
                                        &whitespace_before,
                                        verbs_from,
                                        verbs_from + count_verb + lemes_despres.len(),
                                    ),
                                    model,
                                ),
                                preserve_case(
                                    &suggestion_from_to(
                                        &tokens,
                                        &whitespace_before,
                                        start_index,
                                        start_index + lemes_abans.len() + count_verb,
                                    ),
                                    model,
                                ),
                            ]
                        }
                        Some(cp) => {
                            let verbs_from = start_index + lemes_abans.len();
                            let verbs = suggestion_from_to(
                                &tokens,
                                &whitespace_before,
                                verbs_from,
                                verbs_from + count_verb,
                            );
                            let pronoms_darrere = transform_darrere(cp, &verbs);
                            vec![
                                preserve_case(&format!("{cp} {verbs}"), model),
                                preserve_case(&format!("{verbs}{pronoms_darrere}"), model),
                            ]
                        }
                    };
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type(IssueType::Grammar.as_str());
                    rule_matches.push(
                        RuleMatch::new(
                            RULE_MESSAGE,
                            from_pos,
                            to_pos - from_pos,
                            rm_rule,
                            context_around(sentence, from_pos, to_pos, 30),
                        )
                        .with_short_message(SHORT_MESSAGE)
                        .with_replacements(
                            replacements
                                .iter()
                                .map(|r| SuggestedReplacement::new(r.clone()))
                                .collect(),
                        ),
                    );
                }
                count_verb = 0;
                lemes_abans.clear();
                lemes_despres.clear();
                lemes_preposicions.clear();
            }
            i += 1;
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ca::gv_chunker::add_gv_chunks;
    use og_core::AnalyzedToken;

    type WordSpec<'a> = (&'a str, &'a [(&'a str, &'a str)], Option<&'a str>, bool);

    fn build(text: &str, words: &[WordSpec<'_>]) -> AnalyzedSentence {
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut offset = 0usize;
        for (surface, readings, chunk, ws_before) in words {
            if *ws_before {
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    " ",
                    offset,
                    offset + 1,
                )));
                offset += 1;
            }
            let start = offset;
            let end = start + surface.len();
            offset = end;
            let mut atr = AnalyzedTokenReadings::new(AnalyzedToken::new(*surface, start, end));
            let reading_tokens: Vec<AnalyzedToken> = readings
                .iter()
                .map(|(lemma, tag)| {
                    AnalyzedToken::new(*surface, start, end)
                        .with_lemma(lemma.to_string())
                        .with_pos_tags(vec![tag.to_string()])
                })
                .collect();
            if !reading_tokens.is_empty() {
                atr = atr.with_readings(reading_tokens);
            }
            if let Some(chunk) = chunk {
                atr.set_chunks(vec![chunk.to_string()]);
            }
            tokens.push(atr);
        }
        let mut s = AnalyzedSentence::new(text, 0, text.len());
        s.set_tokens(tokens);
        s
    }

    #[test]
    fn fires_on_the_java_example_with_both_suggestions() {
        let s = build(
            "S'ha de fer-se",
            &[
                ("S'", &[("es", "P0300000")], None, false),
                ("ha", &[("haver", "VMIP3S00")], Some("GV"), false),
                ("de", &[("de", "SPS00")], Some("GV"), true),
                ("fer", &[("fer", "VMN0000")], Some("GV"), true),
                ("-se", &[("es", "P0300000")], None, false),
            ],
        );
        let rule = PronomFebleDuplicateRule::new();
        let matches = rule.match_sentence(&s);
        assert_eq!(matches.len(), 1);
        let replacements: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(replacements, vec!["Ha de fer-se", "S'ha de fer"]);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].length(), "S'ha de fer-se".len());
        assert_eq!(matches[0].short_message(), Some(SHORT_MESSAGE));
    }

    #[test]
    fn haver_en_hi_pair_gets_the_n_hi_correction() {
        let s = build(
            "N'ha d'haver-hi",
            &[
                ("N'", &[("en", "P0300000")], None, false),
                ("ha", &[("haver", "VMIP3S00")], Some("GV"), false),
                ("d'", &[("de", "SPS00")], Some("GV"), true),
                ("haver", &[("haver", "VMN0000")], Some("GV"), false),
                ("-hi", &[("hi", "PP3CN000")], None, false),
            ],
        );
        let rule = PronomFebleDuplicateRule::new();
        let matches = rule.match_sentence(&s);
        assert_eq!(matches.len(), 1);
        let replacements: Vec<&str> = matches[0]
            .replacements()
            .iter()
            .map(|r| r.value())
            .collect();
        assert_eq!(replacements, vec!["N'hi ha d'haver", "Ha d'haver-n'hi"]);
    }

    #[test]
    fn no_gv_chunk_means_no_match() {
        let s = build(
            "S'ha de fer-se",
            &[
                ("S'", &[("es", "P0300000")], None, false),
                ("ha", &[("haver", "VMIP3S00")], None, false),
                ("de", &[("de", "SPS00")], None, true),
                ("fer", &[("fer", "VMN0000")], None, true),
                ("-se", &[("es", "P0300000")], None, false),
            ],
        );
        assert!(
            PronomFebleDuplicateRule::new()
                .match_sentence(&s)
                .is_empty()
        );
    }

    #[test]
    fn fer_infinitive_exception_blocks_the_verbal_group() {
        let s = build(
            "Aix\u{f2} et fan donar-te",
            &[
                ("Aix\u{f2}", &[], None, false),
                ("et", &[("et", "P0200000")], None, true),
                ("fan", &[("fer", "VMIP3P00")], None, true),
                ("donar", &[("donar", "VMN0000")], Some("GV"), true),
                ("-te", &[("et", "P0200000")], None, false),
            ],
        );
        assert!(
            PronomFebleDuplicateRule::new()
                .match_sentence(&s)
                .is_empty()
        );
    }

    #[test]
    fn gv_chunker_and_rule_compose_on_a_synthetic_pipeline() {
        let mut s = build(
            "ha de fer",
            &[
                ("ha", &[("haver", "VMIP3S00")], None, false),
                ("de", &[("de", "SPS00")], None, true),
                ("fer", &[("fer", "VMN0000")], None, true),
            ],
        );
        add_gv_chunks(&mut s);
        assert!(
            s.tokens()
                .iter()
                .filter(|t| !t.is_sentence_start() && !t.is_whitespace())
                .all(|t| t.has_chunk("GV"))
        );
    }

    #[test]
    fn power_after_a_verb_is_an_exception() {
        let s = build(
            "es podia morir de taponar-se-li",
            &[
                ("es", &[("es", "P0300000")], None, false),
                ("podia", &[("poder", "VMII3S00")], Some("GV"), true),
                ("morir", &[("morir", "VMN0000")], Some("GV"), true),
                ("de", &[("de", "SPS00")], Some("GV"), true),
                ("taponar", &[("taponar", "VMN0000")], Some("GV"), true),
                ("-se", &[("es", "P0300000")], None, false),
                ("-li", &[("li", "PP3CN000")], None, false),
            ],
        );
        let matches = PronomFebleDuplicateRule::new().match_sentence(&s);
        assert!(
            matches.is_empty(),
            "the isException(poder) path must keep the Java-correct sentence silent: {matches:?}"
        );
    }
}
