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
    pub const fn new(max: u32, window: f64) -> Self {
        Self { max, window, stack: Vec::new() }
    }

    fn window_millis(&self) -> Millis {
        crate::num::f64_to_u64(self.window * 1000.0)
    }

    /// Drops the requests whose window has passed (`ResourceHandle`'s timer).
    fn release(&mut self, now: Millis) {
        self.stack.retain(|&at| at > now);
    }

    /// Counts a request made at `now` (`push`).
    fn push(&mut self, now: Millis) {
        let at = now.saturating_add(self.window_millis());
        let i = self.stack.partition_point(|&x| x <= at);
        self.stack.insert(i, at);
    }

    /// `available`: requests that can be made right away.
    pub fn available(&mut self, now: Millis) -> u32 {
        self.release(now);
        self.max.saturating_sub(u32::try_from(self.stack.len()).unwrap_or(u32::MAX))
    }

    /// `isFullyUtilized`
    pub fn is_fully_utilized(&mut self, now: Millis) -> bool {
        self.available(now) == 0
    }

    /// Requests counted against the limit at `now`.
    pub fn used(&mut self, now: Millis) -> u32 {
        self.release(now);
        u32::try_from(self.stack.len()).unwrap_or(u32::MAX)
    }

    /// `isEqualLimit`
    #[expect(
        clippy::float_cmp,
        reason = "server windows must match a limiter exactly: both sides are `f64::from(seconds) + latency_seconds`"
    )]
    pub fn is_equal_limit(&self, max: u32, window: f64) -> bool {
        self.max == max && self.window == window
    }

    /// How long until a request fits: until the oldest one is released.
    fn delay(&mut self, now: Millis) -> Millis {
        if self.is_fully_utilized(now) { self.stack.first().map_or(0, |&at| at.saturating_sub(now)) } else { 0 }
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
        write!(f, "Retry after {} seconds", self.millis.saturating_add(500) / 1000)
    }
}

impl std::error::Error for RetryAfter {}

fn triple(value: &str) -> Option<(u32, u32, u32)> {
    let mut fields = value.trim().split(':');
    let triple = (fields.next()?.parse().ok()?, fields.next()?.parse().ok()?, fields.next()?.parse().ok()?);
    fields.next().is_none().then_some(triple)
}

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
        let mut server = Vec::new();
        for rule in rules.split(',').map(str::trim) {
            if rule.is_empty() {
                return;
            }
            let name = format!("x-rate-limit-{rule}").to_ascii_lowercase();
            let (Some(limits), Some(states)) = (header(&name), header(&format!("{name}-state"))) else { return };
            let mut states = states.split(',');
            for limit in limits.split(',') {
                let Some((max, window, _)) = triple(limit) else { return };
                let Some((state, state_window, _)) = states.next().and_then(triple) else { return };
                if max == 0 || window == 0 || state_window != window {
                    return;
                }
                server.push(ServerLimit { max, window: f64::from(window) + latency_seconds, state });
            }
            if states.next().is_some() {
                return;
            }
        }

        // Destroy the limits the server no longer has.
        self.limiters.retain(|l| server.iter().any(|s| l.is_equal_limit(s.max, s.window)));

        // Burst to the server's state where it counts more requests than we do.
        for limiter in &mut self.limiters {
            let Some(s) = server.iter().find(|s| limiter.is_equal_limit(s.max, s.window)) else { continue };
            let used = limiter.used(now);
            if s.state > used {
                let burst = s.state.saturating_sub(used).min(limiter.available(now));
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
                    max: usize::try_from(l.max).unwrap_or(usize::MAX),
                    window: l.window,
                    stack: if ignore_state {
                        Vec::new()
                    } else {
                        l.stack.iter().map(|&at| crate::num::u64_to_f64(at.saturating_sub(now))).collect()
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
                #[expect(
                    clippy::suboptimal_flops,
                    reason = "upstream is `total + window * 1000`; `mul_add` would round differently"
                )]
                let at = total + l.window * 1000.0;
                l.stack.push(at);
            }
        }
        crate::num::f64_to_u64(total)
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
            ("x-rate-limit-rules", "Ip, Account"),
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
    fn incomplete_headers_keep_the_learned_policy_and_pending_requests() {
        let mut limits = RateLimits::initial();
        let mut h = [("x-rate-limit-rules", "Ip"), ("x-rate-limit-ip", "1:10:60"), ("x-rate-limit-ip-state", "1:10:0")];
        limits.adjust(headers(&h), 0.0, 0);
        let learned = limits.clone();
        for (rules, definition, state) in [
            ("Ip, Account", "2:3:4", "1:3:0"),
            ("Ip", "0:10:60", "0:10:0"),
            ("Ip", "1:10:60,3:60:90", "1:10:0"),
            ("Ip", "1:10:60", "1:20:0"),
        ] {
            h[0].1 = rules;
            h[1].1 = definition;
            h[2].1 = state;
            limits.adjust(headers(&h), 0.0, 100);
            assert_eq!(limits, learned);
            assert_eq!(limits.delay(100), 9900);
        }
        assert_eq!(limits.delay(10_000), 0);
        limits.acquire(10_000);
        assert_eq!(limits.delay(10_001), 9999);
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
