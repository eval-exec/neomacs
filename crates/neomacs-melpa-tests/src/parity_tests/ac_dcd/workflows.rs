use expect_test::expect;

use super::ParityBatchCase;

/// The documented setup: `(add-to-list 'ac-modes 'd-mode)` plus
/// `(add-hook 'd-mode-hook #'ac-dcd-setup)`.  Opening a D file in a dub project
/// then has to start dcd-server, discover the import paths from the dmd
/// configuration and from `dub describe`, push them to dcd-client, register
/// `ac-source-dcd` and bind the four ac-dcd keys.
fn setup_starts_the_server_sends_discovered_imports_and_binds_the_dcd_keys() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_starts_the_server_sends_discovered_imports_and_binds_the_dcd_keys",
        r##"(let* ((project (expand-file-name "my project/" ac-dcd-test-root))
       (source (ac-dcd-test-source
                "my project/source/app.d"
                (concat "module app;\n"
                        "import std.stdio;\n"
                        "void main() {\n"
                        "    writeln(\"hallo\");\n"
                        "}\n")))
       (buffer nil))
  (ac-dcd-test-install-tools)
  (ac-dcd-test-write-file
   (expand-file-name "dmd.conf" (getenv "HOME"))
   (concat "[Environment]\n"
           "DFLAGS=-I/usr/include/dmd/phobos -I/usr/include/dmd/druntime/import -L-L/usr/lib\n"))
  (ac-dcd-test-write-file
   (expand-file-name "dub.json" project)
   "{\"name\":\"my-project\",\"dependencies\":{\"cerealed\":\"~master\"}}\n")
  (ac-dcd-test-write-file
   (expand-file-name "dub-describe.json" ac-dcd-test-replies)
   (concat "Performing \"debug\" build.\n"
           "{\"packages\":["
           "{\"path\":\"" project "\",\"importPaths\":[\"source\"]},"
           "{\"path\":\"" (expand-file-name "vendor/cerealed/" ac-dcd-test-root)
           "\",\"importPaths\":[\"source\",\"extra\"]}"
           "]}\n"))
  (ac-dcd-test-reply "imports" "")
  (add-to-list 'ac-modes 'd-mode)
  (add-hook 'd-mode-hook #'ac-dcd-setup)
  (unwind-protect
      (progn
        (setq buffer (find-file-noselect source))
        (set-window-buffer (selected-window) buffer)
        (set-buffer buffer)
        (list
         (ac-dcd-test-server-calls)
         (ac-dcd-test-dub-calls)
         (ac-dcd-test-client-calls)
         (list major-mode
               auto-complete-mode
               ac-sources
               (and (memq 'd-mode ac-modes) t)
               (and (process-live-p (get-process "dcd-server")) t))
         (mapcar (lambda (key) (lookup-key d-mode-map (kbd key)))
                 '("C-c ?" "C-c ." "C-c ," "C-c s"))))
    (remove-hook 'd-mode-hook #'ac-dcd-setup)
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))
    (ac-dcd-test-cleanup)))"##,
        expect![[
            r#"OK ((("-p" "9166")) (("describe")) ((("--tcp" "-I/usr/include/dmd/phobos" "-I/usr/include/dmd/druntime/import" "-I[ORACLE-SANDBOX]/my project/source" "-I[ORACLE-SANDBOX]/vendor/cerealed/source" "-I[ORACLE-SANDBOX]/vendor/cerealed/extra") . "module app;\nimport std.stdio;\nvoid main() {\n    writeln(\"hallo\");\n}\n")) (d-mode t (ac-source-dcd ac-source-words-in-same-mode-buffers) t t) (ac-dcd-show-ddoc-with-buffer ac-dcd-goto-definition ac-dcd-goto-def-pop-marker ac-dcd-search-symbol))"#
        ]],
    )
}

