use og_core::{
    AnalyzedSentence, Category, IssueType, Rule, RuleMatch, RuleMatchContext, RuleMatchRule,
    SuggestedReplacement,
};
use og_languagemodel::{LanguageModel, Probability};
use std::sync::Arc;

const MIN_PROBABILITY: f64 = 1e-14;

struct Replacement {
    tag: &'static str,
    alternative: &'static str,
}

const REPLACEMENTS: &[Replacement] = &[
    Replacement {
        tag: "VBG",
        alternative: "VB",
    },
    Replacement {
        tag: "VBG",
        alternative: "VBN",
    },
    Replacement {
        tag: "VB",
        alternative: "VBG",
    },
    Replacement {
        tag: "VB",
        alternative: "VBZ",
    },
    Replacement {
        tag: "VB",
        alternative: "VBN",
    },
    Replacement {
        tag: "VBZ",
        alternative: "VB",
    },
    Replacement {
        tag: "VBZ",
        alternative: "VBP",
    },
    Replacement {
        tag: "NNS",
        alternative: "NN",
    },
    Replacement {
        tag: "NN",
        alternative: "NNS",
    },
];

pub struct EnglishNgramProbabilityRule {
    model: Arc<dyn LanguageModel>,
}

fn google_words(text: &str) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for part in text.split(|c: char| c.is_whitespace()) {
        if part.is_empty() {
            continue;
        }
        let part_start = offset + text[offset..].find(part).unwrap_or(0);
        let token = if part == "\u{2019}" {
            "'".to_string()
        } else {
            part.to_string()
        };
        out.push((token, part_start, part_start + part.len()));
        offset = part_start + part.len();
    }
    out
}

fn prob(model: &dyn LanguageModel, words: &[&str]) -> Probability {
    model.get_probability(words)
}

fn pos_tags_for(sentence: &AnalyzedSentence, start: usize, end: usize) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for atr in sentence.tokens() {
        let t = atr.token();
        if t.start() == start && t.end() == end {
            for r in atr.readings() {
                if let Some(lemma) = r.lemma() {
                    for tag in r.pos_tags() {
                        out.push((lemma.to_string(), tag.clone()));
                    }
                }
            }
        }
    }
    out
}

fn surface_forms(sentence: &AnalyzedSentence, start: usize, end: usize) -> Vec<String> {
    sentence
        .tokens()
        .iter()
        .filter(|atr| {
            let t = atr.token();
            t.start() == start && t.end() == end
        })
        .map(|atr| atr.token().token().to_string())
        .collect()
}

impl EnglishNgramProbabilityRule {
    pub fn new(model: Arc<dyn LanguageModel>) -> Self {
        Self { model }
    }
}

impl Rule for EnglishNgramProbabilityRule {
    fn id(&self) -> &str {
        "EN_NGRAM_RULE"
    }

    fn description(&self) -> &str {
        "Assume errors for phrases (ngrams) that occur rarely in a reference index"
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Possible Typo")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let text = sentence.text();
        let tokens = google_words(text);
        let mut matches = Vec::new();
        if tokens.len() < 4 {
            return matches;
        }
        for i in 1..tokens.len().saturating_sub(1) {
            let (prev, token, next) = (&tokens[i - 1], &tokens[i], &tokens[i + 1]);
            let p = prob(
                self.model.as_ref(),
                &[prev.0.as_str(), token.0.as_str(), next.0.as_str()],
            );
            if p.prob >= MIN_PROBABILITY || p.total_count > 0 {
                continue;
            }
            let mut suggestions: Vec<String> = Vec::new();
            let mut considered = false;
            let readings = pos_tags_for(sentence, token.1, token.2);
            for rep in REPLACEMENTS {
                let Some((lemma, _)) = readings
                    .iter()
                    .find(|(_, tag)| tag == rep.tag)
                    .map(|(l, t)| (l.clone(), t.clone()))
                else {
                    continue;
                };
                considered = true;
                for alt_tag in [rep.alternative] {
                    let forms = surface_forms(sentence, token.1, token.2);
                    let _ = forms;
                    let candidate = crate::en::ngram_probability::inflect_simple(
                        &lemma, rep.tag, alt_tag, &token.0,
                    );
                    let Some(candidate) = candidate else {
                        continue;
                    };
                    if candidate == token.0 {
                        continue;
                    }
                    let alt_p = prob(
                        self.model.as_ref(),
                        &[prev.0.as_str(), candidate.as_str(), next.0.as_str()],
                    );
                    if alt_p.prob >= p.prob {
                        suggestions.push(candidate);
                    }
                }
            }
            if !considered || suggestions.is_empty() {
                continue;
            }
            let ngram = format!("{} {} {}", prev.0, token.0, next.0);
            let message = format!(
                "The phrase '{ngram}' rarely occurs in the reference corpus ({} times)",
                p.total_count
            );
            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category())
                .with_issue_type("misspelling");
            let start = prev.1;
            let end = next.2;
            let ctx_start = start.saturating_sub(40).min(text.len());
            let ctx_end = (end + 40).min(text.len());
            let mut full_suggestions = Vec::new();
            for s in &suggestions {
                full_suggestions.push(format!("{} {} {}", prev.0, s, next.0));
            }
            matches.push(
                RuleMatch::new(
                    &message,
                    start,
                    end.saturating_sub(start),
                    rm_rule,
                    RuleMatchContext::new(
                        text[ctx_start..ctx_end].to_string(),
                        ctx_start,
                        ctx_end - ctx_start,
                    ),
                )
                .with_replacements(
                    full_suggestions
                        .into_iter()
                        .map(|s| SuggestedReplacement::new(&s))
                        .collect(),
                ),
            );
        }
        matches
    }
}

