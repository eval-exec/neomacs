//! Practical parity for ghostel terminal buffers without the native module.
//!
//! These cases load the Elisp layer with native entry points stubbed so
//! nothing downloads a `.so` or spawns a PTY, then exercise buffer
//! cycling, naming, and the public send/key guards.

use std::time::Duration;

use expect_test::expect;

use crate::{COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, GHOSTEL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)

;; Honour ghostel's documented pure-Elisp test path: `ghostel--load-module'
;; skips download/compile/module-load when `ghostel--new' is already bound.
(defun ghostel--new (&rest _) 'ghostel-test-term)
(defun ghostel--module-version () "0.51.0")
(defun ghostel--redraw (&rest _) nil)
(defun ghostel--set-size (&rest _) nil)
(defun ghostel--write-vt (&rest _) nil)
(defun ghostel--write-pty (&rest _) nil)
(defun ghostel--encode-key (&rest _) "")
(defun ghostel--encode-paste (&rest _) nil)
(setq ghostel-module-auto-install nil)
(require 'ghostel)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst gh493-test-tree
  "f619bcc7834eb0866ab8148297ad7d7619ea7e81")
(defconst gh493-test-manifest
  '(("ghostel-bookmark.el" . "997b7cfd83772ff82c418c57c57bd7bb46575d083cbda693691bb40cb1375421")
    ("ghostel-comint.el" . "11e5db1ae4b95513e75a36a2a9d10af5d4a94fdcc9859ecfb13102d23a5ead38")
    ("ghostel-compile.el" . "646bcedaa66bb4fd558498f6b43c5ab2fbb6db21138a9ba0f804eb70777c0da0")
    ("ghostel-debug.el" . "9e37616f8c6090d5a3a5e579ad35defb38836e236494d9db4f7c90ab998e39cc")
    ("ghostel-desktop.el" . "00150b9a2431aea466972d2681ae0360e5dd6452abe9ba1a9a46ebe1a22e0b25")
    ("ghostel-eshell.el" . "c31c34de84ad4beb304a23ebd477f8a986cce3e454fda415004e21e9b771f7b8")
    ("ghostel-faces.el" . "41d0604659be502b0d3fe862ca422d8312cecad1347890e5d03653e4987440ce")
    ("ghostel-ime.el" . "5ab293c3ecee2b45b904d8bab5bc2a2a3dcd3fafbc09bb8a30e4a694c11c3150")
    ("ghostel-kitty.el" . "a80498d38da5423e9c9263842c1faf5900e126f9f320139b7a57b6ba3cbb5dc3")
    ("ghostel-line-mode.el" . "917ce62278f5f753e3b9a43412faabd902fb0bac73753fe40dd44dc64c823de2")
    ("ghostel-links.el" . "a0be28ad4873ee018c87773ee7e98b48b372f11a77f2335a92555a2e52bd3c78")
    ("ghostel-module-install.el" . "7f761a1848fc9f89b58781e2703c75e4a7870b3eb1b44afeb4e48d3fe89bf807")
    ("ghostel-pkg.el" . "19d6444fc849d964cd5c000c0e7c0b9ec1d0bdb0ddd1473cbd72638d68a66a23")
    ("ghostel-prompt.el" . "e7fb41f1c1e1d3939a13afe937a94149e2a8a0795b3def29ff51c8861d852ea9")
    ("ghostel.el" . "1feecf05d7813f3689e9e6611095269e88b04d85c3b82b1b349497dc138c3e7c")))

(defun gh493-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun gh493-test-source-state ()
  (let* ((located (locate-library "ghostel.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (gh493-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/ghostel.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car gh493-test-manifest)))
      (error "Unexpected installed ghostel payload: %S"
             (or manifest files)))
    (dolist (entry gh493-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (gh493-test-sha file) expected))
          (error "Unexpected installed ghostel source: %S"
                 (cons entry manifest)))))
    (list :tree gh493-test-tree
          :manifest manifest
          :feature (featurep 'ghostel)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'ghostel package-alist)))))))

(defun gh493-test-plant (name)
  (let ((buf (generate-new-buffer name)))
    (with-current-buffer buf
      (setq major-mode 'ghostel-mode))
    buf))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GHOSTEL_MELPA_PIN, "ghostel.el")
        .expect("prepare pinned ghostel source below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare pinned Compat dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn package_load_gates_source_and_exposes_mode_keys_and_defaults() -> ParityBatchCase {
    ParityBatchCase::value(
        "package_load_gates_source_and_exposes_mode_keys_and_defaults",
        r####"
(list :source (gh493-test-source-state)
      :buffer-name ghostel-buffer-name
      :term ghostel-term
      :auto-install ghostel-module-auto-install
      :interrupt (lookup-key ghostel-mode-map (kbd "C-c C-c"))
      :copy (lookup-key ghostel-mode-map (kbd "C-c C-t"))
      :paste (lookup-key ghostel-mode-map (kbd "C-c C-y"))
      :next-link (lookup-key ghostel-mode-map (kbd "C-c C-n"))
      :default-face (and (facep 'ghostel-default) t)
      :palette-len (length ghostel-color-palette))
"####,
        expect![[
            r#"OK (:source (:tree "f619bcc7834eb0866ab8148297ad7d7619ea7e81" :manifest (("ghostel-bookmark.el" . "997b7cfd83772ff82c418c57c57bd7bb46575d083cbda693691bb40cb1375421") ("ghostel-comint.el" . "11e5db1ae4b95513e75a36a2a9d10af5d4a94fdcc9859ecfb13102d23a5ead38") ("ghostel-compile.el" . "646bcedaa66bb4fd558498f6b43c5ab2fbb6db21138a9ba0f804eb70777c0da0") ("ghostel-debug.el" . "9e37616f8c6090d5a3a5e579ad35defb38836e236494d9db4f7c90ab998e39cc") ("ghostel-desktop.el" . "00150b9a2431aea466972d2681ae0360e5dd6452abe9ba1a9a46ebe1a22e0b25") ("ghostel-eshell.el" . "c31c34de84ad4beb304a23ebd477f8a986cce3e454fda415004e21e9b771f7b8") ("ghostel-faces.el" . "41d0604659be502b0d3fe862ca422d8312cecad1347890e5d03653e4987440ce") ("ghostel-ime.el" . "5ab293c3ecee2b45b904d8bab5bc2a2a3dcd3fafbc09bb8a30e4a694c11c3150") ("ghostel-kitty.el" . "a80498d38da5423e9c9263842c1faf5900e126f9f320139b7a57b6ba3cbb5dc3") ("ghostel-line-mode.el" . "917ce62278f5f753e3b9a43412faabd902fb0bac73753fe40dd44dc64c823de2") ("ghostel-links.el" . "a0be28ad4873ee018c87773ee7e98b48b372f11a77f2335a92555a2e52bd3c78") ("ghostel-module-install.el" . "7f761a1848fc9f89b58781e2703c75e4a7870b3eb1b44afeb4e48d3fe89bf807") ("ghostel-pkg.el" . "19d6444fc849d964cd5c000c0e7c0b9ec1d0bdb0ddd1473cbd72638d68a66a23") ("ghostel-prompt.el" . "e7fb41f1c1e1d3939a13afe937a94149e2a8a0795b3def29ff51c8861d852ea9") ("ghostel.el" . "1feecf05d7813f3689e9e6611095269e88b04d85c3b82b1b349497dc138c3e7c")) :feature t :version "20260820.1035") :buffer-name "*ghostel*" :term "xterm-ghostty" :auto-install nil :interrupt ghostel-send-C-c :copy ghostel-copy-mode :paste ghostel-paste :next-link ghostel-next-hyperlink :default-face t :palette-len 16)"#
        ]],
    )
}

fn next_previous_cycle_planted_ghostel_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "next_previous_cycle_planted_ghostel_buffers",
        r####"
(let ((identity (current-buffer))
      (windows (current-window-configuration))
      a b c popped)
  (unwind-protect
      (progn
        (setq a (gh493-test-plant "*ghostel-a*")
              b (gh493-test-plant "*ghostel-b*")
              c (gh493-test-plant "*ghostel-c*"))
        (cl-letf (((symbol-function 'pop-to-buffer)
                   (lambda (buf &rest _)
                     (setq popped (buffer-name buf))
                     buf)))
          (with-current-buffer a (ghostel-next))
          (let ((after-a popped))
            (setq popped nil)
            (with-current-buffer c (ghostel-next))
            (let ((wrap popped))
              (setq popped nil)
              (with-current-buffer b (ghostel-previous))
              (list :source (gh493-test-source-state)
                    :sorted (mapcar #'buffer-name (ghostel--all-buffers))
                    :after-a after-a
                    :wrap wrap
                    :before-b popped
                    :empty
                    (condition-case err
                        (progn
                          (kill-buffer a)
                          (kill-buffer b)
                          (kill-buffer c)
                          (setq a nil b nil c nil)
                          (ghostel-next))
                      (error (list (car err)
                                   (error-message-string err)))))))))
    (dolist (buf (list a b c))
      (when (buffer-live-p buf) (kill-buffer buf)))
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r#"OK (:source (:tree "f619bcc7834eb0866ab8148297ad7d7619ea7e81" :manifest (("ghostel-bookmark.el" . "997b7cfd83772ff82c418c57c57bd7bb46575d083cbda693691bb40cb1375421") ("ghostel-comint.el" . "11e5db1ae4b95513e75a36a2a9d10af5d4a94fdcc9859ecfb13102d23a5ead38") ("ghostel-compile.el" . "646bcedaa66bb4fd558498f6b43c5ab2fbb6db21138a9ba0f804eb70777c0da0") ("ghostel-debug.el" . "9e37616f8c6090d5a3a5e579ad35defb38836e236494d9db4f7c90ab998e39cc") ("ghostel-desktop.el" . "00150b9a2431aea466972d2681ae0360e5dd6452abe9ba1a9a46ebe1a22e0b25") ("ghostel-eshell.el" . "c31c34de84ad4beb304a23ebd477f8a986cce3e454fda415004e21e9b771f7b8") ("ghostel-faces.el" . "41d0604659be502b0d3fe862ca422d8312cecad1347890e5d03653e4987440ce") ("ghostel-ime.el" . "5ab293c3ecee2b45b904d8bab5bc2a2a3dcd3fafbc09bb8a30e4a694c11c3150") ("ghostel-kitty.el" . "a80498d38da5423e9c9263842c1faf5900e126f9f320139b7a57b6ba3cbb5dc3") ("ghostel-line-mode.el" . "917ce62278f5f753e3b9a43412faabd902fb0bac73753fe40dd44dc64c823de2") ("ghostel-links.el" . "a0be28ad4873ee018c87773ee7e98b48b372f11a77f2335a92555a2e52bd3c78") ("ghostel-module-install.el" . "7f761a1848fc9f89b58781e2703c75e4a7870b3eb1b44afeb4e48d3fe89bf807") ("ghostel-pkg.el" . "19d6444fc849d964cd5c000c0e7c0b9ec1d0bdb0ddd1473cbd72638d68a66a23") ("ghostel-prompt.el" . "e7fb41f1c1e1d3939a13afe937a94149e2a8a0795b3def29ff51c8861d852ea9") ("ghostel.el" . "1feecf05d7813f3689e9e6611095269e88b04d85c3b82b1b349497dc138c3e7c")) :feature t :version "20260820.1035") :sorted ("*ghostel-a*" "*ghostel-b*" "*ghostel-c*") :after-a "*ghostel-b*" :wrap "*ghostel-a*" :before-b "*ghostel-a*" :empty (user-error "No ghostel buffers"))"#
        ]],
    )
}

fn send_string_and_send_key_signal_outside_a_terminal() -> ParityBatchCase {
    ParityBatchCase::value(
        "send_string_and_send_key_signal_outside_a_terminal",
        r####"
(list :source (gh493-test-source-state)
      :send
      (condition-case err
          (ghostel-send-string "café")
        (error (list (car err) (error-message-string err))))
      :key
      (condition-case err
          (ghostel-send-key "return")
        (error (list (car err) (error-message-string err)))))
"####,
        expect![[
            r#"OK (:source (:tree "f619bcc7834eb0866ab8148297ad7d7619ea7e81" :manifest (("ghostel-bookmark.el" . "997b7cfd83772ff82c418c57c57bd7bb46575d083cbda693691bb40cb1375421") ("ghostel-comint.el" . "11e5db1ae4b95513e75a36a2a9d10af5d4a94fdcc9859ecfb13102d23a5ead38") ("ghostel-compile.el" . "646bcedaa66bb4fd558498f6b43c5ab2fbb6db21138a9ba0f804eb70777c0da0") ("ghostel-debug.el" . "9e37616f8c6090d5a3a5e579ad35defb38836e236494d9db4f7c90ab998e39cc") ("ghostel-desktop.el" . "00150b9a2431aea466972d2681ae0360e5dd6452abe9ba1a9a46ebe1a22e0b25") ("ghostel-eshell.el" . "c31c34de84ad4beb304a23ebd477f8a986cce3e454fda415004e21e9b771f7b8") ("ghostel-faces.el" . "41d0604659be502b0d3fe862ca422d8312cecad1347890e5d03653e4987440ce") ("ghostel-ime.el" . "5ab293c3ecee2b45b904d8bab5bc2a2a3dcd3fafbc09bb8a30e4a694c11c3150") ("ghostel-kitty.el" . "a80498d38da5423e9c9263842c1faf5900e126f9f320139b7a57b6ba3cbb5dc3") ("ghostel-line-mode.el" . "917ce62278f5f753e3b9a43412faabd902fb0bac73753fe40dd44dc64c823de2") ("ghostel-links.el" . "a0be28ad4873ee018c87773ee7e98b48b372f11a77f2335a92555a2e52bd3c78") ("ghostel-module-install.el" . "7f761a1848fc9f89b58781e2703c75e4a7870b3eb1b44afeb4e48d3fe89bf807") ("ghostel-pkg.el" . "19d6444fc849d964cd5c000c0e7c0b9ec1d0bdb0ddd1473cbd72638d68a66a23") ("ghostel-prompt.el" . "e7fb41f1c1e1d3939a13afe937a94149e2a8a0795b3def29ff51c8861d852ea9") ("ghostel.el" . "1feecf05d7813f3689e9e6611095269e88b04d85c3b82b1b349497dc138c3e7c")) :feature t :version "20260820.1035") :send (user-error "Must be called from a ghostel buffer") :key (user-error "Must be called from a ghostel buffer"))"#
        ]],
    )
}

fn buffer_name_functions_format_title_and_cafe_directory() -> ParityBatchCase {
    ParityBatchCase::value(
        "buffer_name_functions_format_title_and_cafe_directory",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "ghostel-café"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (default-directory root))
  (unwind-protect
      (progn
        (make-directory root t)
        (list :source (gh493-test-source-state)
              :title (ghostel-buffer-name-by-title "yazi café")
              :empty (ghostel-buffer-name-by-title nil)
              :dir (ghostel-buffer-name-by-directory nil)))
    (when (file-exists-p root)
      (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "f619bcc7834eb0866ab8148297ad7d7619ea7e81" :manifest (("ghostel-bookmark.el" . "997b7cfd83772ff82c418c57c57bd7bb46575d083cbda693691bb40cb1375421") ("ghostel-comint.el" . "11e5db1ae4b95513e75a36a2a9d10af5d4a94fdcc9859ecfb13102d23a5ead38") ("ghostel-compile.el" . "646bcedaa66bb4fd558498f6b43c5ab2fbb6db21138a9ba0f804eb70777c0da0") ("ghostel-debug.el" . "9e37616f8c6090d5a3a5e579ad35defb38836e236494d9db4f7c90ab998e39cc") ("ghostel-desktop.el" . "00150b9a2431aea466972d2681ae0360e5dd6452abe9ba1a9a46ebe1a22e0b25") ("ghostel-eshell.el" . "c31c34de84ad4beb304a23ebd477f8a986cce3e454fda415004e21e9b771f7b8") ("ghostel-faces.el" . "41d0604659be502b0d3fe862ca422d8312cecad1347890e5d03653e4987440ce") ("ghostel-ime.el" . "5ab293c3ecee2b45b904d8bab5bc2a2a3dcd3fafbc09bb8a30e4a694c11c3150") ("ghostel-kitty.el" . "a80498d38da5423e9c9263842c1faf5900e126f9f320139b7a57b6ba3cbb5dc3") ("ghostel-line-mode.el" . "917ce62278f5f753e3b9a43412faabd902fb0bac73753fe40dd44dc64c823de2") ("ghostel-links.el" . "a0be28ad4873ee018c87773ee7e98b48b372f11a77f2335a92555a2e52bd3c78") ("ghostel-module-install.el" . "7f761a1848fc9f89b58781e2703c75e4a7870b3eb1b44afeb4e48d3fe89bf807") ("ghostel-pkg.el" . "19d6444fc849d964cd5c000c0e7c0b9ec1d0bdb0ddd1473cbd72638d68a66a23") ("ghostel-prompt.el" . "e7fb41f1c1e1d3939a13afe937a94149e2a8a0795b3def29ff51c8861d852ea9") ("ghostel.el" . "1feecf05d7813f3689e9e6611095269e88b04d85c3b82b1b349497dc138c3e7c")) :feature t :version "20260820.1035") :title "*ghostel: yazi café*" :empty nil :dir "*ghostel: [ORACLE-SANDBOX]/ghostel-café*")"#
        ]],
    )
}

fn other_switches_among_planted_terminals() -> ParityBatchCase {
    ParityBatchCase::value(
        "other_switches_among_planted_terminals",
        r####"
(let ((identity (current-buffer))
      (windows (current-window-configuration))
      here other popped)
  (unwind-protect
      (progn
        (setq here (gh493-test-plant "*ghostel-here*")
              other (gh493-test-plant "*ghostel-other*"))
        (cl-letf (((symbol-function 'pop-to-buffer)
                   (lambda (buf &rest _)
                     (setq popped (buffer-name buf))
                     buf)))
          (with-current-buffer here
            (ghostel-other))
          (list :source (gh493-test-source-state)
                :popped popped)))
    (dolist (buf (list here other))
      (when (buffer-live-p buf) (kill-buffer buf)))
    (set-window-configuration windows)
    (when (buffer-live-p identity)
      (set-buffer identity))))
"####,
        expect![[
            r#"OK (:source (:tree "f619bcc7834eb0866ab8148297ad7d7619ea7e81" :manifest (("ghostel-bookmark.el" . "997b7cfd83772ff82c418c57c57bd7bb46575d083cbda693691bb40cb1375421") ("ghostel-comint.el" . "11e5db1ae4b95513e75a36a2a9d10af5d4a94fdcc9859ecfb13102d23a5ead38") ("ghostel-compile.el" . "646bcedaa66bb4fd558498f6b43c5ab2fbb6db21138a9ba0f804eb70777c0da0") ("ghostel-debug.el" . "9e37616f8c6090d5a3a5e579ad35defb38836e236494d9db4f7c90ab998e39cc") ("ghostel-desktop.el" . "00150b9a2431aea466972d2681ae0360e5dd6452abe9ba1a9a46ebe1a22e0b25") ("ghostel-eshell.el" . "c31c34de84ad4beb304a23ebd477f8a986cce3e454fda415004e21e9b771f7b8") ("ghostel-faces.el" . "41d0604659be502b0d3fe862ca422d8312cecad1347890e5d03653e4987440ce") ("ghostel-ime.el" . "5ab293c3ecee2b45b904d8bab5bc2a2a3dcd3fafbc09bb8a30e4a694c11c3150") ("ghostel-kitty.el" . "a80498d38da5423e9c9263842c1faf5900e126f9f320139b7a57b6ba3cbb5dc3") ("ghostel-line-mode.el" . "917ce62278f5f753e3b9a43412faabd902fb0bac73753fe40dd44dc64c823de2") ("ghostel-links.el" . "a0be28ad4873ee018c87773ee7e98b48b372f11a77f2335a92555a2e52bd3c78") ("ghostel-module-install.el" . "7f761a1848fc9f89b58781e2703c75e4a7870b3eb1b44afeb4e48d3fe89bf807") ("ghostel-pkg.el" . "19d6444fc849d964cd5c000c0e7c0b9ec1d0bdb0ddd1473cbd72638d68a66a23") ("ghostel-prompt.el" . "e7fb41f1c1e1d3939a13afe937a94149e2a8a0795b3def29ff51c8861d852ea9") ("ghostel.el" . "1feecf05d7813f3689e9e6611095269e88b04d85c3b82b1b349497dc138c3e7c")) :feature t :version "20260820.1035") :popped "*ghostel-other*")"#
        ]],
    )
}

#[test]
fn ghostel_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        package_load_gates_source_and_exposes_mode_keys_and_defaults(),
        next_previous_cycle_planted_ghostel_buffers(),
        send_string_and_send_key_signal_outside_a_terminal(),
        buffer_name_functions_format_title_and_cafe_directory(),
        other_switches_among_planted_terminals(),
    ];
    assert_oracle_batch_cases(oracle(), "ghostel-package-batch", "ghostel_parity", &cases);
}
