use expect_test::expect;

use super::ParityBatchCase;

fn elide_truncates_long_items_when_maximum_set() -> ParityBatchCase {
    ParityBatchCase::value(
        "elide_truncates_long_items_when_maximum_set",
        r####"
(let ((browse-kill-ring-maximum-display-length nil)
      (long (make-string 40 ?x)))
  (list :unlimited (browse-kill-ring-elide long)
        :limited
        (let ((browse-kill-ring-maximum-display-length 10))
          (browse-kill-ring-elide long))
        :short
        (let ((browse-kill-ring-maximum-display-length 10))
          (browse-kill-ring-elide "abc"))))
"####,
        expect![[
            r#"OK (:unlimited "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" :limited #("xxxxxxx..." 7 10 (browse-kill-ring-extra t)) :short "abc")"#
        ]],
    )
}

fn setup_populates_browser_buffer_with_kill_ring_items() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_populates_browser_buffer_with_kill_ring_items",
        r####"
(let ((kill-ring '("alpha" "beta" "gamma"))
      (kill-ring-yank-pointer nil)
      (browse-kill-ring-display-style 'separated)
      (browse-kill-ring-display-duplicates t)
      (browse-kill-ring-maximum-display-length nil)
      (browse-kill-ring-show-preview nil)
      (orig (get-buffer-create " *neomacs-bkr-orig*"))
      (kill-buf (get-buffer-create " *neomacs-bkr*")))
  (unwind-protect
      (with-current-buffer orig
        (let ((window-config (current-window-configuration)))
          (browse-kill-ring-setup kill-buf orig nil nil window-config)
          (with-current-buffer kill-buf
            (list :mode major-mode
                  :text (string-trim (buffer-string))
                  :has-alpha (and (search-forward "alpha" nil t) t)
                  :has-beta (and (search-forward "beta" nil t) t)
                  :has-gamma (and (search-forward "gamma" nil t) t)
                  :overlays
                  (length
                   (cl-remove-if-not
                    (lambda (o) (overlay-get o 'browse-kill-ring-target))
                    (overlays-in (point-min) (point-max))))))))
    (let ((kill-buffer-hook nil)
          (kill-buffer-query-functions nil))
      (when (buffer-live-p orig) (kill-buffer orig))
      (when (buffer-live-p kill-buf) (kill-buffer kill-buf)))))
"####,
        expect![[
            r#"OK (:mode browse-kill-ring-mode :text #("alpha\n-------\nbeta\n-------\ngamma" 6 13 (browse-kill-ring-separator t browse-kill-ring-extra t) 19 26 (browse-kill-ring-separator t browse-kill-ring-extra t)) :has-alpha t :has-beta t :has-gamma t :overlays 3)"#
        ]],
    )
}

fn insert_and_highlight_copies_string_into_target_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "insert_and_highlight_copies_string_into_target_buffer",
        r####"
(with-temp-buffer
  (let ((browse-kill-ring-highlight-inserted-item nil))
    (browse-kill-ring-insert-and-highlight "hello")
    (list :text (buffer-string)
          :point (point))))
"####,
        expect![[r#"OK (:text "hello" :point 6)"#]],
    )
}

fn default_keybindings_advise_yank_pop_to_open_browser() -> ParityBatchCase {
    ParityBatchCase::value(
        "default_keybindings_advise_yank_pop_to_open_browser",
        r####"
(let ((browse-kill-ring-replace-yank t)
      (opened nil)
      (before (advice-member-p #'browse-kill-ring--yank-pop-advice 'yank-pop)))
  (when before
    (advice-remove 'yank-pop #'browse-kill-ring--yank-pop-advice))
  (browse-kill-ring-default-keybindings)
  (let ((advised (and (advice-member-p #'browse-kill-ring--yank-pop-advice
                                       'yank-pop)
                      t)))
    (cl-letf (((symbol-function 'browse-kill-ring)
               (lambda (&rest _)
                 (setq opened t)
                 nil)))
      ;; When there is no prior yank, advice should open browse-kill-ring.
      (let ((last-command 'self-insert-command)
            (this-command 'yank-pop))
        (condition-case nil
            (yank-pop)
          (error nil)))
      (list :advised advised
            :opened opened
            :idempotent
            (progn
              (browse-kill-ring-default-keybindings)
              (and (advice-member-p #'browse-kill-ring--yank-pop-advice
                                    'yank-pop)
                   t))))))
"####,
        expect!["OK (:advised t :opened t :idempotent t)"],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        elide_truncates_long_items_when_maximum_set(),
        setup_populates_browser_buffer_with_kill_ring_items(),
        insert_and_highlight_copies_string_into_target_buffer(),
        default_keybindings_advise_yank_pop_to_open_browser(),
    ]
}
