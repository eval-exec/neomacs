//! Assoc's two-argument callback seam, preserving GNU's live function cells.
use super::*;
use crate::tagged::header::SubrFn2;

/// A pure builtin identified by its implementation, rather than its name.
/// This immutable enum contains no Lisp state and is safe to share between
/// mutators; each invocation reads its own Context's comparison options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PureAssocPredicate {
    Eq,
    Eql,
    Equal,
    EqualIncludingProperties,
    StringEqual,
}

impl PureAssocPredicate {
    #[inline]
    fn body(self) -> SubrFn2 {
        match self {
            Self::Eq => builtins::types::builtin_eq_2,
            Self::Eql => builtins::types::builtin_eql_2,
            Self::Equal => builtins::types::builtin_equal_2,
            Self::EqualIncludingProperties => {
                crate::emacs_core::fns::builtin_equal_including_properties_2
            }
            Self::StringEqual => builtins::strings::builtin_string_equal_2,
        }
    }

    fn from_body(body: SubrFn2) -> Option<Self> {
        [
            Self::Eq,
            Self::Eql,
            Self::Equal,
            Self::EqualIncludingProperties,
            Self::StringEqual,
        ]
        .into_iter()
        .find(|predicate| std::ptr::fn_addr_eq(body, predicate.body()))
    }

    #[inline]
    fn compare(self, ctx: &mut Context, arg0: Value, arg1: Value) -> EvalResult {
        match self {
            Self::Eq => builtins::types::builtin_eq_2(ctx, arg0, arg1),
            Self::Eql => builtins::types::builtin_eql_2(ctx, arg0, arg1),
            Self::Equal => builtins::types::builtin_equal_2(ctx, arg0, arg1),
            Self::EqualIncludingProperties => {
                crate::emacs_core::fns::builtin_equal_including_properties_2(ctx, arg0, arg1)
            }
            Self::StringEqual => builtins::strings::builtin_string_equal_2(ctx, arg0, arg1),
        }
    }
}

/// A resolved call's implementation family, immutable and free of Lisp state.
/// Each mutator dispatches only values rooted in its own invocation.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AssocTarget {
    Builtin,
    Bytecode,
    Callable,
}

/// One walk's speculative resolution. The owning assoc roots CALLABLE
/// before any callback. This is local to its mutator's Context; it introduces
/// no shared Lisp-state cache or single-mutator assumption. Existing function
/// epochs guard aliases, advice and redefinition after the funcall prologue.
/// Resolution retains the clock's Acquire snapshot from before the binding
/// reads, paired with the existing producers' Release publication of changes.
pub(crate) struct ResolvedAssocPredicate {
    pub(crate) callable: Value,
    epoch: u64,
    target: AssocTarget,
    pub(crate) pure: Option<PureAssocPredicate>,
    native: Option<CheckedNativeCallback>,
}

impl Context {
    /// Resolve a callable once without changing the original designator.
    /// GNU Fassoc's calln reads that designator's cell on every comparison:
    /// unlike sort, assoc must observe a callback redefining its TESTFN.
    /// Autoloads, macros, mutable cons-form lambdas, invalid callables,
    /// compiler overrides and long/cyclic alias chains retain ordinary funcall,
    /// including deferred errors when no cons entry is visited. Resolution
    /// itself never executes Lisp.
    pub(crate) fn resolve_assoc_predicate(
        &self,
        designator: Value,
    ) -> Option<ResolvedAssocPredicate> {
        if self.compiler_function_overrides_active() {
            return None;
        }
        // Retain the Acquire publication clock BEFORE reading function cells:
        // a later redefinition must leave this proof below the newer epoch.
        let epoch = self.obarray.function_epoch();
        let mut function = self.unwrap_symbol(designator);
        // This is a speculative optimization, so declining a long chain is
        // preferable to introducing an indirection error or an infinite loop.
        for _ in 0..32 {
            if let Some(symbol) = function.as_symbol_id() {
                function = self.unwrap_symbol(builtins::symbol_function_cell_in_obarray(
                    self.obarray(),
                    symbol,
                )?);
                continue;
            }
            let (target, pure) = if let Some((_, entry)) = subr_entry_from_value(function) {
                if entry.dispatch_kind != SubrDispatchKind::Builtin {
                    return None;
                }
                let pure = match entry.function {
                    Some(SubrFn::A2(body)) if entry.min_args == 2 && entry.max_args == Some(2) => {
                        PureAssocPredicate::from_body(body)
                    }
                    _ => None,
                };
                (AssocTarget::Builtin, pure)
            } else {
                match function.veclike_type() {
                    Some(VecLikeType::ByteCode) => (AssocTarget::Bytecode, None),
                    Some(VecLikeType::Lambda | VecLikeType::ModuleFunction) => {
                        (AssocTarget::Callable, None)
                    }
                    _ => return None,
                }
            };
            return Some(ResolvedAssocPredicate {
                callable: function,
                epoch,
                target,
                pure,
                native: native_callback_cache_enabled()
                    .then(|| CheckedNativeCallback::resolve(function, 2))
                    .flatten(),
            });
        }
        None
    }

