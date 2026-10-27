pub mod en;
pub mod engine;
pub mod registry;

pub use en::*;
pub use engine::*;
pub use registry::*;

#[cfg(all(test, target_os = "linux"))]
#[unsafe(link_section = ".init_array")]
#[used]
static BOUND_TEST_ALLOCATOR: extern "C" fn() = {
    // glibc's dynamic mmap threshold funnels freed multi-MB dictionaries back into
    // per-thread arenas; malloc_trim cannot return those pages and long runs OOM.
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
            #[cfg(target_os = "linux")]
            if freed > 0 && og_core::releasable::trim_due(TRIM_INTERVAL) {
                unsafe {
                    libc::malloc_trim(0);
                }
            }
        }
    }
    ReleaseOnDrop {
        _heavy: og_core::releasable::heavy_guard(),
        acquired: std::time::Instant::now(),
    }
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
pub mod confusion_probability;
pub mod crh;
pub mod da;
pub mod de;
pub mod detection;
pub mod el;
pub mod eo;
pub mod es;
pub mod fa;
pub mod fr;
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
