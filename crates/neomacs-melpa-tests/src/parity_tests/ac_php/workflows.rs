use expect_test::expect;

use super::ParityBatchCase;

/// Indexing, which is the first thing that has to happen and the thing every
/// other workflow depends on.
///
/// `ac-php-remake-tags` is the documented command.  What is asserted is the
/// whole contract between the package and its indexer: the argument vector the
/// package built (including the two flags it decides for itself, `--rebuild`
/// and `--realpath_flag`), the configuration file the package wrote when it
/// found none -- the fixture deliberately does not write it -- the index files
/// that appeared where the package looks for them, the progress the process
/// filter parsed out of the indexer's output, and the classes that ended up in
/// the loaded index.
fn indexing_the_project_runs_the_real_indexer_contract_and_loads_the_symbols() -> ParityBatchCase {
    ParityBatchCase::value(
        "indexing_the_project_runs_the_real_indexer_contract_and_loads_the_symbols",
        r##"
(let ((root (ac-php-test-make-project))
      (program (ac-php-test-install-php)))
  (ac-php-test-in-php-buffer
   "src/Service/Cart.php"
   (call-interactively 'ac-php-remake-tags)
   (let* ((finished (ac-php-test-wait-for-index))
          (tags-data (ac-php-get-tags-data))
          (cache (expand-file-name "cache" ac-php-test-root)))
     (list :major-mode major-mode
           :indexer-finished finished
           :calls (mapcar (lambda (call)
                            (mapcar (lambda (argument)
                                      (file-name-nondirectory argument))
                                    call))
                          (ac-php-test-php-calls))
           :config (ac-php-test-read
                    (expand-file-name ".ac-php-conf.json" ac-php-test-project))
           :index-files (sort (mapcar #'file-name-nondirectory
                                      (directory-files-recursively cache ""))
                              #'string<)
           :progress ac-php-phptags-index-progress
           :classes (let (keys)
                      (maphash (lambda (key _value) (push key keys))
                               (ac-php-g--class-map tags-data))
                      (sort keys #'string<))
           :indexed-files (mapcar (lambda (file) (file-relative-name file root))
                                  (append (ac-php-g--file-list tags-data) nil))))))
"##,
        expect![[
            r##"OK (:major-mode php-mode :indexer-finished t :calls (("phpctags" ".ac-php-conf.json" "cache" "--rebuild=no" "--realpath_flag=yes")) :config "{\n  \"use-cscope\": null,\n  \"tag-dir\": null,\n  \"filter\": {\n    \"php-file-ext-list\": [\n      \"php\"\n    ],\n    \"php-path-list\": [\n      \".\"\n    ],\n    \"ignore-ruleset\": [\n      \"# like .gitignore file \",\n      \"/vendor/**/[tT]ests/**/*.php\",\n      \"/vendor/**/[Ee]xamples/**/*.php\",\n      \"/vendor/composer/*.php\",\n      \"/vendor/*.php\",\n      \"# not need php_codesniffer\",\n      \"/vendor/squizlabs/php_codesniffer/**/*.php\",\n      \"#  -- end -- \"\n    ]\n  }\n}" :index-files ("tags-vendor.el" "tags.el") :progress 83 :classes ("\\Shop\\Model\\Product" "\\Shop\\Service\\BaseCart" "\\Shop\\Service\\Cart") :indexed-files ("src/Model/Product.php" "src/Service/BaseCart.php" "src/Service/Cart.php"))"##
        ]],
    )
}

fn completing_this_arrow_offers_the_class_and_its_inherited_members() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_this_arrow_offers_the_class_and_its_inherited_members",
        r##"
(let ((root (ac-php-test-make-project))
      (program (ac-php-test-install-php)))
  (ac-php-test-in-php-buffer
   "src/Service/Cart.php"
   (call-interactively 'ac-php-remake-tags)
   (ac-php-test-wait-for-index)
   (goto-char (point-min))
   (search-forward "return $product;")
   (beginning-of-line)
   (insert "$this->")
   (let ((candidates (ac-php-test-candidates)))
     (list :prefix-point (ac-php-prefix)
           :point (point)
           :candidates (ac-php-test-plain candidates)
           :annotated (ac-php-test-annotated candidates)))))
"##,
        expect![[
            r#"OK (:prefix-point 166 :point 166 :candidates ("itemCount(" "reset(" "total(") :annotated (("itemCount(" :tag-type "m" :access "public" :return-type "" :from "\\Shop\\Service\\BaseCart" :help "") ("reset(" :tag-type "m" :access "public" :return-type "" :from "\\Shop\\Service\\BaseCart" :help "") ("total(" :tag-type "m" :access "public" :return-type "" :from "\\Shop\\Service\\Cart" :help "")))"#
        ]],
    )
}

fn a_local_typed_by_new_completes_the_other_class_without_hiding_anything() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_local_typed_by_new_completes_the_other_class_without_hiding_anything",
        r##"
(let ((root (ac-php-test-make-project))
      (program (ac-php-test-install-php)))
  (ac-php-test-in-php-buffer
   "src/Service/Cart.php"
   (call-interactively 'ac-php-remake-tags)
   (ac-php-test-wait-for-index)
   (cl-flet ((complete-after (text)
               (goto-char (point-min))
               (search-forward "return $product;")
               (beginning-of-line)
               (let ((start (point)))
                 (insert text)
                 (prog1 (ac-php-test-candidates)
                   (delete-region start (point))))))
     (let ((instance (complete-after "$product->"))
           (static (complete-after "Product::")))
       (list :instance (ac-php-test-plain instance)
             :static (ac-php-test-plain static)
             :same-list (equal (ac-php-test-plain instance) (ac-php-test-plain static))
             :annotated (ac-php-test-annotated instance))))))
"##,
        expect![[
            r#"OK (:instance ("CURRENCY" "__construct(" "auditLog(" "getName(" "name" "priceCents" "setPrice(") :static ("CURRENCY" "__construct(" "auditLog(" "getName(" "name" "priceCents" "setPrice(") :same-list t :annotated (("CURRENCY" :tag-type "d" :access "public" :return-type "void" :from "\\Shop\\Model\\Product" :help "") ("__construct(" :tag-type "m" :access "public" :return-type "" :from "\\Shop\\Model\\Product" :help "$name, $priceCents") ("auditLog(" :tag-type "m" :access "protected" :return-type "" :from "\\Shop\\Model\\Product" :help "$message") ("getName(" :tag-type "m" :access "public" :return-type "" :from "\\Shop\\Model\\Product" :help "") ("name" :tag-type "p" :access "private" :return-type "string" :from "\\Shop\\Model\\Product" :help "") ("priceCents" :tag-type "p" :access "public" :return-type "int" :from "\\Shop\\Model\\Product" :help "") ("setPrice(" :tag-type "m" :access "public" :return-type "" :from "\\Shop\\Model\\Product" :help "$cents, $vat=19")))"#
        ]],
    )
}

fn an_unqualified_function_completes_only_inside_its_own_namespace() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_unqualified_function_completes_only_inside_its_own_namespace",
        r##"
(let ((root (ac-php-test-make-project))
      (program (ac-php-test-install-php)))
  (ac-php-test-in-php-buffer
   "src/Model/Product.php"
   (call-interactively 'ac-php-remake-tags)
   (ac-php-test-wait-for-index)
   (goto-char (point-min))
   (search-forward "return $this->name;")
   (beginning-of-line)
   (insert "form")
   (let ((same-namespace (ac-php-test-candidates)))
     (list :in-declaring-namespace (ac-php-test-plain same-namespace)
           :annotated (ac-php-test-annotated same-namespace)
           :in-other-namespace
           (ac-php-test-in-php-buffer
            "src/Service/Cart.php"
            (goto-char (point-min))
            (search-forward "return $product;")
            (beginning-of-line)
            (insert "form")
            (ac-php-test-plain (ac-php-test-candidates)))))))
"##,
        expect![[
            r#"OK (:in-declaring-namespace ("formatMoney(") :annotated (("formatMoney(" :tag-type "f" :access nil :return-type "" :from nil :help "$cents, $currency='EUR'")) :in-other-namespace nil)"#
        ]],
    )
}

fn the_documentation_beside_a_candidate_renders_every_kind_of_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_documentation_beside_a_candidate_renders_every_kind_of_entry",
        r##"
(let ((root (ac-php-test-make-project))
      (program (ac-php-test-install-php)))
  (ac-php-test-in-php-buffer
   "src/Model/Product.php"
   (call-interactively 'ac-php-remake-tags)
   (ac-php-test-wait-for-index)
   (goto-char (point-min))
   (search-forward "return $this->name;")
   (beginning-of-line)
   (let ((start (point)))
     (insert "form")
     (let ((functions (ac-php-test-candidates)))
       (delete-region start (point))
       (insert "$this->")
       (let* ((members (ac-php-test-candidates))
              (named (lambda (name)
                       (car (cl-remove-if-not
                             (lambda (candidate)
                               (string= (substring-no-properties candidate) name))
                             members)))))
         (mapcar (lambda (entry)
                   (list (car entry)
                         (substring-no-properties
                          (ac-php-document (funcall (cdr entry))))))
                 (list (cons "CURRENCY" (lambda () (funcall named "CURRENCY")))
                       (cons "name" (lambda () (funcall named "name")))
                       (cons "setPrice(" (lambda () (funcall named "setPrice(")))
                       (cons "getName(" (lambda () (funcall named "getName(")))
                       (cons "formatMoney(" (lambda () (car functions))))))))))
"##,
        expect![[
            r#"OK (("CURRENCY" "CURRENCY\n\11[  type]:void\n\11[access]:public\n\11[  from]:\\Shop\\Model\\Product") ("name" "name\n\11[  type]:string\n\11[access]:private\n\11[  from]:\\Shop\\Model\\Product") ("setPrice(" "setPrice($cents, $vat=19)\n\11[  type]:\n\11[access]:public\n\11[  from]:\\Shop\\Model\\Product") ("getName(" "getName()\n\11[  type]:\n\11[access]:public\n\11[  from]:\\Shop\\Model\\Product") ("formatMoney(" " formatMoney($cents, $currency='EUR') "))"#
        ]],
    )
}

