//! GNU-backed width policy regressions for the GDM lane.
mod common;

fn parity(form: &str, expected: expect_test::Expect) {
    if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
        expected.assert_eq(&common::run_oracle_eval(form).expect("GNU 31.1 oracle"));
    } else {
        expected.assert_eq(&common::run_neovm_eval(form).expect("neomacs runtime"));
    }
}

#[test]
fn gdm_widths_unibyte_c1_width_and_format() {
    parity(
        r#"(let ((snapshot (lambda (s)
                    (list (string-to-list s) (multibyte-string-p s)
                          (let ((i 0) properties)
                            (while (< i (length s))
                              (push (text-properties-at i s) properties)
                              (setq i (1+ i)))
                            (nreverse properties))))))
             (list (string-width "\200") (string-width "\237")
                   (string-width "a\200b") (string-width "\200\240\377")
                   (funcall snapshot (format "%3s|" "\200"))
                   (funcall snapshot (format "%.1s|" "\200"))
                   (string-width (string-to-multibyte "\200"))))"#,
        expect_test::expect![[r###"OK (4 4 6 6 ((128 124) nil (nil nil)) ((124) nil (nil)) 4)"###]],
    );
}

#[test]
fn gdm_widths_ctl_arrow_controls() {
    parity(
        r#"(let ((ctl-arrow nil))
               (list (string-width "\1") (char-width 1)
                     (string-width "\177") (format "%5s|" "\1")
                     (string-width "a\1b" 1 2)))"#,
        expect_test::expect![[r###"OK (4 4 4 " |" 4)"###]],
    );
}

#[test]
fn gdm_widths_format_dynamic_tab_width() {
    parity(
        r#"(mapcar (lambda (width)
                      (let ((tab-width width)) (list (char-width 9) (string-width "\t")
                            (format "%10s|" "\t") (format "%.4s|" "\t"))))
                    '(4 8 0 -3 1000 1001 2000))"#,
        expect_test::expect![[
            r###"OK ((4 4 "      	|" "	|") (8 8 "  	|" "|") (8 8 "  	|" "|") (8 8 "  	|" "|") (1000 1000 "	|" "|") (8 8 "  	|" "|") (8 8 "  	|" "|"))"###
        ]],
    );
}

#[test]
fn gdm_widths_dynamic_character_table() {
    parity(
        r#"(let ((char-width-table (copy-sequence char-width-table)))
               (set-char-table-range char-width-table ?é 2)
               (set-char-table-range char-width-table ?中 3)
               (set-char-table-range char-width-table ?a 9)
               (list (string-width "é中a") (char-width ?é) (char-width ?a)
                     (format "%3s|" "é") (format "%3c|" ?é) (format "%5S|" "é")
                     (format "%.2s|" "é中")
                     (progn (set-char-table-range char-width-table ?é -1)
                            (char-width ?é))
                     (progn (set-char-table-range char-width-table ?é 1001)
                            (char-width ?é))))"#,
        expect_test::expect![[r###"OK (6 2 1 " é|" " é|" " \"é\"|" "é|" 1000 1000)"###]],
    );
}

#[test]
fn gdm_widths_display_glyphs_use_character_policy() {
    parity(
        r#"(with-temp-buffer
               (setq buffer-display-table (make-display-table))
               (aset buffer-display-table ?a [?中 ?中])
               (aset buffer-display-table ?b [?\t ?\1])
               (let ((tab-width 4) (ctl-arrow nil))
                 (list (string-width "ab") (char-width ?a)
                       (format "%12s|" "ab"))))"#,
        expect_test::expect![[r###"OK (12 4 "ab|")"###]],
    );
}

#[test]
fn gdm_widths_representation_and_property_matrix() {
    parity(
        r#"(let ((ctl-arrow nil) (tab-width 4)
                   (snapshot (lambda (s)
                    (list (string-to-list s) (multibyte-string-p s)
                          (let ((i 0) properties)
                            (while (< i (length s))
                              (push (text-properties-at i s) properties)
                              (setq i (1+ i)))
                            (nreverse properties))))))
             (mapcar (lambda (s)
                       (list (multibyte-string-p s) (string-width s)
                             (funcall snapshot (format "%12s|" s))
                             (funcall snapshot (format "%.6s|" s))))
                     (list "a\1b" "\200\240" "é中" "a\t中"
                           (string-to-multibyte "abc")
                           (propertize (string-to-multibyte "abc") 'face 'italic)
                           (propertize "\200" 'face 'bold)
                           (propertize "é中" 'face 'italic)
                           (string-to-multibyte "\200\240"))))"#,
        expect_test::expect![[
            r###"OK ((nil 6 ((32 32 32 32 32 32 97 1 98 124) nil (nil nil nil nil nil nil nil nil nil nil)) ((97 1 98 124) nil (nil nil nil nil))) (nil 5 ((32 32 32 32 32 32 32 128 160 124) nil (nil nil nil nil nil nil nil nil nil nil)) ((128 160 124) nil (nil nil nil))) (t 3 ((32 32 32 32 32 32 32 32 32 233 20013 124) t (nil nil nil nil nil nil nil nil nil nil nil nil)) ((233 20013 124) t (nil nil nil))) (t 7 ((32 32 32 32 32 97 9 20013 124) t (nil nil nil nil nil nil nil nil nil)) ((97 9 124) t (nil nil nil))) (t 3 ((32 32 32 32 32 32 32 32 32 97 98 99 124) t (nil nil nil nil nil nil nil nil nil nil nil nil nil)) ((97 98 99 124) t (nil nil nil nil))) (t 3 ((32 32 32 32 32 32 32 32 32 97 98 99 124) t (nil nil nil nil nil nil nil nil nil (face italic) (face italic) (face italic) nil)) ((97 98 99 124) t ((face italic) (face italic) (face italic) nil))) (nil 4 ((32 32 32 32 32 32 32 32 128 124) nil (nil nil nil nil nil nil nil nil (face bold) nil)) ((128 124) nil ((face bold) nil))) (t 3 ((32 32 32 32 32 32 32 32 32 233 20013 124) t (nil nil nil nil nil nil nil nil nil (face italic) (face italic) nil)) ((233 20013 124) t ((face italic) (face italic) nil))) (t 8 ((32 32 32 32 4194176 4194208 124) t (nil nil nil nil nil nil nil)) ((4194176 124) t (nil nil))))"###
        ]],
    );
}

