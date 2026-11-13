//! Face sentinels are the boot-time objects, in both directions.
//!
//! GNU `UNSPECIFIEDP` / `IGNORE_DEFFACE_P` / `RESET_P` are `EQ` against the
//! static `Q` symbols (`src/xfaces.c:296-304`). `Finternal_lisp_face_empty_p`
//! walks the lface and returns t only when every attribute slot is that
//! `Qunspecified` (`src/xfaces.c:4555-4571`). `unintern` drops the spelling;
//! slots that still hold the original object stay unspecified, so empty-p
//! is t and `face-attribute-relative-p` of that object is t.
//! `face-attribute-relative-p` and `merge-face-attribute` are the same two
//! `EQ` tests (`src/xfaces.c:4206`, `4221`): the new symbol produced by
//! interning the spelling again is not a sentinel. `:family` then rejects
//! it with `stringp` (`src/xfaces.c` `CHECK_STRING` on a non-sentinel).
//! `reset` is not relative. `set-face-attribute` in `lisp/faces.el` skips a
//! non-string `:family` unless it is `eq` to the original `unspecified`.
//! Color slots use the same three `EQ` tests and then `CHECK_STRING`
//! (`src/xfaces.c:3561-3608`). Inside the unspecified section, `:foreground`
//! of the saved object is accepted and reads back as that object; `:foreground`
//! of the new same-named symbol signals `wrong-type-argument` `stringp`.
//! `map_tty_color` maps only strings (`src/xfaces.c:6620-6648`).
//!
//! The expectation is the prin1 attested GNU Emacs 31.1 (`a360712c9d27`)
//! printed for this form.

#![cfg(unix)]

use crate::common;

