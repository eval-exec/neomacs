//! The Tier-I executor: the tree walker's per-form protocol and special
//! forms, over a compiled [`Node`] tree.
//!
//! Every function here is a line-by-line mirror of a tree-walker function,
//! named in its doc comment.  The mirrors read the live conses at the same
//! points and in the same order, call the same helpers for frames, depth,
//! polls, specpdl entries, temp roots and signals, and differ in three ways
//! only (see the module docs of `tier_i`):
//!
//! - where the tree walker calls `eval_sub (X)` on a subform, the mirror calls
//!   [`Context::ti_child`], which runs the compiled child when X is the very
//!   object it was compiled from and `eval_sub (X)` otherwise;
//! - a form's head class comes from the node's epoch-stamped cache instead of
//!   the shared form-head cache, and a form whose live head, argument list or
//!   class is not the compiled one is dispatched by the tree walker
//!   (`eval_sub_cons_dispatch`);
//! - binders record their lexical cells in slots, which variable references
//!   and `setq` read while the activation is trusted.
//!
//! # Why the slot read is exact
//!
//! A reference to SYM compiled with candidate slots (innermost binder first)
//! returns the first slot holding a cell.  Every candidate is a binder whose
//! scope encloses the reference: the path from the body to the reference is
//! the compiled one, because each node on it was reached through an `eq`
//! check at its position.  While the activation is trusted, each of those
//! binders bound the compiled symbol (a binder whose live element or symbol
//! differs marks the activation untrusted before anything in its scope runs),
//! and recorded either the cell it consed or nil for a dynamic binding.  The
//! environment at the reference holds, above the closure's captured
//! environment, exactly the cells those binders consed plus symbols a bare
//! `defvar` pushed (`assq` skips them): calls and islands leave
//! `self.lexenv` as they found it, through the specpdl.  So the first
//! lexically bound candidate's cell is the first `(SYM . VAL)` cell of the
//! environment, which is `assq`'s answer; with no lexically bound candidate
//! the tree walker's lookup runs.

use super::compile::{CondClause, FormNode, LetOp, Node, Op, Seq, SetqPair, Slot, VarNode};
use super::*;

/// What [`Context::ti_lexical_cell`] knows about a symbol's lexical cell.
enum LexicalCell {
    /// This cell, from a candidate slot.
    Cell(Value),
    /// The environment holds no cell for the symbol.
    None,
    /// Only `assq` can tell.
    Unknown,
}

/// One running activation of a compiled body.
pub(super) struct Act {
    /// This activation's first slot in `TierI::slots`.
    pub(super) base: usize,
    /// A binder's live shape was not the compiled one: take the tree
    /// walker's variable lookups from here on.
    pub(super) untrusted: Cell<bool>,
    /// `NEOVM_TIER_I=verify`: check the balance after every compiled form.
    pub(super) verify: bool,
    /// The captured environment may hold a `(SYM . VAL)` cell.  When it does
    /// not, `assq` on the activation's environment finds only cells the
    /// body's binders consed, so a symbol none of them bound lexically has
    /// no lexical cell at all.
    pub(super) env_bindings: bool,
    /// The environment may hold a bare symbol other than `t` (a special
    /// declaration `lexenv_declares_special` finds): the captured
    /// environment held one, or a `defvar` or a tree-walker island ran at
    /// this activation's level and may have pushed one.  While false no
    /// binder's symbol is declared special by the environment.
    pub(super) markers: Cell<bool>,
}

/// What the captured environment ENV can answer for [`Act::env_bindings`]
/// and [`Act::markers`]: both true when it is too long to look at.
fn captured_env_facts(env: Value) -> (bool, bool) {
    const MAX_SCANNED: usize = 16;
    let mut bindings = false;
    let mut markers = false;
    let mut cursor = env;
    for _ in 0..MAX_SCANNED {
        if !cursor.is_cons() {
            return (bindings, markers);
        }
        let entry = cursor.cons_car();
        if entry.is_cons() {
            bindings = true;
        } else if !entry.is_t() {
            markers = true;
        }
        cursor = cursor.cons_cdr();
    }
    if cursor.is_nil() {
        (bindings, markers)
    } else {
        (true, true)
    }
}

impl Context {
    // -----------------------------------------------------------------------
    // Activation
    // -----------------------------------------------------------------------

