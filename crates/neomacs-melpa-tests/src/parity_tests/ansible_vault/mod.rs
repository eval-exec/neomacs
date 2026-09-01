use std::time::Duration;

use crate::{ANSIBLE_VAULT_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANSIBLE_VAULT_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const ANSIBLE_VAULT_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun neomacs-ansible-vault-write-file (root relative content)
  (let ((path (expand-file-name relative root)))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert content))
    path))

(defun neomacs-ansible-vault-read-file (path)
  (with-temp-buffer
    (insert-file-contents-literally path)
    (buffer-string)))

(defun neomacs-ansible-vault-fixture ()
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root
          (file-name-as-directory
           (expand-file-name "infrastructure" sandbox)))
         (password-file
          (expand-file-name "credentials/team.pass" root))
         (wrong-password-file
          (expand-file-name "credentials/wrong.pass" root))
         (legacy-vault
          (expand-file-name
           "group_vars/production/vault.yml"
           root))
         (production-vault
          (expand-file-name
           "group_vars/production/deploy.vault.yml"
           root))
         (vault-command
          (expand-file-name "bin/ansible-vault" sandbox))
         (vault-log
          (expand-file-name "ansible-vault.log" sandbox)))
    (neomacs-ansible-vault-write-file
     root
     "credentials/team.pass"
     "team-secret")
    (neomacs-ansible-vault-write-file
     root
     "credentials/wrong.pass"
     "wrong-secret")
    (neomacs-ansible-vault-write-file
     root
     "ansible.cfg"
     (format
      "[defaults]\nvault_password_file = %s\n"
      password-file))
    (neomacs-ansible-vault-write-file
     root
     "group_vars/production/vault.yml"
     "$ANSIBLE_VAULT;1.1;AES256\nENC:api_token: initial-secret\nENC:release_channel: stable")
    (neomacs-ansible-vault-write-file
     root
     "group_vars/production/deploy.vault.yml"
     "$ANSIBLE_VAULT;1.2;AES256;prod\nENC:deployment_ring: blue\nENC:approval_required: true")
    (make-directory (file-name-directory vault-command) t)
    (with-temp-file vault-command
      (insert
       "#!/bin/sh\n"
       "mode=$1\n"
       "shift\n"
       "password_file=\n"
       "vault_id=\n"
       "encrypt_vault_id=\n"
       "for argument in \"$@\"; do\n"
       "  case \"$argument\" in\n"
       "    --vault-password-file=*) password_file=${argument#*=} ;;\n"
       "    --vault-id=*)\n"
       "      vault_id=${argument#*=}\n"
       "      password_file=${vault_id#*@}\n"
       "      ;;\n"
       "    --encrypt-vault-id=*) encrypt_vault_id=${argument#*=} ;;\n"
       "  esac\n"
       "done\n"
       "IFS= read -r password < \"$password_file\"\n"
       "printf '%s|%s|%s|%s\\n' \"$mode\" \"$password\" \"$vault_id\" \"$encrypt_vault_id\" >> \"$NEOMACS_ANSIBLE_VAULT_LOG\"\n"
       "if [ \"$password\" != team-secret ]; then\n"
       "  printf 'ERROR! Decryption failed: invalid vault password\\n' >&2\n"
       "  exit 23\n"
       "fi\n"
       "case \"$mode\" in\n"
       "  decrypt)\n"
       "    IFS= read -r _header\n"
       "    while IFS= read -r line || [ -n \"$line\" ]; do\n"
       "      printf '%s\\n' \"${line#ENC:}\"\n"
       "    done\n"
       "    ;;\n"
       "  encrypt)\n"
       "    if [ -n \"$encrypt_vault_id\" ]; then\n"
       "      printf '$ANSIBLE_VAULT;1.2;AES256;%s\\n' \"$encrypt_vault_id\"\n"
       "    else\n"
       "      printf '$ANSIBLE_VAULT;1.1;AES256\\n'\n"
       "    fi\n"
       "    while IFS= read -r line || [ -n \"$line\" ]; do\n"
       "      printf 'ENC:%s\\n' \"$line\"\n"
       "    done\n"
       "    ;;\n"
       "  encrypt_string)\n"
       "    printf '!vault |\\n'\n"
       "    printf '          $ANSIBLE_VAULT;1.1;AES256\\n'\n"
       "    while IFS= read -r line || [ -n \"$line\" ]; do\n"
       "      printf '          ENC:%s\\n' \"$line\"\n"
       "    done\n"
       "    ;;\n"
       "  *) exit 9 ;;\n"
       "esac\n"))
    (set-file-modes vault-command #o755)
    (setenv
     "PATH"
     (concat
      (file-name-directory vault-command)
      path-separator
      "/usr/bin:/bin"))
    (setenv "NEOMACS_ANSIBLE_VAULT_LOG" vault-log)
    (list
     :root root
     :password-file password-file
     :wrong-password-file wrong-password-file
     :legacy-vault legacy-vault
     :vault-command vault-command
     :vault-log vault-log
     :production-vault production-vault)))
"##;

fn ansible_vault_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANSIBLE_VAULT_MELPA_PIN, "ansible-vault.el")
        .expect("prepare pinned ansible-vault source below ./tmp")
        .with_prelude(ANSIBLE_VAULT_TEST_PRELUDE)
        .with_timeout(ANSIBLE_VAULT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ansible-vault parity test")
        .into()
}

/// Multi-probe batch for `assert_ansible_vault_parity` cases (2a).
pub(crate) fn assert_ansible_vault_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ansible_vault_oracle(), &name, "ansible_vault_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ansible_vault_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ansible_vault_batch(&cases);
}

// END generated package batch tests
