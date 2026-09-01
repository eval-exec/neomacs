//! Practical parity for company-auctex's AUCTeX company backends.
//!
//! These cases initialize the backends, complete macros and environments
//! through the public company protocol, insert yasnippet argument fields,
//! complete labels and citations from planted tables, wrap math symbols,
//! and stay silent outside LaTeX-mode.

use std::time::Duration;

use expect_test::expect;

use crate::{
    AUCTEX_GNU_ELPA_PIN, COMPANY_AUCTEX_MELPA_PIN, COMPANY_MELPA_PIN, CachedMelpaOracle,
    YASNIPPET_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'tex)
(require 'latex)
(require 'company)
(require 'yasnippet)
(require 'company-auctex)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")
(setq TeX-parse-self nil
      TeX-auto-save nil
      TeX-master t
      make-backup-files nil
      create-lockfiles nil)

(defconst ca453-test-tree
  "a12166d464c7645ed6474c75b910f9adea04b9aa")
(defconst ca453-test-manifest
  '(("company-auctex-pkg.el" . "25ae06e218a01420effa410daa25c912752adec39b0f75f506c0fc492279af35")
    ("company-auctex.el" . "b227feda2ce4a757661e2c114223148d14628cf9f3208d8ae936ea8bbd74d061")))

(defun ca453-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun ca453-test-source-state ()
  (let* ((located (locate-library "company-auctex.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (ca453-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/company-auctex.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car ca453-test-manifest)))
      (error "Unexpected installed company-auctex payload: %S"
             (or manifest files)))
    (dolist (entry ca453-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (ca453-test-sha file) expected))
          (error "Unexpected installed company-auctex source: %S"
                 (cons entry manifest)))))
    (list :tree ca453-test-tree
          :manifest manifest
          :feature (featurep 'company-auctex)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'company-auctex package-alist)))))))

