#!/usr/bin/env python3
"""User init survives browser restart and overrides shipped WASM defaults."""

import argparse
from contextlib import contextmanager
from functools import partial
from http.server import ThreadingHTTPServer
import json
from pathlib import Path
import sys
from tempfile import TemporaryDirectory
from threading import Thread
from urllib.parse import urlsplit

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from tools.preview import PreviewHandler


@contextmanager
def package_failure_preview(directory):
    """Fail the real worker request, independent of browser CDP target scopes."""
    if directory is None:
        yield None
        return

    class Handler(PreviewHandler):
        def do_GET(self):
            if urlsplit(self.path).path.endswith("/packages.bundle"):
                self.send_error(503, "Package download unavailable for acceptance test")
            else:
                super().do_GET()

        def log_message(self, format, *args):
            pass

    with ThreadingHTTPServer(("127.0.0.1", 0), partial(Handler, directory=str(directory.resolve()))) as server:
        thread = Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            yield f"http://127.0.0.1:{server.server_port}/"
        finally:
            server.shutdown()
            thread.join()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--binary")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--artifacts-dir", required=True)
    parser.add_argument("--block-packages", type=Path, metavar="DISTRIBUTION",
                        help="serve this build temporarily with packages.bundle returning HTTP 503")
    args = parser.parse_args()
    if args.block_packages and not (args.block_packages / "index.html").is_file():
        parser.error("--block-packages must point to a packaged browser distribution")
    with package_failure_preview(args.block_packages) as failure_url, TemporaryDirectory(prefix="browser-init-", dir=Path(__file__).resolve().parents[3] / "tmp") as profile:
        options = chrome_options(args.binary, args.headless)
        options.add_argument(f"--user-data-dir={profile}")
        options.add_argument("--window-size=1800,1100")
        editor = BrowserEditorHarness(webdriver.Chrome(options=options), args.timeout)
        try:
            editor.install_frame_observer()
            editor.driver.get(failure_url or args.url)
            editor.wait_ready()
            editor.wait_for_presentation()
            editor.wait_for_frame_text("default landing page", contains="Welcome to the browser editor.")
            if args.block_packages:
                editor.eval_expression(
                    '''(progn
                      (unless neomacs-wasm-package-error (error "expected unavailable optional packages"))
                      (neomacs-wasm-landing-playground)
                      (insert "(+ 2 3)")
                      (message "FALLBACK-EDITOR-WORKS"))''',
                    "FALLBACK-EDITOR-WORKS",
                )
                print("PASS: blocked optional package download still opens an editable landing page")
                return
            editor.eval_expression(
                '''(progn
                  (unless (and (eq neomacs-wasm-startup-profile 'landing)
                               (featurep 'treemacs) (featurep 'doom-themes)
                               keycast-tab-bar-mode tab-bar-mode global-tab-line-mode
                               tab-line-mode header-line-format
                               (not neomacs-wasm-package-error)
                               (treemacs-get-local-window)
                               (get-buffer-window "*NEO Emacs About*")
                               (memq 'doom-one (custom-available-themes)))
                    (error "landing packages missing: %S" neomacs-wasm-package-error))
                  (message "DEFAULT-LANDING-READY"))''',
                "DEFAULT-LANDING-READY",
            )
            artifacts = Path(args.artifacts_dir)
            artifacts.mkdir(parents=True, exist_ok=True)
            editor.driver.save_screenshot(str(artifacts / "default-landing.png"))
            editor.eval_expression(
                '''(progn (neomacs-wasm-landing-theme 'doom-one)
                  (unless (and (equal custom-enabled-themes '(doom-one))
                               (equal (face-background 'default) "#282c34"))
                    (error "doom-one theme not applied"))
                  (message "DOOM-THEME-APPLIED"))''',
                "DOOM-THEME-APPLIED",
            )
            editor.driver.save_screenshot(str(artifacts / "dark-landing.png"))
            editor.eval_expression(
                '''(progn
                  (with-temp-file "/neomacs-fake/landing-check.el" (insert ";; A file opened through Treemacs"))
                  (select-window (treemacs-get-local-window))
                  (treemacs-refresh)
                  (goto-char (point-min))
                  (search-forward "landing-check.el")
                  (message "TREE-FILE-SELECTED"))''',
                "TREE-FILE-SELECTED",
            )
            editor.dispatch_key("Enter")
            editor.wait_for_frame_text("file opened through Treemacs", contains="A file opened through Treemacs")
            editor.eval_expression(
                '''(progn
                  (unless (equal buffer-file-name "/neomacs-fake/landing-check.el")
                    (error "Treemacs did not visit the file"))
                  (neomacs-wasm-landing-open)
                  (message "TREE-NAVIGATION-WORKS"))''',
                "TREE-NAVIGATION-WORKS",
            )
            init = '''(setq neomacs-wasm-startup-profile 'editor)
(setq neomacs-wasm-test-init-count (1+ (or (bound-and-true-p neomacs-wasm-test-init-count) 0)))
(which-key-mode -1)
'''
            editor.eval_expression(
                '(progn (make-directory user-emacs-directory t) '
                '(with-temp-file (expand-file-name "init.el" user-emacs-directory) '
                f'(insert {json.dumps(init)})) (message "INIT-SAVED"))',
                "INIT-SAVED",
            )
            editor.driver.quit()
            editor.driver = webdriver.Chrome(options=options)
            editor.install_frame_observer()
            editor.driver.get(args.url)
            editor.wait_ready()
            editor.wait_for_presentation()
            editor.eval_expression(
                '''(progn
                  (unless (and (equal user-init-file "/neomacs-fake/.emacs.d/init.el")
                               (equal (bound-and-true-p neomacs-wasm-test-init-count) 1)
                               (eq neomacs-wasm-startup-profile 'editor)
                               (not which-key-mode)
                               (get-buffer-window "*scratch*"))
                    (error "user init not honored: %S" user-init-file))
                  (message "USER-INIT-HONORED"))''',
                "USER-INIT-HONORED",
            )
            print("PASS: persistent user init loaded once; user defaults win; editor profile opens scratch")
            editor.invoke_mx("neomacs-wasm-landing-open")
            editor.wait_for_frame_text("landing command in editor profile", contains="Welcome to the browser editor.")
            landing_init = '''(setq neomacs-wasm-startup-profile 'landing)
(setq neomacs-wasm-landing-personal-info "A test user's introduction.")
'''
            editor.eval_expression(
                '(progn (with-temp-file user-init-file '
                f'(insert {json.dumps(landing_init)})) (message "LANDING-INIT-SAVED"))',
                "LANDING-INIT-SAVED",
            )
            editor.driver.quit()
            editor.driver = webdriver.Chrome(options=options)
            editor.install_frame_observer()
            editor.driver.get(args.url)
            editor.wait_ready()
            editor.wait_for_presentation()
            editor.wait_for_frame_text("landing page", contains="Welcome to the browser editor.")
            editor.eval_expression(
                '''(progn
                  (unless (and which-key-mode
                               (get-buffer-window "*NEO Emacs*")
                               (get-buffer-window "*NEO Emacs Playground*")
                               (eq major-mode 'emacs-lisp-mode)
                               (= (buffer-size) 0)
                               (get-buffer-window "*NEO Emacs About*"))
                    (error "landing windows not ready"))
                  (message "LANDING-READY"))''',
                "LANDING-READY",
            )
            editor.type_native_text("(setq neomacs-wasm-test-evaluation (+ 1 2))")
            editor.wait_for_frame_text("playground editing", contains="(+ 1 2)")
            editor.type_native_control_key("x")
            editor.type_native_control_key("e")
            editor.eval_expression(
                '''(progn
                  (unless (equal (bound-and-true-p neomacs-wasm-test-evaluation) 3)
                    (error "C-x C-e did not evaluate the playground expression"))
                  (message "PLAYGROUND-EVALUATED"))''',
                "PLAYGROUND-EVALUATED",
            )
            editor.eval_expression(
                '''(progn (neomacs-wasm-landing-open)
                  (unless (equal (buffer-string) "(setq neomacs-wasm-test-evaluation (+ 1 2))")
                    (error "playground edits were lost"))
                  (unless (and (treemacs-get-local-window)
                               (get-buffer-window "*NEO Emacs About*"))
                    (error "reopening lost a landing sidebar"))
                  (message "PLAYGROUND-PRESERVED"))''',
                "PLAYGROUND-PRESERVED",
            )
            artifacts = Path(args.artifacts_dir)
            artifacts.mkdir(parents=True, exist_ok=True)
            editor.driver.save_screenshot(str(artifacts / "landing.png"))
            print("PASS: landing command/profile, personal sidebar, which-key, Lisp evaluation; edits survive reopening")
            editor.driver.set_window_size(800, 750)
            editor.eval_expression(
                '''(progn (neomacs-wasm-landing-open)
                  (unless (and (< (frame-width) 100) (one-window-p t)
                               (equal (buffer-name) "*NEO Emacs*")
                               (with-current-buffer "*NEO Emacs Playground*"
                                 (equal (buffer-string) "(setq neomacs-wasm-test-evaluation (+ 1 2))")))
                    (error "narrow layout lost content or retained sidebars"))
                  (message "NARROW-LANDING-READY"))''',
                "NARROW-LANDING-READY",
            )
            editor.driver.save_screenshot(str(artifacts / "narrow-landing.png"))
            print("PASS: narrow landing hides sidebars without losing playground contents")
        except Exception:
            editor.capture_failure_artifacts(args.artifacts_dir)
            raise
        finally:
            editor.driver.quit()


if __name__ == "__main__":
    main()
