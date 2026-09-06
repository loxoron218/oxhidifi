//! Binomial statistical evaluation for ABX trial results.

/// Compute the binomial p-value for a given number of correct
/// identifications out of total trials.
///
/// Uses the binomial cumulative distribution function to compute the
/// probability of observing `k` or more correct identifications by
/// chance (p = 0.5 per trial), producing a one-sided p-value.
#[must_use]
pub fn binomial_p_value(k: u32, n: u32) -> f64 {
    if k == 0 || n == 0 {
        return 1.0;
    }

    let mut p = 0.0_f64;
    for i in k..=n {
        p += binomial_probability(i, n, 0.5);
    }
    p
}

/// Compute the binomial probability mass: C(n, k) * p^k * (1-p)^(n-k).
fn binomial_probability(k: u32, n: u32, p: f64) -> f64 {
    if k > n {
        return 0.0;
    }
    let combinations = binomial_coefficient(n, k);
    combinations * p.powi(k.cast_signed()) * (1.0 - p).powi(n.saturating_sub(k).cast_signed())
}

/// Compute binomial coefficient C(n, k) using an iterative method to
/// avoid overflow.
fn binomial_coefficient(n: u32, k: u32) -> f64 {
    let k = k.min(n.saturating_sub(k));
    if k == 0 {
        return 1.0;
    }
    let mut result = 1.0_f64;
    for i in 1..=k {
        result = result * f64::from(n.saturating_sub(k).saturating_add(i)) / f64::from(i);
    }
    result
}

#[cfg(test)]
mod tests {
    use crate::stats::{binomial_p_value, binomial_probability};

    #[test]
    fn binomial_probability_at_chance() {
        let prob = binomial_probability(5, 10, 0.5);
        assert!((prob - 0.246).abs() < 0.01, "expected ~0.246, got {prob}");
    }

    #[test]
    fn binomial_p_value_all_correct() {
        let p = binomial_p_value(10, 10);
        assert!(
            (p - 0.000_976_562_5).abs() < 0.0001,
            "expected ~0.00098, got {p}"
        );
    }

    #[test]
    fn binomial_p_value_none_correct() {
        let p = binomial_p_value(0, 10);
        assert!((p - 1.0).abs() < f64::EPSILON, "expected 1.0, got {p}");
    }

    #[test]
    fn binomial_p_value_at_chance() {
        let p = binomial_p_value(5, 10);
        assert!((p - 0.623).abs() < 0.05, "expected ~0.623, got {p}");
    }

    #[test]
    fn binomial_k_greater_than_n_returns_zero_prob() {
        let prob = binomial_probability(15, 10, 0.5);
        assert!((prob - 0.0).abs() < f64::EPSILON, "expected 0, got {prob}");
    }

    #[test]
    fn binomial_p_value_empty() {
        assert!((binomial_p_value(0, 0) - 1.0).abs() < f64::EPSILON);
    }
}
