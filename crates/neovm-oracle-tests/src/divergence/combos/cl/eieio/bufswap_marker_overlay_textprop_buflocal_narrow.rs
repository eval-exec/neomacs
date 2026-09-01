//! Combo: buffer-swap-text + EIEIO multi-buffer state tracking + overlays
//! + markers + textprop + buflocal + narrow + undo.
//! Tests cross-buffer text swapping with EIEIO objects managing state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_bufswap_basic_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"init\" \"AAAA-BBBB-CCCC-DDDD\" \"XXXX-YYYY-ZZZZ-WWWW\" 10 10 6 15 6 15) (\"swap1\" \"XXXX-YYYY-ZZZZ-WWWW\" \"AAAA-BBBB-CCCC-DDDD\" 1 1) (\"edit-after-swap\" \"XXXX-YYNEWYY-ZZZZ-WWWW\" \"AAAA-BBBB-CCCC-DDDD\" 1 1)) (\"ins-a\") nil 1 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass buf-state ()
    ((label :initarg :label :accessor bs-label :initform "")
     (edit-count :initarg :count :accessor bs-count :initform 0)
     (last-pos :initarg :pos :accessor bs-pos :initform 0)
     (log :initarg :log :accessor bs-log :initform nil)))
  (let* ((buf-a (generate-new-buffer "bs1a"))
         (buf-b (generate-new-buffer "bs1b"))
         (sa (buf-state :label "A" :count 0 :pos 0 :log nil))
         (sb (buf-state :label "B" :count 0 :pos 0 :log nil)))
    (with-current-buffer buf-a
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'buf 'a)
      (put-text-property 6 10 'buf 'a)
      (put-text-property 11 15 'buf 'a)
      (put-text-property 16 20 'buf 'a))
    (with-current-buffer buf-b
      (insert "XXXX-YYYY-ZZZZ-WWWW")
      (put-text-property 1 5 'buf 'b)
      (put-text-property 6 10 'buf 'b)
      (put-text-property 11 15 'buf 'b)
      (put-text-property 16 20 'buf 'b))
    (let* ((results nil)
           (m-a (with-current-buffer buf-a (set-marker (make-marker) 10)))
           (m-b (with-current-buffer buf-b (set-marker (make-marker) 10)))
           (ov-a (with-current-buffer buf-a (make-overlay 6 15)))
           (ov-b (with-current-buffer buf-b (make-overlay 6 15))))
      (with-current-buffer buf-a
        (overlay-put ov-a 'face 'bold)
        (overlay-put ov-a 'priority 5))
      (with-current-buffer buf-b
        (overlay-put ov-b 'face 'italic)
        (overlay-put ov-b 'priority 5))
      (push (list "init"
                  (with-current-buffer buf-a (buffer-substring-no-properties 1 (point-max)))
                  (with-current-buffer buf-b (buffer-substring-no-properties 1 (point-max)))
                  (marker-position m-a) (marker-position m-b)
                  (overlay-start ov-a) (overlay-end ov-a)
                  (overlay-start ov-b) (overlay-end ov-b)) results)
      (with-current-buffer buf-a
        (setq buffer-undo-list nil)
        (undo-boundary)
        (buffer-swap-text buf-b))
      (setf (bs-count sa) (1+ (bs-count sa)))
      (setf (bs-count sb) (1+ (bs-count sb)))
      (push (list "swap1"
                  (with-current-buffer buf-a (buffer-substring-no-properties 1 (point-max)))
                  (with-current-buffer buf-b (buffer-substring-no-properties 1 (point-max)))
                  (bs-count sa) (bs-count sb)) results)
      (with-current-buffer buf-a
        (goto-char 8)
        (insert "NEW")
        (setf (bs-pos sa) (point))
        (push "ins-a" (bs-log sa)))
      (push (list "edit-after-swap"
                  (with-current-buffer buf-a (buffer-substring-no-properties 1 (point-max)))
                  (with-current-buffer buf-b (buffer-substring-no-properties 1 (point-max)))
                  (bs-count sa) (bs-count sb)) results)
      (setq results (reverse results))
      (list results
            (bs-log sa) (bs-log sb)
            (bs-count sa) (bs-count sb)))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_bufswap_with_narrow_and_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function bs-count)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass swap-ctx ()
    ((name :initarg :name :accessor scx-name :initform "")
     (ops :initarg :ops :accessor scx-ops :initform nil)
     (data :initarg :data :accessor scx-data :initform nil)))
  (let* ((buf-x (generate-new-buffer "bs2x"))
         (buf-y (generate-new-buffer "bs2y"))
         (ctx (swap-ctx :name "swap" :ops nil :data nil)))
    (with-current-buffer buf-x
      (insert "PPPP-QQQQ-RRRR-SSSS-TTTT-UUUU")
      (dotimes (i 6)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) (point-max))
                           'zone (1+ i))))
    (with-current-buffer buf-y
      (insert "1111-2222-3333-4444-5555-6666")
      (dotimes (i 6)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) (point-max))
                           'zone (1+ i))))
    (let* ((results nil)
           (m-x (with-current-buffer buf-x (set-marker (make-marker) 15)))
           (m-y (with-current-buffer buf-y (set-marker (make-marker) 15)))
           (ov-x (with-current-buffer buf-x (make-overlay 6 20)))
           (ov-y (with-current-buffer buf-y (make-overlay 6 20))))
      (with-current-buffer buf-x
        (overlay-put ov-x 'face 'bold)
        (overlay-put ov-x 'priority 5))
      (with-current-buffer buf-y
        (overlay-put ov-y 'face 'italic)
        (overlay-put ov-y 'priority 5))
      (push (list "init"
                  (with-current-buffer buf-x (buffer-string))
                  (with-current-buffer buf-y (buffer-string))
                  (marker-position m-x) (marker-position m-y)) results)
      (with-current-buffer buf-x
        (setq buffer-undo-list nil)
        (undo-boundary)
        (buffer-swap-text buf-y)
        (push "swap" (scx-ops ctx)))
      (push (list "swap1"
                  (with-current-buffer buf-x (buffer-string))
                  (with-current-buffer buf-y (buffer-string))
                  (marker-position m-x) (marker-position m-y)
                  (overlay-start ov-x) (overlay-end ov-x)
                  (overlay-start ov-y) (overlay-end ov-y)) results)
      (with-current-buffer buf-x
        (save-restriction
          (narrow-to-region 5 25)
          (push (list "narrow-x" (point-min) (point-max)) results)
          (goto-char 8)
          (insert "INS")
          (push "narrow-ins" (scx-ops ctx)))
        (push (list "after-narrow"
                    (buffer-string)
                    (marker-position m-x)) results))
      (setf (scx-data ctx)
            (list (with-current-buffer buf-x (buffer-string))
                  (with-current-buffer buf-y (buffer-string))
                  (marker-position m-x) (marker-position m-y)
                  (bs-count (scx-data ctx))))
      (setq results (reverse results))
      (list results
            (scx-ops ctx)
            (scx-data ctx)))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_bufswap_cross_edit_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"init\" \"AAAA-BBBB-CCCC-DDDD-EEEE-FFFF\" \"1111-2222-3333-4444-5555-6666\" 10 10 6 20 6 20) (\"swap\" \"1111-2222-3333-4444-5555-6666\" \"AAAA-BBBB-CCCC-DDDD-EEEE-FFFF\" (\"swap\")) (\"edits\" \"1111-22XXX22-3333-4444-5555-6666\" \"AAAA-BBBB-CYYYCCC-DDDD-EEEE-FFFF\" (\"ins-a\" \"swap\") (\"ins-b\"))) (\"ins\" \"swap\") (\"ins-a\" \"swap\") (\"ins-b\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cross-edit ()
    ((source :initarg :src :accessor ce-src :initform "")
     (target :initarg :tgt :accessor ce-tgt :initform "")
     (edits :initarg :edits :accessor ce-edits :initform nil)
     (shared-ref :allocation :class :accessor ce-ref :initform nil)))
  (let* ((buf-a (generate-new-buffer "bs3a"))
         (buf-b (generate-new-buffer "bs3b"))
         (ea (cross-edit :src "A" :tgt "B" :edits nil))
         (eb (cross-edit :src "B" :tgt "A" :edits nil)))
    (with-current-buffer buf-a
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight))
    (with-current-buffer buf-b
      (insert "1111-2222-3333-4444-5555-6666")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight))
    (let* ((results nil)
           (m-a (with-current-buffer buf-a (set-marker (make-marker) 10)))
           (m-b (with-current-buffer buf-b (set-marker (make-marker) 10)))
           (ov-a (with-current-buffer buf-a (make-overlay 6 20)))
           (ov-b (with-current-buffer buf-b (make-overlay 6 20))))
      (with-current-buffer buf-a
        (overlay-put ov-a 'face 'bold)
        (overlay-put ov-a 'priority 5)
        (setq-local my-ce-log nil))
      (with-current-buffer buf-b
        (overlay-put ov-b 'face 'italic)
        (overlay-put ov-b 'priority 5))
      (setf (ce-ref ea) m-a)
      (push (list "init"
                  (with-current-buffer buf-a (buffer-substring-no-properties 1 (point-max)))
                  (with-current-buffer buf-b (buffer-substring-no-properties 1 (point-max)))
                  (marker-position m-a) (marker-position m-b)
                  (overlay-start ov-a) (overlay-end ov-a)
                  (overlay-start ov-b) (overlay-end ov-b)) results)
      (with-current-buffer buf-a
        (setq buffer-undo-list nil)
        (undo-boundary)
        (buffer-swap-text buf-b)
        (push "swap" (ce-edits ea))
        (setq my-ce-log (cons "swap" my-ce-log)))
      (push (list "swap"
                  (with-current-buffer buf-a (buffer-substring-no-properties 1 (point-max)))
                  (with-current-buffer buf-b (buffer-substring-no-properties 1 (point-max)))
                  (ce-edits ea)) results)
      (with-current-buffer buf-a
        (goto-char 8)
        (insert "XXX")
        (push "ins-a" (ce-edits ea))
        (setq my-ce-log (cons "ins" my-ce-log)))
      (with-current-buffer buf-b
        (goto-char 12)
        (insert "YYY")
        (push "ins-b" (ce-edits eb)))
      (push (list "edits"
                  (with-current-buffer buf-a (buffer-substring-no-properties 1 (point-max)))
                  (with-current-buffer buf-b (buffer-substring-no-properties 1 (point-max)))
                  (ce-edits ea) (ce-edits eb)) results)
      (setq results (reverse results))
      (list results
            (with-current-buffer buf-a my-ce-log)
            (ce-edits ea) (ce-edits eb)))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_bufswap_multi_object_registry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments mapcar 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass buf-registry ()
    ((bufs :initarg :bufs :accessor br-bufs :initform nil)
     (markers :initarg :markers :accessor br-markers :initform nil)
     (overlays :initarg :overlays :accessor br-overlays :initform nil)
     (log :initarg :log :accessor br-log :initform nil)))
  (let* ((reg (buf-registry :bufs nil :markers nil :overlays nil :log nil))
         (bufs nil)
         (mk-list nil)
         (ov-list nil))
    (dotimes (i 3)
      (let ((b (generate-new-buffer (format "bs4_%d" i))))
        (with-current-buffer b
          (insert (format "BUF%d-AAAA-BBBB-CCCC-DDDD" i))
          (put-text-property 1 5 'buf-id i)
          (put-text-property 6 10 'buf-id i)
          (put-text-property 11 15 'buf-id i)
          (put-text-property 16 20 'buf-id i))
        (push b bufs)
        (let ((mk (with-current-buffer b (set-marker (make-marker) 10)))
              (ov (with-current-buffer b (make-overlay 6 15))))
          (with-current-buffer b
            (overlay-put ov 'face 'bold)
            (overlay-put ov 'priority (1+ i)))
          (push mk mk-list)
          (push ov ov-list))))
    (setf (br-bufs reg) (reverse bufs))
    (setf (br-markers reg) (reverse mk-list))
    (setf (br-overlays reg) (reverse ov-list))
    (let* ((results nil)
           (snap-all
            (lambda ()
              (mapcar (lambda (b idx)
                       (with-current-buffer b
                         (list idx (buffer-string)
                               (marker-position (nth idx (br-markers reg)))
                               (overlay-start (nth idx (br-overlays reg)))
                               (overlay-end (nth idx (br-overlays reg))))))
                      (br-bufs reg)
                      (list 0 1 2)))))
      (push (list "init" (funcall snap-all)) results)
      (with-current-buffer (nth 0 (br-bufs reg))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (buffer-swap-text (nth 1 (br-bufs reg)))
        (push "swap0-1" (br-log reg)))
      (push (list "swap0-1" (funcall snap-all)) results)
      (with-current-buffer (nth 1 (br-bufs reg))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (buffer-swap-text (nth 2 (br-bufs reg)))
        (push "swap1-2" (br-log reg)))
      (push (list "swap1-2" (funcall snap-all)) results)
      (with-current-buffer (nth 0 (br-bufs reg))
        (goto-char 8)
        (insert "XXX"))
      (with-current-buffer (nth 1 (br-bufs reg))
        (goto-char 12)
        (insert "YYY"))
      (with-current-buffer (nth 2 (br-bufs reg))
        (goto-char 16)
        (insert "ZZZ"))
      (push (list "edits" (funcall snap-all)) results)
      (setq results (reverse results))
      (list results (br-log reg)))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_bufswap_method_dispatch_cross_buf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass buf-node ()
    ((name :initarg :name :accessor bn-name :initform "")
     (buf-ref :initarg :buf :accessor bn-buf :initform nil)
     (marker :initarg :marker :accessor bn-mk :initform nil)
     (edit-log :initarg :log :accessor bn-log :initform nil)))
  (defclass source-node (buf-node) ())
  (defclass sink-node (buf-node) ())
  (defmethod bn-edit ((node source-node) pos str)
    (with-current-buffer (bn-buf node)
      (goto-char pos)
      (insert str)
      (push (format "source@%d:%S" pos str) (bn-log node))))
  (defmethod bn-edit ((node sink-node) pos str)
    (with-current-buffer (bn-buf node)
      (goto-char pos)
      (insert str)
      (push (format "sink@%d:%S" pos str) (bn-log node))))
  (let* ((buf-a (generate-new-buffer "bs5a"))
         (buf-b (generate-new-buffer "bs5b"))
         (src (source-node :name "src" :buf buf-a :log nil))
         (snk (sink-node :name "snk" :buf buf-b :log nil)))
    (with-current-buffer buf-a
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight))
    (with-current-buffer buf-b
      (insert "1111-2222-3333-4444-5555-6666")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight))
    (let* ((results nil)
           (m-a (with-current-buffer buf-a (set-marker (make-marker) 10)))
           (m-b (with-current-buffer buf-b (set-marker (make-marker) 10))))
      (setf (bn-mk src) m-a)
      (setf (bn-mk snk) m-b)
      (push (list "init"
                  (with-current-buffer buf-a (buffer-string))
                  (with-current-buffer buf-b (buffer-string))
                  (marker-position m-a) (marker-position m-b)) results)
      (bn-edit src 8 "XXX")
      (bn-edit snk 12 "YYY")
      (push (list "edits"
                  (with-current-buffer buf-a (buffer-string))
                  (with-current-buffer buf-b (buffer-string))
                  (bn-log src) (bn-log snk)
                  (marker-position m-a) (marker-position m-b)) results)
      (with-current-buffer buf-a
        (setq buffer-undo-list nil)
        (undo-boundary)
        (buffer-swap-text buf-b))
      (push (list "swap"
                  (with-current-buffer buf-a (buffer-string))
                  (with-current-buffer buf-b (buffer-string))
                  (marker-position m-a) (marker-position m-b)
                  (marker-position (bn-mk src))
                  (marker-position (bn-mk snk))) results)
      (bn-edit src 8 "ZZZ")
      (bn-edit snk 12 "WWW")
      (push (list "post-swap-edits"
                  (with-current-buffer buf-a (buffer-string))
                  (with-current-buffer buf-b (buffer-string))
                  (bn-log src) (bn-log snk)
                  (marker-position m-a) (marker-position m-b)) results)
      (setq results (reverse results))
      (list results
            (cl-typep src 'source-node)
            (cl-typep snk 'sink-node)
            (cl-typep src 'buf-node)
            (cl-typep snk 'buf-node)))))"#,
        expect,
    );
}
