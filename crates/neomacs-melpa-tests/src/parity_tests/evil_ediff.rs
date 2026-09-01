use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_EDIFF_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(setq evil-want-integration t
      evil-want-keybinding nil)
(require 'evil-ediff)
(evil-mode 1)

(defun neomacs-evil-ediff-test-execute (keys)
  "Execute KEYS through the live Ediff control buffer's Evil maps."
  (execute-kbd-macro (kbd keys)))

(defun neomacs-evil-ediff-test-window-state ()
  "Return synchronized viewport state for the live variant windows."
  (list :a (list (window-start ediff-window-A)
                 (window-hscroll ediff-window-A))
        :b (list (window-start ediff-window-B)
                 (window-hscroll ediff-window-B))))

(defun neomacs-evil-ediff-test-help-rows ()
  "Return every Evil-edited row from all seven Ediff help fragments."
  (mapcar
   (lambda (symbol)
     (cons symbol
           (cl-remove-if-not
            (lambda (line)
              (string-match-p
               "previous diff\\|next diff\\|jump to diff\\|highlighting\\|scroll up/dn\\|scroll lt/rt\\|suspend/quit"
               line))
            (split-string (symbol-value symbol) "\n" t))))
   '(ediff-long-help-message-compare2
     ediff-long-help-message-compare3
     ediff-long-help-message-narrow2
     ediff-long-help-message-word-mode
     ediff-long-help-message-merge
     ediff-long-help-message-head
     ediff-long-help-message-tail)))

(defun neomacs-evil-ediff-test-backup-help-rows ()
  "Return the original rows captured by Evil Ediff before initialization."
  (mapcar
   (lambda (entry)
     (cons (car entry)
           (cl-remove-if-not
            (lambda (line)
              (string-match-p
               "previous diff\\|next diff\\|jump to diff\\|highlighting\\|scroll up/dn\\|scroll lt/rt\\|suspend/quit"
               line))
            (split-string (symbol-value (cdr entry)) "\n" t))))
   '((compare2 . evil-ediff-long-help-message-compare2-backup)
     (compare3 . evil-ediff-long-help-message-compare3-backup)
     (narrow2 . evil-ediff-long-help-message-narrow2-backup)
     (word-mode . evil-ediff-long-help-message-word-backup)
     (merge . evil-ediff-long-help-message-merge-backup)
     (head . evil-ediff-long-help-message-head-backup)
     (tail . evil-ediff-long-help-message-tail-backup))))

