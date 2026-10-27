use og_api::server::{AppState, shared_state};
use std::sync::Arc;

#[test]
fn shared_state_collapses_variant_families_into_one_engine() {
    let state = shared_state();
    let en = state.engine_for("en").expect("en engine");
    let en_us = state.engine_for("en-US").expect("en-US engine");
    assert!(
        Arc::ptr_eq(&en, &en_us),
        "en and en-US must share the canonical 'en' engine"
    );
    let de = state.engine_for("de").expect("de engine");
    let de_de = state.engine_for("de-DE").expect("de-DE engine");
    assert!(
        Arc::ptr_eq(&de, &de_de),
        "de and de-DE must share the canonical 'de' engine"
    );
    assert!(!Arc::ptr_eq(&en, &de));
    assert!(state.engine_for("xx").is_none());
}

#[test]
fn concurrent_engine_for_builds_each_code_once() {
    let state = AppState::new();
    let state = &state;
    std::thread::scope(|scope| {
        let handles: Vec<_> = ["da", "da", "da", "da"]
            .into_iter()
            .map(|code| scope.spawn(move || state.engine_for(code).expect("da engine")))
            .collect();
        let engines: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        for engine in &engines[1..] {
            assert!(
                Arc::ptr_eq(&engines[0], engine),
                "every concurrent builder must observe the same engine instance"
            );
        }
    });
}
