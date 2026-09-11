#!/usr/bin/env python3
"""Require Lisp recursion limits to signal without killing the WASM Worker."""

import argparse

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--chrome")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--artifacts-dir")
    parser.add_argument("--limits", type=int, nargs="+", default=[200, 400, 800, 1600])
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--bytecompiled", action="store_true")
    parser.add_argument("--lexical", action="store_true")
    parser.add_argument(
        "--call-style",
        choices=[
            "direct", "argument", "conditional", "conditional-test", "conditional-else", "funcall", "apply", "binding",
            "initializer", "sequential-binding", "sequential-initializer", "protected",
        ],
        default="direct",
    )
    args = parser.parse_args()
    driver = webdriver.Chrome(options=chrome_options(args.chrome, args.headless))
    editor = BrowserEditorHarness(driver, args.timeout)
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        compile_probe = (
            "(fset 'neomacs-stack-probe (byte-compile (symbol-function 'neomacs-stack-probe)))"
            if args.bytecompiled else ""
        )
        body = {
            "direct": "(neomacs-stack-probe)",
            "argument": "(+ 1 (neomacs-stack-probe))",
            "conditional": "(if t (neomacs-stack-probe))",
            "conditional-test": "(if (neomacs-stack-probe) 1 2)",
            "conditional-else": "(if nil 1 2 (neomacs-stack-probe))",
            "funcall": "(funcall #'neomacs-stack-probe)",
            "apply": "(apply #'neomacs-stack-probe nil)",
            "binding": "(let ((neomacs-stack-local 42)) (neomacs-stack-probe))",
            "initializer": "(let ((neomacs-stack-local (neomacs-stack-probe))) neomacs-stack-local)",
            "sequential-binding": "(let* ((neomacs-stack-local 42)) (neomacs-stack-probe))",
            "sequential-initializer": "(let* ((neomacs-stack-local (neomacs-stack-probe))) neomacs-stack-local)",
            "protected": "(unwind-protect (neomacs-stack-probe) (setq neomacs-stack-cleanups (1+ neomacs-stack-cleanups)))",
        }[args.call_style]
        for limit in args.limits:
            # GNU bytecode.c raises a plain error for Bcall overflow; eval.c
            # raises excessive-lisp-nesting. Do not erase that distinction.
            error_handler = (
                f'''(if (and (stringp (cadr err))
                             (string-match-p "Lisp nesting exceeds.*max-lisp-eval-depth" (cadr err)))
                        (message (concat "STACK-" "{limit}-PASS"))
                      (message (concat "STACK-" "FAIL: %S") err))'''
                if args.bytecompiled else '(message (concat "STACK-" "FAIL: %S") err)'
            )
            editor.eval_expression(
                f"""(progn
                      (setq neomacs-stack-cleanups 0)
                      (setq neomacs-stack-local 'outside)
                      (defalias 'neomacs-stack-probe
                        (eval '(function (lambda () {body})) {"t" if args.lexical else "nil"}))
                      {compile_probe}
                      (unwind-protect
                          (condition-case err
                              (let ((max-lisp-eval-depth {limit}))
                                (neomacs-stack-probe)
                                (message (concat "STACK-" "FAIL: returned")))
                            (excessive-lisp-nesting
                              (message (concat "STACK-" "{limit}-PASS")))
                            (error {error_handler}))
                        (fmakunbound 'neomacs-stack-probe)))""",
                f"STACK-{limit}-PASS",
                failure_marker="STACK-FAIL:",
            )
            print(f"PASS: Lisp recursion limit {limit} signals without trapping", flush=True)
            if args.call_style in {
                "binding", "initializer", "sequential-binding", "sequential-initializer",
            }:
                editor.eval_expression(
                    '(if (eq neomacs-stack-local \'outside) (message (concat "STACK-" "BINDING-PASS")) (message (concat "STACK-" "FAIL: leaked binding")))',
                    "STACK-BINDING-PASS", failure_marker="STACK-FAIL:",
                )
            if args.call_style == "protected":
                editor.eval_expression(
                    '(if (> neomacs-stack-cleanups 0) (message (concat "STACK-" "CLEANUP-PASS")) (message (concat "STACK-" "FAIL: missing cleanup")))',
                    "STACK-CLEANUP-PASS", failure_marker="STACK-FAIL:",
                )
                editor.eval_expression(
                    """(let ((trace nil))
                         (let ((caught
                                (condition-case err
                                    (unwind-protect
                                        (unwind-protect
                                            (error "body")
                                          (setq trace (cons 'inner trace))
                                          (error "cleanup"))
                                      (setq trace (cons 'outer trace)))
                                  (error (cadr err)))))
                           (if (and (equal caught "cleanup")
                                    (equal trace '(outer inner)))
                               (message (concat "STACK-" "UNWIND-PASS"))
                             (message (concat "STACK-" "FAIL: unwind %S %S") caught trace))))""",
                    "STACK-UNWIND-PASS", failure_marker="STACK-FAIL:",
                )
        editor.eval_expression(
            '(message (concat "STACK-" "ALIVE-%d") (+ 20 22))',
            "STACK-ALIVE-42",
        )
    except Exception:
        if args.artifacts_dir:
            editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
