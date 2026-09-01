use expect_test::expect;

use super::ParityBatchCase;

fn magit_section_builds_exact_parent_child_tree_and_ranges() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_section_builds_exact_parent_child_tree_and_ranges",
        r##"(with-temp-buffer
               (magit-section-mode)
               (let ((inhibit-read-only t))
                 (magit-insert-section (root "root")
                   (magit-insert-heading "Root")
                   (magit-insert-section (group "group")
                     (magit-insert-heading "Group")
                     (magit-insert-section (item 1)
                       (magit-insert-heading "One")
                       (insert "body one\n"))
                     (magit-insert-section (item 2)
                       (magit-insert-heading "Two")
                       (insert "body two\n"))))
                 (let* ((root magit-root-section)
                        (group (car (oref root children)))
                        (one (car (oref group children)))
                        (two (cadr (oref group children))))
                   (list
                    (mapcar (lambda (section)
                              (list (oref section type)
                                    (oref section value)
                                    (marker-position (oref section start))
                                    (marker-position (oref section content))
                                    (marker-position (oref section end))))
                            (list root group one two))
                    (eq (oref group parent) root)
                    (eq (oref one parent) group)
                    (eq (oref two parent) group)
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))"##,
        expect![[
            r#"OK (((root "root" 1 6 38) (group "group" 6 12 38) (item 1 12 16 25) (item 2 25 29 38)) t t t "Root\nGroup\nOne\nbody one\nTwo\nbody two\n")"#
        ]],
    )
}

fn magit_section_ident_lookup_and_equality_are_structural() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_section_ident_lookup_and_equality_are_structural",
        r##"(with-temp-buffer
               (magit-section-mode)
               (let ((inhibit-read-only t))
                 (magit-insert-section (root 'repository)
                   (magit-insert-heading "Root")
                   (magit-insert-section (branch "main")
                     (magit-insert-heading "main")
                     (magit-insert-section (commit "abc123")
                       (magit-insert-heading "commit"))))
                 (let* ((root magit-root-section)
                        (branch (car (oref root children)))
                        (commit (car (oref branch children)))
                        (ident (magit-section-ident commit))
                        (found (magit-get-section ident)))
                   (list ident
                         (eq found commit)
                         (magit-section-equal found commit)
                         (magit-section-equal commit branch)
                         (magit-get-section
                          '((missing . "abc123")
                            (branch . "main")
                            (root . repository)))))))"##,
        expect![[
            r#"OK (((commit . "abc123") (branch . "main") (root . repository)) t t nil nil)"#
        ]],
    )
}

fn magit_section_at_and_current_section_follow_text_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_section_at_and_current_section_follow_text_properties",
        r##"(with-temp-buffer
               (magit-section-mode)
               (let ((inhibit-read-only t))
                 (magit-insert-section (root nil)
                   (magit-insert-heading "Root")
                   (magit-insert-section (item 'first)
                     (magit-insert-heading "First")
                     (insert "alpha\n"))
                   (magit-insert-section (item 'second)
                     (magit-insert-heading "Second")
                     (insert "beta\n")))
                 (goto-char (point-min))
                 (let ((at-root (magit-section-at))
                       (current-root (magit-current-section)))
                   (search-forward "alpha")
                   (let ((at-first (magit-section-at))
                         (current-first (magit-current-section)))
                     (goto-char (point-max))
                     (list at-root
                           (oref current-root type)
                           (list (oref at-first type)
                                 (oref at-first value))
                           (eq at-first current-first)
                           (eq (magit-current-section)
                               magit-root-section))))))"##,
        expect![[r#"OK (nil root (item first) t t)"#]],
    )
}

fn magit_section_siblings_parent_values_and_depth_first_mapping_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_section_siblings_parent_values_and_depth_first_mapping_are_exact",
        r##"(with-temp-buffer
               (magit-section-mode)
               (let ((inhibit-read-only t))
                 (magit-insert-section (root 'repo)
                   (magit-insert-heading "Root")
                   (magit-insert-section (item 'a)
                     (magit-insert-heading "A"))
                   (magit-insert-section (item 'b)
                     (magit-insert-heading "B")
                     (magit-insert-section (leaf 'inside)
                       (magit-insert-heading "Inside")))
                   (magit-insert-section (item 'c)
                     (magit-insert-heading "C")))
                 (let* ((root magit-root-section)
                        (children (oref root children))
                        (middle (cadr children))
                        order)
                   (magit-map-sections
                    (lambda (section)
                      (push (list (oref section type)
                                  (oref section value))
                            order)))
                   (list
                    (magit-section-parent-value middle)
                    (mapcar (lambda (section) (oref section value))
                            (magit-section-siblings middle 'prev))
                    (mapcar (lambda (section) (oref section value))
                            (magit-section-siblings middle 'next))
                    (mapcar (lambda (section) (oref section value))
                            (magit-section-siblings middle))
                    (nreverse order)))))"##,
        expect![[
            r#"OK (repo (a) (c) (a c) ((item a) (leaf inside) (item b) (item c) (root repo)))"#
        ]],
    )
}

fn magit_section_text_property_runs_cover_root_headings_bodies_and_end_boundary() -> ParityBatchCase
{
    ParityBatchCase::value(
        "magit_section_text_property_runs_cover_root_headings_bodies_and_end_boundary",
        r##"(with-temp-buffer
               (magit-section-mode)
               (let ((inhibit-read-only t))
                 (magit-insert-section (root nil)
                   (magit-insert-heading "Root")
                   (magit-insert-section (item 'one)
                     (magit-insert-heading "One")
                     (insert "body\n"))
                   (magit-insert-section (item 'two)
                     (magit-insert-heading "Two")))
                 (let* ((root magit-root-section)
                        (one (car (oref root children)))
                        (two (cadr (oref root children)))
                        (position (point-min))
                        runs)
                   (while (< position (point-max))
                     (let* ((section
                             (get-text-property
                              position 'magit-section))
                            (next
                             (next-single-property-change
                              position 'magit-section
                              nil (point-max))))
                       (push
                        (list
                         position
                         next
                         (and section
                              (oref section type))
                         (and section
                              (oref section value))
                         (cond
                          ((null section) 'no-section)
                          ((eq section root) 'root-object)
                          ((eq section one) 'one-object)
                          ((eq section two) 'two-object)
                          (t 'unexpected-object)))
                        runs)
                       (setq position next)))
                   (list
                    (buffer-substring-no-properties
                     (point-min) (point-max))
                    (nreverse runs)
                    (eq
                     (get-text-property
                      (oref root start) 'magit-section)
                     root)
                    (eq
                     (get-text-property
                      (oref one content) 'magit-section)
                     one)
                    (eq
                     (get-text-property
                      (1- (oref two end)) 'magit-section)
                     two)
                    (get-text-property
                     (point-max) 'magit-section)))))"##,
        expect![[
            r#"OK ("Root\nOne\nbody\nTwo\n" ((1 6 nil nil no-section) (6 15 item one one-object) (15 19 item two two-object)) nil t t nil)"#
        ]],
    )
}

pub(super) fn hierarchy_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        magit_section_builds_exact_parent_child_tree_and_ranges(),
        magit_section_ident_lookup_and_equality_are_structural(),
        magit_section_at_and_current_section_follow_text_properties(),
        magit_section_siblings_parent_values_and_depth_first_mapping_are_exact(),
        magit_section_text_property_runs_cover_root_headings_bodies_and_end_boundary(),
    ]
}
