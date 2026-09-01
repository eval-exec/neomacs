//! Deep combo: match-data × replace-match × marker × overlay × undo ×
//! text-prop × narrow × regex × buffer-local.
//!
//! Stresses match data preservation and replacement: match-data must be
//! correctly saved/restored across function calls, replace-match must
//! update markers and overlays correctly, and undo must restore match
//! state. This is particularly tricky in a Rust rewrite because match
//! data is global state that must be preserved across callbacks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_match_data_preserved_across_let_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (search-failed \"\\\\([a-z]+\\\\):\\\\([0-9]+\\\\)\")""#]];
    // match-data must survive let-binding of dynamic variables.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "aaa:111 bbb:222 ccc:333")
  (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)")
  (let ((saved-match (match-data)))
    (let ((inhibit-quit t)
          (last-command 'test))
      ;; match-data should still be valid inside let
      (list (match-string 1)
            (match-string 2)
            (equal (match-data) saved-match)))
    ;; After let, match-data should still be valid
    (list (match-string 1)
          (match-string 2)
          (equal (match-data) saved-match)))) "#,
        expect,
    );
}

#[test]
fn combo_match_data_preserved_across_re_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (search-failed \"\\\\([a-z]+\\\\):\\\\([0-9]+\\\\)\")""#]];
    // Outer match data preserved when inner re-search fails.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "alpha:100 beta:200")
  (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)")
  (let ((outer-match (list (match-string 1) (match-string 2))))
    (save-excursion
      (re-search-forward "NONEXISTENT" nil t))
    ;; Outer match data should be preserved after failed search
    (list outer-match
          (match-string 1)
          (match-string 2)))) "#,
        expect,
    );
}

