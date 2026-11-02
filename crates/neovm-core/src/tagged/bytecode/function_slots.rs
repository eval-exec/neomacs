//! Bytecode parameter storage vocabulary (T1 storage cut).

use super::{LambdaParams, SymId, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestSlot {
    Absent,
    Present,
}

/// Exact signed GNU ARGDESC. Decoding is total and never allocates names.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArgTemplate(pub(crate) i64);

/// Host-sized stack shape, issued only after checked descriptor conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackParamShape {
    pub(crate) required: usize,
    pub(crate) nonrest: usize,
    pub(crate) rest: RestSlot,
}

#[derive(Debug, thiserror::Error)]
pub enum ParamShapeError {
    #[error("nonrest argument count {0} cannot be represented by this host")]
    NonRest(i64),
    #[error("argument entry depth overflows the host size")]
    EntryDepth,
}

/// Canonical process-global marker identities. This cache holds only scalar
/// SymIds from the immutable global registry, never Lisp heap pointers or
/// mutator-local state, so all evaluators may read it concurrently.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FormalMarkers {
    pub(crate) optional: SymId,
    pub(crate) rest: SymId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormalRole {
    Variable,
    OptionalMarker,
    RestMarker,
}

/// GNU accepts cons/nil ARGLIST at construction and validates its contents
/// only during invocation. This wrapper preserves that boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicArglist(pub(crate) Value);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormalBinding {
    Required(SymId),
    Optional(SymId),
    Rest(SymId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormalStep {
    Marker,
    Binding(FormalBinding),
    End,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FormalListError {
    #[error("argument list contains a non-symbol formal: {0:?}")]
    NonSymbol(Value),
    #[error("argument list contains an invalid marker order")]
    MarkerOrder,
    #[error("argument list has an invalid tail: {0:?}")]
    Tail(Value),
    #[error("argument list ends with an unbound rest marker")]
    MissingRestVariable,
}

#[repr(C, u8)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionParams {
    Stack(ArgTemplate) = 0,
    Dynamic(DynamicArglist) = 1,
    Named(LambdaParams) = 2,
}

/// Nonnegative Lisp-fixnum stack depth. It stays full width in every consumer.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StackDepth(pub(crate) usize);

#[derive(Debug, thiserror::Error)]
pub enum StackDepthError {
    #[error("stack depth must be a nonnegative fixnum: {0:?}")]
    Value(Value),
    #[error("stack depth {0} is outside the nonnegative fixnum/host domain")]
    Range(u64),
}

#[derive(Debug, thiserror::Error)]
pub enum BytecodeSlotError {
    #[error("invalid bytecode arglist slot: {0:?}")]
    Arglist(Value),
    #[error("bytecode code slot must be an unibyte string: {0:?}")]
    Code(Value),
    #[error("bytecode constants slot must be an ordinary vector: {0:?}")]
    Constants(Value),
    #[error(transparent)]
    Depth(#[from] StackDepthError),
}
