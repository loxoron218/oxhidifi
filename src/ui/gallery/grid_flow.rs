//! `FlowBox` build, spacing, and card resize helpers for library grids.

use libadwaita::{
    gtk::{
        Align::{Center, Start},
        FlowBox, Image, Overlay, Picture,
        SelectionMode::None as SelectionNone,
        accessible::Property::Label,
    },
    prelude::{AccessibleExtManual, Cast, WidgetExt},
};

/// Map a cover size to the grid's row/column spacing in pixels.
///
/// # Arguments
///
/// * `cover_size` - Cover art size in pixels.
///
/// # Returns
///
/// Row/column spacing in pixels (180 px → 12 px).
#[must_use]
pub fn grid_spacing(cover_size: i32) -> u32 {
    (cover_size
        .max(0)
        .cast_unsigned()
        .saturating_mul(12)
        .saturating_add(90))
        / 180
}

/// Update a `FlowBox`'s row/column spacing to match `cover_size`.
pub fn apply_grid_spacing(flow: &FlowBox, cover_size: i32) {
    let spacing = grid_spacing(cover_size);
    flow.set_row_spacing(spacing);
    flow.set_column_spacing(spacing);
}

/// Resize a card's cover/avatar widget to `size` in place.
///
/// Handles both the placeholder `Image` and a decoded `Picture`, keeping
/// the existing widget tree intact so zoom never recreates the cards.
pub fn resize_overlay_cover(overlay: &Overlay, size: i32) {
    if let Some(card) = overlay.parent() {
        card.set_width_request(size);
    }
    overlay.set_width_request(size);
    overlay.set_height_request(size);
    let Some(child) = overlay.child() else {
        return;
    };
    if let Some(img) = child.downcast_ref::<Image>() {
        img.set_pixel_size(size / 2);
        img.set_width_request(size);
        img.set_height_request(size);
        return;
    }
    if let Some(pic) = child.downcast_ref::<Picture>() {
        pic.set_width_request(size);
        pic.set_height_request(size);
    }
}

/// Build a configured `FlowBox` for grid-mode display.
///
/// Spacing is scaled proportionally based on the cover size
/// (180 px → 12 px spacing).
#[must_use]
pub fn build_grid(tooltip: &str, cover_size: i32) -> FlowBox {
    let spacing = grid_spacing(cover_size);
    let flow = FlowBox::builder()
        .min_children_per_line(2)
        .valign(Start)
        .halign(Center)
        .row_spacing(spacing)
        .column_spacing(spacing)
        .selection_mode(SelectionNone)
        .can_focus(true)
        .tooltip_text(tooltip)
        .build();
    flow.update_property(&[Label(tooltip)]);
    flow
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, bail, ensure},
        libadwaita::{
            gtk::{self, Box, Image, Orientation::Vertical, Overlay, test},
            prelude::{BoxExt, Cast, WidgetExt},
        },
    };

    use crate::ui::{
        gallery::{
            card::build_placeholder,
            grid_flow::{build_grid, grid_spacing, resize_overlay_cover},
        },
        zoom::{GRID_ZOOM_MAX, grid_cover_size},
    };

    #[test]
    fn build_grid_sets_tooltip() -> Result<()> {
        let flow = build_grid("Album library grid", 180);
        ensure!(flow.tooltip_text().as_deref() == Some("Album library grid"));
        Ok(())
    }

    #[test]
    fn build_grid_keeps_two_columns_across() {
        for level in 0..=GRID_ZOOM_MAX {
            let flow = build_grid("grid", grid_cover_size(level));
            assert_eq!(
                flow.min_children_per_line(),
                2,
                "level {level} must keep two columns across"
            );
        }
    }

    #[test]
    fn grid_spacing_scales_with_cover_size() {
        let cases = [(0, 0), (120, 8), (150, 10), (180, 12), (210, 14), (240, 16)];
        for (cover_size, expected) in cases {
            assert_eq!(
                grid_spacing(cover_size),
                expected,
                "cover size {cover_size} px must map to spacing {expected} px"
            );
        }
    }

    #[test]
    fn grid_spacing_clamps_negative_cover_sizes() {
        assert_eq!(grid_spacing(-100), 0);
        assert_eq!(grid_spacing(-1), 0);
    }

    #[test]
    fn resize_overlay_cover_resizes_card_and_cover() -> Result<()> {
        let card = Box::new(Vertical, 0);
        let cover = build_placeholder(120);
        let overlay = Overlay::new();
        overlay.set_child(Some(&cover));
        card.append(&overlay);
        resize_overlay_cover(&overlay, 240);

        ensure!(
            card.width_request() == 240,
            "card width must follow the cover size"
        );
        ensure!(
            overlay.width_request() == 240,
            "overlay width must follow the cover size"
        );
        ensure!(
            overlay.height_request() == 240,
            "overlay height must follow the cover size"
        );
        let child = overlay.child();
        let Some(img) = child.as_ref().and_then(|w| w.downcast_ref::<Image>()) else {
            bail!("placeholder cover must be an Image");
        };
        ensure!(
            img.pixel_size() == 120,
            "placeholder icon must scale with the cover size"
        );
        ensure!(
            img.width_request() == 240,
            "placeholder width must follow the cover size"
        );
        ensure!(
            img.height_request() == 240,
            "placeholder height must follow the cover size"
        );
        Ok(())
    }
}
