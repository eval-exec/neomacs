use expect_test::expect;

use super::ParityBatchCase;

fn auto_compile_recursive_start_compiles_real_libraries_and_skips_hidden_and_nosearch_trees()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_recursive_start_compiles_real_libraries_and_skips_hidden_and_nosearch_trees",
        r##"(let* ((root
                 (auto-compile-test-path
                  "recursive-start/"))
                (top
                 (auto-compile-test-write
                  "recursive-start/top.el"
                  "(provide 'auto-compile-top)\n"))
                (nested
                 (auto-compile-test-write
                  "recursive-start/lib/nested.el"
                  "(provide 'auto-compile-nested)\n"))
                (hidden
                 (auto-compile-test-write
                  "recursive-start/.hidden/hidden.el"
                  "(provide 'auto-compile-hidden)\n"))
                (nosearch
                 (auto-compile-test-write
                  "recursive-start/vendor/skipped.el"
                  "(provide 'auto-compile-skipped)\n")))
         (auto-compile-test-write
          "recursive-start/vendor/.nosearch"
          "")
         (toggle-auto-compile root 'start)
         (mapcar
          (lambda (source)
            (list
             (file-relative-name source root)
             (file-exists-p
              (auto-compile-test-dest source))))
          (list top nested hidden nosearch)))"##,
        expect![[
            r#"OK (("top.el" t) ("lib/nested.el" t) (".hidden/hidden.el" nil) ("vendor/skipped.el" nil))"#
        ]],
    )
}

fn auto_compile_recursive_quit_removes_regular_and_stray_destinations_but_respects_skipped_trees()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_recursive_quit_removes_regular_and_stray_destinations_but_respects_skipped_trees",
        r##"(let* ((root
                 (auto-compile-test-path
                  "recursive-quit/"))
                (source
                 (auto-compile-test-write
                  "recursive-quit/library.el"
                  "(provide 'auto-compile-library)\n"))
                (dest
                 (auto-compile-test-write
                  "recursive-quit/library.elc"
                  "compiled"))
                (stray
                 (auto-compile-test-write
                  "recursive-quit/stray.elc"
                  "stray"))
                (hidden
                 (auto-compile-test-write
                  "recursive-quit/.hidden/keep.elc"
                  "hidden"))
                (nosearch
                 (auto-compile-test-write
                  "recursive-quit/vendor/keep.elc"
                  "vendor"))
                (auto-compile-delete-stray-dest t))
         (auto-compile-test-write
          "recursive-quit/vendor/.nosearch"
          "")
         (toggle-auto-compile root 'quit)
         (list
          (file-exists-p source)
          (file-exists-p dest)
          (file-exists-p stray)
          (file-exists-p hidden)
          (file-exists-p nosearch)))"##,
        expect!["OK (t nil nil t t)"],
    )
}

fn auto_compile_recursive_quit_option_controls_nonlibrary_source_destination_removal()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_recursive_quit_option_controls_nonlibrary_source_destination_removal",
        r##"(let* ((root
                 (auto-compile-test-path
                  "recursive-nonlib/"))
                (source
                 (auto-compile-test-write
                  "recursive-nonlib/nonlib.el"
                  "42\n"))
                (dest
                 (auto-compile-test-write
                  "recursive-nonlib/nonlib.elc"
                  "compiled"))
                (auto-compile-predicate-function
                 (lambda (_file) nil))
                (auto-compile-delete-stray-dest nil))
         (let ((auto-compile-toggle-deletes-nonlib-dest
                nil))
           (toggle-auto-compile root 'quit))
         (let ((kept (file-exists-p dest))
               (auto-compile-toggle-deletes-nonlib-dest
                t))
           (toggle-auto-compile root 'quit)
           (list
            (file-exists-p source)
            kept
            (file-exists-p dest))))"##,
        expect!["OK (t t nil)"],
    )
}

