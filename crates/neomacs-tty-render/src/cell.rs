//! Canonical physical terminal-cell value, with no derived cache.
//!
//! The sole scalar representation is 113 bits: three 25-bit optional terminal
//! colours, all eight underline bits, four style bits, a 21-bit Unicode scalar,
//! four independent provenance/width bits and the sole extender-presence bit.
//! The remaining 15 bits are zero.
//! Grapheme bytes remain an ordinary owned Box<str>; only metadata records absence.

use super::{BlankErase, CellAttrs, CellMaterialization, TerminalAdvance};
use neomacs_display_protocol::TerminalColor;
use std::fmt;

const COLOR_BITS: u32 = 25;
const COLOR_MASK: u128 = (1 << COLOR_BITS) - 1;
const UNDERLINE: u32 = 75;
const BOLD: u32 = 83;
const ITALIC: u32 = 84;
const STRIKETHROUGH: u32 = 85;
const INVERSE: u32 = 86;
const CHAR: u32 = 87;
const BLANK_ERASE: u32 = 108;
const MATERIALIZATION: u32 = 109;
const PADDING: u32 = 110;
const TERMINAL_ADVANCE: u32 = 111;
const EXTENDERS_PRESENT: u32 = 112;
const ATTRS_MASK: u128 = (1 << CHAR) - 1;
const CHAR_MASK: u128 = (1 << 21) - 1;

// Constant projections of the sole canonical scalar value; no cached keys.
const CANONICAL_METADATA_MASK: u128 = (1u128 << 113) - 1;
const DAMAGE_CONTENT_MASK: u128 = CANONICAL_METADATA_MASK & !(1u128 << MATERIALIZATION);
// Exact current-default-space predicate: ignore only materialization/advance.
const PRESERVABLE_BLANK_MASK: u128 = DAMAGE_CONTENT_MASK & !(1u128 << TERMINAL_ADVANCE);
const PAINTER_VISIBLE_MASK: u128 =
    ATTRS_MASK | (CHAR_MASK << CHAR) | (1u128 << PADDING) | (1u128 << EXTENDERS_PRESENT);

/// Ephemeral named construction value. It is consumed once and never stored
/// beside the canonical cell metadata. All fields preserve their independent
/// old semantic values; no constructor flag is inferred from another field.
#[derive(Debug)]
pub struct TtyCellValue {
    pub ch: char,
    pub attrs: CellAttrs,
    pub blank_erase: BlankErase,
    pub materialization: CellMaterialization,
    pub padding: bool,
    pub extenders: Option<Box<str>>,
    pub terminal_advance: TerminalAdvance,
}

/// A physical terminal cell. Scalar fields have one canonical stored value.
///
/// The flags are independent: constructors/setters preserve even unusual
/// combinations such as an ASCII character with terminal-resolved advance.
/// `Some("")` extenders stay distinct from `None`, as in the original record.
///
/// Threading/ownership follows ordinary Rust borrowing: an owned cell may be
/// moved to another thread, immutable references may be shared, and mutation
/// requires a unique `&mut TtyCell`. No global evaluator/Lisp state, cache,
/// invalidation protocol, TLS, interior mutability or mutator rendezvous exists.
#[repr(C)]
pub struct TtyCell {
    metadata: u128,
    // None has exactly one representation: clear presence plus an empty box.
    // Some(empty) has the same empty bytes but a SET canonical presence bit.
    extenders: Box<str>,
}

// Keep the authoritative comparison value at the start of each owned cell.
// No pointer reinterpretation relies on these offsets.
const _: () = {
    assert!(std::mem::offset_of!(TtyCell, metadata) == 0);
    assert!(std::mem::offset_of!(TtyCell, extenders) == std::mem::size_of::<u128>());
};

#[inline]
fn encode_color(color: Option<TerminalColor>) -> u128 {
    match color {
        None => 0,
        Some(TerminalColor::Indexed(index)) => (1 << 16) | u128::from(index),
        Some(TerminalColor::Direct { r, g, b }) => {
            (1 << 24) | (u128::from(r) << 16) | (u128::from(g) << 8) | u128::from(b)
        }
    }
}

#[inline]
fn decode_color(bits: u128) -> Option<TerminalColor> {
    // Only the exhaustive typed encoder can construct these private bits.
    if bits & (1 << 24) != 0 {
        Some(TerminalColor::Direct {
            r: (bits >> 16) as u8,
            g: (bits >> 8) as u8,
            b: bits as u8,
        })
    } else if bits == 0 {
        None
    } else {
        Some(TerminalColor::Indexed(bits as u16))
    }
}

#[inline]
fn encode_attrs(attrs: CellAttrs) -> u128 {
    // No `..`: adding a semantic attribute must make this conversion fail to
    // compile until its canonical representation has been explicitly chosen.
    let CellAttrs {
        fg,
        bg,
        bold,
        italic,
        underline,
        underline_color,
        strikethrough,
        inverse,
    } = attrs;
    encode_color(fg)
        | (encode_color(bg) << COLOR_BITS)
        | (encode_color(underline_color) << (2 * COLOR_BITS))
        | (u128::from(underline) << UNDERLINE)
        | (u128::from(bold) << BOLD)
        | (u128::from(italic) << ITALIC)
        | (u128::from(strikethrough) << STRIKETHROUGH)
        | (u128::from(inverse) << INVERSE)
}

