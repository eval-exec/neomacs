//! Deferred GNU inline-opcode error frames. The signal owns the arguments and
//! their GC pin until delivery transfers them to its mutator's specpdl. No
//! shared Lisp state or additional thread-local state is introduced.

use super::{Flow, FlowKind, InFlightRoots, SignalData};
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;

/// The inline opcodes whose operand errors GNU records as a deferred
/// backtrace frame (`record_in_backtrace`). Owned by this layer so the error
/// payload need not name the byte-code module; the VM/JIT classify their own
/// `Op` into these eight cases at the call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InlineErrorOp {
    Car,
    Cdr,
    Nth,
    Elt,
    Aref,
    Aset,
    Setcar,
    Setcdr,
}

/// Validated frame: only GNU's record_in_backtrace error sites can construct
/// one. Its owning pin prevents a deoptimized or native operand from dying
/// before the debugger runs. Existing specpdl unwinding owns the frame after
/// delivery, including condition-case and unwind-protect exits.
#[derive(Clone, Debug)]
pub(crate) struct BytecodeErrorBacktrace {
    function: Value,
    args: Vec<Value>,
    _pin: InFlightRoots,
}

static_assertions::assert_not_impl_any!(BytecodeErrorBacktrace: Send, Sync);

impl BytecodeErrorBacktrace {
    fn new(op: InlineErrorOp, args: &[Value], signal: &SignalData) -> Option<Self> {
        if signal.search_complete {
            return None;
        }
        let wrong_type = signal.symbol == intern("wrong-type-argument");
        let list_error = wrong_type
            && signal
                .data
                .first()
                .is_some_and(|value| *value == Value::symbol("listp"));
        let small_count = |value: Value| value.as_fixnum().is_some_and(|n| (0..=127).contains(&n));
        let (name, record) = match (op, args) {
            (InlineErrorOp::Car, [_]) => ("car", list_error),
            (InlineErrorOp::Cdr, [_]) => ("cdr", list_error),
            (InlineErrorOp::Nth, [n, _]) => ("nth", list_error && small_count(*n)),
            (InlineErrorOp::Elt, [seq, n]) => {
                ("elt", list_error && seq.is_cons() && small_count(*n))
            }
            (InlineErrorOp::Aref, [array, index]) | (InlineErrorOp::Aset, [array, index, _]) => {
                let record = (wrong_type && index.as_fixnum().is_none())
                    || (signal.symbol == intern("args-out-of-range")
                        && (array.is_vector() || array.is_record()));
                (
                    if matches!(op, InlineErrorOp::Aref) {
                        "aref"
                    } else {
                        "aset"
                    },
                    record,
                )
            }
            (InlineErrorOp::Setcar, [cell, _]) => ("setcar", wrong_type && !cell.is_cons()),
            (InlineErrorOp::Setcdr, [cell, _]) => ("setcdr", wrong_type && !cell.is_cons()),
            // GNU records no error frame for the other inline opcodes, or for
            // calls to fallback primitives outside these specific guards.
            _ => return None,
        };
        if !record {
            return None;
        }
        let function = Value::symbol(name);
        let pin = InFlightRoots::pin(std::iter::once(function).chain(args.iter().copied()));
        Some(Self {
            function,
            args: args.to_vec(),
            _pin: pin,
        })
    }

    pub(crate) fn record(self, ctx: &mut crate::emacs_core::eval::Context) {
        ctx.push_backtrace_frame(self.function, &self.args);
    }
}

impl Flow {
    /// Attach only on a cold opcode error. Leaves may attach while borrowing
    /// Context immutably; delivery records after their leaf witness is gone.
    #[cold]
    #[inline(never)]
    pub(crate) fn with_bytecode_error_backtrace(self, op: InlineErrorOp, args: &[Value]) -> Self {
        let mut kind = self.into_kind();
        if let FlowKind::Signal(signal) = &mut kind
            && signal.bytecode_error_backtrace.is_none()
        {
            signal.bytecode_error_backtrace = BytecodeErrorBacktrace::new(op, args, signal);
        }
        Self::from_kind(kind)
    }
}
