use std::time::Duration;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, F_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const F_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// f is a path and filesystem library, so the workflows build a real project
/// tree in the sandbox and walk, read, write, copy and delete it through f's
/// own API, asserting the complete listings and the bytes on disk.
///
/// Every fixture name is ASCII on purpose.  Catalogued divergence 6 --
/// `directory-files' returning undecoded bytes -- sits directly under
/// `f-entries', `f-files' and `f-glob', and a non-ASCII tree makes all three
/// disagree.  Checked before designing it out: `f' adds no distinct failure
/// of its own.  With `Lösung' and `Lösungen' side by side, `f-uniquify'
/// still picks the correct distinguishing components in both editors; only
/// the encoding of the names differs, which is entry 6 and nothing more.  A
/// red test here would restate that entry with `f-' in front of it.
const F_TEST_PRELUDE: &str = r##"
(require 'seq)

(defun f-test-plain (value)
  (cond ((stringp value) (substring-no-properties value))
        ((consp value) (cons (f-test-plain (car value)) (f-test-plain (cdr value))))
        (t value)))

(defun f-test-root ()
  (f-join (getenv "NEOMACS_TEST_SANDBOX_ROOT") "tree"))

(defun f-test-build ()
  "Create a small but realistic project tree and return its root."
  (let ((root (f-test-root)))
    (when (f-directory-p root) (f-delete root t))
    (dolist (directory '("src/core" "src/util" "docs" ".git"))
      (f-mkdir-full-path (f-join root directory)))
    (dolist (entry '(("README.md" . "# Project\n")
                     ("src/core/engine.el" . "(provide 'engine)\n")
                     ("src/core/engine-test.el" . "(require 'engine)\n")
                     ("src/util/strings.el" . "(provide 'strings)\n")
                     ("docs/guide.md" . "guide\n")
                     (".hidden" . "secret\n")
                     (".git/config" . "[core]\n")))
      (f-write-text (cdr entry) 'utf-8 (f-join root (car entry))))
    root))

(defun f-test-relative (root paths)
  "Return PATHS relative to ROOT and sorted, so listings compare cleanly."
  (sort (mapcar (lambda (path) (substring-no-properties (f-relative path root)))
                paths)
        #'string<))

(defun f-test-tree (root)
  "Return every entry below ROOT, each marked as a file or a directory."
  (mapcar (lambda (path)
            (cons (substring-no-properties (f-relative path root))
                  (if (f-directory-p path) 'directory 'file)))
          (sort (f-entries root nil t) #'string<)))
"##;

fn f_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(F_MELPA_PIN, "f.el")
        .expect("prepare pinned f source and dependencies below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash source below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s source below ./tmp")
        .with_prelude(F_TEST_PRELUDE)
        .with_timeout(F_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed f parity test").into()
}

pub(crate) fn assert_f_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(f_oracle(), &name, "f_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn f_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::f_practical_workflows_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_f_batch(&cases);
}

// END generated package batch tests
