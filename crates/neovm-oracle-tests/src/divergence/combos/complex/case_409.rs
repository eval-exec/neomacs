//! Complex combo batch 409 — 20 probes in new territory: window-point/start,
//! pos-visible-in-window-p, input-pending-p, recent-keys, this-command-keys,
//! keyboard-translate, translation-table, key-translation-map, locale-coding-system,
//! file-coding-system-alist, file-equal-p/file-in-directory-p, file-name-base/extension,
//! make-backup-file-name, find-backup-file-name, replace-regexp-in-string with
//! subexp count, sort-subr, sequential-command, describe-key-briefly,
//! help-buffer/help-setup-xref, apropos, and format-find-file.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// window-point / window-start with different window layouts.
#[test]
fn div_cx409_window_point_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\nline3\nline4\nline5")
  (let ((w (selected-window)))
    (set-window-point w 3)
    (set-window-start w 2)
    (list (window-point w)
          (window-start w))))
"##,
        expect,
    );
}

/// pos-visible-in-window-p with partially visible lines.
#[test]
fn div_cx409_pos_visible_window_partial() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaa bbb ccc ddd eee fff ggg hhh iii jjj")
  (list (pos-visible-in-window-p 1)
        (pos-visible-in-window-p (point-max))
        (pos-visible-in-window-p 5 nil t)))
"##,
        expect,
    );
}

/// input-pending-p / recent-keys: keyboard state queries
/// in batch mode (should return nil/empty).
#[test]
fn div_cx409_input_pending_recent_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function last-command-keys)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (input-pending-p)
      (length (recent-keys))
      (this-command-keys)
      (last-command-keys))
"##,
        expect,
    );
}

/// keyboard-translate: translation table for key events.
#[test]
fn div_cx409_keyboard_translate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (keyboard-translate ?a ?b)
  (let ((kt (keyboard-translate ?a)))
    (list kt
          (if kt (char-equal kt ?b) nil))))
"##,
        expect,
    );
}

/// locale-coding-system / file-coding-system-alist:
/// coding system configuration may differ.
#[test]
fn div_cx409_coding_system_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function locale-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (locale-coding-system)
      (keyboard-coding-system)
      (file-coding-system-alist))
"##,
        expect,
    );
}

/// file-equal-p / file-in-directory-p with temp files.
#[test]
fn div_cx409_file_equal_in_dir() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((d (make-temp-file "neo-cx409-dir-" t))
      (f1 (make-temp-file "neo-cx409-f1-"))
      (f2 (make-temp-file "neo-cx409-f2-")))
  (unwind-protect
      (list (file-equal-p f1 f1)
            (file-equal-p f1 f2)
            (file-in-directory-p f1 default-directory)
            (file-in-directory-p f1 d))
    (delete-file f1)
    (ignore-errors (delete-file f2))
    (ignore-errors (delete-directory d t))))
"##,
        expect,
    );
}

/// file-name-base / file-name-extension / file-name-sans-extension.
#[test]
fn div_cx409_file_name_base_ext() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"foo\" \"txt\" \"foo\" \"bar.tar\" \"gz\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (file-name-base "foo.txt")
      (file-name-extension "foo.txt")
      (file-name-sans-extension "foo.txt")
      (file-name-base "/path/to/bar.tar.gz")
      (file-name-extension "/path/to/bar.tar.gz"))
"##,
        expect,
    );
}

/// make-backup-file-name / find-backup-file-name.
#[test]
fn div_cx409_backup_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"/tmp/neo-cx409-fixed-name.el~\" \"el\" \"neo-cx409-fixed-name\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f "/tmp/neo-cx409-fixed-name.el"))
  (list (make-backup-file-name f)
        (file-name-extension (make-backup-file-name f))
        (file-name-base (make-backup-file-name f))))
"##,
        expect,
    );
}

/// replace-regexp-in-string with subexp replacement and count.
#[test]
fn div_cx409_replace_regexp_subexp_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello! world!\" \"ello! world!\" \"X XXX\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (replace-regexp-in-string "\\([a-z]+\\)" "\\1!" "hello world")
        (replace-regexp-in-string "\\([a-z]+\\)" "\\1!" "hello world" nil nil nil 1)
        (replace-regexp-in-string "a" "X" "aaa aaa" nil nil nil 2)))
