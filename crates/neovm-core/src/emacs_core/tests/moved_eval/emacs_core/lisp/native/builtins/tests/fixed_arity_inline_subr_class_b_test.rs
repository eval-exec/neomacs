//! Full-runtime inline-subr registration belongs to the core harness.
//!
//! The real canonical Context factory is supplied through the authority
//! driver; eval retains private classification state and unchanged assertions.

/// Registration classifies each builtin the way GNU `exec_byte_code`
/// treats it: pure buffer reads run without a call, small fixed-arity subrs
/// are called straight off the stack, and everything else takes the
/// by-symbol dispatch.
#[test]
fn inline_subr_kinds_follow_the_gnu_inline_opcode_shape() {
    crate::test_utils::init_test_tracing();
    neovm_eval::test_support::authority::run_inline_subr_kinds_follow_the_gnu_inline_opcode_shape_for_test(
        crate::test_utils::new_context,
    );
}
