//! Bytecode parameter storage vocabulary (T1 storage cut).

use super::{LambdaParams, SymId, Value};
use crate::tagged::value::ValueKind;
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
#[doc(hidden)]
pub struct FormalMarkers {
    pub(crate) optional: SymId,
    pub(crate) rest: SymId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum FormalRole {
    Variable,
    OptionalMarker,
    RestMarker,
}

/// GNU accepts cons/nil ARGLIST at construction and validates its contents
/// only during invocation. This wrapper preserves that boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicArglist(pub(crate) Value);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum FormalBinding {
    Required(SymId),
    Optional(SymId),
    Rest(SymId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum FormalStep {
    Marker,
    Binding(FormalBinding),
    End,
}

#[derive(Debug, thiserror::Error)]
#[doc(hidden)]
pub enum FormalListError {
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

// ---------------------------------------------------------------------------
// Storage-owned impls (T1 coherence): every method whose body needs only the
// lowered vocabulary lives here with the data; the upper crate keeps policy
// behind extension traits.
// ---------------------------------------------------------------------------

impl RestSlot {
    #[inline]
    pub const fn is_present(self) -> bool {
        matches!(self, Self::Present)
    }
}

impl From<i64> for ArgTemplate {
    #[inline]
    fn from(raw: i64) -> Self {
        Self(raw)
    }
}

impl ArgTemplate {
    #[inline]
    pub const fn raw(self) -> i64 {
        self.0
    }

    #[inline]
    pub const fn mandatory(self) -> usize {
        (self.0 & 127) as usize
    }

    #[inline]
    pub const fn nonrest(self) -> i64 {
        self.0 >> 8
    }

    #[inline]
    pub const fn rest(self) -> RestSlot {
        if self.0 & 128 == 0 {
            RestSlot::Absent
        } else {
            RestSlot::Present
        }
    }
}

impl StackParamShape {
    #[inline]
    pub const fn required(self) -> usize {
        self.required
    }

    #[inline]
    pub const fn nonrest(self) -> usize {
        self.nonrest
    }

    #[inline]
    pub const fn rest(self) -> RestSlot {
        self.rest
    }
}

impl DynamicArglist {
    #[inline]
    pub const fn value(self) -> Value {
        self.0
    }
}

impl From<LambdaParams> for FunctionParams {
    fn from(params: LambdaParams) -> Self {
        Self::Named(params)
    }
}

impl TryFrom<Value> for FunctionParams {
    type Error = BytecodeSlotError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value.kind() {
            ValueKind::Fixnum(raw) => Ok(Self::Stack(raw.into())),
            ValueKind::Cons | ValueKind::Nil => Ok(Self::Dynamic(DynamicArglist(value))),
            _ => Err(BytecodeSlotError::Arglist(value)),
        }
    }
}

impl StackDepth {
    pub const ZERO: Self = Self(0);

    /// The deepest frame the in-place iterative Bcall installs. GNU-compiled
    /// functions stay far below it; deeper frames take the recursive entry,
    /// whose reservation reports failure as a Lisp error.
    pub const ITERATIVE_FRAME_LIMIT: Self = Self(u16::MAX as usize);

    #[inline]
    pub const fn get(self) -> usize {
        self.0
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn for_test(depth: usize) -> Self {
        // Test-only constructor (peer R5): delegate to the canonical checked
        // constructor; never rebuild the storage field across the crate
        // boundary.
        Self::try_from(depth).expect("StackDepth::for_test depth is a valid GNU stack depth")
    }
}

impl std::fmt::Display for StackDepth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<usize> for StackDepth {
    type Error = StackDepthError;

    fn try_from(depth: usize) -> Result<Self, Self::Error> {
        if depth > Value::MOST_POSITIVE_FIXNUM as usize {
            return Err(StackDepthError::Range(depth as u64));
        }
        Ok(Self(depth))
    }
}

impl TryFrom<u64> for StackDepth {
    type Error = StackDepthError;

    fn try_from(depth: u64) -> Result<Self, Self::Error> {
        let depth = usize::try_from(depth).map_err(|_| StackDepthError::Range(depth))?;
        Self::try_from(depth)
    }
}

impl TryFrom<Value> for StackDepth {
    type Error = StackDepthError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let depth = value
            .as_fixnum()
            .filter(|depth| *depth >= 0)
            .ok_or(StackDepthError::Value(value))?;
        Self::try_from(depth as u64)
    }
}

impl FunctionParams {
    #[inline]
    pub fn named(&self) -> Option<&LambdaParams> {
        match self {
            Self::Named(params) => Some(params),
            Self::Stack(_) | Self::Dynamic(_) => None,
        }
    }

    /// Each embedded Value is a strong GC child independently of the
    /// separately stored, observable original arglist slot.
    #[inline]
    #[deny(clippy::wildcard_enum_match_arm)]
    #[doc(hidden)]
    pub fn heap_child(&self) -> Option<Value> {
        match self {
            Self::Dynamic(params) => Some(params.value()),
            Self::Stack(_) | Self::Named(_) => None,
        }
    }
}

impl StackDepth {
    #[inline]
    pub fn value(self) -> Value {
        // Constructor invariants bound the value by the positive fixnum limit.
        Value::fixnum(self.0 as i64)
    }
}

impl ArgTemplate {
    #[inline]
    #[doc(hidden)]
    pub fn field_0(&self) -> i64 {
        self.0
    }
}

impl DynamicArglist {
    #[inline]
    #[doc(hidden)]
    pub fn field_0(&self) -> Value {
        self.0
    }
}

impl FormalMarkers {
    #[inline]
    #[doc(hidden)]
    pub fn optional(&self) -> SymId {
        self.optional
    }
    #[inline]
    #[doc(hidden)]
    pub fn rest(&self) -> SymId {
        self.rest
    }
}

impl StackParamShape {
    #[inline]
    #[doc(hidden)]
    pub fn nonrest_ref(&self) -> usize {
        self.nonrest
    }
    #[inline]
    #[doc(hidden)]
    pub fn required_ref(&self) -> usize {
        self.required
    }
    #[inline]
    #[doc(hidden)]
    pub fn rest_ref(&self) -> RestSlot {
        self.rest
    }
}

impl StackParamShape {
    #[doc(hidden)]
    pub fn new(required: usize, nonrest: usize, rest: RestSlot) -> Self {
        // GNU's descriptor is two independent bit fields
        // (bytecode.c:533-537: required = low 7 bits, nonrest = bits 8..), so
        // an inconsistent template may report required > nonrest. Such a
        // template is still a storable GNU object: `accepts`/`call_shape`
        // reject every call through it with GNU's arity error, and the
        // optional-count projection answers None. Do not assert the
        // invariant here; the shape math uses only nonrest + rest.
        Self {
            required,
            nonrest,
            rest,
        }
    }
}

impl FormalMarkers {
    #[doc(hidden)]
    pub fn new(optional: SymId, rest: SymId) -> Self {
        Self { optional, rest }
    }
}
