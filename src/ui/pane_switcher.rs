//! Library view switcher group for wide and narrow modes.

use libadwaita::{
    HeaderBar, ViewStack, ViewSwitcher, ViewSwitcherBar, ViewSwitcherPolicy::Wide,
    gtk::accessible::Property::Label, prelude::AccessibleExtManual,
};

/// Library view switcher widgets shared with the window shell for adaptive
/// narrow-mode wiring.
#[derive(Debug)]
pub struct SwitcherGroup {
    /// Header bar holding the wide-mode `ViewSwitcher` in its title slot.
    pub header: HeaderBar,
    /// Wide-mode switcher shown in the header bar title slot.
    pub switcher: ViewSwitcher,
    /// Narrow-mode `ViewSwitcherBar` shown at the bottom of the content pane.
    pub bar: ViewSwitcherBar,
}

/// Build wide and narrow view switchers bound to the library stack.
///
/// # Arguments
///
/// * `stack` - Library `ViewStack` owning album, artist, and signal pages.
///
/// # Returns
///
/// Wide-mode switcher for the header title slot and narrow-mode bar for the
/// bottom of the content pane.
#[must_use]
pub fn build_switcher_group(stack: &ViewStack) -> (ViewSwitcher, ViewSwitcherBar) {
    let switcher = ViewSwitcher::builder()
        .policy(Wide)
        .stack(stack)
        .css_classes(["view-switcher", "wide"])
        .tooltip_text("Switch views")
        .build();
    switcher.update_property(&[Label("Switch between Albums, Artists, and Signal views")]);
    let switcher_bar = ViewSwitcherBar::builder()
        .stack(stack)
        .css_classes(["view-switcher-bar"])
        .tooltip_text("Switch views")
        .build();
    switcher_bar.update_property(&[Label("Switch between Albums, Artists, and Signal views")]);
    (switcher, switcher_bar)
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            HeaderBar, ViewStack, ViewSwitcher, ViewSwitcherBar,
            gtk::{self, accessible::Property::Label, test},
            prelude::AccessibleExtManual,
        },
    };

    use crate::ui::pane_switcher::SwitcherGroup;

    #[test]
    fn switcher_group_fields_are_accessible() -> Result<()> {
        let stack = ViewStack::builder()
            .css_classes(["test-fixture"])
            .tooltip_text("Test stack")
            .build();
        stack.update_property(&[Label("Test stack")]);
        let group = SwitcherGroup {
            header: HeaderBar::builder()
                .css_classes(["test-fixture"])
                .tooltip_text("Test header")
                .build(),
            switcher: ViewSwitcher::builder()
                .stack(&stack)
                .css_classes(["test-fixture", "wide"])
                .tooltip_text("Test switcher")
                .build(),
            bar: ViewSwitcherBar::builder()
                .stack(&stack)
                .css_classes(["test-fixture"])
                .tooltip_text("Test bar")
                .build(),
        };
        ensure!(
            group.switcher.stack().is_some(),
            "switcher must be bound to a stack"
        );
        ensure!(
            group.bar.stack().is_some(),
            "switcher bar must be bound to a stack"
        );
        Ok(())
    }
}
