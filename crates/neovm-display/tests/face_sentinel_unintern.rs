//! GNU DEFSYM identity after `unintern` (xfaces.c:4206, lread.c:4858-4922).
//!
//! Pinned GNU Emacs 31.1 (`a360712c9d272d950d8d8255ef74570f7e90b7d9`),
//! `emacs -Q --batch`:
//!
//! ```elisp
//! (let* ((old (intern "unspecified"))
//!        (removed (unintern old obarray))
//!        (new (intern "unspecified")))
//!   (list removed (eq new old)
//!         (face-attribute-relative-p :weight old)
//!         (face-attribute-relative-p :weight new)))
//! ```
//!
//! prints `(t nil t nil)`. The same form for `:ignore-defface` prints
//! `(t nil t nil ...)`. `Qunspecified` / `QCignore_defface` stay the
//! original objects. This file is its own test binary so the process-global
//! registry mutation does not leak into the display unit suite.
//! Observed on the pinned binary 2026-11-09.

use neovm_display::emacs_core::xfaces::face_attribute_relative_p;
use neovm_value::intern::{WellKnown, intern, unintern_canonical_id};
use neovm_value::tagged::SymbolMode;
use neovm_value::value::Value;

fn relative(value: Value) -> bool {
    face_attribute_relative_p(&[Value::keyword("weight"), value], SymbolMode::Bare)
        .expect("face-attribute-relative-p")
        .is_t()
}

#[test]
fn unintern_unspecified_keeps_the_original_face_sentinel() {
    let old = intern("unspecified");
    assert_eq!(old, WellKnown::Unspecified.id());
    assert!(
        unintern_canonical_id(old),
        "the seeded unspecified symbol is canonical before unintern"
    );
    let new = intern("unspecified");
    assert_ne!(
        new, old,
        "intern after unintern allocates a new symbol, as GNU Funintern does"
    );
    assert!(
        relative(Value::from_sym_id(old)),
        "GNU xfaces.c:4206 EQ(Qunspecified) still matches the original object"
    );
    assert!(
        !relative(Value::from_sym_id(new)),
        "GNU xfaces.c:4206 does not treat the newly interned unspecified as Qunspecified"
    );
}

#[test]
fn unintern_ignore_defface_keeps_the_original_face_sentinel() {
    let old = intern(":ignore-defface");
    assert_eq!(old, WellKnown::KeywordIgnoreDefface.id());
    assert!(unintern_canonical_id(old));
    let new = intern(":ignore-defface");
    assert_ne!(new, old);
    assert!(
        relative(Value::from_sym_id(old)),
        "GNU xfaces.c:4206 EQ(QCignore_defface) still matches the original object"
    );
    assert!(
        !relative(Value::from_sym_id(new)),
        "a newly interned :ignore-defface is an ordinary symbol"
    );
}
