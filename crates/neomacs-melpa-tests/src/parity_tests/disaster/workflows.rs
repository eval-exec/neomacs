use expect_test::expect;

use super::ParityBatchCase;

/// The primary workflow: `M-x disaster' on a plain C file compiles it with
/// the default compiler command, disassembles the object, shows the
/// assembly buffer in asm-mode with non-assembly lines shadowed, and
/// highlights the line matching the current source line while keeping the
/// source buffer selected.
fn the_default_compiler_flow_disassembles_the_line_under_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_default_compiler_flow_disassembles_the_line_under_point",
        r####"(unwind-protect
    (progn
      (disaster--test-open "app.c" disaster--test-app-c)
      (with-current-buffer "app.c"
        (goto-char (point-min))
        (search-forward "return a + b;")
        (beginning-of-line)
        (disaster--test-with-message-capture
         (disaster)
         (let ((assembly (get-buffer disaster-buffer-assembly)))
           (disaster--test-result
            :source (disaster--test-source-state)
            :source-buffer (buffer-name)
            :current-buffer (buffer-name)
            :windows (length (window-list))
            :selected-window (buffer-name (window-buffer (selected-window)))
            :compilation-killed (not (get-buffer disaster-buffer-compiler))
            :assembly-mode (buffer-local-value 'major-mode assembly)
            :assembly-text
            (disaster--test-normalize
             (with-current-buffer assembly
               (buffer-substring-no-properties (point-min) (point-max))))
            :assembly-point (with-current-buffer assembly (point))
            :assembly-overlays
            (with-current-buffer assembly
              (mapcar
               (lambda (ov)
                 (list :face (overlay-get ov 'face)
                       :line (buffer-substring-no-properties
                              (line-beginning-position)
                              (line-end-position))))
               (overlays-in (point-min) (point-max)))))))))
  (disaster--test-reset))"####,
        expect![[
            r#"OK (:source (:upstream-tree "99fc80bd6c76227721f176644bbd4b5d76b2a22f" :feature t :version "20250828.2224") :source-buffer "app.c" :current-buffer "app.c" :windows 2 :selected-window "app.c" :compilation-killed t :assembly-mode asm-mode :assembly-text "\napp.o:     file format elf64-x86-64\n\n\nDisassembly of section .text:\n\n0000000000000000 <add>:\nadd():\n@@DISASTER-RECORD@@/app.c:2\nint add(int a, int b) {\n  return a + b;\n   0:\11lea    (%rdi,%rsi,1),%eax\n@@DISASTER-RECORD@@/app.c:3\n}\n   3:\11xor    %esi,%esi\n   5:\11xor    %edi,%edi\n   7:\11ret\n\nDisassembly of section .text.startup:\n\n0000000000000000 <main>:\nmain():\n@@DISASTER-RECORD@@/app.c:6\n\nint main(void) {\n  return add(2, 3);\n   0:\11mov    $0x3,%esi\n   5:\11mov    $0x2,%edi\n   a:\11jmp    f <main+0xf>\n" :assembly-point 169 :assembly-overlays ((:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face region :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;") (:face shadow :line "  return a + b;")) :messages ("Running: [ORACLE-SANDBOX]/bin/cc -march=native -g -c -o [ORACLE-SANDBOX]/disaster-fixtures/app.o [ORACLE-SANDBOX]/disaster-fixtures/app.c" "(Shell command succeeded with no output)" "Running: [ORACLE-SANDBOX]/bin/objdump [ORACLE-SANDBOX]/disaster-fixtures/app.o") :cc-calls "[-march=native][-g][-c][-o][@@ROOT@@/disaster-fixtures/app.o][@@ROOT@@/disaster-fixtures/app.c]\n" :objdump-calls "[@@ROOT@@/disaster-fixtures/app.o]\n" :make-calls "")"#
        ]],
    )
}

