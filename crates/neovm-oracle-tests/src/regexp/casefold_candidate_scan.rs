//! GNU parity for case-folded candidate scans.
//!
//! A case-folded `re-search-forward`, `re-search-backward` or `string-match`
//! may skip a position only where GNU's `re_search_2` would never enter the
//! matcher.  These probes search mixed-case text holding the characters whose
//! Unicode case partner is ASCII (K U+212A, ſ, İ, ı -- which GNU's standard
//! case table deliberately does not fold, characters.el:803-812), in texts
//! long enough (over 256 bytes) that neomacs builds its lazily derived
//! scanners, with and without a search bound, in multibyte and unibyte
//! buffers, and through a custom case table.

use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_regexp_casefold_scan_multibyte_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((case-fold-search t)
      (text (concat "(DEFUN alpha-1 ()) (Defun beta) (defun gamma) "
                    "(deſun sigma) (dİfun x) (Key) (defın y) "
                    "BYTE-COMPILE byte-compile Byte-Compile-x xbyte-compile "
                    "let LET* Let*x let* (LET) (CATCH (Throw (rEqUiRe "
                    "İıſK ßẞ Σσς Ａ 中文\n")))
  (with-temp-buffer
    (dotimes (_ 4) (insert text (make-string 80 ?.) "\n"))
    (let* ((md (lambda ()
                 (list (match-beginning 0) (match-end 0)
                       (match-beginning 1) (match-end 1))))
           (fwd (lambda (re &optional start bound)
                  (goto-char (or start (point-min)))
                  (let (acc)
                    (while (re-search-forward re bound t)
                      (push (funcall md) acc))
                    (nreverse acc))))
           (bwd (lambda (re &optional start bound)
                  (goto-char (or start (point-max)))
                  (let (acc)
                    (while (re-search-backward re bound t)
                      (push (funcall md) acc))
                    (nreverse acc)))))
      (mapcar (lambda (re)
                (list re
                      (funcall fwd re)
                      (funcall bwd re)
                      (funcall fwd re 100 260)
                      (funcall bwd re 400 150)))
              '("(defun \\([-a-z0-9]+\\)"
                "\\_<byte-compile\\_>"
                "\\_<let\\*?\\_>"
                "(\\(catch\\|throw\\|defun\\|provide\\|require\\)"
                "k" "s" "i" "ß" "σ")))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"(defun \\\\([-a-z0-9]+\\\\)\" ((1 15 8 15) (20 31 27 31) (33 45 40 45) (289 303 296 303) (308 319 315 319) (321 333 328 333) (577 591 584 591) (596 607 603 607) (609 621 616 621) (865 879 872 879) (884 895 891 895) (897 909 904 909)) ((897 909 904 909) (884 895 891 895) (865 879 872 879) (609 621 616 621) (596 607 603 607) (577 591 584 591) (321 333 328 333) (308 319 315 319) (289 303 296 303) (33 45 40 45) (20 31 27 31) (1 15 8 15)) nil ((321 333 328 333) (308 319 315 319) (289 303 296 303))) (\"\\\\_<byte-compile\\\\_>\" ((87 99 nil nil) (100 112 nil nil) (375 387 nil nil) (388 400 nil nil) (663 675 nil nil) (676 688 nil nil) (951 963 nil nil) (964 976 nil nil)) ((964 976 nil nil) (951 963 nil nil) (676 688 nil nil) (663 675 nil nil) (388 400 nil nil) (375 387 nil nil) (100 112 nil nil) (87 99 nil nil)) ((100 112 nil nil)) ((388 400 nil nil) (375 387 nil nil))) (\"\\\\_<let\\\\*?\\\\_>\" ((142 145 nil nil) (146 150 nil nil) (157 161 nil nil) (163 166 nil nil) (430 433 nil nil) (434 438 nil nil) (445 449 nil nil) (451 454 nil nil) (718 721 nil nil) (722 726 nil nil) (733 737 nil nil) (739 742 nil nil) (1006 1009 nil nil) (1010 1014 nil nil) (1021 1025 nil nil) (1027 1030 nil nil)) ((1027 1030 nil nil) (1021 1025 nil nil) (1010 1014 nil nil) (1006 1009 nil nil) (739 742 nil nil) (733 737 nil nil) (722 726 nil nil) (718 721 nil nil) (451 454 nil nil) (445 449 nil nil) (434 438 nil nil) (430 433 nil nil) (163 166 nil nil) (157 161 nil nil) (146 150 nil nil) (142 145 nil nil)) ((142 145 nil nil) (146 150 nil nil) (157 161 nil nil) (163 166 nil nil)) ((163 166 nil nil) (157 161 nil nil))) (\"(\\\\(catch\\\\|throw\\\\|defun\\\\|provide\\\\|require\\\\)\" ((1 7 2 7) (20 26 21 26) (33 39 34 39) (168 174 169 174) (175 181 176 181) (182 190 183 190) (289 295 290 295) (308 314 309 314) (321 327 322 327) (456 462 457 462) (463 469 464 469) (470 478 471 478) (577 583 578 583) (596 602 597 602) (609 615 610 615) (744 750 745 750) (751 757 752 757) (758 766 759 766) (865 871 866 871) (884 890 885 890) (897 903 898 903) (1032 1038 1033 1038) (1039 1045 1040 1045) (1046 1054 1047 1054)) ((1046 1054 1047 1054) (1039 1045 1040 1045) (1032 1038 1033 1038) (897 903 898 903) (884 890 885 890) (865 871 866 871) (758 766 759 766) (751 757 752 757) (744 750 745 750) (609 615 610 615) (596 602 597 602) (577 583 578 583) (470 478 471 478) (463 469 464 469) (456 462 457 462) (321 327 322 327) (308 314 309 314) (289 295 290 295) (182 190 183 190) (175 181 176 181) (168 174 169 174) (33 39 34 39) (20 26 21 26) (1 7 2 7)) ((168 174 169 174) (175 181 176 181) (182 190 183 190)) ((321 327 322 327) (308 314 309 314) (289 295 290 295) (182 190 183 190) (175 181 176 181) (168 174 169 174))) (\"k\" nil nil nil nil) (\"s\" ((54 55 nil nil) (342 343 nil nil) (630 631 nil nil) (918 919 nil nil)) ((918 919 nil nil) (630 631 nil nil) (342 343 nil nil) (54 55 nil nil)) nil ((342 343 nil nil))) (\"i\" ((55 56 nil nil) (96 97 nil nil) (109 110 nil nil) (122 123 nil nil) (138 139 nil nil) (187 188 nil nil) (343 344 nil nil) (384 385 nil nil) (397 398 nil nil) (410 411 nil nil) (426 427 nil nil) (475 476 nil nil) (631 632 nil nil) (672 673 nil nil) (685 686 nil nil) (698 699 nil nil) (714 715 nil nil) (763 764 nil nil) (919 920 nil nil) (960 961 nil nil) (973 974 nil nil) (986 987 nil nil) (1002 1003 nil nil) (1051 1052 nil nil)) ((1051 1052 nil nil) (1002 1003 nil nil) (986 987 nil nil) (973 974 nil nil) (960 961 nil nil) (919 920 nil nil) (763 764 nil nil) (714 715 nil nil) (698 699 nil nil) (685 686 nil nil) (672 673 nil nil) (631 632 nil nil) (475 476 nil nil) (426 427 nil nil) (410 411 nil nil) (397 398 nil nil) (384 385 nil nil) (343 344 nil nil) (187 188 nil nil) (138 139 nil nil) (122 123 nil nil) (109 110 nil nil) (96 97 nil nil) (55 56 nil nil)) ((109 110 nil nil) (122 123 nil nil) (138 139 nil nil) (187 188 nil nil)) ((397 398 nil nil) (384 385 nil nil) (343 344 nil nil) (187 188 nil nil))) (\"ß\" ((196 197 nil nil) (197 198 nil nil) (484 485 nil nil) (485 486 nil nil) (772 773 nil nil) (773 774 nil nil) (1060 1061 nil nil) (1061 1062 nil nil)) ((1061 1062 nil nil) (1060 1061 nil nil) (773 774 nil nil) (772 773 nil nil) (485 486 nil nil) (484 485 nil nil) (197 198 nil nil) (196 197 nil nil)) ((196 197 nil nil) (197 198 nil nil)) ((197 198 nil nil) (196 197 nil nil))) (\"σ\" ((199 200 nil nil) (200 201 nil nil) (201 202 nil nil) (487 488 nil nil) (488 489 nil nil) (489 490 nil nil) (775 776 nil nil) (776 777 nil nil) (777 778 nil nil) (1063 1064 nil nil) (1064 1065 nil nil) (1065 1066 nil nil)) ((1065 1066 nil nil) (1064 1065 nil nil) (1063 1064 nil nil) (777 778 nil nil) (776 777 nil nil) (775 776 nil nil) (489 490 nil nil) (488 489 nil nil) (487 488 nil nil) (201 202 nil nil) (200 201 nil nil) (199 200 nil nil)) ((199 200 nil nil) (200 201 nil nil) (201 202 nil nil)) ((201 202 nil nil) (200 201 nil nil) (199 200 nil nil))))""#
    ]];
    assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_casefold_scan_unibyte_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((case-fold-search t))
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (dotimes (_ 4)
      (insert "(DEFUN alpha-1 ()) (Defun beta) (defun gamma) x\311y\351 "
              "BYTE-COMPILE byte-compile Byte-Compile-x xbyte-compile "
              "let LET* Let*x let* (LET) (CATCH (Throw (rEqUiRe "
              (make-string 80 ?.) "\n"))
    (let* ((md (lambda ()
                 (list (match-beginning 0) (match-end 0)
                       (match-beginning 1) (match-end 1))))
           (fwd (lambda (re &optional start bound)
                  (goto-char (or start (point-min)))
                  (let (acc)
                    (while (re-search-forward re bound t)
                      (push (funcall md) acc))
                    (nreverse acc))))
           (bwd (lambda (re &optional start bound)
                  (goto-char (or start (point-max)))
                  (let (acc)
                    (while (re-search-backward re bound t)
                      (push (funcall md) acc))
                    (nreverse acc)))))
      (mapcar (lambda (re)
                (list re
                      (funcall fwd re)
                      (funcall bwd re)
                      (funcall fwd re 100 260)
                      (funcall bwd re 400 150)))
              '("(defun \\([-a-z0-9]+\\)"
                "\\_<byte-compile\\_>"
                "\\_<let\\*?\\_>"
                "(\\(catch\\|throw\\|defun\\|provide\\|require\\)"
                "x" "y" "compile")))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"(defun \\\\([-a-z0-9]+\\\\)\" ((1 15 8 15) (20 31 27 31) (33 45 40 45) (237 251 244 251) (256 267 263 267) (269 281 276 281) (473 487 480 487) (492 503 499 503) (505 517 512 517) (709 723 716 723) (728 739 735 739) (741 753 748 753)) ((741 753 748 753) (728 739 735 739) (709 723 716 723) (505 517 512 517) (492 503 499 503) (473 487 480 487) (269 281 276 281) (256 267 263 267) (237 251 244 251) (33 45 40 45) (20 31 27 31) (1 15 8 15)) ((237 251 244 251)) ((269 281 276 281) (256 267 263 267) (237 251 244 251))) (\"\\\\_<byte-compile\\\\_>\" ((52 64 nil nil) (65 77 nil nil) (288 300 nil nil) (301 313 nil nil) (524 536 nil nil) (537 549 nil nil) (760 772 nil nil) (773 785 nil nil)) ((773 785 nil nil) (760 772 nil nil) (537 549 nil nil) (524 536 nil nil) (301 313 nil nil) (288 300 nil nil) (65 77 nil nil) (52 64 nil nil)) nil ((301 313 nil nil) (288 300 nil nil))) (\"\\\\_<let\\\\*?\\\\_>\" ((107 110 nil nil) (111 115 nil nil) (122 126 nil nil) (128 131 nil nil) (343 346 nil nil) (347 351 nil nil) (358 362 nil nil) (364 367 nil nil) (579 582 nil nil) (583 587 nil nil) (594 598 nil nil) (600 603 nil nil) (815 818 nil nil) (819 823 nil nil) (830 834 nil nil) (836 839 nil nil)) ((836 839 nil nil) (830 834 nil nil) (819 823 nil nil) (815 818 nil nil) (600 603 nil nil) (594 598 nil nil) (583 587 nil nil) (579 582 nil nil) (364 367 nil nil) (358 362 nil nil) (347 351 nil nil) (343 346 nil nil) (128 131 nil nil) (122 126 nil nil) (111 115 nil nil) (107 110 nil nil)) ((107 110 nil nil) (111 115 nil nil) (122 126 nil nil) (128 131 nil nil)) ((364 367 nil nil) (358 362 nil nil) (347 351 nil nil) (343 346 nil nil))) (\"(\\\\(catch\\\\|throw\\\\|defun\\\\|provide\\\\|require\\\\)\" ((1 7 2 7) (20 26 21 26) (33 39 34 39) (133 139 134 139) (140 146 141 146) (147 155 148 155) (237 243 238 243) (256 262 257 262) (269 275 270 275) (369 375 370 375) (376 382 377 382) (383 391 384 391) (473 479 474 479) (492 498 493 498) (505 511 506 511) (605 611 606 611) (612 618 613 618) (619 627 620 627) (709 715 710 715) (728 734 729 734) (741 747 742 747) (841 847 842 847) (848 854 849 854) (855 863 856 863)) ((855 863 856 863) (848 854 849 854) (841 847 842 847) (741 747 742 747) (728 734 729 734) (709 715 710 715) (619 627 620 627) (612 618 613 618) (605 611 606 611) (505 511 506 511) (492 498 493 498) (473 479 474 479) (383 391 384 391) (376 382 377 382) (369 375 370 375) (269 275 270 275) (256 262 257 262) (237 243 238 243) (147 155 148 155) (140 146 141 146) (133 139 134 139) (33 39 34 39) (20 26 21 26) (1 7 2 7)) ((133 139 134 139) (140 146 141 146) (147 155 148 155) (237 243 238 243)) ((383 391 384 391) (376 382 377 382) (369 375 370 375) (269 275 270 275) (256 262 257 262) (237 243 238 243))) (\"x\" ((47 48 nil nil) (91 92 nil nil) (93 94 nil nil) (120 121 nil nil) (283 284 nil nil) (327 328 nil nil) (329 330 nil nil) (356 357 nil nil) (519 520 nil nil) (563 564 nil nil) (565 566 nil nil) (592 593 nil nil) (755 756 nil nil) (799 800 nil nil) (801 802 nil nil) (828 829 nil nil)) ((828 829 nil nil) (801 802 nil nil) (799 800 nil nil) (755 756 nil nil) (592 593 nil nil) (565 566 nil nil) (563 564 nil nil) (519 520 nil nil) (356 357 nil nil) (329 330 nil nil) (327 328 nil nil) (283 284 nil nil) (120 121 nil nil) (93 94 nil nil) (91 92 nil nil) (47 48 nil nil)) ((120 121 nil nil)) ((356 357 nil nil) (329 330 nil nil) (327 328 nil nil) (283 284 nil nil))) (\"y\" ((49 50 nil nil) (53 54 nil nil) (66 67 nil nil) (79 80 nil nil) (95 96 nil nil) (285 286 nil nil) (289 290 nil nil) (302 303 nil nil) (315 316 nil nil) (331 332 nil nil) (521 522 nil nil) (525 526 nil nil) (538 539 nil nil) (551 552 nil nil) (567 568 nil nil) (757 758 nil nil) (761 762 nil nil) (774 775 nil nil) (787 788 nil nil) (803 804 nil nil)) ((803 804 nil nil) (787 788 nil nil) (774 775 nil nil) (761 762 nil nil) (757 758 nil nil) (567 568 nil nil) (551 552 nil nil) (538 539 nil nil) (525 526 nil nil) (521 522 nil nil) (331 332 nil nil) (315 316 nil nil) (302 303 nil nil) (289 290 nil nil) (285 286 nil nil) (95 96 nil nil) (79 80 nil nil) (66 67 nil nil) (53 54 nil nil) (49 50 nil nil)) nil ((331 332 nil nil) (315 316 nil nil) (302 303 nil nil) (289 290 nil nil) (285 286 nil nil))) (\"compile\" ((57 64 nil nil) (70 77 nil nil) (83 90 nil nil) (99 106 nil nil) (293 300 nil nil) (306 313 nil nil) (319 326 nil nil) (335 342 nil nil) (529 536 nil nil) (542 549 nil nil) (555 562 nil nil) (571 578 nil nil) (765 772 nil nil) (778 785 nil nil) (791 798 nil nil) (807 814 nil nil)) ((807 814 nil nil) (791 798 nil nil) (778 785 nil nil) (765 772 nil nil) (571 578 nil nil) (555 562 nil nil) (542 549 nil nil) (529 536 nil nil) (335 342 nil nil) (319 326 nil nil) (306 313 nil nil) (293 300 nil nil) (99 106 nil nil) (83 90 nil nil) (70 77 nil nil) (57 64 nil nil)) nil ((335 342 nil nil) (319 326 nil nil) (306 313 nil nil) (293 300 nil nil))))""#
    ]];
    assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_casefold_scan_string_match_offsets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((case-fold-search t)
       (s (concat (make-string 300 ?.)
                  "(DEFUN a-1) (Key) (defun b) (Deſun c) (Defun d) "
                  (make-string 50 ?-)
                  "(dEfUn e) (dİfun f) BYTE-compile 中(defun g)")))
  (mapcar (lambda (re)
            (list re
                  (mapcar (lambda (start)
                            (let ((pos (string-match re s start)))
                              (list start pos
                                    (and pos (match-end 0))
                                    (and pos (match-beginning 1)))))
                          '(0 100 300 301 312 320 340 360 395 400 420 440))))
          '("(defun \\([-a-z0-9]+\\)"
            "\\(byte\\)-compile"
            "(\\(key\\|defun\\)"
            "k" "K")))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"(defun \\\\([-a-z0-9]+\\\\)\" ((0 300 310 307) (100 300 310 307) (300 300 310 307) (301 318 326 325) (312 318 326 325) (320 338 346 345) (340 398 406 405) (360 398 406 405) (395 398 406 405) (400 432 440 439) (420 432 440 439) (440 nil nil nil))) (\"\\\\(byte\\\\)-compile\" ((0 418 430 418) (100 418 430 418) (300 418 430 418) (301 418 430 418) (312 418 430 418) (320 418 430 418) (340 418 430 418) (360 418 430 418) (395 418 430 418) (400 418 430 418) (420 nil nil nil) (440 nil nil nil))) (\"(\\\\(key\\\\|defun\\\\)\" ((0 300 306 301) (100 300 306 301) (300 300 306 301) (301 318 324 319) (312 318 324 319) (320 338 344 339) (340 398 404 399) (360 398 404 399) (395 398 404 399) (400 432 438 433) (420 432 438 433) (440 nil nil nil))) (\"k\" ((0 nil nil nil) (100 nil nil nil) (300 nil nil nil) (301 nil nil nil) (312 nil nil nil) (320 nil nil nil) (340 nil nil nil) (360 nil nil nil) (395 nil nil nil) (400 nil nil nil) (420 nil nil nil) (440 nil nil nil))) (\"K\" ((0 313 314 nil) (100 313 314 nil) (300 313 314 nil) (301 313 314 nil) (312 313 314 nil) (320 nil nil nil) (340 nil nil nil) (360 nil nil nil) (395 nil nil nil) (400 nil nil nil) (420 nil nil nil) (440 nil nil nil))))""#
    ]];
    assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_casefold_scan_custom_case_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((tbl (copy-case-table (standard-case-table))))
  (set-case-syntax-pair #x212A ?k tbl)
  (set-case-syntax-pair ?\[ ?\] tbl)
  (with-temp-buffer
    (dotimes (_ 3)
      (insert (make-string 100 ?.) "(Key) [x] ]y[ (key) (KEY) (Key)\n"))
    (with-case-table tbl
      (let* ((case-fold-search t)
             (fwd (lambda (re)
                    (goto-char (point-min))
                    (let (acc)
                      (while (re-search-forward re nil t)
                        (push (list (match-beginning 0) (match-end 0)) acc))
                      (nreverse acc))))
             (bwd (lambda (re)
                    (goto-char (point-max))
                    (let (acc)
                      (while (re-search-backward re nil t)
                        (push (list (match-beginning 0) (match-end 0)) acc))
                      (nreverse acc)))))
        (list
         (mapcar (lambda (re) (list re (funcall fwd re) (funcall bwd re)))
                 '("k" "(key)" "(Key)" "\\]" "\\[x\\]" "]y\\["))
         (string-match "(key)" (concat (make-string 300 ?.) "(Key)"))
         (string-match "\\]x" (concat (make-string 300 ?.) "[x")))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"k\" ((102 103) (116 117) (122 123) (128 129) (234 235) (248 249) (254 255) (260 261) (366 367) (380 381) (386 387) (392 393)) ((392 393) (386 387) (380 381) (366 367) (260 261) (254 255) (248 249) (234 235) (128 129) (122 123) (116 117) (102 103))) (\"(key)\" ((101 106) (115 120) (121 126) (127 132) (233 238) (247 252) (253 258) (259 264) (365 370) (379 384) (385 390) (391 396)) ((391 396) (385 390) (379 384) (365 370) (259 264) (253 258) (247 252) (233 238) (127 132) (121 126) (115 120) (101 106))) (\"(Key)\" ((101 106) (115 120) (121 126) (127 132) (233 238) (247 252) (253 258) (259 264) (365 370) (379 384) (385 390) (391 396)) ((391 396) (385 390) (379 384) (365 370) (259 264) (253 258) (247 252) (233 238) (127 132) (121 126) (115 120) (101 106))) (\"\\\\]\" ((107 108) (109 110) (111 112) (113 114) (239 240) (241 242) (243 244) (245 246) (371 372) (373 374) (375 376) (377 378)) ((377 378) (375 376) (373 374) (371 372) (245 246) (243 244) (241 242) (239 240) (113 114) (111 112) (109 110) (107 108))) (\"\\\\[x\\\\]\" ((107 110) (239 242) (371 374)) ((371 374) (239 242) (107 110))) (\"]y\\\\[\" ((111 114) (243 246) (375 378)) ((375 378) (243 246) (111 114)))) 300 300)""#
    ]];
    assert_oracle_parity_expect(form, expect);
}
