//! Audio playback preferences group: volume, gapless, and artist order rows.

use std::sync::Arc;

use {
    libadwaita::{
        ComboRow, PreferencesGroup, PreferencesPage, SpinRow, SwitchRow,
        glib::spawn_future_local,
        gtk::{Adjustment, StringList, accessible::Property::Label},
        prelude::{
            AccessibleExtManual, ComboRowExt, ObjectExt, PreferencesGroupExt, PreferencesPageExt,
        },
    },
    tracing::{error, info, warn},
};

use crate::{
    app::runtime::AppState, playback::transport::PlaybackTransport,
    storage::sort_rules::ArtistPlayOrder,
};

/// Persist gapless playback setting, logging on failure.
async fn save_gapless_setting(state: Arc<AppState>, enabled: bool) {
    if let Err(e) = state.storage.set_gapless_enabled(enabled).await {
        error!(error = %e, "Failed to save gapless setting");
    }
}

/// Map a combo index to its artist playback order, if valid.
///
/// # Arguments
///
/// * `idx` - Selected combo row index.
///
/// # Returns
///
/// * `Option<ArtistPlayOrder>` - Order for valid indices, `None` otherwise.
#[must_use]
pub fn artist_play_order_from_index(idx: u32) -> Option<ArtistPlayOrder> {
    ArtistPlayOrder::from_discriminator(u8::try_from(idx).unwrap_or(u8::MAX))
}

/// Map an artist playback order to its combo index.
///
/// # Arguments
///
/// * `order` - Current artist playback order.
///
/// # Returns
///
/// * `u32` - Combo row index for the order.
#[must_use]
pub fn artist_play_order_to_index(order: ArtistPlayOrder) -> u32 {
    u32::from(order.discriminator())
}

/// Persist the artist playback order, logging on failure.
async fn save_artist_play_order(state: Arc<AppState>, order: ArtistPlayOrder) {
    if let Err(e) = state.storage.set_artist_play_order(order).await {
        error!(error = %e, "Failed to save artist playback order");
    }
}

