//! Welcome tour dialog shown on the initial empty state.
//!
//! A first-run [`Dialog`] with a [`Carousel`] of high-fidelity feature
//! slides. It auto-presents once when no library is configured and stays
//! re-openable from the empty state afterwards.

/// Submodules split out to keep this file under the size standard.
pub mod carousel;
pub mod slides;

use std::sync::Arc;

use {
    libadwaita::{
        Carousel, Dialog, HeaderBar, ToolbarView,
        gdk::Key,
        glib::{
            Propagation::{Proceed, Stop},
            object::Cast,
            spawn_future_local,
        },
        gtk::{
            Align::Center,
            Box, Button, EventControllerKey, Label,
            Orientation::{Horizontal, Vertical},
            Window,
            accessible::Property::Label as PropertyLabel,
        },
        prelude::{AccessibleExtManual, AdwDialogExt, BoxExt, ButtonExt, WidgetExt},
    },
    tracing::warn,
};

use crate::{
    app::runtime::AppState,
    storage::Storage,
    ui::welcome::{
        carousel::{WelcomeCarousel, build_carousel, step_slide},
        slides::should_show_welcome,
    },
};

/// Mark the tour as seen so it only auto-presents once.
fn mark_welcome_seen(state: &Arc<AppState>) {
    state.storage.set_welcome_seen_memory();
    state.storage.save_settings();
}

/// Build the "Take the Feature Tour" button that re-opens the dialog.
///
/// # Arguments
///
/// * `state` - Application state for the click handler.
pub fn build_tour_button(state: &Arc<AppState>) -> Button {
    let button = Button::builder()
        .label("Take the Feature Tour")
        .use_underline(true)
        .css_classes(["flat", "text-button"])
        .tooltip_text("Reopen the welcome tour")
        .build();
    button.update_property(&[PropertyLabel("Take the Feature Tour")]);
    let state_clone = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(button.connect_clicked(move |btn| {
            let Some(root) = btn.root() else { return };
            let Ok(parent) = root.downcast::<Window>() else {
                return;
            };
            show_welcome_dialog(&state_clone, &parent);
        }));
    button
}

/// Build the always-visible Get Started button.
///
/// The button stays put on every slide so slide text and icons never shift
/// while touring.
fn build_get_started() -> Button {
    let started = Button::builder()
        .label("Get Started")
        .use_underline(true)
        .halign(Center)
        .css_classes(["suggested-action", "pill", "text-button"])
        .tooltip_text("Close the tour and open your empty library")
        .build();
    started.update_property(&[PropertyLabel("Get Started")]);
    started
}

/// Build arrow-key navigation for the tour slides.
///
/// # Arguments
///
/// * `carousel` - Slide container to step.
/// * `slides` - Slide pages for clamping.
/// * `state` - Application state retaining the key-press connection.
fn build_slide_keys(
    carousel: &Carousel,
    slides: Vec<Box>,
    state: &Arc<AppState>,
) -> EventControllerKey {
    let key_carousel = carousel.clone();
    let key_controller = EventControllerKey::new();
    state
        .handles
        .lock()
        .retain_signal(key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == Key::Left {
                step_slide(&key_carousel, &slides, -1);
                Stop
            } else if key == Key::Right {
                step_slide(&key_carousel, &slides, 1);
                Stop
            } else {
                Proceed
            }
        }));
    key_controller
}

