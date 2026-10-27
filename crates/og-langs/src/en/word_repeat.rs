use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};

const EXCEPTIONS: &[&str] = &[
    "had", "that", "her", "aye", "blah", "mau", "uh", "paw", "cha", "yum", "wop", "woop", "fnarr",
    "fnar", "ha", "omg", "boo", "tick", "twinkle", "ta", "la", "x", "hi", "ho", "heh", "jay",
    "walla", "sri", "hey", "hah", "oh", "ouh", "chop", "ring", "beep", "bleep", "yeah", "gout",
    "quack", "meow", "squawk", "whoa", "si", "honk", "brum", "chi", "santorio", "lapu", "chow",
    "shh", "yummy", "boom", "bye", "ah", "aah", "bang", "woof", "wink", "yes", "tsk", "hush",
    "ding", "choo", "miu", "tuk", "yadda", "doo", "sapiens", "tse", "no",
];

pub struct EnglishWordRepeatRule {
    id: String,
    category: Category,
}

impl EnglishWordRepeatRule {
    pub fn new() -> Self {
        Self {
            id: "ENGLISH_WORD_REPEAT_RULE".to_string(),
            category: Category::new("MISC", "Miscellaneous"),
        }
    }

    fn should_ignore(&self, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        if position == 0 || position >= tokens.len() {
            return false;
        }

        let prev_token = tokens[position - 1].token().token();
        let curr_token = tokens[position].token().token();
        let curr_lower = curr_token.to_lowercase();

        if matches!(curr_lower.as_str(), "did" | "do" | "does") && position + 1 < tokens.len() {
            let next = tokens[position + 1].token().token().to_lowercase();
            if next == "n't" {
                return true;
            }
        }

        if curr_lower == "her" && position >= 2 && position + 1 < tokens.len() {
            let has_verb_before = Self::has_pos_tag(
                tokens,
                position - 2,
                &["VB", "VBP", "VBZ", "VBG", "VBD", "VBN"],
            );
            let has_noun_after = Self::has_pos_tag(tokens, position + 1, &["NN", "NNS", "NNP"]);
            if has_verb_before && has_noun_after {
                return true;
            }
        }

        if curr_lower == "had"
            && position >= 2
            && Self::has_pos_tag(tokens, position - 2, &["PRP", "NN"])
        {
            return true;
        }

        if curr_lower == "that"
            && position + 1 < tokens.len()
            && Self::has_pos_tag(
                tokens,
                position + 1,
                &["MD", "NN", "PRP$", "JJ", "VBZ", "VBD"],
            )
        {
            return true;
        }

        if curr_lower == "can" && Self::has_pos_tag(tokens, position.wrapping_sub(1), &["NN"]) {
            return true;
        }

        if position + 1 < tokens.len() {
            let next_lower = tokens[position + 1].token().token().to_lowercase();
            match curr_lower.as_str() {
                "hip" if next_lower == "hooray" => return true,
                "bam" if next_lower == "bigelow" => return true,
                "wild" if next_lower == "west" => return true,
                "far" if next_lower == "away" => return true,
                "so" if next_lower == "much" || next_lower == "many" => return true,
                "in" => {
                    if position >= 2 {
                        let prev_prev = tokens[position - 2].token().token().to_lowercase();
                        if prev_prev.starts_with("log") || prev_prev.starts_with("sign") {
                            return true;
                        }
                    }
                    if position >= 3 {
                        let prev_prev_prev = tokens[position - 3].token().token().to_lowercase();
                        if prev_prev_prev.starts_with("log") || prev_prev_prev.starts_with("sign") {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }

        if curr_lower == "a" && position >= 2 && tokens[position - 2].token().token() == "." {
            return true;
        }
        if curr_lower == "on" && position >= 2 && tokens[position - 2].token().token() == "." {
            return true;
        }

        if curr_token.len() == 1
            && curr_token
                .chars()
                .next()
                .map(|c| c.is_alphabetic())
                .unwrap_or(false)
            && position >= 2
            && position + 1 < tokens.len()
        {
            let is_single_alpha = |t: &AnalyzedTokenReadings| {
                let s = t.token().token();
                s.len() == 1 && s.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false)
            };
            if is_single_alpha(tokens[position - 2]) && is_single_alpha(tokens[position + 1]) {
                return true;
            }
        }

        if position >= 2 && tokens[position - 2].token().token().to_lowercase() == curr_lower {
            return true;
        }
        if position + 1 < tokens.len()
            && tokens[position + 1].token().token().to_lowercase() == curr_lower
        {
            return true;
        }

        if EXCEPTIONS.contains(&curr_lower.as_str()) && prev_token.to_lowercase() == curr_lower {
            return true;
        }

        if curr_lower == "s" && position >= 2 {
            let prev_prev = tokens[position - 2].token().token();
            if prev_prev == "'" || prev_prev == "\u{2019}" {
                return true;
            }
        }

        if curr_lower == "bora" && prev_token.to_lowercase() == "bora" {
            return true;
        }

        if curr_lower == "wait" && position == 2 {
            return true;
        }

        if curr_lower == "may" && (prev_token == "may" || prev_token == "May") {
            return true;
        }

        if curr_lower == "will" && (prev_token == "will" || prev_token == "Will") {
            return true;
        }

        false
    }

    fn has_pos_tag(tokens: &[&AnalyzedTokenReadings], pos: usize, tags: &[&str]) -> bool {
        if pos >= tokens.len() {
            return false;
        }
        let token = tokens[pos];
        let readings = token.readings();
        for reading in readings {
            for pos_tag in reading.pos_tags() {
                for tag in tags {
                    if pos_tag == *tag || pos_tag.starts_with(tag) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

impl Default for EnglishWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for EnglishWordRepeatRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        "Word repetition"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Redundancy
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut matches = Vec::new();
        let tokens = sentence.non_whitespace_tokens();

        for i in 1..tokens.len() {
            let prev = tokens[i - 1];
            let curr = tokens[i];

            let prev_text = prev.token().token().to_lowercase();
            let curr_text = curr.token().token().to_lowercase();

            if prev_text == curr_text
                && !prev.is_whitespace()
                && !curr.is_whitespace()
                && curr_text.chars().any(|c| c.is_alphabetic())
            {
                if self.should_ignore(&tokens, i) {
                    continue;
                }

                let start = curr.token().start();
                let end = curr.token().end();
                let context_start = start.saturating_sub(20);
                let context_end = std::cmp::min(end + 20, sentence.text().len());
                let context_text = sentence.text()[context_start..context_end].to_string();

                let rule = RuleMatchRule::new(&self.id, self.description())
                    .with_category(self.category.clone())
                    .with_issue_type(IssueType::Redundancy.as_str());

                matches.push(
                    RuleMatch::new(
                        format!("Possible word repetition: '{}'", curr.token().token()),
                        start,
                        end - start,
                        rule,
                        RuleMatchContext::new(
                            context_text,
                            context_start,
                            context_end - context_start,
                        ),
                    )
                    .with_sentence(sentence.text().to_string())
                    .with_replacements(vec![SuggestedReplacement::new(curr.token().token())]),
                );
            }
        }

        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedSentence, AnalyzedToken};

    fn make_sentence(text: &str) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut byte_pos = 0;
        for part in text.split(|c: char| c.is_whitespace()) {
            if part.is_empty() {
                continue;
            }
            if let Some(offset) = text[byte_pos..].find(part) {
                let start = byte_pos + offset;
                let end = start + part.len();
                tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
                    part, start, end,
                )));
                byte_pos = end;
            }
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn test_word_repeat_detection() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = make_sentence("This is is a test");
        let matches = rule.match_sentence(&sentence);
        assert!(!matches.is_empty(), "Should detect 'is is'");
    }

    #[test]
    fn test_word_repeat_exception_haha() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = make_sentence("ha ha that's funny");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "Should ignore 'ha ha'");
    }

    #[test]
    fn test_word_repeat_exception_blah() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = make_sentence("blah blah blah");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "Should ignore 'blah blah blah'");
    }

    #[test]
    fn test_word_repeat_no_repeat() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = make_sentence("This is a test");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_word_repeat_exception_bye() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = make_sentence("bye bye now");
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "Should ignore 'bye bye'");
    }

    fn sentence_with_readings(text: &str, tags: &[(&str, &str)]) -> AnalyzedSentence {
        let mut sentence = make_sentence(text);
        let tokens = sentence.tokens_mut();
        for token in tokens.iter_mut() {
            if let Some((_, tag)) = tags.iter().find(|(word, _)| *word == token.token().token()) {
                let mut reading = AnalyzedToken::new(
                    token.token().token(),
                    token.token().start(),
                    token.token().end(),
                )
                .with_pos_tags(vec![tag.to_string()]);
                reading = reading.with_lemma(token.token().token());
                *token = AnalyzedTokenReadings::new(reading.clone()).with_readings(vec![reading]);
            }
        }
        sentence
    }

    #[test]
    fn ignores_repeated_auxiliary_before_nt() {
        let rule = EnglishWordRepeatRule::default();
        let sentence = make_sentence("She did did n't care");
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn ignores_her_her_between_verb_and_noun() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = sentence_with_readings(
            "Please pass her her phone",
            &[("pass", "VB"), ("phone", "NN")],
        );
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn falls_back_to_the_exception_list_without_clause_readings() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = make_sentence("I think that that thing works");
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn ignores_had_had_after_pronoun() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = sentence_with_readings("If I had had time", &[("I", "PRP")]);
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn ignores_that_that_before_clause_verb() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = sentence_with_readings(
            "I think that that is fine",
            &[("is", "VBZ"), ("think", "VBP")],
        );
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn ignores_can_can_with_noun_reading() {
        let rule = EnglishWordRepeatRule::new();
        let sentence =
            sentence_with_readings("The can can hold water", &[("can", "NN"), ("hold", "VB")]);
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn ignores_doubled_pairs_from_the_java_rule() {
        let rule = EnglishWordRepeatRule::new();
        for text in [
            "hip hip hooray",
            "bam bam bigelow",
            "the wild wild west",
            "far far away",
            "with so so much luck",
            "with so so many options",
        ] {
            let sentence = make_sentence(text);
            assert!(
                rule.match_sentence(&sentence).is_empty(),
                "should ignore '{text}'"
            );
        }
    }

    #[test]
    fn ignores_log_and_sign_in_in() {
        let rule = EnglishWordRepeatRule::new();
        for text in [
            "log in in place",
            "logged them in in time",
            "sign in in person",
        ] {
            let sentence = make_sentence(text);
            assert!(
                rule.match_sentence(&sentence).is_empty(),
                "should ignore '{text}'"
            );
        }
    }

    #[test]
    fn ignores_repetition_after_abbreviation_dot() {
        let rule = EnglishWordRepeatRule::new();
        let a = make_sentence("a . k . a a team");
        assert!(rule.match_sentence(&a).is_empty());
        let on = make_sentence("contact E . ON on Instagram");
        assert!(rule.match_sentence(&on).is_empty());
    }

    #[test]
    fn ignores_spaced_single_letter_spelling() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = make_sentence("b a s i c a l l y");
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn ignores_triple_repetition() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = make_sentence("well well  well");
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn ignores_s_repetition_after_apostrophe() {
        let rule = EnglishWordRepeatRule::new();
        let sentence = make_sentence("It ' s s T");
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn ignores_bora_bora_and_wait_wait() {
        let rule = EnglishWordRepeatRule::new();
        let bora = make_sentence("Bora Bora island");
        assert!(rule.match_sentence(&bora).is_empty());
        let wait = make_sentence("please wait wait");
        assert!(rule.match_sentence(&wait).is_empty());
    }

    #[test]
    fn ignores_may_and_will_case_pairs() {
        let rule = EnglishWordRepeatRule::new();
        for text in [
            "may May come",
            "May may come",
            "will Will go",
            "Will will go",
        ] {
            let sentence = make_sentence(text);
            assert!(
                rule.match_sentence(&sentence).is_empty(),
                "should ignore '{text}'"
            );
        }
    }

    #[test]
    fn reports_redundancy_issue_type() {
        let rule = EnglishWordRepeatRule::new();
        assert_eq!(rule.issue_type(), IssueType::Redundancy);
        let sentence = make_sentence("This is is a test");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].offset(), 8);
        assert_eq!(matches[0].length(), 2);
        assert!(matches[0].message().contains("is"));
    }
}
