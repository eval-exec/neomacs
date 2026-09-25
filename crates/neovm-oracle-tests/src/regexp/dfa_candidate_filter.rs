//! GNU parity of searches under the existence DFA candidate filter (P3.3).
//!
//! Every form searches texts dense with candidates that fail, repeatedly, so
//! a pattern's DFA is built (after 16 failed matcher entries) and then filters
//! candidates.  Neomacs runs each form with `NEOVM_REGEX_DFA` off, on and
//! verify; all three must equal GNU's answer (expectations from
//! `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1`).  `(match-data t)` is
//! compared in full after every search.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// The filter's three modes.
const DFA_MODES: &[&[(&str, &str)]] = &[
    &[("NEOVM_REGEX_DFA", "off")],
    &[("NEOVM_REGEX_DFA", "on")],
    &[("NEOVM_REGEX_DFA", "verify")],
];

/// Lisp helpers shared by the forms: every match of a forward and a backward
/// search, with the match data after each, three times over.
const HELPERS: &str = r#"
(defun dfa-probe--runs (search)
  ;; SEARCH three times: whether the runs agree, and the last run's result
  ;; (by then the filter has built the pattern's DFA).
  (let* ((r1 (funcall search)) (r2 (funcall search)) (r3 (funcall search)))
    (list (equal r1 r2) (equal r2 r3) r3)))
(defun dfa-probe--forward (re &optional bound posix)
  (dfa-probe--runs
   (lambda ()
     (let (out (go t))
       (goto-char (point-min))
       (while (and go (if posix (posix-search-forward re bound t) (re-search-forward re bound t)))
         (push (match-data t) out)
         (when (= (match-beginning 0) (match-end 0))
           (if (eobp) (setq go nil) (forward-char 1))))
       (nreverse out)))))
(defun dfa-probe--backward (re &optional bound posix)
  (dfa-probe--runs
   (lambda ()
     (let (out (go t))
       (goto-char (point-max))
       (while (and go (if posix (posix-search-backward re bound t) (re-search-backward re bound t)))
         (push (match-data t) out)
         (when (= (match-beginning 0) (match-end 0))
           (if (bobp) (setq go nil) (backward-char 1))))
       (nreverse out)))))
(defun dfa-probe--both (text regexps &optional posix)
  (with-temp-buffer
    (insert text)
    (mapcar (lambda (re)
              (list re (dfa-probe--forward re nil posix) (dfa-probe--backward re nil posix)))
            regexps)))
"#;

fn form(body: &str) -> String {
    format!("(progn {HELPERS} {body})")
}

#[test]
fn oracle_dfa_filter_word_and_symbol_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = form(
        r#"(dfa-probe--both
             (concat (apply #'concat (make-list 12 "foofoo barfoo foo_bar fob fooo "))
                     "the foo is here, foo-bar and bar_baz baz ")
             '("\\bfoo\\b" "\\<bar\\>" "\\_<baz\\_>" "\\Bo\\B" "fo\\>" "\\_<foo_bar\\_>"
               "\\bfoo\\(?:bar\\)?\\b"))"#,
    );
    let expect = expect_test::expect![[
        r#""OK ((\"\\\\bfoo\\\\b\" (t t ((15 18 #<killed buffer>) (46 49 #<killed buffer>) (77 80 #<killed buffer>) (108 111 #<killed buffer>) (139 142 #<killed buffer>) (170 173 #<killed buffer>) (201 204 #<killed buffer>) (232 235 #<killed buffer>) (263 266 #<killed buffer>) (294 297 #<killed buffer>) (325 328 #<killed buffer>) (356 359 #<killed buffer>) (377 380 #<killed buffer>) (390 393 #<killed buffer>))) (t t ((390 393 #<killed buffer>) (377 380 #<killed buffer>) (356 359 #<killed buffer>) (325 328 #<killed buffer>) (294 297 #<killed buffer>) (263 266 #<killed buffer>) (232 235 #<killed buffer>) (201 204 #<killed buffer>) (170 173 #<killed buffer>) (139 142 #<killed buffer>) (108 111 #<killed buffer>) (77 80 #<killed buffer>) (46 49 #<killed buffer>) (15 18 #<killed buffer>)))) (\"\\\\<bar\\\\>\" (t t ((19 22 #<killed buffer>) (50 53 #<killed buffer>) (81 84 #<killed buffer>) (112 115 #<killed buffer>) (143 146 #<killed buffer>) (174 177 #<killed buffer>) (205 208 #<killed buffer>) (236 239 #<killed buffer>) (267 270 #<killed buffer>) (298 301 #<killed buffer>) (329 332 #<killed buffer>) (360 363 #<killed buffer>) (394 397 #<killed buffer>) (402 405 #<killed buffer>))) (t t ((402 405 #<killed buffer>) (394 397 #<killed buffer>) (360 363 #<killed buffer>) (329 332 #<killed buffer>) (298 301 #<killed buffer>) (267 270 #<killed buffer>) (236 239 #<killed buffer>) (205 208 #<killed buffer>) (174 177 #<killed buffer>) (143 146 #<killed buffer>) (112 115 #<killed buffer>) (81 84 #<killed buffer>) (50 53 #<killed buffer>) (19 22 #<killed buffer>)))) (\"\\\\_<baz\\\\_>\" (t t ((410 413 #<killed buffer>))) (t t ((410 413 #<killed buffer>)))) (\"\\\\Bo\\\\B\" (t t ((2 3 #<killed buffer>) (3 4 #<killed buffer>) (5 6 #<killed buffer>) (12 13 #<killed buffer>) (16 17 #<killed buffer>) (24 25 #<killed buffer>) (28 29 #<killed buffer>) (29 30 #<killed buffer>) (33 34 #<killed buffer>) (34 35 #<killed buffer>) (36 37 #<killed buffer>) (43 44 #<killed buffer>) (47 48 #<killed buffer>) (55 56 #<killed buffer>) (59 60 #<killed buffer>) (60 61 #<killed buffer>) (64 65 #<killed buffer>) (65 66 #<killed buffer>) (67 68 #<killed buffer>) (74 75 #<killed buffer>) (78 79 #<killed buffer>) (86 87 #<killed buffer>) (90 91 #<killed buffer>) (91 92 #<killed buffer>) (95 96 #<killed buffer>) (96 97 #<killed buffer>) (98 99 #<killed buffer>) (105 106 #<killed buffer>) (109 110 #<killed buffer>) (117 118 #<killed buffer>) (121 122 #<killed buffer>) (122 123 #<killed buffer>) (126 127 #<killed buffer>) (127 128 #<killed buffer>) (129 130 #<killed buffer>) (136 137 #<killed buffer>) (140 141 #<killed buffer>) (148 149 #<killed buffer>) (152 153 #<killed buffer>) (153 154 #<killed buffer>) (157 158 #<killed buffer>) (158 159 #<killed buffer>) (160 161 #<killed buffer>) (167 168 #<killed buffer>) (171 172 #<killed buffer>) (179 180 #<killed buffer>) (183 184 #<killed buffer>) (184 185 #<killed buffer>) (188 189 #<killed buffer>) (189 190 #<killed buffer>) (191 192 #<killed buffer>) (198 199 #<killed buffer>) (202 203 #<killed buffer>) (210 211 #<killed buffer>) (214 215 #<killed buffer>) (215 216 #<killed buffer>) (219 220 #<killed buffer>) (220 221 #<killed buffer>) (222 223 #<killed buffer>) (229 230 #<killed buffer>) (233 234 #<killed buffer>) (241 242 #<killed buffer>) (245 246 #<killed buffer>) (246 247 #<killed buffer>) (250 251 #<killed buffer>) (251 252 #<killed buffer>) (253 254 #<killed buffer>) (260 261 #<killed buffer>) (264 265 #<killed buffer>) (272 273 #<killed buffer>) (276 277 #<killed buffer>) (277 278 #<killed buffer>) (281 282 #<killed buffer>) (282 283 #<killed buffer>) (284 285 #<killed buffer>) (291 292 #<killed buffer>) (295 296 #<killed buffer>) (303 304 #<killed buffer>) (307 308 #<killed buffer>) (308 309 #<killed buffer>) (312 313 #<killed buffer>) (313 314 #<killed buffer>) (315 316 #<killed buffer>) (322 323 #<killed buffer>) (326 327 #<killed buffer>) (334 335 #<killed buffer>) (338 339 #<killed buffer>) (339 340 #<killed buffer>) (343 344 #<killed buffer>) (344 345 #<killed buffer>) (346 347 #<killed buffer>) (353 354 #<killed buffer>) (357 358 #<killed buffer>) (365 366 #<killed buffer>) (369 370 #<killed buffer>) (370 371 #<killed buffer>) (378 379 #<killed buffer>) (391 392 #<killed buffer>))) (t t ((391 392 #<killed buffer>) (378 379 #<killed buffer>) (370 371 #<killed buffer>) (369 370 #<killed buffer>) (365 366 #<killed buffer>) (357 358 #<killed buffer>) (353 354 #<killed buffer>) (346 347 #<killed buffer>) (344 345 #<killed buffer>) (343 344 #<killed buffer>) (339 340 #<killed buffer>) (338 339 #<killed buffer>) (334 335 #<killed buffer>) (326 327 #<killed buffer>) (322 323 #<killed buffer>) (315 316 #<killed buffer>) (313 314 #<killed buffer>) (312 313 #<killed buffer>) (308 309 #<killed buffer>) (307 308 #<killed buffer>) (303 304 #<killed buffer>) (295 296 #<killed buffer>) (291 292 #<killed buffer>) (284 285 #<killed buffer>) (282 283 #<killed buffer>) (281 282 #<killed buffer>) (277 278 #<killed buffer>) (276 277 #<killed buffer>) (272 273 #<killed buffer>) (264 265 #<killed buffer>) (260 261 #<killed buffer>) (253 254 #<killed buffer>) (251 252 #<killed buffer>) (250 251 #<killed buffer>) (246 247 #<killed buffer>) (245 246 #<killed buffer>) (241 242 #<killed buffer>) (233 234 #<killed buffer>) (229 230 #<killed buffer>) (222 223 #<killed buffer>) (220 221 #<killed buffer>) (219 220 #<killed buffer>) (215 216 #<killed buffer>) (214 215 #<killed buffer>) (210 211 #<killed buffer>) (202 203 #<killed buffer>) (198 199 #<killed buffer>) (191 192 #<killed buffer>) (189 190 #<killed buffer>) (188 189 #<killed buffer>) (184 185 #<killed buffer>) (183 184 #<killed buffer>) (179 180 #<killed buffer>) (171 172 #<killed buffer>) (167 168 #<killed buffer>) (160 161 #<killed buffer>) (158 159 #<killed buffer>) (157 158 #<killed buffer>) (153 154 #<killed buffer>) (152 153 #<killed buffer>) (148 149 #<killed buffer>) (140 141 #<killed buffer>) (136 137 #<killed buffer>) (129 130 #<killed buffer>) (127 128 #<killed buffer>) (126 127 #<killed buffer>) (122 123 #<killed buffer>) (121 122 #<killed buffer>) (117 118 #<killed buffer>) (109 110 #<killed buffer>) (105 106 #<killed buffer>) (98 99 #<killed buffer>) (96 97 #<killed buffer>) (95 96 #<killed buffer>) (91 92 #<killed buffer>) (90 91 #<killed buffer>) (86 87 #<killed buffer>) (78 79 #<killed buffer>) (74 75 #<killed buffer>) (67 68 #<killed buffer>) (65 66 #<killed buffer>) (64 65 #<killed buffer>) (60 61 #<killed buffer>) (59 60 #<killed buffer>) (55 56 #<killed buffer>) (47 48 #<killed buffer>) (43 44 #<killed buffer>) (36 37 #<killed buffer>) (34 35 #<killed buffer>) (33 34 #<killed buffer>) (29 30 #<killed buffer>) (28 29 #<killed buffer>) (24 25 #<killed buffer>) (16 17 #<killed buffer>) (12 13 #<killed buffer>) (5 6 #<killed buffer>) (3 4 #<killed buffer>) (2 3 #<killed buffer>)))) (\"fo\\\\>\" (t t nil) (t t nil)) (\"\\\\_<foo_bar\\\\_>\" (t t ((15 22 #<killed buffer>) (46 53 #<killed buffer>) (77 84 #<killed buffer>) (108 115 #<killed buffer>) (139 146 #<killed buffer>) (170 177 #<killed buffer>) (201 208 #<killed buffer>) (232 239 #<killed buffer>) (263 270 #<killed buffer>) (294 301 #<killed buffer>) (325 332 #<killed buffer>) (356 363 #<killed buffer>))) (t t ((356 363 #<killed buffer>) (325 332 #<killed buffer>) (294 301 #<killed buffer>) (263 270 #<killed buffer>) (232 239 #<killed buffer>) (201 208 #<killed buffer>) (170 177 #<killed buffer>) (139 146 #<killed buffer>) (108 115 #<killed buffer>) (77 84 #<killed buffer>) (46 53 #<killed buffer>) (15 22 #<killed buffer>)))) (\"\\\\bfoo\\\\(?:bar\\\\)?\\\\b\" (t t ((15 18 #<killed buffer>) (46 49 #<killed buffer>) (77 80 #<killed buffer>) (108 111 #<killed buffer>) (139 142 #<killed buffer>) (170 173 #<killed buffer>) (201 204 #<killed buffer>) (232 235 #<killed buffer>) (263 266 #<killed buffer>) (294 297 #<killed buffer>) (325 328 #<killed buffer>) (356 359 #<killed buffer>) (377 380 #<killed buffer>) (390 393 #<killed buffer>))) (t t ((390 393 #<killed buffer>) (377 380 #<killed buffer>) (356 359 #<killed buffer>) (325 328 #<killed buffer>) (294 297 #<killed buffer>) (263 266 #<killed buffer>) (232 235 #<killed buffer>) (201 204 #<killed buffer>) (170 173 #<killed buffer>) (139 142 #<killed buffer>) (108 111 #<killed buffer>) (77 80 #<killed buffer>) (46 49 #<killed buffer>) (15 18 #<killed buffer>)))))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(&form, DFA_MODES, expect);
}

#[test]
fn oracle_dfa_filter_line_anchors_and_alternations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = form(
        r#"(dfa-probe--both
             (concat (apply #'concat (make-list 10 "  - item one\n  plain + text\n * star\n"))
                     "- first\n+ plus :: tag\n  + deep :: x\nlast line x\n")
             '("^[ \t]*[-+] item" "x$" "\\(?:^[ \t]*[-+]\\|^[ \t]+[*]\\)[ \t]+\\(.*?[ \t]+::\\)\\([ \t]+\\|$\\)"
               "\\(?:^a\\|^-\\) \\(\\w+\\)" "^$\\|zz"))"#,
    );
    let expect = expect_test::expect![[
        r#""OK ((\"^[ \t]*[-+] item\" (t t ((1 9 #<killed buffer>) (37 45 #<killed buffer>) (73 81 #<killed buffer>) (109 117 #<killed buffer>) (145 153 #<killed buffer>) (181 189 #<killed buffer>) (217 225 #<killed buffer>) (253 261 #<killed buffer>) (289 297 #<killed buffer>) (325 333 #<killed buffer>))) (t t ((325 333 #<killed buffer>) (289 297 #<killed buffer>) (253 261 #<killed buffer>) (217 225 #<killed buffer>) (181 189 #<killed buffer>) (145 153 #<killed buffer>) (109 117 #<killed buffer>) (73 81 #<killed buffer>) (37 45 #<killed buffer>) (1 9 #<killed buffer>)))) (\"x$\" (t t ((395 396 #<killed buffer>) (407 408 #<killed buffer>))) (t t ((407 408 #<killed buffer>) (395 396 #<killed buffer>)))) (\"\\\\(?:^[ \t]*[-+]\\\\|^[ \t]+[*]\\\\)[ \t]+\\\\(.*?[ \t]+::\\\\)\\\\([ \t]+\\\\|$\\\\)\" (t t ((369 379 371 378 378 379 #<killed buffer>) (383 395 387 394 394 395 #<killed buffer>))) (t t ((383 395 387 394 394 395 #<killed buffer>) (369 379 371 378 378 379 #<killed buffer>)))) (\"\\\\(?:^a\\\\|^-\\\\) \\\\(\\\\w+\\\\)\" (t t ((361 368 363 368 #<killed buffer>))) (t t ((361 368 363 368 #<killed buffer>)))) (\"^$\\\\|zz\" (t t ((409 409 #<killed buffer>))) (t t ((409 409 #<killed buffer>)))))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(&form, DFA_MODES, expect);
}

#[test]
fn oracle_dfa_filter_case_folding_and_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = form(
        r#"(let ((case-fold-search t))
             (dfa-probe--both
              (concat (apply #'concat (make-list 10 "Foo foO fo1 FOOX éclair Éclat σΣς KELVIN\x212a "))
                      "foo7 FOO8 ÉCLAIR9 kelvin\n")
              '("foo[0-9]" "éclair[0-9]" "[σΣ]+ς" "k[a-z]+n\x212a" "\\(?:f\\|é\\)[a-zé]+[0-9]")))"#,
    );
    let expect = expect_test::expect![[
        r#""OK ((\"foo[0-9]\" (t t ((421 425 #<killed buffer>) (426 430 #<killed buffer>))) (t t ((426 430 #<killed buffer>) (421 425 #<killed buffer>)))) (\"éclair[0-9]\" (t t ((431 438 #<killed buffer>))) (t t ((431 438 #<killed buffer>)))) (\"[σΣ]+ς\" (t t ((31 34 #<killed buffer>) (73 76 #<killed buffer>) (115 118 #<killed buffer>) (157 160 #<killed buffer>) (199 202 #<killed buffer>) (241 244 #<killed buffer>) (283 286 #<killed buffer>) (325 328 #<killed buffer>) (367 370 #<killed buffer>) (409 412 #<killed buffer>))) (t t ((410 412 #<killed buffer>) (368 370 #<killed buffer>) (326 328 #<killed buffer>) (284 286 #<killed buffer>) (242 244 #<killed buffer>) (200 202 #<killed buffer>) (158 160 #<killed buffer>) (116 118 #<killed buffer>) (74 76 #<killed buffer>) (32 34 #<killed buffer>)))) (\"k[a-z]+nK\" (t t ((35 42 #<killed buffer>) (77 84 #<killed buffer>) (119 126 #<killed buffer>) (161 168 #<killed buffer>) (203 210 #<killed buffer>) (245 252 #<killed buffer>) (287 294 #<killed buffer>) (329 336 #<killed buffer>) (371 378 #<killed buffer>) (413 420 #<killed buffer>))) (t t ((413 420 #<killed buffer>) (371 378 #<killed buffer>) (329 336 #<killed buffer>) (287 294 #<killed buffer>) (245 252 #<killed buffer>) (203 210 #<killed buffer>) (161 168 #<killed buffer>) (119 126 #<killed buffer>) (77 84 #<killed buffer>) (35 42 #<killed buffer>)))) (\"\\\\(?:f\\\\|é\\\\)[a-zé]+[0-9]\" (t t ((9 12 #<killed buffer>) (51 54 #<killed buffer>) (93 96 #<killed buffer>) (135 138 #<killed buffer>) (177 180 #<killed buffer>) (219 222 #<killed buffer>) (261 264 #<killed buffer>) (303 306 #<killed buffer>) (345 348 #<killed buffer>) (387 390 #<killed buffer>) (421 425 #<killed buffer>) (426 430 #<killed buffer>) (431 438 #<killed buffer>))) (t t ((431 438 #<killed buffer>) (426 430 #<killed buffer>) (421 425 #<killed buffer>) (387 390 #<killed buffer>) (345 348 #<killed buffer>) (303 306 #<killed buffer>) (261 264 #<killed buffer>) (219 222 #<killed buffer>) (177 180 #<killed buffer>) (135 138 #<killed buffer>) (93 96 #<killed buffer>) (51 54 #<killed buffer>) (9 12 #<killed buffer>)))))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(&form, DFA_MODES, expect);
}

#[test]
fn oracle_dfa_filter_unibyte_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = form(
        r#"(with-temp-buffer
             (set-buffer-multibyte nil)
             (dotimes (i 20) (insert (+ 128 (% (* i 7) 128)) ?a ?\s ?x ?\351 ?b ?\n))
             (insert "\351z end")
             (mapcar (lambda (re) (list re (dfa-probe--forward re) (dfa-probe--backward re)))
                     '("\351[a-z]" "[\200-\377]x" "x\351b$" "\351z")))"#,
    );
    let expect = expect_test::expect![[
        r#""OK ((\"�[a-z]\" (t t ((5 7 #<killed buffer>) (12 14 #<killed buffer>) (19 21 #<killed buffer>) (26 28 #<killed buffer>) (33 35 #<killed buffer>) (40 42 #<killed buffer>) (47 49 #<killed buffer>) (54 56 #<killed buffer>) (61 63 #<killed buffer>) (68 70 #<killed buffer>) (75 77 #<killed buffer>) (82 84 #<killed buffer>) (89 91 #<killed buffer>) (96 98 #<killed buffer>) (103 105 #<killed buffer>) (106 108 #<killed buffer>) (110 112 #<killed buffer>) (117 119 #<killed buffer>) (124 126 #<killed buffer>) (131 133 #<killed buffer>) (138 140 #<killed buffer>) (141 143 #<killed buffer>))) (t t ((141 143 #<killed buffer>) (138 140 #<killed buffer>) (131 133 #<killed buffer>) (124 126 #<killed buffer>) (117 119 #<killed buffer>) (110 112 #<killed buffer>) (106 108 #<killed buffer>) (103 105 #<killed buffer>) (96 98 #<killed buffer>) (89 91 #<killed buffer>) (82 84 #<killed buffer>) (75 77 #<killed buffer>) (68 70 #<killed buffer>) (61 63 #<killed buffer>) (54 56 #<killed buffer>) (47 49 #<killed buffer>) (40 42 #<killed buffer>) (33 35 #<killed buffer>) (26 28 #<killed buffer>) (19 21 #<killed buffer>) (12 14 #<killed buffer>) (5 7 #<killed buffer>)))) (\"[�-�]x\" (t t nil) (t t nil)) (\"x�b$\" (t t ((4 7 #<killed buffer>) (11 14 #<killed buffer>) (18 21 #<killed buffer>) (25 28 #<killed buffer>) (32 35 #<killed buffer>) (39 42 #<killed buffer>) (46 49 #<killed buffer>) (53 56 #<killed buffer>) (60 63 #<killed buffer>) (67 70 #<killed buffer>) (74 77 #<killed buffer>) (81 84 #<killed buffer>) (88 91 #<killed buffer>) (95 98 #<killed buffer>) (102 105 #<killed buffer>) (109 112 #<killed buffer>) (116 119 #<killed buffer>) (123 126 #<killed buffer>) (130 133 #<killed buffer>) (137 140 #<killed buffer>))) (t t ((137 140 #<killed buffer>) (130 133 #<killed buffer>) (123 126 #<killed buffer>) (116 119 #<killed buffer>) (109 112 #<killed buffer>) (102 105 #<killed buffer>) (95 98 #<killed buffer>) (88 91 #<killed buffer>) (81 84 #<killed buffer>) (74 77 #<killed buffer>) (67 70 #<killed buffer>) (60 63 #<killed buffer>) (53 56 #<killed buffer>) (46 49 #<killed buffer>) (39 42 #<killed buffer>) (32 35 #<killed buffer>) (25 28 #<killed buffer>) (18 21 #<killed buffer>) (11 14 #<killed buffer>) (4 7 #<killed buffer>)))) (\"�z\" (t t ((141 143 #<killed buffer>))) (t t ((141 143 #<killed buffer>)))))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(&form, DFA_MODES, expect);
}

#[test]
fn oracle_dfa_filter_bounded_backward_and_posix_searches() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = form(
        r#"(with-temp-buffer
             (insert (apply #'concat (make-list 12 "ab abc abcd x:y a:b ")))
             (list
              (dfa-probe--forward "abc[a-z]? x" 150)
              (dfa-probe--backward "a:\\(b\\|y\\)" 80)
              (dfa-probe--forward "\\(a\\|ab\\)\\(c\\|bcd\\)" nil t)
              (dfa-probe--backward "\\(a\\|ab\\)\\(c\\|bcd\\) " nil t)
              (progn (goto-char 120) (list (re-search-forward "x:z\\|abcd x" 150 t) (match-data t)))
              (progn (goto-char 200) (list (re-search-backward "y a" 150 t) (match-data t)))))"#,
    );
    let expect = expect_test::expect![[
        r#""OK ((t t ((8 14 #<killed buffer>) (28 34 #<killed buffer>) (48 54 #<killed buffer>) (68 74 #<killed buffer>) (88 94 #<killed buffer>) (108 114 #<killed buffer>) (128 134 #<killed buffer>))) (t t ((237 240 239 240 #<killed buffer>) (217 220 219 220 #<killed buffer>) (197 200 199 200 #<killed buffer>) (177 180 179 180 #<killed buffer>) (157 160 159 160 #<killed buffer>) (137 140 139 140 #<killed buffer>) (117 120 119 120 #<killed buffer>) (97 100 99 100 #<killed buffer>))) (t t ((4 7 4 6 6 7 #<killed buffer>) (8 12 8 9 9 12 #<killed buffer>) (24 27 24 26 26 27 #<killed buffer>) (28 32 28 29 29 32 #<killed buffer>) (44 47 44 46 46 47 #<killed buffer>) (48 52 48 49 49 52 #<killed buffer>) (64 67 64 66 66 67 #<killed buffer>) (68 72 68 69 69 72 #<killed buffer>) (84 87 84 86 86 87 #<killed buffer>) (88 92 88 89 89 92 #<killed buffer>) (104 107 104 106 106 107 #<killed buffer>) (108 112 108 109 109 112 #<killed buffer>) (124 127 124 126 126 127 #<killed buffer>) (128 132 128 129 129 132 #<killed buffer>) (144 147 144 146 146 147 #<killed buffer>) (148 152 148 149 149 152 #<killed buffer>) (164 167 164 166 166 167 #<killed buffer>) (168 172 168 169 169 172 #<killed buffer>) (184 187 184 186 186 187 #<killed buffer>) (188 192 188 189 189 192 #<killed buffer>) (204 207 204 206 206 207 #<killed buffer>) (208 212 208 209 209 212 #<killed buffer>) (224 227 224 226 226 227 #<killed buffer>) (228 232 228 229 229 232 #<killed buffer>))) (t t ((228 233 228 229 229 232 #<killed buffer>) (224 228 224 226 226 227 #<killed buffer>) (208 213 208 209 209 212 #<killed buffer>) (204 208 204 206 206 207 #<killed buffer>) (188 193 188 189 189 192 #<killed buffer>) (184 188 184 186 186 187 #<killed buffer>) (168 173 168 169 169 172 #<killed buffer>) (164 168 164 166 166 167 #<killed buffer>) (148 153 148 149 149 152 #<killed buffer>) (144 148 144 146 146 147 #<killed buffer>) (128 133 128 129 129 132 #<killed buffer>) (124 128 124 126 126 127 #<killed buffer>) (108 113 108 109 109 112 #<killed buffer>) (104 108 104 106 106 107 #<killed buffer>) (88 93 88 89 89 92 #<killed buffer>) (84 88 84 86 86 87 #<killed buffer>) (68 73 68 69 69 72 #<killed buffer>) (64 68 64 66 66 67 #<killed buffer>) (48 53 48 49 49 52 #<killed buffer>) (44 48 44 46 46 47 #<killed buffer>) (28 33 28 29 29 32 #<killed buffer>) (24 28 24 26 26 27 #<killed buffer>) (8 13 8 9 9 12 #<killed buffer>) (4 8 4 6 6 7 #<killed buffer>))) (134 (128 134 #<killed buffer>)) (195 (195 198 #<killed buffer>)))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(&form, DFA_MODES, expect);
}

#[test]
fn oracle_dfa_filter_point_and_looking_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = form(
        r#"(with-temp-buffer
             (insert (apply #'concat (make-list 6 "foo bar  foo\tbaz ")))
             (let (out)
               (dotimes (i (1- (point-max)))
                 (goto-char (1+ i))
                 (push (list (point)
                             (looking-back "foo\\s-*" (line-beginning-position))
                             (save-excursion (and (re-search-backward "\\(?:bar\\|baz\\) *\\=" nil t) (point)))
                             (save-excursion (and (re-search-forward "\\=\\s-*\\(b\\w+\\)" nil t) (match-data t))))
                       out))
               (nreverse out)))"#,
    );
    let expect = expect_test::expect![[
        r#""OK ((1 nil nil nil) (2 nil nil nil) (3 nil nil nil) (4 t nil (4 8 5 8 #<killed buffer>)) (5 t nil (5 8 5 8 #<killed buffer>)) (6 nil nil nil) (7 nil nil nil) (8 nil 5 nil) (9 nil 5 nil) (10 nil 5 nil) (11 nil nil nil) (12 nil nil nil) (13 t nil (13 17 14 17 #<killed buffer>)) (14 t nil (14 17 14 17 #<killed buffer>)) (15 nil nil nil) (16 nil nil nil) (17 nil 14 nil) (18 nil 14 nil) (19 nil nil nil) (20 nil nil nil) (21 t nil (21 25 22 25 #<killed buffer>)) (22 t nil (22 25 22 25 #<killed buffer>)) (23 nil nil nil) (24 nil nil nil) (25 nil 22 nil) (26 nil 22 nil) (27 nil 22 nil) (28 nil nil nil) (29 nil nil nil) (30 t nil (30 34 31 34 #<killed buffer>)) (31 t nil (31 34 31 34 #<killed buffer>)) (32 nil nil nil) (33 nil nil nil) (34 nil 31 nil) (35 nil 31 nil) (36 nil nil nil) (37 nil nil nil) (38 t nil (38 42 39 42 #<killed buffer>)) (39 t nil (39 42 39 42 #<killed buffer>)) (40 nil nil nil) (41 nil nil nil) (42 nil 39 nil) (43 nil 39 nil) (44 nil 39 nil) (45 nil nil nil) (46 nil nil nil) (47 t nil (47 51 48 51 #<killed buffer>)) (48 t nil (48 51 48 51 #<killed buffer>)) (49 nil nil nil) (50 nil nil nil) (51 nil 48 nil) (52 nil 48 nil) (53 nil nil nil) (54 nil nil nil) (55 t nil (55 59 56 59 #<killed buffer>)) (56 t nil (56 59 56 59 #<killed buffer>)) (57 nil nil nil) (58 nil nil nil) (59 nil 56 nil) (60 nil 56 nil) (61 nil 56 nil) (62 nil nil nil) (63 nil nil nil) (64 t nil (64 68 65 68 #<killed buffer>)) (65 t nil (65 68 65 68 #<killed buffer>)) (66 nil nil nil) (67 nil nil nil) (68 nil 65 nil) (69 nil 65 nil) (70 nil nil nil) (71 nil nil nil) (72 t nil (72 76 73 76 #<killed buffer>)) (73 t nil (73 76 73 76 #<killed buffer>)) (74 nil nil nil) (75 nil nil nil) (76 nil 73 nil) (77 nil 73 nil) (78 nil 73 nil) (79 nil nil nil) (80 nil nil nil) (81 t nil (81 85 82 85 #<killed buffer>)) (82 t nil (82 85 82 85 #<killed buffer>)) (83 nil nil nil) (84 nil nil nil) (85 nil 82 nil) (86 nil 82 nil) (87 nil nil nil) (88 nil nil nil) (89 t nil (89 93 90 93 #<killed buffer>)) (90 t nil (90 93 90 93 #<killed buffer>)) (91 nil nil nil) (92 nil nil nil) (93 nil 90 nil) (94 nil 90 nil) (95 nil 90 nil) (96 nil nil nil) (97 nil nil nil) (98 t nil (98 102 99 102 #<killed buffer>)) (99 t nil (99 102 99 102 #<killed buffer>)) (100 nil nil nil) (101 nil nil nil) (102 nil 99 nil))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(&form, DFA_MODES, expect);
}

#[test]
fn oracle_dfa_filter_syntax_classes_and_categories() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = form(
        r#"(dfa-probe--both
             (concat (apply #'concat (make-list 10 "word_1 (sym-bol) 12 αβγ 中文 x-y "))
                     "tail\n")
             '("\\w+\\s-+(\\s_*" "[[:space:]]\\{2\\}" "[[:word:]]+-[[:word:]]"
               "\\cg+ \\cc" "\\sw\\s." "[[:punct:]][[:alpha:]]+)"))"#,
    );
    let expect = expect_test::expect![[
        r#""OK ((\"\\\\w+\\\\s-+(\\\\s_*\" (t t ((6 9 #<killed buffer>) (37 40 #<killed buffer>) (68 71 #<killed buffer>) (99 102 #<killed buffer>) (130 133 #<killed buffer>) (161 164 #<killed buffer>) (192 195 #<killed buffer>) (223 226 #<killed buffer>) (254 257 #<killed buffer>) (285 288 #<killed buffer>))) (t t ((285 288 #<killed buffer>) (254 257 #<killed buffer>) (223 226 #<killed buffer>) (192 195 #<killed buffer>) (161 164 #<killed buffer>) (130 133 #<killed buffer>) (99 102 #<killed buffer>) (68 71 #<killed buffer>) (37 40 #<killed buffer>) (6 9 #<killed buffer>)))) (\"[[:space:]]\\\\{2\\\\}\" (t t nil) (t t nil)) (\"[[:word:]]+-[[:word:]]\" (t t ((9 14 #<killed buffer>) (28 31 #<killed buffer>) (40 45 #<killed buffer>) (59 62 #<killed buffer>) (71 76 #<killed buffer>) (90 93 #<killed buffer>) (102 107 #<killed buffer>) (121 124 #<killed buffer>) (133 138 #<killed buffer>) (152 155 #<killed buffer>) (164 169 #<killed buffer>) (183 186 #<killed buffer>) (195 200 #<killed buffer>) (214 217 #<killed buffer>) (226 231 #<killed buffer>) (245 248 #<killed buffer>) (257 262 #<killed buffer>) (276 279 #<killed buffer>) (288 293 #<killed buffer>) (307 310 #<killed buffer>))) (t t ((307 310 #<killed buffer>) (290 293 #<killed buffer>) (276 279 #<killed buffer>) (259 262 #<killed buffer>) (245 248 #<killed buffer>) (228 231 #<killed buffer>) (214 217 #<killed buffer>) (197 200 #<killed buffer>) (183 186 #<killed buffer>) (166 169 #<killed buffer>) (152 155 #<killed buffer>) (135 138 #<killed buffer>) (121 124 #<killed buffer>) (104 107 #<killed buffer>) (90 93 #<killed buffer>) (73 76 #<killed buffer>) (59 62 #<killed buffer>) (42 45 #<killed buffer>) (28 31 #<killed buffer>) (11 14 #<killed buffer>)))) (\"\\\\cg+ \\\\cc\" (t t ((21 26 #<killed buffer>) (52 57 #<killed buffer>) (83 88 #<killed buffer>) (114 119 #<killed buffer>) (145 150 #<killed buffer>) (176 181 #<killed buffer>) (207 212 #<killed buffer>) (238 243 #<killed buffer>) (269 274 #<killed buffer>) (300 305 #<killed buffer>))) (t t ((302 305 #<killed buffer>) (271 274 #<killed buffer>) (240 243 #<killed buffer>) (209 212 #<killed buffer>) (178 181 #<killed buffer>) (147 150 #<killed buffer>) (116 119 #<killed buffer>) (85 88 #<killed buffer>) (54 57 #<killed buffer>) (23 26 #<killed buffer>)))) (\"\\\\sw\\\\s.\" (t t nil) (t t nil)) (\"[[:punct:]][[:alpha:]]+)\" (t t ((12 17 #<killed buffer>) (43 48 #<killed buffer>) (74 79 #<killed buffer>) (105 110 #<killed buffer>) (136 141 #<killed buffer>) (167 172 #<killed buffer>) (198 203 #<killed buffer>) (229 234 #<killed buffer>) (260 265 #<killed buffer>) (291 296 #<killed buffer>))) (t t ((291 296 #<killed buffer>) (260 265 #<killed buffer>) (229 234 #<killed buffer>) (198 203 #<killed buffer>) (167 172 #<killed buffer>) (136 141 #<killed buffer>) (105 110 #<killed buffer>) (74 79 #<killed buffer>) (43 48 #<killed buffer>) (12 17 #<killed buffer>)))))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(&form, DFA_MODES, expect);
}

#[test]
fn oracle_dfa_filter_keeps_the_fail_stack_overflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = form(
        r#"(with-temp-buffer
             (insert (apply #'concat (make-list 30 "xab xba xabab ")))
             (let ((warm (car (dfa-probe--forward "x\\(?:a\\|b\\)*c"))))
               (erase-buffer)
               (insert "x" (apply #'concat (make-list 100000 "ab")))
               (goto-char (point-min))
               (list warm
                     (condition-case err (re-search-forward "x\\(?:a\\|b\\)*c" nil t)
                       (error (list 'error err))))))"#,
    );
    let expect =
        expect_test::expect![[r#""OK (t (error (error \"Stack overflow in regexp matcher\")))""#]];
    crate::common::assert_oracle_parity_under_envs_expect(&form, DFA_MODES, expect);
}

#[test]
fn oracle_dfa_filter_string_match_loops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((s (apply #'concat (make-list 12 "key=val; k2 = v2;; =x; key=;")))
                        out)
                    (dolist (re '("\\([a-z0-9]+\\) *= *\\([a-z0-9]+\\);" "=\\([a-z]\\);" ";;+ *="))
                      (let ((start 0) hits)
                        (dotimes (_ 3)
                          (setq start 0)
                          (while (string-match re s start)
                            (push (match-data) hits)
                            (setq start (max (1+ (match-beginning 0)) (match-end 0)))))
                        (push (list re (length hits) (car hits)) out)))
                    (nreverse out))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\\\\([a-z0-9]+\\\\) *= *\\\\([a-z0-9]+\\\\);\" 72 (317 325 317 319 322 324)) (\"=\\\\([a-z]\\\\);\" 36 (327 330 328 329)) (\";;+ *=\" 36 (324 328)))""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, DFA_MODES, expect);
}