    /// Run CODE for a closure with ARGLIST and BODY whose formals are bound
    /// (lexically in NEW_ENV, or dynamically when NEW_ENV is nil): the body
    /// part of `run_lexical_closure_body` / `apply_lambda`.  Falls back to the
    /// tree walker's `eval_lambda_body_value` before anything runs when the
    /// closure is not the compiled one or a thread exists.
    pub(super) fn tier_i_run_code(
        &mut self,
        code: &TierCode,
        arglist: Value,
        body: Value,
        new_env: Value,
    ) -> EvalResult {
        if self.threads.current_thread_id() != 0 || self.threads.any_thread_created() {
            self.tier_i.stats.note(TierIEvent::RefuseThreads);
            return self.eval_lambda_body_value(body);
        }
        if arglist.bits() != code.arglist.bits() || body.bits() != code.body.bits() {
            self.tier_i.stats.note(TierIEvent::RefuseArglist);
            return self.eval_lambda_body_value(body);
        }
        let base = self.tier_i.slots.len();
        self.tier_i.slots.resize(base + code.nslots, Value::NIL);
        // A dynamic closure's body runs outside any lexical environment the
        // body's binders control: every lookup takes the tree walker's path.
        let (env_bindings, markers) = if new_env.is_nil() {
            (true, true)
        } else {
            match self.ti_bind_formal_slots(code, base, new_env) {
                Some(captured) => captured_env_facts(captured),
                None => {
                    self.tier_i.slots.truncate(base);
                    self.tier_i.stats.note(TierIEvent::RefuseFormals);
                    return self.eval_lambda_body_value(body);
                }
            }
        };
        self.tier_i.stats.note(TierIEvent::Run);
        let act = Act {
            base,
            untrusted: Cell::new(false),
            verify: self.tier_i.mode() == TierIMode::Verify,
            env_bindings,
            markers: Cell::new(markers),
        };
        let result = self.ti_body(&act, body, &code.seq);
        self.tier_i.slots.truncate(base);
        result
    }

    /// Record the formals' binding cells in their slots: `bind_lexical_formals`
    /// consed one cell per formal onto the captured environment, the last
    /// formal's first, so the first N cells of NEW_ENV are the formals in
    /// reverse.  The captured environment below them, or `None` when they
    /// are not the compiled formals (a mutated arglist); nothing has run yet,
    /// so the caller can still interpret.
    fn ti_bind_formal_slots(
        &mut self,
        code: &TierCode,
        base: usize,
        new_env: Value,
    ) -> Option<Value> {
        let mut cursor = new_env;
        for (index, sym) in code.formals.iter().enumerate().rev() {
            if !cursor.is_cons() {
                return None;
            }
            let cell = cursor.cons_car();
            if !cell.is_cons() || cell.cons_car().bits() != lexenv_binding_symbol_value(*sym).bits()
            {
                return None;
            }
            self.tier_i.slots[base + index] = cell;
            cursor = cursor.cons_cdr();
        }
        Some(cursor)
    }

    /// `eval_lambda_body_value`: the sampled stack probe, then the forms.
    fn ti_body(&mut self, act: &Act, body: Value, seq: &Seq) -> EvalResult {
        let depth = self.depth;
        if depth < STACK_GROWTH_PROBE_START_DEPTH
            || !depth.is_multiple_of(STACK_GROWTH_PROBE_INTERVAL)
        {
            return self.ti_body_forms(act, body, seq);
        }
        stacker::maybe_grow(EVAL_STACK_RED_ZONE, EVAL_STACK_SEGMENT, || {
            self.ti_body_forms(act, body, seq)
        })
    }

