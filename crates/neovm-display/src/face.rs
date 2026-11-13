//! Face system facade (stage-4 W2/E5 FT-1).
//!
//! The face data vocabulary, merge/parse logic and tables moved verbatim to
//! `emacs_core::eval::data::face`; this module re-exports them at the old
//! `crate::face::*` paths and keeps the display-side realization, remapping
//! and dump submodules above.

mod dump_codes;
pub mod realization;
pub(crate) mod remapping;
use neovm_value::value::{Value, ValueKind};
pub use remapping::{FaceRemapEntry, FaceRemapping};

pub use neovm_eval::emacs_core::eval::data::face::*;
#[allow(unused_imports)]
pub(crate) use neovm_eval::emacs_core::eval::data::face::{
    DecorationProperty, FaceStorage, FastFaceHasher, LFACE_ATTRS, LFACE_VECTOR_SIZE,
    face_runtime_string, face_symbol_value, merge_face_height, merged_color_specification,
    normalized_face_name_value, parse_box_value, parse_color_value, parse_underline_value,
    realize_color_spec, retained_color_specification, trace_face_ref_roots,
    underline_color_operations, underline_color_specification,
};

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "face/tests/face_test.rs"]
mod tests;

#[cfg(test)]
#[path = "face/tests/color_specifications_test.rs"]
mod color_specifications_tests;

#[cfg(test)]
#[path = "face/tests/producer_identity_test.rs"]
mod producer_identity_tests;
