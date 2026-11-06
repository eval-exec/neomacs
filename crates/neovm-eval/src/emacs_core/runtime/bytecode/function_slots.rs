//! Validated GNU bytecode slots and parameter domains.
//!
//! Values are owned by the evaluator's existing heap/root scope. These types
//! introduce no global or thread-local Lisp state; callers borrow views while
//! that evaluator is active, and publish no views to another mutator.

use neovm_value::error::{Flow, LispCondition, signal};
use neovm_value::value::{LambdaParams, Value};

pub use neovm_value::tagged::bytecode::function_slots::{
    ArgTemplate, BytecodeSlotError, DynamicArglist, FunctionParams, ParamShapeError, RestSlot,
    StackDepth, StackDepthError, StackParamShape,
};
pub(crate) use neovm_value::tagged::bytecode::function_slots::{
    FormalBinding, FormalListError, FormalMarkers, FormalRole, FormalStep,
};
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

/// Upper policy on the lowered [`ArgTemplate`].
///
/// The descriptor's pure const projections (`raw`, `mandatory`, `nonrest`,
/// `rest`) live on the storage type; the arity/shape policy stays upper and
/// crate-visible. Static dispatch only; callers import the trait.
pub(crate) trait ArgTemplateExt: Sized {
    fn accepts(self, nargs: usize) -> bool;
    fn call_shape(self, nargs: usize) -> Result<StackParamShape, CallShapeError>;
    fn stack_shape(self) -> Result<StackParamShape, ParamShapeError>;
}

impl ArgTemplateExt for ArgTemplate {
    #[inline]
    fn accepts(self, nargs: usize) -> bool {
        self.mandatory() <= nargs
            && (self.rest().is_present()
                || i64::try_from(nargs).is_ok_and(|nargs| nargs <= self.nonrest()))
    }

    /// The stack shape a call with NARGS arguments enters, checked in GNU's
    /// order: the signed arity test first, then host representability.
    #[inline]
    fn call_shape(self, nargs: usize) -> Result<StackParamShape, CallShapeError> {
        if !(0..COMPACT_LIMIT).contains(&self.field_0()) {
            return wide_call_shape(self, nargs);
        }
        let shape = StackParamShape::new(
            self.mandatory(),
            (self.field_0() >> 8) as usize,
            self.rest(),
        );
        if shape.required_ref() <= nargs
            && (shape.rest_ref().is_present() || nargs <= shape.nonrest_ref())
        {
            Ok(shape)
        } else {
            Err(CallShapeError::Arity)
        }
    }

    #[inline]
    fn stack_shape(self) -> Result<StackParamShape, ParamShapeError> {
        let nonrest = usize::try_from(self.nonrest())
            .map_err(|_| ParamShapeError::NonRest(self.nonrest()))?;
        let shape = StackParamShape::new(self.mandatory(), nonrest, self.rest());
        shape.entry_depth()?;
        Ok(shape)
    }
}

/// GNU's byte compiler emits 15-bit descriptors (7-bit counts plus the
/// `&rest` bit, bytecode.c:535-537); their shapes need no host checks.
const COMPACT_LIMIT: i64 = 1 << 15;

#[cold]
#[inline(never)]
fn wide_call_shape(template: ArgTemplate, nargs: usize) -> Result<StackParamShape, CallShapeError> {
    if !template.accepts(nargs) {
        return Err(CallShapeError::Arity);
    }
    template.stack_shape().map_err(CallShapeError::Shape)
}

/// Upper policy on the lowered [`StackParamShape`].
pub(crate) trait StackParamShapeExt: Sized {
    fn optional(self) -> Option<usize>;
    fn entry_depth(self) -> Result<usize, ParamShapeError>;
}

impl StackParamShapeExt for StackParamShape {
    /// Optimization-only optional count. An inconsistent descriptor cannot
    /// become a synthetic optional list, but remains a stored GNU object.
    #[inline]
    fn optional(self) -> Option<usize> {
        self.nonrest_ref().checked_sub(self.required_ref())
    }

