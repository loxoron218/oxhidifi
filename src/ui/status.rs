//! Status bar with scanning progress indicator.
//!
//! Displays scanning progress and status information at the bottom of the window.

use std::sync::Arc;

use {
    async_channel::Receiver,
    libadwaita::{
        glib::spawn_future_local,
        gtk::{
            Align::{End, Start},
            Box, Label,
            Orientation::Horizontal,
            ProgressBar,
            accessible::Property::Label as PropertyLabel,
        },
        prelude::{AccessibleExtManual, BoxExt, WidgetExt},
    },
};

use crate::{
    app::AppState,
    library::scanner::{
        ScanEvent,
        ScanEvent::{ScanCompleted, ScanError, ScanProgress, ScanStarted},
    },
};

/// Status bar showing scanning progress and library information.
pub struct StatusBar {
    /// The root widget containing all status bar elements.
    root: Box,
    /// Label showing current status text.
    status_label: Label,
    /// Progress bar for scanning operations.
    progress_bar: ProgressBar,
}

impl StatusBar {
    /// Create a new status bar.
    ///
    /// Subscribes to scan events from `AppState` and updates the progress
    /// indicator accordingly.
    ///
    /// # Arguments
    ///
    /// * `state` - Application state containing the scan event channel
    #[must_use]
    pub fn new(state: &Arc<AppState>) -> Self {
        let root = Box::builder()
            .orientation(Horizontal)
            .spacing(12)
            .css_classes(["status-bar"])
            .build();

        let status_label = Label::builder()
            .label("Ready")
            .hexpand(true)
            .halign(Start)
            .css_classes(["dim-label", "caption"])
            .can_focus(true)
            .tooltip_text("Current application status")
            .build();
        status_label.update_property(&[PropertyLabel("Status: Ready")]);

        let progress_bar = ProgressBar::builder()
            .hexpand(false)
            .halign(End)
            .show_text(false)
            .build();
        progress_bar.update_property(&[PropertyLabel("Scanning progress")]);

        root.append(&status_label);
        root.append(&progress_bar);

        let status = Self {
            root,
            status_label,
            progress_bar,
        };

        status.subscribe_to_scan_events(state);

        status
    }

    /// Get a reference to the root widget.
    #[must_use]
    pub fn widget(&self) -> &Box {
        &self.root
    }

    /// Update the status text.
    pub fn set_status(&self, text: &str) {
        self.status_label.set_label(text);
    }

    /// Show scanning progress.
    ///
    /// # Arguments
    ///
    /// * `progress` - Progress value between 0.0 and 1.0
    /// * `text` - Optional text to display alongside the progress
    pub fn show_progress(&self, progress: f64, text: Option<&str>) {
        self.progress_bar.set_fraction(progress);
        self.progress_bar.set_visible(true);
        if let Some(t) = text {
            self.status_label.set_label(t);
        }
    }

    /// Hide the progress bar.
    pub fn hide_progress(&self) {
        self.progress_bar.set_visible(false);
        self.progress_bar.set_fraction(0.0);
    }

    /// Reset to default state.
    pub fn reset(&self) {
        self.status_label.set_label("Ready");
        self.hide_progress();
    }

    /// Subscribe to the scan event channel and update the status bar.
    ///
    /// Uses `async_channel` which integrates with `GLib`'s main context via
    /// `spawn_future_local`, unlike `tokio::sync::broadcast` whose wakers
    /// don't wake the `GLib` main loop.
    fn subscribe_to_scan_events(&self, state: &Arc<AppState>) {
        let rx = state.scan_event_rx.clone();
        let status_label = self.status_label.clone();
        let progress_bar = self.progress_bar.clone();

        spawn_future_local(async move {
            Self::run_scan_event_loop(rx, &status_label, &progress_bar).await;
        });
    }

    /// Run the scan event loop, processing events until the channel closes.
    async fn run_scan_event_loop(
        rx: Receiver<ScanEvent>,
        status_label: &Label,
        progress_bar: &ProgressBar,
    ) {
        while let Ok(event) = rx.recv().await {
            Self::handle_scan_event(status_label, progress_bar, event);
        }
    }

    /// Apply a single scan event to the status bar widgets.
    fn handle_scan_event(status_label: &Label, progress_bar: &ProgressBar, event: ScanEvent) {
        match event {
            ScanStarted { directory } => {
                let name = directory.file_name().map_or_else(
                    || directory.display().to_string(),
                    |n| n.to_string_lossy().to_string(),
                );
                status_label.set_label(&format!("Scanning \u{201c}{name}\u{201d}..."));
                progress_bar.set_visible(true);
                progress_bar.set_fraction(0.0);
            }
            ScanProgress {
                files_found,
                files_processed,
                ..
            } => {
                let fraction = f64::from(files_processed) / f64::from(files_found.max(1));
                progress_bar.set_fraction(fraction);
                status_label.set_label(&format!(
                    "Scanning... {files_processed}/{files_found} files"
                ));
            }
            ScanCompleted {
                tracks_added,
                tracks_skipped,
                ..
            } => {
                status_label.set_label(&format!(
                    "Scan complete: {tracks_added} tracks added, {tracks_skipped} skipped"
                ));
                progress_bar.set_fraction(1.0);
                progress_bar.set_visible(false);
            }
            ScanError { error, .. } => {
                status_label.set_label(&format!("Scan error: {error}"));
                progress_bar.set_visible(false);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use {
        anyhow::{Result, ensure},
        libadwaita::prelude::WidgetExt,
    };

    use crate::{app::AppState, ui::status::StatusBar};

    #[test]
    #[ignore = "Requires GTK initialization (display server)"]
    fn status_bar_creates_with_root_widget() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let status_bar = StatusBar::new(&state);
        ensure!(status_bar.widget().first_child().is_some());
        Ok(())
    }

    #[test]
    #[ignore = "Requires GTK initialization (display server)"]
    fn status_bar_label_updates() -> Result<()> {
        let state = Arc::new(AppState::mock()?);
        let status_bar = StatusBar::new(&state);
        status_bar.set_status("Scanning...");
        ensure!(status_bar.status_label.label() == "Scanning...");
        Ok(())
    }
}
