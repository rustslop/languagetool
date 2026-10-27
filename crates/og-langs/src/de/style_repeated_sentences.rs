use super::helpers::{has_no_whitespace_before, has_pos_tag_starting_with, is_paragraph_end};
use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    rule::TextLevelRule,
};
use std::sync::LazyLock;

const MIN_REPEATED: usize = 3;
const MIN_WORDS: usize = 4;

static OPENING_QUOTES: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[\u{201C}\u{201E}\u{00BB}\u{00AB}\"]$").unwrap());
static ENDING_QUOTES: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[\u{201C}\u{201D}\u{00BB}\u{00AB}\"]$").unwrap());

pub struct StyleRepeatedVeryShortSentences {
    category: Category,
    min_words: usize,
    min_repeated: usize,
    exclude_direct_speech: bool,
}

impl StyleRepeatedVeryShortSentences {
    pub fn new() -> Self {
        Self {
            category: Category::new("CREATIVE_WRITING", "Stiltipps für kreatives Schreiben"),
            min_words: MIN_WORDS,
            min_repeated: MIN_REPEATED,
            exclude_direct_speech: true,
        }
    }
}

impl Default for StyleRepeatedVeryShortSentences {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for StyleRepeatedVeryShortSentences {
    fn id(&self) -> &str {
        "STYLE_REPEATED_SHORT_SENTENCES"
    }

    fn description(&self) -> &str {
        "Stakkato-Sätze"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        if sentences.len() < self.min_repeated {
            return rule_matches;
        }
        let mut n_repeated = 0usize;
        let mut start_pos: Vec<usize> = Vec::new();
        let mut end_pos: Vec<usize> = Vec::new();
        let mut repeated: Vec<&AnalyzedSentence> = Vec::new();
        let mut n_para: isize = -1;
        let mut begins_with_direct_speech = false;
        let mut ends_with_direct_speech = false;
        for (n, sentence) in sentences.iter().enumerate() {
            n_para += 1;
            let tokens = sentence.non_whitespace_tokens();
            if self.exclude_direct_speech {
                begins_with_direct_speech = ends_with_direct_speech;
                for (i, token) in tokens.iter().enumerate() {
                    let text = token.token().token();
                    if !begins_with_direct_speech
                        && OPENING_QUOTES.is_match(text)
                        && i < tokens.len() - 1
                        && has_no_whitespace_before(sentence, tokens[i + 1])
                    {
                        begins_with_direct_speech = true;
                        ends_with_direct_speech = true;
                    } else if begins_with_direct_speech
                        && ENDING_QUOTES.is_match(text)
                        && i > 1
                        && has_no_whitespace_before(sentence, token)
                    {
                        ends_with_direct_speech = false;
                    }
                }
            }
            let paragraph_end = is_paragraph_end(sentences, n);
            if !begins_with_direct_speech
                && (!paragraph_end || n_para > 0)
                && tokens.len() > 3
                && tokens.len() <= self.min_words + 2
            {
                repeated.push(sentence);
                start_pos.push(tokens[tokens.len() - 2].token().start());
                end_pos.push(tokens[tokens.len() - 1].token().end());
                n_repeated += 1;
            } else {
                if n_repeated >= self.min_repeated {
                    self.flush(&repeated, &start_pos, &end_pos, &mut rule_matches);
                }
                repeated.clear();
                start_pos.clear();
                end_pos.clear();
                n_repeated = 0;
            }
            if paragraph_end {
                n_para = -1;
            }
        }
        if n_repeated >= self.min_repeated {
            self.flush(&repeated, &start_pos, &end_pos, &mut rule_matches);
        }
        rule_matches
    }
}

impl StyleRepeatedVeryShortSentences {
    fn flush(
        &self,
        repeated: &[&AnalyzedSentence],
        start_pos: &[usize],
        end_pos: &[usize],
        rule_matches: &mut Vec<RuleMatch>,
    ) {
        for (i, sentence) in repeated.iter().enumerate() {
            let start = start_pos[i];
            let end = end_pos[i];
            let rel_start = start - sentence.start();
            let rel_end = end - sentence.start();
            let context_start = rel_start.saturating_sub(30);
            let context_end = std::cmp::min(rel_end + 30, sentence.text().len());
            let context_text = sentence.text()[context_start..context_end].to_string();
            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category.clone())
                .with_issue_type(IssueType::Style.as_str());
            rule_matches.push(
                RuleMatch::new(
                    self.description().to_string(),
                    start,
                    end - start,
                    rm_rule,
                    RuleMatchContext::new(context_text, context_start, context_end - context_start),
                )
                .with_sentence(sentence.text().to_string()),
            );
        }
    }
}

