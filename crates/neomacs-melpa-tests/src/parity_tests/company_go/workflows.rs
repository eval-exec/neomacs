use expect_test::expect;

use super::ParityBatchCase;

/// Loading the package registers the backend command and its documented
/// configuration surface: the five defcustoms with defaults and types, the
/// customization group, and the backend's elisp entry points.
fn loading_registers_the_backend_and_its_configuration() -> ParityBatchCase {
    ParityBatchCase::value(
        "loading_registers_the_backend_and_its_configuration",
        r####"(list
 :source (cgo319-test-source-state)
 :entry-points
 (list :backend-command (commandp 'company-go)
       :prefix (fboundp 'company-go--prefix)
       :candidates (fboundp 'company-go--candidates)
       :annotation (fboundp 'company-go--annotation)
       :meta (fboundp 'company-go--meta)
       :location (fboundp 'company-go--location)
       :doc-buffer (fboundp 'company-go--doc-buffer))
 :options
 (mapcar
  (lambda (option)
    (list :option option
          :custom-variable-p (and (custom-variable-p option) t)
          :standard (eval (car (get option 'standard-value)))
          :type (get option 'custom-type)))
  '(company-go-show-annotation
    company-go-begin-after-member-access
    company-go-insert-arguments
    company-go-gocode-command
    company-go-gocode-args)))"####,
        expect![[
            r#"OK (:source (:upstream-tree "6a38841c337f3615d18392d0d2d6d3292b9b1092" :feature t :version "20170825.1643" :company "20260721.100" :go-mode "20260510.1707") :entry-points (:backend-command t :prefix t :candidates t :annotation nil :meta nil :location t :doc-buffer nil) :options ((:option company-go-show-annotation :custom-variable-p t :standard nil :type boolean) (:option company-go-begin-after-member-access :custom-variable-p t :standard t :type boolean) (:option company-go-insert-arguments :custom-variable-p t :standard t :type boolean) (:option company-go-gocode-command :custom-variable-p t :standard "gocode" :type string) (:option company-go-gocode-args :custom-variable-p t :standard nil :type (repeat string))))"#
        ]],
    )
}

/// The pure candidate pipeline: `company-go--format-meta' strips the func
/// marker and keeps other type prefixes, and `company-go--get-candidates'
/// propertizes each CSV row with its meta and package.
fn the_csv_candidate_pipeline_formats_meta_and_packages() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_csv_candidate_pipeline_formats_meta_and_packages",
        r####"(let ((rows '(("func" "Println" "func(a ...interface{})" "fmt")
                     ("func" "Sprint" "func(a ...interface{}) string" "fmt")
                     ("package" "fmt" "package" "")
                     ("const" "Pi" "float64" "math"))))
  (list
   :format-meta
   (mapcar (lambda (row)
             (list :raw (nth 2 row)
                   :formatted (company-go--format-meta row)))
           rows)
   :candidates
   (mapcar (lambda (cand)
             (list :text (substring-no-properties cand)
                   :meta (get-text-property 0 'meta cand)
                   :package (get-text-property 0 'package cand)))
           (company-go--get-candidates
            '("func,,Println,,func(a ...interface{}),,fmt"
              "const,,Pi,,float64,,math"
              "package,,fmt,,package,,")))))"####,
        expect![[
            r#"OK (:format-meta ((:raw "func(a ...interface{})" :formatted "func Println(a ...interface{})") (:raw "func(a ...interface{}) string" :formatted "func Sprint(a ...interface{}) string") (:raw "package" :formatted "package fmt package") (:raw "float64" :formatted "const Pi float64")) :candidates ((:text "Println" :meta "func Println(a ...interface{})" :package "fmt") (:text "Pi" :meta "const Pi float64" :package "math") (:text "fmt" :meta "package fmt package" :package "")))"#
        ]],
    )
}

/// The invocation contract through a fake gocode: the real arg assembly
/// passes the extra args, the csv-with-package formatter, the buffer file
/// name, and the c<offset> cursor argument, and the canned CSV answer
/// flows through `company-go--candidates' as propertized candidates.
///
/// The cursor argument is recorded as the argument itself, never as its
/// position in the recorded argv: that text quotes the sandbox path, so an
/// index into it pins the harness's path length instead of anything either
/// editor computed.  See DIVERGENCES.md 127.
fn the_invocation_contract_through_a_fake_gocode() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_invocation_contract_through_a_fake_gocode",
        r####"(unwind-protect
    (progn
      (cgo319-test-reset)
      (let* ((root (cgo319-test-root))
             (file (expand-file-name "main.go" root))
             (script (cgo319-test-fake-gocode
                      root
                      "func,,Println,,func(a ...interface{}),,fmt")))
        (let ((coding-system-for-write 'utf-8-unix))
          (with-temp-file file (insert "package main\n\nfunc main() {\n\tfmt.P\n}\n")))
        (let ((buffer (find-file-noselect file)))
          (with-current-buffer buffer
            (goto-char (point-min))
            (search-forward "fmt.P")
            (setq company-go-gocode-command script
                  company-go-gocode-args '("-s"))
            (let* ((candidates (company-go--candidates))
                   (argv (with-temp-buffer
                           (insert-file-contents
                            (expand-file-name "argv.txt" root))
                           (buffer-substring-no-properties
                            (point-min) (point-max)))))
              (list
               :argv argv
               :candidates
               (mapcar (lambda (cand)
                         (list :text (substring-no-properties cand)
                               :meta (get-text-property 0 'meta cand)
                               :package (get-text-property 0 'package cand)))
                       candidates)
               :offset-arg
               (cl-find-if (lambda (argument)
                             (string-match-p "\\`c[0-9]+\\'" argument))
                           (split-string argv "\n" t))))))))
  (cgo319-test-reset))"####,
        expect![[
            r#"OK (:argv "-s\n-f=csv-with-package\nautocomplete\n[ORACLE-SANDBOX]/company-go-fixture/main.go\nc34\n" :candidates ((:text "Println" :meta "func Println(a ...interface{})" :package "fmt")) :offset-arg "c34")"#
        ]],
    )
}

/// The prefix contract: with `company-go-begin-after-member-access' the
/// prefix after a member dot is grabbed (returning the symbol and the
/// trailing dot marker), and with it off the plain symbol is grabbed.
fn the_prefix_contract_at_member_access_dots() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_prefix_contract_at_member_access_dots",
        r####"(unwind-protect
    (progn
      (cgo319-test-reset)
      (with-temp-buffer
        (goto-char (point-min))
        (insert "package main\n\nfunc main() {\n\tfmt.Print\n}\n")
        (goto-char (point-min))
        (search-forward "fmt.P")
        (let ((at-symbol (company-go--prefix)))
          (goto-char (point-min))
          (search-forward "fmt.")
          (let ((at-dot-begin (company-go--prefix)))
            (setq company-go-begin-after-member-access nil)
            (goto-char (point-min))
            (search-forward "fmt.P")
            (let ((at-symbol-plain (company-go--prefix)))
              (list :at-symbol at-symbol
                    :at-dot-begin at-dot-begin
                    :at-symbol-plain at-symbol-plain))))))
  (cgo319-test-reset))"####,
        expect![[
            r#"OK (:at-symbol ("P" "rint" t) :at-dot-begin ("" "Print" t) :at-symbol-plain "P")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loading_registers_the_backend_and_its_configuration(),
        the_csv_candidate_pipeline_formats_meta_and_packages(),
        the_invocation_contract_through_a_fake_gocode(),
        the_prefix_contract_at_member_access_dots(),
    ]
}