/// Call-local typed attributes, never retained alongside a canonical cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PackedCellAttrs(u128);

impl PackedCellAttrs {
    #[inline]
    pub(super) fn new(attrs: CellAttrs) -> Self {
        Self(encode_attrs(attrs))
    }
}

/// Named private construction input; every non-attribute field stays independent.
#[derive(Debug)]
pub(super) struct PackedTtyCellValue {
    pub(super) ch: char,
    pub(super) attrs: PackedCellAttrs,
    pub(super) blank_erase: BlankErase,
    pub(super) materialization: CellMaterialization,
    pub(super) padding: bool,
    pub(super) extenders: Option<Box<str>>,
    pub(super) terminal_advance: TerminalAdvance,
}

impl TtyCell {
    /// Construct the exact semantic value; no normalization or width inference.
    #[inline]
    pub fn new(value: TtyCellValue) -> Self {
        let TtyCellValue {
            ch,
            attrs,
            blank_erase,
            materialization,
            padding,
            extenders,
            terminal_advance,
        } = value;
        Self::from_packed(PackedTtyCellValue {
            ch,
            attrs: PackedCellAttrs::new(attrs),
            blank_erase,
            materialization,
            padding,
            extenders,
            terminal_advance,
        })
    }

    #[inline]
    pub(super) fn from_packed(value: PackedTtyCellValue) -> Self {
        // Exhaustive named input: adding a construction field requires a
        // deliberate representation decision rather than a positional guess.
        let PackedTtyCellValue {
            ch,
            attrs,
            blank_erase,
            materialization,
            padding,
            extenders,
            terminal_advance,
        } = value;
        let blank = match blank_erase {
            BlankErase::DefaultFace => 0,
            BlankErase::Explicit => 1,
        };
        let materialized = match materialization {
            CellMaterialization::Erased => 0,
            CellMaterialization::Written => 1,
        };
        let advance = match terminal_advance {
            TerminalAdvance::OneColumn => 0,
            TerminalAdvance::TerminalResolved => 1,
        };
        let (present, extenders) = match extenders {
            None => (false, Box::<str>::default()),
            Some(extenders) => (true, extenders),
        };
        Self {
            metadata: attrs.0
                | (u128::from(u32::from(ch)) << CHAR)
                | (blank << BLANK_ERASE)
                | (materialized << MATERIALIZATION)
                | (u128::from(padding) << PADDING)
                | (advance << TERMINAL_ADVANCE)
                | (u128::from(present) << EXTENDERS_PRESENT),
            extenders,
        }
    }

    /// Match the old current-default-space predicate without rich decoding.
    /// Materialization and terminal advance are intentionally independent.
    #[inline]
    pub(super) fn preserved_blank_materialization(
        &self,
        attrs: PackedCellAttrs,
    ) -> Option<CellMaterialization> {
        let expected = attrs.0 | (u128::from(u32::from(' ')) << CHAR);
        if self.metadata & PRESERVABLE_BLANK_MASK == expected {
            Some(self.materialization())
        } else {
            None
        }
    }

    #[cfg(test)]
    #[inline]
    pub(super) fn has_attrs(&self, attrs: PackedCellAttrs) -> bool {
        self.metadata & ATTRS_MASK == attrs.0
    }

    #[inline]
    pub fn ch(&self) -> char {
        char::from_u32(((self.metadata >> CHAR) & CHAR_MASK) as u32)
            .expect("canonical TtyCell contains a Unicode scalar")
    }

    /// Reconstruct an ephemeral Copy value; no second attribute record is kept.
    #[inline]
    pub fn attrs(&self) -> CellAttrs {
        CellAttrs {
            fg: decode_color(self.metadata & COLOR_MASK),
            bg: decode_color((self.metadata >> COLOR_BITS) & COLOR_MASK),
            bold: self.bit(BOLD),
            italic: self.bit(ITALIC),
            underline: (self.metadata >> UNDERLINE) as u8,
            underline_color: decode_color((self.metadata >> (2 * COLOR_BITS)) & COLOR_MASK),
            strikethrough: self.bit(STRIKETHROUGH),
            inverse: self.bit(INVERSE),
        }
    }

    #[inline]
    pub fn blank_erase(&self) -> BlankErase {
        if self.bit(BLANK_ERASE) {
            BlankErase::Explicit
        } else {
            BlankErase::DefaultFace
        }
    }

    #[inline]
    pub fn materialization(&self) -> CellMaterialization {
        if self.bit(MATERIALIZATION) {
            CellMaterialization::Written
        } else {
            CellMaterialization::Erased
        }
    }

