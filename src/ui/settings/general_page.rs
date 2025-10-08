use std::{cell::Cell, rc::Rc};

use gtk4::Switch;
use libadwaita::{
    ActionRow, PreferencesGroup, PreferencesPage,
    prelude::{ActionRowExt, PreferencesGroupExt, PreferencesPageExt},
};

/// Manages the UI and logic for the General settings page within the settings dialog.
///
/// This struct encapsulates the parameters needed to create and configure the General
/// preferences page, reducing the number of function arguments and improving code organization.
pub struct GeneralSettingsPage {
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    show_dr_badges_setting: Rc<Cell<bool>>,
    use_original_year_setting: Rc<Cell<bool>>,
}

impl GeneralSettingsPage {
    /// Creates a new `GeneralSettingsPage` instance, holding necessary shared state.
    ///
    /// # Arguments
    ///
    /// * `refresh_library_ui` - Callback to refresh the main library UI.
    /// * `sort_ascending` - Shared state for album sort direction.
    /// * `sort_ascending_artists` - Shared state for artist sort direction.
    /// * `show_dr_badges_setting` - Shared state for DR badges visibility.
    /// * `use_original_year_setting` - Shared state for original year display.
    ///
    /// # Returns
    ///
    /// A new `GeneralSettingsPage` instance.
    pub fn new(
        refresh_library_ui: Rc<dyn Fn(bool, bool)>,
        sort_ascending: Rc<Cell<bool>>,
        sort_ascending_artists: Rc<Cell<bool>>,
        show_dr_badges_setting: Rc<Cell<bool>>,
        use_original_year_setting: Rc<Cell<bool>>,
    ) -> Self {
        Self {
            refresh_library_ui,
            sort_ascending,
            sort_ascending_artists,
            show_dr_badges_setting,
            use_original_year_setting,
        }
    }

    /// Creates and configures the General preferences page.
    ///
    /// This method sets up the General page with display and performance settings.
    ///
    /// # Returns
    ///
    /// A configured `PreferencesPage` for general settings.
    pub fn create_page(&self) -> PreferencesPage {
        // --- General Page ---
        let general_page = PreferencesPage::builder()
            .title("General")
            .icon_name("preferences-system-symbolic")
            .build();

        // Group for General settings
        let general_group = PreferencesGroup::builder().title("Display").build();

        // Toggle switch for DR Value badges
        let dr_badges_row = ActionRow::builder()
            .title("Show DR Value Badges")
            .subtitle("Toggle the visibility of Dynamic Range (DR) Value badges.")
            .activatable(false)
            .build();
        let dr_badges_switch = Switch::builder()
            .valign(gtk4::Align::Center)
            .active(self.show_dr_badges_setting.get())
            .build();
        dr_badges_row.add_suffix(&dr_badges_switch);
        dr_badges_row.set_activatable_widget(Some(&dr_badges_switch));
        let show_dr_badges_setting_clone = self.show_dr_badges_setting.clone();
        let refresh_library_ui_clone = self.refresh_library_ui.clone();
        let sort_ascending_clone = self.sort_ascending.clone();
        let sort_ascending_artists_clone = self.sort_ascending_artists.clone();
        dr_badges_switch.connect_active_notify(move |switch| {
            show_dr_badges_setting_clone.set(switch.is_active());

            // Trigger a UI refresh to update the visibility of DR badges
            (refresh_library_ui_clone)(
                sort_ascending_clone.get(),
                sort_ascending_artists_clone.get(),
            );
        });
        general_group.add(&dr_badges_row);

        // Toggle switch for "Use Original Year"
        let use_original_year_row = ActionRow::builder()
            .title("Use Original Year for Albums")
            .subtitle("Display the original release year instead of the release year.")
            .activatable(false)
            .build();
        let use_original_year_switch = Switch::builder()
            .valign(gtk4::Align::Center)
            .active(self.use_original_year_setting.get())
            .build();
        use_original_year_row.add_suffix(&use_original_year_switch);
        use_original_year_row.set_activatable_widget(Some(&use_original_year_switch));
        let use_original_year_setting_clone = self.use_original_year_setting.clone();
        let refresh_library_ui_clone_for_year = self.refresh_library_ui.clone();
        let sort_ascending_clone_for_year = self.sort_ascending.clone();
        let sort_ascending_artists_clone_for_year = self.sort_ascending_artists.clone();
        use_original_year_switch.connect_active_notify(move |switch| {
            use_original_year_setting_clone.set(switch.is_active());

            // Trigger a UI refresh to update the year display
            (refresh_library_ui_clone_for_year)(
                sort_ascending_clone_for_year.get(),
                sort_ascending_artists_clone_for_year.get(),
            );
        });
        general_group.add(&use_original_year_row);
        general_page.add(&general_group);
        general_page
    }
}