fn completes_a_struct_member_through_auto_complete_and_inserts_the_choice() -> ParityBatchCase {
    ParityBatchCase::value(
        "completes_a_struct_member_through_auto_complete_and_inserts_the_choice",
        r##"(let ((source (ac-dcd-test-source
               "my project/source/app.d"
               (concat "module app;\n"
                       "import std.stdio;\n"
                       "\n"
                       "void main() {\n"
                       "    auto f = File(\"data.txt\");\n"
                       "    f.\n"
                       "}\n"))))
  (ac-dcd-test-install-tools)
  (ac-dcd-test-reply
   "complete"
   (concat "identifiers\n"
           "byLine\tf\n"
           "byLine\tf\n"
           "name\tm\n"
           "isOpen\tm\n"
           "Pattern\tk\n"
           "rawWrite\tf\n"))
  (ac-dcd-test-in-source
   source
   (goto-char (point-min))
   (search-forward "    f.")
   (ac-start :force-init t)
   (ac-update t)
   (let ((candidates ac-candidates)
         (prefix ac-prefix))
     (ac-complete)
     (list
      (ac-dcd-test-client-calls)
      (list prefix (point) (line-number-at-pos))
      (mapcar #'substring-no-properties candidates)
      (mapcar (lambda (c) (get-text-property 0 'ac-dcd-help c)) candidates)
      (mapcar #'ac-dcd-document candidates)
      (ac-dcd-test-buffer-text ac-dcd-output-buffer-name)
      (buffer-substring-no-properties (point-min) (point-max))
      (point)))))"##,
        expect![[
            r#"OK (((("--tcp" "-c" "83" "-p" "9166") . "module app;\nimport std.stdio;\n\nvoid main() {\n    auto f = File(\"data.txt\");\n    f.\n}\n")) ("" 87 6) ("name" "isOpen" "byLine" "rawWrite") ("m" "m" "f\nf" "f") ("member variable name" "member variable name" "candidate kind undetected: f\nf" "function or method") "identifiers\nbyLine\11f\nbyLine\11f\nname\11m\nisOpen\11m\nPattern\11k\nrawWrite\11f\n" "module app;\nimport std.stdio;\n\nvoid main() {\n    auto f = File(\"data.txt\");\n    f.name\n}\n" 87)"#
        ]],
    )
    .fresh_process()
}

fn queries_dcd_at_the_identifier_start_in_bytes_and_never_from_comments_or_strings()
-> ParityBatchCase {
    ParityBatchCase::value(
        "queries_dcd_at_the_identifier_start_in_bytes_and_never_from_comments_or_strings",
        r##"(let ((source (ac-dcd-test-source
               "my project/source/app.d"
               (concat "module app;\n"
                       "import std.stdio;\n"
                       "\n"
                       "void main() {\n"
                       "    writeln(\"Grüße, Welt — 日本語\");\n"
                       "    // TODO: write more\n"
                       "    writ\n"
                       "}\n"))))
  (ac-dcd-test-install-tools)
  (ac-dcd-test-reply
   "complete"
   (concat "identifiers\n"
           "writeln\tf\n"
           "writefln\tf\n"
           "writef\tf\n"
           "File\ts\n"
           "stdin\tv\n"))
  (ac-dcd-test-in-source
   source
   (let* ((identifier-point (progn (goto-char (point-min))
                                   (search-forward "    writ\n")
                                   (1- (point))))
          (string-point (progn (goto-char (point-min))
                               (search-forward "Grüße")
                               (point)))
          (comment-point (progn (goto-char (point-min))
                                (search-forward "// TODO: writ")
                                (point)))
          (identifier (ac-dcd-test-complete-at identifier-point))
          (prefix ac-prefix)
          (in-string (ac-dcd-test-complete-at string-point))
          (in-comment (ac-dcd-test-complete-at comment-point)))
     (list
      (list identifier-point (position-bytes identifier-point) prefix identifier)
      (list string-point (nth 8 (syntax-ppss string-point)) in-string)
      (list comment-point (nth 8 (syntax-ppss comment-point)) in-comment)
      (ac-dcd-test-client-calls)))))"##,
        expect![[
            r#"OK ((112 122 "writ" ("writef" "writeln" "writefln")) (64 58 nil) (97 84 nil) ((("--tcp" "-c" "118" "-p" "9166") . "module app;\nimport std.stdio;\n\nvoid main() {\n    writeln(\"Grüße, Welt — 日本語\");\n    // TODO: write more\n    writ\n}\n")))"#
        ]],
    )
    .fresh_process()
}

