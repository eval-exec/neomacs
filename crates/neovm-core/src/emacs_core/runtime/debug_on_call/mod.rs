//! GNU's entry/exit debugger hooks: `debug-on-next-call` and `debug-on-exit`.
//!
//! These are one mechanism, not two.  `do_debug_on_call` (`src/eval.c:335-341`)
//! is four lines and it does *both* halves:
//!
//! ```c
//! void
//! do_debug_on_call (Lisp_Object code, specpdl_ref count)
//! {
//!   debug_on_next_call = 0;                                     /* 338 */
//!   set_backtrace_debug_on_exit (specpdl_ref_to_ptr (count), true); /* 339 */
//!   call_debugger (list1 (code));                               /* 340 */
//! }
//! ```
//!
//! Line 338 is the whole reason `debug-on-next-call` cannot be probed by
//! assignment: **setting it non-nil ARMS the debugger, and entering the
//! debugger CLEARS it again**, so the value a program reads back is `nil`
//! whenever a call intervened.  Line 339 is `debug-on-exit`: the frame that
//! the entry debugger fired on is also flagged to fire again on the way out.
//!
//! # Where GNU tests the arm
//!
//! Exactly three dispatch sites, and all three test it immediately after
//! `record_in_backtrace` so that line 339 has a frame to flag:
//!
//! | site | GNU | `code` |
//! |------|-----|--------|
//! | `eval_sub` | `src/eval.c:2601-2602` | `Qt` |
//! | `Ffuncall` | `src/eval.c:3189-3190` | `Qlambda` |
//! | `exec_byte_code`, `Bcall` | `src/bytecode.c:798-799` | `Qlambda` |
//!
//! `Fapply` has no check of its own: `apply1`/`Fapply` funnel into `Ffuncall`
//! (`src/eval.c:3192`), so `apply` is armed by the funcall site.  GNU's inline
//! bytecode opcodes (`Bcar`, `Bpoint`, … `bytecode.c:1412-1545`) are *not*
//! `Bcall` and are deliberately not gated -- which is why this port arms
//! `Op::Call` only, and never `Op::CallBuiltin`/`Op::CallBuiltinSym`.
//!
//! # Who else clears it
//!
//! `call_debugger` clears it a second time on entry (`src/eval.c:298`), and
//! `init_eval` clears it at startup (`src/eval.c:248`).  Nothing clears it on
//! the way *out* of a call: it is a one-shot that is spent the moment it is
//! observed.
//!
//! # The bad state, and why it is not spellable here
//!
//! An armed debugger that is still armed after the call it armed is the bad
//! state: the next call would enter the debugger again, and the next, forever.
//! GNU avoids it with an ordering convention -- clear first, then act.  A
//! convention is checkable, not enforceable, so this port makes the ordering
//! the *constructor*:
//!
//! * [`DebugOnCallArm`] has private fields and exactly one constructor,
//!   [`Context::take_debug_on_call_arm`], which performs the disarm before it
//!   returns the token.
//! * The token has exactly one consumer, [`Context::do_debug_on_call`], which
//!   takes it **by value**.
//! * There is no way to ask "is it armed?" that yields anything spendable:
//!   [`Context::debug_on_next_call_is_armed`] returns a bare `bool` and is only
//!   good for routing a call off a fast path.
//!
//! So "observed the arm but left it set" and "entered the debugger without
//! disarming" are both unconstructible rather than merely untested, and the
//! frame that line 339 flags is captured by the constructor from the specpdl
//! top rather than passed in -- GNU's `count` is always the frame
//! `record_in_backtrace` just returned, so "flagged the wrong frame" stops
//! being expressible too.

use super::error::Flow;
use super::eval::Context;
use super::forward::LispBoolFwd;
use super::intern::{SymId, intern};
use super::value::Value;

