//! GNU casefiddle.c:480-518; insdel.c:1786-1803 regression coverage.
mod common;
use common::{run_neovm_eval, run_oracle_eval};

fn parity(form: &str, expected: expect_test::Expect) {
    let gnu = run_oracle_eval(form).expect("GNU 31.1 oracle");
    expected.assert_eq(&gnu);
    if std::env::var_os("UPDATE_EXPECT").is_none() {
        assert_eq!(
            run_neovm_eval(form).expect("neovm evaluation"),
            gnu,
            "{form}"
        );
    }
}

#[test]
fn gdm_region_expansion_boundaries_and_point() {
    parity(
        r#"(mapcar (lambda (s) (mapcar (lambda (p) (with-temp-buffer (insert s) (goto-char p) (upcase-region 1 (point-max)) (list (buffer-string) (point) (point-max) (buffer-size) (buffer-substring (point-min) (point-max)) (progn (goto-char (point-max)) (insert "!") (buffer-string))))) '(1 2 3))) '("ßx" "ﬃx" "ŉx"))"#,
        expect_test::expect![[
            r###"OK ((("SSX" 1 4 3 "SSX" "SSX!") ("SSX" 3 4 3 "SSX" "SSX!") ("SSX" 4 4 3 "SSX" "SSX!")) (("FFIX" 1 5 4 "FFIX" "FFIX!") ("FFIX" 4 5 4 "FFIX" "FFIX!") ("FFIX" 5 5 4 "FFIX" "FFIX!")) (("ʼNX" 1 4 3 "ʼNX" "ʼNX!") ("ʼNX" 3 4 3 "ʼNX" "ʼNX!") ("ʼNX" 4 4 3 "ʼNX" "ʼNX!")))"###
        ]],
    );
}

#[test]
fn gdm_region_dotted_i_point_and_narrowing() {
    parity(
        r#"(list (mapcar (lambda (p) (with-temp-buffer (insert "İxy") (goto-char p) (downcase-region 1 3) (list (point) (point-max) (buffer-string)))) '(1 2 3 4)) (with-temp-buffer (insert "zßxy") (narrow-to-region 2 4) (goto-char 3) (upcase-region 2 4) (list (point-min) (point-max) (point) (buffer-string))))"#,
        expect_test::expect![[
            r###"OK (((1 5 "i̇xy") (3 5 "i̇xy") (4 5 "i̇xy") (5 5 "i̇xy")) (2 5 4 "SSX"))"###
        ]],
    );
}

#[test]
fn gdm_region_marker_overlay_positions() {
    parity(
        r#"(mapcar (lambda (s) (with-temp-buffer (insert s) (let ((a (copy-marker 2)) (b (copy-marker 3 t)) (o (make-overlay 2 3))) (if (eq (aref s 0) ?İ) (downcase-region 1 3) (upcase-region 1 3)) (list (buffer-string) (marker-position a) (marker-position b) (overlay-start o) (overlay-end o) (progn (goto-char a) (following-char)))))) '("İxy" "ßxy" "ﬃxy" "ŉxy" "ẞxy"))"#,
        expect_test::expect![[
            r###"OK (("i̇xy" 2 3 2 3 775) ("SSXy" 2 3 2 3 83) ("FFIXy" 2 3 2 3 70) ("ʼNXy" 2 3 2 3 78) ("ẞXy" 2 3 2 3 88))"###
        ]],
    );
}

#[test]
fn gdm_region_properties_follow_each_expansion() {
    parity(
        r#"(mapcar (lambda (s) (with-temp-buffer (insert s) (upcase-region 1 (point-max)) (buffer-string))) (list (concat "a" (propertize "ß" 'face 'bold) "b") (concat (propertize "a" 'face 'italic) "ßb") (propertize "aßb" 'face 'bold) (concat "ß" (propertize "ß" 'face 'bold) "c") (concat (propertize "a" 'face 'italic) "ß" (propertize "c" 'face 'bold))))"#,
        expect_test::expect![[
            r###"OK (#("ASSB" 2 3 (face bold)) #("ASSB" 0 2 (face italic)) #("ASSB" 0 4 (face bold)) #("SSSSC" 3 4 (face bold)) #("ASSC" 0 2 (face italic) 3 4 (face bold)))"###
        ]],
    );
}

