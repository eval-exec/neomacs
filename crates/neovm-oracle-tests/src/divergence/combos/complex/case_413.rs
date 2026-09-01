//! Complex combo batch 413 — 20 probes in deeper layers: seq.el,
//! map.el, threading macros, anaphoric conditionals, buffer-local
//! operations, word/line counting, occur, face specs, tty colors,
//! string distance, completion metadata, and buffer modification.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// seq-map / seq-filter / seq-sort / seq-uniq from seq.el.
#[test]
fn div_cx413_seq_map_filter_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((2 3 4) (1 3 5) (9 5 4 3 1 1) (1 2 3 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (seq-map #'1+ '(1 2 3))
      (seq-filter #'oddp '(1 2 3 4 5))
      (seq-sort #'> '(3 1 4 1 5 9))
      (seq-uniq '(1 2 2 3 3 3 4)))
"##,
        expect,
    );
}

/// map-elt / map-put! / map-delete / map-keys from map.el.
#[test]
fn div_cx413_map_elt_put_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function map-put!)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (map-put! ht "a" 1)
  (map-put! ht "b" 2)
  (list (map-elt ht "a")
        (map-elt ht "c" 'not-found)
        (map-delete ht "a")
        (map-elt ht "a" 'deleted)))
"##,
        expect,
    );
}

/// thread-first / thread-last: threading macros.
#[test]
fn div_cx413_thread_first_last() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function thread-first)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (thread-first 5 (+ 3) (* 2) (- 4))
      (thread-last "hello world" (upcase) (split-string " ")))
"##,
        expect,
    );
}

/// when-let* / if-let* / and-let*: anaphoric conditionals.
#[test]
fn div_cx413_when_let_if_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 no 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a 5) (b nil))
  (list (if-let* ((x a)) (* x 2) 'no)
        (if-let* ((x b)) (* x 2) 'no)
        (when-let* ((x a) (y (+ x 1))) (* x y))))
"##,
        expect,
    );
}

/// setq-local / setq-default: buffer-local variable setting.
#[test]
fn div_cx413_setq_local_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"local-val\" \"global-val\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (setq-local neo-cx413-local "local-val")
  (setq-default neo-cx413-global "global-val")
  (list neo-cx413-local
        (default-value 'neo-cx413-global)))
"##,
        expect,
    );
}

/// defvar-local: defining buffer-local variables.
#[test]
fn div_cx413_defvar_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"default\" \"default\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defvar-local neo-cx413-dvl "default")
  (with-temp-buffer
    (setq neo-cx413-dvl "local")
    (list neo-cx413-dvl
          (default-value 'neo-cx413-dvl)))
  (list neo-cx413-dvl
        (default-value 'neo-cx413-dvl)))
"##,
        expect,
    );
}

/// compare-buffer-substrings: comparing buffer regions.
#[test]
fn div_cx413_compare_buffer_substrings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (-1 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (list (compare-buffer-substrings nil 1 4 nil 4 7)
        (compare-buffer-substrings nil 1 4 nil 1 4)
        (compare-buffer-substrings nil 1 nil nil 1 nil)))
"##,
        expect,
    );
}

/// count-words / count-words-region: word counting.
#[test]
fn div_cx413_count_words_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world from neovm oracle test")
  (list (count-words-region (point-min) (point-max))
        (count-words (point-min) (point-max))))
"##,
        expect,
    );
}

/// flush-lines / keep-lines: line filtering operations.
#[test]
fn div_cx413_flush_keep_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"other\\nskip\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc123\nother\nabc456\nskip\nabc789")
  (goto-char (point-min))
  (flush-lines "^abc" (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

/// how-many: counting pattern matches.
#[test]
fn div_cx413_how_many() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a b a b a b a")
  (how-many "a" (point-min) (point-max)))
"##,
        expect,
    );
}

/// occur: searching and displaying matches.
#[test]
fn div_cx413_occur_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx413-occur*")))
  (with-current-buffer buf
    (insert "line1 with match\nline2 no\nline3 match again\nline4\nline5 match end\n"))
  (let ((occur-buf (get-buffer-create "*Occur*")))
    (with-current-buffer buf
      (occur "match"))
    (prog1 (with-current-buffer occur-buf
             (count-lines (point-min) (point-max)))
      (kill-buffer buf)
      (kill-buffer occur-buf))))
"##,
        expect,
    );
}

/// assoc-string / assoc-default: case-insensitive alist lookup.
#[test]
fn div_cx413_assoc_string_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((al '(("Foo" . 1) ("bar" . 2) ("BAZ" . 3))))
  (list (assoc-string "foo" al t)
        (assoc-string "BAR" al t)
        (assoc-string "baz" al)
        (assoc-default "FOO" al t)))
"##,
        expect,
    );
}

/// find-face / face-default-spec / face-user-default-spec.
#[test]
fn div_cx413_find_face_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function find-face)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (find-face 'bold)
      (find-face 'nonexistent-cx413-face)
      (face-default-spec 'bold)
      (face-user-default-spec 'bold))
"##,
        expect,
    );
}

/// face-spec-set / face-spec-reset-face.
#[test]
fn div_cx413_face_spec_set_reset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"red\" nil \"unspecified-fg\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-face 'neo-cx413-face)))
  (face-spec-set f '((t (:foreground "red"))))
  (list (face-attribute f :foreground nil 'default)
        (face-spec-reset-face f)
        (face-attribute f :foreground nil 'default)))
"##,
        expect,
    );
}

/// tty-color-define / tty-color-alist / tty-color-clear.
#[test]
fn div_cx413_tty_color_define() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t (\"red\" 1 65535 0 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((color (tty-color-alist)))
  (list (listp color)
        (> (length color) 0)
        (assoc "red" color)))
"##,
        expect,
    );
}

/// string-distance: Levenshtein distance between strings.
#[test]
fn div_cx413_string_distance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 0 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-distance "kitten" "sitting")
      (string-distance "hello" "hello")
      (string-distance "abc" "xyz"))
"##,
        expect,
    );
}

/// buffer-chars-modified-tick: modification tracking.
#[test]
fn div_cx413_buffer_chars_modified_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((tick1 (buffer-chars-modified-tick)))
    (insert "hello")
    (let ((tick2 (buffer-chars-modified-tick)))
      (list (/= tick1 tick2)
            (integerp tick1)
            (integerp tick2)))))
"##,
        expect,
    );
}

/// completion-table-dynamic: dynamic completion table.
#[test]
fn div_cx413_completion_table_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"ap\" (\"apple\" \"apply\" \"apt\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((words '("apple" "apply" "apt" "banana" "band")))
  (let ((table (completion-table-dynamic
                (lambda (str)
                  (all-completions str words)))))
    (list (try-completion "ap" table)
          (all-completions "ap" table))))
"##,
        expect,
    );
}

/// completion-metadata / completion-metadata-get.
#[test]
fn div_cx413_completion_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((table '("hello" "help" "helicopter")))
  (let ((md (completion-metadata "hel" table nil)))
    (list (completion-metadata-get md 'category)
          (completion-metadata-get md 'display-sort-function))))
"##,
        expect,
    );
}

/// display-supports-face-attributes-p: face capability detection.
#[test]
fn div_cx413_display_supports_face_attrs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (display-supports-face-attributes-p '(:foreground "red"))
      (display-supports-face-attributes-p '(:weight bold))
      (display-supports-face-attributes-p '(:stipple "gray1")))
"##,
        expect,
    );
}
