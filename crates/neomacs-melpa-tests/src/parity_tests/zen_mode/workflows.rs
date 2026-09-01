use expect_test::expect;

use super::ParityBatchCase;

fn opens_comments_and_saves_a_real_zen_project_file() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "zen-mode-open"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (path (expand-file-name "src/release.zen" sandbox))
       buffer result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory (file-name-directory path) t)
        (with-temp-file path
          (insert
           "const std = @import(\"std\");\n"
           "fn announce(name: []const u8) void {\n"
           "    std.debug.warn(\"deploy {s}\\n\", .{name});\n"
           "}\n"))
        (setq buffer (find-file-noselect path))
        (with-current-buffer buffer
          (let ((original
                 (buffer-substring-no-properties (point-min) (point-max)))
                commented uncommented disk-after-save syntax-in-comment)
            (goto-char (point-min))
            (forward-line 1)
            (let ((start (point)))
              (forward-line 2)
              (comment-region start (point))
              (setq commented
                    (buffer-substring-no-properties (point-min) (point-max))
                    syntax-in-comment
                    (neomacs-melpa-zen-mode--syntax-state-at "fn announce" 3))
              (uncomment-region start (point)))
            (setq uncommented
                  (buffer-substring-no-properties (point-min) (point-max)))
            (save-buffer)
            (setq disk-after-save
                  (with-temp-buffer
                    (insert-file-contents path)
                    (buffer-substring-no-properties (point-min) (point-max))))
            (setq result
                  (list
                   :file buffer-file-name
                   :mode major-mode
                   :mode-name mode-name
                   :derived (derived-mode-p 'prog-mode)
                   :comments (list comment-start comment-end comment-padding)
                   :indent-function indent-line-function
                   :indent-offset zen-indent-offset
                   :tabs indent-tabs-mode
                   :syntax-function syntax-propertize-function
                   :imenu (copy-tree imenu-generic-expression)
                   :electric (copy-sequence electric-indent-chars)
                   :auto-mode (cdr (assoc "\\.zen\\'" auto-mode-alist))
                   :original original
                   :commented commented
                   :comment-syntax syntax-in-comment
                   :uncommented uncommented
                   :disk disk-after-save
                   :modified (buffer-modified-p))))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t)))
  result)
"####;
    let expect = expect![[
        r####"OK (:file "[ORACLE-SANDBOX]/zen-mode-open/src/release.zen" :mode zen-mode :mode-name "Zen" :derived prog-mode :comments ("// " "" " ") :indent-function zen-mode-indent-line :indent-offset 4 :tabs nil :syntax-function zen-syntax-propertize :imenu (("Enum" "\\<const\\>[[:space:]]+\\([[:word:]_][[:word:]_[:digit:]]*\\).*\\<enum\\>" 1) ("Struct" "\\<const\\>[[:space:]]+\\([[:word:]_][[:word:]_[:digit:]]*\\).*\\<struct\\>" 1) ("Union" "\\<const\\>[[:space:]]+\\([[:word:]_][[:word:]_[:digit:]]*\\).*\\<union\\>" 1) ("Interface" "\\<const\\>[[:space:]]+\\([[:word:]_][[:word:]_[:digit:]]*\\).*\\<interface\\>" 1) ("Fn" "\\<fn\\>[[:space:]]+\\([[:word:]_][[:word:]_[:digit:]]*\\)" 1)) :electric (59 44 41 93 125 10) :auto-mode zen-mode :original "const std = @import(\"std\");\nfn announce(name: []const u8) void {\n    std.debug.warn(\"deploy {s}\\n\", .{name});\n}\n" :commented "const std = @import(\"std\");\n// fn announce(name: []const u8) void {\n//     std.debug.warn(\"deploy {s}\\n\", .{name});\n}\n" :comment-syntax ("fn announce" 35 :depth 0 :string nil :comment t :start 29 :syntax-property nil) :uncommented "const std = @import(\"std\");\nfn announce(name: []const u8) void {\n    std.debug.warn(\"deploy {s}\\n\", .{name});\n}\n" :disk "const std = @import(\"std\");\nfn announce(name: []const u8) void {\n    std.debug.warn(\"deploy {s}\\n\", .{name});\n}\n" :modified nil)"####
    ]];
    ParityBatchCase::value(
        "opens_comments_and_saves_a_real_zen_project_file",
        elisp_form,
        expect,
    )
}

