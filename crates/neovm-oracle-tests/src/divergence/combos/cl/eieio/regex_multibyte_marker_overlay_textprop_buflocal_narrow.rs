//! Combo: cl-eieio regex + multibyte + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests regex search/replace with object-mediated state and multibyte strings in object slots.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_regex_replace_multibyte_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer rx1> 0 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass unicode-node ()
    ((label :initarg :label :accessor un-label :initform "")
     (codepoint :initarg :codepoint :accessor un-cp :initform 0)
     (found :initarg :found :accessor un-found :initform 0)))
  (let* ((buf (generate-new-buffer "rx1"))
         (n1 (unicode-node :label "CJK" :codepoint #x4e2d :found 0))
         (n2 (unicode-node :label "hiragana" :codepoint #x3042 :found 0))
         (n3 (unicode-node :label "emoji" :codepoint #x1f600 :found 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'category 'alpha)
      (put-text-property 6 10 'category 'beta)
      (put-text-property 11 15 'category 'gamma)
      (put-text-property 16 20 'category 'delta)
      (put-text-property 21 25 'category 'epsilon)
      (put-text-property 26 30 'category 'zeta)
      (setq-local nodes (list n1 n2 n3))
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (matches nil))
        (goto-char (point-min))
        (while (re-search-forward "[A-C]+" nil t)
          (setf (un-found n1) (1+ (un-found n1)))
          (push (list 'upper (match-beginning 0) (match-end 0) (match-string 0)) matches))
        (goto-char (point-min))
        (while (re-search-forward "[D-F]+" nil t)
          (setf (un-found n2) (1+ (un-found n2)))
          (push (list 'lower (match-beginning 0) (match-end 0) (match-string 0)) matches))
        (goto-char (point-max))
        (insert (format " | matches=%s n1=%d n2=%d n3=%d m=%d ov=[%d,%d]"
                       (reverse matches) (un-found n1) (un-found n2) (un-found n3)
                       (marker-position m) (overlay-start ov) (overlay-end ov)))
        (setf (marker-position m) 5)
        (put-text-property (1- (point-max)) (point-max) 'match-log t)
        (list (marker-position m) (overlay-start ov) (overlay-end ov) (buffer-string))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_regex_groups_object_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 76 85)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass match-result ()
    ((pattern :initarg :pattern :accessor mr-pattern :initform "")
     (groups :initarg :groups :accessor mr-groups :initform nil)
     (count :initarg :count :accessor mr-count :initform 0)))
  (let* ((buf (generate-new-buffer "rx2"))
         (r1 (match-result :pattern "key=\\([a-z]+\\)" :count 0))
         (r2 (match-result :pattern "num=\\([0-9]+\\)" :count 0))
         (r3 (match-result :pattern "val=\\([^ ]+\\)" :count 0)))
    (with-current-buffer buf
      (insert "key=alpha num=42 val=hello key=beta num=99 val=world key=gamma num=7 val=test")
      (put-text-property 1 10 'field 'k1)
      (put-text-property 11 18 'field 'n1)
      (put-text-property 19 28 'field 'v1)
      (put-text-property 29 38 'field 'k2)
      (put-text-property 39 46 'field 'n2)
      (put-text-property 47 57 'field 'v2)
      (put-text-property 58 68 'field 'k3)
      (put-text-property 69 75 'field 'n3)
      (put-text-property 76 85 'field 'v3)
      (setq-local results (list r1 r2 r3))
      (let* ((ov (make-overlay 1 28))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 5))
             (all-matches nil))
        (undo-boundary)
        (goto-char (point-min))
        (while (re-search-forward "key=\\([a-z]+\\)" nil t)
          (setf (mr-count r1) (1+ (mr-count r1)))
          (push (match-string 1) (mr-groups r1))
          (push (list 'key (match-string 1) (match-beginning 0) (match-end 0)) all-matches))
        (goto-char (point-min))
        (while (re-search-forward "num=\\([0-9]+\\)" nil t)
          (setf (mr-count r2) (1+ (mr-count r2)))
          (push (match-string 1) (mr-groups r2))
          (push (list 'num (match-string 1) (string-to-number (match-string 1))) all-matches))
        (goto-char (point-min))
        (while (re-search-forward "val=\\([^ ]+\\)" nil t)
          (setf (mr-count r3) (1+ (mr-count r3)))
          (push (match-string 1) (mr-groups r3))
          (push (list 'val (match-string 1)) all-matches))
        (setq all-matches (reverse all-matches))
        (goto-char (point-max))
        (insert (format " | keys=%s nums=%s vals=%s all=%s"
                       (reverse (mr-groups r1))
                       (reverse (mr-groups r2))
                       (reverse (mr-groups r3))
                       all-matches))
        (setf (marker-position m) 10)
        (put-text-property (1- (point-max)) (point-max) 'regex-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                results))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_regex_replace_with_callback_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass replacer ()
    ((from :initarg :from :accessor rp-from :initform "")
     (to :initarg :to :accessor rp-to :initform "")
     (count :initarg :count :accessor rp-count :initform 0)))
  (let* ((buf (generate-new-buffer "rx3"))
         (r1 (replacer :from "aaa" :to "AAA" :count 0))
         (r2 (replacer :from "bbb" :to "BBB" :count 0))
         (r3 (replacer :from "ccc" :to "CCC" :count 0)))
    (with-current-buffer buf
      (insert "aaa-bbb-ccc-aaa-bbb-ccc-aaa-bbb-ccc")
      (setq-local replacers (list r1 r2 r3))
      (let* ((ov (make-overlay 5 24))
             (_ (overlay-put ov 'priority 3))
             (m (make-marker))
             (_ (set-marker m 5))
             (replace-log nil))
        (dolist (rp (list r1 r2 r3))
          (goto-char (point-min))
          (while (re-search-forward (regexp-quote (rp-from rp)) nil t)
            (setf (rp-count rp) (1+ (rp-count rp)))
            (push (list (rp-from rp) (match-beginning 0) (match-end 0)) replace-log)))
        (setq replace-log (reverse replace-log))
        (dolist (rp (list r1 r2 r3))
          (goto-char (point-min))
          (while (search-forward (rp-from rp) nil t)
            (replace-match (rp-to rp) t t)))
        (goto-char (point-max))
        (insert (format " | log=%s r1=%d r2=%d r3=%d m=%d ov=[%d,%d] buf=%s"
                       replace-log (rp-count r1) (rp-count r2) (rp-count r3)
                       (marker-position m) (overlay-start ov) (overlay-end ov)
                       (buffer-string)))
        (set-marker m 8)
        (list (marker-position m) (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_regex_narrow_search_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass search-zone ()
    ((name :initarg :name :accessor sz-name :initform "")
     (start :initarg :start :accessor sz-start :initform 1)
     (end :initarg :end :accessor sz-end :initform 1)
     (hits :initarg :hits :accessor sz-hits :initform 0)))
  (let* ((buf (generate-new-buffer "rx4"))
         (z1 (search-zone :name "alpha" :start 1 :end 10))
         (z2 (search-zone :name "beta" :start 11 :end 20))
         (z3 (search-zone :name "gamma" :start 21 :end 30)))
    (with-current-buffer buf
      (insert "abcXdefYgh-ijkXlmnYop-qrsXtuvYwz")
      (put-text-property 1 10 'zone z1)
      (put-text-property 11 20 'zone z2)
      (put-text-property 21 30 'zone z3)
      (setq-local zones (list z1 z2 z3))
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (ov3 (make-overlay 21 30))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (_ (overlay-put ov3 'priority 3))
             (m (make-marker))
             (_ (set-marker m 5))
             (zone-results nil))
        (undo-boundary)
        (dolist (z (list z1 z2 z3))
          (save-excursion
            (save-restriction
              (narrow-to-region (sz-start z) (sz-end z))
              (goto-char (point-min))
              (let ((X-count 0) (Y-count 0))
                (while (re-search-forward "X" nil t) (setq X-count (1+ X-count)))
                (goto-char (point-min))
                (while (re-search-forward "Y" nil t) (setq Y-count (1+ Y-count)))
                (setf (sz-hits z) (+ X-count Y-count))
                (push (list (sz-name z) X-count Y-count) zone-results)))))
        (setq zone-results (reverse zone-results))
        (goto-char (point-max))
        (insert (format " | zones=%s" zone-results))
        (setf (marker-position m) 3)
        (put-text-property (1- (point-max)) (point-max) 'zone-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os1 (overlay-start ov1))
              (oe3 (overlay-end ov3))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os1 oe3 bs
                (marker-position m)
                (buffer-string)
                zones))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_regex_multibyte_replacement_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass string-op ()
    ((op :initarg :op :accessor so-op :initform "")
     (arg :initarg :arg :accessor so-arg :initform "")
     (result :initarg :result :accessor so-result :initform "")))
  (let* ((buf (generate-new-buffer "rx5"))
         (o1 (string-op :op "upcase" :arg "hello" :result "HELLO"))
         (o2 (string-op :op "downcase" :arg "WORLD" :result "world"))
         (o3 (string-op :op "capitalize" :arg "test" :result "Test")))
    (with-current-buffer buf
      (insert "hello \xe4\xb8\xad WORLD \xe3\x81\x82 test \xf0\x9f\x98\x80 end")
      (put-text-property 1 6 'case 'lower)
      (put-text-property 7 8 'case 'none)
      (put-text-property 9 15 'case 'upper)
      (put-text-property 16 17 'case 'none)
      (put-text-property 18 23 'case 'lower)
      (put-text-property 24 25 'case 'none)
      (put-text-property 26 30 'case 'none)
      (setq-local ops (list o1 o2 o3))
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 1))
             (positions nil))
        (undo-boundary)
        (goto-char (point-min))
        (when (re-search-forward "hello" nil t)
          (push (list 'found-hello (match-beginning 0) (match-end 0)) positions)
          (replace-match (so-result o1) t t))
        (goto-char (point-min))
        (when (re-search-forward "WORLD" nil t)
          (push (list 'found-world (match-beginning 0) (match-end 0)) positions)
          (replace-match (so-result o2) t t))
        (goto-char (point-min))
        (when (re-search-forward "test" nil t)
          (push (list 'found-test (match-beginning 0) (match-end 0)) positions)
          (replace-match (so-result o3) t t))
        (setq positions (reverse positions))
        (goto-char (point-max))
        (insert (format " | pos=%s buf=%s" positions (buffer-string)))
        (setf (marker-position m) 5)
        (put-text-property (1- (point-max)) (point-max) 'rx-log t)
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
