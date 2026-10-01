//! Oracle parity tests for the ligature/composition flow (issue #447):
//! `composition-get-gstring` + a rule function filling the gstring must
//! behave identically on both editors.
//!
//! This environment's GNU oracles are TTY-only builds (`font-family-list` is
//! nil in batch), so the FONT-shaping path (`font-shape-gstring` with a real
//! font object) is covered by neovm-core unit tests plus the GUI suite; the
//! oracle here exercises the composition plumbing both editors support
//! without fonts: `compose-gstring-for-terminal`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

#[test]
fn oracle_prop_terminal_composition_fills_glyph_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(condition-case err
                      (let* ((gstring (composition-get-gstring 0 2 nil "->"))
                             (shaped (compose-gstring-for-terminal gstring 'L2R)))
                        (if (null shaped)
                            "OK (nil)"
                          (let ((first (aref shaped 2))
                                (glyph-count
                                 (let ((n 0))
                                   (while (aref shaped (+ 2 n))
                                     (setq n (+ n 1)))
                                   n)))
                            (format "OK (%S %S %S)"
                                    (aref first 0)
                                    (aref first 1)
                                    (>= glyph-count 1)))))
                    (error (format "ERR %S" (car err))))"#;
    assert_oracle_parity(form);
}

#[test]
fn oracle_prop_terminal_composition_covers_the_whole_run() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(condition-case err
                      (let* ((gstring (composition-get-gstring 0 3 nil "-->"))
                             (shaped (compose-gstring-for-terminal gstring 'L2R)))
                        (if (null shaped)
                            "OK (nil)"
                          (let ((n 0))
                            (while (aref shaped (+ 2 n))
                              (setq n (+ n 1)))
                            (format "OK (%S %S)"
                                    (>= n 1)
                                    (aref (aref shaped (+ 1 n)) 1)))))
                    (error (format "ERR %S" (car err))))"#;
    assert_oracle_parity(form);
}
