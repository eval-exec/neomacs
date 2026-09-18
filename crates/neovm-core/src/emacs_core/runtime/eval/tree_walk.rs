//! GNU `eval_sub` and its cons-form dispatch: the tree-walking interpreter's
//! per-form path, moved out of `eval/mod.rs` unchanged so it can be worked on
//! without the parent module's line ceiling.

use super::*;

impl Context {
    pub(crate) fn eval_lambda_body_value(&mut self, body: Value) -> EvalResult {
        self.maybe_grow_eval_stack(|ctx| {
            let mut cursor = body;
            let mut last = Value::NIL;
            while cursor.is_cons() {
                match ctx.eval_sub(cursor.cons_car()) {
                    Ok(value) => last = value,
                    Err(Flow::ThreadBlocked(blocked)) => {
                        let remaining_forms = if blocked.remaining_forms.is_nil() {
                            cursor.cons_cdr()
                        } else {
                            blocked.remaining_forms
                        };
                        return Err(Flow::thread_blocked(blocked.blocker, remaining_forms));
                    }
                    Err(flow) => return Err(flow),
                }
                cursor = cursor.cons_cdr();
            }
            Ok(last)
        })
    }

    /// Evaluate a runtime Value form, matching GNU Emacs's `eval_sub` in eval.c.
    ///
    /// Dispatch order (matching GNU eval.c:2552-2766):
    /// 1. Symbol → lexenv lookup or symbol-value
    /// 2. Non-cons → self-evaluating (return as-is)
    /// 3. Cons → special form / macro / function call
    pub fn eval_sub(&mut self, form: Value) -> EvalResult {
        // 1. Symbol → variable lookup (GNU eval.c:2554-2562)
        // Also unwrap symbol-with-pos when symbols-with-pos-enabled is true.
        let form_unwrapped = self.unwrap_symbol(form);
        if let Some(sym_id) = form_unwrapped.as_symbol_id() {
            // Route the variable-lookup result through the signal dispatcher so a
            // void-variable enters the debugger (debug-on-error) at signal time,
            // while dynamic bindings are still active — symmetric with the cons
            // path (eval_sub_cons) and GNU's Fsignal. `search_complete` keeps this
            // idempotent, so an already-dispatched signal is not re-dispatched.
            let result = self.eval_symbol_by_id(sym_id);
            return self.dispatch_signal_result_if_needed(result);
        }

        // 2. Non-cons → self-evaluating (GNU eval.c:2564-2565)
        if !form_unwrapped.is_cons() {
            return Ok(form_unwrapped);
        }

        self.enter_interpreted_eval_depth()?;

        let result = self.maybe_grow_eval_stack(|ctx| {
            ctx.maybe_quit_before_gc()?;
            if ctx.gc_safe_point_exact_should_collect() {
                ctx.collect_at_eval_safe_point(form);
            }
            ctx.eval_sub_cons(form)
        });
        self.depth -= 1;
        result
    }

    /// GNU `maybe_gc` inside `eval_sub`, with FORM rooted for the collection
    /// (GNU's conservative scan sees it on the C stack).  Out of line so the
    /// per-form path carries only the predicate.
    #[cold]
    #[inline(never)]
    fn collect_at_eval_safe_point(&mut self, form: Value) {
        let specpdl_root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(form);
        self.gc_collect_from_current_roots();
        self.restore_specpdl_roots(specpdl_root_scope);
    }

    /// GNU's `max_lisp_eval_depth` -- the `DEFVAR_INT` cell (`src/eval.c:4405`)
    /// that `eval_sub` dereferences on every entry (`src/eval.c:2585`).
    ///
    /// `self.max_depth` is this port's cache of that cell, kept fresh on write
    /// by [`Self::sync_cached_runtime_binding_by_id`]. A cache has no swap-in,
    /// though, so it is only GNU's cell while nothing has localised the name;
    /// `lisp/eshell/esh-mode.el` localises it deliberately. When the symbol IS
    /// localized the read names the buffer, exactly as GNU's swapped-in cell
    /// does (ledger 196). The gate is one `Vec` index and one flag byte, on a
    /// path that then dispatches a whole form.
    #[inline]
    fn current_max_lisp_eval_depth(&self) -> Option<usize> {
        if !self.obarray.is_localized(max_lisp_eval_depth_symbol()) {
            return None;
        }
        self.obarray
            .value_in_buffer(self.buffers.current_buffer(), "max-lisp-eval-depth")
            .and_then(|value| value.as_fixnum())
            // GNU raises a limit below 100 before it signals
            // (`src/eval.c:2587-2588`) so a handler has room to run.
            .map(|n| n.max(100) as usize)
    }

