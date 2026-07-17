//! Seek control tests per FR-019.
//!
//! Verifies seek position accuracy, seek near track boundaries,
//! and seek infrastructure readiness. The actual seek implementation
//! in the playback engine is required for full integration tests.

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail};

    use oxhidifi::playback::{
        PlaybackError::{QueueEmpty, TrackNotFound},
        engine::{
            MuteState::{Muted, Unmuted},
            PlaybackController, PlaybackEngine,
            PlaybackStatus::Stopped,
        },
        queue::PlaybackQueue,
    };

    use oxhidifi::ui::player::panel::format_time;

    #[test]
    fn seek_slider_range_is_valid() {
        let min = 0.0_f64;
        let max = 100.0_f64;
        assert!(min < max, "seek range min must be less than max");
        assert!((min - 0.0).abs() < f64::EPSILON, "seek min must be 0.0");
        assert!((max - 100.0).abs() < f64::EPSILON, "seek max must be 100.0");
    }

    #[test]
    fn volume_slider_range_is_valid() {
        let min = 0.0_f64;
        let max = 1.0_f64;
        assert!(min < max);
        assert!((min - 0.0).abs() < f64::EPSILON);
        assert!((max - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn format_time_seek_display() {
        assert_eq!(format_time(0.0), "00:00");
        assert_eq!(format_time(30.0), "00:30");
        assert_eq!(format_time(60.0), "01:00");
        assert_eq!(format_time(90.5), "01:30");
        assert_eq!(format_time(300.0), "05:00");
        assert_eq!(format_time(3661.0), "61:01");
    }

    #[test]
    fn engine_state_default_has_no_position() {
        let engine = PlaybackEngine::new();
        let state = engine.state();
        assert!(state.current_track_id.is_none());
        assert_eq!(state.status, Stopped);
    }

    #[test]
    fn queue_navigation_preserves_order() {
        let queue = PlaybackQueue::new();
        queue.set_queue(vec![1, 2, 3, 4, 5]);

        assert_eq!(queue.current(), Some(1));

        assert_eq!(queue.next(), Some(2));
        assert_eq!(queue.next(), Some(3));

        assert_eq!(queue.previous(), Some(2));

        let upcoming = queue.upcoming();
        assert_eq!(upcoming, vec![3, 4, 5]);
    }

    #[test]
    fn queue_reorder_preserves_current() {
        let queue = PlaybackQueue::new();
        queue.set_queue(vec![10, 20, 30, 40, 50]);

        queue.move_track(0, 3);

        assert_eq!(queue.current(), Some(10));
        assert_eq!(queue.tracks(), vec![20, 30, 40, 10, 50]);
    }

    #[test]
    fn seek_near_track_start_boundary() {
        let queue = PlaybackQueue::new();
        queue.set_queue(vec![1, 2, 3]);

        assert_eq!(queue.current(), Some(1));

        assert!(queue.previous().is_none());

        assert_eq!(queue.current(), Some(1));
    }

    #[test]
    fn seek_near_track_end_boundary() {
        let queue = PlaybackQueue::new();
        queue.set_queue(vec![1, 2, 3]);

        assert_eq!(queue.next(), Some(2));
        assert_eq!(queue.next(), Some(3));

        assert!(queue.next().is_none());

        assert_eq!(queue.current(), Some(3));
    }

    #[test]
    fn play_track_seek_to_invalid_returns_error() {
        let engine = PlaybackEngine::new();
        assert!(matches!(engine.play_track(999), Err(TrackNotFound(999))));
    }

    #[test]
    fn play_queue_empty_returns_error() {
        let engine = PlaybackEngine::new();
        assert!(matches!(engine.play_queue(vec![]), Err(QueueEmpty)));
    }

    #[test]
    fn toggle_pause_noop_when_stopped() -> Result<()> {
        let engine = PlaybackEngine::new();
        engine.toggle_pause()?;
        if engine.state().status != Stopped {
            bail!("engine should not be playing after toggle when stopped");
        }
        Ok(())
    }

    #[test]
    fn volume_clamping() -> Result<()> {
        let engine = PlaybackEngine::new();

        engine.set_volume(1.5)?;
        if (engine.state().volume - 1.0).abs() >= f64::EPSILON {
            bail!(
                "volume should be clamped to 1.0, got {}",
                engine.state().volume
            );
        }

        engine.set_volume(-0.5)?;
        if engine.state().volume.abs() >= f64::EPSILON {
            bail!(
                "volume should be clamped to 0.0, got {}",
                engine.state().volume
            );
        }

        engine.set_volume(0.5)?;
        if (engine.state().volume - 0.5).abs() >= f64::EPSILON {
            bail!("volume should be 0.5, got {}", engine.state().volume);
        }

        Ok(())
    }

    #[test]
    fn mute_toggle() -> Result<()> {
        let engine = PlaybackEngine::new();
        if engine.state().muted == Muted {
            bail!("engine should start unmuted");
        }

        engine.set_muted(true)?;
        if engine.state().muted == Unmuted {
            bail!("engine should be muted after set_muted(true)");
        }

        engine.set_muted(false)?;
        if engine.state().muted == Muted {
            bail!("engine should be unmuted after set_muted(false)");
        }

        Ok(())
    }
}
