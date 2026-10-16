//! Artist detail page with album groupings and track listings.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        glib::spawn_future_local,
        gtk::{
            Align::Start, Box, Frame, Label, ListBox, ListBoxRow, Orientation::Vertical,
            ToggleButton, Widget, accessible::Property::Label as PropertyLabel,
            pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, Cast, ToggleButtonExt, WidgetExt},
    },
    tracing::{error, info},
};

use crate::{
    app::runtime::{AppState, NavigationEvent},
    playback::transport::PlaybackTransport,
    ui::{
        detail::{
            artist_actions::{build_artist_actions, refresh_collapse_all_visual},
            artist_album_section::persist_collapse_preference,
            artist_load::{apply_artist_detail, fetch_artist_detail},
            body_compose::DETAIL_COVER_SIZE,
            page::{build_detail_wrapper, build_scroll_content},
        },
        gallery::{
            avatar::build_artist_avatar,
            play_action::{play_artist, play_artist_shuffled},
        },
    },
};

/// Build the artist detail page widget.
pub fn build_artist_detail(
    state: &Arc<AppState>,
    artist_id: i64,
    nav_tx: &Sender<NavigationEvent>,
) -> Widget {
    let wrapper = build_detail_wrapper(nav_tx, "Artist");

    let (scroll, content) = build_scroll_content();

    let artist_image = build_artist_avatar(DETAIL_COVER_SIZE);
    let artist_frame = Frame::builder()
        .child(&artist_image)
        .width_request(DETAIL_COVER_SIZE)
        .height_request(DETAIL_COVER_SIZE)
        .halign(Start)
        .css_classes(["card"])
        .tooltip_text("Artist image")
        .build();
    artist_frame.update_property(&[PropertyLabel("Artist image")]);
    content.append(&artist_frame);

    let name_label = Label::builder()
        .css_classes(["title-2", "heading"])
        .ellipsize(End)
        .hexpand(true)
        .halign(Start)
        .tooltip_text("Artist name")
        .build();
    name_label.update_property(&[PropertyLabel("Artist name")]);
    content.append(&name_label);

    let album_count_label = Label::builder()
        .css_classes(["dim-label", "body"])
        .hexpand(true)
        .halign(Start)
        .tooltip_text("Album count")
        .build();
    album_count_label.update_property(&[PropertyLabel("Album count")]);
    content.append(&album_count_label);

    let albums_expanded = !state.storage.get_artist_albums_collapsed();
    let (actions, play_button, shuffle_button, collapse_button) =
        build_artist_actions(state.playback.shuffle_enabled(), albums_expanded);
    let shuffle_for_play = shuffle_button.clone();
    let play_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(play_button.connect_clicked(move |_| {
            shuffle_for_play.set_active(false);
            let state_cb = Arc::clone(&play_state);
            play_state
                .handles
                .lock()
                .retain_task(spawn_future_local(async move {
                    play_artist(&state_cb, artist_id).await;
                }));
        }));
    let shuffle_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(shuffle_button.connect_toggled(move |button| {
            if !button.is_active() {
                disable_shuffle(&shuffle_state);
                return;
            }
            let state_cb = Arc::clone(&shuffle_state);
            shuffle_state
                .handles
                .lock()
                .retain_task(spawn_future_local(async move {
                    play_artist_shuffled(&state_cb, artist_id).await;
                }));
        }));
    content.append(&actions);

    let albums_container = Box::builder()
        .orientation(Vertical)
        .spacing(18)
        .css_classes(["albums-container"])
        .tooltip_text("Artist albums")
        .build();
    albums_container.update_property(&[PropertyLabel("Artist albums")]);
    content.append(&albums_container);

    scroll.set_child(Some(&content));
    wrapper.append(&scroll);

    wire_collapse_all(state, &collapse_button, &albums_container);

    let sc = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            if let Some(data) = fetch_artist_detail(&sc, artist_id).await {
                apply_artist_detail(
                    &name_label,
                    &album_count_label,
                    &albums_container,
                    &collapse_button,
                    &sc,
                    data,
                );
            }
        }));

    wrapper.upcast()
}

