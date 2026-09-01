use std::time::Duration;

use crate::{AUCTEX_LUA_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AUCTEX_LUA_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const AUCTEX_LUA_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'latex)
(require 'auctex-lua)

(defconst neomacs-auctex-lua-test--original-save-remap
  (lookup-key lua-mode-map [remap save-buffer]))

(defun neomacs-auctex-lua-test--normalize-text (text root)
  "Normalize sandbox-local paths in TEXT produced below ROOT."
  (replace-regexp-in-string (regexp-quote root) "<sandbox>/" text t t))

(defun neomacs-auctex-lua-test--messages (start root)
  "Return every ordered message line after START, normalized below ROOT."
  (with-current-buffer (messages-buffer)
    (let ((transcript
           (neomacs-auctex-lua-test--normalize-text
            (buffer-substring-no-properties
             (min start (point-max)) (point-max))
            root)))
      (split-string (string-trim transcript) "\n" t))))

(defun neomacs-auctex-lua-test--file-text (file)
  "Return FILE's exact literal contents."
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun neomacs-auctex-lua-test--token-properties (token)
  "Return exact display properties at TOKEN in the current buffer."
  (save-excursion
    (goto-char (point-min))
    (search-forward token)
    (let ((position (match-beginning 0)))
      (list
       :face (copy-tree (get-text-property position 'face))
       :font-lock-face
       (copy-tree (get-text-property position 'font-lock-face))
       :syntax-table
       (copy-tree (get-text-property position 'syntax-table))))))

(defun neomacs-auctex-lua-test--cleanup (root)
  "Kill fixture buffers, restore one window, and remove ROOT."
  (ignore-errors (delete-other-windows))
  (dolist (buffer (buffer-list))
    (let ((file (buffer-file-name buffer)))
      (when (or (and file root (string-prefix-p root file))
                (string-match-p " \\[Lua\\]\\*\\'" (buffer-name buffer)))
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (ignore-errors (kill-buffer buffer)))))
  (ignore-errors (delete-other-windows))
  (define-key lua-mode-map [remap save-buffer]
              neomacs-auctex-lua-test--original-save-remap)
  (when (boundp 'LaTeX-edit-Lua-code-parent-buffer)
    (makunbound 'LaTeX-edit-Lua-code-parent-buffer))
  (when (boundp 'LaTeX-edit-Lua-code-parent-buffer-point)
    (makunbound 'LaTeX-edit-Lua-code-parent-buffer-point))
  (when (and root (file-exists-p root))
    (delete-directory root t)))
"####;

fn auctex_lua_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUCTEX_LUA_MELPA_PIN, "auctex-lua.el")
        .expect("prepare pinned auctex-lua source below ./tmp")
        .with_prelude(AUCTEX_LUA_TEST_PRELUDE)
        .with_installed_autoloads()
        .with_timeout(AUCTEX_LUA_TEST_TIMEOUT)
}

#[test]
fn auctex_lua_package_batch() {
    assert_oracle_batch_cases(
        auctex_lua_oracle(),
        "auctex_lua_package_batch",
        "auctex_lua_parity",
        &workflows::practical_workflow_batch_cases(),
    );
}
