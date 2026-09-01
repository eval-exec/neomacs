use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EDITORCONFIG_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'editorconfig)
(require 'editorconfig-tools)

(defvar neomacs-editorconfig-test-sandbox
  (file-name-as-directory
   (or (getenv "NEOMACS_TEST_SANDBOX_ROOT")
       (getenv "HOME"))))

(defun neomacs-editorconfig-test-root (name)
  "Create and return a clean test directory NAME below the oracle sandbox."
  (let ((root (file-name-as-directory
               (expand-file-name name neomacs-editorconfig-test-sandbox))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-editorconfig-test-write (path contents)
  "Write CONTENTS to PATH using deterministic UTF-8 Unix encoding."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region contents nil path nil 'silent))
  path)

(defun neomacs-editorconfig-test-read (path)
  "Read decoded text from PATH."
  (with-temp-buffer
    (insert-file-contents path)
    (buffer-string)))

(defun neomacs-editorconfig-test-bytes (path)
  "Read the exact bytes stored at PATH."
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally path)
    (string-to-list (buffer-string))))

(defun neomacs-editorconfig-test-properties (properties)
  "Return PROPERTIES as a stable, symbol-keyed alist."
  (let (result)
    (maphash (lambda (key value) (push (cons key value) result)) properties)
    (sort result
          (lambda (left right)
            (string< (symbol-name (car left))
                     (symbol-name (car right)))))))

(defun neomacs-editorconfig-test-kill-file-buffer (path)
  "Kill the buffer visiting PATH, if any."
  (when-let* ((buffer (get-file-buffer path)))
    (with-current-buffer buffer
      (set-buffer-modified-p nil))
    (kill-buffer buffer)))
"####;

fn nested_project_rules_merge_globs_unset_values_and_nearest_config() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-editorconfig-test-root "editorconfig-inheritance"))
       (project (expand-file-name "commerce/" root))
       (app (expand-file-name "apps/checkout/" project))
       (service (expand-file-name "src/service.el" app))
       (spec (expand-file-name "spec/unit/test_checkout.py" app))
       (makefile (expand-file-name "Makefile" app))
       (readme (expand-file-name "README.md" project))
       (editorconfig-get-properties-function
        #'editorconfig-core-get-properties-hash))
  (neomacs-editorconfig-test-write
   (expand-file-name ".editorconfig" project)
   (concat
    "root = true\n\n"
    "[*]\n"
    "indent_style = space\n"
    "indent_size = 4\n"
    "max_line_length = 100\n"
    "trim_trailing_whitespace = true\n\n"
    "[*.md]\n"
    "max_line_length = 72\n"))
  (neomacs-editorconfig-test-write
   (expand-file-name ".editorconfig" app)
   (concat
    "[*.{el,py}]\n"
    "indent_size = 2\n\n"
    "[spec/**.py]\n"
    "indent_size = 3\n"
    "trim_trailing_whitespace = unset\n\n"
    "[Makefile]\n"
    "indent_style = tab\n"
    "indent_size = tab\n"
    "tab_width = 8\n"))
  (mapc (lambda (path) (neomacs-editorconfig-test-write path ""))
        (list service spec makefile readme))
  (list
   :service
   (neomacs-editorconfig-test-properties
    (editorconfig-call-get-properties-function service))
   :spec
   (neomacs-editorconfig-test-properties
    (editorconfig-call-get-properties-function spec))
   :makefile
   (neomacs-editorconfig-test-properties
    (editorconfig-call-get-properties-function makefile))
   :readme
   (neomacs-editorconfig-test-properties
    (editorconfig-call-get-properties-function readme))
   :nearest
   (file-relative-name
    (editorconfig-core-get-nearest-editorconfig
     (file-name-directory service))
    root)))
"####;
    let expected = expect![[
        r#"OK (:service ((indent_size . "2") (indent_style . "space") (max_line_length . "100") (trim_trailing_whitespace . "true")) :spec ((indent_size . "3") (indent_style . "space") (max_line_length . "100")) :makefile ((indent_size . "tab") (indent_style . "tab") (max_line_length . "100") (tab_width . "8") (trim_trailing_whitespace . "true")) :readme ((indent_size . "4") (indent_style . "space") (max_line_length . "72") (trim_trailing_whitespace . "true")) :nearest "commerce/apps/checkout/.editorconfig")"#
    ]];
    ParityBatchCase::value(
        "nested_project_rules_merge_globs_unset_values_and_nearest_config",
        elisp_form,
        expected,
    )
}

