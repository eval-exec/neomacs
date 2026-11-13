//! Layout input records for pixel input (stage-4 W2/E5).

use neovm_value::image_identity::ImageSpecIdentity;
use std::sync::Arc;

/// The supported geometry operands of one stretch-space display spec.
#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct SpaceInput {
    pub identity: usize,
    pub operands: [Option<PixelInput>;
        <neovm_value::display_vocab::DisplaySpaceKey as strum::EnumCount>::COUNT],
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum PixelInput {
    Atom(usize),
    Expression(Arc<[Token]>),
}

/// Prefix encoding, with explicit ends for variadic arithmetic. Back references
/// make shared subexpressions and cycles finite without recursive Rust stacks.
#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum Token {
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
#[doc(hidden)]
pub enum Role {
    Expression,
    Arguments,
}
