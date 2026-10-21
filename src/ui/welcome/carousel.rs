//! Welcome tour carousel with slide pages and arrow controls.
//!
//! Split out to keep [`crate::ui::welcome`] under the file-size standard.

use {
    libadwaita::{
        Carousel, CarouselIndicatorDots,
        gtk::{Align::Center, Box, Button, accessible::Property::Label},
        prelude::{AccessibleExtManual, ButtonExt, WidgetExt},
    },
    num_traits::NumCast,
    tracing::warn,
};

use crate::ui::{
    signal_bus::UiHandles,
    welcome::slides::{WELCOME_SLIDES, build_slide},
};

/// Carousel with its slide pages and arrow controls.
#[derive(Debug)]
pub struct WelcomeCarousel {
    /// Slide container driving the tour pages.
    pub carousel: Carousel,
    /// Slide pages in presentation order.
    pub slides: Vec<Box>,
    /// Step-back arrow.
    pub prev: Button,
    /// Step-forward arrow.
    pub next: Button,
    /// Progress dots bound to the carousel.
    pub dots: CarouselIndicatorDots,
}

/// Scroll to the slide at `index`, clamping into range.
fn scroll_to_index(carousel: &Carousel, slides: &[Box], index: u32) {
    let pos = usize::try_from(index)
        .unwrap_or(0)
        .min(slides.len().saturating_sub(1));
    if let Some(target) = slides.get(pos) {
        carousel.scroll_to(target, true);
    }
}

/// Step the carousel by `delta` pages from its current position, clamping
/// into range.
pub fn step_slide(carousel: &Carousel, slides: &[Box], delta: i32) {
    let last = u32::try_from(slides.len().saturating_sub(1)).unwrap_or(0);
    let current: u32 = NumCast::from(carousel.position().round()).unwrap_or(0);
    let target = current.saturating_add_signed(delta).min(last);
    scroll_to_index(carousel, slides, target);
}

/// Build the carousel, its slide pages, and arrow controls.
#[must_use]
pub fn build_carousel() -> WelcomeCarousel {
    let carousel = Carousel::builder()
        .vexpand(true)
        .hexpand(true)
        .allow_mouse_drag(true)
        .css_classes(["welcome-carousel"])
        .tooltip_text("Welcome tour slides")
        .build();
    carousel.update_property(&[Label("Welcome tour slides")]);
    let mut slides = Vec::with_capacity(WELCOME_SLIDES.len());
    for slide in &WELCOME_SLIDES {
        let page = build_slide(slide);
        carousel.append(&page);
        slides.push(page);
    }

    let dots = CarouselIndicatorDots::builder()
        .css_classes(["welcome-dots"])
        .tooltip_text("Tour progress")
        .build();
    dots.update_property(&[Label("Tour progress")]);
    dots.set_carousel(Some(&carousel));

    let prev = Button::builder()
        .icon_name("go-previous-symbolic")
        .valign(Center)
        .opacity(0.0)
        .sensitive(false)
        .css_classes(["circular", "suggested-action", "image-button"])
        .tooltip_text("Previous slide")
        .build();
    prev.update_property(&[Label("Previous slide")]);

    let next = Button::builder()
        .icon_name("go-next-symbolic")
        .valign(Center)
        .css_classes(["circular", "suggested-action", "image-button"])
        .tooltip_text("Next slide")
        .build();
    next.update_property(&[Label("Next slide")]);

    let mut handles = UiHandles::default();
    let focus_carousel = carousel.clone();
    handles.retain_signal(focus_carousel.connect_map(|page| {
        if !page.grab_focus() {
            warn!(
                component = "welcome",
                "carousel did not take keyboard focus"
            );
        }
    }));
    let prev_carousel = carousel.clone();
    let prev_slides = slides.clone();
    handles.retain_signal(prev.connect_clicked(move |_| {
        step_slide(&prev_carousel, &prev_slides, -1);
    }));
    let next_carousel = carousel.clone();
    let next_slides = slides.clone();
    handles.retain_signal(next.connect_clicked(move |_| {
        step_slide(&next_carousel, &next_slides, 1);
    }));
    drop(handles);

    WelcomeCarousel {
        carousel,
        slides,
        prev,
        next,
        dots,
    }
}
