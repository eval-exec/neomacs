mod common;

fn parity(form: &str, expected: expect_test::Expect) {
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        expected.assert_eq(&common::run_oracle_eval(form).expect("GNU 31.1 oracle"));
    } else {
        expected.assert_eq(&common::run_neovm_eval(form).expect("neomacs runtime"));
    }
}

// GNU src/indent.c:818-821: ctl-arrow selects caret (2) or octal (4) columns.
#[test]
fn gdm_columns_control_arrow() {
    parity(
        r#"(mapcar (lambda (multi) (mapcar (lambda (arrow) (with-temp-buffer (set-buffer-multibyte multi) (setq ctl-arrow arrow) (insert "\x01\x02\177") (list (current-column) (progn (goto-char 1) (list (move-to-column 4) (point))) (progn (goto-char (point-max)) (indent-to 15) (string-to-list (buffer-string)))))) '(nil t))) '(nil t))"#,
        expect_test::expect![[
            r#"OK (((12 (4 2) (1 2 127 32 32 32)) (6 (4 3) (1 2 127 9 32 32 32 32 32 32 32))) ((12 (4 2) (1 2 127 32 32 32)) (6 (4 3) (1 2 127 9 32 32 32 32 32 32 32))))"#
        ]],
    );
}

// GNU src/indent.c:1037-1045: skip invisible text at property boundaries.
#[test]
fn gdm_columns_invisible_indentation() {
    parity(
        r#"(list (with-temp-buffer (insert (propertize "   " 'invisible t) "abc") (current-indentation)) (with-temp-buffer (insert "  " (propertize "xx" 'invisible 'foo) "  abc") (current-indentation)) (with-temp-buffer (insert "    abc") (overlay-put (make-overlay 1 3) 'invisible t) (current-indentation)) (with-temp-buffer (setq buffer-invisibility-spec '((foo . t))) (insert (propertize "   " 'invisible 'foo) "abc") (current-indentation)) (with-temp-buffer (insert (propertize "é\t" 'invisible t) " \tabc") (current-indentation)))"#,
        expect_test::expect![[r#"OK (0 4 2 3 8)"#]],
    );
}

// GNU src/buffer.h:1682-1686: invalid tab widths fall back to eight.
#[test]
fn gdm_columns_tab_width_validation() {
    parity(
        r#"(mapcar (lambda (w) (with-temp-buffer (setq tab-width w) (insert "\t") (list (current-column) (progn (goto-char 1) (list (move-to-column 3) (point))) (progn (erase-buffer) (indent-to 20) (string-to-list (buffer-string)))))) '(1001 2000 0 -3 1 4 1000))"#,
        expect_test::expect![[
            r#"OK ((8 (8 2) (9 9 32 32 32 32)) (8 (8 2) (9 9 32 32 32 32)) (8 (8 2) (9 9 32 32 32 32)) (8 (8 2) (9 9 32 32 32 32)) (1 (1 2) (9 9 9 9 9 9 9 9 9 9 9 9 9 9 9 9 9 9 9 9)) (4 (4 2) (9 9 9 9 9)) (1000 (1000 2) (32 32 32 32 32 32 32 32 32 32 32 32 32 32 32 32 32 32 32 32)))"#
        ]],
    );
}

// GNU src/indent.c:425-427,780,798: only selective-display t ends at CR.
#[test]
fn gdm_columns_selective_display() {
    parity(
        r#"(mapcar (lambda (mode) (with-temp-buffer (setq selective-display mode) (insert "ab\rcd") (list (current-column) (progn (goto-char 1) (list (move-to-column 3) (point)))))) '(t nil 3 hide))"#,
        expect_test::expect![[r#"OK ((2 (2 3)) (6 (4 4)) (6 (4 4)) (6 (4 4)))"#]],
    );
}

// GNU src/indent.c:349-362,425-427 versus current_column_1:850-859.
#[test]
fn gdm_columns_selective_display_scan_modes() {
    parity(
        r#"(list (mapcar (lambda (s) (with-temp-buffer (setq selective-display t) (insert s) (current-column))) '("abc\rx" "a\rxyz" "éa\rxyz" "a\réxy")) (with-temp-buffer (insert "a\rxyz") (put-text-property 1 2 'face 'bold) (setq selective-display t) (current-column)))"#,
        expect_test::expect![[r#"OK ((1 3 2 1) 1)"#]],
    );
}

// GNU src/indent.c:780-788: each glyph is one column, TAB advances to tab stop.
#[test]
fn gdm_columns_display_glyphs() {
    parity(
        r#"(mapcar (lambda (glyphs) (with-temp-buffer (setq buffer-display-table (make-display-table)) (aset buffer-display-table ?a glyphs) (insert "ab") (list (current-column) (progn (goto-char 1) (list (move-to-column 1) (point)))))) '([?中 ?中] [?中] [?x ?y] [?\t ?x] []))"#,
        expect_test::expect![[r#"OK ((3 (2 2)) (2 (1 2)) (3 (2 2)) (10 (9 2)) (1 (1 3)))"#]],
    );
}

// GNU src/indent.c:975,982: separate inheriting tab and space insertions.
#[test]
fn gdm_columns_indentation_hook_boundaries() {
    parity(
        r#"(mapcar (lambda (move) (with-temp-buffer (insert "abc") (let (before after) (add-hook 'before-change-functions (lambda (&rest a) (push a before)) nil t) (add-hook 'after-change-functions (lambda (&rest a) (push a after)) nil t) (if move (move-to-column 10 t) (indent-to 10)) (list (nreverse before) (nreverse after) (string-to-list (buffer-string)))))) '(nil t))"#,
        expect_test::expect![[
            r#"OK ((((4 4) (5 5)) ((4 5 0) (5 7 0)) (97 98 99 9 32 32)) (((4 4) (5 5)) ((4 5 0) (5 7 0)) (97 98 99 9 32 32)))"#
        ]],
    );
}

#[test]
fn gdm_columns_compiled_call_paths() {
    parity(
        r#"(let ((fn (byte-compile '(lambda () (with-temp-buffer (setq ctl-arrow nil tab-width 1001) (insert "\001\t") (list (current-column) (progn (goto-char 1) (move-to-column 4)) (indent-to 20) (string-to-list (buffer-string)))))))) (funcall fn) (funcall fn) (funcall fn))"#,
        expect_test::expect![[r#"OK (8 4 20 (1 9 9 32 32 32 32 9))"#]],
    );
}

// GNU src/indent.c:395-462 processes display vectors backwards in simple buffers;
// :760-788 processes them forwards when text properties require the general scan.
#[test]
fn gdm_columns_selective_display_glyph_interactions() {
    parity(
        r#"(mapcar (lambda (props) (mapcar (lambda (glyphs) (with-temp-buffer (setq selective-display t buffer-display-table (make-display-table)) (aset buffer-display-table ?\r glyphs) (insert "ab\rcd") (when props (put-text-property 1 2 'face 'bold)) (list (current-column) (progn (goto-char 1) (list (move-to-column 10) (point)))))) '([?x] [?\r ?z] [?x ?\r] [?x ?\t] []))) '(nil t))"#,
        expect_test::expect![[
            r###"OK (((5 (5 6)) (3 (2 3)) (2 (3 3)) (10 (10 6)) (4 (4 6))) ((5 (5 6)) (2 (2 3)) (3 (3 3)) (10 (10 6)) (4 (4 6))))"###
        ]],
    );
}

// GNU indent.c:805-816; buffer.h:1708-1715. Multibyte widths read the live
// char-width-table; display vectors take precedence and unibyte high bytes
// remain octal columns. Expectations are refreshed from GNU Emacs 31.1.
#[test]
fn gdh_columns_live_character_width_table() {
    parity(
        r#" (let ((char-width-table (copy-sequence char-width-table)))
   (aset char-width-table ?é 7)
   (aset char-width-table ?中 0)
   (aset char-width-table #x3fff80 6)
   (aset char-width-table ?a 9)
   (list
    (mapcar (lambda (props)
              (with-temp-buffer
                (insert "aé中é")
                (when props (put-text-property 1 2 'face 'bold))
                (list (current-column)
                      (progn (goto-char 1) (list (move-to-column 3) (point)))
                      (progn (goto-char (point-max)) (indent-to 20) (current-column)))))
            '(nil t))
    (with-temp-buffer (insert (string #x3fff80)) (current-column))
    (with-temp-buffer (set-buffer-multibyte nil) (insert (unibyte-string 233)) (current-column))
    (with-temp-buffer
      (setq buffer-display-table (make-display-table))
      (aset buffer-display-table ?é [?x ?y])
      (insert "é") (current-column))
    (mapcar (lambda (width)
              (let ((char-width-table (copy-sequence char-width-table)))
                (aset char-width-table ?é width)
                (with-temp-buffer (insert "é") (current-column))))
            '(-1 0 3 1000 1001))))"#,
        expect_test::expect!["OK (((15 (8 3) 20) (15 (8 3) 20)) 6 4 2 (1000 0 3 1000 1000))"],
    );
}

// GNU indent.c:805-816 reads live widths. Each scan must refresh its table
// while repeated characters and memo hash collisions retain exact widths.
#[test]
fn gdh_columns_width_memo_live_calls_and_collisions() {
    parity(
        r#"(let ((char-width-table (copy-sequence char-width-table)))
   (aset char-width-table ?é 7)
   (aset char-width-table #x1e8 9)
   (with-temp-buffer
     (insert "éǨé")
     (let ((first (current-column)))
       (aset char-width-table ?é 3)
       (goto-char 1) (current-column) (goto-char (point-max))
       (let ((second (current-column)))
         (aset char-width-table ?é 0)
         (goto-char 1) (current-column) (goto-char (point-max))
         (list first second (current-column))))))"#,
        expect_test::expect!["OK (23 15 9)"],
    );
}
