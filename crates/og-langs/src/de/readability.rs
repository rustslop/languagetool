use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    rule::TextLevelRule,
};
use std::sync::LazyLock;

const MARK_WORDS: usize = 3;
const MIN_WORDS: usize = 10;
const LEVEL: i32 = 3;

static NON_WORD_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r#"^[.?!…:;,~’'"„“”»«‚‘›‹()\[\]\-–—*×∗·+÷/=]$"#).unwrap());

fn is_vowel(c: char) -> bool {
    "aeiouyAEIOUYäöüÄÖÜ".contains(c)
}

fn simple_syllables_count(word: &str) -> usize {
    let chars: Vec<char> = word.chars().collect();
    if chars.is_empty() {
        return 0;
    }
    let mut n_syllables = 0usize;
    if is_vowel(chars[0]) {
        n_syllables += 1;
    }
    let mut last_double = false;
    for i in 1..chars.len() {
        let c = chars[i];
        if is_vowel(c) {
            let cl = chars[i - 1];
            if last_double {
                n_syllables += 1;
                last_double = false;
            } else if ((c == 'i' || c == 'y') && (cl == 'a' || cl == 'e' || cl == 'A' || cl == 'E'))
                || (c == 'u'
                    && (cl == 'a' || cl == 'e' || cl == 'o' || cl == 'A' || cl == 'E' || cl == 'O'))
                || (c == 'e' && (cl == 'e' || cl == 'i' || cl == 'E' || cl == 'I'))
                || (c == 'a' && (cl == 'a' || cl == 'A'))
            {
                last_double = true;
            } else {
                n_syllables += 1;
                last_double = false;
            }
        } else {
            last_double = false;
        }
    }
    n_syllables.max(1)
}

fn readability_level(fre: f64) -> i32 {
    if fre < 30.0 {
        0
    } else if fre < 50.0 {
        1
    } else if fre < 60.0 {
        2
    } else if fre < 70.0 {
        3
    } else if fre < 80.0 {
        4
    } else if fre < 90.0 {
        5
    } else {
        6
    }
}

fn is_paragraph_end(sentences: &[AnalyzedSentence], index: usize) -> bool {
    if index >= sentences.len() - 1 {
        return true;
    }
    let text = sentences[index].text();
    if text.ends_with("\n\n") || text.ends_with("\n\r\n\r") || text.ends_with("\r\n\r\n") {
        return true;
    }
    let next = sentences[index + 1].text();
    next.starts_with('\n') || next.starts_with("\r\n")
}

pub struct GermanReadabilityRule {
    too_easy_test: bool,
}

impl GermanReadabilityRule {
    pub fn new(too_easy_test: bool) -> Self {
        Self { too_easy_test }
    }

    fn print_message_level(level: i32) -> String {
        let s_level = match level {
            0 => "Sehr schwer",
            1 => "Schwer",
            2 => "Mittelschwer",
            3 => "Mittel",
            4 => "Mittelleicht",
            5 => "Leicht",
            6 => "Sehr leicht",
            _ => return String::new(),
        };
        format!(" {{Grad {level}: {s_level}}}")
    }

    fn flesch_reading_ease(asl: f64, asw: f64) -> f64 {
        180.0 - asl - (58.5 * asw)
    }
}

impl TextLevelRule for GermanReadabilityRule {
    fn id(&self) -> &str {
        if self.too_easy_test {
            "READABILITY_RULE_SIMPLE_DE"
        } else {
            "READABILITY_RULE_DIFFICULT_DE"
        }
    }