fn auto_compile_recursive_start_honors_recompile_option_and_source_freshness() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_recursive_start_honors_recompile_option_and_source_freshness",
        r##"(let* ((root
                 (auto-compile-test-path
                  "recursive-freshness/"))
                (fresh-source
                 (auto-compile-test-write
                  "recursive-freshness/fresh.el"
                  "(provide 'fresh)\n"))
                (stale-source
                 (auto-compile-test-write
                  "recursive-freshness/stale.el"
                  "(provide 'stale)\n"))
                (fresh-dest
                 (auto-compile-test-write
                  "recursive-freshness/fresh.elc"
                  "fresh"))
                (stale-dest
                 (auto-compile-test-write
                  "recursive-freshness/stale.elc"
                  "stale"))
                (compiled nil)
                (auto-compile-toggle-recompiles nil))
         (auto-compile-test-set-time
          fresh-source 1000)
         (auto-compile-test-set-time
          fresh-dest 2000)
         (auto-compile-test-set-time
          stale-dest 1000)
         (auto-compile-test-set-time
          stale-source 2000)
         (cl-letf (((symbol-function
                     'auto-compile-byte-compile)
                    (lambda (file &optional start)
                      (push
                       (list
                        (file-name-nondirectory file)
                        start)
                       compiled)
                      t)))
           (toggle-auto-compile root 'start)
           (nreverse compiled)))"##,
        expect![[r#"OK (("stale.el" t))"#]],
    )
}

fn auto_compile_on_load_rebuilds_outdated_bytecode_and_executes_new_behavior() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_on_load_rebuilds_outdated_bytecode_and_executes_new_behavior",
        r##"(let* ((directory
                 (auto-compile-test-path
                  "load-workflow/"))
                (source
                 (auto-compile-test-write
                  "load-workflow/reloadable.el"
                  "(defun auto-compile-reloadable-value () 'old)\n(provide 'auto-compile-reloadable)\n"))
                (dest
                 (auto-compile-test-dest source)))
         (byte-compile-file source)
         (auto-compile-test-set-time dest 1000)
         (auto-compile-test-write
          "load-workflow/reloadable.el"
          "(defun auto-compile-reloadable-value () 'new)\n(provide 'auto-compile-reloadable)\n")
         (auto-compile-test-set-time source 2000)
         (let ((load-path
                (cons directory load-path))
               (load-suffixes
                '(".elc" ".el"))
               (load-file-rep-suffixes
                '("")))
           (auto-compile-on-load
            "reloadable")
           (when (featurep
                  'auto-compile-reloadable)
             (unload-feature
              'auto-compile-reloadable t))
           (when (fboundp
                  'auto-compile-reloadable-value)
             (fmakunbound
              'auto-compile-reloadable-value))
           (load dest nil nil t)
           (list
            (file-newer-than-file-p dest source)
            (auto-compile-reloadable-value)
            (featurep
             'auto-compile-reloadable))))"##,
        expect!["OK (t new t)"],
    )
}

fn auto_compile_require_advice_rebuilds_then_loads_new_bytecode_behavior() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_require_advice_rebuilds_then_loads_new_bytecode_behavior",
        r##"(let* ((directory
                 (auto-compile-test-path
                  "require-workflow/"))
                (source
                 (auto-compile-test-write
                  "require-workflow/auto-compile-required.el"
                  "(defun auto-compile-required-value () 10)\n(provide 'auto-compile-required)\n"))
                (dest
                 (auto-compile-test-dest source)))
         (byte-compile-file source)
         (auto-compile-test-set-time dest 1000)
         (auto-compile-test-write
          "require-workflow/auto-compile-required.el"
          "(defun auto-compile-required-value () 99)\n(provide 'auto-compile-required)\n")
         (auto-compile-test-set-time source 2000)
         (let ((load-path
                (cons directory load-path))
               (load-suffixes
                '(".elc" ".el"))
               (load-file-rep-suffixes
                '("")))
           (auto-compile-on-load-mode 1)
           (unwind-protect
               (list
                (require 'auto-compile-required)
                (auto-compile-required-value)
                (file-newer-than-file-p
                 dest source)
                (file-name-extension
                 (symbol-file
                  'auto-compile-required-value
                  'defun)))
             (auto-compile-on-load-mode -1)
             (when (featurep
                    'auto-compile-required)
               (unload-feature
                'auto-compile-required t)))))"##,
        expect![[r#"OK (auto-compile-required 99 t "elc")"#]],
    )
}

fn auto_compile_load_advice_is_inert_when_mode_disabled_and_active_when_enabled() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_compile_load_advice_is_inert_when_mode_disabled_and_active_when_enabled",
        r##"(let ((calls nil))
         (cl-letf (((symbol-function
                     'auto-compile-on-load)
                    (lambda (file &optional nosuffix)
                      (push
                       (list
                        (file-name-nondirectory file)
                        nosuffix)
                       calls))))
           (auto-compile-on-load-mode -1)
           (load
            (locate-library "subr-x")
            nil t t)
           (let ((disabled (nreverse calls)))
             (setq calls nil)
             (auto-compile-on-load-mode 1)
             (load
              (locate-library "subr-x")
              nil t t)
             (auto-compile-on-load-mode -1)
             (list
              disabled
              (nreverse calls)))))"##,
        expect![[r#"OK (nil (("subr-x.el" t)))"#]],
    )
}

fn auto_compile_on_load_removes_earlier_stray_bytecode_that_would_shadow_real_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_on_load_removes_earlier_stray_bytecode_that_would_shadow_real_source",
        r##"(let* ((early
                 (auto-compile-test-path
                  "shadow/early/"))
                (late
                 (auto-compile-test-path
                  "shadow/late/"))
                (stray
                 (auto-compile-test-write
                  "shadow/early/shadowed.elc"
                  "stale bytecode"))
                (source
                 (auto-compile-test-write
                  "shadow/late/shadowed.el"
                  "(provide 'auto-compile-shadowed)\n"))
                (load-path
                 (append
                  (list early late)
                  load-path))
                (load-suffixes
                 '(".elc" ".el"))
                (load-file-rep-suffixes
                 '(""))
                (auto-compile-delete-stray-dest
                 t))
         (auto-compile-on-load "shadowed")
         (list
          (file-exists-p source)
          (file-exists-p stray)
          (file-name-nondirectory
           (auto-compile--locate-library
            "shadowed" nil))))"##,
        expect![[r#"OK (t nil "shadowed.el")"#]],
    )
}

fn auto_compile_on_load_compiler_error_deletes_destination_and_contains_failure() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_compile_on_load_compiler_error_deletes_destination_and_contains_failure",
        r##"(let* ((source
                 (auto-compile-test-write
                  "load-error/broken.el"
                  "(provide 'broken)\n"))
                (dest
                 (auto-compile-test-write
                  "load-error/broken.elc"
                  "old"))
                (auto-compile-delete-stray-dest
                 nil))
         (auto-compile-test-set-time dest 1000)
         (auto-compile-test-set-time source 2000)
         (cl-letf (((symbol-function
                     'auto-compile--locate-library)
                    (lambda (&rest _) source))
                   ((symbol-function
                     'auto-compile--byte-compile-file)
                    (lambda (_file)
                      (error "compiler exploded"))))
           (list
            (auto-compile-on-load "broken")
            (file-exists-p dest)
            (current-message))))"##,
        expect![[
            r#"OK ("Deleting [ORACLE-SANDBOX]/auto-compile-fixture/load-error/broken.elc...done" nil nil)"#
        ]],
    )
}

fn auto_compile_loading_guard_prevents_recursive_reentry_for_same_library() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_loading_guard_prevents_recursive_reentry_for_same_library",
        r##"(let ((calls 0)
               (auto-compile--loading
                '("already-loading")))
         (cl-letf (((symbol-function
                     'auto-compile--locate-library)
                    (lambda (&rest _)
                      (setq calls (1+ calls))
                      (error
                       "guard failed"))))
           (list
            (auto-compile-on-load
             "already-loading")
            calls
            auto-compile--loading)))"##,
        expect![[r#"OK (nil 0 ("already-loading"))"#]],
    )
}

fn auto_compile_git_inhibit_distinguishes_attached_and_detached_head() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_compile_git_inhibit_distinguishes_attached_and_detached_head",
        r##"(let* ((repository
                 (auto-compile-test-path
                  "git-workflow/"))
                (default-directory repository))
         (make-directory repository t)
         (call-process "git" nil nil nil
                       "init" "--quiet")
         (auto-compile-test-write
          "git-workflow/tracked.el"
          "(provide 'tracked)\n")
         (call-process "git" nil nil nil
                       "add" "tracked.el")
         (call-process
          "git" nil nil nil
          "-c" "user.name=Parity Test"
          "-c" "user.email=parity@example.invalid"
          "commit" "--quiet" "-m" "initial")
         (let ((attached
                (auto-compile-inhibit-compile-detached-git-head)))
           (call-process
            "git" nil nil nil
            "checkout" "--quiet" "--detach" "HEAD")
           (list
            attached
            (auto-compile-inhibit-compile-detached-git-head))))"##,
        expect!["OK (nil t)"],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_compile_recursive_start_compiles_real_libraries_and_skips_hidden_and_nosearch_trees(),
        auto_compile_recursive_quit_removes_regular_and_stray_destinations_but_respects_skipped_trees(),
        auto_compile_recursive_quit_option_controls_nonlibrary_source_destination_removal(),
        auto_compile_recursive_start_honors_recompile_option_and_source_freshness(),
        auto_compile_on_load_rebuilds_outdated_bytecode_and_executes_new_behavior(),
        auto_compile_require_advice_rebuilds_then_loads_new_bytecode_behavior(),
        auto_compile_load_advice_is_inert_when_mode_disabled_and_active_when_enabled(),
        auto_compile_on_load_removes_earlier_stray_bytecode_that_would_shadow_real_source(),
        auto_compile_on_load_compiler_error_deletes_destination_and_contains_failure(),
        auto_compile_loading_guard_prevents_recursive_reentry_for_same_library(),
        auto_compile_git_inhibit_distinguishes_attached_and_detached_head(),
    ]
}
