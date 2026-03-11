//! Internal utilities shared across the client crate.

/// Cheap pseudo-random float in `[0.0, 1.0)` without an external RNG dep.
///
/// Uses a thread-local counter XOR-mixed with the current timestamp to break
/// burst correlation: successive calls within the same nanosecond still
/// produce different values because the counter increments on every call.
/// The mixing step (xorshift64*) provides good avalanche without the weak
/// multiplicative-only hash that `subsec_nanos * constant` gives.
pub(crate) fn rand_f64() -> f64 {
    use std::cell::Cell;
    use std::time::{SystemTime, UNIX_EPOCH};

    thread_local! {
        static COUNTER: Cell<u64> = const { Cell::new(1) };
    }

    // Seed with current time so distinct threads / process restarts diverge.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);

    // Combine time with a per-thread monotonic counter so rapid successive
    // calls on the same thread always differ even at sub-nanosecond intervals.
    let state = COUNTER.with(|c| {
        let next = c.get().wrapping_add(1);
        c.set(next);
        next
    });

    // xorshift64* — fast, high-quality mixing with no division.
    let mut x = nanos ^ state.wrapping_mul(0x9e3779b97f4a7c15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;

    // Map to [0.0, 1.0) using the upper 53 bits (full mantissa of f64).
    (x >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rand_f64_in_range() {
        for _ in 0..1000 {
            let r = rand_f64();
            assert!((0.0..1.0).contains(&r), "rand_f64() returned {}", r);
        }
    }
}
