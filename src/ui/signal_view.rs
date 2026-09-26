//! Live signal path tab: chain view and atomic refresh.
//!
//! The `Signal` library tab renders immutable snapshots built off-thread by
//! the playback signal path. Rows swap whole-instance on `generation`
//! change so gapless transitions never show mixed chains.

pub mod signal_poll;
pub mod signal_publish;
pub mod signal_tab;
