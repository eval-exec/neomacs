//! Oracle parity for GNU `print.c`'s shared `print_buffer` accumulator under
//! NESTED prints.
//!
//! GNU keeps one `print_buffer` (`src/print.c:79` area). `print_prepare`
//! (`:132-194`) saves a copy of the current content for a nested print and
//! resets `pos`/`pos_byte`; `print_finish` (`:198-228`) inserts
//! `buffer[0..pos]` into the destination; `print_unwind` (`:109-113`)
//! restores the saved BYTES with `memcpy (…, SCHARS (saved))` and leaves
//! `pos` untouched. A `prin1-to-string` (or a `format %S`) performed by a
//! `print-unreadable-function` callback therefore leaks its printed bytes
//! into the outer output: U1/V6, U10, B2 and B3 below are the GNU 31.1
//! answers. B1 pins that GNU's destination buffer is Lisp-invisible
//! (`(get-buffer " prin1")` is nil; the name is free for a user buffer).
//!
//! V1–V5 are the no-nested-print guards: byte-identical before and after the
//! accumulator port. Expects are GNU 31.1 answers recorded with
//! `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_print_buffer_nested_prin1_to_string_leaks_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-unreadable-function
       (lambda (obj _escape) (format "CALLED:%s" (prin1-to-string obj)))))
  (prin1-to-string (make-marker)))