    /// Two-argument funcall protocol around a verified direct comparison.
    /// The epoch and implementation checks happen AFTER quit/depth/backtrace,
    /// GC and debugger entry, exactly where GNU resolves the function cell.
    /// Text properties and symbols-with-pos use the ordinary primitive body.
    #[inline]
    pub(crate) fn apply2_assoc_predicate(
        &mut self,
        designator: Value,
        predicate: &ResolvedAssocPredicate,
        arg0: Value,
        arg1: Value,
    ) -> EvalResult {
        if let Some(proof) = predicate.native {
            return self.apply2_checked_subr(
                designator,
                predicate.callable,
                predicate.epoch,
                proof,
                arg0,
                arg1,
            );
        }
        if predicate.target == AssocTarget::Builtin && predicate.pure.is_none() {
            return self.apply2_resolved_subr(
                designator,
                predicate.callable,
                predicate.epoch,
                arg0,
                arg1,
            );
        }
        self.maybe_quit_before_gc()?;
        self.enter_interpreted_eval_depth()?;
        let bt_count = self.specpdl.len();
        self.push_backtrace_frame(designator, &[arg0, arg1]);
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
                    if ctx.obarray.function_epoch() != predicate.epoch
                        || ctx.compiler_function_overrides_active()
                    {
                        return ctx
                            .funcall_general_untraced(designator, LispArgVec::from_slice(&args));
                    }
                    if predicate.target != AssocTarget::Builtin {
                        #[cfg(feature = "jit")]
                        if predicate.target == AssocTarget::Bytecode {
                            // Fetch only after GC/debugger entry: a dump stub
                            // can be materialized at that safe point. Assoc's
                            // root scope keeps this captured closure alive.
                            let bc_data = predicate.callable.get_bytecode_data().unwrap();
                            return ctx.execute_bytecode_call_2(bc_data, &args, predicate.callable);
                        }
                        return ctx.funcall_general_untraced(
                            predicate.callable,
                            LispArgVec::from_slice(&args),
                        );
                    }
                    // Registration can rewrite an entry in place. Verify the
                    // current object before entering the captured Rust body.
                    let Some((subr_sym, entry)) = subr_call_entry_from_value(predicate.callable)
                    else {
                        return Err(signal(LispCondition::InvalidFunction, vec![designator]));
                    };
                    if entry.dispatch_kind == SubrDispatchKind::Builtin
                        && entry.min_args == 2
                        && entry.max_args == Some(2)
                        && let Some(SubrFn::A2(body)) = entry.function
                        && let Some(pure) = predicate.pure
                        && std::ptr::fn_addr_eq(body, pure.body())
                    {
                        return pure
                            .compare(ctx, arg0, arg1)
                            .map_err(|flow| ctx.validate_throw(flow));
                    }
                    ctx.apply_subr_object_with_entry(subr_sym, predicate.callable, &args, entry)
                }),
            }
        };
        self.depth -= 1;
        self.finish_traced_call(bt_count, result)
    }
}