/// GNU's `code` argument to `do_debug_on_call` -- the single element of the
/// list handed to `debugger` when the *entry* debugger fires.
///
/// GNU spells it `Qt` or `Qlambda` at three literal call sites; an enum is the
/// same information with the site that produced it still attached, and makes
/// the two-valued domain checkable by the compiler instead of by grep.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DebugOnCallCode {
    /// `Qt` -- `eval_sub`'s check on a cons form (`src/eval.c:2602`).
    /// Reaches the debugger as `(t)`, which `debug.el` renders as
    /// "beginning evaluation of function call form".
    EvalForm,
    /// `Qlambda` -- `Ffuncall` (`src/eval.c:3190`) and the bytecode `Bcall`
    /// (`src/bytecode.c:799`).  Reaches the debugger as `(lambda)`, which
    /// `debug.el` renders as "entering a function".
    Funcall,
}

impl DebugOnCallCode {
    /// The Lisp object GNU conses into `list1 (code)`.
    pub(crate) fn value(self) -> Value {
        match self {
            Self::EvalForm => Value::T,
            Self::Funcall => Value::from_sym_id(lambda_code_symbol()),
        }
    }
}

/// A spent-on-construction permission to enter the entry debugger once.
///
/// Holding one of these is proof that `debug-on-next-call` has **already** been
/// cleared -- that is what its only constructor does before returning it.  See
/// the module docs for why that ordering is a type and not a comment.
#[must_use = "an arm that is taken and dropped disarms the debugger without \
              entering it, which loses a debugger entry GNU would have made"]
pub(crate) struct DebugOnCallArm {
    /// GNU's `code` argument (`src/eval.c:336`).
    code: DebugOnCallCode,
    /// GNU's `count` argument: the specpdl index of the backtrace frame this
    /// arm must flag `debug_on_exit` (`src/eval.c:339`).  Captured by the
    /// constructor as the specpdl top, because that is what `count` is at all
    /// three GNU sites.
    frame: usize,
}

fn debug_on_next_call_symbol() -> SymId {
    static SYMBOL: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| intern("debug-on-next-call"))
}

fn lambda_code_symbol() -> SymId {
    static SYMBOL: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| intern("lambda"))
}

fn exit_code_symbol() -> SymId {
    static SYMBOL: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| intern("exit"))
}

fn when_entered_debugger_symbol() -> SymId {
    static SYMBOL: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| intern("internal-when-entered-debugger"))
}

fn debugger_symbol() -> SymId {
    static SYMBOL: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| intern("debugger"))
}

