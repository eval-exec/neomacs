//! Unicode Bidirectional Algorithm (UAX#9) implementation.
//!
//! Provides full bidi text processing for the neomacs display engine:
//! - Character Bidi_Class lookup from Unicode data tables
//! - Explicit embedding level resolution (X1-X8)
//! - Weak type resolution (W1-W7)
//! - Paired Bracket Algorithm (N0/BPA)
//! - Neutral type resolution (N1-N2)
//! - Implicit level resolution (I1-I2)
//! - Whitespace reset (L1)
//! - Visual reordering (L2)
//! - Character mirroring (L4)
//!
//! # Usage
//!
//! ```rust
//! use neomacs_display::core::bidi::{resolve_levels, reorder_visual, apply_mirroring, BidiDir};
//!
//! let text = "Hello \u{05E9}\u{05DC}\u{05D5}\u{05DD}";
//! let levels = resolve_levels(text, BidiDir::Auto);
//! let visual_order = reorder_visual(&levels);
//! ```

pub mod reorder;
pub mod resolver;
pub mod tables;
pub mod types;

pub use reorder::{apply_mirroring, reorder_line, reorder_visual};
pub use resolver::{paragraph_base_level, resolve_levels};
pub use tables::{bidi_class, bidi_mirror, bracket_type};
pub use types::{BidiClass, BidiDir, BracketType, MAX_DEPTH, ResolvedChar};
