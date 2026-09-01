//! Combo: advice-add/advice-remove on EIEIO generic functions (round 2)
//! + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests :before/:after/:around/:override/:filter-args advice on generics
//! with complex multi-method dispatch and buffer editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_adv2_before_after_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass adv2-ctx ()
    ((buf-name :initarg :buf :accessor a2-buf :initform "")
     (log :initarg :log :accessor a2-log :initform nil)
     (call-count :initarg :count :accessor a2-count :initform 0)))
  (defclass adv2-ctx-sub (adv2-ctx)
    ((tag :initarg :tag :accessor a2s-tag :initform "")))
  (defmethod a2-edit ((ctx adv2-ctx) pos str)
    (with-current-buffer (a2-buf ctx)
      (goto-char pos)
      (insert str)
      (push (format "primary@%d" pos) (a2-log ctx))
      (setf (a2-count ctx) (1+ (a2-count ctx)))))
  (defmethod a2-edit ((ctx adv2-ctx-sub) pos str)
    (with-current-buffer (a2-buf ctx)
      (goto-char pos)
      (insert str)
      (push (format "sub-primary@%d" pos) (a2-log ctx))
      (setf (a2-count ctx) (1+ (a2-count ctx)))))
  (let* ((buf (generate-new-buffer "ag2_1"))
         (ctx (adv2-ctx-sub :buf (buffer-name buf) :log nil :count 0 :tag "sub"))
         (adv-log nil)
         (adv-before
          (lambda (pos str)
            (push (format "before@%d:%S" pos str) adv-log)))
         (adv-after
          (lambda (pos str)
            (push (format "after@%d:%S" pos str) adv-log))))
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
      (setq-local my-a2-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (advice-add 'a2-edit :before
                    (lambda (ctx pos str) (funcall adv-before pos str))
                    '((name . a2-adv-before)))
        (advice-add 'a2-edit :after
                    (lambda (ctx pos str) (funcall adv-after pos str))
                    '((name . a2-adv-after)))
        (a2-edit ctx 8 "XXX")
        (push (list "edit1" (a2-count ctx) (a2-log ctx) adv-log
                    (marker-position m)) results)
        (a2-edit ctx 15 "YYY")
        (push (list "edit2" (a2-count ctx) (a2-log ctx) adv-log
                    (marker-position m)) results)
        (advice-remove 'a2-edit '((name . a2-adv-before)))
        (a2-edit ctx 20 "ZZZ")
        (push (list "edit3-no-before" (a2-count ctx) (a2-log ctx) adv-log
                    (marker-position m)) results)
        (advice-remove 'a2-edit '((name . a2-adv-after)))
        (a2-edit ctx 25 "WWW")
        (push (list "edit4-no-advice" (a2-count ctx) (a2-log ctx) adv-log
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'a2-log t)
        (list (buffer-string)
              (a2-count ctx)
              (length adv-log)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-a2-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_adv2_around_wrapper() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass a2r-ctx ()
    ((buf-name :initarg :buf :accessor a2r-buf :initform "")
     (log :initarg :log :accessor a2r-log :initform nil)
     (wrap-depth :initarg :depth :accessor a2r-depth :initform 0)))
  (defmethod a2r-do ((ctx a2r-ctx) pos str)
    (with-current-buffer (a2r-buf ctx)
      (goto-char pos)
      (insert str)
      (push (format "primary@%d" pos) (a2r-log ctx))))
  (let* ((buf (generate-new-buffer "ag2_2"))
         (ctx (a2r-ctx :buf (buffer-name buf) :log nil :depth 0))
         (outer-called nil)
         (inner-called nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h)
      (setq-local my-a2r-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (advice-add 'a2r-do :around
                    (lambda (fn ctx pos str)
                      (push "outer-start" outer-called)
                      (setf (a2r-depth ctx) (1+ (a2r-depth ctx)))
                      (funcall fn ctx pos str)
                      (setf (a2r-depth ctx) (1+ (a2r-depth ctx)))
                      (push "outer-end" outer-called))
                    '((name . a2r-outer)))
        (advice-add 'a2r-do :around
                    (lambda (fn ctx pos str)
                      (push "inner-start" inner-called)
                      (setf (a2r-depth ctx) (+ 10 (a2r-depth ctx)))
                      (funcall fn ctx pos str)
                      (setf (a2r-depth ctx) (+ 10 (a2r-depth ctx)))
                      (push "inner-end" inner-called))
                    '((name . a2r-inner)))
        (a2r-do ctx 8 "XXX")
        (push (list "edit1" (a2r-depth ctx) (a2r-log ctx)
                    outer-called inner-called
                    (marker-position m)) results)
        (advice-remove 'a2r-do '((name . a2r-inner)))
        (a2r-do ctx 15 "YYY")
        (push (list "edit2-no-inner" (a2r-depth ctx) (a2r-log ctx)
                    outer-called inner-called
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 35)
          (a2r-do ctx 10 "ZZZ")
          (push (list "narrow-edit" (a2r-depth ctx) (a2r-log ctx)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S outer=%S inner=%S"
                       results outer-called inner-called))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'a2r-log t)
        (list (buffer-string)
              (a2r-depth ctx)
              (length outer-called) (length inner-called)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-a2r-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_adv2_override_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass a2o-ctx ()
    ((buf-name :initarg :buf :accessor a2o-buf :initform "")
     (log :initarg :log :accessor a2o-log :initform nil)
     (override-count :initarg :oc :accessor a2o-oc :initform 0)))
  (defmethod a2o-do ((ctx a2o-ctx) pos str)
    (with-current-buffer (a2o-buf ctx)
      (goto-char pos)
      (insert str)
      (push (format "primary@%d:%S" pos str) (a2o-log ctx))))
  (let* ((buf (generate-new-buffer "ag2_3"))
         (ctx (a2o-ctx :buf (buffer-name buf) :log nil :oc 0))
         (override-log nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-a2o-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (a2o-do ctx 8 "AAA")
        (push (list "normal" (a2o-log ctx)
                    (buffer-string) (marker-position m)) results)
        (advice-add 'a2o-do :override
                    (lambda (ctx pos str)
                      (push (format "override@%d:%S" pos str) override-log)
                      (setf (a2o-oc ctx) (1+ (a2o-oc ctx))))
                    '((name . a2o-ov)))
        (a2o-do ctx 12 "BBB")
        (push (list "override1" (a2o-log ctx) override-log
                    (a2o-oc ctx)
                    (buffer-string) (marker-position m)) results)
        (a2o-do ctx 16 "CCC")
        (push (list "override2" (a2o-log ctx) override-log
                    (a2o-oc ctx)
                    (buffer-string) (marker-position m)) results)
        (advice-remove 'a2o-do '((name . a2o-ov)))
        (a2o-do ctx 20 "DDD")
        (push (list "restored" (a2o-log ctx) override-log
                    (a2o-oc ctx)
                    (buffer-string) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S override-log=%S"
                       results override-log))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'a2o-log t)
        (list (buffer-string)
              (a2o-oc ctx)
              (length (a2o-log ctx))
              (length override-log)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-a2o-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_adv2_multi_fn_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass a2c-obj ()
    ((name :initarg :name :accessor a2c-name :initform "")
     (log :initarg :log :accessor a2c-log :initform nil)
     (val :initarg :val :accessor a2c-val :initform 0)))
  (defmethod a2c-transform ((obj a2c-obj) delta)
    (setf (a2c-val obj) (+ (a2c-val obj) delta))
    (push (format "transform:+%d=%d" delta (a2c-val obj)) (a2c-log obj)))
  (defmethod a2c-reset ((obj a2c-obj))
    (setf (a2c-val obj) 0)
    (push "reset" (a2c-log obj)))
  (let* ((buf (generate-new-buffer "ag2_4"))
         (obj (a2c-obj :name "chain" :log nil :val 100))
         (advice-log nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h)
      (put-text-property 41 45 'zone 'i)
      (put-text-property 46 50 'zone 'j)
      (setq-local my-a2c-log nil)
      (let* ((ov (make-overlay 6 40))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 20))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (advice-add 'a2c-transform :before
                    (lambda (obj delta)
                      (push (format "before-tf:+%d" delta) advice-log))
                    '((name . a2c-btf)))
        (advice-add 'a2c-transform :after
                    (lambda (obj delta)
                      (push (format "after-tf:+%d->%d" delta (a2c-val obj)) advice-log))
                    '((name . a2c-atf)))
        (advice-add 'a2c-reset :before
                    (lambda (obj)
                      (push (format "before-reset:%d" (a2c-val obj)) advice-log))
                    '((name . a2c-brs)))
        (advice-add 'a2c-reset :after
                    (lambda (obj)
                      (push (format "after-reset:%d" (a2c-val obj)) advice-log))
                    '((name . a2c-ars)))
        (a2c-transform obj 10)
        (push (list "tf1" (a2c-val obj) (a2c-log obj) advice-log) results)
        (a2c-transform obj 25)
        (push (list "tf2" (a2c-val obj) (a2c-log obj) advice-log) results)
        (goto-char 12)
        (insert "XXX")
        (a2c-reset obj)
        (push (list "reset" (a2c-val obj) (a2c-log obj) advice-log
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 45)
          (a2c-transform obj 50)
          (push (list "narrow-tf" (a2c-val obj) (a2c-log obj) advice-log
                      (marker-position m)) results))
        (advice-remove 'a2c-transform '((name . a2c-btf)))
        (advice-remove 'a2c-transform '((name . a2c-atf)))
        (a2c-transform obj 5)
        (push (list "tf-no-adv" (a2c-val obj) (a2c-log obj) advice-log) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S advice-log=%S"
                       results (reverse advice-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'a2c-log t)
        (list (buffer-string)
              (a2c-val obj)
              (a2c-log obj)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-a2c-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_adv2_filter_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass a2f-ctx ()
    ((buf-name :initarg :buf :accessor a2f-buf :initform "")
     (log :initarg :log :accessor a2f-log :initform nil)
     (multiplier :initarg :mult :accessor a2f-mult :initform 1)
     (total-edits :initarg :total :accessor a2f-total :initform 0)))
  (defmethod a2f-insert ((ctx a2f-ctx) pos str repeat)
    (with-current-buffer (a2f-buf ctx)
      (dotimes (_ repeat)
        (goto-char pos)
        (insert str)
        (setf (a2f-total ctx) (1+ (a2f-total ctx))))
      (push (format "insert@%d:%S*x%d" pos str repeat) (a2f-log ctx))))
  (let* ((buf (generate-new-buffer "ag2_5"))
         (ctx (a2f-ctx :buf (buffer-name buf) :log nil :mult 1 :total 0))
         (filter-log nil))
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
      (setq-local my-a2f-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (advice-add 'a2f-insert :filter-args
                    (lambda (ctx pos str repeat)
                      (let ((new-repeat (* repeat (a2f-mult ctx))))
                        (push (format "filter:%d->%d" repeat new-repeat) filter-log)
                        (list ctx pos str new-repeat)))
                    '((name . a2f-flt)))
        (a2f-insert ctx 8 "X" 1)
        (push (list "ins1" (a2f-total ctx) (a2f-log ctx) filter-log
                    (marker-position m)) results)
        (setf (a2f-mult ctx) 3)
        (a2f-insert ctx 15 "Y" 2)
        (push (list "ins2-mult3" (a2f-total ctx) (a2f-log ctx) filter-log
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 40)
          (setf (a2f-mult ctx) 2)
          (a2f-insert ctx 10 "Z" 1)
          (push (list "narrow-ins" (a2f-total ctx) (a2f-log ctx) filter-log
                      (marker-position m)) results))
        (advice-remove 'a2f-insert '((name . a2f-flt)))
        (setf (a2f-mult ctx) 1)
        (a2f-insert ctx 20 "W" 1)
        (push (list "no-filter" (a2f-total ctx) (a2f-log ctx) filter-log
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S filter-log=%S"
                       results (reverse filter-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'a2f-log t)
        (list (buffer-string)
              (a2f-total ctx)
              (a2f-mult ctx)
              (length filter-log)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-a2f-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