"#;
    let expect =
        expect_test::expect![[r##""OK \"#<marker in no buffer>CALLED:#<marker in no buffer>\"""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_format_s_nested_print_leaks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((standard-output (current-buffer)))
    (setq-local print-unreadable-function
                (lambda (_obj _escape)
                  (format "SEEN:%S" (symbol-value 'print-unreadable-function))))
    (prin1 (make-marker))
    (buffer-string)))
"#;
    let expect = expect_test::expect![[r#""OK \"nilSEEN:nil\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_nested_prin1_to_string_same_buffer_double() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-unreadable-function (lambda (obj _escape) (prin1-to-string obj))))
  (prin1-to-string (make-marker)))
"#;
    let expect =
        expect_test::expect![[r##""OK \"#<marker in no buffer>#<marker in no buffer>\"""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_nested_prin1_to_string_prefixed_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-unreadable-function
       (lambda (obj _escape) (concat "A" (prin1-to-string obj)))))
  (prin1-to-string (make-marker)))
"#;
    let expect =
        expect_test::expect![[r##""OK \"#<marker in no buffer>A#<marker in no buffer>\"""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_hidden_prin1_buffer_is_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((buf (get-buffer " prin1")))
  (list (and buf t)
        (with-current-buffer (or buf (get-buffer-create " prin1"))
          (erase-buffer)
          (insert "PRE")
          (buffer-string))
        (prin1-to-string 'abc)
        (with-current-buffer (or buf (get-buffer-create " prin1"))
          (buffer-string))))
"#;
    let expect = expect_test::expect![[r#""OK (nil \"PRE\" \"abc\" \"PRE\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_guard_constant_callback_prin1_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-unreadable-function (lambda (_obj _escape) "ZZZ")))
  (prin1-to-string (make-marker)))
"#;
    let expect = expect_test::expect![[r#""OK \"ZZZ\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_guard_constant_callback_direct_prin1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((standard-output (current-buffer)))
    (setq-local print-unreadable-function (lambda (_obj _escape) "ZZZ"))
    (prin1 (make-marker))
    (buffer-string)))
"#;
    let expect = expect_test::expect![[r#""OK \"ZZZ\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_guard_nil_callback_delegates_prin1_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-unreadable-function (lambda (_obj _escape) nil)))
  (prin1-to-string (make-marker)))
"#;
    let expect = expect_test::expect![[r##""OK \"#<marker in no buffer>\"""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_guard_non_string_callback_prints_nothing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-unreadable-function (lambda (_obj _escape) 42)))
  (prin1-to-string (make-marker)))
"#;
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_guard_nil_callback_delegates_direct_prin1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((standard-output (current-buffer)))
    (setq-local print-unreadable-function (lambda (_obj _escape) nil))
    (prin1 (make-marker))
    (buffer-string)))
"#;
    let expect = expect_test::expect![[r##""OK \"#<marker in no buffer>\"""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// Design rev 2 A3 for the ERROR renderer (lead 18:2xZ): GNU renders error data
// through Fprin1 on the hidden " prin1" stream, and every such call runs
// print(), which shares the process-static print_number_index. With continuous
// numbering and a live table, an error raised from inside an outer
// print-circle print continues that print's numbering: the outer consumed #1,
// so the circular error datum is labelled #2. A per-path private counter would
// answer #1 here.
#[test]
fn oracle_prop_print_buffer_error_render_continuues_shared_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let* ((print-circle t)
       (print-continuous-numbering t)
       (outer (list 9))
       (cell (list 1))
       (_ (setcdr cell cell))
       (print-unreadable-function
        (lambda (_obj _escape) (signal 'error (list "boom" cell)))))
  (condition-case err
      (progn (prin1-to-string (list outer outer (make-marker))) 'no-error)
    (error (error-message-string err))))
"#;
    let expect = expect_test::expect![[r#""OK \"boom: #2=(1 . #2#)\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// Same shape without continuous numbering: GNU's print() resets the index for
// each error-data prin1, so the label restarts at #1.
#[test]
fn oracle_prop_print_buffer_error_render_resets_without_continuous() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let* ((print-circle t)
       (outer (list 9))
       (cell (list 1))
       (_ (setcdr cell cell))
       (print-unreadable-function
        (lambda (_obj _escape) (signal 'error (list "boom" cell)))))
  (condition-case err
      (progn (prin1-to-string (list outer outer (make-marker))) 'no-error)
    (error (error-message-string err))))
"#;
    let expect = expect_test::expect![[r#""OK \"boom: #1=(1 . #1#)\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// Peer P1 for the A3 split: GNU's `print_number_index` is ONE process static,
// so a second Lisp thread that prints while the outer print-circle print is
// in flight (the callback blocks in `thread-join`) continues the SAME
// numbering. The worker's `#2=` label is observable in its returned string;
// a per-thread index would restart at #1. If this diverges, the
// print_number_index relocation must ship with the accumulator.
#[test]
fn oracle_prop_print_buffer_thread_continuous_numbering_shared_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-circle t)
      (print-continuous-numbering t)
      (print-unreadable-function
       (lambda (obj _escape)
         (let* ((shared2 (list 2))
                (worker (make-thread
                         (lambda ()
                           (let ((print-circle t)
                                 (print-continuous-numbering t))
                             (prin1-to-string (list shared2 shared2))))))
                (result (thread-join worker)))
           (concat "W:" result)))))
  (let ((shared (list 1)))
    (prin1-to-string (list shared (make-marker) shared))))
"#;
    let expect = expect_test::expect![[r#""OK \"(#1=(1) #2#)W:(#2=(2) #2#) #1#)\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// Design rev 2 A1: GNU `print_unwind` memcpy's SCHARS (the CHARACTER count)
// BYTES, so a multibyte save is truncated mid-character. The outer printed
// list is `("éé" MARKER)`; the callback nests a `prin1-to-string`, whose
// prepare saves 8 bytes / 6 chars and whose unwind restores only the first
// six bytes (`("éé`, no closing quote or space).
#[test]
fn oracle_prop_print_buffer_nested_leak_multibyte_partial_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-unreadable-function
       (lambda (obj _escape) (prin1-to-string obj))))
  (prin1-to-string (list "éé" (make-marker))))
"#;
    let expect =
        expect_test::expect![[r#""OK \"(\\\"ééer in no buffer>#<marker in no buffer>)\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_nested_leak_multibyte_short_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-unreadable-function
       (lambda (obj _escape) (prin1-to-string obj))))
  (prin1-to-string (list "é" (make-marker))))
"#;
    let expect =
        expect_test::expect![[r#""OK \"(\\\"é\\\"ker in no buffer>#<marker in no buffer>)\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// GNU's print_unwind runs on every non-local exit (record_unwind_protect).
// A throw out of the callback leaves the destination buffer untouched and the
// accumulator restored: a later print starts clean.
#[test]
fn oracle_prop_print_buffer_throw_out_of_callback_restores() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((standard-output (current-buffer)))
    (setq-local print-unreadable-function
                (lambda (_obj _escape) (throw 'out 'thrown)))
    (list (catch 'out (prin1 (make-marker)))
          (buffer-string)
          (progn (princ "after") (buffer-string)))))
"#;
    let expect = expect_test::expect![[r#""OK (thrown \"\" \"after\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_signal_out_of_callback_restores() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((standard-output (current-buffer)))
    (setq-local print-unreadable-function
                (lambda (_obj _escape) (error "boom")))
    (list (condition-case err
              (prin1 (make-marker))
            (error (error-message-string err)))
          (buffer-string)
          (progn (princ "after") (buffer-string)))))
"#;
    let expect = expect_test::expect![[r#""OK (\"boom\" \"\" \"after\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// GNU's accumulator is one process static under the global lock: a Lisp
// thread that prints while a callback yields takes the NESTED save path on
// the same buffer, so its bytes and its left-over `pos` reach the outer
// output. The outer prefix `("AA" ` (6 bytes) is longer than the worker's
// 5-byte `inner`, so every byte the outer later inserts is deterministic.
#[test]
fn oracle_prop_print_buffer_thread_yield_nested_print_leaks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-unreadable-function
       (lambda (obj _escape)
         (let ((worker (make-thread (lambda () (prin1-to-string 'inner)))))
           (thread-join worker)
           "CB"))))
  (prin1-to-string (list "AA" (make-marker) "BB")))
"#;
    let expect = expect_test::expect![[r#""OK \"(\\\"AA\\\"CB \\\"BB\\\")\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// Design rev 2 A3: GNU's print() resets `Vprint_number_table` on every print,
// nested ones included, so a callback that prints destroys the OUTER
// print-circle table. A shared object printed AFTER the callback then meets a
// nil table and signals `wrong-type-argument hash-table-p nil`. With
// `print-continuous-numbering` non-nil the table is retained, the outer
// labels survive, and only the accumulator leak is visible.
#[test]
fn oracle_prop_print_buffer_circle_table_reset_by_nested_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-circle t)
      (print-unreadable-function
       (lambda (obj _escape) (prin1-to-string obj))))
  (let ((shared (list 1)))
    (prin1-to-string (list shared (make-marker) shared))))
"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument hash-table-p nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_circle_table_retained_under_continuous_numbering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-circle t)
      (print-continuous-numbering t)
      (print-unreadable-function
       (lambda (obj _escape) (prin1-to-string obj))))
  (let ((shared (list 1)))
    (prin1-to-string (list shared (make-marker) shared))))
"#;
    let expect =
        expect_test::expect![[r#""OK \"(#1=(1)  in no buffer>#<marker in no buffer> #1#)\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// Full print_prepare caller map: GNU Fterpri (print.c:638) and Fwrite_char
// (print.c:561) take the same accumulator path for a buffer destination, so
// their nested calls leave the same `pos` quirk behind. terpri writes "\n"
// (pos left at 1 by its unwind), write-char then saves that byte, and the
// outer prin1 appends at the left-over position: buffer `"\nZ\nC"`.
#[test]
fn oracle_prop_print_buffer_terpri_in_callback_takes_accumulator_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((standard-output (current-buffer)))
    (setq-local print-unreadable-function
                (lambda (_obj _escape)
                  (terpri)
                  (write-char ?Z)
                  "C"))
    (prin1 (make-marker))
    (buffer-string)))
"#;
    let expect = expect_test::expect![[r#""OK \"\\nZ\\nC\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_buffer_write_char_in_callback_takes_accumulator_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((standard-output (current-buffer)))
    (setq-local print-unreadable-function
                (lambda (_obj _escape) (write-char ?Q) "C"))
    (prin1 (make-marker))
    (buffer-string)))
"#;
    let expect = expect_test::expect![[r#""OK \"QQC\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
