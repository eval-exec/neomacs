//! Fringe-bitmap registry — the real backing store for `define-fringe-bitmap`.
//!
//! GNU Emacs keeps fringe bitmaps in `src/fringe.c` as a growable
//! `struct fringe_bitmap **fringe_bitmaps` array indexed by a small integer
//! that is stashed on the bitmap symbol's `'fringe` plist property. The 25
//! standard built-in bitmaps occupy the low indices; user bitmaps defined via
//! `define-fringe-bitmap` get the next free slot.
//!
//! This module backs both the user bitmaps that magit needs (its
//! section-heading fold arrows `magit-fringe-bitmap>` / `magit-fringe-bitmapv`,
//! applied through a `(left-fringe SYMBOL FACE)` display spec) AND GNU's 24
//! standard built-in bitmaps, which `pre_register_standard_bitmaps` seeds from
//! `fringe_standard_bitmaps::STANDARD_FRINGE_BITMAPS` (transcribed verbatim from
//! `src/fringe.c`). Standard bitmaps take indices `1..=24`; user bitmaps start
//! at `FIRST_USER_FRINGE_BITMAP_INDEX` (25), above the standard range.
//!
//! GC SAFETY: this registry holds **no raw `Value`s** — bits are stored as
//! `Vec<u16>` and the optional `set-fringe-bitmap-face` override is stored as
//! the face *name* (`String`), not a heap `Value`. So the registry is opaque to
//! the GC and never needs rooting, matching the project lesson that
//! `Vec<Value>` inside heap-adjacent containers is invisible to the collector.

use rustc_hash::FxHashMap;

use crate::emacs_core::intern::SymId;

/// The index reserved for the first user-defined fringe bitmap. GNU's standard
/// built-in bitmaps occupy `1..=24` (`MAX_STANDARD_FRINGE_BITMAPS` is 25 in
/// `src/fringe.c` — the array length, slot 0 being `NO_FRINGE_BITMAP`). Those
/// standard bitmaps are now seeded by `pre_register_standard_bitmaps`; user
/// bitmaps from `define-fringe-bitmap` start here, above the standard range, so
/// they never collide with a standard symbol.
pub(crate) const FIRST_USER_FRINGE_BITMAP_INDEX: u32 = 25;

/// Vertical alignment of a fringe bitmap relative to the rows it decorates.
/// Mirrors GNU's `enum {ALIGN_BITMAP_CENTER, ALIGN_BITMAP_TOP, ALIGN_BITMAP_BOTTOM}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FringeBitmapAlign {
    #[default]
    Center = 0,
    Top = 1,
    Bottom = 2,
}

impl FringeBitmapAlign {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// A single registered fringe bitmap.
///
/// `bits` rows are stored **MSB-aligned in a `u16`**: the leftmost pixel column
/// of the bitmap is bit 15, so a renderer reads column `b` (0 = leftmost) as
/// `(bits[r] >> (15 - b)) & 1`. GNU stores the raw row value with the visible
/// pixels in the *low* `width` bits (e.g. `#b01100000` = `0x60` for width 8);
/// we shift each row up by `16 - width` at parse time so the renderer never has
/// to know the width to find the top bit.
#[derive(Clone, Debug, PartialEq)]
pub struct FringeBitmap {
    /// One entry per row; `bits.len() == height`. Each row is MSB-aligned.
    pub bits: Vec<u16>,
    /// Bitmap height in rows (GNU clamps to 0..=255).
    pub height: u8,
    /// Bitmap width in pixel columns (1..=16).
    pub width: u8,
    /// Repeat period (0 = not periodic). Periodic bitmaps repeat `bits`
    /// vertically; we store the period but only single-tile rendering is wired
    /// up downstream for now.
    pub period: u8,
    /// Vertical alignment within the row.
    pub align: FringeBitmapAlign,
    /// `set-fringe-bitmap-face` override, stored as the face *name* (GC-safe).
    pub face: Option<String>,
}

/// The growable user-bitmap registry hung off `Context`.
#[derive(Clone, Debug, Default)]
pub struct FringeBitmapRegistry {
    user_bitmaps: FxHashMap<SymId, FringeBitmap>,
    /// Reverse map index -> symbol so a resolved bitmap index can be looked up
    /// by the display pipeline (which only carries the integer index).
    by_index: FxHashMap<u32, SymId>,
    next_index: u32,
}

impl FringeBitmapRegistry {
    pub fn new() -> Self {
        Self {
            user_bitmaps: FxHashMap::default(),
            by_index: FxHashMap::default(),
            next_index: FIRST_USER_FRINGE_BITMAP_INDEX,
        }
    }

