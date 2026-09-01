//! Combo: cl-eieio condition-case / error handling + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests deeply nested condition-case with EIEIO objects mediating error recovery state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_condition_case_nested_error_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass error-state ()
    ((phase :initarg :phase :accessor es-phase :initform "")
     (error-data :initarg :error-data :accessor es-error :initform nil)
     (recovered :initarg :recovered :accessor es-recovered :initform nil)))
  (let* ((buf (generate-new-buffer "ec1"))
         (s1 (error-state :phase "outer"))
         (s2 (error-state :phase "middle"))
         (s3 (error-state :phase "inner")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 1)
      (put-text-property 6 10 'zone 2)
      (put-text-property 11 15 'zone 3)
      (put-text-property 16 20 'zone 4)
      (put-text-property 21 25 'zone 5)
      (setq-local states (list s1 s2 s3))
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (condition-case err1
            (progn
              (push (list 'outer-enter (es-phase s1)) results)
              (narrow-to-region 6 15)
              (condition-case err2
                  (progn
                    (push (list 'middle-enter (es-phase s2)) results)
                    (goto-char 8)
                    (insert "XXX")
                    (signal 'wrong-type-argument '(stringp 42))
                    (push 'unreachable results))
                (wrong-type-argument
                 (setf (es-error s2) (cdr err2))
                 (setf (es-recovered s2) t)
                 (push (list 'middle-caught (es-error s2) (marker-position m)) results)
                 (widen)
                 (condition-case err3
                     (progn
                       (push (list 'inner-enter (es-phase s3)) results)
                       (goto-char 1)
                       (insert "YYY")
                       (signal 'args-out-of-range '(0 1))
                       (push 'unreachable results))
                   (args-out-of-range
                    (setf (es-error s3) (cdr err3))
                    (setf (es-recovered s3) t)
                    (push (list 'inner-caught (es-error s3) (marker-position m)) results)))))
              (push 'after-middle results))
          (wrong-type-argument
           (setf (es-error s1) (cdr err1))
           (setf (es-recovered s1) t)
           (push (list 'outer-caught (es-error s1)) results)))
        (push (list 'final-point (point) (marker-position m)
                   (overlay-start ov) (overlay-end ov)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s s1=%s s2=%s s3=%s"
                       results
                       (list (es-recovered s1) (es-error s1))
                       (list (es-recovered s2) (es-error s2))
                       (list (es-recovered s3) (es-error s3))))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'error-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                states))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_condition_case_buffer_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass bounds-checker ()
    ((name :initarg :name :accessor bc-name :initform "")
     (attempts :initarg :attempts :accessor bc-attempts :initform 0)
     (errors :initarg :errors :accessor bc-errors :initform 0)))
  (let* ((buf (generate-new-buffer "ec2"))
         (b1 (bounds-checker :name "point-min" :attempts 0 :errors 0))
         (b2 (bounds-checker :name "point-max" :attempts 0 :errors 0))
         (b3 (bounds-checker :name "narrow" :attempts 0 :errors 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'group 1)
      (put-text-property 6 10 'group 2)
      (put-text-property 11 15 'group 3)
      (put-text-property 16 20 'group 4)
      (setq-local checkers (list b1 b2 b3))
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (test-results nil))
        (undo-boundary)
        (setf (bc-attempts b1) (1+ (bc-attempts b1)))
        (condition-case err
            (progn
              (goto-char 0)
              (push 'bad-at-zero test-results))
          (args-out-of-range
           (setf (bc-errors b1) (1+ (bc-errors b1)))
           (push (list 'caught-zero (cdr err)) test-results)))
        (setf (bc-attempts b2) (1+ (bc-attempts b2)))
        (condition-case err
            (progn
              (goto-char (1+ (point-max)))
              (push 'bad-past-max test-results))
          (args-out-of-range
           (setf (bc-errors b2) (1+ (bc-errors b2)))
           (push (list 'caught-max (cdr err)) test-results)))
        (save-restriction
          (narrow-to-region 6 10)
          (setf (bc-attempts b3) (1+ (bc-attempts b3)))
          (condition-case err
              (progn
                (goto-char 1)
                (push 'bad-in-narrow test-results))
            (args-out-of-range
             (setf (bc-errors b3) (1+ (bc-errors b3)))
             (push (list 'caught-narrow (cdr err)) test-results))))
        (setq test-results (reverse test-results))
        (goto-char (point-max))
        (insert (format " | results=%s b1=[%d,%d] b2=[%d,%d] b3=[%d,%d] m=%d"
                       test-results
                       (bc-attempts b1) (bc-errors b1)
                       (bc-attempts b2) (bc-errors b2)
                       (bc-attempts b3) (bc-errors b3)
                       (marker-position m)))
        (set-marker m 8)
        (put-text-property (1- (point-max)) (point-max) 'bounds-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                checkers))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_condition_case_user_signal_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Invalid error symbol\" my-error-b)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass signal-handler ()
    ((signal-type :initarg :signal-type :accessor sh-type :initform nil)
     (data :initarg :data :accessor sh-data :initform nil)
     (handled :initarg :handled :accessor sh-handled :initform 0)))
  (let* ((buf (generate-new-buffer "ec3"))
         (h1 (signal-handler :signal-type 'my-error-a))
         (h2 (signal-handler :signal-type 'my-error-b))
         (h3 (signal-handler :signal-type 'my-error-c)))
    (with-current-buffer buf
      (insert "AA-BB-CC-DD-EE-FF")
      (put-text-property 1 3 'sig 'a)
      (put-text-property 4 6 'sig 'b)
      (put-text-property 7 9 'sig 'c)
      (put-text-property 10 12 'sig 'd)
      (put-text-property 13 15 'sig 'e)
      (put-text-property 16 18 'sig 'f)
      (setq-local handlers (list h1 h2 h3))
      (let* ((ov (make-overlay 4 12))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 4))
             (chain nil))
        (undo-boundary)
        (condition-case err
            (progn
              (push 'enter-a chain)
              (goto-char 4)
              (insert "XA")
              (signal 'my-error-b (list "from-a" (marker-position m))))
          (my-error-a
           (setf (sh-data h1) (cdr err))
           (setf (sh-handled h1) (1+ (sh-handled h1)))
           (push (list 'handled-a (sh-data h1)) chain))
          (my-error-b
           (setf (sh-data h2) (cdr err))
           (setf (sh-handled h2) (1+ (sh-handled h2)))
           (push (list 'caught-b-at-a (sh-data h2)) chain)
           (condition-case err2
               (progn
                 (push 'enter-c chain)
                 (goto-char 1)
                 (insert "YC")
                 (signal 'my-error-c (list "from-b" (marker-position m))))
             (my-error-c
              (setf (sh-data h3) (cdr err2))
              (setf (sh-handled h3) (1+ (sh-handled h3)))
              (push (list 'caught-c (sh-data h3)) chain)))))
        (setq chain (reverse chain))
        (goto-char (point-max))
        (insert (format " | chain=%s h1=%s h2=%s h3=%s m=%d"
                       chain
                       (list (sh-handled h1) (sh-data h1))
                       (list (sh-handled h2) (sh-data h2))
                       (list (sh-handled h3) (sh-data h3))
                       (marker-position m)))
        (set-marker m 6)
        (put-text-property (1- (point-max)) (point-max) 'signal-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                handlers))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_condition_case_overlay_marker_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable m)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass recovery-point ()
    ((label :initarg :label :accessor rp-label :initform "")
     (marker-pos :initarg :marker-pos :accessor rp-mpos :initform 1)
     (overlay-start :initarg :overlay-start :accessor rp-ovstart :initform 1)
     (overlay-end :initarg :overlay-end :accessor rp-ovend :initform 1)))
  (let* ((buf (generate-new-buffer "ec4"))
         (rp1 (recovery-point :label "before"))
         (rp2 (recovery-point :label "after"))
         (rp3 (recovery-point :label "recovered")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'section 1)
      (put-text-property 6 10 'section 2)
      (put-text-property 11 15 'section 3)
      (put-text-property 16 20 'section 4)
      (put-text-property 21 25 'section 5)
      (setq-local rps (list rp1 rp2 rp3))
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 11))
             (snapshot-before nil)
             (snapshot-after nil)
             (snapshot-recovered nil))
        (undo-boundary)
        (setf (rp-mpos rp1) (marker-position m)
              (rp-ovstart rp1) (overlay-start ov)
              (rp-ovend rp1) (overlay-end ov))
        (setq snapshot-before (list (rp-mpos rp1) (rp-ovstart rp1) (rp-ovend rp1)))
        (condition-case err
            (progn
              (goto-char 6)
              (insert "ERROR_TRIGGER")
              (signal 'error '("deliberate error"))
              (push 'unreachable snapshot-after))
          (error
           (setf (rp-mpos rp2) (marker-position m)
                 (rp-ovstart rp2) (overlay-start ov)
                 (rp-ovend rp2) (overlay-end ov))
           (setq snapshot-after (list (rp-mpos rp2) (rp-ovstart rp2) (rp-ovend rp2)))
           (primitive-undo 1 buffer-undo-list)
           (setf (rp-mpos rp3) (marker-position m)
                 (rp-ovstart rp3) (overlay-start ov)
                 (rp-ovend rp3) (overlay-end ov))
           (setq snapshot-recovered (list (rp-mpos rp3) (rp-ovstart rp3) (rp-ovend rp3)))))
        (goto-char (point-max))
        (insert (format " | before=%s after=%s recovered=%s"
                       snapshot-before snapshot-after snapshot-recovered))
        (set-marker m 8)
        (put-text-property (1- (point-max)) (point-max) 'recovery-log t))
      (list (marker-position m) (overlay-start ov) (overlay-end ov) (buffer-string)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_condition_case_text_prop_corruption_guard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable m)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass prop-guard ()
    ((name :initarg :name :accessor pg-name :initform "")
     (prop :initarg :prop :accessor pg-prop :initform nil)
     (expected :initarg :expected :accessor pg-expected :initform nil)
     (actual :initarg :actual :accessor pg-actual :initform nil)))
  (let* ((buf (generate-new-buffer "ec5"))
         (g1 (prop-guard :name "color" :prop 'color :expected 'red))
         (g2 (prop-guard :name "weight" :prop 'weight :expected 5))
         (g3 (prop-guard :name "label" :prop 'label :expected 'start)))
    (with-current-buffer buf
      (insert "XXXXX-YYYYY-ZZZZZ")
      (put-text-property 1 6 'color 'red)
      (put-text-property 1 6 'weight 5)
      (put-text-property 1 6 'label 'start)
      (put-text-property 7 12 'color 'blue)
      (put-text-property 7 12 'weight 10)
      (put-text-property 7 12 'label 'middle)
      (put-text-property 13 17 'color 'green)
      (put-text-property 13 17 'weight 15)
      (put-text-property 13 17 'label 'end)
      (setq-local guards (list g1 g2 g3))
      (let* ((ov (make-overlay 1 12))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (check-log nil))
        (undo-boundary)
        (condition-case err
            (progn
              (goto-char 3)
              (insert "BREAK")
              (remove-text-properties 1 (point-max) '(color nil weight nil))
              (signal 'error '("deliberate corruption"))
              (push 'unreachable check-log))
          (error
           (push (list 'error-caught (cdr err)) check-log)
           (primitive-undo 1 buffer-undo-list)
           (dolist (g (list g1 g2 g3))
             (let ((val (get-text-property 1 (pg-prop g))))
               (setf (pg-actual g) val)
               (push (list (pg-name g) val (if (eq val (pg-expected g)) 'ok 'corrupted)) check-log)))))
        (setq check-log (reverse check-log))
        (goto-char (point-max))
        (insert (format " | log=%s g1=%s g2=%s g3=%s m=%d"
                       check-log
                       (pg-actual g1) (pg-actual g2) (pg-actual g3)
                       (marker-position m)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'guard-log t))
      (list (marker-position m) (overlay-start ov) (overlay-end ov) (buffer-string)))
    (kill-buffer buf)))"#,
        expect,
    );
}
