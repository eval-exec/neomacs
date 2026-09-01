//! Complex combo batch 59 — continued fresh edges + MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx59_org_agenda_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'org-agenda)
      (list (fboundp 'org-agenda)
            (boundp 'org-agenda-files)))
  (error (list :not-available)))
"##,
        expect,
    );
}

#[test]
fn div_cx59_org_capture_template() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'org-capture)
      (list (fboundp 'org-capture)
            (boundp 'org-capture-templates)))
  (error (list :not-available)))
"##,
        expect,
    );
}

#[test]
fn div_cx59_org_sparse_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"* A\\n** B\\n** C\\n* D\\n\" #<killed buffer>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* A\n** B\n** C\n* D\n")
        (org-occur "B")
        (list (buffer-string) (current-buffer))))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx59_org_drawer_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"0:30\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'org)
      (with-temp-buffer
        (org-mode)
        (insert "* Task\n:PROPERTIES:\n:Effort: 0:30\n:Tags: foo,bar\n:END:\n")
        (org-back-to-heading t)
        (list (org-entry-get (point) "Effort")
              (org-entry-get (point) "Tags"))))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx59_format_spec_make_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((spec (format-spec-make ?a "alpha" ?b "beta" ?c "gamma" ?d "delta")))
      (list (format-spec "%a-%b-%c-%d" spec)
            (format-spec "%d-%c-%b-%a" spec)
            (length (format-spec "%a-%b-%c-%d" spec))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx59_read_from_string_error_at_various_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (end-of-file end-of-file end-of-file end-of-file end-of-file end-of-file)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((inputs '("(" "(a" "(a b" "#(" "#1=" "#s(")))
  (mapcar (lambda (input)
            (condition-case e (read-from-string input) (error (car e))))
          inputs))
"##,
        expect,
    );
}

#[test]
fn div_cx59_cl_setf_on_aref_vector_chain_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-rotatef)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v (vector 1 2 3 4 5 6 7 8 9 10)))
  (cl-rotatef (aref v 0) (aref v 9))
  (cl-rotatef (aref v 1) (aref v 8))
  (cl-shiftf (aref v 2) (aref v 7) 0)
  (setf (aref v 3) (aref v 6))
  (append v nil))
"##,
        expect,
    );
}

#[test]
fn div_cx59_buffer_hash_after_various_edit_operations_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"ff3390557335ba88d37755e41514beb03bc499ec\" \"ff3390557335ba88d37755e41514beb03bc499ec\" \"bad86e06a60b48b33324bd9643acc4b46b19bf80\" \"ff3390557335ba88d37755e41514beb03bc499ec\" \"ff3390557335ba88d37755e41514beb03bc499ec\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer (insert "same") (buffer-hash))
      (with-temp-buffer (insert "same") (buffer-hash))
      (with-temp-buffer (insert "same ") (buffer-hash))
      (with-temp-buffer (insert "same") (put-text-property 1 2 'face 'bold) (buffer-hash))
      (with-temp-buffer (insert "same") (let ((m (set-marker (make-marker) 2))) (buffer-hash))))
"##,
        expect,
    );
}

#[test]
fn div_cx59_coding_system_plist_all_codings_batch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (coding-category-utf-8 coding-category-utf-8-auto coding-category-utf-8-sig coding-category-charset coding-category-charset coding-category-emacs-mule coding-category-utf-16-auto coding-category-utf-16-be-nosig coding-category-utf-16-le-nosig coding-category-big5 coding-category-sjis coding-category-iso-8-2 coding-category-raw-text coding-category-raw-text coding-category-undecided coding-category-raw-text)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs) (coding-system-category cs))
        '(utf-8 utf-8-auto utf-8-with-signature latin-1 iso-8859-7
          emacs-mule utf-16 utf-16be utf-16le big5 shift_jis
          euc-jp no-conversion raw-text undecided binary))
"##,
        expect,
    );
}

