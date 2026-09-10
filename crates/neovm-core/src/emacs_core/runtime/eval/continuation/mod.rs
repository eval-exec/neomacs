//! Explicit continuations for interpreted call evaluation.
//!
//! Lisp values retained between steps live on Context's traced operand stack
//! or specpdl, never solely in this Rust Vec. Continuations store stack indices
//! and linear binding-cleanup ownership. The Context remains on its VM thread.

use super::apply::ActiveInterpretedLambdaCall;
use super::special_forms::{ActiveCleanupScope, ActiveLetScope, ConditionalForms};
use super::*;

pub(super) enum PreparedForm {
    Value(Value),
    Call(PreparedCall),
    LetBody {
        body: Value,
        scope: ActiveLetScope,
    },
    Conditional(ConditionalForms),
    ProtectedBody {
        body: Value,
        scope: ActiveCleanupScope,
    },
}

pub(super) struct PreparedCall {
    pub(super) function: Value,
    pub(super) arguments: Value,
    pub(super) target: CallTarget,
}

/// Freeze the resolved subr entry before evaluating arguments: Lisp may
/// redefine its function cell while those arguments are being evaluated.
pub(super) enum CallTarget {
    Subr { sym_id: SymId, entry: SubrEntry },
    Function,
}

enum Step {
    Eval(Value),
    Sequence(Value),
    Return(EvalResult),
    Invoke {
        function: usize,
        first_arg: usize,
        target: CallTarget,
    },
}

enum Continuation {
    Form {
        specpdl: usize,
        operands: usize,
    },
    Arguments {
        cursor: usize,
        function: usize,
        first_arg: usize,
        target: CallTarget,
        backtrace: usize,
    },
    Body {
        cursor: usize,
    },
    Lambda {
        call: ActiveInterpretedLambdaCall,
        operands: usize,
    },
    Let {
        scope: ActiveLetScope,
    },
    Conditional {
        branches: usize,
    },
    Cleanup {
        scope: ActiveCleanupScope,
    },
    SequenceScope {
        sequence: SequenceTempRootScopeState,
        operands: usize,
    },
    Sequence {
        cursor: usize,
    },
}

/// Re-entrant evaluator entries borrow separate storage. Returning it empty
/// avoids retaining any Lisp values, and avoids an allocation per small form.
#[derive(Default)]
pub(crate) struct EvaluationStackPool {
    free: Vec<Vec<Continuation>>,
}

impl EvaluationStackPool {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn take(&mut self) -> Vec<Continuation> {
        self.free.pop().unwrap_or_default()
    }

    fn recycle(&mut self, mut stack: Vec<Continuation>) {
        stack.clear();
        // As with the bytecode interpreter pool, bound retained re-entrant
        // entries without limiting how deeply Lisp may execute.
        if self.free.len() < 64 {
            self.free.push(stack);
        }
    }
}

impl Context {
    pub(super) fn eval_with_continuations(&mut self, form: Value) -> EvalResult {
        let mut continuations = self.evaluation_stacks.take();
        let result = self.run_eval_continuations(form, &mut continuations);
        self.evaluation_stacks.recycle(continuations);
        result
    }

