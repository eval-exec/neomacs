use expect_test::expect;

use super::ParityBatchCase;

fn close_comments_update_idempotently_preserve_properties_and_delete() -> ParityBatchCase {
    let elisp_form = r##"(hcss-test-run
 "helm-css-scss-close-comments"
 (lambda (_root)
   (let ((buffer (hcss-test-buffer
                  " *hcss-close-comments.scss*" #'scss-mode
                  hcss-test-nested-fixture))
         deep depth-two repeated depth-one depth-zero)
     (with-current-buffer buffer
       (setq-local helm-css-scss-include-commented-selector nil)
       (set-buffer-modified-p nil)
       (setq deep
             (list
              :return (helm-css-scss-insert-close-comment 4)
              :buffer (buffer-string) :point (point)
              :modified (buffer-modified-p)
              :count (hcss-test-generated-comment-count)
              :face-runs (hcss-test-face-runs)))
       (setq depth-two
             (list
              :return (helm-css-scss-insert-close-comment 2)
              :buffer (buffer-string) :point (point)
              :modified (buffer-modified-p)
              :count (hcss-test-generated-comment-count)
              :face-runs (hcss-test-face-runs)))
       (let ((before (buffer-string)))
         (setq repeated
               (list
                :return (helm-css-scss-insert-close-comment 2)
                :identical (equal before (buffer-string))
                :buffer (buffer-string) :point (point)
                :count (hcss-test-generated-comment-count)
                :face-runs (hcss-test-face-runs))))
       (goto-char (point-min))
       (search-forward ".card")
       (replace-match ".panel")
       (setq depth-one
             (list
              :return (helm-css-scss-insert-close-comment 1)
              :buffer (buffer-string) :point (point)
              :count (hcss-test-generated-comment-count)
              :old-card-marker (and (string-match-p "/\\*__.*card" (buffer-string)) t)
              :face-runs (hcss-test-face-runs)))
       (setq depth-zero
             (list
              :return (helm-css-scss-insert-close-comment 0)
              :buffer (buffer-string) :point (point)
              :count (hcss-test-generated-comment-count)
              :face-runs (hcss-test-face-runs))))
     (list :depth-four deep :depth-two depth-two :repeat repeated
           :depth-one depth-one :depth-zero depth-zero))))"##;
    let expect = expect![[
        r#"OK (:result (:depth-four (:return nil :buffer #("/* .disabled {\n  color: gray;\n} */\n\n.dashboard,\n.dashboard--compact {\n  color: red;\n\n  .card {\n    padding: 1rem;\n\n    &__title,\n    &__subtitle {\n      color: blue;\n    } /*__ 12: .dashboard, .dashboard--compact .card &__title, &__subtitle */\n\n    .toolbar[data-state=\"ready\"] {\n      &:hover {\n        opacity: .8;\n      } /*__ 18: .dashboard, .dashboard--compact .card .toolbar[data-state=\"ready\"] &:hover */\n    } /*__ 17: .dashboard, .dashboard--compact .card .toolbar[data-state=\"ready\"] */\n  } /*__ 9: .dashboard, .dashboard--compact .card */\n} /*__ 5: .dashboard, .dashboard--compact */\n\n.footer {\n  color: black;\n} /*__ 25: .footer */\n\n@media (min-width: 80rem) {\n  .wide-panel {\n    display: grid;\n  } /*__ 30: @media (min-width: 80rem) .wide-panel */\n} /*__ 29: @media (min-width: 80rem) */\n" 177 179 (face font-lock-function-name-face) 181 212 (face helm-css-scss-selector-depth-face-1) 213 218 (face helm-css-scss-selector-depth-face-2) 219 240 (face helm-css-scss-selector-depth-face-3) 330 332 (face font-lock-function-name-face) 334 365 (face helm-css-scss-selector-depth-face-1) 366 371 (face helm-css-scss-selector-depth-face-2) 372 400 (face helm-css-scss-selector-depth-face-3) 401 408 (face helm-css-scss-selector-depth-face-4) 423 425 (face font-lock-function-name-face) 427 458 (face helm-css-scss-selector-depth-face-1) 459 464 (face helm-css-scss-selector-depth-face-2) 465 493 (face helm-css-scss-selector-depth-face-3) 506 507 (face font-lock-function-name-face) 509 540 (face helm-css-scss-selector-depth-face-1) 541 546 (face helm-css-scss-selector-depth-face-2) 557 558 (face font-lock-function-name-face) 560 591 (face helm-css-scss-selector-depth-face-1) 629 631 (face font-lock-function-name-face) 633 640 (face helm-css-scss-selector-depth-face-1) 717 719 (face font-lock-function-name-face) 721 746 (face helm-css-scss-selector-depth-face-1) 747 758 (face helm-css-scss-selector-depth-face-2) 769 771 (face font-lock-function-name-face) 773 798 (face helm-css-scss-selector-depth-face-1)) :point 803 :modified t :count 8 :face-runs ((178 180 font-lock-function-name-face "12") (182 213 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (214 219 helm-css-scss-selector-depth-face-2 ".card") (220 241 helm-css-scss-selector-depth-face-3 "&__title, &__subtitle") (331 333 font-lock-function-name-face "18") (335 366 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (367 372 helm-css-scss-selector-depth-face-2 ".card") (373 401 helm-css-scss-selector-depth-face-3 ".toolbar[data-state=\"ready\"]") (402 409 helm-css-scss-selector-depth-face-4 "&:hover") (424 426 font-lock-function-name-face "17") (428 459 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (460 465 helm-css-scss-selector-depth-face-2 ".card") (466 494 helm-css-scss-selector-depth-face-3 ".toolbar[data-state=\"ready\"]") (507 508 font-lock-function-name-face "9") (510 541 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (542 547 helm-css-scss-selector-depth-face-2 ".card") (558 559 font-lock-function-name-face "5") (561 592 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (630 632 font-lock-function-name-face "25") (634 641 helm-css-scss-selector-depth-face-1 ".footer") (718 720 font-lock-function-name-face "30") (722 747 helm-css-scss-selector-depth-face-1 "@media (min-width: 80rem)") (748 759 helm-css-scss-selector-depth-face-2 ".wide-panel") (770 772 font-lock-function-name-face "29") (774 799 helm-css-scss-selector-depth-face-1 "@media (min-width: 80rem)"))) :depth-two (:return nil :buffer #("/* .disabled {\n  color: gray;\n} */\n\n.dashboard,\n.dashboard--compact {\n  color: red;\n\n  .card {\n    padding: 1rem;\n\n    &__title,\n    &__subtitle {\n      color: blue;\n    }\n\n    .toolbar[data-state=\"ready\"] {\n      &:hover {\n        opacity: .8;\n      }\n    }\n  } /*__ 9: .dashboard, .dashboard--compact .card */\n} /*__ 5: .dashboard, .dashboard--compact */\n\n.footer {\n  color: black;\n} /*__ 25: .footer */\n\n@media (min-width: 80rem) {\n  .wide-panel {\n    display: grid;\n  } /*__ 30: @media (min-width: 80rem) .wide-panel */\n} /*__ 29: @media (min-width: 80rem) */\n" 268 269 (face font-lock-function-name-face) 271 302 (face helm-css-scss-selector-depth-face-1) 303 308 (face helm-css-scss-selector-depth-face-2) 319 320 (face font-lock-function-name-face) 322 353 (face helm-css-scss-selector-depth-face-1) 391 393 (face font-lock-function-name-face) 395 402 (face helm-css-scss-selector-depth-face-1) 479 481 (face font-lock-function-name-face) 483 508 (face helm-css-scss-selector-depth-face-1) 509 520 (face helm-css-scss-selector-depth-face-2) 531 533 (face font-lock-function-name-face) 535 560 (face helm-css-scss-selector-depth-face-1)) :point 565 :modified t :count 5 :face-runs ((269 270 font-lock-function-name-face "9") (272 303 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (304 309 helm-css-scss-selector-depth-face-2 ".card") (320 321 font-lock-function-name-face "5") (323 354 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (392 394 font-lock-function-name-face "25") (396 403 helm-css-scss-selector-depth-face-1 ".footer") (480 482 font-lock-function-name-face "30") (484 509 helm-css-scss-selector-depth-face-1 "@media (min-width: 80rem)") (510 521 helm-css-scss-selector-depth-face-2 ".wide-panel") (532 534 font-lock-function-name-face "29") (536 561 helm-css-scss-selector-depth-face-1 "@media (min-width: 80rem)"))) :repeat (:return nil :identical t :buffer #("/* .disabled {\n  color: gray;\n} */\n\n.dashboard,\n.dashboard--compact {\n  color: red;\n\n  .card {\n    padding: 1rem;\n\n    &__title,\n    &__subtitle {\n      color: blue;\n    }\n\n    .toolbar[data-state=\"ready\"] {\n      &:hover {\n        opacity: .8;\n      }\n    }\n  } /*__ 9: .dashboard, .dashboard--compact .card */\n} /*__ 5: .dashboard, .dashboard--compact */\n\n.footer {\n  color: black;\n} /*__ 25: .footer */\n\n@media (min-width: 80rem) {\n  .wide-panel {\n    display: grid;\n  } /*__ 30: @media (min-width: 80rem) .wide-panel */\n} /*__ 29: @media (min-width: 80rem) */\n" 268 269 (face font-lock-function-name-face) 271 302 (face helm-css-scss-selector-depth-face-1) 303 308 (face helm-css-scss-selector-depth-face-2) 319 320 (face font-lock-function-name-face) 322 353 (face helm-css-scss-selector-depth-face-1) 391 393 (face font-lock-function-name-face) 395 402 (face helm-css-scss-selector-depth-face-1) 479 481 (face font-lock-function-name-face) 483 508 (face helm-css-scss-selector-depth-face-1) 509 520 (face helm-css-scss-selector-depth-face-2) 531 533 (face font-lock-function-name-face) 535 560 (face helm-css-scss-selector-depth-face-1)) :point 565 :count 5 :face-runs ((269 270 font-lock-function-name-face "9") (272 303 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (304 309 helm-css-scss-selector-depth-face-2 ".card") (320 321 font-lock-function-name-face "5") (323 354 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (392 394 font-lock-function-name-face "25") (396 403 helm-css-scss-selector-depth-face-1 ".footer") (480 482 font-lock-function-name-face "30") (484 509 helm-css-scss-selector-depth-face-1 "@media (min-width: 80rem)") (510 521 helm-css-scss-selector-depth-face-2 ".wide-panel") (532 534 font-lock-function-name-face "29") (536 561 helm-css-scss-selector-depth-face-1 "@media (min-width: 80rem)"))) :depth-one (:return nil :buffer #("/* .disabled {\n  color: gray;\n} */\n\n.dashboard,\n.dashboard--compact {\n  color: red;\n\n  .panel {\n    padding: 1rem;\n\n    &__title,\n    &__subtitle {\n      color: blue;\n    }\n\n    .toolbar[data-state=\"ready\"] {\n      &:hover {\n        opacity: .8;\n      }\n    }\n  }\n} /*__ 5: .dashboard, .dashboard--compact */\n\n.footer {\n  color: black;\n} /*__ 25: .footer */\n\n@media (min-width: 80rem) {\n  .wide-panel {\n    display: grid;\n  }\n} /*__ 29: @media (min-width: 80rem) */\n" 271 272 (face font-lock-function-name-face) 274 305 (face helm-css-scss-selector-depth-face-1) 343 345 (face font-lock-function-name-face) 347 354 (face helm-css-scss-selector-depth-face-1) 433 435 (face font-lock-function-name-face) 437 462 (face helm-css-scss-selector-depth-face-1)) :point 94 :count 3 :old-card-marker nil :face-runs ((272 273 font-lock-function-name-face "5") (275 306 helm-css-scss-selector-depth-face-1 ".dashboard, .dashboard--compact") (344 346 font-lock-function-name-face "25") (348 355 helm-css-scss-selector-depth-face-1 ".footer") (434 436 font-lock-function-name-face "29") (438 463 helm-css-scss-selector-depth-face-1 "@media (min-width: 80rem)"))) :depth-zero (:return nil :buffer "/* .disabled {\n  color: gray;\n} */\n\n.dashboard,\n.dashboard--compact {\n  color: red;\n\n  .panel {\n    padding: 1rem;\n\n    &__title,\n    &__subtitle {\n      color: blue;\n    }\n\n    .toolbar[data-state=\"ready\"] {\n      &:hover {\n        opacity: .8;\n      }\n    }\n  }\n}\n\n.footer {\n  color: black;\n}\n\n@media (min-width: 80rem) {\n  .wide-panel {\n    display: grid;\n  }\n}\n" :point 94 :count 0 :face-runs nil)) :cleanup (:owned-live nil :root-exists nil :overlay-buffer nil :invisible-targets nil :session-advices (nil nil nil nil nil nil) :session-hook nil :helm-alive nil :cache-hook-count 1))"#
    ]];
    ParityBatchCase::value(
        "close_comments_update_idempotently_preserve_properties_and_delete",
        elisp_form,
        expect,
    )
}

