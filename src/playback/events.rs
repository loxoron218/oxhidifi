use super::queue::QueueItem;

/// use crate::playback::events::{PlaybackEvent, PlaybackState};
///
/// // State change event
/// let state_event = PlaybackEvent::StateChanged(PlaybackState::Playing);
///
/// // Position change event (10 seconds in nanoseconds)
/// let position_event = PlaybackEvent::PositionChanged(10_000_000);
/// ```
#[derive(Debug, Clone)]
pub enum PlaybackEvent {
    /// A new song has been loaded for playback
    SongChanged(Box<QueueItem>),
    /// Playback state changed
    StateChanged(PlaybackState),
    /// Position changed (in nanoseconds)
    PositionChanged(u64),
    /// End of stream reached
    EndOfStream,
    /// An error occurred
    Error(String),
}

/// Represents the various states that audio playback can be in.
///
/// These states help song and control the playback lifecycle. The state transitions
/// follow a logical flow that matches typical media player behavior. The states are
/// used by both the playback engine and UI components to maintain synchronization.
///
/// # State Transitions
///
/// ```text
/// Stopped → Playing (when play is initiated)
/// Playing → Paused (when pause is initiated)
/// Paused → Playing (when play is initiated)
/// Playing → Stopped (when stop is initiated)
/// Any → Stopped (when playback ends or is interrupted)
/// ```
///
/// # Examples
///
/// ```
/// use crate::playback::events::PlaybackState;
///
/// let current_state = PlaybackState::Playing;
/// match current_state {
///     PlaybackState::Playing => println!("Playback is active"),
///     PlaybackState::Paused => println!("Playback is paused"),
///     _ => println!("Other state"),
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackState {
    /// Playback is stopped
    Stopped,
    /// Playback is active
    Playing,
    /// Playback is paused
    Paused,
}