// GNU insdel.c:1800 delegates casing growth to offset_intervals;
// intervals.c:837-958,1024-1150 inherits at interiors and sticky boundaries.
#[test]
fn gdm_region_expansion_interval_stickiness() {
    parity(
        r#"(let ((cat 'gdm-casing-sticky-category))
  (put cat 'front-sticky t)
  (put cat 'face 'underline)
  (list
   (mapcar
    (lambda (spec)
      (with-temp-buffer
        (let ((text-property-default-nonsticky (nth 2 spec))
              (char-property-alias-alist (nth 3 spec))
              (default-text-properties (nth 4 spec)))
          (insert (nth 1 spec))
          (upcase-region 1 (point-max))
          (list (car spec) (buffer-string)))))
    (list
     (list 'interior (propertize "aßb" 'face 'bold))
     (list 'left-boundary (concat (propertize "a" 'face 'italic) "ßb"))
     (list 'start-boundary (propertize "ßb" 'face 'bold))
     (list 'front-boundary (concat "a" (propertize "ßb" 'face 'bold 'front-sticky '(face))))
     (list 'rear-boundary (concat (propertize "a" 'face 'italic 'rear-nonsticky '(face)) "ßb"))
     (list 'front-rear-boundary
           (concat (propertize "a" 'face 'italic 'rear-nonsticky '(face))
                   (propertize "ßb" 'face 'bold 'front-sticky '(face))))
     (list 'front-overrides-default (propertize "aßb" 'face 'bold 'front-sticky t 'rear-nonsticky '(face)) '((face . t)))
     (list 'all-rear-splits (propertize "aßb" 'face 'bold 'front-sticky t 'rear-nonsticky t))
     (list 'default-rear (propertize "aßb" 'face 'bold) '((face . t)))
     (list 'default-front (propertize "ßb" 'face 'bold) '((face . nil)))
     (list 'category-front (concat "a" (propertize "ßb" 'category cat)))
     (list 'category-left-priority (concat (propertize "a" 'face 'italic) (propertize "ßb" 'face 'bold 'category cat)))
     (list 'alias-front (concat "a" (propertize "ßb" 'face 'bold 'gdm-front t)) nil '((front-sticky gdm-front)))
     (list 'default-front-property (propertize "ßb" 'face 'bold) nil nil '(front-sticky t))))
   (with-temp-buffer
     (insert (propertize "aßb" 'face 'bold))
     (add-hook 'before-change-functions
               (lambda (b e) (setq-local text-property-default-nonsticky '((face . t)))) nil t)
     (upcase-region 1 (point-max))
     (list 'hook-policy (buffer-string)))
   (with-temp-buffer
     (insert (propertize "aßzz" 'face 'bold))
     (add-hook 'before-change-functions (lambda (b e) (narrow-to-region 1 2)) nil t)
     (upcase-region 1 5)
     (widen)
     (list 'physical-past-zv (buffer-string)))))"#,
        expect_test::expect![[
            r###"OK (((interior #("ASSB" 0 4 (face bold))) (left-boundary #("ASSB" 0 2 (face italic))) (start-boundary #("SSB" 1 3 (face bold))) (front-boundary #("ASSB" 1 2 (front-sticky (face) face bold) 2 4 (front-sticky (face) face bold))) (rear-boundary #("ASSB" 0 1 (rear-nonsticky (face) face italic))) (front-rear-boundary #("ASSB" 0 1 (rear-nonsticky (face) face italic) 1 2 (front-sticky (face) face bold) 2 4 (front-sticky (face) face bold))) (front-overrides-default #("ASSB" 0 4 (face bold front-sticky t rear-nonsticky (face)))) (all-rear-splits #("ASSB" 0 1 (face bold front-sticky t rear-nonsticky t) 1 2 (front-sticky (face) rear-nonsticky (face) face bold) 2 4 (face bold front-sticky t rear-nonsticky t))) (default-rear #("ASSB" 0 1 (face bold) 2 4 (face bold))) (default-front #("SSB" 0 3 (face bold))) (category-front #("ASSB" 1 4 (category gdm-casing-sticky-category))) (category-left-priority #("ASSB" 0 1 (face italic) 1 2 (category gdm-casing-sticky-category face italic) 2 4 (category gdm-casing-sticky-category face bold))) (alias-front #("ASSB" 1 2 (front-sticky (gdm-front face) gdm-front t face bold) 2 4 (gdm-front t face bold))) (default-front-property #("SSB" 0 1 (front-sticky (face) face bold) 1 3 (face bold)))) (hook-policy #("ASSB" 0 1 (face bold) 2 4 (face bold))) (physical-past-zv #("ASSZZ" 0 5 (face bold))))"###
        ]],
    );
}

#[test]
fn gdm_region_capitalize_and_words_expand_consistently() {
    parity(
        r#"(mapcar (lambda (op) (with-temp-buffer (insert (propertize "ßx ﬃx" 'face 'bold)) (goto-char 1) (let ((m (copy-marker 2)) (o (make-overlay 2 3))) (if (memq op '(capitalize-word upcase-word downcase-word)) (funcall op 2) (funcall op 1 (point-max))) (list (buffer-string) (point) (point-max) (marker-position m) (overlay-start o) (overlay-end o))))) '(upcase-region capitalize-region upcase-initials-region upcase-word capitalize-word downcase-word))"#,
        expect_test::expect![[
            r###"OK ((#("SSX FFIX" 1 8 (face bold)) 1 9 2 2 3) (#("Ssx Ffix" 1 8 (face bold)) 1 9 2 2 3) (#("Ssx Ffix" 1 8 (face bold)) 1 9 2 2 3) (#("SSX FFIX" 1 8 (face bold)) 9 9 2 2 3) (#("Ssx Ffix" 1 8 (face bold)) 9 9 2 2 3) (#("ßx ﬃx" 0 5 (face bold)) 6 6 2 2 3))"###
        ]],
    );
}

#[test]
fn gdm_region_equal_aggregate_extents_keep_interior_anchors() {
    parity(
        r#"(with-temp-buffer (let ((ct (copy-sequence (standard-case-table)))) (set-case-table ct) (let ((up (char-table-extra-slot ct 0))) (aset up ?a ?é) (aset up ?é ?A)) (insert "aéx") (goto-char 2) (let ((m (copy-marker 2)) (o (make-overlay 2 3))) (upcase-region 1 3) (list (buffer-string) (point) (point-max) (marker-position m) (overlay-start o) (overlay-end o) (char-after m) (progn (goto-char m) (insert "!") (buffer-string))))))"#,
        expect_test::expect![[r###"OK ("éAx" 2 4 2 2 3 65 "é!Ax")"###]],
    );
}

#[test]
fn gdm_region_indirect_buffers_keep_character_positions() {
    parity(
        r#"(with-temp-buffer (insert "ßxy") (let ((ind (make-indirect-buffer (current-buffer) " *gdm indirect*" t))) (unwind-protect (progn (with-current-buffer ind (goto-char 3)) (goto-char 2) (let ((m (with-current-buffer ind (copy-marker 2))) (o (with-current-buffer ind (make-overlay 2 3)))) (upcase-region 1 3) (list (list (point) (point-max) (buffer-string)) (with-current-buffer ind (list (point) (point-max) (buffer-string) (marker-position m) (overlay-start o) (overlay-end o) (char-after m)))))) (kill-buffer ind))))"#,
        expect_test::expect![[r###"OK ((3 5 "SSXy") (3 4 "SSX" 2 2 3 83))"###]],
    );
}

// GNU casefiddle.c:540-541 prepares modification hooks before the casing
// context, source and byte anchors; 667 sets word point after after hooks.
#[test]
fn gdm_region_hooks_precede_source_and_context_measurement() {
    parity(
        r#"(list (mapcar (lambda (op) (with-temp-buffer (insert "ab") (goto-char 1) (add-hook 'before-change-functions (lambda (b e) (goto-char 1) (insert "é")) nil t) (if (eq op 'upcase-word) (funcall op 1) (funcall op 1 3)) (list (buffer-string) (point) (point-max)))) '(upcase-region upcase-word)) (with-temp-buffer (insert "abc") (add-hook 'before-change-functions (lambda (b e) (set-case-table (make-char-table 'case-table))) nil t) (upcase-region 1 4) (buffer-string)) (with-temp-buffer (insert "a_Σ") (add-hook 'before-change-functions (lambda (b e) (modify-syntax-entry ?_ "w")) nil t) (capitalize-region 1 4) (buffer-string)) (with-temp-buffer (insert "éa") (add-hook 'before-change-functions (lambda (b e) (set-buffer-multibyte nil)) nil t) (upcase-region 1 3) (list (string-to-list (buffer-string)) (point-max))) (with-temp-buffer (insert "ab") (goto-char 1) (add-hook 'after-change-functions (lambda (b e o) (goto-char 1) (insert "é")) nil t) (upcase-word 1) (list (buffer-string) (point) (point-max))) (mapcar (lambda (bounds) (with-temp-buffer (insert "abcd") (goto-char 1) (add-hook 'before-change-functions (lambda (b e) (narrow-to-region (car bounds) (cdr bounds))) nil t) (let ((outcome (condition-case e (progn (upcase-region 1 5) 'ok) (error (car e))))) (list outcome (point-min) (point-max) (point) (buffer-string) (progn (widen) (buffer-string)))))) '((2 . 3) (1 . 3) (2 . 5))))"#,
        expect_test::expect![[
            r###"OK ((("ÉAb" 2 4) ("ÉAb" 3 4)) "abc" "A_ς" ((195 169 97) 4) ("éAB" 3 4) ((args-out-of-range 2 3 2 "b" "abcd") (ok 1 3 1 "AB" "ABCD") (args-out-of-range 2 5 2 "bcd" "abcd")))"###
        ]],
    );
}

// GNU insdel.c:1738-1745,1764-1776 grows logical ZV even when a before
// hook narrows the expansion outside it. Character-based snapshots avoid
// inspecting GNU's inconsistent raw ZV_BYTE in the first two cases.
#[test]
fn gdm_region_hook_narrowing_retains_physical_expansion_bounds() {
    parity(
        r#"(mapcar (lambda (spec) (with-temp-buffer (insert "aßzz") (goto-char 1) (let ((lower (nth 1 spec)) (upper (nth 2 spec))) (add-hook 'before-change-functions (lambda (b e) (narrow-to-region lower upper)) nil t) (let ((outcome (condition-case err (progn (upcase-region (nth 3 spec) (nth 4 spec)) 'ok) (error (car err))))) (list (car spec) outcome (point-min) (point-max) (point) (buffer-substring-no-properties (point-min) (point-max)) (progn (widen) (buffer-substring-no-properties (point-min) (point-max)))))))) '((zv-before 1 1 1 5) (zv-at 1 2 1 5) (zv-after 1 3 1 5) (begv-before 1 5 2 5) (begv-at 2 5 2 5) (begv-after 3 5 2 5)))"#,
        expect_test::expect![[
            r###"OK ((zv-before ok 1 2 1 "A" "ASSZZ") (zv-at ok 1 3 1 "AS" "ASSZZ") (zv-after ok 1 4 1 "ASS" "ASSZZ") (begv-before ok 1 6 1 "aSSZZ" "aSSZZ") (begv-at ok 2 6 2 "SSZZ" "aSSZZ") (begv-after args-out-of-range 3 5 3 "zz" "aßzz"))"###
        ]],
    );
}

// GNU intervals.c:1143-1149 and lisp.h:1316-1324 use mode-dependent EQ
// for category front-stickiness; data.c:876-895 constructs positioned t.
#[test]
fn gdm_region_category_front_sticky_uses_positioned_symbol_equality() {
    parity(
        r#"(mapcar (lambda (mode) (let ((symbols-with-pos-enabled mode)) (mapcar (lambda (kind) (with-temp-buffer (let ((cat 'gdm-casing-positioned-front-category)) (put cat 'front-sticky (if (eq kind 'positioned) (position-symbol t 77) t)) (insert "aßz") (add-text-properties 2 4 (list 'face 'bold 'category cat)) (upcase-region 1 4) (list mode kind (buffer-substring-no-properties 1 (point-max)) (mapcar (lambda (p) (list p (plist-get (text-properties-at p) 'front-sticky) (plist-get (text-properties-at p) 'category) (next-property-change p nil (point-max)))) '(1 2 3 4)))))) '(bare positioned)))) '(nil t))"#,
        expect_test::expect![[
            r###"OK (((nil bare "ASSZ" ((1 nil nil 2) (2 nil gdm-casing-positioned-front-category 5) (3 nil gdm-casing-positioned-front-category 5) (4 nil gdm-casing-positioned-front-category 5))) (nil positioned "ASSZ" ((1 nil nil 2) (2 (category face) gdm-casing-positioned-front-category 3) (3 nil gdm-casing-positioned-front-category 5) (4 nil gdm-casing-positioned-front-category 5)))) ((t bare "ASSZ" ((1 nil nil 2) (2 nil gdm-casing-positioned-front-category 5) (3 nil gdm-casing-positioned-front-category 5) (4 nil gdm-casing-positioned-front-category 5))) (t positioned "ASSZ" ((1 nil nil 2) (2 nil gdm-casing-positioned-front-category 5) (3 nil gdm-casing-positioned-front-category 5) (4 nil gdm-casing-positioned-front-category 5)))))"###
        ]],
    );
}
