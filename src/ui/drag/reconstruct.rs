//! Sort-list reconstruction from `ListBox` row order.

use std::{collections::HashMap, hash::BuildHasher};

use {
    libadwaita::{
        gtk::{ListBox, ListBoxRow},
        prelude::{Cast, WidgetExt},
    },
    tracing::{error, warn},
};

use crate::storage::sort_rules::{
    AlbumSortCriteria, AlbumSortItem, ArtistSortCriteria, ArtistSortItem, SortOrder,
};

use crate::ui::drag::SortListBounds;

/// Parse a `"sort:N"` widget name into a sort item and push to `new_sort`.
fn process_row<T: SortListBounds, S: BuildHasher>(
    row: &ListBoxRow,
    order_map: &HashMap<T::Criteria, SortOrder, S>,
    new_sort: &mut Vec<T>,
) where
    T::Criteria: 'static,
{
    let name = row.widget_name();
    let Some(suffix) = name.strip_prefix("sort:") else {
        warn!(name = %name, "Invalid sort row name");
        return;
    };
    let disc = match suffix.parse::<u8>() {
        Ok(d) => d,
        Err(e) => {
            error!(error = %e, suffix = %suffix, "Failed to parse sort discriminator");
            return;
        }
    };
    let Some(criteria) = T::from_discriminator(disc) else {
        warn!(entity = %T::entity_name(), disc = %disc, "Unknown sort discriminator");
        return;
    };
    let order = order_map
        .get(&criteria)
        .copied()
        .unwrap_or_else(SortOrder::default);
    new_sort.push(T::new(criteria, order));
}

/// Reconstruct sort items from a `ListBox`'s current order using a processing callback.
fn reconstruct_sort_list<C, S, I>(
    list_box: &ListBox,
    order_map: &HashMap<C, SortOrder, S>,
    mut process: impl FnMut(&ListBoxRow, &HashMap<C, SortOrder, S>, &mut Vec<I>),
) -> Vec<I>
where
    S: BuildHasher,
{
    let mut result = Vec::new();
    let mut child = list_box.first_child();
    while let Some(w) = &child {
        if let Ok(row) = w.clone().downcast::<ListBoxRow>() {
            process(&row, order_map, &mut result);
        }
        child = w.next_sibling();
    }
    result
}

/// Reconstructs album sort items from a `ListBox`'s current order.
pub fn reconstruct_albums_sort<S: BuildHasher>(
    list_box: &ListBox,
    order_map: &HashMap<AlbumSortCriteria, SortOrder, S>,
) -> Vec<AlbumSortItem> {
    reconstruct_sort_list(list_box, order_map, process_row::<AlbumSortItem, S>)
}

/// Reconstructs artist sort items from a `ListBox`'s current order.
pub fn reconstruct_artists_sort<S: BuildHasher>(
    list_box: &ListBox,
    order_map: &HashMap<ArtistSortCriteria, SortOrder, S>,
) -> Vec<ArtistSortItem> {
    reconstruct_sort_list(list_box, order_map, process_row::<ArtistSortItem, S>)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fmt::Debug};

    use {
        anyhow::{Result, ensure},
        libadwaita::{
            gtk::{self, ListBox, ListBoxRow, test},
            prelude::WidgetExt,
        },
    };

    use crate::{
        storage::sort_rules::{
            AlbumSortCriteria::{Artist, BitDepth, Title, Year},
            ArtistSortCriteria::{AlbumCount, Name},
            SortOrder::{self, Ascending, Descending},
        },
        ui::drag::reconstruct::{reconstruct_albums_sort, reconstruct_artists_sort},
    };

    fn row_named(name: &str) -> ListBoxRow {
        let row = ListBoxRow::new();
        row.set_widget_name(name);
        row
    }

    fn albums_list_box(names: &[&str]) -> ListBox {
        let list_box = ListBox::new();
        for name in names {
            list_box.append(&row_named(name));
        }
        list_box
    }

    fn assert_reconstructed<I, C>(
        sort: &[I],
        expected_criteria: &[C],
        expected_orders: &[SortOrder],
        criteria_of: impl Fn(&I) -> C,
        order_of: impl Fn(&I) -> SortOrder,
    ) -> Result<()>
    where
        C: PartialEq + Debug,
    {
        let criteria: Vec<C> = sort.iter().map(criteria_of).collect();
        ensure!(
            criteria == expected_criteria,
            "reconstructed order must follow the box order"
        );
        let orders: Vec<SortOrder> = sort.iter().map(order_of).collect();
        ensure!(orders == expected_orders, "order must come from the map");
        Ok(())
    }

    #[test]
    fn reconstruct_albums_sort_preserves_box_order() -> Result<()> {
        let list_box = albums_list_box(&["sort:0", "sort:4", "sort:2"]);
        let mut order_map = HashMap::new();
        order_map.insert(Title, Descending);
        order_map.insert(BitDepth, Ascending);
        order_map.insert(Year, Ascending);

        let sort = reconstruct_albums_sort(&list_box, &order_map);
        assert_reconstructed(
            &sort,
            &[Title, BitDepth, Year],
            &[Descending, Ascending, Ascending],
            |item| item.criteria.clone(),
            |item| item.order,
        )
    }

    #[test]
    fn reconstruct_artists_sort_preserves_box_order() -> Result<()> {
        let list_box = ListBox::new();
        list_box.append(&row_named("sort:1"));
        list_box.append(&row_named("sort:0"));
        let mut order_map = HashMap::new();
        order_map.insert(AlbumCount, Descending);
        order_map.insert(Name, Ascending);

        let sort = reconstruct_artists_sort(&list_box, &order_map);
        assert_reconstructed(
            &sort,
            &[AlbumCount, Name],
            &[Descending, Ascending],
            |item| item.criteria.clone(),
            |item| item.order,
        )
    }

    #[test]
    fn reconstruct_skips_invalid_and_unknown_row_names() -> Result<()> {
        let list_box = albums_list_box(&["not-a-sort-row", "sort:99", "sort:1"]);
        let order_map = HashMap::new();

        let sort = reconstruct_albums_sort(&list_box, &order_map);
        ensure!(
            sort.len() == 1,
            "only the valid known row must survive reconstruction"
        );
        ensure!(
            sort.first().is_some_and(|item| item.criteria == Artist),
            "the surviving row must decode to its criteria"
        );
        Ok(())
    }

    #[test]
    fn reconstruct_defaults_missing_order_to_ascending() -> Result<()> {
        let list_box = albums_list_box(&["sort:0"]);
        let order_map = HashMap::new();

        let sort = reconstruct_albums_sort(&list_box, &order_map);
        ensure!(sort.len() == 1, "a known criteria must be reconstructed");
        ensure!(
            sort.first().is_some_and(|item| item.order == Ascending),
            "a criteria missing from the order map must default to ascending"
        );
        Ok(())
    }
}
