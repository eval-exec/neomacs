use expect_test::expect;

use super::ParityBatchCase;

fn ascii_table_exact_pin_descriptor_dependency_origin_and_feature_contract_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ascii_table_exact_pin_descriptor_dependency_origin_and_feature_contract_match",
        r##"(let ((descriptor
                (cadr
                 (assq
                  'ascii-table
                  package-alist))))
         (list
          (package-desc-name descriptor)
          (package-version-join
           (package-desc-version descriptor))
          (package-desc-summary descriptor)
          (package-desc-kind descriptor)
          (package-desc-reqs descriptor)
          (package-desc-extras descriptor)
          (featurep 'ascii-table)))"##,
        expect![[
            r#"OK (ascii-table "20231215.1527" "Interactive ASCII table." nil ((emacs (24 3))) ((:maintainers ("Lassi Kortela" . "lassi@lassi.io")) (:authors ("Lassi Kortela" . "lassi@lassi.io")) (:keywords "help" "tools") (:revdesc . "dc3c91feff62") (:commit . "dc3c91feff6282303b66816bdcee9e031558ff77") (:url . "https://github.com/lassik/emacs-ascii-table")) t)"#
        ]],
    )
}

fn ascii_table_installed_payload_inventory_sizes_and_content_digests_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_installed_payload_inventory_sizes_and_content_digests_match",
        r##"(let* ((descriptor
                  (cadr
                   (assq 'ascii-table package-alist)))
                 (directory
                  (package-desc-dir descriptor)))
         (mapcar
          (lambda (file)
            (let ((path
                   (expand-file-name file directory)))
              (list
               file
               (file-attribute-size
                (file-attributes path))
               (with-temp-buffer
                 (insert-file-contents-literally path)
                 (secure-hash
                  'sha256
                  (current-buffer))))))
          (sort
           (seq-filter
            (lambda (file)
              (file-regular-p
               (expand-file-name file directory)))
            (directory-files directory nil "\\`[^.]"))
           #'string<)))"##,
        expect![[
            r#"OK (("ascii-table-autoloads.el" 796 "aeb50ebef24754a49da0510c41a5e94c3a8aa3012c4496a9174b510aed0081d5") ("ascii-table-pkg.el" 416 "3d55cf0d7d4b3fea3212f024153b31c1c05cacc94ecdad4c5e1f7ae06c000aff") ("ascii-table.el" 10566 "8e20cb770d349783841fbcc2148c966c150d3ef028effa1c72848e17ac344b45") ("ascii-table.elc" 10565 "56612a442ca393462fadf6772a5ef127e827c0a2a15a6611847a78d13d6ae534"))"#
        ]],
    )
}

fn ascii_table_complete_callable_command_arglist_doc_and_source_surface_matches() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ascii_table_complete_callable_command_arglist_doc_and_source_surface_matches",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (fboundp symbol)
            (commandp symbol)
            (interactive-form symbol)
            (help-function-arglist symbol t)
            (let ((doc (documentation symbol t)))
              (and doc (secure-hash 'sha256 doc)))
            (let ((file (symbol-file symbol 'defun)))
              (and file (file-name-nondirectory file)))))
         '(ascii-table--binary
           ascii-table--class-face
           ascii-table--character-class
           ascii-table--control-caret
           ascii-table--control-name
           ascii-table--control-escape
           ascii-table--table
           ascii-table--column-widths
           ascii-table--width-limit
           ascii-table--revert
           ascii-table--revert-if-active
           ascii-table--set-base
           ascii-table-toggle-control
           ascii-table-toggle-escape
           ascii-table-base-binary
           ascii-table-base-octal
           ascii-table-base-decimal
           ascii-table-base-hex
           ascii-table-mode
           ascii-table))"##,
        expect![[
            r#"OK ((ascii-table--binary t nil nil (codepoint) "901598a7aa9ef036ae78219483c60cc460fb3cba7d1bccf07df37676689baae5" "ascii-table.el") (ascii-table--class-face t nil nil (class) "bc3e7d2e04a27abd602a271ac003d268e141ac3a3a754db6ddc1d27fe5161115" "ascii-table.el") (ascii-table--character-class t nil nil (codepoint) "6bc7de041a693296bf06e225997d063878043b7e4115826317133d662cdb2147" "ascii-table.el") (ascii-table--control-caret t nil nil (codepoint) "8015b824fc847009321238adbdcdfbee1f274f899c32f8c416bd42753a294bfc" "ascii-table.el") (ascii-table--control-name t nil nil (codepoint) "eb304c2d6e188362d2b61c479aadea7ebfc869b3f458adac44bec007addb2d3a" "ascii-table.el") (ascii-table--control-escape t nil nil (codepoint) "fd46c2e6dc98b1f4da99997d11997823f2ce927f3293b16032014f638f210d20" "ascii-table.el") (ascii-table--table t nil nil (codepoints/row) "fe83179db22119b37a3ac38e009d60fb2ff97c04e1ff7c66650bf11a8e6f9974" "ascii-table.el") (ascii-table--column-widths t nil nil (table cols) "957ee2740547ebd345a1f948e06852ee38709dd077db629ee7b2e4811e620590" "ascii-table.el") (ascii-table--width-limit t nil nil nil "7f6cd1c4dafc351574a0c314dec6fb754bdf9a597b0cc887b2f1e15bc00fac51" "ascii-table.el") (ascii-table--revert t nil nil (&optional _arg _noconfirm) "1311cde773426735d40c5942e087226b17384bfb119ecf47608f1a7393af65ad" "ascii-table.el") (ascii-table--revert-if-active t nil nil nil "db8d877bef297ab69852f35a35fc2c00e32dc49715c6a014df7ed6aa5475d57f" "ascii-table.el") (ascii-table--set-base t nil nil (base) "e6eafe8d8d419677b1e9e0399e0a9f699ff9039d9d8abdc5d492f0f7a0d93f91" "ascii-table.el") (ascii-table-toggle-control t t (interactive nil) nil "8be305f795ad5456205d113538a9defe36437b7bb13b158ff2a420559091d29d" "ascii-table.el") (ascii-table-toggle-escape t t (interactive nil) nil "66a441ae6e58338d37b699a31e069b3661b4607c0d73053a146aa7e6479eddfd" "ascii-table.el") (ascii-table-base-binary t t (interactive nil) nil "411ce2335c5dcd10f3d5e47fa6b727e81ac8921a91c2885ab65ffa4b7156e606" "ascii-table.el") (ascii-table-base-octal t t (interactive nil) nil "4772c6a01882788c2afefb8480a506af4f5a836cf8028d45bd5444514396da4d" "ascii-table.el") (ascii-table-base-decimal t t (interactive nil) nil "94ef23d08084456052ffc16d58022c224ca88d299891384cb5083ab8a9fc15b6" "ascii-table.el") (ascii-table-base-hex t t (interactive nil) nil "787968e351d75b7332864c97c0c5fec575c9c854f77f82afc21b2d875c9281bd" "ascii-table.el") (ascii-table-mode t t (interactive nil) nil "c13717a0f587c165b05e3aab349eadc9bcd3d92f318f429f91d99673c60138b0" "ascii-table.el") (ascii-table t t (interactive nil) nil "c964f51383dda80bcf0efe0f81f073cc13b8e965a5d95a4475c7c911fd298ee2" "ascii-table.el"))"#
        ]],
    )
}

fn ascii_table_variables_defaults_mutability_scope_docs_and_source_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_variables_defaults_mutability_scope_docs_and_source_match",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (symbol-value symbol)
            (default-value symbol)
            (default-boundp symbol)
            (special-variable-p symbol)
            (local-variable-if-set-p symbol)
            (custom-variable-p symbol)
            (get symbol 'custom-type)
            (get symbol 'custom-group)
            (let ((doc
                   (documentation-property
                    symbol
                    'variable-documentation
                    t)))
              (and doc (secure-hash 'sha256 doc)))
            (let ((file (symbol-file symbol 'defvar)))
              (and file (file-name-nondirectory file)))))
         '(ascii-table-base
           ascii-table-control
           ascii-table-escape
           ascii-table-mode-map
           ascii-table-mode-hook))"##,
        expect![[
            r#"OK ((ascii-table-base 16 16 t t nil nil nil nil "41b8bc95291f07110b6d0fc1a666dcb2d0798aa9fa84f9bc086c626fd333cf54" "ascii-table.el") (ascii-table-control nil nil t t nil nil nil nil "59cfec03082db714f6516730c30588b4e272315d0ce728da1e93e955877d47b8" "ascii-table.el") (ascii-table-escape nil nil t t nil nil nil nil "ce37cead75dd1d0c4b2012c52e660986edbc5c0a1dcd9088e1f8f41f49d160cf" "ascii-table.el") (ascii-table-mode-map #1=(keymap (9 . ascii-table-toggle-control) (120 . ascii-table-base-hex) (111 . ascii-table-base-octal) (100 . ascii-table-base-decimal) (101 . ascii-table-toggle-escape) (98 . ascii-table-base-binary) keymap (103 . revert-buffer) (60 . beginning-of-buffer) (62 . end-of-buffer) (104 . describe-mode) (63 . describe-mode) (127 . scroll-down-command) (33554464 . scroll-down-command) (32 . scroll-up-command) (113 . quit-window) (57 . digit-argument) (56 . digit-argument) (55 . digit-argument) (54 . digit-argument) (53 . digit-argument) (52 . digit-argument) (51 . digit-argument) (50 . digit-argument) (49 . digit-argument) (48 . digit-argument) (45 . negative-argument) (remap keymap (self-insert-command . undefined))) #1# t t nil nil nil nil "c1143521cab9d0250f8feecff1314588e821e301d675b7f0f006de2572f8224f" "ascii-table.el") (ascii-table-mode-hook nil nil t t nil nil nil nil "9d311d7f8ed4f42e938d8e7fe3ad7dbc4f948d38364d73d9c32f0a653dfe579c" "ascii-table.el"))"#
        ]],
    )
}

fn ascii_table_mode_map_parent_bindings_commands_and_reverse_lookup_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_mode_map_parent_bindings_commands_and_reverse_lookup_match",
        r##"(list
         (keymapp ascii-table-mode-map)
         (eq
          (keymap-parent ascii-table-mode-map)
          special-mode-map)
         (mapcar
          (lambda (key)
            (list
             key
             (lookup-key
              ascii-table-mode-map
              (kbd key))
             (with-temp-buffer
               (use-local-map ascii-table-mode-map)
               (key-binding (kbd key) t))))
          '("b" "e" "d" "o" "x" "TAB"
            "q" "g" "n" "p" "<mouse-2>" "z"))
         (mapcar
          (lambda (command)
            (list
             command
             (mapcar
              #'key-description
              (where-is-internal
               command
               ascii-table-mode-map))))
          '(ascii-table-base-binary
            ascii-table-toggle-escape
            ascii-table-base-decimal
            ascii-table-base-octal
            ascii-table-base-hex
            ascii-table-toggle-control
            revert-buffer
            quit-window)))"##,
        expect![[
            r#"OK (t t (("b" ascii-table-base-binary ascii-table-base-binary) ("e" ascii-table-toggle-escape ascii-table-toggle-escape) ("d" ascii-table-base-decimal ascii-table-base-decimal) ("o" ascii-table-base-octal ascii-table-base-octal) ("x" ascii-table-base-hex ascii-table-base-hex) ("TAB" ascii-table-toggle-control ascii-table-toggle-control) ("q" quit-window quit-window) ("g" revert-buffer revert-buffer) ("n" nil undefined) ("p" nil undefined) ("<mouse-2>" nil mouse-yank-primary) ("z" nil undefined)) ((ascii-table-base-binary ("b")) (ascii-table-toggle-escape ("e")) (ascii-table-base-decimal ("d")) (ascii-table-base-octal ("o")) (ascii-table-base-hex ("x")) (ascii-table-toggle-control ("TAB")) (revert-buffer ("g" "<menu-bar> <file> <revert-buffer>")) (quit-window ("q" "C-x w q"))))"#
        ]],
    )
}