    fn run_eval_continuations(
        &mut self,
        form: Value,
        continuations: &mut Vec<Continuation>,
    ) -> EvalResult {
        let mut step = Step::Eval(form);
        loop {
            step = match step {
                Step::Sequence(body) => {
                    let cursor = self.bc_buf.len();
                    self.bc_buf.push(body);
                    let sequence = self.save_sequence_temp_roots();
                    continuations.push(Continuation::SequenceScope {
                        sequence,
                        operands: cursor,
                    });
                    if body.is_cons() {
                        continuations.push(Continuation::Sequence { cursor });
                        Step::Eval(body.cons_car())
                    } else if body.is_nil() {
                        Step::Return(Ok(Value::NIL))
                    } else {
                        Step::Return(Err(self.listp_error(body)))
                    }
                }
                Step::Eval(form) => {
                    let unwrapped = self.unwrap_symbol(form);
                    if let Some(sym_id) = unwrapped.as_symbol_id() {
                        let result = self.eval_symbol_by_id(sym_id);
                        Step::Return(self.dispatch_signal_result_if_needed(result))
                    } else if !unwrapped.is_cons() {
                        Step::Return(Ok(unwrapped))
                    } else if let Err(flow) = self.enter_interpreted_eval_depth() {
                        Step::Return(Err(flow))
                    } else if let Err(flow) = self.maybe_quit_before_gc() {
                        self.depth -= 1;
                        Step::Return(Err(flow))
                    } else {
                        if self.gc_safe_point_exact_should_collect() {
                            self.collect_at_eval_safe_point(form);
                        }
                        let original_fun = self.unwrap_symbol(form.cons_car());
                        let original_args = form.cons_cdr();
                        let backtrace = self.specpdl.len();
                        continuations.push(Continuation::Form {
                            specpdl: backtrace,
                            operands: self.bc_buf.len(),
                        });
                        self.push_unevalled_backtrace_frame(original_fun, original_args);
                        let prepared = match self.take_debug_on_call_arm(DebugOnCallCode::EvalForm)
                        {
                            Some(arm) => self.do_debug_on_call(arm).and_then(|()| {
                                self.prepare_eval_sub_cons_dispatch(original_fun, original_args)
                            }),
                            None => {
                                self.prepare_eval_sub_cons_dispatch(original_fun, original_args)
                            }
                        };
                        match prepared {
                            Err(flow) => Step::Return(Err(flow)),
                            Ok(PreparedForm::Value(value)) => Step::Return(Ok(value)),
                            Ok(PreparedForm::ProtectedBody { body, scope }) => {
                                continuations.push(Continuation::Cleanup { scope });
                                Step::Eval(body)
                            }
                            Ok(PreparedForm::Conditional(forms)) => {
                                let branches = self.bc_buf.len();
                                self.bc_buf.push(forms.then_form);
                                self.bc_buf.push(forms.else_forms);
                                continuations.push(Continuation::Conditional { branches });
                                Step::Eval(forms.condition)
                            }
                            Ok(PreparedForm::LetBody { body, scope }) => {
                                continuations.push(Continuation::Let { scope });
                                Step::Sequence(body)
                            }
                            Ok(PreparedForm::Call(call)) => {
                                let cursor = self.bc_buf.len();
                                self.bc_buf.push(call.arguments);
                                let function = self.bc_buf.len();
                                self.bc_buf.push(call.function);
                                let first_arg = self.bc_buf.len();
                                if call.arguments.is_cons() {
                                    continuations.push(Continuation::Arguments {
                                        cursor,
                                        function,
                                        first_arg,
                                        target: call.target,
                                        backtrace,
                                    });
                                    Step::Eval(call.arguments.cons_car())
                                } else {
                                    self.set_backtrace_args_evalled_bc_span(
                                        backtrace, first_arg, 0,
                                    );
                                    Step::Invoke {
                                        function,
                                        first_arg,
                                        target: call.target,
                                    }
                                }
                            }
                        }
                    }
                }
                Step::Invoke {
                    function,
                    first_arg,
                    target,
                } => {
                    let function = self.bc_buf[function];
                    let nargs = self.bc_buf.len() - first_arg;
                    match target {
                        CallTarget::Function if is_interpreted_lambda(function) => {
                            let args = LispArgVec::from_slice(&self.bc_buf[first_arg..]);
                            match self.begin_interpreted_lambda(function, &args) {
                                Err(flow) => Step::Return(Err(flow)),
                                Ok(call) => {
                                    let body = call.body;
                                    let cursor = self.bc_buf.len();
                                    self.bc_buf.push(body);
                                    continuations.push(Continuation::Lambda {
                                        call,
                                        operands: cursor,
                                    });
                                    if body.is_cons() {
                                        continuations.push(Continuation::Body { cursor });
                                        Step::Eval(body.cons_car())
                                    } else {
                                        Step::Return(Ok(Value::NIL))
                                    }
                                }
                            }
                        }
                        target => Step::Return(self.maybe_grow_eval_stack(|ctx| {
                            ctx.invoke_prepared_call(target, function, first_arg, nargs)
                        })),
                    }
                }
                Step::Return(result) => {
                    let Some(continuation) = continuations.pop() else {
                        return result;
                    };
                    match continuation {
                        Continuation::Form { specpdl, operands } => {
                            let result = self.dispatch_signal_result_if_needed(result);
                            self.record_sequence_call_roots(specpdl);
                            let result = self.unbind_to_with_result(specpdl, result);
                            self.bc_buf.truncate(operands);
                            self.depth -= 1;
                            Step::Return(result)
                        }
                        Continuation::Arguments {
                            cursor,
                            function,
                            first_arg,
                            target,
                            backtrace,
                        } => match result {
                            Err(flow) => Step::Return(Err(flow)),
                            Ok(value) => {
                                self.bc_buf.push(value);
                                let remaining = self.bc_buf[cursor].cons_cdr();
                                self.bc_buf[cursor] = remaining;
                                if remaining.is_cons() {
                                    continuations.push(Continuation::Arguments {
                                        cursor,
                                        function,
                                        first_arg,
                                        target,
                                        backtrace,
                                    });
                                    Step::Eval(remaining.cons_car())
                                } else if !remaining.is_nil() {
                                    Step::Return(Err(self.listp_error(remaining)))
                                } else {
                                    self.set_backtrace_args_evalled_bc_span(
                                        backtrace,
                                        first_arg,
                                        self.bc_buf.len() - first_arg,
                                    );
                                    Step::Invoke {
                                        function,
                                        first_arg,
                                        target,
                                    }
                                }
                            }
                        },
                        Continuation::Body { cursor } => match result {
                            Ok(value) => {
                                let remaining = self.bc_buf[cursor].cons_cdr();
                                self.bc_buf[cursor] = remaining;
                                if remaining.is_cons() {
                                    continuations.push(Continuation::Body { cursor });
                                    Step::Eval(remaining.cons_car())
                                } else {
                                    Step::Return(Ok(value))
                                }
                            }
                            Err(Flow::ThreadBlocked(blocked)) => {
                                let remaining = if blocked.remaining_forms.is_nil() {
                                    self.bc_buf[cursor].cons_cdr()
                                } else {
                                    blocked.remaining_forms
                                };
                                Step::Return(Err(Flow::thread_blocked(blocked.blocker, remaining)))
                            }
                            Err(flow) => Step::Return(Err(flow)),
                        },
                        Continuation::Lambda { call, operands } => {
                            let result = self.finish_interpreted_lambda(call, result);
                            self.bc_buf.truncate(operands);
                            Step::Return(result)
                        }
                        Continuation::Sequence { cursor } => match result {
                            Ok(value) => {
                                let remaining = self.bc_buf[cursor].cons_cdr();
                                self.bc_buf[cursor] = remaining;
                                if remaining.is_cons() {
                                    continuations.push(Continuation::Sequence { cursor });
                                    Step::Eval(remaining.cons_car())
                                } else if remaining.is_nil() {
                                    Step::Return(Ok(value))
                                } else {
                                    Step::Return(Err(self.listp_error(remaining)))
                                }
                            }
                            Err(Flow::ThreadBlocked(blocked)) => {
                                let remaining = if blocked.remaining_forms.is_nil() {
                                    self.bc_buf[cursor].cons_cdr()
                                } else {
                                    blocked.remaining_forms
                                };
                                Step::Return(Err(Flow::thread_blocked(blocked.blocker, remaining)))
                            }
                            Err(flow) => Step::Return(Err(flow)),
                        },
                        Continuation::Conditional { branches } => match result {
                            Ok(value) if value.is_truthy() => Step::Eval(self.bc_buf[branches]),
                            Ok(_) => Step::Sequence(self.bc_buf[branches + 1]),
                            Err(flow) => Step::Return(Err(flow)),
                        },
                        Continuation::Cleanup { scope } => {
                            Step::Return(self.finish_cleanup_scope(scope, result))
                        }
                        Continuation::Let { scope } => {
                            Step::Return(self.finish_let_scope(scope, result))
                        }
                        Continuation::SequenceScope { sequence, operands } => {
                            self.restore_sequence_temp_roots(sequence);
                            self.bc_buf.truncate(operands);
                            Step::Return(result)
                        }
                    }
                }
            };
        }
    }

