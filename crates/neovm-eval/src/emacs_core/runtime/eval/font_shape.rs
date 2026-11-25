//! Cold font-driver vtable contract owned by the evaluator layer
//! (`N-font-driver-vtable`, stage-4 W4).
//!
//! The display layer (the layout engine, which owns the font system) shapes
//! gstrings, reports glyph text extents, and looks up sealed realized faces;
//! the evaluator owns the gstring cache and one installed `FontDriver` slot.
//! Every method on [`FontDriver`] is required so a production driver cannot
//! silently fall back to an absent implementation if a new method is added.

use super::Context;
use neovm_edit::emacs_core::display_metrics::WindowId;
use neovm_value::display_vocab::{FrameId, GlyphTextMetrics, GstringShapeCacheKey};
use neovm_value::value::Value;

/// The result of a font-shaper driver over one gstring — GNU's
/// `font->driver->shape` return contract (src/font.c Ffont_shape_gstring).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GstringShapeOutcome {
    /// Driver shaped the gstring in place; the value is the number of glyphs
    /// produced (GNU's fixnum return).
    Shaped(i64),
    /// Driver needs a larger glyph vector (GNU grows the gstring and
    /// retries, at most three times).
    NeedLargerGlyphs,
    /// This driver cannot shape this font (GNU: `font->driver->shape` is
    /// NULL -> the value is nil, no composition).
    NotShapable,
}

/// Installed font driver vtable (`N-font-driver-vtable`, GNU `struct font_driver`).
///
/// One 16-byte trait-object slot on [`Context`] carries every method the
/// display/layout font system exposes to core primitives. All methods are
/// required so a production driver gets a compile error instead of a silent
/// fallback when a new method is added.
pub trait FontDriver {
    /// GNU `font->driver->shape` (`src/font.c` `Ffont_shape_gstring`).
    fn shape_gstring(
        &mut self,
        eval: &mut Context,
        gstring: Value,
        direction: Value,
    ) -> GstringShapeOutcome;

    /// GNU `font->driver->text_extents` (`src/font.c` `font_fill_lglyph_metrics`).
    fn glyph_text_metrics(&mut self, font: Value, ch: u32) -> Option<GlyphTextMetrics>;

    /// Lookup of the `(face_id, arena_epoch)` redisplay sealed for a buffer
    /// position (`font-at` and positioned `internal-char-font`).
    ///
    /// `None` means redisplay has not sealed a face for that position, or the
    /// layout engine is already borrowed.
    fn realized_fontset(
        &mut self,
        eval: &mut Context,
        frame_id: FrameId,
        window_id: WindowId,
        lisp_pos: i64,
    ) -> Option<(u32, u64)>;
}

// One fat-pointer niche slot occupies the former `font_shape_fn` position
// (offset 10648) without shifting any pinned Context or JIT offset.
static_assertions::const_assert_eq!(std::mem::size_of::<Option<Box<dyn FontDriver>>>(), 16);

/// Own the installed [`FontDriver`] while a method that needs `&mut Context`
/// executes, restoring the slot on normal return or unwind.
#[must_use]
struct FontDriverScope<'context> {
    context: &'context mut Context,
    driver: Option<Box<dyn FontDriver>>,
}

impl Drop for FontDriverScope<'_> {
    fn drop(&mut self) {
        self.context.font_driver = self.driver.take();
    }
}

