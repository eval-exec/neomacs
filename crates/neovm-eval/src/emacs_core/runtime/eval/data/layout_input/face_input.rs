//! Layout input records for face input (stage-4 W2/E5).

use super::super::face::DecorationProperty;
use neovm_edit::buffer::CharPos0;
use neovm_value::display_vocab::LFaceAttr;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[doc(hidden)]
pub struct StringFaceInputs(pub Arc<[FaceRun]>);

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct FaceRun {
    pub start: CharPos0,
    pub end: CharPos0,
    pub property: FaceProperty,
    pub input: Arc<[Token]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum FaceProperty {
    Face,
    FontLockFace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum Token {
    Atom(usize),
    String { bytes: Arc<[u8]>, multibyte: bool },
    List,
    Attributes,
    Filtered,
    Filter(Option<Arc<[usize]>>),
    Attribute(LFaceAttr),
    Decoration,
    DecorationProperty(DecorationProperty),
    WidthPair,
    InlineBitmap,
    End,
    Reference(usize),
}
