//! Core types for the Unicode Bidirectional Algorithm (UAX#9).

use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Bidi character class as defined in Unicode, stored with GNU Emacs'
/// `bidi_type_t` discriminants from `src/dispextern.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum BidiClass {
    // Strong types. GNU keeps these early because glyphs reserve only
    // three bits for the subset stored in `struct glyph`.
    L = 1,  // STRONG_L
    R = 2,  // STRONG_R
    AL = 7, // STRONG_AL

    // Weak types
    EN = 3,   // WEAK_EN
    ES = 17,  // WEAK_ES
    ET = 18,  // WEAK_ET
    AN = 4,   // WEAK_AN
    CS = 19,  // WEAK_CS
    NSM = 20, // WEAK_NSM
    BN = 5,   // WEAK_BN

    // Neutral types
    B = 6,   // NEUTRAL_B
    S = 21,  // NEUTRAL_S
    WS = 22, // NEUTRAL_WS
    ON = 23, // NEUTRAL_ON

    // Explicit formatting
    LRE = 8,  // Left-to-right embedding
    LRO = 9,  // Left-to-right override
    RLE = 10, // Right-to-left embedding
    RLO = 11, // Right-to-left override
    PDF = 12, // Pop directional format
    LRI = 13, // Left-to-right isolate
    RLI = 14, // Right-to-left isolate
    FSI = 15, // First strong isolate
    PDI = 16, // Pop directional isolate
}

impl BidiClass {
    /// GNU `bidi_type_t` code for this class.
    pub fn gnu_type_code(self) -> u8 {
        self.into()
    }

    /// Convert from a GNU `bidi_type_t` code.  Code 0 is GNU's internal
    /// `UNKNOWN_BT`, which is not a valid Unicode bidi class for real
    /// characters.
    pub fn from_gnu_type_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    /// Whether this is a strong type (L, R, AL).
    pub fn is_strong(self) -> bool {
        matches!(self, BidiClass::L | BidiClass::R | BidiClass::AL)
    }

    /// Whether this is a weak type.
    pub fn is_weak(self) -> bool {
        matches!(
            self,
            BidiClass::EN
                | BidiClass::ES
                | BidiClass::ET
                | BidiClass::AN
                | BidiClass::CS
                | BidiClass::NSM
                | BidiClass::BN
        )
    }

    /// Whether this is a neutral type.
    pub fn is_neutral(self) -> bool {
        matches!(
            self,
            BidiClass::B | BidiClass::S | BidiClass::WS | BidiClass::ON
        )
    }

    /// Whether this is an explicit formatting character.
    pub fn is_explicit(self) -> bool {
        matches!(
            self,
            BidiClass::LRE
                | BidiClass::LRO
                | BidiClass::RLE
                | BidiClass::RLO
                | BidiClass::PDF
                | BidiClass::LRI
                | BidiClass::RLI
                | BidiClass::FSI
                | BidiClass::PDI
        )
    }

    /// Whether this is an isolate initiator (LRI, RLI, FSI).
    pub fn is_isolate_initiator(self) -> bool {
        matches!(self, BidiClass::LRI | BidiClass::RLI | BidiClass::FSI)
    }

    /// Whether this is a removed-by-X9 type (LRE, RLE, LRO, RLO, PDF, BN).
    pub fn is_removed_by_x9(self) -> bool {
        matches!(
            self,
            BidiClass::LRE
                | BidiClass::RLE
                | BidiClass::LRO
                | BidiClass::RLO
                | BidiClass::PDF
                | BidiClass::BN
        )
    }

    /// Map to "strong" direction for neutral resolution.
    /// EN and AN are treated as R for N1/N2 rules.
    pub fn to_strong_for_neutral(self) -> BidiClass {
        match self {
            BidiClass::EN | BidiClass::AN => BidiClass::R,
            other => other,
        }
    }
}

/// Paragraph/base direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum BidiDir {
    /// Left-to-right.
    LTR = 1,
    /// Right-to-left.
    RTL = 2,
    /// Auto-detect from first strong character.
    Auto = 0,
}

impl BidiDir {
    /// GNU `bidi_dir_t` code for this direction.
    pub fn gnu_dir_code(self) -> u8 {
        self.into()
    }

