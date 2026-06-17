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
            Align::Start, Box, GestureClick, Image, Label, Orientation::Vertical, Overlay,
            ScrolledWindow, Widget, pango::EllipsizeMode::End,
        },
        prelude::{BoxExt, WidgetExt},
    },
    tracing::info,
};

use crate::{
    app::{AppState, NavigationEvent::ArtistDetail},
    storage::{
        Artist, Storage,
        settings::ViewMode::{self, Column, Grid},
    },
    ui::library::{
        common::{populate_grid_batched, populate_list_batched},
        empty::{EmptyStateParams, build_empty_state, build_library_grid},
    },
};

/// Size of artist avatar icons in pixels.
const AVATAR_SIZE: i32 = 180;

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
        Ok(a) => a
            .into_iter()
            .filter(|a| a.album_count > 0)
            .collect::<Vec<_>>(),
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

    let batch_size = 50;
    let mut remaining = cards;
    match mode {
        Grid => populate_grid_batched(
            container,
            &mut remaining,
            batch_size,
            "Artist library grid \u{2014} click an artist to view albums",
        ),
        Column => populate_list_batched(
            container,
            &mut remaining,
            batch_size,
            "Artist library list \u{2014} click an artist to view albums",
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
/// Returns a `Box` containing a vertical layout with avatar,
/// name, and album count labels. Matches the album card structural
/// pattern (Overlay wrapper) for consistent card sizing.
fn build_artist_card(state: &Arc<AppState>, artist: &Artist) -> Box {
    let card = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .css_classes(["card"])
        .can_focus(true)
        .tooltip_text(format!("View albums by {}", artist.name))
        .build();

    let avatar = build_artist_avatar();

    let overlay = Overlay::new();
    overlay.set_child(Some(&avatar));
    overlay.set_css_classes(&["cover-overlay"]);

    card.append(&overlay.upcast::<Widget>());

    let name_label = Label::builder()
        .label(&artist.name)
        .ellipsize(End)
        .max_width_chars(20)
        .css_classes(["heading", "title"])
        .halign(Start)
        .build();

    let album_count_label = Label::builder()
        .label(format!("{} albums", artist.album_count))
        .ellipsize(End)
        .max_width_chars(20)
        .css_classes(["dim-label", "caption"])
        .halign(Start)
        .build();

    card.append(&name_label);
    card.append(&album_count_label);

    let gesture = GestureClick::new();
    let state_clone = Arc::clone(state);
    let artist_id = artist.id;
    gesture.connect_released(move |_, _, _, _| {
        let state = Arc::clone(&state_clone);
        spawn_future_local(async move {
            state.send_navigation_event(ArtistDetail(artist_id)).await;
        });
    });
    card.add_controller(gesture);

    card
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::prelude::WidgetExt,
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
            card.first_child().is_some(),
            "artist card must have child content"
        );
        Ok(())
    }
}
