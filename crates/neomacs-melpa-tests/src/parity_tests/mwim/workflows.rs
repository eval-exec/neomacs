use expect_test::expect;

use super::ParityBatchCase;

fn logical_code_comment_cycles_and_prefixes() -> ParityBatchCase {
    let elisp_form = r##"(mwim358-test-run
 "logical-cycle"
 (lambda ()
   (mwim358-test-own-buffer
    "logical" #'emacs-lisp-mode
    "  (message \"界 alpha\")   ; note Ω  \n      ;; full comment Ω  \nplain tail   \n")
   (let ((keys
          (mwim358-test-bind-keys
           '(("C-a" . mwim-beginning-of-code-or-line)
             ("C-e" . mwim-end-of-code-or-line)
             ("M-a" . mwim-beginning-of-code-or-line-or-comment))))
         start beginning end three prefix)
     (mwim358-test-command-loop
      (lambda ()
        (goto-char 12)
        (setq start (mwim358-test-position 'start))
        (dotimes (_ 3)
          (execute-kbd-macro (kbd "C-a"))
          (push (mwim358-test-position 'beginning) beginning))
        (goto-char 12)
        (dotimes (_ 3)
          (execute-kbd-macro (kbd "C-e"))
          (push (mwim358-test-position 'end) end))
        (goto-char 12)
        (dotimes (_ 4)
          (execute-kbd-macro (kbd "M-a"))
          (push (mwim358-test-position 'three) three))
        (goto-char (point-min))
        (execute-kbd-macro (kbd "C-u 2 C-a"))
        (push (mwim358-test-position 'plus-two) prefix)
        (execute-kbd-macro (kbd "C-u - 1 C-e"))
        (push (mwim358-test-position 'minus-one) prefix)))
     (list :keys keys :start start
           :activation
           (list :feature (featurep 'mwim)
                 :source (file-name-nondirectory
                          (symbol-file 'mwim 'defun))
                 :seq (package-built-in-p 'seq '(2 24))
                 :load-suffixes load-suffixes)
           :beginning (nreverse beginning)
           :end (nreverse end)
           :three (nreverse three)
           :prefix (nreverse prefix)
           :text (buffer-string)
           :modified (buffer-modified-p)
           :undo buffer-undo-list))))"##;
    ParityBatchCase::value(
        "logical_code_comment_cycles_and_prefixes",
        elisp_form,
        expect![[
            r#"OK (:result (:keys (("C-a" . mwim-beginning-of-code-or-line) ("C-e" . mwim-end-of-code-or-line) ("M-a" . mwim-beginning-of-code-or-line-or-comment)) :start (start :point 12 :line 1 :column 11 :before " " :after "\"" :mark nil :active nil) :activation (:feature t :source "mwim.el" :seq t :load-suffixes (".el")) :beginning ((beginning :point 3 :line 1 :column 2 :before " " :after "(" :mark nil :active nil) (beginning :point 1 :line 1 :column 0 :before nil :after " " :mark nil :active nil) (beginning :point 3 :line 1 :column 2 :before " " :after "(" :mark nil :active nil)) :end ((end :point 22 :line 1 :column 22 :before ")" :after " " :mark nil :active nil) (end :point 35 :line 1 :column 35 :before " " :after "\n" :mark nil :active nil) (end :point 22 :line 1 :column 22 :before ")" :after " " :mark nil :active nil)) :three ((three :point 3 :line 1 :column 2 :before " " :after "(" :mark nil :active nil) (three :point 1 :line 1 :column 0 :before nil :after " " :mark nil :active nil) (three :point 27 :line 1 :column 27 :before " " :after "n" :mark nil :active nil) (three :point 3 :line 1 :column 2 :before " " :after "(" :mark nil :active nil)) :prefix ((plus-two :point 62 :line 3 :column 0 :before "\n" :after "p" :mark nil :active nil) (minus-one :point 59 :line 2 :column 23 :before "Ω" :after " " :mark nil :active nil)) :text "  (message \"界 alpha\")   ; note Ω  \n      ;; full comment Ω  \nplain tail   \n" :modified nil :undo nil) :cleanup (:new-buffers nil :new-processes nil :new-timers nil :owned-live (nil) :window t :current-buffer t :selected-window t :variables t :kill-state t :transient-mark t :command-state t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn general_cycles_and_buffer_local_lists_are_bidirectional_and_isolated() -> ParityBatchCase {
    let elisp_form = r##"(mwim358-test-run
 "general-cycle"
 (lambda ()
   (let ((first
          (mwim358-test-own-buffer
           "general-a" #'emacs-lisp-mode
           "  code   ; comment Ω  \nplain\n\n"))
         first-cycle second-cycle first-return
         end-forward end-reverse keys forward reverse plain empty)
     (setq-local mwim-beginning-position-functions
                 '(mwim358-test-second-column mwim-code-beginning))
     (setq keys
           (mwim358-test-bind-keys
            '(("C-c m" . mwim) ("C-c b" . mwim-beginning))))
     (mwim358-test-command-loop
      (lambda ()
        (goto-char 5)
        (dotimes (_ 6)
          (execute-kbd-macro (kbd "C-c m"))
          (push (mwim358-test-position 'forward) forward))
        (goto-char 5)
        (dotimes (_ 5)
          (execute-kbd-macro (kbd "C-u C-c m"))
          (push (mwim358-test-position 'reverse) reverse))
        (goto-char (point-min))
        (forward-line 1)
        (forward-char 2)
        (dotimes (_ 3)
          (execute-kbd-macro (kbd "C-c m"))
          (push (mwim358-test-position 'plain) plain))
        (goto-char (point-min))
        (forward-line 2)
        (dotimes (_ 2)
          (execute-kbd-macro (kbd "C-c m"))
          (push (mwim358-test-position 'empty) empty))
        (goto-char 1)
        (dotimes (_ 4)
          (execute-kbd-macro (kbd "C-c b"))
          (push (mwim358-test-position 'buffer-a) first-cycle))))
     (let ((first-result
            (list :local (local-variable-p
                          'mwim-beginning-position-functions)
                  :functions mwim-beginning-position-functions
                  :cycle (nreverse first-cycle)
                  :text (buffer-string)
                  :modified (buffer-modified-p)
                  :undo buffer-undo-list)))
       (mwim358-test-own-buffer
        "general-b" #'emacs-lisp-mode "  code ; note Ω  \n")
       (setq-local mwim-beginning-position-functions
                   '(mwim-line-end mwim-line-beginning))
       (setq-local mwim-end-position-functions
                   '(mwim-comment-beginning mwim-code-end mwim-line-end))
       (mwim358-test-bind-keys
        '(("C-c b" . mwim-beginning) ("C-c e" . mwim-end)))
       (mwim358-test-command-loop
        (lambda ()
          (goto-char 3)
          (dotimes (_ 3)
            (execute-kbd-macro (kbd "C-c b"))
            (push (mwim358-test-position 'buffer-b) second-cycle))
          (goto-char 1)
          (dotimes (_ 4)
            (execute-kbd-macro (kbd "C-c e"))
            (push (mwim358-test-position 'end-forward) end-forward))
          (goto-char 1)
          (dotimes (_ 4)
            (execute-kbd-macro (kbd "C-u C-c e"))
            (push (mwim358-test-position 'end-reverse) end-reverse))))
       (let ((second-result
              (list :local (local-variable-p
                            'mwim-beginning-position-functions)
                    :functions mwim-beginning-position-functions
                    :cycle (nreverse second-cycle)
                    :end-local
                    (local-variable-p 'mwim-end-position-functions)
                    :end-functions mwim-end-position-functions
                    :end-forward (nreverse end-forward)
                    :end-reverse (nreverse end-reverse)
                    :text (buffer-string)
                    :modified (buffer-modified-p)
                    :undo buffer-undo-list)))
         (switch-to-buffer first)
         (goto-char 1)
         (execute-kbd-macro (kbd "C-c b"))
         (setq first-return
               (list :functions mwim-beginning-position-functions
                     :state (mwim358-test-position 'buffer-a-return)))
         (list :keys keys
               :forward (nreverse forward)
               :reverse (nreverse reverse)
               :plain (nreverse plain)
               :empty (nreverse empty)
               :buffer-a first-result
               :buffer-b second-result
               :buffer-a-return first-return))))))"##;
    ParityBatchCase::value(
        "general_cycles_and_buffer_local_lists_are_bidirectional_and_isolated",
        elisp_form,
        expect![[
            r#"OK (:result (:keys (("C-c m" . mwim) ("C-c b" . mwim-beginning)) :forward ((forward :point 1 :line 1 :column 0 :before nil :after " " :mark nil :active nil) (forward :point 3 :line 1 :column 2 :before " " :after "c" :mark nil :active nil) (forward :point 7 :line 1 :column 6 :before "e" :after " " :mark nil :active nil) (forward :point 12 :line 1 :column 11 :before " " :after "c" :mark nil :active nil) (forward :point 23 :line 1 :column 22 :before " " :after "\n" :mark nil :active nil) (forward :point 1 :line 1 :column 0 :before nil :after " " :mark nil :active nil)) :reverse ((reverse :point 23 :line 1 :column 22 :before " " :after "\n" :mark nil :active nil) (reverse :point 12 :line 1 :column 11 :before " " :after "c" :mark nil :active nil) (reverse :point 7 :line 1 :column 6 :before "e" :after " " :mark nil :active nil) (reverse :point 3 :line 1 :column 2 :before " " :after "c" :mark nil :active nil) (reverse :point 1 :line 1 :column 0 :before nil :after " " :mark nil :active nil)) :plain ((plain :point 24 :line 2 :column 0 :before "\n" :after "p" :mark nil :active nil) (plain :point 29 :line 2 :column 5 :before "n" :after "\n" :mark nil :active nil) (plain :point 24 :line 2 :column 0 :before "\n" :after "p" :mark nil :active nil)) :empty ((empty :point 30 :line 3 :column 0 :before "\n" :after "\n" :mark nil :active nil) (empty :point 30 :line 3 :column 0 :before "\n" :after "\n" :mark nil :active nil)) :buffer-a (:local t :functions #1=(mwim358-test-second-column mwim-code-beginning) :cycle ((buffer-a :point 2 :line 1 :column 1 :before " " :after " " :mark nil :active nil) (buffer-a :point 3 :line 1 :column 2 :before " " :after "c" :mark nil :active nil) (buffer-a :point 2 :line 1 :column 1 :before " " :after " " :mark nil :active nil) (buffer-a :point 3 :line 1 :column 2 :before " " :after "c" :mark nil :active nil)) :text "  code   ; comment Ω  \nplain\n\n" :modified nil :undo nil) :buffer-b (:local t :functions (mwim-line-end mwim-line-beginning) :cycle ((buffer-b :point 18 :line 1 :column 17 :before " " :after "\n" :mark nil :active nil) (buffer-b :point 1 :line 1 :column 0 :before nil :after " " :mark nil :active nil) (buffer-b :point 18 :line 1 :column 17 :before " " :after "\n" :mark nil :active nil)) :end-local t :end-functions (mwim-comment-beginning mwim-code-end mwim-line-end) :end-forward ((end-forward :point 10 :line 1 :column 9 :before " " :after "n" :mark nil :active nil) (end-forward :point 7 :line 1 :column 6 :before "e" :after " " :mark nil :active nil) (end-forward :point 18 :line 1 :column 17 :before " " :after "\n" :mark nil :active nil) (end-forward :point 10 :line 1 :column 9 :before " " :after "n" :mark nil :active nil)) :end-reverse ((end-reverse :point 18 :line 1 :column 17 :before " " :after "\n" :mark nil :active nil) (end-reverse :point 7 :line 1 :column 6 :before "e" :after " " :mark nil :active nil) (end-reverse :point 10 :line 1 :column 9 :before " " :after "n" :mark nil :active nil) (end-reverse :point 18 :line 1 :column 17 :before " " :after "\n" :mark nil :active nil)) :text "  code ; note Ω  \n" :modified nil :undo nil) :buffer-a-return (:functions #1# :state (buffer-a-return :point 2 :line 1 :column 1 :before " " :after " " :mark nil :active nil))) :cleanup (:new-buffers nil :new-processes nil :new-timers nil :owned-live (nil nil) :window t :current-buffer t :selected-window t :variables t :kill-state t :transient-mark t :command-state t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn invalid_empty_and_commentless_configurations_fail_or_recover_exactly() -> ParityBatchCase {
    let elisp_form = r##"(mwim358-test-run
 "configuration-boundaries"
 (lambda ()
   (mwim358-test-own-buffer
    "configuration" #'emacs-lisp-mode "  alpha ; note Ω\n")
   (let ((default-positions mwim-position-functions)
         failure before after configured-empty recovery
         commentless commentless-end)
     (mwim358-test-bind-keys
      '(("C-c m" . mwim)
        ("M-a" . mwim-beginning-of-code-or-line-or-comment)))
     (mwim358-test-command-loop
      (lambda ()
        (goto-char 3)
        (setq before (mwim358-test-position 'before-failure))
        (setq-local mwim-position-functions
                    '(mwim-line-beginning mwim358-missing-position
                      mwim-line-end))
        (setq failure
              (mwim358-test-condition
               (lambda () (execute-kbd-macro (kbd "C-c m")))))
        (setq after (mwim358-test-position 'after-failure))
        (setq-local mwim-position-functions nil)
        (execute-kbd-macro (kbd "C-c m"))
        (setq configured-empty
              (mwim358-test-position 'configured-empty))
        (setq-local mwim-position-functions default-positions)
        (execute-kbd-macro (kbd "C-c m"))
        (setq recovery (mwim358-test-position 'recovery))))
     (let ((failure-result
            (list :before before :failure failure :after after
                  :configured-empty configured-empty :recovery recovery
                  :text (buffer-string) :modified (buffer-modified-p)
                  :undo buffer-undo-list)))
       (mwim358-test-own-buffer
        "commentless" #'text-mode "  alpha ; not-a-comment  \n")
       (setq-local comment-start-skip nil)
       (mwim358-test-bind-keys
        '(("M-a" . mwim-beginning-of-code-or-line-or-comment)
          ("M-e" . mwim-end-of-code-or-line)))
       (mwim358-test-command-loop
        (lambda ()
          (goto-char 5)
          (dotimes (_ 3)
            (execute-kbd-macro (kbd "M-a"))
            (push (mwim358-test-position 'commentless) commentless))
          (goto-char 5)
          (dotimes (_ 3)
            (execute-kbd-macro (kbd "M-e"))
            (push (mwim358-test-position 'commentless-end)
                  commentless-end))))
       (list :failure failure-result
             :commentless
             (list :comment-start-skip comment-start-skip
                   :states (nreverse commentless)
                   :end-states (nreverse commentless-end)
                   :text (buffer-string) :modified (buffer-modified-p)
                   :undo buffer-undo-list))))))"##;
    ParityBatchCase::value(
        "invalid_empty_and_commentless_configurations_fail_or_recover_exactly",
        elisp_form,
        expect![[
            r#"OK (:result (:failure (:before (before-failure :point 3 :line 1 :column 2 :before " " :after "a" :mark nil :active nil) :failure (:signal void-function :data (mwim358-missing-position) :message "Symbol’s function definition is void: mwim358-missing-position") :after (after-failure :point 3 :line 1 :column 2 :before " " :after "a" :mark nil :active nil) :configured-empty (configured-empty :point 3 :line 1 :column 2 :before " " :after "a" :mark nil :active nil) :recovery (recovery :point 8 :line 1 :column 7 :before "a" :after " " :mark nil :active nil) :text "  alpha ; note Ω\n" :modified nil :undo nil) :commentless (:comment-start-skip nil :states ((commentless :point 3 :line 1 :column 2 :before " " :after "a" :mark nil :active nil) (commentless :point 1 :line 1 :column 0 :before nil :after " " :mark nil :active nil) (commentless :point 3 :line 1 :column 2 :before " " :after "a" :mark nil :active nil)) :end-states ((commentless-end :point 24 :line 1 :column 23 :before "t" :after " " :mark nil :active nil) (commentless-end :point 26 :line 1 :column 25 :before " " :after "\n" :mark nil :active nil) (commentless-end :point 24 :line 1 :column 23 :before "t" :after " " :mark nil :active nil)) :text "  alpha ; not-a-comment  \n" :modified nil :undo nil)) :cleanup (:new-buffers nil :new-processes nil :new-timers nil :owned-live (nil nil) :window t :current-buffer t :selected-window t :variables t :kill-state t :transient-mark t :command-state t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn prefix_overrun_and_narrowing_respect_logical_boundaries() -> ParityBatchCase {
    let elisp_form = r##"(mwim358-test-run
 "prefix-narrow"
 (lambda ()
   (mwim358-test-own-buffer
    "prefix" #'emacs-lisp-mode
    "  one ; c\n    two\n\tthree   \n")
   (mwim358-test-bind-keys
    '(("<home>" . mwim-beginning-of-code-or-line)
      ("<end>" . mwim-end-of-code-or-line)))
   (let (prefix narrowing)
     (mwim358-test-command-loop
      (lambda ()
        (goto-char 6)
        (execute-kbd-macro (kbd "C-u 0 <home>"))
        (push (mwim358-test-position 'zero) prefix)
        (goto-char 6)
        (execute-kbd-macro (kbd "C-u 1 <home>"))
        (push (mwim358-test-position 'plus-one) prefix)
        (execute-kbd-macro (kbd "C-u - 1 <home>"))
        (push (mwim358-test-position 'minus-one) prefix)
        (goto-char (point-min))
        (execute-kbd-macro (kbd "C-u 2 <home>"))
        (push (mwim358-test-position 'plus-two) prefix)
        (execute-kbd-macro (kbd "C-u 99 <end>"))
        (push (mwim358-test-position 'past-end) prefix)))
     (let ((prefix-result
            (list :states (nreverse prefix)
                  :text (buffer-string)
                  :modified (buffer-modified-p)
                  :undo buffer-undo-list)))
       (mwim358-test-own-buffer
        "narrow" #'emacs-lisp-mode
        "HEAD\n   xx  alpha   ; note Ω  \nTAIL\n")
       (let ((start (progn (goto-char (point-min))
                           (forward-line 1)
                           (forward-char 1)
                           (point)))
             (end (progn (end-of-line) (- (point) 2))))
         (narrow-to-region start end)
         (mwim358-test-bind-keys
          '(("<home>" . mwim-beginning-of-code-or-line)
            ("<end>" . mwim-end-of-code-or-line)))
         (mwim358-test-command-loop
          (lambda ()
            (goto-char (+ (point-min) 7))
            (push (mwim358-test-position 'initial) narrowing)
            (execute-kbd-macro (kbd "<home>"))
            (push (mwim358-test-position 'code) narrowing)
            (execute-kbd-macro (kbd "<home>"))
            (push (mwim358-test-position 'restricted-beginning) narrowing)
            (goto-char (+ (point-min) 7))
            (execute-kbd-macro (kbd "<end>"))
            (push (mwim358-test-position 'code-end) narrowing)
            (execute-kbd-macro (kbd "<end>"))
            (push (mwim358-test-position 'restricted-end) narrowing)))
         (setq narrowing (nreverse narrowing))
         (list :prefix prefix-result
               :narrow
               (list :bounds (list (point-min) (point-max))
                     :states narrowing
                     :visible (buffer-string)
                     :inside
                     (cl-every (lambda (state)
                                 (<= (point-min)
                                     (plist-get (cdr state) :point)
                                     (point-max)))
                               narrowing)
                     :full
                     (save-restriction
                       (widen)
                       (buffer-string))
                     :modified (buffer-modified-p)
                     :undo buffer-undo-list)))))))"##;
    ParityBatchCase::value(
        "prefix_overrun_and_narrowing_respect_logical_boundaries",
        elisp_form,
        expect![[
            r#"OK (:result (:prefix (:states ((zero :point 3 :line 1 :column 2 :before " " :after "o" :mark nil :active nil) (plus-one :point 15 :line 2 :column 4 :before " " :after "t" :mark nil :active nil) (minus-one :point 3 :line 1 :column 2 :before " " :after "o" :mark nil :active nil) (plus-two :point 20 :line 3 :column 8 :before "\11" :after "t" :mark nil :active nil) (past-end :point 29 :line 4 :column 0 :before "\n" :after nil :mark nil :active nil)) :text "  one ; c\n    two\n\11three   \n" :modified nil :undo nil) :narrow (:bounds (7 29) :states ((initial :point 14 :line 1 :column 7 :before "a" :after "l" :mark nil :active nil) (code :point 9 :line 1 :column 2 :before " " :after "x" :mark nil :active nil) (restricted-beginning :point 7 :line 1 :column 0 :before nil :after " " :mark nil :active nil) (code-end :point 18 :line 1 :column 11 :before "a" :after " " :mark nil :active nil) (restricted-end :point 29 :line 1 :column 22 :before "Ω" :after nil :mark nil :active nil)) :visible "  xx  alpha   ; note Ω" :inside t :full "HEAD\n   xx  alpha   ; note Ω  \nTAIL\n" :modified nil :undo nil)) :cleanup (:new-buffers nil :new-processes nil :new-timers nil :owned-live (nil nil) :window t :current-buffer t :selected-window t :variables t :kill-state t :transient-mark t :command-state t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn c_mode_recognizes_marginal_and_comment_only_lines() -> ParityBatchCase {
    let elisp_form = r##"(mwim358-test-run
 "c-comments"
 (lambda ()
   (mwim358-test-own-buffer
    "c" #'c-mode
    "int value;  /* marginal block Ω */   \n  // only comment Ω  \ncode();   \n")
   (let ((keys
          (mwim358-test-bind-keys
           '(("<home>" . mwim-beginning-of-code-or-line-or-comment)
             ("<end>" . mwim-end-of-code-or-line))))
         beginning end prefix)
     (mwim358-test-command-loop
      (lambda ()
        (goto-char 6)
        (dotimes (_ 4)
          (execute-kbd-macro (kbd "<home>"))
          (push (mwim358-test-position 'beginning) beginning))
        (goto-char 6)
        (dotimes (_ 3)
          (execute-kbd-macro (kbd "<end>"))
          (push (mwim358-test-position 'end) end))
        (goto-char 6)
        (execute-kbd-macro (kbd "C-u 1 <home>"))
        (push (mwim358-test-position 'next-beginning) prefix)
        (goto-char 6)
        (execute-kbd-macro (kbd "C-u 1 <end>"))
        (push (mwim358-test-position 'next-end) prefix)
        (execute-kbd-macro (kbd "C-u 9 <end>"))
        (push (mwim358-test-position 'past-end) prefix)))
     (list :mode major-mode :keys keys
           :beginning (nreverse beginning)
           :end (nreverse end)
           :prefix (nreverse prefix)
           :text (buffer-string)
           :modified (buffer-modified-p)
           :undo buffer-undo-list))))"##;
    ParityBatchCase::value(
        "c_mode_recognizes_marginal_and_comment_only_lines",
        elisp_form,
        expect![[
            r#"OK (:result (:mode c-mode :keys (("<home>" . mwim-beginning-of-code-or-line-or-comment) ("<end>" . mwim-end-of-code-or-line)) :beginning ((beginning :point 1 :line 1 :column 0 :before nil :after "i" :mark nil :active nil) (beginning :point 16 :line 1 :column 15 :before " " :after "m" :mark nil :active nil) (beginning :point 1 :line 1 :column 0 :before nil :after "i" :mark nil :active nil) (beginning :point 16 :line 1 :column 15 :before " " :after "m" :mark nil :active nil)) :end ((end :point 11 :line 1 :column 10 :before ";" :after " " :mark nil :active nil) (end :point 38 :line 1 :column 37 :before " " :after "\n" :mark nil :active nil) (end :point 11 :line 1 :column 10 :before ";" :after " " :mark nil :active nil)) :prefix ((next-beginning :point 41 :line 2 :column 2 :before " " :after "/" :mark nil :active nil) (next-end :point 58 :line 2 :column 19 :before "Ω" :after " " :mark nil :active nil) (past-end :point 72 :line 4 :column 0 :before "\n" :after nil :mark nil :active nil)) :text "int value;  /* marginal block Ω */   \n  // only comment Ω  \ncode();   \n" :modified nil :undo nil) :cleanup (:new-buffers nil :new-processes nil :new-timers nil :owned-live (nil) :window t :current-buffer t :selected-window t :variables t :kill-state t :transient-mark t :command-state t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn shifted_home_selection_composes_with_real_kill_and_literal_edit() -> ParityBatchCase {
    let elisp_form = r##"(mwim358-test-run
 "shift-edit"
 (lambda ()
   (mwim358-test-own-buffer
    "shift-edit" #'emacs-lisp-mode "    target-界-value ; note\n")
   (let ((keys
          (mwim358-test-bind-keys
           '(("<home>" . mwim-beginning-of-code-or-line))))
         (shift-select-mode t)
         (transient-mark-mode t)
         (kill-ring nil)
         (kill-ring-yank-pointer nil)
         (interprogram-cut-function nil)
         (interprogram-paste-function nil)
         (save-interprogram-paste-before-kill nil)
         (kill-transform-function nil)
         (kill-do-not-save-duplicates nil)
         first extended selected after-kill final)
     (mwim358-test-command-loop
      (lambda ()
        (goto-char 14)
        (execute-kbd-macro (kbd "<S-home>"))
        (setq first
              (list :state (mwim358-test-position 'first-shift)
                    :region
                    (buffer-substring-no-properties
                     (region-beginning) (region-end))))
        (execute-kbd-macro (kbd "<S-home>"))
        (setq selected
              (buffer-substring-no-properties
               (region-beginning) (region-end)))
        (setq extended
              (list :state (mwim358-test-position 'extended-shift)
                    :region selected))
        (execute-kbd-macro (kbd "C-w"))
        (setq after-kill
              (list :state (mwim358-test-position 'after-kill)
                    :text (buffer-string)
                    :kill-ring (copy-tree kill-ring)
                    :yank-pointer-is-ring
                    (eq kill-ring-yank-pointer kill-ring)
                    :modified (buffer-modified-p)
                    :undo (copy-tree buffer-undo-list)))
        (execute-kbd-macro "[EDIT Ω]")
        (setq final
              (list :state (mwim358-test-position 'after-insert)
                    :text (buffer-string)
                    :kill-ring (copy-tree kill-ring)
                    :yank-pointer-is-ring
                    (eq kill-ring-yank-pointer kill-ring)
                    :modified (buffer-modified-p)
                    :undo (copy-tree buffer-undo-list)))))
     (list :keys keys
           :shift-binding (key-binding (kbd "<S-home>"))
           :first first :extended extended :selected selected
           :after-kill after-kill :final final))))"##;
    ParityBatchCase::value(
        "shifted_home_selection_composes_with_real_kill_and_literal_edit",
        elisp_form,
        expect![[
            r#"OK (:result (:keys (("<home>" . mwim-beginning-of-code-or-line)) :shift-binding nil :first (:state (first-shift :point 5 :line 1 :column 4 :before " " :after "t" :mark 14 :active t) :region "target-界-") :extended (:state (extended-shift :point 1 :line 1 :column 0 :before nil :after " " :mark 14 :active t) :region "    target-界-") :selected "    target-界-" :after-kill (:state (after-kill :point 1 :line 1 :column 0 :before nil :after "v" :mark 1 :active nil) :text "value ; note\n" :kill-ring ("    target-界-") :yank-pointer-is-ring t :modified t :undo (("    target-界-" . 1) ((:marker nil nil) . -13) ((:marker nil nil) . -13) (t . 0))) :final (:state (after-insert :point 9 :line 1 :column 8 :before "]" :after "v" :mark 1 :active nil) :text "[EDIT Ω]value ; note\n" :kill-ring ("    target-界-") :yank-pointer-is-ring t :modified t :undo ((1 . 9) nil ("    target-界-" . 1) ((:marker nil nil) . -13) ((:marker nil nil) . -13) (t . 0)))) :cleanup (:new-buffers nil :new-processes nil :new-timers nil :owned-live (nil) :window t :current-buffer t :selected-window t :variables t :kill-state t :transient-mark t :command-state t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn mode_dispatch_uses_real_org_and_message_line_commands() -> ParityBatchCase {
    let elisp_form = r##"(mwim358-test-run
 "mode-dispatch"
 (lambda ()
   (mwim358-test-own-buffer
    "org" #'org-mode
    "* TODO Ship release Ω        :work:urgent:\n  - [ ] child item\n")
   (let ((org-special-ctrl-a/e t)
         (org-keys
          (mwim358-test-bind-keys
           '(("C-a" . mwim-beginning-of-line)
             ("C-e" . mwim-end-of-line))))
         org-states message-states)
     (mwim358-test-command-loop
      (lambda ()
        (goto-char 12)
        (dotimes (_ 4)
          (execute-kbd-macro (kbd "C-a"))
          (push (mwim358-test-position 'heading-begin) org-states))
        (goto-char 12)
        (dotimes (_ 4)
          (execute-kbd-macro (kbd "C-e"))
          (push (mwim358-test-position 'heading-end) org-states))
        (goto-char (point-max))
        (backward-char 3)
        (dotimes (_ 2)
          (execute-kbd-macro (kbd "C-a"))
          (push (mwim358-test-position 'item-begin) org-states))))
     (let ((org-result
            (list :mode major-mode :keys org-keys
                  :states (nreverse org-states)
                  :text (buffer-string)
                  :modified (buffer-modified-p)
                  :undo buffer-undo-list)))
       (mwim358-test-own-buffer
        "message" #'message-mode
        "Subject: Release Ω\n folded continuation\n\nBody\n")
       (let ((message-keys
              (mwim358-test-bind-keys
               '(("C-a" . mwim-beginning-of-line)))))
         (mwim358-test-command-loop
          (lambda ()
            (goto-char (point-min))
            (end-of-line)
            (dotimes (_ 3)
              (execute-kbd-macro (kbd "C-a"))
              (push (mwim358-test-position 'message-begin)
                    message-states))))
         (list :org org-result
               :message
               (list :mode major-mode :keys message-keys
                     :states (nreverse message-states)
                     :text (buffer-string)
                     :modified (buffer-modified-p)
                     :undo buffer-undo-list)))))))"##;
    ParityBatchCase::value(
        "mode_dispatch_uses_real_org_and_message_line_commands",
        elisp_form,
        expect![[
            r#"OK (:result (:org (:mode org-mode :keys (("C-a" . mwim-beginning-of-line) ("C-e" . mwim-end-of-line)) :states ((heading-begin :point 8 :line 1 :column 7 :before " " :after "S" :mark nil :active nil) (heading-begin :point 1 :line 1 :column 0 :before nil :after "*" :mark nil :active nil) (heading-begin :point 8 :line 1 :column 7 :before " " :after "S" :mark nil :active nil) (heading-begin :point 1 :line 1 :column 0 :before nil :after "*" :mark nil :active nil) (heading-end :point 22 :line 1 :column 21 :before "Ω" :after " " :mark nil :active nil) (heading-end :point 43 :line 1 :column 42 :before ":" :after "\n" :mark nil :active nil) (heading-end :point 22 :line 1 :column 21 :before "Ω" :after " " :mark nil :active nil) (heading-end :point 43 :line 1 :column 42 :before ":" :after "\n" :mark nil :active nil) (item-begin :point 52 :line 2 :column 8 :before " " :after "c" :mark nil :active nil) (item-begin :point 44 :line 2 :column 0 :before "\n" :after " " :mark nil :active nil)) :text "* TODO Ship release Ω        :work:urgent:\n  - [ ] child item\n" :modified nil :undo nil) :message (:mode message-mode :keys (("C-a" . mwim-beginning-of-line)) :states ((message-begin :point 10 :line 1 :column 9 :before " " :after "R" :mark nil :active nil) (message-begin :point 1 :line 1 :column 0 :before nil :after "S" :mark nil :active nil) (message-begin :point 10 :line 1 :column 9 :before " " :after "R" :mark nil :active nil)) :text "Subject: Release Ω\n folded continuation\n\nBody\n" :modified nil :undo nil)) :cleanup (:new-buffers nil :new-processes nil :new-timers nil :owned-live (nil nil) :window t :current-buffer t :selected-window t :variables t :kill-state t :transient-mark t :command-state t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        logical_code_comment_cycles_and_prefixes(),
        general_cycles_and_buffer_local_lists_are_bidirectional_and_isolated(),
        prefix_overrun_and_narrowing_respect_logical_boundaries(),
        c_mode_recognizes_marginal_and_comment_only_lines(),
        shifted_home_selection_composes_with_real_kill_and_literal_edit(),
        mode_dispatch_uses_real_org_and_message_line_commands(),
        invalid_empty_and_commentless_configurations_fail_or_recover_exactly(),
    ]
}
