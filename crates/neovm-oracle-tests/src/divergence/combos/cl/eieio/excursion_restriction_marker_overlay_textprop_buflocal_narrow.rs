//! Combo: cl-eieio save-excursion/save-restriction nesting + marker + overlay + textprop + buflocal + undo.
//! Tests nested excursion/restriction with EIEIO objects mediating buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_nested_excursion_object_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cursor-snapshot ()
    ((name :initarg :name :accessor cs-name :initform "")
     (point :initarg :point :accessor cs-point :initform 1)
     (narrow-start :initarg :narrow-start :accessor cs-nstart :initform nil)
     (narrow-end :initarg :narrow-end :accessor cs-nend :initform nil)))
  (let* ((buf (generate-new-buffer "ex1"))
         (snapshots nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'section 'a)
      (put-text-property 6 10 'section 'b)
      (put-text-property 11 15 'section 'c)
      (put-text-property 16 20 'section 'd)
      (put-text-property 21 25 'section 'e)
      (setq-local snaps snapshots)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 8))
             (outer-point nil)
             (inner-point nil)
             (outer-narrow nil)
             (inner-narrow nil))
        (undo-boundary)
        (save-excursion
          (save-restriction
            (narrow-to-region 6 15)
            (push (cursor-snapshot :name "outer-narrow"
                                   :point (point)
                                   :narrow-start (point-min)
                                   :narrow-end (point-max))
                  snapshots)
            (goto-char 8)
            (insert "XXX")
            (setq outer-point (marker-position m))
            (setq outer-narrow (list (point-min) (point-max)))
            (save-excursion
              (save-restriction
                (widen)
                (narrow-to-region 1 10)
                (push (cursor-snapshot :name "inner-narrow"
                                       :point (point)
                                       :narrow-start (point-min)
                                       :narrow-end (point-max))
                      snapshots)
                (goto-char 3)
                (insert "YYY")
                (setq inner-point (marker-position m))
                (setq inner-narrow (list (point-min) (point-max))))))
          (push (cursor-snapshot :name "after-restore"
                                 :point (point)
                                 :narrow-start (and (/= (point-min) 1) (point-min))
                                 :narrow-end (and (/= (point-max) 25) (point-max)))
                snapshots))
        (let ((final-point (point))
              (final-narrow (list (point-min) (point-max)))
              (snap-data (mapcar (lambda (s) (list (cs-name s) (cs-point s)
                                                   (cs-nstart s) (cs-nend s)))
                                 (reverse snapshots))))
          (goto-char (point-max))
          (insert (format " | outer=%s inner=%s final=%s snaps=%s"
                         (list outer-point outer-narrow)
                         (list inner-point inner-narrow)
                         (list final-point final-narrow)
                         snap-data))
          (setf (marker-position m) 10)
          (put-text-property (1- (point-max)) (point-max) 'excursion-log t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                snaps)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_with_current_buffer_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 15 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass buf-state ()
    ((buf :initarg :buf :accessor bs-buf :initform nil)
     (label :initarg :label :accessor bs-label :initform "")
     (last-pos :initarg :last-pos :accessor bs-last-pos :initform 1)))
  (let* ((buf1 (generate-new-buffer "ex2a"))
         (buf2 (generate-new-buffer "ex2b"))
         (s1 (buf-state :buf buf1 :label "first"))
         (s2 (buf-state :buf buf2 :label "second")))
    (with-current-buffer buf1
      (insert "FIRST-BUFFER-HERE")
      (put-text-property 1 6 'buf 1)
      (put-text-property 7 14 'buf 'content)
      (put-text-property 15 19 'buf 'tail))
    (with-current-buffer buf2
      (insert "SECOND-BUFFER-HERE")
      (put-text-property 1 7 'buf 2)
      (put-text-property 8 15 'buf 'content)
      (put-text-property 16 20 'buf 'tail))
    (with-current-buffer buf1
      (setq-local my-state s1)
      (let* ((ov1 (make-overlay 7 14))
             (_ (overlay-put ov1 'priority 1))
             (m1 (make-marker))
             (_ (set-marker m1 10))
             (positions nil))
        (undo-boundary)
        (setf (bs-last-pos s1) (point))
        (push (list 'buf1-start (point) (marker-position m1)) positions)
        (with-current-buffer buf2
          (setf (bs-last-pos s2) (point))
          (save-excursion
            (goto-char 8)
            (insert "INSERTED")
            (push (list 'buf2-insert (point) (buffer-string)) positions))
          (push (list 'buf2-after (point) (buffer-string)) positions))
        (push (list 'buf1-back (point) (marker-position m1) (buffer-string)) positions)
        (save-excursion
          (with-current-buffer buf2
            (goto-char (point-max))
            (insert "END"))
          (push (list 'buf1-excursion (point) (marker-position m1) (buffer-string)) positions))
        (push (list 'buf1-final (point) (marker-position m1) (buffer-string)) positions)
        (goto-char 7)
        (insert (format "%s" (reverse positions)))
        (setf (marker-position m1) 12)
        (put-text-property 7 (+ 7 (length (format "%s" (reverse positions)))) 'pos-log t)
        (undo-boundary)
        (let ((mp (marker-position m1))
              (os (overlay-start ov1))
              (oe (overlay-end ov1))
              (bs (buffer-string))
              (b2-content (with-current-buffer buf2 (buffer-string))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs b2-content
                (marker-position m1)
                (buffer-string)
                my-state)))
      (kill-buffer buf1)
      (kill-buffer buf2))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_save_restriction_nested_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable m)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-state ()
    ((label :initarg :label :accessor ns-label :initform "")
     (min :initarg :min :accessor ns-min :initform 1)
     (max :initarg :max :accessor ns-max :initform 1)))
  (let* ((buf (generate-new-buffer "ex3"))
         (states nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'block 1)
      (put-text-property 6 10 'block 2)
      (put-text-property 11 15 'block 3)
      (put-text-property 16 20 'block 4)
      (put-text-property 21 25 'block 5)
      (put-text-property 26 30 'block 6)
      (setq-local my-states states)
      (let* ((ov (make-overlay 11 20))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 13)))
        (undo-boundary)
        (push (narrow-state :label "full"
                            :min (point-min) :max (point-max)) states)
        (save-restriction
          (narrow-to-region 11 20)
          (push (narrow-state :label "narrow-1"
                              :min (point-min) :max (point-max)) states)
          (goto-char (point-min))
          (insert "NNN")
          (save-restriction
            (widen)
            (narrow-to-region 1 15)
            (push (narrow-state :label "narrow-2"
                                :min (point-min) :max (point-max)) states)
            (goto-char 6)
            (insert "MMM")))
          (push (narrow-state :label "after-inner"
                              :min (point-min) :max (point-max)) states))
        (push (narrow-state :label "restored"
                            :min (point-min) :max (point-max)) states)
        (let ((state-data (mapcar (lambda (s) (list (ns-label s) (ns-min s) (ns-max s)))
                                  (reverse states)))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (goto-char (point-max))
          (insert (format " | states=%s m=%d ov=[%d,%d]" state-data mp os oe))
          (put-text-property (1- (point-max)) (point-max) 'narrow-log t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-states))))
    (kill-buffer buf))"#,
        expect,
    );
}

#[test]
fn combo_eieio_excursion_marker_overlay_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass edit-cursor ()
    ((id :initarg :id :accessor ec-id :initform 0)
     (saved-point :initarg :saved-point :accessor ec-saved-point :initform 1)
     (marker :initarg :marker :accessor ec-marker :initform nil)))
  (let* ((buf (generate-new-buffer "ex4"))
         (c1 (edit-cursor :id 1 :saved-point 1))
         (c2 (edit-cursor :id 2 :saved-point 10))
         (m1 (make-marker))
         (m2 (make-marker))
         (_ (set-marker m1 5))
         (_ (set-marker m2 15)))
    (setf (ec-marker c1) m1 (ec-marker c2) m2)
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 1)
      (put-text-property 6 10 'zone 2)
      (put-text-property 11 15 'zone 3)
      (put-text-property 16 20 'zone 4)
      (put-text-property 21 25 'zone 5)
      (setq-local cursors (list c1 c2))
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2)))
        (undo-boundary)
        (setf (ec-saved-point c1) (point))
        (save-excursion
          (goto-char 10)
          (insert "XXXX")
          (setf (ec-saved-point c2) (point))
          (save-excursion
            (goto-char 1)
            (insert "YYYY"))
          (let ((c1-pos (marker-position (ec-marker c1)))
                (c2-pos (marker-position (ec-marker c2)))
                (ov1s (overlay-start ov1))
                (ov1e (overlay-end ov1))
                (ov2s (overlay-start ov2))
                (ov2e (overlay-end ov2)))
            (goto-char (point-max))
            (insert (format " | c1=%d c2=%d ov1=[%d,%d] ov2=[%d,%d]"
                           c1-pos c2-pos ov1s ov1e ov2s ov2e))
            (put-text-property (1- (point-max)) (point-max) 'cursor-log t)))
        (let ((restored (point))
              (c1-mp (marker-position (ec-marker (car cursors))))
              (c2-mp (marker-position (ec-marker (cadr cursors))))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list restored c1-mp c2-mp bs
                (buffer-string)
                (marker-position m1)
                (marker-position m2)
                cursors)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_excursion_with_text_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass styled-region ()
    ((name :initarg :name :accessor sr-name :initform "")
     (prop :initarg :prop :accessor sr-prop :initform nil)
     (val :initarg :val :accessor sr-val :initform nil)))
  (let* ((buf (generate-new-buffer "ex5"))
         (r1 (styled-region :name "bold" :prop 'face :val 'bold))
         (r2 (styled-region :name "italic" :prop 'face :val 'italic))
         (r3 (styled-region :name "underline" :prop 'underline :val t)))
    (with-current-buffer buf
      (insert "AA-BB-CC-DD-EE-FF")
      (put-text-property 1 3 'group 1)
      (put-text-property 4 6 'group 2)
      (put-text-property 7 9 'group 3)
      (put-text-property 10 12 'group 4)
      (put-text-property 13 15 'group 5)
      (put-text-property 16 18 'group 6)
      (setq-local regions (list r1 r2 r3))
      (let* ((ov (make-overlay 1 9))
             (_ (overlay-put ov 'priority 3))
             (m (make-marker))
             (_ (set-marker m 4))
             (applied-props nil))
        (undo-boundary)
        (save-excursion
          (save-restriction
            (narrow-to-region 4 12)
            (put-text-property (point-min) (+ (point-min) 3)
                              (sr-prop r1) (sr-val r1))
            (push (list (sr-name r1)
                       (get-text-property (point-min) (sr-prop r1)))
                  applied-props)
            (save-excursion
              (save-restriction
                (widen)
                (narrow-to-region 13 18)
                (put-text-property (point-min) (+ (point-min) 3)
                                  (sr-prop r2) (sr-val r2))
                (push (list (sr-name r2)
                           (get-text-property (point-min) (sr-prop r2)))
                      applied-props)))))
        (put-text-property 1 3 (sr-prop r3) (sr-val r3))
        (push (list (sr-name r3) (get-text-property 1 (sr-prop r3))) applied-props)
        (let ((prop-snapshot (mapcar (lambda (pos)
                                       (get-text-property pos 'face))
                                    '(1 4 7 10 13 16))))
          (goto-char (point-max))
          (insert (format " | props=%s applied=%s" prop-snapshot (reverse applied-props)))
          (setf (marker-position m) 5)
          (put-text-property (1- (point-max)) (point-max) 'prop-log t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (p1 (get-text-property 1 'face))
              (p4 (get-text-property 4 'face))
              (p13 (get-text-property 13 'face)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs p1 p4 p13
                (marker-position m)
                (buffer-string)
                regions)))
      (kill-buffer buf))))"#,
        expect,
    );
}
