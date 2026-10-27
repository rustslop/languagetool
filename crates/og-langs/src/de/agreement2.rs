use super::agreement::{get_agreement_categories, immunized_indices};
use super::helpers::{has_pos_tag_starting_with, uppercase_first_char};
use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use og_tagger::morfologik::MorfologikDictionary;
use std::collections::HashSet;
use std::sync::{Arc, LazyLock};

pub static AGREEMENT2_ANTI_PATTERNS: LazyLock<Option<Vec<crate::engine::XmlPatternRule>>> =
    LazyLock::new(|| {
        let compiler = og_xml::XmlCompiler::new();
        match compiler.compile_file(include_str!("agreement2_antipatterns.xml")) {
            Ok(set) => Some(
                set.rules
                    .into_iter()
                    .map(crate::engine::XmlPatternRule::new)
                    .collect(),
            ),
            Err(_) => None,
        }
    });

pub struct AgreementRule2 {
    synth_dictionary: Option<Arc<MorfologikDictionary>>,
}

impl AgreementRule2 {
    pub fn new(synth_dictionary: Option<Arc<MorfologikDictionary>>) -> Self {
        Self { synth_dictionary }
    }

    fn synthesize(&self, lemma: &str, pos: &str) -> Vec<String> {
        let Some(dict) = &self.synth_dictionary else {
            return Vec::new();
        };
        let Some(entries) = dict.forms_by_lemma().get(lemma) else {
            return Vec::new();
        };
        let lc_lemma = lemma.chars().next().is_some_and(|c| c.is_lowercase());
        let mut out: Vec<String> = Vec::new();
        for (tag, word) in entries {
            if &**tag != pos {
                continue;
            }
            if word.chars().next().is_some_and(|c| c.is_lowercase()) != lc_lemma {
                continue;
            }
            if !out.iter().any(|w| w == &**word) {
                out.push(word.to_string());
            }
        }
        out
    }

    fn get_suggestions(
        &self,
        tokens: &[&AnalyzedTokenReadings],
        i: usize,
        noun_readings: &[og_core::AnalyzedToken],
    ) -> Vec<String> {
        let mut suggestions: Vec<String> = Vec::new();
        let adj_token = tokens[i].readings().first();
        let Some(adj) = adj_token else {
            return suggestions;
        };
        let adj_lemma = adj.lemma().unwrap_or(adj.token());
        for noun_token in noun_readings {
            for noun_tag in noun_token.pos_tags() {
                let gender = if noun_tag.contains(":MAS") {
                    "MAS"
                } else if noun_tag.contains(":FEM") {
                    "FEM"
                } else if noun_tag.contains(":NEU") {
                    "NEU"
                } else {
                    continue;
                };
                let number = if noun_tag.contains(":SIN:") {
                    "SIN"
                } else if noun_tag.contains(":PLU:") {
                    "PLU"
                } else {
                    continue;
                };
                for s in self.synthesize(adj_lemma, &format!("ADJ:NOM:{number}:{gender}:GRU:SOL")) {
                    let full = format!("{} {}", uppercase_first_char(&s), noun_token.token());
                    if !suggestions.contains(&full) {
                        suggestions.push(full);
                    }
                }
            }
        }
        suggestions
    }
}

impl Rule for AgreementRule2 {
    fn id(&self) -> &str {
        "DE_AGREEMENT2"
    }

    fn description(&self) -> &str {
        "Kongruenz von Adjektiv und Nomen (unvollständig!), z.B. 'kleiner (kleines) Haus'"
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("GRAMMAR", "Grammatik")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Grammar
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let tokens = sentence.non_whitespace_tokens();
        let immunized: HashSet<usize> = AGREEMENT2_ANTI_PATTERNS
            .as_ref()
            .map(|rules| immunized_indices(rules, sentence))
            .unwrap_or_default();
        for i in 0..tokens.len() {
            let token = tokens[i].token().token();
            if !tokens[i].is_sentence_start() && !matches!(token, "\"" | "„" | "»" | "«") {
                if i + 1 < tokens.len()
                    && has_pos_tag_starting_with(tokens[i], "ADJ")
                    && has_pos_tag_starting_with(tokens[i + 1], "SUB")
                    && !has_pos_tag_starting_with(tokens[i + 1], "EIG")
                {
                    if immunized.contains(&i)
                        || immunized.contains(&(i + 1))
                        || tokens[i].token().token().eq_ignore_ascii_case("unter")
                    {
                        continue;
                    }
                    if i + 2 < tokens.len() && has_pos_tag_starting_with(tokens[i + 2], "SUB") {
                        break;
                    }
                    let mut set1 = get_agreement_categories(tokens[i], false);
                    let set2 = get_agreement_categories(tokens[i + 1], false);
                    set1.retain(|s| set2.contains(s));
                    if set1.is_empty() {
                        let msg = "Möglicherweise fehlende grammatikalische Übereinstimmung zwischen Adjektiv und Nomen bezüglich Kasus, Numerus oder Genus. Beispiel: 'kleiner Haus' statt 'kleines Haus'";
                        let short_msg = "Möglicherweise keine Übereinstimmung bezüglich Kasus, Numerus oder Genus";
                        let from = tokens[i].token().start();
                        let to = tokens[i + 1].token().end();
                        let rm_rule = RuleMatchRule::new(self.id(), self.description())
                            .with_category(self.category())
                            .with_issue_type(self.issue_type().as_str());
                        let noun_readings = tokens[i + 1].readings().to_vec();
                        let suggestions = self.get_suggestions(&tokens, i, &noun_readings);
                        let mut rm = RuleMatch::new(
                            msg,
                            from,
                            to - from,
                            rm_rule,
                            RuleMatchContext::new(sentence.text().to_string(), from, to - from),
                        )
                        .with_short_message(short_msg)
                        .with_sentence(sentence.text().to_string());
                        if !suggestions.is_empty() {
                            rm = rm.with_replacements(
                                suggestions
                                    .into_iter()
                                    .map(SuggestedReplacement::new)
                                    .collect(),
                            );
                        }
                        rule_matches.push(rm);
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn fires_on_kleiner_haus() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Kleiner Haus am Waldrand steht seit Jahren.");
        assert!(
            matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_AGREEMENT2"),
            "expected DE_AGREEMENT2 match, got {:?}",
            matches
                .matches
                .iter()
                .map(|m| (m.rule().id().to_string(), m.offset()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_fire_on_correct_adj_noun() {
        let engine = crate::engine::LanguageEngine::shared_for_code("de").expect("german engine");
        let matches = engine.check("Kleines Haus am Waldrand steht seit Jahren.");
        assert!(
            !matches
                .matches
                .iter()
                .any(|m| m.rule().id() == "DE_AGREEMENT2")
        );
    }
}
