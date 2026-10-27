use og_core::{
    AnalyzedSentence, AnalyzedTokenReadings, Category, IssueType, RuleMatch, RuleMatchContext,
    RuleMatchRule, SuggestedReplacement,
    rule::{Rule, TextLevelRule},
};
use regex::Regex;
use std::sync::LazyLock;

const REPETITION_MSG: &str = "Possible typo: you repeated a word.";
const REPETITION_DESC: &str = "Word repetition (e.g. 'will will')";
const REPETITION_SHORT: &str = "Word repetition";

static TAUTONYMS_GENUS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"A(?:aptos|canthogyrus|chatina|gagus|gama|lburnus|lces|lle|losa|mandava|mazilia|meiva|nableps|nguilla|nhinga|nostomus|nser|nthias|pus|rcinella|riadne|spredo|stacus|vicularia|xis)|B(?:adis|agarius|agre|alanus|anjos|arbatula|arbus|asiliscus|atasio|elobranchus|elone|elonimorphis|idyanus|ison|ombina|oops|rama|rosme|ubo|ucayana|ufo|uteo|utis)|C(?:alamus|alappa|aleta|allichthys|alotes|apoeta|apreolus|aracal|arassius|ardinalis|arduelis|aretta|asuarius|atla|atostomus|ephea|erastes|haca|halcides|handramara|hanos|haos|hinchilla|hiropotes|hitala|hromis|iconia|idaris|inclus|itellus|lelia|occothraustes|ochlearius|oeligena|olius|olumella|oncholepas|onger|onta|onvoluta|ordylus|oscoroba|ossus|otinga|oturnix|rangon|ressida|rex|ricetus|rocuta|rossoptilon|uraeus|yanicterus|ygnus|ymbium|ynoglossus)|D(?:ama|ario|entex|evario|iuca|ives|olabrifera)|E(?:nhydris|nsifera|nsis|rythrinus|xtra)|F(?:alcipennis|eroculus|icus|ragum|rancolinus|urcula)|G(?:agata|albula|allinago|allus|azella|emma|enetta|erbillus|ibberulus|iraffa|lis|lycimeris|lyphis|obio|oliathus|onorynchus|orilla|rapsus|rus|ryllotalpa|uira|ulo)|H(?:ara|arpa|austellum|emilepidotus|eterophyes|imantopus|ippocampus|ippoglossus|ippopus|istrio|istrionicus|oolock|ucho|uso|yaena|ypnale)|I(?:chthyaetus|cterus|dea|guana|ndicator|ndri)|J(?:acana|aculus|anthina)|K(?:achuga|oilofera)|L(?:actarius|agocephalus|agopus|agurus|ambis|emmus|epadogaster|erwa|euciscus|ima|imanda|imosa|iparis|ithognathus|ithophaga|oa|ota|uscinia|utjanus|utra|utraria|ynx)|M(?:acrophyllum|anacus|argaritifera|armota|artes|ascarinus|ashuna|egacephala|elanodera|eles|elo|elolontha|elongena|enidia|ephitis|ercenaria|eretrix|erluccius|eza|icrostoma|ilvus|itella|itra|itu|odiolus|odulus|ola|olossus|olva|onachus|oniliformis|ops|ustelus|yaka|yospalax|yotis)|N(?:aja|aja|angra|asua|atrix|eita|iviventer|otopterus|ycticorax)|O(?:enanthe|gasawarana|liva|phioscincus|plopomus|reotragus|riolus)|P(?:agrus|angasius|apio|auxi|erdix|eriphylla|erna|etaurista|etronia|hocoena|hoenicurus|hoxinus|hycis|ica|ipa|ipile|ipistrellus|ipra|ithecia|lanorbis|lica|oliocephalus|ollachius|ollicipes|orites|orphyrio|orphyrolaema|orpita|orzana|ristis|seudobagarius|udu|uffinus|ungitius|yrrhocorax|yrrhula)|Q(?:uadrula|uelea)|R(?:ama|anina|apa|asbora|attus|edunca|egulus|emora|etropinna|hinobatos|iparia|ita|upicapra|upicola|utilus)|S(?:accolaimus|alamandra|arda|calpellum|cincus|colytus|ephanoides|erinus|odreana|olea|phyraena|pinachia|pirorbis|pirula|prattus|quatina|taphylaea|uiriri|ula|uta|ynodus)|T(?:adorna|andanus|chagra|elescopium|emnurus|erebellum|etradactylus|etrax|herezopolis|hymallus|ibicen|inca|odus|orpedo|rachurus|rachycorystes|rachyrinchus|ricornis|roglodytes|ropheops|ubifex|yrannus)|U(?:mbraculum|ncia)|V(?:anellus|elella|elutina|icugna|illosa|imba|iviparus|olva|ulpes)|X(?:anthocephalus|anthostigma|enopirostris)|Ypiranga|Z(?:ebrus|era|ingel|ingha|oma|onia|ungaro|ygoneura)|Se").expect("valid regex")
});

