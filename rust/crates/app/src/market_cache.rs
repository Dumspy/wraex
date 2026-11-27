//! Workers fetching the same league share one download and its freshness timestamp.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Entry<T> = Arc<Mutex<Option<(Instant, T)>>>;

#[derive(Default)]
pub(crate) struct MarketCache<T>(Mutex<HashMap<String, Entry<T>>>);

impl<T: Clone> MarketCache<T> {
    pub(crate) fn get(
        &self,
        league: &str,
        ttl_ms: u64,
        fetch: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let entry = self.0.lock().unwrap().entry(league.to_owned()).or_default().clone();
        let mut entry = entry.lock().unwrap();
        if let Some((loaded, value)) = &*entry
            && loaded.elapsed() < Duration::from_millis(ttl_ms)
        {
            return Ok(value.clone());
        }
        let value = fetch()?;
        *entry = Some((Instant::now(), value.clone()));
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn concurrent_items_share_one_download_per_league() {
        let cache = MarketCache::<String>::default();
        let calls = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for league in ["A", "B", "A", "B"] {
                let (cache, calls) = (&cache, &calls);
                scope.spawn(move || {
                    let result = cache
                        .get(league, 60_000, || {
                            calls.fetch_add(1, Ordering::Relaxed);
                            Ok(league.to_owned())
                        })
                        .unwrap();
                    assert_eq!(result, league);
                });
            }
        });
        assert_eq!(calls.into_inner(), 2);
        assert_eq!(cache.get("A", 0, || Ok("new".into())).unwrap(), "new");
    }
}
