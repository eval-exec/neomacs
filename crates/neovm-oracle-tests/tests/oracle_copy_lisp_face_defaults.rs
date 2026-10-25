//! `internal-copy-lisp-face` into the defaults face.
//!
//! GNU stores `:ignore-defface` in the new-frame defaults vector when
//! `internal-set-lisp-face-attribute` is given `unspecified` and FRAME t
//! (`src/xfaces.c:3256-3263`). Copying that vector is a raw `vcopy`
//! (`src/xfaces.c:3183`). `face-attribute` reads through
//! `internal-get-lisp-face-attribute`, which presents the sentinel as
//! `unspecified` (`src/xfaces.c:4288-4289`). A live-frame copy does not
//! invent the sentinel.
//!
//! Refresh expectations only from GNU with `NEOVM_ORACLE_MODE=refresh`
//! `UPDATE_EXPECT=1`.

#![cfg(unix)]

use crate::common;

use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn copy_lisp_face_defaults_present_unspecified_and_keep_the_stored_sentinel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"(progn
  (let ((frame (selected-frame))
        (underline '(:color "#234567" :color foreground-color :color "#345678")))
    (make-face 'ign-src)
    (make-face 'ign-dst)
    (make-face 'ign-frame-dst)
    (make-face 'ign-parent)
    (set-face-attribute 'ign-src nil :foreground "#654321" :weight 'normal)
    (set-face-attribute 'ign-src t :foreground "#654321" :weight 'normal)
    (defun ign-public (face domain)
      (mapcar (lambda (attr) (face-attribute face attr domain))
              '(:foreground :background :underline :inherit :weight)))
    (defun ign-stored (face frame)
      (let ((vec (if frame
                     (internal-lisp-face-p face frame)
                   (internal-lisp-face-p face))))
        ;; underline, inherit, weight (dispextern.h lface_attribute_index)
        (list (aref vec 7) (aref vec 16) (aref vec 5))))
    (let (rows)
      (dolist (shape '(colors inherit underline))
        (set-face-attribute 'ign-src t
          :inherit (if (eq shape 'inherit) '(ign-parent bold) 'unspecified)
          :underline (if (eq shape 'underline) underline 'unspecified))
        (internal-copy-lisp-face 'ign-src 'ign-dst t nil)
        (push (list 'defaults shape
                    (ign-public 'ign-dst t)
                    (ign-stored 'ign-dst nil))
              rows))
      (set-face-attribute 'ign-src frame
        :foreground "#654321"
        :weight 'normal
        :inherit 'unspecified
        :underline underline)
      (internal-copy-lisp-face 'ign-src 'ign-frame-dst frame nil)
      (push (list 'frame 'control
                  (ign-public 'ign-frame-dst frame)
                  (ign-stored 'ign-frame-dst frame))
            rows)
      (nreverse rows))))"##;
    common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r##""OK ((defaults colors (\"#654321\" unspecified unspecified unspecified normal) (:ignore-defface :ignore-defface normal)) (defaults inherit (\"#654321\" unspecified unspecified (ign-parent bold) normal) (:ignore-defface (ign-parent bold) normal)) (defaults underline (\"#654321\" unspecified (:color \"#234567\" :color foreground-color :color \"#345678\") unspecified normal) ((:color \"#234567\" :color foreground-color :color \"#345678\") :ignore-defface normal)) (frame control (\"#654321\" unspecified (:color \"#234567\" :color foreground-color :color \"#345678\") unspecified normal) ((:color \"#234567\" :color foreground-color :color \"#345678\") unspecified normal)))""##
        ]],
    );
}
