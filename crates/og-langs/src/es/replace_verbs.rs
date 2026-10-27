use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement, rule::Rule,
};
use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

const REPLACE_VERBS_DATA: &str = include_str!(
    "../../../../languagetool/languagetool-language-modules/es/src/main/resources/org/languagetool/rules/es/replace_verbs.txt"
);

const ENDINGS: &str = "a|aba|abais|aban|abas|ad|ada|adas|ado|ados|amos|an|ando|ar|ara|arais|aran|aras|are|areis|aremos|aren|ares|aron|ará|arán|arás|aré|aréis|aría|aríais|aríamos|arían|arías|as|ase|aseis|asen|ases|aste|asteis|e|emos|en|es|o|ábamos|áis|áramos|áremos|ásemos|é|éis|ó|arse|arme|arte|arlos|arles|arlas|arnos|aros";

fn desinencies_reluctant_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"^(.+?)({ENDINGS})$")).unwrap())
}

fn desinencies_greedy_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"^(.+)({ENDINGS})$")).unwrap())
}

fn load_wrong_verbs() -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for line in REPLACE_VERBS_DATA.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((wrong_side, right_side)) = line.split_once('=') else {
            continue;
        };
        if right_side.trim().is_empty() {
            continue;
        }
        let replacements: Vec<String> = right_side.split('|').map(str::to_string).collect();
        for wrong_form in wrong_side.split('|') {
            map.insert(wrong_form.to_string(), replacements.clone());
        }
    }
    map
}

fn shared_tagger() -> Option<std::sync::Arc<crate::es::tagger::SpanishTagger>> {
    crate::es::tagger::SpanishTagger::shared()
}

fn shared_synthesizer() -> Option<Arc<og_rules::postponed_adjective::LangSynthesizer>> {
    static SYNTH: og_core::releasable::Releasable<og_rules::postponed_adjective::LangSynthesizer> =
        og_core::releasable::Releasable::new();
    og_core::releasable::shared_get_or_build(&SYNTH, || {
        crate::es::es_resource_dir().and_then(|dir| {
            og_rules::postponed_adjective::LangSynthesizer::for_language(&dir, "es")
                .map(std::sync::Arc::new)
        })
    })
}

fn is_tagged(atr: &AnalyzedTokenReadings) -> bool {
    atr.first_reading()
        .is_some_and(|r| !r.pos_tags().is_empty())
}

fn apply_orthographic_changes(lexeme: &str, desinence: &str) -> String {
    if desinence.starts_with('e')
        || desinence.starts_with('é')
        || desinence.starts_with('i')
        || desinence.starts_with('ï')
    {
        if let Some(stem) = lexeme.strip_suffix('c') {
            return format!("{stem}z");
        }
        if let Some(stem) = lexeme.strip_suffix("qu") {
            return format!("{stem}c");
        }
        if let Some(stem) = lexeme.strip_suffix("gü") {
            return format!("{stem}gu");
        }
        if let Some(stem) = lexeme.strip_suffix("gu") {
            return format!("{stem}g");
        }
        if let Some(stem) = lexeme.strip_suffix('g') {
            return format!("{stem}j");
        }
    }
    lexeme.to_string()
}

pub struct SimpleReplaceVerbsRule {
    wrong_verbs: HashMap<String, Vec<String>>,
    has_tagger: bool,
    has_synth: bool,
}

impl SimpleReplaceVerbsRule {
    pub fn new() -> Self {
        Self {
            wrong_verbs: load_wrong_verbs(),
            has_tagger: shared_tagger().is_some(),
            has_synth: shared_synthesizer().is_some(),
        }
    }