fn choosing_a_method_offers_its_argument_lists_and_expands_the_chosen_one() -> ParityBatchCase {
    ParityBatchCase::value(
        "choosing_a_method_offers_its_argument_lists_and_expands_the_chosen_one",
        r##"
(require 'yasnippet)
(let ((root (ac-php-test-make-project))
      (program (ac-php-test-install-php)))
  (ac-php-test-in-php-buffer
   "src/Service/Cart.php"
   (call-interactively 'ac-php-remake-tags)
   (ac-php-test-wait-for-index)
   ;; The argument templates are expanded by yasnippet, so the workflow needs
   ;; the mode a user of this feature would have on.
   (yas-minor-mode 1)
   (goto-char (point-min))
   (search-forward "return $product;")
   (beginning-of-line)
   (insert "$product->setP")
   (ac-start :force-init t)
   (ac-update t)
   (let ((first-round (ac-php-test-plain ac-candidates)))
     (ac-complete)
     (let ((templates (mapcar #'substring-no-properties ac-php-template-candidates))
           (source (mapcar #'car ac-current-sources)))
       (ac-update t)
       (ac-complete)
       (list :first-round first-round
             :templates templates
             :second-source source
             :line (buffer-substring-no-properties
                    (line-beginning-position) (line-end-position))
             :live-snippets (length (yas-active-snippets)))))))
"##,
        expect![[
            r#"OK (:first-round ("setPrice(") :templates ("$cents)" "$cents, $vat)") :second-source ((candidates . ac-php-template-candidate)) :line "$product->setPrice($cents)return $product;" :live-snippets 1)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        indexing_the_project_runs_the_real_indexer_contract_and_loads_the_symbols(),
        completing_this_arrow_offers_the_class_and_its_inherited_members(),
        a_local_typed_by_new_completes_the_other_class_without_hiding_anything(),
        an_unqualified_function_completes_only_inside_its_own_namespace(),
        the_documentation_beside_a_candidate_renders_every_kind_of_entry(),
        choosing_a_method_offers_its_argument_lists_and_expands_the_chosen_one(),
    ]
}