    /// `eval_lambda_body_forms`.
    fn ti_body_forms(&mut self, act: &Act, body: Value, seq: &Seq) -> EvalResult {
        let mut cursor = body;
        let mut last = Value::NIL;
        let mut index = 0;
        while cursor.is_cons() {
            match self.ti_nth(act, cursor.cons_car(), seq, index) {
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
            index += 1;
        }
        Ok(last)
    }

    // -----------------------------------------------------------------------
    // Subforms
    // -----------------------------------------------------------------------

    /// `eval_sub (FORM)`, through NODE when FORM is the object NODE was
    /// compiled from.
    #[inline(always)]
    fn ti_child(&mut self, act: &Act, form: Value, node: &Node) -> EvalResult {
        if form.bits() == node.form().bits() {
            self.ti_node(act, node)
        } else {
            self.ti_eval_island(act, form)
        }
    }

    /// `eval_sub (FORM)` for the INDEX-th form of a list compiled as SEQ.
    #[inline(always)]
    fn ti_nth(&mut self, act: &Act, form: Value, seq: &Seq, index: usize) -> EvalResult {
        match seq.get(index) {
            Some(node) => self.ti_child(act, form, node),
            None => self.ti_eval_island(act, form),
        }
    }

    /// `eval_sub (FORM)` by the tree walker, at this activation's level: a
    /// bare `defvar` inside it may push a special declaration onto the
    /// environment ([`Act::markers`]).
    #[inline(never)]
    fn ti_eval_island(&mut self, act: &Act, form: Value) -> EvalResult {
        self.tier_i.stats.note(TierIEvent::Island);
        let lexenv = self.lexenv;
        let result = self.eval_sub(form);
        if self.lexenv.bits() != lexenv.bits() {
            act.markers.set(true);
        }
        result
    }

    /// `eval_sub` of the form NODE was compiled from.
    fn ti_node(&mut self, act: &Act, node: &Node) -> EvalResult {
        match node {
            // `eval_sub`'s non-cons arm (the form is no symbol with
            // position, so `unwrap_symbol` leaves it alone).
            Node::Const(value) => Ok(*value),
            Node::Var(var) => self.ti_var(act, var),
            Node::Form(form) => {
                if act.verify {
                    return self.ti_form_verified(act, form);
                }
                self.ti_form(act, form)
            }
            Node::Eval(form) => self.ti_eval_island(act, *form),
        }
    }

    /// `eval_sub`'s symbol arm: the first lexically bound candidate slot; no
    /// lexical cell at all when neither a slot nor the captured environment
    /// can hold one; otherwise the tree walker's lookup.
    #[inline]
    fn ti_var(&mut self, act: &Act, var: &VarNode) -> EvalResult {
        let result = match self.ti_lexical_cell(act, &var.slots) {
            LexicalCell::Cell(cell) => return Ok(cell.cons_cdr()),
            LexicalCell::None => match self.find_symbol_value_by_id(var.sym) {
                Ok(SymbolValueLookup::Bound(value)) => return Ok(value),
                Ok(SymbolValueLookup::Unbound) => Err(signal(
                    LispCondition::VoidVariable,
                    vec![value_from_symbol_id(var.sym)],
                )),
                Err(flow) => Err(flow),
            },
            LexicalCell::Unknown => self.eval_symbol_by_id(var.sym),
        };
        self.dispatch_signal_result_if_needed(result)
    }

    /// The binding cell `assq` would find on the activation's environment for
    /// a symbol whose candidate slots are SLOTS (see the module docs).
    #[inline(always)]
    fn ti_lexical_cell(&self, act: &Act, slots: &[Slot]) -> LexicalCell {
        if act.untrusted.get() {
            return LexicalCell::Unknown;
        }
        for slot in slots {
            let cell = self.tier_i.slots[act.base + usize::from(*slot)];
            if cell.is_cons() {
                return LexicalCell::Cell(cell);
            }
        }
        if act.env_bindings || !self.lexical_binding() {
            // `!lexical_binding()`: the tree walker's lookup skips the
            // environment itself.
            return LexicalCell::Unknown;
        }
        LexicalCell::None
    }

    /// `lexenv_declares_special_cached_in (self.lexenv, ID)`: false without
    /// a walk while the environment can hold no special declaration
    /// ([`Act::markers`]; a `t` element declares only `t`, which a `let`
    /// refuses as a constant before it asks).
    #[inline(always)]
    fn ti_declared_special(&self, act: &Act, id: SymId) -> bool {
        act.markers.get() && self.lexenv_declares_special_cached_in(self.lexenv, id)
    }

    fn ti_untrust(&mut self, act: &Act) {
        if !act.untrusted.get() {
            act.untrusted.set(true);
            self.tier_i.stats.note(TierIEvent::Untrusted);
        }
    }

    // -----------------------------------------------------------------------
    // Cons forms
    // -----------------------------------------------------------------------

    /// `eval_sub`'s cons arm.
    fn ti_form(&mut self, act: &Act, f: &FormNode) -> EvalResult {
        self.enter_interpreted_eval_depth()?;
        let result = self.maybe_grow_eval_stack(|ctx| {
            ctx.maybe_quit_before_gc()?;
            if ctx.gc_safe_point_exact_should_collect() {
                ctx.collect_at_eval_safe_point(f.form);
            }
            ctx.ti_form_cons(act, f)
        });
        self.depth -= 1;
        result
    }

    /// [`Self::ti_form`] with the `verify` balance check: a cons form leaves
    /// the depth, the specpdl and the operand stack as it found them, and a
    /// mirrored special form or call also the lexical environment (a bare
    /// `defvar` and the tree walker's islands may push a symbol onto it).
    #[cold]
    #[inline(never)]
    fn ti_form_verified(&mut self, act: &Act, f: &FormNode) -> EvalResult {
        let depth = self.depth;
        let specpdl = self.specpdl.len();
        let stack = self.bc_buf.len();
        let lexenv = self.lexenv;
        let result = self.ti_form(act, f);
        let lexenv_may_move = matches!(f.op, Op::Special(SpecialFormHandler::Defvar) | Op::Call(_));
        let balanced = self.depth == depth
            && self.specpdl.len() == specpdl
            && self.bc_buf.len() == stack
            && (lexenv_may_move || self.lexenv.bits() == lexenv.bits());
        if !balanced {
            panic!(
                "tier-i verify: {} left depth {}->{} specpdl {}->{} stack {}->{} lexenv moved {}",
                super::super::super::print::print_value(&f.form),
                depth,
                self.depth,
                specpdl,
                self.specpdl.len(),
                stack,
                self.bc_buf.len(),
                self.lexenv.bits() != lexenv.bits()
            );
        }
        result
    }

    /// `eval_sub_cons`: the UNEVALLED frame, `debug-on-next-call`, dispatch,
    /// the signal gate and the frame's retirement.
    fn ti_form_cons(&mut self, act: &Act, f: &FormNode) -> EvalResult {
        let original_fun = self.unwrap_symbol(f.form.cons_car());
        let original_args = f.form.cons_cdr();
        let outer_bt_count = self.specpdl.len();
        let stack_base = self.bc_buf.len();
        self.push_unevalled_form_frame(original_fun, original_args);
        let dispatch_result = match self.take_debug_on_call_arm(DebugOnCallCode::EvalForm) {
            Some(arm) => self.do_debug_on_call(arm).and_then(|()| {
                self.ti_dispatch(act, f, original_fun, original_args, outer_bt_count)
            }),
            None => self.ti_dispatch(act, f, original_fun, original_args, outer_bt_count),
        };
        let result = self.dispatch_signal_result_if_needed(dispatch_result);
        self.retire_cons_frame(outer_bt_count, stack_base, result)
    }

    /// The node's head class, re-read when the function epoch moved.
    #[inline(always)]
    fn ti_head(&self, f: &FormNode) -> FormHead {
        let epoch = self.obarray.function_epoch();
        let (stamp, head) = f.head_cache.get();
        if stamp == epoch {
            return head;
        }
        let head = FormHead::classify(f.head_id, self.obarray.symbol_function_id(f.head_id));
        f.head_cache.set((epoch, head));
        head
    }

    /// The tree walker's dispatch of a form this node cannot run.
    #[inline(never)]
    fn ti_island(
        &mut self,
        act: &Act,
        original_fun: Value,
        original_args: Value,
        outer_bt_count: usize,
    ) -> EvalResult {
        self.tier_i.stats.note(TierIEvent::Island);
        let lexenv = self.lexenv;
        let result = self.eval_sub_cons_dispatch(original_fun, original_args, outer_bt_count);
        if self.lexenv.bits() != lexenv.bits() {
            act.markers.set(true);
        }
        result
    }

    /// `eval_sub_cons_dispatch` for a compiled form: the class arms it takes
    /// for a cached head, with the compiled children; everything else is the
    /// tree walker's own dispatch.
    fn ti_dispatch(
        &mut self,
        act: &Act,
        f: &FormNode,
        original_fun: Value,
        original_args: Value,
        outer_bt_count: usize,
    ) -> EvalResult {
        if original_fun.bits() != f.head.bits()
            || original_args.bits() != f.tail.bits()
            || self.compiler_function_overrides_active()
        {
            return self.ti_island(act, original_fun, original_args, outer_bt_count);
        }
        let head = self.ti_head(f);
        let Some(func) = head.func else {
            return self.ti_island(act, original_fun, original_args, outer_bt_count);
        };
        match &f.op {
            Op::Call(args) => match head.class {
                HeadClass::Subr {
                    function,
                    min_args,
                    max_args,
                } => {
                    let numargs = match list_length(&original_args) {
                        Some(n) => n,
                        None => return Err(self.listp_error(original_args)),
                    };
                    if numargs < min_args as usize || max_args.is_some_and(|m| numargs > m as usize)
                    {
                        return Err(signal(
                            LispCondition::WrongNumberOfArguments,
                            vec![original_fun, Value::fixnum(numargs as i64)],
                        ));
                    }
                    #[cfg(feature = "vm-profile")]
                    if let Some(id) = func.as_subr_id() {
                        crate::emacs_core::bytecode::vm::vm_profile::bump_subr(id);
                    }
                    let (first_arg, nargs) =
                        self.ti_call_args_onto_stack(act, func, original_args, args)?;
                    self.set_backtrace_args_evalled_bc_span(outer_bt_count, first_arg, nargs);
                    self.dispatch_subr_fn_from_bc_stack(function, first_arg, nargs)
                }
                HeadClass::ByteCode | HeadClass::Lambda => {
                    let bc_data = match head.class {
                        HeadClass::ByteCode => func.get_bytecode_data(),
                        _ => None,
                    };
                    if bc_data.is_none() && !matches!(head.class, HeadClass::Lambda) {
                        return self.ti_island(act, original_fun, original_args, outer_bt_count);
                    }
                    if list_length(&original_args).is_none() {
                        return Err(self.listp_error(original_args));
                    }
                    let (first_arg, nargs) =
                        self.ti_call_args_onto_stack(act, func, original_args, args)?;
                    self.set_backtrace_args_evalled_bc_span(outer_bt_count, first_arg, nargs);
                    match bc_data {
                        Some(bc_data) => {
                            self.execute_bytecode_call_from_stack(bc_data, first_arg, nargs, func)
                        }
                        None => self.apply_closure_from_bc_stack(func, first_arg, nargs),
                    }
                }
                HeadClass::SpecialForm(_) | HeadClass::Slow => {
                    self.ti_island(act, original_fun, original_args, outer_bt_count)
                }
            },
            op => match head.class {
                HeadClass::SpecialForm(handler) if op.handler() == Some(handler) => {
                    if list_length(&original_args).is_none() {
                        return Err(self.listp_error(original_args));
                    }
                    self.ti_special(act, f.head_id, original_args, op)
                }
                _ => self.ti_island(act, original_fun, original_args, outer_bt_count),
            },
        }
    }

    /// `eval_call_args_onto_stack`.
    #[inline]
    fn ti_call_args_onto_stack(
        &mut self,
        act: &Act,
        func: Value,
        original_args: Value,
        args: &Seq,
    ) -> Result<(usize, usize), Flow> {
        let func_slot = self.bc_buf.len();
        self.bc_buf.push(func);
        let first_arg = func_slot + 1;
        let mut cursor = original_args;
        let mut index = 0;
        while cursor.is_cons() {
            let arg_form = cursor.cons_car();
            let arg_val = self.ti_nth(act, arg_form, args, index)?;
            self.bc_buf.push(arg_val);
            cursor = cursor.cons_cdr();
            index += 1;
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(cursor));
        }
        Ok((first_arg, self.bc_buf.len() - first_arg))
    }

