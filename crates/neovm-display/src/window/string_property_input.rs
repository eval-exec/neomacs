//! Geometry-bearing display properties in prefix strings. Capturing interval
//! revisions alone cannot detect mutation inside a property's Lisp value.
use super::face_input::StringFaceInputsExt;
use super::pixel_input::SpaceInput;
use super::pixel_input::SpaceInputExt;
use crate::emacs_core::image_catalog::ImageSpecIdentity;
use neovm_edit::buffer::{CharPos0, text_props::TextPropertiesRef};
use neovm_eval::emacs_core::eval::data::display_spec::{
    DisplayEval, DisplayPropertySpecs, display_spec_when_parts,
};
use neovm_value::plist::plist_get;
use neovm_value::value::Value;
use std::ops::ControlFlow;

pub(crate) use neovm_eval::emacs_core::eval::data::layout_input::string_property_input::{
    DisplayRun, SpecInput, SpecPayload, StringContentInput, StringDisplayInputs,
};

pub(crate) trait StringContentInputExt: Sized {
    fn capture(identity: usize, string: &neovm_value::heap_types::LispString) -> Self;
}

impl StringContentInputExt for StringContentInput {
    fn capture(identity: usize, string: &neovm_value::heap_types::LispString) -> Self {
        Self {
            identity,
            bytes: string.as_bytes().into(),
            multibyte: string.is_multibyte(),
            properties_tick: string.intervals().mutation_tick(),
            faces: super::face_input::StringFaceInputs::capture(string.intervals()),
        }
    }
}

pub(crate) trait StringDisplayInputsExt: Sized {
    fn capture(properties: TextPropertiesRef<'_>) -> Self;
}

impl StringDisplayInputsExt for StringDisplayInputs {
    fn capture(properties: TextPropertiesRef<'_>) -> Self {
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
                    eval: decoded.eval,
                    specs: specs.into(),
                });
            }
            true
        });
        Self(runs.into())
    }
}
