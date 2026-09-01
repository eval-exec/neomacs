use expect_test::expect;

use super::ParityBatchCase;

/// Opening a real Ada spec: `auto-mode-alist` routes the file to the mode, the
/// mode refuses to start without a usable grammar, and what comes up is a live
/// Ada parse tree rather than a fallback.  The parser's root node and its child
/// count are pinned so a mode that silently degraded could not pass.
fn opening_an_ada_spec_activates_the_tree_sitter_mode_with_a_live_parser() -> ParityBatchCase {
    ParityBatchCase::value(
        "opening_an_ada_spec_activates_the_tree_sitter_mode_with_a_live_parser",
        r##"(ada-test-in-file
 "src/shop-inventory.ads" ada-test-spec
 (list :mode major-mode
       :routing (assoc-default "shop-inventory.ads" auto-mode-alist #'string-match)
       :ready (treesit-ready-p 'ada)
       :parsers (mapcar #'treesit-parser-language (treesit-parser-list))
       :root (treesit-node-type (treesit-buffer-root-node 'ada))
       :children (treesit-node-child-count (treesit-buffer-root-node 'ada))
       :comment (list comment-start comment-end)
       :indent (list indent-line-function indent-region-function)
       :imenu imenu-create-index-function
       :defun-name-fn treesit-defun-name-function))"##,
        expect![[
            r#"OK (:mode ada-ts-mode :routing ada-ts-mode :ready t :parsers (ada) :root "compilation" :children 2 :comment ("--" "") :indent (ada-ts-mode--indent-line ada-ts-mode--indent-region) :imenu ada-ts-imenu :defun-name-fn ada-ts-mode--defun-name)"#
        ]],
    )
}

fn font_lock_assigns_faces_by_feature_level_across_the_spec() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_assigns_faces_by_feature_level_across_the_spec",
        r##"(ada-test-in-file
 "src/shop-inventory.ads" ada-test-spec
 (let ((level4 (progn (setq-local treesit-font-lock-level 4)
                      (treesit-font-lock-recompute-features)
                      (font-lock-ensure)
                      (ada-test-faces-at
                       '("--  Inventory" "package" "Max_Items" "100" "Item_Id" "Name_Of")))))
   (list :level4 level4
         :level1 (progn (setq-local treesit-font-lock-level 1)
                        (treesit-font-lock-recompute-features)
                        (font-lock-flush)
                        (font-lock-ensure)
                        (ada-test-faces-at '("--  Inventory" "package" "100")))
         :features treesit-font-lock-feature-list)))"##,
        expect![[
            r#"OK (:level4 (("--  Inventory" 1 font-lock-comment-face) ("package" 45 font-lock-keyword-face) ("Max_Items" 75 (font-lock-constant-face font-lock-variable-name-face)) ("100" 107 font-lock-number-face) ("Item_Id" 121 font-lock-type-face) ("Name_Of" 159 (font-lock-function-name-face))) :level1 (("--  Inventory" 1 font-lock-comment-face) ("package" 45 nil) ("100" 107 nil)) :features ((comment definition) (keyword preprocessor string type) (attribute assignment constant control function number operator) (bracket delimiter error label)))"#
        ]],
    )
}

fn indenting_a_flattened_package_body_reproduces_the_original_layout() -> ParityBatchCase {
    ParityBatchCase::value(
        "indenting_a_flattened_package_body_reproduces_the_original_layout",
        r##"(ada-test-in-file
 "src/shop-inventory.adb" ada-test-body
 (setq-local indent-tabs-mode nil)
 (let ((flattened (replace-regexp-in-string "^[ \t]+" "" ada-test-body)))
   (erase-buffer)
   (insert flattened)
   (indent-region (point-min) (point-max))
   (list :flattened flattened
         :indented (buffer-substring-no-properties (point-min) (point-max))
         :matches-original (string= (buffer-substring-no-properties (point-min) (point-max))
                                    ada-test-body)
         :offset ada-ts-mode-indent-offset)))"##,
        expect![[
            r#"OK (:flattened "package body Shop.Inventory is\n\nfunction Name_Of (Id : Item_Id) return String is\nbegin\nreturn \"Artikel\";\nend Name_Of;\n\nprocedure Restock (Id : Item_Id; Count : Natural) is\nRemaining : Natural := Count;\nbegin\nwhile Remaining > 0 loop\nRemaining := Remaining - 1;\nend loop;\nend Restock;\n\nend Shop.Inventory;\n" :indented "package body Shop.Inventory is\n\n   function Name_Of (Id : Item_Id) return String is\n   begin\n      return \"Artikel\";\n   end Name_Of;\n\n   procedure Restock (Id : Item_Id; Count : Natural) is\n      Remaining : Natural := Count;\n   begin\n      while Remaining > 0 loop\n         Remaining := Remaining - 1;\n      end loop;\n   end Restock;\n\nend Shop.Inventory;\n" :matches-original t :offset 3)"#
        ]],
    )
}

fn imenu_and_defun_navigation_follow_the_parse_tree() -> ParityBatchCase {
    ParityBatchCase::value(
        "imenu_and_defun_navigation_follow_the_parse_tree",
        r##"(ada-test-in-file
 "src/shop-inventory.adb" ada-test-body
 (let ((index (ada-test-flatten-index (funcall imenu-create-index-function))))
   (goto-char (point-min))
   (search-forward "return \"Artikel\"")
   (let ((inside (list (point) (substring-no-properties
                                (treesit-defun-name (treesit-defun-at-point))))))
     (beginning-of-defun)
     (let ((start (list (point) (buffer-substring-no-properties
                                 (point) (line-end-position)))))
       (end-of-defun)
       (let ((finish (list (point) (buffer-substring-no-properties
                                    (line-beginning-position 0) (point)))))
         (beginning-of-defun)
         (beginning-of-defun)
         (list :index index :inside inside :start start :finish finish
               :outer (list (point) (buffer-substring-no-properties
                                     (point) (line-end-position)))))))))"##,
        expect![[
            r#"OK (:index (("Package" ("Shop.Inventory" . 1)) ("Subprogram" ("Shop.Inventory" ("Name_Of" . 36) ("Restock" . 138)))) :inside (116 "Name_Of") :start (33 "   function Name_Of (Id : Item_Id) return String is") :finish (134 "   end Name_Of;\n") :outer (1 "package body Shop.Inventory is"))"#
        ]],
    )
}

fn the_comment_box_command_frames_the_subprogram_at_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_comment_box_command_frames_the_subprogram_at_point",
        r##"(ada-test-in-file
 "src/shop-inventory.adb" ada-test-body
 (setq-local indent-tabs-mode nil)
 (goto-char (point-min))
 (search-forward "procedure Restock")
 (call-interactively 'ada-ts-mode-defun-comment-box)
 (list :boxed (buffer-substring-no-properties (point-min) (point-max))
       :point (point)))"##,
        expect![[
            r#"OK (:boxed "package body Shop.Inventory is\n\n   function Name_Of (Id : Item_Id) return String is\n   begin\n      return \"Artikel\";\n   end Name_Of;\n\n   -------------\n   -- Restock --\n   -------------\n\n   procedure Restock (Id : Item_Id; Count : Natural) is\n      Remaining : Natural := Count;\n   begin\n      while Remaining > 0 loop\n         Remaining := Remaining - 1;\n      end loop;\n   end Restock;\n\nend Shop.Inventory;\n" :point 207)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_an_ada_spec_activates_the_tree_sitter_mode_with_a_live_parser(),
        font_lock_assigns_faces_by_feature_level_across_the_spec(),
        indenting_a_flattened_package_body_reproduces_the_original_layout(),
        imenu_and_defun_navigation_follow_the_parse_tree(),
        the_comment_box_command_frames_the_subprogram_at_point(),
    ]
}
