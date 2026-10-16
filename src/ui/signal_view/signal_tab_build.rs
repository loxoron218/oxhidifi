//! Signal tab page builder.
//!
//! Constructs the live vertical stage chain widgets owned by
//! [`SignalTab`](crate::ui::signal_view::signal_tab::SignalTab). The page is
//! plain tab content without its own `HeaderBar`: window controls and the
//! signal options menu live in the main window header bar.

use libadwaita::{
    Banner, StatusPage,
    gtk::{
        Box, ListBox,
        Orientation::{Horizontal, Vertical},
        ScrolledWindow,
        SelectionMode::Single,
        Separator, Stack,
        accessible::Property::Label,
    },
    prelude::{AccessibleExtManual, BoxExt, Cast},
};

use crate::ui::signal_view::{
    signal_chain::wire_explainer, signal_footer::build_signal_footer,
    signal_header::build_signal_header, signal_tab::SignalTab,
};

/// Build the Signal tab page.
///
/// # Returns
///
/// * `SignalTab` - Handles owning the tab root widget.
#[must_use]
pub fn build_signal_page() -> SignalTab {
    let banner = Banner::builder()
        .title("Paused — showing the last-known path")
        .revealed(false)
        .css_classes(["signal-paused-banner"])
        .tooltip_text("Playback paused notice")
        .build();
    banner.update_property(&[Label("Playback paused notice")]);

    let list = ListBox::builder()
        .selection_mode(Single)
        .show_separators(true)
        .css_classes(["boxed-list", "separators"])
        .tooltip_text("Live signal path chain")
        .hexpand(true)
        .vexpand(true)
        .build();
    list.update_property(&[Label("Live signal path chain")]);
    let explainer_wiring = wire_explainer(&list);
    debug_assert!(
        explainer_wiring.len() == 2,
        "explainer keeps its two wirings"
    );

    let scrolled = ScrolledWindow::builder()
        .child(&list)
        .hexpand(true)
        .vexpand(true)
        .css_classes(["signal-chain-scroll", "frame"])
        .tooltip_text("Signal chain scroll area")
        .build();

    let rail = Separator::builder()
        .orientation(Vertical)
        .css_classes(["signal-chain-rail"])
        .tooltip_text("Chain separator")
        .build();
    let chain_row = Box::builder()
        .orientation(Horizontal)
        .spacing(6)
        .css_classes(["signal-chain-row"])
        .tooltip_text("Signal chain")
        .build();
    chain_row.append(&rail);
    chain_row.append(&scrolled);

    let empty = StatusPage::builder()
        .icon_name("audio-x-generic-symbolic")
        .title("No Active Path")
        .description("Play a track to inspect the live signal path from source to output.")
        .css_classes(["signal-empty-page"])
        .tooltip_text("Empty signal path")
        .build();
    empty.update_property(&[Label("Empty signal path")]);

    let content = Stack::builder()
        .css_classes(["signal-content-stack"])
        .tooltip_text("Signal content")
        .build();
    drop(content.add_named(&chain_row, Some("chain")));
    drop(content.add_named(&empty, Some("empty")));
    content.set_visible_child_name("empty");

    let root = Box::builder()
        .orientation(Vertical)
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(6)
        .css_classes(["signal-tab-root"])
        .tooltip_text("Signal path tab")
        .build();
    let header = build_signal_header();
    root.append(header.widget());
    root.append(&banner);
    root.append(&content);
    let footer = build_signal_footer();
    root.append(footer.widget());

    SignalTab::new(root.upcast(), list, banner, content, header, footer)
}