impl Context {
    /// The `DEFVAR_BOOL` cell behind `debug-on-next-call` (`src/eval.c:4496`).
    ///
    /// GNU reads `globals.f_debug_on_next_call` -- a plain C `bool` the
    /// descriptor points at, never a symbol lookup -- so this reaches the same
    /// cell the same way: through the forwarder.  A `Localized` symbol keeps
    /// the descriptor inside its BLV (`make_blv`, `src/data.c:2112-2140`), and
    /// GNU's swap-in leaves the current buffer's value *in that same cell*, so
    /// reading it there is right for the buffer-local case too.
    #[inline]
    fn debug_on_next_call_cell(&self) -> Option<&'static LispBoolFwd> {
        self.obarray
            .debug_on_next_call_bool_fwd(debug_on_next_call_symbol())
    }

    /// GNU's bare `if (debug_on_next_call)` test, with no side effect.
    ///
    /// This is a routing predicate only: it cannot produce a
    /// [`DebugOnCallArm`], so it cannot enter the debugger and cannot leave the
    /// flag set.  The bytecode `Op::Call` arm uses it to steer off its
    /// zero-copy fast path onto the frame-recording path GNU's `Bcall` always
    /// takes, and then takes the arm properly there.
    #[inline]
    pub(crate) fn debug_on_next_call_is_armed(&self) -> bool {
        self.debug_on_next_call_cell().is_some_and(LispBoolFwd::get)
    }

    /// GNU `if (debug_on_next_call) do_debug_on_call (CODE, count)`'s test plus
    /// `do_debug_on_call`'s first line (`src/eval.c:338`), which is the disarm.
    ///
    /// Must be called with the frame `record_in_backtrace` just pushed on top
    /// of the specpdl -- that is GNU's `count` at all three sites.
    #[inline]
    pub(crate) fn take_debug_on_call_arm(
        &mut self,
        code: DebugOnCallCode,
    ) -> Option<DebugOnCallArm> {
        let cell = self.debug_on_next_call_cell()?;
        if !cell.get() {
            return None;
        }
        cell.set(false);
        Some(self.arm_for_specpdl_top(code))
    }

    #[cold]
    #[inline(never)]
    fn arm_for_specpdl_top(&self, code: DebugOnCallCode) -> DebugOnCallArm {
        let frame = self.specpdl.len().saturating_sub(1);
        debug_assert!(
            self.specpdl_entry_is_backtrace(frame),
            "an arm must be taken with the just-recorded backtrace frame on top \
             (GNU's `count` from record_in_backtrace)"
        );
        DebugOnCallArm { code, frame }
    }

    /// GNU `call_debugger`'s own clear (`src/eval.c:298`): entering the
    /// debugger for *any* reason disarms `debug-on-next-call`, so a debugger
    /// session does not re-enter itself on its first call.
    fn disarm_debug_on_next_call(&mut self) {
        if let Some(cell) = self.debug_on_next_call_cell() {
            cell.set(false);
        }
    }

    /// GNU `maybe_call_debugger`'s last conjunct:
    /// `when_entered_debugger < num_nonmacro_input_events` (`src/eval.c:2212`).
    ///
    /// The read-back half of [`Context::call_debugger`]'s stamp, and the reason
    /// the stamp exists at all: without it "the debugger itself signalled an
    /// error" is an infinite loop, which is what the two slots' shared comment
    /// says out loud (`src/eval.c:4544-4552`).  It gates the SIGNAL debugger
    /// only -- `do_debug_on_call` and the six `debug_on_exit` sites call
    /// `call_debugger` unconditionally, measured in both editors
    /// (`the_reentry_guard_gates_the_signal_debugger_only`).
    ///
    /// Both operands are `DEFVAR_INT` slots Lisp can `setq`
    /// (`src/eval.c:4554`, `src/keyboard.c:13903`), which is why they are read
    /// through their forwarders here: rewinding the stamp or bumping the
    /// counter from Lisp has to re-open the gate, exactly as it does in GNU.
    pub(crate) fn debugger_reentry_is_permitted(&self) -> bool {
        let when_entered = self
            .obarray
            .int_forwarder(when_entered_debugger_symbol())
            .map_or(i64::MIN, super::forward::LispIntFwd::get_i64);
        when_entered < self.num_nonmacro_input_events()
    }

    /// GNU `call_debugger` (`src/eval.c:281-333`): `apply1 (Vdebugger, arg)`
    /// under the debugger's own bindings, returning what the debugger returned.
    ///
    /// The value matters -- at every exit site GNU writes
    /// `val = call_debugger (list2 (Qexit, val))`, so a debugger that returns a
    /// different object *replaces the call's result*.
    pub(crate) fn call_debugger(&mut self, arg: Vec<Value>) -> Result<Value, Flow> {
        // eval.c:298, before anything the debugger runs can be dispatched.
        self.disarm_debug_on_next_call();
        // eval.c:299 `when_entered_debugger = num_nonmacro_input_events`.
        // GNU exposes the slot as the `DEFVAR_INT`
        // `internal-when-entered-debugger` (`src/eval.c:4553-4554`) and reads
        // it back in `maybe_call_debugger` (`src/eval.c:2212`) to refuse a
        // second debugger entry within one command --
        // [`Context::debugger_reentry_is_permitted`].  Measured under GNU
        // `-Q --batch`: it reads `-1` at startup and `0` after one entry.
        let events = self.num_nonmacro_input_events();
        self.obarray
            .set_symbol_value_id(when_entered_debugger_symbol(), Value::fixnum(events));
        let debugger = self
            .obarray
            .symbol_value_id(debugger_symbol())
            .copied()
            .unwrap_or(Value::NIL);
        let count = self.specpdl.len();
        // GNU's four `specbind`s, in GNU's order (`src/eval.c:306-314`).
        // `debugger-may-continue` is `debug_while_redisplaying ? Qnil : Qt`
        // there; this port has no redisplay re-entry to detect, and in batch
        // GNU answers `t` -- see the ledger for what a GUI probe would have to
        // establish before the conditional is worth porting.
        self.try_specbind_or_unwind_to(count, intern("debugger-may-continue"), Value::T)?;
        // eval.c:308.  "Resetting redisplaying_p to 0 makes sure that debug
        // output is displayed if the debugger is invoked during redisplay":
        // the debugger must be able to draw even when its caller had display
        // switched off.  Measured, `-Q --batch`, entering from inside
        // `(let ((inhibit-redisplay t)) ...)`: GNU reads `nil` in the debugger
        // and this port read `t` (`tmp/l183-p9.el`).
        self.try_specbind_or_unwind_to(count, intern("inhibit-redisplay"), Value::NIL)?;
        self.try_specbind_or_unwind_to(count, intern("inhibit-debugger"), Value::T)?;
        // eval.c:314, with GNU's own reason attached: "If we are debugging an
        // error while `inhibit-changing-match-data' is bound to non-nil (e.g.,
        // within a call to `string-match-p'), then make sure debugger code can
        // still use match data."  Measured: a `string-match` run inside the
        // debugger sets the match data in GNU and did not here.
        self.try_specbind_or_unwind_to(count, intern("inhibit-changing-match-data"), Value::NIL)?;
        let result = self.apply(debugger, arg);
        self.unbind_to_with_result(count, result)
    }

    /// GNU `do_debug_on_call` (`src/eval.c:335-341`) minus its first line,
    /// which the token's constructor already performed.
    pub(crate) fn do_debug_on_call(&mut self, arm: DebugOnCallArm) -> Result<(), Flow> {
        let DebugOnCallArm { code, frame } = arm;
        // eval.c:339 -- the same entry also arms the exit debugger.
        self.set_backtrace_debug_on_exit(frame, true);
        // eval.c:340 -- `call_debugger (list1 (code))`; the value is discarded.
        self.call_debugger(vec![code.value()]).map(|_| ())
    }

    /// GNU's six `if (backtrace_debug_on_exit (pdl)) val = call_debugger
    /// (list2 (Qexit, val));` sites -- `src/eval.c:2658`, `2777`, `3195`,
    /// `3318` and `src/bytecode.c:827`, `900` -- as one.
    ///
    /// The port pops every backtrace frame through `unbind_to_with_result`, and
    /// each of its fast paths already refuses a `debug_on_exit: true` frame
    /// (`trivial_spec_binding_pop`), so this is the only place such a frame can
    /// be popped and the six sites collapse to one without losing any of them.
    ///
    /// `Err` returns without calling the debugger, and that is GNU: a
    /// non-local exit unwinds `SPECPDL_BACKTRACE` with a bare `break`
    /// (`src/eval.c:3818-3820`), so a `throw` or `signal` out of a flagged
    /// frame is silent.  Measured against GNU: `(catch 'x (f))` where `f`
    /// flags its caller and throws calls the debugger zero times.
    pub(crate) fn run_debug_on_exit(
        &mut self,
        count: usize,
        result: Result<Value, Flow>,
    ) -> Result<Value, Flow> {
        let Ok(value) = result else {
            return result;
        };
        if !self.backtrace_frame_wants_debug_on_exit(count) {
            return Ok(value);
        }
        self.call_debugger(vec![Value::from_sym_id(exit_code_symbol()), value])
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
