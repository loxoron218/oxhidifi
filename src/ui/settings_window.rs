use std::{rc::Rc, sync::Arc};
use std::cell::{Cell, RefCell};

use glib::{MainContext, Propagation};
use glib::source::idle_add_local_once;
use gtk4::{Align, Button, Entry, EventControllerKey, ListBox, SelectionMode, Window};
use gtk4::gdk::Key;
use libadwaita::{ActionRow, PreferencesGroup, PreferencesPage, PreferencesWindow};
use libadwaita::prelude::{ActionRowExt, ButtonExt, Cast, EditableExt, GtkWindowExt, IsA, ObjectExt, ObjectType, PreferencesGroupExt, PreferencesPageExt, PreferencesWindowExt, StaticType, WidgetExt};
use sqlx::SqlitePool;

use crate::data::db::{fetch_album_details_by_id, fetch_all_folders, remove_album_and_tracks, remove_folder_and_albums};
use crate::data::models::Folder;
use crate::ui::components::config::{load_settings, save_settings, Settings};
use crate::ui::components::dialogs::show_remove_folder_confirmation_dialog;
use crate::ui::components::sorting::{connect_sort_reorder_handler, make_sort_row, update_sorting_row_numbers, SortOrder};
use crate::utils::best_dr_persistence::{AlbumKey, DrValueStore};

