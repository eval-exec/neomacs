//! GNU window-system glyph-matrix row capacity, independent of visible rows.

use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, MutexGuard};

/// Coherent numeric inputs copied from one window's allocated desired matrix.
/// The current display minimum is separate: opening a font does not allocate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphMatrixInput {
    row_scale: NonZeroU32,
    allocated_rows: Option<usize>,
}

impl GlyphMatrixInput {
    pub fn row_scale(self) -> NonZeroU32 {
        self.row_scale
    }

    pub fn allocated_rows(self) -> Option<usize> {
        self.allocated_rows
    }
}

#[derive(Debug)]
struct GlyphMatrixAllocation {
    input: GlyphMatrixInput,
    adjusted_font_height: Option<u32>,
    adjusted_scale: NonZeroU32,
}

/// Persistent desired-matrix allocation belonging to one window identity.
///
/// Configuration snapshots retain the same owner; a newly split window starts
/// independently. Batch/TTY windows remain dormant without a heap allocation.
/// The private mutex keeps rows and their scale coherent across a shared
/// configuration owner. It never encloses Lisp, font opening or another lock.
#[derive(Clone, Debug, Default)]
pub struct GlyphMatrixRowScale {
    value: Option<Arc<Mutex<GlyphMatrixAllocation>>>,
}

impl GlyphMatrixRowScale {
    fn allocation(&self) -> Option<MutexGuard<'_, GlyphMatrixAllocation>> {
        self.value.as_ref().map(|value| {
            value
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        })
    }

    pub fn activate(&mut self) {
        if self.value.is_none() {
            self.value = Some(Arc::new(Mutex::new(GlyphMatrixAllocation {
                input: GlyphMatrixInput {
                    row_scale: NonZeroU32::MIN,
                    allocated_rows: None,
                },
                adjusted_font_height: None,
                adjusted_scale: NonZeroU32::MIN,
            })));
        }
    }

    pub fn fresh_window(&self) -> Self {
        let mut window = Self::default();
        if self.value.is_some() {
            window.activate();
        }
        window
    }

    pub fn snapshot(&self) -> GlyphMatrixInput {
        self.allocation().map_or(
            GlyphMatrixInput {
                row_scale: NonZeroU32::MIN,
                allocated_rows: None,
            },
            |state| state.input,
        )
    }

    pub fn current(&self) -> NonZeroU32 {
        self.snapshot().row_scale()
    }

    pub fn allocated_rows(&self) -> Option<usize> {
        self.snapshot().allocated_rows()
    }

    /// Font/overflow changes are consumed by actual redisplay before hooks.
    /// Geometry alone is deliberately excluded: GNU vscroll contraction does
    /// not immediately readjust the allocated matrix.
    pub fn needs_adjustment(&self, smallest_font_height: u32) -> bool {
        self.allocation().is_some_and(|state| {
            state.input.allocated_rows.is_none()
                || state.adjusted_font_height != Some(smallest_font_height.max(1))
                || state.adjusted_scale != state.input.row_scale
        })
    }

    /// GNU adjust_frame_glyphs boundary, after committed geometry is available.
    /// This may shrink logical nrows; physical row storage is a separate owner.
    pub fn adjust(
        &self,
        body_height_px: i64,
        vscroll_px: i32,
        smallest_font_height: u32,
    ) -> Option<GlyphMatrixInput> {
        let mut state = self.allocation()?;
        state.input.allocated_rows = Some(required_gui_glyph_matrix_rows(
            body_height_px,
            vscroll_px,
            smallest_font_height,
            state.input.row_scale,
        ));
        state.adjusted_font_height = Some(smallest_font_height.max(1));
        state.adjusted_scale = state.input.row_scale;
        Some(state.input)
    }

    /// Grow only the exact scale whose allocated matrix really overflowed.
    /// Allocation remains pending until the existing frame retry readjusts.
    pub fn grow_if(&self, expected: NonZeroU32) -> Option<NonZeroU32> {
        let mut state = self.allocation()?;
        if state.input.row_scale != expected {
            return None;
        }
        let next = NonZeroU32::new(expected.get().checked_add(1)?)?;
        state.input.row_scale = next;
        Some(next)
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
    fn font_opening_is_pending_until_real_matrix_adjustment() {
        let mut window = GlyphMatrixRowScale::default();
        window.activate();
        assert!(window.needs_adjustment(20));
        let first = window.adjust(758, 0, 20).unwrap();
        assert_eq!(first.allocated_rows(), Some(43));
        assert!(!window.needs_adjustment(20));
        assert!(window.needs_adjustment(4));
        assert_eq!(window.allocated_rows(), Some(43));
        let adjusted = window.adjust(758, 0, 4).unwrap();
        assert_eq!(adjusted.allocated_rows(), Some(195));
        assert_ne!(first, adjusted);
        assert!(!window.needs_adjustment(4));
    }

    #[test]
    fn vscroll_contraction_stays_allocated_until_explicit_adjustment() {
        let mut window = GlyphMatrixRowScale::default();
        window.activate();
        window.adjust(100, -40, 20).unwrap();
        assert_eq!(window.allocated_rows(), Some(12));
        // Merely requesting redisplay with the same font/scale cannot shrink.
        assert!(!window.needs_adjustment(20));
        assert_eq!(window.allocated_rows(), Some(12));
        window.adjust(100, 0, 20).unwrap();
        assert_eq!(window.allocated_rows(), Some(10));
    }

    #[test]
    fn configuration_shares_allocated_rows_but_a_split_window_starts_unallocated() {
        let mut window = GlyphMatrixRowScale::default();
        window.activate();
        window.adjust(100, 0, 20).unwrap();
        let configuration = window.clone();
        let sibling = window.fresh_window();
        let before = configuration.snapshot();
        window.grow_if(NonZeroU32::MIN).unwrap();
        assert!(configuration.needs_adjustment(20));
        assert_eq!(configuration.allocated_rows(), Some(10));
        window.adjust(100, 0, 20).unwrap();
        assert_eq!(configuration.allocated_rows(), Some(15));
        assert_ne!(configuration.snapshot(), before);
        assert_eq!(sibling.allocated_rows(), None);
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
