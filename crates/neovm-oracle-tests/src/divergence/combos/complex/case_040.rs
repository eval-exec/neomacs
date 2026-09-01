//! Complex combo batch 40 — deep multi-way MEGA combos (4-5 features each).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx40_narrow_word_movement_subword_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 \"camelCaseVar B\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "AAA camelCaseVar BBB")
      (narrow-to-region 5 19)
      (goto-char 5)
      (forward-word 1)
      (list (point) (buffer-string)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_process_coding_narrow_output_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"PRcafé\\n\" \"PRcafé\\nE\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx40-pc*")))
  (with-current-buffer buf
    (insert "PRE\n")
    (narrow-to-region 1 3))
  (let ((p (make-process :name "neo-cx40-pc" :command '("echo" "café")
                         :buffer buf)))
    (set-process-sentinel p #'ignore)
    (set-process-query-on-exit-flag p nil)
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (let ((narrowed (buffer-string)))
             (widen)
             (list narrowed (buffer-string))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx40_overlay_textprop_undo_marker_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789")
  (put-text-property 1 5 'face 'bold)
  (let ((ov (make-overlay 6 12)) (m (set-marker (make-marker) 10)))
    (overlay-put ov 'face 'italic)
    (narrow-to-region 3 24)
    (undo-boundary)
    (goto-char 7) (insert "XYZ")
    (undo-boundary)
    (put-text-property 5 10 'mouse-face 'highlight)
    (let ((state (list (point-min) (point-max) (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (buffer-string)
                       (text-properties-at 1) (text-properties-at 5)
                       (get-char-property 4 'face))))
      (undo)
      (list state (point-min) (point-max)
            (marker-position m) (overlay-start ov)
            (text-properties-at 1) (text-properties-at 4)))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_print_read_hashtable_circle_gensym_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 35 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((ht (make-hash-table :test 'eq))
       (gs (gensym))
       (print-circle t) (print-gensym t))
  (puthash gs ht ht)
  (let ((p (prin1-to-string (list ht gs))))
    (list (string-match "#s(hash-table" p)
          (string-match "#:" p)
          (string-match "#[0-9]=" p)
          (> (length p) 30))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_regex_casefold_multibyte_replace_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"CAFÉ RÉSUMÉ STRASSE 世界\" 23)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (with-temp-buffer
    (insert "Café RÉSUMÉ straße 世界")
    (goto-char 1)
    (while (re-search-forward "[a-zéàüß]+" nil t)
      (replace-match (upcase (match-string 0))))
    (list (buffer-string) (point-max))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_fileio_coding_markers_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx40-fc-")))
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region "café世界\n" nil f nil 'silent))
  (prog1 (with-temp-buffer
           (buffer-enable-undo)
           (let ((coding-system-for-read 'utf-8-unix))
             (insert-file-contents f))
           (let ((m (set-marker (make-marker) 4)))
             (put-text-property 1 3 'face 'bold)
             (goto-char 3) (insert "X")
             (let ((before (list (buffer-string) (marker-position m)
                                 (text-properties-at 1))))
               (undo)
               (list before (buffer-string) (marker-position m)
                     (text-properties-at 1)))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_timer_process_sentinel_filter_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:t 5 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired sentinel-ev filter-data)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((p (make-process :name "neo-cx40-tpf" :command '("echo" "data")
                         :buffer nil
                         :filter (lambda (proc str) (setq filter-data str))
                         :sentinel (lambda (proc event) (setq sentinel-ev event)))))
    (let ((i 0))
      (while (and (not sentinel-ev) (< i 40))
        (accept-process-output p 0.05)
        (setq i (1+ i)))))
  (list timer-fired
        (if filter-data (length filter-data) nil)
        (if sentinel-ev (string-match "finished" sentinel-ev) nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx40_cl_defmethod_inheritance_advice_cond_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (42 :err (:advice-in :base :sub-after :advice-in :base :sub-after))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx40-base () ((val :initarg :val)))
  (defclass neo-cx40-sub (neo-cx40-base) ())
  (let (log)
    (cl-defgeneric neo-cx40-fn (obj))
    (cl-defmethod neo-cx40-fn ((obj neo-cx40-base))
      (push :base log) (oref obj val))
    (cl-defmethod neo-cx40-fn :after ((obj neo-cx40-sub))
      (push :sub-after log))
    (advice-add 'neo-cx40-fn :around
                (lambda (fn &rest a)
                  (push :advice-in log)
                  (condition-case e (apply fn a)
                    (error (push :caught log) :error-caught))))
    (list (neo-cx40-fn (neo-cx40-sub :val 42))
          (condition-case e (neo-cx40-fn (neo-cx40-sub :val :err))
            (error :handled))
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_window_buffer_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable buffer-markers)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx40-wb*")))
  (with-current-buffer buf
    (insert "0123456789ABCDEF")
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 10)))
      (overlay-put ov 'face 'bold)
      (narrow-to-region 2 14)))
  (set-window-buffer (selected-window) buf)
  (set-window-point (selected-window) 6)
  (prog1 (with-current-buffer buf
           (list (window-point) (point-min) (point-max)
                 (marker-position (cdar buffer-markers))
                 (get-char-property 3 'face)
                 (get-char-property 5 'face)))
    (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx40_hash_weak_gc_count_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'key :test 'eq))
      (alive-count 0))
  (dotimes (i 5)
    (let ((obj (cons i nil)))
      (puthash obj i ht)
      (setq alive-count (1+ alive-count))))
  (garbage-collect)
  (list alive-count (hash-table-count ht)))
"##,
        expect,
    );
}

#[test]
fn div_cx40_subword_kill_word_then_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . user-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (subword-mode 1)
      (insert "camelCaseVar rest")
      (goto-char 1)
      (kill-word 1)
      (let ((after-kill (buffer-string)))
        (undo)
        (list after-kill (buffer-string))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_coding_encode_decode_string_region_roundtrip_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((utf-8 nil t) (utf-16be nil t) (latin-1 nil t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((codings '(utf-8 utf-16be latin-1)))
  (mapcar (lambda (cs)
            (let* ((s "café")
                   (enc-str (encode-coding-string s cs))
                   (enc-region (with-temp-buffer
                                 (insert s)
                                 (encode-coding-region (point-min) (point-max) cs)
                                 (buffer-string)))
                   (dec-str (decode-coding-string enc-str cs)))
              (list cs (equal enc-str enc-region) (equal s dec-str))))
          codings))
"##,
        expect,
    );
}

#[test]
fn div_cx40_char_width_table_modify_display_column_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-width-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aéb")
  (let ((orig (char-width ?é)))
    (set-char-table-range (char-width-table) ?é 3)
    (let ((w (char-width ?é))
          (col (current-column))
          (sw (string-width "aéb")))
      (set-char-table-range (char-width-table) ?é orig)
      (list orig w col sw))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_format_escape_all_modes_multibyte_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#(\\\"caf\\\\x00e9\\\\n\t\\\" 0 6 (face bold))\" 0 \"'sym\" 1)""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-escape-newlines t)
      (print-escape-nonascii t)
      (print-escape-multibyte t)
      (print-circle t)
      (print-gensym t)
      (print-quoted t))
  (let ((s (propertize "café\n\t" 'face 'bold))
        (gs (gensym))
        (x (list 1)))
    (setcdr x x)
    (list (prin1-to-string s)
          (string-match "#:" (prin1-to-string gs))
          (prin1-to-string '(quote sym))
          (string-match "#1=" (prin1-to-string (list x x))))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_marker_overlay_undo_narrow_widen_process_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx40-mo*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "0123456789ABCDEF")
    (let ((m (set-marker (make-marker) 6))
          (ov (make-overlay 3 9)))
      (overlay-put ov 'face 'bold)
      (put-text-property 1 5 'face 'italic)
      (narrow-to-region 2 15)
      (undo-boundary)
      (goto-char 5) (insert "X")
      (undo-boundary)
      (delete-region 4 8)
      (let ((state (list (point-min) (point-max)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (buffer-string)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (point-min) (point-max)
              (marker-position m) (overlay-start ov)
              (text-properties-at 1) (buffer-string))))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_subword_upcase_multiple_words() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"CAMELCASE oneTwoThree\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "camelCase oneTwoThree")
      (goto-char 1)
      (upcase-word 2)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_set_buffer_multibyte_overlay_text_prop_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((11 4 3 (face bold)) 6 4 3 (face bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "café世界")
  (put-text-property 1 3 'face 'bold)
  (let ((ov (make-overlay 3 5)) (m (set-marker (make-marker) 4)))
    (overlay-put ov 'face 'italic)
    (undo-boundary)
    (set-buffer-multibyte nil)
    (let ((nil-state (list (length (buffer-string)) (marker-position m)
                           (overlay-start ov) (text-properties-at 1))))
      (set-buffer-multibyte t)
      (list nil-state (length (buffer-string)) (marker-position m)
            (overlay-start ov) (text-properties-at 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_json_multibyte_roundtrip_hash_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"{\\\"name\\\":\\\"café世界\\\",\\\"items\\\":[1,2,\\\"é\\\"],\\\"flag\\\":true}\" ((name . \"café世界\") (items . [1 2 \"é\"]) (flag . t)) \"2017db1d6bdbe11942231c9efa85eda8\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (let* ((data '((name . "café世界") (items . [1 2 "é"]) (flag . t)))
             (enc (json-encode data))
             (dec (json-read-from-string enc)))
        (list enc dec (secure-hash 'md5 enc))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx40_process_env_coding_exitcode_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"envval\" \"café\" 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((env-result
       (let ((process-environment (cons "NEO_CX40_TEST=envval" process-environment)))
         (shell-command-to-string "echo $NEO_CX40_TEST")))
      (coding-result
       (with-temp-buffer
         (let ((coding-system-for-read 'utf-8-unix))
           (call-process "printf" nil t nil "%s" "café"))
         (buffer-string)))
      (exit-result
       (let ((p (make-process :name "neo-cx40-pe" :command '("sh" "-c" "exit 5"))))
         (accept-process-output p 2)
         (process-exit-status p))))
  (list (string-trim env-result) coding-result exit-result))
"##,
        expect,
    );
}

#[test]
fn div_cx40_overlay_evaporate_undo_marker_textprop_display_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((t 2 (face bold) 8) t nil nil 1 nil 0 \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF")
  (put-text-property 1 4 'face 'bold)
  (let ((ov (make-overlay 4 8)) (m (set-marker (make-marker) 6)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (put-text-property 5 8 'display "XX")
    (undo-boundary)
    (delete-region 2 10)
    (let ((state (list (overlayp ov) (marker-position m)
                       (text-properties-at 1) (current-column))))
      (undo)
      (list state (overlayp ov) (overlay-start ov) (overlay-end ov)
            (marker-position m) (text-properties-at 1)
            (current-column) (buffer-string)))))
"##,
        expect,
    );
}
