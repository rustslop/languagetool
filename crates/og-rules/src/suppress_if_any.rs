use og_core::rule::Rule;
use og_core::{AnalyzedSentence, AnalyzedToken, AnalyzedTokenReadings, RuleMatch};
use std::sync::Arc;

pub trait SuppressIfAnySupport: Send + Sync {
    fn analyze_text(&self, text: &str) -> Vec<AnalyzedSentence>;
    fn rules_for_ids(&self, rule_ids: &[&str]) -> Vec<Arc<dyn Rule>>;
}

struct DepthGuard;

impl DepthGuard {
    fn enter() -> Option<Self> {
        DEPTH.with(|depth| {
            if depth.get() > 0 {
                None
            } else {
                depth.set(1);
                Some(DepthGuard)
            }
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(0));
    }
}

thread_local! {
    static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

pub fn accept_rule_match(
    support: &dyn SuppressIfAnySupport,
    tagger: &dyn og_tagger::Tagger,
    rule_ids: &[&str],
    fast_only: bool,
    sentence: &AnalyzedSentence,
    rm: RuleMatch,
) -> Option<RuleMatch> {
    let Some(_guard) = DepthGuard::enter() else {
        return Some(rm);
    };
    let base = sentence.start();
    let from = rm.offset().saturating_sub(base);
    let to = from + rm.length();
    let text = sentence.text();
    for repl in rm.replacements() {
        let replacement = repl.value();
        if let Some(modified) = replace_token_in_analyzed_sentence(
            sentence,
            rm.offset(),
            to - from,
            replacement,
            tagger,
        ) {
            if any_rule_matches(support, rule_ids, &modified, from, to) {
                return None;
            }
            if fast_only {
                continue;
            }
        }
        if !text.is_char_boundary(from) || !text.is_char_boundary(to) {
            continue;
        }
        let new_sentence = format!("{}{}{}", &text[..from], replacement, &text[to..]);
        if let Some(analyzed) = support.analyze_text(&new_sentence).into_iter().next()
            && any_rule_matches(support, rule_ids, &analyzed, from, to)
        {
            return None;
        }
    }
    Some(rm)
}

fn any_rule_matches(
    support: &dyn SuppressIfAnySupport,
    rule_ids: &[&str],
    sentence: &AnalyzedSentence,
    from: usize,
    to: usize,
) -> bool {
    let base = sentence.start();
    for rule in support.rules_for_ids(rule_ids) {
        for m in rule.match_sentence(sentence) {
            let m_from = m.offset().saturating_sub(base);
            let m_to = m_from + m.length();
            if (m_to >= from && m_to <= to) || (to >= m_from && to <= m_to) {
                return true;
            }
        }
    }
    false
}

fn replace_token_in_analyzed_sentence(
    sentence: &AnalyzedSentence,
    from_abs: usize,
    length: usize,
    replacement: &str,
    tagger: &dyn og_tagger::Tagger,
) -> Option<AnalyzedSentence> {
    if replacement.len() != length || replacement.contains(' ') {
        return None;
    }
    let tokens = sentence.tokens();
    let idx = tokens
        .iter()
        .position(|t| !t.is_whitespace() && t.token().start() == from_abs)?;
    let tagged = tagger.tag(&[replacement]);
    if tagged.len() != 1 {
        return None;
    }
    let readings = tagged
        .into_iter()
        .next()
        .map(AnalyzedTokenReadings::into_readings)
        .unwrap_or_default();
    let old = &tokens[idx];
    let mut new_token = AnalyzedToken::new(replacement, old.token().start(), old.token().end());
    if old.token().pos_tags().iter().any(|t| t == "SENT_END") {
        new_token = new_token.with_pos_tags(vec!["SENT_END".to_string()]);
    }
    let mut new_atr = AnalyzedTokenReadings::new(new_token).with_readings(readings);
    new_atr.set_chunks(old.chunks().to_vec());
    if old.is_ignored_by_speller() {
        new_atr.ignore_spelling();
    }
    if old.is_immunized() {
        new_atr.immunize();
    }
    let mut new_tokens = tokens.to_vec();
    new_tokens[idx] = new_atr;
    let mut modified = AnalyzedSentence::new(sentence.text(), sentence.start(), sentence.end());
    modified.set_tokens(new_tokens);
    let pre = sentence.pre_disambiguation_tokens();
    if !pre.is_empty() {
        let mut new_pre = pre.to_vec();
        new_pre[idx] = modified.tokens()[idx].clone();
        modified.set_pre_disambiguation_tokens(new_pre);
    }
    Some(modified)
}
