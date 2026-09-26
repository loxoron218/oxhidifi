//! Whole-path verdict classifier: Limited over Processed over Bit-Perfect.
//!
//! [`resolve_verdict`] is a total pure function over per-stage verdicts. The
//! empty stage list resolves to [`QualityVerdict::BitPerfect`] because an
//! empty chain carries no alteration. Snapshots without a track never reach
//! this function; the UI shows the empty state instead.

use crate::playback::signal_path::QualityVerdict::{self, BitPerfect, Limited, Processed};

/// Classify the whole path from per-stage verdicts.
///
/// Precedence is Limited over Processed over Bit-Perfect: any limited stage
/// forces Limited, otherwise any processed stage forces Processed.
///
/// # Arguments
///
/// * `stages` - Per-stage verdicts in chain order.
///
/// # Returns
///
/// * `QualityVerdict` - Resolved whole-path verdict.
#[must_use]
pub fn resolve_verdict(stages: &[QualityVerdict]) -> QualityVerdict {
    if stages.contains(&Limited) {
        Limited
    } else if stages.contains(&Processed) {
        Processed
    } else {
        BitPerfect
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, ensure};

    use crate::playback::signal_path::{
        QualityVerdict::{BitPerfect, Limited, Processed},
        path_verdict::resolve_verdict,
    };

    #[test]
    fn empty_stage_list_resolves_to_bit_perfect() -> Result<()> {
        ensure!(
            resolve_verdict(&[]) == BitPerfect,
            "empty stage list must resolve to Bit-Perfect"
        );
        Ok(())
    }

    #[test]
    fn all_bit_perfect_stages_resolve_to_bit_perfect() -> Result<()> {
        ensure!(
            resolve_verdict(&[BitPerfect, BitPerfect]) == BitPerfect,
            "all bit-perfect stages must resolve to Bit-Perfect"
        );
        Ok(())
    }

    #[test]
    fn any_processed_stage_forces_processed() -> Result<()> {
        ensure!(
            resolve_verdict(&[BitPerfect, Processed]) == Processed,
            "any Processed stage must force Processed"
        );
        Ok(())
    }

    #[test]
    fn limited_wins_over_processed() -> Result<()> {
        ensure!(
            resolve_verdict(&[Processed, Limited]) == Limited,
            "Limited must win over Processed"
        );
        Ok(())
    }

    #[test]
    fn limited_wins_over_bit_perfect() -> Result<()> {
        ensure!(
            resolve_verdict(&[BitPerfect, Limited]) == Limited,
            "Limited must win over Bit-Perfect"
        );
        Ok(())
    }
}
