//! C-level DEFVAR seeding moved below the display layer (stage-4 K2 display).
//!
//! The evaluator bootstrap used to call the display modules' `syms_of_*`
//! equivalents; these bodies only touch the Obarray and lower-crate
//! vocabulary, so they live with the bootstrap that runs them.

use neovm_value::value::Value;

pub(crate) fn register_bootstrap_vars(obarray: &mut neovm_value::symbol::Obarray) {
    // fontset.c:2237, `Vvertical_centering_font_regexp = Qnil'.
    //
    // The regexp both editors report is NOT this initializer: it comes from
    // `lisp/international/fontset.el:1266', which `loadup.el' preloads in a
    // window-system build.  What the C declaration supplies is the
    // `declared_special' bit, which no `setq' in Lisp can add -- fontset.el's
    // `(defvar vertical-centering-font-regexp)' on line 1259 is the valueless
    // form, which marks the variable special only within that file.  So a
    // Neomacs that seeded the value and skipped the declaration matched GNU on
    // `symbol-value' and still bound it lexically under `let'.
    obarray.define_special_variable("vertical-centering-font-regexp", Value::NIL);
    // fontset.c:2225 DEFVAR_LISP, `Valternate_fontname_alist = Qnil'.  The one
    // name of `syms_of_fontset''s eight this port did not declare (entry 173).
    obarray.define_special_variable("alternate-fontname-alist", Value::NIL);
}
