//! Combo: cl-eieio text-property face vs overlay face interplay
//! + markers + buflocal + narrow + undo.
//! Tests the complex interaction between text property faces and overlay faces,
//! including priority resolution, face merging, and undo behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_tp_face_vs_overlay_face_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tp-ov-face-snap ()
    ((step :initarg :step :accessor tof-step :initform "")
     (tp-face-at-8 :initarg :tpf :accessor tof-tpf :initform nil)
     (ov-face-at-8 :initarg :ovf :accessor tof-ovf :initform nil)
     (char-face-at-8 :initarg :cf :accessor tof-cf :initform nil)
     (m-pos :initarg :m-pos :accessor tof-mp :initform 0)))
  (let* ((buf (generate-new-buffer "tf1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAAAAAABBBBBBBBBBCCCCCCCCCCDDDDDDDDDD")
      (setq-local my-face-log nil)
      (let* ((ov (make-overlay 5 25))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 10))
             (m (set-marker (make-marker) 8))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (tp-ov-face-snap :step "init"
                              :tpf (get-text-property 8 'face)
                              :ovf (overlay-get ov 'face)
                              :cf (get-char-property 8 'face)
                              :m-pos (marker-position m)) snaps)
        (put-text-property 3 12 'face 'italic)
        (setq my-face-log (cons "tp-italic@3-12" my-face-log))
        (push (tp-ov-face-snap :step "tp-italic"
                              :tpf (get-text-property 8 'face)
                              :ovf (overlay-get ov 'face)
                              :cf (get-char-property 8 'face)
                              :m-pos (marker-position m)) snaps)
        (put-text-property 7 15 'face 'underline)
        (setq my-face-log (cons "tp-underline@7-15" my-face-log))
        (push (tp-ov-face-snap :step "tp-overwrite"
                              :tpf (get-text-property 8 'face)
                              :ovf (overlay-get ov 'face)
                              :cf (get-char-property 8 'face)
                              :m-pos (marker-position m)) snaps)
        (overlay-put ov 'face '(bold italic))
        (setq my-face-log (cons "ov-bold-italic" my-face-log))
        (push (tp-ov-face-snap :step "ov-list-face"
                              :tpf (get-text-property 8 'face)
                              :ovf (overlay-get ov 'face)
                              :cf (get-char-property 8 'face)
                              :m-pos (marker-position m)) snaps)
        (overlay-put ov 'priority 0)
        (push (tp-ov-face-snap :step "ov-pri-0"
                              :tpf (get-text-property 8 'face)
                              :ovf (overlay-get ov 'face)
                              :cf (get-char-property 8 'face)
                              :m-pos (marker-position m)) snaps)
        (goto-char 6)
        (insert "XXX")
        (setq my-face-log (cons "insert@6" my-face-log))
        (push (tp-ov-face-snap :step "edit"
                              :tpf (get-text-property 8 'face)
                              :ovf (overlay-get ov 'face)
                              :cf (get-char-property 8 'face)
                              :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (tof-step s) (tof-tpf s)
                                                (tof-ovf s) (tof-cf s)
                                                (tof-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S log=%S"
                       results (reverse my-face-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'tof-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-face-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_tp_face_overlay_multi_region_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tp-ov-undo-snap ()
    ((step :initarg :step :accessor tou-step :initform "")
     (faces :initarg :faces :accessor tou-faces :initform nil)
     (m-pos :initarg :m-pos :accessor tou-mp :initform 0)
     (buf-len :initarg :bl :accessor tou-bl :initform 0)))
  (let* ((buf (generate-new-buffer "tf2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AA-BB-CC-DD-EE-FF-GG-HH-II-JJ")
      (setq-local my-face-hist nil)
      (let* ((ov1 (make-overlay 1 6))
             (ov2 (make-overlay 7 12))
             (ov3 (make-overlay 13 18))
             (_ (overlay-put ov1 'face 'bold))
             (_ (overlay-put ov1 'priority 5))
             (_ (overlay-put ov2 'face 'italic))
             (_ (overlay-put ov2 'priority 10))
             (_ (overlay-put ov3 'face 'underline))
             (_ (overlay-put ov3 'priority 15))
             (m (set-marker (make-marker) 9))
             (results nil)
             (snap-faces
              (lambda ()
                (list (get-text-property 3 'face)
                      (get-text-property 9 'face)
                      (get-text-property 15 'face)
                      (get-text-property 21 'face)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (tp-ov-undo-snap :step "init"
                              :faces (funcall snap-faces)
                              :m-pos (marker-position m)
                              :bl (point-max)) snaps)
        (put-text-property 1 30 'face 'default)
        (setq my-face-hist (cons "tp-default-all" my-face-hist))
        (push (tp-ov-undo-snap :step "tp-default"
                              :faces (funcall snap-faces)
                              :m-pos (marker-position m)
                              :bl (point-max)) snaps)
        (undo-boundary)
        (put-text-property 5 20 'face 'highlight)
        (setq my-face-hist (cons "tp-highlight-mid" my-face-hist))
        (push (tp-ov-undo-snap :step "tp-highlight"
                              :faces (funcall snap-faces)
                              :m-pos (marker-position m)
                              :bl (point-max)) snaps)
        (undo-boundary)
        (goto-char 9)
        (insert "ZZZZ")
        (setq my-face-hist (cons "insert@9" my-face-hist))
        (push (tp-ov-undo-snap :step "edit"
                              :faces (funcall snap-faces)
                              :m-pos (marker-position m)
                              :bl (point-max)) snaps)
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (push (tp-ov-undo-snap :step "undo-edit"
                              :faces (funcall snap-faces)
                              :m-pos (marker-position m)
                              :bl (point-max)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (tp-ov-undo-snap :step "undo-highlight"
                              :faces (funcall snap-faces)
                              :m-pos (marker-position m)
                              :bl (point-max)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (tp-ov-undo-snap :step "undo-default"
                              :faces (funcall snap-faces)
                              :m-pos (marker-position m)
                              :bl (point-max)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (tou-step s) (tou-faces s) (tou-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S hist=%S"
                       results (reverse my-face-hist)))
        (put-text-property (1- (point-max)) (point-max) 'tou-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (overlay-start ov2) (overlay-end ov2)
              (overlay-start ov3) (overlay-end ov3))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_tp_face_narrow_overlay_intersect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tp-narrow-snap ()
    ((step :initarg :step :accessor tns-step :initform "")
     (face-at-edge :initarg :face :accessor tns-face :initform nil)
     (narrow-min :initarg :nmin :accessor tns-nmin :initform 1)
     (narrow-max :initarg :nmax :accessor tns-nmax :initform 0)
     (m-pos :initarg :m-pos :accessor tns-mp :initform 0)))
  (let* ((buf (generate-new-buffer "tf3"))
         (snaps nil))
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
      (setq-local my-tp-log nil)
      (let* ((ov (make-overlay 8 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (tp-narrow-snap :step "init"
                             :face (get-char-property 10 'face)
                             :nmin (point-min) :nmax (point-max)
                             :m-pos (marker-position m)) snaps)
        (put-text-property 6 20 'face 'italic)
        (setq my-tp-log (cons "tp-italic@6-20" my-tp-log))
        (push (tp-narrow-snap :step "tp-face"
                             :face (get-char-property 10 'face)
                             :nmin (point-min) :nmax (point-max)
                             :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 10 25)
          (push (tp-narrow-snap :step "narrow"
                               :face (get-char-property 12 'face)
                               :nmin (point-min) :nmax (point-max)
                               :m-pos (marker-position m)) snaps)
          (put-text-property 12 20 'face 'underline)
          (setq my-tp-log (cons "tp-ul@narrow-12-20" my-tp-log))
          (push (tp-narrow-snap :step "tp-in-narrow"
                               :face (get-char-property 12 'face)
                               :nmin (point-min) :nmax (point-max)
                               :m-pos (marker-position m)) snaps)
          (goto-char 14)
          (insert "KK")
          (setq my-tp-log (cons "ins@narrow-14" my-tp-log))
          (push (tp-narrow-snap :step "edit-narrow"
                               :face (get-char-property 12 'face)
                               :nmin (point-min) :nmax (point-max)
                               :m-pos (marker-position m)) snaps))
        (push (tp-narrow-snap :step "widen"
                             :face (get-char-property 10 'face)
                             :nmin (point-min) :nmax (point-max)
                             :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (tns-step s) (tns-face s)
                                                (tns-nmin s) (tns-nmax s)
                                                (tns-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S tplog=%S"
                       results (reverse my-tp-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'tns-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_tp_face_sticky_nonsticky_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tp-sticky-snap ()
    ((step :initarg :step :accessor tss-step :initform "")
     (face-before :initarg :fb :accessor tss-fb :initform nil)
     (face-at :initarg :fa :accessor tss-fa :initform nil)
     (face-after :initarg :ft :accessor tss-ft :initform nil)
     (m-pos :initarg :m-pos :accessor tss-mp :initform 0)))
  (let* ((buf (generate-new-buffer "tf4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (setq-local my-sticky-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 10))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (put-text-property 1 5 'face 'italic)
        (put-text-property 6 10 'face 'underline)
        (put-text-property 11 15 'face 'default)
        (put-text-property 16 20 'face 'highlight)
        (put-text-property 5 6 'rear-nonsticky t)
        (put-text-property 10 11 'rear-nonsticky t)
        (setq my-sticky-log (cons "setup-sticky" my-sticky-log))
        (push (tp-sticky-snap :step "init"
                             :fb (get-text-property 5 'face)
                             :fa (get-text-property 10 'face)
                             :ft (get-text-property 15 'face)
                             :m-pos (marker-position m)) snaps)
        (goto-char 6)
        (insert "XX")
        (setq my-sticky-log (cons "ins@6" my-sticky-log))
        (push (tp-sticky-snap :step "ins@boundary"
                             :fb (get-text-property 5 'face)
                             :fa (get-text-property 10 'face)
                             :ft (get-text-property 15 'face)
                             :m-pos (marker-position m)) snaps)
        (goto-char 10)
        (insert "YY")
        (setq my-sticky-log (cons "ins@10" my-sticky-log))
        (push (tp-sticky-snap :step "ins@10"
                             :fb (get-text-property 5 'face)
                             :fa (get-text-property 10 'face)
                             :ft (get-text-property 15 'face)
                             :m-pos (marker-position m)) snaps)
        (goto-char 16)
        (insert "ZZ")
        (setq my-sticky-log (cons "ins@16" my-sticky-log))
        (push (tp-sticky-snap :step "ins@16"
                             :fb (get-text-property 5 'face)
                             :fa (get-text-property 10 'face)
                             :ft (get-text-property 15 'face)
                             :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (tss-step s) (tss-fb s) (tss-fa s)
                                                (tss-ft s) (tss-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S sticky=%S"
                       results (reverse my-sticky-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'tss-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (get-text-property 5 'rear-nonsticky)
              (get-text-property 10 'rear-nonsticky))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_tp_face_property_search_next_prev() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass tp-search-snap ()
    ((step :initarg :step :accessor tps-step :initform "")
     (next-bound :initarg :next :accessor tps-next :initform nil)
     (prev-bound :initarg :prev :accessor tps-prev :initform nil)
     (m-pos :initarg :m-pos :accessor tps-mp :initform 0)))
  (let* ((buf (generate-new-buffer "tf5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (setq-local my-search-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 15))
             (results nil)
             (do-search
              (lambda ()
                (let* ((n (text-property-search-forward 'face nil t))
                       (_ (goto-char 1))
                       (p (text-property-search-backward 'face nil t)))
                  (goto-char (point-max))
                  (list (if n (cons (prop-match-beginning n) (prop-match-end n)) nil)
                        (if p (cons (prop-match-beginning p) (prop-match-end p)) nil))))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (tp-search-snap :step "init"
                             :next (car (funcall do-search))
                             :prev (cadr (funcall do-search))
                             :m-pos (marker-position m)) snaps)
        (put-text-property 1 5 'face 'italic)
        (setq my-search-log (cons "tp-italic@1-5" my-search-log))
        (push (tp-search-snap :step "tp-begin"
                             :next (car (funcall do-search))
                             :prev (cadr (funcall do-search))
                             :m-pos (marker-position m)) snaps)
        (put-text-property 25 35 'face 'underline)
        (setq my-search-log (cons "tp-ul@25-35" my-search-log))
        (push (tp-search-snap :step "tp-end"
                             :next (car (funcall do-search))
                             :prev (cadr (funcall do-search))
                             :m-pos (marker-position m)) snaps)
        (put-text-property 11 15 'face 'default)
        (setq my-search-log (cons "tp-default@11-15" my-search-log))
        (push (tp-search-snap :step "tp-hole"
                             :next (car (funcall do-search))
                             :prev (cadr (funcall do-search))
                             :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 5 25)
          (push (tp-search-snap :step "narrow"
                               :next (car (funcall do-search))
                               :prev (cadr (funcall do-search))
                               :m-pos (marker-position m)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (tps-step s) (tps-next s) (tps-prev s)
                                                (tps-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S slog=%S"
                       results (reverse my-search-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'tps-log t)
        (list (buffer-string)
              (length snaps) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-search-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}
