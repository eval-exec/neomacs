//! GNU parity for the character/Emacs-byte conversion index.
//!
//! GNU `buf_charpos_to_bytepos` and `buf_bytepos_to_charpos` (`src/marker.c`)
//! interpolate single-byte spans or scan between known positions; markers
//! supply internal anchors. `Fbyte_to_position` (`src/editfns.c`) first backs
//! continuation bytes up to their character head, and both conversion
//! builtins use the full buffer even while narrowed. Expectations are
//! refreshed from GNU 31.1 with NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_text_index_conversion_character_and_continuation_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(mapcar
 (lambda (multibyte)
   (with-temp-buffer
     (set-buffer-multibyte multibyte)
     (insert (if multibyte
                 (concat "aé中😀\n"
                         (string-to-multibyte (unibyte-string 128 255)) "z")
               (unibyte-string 97 233 128 255 10 122)))
     (line-number-at-pos (point-max))
     (list
      (mapcar (lambda (p)
                (list p (position-bytes p) (char-after p) (char-before p)
                      (save-excursion
                        (goto-char p)
                        (list (point) (char-after) (char-before)))))
              (number-sequence 0 (1+ (point-max))))
      (mapcar (lambda (b) (list b (byte-to-position b)))
              (number-sequence 0 (1+ (position-bytes (point-max))))))))
 '(t nil))
