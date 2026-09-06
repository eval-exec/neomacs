//! The FORM of a `(when FORM . SPEC)` display spec.
//!
//! GNU `handle_single_display_spec` (src/xdisp.c:6130-6160, emacs-31.0.90)
//! evaluates FORM before it looks at SPEC: `nil` and `t` are taken as they
//! are, anything else runs through `dsafe_eval` with `object` bound to the
//! buffer or string carrying the property, `position` to the position in
//! that object, and `buffer-position` to the buffer position being
//! displayed; an error makes the spec inapplicable, like a nil result.  The
//! layout engine asks for these results before it walks a window (it cannot
//! run Lisp while it holds the buffer), so this is the one place that
//! evaluation lives.

use super::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;

impl Context {
    /// Whether the `(when FORM . SPEC)` display spec whose FORM this is
    /// applies at `position` of `object` while displaying `buffer_position`.
    ///
    /// Mirrors src/xdisp.c:6141-6157: the three bindings are dynamic
    /// (`specbind`), FORM is evaluated with errors treated as nil, and the
    /// bindings are undone afterwards whatever happened.
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
        let bound = self
            .try_specbind(intern("object"), object)
            .and_then(|()| self.try_specbind(intern("position"), Value::fixnum(position)))
            .and_then(|()| {
                self.try_specbind(intern("buffer-position"), Value::fixnum(buffer_position))
            });
        let holds = match bound {
            Ok(()) => matches!(self.eval_form(form), Ok(value) if !value.is_nil()),
            Err(_) => false,
        };
        self.unbind_to(count);
        holds
    }
}

#[cfg(test)]
#[path = "display_when_test.rs"]
mod tests;