    /// Convert from GNU `bidi_dir_t` (`NEUTRAL_DIR`, `L2R`, `R2L`).
    pub fn from_gnu_dir_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }

    /// Base embedding level for this direction.
    pub fn base_level(self) -> u8 {
        match self {
            BidiDir::LTR | BidiDir::Auto => 0,
            BidiDir::RTL => 1,
        }
    }
}

/// The paragraph direction a displayed buffer asks for.
///
/// GNU derives this per window at `start_display` (`src/xdisp.c:3600-3611`):
/// it reads the buffer-local `bidi-display-reordering` and
/// `bidi-paragraph-direction` and stores `it->paragraph_embedding`, which
/// `bidi_paragraph_init` (`src/bidi.c:1713`) turns into the base level.  A nil
/// `bidi-display-reordering` disables reordering entirely (`Off`); a
/// `bidi-paragraph-direction` of `'left-to-right`/`'right-to-left` pins the base
/// level, and every other value (including nil) uses UAX#9 P2/P3.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ParagraphDirection {
    /// `bidi-paragraph-direction` nil (or any non-direction value).
    #[default]
    Auto,
    /// `bidi-paragraph-direction` is `left-to-right`.
    LeftToRight,
    /// `bidi-paragraph-direction` is `right-to-left`.
    RightToLeft,
    /// `bidi-display-reordering` is nil: no reordering at all.
    Off,
}

impl ParagraphDirection {
    /// The resolver base direction, or `None` when reordering is disabled.
    pub fn base_dir(self) -> Option<BidiDir> {
        match self {
            ParagraphDirection::Auto => Some(BidiDir::Auto),
            ParagraphDirection::LeftToRight => Some(BidiDir::LTR),
            ParagraphDirection::RightToLeft => Some(BidiDir::RTL),
            ParagraphDirection::Off => None,
        }
    }
}

/// The two paragraph directions a window carries.
///
/// GNU resolves buffer text under the displayed buffer's BVARs
/// (`src/xdisp.c:3600-3611`), but chrome rows are Lisp display strings drawn
/// by `display_mode_line` (`src/xdisp.c:28165`) and `display_tab_bar`
/// (`src/xdisp.c:14727`), which set `it->paragraph_embedding = L2R`
/// unconditionally, and `reseat_to_string` gates string reordering on the
/// DEFAULT `bidi-display-reordering` (`buffer_defaults`), not the buffer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParagraphDirections {
    /// Buffer text and the minibuffer's own text.
    pub text: ParagraphDirection,
    /// Mode/header/tab-line and tab-bar glyph rows.
    pub chrome: ParagraphDirection,
}

impl Default for ParagraphDirections {
    /// GNU defaults: text is UAX#9 automatic and chrome is L2R
    /// (`bidi-display-reordering` defaults to t).
    fn default() -> Self {
        Self {
            text: ParagraphDirection::Auto,
            chrome: ParagraphDirection::LeftToRight,
        }
    }
}

/// Bracket type for the Paired Bracket Algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BracketType {
    None,
    Open(char),  // Opening bracket, stores canonical closing
    Close(char), // Closing bracket, stores canonical opening
}

/// Override status for level stack entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum Override {
    Neutral = 0,
    LTR = 1,
    RTL = 2,
}

impl Override {
    /// GNU `bidi_dir_t` code used for overrides on the directional stack.
    pub fn gnu_dir_code(self) -> u8 {
        self.into()
    }

    /// Convert from GNU `bidi_dir_t` override code.
    pub fn from_gnu_dir_code(code: u8) -> Option<Self> {
        Self::try_from(code).ok()
    }
}

/// Entry on the directional status stack (X1-X8).
#[derive(Debug, Clone, Copy)]
pub struct DirectionalStatus {
    pub level: u8,
    pub override_status: Override,
    pub isolate_status: bool,
}

/// Result of resolving one character's bidi properties.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedChar {
    /// The character.
    pub ch: char,
    /// Original bidi class from Unicode data.
    pub original_class: BidiClass,
    /// Resolved embedding level (0-125).
    pub level: u8,
}

/// Maximum depth of explicit embedding/override/isolate nesting (UAX#9).
pub const MAX_DEPTH: u8 = 125;

/// Maximum size of the BPA stack.
pub const MAX_BPA_STACK: usize = 63;

#[cfg(test)]
#[path = "types/tests/types_test.rs"]
mod tests;