/// In a Makefile project the object is built with `make -k <target>' and
/// the project root is detected through the Makefile marker.
fn the_makefile_workflow_builds_the_object_via_make() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_makefile_workflow_builds_the_object_via_make",
        r####"(unwind-protect
    (progn
      (disaster--test-write
       (expand-file-name "mkproj/.projectile" disaster--test-fixtures)
       "")
      (disaster--test-write
       (expand-file-name "mkproj/Makefile" disaster--test-fixtures)
       "all:\n")
      (disaster--test-open "mkproj/app.c" disaster--test-app-c)
      (with-current-buffer "app.c"
        (goto-char (point-min))
        (search-forward "return a + b;")
        (beginning-of-line)
        (disaster--test-with-message-capture
         (disaster)
         (let ((assembly (get-buffer disaster-buffer-assembly)))
           (disaster--test-result
            :assembly-mode (buffer-local-value 'major-mode assembly)
            :assembly-text
            (disaster--test-normalize
             (with-current-buffer assembly
               (buffer-substring-no-properties (point-min) (point-max)))))))))
  (disaster--test-reset))"####,
        expect![[
            r#"OK (:assembly-mode asm-mode :assembly-text "\napp.o:     file format elf64-x86-64\n\n\nDisassembly of section .text:\n\n0000000000000000 <add>:\nadd():\n@@DISASTER-RECORD@@/app.c:2\nint add(int a, int b) {\n  return a + b;\n   0:\11lea    (%rdi,%rsi,1),%eax\n@@DISASTER-RECORD@@/app.c:3\n}\n   3:\11xor    %esi,%esi\n   5:\11xor    %edi,%edi\n   7:\11ret\n\nDisassembly of section .text.startup:\n\n0000000000000000 <main>:\nmain():\n@@DISASTER-RECORD@@/app.c:6\n\nint main(void) {\n  return add(2, 3);\n   0:\11mov    $0x3,%esi\n   5:\11mov    $0x2,%edi\n   a:\11jmp    f <main+0xf>\n" :messages ("Running: make -k app.o" "(Shell command succeeded with no output)" "Running: [ORACLE-SANDBOX]/bin/objdump [ORACLE-SANDBOX]/disaster-fixtures/mkproj/app.o") :cc-calls "" :objdump-calls "[@@ROOT@@/disaster-fixtures/mkproj/app.o]\n" :make-calls "[-k][app.o]\n")"#
        ]],
    )
}

/// A compile_commands.json entry selects the recorded command verbatim and
/// the object path is parsed out of its `-o' argument.
fn the_compile_commands_database_selects_the_recorded_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_compile_commands_database_selects_the_recorded_command",
        r####"(unwind-protect
    (progn
      (disaster--test-write
       (expand-file-name "cmdproj/.projectile" disaster--test-fixtures)
       "")
      (let ((proj (expand-file-name "cmdproj" disaster--test-fixtures))
            (src (expand-file-name "cmdproj/src/app.c"
                                   disaster--test-fixtures))
            (obj (expand-file-name
                  "cmdproj/build/CMakeFiles/app.dir/src/app.c.o"
                  disaster--test-fixtures)))
        ;; The package reads the database from both the project root
        ;; (root detection) and the recorded directory (command
        ;; selection), like a real CMake layout.
        (let ((json-text
               (format "[{\"directory\": \"%s\", \"file\": \"%s\", \"command\": \"%s -g -c -o %s %s\"}]"
                       (expand-file-name "cmdproj/build"
                                         disaster--test-fixtures)
                       src
                       (expand-file-name "bin/cc" disaster--test-root)
                       obj src)))
          (disaster--test-write
           (expand-file-name "cmdproj/compile_commands.json"
                             disaster--test-fixtures)
           json-text)
          (disaster--test-write
           (expand-file-name "cmdproj/build/compile_commands.json"
                             disaster--test-fixtures)
           json-text))
        (disaster--test-open "cmdproj/src/app.c" disaster--test-app-c)
        (with-current-buffer "app.c"
          (goto-char (point-min))
          (search-forward "return a + b;")
          (beginning-of-line)
          (disaster--test-with-message-capture
           (disaster)
           (let ((assembly (get-buffer disaster-buffer-assembly)))
             (disaster--test-result
              :object-created (file-exists-p obj)
              :assembly-mode (buffer-local-value 'major-mode assembly)
              :assembly-text
              (disaster--test-normalize
               (with-current-buffer assembly
                 (buffer-substring-no-properties (point-min)
                                                 (point-max))))))))))
  (disaster--test-reset))"####,
        expect![[
            r#"OK (:object-created t :assembly-mode asm-mode :assembly-text "\napp.c.o:     file format elf64-x86-64\n\n\nDisassembly of section .text:\n\n0000000000000000 <add>:\nadd():\n@@DISASTER-RECORD@@/app.c:2\nint add(int a, int b) {\n  return a + b;\n   0:\11lea    (%rdi,%rsi,1),%eax\n@@DISASTER-RECORD@@/app.c:3\n}\n   3:\11xor    %esi,%esi\n   5:\11xor    %edi,%edi\n   7:\11ret\n\nDisassembly of section .text.startup:\n\n0000000000000000 <main>:\nmain():\n@@DISASTER-RECORD@@/app.c:6\n\nint main(void) {\n  return add(2, 3);\n   0:\11mov    $0x3,%esi\n   5:\11mov    $0x2,%edi\n   a:\11jmp    f <main+0xf>\n" :messages ("Running: [ORACLE-SANDBOX]/bin/cc -g -c -o [ORACLE-SANDBOX]/disaster-fixtures/cmdproj/build/CMakeFiles/app.dir/src/app.c.o [ORACLE-SANDBOX]/disaster-fixtures/cmdproj/src/app.c" "(Shell command succeeded with no output)" "Running: [ORACLE-SANDBOX]/bin/objdump [ORACLE-SANDBOX]/disaster-fixtures/cmdproj/build/CMakeFiles/app.dir/src/app.c.o") :cc-calls "[-g][-c][-o][@@ROOT@@/disaster-fixtures/cmdproj/build/CMakeFiles/app.dir/src/app.c.o][@@ROOT@@/disaster-fixtures/cmdproj/src/app.c]\n" :objdump-calls "[@@ROOT@@/disaster-fixtures/cmdproj/build/CMakeFiles/app.dir/src/app.c.o]\n" :make-calls "")"#
        ]],
    )
}

