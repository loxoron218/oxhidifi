//! Album catalog queries and format-info aggregation.

use std::collections::HashMap;

use {
    sqlx::{FromRow, QueryBuilder, query_as},
    tracing::warn,
};

use crate::storage::{
    StorageError::Database,
    StorageResult,
    catalog::{Album, NewAlbum},
    database::SqliteStorage,
    formats::FormatInfo,
};

/// Subquery fragment for album count and duration columns.
macro_rules! album_meta_cols {
    () => {
        "(SELECT COUNT(*) FROM tracks WHERE album_id = al.id) AS track_count, (SELECT \
         COALESCE(SUM(duration), 0.0) FROM tracks WHERE album_id = al.id) AS total_duration, \
         al.format_summary, al.lossless, al.format, al.bit_depth, al.sample_rate FROM albums al"
    };
}

impl From<FormatInfoRow> for FormatInfo {
    fn from(row: FormatInfoRow) -> Self {
        raw_info_to_format_info(
            row.formats,
            row.sample_rates.as_deref(),
            row.bit_depths.as_deref(),
            row.channels.as_deref(),
        )
    }
}

/// Raw row from the `GROUP_CONCAT` format info query.
#[derive(Debug, Clone, FromRow)]
struct FormatInfoRow {
    /// Album identifier.
    album_id: i64,
    /// Comma-separated distinct format/codec names.
    formats: Option<String>,
    /// Comma-separated distinct sample rates.
    sample_rates: Option<String>,
    /// Comma-separated distinct bit depths.
    bit_depths: Option<String>,
    /// Comma-separated distinct channel counts.
    channels: Option<String>,
}

impl SqliteStorage {
    /// Insert a new album row and return its ID.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the insert query fails.
    pub async fn insert_album_row(&self, album: &NewAlbum) -> StorageResult<i64> {
        let row_id: (i64,) = query_as(
            "INSERT INTO albums (title, artist_id, year, genre, artwork_path, format_summary, \
             lossless, format, bit_depth, sample_rate) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             RETURNING id",
        )
        .bind(&album.title)
        .bind(album.artist_id)
        .bind(album.year)
        .bind(&album.genre)
        .bind(&album.artwork_path)
        .bind(&album.format_summary)
        .bind(album.lossless)
        .bind(&album.format)
        .bind(album.bit_depth)
        .bind(album.sample_rate)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Database(format!("Insert album failed: {e}")))?;

        Ok(row_id.0)
    }

    /// Fetch a single album row by ID.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn get_album_row(&self, id: i64) -> StorageResult<Option<Album>> {
        query_as::<_, Album>(concat!(
            "SELECT al.id, al.title, al.artist_id, al.year, al.genre, al.artwork_path, ",
            album_meta_cols!(),
            " WHERE al.id = ?",
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Database(format!("Get album failed: {e}")))
    }

    /// Fetch all album rows, ordered by title.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn all_albums_rows(&self) -> StorageResult<Vec<Album>> {
        query_as::<_, Album>(concat!(
            "SELECT al.id, al.title, al.artist_id, al.year, al.genre, al.artwork_path, ",
            album_meta_cols!(),
            " ORDER BY al.title",
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Database(format!("Get all albums failed: {e}")))
    }

    /// Fetch the distinct format info for a single album.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn album_format_info_rows(&self, album_id: i64) -> StorageResult<FormatInfo> {
        #[derive(Debug, Clone, FromRow)]
        struct RawInfo {
            formats: Option<String>,
            sample_rates: Option<String>,
            bit_depths: Option<String>,
            channels: Option<String>,
        }

        let row: Option<RawInfo> = query_as(
            "SELECT GROUP_CONCAT(DISTINCT UPPER(codec)) AS formats, GROUP_CONCAT(DISTINCT \
             sample_rate) AS sample_rates, GROUP_CONCAT(DISTINCT bit_depth) AS bit_depths, \
             GROUP_CONCAT(DISTINCT channels) AS channels FROM tracks WHERE album_id = ?",
        )
        .bind(album_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Database(format!("Get album format info failed: {e}")))?;

        Ok(row.map_or_else(FormatInfo::default, |r| {
            raw_info_to_format_info(
                r.formats,
                r.sample_rates.as_deref(),
                r.bit_depths.as_deref(),
                r.channels.as_deref(),
            )
        }))
    }

    /// Fetch the distinct format info for multiple albums at once.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn albums_format_info_rows(
        &self,
        album_ids: &[i64],
    ) -> StorageResult<HashMap<i64, FormatInfo>> {
        if album_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut builder = QueryBuilder::new(
            "SELECT album_id, GROUP_CONCAT(DISTINCT UPPER(codec)) AS formats, \
             GROUP_CONCAT(DISTINCT sample_rate) AS sample_rates, GROUP_CONCAT(DISTINCT bit_depth) \
             AS bit_depths, GROUP_CONCAT(DISTINCT channels) AS channels FROM tracks WHERE \
             album_id IN (",
        );

        let mut separated = builder.separated(", ");
        for id in album_ids {
            _ = separated.push_bind(id);
        }
        _ = builder.push(") GROUP BY album_id");

        let rows: Vec<FormatInfoRow> = builder
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Database(format!("Get albums format info failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|r| (r.album_id, FormatInfo::from(r)))
            .collect())
    }

    /// Fetch all album rows by artist, ordered by year.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Database`] if the query fails.
    pub async fn albums_by_artist_rows(&self, artist_id: i64) -> StorageResult<Vec<Album>> {
        query_as::<_, Album>(concat!(
            "SELECT al.id, al.title, al.artist_id, al.year, al.genre, al.artwork_path, ",
            album_meta_cols!(),
            " WHERE al.artist_id = ? ORDER BY al.year",
        ))
        .bind(artist_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Database(format!("Get albums by artist failed: {e}")))
    }
}

/// Parse a comma-separated string of integers, logging parse failures.
fn parse_int_list(s: &str) -> Vec<i32> {
    s.split(',')
        .filter_map(|v| {
            let trimmed = v.trim();
            match trimmed.parse::<i32>() {
                Ok(n) => Some(n),
                Err(e) => {
                    warn!(
                        error = %e,
                        value = trimmed,
                        "Skipping unparseable integer in format info",
                    );
                    None
                }
            }
        })
        .collect()
}

/// Parse comma-separated format info strings into a `FormatInfo`.
fn raw_info_to_format_info(
    formats: Option<String>,
    sample_rates: Option<&str>,
    bit_depths: Option<&str>,
    channels: Option<&str>,
) -> FormatInfo {
    FormatInfo {
        formats: formats.map_or_else(Vec::new, |s| {
            s.split(',').map(str::trim).map(str::to_string).collect()
        }),
        sample_rates: sample_rates.map_or_else(Vec::new, parse_int_list),
        bit_depths: bit_depths.map_or_else(Vec::new, parse_int_list),
        channels: channels.map_or_else(Vec::new, parse_int_list),
    }
}