pub struct StyleRepeatedSentenceBeginning {
    category: Category,
}

impl StyleRepeatedSentenceBeginning {
    pub fn new() -> Self {
        Self {
            category: Category::new("CREATIVE_WRITING", "Stiltipps für kreatives Schreiben"),
        }
    }
}

impl Default for StyleRepeatedSentenceBeginning {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for StyleRepeatedSentenceBeginning {
    fn id(&self) -> &str {
        "STYLE_REPEATED_SENTENCE_BEGINNING"
    }

    fn description(&self) -> &str {
        "Subjekt als wiederholter Satzanfang"
    }

    fn category(&self) -> Category {
        self.category.clone()
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        const MIN_REPEATED: usize = 3;
        if sentences.len() < MIN_REPEATED {
            return rule_matches;
        }
        let mut start_pos: Vec<usize> = Vec::new();
        let mut end_pos: Vec<usize> = Vec::new();
        let mut repeated: Vec<&AnalyzedSentence> = Vec::new();
        let mut n_repeated = 0usize;
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            if tokens.len() <= 1 {
                if n_repeated >= MIN_REPEATED {
                    flush_beginning(self, &repeated, &start_pos, &end_pos, &mut rule_matches);
                }
                repeated.clear();
                start_pos.clear();
                end_pos.clear();
                n_repeated = 0;
                continue;
            }
            let first = tokens[1];
            if has_pos_tag_starting_with(first, "ART:DEF:NOM")
                || has_pos_tag_starting_with(first, "ART:IND:NOM")
            {
                let mut no_sub = true;
                for token in tokens.iter().skip(2) {
                    if has_pos_tag_starting_with(token, "VER") {
                        break;
                    }
                    if has_pos_tag_starting_with(token, "SUB") {
                        no_sub = false;
                        end_pos.push(token.token().end());
                        break;
                    }
                }
                if no_sub {
                    end_pos.push(first.token().end());
                }
                repeated.push(sentence);
                start_pos.push(first.token().start());
                n_repeated += 1;
            } else if has_pos_tag_starting_with(first, "PRO:PER:NOM") {
                repeated.push(sentence);
                start_pos.push(first.token().start());
                end_pos.push(first.token().end());
                n_repeated += 1;
            } else {
                if n_repeated >= MIN_REPEATED {
                    flush_beginning(self, &repeated, &start_pos, &end_pos, &mut rule_matches);
                }
                repeated.clear();
                start_pos.clear();
                end_pos.clear();
                n_repeated = 0;
            }
        }
        if n_repeated >= MIN_REPEATED {
            flush_beginning(self, &repeated, &start_pos, &end_pos, &mut rule_matches);
        }
        rule_matches
    }
}

