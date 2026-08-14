//! Active library tab preference.

use serde::{Deserialize, Serialize};

/// Active tab in the library view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActiveTab {
    /// Albums tab.
    Albums,
    /// Artists tab.
    Artists,
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        tempfile::tempdir,
    };

    use crate::storage::{
        active_tab::ActiveTab::{Albums, Artists},
        config::persistence::SettingsStore,
        settings::UserSettings,
    };

    #[test]
    fn active_tab_round_trips() -> Result<()> {
        let dir = tempdir()?;
        let mut store = SettingsStore {
            settings_path: dir.path().join("settings.json"),
            settings: UserSettings::default(),
        };
        ensure!(store.get_active_tab() == Albums);
        store.update_memory(|s| s.active_tab = Artists);
        ensure!(store.get_active_tab() == Artists);
        Ok(())
    }
}
