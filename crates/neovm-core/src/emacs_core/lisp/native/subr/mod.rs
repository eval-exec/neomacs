//! Native Lisp subroutine declarations.
//!
//! A [`SubrSpec`] is the single declaration consumed by startup registration:
//! it keeps the Lisp name, native function shape, arity, dispatch kind, and
//! command metadata together.  The runtime representation remains the static
//! `SymId`-indexed registry described in the static-subr design; this module is
//! the declaration seam above it.

use super::interactive::BuiltinInteractiveSpec;
use super::intern::{SymId, intern};
use crate::tagged::header::{
    SubrDispatchKind, SubrFn, SubrFn0, SubrFn1, SubrFn2, SubrFn3, SubrFnMany, SubrFnManyNoContext,
    SubrFnManySlice,
};
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
thread_local! {
    static INSTALLED_SUBR_BATCHES: std::cell::RefCell<Vec<&'static str>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

/// Lisp-visible argument-count metadata for a native subroutine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SubrArity {
    min: u16,
    max: Option<u16>,
}

impl SubrArity {
    pub(crate) const fn new(min: u16, max: Option<u16>) -> Self {
        if let Some(max) = max {
            assert!(min <= max, "subr minimum arity exceeds its maximum");
        }
        Self { min, max }
    }

    pub(crate) const fn min(self) -> u16 {
        self.min
    }

    pub(crate) const fn max(self) -> Option<u16> {
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
pub(crate) enum NativeFn {
    ContextVec(SubrFnMany),
    ContextSlice(SubrFnManySlice),
    NoContextVec(SubrFnManyNoContext),
}

impl NativeFn {
    const fn into_subr_fn(self) -> SubrFn {
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
            const fn get(self) -> u16 {
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

/// Behavior used only by tests that exercise primitives without an evaluator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NoEvalPlaceholder {
    Nil,
    FixnumZero,
    WindowLineHeight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NoEvalPolicy {
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
    let index = sym_id.0 as usize;
    if policies.len() <= index {
        policies.resize(index + 1, None);
    }
    policies[index] = Some(policy);
}

pub(crate) fn no_eval_policy(sym_id: SymId) -> NoEvalPolicy {
    no_eval_policies()
        .lock()
        .expect("subr no-eval policy registry poisoned")
        .get(sym_id.0 as usize)
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
}

impl SubrSpec {
    /// Declare a vector/slice Rust entrypoint and its Lisp arity contract.
    pub(crate) const fn new(name: &'static str, function: NativeFn, arity: SubrArity) -> Self {
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

    const fn native(name: &'static str, function: SubrFn, arity: SubrArity) -> Self {
        assert!(!name.is_empty(), "a subr must have a Lisp name");
        Self {
            name,
            function: Some(function),
            arity,
            dispatch_kind: SubrDispatchKind::Builtin,
            interactive_spec: None,
            no_eval_policy: NoEvalPolicy::Native,
            command_default: CommandDefault::Enabled,
        }
    }

    pub(crate) const fn interactive(mut self, spec: BuiltinInteractiveSpec) -> Self {
        self.interactive_spec = Some(spec);
        self
    }

    pub(crate) const fn requires_eval_state(mut self) -> Self {
        self.no_eval_policy = NoEvalPolicy::RequiresEvalState;
        self
    }

    pub(crate) const fn placeholder(mut self, placeholder: NoEvalPlaceholder) -> Self {
        self.no_eval_policy = NoEvalPolicy::Placeholder(placeholder);
        self
    }

    pub(crate) const fn disabled_command(mut self) -> Self {
        self.command_default = CommandDefault::Disabled;
        self
    }

    pub(crate) const fn evaluator(
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
        }
    }

    pub(crate) const fn name(self) -> &'static str {
        self.name
    }

    pub(crate) const fn function(self) -> Option<SubrFn> {
        self.function
    }

    pub(crate) const fn arity(self) -> SubrArity {
        self.arity
    }

    pub(crate) const fn dispatch_kind(self) -> SubrDispatchKind {
        self.dispatch_kind
    }

    pub(crate) const fn interactive_spec(self) -> Option<BuiltinInteractiveSpec> {
        self.interactive_spec
    }

    pub(crate) const fn no_eval_policy(self) -> NoEvalPolicy {
        self.no_eval_policy
    }

    pub(crate) const fn command_default(self) -> CommandDefault {
        self.command_default
    }
}

/// One subsystem's compiled, executable native-subr catalog.
///
/// The declaration macro supplies `module_path!()`, while the constructor
/// obtains its unforgeable call site from [`std::panic::Location`]. Construction
/// is const-evaluated, so a localized batch declared outside a sibling
/// `subrs.rs` fails compilation instead of relying on a Rust-source parser.
#[derive(Clone, Copy)]
pub struct SubrBatch {
    #[cfg(test)]
    source_file: &'static str,
    #[cfg(test)]
    owner: &'static str,
    specs: &'static [SubrSpec],
    portability: SubrPortability,
}

/// Whether a compiled primitive may be required by a cross-target image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SubrPortability {
    /// Every supported target compiles a compatible implementation.
    AllTargets,
    /// Availability or ABI varies by compilation target, so a portable image
    /// may carry the function cell but cannot require it from every consumer.
    TargetSpecific,
}

impl SubrBatch {
    #[track_caller]
    pub const fn new(owner: &'static str, specs: &'static [SubrSpec]) -> Self {
        Self::new_inner(owner, specs, false, SubrPortability::AllTargets)
    }

    /// Construct a batch backed only by native product hosts.
    ///
    /// Such a catalog may be empty after target filtering. Its function cells
    /// may appear in a portable image, but a Wasm consumer cannot require them.
    #[track_caller]
    pub const fn native_host(owner: &'static str, specs: &'static [SubrSpec]) -> Self {
        Self::new_inner(owner, specs, true, SubrPortability::TargetSpecific)
    }

    #[track_caller]
    const fn new_inner(
        owner: &'static str,
        specs: &'static [SubrSpec],
        permit_empty: bool,
        portability: SubrPortability,
    ) -> Self {
        let source_file = std::panic::Location::caller().file();
        assert!(
            is_subrs_source_file(source_file),
            "localized subr catalogs must be declared in subrs.rs"
        );
        assert!(!owner.is_empty(), "a subr catalog must have an owner");
        assert!(
            permit_empty || !specs.is_empty(),
            "a subr catalog must not be empty"
        );
        Self {
            #[cfg(test)]
            source_file,
            #[cfg(test)]
            owner,
            specs,
            portability,
        }
    }

    #[cfg(test)]
    pub(crate) const fn source_file(self) -> &'static str {
        self.source_file
    }

    #[cfg(test)]
    pub(crate) const fn owner(self) -> &'static str {
        self.owner
    }

    #[cfg(test)]
    pub(crate) const fn specs(self) -> &'static [SubrSpec] {
        self.specs
    }

