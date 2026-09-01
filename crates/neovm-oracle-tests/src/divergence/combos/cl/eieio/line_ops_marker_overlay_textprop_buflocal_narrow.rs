//! Combo: cl-eieio line-boundary operations (line-beginning-position,
//! line-end-position, forward-line, count-lines) + overlays + markers
//! + textprop + buflocal + narrow + undo.
//! Tests line operations in complex states with narrowing, overlays, and edits.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_line_ops_narrow_overlay_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass line-snap ()
    ((step :initarg :step :accessor ls-step :initform "")
     (line-num :initarg :ln :accessor ls-ln :initform 0)
     (bol :initarg :bol :accessor ls-bol :initform 0)
     (eol :initarg :eol :accessor ls-eol :initform 0)
     (m-pos :initarg :m-pos :accessor ls-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ln1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "line-one\nline-two\nline-three\nline-four\nline-five\n")
      (put-text-property 1 9 'face 'bold)
      (put-text-property 10 18 'face 'italic)
      (put-text-property 19 30 'face 'underline)
      (put-text-property 31 40 'face 'default)
      (put-text-property 41 50 'face 'highlight)
      (setq-local my-ln-log nil)
      (let* ((ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 20))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (goto-char 20)
        (push (line-snap :step "init"
                        :ln (line-number-at-pos)
                        :bol (line-beginning-position)
                        :eol (line-end-position)
                        :m-pos (marker-position m)) snaps)
        (forward-line 1)
        (push (line-snap :step "fwd-line"
                        :ln (line-number-at-pos)
                        :bol (line-beginning-position)
                        :eol (line-end-position)
                        :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 10 40)
          (push (line-snap :step "narrow"
                          :ln (line-number-at-pos)
                          :bol (line-beginning-position)
                          :eol (line-end-position)
                          :m-pos (marker-position m)) snaps)
          (goto-char 15)
          (insert "INSERTED\n")
          (setq my-ln-log (cons "ins-narrow@15" my-ln-log))
          (push (line-snap :step "edit-narrow"
                          :ln (line-number-at-pos)
                          :bol (line-beginning-position)
                          :eol (line-end-position)
                          :m-pos (marker-position m)) snaps))
        (push (line-snap :step "widen"
                        :ln (line-number-at-pos)
                        :bol (line-beginning-position)
                        :eol (line-end-position)
                        :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ls-step s) (ls-ln s)
                                                (ls-bol s) (ls-eol s)
                                                (ls-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S ln-log=%S"
                       results (reverse my-ln-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ls-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (count-lines (point-min) (point-max))
              my-ln-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_line_ops_count_lines_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass count-snap ()
    ((step :initarg :step :accessor cs-step :initform "")
     (total-lines :initarg :tl :accessor cs-tl :initform 0)
     (narrow-lines :initarg :nl :accessor cs-nl :initform 0)
     (m-pos :initarg :m-pos :accessor cs-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ln2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "A\nB\nC\nD\nE\nF\nG\nH\nI\nJ\n")
      (dotimes (i 10)
        (put-text-property (+ 1 (* i 2)) (+ 2 (* i 2)) 'face 'bold))
      (setq-local my-count-log nil)
      (let* ((ov (make-overlay 5 15))
             (_ (overlay-put ov 'face 'italic))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (count-snap :step "init"
                         :tl (count-lines (point-min) (point-max))
                         :nl (count-lines (point-min) (point-max))
                         :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 3 15)
          (push (count-snap :step "narrow"
                           :tl (count-lines (point-min) (point-max))
                           :nl (count-lines (point-min) (point-max))
                           :m-pos (marker-position m)) snaps)
          (goto-char 5)
          (insert "X\nY\n")
          (setq my-count-log (cons "ins-narrow@5" my-count-log))
          (push (count-snap :step "edit-narrow"
                           :tl (count-lines (point-min) (point-max))
                           :nl (count-lines (point-min) (point-max))
                           :m-pos (marker-position m)) snaps))
        (push (count-snap :step "widen"
                         :tl (count-lines (point-min) (point-max))
                         :nl (count-lines (point-min) (point-max))
                         :m-pos (marker-position m)) snaps)
        (goto-char 7)
        (insert "NEW-LINE\n")
        (setq my-count-log (cons "ins@7" my-count-log))
        (push (count-snap :step "edit-widen"
                         :tl (count-lines (point-min) (point-max))
                         :nl (count-lines (point-min) (point-max))
                         :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (cs-step s) (cs-tl s)
                                                (cs-nl s) (cs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S count-log=%S"
                       results (reverse my-count-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'cs-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (count-lines (point-min) (point-max))
              my-count-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_line_ops_forward_line_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fwdln-snap ()
    ((step :initarg :step :accessor fls-step :initform "")
     (point-line :initarg :pl :accessor fls-pl :initform 0)
     (m-line :initarg :ml :accessor fls-ml :initform 0)
     (m-pos :initarg :m-pos :accessor fls-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ln3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "L1\nL2\nL3\nL4\nL5\nL6\nL7\nL8\n")
      (dotimes (i 8)
        (put-text-property (+ 1 (* i 3)) (+ 3 (* i 3)) 'line-num (1+ i)))
      (setq-local my-fl-log nil)
      (let* ((ov (make-overlay 7 16))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (goto-char 1)
        (push (fwdln-snap :step "init"
                         :pl (line-number-at-pos)
                         :ml (line-number-at-pos (marker-position m))
                         :m-pos (marker-position m)) snaps)
        (forward-line 3)
        (push (fwdln-snap :step "fwd3"
                         :pl (line-number-at-pos)
                         :ml (line-number-at-pos (marker-position m))
                         :m-pos (marker-position m)) snaps)
        (goto-char (line-beginning-position))
        (insert "INS\n")
        (setq my-fl-log (cons "ins-line" my-fl-log))
        (push (fwdln-snap :step "insert"
                         :pl (line-number-at-pos)
                         :ml (line-number-at-pos (marker-position m))
                         :m-pos (marker-position m)) snaps)
        (forward-line -2)
        (push (fwdln-snap :step "back2"
                         :pl (line-number-at-pos)
                         :ml (line-number-at-pos (marker-position m))
                         :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fls-step s) (fls-pl s)
                                                (fls-ml s) (fls-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S fl-log=%S"
                       results (reverse my-fl-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fls-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-fl-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_line_ops_delete_line_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass del-line-snap ()
    ((step :initarg :step :accessor dls-step :initform "")
     (line-count :initarg :lc :accessor dls-lc :initform 0)
     (m-pos :initarg :m-pos :accessor dls-mp :initform 0)
     (ov-bounds :initarg :ov :accessor dls-ov :initform nil)))
  (let* ((buf (generate-new-buffer "ln4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "A\nB\nC\nD\nE\nF\nG\n")
      (dotimes (i 7)
        (put-text-property (+ 1 (* i 2)) (+ 2 (* i 2)) 'face 'bold))
      (setq-local my-dl-log nil)
      (let* ((ov (make-overlay 5 11))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (_ (overlay-put ov 'evaporate t))
             (m (set-marker (make-marker) 7))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (del-line-snap :step "init"
                            :lc (count-lines (point-min) (point-max))
                            :m-pos (marker-position m)
                            :ov (list (overlay-start ov) (overlay-end ov)
                                     (overlay-live-p ov))) snaps)
        (goto-char 5)
        (kill-line)
        (kill-line)
        (setq my-dl-log (cons "kill-lines@5" my-dl-log))
        (push (del-line-snap :step "kill-lines"
                            :lc (count-lines (point-min) (point-max))
                            :m-pos (marker-position m)
                            :ov (list (if (overlay-live-p ov) (overlay-start ov) -1)
                                     (if (overlay-live-p ov) (overlay-end ov) -1)
                                     (overlay-live-p ov))) snaps)
        (undo-boundary)
        (goto-char 3)
        (insert "X\n")
        (setq my-dl-log (cons "ins@3" my-dl-log))
        (push (del-line-snap :step "insert"
                            :lc (count-lines (point-min) (point-max))
                            :m-pos (marker-position m)
                            :ov (list (if (overlay-live-p ov) (overlay-start ov) -1)
                                     (if (overlay-live-p ov) (overlay-end ov) -1)
                                     (overlay-live-p ov))) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (del-line-snap :step "undo-ins"
                            :lc (count-lines (point-min) (point-max))
                            :m-pos (marker-position m)
                            :ov (list (if (overlay-live-p ov) (overlay-start ov) -1)
                                     (if (overlay-live-p ov) (overlay-end ov) -1)
                                     (overlay-live-p ov))) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (del-line-snap :step "undo-kill"
                            :lc (count-lines (point-min) (point-max))
                            :m-pos (marker-position m)
                            :ov (list (if (overlay-live-p ov) (overlay-start ov) -1)
                                     (if (overlay-live-p ov) (overlay-end ov) -1)
                                     (overlay-live-p ov))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (dls-step s) (dls-lc s)
                                                (dls-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S dl-log=%S"
                       results (reverse my-dl-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'dls-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-live-p ov)
              my-dl-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_line_ops_buflocal_with_line_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass bl-line-snap ()
    ((step :initarg :step :accessor bls-step :initform "")
     (tab-w :initarg :tw :accessor bls-tw :initform 8)
     (line-num :initarg :ln :accessor bls-ln :initform 0)
     (m-pos :initarg :m-pos :accessor bls-mp :initform 0)))
  (let* ((buf (generate-new-buffer "ln5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAA\nBBB\nCCC\nDDD\nEEE\nFFF\nGGG\n")
      (dotimes (i 7)
        (put-text-property (+ 1 (* i 4)) (+ 4 (* i 4)) 'zone (1+ i)))
      (setq-local tab-width 4)
      (setq-local fill-column 50)
      (setq-local my-bll-log nil)
      (let* ((ov (make-overlay 5 17))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 8))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (bl-line-snap :step "init"
                           :tw tab-width
                           :ln (line-number-at-pos (marker-position m))
                           :m-pos (marker-position m)) snaps)
        (setq-local tab-width 8)
        (setq my-bll-log (cons "tw->8" my-bll-log))
        (push (bl-line-snap :step "tw-change"
                           :tw tab-width
                           :ln (line-number-at-pos (marker-position m))
                           :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 5 20)
          (push (bl-line-snap :step "narrow"
                             :tw tab-width
                             :ln (line-number-at-pos (marker-position m))
                             :m-pos (marker-position m)) snaps)
          (goto-char 8)
          (insert "NNN\n")
          (setq my-bll-log (cons "ins-narrow@8" my-bll-log))
          (push (bl-line-snap :step "edit-narrow"
                             :tw tab-width
                             :ln (line-number-at-pos (marker-position m))
                             :m-pos (marker-position m)) snaps))
        (push (bl-line-snap :step "widen"
                           :tw tab-width
                           :ln (line-number-at-pos (marker-position m))
                           :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (bls-step s) (bls-tw s)
                                                (bls-ln s) (bls-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S bll-log=%S"
                       results (reverse my-bll-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'bls-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              tab-width
              (count-lines (point-min) (point-max))
              my-bll-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
