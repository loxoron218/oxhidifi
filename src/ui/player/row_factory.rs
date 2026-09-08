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

use crate::{
    playback::queue_manager::PlaybackQueue,
    ui::{player::playlist::QueueItemData, signal_handlers::UiHandles},
};

/// Reorder an item within both the queue model and the `ListStore`.
fn reorder_entry(queue: &PlaybackQueue, store: &ListStore, from: usize, to: usize) {
    if from == to || from >= queue.len() || to >= queue.len() {
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
    let adjusted_pos = if to > from { to.saturating_sub(1) } else { to };
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
    if q.remove(pos).is_none() {
        warn!(pos, "Failed to remove track — position out of bounds");
        return;
    }
    if let Ok(index) = u32::try_from(pos)
        && index < store.n_items()
    {
        store.remove(index);
    }
}

/// Create the `SignalListItemFactory` that builds and binds queue rows.
#[must_use]
pub fn build_row_factory(queue: &PlaybackQueue, store: &ListStore) -> SignalListItemFactory {
    let factory = SignalListItemFactory::new();
    let factory_queue = queue.clone();
    let factory_store = store.clone();
    let mut handles = UiHandles::default();

    handles.retain_signal(factory.connect_setup(move |_, list_item| {
        let Some(list_item_obj) = list_item.downcast_ref::<ListItem>() else {
            return;
        };
        let li = list_item_obj.clone();
        let queue_li = factory_queue.clone();
        let store_li = factory_store.clone();
        let mut handles = UiHandles::default();

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
        handles.retain_signal(drag.connect_prepare(move |_, _, _| {
            let pos = li_drag.position();
            let pos_i32 = i32::try_from(pos).unwrap_or(0);
            let value = pos_i32.to_value();
            Some(ContentProvider::for_value(&value))
        }));
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
        handles.retain_signal(remove.connect_clicked(move |_| {
            let pos = usize::try_from(li_remove.position()).unwrap_or(0);
            try_remove_entry(&queue_remove, &store_remove, pos);
        }));

        let drop_target = DropTarget::new(Type::I32, DragAction::MOVE);
        let li_drop = li;
        let queue_drop = queue_li;
        let store_drop = store_li;
        handles.retain_signal(drop_target.connect_drop(move |_, value, _, _| {
            let to_pos = usize::try_from(li_drop.position()).unwrap_or(0);
            handle_drop_value(value, &queue_drop, &store_drop, to_pos);
            true
        }));

        container.append(&handle);
        container.append(&label);
        container.append(&remove);
        container.add_controller(drop_target);

        list_item_obj.set_child(Some(&container));
    }));

    handles.retain_signal(factory.connect_bind(|_, list_item| {
        let Some(list_item) = list_item.downcast_ref::<ListItem>() else {
            return;
        };
        bind_row(list_item);
    }));

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
        anyhow::{Result, anyhow, ensure},
        libadwaita::{
            gio::{ListStore, prelude::ListModelExt},
            glib::{BoxedAnyObject, MainContext, object::Cast, prelude::StaticType},
            gtk::{self, Label, ListView, NoSelection, Widget, Window, test},
            prelude::{GtkWindowExt, WidgetExt},
        },
    };

    use crate::{
        playback::queue_manager::PlaybackQueue,
        ui::player::{
            playlist::populate_store,
            row_factory::{build_row_factory, reorder_entry, try_remove_entry},
        },
    };

    fn collect_labels(widget: &Widget, out: &mut Vec<String>) {
        if let Some(label) = widget.downcast_ref::<Label>() {
            out.push(label.label().to_string());
        }
        let mut next = widget.first_child();
        while let Some(current) = next {
            collect_labels(&current, out);
            next = current.next_sibling();
        }
    }

    fn rows_ready(list_view: &ListView) -> bool {
        let mut labels = Vec::new();
        collect_labels(list_view.upcast_ref::<Widget>(), &mut labels);
        let joined = labels.join("\n");
        joined.contains("Alpha") && joined.contains("Beta")
    }

    fn pump_until_ready(list_view: &ListView) -> Result<()> {
        let mut attempts: usize = 0;
        while attempts < 200 && !rows_ready(list_view) {
            _ = MainContext::default().iteration(true);
            attempts = attempts.saturating_add(1);
        }
        ensure!(
            rows_ready(list_view),
            "rows were not populated after pumping the main context"
        );
        Ok(())
    }

    fn make_store() -> ListStore {
        ListStore::builder()
            .item_type(BoxedAnyObject::static_type())
            .build()
    }

    fn queue_and_store() -> Result<(PlaybackQueue, ListStore)> {
        let queue = PlaybackQueue::new();
        queue.set_queue(vec![10, 20]).map_err(|e| anyhow!("{e}"))?;
        let store = make_store();
        let names = vec![(10, "Alpha".to_string()), (20, "Beta".to_string())];
        populate_store(&store, &queue, &names);
        Ok((queue, store))
    }

    #[test]
    fn try_remove_entry_removes_in_bounds() -> Result<()> {
        let (queue, store) = queue_and_store()?;
        try_remove_entry(&queue, &store, 0);
        ensure!(store.n_items() == 1);
        ensure!(queue.len() == 1);
        ensure!(queue.current() == Some(20));
        Ok(())
    }

    #[test]
    fn try_remove_entry_out_of_bounds_is_noop() -> Result<()> {
        let (queue, store) = queue_and_store()?;
        try_remove_entry(&queue, &store, 5);
        ensure!(store.n_items() == 2);
        ensure!(queue.len() == 2);
        Ok(())
    }

    #[test]
    fn try_remove_entry_shorter_store_skips_oob_removal() -> Result<()> {
        let (queue, store) = queue_and_store()?;
        store.remove(0);
        ensure!(store.n_items() == 1);
        try_remove_entry(&queue, &store, 1);
        ensure!(store.n_items() == 1);
        ensure!(queue.len() == 1);
        Ok(())
    }

    #[test]
    fn reorder_entry_out_of_bounds_is_noop() -> Result<()> {
        let (queue, store) = queue_and_store()?;
        reorder_entry(&queue, &store, 5, 0);
        ensure!(queue.tracks() == vec![10, 20]);
        ensure!(store.n_items() == 2);
        Ok(())
    }

    #[test]
    fn reorder_entry_swaps_in_queue_and_store() -> Result<()> {
        let (queue, store) = queue_and_store()?;
        reorder_entry(&queue, &store, 0, 1);
        ensure!(queue.tracks() == vec![20, 10]);
        ensure!(store.n_items() == 2);
        Ok(())
    }

    #[test]
    fn row_factory_setup_bind_populates_rows() -> Result<()> {
        let (queue, store) = queue_and_store()?;
        let factory = build_row_factory(&queue, &store);
        let selection = NoSelection::new(Some(store));
        let list_view = ListView::new(Some(selection), Some(factory));
        let window = Window::new();
        window.set_child(Some(&list_view));
        window.present();
        pump_until_ready(&list_view)?;
        window.close();
        Ok(())
    }
}