fn ascii_table_derived_mode_registry_and_initial_buffer_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_derived_mode_registry_and_initial_buffer_contract_match",
        r##"(let ((buffer
                (generate-new-buffer
                 " *ascii-table-registry*")))
         (unwind-protect
             (with-current-buffer buffer
               (let ((ascii-table-base 16))
                 (ascii-table-mode))
               (list
                major-mode
                mode-name
                (derived-mode-p 'ascii-table-mode)
                (derived-mode-p 'special-mode)
                (eq
                 (current-local-map)
                 ascii-table-mode-map)
                buffer-read-only
                truncate-lines
                revert-buffer-function
                (local-variable-p
                 'revert-buffer-function)
                (buffer-file-name)
                (point)
                (buffer-substring-no-properties
                 (point-min)
                 (line-end-position 2))))
           (kill-buffer buffer)))"##,
        expect![[
            r#"OK (ascii-table-mode "ASCII" ascii-table-mode special-mode t t nil ascii-table--revert t nil 1 "ASCII Table (hex)\n")"#
        ]],
    )
}

fn ascii_table_generated_autoload_registers_public_command_without_loading_feature()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_generated_autoload_registers_public_command_without_loading_feature",
        r##"(list
         (featurep 'ascii-table)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (and
              (fboundp symbol)
              (autoloadp
               (symbol-function symbol)))
             (commandp symbol)
             (interactive-form symbol)
             (let ((file
                    (symbol-file symbol 'defun)))
               (and file (file-name-nondirectory file)))))
          '(ascii-table
            ascii-table-mode
            ascii-table-base-hex
            ascii-table-toggle-control))
         (memq
          'ascii-table
          package-activated-list))"##,
        expect![[
            r#"OK (nil ((ascii-table t t t (interactive nil) "ascii-table.el") (ascii-table-mode t nil t (interactive nil) "ascii-table.el") (ascii-table-base-hex t nil t (interactive nil) "ascii-table.el") (ascii-table-toggle-control t nil t (interactive nil) "ascii-table.el")) (ascii-table))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn registry_ascii_table_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ascii_table_exact_pin_descriptor_dependency_origin_and_feature_contract_match(),
        ascii_table_installed_payload_inventory_sizes_and_content_digests_match(),
        ascii_table_complete_callable_command_arglist_doc_and_source_surface_matches(),
        ascii_table_variables_defaults_mutability_scope_docs_and_source_match(),
        ascii_table_mode_map_parent_bindings_commands_and_reverse_lookup_match(),
        ascii_table_derived_mode_registry_and_initial_buffer_contract_match(),
    ]
}

pub(super) fn registry_ascii_table_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![ascii_table_generated_autoload_registers_public_command_without_loading_feature()]
}