#[test]
fn gdm_widths_packed_and_cons_display_glyphs() {
    parity(
        r#"(with-temp-buffer
               (setq buffer-display-table (make-display-table))
               (aset buffer-display-table ?a (vector (+ ?中 (ash 1 22)) (cons ?é 2)))
               (aset buffer-display-table ?b (vector -1 'bad (cons ?中 -1)))
               (list (char-width ?a) (string-width "ab") (format "%4s|" "a")))"#,
        expect_test::expect![[r###"OK (3 3 " a|")"###]],
    );
}

#[test]
fn gdm_widths_bytecompiled_context_policy() {
    parity(
        r#"(let ((f (byte-compile '(lambda (s) (list (char-width 1) (string-width s) (format "%5s|" s)))))
                  (ctl-arrow nil) (tab-width 4))
               (let ((i 0) result)
                 (while (< i 200) (setq result (funcall f "\1") i (1+ i)))
                 result))"#,
        expect_test::expect![[r###"OK (4 4 " |")"###]],
    );
}

#[test]
fn gdm_widths_format_message_shared_context_policy() {
    parity(
        r#"(let ((ctl-arrow nil) (tab-width 4)
                  (char-width-table (copy-sequence char-width-table))
                  (snapshot (lambda (s)
                    (list (string-to-list s) (multibyte-string-p s)
                          (let ((i 0) properties)
                            (while (< i (length s))
                              (push (text-properties-at i s) properties)
                              (setq i (1+ i)))
                            (nreverse properties))))))
               (set-char-table-range char-width-table ?é 2)
               (mapcar snapshot
                 (list (format-message "%5s|" "\1")
                       (format-message "%6s|" "\t")
                       (format-message "%.4s|" "\tX")
                       (format-message "%3c|" ?é)
                       (format-message "%6S|" "é")
                       (format-message "%-5s|" (propertize "\200" 'face 'bold))
                       (format-message "%5s|" (string-to-multibyte "abc")))))"#,
        expect_test::expect![[
            r###"OK (((32 1 124) nil (nil nil nil)) ((32 32 9 124) nil (nil nil nil nil)) ((9 124) nil (nil nil)) ((32 233 124) t (nil nil nil)) ((32 32 34 233 34 124) t (nil nil nil nil nil nil)) ((128 32 124) nil ((face bold) (face bold) nil)) ((32 32 97 98 99 124) t (nil nil nil nil nil nil)))"###
        ]],
    );
}
