//! Combo: window-point + save-window-excursion + EIEIO state + overlays
//! + markers + textprop + buflocal + narrow + undo.
//! Tests window configuration management with EIEIO objects tracking
//! window-local state through editing and narrowing operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_window_point_basic_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass win-state ()
    ((buf-name :initarg :buf :accessor ws-buf :initform "")
     (saved-point :initarg :pt :accessor ws-pt :initform 1)
     (saved-start :initarg :start :accessor ws-start :initform 1)
     (snapshots :initarg :snaps :accessor ws-snaps :initform nil)
     (log :initarg :log :accessor ws-log :initform nil)))
  (defmethod ws-snap ((state win-state) label)
    (with-current-buffer (ws-buf state)
      (let ((snap (list label (point) (window-start) (window-end)
                        (marker-position (mark-marker))
                        (ws-pt state))))
        (push snap (ws-snaps state))
        (push (format "snap:%s@%d" label (point)) (ws-log state))
        snap)))
  (let* ((buf (generate-new-buffer "wp1"))
         (state (win-state :buf (buffer-name buf) :pt 1 :start 1 :snaps nil :log nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ-KKKK-LLLL")
      (dotimes (i 12)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) 60)
                           'zone i))
      (setq-local my-wp-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (goto-char 10)
        (set-window-start (selected-window) 1)
        (ws-snap state "init")
        (push (list "init" (point) (marker-position m)) results)
        (save-window-excursion
          (goto-char 25)
          (set-window-point (selected-window) 25)
          (setf (ws-pt state) 25)
          (insert "XXX")
          (push "ins@25" my-wp-log)
          (ws-snap state "excursion")
          (push (list "excursion" (point) (marker-position m)) results))
        (push (list "after-excursion" (point) (marker-position m)) results)
        (ws-snap state "restored")
        (goto-char 8)
        (insert "YYY")
        (setf (ws-pt state) (point))
        (push "ins@8" my-wp-log)
        (ws-snap state "edit")
        (push (list "edit" (point) (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 55)
          (goto-char 10)
          (insert "ZZZ")
          (ws-snap state "narrow")
          (push (list "narrow" (point) (marker-position m)
                      (point-min) (point-max)) results))
        (ws-snap state "post-narrow")
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S snaps=%S log=%S"
                       results
                       (reverse (ws-snaps state))
                       (reverse my-wp-log)))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (ws-pt state)
              (ws-snaps state)
              (ws-log state)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-wp-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_window_point_marker_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass point-tracker ()
    ((buf-name :initarg :buf :accessor pt-buf :initform "")
     (markers :initarg :markers :accessor pt-markers :initform nil)
     (positions :initarg :positions :accessor pt-positions :initform nil)
     (log :initarg :log :accessor pt-log :initform nil)))
  (defmethod pt-track-points ((tracker point-tracker) positions)
    (with-current-buffer (pt-buf tracker)
      (setf (pt-positions tracker) nil)
      (setf (pt-markers tracker) nil)
      (dolist (pos positions)
        (let ((m (set-marker (make-marker) pos)))
          (push m (pt-markers tracker))
          (push pos (pt-positions tracker))))
      (setq (pt-markers tracker) (reverse (pt-markers tracker)))
      (setq (pt-positions tracker) (reverse (pt-positions tracker)))))
  (defmethod pt-snap-markers ((tracker point-tracker))
    (mapcar (lambda (m) (marker-position m)) (pt-markers tracker)))
  (let* ((buf (generate-new-buffer "wp2"))
         (tracker (point-tracker :buf (buffer-name buf) :markers nil :positions nil :log nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (dotimes (i 10)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) 50)
                           'zone i))
      (setq-local my-pt-log nil)
      (let* ((ov (make-overlay 6 40))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (pt-track-points tracker (list 5 10 15 20 25 30 35 40))
        (push (list "init" (pt-snap-markers tracker)) results)
        (goto-char 8)
        (insert "XXX")
        (push "ins@8" my-pt-log)
        (push (list "edit1" (pt-snap-markers tracker)) results)
        (save-window-excursion
          (goto-char 20)
          (insert "YYY")
          (push "excursion-ins@20" my-pt-log)
          (push (list "excursion" (pt-snap-markers tracker)) results))
        (push (list "after-excursion" (pt-snap-markers tracker)) results)
        (save-restriction
          (narrow-to-region 5 50)
          (goto-char 10)
          (insert "ZZZ")
          (push "narrow-ins@10" my-pt-log)
          (push (list "narrow" (pt-snap-markers tracker)
                      (point-min) (point-max)) results))
        (push (list "post-narrow" (pt-snap-markers tracker)) results)
        (goto-char 15)
        (insert "WWW")
        (push "ins@15" my-pt-log)
        (push (list "final" (pt-snap-markers tracker)
                    (overlay-start ov) (overlay-end ov)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S pt-log=%S"
                       results (reverse my-pt-log)))
        (list (buffer-substring-no-properties 1 (point-max))
              results (pt-snap-markers tracker)
              my-pt-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_window_point_overlay_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass overlay-manager ()
    ((buf-name :initarg :buf :accessor om-buf :initform "")
     (overlays :initarg :ovs :accessor om-ovs :initform nil)
     (log :initarg :log :accessor om-log :initform nil)))
  (defmethod om-create-overlay ((mgr overlay-manager) start end face)
    (with-current-buffer (om-buf mgr)
      (let ((ov (make-overlay start end)))
        (overlay-put ov 'face face)
        (overlay-put ov 'priority 5)
        (overlay-put ov 'mgr t)
        (push ov (om-ovs mgr))
        (push (format "create:%d-%d:%S" start end face) (om-log mgr))
        ov)))
  (defmethod om-snap-overlays ((mgr overlay-manager))
    (mapcar (lambda (ov)
             (list (overlay-start ov) (overlay-end ov)
                   (overlay-get ov 'face)
                   (overlay-get ov 'mgr)))
            (om-ovs mgr)))
  (let* ((buf (generate-new-buffer "wp3"))
         (mgr (overlay-manager :buf (buffer-name buf) :ovs nil :log nil))
         (results nil))
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
      (setq-local my-om-log nil)
      (let ((m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (om-create-overlay mgr 6 15 'bold)
        (om-create-overlay mgr 16 25 'italic)
        (om-create-overlay mgr 26 35 'underline)
        (push (list "init" (om-snap-overlays mgr)
                    (marker-position m)) results)
        (save-window-excursion
          (goto-char 10)
          (insert "XXX")
          (push (list "excursion" (om-snap-overlays mgr)
                      (marker-position m)) results)
          (move-overlay (nth 0 (om-ovs mgr)) 5 18)
          (push (list "move-ov" (om-snap-overlays mgr)
                      (marker-position m)) results))
        (push (list "after-excursion" (om-snap-overlays mgr)
                    (marker-position m)) results)
        (goto-char 20)
        (insert "YYY")
        (push (list "edit" (om-snap-overlays mgr)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 40)
          (push (list "narrow" (om-snap-overlays mgr)
                      (point-min) (point-max)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S om-log=%S"
                       results (reverse (om-log mgr))))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (om-snap-overlays mgr)
              (om-log mgr)
              my-om-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_window_point_multibuf_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass buf-switcher ()
    ((current-buf :initarg :cur :accessor bs-cur :initform nil)
     (bufs :initarg :bufs :accessor bs-bufs :initform nil)
     (markers :initarg :markers :accessor bs-markers :initform nil)
     (log :initarg :log :accessor bs-log :initform nil)))
  (defmethod bs-switch-to ((switcher buf-switcher) idx)
    (let ((target (nth idx (bs-bufs switcher))))
      (setf (bs-cur switcher) target)
      (set-buffer target)
      (push (format "switch:%d" idx) (bs-log switcher))))
  (defmethod bs-edit-current ((switcher buf-switcher) pos str)
    (with-current-buffer (bs-cur switcher)
      (goto-char pos)
      (insert str)
      (push (format "edit@%d:%S" pos str) (bs-log switcher))))
  (defmethod bs-snap-all ((switcher buf-switcher))
    (mapcar (lambda (b)
             (with-current-buffer b
               (list (buffer-name b) (point) (point-max))))
            (bs-bufs switcher)))
  (let* ((buf-a (generate-new-buffer "wp4a"))
         (buf-b (generate-new-buffer "wp4b"))
         (buf-c (generate-new-buffer "wp4c"))
         (switcher (buf-switcher :cur buf-a
                                 :bufs (list buf-a buf-b buf-c)
                                 :markers nil :log nil))
         (results nil))
    (with-current-buffer buf-a
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE"))
    (with-current-buffer buf-b
      (insert "1111-2222-3333-4444-5555"))
    (with-current-buffer buf-c
      (insert "XXXX-YYYY-ZZZZ-WWWW-VVVV"))
    (push (list "init" (bs-snap-all switcher)) results)
    (save-window-excursion
      (with-current-buffer buf-a
        (let ((m-a (set-marker (make-marker) 10))
              (m-b (with-current-buffer buf-b (set-marker (make-marker) 10)))
              (m-c (with-current-buffer buf-c (set-marker (make-marker) 10))))
          (push (list "markers-init"
                      (marker-position m-a)
                      (marker-position m-b)
                      (marker-position m-c)) results)
          (bs-switch-to switcher 0)
          (bs-edit-current switcher 8 "XXX")
          (bs-switch-to switcher 1)
          (bs-edit-current switcher 12 "YYY")
          (bs-switch-to switcher 2)
          (bs-edit-current switcher 15 "ZZZ")
          (push (list "after-edits"
                      (marker-position m-a)
                      (marker-position m-b)
                      (marker-position m-c)
                      (bs-snap-all switcher)) results)
          (save-restriction
            (with-current-buffer buf-a
              (narrow-to-region 5 20)
              (bs-switch-to switcher 0)
              (bs-edit-current switcher 6 "NNN")
              (push (list "narrow-edit"
                          (marker-position m-a)
                          (bs-snap-all switcher)
                          (point-min) (point-max)) results)))
          (push (list "final"
                      (marker-position m-a)
                      (marker-position m-b)
                      (marker-position m-c)
                      (bs-snap-all switcher)) results))))
    (setq results (reverse results))
    (list results (bs-log switcher))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_window_point_deep_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass excursion-ctx ()
    ((buf-name :initarg :buf :accessor ec-buf :initform "")
     (depth :initarg :depth :accessor ec-depth :initform 0)
     (max-depth :initarg :max :accessor ec-max :initform 0)
     (ops :initarg :ops :accessor ec-ops :initform nil)
     (log :initarg :log :accessor ec-log :initform nil)))
  (defmethod ec-do-at-depth ((ctx excursion-ctx) depth pos str)
    (with-current-buffer (ec-buf ctx)
      (setf (ec-depth ctx) depth)
      (setf (ec-max ctx) (max (ec-max ctx) depth))
      (goto-char pos)
      (insert str)
      (push (format "d%d@%d:%S" depth pos str) (ec-ops ctx))))
  (let* ((buf (generate-new-buffer "wp5"))
         (ctx (excursion-ctx :buf (buffer-name buf) :depth 0 :max 0 :ops nil :log nil))
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
        (ec-do-at-depth ctx 0 8 "A")
        (push (list "d0" (marker-position m) (ec-ops ctx)) results)
        (save-excursion
          (ec-do-at-depth ctx 1 15 "B")
          (push (list "d1" (marker-position m) (ec-ops ctx)) results)
          (save-excursion
            (ec-do-at-depth ctx 2 25 "C")
            (push (list "d2" (marker-position m) (ec-ops ctx)) results)
            (save-excursion
              (ec-do-at-depth ctx 3 35 "D")
              (push (list "d3" (marker-position m) (ec-ops ctx)) results))
            (push (list "d2-back" (marker-position m)) results))
          (push (list "d1-back" (marker-position m)) results))
        (push (list "d0-back" (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 50)
          (ec-do-at-depth ctx 1 10 "E")
          (push (list "narrow-d1" (marker-position m)
                      (point-min) (point-max)) results)
          (save-excursion
            (ec-do-at-depth ctx 2 20 "F")
            (push (list "narrow-d2" (marker-position m)) results)))
        (push (list "post-narrow" (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S max-depth=%d ops=%S"
                       results (ec-max ctx) (reverse (ec-ops ctx))))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (ec-max ctx)
              (ec-ops ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ec-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
