use expect_test::expect;

use super::ParityBatchCase;

fn apib_mode_authors_navigates_and_reviews_a_real_inventory_blueprint() -> ParityBatchCase {
    ParityBatchCase::value(
        "apib_mode_authors_navigates_and_reviews_a_real_inventory_blueprint",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apib-authoring-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (blueprint
        (expand-file-name
         "contracts/Inventory API.apib"
         root))
       (drafter (expand-file-name "tools/drafter" root))
       (default-directory root)
       buffer
       resolved-drafter
       headings
       hierarchy
       hidden-state
       result)
  (unwind-protect
      (progn
        (neomacs-apib-test-cleanup root)
        (neomacs-apib-test-write-drafter drafter)
        (make-directory (file-name-directory blueprint) t)
        (with-temp-file blueprint
          (insert
           "FORMAT: 1A\n"
           "HOST: https://inventory.example.test\n\n"
           "# Inventory API\n\n"
           "Inventory availability for warehouse clients.\n\n"
           "# Group Products\n\n"
           "## Products Collection [/products]\n\n"
           "### List Products [GET]\n\n"
           "+ Response 200 (application/json)\n"
           "    + Attributes (array[Product], fixed-type)\n"
           "        + id: 42 (number, required) - Product identifier\n"
           "        + name: Hammer (string, required) - Display name\n"))
        (setq buffer (find-file-noselect blueprint))
        (switch-to-buffer buffer)
        (let ((apib-drafter-executable drafter))
          (apib-mode)
          (setq resolved-drafter apib-drafter-executable))
        (goto-char (point-min))
        (search-forward
         "        + name: Hammer (string, required) - Display name")
        (end-of-line)
        (insert
         "\n"
         "        + available: true (boolean, required) - Stock state\n"
         "\n"
         "### Create Product [POST]\n"
         "\n"
         "+ Request (application/json)\n"
         "    + Attributes\n"
         "        + name: Saw (string, required)\n"
         "\n"
         "+ Response 201 (application/json)\n"
         "    + Headers\n"
         "\n"
         "            Location: /products/43\n")
        (font-lock-ensure)
        (goto-char (point-min))
        (dotimes (_ 5)
          (markdown-next-visible-heading 1)
          (push
           (list
            (line-number-at-pos)
            (buffer-substring-no-properties
             (line-beginning-position)
             (line-end-position)))
           headings))
        (setq headings (nreverse headings))
        (goto-char (point-min))
        (while
            (re-search-forward "^#+" nil t)
          (beginning-of-line)
          (looking-at outline-regexp)
          (push
           (list
            (line-number-at-pos)
            (funcall outline-level)
            (buffer-substring-no-properties
             (line-beginning-position)
             (line-end-position)))
           hierarchy)
          (forward-line 1))
        (setq hierarchy (nreverse hierarchy))
        (goto-char (point-min))
        (search-forward "# Group Products")
        (beginning-of-line)
        (outline-hide-subtree)
        (forward-line 2)
        (setq hidden-state
              (list
               (line-number-at-pos)
               (invisible-p (point))
               (buffer-substring-no-properties
                (line-beginning-position)
                (line-end-position))))
        (outline-show-all)
        (save-buffer)
        (goto-char (point-min))
        (search-forward "+ Response 201")
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :parent (derived-mode-p 'markdown-mode)
               :drafter
               (file-relative-name resolved-drafter root)
               :headings headings
               :hierarchy hierarchy
               :hidden hidden-state
               :point
               (list
                (line-number-at-pos)
                (current-column))
               :faces
               (mapcar
                (lambda (token)
                  (list token (neomacs-apib-test-face-at token)))
                '("Inventory API"
                  "Response"
                  "200"
                  "application/json"
                  "Attributes"
                  "42"
                  "number, required"
                  "available"
                  "true"
                  "boolean, required"
                  "Request"
                  "Headers"))
               :modified (buffer-modified-p)
               :disk
               (neomacs-apib-test-file-string blueprint))))
    (neomacs-apib-test-cleanup root))
  result)
