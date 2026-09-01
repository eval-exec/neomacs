//! Combo: compare-buffer-substrings + EIEIO state tracking + overlays
//! + markers + textprop + buflocal + narrow + undo.
//! Tests cross-buffer comparison with EIEIO objects managing diff state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_compare_bufsubst_basic_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 66 60)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass diff-state ()
    ((label :initarg :label :accessor ds-label :initform "")
     (mismatches :initarg :mm :accessor ds-mm :initform nil)
     (total-diff :initarg :td :accessor ds-td :initform 0)
     (log :initarg :log :accessor ds-log :initform nil)))
  (let* ((buf-a (generate-new-buffer "cs1a"))
         (buf-b (generate-new-buffer "cs1b"))
         (ds (diff-state :label "diff" :mm nil :td 0 :log nil)))
    (with-current-buffer buf-a
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h))
    (with-current-buffer buf-b
      (insert "AAAA-BBBB-CXCX-DDDD-EEEE-FXFX-GGGG-HXHX")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h))
    (let* ((results nil)
           (m-a (with-current-buffer buf-a (set-marker (make-marker) 15)))
           (m-b (with-current-buffer buf-b (set-marker (make-marker) 15)))
           (ov-a (with-current-buffer buf-a (make-overlay 6 20)))
           (ov-b (with-current-buffer buf-b (make-overlay 6 20))))
      (with-current-buffer buf-a
        (overlay-put ov-a 'face 'bold)
        (overlay-put ov-a 'priority 5))
      (with-current-buffer buf-b
        (overlay-put ov-b 'face 'italic)
        (overlay-put ov-b 'priority 5))
      (let ((cmp1 (compare-buffer-substrings buf-a 11 15 buf-b 11 15)))
        (push (list "cmp-11-15" cmp1) results)
        (when (/= cmp1 0) (push cmp1 (ds-mm ds))))
      (let ((cmp2 (compare-buffer-substrings buf-a 1 10 buf-b 1 10)))
        (push (list "cmp-1-10" cmp2) results))
      (let ((cmp3 (compare-buffer-substrings buf-a 16 25 buf-b 16 25)))
        (push (list "cmp-16-25" cmp3) results))
      (let ((cmp4 (compare-buffer-substrings buf-a 26 35 buf-b 26 35)))
        (push (list "cmp-26-35" cmp4) results)
        (when (/= cmp4 0) (push cmp4 (ds-mm ds))))
      (setf (ds-td ds) (length (ds-mm ds)))
      (with-current-buffer buf-a
        (goto-char 8)
        (insert "XXX")
        (push "ins-a@8" (ds-log ds)))
      (with-current-buffer buf-b
        (goto-char 12)
        (insert "YYY"))
      (let ((cmp5 (compare-buffer-substrings buf-a 1 20 buf-b 1 20)))
        (push (list "cmp-after-edit" cmp5 (marker-position m-a) (marker-position m-b)) results)
        (when (/= cmp5 0) (push cmp5 (ds-mm ds))))
      (setf (ds-td ds) (length (ds-mm ds)))
      (setq results (reverse results))
      (list results
            (ds-mm ds) (ds-td ds) (ds-log ds)
            (marker-position m-a) (marker-position m-b)))))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_compare_bufsubst_narrow_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cmp-ctx ()
    ((buf-a :initarg :ba :accessor cc-ba :initform nil)
     (buf-b :initarg :bb :accessor cc-bb :initform nil)
     (diff-regions :initarg :dr :accessor cc-dr :initform nil)
     (log :initarg :log :accessor cc-log :initform nil)))
  (defmethod cc-compare ((ctx cmp-ctx) start end)
    (let ((result (compare-buffer-substrings
                   (cc-ba ctx) start end (cc-bb ctx) start end)))
      (push (list start end result) (cc-dr ctx))
      result))
  (let* ((buf-a (generate-new-buffer "cs2a"))
         (buf-b (generate-new-buffer "cs2b"))
         (ctx (cmp-ctx :ba buf-a :bb buf-b :dr nil :log nil)))
    (with-current-buffer buf-a
      (insert "PPPP-QQQQ-RRRR-SSSS-TTTT-UUUU-VVVV-WWWW")
      (setq-local my-cc-log nil))
    (with-current-buffer buf-b
      (insert "PPPP-QQQQ-RXRX-SSSS-TTTT-UXUX-VVVV-WXWX"))
    (let* ((results nil)
           (m-a (with-current-buffer buf-a (set-marker (make-marker) 20)))
           (m-b (with-current-buffer buf-b (set-marker (make-marker) 20)))
           (ov-a (with-current-buffer buf-a (make-overlay 11 25)))
           (ov-b (with-current-buffer buf-b (make-overlay 11 25))))
      (with-current-buffer buf-a
        (overlay-put ov-a 'face 'bold)
        (overlay-put ov-a 'priority 5)
        (put-text-property 1 5 'zone 'p)
        (put-text-property 6 10 'zone 'q)
        (put-text-property 11 15 'zone 'r)
        (put-text-property 16 20 'zone 's)
        (put-text-property 21 25 'zone 't)
        (put-text-property 26 30 'zone 'u)
        (put-text-property 31 35 'zone 'v)
        (put-text-property 36 40 'zone 'w))
      (with-current-buffer buf-b
        (overlay-put ov-b 'face 'italic)
        (overlay-put ov-b 'priority 5))
      (push (list "cmp-full" (cc-compare ctx 1 40)) results)
      (push (list "cmp-match" (cc-compare ctx 1 10)) results)
      (push (list "cmp-diff1" (cc-compare ctx 11 15)) results)
      (push (list "cmp-match2" (cc-compare ctx 16 25)) results)
      (push (list "cmp-diff2" (cc-compare ctx 26 30)) results)
      (with-current-buffer buf-a
        (save-restriction
          (narrow-to-region 6 30)
          (push (list "narrow" (point-min) (point-max)) results)
          (cc-compare ctx 11 15)
          (push (list "narrow-cmp" (cc-dr ctx)) results)
          (goto-char 8)
          (insert "XXX")
          (push "narrow-ins" (cc-log ctx))))
      (push (list "after-narrow"
                  (marker-position m-a) (marker-position m-b)
                  (overlay-start ov-a) (overlay-end ov-a)
                  (overlay-start ov-b) (overlay-end ov-b)) results)
      (let ((cmp-final (cc-compare ctx 1 40)))
        (push (list "final-cmp" cmp-final) results))
      (setq results (reverse results))
      (list results
            (cc-dr ctx) (cc-log ctx)
            (marker-position m-a) (marker-position m-b)))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_compare_bufsubst_multi_region_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"initial-scan\" 5 ((1 5 0) (6 10 -2) (11 15 -2) (16 20 -2) (21 25 0) (26 30 -2) (31 35 -2))) (\"after-edit\" 6 (0 -2 -1 -1 -1 -1 -1) 18 6 28) (\"narrow-cmp\" -2 5 30)) (\"ins@8\") 5 (0 -2 -1 -1 -1 -1 -1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass region-diff ()
    ((region-id :initarg :id :accessor rd-id :initform 0)
     (start :initarg :start :accessor rd-start :initform 0)
     (end :initarg :end :accessor rd-end :initform 0)
     (result :initarg :result :accessor rd-result :initform nil)
     (log :initarg :log :accessor rd-log :initform nil)))
  (let* ((buf-a (generate-new-buffer "cs3a"))
         (buf-b (generate-new-buffer "cs3b"))
         (regions nil)
         (region-specs '((1 5) (6 10) (11 15) (16 20) (21 25) (26 30) (31 35))))
    (with-current-buffer buf-a
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG")
      (dotimes (i 7)
        (let ((s (1+ (* i 5)))
              (e (min (+ 5 (* i 5)) 35)))
          (put-text-property s e 'zone i))))
    (with-current-buffer buf-b
      (insert "AAAA-BXXX-CXCX-DXDX-EEEE-FXFX-GXGX")
      (dotimes (i 7)
        (let ((s (1+ (* i 5)))
              (e (min (+ 5 (* i 5)) 35)))
          (put-text-property s e 'zone i))))
    (dolist (spec region-specs)
      (let ((rd (region-diff :id (car regions)
                             :start (car spec)
                             :end (cadr spec)
                             :result nil :log nil)))
        (push rd regions)))
    (setq regions (reverse regions))
    (let* ((results nil)
           (rd-idx 0)
           (total-diffs 0))
      (dolist (rd regions)
        (let ((cmp (compare-buffer-substrings
                    buf-a (rd-start rd) (rd-end rd)
                    buf-b (rd-start rd) (rd-end rd))))
          (setf (rd-result rd) cmp)
          (when (/= cmp 0)
            (setq total-diffs (1+ total-diffs)))))
      (push (list "initial-scan" total-diffs
                  (mapcar (lambda (rd) (list (rd-start rd) (rd-end rd) (rd-result rd)))
                          regions)) results)
      (with-current-buffer buf-a
        (setq-local my-rd-log nil)
        (let ((ov (make-overlay 6 25))
              (m (set-marker (make-marker) 15)))
          (overlay-put ov 'face 'bold)
          (overlay-put ov 'priority 5)
          (goto-char 8)
          (insert "XXX")
          (push "ins@8" my-rd-log)
          (let ((new-total 0))
            (dolist (rd regions)
              (let ((cmp (compare-buffer-substrings
                          buf-a (rd-start rd) (rd-end rd)
                          buf-b (rd-start rd) (rd-end rd))))
                (setf (rd-result rd) cmp)
                (when (/= cmp 0) (setq new-total (1+ new-total)))))
            (push (list "after-edit" new-total
                        (mapcar (lambda (rd) (rd-result rd)) regions)
                        (marker-position m)
                        (overlay-start ov) (overlay-end ov)) results))
          (save-restriction
            (narrow-to-region 5 30)
            (let ((narrow-cmp (compare-buffer-substrings buf-a 6 20 buf-b 6 20)))
              (push (list "narrow-cmp" narrow-cmp (point-min) (point-max)) results)))
          (setq results (reverse results))
          (list results my-rd-log
                total-diffs
                (mapcar (lambda (rd) (rd-result rd)) regions)))))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_compare_bufsubst_undo_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-diff-tracker ()
    ((label :initarg :label :accessor udt-label :initform "")
     (snapshots :initarg :snaps :accessor udt-snaps :initform nil)
     (log :initarg :log :accessor udt-log :initform nil)))
  (defmethod udt-snap ((tracker undo-diff-tracker) buf-a buf-b start end label)
    (let ((cmp (compare-buffer-substrings buf-a start end buf-b start end)))
      (push (list label start end cmp) (udt-snaps tracker))
      cmp))
  (let* ((buf-a (generate-new-buffer "cs4a"))
         (buf-b (generate-new-buffer "cs4b"))
         (tracker (undo-diff-tracker :label "undo-diff" :snaps nil :log nil)))
    (with-current-buffer buf-a
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (setq-local my-udt-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (with-current-buffer buf-b
          (insert "AAAA-BBBB-CXCX-DDDD-EEEE-FXFX-GGGG-HXHX"))
        (udt-snap tracker buf-a buf-b 1 40 "init")
        (push (list "init" (udt-snaps tracker)) results)
        (goto-char 8)
        (insert "XXX")
        (push "ins@8" (udt-log tracker))
        (setq my-udt-log (cons "ins@8" my-udt-log))
        (udt-snap tracker buf-a buf-b 1 40 "after-ins")
        (push (list "after-ins" (udt-snaps tracker)
                    (marker-position m)) results)
        (undo-boundary)
        (goto-char 12)
        (insert "YYY")
        (push "ins@12" (udt-log tracker))
        (setq my-udt-log (cons "ins@12" my-udt-log))
        (udt-snap tracker buf-a buf-b 1 40 "after-ins2")
        (push (list "after-ins2" (udt-snaps tracker)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 35)
          (udt-snap tracker buf-a buf-b 6 30 "narrow")
          (push (list "narrow-snap" (udt-snaps tracker)
                      (marker-position m)) results))
        (setq results (reverse results))
        (list results
              (udt-log tracker)
              my-udt-log
              (marker-position m)
              (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_compare_bufsubst_class_hierarchy_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass text-source ()
    ((name :initarg :name :accessor ts-name :initform "")
     (buf :initarg :buf :accessor ts-buf :initform nil)
     (log :initarg :log :accessor ts-log :initform nil)))
  (defclass modified-source (text-source)
    ((mod-count :initarg :mc :accessor ms-mc :initform 0)))
  (defclass pristine-source (text-source)
    ((original-md5 :initarg :md5 :accessor ps-md5 :initform "")))
  (defmethod ts-compare ((src text-source) other-buf start end)
    (compare-buffer-substrings (ts-buf src) start end other-buf start end))
  (defmethod ts-compare ((src modified-source) other-buf start end)
    (setf (ms-mc src) (1+ (ms-mc src)))
    (push (format "compare@%d-%d:mc=%d" start end (ms-mc src)) (ts-log src))
    (cl-call-next-method))
  (defmethod ts-compare ((src pristine-source) other-buf start end)
    (push (format "pristine-compare@%d-%d" start end) (ts-log src))
    (cl-call-next-method))
  (let* ((buf-a (generate-new-buffer "cs5a"))
         (buf-b (generate-new-buffer "cs5b"))
         (buf-c (generate-new-buffer "cs5c"))
         (src-mod (modified-source :name "mod" :buf buf-a :log nil :mc 0))
         (src-pri (pristine-source :name "pri" :buf buf-b :log nil :md5 "abc")))
    (with-current-buffer buf-a
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (dotimes (i 8)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) 40)
                           'zone i)))
    (with-current-buffer buf-b
      (insert "AAAA-BBBB-CXCX-DDDD-EEEE-FXFX-GGGG-HXHX"))
    (with-current-buffer buf-c
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH"))
    (let* ((results nil)
           (m-a (with-current-buffer buf-a (set-marker (make-marker) 15)))
           (ov-a (with-current-buffer buf-a (make-overlay 6 25))))
      (with-current-buffer buf-a
        (overlay-put ov-a 'face 'bold)
        (overlay-put ov-a 'priority 5)
        (setq-local my-ts-log nil))
      (push (list "mod-vs-b" (ts-compare src-mod buf-b 1 40)) results)
      (push (list "mod-vs-c" (ts-compare src-mod buf-c 1 40)) results)
      (push (list "pri-vs-c" (ts-compare src-pri buf-c 1 40)) results)
      (push (list "pri-vs-b" (ts-compare src-pri buf-b 1 40)) results)
      (push (list "logs" (ts-log src-mod) (ts-log src-pri)) results)
      (with-current-buffer buf-a
        (goto-char 8)
        (insert "XXX")
        (setq my-ts-log (cons "ins@8" my-ts-log)))
      (push (list "after-edit"
                  (ts-compare src-mod buf-c 1 40)
                  (ms-mc src-mod)
                  (marker-position m-a)
                  (overlay-start ov-a) (overlay-end ov-a)) results)
      (save-restriction
        (with-current-buffer buf-a
          (narrow-to-region 5 35)
          (push (list "narrow-cmp"
                      (ts-compare src-mod buf-c 6 30)
                      (ms-mc src-mod)
                      (point-min) (point-max)) results)))
      (push (list "mc-final" (ms-mc src-mod)) results)
      (setq results (reverse results))
      (list results
            my-ts-log
            (ts-log src-mod) (ts-log src-pri)
            (cl-typep src-mod 'modified-source)
            (cl-typep src-pri 'pristine-source)
            (cl-typep src-mod 'text-source)
            (cl-typep src-pri 'text-source)))))"#,
        expect,
    );
}
