//! Validated GNU bytecode slots and parameter domains.
//!
//! Values are owned by the evaluator's existing heap/root scope. These types
//! introduce no global or thread-local Lisp state; callers borrow views while
//! that evaluator is active, and publish no views to another mutator.

use crate::emacs_core::error::{Flow, LispCondition, signal};
use crate::emacs_core::value::{LambdaParams, Value, ValueKind};
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BytecodeSlotOrigin {
    Constructor,
    Reader,
    Direct,
}

impl BytecodeSlotOrigin {
    #[cold]
    #[inline(never)]
    pub fn invalid_flow(self) -> Flow {
        let (condition, text) = match self {
            Self::Constructor => (LispCondition::Error, "Invalid byte-code object"),
            Self::Reader => (LispCondition::InvalidReadSyntax, "Invalid byte-code object"),
            Self::Direct => (LispCondition::Error, "Invalid byte-code"),
        };
        signal(condition, vec![Value::string(text)])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestSlot {
    Absent,
    Present,
}

impl RestSlot {
    #[inline]
    pub const fn is_present(self) -> bool {
        matches!(self, Self::Present)
    }
}

/// Exact signed GNU ARGDESC. Decoding is total and never allocates names.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArgTemplate(i64);

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

    #[inline]
    pub fn accepts(self, nargs: usize) -> bool {
        self.mandatory() <= nargs
            && (self.rest().is_present()
                || i64::try_from(nargs).is_ok_and(|nargs| nargs <= self.nonrest()))
    }

    /// GNU's byte compiler emits 15-bit descriptors (7-bit counts plus the
    /// `&rest` bit, bytecode.c:535-537); their shapes need no host checks.
    const COMPACT_LIMIT: i64 = 1 << 15;

    /// The stack shape a call with NARGS arguments enters, checked in GNU's
    /// order: the signed arity test first, then host representability.
    #[inline]
    pub fn call_shape(self, nargs: usize) -> Result<StackParamShape, CallShapeError> {
        if !(0..Self::COMPACT_LIMIT).contains(&self.0) {
            return self.wide_call_shape(nargs);
        }
        let shape = StackParamShape {
            required: self.mandatory(),
            nonrest: (self.0 >> 8) as usize,
            rest: self.rest(),
        };
        if shape.required <= nargs && (shape.rest.is_present() || nargs <= shape.nonrest) {
            Ok(shape)
        } else {
            Err(CallShapeError::Arity)
        }
    }

    #[cold]
    #[inline(never)]
    fn wide_call_shape(self, nargs: usize) -> Result<StackParamShape, CallShapeError> {
        if !self.accepts(nargs) {
            return Err(CallShapeError::Arity);
        }
        self.stack_shape().map_err(CallShapeError::Shape)
    }

    #[inline]
    pub fn stack_shape(self) -> Result<StackParamShape, ParamShapeError> {
        let nonrest = usize::try_from(self.nonrest())
            .map_err(|_| ParamShapeError::NonRest(self.nonrest()))?;
        let shape = StackParamShape {
            required: self.mandatory(),
            nonrest,
            rest: self.rest(),
        };
        shape.entry_depth()?;
        Ok(shape)
    }
}

/// Host-sized stack shape, issued only after checked descriptor conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackParamShape {
    required: usize,
    nonrest: usize,
    rest: RestSlot,
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

    /// Optimization-only optional count. An inconsistent descriptor cannot
    /// become a synthetic optional list, but remains a stored GNU object.
    #[inline]
    pub fn optional(self) -> Option<usize> {
        self.nonrest.checked_sub(self.required)
    }

    #[inline]
    pub fn entry_depth(self) -> Result<usize, ParamShapeError> {
        self.nonrest
            .checked_add(usize::from(self.rest.is_present()))
            .ok_or(ParamShapeError::EntryDepth)
    }
}

/// Why a call cannot enter a descriptor-shaped frame.
#[derive(Debug, thiserror::Error)]
pub enum CallShapeError {
    /// GNU signals `wrong-number-of-arguments` with `(mandatory . nonrest)`.
    #[error("argument count outside the descriptor's arity")]
    Arity,
    /// The descriptor's frame cannot be represented on this host.
    #[error(transparent)]
    Shape(ParamShapeError),
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
    optional: crate::emacs_core::intern::SymId,
    rest: crate::emacs_core::intern::SymId,
}

