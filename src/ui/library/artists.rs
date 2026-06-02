//! Artist grid/column view.
//!
//! Displays artists in a responsive `FlowBox` grid. Each artist cell shows
//! an artist icon, name, and album count. Clicking an artist navigates to
//! the artist detail page.
//! Shows an inline empty state when no artists are available.

use std::sync::Arc;

use {
    libadwaita::{
        glib::{prelude::Cast, spawn_future_local},
        gtk::{
            Align::Center, Box, Button, Image, Label, Orientation::Vertical, ScrolledWindow,
            Widget, pango::EllipsizeMode::End,
        },
        prelude::{BoxExt, ButtonExt},
    },
    tracing::info,
};

use crate::{
    app::AppState,
    storage::{
        Artist, Storage,
        settings::ViewMode::{self, Column, Grid},
    },
    ui::library::empty::{
        EmptyStateParams, build_empty_state, build_library_grid, populate_grid, populate_list,
    },
};

/// Size of artist avatar icons in pixels.
const AVATAR_SIZE: i32 = 120;

/// Build the artist grid view.
///
/// Creates a `ScrolledWindow` containing a `FlowBox` populated with
/// artist cards loaded asynchronously from storage. Shows an inline
/// empty state when no artists are available.
#[must_use]
pub fn build_artist_grid(state: &Arc<AppState>) -> ScrolledWindow {
    build_library_grid(
        state,
        "Artist library grid \u{2014} click an artist to view albums",
        |state, container, scrolled, mode| {
            spawn_future_local(async move {
                load_artists(&state, &container, &scrolled, mode).await;
            });
        },
    )
}

/// Load artists from storage and populate the container.
///
/// If no artists exist, shows an inline empty state with an "Add Folder" button.
/// Populates either a grid (`FlowBox`) or column (`ListBox`) depending on view mode.
async fn load_artists(
    state: &Arc<AppState>,
    container: &Box,
    scrolled: &ScrolledWindow,
    mode: ViewMode,
) {
    let artists = match state.storage.get_all_artists().await {
        Ok(a) => a,
        Err(e) => {
            info!(error = %e, "Failed to load artists");
            return;
        }
    };

    if artists.is_empty() {
        let empty_widget = build_empty_state(
            state,
            &EmptyStateParams {
                icon_name: "avatar-default-symbolic",
                icon_label: "Artist icon",
                heading: "No Artists Found",
                heading_label: "No artists found",
                description: "Add a music folder to see your artists here.",
                description_label: "Add a music folder to see your artists here.",
            },
        );
        scrolled.set_child(Some(&empty_widget));
        return;
    }

    let cards: Vec<Widget> = artists
        .iter()
        .map(|artist| build_artist_card(state, artist).upcast())
        .collect();

    match mode {
        Grid => populate_grid(
            container,
            "Artist library grid \u{2014} click an artist to view albums",
            cards,
        ),
        Column => populate_list(
            container,
            "Artist library list \u{2014} click an artist to view albums",
            cards,
        ),
    }
}

/// Build the avatar widget for an artist.
///
/// Returns an `Image` with a generic artist icon.
fn build_artist_avatar() -> Widget {
    let avatar = Image::builder()
        .icon_name("avatar-default-symbolic")
        .pixel_size(AVATAR_SIZE / 2)
        .width_request(AVATAR_SIZE)
        .height_request(AVATAR_SIZE)
        .css_classes(["artist-avatar", "dim-label"])
        .build();
    avatar.upcast()
}

/// Build a single artist card widget.
///
/// Returns a `Button` containing a vertical layout with avatar and
/// name/album count labels.
fn build_artist_card(state: &Arc<AppState>, artist: &Artist) -> Button {
    let card = Button::builder()
        .css_classes(["flat", "card"])
        .can_focus(true)
        .tooltip_text(format!("View albums by {}", artist.name))
        .build();

    let content = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .halign(Center)
        .build();

    let avatar = build_artist_avatar();
    content.append(&avatar);

    let name_label = Label::builder()
        .label(&artist.name)
        .ellipsize(End)
        .max_width_chars(20)
        .css_classes(["heading", "title"])
        .build();

    let album_count_label = Label::builder()
        .label(format!("{} albums", artist.album_count))
        .ellipsize(End)
        .max_width_chars(20)
        .css_classes(["dim-label", "caption"])
        .build();

    content.append(&name_label);
    content.append(&album_count_label);

    card.set_child(Some(&content));

    let _state = Arc::clone(state);
    let artist_id = artist.id;
    card.connect_clicked(move |_| {
        info!(artist_id, "Artist card clicked");
    });

    card
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::prelude::ButtonExt,
    };

    use crate::{app::AppState, storage::Artist, ui::library::artists::build_artist_card};

    #[test]
    #[ignore = "Requires GTK initialization (display server)"]
    fn artist_card_builds_successfully() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let artist = Artist {
            id: 1,
            name: "Test Artist".to_string(),
            album_count: 3,
        };
        let card = build_artist_card(&state, &artist);
        ensure!(
            card.child().is_some(),
            "artist card must have child content"
        );
        Ok(())
    }
}
