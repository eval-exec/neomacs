//! Missing GNU core defaults at the portable-image construction boundary.

use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::Value;

/// Restore only an omitted C-owned declaration when the GNU policy is active.
///
/// GNU window.c:9383–9398 initializes this DEFVAR_LISP default before Lisp.
/// A legacy image can lack its symbol slot; existing cells are Lisp-owned
/// image state and must survive, including localized, alias and unbound
/// cells. A present plain-unbound cell cannot be distinguished from an
/// intentional legacy `makunbound`, so it is deliberately preserved.
///
/// The caller exclusively constructs one Context before Lisp dispatch. This
/// helper adds no shared Lisp state, cache or single-mutator assumption. Its
/// caller's final runtime activation adopts the usual GNU object forwarder.
#[cold]
#[inline(never)]
pub(crate) fn restore_gnu_configuration_hook_default(obarray: &mut Obarray) {
    if !crate::emacs_core::eval::gnu_redisplay_hooks_enabled() {
        return;
    }
    let id = crate::emacs_core::intern::WellKnown::WindowConfigurationChangeHook.id();
    // A legacy image dumped before the port seeded GNU's initial DEFSYM
    // membership lacks the symbol slot entirely; any present slot (a plain
    // value including an intentional legacy `makunbound`, a forwarder, or a
    // localized cell) is Lisp-owned image state and must survive.
    if obarray.get_by_id(id).is_some() {
        return;
    }
    obarray.define_special_variable("window-configuration-change-hook", Value::NIL);
}
