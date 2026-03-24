//! UI helper macros for common widget patterns.

/// Macro to update widget visibility based on item count.
///
/// Hides the widget and returns early if count is 0, otherwise shows it.
/// This eliminates the duplicated hide/show pattern used in count label updates.
#[macro_export]
macro_rules! update_visibility_by_count {
    ($count:expr, $widget:expr) => {
        if $count == 0 {
            $widget.set_visible(false);
            return;
        }

        $widget.set_visible(true);
    };
}
