//! The trade site's rate limits: `web/price-check/trade/RateLimiter.ts` and
//! `adjustRateLimits`/`preventQueueCreation` from `common.ts`, identical in
//! both references.
//!
//! Upstream waits on timers; here the state is plain data and every call takes
//! the current time (`now`, in milliseconds), so callers decide how to wait and
//! the behaviour is testable without a clock. [`RateLimits::delay`] is how long
//! to wait before a request, [`RateLimits::acquire`] records one, and
//! [`RateLimits::adjust`] syncs with the server's `X-Rate-Limit-*` headers.

/// Milliseconds on some monotonic clock.
pub type Millis = u64;

/// One limit: at most `max` requests per `window` seconds (`RateLimiter`).
#[derive(Debug, Clone, PartialEq)]
pub struct RateLimiter {
    pub max: u32,
    /// Seconds, the server's window plus the configured latency allowance.
    pub window: f64,
    /// When each request still counted against the limit is released, oldest first.
    stack: Vec<Millis>,
}

impl RateLimiter {
    pub fn new(max: u32, window: f64) -> Self {
        Self { max, window, stack: Vec::new() }
    }

    fn window_millis(&self) -> Millis {
        (self.window * 1000.0) as Millis
    }

    /// Drops the requests whose window has passed (`ResourceHandle`'s timer).
    fn release(&mut self, now: Millis) {
        self.stack.retain(|&at| at > now);
    }

    /// Counts a request made at `now` (`push`).
    fn push(&mut self, now: Millis) {
        let at = now + self.window_millis();
        let i = self.stack.partition_point(|&x| x <= at);
        self.stack.insert(i, at);
    }

    /// `available`: requests that can be made right away.
    pub fn available(&mut self, now: Millis) -> u32 {
        self.release(now);
        self.max.saturating_sub(self.stack.len() as u32)
    }

    /// `isFullyUtilized`
    pub fn is_fully_utilized(&mut self, now: Millis) -> bool {
        self.available(now) == 0
    }

    /// Requests counted against the limit at `now`.
    pub fn used(&mut self, now: Millis) -> u32 {
        self.release(now);
        self.stack.len() as u32
    }

    /// `isEqualLimit`
    pub fn is_equal_limit(&self, max: u32, window: f64) -> bool {
        self.max == max && self.window == window
    }

    /// How long until a request fits: until the oldest one is released.
    fn delay(&mut self, now: Millis) -> Millis {
        if self.is_fully_utilized(now) { self.stack[0].saturating_sub(now) } else { 0 }
    }
}

/// The limits for one kind of request (`RATE_LIMIT_RULES.SEARCH`, ...).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RateLimits {
    pub limiters: Vec<RateLimiter>,
}

/// A limit from the server's headers.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ServerLimit {
    max: u32,
    window: f64,
    state: u32,
}

/// `preventQueueCreation` refuses to queue requests this long or longer.
pub const MAX_QUEUE_MILLIS: Millis = 1500;

/// A request that would have to wait too long; upstream throws
/// `Retry after N seconds`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryAfter {
    pub millis: Millis,
}

impl std::fmt::Display for RetryAfter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `Math.round(ms / 1000)`
        write!(f, "Retry after {} seconds", (self.millis + 500) / 1000)
    }
}

impl std::error::Error for RetryAfter {}

impl RateLimits {
    /// One limiter, as the app starts (`new RateLimiter(1, 5)` per kind).
    pub fn initial() -> Self {
        Self { limiters: vec![RateLimiter::new(1, 5.0)] }
    }

    /// `RateLimiter.waitMulti`: how long to wait until every limiter has room.
    pub fn delay(&mut self, now: Millis) -> Millis {
        self.limiters.iter_mut().map(|l| l.delay(now)).max().unwrap_or(0)
    }

    /// Counts a request against every limiter. Call when [`Self::delay`] is 0.
    pub fn acquire(&mut self, now: Millis) {
        for limiter in &mut self.limiters {
            limiter.release(now);
            limiter.push(now);
        }
    }

