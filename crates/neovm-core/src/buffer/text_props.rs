//! Buffer-side facade over the value layer's text-property storage.
//!
//! The interval storage lives in [`crate::emacs_core::value::text_props`]
//! beside the values it carries. The casing-insertion policy needs the upper
//! text-property resolver and the evaluator's variable controls, so it stays
//! in this layer; it reaches the storage through the re-export below.

pub use crate::emacs_core::value::text_props::*;

pub(crate) use crate::emacs_core::value::text_props::{
    PropertyPlistApplication, ShiftedTextPropertySource,
};

use crate::buffer::position::{CharLen, CharPos0, CharRange};
use crate::emacs_core::value::Value;

#[path = "text_props/casing_insertion.rs"]
mod casing_insertion;
pub(crate) use casing_insertion::{
    CasingPropertyControls, CasingPropertyMode, CasingPropertyRoots,
};
