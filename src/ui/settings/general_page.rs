use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gtk4::{Button, StringList, StringObject, Switch, Window};
use libadwaita::{
    ActionRow, ComboRow, PreferencesGroup, PreferencesPage,
    prelude::{
        ActionRowExt, ButtonExt, Cast, ComboRowExt, ListModelExt, PreferencesGroupExt,
        PreferencesPageExt,
    },
};

use crate::ui::components::dialogs::show_performance_metrics_dialog;

/// Creates and configures the General preferences page.
///
/// This function sets up the General page with display and performance settings.
///
/// # Arguments
///
/// * `parent` - The parent window for dialogs.
/// * `refresh_library_ui` - Callback to refresh the main library UI.
/// * `sort_ascending` - Shared state for album sort direction.
/// * `sort_ascending_artists` - Shared state for artist sort direction.
/// * `show_dr_badges_setting` - Shared state for DR badges visibility.
/// * `use_original_year_setting` - Shared state for original year display.
/// * `view_mode_setting` - Shared state for view mode.
///
/// # Returns
///
/// A configured `PreferencesPage` for general settings.
#[allow(clippy::too_many_arguments)]
pub fn create_general_page<T: Clone + 'static>(
    parent: &T,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    show_dr_badges_setting: Rc<Cell<bool>>,
    use_original_year_setting: Rc<Cell<bool>>,
    view_mode_setting: Rc<RefCell<String>>,
) -> PreferencesPage
where
    T: AsRef<Window>,
{
    // --- General Page ---
    let general_page = PreferencesPage::builder()
        .title("General")
        .icon_name("preferences-system-symbolic")
        .build();

    // Group for General settings
    let general_group = PreferencesGroup::builder().title("Display").build();

    // Group for Performance settings
    let performance_group = PreferencesGroup::builder().title("Performance").build();

    // Button to show performance metrics
    let performance_metrics_row = ActionRow::builder()
        .title("Performance Metrics")
        .subtitle("View detailed performance statistics and metrics.")
        .activatable(true)
        .build();
    let performance_metrics_button = Button::builder()
        .label("Show Metrics")
        .valign(gtk4::Align::Center)
        .build();
    performance_metrics_row.add_suffix(&performance_metrics_button);
    performance_metrics_row.set_activatable_widget(Some(&performance_metrics_button));

    // Clone necessary variables for the button click handler
    let parent_window_clone = parent.clone();
    performance_metrics_button.connect_clicked(move |_| {
        // We need to get the parent window for the dialog
        show_performance_metrics_dialog(parent_window_clone.as_ref());
    });
    performance_group.add(&performance_metrics_row);

    // Toggle switch for DR Value badges
    let dr_badges_row = ActionRow::builder()
        .title("Show DR Value Badges")
        .subtitle("Toggle the visibility of Dynamic Range (DR) Value badges.")
        .activatable(false)
        .build();
    let dr_badges_switch = Switch::builder()
        .valign(gtk4::Align::Center)
        .active(show_dr_badges_setting.get())
        .build();
    dr_badges_row.add_suffix(&dr_badges_switch);
    dr_badges_row.set_activatable_widget(Some(&dr_badges_switch));
    let show_dr_badges_setting_clone = show_dr_badges_setting.clone();
    let refresh_library_ui_clone = refresh_library_ui.clone();
    let sort_ascending_clone = sort_ascending.clone();
    let sort_ascending_artists_clone = sort_ascending_artists.clone();
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
        .active(use_original_year_setting.get())
        .build();
    use_original_year_row.add_suffix(&use_original_year_switch);
    use_original_year_row.set_activatable_widget(Some(&use_original_year_switch));
    let use_original_year_setting_clone = use_original_year_setting.clone();
    let refresh_library_ui_clone_for_year = refresh_library_ui.clone();
    let sort_ascending_clone_for_year = sort_ascending.clone();
    let sort_ascending_artists_clone_for_year = sort_ascending_artists.clone();
    use_original_year_switch.connect_active_notify(move |switch| {
        use_original_year_setting_clone.set(switch.is_active());

        // Trigger a UI refresh to update the year display
        (refresh_library_ui_clone_for_year)(
            sort_ascending_clone_for_year.get(),
            sort_ascending_artists_clone_for_year.get(),
        );
    });
    general_group.add(&use_original_year_row);

    // ComboRow for View Mode
    let view_mode_row = ComboRow::builder()
        .title("View Mode")
        .subtitle("Choose how albums and artists are displayed.")
        .build();
    let view_options = StringList::new(&["Grid View", "List View"]);
    view_mode_row.set_model(Some(&view_options));

    // Set default selection based on current setting
    let initial_view_mode_index = match view_mode_setting.borrow().as_str() {
        "Grid View" => 0,
        _ => 1,
    };
    view_mode_row.set_selected(initial_view_mode_index);
    let view_mode_setting_clone = view_mode_setting.clone();
    let refresh_library_ui_clone = refresh_library_ui.clone();
    let sort_ascending_clone = sort_ascending.clone();
    let sort_ascending_artists_clone = sort_ascending_artists.clone();
    view_mode_row.connect_selected_notify(move |combo_row| {
        let selected_index = combo_row.selected();
        let selected_item = view_options
            .item(selected_index)
            .and_then(|obj| obj.downcast::<StringObject>().ok())
            .map(|s_obj| s_obj.string().to_string());
        if let Some(mode) = selected_item {
            *view_mode_setting_clone.borrow_mut() = mode;
            (refresh_library_ui_clone)(
                sort_ascending_clone.get(),
                sort_ascending_artists_clone.get(),
            );
        }
    });
    general_group.add(&view_mode_row);
    general_page.add(&general_group);
    general_page.add(&performance_group);
    general_page
}
