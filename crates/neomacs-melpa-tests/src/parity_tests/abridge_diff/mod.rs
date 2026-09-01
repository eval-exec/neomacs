use std::time::Duration;

use crate::{ABRIDGE_DIFF_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ABRIDGE_DIFF_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// abridge-diff advises `smerge-refine-regions', so every workflow has to
/// produce a *real* refinement: real files on disk, the real `diff' program,
/// a real `diff-mode' or `smerge-mode' buffer, and the package's real public
/// commands.  The helpers below only build those fixtures and report what the
/// user can observe; none of them reimplements any part of the package.
const ABRIDGE_DIFF_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'diff)
(require 'diff-mode)
(require 'smerge-mode)

;; A LaTeX-ish paper with paragraph-per-line text: exactly the case the
;; package README describes as unreadable without abridging.  The three
;; changes sit at the start, the middle and the very end of their lines so a
;; single hunk shows three different abridging shapes at once.
(defconst abridge-diff-test-paper-old
  (concat
   "\\section{Résultats}\n"
   "The naïve estimator converges slowly whenever the sampling density is uneven, and the resulting curves stay noisy near the boundary of the domain, which makes any comparison with the reference solution hard to interpret.\n"
   "Every measurement was repeated three times.\n"
   "The Grüneisen parameter γ was held fixed at 1.85 for the whole sweep, and the residuals were accumulated over the full temperature range from 4 K up to 300 K without any smoothing.\n"
   "We thank the anonymous reviewers for their comments.\n"
   "Figure 3 shows the same data on a logarithmic axis, together with the analytic prediction of Eq. (7) and the two bootstrap confidence bands computed from ten thousand resamples of the raw counts.\n"))

(defconst abridge-diff-test-paper-new
  (concat
   "\\section{Résultats}\n"
   "The refined estimator converges slowly whenever the sampling density is uneven, and the resulting curves stay noisy near the boundary of the domain, which makes any comparison with the reference solution hard to interpret.\n"
   "Every measurement was repeated three times.\n"
   "The Grüneisen parameter γ was held fixed at 2.10 for the whole sweep, and the residuals were accumulated over the full temperature range from 4 K up to 300 K without any smoothing.\n"
   "We thank the anonymous reviewers for their comments.\n"
   "Figure 3 shows the same data on a logarithmic axis, together with the analytic prediction of Eq. (7) and the two bootstrap confidence bands computed from ten thousand resamples of the raw counts and their weights.\n"))

;; One long changelog entry with a single early word change, small enough that
;; a whole rendered line fits in an expectation.
(defconst abridge-diff-test-changelog-old
  "Fixed a crash in the exporter when the document contained nested tables with merged cells inside a footnote.\n")

(defconst abridge-diff-test-changelog-new
  "Fixed a hang in the exporter when the document contained nested tables with merged cells inside a footnote.\n")

;; Release notes whose two edits are nine lines apart, so `diff' emits two
;; hunks: one modification and one pure insertion.
(defconst abridge-diff-test-notes-old
  (concat
   "# Release notes\n"
   "The installer now verifies the checksum of every downloaded artifact before it is unpacked, and it refuses to continue whenever the signature does not match the published manifest.\n"
   "\n"
   "## Compatibility\n"
   "The minimum supported version is unchanged.\n"
   "\n"
   "## Known issues\n"
   "Windows builds still report the wrong locale.\n"
   "\n"
   "## Credits\n"
   "Thanks to everyone who filed a report.\n"))

(defconst abridge-diff-test-notes-new
  (concat
   "# Release notes\n"
   "The installer now validates the checksum of every downloaded artifact before it is unpacked, and it refuses to continue whenever the signature does not match the published manifest.\n"
   "\n"
   "## Compatibility\n"
   "The minimum supported version is unchanged.\n"
   "\n"
   "## Known issues\n"
   "Windows builds still report the wrong locale.\n"
   "\n"
   "## Credits\n"
   "Thanks to everyone who filed a report.\n"
   "Special thanks to the translators of the Ελληνικά and 日本語 catalogues.\n"))

;; A real unresolved merge conflict, refined through `smerge-refine'.
(defconst abridge-diff-test-conflict
  (concat
   "# Deployment checklist\n"
   "<<<<<<< HEAD\n"
   "Run the migration script against the staging database before touching production, and keep the maintenance banner up until the smoke tests finish.\n"
   "Notify the on-call engineer in the release channel.\n"
   "=======\n"
   "Run the migration script against the staging database before touching production, and keep the maintenance banner up until every smoke test finishes.\n"
   "Notify the on-call engineer in the release channel.\n"
   ">>>>>>> feature/rollout\n"
   "Archive the build artifacts.\n"))

(defun abridge-diff-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun abridge-diff-test-write (name text)
  "Write TEXT to sandbox file NAME as UTF-8 and return its absolute path."
  (let ((path (abridge-diff-test-path name))
        (coding-system-for-write 'utf-8-unix))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun abridge-diff-test-unified-diff (directory basename old new)
  "Write two real revisions of BASENAME and return real `diff -u' output."
  (let ((old-path (abridge-diff-test-write
                   (concat directory "/old/" basename) old))
        (new-path (abridge-diff-test-write
                   (concat directory "/new/" basename) new))
        (coding-system-for-read 'utf-8-unix))
    (with-temp-buffer
      (call-process diff-command nil t nil
                    "-u" "--label" (concat "a/" basename)
                    "--label" (concat "b/" basename)
                    old-path new-path)
      (buffer-string))))

(defmacro abridge-diff-test-with-buffer (text &rest body)
  "Run BODY in a live window-displayed buffer holding TEXT."
  (declare (indent 1))
  `(let ((buffer (generate-new-buffer "*abridge-diff-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (insert ,text)
           (goto-char (point-min))
           ,@body)
       (kill-buffer buffer))))

(defun abridge-diff-test-rendered ()
  "Return the buffer's lines as displayed under `buffer-invisibility-spec'.
A run hidden with an ellipsis is rendered as the \"...\" the user sees."
  (let ((rendered "")
        (position (point-min)))
    (while (< position (point-max))
      (let* ((value (get-text-property position 'invisible))
             (next (next-single-property-change position 'invisible nil (point-max)))
             (state (invisible-p value)))
        (setq rendered
              (concat rendered
                      (cond ((eq state t) "")
                            (state "...")
                            (t (buffer-substring-no-properties position next)))))
        (setq position next)))
    (split-string rendered "\n")))

(defun abridge-diff-test-hidden ()
  "Return every `invisible' run as (BEG END VALUE TEXT)."
  (let (runs (position (point-min)))
    (while (< position (point-max))
      (let ((value (get-text-property position 'invisible))
            (next (next-single-property-change position 'invisible nil (point-max))))
        (when value
          (push (list position next value
                      (buffer-substring-no-properties position next))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun abridge-diff-test-refined-overlays (property value)
  "Return refinement overlays carrying PROPERTY VALUE, in buffer order.
Each entry is (BEG END FACE REGION-OVERLAY-P TEXT); these are the overlays
abridge-diff reads to decide what to keep visible."
  (mapcar (lambda (overlay)
            (list (overlay-start overlay)
                  (overlay-end overlay)
                  (or (overlay-get overlay 'face)
                      (overlay-get overlay 'font-lock-face))
                  (and (overlay-get overlay 'smerge--refine-region) t)
                  (buffer-substring-no-properties
                   (overlay-start overlay) (overlay-end overlay))))
          (sort (seq-filter (lambda (overlay)
                              (eq (overlay-get overlay property) value))
                            (overlays-in (point-min) (point-max)))
                (lambda (a b)
                  (or (< (overlay-start a) (overlay-start b))
                      (and (= (overlay-start a) (overlay-start b))
                           (< (overlay-end a) (overlay-end b))))))))
"###;

fn abridge_diff_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ABRIDGE_DIFF_MELPA_PIN, "abridge-diff.el")
        .expect("prepare pinned abridge-diff source below ./tmp")
        .with_prelude(ABRIDGE_DIFF_TEST_PRELUDE)
        .with_timeout(ABRIDGE_DIFF_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed abridge-diff parity test")
        .into()
}

/// Multi-probe batch for `assert_abridge_diff_parity` cases (2a).
pub(crate) fn assert_abridge_diff_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(abridge_diff_oracle(), &name, "abridge_diff_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn abridge_diff_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_abridge_diff_batch(&cases);
}

// END generated package batch tests