    // -----------------------------------------------------------------------
    // Special forms
    // -----------------------------------------------------------------------

    /// `run_special_form` for a mirrored special form (the depth restore
    /// included); `Op::Special` runs the tree walker's handler itself.
    fn ti_special(&mut self, act: &Act, call_name: SymId, tail: Value, op: &Op) -> EvalResult {
        if let Op::Special(handler) = op {
            if *handler == SpecialFormHandler::Defvar {
                act.markers.set(true);
            }
            return self.run_special_form(*handler, call_name, tail);
        }
        let saved_depth = self.depth;
        let result = match op {
            Op::Progn(seq) => self.ti_progn(act, tail, seq),
            Op::And(seq) => self.ti_and(act, tail, seq),
            Op::Or(seq) => self.ti_or(act, tail, seq),
            Op::If {
                cond,
                then,
                otherwise,
            } => self.ti_if(act, call_name, tail, cond, then, otherwise),
            Op::Cond(clauses) => self.ti_cond(act, tail, clauses),
            Op::While { test, body } => self.ti_while(act, call_name, tail, test, body),
            Op::Setq(pairs) => self.ti_setq(act, call_name, tail, pairs),
            Op::Let(let_op) => self.ti_let(act, call_name, tail, let_op),
            Op::LetStar(let_op) => self.ti_let_star(act, call_name, tail, let_op),
            Op::Prog1 { first, rest } => self.ti_prog1(act, call_name, tail, first, rest),
            Op::Catch { tag, body } => self.ti_catch(act, call_name, tail, tag, body),
            Op::UnwindProtect { body } => self.ti_unwind_protect(act, call_name, tail, body),
            Op::ConditionCase { body } => self.ti_condition_case(act, call_name, tail, body),
            Op::SaveExcursion(seq) => {
                let count = self.specpdl.len();
                self.record_save_excursion();
                let result = self.ti_progn(act, tail, seq);
                self.unbind_to_with_result(count, result)
            }
            Op::SaveCurrentBuffer(seq) => {
                let count = self.specpdl.len();
                if let Some(buf) = self.buffers.current_buffer() {
                    self.specpdl
                        .push(SpecBinding::SaveCurrentBuffer { buffer_id: buf.id });
                }
                let result = self.ti_progn(act, tail, seq);
                self.unbind_to_with_result(count, result)
            }
            Op::SaveRestriction(seq) => {
                let count = self.specpdl.len();
                if let Some(state) = self.buffers.save_current_restriction_state() {
                    self.specpdl.push(SpecBinding::save_restriction(state));
                }
                let result = self.ti_progn(act, tail, seq);
                self.unbind_to_with_result(count, result)
            }
            Op::Call(_) | Op::Special(_) => unreachable!("not a mirrored special form"),
        };
        self.depth = saved_depth;
        result
    }