"#;
    let expect = expect_test::expect![[
        r#""OK ((((0 nil nil nil (1 97 nil)) (1 1 97 nil (1 97 nil)) (2 2 233 97 (2 233 97)) (3 4 20013 233 (3 20013 233)) (4 7 128512 20013 (4 128512 20013)) (5 11 10 128512 (5 10 128512)) (6 12 4194176 10 (6 4194176 10)) (7 14 4194303 4194176 (7 4194303 4194176)) (8 16 122 4194303 (8 122 4194303)) (9 17 nil 122 (9 nil 122)) (10 nil nil nil (9 nil 122))) ((0 nil) (1 1) (2 2) (3 2) (4 3) (5 3) (6 3) (7 4) (8 4) (9 4) (10 4) (11 5) (12 6) (13 6) (14 7) (15 7) (16 8) (17 9) (18 nil))) (((0 nil nil nil (1 97 nil)) (1 1 97 nil (1 97 nil)) (2 2 233 97 (2 233 97)) (3 3 128 233 (3 128 233)) (4 4 255 128 (4 255 128)) (5 5 10 255 (5 10 255)) (6 6 122 10 (6 122 10)) (7 7 nil 122 (7 nil 122)) (8 nil nil nil (7 nil 122))) ((0 nil) (1 1) (2 2) (3 3) (4 4) (5 5) (6 6) (7 7) (8 nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_index_conversion_narrowed_positions_and_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (dotimes (_ 9000) (insert "aé中😀\n"))
  (let* ((before (copy-marker 16385))
         (after (copy-marker 16385 t))
         (outside (copy-marker 8191))
         (end (copy-marker (point-max)))
         (positions (list 1 8191 16383 16384 16385 16386 32769 (point-max)))
         (probe (lambda ()
                  (line-number-at-pos (point-max))
                  (list
                   (mapcar (lambda (p)
                             (let ((b (position-bytes p)))
                               (list p b (and b (byte-to-position b))
                                     (and b (byte-to-position (1+ b)))
                                     (char-after p) (char-before p)
                                     (save-excursion (goto-char p) (point)))))
                           positions)
                   (mapcar (lambda (m)
                             (list (marker-position m) (position-bytes m)
                                   (char-after m) (char-before m)))
                           (list before after outside end))))))
    (let ((initial (funcall probe)))
      (narrow-to-region 12000 35000)
      (let ((narrowed (funcall probe)))
        (goto-char before)
        (insert "Ω\n")
        (let ((inserted (funcall probe)))
          (widen)
          (list initial narrowed inserted (funcall probe)))))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((((1 1 1 2 97 nil 1) (8191 18019 8191 8192 97 10 8191) (16383 36040 16383 16383 20013 233 16383) (16384 36043 16384 16384 128512 20013 16384) (16385 36047 16385 16386 10 128512 16385) (16386 36048 16386 16387 97 10 16386) (32769 72090 32769 32769 128512 20013 32769) (45001 99001 45001 nil nil 10 45001)) ((16385 36047 10 128512) (16385 36047 10 128512) (8191 18019 97 10) (45001 99001 nil 10))) (((1 1 1 2 nil nil 12000) (8191 18019 8191 8192 nil nil 12000) (16383 36040 16383 16383 20013 233 16383) (16384 36043 16384 16384 128512 20013 16384) (16385 36047 16385 16386 10 128512 16385) (16386 36048 16386 16387 97 10 16386) (32769 72090 32769 32769 128512 20013 32769) (45001 99001 45001 nil nil nil 35000)) ((16385 36047 10 128512) (16385 36047 10 128512) (8191 18019 nil nil) (45001 99001 nil nil))) (((1 1 1 2 nil nil 12000) (8191 18019 8191 8192 nil nil 12000) (16383 36040 16383 16383 20013 233 16383) (16384 36043 16384 16384 128512 20013 16384) (16385 36047 16385 16385 937 128512 16385) (16386 36049 16386 16387 10 937 16386) (32769 72088 32769 32769 233 97 32769) (45001 98999 45001 45001 nil nil 35002)) ((16385 36047 937 128512) (16387 36050 10 10) (8191 18019 nil nil) (45003 99004 nil nil))) (((1 1 1 2 97 nil 1) (8191 18019 8191 8192 97 10 8191) (16383 36040 16383 16383 20013 233 16383) (16384 36043 16384 16384 128512 20013 16384) (16385 36047 16385 16385 937 128512 16385) (16386 36049 16386 16387 10 937 16386) (32769 72088 32769 32769 233 97 32769) (45001 98999 45001 45001 128512 20013 45001)) ((16385 36047 937 128512) (16387 36050 10 10) (8191 18019 97 10) (45003 99004 nil 10))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_index_conversion_gap_edits_and_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (dotimes (_ 9000)
    (insert "aé中😀\n" (string-to-multibyte (unibyte-string 255))))
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (let ((probe (lambda ()
                 (line-number-at-pos (point-max))
                 (list (buffer-size) (gap-position)
                       (mapcar (lambda (p)
                                 (let ((b (position-bytes p)))
                                   (list p b (and b (byte-to-position b))
                                         (and b (byte-to-position (1+ b)))
                                         (char-after p) (char-before p))))
                               (list 1 8191 16384 16385 16386 32769
                                     (1- (point-max)) (point-max))))))
        results)
    (setq results (list (funcall probe)))
    (goto-char 16385)
    (insert "Ω\n" (string-to-multibyte (unibyte-string 128)))
    (push (funcall probe) results)
    (primitive-undo 1 buffer-undo-list)
    (push (funcall probe) results)
    (setq buffer-undo-list nil)
    (delete-region 32760 32781)
    (push (funcall probe) results)
    (primitive-undo 1 buffer-undo-list)
    (push (funcall probe) results)
    (nreverse results)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((54000 54001 ((1 1 1 2 97 nil) (8191 17746 8191 8192 97 4194303) (16384 35497 16384 16384 128512 20013) (16385 35501 16385 16386 10 128512) (16386 35502 16386 16386 4194303 10) (32769 70997 32769 32769 20013 233) (54000 116999 54000 54000 4194303 10) (54001 117001 54001 nil nil 4194303))) (54003 16388 ((1 1 1 2 97 nil) (8191 17746 8191 8192 97 4194303) (16384 35497 16384 16384 128512 20013) (16385 35501 16385 16385 937 128512) (16386 35503 16386 16387 10 937) (32769 70997 32769 32769 4194303 10) (54003 117004 54003 54003 4194303 10) (54004 117006 54004 nil nil 4194303))) (54000 16385 ((1 1 1 2 97 nil) (8191 17746 8191 8192 97 4194303) (16384 35497 16384 16384 128512 20013) (16385 35501 16385 16386 10 128512) (16386 35502 16386 16386 4194303 10) (32769 70997 32769 32769 20013 233) (54000 116999 54000 54000 4194303 10) (54001 117001 54001 nil nil 4194303))) (53979 32760 ((1 1 1 2 97 nil) (8191 17746 8191 8192 97 4194303) (16384 35497 16384 16384 128512 20013) (16385 35501 16385 16386 10 128512) (16386 35502 16386 16386 4194303 10) (32769 71000 32769 32769 4194303 10) (53979 116955 53979 53979 4194303 10) (53980 116957 53980 nil nil 4194303))) (54000 32781 ((1 1 1 2 97 nil) (8191 17746 8191 8192 97 4194303) (16384 35497 16384 16384 128512 20013) (16385 35501 16385 16386 10 128512) (16386 35502 16386 16386 4194303 10) (32769 70997 32769 32769 20013 233) (54000 116999 54000 54000 4194303 10) (54001 117001 54001 nil nil 4194303))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_text_index_conversion_insert_file_contents_and_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((file (make-temp-file "neovm-text-convert-gnu")))
  (unwind-protect
      (progn
        (let ((coding-system-for-write 'utf-8-unix))
          (with-temp-file file (insert "é\r\n中\n😀\nlast")))
        (with-temp-buffer
          (dotimes (_ 9000) (insert "aé中😀\n"))
          (let ((probe (lambda ()
                         (line-number-at-pos (point-max))
                         (list (buffer-size) (point-min) (point-max)
                               (mapcar (lambda (p)
                                         (let ((b (position-bytes p)))
                                           (list p b (and b (byte-to-position b))
                                                 (and b (byte-to-position (1+ b)))
                                                 (char-after p) (char-before p))))
                                       (list 1 8191 16384 16385 16386 32769
                                             (1- (point-max)) (point-max))))))
                (coding-system-for-read 'utf-8-unix)
                results)
            (setq results (list (funcall probe)))
            (goto-char 16385)
            (push (cadr (insert-file-contents file)) results)
            (push (funcall probe) results)
            (goto-char 32769)
            (push (cadr (insert-file-contents file nil 2 11)) results)
            (push (funcall probe) results)
            (narrow-to-region 12000 35000)
            (funcall probe)
            (push (cadr (insert-file-contents file nil nil nil t)) results)
            (push (funcall probe) results)
            (widen)
            (push (funcall probe) results)
            (nreverse results))))
    (delete-file file)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((45000 1 45001 ((1 1 1 2 97 nil) (8191 18019 8191 8192 97 10) (16384 36043 16384 16384 128512 20013) (16385 36047 16385 16386 10 128512) (16386 36048 16386 16387 97 10) (32769 72090 32769 32769 128512 20013) (45000 99000 45000 45001 10 128512) (45001 99001 45001 nil nil 10))) 11 (45011 1 45012 ((1 1 1 2 97 nil) (8191 18019 8191 8192 97 10) (16384 36043 16384 16384 128512 20013) (16385 36047 16385 16385 233 128512) (16386 36049 16386 16387 13 233) (32769 72082 32769 32769 20013 233) (45011 99017 45011 45012 10 128512) (45012 99018 45012 nil nil 10))) 7 (45018 1 45019 ((1 1 1 2 97 nil) (8191 18019 8191 8192 97 10) (16384 36043 16384 16384 128512 20013) (16385 36047 16385 16385 233 128512) (16386 36049 16386 16387 13 233) (32769 72082 32769 32770 13 233) (45018 99029 45018 45019 10 128512) (45019 99030 45019 nil nil 10))) 11 (22029 12000 12011 ((1 1 1 2 nil nil) (8191 18019 8191 8192 nil nil) (16384 36040 16384 16385 nil nil) (16385 36041 16385 16386 nil nil) (16386 36042 16386 16386 nil nil) (32769 nil nil nil nil nil) (12010 26416 12010 12011 116 115) (12011 26417 12011 12011 nil 116))) (22029 1 22030 ((1 1 1 2 97 nil) (8191 18019 8191 8192 97 10) (16384 36040 16384 16385 10 128512) (16385 36041 16385 16386 97 10) (16386 36042 16386 16386 233 97) (32769 nil nil nil nil nil) (22029 48459 22029 22030 10 128512) (22030 48460 22030 nil nil 10))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
