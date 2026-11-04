//! Buffer-side facade over the value layer's text-property storage.
//!
//! The interval storage lives in [`neovm_value::value::text_props`]
//! beside the values it carries. The casing-insertion policy needs the upper
//! text-property resolver and the evaluator's variable controls, so it stays
//! in this layer; it reaches the storage through the re-export below.

pub use neovm_value::value::text_props::*;

pub(crate) use neovm_value::value::text_props::{
    PropertyPlistApplication, ShiftedTextPropertySource,
};

use neovm_value::value::Value;

#[path = "text_props/casing_insertion.rs"]
mod casing_insertion;
pub(crate) use casing_insertion::{CasingPropertyControls, CasingPropertyMode};