static_assertions::assert_impl_all!(FormalMarkers: Send, Sync, Copy, std::fmt::Debug);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormalRole {
    Variable,
    OptionalMarker,
    RestMarker,
}

impl FormalMarkers {
    #[inline]
    pub(crate) fn canonical() -> Self {
        static MARKERS: std::sync::OnceLock<FormalMarkers> = std::sync::OnceLock::new();
        *MARKERS.get_or_init(|| Self {
            optional: crate::emacs_core::intern::intern("&optional"),
            rest: crate::emacs_core::intern::intern("&rest"),
        })
    }

    #[inline]
    pub(crate) fn classify(self, symbol: crate::emacs_core::intern::SymId) -> FormalRole {
        if symbol == self.optional {
            FormalRole::OptionalMarker
        } else if symbol == self.rest {
            FormalRole::RestMarker
        } else {
            FormalRole::Variable
        }
    }
}

/// GNU accepts cons/nil ARGLIST at construction and validates its contents
/// only during invocation. This wrapper preserves that boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicArglist(Value);

impl DynamicArglist {
    #[inline]
    pub const fn value(self) -> Value {
        self.0
    }

    pub(crate) fn invocation(self) -> FormalCursor {
        FormalCursor {
            tail: self.0,
            state: FormalState::Required,
            markers: FormalMarkers::canonical(),
            cycle: crate::emacs_core::plist::TailCycleCheck::new(self.0),
            _mutator: PhantomData,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormalState {
    Required,
    Optional,
    AwaitingRestVariable,
    RestBound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormalBinding {
    Required(crate::emacs_core::intern::SymId),
    Optional(crate::emacs_core::intern::SymId),
    Rest(crate::emacs_core::intern::SymId),
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

/// Invocation-local formal walk. It deliberately reads CDR after a binding
/// callback, preserving GNU's validation order and observable list mutation.
/// Callers root `roots()` through each Lisp-capable binding operation.
pub(crate) struct FormalCursor {
    tail: Value,
    state: FormalState,
    markers: FormalMarkers,
    cycle: crate::emacs_core::plist::TailCycleCheck,
    _mutator: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for FormalCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormalCursor")
            .field("tail", &self.tail)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl FormalCursor {
    pub(crate) fn roots(&self) -> [Value; 2] {
        [self.tail, self.cycle.tortoise()]
    }

    pub(crate) fn has_next_cell(&self) -> bool {
        self.tail.is_cons()
    }

    /// One GNU formal-list iteration, so the caller can poll quit before
    /// every cell, including optional/rest markers, while rooting the cursor.
    pub(crate) fn next_step(&mut self) -> Result<FormalStep, FormalListError> {
        if !self.tail.is_cons() {
            if !self.tail.is_nil() {
                return Err(FormalListError::Tail(self.tail));
            }
            return if self.state == FormalState::AwaitingRestVariable {
                Err(FormalListError::MissingRestVariable)
            } else {
                Ok(FormalStep::End)
            };
        }
        let formal = self.tail.cons_car();
        let formal = formal.as_symbol_with_pos_sym().unwrap_or(formal);
        let symbol = formal
            .as_symbol_id()
            .ok_or(FormalListError::NonSymbol(formal))?;
        if self.markers.classify(symbol) == FormalRole::RestMarker {
            self.state = match self.state {
                FormalState::Required | FormalState::Optional => FormalState::AwaitingRestVariable,
                FormalState::AwaitingRestVariable | FormalState::RestBound => {
                    return Err(FormalListError::MarkerOrder);
                }
            };
            self.advance()?;
            Ok(FormalStep::Marker)
        } else if self.markers.classify(symbol) == FormalRole::OptionalMarker {
            self.state = match self.state {
                FormalState::Required => FormalState::Optional,
                FormalState::Optional
                | FormalState::AwaitingRestVariable
                | FormalState::RestBound => return Err(FormalListError::MarkerOrder),
            };
            self.advance()?;
            Ok(FormalStep::Marker)
        } else {
            Ok(FormalStep::Binding(match self.state {
                FormalState::Required => FormalBinding::Required(symbol),
                FormalState::Optional => FormalBinding::Optional(symbol),
                FormalState::AwaitingRestVariable | FormalState::RestBound => {
                    FormalBinding::Rest(symbol)
                }
            }))
        }
    }

    #[cfg(test)]
    pub(crate) fn next_binding(&mut self) -> Result<Option<FormalBinding>, FormalListError> {
        loop {
            match self.next_step()? {
                FormalStep::Marker => {}
                FormalStep::Binding(binding) => return Ok(Some(binding)),
                FormalStep::End => return Ok(None),
            }
        }
    }

    pub(crate) fn finish_binding(&mut self) -> Result<(), FormalListError> {
        if self.state == FormalState::AwaitingRestVariable {
            self.state = FormalState::RestBound;
        }
        self.advance()
    }

    fn advance(&mut self) -> Result<(), FormalListError> {
        self.tail = self.tail.cons_cdr();
        if let Some(cycle) = self.cycle.step(self.tail) {
            return Err(FormalListError::Tail(cycle));
        }
        Ok(())
    }
}

#[repr(C, u8)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionParams {
    Stack(ArgTemplate) = 0,
    Dynamic(DynamicArglist) = 1,
    Named(LambdaParams) = 2,
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

impl FunctionParams {
    #[inline]
    pub fn named(&self) -> Option<&LambdaParams> {
        match self {
            Self::Named(params) => Some(params),
            Self::Stack(_) | Self::Dynamic(_) => None,
        }
    }

    #[inline]
    pub fn stack_shape(&self) -> Option<StackParamShape> {
        match self {
            Self::Stack(template) => template.stack_shape().ok(),
            Self::Dynamic(args) if args.value().is_nil() => Some(StackParamShape {
                required: 0,
                nonrest: 0,
                rest: RestSlot::Absent,
            }),
            Self::Dynamic(_) => None,
            Self::Named(params) => {
                let shape = StackParamShape {
                    required: params.required.len(),
                    nonrest: params.required.len().checked_add(params.optional.len())?,
                    rest: if params.rest.is_some() {
                        RestSlot::Present
                    } else {
                        RestSlot::Absent
                    },
                };
                shape.entry_depth().ok()?;
                Some(shape)
            }
        }
    }

    /// Each embedded Value is a strong GC child independently of the
    /// separately stored, observable original arglist slot.
    #[inline]
    #[deny(clippy::wildcard_enum_match_arm)]
    pub(crate) fn heap_child(&self) -> Option<Value> {
        match self {
            Self::Dynamic(params) => Some(params.value()),
            Self::Stack(_) | Self::Named(_) => None,
        }
    }

    #[inline]
    pub fn fixed_arity(&self) -> Option<usize> {
        let shape = self.stack_shape()?;
        (shape.rest == RestSlot::Absent && shape.required == shape.nonrest)
            .then_some(shape.required)
    }
}

/// Nonnegative Lisp-fixnum stack depth. It stays full width in every consumer.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StackDepth(usize);

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

    #[inline]
    pub fn value(self) -> Value {
        // Constructor invariants bound the value by the positive fixnum limit.
        Value::fixnum(self.0 as i64)
    }

    #[cfg(test)]
    pub const fn for_test(depth: usize) -> Self {
        assert!(depth <= Value::MOST_POSITIVE_FIXNUM as usize);
        Self(depth)
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

#[derive(Debug, thiserror::Error)]
pub enum StackDepthError {
    #[error("stack depth must be a nonnegative fixnum: {0:?}")]
    Value(Value),
    #[error("stack depth {0} is outside the nonnegative fixnum/host domain")]
    Range(u64),
}

impl StackDepthError {
    #[cold]
    #[inline(never)]
    pub fn into_flow(self, origin: BytecodeSlotOrigin) -> Flow {
        origin.invalid_flow()
    }
}

/// GNU reader/direct-call code string. Those two boundaries accept a
/// multibyte legacy string and normalize it, unlike make-byte-code.
#[derive(Clone, Copy, Debug)]
pub struct BytecodeString {
    value: Value,
    _mutator: PhantomData<Rc<()>>,
}

impl TryFrom<Value> for BytecodeString {
    type Error = BytecodeSlotError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if value.is_string() {
            Ok(Self {
                value,
                _mutator: PhantomData,
            })
        } else {
            Err(BytecodeSlotError::Code(value))
        }
    }
}

impl BytecodeString {
    pub fn into_unibyte(self, origin: BytecodeSlotOrigin) -> Result<UnibyteCode, Flow> {
        let normalized = if self.value.string_is_multibyte() {
            crate::emacs_core::misc::builtin_string_as_unibyte(vec![self.value])
                .map_err(|_| origin.invalid_flow())?
        } else {
            self.value
        };
        UnibyteCode::try_from(normalized).map_err(|error| error.into_flow(origin))
    }
}

/// Validated code handle; borrowed bytes remain tied to this typed view.
#[derive(Clone, Copy, Debug)]
pub struct UnibyteCode {
    value: Value,
    _mutator: PhantomData<Rc<()>>,
}

impl TryFrom<Value> for UnibyteCode {
    type Error = BytecodeSlotError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if value.is_string() && !value.string_is_multibyte() {
            Ok(Self {
                value,
                _mutator: PhantomData,
            })
        } else {
            Err(BytecodeSlotError::Code(value))
        }
    }
}

impl UnibyteCode {
    pub const fn value(self) -> Value {
        self.value
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.value
            .as_lisp_string()
            .expect("UnibyteCode constructor checked the string payload")
            .as_bytes()
    }
}

/// Ordinary vector handle. Elements are data and never reinterpreted by shape.
#[derive(Clone, Copy, Debug)]
pub struct ConstantsVector {
    value: Value,
    _mutator: PhantomData<Rc<()>>,
}

impl TryFrom<Value> for ConstantsVector {
    type Error = BytecodeSlotError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        if value.is_vector() {
            Ok(Self {
                value,
                _mutator: PhantomData,
            })
        } else {
            Err(BytecodeSlotError::Constants(value))
        }
    }
}

impl ConstantsVector {
    pub const fn value(self) -> Value {
        self.value
    }

    pub fn as_slice(&self) -> &[Value] {
        self.value
            .as_vector_data()
            .expect("ConstantsVector constructor checked the vector payload")
            .as_slice()
    }
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

impl BytecodeSlotError {
    #[cold]
    #[inline(never)]
    pub fn into_flow(self, origin: BytecodeSlotOrigin) -> Flow {
        origin.invalid_flow()
    }
}

/// A decoded view of the four constructor slots. The original handles are
/// retained by the closure slots; constant elements are never rewritten here.
#[derive(Clone, Debug)]
pub struct CompiledSlots {
    pub params: FunctionParams,
    pub code: UnibyteCode,
    pub constants: ConstantsVector,
    pub depth: StackDepth,
}

impl TryFrom<[Value; 4]> for CompiledSlots {
    type Error = BytecodeSlotError;

    fn try_from([args, code, constants, depth]: [Value; 4]) -> Result<Self, Self::Error> {
        Ok(Self {
            params: args.try_into()?,
            code: code.try_into()?,
            constants: constants.try_into()?,
            depth: depth.try_into()?,
        })
    }
}

const _: () = assert!(std::mem::size_of::<StackDepth>() == std::mem::size_of::<usize>());
const _: () = assert!(std::mem::size_of::<ArgTemplate>() == std::mem::size_of::<i64>());
static_assertions::assert_impl_all!(StackDepth: Send, Sync, Copy, std::fmt::Debug);
static_assertions::assert_impl_all!(ArgTemplate: Send, Sync, Copy, std::fmt::Debug);

/// Resource failures after the slot's nonnegative-fixnum depth was validated.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FrameStorageError {
    #[error("bytecode frame length overflows host size")]
    Overflow,
    #[error("cannot reserve bytecode frame storage: {0}")]
    Allocation(#[from] std::collections::TryReserveError),
}

impl FrameStorageError {
    #[cold]
    #[inline(never)]
    pub(crate) fn into_flow(self) -> Flow {
        // GNU bytecode.c:514 rejects unavailable bytecode stack space with
        // this generic error. Rust reports host allocation failure instead
        // of letting Vec's capacity-overflow/global-OOM paths abort Lisp.
        signal(
            LispCondition::Error,
            vec![Value::string("Bytecode stack overflow")],
        )
    }
}

const _: () = assert!(
    std::mem::size_of::<FunctionParams>()
        == std::mem::size_of::<LambdaParams>() + std::mem::align_of::<LambdaParams>()
);

#[cfg(test)]
#[path = "tests/function_slots.rs"]
mod tests;
