use std::time::Duration;

use crate::{CachedMelpaOracle, LUA_MODE_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const LUA_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const LUA_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'flymake)
(require 'hideshow)
(require 'imenu)
(require 'lua-mode)

(defun lua-test-face-spans ()
  "Return every contiguous non-default face span in the current buffer."
  (font-lock-ensure (point-min) (point-max))
  (let ((position (point-min))
        spans)
    (while (< position (point-max))
      (let* ((face (or (get-text-property position 'face)
                       (get-text-property position 'font-lock-face)))
             (face-next (next-single-property-change
                         position 'face nil (point-max)))
             (font-lock-next (next-single-property-change
                              position 'font-lock-face nil (point-max)))
             (next (min face-next font-lock-next)))
        (when face
          (push (list (line-number-at-pos position)
                      (save-excursion
                        (goto-char position)
                        (current-column))
                      (buffer-substring-no-properties position next)
                      face)
                spans))
        (setq position next)))
    (nreverse spans)))

(defun lua-test-syntax-at (needle)
  "Describe Lua syntax state within NEEDLE in the current buffer."
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (goto-char (- (point) (/ (length needle) 2)))
    (let* ((state (syntax-ppss))
           (start (nth 8 state)))
      (list needle
            :string (nth 3 state)
            :comment (nth 4 state)
            :start (and start
                        (save-excursion
                          (goto-char start)
                          (list (line-number-at-pos) (current-column))))))))

(defun lua-test-location (position)
  "Describe POSITION by line, column, and complete source line."
  (save-excursion
    (goto-char position)
    (list (line-number-at-pos)
          (current-column)
          (buffer-substring-no-properties
           (line-beginning-position) (line-end-position)))))

(defun lua-test-imenu-snapshot (index)
  "Convert an Imenu INDEX into names and stable source locations."
  (mapcar
   (lambda (entry)
     (if (and (listp (cdr entry))
              (not (markerp (cdr entry))))
         (cons (car entry) (lua-test-imenu-snapshot (cdr entry)))
       (list (car entry) :at (lua-test-location (cdr entry)))))
   index))
"##;

fn lua_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LUA_MODE_MELPA_PIN, "lua-mode.el")
        .expect("prepare pinned lua-mode source below ./tmp")
        .with_prelude(LUA_MODE_TEST_PRELUDE)
        .with_timeout(LUA_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed lua-mode parity test")
        .into()
}

pub(crate) fn assert_lua_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(lua_mode_oracle(), &name, "lua_mode_parity", cases);
}

#[test]
fn lua_mode_package_batch() {
    assert_lua_mode_batch(&workflows::public_workflow_cases());
}