(defun ca453-test-with-tables (macros environments labels bibs math body)
  (cl-letf (((symbol-function 'TeX-symbol-list)
             (lambda () macros))
            ((symbol-function 'LaTeX-length-list)
             (lambda () nil))
            ((symbol-function 'LaTeX-environment-list)
             (lambda () environments))
            ((symbol-function 'LaTeX-label-list)
             (lambda () labels))
            ((symbol-function 'LaTeX-bibitem-list)
             (lambda () bibs)))
    (let ((LaTeX-math-list math)
          (LaTeX-math-default nil)
          (LaTeX-section-list '(("section" 1) ("subsection" 2)))
          (LaTeX-font-list nil)
          (yas-snippet-dirs nil))
      (funcall body))))

(defun ca453-test-in-latex (contents body)
  (with-temp-buffer
    (switch-to-buffer (current-buffer))
    (LaTeX-mode)
    (company-mode 1)
    (insert contents)
    (goto-char (point-max))
    (funcall body)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(COMPANY_AUCTEX_MELPA_PIN, "company-auctex.el")
        .expect("prepare pinned company-auctex source below ./tmp")
        .with_melpa_dependency(COMPANY_MELPA_PIN)
        .expect("prepare pinned company dependency below ./tmp")
        .with_melpa_dependency(YASNIPPET_MELPA_PIN)
        .expect("prepare pinned yasnippet dependency below ./tmp")
        .with_gnu_elpa_dependency(AUCTEX_GNU_ELPA_PIN)
        .expect("prepare pinned AUCTeX dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn init_registers_backends_and_macros_expand_includegraphics() -> ParityBatchCase {
    ParityBatchCase::value(
        "init_registers_backends_and_macros_expand_includegraphics",
        r####"
(let ((company-backends '(company-capf company-files)))
  (company-auctex-init)
  (let ((registered company-backends)
        (again (progn (company-auctex-init) company-backends)))
    (ca453-test-with-tables
     '(("includegraphics" TeX-arg-file)
       ("include" TeX-arg-file)
       ("input" TeX-arg-file)
       ("section" "Title"))
     nil nil nil nil
     (lambda ()
       (ca453-test-in-latex
        "\\incl"
        (lambda ()
          (let* ((prefix (company-auctex-macros 'prefix))
                 (candidates (company-auctex-macros 'candidates prefix)))
            (delete-region (- (point) (length prefix)) (point))
            (insert "includegraphics")
            (company-auctex-macros 'post-completion "includegraphics")
            (list :source (ca453-test-source-state)
                  :registered registered
                  :second-init again
                  :prefix prefix
                  :candidates candidates
                  :buffer (buffer-substring-no-properties (point-min) (point-max))
                  :point (point)
                  :yas yas-minor-mode))))))))
"####,
        expect![[
            r#"OK (:source (:tree "a12166d464c7645ed6474c75b910f9adea04b9aa" :manifest (("company-auctex-pkg.el" . "25ae06e218a01420effa410daa25c912752adec39b0f75f506c0fc492279af35") ("company-auctex.el" . "b227feda2ce4a757661e2c114223148d14628cf9f3208d8ae936ea8bbd74d061")) :feature t :version "20200529.1835") :registered #1=((company-auctex-macros company-auctex-symbols company-auctex-environments) company-auctex-bibs company-auctex-labels company-capf company-files) :second-init #1# :prefix "incl" :candidates ("includegraphics" "include") :buffer "\\includegraphics{Filename}" :point 18 :yas t)"#
        ]],
    )
}

fn environments_expand_begin_end_around_figure() -> ParityBatchCase {
    ParityBatchCase::value(
        "environments_expand_begin_end_around_figure",
        r####"
(ca453-test-with-tables
 nil
 '(("figure" ["htbp!"])
   ("figure*" ["htbp!"])
   ("document"))
 nil nil nil
 (lambda ()
   (ca453-test-in-latex
    "\\begfig"
    (lambda ()
      (let* ((prefix (company-auctex-environments 'prefix))
             (candidates (company-auctex-environments 'candidates prefix)))
        (delete-region (- (point) (length prefix)) (point))
        (insert "begfigure")
        (company-auctex-environments 'post-completion "begfigure")
        (list :source (ca453-test-source-state)
              :prefix prefix
              :candidates candidates
              :buffer (buffer-substring-no-properties (point-min) (point-max))
              :point (point)))))))
"####,
        expect![[
            r#"OK (:source (:tree "a12166d464c7645ed6474c75b910f9adea04b9aa" :manifest (("company-auctex-pkg.el" . "25ae06e218a01420effa410daa25c912752adec39b0f75f506c0fc492279af35") ("company-auctex.el" . "b227feda2ce4a757661e2c114223148d14628cf9f3208d8ae936ea8bbd74d061")) :feature t :version "20200529.1835") :prefix "begfig" :candidates ("begfigure" "begfigure*") :buffer "\\begin{figure}[htbp!]\n\n\\end{figure}" :point 15)"#
        ]],
    )
}

fn labels_and_bibs_complete_from_planted_tables() -> ParityBatchCase {
    ParityBatchCase::value(
        "labels_and_bibs_complete_from_planted_tables",
        r####"
(ca453-test-with-tables
 nil nil
 '(("sec:intro") ("sec:implementation") ("sec:café"))
 '(("knuth1984") ("knuth1992") ("lamport1994"))
 nil
 (lambda ()
   (list
    :source (ca453-test-source-state)
    :ref (ca453-test-in-latex
          "See \\ref{sec:"
          (lambda ()
            (let ((prefix (company-auctex-labels 'prefix)))
              (list :prefix prefix
                    :candidates (company-auctex-labels 'candidates prefix)))))
    :cite (ca453-test-in-latex
           "\\cite[p. 42]{knu"
           (lambda ()
             (let ((prefix (company-auctex-bibs 'prefix)))
               (list :prefix prefix
                     :candidates (company-auctex-bibs 'candidates prefix)))))
    :empty (ca453-test-with-tables
            nil nil nil nil nil
            (lambda ()
              (ca453-test-in-latex
               "\\ref{"
               (lambda ()
                 (company-auctex-labels 'candidates
                                        (company-auctex-labels 'prefix)))))))))
"####,
        expect![[
            r#"OK (:source (:tree "a12166d464c7645ed6474c75b910f9adea04b9aa" :manifest (("company-auctex-pkg.el" . "25ae06e218a01420effa410daa25c912752adec39b0f75f506c0fc492279af35") ("company-auctex.el" . "b227feda2ce4a757661e2c114223148d14628cf9f3208d8ae936ea8bbd74d061")) :feature t :version "20200529.1835") :ref (:prefix "sec:" :candidates ("sec:intro" "sec:implementation" "sec:café")) :cite (:prefix "knu" :candidates ("knuth1984" "knuth1992")) :empty nil)"#
        ]],
    )
}

fn symbols_wrap_math_and_non_latex_prefix_is_silent() -> ParityBatchCase {
    ParityBatchCase::value(
        "symbols_wrap_math_and_non_latex_prefix_is_silent",
        r####"
(ca453-test-with-tables
 nil nil nil nil
 '((?a "alpha" "Greek alpha" 945)
   (?b "beta" "Greek beta" 946)
   (?l "leq" "less or equal" 8804))
 (lambda ()
   (let ((math
          (ca453-test-in-latex
           "Euler wrote \\alp"
           (lambda ()
             (let* ((prefix (company-auctex-symbols 'prefix))
                    (candidates (company-auctex-symbols 'candidates prefix))
                    (annotation (company-auctex-symbols 'annotation "alpha")))
               (delete-region (- (point) (length prefix)) (point))
               (insert "alpha")
               (company-auctex-symbols 'post-completion "alpha")
               (list :prefix prefix
                     :candidates candidates
                     :annotation annotation
                     :buffer (buffer-substring-no-properties
                              (point-min) (point-max))
                     :point (point))))))
         (outside
          (with-temp-buffer
            (insert "\\alp")
            (goto-char (point-max))
            (company-auctex-symbols 'prefix))))
     (list :source (ca453-test-source-state)
           :math math
           :outside outside))))
"####,
        expect![[
            r#"OK (:source (:tree "a12166d464c7645ed6474c75b910f9adea04b9aa" :manifest (("company-auctex-pkg.el" . "25ae06e218a01420effa410daa25c912752adec39b0f75f506c0fc492279af35") ("company-auctex.el" . "b227feda2ce4a757661e2c114223148d14628cf9f3208d8ae936ea8bbd74d061")) :feature t :version "20200529.1835") :math (:prefix "alp" :candidates ("alpha") :annotation " α" :buffer "Euler wrote $\\alpha$" :point 20) :outside nil)"#
        ]],
    )
}

#[test]
fn company_auctex_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        init_registers_backends_and_macros_expand_includegraphics(),
        environments_expand_begin_end_around_figure(),
        labels_and_bibs_complete_from_planted_tables(),
        symbols_wrap_math_and_non_latex_prefix_is_silent(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "company-auctex-rank453",
        "company_auctex_parity",
        &cases,
    );
}