/// A failing compiler run shows the command and the compiler's error
/// output in *disaster-compilation* under compilation-mode and never
/// touches the assembly buffer.
fn a_failed_build_shows_the_compiler_output_in_compilation_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_failed_build_shows_the_compiler_output_in_compilation_mode",
        r####"(unwind-protect
    (progn
      (disaster--test-open "app.c" disaster--test-app-c)
      (with-current-buffer "app.c"
        (goto-char (point-min))
        (search-forward "return a + b;")
        (beginning-of-line)
        (setenv "DISASTER_TEST_FAIL" "1")
        (disaster--test-with-message-capture
         (disaster)
         (let ((makebuf (get-buffer disaster-buffer-compiler)))
           (disaster--test-result
            :compilation-mode (buffer-local-value 'major-mode makebuf)
            :compilation-text
            (disaster--test-normalize
             (with-current-buffer makebuf
               (buffer-substring-no-properties (point-min) (point-max))))
            :assembly-text
            (disaster--test-normalize
             (with-current-buffer
                 (get-buffer-create disaster-buffer-assembly)
               (buffer-string))))))))
  (disaster--test-reset))"####,
        expect![[
            r#"OK (:compilation-mode compilation-mode :compilation-text "@@ROOT@@/bin/cc -march=native -g -c -o @@ROOT@@/disaster-fixtures/app.o @@ROOT@@/disaster-fixtures/app.c\napp.c:2:5: error: expected ';' after expression\n  return a + b\n    ^\n1 error generated.\n" :assembly-text "" :messages ("Running: [ORACLE-SANDBOX]/bin/cc -march=native -g -c -o [ORACLE-SANDBOX]/disaster-fixtures/app.o [ORACLE-SANDBOX]/disaster-fixtures/app.c" "app.c:2:5: error: expected ';' after expression\n  return a + b\n    ^\n1 error generated.") :cc-calls "[-march=native][-g][-c][-o][@@ROOT@@/disaster-fixtures/app.o][@@ROOT@@/disaster-fixtures/app.c]\n" :objdump-calls "" :make-calls "")"#
        ]],
    )
}

/// A non C/C++/Fortran file is rejected with the documented user-error.
fn a_non_c_source_signals_the_user_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_non_c_source_signals_the_user_error",
        r####"(unwind-protect
    (progn
      (disaster--test-open "app.txt" "not a c file\n")
      (with-current-buffer "app.txt"
        (let ((caught nil))
          (condition-case err
              (disaster)
            (error (setq caught (list (car err) (cadr err)))))
          (disaster--test-result :error caught))))
  (disaster--test-reset))"####,
        expect![[
            r#"OK (:error (user-error "Not a C, C++ or Fortran source file") :messages nil :cc-calls "" :objdump-calls "" :make-calls "")"#
        ]],
    )
}