    #[inline]
    pub fn padding(&self) -> bool {
        self.bit(PADDING)
    }

    #[inline]
    pub fn terminal_advance(&self) -> TerminalAdvance {
        if self.bit(TERMINAL_ADVANCE) {
            TerminalAdvance::TerminalResolved
        } else {
            TerminalAdvance::OneColumn
        }
    }

    /// Typed borrowed view of the sole canonical optional presence.
    /// Some(empty) stays distinct from None; no borrowed Option owner is stored.
    #[inline]
    pub fn extenders(&self) -> Option<&str> {
        if self.bit(EXTENDERS_PRESENT) {
            Some(self.extenders.as_ref())
        } else {
            None
        }
    }

    #[inline]
    pub fn set_ch(&mut self, ch: char) {
        self.metadata =
            (self.metadata & !(CHAR_MASK << CHAR)) | (u128::from(u32::from(ch)) << CHAR);
    }

    #[inline]
    pub fn set_attrs(&mut self, attrs: CellAttrs) {
        self.metadata = (self.metadata & !ATTRS_MASK) | encode_attrs(attrs);
    }

    #[inline]
    pub fn set_blank_erase(&mut self, value: BlankErase) {
        self.set_bit(
            BLANK_ERASE,
            match value {
                BlankErase::DefaultFace => false,
                BlankErase::Explicit => true,
            },
        );
    }

    #[inline]
    pub fn set_materialization(&mut self, value: CellMaterialization) {
        self.set_bit(
            MATERIALIZATION,
            match value {
                CellMaterialization::Erased => false,
                CellMaterialization::Written => true,
            },
        );
    }

    #[inline]
    pub fn set_padding(&mut self, value: bool) {
        self.set_bit(PADDING, value);
    }

    #[inline]
    pub fn set_terminal_advance(&mut self, value: TerminalAdvance) {
        self.set_bit(
            TERMINAL_ADVANCE,
            match value {
                TerminalAdvance::OneColumn => false,
                TerminalAdvance::TerminalResolved => true,
            },
        );
    }

    #[inline]
    pub fn set_extenders(&mut self, value: Option<Box<str>>) {
        match value {
            Some(extenders) => {
                self.extenders = extenders;
                self.set_bit(EXTENDERS_PRESENT, true);
            }
            None => {
                self.extenders = Box::<str>::default();
                self.set_bit(EXTENDERS_PRESENT, false);
            }
        }
    }

    /// Damage's existing content domain: every physical field except whether
    /// the cell was erased or written. Erase eligibility and advance still count.
    #[inline]
    pub fn same_damage_content(&self, other: &Self) -> bool {
        (self.metadata & DAMAGE_CONTENT_MASK) == (other.metadata & DAMAGE_CONTENT_MASK)
            && (!self.bit(EXTENDERS_PRESENT) || self.extenders == other.extenders)
    }

    /// Native painter's existing visible domain: character, attributes, padding
    /// and optional grapheme bytes. Erase, materialization and advance are ignored.
    #[inline]
    pub fn same_painter_visible(&self, other: &Self) -> bool {
        (self.metadata & PAINTER_VISIBLE_MASK) == (other.metadata & PAINTER_VISIBLE_MASK)
            && (!self.bit(EXTENDERS_PRESENT) || self.extenders == other.extenders)
    }

    #[inline]
    fn bit(&self, at: u32) -> bool {
        self.metadata & (1 << at) != 0
    }

    #[inline]
    fn set_bit(&mut self, at: u32, value: bool) {
        self.metadata = (self.metadata & !(1 << at)) | (u128::from(value) << at);
    }
}

impl Clone for TtyCell {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            metadata: self.metadata,
            extenders: if self.bit(EXTENDERS_PRESENT) {
                self.extenders.clone()
            } else {
                Box::<str>::default()
            },
        }
    }
}

impl Default for TtyCell {
    fn default() -> Self {
        Self::new(TtyCellValue {
            ch: ' ',
            attrs: CellAttrs::default(),
            blank_erase: BlankErase::DefaultFace,
            materialization: CellMaterialization::Erased,
            padding: false,
            extenders: None,
            terminal_advance: TerminalAdvance::OneColumn,
        })
    }
}

impl PartialEq for TtyCell {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.metadata == other.metadata
            && (!self.bit(EXTENDERS_PRESENT) || equal_present_extenders(self, other))
    }
}

#[cold]
#[inline(never)]
fn equal_present_extenders(left: &TtyCell, right: &TtyCell) -> bool {
    left.extenders == right.extenders
}

impl Eq for TtyCell {}

impl fmt::Debug for TtyCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TtyCell")
            .field("ch", &self.ch())
            .field("attrs", &self.attrs())
            .field("blank_erase", &self.blank_erase())
            .field("materialization", &self.materialization())
            .field("padding", &self.padding())
            .field("extenders", &self.extenders())
            .field("terminal_advance", &self.terminal_advance())
            .finish()
    }
}

#[cfg(test)]
#[path = "cell/tests/cell_test.rs"]
mod tests;