/// Build the Playback preferences group.
///
/// Rows keep their default widget classes (`spin`, `activatable`, `combo`)
/// alongside custom ones: dropping them removes hover/press feedback or
/// leaves the selected value with a black background.
///
/// # Arguments
///
/// * `page` - Preferences page owning the new group.
/// * `state` - Application state for playback settings.
///
/// # Returns
///
/// * `()` - Nothing; the group is added to `page`.
pub fn build_playback_group(page: &PreferencesPage, state: &Arc<AppState>) {
    let playback_group = PreferencesGroup::builder()
        .title("Playback")
        .description("Playback behavior")
        .css_classes(["playback-group"])
        .tooltip_text("Playback behavior settings")
        .build();
    playback_group.update_property(&[Label("Playback behavior settings")]);

    let initial_vol = (state.storage.get_settings_volume() * 100.0).round();
    let adjustment = Adjustment::new(initial_vol, 0.0, 100.0, 1.0, 10.0, 0.0);
    let volume_row = SpinRow::builder()
        .title("Volume")
        .subtitle("Playback volume level (0\u{2013}100)")
        .adjustment(&adjustment)
        .digits(0)
        .css_classes(["spin", "volume-row"])
        .tooltip_text("Adjust playback volume")
        .build();

    let state_vol = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(
            volume_row.connect_notify_local(Some("value"), move |row, _| {
                let vol = row.value() / 100.0;
                if let Err(e) = state_vol.playback.set_volume(vol) {
                    warn!(error = %e, "Failed to set volume from preferences");
                }
            }),
        );

    playback_group.add(&volume_row);

    let gapless_row = SwitchRow::builder()
        .title("Gapless Playback")
        .subtitle("Seamless transitions between tracks")
        .active(state.storage.get_gapless_enabled())
        .css_classes(["activatable", "gapless-row"])
        .tooltip_text("Toggle gapless playback")
        .build();

    let state_gapless = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(gapless_row.connect_active_notify(move |row| {
            let enabled = row.is_active();
            if let Err(e) = state_gapless.playback.set_gapless_enabled(enabled) {
                warn!(error = %e, "Failed to toggle gapless playback");
            }
            state_gapless
                .handles
                .lock()
                .retain_task(spawn_future_local(save_gapless_setting(
                    Arc::clone(&state_gapless),
                    enabled,
                )));
        }));

    playback_group.add(&gapless_row);

    let order_labels: Vec<String> = ArtistPlayOrder::options()
        .iter()
        .map(ToString::to_string)
        .collect();
    let order_refs: Vec<&str> = order_labels.iter().map(String::as_str).collect();
    let order_model = StringList::new(&order_refs);
    let order_row = ComboRow::builder()
        .title("Artist Playback Order")
        .subtitle("Album order when playing all albums by an artist")
        .model(&order_model)
        .selected(artist_play_order_to_index(
            state.storage.get_artist_play_order(),
        ))
        .css_classes(["combo", "artist-order-row"])
        .tooltip_text("Select artist playback order")
        .build();
    let state_order = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(order_row.connect_selected_notify(move |row| {
            let Some(order) = artist_play_order_from_index(row.selected()) else {
                warn!(selected = row.selected(), "Unknown artist playback order");
                return;
            };
            info!(order = ?order, "Artist playback order changed");
            state_order
                .handles
                .lock()
                .retain_task(spawn_future_local(save_artist_play_order(
                    Arc::clone(&state_order),
                    order,
                )));
        }));
    playback_group.add(&order_row);
    page.add(&playback_group);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            ComboRow, PreferencesGroup, PreferencesPage, SpinRow, SwitchRow,
            glib::object::Cast,
            gtk::{self, Widget, accessible::Property::Label, test},
            prelude::{AccessibleExtManual, WidgetExt},
        },
    };

    use crate::{
        app::mocks::{find_descendants, isolated_app_state},
        ui::preferences::audio_playback::build_playback_group,
    };

    fn test_page() -> PreferencesPage {
        let page = PreferencesPage::builder()
            .css_classes(["test-page"])
            .tooltip_text("Test preferences page")
            .build();
        page.update_property(&[Label("Test preferences page")]);
        page
    }

    fn playback_root() -> Result<Widget> {
        let state = Arc::new(isolated_app_state()?);
        let page = test_page();
        build_playback_group(&page, &state);
        Ok(page.upcast())
    }

    #[test]
    fn playback_group_stays_in_focus_chain() -> Result<()> {
        let root = playback_root()?;
        let groups = find_descendants::<PreferencesGroup>(&root);
        ensure!(groups.len() == 1, "page must own the playback group");
        for group in &groups {
            ensure!(
                group.can_focus(),
                "playback group must stay focusable so preferences rows keep keyboard focus for \
                 Escape-to-close"
            );
        }
        Ok(())
    }

    #[test]
    fn playback_rows_keep_type_classes() -> Result<()> {
        let root = playback_root()?;
        let spins = find_descendants::<SpinRow>(&root);
        ensure!(
            spins.len() == 1,
            "playback group must own the volume spin row"
        );
        for spin in &spins {
            ensure!(
                spin.css_classes().contains(&"spin".into()),
                "volume row must keep the default `spin` class"
            );
        }
        let combos = find_descendants::<ComboRow>(&root);
        ensure!(
            combos.len() == 1,
            "playback group must own the artist order combo"
        );
        for combo in &combos {
            ensure!(
                combo.css_classes().contains(&"combo".into()),
                "artist order row must keep the default `combo` class"
            );
        }
        let switches = find_descendants::<SwitchRow>(&root);
        ensure!(
            switches.len() == 1,
            "playback group must own the gapless switch row"
        );
        for switch in &switches {
            ensure!(
                switch.css_classes().contains(&"activatable".into()),
                "gapless row must keep the default `activatable` class"
            );
        }
        Ok(())
    }
}
