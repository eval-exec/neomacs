//! Native Lisp subroutine declarations.
//!
//! A [`SubrSpec`] is the single declaration consumed by startup registration:
//! it keeps the Lisp name, native function shape, arity, dispatch kind, and
//! command metadata together.  The runtime representation remains the static
//! `SymId`-indexed registry described in the static-subr design; this module is
//! the declaration seam above it.

use crate::emacs_core::eval::interactive_registry::BuiltinInteractiveSpec;
use crate::emacs_core::eval::subr_abi::{
    SubrFn, SubrFn0, SubrFn1, SubrFn2, SubrFn3, SubrFn4, SubrFn5, SubrFn6, SubrFnMany,
    SubrFnManyNoContext, SubrFnManySlice,
};
use neovm_value::intern::{SymId, intern};
use neovm_value::tagged::header::SubrDispatchKind;
use std::sync::{Mutex, OnceLock};

pub(crate) mod effect_catalog;
pub(crate) mod effect_parallel;
pub(crate) mod effect_reentry;
#[doc(hidden)]
pub mod effect_schema;
pub(crate) mod leaf;

/// Explicit serial-only declaration. The projection changes neither SubrSpec
/// nor its executable registration; the architecture inventory consumes this marker.
#[doc(hidden)]
pub struct WorldSubr(SubrSpec);

impl WorldSubr {
    #[doc(hidden)]
    pub const fn new(spec: SubrSpec) -> Self {
        Self(spec)
    }

    #[doc(hidden)]
    pub const fn serial(self) -> SubrSpec {
        self.0
    }

    #[doc(hidden)]
    pub const fn declaration(self) -> effect_schema::SchemaEntry<'static> {
        effect_schema::SchemaEntry::classified(
            effect_schema::CatalogKind::Primitive,
            self.0.name,
            effect_schema::Authority::World,
        )
    }

    #[doc(hidden)]
    pub const fn batch_declarations<const N: usize>(
        specs: &[SubrSpec; N],
    ) -> [effect_schema::SchemaEntry<'static>; N] {
        use effect_schema::{Authority, CatalogKind, SchemaEntry};
        let mut rows = [SchemaEntry::classified(CatalogKind::Primitive, "", Authority::World); N];
        let mut index = 0;
        while index < N {
            rows[index] = Self::new(specs[index]).declaration();
            index += 1;
        }
        rows
    }
}

/// An explicit typed World declaration, const-erased to the original serial
/// spec. No effect metadata enters SubrEntry, TLS, or a dispatch call site.
#[doc(hidden)]
#[macro_export]
macro_rules! world_subr {
    ($spec:expr) => {
        const { $crate::emacs_core::subr::WorldSubr::new($spec).serial() }
    };
}
#[doc(hidden)]
pub use world_subr;

/// Classify a complete legacy registry as World-only without changing the enum
/// representation or emitting runtime dispatch. The generator requires this
/// explicit declaration before it emits that registry's exhaustive sidecar.
macro_rules! world_registry {
    ($registry:ty) => {
        const _: () = {
            let _ = std::mem::size_of::<$registry>();
        };
    };
}
pub(crate) use world_registry;

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    static INSTALLED_SUBR_BATCHES: std::cell::RefCell<Vec<&'static str>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

/// One entry in the armed registration trace.
///
/// The trace is the canonical production registration sequence: every subr
/// registration at the choke point records a [`SubrRegistration::Subr`], and
/// the one production `initialize_event_properties` step — which registers no
/// subr but sits between the upper declarations and the public evaluator
/// batch — records [`SubrRegistration::EventSymbolProperties`].
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum SubrRegistration {
    Subr {
        name: &'static str,
        min: u16,
        max: Option<u16>,
    },
    EventSymbolProperties,
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    /// `None` = disarmed: registrations are not recorded. The test arms it
    /// with an empty vector and drains-and-disarms; there is no unbounded
    /// side channel in an ordinary test run.
    static REGISTERED_SUBR_TRACE: std::cell::RefCell<Option<Vec<SubrRegistration>>> =
        const { std::cell::RefCell::new(None) };
}

