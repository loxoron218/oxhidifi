//! Volume and output-mode controls for the player deck.
//!
//! Provides the volume slider with mute and bit-perfect/resampled
//! output-mode toggle used by the side player panel.

use std::sync::Arc;

use {
    libadwaita::{
        gtk::{Box, Button, Orientation::Horizontal, Scale, accessible::Property::Label},
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, RangeExt, ScaleExt, WidgetExt},
    },
    tracing::error,
};

use crate::{
    app::runtime::AppState,
    playback::{
        devices::OutputMode::{self, BitPerfect, Resampled},
        state::MuteState::Unmuted,
        transport::PlaybackTransport,
        volume::format_volume_db,
    },
    storage::database::SqliteStorage,
};

/// Build the volume control section with an output-mode toggle button.
///
/// Returns the container box, the mode toggle button, and the volume scale
/// (for event-driven visual updates by the panel).
pub fn build_volume(state: &Arc<AppState>) -> (Box, Button, Scale) {
    let vol_box = Box::builder().orientation(Horizontal).spacing(6).build();

    let mute_button = Button::builder()
        .icon_name("audio-volume-high-symbolic")
        .css_classes(["flat"])
        .tooltip_text("Mute or unmute")
        .can_focus(true)
        .build();
    mute_button.update_property(&[Label("Mute or unmute")]);
    let state_mute = Arc::clone(state);
    let mute_btn_ref = mute_button.clone();
    state
        .handles
        .lock()
        .retain_signal(mute_button.connect_clicked(move |_| {
            let current = state_mute.playback.state();
            let new_muted = current.muted == Unmuted;
            if let Err(e) = state_mute.playback.set_muted(new_muted) {
                error!(error = %e, "Failed to set mute");
            }
            let icon = if new_muted {
                "audio-volume-muted-symbolic"
            } else {
                "audio-volume-high-symbolic"
            };
            mute_btn_ref.set_icon_name(icon);
        }));
    vol_box.append(&mute_button);

    let initial_volume = state.playback.state().volume;
    let volume_scale = Scale::with_range(Horizontal, 0.0, 1.0, 0.01);
    volume_scale.set_value(initial_volume);
    volume_scale.set_draw_value(false);
    volume_scale.set_hexpand(true);
    volume_scale.set_can_focus(true);
    let initial_db = format_volume_db(initial_volume);
    volume_scale.set_tooltip_text(Some(&format!("Volume: {initial_db}")));
    volume_scale.update_property(&[Label(&format!("Adjust volume ({initial_db})"))]);
    let state_vol = Arc::clone(state);
    let vol_ref = volume_scale.clone();
    state
        .handles
        .lock()
        .retain_signal(volume_scale.connect_value_changed(move |_| {
            let value = vol_ref.value();
            if let Err(e) = state_vol.playback.set_volume(value) {
                error!(error = %e, "Failed to set volume");
            }
            let db = format_volume_db(value);
            vol_ref.set_tooltip_text(Some(&format!("Volume: {db}")));
            vol_ref.update_property(&[Label(&format!("Adjust volume ({db})"))]);
            state_vol.storage.set_volume_memory(value);
            state_vol.storage.save_settings();
        }));
    vol_box.append(&volume_scale);

    let initial_mode = state.playback.state().output_mode;
    let mode_button = Button::builder()
        .icon_name(initial_mode.icon_name())
        .css_classes(["flat", "caption"])
        .tooltip_text(mode_button_tooltip(initial_mode))
        .can_focus(true)
        .build();
    mode_button.update_property(&[Label(mode_button_tooltip(initial_mode))]);
    let state_mode = Arc::clone(state);
    let scale_for_click = volume_scale.clone();
    state
        .handles
        .lock()
        .retain_signal(mode_button.connect_clicked(move |btn| {
            let current_mode = state_mode.playback.state().output_mode;
            let new_mode = match current_mode {
                BitPerfect => Resampled,
                Resampled => BitPerfect,
            };
            if let Err(e) = state_mode.playback.set_output_mode(new_mode) {
                error!(error = %e, "Failed to toggle output mode");
            }
            persist_toggle_output_mode(&state_mode.storage, new_mode);
            btn.set_icon_name(new_mode.icon_name());
            btn.set_tooltip_text(Some(mode_button_tooltip(new_mode)));
            btn.update_property(&[Label(mode_button_tooltip(new_mode))]);
            update_volume_scale_visual(&scale_for_click, new_mode);
        }));
    vol_box.append(&mode_button);

    update_volume_scale_visual(&volume_scale, initial_mode);

    (vol_box, mode_button, volume_scale)
}

/// Update the volume scale's visual state based on the output mode.
///
/// In bit-perfect mode the scale is greyed out and interaction is
/// prevented — the volume is controlled via the ALSA hardware mixer.
/// In resampled mode the tooltip reflects the dB attenuation per FR-020.
pub fn update_volume_scale_visual(scale: &Scale, mode: OutputMode) {
    match mode {
        Resampled => {
            scale.set_sensitive(true);
            let db = format_volume_db(scale.value());
            scale.set_tooltip_text(Some(&format!("Volume: {db}")));
            scale.update_property(&[Label(&format!("Adjust volume ({db})"))]);
        }
        BitPerfect => {
            scale.set_sensitive(false);
            scale.set_tooltip_text(Some(
                "Volume controlled via hardware mixer \u{2014} switch to Resampled for software \
                 volume",
            ));
            scale.update_property(&[Label(
                "Volume controlled via hardware mixer, switch to Resampled for software volume",
            )]);
        }
    }
}

/// Persist the output mode toggled from the side panel.
fn persist_toggle_output_mode(storage: &Arc<SqliteStorage>, mode: OutputMode) {
    storage.set_output_mode_memory(mode);
    storage.save_settings();
}

/// Tooltip text for the mode toggle button.
#[must_use]
pub const fn mode_button_tooltip(mode: OutputMode) -> &'static str {
    match mode {
        BitPerfect => {
            "Bit-Perfect mode \u{2014} no software volume scaling, hardware volume via ALSA mixer"
        }
        Resampled => "Resampled mode \u{2014} software volume scaling, sample rate conversion",
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, Orientation::Horizontal, Scale, test},
            prelude::WidgetExt,
        },
    };

    use crate::{
        playback::devices::OutputMode::{BitPerfect, Resampled},
        ui::player::acoustic_fader::{mode_button_tooltip, update_volume_scale_visual},
    };

    #[test]
    fn mode_button_tooltip_matches_mode() -> Result<()> {
        let bit_perfect = mode_button_tooltip(BitPerfect);
        ensure!(bit_perfect.contains("Bit-Perfect"));
        let resampled = mode_button_tooltip(Resampled);
        ensure!(resampled.contains("Resampled"));
        ensure!(bit_perfect != resampled);
        Ok(())
    }

    #[test]
    fn update_volume_scale_visual_toggles_sensitivity() -> Result<()> {
        let scale = Scale::with_range(Horizontal, 0.0, 1.0, 0.01);
        update_volume_scale_visual(&scale, Resampled);
        ensure!(scale.is_sensitive());
        update_volume_scale_visual(&scale, BitPerfect);
        ensure!(!scale.is_sensitive());
        update_volume_scale_visual(&scale, Resampled);
        ensure!(scale.is_sensitive());
        Ok(())
    }
}
