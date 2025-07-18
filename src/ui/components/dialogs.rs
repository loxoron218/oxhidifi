use std::{rc::Rc, sync::Arc, thread::spawn};
use std::cell::{Cell, RefCell};

use gtk4::{Button, FileChooserAction, FileChooserDialog, Label, ResponseType, Window};
use glib::MainContext;
use libadwaita::prelude::{ButtonExt, DialogExt, FileChooserExt, FileExt, GtkWindowExt, IsA, WidgetExt};
use sqlx::SqlitePool;
use tokio::{runtime::Runtime, sync::mpsc::UnboundedSender};

use crate::data::db::insert_or_get_folder;
use crate::data::scanner::scan_folder;
use crate::ui::components::sorting::SortOrder;
use crate::ui::settings_window::show_settings_dialog;

/// Connects the add folder button to show a folder chooser dialog and trigger scanning.
pub fn connect_add_folder_dialog<T: IsA<Window> + Clone + 'static>(
    add_button: &Button,
    parent_window: T,
    scanning_label: Label,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>, 
    albums_inner_stack: Option<gtk4::Stack>,
) {
    let scanning_label = scanning_label.clone();
    let db_pool = db_pool.clone();
    let sender = sender.clone();
    let parent_window = parent_window.clone();
    let albums_inner_stack_outer = albums_inner_stack.clone(); // Clone for the outer closure
    add_button.connect_clicked(move |_| {
        let dialog = FileChooserDialog::new(
            Some("Open Folder"),
            Some(&parent_window),
            FileChooserAction::SelectFolder,
            &[
                ("Cancel", ResponseType::Cancel),
                ("Open", ResponseType::Accept),
            ],
        );
        dialog.set_modal(true);
        dialog.set_transient_for(Some(&parent_window));
        let scanning_label = scanning_label.clone();
        let db_pool = db_pool.clone();
        let sender = sender.clone();
        let albums_inner_stack_inner = albums_inner_stack_outer.clone(); // Clone for the inner closure
        dialog.connect_response(move |dialog, resp| {
            let scanning_label = scanning_label.clone();
            let db_pool = db_pool.clone();
            let sender = sender.clone();
            if resp == ResponseType::Accept {
                if let Some(folder) = dialog.file() {
                    if let Some(folder_path) = folder.path() {
                        let folder_path_string = folder_path.to_string_lossy().to_string();
                        if let Some(stack) = albums_inner_stack_inner.clone() {
                            stack.set_visible_child_name("scanning_state");
                        } else {
                            scanning_label.set_visible(true);
                        }
                        let db_pool_thread = db_pool.clone();
                        let folder_path_string2 = folder_path_string.clone();
                        let sender_clone = sender.clone();
                        spawn(move || {
                            let rt = Runtime::new().unwrap();
                            rt.block_on(async {
                                let folder_id = match insert_or_get_folder(
                                    &db_pool_thread,
                                    &folder_path_string2,
                                )
                                .await
                                {
                                    Ok(id) => id,
                                    Err(_) => return,
                                };
                                let _ = scan_folder(
                                    &db_pool_thread,
                                    &folder_path_string2,
                                    folder_id,
                                )
                                .await;

                                // Notify main thread to update UI
                                sender_clone.send(()).ok();
                            });
                        });
                    }
                }
            }
            dialog.close();
        });
        dialog.show();
    });
}

/// Connects the settings button to open the settings dialog asynchronously.
pub fn connect_settings_dialog(
    settings_button: &Button,
    parent_window: impl IsA<Window> + Clone + 'static,
    sort_orders: Rc<RefCell<Vec<SortOrder>>>,
    refresh_library_ui: Rc<dyn Fn(bool, bool)>,
    sort_ascending: Rc<Cell<bool>>,
    sort_ascending_artists: Rc<Cell<bool>>,
    db_pool: Arc<SqlitePool>,
) {
    let window_clone = parent_window.clone();
    let sort_orders2 = sort_orders.clone();
    let refresh_library_ui = refresh_library_ui.clone();
    let sort_ascending = sort_ascending.clone();
    let sort_ascending_artists = sort_ascending_artists.clone();
    let db_pool2 = db_pool.clone();
    settings_button.connect_clicked(move |_| {
        let db_pool = db_pool2.clone();
        let refresh_library_ui_clone = refresh_library_ui.clone();
        let sort_ascending_clone = sort_ascending.clone();
        let sort_ascending_artists_clone = sort_ascending_artists.clone();
        let window_clone_inner = window_clone.clone();
        let sort_orders_clone = sort_orders2.clone();
        MainContext::default().spawn_local(async move {
            show_settings_dialog(
                &window_clone_inner,
                sort_orders_clone,
                refresh_library_ui_clone,
                sort_ascending_clone,
                sort_ascending_artists_clone,
                db_pool.clone(),
            );
        });
    });
}