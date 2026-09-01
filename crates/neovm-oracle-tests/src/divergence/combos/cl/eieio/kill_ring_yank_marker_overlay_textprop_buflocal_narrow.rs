//! Combo: cl-eieio kill-ring / yank + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests kill-region/yank with EIEIO objects tracking kill-ring operations and text property propagation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_kill_yank_with_text_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass kill-record ()
    ((text :initarg :text :accessor kr-text :initform "")
     (props :initarg :props :accessor kr-props :initform nil)
     (yank-count :initarg :yank-count :accessor kr-yank :initform 0)))
  (let* ((buf (generate-new-buffer "ky1"))
         (kr1 (kill-record :text "" :props nil :yank-count 0))
         (kr2 (kill-record :text "" :props nil :yank-count 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'flavor 'cherry)
      (put-text-property 6 10 'flavor 'lemon)
      (put-text-property 11 15 'flavor 'lime)
      (put-text-property 16 20 'flavor 'orange)
      (put-text-property 21 25 'flavor 'grape)
      (setq-local records (list kr1 kr2))
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (setf (kr-text kr1) (buffer-substring 1 6))
        (setf (kr-props kr1) (get-text-property 1 'flavor))
        (kill-region 1 6)
        (push (list 'after-kill1 (buffer-string) (marker-position m)) results)
        (goto-char (point-max))
        (yank)
        (setf (kr-yank kr1) (1+ (kr-yank kr1)))
        (push (list 'after-yank1 (buffer-string) (marker-position m)
                   (get-text-property (1- (point-max)) 'flavor)) results)
        (setf (kr-text kr2) (buffer-substring 1 6))
        (setf (kr-props kr2) (get-text-property 1 'flavor))
        (kill-region 1 6)
        (push (list 'after-kill2 (buffer-string) (marker-position m)) results)
        (save-excursion
          (goto-char 5)
          (yank)
          (setf (kr-yank kr2) (1+ (kr-yank kr2)))
          (push (list 'after-yank2-at-5 (buffer-string) (marker-position m)
                     (get-text-property 5 'flavor)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s kr1=[%s,%s,%d] kr2=[%s,%s,%d]"
                       results
                       (kr-text kr1) (kr-props kr1) (kr-yank kr1)
                       (kr-text kr2) (kr-props kr2) (kr-yank kr2)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'kill-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                records))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_kill_yank_narrow_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass region-op ()
    ((op :initarg :op :accessor ro-op :initform "")
     (start :initarg :start :accessor ro-start :initform 1)
     (end :initarg :end :accessor ro-end :initform 1)
     (content :initarg :content :accessor ro-content :initform "")))
  (let* ((buf (generate-new-buffer "ky2"))
         (op1 (region-op :op "kill-narrow"))
         (op2 (region-op :op "yank-narrow"))
         (op3 (region-op :op "kill-wide")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'tier 1)
      (put-text-property 6 10 'tier 2)
      (put-text-property 11 15 'tier 3)
      (put-text-property 16 20 'tier 4)
      (put-text-property 21 25 'tier 5)
      (setq-local ops (list op1 op2 op3))
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 6))
             (log nil))
        (undo-boundary)
        (save-restriction
          (narrow-to-region 6 15)
          (setf (ro-start op1) (point-min)
                (ro-end op1) (+ (point-min) 5)
                (ro-content op1) (buffer-substring (point-min) (+ (point-min) 5)))
          (kill-region (point-min) (+ (point-min) 5))
          (push (list 'killed-in-narrow (buffer-string) (marker-position m)) log))
        (push (list 'after-widen (buffer-string) (marker-position m)) log)
        (goto-char 5)
        (yank)
        (setf (ro-start op2) 5
              (ro-end op2) (point)
              (ro-content op2) (buffer-substring 5 (point)))
        (push (list 'yanked-at-5 (buffer-string) (marker-position m)
                   (get-text-property 5 'tier)) log)
        (setf (ro-start op3) 1
              (ro-end op3) 5
              (ro-content op3) (buffer-substring 1 5))
        (kill-region 1 5)
        (push (list 'killed-outside (buffer-string) (marker-position m)) log)
        (setq log (reverse log))
        (goto-char (point-max))
        (insert (format " | log=%s op1=%s op2=%s op3=%s"
                       log
                       (list (ro-content op1))
                       (list (ro-content op2) (get-text-property 5 'tier))
                       (list (ro-content op3))))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'region-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                ops))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_kill_yank_multiple_yanks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 27 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass yank-tracker ()
    ((yank-number :initarg :yank-number :accessor yt-num :initform 0)
     (yanked-text :initarg :yanked-text :accessor yt-text :initform "")
     (pos :initarg :pos :accessor yt-pos :initform 1)))
  (let* ((buf (generate-new-buffer "ky3"))
         (yt1 (yank-tracker :yank-number 1))
         (yt2 (yank-tracker :yank-number 2))
         (yt3 (yank-tracker :yank-number 3)))
    (with-current-buffer buf
      (insert "ALPHA-BETA-GAMMA-DELTA-EPSILON")
      (put-text-property 1 6 'group 'a)
      (put-text-property 7 12 'group 'b)
      (put-text-property 13 19 'group 'c)
      (put-text-property 20 26 'group 'd)
      (put-text-property 27 34 'group 'e)
      (setq-local trackers (list yt1 yt2 yt3))
      (let* ((ov (make-overlay 7 19))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 7))
             (log nil))
        (undo-boundary)
        (kill-region 1 7)
        (push (list 'kill-alpha (buffer-string)) log)
        (kill-region 1 6)
        (push (list 'kill-beta (buffer-string)) log)
        (kill-region 1 7)
        (push (list 'kill-gamma (buffer-string)) log)
        (goto-char (point-max))
        (yank)
        (setf (yt-text yt1) (buffer-substring (mark t) (point)))
        (setf (yt-pos yt1) (mark t))
        (push (list 'yank1 (yt-text yt1) (yt-pos yt1)) log)
        (goto-char 1)
        (yank)
        (setf (yt-text yt2) (buffer-substring (mark t) (point)))
        (setf (yt-pos yt2) (mark t))
        (push (list 'yank2 (yt-text yt2) (yt-pos yt2)) log)
        (goto-char (point-max))
        (insert "SEP")
        (yank)
        (setf (yt-text yt3) (buffer-substring (- (point) (length (yt-text yt1))) (point)))
        (setf (yt-pos yt3) (- (point) (length (yt-text yt1))))
        (push (list 'yank3 (yt-text yt3) (yt-pos yt3)) log)
        (setq log (reverse log))
        (goto-char (point-max))
        (insert (format " | log=%s m=%d ov=[%d,%d]"
                       log (marker-position m) (overlay-start ov) (overlay-end ov)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'yank-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                trackers))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_kill_yank_with_marker_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass anchor ()
    ((name :initarg :name :accessor an-name :initform "")
     (marker :initarg :marker :accessor an-marker :initform nil)))
  (let* ((buf (generate-new-buffer "ky4"))
         (a1 (anchor :name "left"))
         (a2 (anchor :name "right"))
         (a3 (anchor :name "center")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 'alpha)
      (put-text-property 6 10 'zone 'beta)
      (put-text-property 11 15 'zone 'gamma)
      (put-text-property 16 20 'zone 'delta)
      (put-text-property 21 25 'zone 'epsilon)
      (setq-local anchors (list a1 a2 a3))
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (m3 (make-marker))
             (_ (set-marker m1 1))
             (_ (set-marker-insertion-type m1 nil))
             (_ (set-marker m2 10))
             (_ (set-marker-insertion-type m2 t))
             (_ (set-marker m3 6)))
        (setf (an-marker a1) m1 (an-marker a2) m2 (an-marker a3) m3)
        (undo-boundary)
        (let ((before (list (marker-position m1) (marker-position m2) (marker-position m3))))
          (kill-region 6 11)
          (let ((after-kill (list (marker-position m1) (marker-position m2) (marker-position m3))))
            (goto-char (point-max))
            (yank)
            (let ((after-yank (list (marker-position m1) (marker-position m2) (marker-position m3)
                                   (overlay-start ov) (overlay-end ov))))
              (goto-char (point-max))
              (insert (format " | before=%s after-kill=%s after-yank=%s"
                             before after-kill after-yank))
              (set-marker m3 4)
              (put-text-property (1- (point-max)) (point-max) 'anchor-log t))
            (undo-boundary)
            (let ((mp (marker-position m3))
                  (os (overlay-start ov))
                  (oe (overlay-end ov))
                  (bs (buffer-string)))
              (primitive-undo 1 buffer-undo-list)
              (list mp os oe bs
                    (marker-position m3)
                    (buffer-string)
                    anchors)))))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_kill_yank_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable m)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass edit-step ()
    ((step :initarg :step :accessor es-step :initform 0)
     (action :initarg :action :accessor es-action :initform "")
     (buf-len :initarg :buf-len :accessor es-len :initform 0)))
  (let* ((buf (generate-new-buffer "ky5"))
         (s1 (edit-step :step 1 :action "kill"))
         (s2 (edit-step :step 2 :action "yank"))
         (s3 (edit-step :step 3 :action "undo")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'order 1)
      (put-text-property 6 10 'order 2)
      (put-text-property 11 15 'order 3)
      (put-text-property 16 20 'order 4)
      (put-text-property 21 25 'order 5)
      (setq-local steps (list s1 s2 s3))
      (let* ((ov (make-overlay 1 25))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 13))
             (snapshots nil))
        (undo-boundary)
        (setf (es-len s1) (point-max))
        (kill-region 6 16)
        (push (list 'after-kill (buffer-string) (marker-position m)
                   (overlay-start ov) (overlay-end ov)) snapshots)
        (undo-boundary)
        (setf (es-len s2) (point-max))
        (goto-char (point-max))
        (yank)
        (push (list 'after-yank (buffer-string) (marker-position m)
                   (overlay-start ov) (overlay-end ov)
                   (get-text-property (1- (point-max)) 'order)) snapshots)
        (undo-boundary)
        (setf (es-len s3) (point-max))
        (primitive-undo 1 buffer-undo-list)
        (push (list 'after-undo1 (buffer-string) (marker-position m)
                   (overlay-start ov) (overlay-end ov)) snapshots)
        (primitive-undo 1 buffer-undo-list)
        (push (list 'after-undo2 (buffer-string) (marker-position m)
                   (overlay-start ov) (overlay-end ov)) snapshots)
        (setq snapshots (reverse snapshots))
        (goto-char (point-max))
        (insert (format " | snaps=%s s1=%d s2=%d s3=%d"
                       snapshots (es-len s1) (es-len s2) (es-len s3)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 'undo-log t))
      (list (marker-position m) (overlay-start ov) (overlay-end ov) (buffer-string)
            steps))
    (kill-buffer buf)))"#,
        expect,
    );
}