/// Lisp-visible argument-count metadata for a native subroutine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct SubrArity {
    min: u16,
    max: Option<u16>,
}

impl SubrArity {
    #[doc(hidden)]
    pub const fn new(min: u16, max: Option<u16>) -> Self {
        if let Some(max) = max {
            assert!(min <= max, "subr minimum arity exceeds its maximum");
        }
        Self { min, max }
    }

    #[doc(hidden)]
    pub const fn min(self) -> u16 {
        self.min
    }

    #[doc(hidden)]
    pub const fn max(self) -> Option<u16> {
        self.max
    }
}

/// Vector/slice calling convention for a native Lisp subroutine.
///
/// Vector entrypoints may implement either a fixed or unbounded Lisp arity, so
/// their [`SubrArity`] remains an independent declaration. Fixed-slot
/// entrypoints use [`SubrSpec::fixed0`], [`SubrSpec::fixed1`],
/// [`SubrSpec::fixed2`], or [`SubrSpec::fixed3`] instead: those constructors
/// derive the maximum arity from the Rust function-pointer type.
#[derive(Clone, Copy)]
#[doc(hidden)]
pub enum NativeFn {
    ContextVec(SubrFnMany),
    ContextSlice(SubrFnManySlice),
    NoContextVec(SubrFnManyNoContext),
}

impl NativeFn {
    #[doc(hidden)]
    pub const fn into_subr_fn(self) -> SubrFn {
        match self {
            Self::ContextVec(function) => SubrFn::Many(function),
            Self::ContextSlice(function) => SubrFn::ManySlice(function),
            Self::NoContextVec(function) => SubrFn::ManyNoContext(function),
        }
    }
}

macro_rules! fixed_minimum {
    ($name:ident { $($variant:ident = $value:literal),+ $(,)? }) => {
        /// Valid required-argument counts for the corresponding fixed-slot
        /// native entrypoint.
        #[allow(dead_code)]
        #[repr(u16)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name {
            $($variant = $value),+
        }

        impl $name {
            #[doc(hidden)]
            pub const fn get(self) -> u16 {
                self as u16
            }
        }
    };
}

fixed_minimum!(FixedMin1 { Zero = 0, One = 1 });
fixed_minimum!(FixedMin2 {
    Zero = 0,
    One = 1,
    Two = 2,
});
fixed_minimum!(FixedMin3 {
    Zero = 0,
    One = 1,
    Two = 2,
    Three = 3,
});
fixed_minimum!(FixedMin4 {
    Zero = 0,
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
});
fixed_minimum!(FixedMin5 {
    Zero = 0,
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
});
fixed_minimum!(FixedMin6 {
    Zero = 0,
    One = 1,
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
});

/// Behavior used only by tests that exercise primitives without an evaluator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum NoEvalPlaceholder {
    Nil,
    FixnumZero,
    WindowLineHeight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum NoEvalPolicy {
    Native,
    RequiresEvalState,
    Placeholder(NoEvalPlaceholder),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandDefault {
    Enabled,
    Disabled,
}

static NO_EVAL_POLICIES: OnceLock<Mutex<Vec<Option<NoEvalPolicy>>>> = OnceLock::new();

fn no_eval_policies() -> &'static Mutex<Vec<Option<NoEvalPolicy>>> {
    NO_EVAL_POLICIES.get_or_init(|| Mutex::new(Vec::new()))
}

pub(crate) fn record_no_eval_policy(name: &str, policy: NoEvalPolicy) {
    let sym_id = intern(name);
    let mut policies = no_eval_policies()
        .lock()
        .expect("subr no-eval policy registry poisoned");
    let index = sym_id.to_raw() as usize;
    if policies.len() <= index {
        policies.resize(index + 1, None);
    }
    policies[index] = Some(policy);
}

pub(crate) fn no_eval_policy(sym_id: SymId) -> NoEvalPolicy {
    no_eval_policies()
        .lock()
        .expect("subr no-eval policy registry poisoned")
        .get(sym_id.to_raw() as usize)
        .copied()
        .flatten()
        .unwrap_or(NoEvalPolicy::Native)
}

