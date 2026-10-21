//! Welcome tour slide catalog and page builder.
//!
//! Holds the high-fidelity feature slides in presentation order plus the
//! pure auto-present decision, keeping [`crate::ui::welcome`] under the
//! file-size standard.

use libadwaita::{
    gtk::{
        Align::Center, Box, Image, Justification::Center as JustifyCenter, Label,
        Orientation::Vertical, accessible::Property::Label as PropertyLabel,
    },
    prelude::{AccessibleExtManual, BoxExt},
};

/// High-fidelity feature slides in presentation order.
pub const WELCOME_SLIDES: [WelcomeSlide; 4] = [
    WelcomeSlide {
        icon_name: "audio-headphones-symbolic",
        title: "Bit-perfect playback",
        body: "Music plays at its native sample rate and bit depth whenever your device supports \
               it — no hidden resampling or volume mangling.",
    },
    WelcomeSlide {
        icon_name: "media-playback-start-symbolic",
        title: "Gapless transitions",
        body: "Live albums and classical works flow without gaps or clicks, even across \
               sample-rate changes.",
    },
    WelcomeSlide {
        icon_name: "audio-x-generic-symbolic",
        title: "Transparent resampling, true high-res",
        body: "Resampling kicks in only when the device needs it (SNR > 120 dB), with support up \
               to 192 kHz / 24-bit across FLAC, MP3, AAC, Ogg, Opus, WAV, and AIFF.",
    },
    WelcomeSlide {
        icon_name: "folder-music-symbolic",
        title: "Know what you hear",
        body: "The signal-path inspector shows a Bit-Perfect, Processed, or Limited verdict for \
               every track. Choose Get Started below, then add a music folder to begin.",
    },
];

/// One welcome carousel slide.
#[derive(Debug, Clone, Copy)]
pub struct WelcomeSlide {
    /// Symbolic icon name.
    pub icon_name: &'static str,
    /// Slide title.
    pub title: &'static str,
    /// Slide body copy.
    pub body: &'static str,
}

/// Whether the welcome dialog should auto-present.
///
/// # Arguments
///
/// * `dirs_empty` - No library directories are configured.
/// * `seen` - The dialog has been shown before.
#[must_use]
pub const fn should_show_welcome(dirs_empty: bool, seen: bool) -> bool {
    dirs_empty && !seen
}

/// Build one carousel slide page.
#[must_use]
pub fn build_slide(slide: &WelcomeSlide) -> Box {
    let page = Box::builder()
        .orientation(Vertical)
        .spacing(18)
        .valign(Center)
        .halign(Center)
        .vexpand(true)
        .hexpand(true)
        .css_classes(["welcome-slide"])
        .tooltip_text(slide.title)
        .build();
    page.update_property(&[PropertyLabel(slide.title)]);

    let icon = Image::builder()
        .icon_name(slide.icon_name)
        .pixel_size(96)
        .css_classes(["icon-drop-shadow", "dim-label"])
        .tooltip_text(slide.title)
        .build();
    icon.update_property(&[PropertyLabel(slide.title)]);

    let title = Label::builder()
        .label(slide.title)
        .wrap(true)
        .justify(JustifyCenter)
        .css_classes(["title-2", "accent"])
        .tooltip_text(slide.title)
        .build();
    title.update_property(&[PropertyLabel(slide.title)]);

    let body = Label::builder()
        .label(slide.body)
        .wrap(true)
        .max_width_chars(48)
        .css_classes(["dim-label", "body"])
        .tooltip_text(slide.body)
        .build();
    body.update_property(&[PropertyLabel(slide.body)]);

    page.append(&icon);
    page.append(&title);
    page.append(&body);
    page
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::gtk::{self, test},
    };

    use crate::ui::welcome::slides::{WELCOME_SLIDES, should_show_welcome};

    #[test]
    fn welcome_shows_once_on_empty_state() -> Result<()> {
        ensure!(
            should_show_welcome(true, false),
            "first run with no library must show the tour"
        );
        ensure!(
            !should_show_welcome(true, true),
            "a seen tour must not reappear"
        );
        ensure!(
            !should_show_welcome(false, false),
            "a configured library must not trigger the tour"
        );
        ensure!(
            !should_show_welcome(false, true),
            "a seen tour with a library must not trigger"
        );
        ensure!(
            WELCOME_SLIDES.len() == 4,
            "tour must cover the four hi-fi feature slides"
        );
        Ok(())
    }
}
