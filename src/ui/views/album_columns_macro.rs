//! Album column macro for creating simple text columns.

/// Macro to create simple text columns with boilerplate reduction.
#[macro_export]
macro_rules! label_column {
    ($title:expr, $extractor:expr, $resizable:expr, $fixed_width:expr) => {{
        let factory = SignalListItemFactory::new();
        let extractor_fn = $extractor;

        factory.connect_setup(|_, list_item| {
            let label = Label::builder().ellipsize(End).xalign(0.0).build();
            if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
                list_item.set_child(Some(&label));
            }
        });

        factory.connect_bind(move |_, list_item| {
            if let Some(list_item) = list_item.downcast_ref::<ListItem>() {
                if let Some(child) = list_item.child() {
                    if let Some(label) = child.downcast_ref::<Label>() {
                        if let Some(boxed) = list_item.item() {
                            if let Ok(album_obj) = boxed.downcast::<BoxedAnyObject>() {
                                let album = album_obj.borrow::<Album>();
                                if let Some(text) = (extractor_fn)(&album) {
                                    label.set_text(&text);
                                    label.set_visible(true);
                                } else {
                                    label.set_visible(false);
                                }
                            }
                        }
                    }
                }
            }
        });

        let column = ColumnViewColumn::new(Some($title), Some(factory.upcast::<ListItemFactory>()));

        if $resizable {
            column.set_resizable(true);
        }

        if let Some(width) = $fixed_width {
            column.set_fixed_width(width);
            column.set_expand(false);
        } else {
            column.set_expand(true);
        }

        column
    }};
}
