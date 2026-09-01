use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_yank_image_attach_and_directory_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* Task                                                               :ATTACH:\\n:PROPERTIES:\\n:ID: yank-image-id\\n:END:\\n[[attachment:fixed-image.png][fixed-image.png]]\" \"data/ya/nk-image-id\" t \"PNGDATA\" \"* Task                                                               :ATTACH:\\n:PROPERTIES:\\n:ID: yank-image-id\\n:END:\\n[[attachment:fixed-image.png][fixed-image.png]]\\n[[file:images/fixed-image.jpeg]]\" t \"JPEGDATA\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (let* ((root (make-temp-file "org-yank-image" t))
         (org-file (expand-file-name "notes.org" root))
         (image-dir (expand-file-name "images" root))
         (org-attach-id-dir (expand-file-name "data" root))
         (org-attach-store-link-p nil)
         (org-yank-image-file-name-function
          (lambda () "fixed-image")))
    (unwind-protect
        (progn
          (with-temp-file org-file
            (insert "* Task\n:PROPERTIES:\n:ID: yank-image-id\n:END:\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-max))
            (let ((org-yank-image-save-method 'attach))
              (org--image-yank-media-handler "image/png" "PNGDATA"))
            (let* ((attach-buffer
                    (buffer-substring-no-properties (point-min) (point-max)))
                   (attach-dir (org-attach-dir))
                   (attach-file (expand-file-name "fixed-image.png" attach-dir))
                   (attach-data (with-temp-buffer
                                  (insert-file-contents-literally attach-file)
                                  (buffer-string))))
              (goto-char (point-max))
              (insert "\n")
              (let ((org-yank-image-save-method image-dir))
                (org--image-yank-media-handler "image/jpeg" "JPEGDATA"))
              (let* ((dir-buffer
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>"
                       (buffer-substring-no-properties
                        (point-min) (point-max))))
                     (dir-file
                      (car (directory-files image-dir 'absolute
                                            "\\`fixed-image\\.")))
                     (dir-data (with-temp-buffer
                                 (insert-file-contents-literally dir-file)
                                 (buffer-string))))
                (list (replace-regexp-in-string
                       (regexp-quote root) "<root>" attach-buffer)
                      (file-relative-name attach-dir root)
                      (file-exists-p attach-file)
                      attach-data
                      dir-buffer
                      (file-exists-p dir-file)
                      dir-data)))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_copied_files_dnd_file_link_and_attach_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"* Task\\n:PROPERTIES:\\n:ID: dnd-id\\n:END:\\n[[<root>/a.txt]] [[<root>/b.txt]] \" \"* Task                                                               :ATTACH:\\n:PROPERTIES:\\n:ID: dnd-id\\n:END:\\n[[<root>/a.txt]] [[<root>/b.txt]] \\n[[attachment:a.txt]]\" \"data/dn/d-id\" (\"a.txt\") \"A\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (require 'dnd)
  (let* ((root (make-temp-file "org-dnd" t))
         (org-file (expand-file-name "notes.org" root))
         (a (expand-file-name "a.txt" root))
         (b (expand-file-name "b.txt" root))
         (org-attach-id-dir (expand-file-name "data" root))
         (org-attach-store-link-p nil)
         (org-attach-method 'cp))
    (unwind-protect
        (progn
          (with-temp-file a (insert "A\n"))
          (with-temp-file b (insert "B\n"))
          (with-temp-file org-file
            (insert "* Task\n:PROPERTIES:\n:ID: dnd-id\n:END:\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-max))
            (let ((org-yank-dnd-method 'file-link))
              (org--dnd-multi-local-file-handler
               (list (concat "file://" a) (concat "file://" b))
               'copy))
            (let ((after-links
                   (replace-regexp-in-string
                    (regexp-quote root) "<root>"
                    (buffer-substring-no-properties
                     (point-min) (point-max)))))
              (goto-char (point-max))
              (insert "\n")
              (let ((org-yank-dnd-method 'attach))
                (org--dnd-local-file-handler (concat "file://" a) 'copy ""))
              (let* ((dir (org-attach-dir))
                     (files (sort (org-attach-file-list dir) #'string<))
                     (attached-data
                      (with-temp-buffer
                        (insert-file-contents-literally
                         (expand-file-name "a.txt" dir))
                        (buffer-string))))
                (list after-links
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>"
                       (buffer-substring-no-properties
                        (point-min) (point-max)))
                      (file-relative-name dir root)
                      files
                      attached-data)))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_xds_direct_save_attach_and_file_link_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (let* ((root (make-temp-file "org-xds" t))
         (org-file (expand-file-name "notes.org" root))
         (target (expand-file-name "linked.bin" root))
         (org-attach-id-dir (expand-file-name "data" root))
         (org-attach-store-link-p nil))
    (unwind-protect
        (progn
          (with-temp-file org-file
            (insert "* Task\n:PROPERTIES:\n:ID: xds-id\n:END:\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-max))
            (let ((org-yank-dnd-method 'attach))
              (let ((attach-name (org--dnd-xds-function t "drop.txt")))
                (with-temp-file attach-name (insert "DROP\n"))
                (org--dnd-xds-function nil attach-name)
                (let ((after-attach
                       (buffer-substring-no-properties
                        (point-min) (point-max)))
                      (attach-exists (file-exists-p attach-name)))
                  (goto-char (point-max))
                  (insert "\n")
                  (let ((org-yank-dnd-method 'file-link))
                    (cl-letf (((symbol-function 'read-file-name)
                               (lambda (&rest _) target)))
                      (let ((link-name (org--dnd-xds-function t "linked.bin")))
                        (with-temp-file link-name (insert "LINK\n"))
                        (org--dnd-xds-function nil link-name)
                        (list (file-relative-name attach-name root)
                              attach-exists
                              (replace-regexp-in-string
                               (regexp-quote root) "<root>"
                               after-attach)
                              (file-relative-name link-name root)
                              (file-exists-p link-name)
                              (replace-regexp-in-string
                               (regexp-quote root) "<root>"
                               (buffer-substring-no-properties
                                (point-min) (point-max)))))))))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_yank_adjusted_folded_subtree_visibility_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-cycle)
  (require 'org-fold)
  (with-temp-buffer
    (let ((kill-ring nil)
          (kill-ring-yank-pointer nil)
          (org-yank-adjusted-subtrees t)
          (org-yank-folded-subtrees t))
      (org-mode)
      (insert "* Target\n")
      (insert "** Existing\nExisting body\n")
      (insert "* Source\n")
      (insert "** TODO Child\n")
      (insert "Child body\n")
      (insert "*** Grand\nGrand body\n")
      (goto-char (point-min))
      (search-forward "Child")
      (beginning-of-line)
      (org-copy-subtree 1)
      (let ((copied (current-kill 0 t)))
        (goto-char (point-min))
        (search-forward "Existing body")
        (end-of-line)
        (insert "\n")
        (org-yank nil)
        (let ((after-yank
               (buffer-substring-no-properties (point-min) (point-max)))
              (visibility
               (mapcar
                (lambda (needle)
                  (save-excursion
                    (goto-char (point-min))
                    (search-forward needle)
                    (list needle
                          (org-outline-level)
                          (invisible-p (point))
                          (get-text-property (point) 'invisible))))
                '("Target" "Existing" "Child body" "Grand" "Grand body"
                  "Source")))
              (swallow
               (save-excursion
                 (goto-char (point-min))
                 (search-forward "Existing body")
                 (let ((beg (line-beginning-position)))
                   (search-forward "Grand body")
                   (org-yank-folding-would-swallow-text beg (point))))))
          (org-fold-show-all)
          (list copied
                after-yank
                visibility
                swallow
                (org-element-map (org-element-parse-buffer) 'headline
                  (lambda (h)
                    (list (org-element-property :level h)
                          (org-element-property :todo-keyword h)
                          (org-element-property :raw-value h))))
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_dnd_ask_private_image_xds_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"What to do with file?\" ((97 \"attach\" attach) (111 \"open\" open) (102 \"insert file: link\" file-link)) file-link) (\"What to do with dropped file?\" ((97 \"attach\" attach) (111 \"open\" open) (102 \"insert file: link\" file-link)) attach) (\"What to do with dropped file?\" ((97 \"attach\" attach) (111 \"open\" open) (102 \"insert file: link\" file-link)) attach)) nil (\"File `file://<root>/plain.txt' is not readable, skipping\" \"File `file://<root>/missing.txt' is not readable, skipping\" \"File \\\"plain.txt\\\" is now an attachment\") \"* Task                                                               :ATTACH:\\n:PROPERTIES:\\n:ID: ask-dnd-id\\n:END:\\n\\n[[<root>/plain.txt]]|\\n[[file:images/image.png]]\\n[[attachment:plain.txt]]\\n[[attachment:xds.txt]]\\n[[attachment:open.txt]]\" \"data/as/k-dnd-id\" (\"open.txt\" \"plain.txt\" \"xds.txt\") (\"image.png\") nil \"PNG\\n\" \"PLAIN\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-attach)
  (require 'dnd)
  (let* ((root (make-temp-file "org-dnd-ask" t))
         (org-file (expand-file-name "notes.org" root))
         (plain (expand-file-name "plain.txt" root))
         (image (expand-file-name "image.png" root))
         (missing (expand-file-name "missing.txt" root))
         (image-dir (expand-file-name "images" root))
         (org-attach-id-dir (expand-file-name "data" root))
         (org-attach-store-link-p nil)
         (org-attach-method 'cp)
         (org-yank-dnd-default-attach-method 'cp)
         (answers '(file-link attach attach file-link open))
         choice-log open-log messages)
    (unwind-protect
        (progn
          (with-temp-file plain (insert "PLAIN\n"))
          (with-temp-file image (insert "PNG\n"))
          (with-temp-file org-file
            (insert "* Task\n:PROPERTIES:\n:ID: ask-dnd-id\n:END:\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (goto-char (point-max))
            (cl-letf (((symbol-function 'org--dnd-rmc)
                       (lambda (prompt choices)
                         (let ((answer (pop answers)))
                           (push (list prompt
                                       (mapcar (lambda (choice)
                                                 (list (nth 0 choice)
                                                       (nth 1 choice)
                                                       (nth 2 choice)))
                                               choices)
                                       answer)
                                 choice-log)
                           answer)))
                      ((symbol-function 'dnd-open-local-file)
                       (lambda (uri action)
                         (push (list uri action) open-log)
                         'opened))
                      ((symbol-function 'message)
                       (lambda (fmt &rest args)
                         (push (apply #'format fmt args) messages))))
              (let ((org-yank-dnd-method 'ask))
                (org--copied-files-yank-media-handler
                 "x/special-gnome-files"
                 (concat "copy\nfile://" plain "\nfile://" missing "\0"))
                (insert "\n")
                (org--dnd-local-file-handler (concat "file://" plain)
                                             'copy "|")
                (insert "\n"))
              (let ((org-yank-dnd-method 'attach)
                    (org-yank-image-save-method image-dir))
                (org--dnd-local-file-handler (concat "file://" image)
                                             'copy "\n"))
              (let ((org-yank-dnd-method 'attach))
                (org--dnd-local-file-handler (concat "file://" plain)
                                             'private "\n"))
              (let ((org-yank-dnd-method 'ask))
                (let ((xds-name (org--dnd-xds-function t "xds.txt")))
                  (with-temp-file xds-name (insert "XDS\n"))
                  (org--dnd-xds-function nil xds-name))
                (insert "\n")
                (let ((open-name (org--dnd-xds-function t "open.txt")))
                  (with-temp-file open-name (insert "OPEN\n"))
                  (org--dnd-xds-function nil open-name)))
              (let* ((attach-dir (org-attach-dir))
                     (attach-files
                      (and (file-directory-p attach-dir)
                           (sort (org-attach-file-list attach-dir)
                                 #'string<)))
                     (image-files
                      (and (file-directory-p image-dir)
                           (sort (directory-files image-dir nil
                                                  "\\`image\\.png\\'")
                                 #'string<))))
                (list (nreverse choice-log)
                      (nreverse open-log)
                      (mapcar (lambda (message)
                                (replace-regexp-in-string
                                 (regexp-quote root) "<root>" message))
                              (nreverse messages))
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>"
                       (buffer-substring-no-properties
                        (point-min) (point-max)))
                      (file-relative-name attach-dir root)
                      attach-files
                      image-files
                      org--dnd-xds-method
                      (and image-files
                           (with-temp-buffer
                             (insert-file-contents-literally
                              (expand-file-name (car image-files)
                                                image-dir))
                             (buffer-string)))
                      (and (member "plain.txt" attach-files)
                           (with-temp-buffer
                             (insert-file-contents-literally
                              (expand-file-name "plain.txt" attach-dir))
                             (buffer-string))))))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}
