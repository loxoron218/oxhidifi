//! Per-album section for the artist detail page.
//!
//! Builds a collapsible album header (disclosure toggle, thumbnail, title,
//! metadata, open button) with its track list, split from `artist_page` to
//! keep each module under the file-size limit.

use std::sync::Arc;

use {
    libadwaita::{
        glib::{idle_add_local, spawn_future_local},
        gtk::{
            Box, Button, ListBox, Orientation::Vertical, ToggleButton, accessible::Property::Label,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, ToggleButtonExt, WidgetExt},
    },
    tokio::runtime::Handle,
    tracing::info,
};

use crate::{
    app::runtime::{AppState, NavigationEvent::AlbumDetail},
    storage::{
        catalog::{Album, Track},
        formats::FormatInfo,
    },
    ui::detail::{
        album_header::{
            COLLAPSED_ICON, EXPANDED_ICON, build_album_header, toggle_label, toggle_tooltip,
        },
        tracklist::fill_track_list_batch,
    },
};

/// Build a section for a single album in the artist detail page.
///
/// Creates a header with disclosure toggle, thumbnail, title, metadata, an
/// explicit open button, and a track list. The toggle collapses the track
/// list in place; the open button navigates to the album's detail page. A
/// disclosure toggle plus separate open action keeps the two affordances
/// distinct for keyboard and screen-reader users, unlike a whole-header
/// click target.
///
/// Returns the section widget, track list box, and disclosure toggle.
/// Cover art is loaded asynchronously off the main thread.
///
/// # Arguments
///
/// * `state` - Application state.
/// * `album` - Album to display.
/// * `format_info` - Precomputed format summary for the album.
/// * `tracks` - Tracks belonging to the album, in display order.
/// * `expanded` - Whether the track list starts revealed.
///
/// # Returns
///
/// * `(Box, ListBox, ToggleButton)` - Section widget, its track list box for selection management,
///   and its disclosure toggle for per-section control.
pub fn build_album_section(
    state: &Arc<AppState>,
    album: &Album,
    format_info: &FormatInfo,
    tracks: Vec<Track>,
    expanded: bool,
) -> (Box, ListBox, ToggleButton) {
    let section = Box::builder().orientation(Vertical).spacing(6).build();
    let (header, toggle, open_button) = build_album_header(state, album, format_info, expanded);
    wire_open_button(state, &open_button, album.id);
    section.append(&header);
    let track_list = build_track_list(state, tracks, expanded);
    section.append(&track_list);
    wire_expand_toggle(state, &toggle, &track_list, album.id, &album.title);
    (section, track_list, toggle)
}

/// Build the track list with batched idle fill.
///
/// # Arguments
///
/// * `state` - Application state for row playback actions.
/// * `tracks` - Tracks belonging to the album, in display order.
/// * `expanded` - Whether the list starts visible.
///
/// # Returns
///
/// * `ListBox` - Track list populated in batches off the critical path.
fn build_track_list(state: &Arc<AppState>, tracks: Vec<Track>, expanded: bool) -> ListBox {
    let track_list = ListBox::builder().css_classes(["boxed-list"]).build();
    track_list.set_visible(expanded);
    let mut remaining: Vec<(Track, usize)> = tracks
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, i.saturating_add(1)))
        .collect();
    remaining.reverse();
    let list = track_list.clone();
    let state_cb = Arc::clone(state);
    state.handles.lock().retain_source(idle_add_local(move || {
        fill_track_list_batch(&mut remaining, &list, &state_cb)
    }));
    track_list
}

/// Wire the open button to navigate to the album detail page.
///
/// # Arguments
///
/// * `state` - Application state owning signal handles.
/// * `button` - Open button to wire.
/// * `album_id` - Album to open on click.
fn wire_open_button(state: &Arc<AppState>, button: &Button, album_id: i64) {
    let nav_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(button.connect_clicked(move |_| {
            let ns = Arc::clone(&nav_state);
            nav_state
                .handles
                .lock()
                .retain_task(spawn_future_local(async move {
                    ns.send_navigation_event(AlbumDetail(album_id)).await;
                }));
        }));
}

/// Wire the disclosure toggle to reveal only its own track list.
///
/// The toggle is intentionally in-session only: it never writes settings and
/// never touches sibling sections. The collapse-all master owns the persisted
/// global default that fresh pages initialize from.
///
/// # Arguments
///
/// * `state` - Application state owning signal handles.
/// * `toggle` - Disclosure toggle to wire.
/// * `track_list` - Track list revealed or hidden by the toggle.
/// * `album_id` - Album owning the section, for structured logging.
/// * `title` - Album title for tooltip and accessible-label updates.
fn wire_expand_toggle(
    state: &Arc<AppState>,
    toggle: &ToggleButton,
    track_list: &ListBox,
    album_id: i64,
    title: &str,
) {
    let list = track_list.clone();
    let toggle_title = title.to_owned();
    state
        .handles
        .lock()
        .retain_signal(toggle.connect_toggled(move |button| {
            let is_expanded = button.is_active();
            button.set_icon_name(if is_expanded {
                EXPANDED_ICON
            } else {
                COLLAPSED_ICON
            });
            button.set_tooltip_text(Some(&toggle_tooltip(&toggle_title, is_expanded)));
            button.update_property(&[Label(&toggle_label(&toggle_title, is_expanded))]);
            list.set_visible(is_expanded);
            info!(album_id, is_expanded, "Artist album section toggled",);
        }));
}

