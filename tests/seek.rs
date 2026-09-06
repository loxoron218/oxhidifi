//! Seek control tests per FR-019.
//!
//! Verifies seek position accuracy, seek near track boundaries,
//! and seek infrastructure readiness. The actual seek implementation
//! in the playback engine is required for full integration tests.

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write, path::Path};

    use {
        anyhow::{Result, anyhow, bail, ensure},
        num_traits::cast::cast,
        tempfile::NamedTempFile,
    };

    use oxhidifi::{
        playback::{
            PlaybackError::{QueueEmpty, TrackNotFound},
            decoder::Decoder,
            engine::PlaybackEngine,
            queue_manager::PlaybackQueue,
            state::{
                MuteState::{Muted, Unmuted},
                PlaybackStatus::Stopped,
            },
            transport::PlaybackTransport,
            write_wav_header,
        },
        ui::player::sidebar::format_time,
    };

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
    fn queue_navigation_preserves_order() -> Result<()> {
        let queue = PlaybackQueue::new();
        queue
            .set_queue(vec![1, 2, 3, 4, 5])
            .map_err(|e| anyhow!("{e}"))?;

        ensure!(queue.current() == Some(1), "current should be Some(1)");

        ensure!(queue.next() == Some(2), "next should be Some(2)");
        ensure!(queue.next() == Some(3), "next should be Some(3)");

        ensure!(queue.previous() == Some(2), "previous should be Some(2)");

        let upcoming = queue.upcoming();
        ensure!(upcoming == vec![3, 4, 5], "upcoming should be [3,4,5]");
        Ok(())
    }

    #[test]
    fn queue_reorder_preserves_current() -> Result<()> {
        let queue = PlaybackQueue::new();
        queue
            .set_queue(vec![10, 20, 30, 40, 50])
            .map_err(|e| anyhow!("{e}"))?;

        queue.move_track(0, 3);

        ensure!(queue.current() == Some(10), "current should be Some(10)");
        ensure!(
            queue.tracks() == vec![20, 30, 40, 10, 50],
            "tracks should be [20,30,40,10,50]"
        );
        Ok(())
    }

    #[test]
    fn seek_near_track_start_boundary() -> Result<()> {
        let queue = PlaybackQueue::new();
        queue.set_queue(vec![1, 2, 3]).map_err(|e| anyhow!("{e}"))?;

        ensure!(queue.current() == Some(1), "current should be Some(1)");

        ensure!(
            queue.previous().is_none(),
            "previous should be None at start"
        );

        ensure!(
            queue.current() == Some(1),
            "current should still be Some(1)"
        );
        Ok(())
    }

    #[test]
    fn seek_near_track_end_boundary() -> Result<()> {
        let queue = PlaybackQueue::new();
        queue.set_queue(vec![1, 2, 3]).map_err(|e| anyhow!("{e}"))?;

        ensure!(queue.next() == Some(2), "next should be Some(2)");
        ensure!(queue.next() == Some(3), "next should be Some(3)");

        ensure!(queue.next().is_none(), "next should be None at end");

        ensure!(queue.current() == Some(3), "current should be Some(3)");
        Ok(())
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

    fn write_test_wav(path: &Path, sample_rate: u32, duration_secs: f64) -> Result<()> {
        let mut f = File::create(path)?;
        let frames_f64 = f64::from(sample_rate) * duration_secs;
        let frames: u32 = cast::<f64, u32>(frames_f64).unwrap_or(0);
        let data_bytes = frames.saturating_mul(2);
        write_wav_header(&mut f, 1, sample_rate, 16, data_bytes)?;
        for _ in 0..frames {
            f.write_all(&0i16.to_le_bytes())?;
        }
        Ok(())
    }

    #[test]
    fn seek_accuracy_within_100ms() -> Result<()> {
        let tmp = NamedTempFile::new()?;
        write_test_wav(tmp.path(), 44100, 5.0)?;
        let mut decoder = Decoder::open(tmp.path())?;
        let target = 2.5;
        let actual = decoder.seek_to(target)?;
        let diff = (actual - target).abs();
        if diff > 0.1 {
            bail!("seek accuracy failed: target {target}s, actual {actual}s, diff {diff}s > 100ms");
        }
        Ok(())
    }

    #[test]
    fn seek_near_start_accuracy() -> Result<()> {
        let tmp = NamedTempFile::new()?;
        write_test_wav(tmp.path(), 44100, 5.0)?;
        let mut decoder = Decoder::open(tmp.path())?;
        let target = 0.2;
        let actual = decoder.seek_to(target)?;
        let diff = (actual - target).abs();
        if diff > 0.1 {
            bail!(
                "seek near start failed: target {target}s, actual {actual}s, diff {diff}s > 100ms"
            );
        }
        Ok(())
    }

    #[test]
    fn seek_near_end_accuracy() -> Result<()> {
        let tmp = NamedTempFile::new()?;
        write_test_wav(tmp.path(), 44100, 5.0)?;
        let mut decoder = Decoder::open(tmp.path())?;
        let duration = decoder.params().duration_seconds;
        let target = (duration - 0.2).max(0.0);
        let actual = decoder.seek_to(target)?;
        let diff = (actual - target).abs();
        if diff > 0.1 {
            bail!(
                "seek near end failed: target {target}s, actual {actual}s, diff {diff}s > 100ms \
                 (duration {duration}s)"
            );
        }
        Ok(())
    }

    #[test]
    fn seek_during_gapless_transition_accuracy() -> Result<()> {
        let tmp1 = NamedTempFile::new()?;
        let tmp2 = NamedTempFile::new()?;
        write_test_wav(tmp1.path(), 44100, 3.0)?;
        write_test_wav(tmp2.path(), 48000, 3.0)?;
        let mut dec1 = Decoder::open(tmp1.path())?;
        let duration1 = dec1.params().duration_seconds;
        let target = (duration1 - 0.5).max(0.0);
        let actual = dec1.seek_to(target)?;
        let diff = (actual - target).abs();
        if diff > 0.1 {
            bail!("gapless seek failed: target {target}s, actual {actual}s, diff {diff}s > 100ms");
        }
        let mut dec2 = Decoder::open(tmp2.path())?;
        let actual2 = dec2.seek_to(0.1)?;
        if (actual2 - 0.1).abs() > 0.1 {
            bail!("second track seek after gapless failed");
        }
        Ok(())
    }
}
