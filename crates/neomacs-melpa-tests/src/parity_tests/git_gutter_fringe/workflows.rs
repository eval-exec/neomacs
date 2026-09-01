use expect_test::expect;

use super::ParityBatchCase;

fn mixed_hunks_install_exact_left_fringe_display_specs() -> ParityBatchCase {
    let elisp_form = r#"(ggf351-test-run
 "mixed-left"
 (lambda (_fixture baseline-margins baseline-fringes)
   (git-gutter-mode 1)
   (ggf351-test-wait 3)
   (list
    :backend (list git-gutter:init-function git-gutter:view-diff-function
                   git-gutter:clear-function git-gutter:window-width)
    :side git-gutter-fr:side
    :faces
    (mapcar (lambda (face) (list face (face-attribute face :inherit nil nil)))
            '(git-gutter-fr:added git-gutter-fr:modified git-gutter-fr:deleted))
    :bitmaps
    (mapcar (lambda (bitmap) (list bitmap (and (fringe-bitmap-p bitmap) t)))
            '(git-gutter-fr:added git-gutter-fr:modified git-gutter-fr:deleted))
    :bitmap-symbols-distinct
    (= 3 (length (delete-dups
                  '(git-gutter-fr:added git-gutter-fr:modified
                    git-gutter-fr:deleted))))
    :mode (list git-gutter-mode git-gutter:enabled git-gutter:vcs-type
                (local-variable-p 'git-gutter-fr:bitmap-references)
                (length git-gutter-fr:bitmap-references))
    :hunks (ggf351-test-hunks)
    :refs (ggf351-test-refs)
    :overlays (ggf351-test-owned-overlays)
    :geometry (list :margins-before baseline-margins
                    :margins-after (window-margins)
                    :fringes-before baseline-fringes
                    :fringes-after (window-fringes))
    :margin-signs (ggf351-test-margin-signs)
    :batch-gate (list :graphic (display-graphic-p)
                      :window-system window-system
                      :rows (ggf351-test-row-bitmaps)))))"#;
    let expected = expect![[
        r#"OK (:result (:backend (git-gutter-fr:init git-gutter-fr:view-diff-infos git-gutter-fr:clear -1) :side left-fringe :faces ((git-gutter-fr:added (git-gutter:added fringe)) (git-gutter-fr:modified (git-gutter:modified fringe)) (git-gutter-fr:deleted (git-gutter:deleted fringe))) :bitmaps ((git-gutter-fr:added t) (git-gutter-fr:modified t) (git-gutter-fr:deleted t)) :bitmap-symbols-distinct t :mode (t t git t 3) :hunks ((modified 2 2 "@@ -2 +2 @@ alpha\n-beta\n+BETA changed") (deleted 4 4 "@@ -5 +4,0 @@ delta\n-epsilon") (added 8 9 "@@ -8,0 +8,2 @@ theta\n+added one\n+added two\n")) :refs ((:start 47 :end 57 :live t :display #3=(left-fringe git-gutter-fr:added git-gutter-fr:added)) (:start 26 :end 26 :live t :display #2=(left-fringe git-gutter-fr:deleted git-gutter-fr:deleted)) (:start 7 :end 7 :live t :display #1=(left-fringe git-gutter-fr:modified git-gutter-fr:modified))) :overlays ((:start 7 :end 7 :start-line 2 :end-line 2 :git-gutter t :fringe-helper t :parent nil :display #1#) (:start 26 :end 26 :start-line 4 :end-line 4 :git-gutter t :fringe-helper t :parent nil :display #2#) (:start 47 :end 57 :start-line 8 :end-line 9 :git-gutter t :fringe-helper t :parent nil :display #3#) (:start 57 :end 57 :start-line 9 :end-line 9 :git-gutter t :fringe-helper nil :parent 0 :display #3#)) :geometry (:margins-before (nil) :margins-after (nil) :fringes-before (0 0 nil nil) :fringes-after (0 0 nil nil)) :margin-signs nil :batch-gate (:graphic nil :window-system nil :rows ((1 nil) (2 nil) (3 nil) (4 nil) (5 nil) (6 nil) (7 nil) (8 nil) (9 nil) (10 nil) (11 nil) (12 nil)))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :mode nil :enabled nil :refs nil :owned-overlays nil :processes-live nil :process-buffers-live nil :window-restored t :margins-restored t :fringes-restored t :buffer-restored t :body-error nil :cleanup-errors nil :environment-restored t :callbacks-restored t))"#
    ]];
    ParityBatchCase::value(
        "mixed_hunks_install_exact_left_fringe_display_specs",
        elisp_form,
        expected,
    )
}

fn right_side_saved_refresh_replaces_the_entire_generation() -> ParityBatchCase {
    let elisp_form = r#"(ggf351-test-run
 "right-refresh"
 (lambda (_fixture baseline-margins baseline-fringes)
   (setq git-gutter-fr:side 'right-fringe)
   (git-gutter-mode 1)
   (ggf351-test-wait 3)
   (let* ((old-overlays
           (seq-filter
            (lambda (overlay)
              (or (overlay-get overlay 'git-gutter)
                  (overlay-get overlay 'fringe-helper-parent)))
            (apply #'append (overlay-lists))))
          (old (ggf351-test-owned-overlays))
          refreshed)
     (erase-buffer)
     (insert
      "alpha\nbeta\nGAMMA moved\ndelta\nepsilon\nzeta\neta\ntheta\nadded solo\niota\nKAPPA changed\n")
     (save-buffer)
     (ggf351-test-wait 3)
     (setq refreshed
           (list
            :old old
            :old-live (mapcar (lambda (overlay)
                                (and (overlay-buffer overlay) t))
                              old-overlays)
            :hunks (ggf351-test-hunks)
            :refs (ggf351-test-refs)
            :overlays (ggf351-test-owned-overlays)
            :all-right
            (seq-every-p
             (lambda (entry)
               (eq (car (plist-get entry :display)) 'right-fringe))
             (ggf351-test-owned-overlays))
            :any-left
            (seq-some
             (lambda (entry)
               (eq (car (plist-get entry :display)) 'left-fringe))
             (ggf351-test-owned-overlays))
            :geometry
            (list :margins-before baseline-margins
                  :margins-after (window-margins)
                  :fringes-before baseline-fringes
                  :fringes-after (window-fringes))))
     refreshed)))"#;
    let expected = expect![[
        r#"OK (:result (:old ((:start 7 :end 7 :start-line 2 :end-line 2 :git-gutter t :fringe-helper t :parent nil :display (right-fringe git-gutter-fr:modified git-gutter-fr:modified)) (:start 26 :end 26 :start-line 4 :end-line 4 :git-gutter t :fringe-helper t :parent nil :display (right-fringe git-gutter-fr:deleted git-gutter-fr:deleted)) (:start 47 :end 57 :start-line 8 :end-line 9 :git-gutter t :fringe-helper t :parent nil :display #1=(right-fringe git-gutter-fr:added git-gutter-fr:added)) (:start 57 :end 57 :start-line 9 :end-line 9 :git-gutter t :fringe-helper nil :parent 0 :display #1#)) :old-live (nil nil nil nil) :hunks ((modified 3 3 "@@ -3 +3 @@ beta\n-gamma\n+GAMMA moved") (added 9 9 "@@ -8,0 +9 @@ theta\n+added solo") (modified 11 11 "@@ -10 +11 @@ iota\n-kappa\n+KAPPA changed\n")) :refs ((:start 69 :end 69 :live t :display #4=(right-fringe git-gutter-fr:modified git-gutter-fr:modified)) (:start 53 :end 53 :live t :display #3=(right-fringe git-gutter-fr:added git-gutter-fr:added)) (:start 12 :end 12 :live t :display #2=(right-fringe git-gutter-fr:modified git-gutter-fr:modified))) :overlays ((:start 12 :end 12 :start-line 3 :end-line 3 :git-gutter t :fringe-helper t :parent nil :display #2#) (:start 53 :end 53 :start-line 9 :end-line 9 :git-gutter t :fringe-helper t :parent nil :display #3#) (:start 69 :end 69 :start-line 11 :end-line 11 :git-gutter t :fringe-helper t :parent nil :display #4#)) :all-right t :any-left nil :geometry (:margins-before (nil) :margins-after (nil) :fringes-before (0 0 nil nil) :fringes-after (0 0 nil nil))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :mode nil :enabled nil :refs nil :owned-overlays nil :processes-live nil :process-buffers-live nil :window-restored t :margins-restored t :fringes-restored t :buffer-restored t :body-error nil :cleanup-errors nil :environment-restored t :callbacks-restored t))"#
    ]];
    ParityBatchCase::value(
        "right_side_saved_refresh_replaces_the_entire_generation",
        elisp_form,
        expected,
    )
}

fn navigation_popup_mark_and_edit_hooks_use_the_real_generation() -> ParityBatchCase {
    let elisp_form = r#"(ggf351-test-run
 "navigation-edit"
 (lambda (_fixture _baseline-margins _baseline-fringes)
   (git-gutter-mode 1)
   (ggf351-test-wait 3)
   (let ((generation-before (ggf351-test-owned-overlays))
         (owned-before (ggf351-test-owned-overlay-objects))
         next reverse end mark popup edited cleared)
     (goto-char (point-min))
     (dotimes (_ 4)
       (git-gutter:next-hunk 1)
       (push (list (line-number-at-pos) (current-column)) next))
     (git-gutter:previous-hunk 1)
     (setq reverse (list (line-number-at-pos) (current-column)))
     (git-gutter:end-of-hunk)
     (setq end (list (line-number-at-pos) (current-column)))
     (git-gutter:mark-hunk)
     (setq mark
           (list :point (line-number-at-pos (point))
                 :mark (line-number-at-pos (mark))
                 :active mark-active
                 :text (buffer-substring-no-properties
                        (region-beginning) (region-end))))
     (deactivate-mark)
     (goto-char (point-min))
     (git-gutter:next-hunk 1)
     (git-gutter:popup-hunk)
     (setq popup
           (with-current-buffer git-gutter:popup-buffer
             (list :mode major-mode :read-only buffer-read-only
                   :text (buffer-substring-no-properties
                          (point-min) (point-max)))))
     (goto-char (point-min))
     (forward-line 7)
     (end-of-line)
     (insert "\nadded middle")
     (setq edited
           (list :text (buffer-substring-no-properties (point-min) (point-max))
                 :overlays (ggf351-test-owned-overlays)
                 :generation-before-navigation generation-before
                 :generation-after-navigation
                 (mapcar (lambda (overlay)
                           (and (overlay-buffer overlay) t))
                         owned-before)))
     (let ((owned-after-edit (ggf351-test-owned-overlay-objects)))
       (git-gutter:clear)
       (setq cleared
             (list :mode git-gutter-mode :enabled git-gutter:enabled
                   :refs git-gutter-fr:bitmap-references
                   :overlays (ggf351-test-owned-overlays)
                   :old-live
                   (mapcar (lambda (overlay)
                             (and (overlay-buffer overlay) t))
                           owned-after-edit))))
     (when (buffer-live-p (get-buffer git-gutter:popup-buffer))
       (kill-buffer git-gutter:popup-buffer))
     (list :next (nreverse next) :previous reverse :end end :mark mark
           :popup popup :edited edited :cleared cleared))))"#;
    let expected = expect![[
        r#"OK (:result (:next ((2 0) (4 0) (8 0) (2 0)) :previous (8 0) :end (9 0) :mark (:point 8 :mark 10 :active t :text "added one\nadded two\n") :popup (:mode diff-mode :read-only t :text "@@ -2 +2 @@ alpha\n-beta\n+BETA changed\n") :edited (:text "alpha\nBETA changed\ngamma\ndelta\nzeta\neta\ntheta\nadded one\nadded middle\nadded two\niota\nkappa\n" :overlays ((:start 7 :end 7 :start-line 2 :end-line 2 :git-gutter t :fringe-helper t :parent nil :display #2=(left-fringe git-gutter-fr:modified git-gutter-fr:modified)) (:start 26 :end 26 :start-line 4 :end-line 4 :git-gutter t :fringe-helper t :parent nil :display #3=(left-fringe git-gutter-fr:deleted git-gutter-fr:deleted)) (:start 47 :end 70 :start-line 8 :end-line 10 :git-gutter t :fringe-helper t :parent nil :display #1=(left-fringe git-gutter-fr:added git-gutter-fr:added)) (:start 57 :end 57 :start-line 9 :end-line 9 :git-gutter nil :fringe-helper nil :parent 0 :display #1#) (:start 70 :end 70 :start-line 10 :end-line 10 :git-gutter t :fringe-helper nil :parent 0 :display #1#)) :generation-before-navigation ((:start 7 :end 7 :start-line 2 :end-line 2 :git-gutter t :fringe-helper t :parent nil :display #2#) (:start 26 :end 26 :start-line 4 :end-line 4 :git-gutter t :fringe-helper t :parent nil :display #3#) (:start 47 :end 57 :start-line 8 :end-line 9 :git-gutter t :fringe-helper t :parent nil :display #1#) (:start 57 :end 57 :start-line 9 :end-line 9 :git-gutter t :fringe-helper nil :parent 0 :display #1#)) :generation-after-navigation (t t t t)) :cleared (:mode nil :enabled nil :refs nil :overlays nil :old-live (nil nil nil nil nil))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :mode nil :enabled nil :refs nil :owned-overlays nil :processes-live nil :process-buffers-live nil :window-restored t :margins-restored t :fringes-restored t :buffer-restored t :body-error nil :cleanup-errors nil :environment-restored t :callbacks-restored t))"#
    ]];
    ParityBatchCase::value(
        "navigation_popup_mark_and_edit_hooks_use_the_real_generation",
        elisp_form,
        expected,
    )
}

fn clean_toggle_and_linum_preserve_unrelated_ownership() -> ParityBatchCase {
    let elisp_form = r#"(ggf351-test-run
 "toggle-linum"
 (lambda (_fixture baseline-margins baseline-fringes)
   (let ((sentinel (make-overlay (point-min) (point-min)))
         dirty clean boundary off on final)
     (overlay-put sentinel 'ggf351-unrelated 'sentinel)
     (git-gutter-mode 1)
     (ggf351-test-wait 3)
     (when (require 'linum nil t) (linum-mode 1))
     (setq dirty
           (list :hunks (ggf351-test-hunks)
                 :overlays (ggf351-test-owned-overlays)
                 :linum (ggf351-test-linum-artifacts)
                 :sentinel (overlay-get sentinel 'ggf351-unrelated)))
     (erase-buffer)
     (insert ggf351-test-baseline)
     (save-buffer)
     (ggf351-test-wait 0)
     (setq clean
           (list :mode git-gutter-mode :enabled git-gutter:enabled
                 :hunks git-gutter:diffinfos
                 :refs git-gutter-fr:bitmap-references
                 :overlays (ggf351-test-owned-overlays)
                 :linum (ggf351-test-linum-artifacts)
                 :sentinel (overlay-get sentinel 'ggf351-unrelated)))
     ;; Delete the first committed row so the zero-length boundary anchor is
     ;; exercised by the public refresh/toggle lifecycle.
     (goto-char (point-min))
     (delete-region (point) (progn (forward-line 1) (point)))
     (save-buffer)
     (ggf351-test-wait 1)
     (setq boundary
           (list :hunks (ggf351-test-hunks)
                 :overlays (ggf351-test-owned-overlays)
                 :linum (ggf351-test-linum-artifacts)))
     (git-gutter:toggle)
     (setq off
           (list :mode git-gutter-mode :enabled git-gutter:enabled
                 :refs git-gutter-fr:bitmap-references
                 :overlays (ggf351-test-owned-overlays)
                 :linum (ggf351-test-linum-artifacts)
                 :sentinel (overlay-get sentinel 'ggf351-unrelated)))
     (git-gutter:toggle)
     (ggf351-test-wait 1)
     (setq on
           (list :mode git-gutter-mode :enabled git-gutter:enabled
                 :hunks (ggf351-test-hunks)
                 :refs (ggf351-test-refs)
                 :overlays (ggf351-test-owned-overlays)
                 :linum (ggf351-test-linum-artifacts)
                 :sentinel (overlay-get sentinel 'ggf351-unrelated)))
     (git-gutter:toggle)
     (when (bound-and-true-p linum-mode) (linum-mode -1))
     (setq final
           (list :mode git-gutter-mode :enabled git-gutter:enabled
                 :refs git-gutter-fr:bitmap-references
                 :overlays (ggf351-test-owned-overlays)
                 :sentinel (overlay-get sentinel 'ggf351-unrelated)
                 :margins (list baseline-margins (window-margins))
                 :fringes (list baseline-fringes (window-fringes))))
     (delete-overlay sentinel)
     (list :dirty dirty :clean clean :boundary boundary
           :off off :on on :final final))))"#;
    let expected = expect![[
        r#"OK (:result (:dirty (:hunks ((modified 2 2 "@@ -2 +2 @@ alpha\n-beta\n+BETA changed") (deleted 4 4 "@@ -5 +4,0 @@ delta\n-epsilon") (added 8 9 "@@ -8,0 +8,2 @@ theta\n+added one\n+added two\n")) :overlays ((:start 7 :end 7 :start-line 2 :end-line 2 :git-gutter t :fringe-helper t :parent nil :display (left-fringe git-gutter-fr:modified git-gutter-fr:modified)) (:start 26 :end 26 :start-line 4 :end-line 4 :git-gutter t :fringe-helper t :parent nil :display (left-fringe git-gutter-fr:deleted git-gutter-fr:deleted)) (:start 47 :end 57 :start-line 8 :end-line 9 :git-gutter t :fringe-helper t :parent nil :display #1=(left-fringe git-gutter-fr:added git-gutter-fr:added)) (:start 57 :end 57 :start-line 9 :end-line 9 :git-gutter t :fringe-helper nil :parent 0 :display #1#)) :linum (:mode t :artifacts ((:line 1 :live t :text " 1" :display #13=(#2=(margin left-margin) #(" 1" 0 2 (face linum)))) (:line 2 :live t :text " 2" :display #12=(#2# #(" 2" 0 2 (face linum)))) (:line 3 :live t :text " 3" :display #11=(#2# #(" 3" 0 2 (face linum)))) (:line 4 :live t :text " 4" :display #10=(#2# #(" 4" 0 2 (face linum)))) (:line 5 :live t :text " 5" :display #9=(#2# #(" 5" 0 2 (face linum)))) (:line 6 :live t :text " 6" :display #8=(#2# #(" 6" 0 2 (face linum)))) (:line 7 :live t :text " 7" :display #7=(#2# #(" 7" 0 2 (face linum)))) (:line 8 :live t :text " 8" :display #6=(#2# #(" 8" 0 2 (face linum)))) (:line 9 :live t :text " 9" :display #5=(#2# #(" 9" 0 2 (face linum)))) (:line 10 :live t :text "10" :display #4=(#2# #("10" 0 2 (face linum)))) (:line 11 :live t :text "11" :display #3=(#2# #("11" 0 2 (face linum)))))) :sentinel sentinel) :clean (:mode t :enabled t :hunks nil :refs nil :overlays nil :linum (:mode t :artifacts ((:line 1 :live t :text "11" :display #3#) (:line 1 :live t :text "10" :display #4#) (:line 1 :live t :text " 9" :display #5#) (:line 1 :live t :text " 8" :display #6#) (:line 1 :live t :text " 7" :display #7#) (:line 1 :live t :text " 6" :display #8#) (:line 1 :live t :text " 5" :display #9#) (:line 1 :live t :text " 4" :display #10#) (:line 1 :live t :text " 3" :display #11#) (:line 1 :live t :text " 2" :display #12#) (:line 1 :live t :text " 1" :display #13#))) :sentinel sentinel) :boundary (:hunks ((deleted 1 1 "@@ -1 +0,0 @@\n-alpha\n")) :overlays ((:start 1 :end 1 :start-line 1 :end-line 1 :git-gutter t :fringe-helper t :parent nil :display (left-fringe git-gutter-fr:deleted git-gutter-fr:deleted))) :linum (:mode t :artifacts ((:line 1 :live t :text "11" :display #3#) (:line 1 :live t :text "10" :display #4#) (:line 1 :live t :text " 9" :display #5#) (:line 1 :live t :text " 8" :display #6#) (:line 1 :live t :text " 7" :display #7#) (:line 1 :live t :text " 6" :display #8#) (:line 1 :live t :text " 5" :display #9#) (:line 1 :live t :text " 4" :display #10#) (:line 1 :live t :text " 3" :display #11#) (:line 1 :live t :text " 2" :display #12#) (:line 1 :live t :text " 1" :display #13#)))) :off (:mode nil :enabled nil :refs nil :overlays nil :linum (:mode t :artifacts ((:line 1 :live t :text "11" :display #3#) (:line 1 :live t :text "10" :display #4#) (:line 1 :live t :text " 9" :display #5#) (:line 1 :live t :text " 8" :display #6#) (:line 1 :live t :text " 7" :display #7#) (:line 1 :live t :text " 6" :display #8#) (:line 1 :live t :text " 5" :display #9#) (:line 1 :live t :text " 4" :display #10#) (:line 1 :live t :text " 3" :display #11#) (:line 1 :live t :text " 2" :display #12#) (:line 1 :live t :text " 1" :display #13#))) :sentinel sentinel) :on (:mode t :enabled t :hunks ((deleted 1 1 "@@ -1 +0,0 @@\n-alpha\n")) :refs ((:start 1 :end 1 :live t :display #14=(left-fringe git-gutter-fr:deleted git-gutter-fr:deleted))) :overlays ((:start 1 :end 1 :start-line 1 :end-line 1 :git-gutter t :fringe-helper t :parent nil :display #14#)) :linum (:mode t :artifacts ((:line 1 :live t :text "11" :display #3#) (:line 1 :live t :text "10" :display #4#) (:line 1 :live t :text " 9" :display #5#) (:line 1 :live t :text " 8" :display #6#) (:line 1 :live t :text " 7" :display #7#) (:line 1 :live t :text " 6" :display #8#) (:line 1 :live t :text " 5" :display #9#) (:line 1 :live t :text " 4" :display #10#) (:line 1 :live t :text " 3" :display #11#) (:line 1 :live t :text " 2" :display #12#) (:line 1 :live t :text " 1" :display #13#))) :sentinel sentinel) :final (:mode nil :enabled nil :refs nil :overlays nil :sentinel sentinel :margins ((nil) (nil)) :fringes ((0 0 nil nil) (0 0 nil nil)))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :mode nil :enabled nil :refs nil :owned-overlays nil :processes-live nil :process-buffers-live nil :window-restored t :margins-restored t :fringes-restored t :buffer-restored t :body-error nil :cleanup-errors nil :environment-restored t :callbacks-restored t))"#
    ]];
    ParityBatchCase::value(
        "clean_toggle_and_linum_preserve_unrelated_ownership",
        elisp_form,
        expected,
    )
}

fn external_git_failure_clears_then_real_git_recovers() -> ParityBatchCase {
    let elisp_form = r##"(ggf351-test-run
 "git-failure-recovery"
 (lambda (fixture _baseline-margins _baseline-fringes)
   (erase-buffer)
   (insert "alpha\nBETA\ngamma\ndelta\nepsilon\nzeta\neta\ntheta\niota\nkappa\n")
   (save-buffer)
   (git-gutter-mode 1)
   (ggf351-test-wait 1)
   (let* ((before
           (list :mode git-gutter-mode :enabled git-gutter:enabled
                 :vcs git-gutter:vcs-type :hunks (ggf351-test-hunks)
                 :refs (length git-gutter-fr:bitmap-references)
                 :overlays (length (ggf351-test-owned-overlays))))
          (real-git (plist-get fixture :real-git))
          (bin (expand-file-name "fail-bin" (plist-get fixture :root)))
          (wrapper (expand-file-name "git" bin))
          (trace (expand-file-name "failure.trace" (plist-get fixture :root)))
          (python
           (or (executable-find "python3" t)
               (error "GGF351: absolute python3 is required for failure shim")))
          (expected-cwd
           (directory-file-name (file-name-directory (buffer-file-name))))
          (script
           (format
            (concat
             "#!%s\n"
             "import os, pathlib, sys\n"
             "expected = ['--no-pager', '-c', 'diff.autorefreshindex=0', 'diff', '--no-color', '--no-ext-diff', '--relative', '-U0', '--', 'sample.txt']\n"
             "actual = sys.argv[1:]\n"
             "cwd = os.getcwd()\n"
             "trace = pathlib.Path(%S)\n"
             "if cwd != %S or actual != expected:\n"
             "    fields = ['interpreter', sys.executable, 'route', 'unexpected', 'cwd', cwd, 'argc', str(len(actual)), *actual, 'status', '97']\n"
             "    trace.write_bytes(('\\0'.join(fields) + '\\0').encode('utf-8'))\n"
             "    sys.stderr.write('GGF351_FAIL_GIT: unexpected request\\n')\n"
             "    raise SystemExit(97)\n"
             "fields = ['interpreter', sys.executable, 'route', 'recognized-diff', 'cwd', cwd, 'argc', str(len(actual)), *actual, 'status', '7']\n"
             "trace.write_bytes(('\\0'.join(fields) + '\\0').encode('utf-8'))\n"
             "sys.stderr.write('GGF351_FAIL_GIT: intentional diff failure\\n')\n"
             "raise SystemExit(7)\n")
            python trace expected-cwd))
          failure failure-process recovery trace-fields)
     (make-directory bin)
     (unless (and (file-name-absolute-p python)
                  (file-executable-p python)
                  (not (string-match-p "[\n\r]" python)))
       (error "GGF351: unsafe resolved python3 interpreter: %S" python))
     (ggf351-test-write wrapper script)
     (set-file-modes wrapper #o755)
     (let ((exec-path (cons bin exec-path))
           (process-environment (copy-sequence process-environment)))
       (setenv "PATH" (concat bin path-separator (getenv "PATH")))
       (call-interactively #'git-gutter)
       (setq failure-process (ggf351-test-wait 0))
       (setq failure
             (list :mode git-gutter-mode :enabled git-gutter:enabled
                   :vcs git-gutter:vcs-type :hunks git-gutter:diffinfos
                   :refs (length git-gutter-fr:bitmap-references)
                   :overlays (length (ggf351-test-owned-overlays))
                   :process
                   (list :status (process-status failure-process)
                         :exit (process-exit-status failure-process)
                         :buffer-live
                         (buffer-live-p (process-buffer failure-process))))))
     (call-interactively #'git-gutter)
     (ggf351-test-wait 1)
     (setq recovery
           (list :mode git-gutter-mode :enabled git-gutter:enabled
                 :vcs git-gutter:vcs-type :hunks (ggf351-test-hunks)
                 :refs (length git-gutter-fr:bitmap-references)
                 :overlays (length (ggf351-test-owned-overlays))))
     (setq trace-fields
           (with-temp-buffer
             (insert-file-contents-literally trace)
             (split-string (decode-coding-string (buffer-string) 'utf-8)
                           "\0" t)))
     (unless (equal (cadr trace-fields) python)
       (error "GGF351: failure shim used wrong interpreter: expected=%S trace=%S"
              python trace-fields))
     (setf (cadr trace-fields) "[PYTHON3]")
     (list :real-git-absolute (file-name-absolute-p real-git)
           :python-absolute (file-name-absolute-p python)
           :before before :failure failure :recovery recovery
           :trace trace-fields))))"##;
    let expected = expect![[
        r#"OK (:result (:real-git-absolute t :python-absolute t :before (:mode t :enabled t :vcs git :hunks ((modified 2 2 "@@ -2 +2 @@ alpha\n-beta\n+BETA\n")) :refs 1 :overlays 1) :failure (:mode t :enabled t :vcs git :hunks nil :refs 0 :overlays 0 :process (:status exit :exit 7 :buffer-live nil)) :recovery (:mode t :enabled t :vcs git :hunks ((modified 2 2 "@@ -2 +2 @@ alpha\n-beta\n+BETA\n")) :refs 1 :overlays 1) :trace ("interpreter" "[PYTHON3]" "route" "recognized-diff" "cwd" "[ORACLE-SANDBOX]/git-gutter-fringe-git-failure-recovery/project space Ω/src" "argc" "10" "--no-pager" "-c" "diff.autorefreshindex=0" "diff" "--no-color" "--no-ext-diff" "--relative" "-U0" "--" "sample.txt" "status" "7")) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :root-exists nil :root-owned nil :mode nil :enabled nil :refs nil :owned-overlays nil :processes-live nil :process-buffers-live nil :window-restored t :margins-restored t :fringes-restored t :buffer-restored t :body-error nil :cleanup-errors nil :environment-restored t :callbacks-restored t))"#
    ]];
    ParityBatchCase::value(
        "external_git_failure_clears_then_real_git_recovers",
        elisp_form,
        expected,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mixed_hunks_install_exact_left_fringe_display_specs(),
        right_side_saved_refresh_replaces_the_entire_generation(),
        navigation_popup_mark_and_edit_hooks_use_the_real_generation(),
        clean_toggle_and_linum_preserve_unrelated_ownership(),
        external_git_failure_clears_then_real_git_recovers(),
    ]
}