"##,
        expect,
    );
}

/// sort-subr with custom predicate: sorting buffer regions.
#[test]
fn div_cx409_sort_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"banana\\napple\\ncherry\\ndate\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "banana\napple\ncherry\ndate\n")
  (sort-subr nil 'forward-line 'end-of-line nil nil
             (lambda (a b) (string< (buffer-substring a b) (buffer-substring (car b) (cdr b)))))
  (buffer-string))
"##,
        expect,
    );
}

/// describe-key-briefly: formatted key description.
#[test]
fn div_cx409_describe_key_briefly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"C-c C-f\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'help)
  (with-temp-buffer
    (let ((map (make-sparse-keymap)))
      (define-key map "a" 'forward-char)
      (list (describe-key-briefly "a" map)
            (key-description (kbd "C-c C-f"))))))
"##,
        expect,
    );
}

/// help-buffer / help-setup-xref: help infrastructure.
#[test]
fn div_cx409_help_buffer_xref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"*Help*\" \" *temp*\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'help-mode)
  (with-temp-buffer
    (help-setup-xref (list 'forward-char) (interactive-form 'forward-char))
    (list (help-buffer)
          (buffer-name (current-buffer)))))
"##,
        expect,
    );
}

/// apropos: symbol searching may behave differently.
#[test]
fn div_cx409_apropos_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((delete-forward-char forward-char kill-forward-chars) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'apropos)
  (let* ((items (apropos "forward-char"))
         (buf (get-buffer-create "*Apropos*"))
         (text (with-current-buffer buf
                 (buffer-substring-no-properties (point-min) (point-max)))))
    (prog1 (list (mapcar #'car items)
                 (and (string-match-p "delete-forward-char\n  Command: Delete" text) t)
                 (and (string-match-p "forward-char\n  Command: Move point" text) t)
                 (and (string-match-p "kill-forward-chars\n  Function: (not documented)" text) t))
      (kill-buffer buf))))
"##,
        expect,
    );
}

/// format-find-file / format-insert-file: format conversion
/// on file read.
#[test]
fn div_cx409_format_find_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Unknown format \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx409-fmt-")))
  (with-temp-file f (insert "test content"))
  (unwind-protect
      (with-temp-buffer
        (format-find-file f '(""))
        (buffer-string))
    (delete-file f)))
"##,
        expect,
    );
}

/// save-buffer / basic-save-buffer: save operations.
#[test]
fn div_cx409_save_buffer_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx409-sv-")))
  (unwind-protect
      (with-temp-file f
        (insert "original")
        (list (buffer-modified-p)
              (buffer-file-name)))
    (delete-file f)))
"##,
        expect,
    );
}

/// window-vscroll / set-window-vscroll: vertical scroll.
#[test]
fn div_cx409_window_vscroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert (make-string 100 ?a))
  (let ((w (selected-window)))
    (list (window-vscroll w)
          (set-window-vscroll w 10.0)
          (window-vscroll w))))
"##,
        expect,
    );
}

/// compare-window-configurations: structural equality (same config).
#[test]
fn div_cx409_compare_window_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((c1 (current-window-configuration))
      (c2 (current-window-configuration)))
  (compare-window-configurations c1 c2))
"##,
        expect,
    );
}

/// force-window-update / redisplay: redisplay triggers.
#[test]
fn div_cx409_force_window_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "test")
  (list (force-window-update (selected-window))
        (redisplay t)))
"##,
        expect,
    );
}

/// translation-table / set-translation-table:
/// character translation tables.
#[test]
fn div_cx409_translation_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tt (make-translation-table)))
  (list (char-table-p tt)
        (condition-case e (set-translation-table tt) (error (car e)))))
"##,
        expect,
    );
}

/// sit-for with zero seconds: yields to process I/O.
#[test]
fn div_cx409_sit_for_zero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (sit-for 0)
      (sit-for 0.01))
"##,
        expect,
    );
}
