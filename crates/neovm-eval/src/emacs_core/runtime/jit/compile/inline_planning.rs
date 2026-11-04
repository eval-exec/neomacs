//! Compile-local admission and front selection for named inline regions.
//!
//! Threading: this helper runs within one compiler's existing scoped state.
//! It borrows the compiling mutator's source and obarray; returned Lisp handles
//! are rooted by the fused constant pool. It adds no runtime cache or shared
//! mutable Lisp state. Knob parsing remains in `knobs`.

use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use std::rc::Rc;

use super::{CompileRequest, Inline2Mode, active_numeric_feedback, jit_inline2_mode};
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::jit::inline::{self, FusedBody};
use neovm_value::symbol::Obarray;
use neovm_value::value::Value;

/// Combine the named admission hook with the separate self-recursive
/// Full allocator exception.
/// Threading: scalar compile facts and existing atomic source heat only.
pub(super) fn named_tier_eligible(
    source: &ByteCodeFunction,
    request: CompileRequest,
    self_recursive: bool,
) -> bool {
    super::knobs::named_inline_tier_eligible(source, request.origin)
        || (matches!(jit_inline2_mode(), Inline2Mode::Named | Inline2Mode::All)
            && super::lowering::active_regalloc_choice() == super::lowering::RegallocChoice::Full
            && self_recursive)
}

/// Run the shared v2 front before the backend decision. A named pass retains
/// static, closure and HOF candidates in mixed callers; if it selects none,
/// retain the existing static front. Off performs no scan or feedback copy.
/// Threading: all returned annotations are compiler-local immutable data.
pub(super) fn early_fused(
    source: &ByteCodeFunction,
    constants: &[Value],
    obarray: Option<&Obarray>,
    native_arity: usize,
    named_t2: bool,
) -> Option<Rc<FusedBody>> {
    if !jit_inline2_mode().enabled() || !inline::jit_inline_on() {
        return None;
    }
    let _phase = crate::emacs_core::jit::stats::enter_phase(
        crate::emacs_core::jit::stats::CompilePhase::Fuse,
    );
    let ops = source.executable_ops();
    let feedback: Vec<_> = (0..ops.len()).map(active_numeric_feedback).collect();
    let offset_map = source.executable_gnu_byte_offset_map();
    let named = named_t2.then(|| obarray).flatten().and_then(|obarray| {
        inline::fuse_named_calls_v2(ops, constants, offset_map, native_arity, &feedback, obarray)
    });
    named
        .or_else(|| inline::fuse_calls_v2(ops, constants, offset_map, native_arity, &feedback))
        .map(Rc::new)
}

/// The default profile's HOF-only front for a body the opt front leaves on
/// Legacy: `Some` only when a `mapc`/`mapcar` site with an admitted literal
/// callback was selected, so every other body keeps the Legacy compile.
/// Threading: compiler-local, as [`early_fused`].
pub(super) fn hof_only_fused(
    source: &ByteCodeFunction,
    constants: &[Value],
    native_arity: usize,
) -> Option<Rc<FusedBody>> {
    if !super::knobs::jit_hof_inline() || jit_inline2_mode().enabled() || !inline::jit_inline_on() {
        return None;
    }
    let ops = source.executable_ops();
    let _phase = crate::emacs_core::jit::stats::enter_phase(
        crate::emacs_core::jit::stats::CompilePhase::Fuse,
    );
    let feedback: Vec<_> = (0..ops.len()).map(active_numeric_feedback).collect();
    inline::fuse_hof_calls_v2(
        ops,
        constants,
        source.executable_gnu_byte_offset_map(),
        native_arity,
        &feedback,
    )
    .map(Rc::new)
}

/// Whether `source` has a `mapc`/`mapcar` site the default front's HOF-only
/// path would select, judged with fixnum feedback assumed (callers decide
/// once and cache it). Threading: compiler-local, as [`hof_only_fused`].
pub(crate) fn hof_caller_selectable(source: &ByteCodeFunction) -> bool {
    if !super::knobs::jit_hof_inline()
        || jit_inline2_mode().enabled()
        || !inline::jit_inline_on()
        || !inline::mentions_hof(&source.constants)
        || !source.constants.iter().any(|constant| {
            constant
                .get_bytecode_data()
                .is_some_and(is_hof_callback_candidate)
        })
    {
        return false;
    }
    let Ok(params) = super::param_shape::JitParamShape::try_from(source) else {
        return false;
    };
    let ops = source.executable_ops();
    let feedback = vec![crate::emacs_core::jit::NumericFeedback::FixnumOnly; ops.len()];
    inline::fuse_hof_calls_v2(
        ops,
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        params.entry_depth(),
        &feedback,
    )
    .is_some()
}

/// A constant that could be an admitted HOF callback (a literal lambda or a
/// `make-closure` template): the structural census verdict for one argument,
/// and no call. A necessary condition only, so callers whose lambdas call
/// something skip the front's CFG analysis.
fn is_hof_callback_candidate(callee: &ByteCodeFunction) -> bool {
    inline::census_callee_verdict(callee, 1).is_ok()
        && !callee.executable_ops().iter().any(|op| {
            matches!(
                op,
                crate::emacs_core::bytecode::Op::Call(_)
                    | crate::emacs_core::bytecode::Op::Apply(_)
            )
        })
}