"####,
        expect![[
            r#####"OK (:file "contracts/Inventory API.apib" :mode apib-mode :parent markdown-mode :drafter "tools/drafter" :headings ((4 "# Inventory API") (8 "# Group Products") (10 "## Products Collection [/products]") (12 "### List Products [GET]") (20 "### Create Product [POST]")) :hierarchy ((4 1 "# Inventory API") (8 1 "# Group Products") (10 2 "## Products Collection [/products]") (12 3 "### List Products [GET]") (20 3 "### Create Product [POST]")) :hidden (10 2 "## Products Collection [/products]") :point (26 14) :faces (("Inventory API" markdown-header-face-1) ("Response" font-lock-keyword-face) ("200" font-lock-constant-face) ("application/json" font-lock-variable-name-face) ("Attributes" font-lock-keyword-face) ("42" font-lock-constant-face) ("number, required" font-lock-constant-face) ("available" nil) ("true" font-lock-constant-face) ("boolean, required" font-lock-constant-face) ("Request" font-lock-keyword-face) ("Headers" font-lock-keyword-face)) :modified nil :disk "FORMAT: 1A\nHOST: https://inventory.example.test\n\n# Inventory API\n\nInventory availability for warehouse clients.\n\n# Group Products\n\n## Products Collection [/products]\n\n### List Products [GET]\n\n+ Response 200 (application/json)\n    + Attributes (array[Product], fixed-type)\n        + id: 42 (number, required) - Product identifier\n        + name: Hammer (string, required) - Display name\n        + available: true (boolean, required) - Stock state\n\n### Create Product [POST]\n\n+ Request (application/json)\n    + Attributes\n        + name: Saw (string, required)\n\n+ Response 201 (application/json)\n    + Headers\n\n            Location: /products/43\n\n")"#####
        ]],
    )
}

fn apib_mode_validates_parses_and_exports_assets_from_a_saved_blueprint() -> ParityBatchCase {
    ParityBatchCase::value(
        "apib_mode_validates_parses_and_exports_assets_from_a_saved_blueprint",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apib-publish-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (blueprint
        (expand-file-name
         "contracts/Inventory API.apib"
         root))
       (drafter (expand-file-name "tools/drafter" root))
       (trace (expand-file-name "drafter.trace" root))
       (default-directory root)
       buffer
       validation
       parse-result
       json-assets
       schema-assets
       valid
       result)
  (unwind-protect
      (progn
        (neomacs-apib-test-cleanup root)
        (neomacs-apib-test-write-drafter drafter)
        (make-directory (file-name-directory blueprint) t)
        (with-temp-file blueprint
          (insert
           "FORMAT: 1A\n"
           "HOST: https://inventory.example.test\n\n"
           "# Inventory API\n\n"
           "## Product [/products/{id}]\n\n"
           "### Retrieve Product [GET]\n\n"
           "+ Parameters\n"
           "    + id: 42 (number, required)\n\n"
           "+ Response 200 (application/json)\n"
           "    + Attributes\n"
           "        + id: 42 (number)\n"
           "        + name: Hammer (string)\n"
           "        + available: true (boolean)\n"))
        (setq buffer (find-file-noselect blueprint))
        (switch-to-buffer buffer)
        (let ((apib-drafter-executable drafter)
              (apib-result-buffer "*apib-publish-result*")
              (apib-asset-buffer "*apib-publish-assets*"))
          (apib-mode)
          (save-window-excursion
            (apib-validate)
            (with-current-buffer apib-result-buffer
              (setq validation
                    (list
                     major-mode
                     (buffer-substring-no-properties
                      (point-min)
                      (point-max)))))
            (switch-to-buffer buffer)
            (apib-parse)
            (with-current-buffer apib-result-buffer
              (setq parse-result
                    (list
                     major-mode
                     (buffer-substring-no-properties
                      (point-min)
                      (point-max)))))
            (switch-to-buffer buffer)
            (apib-get-json)
            (with-current-buffer apib-asset-buffer
              (setq json-assets
                    (buffer-substring-no-properties
                     (point-min)
                     (point-max))))
            (switch-to-buffer buffer)
            (apib-get-json-schema)
            (with-current-buffer apib-asset-buffer
              (setq schema-assets
                    (buffer-substring-no-properties
                     (point-min)
                     (point-max))))
            (switch-to-buffer buffer)
            (setq valid (apib-valid-p))))
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :validation validation
               :parse parse-result
               :json-assets json-assets
               :schema-assets schema-assets
               :valid valid
               :trace (neomacs-apib-test-file-string trace)
               :modified (buffer-modified-p)
               :disk
               (neomacs-apib-test-file-string blueprint))))
    (neomacs-apib-test-cleanup root))
  result)