static TAUTONYMS_SPECIES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"a(?:aptos|canthogyrus|chatina|gagus|gama|lburnus|lces|lle|losa|mandava|mazilia|meiva|nableps|nguilla|nhinga|nostomus|nser|nthias|pus|rcinella|riadne|spredo|stacus|vicularia|xis)|b(?:adis|agarius|agre|alanus|anjos|arbatula|arbus|asiliscus|atasio|elobranchus|elone|elonimorphis|idyanus|ison|ombina|oops|rama|rosme|ubo|ucayana|ufo|uteo|utis)|c(?:alamus|alappa|aleta|allichthys|alotes|apoeta|apreolus|aracal|arassius|ardinalis|arduelis|aretta|asuarius|atla|atostomus|ephea|erastes|haca|halcides|handramara|hanos|haos|hinchilla|hiropotes|hitala|hromis|iconia|idaris|inclus|itellus|lelia|occothraustes|ochlearius|oeligena|olius|olumella|oncholepas|onger|onta|onvoluta|ordylus|oscoroba|ossus|otinga|oturnix|rangon|ressida|rex|ricetus|rocuta|rossoptilon|uraeus|yanicterus|ygnus|ymbium|ynoglossus)|d(?:ama|ario|entex|evario|iuca|ives|olabrifera)|e(?:nhydris|nsifera|nsis|rythrinus|xtra)|f(?:alcipennis|eroculus|icus|ragum|rancolinus|urcula)|g(?:agata|albula|allinago|allus|azella|emma|enetta|erbillus|ibberulus|iraffa|lis|lycimeris|lyphis|obio|oliathus|onorynchus|orilla|rapsus|rus|ryllotalpa|uira|ulo)|h(?:ara|arpa|austellum|emilepidotus|eterophyes|imantopus|ippocampus|ippoglossus|ippopus|istrio|istrionicus|oolock|ucho|uso|yaena|ypnale)|i(?:chthyaetus|cterus|dea|guana|ndicator|ndri)|j(?:acana|aculus|anthina)|k(?:achuga|oilofera)|l(?:actarius|agocephalus|agopus|agurus|ambis|emmus|epadogaster|erwa|euciscus|ima|imanda|imosa|iparis|ithognathus|ithophaga|oa|ota|uscinia|utjanus|utra|utraria|ynx)|m(?:acrophyllum|anacus|argaritifera|armota|artes|ascarinus|ashuna|egacephala|elanodera|eles|elo|elolontha|elongena|enidia|ephitis|ercenaria|eretrix|erluccius|eza|icrostoma|ilvus|itella|itra|itu|odiolus|odulus|ola|olossus|olva|onachus|oniliformis|ops|ustelus|yaka|yospalax|yotis)|n(?:aja|aja|angra|asua|atrix|eita|iviventer|otopterus|ycticorax)|o(?:enanthe|gasawarana|liva|phioscincus|plopomus|reotragus|riolus)|p(?:agrus|angasius|apio|auxi|erdix|eriphylla|erna|etaurista|etronia|hocoena|hoenicurus|hoxinus|hycis|ica|ipa|ipile|ipistrellus|ipra|ithecia|lanorbis|lica|oliocephalus|ollachius|ollicipes|orites|orphyrio|orphyrolaema|orpita|orzana|ristis|seudobagarius|udu|uffinus|ungitius|yrrhocorax|yrrhula)|q(?:uadrula|uelea)|r(?:ama|anina|apa|asbora|attus|edunca|egulus|emora|etropinna|hinobatos|iparia|ita|upicapra|upicola|utilus)|s(?:accolaimus|alamandra|arda|calpellum|cincus|colytus|ephanoides|erinus|odreana|olea|phyraena|pinachia|pirorbis|pirula|prattus|quatina|taphylaea|uiriri|ula|uta|ynodus)|t(?:adorna|andanus|chagra|elescopium|emnurus|erebellum|etradactylus|etrax|herezopolis|hymallus|ibicen|inca|odus|orpedo|rachurus|rachycorystes|rachyrinchus|ricornis|roglodytes|ropheops|ubifex|yrannus)|u(?:mbraculum|ncia)|v(?:anellus|elella|elutina|icugna|illosa|imba|iviparus|olva|ulpes)|x(?:anthocephalus|anthostigma|enopirostris)|ypiranga|z(?:ebrus|era|ingel|ingha|oma|onia|ungaro|ygoneura)|se").expect("valid regex")
});

