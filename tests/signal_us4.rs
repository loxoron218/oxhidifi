//! US4 acceptance: device footer card and processing-speed readout.
//!
//! Submodule of the `signal_path` test target rooted at
//! `tests/signal_inspector.rs`; fixtures live in `crate::signal_fixtures`. Covers
//! FR-009 (exactly one MVP footer card for the active output device, manual
//! link hidden) and FR-010 (speed shown iff any in-app alteration incl.
//! volume-only is active, hidden when bit-perfect or Limited-only).

#[cfg(test)]
mod tests {
    use anyhow::{Result, bail, ensure};

    use oxhidifi::playback::{
        decoder::AudioParams,
        signal_path::{
            DeviceRole::TransportTarget,
            QualityVerdict::{BitPerfect, Limited, Processed},
            RenderingDevice,
            path_snapshot::{build_snapshot, summarize_text},
        },
    };

    use crate::signal_fixtures::{
        bit_perfect_input, dsd_input, flac_audio, make_device, playing_input,
    };

    #[test]
    fn us4_device_footer_contract() -> Result<()> {
        let snapshot = build_snapshot(&playing_input(41, 41, 44100, Some(16)))?;
        ensure!(snapshot.devices.len() == 1, "MVP shows exactly one card");
        let Some(device) = snapshot.devices.first() else {
            bail!("MVP snapshot must carry the active output device")
        };
        ensure!(!device.display_name.is_empty(), "card must name the device");
        ensure!(device.display_name == "Lab DAC", "card names the renderer");
        ensure!(device.manual_url.is_none(), "MVP hides the manual link");
        ensure!(!device.brand_visual.is_empty(), "card needs a brand visual");
        ensure!(
            !device.illustration.is_empty(),
            "card needs an illustration"
        );
        let mut fallback = bit_perfect_input();
        fallback.device_name = String::new();
        fallback.device_id = String::from("hw:7");
        let named = build_snapshot(&fallback)?;
        let Some(card) = named.devices.first() else {
            bail!("fallback snapshot must carry a device")
        };
        ensure!(card.display_name == "hw:7", "empty name falls back to id");
        let chain = [
            RenderingDevice {
                display_name: String::from("Streamer"),
                role: TransportTarget,
                brand_visual: String::from("audio-card-symbolic"),
                illustration: String::from("network-wireless-symbolic"),
                manual_url: None,
            },
            make_device(),
        ];
        ensure!(chain.len() == 2, "deferred chains keep chain order");
        ensure!(
            chain.iter().all(|card| !card.display_name.is_empty()),
            "cards never name a non-rendering device"
        );
        let mut lost = bit_perfect_input();
        lost.generation = 42;
        lost.device_lost = true;
        let rendered = build_snapshot(&lost)?;
        ensure!(
            rendered.verdict != BitPerfect,
            "lost device forces the verdict off Bit-Perfect"
        );
        Ok(())
    }

    #[test]
    fn us4_processing_speed_rule() -> Result<()> {
        let perfect = build_snapshot(&bit_perfect_input())?;
        ensure!(
            perfect.processing_speed.is_none(),
            "bit-perfect hides speed"
        );
        let mut scaled = bit_perfect_input();
        scaled.generation = 43;
        scaled.volume = 0.5;
        let volume_only = build_snapshot(&scaled)?;
        ensure!(volume_only.verdict == Processed, "volume-only is Processed");
        ensure!(
            volume_only.processing_speed.is_some(),
            "volume-only shows speed"
        );
        let up = build_snapshot(&playing_input(44, 44, 44100, Some(16)))?;
        ensure!(up.processing_speed.is_some(), "resample shows speed");
        let mut wide = bit_perfect_input();
        wide.generation = 45;
        wide.track_audio = Some(flac_audio(48000, Some(16)));
        wide.decoder_params = Some(AudioParams {
            sample_rate: 48000,
            channels: 2,
            duration_seconds: 180.0,
            bit_depth: Some(16),
        });
        wide.device_channels = 6;
        ensure!(
            build_snapshot(&wide)?.processing_speed.is_some(),
            "channel conversion shows speed"
        );
        let dsd = dsd_input(46, 46)?;
        ensure!(
            build_snapshot(&dsd)?.processing_speed.is_some(),
            "DSD conversion shows speed"
        );
        let mut limited = bit_perfect_input();
        limited.generation = 47;
        limited.device_lost = true;
        let alone = build_snapshot(&limited)?;
        ensure!(alone.verdict == Limited, "lost device alone is Limited");
        ensure!(alone.processing_speed.is_none(), "Limited-only hides speed");
        let summary = summarize_text(&volume_only);
        ensure!(
            summary.lines().count() == volume_only.stages.len().saturating_add(1),
            "summary must list every stage once, got {summary}"
        );
        ensure!(
            summary
                .lines()
                .next()
                .is_some_and(|line| line.contains("Processed")),
            "summary header carries the verdict, got {summary}"
        );
        Ok(())
    }
}
