//! Combo: format-mode-line + EIEIO objects + overlays + markers + textprop
//! + buflocal variables + narrow + undo.
//! Tests mode-line formatting interplay with EIEIO state and buffer ops.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_mode_line_basic_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mode-ctx ()
    ((label :initarg :label :accessor mcx-label :initform "")
     (buf-name :initarg :buf :accessor mcx-buf :initform "")
     (log :initarg :log :accessor mcx-log :initform nil)
     (edit-count :initarg :ec :accessor mcx-ec :initform 0)))
  (defmethod mcx-record-edit ((ctx mode-ctx) pos str)
    (setf (mcx-ec ctx) (1+ (mcx-ec ctx)))
    (push (format "edit@%d:%S" pos str) (mcx-log ctx)))
  (let* ((buf (generate-new-buffer "ml1"))
         (ctx (mode-ctx :label "ml-test" :buf (buffer-name buf) :log nil :ec 0)))
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
      (setq-local my-ml-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil)
             (ml1 (format-mode-line mode-line-format)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" ml1 (mcx-ec ctx) (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (mcx-record-edit ctx 8 "XXX")
        (setq my-ml-log (cons "ins@8" my-ml-log))
        (let ((ml2 (format-mode-line mode-line-format)))
          (push (list "edit" ml2 (mcx-ec ctx) (marker-position m)) results))
        (setq-local mode-name "TEST-MODE")
        (let ((ml3 (format-mode-line mode-line-format)))
          (push (list "custom-mode" ml3) results))
        (save-restriction
          (narrow-to-region 5 35)
          (let ((ml4 (format-mode-line mode-line-format)))
            (push (list "narrow" ml4 (point-min) (point-max)) results))
          (goto-char 10)
          (insert "YYY")
          (mcx-record-edit ctx 10 "YYY"))
        (let ((ml5 (format-mode-line mode-line-format)))
          (push (list "post-narrow" ml5 (mcx-ec ctx) (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ml-log=%S" results (reverse my-ml-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ml-log t)
        (list (buffer-substring-no-properties 1 (point-max))
              (mcx-ec ctx)
              (mcx-log ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ml-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_mode_line_custom_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mode-tracker ()
    ((buf-name :initarg :buf :accessor mt-buf :initform "")
     (snapshots :initarg :snaps :accessor mt-snaps :initform nil)
     (val :initarg :val :accessor mt-val :initform 0)
     (log :initarg :log :accessor mt-log :initform nil)))
  (defmethod mt-snap-mode ((tracker mode-tracker) label)
    (with-current-buffer (mt-buf tracker)
      (let ((ml (format-mode-line mode-line-format)))
        (push (list label ml (mt-val tracker)) (mt-snaps tracker))
        ml)))
  (let* ((buf (generate-new-buffer "ml2"))
         (tracker (mode-tracker :buf (buffer-name buf) :snaps nil :val 0 :log nil)))
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
      (setq-local my-mt-log nil)
      (let* ((ov (make-overlay 6 40))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 20))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (mt-snap-mode tracker "init")
        (setq-local mode-line-format
                    (list (propertize "[%b]" 'face 'mode-line-buffer-id)
                          " " mode-line-format))
        (mt-snap-mode tracker "custom-fmt")
        (push (list "snap-init" (mt-snaps tracker)) results)
        (goto-char 8)
        (insert "XXX")
        (setf (mt-val tracker) (1+ (mt-val tracker)))
        (push "ins@8" (mt-log tracker))
        (setq my-mt-log (cons "ins@8" my-mt-log))
        (mt-snap-mode tracker "after-edit")
        (save-restriction
          (narrow-to-region 5 45)
          (mt-snap-mode tracker "narrow")
          (push (list "narrow" (point-min) (point-max)) results)
          (goto-char 10)
          (insert "YYY")
          (setf (mt-val tracker) (+ (mt-val tracker) (marker-position m)))
          (mt-snap-mode tracker "narrow-edit"))
        (setq-local mode-name "CUSTOM")
        (mt-snap-mode tracker "custom-mode-name")
        (push (list "final" (mt-snaps tracker) (mt-val tracker)
                    (marker-position m)
                    (overlay-start ov) (overlay-end ov)) results)
        (setq results (reverse results))
        (list results (mt-log tracker) my-mt-log
              (mt-val tracker) (marker-position m))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_mode_line_read_only_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ro-ctx ()
    ((buf-name :initarg :buf :accessor roc-buf :initform "")
     (log :initarg :log :accessor roc-log :initform nil)
     (ro-edits :initarg :roe :accessor roc-roe :initform nil)))
  (defmethod roc-attempt-edit ((ctx ro-ctx) pos str)
    (condition-case err
        (with-current-buffer (roc-buf ctx)
          (goto-char pos)
          (insert str)
          (push (format "ok@%d" pos) (roc-log ctx)))
      (buffer-read-only
       (push (format "ro@%d" pos) (roc-roe ctx)))))
  (let* ((buf (generate-new-buffer "ml3"))
         (ctx (ro-ctx :buf (buffer-name buf) :log nil :roe nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (setq-local my-roc-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (ov-ro (make-overlay 16 25))
             (_ (overlay-put ov-ro 'read-only t))
             (_ (overlay-put ov-ro 'face 'error))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (let ((ml1 (format-mode-line mode-line-format)))
          (push (list "init" ml1 (buffer-read-only-p)) results))
        (roc-attempt-edit ctx 8 "XXX")
        (push (list "edit-ok" (roc-log ctx) (roc-roe ctx)
                    (marker-position m)) results)
        (roc-attempt-edit ctx 18 "YYY")
        (push (list "edit-ro" (roc-log ctx) (roc-roe ctx)
                    (marker-position m)) results)
        (setq-local buffer-read-only t)
        (let ((ml2 (format-mode-line mode-line-format)))
          (push (list "buf-ro" ml2 (buffer-read-only-p)) results))
        (roc-attempt-edit ctx 3 "ZZZ")
        (push (list "edit-buf-ro" (roc-log ctx) (roc-roe ctx)) results)
        (setq-local buffer-read-only nil)
        (roc-attempt-edit ctx 3 "ZZZ")
        (push (list "edit-unlocked" (roc-log ctx) (roc-roe ctx)
                    (marker-position m)) results)
        (let ((ml3 (format-mode-line mode-line-format)))
          (push (list "final" ml3 (buffer-read-only-p)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S roc-log=%S"
                       results my-roc-log))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (roc-log ctx) (roc-roe ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (overlay-start ov-ro) (overlay-end ov-ro))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_mode_line_multibuf_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass buf-observer ()
    ((name :initarg :name :accessor bo-name :initform "")
     (buf :initarg :buf :accessor bo-buf :initform nil)
     (mode-snaps :initarg :snaps :accessor bo-snaps :initform nil)
     (log :initarg :log :accessor bo-log :initform nil)))
  (defclass active-observer (buf-observer)
    ((active :initarg :active :accessor bao-active :initform t)))
  (defclass inactive-observer (buf-observer)
    ((reason :initarg :reason :accessor iao-reason :initform "")))
  (defmethod bo-snap ((obs buf-observer))
    (with-current-buffer (bo-buf obs)
      (let ((ml (format-mode-line mode-line-format)))
        (push ml (bo-snaps obs))
        ml)))
  (defmethod bo-snap ((obs active-observer))
    (when (bao-active obs)
      (cl-call-next-method)))
  (defmethod bo-snap ((obs inactive-observer))
    (push (format "inactive:%s" (iao-reason obs)) (bo-log obs))
    nil)
  (let* ((buf-a (generate-new-buffer "ml4a"))
         (buf-b (generate-new-buffer "ml4b"))
         (obs-a (active-observer :name "a" :buf buf-a :snaps nil :log nil :active t))
         (obs-b (inactive-observer :name "b" :buf buf-b :snaps nil :log nil :reason "paused"))
         (obs-c (active-observer :name "c" :buf buf-b :snaps nil :log nil :active nil))
         (results nil))
    (with-current-buffer buf-a
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-bo-log nil))
    (with-current-buffer buf-b
      (insert "XXXX-YYYY-ZZZZ-WWWW")
      (put-text-property 1 5 'face 'shadow)
      (put-text-property 6 10 'face 'success)
      (put-text-property 11 15 'face 'warning)
      (put-text-property 16 20 'face 'error))
    (push (list "snap-a" (bo-snap obs-a)) results)
    (push (list "snap-b" (bo-snap obs-b)) results)
    (push (list "snap-c" (bo-snap obs-c)) results)
    (with-current-buffer buf-a
      (let* ((ov (make-overlay 6 15))
             (m (set-marker (make-marker) 10)))
        (overlay-put ov 'face 'bold)
        (overlay-put ov 'priority 5)
        (goto-char 8)
        (insert "XXX")
        (setq my-bo-log (cons "ins@8" my-bo-log))
        (push (list "edit" (bo-snap obs-a) (marker-position m)
                    (overlay-start ov) (overlay-end ov)) results)
        (save-restriction
          (narrow-to-region 5 20)
          (push (list "narrow" (bo-snap obs-a) (point-min) (point-max)) results))
        (setf (bao-active obs-c) t)
        (push (list "snap-c-activated" (bo-snap obs-c)) results)))
    (setq results (reverse results))
    (list results
          (bo-snaps obs-a) (bo-snaps obs-b) (bo-snaps obs-c)
          (bo-log obs-b) (bo-log obs-c)
          (with-current-buffer buf-a my-bo-log)
          (cl-typep obs-a 'active-observer)
          (cl-typep obs-b 'inactive-observer)
          (cl-typep obs-c 'active-observer))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_mode_line_props_and_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ml-state ()
    ((buf-name :initarg :buf :accessor mls-buf :initform "")
     (edits :initarg :edits :accessor mls-edits :initform nil)
     (mode-history :initarg :mh :accessor mls-mh :initform nil)
     (counter :initarg :ctr :accessor mls-ctr :initform 0)))
  (defmethod mls-do-edit ((state ml-state) pos str)
    (with-current-buffer (mls-buf state)
      (goto-char pos)
      (insert str)
      (setf (mls-ctr state) (1+ (mls-ctr state)))
      (push (format "edit@%d:%S:ctr=%d" pos str (mls-ctr state)) (mls-edits state))
      (let ((ml (format-mode-line mode-line-format)))
        (push (format "after-edit:%S" ml) (mls-mh state))
        ml)))
  (let* ((buf (generate-new-buffer "ml5"))
         (state (ml-state :buf (buffer-name buf) :edits nil :mh nil :ctr 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ-KKKK-LLLL")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'shadow)
      (put-text-property 26 30 'face 'highlight)
      (put-text-property 31 35 'face 'success)
      (put-text-property 36 40 'face 'warning)
      (put-text-property 41 45 'face 'error)
      (put-text-property 46 50 'face 'match)
      (setq-local my-mls-log nil)
      (let* ((ov1 (make-overlay 6 20))
             (ov2 (make-overlay 26 40))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 10))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 5))
             (m (set-marker (make-marker) 25))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (format-mode-line mode-line-format)
                    (mls-ctr state) (marker-position m)) results)
        (push (list "edit1" (mls-do-edit state 8 "XXX")
                    (marker-position m)) results)
        (setq-local mode-name "EDIT1")
        (push (list "mode-edit1" (format-mode-line mode-line-format)) results)
        (save-restriction
          (narrow-to-region 5 45)
          (push (list "edit2" (mls-do-edit state 10 "YYY")
                      (marker-position m) (point-min) (point-max)) results)
          (setq-local mode-name "NARROW-EDIT")
          (push (list "narrow-mode" (format-mode-line mode-line-format)) results))
        (push (list "edit3" (mls-do-edit state 20 "ZZZ")
                    (marker-position m)) results)
        (setq-local mode-name "FINAL")
        (push (list "final" (format-mode-line mode-line-format)
                    (mls-ctr state) (marker-position m)
                    (overlay-start ov1) (overlay-end ov1)
                    (overlay-start ov2) (overlay-end ov2)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S mls-edits=%S mls-mh=%S"
                       results
                       (reverse (mls-edits state))
                       (reverse (mls-mh state))))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (mls-ctr state)
              (mls-edits state) (mls-mh state)
              (marker-position m)
              my-mls-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