static PRONOUNS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("mas|n?[ao]s?|se").expect("valid regex"));
static REDUPLICATED_ADVERBS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?i)já|logo|fácil").expect("valid regex"));

pub struct PortugueseWordRepeatRule;

impl PortugueseWordRepeatRule {
    pub fn new() -> Self {
        Self
    }

    fn word_repetition_of(word: &str, tokens: &[&AnalyzedTokenReadings], position: usize) -> bool {
        position > 0
            && tokens[position - 1].token().token() == word
            && tokens[position].token().token() == word
    }

    fn ignore(
        &self,
        tokens: &[&AnalyzedTokenReadings],
        ws_flags: &[bool],
        position: usize,
    ) -> bool {
        if Self::word_repetition_of("blá", tokens, position)
            || Self::word_repetition_of("se", tokens, position)
            || Self::word_repetition_of("sapiens", tokens, position)
            || Self::word_repetition_of("tuk", tokens, position)
            || Self::word_repetition_of("Phi", tokens, position)
            || Self::word_repetition_of("Li", tokens, position)
            || Self::word_repetition_of("Xiao", tokens, position)
            || Self::word_repetition_of("Duran", tokens, position)
            || Self::word_repetition_of("Wagga", tokens, position)
            || Self::word_repetition_of("Abdullah", tokens, position)
            || Self::word_repetition_of("Nwe", tokens, position)
            || Self::word_repetition_of("Pago", tokens, position)
            || Self::word_repetition_of("Cao", tokens, position)
        {
            return true;
        }
        let prev = tokens[position - 1].token().token();
        let curr = tokens[position].token().token();
        if TAUTONYMS_GENUS.is_match(prev) && TAUTONYMS_SPECIES.is_match(curr) {
            return true;
        }
        if position >= 2
            && tokens[position - 2].token().token() == "-"
            && !ws_flags.get(position - 1).copied().unwrap_or(true)
            && PRONOUNS.is_match(curr)
        {
            return true;
        }
        REDUPLICATED_ADVERBS.is_match(curr)
    }
}

impl Default for PortugueseWordRepeatRule {
    fn default() -> Self {
        Self::new()
    }
}