fn shows_the_ddoc_for_the_symbol_at_point_and_reports_undocumented_symbols() -> ParityBatchCase {
    ParityBatchCase::value(
        "shows_the_ddoc_for_the_symbol_at_point_and_reports_undocumented_symbols",
        r##"(let ((source (ac-dcd-test-source
               "my project/source/app.d"
               (concat "module app;\n"
                       "import std.stdio;\n"
                       "\n"
                       "void main() {\n"
                       "    writeln(\"hallo\");\n"
                       "}\n"))))
  (ac-dcd-test-install-tools)
  (ac-dcd-test-reply
   "doc"
   (concat "Writes its arguments to stdout.\\n"
           "\\nParams:\\n"
           "    args = die Grüße\\n"
           "\\nLiteral in D source: a\\\\nb\\n"))
  (ac-dcd-test-in-source
   source
   (goto-char (point-min))
   (search-forward "    writeln(\"hallo\");")
   (insert "  // frisch getippt")
   (goto-char (point-min))
   (search-forward "    writeln")
   (let ((modified-before (buffer-modified-p))
         (point-before (point)))
     (call-interactively 'ac-dcd-show-ddoc-with-buffer)
     (let ((found
            (list modified-before
                  (buffer-modified-p)
                  (list point-before (point))
                  (ac-dcd-test-file-contents source)
                  (ac-dcd-test-buffer-text ac-dcd-document-buffer-name)
                  (with-current-buffer ac-dcd-document-buffer-name (point))
                  (ac-dcd-test-displayed-buffers)
                  (ac-dcd-test-client-calls))))
       (ac-dcd-test-reply "doc" "\n\n\n")
       (list
        found
        (condition-case error
            (progn (call-interactively 'ac-dcd-show-ddoc-with-buffer)
                   'unexpectedly-succeeded)
          (error (list (car error) (cdr error))))
        (length (ac-dcd-test-client-calls)))))))"##,
        expect![[
            r#"OK ((t nil (57 57) "module app;\nimport std.stdio;\n\nvoid main() {\n    writeln(\"hallo\");  // frisch getippt\n}\n" "Writes its arguments to stdout.\n\nParams:\n    args = die Grüße\n\nLiteral in D source: a\\nb\n" 1 ("*dcd-document*" "app.d") ((("--tcp" "-c" "57" "-p" "9166" "-d" "[ORACLE-SANDBOX]/my project/source/app.d") . ""))) (error ("No document for the symbol at point!")) 2)"#
        ]],
    )
    .fresh_process()
}

