use expect_test::expect;

use super::ParityBatchCase;

fn magit_section_hide_show_and_toggle_preserve_body_and_hidden_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_section_hide_show_and_toggle_preserve_body_and_hidden_state",
        r##"(with-temp-buffer
               (magit-section-mode)
               (let ((inhibit-read-only t))
                 (magit-insert-section (root nil)
                   (magit-insert-heading "Root")
                   (magit-insert-section (item 'one)
                     (magit-insert-heading "One")
                     (insert "body\n")))
                 (let* ((item (car (oref magit-root-section children)))
                        (content (marker-position (oref item content))))
                   (magit-section-hide item)
                   (let ((hidden
                          (list (oref item hidden)
                                (invisible-p content)
                                (buffer-substring-no-properties
                                 (point-min) (point-max)))))
                     (magit-section-show item)
                     (let ((shown
                            (list (oref item hidden)
                                  (invisible-p content)
                                  (buffer-substring-no-properties
                                   (point-min) (point-max)))))
                       (magit-section-toggle item)
                       (list hidden
                             shown
                             (list (oref item hidden)
                                   (invisible-p content)
                                   (buffer-substring-no-properties
                                    (point-min) (point-max)))))))))"##,
        expect![[
            r#"OK ((t t "Root\nOne\nbody\n") (nil nil "Root\nOne\nbody\n") (t t "Root\nOne\nbody\n"))"#
        ]],
    )
}

fn magit_section_show_and_hide_children_recurse_with_exact_depth() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_section_show_and_hide_children_recurse_with_exact_depth",
        r##"(with-temp-buffer
               (magit-section-mode)
               (let ((inhibit-read-only t))
                 (magit-insert-section (root nil)
                   (magit-insert-heading "Root")
                   (magit-insert-section (group 'g)
                     (magit-insert-heading "Group")
                     (magit-insert-section (item 'one)
                       (magit-insert-heading "One")
                       (insert "one body\n"))
                     (magit-insert-section (item 'two)
                       (magit-insert-heading "Two")
                       (insert "two body\n"))))
                 (let* ((root magit-root-section)
                        (group (car (oref root children)))
                        (one (car (oref group children)))
                        (two (cadr (oref group children))))
                   (magit-section-hide-children root)
                   (let ((hidden (mapcar
                                  (lambda (section)
                                    (and (oref section hidden) t))
                                  (list group one two))))
                     (magit-section-show-children root 0)
                     (let ((depth-zero
                            (mapcar
                             (lambda (section)
                               (and (oref section hidden) t))
                             (list group one two))))
                       (magit-section-show-children root)
                       (list
                        hidden
                        depth-zero
                        (mapcar
                         (lambda (section)
                           (and (oref section hidden) t))
                         (list group one two))))))))"##,
        expect![[r#"OK ((t nil nil) (t nil nil) (nil nil nil))"#]],
    )
}

fn magit_section_lazy_body_is_inserted_only_when_first_shown() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_section_lazy_body_is_inserted_only_when_first_shown",
        r##"(with-temp-buffer
               (magit-section-mode)
               (let ((inhibit-read-only t)
                     (calls 0))
                 (magit-insert-section (root nil)
                   (magit-insert-heading "Root")
                   (magit-insert-section (item 'lazy t)
                     (magit-insert-heading "Lazy")
                     (magit-insert-section-body
                       (setq calls (1+ calls))
                       (insert "deferred body\n"))))
                 (let ((item (car (oref magit-root-section children))))
                   (let ((before (list calls
                                       (buffer-substring-no-properties
                                        (point-min) (point-max))
                                       (functionp (oref item washer)))))
                     (magit-section-show item)
                     (let ((first (list calls
                                        (buffer-substring-no-properties
                                         (point-min) (point-max))
                                        (oref item washer))))
                       (magit-section-hide item)
                       (magit-section-show item)
                       (list before
                             first
                             (list calls
                                   (buffer-substring-no-properties
                                    (point-min) (point-max))
                                   (oref item washer))))))))"##,
        expect![[
            r#"OK ((0 "Root\nLazy\n" t) (1 "Root\nLazy\ndeferred body\n" nil) (1 "Root\nLazy\ndeferred body\n" nil))"#
        ]],
    )
}

fn magit_section_root_cannot_be_hidden() -> ParityBatchCase {
    ParityBatchCase::signal(
        "magit_section_root_cannot_be_hidden",
        r##"(with-temp-buffer
               (magit-section-mode)
               (let ((inhibit-read-only t))
                 (magit-insert-section (root nil)
                   (magit-insert-heading "Root"))
                 (magit-section-hide magit-root-section)))"##,
        expect![[r#"ERR (user-error "Cannot hide root section")"#]],
    )
}

pub(super) fn visibility_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        magit_section_hide_show_and_toggle_preserve_body_and_hidden_state(),
        magit_section_show_and_hide_children_recurse_with_exact_depth(),
        magit_section_lazy_body_is_inserted_only_when_first_shown(),
        magit_section_root_cannot_be_hidden(),
    ]
}
