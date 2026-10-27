use std::collections::HashMap;
use std::path::Path;

use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use og_tagger::arabic_synthesizer::ArabicSynthesizer;
use og_tagger::arabic_tagger::{get_flag, set_flag};

pub struct ArabicTransVerbRule {
    wrong_words: HashMap<String, Vec<String>>,
    synthesizer: ArabicSynthesizer,
}

pub(crate) fn load_wrong_words(data: &str) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for raw_line in data.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(pos) = line.find('#') {
            line = line[..pos].trim();
        }
        let (conf_pair, _message) = match line.split_once('\t') {
            Some((pair, message)) => (pair, Some(message)),
            None => (line, None),
        };
        let Some((wrong_forms, suggestion)) = conf_pair.split_once('=') else {
            continue;
        };
        let suggestions: Vec<String> = suggestion.split('|').map(String::from).collect();
        for wrong_form in wrong_forms.split('|') {
            map.insert(wrong_form.trim().to_lowercase(), suggestions.clone());
        }
    }
    map
}

impl ArabicTransVerbRule {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        Self {
            wrong_words: load_wrong_words(include_str!(
                "../../../../languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/rules/ar/verb_trans_to_untrans2.txt"
            )),
            synthesizer: ArabicSynthesizer::from_resource_dir(resource_dir),
        }
    }

    fn is_attached_transitive_verb(&self, token: &AnalyzedTokenReadings) -> bool {
        for reading in token.readings() {
            if !reading.pos_tags().is_empty()
                && reading
                    .lemma()
                    .is_some_and(|lemma| self.wrong_words.contains_key(lemma))
            {
                return true;
            }
        }
        false
    }

    fn prepositions_for(&self, token: &AnalyzedTokenReadings) -> &[String] {
        const EMPTY: &[String] = &[];
        for reading in token.readings() {
            if !reading.pos_tags().is_empty()
                && let Some(lemma) = reading.lemma()
                && let Some(prepositions) = self.wrong_words.get(lemma)
            {
                return prepositions;
            }
        }
        EMPTY
    }

    fn is_right_preposition(
        &self,
        next_token: &AnalyzedTokenReadings,
        prepositions: &[String],
    ) -> bool {
        next_token
            .readings()
            .first()
            .and_then(|r| r.lemma())
            .is_some_and(|lemma| prepositions.iter().any(|p| p == lemma))
    }

    fn generate_new_form(&self, word: &str, postag: &str, flag: char) -> String {
        let mut new_postag = set_flag(postag, "PRONOUN", flag);
        if flag != '-' {
            new_postag = set_flag(&new_postag, "OPTION", 'D');
        }
        self.synthesizer
            .synthesize(word, &new_postag)
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    fn generate_unattached_new_form(&self, token: &AnalyzedTokenReadings) -> String {
        let reading = token.readings().first();
        let lemma = reading.and_then(|r| r.lemma()).unwrap_or("");
        let postag = reading
            .and_then(|r| r.pos_tags().first())
            .map(String::as_str)
            .unwrap_or("");
        self.generate_new_form(lemma, postag, '-')
    }

    fn generate_attached_new_form(
        &self,
        preposition_lemma: &str,
        prev_token: &AnalyzedTokenReadings,
    ) -> String {
        let postag = "PR-;---;---";
        let prev_postag = prev_token
            .readings()
            .first()
            .and_then(|r| r.pos_tags().first())
            .map(String::as_str)
            .unwrap_or("");
        let flag = get_flag(prev_postag, "PRONOUN");
        self.generate_new_form(preposition_lemma, postag, flag)
    }
}

impl Rule for ArabicTransVerbRule {
    fn id(&self) -> &str {
        "AR_VERB_TRANSITIVE_IINDIRECT"
    }

