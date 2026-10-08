//! Active library tab preference.

use serde::{Deserialize, Serialize};

/// Active tab in the library view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActiveTab {
    /// Albums tab.
    Albums,
    /// Artists tab.
    Artists,
    /// Live audio signal path tab.
    Signal,
}

impl ActiveTab {
    /// Check if this is the albums tab.
    #[must_use]
    pub const fn is_albums(self) -> bool {
        matches!(self, Self::Albums)
    }

    /// Check if this is the signal path tab.
    #[must_use]
    pub const fn is_signal(self) -> bool {
        matches!(self, Self::Signal)
    }

    /// Cycle to the next tab in `ViewStack` order.
    ///
    /// Order matches [`build_library_stack`](crate::ui::panes::build_library_stack)
    /// insertion: albums, artists, signal, wrapping back to albums.
    ///
    /// # Returns
    ///
    /// The tab following `self`, wrapping from signal back to albums.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Albums => Self::Artists,
            Self::Artists => Self::Signal,
            Self::Signal => Self::Albums,
        }
    }

    /// Cycle to the previous tab in `ViewStack` order.
    ///
    /// Inverse of [`next`](Self::next): signal, artists, albums, wrapping
    /// back to signal.
    ///
    /// # Returns
    ///
    /// The tab preceding `self`, wrapping from albums back to signal.
    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Albums => Self::Signal,
            Self::Artists => Self::Albums,
            Self::Signal => Self::Artists,
        }
    }

    /// `ViewStack` child name for this tab.
    ///
    /// # Returns
    ///
    /// `albums`, `artists`, or `signal`, matching the names registered in
    /// the library view stack.
    #[must_use]
    pub const fn stack_name(self) -> &'static str {
        match self {
            Self::Albums => "albums",
            Self::Artists => "artists",
            Self::Signal => "signal",
        }
    }

    /// Parse a `ViewStack` child name into its tab.
    ///
    /// # Arguments
    ///
    /// * `name` - Visible child name (`albums`, `artists`, `signal`).
    ///
    /// # Returns
    ///
    /// The matching tab; unknown names fall back to the albums tab.
    #[must_use]
    pub fn from_stack_name(name: &str) -> Self {
        if name == "artists" {
            Self::Artists
        } else if name == "signal" {
            Self::Signal
        } else {
            Self::Albums
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        tempfile::tempdir,
    };

    use crate::storage::{
        active_tab::ActiveTab::{self, Albums, Artists, Signal},
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
        store.update_memory(|s| s.active_tab = Signal);
        ensure!(store.get_active_tab() == Signal);
        ensure!(Signal.is_signal());
        ensure!(!Albums.is_signal());
        Ok(())
    }

    #[test]
    fn next_cycles_in_stack_order() -> Result<()> {
        ensure!(Albums.next() == Artists, "albums must advance to artists");
        ensure!(Artists.next() == Signal, "artists must advance to signal");
        ensure!(Signal.next() == Albums, "signal must wrap to albums");
        Ok(())
    }

    #[test]
    fn prev_cycles_in_reverse_order() -> Result<()> {
        ensure!(Albums.prev() == Signal, "albums must wrap back to signal");
        ensure!(Artists.prev() == Albums, "artists must go back to albums");
        ensure!(Signal.prev() == Artists, "signal must go back to artists");
        Ok(())
    }

    #[test]
    fn stack_names_round_trip() -> Result<()> {
        ensure!(Albums.stack_name() == "albums");
        ensure!(Artists.stack_name() == "artists");
        ensure!(Signal.stack_name() == "signal");
        ensure!(Albums == ActiveTab::from_stack_name("albums"));
        ensure!(Artists == ActiveTab::from_stack_name("artists"));
        ensure!(Signal == ActiveTab::from_stack_name("signal"));
        ensure!(
            Albums == ActiveTab::from_stack_name("unknown"),
            "unknown names must fall back to albums"
        );
        Ok(())
    }
}