/// Complete startup declaration for one Rust-backed Lisp function.
#[derive(Clone, Copy)]
pub struct SubrSpec {
    name: &'static str,
    function: Option<SubrFn>,
    arity: SubrArity,
    dispatch_kind: SubrDispatchKind,
    interactive_spec: Option<BuiltinInteractiveSpec>,
    no_eval_policy: NoEvalPolicy,
    command_default: CommandDefault,
    /// The builtin's leaf entry for `Op::Call` sites (`leaf::LeafShape::Bcall`).
    leaf: Option<&'static leaf::LeafSpec>,
}

impl SubrSpec {
    /// Declare a vector/slice Rust entrypoint and its Lisp arity contract.
    #[doc(hidden)]
    pub const fn new(name: &'static str, function: NativeFn, arity: SubrArity) -> Self {
        assert!(!name.is_empty(), "a subr must have a Lisp name");
        Self::native(name, function.into_subr_fn(), arity)
    }

    /// Declare a zero-slot Rust entrypoint. Its Lisp arity is exactly zero.
    pub const fn fixed0(name: &'static str, function: SubrFn0) -> Self {
        Self::native(name, SubrFn::A0(function), SubrArity::new(0, Some(0)))
    }

    /// Declare a one-slot Rust entrypoint. The maximum arity is derived from
    /// the function-pointer type; `minimum` is closed over the valid range.
    pub const fn fixed1(name: &'static str, function: SubrFn1, minimum: FixedMin1) -> Self {
        Self::native(
            name,
            SubrFn::A1(function),
            SubrArity::new(minimum.get(), Some(1)),
        )
    }

    /// Declare a two-slot Rust entrypoint with a type-checked maximum arity.
    pub const fn fixed2(name: &'static str, function: SubrFn2, minimum: FixedMin2) -> Self {
        Self::native(
            name,
            SubrFn::A2(function),
            SubrArity::new(minimum.get(), Some(2)),
        )
    }

    /// Declare a three-slot Rust entrypoint with a type-checked maximum arity.
    pub const fn fixed3(name: &'static str, function: SubrFn3, minimum: FixedMin3) -> Self {
        Self::native(
            name,
            SubrFn::A3(function),
            SubrArity::new(minimum.get(), Some(3)),
        )
    }
    pub const fn fixed4(name: &'static str, function: SubrFn4, minimum: FixedMin4) -> Self {
        Self::native(
            name,
            SubrFn::A4(function),
            SubrArity::new(minimum.get(), Some(4)),
        )
    }
    pub const fn fixed5(name: &'static str, function: SubrFn5, minimum: FixedMin5) -> Self {
        Self::native(
            name,
            SubrFn::A5(function),
            SubrArity::new(minimum.get(), Some(5)),
        )
    }
    pub const fn fixed6(name: &'static str, function: SubrFn6, minimum: FixedMin6) -> Self {
        Self::native(
            name,
            SubrFn::A6(function),
            SubrArity::new(minimum.get(), Some(6)),
        )
    }

    #[doc(hidden)]
    pub const fn native(name: &'static str, function: SubrFn, arity: SubrArity) -> Self {
        assert!(!name.is_empty(), "a subr must have a Lisp name");
        Self {
            name,
            function: Some(function),
            arity,
            dispatch_kind: SubrDispatchKind::Builtin,
            interactive_spec: None,
            no_eval_policy: NoEvalPolicy::Native,
            command_default: CommandDefault::Enabled,
            leaf: None,
        }
    }

    #[doc(hidden)]
    pub const fn interactive(mut self, spec: BuiltinInteractiveSpec) -> Self {
        self.interactive_spec = Some(spec);
        self
    }

    #[doc(hidden)]
    pub const fn requires_eval_state(mut self) -> Self {
        self.no_eval_policy = NoEvalPolicy::RequiresEvalState;
        self
    }

    #[doc(hidden)]
    pub const fn placeholder(mut self, placeholder: NoEvalPlaceholder) -> Self {
        self.no_eval_policy = NoEvalPolicy::Placeholder(placeholder);
        self
    }

    #[doc(hidden)]
    pub const fn disabled_command(mut self) -> Self {
        self.command_default = CommandDefault::Disabled;
        self
    }

