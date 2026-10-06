//! GNU-refreshed GDN editing primitive regressions.
#[path = "../src/common.rs"]
mod common;

#[test]
fn oracle_gdn_buf07() {
    common::assert_oracle_parity_expect(
        r#"(with-temp-buffer (insert "中bc") (goto-char 1) (forward-word) (point))"#,
        expect_test::expect![[r#""OK 2""#]],
    );
}

#[test]
fn oracle_gdn_buf14() {
    common::assert_oracle_parity_expect(
        r#"(with-temp-buffer (insert "abc") (setq buffer-read-only t) (list (delete-char 0) (buffer-string)))"#,
        expect_test::expect![[r#""OK (nil \"abc\")""#]],
    );
}

#[test]
fn oracle_gdn_buf15() {
    common::assert_oracle_parity_expect(
        r#"(with-temp-buffer (insert "abc") (goto-char 2) (delete-char nil))"#,
        expect_test::expect![[r#""ERR (wrong-type-argument fixnump nil)""#]],
    );
}

#[test]
fn oracle_gdn_buf16() {
    common::assert_oracle_parity_expect(
        r#"(with-temp-buffer (insert "abc") (list (insert-buffer-substring nil 1 2) (buffer-string)))"#,
        expect_test::expect![[r#""ERR (wrong-type-argument stringp nil)""#]],
    );
}

#[test]
fn oracle_gdn_buf17() {
    common::assert_oracle_parity_expect(
        r#"(with-temp-buffer (set-buffer-multibyte nil) (insert "abc\351") (subst-char-in-region 1 5 #x3fffe9 ?z) (subst-char-in-region 1 5 #x161 ?Z) (buffer-string))"#,
        expect_test::expect![[r#""OK \"Zbcz\"""#]],
    );
}

#[test]
fn oracle_gdn_buf18() {
    common::assert_oracle_parity_expect(
        r#"(with-temp-buffer (insert "abé") (subst-char-in-region 1 4 ?a ?é))"#,
        expect_test::expect![[
            r#""ERR (error \"Characters in ‘subst-char-in-region’ have different byte-lengths\")""#
        ]],
    );
}

#[test]
fn oracle_gdn_buf19() {
    common::assert_oracle_parity_expect(
        r#"(let ((b (generate-new-buffer " o"))) (with-current-buffer b (insert "é中😀abcd")) (with-temp-buffer (insert "abcdefghijklmnop") (let ((m (with-current-buffer b (copy-marker 4)))) (prog1 (list (char-after m) (char-before m)) (kill-buffer b)))))"#,
        expect_test::expect![[r#""OK (106 105)""#]],
    );
}

#[test]
fn oracle_gdn_word_motion_scripts_categories_and_overrides() {
    common::assert_oracle_parity_expect(
        r#"(list
      (mapcar (lambda (s) (with-temp-buffer (insert s) (goto-char 1)
        (let ((f (progn (forward-word) (point))))
          (goto-char (point-max)) (backward-word) (list f (point)))))
        '("中bc" "abαβ" "ひらカタ" "abc中" "éabc"))
      (let ((char-script-table (make-char-table 'char-script-table 'latin)))
        (set-char-table-range char-script-table ?b 'greek)
        (with-temp-buffer (insert "ab") (goto-char 1) (forward-word) (point)))
      (let ((word-separating-categories nil))
        (with-temp-buffer (insert "ひらカタ") (goto-char 1) (forward-word) (point))))"#,
        expect_test::expect![[r#""OK (((2 2) (3 3) (3 3) (4 4) (5 1)) 2 5)""#]],
    );
}

#[test]
fn oracle_gdn_word_motion_nil_category_sets() {
    common::assert_oracle_parity_expect(
        r#"(mapcar (lambda (different-scripts)
      (mapcar (lambda (present)
        (with-temp-buffer
          (insert "ab")
          (let ((table (make-char-table 'category-table nil))
                (char-script-table (make-char-table 'char-script-table 'latin))
                (word-separating-categories '((nil . nil)))
                (word-combining-categories '((nil . nil))))
            (when different-scripts
              (set-char-table-range char-script-table ?b 'greek))
            (when (memq present '(first both))
              (set-char-table-range table ?a (make-category-set "")))
            (when (memq present '(second both))
              (set-char-table-range table ?b (make-category-set "")))
            (set-category-table table)
            (goto-char 1)
            (let ((forward (progn (forward-word) (point))))
              (goto-char (point-max))
              (backward-word)
              (list forward (point))))))
        '(none first second both))) '(nil t))"#,
        expect_test::expect![[r#""OK (((3 1) (3 1) (3 1) (2 2)) ((2 2) (2 2) (2 2) (3 1)))""#]],
    );
}

#[test]
fn oracle_gdn_word_motion_script_entry_identity() {
    common::assert_oracle_parity_expect(
        r#"(mapcar (lambda (make-entry)
      (mapcar (lambda (shared)
        (let* ((left (funcall make-entry))
               (right (if shared left (funcall make-entry)))
               (char-script-table (make-char-table 'char-script-table nil)))
          (set-char-table-range char-script-table ?a left)
          (set-char-table-range char-script-table ?b right)
          (with-temp-buffer
            (insert "ab")
            (set-category-table (make-char-table 'category-table nil))
            (goto-char 1)
            (let ((forward (progn (forward-word) (point))))
              (goto-char (point-max))
              (backward-word)
              (list (equal left right) (eq left right) forward (point))))))
        '(nil t)))
      (list (lambda () (vector 1))
            (lambda () (copy-sequence "script"))
            (lambda () (make-symbol "script"))))"#,
        expect_test::expect![[
            r#""OK (((t nil 2 2) (t t 3 1)) ((t nil 2 2) (t t 3 1)) ((nil nil 2 2) (t t 3 1)))""#
        ]],
    );
}

#[test]
fn oracle_gdn_word_motion_preserves_emacs_character_codes() {
    common::assert_oracle_parity_expect(
        r#"(mapcar (lambda (shape)
      (mapcar (lambda (properties)
        (mapcar (lambda (mode)
          (let ((char-script-table (make-char-table 'char-script-table 'latin))
                (word-combining-categories '((?r . nil)))
                (word-separating-categories '((?r . nil)))
                (code (nth 2 shape)))
            (set-char-table-range char-script-table code
              (if (eq mode 'separate) 'latin 'greek))
            (set-char-table-range char-script-table #x80 'latin)
            (set-char-table-range char-script-table #xfffd 'latin)
            (with-temp-buffer
              (set-buffer-multibyte (not (car shape)))
              (insert (nth 1 shape))
              (setq-local parse-sexp-lookup-properties t)
              (let ((syntax (make-syntax-table))
                    (categories (make-char-table 'category-table nil)))
                (modify-syntax-entry code
                  (if (memq properties '(cons table)) " " "w") syntax)
                (modify-syntax-entry #x80 " " syntax)
                (modify-syntax-entry #xfffd " " syntax)
                (modify-syntax-entry ?a "w" syntax)
                (set-syntax-table syntax)
                (unless (eq mode 'script)
                  (set-char-table-range categories code (make-category-set "r"))
                  (set-char-table-range categories ?a (make-category-set "")))
                (set-category-table categories))
              (cond
                ((eq properties 'face)
                 (put-text-property 1 (point-max) 'face 'bold))
                ((eq properties 'cons)
                 (put-text-property 1 2 'syntax-table (string-to-syntax "w")))
                ((eq properties 'table)
                 (let ((override (make-syntax-table)))
                   (modify-syntax-entry code "w" override)
                   (modify-syntax-entry #x80 " " override)
                   (modify-syntax-entry #xfffd " " override)
                   (put-text-property 1 2 'syntax-table override))))
              (goto-char 1)
              (let ((forward (progn (forward-word) (point))))
                (goto-char (point-max))
                (backward-word)
                (list forward (point))))))
          '(script combine separate))) '(none face cons table)))
      (list (list t (unibyte-string #x80 ?a) #x3fff80)
            (list nil (string-to-multibyte (unibyte-string #x80 ?a)) #x3fff80)
            (list nil (string #x110000 ?a) #x110000)))"#,
        expect_test::expect![[
            r#""OK ((((2 2) (3 1) (2 2)) ((2 2) (3 1) (2 2)) ((2 2) (3 1) (2 2)) ((2 2) (3 1) (2 2))) (((2 2) (3 1) (2 2)) ((2 2) (3 1) (2 2)) ((2 2) (3 1) (2 2)) ((2 2) (3 1) (2 2))) (((2 2) (3 1) (2 2)) ((2 2) (3 1) (2 2)) ((2 2) (3 1) (2 2)) ((2 2) (3 1) (2 2))))""#
        ]],
    );
}

#[test]
fn oracle_gdn_native_error_quoting_styles() {
    common::assert_oracle_parity_expect(
        r#"(mapcar (lambda (style)
      (let ((text-quoting-style style))
        (list (condition-case e (with-temp-buffer (insert "abé")
          (subst-char-in-region 1 4 ?a ?é)) (error e))
          (let ((dead (generate-new-buffer "dead"))) (kill-buffer dead)
            (condition-case e (buffer-swap-text dead) (error e))))))
      '(curve grave straight))"#,
        expect_test::expect![[
            r#""OK (((error \"Characters in ‘subst-char-in-region’ have different byte-lengths\") (error \"Cannot swap a dead buffer’s text\")) ((error \"Characters in `subst-char-in-region' have different byte-lengths\") (error \"Cannot swap a dead buffer's text\")) ((error \"Characters in 'subst-char-in-region' have different byte-lengths\") (error \"Cannot swap a dead buffer's text\")))""#
        ]],
    );
}

#[test]
fn oracle_gdn_buffer_validation_and_encoding_shapes() {
    common::assert_oracle_parity_expect(
        r#"(list
      (mapcar (lambda (arg) (condition-case e (delete-char arg)
        (error (list (car e) (cadr e)
          (if (markerp (caddr e)) 'marker (caddr e))))))
        (list nil 1.0 'a (expt 2 70) (point-marker)))
      (condition-case e (insert-buffer-substring-no-properties nil) (error e))
      (condition-case e (char-after (make-marker)) (error e))
      (condition-case e (char-before (make-marker)) (error e))
      (with-temp-buffer (set-buffer-multibyte nil) (insert (unibyte-string 97 233))
        (subst-char-in-region 1 3 #x161 #x1ff) (string-to-list (buffer-string)))
      (with-temp-buffer (insert "aé") (subst-char-in-region 1 3 ?é ?ü)
        (buffer-string)))"#,
        expect_test::expect![[
            r#""OK (((wrong-type-argument fixnump nil) (wrong-type-argument fixnump 1.0) (wrong-type-argument fixnump a) (wrong-type-argument fixnump 1180591620717411303424) (wrong-type-argument fixnump marker)) (wrong-type-argument stringp nil) (error \"Marker does not point anywhere\") (error \"Marker does not point anywhere\") (255 233) \"aü\")""#
        ]],
    );
}

#[test]
fn oracle_gdn_transpose_indirect_points_and_markers() {
    common::assert_oracle_parity_expect(
        r#"(mapcar (lambda (leave)
  (with-temp-buffer
    (insert "abcdefgh") (goto-char 2)
    (let* ((sibling (make-indirect-buffer (current-buffer) " transpose-sibling"))
           (m (with-current-buffer sibling (goto-char 6) (point-marker))))
      (unwind-protect
        (progn (transpose-regions 1 3 5 7 leave)
          (list (point) (with-current-buffer sibling (point))
                (marker-position m) (buffer-string)))
        (kill-buffer sibling))))) '(nil t))"#,
        expect_test::expect![[r#""OK ((6 2 2 \"efcdabgh\") (2 6 6 \"efcdabgh\"))""#]],
    );
}

#[test]
fn oracle_gdn_compiled_editing_primitives() {
    common::assert_oracle_parity_expect(
        r#"(progn
      (require 'bytecomp)
      (let ((byte-compile-warnings nil))
        (mapcar (lambda (body)
          (let ((fn (byte-compile body)) answer)
            (dotimes (_ 20) (setq answer (funcall fn))) answer))
          '((lambda () (with-temp-buffer (insert "abc") (setq buffer-read-only t)
               (list (delete-char 0) (buffer-string))))
            (lambda () (with-temp-buffer (insert "abc") (goto-char 2)
               (condition-case e (delete-char nil) (error e))))
            (lambda () (with-temp-buffer (set-buffer-multibyte nil)
               (insert (unibyte-string 97 233)) (subst-char-in-region 1 3 #x161 ?Z)
               (string-to-list (buffer-string))))
            (lambda () (with-temp-buffer (insert "abcdefgh") (goto-char 2)
               (transpose-regions 1 3 5 7 t) (point)))))))"#,
        expect_test::expect![[
            r#""OK ((nil \"abc\") (wrong-type-argument fixnump nil) (90 233) 2)""#
        ]],
    );
}
