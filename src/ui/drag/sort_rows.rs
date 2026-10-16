//! Sort-list row construction and drop reordering for [`SortListBuilder`].
//!
//! Implements the row builders behind [`crate::ui::drag::SortListBuilder`];
//! shared traits and wrappers stay in the parent [`crate::ui::drag`] module.

use std::{collections::HashMap, sync::Arc};

use {
    libadwaita::{
        glib::Value,
        gtk::{
            Align::{Center, Start},
            Box, Button, DragSource, DropTarget, Image, Label, ListBox, ListBoxRow,
            Orientation::Horizontal,
            SelectionMode::None,
            accessible::Property::Label as PropertyLabel,
            gdk::{ContentProvider, DragAction},
        },
        prelude::{
            AccessibleExtManual, BoxExt, ButtonExt, ListBoxRowExt, StaticType, ToValue, WidgetExt,
        },
    },
    parking_lot::Mutex,
};

use crate::{
    storage::sort_rules::SortOrder::{self, Ascending, Descending},
    ui::{
        drag::{SortListBounds, SortListBuilder, adjust_insert_index},
        signal_handlers::UiHandles,
    },
};

impl<T, F, U> SortListBuilder<T, F, U>
where
    T: SortListBounds,
    T::Criteria: 'static,
    F: Fn(&ListBox, &HashMap<T::Criteria, SortOrder>) -> Vec<T> + Clone + 'static,
    U: Fn(Vec<T>) + Clone + 'static,
{
    /// Add a single sort row to the list box.
    fn add_row(&self, item: &T) {
        let row = ListBoxRow::builder()
            .css_classes(["sort-row"])
            .tooltip_text("Sort row")
            .build();
        row.update_property(&[PropertyLabel(&format!(
            "Sort row {}",
            item.criteria().to_string()
        ))]);
        row.set_widget_name(&format!("sort:{}", item.discriminator()));

        let row_box = Box::builder()
            .orientation(Horizontal)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .css_classes(["sort-row-box"])
            .tooltip_text("Sort row container")
            .build();

        let drag_handle = Image::builder()
            .icon_name("list-drag-handle-symbolic")
            .css_classes(["dim-label"])
            .tooltip_text("Drag to reorder")
            .build();
        drag_handle.update_property(&[PropertyLabel("Drag handle")]);
        row_box.append(&drag_handle);

        let label = Label::builder()
            .label(item.criteria().to_string())
            .halign(Start)
            .hexpand(true)
            .css_classes(["sort-label"])
            .tooltip_text("Sort criteria")
            .build();
        label.update_property(&[PropertyLabel(&item.criteria().to_string())]);
        row_box.append(&label);

        let toggle = Button::builder()
            .icon_name(if item.order() == &Ascending {
                "pan-up-symbolic"
            } else {
                "pan-down-symbolic"
            })
            .css_classes(["flat", "circular", "image-button"])
            .valign(Center)
            .tooltip_text("Toggle sort order")
            .build();
        toggle.update_property(&[PropertyLabel("Toggle sort order")]);

        let criteria_clone = item.criteria().clone();
        let toggle_clone = toggle.clone();
        let lb_clone = self.list_box.clone();
        let om_clone = Arc::clone(&self.order_map);
        let reconstruct_click = self.reconstruct.clone();
        let update_click = self.update.clone();

        let mut handles = UiHandles::default();
        handles.retain_signal(toggle.connect_clicked(move |_| {
            let mut om = om_clone.lock();
            let new_order = match om.get(&criteria_clone).unwrap_or(&Ascending) {
                Ascending => Descending,
                Descending => Ascending,
            };
            _ = om.insert(criteria_clone.clone(), new_order);
            toggle_clone.set_icon_name(match new_order {
                Ascending => "pan-up-symbolic",
                Descending => "pan-down-symbolic",
            });

            let new_sort = reconstruct_click(&lb_clone, &om);
            update_click(new_sort);
            drop(om);
        }));

        row_box.append(&toggle);
        row.set_child(Some(&row_box));

        let drag_source = DragSource::new();
        drag_source.set_actions(DragAction::MOVE);
        let row_clone = row.clone();
        handles.retain_signal(drag_source.connect_prepare(move |_, _, _| {
            Some(ContentProvider::for_value(&row_clone.to_value()))
        }));
        row.add_controller(drag_source);

        let drop_target = DropTarget::new(ListBoxRow::static_type(), DragAction::MOVE);
        drop_target.set_preload(true);
        let lb_clone2 = self.list_box.clone();
        let target_row_clone = row.clone();
        let om_clone2 = Arc::clone(&self.order_map);
        let reconstruct_drop = self.reconstruct.clone();
        let update_drop = self.update.clone();

        let lb = lb_clone2;
        let target = target_row_clone;
        let om = om_clone2;
        let reconstruct = reconstruct_drop;
        let update = update_drop;
        handles.retain_signal(drop_target.connect_drop(move |_, value, _, _| {
            Self::on_sort_row_drop(value, &lb, target.index(), &om, &reconstruct, &update)
        }));
        row.add_controller(drop_target);

        self.list_box.append(&row);
    }

    /// Handle a drop event for reordering sort rows.
    fn on_sort_row_drop(
        value: &Value,
        lb: &ListBox,
        target_idx: i32,
        om: &Mutex<HashMap<T::Criteria, SortOrder>>,
        reconstruct: &F,
        update: &U,
    ) -> bool
    where
        F: Fn(&ListBox, &HashMap<T::Criteria, SortOrder>) -> Vec<T>,
        U: Fn(Vec<T>),
    {
        let Ok(row) = value.get::<ListBoxRow>() else {
            return true;
        };
        let source_idx = row.index();
        if source_idx == target_idx {
            return true;
        }
        lb.remove(&row);
        lb.insert(&row, adjust_insert_index(source_idx, target_idx));
        let om_guard = om.lock();
        let new_sort = reconstruct(lb, &om_guard);
        update(new_sort);
        drop(om_guard);
        true
    }

    /// Build a sort‑configuration list box from the given items.
    ///
    /// # Arguments
    ///
    /// * `sort_items` - The sort items to render as rows
    /// * `reconstruct` - Callback that rebuilds the sort vector from box order
    /// * `update` - Callback that persists a new sort vector
    pub fn build(sort_items: &[T], reconstruct: F, update: U) -> ListBox {
        let list_box = ListBox::builder()
            .selection_mode(None)
            .css_classes(["boxed-list", "sort-list"])
            .tooltip_text("Sort configuration list")
            .build();
        list_box.update_property(&[PropertyLabel("Sort configuration list")]);

        let order_map: Arc<Mutex<HashMap<T::Criteria, SortOrder>>> =
            Arc::new(Mutex::new(HashMap::with_capacity(sort_items.len())));
        for item in sort_items {
            let mut om = order_map.lock();
            _ = om.insert(item.criteria().clone(), *item.order());
        }

        let builder = Self {
            list_box,
            order_map,
            reconstruct,
            update,
        };
        for item in sort_items {
            builder.add_row(item);
        }

        builder.list_box
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::{Display, Formatter, Result as FmtResult};

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            glib::object::Cast,
            gtk::{self, ListBoxRow, Widget, test},
            prelude::WidgetExt,
        },
    };

    use crate::{
        app::mocks::find_descendants,
        storage::sort_rules::SortOrder::{self, Ascending, Descending},
        ui::drag::{SortItem, SortListBuilder},
    };

    #[derive(Clone, PartialEq, Eq, Hash)]
    enum TestCriteria {
        Title,
        Year,
    }

    impl Display for TestCriteria {
        fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
            formatter.write_str(match self {
                Self::Title => "Title",
                Self::Year => "Year",
            })
        }
    }

    #[derive(Clone)]
    struct TestSortItem {
        criteria: TestCriteria,
        order: SortOrder,
    }

    impl SortItem for TestSortItem {
        type Criteria = TestCriteria;

        fn criteria(&self) -> &Self::Criteria {
            &self.criteria
        }

        fn order(&self) -> &SortOrder {
            &self.order
        }

        fn new(criteria: Self::Criteria, order: SortOrder) -> Self {
            Self { criteria, order }
        }

        fn from_discriminator(discriminator: u8) -> Option<Self::Criteria> {
            match discriminator {
                0 => Some(TestCriteria::Title),
                1 => Some(TestCriteria::Year),
                _ => None,
            }
        }

        fn entity_name() -> &'static str {
            "test sort item"
        }

        fn discriminator(&self) -> u8 {
            match self.criteria {
                TestCriteria::Title => 0,
                TestCriteria::Year => 1,
            }
        }
    }

    #[test]
    fn sort_rows_keep_keyboard_focus() -> Result<()> {
        let items = vec![
            TestSortItem::new(TestCriteria::Title, Ascending),
            TestSortItem::new(TestCriteria::Year, Descending),
        ];
        let list = SortListBuilder::build(&items, |_, _| Vec::new(), |_| {});
        let root: Widget = list.upcast();
        let rows = find_descendants::<ListBoxRow>(&root);
        ensure!(rows.len() == 2, "sort list must own one row per item");
        for row in &rows {
            ensure!(
                row.can_focus(),
                "sort rows must stay focusable for keyboard navigation"
            );
        }
        Ok(())
    }
}