    /// `sf_progn_value`.
    fn ti_progn(&mut self, act: &Act, forms: Value, seq: &Seq) -> EvalResult {
        let temp_scope = self.save_sequence_temp_roots();
        let result = self.ti_progn_forms(act, forms, seq);
        self.restore_sequence_temp_roots(temp_scope);
        result
    }

    /// The loop inside `sf_progn_value`'s temp-root scope.
    fn ti_progn_forms(&mut self, act: &Act, forms: Value, seq: &Seq) -> EvalResult {
        let mut cursor = forms;
        let mut last = Value::NIL;
        let mut index = 0;
        while cursor.is_cons() {
            match self.ti_nth(act, cursor.cons_car(), seq, index) {
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
            index += 1;
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(forms));
        }
        Ok(last)
    }

    /// `sf_and_value`.
    fn ti_and(&mut self, act: &Act, tail: Value, seq: &Seq) -> EvalResult {
        let mut cursor = tail;
        let mut last = Value::T;
        let mut index = 0;
        while cursor.is_cons() {
            last = self.ti_nth(act, cursor.cons_car(), seq, index)?;
            if last.is_nil() {
                return Ok(Value::NIL);
            }
            cursor = cursor.cons_cdr();
            index += 1;
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(last)
    }

    /// `sf_or_value`.
    fn ti_or(&mut self, act: &Act, tail: Value, seq: &Seq) -> EvalResult {
        let mut cursor = tail;
        let mut index = 0;
        while cursor.is_cons() {
            let value = self.ti_nth(act, cursor.cons_car(), seq, index)?;
            if value.is_truthy() {
                return Ok(value);
            }
            cursor = cursor.cons_cdr();
            index += 1;
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(Value::NIL)
    }

    /// `sf_if_value_named`.
    fn ti_if(
        &mut self,
        act: &Act,
        call_name: SymId,
        tail: Value,
        cond: &Node,
        then: &Node,
        otherwise: &Seq,
    ) -> EvalResult {
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
        if self.ti_child(act, cond_form, cond)?.is_truthy() {
            self.ti_child(act, then_form, then)
        } else {
            self.ti_progn(act, rest, otherwise)
        }
    }

    /// `sf_cond_value`.
    fn ti_cond(&mut self, act: &Act, tail: Value, compiled: &[CondClause]) -> EvalResult {
        let mut clauses = tail;
        let mut index = 0;
        while clauses.is_cons() {
            let clause = clauses.cons_car();
            clauses = clauses.cons_cdr();
            let compiled_clause = compiled
                .get(index)
                .filter(|compiled| compiled.clause.bits() == clause.bits());
            index += 1;
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
            let test_value = match compiled_clause {
                Some(compiled) => self.ti_child(act, test, &compiled.test)?,
                None => self.ti_eval_island(act, test)?,
            };
            if test_value.is_truthy() {
                if body.is_nil() {
                    return Ok(test_value);
                }
                return match compiled_clause {
                    Some(compiled) => self.ti_progn(act, body, &compiled.body),
                    None => {
                        act.markers.set(true);
                        self.sf_progn_value(body)
                    }
                };
            }
        }
        if !clauses.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(Value::NIL)
    }

    /// `sf_while_value_named`.
    fn ti_while(
        &mut self,
        act: &Act,
        call_name: SymId,
        tail: Value,
        test: &Node,
        body_seq: &Seq,
    ) -> EvalResult {
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
            if self.ti_child(act, test_form, test)?.is_nil() {
                return Ok(Value::NIL);
            }
            self.ti_progn(act, body, body_seq)?;
            iters += 1;
            if iters == 1_000_000 {
                let cond_str = super::super::super::print::print_value(&test_form);
                tracing::warn!(
                    "while loop exceeded 1M iterations, cond: {}",
                    &cond_str[..cond_str.len().min(300)]
                );
            }
            self.maybe_quit()?;
        }
    }

    /// `sf_setq_value_named`.
    fn ti_setq(
        &mut self,
        act: &Act,
        call_name: SymId,
        tail: Value,
        pairs: &[SetqPair],
    ) -> EvalResult {
        if tail.is_nil() {
            return Ok(Value::NIL);
        }
        let mut cursor = tail;
        let mut last = Value::NIL;
        let mut nargs: usize = 0;
        let mut index = 0;
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
            let pair = pairs.get(index);
            index += 1;
            let symbol = self.unwrap_symbol(symbol);
            let Some(sym_id) = symbol.as_symbol_id() else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("symbolp"), symbol],
                ));
            };
            let value = match pair {
                Some(pair) => self.ti_child(act, value_form, &pair.value)?,
                None => self.ti_eval_island(act, value_form)?,
            };
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
            // `assign_setq_by_id`'s first stage is `assq` on the environment,
            // which the pair's candidate slots answer when the live symbol is
            // the compiled one.
            let cell = match pair.filter(|pair| pair.symbol.bits() == symbol.bits()) {
                Some(pair) => self.ti_lexical_cell(act, &pair.slots),
                None => LexicalCell::Unknown,
            };
            match cell {
                LexicalCell::Cell(cell) => lexenv_set(cell, value),
                LexicalCell::None => {
                    self.assign_setq_dynamic_by_id(sym_id, value)?;
                }
                LexicalCell::Unknown => {
                    self.assign_setq_by_id(sym_id, value)?;
                }
            }
            last = value;
        }
        if !cursor.is_nil() {
            return Err(self.listp_error(tail));
        }
        Ok(last)
    }

    /// `sf_let_value_named`, recording each binder's cell (or nil) in its
    /// slot.
    fn ti_let(&mut self, act: &Act, call_name: SymId, tail: Value, op: &LetOp) -> EvalResult {
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
        let mut lexical_bindings: SmallVec<[(SymId, Value, Option<Slot>); 8]> = SmallVec::new();
        let mut dynamic_sym_ids = LetBindingVec::new();
        let mut dynamic_slots: SmallVec<[Slot; 8]> = SmallVec::new();
        let use_lexical = self.lexical_binding();
        let mut constant_binding_error: Option<String> = None;
        let temps_base = self.bc_buf.len();
        if varlist.bits() != op.varlist.bits() {
            self.ti_untrust(act);
        }
        let mut bindings = varlist;
        let mut index = 0;

        while bindings.is_cons() {
            let element = bindings.cons_car();
            let binding = self.unwrap_symbol(element);
            bindings = bindings.cons_cdr();
            let compiled = op.bindings.get(index);
            index += 1;
            if let Some(id) = binding.as_symbol_id() {
                let slot = compiled
                    .filter(|c| c.element.bits() == element.bits() && c.sym == id)
                    .map(|c| c.slot);
                if slot.is_none() {
                    self.ti_untrust(act);
                }
                // A bare binder binds nil, which is never a keyword's own value.
                if let Some(name) = let_constant_error_name(&self.obarray, id, Value::NIL) {
                    if constant_binding_error.is_none() {
                        constant_binding_error = Some(name);
                    }
                    continue;
                }
                if use_lexical
                    && !self.obarray.is_special_id(id)
                    && !self.ti_declared_special(act, id)
                {
                    lexical_bindings.push((id, Value::NIL, slot));
                } else {
                    dynamic_sym_ids.push((id, Value::NIL));
                    dynamic_slots.extend(slot);
                }
                continue;
            }
            if !binding.is_cons() {
                self.bc_buf.truncate(temps_base);
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), binding],
                ));
            }
            let head = self.unwrap_symbol(binding.cons_car());
            let Some(id) = head.as_symbol_id() else {
                self.bc_buf.truncate(temps_base);
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("symbolp"), head],
                ));
            };
            let compiled = compiled.filter(|c| c.element.bits() == element.bits() && c.sym == id);
            if compiled.is_none() {
                self.ti_untrust(act);
            }
            let mut value_tail = binding.cons_cdr();
            let value = if value_tail.is_nil() {
                Value::NIL
            } else if value_tail.is_cons() {
                let init_form = value_tail.cons_car();
                value_tail = value_tail.cons_cdr();
                if !value_tail.is_nil() {
                    self.bc_buf.truncate(temps_base);
                    return Err(signal(
                        "error",
                        vec![
                            Value::string("`let' bindings can have only one value-form"),
                            binding,
                        ],
                    ));
                }
                let init = match compiled.and_then(|c| c.init.as_ref()) {
                    Some(node) => self.ti_child(act, init_form, node),
                    None => self.ti_eval_island(act, init_form),
                };
                match init {
                    Ok(value) => value,
                    Err(err) => {
                        self.bc_buf.truncate(temps_base);
                        return Err(err);
                    }
                }
            } else {
                self.bc_buf.truncate(temps_base);
                return Err(self.listp_error(binding));
            };
            self.bc_buf.push(value);
            if let Some(name) = let_constant_error_name(&self.obarray, id, value) {
                if constant_binding_error.is_none() {
                    constant_binding_error = Some(name);
                }
                continue;
            }
            let slot = compiled.map(|c| c.slot);
            if use_lexical && !self.obarray.is_special_id(id) && !self.ti_declared_special(act, id)
            {
                lexical_bindings.push((id, value, slot));
            } else {
                dynamic_sym_ids.push((id, value));
                dynamic_slots.extend(slot);
            }
        }
        if !bindings.is_nil() {
            self.bc_buf.truncate(temps_base);
            return Err(self.listp_error(varlist));
        }
        if let Some(name) = constant_binding_error {
            self.bc_buf.truncate(temps_base);
            return Err(signal(
                LispCondition::SettingConstant,
                vec![Value::symbol(name)],
            ));
        }
        if index != op.bindings.len() {
            self.ti_untrust(act);
        }

        // As `sf_let_value_named`: nothing below can collect until the
        // environment is installed (only conses are allocated).
        self.bc_buf.truncate(temps_base);
        let lexenv_at_entry = self.lexenv;
        let specpdl_count = self.specpdl.len();
        if use_lexical {
            self.push_specpdl_with(|| SpecBinding::LexicalEnv {
                old_lexenv: lexenv_at_entry,
            });
        }
        let mut new_lexenv = lexenv_at_entry;
        for (sym_id, val, slot) in &lexical_bindings {
            let binding_pair = Value::make_cons(lexenv_binding_symbol_value(*sym_id), *val);
            new_lexenv = Value::make_cons(binding_pair, new_lexenv);
            if let Some(slot) = slot {
                self.tier_i.slots[act.base + usize::from(*slot)] = binding_pair;
            }
        }
        for slot in &dynamic_slots {
            self.tier_i.slots[act.base + usize::from(*slot)] = Value::NIL;
        }
        self.lexenv = new_lexenv;

        let temp_scope = self.save_eval_temp_roots();
        for value in lexical_bindings
            .iter()
            .map(|(_, value, _)| value)
            .chain(dynamic_sym_ids.iter().map(|(_, value)| value))
        {
            self.push_eval_temp_root(*value);
        }
        for (sym_id, value) in &dynamic_sym_ids {
            if let Err(flow) = self.try_specbind(*sym_id, *value) {
                let result = self.unbind_to_with_result(specpdl_count, Err(flow));
                self.restore_eval_temp_roots_to_sequence(temp_scope);
                return result;
            }
        }

        let result = self.ti_progn(act, body, &op.body);
        let result = self.unbind_lexenv_frame(specpdl_count, result);
        self.restore_eval_temp_roots_to_sequence(temp_scope);
        result
    }

    /// `sf_let_star_value_named`.
    fn ti_let_star(&mut self, act: &Act, call_name: SymId, tail: Value, op: &LetOp) -> EvalResult {
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
        if use_lexical {
            let old_lexenv = self.lexenv;
            self.push_specpdl_with(|| SpecBinding::LexicalEnv { old_lexenv });
        }
        if varlist.bits() != op.varlist.bits() {
            self.ti_untrust(act);
        }

        let temp_scope = self.save_eval_temp_roots();
        let val_temp_slot = self.push_eval_temp_root_slot(Value::NIL);
        let init_result = self.ti_let_star_bindings(act, varlist, use_lexical, val_temp_slot, op);
        if let Err(error) = init_result {
            let result = self.unbind_to_with_result(specpdl_count, Err(error));
            self.restore_eval_temp_roots_to_sequence(temp_scope);
            return result;
        }

        let result = self.ti_progn(act, body, &op.body);
        let result = self.unbind_to_with_result(specpdl_count, result);
        self.restore_eval_temp_roots_to_sequence(temp_scope);
        result
    }

    /// The binding loop of `sf_let_star_value_named`.
    fn ti_let_star_bindings(
        &mut self,
        act: &Act,
        varlist: Value,
        use_lexical: bool,
        val_temp_slot: usize,
        op: &LetOp,
    ) -> Result<(), Flow> {
        let mut bindings = varlist;
        let mut index = 0;
        while bindings.is_cons() {
            let element = bindings.cons_car();
            let binding = self.unwrap_symbol(element);
            bindings = bindings.cons_cdr();
            let compiled = op.bindings.get(index);
            index += 1;
            let (id, value, slot) = if let Some(id) = binding.as_symbol_id() {
                let slot = compiled
                    .filter(|c| c.element.bits() == element.bits() && c.sym == id)
                    .map(|c| c.slot);
                (id, Value::NIL, slot)
            } else if binding.is_cons() {
                let head = self.unwrap_symbol(binding.cons_car());
                let Some(id) = head.as_symbol_id() else {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("symbolp"), head],
                    ));
                };
                let compiled =
                    compiled.filter(|c| c.element.bits() == element.bits() && c.sym == id);
                if compiled.is_none() {
                    // Before the init runs: it may see this let*'s earlier
                    // binders, whose slots are no longer the compiled ones.
                    self.ti_untrust(act);
                }
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
                    match compiled.and_then(|c| c.init.as_ref()) {
                        Some(node) => self.ti_child(act, init_form, node)?,
                        None => self.ti_eval_island(act, init_form)?,
                    }
                } else {
                    return Err(self.listp_error(binding));
                };
                (id, value, compiled.map(|c| c.slot))
            } else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), binding],
                ));
            };
            if slot.is_none() {
                self.ti_untrust(act);
            }
            self.set_eval_temp_root_slot(val_temp_slot, value);

            if let Some(name) = let_constant_error_name(&self.obarray, id, value) {
                return Err(signal(
                    LispCondition::SettingConstant,
                    vec![Value::symbol(&name)],
                ));
            }
            if use_lexical && !self.obarray.is_special_id(id) && !self.ti_declared_special(act, id)
            {
                let binding = Value::make_cons(lexenv_binding_symbol_value(id), value);
                self.lexenv = Value::make_cons(binding, self.lexenv);
                if let Some(slot) = slot {
                    self.tier_i.slots[act.base + usize::from(slot)] = binding;
                }
            } else {
                if let Some(slot) = slot {
                    self.tier_i.slots[act.base + usize::from(slot)] = Value::NIL;
                }
                self.try_specbind(id, value)?;
            }
        }
        if !bindings.is_nil() {
            return Err(self.listp_error(varlist));
        }
        if index != op.bindings.len() {
            self.ti_untrust(act);
        }
        Ok(())
    }

    /// `sf_prog1_value_named`.
    fn ti_prog1(
        &mut self,
        act: &Act,
        call_name: SymId,
        tail: Value,
        first_node: &Node,
        rest_seq: &Seq,
    ) -> EvalResult {
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
        let first = self.ti_child(act, first_form, first_node)?;
        let specpdl_root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(first);
        let result = self.ti_progn(act, rest, rest_seq);
        self.restore_specpdl_roots(specpdl_root_scope);
        result?;
        Ok(first)
    }

    /// `sf_catch_value_named`.
    fn ti_catch(
        &mut self,
        act: &Act,
        call_name: SymId,
        tail: Value,
        tag_node: &Node,
        body_seq: &Seq,
    ) -> EvalResult {
        if tail.is_nil() {
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![Value::from_sym_id(call_name), Value::fixnum(0)],
            ));
        }
        if !tail.is_cons() {
            return Err(self.listp_error(tail));
        }
        let tag = self.ti_child(act, tail.cons_car(), tag_node)?;
        self.push_condition_frame(ConditionFrame::Catch {
            tag,
            resume: ResumeTarget::InterpreterCatch,
        });
        let specpdl_count = self.specpdl.len();
        let result = match self.ti_progn(act, tail.cons_cdr(), body_seq) {
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
        self.unbind_to_with_result(specpdl_count, result)
    }

    /// `sf_unwind_protect_value_named`.
    fn ti_unwind_protect(
        &mut self,
        act: &Act,
        call_name: SymId,
        tail: Value,
        body_node: &Node,
    ) -> EvalResult {
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
        let lexenv = self.lexenv;
        self.push_specpdl_with(|| SpecBinding::UnwindProtect {
            forms: cleanup_forms,
            lexenv,
        });
        let result = self.ti_child(act, body, body_node);
        self.unbind_to_with_result(specpdl_count, result)
    }

    /// `sf_condition_case_value_named`: its parse, then the shared
    /// `run_condition_case_body` with the compiled body.
    fn ti_condition_case(
        &mut self,
        act: &Act,
        call_name: SymId,
        tail: Value,
        body_node: &Node,
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
                        super::super::super::print::print_value(&handler)
                    ))],
                ));
            }
            let head = handler.cons_car();
            if !(head.is_symbol() || head.is_symbol_with_pos() || head.is_cons()) {
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Invalid condition handler: {}",
                        super::super::super::print::print_value(&handler)
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
            ctx.ti_child(act, body, body_node)
        })
    }
}
