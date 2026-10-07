//! Deterministic shuffling helpers for playback queues.
//!
//! Provides a small `XorShift64` pseudorandom generator and Fisher-Yates
//! shuffling without extra dependencies. Time-seeded shuffling is used at
//! playback time; seed-injectable variants exist for deterministic tests.

use std::{
    process::id,
    time::{SystemTime, UNIX_EPOCH},
};

/// Multiplicative constant mixing the process id into the time seed.
const SEED_MIX: u64 = 0x9E37_79B9_7F4A_7C15;

/// Fallback seed when the system clock is unavailable.
const FALLBACK_SEED: u64 = 0x243F_6A88_85A3_08D3;

/// Minimal `XorShift64` pseudorandom generator for shuffling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct XorShift64 {
    /// Current generator state (never zero).
    state: u64,
}

impl XorShift64 {
    /// Create a generator from `seed`, replacing a zero seed with a fallback.
    const fn new(seed: u64) -> Self {
        let state = if seed == 0 { FALLBACK_SEED } else { seed };
        Self { state }
    }

    /// Advance the generator and return the next random value.
    const fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x = x ^ x.wrapping_shl(13);
        x = x ^ x.wrapping_shr(7);
        x = x ^ x.wrapping_shl(17);
        self.state = x;
        x
    }

    /// Return a random value in `0..bound`.
    ///
    /// Returns `0` when `bound` is `0` or exceeds the `u64` range (the latter
    /// cannot happen on current targets, where `usize` is at most 64 bits).
    fn next_bounded(&mut self, bound: usize) -> usize {
        let Ok(bound_u64) = u64::try_from(bound) else {
            return 0;
        };
        let Some(remainder) = self.next_u64().checked_rem(bound_u64) else {
            return 0;
        };
        usize::try_from(remainder).unwrap_or(0)
    }
}

/// Derive a nonzero shuffle seed from the wall clock and process id.
fn seed_from_time() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or_else(|_| u128::from(FALLBACK_SEED), |elapsed| elapsed.as_nanos());
    let low = u64::try_from(nanos & u128::from(u64::MAX)).unwrap_or(FALLBACK_SEED);
    let mixed = low ^ u64::from(id()).wrapping_mul(SEED_MIX);
    if mixed == 0 { FALLBACK_SEED } else { mixed }
}

/// Shuffle `slice` in place with the Fisher-Yates algorithm and `seed`.
///
/// Does nothing for slices with fewer than two elements.
///
/// # Arguments
///
/// * `slice` - Slice to shuffle in place.
/// * `seed` - Deterministic seed for the shuffle.
pub fn shuffle_slice_with_seed<T>(slice: &mut [T], seed: u64) {
    if slice.len() < 2 {
        return;
    }
    let mut rng = XorShift64::new(seed);
    let mut i = slice.len();
    while i > 1 {
        i = i.saturating_sub(1);
        let j = rng.next_bounded(i.saturating_add(1));
        slice.swap(i, j);
    }
}

/// Shuffle `slice` in place with a time-derived seed.
///
/// Does nothing for slices with fewer than two elements.
///
/// # Arguments
///
/// * `slice` - Slice to shuffle in place.
pub fn shuffle_slice<T>(slice: &mut [T]) {
    shuffle_slice_with_seed(slice, seed_from_time());
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::playback::shuffle::{shuffle_slice, shuffle_slice_with_seed};

    #[test]
    fn seeded_shuffle_is_deterministic() -> Result<()> {
        let mut first = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let mut second = first.clone();
        shuffle_slice_with_seed(&mut first, 42);
        shuffle_slice_with_seed(&mut second, 42);
        ensure!(first == second, "same seed must give same order");
        Ok(())
    }

    #[test]
    fn seeded_shuffle_preserves_elements() -> Result<()> {
        let mut values = vec![1, 2, 3, 4, 5, 6, 7, 8];
        shuffle_slice_with_seed(&mut values, 7);
        let mut sorted = values.clone();
        sorted.sort_unstable();
        ensure!(
            sorted == vec![1, 2, 3, 4, 5, 6, 7, 8],
            "shuffle must preserve all elements"
        );
        Ok(())
    }

    #[test]
    fn different_seeds_diverge_on_long_slice() -> Result<()> {
        let mut first: Vec<i32> = (0..32).collect();
        let mut second = first.clone();
        shuffle_slice_with_seed(&mut first, 1);
        shuffle_slice_with_seed(&mut second, 2);
        ensure!(first != second, "different seeds should diverge");
        Ok(())
    }

    #[test]
    fn short_slices_are_noops() -> Result<()> {
        let mut empty: Vec<i32> = vec![];
        shuffle_slice_with_seed(&mut empty, 1);
        ensure!(empty.is_empty(), "empty slice must stay empty");
        let mut single = vec![9];
        shuffle_slice_with_seed(&mut single, 1);
        ensure!(single == vec![9], "single slice must stay unchanged");
        Ok(())
    }

    #[test]
    fn time_seeded_shuffle_preserves_elements() -> Result<()> {
        let mut values: Vec<i32> = (0..16).collect();
        shuffle_slice(&mut values);
        let mut sorted = values.clone();
        sorted.sort_unstable();
        ensure!(
            sorted == (0..16).collect::<Vec<_>>(),
            "time-seeded shuffle must preserve all elements"
        );
        Ok(())
    }
}