fn is_word(token: &str) -> bool {
    if token
        .chars()
        .any(|c| ('\u{1F300}'..='\u{1FAFF}').contains(&c) || ('\u{2600}'..='\u{27BF}').contains(&c))
    {
        return false;
    }
    if !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_digit() || c == ' ' || c == '\u{00A0}')
    {
        return false;
    }
    if token.chars().count() == 1 {
        return token.chars().next().is_some_and(char::is_alphabetic);
    }
    true
}

impl Rule for PortugueseWordRepeatRule {
    fn id(&self) -> &str {
        "PORTUGUESE_WORD_REPEAT_RULE"
    }

    fn description(&self) -> &str {
        REPETITION_DESC
    }

    fn category(&self) -> Category {
        Category::new("REPETITIONS", "Repetitions")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Other("duplication".to_string())
    }

    fn match_sentence(&self, sentence: &AnalyzedSentence) -> Vec<RuleMatch> {
        let mut rule_matches = Vec::new();
        let all_tokens = sentence.tokens();
        let mut tokens: Vec<&AnalyzedTokenReadings> = Vec::with_capacity(all_tokens.len());
        let mut ws_flags: Vec<bool> = Vec::with_capacity(all_tokens.len());
        let mut prev_ws = false;
        for t in all_tokens {
            if t.is_whitespace() {
                prev_ws = true;
                continue;
            }
            tokens.push(t);
            ws_flags.push(prev_ws);
            prev_ws = false;
        }
        let mut prev_token = String::new();
        for i in 0..tokens.len() {
            let token = tokens[i].token().token();
            if tokens[i].is_immunized() {
                prev_token.clear();
                continue;
            }
            if is_word(token)
                && !prev_token.is_empty()
                && prev_token.to_lowercase() == token.to_lowercase()
                && i > 0
                && !self.ignore(&tokens, &ws_flags, i)
            {
                let prev_pos = tokens[i - 1].token().start();
                let pos = tokens[i].token().start();
                let match_len = pos + prev_token.len() - prev_pos;
                let ctx_start = prev_pos.saturating_sub(40);
                let ctx_end = std::cmp::min(pos + prev_token.len() + 40, sentence.text().len());
                let context_text = sentence.text()[ctx_start..ctx_end].to_string();
                let rm_rule = RuleMatchRule::new(self.id(), self.description())
                    .with_category(self.category())
                    .with_issue_type(self.issue_type().as_str());
                rule_matches.push(
                    RuleMatch::new(
                        REPETITION_MSG,
                        prev_pos,
                        match_len,
                        rm_rule,
                        RuleMatchContext::new(context_text, ctx_start, ctx_end - ctx_start),
                    )
                    .with_short_message(REPETITION_SHORT)
                    .with_replacements(vec![SuggestedReplacement::new(&prev_token)])
                    .with_sentence(sentence.text().to_string()),
                );
            }
            prev_token = token.to_string();
        }
        rule_matches
    }
}