"####,
        expect![[
            r#####"OK (:file "contracts/Inventory API.apib" :mode apib-mode :validation (compilation-mode "[ORACLE-SANDBOX]/apib-publish-workflow/tools/drafter -lu [ORACLE-SANDBOX]/apib-publish-workflow/contracts/Inventory API.apib\nOK: API Blueprint is valid\n") :parse (compilation-mode "[ORACLE-SANDBOX]/apib-publish-workflow/tools/drafter -f json -u [ORACLE-SANDBOX]/apib-publish-workflow/contracts/Inventory API.apib\n{\"element\":\"parseResult\",\"content\":[{\"element\":\"category\",\"content\":[{\"element\":\"asset\",\"attributes\":{\"contentType\":{\"element\":\"string\",\"content\":\"application/json\"}},\"content\":\"{\\\"id\\\":42,\\\"name\\\":\\\"Hammer\\\",\\\"available\\\":true}\"},{\"element\":\"asset\",\"attributes\":{\"contentType\":{\"element\":\"string\",\"content\":\"application/schema+json\"}},\"content\":\"{\\\"$schema\\\":\\\"http://json-schema.org/draft-04/schema#\\\",\\\"type\\\":\\\"object\\\",\\\"required\\\":[\\\"id\\\",\\\"name\\\"],\\\"properties\\\":{\\\"id\\\":{\\\"type\\\":\\\"number\\\"},\\\"name\\\":{\\\"type\\\":\\\"string\\\"},\\\"available\\\":{\\\"type\\\":\\\"boolean\\\"}}}\"}]},{\"element\":\"asset\",\"attributes\":{\"contentType\":{\"element\":\"string\",\"content\":\"application/json\"}},\"content\":\"{\\\"id\\\":43,\\\"name\\\":\\\"Saw\\\",\\\"available\\\":false}\"}]}\n") :json-assets "{\"id\":43,\"name\":\"Saw\",\"available\":false}\n\n{\"id\":42,\"name\":\"Hammer\",\"available\":true}\n" :schema-assets "{\"$schema\":\"http://json-schema.org/draft-04/schema#\",\"type\":\"object\",\"required\":[\"id\",\"name\"],\"properties\":{\"id\":{\"type\":\"number\"},\"name\":{\"type\":\"string\"},\"available\":{\"type\":\"boolean\"}}}\n" :valid t :trace "argv=<-lu><[ORACLE-SANDBOX]/apib-publish-workflow/contracts/Inventory API.apib>\nargv=<-f><json><-u><[ORACLE-SANDBOX]/apib-publish-workflow/contracts/Inventory API.apib>\nargv=<-f><json><-u><[ORACLE-SANDBOX]/apib-publish-workflow/contracts/Inventory API.apib>\nargv=<-f><json><-u><[ORACLE-SANDBOX]/apib-publish-workflow/contracts/Inventory API.apib>\nargv=<-lu>\nstdin=<FORMAT: 1A\nHOST: https://inventory.example.test\n\n# Inventory API\n\n## Product [/products/{id}]\n\n### Retrieve Product [GET]\n\n+ Parameters\n    + id: 42 (number, required)\n\n+ Response 200 (application/json)\n    + Attributes\n        + id: 42 (number)\n        + name: Hammer (string)\n        + available: true (boolean)>\n" :modified nil :disk "FORMAT: 1A\nHOST: https://inventory.example.test\n\n# Inventory API\n\n## Product [/products/{id}]\n\n### Retrieve Product [GET]\n\n+ Parameters\n    + id: 42 (number, required)\n\n+ Response 200 (application/json)\n    + Attributes\n        + id: 42 (number)\n        + name: Hammer (string)\n        + available: true (boolean)\n")"#####
        ]],
    )
}

