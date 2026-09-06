//! Settings persistence verification (T058) per FR-028.
//!
//! Configures audio device, view preferences, volume, and window geometry,
//! persists via `SettingsStore::save_sync`, reloads via
//! `SettingsStore::load_from_path`, and asserts all settings are restored from
//! the XDG config path.

use std::path::Path;

use oxhidifi::{
    playback::devices::OutputMode::{self, BitPerfect},
    storage::{
        active_tab::ActiveTab::{self, Artists},
        config::persistence::SettingsStore,
        view_mode::ViewMode::{self, Column},
    },
};

const fn assert_types(_: OutputMode, _: ActiveTab, _: ViewMode, _: &Path) {}

fn use_assert_types() {
    assert_types(BitPerfect, Artists, Column, Path::new("/tmp"));
}

/// Set every persisted preference on the in-memory settings.
fn configure_preferences(store: &mut SettingsStore) {
    use_assert_types();
    store.update_memory(|s| {
        s.audio_device = Some("hw:0".to_string());
        s.volume = 0.42;
        s.view_mode = Column;
        s.active_tab = Artists;
        s.window_width = 960;
        s.window_height = 640;
        s.window_maximized = true;
        s.gapless_enabled = false;
        s.show_album_labels = false;
        s.output_mode = BitPerfect;
    });
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        tempfile::tempdir,
    };

    use oxhidifi::{
        playback::devices::OutputMode::BitPerfect,
        storage::{
            active_tab::ActiveTab::Artists, config::persistence::SettingsStore,
            view_mode::ViewMode::Column,
        },
    };

    use {crate::configure_preferences, tokio::test};

    #[test]
    async fn settings_survive_save_and_reload() -> Result<()> {
        let dir = tempdir()?;
        let settings_path = dir.path().join("settings.json");

        let mut store = SettingsStore::load_from_path(&settings_path).await?;
        configure_preferences(&mut store);
        store.save_sync()?;
        drop(store);

        let store = SettingsStore::load_from_path(&settings_path).await?;
        let s = store.get();

        ensure!(
            s.audio_device.as_deref() == Some("hw:0"),
            "audio device not restored"
        );
        ensure!(
            (s.volume - 0.42).abs() < f64::EPSILON,
            "volume not restored"
        );
        ensure!(s.view_mode == Column, "view mode not restored");
        ensure!(s.active_tab == Artists, "active tab not restored");
        ensure!(s.window_width == 960, "window width not restored");
        ensure!(s.window_height == 640, "window height not restored");
        ensure!(s.window_maximized, "window maximized not restored");
        ensure!(!s.gapless_enabled, "gapless toggle not restored");
        ensure!(!s.show_album_labels, "album labels not restored");
        ensure!(s.output_mode == BitPerfect, "output mode not restored");

        Ok(())
    }

    #[test]
    async fn reload_uses_explicit_xdg_config_path() -> Result<()> {
        let dir = tempdir()?;
        let settings_path = dir.path().join("oxhidifi").join("settings.json");

        let mut store = SettingsStore::load_from_path(&settings_path).await?;
        store.update_memory(|s| s.volume = 0.9);
        store.save_sync()?;
        drop(store);

        ensure!(settings_path.exists(), "settings file must be written");

        let store = SettingsStore::load_from_path(&settings_path).await?;
        ensure!(
            (store.get_volume() - 0.9).abs() < f64::EPSILON,
            "volume not restored from explicit path"
        );
        Ok(())
    }
}