    /// GNU `eval_sub` (`src/eval.c:2585`): `lisp_eval_depth++` and one
    /// compare against `max_lisp_eval_depth` on the common path; the
    /// localized cell, the cache refresh and the signal are the cold tail.
    #[inline(always)]
    pub(super) fn enter_interpreted_eval_depth(&mut self) -> Result<(), Flow> {
        self.depth += 1;
        // `max_lisp_eval_depth_localized` stands in for the per-call symbol
        // lookup: it is set the one time the variable is made buffer-local.
        if self.depth <= self.max_depth && !self.obarray.max_lisp_eval_depth_localized {
            return Ok(());
        }
        self.enter_interpreted_eval_depth_slow()
    }

    /// The tail of [`Self::enter_interpreted_eval_depth`]: `depth` is already
    /// counted; decide against the localized or refreshed limit, undoing the
    /// count when the nesting signal fires.
    #[cold]
    #[inline(never)]
    fn enter_interpreted_eval_depth_slow(&mut self) -> Result<(), Flow> {
        if let Some(buffer_limit) = self.current_max_lisp_eval_depth() {
            if self.depth > buffer_limit {
                let overflow_depth = self.depth as i64;
                self.depth -= 1;
                return Err(signal(
                    "excessive-lisp-nesting",
                    vec![Value::fixnum(overflow_depth)],
                ));
            }
            return Ok(());
        }
        // Refresh the cached limit by SYMBOL, not by name: `symbol_value`
        // interns its `&str` on every call, and this runs whenever the depth
        // passes the cached limit -- which is every entry once Lisp raises
        // `max-lisp-eval-depth`, not the rare event the shape suggests.
        if self.depth > self.max_depth
            && let Some(v) = self.obarray.symbol_value_id(max_lisp_eval_depth_symbol())
            && let Some(n) = v.as_fixnum()
        {
            let new_max = n.max(100) as usize;
            if new_max != self.max_depth {
                self.max_depth = new_max;
            }
        }
        if self.depth > self.max_depth {
            let overflow_depth = self.depth as i64;
            self.depth -= 1;
            return Err(signal(
                "excessive-lisp-nesting",
                vec![Value::fixnum(overflow_depth)],
            ));
        }
        Ok(())
    }

    fn eval_sub_cons(&mut self, form: Value) -> EvalResult {
        let original_fun = self.unwrap_symbol(form.cons_car());
        let original_args = form.cons_cdr();

        // GNU eval.c:2583-2585 records an UNEVALLED backtrace frame on
        // every `eval_sub` cons-form evaluation. The frame starts in
        // UNEVALLED shape holding the surface function symbol and the
        // raw argument-form cons list, then transitions to EVALD in
        // place via `set_backtrace_args` once arguments have been
        // evaluated (eval.c:2638, 2660, 3299). Special forms leave
        // the frame UNEVALLED throughout.
        let outer_bt_count = self.specpdl.len();
        let stack_base = self.bc_buf.len();
        self.push_unevalled_backtrace_frame(original_fun, original_args);
        // GNU eval.c:2601-2602, immediately after `record_in_backtrace` and
        // before any dispatch: `if (debug_on_next_call) do_debug_on_call (Qt,
        // count)`.  Taking the arm IS the disarm (see `debug_on_call`), and
        // the same call flags this frame's `debug_on_exit`.
        let dispatch_result = match self.take_debug_on_call_arm(DebugOnCallCode::EvalForm) {
            Some(arm) => self.do_debug_on_call(arm).and_then(|()| {
                self.eval_sub_cons_dispatch(original_fun, original_args, outer_bt_count)
            }),
            None => self.eval_sub_cons_dispatch(original_fun, original_args, outer_bt_count),
        };
        let result = self.dispatch_signal_result_if_needed(dispatch_result);
        self.record_sequence_call_roots(outer_bt_count);
        let result = self.unbind_to_with_result(outer_bt_count, result);
        // The evaluated call parked its function and arguments on the VM
        // operand stack (see `eval_sub_cons_dispatch`); the frame that
        // referenced them is gone, so is their span.
        self.bc_buf.truncate(stack_base);
        result
    }