    #[cfg(test)]
    pub(crate) const fn portability(self) -> SubrPortability {
        self.portability
    }

    pub(crate) fn install(self, ctx: &mut crate::emacs_core::eval::Context) {
        #[cfg(test)]
        INSTALLED_SUBR_BATCHES.with(|installed| installed.borrow_mut().push(self.owner));
        ctx.register_subrs_with_portability(self.specs, self.portability);
    }
}

#[cfg(test)]
pub(crate) fn reset_installed_subr_batches() {
    INSTALLED_SUBR_BATCHES.with(|installed| installed.borrow_mut().clear());
}

#[cfg(test)]
pub(crate) fn take_installed_subr_batches() -> Vec<&'static str> {
    INSTALLED_SUBR_BATCHES.with(|installed| std::mem::take(&mut *installed.borrow_mut()))
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
macro_rules! define_subrs {
    (native_host; $($spec:expr),+ $(,)?) => {
        pub(crate) const SUBRS: $crate::emacs_core::subr::SubrBatch =
            $crate::emacs_core::subr::SubrBatch::native_host(
                module_path!(),
                &[$($spec),+],
            );

        pub(crate) fn register_subrs(ctx: &mut $crate::emacs_core::eval::Context) {
            SUBRS.install(ctx);
        }
    };
    ($($spec:expr),+ $(,)?) => {
        pub(crate) const SUBRS: $crate::emacs_core::subr::SubrBatch =
            $crate::emacs_core::subr::SubrBatch::new(
                module_path!(),
                &[$($spec),+],
            );

        pub(crate) fn register_subrs(
            ctx: &mut $crate::emacs_core::eval::Context,
        ) {
            SUBRS.install(ctx);
        }
    };
}

pub(crate) use define_subrs;

#[cfg(test)]
mod tests;