(defun neomacs-evil-ediff-test-workflow-bindings ()
  "Return representative bindings from the live Ediff control map."
  (mapcar (lambda (key)
            (cons key (lookup-key ediff-mode-map (kbd key))))
          '("j" "gg" "l" "h" "C-z")))

(defun neomacs-evil-ediff-test-session (texts split body)
  "Run BODY in a real Ediff session over TEXTS using SPLIT.
TEXTS contains two or three variant strings.  BODY receives the
control buffer followed by the live variant buffers."
  (let ((buffers
         (cl-loop for suffix in '("A" "B" "C")
                  for _text in texts
                  collect (generate-new-buffer
                           (format " *evil-ediff-release-%s*" suffix))))
        control)
    (save-window-excursion
      (unwind-protect
          (progn
            ;; Each probe must enter Ediff from the same one-window baseline;
            ;; Ediff derives viewport geometry from the existing layout.
            (delete-other-windows)
            (cl-mapc (lambda (buffer text)
                       (with-current-buffer buffer (insert text)))
                     buffers texts)
            (let ((ediff-window-setup-function #'ediff-setup-windows-plain)
                  (ediff-split-window-function split)
                  (ediff-keep-variants t))
              (setq control
                    (if (cddr buffers)
                        (ediff-buffers3 (nth 0 buffers)
                                        (nth 1 buffers)
                                        (nth 2 buffers))
                      (ediff-buffers (nth 0 buffers) (nth 1 buffers)))))
            (with-current-buffer control
              (apply body control buffers)))
        (when (buffer-live-p control)
          (with-current-buffer control
            (let ((ediff-keep-variants t))
              (condition-case nil
                  (ediff-really-quit nil)
                (error (kill-buffer control))))))
        (dolist (buffer buffers)
          (when (buffer-live-p buffer) (kill-buffer buffer)))))))
"####;

fn side_by_side_review_navigates_and_reconciles_both_release_hunks() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-ediff-test-session
 '("header\nrelease = candidate\ncommon\nretries = 3\nfooter\n"
   "header\nrelease = draft\ncommon\nretries = 1\nfooter\n")
 #'split-window-horizontally
 (lambda (_control buffer-a buffer-b)
   (let ((start (list :state evil-state
                      :differences ediff-number-of-differences
                      :current ediff-current-difference)))
     (neomacs-evil-ediff-test-execute "j")
     (let ((first ediff-current-difference))
       (neomacs-evil-ediff-test-execute "j")
       (let ((second ediff-current-difference))
         (neomacs-evil-ediff-test-execute "k")
         (let ((back ediff-current-difference))
           (neomacs-evil-ediff-test-execute "l")
           (let ((after-a-to-b
                  (with-current-buffer buffer-b (buffer-string))))
             (neomacs-evil-ediff-test-execute "G")
             (let ((last ediff-current-difference))
               (neomacs-evil-ediff-test-execute "h")
               (list
                :start start
                :visited (list first second back last)
                :after-a-to-b after-a-to-b
                :final-a (with-current-buffer buffer-a (buffer-string))
                :final-b (with-current-buffer buffer-b (buffer-string))
                :bindings
                (mapcar (lambda (key)
                          (cons key (lookup-key ediff-mode-map (kbd key))))
                        '("j" "k" "gg" "G" "l" "h" "z" "C-z")))))))))))
"####;
    let expected = expect![[
        r#"OK (:start (:state motion :differences 2 :current -1) :visited (0 1 0 1) :after-a-to-b "header\nrelease = candidate\ncommon\nretries = 1\nfooter\n" :final-a "header\nrelease = candidate\ncommon\nretries = 1\nfooter\n" :final-b "header\nrelease = candidate\ncommon\nretries = 1\nfooter\n" :bindings (("j" . ediff-next-difference) ("k" . ediff-previous-difference) ("gg" . evil-ediff-first-difference) ("G" . evil-ediff-last-difference) ("l" . ediff-copy-A-to-B) ("h" . ediff-copy-B-to-A) ("z" keymap (104 . evil-ediff-scroll-left) (108 . evil-ediff-scroll-right)) ("C-z" . ediff-suspend)))"#
    ]];
    ParityBatchCase::value(
        "side_by_side_review_navigates_and_reconciles_both_release_hunks",
        elisp_form,
        expected,
    )
}

fn vim_jumps_select_first_numbered_and_last_differences() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-ediff-test-session
 '("alpha = old\nkeep-1\nbeta = old\nkeep-2\ngamma = old\n"
   "alpha = new\nkeep-1\nbeta = new\nkeep-2\ngamma = new\n")
 #'split-window-horizontally
 (lambda (_control _buffer-a _buffer-b)
   (let ((steps nil))
     (dolist (keys '("G" "gg" "2 d" "N" "j"))
       (neomacs-evil-ediff-test-execute keys)
       (push (list keys ediff-current-difference) steps))
     (list :differences ediff-number-of-differences
           :steps (nreverse steps)
           :commands
           (mapcar (lambda (key)
                     (cons key (lookup-key ediff-mode-map (kbd key))))
                   '("G" "gg" "d" "N" "j"))))))
"####;
    let expected = expect![[
        r#"OK (:differences 3 :steps (("G" 2) ("gg" 0) ("2 d" 1) ("N" 0) ("j" 1)) :commands (("G" . evil-ediff-last-difference) ("gg" . evil-ediff-first-difference) ("d" . ediff-jump-to-difference) ("N" . ediff-previous-difference) ("j" . ediff-next-difference)))"#
    ]];
    ParityBatchCase::value(
        "vim_jumps_select_first_numbered_and_last_differences",
        elisp_form,
        expected,
    )
}

fn viewport_keys_synchronize_variants_and_dispatch_evil_scroll_commands() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((long-tail (make-string 140 ?x))
       (text-a
        (mapconcat
         (lambda (line)
           (format "%03d %s %s" line
                   (if (= line 40) "candidate" "shared") long-tail))
         (number-sequence 1 80) "\n"))
       (text-b
        (mapconcat
         (lambda (line)
           (format "%03d %s %s" line
                   (if (= line 40) "draft" "shared") long-tail))
         (number-sequence 1 80) "\n")))
  (neomacs-evil-ediff-test-session
   (list text-a text-b)
   #'split-window-horizontally
   (lambda (_control _buffer-a _buffer-b)
     (let ((scroll-commands nil))
       (add-hook 'post-command-hook
                 (lambda ()
                   (when (memq this-command
                               '(evil-ediff-scroll-down
                                 evil-ediff-scroll-up))
                     (push this-command scroll-commands)))
                 nil t)
       (neomacs-evil-ediff-test-execute "j")
       (let ((selected (neomacs-evil-ediff-test-window-state)))
       (neomacs-evil-ediff-test-execute "C-e")
       (let ((after-control-e (neomacs-evil-ediff-test-window-state)))
         (neomacs-evil-ediff-test-execute "C-y")
         (let ((after-control-y (neomacs-evil-ediff-test-window-state)))
           (neomacs-evil-ediff-test-execute "4 C-d")
           (let ((after-four-control-d
                  (neomacs-evil-ediff-test-window-state)))
             (neomacs-evil-ediff-test-execute "z l")
             (let ((right (neomacs-evil-ediff-test-window-state)))
               (neomacs-evil-ediff-test-execute "z h")
               (let ((horizontal-round-trip
                      (neomacs-evil-ediff-test-window-state)))
                 (neomacs-evil-ediff-test-execute "C-u")
                 (let* ((after-control-u
                         (neomacs-evil-ediff-test-window-state))
                        (control-u-a (plist-get after-control-u :a))
                        (control-u-b (plist-get after-control-u :b)))
                   (list
                    :selected selected
                    :after-control-e after-control-e
                    :after-control-y after-control-y
                    :after-four-control-d after-four-control-d
                    :right right
                    :horizontal-round-trip horizontal-round-trip
                    :control-u
                    (list
                     :variants-synchronized (equal control-u-a control-u-b)
                     :moved-toward-buffer-start
                     (< (car control-u-a)
                        (car (plist-get after-four-control-d :a)))
                     :horizontal-offsets
                     (list (cadr control-u-a) (cadr control-u-b)))
                    :scroll-commands (nreverse scroll-commands)))))))))))))
"####;
    let expected = expect![
        "OK (:selected (:a (5929 0) :b (5929 0)) :after-control-e (:a (4713 0) :b (4713 0)) :after-control-y (:a (4409 0) :b (4409 0)) :after-four-control-d (:a (5169 0) :b (5169 0)) :right (:a (5169 16) :b (5169 16)) :horizontal-round-trip (:a (5169 0) :b (5169 0)) :control-u (:variants-synchronized t :moved-toward-buffer-start t :horizontal-offsets (0 0)) :scroll-commands (evil-ediff-scroll-down evil-ediff-scroll-up))"
    ];
    ParityBatchCase::value(
        "viewport_keys_synchronize_variants_and_dispatch_evil_scroll_commands",
        elisp_form,
        expected,
    )
}

fn help_panel_describes_the_installed_vim_workflow() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-ediff-test-session
 '("release candidate\nship today\n"
   "release draft\nship tomorrow\n")
 #'split-window-horizontally
 (lambda (_control _buffer-a _buffer-b)
   (neomacs-evil-ediff-test-execute "?")
   (list
    :long-help ediff-use-long-help-message
    :rows
    (cl-remove-if-not
     (lambda (line)
       (string-match-p
        "previous diff\\|next diff\\|jump to diff\\|scroll up/dn\\|scroll lt/rt\\|suspend/quit"
        line))
     (split-string (buffer-substring-no-properties (point-min) (point-max))
                   "\n" t))
    :commands
    (mapcar (lambda (key)
              (cons key (lookup-key ediff-mode-map (kbd key))))
            '("j" "k" "d" "H" "C-d" "C-u" "z l" "z h" "C-z" "z")))))
"####;
    let expected = expect![[
        r#"OK (:long-help t :rows ("k,N,p -previous diff |     | -vert/horiz split   |a/b -copy A/B's region to B/A" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output" "    i -status info   |     ? -help off           |C-z/q -suspend/quit") :commands (("j" . ediff-next-difference) ("k" . ediff-previous-difference) ("d" . ediff-jump-to-difference) ("H" . ediff-toggle-hilit) ("C-d" . evil-ediff-scroll-down) ("C-u" . evil-ediff-scroll-up) ("z l" . evil-ediff-scroll-right) ("z h" . evil-ediff-scroll-left) ("C-z" . ediff-suspend) ("z" keymap (104 . evil-ediff-scroll-left) (108 . evil-ediff-scroll-right))))"#
    ]];
    ParityBatchCase::value(
        "help_panel_describes_the_installed_vim_workflow",
        elisp_form,
        expected,
    )
}

fn stacked_review_preserves_highlighting_while_withholding_side_copy_aliases() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-ediff-test-session
 '("service = api\nreplicas = 3\n"
   "service = worker\nreplicas = 5\n")
 #'split-window-vertically
 (lambda (_control buffer-a buffer-b)
   (neomacs-evil-ediff-test-execute "j")
   (let ((before (list ediff-highlighting-style
                       ediff-highlight-all-diffs
                       ediff-use-faces)))
     (neomacs-evil-ediff-test-execute "h")
     (list :before before
           :after (list ediff-highlighting-style
                        ediff-highlight-all-diffs
                        ediff-use-faces)
           :buffers (list (with-current-buffer buffer-a (buffer-string))
                          (with-current-buffer buffer-b (buffer-string)))
           :bindings
           (mapcar (lambda (key)
                     (cons key (lookup-key ediff-mode-map (kbd key))))
                   '("h" "H" "l" "a" "b"))))))
"####;
    let expected = expect![[
        r#"OK (:before (ascii t t) :after (off nil t) :buffers ("service = api\nreplicas = 3\n" "service = worker\nreplicas = 5\n") :bindings (("h" . ediff-toggle-hilit) ("H" . ediff-toggle-hilit) ("l") ("a" . ediff-copy-A-to-B) ("b" . ediff-copy-B-to-A)))"#
    ]];
    ParityBatchCase::value(
        "stacked_review_preserves_highlighting_while_withholding_side_copy_aliases",
        elisp_form,
        expected,
    )
}

fn three_way_review_keeps_explicit_copy_chords_and_vim_navigation() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-ediff-test-session
 '("region = alpha\ncommon\ntail = shared\n"
   "region = beta\ncommon\ntail = shared\n"
   "region = gamma\ncommon\ntail = changed\n")
 #'split-window-horizontally
 (lambda (_control buffer-a buffer-b buffer-c)
   (neomacs-evil-ediff-test-execute "j")
   (let ((first ediff-current-difference))
     (neomacs-evil-ediff-test-execute "a b")
     (let ((after-copy (with-current-buffer buffer-b (buffer-string))))
       (neomacs-evil-ediff-test-execute "G")
       (list :state evil-state
             :differences ediff-number-of-differences
             :visited (list first ediff-current-difference)
             :after-copy after-copy
             :a (with-current-buffer buffer-a (buffer-string))
             :c (with-current-buffer buffer-c (buffer-string))
             :bindings
             (mapcar (lambda (key)
                       (cons key (lookup-key ediff-mode-map (kbd key))))
                     '("j" "k" "gg" "G" "a b" "b a" "l" "h")))))))
"####;
    let expected = expect![[
        r#"OK (:state motion :differences 2 :visited (0 1) :after-copy "region = alpha\ncommon\ntail = shared\n" :a "region = alpha\ncommon\ntail = shared\n" :c "region = gamma\ncommon\ntail = changed\n" :bindings (("j" . ediff-next-difference) ("k" . ediff-previous-difference) ("gg" . evil-ediff-first-difference) ("G" . evil-ediff-last-difference) ("a b" . ediff-copy-A-to-B) ("b a" . ediff-copy-B-to-A) ("l") ("h" . ediff-toggle-hilit)))"#
    ]];
    ParityBatchCase::value(
        "three_way_review_keeps_explicit_copy_chords_and_vim_navigation",
        elisp_form,
        expected,
    )
}

fn suspend_revert_and_reinitialize_expose_the_historical_lifecycle() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (neomacs-evil-ediff-test-session
     '("release = candidate\n" "release = draft\n")
     #'split-window-horizontally
     (lambda (control _buffer-a _buffer-b)
       (let ((events nil)
             (before-help (neomacs-evil-ediff-test-help-rows))
             (backup-help (neomacs-evil-ediff-test-backup-help-rows))
             (before-bindings
              (neomacs-evil-ediff-test-workflow-bindings)))
         (add-hook 'ediff-suspend-hook
                   (lambda () (push (list major-mode evil-state) events))
                   nil t)
         (neomacs-evil-ediff-test-execute "C-z")
         (let ((left-control-buffer (not (eq (current-buffer) control))))
           (with-current-buffer control
             (evil-ediff-revert)
             (let ((reverted
                    (list
                     :state (evil-initial-state 'ediff-mode)
                     :startup-hook
                     (and (memq 'evil-ediff-startup-hook ediff-startup-hook) t)
                     :help-changed evil-ediff-help-changed
                     :help (neomacs-evil-ediff-test-help-rows)
                     :bindings (neomacs-evil-ediff-test-workflow-bindings))))
               (evil-ediff-init)
               (evil-ediff-init)
               (list
                :suspend
                (list :events (nreverse events)
                      :left-control-buffer left-control-buffer
                      :control-live (buffer-live-p control))
                :before
                (list :state 'motion
                      :help before-help
                      :backup-help backup-help
                      :bindings before-bindings)
                :reverted reverted
                :reinitialized
                (list
                 :state (evil-initial-state 'ediff-mode)
                 :startup-hook-count
                 (cl-count 'evil-ediff-startup-hook ediff-startup-hook)
                 :help-changed evil-ediff-help-changed
                 :help (neomacs-evil-ediff-test-help-rows)
                 :bindings
                 (neomacs-evil-ediff-test-workflow-bindings)))))))))
  ;; The batch prelude begins initialized.  Re-establish that state even if
  ;; an assertion probe above exits early, so later cases remain independent.
  (evil-ediff-init))
"####;
    let expected = expect![[
        r#"OK (:suspend (:events ((ediff-mode motion)) :left-control-buffer t :control-live t) :before (:state motion :help ((ediff-long-help-message-compare2 "k,N,p -previous diff |     | -vert/horiz split   |a/b -copy A/B's region to B/A" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-compare3 "k,N,p -previous diff |     | -vert/horiz split   | xy -copy buf X's region to Y" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-narrow2 "k,N,p -previous diff |     | -vert/horiz split   |a/b -copy A/B's region to B/A" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-word-mode "k,N,p -previous diff |     | -vert/horiz split   | xy -copy buf X's region to Y" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |                           |" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-merge "k,N,p -previous diff |     | -vert/horiz split   |  x -copy buf X's region to C" "  j,n -next diff     |     H -highlighting       |  r -restore buf C's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  |     X -read-only in buf X | wx -save buf X" "zh/zl -scroll lt/rt  |     m -wide display       | wd -save diff output") (ediff-long-help-message-head) (ediff-long-help-message-tail "    i -status info   |     ? -help off           |C-z/q -suspend/quit")) :backup-help ((compare2 "p,DEL -previous diff |     | -vert/horiz split   |a/b -copy A/B's region to B/A" "n,SPC -next diff     |     h -highlighting       | rx -restore buf X's old diff" "    j -jump to diff  |     @ -auto-refinement    |  * -refine current region" "  v/V -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "  </> -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (compare3 "p,DEL -previous diff |     | -vert/horiz split   | xy -copy buf X's region to Y" "n,SPC -next diff     |     h -highlighting       | rx -restore buf X's old diff" "    j -jump to diff  |     @ -auto-refinement    |  * -refine current region" "  v/V -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "  </> -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (narrow2 "p,DEL -previous diff |     | -vert/horiz split   |a/b -copy A/B's region to B/A" "n,SPC -next diff     |     h -highlighting       | rx -restore buf X's old diff" "    j -jump to diff  |     @ -auto-refinement    |  * -refine current region" "  v/V -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "  </> -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (word-mode "p,DEL -previous diff |     | -vert/horiz split   | xy -copy buf X's region to Y" "n,SPC -next diff     |     h -highlighting       | rx -restore buf X's old diff" "    j -jump to diff  |                           |" "  v/V -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "  </> -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (merge "p,DEL -previous diff |     | -vert/horiz split   |  x -copy buf X's region to C" "n,SPC -next diff     |     h -highlighting       |  r -restore buf C's old diff" "    j -jump to diff  |     @ -auto-refinement    |  * -refine current region" "  v/V -scroll up/dn  |     X -read-only in buf X | wx -save buf X" "  </> -scroll lt/rt  |     m -wide display       | wd -save diff output") (head) (tail "    i -status info   |     ? -help off           |  z/q -suspend/quit")) :bindings (("j" . ediff-next-difference) ("gg" . evil-ediff-first-difference) ("l" . ediff-copy-A-to-B) ("h" . ediff-copy-B-to-A) ("C-z" . ediff-suspend))) :reverted (:state emacs :startup-hook nil :help-changed nil :help ((ediff-long-help-message-compare2 "k,N,p -previous diff |     | -vert/horiz split   |a/b -copy A/B's region to B/A" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-compare3 "k,N,p -previous diff |     | -vert/horiz split   | xy -copy buf X's region to Y" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-narrow2 "k,N,p -previous diff |     | -vert/horiz split   |a/b -copy A/B's region to B/A" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-word-mode "k,N,p -previous diff |     | -vert/horiz split   | xy -copy buf X's region to Y" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |                           |" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-merge "k,N,p -previous diff |     | -vert/horiz split   |  x -copy buf X's region to C" "  j,n -next diff     |     H -highlighting       |  r -restore buf C's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  |     X -read-only in buf X | wx -save buf X" "zh/zl -scroll lt/rt  |     m -wide display       | wd -save diff output") (ediff-long-help-message-head) (ediff-long-help-message-tail "    i -status info   |     ? -help off           |C-z/q -suspend/quit")) :bindings (("j" . ediff-next-difference) ("gg" . evil-ediff-first-difference) ("l" . ediff-copy-A-to-B) ("h" . ediff-copy-B-to-A) ("C-z" . ediff-suspend))) :reinitialized (:state motion :startup-hook-count 1 :help-changed t :help ((ediff-long-help-message-compare2 "k,N,p -previous diff |     | -vert/horiz split   |a/b -copy A/B's region to B/A" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-compare3 "k,N,p -previous diff |     | -vert/horiz split   | xy -copy buf X's region to Y" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-narrow2 "k,N,p -previous diff |     | -vert/horiz split   |a/b -copy A/B's region to B/A" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-word-mode "k,N,p -previous diff |     | -vert/horiz split   | xy -copy buf X's region to Y" "  j,n -next diff     |     H -highlighting       | rx -restore buf X's old diff" "    d -jump to diff  |                           |" "C-u/d -scroll up/dn  | #f/#h -focus/hide regions | wx -save buf X" "zh/zl -scroll lt/rt  |     X -read-only in buf X | wd -save diff output") (ediff-long-help-message-merge "k,N,p -previous diff |     | -vert/horiz split   |  x -copy buf X's region to C" "  j,n -next diff     |     H -highlighting       |  r -restore buf C's old diff" "    d -jump to diff  |     @ -auto-refinement    |  * -refine current region" "C-u/d -scroll up/dn  |     X -read-only in buf X | wx -save buf X" "zh/zl -scroll lt/rt  |     m -wide display       | wd -save diff output") (ediff-long-help-message-head) (ediff-long-help-message-tail "    i -status info   |     ? -help off           |C-z/q -suspend/quit")) :bindings (("j" . ediff-next-difference) ("gg" . evil-ediff-first-difference) ("l" . ediff-copy-A-to-B) ("h" . ediff-copy-B-to-A) ("C-z" . ediff-suspend))))"#
    ]];
    ParityBatchCase::value(
        "suspend_revert_and_reinitialize_expose_the_historical_lifecycle",
        elisp_form,
        expected,
    )
}

fn navigation_outside_an_ediff_control_panel_reports_the_exact_user_error() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (evil-local-mode 1)
  (evil-motion-state)
  (evil-ediff-first-difference))
"####;
    let expected =
        expect![[r#"ERR (user-error "nil: This command runs in Ediff Control Buffer only!")"#]];
    ParityBatchCase::signal(
        "navigation_outside_an_ediff_control_panel_reports_the_exact_user_error",
        elisp_form,
        expected,
    )
}

#[test]
fn evil_ediff_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(EVIL_EDIFF_MELPA_PIN, "evil-ediff.el")
            .expect("prepare revision-pinned Evil Ediff source below ./tmp")
            .with_timeout(Duration::from_secs(300))
            .with_prelude(PRELUDE),
        "evil-ediff-package-batch",
        "Evil Ediff",
        &[
            side_by_side_review_navigates_and_reconciles_both_release_hunks(),
            vim_jumps_select_first_numbered_and_last_differences(),
            viewport_keys_synchronize_variants_and_dispatch_evil_scroll_commands(),
            help_panel_describes_the_installed_vim_workflow(),
            stacked_review_preserves_highlighting_while_withholding_side_copy_aliases(),
            three_way_review_keeps_explicit_copy_chords_and_vim_navigation(),
            suspend_revert_and_reinitialize_expose_the_historical_lifecycle(),
            navigation_outside_an_ediff_control_panel_reports_the_exact_user_error(),
        ],
    );
}
