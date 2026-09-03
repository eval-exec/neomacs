//! Macro expansion: the expansion scope, its cache, and the per-macro perf counters.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;

impl Context {
    pub(crate) fn with_macro_expansion_scope(
        &mut self,
        f: impl FnOnce(&mut Self) -> EvalResult,
    ) -> EvalResult {
        self.macro_expansion_scope_depth += 1;
        let scope_enter_start = self.macro_perf_enabled.then(crate::host::time::Instant::now);
        let state = match self.begin_macro_expansion_scope_frame() {
            Ok(state) => state,
            Err(flow) => {
                self.macro_expansion_scope_depth =
                    self.macro_expansion_scope_depth.saturating_sub(1);
                return Err(flow);
            }
        };
        if let Some(start) = scope_enter_start {
            self.macro_perf_stats
                .scope_enter
                .note_duration(start.elapsed());
        }
        let result = f(self);
        let scope_exit_start = self.macro_perf_enabled.then(crate::host::time::Instant::now);
        let result = self.finish_macro_expansion_scope_frame(state, result);
        if let Some(start) = scope_exit_start {
            self.macro_perf_stats
                .scope_exit
                .note_duration(start.elapsed());
        }
        self.macro_expansion_scope_depth = self.macro_expansion_scope_depth.saturating_sub(1);
        result
    }

    pub(super) fn begin_macro_expansion_scope_frame(
        &mut self,
    ) -> Result<ActiveMacroExpansionScopeState, Flow> {
        let saved_specpdl_len = self.specpdl.len();
        let old_dynvars = self
            .obarray
            .symbol_value_id(macroexp_dynvars_symbol())
            .copied()
            .unwrap_or(Value::NIL);

        let dynvars_root_index = self.specpdl.len();
        self.specpdl
            .push(SpecBinding::GcRoot { value: old_dynvars });
        let mut dynvars = old_dynvars;
        // GNU eval.c walks Vinternal_interpreter_environment directly and only
        // extends `macroexp--dynvars` for bare symbols. Dynamic specpdl
        // bindings are not part of this macro-expansion state.
        let mut cursor = self.lexenv;
        while cursor.is_cons() {
            let entry = cursor.cons_car();
            if let Some(sym) = entry.as_symbol_id() {
                dynvars = Value::cons(Value::from_sym_id(sym), dynvars);
                match self.specpdl.get_mut(dynvars_root_index) {
                    Some(SpecBinding::GcRoot { value }) => *value = dynvars,
                    other => panic!("expected macro-expansion dynvars gc root, got {other:?}"),
                }
            }
            cursor = cursor.cons_cdr();
        }

        // GNU eval.c specbinds `lexical-binding' during ordinary macro calls
        // so the macro can know whether its expansion will be interpreted
        // lexically.  This must be a real specpdl binding: `lexical-binding'
        // is LOCALIZED, so writing the raw symbol default can leak across
        // buffers and diverges from GNU's SPECPDL_LET_LOCAL/DEFAULT behavior.
        self.try_specbind_or_unwind_to(
            saved_specpdl_len,
            lexical_binding_symbol(),
            Value::bool_val(!self.lexenv.is_nil()),
        )?;
        if !crate::emacs_core::value::eq_value(&dynvars, &old_dynvars) {
            self.try_specbind_or_unwind_to(saved_specpdl_len, macroexp_dynvars_symbol(), dynvars)?;
        }

        Ok(ActiveMacroExpansionScopeState { saved_specpdl_len })
    }

    pub(super) fn finish_macro_expansion_scope_frame(
        &mut self,
        state: ActiveMacroExpansionScopeState,
        result: EvalResult,
    ) -> EvalResult {
        self.unbind_to_with_result(state.saved_specpdl_len, result)
    }

    #[inline]
    pub(crate) fn macro_expansion_mutation_epoch(&self) -> u64 {
        self.macro_expansion_mutation_epoch
    }

    #[inline]
    pub(crate) fn note_macro_expansion_mutation(&mut self) {
        if self.macro_expansion_scope_depth > 0 {
            self.macro_expansion_mutation_epoch =
                self.macro_expansion_mutation_epoch.wrapping_add(1);
        }
    }