    fn reading_tags_for_desinence(
        &self,
        tagger: &crate::es::tagger::SpanishTagger,
        desinence: &str,
    ) -> Vec<String> {
        let probe = format!("am{desinence}");
        let tagged = og_tagger::Tagger::tag(tagger, &[probe.as_str()]);
        tagged
            .first()
            .map(|atr| {
                atr.readings()
                    .iter()
                    .filter_map(|r| r.pos_tags().first().cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for SimpleReplaceVerbsRule {
    fn default() -> Self {
        Self::new()
    }
}

impl Rule for SimpleReplaceVerbsRule {
    fn id(&self) -> &str {
        "ES_SIMPLE_REPLACE_VERBS"
    }

    fn description(&self) -> &str {
        "Detecta verbos incorrectos y propone sugerencias."
    }

    fn is_default_on(&self) -> bool {
        true
    }

    fn category(&self) -> Category {
        Category::new("TYPOS", "Ortografía")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Misspelling
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        if !self.has_tagger || !self.has_synth || self.wrong_verbs.is_empty() {
            return Vec::new();
        }
        let tagger = shared_tagger().expect("tagger checked above");
        let synth = shared_synthesizer().expect("synth checked above");
        let mut matches = Vec::new();
        for atr in sentence.non_whitespace_tokens() {
            if is_tagged(atr) {
                continue;
            }
            let original_token = atr.token().token();
            let token_string = original_token.to_lowercase();
            let mut reading_tags: Vec<String> = Vec::new();
            let mut infinitive: Option<String> = None;
            for re in [desinencies_reluctant_re(), desinencies_greedy_re()] {
                let Some(caps) = re.captures(&token_string) else {
                    continue;
                };
                let lexeme = caps.get(1).unwrap().as_str();
                let desinence = caps.get(2).unwrap().as_str();
                let lexeme = apply_orthographic_changes(lexeme, desinence);
                let candidate_infinitive = format!("{lexeme}ar");
                if self.wrong_verbs.contains_key(&candidate_infinitive) {
                    let tags = self.reading_tags_for_desinence(tagger.as_ref(), desinence);
                    if !tags.is_empty() {
                        reading_tags = tags;
                        infinitive = Some(candidate_infinitive);
                        break;
                    }
                }
            }
            let Some(infinitive) = infinitive else {
                continue;
            };
            let Some(replacement_infinitives) = self.wrong_verbs.get(&infinitive) else {
                continue;
            };
            let mut possible_replacements: Vec<String> = Vec::new();
            for replacement_infinitive in replacement_infinitives {
                if replacement_infinitive.starts_with('(') {
                    possible_replacements.push(replacement_infinitive.clone());
                    continue;
                }
                let parts: Vec<&str> = replacement_infinitive.split(' ').collect();
                for tag in &reading_tags {
                    let mut postag = tag.clone();
                    if parts[0] == "haver" {
                        postag = format!("VA{}", &tag[std::cmp::min(2, tag.len())..]);
                    }
                    let synthesized = synth.synthesize_exact(parts[0], &postag);
                    for form in synthesized {
                        let mut s = form;
                        for extra in &parts[1..] {
                            s = format!("{s} {extra}");
                        }
                        if !possible_replacements.contains(&s) {
                            possible_replacements.push(s);
                        }
                    }
                }
            }
            if possible_replacements.is_empty() {
                continue;
            }
            let start = atr.token().start();
            let length = atr.token().end() - start;
            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                .with_category(self.category())
                .with_issue_type(self.issue_type().as_str());
            let ctx_start = start.saturating_sub(40);
            let ctx_end = std::cmp::min(start + length + 40, sentence.text().len());
            matches.push(
                RuleMatch::new(
                    format!("Verbo incorrecto: {original_token}"),
                    start,
                    length,
                    rm_rule,
                    RuleMatchContext::new(
                        sentence.text()[ctx_start..ctx_end].to_string(),
                        ctx_start,
                        ctx_end - ctx_start,
                    ),
                )
                .with_replacements(
                    possible_replacements
                        .iter()
                        .map(|r| SuggestedReplacement::new(r.clone()))
                        .collect(),
                )
                .with_sentence(sentence.text().to_string()),
            );
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::es::test_support::analyze_spanish;

    #[test]
    fn flags_incorrect_verb_with_synthesized_replacement() {
        let rule = SimpleReplaceVerbsRule::new();
        let sentences = analyze_spanish("El volcán eruptó ayer.");
        let matches = rule.match_sentence(&sentences[0]);
        assert_eq!(matches.len(), 1);
        assert!(
            matches[0]
                .replacements()
                .iter()
                .any(|r| r.value() == "eructó" || r.value().contains("erupcion"))
        );
    }

    #[test]
    fn skips_correctly_tagged_verbs() {
        let rule = SimpleReplaceVerbsRule::new();
        let sentences = analyze_spanish("El volcán entró en erupción.");
        let matches = rule.match_sentence(&sentences[0]);
        assert!(matches.is_empty());
    }
}