    /// `adjustRateLimits`: syncs with a response's headers. `header` looks a
    /// header up by lower-case name (`Headers.get` ignores case, and the rule
    /// names are `Ip`, `Account`, ...); `latency_seconds` is the price-check
    /// widget's `apiLatencySeconds`, added to every window (`DESYNC_FIX`).
    pub fn adjust<'a>(&mut self, header: impl Fn(&str) -> Option<&'a str>, latency_seconds: f64, now: Millis) {
        let Some(rules) = header("x-rate-limit-rules") else { return };
        let rules: Vec<&str> = rules.split(',').collect();
        let name = |r: &str, suffix: &str| format!("x-rate-limit-{r}{suffix}").to_ascii_lowercase();
        let limit_str: Vec<&str> = rules.iter().map(|r| header(&name(r, "")).unwrap_or("")).collect();
        let state_str: Vec<&str> = rules.iter().map(|r| header(&name(r, "-state")).unwrap_or("")).collect();
        self.adjust_from(&limit_str.join(","), &state_str.join(","), latency_seconds, now);
    }

    /// `_adjustRateLimits`: `limit_str` and `state_str` are `max:window:...`
    /// triples, comma-separated, one per server limit.
    fn adjust_from(&mut self, limit_str: &str, state_str: &str, latency_seconds: f64, now: Millis) {
        let number = |s: Option<&str>| s.map_or(f64::NAN, crate::js::number);
        let states: Vec<f64> = state_str.split(',').map(|r| number(r.split(':').next())).collect();
        let server: Vec<ServerLimit> = limit_str
            .split(',')
            .enumerate()
            .map(|(i, rule)| {
                let mut parts = rule.split(':');
                let max = number(parts.next());
                let window = number(parts.next()) + latency_seconds;
                let state = states.get(i).copied().unwrap_or(f64::NAN);
                // Counts beyond u32 or NaN don't occur in GGG's headers.
                ServerLimit { max: max.max(0.0) as u32, window, state: state.max(0.0) as u32 }
            })
            .collect();

        // Destroy the limits the server no longer has.
        self.limiters.retain(|l| server.iter().any(|s| l.is_equal_limit(s.max, s.window)));

        // Burst to the server's state where it counts more requests than we do.
        for limiter in &mut self.limiters {
            let s = server.iter().find(|s| limiter.is_equal_limit(s.max, s.window)).expect("kept above");
            let used = limiter.used(now);
            if s.state > used {
                let burst = (s.state - used).min(limiter.available(now));
                for _ in 0..burst {
                    limiter.push(now);
                }
            }
        }

        // Add the new ones, as full as the server says.
        for s in &server {
            if self.limiters.iter().any(|l| l.is_equal_limit(s.max, s.window)) {
                continue;
            }
            let mut limiter = RateLimiter::new(s.max, s.window);
            // `rl.wait()` per used request. Past `max` (a server state above its
            // own limit), upstream's extra waits queue and borrow once room
            // frees up; here they're dropped.
            for _ in 0..s.state.min(s.max) {
                limiter.push(now);
            }
            self.limiters.push(limiter);
        }
    }

    /// `RateLimiter.estimateTime`: how long `count` requests take from `now`,
    /// or from an empty state with `ignore_state`.
    pub fn estimate_time(&mut self, count: u32, ignore_state: bool, now: Millis) -> Millis {
        struct Sim {
            max: usize,
            window: f64,
            stack: Vec<f64>,
        }
        let mut sim: Vec<Sim> = self
            .limiters
            .iter_mut()
            .map(|l| {
                l.release(now);
                Sim {
                    max: l.max as usize,
                    window: l.window,
                    stack: if ignore_state {
                        Vec::new()
                    } else {
                        l.stack.iter().map(|&at| (at - now) as f64).collect()
                    },
                }
            })
            .collect();

        let mut total = 0.0;
        for _ in 0..count {
            while sim.iter().any(|l| l.stack.len() >= l.max) {
                let wait = sim
                    .iter()
                    .filter(|l| l.stack.len() >= l.max)
                    .map(|l| l.stack.first().map_or(0.0, |&first| first - total))
                    .fold(0.0, f64::max);
                total += wait;
                for l in &mut sim {
                    l.stack.retain(|&t| t > total);
                }
                // A limit of 0 never frees up; upstream spins forever there.
                if wait == 0.0 && sim.iter().any(|l| l.max == 0) {
                    return Millis::MAX;
                }
            }
            for l in &mut sim {
                l.stack.push(total + l.window * 1000.0);
            }
        }
        total as Millis
    }
}