    fn invoke_prepared_call(
        &mut self,
        target: CallTarget,
        function: Value,
        first_arg: usize,
        nargs: usize,
    ) -> EvalResult {
        match target {
            CallTarget::Subr { sym_id, entry } => {
                let result = if Self::subr_entry_uses_fixed_value_call(entry) {
                    self.dispatch_subr_entry_from_bc_stack(entry, first_arg, nargs)
                } else {
                    let args = LispArgVec::from_slice(&self.bc_buf[first_arg..first_arg + nargs]);
                    if entry.dispatch_kind == SubrDispatchKind::ContextCallable {
                        return self.apply_evaluator_callable_by_id(sym_id, args);
                    }
                    self.dispatch_subr_entry_unchecked(entry, args)
                };
                result.unwrap_or_else(|| {
                    Err(signal(
                        LispCondition::VoidFunction,
                        vec![Value::from_sym_id(sym_id)],
                    ))
                })
            }
            CallTarget::Function => {
                if let Some(bc_data) = function.get_bytecode_data() {
                    self.execute_bytecode_call_from_stack(bc_data, first_arg, nargs, function)
                } else {
                    let args = LispArgVec::from_slice(&self.bc_buf[first_arg..first_arg + nargs]);
                    self.funcall_general_untraced(function, args)
                }
            }
        }
    }
}

fn is_interpreted_lambda(function: Value) -> bool {
    matches!(
        function.kind(),
        ValueKind::Veclike(VecLikeType::Lambda | VecLikeType::Macro)
    ) || (function.is_cons() && cons_head_symbol_id(&function) == Some(lambda_symbol()))
}
