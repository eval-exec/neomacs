use std::time::Duration;

use crate::{ADO_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ADO_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Fixtures shared by the workflows.
///
/// ado-mode is a large, old major mode for Stata do-files and ado-files.  Its
/// editing half -- twenty `auto-mode-alist' extensions, a brace-aware indenter,
/// `///' continuation handling, its own font-lock faces, command motion, macro
/// quoting and an opt-in imenu index -- is all in-process Elisp and is what
/// these workflows drive, on real files visited with `find-file-noselect'.
///
/// The half that talks to Stata is **not** covered, because it cannot be
/// covered honestly on this host: `ado-send-command-to-stata' and its siblings
/// drive a *running* Stata through `osascript' on macOS, a bundled
/// `send2stata.exe' on Windows and `send2ztata.sh' (xdotool) on GNU/Linux, and
/// no Stata is installed here.  Stubbing those would assert nothing about the
/// package.
///
/// Two pieces of setup are needed and both use the package's own knobs.
/// `ado-personal-dir' and `ado-new-dir' are set to a sandbox directory: with
/// neither set and no Stata to ask, the mode body signals "Could not find
/// Console Stata" partway through and never reaches `run-mode-hooks'.  Their
/// docstrings say they should be "set by hand ... to a directory in the Stata
/// ado-path", which is exactly what a user without Stata in the default
/// location must do.  Directory-local variables are switched off because the
/// sandbox lives below the Neomacs checkout, whose `.dir-locals.el' sets
/// `tab-width' to 8 for every mode and would mask the mode's own value.
const ADO_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

;; The per-case sandbox lives below the Neomacs checkout, whose `.dir-locals.el'
;; sets `tab-width' to 8 for every mode and would mask the mode's own setting.
(setq enable-dir-local-variables nil)

(defvar ado-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

;; Without Stata installed, ado-mode's setup asks Stata where PERSONAL is and
;; the mode body aborts.  Both directories are documented defcustoms meant to be
;; "set by hand ... to a directory in the Stata ado-path", which is what a user
;; without Stata in the default location has to do.
(defun ado-test-configure ()
  (let ((personal (expand-file-name "stata/personal/" ado-test-root)))
    (make-directory personal t)
    (setq ado-personal-dir personal
          ado-new-dir personal
          ado-add-sysdir-font-lock nil)))

(defconst ado-test-program
  "*! version 1.0.0  01jan2020
program define mysum, rclass
	version 16
	syntax varlist(numeric) [if] [in] [, Detail]
	marksample touse
	quietly summarize `varlist' if `touse', `detail'
	if `r(N)' == 0 {
		display as error \"no observations\"
		exit 2000
	}
	else {
		display as text \"mean = \" as result %9.4f `r(mean)'
	}
	return scalar mean = `r(mean)'
end
")

(defun ado-test-write (name text)
  (let ((path (expand-file-name name ado-test-root)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defun ado-test-visit (name text)
  (ado-test-configure)
  (let ((buffer (find-file-noselect (ado-test-write name text))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defun ado-test-face-runs ()
  (font-lock-ensure)
  (let ((position (point-min))
        (runs nil))
    (while (< position (point-max))
      (let ((next (next-single-property-change position 'face nil (point-max))))
        (push (cons (buffer-substring-no-properties position next)
                    (get-text-property position 'face))
              runs)
        (setq position next)))
    (nreverse runs)))

(defun ado-test-text ()
  (buffer-substring-no-properties (point-min) (point-max)))
"##;

fn ado_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ADO_MODE_MELPA_PIN, "ado-mode.el")
        .expect("prepare pinned ado-mode source below ./tmp")
        .with_prelude(ADO_MODE_TEST_PRELUDE)
        .with_timeout(ADO_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ado-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_ado_mode_parity` cases (2a).
pub(crate) fn assert_ado_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ado_mode_oracle(), &name, "ado_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ado_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ado_mode_batch(&cases);
}

// END generated package batch tests
