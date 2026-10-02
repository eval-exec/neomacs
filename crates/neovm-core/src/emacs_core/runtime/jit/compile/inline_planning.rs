//! Compile-local admission and front selection for named inline regions.
//!
//! Threading: this helper runs within one compiler's existing scoped state.
//! It borrows the compiling mutator's source and obarray; returned Lisp handles
//! are rooted by the fused constant pool. It adds no runtime cache or shared
//! mutable Lisp state. Knob parsing remains in `knobs`.

use std::rc::Rc;

use super::{CompileRequest, Inline2Mode, active_numeric_feedback, jit_inline2_mode};
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::jit::inline::{self, FusedBody};
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::Value;

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
