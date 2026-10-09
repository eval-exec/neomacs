//! Sort's captured builtin predicate, with the ordinary funcall protocol.
#![deny(clippy::wildcard_enum_match_arm)]
use super::*;
use crate::emacs_core::builtins::higher_order::{NativeSortCall, SortPredicate};
use crate::tagged::header::{SubrFn2, SubrFnManySlice};

#[cfg(test)]
#[path = "tests/sort_stack_boundary_test.rs"]
mod sort_stack_boundary_tests;

/// Captured native bodies have distinct arity and pointer proofs. Naming the
/// body keeps validation and invocation exhaustive when a body kind is added.
#[derive(Clone, Copy, Debug)]
enum BufferedSortBody {
    NumericLessp,
    StringLessp,
}

/// A per-call proof that the selected native body cannot enter Lisp or collect.
/// Verification and invocation stay in the same mutator's uninterrupted begin;
/// no proof is cached across a callback, registration rewrite, or activation.
#[derive(Debug)]
#[repr(transparent)]
#[must_use = "consume this proof when invoking the trusted native body"]
struct VerifiedBufferedSortBody {
    kind: BufferedSortBody,
    _owner: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl VerifiedBufferedSortBody {
    #[inline]
    fn verify(kind: BufferedSortBody, entry: &SubrEntry) -> Option<Self> {
        if entry.dispatch_kind != SubrDispatchKind::Builtin {
            return None;
        }
        match kind {
            BufferedSortBody::NumericLessp => {
                let Some(SubrFn::ManySlice(actual)) = entry.function else {
                    return None;
                };
                if entry.min_args > 2
                    || entry.max_args.is_some_and(|maximum| maximum < 2)
                    || !std::ptr::fn_addr_eq(
                        actual,
                        builtins::builtin_num_lt_slice as SubrFnManySlice,
                    )
                {
                    return None;
                }
            }
            BufferedSortBody::StringLessp => {
                let Some(SubrFn::A2(actual)) = entry.function else {
                    return None;
                };
                if entry.min_args != 2
                    || entry.max_args != Some(2)
                    || !std::ptr::fn_addr_eq(
                        actual,
                        builtins::strings::builtin_string_lessp_2 as SubrFn2,
                    )
                {
                    return None;
                }
            }
        }
        Some(Self {
            kind,
            _owner: std::marker::PhantomData,
        })
    }

    /// The pointer proof identifies these exact bodies. Neither body invokes
    /// Lisp or collects; a native error constructs Flow for later publication.
    #[inline]
    fn apply(self, ctx: &mut Context, left: Value, right: Value) -> EvalResult {
        match self.kind {
            BufferedSortBody::NumericLessp => builtins::builtin_num_lt_slice(ctx, &[left, right]),
            BufferedSortBody::StringLessp => {
                builtins::strings::builtin_string_lessp_2(ctx, left, right)
            }
        }
    }
}

static_assertions::assert_not_impl_any!(VerifiedBufferedSortBody: Send, Sync, Clone, Copy);
const _: () = assert!(std::mem::size_of::<VerifiedBufferedSortBody>() == 1);

impl Context {
    /// A captured `string-lessp` implementation, including `string<` aliases.
    ///
    /// The owning sort roots SUBR in its invocation's root scope. This helper
    /// uses that mutator's Context and introduces no shared Lisp-state cache or
    /// single-mutator assumption. Registration's existing epoch guards remain
    /// authoritative, and the actual object is read after the funcall prologue.
    ///
    /// The prologue and unwind match `apply2_resolved_subr`; only the verified
    /// fixed-arity builtin body bypasses generic native dispatch. Redefinition,
    /// advice, debugger and GC-hook changes retain the captured object.
    #[inline]
    pub(crate) fn apply2_sort_string_lessp(
        &mut self,
        subr: Value,
        epoch: u64,
        arg0: Value,
        arg1: Value,
    ) -> EvalResult {
        self.maybe_quit_before_gc()?;
        self.enter_interpreted_eval_depth()?;
        let bt_count = self.specpdl.len();
        self.push_backtrace_frame(subr, &[arg0, arg1]);
        let result = {
            if self.gc_safe_point_exact_should_collect() {
                self.gc_collect_from_current_roots();
            }
            let entered = match self.take_debug_on_call_arm(DebugOnCallCode::Funcall) {
                Some(arm) => self.do_debug_on_call(arm),
                None => Ok(()),
            };
            match entered {
                Err(flow) => Err(flow),
                Ok(()) => self.maybe_grow_eval_stack(|ctx| {
                    let args = [arg0, arg1];
                    if ctx.obarray.function_epoch() != epoch
                        || ctx.compiler_function_overrides_active()
                    {
                        return ctx.funcall_general_untraced(subr, LispArgVec::from_slice(&args));
                    }
                    // Read the current object after GC/debugger callbacks, as
                    // the ordinary helper does. Callers do not need the unused
                    // interactive metadata's global registry lookup.
                    let Some((subr_sym, entry)) = subr_call_entry_from_value(subr) else {
                        return Err(signal(LispCondition::InvalidFunction, vec![subr]));
                    };
                    if entry.dispatch_kind == SubrDispatchKind::Builtin
                        && entry.min_args == 2
                        && entry.max_args == Some(2)
                        && let Some(SubrFn::A2(body)) = entry.function
                        && std::ptr::fn_addr_eq(
                            body,
                            builtins::strings::builtin_string_lessp_2 as SubrFn2,
                        )
                    {
                        return builtins::strings::builtin_string_lessp_2(ctx, arg0, arg1)
                            .map_err(|flow| ctx.validate_throw(flow));
                    }
                    ctx.apply_subr_object_with_entry(subr_sym, subr, &args, entry)
                }),
            }
        };
        self.depth -= 1;
        self.finish_traced_call(bt_count, result)
    }

