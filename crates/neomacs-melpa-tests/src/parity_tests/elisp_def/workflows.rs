use expect_test::expect;

use super::ParityBatchCase;

fn enables_real_navigation_keys_and_restores_major_mode_bindings() -> ParityBatchCase {
    let elisp_form = r##"(edt-test-run
 "elisp-def-mode-keys"
 (lambda (root)
   (let* ((library
           (edt-test-write
            root "ed349-library.el"
            ";;; ed349-library.el --- Lisp-2 fixture -*- lexical-binding: t; -*-\n\n(defvar ed349/dual 41\n  \"A value sharing its symbol with a function.\")\n\n(defun ed349/dual (value)\n  \"Return VALUE plus the shared variable.\"\n  (+ value ed349/dual))\n\n(provide 'ed349-library)\n"))
          (consumer-file
           (edt-test-write
            root "consumer.el"
            ";;; consumer.el --- real key fixture -*- lexical-binding: t; -*-\n\n(ed349/dual 2)\n(setq ed349/dual 42)\n"))
          (emacs-lisp-mode-hook nil)
          consumer before enabled repeated navigation disabled)
     (load library nil 'nomessage t)
     (edt-test-register-feature 'ed349-library)
     (setq consumer (find-file-noselect consumer-file))
     (switch-to-buffer consumer)
     (emacs-lisp-mode)
     (setq before
           (list :mode elisp-def-mode
                 :local (local-variable-p 'elisp-def-mode)
                 :m-dot (key-binding (kbd "M-."))
                 :m-comma (key-binding (kbd "M-,"))
                 :lighter (assq 'elisp-def-mode minor-mode-alist)
                 :text (buffer-substring-no-properties
                        (point-min) (point-max))))
     (elisp-def-mode 1)
     (setq enabled
           (list :return elisp-def-mode
                 :mode elisp-def-mode
                 :local (local-variable-p 'elisp-def-mode)
                 :m-dot (key-binding (kbd "M-."))
                 :m-comma (key-binding (kbd "M-,"))
                 :lighter (assq 'elisp-def-mode minor-mode-alist)))
     (elisp-def-mode 1)
     (setq repeated
           (list :mode elisp-def-mode
                 :map-count
                 (cl-count 'elisp-def-mode minor-mode-map-alist
                           :key #'car :test #'eq)))
     (edt-test-position consumer "ed349/dual" 1)
     (setq navigation (edt-test-jump 'key))
     (elisp-def-mode -1)
     (setq disabled
           (list :mode elisp-def-mode
                 :local (local-variable-p 'elisp-def-mode)
                 :m-dot (key-binding (kbd "M-."))
                 :m-comma (key-binding (kbd "M-,"))))
     (list :before before :enabled enabled :repeated repeated
           :navigation navigation :disabled disabled))))"##;
    let expected = expect![[
        r#"OK (:result (:before (:mode nil :local nil :m-dot xref-find-definitions :m-comma xref-go-back :lighter (elisp-def-mode " ElispDef") :text ";;; consumer.el --- real key fixture -*- lexical-binding: t; -*-\n\n(ed349/dual 2)\n(setq ed349/dual 42)\n") :enabled (:return t :mode t :local t :m-dot elisp-def :m-comma xref-go-back :lighter (elisp-def-mode " ElispDef")) :repeated (:mode t :map-count 1) :navigation (:invocation key :origin (:buffer "consumer.el" :file "consumer.el" :point 68 :line 3 :column 1 :text "(ed349/dual 2)" :symbol ed349/dual :selected t) :public-return (:timerp nil :same-as-new-timer nil) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ed349-library.el" :file "ed349-library.el" :point 149 :line 6 :column 7 :text "(defun ed349/dual (value)" :symbol ed349/dual :selected t) :highlight ((:start 149 :end 159 :face highlight :text "ed349/dual")) :origin-mark (:mark 68 :active t) :jump-history (:backward ((:file "consumer.el" :point 68 :line 3 :column 1)) :forward nil) :back (:location (:buffer "consumer.el" :file "consumer.el" :point 68 :line 3 :column 1 :text "(ed349/dual 2)" :symbol ed349/dual :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ed349-library.el" :point 149 :line 6 :column 7)))) :after-dispatch (:scheduled nil :highlight nil)) :disabled (:mode nil :local t :m-dot xref-find-definitions :m-comma xref-go-back)) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :highlight-overlays-live nil :xref (:backward nil :forward nil) :fixture-features-live nil :runtime-functions-live nil :prefixed-symbols-live nil :root-exists nil :root-owned nil :window-restored t :hook-restored t :load-history-restored t :placeholder-restored t :completion-adapter-restored t :local-variables-disabled t :completion-remaining nil :completion-calls nil :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value(
        "enables_real_navigation_keys_and_restores_major_mode_bindings",
        elisp_form,
        expected,
    )
}

fn navigates_lisp2_globals_and_round_trips_xref_with_real_highlight() -> ParityBatchCase {
    let elisp_form = r##"(edt-test-run
 "elisp-def-lisp2-globals"
 (lambda (root)
   (let* ((library
           (edt-test-write
            root "ed349-globals.el"
            ";;; ed349-globals.el --- Lisp-2 source -*- lexical-binding: t; -*-\n\n(defvar ed349/item 41\n  \"A shared variable value.\")\n\n(defun ed349/item (value)\n  \"Return VALUE plus the shared variable.\"\n  (+ value ed349/item))\n\n(provide 'ed349-globals)\n"))
          (usage-file
           (edt-test-write
            root "global-usage.el"
            ";;; global-usage.el --- Lisp-2 calls -*- lexical-binding: t; -*-\n\n(ed349/item 2)\n(setq ed349/item 42)\n"))
          (emacs-lisp-mode-hook nil)
          usage function variable)
     (load library nil 'nomessage t)
     (edt-test-register-feature 'ed349-globals)
     (setq usage (find-file-noselect usage-file))
     (with-current-buffer usage (emacs-lisp-mode))
     (edt-test-position usage "ed349/item" 1)
     (setq function (edt-test-jump 'command))
     (edt-test-position usage "ed349/item" 2)
     (setq variable (edt-test-jump 'command))
     (list :function function :variable variable))))"##;
    let expected = expect![[
        r#"OK (:result (:function (:invocation command :origin (:buffer "global-usage.el" :file "global-usage.el" :point 68 :line 3 :column 1 :text "(ed349/item 2)" :symbol ed349/item :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ed349-globals.el" :file "ed349-globals.el" :point 129 :line 6 :column 7 :text "(defun ed349/item (value)" :symbol ed349/item :selected t) :highlight ((:start 129 :end 139 :face highlight :text "ed349/item")) :origin-mark (:mark 68 :active t) :jump-history (:backward ((:file "global-usage.el" :point 68 :line 3 :column 1)) :forward nil) :back (:location (:buffer "global-usage.el" :file "global-usage.el" :point 68 :line 3 :column 1 :text "(ed349/item 2)" :symbol ed349/item :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ed349-globals.el" :point 129 :line 6 :column 7)))) :after-dispatch (:scheduled nil :highlight nil)) :variable (:invocation command :origin (:buffer "global-usage.el" :file "global-usage.el" :point 88 :line 4 :column 6 :text "(setq ed349/item 42)" :symbol ed349/item :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ed349-globals.el" :file "ed349-globals.el" :point 77 :line 3 :column 8 :text "(defvar ed349/item 41" :symbol ed349/item :selected t) :highlight ((:start 77 :end 87 :face highlight :text "ed349/item")) :origin-mark (:mark 88 :active t) :jump-history (:backward ((:file "global-usage.el" :point 88 :line 4 :column 6)) :forward nil) :back (:location (:buffer "global-usage.el" :file "global-usage.el" :point 88 :line 4 :column 6 :text "(setq ed349/item 42)" :symbol ed349/item :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ed349-globals.el" :point 77 :line 3 :column 8)))) :after-dispatch (:scheduled nil :highlight nil))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :highlight-overlays-live nil :xref (:backward nil :forward nil) :fixture-features-live nil :runtime-functions-live nil :prefixed-symbols-live nil :root-exists nil :root-owned nil :window-restored t :hook-restored t :load-history-restored t :placeholder-restored t :completion-adapter-restored t :local-variables-disabled t :completion-remaining nil :completion-calls nil :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value(
        "navigates_lisp2_globals_and_round_trips_xref_with_real_highlight",
        elisp_form,
        expected,
    )
}

fn finds_lexical_bindings_in_edited_and_macro_expanded_code() -> ParityBatchCase {
    let elisp_form = r##"(edt-test-run
 "elisp-def-lexical-bindings"
 (lambda (root)
   (let* ((source-file
           (edt-test-write
            root "lexical-workflow.el"
            ";;; lexical-workflow.el --- edited bindings -*- lexical-binding: t; -*-\n\n(defun ed349/locals (argument pair)\n  \"Exercise practical lexical binding navigation.\"\n  (let ((outer 10))\n    (let ((inner (+ outer 1)))\n      inner))\n  (let* ((value 1)\n         (value (+ value 1)))\n    value)\n  (condition-case issue\n      (car nil)\n    (error issue))\n  (cl-destructuring-bind (left right) pair\n    (+ left right))\n  (-let (((head . tail) pair))\n    (list head tail argument)))\n\n(let ((draft 1))\n  (setf draft))\n"))
          (emacs-lisp-mode-hook nil)
          (buffer (find-file-noselect source-file))
          results)
     (with-current-buffer buffer (emacs-lisp-mode))
     (dolist (probe '((parameter "argument" 2)
                      (nested-initializer "outer" 2)
                      (let-star-initializer "value" 3)
                      (let-star-body "value" 4)
                      (condition-handler "issue" 2)
                      (cl-destructured "left" 2)
                      (dash-destructured "head" 2)
                      (unfinished-edit "draft" 2)))
       (edt-test-position buffer (nth 1 probe) (nth 2 probe))
       (push (list (car probe) (edt-test-jump 'command)) results))
     (nreverse results))))"##;
    let expected = expect![[
        r#"OK (:result ((parameter (:invocation command :origin (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 459 :line 17 :column 20 :text "    (list head tail argument)))" :symbol argument :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 95 :line 3 :column 21 :text "(defun ed349/locals (argument pair)" :symbol argument :selected t) :highlight ((:start 95 :end 103 :face highlight :text "argument")) :origin-mark (:mark 459 :active t) :jump-history (:backward ((:file "lexical-workflow.el" :point 459 :line 17 :column 20)) :forward nil) :back (:location (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 459 :line 17 :column 20 :text "    (list head tail argument)))" :symbol argument :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "lexical-workflow.el" :point 95 :line 3 :column 21)))) :after-dispatch (:scheduled nil :highlight nil))) (nested-initializer (:invocation command :origin (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 201 :line 6 :column 20 :text "    (let ((inner (+ outer 1)))" :symbol outer :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 170 :line 5 :column 9 :text "  (let ((outer 10))" :symbol outer :selected t) :highlight ((:start 170 :end 175 :face highlight :text "outer")) :origin-mark (:mark 201 :active t) :jump-history (:backward ((:file "lexical-workflow.el" :point 201 :line 6 :column 20)) :forward nil) :back (:location (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 201 :line 6 :column 20 :text "    (let ((inner (+ outer 1)))" :symbol outer :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "lexical-workflow.el" :point 170 :line 5 :column 9)))) :after-dispatch (:scheduled nil :highlight nil))) (let-star-initializer (:invocation command :origin (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 264 :line 9 :column 19 :text "         (value (+ value 1)))" :symbol value :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 236 :line 8 :column 10 :text "  (let* ((value 1)" :symbol value :selected t) :highlight ((:start 236 :end 241 :face highlight :text "value")) :origin-mark (:mark 264 :active t) :jump-history (:backward ((:file "lexical-workflow.el" :point 264 :line 9 :column 19)) :forward nil) :back (:location (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 264 :line 9 :column 19 :text "         (value (+ value 1)))" :symbol value :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "lexical-workflow.el" :point 236 :line 8 :column 10)))) :after-dispatch (:scheduled nil :highlight nil))) (let-star-body (:invocation command :origin (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 279 :line 10 :column 4 :text "    value)" :symbol value :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 255 :line 9 :column 10 :text "         (value (+ value 1)))" :symbol value :selected t) :highlight ((:start 255 :end 260 :face highlight :text "value")) :origin-mark (:mark 279 :active t) :jump-history (:backward ((:file "lexical-workflow.el" :point 279 :line 10 :column 4)) :forward nil) :back (:location (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 279 :line 10 :column 4 :text "    value)" :symbol value :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "lexical-workflow.el" :point 255 :line 9 :column 10)))) :after-dispatch (:scheduled nil :highlight nil))) (condition-handler (:invocation command :origin (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 337 :line 13 :column 11 :text "    (error issue))" :symbol issue :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 304 :line 11 :column 18 :text "  (condition-case issue" :symbol issue :selected t) :highlight ((:start 304 :end 309 :face highlight :text "issue")) :origin-mark (:mark 337 :active t) :jump-history (:backward ((:file "lexical-workflow.el" :point 337 :line 13 :column 11)) :forward nil) :back (:location (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 337 :line 13 :column 11 :text "    (error issue))" :symbol issue :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "lexical-workflow.el" :point 304 :line 11 :column 18)))) :after-dispatch (:scheduled nil :highlight nil))) (cl-destructured (:invocation command :origin (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 395 :line 15 :column 7 :text "    (+ left right))" :symbol left :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 371 :line 14 :column 26 :text "  (cl-destructuring-bind (left right) pair" :symbol left :selected t) :highlight ((:start 371 :end 375 :face highlight :text "left")) :origin-mark (:mark 395 :active t) :jump-history (:backward ((:file "lexical-workflow.el" :point 395 :line 15 :column 7)) :forward nil) :back (:location (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 395 :line 15 :column 7 :text "    (+ left right))" :symbol left :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "lexical-workflow.el" :point 371 :line 14 :column 26)))) :after-dispatch (:scheduled nil :highlight nil))) (dash-destructured (:invocation command :origin (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 449 :line 17 :column 10 :text "    (list head tail argument)))" :symbol head :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 419 :line 16 :column 11 :text "  (-let (((head . tail) pair))" :symbol head :selected t) :highlight ((:start 419 :end 423 :face highlight :text "head")) :origin-mark (:mark 449 :active t) :jump-history (:backward ((:file "lexical-workflow.el" :point 449 :line 17 :column 10)) :forward nil) :back (:location (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 449 :line 17 :column 10 :text "    (list head tail argument)))" :symbol head :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "lexical-workflow.el" :point 419 :line 16 :column 11)))) :after-dispatch (:scheduled nil :highlight nil))) (unfinished-edit (:invocation command :origin (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 497 :line 20 :column 8 :text "  (setf draft))" :symbol draft :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 479 :line 19 :column 7 :text "(let ((draft 1))" :symbol draft :selected t) :highlight ((:start 479 :end 484 :face highlight :text "draft")) :origin-mark (:mark 497 :active t) :jump-history (:backward ((:file "lexical-workflow.el" :point 497 :line 20 :column 8)) :forward nil) :back (:location (:buffer "lexical-workflow.el" :file "lexical-workflow.el" :point 497 :line 20 :column 8 :text "  (setf draft))" :symbol draft :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "lexical-workflow.el" :point 479 :line 19 :column 7)))) :after-dispatch (:scheduled nil :highlight nil)))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :highlight-overlays-live nil :xref (:backward nil :forward nil) :fixture-features-live nil :runtime-functions-live nil :prefixed-symbols-live nil :root-exists nil :root-owned nil :window-restored t :hook-restored t :load-history-restored t :placeholder-restored t :completion-adapter-restored t :local-variables-disabled t :completion-remaining nil :completion-calls nil :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value(
        "finds_lexical_bindings_in_edited_and_macro_expanded_code",
        elisp_form,
        expected,
    )
}

fn navigates_macro_function_use_generated_definitions_face_and_feature() -> ParityBatchCase {
    let elisp_form = r##"(edt-test-run
 "elisp-def-generated-definitions"
 (lambda (root)
   (let* ((library
           (edt-test-write
            root "library Ω/ed349-generated.el"
            ";;; ed349-generated.el --- generated definitions -*- lexical-binding: t; -*-\n\n(require 'cl-lib)\n\n(defun ed349/transform (value)\n  \"Double VALUE.\"\n  (* value 2))\n\n(defface ed349/notice\n  '((t (:weight bold)))\n  \"A deterministic fixture face.\")\n\n(define-derived-mode ed349-mode fundamental-mode \"ED349\")\n\n(cl-defstruct ed349/point x y)\n\n(provide 'ed349-generated)\n"))
          (usage-file
           (edt-test-write
            root "generated-usage.el"
            ";;; generated-usage.el --- public uses -*- lexical-binding: t; -*-\n\n(require 'ed349-generated)\n(require 'dash)\n\n(->> 7 ed349/transform)\n(make-ed349/point :x 1 :y 2)\ned349-mode-hook\n'ed349/notice\n"))
          (emacs-lisp-mode-hook nil)
          usage results)
     (when (facep 'ed349/notice)
       (error "ELISP-DEF fixture face already exists: ed349/notice"))
     (setq load-path (cons (file-name-directory library) load-path))
     (load library nil 'nomessage t)
     (edt-test-register-feature 'ed349-generated)
     (setq usage (find-file-noselect usage-file))
     (with-current-buffer usage (emacs-lisp-mode))
     (dolist (probe '((threaded-function "ed349/transform" 1)
                      (feature "ed349-generated" 1)
                      (struct-constructor "make-ed349/point" 1)
                      (generated-hook "ed349-mode-hook" 1)
                      (face "ed349/notice" 1)))
       (edt-test-position usage (nth 1 probe) (nth 2 probe))
       (push (list (car probe) (edt-test-jump 'command)) results))
     (nreverse results))))"##;
    let expected = expect![[
        r#"OK (:result ((threaded-function (:invocation command :origin (:buffer "generated-usage.el" :file "generated-usage.el" :point 120 :line 6 :column 7 :text "(->> 7 ed349/transform)" :symbol ed349/transform :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ed349-generated.el" :file "ed349-generated.el" :point 105 :line 5 :column 7 :text "(defun ed349/transform (value)" :symbol ed349/transform :selected t) :highlight ((:start 105 :end 120 :face highlight :text "ed349/transform")) :origin-mark (:mark 120 :active t) :jump-history (:backward ((:file "generated-usage.el" :point 120 :line 6 :column 7)) :forward nil) :back (:location (:buffer "generated-usage.el" :file "generated-usage.el" :point 120 :line 6 :column 7 :text "(->> 7 ed349/transform)" :symbol ed349/transform :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ed349-generated.el" :point 105 :line 5 :column 7)))) :after-dispatch (:scheduled nil :highlight nil))) (feature (:invocation command :origin (:buffer "generated-usage.el" :file "generated-usage.el" :point 79 :line 3 :column 10 :text "(require 'ed349-generated)" :symbol ed349-generated :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ed349-generated.el" :file "ed349-generated.el" :point 345 :line 17 :column 9 :text "(provide 'ed349-generated)" :symbol nil :selected t) :highlight ((:start 345 :end 361 :face highlight :text "'ed349-generated")) :origin-mark (:mark 79 :active t) :jump-history (:backward ((:file "generated-usage.el" :point 79 :line 3 :column 10)) :forward nil) :back (:location (:buffer "generated-usage.el" :file "generated-usage.el" :point 79 :line 3 :column 10 :text "(require 'ed349-generated)" :symbol ed349-generated :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ed349-generated.el" :point 345 :line 17 :column 9)))) :after-dispatch (:scheduled nil :highlight nil))) (struct-constructor (:invocation command :origin (:buffer "generated-usage.el" :file "generated-usage.el" :point 138 :line 7 :column 1 :text "(make-ed349/point :x 1 :y 2)" :symbol make-ed349/point :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ed349-generated.el" :file "ed349-generated.el" :point 318 :line 15 :column 14 :text "(cl-defstruct ed349/point x y)" :symbol ed349/point :selected t) :highlight ((:start 318 :end 329 :face highlight :text "ed349/point")) :origin-mark (:mark 138 :active t) :jump-history (:backward ((:file "generated-usage.el" :point 138 :line 7 :column 1)) :forward nil) :back (:location (:buffer "generated-usage.el" :file "generated-usage.el" :point 138 :line 7 :column 1 :text "(make-ed349/point :x 1 :y 2)" :symbol make-ed349/point :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ed349-generated.el" :point 318 :line 15 :column 14)))) :after-dispatch (:scheduled nil :highlight nil))) (generated-hook (:invocation command :origin (:buffer "generated-usage.el" :file "generated-usage.el" :point 166 :line 8 :column 0 :text "ed349-mode-hook" :symbol ed349-mode-hook :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ed349-generated.el" :file "ed349-generated.el" :point 266 :line 13 :column 21 :text "(define-derived-mode ed349-mode fundamental-mode \"ED349\")" :symbol ed349-mode :selected t) :highlight ((:start 266 :end 276 :face highlight :text "ed349-mode")) :origin-mark (:mark 166 :active t) :jump-history (:backward ((:file "generated-usage.el" :point 166 :line 8 :column 0)) :forward nil) :back (:location (:buffer "generated-usage.el" :file "generated-usage.el" :point 166 :line 8 :column 0 :text "ed349-mode-hook" :symbol ed349-mode-hook :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ed349-generated.el" :point 266 :line 13 :column 21)))) :after-dispatch (:scheduled nil :highlight nil))) (face (:invocation command :origin (:buffer "generated-usage.el" :file "generated-usage.el" :point 183 :line 9 :column 1 :text "'ed349/notice" :symbol ed349/notice :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ed349-generated.el" :file "ed349-generated.el" :point 172 :line 9 :column 9 :text "(defface ed349/notice" :symbol ed349/notice :selected t) :highlight ((:start 172 :end 184 :face highlight :text "ed349/notice")) :origin-mark (:mark 183 :active t) :jump-history (:backward ((:file "generated-usage.el" :point 183 :line 9 :column 1)) :forward nil) :back (:location (:buffer "generated-usage.el" :file "generated-usage.el" :point 183 :line 9 :column 1 :text "'ed349/notice" :symbol ed349/notice :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ed349-generated.el" :point 172 :line 9 :column 9)))) :after-dispatch (:scheduled nil :highlight nil)))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :highlight-overlays-live nil :xref (:backward nil :forward nil) :fixture-features-live nil :runtime-functions-live nil :prefixed-symbols-live nil :root-exists nil :root-owned nil :window-restored t :hook-restored t :load-history-restored t :placeholder-restored t :completion-adapter-restored t :local-variables-disabled t :completion-remaining nil :completion-calls nil :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value(
        "navigates_macro_function_use_generated_definitions_face_and_feature",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn handles_sharp_quote_docstrings_unquote_and_quoted_ambiguity() -> ParityBatchCase {
    let elisp_form = r##"(edt-test-run
 "elisp-def-ergonomics"
 (lambda (root)
   (let* ((library
           (edt-test-write
            root "ed349-ergonomic-library.el"
            ";;; ed349-ergonomic-library.el --- ergonomic globals -*- lexical-binding: t; -*-\n\n(defvar ed349/choice 17\n  \"Variable namespace.\")\n\n(defun ed349/choice (value)\n  \"Function namespace.\"\n  value)\n\n(provide 'ed349-ergonomic-library)\n"))
          (usage-file
           (edt-test-write
            root "ergonomic-usage.el"
            ";;; ergonomic-usage.el --- practical point forms -*- lexical-binding: t; -*-\n\n(defun ed349/ergonomic (argument items)\n  \"Return ARGUMENT.\"\n  (list #'ed349/choice\n        `(payload ,@items)\n        'ed349/choice))\n"))
          (emacs-lisp-mode-hook nil)
          usage sharp docstring unquote ambiguous-function ambiguous-variable)
     (load library nil 'nomessage t)
     (edt-test-register-feature 'ed349-ergonomic-library)
     (setq usage (find-file-noselect usage-file))
     (with-current-buffer usage (emacs-lisp-mode))
     (edt-test-position usage "#'ed349/choice" 1 0)
     (setq sharp (edt-test-jump 'command))
     (edt-test-position usage "ARGUMENT." 1 0)
     (setq docstring (edt-test-jump 'command))
     (edt-test-position usage "@items" 1 1)
     (setq unquote (edt-test-jump 'command))
     (edt-test-position usage "'ed349/choice" 2 1)
     (setq edt-test-completion-plan
           (list (list :prompt
                       "ed349/choice is a function and a variable, choose: "
                       :candidates '("function" "variable")
                       :choice 'function)))
     (let ((completing-read-function #'edt-test-strict-completing-read))
       (setq ambiguous-function (edt-test-jump 'command)))
     (edt-test-position usage "'ed349/choice" 2 1)
     (setq edt-test-completion-plan
           (list (list :prompt
                       "ed349/choice is a function and a variable, choose: "
                       :candidates '("function" "variable")
                       :choice 'variable)))
     (let ((completing-read-function #'edt-test-strict-completing-read))
       (setq ambiguous-variable (edt-test-jump 'command)))
     (list :sharp-quote sharp :docstring docstring :unquote unquote
           :ambiguous-function ambiguous-function
           :ambiguous-variable ambiguous-variable
           :completion-calls (edt-test-completion-calls)))))"##;
    let expected = expect![[
        r#"OK (:result (:sharp-quote (:invocation command :origin (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 148 :line 5 :column 8 :text "  (list #'ed349/choice" :symbol nil :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ed349-ergonomic-library.el" :file "ed349-ergonomic-library.el" :point 140 :line 6 :column 7 :text "(defun ed349/choice (value)" :symbol ed349/choice :selected t) :highlight ((:start 140 :end 152 :face highlight :text "ed349/choice")) :origin-mark (:mark 148 :active t) :jump-history (:backward ((:file "ergonomic-usage.el" :point 148 :line 5 :column 8)) :forward nil) :back (:location (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 148 :line 5 :column 8 :text "  (list #'ed349/choice" :symbol nil :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ed349-ergonomic-library.el" :point 140 :line 6 :column 7)))) :after-dispatch (:scheduled nil :highlight nil)) :docstring (:invocation command :origin (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 129 :line 4 :column 10 :text "  \"Return ARGUMENT.\"" :symbol ARGUMENT. :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 103 :line 3 :column 24 :text "(defun ed349/ergonomic (argument items)" :symbol argument :selected t) :highlight ((:start 103 :end 111 :face highlight :text "argument")) :origin-mark (:mark 129 :active t) :jump-history (:backward ((:file "ergonomic-usage.el" :point 129 :line 4 :column 10)) :forward nil) :back (:location (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 129 :line 4 :column 10 :text "  \"Return ARGUMENT.\"" :symbol ARGUMENT. :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ergonomic-usage.el" :point 103 :line 3 :column 24)))) :after-dispatch (:scheduled nil :highlight nil)) :unquote (:invocation command :origin (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 183 :line 6 :column 20 :text "        `(payload ,@items)" :symbol items :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 112 :line 3 :column 33 :text "(defun ed349/ergonomic (argument items)" :symbol items :selected t) :highlight ((:start 112 :end 117 :face highlight :text "items")) :origin-mark (:mark 183 :active t) :jump-history (:backward ((:file "ergonomic-usage.el" :point 183 :line 6 :column 20)) :forward nil) :back (:location (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 183 :line 6 :column 20 :text "        `(payload ,@items)" :symbol items :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ergonomic-usage.el" :point 112 :line 3 :column 33)))) :after-dispatch (:scheduled nil :highlight nil)) :ambiguous-function (:invocation command :origin (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 199 :line 7 :column 9 :text "        'ed349/choice))" :symbol ed349/choice :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ed349-ergonomic-library.el" :file "ed349-ergonomic-library.el" :point 140 :line 6 :column 7 :text "(defun ed349/choice (value)" :symbol ed349/choice :selected t) :highlight ((:start 140 :end 152 :face highlight :text "ed349/choice")) :origin-mark (:mark 199 :active t) :jump-history (:backward ((:file "ergonomic-usage.el" :point 199 :line 7 :column 9)) :forward nil) :back (:location (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 199 :line 7 :column 9 :text "        'ed349/choice))" :symbol ed349/choice :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ed349-ergonomic-library.el" :point 140 :line 6 :column 7)))) :after-dispatch (:scheduled nil :highlight nil)) :ambiguous-variable (:invocation command :origin (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 199 :line 7 :column 9 :text "        'ed349/choice))" :symbol ed349/choice :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "ed349-ergonomic-library.el" :file "ed349-ergonomic-library.el" :point 91 :line 3 :column 8 :text "(defvar ed349/choice 17" :symbol ed349/choice :selected t) :highlight ((:start 91 :end 103 :face highlight :text "ed349/choice")) :origin-mark (:mark 199 :active t) :jump-history (:backward ((:file "ergonomic-usage.el" :point 199 :line 7 :column 9)) :forward nil) :back (:location (:buffer "ergonomic-usage.el" :file "ergonomic-usage.el" :point 199 :line 7 :column 9 :text "        'ed349/choice))" :symbol ed349/choice :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "ed349-ergonomic-library.el" :point 91 :line 3 :column 8)))) :after-dispatch (:scheduled nil :highlight nil)) :completion-calls (("ed349/choice is a function and a variable, choose: " #'variable nil t nil nil nil nil) ("ed349/choice is a function and a variable, choose: " #'variable nil t nil nil nil nil))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :highlight-overlays-live nil :xref (:backward nil :forward nil) :fixture-features-live nil :runtime-functions-live nil :prefixed-symbols-live nil :root-exists nil :root-owned nil :window-restored t :hook-restored t :load-history-restored t :placeholder-restored t :completion-adapter-restored t :local-variables-disabled t :completion-remaining nil :completion-calls (("ed349/choice is a function and a variable, choose: " #'variable nil t nil nil nil nil) ("ed349/choice is a function and a variable, choose: " #'variable nil t nil nil nil nil)) :body-error nil :cleanup-errors nil))"#
    ]];
    ParityBatchCase::value(
        "handles_sharp_quote_docstrings_unquote_and_quoted_ambiguity",
        elisp_form,
        expected,
    )
}

fn reports_missing_definitions_and_documented_analysis_limits_exactly() -> ParityBatchCase {
    let elisp_form = r##"(edt-test-run
 "elisp-def-failures-limits"
 (lambda (root)
   (let* ((source-file
           (edt-test-write
            root "failure-usage.el"
            ";;; failure-usage.el --- exact public boundaries -*- lexical-binding: t; -*-\n\n'ed349/absent\n#'ed349/runtime-only\n(ed349/missing 1)\n(+ ed349/missing 1)\n\n(-let ((item 1)\n       (item 2))\n  (+ item 3))\n\n(cl-labels ((local-fn (value) (+ value 1)))\n  (local-fn 3))\n"))
          (emacs-lisp-mode-hook nil)
          (buffer (find-file-noselect source-file))
          unknown-quoted runtime-only function-missing variable-missing
          dash-limit labels-limit end-boundary)
     (with-current-buffer buffer (emacs-lisp-mode))
     (edt-test-fset 'ed349/runtime-only (lambda (value) value))
     (edt-test-reset-navigation buffer)
     (edt-test-position buffer "'ed349/absent" 1 1)
     (setq unknown-quoted (edt-test-failure))
     (edt-test-reset-navigation buffer)
     (edt-test-position buffer "#'ed349/runtime-only" 1 0)
     (setq runtime-only (edt-test-failure))
     (edt-test-reset-navigation buffer)
     (edt-test-position buffer "ed349/missing" 1 0)
     (setq function-missing (edt-test-failure))
     (edt-test-reset-navigation buffer)
     (edt-test-position buffer "ed349/missing" 2 0)
     (setq variable-missing (edt-test-failure))
     (edt-test-reset-navigation buffer)
     (edt-test-position buffer "item" 3 0)
     (setq dash-limit (edt-test-jump 'command))
     (edt-test-reset-navigation buffer)
     (edt-test-position buffer "local-fn" 2 0)
     (setq labels-limit (edt-test-failure))
     (let ((end-buffer (generate-new-buffer " *ed349-end-boundary*")))
       (switch-to-buffer end-buffer)
       (emacs-lisp-mode)
       (insert "c-basic-offset")
       (goto-char (point-max))
       (edt-test-reset-navigation end-buffer)
       (setq end-boundary (edt-test-jump 'command)))
     (list :unknown-quoted-before-push unknown-quoted
           :runtime-only-after-push runtime-only
           :function-missing function-missing
           :variable-missing variable-missing
           :dash-duplicate-limit dash-limit
           :cl-labels-limit labels-limit
           :end-boundary end-boundary))))"##;
    let expected = expect![[
        r##"OK (:result (:unknown-quoted-before-push (:condition (:signal user-error :data ("Couldn’t identify where ed349/absent is defined") :message "Couldn’t identify where ed349/absent is defined") :before (:location (:buffer "failure-usage.el" :file "failure-usage.el" :point 80 :line 3 :column 1 :text "'ed349/absent" :symbol ed349/absent :selected t) :mark nil :mark-active nil :xref (:backward nil :forward nil)) :after (:location (:buffer "failure-usage.el" :file "failure-usage.el" :point 80 :line 3 :column 1 :text "'ed349/absent" :symbol ed349/absent :selected t) :same-buffer t :same-point t :mark nil :mark-active nil :xref (:backward nil :forward nil)) :new-timers 0 :highlight nil) :runtime-only-after-push (:condition (:signal user-error :data ("Couldn’t find definition for function ed349/runtime-only") :message "Couldn’t find definition for function ed349/runtime-only") :before (:location (:buffer "failure-usage.el" :file "failure-usage.el" :point 93 :line 4 :column 0 :text "#'ed349/runtime-only" :symbol nil :selected t) :mark nil :mark-active nil :xref (:backward nil :forward nil)) :after (:location (:buffer "failure-usage.el" :file "failure-usage.el" :point 93 :line 4 :column 0 :text "#'ed349/runtime-only" :symbol nil :selected t) :same-buffer t :same-point t :mark 93 :mark-active t :xref (:backward ((:file "failure-usage.el" :point 93 :line 4 :column 0)) :forward nil)) :new-timers 0 :highlight nil) :function-missing (:condition (:signal user-error :data ("Couldn’t find definition for function ed349/missing") :message "Couldn’t find definition for function ed349/missing") :before (:location (:buffer "failure-usage.el" :file "failure-usage.el" :point 115 :line 5 :column 1 :text "(ed349/missing 1)" :symbol ed349/missing :selected t) :mark nil :mark-active nil :xref (:backward nil :forward nil)) :after (:location (:buffer "failure-usage.el" :file "failure-usage.el" :point 115 :line 5 :column 1 :text "(ed349/missing 1)" :symbol ed349/missing :selected t) :same-buffer t :same-point t :mark 115 :mark-active t :xref (:backward ((:file "failure-usage.el" :point 115 :line 5 :column 1)) :forward nil)) :new-timers 0 :highlight nil) :variable-missing (:condition (:signal user-error :data ("Couldn’t find definition for variable ed349/missing") :message "Couldn’t find definition for variable ed349/missing") :before (:location (:buffer "failure-usage.el" :file "failure-usage.el" :point 135 :line 6 :column 3 :text "(+ ed349/missing 1)" :symbol ed349/missing :selected t) :mark nil :mark-active nil :xref (:backward nil :forward nil)) :after (:location (:buffer "failure-usage.el" :file "failure-usage.el" :point 135 :line 6 :column 3 :text "(+ ed349/missing 1)" :symbol ed349/missing :selected t) :same-buffer t :same-point t :mark 135 :mark-active t :xref (:backward ((:file "failure-usage.el" :point 135 :line 6 :column 3)) :forward nil)) :new-timers 0 :highlight nil) :dash-duplicate-limit (:invocation command :origin (:buffer "failure-usage.el" :file "failure-usage.el" :point 191 :line 10 :column 5 :text "  (+ item 3))" :symbol item :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "failure-usage.el" :file "failure-usage.el" :point 161 :line 8 :column 8 :text "(-let ((item 1)" :symbol item :selected t) :highlight ((:start 161 :end 165 :face highlight :text "item")) :origin-mark (:mark 191 :active t) :jump-history (:backward ((:file "failure-usage.el" :point 191 :line 10 :column 5)) :forward nil) :back (:location (:buffer "failure-usage.el" :file "failure-usage.el" :point 191 :line 10 :column 5 :text "  (+ item 3))" :symbol item :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "failure-usage.el" :point 161 :line 8 :column 8)))) :after-dispatch (:scheduled nil :highlight nil)) :cl-labels-limit (:condition (:signal user-error :data ("Couldn’t find definition for function local-fn") :message "Couldn’t find definition for function local-fn") :before (:location (:buffer "failure-usage.el" :file "failure-usage.el" :point 248 :line 13 :column 3 :text "  (local-fn 3))" :symbol local-fn :selected t) :mark nil :mark-active nil :xref (:backward nil :forward nil)) :after (:location (:buffer "failure-usage.el" :file "failure-usage.el" :point 248 :line 13 :column 3 :text "  (local-fn 3))" :symbol local-fn :selected t) :same-buffer t :same-point t :mark 248 :mark-active t :xref (:backward ((:file "failure-usage.el" :point 248 :line 13 :column 3)) :forward nil)) :new-timers 0 :highlight nil) :end-boundary (:invocation command :origin (:buffer " *ed349-end-boundary*" :file nil :point 15 :line 1 :column 14 :text "c-basic-offset" :symbol c-basic-offset :selected t) :public-return (:timerp t :same-as-new-timer t) :timer (:new-count 1 :scheduled-before t :remaining-delay-tenths 5) :target (:buffer "cc-vars.el" :file "cc-vars.el" :point 19065 :line 481 :column 22 :text "(defcustom-c-stylevar c-basic-offset 4" :symbol c-basic-offset :selected t) :highlight ((:start 19065 :end 19079 :face highlight :text "c-basic-offset")) :origin-mark (:mark 15 :active t) :jump-history (:backward ((:file nil :point 15 :line 1 :column 14)) :forward nil) :back (:location (:buffer " *ed349-end-boundary*" :file nil :point 15 :line 1 :column 14 :text "c-basic-offset" :symbol c-basic-offset :selected t) :same-buffer t :same-point t :history (:backward nil :forward ((:file "cc-vars.el" :point 19065 :line 481 :column 22)))) :after-dispatch (:scheduled nil :highlight nil))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :highlight-overlays-live nil :xref (:backward nil :forward nil) :fixture-features-live nil :runtime-functions-live nil :prefixed-symbols-live nil :root-exists nil :root-owned nil :window-restored t :hook-restored t :load-history-restored t :placeholder-restored t :completion-adapter-restored t :local-variables-disabled t :completion-remaining nil :completion-calls nil :body-error nil :cleanup-errors nil))"##
    ]];
    ParityBatchCase::value(
        "reports_missing_definitions_and_documented_analysis_limits_exactly",
        elisp_form,
        expected,
    )
}

pub(super) fn public_workflow_cases() -> Vec<ParityBatchCase> {
    vec![
        enables_real_navigation_keys_and_restores_major_mode_bindings(),
        navigates_lisp2_globals_and_round_trips_xref_with_real_highlight(),
        finds_lexical_bindings_in_edited_and_macro_expanded_code(),
        navigates_macro_function_use_generated_definitions_face_and_feature(),
        handles_sharp_quote_docstrings_unquote_and_quoted_ambiguity(),
        reports_missing_definitions_and_documented_analysis_limits_exactly(),
    ]
}
