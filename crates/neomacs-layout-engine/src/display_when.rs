//! Evaluated `(when FORM . SPEC)` display-spec conditions for one window walk.
//!
//! GNU evaluates FORM inline while it walks the text
//! (`handle_single_display_spec`, src/xdisp.c:6130-6160, emacs-31.0.90).
//! This engine cannot run Lisp while it holds the buffer for a walk, so the
//! forms in the window's span are evaluated once, before the walk, through
//! `Context::display_when_form_holds`, and the results ride along in the
//! `LayoutBufferSnapshot` the walk reads from.  The classifier then asks
//! [`DisplayWhenConditions::holds`] instead of assuming every non-nil FORM
//! is true, which is what made dashboard's `(when (not (display-graphic-p))
//! …)` clause win on a graphic frame.

use std::rc::Rc;

use neovm_core::buffer::{Buffer, BufferId, CharPos0, EmacsByteRange};
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::display_spec::{DisplayPropertySpecs, display_spec_when_parts};
use neovm_core::emacs_core::value::{Value, get_string_text_properties_table_for_value};
use rustc_hash::FxHashMap;

use crate::neovm_bridge::LayoutBufferView;

/// The evaluated conditions a walk consults.
///
/// `structural()` is the state of a reader with no evaluation behind it (a
/// synchronous query, a test, a chrome string): every non-nil FORM holds,
/// which is the pre-evaluation behaviour and GNU's `enable_eval_p` fallback
/// inverted -- declared, not GNU.  Once evaluated, a FORM the scan did not
/// see (an object the scan does not cover) also falls back to that.
#[derive(Clone, Debug, Default)]
pub struct DisplayWhenConditions {
    evaluated: Option<Rc<FxHashMap<Value, bool>>>,
}

impl DisplayWhenConditions {
    pub fn structural() -> Self {
        Self::default()
    }

    pub fn evaluated(results: FxHashMap<Value, bool>) -> Self {
        Self {
            evaluated: Some(Rc::new(results)),
        }
    }

    /// Whether a `(when FORM . SPEC)` spec applies: `nil` never, `t` always
    /// (GNU takes both literally, src/xdisp.c:6141), anything else by its
    /// evaluated result.
    pub fn holds(&self, form: Value) -> bool {
        if form.is_nil() {
            return false;
        }
        if form.is_symbol_named("t") {
            return true;
        }
        match &self.evaluated {
            Some(results) => results.get(&form).copied().unwrap_or(true),
            None => true,
        }
    }
}

/// One FORM occurrence with the bindings GNU gives it (src/xdisp.c:6147-6149).
struct WhenFormSite {
    form: Value,
    object: Value,
    position: i64,
    buffer_position: i64,
}

/// Evaluate the `when` forms of every display spec the walk of
/// `[from, to)` of `buf_id` can reach: `display`, `line-prefix` and
/// `wrap-prefix` text properties and overlay properties, the buffer-local
/// `line-prefix`/`wrap-prefix` values, and the `display` properties inside
/// those prefix strings.
///
/// A FORM is evaluated once, with the bindings of its first occurrence;
/// GNU evaluates it at every occurrence, so a FORM that reads `position`
/// or `buffer-position` and expects a different answer per occurrence is
/// not reproduced (declared).
pub(crate) fn evaluate_window_display_when_forms(
    evaluator: &mut Context,
    buf_id: BufferId,
    from: CharPos0,
    to: CharPos0,
) -> DisplayWhenConditions {
    let sites = match evaluator.buffer_manager().get(buf_id) {
        Some(buffer) => collect_when_form_sites(buffer, from, to),
        None => Vec::new(),
    };
    let mut results = FxHashMap::default();
    for site in sites {
        if results.contains_key(&site.form) {
            continue;
        }
        let holds = evaluator.display_when_form_holds(
            site.form,
            site.object,
            site.position,
            site.buffer_position,
        );
        results.insert(site.form, holds);
    }
    DisplayWhenConditions::evaluated(results)
}

