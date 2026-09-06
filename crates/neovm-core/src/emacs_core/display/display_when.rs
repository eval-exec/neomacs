//! The FORM of a `(when FORM . SPEC)` display spec.
//!
//! GNU `handle_single_display_spec` (src/xdisp.c:6130-6164, emacs-31.0.90)
//! evaluates FORM before it looks at SPEC: `nil` and `t` are taken as they
//! are, anything else runs through `dsafe_eval` with `object` bound to the
//! buffer or string carrying the property, `position` to the position in
//! that object, and `buffer-position` to the buffer position being
//! displayed (src/xdisp.c:6152-6154); an error makes the spec inapplicable,
//! like a nil result.  The layout engine asks for these results before it
//! walks a window (it cannot run Lisp while it holds the buffer), so this is
//! the one place that evaluation lives.

use super::eval::Context;
use crate::buffer::BufferId;
use crate::emacs_core::error::Flow;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;

impl Context {
    /// Run `f` with `buf_id` current, the way GNU's display iterator runs
    /// with the window's buffer selected ("Really select the buffer, for the
    /// sake of buffer-local variables", src/xdisp.c:20533-20535), and put the
    /// caller's buffer back afterwards.  `Err` when `buf_id` is not live.
    pub fn with_display_buffer_current<T>(
        &mut self,
        buf_id: BufferId,
        f: impl FnOnce(&mut Self) -> T,
    ) -> Result<T, Flow> {
        let saved = self.buffers.current_buffer_id();
        if saved != Some(buf_id) {
            self.set_current_buffer_unrecorded(buf_id)?;
        }
        let result = f(self);
        if let Some(saved) = saved {
            self.restore_current_buffer_if_live(saved);
        }
        Ok(result)
    }

    /// Whether the `(when FORM . SPEC)` display spec whose FORM this is
    /// applies at `position` of `object` while displaying `buffer_position`.
    ///
    /// GNU `dsafe_eval` is `dsafe_calln (true, Qeval, sexpr, Qt)`
    /// (src/xdisp.c:3170-3173): `inhibit-redisplay` and `inhibit-quit` are
    /// bound around a lexical `eval` and any error yields nil
    /// (src/xdisp.c:3118-3131).  The same binding pattern the port uses for
    /// its other `dsafe_call` sites (`set-message-function`,
    /// `fontification-functions`).  Declared, not ported: GNU's
    /// `inhibit-eval-during-redisplay` short-circuit (no such variable here)
    /// and the `internal_condition_case_n (…, Qt, …)` catch-all that keeps
    /// the debugger out of an erroring FORM (`debug-on-error` still enters
    /// it, as with the other `dsafe_call` ports).
    pub fn display_when_form_holds(
        &mut self,
        form: Value,
        object: Value,
        position: i64,
        buffer_position: i64,
    ) -> bool {
        if form.is_nil() {
            return false;
        }
        if form.is_symbol_named("t") {
            return true;
        }
        let count = self.specpdl.len();
        let bindings = [
            (intern("inhibit-redisplay"), Value::T),
            (intern("inhibit-quit"), Value::T),
            (intern("object"), object),
            (intern("position"), Value::fixnum(position)),
            (intern("buffer-position"), Value::fixnum(buffer_position)),
        ];
        for (symbol, value) in bindings {
            if let Err(flow) = self.try_specbind_or_unwind_to(count, symbol, value) {
                tracing::debug!(
                    "display `when' form: could not bind its environment: {:?}",
                    flow
                );
                return false;
            }
        }
        let result = self.funcall_general(Value::symbol("eval"), vec![form, Value::T]);
        match self.unbind_to_with_result(count, result) {
            Ok(value) => !value.is_nil(),
            Err(flow) => {
                tracing::debug!("display `when' form signaled; treated as nil: {:?}", flow);
                false
            }
        }
    }
}

#[cfg(test)]
#[path = "display_when_test.rs"]
mod tests;
