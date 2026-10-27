use og_core::{
    AnalyzedSentence, Category, IssueType, RuleMatch, RuleMatchContext, RuleMatchRule,
    rule::TextLevelRule,
};
use regex::Regex;
use std::sync::LazyLock;

const MARK_WORDS: usize = 3;
const MIN_WORDS: usize = 10;
const LEVEL: i32 = 3;

static NON_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[.?!…:;,~’'"„“”»«‚‘›‹()\[\]\-–—*×∗·+÷/=]"#).expect("valid regex")
});

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

fn is_vowel(c: char) -> bool {
    matches!(
        c,
        'a' | 'e'
            | 'i'
            | 'o'
            | 'u'
            | 'y'
            | 'A'
            | 'E'
            | 'I'
            | 'O'
            | 'U'
            | 'Y'
            | 'á'
            | 'é'
            | 'í'
            | 'ó'
            | 'ú'
            | 'à'
            | 'Á'
            | 'É'
            | 'Í'
            | 'Ó'
            | 'Ú'
            | 'À'
            | 'â'
            | 'ê'
            | 'ô'
            | 'Â'
            | 'Ê'
            | 'Ô'
            | 'Ü'
    )
}

pub struct PortugueseReadabilityRule {
    too_easy_test: bool,
}

impl PortugueseReadabilityRule {
    pub fn new(too_easy_test: bool) -> Self {
        Self { too_easy_test }
    }

    fn print_message_level(level: i32) -> String {
        let s_level = match level {
            0 => "Muito complexo",
            1 => "Complexo",
            2 => "Moderadamente complexo",
            3 => "Meio-termo",
            4 => "Moderadamente simples",
            5 => "Simples",
            6 => "Muito simples",
            _ => return String::new(),
        };
        format!(" {{Nível {level}: {s_level}}}")
    }

    fn flesch_reading_ease(asl: f64, asw: f64) -> f64 {
        206.84 - (1.02 * asl) - (60.0 * asw)
    }

    pub fn simple_syllables_count(word: &str) -> usize {
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
                } else if ((c == 'ã' || c == 'õ') && (cl == 'e' || cl == 'o'))
                    || (c == 'a' && matches!(cl, 'e' | 'i' | 'í' | 'o' | 'u' | 'ú'))
                    || (c == 'e' && matches!(cl, 'e' | 'i' | 'í' | 'o' | 'a' | 'u'))
                    || (c == 'i' && matches!(cl, 'a' | 'e' | 'o' | 'u' | 'á' | 'é'))
                    || (c == 'í' && (cl == 'a' || cl == 'e'))
                    || (c == 'o' && matches!(cl, 'a' | 'á' | 'e' | 'é' | 'i' | 'í' | 'u'))
                    || (c == 'u' && matches!(cl, 'a' | 'á' | 'e' | 'é' | 'i' | 'o'))
                    || (c == 'ú' && (cl == 'a' || cl == 'e' || cl == 'o'))
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
        if n_syllables == 0 { 1 } else { n_syllables }
    }

    fn message(&self, level: i32, fre: i32, asl: i32, asw: i32) -> String {
        let (simple, few) = if self.too_easy_test {
            ("fácil", "poucas")
        } else {
            ("difícil", "muitas")
        };
        format!(
            "Legibilidade {{FRE: {fre}, ASL: {asl}, ASW: {asw}}}: O texto deste parágrafo é {simple}{}{}. Tem {few} palavras por frase e {few} sílabas por palavra.",
            Self::print_message_level(level),
            "."
        )
    }
}

impl TextLevelRule for PortugueseReadabilityRule {
    fn id(&self) -> &str {
        if self.too_easy_test {
            "READABILITY_RULE_SIMPLE_PT"
        } else {
            "READABILITY_RULE_DIFFICULT_PT"
        }
    }

    fn description(&self) -> &str {
        if self.too_easy_test {
            "Legibilidade: texto demasiado simples"
        } else {
            "Legibilidade: texto demasiado complexo"
        }
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("TEXT_ANALYSIS", "Análise de Texto")
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
                if s_token != "<S>" && !NON_WORD.is_match(s_token) {
                    n_words += 1;
                    n_syllables += Self::simple_syllables_count(s_token);
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
                        let msg = self.message(r_level, fre as i32, asl as i32, asw as i32);
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_category(self.category())
                            .with_issue_type(self.issue_type().as_str());
                        rule_matches.push(
                            RuleMatch::new(
                                msg,
                                start,
                                end - start,
                                rm_rule,
                                RuleMatchContext::new(String::new(), start, end - start),
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
        if n_all_sentences > 0 && n_all_words > 0 {
            let asl = n_all_words as f64 / n_all_sentences as f64;
            let asw = n_all_syllables as f64 / n_all_words as f64;
            let fre = Self::flesch_reading_ease(asl, asw);
            let r_level = readability_level(fre);
            if n_paragraph > 1 && self.too_easy_test && r_level > LEVEL
                || !self.too_easy_test && r_level < LEVEL
            {
                return rule_matches;
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pt::test_util::make_sentence;

    #[test]
    fn syllable_counts_follow_java_vowel_rules() {
        assert_eq!(PortugueseReadabilityRule::simple_syllables_count("casa"), 2);
        assert_eq!(PortugueseReadabilityRule::simple_syllables_count("pá"), 1);
        assert_eq!(PortugueseReadabilityRule::simple_syllables_count("pai"), 1);
        assert_eq!(PortugueseReadabilityRule::simple_syllables_count("café"), 2);
        assert_eq!(
            PortugueseReadabilityRule::simple_syllables_count("ultrassom"),
            3
        );
    }

    #[test]
    fn ids_match_java() {
        assert_eq!(
            PortugueseReadabilityRule::new(true).id(),
            "READABILITY_RULE_SIMPLE_PT"
        );
        assert_eq!(
            PortugueseReadabilityRule::new(false).id(),
            "READABILITY_RULE_DIFFICULT_PT"
        );
    }

    #[test]
    fn long_hard_paragraph_flags_difficult() {
        let rule = PortugueseReadabilityRule::new(false);
        let s1 = make_sentence(
            "Considerando extraordinariamente complexas circunstâncias institucionais contemporâneas, \
             extraordinariamente influenciam consideravelmente determinadas transformações estructurais.",
        );
        let matches = rule.match_text(s1.text(), std::slice::from_ref(&s1));
        assert!(!matches.is_empty());
    }

    #[test]
    fn multiple_paragraphs_required_for_global_check() {
        let rule = PortugueseReadabilityRule::new(false);
        let s = make_sentence("Frase curta simples aqui agora mesmo.");
        let text = s.text().to_string();
        assert!(rule.match_text(&text, &[s]).is_empty());
    }
}
