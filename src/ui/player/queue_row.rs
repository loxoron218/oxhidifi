//! Queue row factory with drag-and-drop reordering and removal.

use {
    libadwaita::{
        gdk::{ContentProvider, DragAction},
        gio::{ListStore, prelude::ListModelExt},
        glib::{BoxedAnyObject, Value, types::Type, value::ToValue},
        gtk::{
            Align::Start, Box, Button, DragSource, DropTarget, Label, ListItem,
            Orientation::Horizontal, SignalListItemFactory,
            accessible::Property::Label as PropertyLabel, pango::EllipsizeMode::End,
        },
        prelude::{AccessibleExtManual, BoxExt, ButtonExt, Cast, ListItemExt, WidgetExt},
    },
    tracing::{error, warn},
};

use crate::{playback::queue::PlaybackQueue, ui::player::queue::QueueItemData};

/// Reorder an item within both the queue model and the `ListStore`.
fn reorder_entry(queue: &PlaybackQueue, store: &ListStore, from: usize, to: usize) {
    if from == to {
        return;
    }
    queue.move_track(from, to);
    let Ok(from_u32) = u32::try_from(from) else {
        return;
    };
    let Some(item) = store.item(from_u32) else {
        return;
    };
    store.remove(from_u32);
    let adjusted_pos = if to > from { to - 1 } else { to };
    let Ok(adjusted_u32) = u32::try_from(adjusted_pos) else {
        return;
    };
    store.insert(adjusted_u32, &item);
}

/// Process a drop value for reordering.
fn handle_drop_value(value: &Value, queue: &PlaybackQueue, store: &ListStore, to_pos: usize) {
    let from = match value.get::<i32>() {
        Ok(v) => v,
        Err(e) => {
            error!(error = %e, "Failed to get drop value");
            return;
        }
    };
    let from_u = usize::try_from(from).unwrap_or(0);
    reorder_entry(queue, store, from_u, to_pos);
}

/// Remove a track from the queue at the given position, updating the store.
/// Logs a warning if the position is out of bounds.
fn try_remove_entry(q: &PlaybackQueue, store: &ListStore, pos: usize) {
    store.remove(u32::try_from(pos).unwrap_or(0));
    if q.remove(pos).is_none() {
        warn!(pos, "Failed to remove track — position out of bounds");
    }
}

/// Create the `SignalListItemFactory` that builds and binds queue rows.
#[must_use]
pub fn build_row_factory(queue: &PlaybackQueue, store: &ListStore) -> SignalListItemFactory {
    let factory = SignalListItemFactory::new();
    let factory_queue = queue.clone();
    let factory_store = store.clone();

    factory.connect_setup(move |_, list_item| {
        let Some(list_item_obj) = list_item.downcast_ref::<ListItem>() else {
            return;
        };
        let li = list_item_obj.clone();
        let queue_li = factory_queue.clone();
        let store_li = factory_store.clone();

        let container = Box::builder()
            .orientation(Horizontal)
            .spacing(6)
            .margin_top(3)
            .margin_bottom(3)
            .margin_start(6)
            .margin_end(6)
            .build();

        let handle = Button::builder()
            .icon_name("list-drag-handle-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Drag to reorder")
            .can_focus(true)
            .build();
        handle.update_property(&[PropertyLabel("Drag handle")]);

        let drag = DragSource::builder().actions(DragAction::MOVE).build();
        let li_drag = li.clone();
        drag.connect_prepare(move |_, _, _| {
            let pos = li_drag.position();
            let pos_i32 = i32::try_from(pos).unwrap_or(0);
            let value = pos_i32.to_value();
            Some(ContentProvider::for_value(&value))
        });
        handle.add_controller(drag);

        let label = Label::builder()
            .ellipsize(End)
            .max_width_chars(25)
            .halign(Start)
            .hexpand(true)
            .build();
        label.update_property(&[PropertyLabel("Track name in queue")]);

        let remove = Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Remove from queue")
            .can_focus(true)
            .build();
        remove.update_property(&[PropertyLabel("Remove from queue")]);

        let li_remove = li.clone();
        let queue_remove = queue_li.clone();
        let store_remove = store_li.clone();
        remove.connect_clicked(move |_| {
            let pos = li_remove.position() as usize;
            try_remove_entry(&queue_remove, &store_remove, pos);
        });

        let drop = DropTarget::new(Type::I32, DragAction::MOVE);
        let li_drop = li;
        let queue_drop = queue_li;
        let store_drop = store_li;
        drop.connect_drop(move |_, value, _, _| {
            let to_pos = li_drop.position() as usize;
            handle_drop_value(value, &queue_drop, &store_drop, to_pos);
            true
        });

        container.append(&handle);
        container.append(&label);
        container.append(&remove);
        container.add_controller(drop);

        list_item_obj.set_child(Some(&container));
    });

    factory.connect_bind(|_, list_item| {
        let Some(list_item) = list_item.downcast_ref::<ListItem>() else {
            return;
        };
        bind_row(list_item);
    });

    factory
}

/// Bind data to a row widget (called when item data changes).
fn bind_row(list_item: &ListItem) {
    let Some(item) = list_item.item() else {
        return;
    };
    let Some(boxed) = item.downcast_ref::<BoxedAnyObject>() else {
        return;
    };
    let data = boxed.borrow::<QueueItemData>();

    let Some(child) = list_item.child() else {
        return;
    };
    let Some(container) = child.downcast_ref::<Box>() else {
        return;
    };
    let Some(handle) = container.first_child() else {
        return;
    };
    let Some(label_widget) = handle.next_sibling() else {
        return;
    };
    let Some(label) = label_widget.downcast_ref::<Label>() else {
        return;
    };

    label.set_label(&data.name);

    let mut classes: Vec<&str> = vec![];
    if data.is_current {
        classes.push("heading");
    }
    label.set_css_classes(&classes);
}

#[cfg(test)]
mod tests {
    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gio::{ListStore, prelude::ListModelExt},
            glib::{BoxedAnyObject, prelude::StaticType},
            gtk::{self, test},
        },
    };

    use crate::{
        playback::queue::PlaybackQueue,
        ui::player::{queue::populate_store, queue_row::try_remove_entry},
    };

    fn make_store() -> ListStore {
        ListStore::builder()
            .item_type(BoxedAnyObject::static_type())
            .build()
    }

    fn queue_and_store() -> (PlaybackQueue, ListStore) {
        let queue = PlaybackQueue::new();
        queue.set_queue(vec![10, 20]);
        let store = make_store();
        let names = vec![(10, "Alpha".to_string()), (20, "Beta".to_string())];
        populate_store(&store, &queue, &names);
        (queue, store)
    }

    #[test]
    fn try_remove_entry_removes_in_bounds() -> Result<()> {
        let (queue, store) = queue_and_store();
        try_remove_entry(&queue, &store, 0);
        ensure!(store.n_items() == 1);
        ensure!(queue.len() == 1);
        ensure!(queue.current() == Some(20));
        Ok(())
    }

    #[test]
    fn try_remove_entry_out_of_bounds_is_noop() -> Result<()> {
        let (queue, store) = queue_and_store();
        try_remove_entry(&queue, &store, 5);
        ensure!(store.n_items() == 2);
        ensure!(queue.len() == 2);
        Ok(())
    }
}