fn flush_beginning(
    rule: &StyleRepeatedSentenceBeginning,
    repeated: &[&AnalyzedSentence],
    start_pos: &[usize],
    end_pos: &[usize],
    rule_matches: &mut Vec<RuleMatch>,
) {
    for (i, sentence) in repeated.iter().enumerate() {
        let start = start_pos[i];
        let end = end_pos[i];
        let rel_start = start - sentence.start();
        let rel_end = end - sentence.start();
        let context_start = rel_start.saturating_sub(30);
        let context_end = std::cmp::min(rel_end + 30, sentence.text().len());
        let context_text = sentence.text()[context_start..context_end].to_string();
        let rm_rule = RuleMatchRule::new(rule.id(), rule.description())
            .with_category(rule.category())
            .with_issue_type(IssueType::Style.as_str());
        rule_matches.push(
            RuleMatch::new(
                rule.description().to_string(),
                start,
                end.saturating_sub(start),
                rm_rule,
                RuleMatchContext::new(context_text, context_start, context_end - context_start),
            )
            .with_sentence(sentence.text().to_string()),
        );
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn detects_staccato_sentences() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let text = "Das Auto kam näher. Der Hund schlief. Die Reifen quietschten.";
        let mut request = og_core::CheckRequest::new(text, engine.language().clone());
        request.enabled_rules = vec!["STYLE_REPEATED_SHORT_SENTENCES".to_string()];
        let matches = engine.check_with_request(&request);
        assert_eq!(
            matches
                .matches
                .iter()
                .filter(|m| m.rule().id() == "STYLE_REPEATED_SHORT_SENTENCES")
                .count(),
            3
        );
    }

    #[test]
    fn detects_repeated_sentence_beginning() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let text =
            "Das Auto kam näher. Der Hund lief langsam über die Straße. Die Reifen quietschten.";
        let mut request = og_core::CheckRequest::new(text, engine.language().clone());
        request.enabled_rules = vec!["STYLE_REPEATED_SENTENCE_BEGINNING".to_string()];
        let matches = engine.check_with_request(&request);
        assert_eq!(
            matches
                .matches
                .iter()
                .filter(|m| m.rule().id() == "STYLE_REPEATED_SENTENCE_BEGINNING")
                .count(),
            3
        );
    }

    use super::{StyleRepeatedSentenceBeginning, StyleRepeatedVeryShortSentences};
    use crate::test_util::sentences_split_after;
    use og_core::rule::TextLevelRule as _;
    use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, IssueType};

    fn tagged_sentence(text: &str, start: usize, tags: &[(&str, &str)]) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, start, start + text.len());
        let mut tokens = vec![AnalyzedTokenReadings::new(
            AnalyzedToken::new("<S>", start, start).with_pos_tags(vec!["SENT_START".to_string()]),
        )];
        let mut pos = 0usize;
        for word in text.split_whitespace() {
            let token_start = start + pos + text[pos..].find(word).expect("ordered words");
            let token_end = token_start + word.len();
            let tag = tags
                .iter()
                .find(|(w, _)| *w == word)
                .map(|(_, t)| (*t).to_string());
            let mut reading = AnalyzedToken::new(word, token_start, token_end);
            if let Some(tag) = tag {
                reading = reading.with_pos_tags(vec![tag]);
            }
            tokens.push(AnalyzedTokenReadings::new(reading));
            pos = token_end - start;
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn fewer_than_three_sentences_return_early() {
        let short = StyleRepeatedVeryShortSentences::default();
        let beginning = StyleRepeatedSentenceBeginning::default();
        let sentences = sentences_split_after("Das Auto kam. Der Hund schlief.", &["."]);
        assert!(short.match_text("", &sentences).is_empty());
        assert!(beginning.match_text("", &sentences).is_empty());
    }

    #[test]
    fn flags_each_sentence_of_a_staccato_run() {
        let rule = StyleRepeatedVeryShortSentences::new();
        let text = "Das Auto kam näher. Der Hund schlief. Die Reifen quietschten.";
        let sentences = sentences_split_after(text, &[".", "."]);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].offset(), text.find("näher").unwrap());
        assert_eq!(matches[0].length(), "näher.".len());
        assert_eq!(matches[1].offset(), text.find("schlief").unwrap());
        assert_eq!(matches[2].offset(), text.find("quietschten").unwrap());
        assert_eq!(matches[0].rule().id(), "STYLE_REPEATED_SHORT_SENTENCES");
        assert_eq!(matches[0].message(), "Stakkato-Sätze");
    }

    #[test]
    fn a_long_sentence_breaks_the_run() {
        let rule = StyleRepeatedVeryShortSentences::default();
        let text = "Das Auto kam näher. Der Hund schlief. Die Reifen quietschten. Die ganz langsamen und sehr lauten Reifen quietschten laut im Regen. Der Wagen hielt. Die Tür öffnete. Der Motor starb.";
        let sentences = sentences_split_after(text, &[".", ".", ".", ".", ".", "."]);
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 6);
        assert_eq!(matches[0].offset(), text.find("näher").unwrap());
        assert_eq!(matches[2].offset(), text.find("quietschten").unwrap());
        assert_eq!(matches[3].offset(), text.rfind("hielt").unwrap());
        assert_eq!(matches[5].offset(), text.rfind("starb").unwrap());
    }

    #[test]
    fn paragraph_breaks_reset_the_run() {
        let rule = StyleRepeatedVeryShortSentences::default();
        let text = "Das Auto kam.\n\nDer Hund schlief. Die Reifen quietschten.";
        let sentences = sentences_split_after(text, &[".\n\n", "."]);
        assert!(rule.match_text(text, &sentences).is_empty());
    }

    #[test]
    fn sentences_inside_direct_speech_do_not_count() {
        let rule = StyleRepeatedVeryShortSentences::default();
        let mut speech = AnalyzedSentence::new("\u{201E}Hallo.\u{201C}", 0, 12);
        speech.set_tokens(vec![
            AnalyzedTokenReadings::new(
                AnalyzedToken::new("<S>", 0, 0).with_pos_tags(vec!["SENT_START".to_string()]),
            ),
            AnalyzedTokenReadings::new(AnalyzedToken::new("\u{201E}", 0, 3)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("Hallo", 3, 8)),
            AnalyzedTokenReadings::new(AnalyzedToken::new("\u{201C}", 8, 11)),
            AnalyzedTokenReadings::new(AnalyzedToken::new(".", 11, 12)),
        ]);
        let rest = sentences_split_after(
            "Der Hund schlief. Die Reifen quietschten. Der Motor starb.",
            &[".", "."],
        );
        let sentences = vec![speech, rest[0].clone(), rest[1].clone(), rest[2].clone()];
        let matches = rule.match_text("", &sentences);
        assert_eq!(
            matches.len(),
            3,
            "the quoted sentence must not join the staccato run"
        );
    }

    #[test]
    fn flags_repeated_article_and_pronoun_beginnings() {
        let rule = StyleRepeatedSentenceBeginning::new();
        let text = "Der Hund lief. Er schnupperte. Die Katze schrie.";
        let sentences = vec![
            tagged_sentence(
                "Der Hund lief.",
                0,
                &[
                    ("Der", "ART:DEF:NOM:SIN:MAS"),
                    ("Hund", "SUB:NOM:SIN:MAS"),
                    ("lief", "VER:3:SIN:PRA"),
                ],
            ),
            tagged_sentence(
                "Er schnupperte.",
                15,
                &[
                    ("Er", "PRO:PER:NOM:SIN:3"),
                    ("schnupperte", "VER:3:SIN:PRA"),
                ],
            ),
            tagged_sentence(
                "Die Katze schrie.",
                31,
                &[
                    ("Die", "ART:DEF:NOM:SIN:FEM"),
                    ("Katze", "SUB:NOM:SIN:FEM"),
                    ("schrie", "VER:3:SIN:PRA"),
                ],
            ),
        ];
        let matches = rule.match_text(text, &sentences);
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].offset(), 0);
        assert_eq!(matches[0].length(), "Der Hund".len());
        assert_eq!(matches[1].offset(), 15);
        assert_eq!(matches[1].length(), 2);
        assert_eq!(matches[2].offset(), 31);
        assert_eq!(matches[2].length(), "Die Katze".len());
        assert_eq!(matches[0].rule().id(), "STYLE_REPEATED_SENTENCE_BEGINNING");
        assert_eq!(matches[0].message(), "Subjekt als wiederholter Satzanfang");
    }

    #[test]
    fn articles_without_a_noun_end_at_the_article() {
        let rule = StyleRepeatedSentenceBeginning::default();
        let sentences = vec![
            tagged_sentence(
                "Die läuft.",
                0,
                &[("Die", "ART:DEF:NOM:SIN:FEM"), ("läuft", "VER:3:SIN:PRA")],
            ),
            tagged_sentence(
                "Die rennt.",
                10,
                &[("Die", "ART:DEF:NOM:SIN:FEM"), ("rennt", "VER:3:SIN:PRA")],
            ),
            tagged_sentence(
                "Die fliegt.",
                20,
                &[("Die", "ART:DEF:NOM:SIN:FEM"), ("fliegt", "VER:3:SIN:PRA")],
            ),
        ];
        let matches = rule.match_text("", &sentences);
        assert_eq!(matches.len(), 3);
        for m in &matches {
            assert_eq!(m.length(), "Die".len());
        }
    }

    #[test]
    fn other_beginnings_and_single_tokens_reset_the_run() {
        let rule = StyleRepeatedSentenceBeginning::default();
        let mixed = vec![
            tagged_sentence(
                "Der Hund lief.",
                0,
                &[
                    ("Der", "ART:DEF:NOM:SIN:MAS"),
                    ("Hund", "SUB:NOM:SIN:MAS"),
                    ("lief", "VER:3:SIN:PRA"),
                ],
            ),
            tagged_sentence(
                "Schnell war er.",
                15,
                &[("Schnell", "ADJ:PRD:GRU"), ("war", "VER:3:SIN:PRA")],
            ),
            tagged_sentence(
                "Der Hund schlief.",
                31,
                &[
                    ("Der", "ART:DEF:NOM:SIN:MAS"),
                    ("Hund", "SUB:NOM:SIN:MAS"),
                    ("schlief", "VER:3:SIN:PRA"),
                ],
            ),
        ];
        assert!(rule.match_text("", &mixed).is_empty());

        let interrupted = vec![
            tagged_sentence(
                "Der Hund lief.",
                0,
                &[
                    ("Der", "ART:DEF:NOM:SIN:MAS"),
                    ("Hund", "SUB:NOM:SIN:MAS"),
                    ("lief", "VER:3:SIN:PRA"),
                ],
            ),
            tagged_sentence("!", 15, &[]),
            tagged_sentence(
                "Er schnupperte.",
                17,
                &[
                    ("Er", "PRO:PER:NOM:SIN:3"),
                    ("schnupperte", "VER:3:SIN:PRA"),
                ],
            ),
            tagged_sentence(
                "Die Katze schrie.",
                33,
                &[
                    ("Die", "ART:DEF:NOM:SIN:FEM"),
                    ("Katze", "SUB:NOM:SIN:FEM"),
                    ("schrie", "VER:3:SIN:PRA"),
                ],
            ),
        ];
        assert!(rule.match_text("", &interrupted).is_empty());
    }

    #[test]
    fn metadata_is_creative_writing_style_off_by_default() {
        for id in [
            StyleRepeatedVeryShortSentences::default().id(),
            StyleRepeatedSentenceBeginning::default().id(),
        ] {
            assert!(id.starts_with("STYLE_REPEATED_"));
        }
        let short = StyleRepeatedVeryShortSentences::default();
        assert_eq!(short.description(), "Stakkato-Sätze");
        assert!(!short.is_default_on());
        assert_eq!(short.category().id().as_str(), "CREATIVE_WRITING");
        assert_eq!(short.issue_type(), IssueType::Style);
    }
}