    fn description(&self) -> &str {
        if self.too_easy_test {
            "Lesbarkeit: Zu einfacher Text"
        } else {
            "Lesbarkeit: Zu schwieriger Text"
        }
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("CREATIVE_WRITING", "Stiltipps für kreatives Schreiben")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let mut n_paragraph = 0usize;
        let mut n_all_sentences = 0usize;
        let mut n_all_words = 0usize;
        let mut n_all_syllables = 0usize;
        let mut n_sentences = 0usize;
        let mut n_words = 0usize;
        let mut n_syllables = 0usize;
        let mut start_pos: Option<usize> = None;
        let mut end_pos: Option<usize> = None;
        for (n, sentence) in sentences.iter().enumerate() {
            let tokens = sentence.non_whitespace_tokens();
            if start_pos.is_none() && tokens.len() > 1 {
                start_pos = Some(tokens[1].token().start());
            }
            if end_pos.is_none() && tokens.len() > MARK_WORDS {
                end_pos = Some(tokens[MARK_WORDS].token().end());
            }
            n_sentences += 1;
            for token in &tokens {
                let s_token = token.token().token();
                if s_token != "<S>" && !NON_WORD_REGEX.is_match(s_token) {
                    n_words += 1;
                    n_syllables += simple_syllables_count(s_token);
                }
            }
            if is_paragraph_end(sentences, n) {
                if n_words >= MIN_WORDS
                    && let (Some(start), Some(end)) = (start_pos, end_pos)
                {
                    let asl = n_words as f64 / n_sentences as f64;
                    let asw = n_syllables as f64 / n_words as f64;
                    let fre = Self::flesch_reading_ease(asl, asw);
                    let r_level = readability_level(fre);
                    if (self.too_easy_test && r_level > LEVEL)
                        || (!self.too_easy_test && r_level < LEVEL)
                    {
                        let simple = if self.too_easy_test {
                            "einfach"
                        } else {
                            "schwierig"
                        };
                        let few = if self.too_easy_test {
                            "wenige"
                        } else {
                            "viele"
                        };
                        let msg = format!(
                            "Lesbarkeit: Der Text dieses Absatzes ist zu {simple}{}. Zu {few} Wörter pro Satz und zu {few} Silben pro Wort.",
                            Self::print_message_level(r_level),
                        );
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_category(self.category())
                            .with_issue_type(self.issue_type().as_str());
                        rule_matches.push(
                            RuleMatch::new(
                                msg,
                                start,
                                end - start,
                                rm_rule,
                                RuleMatchContext::new(
                                    sentence.text().to_string(),
                                    start,
                                    end - start,
                                ),
                            )
                            .with_sentence(sentence.text().to_string()),
                        );
                    }
                }
                n_all_sentences += n_sentences;
                n_all_words += n_words;
                n_all_syllables += n_syllables;
                n_sentences = 0;
                n_words = 0;
                n_syllables = 0;
                start_pos = None;
                end_pos = None;
                n_paragraph += 1;
            }
        }
        let asl = n_all_words as f64 / n_all_sentences as f64;
        let asw = n_all_syllables as f64 / n_all_words as f64;
        let fre = Self::flesch_reading_ease(asl, asw);
        let r_level = readability_level(fre);
        if n_paragraph > 1 && self.too_easy_test && r_level > LEVEL
            || !self.too_easy_test && r_level < LEVEL
        {
            rule_matches
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {

    fn difficult_text() -> String {
        "Die außerordentlich bedeutsamen Landesinnenministeriumsveranstaltungsunterlagen der vereinten Republik Deutschland wurden einer gründlichen Überprüfung unterzogen. Die zuständigen Behördenvertreterinnen und Behördenvertreter erörterten außerordentlich komplizierte Verwaltungsangelegenheiten.".to_string()
    }

    #[test]
    fn flags_difficult_paragraph_when_enabled() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let mut request = og_core::CheckRequest::new(difficult_text(), engine.language().clone());
        request.enabled_rules = vec!["READABILITY_RULE_DIFFICULT_DE".to_string()];
        request.picky = true;
        let matches = engine.check_with_request(&request);
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "READABILITY_RULE_DIFFICULT_DE"),
            "expected READABILITY_RULE_DIFFICULT_DE match"
        );
    }

    #[test]
    fn default_off_rules_do_not_fire_in_plain_check() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check(&difficult_text());
        assert!(
            !matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "READABILITY_RULE_DIFFICULT_DE"),
            "readability rule is default-off in Java and must not fire without explicit enablement"
        );
    }
}
