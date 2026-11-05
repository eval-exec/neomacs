//! Buffer-side facade over the value layer's text-property storage.
//!
//! The interval storage lives in [`neovm_value::value::text_props`]
//! beside the values it carries. The casing-insertion policy needs the upper
//! text-property resolver and the evaluator's variable controls, so it stays
//! in this layer; it reaches the storage through the re-export below.

pub use neovm_value::value::text_props::*;

#[doc(hidden)]
pub use neovm_value::value::text_props::{PropertyPlistApplication, ShiftedTextPropertySource};

use neovm_value::value::Value;

#[cfg(not(any(test, feature = "test-support")))]
#[path = "text_props/casing_insertion.rs"]
mod casing_insertion;
#[cfg(any(test, feature = "test-support"))]
#[path = "text_props/casing_insertion.rs"]
pub mod casing_insertion;
#[doc(hidden)]
pub use casing_insertion::{CasingPropertyControls, CasingPropertyMode};
