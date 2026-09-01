use std::time::Duration;

use crate::{ANSIBLE_DOC_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANSIBLE_DOC_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ANSIBLE_DOC_TEST_PRELUDE: &str = r####"
(unless
    (fboundp 'yaml-mode)
  (define-derived-mode yaml-mode fundamental-mode "YAML"))

(defun neomacs-ansible-doc-test-write-executable (file content)
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert content))
  (set-file-modes file #o755))

(defun neomacs-ansible-doc-test-install-tool (root)
  (let* ((bin (expand-file-name "tools/bin" root))
         (trace (expand-file-name "tools/invocations.log" root))
         (program (expand-file-name "ansible-doc" bin)))
    (neomacs-ansible-doc-test-write-executable
     program
     (concat
      "#!/bin/sh\n"
      "printf 'ansible-doc cwd=%s' \"$PWD\" >> \"$ANSIBLE_DOC_TEST_TRACE\"\n"
      "for argument do printf ' <%s>' \"$argument\" >> \"$ANSIBLE_DOC_TEST_TRACE\"; done\n"
      "printf '\\n' >> \"$ANSIBLE_DOC_TEST_TRACE\"\n"
      "case \"$1\" in\n"
      "  --list)\n"
      "    printf '%s\\n' \\\n"
      "      'copy                         Copy files to remote hosts' \\\n"
      "      'file                         Manage files and file properties' \\\n"
      "      'user                         Manage user accounts' \\\n"
      "      'ansible.builtin.template     Render a template on a remote host'\n"
      "    ;;\n"
      "  copy)\n"
      "    printf '%s\\n' \\\n"
      "      '> COPY' \\\n"
      "      '' \\\n"
      "      'Copy application configuration to managed hosts.' \\\n"
      "      '' \\\n"
      "      'Options (= is mandatory):' \\\n"
      "      '= src' \\\n"
      "      '    Local path of the configuration file.' \\\n"
      "      '- dest' \\\n"
      "      '    Absolute path on the managed host.' \\\n"
      "      '    [Default: /etc/myapp/app.conf]' \\\n"
      "      '- backup' \\\n"
      "      '    Create a backup before replacing the file.' \\\n"
      "      '    (Choices: yes, no)' \\\n"
      "      '    See [file] for ownership and mode management.' \\\n"
      "      'Notes:  The source is read from the control machine.' \\\n"
      "      'Requirements:  none' \\\n"
      "      '' \\\n"
      "      '# - name: Deploy the application configuration' \\\n"
      "      '  copy:' \\\n"
      "      '    src: files/app.conf' \\\n"
      "      '    dest: /etc/myapp/app.conf' \\\n"
      "      '    backup: yes'\n"
      "    ;;\n"
      "  file)\n"
      "    printf '%s\\n' \\\n"
      "      '> FILE' \\\n"
      "      '' \\\n"
      "      'Manage ownership, permissions, and state of remote paths.' \\\n"
      "      '' \\\n"
      "      'Options (= is mandatory):' \\\n"
      "      '= path' \\\n"
      "      '    Path to manage.' \\\n"
      "      '- state' \\\n"
      "      '    Desired path state.' \\\n"
      "      '    (Choices: file, directory, absent)' \\\n"
      "      '- mode' \\\n"
      "      '    Filesystem permissions.' \\\n"
      "      '' \\\n"
      "      '# - name: Create the configuration directory' \\\n"
      "      '  file:' \\\n"
      "      '    path: /etc/myapp' \\\n"
      "      '    state: directory' \\\n"
      "      '    mode: 0750'\n"
      "    ;;\n"
      "  user)\n"
      "    if test -f \"$ANSIBLE_DOC_TEST_ROOT/user-doc-updated\"; then\n"
      "      summary='Manage application service accounts and login policy.'\n"
      "      shell_default='/usr/sbin/nologin'\n"
      "    else\n"
      "      summary='Manage application service accounts.'\n"
      "      shell_default='/bin/sh'\n"
      "    fi\n"
      "    printf '%s\\n' \\\n"
      "      '> USER' \\\n"
      "      '' \\\n"
      "      \"$summary\" \\\n"
      "      '' \\\n"
      "      'Options (= is mandatory):' \\\n"
      "      '= name' \\\n"
      "      '    Account name.' \\\n"
      "      '- shell' \\\n"
      "      '    Login shell.' \\\n"
      "      \"    [Default: $shell_default]\" \\\n"
      "      '- system' \\\n"
      "      '    Create a system account.' \\\n"
      "      '    (Choices: yes, no)' \\\n"
      "      '' \\\n"
      "      '# - name: Create the application account' \\\n"
      "      '  user:' \\\n"
      "      '    name: myapp' \\\n"
      "      '    system: yes'\n"
      "    ;;\n"
      "  *)\n"
      "    printf 'unknown module: %s\\n' \"$1\" >&2\n"
      "    exit 2\n"
      "    ;;\n"
      "esac\n"))
    (list :bin bin :trace trace)))

(defun neomacs-ansible-doc-test-use-tool (root tool)
  (let ((bin (plist-get tool :bin))
        (trace (plist-get tool :trace)))
    (setq exec-path (cons bin exec-path)
          process-environment (copy-sequence process-environment))
    (setenv
     "PATH"
     (concat
      bin
      path-separator
      (or (getenv "PATH") "")))
    (setenv "ANSIBLE_DOC_TEST_TRACE" trace)
    (setenv "ANSIBLE_DOC_TEST_ROOT" root)))

(defun neomacs-ansible-doc-test-file-string (file)
  (with-temp-buffer
    (insert-file-contents file)
    (buffer-string)))

(defun neomacs-ansible-doc-test-cleanup (root)
  (dolist (buffer (buffer-list))
    (when
        (or
         (string-prefix-p "*ansible-doc " (buffer-name buffer))
         (let ((file (buffer-file-name buffer)))
           (and file (string-prefix-p root file))))
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  (when
      (file-exists-p root)
    (delete-directory root t)))
"####;

fn ansible_doc_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANSIBLE_DOC_MELPA_PIN, "ansible-doc.el")
        .expect("prepare pinned ansible-doc source below ./tmp")
        .with_prelude(ANSIBLE_DOC_TEST_PRELUDE)
        .with_timeout(ANSIBLE_DOC_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ansible-doc parity test")
        .into()
}

/// Multi-probe batch for `assert_ansible_doc_parity` cases (2a).
pub(crate) fn assert_ansible_doc_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ansible_doc_oracle(), &name, "ansible_doc_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ansible_doc_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ansible_doc_batch(&cases);
}

// END generated package batch tests
