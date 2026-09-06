//! The FORM of a `(when FORM . SPEC)` display spec.
//!
//! GNU `handle_single_display_spec` (src/xdisp.c:6130-6164, emacs-31.0.90)
//! evaluates FORM before it looks at SPEC: `nil` and `t` are taken as they
//! are, anything else runs through `dsafe_eval` with `object` bound to the
//! buffer or string carrying the property, `position` to the position in
//! that object, and `buffer-position` to the buffer position being
//! displayed (src/xdisp.c:6152-6154); an error makes the spec inapplicable,
//! like a nil result.  The layout engine collects the forms of a window's
//! span before it walks the window (it cannot run Lisp while it holds the
//! buffer) and evaluates them here, so this is the one place that
//! evaluation lives.

use super::eval::Context;
use crate::buffer::BufferId;
use crate::emacs_core::error::Flow;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;
use rustc_hash::FxHashMap;

/// One `(when FORM . SPEC)` occurrence with the bindings GNU gives its FORM
/// (src/xdisp.c:6152-6154).
#[derive(Clone, Copy, Debug)]
pub struct DisplayWhenSite {
    pub form: Value,
    /// The buffer or string carrying the property.
    pub object: Value,
    /// The position in `object` (GNU `position`).
    pub position: i64,
    /// The buffer position being displayed (GNU `buffer-position`).
    pub buffer_position: i64,
    /// False inside `(disable-eval …)`: GNU then takes FORM as nil
    /// (src/xdisp.c:6139-6140) and the spec never applies.
    pub eval_enabled: bool,
}

impl Context {
    /// Run `f` with `buf_id` current, the way GNU's display iterator runs
    /// with the window's buffer selected ("Really select the buffer, for the
    /// sake of buffer-local variables", src/xdisp.c:20533-20535), and put the
    /// caller's buffer back afterwards.  `Err` when `buf_id` is not live.
    /// If `f` killed the caller's buffer, the window's buffer stays current
    /// (there is nothing to restore), which is what `Fkill_buffer` leaves
    /// behind in GNU too.
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

    /// Evaluate every distinct FORM of `sites` with `buf_id` current, each
    /// with the bindings of its first occurrence, and return what each held.
    ///
    /// The site values are rooted for the span of the loop: a FORM may drop
    /// the property that made a later site's FORM reachable, and the
    /// collector is precise (no stack scan).  A FORM is evaluated once; GNU
    /// evaluates it at every occurrence (declared).  `Err` carries a flow
    /// that is not an error signal -- a `throw`, a thread block, an exit --
    /// which GNU lets unwind through redisplay.
    pub fn evaluate_display_when_sites(
        &mut self,
        buf_id: BufferId,
        sites: &[DisplayWhenSite],
    ) -> Result<FxHashMap<Value, bool>, Flow> {
        let root_scope = self.save_specpdl_roots();
        for site in sites {
            for value in [site.form, site.object] {
                if value.is_heap_object() {
                    self.push_specpdl_root(value);
                }
            }
        }
        let evaluated = self.with_display_buffer_current(buf_id, |ctx| {
            let mut results: FxHashMap<Value, bool> = FxHashMap::default();
            for site in sites {
                if results.contains_key(&site.form) {
                    continue;
                }
                let holds = if site.eval_enabled {
                    ctx.display_when_form_holds(
                        site.form,
                        site.object,
                        site.position,
                        site.buffer_position,
                    )?
                } else {
                    false
                };
                results.insert(site.form, holds);
            }
            Ok(results)
        });
        self.restore_specpdl_roots(root_scope);
        evaluated?
    }

    /// Whether the `(when FORM . SPEC)` display spec whose FORM this is
    /// applies at `position` of `object` while displaying `buffer_position`.
    ///
    /// GNU `dsafe_eval` is `dsafe_calln (true, Qeval, sexpr, Qt)`
    /// (src/xdisp.c:3170-3173): `inhibit-redisplay` and `inhibit-quit` are
    /// bound around a lexical `eval` and any error signal yields nil
    /// (src/xdisp.c:3118-3131).  The port's `safe_funcall` shape: the same
    /// bindings plus `inhibit-debugger` in place of GNU's catch-all
    /// condition handler, an error signal muted to `Ok(false)`, every other
    /// flow (a `throw`, a thread block, an exit) returned to the caller as
    /// GNU lets it unwind.  Declared, not ported: GNU's
    /// `inhibit-eval-during-redisplay` short-circuit (no such variable
    /// here) and `dsafe_eval_handler`'s "Error during redisplay" line in
    /// `*Messages*` (src/xdisp.c:3098-3104); the error is logged instead.
    pub fn display_when_form_holds(
        &mut self,
        form: Value,
        object: Value,
        position: i64,
        buffer_position: i64,
    ) -> Result<bool, Flow> {
        if form.is_nil() {
            return Ok(false);
        }
        if form.is_symbol_named("t") {
            return Ok(true);
        }
        let count = self.specpdl.len();
        let result = (|| {
            for (symbol, value) in [
                (intern("inhibit-redisplay"), Value::T),
                (intern("inhibit-quit"), Value::T),
                (intern("inhibit-debugger"), Value::T),
                (intern("object"), object),
                (intern("position"), Value::fixnum(position)),
                (intern("buffer-position"), Value::fixnum(buffer_position)),
            ] {
                self.try_specbind_or_unwind_to(count, symbol, value)?;
            }
            self.funcall_general(Value::symbol("eval"), vec![form, Value::T])
        })();
        match self.unbind_to_with_result(count, result) {
            Ok(value) => Ok(!value.is_nil()),
            Err(Flow::Signal(signal)) => {
                tracing::debug!(?signal, "display `when' form signaled; treated as nil");
                Ok(false)
            }
            Err(flow) => Err(flow),
        }
    }
}

#[cfg(test)]
#[path = "display_when_test.rs"]
mod tests;
