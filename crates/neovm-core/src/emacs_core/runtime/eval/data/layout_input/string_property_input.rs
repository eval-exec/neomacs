//! Layout input records for string property input (stage-4 W2/E5).

use super::pixel_input::SpaceInput;
use crate::emacs_core::display::display_spec::DisplayEval;
use neovm_edit::buffer::CharPos0;
use neovm_value::image_identity::ImageSpecIdentity;
use std::sync::Arc;

/// String storage inputs shared by prefix strings and replacement strings.
/// Deliberately does not follow display properties recursively: GNU suppresses
/// recursive string-display replacement. Face inputs are captured separately
/// from display properties because replacement strings can still carry faces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StringContentInput {
    pub(crate) identity: usize,
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) multibyte: bool,
    pub(crate) properties_tick: u64,
    pub(crate) faces: super::face_input::StringFaceInputs,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StringDisplayInputs(pub(crate) Arc<[DisplayRun]>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayRun {
    pub(crate) start: CharPos0,
    pub(crate) end: CharPos0,
    pub(crate) eval: DisplayEval,
    pub(crate) specs: Box<[SpecInput]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpecInput {
    pub(crate) condition: Option<usize>,
    pub(crate) payload: SpecPayload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SpecPayload {
    Space(SpaceInput),
    Text(StringContentInput),
    Image(ImageSpecIdentity),
    // Preserve order and identity of surrounding specs. Their nested payloads
    // (modifiers and other resource types) need separate capture.
    Other(usize),
}
