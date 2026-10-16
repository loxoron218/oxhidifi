//! Detail-page push navigation for album and artist drill-down.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        NavigationPage, NavigationView,
        gtk::{Widget, accessible::Property::Label},
        prelude::AccessibleExtManual,
    },
    tracing::info,
};

use crate::{
    app::runtime::{
        AppState,
        NavigationEvent::{self, AlbumDetail, ArtistDetail, Back},
    },
    ui::{
        detail::{album_page::build_album_detail, artist_page::build_artist_detail},
        nav_tags::{detail_album_tag, detail_artist_tag},
    },
};

/// Push a styled detail page with a unique tag and accessible label.
///
/// # Arguments
///
/// * `nav_view` - Navigation view receiving the page.
/// * `title` - Visible page title.
/// * `tag` - Unique page tag for push/pop matching.
/// * `detail` - Detail content widget.
/// * `label` - Accessible label shared with the tooltip.
fn push_detail_page(
    nav_view: &NavigationView,
    title: &str,
    tag: &str,
    detail: &Widget,
    label: &str,
) {
    let page = NavigationPage::builder()
        .child(detail)
        .title(title)
        .tag(tag)
        .css_classes(["detail-page"])
        .tooltip_text(label)
        .build();
    page.update_property(&[Label(label)]);
    nav_view.push(&page);
}

/// Handle navigation events (album/artist detail, back navigation).
///
/// Page stacks are managed via [`NavigationView`]: detail pages are pushed
/// as [`NavigationPage`]s with unique per-entity tags
/// (`detail-album-{id}`, `detail-artist-{id}`), and back navigation pops a
/// single page so drill-down unwinds one level at a time (album → artist →
/// library). Unique tags allow drill-down (artist → album) without
/// triggering `Duplicate page tag in AdwNavigationView: detail`. This
/// satisfies Constitution III which mandates `AdwNavigationView` for
/// push/pop stacks.
pub fn handle_navigation_event(
    nav_state: &Arc<AppState>,
    nav_view: &NavigationView,
    nav_tx: &Sender<NavigationEvent>,
    event: NavigationEvent,
) {
    match event {
        AlbumDetail(album_id) => {
            info!(album_id, "Navigating to album detail",);
            let tag = detail_album_tag(album_id);
            if nav_view
                .visible_page_tag()
                .is_some_and(|visible| visible.as_str() == tag)
            {
                info!(album_id, "Album detail already visible",);
                return;
            }
            let detail = build_album_detail(nav_state, album_id, nav_tx);
            push_detail_page(nav_view, "Album", &tag, &detail, "Album detail");
        }
        ArtistDetail(artist_id) => {
            info!(artist_id, "Navigating to artist detail",);
            let tag = detail_artist_tag(artist_id);
            if nav_view
                .visible_page_tag()
                .is_some_and(|visible| visible.as_str() == tag)
            {
                info!(artist_id, "Artist detail already visible",);
                return;
            }
            let detail = build_artist_detail(nav_state, artist_id, nav_tx);
            push_detail_page(nav_view, "Artist", &tag, &detail, "Artist detail");
        }
        Back => {
            let previous = nav_view.visible_page_tag().map(|tag| tag.to_string());
            info!(previous_tag = previous, "Navigating back one level",);
            _ = nav_view.pop();
        }
    }
}
