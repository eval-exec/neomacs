//! Special forms: the evaluator's handling of quote, if, cond, let, while, condition-case, catch, unwind-protect and the rest of GNU eval.c's DEFUN special forms.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;

impl Context {
    pub(super) fn try_special_form_value_id(
        &mut self,
        sym_id: SymId,
        tail: Value,
    ) -> Option<EvalResult> {
        self.try_special_form_with_surface(sym_id, sym_id, tail)
    }

    pub(super) fn try_aliased_special_form_value_id(
        &mut self,
        surface_id: SymId,
        target_id: SymId,
        tail: Value,
    ) -> Option<EvalResult> {
        self.try_special_form_with_surface(surface_id, target_id, tail)
    }

    /// The single special-form dispatch table. `target_id` selects the form
    /// (already resolved through any defalias chain); `surface_id` is the
    /// symbol the source form actually named, threaded into the forms whose
    /// errors report a call name — GNU signals with the surface symbol
    /// object itself, so an uninterned alias keeps its identity. The direct
    /// (non-aliased) path passes the same id for both.
    pub(super) fn try_special_form_with_surface(
        &mut self,
        surface_id: SymId,
        target_id: SymId,
        tail: Value,
    ) -> Option<EvalResult> {
        let saved_depth = self.depth;
        let result = match evaluator_handler(target_id) {
            Some(EvaluatorHandler::SpecialForm(handler)) => Some(match handler {
                // Forms that report the surface name in their signals.
                SpecialFormHandler::Quote => self.sf_quote_value_named(surface_id, tail),
                SpecialFormHandler::Function => self.sf_function_value_named(surface_id, tail),
                SpecialFormHandler::Let => self.sf_let_value_named(surface_id, tail),
                SpecialFormHandler::LetStar => self.sf_let_star_value_named(surface_id, tail),
                SpecialFormHandler::Setq => self.sf_setq_value_named(surface_id, tail),
                SpecialFormHandler::If => self.sf_if_value_named(surface_id, tail),
                SpecialFormHandler::While => self.sf_while_value_named(surface_id, tail),
                SpecialFormHandler::Prog1 => self.sf_prog1_value_named(surface_id, tail),
                SpecialFormHandler::Defvar => self.sf_defvar_value_named(surface_id, tail),
                SpecialFormHandler::Defconst => self.sf_defconst_value_named(surface_id, tail),
                SpecialFormHandler::Catch => self.sf_catch_value_named(surface_id, tail),
                SpecialFormHandler::UnwindProtect => {
                    self.sf_unwind_protect_value_named(surface_id, tail)
                }
                SpecialFormHandler::ConditionCase => {
                    self.sf_condition_case_value_named(surface_id, tail)
                }
                // Forms whose signals never carry a call name.
                SpecialFormHandler::And => self.sf_and_value(tail),
                SpecialFormHandler::Or => self.sf_or_value(tail),
                SpecialFormHandler::Cond => self.sf_cond_value(tail),
                SpecialFormHandler::Progn => self.sf_progn_value(tail),
                SpecialFormHandler::SaveExcursion => self.sf_save_excursion_value(tail),
                SpecialFormHandler::SaveCurrentBuffer => self.sf_save_current_buffer_value(tail),
                SpecialFormHandler::SaveRestriction => self.sf_save_restriction_value(tail),
                SpecialFormHandler::Interactive => Ok(Value::NIL),
            }),
            Some(EvaluatorHandler::Callable(_)) => None,
            None => match target_id {
                // These evaluator-internal forms have no public subr
                // declaration and are reachable only by their canonical name.
                id if id == lambda_symbol() && surface_id == target_id => {
                    Some(self.sf_lambda_value(tail))
                }
                id if id == byte_code_literal_symbol() && surface_id == target_id => {
                    Some(self.sf_byte_code_literal_value(tail))
                }
                id if id == byte_code_symbol() && surface_id == target_id => {
                    Some(self.sf_byte_code_value(tail))
                }
                _ => None,
            },
        };
        self.depth = saved_depth;
        result
    }