fn apib_mode_guides_an_author_from_a_drafter_error_back_to_the_broken_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "apib_mode_guides_an_author_from_a_drafter_error_back_to_the_broken_source",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "apib-diagnostic-workflow"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (blueprint
        (expand-file-name
         "contracts/Broken Inventory.apib"
         root))
       (drafter (expand-file-name "tools/drafter" root))
       (trace (expand-file-name "drafter.trace" root))
       (default-directory root)
       buffer
       validation
       destination
       valid
       result)
  (unwind-protect
      (progn
        (neomacs-apib-test-cleanup root)
        (neomacs-apib-test-write-drafter drafter)
        (make-directory (file-name-directory blueprint) t)
        (with-temp-file blueprint
          (insert
           "FORMAT: 1A\n"
           "HOST: https://inventory.example.test\n"
           "\n"
           "# Inventory API\n"
           "\n"
           "## Product [/products/{id}]\n"
           "\n"
           "### Retrieve Product [GET]\n"
           "\n"
           "+ Response 200 (application/json)\n"
           "    + Attributes\n"
           "    + id forty-two (number, required)\n"))
        (setq buffer (find-file-noselect blueprint))
        (switch-to-buffer buffer)
        (let ((apib-drafter-executable drafter)
              (apib-result-buffer "*apib-diagnostic-result*"))
          (apib-mode)
          (save-window-excursion
            (apib-validate)
            (with-current-buffer apib-result-buffer
              (setq validation
                    (list
                     major-mode
                     (buffer-substring-no-properties
                      (point-min)
                      (point-max))))
              (goto-char (point-min))
              (next-error 1 t)
              (let
                  ((source-buffer
                    (window-buffer (selected-window))))
                (with-current-buffer source-buffer
                  (setq destination
                        (list
                         (buffer-name)
                         (file-relative-name
                          buffer-file-name
                          root)
                         (line-number-at-pos)
                         (current-column)
                         (buffer-substring-no-properties
                          (line-beginning-position)
                          (line-end-position)))))))
            (switch-to-buffer buffer)
            (setq valid (apib-valid-p))))
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :validation validation
               :destination destination
               :valid valid
               :trace (neomacs-apib-test-file-string trace)
               :modified (buffer-modified-p)
               :disk
               (neomacs-apib-test-file-string blueprint))))
    (neomacs-apib-test-cleanup root))
  result)
"####,
        expect![[
            r#####"OK (:file "contracts/Broken Inventory.apib" :mode apib-mode :validation (compilation-mode "[ORACLE-SANDBOX]/apib-diagnostic-workflow/tools/drafter -lu [ORACLE-SANDBOX]/apib-diagnostic-workflow/contracts/Broken Inventory.apib\nerror: API description parse error, line 12, column 3 - line 12, column 16\n") :destination ("Broken Inventory.apib" "contracts/Broken Inventory.apib" 12 15 "    + id forty-two (number, required)") :valid nil :trace "argv=<-lu><[ORACLE-SANDBOX]/apib-diagnostic-workflow/contracts/Broken Inventory.apib>\nargv=<-lu>\nstdin=<FORMAT: 1A\nHOST: https://inventory.example.test\n\n# Inventory API\n\n## Product [/products/{id}]\n\n### Retrieve Product [GET]\n\n+ Response 200 (application/json)\n    + Attributes\n    + id forty-two (number, required)>\n" :modified nil :disk "FORMAT: 1A\nHOST: https://inventory.example.test\n\n# Inventory API\n\n## Product [/products/{id}]\n\n### Retrieve Product [GET]\n\n+ Response 200 (application/json)\n    + Attributes\n    + id forty-two (number, required)\n")"#####
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        apib_mode_authors_navigates_and_reviews_a_real_inventory_blueprint(),
        apib_mode_validates_parses_and_exports_assets_from_a_saved_blueprint(),
        apib_mode_guides_an_author_from_a_drafter_error_back_to_the_broken_source(),
    ]
}
