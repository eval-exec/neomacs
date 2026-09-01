use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_publish_sitemap_recursive_sorting_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"alpha.html\" \"index.html\" \"notes/beta.html\" \"notes/gamma.html\") \"#+TITLE: Site Map\\n\\n- [[file:alpha.org][Alpha]]\\n- notes\\n  - [[file:notes/beta.org][Beta]]\\n  - [[file:notes/gamma.org][Gamma]]\" ((\"alpha.html\" t t) (\"notes/beta.html\" t t) (\"notes/gamma.html\" t t) (\"draft.html\" nil nil) (\"index.html\" t t)))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-publish)
  (let* ((root (make-temp-file "org-publish-site" t))
         (pub (make-temp-file "org-publish-out" t))
         (sub (expand-file-name "notes" root))
         (org-publish-use-timestamps-flag nil)
         (org-publish-project-alist
          `(("site"
             :base-directory ,root
             :publishing-directory ,pub
             :recursive t
             :exclude "draft"
             :auto-sitemap t
             :sitemap-filename "index.org"
             :sitemap-title "Site Map"
             :sitemap-style tree
             :sitemap-sort-files anti-chronologically
             :sitemap-sort-folders first
             :publishing-function org-html-publish-to-html
             :with-toc nil))))
    (unwind-protect
        (progn
          (make-directory sub)
          (with-temp-file (expand-file-name "alpha.org" root)
            (insert "#+TITLE: Alpha\n#+DATE: <2026-05-20 Wed>\n* A\n"))
          (with-temp-file (expand-file-name "draft.org" root)
            (insert "#+TITLE: Draft\n#+DATE: <2026-05-30 Sat>\n* D\n"))
          (with-temp-file (expand-file-name "beta.org" sub)
            (insert "#+TITLE: Beta\n#+DATE: <2026-05-25 Mon>\n* B\n"))
          (with-temp-file (expand-file-name "gamma.org" sub)
            (insert "#+TITLE: Gamma\n#+DATE: <2026-05-22 Fri>\n* G\n"))
          (org-publish-project "site" t)
          (let ((sitemap (expand-file-name "index.org" root)))
            (list
             (sort (mapcar (lambda (file) (file-relative-name file pub))
                           (directory-files-recursively pub ".*" nil))
                   #'string<)
             (with-temp-buffer
               (insert-file-contents sitemap)
               (buffer-string))
             (mapcar (lambda (name)
                       (let ((file (expand-file-name name pub)))
                         (list name
                               (file-exists-p file)
                               (and (file-exists-p file)
                                    (with-temp-buffer
                                      (insert-file-contents file)
                                      (not (null
                                            (string-match-p
                                             "<title>.*</title>"
                                             (buffer-string)))))))))
                     '("alpha.html" "notes/beta.html" "notes/gamma.html"
                       "draft.html" "index.html")))))
      (delete-directory root t)
      (delete-directory pub t))))"##,
        expect,
    );
}

#[test]
fn org_publish_attachment_include_and_project_lookup_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"assets/logo.txt\" \"manual.dat\") (\"assets\" \"assets\" nil) ((\"logo.txt\" nil nil) (\"manual.dat\" t \"manual\") (\"secret.txt\" nil nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-publish)
  (let* ((root (make-temp-file "org-publish-assets" t))
         (pub (make-temp-file "org-publish-assets-out" t))
         (asset (expand-file-name "assets/logo.txt" root))
         (org-publish-use-timestamps-flag nil)
         (org-publish-project-alist
          `(("assets"
             :base-directory ,root
             :base-extension "txt"
             :publishing-directory ,pub
             :recursive t
             :include ("manual.dat")
             :exclude "secret"
             :publishing-function org-publish-attachment))))
    (unwind-protect
        (progn
          (make-directory (file-name-directory asset) t)
          (with-temp-file asset (insert "logo"))
          (with-temp-file (expand-file-name "secret.txt" root) (insert "secret"))
          (with-temp-file (expand-file-name "manual.dat" root) (insert "manual"))
          (let* ((project (assoc "assets" org-publish-project-alist))
                 (base-files (mapcar (lambda (file)
                                       (file-relative-name file root))
                                     (org-publish-get-base-files project)))
                 (lookups (mapcar
                           (lambda (name)
                             (let ((p (org-publish-get-project-from-filename
                                       (expand-file-name name root))))
                               (and p (car p))))
                           '("assets/logo.txt" "manual.dat" "secret.txt"))))
            (org-publish-project "assets" t)
            (list
             (sort base-files #'string<)
             lookups
             (mapcar (lambda (name)
                       (let ((file (expand-file-name name pub)))
                         (list name
                               (file-exists-p file)
                               (and (file-exists-p file)
                                    (with-temp-buffer
                                      (insert-file-contents file)
                                      (buffer-string))))))
                     '("logo.txt" "manual.dat" "secret.txt")))))
      (delete-directory root t)
      (delete-directory pub t))))"##,
        expect,
    );
}

#[test]
fn org_publish_needed_timestamp_cache_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"‘org-publish-cache-file-needs-publishing’ called, but no cache present\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-publish)
  (let* ((root (make-temp-file "org-publish-cache" t))
         (pub (make-temp-file "org-publish-cache-out" t))
         (src (expand-file-name "page.org" root))
         (org-publish-use-timestamps-flag t)
         (org-publish-timestamp-directory
          (expand-file-name "timestamps" root))
         (org-publish-project-alist
          `(("cache"
             :base-directory ,root
             :publishing-directory ,pub
             :publishing-function org-html-publish-to-html
             :with-toc nil))))
    (unwind-protect
        (progn
          (with-temp-file src
            (insert "#+TITLE: Cache\n* One\n"))
          (let* ((project (assoc "cache" org-publish-project-alist))
                 (before (org-publish-needed-p
                          src pub #'org-html-publish-to-html pub root)))
            (org-publish-project "cache" t)
            (let ((after (org-publish-needed-p
                          src pub #'org-html-publish-to-html pub root)))
              (sleep-for 1)
              (with-temp-file src
                (insert "#+TITLE: Cache\n* Two\n"))
              (let ((changed (org-publish-needed-p
                              src pub #'org-html-publish-to-html pub root))
                    (timestamp
                     (org-publish-timestamp-filename
                      src pub #'org-html-publish-to-html)))
                (list before
                      after
                      changed
                      (file-exists-p timestamp)
                      (file-exists-p (expand-file-name "page.html" pub))
                      (car project))))))
      (delete-directory root t)
      (delete-directory pub t))))"##,
        expect,
    );
}

#[test]
fn org_publish_components_index_file_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"‘org-publish-cache-get’ called, but no cache present\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-publish)
  (let* ((root (make-temp-file "org-publish-components" t))
         (pub (make-temp-file "org-publish-components-out" t))
         (static (expand-file-name "static" root))
         (org-publish-use-timestamps-flag nil)
         (org-publish-project-alist
          `(("org"
             :base-directory ,root
             :base-extension "org"
             :publishing-directory ,pub
             :recursive nil
             :publishing-function org-html-publish-to-html
             :makeindex t
             :with-toc nil)
            ("static"
             :base-directory ,static
             :base-extension "txt"
             :publishing-directory ,pub
             :recursive nil
             :publishing-function org-publish-attachment)
            ("site" :components ("org" "static")))))
    (unwind-protect
        (progn
          (make-directory static)
          (with-temp-file (expand-file-name "page.org" root)
            (insert "#+TITLE: Page\n")
            (insert "* Heading\n")
            (insert "\\index{Alpha} Body with index.\n"))
          (with-temp-file (expand-file-name "second.org" root)
            (insert "#+TITLE: Second\n")
            (insert "* Other\n")
            (insert "\\index{Beta} Other body.\n"))
          (with-temp-file (expand-file-name "asset.txt" static)
            (insert "asset body"))
          (let* ((page (expand-file-name "page.org" root))
                 (project (org-publish-get-project-from-filename page))
                 (single (org-publish-file page project t)))
            (org-publish-project "site" t)
            (org-publish-all t)
            (let* ((files (sort
                           (mapcar (lambda (file)
                                     (file-relative-name file pub))
                                   (directory-files-recursively pub ".*" nil))
                           #'string<))
                   (page-html (expand-file-name "page.html" pub))
                   (index-html (expand-file-name "theindex.html" pub)))
              (list (car project)
                    (file-relative-name single pub)
                    files
                    (mapcar (lambda (name)
                              (let ((file (expand-file-name name pub)))
                                (list name
                                      (file-exists-p file)
                                      (and (file-exists-p file)
                                           (with-temp-buffer
                                             (insert-file-contents file)
                                             (not (null
                                                   (string-match-p
                                                    (if (equal name
                                                               "asset.txt")
                                                        "asset body"
                                                      "<title>")
                                                    (buffer-string)))))))))
                            '("page.html" "second.html" "asset.txt"
                              "theindex.html"))
                    (and (file-exists-p page-html)
                         (with-temp-buffer
                           (insert-file-contents page-html)
                           (not (null
                                 (string-match-p "index" (buffer-string))))))
                    (and (file-exists-p index-html)
                         (with-temp-buffer
                           (insert-file-contents index-html)
                           (buffer-string)))))))
      (delete-directory root t)
      (delete-directory pub t))))"##,
        expect,
    );
}

#[test]
fn org_publish_custom_hooks_cache_sitemap_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"‘org-publish-cache-get’ called, but no cache present\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-publish)
  (let* ((root (make-temp-file "org-publish-custom" t))
         (pub (make-temp-file "org-publish-custom-out" t))
         (org-publish-use-timestamps-flag nil)
         (org-publish-timestamp-directory
          (expand-file-name "timestamps" root))
         (calls nil)
         (publisher
          (lambda (plist filename pub-dir)
            (push (list 'publish
                        (file-relative-name filename root)
                        (file-relative-name pub-dir root)
                        (plist-get plist :custom))
                  calls)
            (let ((out (expand-file-name
                        (concat (file-name-base filename) ".txt")
                        pub-dir)))
              (with-temp-file out
                (insert "PUBLISHED:" (file-name-nondirectory filename)
                        ":" (plist-get plist :custom)))
              out)))
         (formatter
          (lambda (entry style project)
            (format "ENTRY[%s:%s:%s:%s]"
                    entry
                    style
                    (org-publish-find-title entry project)
                    (org-publish-find-date entry project))))
         (sitemap-fn
          (lambda (title list)
            (push (list 'sitemap title list) calls)
            (concat "#+TITLE: " title "\n"
                    (org-list-to-org list))))
         (org-publish-project-alist
          `(("custom"
             :base-directory ,root
             :base-extension "org"
             :publishing-directory ,pub
             :recursive nil
             :custom "value"
             :auto-sitemap t
             :sitemap-title "Custom Map"
             :sitemap-filename "map.org"
             :sitemap-style list
             :sitemap-sort-files alphabetically
             :sitemap-format-entry ,formatter
             :sitemap-function ,sitemap-fn
             :preparation-function
             ,(lambda (plist)
                (push (list 'prepare
                            (file-name-nondirectory
                             (plist-get plist :base-directory)))
                      calls))
             :completion-function
             ,(lambda (plist)
                (push (list 'complete
                            (file-name-nondirectory
                             (plist-get plist :publishing-directory)))
                      calls))
             :publishing-function ,publisher))))
    (unwind-protect
        (progn
          (with-temp-file (expand-file-name "b.org" root)
            (insert "#+TITLE: Bee\n#+DATE: <2026-05-28 Thu>\n* B\n"))
          (with-temp-file (expand-file-name "a.org" root)
            (insert "#+TITLE: Aye\n#+DATE: <2026-05-27 Wed>\n* A\n"))
          (let* ((project (assoc "custom" org-publish-project-alist))
                 (base-files-before
                  (sort (mapcar (lambda (file)
                                  (file-relative-name file root))
                                (org-publish-get-base-files project))
                        #'string<))
                 (expanded (org-publish-expand-projects
                            org-publish-project-alist)))
            (org-publish-cache-set-file-property
             (expand-file-name "a.org" root) :probe "cached")
            (org-publish-project "custom" t)
            (let ((map-file (expand-file-name "map.org" root))
                  (a-out (expand-file-name "a.txt" pub))
                  (b-out (expand-file-name "b.txt" pub)))
              (list base-files-before
                    (mapcar #'car expanded)
                    (org-publish-property :custom project)
                    (org-publish-cache-get-file-property
                     (expand-file-name "a.org" root) :probe)
                    (file-exists-p
                     (org-publish-timestamp-filename
                      (expand-file-name "a.org" root)
                      pub publisher))
                    (nreverse calls)
                    (with-temp-buffer
                      (insert-file-contents map-file)
                      (buffer-substring-no-properties
                       (point-min) (point-max)))
                    (mapcar
                     (lambda (file)
                       (list (file-name-nondirectory file)
                             (file-exists-p file)
                             (and (file-exists-p file)
                                  (with-temp-buffer
                                    (insert-file-contents file)
                                    (buffer-substring-no-properties
                                     (point-min) (point-max))))))
                     (list a-out b-out))))))
      (delete-directory root t)
      (delete-directory pub t))))"##,
        expect,
    );
}

#[test]
fn org_publish_crossref_current_project_cache_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"about.org\" \"index.org\" \"sitemap.org\") \"pages\" (t nil (\"index.org\") nil nil) \"about-target\" (\"about.org\" \"index.org\" \"sitemap.org\") (\"about.html\" \"index.html\" \"sitemap.html\") \"#+TITLE: Crossrefs\\n\\n- [[file:about.org][About]]\\n- [[file:index.org][Index]]\" ((\"index.html\" t (t t nil)) (\"about.html\" t (t t t)) (\"sitemap.html\" t (t t nil))) \"About\" nil)""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-publish)
  (require 'ox-html)
  (let* ((root (make-temp-file "org-publish-crossref" t))
         (pub (make-temp-file "org-publish-crossref-out" t))
         (org-publish-use-timestamps-flag t)
         (org-publish-timestamp-directory
          (expand-file-name "timestamps" root))
         (published nil)
         (org-publish-after-publishing-hook
          (list (lambda (file &rest _)
                  (push (file-relative-name file root) published))))
         (org-publish-project-alist
          `(("pages"
             :base-directory ,root
             :base-extension "org"
             :publishing-directory ,pub
             :recursive nil
             :auto-sitemap t
             :sitemap-filename "sitemap.org"
             :sitemap-title "Crossrefs"
             :publishing-function org-html-publish-to-html
             :html-link-home "index.html"
             :with-toc nil))))
    (unwind-protect
        (progn
          (with-temp-file (expand-file-name "index.org" root)
            (insert "#+TITLE: Index\n")
            (insert "* Home\n")
            (insert ":PROPERTIES:\n:CUSTOM_ID: home\n:END:\n")
            (insert "See [[file:about.org::*About][About]] and [[#home][Home]].\n"))
          (with-temp-file (expand-file-name "about.org" root)
            (insert "#+TITLE: About\n")
            (insert "* About\n")
            (insert ":PROPERTIES:\n:CUSTOM_ID: about-target\n:END:\n")
            (insert "Back [[file:index.org::#home][Home custom]].\n"))
          (let* ((index (expand-file-name "index.org" root))
                 (about (expand-file-name "about.org" root))
                 (project (org-publish-get-project-from-filename index))
                 (base-files
                  (sort (mapcar (lambda (file)
                                  (file-relative-name file root))
                                (org-publish-get-base-files project))
                        #'string<)))
            (with-current-buffer (find-file-noselect index)
              (org-mode)
              (org-publish-current-file t))
            (org-publish-initialize-cache "pages")
            (let ((after-current-file
                   (list (file-exists-p (expand-file-name "index.html" pub))
                         (file-exists-p (expand-file-name "about.html" pub))
                         (sort published #'string<)
                         (org-publish-cache-get-file-property
                          index :title nil t)
                         (org-publish-cache-get-file-property
                          index :crossrefs nil t)))
                  (resolved-about
                   (org-publish-resolve-external-link
                    "#about-target" about t)))
              (setq published nil)
              (with-current-buffer (find-file-noselect about)
                (org-mode)
                (org-publish-current-project t))
              (org-publish-initialize-cache "pages")
              (let* ((files
                      (sort
                       (mapcar (lambda (file)
                                 (file-relative-name file pub))
                               (directory-files-recursively pub ".*" nil))
                       #'string<))
                     (sitemap (expand-file-name "sitemap.org" root))
                     (html-summaries
                      (mapcar
                       (lambda (name)
                         (let ((file (expand-file-name name pub)))
                           (list name
                                 (file-exists-p file)
                                 (and (file-exists-p file)
                                      (with-temp-buffer
                                        (insert-file-contents file)
                                        (mapcar
                                         (lambda (needle)
                                           (not (null
                                                 (string-match-p
                                                  needle
                                                  (buffer-string)))))
                                         '("<title>" "About"
                                           "Home custom")))))))
                       '("index.html" "about.html" "sitemap.html"))))
                (list base-files
                      (car project)
                      after-current-file
                      resolved-about
                      (sort published #'string<)
                      files
                      (and (file-exists-p sitemap)
                           (with-temp-buffer
                             (insert-file-contents sitemap)
                             (buffer-substring-no-properties
                              (point-min) (point-max))))
                      html-summaries
                      (org-publish-cache-get-file-property
                       about :title nil t)
                      (org-publish-cache-get-file-property
                       about :crossrefs nil t))))))
      (when (get-file-buffer (expand-file-name "index.org" root))
        (kill-buffer (get-file-buffer (expand-file-name "index.org" root))))
      (when (get-file-buffer (expand-file-name "about.org" root))
        (kill-buffer (get-file-buffer (expand-file-name "about.org" root))))
      (delete-directory root t)
      (delete-directory pub t))))"##,
        expect,
    );
}
