//! Geometry-bearing display properties in prefix strings. Capturing interval
//! revisions alone cannot detect mutation inside a property's Lisp value.
use super::pixel_input::SpaceInput;
use crate::buffer::{CharPos0, text_props::TextPropertiesRef};
use crate::emacs_core::{
    display_spec::{DisplayPropertySpecs, display_spec_when_parts},
    image_catalog::ImageSpecIdentity,
    plist::plist_get,
    value::Value,
};
use std::{ops::ControlFlow, sync::Arc};

/// String storage inputs shared by prefix strings and replacement strings.
/// Deliberately does not follow display properties recursively: GNU suppresses
/// recursive string-display replacement. Face inputs are captured separately
/// from display properties because replacement strings can still carry faces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StringContentInput {
    identity: usize,
    bytes: Arc<[u8]>,
    multibyte: bool,
    properties_tick: u64,
    faces: super::face_input::StringFaceInputs,
}

impl StringContentInput {
    pub(super) fn capture(identity: usize, string: &crate::heap_types::LispString) -> Self {
        Self {
            identity,
            bytes: string.as_bytes().into(),
            multibyte: string.is_multibyte(),
            properties_tick: string.intervals().mutation_tick(),
            faces: super::face_input::StringFaceInputs::capture(string.intervals()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct StringDisplayInputs(Arc<[DisplayRun]>);

#[derive(Clone, Debug, PartialEq, Eq)]
struct DisplayRun {
    start: CharPos0,
    end: CharPos0,
    eval_enabled: bool,
    specs: Box<[SpecInput]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SpecInput {
    condition: Option<usize>,
    payload: SpecPayload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SpecPayload {
    Space(SpaceInput),
    Text(StringContentInput),
    Image(ImageSpecIdentity),
    // Preserve order and identity of surrounding specs. Their nested payloads
    // (modifiers and other resource types) need separate capture.
    Other(usize),
}

impl StringDisplayInputs {
    pub(super) fn capture(properties: TextPropertiesRef<'_>) -> Self {
        let mut runs = Vec::new();
        let display = Value::symbol("display");
        properties.for_each_interval_from_char_pos(CharPos0::new(0), |start, end, plist| {
            if let Some(value) = plist_get(plist, &display) {
                let decoded = DisplayPropertySpecs::of(value);
                let mut specs = Vec::new();
                decoded.for_each(|spec| {
                    // GNU and the classifier unwrap WHEN exactly once. The
                    // condition is not evaluated here; dynamic conditions have
                    // their own pre-walk evaluation/reuse policy.
                    let (condition, spec) = match display_spec_when_parts(spec) {
                        Some((form, payload)) => (Some(form.bits()), payload),
                        None => (None, spec),
                    };
                    let payload = match SpaceInput::capture(spec) {
                        Some(space) => SpecPayload::Space(space),
                        None => match spec.as_lisp_string() {
                            Some(string) => {
                                SpecPayload::Text(StringContentInput::capture(spec.bits(), string))
                            }
                            None => match ImageSpecIdentity::from_lisp_spec(&spec) {
                                Some(image) => SpecPayload::Image(image),
                                None => SpecPayload::Other(spec.bits()),
                            },
                        },
                    };
                    specs.push(SpecInput { condition, payload });
                    ControlFlow::Continue(())
                });
                runs.push(DisplayRun {
                    start,
                    end,
                    eval_enabled: decoded.eval_enabled,
                    specs: specs.into(),
                });
            }
            true
        });
        Self(runs.into())
    }
}