    fn description(&self) -> &str {
        "َTransitive verbs corrected to indirect transitive"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        if self.wrong_words.is_empty() {
            return rule_matches;
        }
        let mut prev_token_index = 0usize;
        for i in 1..tokens.len() {
            let token = tokens[i];
            if prev_token_index > 0 {
                let prev = tokens[prev_token_index];
                let prev_token_str = prev.token().token();
                let attached = self.is_attached_transitive_verb(prev);
                let prepositions = self.prepositions_for(prev);
                let right_preposition = self.is_right_preposition(token, prepositions);
                if attached && !right_preposition {
                    let verb = self.generate_unattached_new_form(prev);
                    let new_preposition = prepositions.first().map(String::as_str).unwrap_or("");
                    let preposition = self.generate_attached_new_form(new_preposition, prev);
                    let replacement = format!("{verb} {preposition}");
                    let message = format!(
                        "قل <suggestion>{replacement}</suggestion> بدلا من '{prev_token_str}' لأنّ الفعل  متعد بحرف  ."
                    );
                    let start = prev.token().start();
                    let end = prev.token().end();
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str());
                    let ctx_start = (start - sentence.start()).saturating_sub(40);
                    let ctx_start = {
                        let mut cs = ctx_start;
                        while cs > 0 && !sentence.text().is_char_boundary(cs) {
                            cs += 1;
                        }
                        cs
                    };
                    let ctx_end = std::cmp::min(
                        token.token().end() - sentence.start() + 40,
                        sentence.text().len(),
                    );
                    let ctx_end = {
                        let mut ce = ctx_end;
                        let text_len = sentence.text().len();
                        while ce < text_len && !sentence.text().is_char_boundary(ce) {
                            ce += 1;
                        }
                        ce
                    };
                    let context_text = sentence.text()[ctx_start..ctx_end].to_string();
                    rule_matches.push(
                        RuleMatch::new(
                            &message,
                            start,
                            end - start,
                            rm_rule,
                            RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                        )
                        .with_replacements(vec![SuggestedReplacement::new(&replacement)])
                        .with_sentence(sentence.text().to_string()),
                    );
                }
            }
            if self.is_attached_transitive_verb(token) {
                prev_token_index = i;
            } else {
                prev_token_index = 0;
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings};

    #[test]
    fn loads_java_verb_list() {
        let words = load_wrong_words("# comment\n أَفَاضَ=في\n أفاض=في\n أَقَادَ=إلى\n");
        assert_eq!(words.len(), 3);
        assert_eq!(
            words.get("أَفَاضَ").map(|v| v.as_slice()),
            Some(vec!["في".to_string()].as_slice())
        );
    }

    fn resource_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(
            "/home/agent/opengrammar-rs/languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/resource/ar",
        )
    }

    #[test]
    fn flags_transitive_verb_without_proper_preposition() {
        let rule = ArabicTransVerbRule::from_resource_dir(&resource_dir());
        let text = "أفاض الرجل";
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.push(AnalyzedTokenReadings::new(
            AnalyzedToken::new("أفاض", 0, 8)
                .with_lemma("أفاض".to_string())
                .with_pos_tags(vec!["V---------;---".to_string()]),
        ));
        tokens.push(AnalyzedTokenReadings::new(AnalyzedToken::new(
            "الرجل",
            9,
            15,
        )));
        sentence.set_tokens(tokens);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "expected one match on the verb token");
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].length(), 8);
        assert!(matches[0].message().contains("<suggestion>"));
        assert!(matches[0].message().contains("بدلا من 'أفاض'"));
    }

    #[test]
    fn accepts_verb_followed_by_proper_preposition() {
        let rule = ArabicTransVerbRule::from_resource_dir(&resource_dir());
        let text = "أفاض في";
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        tokens.push(AnalyzedTokenReadings::new(
            AnalyzedToken::new("أفاض", 0, 8)
                .with_lemma("أفاض".to_string())
                .with_pos_tags(vec!["V---------;---".to_string()]),
        ));
        tokens.push(AnalyzedTokenReadings::new(
            AnalyzedToken::new("في", 9, 11)
                .with_lemma("في".to_string())
                .with_pos_tags(vec!["PR-;---;---".to_string()]),
        ));
        sentence.set_tokens(tokens);
        let matches = rule.match_sentence(&sentence);
        assert!(matches.is_empty(), "preposition must suppress the match");
    }
}
