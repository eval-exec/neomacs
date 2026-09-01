use expect_test::expect;

use super::ParityBatchCase;

fn magit_blame_addition_populates_commit_details_for_a_visited_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "magit_blame_addition_populates_commit_details_for_a_visited_file",
        r##"(let* ((root (make-temp-file "magit-blame-" t))
                    (default-directory (file-name-as-directory root))
                    (file (expand-file-name "tracked.txt" root))
                    (processes-before (process-list))
                    buffer)
               (unwind-protect
                   (progn
                     (magit-git "init" ".")
                     (with-temp-file file
                       (insert "first\nsecond\n"))
                     (magit-git "add" "tracked.txt")
                     (magit-git "commit" "-m" "initial")
                     (setq buffer (find-file-noselect file))
                     (switch-to-buffer buffer)
                     (magit-blame-addition nil)
                     (neomacs-magit-test-wait-for-blame processes-before)
                     (let ((display-text
                            (mapconcat
                             (lambda (overlay)
                               (let ((before
                                      (overlay-get
                                       overlay 'before-string))
                                     (after
                                      (overlay-get
                                       overlay 'after-string)))
                                 (concat
                                  (if (stringp before) before "")
                                  (if (stringp after) after ""))))
                             (overlays-in
                              (point-min) (point-max))
                             "")))
                       (list
                        magit-blame-mode
                        (and
                         (string-match-p
                          "A U Thor" display-text)
                         t)
                        (and
                         (string-match-p
                          "initial" display-text)
                         t)
                        (buffer-string)
                        (not
                         (seq-some
                          #'process-live-p
                          (seq-remove
                           (lambda (process)
                             (memq process processes-before))
                           (process-list)))))))
                 (neomacs-magit-test-settle processes-before)
                 (when (buffer-live-p buffer)
                   (with-current-buffer buffer
                     (when magit-blame-mode
                       (magit-blame-quit)))
                   (kill-buffer buffer))
                 (delete-directory root t)))"##,
        expect![[r#"OK (t t t "first\nsecond\n" t)"#]],
    )
}

fn magit_blame_cycle_style_rewrites_real_blame_details_for_every_visualization() -> ParityBatchCase
{
    let elisp_form = r####"
(let* ((root (make-temp-file "magit-blame-cycle-" t))
       (default-directory (file-name-as-directory root))
       (file (expand-file-name "calculation.sage" root))
       (processes-before (process-list))
       buffer)
  (unwind-protect
      (progn
        (magit-git "init" ".")
        (with-temp-file file
          (insert "ring = PolynomialRing(QQ, 'x')\npolynomial = ring.gen()^2 - 1\nroots = polynomial.roots()\n"))
        (magit-git "add" "calculation.sage")
        (let ((process-environment
               (append
                '("GIT_AUTHOR_DATE=2001-02-03T04:05:06+0000"
                  "GIT_COMMITTER_DATE=2001-02-03T04:05:06+0000")
                process-environment)))
          (magit-git "commit" "-m" "establish polynomial workflow"))
        (with-temp-file file
          (insert "ring = PolynomialRing(QQ, 'x')\npolynomial = ring.gen()^2 - 4\nroots = polynomial.roots()\n"))
        (magit-git "add" "calculation.sage")
        (let ((process-environment
               (append
                '("GIT_AUTHOR_DATE=2002-03-04T05:06:07+0000"
                  "GIT_COMMITTER_DATE=2002-03-04T05:06:07+0000")
                process-environment)))
          (magit-git "commit" "-m" "change polynomial constant"))
        (setq buffer (find-file-noselect file))
        (switch-to-buffer buffer)
        (magit-blame-addition nil)
        (neomacs-magit-test-wait-for-blame processes-before)
        (unless (= (seq-count
                    (lambda (overlay)
                      (overlay-get overlay 'magit-blame-heading))
                   (overlays-in (point-min) (point-max)))
                   3)
          (error "blame process completed without deterministic overlays"))
        (cl-labels
            ((snapshot ()
               (let ((headings
                      (sort
                       (seq-filter
                        (lambda (overlay)
                          (overlay-get overlay 'magit-blame-heading))
                        (overlays-in (point-min) (point-max)))
                       (lambda (left right)
                         (< (overlay-start left) (overlay-start right)))))
                     (highlights
                      (sort
                       (seq-filter
                        (lambda (overlay)
                          (overlay-get overlay 'magit-blame-highlight))
                        (overlays-in (point-min) (point-max)))
                       (lambda (left right)
                         (< (overlay-start left) (overlay-start right))))))
                 (list
                  :style (car magit-blame--style)
                  :margin left-margin-width
                  :headings
                  (mapcar
                   (lambda (overlay)
                     (let ((before (overlay-get overlay 'before-string))
                           (revinfo (overlay-get overlay 'magit-blame-revinfo)))
                       (list
                        :range (list (overlay-start overlay)
                                     (overlay-end overlay))
                        :text (and before (substring-no-properties before))
                        :author (cdr (assoc "author" revinfo))
                        :summary (cdr (assoc "summary" revinfo)))))
                   headings)
                  :highlight-ranges
                  (mapcar
                   (lambda (overlay)
                     (list (overlay-start overlay)
                           (overlay-end overlay)
                           (overlay-get overlay 'font-lock-face)))
                   highlights)))))
          (let ((headings (snapshot)))
            (magit-blame-cycle-style)
            (let ((highlight (snapshot)))
              (magit-blame-cycle-style)
              (let ((lines (snapshot)))
                (magit-blame-cycle-style)
                (list
                 :headings headings
                 :highlight highlight
                 :lines lines
                 :wrapped (snapshot)
                 :buffer (buffer-substring-no-properties
                          (point-min) (point-max))
                 :mode magit-blame-mode))))))
    (neomacs-magit-test-settle processes-before)
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (setq magit-blame-process nil)
        (when magit-blame-mode
          (magit-blame-mode -1)))
      (kill-buffer buffer))
    (delete-directory root t)))
"####;
    let expect = expect![[
        r####"OK (:headings (:style headings :margin 0 :headings ((:range (1 32) :text "A U Thor             2001-02-03 04:05 establish polynomial workflow\n" :author "A U Thor" :summary "establish polynomial workflow") (:range (32 62) :text "A U Thor             2002-03-04 05:06 change polynomial constant\n" :author "A U Thor" :summary "change polynomial constant") (:range (62 89) :text "A U Thor             2001-02-03 04:05 establish polynomial workflow\n" :author "A U Thor" :summary "establish polynomial workflow")) :highlight-ranges ((1 32 nil) (32 62 nil) (62 89 nil))) :highlight (:style highlight :margin 0 :headings ((:range (1 32) :text nil :author "A U Thor" :summary "establish polynomial workflow") (:range (32 62) :text nil :author "A U Thor" :summary "change polynomial constant") (:range (62 89) :text nil :author "A U Thor" :summary "establish polynomial workflow")) :highlight-ranges ((1 32 magit-blame-highlight) (32 62 magit-blame-highlight) (62 89 magit-blame-highlight))) :lines (:style lines :margin 0 :headings ((:range (1 32) :text " \n" :author "A U Thor" :summary "establish polynomial workflow") (:range (32 62) :text " \n" :author "A U Thor" :summary "change polynomial constant") (:range (62 89) :text " \n" :author "A U Thor" :summary "establish polynomial workflow")) :highlight-ranges ((1 32 nil) (32 62 nil) (62 89 nil))) :wrapped (:style headings :margin 0 :headings ((:range (1 32) :text "A U Thor             2001-02-03 04:05 establish polynomial workflow\n" :author "A U Thor" :summary "establish polynomial workflow") (:range (32 62) :text "A U Thor             2002-03-04 05:06 change polynomial constant\n" :author "A U Thor" :summary "change polynomial constant") (:range (62 89) :text "A U Thor             2001-02-03 04:05 establish polynomial workflow\n" :author "A U Thor" :summary "establish polynomial workflow")) :highlight-ranges ((1 32 nil) (32 62 nil) (62 89 nil))) :buffer "ring = PolynomialRing(QQ, 'x')\npolynomial = ring.gen()^2 - 4\nroots = polynomial.roots()\n" :mode t)"####
    ]];
    ParityBatchCase::value(
        "magit_blame_cycle_style_rewrites_real_blame_details_for_every_visualization",
        elisp_form,
        expect,
    )
    .fresh_process()
}

pub(super) fn blame_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        magit_blame_addition_populates_commit_details_for_a_visited_file(),
        magit_blame_cycle_style_rewrites_real_blame_details_for_every_visualization(),
    ]
}