/// Show the settings dialog. Call from your settings button handler.
/// Accepts a shared SortOrder state and a callback to refresh the albums grid.
pub fn show_settings_dialog(
    parent: &impl IsA<Window>,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    db_pool: Arc<SqlitePool>,
    is_settings_open: Rc<Cell<bool>>,
) {

    // Create the settings window (acts as a modal dialog)
    let dialog = PreferencesWindow::builder()
        .transient_for(parent)
        .default_width(900)
        .default_height(700)
        .modal(true)
        .build();
    is_settings_open.set(true);

    // Add margin to match Bottles spacing
    dialog.set_margin_top(32);
    dialog.set_margin_bottom(32);
    dialog.set_margin_start(32);
    dialog.set_margin_end(32);

    // General page
    let general_page = PreferencesPage::builder()
        .title("General")
        .icon_name("preferences-system-symbolic")
        .build();
    let general_group = PreferencesGroup::builder().build();
    ("general_group ptr: {:?}", general_group.as_ptr());

    // Folders group (before sorting)
    let folders_group = PreferencesGroup::builder()
        .title("Library Folders")
        .description("Remove folders to exclude their music from your library.")
        .build();
    let list_box = ListBox::new();
    list_box.set_selection_mode(SelectionMode::None);
    folders_group.add(&list_box);
    let folders_group = Rc::new(folders_group);
    let list_box = Rc::new(list_box);
    ("folders_group ptr: {:?}", folders_group.as_ref().as_ptr());
    let main_context = Rc::new(MainContext::default());

    // Helper to update the folders group UI
    fn refresh_folder_display(
        folders_group: Rc<PreferencesGroup>,
        folders: &[Folder],
        db_pool: Arc<SqlitePool>,
        refresh_library_ui: Rc<dyn Fn(bool, bool)>,
        sort_ascending: Rc<Cell<bool>>,
        sort_ascending_artists: Rc<Cell<bool>>,
        main_context: Rc<MainContext>,
        list_box: Rc<ListBox>,
    ) {

        // Print all direct children before removal
        let mut child = folders_group.as_ref().first_child();
        ("--- Direct children of folders_group before removal ---");
        while let Some(widget) = child {
            ("Direct child: {} (ptr: {:?})", widget.type_().name(), widget.as_ptr());
            child = widget.next_sibling();
        }

        // Remove all children from the ListBox
        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }

        // Sort folders alphabetically by path before displaying
        let mut sorted_folders = folders.to_vec();
        sorted_folders.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
        if sorted_folders.is_empty() {
            let empty_row = ActionRow::builder()
                .title("No folders added.")
                .activatable(false)
                .selectable(false)
                .build();
            empty_row.add_css_class("dim-label");
            list_box.append(&empty_row);
            list_box.queue_allocate();
            list_box.queue_draw();
            return;
        } else {
            for folder in &sorted_folders {
                if folder.path.trim().is_empty() {
                    continue;
                }
                let row = ActionRow::builder()
                    .title(folder.path.clone())
                    .build();
                let remove_btn = Button::builder()
                    .icon_name("window-close-symbolic")
                    .valign(Align::Center)
                    .css_classes(["flat"])
                    .build();
                let folder_id = folder.id;

// Explicitly clone variables for closure capture
let db_pool_c = db_pool.clone();
let refresh_library_ui_c = refresh_library_ui.clone();
let sort_ascending_c = sort_ascending.clone();
let sort_ascending_artists_c = sort_ascending_artists.clone();
let folders_group_c = folders_group.clone();
let main_context_c = main_context.clone();
let list_box_c = list_box.clone();
remove_btn.connect_clicked(move |btn| {
    let parent_widget = btn.ancestor(Window::static_type()).expect("Should be in a window");
    let parent_window = parent_widget.downcast_ref::<Window>().expect("Should be a window");
    let db_pool = db_pool_c.clone();
    let refresh_library_ui = refresh_library_ui_c.clone();
    let sort_ascending = sort_ascending_c.clone();
    let sort_ascending_artists = sort_ascending_artists_c.clone();
    let folders_group = folders_group_c.clone();
    let main_context = main_context_c.clone();
    let list_box = list_box_c.clone();
    let folder_id = folder_id;
    show_remove_folder_confirmation_dialog(parent_window, move || {
        main_context.spawn_local({
            let db_pool = db_pool.clone();
            let refresh_library_ui = refresh_library_ui.clone();
            let sort_ascending = sort_ascending.clone();
            let sort_ascending_artists = sort_ascending_artists.clone();
            let folders_group = folders_group.clone();
            let list_box = list_box.clone();
            let main_context = main_context.clone();
            let folder_id = folder_id;
            async move {
                let _ = remove_folder_and_albums(&db_pool, folder_id).await;
                (refresh_library_ui)(sort_ascending.get(), sort_ascending_artists.get());
                let folders = fetch_all_folders(&db_pool).await.unwrap_or_else(|_| vec![]);
                let folders_group = folders_group.clone();
                let db_pool = db_pool.clone();
                let refresh_library_ui = refresh_library_ui.clone();
                let sort_ascending = sort_ascending.clone();
                let sort_ascending_artists = sort_ascending_artists.clone();
                let main_context = main_context.clone();
                let list_box = list_box.clone();
                idle_add_local_once(move || {
                    refresh_folder_display(
                        folders_group,
                        &folders,
                        db_pool,
                        refresh_library_ui,
                        sort_ascending,
                        sort_ascending_artists,
                        main_context,
                        list_box,
                    );
                });
            }
        });
    });
});
                row.add_suffix(&remove_btn);
                list_box.append(&row);
            }
            list_box.queue_allocate();
            list_box.queue_draw();
        }
    }

    // Initial population of the folders group
    let folders_group_clone = folders_group.clone();
    let db_pool_clone = db_pool.clone();
    let refresh_library_ui_clone = refresh_library_ui.clone();
    let sort_ascending_clone = sort_ascending.clone();
    let sort_ascending_artists_clone = sort_ascending_artists.clone();
    let main_context_clone = main_context.clone();
    main_context.spawn_local(async move {
        let folders = fetch_all_folders(&db_pool_clone).await.unwrap_or_else(|_| vec![]);
        idle_add_local_once(move || {
            refresh_folder_display(
                folders_group_clone,
                &folders,
                db_pool_clone,
                refresh_library_ui_clone,
                sort_ascending_clone,
                sort_ascending_artists_clone,
                main_context_clone,
                list_box.clone(),
            );
        });
    });

    // Sorting group with title
    let sorting_group = PreferencesGroup::builder()
        .title("Sorting")
        .description("Albums will be sorted according to the order below. Drag to reorder.")
        .build();
    ("sorting_group ptr: {:?}", sorting_group.as_ptr());

    // Restore the declaration of sort_listbox before use
    let sort_listbox = ListBox::new();
    sort_listbox.set_selection_mode(SelectionMode::None);
    sorting_group.add(&sort_listbox);

    // Populate the ListBox with ActionRows for each sort order
    for order in sort_orders.borrow().iter() {
        let list_row = make_sort_row(
            order,
            sort_orders.clone(),
            refresh_library_ui.clone(),
            sort_ascending.clone(),
            sort_ascending_artists.clone(),
        );
        sort_listbox.append(&list_row);
    }
    update_sorting_row_numbers(&sort_listbox);

    // On reorder, update shared sort_orders, persist, and refresh
    connect_sort_reorder_handler(
        &sort_listbox,
        sort_orders.clone(),
        refresh_library_ui.clone(),
        sort_ascending.clone(),
        sort_ascending_artists.clone(),
    );

    // Save sort order on settings window close
    let sort_orders_rc = sort_orders.clone();
    let is_settings_open_clone = is_settings_open.clone();
    dialog.connect_close_request(move |_| {
        let current_orders = sort_orders_rc.borrow().clone();
        let prev = load_settings();
        let _ = save_settings(&Settings {
            sort_orders: current_orders,
            sort_ascending_albums: prev.sort_ascending_albums,
            sort_ascending_artists: prev.sort_ascending_artists,
            completed_albums: prev.completed_albums,
        });
        is_settings_open_clone.set(false);
        Propagation::Proceed
    });

    // Add groups to the general page
    general_page.add(folders_group.as_ref());
    general_page.add(&general_group);
    general_page.add(&sorting_group);

    // ESC key closes dialog
    let key_controller = EventControllerKey::new();
    {
        let dialog = dialog.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == Key::Escape {
                dialog.close();
                return true.into();
            }
            false.into()
        });
    }
    dialog.add_controller(key_controller);

    // Library page
    let library_page = PreferencesPage::builder()
        .title("Library")
        .icon_name("folder-music-symbolic")
        .build();

    // Group for individual album deletion
    let individual_deletion_group = PreferencesGroup::builder()
        .title("Delete Individual Albums")
        .description("WARNING: This will permanently delete the album from your library and its DR value from your preferences.")
        .build();
    let album_id_entry = Entry::builder()
        .placeholder_text("Enter Album ID to delete")
        .build();
    individual_deletion_group.add(&album_id_entry);
    let delete_album_button = Button::builder()
        .label("Delete Album")
        .css_classes(vec!["destructive-action"])
        .build();
    individual_deletion_group.add(&delete_album_button);
    let db_pool_clone_for_delete = db_pool.clone();
    let refresh_library_ui_clone_for_delete = refresh_library_ui.clone();
    let sort_ascending_clone_for_delete = sort_ascending.clone();
    let sort_ascending_artists_clone_for_delete = sort_ascending_artists.clone();
    delete_album_button.connect_clicked(move |_| {
        let album_id_str = album_id_entry.text().to_string();
        if let Ok(album_id) = album_id_str.parse::<i64>() {
            let db_pool = db_pool_clone_for_delete.clone();
            let refresh_library_ui = refresh_library_ui_clone_for_delete.clone();
            let sort_ascending = sort_ascending_clone_for_delete.clone();
            let sort_ascending_artists = sort_ascending_artists_clone_for_delete.clone();
            MainContext::default().spawn_local(async move {

                // Remove from database
                if let Err(_e) = remove_album_and_tracks(&db_pool, album_id).await {
                } else {

                    // Remove from JSON store
                    let mut dr_store = DrValueStore::load();
                    if let Ok(album_details) = fetch_album_details_by_id(&db_pool, album_id).await {
                        let album_key = AlbumKey {
                            title: album_details.title,
                            artist: album_details.artist_name,
                            folder_path: album_details.folder_path,
                        };
                        dr_store.remove_dr_value(&album_key);
                        if let Err(_e) = dr_store.save() {
                        }
                    } else {
                    }
                    (refresh_library_ui)(sort_ascending.get(), sort_ascending_artists.get());
                }
            });
        } else {
        }
    });
    library_page.add(&individual_deletion_group);

    // Audio page
    let audio_page = PreferencesPage::builder()
        .title("Audio")
        .icon_name("audio-speakers-symbolic")
        .build();
    let audio_group = PreferencesGroup::builder().build();
    ("audio_group ptr: {:?}", audio_group.as_ptr());
    audio_page.add(&audio_group);

    // Add pages to the window
    dialog.add(&general_page);
    dialog.add(&library_page);
    dialog.add(&audio_page);
    dialog.present();
}
