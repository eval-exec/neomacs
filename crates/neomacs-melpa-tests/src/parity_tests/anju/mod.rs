use std::time::Duration;

use crate::{ANJU_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod context_menu;
mod initialization;
mod mode_line;
mod registry;
mod style_text;
mod utils;
mod workflows;

const ANJU_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ANJU_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun anju-test-menu-entries (menu)
  (let (result)
    (map-keymap
     (lambda (event item)
       (when (eq (car-safe item) 'menu-item)
         (let ((definition (nth 2 item))
               (properties (nthcdr 3 item)))
           (push
            (list
             event
             (nth 1 item)
             (if (keymapp definition) '<submenu> definition)
             :enable (plist-get properties :enable)
             :visible (plist-get properties :visible)
             :style (plist-get properties :style)
             :selected (plist-get properties :selected)
             :help (plist-get properties :help))
            result))))
     menu)
    (nreverse result)))

(defun anju-test-menu-labels (menu)
  (mapcar #'cadr (anju-test-menu-entries menu)))

(defun anju-test-buffer (name mode directory)
  (let ((buffer (get-buffer-create name)))
    (with-current-buffer buffer
      (setq default-directory directory)
      (funcall mode))
    buffer))

(defun anju-test-kill-buffers (buffers)
  (mapc
   (lambda (buffer)
     (when (buffer-live-p buffer)
       (kill-buffer buffer)))
   buffers))
"##;

fn anju_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANJU_MELPA_PIN, source_file)
        .expect("prepare pinned anju source and dependencies below ./tmp")
        .with_prelude(ANJU_TEST_PRELUDE)
        .with_timeout(ANJU_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed anju parity test").into()
}

/// Multi-probe batch for `assert_anju_autoload_parity` cases (2a).
pub(crate) fn assert_anju_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        anju_oracle("anju-autoloads.el"),
        &name,
        "anju_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_anju_parity` cases (2a).
pub(crate) fn assert_anju_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(anju_oracle("anju.el"), &name, "anju_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn anju_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_anju_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_anju_autoload_batch(&cases);
}

#[test]
fn anju_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        context_menu::context_menu_public_surface_batch_cases(),
        initialization::initialization_public_surface_batch_cases(),
        mode_line::mode_line_public_surface_batch_cases(),
        registry::registry_anju_batch_cases(),
        style_text::style_text_public_surface_batch_cases(),
        utils::utils_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_anju_batch(&cases);
}

// END generated package batch tests
