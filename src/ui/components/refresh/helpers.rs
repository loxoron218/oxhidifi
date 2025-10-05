use std::{cell::Cell, rc::Rc, thread_local};

use gtk4::Window;
use libadwaita::prelude::WidgetExt;

use crate::ui::{
    components::{
        refresh::RefreshService,
        view_controls::{
            list_view::population::populate_albums_column_view, view_mode::ViewMode::ListView,
        },
    },
    grids::{
        album_grid_population::populate_albums_grid,
        album_grid_rebuilder::rebuild_albums_grid_for_window,
        album_grid_state::AlbumGridState::Empty, artist_grid_population::populate_artist_grid,
    },
    search::clear_grid,
};

impl RefreshService {
    /// A new helper function specifically for the albums tab
    pub(crate) async fn repopulate_albums_tab(&self) {
        if let (Some(grid), Some(stack)) = (
            self.albums_grid_cell.borrow().as_ref(),
            self.albums_stack_cell.borrow().as_ref(),
        ) {
            clear_grid(grid);
            self.set_inner_stack_state(stack, self.scanning_label_albums.is_visible());
            populate_albums_grid(
                grid,
                self.db_pool.clone(),
                self.sort_ascending.get(),
                Rc::clone(&self.sort_orders),
                &self.screen_info,
                stack,
                &self.album_count_label,
                self.show_dr_badges.clone(),
                self.use_original_year.clone(),
                self.player_bar.clone(),
                self.current_zoom_level
                    .as_ref()
                    .map(|zoom| zoom.get())
                    .unwrap_or_default(),
                self.image_loader.clone(),
            )
            .await;
        } else {
            // If we don't have the grid or stack, but we have a stack, set it to a default state
            // to avoid leaving the UI in a loading state indefinitely
            if let Some(stack) = self.albums_stack_cell.borrow().as_ref() {
                // Set to empty state as a fallback
                stack.set_visible_child_name(Empty.as_str());
            }
        }
    }

    /// A new helper function specifically for the artists tab
    pub(crate) async fn repopulate_artists_tab(&self) {
        if let (Some(grid), Some(stack)) = (
            self.artist_grid_cell.borrow().as_ref(),
            self.artists_stack_cell.borrow().as_ref(),
        ) {
            clear_grid(grid);
            self.set_inner_stack_state(stack, self.scanning_label_artists.is_visible());
            populate_artist_grid(
                grid,
                self.db_pool.clone(),
                self.sort_ascending_artists.get(),
                &self.stack,
                &self.left_btn_stack,
                &self.right_btn_box,
                &self.screen_info,
                self.sender.clone(),
                self.nav_history.clone(),
                stack,
                self.artist_count_label.clone(),
                self.show_dr_badges.clone(),
                self.use_original_year.clone(),
                self.player_bar.clone(),
                self.current_zoom_level
                    .as_ref()
                    .map(|zoom| zoom.get())
                    .unwrap_or_default(),
            );
        }
    }

    /// A new helper function specifically for repopulating the ColumnView in ListView mode
    pub(crate) async fn repopulate_column_view(&self, window: &Window) {
        // Get the scanning label from the albums stack if it exists
        let scanning_label = if self.albums_stack_cell.borrow().as_ref().is_some() {
            // Check if scanning label is visible
            self.scanning_label_albums.is_visible()
        } else {
            false
        };

        // Check if the "Show DR Value Badges" setting has changed
        let current_show_dr_badges = self.show_dr_badges.get();
        let previous_show_dr_badges = self.previous_show_dr_badges.get();

        // Check if the "Use Original Year" setting has changed
        // We need to track the previous state of this setting as well
        thread_local! {
            // Default to true to force initial population
            static PREVIOUS_USE_ORIGINAL_YEAR: Cell<bool> = const { Cell::new(true) };
        }
        let current_use_original_year = self.use_original_year.get();
        let previous_use_original_year = PREVIOUS_USE_ORIGINAL_YEAR.with(|cell| cell.get());
        if current_show_dr_badges != previous_show_dr_badges
            || current_use_original_year != previous_use_original_year
        {
            // Update the previous states
            self.previous_show_dr_badges.set(current_show_dr_badges);
            PREVIOUS_USE_ORIGINAL_YEAR.with(|cell| cell.set(current_use_original_year));

            // Rebuild the albums grid with ListView mode
            let model = rebuild_albums_grid_for_window(
                &self.stack,
                &self.scanning_label_albums,
                &self.screen_info,
                &self.albums_grid_cell,
                &self.albums_stack_cell,
                window,
                &self.db_pool,
                &self.sender,
                self.album_count_label.clone(),
                ListView,
                current_use_original_year,
                self.show_dr_badges.clone(),
                Some(Rc::new(self.clone())),
                None,
                self.image_loader.clone(),
            );

            // Set the ColumnView model in the RefreshService
            self.set_column_view_model(model.clone());

            // If we have a model, populate the column view with data
            if let Some(model) = model {
                // Get the albums stack to pass to the population function
                if let Some(albums_stack) = self.albums_stack_cell.borrow().as_ref() {
                    let albums_stack_clone = albums_stack.clone();

                    // Set the inner stack state based on scanning visibility
                    self.set_inner_stack_state(&albums_stack_clone, scanning_label);

                    // Repopulate the ColumnView with updated data
                    populate_albums_column_view(
                        &model,
                        self.db_pool.clone(),
                        self.sort_ascending.get(),
                        Rc::clone(&self.sort_orders),
                        &albums_stack_clone,
                        &self.album_count_label,
                        self.use_original_year.clone(),
                        self.player_bar.clone(),
                    )
                    .await;
                }
            }
        } else {
            // For refresh operations, we should use the existing model, not rebuild the grid
            if let Some(model) = self.column_view_model.borrow().as_ref() {
                // Get the albums stack to pass to the population function
                if let Some(albums_stack) = self.albums_stack_cell.borrow().as_ref() {
                    let albums_stack_clone = albums_stack.clone();

                    // Set the inner stack state based on scanning visibility
                    self.set_inner_stack_state(&albums_stack_clone, scanning_label);

                    // Repopulate the ColumnView with updated data
                    populate_albums_column_view(
                        model,
                        self.db_pool.clone(),
                        self.sort_ascending.get(),
                        Rc::clone(&self.sort_orders),
                        &albums_stack_clone,
                        &self.album_count_label,
                        self.use_original_year.clone(),
                        self.player_bar.clone(),
                    )
                    .await;
                }
            } else {
                // If we don't have a model but we have a stack, set it to a default state
                // to avoid leaving the UI in a loading state indefinitely
                if let Some(stack) = self.albums_stack_cell.borrow().as_ref() {
                    // Set to empty state as a fallback
                    stack.set_visible_child_name(Empty.as_str());
                }
            }
        }
    }
}
