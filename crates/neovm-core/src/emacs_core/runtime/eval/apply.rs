//! Function application: closures, arity checks, argument binding, and the funcall/apply paths (GNU eval.c funcall_lambda / apply_lambda).
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;

impl Context {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn make_interpreted_closure_with_expr_runtime_hook(
        &mut self,
        params_value: Value,
        body_value: Value,
        env_value: Value,
        docstring_value: Value,
        iform_value: Value,
    ) -> EvalResult {
        let root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(params_value);
        self.push_specpdl_root(body_value);
        self.push_specpdl_root(env_value);
        self.push_specpdl_root(docstring_value);
        self.push_specpdl_root(iform_value);

        if !env_value.is_nil() {
            let closure_hook = self.visible_variable_value_or_nil_by_id(
                internal_make_interpreted_closure_function_symbol(),
            );
            if !closure_hook.is_nil() {
                self.push_specpdl_root(closure_hook);
                let result = self.apply(
                    closure_hook,
                    vec![
                        params_value,
                        body_value,
                        env_value,
                        docstring_value,
                        iform_value,
                    ],
                );
                self.restore_specpdl_roots(root_scope);
                return result;
            }
        }

        let result = builtins::symbols::make_interpreted_closure_from_parts(
            &params_value,
            &body_value,
            &env_value,
            Some(&docstring_value),
            Some(&iform_value),
        );
        self.restore_specpdl_roots(root_scope);
        result
    }

    pub(super) fn make_interpreted_closure_with_value_runtime_hook(
        &mut self,
        source_function: Value,
        params_value: Value,
        body_value: Value,
        env_value: Value,
        docstring_value: Value,
        iform_value: Value,
    ) -> EvalResult {
        let root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(source_function);
        self.push_specpdl_root(params_value);
        self.push_specpdl_root(body_value);
        self.push_specpdl_root(env_value);
        self.push_specpdl_root(docstring_value);
        self.push_specpdl_root(iform_value);

        if !env_value.is_nil() {
            let closure_hook = self.visible_variable_value_or_nil_by_id(
                internal_make_interpreted_closure_function_symbol(),
            );
            if !closure_hook.is_nil() {
                self.push_specpdl_root(closure_hook);
                let result = self.apply(
                    closure_hook,
                    vec![
                        params_value,
                        body_value,
                        env_value,
                        docstring_value,
                        iform_value,
                    ],
                );
                self.restore_specpdl_roots(root_scope);
                return result;
            }
        }

        let result = builtins::symbols::make_interpreted_closure_from_parts(
            &params_value,
            &body_value,
            &env_value,
            Some(&docstring_value),
            Some(&iform_value),
        );
        self.restore_specpdl_roots(root_scope);
        result
    }

