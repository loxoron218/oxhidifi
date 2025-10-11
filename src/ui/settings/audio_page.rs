use libadwaita::PreferencesPage;

/// Creates and configures the Audio preferences page.
///
/// This function sets up the Audio page, which is currently empty but prepared for future use.
///
/// # Returns
///
/// A configured `PreferencesPage` for audio settings.
pub fn create_audio_page() -> PreferencesPage {
    // --- Audio Page (Currently empty, but kept for potential future use) ---
    PreferencesPage::builder()
        .title("Audio")
        .icon_name("audio-speakers-symbolic")
        .build()
}
