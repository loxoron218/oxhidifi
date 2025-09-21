use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use gtk4::{
    Button, ColumnView, FlowBox, Label, Stack, Window,
    gio::ListStore,
    glib::{MainContext, WeakRef},
};
use libadwaita::{
    Clamp, ViewStack,
    prelude::{ButtonExt, Cast, ListModelExt, ObjectExt},
};
use sqlx::SqlitePool;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    ui::{
        components::{
            dialogs::create_add_folder_dialog_handler,
            refresh::RefreshService,
            view_controls::{
                list_view::{
                    column_view::zoom_manager::ColumnViewZoomManager,
                    data_model::AlbumListItemObject,
                },
                view_mode::ViewMode::{self, GridView, ListView},
            },
        },
        grids::album_grid_builder::{build_albums_grid, build_albums_list_view},
        pages::album::album_page::album_page,
    },
    utils::screen::ScreenInfo,
};

/// Rebuilds the albums grid in the main window.
///
/// This function is responsible for clearing the existing albums grid,
/// re-initializing the `FlowBox` and `Stack` for albums, and adding them
/// back to the main `ViewStack`. It's called when the grid needs to be
/// completely refreshed, for instance, after a major library update.
///
/// # Arguments
/// * `stack` - The main `libadwaita::ViewStack` where the albums grid is displayed.
/// * `scanning_label_albums` - A `gtk4::Label` used to show scanning feedback.
/// * `screen_info` - A `Rc<RefCell<ScreenInfo>>` providing screen dimension details.
/// * `albums_grid_cell` - A `Rc<RefCell<Option<FlowBox>>>` holding a reference to the albums `FlowBox`.
/// * `albums_stack_cell` - A `Rc<RefCell<Option<Stack>>>` holding a reference to the albums `Stack`.
/// * `window` - The main application window.
/// * `db_pool` - The database pool for database operations.
/// * `sender` - The sender for UI refresh signals.
/// * `album_count_label` - A `gtk4::Label` to display the album count.
/// * `view_mode` - The current view mode (GridView or ListView).
/// * `use_original_year` - Whether to display the original release year.
/// * `show_dr_badges` - A `Rc<Cell<bool>>` indicating whether to show DR badges.
pub fn rebuild_albums_grid_for_window(
    stack: &ViewStack,
    scanning_label_albums: &Label,
    screen_info: &Rc<RefCell<ScreenInfo>>,
    albums_grid_cell: &Rc<RefCell<Option<FlowBox>>>,
    albums_stack_cell: &Rc<RefCell<Option<Stack>>>,
    window: &Window,
    db_pool: &Arc<SqlitePool>,
    sender: &UnboundedSender<()>,
    album_count_label: Rc<Label>,
    view_mode: ViewMode,
    use_original_year: bool,
    show_dr_badges: Rc<Cell<bool>>,
    refresh_service: Option<Rc<RefreshService>>,
    column_view_zoom_manager: Option<Rc<ColumnViewZoomManager>>,
) -> Option<ListStore> {
    // Remove old grid widget from the stack if it exists to prevent duplicates.
    if let Some(child) = stack.child_by_name("albums") {
        stack.remove(&child);
    }

    // Clear the Rc<RefCell>s to drop previous instances of FlowBox and Stack,
    // ensuring a clean rebuild and releasing associated resources.
    *albums_grid_cell.borrow_mut() = None;
    *albums_stack_cell.borrow_mut() = None;

    // Build the new albums grid or list view and its containing stack based on the view mode.
    // This ensures the albums view is always up-to-date and correctly displayed according to
    // the user's selected view preference (grid or list).
    match view_mode {
        GridView => {
            // Create a new "Add Music" button for the grid view
            let add_music_button_grid = Button::with_label("Add Music");

            // Attach the click handler to the new button
            let add_folder_handler = create_add_folder_dialog_handler(
                window.clone(),
                scanning_label_albums.clone(),
                db_pool.clone(),
                sender.clone(),
                albums_stack_cell.clone(),
            );
            add_music_button_grid.connect_clicked(move |_| {
                add_folder_handler();
            });

            // Build a new albums grid and its containing stack.
            // Get screen info values to avoid double borrowing
            let cover_size = screen_info.borrow().cover_size;
            let tile_size = screen_info.borrow().tile_size;
            let (albums_stack, albums_grid) = build_albums_grid(
                scanning_label_albums,
                cover_size,
                tile_size,
                &add_music_button_grid,
                album_count_label.clone(),
            );

            // Add the newly created albums stack to the main ViewStack.
            stack.add_titled(&albums_stack, Some("albums"), "Albums");

            // Store references to the new FlowBox and Stack in the cells for later access.
            *albums_grid_cell.borrow_mut() = Some(albums_grid.clone());
            *albums_stack_cell.borrow_mut() = Some(albums_stack.clone());
            None
        }
        ListView => {
            // Clone the necessary values for the closure
            let stack_clone = stack.clone();
            let db_pool_clone = db_pool.clone();
            let sender_clone = sender.clone();
            let show_dr_badges_clone = show_dr_badges.clone();
            let refresh_service_clone = refresh_service.clone();

            // Create a new "Add Music" button for the list view
            let add_music_button_list = Button::with_label("Add Music");

            // Attach the click handler to the new button
            let add_folder_handler = create_add_folder_dialog_handler(
                window.clone(),
                scanning_label_albums.clone(),
                db_pool.clone(),
                sender.clone(),
                albums_stack_cell.clone(),
            );
            add_music_button_list.connect_clicked(move |_| {
                add_folder_handler();
            });

            // Build a new albums list view and its containing stack.
            let (albums_stack, _column_view_scrolled, model, column_view) = build_albums_list_view(
                album_count_label.clone(),
                use_original_year,
                show_dr_badges,
                &add_music_button_list,
                Some(move |column_view: &ColumnView, position: u32| {
                    // Get the item at the activated position from the ColumnView's model
                    if let Some(model) = column_view.model()
                        && let Some(item) = model.item(position)
                    {
                        // Try to cast the generic item to AlbumListItemObject to access album data
                        if let Some(album_item) = item.downcast_ref::<AlbumListItemObject>() {
                            // Extract the album ID from the AlbumListItemObject's wrapped AlbumListItem
                            if let Some(album) = album_item.item().as_ref() {
                                // Extract the album ID from the AlbumListItemObject's wrapped AlbumListItem
                                let album_id = album.basic_info.id;

                                // Clone the necessary values for the async block
                                let stack_clone = stack_clone.clone();
                                let db_pool_clone = db_pool_clone.clone();
                                let sender_clone = sender_clone.clone();
                                let show_dr_badges_clone = show_dr_badges_clone.clone();
                                let refresh_service_clone = refresh_service_clone.clone();

                                // Spawn an async task to load and display the album detail page
                                MainContext::default().spawn_local(async move {
                                        // Create weak references for the UI components
                                        let stack_weak = stack_clone.downgrade();

                                        // Try to get the player_bar and other navigation components from the refresh service
                                        let (header_btn_stack_weak, header_right_btn_box_weak, player_bar) =
                                            if let Some(refresh_service) = refresh_service_clone {
                                                (
                                                    refresh_service.get_left_btn_stack(),
                                                    // We don't have direct access to right_btn_box in RefreshService
                                                    WeakRef::<Clamp>::new(),
                                                    Some(refresh_service.get_player_bar())
                                                )
                                            } else {
                                                // Fallback to placeholder values if refresh_service is not available
                                                (
                                                    stack_clone.downgrade(),
                                                    WeakRef::<Clamp>::new(),
                                                    None
                                                )
                                            };

                                            // If we have a player_bar, call the album_page function to build and display the album detail page
                                        if let Some(player_bar) = player_bar {
                                            // Call the album_page function to build and display the album detail page
                                            album_page(
                                                stack_weak,
                                                db_pool_clone,
                                                album_id,
                                                header_btn_stack_weak,
                                                header_right_btn_box_weak,
                                                sender_clone,
                                                show_dr_badges_clone,
                                                player_bar,
                                            ).await;
                                        } else {
                                            // If we don't have a player_bar, we can't navigate to the album detail page
                                            // This is a limitation of our simplified implementation
                                            println!("Cannot navigate to album detail page: PlayerBar not available");
                                        }
                                    });
                            }
                        }
                    }
                }),
                column_view_zoom_manager,
            );

            // Add the newly created albums stack to the main ViewStack.
            stack.add_titled(&albums_stack, Some("albums"), "Albums");

            // For ListView mode, we don't store the FlowBox since we're using a ColumnView
            // Store the stack for later access.
            *albums_stack_cell.borrow_mut() = Some(albums_stack.clone());

            // Store the ColumnView widget in the RefreshService
            if let Some(refresh_service) = refresh_service {
                refresh_service.set_column_view_widget(Some(column_view));
            }
            Some(model)
        }
    }
}
