use std::{cell::Cell, future::Future, rc::Rc, sync::Arc};

use glib::{MainContext, Propagation, WeakRef};
use gtk4::{Button, CallbackAction, FlowBox, KeyvalTrigger, Shortcut, ShortcutController, ToggleButton};
use gtk4::gdk::{Key, ModifierType};
use libadwaita::{ApplicationWindow, Clamp, ViewStack};
use libadwaita::prelude::{ButtonExt, ObjectExt, ToggleButtonExt, WidgetExt};
use sqlx::SqlitePool;

use crate::ui::components::config::{load_settings, save_settings};
use crate::ui::search_bar::SearchBar;

/// Handles navigation for the music library app, including stack switching and back button logic.
pub fn connect_album_navigation<Fut, F>(
    albums_grid: &FlowBox,
    stack: &ViewStack,
    db_pool: Arc<SqlitePool>,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    album_page: F,
) where
    F: Fn(
        WeakRef<ViewStack>,
        Arc<SqlitePool>,
        i64,
        WeakRef<ViewStack>,
    ) -> Fut
        + 'static,
    Fut: Future<Output = ()> + 'static,
{
    let stack_weak = stack.downgrade();
    let db_pool = db_pool.clone();
    let left_btn_stack_weak = left_btn_stack.downgrade();
    let right_btn_box = right_btn_box.downgrade();
    albums_grid.connect_child_activated(move |_, child| {
        let left_btn_stack = left_btn_stack_weak
            .upgrade()
            .expect("left_btn_stack disappeared");
        let right_btn_box = right_btn_box.upgrade().expect("right_btn_box disappeared");
        if let Some(album_id_ptr) = unsafe { child.data::<i64>("album_id") } {
            let album_id = unsafe { *album_id_ptr.as_ref() };
            left_btn_stack.set_visible_child_name("back");
            right_btn_box.set_visible(false.into());
            MainContext::default().spawn_local(album_page(
                stack_weak.clone(),
                db_pool.clone(),
                album_id,
                left_btn_stack_weak.clone(),
            ));
        }
    });
}

/// Connects the back button to navigate back to the albums grid and update button visibility.
pub fn connect_back_button(
    back_button: &Button,
    stack: &ViewStack,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    last_tab: Rc<Cell<&'static str>>,
) {
    let stack_clone = stack.clone();
    let left_btn_stack_clone = left_btn_stack.clone();
    let right_btn_box_clone = right_btn_box.clone();
    back_button.connect_clicked(move |_| {
        let tab = last_tab.get();
        stack_clone.set_visible_child_name(tab);
        left_btn_stack_clone.set_visible_child_name("main");
        right_btn_box_clone.set_visible(true);
    });
}

/// Handles Esc key navigation: if not on main grid, go back to albums and update buttons.
pub fn handle_esc_navigation(
    stack: ViewStack,
    left_btn_stack: ViewStack,
    right_btn_box: Clamp,
    last_tab: Rc<Cell<&'static str>>,
) -> impl Fn() {
    move || {
        let page = stack.visible_child_name().unwrap_or_default();
        if page != "albums" && page != "artists" {
            let tab = last_tab.get();
            stack.set_visible_child_name(tab);
            left_btn_stack.set_visible_child_name("main");
            right_btn_box.set_visible(true);
        }
    }
}

/// Handles sort button click logic and stack notifications for updating sort icons
pub fn connect_sort_button(
    sort_button: &Button,
    stack: &ViewStack,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
) {

    // Sort button click handler
    let sort_button_clone = sort_button.clone();
    let refresh_library_ui_clone = refresh_library_ui.clone();
    let sort_ascending_clone = sort_ascending.clone();
    let sort_ascending_artists_clone = sort_ascending_artists.clone();
    let stack_clone = stack.clone();
    sort_button.connect_clicked(move |_| {
        let mut settings = load_settings();
        let page = stack_clone.visible_child_name().unwrap_or_default();
        if page == "albums" {
            let asc = !sort_ascending_clone.get();
            sort_ascending_clone.set(asc);
            settings.sort_ascending_albums = asc;
            let _ = save_settings(&settings);
            sort_button_clone.set_icon_name(if asc {
                "view-sort-descending-symbolic"
            } else {
                "view-sort-ascending-symbolic"
            });
            refresh_library_ui_clone(asc, sort_ascending_artists_clone.get());
        } else if page == "artists" {
            let asc = !sort_ascending_artists_clone.get();
            sort_ascending_artists_clone.set(asc);
            settings.sort_ascending_artists = asc;
            let _ = save_settings(&settings);
            sort_button_clone.set_icon_name(if asc {
                "view-sort-descending-symbolic"
            } else {
                "view-sort-ascending-symbolic"
            });
            refresh_library_ui_clone(sort_ascending_clone.get(), asc);
        }
    });

    // Stack notification handler for updating sort icons
    let sort_button = sort_button.clone();
    let sort_ascending = sort_ascending.clone();
    let sort_ascending_artists = sort_ascending_artists.clone();
    stack.connect_notify_local(Some("visible-child-name"), move |stack, _| {
        let page = stack.visible_child_name().unwrap_or_default();
        if page == "artists" {
            sort_button.set_icon_name(if sort_ascending_artists.get() {
                "view-sort-descending-symbolic"
            } else {
                "view-sort-ascending-symbolic"
            });
        } else {
            sort_button.set_icon_name(if sort_ascending.get() {
                "view-sort-descending-symbolic"
            } else {
                "view-sort-ascending-symbolic"
            });
        }
    });
}