fn inflect_simple(lemma: &str, from_tag: &str, to_tag: &str, surface: &str) -> Option<String> {
    let _ = (lemma, from_tag);
    let out = match to_tag {
        "VB" | "VBP" => strip_verbal_suffix(surface),
        "VBZ" => {
            let stem = strip_verbal_suffix(surface);
            format!("{stem}s")
        }
        "VBG" => {
            let stem = strip_verbal_suffix(surface);
            if stem.ends_with('e') && !stem.ends_with("ee") {
                format!("{}ing", &stem[..stem.len() - 1])
            } else {
                format!("{stem}ing")
            }
        }
        "VBN" => {
            let stem = strip_verbal_suffix(surface);
            if stem.ends_with('e') && !stem.ends_with("ee") {
                format!("{stem}d")
            } else {
                format!("{stem}ed")
            }
        }
        "NN" => {
            if surface.ends_with('s') && !surface.ends_with("ss") {
                surface[..surface.len() - 1].to_string()
            } else {
                surface.to_string()
            }
        }
        "NNS" => {
            if surface.ends_with('s') {
                surface.to_string()
            } else if surface.ends_with("ch")
                || surface.ends_with("sh")
                || surface.ends_with("x")
                || surface.ends_with("s")
            {
                format!("{surface}es")
            } else if ends_consonant_y(surface)
                && !surface.ends_with("ay")
                && !surface.ends_with("ey")
                && !surface.ends_with("oy")
                && !surface.ends_with("uy")
            {
                format!("{}ies", &surface[..surface.len() - 1])
            } else {
                format!("{surface}s")
            }
        }
        _ => return None,
    };
    Some(out)
}