    /// Seed the 24 GNU standard built-in fringe bitmaps (indices 1..=24) into
    /// the registry, exactly as `src/fringe.c`'s `standard_bitmaps[]` table.
    ///
    /// Each entry's symbol name is interned (via the global symbol registry) and
    /// stored both ways (`SymId -> FringeBitmap` and `index -> SymId`), so
    /// `index_of(sym)` and `get_by_index(index)` resolve standard symbols just
    /// like user bitmaps. Bits are MSB-aligned via `parse_bits_rows` (the same
    /// transform `define-fringe-bitmap` applies).
    ///
    /// `next_index` is advanced past the standard range to
    /// `MAX_STANDARD_FRINGE_BITMAPS` (25), so the first user bitmap defined via
    /// `define-fringe-bitmap` still gets index 25
    /// (`FIRST_USER_FRINGE_BITMAP_INDEX`) and never collides with a standard one.
    ///
    /// Returns the `(SymId, index)` pairs so the caller can set each symbol's
    /// `'fringe` plist property to its index (matching `define-fringe-bitmap` and
    /// `lisp/fringe.el`'s `(put SYM 'fringe N)` loop).
    pub fn pre_register_standard_bitmaps(&mut self) -> Vec<(SymId, u32)> {
        use crate::emacs_core::builtins::fringe_standard_bitmaps::{
            MAX_STANDARD_FRINGE_BITMAPS, STANDARD_FRINGE_BITMAPS,
        };

        let mut assigned = Vec::with_capacity(STANDARD_FRINGE_BITMAPS.len());
        for entry in STANDARD_FRINGE_BITMAPS {
            let sym = crate::emacs_core::intern::intern(entry.name);
            let bits = parse_bits_rows(entry.rows, entry.width);
            let bitmap = FringeBitmap {
                bits,
                height: entry.height,
                width: entry.width,
                period: entry.period,
                align: entry.align,
                face: None,
            };
            self.user_bitmaps.insert(sym, bitmap);
            self.by_index.insert(entry.index, sym);
            assigned.push((sym, entry.index));
        }
        // Keep user bitmaps above the standard range. `next_index` may already
        // have been bumped by an earlier `define-fringe-bitmap` (e.g. in tests
        // that seed lazily); never lower it below the standard count.
        self.next_index = self.next_index.max(MAX_STANDARD_FRINGE_BITMAPS);
        assigned
    }

    /// Store (or replace) a bitmap for `sym`, returning the assigned index.
    /// A re-definition of an existing symbol keeps its previously assigned
    /// index (GNU: "If BITMAP already exists, the existing definition is
    /// replaced."), preserving any `'fringe` integer already on the symbol.
    pub fn define(
        &mut self,
        sym: SymId,
        existing_index: Option<u32>,
        mut bitmap: FringeBitmap,
    ) -> u32 {
        let index = match existing_index.or_else(|| self.index_of(sym)) {
            Some(index) => index,
            None => {
                let index = self.next_index;
                self.next_index = self.next_index.saturating_add(1);
                index
            }
        };
        // Preserve a face override set by an earlier `set-fringe-bitmap-face`
        // when a later `define-fringe-bitmap` replaces the geometry only.
        if bitmap.face.is_none()
            && let Some(prev) = self.user_bitmaps.get(&sym)
        {
            bitmap.face = prev.face.clone();
        }
        self.user_bitmaps.insert(sym, bitmap);
        self.by_index.insert(index, sym);
        index
    }

    /// Remove a bitmap (GNU `destroy-fringe-bitmap`). The freed index is left
    /// in `by_index` cleared; we do not reuse it eagerly (GNU walks the array
    /// for a free slot, but never below `MAX_STANDARD_FRINGE_BITMAPS`).
    pub fn destroy(&mut self, sym: SymId) {
        if let Some(bitmap) = self.user_bitmaps.remove(&sym) {
            let _ = bitmap;
        }
        self.by_index.retain(|_, s| *s != sym);
    }

