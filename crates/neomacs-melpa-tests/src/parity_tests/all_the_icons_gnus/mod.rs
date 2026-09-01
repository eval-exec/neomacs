use std::time::Duration;

use crate::{ALL_THE_ICONS_GNUS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ALL_THE_ICONS_GNUS_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// The package rewrites Gnus's line formats to contain icon glyphs, so the
/// honest test is a real Gnus summary buffer rendered through them.  That turns
/// out to be reachable in batch without a server or a network: an `nndoc'
/// ephemeral group over an mbox file in the sandbox gives a real
/// `gnus-summary-mode' buffer with real articles.  Two settings make it
/// deterministic and are both ordinary user options -- `gnus-batch-mode', and
/// `gnus-use-byte-compile' nil, without which Gnus compiles the format spec at
/// runtime and the compilation raises "Defining as dynamic an already lexical
/// var" partway through building the summary.
///
/// Everything the workflows read they created: the mbox, the ephemeral group,
/// and the buffers.  No `.newsrc', no server, no other group is consulted.
///
/// Icon glyphs are private-use characters, so a format string is reported with
/// each glyph replaced by `<icon CODE>' and the glyphs listed separately with
/// their font family -- readable in a snapshot, and still exact.  Which glyph a
/// name maps to belongs to all-the-icons' own suite; what is pinned here is
/// which formats the package rewrites and what it puts in them.
const ALL_THE_ICONS_GNUS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'gnus-group)
(require 'gnus-sum)
(require 'gnus-topic)

(defconst aig-test-mbox
  (concat
   "From alice@example.org Mon Jan  1 10:00:00 2024\n"
   "From: Alice Adams <alice@example.org>\n"
   "To: team@example.org\n"
   "Subject: Release plan\n"
   "Date: Mon, 1 Jan 2024 10:00:00 +0000\n"
   "Message-ID: <one@example.org>\n"
   "\n"
   "Let us ship on Friday.\n"
   "\n"
   "From bob@example.org Tue Jan  2 11:30:00 2024\n"
   "From: Bob Brown <bob@example.org>\n"
   "To: team@example.org\n"
   "Subject: Re: Release plan\n"
   "Date: Tue, 2 Jan 2024 11:30:00 +0000\n"
   "Message-ID: <two@example.org>\n"
   "References: <one@example.org>\n"
   "\n"
   "Friday works for me.\n"
   "\n")
  "Two real messages, the second a reply, so the summary is threaded.")

(defun aig-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun aig-test-write (name text)
  (let ((path (aig-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun aig-test-render (value)
  "Report VALUE with icon glyphs replaced by markers, plus their fonts."
  (if (not (stringp value))
      value
    (let ((plain (substring-no-properties value))
          (icons nil)
          (rendered "")
          (position 0))
      (while (< position (length plain))
        (let ((character (aref plain position))
              (face (get-text-property position 'face value)))
          (if (>= character #xE000)
              (progn
                (push (list position character
                            (and (listp face) (plist-get face :family)))
                      icons)
                (setq rendered (concat rendered (format "<icon %d>" character))))
            (setq rendered (concat rendered (string character)))))
        (setq position (1+ position)))
      (list rendered (nreverse icons)))))

(defun aig-test-formats ()
  "Report every Gnus format variable the package rewrites."
  (list :summary (aig-test-render gnus-summary-line-format)
        :group (aig-test-render gnus-group-line-format)
        :topic (aig-test-render gnus-topic-line-format)
        :user-date (aig-test-render (cdr (assq t gnus-user-date-format-alist)))
        :tree-root (aig-test-render gnus-sum-thread-tree-root)
        :tree-false-root (aig-test-render gnus-sum-thread-tree-false-root)
        :tree-vertical (aig-test-render gnus-sum-thread-tree-vertical)
        :tree-single-leaf (aig-test-render gnus-sum-thread-tree-single-leaf)))

(defun aig-test-prepare-gnus ()
  "Point Gnus entirely at the sandbox and make its output deterministic."
  (setq gnus-home-directory (aig-test-path "")
        gnus-directory (aig-test-path "News/")
        message-directory (aig-test-path "Mail/")
        gnus-startup-file (aig-test-path ".newsrc")
        gnus-init-file (aig-test-path "gnus-init.el")
        gnus-select-method '(nnnil "")
        gnus-secondary-select-methods nil
        gnus-verbose 0
        gnus-batch-mode t
        ;; Without this Gnus byte-compiles the format spec at runtime, and the
        ;; compilation aborts partway through building the summary.
        gnus-use-byte-compile nil))

(defun aig-test-open-mbox (name path)
  "Open the mbox at PATH as an ephemeral nndoc group."
  (gnus-group-read-ephemeral-group
   name
   (list 'nndoc path (list 'nndoc-address path) '(nndoc-article-type mbox))))

(defun aig-test-summary-buffer ()
  (cl-find-if (lambda (buffer) (string-prefix-p "*Summary" (buffer-name buffer)))
              (buffer-list)))

(defun aig-test-summary-render ()
  (let ((buffer (aig-test-summary-buffer)))
    (if (null buffer)
        'no-summary-buffer
      (with-current-buffer buffer
        (list :mode major-mode
              :lines (mapcar #'aig-test-render
                             (split-string (buffer-string) "\n" t)))))))

(defun aig-test-kill-gnus-buffers ()
  (dolist (buffer (buffer-list))
    (when (string-match-p "\\*Summary\\|\\*Article\\|nndoc\\|nntpd\\|gnus"
                          (buffer-name buffer))
      (let ((kill-buffer-query-functions nil))
        (ignore-errors (kill-buffer buffer))))))

(defun aig-test-compositions ()
  "Return the composed regions and their faces in the current buffer."
  (let ((position (point-min))
        (result nil))
    (while (< position (point-max))
      (let ((next (next-single-property-change position 'composition nil (point-max))))
        (when (get-text-property position 'composition)
          (push (list (buffer-substring-no-properties position next)
                      (get-text-property position 'face))
                result))
        (setq position next)))
    (nreverse result)))
"##;

fn all_the_icons_gnus_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALL_THE_ICONS_GNUS_MELPA_PIN, "all-the-icons-gnus.el")
        .expect("prepare pinned all-the-icons-gnus source below ./tmp")
        .with_prelude(ALL_THE_ICONS_GNUS_TEST_PRELUDE)
        .with_timeout(ALL_THE_ICONS_GNUS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed all-the-icons-gnus parity test")
        .into()
}

/// Multi-probe batch for `assert_all_the_icons_gnus_parity` cases (2a).
pub(crate) fn assert_all_the_icons_gnus_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        all_the_icons_gnus_oracle(),
        &name,
        "all_the_icons_gnus_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn all_the_icons_gnus_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_all_the_icons_gnus_batch(&cases);
}

// END generated package batch tests
