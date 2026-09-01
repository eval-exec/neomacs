//! Combo: cl-labels + closures + EIEIO interop + overlays + markers
//! + textprop + buflocal + narrow + undo.
//! Tests complex local function definitions interacting with EIEIO objects,
//! closures over mutable state, and higher-order function chains.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_cl_labels_recursive_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass edit-chain ()
    ((buf-name :initarg :buf :accessor ech-buf :initform "")
     (edits :initarg :edits :accessor ech-edits :initform nil)
     (depth :initarg :depth :accessor ech-depth :initform 0)
     (log :initarg :log :accessor ech-log :initform nil)))
  (defmethod ech-run-chain ((chain edit-chain) positions)
    (with-current-buffer (ech-buf chain)
      (cl-labels
          ((do-edit (pos str remaining)
                    (when remaining
                      (let ((this-pos (car remaining))
                            (this-str (cadr remaining))
                            (rest (cddr remaining)))
                        (goto-char this-pos)
                        (insert this-str)
                        (setf (ech-depth chain) (1+ (ech-depth chain)))
                        (push (format "d%d@%d:%S" (ech-depth chain) this-pos this-str)
                              (ech-edits chain))
                        (do-edit this-pos this-str rest)))))
        (do-edit 1 "" positions))))
  (let* ((buf (generate-new-buffer "lb1"))
         (chain (edit-chain :buf (buffer-name buf) :edits nil :depth 0 :log nil))
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
      (setq-local my-ech-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (ech-run-chain chain (list 8 "XXX" 20 "YYY" 32 "ZZZ"))
        (push (list "chain1" (ech-depth chain) (ech-edits chain)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 50)
          (ech-run-chain chain (list 10 "AAA" 25 "BBB"))
          (push (list "chain-narrow" (ech-depth chain) (ech-edits chain)
                      (marker-position m) (point-min) (point-max)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S depth=%d"
                       results (ech-depth chain)))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (ech-depth chain) (ech-edits chain)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-ech-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_cl_labels_mutual_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mutual-ctx ()
    ((buf-name :initarg :buf :accessor mc-buf :initform "")
     (ops :initarg :ops :accessor mc-ops :initform nil)
     (toggle :initarg :toggle :accessor mc-toggle :initform nil)
     (log :initarg :log :accessor mc-log :initform nil)))
  (defmethod mc-run ((ctx mutual-ctx) steps)
    (with-current-buffer (mc-buf ctx)
      (cl-labels
          ((step-a (n pos)
                   (when (> n 0)
                     (goto-char pos)
                     (insert (format "A%d" n))
                     (push (format "A%d@%d" n pos) (mc-ops ctx))
                     (push 'a (mc-toggle ctx))
                     (step-b (1- n) (+ pos 5))))
           (step-b (n pos)
                   (when (> n 0)
                     (goto-char pos)
                     (insert (format "B%d" n))
                     (push (format "B%d@%d" n pos) (mc-ops ctx))
                     (push 'b (mc-toggle ctx))
                     (step-a (1- n) (+ pos 5))))))
        (step-a steps 8))))
  (let* ((buf (generate-new-buffer "lb2"))
         (ctx (mutual-ctx :buf (buffer-name buf) :ops nil :toggle nil :log nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (dotimes (i 10)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) 50)
                           'zone i))
      (setq-local my-mc-log nil)
      (let* ((ov (make-overlay 6 40))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 20)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (mc-run ctx 3)
        (push (list "run1" (mc-ops ctx) (mc-toggle ctx)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 80)
          (setf (mc-ops ctx) nil)
          (setf (mc-toggle ctx) nil)
          (mc-run ctx 2)
          (push (list "run-narrow" (mc-ops ctx) (mc-toggle ctx)
                      (marker-position m) (point-min) (point-max)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (mc-ops ctx) (mc-toggle ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-mc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_cl_labels_higher_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ho-ctx ()
    ((buf-name :initarg :buf :accessor ho-buf :initform "")
     (transforms :initarg :transforms :accessor ho-transforms :initform nil)
     (results :initarg :results :accessor ho-results :initform nil)
     (log :initarg :log :accessor ho-log :initform nil)))
  (defmethod ho-apply ((ctx ho-ctx) transformer pos)
    (with-current-buffer (ho-buf ctx)
      (let ((result (funcall transformer pos)))
        (push result (ho-results ctx))
        (push (format "apply@%d:%S" pos result) (ho-log ctx))
        result)))
  (defmethod ho-compose ((ctx ho-ctx) fns pos)
    (let ((result pos))
      (dolist (fn fns)
        (setq result (funcall fn result)))
      (push (list 'composed pos result) (ho-results ctx))
      result))
  (let* ((buf (generate-new-buffer "lb3"))
         (ctx (ho-ctx :buf (buffer-name buf) :transforms nil :results nil :log nil))
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
      (setq-local my-ho-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (cl-labels
            ((make-inserter
              (str)
              (lambda (pos)
                (goto-char pos)
                (insert str)
                (point)))
             (make-deleter
              (n)
              (lambda (pos)
                (delete-region pos (min (+ pos n) (point-max)))
                (point)))
             (make-prop-setter
              (prop val)
              (lambda (pos)
                (put-text-property pos (min (+ pos 3) (point-max)) prop val)
                (list pos prop val)))))
          (let* ((ins-xxx (make-inserter "XXX"))
                 (ins-yyy (make-inserter "YYY"))
                 (del-2 (make-deleter 2))
                 (prop-bold (make-prop-setter 'face 'bold)))
            (push (list "ins1" (ho-apply ctx ins-xxx 8)
                        (marker-position m)) results)
            (push (list "ins2" (ho-apply ctx ins-yyy 20)
                        (marker-position m)) results)
            (push (list "del" (ho-apply ctx del-2 10)
                        (marker-position m)) results)
            (push (list "prop" (ho-apply ctx prop-bold 15)
                        (marker-position m)) results)
            (push (list "compose"
                        (ho-compose ctx (list (make-inserter "ZZZ")
                                              (make-inserter "WWW"))
                                    30)
                        (marker-position m)) results)
            (save-restriction
              (narrow-to-region 5 50)
              (push (list "narrow-compose"
                          (ho-compose ctx (list (make-inserter "NNN"))
                                      10)
                          (marker-position m) (point-min) (point-max)) results))
            (setq results (reverse results))
            (goto-char (point-max))
            (insert (format " | results=%S ho-results=%S"
                           results (reverse (ho-results ctx))))
            (set-marker m 3)
            (list (buffer-substring-no-properties 1 (point-max))
                  (ho-results ctx) (ho-log ctx)
                  (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  my-ho-log))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_cl_labels_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass capture-ctx ()
    ((buf-name :initarg :buf :accessor cc-buf :initform "")
     (captured-vals :initarg :vals :accessor cc-vals :initform nil)
     (log :initarg :log :accessor cc-log :initform nil)))
  (defmethod cc-make-scanners ((ctx capture-ctx))
    (with-current-buffer (cc-buf ctx)
      (cl-labels
          ((make-scanner
            (prop start end label)
            (let ((count 0))
              (lambda ()
                (goto-char start)
                (setq count 0)
                (while (and (< (point) end) (< count 10))
                  (let ((next (next-single-char-property-change
                               (point) prop nil end)))
                    (when next
                      (setq count (1+ count))
                      (goto-char next))
                    (unless next (goto-char end))))
                (push (list label count) (cc-vals ctx))
                (push (format "scan:%s:%d" label count) (cc-log ctx))
                count)))))
        (list (make-scanner 'face 1 20 "face-scan")
              (make-scanner 'zone 1 20 "zone-scan")))))
  (let* ((buf (generate-new-buffer "lb4"))
         (ctx (capture-ctx :buf (buffer-name buf) :vals nil :log nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (setq-local my-cc-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (scanners (cc-make-scanners ctx)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "scan1" (funcall (nth 0 scanners))
                    (funcall (nth 1 scanners))
                    (cc-vals ctx)
                    (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-cc-log (cons "ins@8" my-cc-log))
        (setf (cc-vals ctx) nil)
        (push (list "scan2" (funcall (nth 0 scanners))
                    (funcall (nth 1 scanners))
                    (cc-vals ctx)
                    (marker-position m)) results)
        (put-text-property 9 12 'face 'error)
        (setf (cc-vals ctx) nil)
        (push (list "scan3" (funcall (nth 0 scanners))
                    (cc-vals ctx)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 25)
          (setf (cc-vals ctx) nil)
          (push (list "narrow-scan" (funcall (nth 0 scanners))
                      (cc-vals ctx) (point-min) (point-max)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S" results))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (cc-vals ctx) (cc-log ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-cc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_cl_labels_map_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ov-mapper ()
    ((buf-name :initarg :buf :accessor om-buf :initform "")
     (overlay-data :initarg :data :accessor om-data :initform nil)
     (log :initarg :log :accessor om-log :initform nil)))
  (defmethod om-map-overlays ((mapper ov-mapper) start end)
    (with-current-buffer (om-buf mapper)
      (setf (om-data mapper) nil)
      (cl-labels
          ((collect-ov (ov)
            (push (list (overlay-start ov)
                        (overlay-end ov)
                        (overlay-get ov 'face)
                        (overlay-get ov 'priority))
                  (om-data mapper))))
        (mapcar (lambda (ov) (collect-ov ov)) (overlays-in start end)))
      (setq (om-data mapper) (reverse (om-data mapper)))
      (push (format "map:%d-%d:%d" start end (length (om-data mapper))) (om-log mapper))
      (om-data mapper)))
  (defmethod om-modify-overlays ((mapper ov-mapper) start end delta)
    (with-current-buffer (om-buf mapper)
      (cl-labels
          ((shift-ov (ov)
            (let ((s (overlay-start ov))
                  (e (overlay-end ov)))
              (move-overlay ov (+ s delta) (+ e delta))
              (push (format "shift:%d->%d" s (+ s delta)) (om-log mapper)))))
        (mapcar (lambda (ov) (shift-ov ov)) (overlays-in start end)))))
  (let* ((buf (generate-new-buffer "lb5"))
         (mapper (ov-mapper :buf (buffer-name buf) :data nil :log nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (dotimes (i 10)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) 50)
                           'zone i))
      (setq-local my-om-log nil)
      (let* ((ov1 (make-overlay 6 10))
             (ov2 (make-overlay 16 20))
             (ov3 (make-overlay 26 30))
             (ov4 (make-overlay 36 40))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 5))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 10))
             (_ (overlay-put ov3 'face 'underline))
             (_ (overlay-put ov3 'priority 15))
             (_ (overlay-put ov4 'face 'shadow))
             (_ (overlay-put ov4 'priority 20))
             (m (set-marker (make-marker) 20)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "map-init" (om-map-overlays mapper 1 50)
                    (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-om-log (cons "ins@8" my-om-log))
        (push (list "map-edit" (om-map-overlays mapper 1 55)
                    (marker-position m)) results)
        (delete-overlay ov2)
        (push (list "map-del-ov2" (om-map-overlays mapper 1 55)
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 45)
          (push (list "narrow-map" (om-map-overlays mapper 5 45)
                      (point-min) (point-max)
                      (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S om-log=%S"
                       results (reverse (om-log mapper))))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (om-data mapper) (om-log mapper)
              (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov3) (overlay-end ov3)
              (overlay-start ov4) (overlay-end ov4)
              my-om-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
