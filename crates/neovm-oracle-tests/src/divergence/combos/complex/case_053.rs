//! Complex combo batch 53 — continued MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx53_subword_kill_word_chain_undo_marker_overlay_narrow_full_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (subword-mode 1)
      (insert "pre myCamelCaseVar rest anotherWord end")
      (put-text-property 1 3 'face 'bold)
      (let ((ov (make-overlay 5 18)) (m (set-marker (make-marker) 10)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 3 35)
        (undo-boundary)
        (goto-char 5) (kill-word 1)
        (undo-boundary)
        (goto-char 5) (upcase-word 1)
        (undo-boundary)
        (goto-char 5) (kill-word 1)
        (let ((state (list (buffer-string) (marker-position m)
                           (overlayp ov) (overlay-start ov)
                           (text-properties-at 1))))
          (undo) (undo) (undo)
          (list state (buffer-string) (marker-position m)
                (overlayp ov) (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx53_superword_forward_backward_kill_undo_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (superword-mode 1)
      (insert "pre snake_case_var rest camelCase end")
      (put-text-property 1 3 'face 'bold)
      (let ((ov (make-overlay 5 19)) (m (set-marker (make-marker) 12)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 3 38)
        (undo-boundary)
        (goto-char 5) (forward-word 1)
        (let ((fwd-pos (point)))
          (backward-word 1)
          (kill-word 1)
          (let ((state (list fwd-pos (buffer-string) (marker-position m)
                             (overlayp ov) (overlay-start ov))))
            (undo)
            (list state (buffer-string) (marker-position m)
                  (overlayp ov) (overlay-start ov)
                  (text-properties-at 1))))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx53_process_output_buffer_decode_encode_hash_narrow_overlay_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx53-po*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "HEADER\n")
    (put-text-property 1 7 'face 'bold)
    (let ((ov (make-overlay 3 5))) (overlay-put ov 'face 'italic))
    (narrow-to-region 1 7))
  (let ((p (make-process :name "neo-cx53-po" :command '("printf" "%s" "café世界")
                         :buffer buf)))
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (widen)
           (let ((content (buffer-string)))
             (encode-coding-region 1 (point-max) 'utf-8)
             (let ((enc-len (length (buffer-string))))
               (decode-coding-region 1 (point-max) 'utf-8)
               (let ((dec-len (length (buffer-string))))
                 (list content enc-len dec-len
                       (equal content (buffer-string))
                       (secure-hash 'sha256 (buffer-string))
                       (text-properties-at 0)
                       (text-properties-at 7)
                       (length (overlays-in 1 20))))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx53_cl_defmethod_eql_dispatch_with_advice_error_recovery_call_next_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"boom\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx53-cls () ((val :initarg :val)))
  (let (log)
    (cl-defgeneric neo-cx53-fn (obj))
    (cl-defmethod neo-cx53-fn ((obj (eql :keyword))) (push :kw log) :kw)
    (cl-defmethod neo-cx53-fn ((obj (eql :special))) (push :sp log) (error "boom"))
    (cl-defmethod neo-cx53-fn :around ((obj neo-cx53-cls))
      (push :around log) (condition-case e (cl-call-next-method) (error :recovered)))
    (cl-defmethod neo-cx53-fn ((obj neo-cx53-cls)) (push :primary log) (oref obj val))
    (advice-add 'neo-cx53-fn :before (lambda (&rest a) (push :adv log)))
    (list (neo-cx53-fn :keyword)
          (neo-cx53-fn :special)
          (neo-cx53-fn (neo-cx53-cls :val 99))
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_cx53_json_dom_xml_chain_deeply_nested_multibyte_hash_secure_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (2 (\"café\" \"世界\") (\"1\" \"2\") (\"café\" \"世界\") \"6f3305007d09ab873aace1a2920c0ff71b4459ddd7a47ab7b1aaf01a149d3dc3\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json) (require 'dom) (require 'xml)
      (let* ((xml (with-temp-buffer
                    (insert "<root attr=\"val\"><group><item id=\"1\">café</item><item id=\"2\">世界</item></group></root>")
                    (car (xml-parse-region (point-min) (point-max)))))
             (items (dom-by-tag xml 'item))
             (json-data (mapcar (lambda (item)
                                  `((id . ,(dom-attr item 'id)) (text . ,(dom-text item))))
                                items))
             (json-enc (json-encode `((items . ,json-data))))
             (json-dec (json-read-from-string json-enc))
             (back (cdr (assoc 'items json-dec))))
        (list (length items)
              (mapcar #'dom-text items)
              (mapcar (lambda (item) (dom-attr item 'id)) items)
              (mapcar (lambda (bd) (cdr (assoc 'text bd))) back)
              (secure-hash 'sha256 json-enc))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx53_window_config_register_buffer_split_merge_marker_overlay_narrow_dedicated_hscroll_vscroll_full_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx53-wc*")))
  (with-current-buffer buf
    (insert (make-string 80 ?x))
    (let ((m (set-marker (make-marker) 30)) (ov (make-overlay 10 40)))
      (overlay-put ov 'face 'bold)
      (put-text-property 1 5 'face 'italic)
      (put-text-property 50 55 'display "XX")))
  (set-window-buffer (selected-window) buf)
  (set-window-point (selected-window) 35)
  (set-window-hscroll (selected-window) 8)
  (set-window-vscroll (selected-window) 3)
  (set-window-dedicated-p (selected-window) t)
  (let ((cfg (current-window-configuration)))
    (set-register ?w cfg)
    (split-window nil nil 'right)
    (split-window nil nil 'below)
    (split-window (selected-window) nil 'below)
    (let ((split4 (count-windows)))
      (set-window-configuration cfg)
      (prog1 (list split4 (count-windows)
                   (window-point) (window-hscroll) (window-vscroll) (window-dedicated-p)
                   (with-current-buffer buf (marker-position (cdar buffer-markers)))
                   (with-current-buffer buf (length (overlays-at 20)))
                   (with-current-buffer buf (text-properties-at 1))
                   (with-current-buffer buf (text-properties-at 50)))
        (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
        (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx53_undo_redo_marker_overlay_textprop_display_evaporate_narrow_env_exitcode_coding_timer_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX53=v" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX53"))))
        (exit-code
         (let ((p (make-process :name "neo-cx53-ec" :command '("sh" "-c" "exit 4")))
           (accept-process-output p 2)
           (process-exit-status p))))
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "0123456789ABCDEF")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 5 8 'display "XX")
      (let ((ov (make-overlay 6 12)) (m (set-marker (make-marker) 9)))
        (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t)
        (narrow-to-region 3 14)
        (undo-boundary)
        (goto-char 7) (insert "YZ")
        (undo-boundary)
        (delete-region 4 10)
        (let ((state (list (buffer-string) (marker-position m)
                           (overlayp ov) (current-column))))
          (undo)
          (list env-val exit-code timer-fired state (buffer-string)
                (marker-position m) (overlayp ov) (overlay-start ov)
                (text-properties-at 1) (current-column)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx53_read_eval_backquote_destructuring_splice_lexical_macro_multibyte_hash_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((data '((("café" . 1) ("世界" . 2) ("😀" . 3))))
    (let ((result (eval (car (read-from-string
                               "`(:names ,(mapcar #'car (caar ,data))
                                  :vals ,(mapcar #'cdr (caar ,data))
                                  :len ,(length (caar ,data)))")) t)))
      (list (plist-get result :names)
            (plist-get result :vals)
            (plist-get result :len)
            (secure-hash 'md5 (prin1-to-string result))))))
"##,
        expect,
    );
}

#[test]
fn div_cx53_print_circle_gensym_struct_vector_propertized_escape_all_length_level_full_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx53-rec a b c)
  (let ((print-circle t) (print-gensym t) (print-quoted t)
        (print-escape-newlines t) (print-escape-nonascii t)
        (print-escape-multibyte t) (print-length 3) (print-level 3))
    (let* ((r1 (make-neo-cx53-rec :a "café" :b 1 :c t))
           (r2 (make-neo-cx53-rec :a "世界" :b 2 :c nil))
           (gs1 (gensym)) (gs2 (gensym))
           (s (propertize "data\n\t" 'face 'bold))
           (v (vector r1 r2 r1))
           (x (list 0)))
      (setcdr x x)
      (list (prin1-to-string v)
            (prin1-to-string `(',gs1 ',gs2))
            (prin1-to-string s)
            (prin1-to-string `[,s ,s ,x ,x])
            (length (prin1-to-string (list r1 r2 r1 r2)))
            (length (prin1-to-string (vector (list 1 2) (list 3 4) (list 5 6) (list 7)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx53_subword_forward_backward_cycle_superword_default_comparison_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 6 15)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "snake_case_var camelCase")
      (goto-char 1)
      (let ((default-fwd (progn (forward-word 1) (point))))
        (goto-char 1)
        (subword-mode 1)
        (let ((sub-fwd (progn (forward-word 1) (point))))
          (goto-char 1)
          (superword-mode 1)
          (let ((super-fwd (progn (forward-word 1) (point))))
            (goto-char (1+ super-fwd))
            (superword-mode -1)
            (subword-mode -1)
            (list default-fwd sub-fwd super-fwd)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx53_coding_priority_utf8_utf16_big5_position_category_combined_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((prio (coding-system-priority-list)))
  (list (length prio)
        (cl-position 'utf-8 prio)
        (cl-position 'utf-16 prio)
        (cl-position 'big5 prio)
        (cl-position 'utf-8-auto prio)
        (coding-system-category 'utf-8)
        (coding-system-category 'utf-8-auto)
        (coding-system-category 'emacs-mule)))
"##,
        expect,
    );
}

#[test]
fn div_cx53_set_buffer_multibyte_overlay_marker_textprop_display_evaporate_undo_narrow_widen_full_chain_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Changing multibyteness in a narrowed buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789ABCDEF0123456789")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'display "XX")
  (put-text-property 11 15 'mouse-face 'highlight)
  (let ((ov (make-overlay 16 24)) (m (set-marker (make-marker) 20)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 36)
    (undo-boundary)
    (set-buffer-multibyte nil)
    (let ((s1 (list (length (buffer-string)) (marker-position m)
                    (overlayp ov) (overlay-start ov)
                    (text-properties-at 1) (current-column))))
      (set-buffer-multibyte t)
      (undo-boundary)
      (goto-char 15) (insert "PQR")
      (undo-boundary)
      (delete-region 8 22)
      (undo-boundary)
      (set-text-properties 5 12 '(face underline font-lock-face keyword))
      (let ((s2 (list (length (buffer-string)) (marker-position m)
                     (overlayp ov) (text-properties-at 1)
                     (text-properties-at 5))))
        (undo) (undo) (undo) (undo)
        (widen)
        (list s1 s2 (length (buffer-string))
              (marker-position m) (overlayp ov) (overlay-start ov) (overlay-end ov)
              (text-properties-at 1) (text-properties-at 5) (text-properties-at 10)
              (current-column) (buffer-string))))))
"##,
        expect,
    );
}