fn goto_definition_jumps_across_files_by_byte_offset_and_the_marker_ring_returns() -> ParityBatchCase
{
    ParityBatchCase::value(
        "goto_definition_jumps_across_files_by_byte_offset_and_the_marker_ring_returns",
        r##"(let* ((source (ac-dcd-test-source
                "my project/source/app.d"
                (concat "module app;\n"
                        "import std.stdio;\n"
                        "\n"
                        "void gruessen() {\n"
                        "    writeln(\"Grüße\");\n"
                        "}\n"
                        "\n"
                        "void main() {\n"
                        "    gruessen();\n"
                        "}\n")))
       (target (ac-dcd-test-source
                "vendor/phobos/std/stdio.d"
                (concat "module std.stdio;\n"
                        "// Grüße aus Phobos — hier kommt die Ausgabe\n"
                        "void writeln(T...)(T args) { }\n")))
       (target-offset
        (with-temp-buffer
          (insert-file-contents target)
          (goto-char (point-min))
          (search-forward "void ")
          (number-to-string (1- (position-bytes (point))))))
       (local-offset
        (with-temp-buffer
          (insert-file-contents source)
          (goto-char (point-min))
          (search-forward "void gruessen")
          (search-backward "gruessen")
          (number-to-string (1- (position-bytes (point))))))
       (visited nil))
  (ac-dcd-test-install-tools)
  (ac-dcd-test-reply "location" (concat target "\t" target-offset "\n"))
  (unwind-protect
      (ac-dcd-test-in-source
       source
       (goto-char (point-min))
       (search-forward "    writeln")
       (let ((origin (point)))
         (call-interactively 'ac-dcd-goto-definition)
         (setq visited (current-buffer))
         (let ((jumped
                (list (buffer-name)
                      (file-relative-name (buffer-file-name) ac-dcd-test-root)
                      (point)
                      (buffer-substring-no-properties (point) (+ (point) 7))
                      (ring-length ac-dcd-goto-definition-marker-ring))))
           (call-interactively 'ac-dcd-goto-def-pop-marker)
           (let ((returned (list (buffer-name) (point) origin
                                 (ring-length ac-dcd-goto-definition-marker-ring))))
             (ac-dcd-test-reply "location" "Not found\n")
             (goto-char (point-min))
             (search-forward "    gruessen();")
             (call-interactively 'ac-dcd-goto-definition)
             (let ((missing (list (buffer-name) (point)
                                  (ac-dcd-test-last-message)
                                  (ring-length ac-dcd-goto-definition-marker-ring))))
               (ac-dcd-test-reply "location" (concat "stdin\t" local-offset "\n"))
               (call-interactively 'ac-dcd-goto-definition)
               (list jumped
                     returned
                     missing
                     (list (buffer-name) (point)
                           (buffer-substring-no-properties (point) (+ (point) 8))
                           (ring-length ac-dcd-goto-definition-marker-ring))
                     (ac-dcd-test-client-calls)))))))
    (when (buffer-live-p visited)
      (with-current-buffer visited (set-buffer-modified-p nil))
      (kill-buffer visited))))"##,
        expect![[
            r#"OK (("stdio.d" "vendor/phobos/std/stdio.d" 69 "writeln" 1) ("app.d" 61 61 0) ("app.d" 104 "Not found" 0) ("app.d" 37 "gruessen" 1) ((("--tcp" "-c" "61" "-p" "9166" "-l" "[ORACLE-SANDBOX]/my project/source/app.d") . "") (("--tcp" "-c" "106" "-p" "9166" "-l" "[ORACLE-SANDBOX]/my project/source/app.d") . "") (("--tcp" "-c" "106" "-p" "9166" "-l" "[ORACLE-SANDBOX]/my project/source/app.d") . "")))"#
        ]],
    )
    .fresh_process()
}

fn search_symbol_lists_matches_and_visits_a_single_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "search_symbol_lists_matches_and_visits_a_single_match",
        r##"(let* ((source (ac-dcd-test-source
                "my project/source/app.d"
                (concat "module app;\n"
                        "import std.stdio;\n"
                        "\n"
                        "void main() {\n"
                        "    writeln(\"Grüße\");\n"
                        "}\n")))
       (stdio (ac-dcd-test-source
               "vendor/phobos/std/stdio.d"
               (concat "module std.stdio;\n"
                       "// Grüße aus Phobos — hier kommt die Ausgabe\n"
                       "void writeln(T...)(T args) { }\n")))
       (file-d (ac-dcd-test-source
                "vendor/phobos/std/file.d"
                "module std.file;\nvoid writeln() { }\n"))
       (visited nil))
  (ac-dcd-test-install-tools)
  (ac-dcd-test-reply
   "search"
   (concat stdio "\tf\t68\n" file-d "\tf\t22\n"))
  (unwind-protect
      (ac-dcd-test-in-source
       source
       (goto-char (point-min))
       (search-forward "    writeln")
       (call-interactively 'ac-dcd-search-symbol)
       (let ((listed
              (list (ac-dcd-test-buffer-text ac-dcd-search-symbol-buffer-name)
                    (with-current-buffer ac-dcd-search-symbol-buffer-name
                      (list (point)
                            (lookup-key (current-local-map) "q")
                            (lookup-key (current-local-map) (kbd "RET"))))
                    (ac-dcd-test-displayed-buffers)
                    (ac-dcd-test-client-calls))))
         (ac-dcd-test-reply "search" (concat stdio "\tf\t68\n"))
         (set-buffer (get-file-buffer source))
         (goto-char (point-min))
         (search-forward "    writeln")
         (call-interactively 'ac-dcd-search-symbol)
         (setq visited (window-buffer (selected-window)))
         (list
          listed
          (ac-dcd-test-buffer-text ac-dcd-search-symbol-buffer-name)
          (list (buffer-name) (buffer-name visited))
          (with-current-buffer visited
            (list (file-relative-name (buffer-file-name) ac-dcd-test-root)
                  (point)
                  (buffer-substring-no-properties (point) (line-end-position))
                  (lookup-key (current-local-map) (kbd "C-c <left>"))))
          (length (ac-dcd-test-client-calls)))))
    (when (buffer-live-p visited)
      (with-current-buffer visited (set-buffer-modified-p nil))
      (kill-buffer visited))))"##,
        expect![[
            r#"OK (("[ORACLE-SANDBOX]/vendor/phobos/std/stdio.d\11f\01168\n[ORACLE-SANDBOX]/vendor/phobos/std/file.d\11f\01122" (1 delete-window ac-dcd-visit-file-in-line) ("*dcd-search-symbol*" "app.d") ((("--tcp" "--search" "writeln") . ""))) "[ORACLE-SANDBOX]/vendor/phobos/std/stdio.d\11f\01168" ("app.d" "stdio.d") ("vendor/phobos/std/stdio.d" 68 " writeln(T...)(T args) { }" (lambda nil (interactive) (switch-to-buffer (get-buffer-create ac-dcd-search-symbol-buffer-name)))) 2)"#
        ]],
    )
    .fresh_process()
}

