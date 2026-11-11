//! Oracle parity tests for source-file coding selection during `load`.
//!
//! GNU's `load-with-code-conversion` inserts source into a multibyte buffer,
//! where `find-auto-coding` gives an explicit `coding:` cookie precedence
//! before the Lisp reader sees any forms.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_load_decodes_iso_2022_source_from_first_line_cookie() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"日本語\" (26085 26412 35486))""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let* ((dir (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
         (file (expand-file-name "legacy-source.el" dir))
         (prefix
          (string-as-unibyte
           ";;; legacy-source.el --- fixture -*- coding: iso-2022-7bit; -*-
(setq neovm--legacy-source-value \""))
         (encoded-japanese
          (apply #'unibyte-string
                 '(27 36 66 70 124 75 92 56 108 27 40 66)))
         (suffix (string-as-unibyte "\")
"))
         (source (concat prefix encoded-japanese suffix)))
    (unwind-protect
        (progn
          (let ((coding-system-for-write 'no-conversion))
            (write-region source nil file nil 'silent))
          (makunbound 'neovm--legacy-source-value)
          ;; The oracle harness binds this to UTF-8 while it reads the test
          ;; form.  Ordinary package loading leaves it nil so the source
          ;; cookie can select the coding system.
          (let ((coding-system-for-read nil))
            (list (load file nil t t)
                  neovm--legacy-source-value
                  (append neovm--legacy-source-value nil))))
      (when (file-exists-p file)
        (delete-file file))))"#,
        expect,
    );
}

#[test]
fn oracle_load_auto_detects_iso_2022_source_without_a_valid_cookie() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"こん\" (12371 12435) iso-2022-7bit-unix)""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let* ((dir (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
         (file (expand-file-name "auto-detected-legacy-source.el" dir))
         ;; Like navi2ch-multibbs.el, the first line closes an empty -*- block
         ;; and the second line's coding tag is therefore not a valid cookie.
         (prefix
          (string-as-unibyte
           ";;; auto-detected-legacy-source.el --- fixture -*-
;;; coding: iso-2022-7bit; -*-
(setq neovm--auto-detected-source-value \""))
         (encoded-japanese
          (apply #'unibyte-string
                 '(27 36 66 36 51 36 115 27 40 66)))
         (suffix (string-as-unibyte "\")
"))
         (source (concat prefix encoded-japanese suffix)))
    (unwind-protect
        (progn
          (let ((coding-system-for-write 'no-conversion))
            (write-region source nil file nil 'silent))
          (makunbound 'neovm--auto-detected-source-value)
          (let ((coding-system-for-read nil))
            (list (load file nil t t)
                  neovm--auto-detected-source-value
                  (append neovm--auto-detected-source-value nil)
                  last-coding-system-used)))
      (when (file-exists-p file)
        (delete-file file))
      (makunbound 'neovm--auto-detected-source-value)))"#,
        expect,
    );
}

#[test]
fn oracle_nested_source_load_restores_the_outer_reader() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t (a-start b-start c-start d c-end b-end a-end) (t t t t))""#
    ]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let* ((dir (getenv "NEOVM_ORACLE_TEST_TMPDIR"))
         (files
          '(("neovm-reader-a.el"
             . "(provide 'neovm-reader-a)
(push 'a-start neovm--reader-trace)
(require 'neovm-reader-b)
(push 'a-end neovm--reader-trace)
")
            ("neovm-reader-b.el"
             . "(provide 'neovm-reader-b)
(push 'b-start neovm--reader-trace)
(require 'neovm-reader-c)
(push 'b-end neovm--reader-trace)
")
            ("neovm-reader-c.el"
             . "(provide 'neovm-reader-c)
(push 'c-start neovm--reader-trace)
(require 'neovm-reader-a)
(require 'neovm-reader-d)
(push 'c-end neovm--reader-trace)
")
            ("neovm-reader-d.el"
             . "(provide 'neovm-reader-d)
(push 'd neovm--reader-trace)
")))
         (paths
          (mapcar (lambda (entry)
                    (expand-file-name (car entry) dir))
                  files))
         (load-path (cons dir load-path))
         (load-suffixes '(".el")))
    (unwind-protect
        (progn
          (defvar neovm--reader-trace)
          (setq neovm--reader-trace nil)
          (mapc (lambda (entry)
                  (let ((coding-system-for-write 'utf-8-unix))
                    (write-region
                     (cdr entry) nil
                     (expand-file-name (car entry) dir)
                     nil 'silent)))
                files)
          (let ((coding-system-for-read nil))
            (list
             (load (car paths) nil t t)
             (nreverse neovm--reader-trace)
             (mapcar #'featurep
                     '(neovm-reader-a neovm-reader-b
                       neovm-reader-c neovm-reader-d)))))
      (mapc (lambda (file)
              (when (file-exists-p file)
                (delete-file file)))
            paths)
      (makunbound 'neovm--reader-trace)))"#,
        expect,
    );
}

#[test]
fn oracle_insert_file_contents_auto_coding_source_extent_depends_on_destination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((4100 4100 nil t t) (1028 1028 nil nil nil))""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let* ((file (expand-file-name "coding-extent.txt"
                                          (getenv "NEOVM_ORACLE_TEST_TMPDIR")))
                  (footer "\n;; Local Variables:\n;; coding: utf-8\n;; End:\n")
                  (contents (concat (make-string (- 4100 (length footer)) ?x) footer)))
             (unwind-protect
                 (progn
                   (let ((coding-system-for-write 'utf-8-unix))
                     (write-region contents nil file nil 'silent))
                   (mapcar
                    (lambda (nonempty)
                      (with-temp-buffer
                        (when nonempty (insert "prefix"))
                        (let* ((target (current-buffer))
                               observed
                               (coding-system-for-read nil)
                               (set-auto-coding-function
                                (lambda (_filename size)
                                  (setq observed
                                        (list size (buffer-size)
                                              enable-multibyte-characters
                                              (eq target (current-buffer))
                                              (and (save-excursion
                                                     (goto-char (point-min))
                                                     (search-forward "coding: utf-8" nil t)) t)))
                                  'utf-8)))
                          (insert-file-contents file)
                          observed)))
                    '(nil t)))
               (when (file-exists-p file) (delete-file file))))"#,
        expect,
    );
}

#[test]
fn oracle_info_reads_utf8_footer_in_a_file_just_over_four_kib() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Top\" utf-8-unix t t)""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(progn
             (require 'info)
             (let* ((file (expand-file-name "coding-footer.info"
                                           (getenv "NEOVM_ORACLE_TEST_TMPDIR")))
                    (prefix "\037\nFile: coding-footer.info, Node: Top\n\n标题内容\0\n")
                    (footer "\n\f\nLocal Variables:\ncoding: utf-8\nEnd:\n")
                    (contents (concat prefix
                                      (make-string (- 4100 (string-bytes prefix)
                                                      (length footer)) ?x)
                                      footer)))
               (unwind-protect
                   (progn
                     (let ((coding-system-for-write 'utf-8-unix))
                       (write-region contents nil file nil 'silent))
                     (let ((coding-system-for-read nil))
                       (Info-find-node file "Top"))
                     (list Info-current-node buffer-file-coding-system
                           (and (save-excursion
                                  (goto-char (point-min))
                                  (search-forward "标题内容" nil t)) t)
                           (= (buffer-size) (length contents))))
                 (when (get-buffer "*info*") (kill-buffer "*info*"))
                 (when (file-exists-p file) (delete-file file)))))"#,
        expect,
    );
}

#[test]
fn oracle_nonempty_auto_coding_probe_preserves_physical_file_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1928 1928 nil) (4096 4096 t) (4096 4096 t) (4096 4096 nil) (30 30 nil) (1024 1024 nil) (4096 4096 t) (1024 1024 nil) (1024 1024 nil))""#
    ]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let ((file (expand-file-name "bounded-probe.txt"
                                        (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
             (unwind-protect
                 (mapcar
                  (lambda (case)
                    (let ((contents (make-string (nth 0 case) ?x)))
                      (setq contents (concat (substring contents 0 3500)
                                             "TAIL-MARKER"
                                             (substring contents 3511)))
                      (let ((coding-system-for-write 'utf-8-unix))
                        (write-region contents nil file nil 'silent)))
                    (with-temp-buffer
                      (insert "prefix")
                      (let* (observed
                            (coding-system-for-read nil)
                            (set-auto-coding-function
                             (lambda (_filename size)
                               (setq observed
                                     (list size (buffer-size)
                                           (and (save-excursion
                                                  (goto-char (point-min))
                                                  (search-forward "TAIL-MARKER" nil t)) t)))
                               'utf-8)))
                        (insert-file-contents file nil (nth 1 case) (nth 2 case))
                        observed)))
                  '((5000 0 5000) (20000 0 5000) (20000 500 5500)
                    (20000 0 20000) (20000 50 80)
                    (4096 0 nil) (4096 0 4096) (4096 0 5000)
                    (8192 4096 nil)))
               (when (file-exists-p file) (delete-file file))))"#,
        expect,
    );
}

#[test]
fn oracle_empty_auto_coding_decodes_callback_mutations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((replace 6000 6000 89 120 t 1) (insert 6002 6002 89 90 t 1) (delete 5998 5998 120 120 t 1) (nil 6000 6000 89 120 t 1) (error error 0 t t 1))""#
    ]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let ((file (expand-file-name "mutating-probe.txt" (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
             (unwind-protect
                 (progn
                   (let ((coding-system-for-write 'utf-8-unix))
                     (write-region (make-string 6000 ?x) nil file nil 'silent))
                   (mapcar
                    (lambda (action)
                      (with-temp-buffer
                        (let ((coding-system-for-read nil)
                              (file-coding-system-alist '(("." . utf-8-unix)))
                              (set-auto-coding-function
                               (lambda (_ size)
                                 (goto-char 3501)
                                 (cond ((eq action 'replace) (delete-char 1) (insert "Y"))
                                       ((eq action 'insert) (insert "YZ"))
                                       ((eq action 'delete) (delete-char 2))
                                       ((eq action 'nil) (delete-char 1) (insert "Y"))
                                       ((eq action 'error) (insert "YZ") (error "probe failed")))
                                 (unless (eq action 'nil) 'utf-8-unix))))
                          (condition-case err
                              (let ((result (insert-file-contents file)))
                                (list action (cadr result) (buffer-size)
                                      (char-after 3501) (char-after 3502)
                                      enable-multibyte-characters (point)))
                            (error (list action (car err) (buffer-size)
                                         enable-multibyte-characters buffer-undo-list (point)))))))
                    '(replace insert delete nil error)))
               (when (file-exists-p file) (delete-file file))))"#,
        expect,
    );
}

#[test]
fn oracle_empty_filename_coding_can_change_the_staged_raw_source() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((((0 nil t) 1 1 89 t 1) ((0 nil t) 1 1 89 t 1)) (((6000 nil t) 6001 6001 89 t 1) ((6000 nil t) 6001 6001 89 t 1)))""#
    ]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let ((file (expand-file-name "filename-probe.txt" (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
             (unwind-protect
                 (mapcar
                  (lambda (length)
                    (let ((coding-system-for-write 'utf-8-unix))
                      (write-region (make-string length ?x) nil file nil 'silent))
                    (mapcar
                     (lambda (use-auto)
                       (with-temp-buffer
                         (let* (observed
                                (coding-system-for-read nil)
                                (set-auto-coding-function (and use-auto (lambda (_ size) nil)))
                                (file-coding-system-alist '(("." . neovm-mutating-filename-coding))))
                           (fset 'neovm-mutating-filename-coding
                                 (lambda (_args)
                                   (setq observed (list (buffer-size) enable-multibyte-characters
                                                        (eq buffer-undo-list t)))
                                   (goto-char (point-min)) (insert "Y") 'utf-8-unix))
                           (let ((result (insert-file-contents file)))
                             (list observed (cadr result) (buffer-size) (char-after 1)
                                   enable-multibyte-characters (point))))))
                     '(nil t)))
                  '(0 6000))
               (fmakunbound 'neovm-mutating-filename-coding)
               (when (file-exists-p file) (delete-file file))))"#,
        expect,
    );
}

/// Temporary file exposure and its cleanup preserve the callback's observable
/// modification state, including when the filename callback exits with an error.
#[test]
fn oracle_empty_auto_coding_staging_preserves_modified_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let ((file (expand-file-name "staging-modified.txt"
                                        (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
             (unwind-protect
                 (progn
                   (let ((coding-system-for-write 'no-conversion))
                     (write-region (make-string 6000 ?x) nil file nil 'silent))
                   (mapcar
                    (lambda (initial)
                      (mapcar
                       (lambda (action)
                         (with-temp-buffer
                           (set-buffer-modified-p initial)
                           (let* ((before (list (buffer-modified-tick)
                                                (buffer-chars-modified-tick)))
                                  seen final
                                  (coding-system-for-read nil)
                                  (set-auto-coding-function nil)
                                  (file-coding-system-alist
                                   '(("." . neovm--staging-error-coding))))
                             (fset 'neovm--staging-error-coding
                                   (lambda (_args)
                                     (setq seen
                                           (list (buffer-modified-p)
                                                 (equal before
                                                        (list (buffer-modified-tick)
                                                              (buffer-chars-modified-tick)))))
                                     (cond ((eq action 'mark) (set-buffer-modified-p t))
                                           ((eq action 'clean) (set-buffer-modified-p nil))
                                           ((eq action 'edit) (insert "Y"))
                                           ((eq action 'edit-clean)
                                            (insert "Y") (set-buffer-modified-p nil))
                                           ((eq action 'props)
                                            (put-text-property 1 2 'face 'bold)))
                                     (setq final (list (buffer-modified-tick)
                                                       (buffer-chars-modified-tick)))
                                     (error "coding probe error")))
                             (condition-case nil (insert-file-contents file) (error nil))
                             (list action seen (buffer-size) (buffer-modified-p)
                                   (equal final (list (buffer-modified-tick)
                                                      (buffer-chars-modified-tick)))
                                   enable-multibyte-characters))))
                       '(none mark clean edit edit-clean props)))
                    '(nil t)))
               (fmakunbound 'neovm--staging-error-coding)
               (when (file-exists-p file) (delete-file file))))"#,
        expect_test::expect![[
            r#""OK (((none (nil t) 0 nil t t) (mark (nil t) 0 t t t) (clean (nil t) 0 nil t t) (edit (nil t) 0 t t t) (edit-clean (nil t) 0 nil t t) (props (nil t) 0 t t t)) ((none (t t) 0 t t t) (mark (t t) 0 t t t) (clean (t t) 0 nil t t) (edit (t t) 0 t t t) (edit-clean (t t) 0 nil t t) (props (t t) 0 t t t)))""#
        ]],
    );
}

/// Existing markers and overlays stay put during raw exposure; the final
/// decoded insertion then applies their ordinary insertion behavior.
#[test]
fn oracle_empty_auto_coding_staging_preserves_markers_and_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let ((file (expand-file-name "staging-markers.txt"
                                        (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
             (unwind-protect
                 (progn
                   (let ((coding-system-for-write 'no-conversion))
                     (write-region (make-string 6000 ?x) nil file nil 'silent))
                   (with-temp-buffer
                     (let* ((after-marker (copy-marker (point-min) t))
                            (overlay (make-overlay 1 1 nil t t))
                            seen
                            (coding-system-for-read nil)
                            (set-auto-coding-function
                             (lambda (_name _size)
                               (setq seen (list (marker-position after-marker)
                                                (overlay-start overlay) (overlay-end overlay)
                                                (point) (buffer-size)))
                               'utf-8-unix)))
                       (insert-file-contents file)
                       (list seen (marker-position after-marker)
                             (overlay-start overlay) (overlay-end overlay)))))
               (when (file-exists-p file) (delete-file file))))"#,
        expect_test::expect![[r#""OK ((1 1 1 1 6000) 6001 6001 6001)""#]],
    );
}

/// Raw bytes change the shared physical size, while only the destination's
/// accessible end grows, whether that destination is the base or indirect view.
#[test]
fn oracle_empty_auto_coding_staging_preserves_indirect_views() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let ((file (expand-file-name "staging-indirect.txt"
                                        (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
             (unwind-protect
                 (progn
                   (let ((coding-system-for-write 'no-conversion))
                     (write-region "ABC" nil file nil 'silent))
                   (mapcar
                    (lambda (use-indirect)
                      (let* ((base (generate-new-buffer " *staging-base*"))
                             (view (make-indirect-buffer base " *staging-view*" t))
                             (base-marker (with-current-buffer base (copy-marker 1 t)))
                             (view-marker (with-current-buffer view (copy-marker 1 t)))
                             observed)
                        (unwind-protect
                            (with-current-buffer (if use-indirect view base)
                              (let ((coding-system-for-read nil)
                                    (set-auto-coding-function
                                     (lambda (_name _size)
                                       (setq observed
                                             (list
                                              (with-current-buffer base
                                                (list (point-min) (point-max) (buffer-size)
                                                      (point) (marker-position base-marker)))
                                              (with-current-buffer view
                                                (list (point-min) (point-max) (buffer-size)
                                                      (point) (marker-position view-marker)))))
                                       'utf-8-unix)))
                                (let ((result (insert-file-contents file nil)))
                                  (list use-indirect observed (cadr result)))))
                          (set-marker base-marker nil)
                          (set-marker view-marker nil)
                          (when (buffer-live-p view) (kill-buffer view))
                          (when (buffer-live-p base) (kill-buffer base)))))
                    '(nil t)))
               (when (file-exists-p file) (delete-file file))))"#,
        expect_test::expect![[
            r#""OK ((nil ((1 4 3 1 1) (1 1 3 1 1)) 3) (t ((1 1 3 1 1) (1 4 3 1 1)) 3))""#
        ]],
    );
}

/// Fresh and previously swapped destinations can collide with a donor's
/// character tick; the decoder must consume the callback's resulting owner.
#[test]
fn oracle_empty_auto_coding_staging_honors_historical_text_swap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r#"(let ((file (expand-file-name "staging-swap.txt"
                                        (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
             (unwind-protect
                 (progn
                   (let ((coding-system-for-write 'no-conversion))
                     (write-region "A" nil file nil 'silent))
                   (mapcar
                    (lambda (history)
                      (let ((donor (generate-new-buffer " *staging-swap-donor*"))
                            observed)
                        (unwind-protect
                            (progn
                              (with-current-buffer donor (set-buffer-multibyte nil))
                              (with-temp-buffer
                                (when history
                                  (let ((empty (generate-new-buffer " *staging-swap-history*")))
                                    (unwind-protect (buffer-swap-text empty)
                                      (kill-buffer empty))))
                                (set-buffer-modified-p nil)
                                (let ((coding-system-for-read nil)
                                      (set-auto-coding-function
                                       (lambda (_name _size)
                                         (setq observed
                                               (list (buffer-chars-modified-tick)
                                                     (with-current-buffer donor
                                                       (buffer-chars-modified-tick))))
                                         (buffer-swap-text donor)
                                         (setq observed
                                               (append observed
                                                       (list (buffer-chars-modified-tick)
                                                             (buffer-size))))
                                         'utf-8-unix)))
                                  (let ((result (insert-file-contents file)))
                                    (list history observed (cadr result) (buffer-string)
                                          (with-current-buffer donor (buffer-string)))))))
                          (when (buffer-live-p donor) (kill-buffer donor)))))
                    '(nil t)))
               (when (file-exists-p file) (delete-file file))))"#,
        expect_test::expect![[r#""OK ((nil (1 1 2 0) 0 \"\" \"A\") (t (2 1 2 0) 0 \"\" \"A\"))""#]],
    );
}