    #[inline]
    fn entry_depth(self) -> Result<usize, ParamShapeError> {
        self.nonrest_ref()
            .checked_add(usize::from(self.rest_ref().is_present()))
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

static_assertions::assert_impl_all!(FormalMarkers: Send, Sync, Copy, std::fmt::Debug);

/// Upper policy on the lowered [`FormalMarkers`] (it interns GNU's marker
/// names in the process-global obarray).
pub(crate) trait FormalMarkersExt: Sized {
    fn canonical() -> Self;
    fn classify(self, symbol: neovm_value::intern::SymId) -> FormalRole;
}

impl FormalMarkersExt for FormalMarkers {
    #[inline]
    fn canonical() -> Self {
        static MARKERS: std::sync::OnceLock<FormalMarkers> = std::sync::OnceLock::new();
        *MARKERS.get_or_init(|| {
            Self::new(
                neovm_value::intern::intern("&optional"),
                neovm_value::intern::intern("&rest"),
            )
        })
    }

    #[inline]
    fn classify(self, symbol: neovm_value::intern::SymId) -> FormalRole {
        if symbol == self.optional() {
            FormalRole::OptionalMarker
        } else if symbol == self.rest() {
            FormalRole::RestMarker
        } else {
            FormalRole::Variable
        }
    }
}

/// Upper policy on the lowered [`DynamicArglist`] (invocation owns the
/// GNU formal walk and its rooted cursor).
pub(crate) trait DynamicArglistExt: Sized {
    fn invocation(self) -> FormalCursor;
}

impl DynamicArglistExt for DynamicArglist {
    fn invocation(self) -> FormalCursor {
        FormalCursor {
            tail: self.field_0(),
            state: FormalState::Required,
            markers: FormalMarkers::canonical(),
            cycle: neovm_value::plist::TailCycleCheck::new(self.field_0()),
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

/// Invocation-local formal walk. It deliberately reads CDR after a binding
/// callback, preserving GNU's validation order and observable list mutation.
/// Callers root `roots()` through each Lisp-capable binding operation.
pub(crate) struct FormalCursor {
    tail: Value,
    state: FormalState,
    markers: FormalMarkers,
    cycle: neovm_value::plist::TailCycleCheck,
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
    #[doc(hidden)]
    pub fn roots(&self) -> [Value; 2] {
        [self.tail, self.cycle.tortoise()]
    }

    #[doc(hidden)]
    pub fn has_next_cell(&self) -> bool {
        self.tail.is_cons()
    }

    /// One GNU formal-list iteration, so the caller can poll quit before
    /// every cell, including optional/rest markers, while rooting the cursor.
    #[doc(hidden)]
    pub fn next_step(&mut self) -> Result<FormalStep, FormalListError> {
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
            .as_bare_symbol_id()
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
    #[doc(hidden)]
    pub fn next_binding(&mut self) -> Result<Option<FormalBinding>, FormalListError> {
        loop {
            match self.next_step()? {
                FormalStep::Marker => {}
                FormalStep::Binding(binding) => return Ok(Some(binding)),
                FormalStep::End => return Ok(None),
            }
        }
    }

    #[doc(hidden)]
    pub fn finish_binding(&mut self) -> Result<(), FormalListError> {
        if self.state == FormalState::AwaitingRestVariable {
            self.state = FormalState::RestBound;
        }
        self.advance()
    }

    #[doc(hidden)]
    pub fn advance(&mut self) -> Result<(), FormalListError> {
        self.tail = self.tail.cons_cdr();
        if let Some(cycle) = self.cycle.step(self.tail) {
            return Err(FormalListError::Tail(cycle));
        }
        Ok(())
    }
}

/// Upper policy on the lowered [`FunctionParams`].
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) trait FunctionParamsExt {
    fn stack_shape(&self) -> Option<StackParamShape>;
    fn fixed_arity(&self) -> Option<usize>;
}

impl FunctionParamsExt for FunctionParams {
    #[inline]
    fn stack_shape(&self) -> Option<StackParamShape> {
        match self {
            Self::Stack(template) => template.stack_shape().ok(),
            Self::Dynamic(args) if args.value().is_nil() => {
                Some(StackParamShape::new(0, 0, RestSlot::Absent))
            }
            Self::Dynamic(_) => None,
            Self::Named(params) => {
                let shape = StackParamShape::new(
                    params.required.len(),
                    params.required.len().checked_add(params.optional.len())?,
                    if params.rest.is_some() {
                        RestSlot::Present
                    } else {
                        RestSlot::Absent
                    },
                );
                shape.entry_depth().ok()?;
                Some(shape)
            }
        }
    }

    #[inline]
    fn fixed_arity(&self) -> Option<usize> {
        let shape = self.stack_shape()?;
        (shape.rest_ref() == RestSlot::Absent && shape.required_ref() == shape.nonrest_ref())
            .then_some(shape.required_ref())
    }
}

/// Upper policy on the lowered [`StackDepthError`] (signals through the
/// upper slot-origin policy).
pub(crate) trait StackDepthErrorExt: Sized {
    fn into_flow(self, origin: BytecodeSlotOrigin) -> Flow;
}

impl StackDepthErrorExt for StackDepthError {
    #[cold]
    #[inline(never)]
    fn into_flow(self, origin: BytecodeSlotOrigin) -> Flow {
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

/// Upper policy on the lowered [`BytecodeSlotError`] (signals through the
/// upper slot-origin policy).
pub(crate) trait BytecodeSlotErrorExt: Sized {
    fn into_flow(self, origin: BytecodeSlotOrigin) -> Flow;
}

impl BytecodeSlotErrorExt for BytecodeSlotError {
    #[cold]
    #[inline(never)]
    fn into_flow(self, origin: BytecodeSlotOrigin) -> Flow {
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
    #[doc(hidden)]
    pub fn into_flow(self) -> Flow {
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
