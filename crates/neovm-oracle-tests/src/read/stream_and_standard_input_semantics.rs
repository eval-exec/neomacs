//! Oracle parity tests for `read` STREAM semantics and `standard-input`.
//!
//! These target the reader's *stream* surface rather than plain
//! `read-from-string`: reading successive forms from a buffer (with point
//! advancement), `standard-input` resolution (buffer / string / `t`), the
//! `end-of-file` boundary, and — most importantly — the load/eval-buffer
//! contract where `standard-input` is bound to the stream being loaded so a
//! `(read)` inside a form consumes the *next* top-level form and the loop
//! resumes after it (GNU `readevalloop`; neomacs issue #179).
//!
//! Every form evaluates to a value that is identical between GNU Emacs and
//! neomacs (no addresses, temp-file paths, or buffer identities leak into the
//! compared output), so any mismatch is a genuine reader divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ---------------------------------------------------------------------------
// read from a buffer stream: successive forms + point advancement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_buffer_stream_successive_forms_and_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"
(with-temp-buffer
  (insert "(a b c) 42 \"hi\" [1 2] foo")
  (goto-char (point-min))
  (let* ((f1 (read (current-buffer))) (p1 (point))
         (f2 (read (current-buffer))) (p2 (point))
         (f3 (read (current-buffer)))
         (f4 (read (current-buffer)))
         (f5 (read (current-buffer))))
    ;; f1..f5 plus the two intermediate point positions.  Point must advance
    ;; past exactly the bytes each form consumed.
    (list f1 p1 f2 p2 f3 f4 f5)))
"##;
    assert_oracle_parity(form);
}

#[test]
fn oracle_prop_read_buffer_stream_various_syntaxes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"
(with-temp-buffer
  (insert "?A ?\\n #'car '(quoted) `(bq ,x ,@y) -3.5 #x1F ##")
  (goto-char (point-min))
  (let (out)
    (dotimes (_ 8)
      (push (read (current-buffer)) out))
    (nreverse out)))
"##;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// standard-input resolution: buffer, string, and the batch `t` default
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_standard_input_bound_to_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"
(with-temp-buffer
  (insert "111 222 333")
  (goto-char (point-min))
  (let ((standard-input (current-buffer)))
    ;; `(read)' with no argument reads from `standard-input'.
    (list (read) (read) (read) (point))))
"##;
    assert_oracle_parity(form);
}

#[test]
fn oracle_prop_read_standard_input_bound_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"
(list
  (read "(x . y)")
  (read "?A")
  (read "[1 2 3]")
  (let ((standard-input "sym-from-standard-input")) (read)))
"##;
    assert_oracle_parity(form);
}

// NOTE: `standard-input` = t (and the batch `(read)` default that resolves to
// it) routes through `(read-minibuffer "Lisp expression: ")` in BOTH engines
// now — GNU always did; neomacs was fixed to match (previously it signaled
// `end-of-file` outright). This reads a line from the minibuffer interactively
// or from stdin in `--batch`, printing the prompt, and parses it as one Lisp
// expression. It is not probed here because the oracle harness can't inject
// stdin and the prompt goes to the same stdout the harness parses; the parity
// is covered by a direct A/B check against GNU 31 instead.

// ---------------------------------------------------------------------------
// end-of-file boundary
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_end_of_file_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"
(list
  ;; Empty string stream.
  (condition-case e (read "") (end-of-file 'eof-empty-string) (error (list 'err (car e))))
  ;; Whitespace/comment-only string stream.
  (condition-case e (read "   ;; just a comment\n") (end-of-file 'eof-ws) (error (list 'err (car e))))
  ;; Empty buffer stream.
  (with-temp-buffer
    (goto-char (point-min))
    (condition-case e (read (current-buffer)) (end-of-file 'eof-empty-buf)))
  ;; One form then past-end.
  (with-temp-buffer
    (insert "1")
    (goto-char (point-min))
    (list (read (current-buffer))
          (condition-case e (read (current-buffer)) (end-of-file 'eof-past-end)))))
"##;
    assert_oracle_parity(form);
}

#[test]
fn oracle_prop_internal_load_stream_name_is_not_forgeable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU removes its internal `get-file-char` symbol from the obarray and
    // recognizes it by object identity.  Ordinary interned symbols bearing
    // either GNU's printed name or neomacs's former sentinel name must remain
    // ordinary function streams, not acquire access to loader state.
    let form = r##"
(mapcar
 (lambda (stream)
   (let ((standard-input stream))
     (condition-case e
         (list 'value (read))
       (error (list 'error (car e) (cdr e))))))
 '(internal--load-read-stream get-file-char))
"##;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// read-from-string START/END and the returned index
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_from_string_start_end_and_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"
(list
  (read-from-string "(1 2 3)")
  (read-from-string "xx(a b)yy" 2 7)
  (let ((r (read-from-string "12 34"))) (list (car r) (cdr r)))
  (read-from-string "hello world" 0 5)
  ;; START past a leading form leaves the reader at the next one.
  (let* ((s "10 20 30")
         (r1 (read-from-string s))
         (r2 (read-from-string s (cdr r1))))
    (list r1 r2)))
"##;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// The load / eval-buffer contract (issue #179): a `(read)' inside a loaded
// form consumes the NEXT top-level form, and the loop resumes after it.
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_read_inside_eval_buffer_consumes_next_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU `Feval_buffer' binds `standard-input' to the buffer being evaluated,
    // so form 1's `(read)' reads form 2 and the loop skips it.  Result is
    // `((defvar oracle179eb-skipped 'was-skipped) nil resumed)' in both.
    let form = r##"
(with-temp-buffer
  (insert "(defvar oracle179eb-consumed (read))\n")
  (insert "(defvar oracle179eb-skipped 'was-skipped)\n")
  (insert "(defvar oracle179eb-after 'resumed)\n")
  (eval-buffer)
  (list oracle179eb-consumed
        (boundp 'oracle179eb-skipped)
        oracle179eb-after))
"##;
    assert_oracle_parity(form);
}

#[test]
fn oracle_prop_read_inside_load_consumes_next_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Same contract via `load' of a real file (the readevalloop path).  This
    // is chemacs2's crashing pattern: `(read)' with `standard-input' = the
    // load stream reads the next top-level form.
    let form = r##"
(let ((f (make-temp-file "oracle-read179-" nil ".el")))
  (unwind-protect
      (progn
        (with-temp-file f
          (insert "(defvar oracle179ld-consumed (read))\n")
          (insert "(defvar oracle179ld-skipped 'was-skipped)\n")
          (insert "(defvar oracle179ld-after 'resumed)\n"))
        (load f nil t)
        (list oracle179ld-consumed
              (boundp 'oracle179ld-skipped)
              oracle179ld-after))
    (ignore-errors (delete-file f))))
"##;
    assert_oracle_parity(form);
}

#[test]
fn oracle_prop_read_multiple_reads_accumulate_from_load_stream() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A form that reads several following forms itself, then the loop resumes
    // after the last one it consumed.  Exercises repeated advancement of the
    // shared load cursor.
    let form = r##"
(with-temp-buffer
  (insert "(defvar oracle179m-sum (+ (read) (read) (read)))\n")
  (insert "100\n")
  (insert "200\n")
  (insert "300\n")
  (insert "(defvar oracle179m-after 'resumed)\n")
  (eval-buffer)
  (list oracle179m-sum
        (boundp 'oracle179m-after)
        oracle179m-after))
"##;
    assert_oracle_parity(form);
}