const ADVERBS: &[&str] = &[
    "Abaixo",
    "Acaso",
    "Acima",
    "Acolá",
    "Ademais",
    "Adentro",
    "Adiante",
    "Adicionalmente",
    "Afinal",
    "Afora",
    "Agora",
    "Aí",
    "Ainda",
    "Além",
    "Algures",
    "Ali",
    "Aliás",
    "Amanhã",
    "Amiúde",
    "Antigamente",
    "Aonde",
    "Apenas",
    "Apesar",
    "Aquém",
    "Aqui",
    "Assaz",
    "Assim",
    "Até",
    "Atrás",
    "Bastante",
    "Bem",
    "Bondosamente",
    "Breve",
    "Cá",
    "Casualmente",
    "Cedo",
    "Certamente",
    "Certo",
    "Constantemente",
    "Cuidadosamente",
    "Dantes",
    "Debaixo",
    "Debalde",
    "Decerto",
    "Defronte",
    "Demais",
    "Demasiado",
    "Dentro",
    "Depois",
    "Depressa",
    "Detrás",
    "Devagar",
    "Doravante",
    "E",
    "Efetivamente",
    "Embaixo",
    "Embora",
    "Enfim",
    "Então",
    "Entrementes",
    "Exclusivamente",
    "Externamente",
    "Fora",
    "Frequentemente",
    "Generosamente",
    "Hoje",
    "Imediatamente",
    "Inclusivamente",
    "Inda",
    "Já",
    "Jamais",
    "Lá",
    "Logo",
    "Longe",
    "Mais",
    "Mal",
    "Mas",
    "Melhor",
    "Menos",
    "Mesmo",
    "Muito",
    "Não",
    "Nem",
    "Nenhuras",
    "Nunca",
    "Onde",
    "Ontem",
    "Ora",
    "Ou",
    "Outra",
    "Outro",
    "Outrora",
    "Outrossim",
    "Perto",
    "Pior",
    "Porventura",
    "Possivelmente",
    "Pouco",
    "Primeiramente",
    "Primeiro",
    "Principalmente",
    "Provavelmente",
    "Provisoriamente",
    "Quanto",
    "Quão",
    "Quase",
    "Quiçá",
    "Realmente",
    "Salvo",
    "Seguidamente",
    "Sempre",
    "Senão",
    "Será",
    "Sim",
    "Simplesmente",
    "Só",
    "Sobremaneira",
    "Sobremodo",
    "Sobretudo",
    "Somente",
    "Sucessivamente",
    "Talvez",
    "Também",
    "Tampouco",
    "Tanto",
    "Tão",
    "Tarde",
    "Ultimamente",
    "Unicamente",
];

const BEGINNING_DESC: &str = "Successive sentences beginning with the same word";
const BEGINNING_WORD: &str = "Three successive sentences begin with the same word.";
const BEGINNING_ADV: &str = "Two successive sentences begin with the same adverb.";
const BEGINNING_THESAURUS: &str =
    "Consider rewording the sentence or use a thesaurus to find a synonym.";

pub struct PortugueseWordRepeatBeginningRule;

impl PortugueseWordRepeatBeginningRule {
    pub fn new() -> Self {
        Self
    }

    fn is_adverb(token: &str) -> bool {
        ADVERBS.contains(&token)
    }

    fn is_exception(token: &str) -> bool {
        matches!(token, ":" | "–" | "-" | "✔️" | "➡️" | "—" | "⭐️" | "⚠️")
    }
}

impl Default for PortugueseWordRepeatBeginningRule {
    fn default() -> Self {
        Self::new()
    }
}

impl TextLevelRule for PortugueseWordRepeatBeginningRule {
    fn id(&self) -> &str {
        "PORTUGUESE_WORD_REPEAT_BEGINNING_RULE"
    }

    fn description(&self) -> &str {
        BEGINNING_DESC
    }

    fn category(&self) -> Category {
        Category::new("REPETITIONS_STYLE", "Repetitions Style")
    }

    fn issue_type(&self) -> IssueType {
        IssueType::Style
    }

