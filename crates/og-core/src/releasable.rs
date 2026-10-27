use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

struct CellState<T> {
    value: Option<Arc<T>>,
    last_use: Option<Instant>,
}

pub struct Releasable<T> {
    state: Mutex<CellState<T>>,
    building: Mutex<()>,
}

impl<T> Releasable<T> {
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(CellState {
                value: None,
                last_use: None,
            }),
            building: Mutex::new(()),
        }
    }
}

impl<T> Default for Releasable<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub type ReleaseFn = std::sync::Arc<dyn Fn(Option<Instant>) -> usize + Send + Sync>;

fn registry() -> &'static Mutex<HashMap<usize, ReleaseFn>> {
    static REGISTRY: OnceLock<Mutex<HashMap<usize, ReleaseFn>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn register_releaser(key: usize, release: ReleaseFn) {
    if let Ok(mut registry) = registry().lock() {
        registry.entry(key).or_insert(release);
    }
}

pub fn shared_probe<T>(cell: &'static Releasable<T>) -> Option<Arc<T>>
where
    T: Send + Sync + 'static,
{
    let now = Instant::now();
    let mut guard = cell.state.lock().ok()?;
    let hit = guard.value.clone()?;
    guard.last_use = Some(now);
    Some(hit)
}

pub fn shared_get_or_build<T>(
    cell: &'static Releasable<T>,
    build: impl FnOnce() -> Option<Arc<T>>,
) -> Option<Arc<T>>
where
    T: Send + Sync + 'static,
{
    let now = Instant::now();
    if let Ok(mut guard) = cell.state.lock()
        && guard.value.is_some()
    {
        let hit = guard.value.clone();
        guard.last_use = Some(now);
        return hit;
    }
    let _build_guard = cell
        .building
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Ok(mut guard) = cell.state.lock()
        && guard.value.is_some()
    {
        let hit = guard.value.clone();
        guard.last_use = Some(now);
        return hit;
    }
    let built = build();
    let mut stored = false;
    let outcome = {
        let Ok(mut guard) = cell.state.lock() else {
            return built;
        };
        if guard.value.is_none() {
            guard.value = built.clone();
            guard.last_use = Some(now);
            stored = true;
        }
        guard.value.clone()
    };
    if stored {
        let key = cell as *const Releasable<T> as usize;
        if let Ok(mut registry) = registry().lock() {
            registry.entry(key).or_insert_with(|| {
                std::sync::Arc::new(move |keep_used_since| release_cell(cell, keep_used_since))
            });
        }
    }
    outcome
}

fn release_cell<T>(cell: &Releasable<T>, keep_used_since: Option<Instant>) -> usize {
    let _dropped = {
        let Ok(mut guard) = cell.state.lock() else {
            return 0;
        };
        if let Some(since) = keep_used_since
            && guard.last_use.is_some_and(|used| used > since)
        {
            return 0;
        }
        guard.value.take()
    };
    usize::from(_dropped.is_some())
}

pub fn release_all() -> usize {
    release_with_filter(None)
}

pub fn release_unused_since(since: Instant) -> usize {
    release_with_filter(Some(since))
}

pub fn trim_due(interval: std::time::Duration) -> bool {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let Ok(mut guard) = LAST.lock() else {
        return false;
    };
    let now = Instant::now();
    if guard.is_some_and(|last| now.duration_since(last) < interval) {
        return false;
    }
    *guard = Some(now);
    true
}

fn release_with_filter(keep_used_since: Option<Instant>) -> usize {
    let releases: Vec<ReleaseFn> = match registry().lock() {
        Ok(registry) => registry.values().cloned().collect(),
        Err(_) => return 0,
    };
    releases
        .iter()
        .map(|release| release(keep_used_since))
        .sum()
}

thread_local! {
    static HEAVY_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

const HEAVY_PERMITS: usize = 3;

static HEAVY_AVAILABLE: Mutex<usize> = Mutex::new(HEAVY_PERMITS);
static HEAVY_CV: std::sync::Condvar = std::sync::Condvar::new();

pub struct HeavyGuard {
    acquired: bool,
}

impl HeavyGuard {
    fn release(&mut self) {
        if !self.acquired {
            HEAVY_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
            return;
        }
        self.acquired = false;
        HEAVY_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
        if let Ok(mut available) = HEAVY_AVAILABLE.lock() {
            *available += 1;
            HEAVY_CV.notify_one();
        }
    }
}

impl Drop for HeavyGuard {
    fn drop(&mut self) {
        self.release();
    }
}

// Reentrant on purpose: heavyweight tests hold this across engine builds on the same thread.
pub fn heavy_guard() -> HeavyGuard {
    let mut acquired = false;
    if HEAVY_DEPTH.with(|depth| depth.get()) == 0 {
        let mut available = HEAVY_AVAILABLE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *available == 0 {
            available = HEAVY_CV
                .wait(available)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *available -= 1;
        acquired = true;
    }
    HEAVY_DEPTH.with(|depth| depth.set(depth.get() + 1));
    HeavyGuard { acquired }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    static SERIALIZE: Mutex<()> = Mutex::new(());

    #[test]
    fn builds_once_and_serves_from_cache() {
        let _serialized = SERIALIZE.lock().unwrap();
        static CELL: Releasable<String> = Releasable::new();
        static BUILDS: AtomicUsize = AtomicUsize::new(0);
        let first = shared_get_or_build(&CELL, || {
            BUILDS.fetch_add(1, Ordering::SeqCst);
            Some(Arc::new("value".to_string()))
        })
        .unwrap();
        let second = shared_get_or_build(&CELL, || {
            BUILDS.fetch_add(1, Ordering::SeqCst);
            Some(Arc::new("other".to_string()))
        })
        .unwrap();
        assert_eq!(BUILDS.load(Ordering::SeqCst), 1);
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn failed_builds_are_not_cached() {
        let _serialized = SERIALIZE.lock().unwrap();
        static CELL: Releasable<String> = Releasable::new();
        static BUILDS: AtomicUsize = AtomicUsize::new(0);
        let build = || {
            BUILDS.fetch_add(1, Ordering::SeqCst);
            None
        };
        assert!(shared_get_or_build(&CELL, build).is_none());
        assert!(shared_get_or_build(&CELL, build).is_none());
        assert_eq!(BUILDS.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn release_all_drops_cached_values_but_keeps_live_arcs() {
        let _serialized = SERIALIZE.lock().unwrap();
        static CELL: Releasable<String> = Releasable::new();
        static BUILDS: AtomicUsize = AtomicUsize::new(0);
        let held = shared_get_or_build(&CELL, || {
            BUILDS.fetch_add(1, Ordering::SeqCst);
            Some(Arc::new("first".to_string()))
        })
        .unwrap();
        release_all();
        assert!(CELL.state.lock().unwrap().value.is_none());
        let rebuilt = shared_get_or_build(&CELL, || {
            BUILDS.fetch_add(1, Ordering::SeqCst);
            Some(Arc::new("second".to_string()))
        })
        .unwrap();
        assert_eq!(BUILDS.load(Ordering::SeqCst), 2);
        assert!(!Arc::ptr_eq(&held, &rebuilt));
        assert_eq!(*held, "first");
        assert_eq!(*rebuilt, "second");
    }

    #[test]
    fn release_unused_since_keeps_cells_used_after_the_instant() {
        let _serialized = SERIALIZE.lock().unwrap();
        static USED: Releasable<String> = Releasable::new();
        static IDLE: Releasable<String> = Releasable::new();
        static USED_BUILDS: AtomicUsize = AtomicUsize::new(0);
        static IDLE_BUILDS: AtomicUsize = AtomicUsize::new(0);
        let _idle = shared_get_or_build(&IDLE, || {
            IDLE_BUILDS.fetch_add(1, Ordering::SeqCst);
            Some(Arc::new("idle".to_string()))
        })
        .unwrap();
        let since = Instant::now();
        std::thread::sleep(Duration::from_millis(2));
        let used = shared_get_or_build(&USED, || {
            USED_BUILDS.fetch_add(1, Ordering::SeqCst);
            Some(Arc::new("used".to_string()))
        })
        .unwrap();
        release_unused_since(since);
        assert!(
            USED.state.lock().unwrap().value.is_some(),
            "a cell used after the instant must survive"
        );
        assert!(
            IDLE.state.lock().unwrap().value.is_none(),
            "a cell idle since before the instant must be released"
        );
        let again = shared_get_or_build(&USED, || {
            USED_BUILDS.fetch_add(1, Ordering::SeqCst);
            Some(Arc::new("rebuilt".to_string()))
        })
        .unwrap();
        assert!(Arc::ptr_eq(&used, &again));
        assert_eq!(USED_BUILDS.load(Ordering::SeqCst), 1);
        assert_eq!(IDLE_BUILDS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_callers_build_exactly_once() {
        let _serialized = SERIALIZE.lock().unwrap();
        static CELL: Releasable<Vec<usize>> = Releasable::new();
        static BUILDS: AtomicUsize = AtomicUsize::new(0);
        let results: Mutex<Vec<Arc<Vec<usize>>>> = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for t in 0..4 {
                let results = &results;
                scope.spawn(move || {
                    let value = shared_get_or_build(&CELL, || {
                        BUILDS.fetch_add(1, Ordering::SeqCst);
                        Some(Arc::new(vec![t]))
                    })
                    .unwrap();
                    results.lock().unwrap().push(value);
                });
            }
        });
        let results = results.into_inner().unwrap();
        assert_eq!(BUILDS.load(Ordering::SeqCst), 1);
        assert_eq!(results.len(), 4);
        assert!(results.iter().all(|r| Arc::ptr_eq(r, &results[0])));
    }

    #[test]
    fn heavy_guard_is_reentrant_on_the_same_thread() {
        let outer = heavy_guard();
        let inner = heavy_guard();
        drop(inner);
        drop(outer);
        let fresh = heavy_guard();
        drop(fresh);
    }

    #[test]
    fn heavy_guard_blocks_callers_beyond_the_permit_budget() {
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let release_rx = std::sync::Arc::new(std::sync::Mutex::new(release_rx));
        let holders: Vec<_> = (0..HEAVY_PERMITS)
            .map(|_| {
                let release_rx = std::sync::Arc::clone(&release_rx);
                let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
                let handle = std::thread::spawn(move || {
                    let _guard = heavy_guard();
                    held_tx.send(()).unwrap();
                    let _ = release_rx.lock().unwrap().recv();
                });
                (held_rx, handle)
            })
            .collect();
        for (held_rx, _) in &holders {
            held_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("each holder thread must acquire a permit");
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let _guard = heavy_guard();
            tx.send(()).unwrap();
        });
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "a caller beyond the permit budget must wait while all permits are held"
        );
        drop(release_tx);
        for (_, holder) in holders {
            holder.join().unwrap();
        }
        rx.recv_timeout(Duration::from_secs(5))
            .expect("releasing the permits must let the waiting caller through");
        handle.join().unwrap();
    }

    #[test]
    fn poisoned_cell_lock_returns_the_build_without_caching() {
        let _serialized = SERIALIZE.lock().unwrap();
        static CELL: Releasable<String> = Releasable::new();
        static BUILDS: AtomicUsize = AtomicUsize::new(0);
        let poisoned = std::thread::spawn(move || {
            let _guard = CELL.state.lock().unwrap();
            panic!("poison the cell lock");
        })
        .join();
        assert!(poisoned.is_err());
        let first = shared_get_or_build(&CELL, || {
            BUILDS.fetch_add(1, Ordering::SeqCst);
            Some(Arc::new("a".to_string()))
        });
        let second = shared_get_or_build(&CELL, || {
            BUILDS.fetch_add(1, Ordering::SeqCst);
            Some(Arc::new("b".to_string()))
        });
        assert_eq!(first.as_deref().map(String::as_str), Some("a"));
        assert_eq!(second.as_deref().map(String::as_str), Some("b"));
        assert_eq!(BUILDS.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn default_constructs_an_empty_cell_usable_at_runtime() {
        let _serialized = SERIALIZE.lock().unwrap();
        static FROM_DEFAULT: std::sync::OnceLock<&'static Releasable<String>> =
            std::sync::OnceLock::new();
        static FROM_NEW: std::sync::OnceLock<&'static Releasable<String>> =
            std::sync::OnceLock::new();
        let default_cell = FROM_DEFAULT.get_or_init(|| Box::leak(Box::new(Releasable::default())));
        let new_cell = FROM_NEW.get_or_init(|| Box::leak(Box::new(Releasable::new())));
        for cell in [default_cell, new_cell] {
            assert!(cell.state.lock().unwrap().value.is_none());
            let value =
                shared_get_or_build(cell, || Some(Arc::new("runtime".to_string()))).unwrap();
            assert_eq!(*value, "runtime");
            assert!(cell.state.lock().unwrap().value.is_some());
        }
    }

    #[test]
    fn release_with_poisoned_state_lock_keeps_the_cached_value() {
        let _serialized = SERIALIZE.lock().unwrap();
        static CELL: Releasable<String> = Releasable::new();
        let value = shared_get_or_build(&CELL, || Some(Arc::new("kept".to_string()))).unwrap();
        let poisoned = std::thread::spawn(move || {
            let _guard = CELL.state.lock().unwrap();
            panic!("poison the cell state lock");
        })
        .join();
        assert!(poisoned.is_err());
        release_all();
        let guard = CELL
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(
            guard.value.is_some(),
            "a poisoned cell lock must not clear the cached value"
        );
        assert_eq!(*value, "kept");
    }

    #[test]
    fn poisoned_build_lock_recovers_and_caches() {
        let _serialized = SERIALIZE.lock().unwrap();
        static CELL: Releasable<String> = Releasable::new();
        static BUILDS: AtomicUsize = AtomicUsize::new(0);
        let poisoned = std::thread::spawn(move || {
            let _guard = CELL.building.lock().unwrap();
            panic!("poison the build lock");
        })
        .join();
        assert!(poisoned.is_err());
        let first = shared_get_or_build(&CELL, || {
            BUILDS.fetch_add(1, Ordering::SeqCst);
            Some(Arc::new("a".to_string()))
        })
        .unwrap();
        let second = shared_get_or_build(&CELL, || {
            BUILDS.fetch_add(1, Ordering::SeqCst);
            Some(Arc::new("b".to_string()))
        })
        .unwrap();
        assert_eq!(BUILDS.load(Ordering::SeqCst), 1);
        assert!(Arc::ptr_eq(&first, &second));
    }
}
