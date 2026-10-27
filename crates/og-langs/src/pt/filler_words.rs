use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, rule::TextLevelRule,
};
use regex::Regex;
use std::sync::LazyLock;

const MIN_PERCENT: i64 = 8;

const FILLER_WORDS: &[&str] = &[
    "abundante",
    "acrescentou",
    "acrescidamente",
    "adição",
    "agora",
    "ainda",
    "além",
    "algo",
    "algum",
    "alguma",
    "algumas",
    "alguns",
    "aparecer",
    "aparentemente",
    "apenas",
    "apesar",
    "aproximadamente",
    "assim",
    "atrás",
    "atualmente",
    "automaticamente",
    "bem",
    "bonito",
    "certamente",
    "certo",
    "claramente",
    "claro",
    "completam",
    "completamente",
    "completo",
    "comumente",
    "consequentemente",
    "consistentemente",
    "continuamente",
    "contra",
    "contraste",
    "contudo",
    "cuidado",
    "curto",
    "dependendo",
    "depois",
    "desigual",
    "determinado",
    "deve",
    "dever",
    "difícil",
    "direito",
    "dúvida",
    "embora",
    "enquanto",
    "entanto",
    "ergo",
    "especial",
    "estranhamente",
    "eventualmente",
    "evidentemente",
    "expressar",
    "extremamente",
    "fácil",
    "famoso",
    "feio",
    "felizmente",
    "francamente",
    "frequência",
    "frequentemente",
    "geralmente",
    "graças",
    "impressionante",
    "impronunciável",
    "incomum",
    "indizível",
    "infelizmente",
    "irrelevante",
    "irrelevantes",
    "já",
    "justo",
    "lento",
    "longo",
    "lugares",
    "maior",
    "mais",
    "mas",
    "melhor",
    "mesmo",
    "muita",
    "muitas",
    "muito",
    "muitos",
    "múltipla",
    "nada",
    "não",
    "natural",
    "naturalmente",
    "natureza",
    "nehumas",
    "nenhum",
    "nenhuma",
    "nenhuns",
    "nomeadamente",
    "normalmente",
    "novo",
    "número",
    "nunca",
    "óbvio",
    "ocasionalmente",
    "outra",
    "outros",
    "para",
    "parente",
    "particularmente",
    "pessoa",
    "pode",
    "poderia",
    "pois",
    "porém",
    "porque",
    "portanto",
    "possível",
    "possivelmente",
    "pouca",
    "poucas",
    "pouco",
    "poucos",
    "prático",
    "precisas",
    "principalmente",
    "provável",
    "provavelmente",
    "quaisquer",
    "qualquer",
    "quase",
    "rápido",
    "raramente",
    "razoavelmente",
    "realmente",
    "recentemente",
    "relativamente",
    "repente",
    "sempre",
    "senão",
    "sentida",
    "sentidas",
    "sentido",
    "sentidos",
    "siga",
    "significativo",
    "sim",
    "simples",
    "simplesmente",
    "sobre",
    "sozinho",
    "suave",
    "suavemente",
    "substancialmente",
    "suficientemente",
    "tipo",
    "tornar",
    "tornaram",
    "tornou",
    "total",
    "totalmente",
    "toda",
    "todas",
    "todo",
    "todos",
    "tudo",
    "ultrajante",
    "velho",
    "verdade",
    "vez",
    "vezes",
    "volta",
];

static OPENING_QUOTES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[\"\u{201C}\u{201E}\u{00BB}\u{00AB}]").expect("valid regex"));
static ENDING_QUOTES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[\"\u{201C}\u{201D}\u{00BB}\u{00AB}]").expect("valid regex"));
static NON_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[.?!…:;,~’'\"„“”»«‚‘›‹()\[\]\-–—*×∗·+÷/=]"#).expect("valid regex")
});

pub struct PortugueseFillerWordsRule;

impl PortugueseFillerWordsRule {
    pub fn new() -> Self {
        Self
    }

    fn is_filler_word(token: &str) -> bool {
        FILLER_WORDS.contains(&token)
    }

    fn is_exception(tokens: &[&AnalyzedTokenReadings], num: usize) -> bool {
        num >= 1 && tokens[num].token().token() == "mas" && tokens[num - 1].token().token() == ","
    }
}

impl Default for PortugueseFillerWordsRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for PortugueseFillerWordsRule {
    fn id(&self) -> &str {
        "FILLER_WORDS_PT"
    }

    fn description(&self) -> &str {
        "Filler words"
    }

    fn is_default_on(&self) -> bool {
        false
    }

    fn category(&self) -> Category {
        Category::new("STYLE", "Style")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let mut pending: Vec<(usize, usize, &AnalyzedSentence)> = Vec::new();
        let mut word_count = 0usize;
        let mut is_direct_speech = false;

        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            for n in 1..tokens.len() {
                let token = tokens[n];
                let s_token = token.token().token();
                let opens =
                    !is_direct_speech && OPENING_QUOTES.is_match(s_token) && n < tokens.len() - 1;
                if opens {
                    is_direct_speech = true;
                } else if is_direct_speech && ENDING_QUOTES.is_match(s_token) {
                    is_direct_speech = false;
                } else if !token.is_whitespace() && s_token != "<S>" && !NON_WORD.is_match(s_token)
                {
                    word_count += 1;
                    if Self::is_filler_word(s_token) && !Self::is_exception(&tokens, n) {
                        pending.push((token.token().start(), token.token().end(), sentence));
                    }
                }
            }
        }

        let num_matches = pending.len();
        if word_count > 0 {
            let percent = (num_matches * 100 / word_count.max(1)) as i64;
            if percent > MIN_PERCENT {
                for (start, end, sentence) in pending {
                    let ctx_start = start.saturating_sub(40);
                    let ctx_end = std::cmp::min(end + 40, sentence.text().len());
                    let context_text = sentence.text()[ctx_start..ctx_end].to_string();
                    let rm_rule = RuleMatchRule::new(self.id(), self.description())
                        .with_category(self.category())
                        .with_issue_type(self.issue_type().as_str());
                    rule_matches.push(
                        RuleMatch::new(
                            "The word is classified as filler word. Delete it if possible.",
                            start,
                            end - start,
                            rm_rule,
                            RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                        )
                        .with_sentence(sentence.text().to_string()),
                    );
                }
            }
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pt::test_util::make_sentence;

    #[test]
    fn flags_fillers_above_percent_limit() {
        let s = make_sentence("Ainda agora sempre realmente naturalmente certamente verdade");
        let text = s.text().to_string();
        let matches = PortugueseFillerWordsRule::new().match_text(&text, &[s]);
        assert!(
            !matches.is_empty(),
            "6 of 7 words are fillers, must exceed 8 percent"
        );
    }

    #[test]
    fn stays_quiet_below_percent_limit() {
        let s = make_sentence("O menino foi ontem à feira com a mãe comprar fruta fresca");
        let text = s.text().to_string();
        let matches = PortugueseFillerWordsRule::new().match_text(&text, &[s]);
        assert!(matches.is_empty(), "1 filler of many words must stay quiet");
    }

    #[test]
    fn comma_mas_is_exception() {
        let s = make_sentence("Ele respondeu, mas o júri aceitou a explicação escrita");
        let text = s.text().to_string();
        let matches = PortugueseFillerWordsRule::new().match_text(&text, &[s]);
        assert!(
            matches.is_empty(),
            "'mas' after ',' is excepted; counting it would give 1 of 8 = 12 percent"
        );
    }
}
