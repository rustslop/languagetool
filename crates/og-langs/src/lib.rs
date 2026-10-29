pub mod en;
#[path = "en-module-refresh/mod.rs"]
pub mod en_module_refresh;
pub mod engine;
pub mod registry;

pub use en::*;
pub use engine::*;
pub use registry::*;

#[cfg(all(test, target_os = "linux"))]
#[unsafe(link_section = ".init_array")]
#[used]
static BOUND_TEST_ALLOCATOR: extern "C" fn() = {
    extern "C" fn bound() {
        unsafe {
            libc::mallopt(libc::M_ARENA_MAX, 6);
            libc::mallopt(libc::M_MMAP_THRESHOLD, 2 * 1024 * 1024);
            libc::mallopt(libc::M_TRIM_THRESHOLD, 4 * 1024 * 1024);
        }
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(6)
            .build_global();
    }
    bound
};

#[cfg(test)]
pub(crate) fn release_guard() -> impl Drop {
    struct ReleaseOnDrop {
        _gate: HeavyTestGate,
        _heavy: og_core::releasable::HeavyGuard,
        acquired: std::time::Instant,
    }
    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            const ENGINE_IDLE_GRACE: std::time::Duration = std::time::Duration::from_secs(120);
            const TRIM_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);
            let keep_used_since = std::time::Instant::now()
                .checked_sub(ENGINE_IDLE_GRACE)
                .map_or(self.acquired, |floor| self.acquired.max(floor));
            let freed = og_core::releasable::release_unused_since(keep_used_since);
            let evicted = og_core::releasable::evict_lru_until_rss_below(
                test_rss_budget(),
                Some(self.acquired),
            );
            #[cfg(target_os = "linux")]
            if (freed > 0 || evicted > 0) && og_core::releasable::trim_due(TRIM_INTERVAL) {
                unsafe {
                    libc::malloc_trim(0);
                }
            }
        }
    }
    let budget = test_rss_budget();
    og_core::releasable::set_eviction_budget_bytes(budget);
    og_core::releasable::set_wait_watermark_bytes(test_rss_wait_watermark());
    ReleaseOnDrop {
        _gate: heavy_test_gate(),
        _heavy: og_core::releasable::heavy_guard(),
        acquired: std::time::Instant::now(),
    }
}

#[cfg(test)]
struct HeavyTestGate {
    acquired: bool,
}

#[cfg(test)]
impl Drop for HeavyTestGate {
    fn drop(&mut self) {
        if !self.acquired {
            HEAVY_TEST_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
            return;
        }
        self.acquired = false;
        HEAVY_TEST_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
        if let Ok(mut available) = HEAVY_TEST_AVAILABLE.lock() {
            *available += 1;
            HEAVY_TEST_CV.notify_one();
        }
    }
}

#[cfg(test)]
thread_local! {
    static HEAVY_TEST_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
static HEAVY_TEST_AVAILABLE: std::sync::Mutex<usize> = std::sync::Mutex::new(0);

#[cfg(test)]
static HEAVY_TEST_CV: std::sync::Condvar = std::sync::Condvar::new();

#[cfg(test)]
fn heavy_test_permits() -> usize {
    const SECOND_PERMIT_FLOOR_MB: usize = 3 * 1024;
    static PERMITS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *PERMITS.get_or_init(|| {
        std::env::var("OG_LANGS_TEST_HEAVY_PERMITS")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|&permits| permits > 0)
            .unwrap_or_else(|| {
                let floor = SECOND_PERMIT_FLOOR_MB * 1024 * 1024;
                1 + usize::from(
                    og_core::releasable::available_bytes().is_some_and(|available| {
                        available < usize::MAX
                            && og_core::releasable::resident_bytes().saturating_add(available)
                                >= floor
                    }),
                )
            })
    })
}

#[cfg(test)]
fn ensure_heavy_test_permits() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        if let Ok(mut available) = HEAVY_TEST_AVAILABLE.lock() {
            *available = heavy_test_permits();
        }
    });
}

#[cfg(test)]
fn heavy_test_gate() -> HeavyTestGate {
    ensure_heavy_test_permits();
    let mut acquired = false;
    if HEAVY_TEST_DEPTH.with(|depth| depth.get()) == 0 {
        let mut available = HEAVY_TEST_AVAILABLE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *available == 0 {
            available = HEAVY_TEST_CV
                .wait(available)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        *available -= 1;
        acquired = true;
    }
    HEAVY_TEST_DEPTH.with(|depth| depth.set(depth.get() + 1));
    HeavyTestGate { acquired }
}

#[cfg(test)]
fn test_rss_wait_watermark() -> usize {
    const DEFAULT_WATERMARK_MB: usize = 3584;
    const EXTRA_WATERMARK_MB_PER_PERMIT: usize = 768;
    static WATERMARK: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *WATERMARK.get_or_init(|| {
        let mb = std::env::var("OG_LANGS_TEST_RSS_WAIT_MB")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|&mb| mb > 0)
            .unwrap_or(DEFAULT_WATERMARK_MB)
            + (heavy_test_permits().saturating_sub(1)) * EXTRA_WATERMARK_MB_PER_PERMIT;
        mb.saturating_mul(1024 * 1024)
    })
}