    /// Attach the builtin's leaf entry (`subr::leaf`), which `Op::Call`
    /// sites on this builtin may call instead of the protocol call. Checked
    /// at compile time: a Bcall-shaped leaf of this builtin's name, taking
    /// every fixed argument slot the builtin has (missing optionals arrive
    /// as nil in both).
    #[doc(hidden)]
    pub const fn leaf(mut self, leaf: &'static leaf::LeafSpec) -> Self {
        assert!(
            matches!(leaf.shape, leaf::LeafShape::Bcall),
            "only a Bcall-shaped leaf answers as the registered builtin"
        );
        assert!(
            const_str_eq(leaf.name, self.name),
            "a leaf is attached to the builtin it names"
        );
        assert!(
            matches!(self.dispatch_kind, SubrDispatchKind::Builtin),
            "only a plain builtin has a leaf"
        );
        let slots = match self.function {
            Some(SubrFn::A1(_)) => 1,
            Some(SubrFn::A2(_)) => 2,
            Some(SubrFn::A3(_)) => 3,
            _ => panic!("a leaf needs a fixed-arity builtin of one to three slots"),
        };
        assert!(
            leaf.entry.slots() == slots,
            "the leaf takes exactly the builtin's argument slots"
        );
        self.leaf = Some(leaf);
        self
    }

    #[doc(hidden)]
    pub const fn evaluator(
        name: &'static str,
        arity: SubrArity,
        dispatch_kind: SubrDispatchKind,
    ) -> Self {
        assert!(
            matches!(
                dispatch_kind,
                SubrDispatchKind::ContextCallable | SubrDispatchKind::SpecialForm
            ),
            "evaluator subrs require an evaluator-owned dispatch kind"
        );
        Self {
            name,
            function: None,
            arity,
            dispatch_kind,
            interactive_spec: None,
            no_eval_policy: NoEvalPolicy::RequiresEvalState,
            command_default: CommandDefault::Enabled,
            leaf: None,
        }
    }

    #[doc(hidden)]
    pub const fn name(self) -> &'static str {
        self.name
    }

    #[doc(hidden)]
    pub const fn function(self) -> Option<SubrFn> {
        self.function
    }

    #[doc(hidden)]
    pub const fn arity(self) -> SubrArity {
        self.arity
    }

    #[doc(hidden)]
    pub const fn dispatch_kind(self) -> SubrDispatchKind {
        self.dispatch_kind
    }

    #[doc(hidden)]
    pub const fn interactive_spec(self) -> Option<BuiltinInteractiveSpec> {
        self.interactive_spec
    }

    #[doc(hidden)]
    pub const fn no_eval_policy(self) -> NoEvalPolicy {
        self.no_eval_policy
    }

    #[doc(hidden)]
    pub const fn command_default(self) -> CommandDefault {
        self.command_default
    }

    #[doc(hidden)]
    pub const fn leaf_spec(self) -> Option<&'static leaf::LeafSpec> {
        self.leaf
    }
}

const fn const_str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// One subsystem's compiled, executable native-subr catalog.
///
/// The declaration macro supplies `module_path!()`, while the constructor
/// obtains its unforgeable call site from [`std::panic::Location`]. Construction
/// is const-evaluated, so a localized batch declared outside a sibling
/// `subrs.rs` fails compilation instead of relying on a Rust-source parser.
#[derive(Clone, Copy)]
pub struct SubrBatch {
    #[cfg(any(test, feature = "test-support"))]
    source_file: &'static str,
    #[cfg(any(test, feature = "test-support"))]
    owner: &'static str,
    specs: &'static [SubrSpec],
}

