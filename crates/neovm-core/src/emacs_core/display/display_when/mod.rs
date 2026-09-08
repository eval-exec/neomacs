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
use crate::emacs_core::display_spec::{DisplayPropertySpecs, display_spec_when_parts};
use crate::emacs_core::error::Flow;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;
use crate::window::{Window, WindowId};
use rustc_hash::FxHashMap;
use std::ops::ControlFlow;

/// A display property containing `when` clauses, with GNU's occurrence
/// bindings. Keep the entire property so a string's first replacement can
/// stop evaluation of the remaining clauses (xdisp.c:6034-6040).
#[derive(Clone, Copy, Debug)]
pub struct DisplayWhenSite {
    pub property: Value,
    /// The buffer or string carrying the property.
    pub object: Value,
    /// The position in `object` (GNU `position`).
    pub position: i64,
    /// The buffer position being displayed (GNU `buffer-position`).
    pub buffer_position: i64,
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

    /// Evaluate reachable, enabled FORMs with the bindings of their first
    /// occurrence. The caller supplies its single-spec replacement classifier
    /// so string evaluation and rendering stop on the same winning clause.
    ///
    /// Snapshot and root every property's elements before running Lisp. A
    /// FORM can mutate the list/vector or remove a later site's property and
    /// collect; neither may invalidate our iteration or its saved elements.
    /// Once-per-`equal`-FORM evaluation remains a declared difference from GNU.
    /// `Err` carries non-signal flows, which GNU lets unwind through redisplay.
    pub fn evaluate_display_when_sites(
        &mut self,
        buf_id: BufferId,
        window_id: Option<WindowId>,
        sites: &[DisplayWhenSite],
        mut replaces_text: impl FnMut(Value) -> bool,
    ) -> Result<FxHashMap<Value, bool>, Flow> {
        let root_scope = self.save_specpdl_roots();
        let mut properties = Vec::with_capacity(sites.len());
        for site in sites {
            let property = DisplayPropertySpecs::of(site.property);
            let mut specs = Vec::new();
            property.for_each(|spec| {
                specs.push(spec);
                ControlFlow::Continue(())
            });
            for value in [site.property, site.object]
                .into_iter()
                .chain(specs.iter().copied())
            {
                if value.is_heap_object() {
                    self.push_specpdl_root(value);
                }
            }
            properties.push((property.eval_enabled, specs));
        }
        let evaluated = self.with_display_buffer_current(buf_id, |ctx| {
            let mut results: FxHashMap<Value, bool> = FxHashMap::default();
            // GNU saves a numeric character position, not an edit-tracking
            // marker (xdisp.c:21585-21611). Recompute its byte position after
            // Lisp has run, including when Lisp exits through a nonlocal flow.
            let window_point = window_id.and_then(|id| {
                if ctx
                    .frames
                    .selected_frame()
                    .map(|frame| frame.selected_window)
                    == Some(id)
                {
                    return None;
                }
                let frame = ctx.frames.get(ctx.frames.find_window_frame_id(id)?)?;
                match frame.find_window(id)? {
                    Window::Leaf {
                        buffer_id, point, ..
                    } if *buffer_id == buf_id => Some(*point),
                    _ => None,
                }
            });
            let saved_point = window_point.and_then(|point| {
                let buffer = ctx.buffers.get_mut(buf_id)?;
                let saved = buffer.point_char_pos();
                let target = buffer.char_pos_to_emacs_byte_pos_clamped(point.to_char_pos());
                buffer.goto_emacs_byte_pos(target);
                Some(saved)
            });
            let outcome = ctx.with_unwind_scope(|ctx| {
                for (site, (eval_enabled, specs)) in sites.iter().zip(properties) {
                    for spec in specs {
                        let resolved = if let Some((form, inner)) = display_spec_when_parts(spec) {
                            // Earlier clauses may have installed this pair
                            // after the snapshot. Root the values we actually
                            // decoded: FORM can detach its own cdr and collect,
                            // and the cached key must survive later clauses.
                            for value in [form, inner] {
                                if value.is_heap_object() {
                                    ctx.push_specpdl_root(value);
                                }
                            }
                            if form.is_nil() {
                                continue;
                            }
                            if !form.is_symbol_named("t") {
                                // Disabled sites have no cached answer: the policy
                                // belongs to this property, not every equal FORM.
                                if !eval_enabled {
                                    continue;
                                }
                                let holds = if let Some(holds) = results.get(&form) {
                                    *holds
                                } else {
                                    let holds = ctx.display_when_form_holds(
                                        form,
                                        site.object,
                                        site.position,
                                        site.buffer_position,
                                    )?;
                                    results.insert(form, holds);
                                    holds
                                };
                                if !holds {
                                    continue;
                                }
                            }
                            inner
                        } else {
                            spec
                        };
                        if site.object.is_string() && replaces_text(resolved) {
                            break;
                        }
                    }
                }
                Ok(Value::NIL)
            });
            if let Some(point) = saved_point {
                ctx.restore_current_buffer_if_live(buf_id);
                if let Some(buffer) = ctx.buffers.get_mut(buf_id) {
                    let target = buffer.char_pos_to_emacs_byte_pos_clamped(point);
                    buffer.goto_emacs_byte_pos(target);
                }
            }
            outcome?;
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
    /// here), `dsafe_eval_handler`'s "Error during redisplay" line in
    /// `*Messages*` (src/xdisp.c:3098-3104; the error is logged instead),
    /// the abort of a nested redisplay through `Ftop_level`
    /// (src/xdisp.c:3127-3136; `inhibit-redisplay` is bound, so none can
    /// start), and `backtrace-on-redisplay-error` (src/xdisp.c:3159-3160).
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
#[path = "tests/mod.rs"]
mod tests;
