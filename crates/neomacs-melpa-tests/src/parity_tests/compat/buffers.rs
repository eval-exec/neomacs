use expect_test::expect;

use super::ParityBatchCase;

fn compat_with_work_buffer_nests_distinct_reusable_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "compat_with_work_buffer_nests_distinct_reusable_buffers",
        r##"(let (outer inner result)
               (setq result
                     (with-work-buffer
                       (setq outer (current-buffer))
                       (insert "outer")
                       (list
                        (string-match-p
                         "\\` \\*\\(work\\|temp\\)\\*"
                         (buffer-name))
                        (buffer-string)
                        (with-work-buffer
                          (setq inner (current-buffer))
                          (insert "inner")
                          (list
                           (not (eq outer inner))
                           (string-match-p
                            "\\` \\*\\(work\\|temp\\)\\*"
                            (buffer-name))
                           (buffer-string))))))
               (list result
                     (buffer-live-p outer)
                     (buffer-live-p inner)))"##,
        expect![[r#"OK ((0 "outer" (t 0 "inner")) t t)"#]],
    )
}

fn compat_insert_into_buffer_honors_default_start_and_end_ranges() -> ParityBatchCase {
    ParityBatchCase::value(
        "compat_insert_into_buffer_honors_default_start_and_end_ranges",
        r##"(list
               (with-temp-buffer
                 (let ((destination (current-buffer)))
                   (insert "abc")
                   (with-temp-buffer
                     (insert "def")
                     (insert-into-buffer destination))
                   (buffer-string)))
               (with-temp-buffer
                 (let ((destination (current-buffer)))
                   (insert "abc")
                   (with-temp-buffer
                     (insert "def")
                     (insert-into-buffer destination 2))
                   (buffer-string)))
               (with-temp-buffer
                 (let ((destination (current-buffer)))
                   (insert "abc")
                   (with-temp-buffer
                     (insert "def")
                     (insert-into-buffer destination 2 3))
                   (buffer-string))))"##,
        expect![[r#"OK ("abcdef" "abcef" "abce")"#]],
    )
}

fn compat_with_buffer_unmodified_if_unchanged_tracks_net_and_real_edits() -> ParityBatchCase {
    ParityBatchCase::value(
        "compat_with_buffer_unmodified_if_unchanged_tracks_net_and_real_edits",
        r##"(with-temp-buffer
               (insert "base")
               (set-buffer-modified-p nil)
               (let ((unchanged
                      (progn
                        (with-buffer-unmodified-if-unchanged
                          (goto-char (point-max))
                          (insert "x")
                          (delete-char -1))
                        (buffer-modified-p)))
                     changed)
                 (with-buffer-unmodified-if-unchanged
                   (goto-char (point-max))
                   (insert "!"))
                 (setq changed (buffer-modified-p))
                 (list unchanged
                       changed
                       (buffer-string))))"##,
        expect![[r#"OK (nil t "base!")"#]],
    )
}

fn compat_buffer_match_and_match_buffers_cover_boolean_name_mode_and_logic_forms() -> ParityBatchCase
{
    ParityBatchCase::value(
        "compat_buffer_match_and_match_buffers_cover_boolean_name_mode_and_logic_forms",
        r##"(let* ((first (generate-new-buffer
                            "*compat-alpha*"))
                    (second (generate-new-buffer
                             "*compat-beta*"))
                    (third (generate-new-buffer
                            "*other*"))
                    (parent (make-symbol "compat-parent-mode"))
                    (child (make-symbol "compat-child-mode")))
               (unwind-protect
                   (progn
                     (put child 'derived-mode-parent parent)
                     (with-current-buffer first
                       (setq major-mode child))
                     (with-current-buffer second
                       (setq major-mode parent))
                     (with-current-buffer third
                       (setq major-mode 'fundamental-mode))
                     (list
                      (mapcar
                       (lambda (condition)
                         (and
                          (buffer-match-p condition first)
                          t))
                       `(t
                         nil
                         "compat"
                         (derived-mode . ,parent)
                         (major-mode . ,child)
                         (not
                          (major-mode . fundamental-mode))
                         (and "alpha"
                              (major-mode . ,child))
                         (or "missing"
                             (major-mode . ,child))))
                      (mapcar
                       #'buffer-name
                       (match-buffers
                        `(or
                          (major-mode . ,child)
                          (major-mode . ,parent))
                        (list first second third)))))
                 (mapc
                  (lambda (buffer)
                    (when (buffer-live-p buffer)
                      (kill-buffer buffer)))
                  (list first second third))))"##,
        expect![[r#"OK ((t nil t t t t t t) ("*compat-beta*" "*compat-alpha*"))"#]],
    )
}

pub(super) fn buffers_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        compat_with_work_buffer_nests_distinct_reusable_buffers(),
        compat_insert_into_buffer_honors_default_start_and_end_ranges(),
        compat_with_buffer_unmodified_if_unchanged_tracks_net_and_real_edits(),
        compat_buffer_match_and_match_buffers_cover_boolean_name_mode_and_logic_forms(),
    ]
}
