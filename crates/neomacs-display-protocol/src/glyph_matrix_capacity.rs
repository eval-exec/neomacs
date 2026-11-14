//! GNU window-system glyph-matrix row capacity, independent of visible rows.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Persistent scale belonging to one logical window identity.
///
/// Configuration snapshots and restored copies share this numeric handle,
/// just as GNU configurations retain the original window object. A newly
/// created or split window receives a fresh default handle instead.
#[derive(Clone, Debug, Default)]
pub struct GlyphMatrixRowScale {
    // Batch and TTY windows keep an allocation-free dormant scale of one.
    value: Option<Arc<AtomicU32>>,
}

impl GlyphMatrixRowScale {
    /// Activate a graphical window before any configurations can clone it.
    /// Repeated installation preserves the same logical window's learned scale.
    pub fn activate(&mut self) {
        if self.value.is_none() {
            self.value = Some(Arc::new(AtomicU32::new(1)));
        }
    }

    /// A new window has independent history, preserving the parent's domain.
    pub fn fresh_window(&self) -> Self {
        let mut window = Self::default();
        if self.value.is_some() {
            window.activate();
        }
        window
    }

    pub fn current(&self) -> NonZeroU32 {
        let Some(value) = &self.value else {
            return NonZeroU32::MIN;
        };
        // Initialization and checked growth are the only writers; neither
        // can publish zero. No other memory is synchronized by this scalar.
        NonZeroU32::new(value.load(Ordering::Relaxed))
            .expect("glyph matrix scale is always positive")
    }

    /// Grow once only for the exact scale whose matrix actually overflowed.
    /// A stale observation or an exhausted integer must recollect instead.
    pub fn grow_if(&self, expected: NonZeroU32) -> Option<NonZeroU32> {
        let value = self.value.as_ref()?;
        let next = NonZeroU32::new(expected.get().checked_add(1)?)?;
        value
            .compare_exchange(
                expected.get(),
                next.get(),
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .ok()
            .map(|_| next)
    }
}

/// Total rows reserved by GNU 31.1 `required_matrix_height`
/// (`src/dispnew.c:1759` at a360712c9d272d950d8d8255ef74570f7e90b7d9).
///
/// The two partial-row slots and three chrome slots are outside the window's
/// persistent scale. This is matrix capacity, not a viewport or source budget.
pub fn required_gui_glyph_matrix_rows(
    body_height_px: i64,
    vscroll_px: i32,
    smallest_opened_font_height: u32,
    row_scale: NonZeroU32,
) -> usize {
    let body_height_px = body_height_px.max(0) as u64;
    let window_pixel_height = body_height_px + u64::from(vscroll_px.unsigned_abs());
    let font_height = u64::from(smallest_opened_font_height.max(1));
    let font_rows = window_pixel_height.div_ceil(font_height);
    let rows = u128::from(font_rows) * u128::from(row_scale.get()) + 5;
    usize::try_from(rows).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_cloned_tty_window_scales_remain_dormant() {
        let window = GlyphMatrixRowScale::default();
        let clone = window.clone();
        let sibling = window.fresh_window();
        assert!(window.value.is_none());
        assert!(clone.value.is_none());
        assert!(sibling.value.is_none());
        assert_eq!(window.current(), NonZeroU32::MIN);
        assert_eq!(window.grow_if(NonZeroU32::MIN), None);
        assert!(window.value.is_none());
    }

    #[test]
    fn explicit_gui_activation_preserves_shared_scale_and_is_idempotent() {
        let mut window = GlyphMatrixRowScale::default();
        window.activate();
        let clone = window.clone();
        let two = NonZeroU32::new(2).unwrap();
        assert_eq!(clone.grow_if(NonZeroU32::MIN), Some(two));
        window.activate();
        assert_eq!(window.current(), two);
        assert!(Arc::ptr_eq(
            window.value.as_ref().unwrap(),
            clone.value.as_ref().unwrap()
        ));
        let sibling = window.fresh_window();
        assert_eq!(sibling.current(), NonZeroU32::MIN);
        assert!(!Arc::ptr_eq(
            window.value.as_ref().unwrap(),
            sibling.value.as_ref().unwrap()
        ));
    }

    #[test]
    fn window_scale_survives_configuration_clone_and_original_window_drop() {
        let mut live = GlyphMatrixRowScale::default();
        live.activate();
        let saved_configuration = live.clone();
        let restored = saved_configuration.clone();
        let two = NonZeroU32::new(2).unwrap();
        let three = NonZeroU32::new(3).unwrap();
        assert_eq!(live.grow_if(NonZeroU32::MIN), Some(two));
        drop(live);
        assert_eq!(saved_configuration.current(), two);
        assert_eq!(restored.grow_if(two), Some(three));
        drop(saved_configuration);
        assert_eq!(restored.current(), three);
    }

    #[test]
    fn a_new_window_scale_is_independent_and_stale_growth_is_refused() {
        let mut original = GlyphMatrixRowScale::default();
        original.activate();
        let sibling = original.fresh_window();
        let two = NonZeroU32::new(2).unwrap();
        assert_eq!(original.grow_if(NonZeroU32::MIN), Some(two));
        assert_eq!(original.grow_if(NonZeroU32::MIN), None);
        assert_eq!(original.current(), two);
        assert_eq!(sibling.current(), NonZeroU32::MIN);
    }

    #[test]
    fn gui_matrix_capacity_includes_partial_rows_and_chrome_reserve() {
        assert_eq!(
            required_gui_glyph_matrix_rows(758, 0, 20, NonZeroU32::MIN),
            43
        );
        assert_eq!(
            required_gui_glyph_matrix_rows(100, 0, 20, NonZeroU32::MIN),
            10
        );
    }

    #[test]
    fn gui_matrix_capacity_scales_font_rows_before_chrome_and_uses_absolute_vscroll() {
        let scale = NonZeroU32::new(2).unwrap();
        assert_eq!(required_gui_glyph_matrix_rows(100, -10, 20, scale), 17);
        assert_eq!(required_gui_glyph_matrix_rows(100, 10, 20, scale), 17);
    }

    #[test]
    fn gui_matrix_capacity_keeps_reserve_for_empty_body_and_clamps_font_height() {
        assert_eq!(required_gui_glyph_matrix_rows(0, 0, 0, NonZeroU32::MIN), 5);
        assert_eq!(required_gui_glyph_matrix_rows(1, 0, 0, NonZeroU32::MIN), 6);
    }
}