/// Build the welcome dialog.
///
/// Closing the dialog (Get Started, Escape, or window close) marks the tour
/// as seen via [`mark_welcome_seen`].
pub fn build_welcome_dialog(state: &Arc<AppState>) -> Dialog {
    let dialog = Dialog::builder()
        .css_classes(["welcome-dialog"])
        .tooltip_text("Welcome tour")
        .build();

    let toolbar = ToolbarView::builder()
        .css_classes(["welcome-toolbar"])
        .tooltip_text("Welcome tour")
        .build();
    toolbar.update_property(&[PropertyLabel("Welcome tour")]);
    let header = HeaderBar::builder()
        .show_title(true)
        .css_classes(["welcome-header"])
        .tooltip_text("Welcome header")
        .build();
    header.update_property(&[PropertyLabel("Welcome header")]);
    let title = Label::builder()
        .label("Welcome to Oxhidifi")
        .css_classes(["title-4"])
        .tooltip_text("Welcome to Oxhidifi")
        .build();
    title.update_property(&[PropertyLabel("Welcome to Oxhidifi")]);
    header.set_title_widget(Some(&title));
    toolbar.add_top_bar(&header);

    let page = Box::builder()
        .orientation(Vertical)
        .spacing(24)
        .margin_start(24)
        .margin_end(24)
        .margin_bottom(24)
        .vexpand(true)
        .hexpand(true)
        .css_classes(["welcome-page"])
        .tooltip_text("Welcome tour content")
        .build();
    page.update_property(&[PropertyLabel("Welcome tour content")]);

    let WelcomeCarousel {
        carousel,
        slides,
        prev,
        next,
        dots,
    } = build_carousel();
    page.append(&dots);

    let middle = Box::builder()
        .orientation(Horizontal)
        .spacing(18)
        .vexpand(true)
        .hexpand(true)
        .css_classes(["welcome-middle"])
        .tooltip_text("Welcome tour slides")
        .build();
    middle.update_property(&[PropertyLabel("Welcome tour slides")]);
    middle.append(&prev);
    middle.append(&carousel);
    middle.append(&next);
    page.append(&middle);

    let started = build_get_started();
    page.append(&started);
    page.add_controller(build_slide_keys(&carousel, slides, state));

    let dialog_started = dialog.clone();
    state
        .handles
        .lock()
        .retain_signal(started.connect_clicked(move |_| {
            if !dialog_started.close() {
                warn!(
                    component = "welcome",
                    "Get Started did not close the dialog"
                );
            }
        }));

    let total = carousel.n_pages();
    state
        .handles
        .lock()
        .retain_signal(carousel.connect_page_changed(move |_, index| {
            let at_first = index == 0;
            let at_last = index.saturating_add(1) >= total;
            prev.set_sensitive(!at_first);
            prev.set_opacity(if at_first { 0.0 } else { 1.0 });
            next.set_sensitive(!at_last);
            next.set_opacity(if at_last { 0.0 } else { 1.0 });
        }));

    let seen_state = Arc::clone(state);
    state
        .handles
        .lock()
        .retain_signal(dialog.connect_closed(move |_| mark_welcome_seen(&seen_state)));

    toolbar.set_content(Some(&page));
    dialog.set_child(Some(&toolbar));
    dialog
}

/// Build and present the welcome dialog unconditionally.
pub fn show_welcome_dialog(state: &Arc<AppState>, parent: &Window) {
    build_welcome_dialog(state).present(Some(parent));
}

/// Present the welcome dialog once when no library is configured.
///
/// Fast-paths on the in-memory flag; the directory check runs async so window
/// construction never blocks.
pub fn show_welcome_if_first_run(state: &Arc<AppState>, parent: &Window) {
    if state.storage.get_welcome_seen() {
        return;
    }
    let task_state = Arc::clone(state);
    let parent = parent.clone();
    state
        .handles
        .lock()
        .retain_task(spawn_future_local(async move {
            if task_state.storage.get_welcome_seen() {
                return;
            }
            let dirs = task_state
                .storage
                .list_library_directories()
                .await
                .unwrap_or_default();
            if should_show_welcome(dirs.is_empty(), task_state.storage.get_welcome_seen()) {
                show_welcome_dialog(&task_state, &parent);
            }
        }));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, test},
            prelude::{AdwDialogExt, WidgetExt},
        },
    };

    use crate::{
        app::{preview_state::pump_in_test_runtime, runtime::AppState},
        ui::welcome::{build_welcome_dialog, carousel::build_carousel, slides::WELCOME_SLIDES},
    };

    #[test]
    fn carousel_holds_one_page_per_slide() -> Result<()> {
        let tour = build_carousel();
        ensure!(
            tour.carousel.n_pages() == u32::try_from(WELCOME_SLIDES.len()).unwrap_or(0),
            "carousel must hold one page per slide"
        );
        ensure!(
            tour.dots.carousel().is_some(),
            "progress dots must track the carousel"
        );
        ensure!(
            tour.prev.opacity() < f64::EPSILON && !tour.prev.is_sensitive(),
            "back arrow must fade out on the first slide"
        );
        ensure!(
            tour.next.opacity() > 1.0 - f64::EPSILON && tour.next.is_sensitive(),
            "forward arrow must accept mouse input"
        );
        Ok(())
    }

    #[test]
    fn dialog_builds_with_expected_size() -> Result<()> {
        pump_in_test_runtime(|| {
            let state = Arc::new(AppState::mock()?);
            let dialog = build_welcome_dialog(&state);
            ensure!(
                dialog.child().is_some(),
                "welcome dialog must hold tour content"
            );
            Ok(())
        })?
    }
}