fn honors_safe_file_local_indentation_without_leaking_to_sibling_files() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "zen-mode-file-locals"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (two-space (expand-file-name "two-space.zen" sandbox))
       (sibling (expand-file-name "sibling.zen" sandbox))
       (negative (expand-file-name "negative.zen" sandbox))
       (rejected (expand-file-name "rejected.zen" sandbox))
       buffers result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory sandbox t)
        (with-temp-file two-space
          (insert
           "// -*- zen-indent-offset: 2 -*-\n"
           "fn main() void {\n"
           "return;\n"
           "}\n"))
        (with-temp-file sibling
          (insert "fn helper() void {\nreturn;\n}\n"))
        (with-temp-file negative
          (insert
           "// -*- zen-indent-offset: -2 -*-\n"
           "fn noop() void {}\n"))
        (with-temp-file rejected
          (insert
           "// -*- zen-indent-offset: \"wide\" -*-\n"
           "fn noop() void {}\n"))
        (let ((enable-local-variables :safe)
              (enable-local-eval nil))
          (setq buffers
                (mapcar
                 #'find-file-noselect
                 (list two-space sibling negative rejected))))
        (with-current-buffer (nth 0 buffers)
          (indent-region (point-min) (point-max)))
        (with-current-buffer (nth 1 buffers)
          (indent-region (point-min) (point-max)))
        (setq result
              (list
               :two-space
               (with-current-buffer (nth 0 buffers)
                 (list
                  :file buffer-file-name
                  :mode major-mode
                  :offset zen-indent-offset
                  :local-entry
                  (copy-tree
                   (assq 'zen-indent-offset file-local-variables-alist))
                  :text
                  (buffer-substring-no-properties (point-min) (point-max))
                  :indents (neomacs-melpa-zen-mode--line-indents)))
               :sibling
               (with-current-buffer (nth 1 buffers)
                 (list
                  :file buffer-file-name
                  :mode major-mode
                  :offset zen-indent-offset
                  :local-entry
                  (copy-tree
                   (assq 'zen-indent-offset file-local-variables-alist))
                  :text
                  (buffer-substring-no-properties (point-min) (point-max))
                  :indents (neomacs-melpa-zen-mode--line-indents)))
               :negative
               (with-current-buffer (nth 2 buffers)
                 (list
                  :offset zen-indent-offset
                  :local-entry
                  (copy-tree
                   (assq 'zen-indent-offset file-local-variables-alist))))
               :rejected
               (with-current-buffer (nth 3 buffers)
                 (list
                  :offset zen-indent-offset
                  :local-entry
                  (copy-tree
                   (assq 'zen-indent-offset file-local-variables-alist))))
               :safe-contract
               (list
                (safe-local-variable-p 'zen-indent-offset 2)
                (safe-local-variable-p 'zen-indent-offset -2)
                (safe-local-variable-p 'zen-indent-offset "wide")))))
    (dolist (buffer buffers)
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer)))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t)))
  result)
"####;
    let expect = expect![[
        r####"OK (:two-space (:file "[ORACLE-SANDBOX]/zen-mode-file-locals/two-space.zen" :mode zen-mode :offset 2 :local-entry (zen-indent-offset . 2) :text "// -*- zen-indent-offset: 2 -*-\nfn main() void {\n  return;\n}\n" :indents ((1 0 "// -*- zen-indent-offset: 2 -*-") (2 0 "fn main() void {") (3 2 "  return;") (4 0 "}"))) :sibling (:file "[ORACLE-SANDBOX]/zen-mode-file-locals/sibling.zen" :mode zen-mode :offset 4 :local-entry nil :text "fn helper() void {\n    return;\n}\n" :indents ((1 0 "fn helper() void {") (2 4 "    return;") (3 0 "}"))) :negative (:offset -2 :local-entry (zen-indent-offset . -2)) :rejected (:offset 4 :local-entry nil) :safe-contract (t t nil))"####
    ]];
    ParityBatchCase::value(
        "honors_safe_file_local_indentation_without_leaking_to_sibling_files",
        elisp_form,
        expect,
    )
}

fn fontifies_a_complete_release_module_with_exact_semantic_spans() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "const std = @import(\"std\");\n"
   "/// Release metadata for Ω builds.\n"
   "const Release = struct {\n"
   "    name: []const u8,\n"
   "    artifact: ?*const Artifact,\n"
   "};\n"
   "//// ordinary comment, not documentation\n"
   "fn deploy(release: *const Release, matrix: [3]u8) !void {\n"
   "    var message: []u8 = \"release \\\"α\\\"\";\n"
   "    const banner = \\\\shipping Ω\n"
   "                   \\\\\n"
   "                   \\\\complete\n"
   "    audit: {\n"
   "        const chosen = matrix[0..2];\n"
   "        for (chosen) |item, index| {\n"
   "            if (item == undefined) break :audit;\n"
   "        }\n"
   "    }\n"
   "    const range = 1...4;\n"
   "    const value = parse() catch |err| { return err; };\n"
   "    deprecated fn old() void {}\n"
   "}\n"
   "\n"
   "const Wire = struct {\n"
   "    c_message: [*c]u8,\n"
   "    sentinel: [*:0]u8,\n"
   "    optional: ?*[]u8,\n"
   "    vector: *@Vector,\n"
   "    aligned: *align(16) volatile allowzero Artifact,\n"
   "};\n"
   "const meta_type = @TypeOf(Wire);\n"
   "const declared_type: @Type = Wire;\n"
   "const frame_type = @Frame;\n"
   "const escaped = '\\'';\n"
   "const trailing = \"string\\\\\";\n")
  (zen-mode)
  (font-lock-ensure)
  (list
   :source (buffer-substring-no-properties (point-min) (point-max))
   :faces (neomacs-melpa-zen-mode--face-runs)
   :boundaries
   (mapcar
    #'neomacs-melpa-zen-mode--face-segments
    '("/// Release metadata"
      "//// ordinary comment"
      "name: []const u8"
      "artifact: ?*const Artifact"
      "@import"
      "audit: {"
      "break :audit"
      "|item, index|"
      "0..2"
      "1...4"
      "catch |err|"
      "deprecated fn old"
      "c_message: [*c]u8"
      "sentinel: [*:0]u8"
      "optional: ?*[]u8"
      "vector: *@Vector"
      "aligned: *align(16) volatile allowzero Artifact"
      "@TypeOf(Wire)"
      "@Type = Wire"
      "@Frame"
      "'\\''"
      "string"))
   :syntax
   (mapcar
    (lambda (probe)
      (neomacs-melpa-zen-mode--syntax-state-at (car probe) (cadr probe)))
    '(("Release metadata" 3)
      ("ordinary comment" 3)
      ("release \\\"α" 2)
      ("shipping Ω" 3)
      ("complete" 3)
      ("audit: {" 7)))
   :face-contracts
   (mapcar
    (lambda (face)
      (list
       face
       :inherit (face-attribute face :inherit nil nil)
       :underline (face-attribute face :underline nil nil)))
    '(zen-multiline-string-face
      zen-label-face
      zen-catch-vertical-bar-face
      zen-slice-range-face
      zen-int-range-face
      zen-error-face))))
"####;
    let expect = expect![[
        r####"OK (:source "const std = @import(\"std\");\n/// Release metadata for Ω builds.\nconst Release = struct {\n    name: []const u8,\n    artifact: ?*const Artifact,\n};\n//// ordinary comment, not documentation\nfn deploy(release: *const Release, matrix: [3]u8) !void {\n    var message: []u8 = \"release \\\"α\\\"\";\n    const banner = \\\\shipping Ω\n                   \\\\\n                   \\\\complete\n    audit: {\n        const chosen = matrix[0..2];\n        for (chosen) |item, index| {\n            if (item == undefined) break :audit;\n        }\n    }\n    const range = 1...4;\n    const value = parse() catch |err| { return err; };\n    deprecated fn old() void {}\n}\n\nconst Wire = struct {\n    c_message: [*c]u8,\n    sentinel: [*:0]u8,\n    optional: ?*[]u8,\n    vector: *@Vector,\n    aligned: *align(16) volatile allowzero Artifact,\n};\nconst meta_type = @TypeOf(Wire);\nconst declared_type: @Type = Wire;\nconst frame_type = @Frame;\nconst escaped = '\\'';\nconst trailing = \"string\\\\\";\n" :faces (("const" font-lock-keyword-face 1 6) ("std" font-lock-variable-name-face 7 10) ("@import" font-lock-builtin-face 13 20) ("\"std\"" font-lock-string-face 21 26) ("/// Release metadata for Ω builds.\n" font-lock-doc-face 29 64) ("const" font-lock-keyword-face 64 69) ("Release" font-lock-variable-name-face 70 77) ("struct" font-lock-keyword-face 80 86) ("name" font-lock-variable-name-face 93 97) ("const" zen-error-face 101 106) ("u8" font-lock-variable-name-face 107 109) ("artifact" font-lock-variable-name-face 115 123) ("const" zen-error-face 127 132) ("Artifact" font-lock-variable-name-face 133 141) ("//// " font-lock-comment-delimiter-face 146 151) ("ordinary comment, not documentation\n" font-lock-comment-face 151 187) ("fn" font-lock-keyword-face 187 189) ("deploy" font-lock-function-name-face 190 196) ("release" font-lock-variable-name-face 197 204) ("const" zen-error-face 207 212) ("Release" font-lock-variable-name-face 213 220) ("matrix" font-lock-variable-name-face 222 228) ("[3]u8" font-lock-type-face 230 235) ("!" font-lock-negation-char-face 237 238) ("void" font-lock-type-face 238 242) ("var" font-lock-keyword-face 249 252) ("message" font-lock-variable-name-face 253 260) ("[]u8" font-lock-type-face 262 266) ("\"release \\\"α\\\"\"" font-lock-string-face 269 284) ("const" font-lock-keyword-face 290 295) ("banner" font-lock-variable-name-face 296 302) ("\\\\shipping Ω\n" zen-multiline-string-face 305 318) ("\\\\\n" zen-multiline-string-face 337 340) ("\\\\complete\n" zen-multiline-string-face 359 370) ("audit:" zen-label-face 374 380) ("const" font-lock-keyword-face 391 396) ("chosen" font-lock-variable-name-face 397 403) (".." zen-slice-range-face 414 416) ("for" font-lock-keyword-face 428 431) ("|" zen-catch-vertical-bar-face 441 442) ("item, index" font-lock-variable-name-face 442 453) ("|" zen-catch-vertical-bar-face 453 454) ("if" font-lock-keyword-face 469 471) ("undefined" font-lock-constant-face 481 490) ("break" font-lock-keyword-face 492 497) (" :audit" zen-label-face 497 504) ("const" font-lock-keyword-face 526 531) ("range" font-lock-variable-name-face 532 537) ("..." zen-int-range-face 541 544) ("const" font-lock-keyword-face 551 556) ("value" font-lock-variable-name-face 557 562) ("catch" font-lock-keyword-face 573 578) ("|" zen-catch-vertical-bar-face 579 580) ("err" font-lock-variable-name-face 580 583) ("|" zen-catch-vertical-bar-face 583 584) ("return" font-lock-keyword-face 587 593) ("deprecated" zen-error-face 606 616) ("fn" font-lock-keyword-face 617 619) ("old" font-lock-function-name-face 620 623) ("void" font-lock-type-face 626 630) ("const" font-lock-keyword-face 637 642) ("Wire" font-lock-variable-name-face 643 647) ("struct" font-lock-keyword-face 650 656) ("c_message" font-lock-variable-name-face 663 672) ("[*c]u8" font-lock-type-face 674 680) ("sentinel" font-lock-variable-name-face 686 694) ("[*:0]u8" font-lock-type-face 696 703) ("optional" font-lock-variable-name-face 709 717) ("?*[]u8" font-lock-type-face 719 725) ("vector" font-lock-variable-name-face 731 737) ("@Vector" font-lock-type-face 740 747) ("aligned" font-lock-variable-name-face 753 760) ("align" font-lock-keyword-face 763 768) ("volatile" font-lock-keyword-face 773 781) ("allowzero" font-lock-keyword-face 782 791) ("const" font-lock-keyword-face 805 810) ("meta_type" font-lock-variable-name-face 811 820) ("@TypeOf" font-lock-type-face 823 830) ("const" font-lock-keyword-face 838 843) ("declared_type" font-lock-variable-name-face 844 857) ("@Type" font-lock-type-face 859 864) ("const" font-lock-keyword-face 873 878) ("frame_type" font-lock-variable-name-face 879 889) ("@Frame" font-lock-type-face 892 898) ("const" font-lock-keyword-face 900 905) ("escaped" font-lock-variable-name-face 906 913) ("'\\''" font-lock-string-face 916 920) ("const" font-lock-keyword-face 922 927) ("trailing" font-lock-variable-name-face 928 936) ("\"string\\\\\"" font-lock-string-face 939 949)) :boundaries (("/// Release metadata" 29 49 (("/// Release metadata" font-lock-doc-face 0 20))) ("//// ordinary comment" 146 167 (("//// " font-lock-comment-delimiter-face 0 5) ("ordinary comment" font-lock-comment-face 5 21))) ("name: []const u8" 93 109 (("name" font-lock-variable-name-face 0 4) (": []" nil 4 8) ("const" zen-error-face 8 13) (" " nil 13 14) ("u8" font-lock-variable-name-face 14 16))) ("artifact: ?*const Artifact" 115 141 (("artifact" font-lock-variable-name-face 0 8) (": ?*" nil 8 12) ("const" zen-error-face 12 17) (" " nil 17 18) ("Artifact" font-lock-variable-name-face 18 26))) ("@import" 13 20 (("@import" font-lock-builtin-face 0 7))) ("audit: {" 374 382 (("audit:" zen-label-face 0 6) (" {" nil 6 8))) ("break :audit" 492 504 (("break" font-lock-keyword-face 0 5) (" :audit" zen-label-face 5 12))) ("|item, index|" 441 454 (("|" zen-catch-vertical-bar-face 0 1) ("item, index" font-lock-variable-name-face 1 12) ("|" zen-catch-vertical-bar-face 12 13))) ("0..2" 413 417 (("0" nil 0 1) (".." zen-slice-range-face 1 3) ("2" nil 3 4))) ("1...4" 540 545 (("1" nil 0 1) ("..." zen-int-range-face 1 4) ("4" nil 4 5))) ("catch |err|" 573 584 (("catch" font-lock-keyword-face 0 5) (" " nil 5 6) ("|" zen-catch-vertical-bar-face 6 7) ("err" font-lock-variable-name-face 7 10) ("|" zen-catch-vertical-bar-face 10 11))) ("deprecated fn old" 606 623 (("deprecated" zen-error-face 0 10) (" " nil 10 11) ("fn" font-lock-keyword-face 11 13) (" " nil 13 14) ("old" font-lock-function-name-face 14 17))) ("c_message: [*c]u8" 663 680 (("c_message" font-lock-variable-name-face 0 9) (": " nil 9 11) ("[*c]u8" font-lock-type-face 11 17))) ("sentinel: [*:0]u8" 686 703 (("sentinel" font-lock-variable-name-face 0 8) (": " nil 8 10) ("[*:0]u8" font-lock-type-face 10 17))) ("optional: ?*[]u8" 709 725 (("optional" font-lock-variable-name-face 0 8) (": " nil 8 10) ("?*[]u8" font-lock-type-face 10 16))) ("vector: *@Vector" 731 747 (("vector" font-lock-variable-name-face 0 6) (": *" nil 6 9) ("@Vector" font-lock-type-face 9 16))) ("aligned: *align(16) volatile allowzero Artifact" 753 800 (("aligned" font-lock-variable-name-face 0 7) (": *" nil 7 10) ("align" font-lock-keyword-face 10 15) ("(16) " nil 15 20) ("volatile" font-lock-keyword-face 20 28) (" " nil 28 29) ("allowzero" font-lock-keyword-face 29 38) (" Artifact" nil 38 47))) ("@TypeOf(Wire)" 823 836 (("@TypeOf" font-lock-type-face 0 7) ("(Wire)" nil 7 13))) ("@Type = Wire" 859 871 (("@Type" font-lock-type-face 0 5) (" = Wire" nil 5 12))) ("@Frame" 892 898 (("@Frame" font-lock-type-face 0 6))) ("'\\''" 916 920 (("'\\''" font-lock-string-face 0 4))) ("string" 940 946 (("string" font-lock-string-face 0 6)))) :syntax (("Release metadata" 36 :depth 0 :string nil :comment t :start 29 :syntax-property nil) ("ordinary comment" 154 :depth 0 :string nil :comment t :start 146 :syntax-property nil) ("release \\\"α" 272 :depth 1 :string 34 :comment nil :start 269 :syntax-property nil) ("shipping Ω" 310 :depth 1 :string t :comment nil :start 305 :syntax-property 1) ("complete" 364 :depth 1 :string t :comment nil :start 359 :syntax-property 1) ("audit: {" 381 :depth 1 :string nil :comment nil :start nil :syntax-property nil)) :face-contracts ((zen-multiline-string-face :inherit font-lock-string-face :underline unspecified) (zen-label-face :inherit font-lock-builtin-face :underline unspecified) (zen-catch-vertical-bar-face :inherit font-lock-negation-char-face :underline unspecified) (zen-slice-range-face :inherit font-lock-keyword-face :underline unspecified) (zen-int-range-face :inherit font-lock-negation-char-face :underline unspecified) (zen-error-face :inherit font-lock-warning-face :underline t)))"####
    ]];
    ParityBatchCase::value(
        "fontifies_a_complete_release_module_with_exact_semantic_spans",
        elisp_form,
        expect,
    )
}

fn reindents_a_customized_project_module_idempotently() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "const Release = struct {\n"
   "name: []const u8,\n"
   "targets: [3]u8,\n"
   "};\n"
   "\n"
   "fn publish(\n"
   "release: Release,\n"
   "channel: []const u8,\n"
   ") void {\n"
   "if (release.targets.len > 0) {\n"
   "send(\n"
   "release.name,\n"
   "channel,\n"
   ");\n"
   "} else { // keep operator context\n"
   "warn(\"empty\");\n"
   "}\n"
   "}\n")
  (zen-mode)
  (setq-local zen-indent-offset 2)
  (goto-char (point-min))
  (forward-line 11)
  (search-forward "release")
  (let ((token-offset (- (point) (line-beginning-position))))
    (indent-region (point-min) (point-max))
    (let ((after-first
           (buffer-substring-no-properties (point-min) (point-max)))
          (point-after-first (point))
          (indents (neomacs-melpa-zen-mode--line-indents)))
      (indent-region (point-min) (point-max))
      (list
       :text (buffer-substring-no-properties (point-min) (point-max))
       :indents indents
       :idempotent
       (equal after-first
              (buffer-substring-no-properties (point-min) (point-max)))
       :offset zen-indent-offset
       :tabs indent-tabs-mode
       :contains-tab
       (string-match-p "\t" (buffer-string))
       :point-after-first point-after-first
       :point-after-second (point)
       :token-offset-before token-offset
       :token-at-point
       (buffer-substring-no-properties
        (line-beginning-position) (line-end-position))))))
"####;
    let expect = expect![[
        r####"OK (:text "const Release = struct {\n  name: []const u8,\n  targets: [3]u8,\n};\n\nfn publish(\n  release: Release,\n  channel: []const u8,\n) void {\n  if (release.targets.len > 0) {\n    send(\n      release.name,\n      channel,\n    );\n  } else { // keep operator context\n    warn(\"empty\");\n  }\n}\n" :indents ((1 0 "const Release = struct {") (2 2 "  name: []const u8,") (3 2 "  targets: [3]u8,") (4 0 "};") (5 0 "") (6 0 "fn publish(") (7 2 "  release: Release,") (8 2 "  channel: []const u8,") (9 0 ") void {") (10 2 "  if (release.targets.len > 0) {") (11 4 "    send(") (12 6 "      release.name,") (13 6 "      channel,") (14 4 "    );") (15 2 "  } else { // keep operator context") (16 4 "    warn(\"empty\");") (17 2 "  }") (18 0 "}")) :idempotent t :offset 2 :tabs nil :contains-tab nil :point-after-first 188 :point-after-second 188 :token-offset-before 7 :token-at-point "      release.name,")"####
    ]];
    ParityBatchCase::value(
        "reindents_a_customized_project_module_idempotently",
        elisp_form,
        expect,
    )
}

fn repairs_multiline_string_syntax_during_live_editing() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "const banner = \\\\shipping Ω\n"
   "               \\\\second stage\n"
   "               \\\\\n"
   "               \\\\complete\n"
   "const after = true;\n")
  (zen-mode)
  (font-lock-ensure)
  (let ((original-text
         (buffer-substring-no-properties (point-min) (point-max)))
        (original-faces (neomacs-melpa-zen-mode--face-runs))
        (original-syntax
         (list
          (neomacs-melpa-zen-mode--syntax-state-at "second stage" 3)
          (neomacs-melpa-zen-mode--syntax-state-at "complete" 3)
          (neomacs-melpa-zen-mode--syntax-state-at "after" 2)))
        edited-text edited-faces edited-syntax)
    (goto-char (point-min))
    (search-forward "\\\\second stage")
    (goto-char (match-beginning 0))
    (delete-char 1)
    (font-lock-flush)
    (font-lock-ensure)
    (setq edited-text
          (buffer-substring-no-properties (point-min) (point-max))
          edited-faces (neomacs-melpa-zen-mode--face-runs)
          edited-syntax
          (list
           (neomacs-melpa-zen-mode--syntax-state-at "second stage" 3)
           (neomacs-melpa-zen-mode--syntax-state-at "complete" 3)
           (neomacs-melpa-zen-mode--syntax-state-at "after" 2)))
    (goto-char (point-min))
    (search-forward "\\second stage")
    (goto-char (match-beginning 0))
    (insert "\\")
    (font-lock-flush)
    (font-lock-ensure)
    (list
     :original
     (list :text original-text :faces original-faces :syntax original-syntax)
     :edited
     (list :text edited-text :faces edited-faces :syntax edited-syntax)
     :repaired
     (list
      :text (buffer-substring-no-properties (point-min) (point-max))
      :faces (neomacs-melpa-zen-mode--face-runs)
      :syntax
      (list
       (neomacs-melpa-zen-mode--syntax-state-at "second stage" 3)
       (neomacs-melpa-zen-mode--syntax-state-at "complete" 3)
       (neomacs-melpa-zen-mode--syntax-state-at "after" 2)))
     :restored
     (equal
      original-text
      (buffer-substring-no-properties (point-min) (point-max))))))
