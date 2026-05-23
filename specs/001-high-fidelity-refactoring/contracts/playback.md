# Playback Interface Contract

## Purpose

The playback engine exposes a command interface for the UI layer to control
playback, and emits events for state changes.

## Traits

### `PlaybackController` — consumed by UI

```rust
/// Commands the UI can issue to the playback engine.
pub trait PlaybackController: Send + 'static {
    fn play_track(&self, track_id: i64) -> Result<()>;
    fn play_queue(&self, queue: Vec<i64>) -> Result<()>;
    fn toggle_pause(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn next_track(&self) -> Result<()>;
    fn previous_track(&self) -> Result<()>;
    fn seek(&self, position: Duration) -> Result<()>;
    fn set_volume(&self, volume: f64) -> Result<()>;
    fn set_muted(&self, muted: bool) -> Result<()>;
}
```

### `PlaybackState` — observed by UI

```rust
/// Current state of the playback engine.
pub struct PlaybackState {
    pub current_track: Option<Track>,
    pub position: Duration,
    pub duration: Duration,
    pub volume: f64,
    pub is_muted: bool,
    pub is_playing: bool,
    pub queue: Vec<Track>,
}
```

### `PlaybackEvent` — streamed to UI

```rust
/// Events emitted by the playback engine.
pub enum PlaybackEvent {
    TrackStarted { track: Track, position: Duration },
    TrackProgress { position: Duration },
    TrackFinished { track_id: i64, next_track_id: Option<i64> },
    Paused,
    Resumed,
    Stopped,
    VolumeChanged { volume: f64 },
    Muted,
    Unmuted,
    Seeked { position: Duration },
    QueueChanged { queue: Vec<Track> },
    DeviceLost { error: String },
    Error { error: PlaybackError },
}
```

## Audio Pipeline Topology

```
DecoderTask (symphonia)
  │  decoded PCM (mono/stereo float, native sample rate)
  ▼
ResamplerTask (rubato)
  │  resampled PCM (device sample rate)
  ▼
OutputTask (cpal callback)
  │  device ring buffer (rtrb)
```

- Each stage runs on its own thread/task
- Communication between stages via `rtrb::RingBuffer` (lock-free SPSC)
- The decoder pre-buffers the next track during the last ~1 second of current track playback for gapless transitions
- Sample rate changes between tracks trigger resampler reconfiguration; the resampler is reset and new coefficients are computed

## Error Types

```rust
/// Errors originating from the playback engine.
#[derive(Error, Debug)]
pub enum PlaybackError {
    #[error("Track not found: {0}")]
    TrackNotFound(i64),
    #[error("Decoder error: {0}")]
    DecoderError(#[from] DecoderError),
    #[error("Output device error: {0}")]
    OutputError(#[from] OutputError),
    #[error("Device disconnected")]
    DeviceDisconnected,
    #[error("No device available")]
    NoDeviceAvailable,
    #[error("Queue empty")]
    QueueEmpty,
}
```
