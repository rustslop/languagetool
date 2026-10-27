use std::collections::HashMap;
use std::path::Path;

use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule, rule::Rule,
};
use og_tagger::arabic_synthesizer::ArabicSynthesizer;
use og_tagger::arabic_tagger::ArabicTagger;

pub struct ArabicInflectedOneWordReplaceRule {
    wrong_words: HashMap<String, Suggestion>,
    tagger: ArabicTagger,
    synthesizer: ArabicSynthesizer,
}

pub(crate) struct Suggestion {
    words: Vec<String>,
    message: Option<String>,
}

pub(crate) fn load_suggestions(data: &str) -> HashMap<String, Suggestion> {
    let mut map: HashMap<String, Suggestion> = HashMap::new();
    for raw_line in data.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(pos) = line.find('#') {
            line = line[..pos].trim();
        }
        let (conf_pair, message) = match line.split_once('\t') {
            Some((pair, message)) => (pair, Some(message)),
            None => (line, None),
        };
        let Some((wrong_forms, suggestion)) = conf_pair.split_once('=') else {
            continue;
        };
        for wrong_form in wrong_forms.split('|') {
            map.insert(
                wrong_form.trim().to_lowercase(),
                Suggestion {
                    words: suggestion.split('|').map(String::from).collect(),
                    message: message.map(String::from),
                },
            );
        }
    }
    map
}

impl ArabicInflectedOneWordReplaceRule {
    pub fn from_resource_dir(resource_dir: &Path) -> Self {
        Self {
            wrong_words: load_suggestions(include_str!(
                "../../../../languagetool/languagetool-language-modules/ar/src/main/resources/org/languagetool/rules/ar/inflected_one_word.txt"
            )),
            tagger: ArabicTagger::from_resource_dir(resource_dir).with_new_style_pronoun_tag(),
            synthesizer: ArabicSynthesizer::from_resource_dir(resource_dir),
        }
    }
}

impl Rule for ArabicInflectedOneWordReplaceRule {
    fn id(&self) -> &str {
        "AR_INFLECTED_ONE_WORD"
    }

    fn description(&self) -> &str {
        "قاعدة تطابق الكلمات التي يجب تجنبها وتقترح تصويبا لها"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("MISC", "Miscellaneous")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Inconsistency
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let tokens = sentence.non_whitespace_tokens();
        let mut rule_matches = Vec::new();
        if self.wrong_words.is_empty() {
            return rule_matches;
        }
        for token in tokens {
            for word_tok in token.readings() {
                let Some(entry) = word_tok
                    .lemma()
                    .and_then(|lemma| self.wrong_words.get(lemma))
                else {
                    continue;
                };
                let sug_msg = entry.message.as_deref().unwrap_or("");
                let mut replacement = String::new();
                for proposition in &entry.words {
                    for form in
                        self.synthesizer
                            .inflect_lemma_like(&self.tagger, proposition, word_tok)
                    {
                        replacement.push_str(&format!("<suggestion>{form}</suggestion>&nbsp;"));
                    }
                }
                let message = format!(
                    "' الكلمة خاطئة {} ' ،{sug_msg}. استعمل  {replacement}",
                    token.token().token()
                );
                let short_message = format!("خطأ في استعمال كلمة:{sug_msg}");
                let start = token.token().start();
                let end = token.token().end();
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
                let ctx_end = std::cmp::min(end - sentence.start() + 40, sentence.text().len());
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
                    .with_short_message(short_message)
                    .with_sentence(sentence.text().to_string()),
                );
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_suggestions_with_messages() {
        let words = load_suggestions("# c\nأبحاث=بحوث\tبحوث أفصح وأصحّ\n");
        assert_eq!(words.len(), 1);
        let entry = words.get("أبحاث").unwrap();
        assert_eq!(entry.words, vec!["بحوث".to_string()]);
        assert_eq!(entry.message.as_deref(), Some("بحوث أفصح وأصحّ"));
    }
}
