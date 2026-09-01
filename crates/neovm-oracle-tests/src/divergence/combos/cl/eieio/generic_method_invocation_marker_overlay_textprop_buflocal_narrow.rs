//! Combo: cl-eieio generic-method invocation + marker + overlay + textprop + buflocal + narrow + undo.
//! Tests defgeneric/defmethod dispatch with :before/:after/:around qualifiers and buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_generic_before_after_around_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass log-entry ()
    ((timestamp :initarg :timestamp :accessor entry-timestamp :initform 0)
     (message :initarg :message :accessor entry-message :initform "")))
  (defclass error-entry (log-entry)
    ((err-code :initarg :err-code :accessor error-code :initform 0)))
  (defclass warning-entry (log-entry)
    ((source :initarg :source :accessor warning-source :initform "")))
  (defvar entry-log nil)
  (defgeneric process-entry (entry)
    (:documentation "Process a log entry."))
  (defmethod process-entry :before ((entry log-entry))
    (push (format "before:%s" (type-of entry)) entry-log))
  (defmethod process-entry ((entry log-entry))
    (push (format "primary:%s:msg=%s" (type-of entry) (entry-message entry)) entry-log)
    (format "processed[%s]" (entry-message entry)))
  (defmethod process-entry :after ((entry log-entry))
    (push (format "after:%s" (type-of entry)) entry-log))
  (defmethod process-entry :around ((entry log-entry))
    (push "around:start" entry-log)
    (let ((result (cl-call-next-method)))
      (push "around:end" entry-log)
      result))
  (defmethod process-entry ((entry error-entry))
    (push (format "error-primary:code=%d" (error-code entry)) entry-log)
    (format "error-processed[%s:code=%d]" (entry-message entry) (error-code entry)))
  (defmethod process-entry ((entry warning-entry))
    (push (format "warn-primary:src=%s" (warning-source entry)) entry-log)
    (format "warn-processed[%s:src=%s]" (entry-message entry) (warning-source entry)))
  (let* ((buf (generate-new-buffer "gm1"))
         (e1 (error-entry :timestamp 100 :message "crash" :err-code 500))
         (e2 (warning-entry :timestamp 200 :message "deprecated" :source "parser")))
    (with-current-buffer buf
      (insert "ERR:crash:500-WARN:deprecated:parser")
      (put-text-property 1 4 'level 'error)
      (put-text-property 5 10 'level 'critical)
      (put-text-property 11 14 'level 'code)
      (put-text-property 15 19 'level 'warning)
      (put-text-property 20 31 'level 'notice)
      (setq-local entry-log nil)
      (setq-local entry1 e1)
      (setq-local entry2 e2)
      (let* ((ov (make-overlay 1 14))
             (_ (overlay-put ov 'priority 10))
             (m (make-marker))
             (_ (set-marker m 5)))
        (undo-boundary)
        (let ((r1 (process-entry e1))
              (log1 (reverse entry-log)))
          (setq entry-log nil)
          (let ((r2 (process-entry e2))
                (log2 (reverse entry-log)))
            (goto-char 1)
            (insert (format "%s|%s" r1 r2))
            (setf (marker-position m) (+ (length r1) 2))
            (put-text-property 1 (+ 1 (length r1)) 'result 'error)
            (put-text-property (+ 2 (length r1)) (+ 2 (length r1) (length r2)) 'result 'warning)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (tp1 (get-text-property 1 'level))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe tp1 bs log1 log2
                (marker-position m)
                (buffer-string)
                entry1 entry2)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_method_combination_buffer_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass scored-item ()
    ((name :initarg :name :accessor item-name :initform "")
     (base-score :initarg :base-score :accessor item-base-score :initform 0)))
  (defclass bonus-item (scored-item)
    ((bonus :initarg :bonus :accessor item-bonus :initform 0)))
  (defgeneric compute-score (item)
    (:method-combination +))
  (defmethod compute-score + ((item scored-item))
    (item-base-score item))
  (defmethod compute-score + ((item bonus-item))
    (item-bonus item))
  (let* ((buf (generate-new-buffer "mc1"))
         (bi (bonus-item :name "sword" :base-score 50 :bonus 25)))
    (with-current-buffer buf
      (insert "ITEM:sword-SCORE:50-BONUS:25")
      (put-text-property 1 5 'field 'item)
      (put-text-property 6 11 'field 'name)
      (put-text-property 12 17 'field 'score)
      (put-text-property 18 23 'field 'bonus)
      (put-text-property 24 26 'field 'bonus-val)
      (setq-local scored bi)
      (let* ((ov (make-overlay 12 26))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 15)))
        (undo-boundary)
        (let ((total (compute-score bi))
              (base (item-base-score bi))
              (bonus (item-bonus bi)))
          (setf (item-base-score bi) (+ base 10))
          (setf (item-bonus bi) (+ bonus 5))
          (let ((new-total (compute-score bi)))
            (goto-char 18)
            (insert (format "TOTAL:%d+" new-total))
            (setf (marker-position m) 25)
            (put-text-property 18 (+ 18 (length (format "TOTAL:%d+" new-total)))
                              'computed t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (final-score (compute-score scored)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs final-score
                (marker-position m)
                (buffer-string)
                scored)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_next_method_chain_buffer_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"button-label is already defined as something else than a generic function\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass widget ()
    ((id :initarg :id :accessor widget-id :initform "")
     (visible :initarg :visible :accessor widget-visible :initform t)))
  (defclass button (widget)
    ((label :initarg :label :accessor button-label :initform "click")
     (enabled :initarg :enabled :accessor button-enabled :initform t)))
  (defclass checkbox (button)
    ((checked :initarg :checked :accessor checkbox-checked :initform nil)))
  (defgeneric render-widget (w)
    (:method-combination progn))
  (defmethod render-widget progn ((w widget))
    (format "widget[%s]" (widget-id w)))
  (defmethod render-widget progn ((b button))
    (format "button[%s:enabled=%s]" (button-label b) (button-enabled b)))
  (defmethod render-widget progn ((c checkbox))
    (format "checkbox[checked=%s]" (checkbox-checked c)))
  (let* ((buf (generate-new-buffer "nm1"))
         (cb (checkbox :id "cb1" :label "agree" :enabled t :checked nil)))
    (with-current-buffer buf
      (insert "WIDGET:cb1-BUTTON:agree-CHECKBOX:nil")
      (put-text-property 1 7 'level 'widget)
      (put-text-property 8 11 'level 'id)
      (put-text-property 12 19 'level 'button)
      (put-text-property 20 24 'level 'label)
      (put-text-property 25 34 'level 'checkbox)
      (put-text-property 35 38 'level 'state)
      (setq-local my-widget cb)
      (let* ((ov (make-overlay 1 24))
             (_ (overlay-put ov 'priority 3))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (let ((render-result (render-widget cb))
              (cb-checked (checkbox-checked cb))
              (cb-enabled (button-enabled cb)))
          (setf (checkbox-checked cb) t)
          (setf (button-enabled cb) nil)
          (let ((new-render (render-widget cb)))
            (goto-char 25)
            (insert (format "[%s->%s]" render-result new-render))
            (setf (marker-position m) 30)
            (put-text-property 25 (+ 25 (length (format "[%s->%s]" render-result new-render)))
                              'render-change t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (final-checked (checkbox-checked my-widget))
              (final-enabled (button-enabled my-widget)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs final-checked final-enabled
                (marker-position m)
                (buffer-string)
                my-widget)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_static_method_dispatch_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass expr ()
    ((op :initarg :op :accessor expr-op :initform nil)))
  (defclass const-expr (expr)
    ((val :initarg :val :accessor const-val :initform 0)))
  (defclass binop-expr (expr)
    ((left :initarg :left :accessor binop-left :initform nil)
     (right :initarg :right :accessor binop-right :initform nil)))
  (defgeneric eval-expr (e))
  (defmethod eval-expr ((e const-expr))
    (const-val e))
  (defmethod eval-expr ((e binop-expr))
    (let ((l (eval-expr (binop-left e)))
          (r (eval-expr (binop-right e)))
          (op (expr-op e)))
      (cond ((eq op '+) (+ l r))
            ((eq op '*) (* l r))
            ((eq op '-) (- l r))
            (t 0))))
  (let* ((buf (generate-new-buffer "sm1"))
         (expr1 (binop-expr
                 :op '+
                 :left (binop-expr :op '*
                                   :left (const-expr :val 3)
                                   :right (const-expr :val 4))
                 :right (const-expr :val 5))))
    (with-current-buffer buf
      (insert "EXPR:(3*4)+5=RESULT")
      (put-text-property 1 5 'region 'expr-start)
      (put-text-property 6 8 'region 'mul)
      (put-text-property 9 12 'region 'add)
      (put-text-property 13 14 'region 'num)
      (put-text-property 15 21 'region 'result)
      (setq-local my-expr expr1)
      (let* ((ov (make-overlay 6 12))
             (_ (overlay-put ov 'face 'underline))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 1 14)
        (undo-boundary)
        (let ((result (eval-expr expr1)))
          (goto-char (point-min))
          (insert (format "=%d=" result))
          (setf (marker-position m) 5)
          (put-text-property 1 (+ 1 (length (format "=%d=" result))) 'computed t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-substring (point-min) (point-max))))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-expr)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_no_applicable_method_error_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass container ()
    ((items :initarg :items :accessor container-items :initform nil)))
  (defclass list-container (container) ())
  (defgeneric serialize (obj))
  (defmethod serialize ((c list-container))
    (format "list:%s" (container-items c)))
  (let* ((buf (generate-new-buffer "na1"))
         (lc (list-container :items '(1 2 3)))
         (bc (container :items '(a b c))))
    (with-current-buffer buf
      (insert "LIST:123-BASE:abc-ERR:pending")
      (put-text-property 1 5 'type 'list)
      (put-text-property 6 9 'type 'values)
      (put-text-property 10 14 'type 'base)
      (put-text-property 15 18 'type 'base-values)
      (put-text-property 19 22 'type 'error)
      (put-text-property 23 30 'type 'error-status)
      (setq-local cont1 lc)
      (setq-local cont2 bc)
      (let* ((ov (make-overlay 1 18))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 7))
             (results nil)
             (errors nil))
        (undo-boundary)
        (condition-case err
            (push (serialize lc) results)
          (error (push (format "err1:%s" err) errors)))
        (condition-case err
            (push (serialize bc) results)
          (no-method (push (format "no-method:%s" (car (cdr err))) errors))
          (error (push (format "err2:%s" err) errors)))
        (goto-char 23)
        (insert (format "[%s|%s]"
                       (mapconcat #'identity (reverse results) ",")
                       (mapconcat #'identity (reverse errors) ",")))
        (setf (marker-position m) 25)
        (put-text-property 23 (+ 23 (length (format "[%s|%s]"
                                                      (mapconcat #'identity (reverse results) ",")
                                                      (mapconcat #'identity (reverse errors) ","))))
                          'error-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (cont1-type (type-of cont1))
              (cont2-type (type-of cont2)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs cont1-type cont2-type
                (marker-position m)
                (buffer-string)
                results errors cont1 cont2)))
      (kill-buffer buf))))"#,
        expect,
    );
}
