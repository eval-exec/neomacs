use std::time::Duration;

use crate::{ANT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANT_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ANT_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun neomacs-ant-write-file (root relative content)
  (let ((path (expand-file-name relative root)))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert content))
    path))

(defun neomacs-ant-read-file (path)
  (with-temp-buffer
    (insert-file-contents-literally path)
    (buffer-string)))

(defun neomacs-ant-fixture ()
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root
          (file-name-as-directory
           (expand-file-name "storefront" sandbox)))
         (build-file (expand-file-name "build.xml" root))
         (source
          (expand-file-name
           "src/main/java/example/Checkout.java"
           root))
         (ant-command
          (expand-file-name "bin/ant" sandbox))
         (command-log
          (expand-file-name "ant-commands.log" sandbox)))
    (neomacs-ant-write-file
     root
     "build.xml"
     (concat
      "<project name=\"storefront\" default=\"test\">\n"
      "  <target name=\"clean\" description=\"Remove build output\"/>\n"
      "  <target name=\"compile\" description=\"Compile Java sources\"/>\n"
      "  <target name=\"test\" description=\"Run unit tests\"/>\n"
      "  <target name=\"package\" description=\"Create release archive\"/>\n"
      "</project>\n"))
    (neomacs-ant-write-file
     root
     "src/main/java/example/Checkout.java"
     (concat
      "package example;\n\n"
      "final class Checkout {\n"
      "  boolean ready() { return true; }\n"
      "}\n"))
    (make-directory (file-name-directory ant-command) t)
    (with-temp-file ant-command
      (insert
       "#!/bin/sh\n"
       "printf '%s|%s\\n' \"$PWD\" \"$*\" >> \"$NEOMACS_ANT_COMMAND_LOG\"\n"
       "build_file=build.xml\n"
       "previous=\n"
       "for argument in \"$@\"; do\n"
       "  case \"$previous\" in\n"
       "    -f|-file|-buildfile) build_file=$argument ;;\n"
       "  esac\n"
       "  previous=$argument\n"
       "done\n"
       "printf 'Buildfile: %s/%s\\n\\n' \"$PWD\" \"$build_file\"\n"
       "for task in \"$@\"; do\n"
       "  case \"$task\" in\n"
       "    -*) ;;\n"
       "    compile)\n"
       "      printf 'compile:\\n    [javac] Compiling 12 source files\\n'\n"
       "      ;;\n"
       "    test)\n"
       "      printf 'test:\\n    [junit] Tests run: 48, Failures: 0\\n'\n"
       "      ;;\n"
       "    deploy)\n"
       "      printf 'deploy:\\n     [copy] storefront.jar -> staging\\n'\n"
       "      ;;\n"
       "  esac\n"
       "done\n"
       "printf '\\nBUILD SUCCESSFUL\\n'\n"))
    (set-file-modes ant-command #o755)
    (setenv "NEOMACS_ANT_COMMAND_LOG" command-log)
    (list
     :root root
     :build-file build-file
     :source source
     :ant-command ant-command
     :command-log command-log)))

(defun neomacs-ant-wait-for-compilation (buffer)
  (let ((deadline (+ (float-time) 15.0)))
    (while (and
            (buffer-live-p buffer)
            (get-buffer-process buffer)
            (< (float-time) deadline))
      (accept-process-output
       (get-buffer-process buffer)
       0.02))
    (and
     (buffer-live-p buffer)
     (not (get-buffer-process buffer)))))

(defun neomacs-ant-normalized-compilation (buffer)
  (with-current-buffer buffer
    (let ((text
           (buffer-substring-no-properties
            (point-min) (point-max))))
      (setq
       text
       (replace-regexp-in-string
        "Compilation started at [^\n]+"
        "Compilation started at [TIME]"
        text))
      (replace-regexp-in-string
       "Compilation finished at [^\n]+"
       "Compilation finished at [TIME]"
       text))))
"##;

fn ant_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANT_MELPA_PIN, "ant.el")
        .expect("prepare pinned ant source below ./tmp")
        .with_prelude(ANT_TEST_PRELUDE)
        .with_timeout(ANT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed ant parity test").into()
}

/// Multi-probe batch for `assert_ant_parity` cases (2a).
pub(crate) fn assert_ant_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ant_oracle(), &name, "ant_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ant_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ant_batch(&cases);
}

// END generated package batch tests