fn collect_when_form_sites(buffer: &Buffer, from: CharPos0, to: CharPos0) -> Vec<WhenFormSite> {
    let mut sites = Vec::new();
    let buffer_object = Value::make_buffer(buffer.id());
    let to = to.min(buffer.layout_point_max_char_pos()).max(from);
    let gnu_pos = |pos: CharPos0| pos.get() as i64 + 1;

    for name in ["display", "line-prefix", "wrap-prefix"] {
        let name_value = Value::symbol(name);
        let mut pos = from;
        while pos < to {
            let bytepos = buffer.layout_char_pos_to_emacs_byte_pos(pos);
            if let Some(value) = buffer.layout_text_prop_at_emacs_byte_pos(bytepos, name_value) {
                if name == "display" {
                    collect_when_forms(
                        value,
                        buffer_object,
                        gnu_pos(pos),
                        gnu_pos(pos),
                        &mut sites,
                    );
                } else {
                    collect_prefix_string_when_forms(value, gnu_pos(pos), &mut sites);
                }
            }
            let next = buffer.next_watched_property_change_at_char_pos(pos, to, &[name_value]);
            if next <= pos {
                break;
            }
            pos = next;
        }
    }

    let overlays = buffer.overlays();
    let range = EmacsByteRange::new(
        buffer.layout_char_pos_to_emacs_byte_pos(from),
        buffer.layout_char_pos_to_emacs_byte_pos(to),
    );
    for overlay in overlays.overlays_in_emacs_byte_range(range) {
        let start = overlays
            .overlay_start_emacs_byte_pos(overlay)
            .map(|bytepos| buffer.layout_emacs_byte_pos_to_char_pos(bytepos))
            .unwrap_or(from)
            .max(from);
        if let Some(value) = overlays.overlay_get_named(overlay, Value::symbol("display")) {
            collect_when_forms(
                value,
                buffer_object,
                gnu_pos(start),
                gnu_pos(start),
                &mut sites,
            );
        }
        for name in ["line-prefix", "wrap-prefix"] {
            if let Some(value) = overlays.overlay_get_named(overlay, Value::symbol(name)) {
                collect_prefix_string_when_forms(value, gnu_pos(start), &mut sites);
            }
        }
    }

    for name in ["line-prefix", "wrap-prefix"] {
        if let Some(value) = buffer.buffer_local_value(name) {
            collect_prefix_string_when_forms(value, gnu_pos(from), &mut sites);
        }
    }
    sites
}

/// The `when` forms of one display property value.
fn collect_when_forms(
    value: Value,
    object: Value,
    position: i64,
    buffer_position: i64,
    sites: &mut Vec<WhenFormSite>,
) {
    let specs = DisplayPropertySpecs::of(value);
    specs.for_each(|spec| {
        if let Some((form, _)) = display_spec_when_parts(spec)
            && !form.is_nil()
            && !form.is_symbol_named("t")
        {
            // `(disable-eval SPEC)`: GNU takes FORM as nil (src/xdisp.c:6143-6144).
            sites.push(WhenFormSite {
                form: if specs.eval_enabled { form } else { Value::NIL },
                object,
                position,
                buffer_position,
            });
        }
        std::ops::ControlFlow::Continue(())
    });
}

/// The `when` forms inside a prefix STRING's own `display` properties, with
/// `object` bound to the string and `position` to the index in it.
fn collect_prefix_string_when_forms(
    string: Value,
    buffer_position: i64,
    sites: &mut Vec<WhenFormSite>,
) {
    let Some(lisp_string) = string.as_lisp_string() else {
        return;
    };
    let Some(props) = get_string_text_properties_table_for_value(string) else {
        return;
    };
    let display = Value::symbol("display");
    let mut previous: Option<Value> = None;
    for index in 0..lisp_string.schars() {
        let value = props.get_property_at_char_pos(CharPos0::new(index), display);
        if value == previous {
            continue;
        }
        if let Some(value) = value {
            collect_when_forms(value, string, index as i64, buffer_position, sites);
        }
        previous = value;
    }
}

#[cfg(test)]
#[path = "display_when_test.rs"]
mod tests;
