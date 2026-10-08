//! Artist-scoped user preference API on `SqliteStorage`.
//!
//! Extracted from `user_prefs` to keep each file under 400 lines.

use crate::storage::{
    StorageError::{self, Database},
    database::SqliteStorage,
    sort_rules::ArtistPlayOrder,
};

impl SqliteStorage {
    /// Get the preferred album ordering for artist-wide playback.
    pub fn get_artist_play_order(&self) -> ArtistPlayOrder {
        self.settings.read().get_artist_play_order()
    }

    /// Set the preferred album ordering for artist-wide playback.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be saved.
    pub async fn set_artist_play_order(&self, order: ArtistPlayOrder) -> Result<(), StorageError> {
        self.settings
            .write()
            .update_memory(|s| s.artist_play_order = order);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save artist play order: {e}")))?;
        Ok(())
    }

    /// Set the preferred album ordering for artist-wide playback in memory.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_artist_play_order_memory(&self, order: ArtistPlayOrder) {
        self.settings
            .write()
            .update_memory(|s| s.artist_play_order = order);
    }

    /// Get whether album sections on artist detail pages start collapsed.
    pub fn get_artist_albums_collapsed(&self) -> bool {
        self.settings.read().get_artist_albums_collapsed()
    }

    /// Set whether album sections start collapsed, persisting to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if settings cannot be saved.
    pub async fn set_artist_albums_collapsed(&self, collapsed: bool) -> Result<(), StorageError> {
        self.settings
            .write()
            .update_memory(|s| s.artist_albums_collapsed = collapsed);
        self.save_settings_async()
            .await
            .map_err(|e| Database(format!("Failed to save album collapse state: {e}")))?;
        Ok(())
    }

    /// Set whether album sections start collapsed in memory.
    ///
    /// The debounced disk write is triggered via [`Self::save_settings`],
    /// which runs in the background so callers are not blocked.
    pub fn set_artist_albums_collapsed_memory(&self, collapsed: bool) {
        self.settings
            .write()
            .update_memory(|s| s.artist_albums_collapsed = collapsed);
    }
}

#[cfg(test)]
mod tests {
    use std::fs::read_to_string;

    use {
        anyhow::{Result, ensure},
        serde_json::from_str,
        tempfile::tempdir,
        tokio::test,
    };

    use crate::storage::{
        database::tests::storage_in,
        settings::UserSettings,
        sort_rules::ArtistPlayOrder::{DateAsc, TitleDesc},
    };

    #[test]
    async fn artist_play_order_round_trips() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;
        ensure!(
            storage.get_artist_play_order() == DateAsc,
            "default must be DateAsc"
        );
        storage.set_artist_play_order(TitleDesc).await?;
        ensure!(
            storage.get_artist_play_order() == TitleDesc,
            "order must round-trip"
        );
        Ok(())
    }

    #[test]
    async fn artist_albums_collapsed_round_trips() -> Result<()> {
        let dir = tempdir()?;
        let storage = storage_in(&dir).await?;
        let settings_path = storage.settings.read().path().to_path_buf();
        ensure!(
            !storage.get_artist_albums_collapsed(),
            "albums must default to expanded"
        );
        storage.set_artist_albums_collapsed(true).await?;
        ensure!(
            storage.get_artist_albums_collapsed(),
            "collapsed state must round-trip"
        );
        let file_settings: UserSettings = from_str(&read_to_string(&settings_path)?)?;
        ensure!(
            file_settings.artist_albums_collapsed,
            "collapsed state must reach the settings file on disk"
        );
        storage.set_artist_albums_collapsed_memory(false);
        ensure!(
            !storage.get_artist_albums_collapsed(),
            "memory setter must restore expanded state"
        );
        Ok(())
    }
}