"####;
    let expect = expect![[
        r####"OK (:original (:text "const banner = \\\\shipping Ω\n               \\\\second stage\n               \\\\\n               \\\\complete\nconst after = true;\n" :faces (("const" font-lock-keyword-face 1 6) ("banner" font-lock-variable-name-face 7 13) ("\\\\shipping Ω\n" zen-multiline-string-face 16 29) ("\\\\second stage\n" zen-multiline-string-face 44 59) ("\\\\\n" zen-multiline-string-face 74 77) ("\\\\complete\n" zen-multiline-string-face 92 103) ("const" font-lock-keyword-face 103 108) ("after" font-lock-variable-name-face 109 114) ("true" font-lock-constant-face 117 121)) :syntax (("second stage" 49 :depth 0 :string t :comment nil :start 44 :syntax-property 1) ("complete" 97 :depth 0 :string t :comment nil :start 92 :syntax-property 1) ("after" 111 :depth 0 :string nil :comment nil :start nil :syntax-property nil))) :edited (:text "const banner = \\\\shipping Ω\n               \\second stage\n               \\\\\n               \\\\complete\nconst after = true;\n" :faces (("const" font-lock-keyword-face 1 6) ("banner" font-lock-variable-name-face 7 13) ("\\\\shipping Ω\n" zen-multiline-string-face 16 29) ("\\\\\n" zen-multiline-string-face 73 76) ("\\\\complete\n" zen-multiline-string-face 91 102) ("const" font-lock-keyword-face 102 107) ("after" font-lock-variable-name-face 108 113) ("true" font-lock-constant-face 116 120)) :syntax (("second stage" 48 :depth 0 :string nil :comment nil :start nil :syntax-property nil) ("complete" 96 :depth 0 :string t :comment nil :start 91 :syntax-property 1) ("after" 110 :depth 0 :string nil :comment nil :start nil :syntax-property nil))) :repaired (:text "const banner = \\\\shipping Ω\n               \\\\second stage\n               \\\\\n               \\\\complete\nconst after = true;\n" :faces (("const" font-lock-keyword-face 1 6) ("banner" font-lock-variable-name-face 7 13) ("\\\\shipping Ω\n" zen-multiline-string-face 16 29) ("\\\\second stage\n" zen-multiline-string-face 44 59) ("\\\\\n" zen-multiline-string-face 74 77) ("\\\\complete\n" zen-multiline-string-face 92 103) ("const" font-lock-keyword-face 103 108) ("after" font-lock-variable-name-face 109 114) ("true" font-lock-constant-face 117 121)) :syntax (("second stage" 49 :depth 0 :string t :comment nil :start 44 :syntax-property 1) ("complete" 97 :depth 0 :string t :comment nil :start 92 :syntax-property 1) ("after" 111 :depth 0 :string nil :comment nil :start nil :syntax-property nil))) :restored t)"####
    ]];
    ParityBatchCase::value(
        "repairs_multiline_string_syntax_during_live_editing",
        elisp_form,
        expect,
    )
}

