use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_persist_grouped_elisp_version_load_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil (nil \"v1\" \"literal\" :keyword) nil (:reset t) ((elisp org-persist-test-var) (version \"v1\") (elisp-data \"literal\") (elisp-data :keyword) (elisp org-persist-test-var) (version \"v1\") (elisp-data \"literal\") (elisp-data :keyword) (elisp org-persist-test-var) (version \"v1\") (elisp-data \"literal\") (elisp-data :keyword) (elisp org-persist-test-var) (version \"v1\") (elisp-data \"literal\") (elisp-data :keyword)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-persist)
  (let* ((root (make-temp-file "org-persist" t))
         (org-persist-directory (expand-file-name "cache/" root))
         (org-persist--index nil)
         (org-persist--index-hash nil)
         (org-persist--index-age nil)
         (org-persist--write-cache (make-hash-table :test #'equal))
         (org-persist-before-write-hook nil)
         (org-persist-before-read-hook nil)
         (org-persist-after-read-hook nil)
         (org-persist-default-expiry 'never)
         (org-persist-test-var '(:old nil))
         (read-events nil))
    (unwind-protect
        (progn
          (add-hook 'org-persist-after-read-hook
                    (lambda (container associated)
                      (push (list container associated) read-events)))
          (setq org-persist-test-var '(:value (1 2 3) :label "alpha"))
          (org-persist-register
           '((elisp org-persist-test-var)
             (version "v1")
             "literal"
             :keyword)
           '(:key "suite")
           :write-immediately t)
          (let ((read-one (org-persist-read
                           'org-persist-test-var '(:key "suite")))
                (read-related (org-persist-read
                               '(version "v1") '(:key "suite")
                               nil nil :read-related t)))
            (setq org-persist-test-var '(:reset t))
            (let ((loaded (org-persist-load
                           'org-persist-test-var '(:key "suite"))))
              (org-persist-unregister
               '(version "v1") '(:key "suite") :remove-related t)
              (list read-one
                    read-related
                    loaded
                    org-persist-test-var
                    (mapcar #'car (reverse read-events))
                    (org-persist-read
                     'org-persist-test-var '(:key "suite"))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_persist_buffer_local_hash_match_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((:buffer-value \"original\" :items (a b)) nil (:buffer-value \"original\" :items (a b)) (:buffer-value \"original\" :items (a b)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-persist)
  (defvar org-persist-test-buffer-var nil)
  (let* ((root (make-temp-file "org-persist-buffer" t))
         (file (expand-file-name "note.org" root))
         (org-persist-directory (expand-file-name "cache/" root))
         (org-persist--index nil)
         (org-persist--index-hash nil)
         (org-persist--index-age nil)
         (org-persist--write-cache (make-hash-table :test #'equal))
         (org-persist-default-expiry 'never))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "* A\nBody\n"))
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (setq-local org-persist-test-buffer-var
                        '(:buffer-value "original" :items (a b)))
            (org-persist-register
             'org-persist-test-buffer-var (current-buffer)
             :write-immediately t)
            (let ((same-hash (org-persist-read
                              'org-persist-test-buffer-var
                              (current-buffer) t)))
              (goto-char (point-max))
              (insert "Changed in memory.\n")
              (let ((mismatch-hash (org-persist-read
                                    'org-persist-test-buffer-var
                                    (current-buffer) t))
                    (ignore-hash (org-persist-read
                                  'org-persist-test-buffer-var
                                  (current-buffer) nil)))
                (setq-local org-persist-test-buffer-var '(:reset t))
                (org-persist-load
                 'org-persist-test-buffer-var
                 (list :file file))
                (list same-hash
                      mismatch-hash
                      ignore-hash
                      org-persist-test-buffer-var
                      (not (null kill-buffer-hook)))))))
      (when (get-file-buffer file) (kill-buffer (get-file-buffer file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_persist_file_container_gc_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t \"one\\ntwo\\n\" (\"<persist-file>\" \"file-v1\" \"payload\") t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-persist)
  (let* ((root (make-temp-file "org-persist-file" t))
         (source (expand-file-name "source.txt" root))
         (org-persist-directory (expand-file-name "cache/" root))
         (org-persist--index nil)
         (org-persist--index-hash nil)
         (org-persist--index-age nil)
         (org-persist--write-cache (make-hash-table :test #'equal))
         (org-persist-default-expiry 'never))
    (unwind-protect
        (progn
          (with-temp-file source
            (insert "one\ntwo\n"))
          (org-persist-register
           '((file) (version "file-v1") "payload")
           source
           :write-immediately t)
          (let* ((stored (org-persist-read '(file) source))
                 (stored-exists (and stored (file-exists-p stored)))
                 (stored-text (and stored
                                   (with-temp-buffer
                                     (insert-file-contents stored)
                                     (buffer-string))))
                 (related (org-persist-read
                           '(version "file-v1") source
                           nil nil :read-related t)))
            (org-persist-unregister
             '(file) source :remove-related t)
            (list stored-exists
                  stored-text
                  (mapcar (lambda (x)
                            (if (and (stringp x)
                                     (file-name-absolute-p x))
                                "<persist-file>"
                              x))
                          related)
                  (and stored (file-exists-p stored))
                  (org-persist-read '(file) source))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_persist_shared_hooks_loadall_gc_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((:left (shared-node)) (:right (shared-node)) \"shared-v1\") t \"shared-v1\" nil nil (:load \"reset\") 5 4 3 2 nil nil ((before-write (elisp org-persist-test-a)) (before-write (elisp org-persist-test-b)) (before-write (version \"shared-v1\")) (before-read (elisp org-persist-test-a)) (before-read (elisp org-persist-test-b)) (before-read (version \"shared-v1\")) (after-read (elisp org-persist-test-a)) (after-read (elisp org-persist-test-b)) (after-read (version \"shared-v1\")) (before-write (elisp org-persist-test-load)) (before-read (elisp org-persist-test-load)) (after-read (elisp org-persist-test-load)) (before-write (elisp org-persist-test-blocked)) (before-write (elisp-data \"expired\")) (before-write (elisp-data (:gone t))) (before-read (elisp org-persist-test-a)) (before-read (elisp org-persist-test-b)) (before-read (version \"shared-v1\")) (after-read (elisp org-persist-test-a)) (after-read (elisp org-persist-test-b)) (after-read (version \"shared-v1\")) (before-write (elisp org-persist-test-blocked))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-persist)
  (defvar org-persist-test-a nil)
  (defvar org-persist-test-b nil)
  (defvar org-persist-test-load nil)
  (defvar org-persist-test-blocked nil)
  (let* ((root (make-temp-file "org-persist-shared" t))
         (org-persist-directory (expand-file-name "cache/" root))
         (org-persist--index nil)
         (org-persist--index-hash nil)
         (org-persist--index-age nil)
         (org-persist--write-cache (make-hash-table :test #'equal))
         (org-persist-before-write-hook nil)
         (org-persist-before-read-hook nil)
         (org-persist-after-read-hook nil)
         (org-persist-default-expiry 'never)
         (events nil))
    (unwind-protect
        (progn
          (add-hook 'org-persist-before-write-hook
                    (lambda (container associated)
                      (push (list 'before-write container associated) events)
                      (equal container '(elisp org-persist-test-blocked))))
          (add-hook 'org-persist-before-read-hook
                    (lambda (container associated)
                      (push (list 'before-read container associated) events)
                      nil))
          (add-hook 'org-persist-after-read-hook
                    (lambda (container associated)
                      (push (list 'after-read container associated) events)))
          (let ((shared (list 'shared-node)))
            (setq org-persist-test-a (list :left shared)
                  org-persist-test-b (list :right shared)
                  org-persist-test-load '(:load "initial")
                  org-persist-test-blocked '(:blocked t)))
          (org-persist-register
           '((elisp org-persist-test-a)
             (elisp org-persist-test-b)
             (version "shared-v1"))
           '(:key "shared")
           :write-immediately t)
          (org-persist-register
           'org-persist-test-load
           '(:key "load-all")
           :write-immediately t)
          (let ((blocked-write
                 (org-persist-register
                  'org-persist-test-blocked
                  '(:key "blocked")
                  :write-immediately t)))
            (org-persist-register
             '("expired" (elisp-data (:gone t)))
             '(:key "expired")
             :expiry 0
             :write-immediately t)
            (let* ((related
                    (org-persist-read
                     '(version "shared-v1")
                     '(:key "shared")
                     nil nil :read-related t))
                   (read-a (nth 0 related))
                   (read-b (nth 1 related))
                   (read-version (nth 2 related))
                   (shared-eq (eq (cadr read-a) (cadr read-b)))
                   (blocked-read
                    (org-persist-read
                     'org-persist-test-blocked '(:key "blocked")))
                   (index-before-gc (length org-persist--index))
                   (files-before-gc
                    (and (file-exists-p org-persist-directory)
                         (length
                          (directory-files-recursively
                           org-persist-directory ".+" nil)))))
              (setq org-persist-test-load '(:load "reset"))
              (org-persist-load-all '(:key "load-all"))
              (org-persist-gc)
              (let ((expired-after
                     (org-persist-read
                      "expired" '(:key "expired")
                      nil nil :read-related t))
                    (index-after-gc (length org-persist--index))
                    (files-after-gc
                     (and (file-exists-p org-persist-directory)
                          (length
                           (directory-files-recursively
                            org-persist-directory ".+" nil)))))
                (org-persist-unregister
                 '(version "shared-v1") 'all :remove-related t)
                (list related
                      shared-eq
                      read-version
                      blocked-write
                      blocked-read
                      org-persist-test-load
                      index-before-gc
                      index-after-gc
                      files-before-gc
                      files-after-gc
                      expired-after
                      (org-persist-read
                       '(version "shared-v1") '(:key "shared")
                       nil nil :read-related t)
                       (mapcar
                        (lambda (event)
                          (list (car event) (cadr event)))
                        (reverse events)))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_persist_write_read_gc_unregister_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 3) 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-persist)
  (let* ((root (make-temp-file "org-persist-lc" t))
         (org-persist-directory (expand-file-name "persist-data" root))
         (org-persist--index nil)
         (events nil))
    (unwind-protect
        (progn
          ;; Write entries
          (org-persist-write :version '(version "lc-v1")
                             :value '(:data "hello" :num 42)
                             :key "lc-key")
          (org-persist-write :version '(version "lc-v2")
                             :value '(:data "world" :num 99)
                             :key "lc-key-2")
          (let ((index-after-write (length org-persist--index)))
            ;; Read back
            (let ((read-v1 (org-persist-read '(version "lc-v1")
                                             '(:key "lc-key")))
                  (read-v2 (org-persist-read '(version "lc-v2")
                                             '(:key "lc-key-2")))
                  (read-missing (org-persist-read '(version "no-such")
                                                  '(:key "missing"))))
              ;; GC
              (org-persist-gc)
              (let ((index-after-gc (length org-persist--index)))
                ;; Unregister
                (org-persist-unregister '(version "lc-v1") 'all)
                (let ((index-after-unreg (length org-persist--index))
                      (read-after-unreg (org-persist-read
                                         '(version "lc-v1")
                                         '(:key "lc-key"))))
                  (list index-after-write
                        read-v1
                        read-v2
                        read-missing
                        index-after-gc
                        index-after-unreg
                         read-after-unreg
                         (file-exists-p org-persist-directory)))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_persist_write_read_hash_table_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 3) 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-persist)
  (let* ((root (make-temp-file "org-persist-hash" t))
         (org-persist-directory (expand-file-name "persist-data" root))
         (org-persist--index nil))
    (unwind-protect
        (progn
          (org-persist-write :version '(version "hash-v1")
                             :value '(:str "hello" :num 42 :list (1 2 3))
                             :key "hash-key")
          (org-persist-write :version '(version "hash-v2")
                             :value '(("key1" . "val1") ("key2" . "val2"))
                             :key "assoc-key")
          (let ((idx-len (length org-persist--index)))
            (let ((read-hash (org-persist-read '(version "hash-v1") '(:key "hash-key")))
                  (read-assoc (org-persist-read '(version "hash-v2") '(:key "assoc-key"))))
              (let ((dir-files
                     (and (file-exists-p org-persist-directory)
                          (length (directory-files-recursively
                                   org-persist-directory "." nil)))))
                (org-persist-unregister '(version "hash-v1") 'all)
                (org-persist-unregister '(version "hash-v2") 'all)
                (let ((after-unreg (length org-persist--index)))
                  (list idx-len read-hash read-assoc dir-files after-unreg
                        (file-exists-p org-persist-directory)))))))
      (delete-directory root t))))"##,
        expect,
    );
}