    pub(super) fn listp_error(&self, value: Value) -> Flow {
        // GNU `CHECK_LIST` walks the cdr chain until it finds the
        // non-cons tail and signals
        // `(wrong-type-argument listp TAIL)` with the offending
        // tail element, not the whole input. Verified against
        // emacs 31.0.50 via:
        //   (condition-case e (length '(1 . 2)) (error e))
        //     -> (wrong-type-argument listp 2)
        //   (condition-case e (let ((x 1) . 2) x) (error e))
        //     -> (wrong-type-argument listp 2)
        let mut tail = value;
        while tail.is_cons() {
            tail = tail.cons_cdr();
        }
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), tail],
        )
    }

    pub(super) fn value_list_len_or_error(&self, list: Value) -> Result<usize, Flow> {
        list_length(&list).ok_or_else(|| self.listp_error(list))
    }

    pub(super) fn one_unevalled_arg(&self, name: SymId, tail: Value) -> Result<Value, Flow> {
        let mut cursor = tail;
        if !cursor.is_cons() {
            return if cursor.is_nil() {
                Err(signal(
                    LispCondition::WrongNumberOfArguments,
                    vec![Value::from_sym_id(name), Value::fixnum(0)],
                ))
            } else {
                Err(self.listp_error(tail))
            };
        }
        let arg = cursor.cons_car();
        cursor = cursor.cons_cdr();
        if !cursor.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![
                    Value::from_sym_id(name),
                    Value::fixnum(self.value_list_len_or_error(tail)? as i64),
                ],
            ));
        }
        Ok(arg)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_quote_value(&mut self, tail: Value) -> EvalResult {
        self.sf_quote_value_named(quote_symbol(), tail)
    }

    pub(super) fn sf_quote_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        self.one_unevalled_arg(call_name, tail)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_function_value(&mut self, tail: Value) -> EvalResult {
        self.sf_function_value_named(function_symbol(), tail)
    }

    pub(super) fn sf_function_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        let arg = self.one_unevalled_arg(call_name, tail)?;
        if cons_head_symbol_id(&arg) == Some(lambda_symbol()) {
            return self.instantiate_callable_cons_form(arg);
        }
        Ok(arg)
    }

    pub(super) fn sf_lambda_value(&mut self, tail: Value) -> EvalResult {
        self.instantiate_callable_cons_form(Value::cons(Value::from_sym_id(lambda_symbol()), tail))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_let_value(&mut self, tail: Value) -> EvalResult {
        self.sf_let_value_named(let_symbol(), tail)
    }

    pub(super) fn sf_let_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }

        let varlist = tail.cons_car();
        let body = tail.cons_cdr();
        let mut lexical_bindings = LetBindingVec::new();
        let mut dynamic_sym_ids = LetBindingVec::new();
        let use_lexical = self.lexical_binding();
        let mut constant_binding_error: Option<String> = None;
        let specpdl_root_scope = self.save_specpdl_roots();
        let mut bindings = varlist;

        while bindings.is_cons() {
            let binding = self.unwrap_symbol(bindings.cons_car());
            bindings = bindings.cons_cdr();
            if let Some(id) = binding.as_symbol_id() {
                // A bare binder binds nil, which is never a keyword's own value.
                if let Some(name) = let_constant_error_name(&self.obarray, id, Value::NIL) {
                    if constant_binding_error.is_none() {
                        constant_binding_error = Some(name);
                    }
                    continue;
                }
                if use_lexical
                    && !self.obarray.is_special_id(id)
                    && !self.lexenv_declares_special_cached_in(self.lexenv, id)
                {
                    lexical_bindings.push((id, Value::NIL));
                } else {
                    dynamic_sym_ids.push((id, Value::NIL));
                }
                continue;
            }
            if !binding.is_cons() {
                self.restore_specpdl_roots(specpdl_root_scope);
                // GNU takes `(car elt)` of a non-symbol binding, so a non-list
                // element signals `(wrong-type-argument listp ELT)`.
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), binding],
                ));
            }
            let head = self.unwrap_symbol(binding.cons_car());
            let Some(id) = head.as_symbol_id() else {
                self.restore_specpdl_roots(specpdl_root_scope);
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("symbolp"), head],
                ));
            };
            let mut value_tail = binding.cons_cdr();
            let value = if value_tail.is_nil() {
                Value::NIL
            } else if value_tail.is_cons() {
                let init_form = value_tail.cons_car();
                value_tail = value_tail.cons_cdr();
                if !value_tail.is_nil() {
                    self.restore_specpdl_roots(specpdl_root_scope);
                    return Err(signal(
                        "error",
                        vec![
                            Value::string("`let' bindings can have only one value-form"),
                            binding,
                        ],
                    ));
                }
                match self.eval_sub(init_form) {
                    Ok(value) => value,
                    Err(err) => {
                        self.restore_specpdl_roots(specpdl_root_scope);
                        return Err(err);
                    }
                }
            } else {
                self.restore_specpdl_roots(specpdl_root_scope);
                return Err(self.listp_error(binding));
            };
            self.push_specpdl_root(value);
            if let Some(name) = let_constant_error_name(&self.obarray, id, value) {
                if constant_binding_error.is_none() {
                    constant_binding_error = Some(name);
                }
                continue;
            }
            if use_lexical
                && !self.obarray.is_special_id(id)
                && !self.lexenv_declares_special_cached_in(self.lexenv, id)
            {
                lexical_bindings.push((id, value));
            } else {
                dynamic_sym_ids.push((id, value));
            }
        }
        if !bindings.is_nil() {
            self.restore_specpdl_roots(specpdl_root_scope);
            return Err(self.listp_error(varlist));
        }
        if let Some(name) = constant_binding_error {
            self.restore_specpdl_roots(specpdl_root_scope);
            return Err(signal(
                LispCondition::SettingConstant,
                vec![Value::symbol(name)],
            ));
        }

        // CRITICAL: Restore specpdl roots (drop init-form GcRoot entries) BEFORE
        // pushing LexicalEnv/Let entries. Otherwise `restore_specpdl_roots`
        // drains from `saved_len` and re-extends with non-GcRoot entries,
        // MOVING our LexicalEnv to a lower index. Then `unbind_to(specpdl_count)`
        // becomes a no-op because specpdl.len() already matches, and the stale
        // LexicalEnv leaks below. This caused lexical binding leaks — closures
        // created in the body captured oversized environments.
        self.restore_specpdl_roots(specpdl_root_scope);

        // Save lexenv AFTER init forms run (matches GNU eval.c:1167:
        //   `lexenv = Vinternal_interpreter_environment;`).
        // Capture specpdl_count AFTER restoring so LexicalEnv sits exactly at
        // specpdl[specpdl_count] and unbind_to will pop it.
        let lexenv_at_entry = self.lexenv;
        let specpdl_count = self.specpdl.len();

        // Always save the entry-point lexenv on the specpdl when in lexical
        // mode, so unbind_to restores it regardless of what the body does.
        // Matches GNU's specbind(Qinternal_interpreter_environment).
        if use_lexical {
            self.specpdl.push(SpecBinding::LexicalEnv {
                old_lexenv: lexenv_at_entry,
            });
        }

        // Build new lexenv locally by consing bindings onto the ENTRY-POINT
        // lexenv (not self.lexenv which may have been modified by init forms).
        // Matches GNU eval.c:1167-1186.
        let mut new_lexenv = lexenv_at_entry;
        for (sym_id, val) in &lexical_bindings {
            let binding_pair = Value::make_cons(
                crate::emacs_core::eval::lexenv_binding_symbol_value(*sym_id),
                *val,
            );
            self.specpdl.push(SpecBinding::GcRoot {
                value: binding_pair,
            });
            new_lexenv = Value::make_cons(binding_pair, new_lexenv);
            match self.specpdl.last_mut() {
                Some(SpecBinding::GcRoot { value }) => *value = new_lexenv,
                _ => unreachable!(),
            }
        }
        // Install the new lexenv atomically.
        self.lexenv = new_lexenv;

        let temp_scope = self.save_eval_temp_roots();
        for (_, value) in lexical_bindings.iter().chain(dynamic_sym_ids.iter()) {
            self.push_eval_temp_root(*value);
        }
        for (sym_id, value) in &dynamic_sym_ids {
            if let Err(flow) = self.try_specbind(*sym_id, *value) {
                let result = self.unbind_to_with_result(specpdl_count, Err(flow));
                self.restore_eval_temp_roots_to_sequence(temp_scope);
                return result;
            }
        }

        let result = self.sf_progn_value(body);
        let result = self.unbind_to_with_result(specpdl_count, result);
        self.restore_eval_temp_roots_to_sequence(temp_scope);
        result
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_let_star_value(&mut self, tail: Value) -> EvalResult {
        self.sf_let_star_value_named(let_star_symbol(), tail)
    }

    pub(super) fn sf_let_star_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }

        let varlist = tail.cons_car();
        let body = tail.cons_cdr();
        let use_lexical = self.lexical_binding();
        let specpdl_count = self.specpdl.len();
        // Mirrors GNU Flet_star: specbind(Qinternal_interpreter_environment, lexenv)
        // before any per-variable specbinds. unbind_to pops everything.
        if use_lexical {
            self.specpdl.push(SpecBinding::LexicalEnv {
                old_lexenv: self.lexenv,
            });
        }

        let temp_scope = self.save_eval_temp_roots();
        let val_temp_slot = self.push_eval_temp_root_slot(Value::NIL);
        let init_result: Result<(), Flow> = (|| {
            let mut bindings = varlist;
            while bindings.is_cons() {
                let binding = self.unwrap_symbol(bindings.cons_car());
                bindings = bindings.cons_cdr();
                let (id, value) = if let Some(id) = binding.as_symbol_id() {
                    (id, Value::NIL)
                } else if binding.is_cons() {
                    let head = self.unwrap_symbol(binding.cons_car());
                    let Some(id) = head.as_symbol_id() else {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("symbolp"), head],
                        ));
                    };
                    let mut value_tail = binding.cons_cdr();
                    let value = if value_tail.is_nil() {
                        Value::NIL
                    } else if value_tail.is_cons() {
                        let init_form = value_tail.cons_car();
                        value_tail = value_tail.cons_cdr();
                        if !value_tail.is_nil() {
                            return Err(signal(
                                "error",
                                vec![
                                    Value::string("`let' bindings can have only one value-form"),
                                    binding,
                                ],
                            ));
                        }
                        self.eval_sub(init_form)?
                    } else {
                        return Err(self.listp_error(binding));
                    };
                    (id, value)
                } else {
                    // GNU takes `(car elt)`, so a non-list element signals
                    // `(wrong-type-argument listp ELT)`.
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("listp"), binding],
                    ));
                };
                self.set_eval_temp_root_slot(val_temp_slot, value);

                if let Some(name) = let_constant_error_name(&self.obarray, id, value) {
                    return Err(signal(
                        LispCondition::SettingConstant,
                        vec![Value::symbol(&name)],
                    ));
                }
                if use_lexical
                    && !self.obarray.is_special_id(id)
                    && !self.lexenv_declares_special_cached_in(self.lexenv, id)
                {
                    // Matches GNU Flet_star (eval.c:1113-1120):
                    // Direct cons onto Vinternal_interpreter_environment.
                    // The LexicalEnv entry at specpdl_count saves the pre-let*
                    // state; unbind_to restores it.
                    let binding = Value::make_cons(lexenv_binding_symbol_value(id), value);
                    self.lexenv = Value::make_cons(binding, self.lexenv);
                } else {
                    self.try_specbind(id, value)?;
                }
            }
            if !bindings.is_nil() {
                return Err(self.listp_error(varlist));
            }
            Ok(())
        })();
        if let Err(error) = init_result {
            let result = self.unbind_to_with_result(specpdl_count, Err(error));
            self.restore_eval_temp_roots_to_sequence(temp_scope);
            return result;
        }

        let result = self.sf_progn_value(body);
        let result = self.unbind_to_with_result(specpdl_count, result);
        self.restore_eval_temp_roots_to_sequence(temp_scope);
        result
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_setq_value(&mut self, tail: Value) -> EvalResult {
        self.sf_setq_value_named(setq_symbol(), tail)
    }

    pub(super) fn sf_setq_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Ok(Value::NIL);
        }
        let mut cursor = tail;
        let mut last = Value::NIL;
        let mut nargs: usize = 0;
        while cursor.is_cons() {
            let symbol = cursor.cons_car();
            cursor = cursor.cons_cdr();
            nargs += 1;
            if cursor.is_nil() {
                return Err(signal(
                    LispCondition::WrongNumberOfArguments,
                    vec![Value::from_sym_id(call_name), Value::fixnum(nargs as i64)],
                ));
            }
            if !cursor.is_cons() {
                return Err(self.listp_error(tail));
            }
            let value_form = cursor.cons_car();
            cursor = cursor.cons_cdr();
            nargs += 1;
            let symbol = self.unwrap_symbol(symbol);
            let Some(sym_id) = symbol.as_symbol_id() else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("symbolp"), symbol],
                ));
            };
            let value = self.eval_sub(value_form)?;
            // Debug probe for multibyte assignments to default-directory.
            // Kept at debug level so it doesn't pollute normal error
            // output (Doom always fires this with pure-ASCII paths that
            // happen to carry the multibyte flag from string decoding).
            if sym_id == default_directory_symbol()
                && value.is_string()
                && value.string_is_multibyte()
            {
                tracing::debug!(
                    "SETQ default-directory to MULTIBYTE string: {:?}",
                    value
                        .as_lisp_string()
                        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
                        .unwrap_or_default(),
                );
            }
            self.assign_setq_by_id(sym_id, value)?;
            last = value;
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(last)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_if_value(&mut self, tail: Value) -> EvalResult {
        self.sf_if_value_named(if_symbol(), tail)
    }

    pub(super) fn sf_if_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }
        let cond_form = tail.cons_car();
        let mut rest = tail.cons_cdr();
        if rest.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(1)],
            ));
        }
        if !rest.is_cons() {
            return Err(self.listp_error(tail));
        }
        let then_form = rest.cons_car();
        rest = rest.cons_cdr();
        if self.eval_sub(cond_form)?.is_truthy() {
            self.eval_sub(then_form)
        } else {
            self.sf_progn_value(rest)
        }
    }

    pub(super) fn sf_and_value(&mut self, tail: Value) -> EvalResult {
        let mut cursor = tail;
        let mut last = Value::T;
        while cursor.is_cons() {
            last = self.eval_sub(cursor.cons_car())?;
            if last.is_nil() {
                return Ok(Value::NIL);
            }
            cursor = cursor.cons_cdr();
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(last)
    }

    pub(super) fn sf_or_value(&mut self, tail: Value) -> EvalResult {
        let mut cursor = tail;
        while cursor.is_cons() {
            let value = self.eval_sub(cursor.cons_car())?;
            if value.is_truthy() {
                return Ok(value);
            }
            cursor = cursor.cons_cdr();
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(Value::NIL)
    }

    pub(super) fn sf_cond_value(&mut self, tail: Value) -> EvalResult {
        let mut clauses = tail;
        while clauses.is_cons() {
            let clause = clauses.cons_car();
            clauses = clauses.cons_cdr();
            if clause.is_nil() {
                continue;
            }
            if !clause.is_cons() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), clause],
                ));
            }
            let test = clause.cons_car();
            let body = clause.cons_cdr();
            let test_value = self.eval_sub(test)?;
            if test_value.is_truthy() {
                if body.is_nil() {
                    return Ok(test_value);
                }
                return self.sf_progn_value(body);
            }
        }
        if !clauses.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(Value::NIL)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_while_value(&mut self, tail: Value) -> EvalResult {
        self.sf_while_value_named(while_symbol(), tail)
    }

    pub(super) fn sf_while_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }
        let test_form = tail.cons_car();
        let body = tail.cons_cdr();
        let mut iters: u64 = 0;
        loop {
            if self.eval_sub(test_form)?.is_nil() {
                return Ok(Value::NIL);
            }
            self.sf_progn_value(body)?;
            iters += 1;
            if iters == 1_000_000 {
                let cond_str = super::super::print::print_value(&test_form);
                tracing::warn!(
                    "while loop exceeded 1M iterations, cond: {}",
                    &cond_str[..cond_str.len().min(300)]
                );
            }
            self.maybe_quit()?;
        }
    }

    pub(super) fn sf_progn_value(&mut self, forms: Value) -> EvalResult {
        let temp_scope = self.save_sequence_temp_roots();
        let result = (|| {
            let mut cursor = forms;
            let mut last = Value::NIL;
            while cursor.is_cons() {
                match self.eval_sub(cursor.cons_car()) {
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
            if !cursor.is_nil() {
                return Err(self.listp_error(forms));
            }
            Ok(last)
        })();
        self.restore_sequence_temp_roots(temp_scope);
        result
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_prog1_value(&mut self, tail: Value) -> EvalResult {
        self.sf_prog1_value_named(prog1_symbol(), tail)
    }

    pub(super) fn sf_prog1_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }
        let first_form = tail.cons_car();
        let rest = tail.cons_cdr();
        let first = self.eval_sub(first_form)?;
        let specpdl_root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(first);
        let result = self.sf_progn_value(rest);
        self.restore_specpdl_roots(specpdl_root_scope);
        result?;
        Ok(first)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_defvar_value(&mut self, tail: Value) -> EvalResult {
        self.sf_defvar_value_named(defvar_symbol(), tail)
    }

    pub(super) fn sf_defvar_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }

        let symbol = self.unwrap_symbol(tail.cons_car());
        let Some(sym_id) = symbol.as_symbol_id() else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), symbol],
            ));
        };
        let mut rest = tail.cons_cdr();

        if rest.is_nil() {
            if self.lexical_binding()
                && !self.lexenv.is_nil()
                && !self.obarray.is_special_id(sym_id)
            {
                self.lexenv = Value::cons(Value::from_sym_id(sym_id), self.lexenv);
            }
            return Ok(Value::from_sym_id(sym_id));
        }
        if !rest.is_cons() {
            return Err(self.listp_error(tail));
        }
        let init_form = rest.cons_car();
        rest = rest.cons_cdr();
        let documentation = if rest.is_nil() {
            Value::NIL
        } else if rest.is_cons() {
            let doc = rest.cons_car();
            rest = rest.cons_cdr();
            if !rest.is_nil() {
                return Err(signal("error", vec![Value::string("Too many arguments")]));
            }
            doc
        } else {
            return Err(self.listp_error(tail));
        };

        let mut define_args = vec![symbol];
        if !documentation.is_nil() {
            define_args.push(documentation);
        }
        super::super::builtins::symbols::builtin_internal_define_uninitialized_variable(
            self,
            define_args,
        )?;

        let was_bound = default_toplevel_value_in_state(
            &self.obarray,
            self.specpdl.as_slice(),
            Some(&self.buffers.buffer_defaults),
            sym_id,
        )
        .is_some()
            || self.obarray.is_constant_id(sym_id);
        if !was_bound {
            let value = self.eval_sub(init_form)?;
            set_default_toplevel_value(self, vec![symbol, value])?;
        }

        Ok(Value::from_sym_id(sym_id))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_defconst_value(&mut self, tail: Value) -> EvalResult {
        self.sf_defconst_value_named(defconst_symbol(), tail)
    }

    pub(super) fn sf_defconst_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }
        let symbol = self.unwrap_symbol(tail.cons_car());
        let Some(sym_id) = symbol.as_symbol_id() else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), symbol],
            ));
        };
        let mut rest = tail.cons_cdr();
        if rest.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(1)],
            ));
        }
        if !rest.is_cons() {
            return Err(self.listp_error(tail));
        }
        let init_form = rest.cons_car();
        rest = rest.cons_cdr();
        let documentation = if rest.is_nil() {
            Value::NIL
        } else if rest.is_cons() {
            let doc = rest.cons_car();
            rest = rest.cons_cdr();
            if !rest.is_nil() {
                return Err(signal("error", vec![Value::string("Too many arguments")]));
            }
            doc
        } else {
            return Err(self.listp_error(tail));
        };

        let mut define_args = vec![symbol];
        if !documentation.is_nil() {
            define_args.push(documentation);
        }
        super::super::builtins::symbols::builtin_internal_define_uninitialized_variable(
            self,
            define_args,
        )?;

        let value = self.eval_sub(init_form)?;
        super::super::data::set_default(self, vec![symbol, value])?;
        self.obarray.make_special_id(sym_id);
        self.obarray
            .put_property_id(sym_id, intern("risky-local-variable"), Value::T)?;
        Ok(Value::from_sym_id(sym_id))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_catch_value(&mut self, tail: Value) -> EvalResult {
        self.sf_catch_value_named(catch_symbol(), tail)
    }

    pub(super) fn sf_catch_value_named(&mut self, call_name: SymId, tail: Value) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }
        let tag = self.eval_sub(tail.cons_car())?;
        self.push_condition_frame(ConditionFrame::Catch {
            tag,
            resume: ResumeTarget::InterpreterCatch,
        });
        let specpdl_count = self.specpdl.len();
        let result = match self.sf_progn_value(tail.cons_cdr()) {
            Ok(value) => Ok(value),
            Err(Flow::Signal(sig)) => match self.dispatch_signal_if_needed(sig) {
                Ok(dispatched) => Err(Flow::Signal(dispatched)),
                Err(Flow::Throw(thrown)) if eq_value(&tag, &thrown.tag) => Ok(thrown.value),
                Err(flow) => Err(flow),
            },
            Err(Flow::Throw(thrown)) if eq_value(&tag, &thrown.tag) => Ok(thrown.value),
            Err(flow) => Err(flow),
        };
        self.pop_condition_frame();
        // Catching moves the value out of pinned ThrowData. Carry it through
        // all cleanup so it stays rooted and a cleanup nonlocal exit can
        // replace it, matching GNU's `unbind_to (count, value)`.
        self.unbind_to_with_result(specpdl_count, result)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_unwind_protect_value(&mut self, tail: Value) -> EvalResult {
        self.sf_unwind_protect_value_named(unwind_protect_symbol(), tail)
    }

    pub(super) fn sf_unwind_protect_value_named(
        &mut self,
        call_name: SymId,
        tail: Value,
    ) -> EvalResult {
        // GNU eval.c:1461 declares `unwind-protect` with min_args=1.
        // The generic arity check in GNU `eval_sub` (eval.c:2612) runs
        // for every SUBRP including UNEVALLED. Neomacs skips that check
        // for special forms (dispatch_kind != SpecialForm at
        // eval.rs:6599) so each special form validates itself -- see
        // `sf_condition_case_value_named`.
        let nargs = self.value_list_len_or_error(tail)?;
        if nargs < 1 {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(nargs as i64)],
            ));
        }
        let body = tail.cons_car();
        let cleanup_forms = tail.cons_cdr();
        let specpdl_count = self.specpdl.len();
        self.specpdl.push(SpecBinding::UnwindProtect {
            forms: cleanup_forms,
            lexenv: self.lexenv,
        });
        let result = self.eval_sub(body);
        self.unbind_to_with_result(specpdl_count, result)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn sf_condition_case_value(&mut self, tail: Value) -> EvalResult {
        self.sf_condition_case_value_named(condition_case_symbol(), tail)
    }

    pub(super) fn sf_condition_case_value_named(
        &mut self,
        call_name: SymId,
        tail: Value,
    ) -> EvalResult {
        let nargs = self.value_list_len_or_error(tail)?;
        if nargs < 2 {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(nargs as i64)],
            ));
        }
        let var = self.unwrap_symbol(tail.cons_car());
        let Some(var_id) = var.as_symbol_id() else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), var],
            ));
        };
        let rest = tail.cons_cdr();
        if !rest.is_cons() {
            return Err(self.listp_error(tail));
        }
        let body = rest.cons_car();
        let handlers = rest.cons_cdr();

        let mut handlers_vec = Vec::new();
        let mut success_handler_idx: Option<usize> = None;
        let mut cursor = handlers;
        while cursor.is_cons() {
            let handler = cursor.cons_car();
            let handler_index = handlers_vec.len();
            handlers_vec.push(handler);
            cursor = cursor.cons_cdr();
            if handler.is_nil() {
                continue;
            }
            if !handler.is_cons() {
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Invalid condition handler: {}",
                        super::super::print::print_value(&handler)
                    ))],
                ));
            }
            let head = handler.cons_car();
            if !(head.is_symbol() || head.is_symbol_with_pos() || head.is_cons()) {
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Invalid condition handler: {}",
                        super::super::print::print_value(&handler)
                    ))],
                ));
            }
            let head_unwrapped = self.unwrap_symbol(head);
            if head_unwrapped.is_symbol_named(":success") {
                success_handler_idx = Some(handler_index);
            }
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(handlers));
        }

        self.run_condition_case_body(var, var_id, &handlers_vec, success_handler_idx, |ctx| {
            ctx.eval_sub(body)
        })
    }

    pub(super) fn run_condition_case_body(
        &mut self,
        var: Value,
        var_id: SymId,
        handlers_vec: &[Value],
        success_handler_idx: Option<usize>,
        eval_body: impl FnOnce(&mut Self) -> EvalResult,
    ) -> EvalResult {
        let condition_stack_base = self.condition_stack_len();
        for (idx, handler) in handlers_vec.iter().enumerate().rev() {
            if success_handler_idx == Some(idx) || handler.is_nil() {
                continue;
            }
            if !handler.is_cons() {
                continue;
            }
            let conditions = handler.cons_car();
            self.push_condition_frame(ConditionFrame::ConditionCase {
                conditions,
                resume: ResumeTarget::InterpreterConditionCase {
                    handler_index: idx,
                    condition_stack_base,
                },
            });
        }

        match eval_body(self) {
            Ok(value) => {
                self.truncate_condition_stack(condition_stack_base);
                if let Some(idx) = success_handler_idx {
                    let handler = handlers_vec[idx];
                    let bind_var = !var.is_nil();
                    // Mirror the error-handler arm: bind VAR lexically when
                    // lexical binding is in effect and VAR is not special, else
                    // dynamically (GNU condition-case binds the :success var the
                    // same way it binds an error handler's var).
                    let use_lexical_binding = bind_var
                        && self.lexical_binding()
                        && !is_runtime_dynamically_special(&self.obarray, var_id)
                        && !self.lexenv_declares_special_cached_in(self.lexenv, var_id);
                    let specpdl_count = self.specpdl.len();
                    if use_lexical_binding {
                        self.specpdl.push(SpecBinding::LexicalEnv {
                            old_lexenv: self.lexenv,
                        });
                        let binding = Value::make_cons(lexenv_binding_symbol_value(var_id), value);
                        self.lexenv = Value::make_cons(binding, self.lexenv);
                    } else if bind_var {
                        if let Err(flow) = self.try_specbind(var_id, value) {
                            return self.unbind_to_with_result(specpdl_count, Err(flow));
                        }
                    }
                    let result = self.sf_progn_value(handler.cons_cdr());
                    return self.unbind_to_with_result(specpdl_count, result);
                }
                Ok(value)
            }
            Err(Flow::Signal(sig)) => {
                let sig = match self.dispatch_signal_if_needed(sig) {
                    Ok(dispatched) => dispatched,
                    Err(flow) => {
                        self.truncate_condition_stack(condition_stack_base);
                        return Err(flow);
                    }
                };
                self.truncate_condition_stack(condition_stack_base);
                if let Some(ResumeTarget::InterpreterConditionCase {
                    handler_index,
                    condition_stack_base: selected_stack_base,
                }) = sig.selected_resume.clone()
                    && selected_stack_base == condition_stack_base
                {
                    let handler = handlers_vec[handler_index];
                    let bind_var = !var.is_nil();
                    let binding_value = make_signal_binding_value(&sig);
                    let use_lexical_binding = bind_var
                        && self.lexical_binding()
                        && !is_runtime_dynamically_special(&self.obarray, var_id)
                        && !self.lexenv_declares_special_cached_in(self.lexenv, var_id);

                    let specpdl_count = self.specpdl.len();
                    if use_lexical_binding {
                        // Match GNU: specbind the lexenv, then cons the
                        // binding directly.
                        self.specpdl.push(SpecBinding::LexicalEnv {
                            old_lexenv: self.lexenv,
                        });
                        let binding =
                            Value::make_cons(lexenv_binding_symbol_value(var_id), binding_value);
                        self.lexenv = Value::make_cons(binding, self.lexenv);
                    } else if bind_var {
                        if let Err(flow) = self.try_specbind(var_id, binding_value) {
                            return self.unbind_to_with_result(specpdl_count, Err(flow));
                        }
                    }
                    let result = self.sf_progn_value(handler.cons_cdr());
                    return self.unbind_to_with_result(specpdl_count, result);
                }
                Err(Flow::Signal(sig))
            }
            Err(flow @ Flow::ThreadBlocked(_)) => {
                self.truncate_condition_stack(condition_stack_base);
                if let Flow::ThreadBlocked(ref blocked) = flow
                    && !blocked.remaining_forms.is_nil()
                {
                    return Err(Flow::thread_blocked(
                        blocked.blocker,
                        crate::emacs_core::threads::make_thread_condition_case_continuation(
                            var,
                            blocked.remaining_forms,
                            Value::list(handlers_vec.to_vec()),
                            self.lexenv,
                        ),
                    ));
                }
                Err(flow)
            }
            // A shutdown is not a condition: condition-case cannot handle it,
            // matching GNU, where Fkill_emacs exits and no handler ever runs.
            Err(flow @ (Flow::Throw(_) | Flow::Shutdown(_))) => {
                self.truncate_condition_stack(condition_stack_base);
                Err(flow)
            }
        }
    }

    pub(crate) fn resume_thread_condition_case_continuation(
        &mut self,
        var: Value,
        body: Value,
        handlers: Value,
        lexenv: Value,
    ) -> EvalResult {
        let Some(var_id) = var.as_symbol_id() else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), var],
            ));
        };
        let Some(handlers_vec) = list_to_vec(&handlers) else {
            return Err(self.listp_error(handlers));
        };
        let success_handler_idx = handlers_vec.iter().position(|handler| {
            handler.is_cons()
                && self
                    .unwrap_symbol(handler.cons_car())
                    .is_symbol_named(":success")
        });

        let specpdl_count = self.specpdl.len();
        self.specpdl.push(SpecBinding::LexicalEnv {
            old_lexenv: self.lexenv,
        });
        self.lexenv = lexenv;
        let thread_id = self.threads.current_thread_id();
        let pending = self.threads.take_pending_thread_signal(thread_id);
        let result =
            self.run_condition_case_body(var, var_id, &handlers_vec, success_handler_idx, |ctx| {
                if let Some(flow) = pending {
                    Err(flow)
                } else {
                    ctx.sf_progn_value(body)
                }
            });
        self.unbind_to_with_result(specpdl_count, result)
    }

    pub(super) fn sf_save_excursion_value(&mut self, tail: Value) -> EvalResult {
        let count = self.specpdl.len();
        self.record_save_excursion();
        let result = self.sf_progn_value(tail);
        self.unbind_to_with_result(count, result)
    }

    pub(super) fn sf_save_current_buffer_value(&mut self, tail: Value) -> EvalResult {
        // Specpdl-carried like the VM arm and GNU's
        // record_unwind_current_buffer, so a panic contained at a module/JIT
        // boundary inside the body restores the buffer via the boundary
        // unwind (an imperative restore here would be skipped, leaving the
        // wrong buffer current). PS fix-wave sweep hit.
        let count = self.specpdl.len();
        if let Some(buf) = self.buffers.current_buffer() {
            self.specpdl
                .push(SpecBinding::SaveCurrentBuffer { buffer_id: buf.id });
        }
        let result = self.sf_progn_value(tail);
        self.unbind_to_with_result(count, result)
    }

    pub(super) fn sf_save_restriction_value(&mut self, tail: Value) -> EvalResult {
        let count = self.specpdl.len();
        if let Some(state) = self.buffers.save_current_restriction_state() {
            self.specpdl.push(SpecBinding::save_restriction(state));
        }
        let result = self.sf_progn_value(tail);
        self.unbind_to_with_result(count, result)
    }

    pub(super) fn validate_throw(&self, flow: Flow) -> Flow {
        match flow {
            Flow::Throw(ref thrown) => {
                if self.has_active_catch(&thrown.tag) {
                    flow
                } else {
                    signal(LispCondition::NoCatch, vec![thrown.tag, thrown.value])
                }
            }
            other => other,
        }
    }

    /// Recursively walk a `Value`, treating everything as literal data
    /// except `(byte-code-literal ...)` cons cells which are converted to
    /// `Value::ByteCode` via `sf_byte_code_literal_value`.
    pub(super) fn quote_value_with_bytecode(&mut self, value: Value) -> EvalResult {
        if value.is_cons() && cons_head_symbol_id(&value) == Some(byte_code_literal_symbol()) {
            return self.sf_byte_code_literal_value(value.cons_cdr());
        }

        match value.kind() {
            ValueKind::Veclike(VecLikeType::Vector) => {
                let items = value.as_vector_data().unwrap();
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.quote_value_with_bytecode(*item)?);
                }
                Ok(Value::vector(values))
            }
            _ => Ok(value),
        }
    }

    pub(super) fn sf_byte_code_literal_value(&mut self, tail: Value) -> EvalResult {
        let vector = self.one_unevalled_arg(byte_code_literal_symbol(), tail)?;
        let Some(items) = vector.as_vector_data() else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("vectorp"), vector],
            ));
        };

        if items.len() < 4 {
            return Ok(vector);
        }

        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(self.quote_value_with_bytecode(*item)?);
        }

        crate::emacs_core::builtins::make_byte_code_from_slots(&values)
    }

    pub(super) fn sf_byte_code_value(&mut self, tail: Value) -> EvalResult {
        let args = list_to_vec(&tail).ok_or_else(|| self.listp_error(tail))?;
        if args.len() != 3 {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::symbol("byte-code"), Value::fixnum(args.len() as i64)],
            ));
        }
        let trace_toplevel_bytecode = std::env::var_os("NEOVM_TRACE_TOPLEVEL_BYTECODE").is_some();
        let load_file_name = if trace_toplevel_bytecode {
            self.obarray()
                .symbol_value("load-file-name")
                .and_then(|value| {
                    value
                        .as_lisp_string()
                        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
                })
                .unwrap_or_else(|| "<unknown>".to_string())
        } else {
            String::new()
        };
        let decode_start = trace_toplevel_bytecode.then(crate::host_time::Instant::now);

        let bytecode_str = args[0];
        let constants_vec = self.quote_value_with_bytecode(args[1])?;
        let maxdepth = args[2];

        use crate::emacs_core::bytecode::ByteCodeFunction;
        use crate::emacs_core::bytecode::decode::decode_gnu_bytecode_with_offset_map;
        use crate::emacs_core::value::LambdaParams;

        // Bytecode strings are unibyte and may contain non-UTF-8 bytes.
        // Access raw bytes directly, same fix as make_byte_code_from_parts.
        let raw_bytes = if let Some(ls) = bytecode_str.as_lisp_string() {
            ls.as_bytes().to_vec()
        } else {
            Vec::new()
        };

        let mut constants: Vec<Value> = match constants_vec.kind() {
            ValueKind::Veclike(VecLikeType::Vector) => {
                constants_vec.as_vector_data().unwrap().clone()
            }
            _ => Vec::new(),
        };

        for constant in &mut constants {
            *constant = crate::emacs_core::builtins::try_convert_nested_compiled_literal(*constant);
        }

        let (ops, gnu_byte_offset_map) =
            decode_gnu_bytecode_with_offset_map(&raw_bytes, &mut constants).map_err(|e| {
                signal(
                    "error",
                    vec![Value::string(format!("bytecode decode error: {}", e))],
                )
            })?;
        if let Some(start) = decode_start {
            tracing::info!(
                "TOPLEVEL-BYTECODE decode file={} bytes={} consts={} ops={} elapsed={:.2?}",
                load_file_name,
                raw_bytes.len(),
                constants.len(),
                ops.len(),
                start.elapsed()
            );
        }

        let max_stack = match maxdepth.kind() {
            ValueKind::Fixnum(n) => n as u16,
            _ => 16,
        };

        let bc = ByteCodeFunction {
            source_id: super::super::bytecode::fresh_bytecode_source_id(),
            ops,
            // The instructions above came straight from the sealing decoder;
            // the stack proof is recomputed below once every shape field
            // (params/lexical/arglist/env/max_stack) is in place.
            ops_sealed: true,
            stack_verified: false,
            constants: constants.into(),
            max_stack,
            params: LambdaParams::simple(vec![]),
            arglist: Value::NIL,
            lexical: false,
            env: None,
            gnu_byte_offset_map: Some(gnu_byte_offset_map),
            gnu_bytecode_bytes: None,
            docstring: None,
            doc_form: None,
            interactive: None,
            closure_slot_count: 4,
            extra_slots: Vec::new(),
            #[cfg(feature = "jit")]
            runtime: Some(crate::emacs_core::jit::Runtime::new()),
            lazy_gnu_code: None,
        };

        let mut vm = super::super::bytecode::Vm::from_context(self);
        let exec_start = trace_toplevel_bytecode.then(crate::host_time::Instant::now);
        let result = vm.execute(&bc, vec![]);
        if let Some(start) = exec_start {
            tracing::info!(
                "TOPLEVEL-BYTECODE exec   file={} ops={} elapsed={:.2?}",
                load_file_name,
                bc.executable_ops().len(),
                start.elapsed()
            );
        }
        result
    }

    pub(crate) fn defalias_value(&mut self, sym: Value, def: Value) -> EvalResult {
        let plan = builtins::plan_defalias_in_obarray(self.obarray(), &[sym, def])?;
        let builtins::DefaliasPlan { action, result, .. } = plan;
        self.record_load_history_entry(LoadHistoryEntry::function(result, def));
        self.record_defalias_function_history(result);
        match action {
            builtins::DefaliasAction::SetFunction { symbol, definition } => {
                self.note_macro_expansion_mutation();
                self.obarray.set_symbol_function_id(symbol, definition);
            }
            builtins::DefaliasAction::CallHook {
                hook,
                symbol_value,
                definition,
            } => {
                self.apply(hook, vec![symbol_value, definition])?;
            }
        }
        if let Some(symbol) = result.as_symbol_id() {
            let definition = self
                .obarray
                .symbol_function_id(symbol)
                .unwrap_or(Value::NIL);
            crate::emacs_core::interactive::sync_interactive_registry_for_symbol_definition(
                &mut self.interactive,
                symbol,
                definition,
            );
        }
        Ok(result)
    }

    /// GNU `defalias` records a `function-history' entry whenever a symbol
    /// that already has a function definition is redefined (`olddef' is
    /// non-nil; src/data.c:983-991).  This drives, among other things, the
    /// `Properties:' line of `apropos' output, so the property must exist on
    /// multiply-defined symbols (e.g. a C/preloaded builtin later redefined
    /// when its `.el' loads).  Must run *before* the new definition is
    /// installed, so the previous definition is still readable.
    pub(crate) fn record_defalias_function_history(&mut self, result: Value) {
        if let Some(symbol) = result.as_symbol_id() {
            let olddef = self
                .obarray
                .symbol_function_id(symbol)
                .unwrap_or(Value::NIL);
            if !olddef.is_nil() {
                self.add_to_function_history(symbol, olddef);
            }
        }
    }

    /// Port of GNU `add_to_function_history` (`src/data.c:933-968`): push
    /// `(FILE OLDDEF . PAST)` onto SYMBOL's `function-history' property, where
    /// FILE is the file currently being loaded (the trailing string element of
    /// `current-load-list', or nil).  If the property already has a record for
    /// FILE, the stale record is removed first so the history reflects only one
    /// entry per file (so an unload reverts cleanly).
    pub(super) fn add_to_function_history(&mut self, symbol: SymId, olddef: Value) {
        let history_prop = intern("function-history");
        let past = self
            .obarray
            .get_property_id(symbol, history_prop)
            .unwrap_or(Value::NIL);

        // FILE = trailing string element of current-load-list (GNU walks the
        // list looking for an entry whose cdr is nil and car is a string).
        let mut file = Value::NIL;
        let mut tail = self.visible_variable_value_or_nil("current-load-list");
        while tail.is_cons() {
            if tail.cons_cdr().is_nil() && tail.cons_car().is_string() {
                file = tail.cons_car();
            }
            tail = tail.cons_cdr();
        }

        // `(plist-member PAST FILE 'equal)` — find the existing record for FILE.
        if let Some(tem) = Self::plist_member_equal(past, file) {
            if tem == past {
                // New def from the same file as the last change: nothing to do.
                return;
            }
            // Remove the previous info for this file by splicing it out:
            // prev = nthcdr(len(past) - len(tem) - 2, past); (setcdr prev (cdr tem)).
            let past_len = Self::list_length(past);
            let tem_len = Self::list_length(tem);
            let tempos = past_len - tem_len;
            if tempos >= 2 {
                let mut prev = past;
                for _ in 0..(tempos - 2) {
                    prev = prev.cons_cdr();
                }
                if prev.is_cons() {
                    prev.set_cdr(tem.cons_cdr());
                }
            }
        }

        let roots = self.save_specpdl_roots();
        self.push_specpdl_root(past);
        self.push_specpdl_root(olddef);
        let new_history = Value::cons(file, Value::cons(olddef, past));
        self.push_specpdl_root(new_history);
        let _ = self
            .obarray
            .put_property_id(symbol, history_prop, new_history);
        self.restore_specpdl_roots(roots);
    }

    /// `(plist-member PLIST KEY 'equal)` restricted to the key positions
    /// (even indices), returning the tail starting at the matching key.
    pub(super) fn plist_member_equal(plist: Value, key: Value) -> Option<Value> {
        let mut tail = plist;
        while tail.is_cons() {
            if equal_value(&tail.cons_car(), &key, 0) {
                return Some(tail);
            }
            // Advance two cells (key, value).
            let cdr = tail.cons_cdr();
            if !cdr.is_cons() {
                break;
            }
            tail = cdr.cons_cdr();
        }
        None
    }

    pub(super) fn list_length(list: Value) -> i64 {
        let mut n = 0;
        let mut tail = list;
        while tail.is_cons() {
            n += 1;
            tail = tail.cons_cdr();
        }
        n
    }

    pub(super) fn current_load_list_is_file_context(current_load_list: Value) -> bool {
        let mut tail = current_load_list;
        while tail.is_cons() {
            if tail.cons_cdr().is_nil() && tail.cons_car().is_string() {
                return true;
            }
            tail = tail.cons_cdr();
        }
        false
    }

    pub(crate) fn record_load_history_entry(&mut self, entry: LoadHistoryEntry) {
        // GNU `defalias` omits autoload definitions while constructing a dump:
        // those entries are bootstrap implementation detail and otherwise add
        // substantial dead weight to the persisted load history.  Runtime
        // package autoload files have dump-mode=nil and remain observable.
        if entry.is_autoload_definition()
            && self.visible_variable_value_or_nil("dump-mode").is_truthy()
        {
            return;
        }
        let dedup = entry.should_deduplicate();
        let entry = entry.into_lisp_value();
        let current_load_list = self.visible_variable_value_or_nil("current-load-list");
        // GNU Frequire (fns.c) computes `from_file = load_in_progress` first
        // and only falls back to walking Vcurrent_load_list for the last
        // string element when no load is running (eval-buffer of a file).
        // During a load, `load-in-progress` is specbound t exactly while
        // `current-load-list` is bound to (filename), so the truthy check is
        // an O(1) equivalent of the walk. Without it, every recorded entry
        // re-walked the whole accumulated list — O(n^2) across a file with n
        // definitions (measured ~20% of the load of an 8000-form autoload
        // file, GNU LOADHIST_ATTACH is an unconditional O(1) prepend).
        let in_file_context = self
            .visible_variable_value_or_nil("load-in-progress")
            .is_truthy()
            || Self::current_load_list_is_file_context(current_load_list);
        if !in_file_context {
            return;
        }

        if dedup {
            let mut cursor = current_load_list;
            while cursor.is_cons() {
                if equal_value(&cursor.cons_car(), &entry, 0) {
                    return;
                }
                cursor = cursor.cons_cdr();
            }
        }

        let roots = self.save_specpdl_roots();
        self.push_specpdl_root(current_load_list);
        self.push_specpdl_root(entry);
        self.set_variable("current-load-list", Value::cons(entry, current_load_list));
        self.restore_specpdl_roots(roots);
    }

    #[tracing::instrument(level = "info", skip(self, subfeatures))]
    pub(crate) fn provide_value(
        &mut self,
        feature: Value,
        subfeatures: Option<Value>,
    ) -> EvalResult {
        self.note_macro_expansion_mutation();
        provide_value_in_state(&mut self.obarray, &mut self.features, feature, subfeatures)?;
        self.record_load_history_entry(LoadHistoryEntry::ProvidedFeature(feature));
        // GNU Emacs Fprovide (fns.c): after adding the feature, run any
        // load-hooks registered in `after-load-alist`.
        //   tem = Fassq(feature, Vafter_load_alist);
        //   if (CONSP(tem))  Fmapc(Qfuncall, XCDR(tem));
        //
        // GNU Emacs Fprovide: (mapc #'funcall (cdr (assq feature after-load-alist)))
        // Does NOT clear load-file-name — the delayed-func from eval-after-load
        // defers to after-load-functions when load-file-name is set, and
        // do-after-load-evaluation fires those hooks after the file finishes loading.
        self.run_after_load_hooks_for_feature(feature)?;
        Ok(feature)
    }

    /// Run `after-load-alist` callbacks for FEATURE, mirroring GNU's
    /// `Fprovide` behavior: `(mapc #'funcall (cdr (assq feature after-load-alist)))`.
    pub(super) fn run_after_load_hooks_for_feature(&mut self, feature: Value) -> Result<(), Flow> {
        let after_load_alist = self
            .obarray
            .symbol_value("after-load-alist")
            .cloned()
            .unwrap_or(Value::NIL);
        if after_load_alist.is_nil() {
            return Ok(());
        }
        // Walk after-load-alist looking for an entry whose car `eq` FEATURE.
        let entry = {
            let mut cursor = after_load_alist;
            let mut found = Value::NIL;
            while cursor.is_cons() {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                if pair_car.is_cons() {
                    let inner_pair_car = pair_car.cons_car();
                    if inner_pair_car == feature {
                        found = pair_car;
                        break;
                    }
                }
                cursor = pair_cdr;
            }
            found
        };
        if entry.is_nil() {
            return Ok(());
        }
        // entry is (FEATURE callback1 callback2 ...).
        // Call funcall on each callback in the cdr.
        // A callback can delete this entry from after-load-alist (its only
        // root) and trigger GC; root the entry for the walk, plus the moving
        // cursor in an updatable slot so even a mid-chain setcdr cannot free
        // the remainder we still read (marking is transitive from the
        // cursor, matching GNU's conservatively scanned tail local).
        let root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(entry);
        let cursor_slot = self.push_specpdl_root_slot(Value::NIL);
        let callbacks = entry.cons_cdr();
        let mut cursor = callbacks;
        let result = loop {
            if !cursor.is_cons() {
                break Ok(());
            }
            self.set_specpdl_root_slot(&cursor_slot, cursor);
            let pair_car = cursor.cons_car();
            let pair_cdr = cursor.cons_cdr();
            let callback = pair_car;
            if let Err(err) = self.apply(callback, vec![]) {
                break Err(err);
            }
            cursor = pair_cdr;
        };
        self.restore_specpdl_roots(root_scope);
        result
    }

    #[tracing::instrument(level = "info", skip(self), err(Debug))]
    pub(crate) fn require_value(
        &mut self,
        feature: Value,
        filename: Option<Value>,
        noerror: Option<Value>,
    ) -> EvalResult {
        let feature_name = super::super::builtins::symbols::symbol_id(&feature)
            .map(|sid| resolve_sym(sid).to_string());
        let filename_str = filename.as_ref().and_then(|v| {
            v.as_lisp_string()
                .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        });
        match plan_require_in_state(
            &self.obarray,
            self.buffers.current_buffer(),
            self.runtime_resource_store.as_deref(),
            &mut self.features,
            &self.require_stack,
            feature,
            filename,
            noerror,
        ) {
            Err(e) => {
                let rendered = super::super::error::format_flow_with_eval(self, &e);
                tracing::error!(
                    feature = ?feature_name,
                    filename = ?filename_str,
                    "require plan failed: {}", rendered
                );
                return Err(e);
            }
            Ok(plan) => {
                self.record_load_history_entry(LoadHistoryEntry::RequiredFeature(feature));
                match plan {
                    RequirePlan::Return(value) => Ok(value),
                    RequirePlan::Load {
                        sym_id,
                        name,
                        path,
                        missing_file,
                    } => {
                        // The nesting entry rides the specpdl (GNU fns.c
                        // Frequire: record_unwind_protect (require_unwind,
                        // ...)), so a panic contained at a module/JIT
                        // boundary inside the load pops it via the boundary
                        // unwind instead of leaking a spurious "Recursive
                        // require" entry.
                        let spec_entry = self.specpdl.len();
                        self.specpdl.push(SpecBinding::RequireStack {
                            len: self.require_stack.len(),
                        });
                        self.require_stack.push(sym_id);
                        let result =
                            super::super::autoload::with_implicit_load_state(self, |eval| {
                                eval.load_file_internal_with_options(
                                    &path,
                                    super::super::load::LoadOptions::implicit_dependency(
                                        missing_file,
                                    ),
                                )?;
                                eval.refresh_features_from_variable();
                                finish_require_in_state(&eval.features, sym_id, &name, Some(&path))
                            });
                        let result = self.unbind_to_with_result(spec_entry, result);
                        if let Err(ref e) = result
                            && !self.flow_has_active_handler(e)
                        {
                            let noerror_val =
                                noerror.as_ref().map(|v| !v.is_nil()).unwrap_or(false);
                            let path_str = path.display().to_string();
                            let rendered = super::super::error::format_flow_with_eval(self, e);
                            tracing::error!(
                                feature_name = ?feature_name,
                                path = %path_str,
                                noerror = noerror_val,
                                "require failed: {}", rendered
                            );
                        }
                        result
                    }
                }
            }
        }
    }

    pub(super) fn flow_has_active_handler(&self, flow: &Flow) -> bool {
        match flow {
            Flow::Signal(sig) => self.has_active_condition_handler_for_signal(sig),
            Flow::Throw(thrown) => self.has_active_catch(&thrown.tag),
            // Nothing handles a shutdown; it unwinds to the process boundary.
            Flow::ThreadBlocked(_) | Flow::Shutdown(_) => false,
        }
    }
}