use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn face_sentinel_unintern_keeps_the_static_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"(progn
  (defun neo-call (label thunk)
    (condition-case err
        (list label 'ok (funcall thunk))
      (error (list label 'err (car err) (cdr err)))))
  (defun neo-face (tag)
    (let ((face (intern (format "neo-face-%s" tag))))
      (make-face face)
      face))
  (defun neo-readback (face attr old new)
    (let ((via-face (face-attribute face attr))
          (via-get (internal-get-lisp-face-attribute face attr)))
      (list
       (list 'face-attribute
             (list 'eq-new (eq via-face new))
             (list 'eq-old (eq via-face old))
             (list 'name (and (symbolp via-face) (symbol-name via-face)))
             (list 'value via-face))
       (list 'internal-get
             (list 'eq-new (eq via-get new))
             (list 'eq-old (eq via-get old))
             (list 'name (and (symbolp via-get) (symbol-name via-get)))
             (list 'value via-get))
       (list 'empty-p (internal-lisp-face-empty-p face))
       (list 'face-p
             (let ((vec (internal-lisp-face-p face)))
               (list (vectorp vec)
                     (and (vectorp vec) (eq (aref vec 0) 'face))))))))
  (defun neo-sentinel (label name attr)
    (let* ((old (intern name))
           (face-old (neo-face (format "%s-old" label)))
           (face-new (neo-face (format "%s-new" label)))
           (face-set (neo-face (format "%s-set" label)))
           (removed (unintern old obarray))
           (new (intern name))
           (body
            (list label
                  (list 'unintern removed 'eq-new-old (eq new old))
                  (list 'relative :weight
                        'old (face-attribute-relative-p :weight old)
                        'new (face-attribute-relative-p :weight new))
                  (list 'merge :weight
                        'old-eq-old (eq (merge-face-attribute :weight old 'bold) old)
                        'old-eq-bold (eq (merge-face-attribute :weight old 'bold) 'bold)
                        'new-eq-new (eq (merge-face-attribute :weight new 'bold) new)
                        'new-eq-old (eq (merge-face-attribute :weight new 'bold) old)
                        'new-eq-bold (eq (merge-face-attribute :weight new 'bold) 'bold))
                  (neo-call 'internal-set-old
                    (lambda ()
                      (internal-set-lisp-face-attribute face-old attr old)
                      (neo-readback face-old attr old new)))
                  (neo-call 'internal-set-new
                    (lambda ()
                      (internal-set-lisp-face-attribute face-new attr new)
                      (neo-readback face-new attr old new)))
                  (neo-call 'set-face-new
                    (lambda ()
                      (set-face-attribute face-set nil attr new)
                      (neo-readback face-set attr old new))))))
      (if (string-equal name "unspecified")
          (append body
                  (list
                   (neo-call 'foreground-old
                     (lambda ()
                       (internal-set-lisp-face-attribute face-old :foreground old)
                       (neo-readback face-old :foreground old new)))
                   (neo-call 'foreground-new
                     (lambda ()
                       (internal-set-lisp-face-attribute face-new :foreground new)
                       (neo-readback face-new :foreground old new)))))
        body)))
  (list
   (neo-sentinel 'ignore ":ignore-defface" :family)
   (neo-sentinel 'reset "reset" :family)
   (neo-sentinel 'unspecified "unspecified" :weight)))"##;
    common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r##""OK ((ignore (unintern t eq-new-old nil) (relative :weight old t new nil) (merge :weight old-eq-old nil old-eq-bold t new-eq-new t new-eq-old nil new-eq-bold nil) (internal-set-old ok ((face-attribute (eq-new nil) (eq-old nil) (name \"unspecified\") (value unspecified)) (internal-get (eq-new nil) (eq-old nil) (name \"unspecified\") (value unspecified)) (empty-p nil) (face-p (t t)))) (internal-set-new err wrong-type-argument (stringp :ignore-defface)) (set-face-new ok ((face-attribute (eq-new nil) (eq-old nil) (name \"unspecified\") (value unspecified)) (internal-get (eq-new nil) (eq-old nil) (name \"unspecified\") (value unspecified)) (empty-p t) (face-p (t t))))) (reset (unintern t eq-new-old nil) (relative :weight old nil new nil) (merge :weight old-eq-old t old-eq-bold nil new-eq-new t new-eq-old nil new-eq-bold nil) (internal-set-old ok ((face-attribute (eq-new nil) (eq-old t) (name \"reset\") (value reset)) (internal-get (eq-new nil) (eq-old t) (name \"reset\") (value reset)) (empty-p nil) (face-p (t t)))) (internal-set-new err wrong-type-argument (stringp reset)) (set-face-new ok ((face-attribute (eq-new nil) (eq-old nil) (name \"unspecified\") (value unspecified)) (internal-get (eq-new nil) (eq-old nil) (name \"unspecified\") (value unspecified)) (empty-p t) (face-p (t t))))) (unspecified (unintern t eq-new-old nil) (relative :weight old t new nil) (merge :weight old-eq-old nil old-eq-bold t new-eq-new t new-eq-old nil new-eq-bold nil) (internal-set-old ok ((face-attribute (eq-new nil) (eq-old t) (name \"unspecified\") (value unspecified)) (internal-get (eq-new nil) (eq-old t) (name \"unspecified\") (value unspecified)) (empty-p t) (face-p (t t)))) (internal-set-new ok ((face-attribute (eq-new t) (eq-old nil) (name \"unspecified\") (value unspecified)) (internal-get (eq-new t) (eq-old nil) (name \"unspecified\") (value unspecified)) (empty-p nil) (face-p (t t)))) (set-face-new ok ((face-attribute (eq-new t) (eq-old nil) (name \"unspecified\") (value unspecified)) (internal-get (eq-new t) (eq-old nil) (name \"unspecified\") (value unspecified)) (empty-p nil) (face-p (t t)))) (foreground-old ok ((face-attribute (eq-new nil) (eq-old t) (name \"unspecified\") (value unspecified)) (internal-get (eq-new nil) (eq-old t) (name \"unspecified\") (value unspecified)) (empty-p t) (face-p (t t)))) (foreground-new err wrong-type-argument (stringp unspecified))))""##
        ]],
    );
}

/// `facep` and `internal-lisp-face-p` are `Fgethash` on the symbol object.
///
/// GNU `Finternal_lisp_face_p` (`src/xfaces.c:3120-3141`) resolves aliases and
/// then calls `lface_from_face_name`. That is `Fgethash` (`src/xfaces.c:2060-2075`)
/// on an eq table (`src/xfaces.c:7620-7622` for `Vface_new_frame_defaults`;
/// the frame table is `make_hash_table` with `hashtest_eq`). `resolve_face_name`
/// interns only strings (`src/xfaces.c:2013-2020`). A `make-symbol` of a face
/// name is not a face. After `unintern`, the original object stays the key;
/// the symbol `intern` allocates for that spelling is not a face, on the
/// global table or the frame table.
///
/// The expectation is the prin1 attested GNU Emacs 31.1 (`a360712c9d27`)
/// printed for this form. Before this fix, Neomacs
/// `make-symbol` was a face through the name fallback, and `internal-get` on
/// the saved object inserted the re-interned symbol into the frame hash.
#[test]
fn facep_and_internal_lisp_face_p_are_eq_on_the_symbol_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"(let* ((name "neo-lface-eq-3616")
       (canon (intern name))
       (fresh (make-symbol name))
       (frame (selected-frame))
       (row (lambda (label sym)
              (list label
                    (list 'facep (not (null (facep sym))))
                    (list 'internal-global
                          (let ((v (internal-lisp-face-p sym)))
                            (and (vectorp v) (eq (aref v 0) 'face))))
                    (list 'internal-frame
                          (let ((v (internal-lisp-face-p sym frame)))
                            (and (vectorp v) (eq (aref v 0) 'face))))
                    (condition-case err
                        (list 'get (internal-get-lisp-face-attribute sym :weight frame))
                      (error (list 'err (car err))))
                    (list 'frame-hash
                          (not (null (gethash sym (frame--face-hash-table frame)))))
                    (list 'defaults
                          (not (null (gethash sym face--new-frame-defaults))))))))
  (make-face canon)
  (let ((canon-row (funcall row 'canon canon))
        (fresh-row (funcall row 'fresh fresh))
        (removed (unintern canon obarray))
        (re (intern name)))
    (list
     (list 'eq-fresh (eq fresh canon))
     canon-row
     fresh-row
     (list 'after-unintern removed (eq re canon)
           (funcall row 'saved canon)
           (funcall row 'reintern re)))))"##;
    common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((eq-fresh nil) (canon (facep t) (internal-global t) (internal-frame t) (get unspecified) (frame-hash t) (defaults t)) (fresh (facep nil) (internal-global nil) (internal-frame nil) (err error) (frame-hash nil) (defaults nil)) (after-unintern t nil (saved (facep t) (internal-global t) (internal-frame t) (get unspecified) (frame-hash t) (defaults t)) (reintern (facep nil) (internal-global nil) (internal-frame nil) (err error) (frame-hash nil) (defaults nil))))""#
        ]],
    );
}