fn ends_consonant_y(s: &str) -> bool {
    s.ends_with('y') && s.chars().rev().nth(1).is_some_and(|c| !is_vowel(c))
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

fn strip_verbal_suffix(surface: &str) -> String {
    for suffix in ["ing", "ed", "es", "s"] {
        if let Some(stripped) = surface.strip_suffix(suffix)
            && stripped.chars().count() >= 2
        {
            if suffix == "ed" && stripped.ends_with('i') {
                return format!("{}y", &stripped[..stripped.len() - 1]);
            }
            if (suffix == "ing" || suffix == "ed")
                && stripped.ends_with(stripped.chars().last().unwrap())
                && stripped.chars().count() >= 3
                && !is_vowel(stripped.chars().last().unwrap())
            {
                let last = stripped.chars().last().unwrap();
                if stripped.chars().rev().nth(1) == Some(last) {
                    return stripped.to_string();
                }
                return stripped.to_string();
            }
            return stripped.to_string();
        }
    }
    surface.to_string()
}

pub fn inflect(lemma: &str, from: &str, to: &str, surface: &str) -> Option<String> {
    inflect_simple(lemma, from, to, surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    use og_core::{AnalyzedToken, AnalyzedTokenReadings};
    use og_languagemodel::DiskLanguageModel;

    #[test]
    fn rule_runs_without_model_data() {
        let model = DiskLanguageModel::from_pairs(&[(vec!["run"], 10)]);
        let rule = EnglishNgramProbabilityRule::new(Arc::new(model));
        let sentence = AnalyzedSentence::new("We run fast today.", 0, 19);
        let matches = rule.match_sentence(&sentence);
        assert!(matches.len() <= 1);
    }

    fn tagged_sentence(text: &str, tags: &[(&str, &str, &str)]) -> AnalyzedSentence {
        let mut sentence = AnalyzedSentence::new(text, 0, text.len());
        let mut tokens = Vec::new();
        let mut offset = 0usize;
        for word in text.split_whitespace() {
            let start = offset + text[offset..].find(word).unwrap_or(0);
            let end = start + word.len();
            offset = end;
            let mut token = AnalyzedToken::new(word, start, end);
            if let Some((_, lemma, tag)) = tags.iter().find(|(w, _, _)| *w == word) {
                token = token
                    .with_lemma(*lemma)
                    .with_pos_tags(vec![tag.to_string()]);
            }
            tokens.push(AnalyzedTokenReadings::new(token));
        }
        sentence.set_tokens(tokens);
        sentence
    }

    #[test]
    fn rare_trigram_with_inflectable_verb_is_flagged_with_phrase_suggestions() {
        let model = DiskLanguageModel::from_pairs(&[(vec!["We"], 100_000_000)]);
        let rule = EnglishNgramProbabilityRule::new(Arc::new(model));
        let text = "We runs fast today.";
        let sentence = tagged_sentence(text, &[("runs", "run", "VBZ")]);
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1, "{:?}", matches);
        let m = &matches[0];
        assert_eq!(m.rule().id(), "EN_NGRAM_RULE");
        assert_eq!(
            m.message(),
            "The phrase 'We runs fast' rarely occurs in the reference corpus (0 times)"
        );
        assert_eq!(m.offset(), 0);
        assert_eq!(m.length(), text.find("fast").unwrap() + "fast".len());
        let values: Vec<&str> = m.replacements().iter().map(|r| r.value()).collect();
        assert_eq!(values, ["We run fast", "We run fast"]);
    }

    #[test]
    fn attested_trigrams_are_never_flagged() {
        let model = DiskLanguageModel::from_pairs(&[(vec!["We", "runs", "fast"], 7)]);
        let rule = EnglishNgramProbabilityRule::new(Arc::new(model));
        let sentence = tagged_sentence("We runs fast today.", &[("runs", "run", "VBZ")]);
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn identical_inflection_and_foreign_pos_tags_stay_silent() {
        let model = DiskLanguageModel::from_pairs(&[(vec!["The"], 100_000_000)]);
        let rule = EnglishNgramProbabilityRule::new(Arc::new(model));
        let cats = tagged_sentence("The cats sleep now.", &[("cats", "cat", "NN")]);
        assert!(rule.match_sentence(&cats).is_empty());
        let adjectives = tagged_sentence("The quick runs now.", &[("quick", "quick", "JJ")]);
        assert!(rule.match_sentence(&adjectives).is_empty());
    }

    #[test]
    fn sentences_shorter_than_four_words_return_no_matches() {
        let model = DiskLanguageModel::from_pairs(&[(vec!["We"], 100_000_000)]);
        let rule = EnglishNgramProbabilityRule::new(Arc::new(model));
        let sentence = tagged_sentence("We runs fast.", &[("runs", "run", "VBZ")]);
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn inflect_matrix_pins_the_simplified_inflector() {
        assert_eq!(inflect("run", "VBZ", "VB", "runs").as_deref(), Some("run"));
        assert_eq!(
            inflect("walk", "VBG", "VB", "walking").as_deref(),
            Some("walk")
        );
        assert_eq!(
            inflect("make", "VB", "VBG", "make").as_deref(),
            Some("making")
        );
        assert_eq!(
            inflect("walk", "VB", "VBN", "walk").as_deref(),
            Some("walked")
        );
        assert_eq!(inflect("run", "VB", "VBZ", "run").as_deref(), Some("runs"));
        assert_eq!(inflect("cat", "NN", "NNS", "cat").as_deref(), Some("cats"));
        assert_eq!(inflect("box", "NN", "NNS", "box").as_deref(), Some("boxes"));
        assert_eq!(
            inflect("city", "NN", "NNS", "city").as_deref(),
            Some("cities")
        );
        assert_eq!(inflect("cat", "NNS", "NN", "cats").as_deref(), Some("cat"));
        assert_eq!(inflect("cat", "NN", "CD", "cat"), None);
    }

    #[test]
    fn google_words_maps_the_typographic_apostrophe_and_skips_extra_spaces() {
        let words = google_words("we  don’t  ’  go");
        let spans: Vec<(&str, usize, usize)> =
            words.iter().map(|(w, s, e)| (w.as_str(), *s, *e)).collect();
        assert_eq!(
            spans,
            [
                ("we", 0, 2),
                ("don’t", 4, 11),
                ("'", 13, 16),
                ("go", 18, 20),
            ]
        );
    }
}
