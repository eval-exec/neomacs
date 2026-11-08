//! `equal-including-properties` tests whose harness is the runtime-startup
//! image. The canonical hook proof is minted in core. The rest of
//! `fns_test.rs` stays in eval. Assertions are unchanged.

/// `equal-including-properties` must honor `symbols-with-pos-enabled` exactly
/// like plain `equal`: GNU's `Fequal_including_properties` shares the same
/// `internal_equal` that unwraps position-symbols when the flag is set.  The
/// byte-compiler binds the flag to `t`, so a hardcoded `false` on the entry
/// path silently diverged (returning nil where GNU returns t).  Probed against
/// GNU Emacs 31: flag off -> (nil nil); flag on -> (t t).
#[test]
fn equal_including_properties_honors_symbols_with_pos_enabled() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    // Flag off (default): a position-symbol differs from another position of
    // the same symbol AND from its bare symbol.
    let off = ev.eval_str(
        "(let ((a (position-symbol 'foo 5)) (b (position-symbol 'foo 9)))
           (list (equal-including-properties a b)
                 (equal-including-properties a 'foo)))",
    );
    assert_eq!(
        crate::emacs_core::format_eval_result(&off),
        "OK (nil nil)",
        "flag off: position-symbols are distinct (GNU 31 parity)"
    );
    // Flag on: positions are ignored, so both compare eq-wise -> t.
    let on = ev.eval_str(
        "(let ((a (position-symbol 'foo 5)) (b (position-symbol 'foo 9)))
           (let ((symbols-with-pos-enabled t))
             (list (equal-including-properties a b)
                   (equal-including-properties a 'foo))))",
    );
    assert_eq!(
        crate::emacs_core::format_eval_result(&on),
        "OK (t t)",
        "flag on: position ignored, compared eq-wise (GNU 31 parity)"
    );
}

/// GNU Emacs 31 parity for the fast-path-relevant edges of
/// `equal-including-properties`: distinct NaN boxes (bitwise float equality),
/// `-0.0` vs `0.0` (distinct bits -> nil), int vs float, records, bool-vectors,
/// unibyte-vs-multibyte strings (ASCII equal; high bytes differ by SBYTES),
/// property-order-independence, and propertized-vs-plain.  The whole vector was
/// probed against GNU 31 and pins to `(t nil nil t nil t nil t nil t nil t)`.
#[test]
fn equal_including_properties_gnu_parity_edges() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();
    let edges = ev.eval_str(
        "(list
           (equal-including-properties (/ 0.0 0.0) (/ 0.0 0.0))
           (equal-including-properties -0.0 0.0)
           (equal-including-properties 1 1.0)
           (equal-including-properties (record 'r 1 2) (record 'r 1 2))
           (equal-including-properties (record 'r 1 2) (record 'r 1 3))
           (equal-including-properties (bool-vector t nil t) (bool-vector t nil t))
           (equal-including-properties (bool-vector t nil t) (bool-vector t nil nil))
           (equal-including-properties (string-to-unibyte \"abc\") (string-to-multibyte \"abc\"))
           (let ((u (unibyte-string 200 201))
                 (m (string-to-multibyte (unibyte-string 200 201))))
             (equal-including-properties u m))
           (equal-including-properties (propertize \"ab\" 'a 1 'b 2)
                                       (propertize \"ab\" 'b 2 'a 1))
           (equal-including-properties (propertize \"abc\" 'face 'bold) \"abc\")
           (equal-including-properties (propertize \"abc\" 'face 'bold)
                                       (propertize \"abc\" 'face 'bold)))",
    );
    assert_eq!(
        crate::emacs_core::format_eval_result(&edges),
        "OK (t nil nil t nil t nil t nil t nil t)",
        "equal-including-properties edge behavior must match GNU Emacs 31"
    );
}
