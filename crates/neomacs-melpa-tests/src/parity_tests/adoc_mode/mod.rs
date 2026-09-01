use std::time::Duration;

use crate::{ADOC_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ADOC_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Two real AsciiDoc documents plus sandbox helpers.
///
/// adoc-mode is font-lock heavy: nearly everything a user sees comes from
/// keywords that run over multi-line constructs, so the fixtures are written to
/// contain the markup people actually type -- a document header with attribute
/// entries, three title levels, constrained bold/italic/monospace, an inline
/// link, ordered and unordered lists, admonition paragraphs, an attribute
/// reference and a `[source,ruby]' block whose body the mode fontifies with
/// Ruby's own keywords.  The files are written into the per-case sandbox and
/// visited with `find-file-noselect', so `auto-mode-alist' chooses the mode.
///
/// `transient-mark-mode' is enabled where a workflow selects a region: it is on
/// by default for interactive users but off in batch, and the styling commands
/// take a different branch when there is no active region.
const ADOC_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst adoc-test-guide
  (concat
   "= Field Guide to Widgets\n"
   "Jane Roe <jane@example.org>\n"
   ":toc: left\n"
   ":sourcedir: ./src\n"
   "\n"
   "A short *bold* intro with _italic_ and `monospace` text, plus a\n"
   "https://example.org/widgets[widget catalogue] link.\n"
   "\n"
   "== Getting Started\n"
   "\n"
   "NOTE: Install the toolchain before you begin.\n"
   "\n"
   ". Download the archive\n"
   ". Unpack it\n"
   "\n"
   "* First bullet\n"
   "* Second bullet\n"
   "\n"
   "[source,ruby]\n"
   "----\n"
   "def widget(name)\n"
   "  puts \"building\"\n"
   "end\n"
   "----\n"
   "\n"
   "=== Configuration\n"
   "\n"
   "The sourcedir attribute points at {sourcedir}.\n"
   "\n"
   "== Troubleshooting\n"
   "\n"
   "WARNING: Never edit the generated files.\n"))

(defconst adoc-test-unicode
  (concat
   "= 日本語ハンドブック\n"
   ":author: Renée Dupré\n"
   "\n"
   "== Café Notes — Grüße\n"
   "\n"
   "この段落は *太字* と _斜体_ と `等幅` を含みます。ASCII mixed in.\n"
   "\n"
   "TIP: Les accents é, è, ê et ü doivent rester intacts.\n"
   "\n"
   "* 項目 1\n"
   "* Élément 2\n"
   "\n"
   "=== Ünicode Anhang\n"
   "\n"
   "Une phrase française assez longue pour être remplie sur plusieurs lignes par la commande de remplissage standard d'Emacs sans problème.\n"))

(defun adoc-test-path (name)
  "Return the absolute sandbox path of NAME."
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun adoc-test-write (name text)
  "Write TEXT to sandbox file NAME and return its path."
  (let ((path (adoc-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun adoc-test-open (name text)
  "Visit a sandbox file holding TEXT, display it, and return its buffer."
  (let ((buffer (find-file-noselect (adoc-test-write name text))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defun adoc-test-face-at (needle &optional offset)
  "Return the face at NEEDLE's start plus OFFSET, or `not-found'."
  (save-excursion
    (goto-char (point-min))
    (if (search-forward needle nil t)
        (get-text-property (+ (match-beginning 0) (or offset 0)) 'face)
      'not-found)))

(defun adoc-test-faces (specs)
  "Return (NEEDLE . FACE) for every (NEEDLE OFFSET) in SPECS."
  (font-lock-ensure)
  (mapcar (lambda (spec)
            (cons (car spec) (adoc-test-face-at (car spec) (cadr spec))))
          specs))

(defun adoc-test-face-runs (start end)
  "Return the (TEXT . FACE) runs font lock produced between START and END."
  (font-lock-ensure)
  (let ((position start)
        (runs nil))
    (while (< position end)
      (let ((next (next-single-property-change position 'face nil end))
            (face (get-text-property position 'face)))
        (push (cons (buffer-substring-no-properties position next) face) runs)
        (setq position next)))
    (nreverse runs)))

(defun adoc-test-plain (value)
  "Return VALUE with every string stripped of its text properties."
  (cond ((stringp value) (substring-no-properties value))
        ((consp value) (cons (adoc-test-plain (car value))
                             (adoc-test-plain (cdr value))))
        (t value)))

(defun adoc-test-line ()
  "Return the current line without properties."
  (buffer-substring-no-properties (line-beginning-position)
                                  (line-end-position)))

(defun adoc-test-lines (count)
  "Return COUNT lines starting at the current one, without properties."
  (buffer-substring-no-properties (line-beginning-position)
                                  (line-end-position count)))

(defun adoc-test-where ()
  "Report where point is and what the line looks like."
  (list :point (point)
        :column (current-column)
        :line-number (line-number-at-pos)
        :line (adoc-test-line)))
"##;

fn adoc_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ADOC_MODE_MELPA_PIN, "adoc-mode.el")
        .expect("prepare pinned adoc-mode source below ./tmp")
        .with_prelude(ADOC_MODE_TEST_PRELUDE)
        .with_timeout(ADOC_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed adoc-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_adoc_mode_parity` cases (2a).
pub(crate) fn assert_adoc_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(adoc_mode_oracle(), &name, "adoc_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn adoc_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_adoc_mode_batch(&cases);
}

// END generated package batch tests
