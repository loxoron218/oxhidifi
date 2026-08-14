//! Display-order index memo for library grids.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::app::runtime::SortMemo;

/// Look up or compute the memoized display-order indices for a library grid.
///
/// Returns the memoized indices when the grid `generation` and the current
/// sort `config` both match, so repeated tab/mode switches reuse the sort
/// instead of re-comparing every item. Otherwise computes them with `compute`
/// (passed the current `config`), stores the result in `memo`, and returns it.
pub fn memoized_sort_indices<C>(
    generation: u64,
    config: C,
    memo: &Mutex<Option<SortMemo<C>>>,
    compute: impl FnOnce(&C) -> Vec<usize>,
) -> Arc<[usize]>
where
    C: PartialEq,
{
    let mut memo = memo.lock();
    if let Some(m) = memo.as_ref()
        && m.generation == generation
        && m.config == config
    {
        return Arc::clone(&m.indices);
    }
    let indices: Arc<[usize]> = Arc::from(compute(&config));
    *memo = Some(SortMemo {
        generation,
        config,
        indices: Arc::clone(&indices),
    });
    indices
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;

    use crate::{app::runtime::SortMemo, ui::gallery::order_memo::memoized_sort_indices};

    fn memo_harness() -> Mutex<Option<SortMemo<u8>>> {
        Mutex::new(None)
    }

    fn sort_compute() -> impl FnOnce(&u8) -> Vec<usize> {
        |config: &u8| vec![usize::from(*config), 9]
    }

    #[test]
    fn memoized_sort_indices_reuses_cache_without_recompute() {
        let memo = memo_harness();

        let first = memoized_sort_indices(1, 5u8, &memo, sort_compute());
        let second = memoized_sort_indices(1, 5u8, &memo, sort_compute());
        assert!(
            Arc::ptr_eq(&first, &second),
            "same generation and config must return the memoized indices"
        );
        assert_eq!(&*first, &[5, 9]);
    }

    #[test]
    fn memoized_sort_indices_recomputes_on_generation_change() {
        let memo = memo_harness();

        let first = memoized_sort_indices(1, 5u8, &memo, sort_compute());
        let second = memoized_sort_indices(2, 5u8, &memo, sort_compute());
        assert!(
            !Arc::ptr_eq(&first, &second),
            "a generation bump must invalidate the memoized indices"
        );
    }

    #[test]
    fn memoized_sort_indices_recomputes_on_config_change() {
        let memo = memo_harness();

        let first = memoized_sort_indices(1, 5u8, &memo, sort_compute());
        let second = memoized_sort_indices(1, 7u8, &memo, sort_compute());
        assert!(
            !Arc::ptr_eq(&first, &second),
            "a different config must invalidate the memoized indices"
        );
        assert_eq!(&*second, &[7, 9]);
    }
}
