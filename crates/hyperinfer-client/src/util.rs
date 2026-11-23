//! Internal utilities shared across the client crate.

/// Random float in `[0.0, 1.0)` using the `rand` crate.
pub(crate) fn rand_f64() -> f64 {
    rand::random::<f64>()
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
