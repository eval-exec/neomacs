use std::time::Duration;

use crate::{ALL_THE_ICONS_IBUFFER_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALL_THE_ICONS_IBUFFER_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// all-the-icons-ibuffer defines four Ibuffer columns -- `icon`, `size-h`,
/// `mode+` and `filename-and-process+` -- and a minor mode that swaps
/// `ibuffer-formats` to a layout using them.  These workflows create real
/// buffers, render a real Ibuffer over them and drive the real minor mode.
///
/// Two deliberate constraints.  Ibuffer renders `buffer-list`, whose order and
/// contents are catalogue entry 13, so every fixture buffer is named `atib-*`,
/// the listing is filtered to that prefix, and every assertion is sorted by
/// buffer name -- nothing depends on buffers this suite did not create.  And
/// as in the dired suite, the icon glyph and its font family are
/// all-the-icons' own surface: the icon column is described structurally
/// (a non-ASCII glyph carrying display and face properties, followed by the
/// package's half-width spacer) and no codepoint or family is ever named.
const ALL_THE_ICONS_IBUFFER_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'ibuffer)

(setq make-backup-files nil create-lockfiles nil)

(defvar atib-test-root (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun atib-test-write (name text)
  (let ((path (expand-file-name name atib-test-root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun atib-test-make-buffers ()
  "Create the fixture buffers.  Every name starts with `atib-' so the ibuffer
listing can be filtered down to exactly these, which keeps the workflows
independent of `buffer-list' order and contents (catalogue entry 13)."
  (let ((el (atib-test-write "atib-code.el" ";; Grüße\n(defun f () 1)\n"))
        (py (atib-test-write "atib-script.py" "print('hallo')\n")))
    (find-file-noselect el)
    (find-file-noselect py)
    (with-current-buffer (get-buffer-create "atib-plain")
      (fundamental-mode)
      (erase-buffer)
      (insert "einfacher Text\n"))
    (with-current-buffer (get-buffer-create "atib-large")
      (fundamental-mode)
      (erase-buffer)
      (insert (make-string 2048 ?x)))
    (with-current-buffer (get-buffer-create "atib-org")
      (org-mode)
      (erase-buffer)
      (insert "* Notiz\n"))
    (list el py)))

(defun atib-test-kill-buffers ()
  (let ((kill-buffer-query-functions nil))
    (dolist (buffer (buffer-list))
      (when (string-prefix-p "atib-" (buffer-name buffer))
        (with-current-buffer buffer (set-buffer-modified-p nil))
        (kill-buffer buffer)))))

(defun atib-test-describe-cell (text)
  "Describe a rendered column cell structurally, never naming a font family.
The icon glyph and its family are all-the-icons' surface, covered there."
  (let ((plain (substring-no-properties text)))
    (list (copy-sequence plain)
          (length plain)
          (if (text-properties-at 0 text) 'props 'plain))))

(defun atib-test-lines ()
  "Every fixture line as (NAME . COLUMN-CELLS), sorted by name so the result
does not depend on `buffer-list' order."
  (save-excursion
    (goto-char (point-min))
    (let (rows)
      (while (not (eobp))
        (let ((line (buffer-substring (line-beginning-position) (line-end-position))))
          (when (string-match-p "atib-" line)
            (push (copy-sequence (substring-no-properties line)) rows)))
        (forward-line 1))
      (sort rows #'string<))))

(defun atib-test-icon-cells ()
  "For each fixture line: the buffer name and the shape of its icon column.
Describes the glyph structurally -- that it is a non-ASCII character carrying
display and face properties -- and never reports which codepoint or family
all-the-icons chose, because that mapping is all-the-icons' own surface."
  (save-excursion
    (goto-char (point-min))
    (let (rows)
      (while (not (eobp))
        (let ((buffer (ibuffer-current-buffer)))
          (when (and buffer (string-prefix-p "atib-" (buffer-name buffer)))
            (let* ((bol (line-beginning-position))
                   (glyph (char-after (+ bol 5)))
                   (spacer (char-after (+ bol 6))))
              (push (list (copy-sequence (buffer-name buffer))
                          (if (and glyph (> glyph 127)) 'icon-glyph glyph)
                          (if (get-text-property (+ bol 5) 'display) 'display 'no-display)
                          (if (get-text-property (+ bol 5) 'font-lock-face) 'face 'no-face)
                          spacer
                          (copy-tree (get-text-property (+ bol 6) 'display)))
                    rows))))
        (forward-line 1))
      (sort rows (lambda (a b) (string< (car a) (car b)))))))

(defun atib-test-columns ()
  "Each fixture line split into its rendered columns, name first."
  (save-excursion
    (goto-char (point-min))
    (let (rows)
      (while (not (eobp))
        (let ((buffer (ibuffer-current-buffer)))
          (when (and buffer (string-prefix-p "atib-" (buffer-name buffer)))
            (push (cons (copy-sequence (buffer-name buffer))
                        (mapcar (lambda (field) (copy-sequence field))
                                (split-string
                                 (substring-no-properties
                                  (buffer-substring (+ (line-beginning-position) 7)
                                                    (line-end-position)))
                                 "  +" t)))
                  rows)))
        (forward-line 1))
      (sort rows (lambda (a b) (string< (car a) (car b)))))))

(defmacro atib-test-in-ibuffer (&rest body)
  "Render an ibuffer restricted to the fixture buffers, in a stable order."
  `(let ((all-the-icons-ibuffer-display-predicate (lambda () t))
         buffer)
     (unwind-protect
         (progn
           (atib-test-make-buffers)
           (ibuffer nil "*atib-ibuffer*" '((name . "^atib-")) t)
           (setq buffer (get-buffer "*atib-ibuffer*"))
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           ,@body)
       (let ((kill-buffer-query-functions nil))
         (when (buffer-live-p buffer) (kill-buffer buffer)))
       (atib-test-kill-buffers))))
"##;

fn all_the_icons_ibuffer_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALL_THE_ICONS_IBUFFER_MELPA_PIN, "all-the-icons-ibuffer.el")
        .expect("prepare pinned all-the-icons-ibuffer source below ./tmp")
        .with_prelude(ALL_THE_ICONS_IBUFFER_TEST_PRELUDE)
        .with_timeout(ALL_THE_ICONS_IBUFFER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed all-the-icons-ibuffer parity test")
        .into()
}

/// Multi-probe batch for `assert_all_the_icons_ibuffer_parity` cases (2a).
pub(crate) fn assert_all_the_icons_ibuffer_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        all_the_icons_ibuffer_oracle(),
        &name,
        "all_the_icons_ibuffer_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn all_the_icons_ibuffer_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_all_the_icons_ibuffer_batch(&cases);
}

// END generated package batch tests
