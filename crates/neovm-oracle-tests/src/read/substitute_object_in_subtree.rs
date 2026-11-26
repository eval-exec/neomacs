//! Oracle parity tests for `lread--substitute-object-in-subtree`.
//!
//! GNU `Flread__substitute_object_in_subtree` and `substitute_object_recurse`
//! (src/lread.c) put a `#N=` object everywhere its placeholder occurs, in
//! place. The reader uses them for its own `#N=`, and edebug's reader
//! (`edebug-read-special`, lisp/emacs-lisp/edebug.el) calls the subr.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_lread_substitute_object_in_subtree_char_table_skips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // GNU lread.c starts sub-char-table traversal at raw vector slot 2,
    // skipping contents[0] at each depth on the pinned 64-bit oracle.
    let form = r#"(list
  (let* ((ph (cons nil nil)) (ct (make-char-table 'test)))
    (aset ct ?a ph)
    (set-char-table-range ct '(#x3000 . #x30ff) ph)
    (lread--substitute-object-in-subtree ct ph t)
    (list (eq (aref ct ?a) ct) (eq (aref ct #x3042) ct))))"#;
    crate::common::assert_oracle_parity_expect(form, expect_test::expect![[r#""OK ((t nil))""#]]);
}

#[test]
fn oracle_prop_lread_substitute_object_in_subtree_walks_containers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Conses, vectors, records, byte-code constants, interpreted-closure
    // environments and string text properties are substituted in place;
    // hash tables and bool-vectors are left alone. Atoms, an OBJECT that is
    // the placeholder itself, and a COMPLETED hash table all return nil.
    let form = r#"(let* ((ph (cons nil nil))
       (s (propertize "ab" 'p ph 'q (list ph)))
       (f (make-byte-code 0 "\300\207" (vector ph) 1))
       (c (make-interpreted-closure nil '(x) (list (cons 'x ph))))
       (h (make-hash-table :test 'eq))
       (obj (list 1 ph (vector 2 ph) (cons ph 3) (record 'r ph) s f c h
                  (make-bool-vector 2 t)))
       (ph2 (cons nil nil))
       (small (list 1 ph2 (vector 2 ph2) (cons ph2 3))))
  (puthash 'k ph h)
  (list (lread--substitute-object-in-subtree obj ph t)
        (eq (nth 1 obj) obj)
        (eq (aref (nth 2 obj) 1) obj)
        (eq (car (nth 3 obj)) obj)
        (eq (aref (nth 4 obj) 1) obj)
        (eq (get-text-property 1 'p s) obj)
        (eq (car (get-text-property 0 'q s)) obj)
        (eq (aref (aref f 2) 0) obj)
        (eq (funcall c) obj)
        (eq (gethash 'k h) ph)
        (progn (lread--substitute-object-in-subtree small ph2 t)
               (let ((print-circle t)) (prin1-to-string small)))
        (lread--substitute-object-in-subtree 1 2 3)
        (lread--substitute-object-in-subtree ph ph t)
        (let ((l (list 1 2))) (lread--substitute-object-in-subtree l ph t) l)
        (let* ((l (list 1 ph)) (done (make-hash-table :test 'eq)))
          (puthash l nil done)
          (lread--substitute-object-in-subtree l ph done)
          (eq (nth 1 l) l))))"#;
    let expect = expect_test::expect![[
        r##""OK (nil t t t t t t t t t \"#1=(1 #1# [2 #1#] (#1# . 3))\" nil nil (1 2) t)""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_lread_substitute_object_in_subtree_signals_sequencep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Any other pseudovector signals `(wrong-type-argument sequencep X)`
    // after the slots walked before it were substituted. A symbol with
    // position is such a pseudovector unless `symbols-with-pos-enabled`.
    let form = r#"(let* ((ph (cons nil nil))
       (obj (list ph (current-buffer) ph))
       (v (vector ph (make-marker) ph))
       (e1 (condition-case e (lread--substitute-object-in-subtree obj ph t) (error e)))
       (e2 (condition-case e (lread--substitute-object-in-subtree v ph t) (error e)))
       (pos (list (position-symbol 'a 5) ph)))
  (list (car e1) (car (cdr e1)) (bufferp (nth 2 e1))
        (eq (car obj) obj) (eq (nth 2 obj) ph)
        (car e2) (car (cdr e2)) (markerp (nth 2 e2))
        (eq (aref v 0) v) (eq (aref v 2) ph)
        (car (condition-case e (lread--substitute-object-in-subtree pos ph t) (error e)))
        (let ((symbols-with-pos-enabled t))
          (lread--substitute-object-in-subtree pos ph t)
          (eq (nth 1 pos) pos))))"#;
    let expect = expect_test::expect![[
        r#""OK (wrong-type-argument sequencep t t t wrong-type-argument sequencep t t t wrong-type-argument t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_read_circle_substitutes_like_the_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The reader's own `#N=` goes through the same walk: it reaches a
    // byte-code literal's constants, and a positioned symbol in a non-cons
    // `#N=` object signals `sequencep` unless `symbols-with-pos-enabled`.
    let form = r##"(let ((x (read "#1=#[0 \"\\300\\207\" [#1#] 1]")))
  (list (eq (aref (aref x 2) 0) x)
        (car (condition-case e (read-positioning-symbols "#1=[a #1#]") (error e)))
        (let ((symbols-with-pos-enabled t))
          (let ((v (read-positioning-symbols "#1=[a #1#]")))
            (eq (aref v 1) v)))))"##;
    let expect = expect_test::expect![[r#""OK (t wrong-type-argument t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_edebug_eval_top_level_form_keeps_circular_literals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `edebug-eval-top-level-form` reads `#N=` through `edebug-read-special`,
    // which calls the subr. Its `eval-expression` prints each value to
    // stdout, hence the transcript prefix.
    let form = r#"(progn
  (require 'edebug)
  (defvar neo-v nil)
  (let ((edebug-all-forms t) (edebug-all-defs t) (edebug-initial-mode 'Go-nonstop)
        (print-circle t))
    (with-temp-buffer
      (emacs-lisp-mode)
      (insert "(setq neo-v '#1=(a . #1#))")
      (goto-char (point-min))
      (edebug-eval-top-level-form))
    (with-temp-buffer
      (emacs-lisp-mode)
      (insert "(defun neo-f () (list '#1=(b . #1#) [#2=(c) #2#] '#3=[d #3#]))")
      (goto-char (point-min))
      (edebug-eval-top-level-form))
    (let ((r (neo-f)))
      (list (eq (cdr neo-v) neo-v) (car neo-v)
            (eq (cdr (car r)) (car r))
            (eq (aref (nth 1 r) 0) (aref (nth 1 r) 1))
            (eq (aref (nth 2 r) 1) (nth 2 r))))))"#;
    let expect = expect_test::expect![[r##""#1=(a . #1#)neo-fOK (t a t t t)""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_lread_substitute_object_in_subtree_terminates_on_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The walk must stop at nodes it has already entered (GNU's `seen` list
    // in `substitute_object_recurse`, src/lread.c). These shapes revisit a
    // node: GNU's own ERT case (test/src/lread-tests.el), a nested `#N=`
    // reader label, and a circular list whose placeholder sits inside the
    // cycle. A walk without cycle termination never returns on them.
    let form = r##"(list
  (let ((x (cons 0 1)))
    (setcar x x)
    (lread--substitute-object-in-subtree x 1 t)
    (eq x (cdr x)))
  (let ((v (read "#1=[#2=[#1# #2#]]")))
    (list (eq (aref (aref v 0) 0) v)
          (eq (aref (aref v 0) 1) (aref v 0))))
  (let* ((x (list 1 2 3)) (ph (cons nil nil)))
    (setcdr (nthcdr 2 x) x)
    (setcar (cdr x) ph)
    (lread--substitute-object-in-subtree x ph t)
    (list (eq (nth 1 x) x) (eq (nthcdr 3 x) x))))"##;
    let expect = expect_test::expect![[r#""OK (t (t t) (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_lread_substitute_object_in_subtree_char_table_raw_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // fillarray leaves the independent ASCII slot intact in GNU.
    let form = r#"(let* ((ph (cons nil nil)) (ct (make-char-table 'test)))
  (aset ct ?a 'ascii-value)
  (fillarray ct ph)
  (lread--substitute-object-in-subtree ct ph t)
  (list (aref ct ?a) (char-table-range ct ?a)
        (eq (char-table-range ct nil) ct) (eq (aref ct #x10000) ct)))"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK (ascii-value ascii-value t t)""#]],
    );
}

#[test]
fn oracle_prop_lread_substitute_object_in_subtree_char_table_extras() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((ph (cons nil nil)) (purpose (make-symbol "extra-test")))
  (put purpose 'char-table-extra-slots 2)
  (let ((ct (make-char-table purpose)))
    (set-char-table-extra-slot ct 0 ph)
    (set-char-table-extra-slot ct 1 (vector ph))
    (lread--substitute-object-in-subtree ct ph t)
    (list (eq (char-table-extra-slot ct 0) ct)
          (eq (aref (char-table-extra-slot ct 1) 0) ct))))"#;
    crate::common::assert_oracle_parity_expect(form, expect_test::expect![[r#""OK (t t)""#]]);
}

#[test]
fn oracle_prop_lread_substitute_object_in_subtree_char_table_purpose_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Raw slot stores permit a non-symbol purpose and a self-parent.
    let form = r#"(list
  (let* ((ph (make-symbol "purpose")) (ct (make-char-table ph)))
    (lread--substitute-object-in-subtree ct ph t)
    (eq (char-table-subtype ct) ct))
  (let* ((ph (make-char-table 'test)) (ct (make-char-table 'test)))
    (set-char-table-parent ct ph)
    (lread--substitute-object-in-subtree ct ph t)
    (eq (char-table-parent ct) ct)))"#;
    crate::common::assert_oracle_parity_expect(form, expect_test::expect![[r#""OK (t t)""#]]);
}

#[test]
fn oracle_prop_lread_substitute_object_in_subtree_char_table_depth_skips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((ph (cons nil nil)) (ct (make-char-table 'test)))
  (set-char-table-range ct '(#x10000 . #x10fff) ph)
  (set-char-table-range ct '(#x11000 . #x1107f) ph)
  (aset ct #x11080 ph)
  (aset ct #x11081 ph)
  (lread--substitute-object-in-subtree ct ph t)
  (mapcar (lambda (c) (eq (aref ct c) ct))
          '(#x10001 #x11001 #x11080 #x11081)))"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK (nil nil nil t)""#]],
    );
}

#[test]
fn oracle_prop_lread_substitute_object_in_subtree_char_table_partial_failure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((ph (cons nil nil)) (purpose (make-symbol "failure-test")))
  (put purpose 'char-table-extra-slots 3)
  (let ((ct (make-char-table purpose ph)))
    (set-char-table-extra-slot ct 0 ph)
    (set-char-table-extra-slot ct 1 (current-buffer))
    (set-char-table-extra-slot ct 2 ph)
    (list (condition-case err
              (lread--substitute-object-in-subtree ct ph t)
            (error (list (car err) (cadr err))))
          (eq (char-table-range ct nil) ct)
          (eq (char-table-extra-slot ct 0) ct)
          (eq (char-table-extra-slot ct 2) ph))))"#;
    crate::common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK ((wrong-type-argument sequencep) t t t)""#]],
    );
}

#[test]
fn oracle_prop_lread_substitute_writes_back_every_bytecode_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU's CLOSURE branch of `substitute_object_recurse` takes `PVSIZE` and
    // `ASET`s every closure slot back (src/lread.c), so the substitution is
    // observable through `aref` at slots outside the constants vector -- the
    // arglist (0) and docstring (4) here -- and the printer shows the closure
    // referring to itself. `aset` on a closure itself still signals
    // `(wrong-type-argument arrayp F)`: GNU's `ARRAYP` excludes closures.
    let form = r#"(let* ((ph (cons nil nil))
       (f (make-byte-code ph "\300\207" (vector ph) 1 ph)))
  (lread--substitute-object-in-subtree f ph t)
  (list (eq (aref f 0) f)
        (eq (aref (aref f 2) 0) f)
        (eq (aref f 4) f)
        (prin1-to-string f)
        (condition-case e (aset f 0 1) (error (car e)))
        (condition-case e (aset f 0 1) (error (car (cdr e))))))"#;
    let expect = expect_test::expect![[
        r##""OK (t t t \"#[#0 \\\"\\\\300\\\\207\\\" [#0] 1 #0]\" wrong-type-argument arrayp)""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_lread_substitute_bytecode_readers_share_the_slot_store() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Every non-execution reader of a substituted byte-code closure sees the same
    // stored slot: `aref`, `documentation` (slot 4, nil because the slot holds
    // the closure, not a string), `interactive-form` (slot 5, `(interactive
    // F)`), and the printer. Reading any of them from the immutable function
    // fields instead would disagree with `aref` after the walk.
    let form = r#"(let* ((ph (cons nil nil))
       (f (make-byte-code 0 "\300\207" (vector ph) 1 ph ph)))
  (lread--substitute-object-in-subtree f ph t)
  (list (eq (aref f 4) f)
        (eq (aref f 5) f)
        (documentation f t)
        (eq (car (interactive-form f)) 'interactive)
        (eq (car (cdr (interactive-form f))) f)
        (prin1-to-string f)))"#;
    let expect = expect_test::expect![[
        r##""OK (t t nil t t \"#[0 \\\"\\\\300\\\\207\\\" [#0] 1 #0 #0]\")""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