#[test]
fn combo_replace_match_marker_overlay_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((#(\"hello UNIVERSE hello UNIVERSE hello\" 0 5 (word hello1) 15 20 (word hello2)) 7 22 1 15 16 31 nil hello2) (#(\"hello WORLD hello WORLD hello\" 0 5 (word hello1) 6 11 (word world1) 12 17 (word hello2) 18 23 (word world2)) 7 24 1 12 13 25 world1 world2))""#
    ]];
    // replace-match with fixedcase and literal, markers/overlays track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "hello WORLD hello WORLD hello")
  (let ((m1 (copy-marker 7 nil))
        (m2 (copy-marker 19 t))
        (ov1 (make-overlay 1 12))
        (ov2 (make-overlay 13 25)))
    (overlay-put ov1 'part 'first)
    (overlay-put ov2 'part 'second)
    (put-text-property 1 6 'word 'hello1)
    (put-text-property 7 12 'word 'world1)
    (put-text-property 13 18 'word 'hello2)
    (put-text-property 19 24 'word 'world2)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "WORLD" nil t)
      (replace-match "UNIVERSE" t t))
    (let ((after-replace (list (buffer-string)
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov1) (overlay-end ov1)
                               (overlay-start ov2) (overlay-end ov2)
                               (get-text-property 7 'word)
                               (get-text-property 19 'word))))
      (primitive-undo 1 buffer-undo-list)
      (let ((after-undo (list (buffer-string)
                              (marker-position m1)
                              (marker-position m2)
                              (overlay-start ov1) (overlay-end ov1)
                              (overlay-start ov2) (overlay-end ov2)
                              (get-text-property 7 'word)
                              (get-text-property 19 'word))))
        (list after-replace after-undo))))) "#,
        expect,
    );
}

#[test]
fn combo_replace_match_shorter_markers_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((#(\"AAA-X-BBB-X-CCC\" 0 3 (sect a) 6 9 (sect b) 12 15 (sect c)) 4 10 12 5 11 nil nil) (#(\"AAA-XXXXXXX-BBB-XXXXXXX-CCC\" 0 3 (sect a) 4 11 (sect x1) 12 15 (sect b) 16 23 (sect x2) 24 27 (sect c)) 4 16 17 5 17 nil x1))""#
    ]];
    // Replace with shorter string; markers after match must retreat.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-XXXXXXX-BBB-XXXXXXX-CCC")
  (let ((m-a (copy-marker 4 nil))
        (m-b (copy-marker 16 t))
        (m-c (copy-marker 24 nil))
        (ov (make-overlay 5 23)))
    (overlay-put ov 'span 'middle)
    (put-text-property 1 4 'sect 'a)
    (put-text-property 5 12 'sect 'x1)
    (put-text-property 13 16 'sect 'b)
    (put-text-property 17 24 'sect 'x2)
    (put-text-property 25 28 'sect 'c)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "XXXXXXX" nil t)
      (replace-match "X"))
    (let ((after (list (buffer-string)
                       (marker-position m-a)
                       (marker-position m-b)
                       (marker-position m-c)
                       (overlay-start ov) (overlay-end ov)
                       (get-text-property 4 'sect)
                       (get-text-property 5 'sect))))
      (primitive-undo 1 buffer-undo-list)
      (let ((restored (list (buffer-string)
                            (marker-position m-a)
                            (marker-position m-b)
                            (marker-position m-c)
                            (overlay-start ov) (overlay-end ov)
                            (get-text-property 4 'sect)
                            (get-text-property 5 'sect))))
        (list after restored))))) "#,
        expect,
    );
}

#[test]
fn combo_match_data_with_group_replacement_and_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 31 40)""#]];
    // Group capture + backreference replacement in narrowed buffer.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "alpha:100 beta:200 gamma:300 delta:400")
  (let ((m1 (copy-marker 10 nil))
        (m2 (copy-marker 20 t))
        (ov (make-overlay 11 30)))
    (overlay-put ov 'scope 'middle)
    (put-text-property 1 10 'group 'g1)
    (put-text-property 11 20 'group 'g2)
    (put-text-property 21 30 'group 'g3)
    (put-text-property 31 40 'group 'g4)
    (undo-boundary)
    (narrow-to-region 11 30)
    (goto-char (point-min))
    (while (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" nil t)
      (replace-match "\\1=\\2" t))
    (let ((narrowed (buffer-string))
          (match-after (list (match-string 1) (match-string 2))))
      (widen)
      (let ((full (buffer-string))
            (m1-pos (marker-position m1))
            (m2-pos (marker-position m2))
            (ov-range (list (overlay-start ov) (overlay-end ov))))
        (primitive-undo 1 buffer-undo-list)
        (let ((restored (buffer-string))
              (m1-restored (marker-position m1))
              (m2-restored (marker-position m2)))
          (list narrowed match-after full m1-pos m2-pos ov-range
                restored m1-restored m2-restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_match_data_replace_loop_with_overlay_evaporate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"XX XX XX XX XX XX\" ((1 3 \"AA\") (4 7 \"CC\") (9 10 \"EE\"))) (\"AA BB CC DD EE FF\" ((7 10 \"EE\"))))""#
    ]];
    // Loop replace with evaporate overlays; some should vanish.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AA BB CC DD EE FF")
  (let ((ovs nil))
    (dolist (pair '(("AA" . 1) ("CC" . 5) ("EE" . 9)))
      (let ((ov (make-overlay (cdr pair) (+ (cdr pair) 2))))
        (overlay-put ov 'tag (car pair))
        (overlay-put ov 'evaporate t)
        (push ov ovs)))
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "[A-Z][A-Z]" nil t)
      (replace-match "XX"))
    (let ((after (list (buffer-string)
                       (mapcar (lambda (ov)
                                 (and (overlay-start ov)
                                      (list (overlay-start ov)
                                            (overlay-end ov)
                                            (overlay-get ov 'tag))))
                               (nreverse ovs)))))
      (primitive-undo 1 buffer-undo-list)
      (let ((restored (list (buffer-string)
                            (mapcar (lambda (ov)
                                      (and (overlay-start ov)
                                           (list (overlay-start ov)
                                                 (overlay-end ov)
                                                 (overlay-get ov 'tag))))
                                    (nreverse ovs)))))
        (list after restored))))) "#,
        expect,
    );
}