    /// Look up a bitmap by its defining symbol.
    pub fn get(&self, sym: SymId) -> Option<&FringeBitmap> {
        self.user_bitmaps.get(&sym)
    }

    /// Look up a bitmap by its resolved integer index.
    pub fn get_by_index(&self, index: u32) -> Option<&FringeBitmap> {
        let sym = self.by_index.get(&index)?;
        self.user_bitmaps.get(sym)
    }

    /// The symbol a resolved integer index names, if any. GNU's
    /// `get_fringe_bitmap_name`, used to report laid-out rows back to Lisp.
    pub fn symbol_for_index(&self, index: u32) -> Option<SymId> {
        self.by_index.get(&index).copied()
    }

    /// The index a symbol's bitmap is stored at, if any.
    pub fn index_of(&self, sym: SymId) -> Option<u32> {
        self.by_index
            .iter()
            .find_map(|(index, s)| (*s == sym).then_some(*index))
    }

    /// Record a `set-fringe-bitmap-face` override (by face name). Returns true
    /// when the bitmap exists.
    pub fn set_face(&mut self, sym: SymId, face: Option<String>) -> bool {
        match self.user_bitmaps.get_mut(&sym) {
            Some(bitmap) => {
                bitmap.face = face;
                true
            }
            None => false,
        }
    }

    /// Iterate `(index, &bitmap)` over every registered user bitmap. Used to
    /// build a per-frame snapshot for the display pipeline.
    pub fn iter_indexed(&self) -> impl Iterator<Item = (u32, &FringeBitmap)> {
        self.by_index
            .iter()
            .filter_map(|(index, sym)| self.user_bitmaps.get(sym).map(|bitmap| (*index, bitmap)))
    }
}

/// Parse the BITS argument (a vector of integers or a string of rows) into
/// MSB-aligned `u16` rows. `width` is the validated 1..=16 pixel width; each
/// source row's low `width` bits are shifted up so the leftmost column lands on
/// bit 15. Returns one entry per source row (length == source length); HEIGHT
/// padding is applied separately by the caller.
///
/// GNU's `Faref` on a string BITS returns the character *code* for each row, so
/// a unibyte string row is a single byte and a multibyte string row is a
/// codepoint; we mirror that by iterating characters and masking to `width`.
pub(crate) fn parse_bits_rows(bits: &[u32], width: u8) -> Vec<u16> {
    let width = width.clamp(1, 16);
    let shift = 16 - u32::from(width);
    let mask: u32 = if width >= 16 {
        0xFFFF
    } else {
        (1u32 << width) - 1
    };
    bits.iter()
        .map(|row| {
            let row = row & mask;
            ((row << shift) & 0xFFFF) as u16
        })
        .collect()
}

/// Apply HEIGHT to a row list parsed from BITS, centering shorter content the
/// way GNU does (`fill1 = (height - h) / 2`, `fill2 = height - h - fill1`).
/// When `height` is `None` the natural BITS length is used.
pub(crate) fn fit_rows_to_height(rows: Vec<u16>, height: Option<u8>) -> (Vec<u16>, u8) {
    let natural = rows.len().min(255) as u8;
    let target = match height {
        Some(h) => h,
        None => return (rows, natural),
    };
    if target == natural {
        return (rows, target);
    }
    if target < natural {
        // GNU keeps the leading rows when HEIGHT < natural length.
        let mut trimmed = rows;
        trimmed.truncate(usize::from(target));
        return (trimmed, target);
    }
    let extra = usize::from(target) - rows.len();
    let fill1 = extra / 2;
    let fill2 = extra - fill1;
    let mut out = Vec::with_capacity(usize::from(target));
    out.extend(std::iter::repeat_n(0u16, fill1));
    out.extend(rows);
    out.extend(std::iter::repeat_n(0u16, fill2));
    (out, target)
}

#[cfg(test)]
#[path = "tests/fringe_bitmap.rs"]
mod tests;