/// When the disassembly does not contain the current source line, the
/// documented user-error is signalled.
fn the_assembly_without_the_source_line_signals() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_assembly_without_the_source_line_signals",
        r####"(unwind-protect
    (progn
      (disaster--test-open "app.c" disaster--test-app-c)
      (with-current-buffer "app.c"
        (goto-char (point-min))
        (search-forward "return a + b;")
        (beginning-of-line)
        (setenv "DISASTER_TEST_OBJDUMP_MISS" "1")
        (let ((caught nil))
          (condition-case err
              (disaster)
            (error (setq caught (list (car err) (cadr err)))))
          (disaster--test-result :error caught))))
  (disaster--test-reset))"####,
        expect![[
            r#"OK (:error (user-error "Couldn’t find corresponding assembly line") :messages nil :cc-calls "[-march=native][-g][-c][-o][@@ROOT@@/disaster-fixtures/app.o][@@ROOT@@/disaster-fixtures/app.c]\n" :objdump-calls "[@@ROOT@@/disaster-fixtures/app.o]\n" :make-calls "")"#
        ]],
    )
}

/// The public project-root detection follows the documented marker
/// precedence (under the suite's customized marker list): .projectile
/// beats CMakeLists.txt; a custom LOOKS argument -- a single marker or a
/// paired sublist -- overrides the list; and the parent walk is
/// sandbox-relative.
fn the_project_root_detection_follows_the_marker_precedence() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_project_root_detection_follows_the_marker_precedence",
        r####"(unwind-protect
    (progn
      (let* ((fixtures (directory-file-name disaster--test-fixtures))
             (deep (expand-file-name "root/nested/deep" fixtures))
             (file (expand-file-name "root/nested/deep/app.c" fixtures))
             (dirlist (disaster--find-parent-dirs file)))
        (disaster--test-write file "int x;\n")
        (dolist (marker '(".projectile" "CMakeLists.txt" "Makefile"
                          "setup.py" "package.json"))
          (disaster--test-write
           (expand-file-name (concat "root/" marker) fixtures) ""))
        (let* ((projectile-root
                (disaster--test-normalize
                 (disaster-find-project-root nil file)))
               (after-delete
                (progn
                  (delete-file (expand-file-name "root/.projectile"
                                                 fixtures))
                  (disaster--test-normalize
                   (disaster-find-project-root nil file))))
               (custom-makefile
                (disaster--test-normalize
                 (disaster-find-project-root "Makefile" file)))
               (custom-pair
                (disaster--test-normalize
                 (disaster-find-project-root
                  (list "setup.py" "package.json") file))))
          (list :deep (disaster--test-normalize deep)
                :projectile-root projectile-root
                :after-delete after-delete
                :custom-makefile custom-makefile
                :custom-pair custom-pair
                :walk-tail
                (mapcar (lambda (dir)
                          (file-relative-name dir disaster--test-root))
                        (cl-subseq dirlist 0 4))
                :walk-continues (> (length dirlist) 4)))))
  (disaster--test-reset))"####,
        expect![[
            r#"OK (:deep "@@ROOT@@/disaster-fixtures/root/nested/deep" :projectile-root "@@ROOT@@/disaster-fixtures/root/" :after-delete "@@ROOT@@/disaster-fixtures/root/" :custom-makefile "@@ROOT@@/disaster-fixtures/root/" :custom-pair "@@ROOT@@/disaster-fixtures/root/" :walk-tail ("disaster-fixtures/root/nested/deep/" "disaster-fixtures/root/nested/" "disaster-fixtures/root/" "disaster-fixtures/") :walk-continues t)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        the_default_compiler_flow_disassembles_the_line_under_point(),
        the_makefile_workflow_builds_the_object_via_make(),
        the_compile_commands_database_selects_the_recorded_command(),
        a_failed_build_shows_the_compiler_output_in_compilation_mode(),
        a_non_c_source_signals_the_user_error(),
        the_assembly_without_the_source_line_signals(),
        the_project_root_detection_follows_the_marker_precedence(),
    ]
}
