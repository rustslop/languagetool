use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static EVENTS: RefCell<Vec<TraceEvent>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug, Clone)]
pub struct TraceEvent {
    pub pattern_idx: usize,
    pub token_idx: usize,
    pub token: String,
    pub cause: String,
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn clear() {
    EVENTS.with(|events| events.borrow_mut().clear());
}

pub fn take() -> Vec<TraceEvent> {
    EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

pub fn record(pattern_idx: usize, token_idx: usize, token: &str, cause: String) {
    if !enabled() {
        return;
    }
    EVENTS.with(|events| {
        events.borrow_mut().push(TraceEvent {
            pattern_idx,
            token_idx,
            token: token.to_string(),
            cause,
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_is_gated_by_the_enabled_flag() {
        set_enabled(false);
        clear();
        record(0, 0, "tok", "cause".to_string());
        assert!(take().is_empty());
        set_enabled(true);
        record(1, 2, "word", "postag mismatch".to_string());
        let events = take();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].pattern_idx, 1);
        assert_eq!(events[0].token_idx, 2);
        assert_eq!(events[0].token, "word");
        assert_eq!(events[0].cause, "postag mismatch");
        assert!(take().is_empty(), "take drains the buffer");
        set_enabled(false);
        assert!(!enabled());
    }

    #[test]
    fn clear_drops_buffered_events() {
        set_enabled(true);
        record(0, 0, "a", "x".to_string());
        record(0, 1, "b", "y".to_string());
        clear();
        assert!(take().is_empty());
        set_enabled(false);
    }

    #[test]
    fn enabled_round_trips() {
        set_enabled(true);
        assert!(enabled());
        set_enabled(false);
        assert!(!enabled());
    }
}
