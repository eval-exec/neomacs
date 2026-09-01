use expect_test::expect;

use super::ParityBatchCase;

fn buffer_names_and_position_translation_are_deterministic() -> ParityBatchCase {
    ParityBatchCase::value(
        "buffer_names_and_position_translation_are_deterministic",
        r####"
(let ((shell-pop-internal-mode-buffer "*shell*"))
  (list :name-1 (shell-pop--shell-buffer-name 1)
        :name-3 (shell-pop--shell-buffer-name 3)
        :plain
        (let ((shell-pop-internal-mode-buffer "shell"))
          (shell-pop--shell-buffer-name 2))
        :pos-top (shell-pop--translate-position "top")
        :pos-bottom (shell-pop--translate-position "bottom")
        :pos-left (shell-pop--translate-position "left")
        :pos-right (shell-pop--translate-position "right")))
"####,
        expect![[
            r#"OK (:name-1 "*shell-1*" :name-3 "*shell-3*" :plain "shell-2" :pos-top above :pos-bottom below :pos-left left :pos-right right)"#
        ]],
    )
}

fn window_size_scales_with_height_setting() -> ParityBatchCase {
    ParityBatchCase::value(
        "window_size_scales_with_height_setting",
        r####"
(let ((shell-pop-full-span nil)
      (shell-pop-window-position "bottom"))
  (list :default
        (let ((shell-pop-window-height 30))
          (shell-pop--calculate-window-size))
        :half
        (let ((shell-pop-window-height 50))
          (shell-pop--calculate-window-size))
        :almost-full
        (let ((shell-pop-window-height 90))
          (shell-pop--calculate-window-size))
        :half-is-smaller
        (let ((a (let ((shell-pop-window-height 30))
                   (shell-pop--calculate-window-size)))
              (b (let ((shell-pop-window-height 50))
                   (shell-pop--calculate-window-size))))
          (< b a))))
"####,
        expect!["OK (:default 17 :half 12 :almost-full 2 :half-is-smaller t)"],
    )
}

fn switch_to_shell_buffer_creates_and_renames_via_mode_func() -> ParityBatchCase {
    ParityBatchCase::value(
        "switch_to_shell_buffer_creates_and_renames_via_mode_func",
        r####"
(let ((shell-pop-internal-mode "shell")
      (shell-pop-internal-mode-buffer "*shell*")
      (shell-pop-internal-mode-func #'neomacs-shell-pop-test-fake-shell)
      (shell-pop-last-shell-buffer-index 1)
      (shell-pop--is-shell-buffer nil))
  (unwind-protect
      (progn
        (dolist (name '("*shell*" "*shell-1*" "*shell-2*"))
          (when (get-buffer name)
            (let ((kill-buffer-query-functions nil)
                  (kill-buffer-hook nil))
              (kill-buffer name))))
        (shell-pop--switch-to-shell-buffer 1)
        (list :created (buffer-name)
              :is-shell shell-pop--is-shell-buffer
              :index shell-pop-last-shell-buffer-index
              :lives (and (get-buffer "*shell-1*") t)
              :contents
              (with-current-buffer "*shell-1*"
                (string-trim (buffer-string)))
              :second
              (progn
                (shell-pop--switch-to-shell-buffer 2)
                (list :name (buffer-name)
                      :index shell-pop-last-shell-buffer-index
                      :first-still-live (and (get-buffer "*shell-1*") t)))))
    (dolist (name '("*shell*" "*shell-1*" "*shell-2*"))
      (when (get-buffer name)
        (let ((kill-buffer-query-functions nil)
              (kill-buffer-hook nil))
          (kill-buffer name))))))
"####,
        expect![[
            r##"OK (:created "*shell-1*" :is-shell t :index 1 :lives t :contents "# fake shell" :second (:name "*shell-2*" :index 2 :first-still-live t))"##
        ]],
    )
}

fn unused_index_scan_skips_existing_shell_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "unused_index_scan_skips_existing_shell_buffers",
        r####"
(let ((shell-pop-internal-mode-buffer "*shell*")
      (a (get-buffer-create "*shell-1*"))
      (b (get-buffer-create "*shell-2*")))
  (unwind-protect
      (let ((cell (shell-pop-get-unused-internal-mode-buffer-window)))
        (list :index (car cell)
              :window (cdr cell)
              :name-matches (equal (shell-pop--shell-buffer-name (car cell))
                                   "*shell-3*")))
    (let ((kill-buffer-query-functions nil)
          (kill-buffer-hook nil))
      (when (buffer-live-p a) (kill-buffer a))
      (when (buffer-live-p b) (kill-buffer b)))))
"####,
        expect!["OK (:index 3 :window nil :name-matches t)"],
    )
}

fn target_index_handles_prefix_and_default() -> ParityBatchCase {
    ParityBatchCase::value(
        "target_index_handles_prefix_and_default",
        r####"
(let ((shell-pop-last-shell-buffer-index 5)
      (shell-pop-internal-mode-buffer "*shell*"))
  (dolist (name '("*shell-1*" "*shell-2*" "*shell-3*"))
    (when (get-buffer name)
      (let ((kill-buffer-query-functions nil)
            (kill-buffer-hook nil))
        (kill-buffer name))))
  (list :nil (shell-pop--target-index nil)
        :numeric (shell-pop--target-index 3)
        :raw-prefix
        ;; C-u means "next unused index"
        (shell-pop--target-index '(4))))
"####,
        expect!["OK (:nil 5 :numeric 3 :raw-prefix 1)"],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        buffer_names_and_position_translation_are_deterministic(),
        window_size_scales_with_height_setting(),
        switch_to_shell_buffer_creates_and_renames_via_mode_func(),
        unused_index_scan_skips_existing_shell_buffers(),
        target_index_handles_prefix_and_default(),
    ]
}
