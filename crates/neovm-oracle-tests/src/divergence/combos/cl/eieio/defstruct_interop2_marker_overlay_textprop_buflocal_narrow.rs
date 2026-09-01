//! Combo: cl-defstruct + EIEIO interop + overlays + markers + textprop
//! + buflocal variables + narrow + undo.
//! Tests complex interactions between defstruct instances and defclass objects
//! sharing state through editing operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_defstruct_bridge_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (bridge-node (:constructor bridge-node-create))
    label marker overlay log)
  (defclass bridge-ctx ()
    ((name :initarg :name :accessor bc-name :initform "")
     (struct-nodes :initarg :nodes :accessor bc-nodes :initform nil)
     (edit-count :initarg :ec :accessor bc-ec :initform 0)
     (log :initarg :log :accessor bc-log :initform nil)))
  (defmethod bc-add-node ((ctx bridge-ctx) buf label pos start end)
    (with-current-buffer buf
      (let* ((m (set-marker (make-marker) pos))
             (ov (make-overlay start end))
             (nd (bridge-node-create :label label :marker m :overlay ov :log nil)))
        (overlay-put ov 'face (intern label))
        (overlay-put ov 'priority 5)
        (push nd (bc-nodes ctx))
        nd)))
  (defmethod bc-edit-at-node ((ctx bridge-ctx) nd insert-pos str)
    (let ((m (bridge-node-marker nd))
          (ov (bridge-node-overlay nd)))
      (goto-char insert-pos)
      (insert str)
      (setf (bc-ec ctx) (1+ (bc-ec ctx)))
      (push (format "edit@%d:%S" insert-pos str) (bridge-node-log nd))
      (push (format "node-%s@%d" (bridge-node-label nd) insert-pos) (bc-log ctx))
      (list (marker-position m) (overlay-start ov) (overlay-end ov))))
  (let* ((buf (generate-new-buffer "ds1"))
         (ctx (bridge-ctx :name "bridge" :nodes nil :ec 0 :log nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (dotimes (i 10)
        (put-text-property (1+ (* i 5)) (min (+ 5 (* i 5)) 50)
                           'zone i))
      (setq-local my-bc-log nil)
      (bc-add-node ctx buf "bold" 10 6 15)
      (bc-add-node ctx buf "italic" 25 21 30)
      (bc-add-node ctx buf "underline" 40 36 45)
      (setq buffer-undo-list nil)
      (undo-boundary)
      (let* ((results nil)
             (n1 (nth 0 (bc-nodes ctx)))
             (n2 (nth 1 (bc-nodes ctx)))
             (n3 (nth 2 (bc-nodes ctx))))
        (push (list "init"
                    (bridge-node-label n1) (bridge-node-label n2) (bridge-node-label n3)
                    (marker-position (bridge-node-marker n1))
                    (marker-position (bridge-node-marker n2))
                    (marker-position (bridge-node-marker n3))
                    (bc-ec ctx)) results)
        (push (list "edit-n1" (bc-edit-at-node ctx n1 8 "XXX")) results)
        (push (list "edit-n2" (bc-edit-at-node ctx n2 22 "YYY")) results)
        (push (list "edit-n3" (bc-edit-at-node ctx n3 38 "ZZZ")) results)
        (setq my-bc-log (cons "3-edits" my-bc-log))
        (push (list "after-edits"
                    (bc-ec ctx)
                    (mapcar (lambda (nd)
                             (list (bridge-node-label nd)
                                   (marker-position (bridge-node-marker nd))
                                   (overlay-start (bridge-node-overlay nd))
                                   (overlay-end (bridge-node-overlay nd))
                                   (bridge-node-log nd)))
                            (bc-nodes ctx))) results)
        (save-restriction
          (narrow-to-region 5 50)
          (push (list "edit-narrow-n1" (bc-edit-at-node ctx n1 10 "AAA")) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S bc-log=%S"
                       results (reverse (bc-log ctx))))
        (list (buffer-substring-no-properties 1 (point-max))
              (bc-ec ctx)
              (bc-log ctx)
              (mapcar (lambda (nd)
                       (list (bridge-node-label nd)
                             (marker-position (bridge-node-marker nd))
                             (overlay-start (bridge-node-overlay nd))
                             (overlay-end (bridge-node-overlay nd))
                             (bridge-node-log nd)))
                      (bc-nodes ctx))
              my-bc-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defstruct_shared_state_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (xform-step (:constructor xform-step-create))
    label pos str result)
  (defclass xform-engine ()
    ((steps :initarg :steps :accessor xe-steps :initform nil)
     (buf-name :initarg :buf :accessor xe-buf :initform "")
     (applied :initarg :applied :accessor xe-applied :initform nil)
     (log :initarg :log :accessor xe-log :initform nil)))
  (defmethod xe-apply-step ((engine xform-engine) step)
    (with-current-buffer (xe-buf engine)
      (let* ((pos (xform-step-pos step))
             (str (xform-step-str step))
             (before (buffer-substring-no-properties pos (min (+ pos 5) (point-max)))))
        (goto-char pos)
        (insert str)
        (setf (xform-step-result step) (list before pos (point)))
        (push step (xe-applied engine))
        (push (format "apply:%s@%d" (xform-step-label step) pos) (xe-log engine)))))
  (let* ((buf (generate-new-buffer "ds2"))
         (engine (xform-engine :steps nil :buf (buffer-name buf) :applied nil :log nil))
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
      (setq-local my-xe-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (s1 (xform-step-create :label "ins1" :pos 8 :str "XXX" :result nil))
             (s2 (xform-step-create :label "ins2" :pos 18 :str "YYY" :result nil))
             (s3 (xform-step-create :label "ins3" :pos 28 :str "ZZZ" :result nil))
             (s4 (xform-step-create :label "ins4" :pos 12 :str "WWW" :result nil)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (xe-apply-step engine s1)
        (push (list "s1" (xform-step-result s1) (marker-position m)) results)
        (xe-apply-step engine s2)
        (push (list "s2" (xform-step-result s2) (marker-position m)) results)
        (xe-apply-step engine s3)
        (push (list "s3" (xform-step-result s3) (marker-position m)) results)
        (setq my-xe-log (cons "3-applied" my-xe-log))
        (save-restriction
          (narrow-to-region 5 45)
          (xe-apply-step engine s4)
          (push (list "s4-narrow" (xform-step-result s4)
                      (marker-position m) (point-min) (point-max)) results)
          (setq my-xe-log (cons "narrow-apply" my-xe-log)))
        (push (list "final"
                    (mapcar (lambda (s)
                             (list (xform-step-label s) (xform-step-result s)))
                            (xe-applied engine))
                    (xe-log engine)
                    (marker-position m)
                    (overlay-start ov) (overlay-end ov)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S xe-applied=%d"
                       results (length (xe-applied engine))))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              results
              (length (xe-applied engine))
              (xe-log engine)
              my-xe-log))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defstruct_class_hierarchy_mix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (event (:constructor event-create))
    type pos charcount timestamp)
  (defclass event-handler ()
    ((buf-name :initarg :buf :accessor eh-buf :initform "")
     (events :initarg :events :accessor eh-events :initform nil)
     (total-chars :initarg :tc :accessor eh-tc :initform 0)
     (log :initarg :log :accessor eh-log :initform nil)))
  (defclass logging-handler (event-handler)
    ((verbose :initarg :verbose :accessor lh-verbose :initform t)))
  (defclass silent-handler (event-handler)
    ((threshold :initarg :threshold :accessor sh-threshold :initform 10)))
  (defmethod eh-handle ((handler event-handler) ev)
    (push ev (eh-events handler))
    (setf (eh-tc handler) (+ (eh-tc handler) (event-charcount ev))))
  (defmethod eh-handle ((handler logging-handler) ev)
    (cl-call-next-method)
    (when (lh-verbose handler)
      (push (format "log:%s@%d:+%d" (event-type ev) (event-pos ev) (event-charcount ev))
            (eh-log handler))))
  (defmethod eh-handle ((handler silent-handler) ev)
    (when (> (event-charcount ev) (sh-threshold handler))
      (cl-call-next-method)
      (push (format "silent-big:%s@%d" (event-type ev) (event-pos ev)) (eh-log handler))))
  (let* ((buf (generate-new-buffer "ds3"))
         (lh (logging-handler :buf (buffer-name buf) :events nil :tc 0 :log nil :verbose t))
         (sh (silent-handler :buf (buffer-name buf) :events nil :tc 0 :log nil :threshold 2))
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
      (setq-local my-eh-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (let ((e1 (event-create :type "insert" :pos 8 :charcount 3 :timestamp 1)))
          (goto-char 8) (insert "XXX")
          (eh-handle lh e1) (eh-handle sh e1)
          (push (list "e1" (eh-tc lh) (eh-tc sh) (eh-log lh) (eh-log sh)
                      (marker-position m)) results))
        (let ((e2 (event-create :type "insert" :pos 15 :charcount 1 :timestamp 2)))
          (goto-char 15) (insert "Y")
          (eh-handle lh e2) (eh-handle sh e2)
          (push (list "e2" (eh-tc lh) (eh-tc sh) (eh-log lh) (eh-log sh)
                      (marker-position m)) results))
        (let ((e3 (event-create :type "insert" :pos 25 :charcount 5 :timestamp 3)))
          (goto-char 25) (insert "ZZZZZ")
          (eh-handle lh e3) (eh-handle sh e3)
          (push (list "e3" (eh-tc lh) (eh-tc sh) (eh-log lh) (eh-log sh)
                      (marker-position m)) results))
        (setq my-eh-log (cons "3-events" my-eh-log))
        (save-restriction
          (narrow-to-region 5 45)
          (let ((e4 (event-create :type "insert" :pos 10 :charcount 2 :timestamp 4)))
            (goto-char 10) (insert "WW")
            (eh-handle lh e4) (eh-handle sh e4)
            (push (list "e4-narrow" (eh-tc lh) (eh-tc sh) (eh-log sh)
                        (marker-position m)) results)))
        (push (list "final"
                    (length (eh-events lh)) (length (eh-events sh))
                    (eh-tc lh) (eh-tc sh)
                    (marker-position m)
                    (overlay-start ov) (overlay-end ov)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S my-eh-log=%S"
                       results (reverse my-eh-log)))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (eh-log lh) (eh-log sh)
              (eh-tc lh) (eh-tc sh)
              (cl-typep lh 'logging-handler)
              (cl-typep sh 'silent-handler)
              (cl-typep lh 'event-handler)
              (cl-typep sh 'event-handler)
              my-eh-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defstruct_copy_merge_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (text-region (:constructor text-region-create))
    label start end face marker)
  (defclass region-manager ()
    ((buf-name :initarg :buf :accessor rm-buf :initform "")
     (regions :initarg :regions :accessor rm-regions :initform nil)
     (log :initarg :log :accessor rm-log :initform nil)))
  (defmethod rm-add-region ((mgr region-manager) label start end face)
    (with-current-buffer (rm-buf mgr)
      (let* ((m (set-marker (make-marker) start))
             (ov (make-overlay start end))
             (tr (text-region-create :label label :start start :end end
                                     :face face :marker m)))
        (overlay-put ov 'face face)
        (overlay-put ov 'priority 5)
        (put-text-property start end 'region-label label)
        (push tr (rm-regions mgr))
        (push (format "add:%s@%d-%d" label start end) (rm-log mgr))
        tr)))
  (defmethod rm-merge-regions ((mgr region-manager) r1 r2 new-label)
    (let* ((new-start (min (text-region-start r1) (text-region-start r2)))
           (new-end (max (text-region-end r1) (text-region-end r2))))
      (rm-add-region mgr new-label new-start new-end 'highlight)))
  (let* ((buf (generate-new-buffer "ds4"))
         (mgr (region-manager :buf (buffer-name buf) :regions nil :log nil))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH-IIII-JJJJ")
      (setq-local my-rm-log nil)
      (setq buffer-undo-list nil)
      (undo-boundary)
      (let* ((r1 (rm-add-region mgr "r1" 1 10 'bold))
             (r2 (rm-add-region mgr "r2" 11 20 'italic))
             (r3 (rm-add-region mgr "r3" 21 30 'underline))
             (r4 (rm-add-region mgr "r4" 31 40 'shadow))
             (m (set-marker (make-marker) 15)))
        (push (list "init"
                    (mapcar (lambda (r)
                             (list (text-region-label r)
                                   (text-region-start r)
                                   (text-region-end r)
                                   (marker-position (text-region-marker r))))
                            (rm-regions mgr))
                    (marker-position m)) results)
        (let ((r5 (copy-text-region r1)))
          (setf (text-region-label r5) "r1-copy")
          (setf (text-region-start r5) 6)
          (setf (text-region-end r5) 15)
          (push r5 (rm-regions mgr)))
        (push (list "after-copy"
                    (length (rm-regions mgr))
                    (marker-position m)) results)
        (rm-merge-regions mgr r2 r3 "merged-r2-r3")
        (push (list "after-merge"
                    (length (rm-regions mgr))
                    (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-rm-log (cons "ins@8" my-rm-log))
        (push (list "after-edit"
                    (mapcar (lambda (r)
                             (list (text-region-label r)
                                   (marker-position (text-region-marker r))))
                            (rm-regions mgr))
                    (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 45)
          (push (list "narrow" (point-min) (point-max)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S rm-log=%S"
                       results (reverse (rm-log mgr))))
        (set-marker m 3)
        (list (buffer-substring-no-properties 1 (point-max))
              (length (rm-regions mgr))
              (rm-log mgr)
              (marker-position m)
              my-rm-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defstruct_method_dispatch_across_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (cmd (:constructor cmd-create))
    type pos text undoable)
  (defclass cmd-executor ()
    ((buf-name :initarg :buf :accessor ce-buf :initform "")
     (history :initarg :history :accessor ce-history :initform nil)
     (executed :initarg :executed :accessor ce-executed :initform 0)
     (skipped :initarg :skipped :accessor ce-skipped :initform 0)
     (log :initarg :log :accessor ce-log :initform nil)))
  (defclass strict-executor (cmd-executor) ())
  (defclass lenient-executor (cmd-executor) ())
  (defmethod ce-exec ((exec cmd-executor) c)
    (when (cmd-undoable c)
      (with-current-buffer (ce-buf exec)
        (goto-char (cmd-pos c))
        (insert (cmd-text c))
        (setf (ce-executed exec) (1+ (ce-executed exec)))
        (push c (ce-history exec))
        (push (format "exec:%s@%d" (cmd-type c) (cmd-pos c)) (ce-log exec)))))
  (defmethod ce-exec ((exec strict-executor) c)
    (if (cmd-undoable c)
        (cl-call-next-method)
      (setf (ce-skipped exec) (1+ (ce-skipped exec)))
      (push (format "skip:%s@%d" (cmd-type c) (cmd-pos c)) (ce-log exec))))
  (defmethod ce-exec ((exec lenient-executor) c)
    (cl-call-next-method)
    (when (not (cmd-undoable c))
      (setf (ce-skipped exec) (1+ (ce-skipped exec)))
      (push (format "lenient-skip:%s@%d" (cmd-type c) (cmd-pos c)) (ce-log exec))))
  (let* ((buf (generate-new-buffer "ds5"))
         (strict (strict-executor :buf (buffer-name buf) :history nil :executed 0 :skipped 0 :log nil))
         (lenient (lenient-executor :buf (buffer-name buf) :history nil :executed 0 :skipped 0 :log nil))
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
      (setq-local my-ce-log nil)
      (let* ((ov (make-overlay 6 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (cmds (list (cmd-create :type "ins" :pos 8 :text "XXX" :undoable t)
                         (cmd-create :type "ins" :pos 15 :text "YYY" :undoable t)
                         (cmd-create :type "ins" :pos 25 :text "ZZZ" :undoable nil)
                         (cmd-create :type "ins" :pos 35 :text "WWW" :undoable t))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (dolist (c cmds)
          (ce-exec strict c))
        (push (list "strict-pass"
                    (ce-executed strict) (ce-skipped strict)
                    (ce-log strict)
                    (marker-position m)) results)
        (dolist (c cmds)
          (ce-exec lenient c))
        (push (list "lenient-pass"
                    (ce-executed lenient) (ce-skipped lenient)
                    (ce-log lenient)
                    (marker-position m)) results)
        (setq my-ce-log (cons "2-passes" my-ce-log))
        (save-restriction
          (narrow-to-region 5 45)
          (let ((c5 (cmd-create :type "ins" :pos 10 :text "NNN" :undoable nil)))
            (ce-exec strict c5)
            (ce-exec lenient c5)
            (push (list "narrow"
                        (ce-executed strict) (ce-skipped strict)
                        (ce-executed lenient) (ce-skipped lenient)
                        (marker-position m)) results)))
        (push (list "final"
                    (buffer-substring-no-properties 1 (point-max))
                    (ce-executed strict) (ce-skipped strict)
                    (ce-executed lenient) (ce-skipped lenient)
                    (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (cl-typep strict 'strict-executor)
                    (cl-typep lenient 'lenient-executor)) results)
        (setq results (reverse results))
        (list results
              (ce-log strict) (ce-log lenient)
              my-ce-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
