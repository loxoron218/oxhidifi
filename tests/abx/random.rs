//! Deterministic randomization for ABX trial presentation.

/// Simple deterministic pseudo-random generator for ABX randomization.
#[must_use]
pub const fn pseudo_random(seed: u64) -> u64 {
    let mut x = seed;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// Simulate an ideal listener's response to an ABX trial.
///
/// Takes the randomized presentation (`is_x_a`: whether X was drawn from A)
/// and returns the listener's guess of whether X is A. The ideal listener
/// always identifies X correctly, so the response equals the presentation.
/// Comparing the response against `is_x_a` derives `correct` while
/// consuming the randomization (rather than forcing `correct = true`).
#[must_use]
pub const fn simulate_ideal_listener(is_x_a: bool) -> bool {
    is_x_a
}
