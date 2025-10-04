use crate::{
    data::db::crud::{fetch_album_by_id, fetch_artist_by_id, fetch_songs_by_album},
    playback::{
        error::PlaybackError::{self, DatabaseError},
        queue::QueueItem,
    },
};

use super::main::PlaybackController;

impl PlaybackController {
    /// Queues all songs from an album for playback
    ///
    /// This method fetches album, artist, and song information from the database,
    /// creates QueueItem objects for each song, clears the existing queue,
    /// adds the new items, sets the current album ID and index, and loads and plays
    /// the first song.
    ///
    /// # Arguments
    /// * `album_id` - The ID of the album to queue
    ///
    /// # Returns
    /// A `Result` indicating success or a `PlaybackError` on failure
    pub async fn queue_album(&mut self, album_id: i64) -> Result<(), PlaybackError> {
        // Fetch album information
        let album = fetch_album_by_id(&self.db_pool, album_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch album: {}", e)))?;

        // Fetch artist information
        let artist = fetch_artist_by_id(&self.db_pool, album.artist_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch artist: {}", e)))?;

        // Fetch songs for the album
        let songs = fetch_songs_by_album(&self.db_pool, album_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch songs: {}", e)))?;

        // Clear existing queue
        self.queue.clear();

        // Create QueueItem for each song
        let queue_items: Vec<QueueItem> = songs
            .into_iter()
            .map(|song| QueueItem {
                song_title: song.title,
                album_title: album.title.clone(),
                artist_name: artist.name.clone(),
                song_path: song.path,
                cover_art_path: album.cover_art.clone(),
                bit_depth: song.bit_depth,
                sample_rate: song.sample_rate,
                format: song.format,
                duration: song.duration,
            })
            .collect();

        // Add new items to queue
        self.queue.items = queue_items;

        // Set current album ID and index
        self.queue.current_album_id = Some(album_id);
        self.queue.current_index = if self.queue.items.is_empty() {
            None
        } else {
            Some(0)
        };

        // Load and play the first song if there are songs
        if let Some(first_song) = self.queue.current_song() {
            self.load_song(first_song.song_path.clone())?;
            self.play()?;
        }
        Ok(())
    }

    /// Queues all songs from an album, starting playback from a specific song
    ///
    /// This method fetches album, artist, and song information from the database,
    /// creates QueueItem objects for all songs in the album, clears the existing queue,
    /// adds all items to the queue, sets the current album ID and index to the selected song,
    /// and loads and plays the selected song.
    ///
    /// # Arguments
    /// * `album_id` - The ID of the album
    /// * `start_song_id` - The ID of the song to start playing from
    ///
    /// # Returns
    /// A `Result` indicating success or a `PlaybackError` on failure
    pub async fn queue_songs_from(
        &mut self,
        album_id: i64,
        start_song_id: i64,
    ) -> Result<(), PlaybackError> {
        // Fetch album information
        let album = fetch_album_by_id(&self.db_pool, album_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch album: {}", e)))?;

        // Fetch artist information
        let artist = fetch_artist_by_id(&self.db_pool, album.artist_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch artist: {}", e)))?;

        // Fetch songs for the album
        let songs = fetch_songs_by_album(&self.db_pool, album_id)
            .await
            .map_err(|e| DatabaseError(format!("Failed to fetch songs: {}", e)))?;

        // Find the starting song position
        let start_index = songs
            .iter()
            .position(|song| song.id == start_song_id)
            .ok_or_else(|| DatabaseError("Start song not found in album".to_string()))?;

        // Clear existing queue
        self.queue.clear();

        // Create QueueItem for each song in the album
        let queue_items: Vec<QueueItem> = songs
            .iter()
            .map(|song| QueueItem {
                song_title: song.title.clone(),
                album_title: album.title.clone(),
                artist_name: artist.name.clone(),
                song_path: song.path.clone(),
                cover_art_path: album.cover_art.clone(),
                bit_depth: song.bit_depth,
                sample_rate: song.sample_rate,
                format: song.format.clone(),
                duration: song.duration,
            })
            .collect();

        // Add all items to queue
        self.queue.items = queue_items;

        // Set current album ID and index to the selected song
        self.queue.current_album_id = Some(album_id);
        self.queue.current_index = if self.queue.items.is_empty() {
            None
        } else {
            Some(start_index)
        };

        // Load and play the selected song if there are songs
        if let Some(selected_song) = self.queue.current_song() {
            self.load_song(selected_song.song_path.clone())?;
            self.play()?;
        }
        Ok(())
    }
}
