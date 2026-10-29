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

trait CellReleaser: Send + Sync {
    fn release(&self, keep_used_since: Option<Instant>) -> usize;
    fn last_use(&self) -> Option<Instant>;
    fn has_value(&self) -> bool;
}

struct StaticCell<T: 'static> {
    cell: &'static Releasable<T>,
}

impl<T> CellReleaser for StaticCell<T>
where
    T: Send + Sync + 'static,
{
    fn release(&self, keep_used_since: Option<Instant>) -> usize {
        release_cell(self.cell, keep_used_since)
    }

    fn last_use(&self) -> Option<Instant> {
        self.cell.state.lock().ok().and_then(|guard| guard.last_use)
    }

    fn has_value(&self) -> bool {
        self.cell
            .state
            .lock()
            .is_ok_and(|guard| guard.value.is_some())
    }
}

fn registry() -> &'static Mutex<HashMap<usize, Arc<dyn CellReleaser>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<usize, Arc<dyn CellReleaser>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

static EVICTION_BUDGET: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);
static WAIT_WATERMARK: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);

pub fn set_eviction_budget_bytes(bytes: usize) {
    EVICTION_BUDGET.store(bytes, std::sync::atomic::Ordering::Release);
}

pub fn eviction_budget_bytes() -> usize {
    EVICTION_BUDGET.load(std::sync::atomic::Ordering::Acquire)
}

pub fn set_wait_watermark_bytes(bytes: usize) {
    WAIT_WATERMARK.store(bytes, std::sync::atomic::Ordering::Release);
}

pub fn wait_watermark_bytes() -> usize {
    WAIT_WATERMARK.load(std::sync::atomic::Ordering::Acquire)
}

pub type ReleaseFn = std::sync::Arc<dyn Fn(Option<Instant>) -> usize + Send + Sync>;

struct FnCell {
    release: ReleaseFn,
}

impl CellReleaser for FnCell {
    fn release(&self, keep_used_since: Option<Instant>) -> usize {
        (self.release)(keep_used_since)
    }

    fn last_use(&self) -> Option<Instant> {
        None
    }

    fn has_value(&self) -> bool {
        false
    }
}

