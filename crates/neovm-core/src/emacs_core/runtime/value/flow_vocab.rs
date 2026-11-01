//! Flow vocabulary shared by the error carrier and the evaluator.
//!
//! Both types are pure data: primitives only, no Lisp values and no `Context`.
//! They live in the value layer so the error carrier (`runtime/error`) can
//! name them without reaching up into `eval`; `eval` re-exports both, so every
//! existing `eval::…` path keeps meaning.
//!
//! [`ShutdownRequest`] is the recorded state between `kill-emacs` and the
//! process exit. GNU's `Fkill_emacs` is `noreturn` and exits from inside the
//! subr; this port has to walk control back out to `main`, so the request is a
//! value. [`ResumeTarget`] names where a nonlocal exit resumes; only indices,
//! ids and lengths describe it, never runtime state.

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ResumeTarget {
    CommandLoopExit,
    CommandLoopTopLevel,
    InterpreterCatch,
    InterpreterConditionCase {
        handler_index: usize,
        condition_stack_base: usize,
    },
    /// GNU safe_funcall's Qt barrier. The index identifies a live handler
    /// on its owning Context's mutator; it is never shared across Contexts.
    SafeFuncall {
        condition_stack_base: usize,
    },
    /// GNU `menu_item_eval_property`'s Qerror barrier. Unlike the Qt
    /// `SafeFuncall` barrier, only `error` and its subclasses are caught:
    /// `quit` and throws with an outer catch keep unwinding.
    MenuItemProperty {
        condition_stack_base: usize,
    },
    VmCatch {
        resume_id: u64,
        target: u32,
        stack_len: usize,
        spec_depth: usize,
        bind_stack_len: usize,
    },
    VmConditionCase {
        resume_id: u64,
        target: u32,
        stack_len: usize,
        spec_depth: usize,
        bind_stack_len: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShutdownRequest {
    pub exit_code: i32,
    pub restart: bool,
}
