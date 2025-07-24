use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Folder {
    pub id: i64,
    pub path: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Artist {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub artist_id: i64,
    pub year: Option<i32>,
    pub original_release_date: Option<String>,
    pub cover_art: Option<Vec<u8>>,
    pub folder_id: i64,
    pub dr_value: Option<u8>,
    pub dr_completed: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: i64,
    pub title: String,
    pub album_id: i64,
    pub artist_id: i64,
    pub path: String,
    pub duration: Option<u32>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub format: Option<String>,
    pub bit_depth: Option<u32>,
    pub frequency: Option<u32>,
}
