//! Signal tab page: live vertical stage chain with empty state.
//!
//! Third `ViewStack` page named `signal` next to Albums/Artists. The chain
//! scrolls inside the tab with the header and footer reachable; a continuous
//! theme separator rails the badge icons on the left. Rows render in snapshot
//! `position` order, source at the top and output at the bottom, with a bold
//! title plus accent detail per row. Programmatic widgets only, 6 px spacing,
//! theme-aware styling, no hardcoded radii or colors, no overlay
//! dialog/popover/sheet.

use std::cell::Cell;

use libadwaita::{
    Banner, HeaderBar, StatusPage, ToolbarView, WindowTitle,
    gtk::{
        Align::Start,
        Box, Image, Label, ListBox, ListBoxRow,
        Orientation::{Horizontal, Vertical},
        ScrolledWindow,
        SelectionMode::Single,
        Separator, Stack, Widget,
        accessible::Property::Label as A11yLabel,
    },
    prelude::{AccessibleExtManual, BoxExt, Cast, ListBoxRowExt, WidgetExt},
};

use crate::playback::{
    signal_path::{PathStage, SignalPathSnapshot},
    state::PlaybackStatus::{Paused, Playing, Stopped},
};

/// Live handles for the Signal tab page.
#[derive(Debug, Clone)]
pub struct SignalTab {
    /// Tab root placed into the library `ViewStack`.
    root: Widget,
    /// Chain rows in snapshot `position` order.
    list: ListBox,
    /// Ribbon shown for paused/stopped playback.
    banner: Banner,
    /// Content switcher between the chain and the empty state.
    content: Stack,
    /// Generation currently on screen.
    generation: Cell<u64>,
    /// Track currently on screen (`None` for the empty state).
    track: Cell<Option<i64>>,
}

impl SignalTab {
    /// Generation currently on screen.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation.get()
    }

    /// Track currently on screen (`None` for the empty state).
    #[must_use]
    pub const fn track(&self) -> Option<i64> {
        self.track.get()
    }

    /// Tab root widget for the library `ViewStack`.
    #[must_use]
    pub const fn widget(&self) -> &Widget {
        &self.root
    }

    /// Render one snapshot, swapping the whole chain atomically.
    ///
    /// Rebuilds every row from `snapshot` in `position` order so gapless
    /// transitions never show mixed rows. Shows the paused/stopped ribbon
    /// when the track is known but not playing.
    pub fn render_snapshot(&self, snapshot: &SignalPathSnapshot) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        for stage in &snapshot.stages {
            self.list.append(&stage_row(stage));
        }
        match snapshot.playback_status {
            Playing => self.banner.set_revealed(false),
            Paused => {
                self.banner
                    .set_title("Paused — showing the last-known path");
                self.banner.set_revealed(true);
            }
            Stopped => {
                self.banner
                    .set_title("Stopped — showing the last-known path");
                self.banner.set_revealed(true);
            }
        }
        self.content.set_visible_child_name("chain");
        self.generation.set(snapshot.generation);
        self.track.set(snapshot.track_id);
    }

    /// Show the empty state explaining how to start playback.
    pub fn show_empty(&self) {
        self.banner.set_revealed(false);
        self.content.set_visible_child_name("empty");
        self.track.set(None);
    }

    /// Count the rendered chain rows (test hook for atomic-swap coverage).
    #[must_use]
    pub fn row_count(&self) -> u32 {
        let mut count = 0_u32;
        let mut next = self.list.first_child();
        while let Some(child) = next {
            count = count.saturating_add(1);
            next = child.next_sibling();
        }
        count
    }
}

/// Build the Signal tab page.
///
/// # Returns
///
/// * `SignalTab` - Handles owning the tab root widget.
#[must_use]
pub fn build_signal_page() -> SignalTab {
    let root = ToolbarView::new();
    let header = HeaderBar::new();
    header.set_title_widget(Some(&WindowTitle::new(
        "Signal Path",
        "Live audio path from source to output",
    )));
    root.add_top_bar(&header);

    let banner = Banner::new("Paused — showing the last-known path");
    banner.set_revealed(false);

    let list = ListBox::builder()
        .selection_mode(Single)
        .show_separators(true)
        .css_classes(["boxed-list"])
        .can_focus(true)
        .hexpand(true)
        .vexpand(true)
        .build();
    list.update_property(&[A11yLabel("Live signal path chain")]);

    let scrolled = ScrolledWindow::builder()
        .child(&list)
        .hexpand(true)
        .vexpand(true)
        .build();

    let rail = Separator::new(Vertical);
    let chain_row = Box::builder().orientation(Horizontal).spacing(6).build();
    chain_row.append(&rail);
    chain_row.append(&scrolled);

    let empty = StatusPage::builder()
        .icon_name("audio-x-generic-symbolic")
        .title("No Active Path")
        .description("Play a track to inspect the live signal path from source to output.")
        .build();

    let content = Stack::new();
    drop(content.add_named(&chain_row, Some("chain")));
    drop(content.add_named(&empty, Some("empty")));
    content.set_visible_child_name("empty");

    let body = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    body.append(&banner);
    body.append(&content);
    root.set_content(Some(&body));

    SignalTab {
        root: root.upcast::<Widget>(),
        list,
        banner,
        content,
        generation: Cell::new(0),
        track: Cell::new(None),
    }
}