pub fn register_releaser(key: usize, release: ReleaseFn) {
    if let Ok(mut registry) = registry().lock() {
        registry
            .entry(key)
            .or_insert_with(|| Arc::new(FnCell { release }));
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
    drop(guard);
    maybe_evict_over_budget();
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
        drop(guard);
        maybe_evict_over_budget();
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
    evict_lru_until_rss_below(eviction_budget_bytes(), None);
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
            registry
                .entry(key)
                .or_insert_with(|| Arc::new(StaticCell { cell }));
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
    let releases: Vec<Arc<dyn CellReleaser>> = match registry().lock() {
        Ok(registry) => registry.values().cloned().collect(),
        Err(_) => return 0,
    };
    releases
        .iter()
        .map(|release| release.release(keep_used_since))
        .sum()
}

#[cfg(target_os = "linux")]
pub fn resident_bytes() -> usize {
    let Ok(statm) = std::fs::read_to_string("/proc/self/statm") else {
        return usize::MAX;
    };
    let Some(pages) = statm.split_whitespace().nth(1).and_then(|pages| {
        pages
            .parse::<usize>()
            .ok()
            .map(|pages| pages.saturating_mul(page_size()))
    }) else {
        return usize::MAX;
    };
    if pages == 0 {
        return usize::MAX;
    }
    pages
}

#[cfg(target_os = "linux")]
fn page_size() -> usize {
    static PAGE_SIZE: OnceLock<usize> = OnceLock::new();
    *PAGE_SIZE.get_or_init(|| {
        let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if size <= 0 { 4096 } else { size as usize }
    })
}

#[cfg(not(target_os = "linux"))]
pub fn resident_bytes() -> usize {
    usize::MAX
}

fn evictable_cells(protect_after: Option<Instant>) -> Vec<Arc<dyn CellReleaser>> {
    let cells: Vec<Arc<dyn CellReleaser>> = match registry().lock() {
        Ok(registry) => registry.values().cloned().collect(),
        Err(_) => return Vec::new(),
    };
    let mut cells: Vec<_> = cells
        .into_iter()
        .filter(|cell| {
            cell.has_value()
                && cell
                    .last_use()
                    .is_some_and(|used| protect_after.is_none_or(|protect| used <= protect))
        })
        .collect();
    cells.sort_by_key(|cell| cell.last_use());
    cells
}

pub fn evict_lru(protect_after: Option<Instant>) -> bool {
    match evictable_cells(protect_after).into_iter().next() {
        Some(victim) => victim.release(protect_after) > 0,
        None => false,
    }
}

#[cfg(target_os = "linux")]
fn trim_released_pages() {
    unsafe {
        libc::malloc_trim(0);
    }
}

#[cfg(not(target_os = "linux"))]
fn trim_released_pages() {}

pub fn evict_lru_until_rss_below(budget_bytes: usize, protect_after: Option<Instant>) -> usize {
    if resident_bytes() <= budget_bytes {
        return 0;
    }
    let mut evicted = 0usize;
    for victim in evictable_cells(protect_after) {
        if resident_bytes() <= budget_bytes {
            break;
        }
        if victim.release(protect_after) > 0 {
            // glibc keeps freed heap pages resident until a trim, so without this
            // the loop cannot observe its own progress and evicts every victim.
            trim_released_pages();
            evicted += 1;
        }
    }
    evicted
}

const PROBE_EVICT_GRACE: std::time::Duration = std::time::Duration::from_secs(30);
const PROBE_EVICT_INTERVAL_MS: u64 = 1000;

const HEADROOM_WAIT_SLICE_MS: u64 = 25;
const HEADROOM_WAIT_MAX_MS: u64 = 1_000;

fn wait_for_headroom() {
    let watermark = wait_watermark_bytes();
    if watermark == usize::MAX
        || HEAVY_DEPTH.with(|depth| depth.get()) > 0
        || resident_bytes() <= watermark
    {
        return;
    }
    // Sleeping cannot free pages; only a trim by a concurrent releaser can, so
    // wait just long enough for an in-flight gated test to drop its engines.
    let mut waited = 0u64;
    while waited < HEADROOM_WAIT_MAX_MS {
        std::thread::sleep(std::time::Duration::from_millis(HEADROOM_WAIT_SLICE_MS));
        waited += HEADROOM_WAIT_SLICE_MS;
        if resident_bytes() <= watermark {
            break;
        }
    }
}

pub fn maybe_evict_over_budget() {
    static LAST_CHECK_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let budget = eviction_budget_bytes();
    if budget == usize::MAX {
        return;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0);
    let last = LAST_CHECK_MS.load(std::sync::atomic::Ordering::Relaxed);
    if last != 0 && now_ms.saturating_sub(last) < PROBE_EVICT_INTERVAL_MS {
        return;
    }
    if LAST_CHECK_MS
        .compare_exchange(
            last,
            now_ms,
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
        )
        .is_err()
    {
        return;
    }
    evict_over_budget_with(budget, PROBE_EVICT_GRACE);
    wait_for_headroom();
}

fn evict_over_budget_with(budget_bytes: usize, grace: std::time::Duration) {
    if resident_bytes() > budget_bytes {
        let protect_after = Instant::now().checked_sub(grace);
        evict_lru_until_rss_below(budget_bytes, protect_after);
    }
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

    #[test]
    fn evict_lru_drops_the_oldest_cell_first() {
        let _serialized = SERIALIZE.lock().unwrap();
        static OLDEST: Releasable<String> = Releasable::new();
        static MIDDLE: Releasable<String> = Releasable::new();
        static NEWEST: Releasable<String> = Releasable::new();
        release_all();
        shared_get_or_build(&OLDEST, || Some(Arc::new("oldest".to_string()))).unwrap();
        std::thread::sleep(Duration::from_millis(2));
        shared_get_or_build(&MIDDLE, || Some(Arc::new("middle".to_string()))).unwrap();
        std::thread::sleep(Duration::from_millis(2));
        shared_get_or_build(&NEWEST, || Some(Arc::new("newest".to_string()))).unwrap();
        assert!(evict_lru(None));
        assert!(OLDEST.state.lock().unwrap().value.is_none());
        assert!(MIDDLE.state.lock().unwrap().value.is_some());
        assert!(NEWEST.state.lock().unwrap().value.is_some());
        assert!(evict_lru(None));
        assert!(MIDDLE.state.lock().unwrap().value.is_none());
        assert!(NEWEST.state.lock().unwrap().value.is_some());
        assert!(evict_lru(None));
        assert!(NEWEST.state.lock().unwrap().value.is_none());
        assert!(!evict_lru(None));
    }

    #[test]
    fn evict_lru_until_rss_below_keeps_cells_used_after_the_protect_instant() {
        let _serialized = SERIALIZE.lock().unwrap();
        static OLD: Releasable<String> = Releasable::new();
        static FRESH: Releasable<String> = Releasable::new();
        shared_get_or_build(&OLD, || Some(Arc::new("old".to_string()))).unwrap();
        let protect = Instant::now();
        std::thread::sleep(Duration::from_millis(2));
        let fresh = shared_get_or_build(&FRESH, || Some(Arc::new("fresh".to_string()))).unwrap();
        let evicted = evict_lru_until_rss_below(0, Some(protect));
        assert!(evicted >= 1);
        assert!(
            OLD.state.lock().unwrap().value.is_none(),
            "the cell last used before the protect instant must be evicted"
        );
        assert!(
            FRESH.state.lock().unwrap().value.is_some(),
            "the cell last used after the protect instant must survive"
        );
        assert_eq!(*fresh, "fresh");
    }

    #[test]
    fn evict_lru_until_rss_below_skips_eviction_under_an_unreachable_budget() {
        let _serialized = SERIALIZE.lock().unwrap();
        static CELL: Releasable<String> = Releasable::new();
        shared_get_or_build(&CELL, || Some(Arc::new("kept".to_string()))).unwrap();
        assert_eq!(evict_lru_until_rss_below(usize::MAX, None), 0);
        assert!(CELL.state.lock().unwrap().value.is_some());
    }

    #[test]
    fn resident_bytes_reports_a_live_process_footprint() {
        let resident = resident_bytes();
        assert!(resident > 0);
        assert!(resident < usize::MAX);
    }

    #[test]
    fn evict_over_budget_with_spares_cells_used_within_the_grace() {
        let _serialized = SERIALIZE.lock().unwrap();
        static OLD: Releasable<String> = Releasable::new();
        static FRESH: Releasable<String> = Releasable::new();
        set_eviction_budget_bytes(0);
        shared_get_or_build(&OLD, || Some(Arc::new("old".to_string()))).unwrap();
        std::thread::sleep(Duration::from_millis(4));
        let fresh = shared_get_or_build(&FRESH, || Some(Arc::new("fresh".to_string()))).unwrap();
        evict_over_budget_with(0, Duration::from_millis(2));
        assert!(
            OLD.state.lock().unwrap().value.is_none(),
            "a cell idle past the grace must be evicted under budget pressure"
        );
        assert!(
            FRESH.state.lock().unwrap().value.is_some(),
            "a cell used within the grace must survive"
        );
        assert_eq!(*fresh, "fresh");
        set_eviction_budget_bytes(usize::MAX);
    }
}