fn indexes_and_navigates_every_supported_definition_kind() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "const Stage = enum { queued, shipping, complete };\n"
   "const Release = struct { name: []const u8 };\n"
   "const Outcome = union { success: u8, failure: anyerror };\n"
   "const Reporter = interface { fn report(self: *Reporter) void; };\n"
   "\n"
   "// fn commented_out() void {}\n"
   "const note = \"fn string_fake() void {}\";\n"
   "const script = \\\\fn multiline_fake() void {}\n"
   "FN uppercase() void {}\n"
   "CONST Fake = struct {};\n"
   "\n"
   "fn helper(release: Release) void {\n"
   "    fn nested() void {}\n"
   "    nested();\n"
   "}\n"
   "\n"
   "fn main() void {\n"
   "    helper(undefined);\n"
   "}\n")
  (zen-mode)
  (let* ((normalized (neomacs-melpa-zen-mode--imenu-index))
         (raw (imenu-default-create-index-function))
         (functions (cdr (assoc "Fn" raw)))
         (nested (assoc "nested" functions))
         (main (assoc "main" functions))
         jumps)
    (goto-char (point-min))
    (imenu nested)
    (push
     (list
      :name "nested"
      :point (point)
      :line (line-number-at-pos)
      :column (current-column)
      :text (buffer-substring-no-properties
             (line-beginning-position) (line-end-position)))
     jumps)
    (imenu main)
    (push
     (list
      :name "main"
      :point (point)
      :line (line-number-at-pos)
      :column (current-column)
      :text (buffer-substring-no-properties
             (line-beginning-position) (line-end-position)))
     jumps)
    (list
     :index normalized
     :jumps (nreverse jumps)
     :mark (mark t)
     :mode major-mode
     :expression (copy-tree imenu-generic-expression))))
