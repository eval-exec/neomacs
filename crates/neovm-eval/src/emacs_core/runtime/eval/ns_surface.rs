//! The NS C surface: the `DEFVAR_LISP` names `src/nsterm.m`'s
//! `syms_of_nsterm` installs before Lisp loadup on the macOS reference build
//! (GNU 31.1, `--with-ns --without-x`).
//!
//! The BOOL half of the same family (`ns-use-native-fullscreen`,
//! `ns-use-srgb-colorspace`, `ns-click-through`, `ns-use-fullscreen-animation`,
//! `ns-use-mwheel-acceleration`, `ns-use-mwheel-momentum` from
//! `nsterm.m:11662-11694`, and `ns-use-proxy-icon` from `nsfns.m:4235`) is
//! declared by `defvar_bool::GNU_BOOL_VARIABLES` with the NS surface flag, so
//! its registration and `byte-boolean-vars` membership come from the same
//! table as every other `DEFVAR_BOOL`.
//!
//! Layering: `neovm-system` sits above this crate (it depends on
//! `neovm-eval`), so the NS family cannot mirror
//! `neovm_system::emacs_core::system::platform::w32` at the `Context`
//! construction site.  These names need no subrs, only variable seeding, so
//! the family lives here and is called from `Context::seed_c_level_defvars`
//! when the reference surface is NS.

use neovm_value::symbol::Obarray;
use neovm_value::value::Value;

/// Register the NS-only `DEFVAR_LISP` names; `src/nsterm.m:11568-11657`.
pub(crate) fn register_ns_c_level_defvars(obarray: &mut Obarray) {
    // The four modifier descriptions: `nsterm.m:11568`, `11589`, `11614`,
    // `11635`.  Under `NS_IMPL_COCOA` -- the macOS reference build -- the
    // command modifier is `super`; GNU's GNUstep branch picks `meta`
    // instead, and Neo has no GNUstep backend.
    for (name, value) in [
        ("ns-alternate-modifier", "meta"),
        ("ns-command-modifier", "super"),
        ("ns-control-modifier", "control"),
        ("ns-function-modifier", "none"),
    ] {
        obarray.define_special_variable(name, Value::symbol(value));
    }
    // `nsterm.m:11645` DEFVAR_LISP, `ns_antialias_text = Qt`.
    obarray.define_special_variable("ns-antialias-text", Value::T);
    // `nsterm.m:11653` / `11657` DEFVAR_LISP, both `Qnil`.
    obarray.define_special_variable("ns-confirm-quit", Value::NIL);
    obarray.define_special_variable("ns-auto-hide-menu-bar", Value::NIL);
}
