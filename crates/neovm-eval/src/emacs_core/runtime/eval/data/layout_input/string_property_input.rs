//! Layout input records for string property input (stage-4 W2/E5).

use super::pixel_input::SpaceInput;
use crate::emacs_core::eval::data::display_spec::DisplayEval;
use neovm_edit::buffer::CharPos0;
use neovm_value::image_identity::ImageSpecIdentity;
use std::sync::Arc;

/// String storage inputs shared by prefix strings and replacement strings.
/// Deliberately does not follow display properties recursively: GNU suppresses
/// recursive string-display replacement. Face inputs are captured separately
/// from display properties because replacement strings can still carry faces.
#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct StringContentInput {
    pub identity: usize,
    pub bytes: Arc<[u8]>,
    pub multibyte: bool,
    pub properties_tick: u64,
    pub faces: super::face_input::StringFaceInputs,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[doc(hidden)]
pub struct StringDisplayInputs(pub Arc<[DisplayRun]>);

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct DisplayRun {
    pub start: CharPos0,
    pub end: CharPos0,
    pub eval: DisplayEval,
    pub specs: Box<[SpecInput]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct SpecInput {
    pub condition: Option<usize>,
    pub payload: SpecPayload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum SpecPayload {
    Space(SpaceInput),
    Text(StringContentInput),
    Image(ImageSpecIdentity),
    // Preserve order and identity of surrounding specs. Their nested payloads
    // (modifiers and other resource types) need separate capture.
    Other(usize),
}