fn global_mode_applies_project_style_and_save_policy_to_a_real_lisp_file() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-editorconfig-test-root "editorconfig-save-policy"))
       (source (expand-file-name "src/checkout.el" root))
       (editorconfig-get-properties-function
        #'editorconfig-core-get-properties-hash)
       (editorconfig-trim-whitespaces-mode nil)
       applied
       (editorconfig-after-apply-functions
        (list (lambda (properties)
                (push (list (gethash 'indent_style properties)
                            (gethash 'indent_size properties))
                      applied))))
       (make-backup-files nil)
       (enable-local-variables :all)
       (enable-dir-local-variables nil)
       buffer result)
  (neomacs-editorconfig-test-write
   (expand-file-name ".editorconfig" root)
   (concat
    "root = true\n\n"
    "[*.el]\n"
    "indent_style = space\n"
    "indent_size = 2\n"
    "max_line_length = 72\n"
    "trim_trailing_whitespace = true\n"
    "insert_final_newline = true\n"))
  (neomacs-editorconfig-test-write
   source
   "(defun checkout-total ()    \n  (+ 20 22))  ")
  (unwind-protect
      (progn
        (editorconfig-mode 1)
        (setq buffer (find-file-noselect source))
        (with-current-buffer buffer
          (goto-char (point-min))
          (insert ";; managed by EditorConfig\n")
          (let ((before-save
                 (list
                  :mode major-mode
                  :indent-tabs indent-tabs-mode
                  :tab-width tab-width
                  :lisp-offset lisp-indent-offset
                  :fill-column fill-column
                  :final-newline require-final-newline
                  :mode-final-newline mode-require-final-newline
                  :trim-hook
                  (not (null
                        (memq #'editorconfig--delete-trailing-whitespace
                              before-save-hook)))
                  :properties
                  (neomacs-editorconfig-test-properties
                   editorconfig-properties-hash))))
            (save-buffer)
            (setq result
                  (list
                   :before-save before-save
                   :buffer
                   (buffer-substring-no-properties (point-min) (point-max))
                   :file (neomacs-editorconfig-test-read source)
                   :applied (nreverse applied)
                   :mode-enabled editorconfig-mode
                   :find-file-advice
                   (not (null
                         (advice-member-p
                          #'editorconfig--advice-find-file-noselect
                          'find-file-noselect))))))))
    (when editorconfig-mode (editorconfig-mode -1))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  result)
"####;
    let expected = expect![[
        r#"OK (:before-save (:mode emacs-lisp-mode :indent-tabs nil :tab-width 2 :lisp-offset 2 :fill-column 72 :final-newline t :mode-final-newline t :trim-hook t :properties ((indent_size . "2") (indent_style . "space") (insert_final_newline . "true") (max_line_length . "72") (trim_trailing_whitespace . "true"))) :buffer ";; managed by EditorConfig\n(defun checkout-total ()\n  (+ 20 22))\n" :file ";; managed by EditorConfig\n(defun checkout-total ()\n  (+ 20 22))\n" :applied (("space" "2")) :mode-enabled t :find-file-advice t)"#
    ]];
    ParityBatchCase::value(
        "global_mode_applies_project_style_and_save_policy_to_a_real_lisp_file",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn file_local_variables_are_overridden_or_preserved_by_user_policy() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-editorconfig-test-root "editorconfig-file-locals"))
       (source (expand-file-name "pricing.el" root))
       (editorconfig-get-properties-function
        #'editorconfig-core-get-properties-hash)
       (editorconfig-trim-whitespaces-mode nil)
       (enable-local-variables :all)
       (enable-dir-local-variables nil)
       (enable-local-eval nil)
       buffer result)
  (neomacs-editorconfig-test-write
   (expand-file-name ".editorconfig" root)
   (concat
    "root = true\n\n"
    "[*.el]\n"
    "indent_style = space\n"
    "indent_size = 2\n"
    "tab_width = 3\n"))
  (neomacs-editorconfig-test-write
   source
   (concat
    "(defun price-with-tax (price) (* price 1.2))\n\n"
    ";; Local Variables:\n"
    ";; tab-width: 7\n"
    ";; lisp-indent-offset: 6\n"
    ";; End:\n"))
  (unwind-protect
      (progn
        (editorconfig-mode 1)
        (cl-labels
            ((visit (override)
               (let ((editorconfig-override-file-local-variables override))
                 (setq buffer (find-file-noselect source))
                 (prog1
                     (with-current-buffer buffer
                       (list
                        :tab-width tab-width
                        :indent-tabs indent-tabs-mode
                        :lisp-offset lisp-indent-offset
                        :file-locals
                        (mapcar
                         (lambda (variable)
                           (assq variable file-local-variables-alist))
                         '(tab-width lisp-indent-offset))
                        :properties
                        (neomacs-editorconfig-test-properties
                         editorconfig-properties-hash)))
                   (neomacs-editorconfig-test-kill-file-buffer source)
                   (setq buffer nil)))))
          (setq result
                (list :override (visit t)
                      :preserve (visit nil)))))
    (when editorconfig-mode (editorconfig-mode -1))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  result)
"####;
    let expected = expect![[
        r#"OK (:override (:tab-width 3 :indent-tabs nil :lisp-offset 2 :file-locals ((tab-width . 7) (lisp-indent-offset . 6)) :properties ((indent_size . "2") (indent_style . "space") (tab_width . "3"))) :preserve (:tab-width 7 :indent-tabs nil :lisp-offset 6 :file-locals ((tab-width . 7) (lisp-indent-offset . 6)) :properties ((indent_size . "2") (indent_style . "space") (tab_width . "3"))))"#
    ]];
    ParityBatchCase::value(
        "file_local_variables_are_overridden_or_preserved_by_user_policy",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn charset_and_crlf_rules_control_the_exact_bytes_written_on_save() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-editorconfig-test-root "editorconfig-coding"))
       (invoice (expand-file-name "invoice.txt" root))
       (editorconfig-get-properties-function
        #'editorconfig-core-get-properties-hash)
       (editorconfig-trim-whitespaces-mode nil)
       (make-backup-files nil)
       (enable-dir-local-variables nil)
       buffer result)
  (neomacs-editorconfig-test-write
   (expand-file-name ".editorconfig" root)
   (concat
    "root = true\n\n"
    "[*.txt]\n"
    "charset = utf-8-bom\n"
    "end_of_line = crlf\n"
    "insert_final_newline = true\n"))
  (neomacs-editorconfig-test-write invoice "draft")
  (unwind-protect
      (progn
        (editorconfig-mode 1)
        (setq buffer (find-file-noselect invoice))
        (with-current-buffer buffer
          (let ((coding-before buffer-file-coding-system))
            (erase-buffer)
            (insert "café")
            (save-buffer)
            (setq result
                  (list
                   :coding-before coding-before
                   :coding-after buffer-file-coding-system
                   :base (coding-system-base buffer-file-coding-system)
                   :eol (coding-system-eol-type buffer-file-coding-system)
                   :text (neomacs-editorconfig-test-read invoice)
                   :bytes (neomacs-editorconfig-test-bytes invoice))))))
    (when editorconfig-mode (editorconfig-mode -1))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  result)
"####;
    let expected = expect![[
        r#"OK (:coding-before utf-8-with-signature-dos :coding-after utf-8-with-signature-dos :base utf-8-with-signature :eol 1 :text "café\n" :bytes (239 187 191 99 97 102 195 169 13 10))"#
    ]];
    ParityBatchCase::value(
        "charset_and_crlf_rules_control_the_exact_bytes_written_on_save",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn parsed_config_cache_is_reused_then_invalidated_after_a_real_edit() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (neomacs-editorconfig-test-root "editorconfig-cache"))
       (config (expand-file-name ".editorconfig" root))
       (source (expand-file-name "service.py" root)))
  (clrhash editorconfig-core-handle--cache-hash)
  (neomacs-editorconfig-test-write
   config
   "root = true\n\n[*.py]\nindent_style = space\nindent_size = 2\n")
  (neomacs-editorconfig-test-write source "def total():\n    return 42\n")
  (let* ((first-handle (editorconfig-core-handle config))
         (second-handle (editorconfig-core-handle config))
         (first-properties
          (editorconfig-core-get-properties-hash source)))
    (neomacs-editorconfig-test-write
     config
     "root = true\n\n[*.py]\nindent_style = tab\nindent_size = 6\n")
    (set-file-times config (time-add (current-time) 10))
    (let* ((third-handle (editorconfig-core-handle config))
           (second-properties
            (editorconfig-core-get-properties-hash source)))
      (list
       :cache-reused (eq first-handle second-handle)
       :cache-invalidated (not (eq second-handle third-handle))
       :before
       (neomacs-editorconfig-test-properties first-properties)
       :after
       (neomacs-editorconfig-test-properties second-properties)
       :cached-configs
       (hash-table-count editorconfig-core-handle--cache-hash)))))
"####;
    let expected = expect![[
        r#"OK (:cache-reused t :cache-invalidated t :before ((indent_size . "2") (indent_style . "space")) :after ((indent_size . "6") (indent_style . "tab")) :cached-configs 1)"#
    ]];
    ParityBatchCase::value(
        "parsed_config_cache_is_reused_then_invalidated_after_a_real_edit",
        elisp_form,
        expected,
    )
}

fn editorconfig_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EDITORCONFIG_MELPA_PIN, "editorconfig.el")
        .expect("prepare pinned EditorConfig source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn editorconfig_practical_workflows_batch() {
    let cases = vec![
        nested_project_rules_merge_globs_unset_values_and_nearest_config(),
        global_mode_applies_project_style_and_save_policy_to_a_real_lisp_file(),
        file_local_variables_are_overridden_or_preserved_by_user_policy(),
        charset_and_crlf_rules_control_the_exact_bytes_written_on_save(),
        parsed_config_cache_is_reused_then_invalidated_after_a_real_edit(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("editorconfig parity batch");
    assert_oracle_batch_cases(
        editorconfig_oracle(),
        test_name,
        "editorconfig parity",
        &cases,
    );
}
