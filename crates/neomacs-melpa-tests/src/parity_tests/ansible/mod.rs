use std::time::Duration;

use crate::{ANSIBLE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANSIBLE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const ANSIBLE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun neomacs-ansible-write-file (root relative content)
  (let ((path
         (expand-file-name relative root)))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert content))
    path))

(defun neomacs-ansible-face-at (needle)
  (save-excursion
    (goto-char (point-min))
    (search-forward needle)
    (get-text-property
     (- (point) (length needle))
     'face)))

(defun neomacs-ansible-read-file (path)
  (with-temp-buffer
    (insert-file-contents-literally path)
    (buffer-string)))

(defun neomacs-ansible-fixture ()
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root
          (file-name-as-directory
           (expand-file-name "ansible-project" sandbox)))
         (site (expand-file-name "site.yml" root))
         (deploy
          (expand-file-name
           "playbooks/production/deploy.yml"
           root))
         (rollback
          (expand-file-name
           "playbooks/production/rollback.yml"
           root))
         (vault
          (expand-file-name
           "group_vars/production/vault.yml"
           root))
         (password-file
          (expand-file-name "credentials/vault-pass" root))
         (bin (expand-file-name "bin" sandbox))
         (vault-command
          (expand-file-name "ansible-vault" bin))
         (vault-log
          (expand-file-name "ansible-vault.log" sandbox)))
    (make-directory (expand-file-name "roles/api/tasks" root) t)
    (neomacs-ansible-write-file
     root
     "site.yml"
     "---\n- name: Deploy storefront\n  hosts: production\n  roles:\n    - api\n")
    (neomacs-ansible-write-file
     root
     "playbooks/production/deploy.yml"
     "---\n- hosts: production\n  tasks:\n    - name: Publish release\n      copy:\n        src: \"{{ artifact_path }}\"\n        dest: /srv/storefront/app.tar\n      when: release_ready\n")
    (neomacs-ansible-write-file
     root
     "playbooks/production/rollback.yml"
     "---\n- hosts: production\n  tasks:\n    - name: Restore previous release\n      command: /srv/storefront/rollback\n")
    (neomacs-ansible-write-file
     root
     "group_vars/production/vault.yml"
     "$ANSIBLE_VAULT;1.1;AES256\nENC:api_token: initial-secret\nENC:release_channel: stable")
    (neomacs-ansible-write-file
     root
     "credentials/vault-pass"
     "team-secret")
    (make-directory bin t)
    (with-temp-file vault-command
      (insert
       "#!/bin/sh\n"
       "mode=$1\n"
       "shift\n"
       "password_file=\n"
       "while [ \"$#\" -gt 1 ]; do\n"
       "  case \"$1\" in\n"
       "    --vault-password-file=*) password_file=${1#*=} ;;\n"
       "  esac\n"
       "  shift\n"
       "done\n"
       "input=$1\n"
       "password=$(sed -n '1p' \"$password_file\")\n"
       "printf '%s|%s\\n' \"$mode\" \"$password\" >> \"$NEOMACS_ANSIBLE_VAULT_LOG\"\n"
       "case \"$password\" in\n"
       "  release-secret|team-secret) ;;\n"
       "  *) printf 'invalid vault password\\n' >&2; exit 23 ;;\n"
       "esac\n"
       "case \"$mode\" in\n"
       "  encrypt)\n"
       "    printf '$ANSIBLE_VAULT;1.1;AES256\\n'\n"
       "    sed 's/^/ENC:/' \"$input\"\n"
       "    ;;\n"
       "  decrypt)\n"
       "    sed '1d; s/^ENC://' \"$input\"\n"
       "    ;;\n"
       "  *) exit 9 ;;\n"
       "esac\n"))
    (set-file-modes vault-command #o755)
    (setenv "NEOMACS_ANSIBLE_VAULT_LOG" vault-log)
    (setq exec-path (cons bin exec-path))
    (list root site deploy rollback vault password-file vault-log)))
"##;

fn ansible_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANSIBLE_MELPA_PIN, source_file)
        .expect("prepare pinned ansible source below ./tmp")
        .with_prelude(ANSIBLE_TEST_PRELUDE)
        .with_timeout(ANSIBLE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ansible parity test")
        .into()
}

/// Multi-probe batch for `assert_ansible_parity` cases (2a).
pub(crate) fn assert_ansible_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ansible_oracle("ansible.el"), &name, "ansible_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ansible_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ansible_batch(&cases);
}

// END generated package batch tests