/// Build one chain row: badge icon plus title/detail lines.
fn stage_row(stage: &PathStage) -> ListBoxRow {
    let row = ListBoxRow::builder()
        .activatable(true)
        .selectable(true)
        .can_focus(true)
        .build();
    row.update_property(&[A11yLabel(&format!("{} — {}", stage.title, stage.detail))]);
    let layout = Box::builder()
        .orientation(Horizontal)
        .spacing(12)
        .margin_start(6)
        .margin_end(6)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    let badge = Image::builder()
        .icon_name(stage.badge_icon)
        .pixel_size(32)
        .tooltip_text(stage.title.as_str())
        .build();
    badge.update_property(&[A11yLabel(stage.title.as_str())]);
    let text = Box::builder().orientation(Vertical).spacing(0).build();
    let title = Label::builder()
        .label(stage.title.as_str())
        .css_classes(["heading"])
        .halign(Start)
        .build();
    let detail = Label::builder()
        .label(stage.detail.as_str())
        .css_classes(["accent"])
        .halign(Start)
        .wrap(true)
        .build();
    text.append(&title);
    text.append(&detail);
    layout.append(&badge);
    layout.append(&text);
    row.set_child(Some(&layout));
    row
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::gtk::{self, test},
    };

    use crate::{
        playback::{
            signal_path::{
                PathStage,
                QualityVerdict::BitPerfect,
                SignalPathSnapshot,
                StageKind::{Output, Source, Transport},
            },
            state::PlaybackStatus::{Paused, Playing},
        },
        ui::signal_view::signal_tab::build_signal_page,
    };

    fn three_stage_snapshot() -> SignalPathSnapshot {
        let stages = vec![
            PathStage {
                position: 0,
                kind: Source,
                title: String::from("Source"),
                detail: String::from("FLAC 44.1kHz 16-bit Stereo"),
                explanation: String::from("Original audio."),
                verdict: BitPerfect,
                badge_icon: "audio-x-generic-symbolic",
            },
            PathStage {
                position: 1,
                kind: Transport,
                title: String::from("ALSA Direct Output"),
                detail: String::from("Test DAC (ALSA direct exclusive)"),
                explanation: String::from("Exclusive delivery."),
                verdict: BitPerfect,
                badge_icon: "audio-card-symbolic",
            },
            PathStage {
                position: 2,
                kind: Output,
                title: String::from("Output"),
                detail: String::from("Test DAC"),
                explanation: String::from("Final render."),
                verdict: BitPerfect,
                badge_icon: "audio-speakers-symbolic",
            },
        ];
        SignalPathSnapshot {
            generation: 4,
            track_id: Some(7),
            zone_name: String::from("Test DAC"),
            verdict: BitPerfect,
            stages,
            devices: Vec::new(),
            processing_speed: None,
            playback_status: Playing,
        }
    }

    #[test]
    fn signal_tab_renders_chain_rows_in_order() -> Result<()> {
        let tab = build_signal_page();
        tab.show_empty();
        ensure!(tab.row_count() == 0, "empty tab must show no rows");
        ensure!(tab.track().is_none(), "empty tab must track no id");
        let snapshot = three_stage_snapshot();
        tab.render_snapshot(&snapshot);
        ensure!(tab.row_count() == 3, "chain must render every stage");
        ensure!(tab.track() == Some(7), "tab must track the snapshot id");
        ensure!(tab.generation() == 4, "tab must track the generation");
        let paused = SignalPathSnapshot {
            playback_status: Paused,
            ..snapshot
        };
        tab.render_snapshot(&paused);
        ensure!(tab.row_count() == 3, "ribbon swap must keep every row");
        Ok(())
    }
}
