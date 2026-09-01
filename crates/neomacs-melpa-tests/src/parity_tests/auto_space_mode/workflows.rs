use expect_test::expect;

use super::ParityBatchCase;

fn typing_a_multilingual_release_note_spaces_only_real_script_boundaries() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (with-temp-buffer
      (neomacs-auto-space-test--reset)
      (auto-space-mode 1)
      (neomacs-auto-space-test--type
       (concat "Core" (string #x20000) "A かなB 한C 接口_v2 预算$5"))
      (list
       :text (buffer-substring-no-properties (point-min) (point-max))
       :point (point)
       :point-max (point-max)
       :at-eob (eobp)
       :mode auto-space-mode
       :hook-count (neomacs-auto-space-test--hook-count)))
  (neomacs-auto-space-test--reset))
"####;
    let expect = expect![[
        r####"OK (:text "Core 𠀀 A かな B 한 C 接口_v2 预算 $5" :point 30 :point-max 30 :at-eob t :mode t :hook-count 1)"####
    ]];
    ParityBatchCase::value(
        "typing_a_multilingual_release_note_spaces_only_real_script_boundaries",
        elisp_form,
        expect,
    )
}

fn one_typing_action_and_its_automatic_space_undo_as_one_edit() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (with-temp-buffer
      (neomacs-auto-space-test--reset)
      (buffer-enable-undo)
      (insert "版本")
      (set-buffer-modified-p nil)
      (setq buffer-undo-list nil)
      (goto-char (point-max))
      (auto-space-mode 1)
      (self-insert-command 1 ?A)
      (let ((after-typing
             (list
              :text (buffer-substring-no-properties (point-min) (point-max))
              :point (point)
              :modified (buffer-modified-p))))
        (undo-boundary)
        (undo-only 1)
        (list
         :after-typing after-typing
         :after-undo
         (list
          :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :modified (buffer-modified-p)))))
  (neomacs-auto-space-test--reset))
"####;
    let expect = expect![[
        r####"OK (:after-typing (:text "版本 A" :point 5 :modified t) :after-undo (:text "版本" :point 3 :modified nil))"####
    ]];
    ParityBatchCase::value(
        "one_typing_action_and_its_automatic_space_undo_as_one_edit",
        elisp_form,
        expect,
    )
}

fn enabling_the_mode_leaves_existing_prose_untouched_and_spaces_new_input() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (with-temp-buffer
      (neomacs-auto-space-test--reset)
      (insert "历史API remains compact\n")
      (set-buffer-modified-p nil)
      (auto-space-mode 1)
      (let ((after-enable
             (list
              :text (buffer-substring-no-properties (point-min) (point-max))
              :modified (buffer-modified-p))))
        (goto-char (point-max))
        (neomacs-auto-space-test--type "新V2")
        (list
         :after-enable after-enable
         :after-typing
         (list
          :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :modified (buffer-modified-p)))))
  (neomacs-auto-space-test--reset))
"####;
    let expect = expect![[
        r####"OK (:after-enable (:text "历史API remains compact\n" :modified nil) :after-typing (:text "历史API remains compact\n新 V2" :point 27 :modified t))"####
    ]];
    ParityBatchCase::value(
        "enabling_the_mode_leaves_existing_prose_untouched_and_spaces_new_input",
        elisp_form,
        expect,
    )
}

fn region_add_formats_a_practical_bilingual_release_paragraph() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert "Release说明: Neomacs支持Rust与Emacs29；预算$5用于CI。\n")
  (goto-char (point-min))
  (search-forward "Neomacs")
  (let ((before (list (point) (current-column)))
        (return-value
         (auto-space-add-in-region (point-min) (point-max))))
    (list
     :return return-value
     :text (buffer-substring-no-properties (point-min) (point-max))
     :point-before before
     :point-after (list (point) (current-column))
     :modified (buffer-modified-p))))
"####;
    let expect = expect![[
        r####"OK (:return nil :text "Release 说明: Neomacs 支持 Rust 与 Emacs29；预算 $5 用于 CI。\n" :point-before (19 20) :point-after (20 21) :modified t)"####
    ]];
    ParityBatchCase::value(
        "region_add_formats_a_practical_bilingual_release_paragraph",
        elisp_form,
        expect,
    )
}

fn region_remove_compacts_only_cjk_ascii_whitespace_boundaries() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert "版本  2 发布 日期\tAPI server  状态  OK\n")
  (goto-char (point-min))
  (search-forward "日期")
  (let ((before (list (point) (current-column)))
        (return-value
         (auto-space-remove-in-region (point-min) (point-max))))
    (list
     :return return-value
     :text (buffer-substring-no-properties (point-min) (point-max))
     :point-before before
     :point-after (list (point) (current-column))
     :modified (buffer-modified-p))))
