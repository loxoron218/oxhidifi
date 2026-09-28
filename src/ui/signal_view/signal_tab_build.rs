//! Signal tab page builder.
//!
//! Constructs the live vertical stage chain widgets owned by
//! [`SignalTab`](crate::ui::signal_view::signal_tab::SignalTab).

use libadwaita::{
    Banner, HeaderBar, StatusPage, ToolbarView, WindowTitle,
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
    signal_header::build_signal_header, signal_menu::build_signal_menu, signal_tab::SignalTab,
};

/// Build the Signal tab page.
///
/// # Returns
///
/// * `SignalTab` - Handles owning the tab root widget.
#[must_use]
pub fn build_signal_page() -> SignalTab {
    let root = ToolbarView::new();
    let header_bar = HeaderBar::new();
    header_bar.set_title_widget(Some(&WindowTitle::new(
        "Signal Path",
        "Live audio path from source to output",
    )));
    let menu = build_signal_menu();
    header_bar.pack_end(menu.menu_button());
    root.add_top_bar(&header_bar);

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
    let header = build_signal_header();
    body.append(header.widget());
    body.append(&banner);
    body.append(&content);
    let footer = build_signal_footer();
    body.append(footer.widget());
    root.set_content(Some(&body));

    SignalTab::new(root.upcast(), list, banner, content, header, footer, menu)
}
