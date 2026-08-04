//! Common UI widgets and helpers for detail pages.

use std::sync::Arc;

use {
    async_channel::Sender,
    libadwaita::{
        gdk::Key,
        glib::{
            ControlFlow::{self, Break, Continue},
            Propagation::{Proceed, Stop},
        },
        gtk::{
            Align::Start,
            Box, Button, EventControllerKey, Label, ListBox,
            Orientation::{Horizontal, Vertical},
            ScrolledWindow,
            accessible::Property::Label as PropertyLabel,
            prelude::{AccessibleExtManual, BoxExt, WidgetExt},
        },
        prelude::ButtonExt,
    },
    tracing::error,
};

use crate::{
    app::{
        AppState,
        NavigationEvent::{self, Back},
    },
    storage::records::Track,
    ui::detail::track_row::build_track_row,
};

/// Number of tracks to add per batch in the detail page track list.
const BATCH_SIZE: usize = 10;

/// Append up to `BATCH_SIZE` track rows from `remaining` to the list.
/// Call from an idle callback; returns `Continue` if more remain, `Break` when done.
pub fn fill_track_list_batch(
    remaining: &mut Vec<(Track, usize)>,
    track_list: &ListBox,
    state: &Arc<AppState>,
) -> ControlFlow {
    for _ in 0..BATCH_SIZE {
        let Some((track, display_num)) = remaining.pop() else {
            break;
        };
        let row = build_track_row(state, &track, display_num);
        track_list.append(&row);
    }
    if remaining.is_empty() {
        Break
    } else {
        Continue
    }
}

/// Build the wrapper box with back navigation and header bar for a detail page.
#[must_use]
pub fn build_detail_wrapper(nav_tx: &Sender<NavigationEvent>, title: &str) -> Box {
    let wrapper = Box::builder().orientation(Vertical).can_focus(true).build();
    wrapper.update_property(&[PropertyLabel(&format!("{title} detail page"))]);
    let back_button = setup_back_navigation(&wrapper, nav_tx.clone());
    let header_bar = build_detail_header(&back_button, title);
    wrapper.append(&header_bar);
    wrapper
}

/// Try to send a Back navigation event, logging on failure.
pub fn try_send_back(tx: &Sender<NavigationEvent>) {
    if let Err(e) = tx.try_send(Back) {
        error!(error = %e, "Failed to send Back navigation");
    }
}

/// Set up back button and Escape key navigation.
pub fn setup_back_navigation(widget: &impl WidgetExt, nav_tx: Sender<NavigationEvent>) -> Button {
    let back_button = Button::builder()
        .icon_name("go-previous-symbolic")
        .tooltip_text("Back to library")
        .css_classes(["flat"])
        .can_focus(true)
        .build();
    back_button.update_property(&[PropertyLabel("Back to library")]);

    let ntx = nav_tx.clone();
    back_button.connect_clicked(move |_| {
        try_send_back(&ntx);
    });

    let nav_back = nav_tx;
    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if key == Key::Escape {
            try_send_back(&nav_back);
            Stop
        } else {
            Proceed
        }
    });
    widget.add_controller(key_controller);

    back_button
}

/// Build a scrollable content area with standard margins and spacing.
#[must_use]
pub fn build_scroll_content() -> (ScrolledWindow, Box) {
    let scroll = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .build();

    let content = Box::builder()
        .orientation(Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(18)
        .margin_end(18)
        .build();

    (scroll, content)
}

/// Build a header bar-like box with back button and title.
#[must_use]
pub fn build_detail_header(back_button: &Button, title: &str) -> Box {
    let header = Box::builder()
        .orientation(Horizontal)
        .spacing(6)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .css_classes(["toolbar"])
        .build();

    header.append(back_button);

    let title_label = Label::builder()
        .label(title)
        .css_classes(["title-4", "heading"])
        .hexpand(true)
        .halign(Start)
        .build();
    title_label.update_property(&[PropertyLabel(&format!("{title} detail page"))]);
    header.append(&title_label);

    header
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        async_channel::unbounded,
        libadwaita::gtk::{self, test},
    };

    use crate::{
        app::NavigationEvent::{self, Back},
        ui::detail::common::try_send_back,
    };

    #[test]
    fn try_send_back_forwards_back_event() -> Result<()> {
        let (tx, rx) = unbounded::<NavigationEvent>();
        try_send_back(&tx);
        ensure!(matches!(rx.try_recv(), Ok(Back)));
        Ok(())
    }
}