/// Update the global collapse preference and schedule a debounced save.
///
/// The debounced saver needs a Tokio runtime (present in the running app via
/// `Runtime::block_on`). Unit tests drive the toggle without one, so the disk
/// write is skipped there while the in-memory value still updates.
///
/// # Arguments
///
/// * `state` - Application state owning settings and the debounced saver.
/// * `collapsed` - Whether album sections should start collapsed.
pub fn persist_collapse_preference(state: &Arc<AppState>, collapsed: bool) {
    state.storage.set_artist_albums_collapsed_memory(collapsed);
    if Handle::try_current().is_ok() {
        state.storage.save_settings();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, anyhow, bail, ensure},
        libadwaita::{
            gtk::{self, Box, Button, ToggleButton, Widget, test},
            prelude::{Cast, ToggleButtonExt, WidgetExt},
        },
    };

    use crate::{
        app::{mocks::isolated_app_state, runtime::AppState},
        storage::{
            catalog::{Album, Track},
            formats::FormatInfo,
        },
        ui::detail::artist_album_section::build_album_section,
    };

    fn mock_album() -> Album {
        Album {
            id: 7,
            title: "Test Album".into(),
            artist_id: 3,
            year: Some(2024),
            genre: Some("Jazz".into()),
            artwork_path: None,
            track_count: 1,
            total_duration: 200.0,
            format_summary: String::new(),
            lossless: true,
            format: "FLAC".into(),
            bit_depth: Some(24),
            sample_rate: Some(96_000),
        }
    }

    fn section_header(section: &Box) -> Result<Box> {
        let Some(header) = section.first_child() else {
            bail!("section must start with a header")
        };
        header
            .downcast::<Box>()
            .map_err(|header| anyhow!("section header must be a box, got {header:?}"))
    }

    fn section_toggle(section: &Box) -> Result<ToggleButton> {
        let header = section_header(section)?;
        let Some(toggle) = header.first_child() else {
            bail!("header must start with a disclosure toggle")
        };
        toggle
            .downcast::<ToggleButton>()
            .map_err(|widget| anyhow!("header child must be a toggle, got {widget:?}"))
    }

    fn section_open_button(section: &Box) -> Result<Button> {
        let header = section_header(section)?;
        let mut next = header.first_child();
        let mut last: Option<Widget> = None;
        while let Some(child) = next {
            last = Some(child.clone());
            next = child.next_sibling();
        }
        let Some(last) = last else {
            bail!("header must hold an open button")
        };
        last.downcast::<Button>()
            .map_err(|widget| anyhow!("header tail must be a button, got {widget:?}"))
    }

    #[test]
    fn expanded_section_reveals_tracks_and_marks_toggle() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let (section, list, _) =
            build_album_section(&state, &mock_album(), &FormatInfo::default(), vec![], true);
        let toggle = section_toggle(&section)?;
        ensure!(toggle.is_active(), "toggle must start active when expanded");
        ensure!(list.is_visible(), "track list must start visible");
        ensure!(
            toggle
                .tooltip_text()
                .is_some_and(|tip| tip.contains("Collapse")),
            "toggle must offer collapse when expanded"
        );
        let open = section_open_button(&section)?;
        ensure!(
            open.tooltip_text().is_some_and(|tip| tip.contains("Open")),
            "open button must keep the album navigation affordance"
        );
        Ok(())
    }

    #[test]
    fn collapsed_section_hides_tracks_and_marks_toggle() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let (section, list, _) =
            build_album_section(&state, &mock_album(), &FormatInfo::default(), vec![], false);
        let toggle = section_toggle(&section)?;
        ensure!(
            !toggle.is_active(),
            "toggle must start inactive when collapsed"
        );
        ensure!(!list.is_visible(), "track list must start hidden");
        ensure!(
            toggle
                .tooltip_text()
                .is_some_and(|tip| tip.contains("Expand")),
            "toggle must offer expand when collapsed"
        );
        Ok(())
    }

    #[test]
    fn toggling_section_flips_only_its_own_list() -> Result<()> {
        let state = Arc::new(isolated_app_state()?);
        state.storage.set_artist_albums_collapsed_memory(false);
        let (first_section, first_list, _) =
            build_album_section(&state, &mock_album(), &FormatInfo::default(), vec![], true);
        let (_, second_list, _) =
            build_album_section(&state, &mock_album(), &FormatInfo::default(), vec![], true);
        let toggle = section_toggle(&first_section)?;
        toggle.set_active(false);
        ensure!(
            !first_list.is_visible(),
            "collapsing must hide the track list"
        );
        ensure!(
            second_list.is_visible(),
            "collapsing must leave sibling sections untouched"
        );
        ensure!(
            !state.storage.get_artist_albums_collapsed(),
            "individual toggles must not rewrite the persisted master default"
        );
        toggle.set_active(true);
        ensure!(
            first_list.is_visible(),
            "expanding must reveal the track list"
        );
        ensure!(
            !state.storage.get_artist_albums_collapsed(),
            "individual toggles must leave the master default alone"
        );
        Ok(())
    }

    #[test]
    fn track_row_factory_still_builds_for_collapsed_section() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let (section, list, toggle) = build_album_section(
            &state,
            &mock_album(),
            &FormatInfo::default(),
            vec![Track::test_fixture()],
            false,
        );
        ensure!(section.is_visible(), "collapsed section stays mounted");
        ensure!(!toggle.is_active(), "collapsed toggle stays inactive");
        ensure!(!list.is_visible(), "collapsed list stays hidden");
        Ok(())
    }
}
