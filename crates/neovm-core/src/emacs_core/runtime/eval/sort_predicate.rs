//! Sort's captured builtin predicate, with the ordinary funcall protocol.
use super::*;
use crate::tagged::header::SubrFn2;

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
}