#[cfg(test)]
fn test_rss_budget() -> usize {
    const DEFAULT_BUDGET_MB: usize = 2048;
    static BUDGET: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *BUDGET.get_or_init(|| {
        let mb = std::env::var("OG_LANGS_TEST_RSS_BUDGET_MB")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|&mb| mb > 0)
            .unwrap_or(DEFAULT_BUDGET_MB);
        mb.saturating_mul(1024 * 1024)
    })
}

pub(crate) fn weak_shared<T>(
    build: impl FnOnce() -> Option<std::sync::Arc<T>>,
) -> Option<std::sync::Arc<T>>
where
    T: Send + Sync + 'static,
{
    use std::any::{Any, TypeId};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock, Weak};
    static CACHE: OnceLock<Mutex<HashMap<TypeId, Weak<dyn Any + Send + Sync>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = TypeId::of::<T>();
    if let Some(hit) = cache
        .lock()
        .ok()
        .and_then(|guard| guard.get(&key).cloned())
        .and_then(|weak| weak.upgrade())
        .and_then(|any| any.downcast::<T>().ok())
    {
        return Some(hit);
    }
    let built = build()?;
    if let Ok(mut guard) = cache.lock() {
        let erased: Arc<dyn Any + Send + Sync> = built.clone();
        guard.insert(key, Arc::downgrade(&erased));
    }
    Some(built)
}
pub mod ar;
pub mod ast;
pub mod be;
pub mod br;
pub mod ca;
#[path = "ca-filter-semantics-delta/mod.rs"]
pub mod ca_filter_semantics_delta;
#[path = "ca-module-refresh/mod.rs"]
pub mod ca_module_refresh;
#[path = "ca-phrase-repeat-rule/mod.rs"]
pub mod ca_phrase_repeat_rule;
#[path = "ca-post-overlap-pipeline/mod.rs"]
pub mod ca_post_overlap_pipeline;
pub mod confusion_probability;
pub mod crh;
pub mod da;
pub mod de;
#[path = "de-module-refresh/mod.rs"]
pub mod de_module_refresh;
pub mod detection;
pub mod el;
pub mod eo;
pub mod es;
#[path = "es-module-refresh/mod.rs"]
pub mod es_module_refresh;
pub mod fa;
pub mod fr;
#[path = "fr-module-refresh/mod.rs"]
pub mod fr_module_refresh;
pub mod ga;
pub mod gl;
pub mod is;
pub mod it;
pub mod ja;
pub mod km;
pub mod lt;
pub mod match_priority;
pub mod ml;
pub mod nl;
pub mod paragraph_repeat_beginning;
pub mod pl;
pub mod pt;
#[path = "pt-module-refresh/mod.rs"]
pub mod pt_module_refresh;
pub mod ro;
pub mod ru;
pub mod sk;
pub mod sl;
pub mod specific_case;
pub mod sr;
pub mod sv;
pub mod ta;
#[cfg(test)]
pub(crate) mod test_util;
pub mod tl;
pub mod uk;
pub mod upper_case_ngram;
pub mod zh;

#[cfg(test)]
mod weak_shared_tests {
    use super::weak_shared;
    use std::sync::Arc;

    struct Probe(u32);

    #[test]
    fn reuses_the_built_arc_while_it_is_alive() {
        let first = weak_shared(|| Some(Arc::new(Probe(7)))).expect("built");
        let second = weak_shared(|| Some(Arc::new(Probe(9)))).expect("built");
        assert!(std::ptr::eq(first.as_ref(), second.as_ref()));
        assert_eq!(second.0, 7);
        drop(second);
        let third = weak_shared(|| Some(Arc::new(Probe(11)))).expect("built");
        assert!(std::ptr::eq(first.as_ref(), third.as_ref()));
    }

    #[test]
    fn rebuilds_after_the_last_arc_drops_and_propagates_none() {
        let built = weak_shared(|| Some(Arc::new(Probe(1)))).expect("built");
        drop(built);
        let rebuilt = weak_shared(|| Some(Arc::new(Probe(2)))).expect("rebuilt");
        assert_eq!(rebuilt.0, 2);
        struct Never;
        assert!(weak_shared::<Never>(|| None).is_none());
    }
}