/// Connects navigation for albums/artists tab toggle buttons, sort reset, and last_tab tracking.
pub fn connect_tab_navigation(
    albums_btn: &ToggleButton,
    artists_btn: &ToggleButton,
    stack: &ViewStack,
    sort_button: &Button,
    last_tab: Rc<Cell<&'static str>>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    rebuild_artists_grid_opt: Option<impl Fn() + 'static>,
) {

    // Albums button logic
    {
        let stack = stack.clone();
        let sort_ascending = sort_ascending.clone();
        let sort_ascending_artists = sort_ascending_artists.clone();
        let refresh_library_ui = refresh_library_ui.clone();
        let sort_button = sort_button.clone();
        let last_tab = last_tab.clone();
        let albums_btn = albums_btn.clone();
        let artists_btn = artists_btn.clone();
        albums_btn.clone().connect_clicked(move |_| {
            last_tab.set("albums");
            stack.set_visible_child_name("albums");

            // Restore last used or persistent sort direction for albums
            let ascending = sort_ascending.get();
            sort_button.set_icon_name(if ascending {
                "view-sort-descending-symbolic"
            } else {
                "view-sort-ascending-symbolic"
            });
            refresh_library_ui(ascending, sort_ascending_artists.get());
            albums_btn.set_active(true);
            artists_btn.set_active(false.into());
        });
    }

    // Artists button logic
    {
        let stack = stack.clone();
        let sort_ascending = sort_ascending.clone();
        let sort_ascending_artists = sort_ascending_artists.clone();
        let refresh_library_ui = refresh_library_ui.clone();
        let sort_button = sort_button.clone();
        let last_tab = last_tab.clone();
        let albums_btn = albums_btn.clone();
        let artists_btn = artists_btn.clone();
        artists_btn.clone().connect_clicked(move |_| {
            last_tab.set("artists");

            // Only switch if the child exists, otherwise rebuild and then switch
            if stack.child_by_name("artists").is_some() {
                stack.set_visible_child_name("artists");
            } else if let Some(ref rebuild_artists_grid) = rebuild_artists_grid_opt {
                rebuild_artists_grid();

                // After rebuild, set visible child (will be present now)
                if stack.child_by_name("artists").is_some() {
                    stack.set_visible_child_name("artists");
                }
            }
            // Restore last used or persistent sort direction for artists
            let ascending = sort_ascending_artists.get();
            sort_button.set_icon_name(if ascending {
                "view-sort-descending-symbolic"
            } else {
                "view-sort-ascending-symbolic"
            });
            refresh_library_ui(sort_ascending.get(), ascending);
            albums_btn.set_active(false.into());
            artists_btn.set_active(true);
        });
    }
}

/// Sets up keyboard shortcuts and ESC navigation for the main window.
pub fn setup_keyboard_shortcuts(
    window: &ApplicationWindow,
    search_bar: &SearchBar,
    refresh_library_ui: &Rc<dyn Fn(bool, bool)>,
    sort_ascending: &Rc<Cell<bool>>,
    sort_ascending_artists: &Rc<Cell<bool>>,
    stack: &ViewStack,
    left_btn_stack: &ViewStack,
    right_btn_box: &Clamp,
    last_tab: &Rc<Cell<&'static str>>,
) {
    let accel_group = ShortcutController::new();
    let refresh_library_ui_esc = refresh_library_ui.clone();
    let sort_ascending_esc = sort_ascending.clone();
    let search_revealer_esc = search_bar.revealer.clone();
    let search_button_esc = search_bar.button.clone();
    let sort_ascending_artists_for_esc = sort_ascending_artists.clone();
    let esc_nav = handle_esc_navigation(
        stack.clone(),
        left_btn_stack.clone(),
        right_btn_box.clone(),
        last_tab.clone(),
    );
    let esc_shortcut = Shortcut::builder()
        .trigger(&KeyvalTrigger::new(Key::Escape, ModifierType::empty()))
        .action(&CallbackAction::new(move |_, _| {
            if search_revealer_esc.reveals_child() {
                search_revealer_esc.set_reveal_child(false);
                search_button_esc.set_visible(true);
                refresh_library_ui_esc(
                    sort_ascending_esc.get(),
                    sort_ascending_artists_for_esc.get(),
                );
                return Propagation::Stop;
            }
            esc_nav();
            Propagation::Stop
        }))
        .build();
    accel_group.add_shortcut(esc_shortcut);
    window.add_controller(accel_group);
}