static_assertions::assert_not_impl_any!(FontDriverScope<'static>: Send, Sync);

impl Context {
    /// Run `f` with exclusive access to both `self` and the installed
    /// [`FontDriver`], restoring the driver even when `f` returns early or
    /// unwinds. Returns `None` when no driver is installed or while an outer
    /// `with_font_driver` call (`shape_gstring` or `realized_fontset`) already
    /// holds the slot; any nested font-driver call (including
    /// `glyph_text_metrics` and `realized_fontset`) observes `None` until the
    /// outer call finishes. Production `REDISPLAY_RUNTIME` methods do not call
    /// back into Lisp or into one another.
    pub fn with_font_driver<R>(
        &mut self,
        f: impl FnOnce(&mut Context, &mut dyn FontDriver) -> R,
    ) -> Option<R> {
        let driver = self.font_driver.take()?;
        let mut scope = FontDriverScope {
            context: self,
            driver: Some(driver),
        };
        let driver = scope.driver.as_deref_mut()?;
        Some(f(scope.context, driver))
    }
}

/// The shaped-gstring cache owns its values strongly, like GNU's staticpro'd
/// gstring_hash_table (src/composite.c:2201-2202). Keys contain no Lisp edges.
pub(crate) fn trace_gstring_shape_cache(
    cache: &std::collections::HashMap<GstringShapeCacheKey, Value>,
    visit: &mut dyn FnMut(Value),
) {
    for value in cache.values().copied() {
        visit(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neovm_text::emacs_core::text_host::TextHost;
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;

    struct RecordingDriver {
        shape_calls: Rc<Cell<usize>>,
        metrics_calls: Rc<Cell<usize>>,
        realized_calls: Rc<Cell<usize>>,
        panic_on_shape: bool,
    }

    impl FontDriver for RecordingDriver {
        fn shape_gstring(
            &mut self,
            eval: &mut Context,
            _gstring: Value,
            _direction: Value,
        ) -> GstringShapeOutcome {
            self.shape_calls.set(self.shape_calls.get() + 1);
            assert!(
                eval.font_driver.is_none(),
                "slot must be taken while a &mut Context driver method runs"
            );
            assert!(
                eval.with_font_driver(|_, _| ()).is_none(),
                "nested with_font_driver must see None while the outer call holds the driver"
            );
            if self.panic_on_shape {
                panic!("simulated font driver panic");
            }
            GstringShapeOutcome::Shaped(1)
        }

        fn glyph_text_metrics(&mut self, _font: Value, ch: u32) -> Option<GlyphTextMetrics> {
            self.metrics_calls.set(self.metrics_calls.get() + 1);
            Some(GlyphTextMetrics {
                code: ch,
                width: 8,
                lbearing: 0,
                rbearing: 8,
                ascent: 12,
                descent: 3,
            })
        }

        fn realized_fontset(
            &mut self,
            _eval: &mut Context,
            frame_id: FrameId,
            window_id: WindowId,
            lisp_pos: i64,
        ) -> Option<(u32, u64)> {
            self.realized_calls.set(self.realized_calls.get() + 1);
            Some(((frame_id.0 as u32) + (window_id.0 as u32), lisp_pos as u64))
        }
    }

    #[test]
    fn font_driver_slot_serves_all_three_methods_and_blocks_reentrant_borrow() {
        let mut eval = Context::new(crate::test_utils::unit_runtime());
        let shape_calls = Rc::new(Cell::new(0));
        let metrics_calls = Rc::new(Cell::new(0));
        let realized_calls = Rc::new(Cell::new(0));
        eval.font_driver = Some(Box::new(RecordingDriver {
            shape_calls: shape_calls.clone(),
            metrics_calls: metrics_calls.clone(),
            realized_calls: realized_calls.clone(),
            panic_on_shape: false,
        }));

        let shaped = eval
            .with_font_driver(|eval, driver| driver.shape_gstring(eval, Value::NIL, Value::NIL));
        assert_eq!(shaped, Some(GstringShapeOutcome::Shaped(1)));
        assert_eq!(shape_calls.get(), 1);
        assert!(eval.font_driver.is_some());

        let metrics = eval
            .font_glyph_text_metrics(Value::NIL, 0x41)
            .expect("glyph metrics");
        assert_eq!(metrics.code, 0x41);
        assert_eq!(metrics.width, 8);
        assert_eq!(metrics_calls.get(), 1);

        let realized = eval.with_font_driver(|eval, driver| {
            driver.realized_fontset(eval, FrameId(4), WindowId(3), 9)
        });
        assert_eq!(realized, Some(Some((7, 9))));
        assert_eq!(realized_calls.get(), 1);
        assert!(eval.font_driver.is_some());
    }

    #[test]
    fn with_font_driver_restores_slot_after_panic_unwind() {
        let mut eval = Context::new(crate::test_utils::unit_runtime());
        let shape_calls = Rc::new(Cell::new(0));
        let metrics_calls = Rc::new(Cell::new(0));
        let realized_calls = Rc::new(Cell::new(0));
        eval.font_driver = Some(Box::new(RecordingDriver {
            shape_calls: shape_calls.clone(),
            metrics_calls: metrics_calls.clone(),
            realized_calls: realized_calls.clone(),
            panic_on_shape: true,
        }));

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = eval.with_font_driver(|eval, driver| {
                driver.shape_gstring(eval, Value::NIL, Value::NIL)
            });
        }));
        assert!(result.is_err(), "driver panic must propagate");
        assert_eq!(shape_calls.get(), 1);
        assert!(
            eval.font_driver.is_some(),
            "FontDriverScope::drop must restore font_driver after unwind"
        );

        let realized = eval.with_font_driver(|eval, driver| {
            driver.realized_fontset(eval, FrameId(2), WindowId(5), 11)
        });
        assert_eq!(realized, Some(Some((7, 11))));
        assert_eq!(realized_calls.get(), 1);
    }
}