    pub(crate) fn note_runtime_macro_expansion(
        &mut self,
        form: Value,
        expand_elapsed: std::time::Duration,
    ) {
        self.macro_expand_calls = self.macro_expand_calls.saturating_add(1);
        self.macro_expand_total_us = self
            .macro_expand_total_us
            .saturating_add(expand_elapsed.as_micros() as u64);
        if self.macro_perf_enabled && expand_elapsed.as_millis() > 50 {
            let macro_head = if form.is_cons() {
                form.cons_car().as_symbol_name().unwrap_or("<non-symbol>")
            } else {
                "<atom>"
            };
            let form_str = crate::emacs_core::print::print_value(&form);
            let form_preview: String = form_str.chars().take(200).collect();
            tracing::warn!(
                "runtime macro expansion head={macro_head} took {expand_elapsed:.2?} form={form_preview}"
            );
        }
    }

    pub(super) fn apply_macro_callable_for_macroexpand(
        &mut self,
        callable: Value,
        args: Vec<Value>,
    ) -> Result<Value, Flow> {
        let perf_start = self.macro_perf_enabled.then(crate::host::time::Instant::now);
        // GNU Fmacroexpand applies the macro expander directly.  The
        // eval.c macro-call path specbinds `lexical-binding`, but the
        // Fmacroexpand path does not; bytecomp relies on the current
        // buffer's visible `lexical-binding` while macroexpanding source.
        let result = self.apply(callable, args);
        if let Some(start) = perf_start {
            self.macro_perf_stats
                .macro_apply
                .note_duration(start.elapsed());
        }
        result
    }

    pub(crate) fn expand_macro_for_macroexpand(
        &mut self,
        form: Value,
        definition: Value,
        args: Vec<Value>,
        environment: Option<Value>,
    ) -> Result<Value, Flow> {
        let perf_start = self.macro_perf_enabled.then(crate::host::time::Instant::now);
        let expand_start = crate::host::time::Instant::now();
        let specpdl_root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(form);
        self.push_specpdl_root(definition);
        if let Some(environment) = environment {
            self.push_specpdl_root(environment);
        }

        let result = (|| {
            let expanded = if definition.is_macro() {
                self.apply_macro_callable_for_macroexpand(definition, args)?
            } else if cons_head_symbol_id(&definition) == Some(macro_symbol()) {
                self.apply_macro_callable_for_macroexpand(definition.cons_cdr(), args)?
            } else if self.function_value_is_callable(&definition) {
                // GNU `macroexpand` ENVIRONMENT entries store the macro
                // expander itself, not the full `(macro . fn)` function cell.
                self.apply_macro_callable_for_macroexpand(definition, args)?
            } else {
                return Err(signal(LispCondition::InvalidFunction, vec![definition]));
            };

            self.note_runtime_macro_expansion(form, expand_start.elapsed());
            Ok(expanded)
        })();
        self.restore_specpdl_roots(specpdl_root_scope);
        if let Some(start) = perf_start {
            self.macro_perf_stats
                .expand_macro
                .note_duration(start.elapsed());
        }
        result
    }

    pub(crate) fn note_eager_macro_perf_step1(&mut self, duration: std::time::Duration) {
        if self.macro_perf_enabled {
            self.macro_perf_stats.eager_step1.note_duration(duration);
        }
    }

    pub(crate) fn note_eager_macro_perf_step3(&mut self, duration: std::time::Duration) {
        if self.macro_perf_enabled {
            self.macro_perf_stats.eager_step3.note_duration(duration);
        }
    }

    pub(crate) fn note_eager_macro_perf_step4(&mut self, duration: std::time::Duration) {
        if self.macro_perf_enabled {
            self.macro_perf_stats.eager_step4.note_duration(duration);
        }
    }

    pub(crate) fn macro_perf_summary(&self) -> Option<String> {
        if !self.macro_perf_enabled {
            return None;
        }

        let mut parts = vec![format!(
            "expansions:{} expand-total:{:.2}ms",
            self.macro_expand_calls,
            self.macro_expand_total_us as f64 / 1000.0
        )];

        for counter in [
            self.macro_perf_stats.scope_enter.summary("scope-enter"),
            self.macro_perf_stats.scope_exit.summary("scope-exit"),
            self.macro_perf_stats.macro_apply.summary("macro-apply"),
            self.macro_perf_stats.expand_macro.summary("expand-macro"),
            self.macro_perf_stats.eager_step1.summary("eager-step1"),
            self.macro_perf_stats.eager_step3.summary("eager-step3"),
            self.macro_perf_stats.eager_step4.summary("eager-step4"),
        ]
        .into_iter()
        .flatten()
        {
            parts.push(counter);
        }

        Some(parts.join(" | "))
    }

    #[inline]
    pub(crate) fn macro_perf_enabled(&self) -> bool {
        self.macro_perf_enabled
    }
}
