//! Layout input records for pixel input (stage-4 W2/E5).

use neovm_value::image_identity::ImageSpecIdentity;
use std::sync::Arc;

/// The supported geometry operands of one stretch-space display spec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpaceInput {
    pub(crate) identity: usize,
    pub(crate) operands: [Option<PixelInput>;
        <crate::emacs_core::display_spec::DisplaySpaceKey as strum::EnumCount>::COUNT],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PixelInput {
    Atom(usize),
    Expression(Arc<[Token]>),
}

/// Prefix encoding, with explicit ends for variadic arithmetic. Back references
/// make shared subexpressions and cycles finite without recursive Rust stacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Token {
    Atom(usize),
    Sum,
    Difference,
    AbsolutePixels(usize),
    Scale(usize),
    Image(ImageSpecIdentity),
    End,
    Reference(usize),
    Opaque(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Role {
    Expression,
    Arguments,
}
