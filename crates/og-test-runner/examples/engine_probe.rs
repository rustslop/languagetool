use og_langs::engine::LanguageEngine;
use std::sync::Arc;

fn main() {
    let texts: Vec<&str> = vec![
        "An actress’s role",
        "Part of what makes the ADT Smarthings panel so appealing is it’s reliability for the security alarm.",
        "This sound slike an error.",
        "The air get's colder.",
        "What else she doing?",
    ];
    let engine = Arc::new(LanguageEngine::english());
    og_rules::pattern_trace::set_enabled(true);
    let request = og_core::CheckRequest::new("", og_core::Language::English);
    for text in texts {
        let mut req = request.clone();
        req.text = text.to_string();
        req.picky = true;
        let result = engine.check_with_request(&req);
        println!("TEXT: {text:?}");
        for ev in og_rules::pattern_trace::take().iter().take(60) {
            println!(
                "  TRACE elem={} tok={} token={:?} cause={}",
                ev.pattern_idx, ev.token_idx, ev.token, ev.cause
            );
        }
        og_rules::pattern_trace::clear();
        for m in &result.matches {
            println!(
                "  {} {}..{} {:?}",
                m.rule().id(),
                m.offset(),
                m.offset() + m.length(),
                m.replacements()
                    .iter()
                    .map(|r| r.value().to_string())
                    .collect::<Vec<_>>()
            );
        }
    }
}