#[test]
fn div_cx59_weak_hash_all_weakness_types_eviction_marker_key_cons_value_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((key . 0) (value . 0) (key-and-value . 0) (key-or-value . 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((results nil))
  (dolist (w '(key value key-and-value key-or-value))
    (let ((ht (make-hash-table :weakness w :test 'eq)))
      (dotimes (i 3) (puthash (cons i nil) (cons (* i 10) nil) ht))
      (garbage-collect)
      (push (cons w (hash-table-count ht)) results)))
  (nreverse results))
"##,
        expect,
    );
}

#[test]
fn div_cx59_subword_superword_toggle_word_ops_undo_marker_overlay_narrow_env_exitcode_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((env-val
       (let ((process-environment (cons "NEO_CX59=v" process-environment)))
         (string-trim (shell-command-to-string "echo $NEO_CX59"))))
      (exit-code
       (let ((p (make-process :name "neo-cx59-ec" :command '("sh" "-c" "exit 6")))
         (accept-process-output p 2)
         (process-exit-status p))))
  (condition-case e
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "pre myCamelCaseVar snake_case_var rest end")
        (put-text-property 1 3 'face 'bold)
        (let ((ov (make-overlay 5 35)) (m (set-marker (make-marker) 15)))
          (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
          (narrow-to-region 3 43)
          (goto-char 5) (forward-word 1)
          (let ((default-pos (point)))
            (subword-mode 1) (goto-char 5) (forward-word 1)
            (let ((sub-pos (point)))
              (goto-char 5) (kill-word 1)
              (let ((state (list (buffer-string) (marker-position m)
                                 (overlayp ov) (overlay-start ov))))
                (subword-mode -1) (undo)
                (list env-val exit-code default-pos sub-pos state
                      (buffer-string) (marker-position m)
                      (overlayp ov) (overlay-start ov)
                      (text-properties-at 1))))))
    (error (list env-val exit-code :errored))))
"##,
        expect,
    );
}

#[test]
fn div_cx59_process_output_narrow_decode_encode_hash_overlay_textprop_undo_env_exitcode_timer_weak_hash_display_evaporate_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX59=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX59"))))
        (exit-code
         (let ((p (make-process :name "neo-cx59-ec" :command '("sh" "-c" "exit 8")))
           (accept-process-output p 2)
           (process-exit-status p))))
    (sit-for 0.01)
    (let ((weak-ht (make-hash-table :weakness 'key :test 'eq)))
      (puthash (cons 1 nil) :v weak-ht)
      (garbage-collect)
      (let ((buf (get-buffer-create " *neo-cx59-po*")))
        (with-current-buffer buf
          (buffer-enable-undo)
          (insert "HEADER\n")
          (put-text-property 1 7 'face 'bold)
          (put-text-property 3 5 'display "XX")
          (let ((ov (make-overlay 4 6))) (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t))
          (narrow-to-region 1 7))
        (let ((p (make-process :name "neo-cx59-po" :command '("printf" "%s" "café世界")
                               :buffer buf)))
          (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
          (accept-process-output p 1))
        (prog1 (with-current-buffer buf
                 (widen)
                 (let ((content (buffer-string)))
                   (undo)
                   (list env-val exit-code timer-fired content (buffer-string)
                         (text-properties-at 0) (text-properties-at 7)
                         (length (overlays-in 1 20))
                         (hash-table-count weak-ht))))
          (kill-buffer buf))))))
"##,
        expect,
    );
}

#[test]
fn div_cx59_json_deeply_nested_with_org_xml_dom_struct_hash_secure_print_circle_env_exitcode_weak_hash_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx59-item id text)
  (let ((env-val
         (let ((process-environment (cons "NEO_CX59=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX59"))))
        (exit-code
         (let ((p (make-process :name "neo-cx59-ec" :command '("sh" "-c" "exit 3")))
           (accept-process-output p 2)
           (process-exit-status p)))
        (weak-ht (make-hash-table :weakness 'key :test 'eq)))
    (puthash (cons 1 nil) :v weak-ht)
    (garbage-collect)
    (condition-case e
        (progn (require 'json) (require 'dom) (require 'xml)
          (let* ((recs (list (make-neo-cx59-item :id 1 :text "café")
                              (make-neo-cx59-item :id 2 :text "世界")
                              (make-neo-cx59-item :id 3 :text "😀")))
                 (json-enc (json-encode (mapcar (lambda (r) `((id . ,(neo-cx59-item-id r)) (text . ,(neo-cx59-item-text r)))) recs)))
                 (json-dec (json-read-from-string json-enc)))
            (let ((print-circle t))
              (list env-val exit-code
                    (mapcar #'neo-cx59-item-text recs)
                    (mapcar (lambda (bd) (cdr (assoc 'text bd))) json-dec)
                    (secure-hash 'sha256 json-enc)
                    (hash-table-count weak-ht)
                    (length json-enc)))))
      (error (list env-val exit-code :errored)))))
"##,
        expect,
    );
}

#[test]
fn div_cx59_set_buffer_multibyte_undo_narrow_widen_marker_overlay_textprop_display_evaporate_env_exitcode_coding_timer_weak_hash_regex_replace_full_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX59=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX59"))))
        (exit-code
         (let ((p (make-process :name "neo-cx59-ec" :command '("sh" "-c" "exit 5")))
           (accept-process-output p 2)
           (process-exit-status p))))
    (sit-for 0.01)
    (let ((weak-ht (make-hash-table :weakness 'key :test 'eq)))
      (puthash (cons 1 nil) :v weak-ht)
      (garbage-collect)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "café世界0123456789ABCDEF0123456789")
        (put-text-property 1 3 'face 'bold)
        (put-text-property 4 6 'display "XX")
        (let ((ov (make-overlay 7 14)) (m (set-marker (make-marker) 10)))
          (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
          (narrow-to-region 3 30)
          (undo-boundary)
          (set-buffer-multibyte nil)
          (let ((nil-1 (list (length (buffer-string)) (marker-position m)
                            (overlayp ov) (text-properties-at 1))))
            (set-buffer-multibyte t)
            (undo-boundary)
            (let ((case-fold-search t))
              (goto-char 1)
              (while (re-search-forward "[a-z0-9]+" nil t)
                (replace-match (upcase (match-string 0))))
              (let ((state (list (buffer-string) (marker-position m)
                                 (overlayp ov) (overlay-start ov)
                                 (text-properties-at 1) (current-column))))
                (undo) (undo) (undo)
                (widen)
                (list env-val exit-code timer-fired nil-1 state
                      (length (buffer-string)) (marker-position m)
                      (overlayp ov) (overlay-start ov)
                      (text-properties-at 1) (buffer-string)
                      (hash-table-count weak-ht))))))))))
"##,
        expect,
    );
}
