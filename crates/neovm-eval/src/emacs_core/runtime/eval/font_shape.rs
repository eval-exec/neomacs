//! Cold gstring-shaping contract owned by the evaluator layer (stage-4 W4).
//!
//! The display layer (the layout engine, which owns the font system) shapes
//! gstrings; the evaluator owns the cache and the installed driver seam. The
//! outcome enum and the driver alias carry only value types plus Context, so
//! they live below display. The cache trace is the evaluator's GC visit.

use super::Context;
use neovm_value::display_vocab::GstringShapeCacheKey;
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

/// The installed font-shaping driver seam. neovm-core owns the gstring
/// contract (GNU src/font.c) but not the shaping engine: the display layer
/// installs the driver exactly the way it installs `redisplay_fn`.
pub type FontShapeFn = Box<dyn FnMut(&mut Context, Value, Value) -> GstringShapeOutcome>;

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