fn public_navigation_covers_css_scss_less_comments_and_boundaries() -> ParityBatchCase {
    let elisp_form = r##"(hcss-test-run
 "helm-css-scss-navigation"
 (lambda (_root)
   (let ((css (hcss-test-buffer
               " *hcss-navigation.css*" #'css-mode
               "/* .disabled { color: gray; } */\n.alpha,\n.beta { color: red; }\n.gamma { color: blue; }\n"))
         (scss (hcss-test-buffer
                " *hcss-navigation.scss*" #'scss-mode
                ".shell {\n  .panel { color: red; }\n}\n.footer { color: black; }\n"))
         (less (hcss-test-buffer
                " *hcss-navigation.less*" #'less-css-mode
                ".theme {\n  .link { color: blue; }\n}\n.release { color: green; }\n"))
         css-next css-previous commented scss-next less-next)
     (with-current-buffer css
       (setq-local helm-css-scss-include-commented-selector nil)
       (goto-char (point-min))
       (dotimes (_ 3)
         (push (hcss-test-movement-state
                #'helm-css-scss-move-and-echo-next-selector)
               css-next))
       (goto-char (point-max))
       (dotimes (_ 3)
         (push (hcss-test-movement-state
                #'helm-css-scss-move-and-echo-previous-selector)
               css-previous))
       (setq-local helm-css-scss-include-commented-selector t)
       (goto-char (point-min))
       (setq commented
             (hcss-test-movement-state
              #'helm-css-scss-move-and-echo-next-selector)))
     (with-current-buffer scss
       (setq-local helm-css-scss-include-commented-selector nil)
       (goto-char (point-min))
       (dotimes (_ 4)
         (push (hcss-test-movement-state
                #'helm-css-scss-move-and-echo-next-selector)
               scss-next)))
     (with-current-buffer less
       (setq-local helm-css-scss-include-commented-selector nil)
       (goto-char (point-min))
       (dotimes (_ 4)
         (push (hcss-test-movement-state
                #'helm-css-scss-move-and-echo-next-selector)
               less-next)))
     (list :css-next (nreverse css-next)
           :css-previous (nreverse css-previous)
           :commented commented
           :scss-next (nreverse scss-next)
           :less-next (nreverse less-next)))))"##;
    let expect = expect![[
        r#"OK (:result (:css-next ((:return ".alpha, .beta" :message nil :point 48 :line 3 :column 6 :char-after 123) (:return ".gamma" :message nil :point 71 :line 4 :column 7 :char-after 123) (:return "No more exist the next target from here" :message nil :point 88 :line 5 :column 0 :char-after nil)) :css-previous ((:return ".gamma" :message nil :point 71 :line 4 :column 7 :char-after 123) (:return ".alpha, .beta" :message nil :point 48 :line 3 :column 6 :char-after 123) (:return "No more exist the previous target from here" :message nil :point 1 :line 1 :column 0 :char-after 47)) :commented (:return "/* .disabled" :message nil :point 14 :line 1 :column 13 :char-after 123) :scss-next ((:return ".shell" :message nil :point 8 :line 1 :column 7 :char-after 123) (:return ".panel" :message nil :point 19 :line 2 :column 9 :char-after 123) (:return ".footer" :message nil :point 45 :line 4 :column 8 :char-after 123) (:return "No more exist the next target from here" :message nil :point 63 :line 5 :column 0 :char-after nil)) :less-next ((:return ".theme" :message nil :point 8 :line 1 :column 7 :char-after 123) (:return ".link" :message nil :point 18 :line 2 :column 8 :char-after 123) (:return ".release" :message nil :point 46 :line 4 :column 9 :char-after 123) (:return "No more exist the next target from here" :message nil :point 64 :line 5 :column 0 :char-after nil))) :cleanup (:owned-live nil :root-exists nil :overlay-buffer nil :invisible-targets nil :session-advices (nil nil nil nil nil nil) :session-hook nil :helm-alive nil :cache-hook-count 1))"#
    ]];
    ParityBatchCase::value(
        "public_navigation_covers_css_scss_less_comments_and_boundaries",
        elisp_form,
        expect,
    )
}

fn malformed_source_preserves_exact_real_edit_and_condition() -> ParityBatchCase {
    let elisp_form = r##"(hcss-test-run
 "helm-css-scss-malformed-source"
 (lambda (_root)
   (let ((buffer (hcss-test-buffer
                  " *hcss-malformed.css*" #'css-mode
                  ".broken {\n  color: red;\n")))
     (with-current-buffer buffer
       (setq-local helm-css-scss-include-commented-selector nil)
       (set-buffer-modified-p nil)
       (let ((before (buffer-string))
             (before-point (point)))
         (list
          :condition
          (hcss-test-capture
           (lambda () (helm-css-scss-insert-close-comment 2)))
          :unchanged (equal before (buffer-string))
          :buffer (buffer-string)
          :point-before before-point :point-after (point)
          :modified (buffer-modified-p)))))))"##;
    let expect = expect![[
        r#"OK (:result (:condition (:signal scan-error :data ("Unbalanced parentheses" 9 25)) :unchanged t :buffer ".broken {\n  color: red;\n" :point-before 25 :point-after 25 :modified nil) :cleanup (:owned-live nil :root-exists nil :overlay-buffer nil :invisible-targets nil :session-advices (nil nil nil nil nil nil) :session-hook nil :helm-alive nil :cache-hook-count 1))"#
    ]];
    ParityBatchCase::value(
        "malformed_source_preserves_exact_real_edit_and_condition",
        elisp_form,
        expect,
    )
}

pub(super) fn public_workflow_cases() -> Vec<ParityBatchCase> {
    vec![
        close_comments_update_idempotently_preserve_properties_and_delete(),
        public_navigation_covers_css_scss_less_comments_and_boundaries(),
        malformed_source_preserves_exact_real_edit_and_condition(),
    ]
}