/// Set every loaded album section to the shared expanded state.
///
/// Walks the albums container instead of sharing a widget registry, so no
/// cross-thread state is needed: toggling before data loads simply persists
/// the preference that sections initialize from.
///
/// # Arguments
///
/// * `container` - Albums container holding one section box per album.
/// * `expanded` - Whether sections should be revealed.
fn set_all_sections(container: &Box, expanded: bool) {
    let mut next = container.first_child();
    while let Some(child) = next {
        next = child.next_sibling();
        let Ok(section) = child.downcast::<Box>() else {
            continue;
        };
        let Some(header) = section.first_child() else {
            continue;
        };
        let Ok(header) = header.downcast::<Box>() else {
            continue;
        };
        let Some(toggle) = header.first_child() else {
            continue;
        };
        let Ok(toggle) = toggle.downcast::<ToggleButton>() else {
            continue;
        };
        if toggle.is_active() != expanded {
            toggle.set_active(expanded);
        }
    }
}

/// Wire the collapse-all toggle to every album section.
///
/// Driving each section toggle reuses its own visibility-only handler, so the
/// master fans out the shared state and is the sole writer of the persisted
/// default; the debounced saver coalesces the burst.
///
/// # Arguments
///
/// * `state` - Application state owning signal handles and settings.
/// * `master` - Collapse-all toggle in the header actions.
/// * `container` - Albums container holding the section boxes.
fn wire_collapse_all(state: &Arc<AppState>, master: &ToggleButton, container: &Box) {
    let sections = container.clone();
    let persist = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(master.connect_toggled(move |button| {
            let expanded = button.is_active();
            refresh_collapse_all_visual(button, expanded);
            info!(expanded, "Artist albums collapse-all toggled",);
            set_all_sections(&sections, expanded);
            persist_collapse_preference(&persist, !expanded);
        }));
}

/// Disable shuffle mode, logging failures.
///
/// Called when the shuffle toggle is switched off or when ordered playback
/// starts via the Play button.
///
/// # Arguments
///
/// * `state` - Application state.
fn disable_shuffle(state: &AppState) {
    if let Err(e) = state.playback.set_shuffle_enabled(false) {
        error!(error = %e, "Failed to disable shuffle mode");
    }
}

/// When a row is selected in one album's track list, unselect all rows
/// in the other albums' track lists to keep a single active highlight.
///
/// # Arguments
///
/// * `row` - Newly selected row, or `None` when the selection was cleared.
/// * `others` - Track lists belonging to sibling album sections.
pub fn clear_other_lists(row: Option<&ListBoxRow>, others: &[ListBox]) {
    if row.is_none() {
        return;
    }
    for other in others {
        other.unselect_all();
    }
}

#[cfg(test)]
mod tests {
    use std::slice::from_ref;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, ListBox, ListBoxRow, accessible::Property::Label, test},
            prelude::AccessibleExtManual,
        },
    };

    use crate::ui::detail::artist_page::clear_other_lists;

    #[test]
    fn clear_other_lists_keeps_active_selection() -> Result<()> {
        let active = ListBox::builder()
            .css_classes(["test-list"])
            .tooltip_text("Test list")
            .build();
        active.update_property(&[Label("Test list")]);
        let other = ListBox::builder()
            .css_classes(["test-list"])
            .tooltip_text("Test list")
            .build();
        other.update_property(&[Label("Test list")]);
        let active_row = ListBoxRow::builder()
            .css_classes(["test-row"])
            .tooltip_text("Test row")
            .build();
        active_row.update_property(&[Label("Test row")]);
        let other_row = ListBoxRow::builder()
            .css_classes(["test-row"])
            .tooltip_text("Test row")
            .build();
        other_row.update_property(&[Label("Test row")]);
        active.append(&active_row);
        other.append(&other_row);
        active.select_row(Some(&active_row));
        other.select_row(Some(&other_row));
        ensure!(active.selected_row().is_some());
        ensure!(other.selected_row().is_some());

        clear_other_lists(Some(&active_row), from_ref(&other));

        ensure!(active.selected_row().is_some());
        ensure!(other.selected_row().is_none());
        Ok(())
    }

    #[test]
    fn clear_other_lists_none_row_is_noop() -> Result<()> {
        let other = ListBox::builder()
            .css_classes(["test-list"])
            .tooltip_text("Test list")
            .build();
        other.update_property(&[Label("Test list")]);
        let other_row = ListBoxRow::builder()
            .css_classes(["test-row"])
            .tooltip_text("Test row")
            .build();
        other_row.update_property(&[Label("Test row")]);
        other.append(&other_row);
        other.select_row(Some(&other_row));

        clear_other_lists(None, from_ref(&other));

        ensure!(other.selected_row().is_some());
        Ok(())
    }
}