/// `preventQueueCreation`: refuses requests that would queue for 1.5 s or more.
/// A target that can never run without queueing is let through.
pub fn prevent_queue_creation(targets: &mut [(u32, &mut RateLimits)], now: Millis) -> Result<(), RetryAfter> {
    let mut estimated_millis = 0;
    for (count, limits) in targets.iter_mut() {
        let estimated = limits.estimate_time(*count, false, now);
        let clean = limits.estimate_time(*count, true, now);
        if estimated != clean {
            estimated_millis = estimated_millis.max(estimated);
        }
    }
    if estimated_millis >= MAX_QUEUE_MILLIS { Err(RetryAfter { millis: estimated_millis }) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<&'a str> {
        move |name| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
    }

    #[test]
    fn a_full_limit_waits_for_its_oldest_request() {
        let mut limits = RateLimits::initial();
        assert_eq!(limits.delay(0), 0);
        limits.acquire(0);
        assert_eq!(limits.delay(1000), 4000);
        assert_eq!(limits.delay(5000), 0);
        limits.acquire(5000);
        assert_eq!(limits.delay(5000), 5000);
    }

    #[test]
    fn headers_replace_add_and_burst_limits() {
        let mut limits = RateLimits::initial();
        let h = [
            ("x-rate-limit-rules", "Ip,Account"),
            ("x-rate-limit-ip", "8:10:60,15:60:120"),
            ("x-rate-limit-ip-state", "3:10:0,5:60:0"),
            ("x-rate-limit-account", "3:5:60"),
            ("x-rate-limit-account-state", "1:5:0"),
        ];
        limits.adjust(headers(&h), 2.0, 0);
        let got: Vec<(u32, f64, u32)> = limits.limiters.iter_mut().map(|l| (l.max, l.window, l.used(0))).collect();
        assert_eq!(got, [(8, 12.0, 3), (15, 62.0, 5), (3, 7.0, 1)]);

        // The server counts two more on the account limit: burst to match.
        let h = [
            ("x-rate-limit-rules", "Account"),
            ("x-rate-limit-account", "3:5:60"),
            ("x-rate-limit-account-state", "3:5:0"),
        ];
        limits.adjust(headers(&h), 2.0, 100);
        assert_eq!(limits.limiters.len(), 1);
        assert_eq!(limits.limiters[0].used(100), 3);
        assert_eq!(limits.delay(100), 7000 - 100);
    }

    #[test]
    fn without_rules_nothing_changes() {
        let mut limits = RateLimits::initial();
        limits.adjust(headers(&[]), 2.0, 0);
        assert_eq!(limits, RateLimits::initial());
    }

    #[test]
    fn estimates_follow_the_simulation() {
        let mut limits = RateLimits { limiters: vec![RateLimiter::new(2, 10.0)] };
        assert_eq!(limits.estimate_time(2, false, 0), 0);
        assert_eq!(limits.estimate_time(3, false, 0), 10_000);
        limits.acquire(0);
        limits.acquire(1000);
        assert_eq!(limits.estimate_time(1, false, 2000), 8000);
        assert_eq!(limits.estimate_time(1, true, 2000), 0);
    }

    #[test]
    fn long_queues_are_refused() {
        let mut search = RateLimits::initial();
        let mut fetch = RateLimits::initial();
        assert!(prevent_queue_creation(&mut [(1, &mut search), (1, &mut fetch)], 0).is_ok());
        search.acquire(0);
        let err = prevent_queue_creation(&mut [(1, &mut search), (1, &mut fetch)], 1000).unwrap_err();
        assert_eq!(err.to_string(), "Retry after 4 seconds");
        // Under 1.5 s is fine.
        assert!(prevent_queue_creation(&mut [(1, &mut search)], 4000).is_ok());
        // Two requests on a one-per-window limit can never avoid a queue.
        let mut clean = RateLimits::initial();
        assert!(prevent_queue_creation(&mut [(2, &mut clean)], 0).is_ok());
    }
}
