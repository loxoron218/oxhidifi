use std::{rc::Rc, result::Result::Ok, sync::Arc, thread::spawn};
use std::cell::{Cell, RefCell};

use gtk4::{Button, ButtonsType::OkCancel, FileChooserAction::SelectFolder, FileChooserDialog, Label, MessageDialog, MessageType::Warning, Stack, Window};
use gtk4::ResponseType::{Accept, Cancel, Ok as GtkOk};
use glib::MainContext;
use libadwaita::prelude::{ButtonExt, DialogExt, FileChooserExt, FileExt, GtkWindowExt, IsA, WidgetExt};
use sqlx::SqlitePool;
use tokio::{runtime::Runtime, sync::mpsc::UnboundedSender};

use crate::data::db::insert_or_get_folder;
use crate::data::scanner::scan_folder;
use crate::ui::components::sorting::SortOrder;
use crate::ui::settings_window::show_settings_dialog;

/// Connects the add folder button to show a folder chooser dialog and trigger scanning.
pub fn create_add_folder_dialog_handler<T: IsA<Window> + Clone + 'static>(
    parent_window: T,
    scanning_label: Label,
    db_pool: Arc<SqlitePool>,
    sender: UnboundedSender<()>,
    albums_inner_stack: Rc<RefCell<Option<Stack>>>,
) -> Box<dyn Fn() + 'static> {
    let scanning_label = scanning_label.clone();
    let db_pool = db_pool.clone();
    let sender = sender.clone();
    let parent_window = parent_window.clone();
    let albums_inner_stack = albums_inner_stack.clone(); // This clone is now a clone of the Rc
    Box::new(move || {
        let dialog = FileChooserDialog::new(
            Some("Open Folder"),
            Some(&parent_window),
            SelectFolder,
            &[
                ("Cancel", Cancel),
                ("Open", Accept),
            ],
        );
        dialog.set_modal(true);
        dialog.set_transient_for(Some(&parent_window));
        let scanning_label_clone = scanning_label.clone();
        let db_pool_clone = db_pool.clone();
        let sender_clone = sender.clone();
        let albums_inner_stack_clone = albums_inner_stack.clone();
        dialog.connect_response(move |dialog, resp| {
            if resp == Accept {
                if let Some(folder) = dialog.file() {
                    if let Some(folder_path) = folder.path() {
                        let folder_path_string = folder_path.to_string_lossy().to_string();
                        if let Some(stack) = albums_inner_stack_clone.borrow().as_ref() { // Borrow the RefCell and then get the Option
                            stack.set_visible_child_name("scanning_state");
                            scanning_label_clone.set_visible(true);
                        } else {
                            scanning_label_clone.set_visible(true);
                        }
                        let db_pool_thread = db_pool_clone.clone();
                        let folder_path_string2 = folder_path_string.clone();
                        let sender_for_spawn = sender_clone.clone(); // Clone for the spawned thread
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
                                sender_for_spawn.send(()).ok();
                            });
                        });
                    }
                }
            }
            dialog.close();
        });
        dialog.show();
    })
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
    is_settings_open: Rc<Cell<bool>>,
) {
    let window_clone = parent_window.clone();
    let sort_orders2 = sort_orders.clone();
    let refresh_library_ui = refresh_library_ui.clone();
    let sort_ascending = sort_ascending.clone();
    let sort_ascending_artists = sort_ascending_artists.clone();
    let db_pool2 = db_pool.clone();
    let is_settings_open_for_closure = is_settings_open.clone();
    settings_button.connect_clicked(move |_| {
        let db_pool = db_pool2.clone();
        let refresh_library_ui_clone = refresh_library_ui.clone();
        let sort_ascending_clone = sort_ascending.clone();
        let sort_ascending_artists_clone = sort_ascending_artists.clone();
        let window_clone_inner = window_clone.clone();
        let sort_orders_clone = sort_orders2.clone();
        let is_settings_open_for_async = is_settings_open_for_closure.clone();
        MainContext::default().spawn_local(async move {
            show_settings_dialog(
                &window_clone_inner,
                sort_orders_clone,
                refresh_library_ui_clone,
                sort_ascending_clone,
                sort_ascending_artists_clone,
                db_pool.clone(),
                is_settings_open_for_async,
            );
        });
    });
}

/// Shows a confirmation dialog for removing a folder.
/// The `on_confirm` closure is called if the user confirms the removal.
pub fn show_remove_folder_confirmation_dialog<F: FnOnce() + 'static>(
    parent: &impl IsA<Window>,
    on_confirm: F,
) where F: FnOnce() + 'static {
    let on_confirm_rc = Rc::new(RefCell::new(Some(on_confirm)));
    let dialog = MessageDialog::builder()
        .transient_for(parent)
        .modal(true)
        .buttons(OkCancel)
        .message_type(Warning)
        .text("Remove Folder?")
        .secondary_text("Removing this folder will delete all custom metadata associated with your music, including Best DR values. This action cannot be undone.")
        .build();

    // Make the "OK" button red to indicate a destructive action
    if let Some(ok_button) = dialog.widget_for_response(GtkOk) {
        ok_button.add_css_class("destructive-action");
    }
    dialog.connect_response(move |dialog, response| {
        if response == GtkOk {
            if let Some(f) = on_confirm_rc.borrow_mut().take() {
                f();
            }
        }
        dialog.close();
    });
    dialog.show();
}