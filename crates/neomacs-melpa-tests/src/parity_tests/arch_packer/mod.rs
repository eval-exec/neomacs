use std::time::Duration;

use crate::{ARCH_PACKER_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

// The in-fixture waits reset their 150 s deadline on observable progress, so a
// merely starved run can spend well over the old 180 s bound; the outer bound
// only has to cover several progress stalls plus cleanup.  nextest's MELPA
// slow timeout (960 s, .config/nextest.toml) remains the hard ceiling.
const ARCH_PACKER_TEST_TIMEOUT: Duration = Duration::from_secs(600);
const ARCH_PACKER_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'thingatpt)

(defvar neomacs-arch-packer-test-start-process nil)
(defvar neomacs-arch-packer-test-shell nil)

(defun neomacs-arch-packer-test-root (name)
  (file-name-as-directory
   (expand-file-name
    name
    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))

(defun neomacs-arch-packer-test-file-string (file)
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun neomacs-arch-packer-test-trace-through (file command)
  "Return FILE through COMMAND's complete trace line.
The package asynchronously refreshes its menu after an action; exclude that
follow-up query from the action snapshot regardless of sentinel timing."
  (let* ((text (neomacs-arch-packer-test-file-string file))
         (end (string-match
               (concat (regexp-quote command) "\n")
               text)))
    (unless end
      (error "arch-packer trace lacks command: %s" command))
    (substring text 0 (match-end 0))))

(defun neomacs-arch-packer-test-write-executable (file content)
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert content))
  (set-file-modes file #o755))

(defun neomacs-arch-packer-test-prepare (name)
  (let* ((root (neomacs-arch-packer-test-root name))
         (bin (expand-file-name "bin/" root))
         (trace (expand-file-name "pacman.trace" root))
         (state (expand-file-name "repository-state" root))
         (child-init
          (expand-file-name "async-child-init.el" root))
         (pacman (expand-file-name "pacman" bin))
         (pacaur (expand-file-name "pacaur" bin))
         (sudo (expand-file-name "sudo" bin))
         (shell (expand-file-name "arch-packer-shell" bin)))
    (neomacs-arch-packer-test-cleanup root)
    ;; The package hard-codes the FHS path /bin/bash and drives it as an
    ;; interactive shell: commands go in one line at a time and the package's
    ;; process filter matches whole markers ("Pacman finished") inside a filter
    ;; chunk.  A real interactive bash prints prompts and PS2 continuations and
    ;; runs the multi-line exit-status probe; under load a chunk boundary can
    ;; split a marker, and timeouts were observed on BOTH editors (GNU and
    ;; Neomacs) in the full-suite and 12-way runs.  A fixture-owned shell keeps
    ;; the same command grammar and the same fixture pacman/pacaur binaries,
    ;; but emits each marker as one write, so the workflow does not depend on
    ;; prompt scheduling.  It prints prompts because the package's
    ;; open-shell-process relies on accept-process-output returning non-nil
    ;; before it sends the first command.  Identical for both editors.
    (neomacs-arch-packer-test-write-executable
     shell
     (concat
      "#!/bin/sh\n"
      "last=0\n"
      "printf 'bash-5.3$ '\n"
      "while IFS= read -r line; do\n"
      "  case \"$line\" in\n"
      "    'if ['*)\n"
      "      while IFS= read -r continuation; do\n"
      "        case \"$continuation\" in *';fi'*) break ;; esac\n"
      "      done\n"
      "      sleep 0.2\n"
      "      if [ \"$last\" -ne 0 ]; then\n"
      "        printf 'Pacman error\\n'\n"
      "      else\n"
      "        printf 'Pacman finished\\n'\n"
      "      fi\n"
      "      printf 'bash-5.3$ '\n"
      "      ;;\n"
      "    '')\n"
      "      ;;\n"
      "    *)\n"
      "      cmd=${line%'; history -d $((HISTCMD-1))'}\n"
      "      eval \"$cmd\"\n"
      "      last=$?\n"
      "      printf 'bash-5.3$ '\n"
      "      ;;\n"
      "  esac\n"
      "done\n"))
    (setq neomacs-arch-packer-test-shell shell)
    (setq neomacs-arch-packer-test-start-process
          (symbol-function 'start-process))
    (fset
     'start-process
     (lambda (process-name buffer program &rest program-args)
       (apply
        neomacs-arch-packer-test-start-process
        process-name
        buffer
        (if
            (and
             (string= process-name "arch-packer-process")
             (string= program "/bin/bash"))
            neomacs-arch-packer-test-shell
          program)
        program-args)))
    (make-directory bin t)
    (with-temp-file child-init
      (prin1
       `(let
            ((default-directory
              ,(file-name-directory
                (directory-file-name
                 (file-name-directory
                  (getenv "NEOMACS_PACKAGE_SOURCE"))))))
          (normal-top-level-add-subdirs-to-load-path))
       (current-buffer))
      (insert "\n"))
    (setq async-child-init child-init)
    (neomacs-arch-packer-test-write-executable
     pacman
     (concat
      "#!/bin/sh\n"
      "set -eu\n"
      "trace=${ARCH_PACKER_TEST_TRACE:?}\n"
      "state=${ARCH_PACKER_TEST_STATE:?}\n"
      "manager=${0##*/}\n"
      "printf '%s' \"$manager\" >> \"$trace\"\n"
      "for argument do printf ' <%s>' \"$argument\" >> \"$trace\"; done\n"
      "printf '\\n' >> \"$trace\"\n"
      "case \"$*\" in\n"
      "  '-Sy')\n"
      "    printf '%s\\n' ':: Synchronizing package databases...'\n"
      "    exit 0 ;;\n"
      "  '-Qu')\n"
      "    exit 0 ;;\n"
      "  '-Qe --info')\n"
      "    if test -f \"$state\"; then kernel='6.9.2-1'; desc='Kernel after repository refresh';\n"
      "    else kernel='6.9.1-1'; desc='The Linux kernel'; fi\n"
      "    cat <<EOF\n"
      "Name : linux\n"
      "Version : $kernel\n"
      "Description : $desc\n"
      "URL : https://archlinux.org/packages/core/x86_64/linux/\n"
      "Validated By : Signature\n"
      "\n"
      "Name : ripgrep\n"
      "Version : 14.1.0-1\n"
      "Description : Search recursively for a regex pattern\n"
      "URL : https://archlinux.org/packages/extra/x86_64/ripgrep/\n"
      "Validated By : Signature\n"
      "\n"
      "Name : old-theme\n"
      "Version : 1.0-2\n"
      "Description : Retired desktop theme\n"
      "URL : https://packages.example.test/old-theme\n"
      "Validated By : Signature\n"
      "\n"
      "Name : local-helper\n"
      "Version : 2.4-1\n"
      "Description : Locally installed AUR helper\n"
      "URL : https://aur.archlinux.org/packages/local-helper\n"
      "Validated By : None\n"
      "\n"
      "Name : neovim\n"
      "Version : 0.9.5-1\n"
      "Description : Installed modal editor awaiting a manual update\n"
      "URL : https://archlinux.org/packages/extra/x86_64/neovim/\n"
      "Validated By : Signature\n"
      "EOF\n"
      "    exit 0 ;;\n"
      "  'linux -Qe --info')\n"
      "    payload=$(cat <<'EOF'\n"
      "Name            : linux\n"
      "Version         : 6.9.1-1\n"
      "Depends On      : coreutils  kmod  mkinitcpio\n"
      "Description     : The Linux kernel\n"
      "URL             : https://archlinux.org/packages/core/x86_64/linux/\n"
      "Validated By    : Signature\n"
      "EOF\n"
      "    )\n"
      "    printf '%s' \"$payload\"\n"
      "    exit 0 ;;\n"
      "  '-Ss editor')\n"
      "    cat <<'EOF'\n"
      "extra/neovim 0.10.0-2\n"
      "    Fork of Vim focused on extensibility and usability\n"
      "extra/helix 24.3-1\n"
      "    A post-modern modal text editor\n"
      "aur/emacs-git 30.0.50.r12345-1\n"
      "    Development branch of the extensible editor\n"
      "EOF\n"
      "    exit 0 ;;\n"
      "  '-S --noconfirm neovim'|'-S --noconfirm linux')\n"
      "    printf '%s\\n' 'resolving dependencies...' 'installing requested package'\n"
      "    exit 0 ;;\n"
      "  '-Rsn --noconfirm old-theme')\n"
      "    printf '%s\\n' 'checking dependencies...' 'removing old-theme'\n"
      "    exit 0 ;;\n"
      "  *)\n"
      "    printf 'unexpected pacman arguments: <%s>\\n' \"$*\" >&2\n"
      "    exit 64 ;;\n"
      "esac\n"))
    (copy-file pacman pacaur t)
    (set-file-modes pacaur #o755)
    (neomacs-arch-packer-test-write-executable
     sudo
     (concat
      "#!/bin/sh\n"
      "set -eu\n"
      "test \"$#\" -gt 0 || { printf '%s\\n' 'sudo: missing command' >&2; exit 64; }\n"
      "test \"$1\" = pacman || { printf 'sudo: unexpected command <%s>\\n' \"$1\" >&2; exit 64; }\n"
      "shift\n"
      "exec pacman \"$@\"\n"))
    (setq exec-path (cons bin exec-path)
          process-environment (copy-sequence process-environment))
    (setenv
     "PATH"
     (concat
      bin
      path-separator
      (or (getenv "PATH") "")))
    (setenv "ARCH_PACKER_TEST_TRACE" trace)
    (setenv "ARCH_PACKER_TEST_STATE" state)
    (list :root root :trace trace :state state)))

(defun neomacs-arch-packer-test-progress-token ()
  "Observable progress of the package's async workflow.
The trace file, the process buffer's mode/size and the output buffer's size
all advance as bash, the fake pacman and the menu rebuild do their work."
  (list
   (ignore-errors
     (let ((attributes (file-attributes (getenv "ARCH_PACKER_TEST_TRACE"))))
       (and attributes
            (list (nth 7 attributes) (nth 5 attributes)))))
   (and (get-buffer arch-packer-process-buffer)
        (with-current-buffer arch-packer-process-buffer
          (list major-mode (buffer-size))))
   (and (get-buffer arch-packer-process-output-buffer)
        (with-current-buffer arch-packer-process-output-buffer
          (buffer-size)))))

(defun neomacs-arch-packer-test-wait-for (predicate)
  "Wait for PREDICATE with a progress-reset deadline.

The pacman stand-in, the shell filter and the menu rebuild are all real
process and display work; on a loaded machine ten seconds of polling was
not enough, and even a minute was not enough when the host is running
other builds (both editors were measured to exceed 60 s here, on
different cases).  A fixed wall-clock deadline is the wrong guard for a
merely starved run, so the deadline resets whenever the workflow makes
observable progress (trace, process mode/size, output buffer); a real
hang still reaches the deadline because nothing advances.  The batch's
own timeout is the outer bound."
  (let ((deadline (+ (float-time) 150))
        (last-progress (neomacs-arch-packer-test-progress-token)))
    (while (and (< (float-time) deadline)
                (not (funcall predicate)))
      (accept-process-output nil 0.05)
      (let ((progress (neomacs-arch-packer-test-progress-token)))
        (unless (equal progress last-progress)
          (setq last-progress progress
                deadline (+ (float-time) 150)))))
    (unless (funcall predicate)
      ;; A bare timeout says nothing about WHICH step stalled.  Include the
      ;; observable state the predicate reads, so a failure names the step and
      ;; the editor's view of it.
      (error "arch-packer workflow timed out %s"
             (let ((print-circle nil) (print-length 40) (print-level 6))
               (prin1-to-string
                (list
                 :trace
                 (ignore-errors
                   (neomacs-arch-packer-test-file-string
                    (getenv "ARCH_PACKER_TEST_TRACE")))
                 :output
                 (ignore-errors
                   (with-current-buffer arch-packer-process-output-buffer
                     (buffer-string)))
                 :processes
                 (mapcar (lambda (process)
                           (list (process-name process)
                                 (process-status process)))
                         (process-list))
                 :buffers
                 (mapcar (lambda (buffer)
                           (cons (buffer-name buffer) major-mode))
                         (buffer-list)))))))))

(defun neomacs-arch-packer-test-cleanup (root)
  (let* ((buffer
          (get-buffer "*Pacman-Packages*"))
         (process
          (or
           (get-process "arch-packer-process")
           (and
            buffer
            (get-buffer-process buffer)))))
    (when process
      (ignore-errors
        (delete-process process))))
  (when neomacs-arch-packer-test-start-process
    (fset
     'start-process
     neomacs-arch-packer-test-start-process)
    (setq neomacs-arch-packer-test-start-process nil))
  (remove-hook 'post-command-hook 'arch-packer-status-reporter)
  (dolist
      (name
       '("*Pacman-Packages*"
         "*arch-packer-output*"
         "*pacman-package-info*"))
    (let ((buffer (get-buffer name)))
      (when buffer
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer))))
  (when
      (and root (file-exists-p root))
    (delete-directory root t)))
"####;

fn arch_packer_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ARCH_PACKER_MELPA_PIN, "arch-packer.el")
        .expect("prepare pinned arch-packer source below ./tmp")
        .with_prelude(ARCH_PACKER_TEST_PRELUDE)
        .with_timeout(ARCH_PACKER_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed arch-packer parity test")
        .into()
}

/// Multi-probe batch for `assert_arch_packer_parity` cases (2a).
pub(crate) fn assert_arch_packer_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(arch_packer_oracle(), &name, "arch_packer_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn arch_packer_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_arch_packer_batch(&cases);
}

// END generated package batch tests
