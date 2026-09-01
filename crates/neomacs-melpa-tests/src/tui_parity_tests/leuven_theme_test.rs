use std::time::Duration;

use expect_test::expect;
use neomacs_tui_tests::{RawTerminalSnapshot, TuiSession};

use crate::{CachedMelpaOracle, LEUVEN_THEME_MELPA_PIN};

use super::support::PackageTuiPair;

const LEUVEN_TUI_PRELUDE: &str = r####"
(require 'cl-lib)

(defvar neomacs-leuven-tui-lifecycle-report nil)
(defvar neomacs-leuven-tui-phase nil)
(defvar neomacs-leuven-tui-original-controls nil)
(defvar neomacs-leuven-tui-light-scale-history nil)
(defvar neomacs-leuven-tui-dark-scale-history nil)

(defconst neomacs-leuven-tui-buffers
  '("*Leuven Elisp*" "*Leuven Org*" "*Leuven Diff*"))

(defconst neomacs-leuven-tui-control-symbols
  '(leuven-scale-org-document-title
    leuven-scale-outline-headlines
    leuven-scale-org-agenda-structure
    leuven-scale-volatile-highlight
    leuven-dark-scale-org-document-title
    leuven-dark-scale-outline-headlines
    leuven-dark-scale-org-agenda-structure
    leuven-dark-scale-volatile-highlight))

(defun neomacs-leuven-tui-face (face attributes)
  "Return direct and resolved ATTRIBUTES for FACE on the selected frame."
  (list
   :direct
   (mapcar (lambda (attribute)
             (cons attribute (face-attribute face attribute nil nil)))
           attributes)
   :resolved
   (mapcar (lambda (attribute)
             (cons attribute (face-attribute face attribute nil 'default)))
           attributes)))

(defun neomacs-leuven-tui-default-state ()
  "Return public lifecycle and default-face state for the selected frame."
  (list
   :enabled (copy-sequence custom-enabled-themes)
   :mode (frame-parameter nil 'background-mode)
   :direct
   (list (face-attribute 'default :foreground nil nil)
         (face-attribute 'default :background nil nil))
   :resolved
   (list (face-attribute 'default :foreground nil 'default)
         (face-attribute 'default :background nil 'default))))

(defun neomacs-leuven-tui-source-directory ()
  "Return the installed Leuven directory selected over GNU's built-in copy."
  (let ((file
         (locate-file
          "leuven-theme.el"
          (cl-remove-if-not #'stringp custom-theme-load-path))))
    (and file
         (file-name-nondirectory
          (directory-file-name (file-name-directory file))))))

(defun neomacs-leuven-tui-start ()
  "Drive the real public light/dark lifecycle and display its exact report."
  (let* ((baseline (neomacs-leuven-tui-default-state))
         (loaded-no-enable (load-theme 'leuven t t))
         (registered
          (list :result loaded-no-enable
                :known (and (custom-theme-p 'leuven) t)
                :enabled (copy-sequence custom-enabled-themes))))
    (enable-theme 'leuven)
    (let ((light (neomacs-leuven-tui-default-state)))
      (load-theme 'leuven-dark t)
      (let ((dark (neomacs-leuven-tui-default-state)))
        (disable-theme 'leuven-dark)
        (let ((light-restored (neomacs-leuven-tui-default-state)))
          (disable-theme 'leuven)
          (let ((restored (neomacs-leuven-tui-default-state)))
            ;; A repeated public disable is deliberately a no-op.
            (disable-theme 'leuven)
            (with-current-buffer (get-buffer-create "*Leuven Lifecycle*")
              (let ((inhibit-read-only t))
                (erase-buffer)
                (insert
                 (format "CAP %S\n"
                         (list :cells (display-color-cells)
                               :visual-class (display-visual-class)
                               :display-type
                               (frame-parameter nil 'display-type)
                               :graphic (display-graphic-p)
                               :gate
                               (face-spec-set-match-display
                                '((class color) (min-colors 89)) nil)))
                 (format "SOURCE %S\n"
                         (neomacs-leuven-tui-source-directory))
                 (format "REGISTERED %S\n" registered)
                 (format "BASELINE %S\n" baseline)
                 (format "LIGHT %S\n" light)
                 (format "DARK %S\n" dark)
                 (format "LIGHT-RESTORED %S\n" light-restored)
                 (format "BASELINE-RESTORED %S\n" restored)
                 (format "RESTORATION %S\n"
                         (list :light (equal light light-restored)
                               :baseline (equal baseline restored)
                               :second-disable
                               (equal restored
                                      (neomacs-leuven-tui-default-state))))
                 "LEUVEN-TUI-READY\n"))
              (setq neomacs-leuven-tui-lifecycle-report (buffer-string))
              (goto-char (point-min))
              (special-mode)
              (switch-to-buffer (current-buffer))
              (delete-other-windows))))))))

(defun neomacs-leuven-tui-populate-buffers ()
  "Create and fontify the representative real editing buffers."
  (with-current-buffer (get-buffer-create "*Leuven Elisp*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert
       ";; Publish release Ω after review.\n"
       "(defconst release-limit 42)\n"
       "(defun deploy-release (artifact)\n"
       "  \"Ship ARTIFACT safely.\"\n"
       "  (when artifact (message \"ship %s\" artifact)))\n")
      (emacs-lisp-mode)
      (font-lock-ensure)))
  (with-current-buffer (get-buffer-create "*Leuven Org*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert
       "#+title: Release Control Ω\n"
       "* TODO Deploy service\n"
       "** DONE Verify rollback\n"
       "Read the [[https://example.test/runbook][runbook]].\n"
       "#+begin_src emacs-lisp\n"
       "(message \"ship\")\n"
       "#+end_src\n")
      (org-mode)
      (font-lock-ensure)))
  (with-current-buffer (get-buffer-create "*Leuven Diff*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert
       "diff --git a/release.el b/release.el\n"
       "--- a/release.el\n"
       "+++ b/release.el\n"
       "@@ -1,2 +1,2 @@\n"
       " context line\n"
       "-old release\n"
       "+new release Ω\n")
      (diff-mode)
      (font-lock-ensure))))

(defun neomacs-leuven-tui-direct-heights ()
  "Return direct applied heights for all documented scaled surfaces."
  (mapcar (lambda (face) (face-attribute face :height nil nil))
          '(org-document-title org-level-1 org-level-2
            org-agenda-structure org-agenda-date next-error)))

(defun neomacs-leuven-tui-set-controls (prefix values)
  "Set the four public PREFIX scaling controls to VALUES through Custom."
  (cl-mapc
   (lambda (suffix value)
     (customize-set-variable
      (intern (format "%s-scale-%s" prefix suffix)) value))
   '(org-document-title outline-headlines
     org-agenda-structure volatile-highlight)
   values))

(defun neomacs-leuven-tui-exercise-light-controls ()
  "Record numeric/nil/default/reload behavior for all light controls."
  (unless neomacs-leuven-tui-light-scale-history
    (neomacs-leuven-tui-set-controls 'leuven '(1.45 1.25 1.75 1.2))
    (load-theme 'leuven t)
    (let ((numeric (neomacs-leuven-tui-direct-heights)))
      (neomacs-leuven-tui-set-controls 'leuven '(nil nil nil nil))
      (let ((nil-before-reload (neomacs-leuven-tui-direct-heights)))
        (load-theme 'leuven t)
        (let ((nil-after-reload (neomacs-leuven-tui-direct-heights)))
          (neomacs-leuven-tui-set-controls 'leuven '(t t t t))
          (load-theme 'leuven t)
          (let ((defaults (neomacs-leuven-tui-direct-heights)))
            (neomacs-leuven-tui-set-controls
             'leuven '(1.45 1.25 1.75 1.2))
            (load-theme 'leuven t)
            (setq neomacs-leuven-tui-light-scale-history
                  (list (list 'numeric numeric)
                        (list 'nil-before-reload nil-before-reload)
                        (list 'nil-after-reload nil-after-reload)
                        (list 'default defaults)
                        (list 'final-numeric
                              (neomacs-leuven-tui-direct-heights))))))))))

(defun neomacs-leuven-tui-exercise-dark-controls ()
  "Record numeric/nil/default/reload behavior for all dark controls."
  (unless neomacs-leuven-tui-dark-scale-history
    ;; Measure dark without Leuven light supplying direct heights beneath it.
    ;; Rebuild the public dark-over-light stack after the control matrix.
    (when (custom-theme-enabled-p 'leuven)
      (disable-theme 'leuven))
    (neomacs-leuven-tui-set-controls
     'leuven-dark '(1.55 1.35 1.65 1.15))
    (load-theme 'leuven-dark t)
    (let ((numeric (neomacs-leuven-tui-direct-heights)))
      (neomacs-leuven-tui-set-controls 'leuven-dark '(nil nil nil nil))
      (let ((nil-before-reload (neomacs-leuven-tui-direct-heights)))
        (load-theme 'leuven-dark t)
        (let ((nil-after-reload (neomacs-leuven-tui-direct-heights)))
          (neomacs-leuven-tui-set-controls 'leuven-dark '(t t t t))
          (load-theme 'leuven-dark t)
          (let ((defaults (neomacs-leuven-tui-direct-heights)))
            (neomacs-leuven-tui-set-controls
             'leuven-dark '(1.55 1.35 1.65 1.15))
            (load-theme 'leuven-dark t)
            (setq neomacs-leuven-tui-dark-scale-history
                  (list (list 'numeric numeric)
                        (list 'nil-before-reload nil-before-reload)
                        (list 'nil-after-reload nil-after-reload)
                        (list 'default defaults)
                        (list 'final-numeric
                              (neomacs-leuven-tui-direct-heights)))))))))
    (when (custom-theme-enabled-p 'leuven-dark)
      (disable-theme 'leuven-dark))
    (enable-theme 'leuven)
    (enable-theme 'leuven-dark)))

(defun neomacs-leuven-tui-show-buffer (name)
  "Show test buffer NAME with a deterministic phase marker."
  (switch-to-buffer name)
  (setq-local header-line-format
              (format "LEUVEN %s %s"
                      (upcase (symbol-name neomacs-leuven-tui-phase)) name))
  (goto-char (point-min))
  (delete-other-windows)
  (redisplay t))

(defun neomacs-leuven-tui-use-light ()
  "Select light Leuven and show the real Elisp buffer."
  (interactive)
  (when (custom-theme-enabled-p 'leuven-dark)
    (disable-theme 'leuven-dark))
  (unless (custom-theme-enabled-p 'leuven)
    (enable-theme 'leuven))
  ;; Leuven was enabled before these mode faces were defined.  Their late
  ;; `defface' calls must still pick up the already-enabled theme.
  (require 'org)
  (require 'org-agenda)
  (require 'diff-mode)
  (unless neomacs-leuven-tui-original-controls
    (setq neomacs-leuven-tui-original-controls
          (mapcar (lambda (symbol) (cons symbol (symbol-value symbol)))
                  neomacs-leuven-tui-control-symbols)))
  (neomacs-leuven-tui-exercise-light-controls)
  (setq neomacs-leuven-tui-phase 'light)
  (neomacs-leuven-tui-populate-buffers)
  (neomacs-leuven-tui-show-buffer "*Leuven Elisp*"))

(defun neomacs-leuven-tui-use-dark ()
  "Stack dark Leuven over light and show the real Elisp buffer."
  (interactive)
  (unless (custom-theme-enabled-p 'leuven)
    (enable-theme 'leuven))
  (neomacs-leuven-tui-exercise-dark-controls)
  (setq neomacs-leuven-tui-phase 'dark)
  (neomacs-leuven-tui-populate-buffers)
  (neomacs-leuven-tui-show-buffer "*Leuven Elisp*"))

(defun neomacs-leuven-tui-show-elisp ()
  (interactive)
  (neomacs-leuven-tui-show-buffer "*Leuven Elisp*"))

(defun neomacs-leuven-tui-show-org ()
  (interactive)
  (neomacs-leuven-tui-show-buffer "*Leuven Org*"))

(defun neomacs-leuven-tui-show-diff ()
  (interactive)
  (neomacs-leuven-tui-show-buffer "*Leuven Diff*"))

(defun neomacs-leuven-tui-property-run (buffer token)
  "Return TOKEN's exact real face-property run in BUFFER."
  (with-current-buffer buffer
    (save-excursion
      (goto-char (point-min))
      (search-forward token)
      (let* ((position (match-beginning 0))
             (face (get-text-property position 'face))
             (start position)
             (end position))
        (while (and (> start (point-min))
                    (equal face (get-text-property (1- start) 'face)))
          (setq start (1- start)))
        (while (and (< end (point-max))
                    (equal face (get-text-property end 'face)))
          (setq end (1+ end)))
        (list :token token :face face
              :run (buffer-substring-no-properties start end))))))

(defun neomacs-leuven-tui-report-face (face attributes)
  "Insert one compact direct/resolved FACE report."
  (insert
   (format "FACE-D %s %S\n"
           face
           (mapcar (lambda (attribute)
                     (cons attribute
                           (face-attribute face attribute nil nil)))
                   attributes))
   (format "FACE-R %s %S\n"
           face
           (mapcar (lambda (attribute)
                     (cons attribute
                           (face-attribute face attribute nil 'default)))
                   attributes))))

(defun neomacs-leuven-tui-report-run (buffer token)
  "Insert one compact real property-run report."
  (let ((run (neomacs-leuven-tui-property-run buffer token))
        (print-escape-newlines t))
    (insert (format "RUN %s %S %S\n"
                    (substring buffer 8 -1)
                    (plist-get run :face)
                    (plist-get run :run)))))

(defun neomacs-leuven-tui-show-report ()
  "Show exact applied faces and real font-lock runs for the active variant."
  (interactive)
  (with-current-buffer (get-buffer-create "*Leuven Lifecycle*")
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert (format "PHASE %s %S\n"
                      neomacs-leuven-tui-phase
                      (copy-sequence custom-enabled-themes)))
      (dolist (entry
               (if (eq neomacs-leuven-tui-phase 'light)
                   neomacs-leuven-tui-light-scale-history
                 neomacs-leuven-tui-dark-scale-history))
        (insert (format "SCALE-DIRECT %s %s %S\n"
                        neomacs-leuven-tui-phase (car entry) (cadr entry))))
      (neomacs-leuven-tui-report-face 'default '(:foreground :background))
      (neomacs-leuven-tui-report-face
       'font-lock-comment-face '(:foreground :slant))
      (neomacs-leuven-tui-report-face 'font-lock-keyword-face '(:foreground))
      (neomacs-leuven-tui-report-face
       'font-lock-function-name-face '(:foreground))
      (neomacs-leuven-tui-report-face 'diff-context '(:foreground :background))
      (neomacs-leuven-tui-report-face
       'diff-header '(:foreground :background :weight))
      (neomacs-leuven-tui-report-face
       'org-document-title '(:foreground :weight :height))
      (neomacs-leuven-tui-report-face
       'org-level-1 '(:foreground :background :height))
      (neomacs-leuven-tui-report-face 'org-link '(:foreground :underline))
      (neomacs-leuven-tui-report-face 'org-block '(:foreground :background))
      (neomacs-leuven-tui-report-run "*Leuven Elisp*" ";; Publish release")
      (neomacs-leuven-tui-report-run "*Leuven Elisp*" "defun")
      (neomacs-leuven-tui-report-run "*Leuven Elisp*" "deploy-release")
      (neomacs-leuven-tui-report-run "*Leuven Elisp*" "\"Ship ARTIFACT safely.\"")
      (neomacs-leuven-tui-report-run "*Leuven Org*" "Release Control")
      (neomacs-leuven-tui-report-run "*Leuven Org*" "TODO")
      (neomacs-leuven-tui-report-run "*Leuven Org*" "DONE")
      (neomacs-leuven-tui-report-run "*Leuven Org*" "runbook")
      (neomacs-leuven-tui-report-run "*Leuven Org*" "#+begin_src")
      (neomacs-leuven-tui-report-run "*Leuven Diff*" "diff --git")
      (neomacs-leuven-tui-report-run "*Leuven Diff*" "@@ -1,2")
      (neomacs-leuven-tui-report-run "*Leuven Diff*" " context line")
      (neomacs-leuven-tui-report-run "*Leuven Diff*" "-old release")
      (neomacs-leuven-tui-report-run "*Leuven Diff*" "+new release"))
    (goto-char (point-min))
    (special-mode)
    (switch-to-buffer (current-buffer))
    (delete-other-windows)))

(defun neomacs-leuven-tui-finish ()
  "Disable both themes, kill fixtures, and prove baseline restoration."
  (interactive)
  (dolist (theme '(leuven-dark leuven))
    (when (custom-theme-enabled-p theme)
      (disable-theme theme)))
  (dolist (entry neomacs-leuven-tui-original-controls)
    (customize-set-variable (car entry) (cdr entry)))
  (dolist (name neomacs-leuven-tui-buffers)
    (when (get-buffer name) (kill-buffer name)))
  (when (get-buffer "*Leuven Lifecycle*")
    (kill-buffer "*Leuven Lifecycle*"))
  (switch-to-buffer (get-buffer-create "*Leuven Clean*"))
  (let ((inhibit-read-only t))
    (erase-buffer)
    (insert (format "LEUVEN-TUI-CLEAN %S"
                    (neomacs-leuven-tui-default-state))))
  (delete-other-windows))

(add-hook 'emacs-startup-hook #'neomacs-leuven-tui-start)
"####;

const REPORT_PREFIXES: &[&str] = &[
    "CAP ",
    "SOURCE ",
    "REGISTERED ",
    "BASELINE ",
    "LIGHT ",
    "DARK ",
    "LIGHT-RESTORED ",
    "BASELINE-RESTORED ",
    "RESTORATION ",
    "PHASE ",
    "SCALE-DIRECT ",
    "FACE-D ",
    "FACE-R ",
    "RUN ",
    "LEUVEN-TUI-READY",
];

fn lifecycle_report(pair: &PackageTuiPair, gnu: bool) -> String {
    let grid = if gnu {
        pair.gnu.text_grid()
    } else {
        pair.neo.text_grid()
    };
    grid.into_iter()
        .map(|row| row.trim_end().to_owned())
        .filter(|row| REPORT_PREFIXES.iter().any(|prefix| row.starts_with(prefix)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn wait_for<F>(session: &mut TuiSession, timeout: Duration, description: &str, predicate: F)
where
    F: Fn(&[String]) -> bool,
{
    session.read_until(timeout, |grid| predicate(grid));
    let grid = session.text_grid();
    assert!(
        predicate(&grid),
        "{} timed out waiting for {description}:\n{}",
        session.name,
        grid.join("\n")
    );
}

fn invoke(session: &mut TuiSession, command: &str, ready: &str) {
    session.send_keys("M-x");
    wait_for(
        session,
        Duration::from_secs(8),
        &format!("M-x prompt before {command}"),
        |grid| grid.iter().any(|row| row.contains("M-x")),
    );
    session.send(command.as_bytes());
    session.send_keys("RET");
    wait_for(
        session,
        Duration::from_secs(12),
        &format!("{command} readiness marker {ready:?}"),
        |grid| grid.iter().any(|row| row.contains(ready)),
    );
}

fn invoke_both(pair: &mut PackageTuiPair, command: &str, ready: &str) {
    invoke(&mut pair.gnu, command, ready);
    invoke(&mut pair.neo, command, ready);
}

fn ansi_rows(session: &TuiSession, needles: &[&str]) -> String {
    let grid = session.text_grid();
    needles
        .iter()
        .map(|needle| {
            let row = grid
                .iter()
                .position(|contents| contents.contains(needle))
                .unwrap_or_else(|| {
                    panic!(
                        "{} never rendered {needle:?}:\n{}",
                        session.name,
                        grid.join("\n")
                    )
                }) as u16;
            RawTerminalSnapshot::capture_rows(session.screen(), row..row + 1).ansi_grid()
        })
        .collect::<Vec<_>>()
        .join("")
}

fn assert_rendered_rows(
    pair: &PackageTuiPair,
    label: &str,
    needles: &[&str],
    expected: expect_test::Expect,
    neo_mismatches: &mut Vec<String>,
) -> String {
    let gnu = ansi_rows(&pair.gnu, needles);
    let neo = ansi_rows(&pair.neo, needles);
    expected.assert_eq(&gnu);
    record_neo_mismatch(neo_mismatches, label, &neo, &gnu);
    gnu
}

fn record_neo_mismatch(mismatches: &mut Vec<String>, label: &str, neo: &str, gnu: &str) {
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU: {gnu:?}\nNeo: {neo:?}"));
    }
}

fn exact_row(session: &TuiSession, needle: &str) -> String {
    session
        .text_grid()
        .into_iter()
        .find(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("{} did not render {needle:?}", session.name))
        .trim_end()
        .to_owned()
}

#[test]
fn leuven_theme_real_color_lifecycle_matches_gnu() {
    let oracle = CachedMelpaOracle::new(LEUVEN_THEME_MELPA_PIN, "leuven-theme.el")
        .expect("prepare exact Leuven Theme source below ./tmp")
        .with_prelude(LEUVEN_TUI_PRELUDE);
    let mut pair = PackageTuiPair::spawn("leuven-theme-lifecycle", oracle.prepared_packages())
        .expect("spawn real Leuven Theme PTY pair");
    let mut neo_mismatches = Vec::new();

    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("LEUVEN-TUI-READY"));
    wait_for(
        &mut pair.gnu,
        Duration::from_secs(20),
        "Leuven lifecycle readiness",
        ready,
    );
    wait_for(
        &mut pair.neo,
        Duration::from_secs(30),
        "Leuven lifecycle readiness",
        ready,
    );

    let gnu_report = lifecycle_report(&pair, true);
    let neo_report = lifecycle_report(&pair, false);
    let expected = expect![[r##"
        CAP (:cells 16777216 :visual-class static-color :display-type color :graphic nil :gate t)
        SOURCE "leuven-theme-20260213.1052"
        REGISTERED (:result t :known t :enabled nil)
        BASELINE (:enabled nil :mode dark :direct ("unspecified-fg" "unspecified-bg") :resolved ("unspecified-fg" "unspecified-bg"))
        LIGHT (:enabled (leuven) :mode light :direct ("#333333" "#FFFFFF") :resolved ("#333333" "#FFFFFF"))
        DARK (:enabled (leuven-dark leuven) :mode dark :direct ("#cfccd2" "#25202a") :resolved ("#cfccd2" "#25202a"))
        LIGHT-RESTORED (:enabled (leuven) :mode light :direct ("#333333" "#FFFFFF") :resolved ("#333333" "#FFFFFF"))
        BASELINE-RESTORED (:enabled nil :mode dark :direct ("unspecified-fg" "unspecified-bg") :resolved ("unspecified-fg" "unspecified-bg"))
        RESTORATION (:light t :baseline t :second-disable t)
        LEUVEN-TUI-READY"##]];
    expected.assert_eq(&gnu_report);
    record_neo_mismatch(
        &mut neo_mismatches,
        "public theme lifecycle",
        &neo_report,
        &gnu_report,
    );

    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-use-light",
        ";; Publish release Ω",
    );
    let light_elisp = assert_rendered_rows(
        &pair,
        "light Elisp rows",
        &[
            ";; Publish release",
            "defconst",
            "defun",
            "Ship ARTIFACT",
            "when artifact",
        ],
        expect![[r#"
            [0;38;2;141;141;132;48;2;255;255;255m;; [0;2;38;2;160;161;167;48;2;255;255;255mPublish release Ω after review. [0;38;2;51;51;51;48;2;255;255;255m                                                                                                                             [0m
            [0;38;2;51;51;51;48;2;255;255;255m([0;38;2;0;0;255;48;2;255;255;255mdefconst[0;38;2;51;51;51;48;2;255;255;255m [0;38;2;186;54;165;48;2;255;255;255mrelease-limit[0;38;2;51;51;51;48;2;255;255;255m 42)                                                                                                                                     [0m
            [0;38;2;51;51;51;48;2;255;255;255m([0;38;2;0;0;255;48;2;255;255;255mdefun[0;38;2;51;51;51;48;2;255;255;255m [0;38;2;0;102;153;48;2;255;255;255mdeploy-release[0;38;2;51;51;51;48;2;255;255;255m (artifact)                                                                                                                                [0m
            [0;38;2;51;51;51;48;2;255;255;255m  [0;38;2;3;106;7;48;2;255;255;255m"Ship ARTIFACT safely."[0;38;2;51;51;51;48;2;255;255;255m                                                                                                                                       [0m
            [0;38;2;51;51;51;48;2;255;255;255m  ([0;38;2;0;0;255;48;2;255;255;255mwhen[0;38;2;51;51;51;48;2;255;255;255m artifact (message [0;38;2;0;128;0;48;2;255;255;255m"ship %s"[0;38;2;51;51;51;48;2;255;255;255m artifact)))                                                                                                                 [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-show-org",
        "Release Control Ω",
    );
    let light_org = assert_rendered_rows(
        &pair,
        "light Org rows",
        &[
            "Release Control",
            "TODO Deploy",
            "DONE Verify",
            "runbook",
            "begin_src",
        ],
        expect![[r#"
            [0;38;2;0;142;209;48;2;234;234;255m#+title:[0;38;2;51;51;51;48;2;255;255;255m [0;1;38;2;0;0;0;48;2;255;255;255mRelease Control Ω [0;38;2;51;51;51;48;2;255;255;255m                                                                                                                                     [0m
            [0;1;38;2;60;60;60;48;2;240;240;240m* [0;1;38;2;216;171;167;48;2;255;230;228mTODO[0;1;38;2;60;60;60;48;2;240;240;240m Deploy service[0;38;2;51;51;51;48;2;255;255;255m                                                                                                                                           [0m
            [0;1;38;2;18;53;85;48;2;229;244;251m** [0;1;38;2;137;197;143;48;2;226;254;222mDONE[0;1;38;2;18;53;85;48;2;229;244;251m [0;38;2;173;173;173;48;2;229;244;251mVerify rollback[0;38;2;51;51;51;48;2;255;255;255m                                                                                                                                         [0m
            [0;38;2;51;51;51;48;2;255;255;255mRead the [0;4;38;2;0;109;175;48;2;255;255;255mrunbook[0;38;2;51;51;51;48;2;255;255;255m.                                                                                                                                               [0m
            [0;4;38;2;85;85;85;48;2;226;225;213m#+begin_src emacs-lisp                                                                                                                                          [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(&mut pair, "neomacs-leuven-tui-show-diff", "diff --git");
    let light_diff = assert_rendered_rows(
        &pair,
        "light Diff rows",
        &[
            "diff --git",
            "@@ -1,2",
            " context line",
            "-old release",
            "+new release",
        ],
        expect![[r#"
            [0;1;38;2;128;0;0;48;2;255;255;175mdiff --git a/release.el b/release.el                                                                                                                            [0m
            [0;38;2;153;0;153;48;2;255;238;255m@@ -1,2 +1,2 @@[0;38;2;51;51;51;48;2;255;255;255m                                                                                                                                                 [0m
            [0;38;2;160;161;167;48;2;255;255;255m context line                                                                                                                                                   [0m
            [0;38;2;204;51;51;48;2;255;220;224m-[0;38;2;51;51;51;48;2;255;182;186mold[0;38;2;51;51;51;48;2;254;232;233m release                                                                                                                                                    [0m
            [0;38;2;58;153;58;48;2;205;255;216m+[0;38;2;51;51;51;48;2;151;242;149mnew[0;38;2;51;51;51;48;2;221;255;221m release [0;38;2;51;51;51;48;2;151;242;149mΩ[0;38;2;51;51;51;48;2;221;255;221m                                                                                                                                                  [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(&mut pair, "neomacs-leuven-tui-show-report", "PHASE light");
    let gnu_light_report = lifecycle_report(&pair, true);
    let neo_light_report = lifecycle_report(&pair, false);
    let light_report = expect![[r##"
        PHASE light (leuven)
        SCALE-DIRECT light numeric (1.45 1.25 1.0 1.75 1.75 1.2)
        SCALE-DIRECT light nil-before-reload (1.45 1.25 1.0 1.75 1.75 1.2)
        SCALE-DIRECT light nil-after-reload (unspecified unspecified 1.0 unspecified unspecified unspecified)
        SCALE-DIRECT light default (1.8 1.3 1.0 1.6 1.6 1.1)
        SCALE-DIRECT light final-numeric (1.45 1.25 1.0 1.75 1.75 1.2)
        FACE-D default ((:foreground . "#333333") (:background . "#FFFFFF"))
        FACE-R default ((:foreground . "#333333") (:background . "#FFFFFF"))
        FACE-D font-lock-comment-face ((:foreground . "#A0A1A7") (:slant . italic))
        FACE-R font-lock-comment-face ((:foreground . "#A0A1A7") (:slant . italic))
        FACE-D font-lock-keyword-face ((:foreground . "#0000FF"))
        FACE-R font-lock-keyword-face ((:foreground . "#0000FF"))
        FACE-D font-lock-function-name-face ((:foreground . "#006699"))
        FACE-R font-lock-function-name-face ((:foreground . "#006699"))
        FACE-D diff-context ((:foreground . "#A0A1A7") (:background . unspecified))
        FACE-R diff-context ((:foreground . "#A0A1A7") (:background . "#FFFFFF"))
        FACE-D diff-header ((:foreground . "#800000") (:background . "#FFFFAF") (:weight . bold))
        FACE-R diff-header ((:foreground . "#800000") (:background . "#FFFFAF") (:weight . bold))
        FACE-D org-document-title ((:foreground . "black") (:weight . bold) (:height . 1.45))
        FACE-R org-document-title ((:foreground . "black") (:weight . bold) (:height . 1))
        FACE-D org-level-1 ((:foreground . "#3C3C3C") (:background . "#F0F0F0") (:height . 1.25))
        FACE-R org-level-1 ((:foreground . "#3C3C3C") (:background . "#F0F0F0") (:height . 1))
        FACE-D org-link ((:foreground . "#006DAF") (:underline . t))
        FACE-R org-link ((:foreground . "#006DAF") (:underline . t))
        FACE-D org-block ((:foreground . "#000088") (:background . "#FFFFE0"))
        FACE-R org-block ((:foreground . "#000088") (:background . "#FFFFE0"))
        RUN Elisp font-lock-comment-delimiter-face ";; "
        RUN Elisp font-lock-keyword-face "defun"
        RUN Elisp font-lock-function-name-face "deploy-release"
        RUN Elisp font-lock-doc-face "\"Ship ARTIFACT safely.\""
        RUN Org org-document-title "Release Control Ω\n"
        RUN Org (org-todo org-level-1) "TODO"
        RUN Org (org-done org-level-2) "DONE"
        RUN Org org-link "[[https://example.test/runbook][runbook]]"
        RUN Org org-block-begin-line "#+begin_src emacs-lisp\n"
        RUN Diff diff-header "diff --git a/release.el b/release.el\n--- "
        RUN Diff diff-hunk-header "@@ -1,2 +1,2 @@"
        RUN Diff diff-context " context line\n"
        RUN Diff diff-indicator-removed "-"
        RUN Diff diff-indicator-added "+""##]];
    light_report.assert_eq(&gnu_light_report);
    record_neo_mismatch(
        &mut neo_mismatches,
        "light applied-face and property report",
        &neo_light_report,
        &gnu_light_report,
    );

    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-use-dark",
        ";; Publish release Ω",
    );
    assert_rendered_rows(
        &pair,
        "dark Elisp rows",
        &[
            ";; Publish release",
            "defconst",
            "defun",
            "Ship ARTIFACT",
            "when artifact",
        ],
        expect![[r#"
            [0;38;2;118;114;131;48;2;37;32;42m;; [0;2;38;2;118;114;131;48;2;37;32;42mPublish release Ω after review. [0;38;2;207;204;210;48;2;37;32;42m                                                                                                                             [0m
            [0;38;2;207;204;210;48;2;37;32;42m([0;38;2;255;255;11;48;2;37;32;42mdefconst[0;38;2;207;204;210;48;2;37;32;42m [0;38;2;74;201;100;48;2;37;32;42mrelease-limit[0;38;2;207;204;210;48;2;37;32;42m 42)                                                                                                                                     [0m
            [0;38;2;207;204;210;48;2;37;32;42m([0;38;2;255;255;11;48;2;37;32;42mdefun[0;38;2;207;204;210;48;2;37;32;42m [0;38;2;255;153;111;48;2;37;32;42mdeploy-release[0;38;2;207;204;210;48;2;37;32;42m (artifact)                                                                                                                                [0m
            [0;38;2;207;204;210;48;2;37;32;42m  [0;38;2;253;149;250;48;2;37;32;42m"Ship ARTIFACT safely."[0;38;2;207;204;210;48;2;37;32;42m                                                                                                                                       [0m
            [0;38;2;207;204;210;48;2;37;32;42m  ([0;38;2;255;255;11;48;2;37;32;42mwhen[0;38;2;207;204;210;48;2;37;32;42m artifact (message [0;38;2;255;127;255;48;2;37;32;42m"ship %s"[0;38;2;207;204;210;48;2;37;32;42m artifact)))                                                                                                                 [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-show-org",
        "Release Control Ω",
    );
    assert_rendered_rows(
        &pair,
        "dark Org rows",
        &[
            "Release Control",
            "TODO Deploy",
            "DONE Verify",
            "runbook",
            "begin_src",
        ],
        expect![[r#"
            [0;38;2;255;113;56;48;2;56;51;42m#+title:[0;38;2;207;204;210;48;2;37;32;42m [0;1;38;2;255;255;255;48;2;37;32;42mRelease Control Ω [0;38;2;207;204;210;48;2;37;32;42m                                                                                                                                     [0m
            [0;1;38;2;199;195;203;48;2;50;45;55m* [0;1;38;2;44;84;98;48;2;37;55;67mTODO[0;1;38;2;199;195;203;48;2;50;45;55m Deploy service[0;38;2;207;204;210;48;2;37;32;42m                                                                                                                                           [0m
            [0;1;38;2;239;202;178;48;2;61;42;45m** [0;1;38;2;73;68;78;48;2;50;45;55mDONE[0;1;38;2;239;202;178;48;2;61;42;45m [0;38;2;87;82;92;48;2;61;42;45mVerify rollback[0;38;2;207;204;210;48;2;37;32;42m                                                                                                                                         [0m
            [0;38;2;207;204;210;48;2;37;32;42mRead the [0;4;38;2;255;146;90;48;2;37;32;42mrunbook[0;38;2;207;204;210;48;2;37;32;42m.                                                                                                                                               [0m
            [0;4;38;2;174;170;178;48;2;34;30;52m#+begin_src emacs-lisp                                                                                                                                          [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(&mut pair, "neomacs-leuven-tui-show-diff", "diff --git");
    assert_rendered_rows(
        &pair,
        "dark Diff rows",
        &[
            "diff --git",
            "@@ -1,2",
            " context line",
            "-old release",
            "+new release",
        ],
        expect![[r#"
            [0;1;38;2;131;255;255;48;2;37;32;115mdiff --git a/release.el b/release.el                                                                                                                            [0m
            [0;38;2;107;255;111;48;2;37;47;42m@@ -1,2 +1,2 @@[0;38;2;207;204;210;48;2;37;32;42m                                                                                                                                                 [0m
            [0;38;2;123;119;127;48;2;37;32;42m context line                                                                                                                                                   [0m
            [0;38;2;56;204;210;48;2;37;64;70m-[0;38;2;207;204;210;48;2;6;73;79mold[0;38;2;207;204;210;48;2;37;53;62m release                                                                                                                                                    [0m
            [0;38;2;201;102;204;48;2;83;32;78m+[0;38;2;207;204;210;48;2;109;13;115mnew[0;38;2;207;204;210;48;2;68;32;73m release [0;38;2;207;204;210;48;2;109;13;115mΩ[0;38;2;207;204;210;48;2;68;32;73m                                                                                                                                                  [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(&mut pair, "neomacs-leuven-tui-show-report", "PHASE dark");
    let gnu_dark_report = lifecycle_report(&pair, true);
    let neo_dark_report = lifecycle_report(&pair, false);
    let dark_report = expect![[r##"
        PHASE dark (leuven-dark leuven)
        SCALE-DIRECT dark numeric (1.55 1.35 1.0 1.65 1.65 1.15)
        SCALE-DIRECT dark nil-before-reload (1.55 1.35 1.0 1.65 1.65 1.15)
        SCALE-DIRECT dark nil-after-reload (unspecified unspecified 1.0 unspecified unspecified unspecified)
        SCALE-DIRECT dark default (1.8 1.3 1.0 1.6 1.6 1.1)
        SCALE-DIRECT dark final-numeric (1.55 1.35 1.0 1.65 1.65 1.15)
        FACE-D default ((:foreground . "#cfccd2") (:background . "#25202a"))
        FACE-R default ((:foreground . "#cfccd2") (:background . "#25202a"))
        FACE-D font-lock-comment-face ((:foreground . "#767283") (:slant . italic))
        FACE-R font-lock-comment-face ((:foreground . "#767283") (:slant . italic))
        FACE-D font-lock-keyword-face ((:foreground . "#ffff0b"))
        FACE-R font-lock-keyword-face ((:foreground . "#ffff0b"))
        FACE-D font-lock-function-name-face ((:foreground . "#ff996f"))
        FACE-R font-lock-function-name-face ((:foreground . "#ff996f"))
        FACE-D diff-context ((:foreground . "#7b777f") (:background . unspecified))
        FACE-R diff-context ((:foreground . "#7b777f") (:background . "#25202a"))
        FACE-D diff-header ((:foreground . "#83ffff") (:background . "#252073") (:weight . bold))
        FACE-R diff-header ((:foreground . "#83ffff") (:background . "#252073") (:weight . bold))
        FACE-D org-document-title ((:foreground . "#ffffff") (:weight . bold) (:height . 1.55))
        FACE-R org-document-title ((:foreground . "#ffffff") (:weight . bold) (:height . 1))
        FACE-D org-level-1 ((:foreground . "#c7c3cb") (:background . "#322d37") (:height . 1.35))
        FACE-R org-level-1 ((:foreground . "#c7c3cb") (:background . "#322d37") (:height . 1))
        FACE-D org-link ((:foreground . "#ff925a") (:underline . t))
        FACE-R org-link ((:foreground . "#ff925a") (:underline . t))
        FACE-D org-block ((:foreground . "#ffff7f") (:background . "#252046"))
        FACE-R org-block ((:foreground . "#ffff7f") (:background . "#252046"))
        RUN Elisp font-lock-comment-delimiter-face ";; "
        RUN Elisp font-lock-keyword-face "defun"
        RUN Elisp font-lock-function-name-face "deploy-release"
        RUN Elisp font-lock-doc-face "\"Ship ARTIFACT safely.\""
        RUN Org org-document-title "Release Control Ω\n"
        RUN Org (org-todo org-level-1) "TODO"
        RUN Org (org-done org-level-2) "DONE"
        RUN Org org-link "[[https://example.test/runbook][runbook]]"
        RUN Org org-block-begin-line "#+begin_src emacs-lisp\n"
        RUN Diff diff-header "diff --git a/release.el b/release.el\n--- "
        RUN Diff diff-hunk-header "@@ -1,2 +1,2 @@"
        RUN Diff diff-context " context line\n"
        RUN Diff diff-indicator-removed "-"
        RUN Diff diff-indicator-added "+""##]];
    dark_report.assert_eq(&gnu_dark_report);
    record_neo_mismatch(
        &mut neo_mismatches,
        "dark applied-face and property report",
        &neo_dark_report,
        &gnu_dark_report,
    );

    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-use-light",
        ";; Publish release Ω",
    );
    let restored_gnu_elisp = ansi_rows(
        &pair.gnu,
        &[
            ";; Publish release",
            "defconst",
            "defun",
            "Ship ARTIFACT",
            "when artifact",
        ],
    );
    assert_eq!(
        restored_gnu_elisp, light_elisp,
        "GNU light Elisp rendering was not restored"
    );
    let restored_neo_elisp = ansi_rows(
        &pair.neo,
        &[
            ";; Publish release",
            "defconst",
            "defun",
            "Ship ARTIFACT",
            "when artifact",
        ],
    );
    record_neo_mismatch(
        &mut neo_mismatches,
        "post-dark restored light Elisp rows",
        &restored_neo_elisp,
        &light_elisp,
    );
    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-show-org",
        "Release Control Ω",
    );
    let restored_gnu_org = ansi_rows(
        &pair.gnu,
        &[
            "Release Control",
            "TODO Deploy",
            "DONE Verify",
            "runbook",
            "begin_src",
        ],
    );
    assert_eq!(
        restored_gnu_org, light_org,
        "GNU light Org rendering was not restored"
    );
    let restored_neo_org = ansi_rows(
        &pair.neo,
        &[
            "Release Control",
            "TODO Deploy",
            "DONE Verify",
            "runbook",
            "begin_src",
        ],
    );
    record_neo_mismatch(
        &mut neo_mismatches,
        "post-dark restored light Org rows",
        &restored_neo_org,
        &light_org,
    );
    invoke_both(&mut pair, "neomacs-leuven-tui-show-diff", "diff --git");
    let restored_gnu_diff = ansi_rows(
        &pair.gnu,
        &[
            "diff --git",
            "@@ -1,2",
            " context line",
            "-old release",
            "+new release",
        ],
    );
    assert_eq!(
        restored_gnu_diff, light_diff,
        "GNU light Diff rendering was not restored"
    );
    let restored_neo_diff = ansi_rows(
        &pair.neo,
        &[
            "diff --git",
            "@@ -1,2",
            " context line",
            "-old release",
            "+new release",
        ],
    );
    record_neo_mismatch(
        &mut neo_mismatches,
        "post-dark restored light Diff rows",
        &restored_neo_diff,
        &light_diff,
    );

    invoke_both(&mut pair, "neomacs-leuven-tui-finish", "LEUVEN-TUI-CLEAN");

    let gnu_clean = exact_row(&pair.gnu, "LEUVEN-TUI-CLEAN");
    let neo_clean = exact_row(&pair.neo, "LEUVEN-TUI-CLEAN");
    let clean = expect![[
        r#"LEUVEN-TUI-CLEAN (:enabled nil :mode dark :direct ("unspecified-fg" "unspecified-bg") :resolved ("unspecified-fg" "unspecified-bg"))"#
    ]];
    clean.assert_eq(&gnu_clean);
    record_neo_mismatch(
        &mut neo_mismatches,
        "final cleanup state",
        &neo_clean,
        &gnu_clean,
    );

    assert!(
        neo_mismatches.is_empty(),
        "Leuven Theme Neo divergences:\n{}",
        neo_mismatches.join("\n\n")
    );
}