    fn match_text(&self, _text: &str, sentences: &[AnalyzedSentence]) -> Vec<RuleMatch> {
        let mut last_token = String::new();
        let mut before_last_token = String::new();
        let mut rule_matches = Vec::new();
        let mut prev_sentence: Option<&AnalyzedSentence> = None;
        for sentence in sentences {
            let tokens = sentence.non_whitespace_tokens();
            let first_idx = tokens
                .iter()
                .position(|t| !t.is_sentence_start())
                .unwrap_or(0);
            let mut token = "";
            if tokens.len() > first_idx {
                let analyzed_token = tokens[first_idx];
                token = analyzed_token.token().token();
                if tokens.len() > first_idx + 2 {
                    let is_word = token.chars().count() != 1
                        || token.chars().next().is_some_and(char::is_alphabetic);
                    let prev_text = prev_sentence.map(|s| s.text().trim()).unwrap_or("");
                    if is_word
                        && last_token == token
                        && !Self::is_exception(token)
                        && !Self::is_exception(tokens[first_idx + 1].token().token())
                        && !Self::is_exception(tokens[first_idx + 2].token().token())
                        && prev_sentence.is_some()
                        && prev_text.len() > 1
                        && prev_text.ends_with(['.', '?', '!'])
                    {
                        let short_msg = if Self::is_adverb(token) {
                            BEGINNING_ADV
                        } else if before_last_token == token {
                            BEGINNING_WORD
                        } else {
                            ""
                        };
                        if !short_msg.is_empty() {
                            let msg = format!("{short_msg} {BEGINNING_THESAURUS}");
                            let start = tokens[first_idx].token().start();
                            let end = start + token.len();
                            let sent_start = sentence.start();
                            let ctx_start = start.saturating_sub(40).max(sent_start);
                            let ctx_end =
                                std::cmp::min(end + 40, sent_start + sentence.text().len());
                            let local_start = ctx_start - sent_start;
                            let local_end = (ctx_end - sent_start).min(sentence.text().len());
                            let context_text = sentence.text()[local_start..local_end].to_string();
                            let rm_rule = RuleMatchRule::new(self.id(), self.description())
                                .with_category(self.category())
                                .with_issue_type(self.issue_type().as_str());
                            rule_matches.push(
                                RuleMatch::new(
                                    msg,
                                    start,
                                    end - start,
                                    rm_rule,
                                    RuleMatchContext::new(context_text, ctx_start, ctx_end),
                                )
                                .with_short_message(short_msg)
                                .with_sentence(sentence.text().to_string()),
                            );
                        }
                    }
                }
            }
            before_last_token = last_token;
            last_token = token.to_string();
            prev_sentence = Some(sentence);
        }
        rule_matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pt::test_util::make_sentence;

    #[test]
    fn matches_java_example_pair() {
        let rule = PortugueseWordRepeatRule::new();
        let sentence = make_sentence("Este é é apenas uma frase de exemplo.");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacements()[0].value(), "é");
    }

    #[test]
    fn ignores_tautonym_species_names() {
        let rule = PortugueseWordRepeatRule::new();
        let sentence = make_sentence("Vulpes vulpes é uma raposa.");
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn ignores_reduplicated_adverbs() {
        let rule = PortugueseWordRepeatRule::new();
        let sentence = make_sentence("Logo logo vamos sair.");
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn ignores_blá_blá_and_se_se() {
        let rule = PortugueseWordRepeatRule::new();
        assert!(
            rule.match_sentence(&make_sentence("O blá blá continua."))
                .is_empty()
        );
        let sentence = make_sentence("Caso se se confirme, aviso.");
        assert!(rule.match_sentence(&sentence).is_empty());
    }

    #[test]
    fn flags_plain_repetition() {
        let rule = PortugueseWordRepeatRule::new();
        let sentence = make_sentence("Ele disse isso isso ontem.");
        let matches = rule.match_sentence(&sentence);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn beginning_rule_matches_java_example_pair() {
        let rule = PortugueseWordRepeatBeginningRule::new();
        let s1 = make_sentence("Além disso, a rua é quase completamente residêncial.");
        let s2 = make_sentence("Além disso, foi chamada em nome de um poeta.");
        let text = format!("{} {}", s1.text(), s2.text());
        let matches = rule.match_text(&text, &[s1, s2]);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].short_message().unwrap().contains("adverb"));
    }

    #[test]
    fn beginning_rule_requires_ending_sentence_before() {
        let rule = PortugueseWordRepeatBeginningRule::new();
        let s1 = make_sentence("Além disso, a rua");
        let s2 = make_sentence("Além disso, foi chamada");
        let text = format!("{} {}", s1.text(), s2.text());
        assert!(rule.match_text(&text, &[s1, s2]).is_empty());
    }
}