    /// Evaluate a call's argument forms onto the VM operand stack, FUNC
    /// parked beneath them (both rooted by the stack), and return the first
    /// argument's slot and the count.  An improper tail signals `listp` at
    /// the point GNU's `eval_sub` loop reaches it.
    #[inline]
    fn eval_call_args_onto_stack(
        &mut self,
        func: Value,
        original_args: Value,
    ) -> Result<(usize, usize), Flow> {
        let func_slot = self.bc_buf.len();
        self.bc_buf.push(func);
        let first_arg = func_slot + 1;
        let mut cursor = original_args;
        while cursor.is_cons() {
            let arg_form = cursor.cons_car();
            let arg_val = self.eval_sub(arg_form)?;
            self.bc_buf.push(arg_val);
            cursor = cursor.cons_cdr();
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(cursor));
        }
        Ok((first_arg, self.bc_buf.len() - first_arg))
    }

    fn eval_sub_cons_dispatch(
        &mut self,
        original_fun: Value,
        original_args: Value,
        outer_bt_count: usize,
    ) -> EvalResult {
        // Resolve function (GNU eval.c:2600-2605)
        let sym_id = original_fun.as_symbol_id();

        // Everything this head decides -- whether it is an evaluator-internal
        // literal form, what its function cell holds, and whether that cell is
        // a subr -- depends only on the symbol and the function epoch.  A form
        // is evaluated 32.5 times on average here (measured on magit-status;
        // 61.4 on org-journal-open), so re-deriving it per evaluation is
        // almost entirely repeat work.  One probe answers all three.
        //
        // The cache is bypassed entirely while compiler function overrides are
        // active, exactly as the direct resolution below was.
        let overrides_active = self.compiler_function_overrides_active();
        let head = match sym_id {
            Some(sym_id) if !overrides_active => {
                let epoch = self.obarray.function_epoch();
                Some(match self.form_head_cache.find(sym_id, epoch) {
                    Some(entry) => entry,
                    None => {
                        let func = self.obarray.symbol_function_id(sym_id);
                        let entry = FormHeadCacheEntry {
                            epoch,
                            sym: sym_id,
                            literal_head: sym_id == lambda_symbol()
                                || sym_id == byte_code_literal_symbol()
                                || sym_id == byte_code_symbol(),
                            func,
                            subr: func.and_then(subr_call_entry_from_value),
                        };
                        self.form_head_cache.push(entry);
                        entry
                    }
                })
            }
            _ => None,
        };

        // Keep only evaluator-internal literal forms on the pre-resolution
        // fast path. GNU decides public special-form dispatch from the
        // function cell's UNEVALLED subr, so user-visible special forms
        // should flow through the resolved subr surface below.
        //
        // With overrides active there is no cached head, so the three literal
        // heads are still tested directly.
        if let Some(sym_id) = sym_id
            && head.map_or_else(
                || {
                    sym_id == lambda_symbol()
                        || sym_id == byte_code_literal_symbol()
                        || sym_id == byte_code_symbol()
                },
                |head| head.literal_head,
            )
            && let Some(result) = self.try_special_form_value_id(sym_id, original_args)
        {
            return result;
        }

        // GNU `eval_sub` (`src/eval.c:2600-2680`): the symbol's function cell
        // is read once; a SUBRP that is not UNEVALLED evaluates its arguments
        // into `argvals` and calls by `maxargs`, a COMPILEDP goes to
        // `apply_lambda`.  Neither path re-examines aliases, autoloads,
        // macros, overrides or callability -- the cell already IS a fixed-
        // arity builtin or a byte-code object -- so those probes stay on the
        // full resolution below, which every other cell shape still takes.
        let prefetched_cell = head.and_then(|head| head.func);
        if let Some(sym_id) = sym_id
            && let Some(func) = prefetched_cell
        {
            let subr = head.and_then(|head| head.subr);
            if let Some((target_sym_id, entry)) = subr
                && entry.dispatch_kind == SubrDispatchKind::SpecialForm
                && target_sym_id == sym_id
            {
                // GNU eval.c:2624: `list_length (args_left)` runs for every
                // SUBRP, UNEVALLED ones included, before the dispatch.  The
                // frame stays UNEVALLED (eval.c:2618-2619).
                if list_length(&original_args).is_none() {
                    return Err(self.listp_error(original_args));
                }
                if let Some(result) = self.try_special_form_value_id(sym_id, original_args) {
                    return result;
                }
            }
            if let Some((target_sym_id, entry)) = subr
                && entry.dispatch_kind == SubrDispatchKind::Builtin
                && Self::subr_entry_uses_fixed_value_call(entry)
            {
                let numargs = match list_length(&original_args) {
                    Some(n) => n,
                    None => return Err(self.listp_error(original_args)),
                };
                let min = entry.min_args as usize;
                let max_ok = match entry.max_args {
                    Some(m) => numargs <= m as usize,
                    None => true,
                };
                if numargs < min || !max_ok {
                    return Err(signal(
                        LispCondition::WrongNumberOfArguments,
                        vec![original_fun, Value::fixnum(numargs as i64)],
                    ));
                }
                let (first_arg, nargs) = self.eval_call_args_onto_stack(func, original_args)?;
                self.set_backtrace_args_evalled_bc_span(outer_bt_count, first_arg, nargs);
                return self.maybe_grow_eval_stack(|ctx| {
                    ctx.dispatch_subr_entry_from_bc_stack(entry, first_arg, nargs)
                        .unwrap_or_else(|| {
                            Err(signal(
                                LispCondition::VoidFunction,
                                vec![Value::from_sym_id(target_sym_id)],
                            ))
                        })
                });
            }
            if let Some(bc_data) = func.get_bytecode_data() {
                if list_length(&original_args).is_none() {
                    return Err(self.listp_error(original_args));
                }
                let (first_arg, nargs) = self.eval_call_args_onto_stack(func, original_args)?;
                self.set_backtrace_args_evalled_bc_span(outer_bt_count, first_arg, nargs);
                return self.maybe_grow_eval_stack(|ctx| {
                    ctx.execute_bytecode_call_from_stack(bc_data, first_arg, nargs, func)
                });
            }
        }

        // Resolve function value
        let func = if let Some(sym_id) = sym_id {
            if let Some(override_func) = self
                .compiler_function_overrides_active()
                .then(|| compiler_function_override_in_obarray(&self.obarray, sym_id))
                .flatten()
            {
                override_func
            } else {
                match prefetched_cell.or_else(|| self.obarray.symbol_function_id(sym_id)) {
                    Some(f) => {
                        let mut f = f;
                        // Follow symbol indirection (GNU eval.c:2604)
                        if let Some(alias_id) = f.as_symbol_id()
                            && let Some(resolved) = self.obarray.indirect_function_id(alias_id)
                        {
                            f = resolved;
                        }
                        loop {
                            if !super::super::autoload::is_autoload_value(&f) {
                                break f;
                            }

                            match self.load_named_autoload_call_step(sym_id, f)? {
                                NamedAutoloadCallStep::RetrySymbol { autoload_form } => {
                                    // GNU `eval_sub` jumps back to named
                                    // function resolution after each autoload
                                    // hop.  The returned form is the current
                                    // indirect function cell for that symbol.
                                    f = autoload_form;
                                }
                                NamedAutoloadCallStep::DispatchFunction { function } => {
                                    break function;
                                }
                                NamedAutoloadCallStep::Void => {
                                    return Err(signal(
                                        LispCondition::VoidFunction,
                                        vec![original_fun],
                                    ));
                                }
                            }
                        }
                    }
                    _ => {
                        return Err(signal(
                            LispCondition::VoidFunction,
                            vec![Value::from_sym_id(sym_id)],
                        ));
                    }
                }
            }
        } else {
            // GNU eval_sub runs every non-symbol function position through
            // Ffunction(list1(fun)).  `function` only transforms literal
            // `(lambda ...)` forms; byte-code objects, subrs, and malformed
            // values are quoted through to the normal callable validation
            // below.
            if original_fun.is_cons() && cons_head_symbol_id(&original_fun) == Some(lambda_symbol())
            {
                self.instantiate_callable_cons_form(original_fun)?
            } else {
                original_fun
            }
        };

        if let Some(surface_sym_id) = sym_id
            && let Some(target_sym_id) = func.as_subr_id()
            && self.subr_is_special_form_id(target_sym_id)
        {
            // GNU eval.c:2624 runs `list_length (args_left)` for *every*
            // SUBRP `fun` — including UNEVALLED special forms — BEFORE
            // dispatching to the special-form C function. `list_length`
            // ends in `CHECK_LIST_END`, so an improper top-level argument
            // list (e.g. `(progn a . b)`, `(if t a . b)`, `(when t . b)`)
            // signals `(wrong-type-argument listp BAD-CDR)` up front,
            // *before* any body form is evaluated. Neo otherwise validated
            // lazily and evaluated the first element first (wrong error /
            // no error). Match GNU: validate the arg-list structure here.
            if list_length(&original_args).is_none() {
                return Err(self.listp_error(original_args));
            }
            // The outer eval_sub_cons UNEVALLED frame (pushed by the
            // wrapper) already records the surface function and raw
            // argument forms. Special forms leave the frame UNEVALLED
            // throughout (no `set_backtrace_args_evalled` call),
            // matching GNU eval.c:2618-2619.
            let result = if surface_sym_id == target_sym_id {
                self.try_special_form_value_id(surface_sym_id, original_args)
            } else {
                self.try_aliased_special_form_value_id(surface_sym_id, target_sym_id, original_args)
            };
            if let Some(result) = result {
                return result;
            }
        }

        // Check for macro (GNU eval.c:2730-2755)
        if func.is_macro() {
            // GNU expands a macro via `apply1 (Fcdr (fun), original_args)`
            // (eval.c:2766), and `apply1` -> `Fapply` -> `list_length`
            // (eval.c:3065/fns.c:115) validates the argument-list structure
            // up front. An improper macro-call tail (e.g. `(when t . b)`)
            // therefore signals `(wrong-type-argument listp BAD-CDR)` rather
            // than silently dropping the bad cdr. `value_list_to_values`
            // walks lazily and would otherwise swallow the improper tail.
            if list_length(&original_args).is_none() {
                return Err(self.listp_error(original_args));
            }
            let arg_values = value_list_to_values(&original_args);
            let bt_count = self.specpdl.len();
            self.push_backtrace_frame(original_fun, &arg_values);
            let expanded =
                self.with_macro_expansion_scope(|eval| eval.apply_lambda(func, arg_values));
            let expanded = self.unbind_to_with_result(bt_count, expanded);
            let expanded = expanded?;
            let expanded_root_count = self.specpdl.len();
            self.push_specpdl_root(expanded);
            let result = self.eval_sub(expanded);
            return self.unbind_to_with_result(expanded_root_count, result);
        }
        if cons_head_symbol_id(&func) == Some(macro_symbol()) {
            // Cons-cell macro: (macro . fn) — GNU eval.c:2730
            // Same up-front `apply1`/`list_length` validation as the
            // `func.is_macro()` branch above (GNU eval.c:2766).
            if list_length(&original_args).is_none() {
                return Err(self.listp_error(original_args));
            }
            let macro_fn = func.cons_cdr();
            let arg_values = value_list_to_values(&original_args);
            let bt_count = self.specpdl.len();
            self.push_backtrace_frame(original_fun, &arg_values);
            let expanded = self.with_macro_expansion_scope(|eval| eval.apply(macro_fn, arg_values));
            let expanded = self.unbind_to_with_result(bt_count, expanded);
            let expanded = expanded?;
            let expanded_root_count = self.specpdl.len();
            self.push_specpdl_root(expanded);
            let result = self.eval_sub(expanded);
            return self.unbind_to_with_result(expanded_root_count, result);
        }

        // GNU eval.c:2606-2614: for SUBRP `fun`, check arity
        // against the raw `original_args` count BEFORE any arg
        // evaluation, and on mismatch signal
        // `(wrong-number-of-arguments original_fun numargs)` where
        // `original_fun` is the XCAR of the form (the surface
        // symbol, not the resolved subr value). This is how GNU
        // gets `(wrong-number-of-arguments car 0)` for a direct
        // `(car)` call -- the arity check runs inline in eval_sub
        // and never reaches `funcall_subr` which would have emitted
        // `#<subr car>` via `XSETSUBR`.
        //
        // For non-subrs (closures, bytecode, lambdas, cons forms)
        // the dispatch falls through to the normal apply path,
        // which signals with `fun` itself -- also matching GNU
        // funcall_lambda and funcall_subr.
        // GNU keeps the resolved XSUBR in `fun` across argument
        // evaluation and calls it directly. Preserve the SubrEntry we
        // resolved for the direct eval_sub arity check instead of
        // looking it up again after evaluating args.
        let direct_subr_entry = if let Some((sym_id, entry)) = subr_entry_from_value(func) {
            if entry.dispatch_kind != SubrDispatchKind::SpecialForm {
                let numargs = match list_length(&original_args) {
                    Some(n) => n,
                    None => return Err(self.listp_error(original_args)),
                };
                let min = entry.min_args as usize;
                let max_ok = match entry.max_args {
                    Some(m) => numargs <= m as usize,
                    None => true, // &rest / MANY
                };
                if numargs < min || !max_ok {
                    return Err(signal(
                        LispCondition::WrongNumberOfArguments,
                        vec![original_fun, Value::fixnum(numargs as i64)],
                    ));
                }
                Some((sym_id, entry))
            } else {
                None
            }
        } else {
            None
        };

        // GNU eval.c:2716-2726: when `fun` is not a subr, closure,
        // bytecode, or cons-shaped lambda/autoload/macro, signal
        // `(invalid-function original_fun)` with the SURFACE
        // symbol. Verified against emacs 31.0.50:
        //   (fset 'vm-fsetint 1)
        //   (condition-case e (vm-fsetint) (error e))
        //     → (invalid-function vm-fsetint)
        //
        // The check runs inline in eval_sub so the dispatcher
        // `funcall_general` never sees the invalid value and
        // never emits the resolved fncell contents as signal data.
        if !self.function_value_is_callable(&func) {
            if func.is_nil() {
                return Err(signal(LispCondition::VoidFunction, vec![original_fun]));
            }
            return Err(signal(LispCondition::InvalidFunction, vec![original_fun]));
        }

        // Regular function call: evaluate args, promote the outer
        // UNEVALLED frame to EVALD in place, then dispatch directly.
        // Matches GNU `eval_sub` non-UNEVALLED SUBRP path
        // (eval.c:2631-2640) and CLOSUREP → apply_lambda
        // (eval.c:2715, 3292-3300) which both mutate the outer
        // record_in_backtrace entry via `set_backtrace_args`.
        //
        // `func` and each evaluated arg are rooted on the specpdl via
        // `push_specpdl_root`. GNU relies on conservative stack
        // scanning of `SAFE_ALLOCA_LISP (vals, numargs)` plus the
        // `fun` C local; neomacs uses exact GC, so a local
        // `Vec<Value>` and the Rust-local `func` Value are invisible
        // to the tracer.
        //
        // `func` is rooted BEFORE the arg loop so it survives GC
        // triggered by any arg evaluator, and stays rooted through
        // `funcall_general_untraced` below -- it only gets popped by
        // the outer `eval_sub_cons` `unbind_to(outer_bt_count)`. This
        // is specifically needed when `original_fun` is a cons
        // (lambda-literal head): the resolved Lambda Value lives only
        // on the Rust stack, and the outer UNEVALLED frame records
        // `original_fun`, not `func`.
        //
        // Per-arg roots are popped once `set_backtrace_args_evalled`
        // transfers ownership to the outer frame's args slot.
        // GNU uses SAFE_ALLOCA_LISP for evaluated arguments here. Keep the
        // common arities inline instead of allocating a heap Vec per call.
        // GNU validates the argument-list structure UP FRONT, before
        // evaluating any argument: the subr path runs a single
        // `list_length (args_left)` (eval.c:2624) and `apply_lambda` runs
        // `list_length (args)` (eval.c:3302). Both end in `CHECK_LIST_END`,
        // so an improper arg list (e.g. `((lambda (a &rest b) b) x . y)`)
        // signals `(wrong-type-argument listp BAD-CDR)` *before* `x` is ever
        // evaluated. Neo previously evaluated args lazily and only checked
        // the tail afterwards, leaking a void-variable error for `x` first.
        // Subrs already walked the spine once for the arity check above
        // (`direct_subr_entry` is only Some when that walk returned a
        // length), so re-walking here would make the spine cost 3x per
        // interpreted subr call where GNU pays 1x + the eval walk. Only
        // the closure/bytecode/lambda paths still need the up-front walk.
        if direct_subr_entry.is_none() && list_length(&original_args).is_none() {
            return Err(self.listp_error(original_args));
        }
        // GNU eval.c:2640-2680 evaluates the arguments into a C array
        // (`argvals`, or `vals` from SAFE_ALLOCA) and records that array as
        // the frame's EVALD args before the call.  The port's array is the VM
        // operand stack: the precise root walk traces it in full, so a push
        // roots an argument -- no specpdl entry per argument -- and
        // `eval_sub_cons` truncates the stack back once the frame is gone.
        // The function value sits below the arguments for the same reason
        // (GNU's `fun` is a C local).
        let (first_arg, nargs) = self.eval_call_args_onto_stack(func, original_args)?;
        self.set_backtrace_args_evalled_bc_span(outer_bt_count, first_arg, nargs);

        if let Some((sym_id, entry)) = direct_subr_entry
            && Self::subr_entry_uses_fixed_value_call(entry)
        {
            return self.maybe_grow_eval_stack(|ctx| {
                ctx.dispatch_subr_entry_from_bc_stack(entry, first_arg, nargs)
                    .unwrap_or_else(|| {
                        Err(signal(
                            LispCondition::VoidFunction,
                            vec![Value::from_sym_id(sym_id)],
                        ))
                    })
            });
        }

        if let Some((sym_id, entry)) = direct_subr_entry {
            return self.maybe_grow_eval_stack(|ctx| {
                let args = LispArgVec::from_slice(&ctx.bc_buf[first_arg..first_arg + nargs]);
                if entry.dispatch_kind == SubrDispatchKind::ContextCallable {
                    return ctx.apply_evaluator_callable_by_id(sym_id, args);
                }
                ctx.dispatch_subr_entry_unchecked(entry, args)
                    .unwrap_or_else(|| {
                        Err(signal(
                            LispCondition::VoidFunction,
                            vec![Value::from_sym_id(sym_id)],
                        ))
                    })
            });
        }

        // A byte-code callee takes its arguments where they lie, through the
        // same stack path as a `Bcall` (leaf slot and tiered plan included);
        // the frame recorded above is its backtrace frame, as GNU's
        // `apply_lambda` adds none of its own.
        if let Some(bc_data) = func.get_bytecode_data() {
            return self.maybe_grow_eval_stack(|ctx| {
                ctx.execute_bytecode_call_from_stack(bc_data, first_arg, nargs, func)
            });
        }

        self.maybe_grow_eval_stack(|ctx| {
            let args = LispArgVec::from_slice(&ctx.bc_buf[first_arg..first_arg + nargs]);
            ctx.funcall_general_untraced(func, args)
        })
    }
}
