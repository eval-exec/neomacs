use expect_test::expect;

use super::ParityBatchCase;

/// A search over a real fixture tree populates the `rg-mode' results
/// buffer with grouped file headers and match rows, the first match on
/// each row carries `rg-match-face', the command line is hidden per
/// `rg-hide-command', and `rg-next-file'/`rg-prev-file' walk the file
/// groups in both directions.
fn a_search_populates_the_results_buffer_and_navigates_files() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_search_populates_the_results_buffer_and_navigates_files",
        r####"(unwind-protect
    (progn
      (rg-test-reset)
      (let* ((root (rg-test-root "widgets"))
             (a (expand-file-name "a.txt" root))
             (b (expand-file-name "b.txt" root))
             (c (expand-file-name "c.txt" root)))
        (rg-test-write a "widget one\nplain\na widget two\n")
        (rg-test-write b "line\nwidget here\n")
        (rg-test-write c "nothing to see\n")
        (let* ((buffer (rg-test-run "widget" root))
               (first-match-pos
                (progn
                  (rg-test-wait buffer)
                  (with-current-buffer buffer
                    (goto-char (point-min))
                    ;; The header line names the fixture directory and the
                    ;; hidden command line repeats the pattern, so the bare
                    ;; pattern first hits the header rather than a match;
                    ;; locate the RESULT row through its full match text
                    ;; instead.
                    (search-forward "widget one")
                    (match-beginning 0)))))
          (with-current-buffer buffer
            (goto-char (point-min))
            (let ((initial
                   (list :mode major-mode
                         :grouped rg-group-result
                         :content
                         (rg-test-mask
                          (buffer-substring-no-properties (point-min) (point-max)))
                         :command-hidden
                         (let ((hidden nil))
                           (goto-char (point-min))
                           (while (not (eobp))
                             (when (invisible-p (point))
                               (setq hidden t))
                             (forward-line))
                           hidden)
                         :first-match-faces
                         (let ((faces nil))
                           (goto-char first-match-pos)
                           (while (and (not (eolp))
                                       (not (get-text-property (point) 'rg-file-message)))
                             ;; `rg-filter' writes the match highlight into
                             ;; `font-lock-face' and explicitly clears `face',
                             ;; so both have to be recorded to say which one
                             ;; carries `rg-match-face'.
                             (push (list (buffer-substring-no-properties
                                          (point) (1+ (point)))
                                         (get-text-property (point) 'face)
                                         (get-text-property (point) 'font-lock-face))
                                   faces)
                             (forward-char))
                           (nreverse faces))
                         :point-offset (rg-test-offset (point))
                         :file-tags
                         (let (tags)
                           (goto-char (point-min))
                           (while (not (eobp))
                             (when (get-text-property (point) 'rg-file-message)
                               (push t tags))
                             (forward-line))
                           (length tags)))))
              (rg-next-file 1)
              (let ((next-file (list :point-offset (rg-test-offset (point))
                                     :line (buffer-substring-no-properties
                                            (line-beginning-position)
                                            (line-end-position)))))
                (rg-prev-file 1)
                (list :initial initial
                      :next-file next-file
                      :prev-file
                      (list :point-offset (rg-test-offset (point))
                            :line (buffer-substring-no-properties
                                   (line-beginning-position)
                                   (line-end-position))))))))))
  (rg-test-reset))"####,
        expect![[
            r#"OK (:initial (:mode rg-mode :grouped t :content "-*- mode: rg; default-directory: \"[ORACLE-SANDBOX]/rg-fixture-widgets/\" -*-\nrg started at [TIME]\n\n/etc/profiles/per-user/exec/bin/rg --color=always --colors=match:fg:red --colors=path:fg:magenta --colors=line:fg:green --colors=column:none -n --column -i --sort path . --heading --no-config -e widget\n\nFile: ./a.txt\n   1   1 widget one\n   3   3 a widget two\n\nFile: ./b.txt\n   2   1 widget here\n\nrg finished (3 matches found) at [TIME], duration [N] s\n" :command-hidden nil :first-match-faces (("w" nil rg-match-face) ("i" nil rg-match-face) ("d" nil rg-match-face) ("g" nil rg-match-face) ("e" nil rg-match-face) ("t" nil rg-match-face) (" " nil nil) ("o" nil nil) ("n" nil nil) ("e" nil nil)) :point-offset 272 :file-tags 2) :next-file (:point-offset 402 :line "") :prev-file (:point-offset 310 :line "   2   1 widget here"))"#
        ]],
    )
}

/// The wgrep round trip: a search result edited through wgrep writes the
/// modified line back to the file on disk when the edit is finished, and
/// the results buffer reverts to read-only rg-mode.
fn wgrep_editing_round_trips_through_the_disk() -> ParityBatchCase {
    ParityBatchCase::value(
        "wgrep_editing_round_trips_through_the_disk",
        r####"(unwind-protect
    (progn
      (rg-test-reset)
      (let* ((root (rg-test-root "wgrep"))
             (a (expand-file-name "a.txt" root)))
        (rg-test-write a "widget one\nplain\n")
        (let* ((buffer (rg-test-run "widget" root)))
          (rg-test-wait buffer)
          (with-current-buffer buffer
            (goto-char (point-min))
            ;; The hidden command line also contains the pattern, so locate
            ;; the RESULT row through its full match text instead.
            (search-forward "widget one")
            (wgrep-change-to-wgrep-mode)
            (let ((editable (list :mode major-mode
                                  :read-only buffer-read-only
                                  :content (rg-test-mask
                                            (buffer-substring-no-properties
                                             (point-min) (point-max))))))
              (goto-char (point-min))
              (search-forward "widget one")
              (replace-match "gadget one" nil t)
              (wgrep-finish-edit)
              ;; `wgrep-finish-edit' commits into the FILE BUFFER without
              ;; saving; the user's save step writes the disk.
              (let* ((file-buffer (find-file-noselect a))
                     (committed
                      (with-current-buffer file-buffer
                        (buffer-substring-no-properties
                         (point-min) (point-max)))))
                (with-current-buffer file-buffer
                  (save-buffer)
                  (kill-buffer))
                (list :editable editable
                      :finished (list :mode major-mode
                                      :read-only buffer-read-only)
                      :committed committed
                      :disk
                      (with-temp-buffer
                        (insert-file-contents a)
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))
  (rg-test-reset))"####,
        expect![[
            r#"OK (:editable (:mode rg-mode :read-only nil :content "-*- mode: rg; default-directory: \"[ORACLE-SANDBOX]/rg-fixture-wgrep/\" -*-\nrg started at [TIME]\n\n/etc/profiles/per-user/exec/bin/rg --color=always --colors=match:fg:red --colors=path:fg:magenta --colors=line:fg:green --colors=column:none -n --column -i --sort path . --heading --no-config -e widget\n\nFile: ./a.txt\n   1   1 widget one\n\nrg finished (1 matches found) at [TIME], duration [N] s\n") :finished (:mode rg-mode :read-only t) :committed "gadget one\nplain\n" :disk "gadget one\nplain\n")"#
        ]],
    )
}

/// The transient menu surface: `rg-menu' is a command, its pattern and
/// type suffix bindings exist, and the result-mode keymap carries the
/// documented navigation and editing keys.
fn the_menu_and_keymap_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_menu_and_keymap_surface",
        r####"(list
 :source (rg-test-source-state)
 :menu
 (list :command (commandp 'rg-menu)
       :pattern-suffix (assoc "p" (transient-get-suffix 'rg-menu "p"))
       :type-suffix (assoc "t" (transient-get-suffix 'rg-menu "t")))
 :keymap
 (list :next-file (lookup-key rg-mode-map (kbd "M-n"))
       :prev-file (lookup-key rg-mode-map (kbd "M-p"))
       :refresh (lookup-key rg-mode-map (kbd "g"))
       :wgrep (lookup-key rg-mode-map (kbd "r"))
       :abort (lookup-key rg-mode-map (kbd "C-c C-k")))
 :aliases
 (list :kill-rg (fboundp 'kill-rg)
       :rg-kill-current (fboundp 'rg-kill-current)
       :rg-save-search-as-name (fboundp 'rg-save-search-as-name)))"####,
        expect![[
            r#"OK (:source (:upstream-tree "77f2abe594fb0a6e6ec827dceaf70ef50f897e7c" :feature t :version "20260517.1310" :transient "20260725.1105" :wgrep "20230203.1214" :executable "rg") :menu (:command t :pattern-suffix nil :type-suffix nil) :keymap (:next-file compilation-next-error :prev-file compilation-previous-error :refresh rg-recompile :wgrep rg-rerun-change-regexp :abort kill-compilation) :aliases (:kill-rg t :rg-kill-current t :rg-save-search-as-name t))"#
        ]],
    )
}

/// The configuration surface: the result-buffer layout toggles, the
/// command-line flags plumbing, and the case-sensitivity policy all carry
/// their documented defaults.
fn the_configuration_surface() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_configuration_surface",
        r####"(let ((options
       '(rg-group-result
         rg-show-header
         rg-show-columns
         rg-hide-command
         rg-ignore-case
         rg-ignore-ripgreprc)))
  (list
   :options
   (mapcar
    (lambda (option)
      (list :option option
            :standard (eval (car (get option 'standard-value)))
            :type (get option 'custom-type)))
    options)
   :command-flags
   (list :required rg-required-command-line-flags
         :flags rg-command-line-flags)
   :faces
   (list :match (face-all-attributes 'rg-match-face)
         :filename (face-all-attributes 'rg-filename-face)
         :line-number (face-all-attributes 'rg-line-number-face))))"####,
        expect![[
            r#"OK (:options ((:option rg-group-result :standard t :type boolean) (:option rg-show-header :standard t :type boolean) (:option rg-show-columns :standard nil :type boolean) (:option rg-hide-command :standard t :type boolean) (:option rg-ignore-case :standard case-fold-search :type (choice (const :tag "Case Fold Search" case-fold-search) (const :tag "Smart" smart) (const :tag "Force" force) (const :tag "Off" nil))) (:option rg-ignore-ripgreprc :standard t :type boolean)) :command-flags (:required ("--color=always" "--colors=match:fg:red" "--colors=path:fg:magenta" "--colors=line:fg:green" "--colors=column:none" "-n") :flags nil) :faces (:match ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified)) :filename ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified)) :line-number ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        a_search_populates_the_results_buffer_and_navigates_files(),
        wgrep_editing_round_trips_through_the_disk(),
        the_menu_and_keymap_surface(),
        the_configuration_surface(),
    ]
}
