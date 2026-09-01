use expect_test::expect;

use super::ParityBatchCase;

fn asm_blox_exact_pin_descriptor_dependency_origin_and_feature_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_exact_pin_descriptor_dependency_origin_and_feature_contract_match",
        r##"(let ((descriptor
                (cadr
                 (assq 'asm-blox package-alist))))
         (list
          (package-desc-name descriptor)
          (package-version-join
           (package-desc-version descriptor))
          (package-desc-summary descriptor)
          (package-desc-kind descriptor)
          (package-desc-reqs descriptor)
          (package-desc-extras descriptor)
          (featurep 'asm-blox)
          (featurep 'asm-blox-puzzles)
          (featurep 'yaml)))"##,
        expect![[
            r#"OK (asm-blox "20240106.1930" "Programming game involving WAT." nil ((emacs (26 1)) (yaml (0 5 1))) ((:keywords "games") (:revdesc . "6731d8e4f78d") (:commit . "6731d8e4f78d0b43ec9b90d8184c1d86d725ac7c") (:url . "https://github.com/zkry/asm-blox")) t t t)"#
        ]],
    )
}

fn asm_blox_installed_payload_inventory_sizes_and_content_digests_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_installed_payload_inventory_sizes_and_content_digests_match",
        r##"(let* ((descriptor
                  (cadr
                   (assq 'asm-blox package-alist)))
                 (directory
                  (package-desc-dir descriptor)))
         (mapcar
          (lambda (file)
            (let ((path
                   (expand-file-name file directory)))
              (list
               file
               (file-attribute-size
                (file-attributes path))
               (with-temp-buffer
                 (insert-file-contents-literally path)
                 (secure-hash
                  'sha256
                  (current-buffer))))))
          (sort
           (seq-filter
            (lambda (file)
              (file-regular-p
               (expand-file-name file directory)))
            (directory-files directory nil "\\`[^.]"))
           #'string<)))"##,
        expect![[
            r#"OK (("asm-blox-autoloads.el" 952 "2feebea65a2d99cf3ab0b1ebcae3878465f957781d8ff89d5a461fd25ddac49f") ("asm-blox-pkg.el" 316 "e66b987e19b09ce5d2c9b447a5b90491d88ffb982ed78bf1410f35854364049c") ("asm-blox-puzzles.el" 51075 "fb7d70d6d8e8057c5ac7712e3b7fcdd4f04243f739fb3c828e1cd4561c7773b9") ("asm-blox-puzzles.elc" 32187 "6953a8b047cabca0f349ee24a8f56d1ae9846c3ba719d994b32c49a82bbc7fa6") ("asm-blox.el" 159389 "25d33612f757c4d682cf8d13112460682668dda06c63b234bebfa14c937153a7") ("asm-blox.elc" 176059 "99fc3783ccf8f2fee157de04ccb7ad60df0194403cdb3b22c214547078b9a46a") ("asm-blox.info" 25413 "22d1301c320c68b7609115a07e5e028a92321590cd3559793216652f096e7b91") ("dir" 541 "1d727514971d5b4b2c807ee0d6b892a3714d3a631fe14fbf54c97617fa5bc027"))"#
        ]],
    )
}

fn asm_blox_complete_callable_command_arglist_and_source_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_complete_callable_command_arglist_and_source_surface_matches",
        r##"(let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "asm-blox"
                  (symbol-name symbol))
                 (not
                  (string-suffix-p
                   "--inliner"
                   (symbol-name symbol)))
                 (not
                  (string-suffix-p
                   "--cmacro"
                   (symbol-name symbol)))
                 (fboundp symbol)
                 (let ((file
                        (symbol-file symbol 'defun)))
                   (and file
                        (member
                         (file-name-nondirectory file)
                         '("asm-blox.el"
                           "asm-blox-puzzles.el")))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (commandp symbol)
             (interactive-form symbol)
             (prin1-to-string
              (help-function-arglist symbol t))
             (let ((file
                    (symbol-file symbol 'defun)))
               (and file
                    (file-name-nondirectory file)))))
          (sort symbols
                (lambda (left right)
                  (string<
                   (symbol-name left)
                   (symbol-name right))))))"##,
        expect![[
            r#"OK ((asm-blox t (interactive nil) "nil" "asm-blox.el") (asm-blox--backup-file-for-current-buffer nil nil "nil" "asm-blox.el") (asm-blox--beginning-of-box nil nil "nil" "asm-blox.el") (asm-blox--beginning-of-line nil nil "nil" "asm-blox.el") (asm-blox--binary-operation nil nil "(cell-runtime function)" "asm-blox.el") (asm-blox--box-point-forward nil nil "(ct)" "asm-blox.el") (asm-blox--build-font-lock-keywords nil nil "nil" "asm-blox.el") (asm-blox--cell-at-moved-row-col nil nil "(row col dir)" "asm-blox.el") (asm-blox--cell-at-row-col nil nil "(row col)" "asm-blox.el") (asm-blox--cell-message-at-pos nil nil "(row col)" "asm-blox.el") (asm-blox--cell-runtime-col nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-create nil nil "(&rest --cl-rest--)" "asm-blox.el") (asm-blox--cell-runtime-current-instruction nil nil "(cell-runtime)" "asm-blox.el") (asm-blox--cell-runtime-down nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-get nil nil "(cell-runtime direction)" "asm-blox.el") (asm-blox--cell-runtime-get-extra nil nil "(cell-runtime direction)" "asm-blox.el") (asm-blox--cell-runtime-instructions nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-instructions-length nil nil "(cell-runtime)" "asm-blox.el") (asm-blox--cell-runtime-left nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-merge-ports-with-staging nil nil "(cell-runtime)" "asm-blox.el") (asm-blox--cell-runtime-message-function nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-p nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-pc nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-pc-inc nil nil "(cell-runtime)" "asm-blox.el") (asm-blox--cell-runtime-pop nil nil "(cell-runtime)" "asm-blox.el") (asm-blox--cell-runtime-push nil nil "(cell-runtime value)" "asm-blox.el") (asm-blox--cell-runtime-right nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-row nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-run-function nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-run-spec nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-run-state nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-send nil nil "(cell-runtime direction)" "asm-blox.el") (asm-blox--cell-runtime-set-stack nil nil "(cell-runtime offset &optional op)" "asm-blox.el") (asm-blox--cell-runtime-set-staging-value-from-direction nil nil "(cell-runtime direction value)" "asm-blox.el") (asm-blox--cell-runtime-set-value-from-direction nil nil "(cell-runtime direction value)" "asm-blox.el") (asm-blox--cell-runtime-skip-labels nil nil "(cell-runtime)" "asm-blox.el") (asm-blox--cell-runtime-stack nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-stack-get nil nil "(cell-runtime loc)" "asm-blox.el") (asm-blox--cell-runtime-staging-down nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-staging-left nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-staging-right nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-staging-up nil nil "(x)" "asm-blox.el") (asm-blox--cell-runtime-step nil nil "(cell-runtime)" "asm-blox.el") (asm-blox--cell-runtime-up nil nil "(x)" "asm-blox.el") (asm-blox--cell-sink-col nil nil "(x)" "asm-blox.el") (asm-blox--cell-sink-create nil nil "(&rest --cl-rest--)" "asm-blox.el") (asm-blox--cell-sink-default-editor-text nil nil "(x)" "asm-blox.el") (asm-blox--cell-sink-editor-point nil nil "(x)" "asm-blox.el") (asm-blox--cell-sink-editor-text nil nil "(x)" "asm-blox.el") (asm-blox--cell-sink-err-val nil nil "(x)" "asm-blox.el") (asm-blox--cell-sink-expected-data nil nil "(x)" "asm-blox.el") (asm-blox--cell-sink-expected-text nil nil "(x)" "asm-blox.el") (asm-blox--cell-sink-get nil nil "(sink)" "asm-blox.el") (asm-blox--cell-sink-idx nil nil "(x)" "asm-blox.el") (asm-blox--cell-sink-insert-character nil nil "(sink char)" "asm-blox.el") (asm-blox--cell-sink-move-point nil nil "(sink point)" "asm-blox.el") (asm-blox--cell-sink-name nil nil "(x)" "asm-blox.el") (asm-blox--cell-sink-p nil nil "(x)" "asm-blox.el") (asm-blox--cell-sink-row nil nil "(x)" "asm-blox.el") (asm-blox--cell-source-col nil nil "(x)" "asm-blox.el") (asm-blox--cell-source-create nil nil "(&rest --cl-rest--)" "asm-blox.el") (asm-blox--cell-source-current-value nil nil "(source)" "asm-blox.el") (asm-blox--cell-source-data nil nil "(x)" "asm-blox.el") (asm-blox--cell-source-idx nil nil "(x)" "asm-blox.el") (asm-blox--cell-source-name nil nil "(x)" "asm-blox.el") (asm-blox--cell-source-p nil nil "(x)" "asm-blox.el") (asm-blox--cell-source-pop nil nil "(source)" "asm-blox.el") (asm-blox--cell-source-row nil nil "(x)" "asm-blox.el") (asm-blox--code-node-create nil nil "(&rest --cl-rest--)" "asm-blox.el") (asm-blox--code-node-validate nil nil "(code-node)" "asm-blox.el") (asm-blox--col-arrow-label-display nil nil "(position type row)" "asm-blox.el") (asm-blox--col-register-display nil nil "(row col direction)" "asm-blox.el") (asm-blox--coords-to-end-of-box nil nil "nil" "asm-blox.el") (asm-blox--create-execution-buffer nil nil "(box-contents extra-cells)" "asm-blox.el") (asm-blox--create-sexp-code-node nil nil "(row col code)" "asm-blox.el") (asm-blox--create-widges-from-gameboard nil nil "nil" "asm-blox.el") (asm-blox--create-yaml-code-node nil nil "(row col code)" "asm-blox.el") (asm-blox--display-widget nil nil "nil" "asm-blox.el") (asm-blox--draw-win-message nil nil "nil" "asm-blox.el") (asm-blox--ensure-buffer-not-empty nil nil "nil" "asm-blox.el") (asm-blox--execution-next-multiple-commands t (interactive nil) "nil" "asm-blox.el") (asm-blox--execution-run t (interactive nil) "nil" "asm-blox.el") (asm-blox--extra-gameboard-step nil nil "nil" "asm-blox.el") (asm-blox--find-closing-match nil nil "nil" "asm-blox.el") (asm-blox--find-opening-match nil nil "nil" "asm-blox.el") (asm-blox--flatten-list nil nil "(tree)" "asm-blox.el") (asm-blox--font-for-difficulty nil nil "(difficulty)" "asm-blox.el") (asm-blox--forward-line t (interactive nil) "nil" "asm-blox.el") (asm-blox--func-in-buffer nil nil "(func)" "asm-blox.el") (asm-blox--gameboard-in-final-state-p nil nil "nil" "asm-blox.el") (asm-blox--gameboard-source-at-pos nil nil "(row col &optional dir)" "asm-blox.el") (asm-blox--gameboard-step nil nil "nil" "asm-blox.el") (asm-blox--generate-new-puzzle-filename nil nil "(name)" "asm-blox.el") (asm-blox--get-box-content nil nil "(row col)" "asm-blox.el") (asm-blox--get-box-line-content nil nil "(row col line-no)" "asm-blox.el") (asm-blox--get-direction-col-registers nil nil "(row col direction)" "asm-blox.el") (asm-blox--get-direction-row-registers nil nil "(row col direction)" "asm-blox.el") (asm-blox--get-error-at-cell nil nil "(row col)" "asm-blox.el") (asm-blox--get-puzzle-by-id nil nil "(name)" "asm-blox.el") (asm-blox--get-sink-name-at-position nil nil "(row col)" "asm-blox.el") (asm-blox--get-source-idx-at-position nil nil "(row col)" "asm-blox.el") (asm-blox--get-value-from-direction nil nil "(cell-runtime direction)" "asm-blox.el") (asm-blox--get-value-from-staging-direction nil nil "(cell-runtime direction)" "asm-blox.el") (asm-blox--highlight-pairs nil nil "nil" "asm-blox.el") (asm-blox--in-buffer nil nil "(code)" "asm-blox.el") (asm-blox--initialize-box-contents nil nil "nil" "asm-blox.el") (asm-blox--initialize-undo-stacks nil nil "nil" "asm-blox.el") (asm-blox--kill nil nil "(beg end &optional copy-only)" "asm-blox.el") (asm-blox--make-editor-widget nil nil "(sink)" "asm-blox.el") (asm-blox--make-label nil nil "nil" "asm-blox.el") (asm-blox--make-puzzle-idx-file-name nil nil "(id idx)" "asm-blox.el") (asm-blox--make-sink-widget nil nil "(sink)" "asm-blox.el") (asm-blox--make-source-widget nil nil "(source)" "asm-blox.el") (asm-blox--match-keyword nil nil "(limit)" "asm-blox.el") (asm-blox--match-port nil nil "(limit)" "asm-blox.el") (asm-blox--mirror-direction nil nil "(direction)" "asm-blox.el") (asm-blox--move-point-to-end-of-box-content nil nil "nil" "asm-blox.el") (asm-blox--move-to-box nil nil "(row col)" "asm-blox.el") (asm-blox--move-to-box-point nil nil "(row col)" "asm-blox.el") (asm-blox--move-to-end-of-box nil nil "(row col)" "asm-blox.el") (asm-blox--newline t (interactive nil) "nil" "asm-blox.el") (asm-blox--next-row-cell t (interactive nil) "nil" "asm-blox.el") (asm-blox--on-edit-eldoc nil nil "nil" "asm-blox.el") (asm-blox--pair-create-overlays nil nil "(start end)" "asm-blox.el") (asm-blox--pair-delete-overlays nil nil "nil" "asm-blox.el") (asm-blox--parse-assembly nil nil "(code)" "asm-blox.el") (asm-blox--parse-cell nil nil "(coords code)" "asm-blox.el") (asm-blox--parse-error-p nil nil "(err)" "asm-blox.el") (asm-blox--parse-saved-buffer nil nil "nil" "asm-blox.el") (asm-blox--parse-tree-to-asm nil nil "(parse)" "asm-blox.el") (asm-blox--parse-tree-to-asm* nil nil "(parse)" "asm-blox.el") (asm-blox--point-context nil nil "nil" "asm-blox.el") (asm-blox--portp nil nil "(x)" "asm-blox.el") (asm-blox--printable-char-p nil nil "(c)" "asm-blox.el") (asm-blox--problem-spec-banned-commands nil nil "(x)" "asm-blox.el") (asm-blox--problem-spec-create nil nil "(&rest --cl-rest--)" "asm-blox.el") (asm-blox--problem-spec-description nil nil "(x)" "asm-blox.el") (asm-blox--problem-spec-difficulty nil nil "(x)" "asm-blox.el") (asm-blox--problem-spec-name nil nil "(x)" "asm-blox.el") (asm-blox--problem-spec-p nil nil "(x)" "asm-blox.el") (asm-blox--problem-spec-sinks nil nil "(x)" "asm-blox.el") (asm-blox--problem-spec-sources nil nil "(x)" "asm-blox.el") (asm-blox--propertize-errors nil nil "nil" "asm-blox.el") (asm-blox--push-undo-stack-value nil nil "nil" "asm-blox.el") (asm-blox--puzzle-selection-setup-buffer nil nil "(id)" "asm-blox.el") (asm-blox--puzzle-won-p nil nil "(puzzle-name)" "asm-blox.el") (asm-blox--puzzles-by-difficulty nil nil "nil" "asm-blox.el") (asm-blox--refresh-contents t (interactive nil) "nil" "asm-blox.el") (asm-blox--remove-value-from-direction nil nil "(cell-runtime direction)" "asm-blox.el") (asm-blox--remove-value-from-staging-direction nil nil "(cell-runtime direction)" "asm-blox.el") (asm-blox--replace-box-text nil nil "(text)" "asm-blox.el") (asm-blox--reset-extra-gameboard-cells-state nil nil "nil" "asm-blox.el") (asm-blox--resolve-labels nil nil "(asm)" "asm-blox.el") (asm-blox--resolve-port-values nil nil "nil" "asm-blox.el") (asm-blox--restore-backup nil nil "nil" "asm-blox.el") (asm-blox--row-arrow-label-display nil nil "(position type col)" "asm-blox.el") (asm-blox--row-register-display nil nil "(row col direction)" "asm-blox.el") (asm-blox--saved-puzzle-ct-ids nil nil "(id)" "asm-blox.el") (asm-blox--set-box-content nil nil "(row col text)" "asm-blox.el") (asm-blox--set-cell-at-row-col nil nil "(row col cell-runtime)" "asm-blox.el") (asm-blox--shift-box nil nil "(drow dcol)" "asm-blox.el") (asm-blox--step nil nil "nil" "asm-blox.el") (asm-blox--swap-box-contents nil nil "(row-1 col-1 row-2 col-2)" "asm-blox.el") (asm-blox--swap-undo-stacks nil nil "(row-1 col-1 row-2 col-2)" "asm-blox.el") (asm-blox--transform-sexp-data nil nil "(plist)" "asm-blox.el") (asm-blox--true-p nil nil "(v)" "asm-blox.el") (asm-blox--unary-operation nil nil "(cell-runtime function)" "asm-blox.el") (asm-blox--undo-state-box-col nil nil "(x)" "asm-blox.el") (asm-blox--undo-state-box-row nil nil "(x)" "asm-blox.el") (asm-blox--undo-state-create nil nil "(&rest --cl-rest--)" "asm-blox.el") (asm-blox--undo-state-p nil nil "(x)" "asm-blox.el") (asm-blox--undo-state-redo-list nil nil "(x)" "asm-blox.el") (asm-blox--undo-state-text nil nil "(x)" "asm-blox.el") (asm-blox--valid-position nil nil "(row col &optional dir)" "asm-blox.el") (asm-blox--verify-controller nil nil "(spec)" "asm-blox.el") (asm-blox--verify-heap nil nil "(spec)" "asm-blox.el") (asm-blox--verify-port nil nil "(name port)" "asm-blox.el") (asm-blox--verify-stack nil nil "(spec)" "asm-blox.el") (asm-blox--win-file-for-current-buffer nil nil "nil" "asm-blox.el") (asm-blox--yaml-create-controller nil nil "(row col _ spec)" "asm-blox.el") (asm-blox--yaml-create-heap nil nil "(row col _ spec)" "asm-blox.el") (asm-blox--yaml-create-stack nil nil "(row col _ spec)" "asm-blox.el") (asm-blox--yaml-get-editor-sink nil nil "(_)" "asm-blox.el") (asm-blox--yaml-message-heap nil nil "(cell-runtime)" "asm-blox.el") (asm-blox--yaml-message-stack nil nil "(cell-runtime)" "asm-blox.el") (asm-blox--yaml-step-controller nil nil "(cell-runtime)" "asm-blox.el") (asm-blox--yaml-step-heap nil nil "(cell-runtime)" "asm-blox.el") (asm-blox--yaml-step-stack nil nil "(cell-runtime)" "asm-blox.el") (asm-blox-backward-delete-char t (interactive nil) "nil" "asm-blox.el") (asm-blox-beginning-of-buffer t (interactive nil) "nil" "asm-blox.el") (asm-blox-check-winning-conditions nil nil "nil" "asm-blox.el") (asm-blox-code-node-children nil nil "(x)" "asm-blox.el") (asm-blox-code-node-end-pos nil nil "(x)" "asm-blox.el") (asm-blox-code-node-p nil nil "(x)" "asm-blox.el") (asm-blox-code-node-start-pos nil nil "(x)" "asm-blox.el") (asm-blox-complete t (interactive nil) "nil" "asm-blox.el") (asm-blox-copy-region t (interactive "r") "(beg end)" "asm-blox.el") (asm-blox-delete-char t (interactive nil) "nil" "asm-blox.el") (asm-blox-display--insert-middle-row-space nil nil "(row)" "asm-blox.el") (asm-blox-display--insert-row-bottom nil nil "(row)" "asm-blox.el") (asm-blox-display--insert-row-middle nil nil "(row box-row)" "asm-blox.el") (asm-blox-display--insert-row-top nil nil "(row)" "asm-blox.el") (asm-blox-display--insert-v-border nil nil "(position)" "asm-blox.el") (asm-blox-display-game-board nil nil "nil" "asm-blox.el") (asm-blox-eldoc nil nil "(&rest _ignored)" "asm-blox.el") (asm-blox-eldoc-setup nil nil "nil" "asm-blox.el") (asm-blox-end-of-buffer t (interactive nil) "nil" "asm-blox.el") (asm-blox-execution-code-highlight nil nil "nil" "asm-blox.el") (asm-blox-execution-draw-stack nil nil "nil" "asm-blox.el") (asm-blox-execution-mode t (interactive nil) "nil" "asm-blox.el") (asm-blox-execution-next-command t (interactive nil) "nil" "asm-blox.el") (asm-blox-get-line-col-num nil nil "(&optional point)" "asm-blox.el") (asm-blox-in-box-p nil nil "nil" "asm-blox.el") (asm-blox-kill-line t (interactive nil) "nil" "asm-blox.el") (asm-blox-kill-region t (interactive "r") "(beg end)" "asm-blox.el") (asm-blox-kill-word t (interactive nil) "nil" "asm-blox.el") (asm-blox-mode t (interactive nil) "nil" "asm-blox.el") (asm-blox-move-beginning-of-line t (interactive nil) "nil" "asm-blox.el") (asm-blox-move-end-of-line t (interactive nil) "nil" "asm-blox.el") (asm-blox-next-cell t (interactive nil) "nil" "asm-blox.el") (asm-blox-prev-cell t (interactive nil) "nil" "asm-blox.el") (asm-blox-puzzle-selection-mode t (interactive nil) "nil" "asm-blox.el") (asm-blox-puzzle-selection-prepare-buffer nil nil "nil" "asm-blox.el") (asm-blox-puzzle-selection-refresh t (interactive nil) "nil" "asm-blox.el") (asm-blox-puzzles--add nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--clock nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--constant nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--delete-word nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--diagnostic-test nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--differential-converter nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--filter nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--hello-world nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--identity nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--inc-ct nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--indentation nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--interrupt-handler nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--list-length nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--list-reverse nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--make-interrupt-handler-seq nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--make-interrupt-handler-solution nil nil "(a b c d)" "asm-blox-puzzles.el") (asm-blox-puzzles--meeting-point nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--merge-step nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--number-sorter nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--number-sum nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--sequence-counter nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--sequence-generator nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--sequence-indexer nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--sequence-peak-detector nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--sequence-reverser nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--sequence-sorter nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--signal-amplifier nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--signal-comparator nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--signal-divider nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--signal-edge-detector nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--signal-multiplier nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--signal-pattern-detector nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--signal-window-filter nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--simple-graph nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--stack-machine nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--stack-machine-solver nil nil "(args ops)" "asm-blox-puzzles.el") (asm-blox-puzzles--triangle-area nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--turing nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles--upcase nil nil "nil" "asm-blox-puzzles.el") (asm-blox-puzzles-list-of-lists-to-lisp nil nil "(lists)" "asm-blox-puzzles.el") (asm-blox-puzzles-random-list-of-lists nil nil "(&optional limit)" "asm-blox-puzzles.el") (asm-blox-redo t (interactive nil) "nil" "asm-blox.el") (asm-blox-redraw-game-board nil nil "nil" "asm-blox.el") (asm-blox-select-puzzle t (interactive nil) "nil" "asm-blox.el") (asm-blox-self-insert-command t (interactive nil) "nil" "asm-blox.el") (asm-blox-shift-box-down t (interactive nil) "nil" "asm-blox.el") (asm-blox-shift-box-left t (interactive nil) "nil" "asm-blox.el") (asm-blox-shift-box-right t (interactive nil) "nil" "asm-blox.el") (asm-blox-shift-box-up t (interactive nil) "nil" "asm-blox.el") (asm-blox-start-execution t (interactive nil) "nil" "asm-blox.el") (asm-blox-undo t (interactive nil) "nil" "asm-blox.el") (asm-blox-yank t (interactive nil) "nil" "asm-blox.el"))"#
        ]],
    )
}

fn asm_blox_complete_owned_variable_defaults_scope_custom_and_source_surface_matches()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_complete_owned_variable_defaults_scope_custom_and_source_surface_matches",
        r##"(cl-labels
        ((stable
          (value)
          (cond
           ((keymapp value)
            :keymap)
           ((char-table-p value)
            (list
             :char-table
             (char-table-subtype value)))
           ((hash-table-p value)
            (let (entries)
              (maphash
               (lambda (key item)
                 (push
                  (cons
                   (stable key)
                   (stable item))
                  entries))
               value)
              (list
               :hash-table
               (sort
                entries
                (lambda (left right)
                  (string<
                   (prin1-to-string left)
                   (prin1-to-string right)))))))
           ((and
             (functionp value)
             (not
              (symbolp value)))
            :function)
           ((consp value)
            (cons
             (stable (car value))
             (stable (cdr value))))
           ((vectorp value)
            (cons
             :vector
             (mapcar
              #'stable
              (append value nil))))
           (t value))))
       (let (symbols)
         (mapatoms
          (lambda (symbol)
            (when
                (and
                 (string-prefix-p
                  "asm-blox"
                  (symbol-name symbol))
                 (boundp symbol)
                 (let ((file
                        (symbol-file symbol 'defvar)))
                   (and file
                        (member
                         (file-name-nondirectory file)
                         '("asm-blox.el"
                           "asm-blox-puzzles.el")))))
              (push symbol symbols))))
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (condition-case error
                 (prin1-to-string
                  (stable
                   (default-value symbol)))
               (error
                (prin1-to-string
                 (stable error))))
             (special-variable-p symbol)
             (local-variable-if-set-p symbol)
             (custom-variable-p symbol)
             (prin1-to-string
              (stable
               (get symbol 'custom-type)))
             (prin1-to-string
              (stable
               (get symbol 'custom-group)))
             (let ((file
                    (symbol-file symbol 'defvar)))
               (and file
                    (file-name-nondirectory file)))))
          (sort symbols
                (lambda (left right)
                  (string<
                   (symbol-name left)
                   (symbol-name right)))))))"##,
        expect![[
            r##"OK ((asm-blox--all-completions "(\"set\" \"clr\" \"const\" \"dup\" \"abs\" \"add\" \"sub\" \"mul\" \"div\" \"neg\" \"rem\" \"and\" \"not\" \"or\" \"eq\" \"ne\" \"lt\" \"le\" \"gt\" \"ge\" \"gz\" \"lz\" \"eqz\" \"block\" \"loop\" \"inc\" \"dec\" \"br_if\" \"br\" \"nop\" \"drop\" \"send\" \"get\" \"left\" \"right\" \"up\" \"down\" \"fn\")" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--beginning-of-box-points "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--branch-labels "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--current-widgets "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--disable-redraw "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--display-mode "edit" t t nil "nil" "nil" "asm-blox.el") (asm-blox--end-of-box-points "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--extra-gameboard-cells "nil" t t nil "nil" "nil" "asm-blox.el") (asm-blox--gameboard "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--gameboard-col-ct "4" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--gameboard-row-ct "3" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--gameboard-state "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--keywords "(\"SET\" \"CLR\" \"CONST\" \"DUP\" \"ABS\" \"ADD\" \"SUB\" \"MUL\" \"DIV\" \"NEG\" \"REM\" \"AND\" \"NOT\" \"OR\" \"EQ\" \"NE\" \"LT\" \"LE\" \"GT\" \"GE\" \"GZ\" \"LZ\" \"EQZ\" \"BLOCK\" \"LOOP\" \"INC\" \"DEC\" \"BR_IF\" \"BR\" \"NOP\" \"DROP\" \"SEND\" \"GET\" \"LEFT\" \"RIGHT\" \"UP\" \"DOWN\" \"FN\")" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--mirror-buffer-name "\"*asm-blox-temp*\"" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--parse-depth "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--show-pair-idle-timer "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--skip-initial-parsing "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--undo-stacks "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox--widget-row-idx "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-base-operations "(GET SET TEE CONST NULL IS_NULL DROP NOP ADD INC DEC SUB MUL DIV REM AND OR EQZ GZ LZ EQ NE LT GT GE LE SEND PUSH POP CLR NOT DUP ABS)" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-box-contents "nil" t t nil "nil" "nil" "asm-blox.el") (asm-blox-box-height "12" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-box-width "20" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-command-specs "((SET integerp asm-blox--subexpressions) (CLR) (CONST integerp) (DUP asm-blox--subexpressions) (ABS asm-blox--subexpressions) (ADD asm-blox--subexpressions) (SUB asm-blox--subexpressions) (MUL asm-blox--subexpressions) (DIV asm-blox--subexpressions) (NEG asm-blox--subexpressions) (REM asm-blox--subexpressions) (AND asm-blox--subexpressions) (NOT asm-blox--subexpressions) (OR asm-blox--subexpressions) (EQ asm-blox--subexpressions) (NE asm-blox--subexpressions) (LT asm-blox--subexpressions) (LE asm-blox--subexpressions) (GT asm-blox--subexpressions) (GE asm-blox--subexpressions) (GZ asm-blox--subexpressions) (LZ asm-blox--subexpressions) (EQZ asm-blox--subexpressions) (BLOCK asm-blox--subexpressions) (LOOP asm-blox--subexpressions) (INC integerp) (DEC integerp) (BR_IF integerp) (BR integerp) (NOP) (DROP asm-blox--subexpressions) (SEND asm-blox--portp asm-blox--subexpressions) (GET :function) (LEFT) (RIGHT) (UP) (DOWN) (FN t))" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-display--arrow-down "\"↓\"" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-display--arrow-left "\"←\"" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-display--arrow-right "\"→\"" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-display--arrow-up "\"↑\"" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-display--box-inside "\"                    \"" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-display--box-line-top-bottom "\"────────────────────\"" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-display--space-between "\"     \"" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-display--space-start "\"      \"" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-display-chars "(:hash-table ((:box-bottom-left . 9492) (:box-bottom-right . 9496) (:box-horizontal . 9472) (:box-top-left . 9484) (:box-top-right . 9488) (:box-vertical . 9474)))" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-eldoc-specs "((SET \"POP -> X; Set stack item nth from the bottom to X.\" stack-offset rest) (CLR \"clear the entire stack.\") (CONST \"Push number onto the stack.\" number) (DUP \"duplicate the stack (must be only 1 or 2 items on stack)\" rest) (ABS \"POP -> X; PUSH the absolute value of X.\" rest) (ADD \"POP -> X; POP -> Y; PUSH Y + X.\" rest) (SUB \"POP -> X; POP -> Y; PUSH Y - X.\" rest) (MUL \"POP -> X; POP -> Y; PUSH Y * X.\" rest) (DIV \"POP -> X; POP -> Y; PUSH Y / X.\" rest) (NEG \"POP -> X; PUSH -X\" rest) (REM \"POP -> X; POP -> Y; PUSH Y % X.\" rest) (AND \"POP -> X; POP -> Y; PUSH 1 if X and Y are non-0, otherwise 0.\" rest) (NOT \"POP -> X; PUSH 1 if X is 0; otherwise 1;\" rest) (OR \"POP -> X; POP -> Y; PUSH 1 if X or Y are non-0, otherwise 0.\" rest) (EQ \"POP -> X; POP -> Y; PUSH 1 if Y = X, otherwise 0.\" rest) (NE \"POP -> X; POP -> Y; PUSH 1 if Y != X, otherwise 0.\" rest) (LT \"POP -> X; POP -> Y; PUSH 1 if Y < X, otherwise 0.\" rest) (LE \"POP -> X; POP -> Y; PUSH 1 if Y <= X, otherwise 0.\" rest) (GT \"POP -> X; POP -> Y; PUSH 1 if Y > X, otherwise 0.\" rest) (GE \"POP -> X; POP -> Y; PUSH 1 if Y >= X, otherwise 0.\" rest) (GZ \"POP -> X; PUSH 1 if X > 0.\" rest) (LZ \"POP -> X; PUSH 1 if X < 0.\" rest) (EQZ \"POP -> X; PUSH 1 if X = 0.\" rest) (BLOCK \"create block control flow\" rest) (LOOP \"create loop control flow\" rest) (INC \"increment value on stack stack-offset from bottom (-1 means top)\" stack-offset) (DEC \"decrement value on stack stack-offset from bottom (-1 means top)\" stack-offset) (BR_IF \"POP -> X; exit to control-flow at nesting-level if X is not 0.\" nesting-level) (BR \"exit to control-flow at nesting-level if X is not 0.\" nesting-level) (NOP \"do nothing\") (DROP \"pop top item on stack\" rest) (SEND \"POP -> X; sent X to port.\" port rest) (GET \"PUSH item from port or stack-offset\" stack-offset-or-port) (LEFT \"PUSH item from left port\") (RIGHT \"PUSH item from right port\") (UP \"PUSH item from up port\") (DOWN \"PUSH item from down port\"))" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-execution-mode-abbrev-table "#<obarray n=1>" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-execution-mode-hook "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-execution-mode-map ":keymap" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-execution-origin-buffer "nil" t t nil "nil" "nil" "asm-blox.el") (asm-blox-mode-abbrev-table "#<obarray n=1>" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-mode-hook "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-mode-map ":keymap" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-mode-syntax-table "(:char-table syntax-table)" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-multi-step-ct "10" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-pair-overlays "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-parse-errors "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-puzzle-selection-mode-map ":keymap" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-puzzles "(asm-blox-puzzles--indentation asm-blox-puzzles--constant asm-blox-puzzles--identity asm-blox-puzzles--add asm-blox-puzzles--filter asm-blox-puzzles--number-sum asm-blox-puzzles--number-sorter asm-blox-puzzles--clock asm-blox-puzzles--list-length asm-blox-puzzles--list-reverse asm-blox-puzzles--inc-ct asm-blox-puzzles--upcase asm-blox-puzzles--merge-step asm-blox-puzzles--hello-world asm-blox-puzzles--simple-graph asm-blox-puzzles--meeting-point asm-blox-puzzles--turing asm-blox-puzzles--stack-machine asm-blox-puzzles--delete-word asm-blox-puzzles--triangle-area asm-blox-puzzles--diagnostic-test asm-blox-puzzles--signal-amplifier asm-blox-puzzles--differential-converter asm-blox-puzzles--signal-comparator asm-blox-puzzles--sequence-generator asm-blox-puzzles--sequence-counter asm-blox-puzzles--signal-edge-detector asm-blox-puzzles--interrupt-handler asm-blox-puzzles--signal-pattern-detector asm-blox-puzzles--sequence-peak-detector asm-blox-puzzles--sequence-reverser asm-blox-puzzles--signal-multiplier asm-blox-puzzles--signal-window-filter asm-blox-puzzles--signal-divider asm-blox-puzzles--sequence-indexer asm-blox-puzzles--sequence-sorter)" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-runtime-error "nil" t nil nil "nil" "nil" "asm-blox.el") (asm-blox-save-directory-name "\"[ORACLE-HOME]/.emacs.d/.asm-blox\"" t nil ((funcall #'#[nil ((expand-file-name ".asm-blox" user-emacs-directory)) (t)])) "directory" "nil" "asm-blox.el"))"##
        ]],
    )
    .fresh_process()
}

fn asm_blox_struct_layout_constructor_predicate_accessor_and_mutation_contracts_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_struct_layout_constructor_predicate_accessor_and_mutation_contracts_match",
        r##"(let* ((node
                 (asm-blox--code-node-create
                  :children '(CONST 7)
                  :start-pos 3
                  :end-pos 12))
                (runtime
                 (asm-blox--cell-runtime-create
                  :instructions (list node)
                  :pc 0
                  :stack '(2 1)
                  :row 1
                  :col 2))
                (source
                 (asm-blox--cell-source-create
                  :row -1 :col 2
                  :data '(4 5) :name "I"))
                (sink
                 (asm-blox--cell-sink-create
                  :row 3 :col 1
                  :expected-data '(9)
                  :name "O"
                  :editor-text "abc"
                  :editor-point 2))
                (problem
                 (asm-blox--problem-spec-create
                  :name "Fixture"
                  :difficulty 'medium
                  :sources (list source)
                  :sinks (list sink)
                  :description "Practical fixture"
                  :banned-commands '(DIV)))
                (undo
                 (asm-blox--undo-state-create
                  :text "old"
                  :box-row 4
                  :box-col 6
                  :redo-list nil)))
         (setf
          (asm-blox-code-node-end-pos node) 13
          (asm-blox--cell-runtime-stack runtime) '(8 2 1)
          (asm-blox--cell-source-idx source) 1
          (asm-blox--cell-sink-idx sink) 1
          (asm-blox--cell-sink-err-val sink) 9
          (asm-blox--problem-spec-difficulty problem) 'hard
          (asm-blox--undo-state-redo-list undo) '(redo))
         (list
          (list
           (asm-blox-code-node-p node)
           (asm-blox-code-node-children node)
           (asm-blox-code-node-start-pos node)
           (asm-blox-code-node-end-pos node))
          (asm-blox-test-runtime-summary runtime)
          (asm-blox-test-source-summary source)
          (asm-blox-test-sink-summary sink)
          (asm-blox-test-problem-summary problem)
          (list
           (asm-blox--undo-state-p undo)
           (asm-blox--undo-state-text undo)
           (asm-blox--undo-state-box-row undo)
           (asm-blox--undo-state-box-col undo)
           (asm-blox--undo-state-redo-list undo))))"##,
        expect![[
            r#"OK ((t (CONST 7) 3 13) (:row 1 :col 2 :pc 0 :stack (8 2 1) :ports (nil nil nil nil) :staging (nil nil nil nil) :state nil) (-1 2 #1=(4 5) "I" 1) (3 1 #2=(9) "O" 1 9 "abc" 2 nil) ("Fixture" hard ((-1 2 #1# "I" 1)) ((3 1 #2# "O" 1 9 "abc" 2 nil)) "Practical fixture" (DIV)) (t "old" 4 6 (redo)))"#
        ]],
    )
}

fn asm_blox_keymaps_modes_syntax_completion_font_lock_and_eldoc_registry_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asm_blox_keymaps_modes_syntax_completion_font_lock_and_eldoc_registry_match",
        r##"(list
         (mapcar
          (lambda (entry)
            (list
             (car entry)
             (lookup-key
              (cdr entry)
              (kbd
               (car entry)))))
          (list
           (cons "C-c C-c" asm-blox-mode-map)
           (cons "RET" asm-blox-mode-map)
           (cons "<tab>" asm-blox-mode-map)
           (cons "C-w" asm-blox-mode-map)
           (cons "M-<right>" asm-blox-mode-map)
           (cons "n" asm-blox-execution-mode-map)
           (cons "N" asm-blox-execution-mode-map)
           (cons "r" asm-blox-execution-mode-map)
           (cons "RET" asm-blox-puzzle-selection-mode-map)))
         (with-syntax-table
             asm-blox-mode-syntax-table
           (char-syntax ?|))
         (length asm-blox-command-specs)
         (length asm-blox--all-completions)
         (seq-take asm-blox--all-completions 8)
         (length asm-blox--keywords)
         (asm-blox--build-font-lock-keywords)
         (assoc 'SEND asm-blox-eldoc-specs)
         (assoc 'BR_IF asm-blox-eldoc-specs)
         (assoc "\\.asbx\\'" auto-mode-alist))"##,
        expect![[
            r#"OK ((("C-c C-c" asm-blox-start-execution) ("RET" asm-blox--newline) ("<tab>" asm-blox-next-cell) ("C-w" asm-blox-kill-region) ("M-<right>" asm-blox-shift-box-right) ("n" asm-blox-execution-next-command) ("N" asm-blox--execution-next-multiple-commands) ("r" asm-blox--execution-run) ("RET" asm-blox-select-puzzle)) 32 38 38 ("set" "clr" "const" "dup" "abs" "add" "sub" "mul") 38 (("module" . font-lock-keyword-face) (asm-blox--match-keyword (1 font-lock-keyword-face)) (":[a-zA-Z-]+" . font-lock-constant-face) ("\\(;[^\n]*?\\)│" (1 font-lock-comment-face)) (asm-blox--match-port (0 font-lock-keyword-face))) (SEND "POP -> X; sent X to port." port rest) (BR_IF "POP -> X; exit to control-flow at nesting-level if X is not 0." nesting-level) ("\\.asbx\\'" . asm-blox-mode))"#
        ]],
    )
}

fn asm_blox_generated_autoload_registers_entry_points_without_loading_feature() -> ParityBatchCase {
    ParityBatchCase::value(
        "asm_blox_generated_autoload_registers_entry_points_without_loading_feature",
        r##"(list
         (featurep 'asm-blox)
         (featurep 'asm-blox-puzzles)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (and
              (fboundp symbol)
              (autoloadp
               (symbol-function symbol)))
             (commandp symbol)
             (interactive-form symbol)
             (let ((file
                    (symbol-file symbol 'defun)))
               (and file
                    (file-name-nondirectory file)))))
          '(asm-blox
            asm-blox-mode))
         (memq 'asm-blox package-activated-list))"##,
        expect![[
            r#"OK (nil nil ((asm-blox t t t (interactive nil) "asm-blox.el") (asm-blox-mode t nil t (interactive nil) "asm-blox.el")) (asm-blox yaml))"#
        ]],
    )
}

pub(super) fn registry_asm_blox_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asm_blox_exact_pin_descriptor_dependency_origin_and_feature_contract_match(),
        asm_blox_installed_payload_inventory_sizes_and_content_digests_match(),
        asm_blox_complete_callable_command_arglist_and_source_surface_matches(),
        asm_blox_complete_owned_variable_defaults_scope_custom_and_source_surface_matches(),
        asm_blox_struct_layout_constructor_predicate_accessor_and_mutation_contracts_match(),
        asm_blox_keymaps_modes_syntax_completion_font_lock_and_eldoc_registry_match(),
    ]
}

pub(super) fn registry_asm_blox_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![asm_blox_generated_autoload_registers_entry_points_without_loading_feature()]
}
