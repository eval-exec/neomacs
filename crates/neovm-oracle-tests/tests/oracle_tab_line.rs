#[path = "../src/common.rs"]
mod common;

#[test]
fn global_tab_line_activates_existing_and_new_major_mode_buffers() {
    let form = r#"
(progn
  (require 'tab-line)
  (let ((existing (get-buffer-create "issue583-existing"))
        (excluded (get-buffer-create " issue583-internal"))
        (created nil))
    (unwind-protect
        (progn
          (global-tab-line-mode 1)
          (let ((before (mapcar
                         (lambda (buffer)
                           (with-current-buffer buffer
                             (list tab-line-mode tab-line-format)))
                         (list existing excluded))))
            (setq created (get-buffer-create "issue583-new"))
            (with-current-buffer created
              (text-mode))
            (let ((after (with-current-buffer created
                           (list tab-line-mode tab-line-format))))
              (global-tab-line-mode -1)
              (list before after
                    (mapcar (lambda (buffer)
                              (with-current-buffer buffer
                                (list tab-line-mode tab-line-format)))
                            (list existing excluded created))))))
      (global-tab-line-mode -1)
      (mapc #'kill-buffer (delq nil (list existing excluded created))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((t (:eval (tab-line-format))) (nil nil)) (t (:eval (tab-line-format))) ((nil nil) (nil nil) (nil nil)))""#
    ]];
    common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn flat_button_box_preserves_style_and_explicit_color_in_either_order() {
    let form = r##"
(progn
  (make-face 'issue583-box)
  (mapcar
   (lambda (box)
     (set-face-attribute 'issue583-box nil :box box)
     (face-attribute 'issue583-box :box))
   '(t 2 (:line-width 2) (:style nil)
       (:style flat-button)
       (:style flat-button :color "#2468ac")
       (:color "#2468ac" :style flat-button)
       (:style released-button) (:style pressed-button))))
"##;
    let expect = expect_test::expect![[
        r##""OK (1 2 (:line-width 2) (:style nil) (:style flat-button) (:style flat-button :color \"#2468ac\") (:color \"#2468ac\" :style flat-button) (:style released-button) (:style pressed-button))""##
    ]];
    common::assert_oracle_parity_expect(form, expect);
}