"####;
    let expect = expect![[
        r####"OK (:index (("Fn" ("report" 4 0 "const Reporter = interface { fn report(self: *Reporter) void; };") ("helper" 12 0 "fn helper(release: Release) void {") ("nested" 13 0 "    fn nested() void {}") ("main" 17 0 "fn main() void {")) ("Interface" ("Reporter" 4 0 "const Reporter = interface { fn report(self: *Reporter) void; };")) ("Union" ("Outcome" 3 0 "const Outcome = union { success: u8, failure: anyerror };")) ("Struct" ("Release" 2 0 "const Release = struct { name: []const u8 };")) ("Enum" ("Stage" 1 0 "const Stage = enum { queued, shipping, complete };"))) :jumps ((:name "nested" :point 420 :line 13 :column 0 :text "    fn nested() void {}") (:name "main" :point 461 :line 17 :column 0 :text "fn main() void {")) :mark 420 :mode zen-mode :expression (("Enum" "\\<const\\>[[:space:]]+\\([[:word:]_][[:word:]_[:digit:]]*\\).*\\<enum\\>" 1) ("Struct" "\\<const\\>[[:space:]]+\\([[:word:]_][[:word:]_[:digit:]]*\\).*\\<struct\\>" 1) ("Union" "\\<const\\>[[:space:]]+\\([[:word:]_][[:word:]_[:digit:]]*\\).*\\<union\\>" 1) ("Interface" "\\<const\\>[[:space:]]+\\([[:word:]_][[:word:]_[:digit:]]*\\).*\\<interface\\>" 1) ("Fn" "\\<fn\\>[[:space:]]+\\([[:word:]_][[:word:]_[:digit:]]*\\)" 1)))"####
    ]];
    ParityBatchCase::value(
        "indexes_and_navigates_every_supported_definition_kind",
        elisp_form,
        expect,
    )
}