    pub(super) fn eval_dynamic_documentation_value(
        &mut self,
        value: Value,
    ) -> Result<Option<Value>, Flow> {
        if !value.is_cons() || value.cons_car().as_symbol_name() != Some(":documentation") {
            return Ok(None);
        }

        let tail = value.cons_cdr();
        if tail.is_nil() {
            return Ok(Some(Value::NIL));
        }
        if !tail.is_cons() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), value],
            ));
        }

        self.eval_value(&tail.cons_car()).map(Some)
    }

    #[inline(always)]
    pub(crate) fn push_backtrace_frame(&mut self, function: Value, args: &[Value]) {
        match args {
            [arg] => {
                self.specpdl.push(SpecBinding::Backtrace1 {
                    function,
                    arg: *arg,
                    debug_on_exit: false,
                });
                return;
            }
            [arg0, arg1] => {
                self.specpdl.push(SpecBinding::Backtrace2 {
                    function,
                    arg0: *arg0,
                    arg1: *arg1,
                });
                return;
            }
            _ => {}
        }
        let args = self.backtrace_args_from_slice(args);
        self.specpdl.push(SpecBinding::Backtrace {
            function,
            args,
            debug_on_exit: false,
        });
    }

    /// Backtrace push for a native (JIT) caller: args live in the generated
    /// code's call-args slot. Reads them in place — the common 1-2 arity
    /// cases go straight into the compact specpdl forms with no intermediate
    /// collection (the SmallVec built merely to pass `&[Value]` was a
    /// measured ~30 Ir/call tax on native-to-native recursion).
    ///
    /// # Safety
    /// `args_ptr` must address `nargs` valid tagged words, alive for the
    /// duration of this call (the caller's call-args slot).
    pub(crate) unsafe fn push_backtrace_frame_from_native_args(
        &mut self,
        function: Value,
        args_ptr: *const i64,
        nargs: usize,
    ) {
        // SAFETY: caller contract.
        let read = |i: usize| Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
        match nargs {
            1 => {
                self.specpdl.push(SpecBinding::Backtrace1 {
                    function,
                    arg: read(0),
                    debug_on_exit: false,
                });
            }
            2 => {
                self.specpdl.push(SpecBinding::Backtrace2 {
                    function,
                    arg0: read(0),
                    arg1: read(1),
                });
            }
            _ => {
                // GNU stores exactly this: a pointer into the caller's
                // frame plus the count. The 3+-arity path previously
                // copied the args twice and parked them on the owned
                // side-stack — ~100 Ir per call on 3-arg native
                // recursion (tak).
                self.specpdl.push(SpecBinding::BacktraceNative {
                    function,
                    args_ptr,
                    nargs: nargs as u32,
                });
            }
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn push_backtrace_frame_owned(&mut self, function: Value, args: LispArgVec) {
        match args.as_slice() {
            [arg] => {
                self.specpdl.push(SpecBinding::Backtrace1 {
                    function,
                    arg: *arg,
                    debug_on_exit: false,
                });
                return;
            }
            [arg0, arg1] => {
                self.specpdl.push(SpecBinding::Backtrace2 {
                    function,
                    arg0: *arg0,
                    arg1: *arg1,
                });
                return;
            }
            _ => {}
        }
        let args = self.backtrace_args_from_owned(args);
        self.specpdl.push(SpecBinding::Backtrace {
            function,
            args,
            debug_on_exit: false,
        });
    }

    #[inline(always)]
    pub(crate) fn push_backtrace_frame_from_bc_stack(
        &mut self,
        function: Value,
        args_start: usize,
        nargs: usize,
    ) -> BytecodeBacktraceFrame {
        let base = self.specpdl.len();
        let range = BytecodeBacktraceRange::new(args_start, nargs);
        let (args, owns_args) = match BytecodeBacktraceSpan::try_new(args_start, nargs) {
            Some(span) => (BacktraceArgs::evaluated_bc_stack(span), false),
            None => (self.backtrace_args_from_oversized_bc_stack(range), true),
        };
        self.specpdl.push(SpecBinding::Backtrace {
            function,
            args,
            debug_on_exit: false,
        });
        BytecodeBacktraceFrame::new(base, owns_args)
    }

    /// Semantic fallback for a bytecode stack span too large for the compact
    /// descriptor. Keep it out of the ordinary Bcall instruction stream: a
    /// packed span covers every normally allocatable frame, while this path
    /// must retain behavior rather than impose a representation limit.
    #[cold]
    #[inline(never)]
    pub(super) fn backtrace_args_from_oversized_bc_stack(
        &mut self,
        range: BytecodeBacktraceRange,
    ) -> BacktraceArgs {
        let index = self.backtrace_args_stack.len();
        self.backtrace_args_stack
            .push(BacktraceArgStorage::BytecodeStack(range));
        BacktraceArgs::evaluated(index)
    }

    /// Push a backtrace frame for a special-form call (`nargs == UNEVALLED`
    /// in GNU eval.c:2585). `original_args` is the cons list of un-evaluated
    /// argument forms — XCDR of the original form. The walker emits
    /// `(nil FUNC FORMS FLAGS)` for these frames.
    pub(crate) fn push_unevalled_backtrace_frame(&mut self, function: Value, original_args: Value) {
        self.specpdl.push(SpecBinding::Backtrace {
            function,
            args: BacktraceArgs::unevalled(original_args),
            debug_on_exit: false,
        });
    }

    #[inline]
    pub(super) fn store_backtrace_args(&mut self, args: LispArgVec) -> usize {
        let index = self.backtrace_args_stack.len();
        self.backtrace_args_stack
            .push(BacktraceArgStorage::Values(args));
        index
    }

    #[inline]
    pub(super) fn backtrace_args_from_slice(&mut self, args: &[Value]) -> BacktraceArgs {
        match args {
            [] => BacktraceArgs::evaluated0(),
            _ => BacktraceArgs::evaluated(self.store_backtrace_args(LispArgVec::from_slice(args))),
        }
    }

    #[inline]
    pub(super) fn backtrace_args_from_owned(&mut self, args: LispArgVec) -> BacktraceArgs {
        if args.is_empty() {
            BacktraceArgs::evaluated0()
        } else {
            BacktraceArgs::evaluated(self.store_backtrace_args(args))
        }
    }

    pub(super) fn evaluated_backtrace_from_slice(
        &mut self,
        function: Value,
        debug_on_exit: bool,
        args: &[Value],
    ) -> SpecBinding {
        match args {
            [arg] => SpecBinding::Backtrace1 {
                function,
                arg: *arg,
                debug_on_exit,
            },
            [arg0, arg1] if !debug_on_exit => SpecBinding::Backtrace2 {
                function,
                arg0: *arg0,
                arg1: *arg1,
            },
            _ => SpecBinding::Backtrace {
                function,
                args: self.backtrace_args_from_slice(args),
                debug_on_exit,
            },
        }
    }

    pub(super) fn evaluated_backtrace_from_owned(
        &mut self,
        function: Value,
        debug_on_exit: bool,
        args: LispArgVec,
    ) -> SpecBinding {
        match args.as_slice() {
            [arg] => SpecBinding::Backtrace1 {
                function,
                arg: *arg,
                debug_on_exit,
            },
            [arg0, arg1] if !debug_on_exit => SpecBinding::Backtrace2 {
                function,
                arg0: *arg0,
                arg1: *arg1,
            },
            _ => SpecBinding::Backtrace {
                function,
                args: self.backtrace_args_from_owned(args),
                debug_on_exit,
            },
        }
    }

    #[inline]
    pub(super) fn release_backtrace_args(&mut self, args: &BacktraceArgs) {
        let Some(index) = args.owned_index() else {
            return;
        };
        self.release_owned_backtrace_args(index);
    }

    #[inline(never)]
    pub(super) fn release_owned_backtrace_args(&mut self, index: usize) {
        if index >= self.backtrace_args_stack.len() {
            // Healed residue: a panic contained at a JIT-shim/module boundary
            // truncated `backtrace_args_stack` while the panicked extent's
            // Backtrace specpdl entries survive for the deferred depth-based
            // unwind (`restore_jit_shim_boundary` doc). Their slots are
            // already gone; releasing degrades to a no-op by design.
            return;
        }
        debug_assert_eq!(
            index + 1,
            self.backtrace_args_stack.len(),
            "backtrace args stack should unwind in LIFO order"
        );
        if index + 1 == self.backtrace_args_stack.len() {
            self.backtrace_args_stack.pop();
        } else {
            self.backtrace_args_stack[index].clear();
        }
    }

    /// Test-only: observed depth of the backtrace args stack (containment
    /// regression tests assert healed residue leaves it at base).
    #[cfg(test)]
    pub(crate) fn backtrace_args_stack_len_for_test(&self) -> usize {
        self.backtrace_args_stack.len()
    }

    pub(super) fn release_backtrace_args_in_specpdl_suffix(&mut self, count: usize) {
        let mut truncate_to = self.backtrace_args_stack.len();
        for binding in self.specpdl[count..].iter().rev() {
            if let SpecBinding::Backtrace { args, .. } = binding
                && let BacktraceArgsView::Evaluated(index) = args.view()
            {
                if index >= truncate_to {
                    // Healed residue (see `release_backtrace_args`): the slot
                    // was already truncated by a containment boundary restore.
                    continue;
                }
                debug_assert_eq!(
                    index + 1,
                    truncate_to,
                    "backtrace args stack should match the specpdl unwind suffix"
                );
                truncate_to = index;
            }
        }
        self.backtrace_args_stack.truncate(truncate_to);
    }

    pub(crate) fn backtrace_args_values(&self, args: &BacktraceArgs) -> LispArgVec {
        match args.view() {
            BacktraceArgsView::Unevalled(value) => smallvec::smallvec![value],
            BacktraceArgsView::Evaluated0 => LispArgVec::new(),
            BacktraceArgsView::Evaluated(index) => match self.backtrace_args_stack.get(index) {
                Some(BacktraceArgStorage::Values(values)) => values.clone(),
                Some(BacktraceArgStorage::BytecodeStack(range)) => {
                    let end = range.start.saturating_add(range.len);
                    if end <= self.bc_buf.len() {
                        LispArgVec::from_slice(&self.bc_buf[range.start..end])
                    } else {
                        LispArgVec::new()
                    }
                }
                None => LispArgVec::new(),
            },
            BacktraceArgsView::EvaluatedBcStack(span) => {
                let start = span.start();
                let len = span.len();
                let end = start.saturating_add(len);
                if end <= self.bc_buf.len() {
                    LispArgVec::from_slice(&self.bc_buf[start..end])
                } else {
                    LispArgVec::new()
                }
            }
        }
    }

    /// Copy the logical GNU backtrace fields from any compact physical frame.
    /// Backtrace inspection is cold; centralizing the representation split
    /// keeps callers exhaustive without putting a larger enum in the hot
    /// specpdl entry itself.
    pub(crate) fn backtrace_entry_values(
        &self,
        entry: &SpecBinding,
    ) -> Option<(Value, LispArgVec, bool, bool)> {
        match entry {
            SpecBinding::Backtrace {
                function,
                args,
                debug_on_exit,
            } => Some((
                *function,
                self.backtrace_args_values(args),
                *debug_on_exit,
                args.is_unevalled(),
            )),
            SpecBinding::Backtrace1 {
                function,
                arg,
                debug_on_exit,
            } => Some((*function, smallvec::smallvec![*arg], *debug_on_exit, false)),
            SpecBinding::Backtrace2 {
                function,
                arg0,
                arg1,
            } => Some((*function, smallvec::smallvec![*arg0, *arg1], false, false)),
            SpecBinding::BacktraceNative {
                function,
                args_ptr,
                nargs,
            } => {
                // SAFETY: variant contract — the caller's call-args slot
                // outlives this entry.
                let args = (0..*nargs as usize)
                    .map(|i| Value::from_bits(unsafe { *args_ptr.add(i) } as usize))
                    .collect();
                Some((*function, args, false, false))
            }
            _ => None,
        }
    }

    /// True when the specpdl entry at `index` is a backtrace frame at all --
    /// GNU's `eassert (pdl->kind == SPECPDL_BACKTRACE)`
    /// (`src/eval.c:146, 154`, `src/lisp.h:3736`).
    pub(crate) fn specpdl_entry_is_backtrace(&self, index: usize) -> bool {
        matches!(
            self.specpdl.get(index),
            Some(
                SpecBinding::Backtrace { .. }
                    | SpecBinding::Backtrace1 { .. }
                    | SpecBinding::Backtrace2 { .. }
                    | SpecBinding::BacktraceNative { .. }
            )
        )
    }

    /// GNU `Fdefvaralias`'s specpdl scan (`src/eval.c:702-711`): is SYMBOL
    /// dynamically rebound anywhere on the current binding stack?
    ///
    /// GNU walks from `specpdl_ptr` down to `specpdl` -- the *whole* stack, not
    /// the current frame -- and compares with `EQ`, so no alias resolution
    /// happens: the question is about this exact symbol.  A binding that has
    /// already been unwound is gone from the stack and therefore not found,
    /// which is the difference between rows 1 and 2 of `tmp/l183-p6.el`.
    pub(crate) fn symbol_is_let_bound(&self, symbol: SymId) -> bool {
        self.specpdl
            .iter()
            .rev()
            .any(|entry| entry.let_bound_symbol() == Some(symbol))
    }

    /// GNU `backtrace_debug_on_exit` (`src/lisp.h:3733-3738`) for the frame at
    /// `index`, answering `false` for anything that is not a backtrace frame so
    /// that a caller unbinding a plain `let` region asks the question safely.
    pub(crate) fn backtrace_frame_wants_debug_on_exit(&self, index: usize) -> bool {
        match self.specpdl.get(index) {
            Some(SpecBinding::Backtrace { debug_on_exit, .. })
            | Some(SpecBinding::Backtrace1 { debug_on_exit, .. }) => *debug_on_exit,
            // Structurally false, by the variants' own contract.
            Some(SpecBinding::Backtrace2 { .. } | SpecBinding::BacktraceNative { .. }) => false,
            _ => false,
        }
    }

    /// GNU `set_backtrace_debug_on_exit` (`src/eval.c:151-156`).
    ///
    /// GNU's `bt` struct always has the bit (`src/lisp.h:3628`); this port's
    /// specpdl entry does not, because [`SpecBinding::Backtrace2`] and
    /// [`SpecBinding::BacktraceNative`] drop it to stay inside the hot entry's
    /// size budget and are documented as "structurally false".  Setting the
    /// flag on one of those therefore *promotes* the frame to the owned
    /// [`SpecBinding::Backtrace`] shape rather than silently losing the
    /// debugger entry -- the promotion is the payment for the size win, and it
    /// is on a cold path by construction (nothing but a debugger sets this).
    ///
    /// Returns whether a backtrace frame was found, mirroring GNU's
    /// `if (backtrace_p (pdl))` guard in `Fbacktrace_debug` (`src/eval.c:4025`).
    pub(crate) fn set_backtrace_debug_on_exit(&mut self, index: usize, flag: bool) -> bool {
        match self.specpdl.get_mut(index) {
            Some(SpecBinding::Backtrace { debug_on_exit, .. })
            | Some(SpecBinding::Backtrace1 { debug_on_exit, .. }) => {
                *debug_on_exit = flag;
                true
            }
            Some(SpecBinding::Backtrace2 { .. } | SpecBinding::BacktraceNative { .. }) => {
                if !flag {
                    // Already false by the variant's contract; nothing to do,
                    // and in particular nothing to promote.
                    return true;
                }
                self.promote_backtrace_frame_for_debug_on_exit(index);
                true
            }
            _ => false,
        }
    }

    /// Rewrite the compact frame at `index` into the owned
    /// [`SpecBinding::Backtrace`] shape with `debug_on_exit` set.
    ///
    /// The one subtlety is `backtrace_args_stack`: its slots are pushed in
    /// specpdl order and released LIFO (`release_backtrace_args_in_specpdl_suffix`
    /// asserts exactly that), so a promotion in the middle of the stack has to
    /// *insert* at the position this frame's slot would have occupied and shift
    /// the frames above it, not push on top.  For the entry-debugger path the
    /// frame is always the specpdl top and the insert degenerates to a push.
    #[cold]
    #[inline(never)]
    pub(super) fn promote_backtrace_frame_for_debug_on_exit(&mut self, index: usize) {
        let (function, values) = match &self.specpdl[index] {
            SpecBinding::Backtrace2 {
                function,
                arg0,
                arg1,
            } => (*function, smallvec::smallvec![*arg0, *arg1]),
            SpecBinding::BacktraceNative {
                function,
                args_ptr,
                nargs,
            } => {
                let (function, args_ptr, nargs) = (*function, *args_ptr, *nargs as usize);
                // SAFETY: variant contract -- the native caller's call-args
                // slot outlives this entry, and the entry is live here.
                let values = (0..nargs)
                    .map(|i| Value::from_bits(unsafe { *args_ptr.add(i) } as usize))
                    .collect::<LispArgVec>();
                (function, values)
            }
            _ => return,
        };

        // The slot's position: the first owned slot belonging to a frame ABOVE
        // this one, or the top of the stack when there is none.
        let mut insert_at = self.backtrace_args_stack.len();
        for binding in &self.specpdl[index + 1..] {
            if let SpecBinding::Backtrace { args, .. } = binding
                && let Some(owned) = args.owned_index()
            {
                insert_at = insert_at.min(owned);
            }
        }
        self.backtrace_args_stack
            .insert(insert_at, BacktraceArgStorage::Values(values));
        for binding in self.specpdl[index + 1..].iter_mut() {
            if let SpecBinding::Backtrace { args, .. } = binding
                && let Some(owned) = args.owned_index()
                && owned >= insert_at
            {
                *args = BacktraceArgs::evaluated(owned + 1);
            }
        }
        self.specpdl[index] = SpecBinding::Backtrace {
            function,
            args: BacktraceArgs::evaluated(insert_at),
            debug_on_exit: true,
        };
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn backtrace_args_len(&self, args: &BacktraceArgs) -> usize {
        match args.view() {
            BacktraceArgsView::Unevalled(_) => 1,
            BacktraceArgsView::Evaluated0 => 0,
            BacktraceArgsView::Evaluated(index) => {
                self.backtrace_args_stack
                    .get(index)
                    .map_or(0, |storage| match storage {
                        BacktraceArgStorage::Values(values) => values.len(),
                        BacktraceArgStorage::BytecodeStack(range) => range.len,
                    })
            }
            BacktraceArgsView::EvaluatedBcStack(span) => span.len(),
        }
    }

    pub(super) fn trace_backtrace_args(&self, args: &BacktraceArgs, visit: &mut dyn FnMut(Value)) {
        match args.view() {
            BacktraceArgsView::Unevalled(value) => visit(value),
            BacktraceArgsView::Evaluated0 => {}
            BacktraceArgsView::Evaluated(index) => {
                if let Some(storage) = self.backtrace_args_stack.get(index) {
                    match storage {
                        BacktraceArgStorage::Values(values) => {
                            for arg in values.iter().copied() {
                                visit(arg);
                            }
                        }
                        BacktraceArgStorage::BytecodeStack(range) => {
                            let end = range.start.saturating_add(range.len);
                            if end <= self.bc_buf.len() {
                                for arg in self.bc_buf[range.start..end].iter().copied() {
                                    visit(arg);
                                }
                            }
                        }
                    }
                }
            }
            BacktraceArgsView::EvaluatedBcStack(span) => {
                let start = span.start();
                let end = start.saturating_add(span.len());
                if end <= self.bc_buf.len() {
                    for arg in self.bc_buf[start..end].iter().copied() {
                        visit(arg);
                    }
                }
            }
        }
    }

    /// Promote the UNEVALLED backtrace frame at `specpdl[count]` to the
    /// EVALD shape in place. Mirrors GNU `set_backtrace_args`
    /// (eval.c:144-156) called at eval.c:2638, 2660, 3299 after
    /// argument evaluation completes.
    ///
    /// `count` is the `specpdl.len()` observed *before* the outer
    /// `push_unevalled_backtrace_frame` — the same value a caller
    /// would pass to `unbind_to`.
    ///
    /// Panics if the slot is not an UNEVALLED backtrace frame. Callers
    /// must keep the invariant that every `set_backtrace_args_evalled`
    /// matches exactly one prior `push_unevalled_backtrace_frame`.
    pub(crate) fn set_backtrace_args_evalled(&mut self, count: usize, evaluated: &[Value]) {
        let (function, debug_on_exit) = match self.specpdl.get(count) {
            Some(SpecBinding::Backtrace {
                function,
                args,
                debug_on_exit,
            }) if args.is_unevalled() => (*function, *debug_on_exit),
            other => panic!(
                "set_backtrace_args_evalled: expected UNEVALLED Backtrace at specpdl[{count}], got {other:?}"
            ),
        };
        let replacement = self.evaluated_backtrace_from_slice(function, debug_on_exit, evaluated);
        self.specpdl[count] = replacement;
    }

    pub(crate) fn set_backtrace_args_evalled_owned(&mut self, count: usize, evaluated: LispArgVec) {
        let (function, debug_on_exit) = match self.specpdl.get(count) {
            Some(SpecBinding::Backtrace {
                function,
                args,
                debug_on_exit,
            }) if args.is_unevalled() => (*function, *debug_on_exit),
            other => panic!(
                "set_backtrace_args_evalled_owned: expected UNEVALLED Backtrace at specpdl[{count}], got {other:?}"
            ),
        };
        let replacement = self.evaluated_backtrace_from_owned(function, debug_on_exit, evaluated);
        self.specpdl[count] = replacement;
    }

    pub(crate) fn save_specpdl_roots(&self) -> SpecpdlRootScopeState {
        SpecpdlRootScopeState {
            saved_len: self.specpdl.len(),
        }
    }

    pub(crate) fn record_native_unwind(&mut self, action: NativeUnwindAction) -> NativeUnwindToken {
        let index = self.specpdl.len();
        self.specpdl.push(SpecBinding::NativeUnwind { action });
        NativeUnwindToken { index }
    }

    pub(crate) fn native_unwind_action_mut(
        &mut self,
        token: NativeUnwindToken,
    ) -> Option<&mut NativeUnwindAction> {
        match self.specpdl.get_mut(token.index) {
            Some(SpecBinding::NativeUnwind { action }) => Some(action),
            _ => None,
        }
    }

    pub(crate) fn push_specpdl_root(&mut self, value: Value) {
        self.specpdl.push(SpecBinding::GcRoot { value });
    }

    /// Push a GcRoot whose value can be UPDATED in place: one reusable root
    /// for traversals that must keep a moving cursor (list tail, hook chain
    /// cons) alive across per-element Lisp callbacks. A single slot per
    /// traversal — per-entry root pushes multiply root-seed work into every
    /// collection, which exact-GC stress mode turns into minutes.
    pub(crate) fn push_specpdl_root_slot(&mut self, value: Value) -> SpecpdlRootSlot {
        let index = self.specpdl.len();
        self.specpdl.push(SpecBinding::GcRoot { value });
        SpecpdlRootSlot { index }
    }

    /// Re-point an updatable root slot at a new value. The slot must still
    /// be live (not unwound); the debug assert catches misuse.
    pub(crate) fn set_specpdl_root_slot(&mut self, slot: &SpecpdlRootSlot, value: Value) {
        match self.specpdl.get_mut(slot.index) {
            Some(SpecBinding::GcRoot { value: slot_value }) => *slot_value = value,
            other => {
                debug_assert!(false, "specpdl root slot unwound or replaced: {other:?}");
            }
        }
    }

    pub(super) fn save_eval_temp_roots(&self) -> EvalTempRootScopeState {
        EvalTempRootScopeState {
            saved_len: self.eval_temp_roots.len(),
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn restore_eval_temp_roots(&mut self, scope: EvalTempRootScopeState) {
        self.eval_temp_roots.truncate(scope.saved_len);
    }

    pub(super) fn restore_eval_temp_roots_to_sequence(&mut self, scope: EvalTempRootScopeState) {
        let current_len = self.eval_temp_roots.len();
        let let_temp_roots = if current_len > scope.saved_len {
            self.eval_temp_roots[scope.saved_len..].to_vec()
        } else {
            Vec::new()
        };
        self.eval_temp_roots
            .truncate(scope.saved_len.min(current_len));
        let Some(frame) = self.sequence_temp_root_frames.last_mut() else {
            return;
        };
        if scope.saved_len < frame.saved_len {
            return;
        }
        frame.let_temp_roots = let_temp_roots;
        self.refresh_current_sequence_temp_roots();
    }

    pub(super) fn push_eval_temp_root(&mut self, value: Value) {
        self.eval_temp_roots.push(value);
    }

    pub(super) fn push_eval_temp_root_slot(&mut self, value: Value) -> usize {
        let slot = self.eval_temp_roots.len();
        self.eval_temp_roots.push(value);
        slot
    }

    pub(super) fn set_eval_temp_root_slot(&mut self, slot: usize, value: Value) {
        if let Some(root) = self.eval_temp_roots.get_mut(slot) {
            *root = value;
        }
    }

    pub(super) fn save_sequence_temp_roots(&mut self) -> SequenceTempRootScopeState {
        let saved_len = self.eval_temp_roots.len();
        self.sequence_temp_root_frames.push(SequenceTempRootFrame {
            saved_len,
            call_roots: Vec::new(),
            let_temp_roots: Vec::new(),
        });
        SequenceTempRootScopeState { saved_len }
    }

    pub(super) fn restore_sequence_temp_roots(&mut self, scope: SequenceTempRootScopeState) {
        let frame = self
            .sequence_temp_root_frames
            .pop()
            .expect("sequence temp root restore without matching save");
        let saved_len = frame.saved_len;
        debug_assert_eq!(saved_len, scope.saved_len);
        self.eval_temp_roots.truncate(scope.saved_len);
    }

    pub(super) fn record_sequence_temp_roots_from_backtrace(&mut self, count: usize) {
        let Some(frame) = self.sequence_temp_root_frames.last() else {
            return;
        };
        let saved_len = frame.saved_len;
        let Some(entry) = self.specpdl.get(count) else {
            return;
        };
        let Some((_, values, _, unevalled)) = self.backtrace_entry_values(entry) else {
            return;
        };
        if unevalled {
            return;
        }
        let frame_index = self.sequence_temp_root_frames.len() - 1;
        self.sequence_temp_root_frames[frame_index].call_roots = values.to_vec();
        debug_assert!(self.eval_temp_roots.len() >= saved_len);
        self.refresh_current_sequence_temp_roots();
    }

    pub(super) fn refresh_current_sequence_temp_roots(&mut self) {
        let Some(frame) = self.sequence_temp_root_frames.last() else {
            return;
        };
        self.eval_temp_roots.truncate(frame.saved_len);
        self.eval_temp_roots
            .extend(frame.call_roots.iter().copied());
        self.eval_temp_roots
            .extend(frame.let_temp_roots.iter().copied());
    }

    pub(crate) fn record_save_excursion(&mut self) -> Option<usize> {
        let (buffer_id, point) = self
            .buffers
            .current_buffer()
            .map(|buffer| (buffer.id, buffer.point_lisp_char_pos()))?;
        let marker = super::super::marker::make_registered_buffer_marker(
            &mut self.buffers,
            buffer_id,
            point,
            false,
        );
        let marker_id = super::super::marker::marker_id_value(&marker)
            .expect("registered save-excursion marker should carry an id");
        let count = self.specpdl.len();
        self.specpdl.push(SpecBinding::SaveExcursion {
            buffer_id,
            marker_id,
            marker,
        });
        Some(count)
    }

    pub(crate) fn restore_specpdl_roots(&mut self, scope: SpecpdlRootScopeState) {
        if self.specpdl.len() <= scope.saved_len {
            return;
        }
        if self.specpdl[scope.saved_len..]
            .iter()
            .all(|binding| matches!(binding, SpecBinding::GcRoot { .. }))
        {
            self.specpdl.truncate(scope.saved_len);
            return;
        }

        // GNU's specpdl is unwound in place by moving the stack pointer.
        // Keep Neomacs' extra GC-root entries just as cheap: remove root-only
        // sentinels from the active suffix without allocating a temporary tail.
        let mut index = 0usize;
        self.specpdl.retain(|binding| {
            let keep = index < scope.saved_len || !matches!(binding, SpecBinding::GcRoot { .. });
            index += 1;
            keep
        });
    }
    pub(crate) fn push_vm_root_frame(&mut self) {
        self.vm_root_frames.push(VmRootFrame::new());
    }

    pub(crate) fn pop_vm_root_frame(&mut self) {
        self.vm_root_frames.pop();
    }

    pub(crate) fn push_vm_frame_root(&mut self, value: Value) {
        self.vm_root_frames
            .last_mut()
            .expect("VM root frame missing")
            .roots
            .push(value);
    }

    pub(crate) fn push_vm_frame_root_slot(&mut self, value: Value) -> usize {
        let roots = &mut self
            .vm_root_frames
            .last_mut()
            .expect("VM root frame missing")
            .roots;
        let slot = roots.len();
        roots.push(value);
        slot
    }

    pub(crate) fn set_vm_frame_root_slot(&mut self, slot: usize, value: Value) {
        self.vm_root_frames
            .last_mut()
            .expect("VM root frame missing")
            .roots[slot] = value;
    }

    pub(crate) fn push_eval_result_roots(&mut self, result: &EvalResult) {
        match result {
            Ok(value) => self.push_vm_frame_root(*value),
            Err(Flow::Signal(sig)) => {
                for value in sig.data.iter().copied() {
                    self.push_vm_frame_root(value);
                }
                if let Some(raw_data) = sig.raw_data {
                    self.push_vm_frame_root(raw_data);
                }
            }
            Err(Flow::Throw(thrown)) => {
                self.push_vm_frame_root(thrown.tag);
                self.push_vm_frame_root(thrown.value);
            }
            Err(Flow::ThreadBlocked(blocked)) => {
                self.push_vm_frame_root(blocked.blocker);
                self.push_vm_frame_root(blocked.remaining_forms);
            }
            // No Lisp values to root.
            Err(Flow::Shutdown(_)) => {}
        }
    }

    pub(crate) fn save_vm_roots(&mut self) -> VmRootScopeState {
        let pushed_vm_root_frame = self.vm_root_frames.is_empty();
        if pushed_vm_root_frame {
            self.push_vm_root_frame();
        }
        VmRootScopeState {
            pushed_vm_root_frame,
            saved_vm_root_frame_len: self.vm_root_frames.last().map(|frame| frame.roots.len()),
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn save_vm_frame_roots(&self) -> usize {
        self.vm_root_frames
            .last()
            .expect("VM root frame missing")
            .roots
            .len()
    }

    pub(crate) fn restore_vm_frame_roots(&mut self, saved_len: usize) {
        self.vm_root_frames
            .last_mut()
            .expect("VM root frame missing")
            .roots
            .truncate(saved_len);
    }

    pub(crate) fn restore_vm_roots(&mut self, scope: VmRootScopeState) {
        if let Some(saved_len) = scope.saved_vm_root_frame_len {
            self.restore_vm_frame_roots(saved_len);
        }
        if scope.pushed_vm_root_frame {
            self.pop_vm_root_frame();
        }
    }

    pub(crate) fn unbind_to_with_result(&mut self, count: usize, result: EvalResult) -> EvalResult {
        let specpdl_len = self.specpdl.len();
        if specpdl_len == count {
            return result;
        }
        if specpdl_len == count + 1 {
            let trivial_pop = self.specpdl.last().and_then(trivial_spec_binding_pop);
            if let Some(trivial_pop) = trivial_pop {
                if let TrivialSpecBindingPop::BacktraceArgs(args) = trivial_pop {
                    self.release_backtrace_args(&args);
                }
                // SAFETY: `trivial_spec_binding_pop` is the closed proof that
                // the top variant has no owned Rust payload. Its only copied
                // cleanup state is `BacktraceArgs`, which was released above.
                // This is GNU's common `specpdl_ptr--` without routing every
                // call through `SpecBinding`'s whole-enum drop glue.
                unsafe { self.specpdl.set_len(count) };
                return result;
            }
        }
        // GNU's six `if (backtrace_debug_on_exit (...)) val = call_debugger
        // (list2 (Qexit, val));` sites, as one.  Reaching here means the suffix
        // is not all-trivial, and `trivial_spec_binding_pop` treats exactly the
        // `debug_on_exit: true` frames as non-trivial -- so no fast path above
        // can have skipped a flagged frame, and this is the only place one can
        // be popped.  GNU runs the debugger BEFORE `specpdl_ptr--` so the frame
        // is still in the backtrace it shows.
        let result = self.run_debug_on_exit(count, result);
        if self.specpdl.len() <= count {
            // The debugger's own Lisp unwound past this region (a `throw` out
            // of `debug` reaches `top-level`); there is nothing left to pop.
            return result;
        }

        // GNU `unbind_to` over a suffix of plain, untrapped `let` bindings:
        // pop each entry and store its old value back.  No Lisp runs, so
        // `result` needs no root protection and only GNU's own quit-flag
        // bracket applies.
        if self.specpdl_suffix_is_plain_lets(count) {
            self.unbind_plain_lets(count);
            return result;
        }

        if self.specpdl[count..]
            .iter()
            .all(spec_binding_has_trivial_unbind)
        {
            // GNU's common eval path pops SPECPDL_BACKTRACE by moving
            // specpdl_ptr. Avoid result rooting and full unwind work when the
            // suffix has no cleanup or dynamic binding restoration.
            self.release_backtrace_args_in_specpdl_suffix(count);
            self.specpdl.truncate(count);
            return result;
        }

        self.drain_unwind_to(count, result)
    }

    /// Drain every specbinding down to COUNT while carrying RESULT through
    /// arbitrary Lisp cleanup.
    ///
    /// Each failed cleanup has already popped its own entry. Keep unwinding so
    /// lower bindings cannot leak; if another cleanup exits nonlocally, that
    /// later/lower flow supersedes the earlier one just as it does in GNU.
    /// Every specpdl entry above `count` is a `let` of a plain value cell with
    /// no variable watcher — the bindings GNU's `unbind_to` restores with a
    /// bare `SET_SYMBOL_VAL`.
    pub(super) fn specpdl_suffix_is_plain_lets(&self, count: usize) -> bool {
        self.specpdl[count..].iter().all(|binding| match binding {
            SpecBinding::Let { sym_id, .. } => {
                self.obarray.get_by_id(*sym_id).is_some_and(|sym| {
                    sym.redirect() == crate::emacs_core::symbol::SymbolRedirect::Plainval
                }) && !self.watchers.has_watchers(*sym_id)
            }
            _ => false,
        })
    }

    /// Restore a suffix that [`Self::specpdl_suffix_is_plain_lets`] accepted.
    pub(super) fn unbind_plain_lets(&mut self, count: usize) {
        let quitf = self.quit_flag_value();
        if !quitf.is_nil() {
            self.set_quit_flag_value(Value::NIL);
        }
        while self.specpdl.len() > count {
            let Some(SpecBinding::Let { sym_id, old_value }) = self.specpdl.pop() else {
                unreachable!("the suffix was checked to hold only plain let bindings");
            };
            self.obarray
                .store_plain_value_id(sym_id, old_value.as_plain());
            self.sync_cached_runtime_binding_by_id(sym_id, old_value.get().unwrap_or(Value::NIL));
        }
        if !quitf.is_nil() && self.quit_flag_value().is_nil() {
            self.set_quit_flag_value(quitf);
        }
    }

    pub(super) fn drain_unwind_to(&mut self, count: usize, result: EvalResult) -> EvalResult {
        // GNU eval.c `unbind_to(count, value)` carries VALUE through cleanup.
        // In Rust the value is not on the C stack/register root set, so keep
        // all heap payloads rooted while unwind-protect/watchers may allocate.
        let root_scope = self.save_vm_roots();
        self.push_eval_result_roots(&result);
        let mut cleanup_error = None;
        while self.specpdl.len() > count {
            match self.unbind_to_result(count) {
                Ok(()) => break,
                Err(flow) => {
                    let rooted_error: EvalResult = Err(flow);
                    self.push_eval_result_roots(&rooted_error);
                    cleanup_error = rooted_error.err();
                    // A cleanup nonlocal exit has already popped its own
                    // specbinding. Continue toward COUNT so lower dynamic
                    // bindings are not leaked. A lower cleanup flow replaces
                    // this one, matching GNU's nested nonlocal unwinding.
                }
            }
        }
        self.restore_vm_roots(root_scope);
        if let Some(flow) = cleanup_error {
            return Err(flow);
        }
        result
    }

    /// Execute BODY and unwind every typed/Lisp cleanup it registers, even
    /// when BODY returns early with `?`.
    ///
    /// This is the native-runtime equivalent of GNU's
    /// `record_unwind_protect` + `unbind_to`: callers put the fallible body in
    /// the closure, so Rust control flow cannot bypass the cleanup boundary.
    pub(crate) fn with_unwind_scope(
        &mut self,
        body: impl FnOnce(&mut Self) -> EvalResult,
    ) -> EvalResult {
        let count = self.specpdl.len();
        let result = body(self);
        self.drain_unwind_to(count, result)
    }

    #[inline]
    /// Grow the JIT residual-root window stack to hold at least `need` slots
    /// and republish the pointer/capacity mirrors generated code reads. Called
    /// from the cold grow shim only; new slots are NIL so every slot below
    /// `len` is always a valid traced Value.
    pub(crate) fn jit_root_stack_grow(&mut self, need: usize) {
        let new_len = need.max(64).next_power_of_two();
        self.jit_root_stack.resize(new_len, Value::NIL);
        self.jit_root_stack_ptr = self.jit_root_stack.as_mut_ptr();
        self.jit_root_stack_cap = new_len;
    }

    /// GNU `specpdl_ptr--` for the JIT native-call exit: pop the call's own
    /// `BacktraceNative` frame without touching the result value at all.
    /// Returns false when the stack is not in the balanced single-frame
    /// state (nested imbalance, debug residue) — the caller then takes the
    /// general [`Self::pop_bytecode_backtrace_frame_with_result`] path.
    #[inline]
    pub(crate) fn pop_native_backtrace_frame(&mut self, count: usize) -> bool {
        if self.specpdl.len() == count + 1
            && matches!(
                self.specpdl.last(),
                Some(SpecBinding::BacktraceNative { .. })
            )
        {
            // SAFETY: BacktraceNative owns no heap payload (a Value plus a
            // raw pointer and a length), so the length store alone is the
            // pointer-decrement pop; no drop glue needs to run.
            unsafe { self.specpdl.set_len(count) };
            true
        } else {
            false
        }
    }

    pub(crate) fn pop_bytecode_backtrace_frame_with_result(
        &mut self,
        count: usize,
        result: EvalResult,
    ) -> EvalResult {
        // GNU Ffuncall/Breturn exit protocol: a balanced call whose only
        // outstanding entry is its own non-debug Backtrace frame pops it with
        // a pointer decrement (eval.c "specpdl_ptr--"). debug_on_exit: false
        // is load-bearing — a future real backtrace-debug implementation
        // (GNU calls call_debugger(list2(Qexit, val)) first) must land in the
        // unbind_to_with_result fallback below.
        let can_pop = self.specpdl.len() == count + 1
            && matches!(
                self.specpdl.last(),
                Some(SpecBinding::Backtrace {
                    args,
                    debug_on_exit: false,
                    ..
                }) if args.is_evaluated()
            )
            || self.specpdl.len() == count + 1
                && matches!(
                    self.specpdl.last(),
                    // The inline evaluated variants (structurally
                    // debug_on_exit: false) own no side-stack payload —
                    // pointer-decrement pop, same as GNU specpdl_ptr--.
                    // BacktraceNative in particular is what every JIT
                    // native call's frame is; without this arm each pop
                    // took the full unbind_to fallback (~44 Ir measured).
                    Some(
                        SpecBinding::Backtrace1 {
                            debug_on_exit: false,
                            ..
                        } | SpecBinding::Backtrace2 { .. }
                            | SpecBinding::BacktraceNative { .. }
                    )
                );

        if can_pop {
            let binding = self.specpdl.pop().expect("can_pop checked len");
            if let SpecBinding::Backtrace { args, .. } = &binding {
                // Out-of-line args (Evaluated(_)) hold a backtrace_args_stack
                // slot that must unwind LIFO; release no-ops for the inline
                // variants.
                self.release_backtrace_args(args);
            }
            return result;
        }

        self.unbind_to_with_result(count, result)
    }

    #[inline]
    pub(crate) fn pop_bytecode_backtrace_token_with_result(
        &mut self,
        frame: BytecodeBacktraceFrame,
        result: EvalResult,
    ) -> EvalResult {
        self.pop_bytecode_backtrace_frame_with_result(frame.base(), result)
    }

    /// GNU `Breturn`: `if (backtrace_debug_on_exit (pdl)) val = call_debugger
    /// (list2 (Qexit, val));` and only then `specpdl_ptr--`
    /// (`src/bytecode.c:825-828`).
    ///
    /// The pop cannot run the debugger itself -- it is a bare length store with
    /// no result to replace -- so it REFUSES instead, handing the token back so
    /// the caller can take [`Self::pop_bytecode_backtrace_token_with_result`],
    /// which does. That makes "popped a frame that owed a debugger entry"
    /// unconstructible through this API rather than merely unreached.
    ///
    /// Ledger 172 §7 argued no flagged frame could reach the fast pops, and
    /// that was measured false: `backtrace-debug` flags an arbitrary live frame
    /// by index, including a byte-compiled caller already routed to the fast
    /// return.  Measured, `-Q --batch`, `tmp/l183-p10.el` -- a `byte-compile`d
    /// function whose callee runs `(backtrace-debug 1 t)` calls the debugger
    /// once in GNU and called it zero times here, while the interpreted twin
    /// agreed in both editors.
    #[inline(always)]
    pub(crate) fn pop_fast_bytecode_backtrace_frame(
        &mut self,
        frame: BytecodeBacktraceFrame,
    ) -> FastBytecodePop {
        if self.backtrace_frame_wants_debug_on_exit(frame.base()) {
            return FastBytecodePop::OwesDebugOnExit(frame);
        }
        self.pop_fast_bytecode_backtrace_frame_unchecked(frame);
        FastBytecodePop::Popped
    }

    /// [`Self::pop_fast_bytecode_backtrace_frame`] without its
    /// `backtrace_debug_on_exit` test.
    ///
    /// The single caller is the iterative driver's `Breturn`, whose eligibility
    /// gate has already asked the question about this exact specpdl index --
    /// `cleanup.specpdl_base - 1` -- three lines earlier and routed a flagged
    /// frame to the generic unwind.  Asking twice on the hottest return path in
    /// the interpreter buys nothing; asking NOWHERE is the bug this entry
    /// fixes, which is why the proof is named at the call site.
    #[inline(always)]
    pub(crate) fn pop_fast_bytecode_backtrace_frame_unchecked(
        &mut self,
        frame: BytecodeBacktraceFrame,
    ) {
        debug_assert!(
            !self.backtrace_frame_wants_debug_on_exit(frame.base()),
            "the unchecked bytecode pop was handed a frame owing a debugger entry"
        );
        let frame_word = frame.0;
        debug_assert_eq!(
            self.specpdl.len(),
            frame.base() + 1,
            "fast bytecode pop requires its frame to remain the specpdl top"
        );
        debug_assert!(matches!(
            self.specpdl.last(),
            Some(SpecBinding::Backtrace {
                args,
                debug_on_exit: false,
                ..
            }) if args.is_bytecode_storage()
        ));
        let count = if frame_word & BytecodeBacktraceFrame::OWNED_ARGS_FLAG == 0 {
            // The ordinary token is exactly its base: no mask or descriptor
            // decode on GNU's Breturn-shaped hot path.
            frame_word
        } else {
            self.release_oversized_bytecode_backtrace_frame(frame_word)
        };

        // SAFETY: `BytecodeBacktraceFrame` is private, non-Copy, and only
        // constructed immediately after pushing this Backtrace variant. The
        // interpreter driver consumes it only after the nested call restored
        // the exact specpdl depth; debug builds verify that protocol above.
        // Backtrace's fields (`Value`,
        // `BacktraceArgs`, bool) need no drop, so reducing the length is GNU's
        // `specpdl_ptr--` without leaking an owned Rust payload. Any path that
        // can leave another binding or debug-on-exit state uses the exhaustive
        // `pop_bytecode_backtrace_frame_with_result`/`unbind_to` path instead.
        unsafe { self.specpdl.set_len(count) };
    }

    /// Release the semantic fallback for a bytecode argument span that could
    /// not fit the compact descriptor. Keeping both the descriptor decode and
    /// side-stack maintenance here leaves the ordinary return as one predicted
    /// branch plus the `specpdl` pointer decrement.
    #[cold]
    #[inline(never)]
    pub(super) fn release_oversized_bytecode_backtrace_frame(
        &mut self,
        frame_word: usize,
    ) -> usize {
        let count = frame_word & BytecodeBacktraceFrame::BASE_MASK;
        let args = match self.specpdl.get(count) {
            Some(SpecBinding::Backtrace {
                args,
                debug_on_exit: false,
                ..
            }) => *args,
            _ => panic!("oversized bytecode pop requires its own non-debug backtrace frame"),
        };
        let index = args
            .owned_index()
            .expect("oversized bytecode backtrace must own an argument slot");
        self.release_owned_backtrace_args(index);
        count
    }

    pub(super) fn apply_internal(
        &mut self,
        function: Value,
        args: LispArgVec,
        record_backtrace: bool,
    ) -> EvalResult {
        self.maybe_quit_before_gc()?;
        self.enter_interpreted_eval_depth()?;
        let bt_count = self.specpdl.len();
        if record_backtrace {
            self.push_backtrace_frame(function, &args);
        }
        let result = {
            if self.gc_safe_point_exact_should_collect() {
                self.gc_collect_from_current_roots();
            }
            // GNU Ffuncall eval.c:3185-3190 in order: record_in_backtrace,
            // maybe_gc, then `if (debug_on_next_call) do_debug_on_call
            // (Qlambda, count)`.  Only when a frame was recorded -- GNU's
            // Ffuncall always records one, and `record_backtrace: false` is
            // this port's marker for a caller that already did.
            let armed = if record_backtrace {
                self.take_debug_on_call_arm(DebugOnCallCode::Funcall)
            } else {
                None
            };
            let entered = match armed {
                Some(arm) => self.do_debug_on_call(arm),
                None => Ok(()),
            };
            // GNU does not probe stack space for every funcall. Keep growth
            // checks at the function-application boundary, but only on coarse
            // depth intervals so normal startup is not dominated by TLS lookups
            // in stacker::maybe_grow.
            match entered {
                Err(flow) => Err(flow),
                Ok(()) => {
                    self.maybe_grow_eval_stack(|ctx| ctx.funcall_general_untraced(function, args))
                }
            }
        };
        self.depth -= 1;
        let result = self.dispatch_signal_result_if_needed(result);
        self.unbind_to_with_result(bt_count, result)
    }

    /// Apply a function value to evaluated arguments.
    pub(crate) fn apply<A>(&mut self, function: Value, args: A) -> EvalResult
    where
        A: Into<LispArgVec>,
    {
        self.apply_internal(function, args.into(), true)
    }

    /// Call Lisp while suppressing signaled conditions, matching GNU Emacs's
    /// `safe_funcall` (`src/eval.c`).  GNU uses this at diagnostic and display
    /// boundaries where a broken callback must not recursively replace the
    /// operation that is already handling an error.  `throw` and suspended
    /// thread flow remain nonlocal exits; `internal_condition_case_n` does not
    /// catch them in GNU either.
    ///
    /// `nil` is deliberately both the error sentinel and a valid callback
    /// result.  Callers that need a fallback use it in either case, exactly as
    /// GNU's `safe_calln` callers do.
    pub(crate) fn safe_funcall<A>(&mut self, function: Value, args: A) -> EvalResult
    where
        A: Into<LispArgVec>,
    {
        let specpdl_count = self.specpdl.len();
        let result = (|| {
            self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-redisplay"), Value::T)?;
            // GNU's catch-all internal condition handler prevents the debugger
            // from running. Neomacs dispatches signals on function return, so
            // an explicit binding provides the same boundary before we demote
            // the resulting Flow::Signal below.
            self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-debugger"), Value::T)?;
            self.apply(function, args)
        })();
        let result = self.unbind_to_with_result(specpdl_count, result);
        match result {
            Err(Flow::Signal(flow)) => {
                tracing::debug!(?flow, "error muted by safe_funcall");
                Ok(Value::NIL)
            }
            other => other,
        }
    }

    /// Apply from GNU's Lisp-visible `apply` / `funcall` subrs.
    ///
    /// GNU bytecode `Bcall` increments the same `lisp_eval_depth` counter
    /// before dispatching. If the callee is Lisp-visible `apply` or
    /// `funcall`, GNU then enters `Ffuncall`, whose depth guard observes the
    /// active `Bcall`. Neomacs mirrors that by using the single shared
    /// `Context::depth` counter for both interpreter and bytecode call sites.
    pub(crate) fn apply_from_lisp_funcall<A>(&mut self, function: Value, args: A) -> EvalResult
    where
        A: Into<LispArgVec>,
    {
        self.apply_internal(function, args.into(), true)
    }

    #[inline]
    pub(crate) fn apply0(&mut self, function: Value) -> EvalResult {
        self.apply(function, LispArgVec::new())
    }

    #[inline]
    pub(crate) fn apply1(&mut self, function: Value, arg0: Value) -> EvalResult {
        let mut args = LispArgVec::new();
        args.push(arg0);
        self.apply(function, args)
    }

    #[inline]
    pub(crate) fn apply2(&mut self, function: Value, arg0: Value, arg1: Value) -> EvalResult {
        let mut args = LispArgVec::new();
        args.push(arg0);
        args.push(arg1);
        self.apply(function, args)
    }

    pub(crate) fn apply_untraced<A>(&mut self, function: Value, args: A) -> EvalResult
    where
        A: Into<LispArgVec>,
    {
        self.apply_internal(function, args.into(), false)
    }

    /// Apply FUNC to ARGS, but record FRAME_FUNCTION (not FUNC) in the
    /// runtime backtrace frame. Used by `eval_sub_cons` when the form
    /// dispatches through a symbol: the symbol is what GNU stores in
    /// specpdl (and what `backtrace-frame` returns), while the
    /// resolved function cell is what actually runs.
    ///
    /// Mirrors GNU's `eval_sub` SYMBOLP arm at `eval.c:2600-2625`,
    /// where `original_fun` (the symbol) is the value written into the
    /// specpdl entry via `record_in_backtrace (original_fun, args, ...)`.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn apply_with_frame_function(
        &mut self,
        frame_function: Value,
        func: Value,
        args: impl Into<LispArgVec>,
    ) -> EvalResult {
        let args = args.into();
        let bt_count = self.specpdl.len();
        self.push_backtrace_frame(frame_function, &args);
        let result = self.maybe_gc_and_quit().and_then(|_| {
            self.maybe_grow_eval_stack(|ctx| ctx.funcall_general_untraced(func, args))
        });
        let result = self.dispatch_signal_result_if_needed(result);
        self.unbind_to_with_result(bt_count, result)
    }

    /// Unified function dispatch — matches GNU Emacs's funcall_general.
    /// Called by both the tree-walking interpreter (via apply) and the
    /// bytecode VM (via Vm::call_function).
    pub(crate) fn funcall_general<A>(&mut self, function: Value, args: A) -> EvalResult
    where
        A: Into<LispArgVec>,
    {
        let args = args.into();
        let bt_count = self.specpdl.len();
        self.push_backtrace_frame(function, &args);
        // Same GNU Ffuncall arm site as `apply_internal` (eval.c:3189-3190):
        // this entry records its own backtrace frame, so it is a funcall in
        // GNU's sense and must be armable.
        let result = match self.take_debug_on_call_arm(DebugOnCallCode::Funcall) {
            Some(arm) => self
                .do_debug_on_call(arm)
                .and_then(|()| self.funcall_general_untraced(function, args)),
            None => self.funcall_general_untraced(function, args),
        };
        let result = self.dispatch_signal_result_if_needed(result);
        self.unbind_to_with_result(bt_count, result)
    }

    /// Execute a bytecode function through the JIT tier-up seam. This is THE
    /// dispatch point for every bytecode call: `funcall_general_untraced`
    /// (interpreter / funcall / apply) AND the VM's own bytecode→bytecode
    /// fast paths (`Vm::call_function_untraced_owned`) route here, so a
    /// function called only from compiled code accumulates heat and tiers up
    /// exactly like one called through funcall/eval. (Previously only the
    /// funcall path consulted the plan, so in fully byte-compiled code —
    /// where calls flow through the VM's Op::Call — the JIT never engaged
    /// at all.)
    ///
    /// The `match` over the dispatch plan is intentionally exhaustive: once
    /// a compiled tier exists it MUST be handled here, enforced by the
    /// compiler. Behind the `jit` feature; the default build is unchanged.
    pub(crate) fn execute_bytecode_call(
        &mut self,
        bc_data: &super::super::bytecode::ByteCodeFunction,
        args: LispArgVec,
        func_value: Value,
    ) -> EvalResult {
        #[cfg(feature = "jit")]
        {
            use crate::emacs_core::jit::Plan;
            match bc_data
                .jit_runtime()
                .dispatch_sized(bc_data.executable_ops().len())
            {
                Plan::Interpret => {
                    let mut vm = super::super::bytecode::Vm::from_context(self);
                    vm.execute_with_func_value(bc_data, args, func_value)
                }
                Plan::Compiled => {
                    // Run native code when the body is compilable and the
                    // call is valid (arity is checked inside
                    // try_run_compiled). Ok(None) — non-compilable body, a
                    // deopt (sound to rerun: guards never follow a call),
                    // or an arity mismatch — falls back to the Tier-0
                    // interpreter; Err propagates a Flow raised by a
                    // runtime call inside native code.
                    //
                    // Root the executing function for the duration: native
                    // code references its constants by raw bits, and a
                    // runtime call inside (Call/cons) may trigger GC.
                    let saved_roots = save_scratch_gc_roots();
                    push_scratch_gc_root(func_value);
                    let ctx_ptr = self as *mut Context;
                    let native = crate::emacs_core::jit::try_run_compiled(
                        ctx_ptr, bc_data, func_value, &args,
                    );
                    restore_scratch_gc_roots(saved_roots);
                    match native {
                        Ok(Some(bits)) => Ok(crate::emacs_core::value::Value::from_bits(bits)),
                        Ok(None) => {
                            crate::emacs_core::jit::note_seam_interp_fallback();
                            let mut vm = super::super::bytecode::Vm::from_context(self);
                            vm.execute_with_func_value(bc_data, args, func_value)
                        }
                        Err(flow) => Err(flow),
                    }
                }
            }
        }
        #[cfg(not(feature = "jit"))]
        {
            let mut vm = super::super::bytecode::Vm::from_context(self);
            vm.execute_with_func_value(bc_data, args, func_value)
        }
    }

    /// [`Context::execute_bytecode_call`] for arguments that already live on
    /// the GC-traced `bc_buf` at `[args_start, args_start + nargs)` — the
    /// VM's hot bytecode→bytecode path. The interpreter tier runs directly
    /// from the stack span (no `LispArgVec`); the compiled tier materializes
    /// one owned copy for `try_run_compiled` (native code may hold `args_ptr`
    /// across `bc_buf` reallocations, so it must not point into the Vec),
    /// which is amortized by the native run it precedes.
    pub(crate) fn execute_bytecode_call_from_stack(
        &mut self,
        bc_data: &super::super::bytecode::ByteCodeFunction,
        args_start: usize,
        nargs: usize,
        func_value: Value,
    ) -> EvalResult {
        match self.dispatch_bytecode_call_from_stack(bc_data, args_start, nargs, func_value) {
            BytecodeStackCallDispatch::Interpret => {
                let mut vm = super::super::bytecode::Vm::from_context(self);
                vm.execute_from_stack_args(bc_data, args_start, nargs, func_value)
            }
            BytecodeStackCallDispatch::Complete(result) => result,
        }
    }

    /// Consult the tier dispatcher once without recursively entering Tier 0.
    ///
    /// This is the typed seam used by the bytecode interpreter's iterative
    /// `Bcall` transition.  `Interpret` means the caller must install a Tier-0
    /// frame; `Complete` means native code either returned or raised a flow.
    pub(crate) fn dispatch_bytecode_call_from_stack(
        &mut self,
        bc_data: &super::super::bytecode::ByteCodeFunction,
        args_start: usize,
        nargs: usize,
        func_value: Value,
    ) -> BytecodeStackCallDispatch {
        #[cfg(feature = "jit")]
        {
            use crate::emacs_core::jit::Plan;
            match bc_data
                .jit_runtime()
                .dispatch_sized(bc_data.executable_ops().len())
            {
                Plan::Interpret => BytecodeStackCallDispatch::Interpret,
                Plan::Compiled => {
                    let args = LispArgVec::from_slice(&self.bc_buf[args_start..args_start + nargs]);
                    let saved_roots = save_scratch_gc_roots();
                    push_scratch_gc_root(func_value);
                    let ctx_ptr = self as *mut Context;
                    let native = crate::emacs_core::jit::try_run_compiled(
                        ctx_ptr, bc_data, func_value, &args,
                    );
                    restore_scratch_gc_roots(saved_roots);
                    match native {
                        Ok(Some(bits)) => BytecodeStackCallDispatch::Complete(Ok(
                            crate::emacs_core::value::Value::from_bits(bits),
                        )),
                        Ok(None) => {
                            crate::emacs_core::jit::note_seam_interp_fallback();
                            BytecodeStackCallDispatch::Interpret
                        }
                        Err(flow) => BytecodeStackCallDispatch::Complete(Err(flow)),
                    }
                }
            }
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = (bc_data, args_start, nargs, func_value);
            BytecodeStackCallDispatch::Interpret
        }
    }

    pub(crate) fn funcall_general_untraced(
        &mut self,
        function: Value,
        args: impl Into<LispArgVec>,
    ) -> EvalResult {
        let args = args.into();
        match function.kind() {
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                // get_bytecode_data returns a reference into the GC-managed
                // ByteCodeObj.  GNU's bytecode interpreter executes from the
                // function struct in place, never copying.  Don't clone here
                // either — bytecode functions can have thousands of ops, and
                // cloning per call dominated debug-build batch-byte-compile
                // runtime.
                let bc_data = function.get_bytecode_data().unwrap();
                self.execute_bytecode_call(bc_data, args, function)
            }
            ValueKind::Veclike(VecLikeType::Lambda) => self.apply_lambda(function, args),
            ValueKind::Veclike(VecLikeType::Macro) => self.apply_lambda(function, args),
            ValueKind::Subr(_) => self.apply_subr_object(function, args, true),
            ValueKind::Veclike(VecLikeType::Subr) => self.apply_subr_object(function, args, true),
            ValueKind::Veclike(VecLikeType::ModuleFunction) => {
                self.apply_module_function(function, args)
            }
            ValueKind::Symbol(id) => self.apply_symbol_callable_untraced(id, args, true),
            ValueKind::T => self.apply_symbol_callable_untraced(intern("t"), args, true),
            ValueKind::Nil => Err(signal(
                LispCondition::VoidFunction,
                vec![Value::symbol("nil")],
            )),
            _ if function.is_symbol_with_pos() => {
                // Transparently unwrap symbol-with-pos → bare symbol for funcall dispatch.
                let bare = function.as_symbol_with_pos_sym().unwrap();
                self.funcall_general_untraced(bare, args)
            }
            ValueKind::Cons => {
                if super::super::autoload::is_autoload_value(&function) {
                    Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("symbolp"), function],
                    ))
                } else if cons_head_symbol_id(&function) == Some(lambda_symbol()) {
                    self.apply_lambda(function, args)
                } else {
                    Err(signal(LispCondition::InvalidFunction, vec![function]))
                }
            }
            _ => Err(signal(LispCondition::InvalidFunction, vec![function])),
        }
    }

    /// Convert a `(lambda ...)` or `(closure ...)` cons cell into a
    /// `Value::Lambda`.  This mirrors GNU Emacs's `funcall_lambda` which
    /// handles both forms.  Used by both the interpreter and the bytecode VM.
    pub(crate) fn instantiate_callable_cons_form(&mut self, function: Value) -> EvalResult {
        if !function.is_cons() {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        }
        if list_length(&function).is_none() {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        }

        // Unwrap symbol-with-pos on the car so (lambda ...) / (closure ...)
        // forms with position-wrapped heads are recognized.
        let head_val = self.unwrap_symbol(function.cons_car());
        let Some(head_id) = head_val.as_symbol_id() else {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        };
        let mut tail = function.cons_cdr();

        let (env_value, params_value, is_lambda) = if head_id == lambda_symbol() {
            if !tail.is_cons() {
                return Err(signal(LispCondition::InvalidFunction, vec![function]));
            }
            let params_value = tail.cons_car();
            tail = tail.cons_cdr();
            // Mirrors GNU eval_sub lambda handling: a lambda gets
            // a lexical closure env only when
            // Vinternal_interpreter_environment is non-nil (i.e.
            // lexical mode is active). We use self.lexenv as the
            // single source of truth, matching GNU.
            let env_value = if !self.lexenv.is_nil() {
                self.lexenv
            } else {
                Value::NIL
            };
            (env_value, params_value, true)
        } else if head_id == closure_symbol() {
            if !tail.is_cons() {
                return Err(signal(LispCondition::InvalidFunction, vec![function]));
            }
            let env_value = tail.cons_car();
            tail = tail.cons_cdr();
            if !tail.is_cons() {
                return Err(signal(LispCondition::InvalidFunction, vec![function]));
            }
            let params_value = tail.cons_car();
            tail = tail.cons_cdr();
            (env_value, params_value, false)
        } else {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        };

        let specpdl_root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(function);

        let docstring_value = if tail.is_cons() {
            let value = tail.cons_car();
            let rest = tail.cons_cdr();
            if value.is_string() && !rest.is_nil() {
                tail = rest;
                value
            } else {
                Value::NIL
            }
        } else {
            Value::NIL
        };

        let mut doc_form_value = Value::NIL;
        if tail.is_cons() {
            let item = tail.cons_car();
            if let Some(doc_form) = self.eval_dynamic_documentation_value(item)? {
                doc_form_value = doc_form;
                tail = tail.cons_cdr();
            }
        }

        while tail.is_cons() {
            let item = tail.cons_car();
            if !item.is_cons()
                || item.cons_car().as_symbol_id() != Some(declare_symbol())
                || list_length(&item).is_none()
            {
                break;
            }
            tail = tail.cons_cdr();
        }

        let iform_value = if tail.is_cons() {
            let item = tail.cons_car();
            if item.is_cons() && item.cons_car().as_symbol_id() == Some(interactive_symbol_id()) {
                tail = tail.cons_cdr();
                item
            } else {
                Value::NIL
            }
        } else {
            Value::NIL
        };

        let body_value = if tail.is_nil() {
            Value::list(vec![Value::NIL])
        } else {
            tail
        };
        let closure_doc_value = if !doc_form_value.is_nil() {
            doc_form_value
        } else {
            docstring_value
        };

        self.push_specpdl_root(params_value);
        self.push_specpdl_root(body_value);
        self.push_specpdl_root(env_value);
        self.push_specpdl_root(closure_doc_value);
        self.push_specpdl_root(iform_value);

        let result = if is_lambda {
            self.make_interpreted_closure_with_value_runtime_hook(
                function,
                params_value,
                body_value,
                env_value,
                closure_doc_value,
                iform_value,
            )
        } else {
            builtins::symbols::make_interpreted_closure_from_parts(
                &params_value,
                &body_value,
                &env_value,
                Some(&closure_doc_value),
                Some(&iform_value),
            )
        };
        self.restore_specpdl_roots(specpdl_root_scope);
        result
    }

    /// GNU funcall_subr (eval.c:3266-3280) pre-checks arity and
    /// signals `(wrong-number-of-arguments #<subr NAME> NUMARGS)`
    /// with the SUBR value. Call this before dispatching to a
    /// builtin so the check matches GNU's `funcall_subr` exactly
    /// and we never depend on the builtin's expect_args helper
    /// (which would emit `Value::symbol(name)` instead of the
    /// subr value).
    ///
    /// Returns `Some(Flow::Signal)` on arity mismatch, `None` when
    /// the arity is acceptable or the subr has no explicit arity
    /// registered (opt-out).
    #[inline]
    pub(super) fn check_funcall_subr_arity_value(
        &self,
        function: Value,
        nargs: usize,
    ) -> Option<Flow> {
        let (_, entry) = subr_entry_from_value(function)?;
        let min = entry.min_args as usize;
        let max = entry.max_args.map(|m| m as usize);
        // Opt-out: a subr registered with (0, None) has declared
        // "I do my own arity check". Keep the legacy behaviour for
        // those until each one is migrated explicitly.
        if min == 0 && max.is_none() {
            return None;
        }
        let arity_bad = nargs < min || max.is_some_and(|m| nargs > m);
        if arity_bad {
            Some(signal(
                LispCondition::WrongNumberOfArguments,
                vec![function, Value::fixnum(nargs as i64)],
            ))
        } else {
            None
        }
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn check_funcall_subr_arity(&self, sym_id: SymId, nargs: usize) -> Option<Flow> {
        self.check_funcall_subr_arity_value(Value::subr_from_sym_id(sym_id), nargs)
    }

    pub(super) fn dispatch_subr_value_internal(
        &mut self,
        function: Value,
        args: LispArgVec,
        wrong_arity_callee: Value,
    ) -> Option<EvalResult> {
        let (_, entry) = subr_entry_from_value(function)?;
        self.dispatch_subr_entry_internal(entry, args, wrong_arity_callee)
    }

    pub(super) fn dispatch_subr_entry_internal(
        &mut self,
        entry: SubrEntry,
        args: LispArgVec,
        wrong_arity_callee: Value,
    ) -> Option<EvalResult> {
        let func = entry.function?;
        let nargs = args.len();
        if (nargs as u16) < entry.min_args {
            return Some(Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![wrong_arity_callee, Value::fixnum(nargs as i64)],
            )));
        }
        if let Some(max) = entry.max_args
            && nargs as u16 > max
        {
            return Some(Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![wrong_arity_callee, Value::fixnum(nargs as i64)],
            )));
        }
        Some(self.dispatch_subr_func_unchecked(func, args))
    }

    #[inline]
    pub(super) fn dispatch_subr_entry_unchecked(
        &mut self,
        entry: SubrEntry,
        args: LispArgVec,
    ) -> Option<EvalResult> {
        let func = entry.function?;
        Some(self.dispatch_subr_func_unchecked(func, args))
    }

    #[inline]
    pub(crate) fn subr_entry_uses_fixed_value_call(entry: SubrEntry) -> bool {
        entry.dispatch_kind == SubrDispatchKind::Builtin
            && matches!(
                entry.function,
                Some(
                    SubrFn::A0(_)
                        | SubrFn::A1(_)
                        | SubrFn::A2(_)
                        | SubrFn::A3(_)
                        | SubrFn::A4(_)
                        | SubrFn::A5(_)
                        | SubrFn::A6(_)
                        | SubrFn::A7(_)
                        | SubrFn::A8(_),
                )
            )
    }

    #[inline]
    pub(super) fn backtrace_arg_or_nil(&self, args: &BacktraceArgs, index: usize) -> Value {
        match args.view() {
            BacktraceArgsView::Unevalled(_) | BacktraceArgsView::Evaluated0 => Value::NIL,
            BacktraceArgsView::Evaluated(args_index) => self
                .backtrace_args_stack
                .get(args_index)
                .and_then(|storage| match storage {
                    BacktraceArgStorage::Values(values) => values.get(index).copied(),
                    BacktraceArgStorage::BytecodeStack(range) => (index < range.len)
                        .then(|| self.bc_buf.get(range.start.saturating_add(index)).copied())
                        .flatten(),
                })
                .unwrap_or(Value::NIL),
            BacktraceArgsView::EvaluatedBcStack(span) => {
                if index < span.len() {
                    self.bc_buf
                        .get(span.start().saturating_add(index))
                        .copied()
                        .unwrap_or(Value::NIL)
                } else {
                    Value::NIL
                }
            }
        }
    }

    #[inline]
    pub(super) fn backtrace_evaluated_arg_or_nil(&self, count: usize, index: usize) -> Value {
        match self.specpdl.get(count) {
            Some(SpecBinding::Backtrace { args, .. }) if args.is_evaluated() => {
                self.backtrace_arg_or_nil(args, index)
            }
            Some(SpecBinding::Backtrace1 { arg, .. }) => {
                if index == 0 {
                    *arg
                } else {
                    Value::NIL
                }
            }
            Some(SpecBinding::Backtrace2 { arg0, arg1, .. }) => match index {
                0 => *arg0,
                1 => *arg1,
                _ => Value::NIL,
            },
            Some(SpecBinding::BacktraceNative {
                args_ptr, nargs, ..
            }) => {
                if index < *nargs as usize {
                    // SAFETY: variant contract — the caller's call-args
                    // slot outlives this entry.
                    Value::from_bits(unsafe { *args_ptr.add(index) } as usize)
                } else {
                    Value::NIL
                }
            }
            Some(other) => panic!(
                "backtrace_evaluated_arg_or_nil: expected EVALD Backtrace at specpdl[{count}], got {other:?}"
            ),
            None => panic!("backtrace_evaluated_arg_or_nil: specpdl index out of range"),
        }
    }

    #[inline]
    pub(crate) fn dispatch_subr_entry_from_backtrace_unchecked(
        &mut self,
        entry: SubrEntry,
        backtrace_count: usize,
    ) -> Option<EvalResult> {
        match entry.function? {
            SubrFn::A0(func) => Some(func(self)),
            SubrFn::A1(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                Some(func(self, arg0))
            }
            SubrFn::A2(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                Some(func(self, arg0, arg1))
            }
            SubrFn::A3(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                Some(func(self, arg0, arg1, arg2))
            }
            SubrFn::A4(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                let arg3 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 3);
                Some(func(self, arg0, arg1, arg2, arg3))
            }
            SubrFn::A5(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                let arg3 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 3);
                let arg4 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 4);
                Some(func(self, arg0, arg1, arg2, arg3, arg4))
            }
            SubrFn::A6(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                let arg3 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 3);
                let arg4 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 4);
                let arg5 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 5);
                Some(func(self, arg0, arg1, arg2, arg3, arg4, arg5))
            }
            SubrFn::A7(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                let arg3 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 3);
                let arg4 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 4);
                let arg5 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 5);
                let arg6 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 6);
                Some(func(self, arg0, arg1, arg2, arg3, arg4, arg5, arg6))
            }
            SubrFn::A8(func) => {
                let arg0 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 0);
                let arg1 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 1);
                let arg2 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 2);
                let arg3 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 3);
                let arg4 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 4);
                let arg5 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 5);
                let arg6 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 6);
                let arg7 = self.backtrace_evaluated_arg_or_nil(backtrace_count, 7);
                Some(func(self, arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7))
            }
            SubrFn::Many(_) | SubrFn::ManyNoContext(_) | SubrFn::ManySlice(_) => None,
        }
    }

    #[inline]
    pub(super) fn dispatch_subr_func_unchecked(
        &mut self,
        func: crate::tagged::header::SubrFn,
        args: LispArgVec,
    ) -> EvalResult {
        match func {
            crate::tagged::header::SubrFn::Many(func) => func(self, args.into_vec()),
            crate::tagged::header::SubrFn::ManyNoContext(func) => func(args.into_vec()),
            crate::tagged::header::SubrFn::ManySlice(func) => func(self, &args),
            crate::tagged::header::SubrFn::A0(func) => func(self),
            crate::tagged::header::SubrFn::A1(func) => {
                func(self, args.first().copied().unwrap_or(Value::NIL))
            }
            crate::tagged::header::SubrFn::A2(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A3(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A4(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
                args.get(3).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A5(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
                args.get(3).copied().unwrap_or(Value::NIL),
                args.get(4).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A6(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
                args.get(3).copied().unwrap_or(Value::NIL),
                args.get(4).copied().unwrap_or(Value::NIL),
                args.get(5).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A7(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
                args.get(3).copied().unwrap_or(Value::NIL),
                args.get(4).copied().unwrap_or(Value::NIL),
                args.get(5).copied().unwrap_or(Value::NIL),
                args.get(6).copied().unwrap_or(Value::NIL),
            ),
            crate::tagged::header::SubrFn::A8(func) => func(
                self,
                args.first().copied().unwrap_or(Value::NIL),
                args.get(1).copied().unwrap_or(Value::NIL),
                args.get(2).copied().unwrap_or(Value::NIL),
                args.get(3).copied().unwrap_or(Value::NIL),
                args.get(4).copied().unwrap_or(Value::NIL),
                args.get(5).copied().unwrap_or(Value::NIL),
                args.get(6).copied().unwrap_or(Value::NIL),
                args.get(7).copied().unwrap_or(Value::NIL),
            ),
        }
    }

    #[inline]
    pub(super) fn apply_subr_object(
        &mut self,
        function: Value,
        args: LispArgVec,
        _rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        let Some((sym_id, entry)) = subr_entry_from_value(function) else {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        };
        self.apply_subr_object_with_entry(sym_id, function, args, entry)
    }

    #[inline]
    pub(super) fn apply_subr_object_with_entry(
        &mut self,
        sym_id: SymId,
        function: Value,
        args: LispArgVec,
        entry: SubrEntry,
    ) -> EvalResult {
        if entry.dispatch_kind == SubrDispatchKind::SpecialForm {
            return Err(signal(LispCondition::InvalidFunction, vec![function]));
        }
        if entry.dispatch_kind == SubrDispatchKind::ContextCallable {
            return self.apply_evaluator_callable_by_id(sym_id, args);
        }
        if let Some(result) = self.dispatch_subr_entry_internal(entry, args, function) {
            result.map_err(|flow| self.validate_throw(flow))
        } else {
            Err(signal(
                LispCondition::VoidFunction,
                vec![Value::from_sym_id(sym_id)],
            ))
        }
    }

    /// Apply a dynamic module function.
    #[inline]
    pub(super) fn apply_module_function(
        &mut self,
        function: Value,
        args: LispArgVec,
    ) -> EvalResult {
        super::super::dynamic_module::apply_module_function(self, function, args.to_vec())
    }

    #[inline]
    pub(super) fn resolve_named_call_target_by_id(&mut self, sym_id: SymId) -> NamedCallTarget {
        let compiler_overrides_active = self.compiler_function_overrides_active();
        let function_epoch = self.obarray.function_epoch();
        if !compiler_overrides_active {
            // Fast path: a HashMap lookup that returns the cached target
            // when the function epoch hasn't moved.  An epoch mismatch
            // signals that some `defalias`/`fset`/autoload installation
            // happened since the cached entry was recorded; in that case
            // fall through and replace the entry below.
            if let Some(entry) = self.named_call_cache.get(&sym_id)
                && entry.function_epoch == function_epoch
            {
                return entry.target.clone();
            }
        }

        let target =
            if let Some(func) = compiler_function_override_in_obarray(&self.obarray, sym_id) {
                NamedCallTarget::Obarray(func)
            } else if let Some(func) = self.obarray.symbol_function_id(sym_id) {
                match func.kind() {
                    ValueKind::Nil => NamedCallTarget::Void,
                    // `(fset 'foo (symbol-function 'foo))` writes `#<subr foo>` into
                    // the function cell. Treat this as the canonical callable
                    // object, not an obarray indirection cycle.
                    ValueKind::Subr(sid) if sid == sym_id => {
                        NamedCallTarget::Subr(Value::subr_from_sym_id(sid))
                    }
                    ValueKind::Veclike(VecLikeType::Subr) if func.as_subr_id() == Some(sym_id) => {
                        NamedCallTarget::Subr(func)
                    }
                    _ => NamedCallTarget::Obarray(func),
                }
            } else if self.obarray.is_function_unbound_id(sym_id) {
                NamedCallTarget::Void
            } else if lookup_global_subr_entry(sym_id).is_some() {
                NamedCallTarget::Subr(Value::subr_from_sym_id(sym_id))
            } else {
                NamedCallTarget::Void
            };

        if !compiler_overrides_active {
            // Cap the cache to avoid unbounded growth on pathologic
            // workloads.  Past the cap we just stop caching new entries
            // — better to take an O(1) miss than to evict a hot entry.
            if self.named_call_cache.len() < NAMED_CALL_CACHE_CAPACITY {
                self.named_call_cache.insert(
                    sym_id,
                    NamedCallCacheEntry {
                        function_epoch,
                        target: target.clone(),
                    },
                );
            }
        }

        target
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn resolve_named_call_target(&mut self, name: &str) -> NamedCallTarget {
        self.resolve_named_call_target_by_id(intern(name))
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn store_named_call_cache(&mut self, symbol: SymId, target: NamedCallTarget) {
        let function_epoch = self.obarray.function_epoch();
        if self.named_call_cache.len() < NAMED_CALL_CACHE_CAPACITY {
            self.named_call_cache.insert(
                symbol,
                NamedCallCacheEntry {
                    function_epoch,
                    target,
                },
            );
        }
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn apply_named_callable_by_id(
        &mut self,
        sym_id: SymId,
        args: LispArgVec,
        invalid_fn: Value,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        let frame_function = Value::from_sym_id(sym_id);
        let bt_count = self.specpdl.len();
        self.push_backtrace_frame(frame_function, &args);
        let result = self.apply_named_callable_by_id_core(
            sym_id,
            args,
            invalid_fn,
            rewrite_builtin_wrong_arity,
        );
        self.unbind_to_with_result(bt_count, result)
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn apply_named_callable(
        &mut self,
        name: &str,
        args: LispArgVec,
        invalid_fn: Value,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        let frame_function = Value::symbol(name);
        let bt_count = self.specpdl.len();
        self.push_backtrace_frame(frame_function, &args);
        let result =
            self.apply_named_callable_core(name, args, invalid_fn, rewrite_builtin_wrong_arity);
        self.unbind_to_with_result(bt_count, result)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn apply_named_callable_by_id_core(
        &mut self,
        sym_id: SymId,
        args: LispArgVec,
        invalid_fn: Value,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        match self.resolve_named_call_target_by_id(sym_id) {
            NamedCallTarget::Obarray(func) => {
                if super::super::autoload::is_autoload_value(&func) {
                    return self.apply_named_autoload_callable_by_id(
                        sym_id,
                        func,
                        args,
                        rewrite_builtin_wrong_arity,
                    );
                }
                let function_is_callable = self.function_value_is_callable(&func);

                match self.apply_untraced(func, args) {
                    Err(Flow::Signal(sig))
                        if !function_is_callable && sig.symbol == invalid_function_symbol() =>
                    {
                        Err(signal(
                            LispCondition::InvalidFunction,
                            vec![Value::from_sym_id(sym_id)],
                        ))
                    }
                    other => other,
                }
            }
            NamedCallTarget::Subr(func) => {
                let Some((sym_id, entry)) = subr_entry_from_value(func) else {
                    return Err(signal(LispCondition::InvalidFunction, vec![invalid_fn]));
                };
                if entry.dispatch_kind == SubrDispatchKind::SpecialForm {
                    return Err(signal(LispCondition::InvalidFunction, vec![invalid_fn]));
                }
                self.apply_subr_object_with_entry(sym_id, func, args, entry)
            }
            NamedCallTarget::Void => Err(signal(
                LispCondition::VoidFunction,
                vec![Value::from_sym_id(sym_id)],
            )),
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn apply_named_callable_core(
        &mut self,
        name: &str,
        args: LispArgVec,
        invalid_fn: Value,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        match self.resolve_named_call_target(name) {
            NamedCallTarget::Obarray(func) => {
                if super::super::autoload::is_autoload_value(&func) {
                    return self.apply_named_autoload_callable(
                        name,
                        func,
                        args,
                        rewrite_builtin_wrong_arity,
                    );
                }
                let function_is_callable = self.function_value_is_callable(&func);

                match self.apply(func, args) {
                    Err(Flow::Signal(sig))
                        if !function_is_callable && sig.symbol == invalid_function_symbol() =>
                    {
                        Err(signal(
                            LispCondition::InvalidFunction,
                            vec![Value::symbol(name)],
                        ))
                    }
                    other => other,
                }
            }
            NamedCallTarget::Subr(func) => {
                let _sym_id = intern(name);
                let result = self.apply_subr_object(func, args, rewrite_builtin_wrong_arity);
                // Do NOT poison the cache with Void when the subr was found.
                if func
                    .as_subr_id()
                    .and_then(lookup_global_subr_entry)
                    .is_some_and(|e| e.dispatch_kind == SubrDispatchKind::SpecialForm)
                {
                    Err(signal(LispCondition::InvalidFunction, vec![invalid_fn]))
                } else {
                    result
                }
            }
            NamedCallTarget::Void => Err(signal(
                LispCondition::VoidFunction,
                vec![Value::symbol(name)],
            )),
        }
    }

    pub(super) fn apply_named_autoload_callable(
        &mut self,
        name: &str,
        autoload_form: Value,
        args: LispArgVec,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        self.apply_named_autoload_callable_by_id(
            intern(name),
            autoload_form,
            args,
            rewrite_builtin_wrong_arity,
        )
    }

    pub(super) fn apply_named_autoload_callable_by_id(
        &mut self,
        sym_id: SymId,
        autoload_form: Value,
        args: LispArgVec,
        _rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        // Startup wrappers often expose autoload-shaped function cells for names
        // backed by builtins. Keep the autoload shape while preserving callability.
        if lookup_global_subr_entry(sym_id).is_some() {
            let subr = Value::subr_from_sym_id(sym_id);
            // GNU-faithful pre-check via check_funcall_subr_arity.
            if let Some(flow) = self.check_funcall_subr_arity_value(subr, args.len()) {
                return Err(flow);
            }
            if let Some(result) = self.dispatch_subr_value_internal(
                subr,
                args.clone(),
                Value::subr_from_sym_id(sym_id),
            ) {
                return result;
            }
        }

        let mut current_autoload = autoload_form;
        let function = loop {
            match self.load_named_autoload_call_step(sym_id, current_autoload)? {
                NamedAutoloadCallStep::RetrySymbol { autoload_form } => {
                    // GNU `funcall_general` sets `fun = original_fun` and
                    // jumps to `retry`, preserving the symbol identity across
                    // any number of chained autoload declarations.
                    current_autoload = autoload_form;
                }
                NamedAutoloadCallStep::DispatchFunction { function } => break function,
                NamedAutoloadCallStep::Void => {
                    return Err(signal(
                        LispCondition::VoidFunction,
                        vec![Value::from_sym_id(sym_id)],
                    ));
                }
            }
        };

        let function_is_callable = self.function_value_is_callable(&function);
        match self.apply_untraced(function, args) {
            Err(Flow::Signal(sig))
                if !function_is_callable && sig.symbol == invalid_function_symbol() =>
            {
                Err(signal(
                    LispCondition::InvalidFunction,
                    vec![Value::from_sym_id(sym_id)],
                ))
            }
            other => other,
        }
    }

    pub(super) fn load_named_autoload_call_step(
        &mut self,
        sym_id: SymId,
        autoload_form: Value,
    ) -> Result<NamedAutoloadCallStep, Flow> {
        let loaded = super::super::autoload::builtin_autoload_do_load(
            self,
            vec![autoload_form, Value::from_sym_id(sym_id)],
        )?;

        Ok(if loaded.is_nil() {
            NamedAutoloadCallStep::Void
        } else if super::super::autoload::is_autoload_value(&loaded) {
            NamedAutoloadCallStep::RetrySymbol {
                autoload_form: loaded,
            }
        } else {
            NamedAutoloadCallStep::DispatchFunction { function: loaded }
        })
    }

    pub(super) fn apply_evaluator_callable_by_id(
        &mut self,
        sym_id: SymId,
        args: LispArgVec,
    ) -> EvalResult {
        match evaluator_handler(sym_id) {
            Some(EvaluatorHandler::Callable(CallableHandler::Throw)) => {
                if args.len() != 2 {
                    return Err(signal(
                        LispCondition::WrongNumberOfArguments,
                        vec![
                            Value::subr_from_sym_id(sym_id),
                            Value::fixnum(args.len() as i64),
                        ],
                    ));
                }
                let tag = args[0];
                let value = args[1];
                if self.has_active_catch(&tag) {
                    Err(Flow::throw(tag, value))
                } else {
                    Err(signal(LispCondition::NoCatch, vec![tag, value]))
                }
            }
            Some(EvaluatorHandler::SpecialForm(_)) | None => Err(signal(
                LispCondition::VoidFunction,
                vec![Value::from_sym_id(sym_id)],
            )),
        }
    }

    pub(super) fn apply_lambda(&mut self, func_value: Value, args: LispArgVec) -> EvalResult {
        let raw_cons_lambda = func_value.is_cons();
        let (arglist, body, env) = if raw_cons_lambda {
            let tail = func_value.cons_cdr();
            if !tail.is_cons() {
                return Err(signal(LispCondition::InvalidFunction, vec![func_value]));
            }
            (tail.cons_car(), tail.cons_cdr(), None)
        } else {
            let Some(arglist) = func_value.closure_slot(CLOSURE_ARGLIST) else {
                return Err(signal(LispCondition::InvalidFunction, vec![func_value]));
            };
            let Some(body) = func_value.closure_body_value() else {
                return Err(signal(LispCondition::InvalidFunction, vec![func_value]));
            };
            (arglist, body, func_value.closure_env().unwrap_or(None))
        };

        // Root the function value on the specpdl so GC can trace it
        // (keeping body, env, and params alive through the call).
        let root_count = self.specpdl.len();
        self.specpdl.push(SpecBinding::GcRoot { value: func_value });
        if raw_cons_lambda {
            let old_lexenv = std::mem::replace(&mut self.lexenv, Value::NIL);
            self.specpdl.push(SpecBinding::LexicalEnv { old_lexenv });
        }

        let call_state = match self.begin_lambda_call(func_value, arglist, env, &args) {
            Ok(state) => state,
            Err(err) => {
                return self.unbind_to_with_result(root_count, Err(err));
            }
        };
        let result = match self.eval_lambda_body_value(body) {
            Err(Flow::ThreadBlocked(blocked))
                if !blocked.remaining_forms.is_nil()
                    && crate::emacs_core::threads::thread_condition_case_continuation_parts(
                        blocked.remaining_forms,
                    )
                    .is_none() =>
            {
                match builtins::symbols::make_interpreted_closure_from_parts(
                    &Value::NIL,
                    &blocked.remaining_forms,
                    &self.lexenv,
                    None,
                    None,
                ) {
                    Ok(resume_function) => {
                        Err(Flow::thread_blocked(blocked.blocker, resume_function))
                    }
                    Err(flow) => Err(flow),
                }
            }
            other => other,
        };
        let result = self.finish_lambda_call(call_state, result);
        self.unbind_to_with_result(root_count, result)
    }

    #[inline]
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn bind_lexical_value_rooted(&mut self, sym: SymId, value: Value) {
        bind_lexical_value_rooted_in_specpdl(&mut self.lexenv, &mut self.specpdl, sym, value);
    }
}
