//! Oracle guards for `documentation-dynamic-reload`'s **retry**, which ledger
//! 182 §10 recorded as declared here and not implemented.
//!
//! GNU's reader does not answer nil when a docstring reference fails to
//! resolve.  It rereads whatever the reference points into and tries once
//! more:
//!
//! ```c
//!   Lisp_Object doc = tem;
//!   tem = get_doc_string (tem, 0);
//!   if (NILP (tem) && try_reload)
//!     {
//!       /* The file is newer, we need to reset the pointers.  */
//!       reread_doc_file (Fcar_safe (doc));
//!       try_reload = false;
//!       goto retry;
//!     }
//! ```
//!
//! (`src/doc.c:441-447`, and the identical block for functions at
//! `src/doc.c:365-375`.)  `reread_doc_file` is one `if` over the SHAPE of the
//! reference (`src/doc.c:311-317`): a `(FILE . POS)` cons re-`load`s FILE, a
//! bare fixnum re-runs `Fsnarf_documentation`.  Both arms rewrite the plist,
//! which is the only reason the retry can succeed where the first read failed.
//!
//! **The state is reachable in this port and not only in GNU.**  The dumped
//! image carries **1835** `(FILE . POS)` references, every one of them into a
//! `lisp/**/*.elc` that `cargo xtask fresh-build` regenerates, and the state
//! the flag's own docstring names -- "if these files have changed since they
//! were initially loaded" -- is a **recompile**, because the offset lives in
//! the compiled file as a literal `(#$ . N)`.  Editing a docstring that
//! precedes another one and recompiling moves the second one's `N`, and every
//! reference already in the image then points at the wrong bytes.
//!
//! A four-byte prefix on an existing `.elc` is NOT that state and is not a
//! model of it: the recorded `N` does not move either, so GNU answers nil there
//! too.  Measured both ways; only the recompile is a divergence.
//!
//! Every form below sets `documentation-dynamic-reload` explicitly, in both
//! directions, because the flag is the mechanism under test and because a
//! reading sweep is a WRITE while it is on (ledger 182 §4).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// The `(FILE . POS)` arm: a reference into a compiled file whose bytes have
/// moved.
///
/// The fixture is written by the form itself rather than taken from the
/// preloaded `.elc`s, for two reasons.  Ledger 189 §1 measured that pointing a
/// doc probe at a working checkout is not a fixed target -- with the reload off
/// GNU answers nil for 129 names whose `.elc` have drifted from its `etc/DOC`
/// -- and the two editors do not preload the same set of files anyway.  A
/// four-line file that both editors read the same way removes both.
///
/// `#@14 ` is five bytes, so position 5 is the first byte of the record and
/// `\037` terminates it: exactly `make-docfile`'s and the byte compiler's
/// dynamic-docstring layout, which is what `src/doc.c:240-263` validates.
///
/// The columns are the mechanism, not the text:
///
/// 1. the reference resolves before anything is broken;
/// 2. with the reload OFF the stale reference reads nil -- the control, and
///    the row that proves the corruption took;
/// 3. and the plist still holds the stale reference, i.e. nothing was
///    rewritten;
/// 4. with the reload ON the docstring comes back;
/// 5. and the plist holds a reference again, because the reread re-ran the
///    file's `put`.
#[test]
fn oracle_a_stale_reference_into_a_compiled_file_is_reread_and_retried() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r##"
(let ((f (expand-file-name "l194-reread-cons.el" temporary-file-directory)))
  (unwind-protect
      (progn
        (with-temp-file f
          (insert "#@14 doc for 194.\037\n")
          (insert (format "(put 'l194-victim 'variable-documentation (cons %S 5))\n" f)))
        (set 'documentation-dynamic-reload nil)
        (load f nil t t)
        (let ((fresh (documentation-property 'l194-victim 'variable-documentation t)))
          (put 'l194-victim 'variable-documentation (cons f 9))
          (let* ((off (documentation-property 'l194-victim 'variable-documentation t))
                 (off-plist (get 'l194-victim 'variable-documentation)))
            (set 'documentation-dynamic-reload t)
            (let* ((on (documentation-property 'l194-victim 'variable-documentation t))
                   (on-plist (get 'l194-victim 'variable-documentation)))
              (list fresh
                    off
                    (equal off-plist (cons f 9))
                    on
                    (equal on-plist (cons f 5)))))))
    (when (file-exists-p f) (delete-file f))))"##;
    let expect = expect_test::expect![[r#""OK (\"doc for 194.\" nil t \"doc for 194.\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The retry is taken **once**, and the proof is a count rather than an
/// argument: `try_reload = false` is assigned before the `goto`, so a file that
/// does not repair the reference is loaded exactly one time and then the answer
/// is nil.
///
/// Without the count this test would pass on a port that never rereads at all
/// (nil is nil), and it would also pass on one that loops forever until the
/// process is killed -- which is the shape ledger 174's "a fail-fast run is a
/// false green" warns about, met here by asserting the middle value.
#[test]
fn oracle_a_reread_that_does_not_repair_the_reference_happens_exactly_once() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((f (expand-file-name "l194-reread-once.el" temporary-file-directory)))
  (unwind-protect
      (progn
        (with-temp-file f
          (insert "(setq l194-load-count (1+ (or (bound-and-true-p l194-load-count) 0)))\n"))
        (set 'l194-load-count 0)
        (set 'documentation-dynamic-reload t)
        (put 'l194-nr 'variable-documentation (cons f 5))
        (list (documentation-property 'l194-nr 'variable-documentation t)
              l194-load-count))
    (when (file-exists-p f) (delete-file f))))"#;
    let expect = expect_test::expect![[r#""OK (nil 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The bare-fixnum arm: `reread_doc_file (Fcar_safe (doc))` with a nil car is
/// `Fsnarf_documentation (Vdoc_file_name)`, so GNU **repairs the plist** and
/// then answers out of the repaired entry.
///
/// `case-fold-search` is a `DEFVAR_BOOL` in both editors and its entry is an
/// integer in both (ledger 182), so the row that is pinned is the repair, not
/// the offset -- the offsets differ between the two DOC images and between GNU
/// builds.
///
/// The reload-off arm is the control that says the corruption took, and it is
/// also the row a port with no retry answers in both columns.
#[test]
fn oracle_a_doc_position_that_is_not_a_record_is_repaired_by_the_reread() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((orig (get 'case-fold-search 'variable-documentation)))
  (set 'documentation-dynamic-reload nil)
  (put 'case-fold-search 'variable-documentation 7)
  (let ((off (documentation-property 'case-fold-search 'variable-documentation t))
        (off-plist (get 'case-fold-search 'variable-documentation)))
    (set 'documentation-dynamic-reload t)
    (let ((on (documentation-property 'case-fold-search 'variable-documentation t))
          (on-plist (get 'case-fold-search 'variable-documentation)))
      (list (integerp orig)
            off
            off-plist
            on
            (equal on-plist orig)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil 7 \"Non-nil if searches and matches should ignore case.\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The fixnum `0` is NOT a stale reference, and the order of two lines in GNU
/// is the whole reason:
///
/// ```c
///   if (BASE_EQ (tem, make_fixnum (0))) tem = Qnil;
///   if (FIXNUMP (tem) || (CONSP (tem) && FIXNUMP (XCDR (tem)))) { ... retry ... }
/// ```
///
/// (`src/doc.c:433-437`.)  The zero is erased *before* the test that could send
/// it to `get_doc_string`, so it never reaches the reread -- which matters
/// because `case-fold-search` is a name the reread would otherwise **repair**,
/// turning GNU's reserved "there is no doc" into a docstring.
///
/// This is the negative half of the retry and it cannot be seen from the
/// answer alone: both a correct port and one that rereads answer nil for the
/// docstring.  The plist column is what separates them.
#[test]
fn oracle_the_reserved_zero_is_not_a_stale_reference_and_is_never_reread() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (set 'documentation-dynamic-reload t)
  (put 'case-fold-search 'variable-documentation 0)
  (list (documentation-property 'case-fold-search 'variable-documentation t)
        (get 'case-fold-search 'variable-documentation)))"#;
    let expect = expect_test::expect![[r#""OK (nil 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The scenario the flag exists for, end to end: a file is byte-compiled,
/// loaded, then **edited and recompiled**, and the reference already in the
/// image points at the wrong bytes.
///
/// This is the only faithful model of "the file is newer" (`src/doc.c:373`),
/// because the byte compiler writes the offset into the compiled file as a
/// literal `(#$ . N)`: the reader turns `#$` into `load-file-name` but takes
/// `N` verbatim, so a reload only helps when the recorded `N` itself changed.
/// The first docstring is lengthened so that the second one -- the one under
/// test -- moves.
///
/// The offsets themselves are deliberately NOT pinned: they are a property of
/// the compiled output's layout, and pinning them would turn a compiler header
/// change into a red on a test about documentation.  The fourth column asserts
/// only that the reference is no longer the one the image held.
#[test]
fn oracle_a_recompiled_file_moves_its_docstrings_and_the_reread_follows_them() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((src (expand-file-name "l194-recompiled.el" temporary-file-directory))
       (elc (concat src "c"))
       (write (lambda (pad)
                (with-temp-file src
                  (insert ";;; l194-recompiled.el --- fixture  -*- lexical-binding: t -*-\n")
                  (insert (format "(defvar l194-g-first nil\n  \"First docstring.%s\")\n" pad))
                  (insert "(defvar l194-g-second nil\n  \"Second docstring, the one under test.\")\n")
                  (insert "(provide 'l194-recompiled)\n")))))
  (unwind-protect
      (progn
        (set 'documentation-dynamic-reload nil)
        (funcall write "")
        (byte-compile-file src)
        (load elc nil t t)
        (let ((ref-loaded (get 'l194-g-second 'variable-documentation))
              (doc-loaded (documentation-property 'l194-g-second 'variable-documentation t)))
          (funcall write " Now with a much longer first docstring so every later offset moves.")
          (byte-compile-file src)
          (let ((off (documentation-property 'l194-g-second 'variable-documentation t)))
            (set 'documentation-dynamic-reload t)
            (let ((on (documentation-property 'l194-g-second 'variable-documentation t)))
              (list doc-loaded
                    off
                    on
                    (equal (get 'l194-g-second 'variable-documentation) ref-loaded))))))
    (when (file-exists-p src) (delete-file src))
    (when (file-exists-p elc) (delete-file elc))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Second docstring, the one under test.\" nil \"Second docstring, the one under test.\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
