//! Detail page frame: wrapper, header bar, scroll content, and back navigation.

use {
    async_channel::Sender,
    libadwaita::{
        gdk::Key,
        glib::Propagation::{Proceed, Stop},
        gtk::{
            Align::Start,
            Box as GtkBox, Button, EventControllerKey, Label,
            Orientation::{Horizontal, Vertical},
            ScrolledWindow,
            accessible::Property::Label as PropertyLabel,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, WidgetExt},
    },
    tracing::error,
};

use crate::{
    app::runtime::NavigationEvent::{self, Back},
    ui::signal_handlers::UiHandles,
};

/// Build the wrapper box with back navigation and header bar for a detail page.
#[must_use]
pub fn build_detail_wrapper(nav_tx: &Sender<NavigationEvent>, title: &str) -> GtkBox {
    let wrapper = GtkBox::builder()
        .orientation(Vertical)
        .can_focus(true)
        .build();
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

    let mut handles = UiHandles::default();
    let ntx = nav_tx.clone();
    handles.retain_signal(back_button.connect_clicked(move |_| {
        try_send_back(&ntx);
    }));

    let nav_back = nav_tx;
    let key_controller = EventControllerKey::new();
    handles.retain_signal(key_controller.connect_key_pressed(move |_, key, _, _| {
        if key == Key::Escape {
            try_send_back(&nav_back);
            Stop
        } else {
            Proceed
        }
    }));
    widget.add_controller(key_controller);

    back_button
}

/// Build a scrollable content area with standard margins and spacing.
#[must_use]
pub fn build_scroll_content() -> (ScrolledWindow, GtkBox) {
    let scroll = ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .build();

    let content = GtkBox::builder()
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
pub fn build_detail_header(back_button: &Button, title: &str) -> GtkBox {
    let header = GtkBox::builder()
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
    };

    use crate::{
        app::runtime::NavigationEvent::{self, Back},
        ui::detail::page::try_send_back,
    };

    #[test]
    fn try_send_back_forwards_back_event() -> Result<()> {
        let (tx, rx) = unbounded::<NavigationEvent>();
        try_send_back(&tx);
        ensure!(matches!(rx.try_recv(), Ok(Back)));
        Ok(())
    }
}