impl SubrBatch {
    #[track_caller]
    pub const fn new(owner: &'static str, specs: &'static [SubrSpec]) -> Self {
        let source_file = std::panic::Location::caller().file();
        assert!(
            is_subrs_source_file(source_file),
            "localized subr catalogs must be declared in subrs.rs"
        );
        assert!(!owner.is_empty(), "a subr catalog must have an owner");
        assert!(!specs.is_empty(), "a subr catalog must not be empty");
        Self {
            #[cfg(any(test, feature = "test-support"))]
            source_file,
            #[cfg(any(test, feature = "test-support"))]
            owner,
            specs,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub const fn source_file(self) -> &'static str {
        self.source_file
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub const fn owner(self) -> &'static str {
        self.owner
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub const fn specs(self) -> &'static [SubrSpec] {
        self.specs
    }

    #[doc(hidden)]
    pub fn install(self, ctx: &mut crate::emacs_core::eval::Context) {
        #[cfg(any(test, feature = "test-support"))]
        INSTALLED_SUBR_BATCHES.with(|installed| installed.borrow_mut().push(self.owner));
        ctx.register_subrs(self.specs);
    }
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn reset_installed_subr_batches() {
    INSTALLED_SUBR_BATCHES.with(|installed| installed.borrow_mut().clear());
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn take_installed_subr_batches() -> Vec<&'static str> {
    INSTALLED_SUBR_BATCHES.with(|installed| std::mem::take(&mut *installed.borrow_mut()))
}

/// Record one registration at the single choke point (H3): the canonical
/// order+arity trace the core-side names+order test compares against the
/// eval registrar plus the checked-in upper snapshot. A no-op while the
/// trace is disarmed.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn record_registered_subr(name: &'static str, arity: SubrArity) {
    REGISTERED_SUBR_TRACE.with(|trace| {
        if let Some(trace) = trace.borrow_mut().as_mut() {
            trace.push(SubrRegistration::Subr {
                name,
                min: arity.min(),
                max: arity.max(),
            });
        }
    });
}

/// Record the production `initialize_event_properties` step.
///
/// The step registers no subr; it initializes event symbol properties between
/// the upper compatibility declarations and the public evaluator batch. The
/// plan and the T6 order gate need that position in the trace, so the
/// production path records the marker while the trace is armed.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn record_event_symbol_properties_step() {
    REGISTERED_SUBR_TRACE.with(|trace| {
        if let Some(trace) = trace.borrow_mut().as_mut() {
            trace.push(SubrRegistration::EventSymbolProperties);
        }
    });
}

/// Arm the trace with an empty vector (from now on, registrations record).
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn reset_registered_subr_trace_for_test() {
    REGISTERED_SUBR_TRACE.with(|trace| *trace.borrow_mut() = Some(Vec::new()));
}

/// Drain the trace and disarm it, returning what was recorded since the
/// last reset. An unarmed trace yields an empty vector.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub fn take_registered_subr_trace_for_test() -> Vec<SubrRegistration> {
    REGISTERED_SUBR_TRACE.with(|trace| trace.borrow_mut().take().unwrap_or_default())
}

const fn is_subrs_source_file(path: &str) -> bool {
    const NAME: &[u8] = b"subrs.rs";
    let bytes = path.as_bytes();
    if bytes.len() < NAME.len() {
        return false;
    }
    let start = bytes.len() - NAME.len();
    let mut index = 0;
    while index < NAME.len() {
        if bytes[start + index] != NAME[index] {
            return false;
        }
        index += 1;
    }
    start == 0 || bytes[start - 1] == b'/' || bytes[start - 1] == b'\\'
}

/// Define a localized declaration catalog and its only registrar from the
/// same const data. This makes the compiled catalog—not syntax inferred by an
/// architecture test—the source of truth for installation.
#[doc(hidden)]
#[macro_export]
macro_rules! define_subrs {
    ($($spec:expr),+ $(,)?) => {
        #[doc(hidden)]
        pub const SUBRS: $crate::emacs_core::subr::SubrBatch =
            $crate::emacs_core::subr::SubrBatch::new(
                module_path!(),
                &[$($spec),+],
            );

        // Apply cfg attributes to array elements before deriving declarations.
        const _: () = $crate::emacs_core::subr::effect_schema::assert_valid(
            &$crate::emacs_core::subr::WorldSubr::batch_declarations(&[$($spec),+]),
        );

        #[doc(hidden)]
        pub fn register_subrs(
            ctx: &mut $crate::emacs_core::eval::Context,
        ) {
            SUBRS.install(ctx);
        }
    };
}

#[doc(hidden)]
pub use define_subrs;

#[cfg(test)]
#[path = "tests/subr_test.rs"]
mod tests;
