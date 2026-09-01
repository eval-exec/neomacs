//! Combo: deep nesting eval chains (save-excursion, unwind-protect,
//! condition-case, dynamic let-binding) + EIEIO state tracking + overlays
//! + markers + textprop + buflocal + narrow + undo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_deep_excursion_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass nest-ctx ()
    ((buf-name :initarg :buf :accessor nc-buf :initform "")
     (depth :initarg :depth :accessor nc-depth :initform 0)
     (max-depth :initarg :max :accessor nc-max :initform 0)
     (edits :initarg :edits :accessor nc-edits :initform nil)
     (log :initarg :log :accessor nc-log :initform nil)))
  (defmethod nc-edit ((ctx nest-ctx) pos str)
    (with-current-buffer (nc-buf ctx)
      (setf (nc-depth ctx) (1+ (nc-depth ctx)))
      (setf (nc-max ctx) (max (nc-max ctx) (nc-depth ctx)))
      (goto-char pos)
      (insert str)
      (push (format "d%d@%d:%S" (nc-depth ctx) pos str) (nc-edits ctx))
      (setf (nc-depth ctx) (1- (nc-depth ctx)))))
  (let* ((buf (generate-new-buffer "dn1"))
         (ctx (nest-ctx :buf (buffer-name buf) :depth 0 :max 0 :edits nil :log nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (dotimes (i 10)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) 50)
                           'zone i))
      (setq-local my-nc-log nil)
      (let* ((ov (make-overlay 6 40))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 20)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (nc-edit ctx 8 "A")
        (push (list "d0" (marker-position m) (nc-edits ctx)) results)
        (save-excursion
          (nc-edit ctx 15 "B")
          (push (list "d1a" (marker-position m) (nc-edits ctx)) results)
          (save-excursion
            (nc-edit ctx 25 "C")
            (push (list "d2" (marker-position m) (nc-edits ctx)) results)
            (save-excursion
              (nc-edit ctx 35 "D")
              (push (list "d3" (marker-position m) (nc-edits ctx)) results))
            (push (list "d2-back" (marker-position m)) results))
          (save-excursion
            (nc-edit ctx 30 "E")
            (push (list "d1b" (marker-position m) (nc-edits ctx)) results)
            (save-restriction
              (narrow-to-region 5 55)
              (nc-edit ctx 10 "F")
              (push (list "d2-narrow" (marker-position m)
                          (point-min) (point-max)) results)))
          (push (list "d1-after" (marker-position m)) results))
        (push (list "d0-back" (marker-position m)) results)
        (nc-edit ctx 40 "G")
        (push (list "final" (marker-position m) (nc-edits ctx)
                    (nc-max ctx) (nc-depth ctx)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S max=%d"
                       results (nc-max ctx)))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (nc-max ctx) (nc-edits ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-nc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_deep_unwind_protect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass guard-ctx ()
    ((buf-name :initarg :buf :accessor gc-buf :initform "")
     (guards :initarg :guards :accessor gc-guards :initform nil)
     (body-log :initarg :body :accessor gc-body :initform nil)
     (cleanup-log :initarg :cleanup :accessor gc-cleanup :initform nil)
     (errors :initarg :errors :accessor gc-errors :initform nil)))
  (defmethod gc-do-edit ((ctx guard-ctx) pos str guard-label)
    (unwind-protect
        (with-current-buffer (gc-buf ctx)
          (push guard-label (gc-guards ctx))
          (push (format "body:%s@%d" guard-label pos) (gc-body ctx))
          (goto-char pos)
          (insert str)
          (when (equal str "ERR")
            (error "forced error at %d" pos)))
      (push (format "cleanup:%s" guard-label) (gc-cleanup ctx))))
  (let* ((buf (generate-new-buffer "dn2"))
         (ctx (guard-ctx :buf (buffer-name buf) :guards nil :body nil :cleanup nil :errors nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (setq-local my-gc-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (gc-do-edit ctx 8 "XXX" "g1")
        (push (list "g1" (gc-body ctx) (gc-cleanup ctx)
                    (marker-position m)) results)
        (condition-case err
            (gc-do-edit ctx 15 "ERR" "g2")
          (error
           (push (format "caught:%S" err) (gc-errors ctx))))
        (push (list "g2-err" (gc-body ctx) (gc-cleanup ctx) (gc-errors ctx)
                    (marker-position m)) results)
        (gc-do-edit ctx 25 "YYY" "g3")
        (push (list "g3" (gc-body ctx) (gc-cleanup ctx)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 40)
          (condition-case err
              (gc-do-edit ctx 10 "ERR" "g4-narrow")
            (error
             (push (format "caught-narrow:%S" err) (gc-errors ctx))))
          (push (list "g4-narrow" (gc-body ctx) (gc-cleanup ctx) (gc-errors ctx)
                      (marker-position m) (point-min) (point-max)) results))
        (gc-do-edit ctx 35 "ZZZ" "g5")
        (push (list "g5" (gc-body ctx) (gc-cleanup ctx) (gc-errors ctx)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (gc-guards ctx) (gc-body ctx) (gc-cleanup ctx) (gc-errors ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-gc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_deep_dynamic_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass dyn-ctx ()
    ((buf-name :initarg :buf :accessor dc-buf :initform "")
     (val-stack :initarg :stack :accessor dc-stack :initform nil)
     (log :initarg :log :accessor dc-log :initform nil)))
  (defvar dyn-test-val 0)
  (defmethod dc-push-val ((ctx dyn-ctx))
    (push dyn-test-val (dc-stack ctx))
    (push (format "push:%d" dyn-test-val) (dc-log ctx))
    dyn-test-val)
  (let* ((buf (generate-new-buffer "dn3"))
         (ctx (dyn-ctx :buf (buffer-name buf) :stack nil :log nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (setq-local my-dc-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (setq dyn-test-val 1)
        (dc-push-val ctx)
        (push (list "outer-1" dyn-test-val (dc-stack ctx)) results)
        (let ((dyn-test-val 10))
          (dc-push-val ctx)
          (push (list "inner-10" dyn-test-val (dc-stack ctx)) results)
          (goto-char 8)
          (insert "XXX")
          (let ((dyn-test-val 100))
            (dc-push-val ctx)
            (push (list "deep-100" dyn-test-val (dc-stack ctx)
                        (marker-position m)) results)
            (save-restriction
              (narrow-to-region 5 40)
              (let ((dyn-test-val 1000))
                (dc-push-val ctx)
                (push (list "narrow-1000" dyn-test-val (dc-stack ctx)
                            (marker-position m) (point-min) (point-max)) results))))
          (dc-push-val ctx)
          (push (list "inner-10-back" dyn-test-val (dc-stack ctx)
                      (marker-position m)) results))
        (dc-push-val ctx)
        (push (list "outer-1-back" dyn-test-val (dc-stack ctx)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S stack=%S"
                       results (reverse (dc-stack ctx))))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (dc-stack ctx) (dc-log ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-dc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_deep_condition_case_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass err-ctx ()
    ((buf-name :initarg :buf :accessor ec-buf :initform "")
     (caught :initarg :caught :accessor ec-caught :initform nil)
     (successful :initarg :ok :accessor ec-ok :initform nil)
     (log :initarg :log :accessor ec-log :initform nil)))
  (defmethod ec-try-edit ((ctx err-ctx) pos str should-err)
    (condition-case err
        (with-current-buffer (ec-buf ctx)
          (goto-char pos)
          (insert str)
          (push (format "ok@%d:%S" pos str) (ec-ok ctx))
          (when should-err
            (error "edit-error@%d" pos))
          t)
      (error
       (push (format "caught:%S" err) (ec-caught ctx))
       nil)))
  (let* ((buf (generate-new-buffer "dn4"))
         (ctx (err-ctx :buf (buffer-name buf) :caught nil :ok nil :log nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (setq-local my-ec-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "try1" (ec-try-edit ctx 8 "XXX" nil)
                    (ec-ok ctx) (ec-caught ctx)
                    (marker-position m)) results)
        (push (list "try2" (ec-try-edit ctx 15 "YYY" t)
                    (ec-ok ctx) (ec-caught ctx)
                    (marker-position m)) results)
        (condition-case outer-err
            (progn
              (ec-try-edit ctx 25 "ZZZ" nil)
              (condition-case inner-err
                  (ec-try-edit ctx 30 "ERR" t)
                (error
                 (push (format "inner:%S" inner-err) (ec-log ctx))
                 (ec-try-edit ctx 35 "WWW" nil)))
              (error "outer-never"))
          (error
           (push (format "outer:%S" outer-err) (ec-log ctx))))
        (push (list "nested" (ec-ok ctx) (ec-caught ctx) (ec-log ctx)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 45)
          (condition-case narrow-err
              (ec-try-edit ctx 10 "NNN" t)
            (error
             (push (format "narrow:%S" narrow-err) (ec-log ctx))))
          (push (list "narrow" (ec-ok ctx) (ec-caught ctx) (ec-log ctx)
                      (marker-position m) (point-min) (point-max)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (ec-ok ctx) (ec-caught ctx) (ec-log ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ec-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_deep_mixed_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mixed-ctx ()
    ((buf-name :initarg :buf :accessor mx-buf :initform "")
     (ops :initarg :ops :accessor mx-ops :initform nil)
     (depth-track :initarg :depth :accessor mx-depth :initform nil)
     (log :initarg :log :accessor mx-log :initform nil)))
  (defvar mx-depth-counter 0)
  (defmethod mx-record ((ctx mixed-ctx) label)
    (push (format "%s:d%d" label mx-depth-counter) (mx-ops ctx))
    (push mx-depth-counter (mx-depth-track ctx)))
  (let* ((buf (generate-new-buffer "dn5"))
         (ctx (mixed-ctx :buf (buffer-name buf) :ops nil :depth nil :log nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ-KKKK-LLLL")
      (dotimes (i 12)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) 60)
                           'zone i))
      (setq-local my-mx-log nil)
      (let* ((ov (make-overlay 6 50))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 25)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (setq mx-depth-counter 0)
        (mx-record ctx "root")
        (push (list "root" mx-depth-counter (mx-ops ctx)) results)
        (save-excursion
          (let ((mx-depth-counter (1+ mx-depth-counter)))
            (mx-record ctx "se1")
            (goto-char 8)
            (insert "A")
            (save-excursion
              (let ((mx-depth-counter (1+ mx-depth-counter)))
                (mx-record ctx "se2")
                (goto-char 15)
                (insert "B")
                (save-restriction
                  (narrow-to-region 5 55)
                  (let ((mx-depth-counter (1+ mx-depth-counter)))
                    (mx-record ctx "se2-nr")
                    (goto-char 10)
                    (insert "C"))
                  (unwind-protect
                      (let ((mx-depth-counter (1+ mx-depth-counter)))
                        (mx-record ctx "se2-nr-up")
                        (goto-char 12)
                        (insert "D")
                        (setq my-mx-log (cons "deep-ins" my-mx-log)))
                    (mx-record ctx "se2-nr-cleanup")))
                (mx-record ctx "se2-back")))
              (mx-record ctx "se1-inner"))
            (condition-case err
                (let ((mx-depth-counter (1+ mx-depth-counter)))
                  (mx-record ctx "se1-err")
                  (goto-char 30)
                  (insert "E")
                  (error "test-error"))
              (error
               (mx-record ctx "se1-caught")))
            (mx-record ctx "se1-back"))
          (mx-record ctx "se0-back"))
        (mx-record ctx "root-back")
        (push (list "final" mx-depth-counter (mx-ops ctx) (mx-depth-track ctx)
                    (marker-position m)
                    (overlay-start ov) (overlay-end ov)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ops=%S"
                       results (reverse (mx-ops ctx))))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (mx-ops ctx) (mx-depth-track ctx)
              (marker-position m)
              my-mx-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