fn a_failing_or_missing_dcd_client_reports_through_the_error_buffer_and_echo_area()
-> ParityBatchCase {
    ParityBatchCase::value(
        "a_failing_or_missing_dcd_client_reports_through_the_error_buffer_and_echo_area",
        r##"(let ((source (ac-dcd-test-source
               "my project/source/app.d"
               (concat "module app;\n"
                       "import std.stdio;\n"
                       "\n"
                       "void main() {\n"
                       "    writ\n"
                       "}\n"))))
  (ac-dcd-test-install-tools)
  (ac-dcd-test-reply
   "complete"
   "dcd-client: Error: Could not connect to the server: Connection refused\n"
   1)
  (ac-dcd-test-in-source
   source
   (goto-char (point-min))
   (search-forward "    writ")
   (let* ((candidates (ac-dcd-test-complete-at (point)))
          (error-text (ac-dcd-test-buffer-text ac-dcd-error-buffer-name))
          (error-lines (split-string error-text "\n"))
          (failed
           (list candidates
                 (if (string-match-p
                      "\\`[A-Z][a-z][a-z] [A-Z][a-z][a-z] [ 0-9][0-9] [0-9][0-9]:[0-9][0-9]:[0-9][0-9] [0-9][0-9][0-9][0-9]\\'"
                      (car error-lines))
                     'current-time-string
                   (car error-lines))
                 (cdr error-lines)
                 (with-current-buffer ac-dcd-error-buffer-name (point))
                 (ac-dcd-test-buffer-text ac-dcd-output-buffer-name)
                 (ac-dcd-test-displayed-buffers)
                 (ac-dcd-test-client-calls))))
     (let ((ac-dcd-executable nil))
       (list
        failed
        (ac-dcd-test-complete-at (point))
        (ac-dcd-test-last-message)
        (length (ac-dcd-test-client-calls)))))))"##,
        expect![[
            r#"OK ((nil current-time-string ("\"[ORACLE-SANDBOX]/bin/dcd-client --tcp -c 50 -p 9166\" failed." "Error type is: Could not connect to the server : Connection refused" "") 1 "dcd-client: Error: Could not connect to the server: Connection refused\n" ("*dcd-error*" "app.d") ((("--tcp" "-c" "50" "-p" "9166") . "module app;\nimport std.stdio;\n\nvoid main() {\n    writ\n}\n"))) nil "ac-dcd error: could not find dcd-client executable" 1)"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        setup_starts_the_server_sends_discovered_imports_and_binds_the_dcd_keys(),
        completes_a_struct_member_through_auto_complete_and_inserts_the_choice(),
        queries_dcd_at_the_identifier_start_in_bytes_and_never_from_comments_or_strings(),
        shows_the_ddoc_for_the_symbol_at_point_and_reports_undocumented_symbols(),
        goto_definition_jumps_across_files_by_byte_offset_and_the_marker_ring_returns(),
        search_symbol_lists_matches_and_visits_a_single_match(),
        a_failing_or_missing_dcd_client_reports_through_the_error_buffer_and_echo_area(),
    ]
}
