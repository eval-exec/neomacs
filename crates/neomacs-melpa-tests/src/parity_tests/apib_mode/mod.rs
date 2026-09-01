use std::time::Duration;

use crate::{APIB_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const APIB_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const APIB_MODE_TEST_PRELUDE: &str = r####"
(defun neomacs-apib-test-face-at (needle)
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (get-text-property
     (- (point) (length needle))
     'face)))

(defun neomacs-apib-test-file-string (file)
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun neomacs-apib-test-write-drafter (file)
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert
     "#!/bin/sh\n"
     "set -eu\n"
     "trace=\"${0%/*}/../drafter.trace\"\n"
     "printf 'argv=' >> \"$trace\"\n"
     "for argument in \"$@\"; do\n"
     "  printf '<%s>' \"$argument\" >> \"$trace\"\n"
     "done\n"
     "printf '\\n' >> \"$trace\"\n"
     "if test \"$#\" -eq 1 && test \"$1\" = '-lu'; then\n"
     "  input=$(cat)\n"
     "  printf 'stdin=<%s>\\n' \"$input\" >> \"$trace\"\n"
     "  case \"$input\" in\n"
     "    *\"id forty-two\"*) exit 1 ;;\n"
     "    *) exit 0 ;;\n"
     "  esac\n"
     "fi\n"
     "last=''\n"
     "for argument in \"$@\"; do last=$argument; done\n"
     "if test \"${1-}\" = '-lu'; then\n"
     "  if grep -q 'id forty-two' \"$last\"; then\n"
     "    printf '%s\\n' 'error: API description parse error, line 12, column 3 - line 12, column 16'\n"
     "    exit 1\n"
     "  fi\n"
     "  printf '%s\\n' 'OK: API Blueprint is valid'\n"
     "  exit 0\n"
     "fi\n"
     "if test \"${1-}\" = '-f'; then\n"
     "  if grep -q 'id forty-two' \"$last\"; then\n"
     "    printf '%s\\n' '{\"element\":\"annotation\",\"content\":\"API Blueprint is invalid\"}'\n"
     "    exit 0\n"
     "  fi\n"
     "  cat <<'JSON'\n"
     "{\"element\":\"parseResult\",\"content\":[{\"element\":\"category\",\"content\":[{\"element\":\"asset\",\"attributes\":{\"contentType\":{\"element\":\"string\",\"content\":\"application/json\"}},\"content\":\"{\\\"id\\\":42,\\\"name\\\":\\\"Hammer\\\",\\\"available\\\":true}\"},{\"element\":\"asset\",\"attributes\":{\"contentType\":{\"element\":\"string\",\"content\":\"application/schema+json\"}},\"content\":\"{\\\"$schema\\\":\\\"http://json-schema.org/draft-04/schema#\\\",\\\"type\\\":\\\"object\\\",\\\"required\\\":[\\\"id\\\",\\\"name\\\"],\\\"properties\\\":{\\\"id\\\":{\\\"type\\\":\\\"number\\\"},\\\"name\\\":{\\\"type\\\":\\\"string\\\"},\\\"available\\\":{\\\"type\\\":\\\"boolean\\\"}}}\"}]},{\"element\":\"asset\",\"attributes\":{\"contentType\":{\"element\":\"string\",\"content\":\"application/json\"}},\"content\":\"{\\\"id\\\":43,\\\"name\\\":\\\"Saw\\\",\\\"available\\\":false}\"}]}\n"
     "JSON\n"
     "  exit 0\n"
     "fi\n"
     "exit 64\n"))
  (set-file-modes file #o755))

(defun neomacs-apib-test-cleanup (root)
  (dolist
      (buffer (buffer-list))
    (let ((file (buffer-file-name buffer))
          (name (buffer-name buffer)))
      (when
          (or
           (and file (string-prefix-p root file))
           (string-prefix-p "*apib-" name))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer))))
  (when
      (file-exists-p root)
    (delete-directory root t)))
"####;

fn apib_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(APIB_MODE_MELPA_PIN, "apib-mode.el")
        .expect("prepare pinned apib-mode source below ./tmp")
        .with_prelude(APIB_MODE_TEST_PRELUDE)
        .with_timeout(APIB_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed apib-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_apib_mode_parity` cases (2a).
pub(crate) fn assert_apib_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(apib_mode_oracle(), &name, "apib_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn apib_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_apib_mode_batch(&cases);
}

// END generated package batch tests