    /// Begin a captured native comparison only when its ordinary funcall
    /// prologue cannot collect, enter Lisp, or signal. The ordinary Rust
    /// stack probe still runs before the proven native body; growing that
    /// stack cannot expose the private permutation to Lisp or collect.
    ///
    /// GNU eval.c:3194-3211 polls quit, increments depth, records the captured
    /// function, collects, and enters the debugger before calling its body.
    /// Every failed guard below returns before changing depth or publishing a
    /// frame. The caller must publish its permutation and roots before taking
    /// the ordinary callback path. No registry/body proof is shared between
    /// sort activations or mutators.
    ///
    /// A successful begin retains the ordinary frame until finish. A native
    /// error only constructs Flow here: the caller must publish live vector
    /// state and roots before finish can dispatch signal hooks or the debugger.
    #[inline]
    pub(crate) fn begin_buffered_native_sort_call(
        &mut self,
        predicate: SortPredicate,
        left: Value,
        right: Value,
    ) -> Option<NativeSortCall> {
        // Profiling lookup increments a call counter before validation. Keep
        // its ordinary dispatch path so a rejected fast begin never counts twice.
        #[cfg(feature = "vm-profile")]
        return None;
        let (subr, epoch, body) = match predicate {
            SortPredicate::NumericLessp { subr, epoch } => {
                (subr, epoch, BufferedSortBody::NumericLessp)
            }
            SortPredicate::StringLessp { subr, epoch } => {
                (subr, epoch, BufferedSortBody::StringLessp)
            }
            SortPredicate::ValueLt | SortPredicate::Generic(_) | SortPredicate::Subr { .. } => {
                return None;
            }
        };
        let entered_depth = self.depth.checked_add(1)?;
        if !self.attention_clear(super::AttentionMask::QUIT)
            || self.debug_on_next_call_is_armed()
            || self.obarray.max_lisp_eval_depth_localized
            || entered_depth > self.max_depth
            || self.obarray.function_epoch() != epoch
            || self.compiler_function_overrides_active()
        {
            return None;
        }
        // A conservative, read-only subset of the existing GC safe-point
        // predicate. Its slow tail refreshes settings, so do not call it while
        // values are still buffered and before deciding whether to publish.
        // Inhibited GC cannot run a hook; all other unusual GC states defer.
        if self.gc_inhibit_depth == 0
            && (self.tagged_heap.sweep_in_progress()
                || self.tagged_heap.mark_in_progress()
                || self.gc_pending
                || self.gc_stress
                || self.tagged_heap.gc_threshold_is_overridden()
                || self.tagged_heap.should_collect())
        {
            return None;
        }
        let (_, entry) = subr_call_entry_from_value(subr)?;
        let body = VerifiedBufferedSortBody::verify(body, &entry)?;

        // These guards prove the fast halves of enter_interpreted_eval_depth,
        // maybe_quit, maybe_gc, and debug-on-call. The native body is reached
        // with the same depth and backtrace as funcall (GNU sort.c:198-214,
        // eval.c:3194-3216). The ordinary Rust stack probe below cannot
        // collect or enter Lisp, so it needs no permutation publication.
        self.depth = entered_depth;
        let frame_base = self.specpdl.len();
        self.push_backtrace_frame(subr, &[left, right]);
        // Consume the per-call body proof across the ordinary Rust stack
        // probe; it permits no Lisp callback or collector entry on that stack.
        let result = self
            .maybe_grow_eval_stack(move |ctx| body.apply(ctx, left, right))
            .map_err(|flow| self.validate_throw(flow));
        Some(NativeSortCall { frame_base, result })
    }

    /// Finish exactly one successful begin after any required state/root
    /// publication. GNU eval.c:3213-3216 decrements depth before exit debugging
    /// and frame removal; finish_traced_call also preserves signal-hook order.
    #[inline]
    pub(crate) fn finish_buffered_native_sort_call(&mut self, call: NativeSortCall) -> EvalResult {
        self.depth -= 1;
        self.finish_traced_call(call.frame_base, call.result)
    }
}