"####;
    let expect = expect![[
        r####"OK (:return nil :text "版本2发布 日期API server状态OK\n" :point-before (12 17) :point-after (9 14) :modified t)"####
    ]];
    ParityBatchCase::value(
        "region_remove_compacts_only_cjk_ascii_whitespace_boundaries",
        elisp_form,
        expect,
    )
}

fn selected_region_uses_an_advancing_end_marker_and_preserves_point() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert "前A中B后 Ω")
  (let (start end)
    (goto-char (point-min))
    (search-forward "A")
    (setq start (1- (point)))
    (search-forward "B")
    (setq end (point))
    (goto-char (point-max))
    (let ((point-before (point)))
      (auto-space-add-in-region start end)
      (list
       :selection-before (list start end)
       :text (buffer-substring-no-properties (point-min) (point-max))
       :point-before point-before
       :point-after (point)
       :point-max (point-max)))))
"####;
    let expect = expect![[
        r####"OK (:selection-before (2 5) :text "前A 中 B 后 Ω" :point-before 8 :point-after 11 :point-max 11)"####
    ]];
    ParityBatchCase::value(
        "selected_region_uses_an_advancing_end_marker_and_preserves_point",
        elisp_form,
        expect,
    )
}

fn global_mode_is_idempotent_across_buffers_and_disable_stops_future_spacing() -> ParityBatchCase {
    let elisp_form = r####"
(let ((first (generate-new-buffer " *auto-space-first*"))
      (second (generate-new-buffer " *auto-space-second*"))
      enabled disabled result)
  (unwind-protect
      (progn
        (neomacs-auto-space-test--reset)
        (auto-space-mode 1)
        (auto-space-mode 1)
        (setq enabled
              (list
               :mode auto-space-mode
               :hook-count (neomacs-auto-space-test--hook-count)
               :registered
               (and (memq 'auto-space-mode global-minor-modes) t)))
        (with-current-buffer first
          (neomacs-auto-space-test--type "中A"))
        (with-current-buffer second
          (neomacs-auto-space-test--type "한B"))
        (auto-space-mode -1)
        (setq disabled
              (list
               :mode auto-space-mode
               :hook-count (neomacs-auto-space-test--hook-count)
               :registered
               (and (memq 'auto-space-mode global-minor-modes) t)))
        (with-current-buffer first
          (neomacs-auto-space-test--type "C中"))
        (setq result
              (list
               :enabled enabled
               :disabled disabled
               :first (with-current-buffer first
                        (buffer-substring-no-properties (point-min) (point-max)))
               :second (with-current-buffer second
                         (buffer-substring-no-properties (point-min) (point-max))))))
    (neomacs-auto-space-test--reset)
    (neomacs-auto-space-test--kill-buffers (list first second)))
  result)
"####;
    let expect = expect![[
        r####"OK (:enabled (:mode t :hook-count 1 :registered t) :disabled (:mode nil :hook-count 0 :registered nil) :first "中 AC中" :second "한 B")"####
    ]];
    ParityBatchCase::value(
        "global_mode_is_idempotent_across_buffers_and_disable_stops_future_spacing",
        elisp_form,
        expect,
    )
}

fn automatic_spacing_preserves_token_properties_without_leaking_them() -> ParityBatchCase {
    let elisp_form = r####"
(unwind-protect
    (with-temp-buffer
      (neomacs-auto-space-test--reset)
      (insert
       (propertize "中"
                   'face 'warning
                   'neomacs-token 'localized-heading
                   'rear-nonsticky '(face neomacs-token)))
      (goto-char (point-max))
      (auto-space-mode 1)
      (self-insert-command 1 ?A)
      (list
       :text (buffer-substring-no-properties (point-min) (point-max))
       :cells
       (let (cells)
         (dotimes (offset (buffer-size) (nreverse cells))
           (let ((position (+ (point-min) offset)))
             (push
              (list
               (char-after position)
               (get-text-property position 'face)
               (get-text-property position 'neomacs-token))
              cells))))
       :point (point)))
  (neomacs-auto-space-test--reset))
"####;
    let expect = expect![[
        r####"OK (:text "中 A" :cells ((20013 warning localized-heading) (32 nil nil) (65 nil nil)) :point 4)"####
    ]];
    ParityBatchCase::value(
        "automatic_spacing_preserves_token_properties_without_leaking_them",
        elisp_form,
        expect,
    )
}

pub(crate) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        typing_a_multilingual_release_note_spaces_only_real_script_boundaries(),
        one_typing_action_and_its_automatic_space_undo_as_one_edit(),
        enabling_the_mode_leaves_existing_prose_untouched_and_spaces_new_input(),
        region_add_formats_a_practical_bilingual_release_paragraph(),
        region_remove_compacts_only_cjk_ascii_whitespace_boundaries(),
        selected_region_uses_an_advancing_end_marker_and_preserves_point(),
        global_mode_is_idempotent_across_buffers_and_disable_stops_future_spacing(),
        automatic_spacing_preserves_token_properties_without_leaking_them(),
    ]
}
