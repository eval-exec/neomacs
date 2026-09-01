use std::time::Duration;

use crate::{AC_ETAGS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_ETAGS_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// A real project, a real etags-format TAGS file on disk, the setup the
/// README prescribes (`ac-etags-setup' then `ac-etags-ac-setup'), and
/// completion driven end to end through `ac-start' / `ac-update' /
/// `ac-complete' in a window-displayed buffer.
///
/// The tags tables are selected with the real `visit-tags-table' command.
/// etags asks two questions along the way - "Keep current list of tags tables
/// also?" and "Tags file has changed, read new contents?" - and both have a
/// documented user option that answers them in advance, so no prompt is
/// stubbed and nothing blocks: `tags-add-tables' decides the first and
/// `tags-revert-without-query' the second.
const AC_ETAGS_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'auto-complete)
(require 'etags)

;; Two real projects.  Several tags share the "bank_" prefix across different
;; source files, and one is a Unicode identifier, so completion is exercised
;; on multibyte text as well.
(defconst ac-etags-test-bank-entries
  '(("src/bank.c"
     ("int bank_open(void)" "bank_open" 1 0)
     ("int bank_close(void)" "bank_close" 2 35)
     ("int bank_transfer(int amount)" "bank_transfer" 3 71))
    ("src/util.c"
     ("int util_hash(const char *s)" "util_hash" 1 0)
     ("int bank_audit(void)" "bank_audit" 2 43))
    ("src/report.js"
     ("function bank_überweisung(betrag)" "bank_überweisung" 1 0))))

(defconst ac-etags-test-ledger-entries
  '(("lib/ledger.c"
     ("void bank_reconcile(void)" "bank_reconcile" 1 0)
     ("void bank_settle(void)" "bank_settle" 2 30))))

(defun ac-etags-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ac-etags-test-write (name text)
  (let ((path (ac-etags-test-path name))
        (coding-system-for-write 'utf-8-unix))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun ac-etags-test-tags-file (name entries)
  "Write a real etags-format TAGS file for ENTRIES and return its path.
ENTRIES is ((SOURCE (PATTERN TAG LINE OFFSET)...)...).  This is the format
the etags program emits, so no external binary is needed."
  (ac-etags-test-write
   name
   (mapconcat
    (lambda (entry)
      (let ((body (mapconcat
                   (lambda (tag)
                     (format "%s\177%s\001%d,%d\n"
                             (nth 0 tag) (nth 1 tag) (nth 2 tag) (nth 3 tag)))
                   (cdr entry) "")))
        (format "\f\n%s,%d\n%s" (car entry) (string-bytes body) body)))
    entries "")))

(defmacro ac-etags-test-in-buffer (&rest body)
  "Run BODY in a window-displayed buffer set up the way the README says."
  (declare (indent 0))
  `(let ((buffer (generate-new-buffer "*ac-etags-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (setq ac-sources nil)
           (ac-etags-setup)
           (ac-etags-ac-setup)
           ,@body)
       (kill-buffer buffer))))

(defmacro ac-etags-test-with-etags (add-tables &rest body)
  "Run BODY with private etags state and no etags confirmation prompts.
ADD-TABLES becomes `tags-add-tables': nil makes every `visit-tags-table'
start a fresh list, t makes it keep the tables already selected."
  (declare (indent 1))
  `(let ((tags-table-list nil)
         (tags-file-name nil)
         (tags-add-tables ,add-tables)
         (tags-revert-without-query t))
     ,@body))

(defun ac-etags-test-table-names ()
  "Return `tags-table-list' as the project directory names holding each TAGS."
  (mapcar (lambda (path)
            (file-name-nondirectory
             (directory-file-name (file-name-directory path))))
          tags-table-list))

(defun ac-etags-test-candidates ()
  "Start completion at point and return the plain candidate strings."
  (ac-start :force-init t)
  (ac-update t)
  (mapcar #'substring-no-properties ac-candidates))

(defun ac-etags-test-candidate-properties ()
  "Return each candidate with the text properties auto-complete attached."
  (mapcar (lambda (candidate)
            (list (substring-no-properties candidate)
                  (get-text-property 0 'symbol candidate)
                  (get-text-property 0 'popup-face candidate)
                  (get-text-property 0 'selection-face candidate)))
          ac-candidates))

(defun ac-etags-test-cache-entries ()
  "Return `ac-etags--completion-cache' as an alist sorted by prefix."
  (let (entries)
    (maphash (lambda (key value) (push (cons key value) entries))
             ac-etags--completion-cache)
    (sort entries (lambda (a b) (string< (car a) (car b))))))
"###;

fn ac_etags_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_ETAGS_MELPA_PIN, "ac-etags.el")
        .expect("prepare pinned ac-etags source below ./tmp")
        .with_prelude(AC_ETAGS_TEST_PRELUDE)
        .with_timeout(AC_ETAGS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-etags parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_etags_parity` cases (2a).
pub(crate) fn assert_ac_etags_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_etags_oracle(), &name, "ac_etags_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_etags_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_etags_batch(&cases);
}

// END generated package batch tests