fn electrically_reindents_real_typed_delimiters_through_normal_command_hooks() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (cl-labels
      ((exercise (label source typed)
         (erase-buffer)
         (insert source)
         (zen-mode)
         (electric-indent-local-mode 1)
         (goto-char (point-max))
         (let ((last-command-event typed)
               (this-command 'self-insert-command)
               (real-this-command 'self-insert-command))
           (call-interactively #'self-insert-command))
         (list
          label
          :typed typed
          :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :column (current-column)
          :indents (neomacs-melpa-zen-mode--line-indents)
          :enabled electric-indent-mode
          :registered (and (memq typed electric-indent-chars) t))))
    (list
     :cases
     (list
      (exercise 'semicolon "fn main() void {\n        return" ?\;)
      (exercise 'comma "fn main() void {\n        call(first" ?,)
      (exercise 'close-paren "fn main() void {\n        call(value" ?\))
      (exercise 'close-bracket
                "fn main() void {\n        const value = items[0" ?\])
      (exercise 'close-brace "fn main() void {\n        return;\n        " ?})
      (exercise 'newline "fn main() void {" ?\n)
      (exercise 'inside-string
                "fn main() void {\n        const text = \"value" ?\;)
      (exercise 'inside-comment "fn main() void {\n        // note" ?\;))
     :hook
     (and
      (memq #'electric-indent-post-self-insert-function
            post-self-insert-hook)
      t))))
"####;
    let expect = expect![[
        r####"OK (:cases ((semicolon :typed 59 :text "fn main() void {\n    return;" :point 29 :column 11 :indents ((1 0 "fn main() void {") (2 4 "    return;")) :enabled t :registered t) (comma :typed 44 :text "fn main() void {\n    call(first," :point 33 :column 15 :indents ((1 0 "fn main() void {") (2 4 "    call(first,")) :enabled t :registered t) (close-paren :typed 41 :text "fn main() void {\n    call(value)" :point 33 :column 15 :indents ((1 0 "fn main() void {") (2 4 "    call(value)")) :enabled t :registered t) (close-bracket :typed 93 :text "fn main() void {\n    const value = items[0]" :point 44 :column 26 :indents ((1 0 "fn main() void {") (2 4 "    const value = items[0]")) :enabled t :registered t) (close-brace :typed 125 :text "fn main() void {\n        return;\n}" :point 35 :column 1 :indents ((1 0 "fn main() void {") (2 8 "        return;") (3 0 "}")) :enabled t :registered t) (newline :typed 10 :text "fn main() void {\n    " :point 22 :column 4 :indents ((1 0 "fn main() void {") (2 4 "    ")) :enabled t :registered t) (inside-string :typed 59 :text "fn main() void {\n    const text = \"value;" :point 42 :column 24 :indents ((1 0 "fn main() void {") (2 4 "    const text = \"value;")) :enabled t :registered t) (inside-comment :typed 59 :text "fn main() void {\n    // note;" :point 30 :column 12 :indents ((1 0 "fn main() void {") (2 4 "    // note;")) :enabled t :registered t)) :hook t)"####
    ]];
    ParityBatchCase::value(
        "electrically_reindents_real_typed_delimiters_through_normal_command_hooks",
        elisp_form,
        expect,
    )
}

fn recovers_an_incomplete_module_without_corrupting_the_buffer() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "fn publish(release: Release,\n"
   "channel: []const u8 {\n"
   "if (release.ready) {\n"
   "const message = \\\\shipping Ω\n"
   "// operator is still typing\n")
  (zen-mode)
  (indent-region (point-min) (point-max))
  (font-lock-ensure)
  (let* ((broken-text
          (buffer-substring-no-properties (point-min) (point-max)))
         (broken-indents (neomacs-melpa-zen-mode--line-indents))
         (broken-faces (neomacs-melpa-zen-mode--face-runs))
         (state (syntax-ppss (point-max)))
         (broken-syntax
          (list (nth 0 state) (nth 3 state) (nth 4 state) (nth 8 state)))
         (broken-check
          (condition-case caught
              (list 'ok (check-parens))
            (error (list 'error (car caught) (cdr caught))))))
    (goto-char (point-max))
    (insert "    return;\n}\n}\n")
    (goto-char (point-min))
    (search-forward "u8 {")
    (backward-char 2)
    (insert ")")
    (indent-region (point-min) (point-max))
    (font-lock-flush)
    (font-lock-ensure)
    (list
     :broken
     (list
      :text broken-text
      :indents broken-indents
      :faces broken-faces
      :syntax broken-syntax
      :check broken-check)
     :repaired
     (list
      :text (buffer-substring-no-properties (point-min) (point-max))
      :indents (neomacs-melpa-zen-mode--line-indents)
      :faces (neomacs-melpa-zen-mode--face-runs)
      :check
      (condition-case caught
          (progn (check-parens) '(ok))
        (error (list 'error (car caught) (cdr caught))))))))
"####;
    let expect = expect![[
        r####"OK (:broken (:text "fn publish(release: Release,\n           channel: []const u8 {\n               if (release.ready) {\n                   const message = \\\\shipping Ω\n                       // operator is still typing\n" :indents ((1 0 "fn publish(release: Release,") (2 11 "           channel: []const u8 {") (3 15 "               if (release.ready) {") (4 19 "                   const message = \\\\shipping Ω") (5 23 "                       // operator is still typing")) :faces (("fn" font-lock-keyword-face 1 3) ("publish" font-lock-function-name-face 4 11) ("release" font-lock-variable-name-face 12 19) ("Release" font-lock-type-face 21 28) ("channel" font-lock-variable-name-face 41 48) ("const" zen-error-face 52 57) ("u8" font-lock-variable-name-face 58 60) ("if" font-lock-keyword-face 78 80) ("." font-lock-negation-char-face 89 90) ("const" font-lock-keyword-face 118 123) ("message" font-lock-variable-name-face 124 131) ("\\\\shipping Ω\n" zen-multiline-string-face 134 147) ("// " font-lock-comment-delimiter-face 170 173) ("operator is still typing\n" font-lock-comment-face 173 198)) :syntax (3 nil nil nil) :check (error user-error ("Unmatched bracket or quote"))) :repaired (:text "fn publish(release: Release,\n           channel: []const u8) {\n    if (release.ready) {\n        const message = \\\\shipping Ω\n            // operator is still typing\n            return;\n    }\n}\n" :indents ((1 0 "fn publish(release: Release,") (2 11 "           channel: []const u8) {") (3 4 "    if (release.ready) {") (4 8 "        const message = \\\\shipping Ω") (5 12 "            // operator is still typing") (6 12 "            return;") (7 4 "    }") (8 0 "}")) :faces (("fn" font-lock-keyword-face 1 3) ("publish" font-lock-function-name-face 4 11) ("release" font-lock-variable-name-face 12 19) ("Release" font-lock-type-face 21 28) ("channel" font-lock-variable-name-face 41 48) ("const" zen-error-face 52 57) ("u8" font-lock-variable-name-face 58 60) ("if" font-lock-keyword-face 68 70) ("." font-lock-negation-char-face 79 80) ("const" font-lock-keyword-face 97 102) ("message" font-lock-variable-name-face 103 110) ("\\\\shipping Ω\n" zen-multiline-string-face 113 126) ("// " font-lock-comment-delimiter-face 138 141) ("operator is still typing\n" font-lock-comment-face 141 166) ("return" font-lock-keyword-face 178 184)) :check (ok)))"####
    ]];
    ParityBatchCase::value(
        "recovers_an_incomplete_module_without_corrupting_the_buffer",
        elisp_form,
        expect,
    )
}

pub(crate) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opens_comments_and_saves_a_real_zen_project_file(),
        honors_safe_file_local_indentation_without_leaking_to_sibling_files(),
        fontifies_a_complete_release_module_with_exact_semantic_spans(),
        reindents_a_customized_project_module_idempotently(),
        repairs_multiline_string_syntax_during_live_editing(),
        indexes_and_navigates_every_supported_definition_kind(),
        electrically_reindents_real_typed_delimiters_through_normal_command_hooks(),
        recovers_an_incomplete_module_without_corrupting_the_buffer(),
    ]
}
