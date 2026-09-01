use expect_test::expect;

use super::ParityBatchCase;

fn readme_eshell_prompt_renders_deep_hidden_directory_with_exact_faces() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-shrink-path-test-with-home
 "shrink-path-prompt"
 (lambda (_root home)
   (let ((working (file-name-as-directory
                   (expand-file-name
                    "Workspaces/Neomacs/.build/cache" home))))
     (make-directory working t)
     (let* ((default-directory working)
            (parts (shrink-path-prompt))
            (prompt
             (concat
              (propertize (car parts) 'face 'font-lock-comment-face)
              (propertize (cdr parts) 'face 'font-lock-constant-face)
              (propertize " [main]" 'face 'font-lock-function-name-face)
              (propertize " λ " 'face 'default))))
       (list
        :directory (shrink-path-dirs)
        :parts parts
        :prompt (substring-no-properties prompt)
        :faces (neomacs-shrink-path-test-face-spans prompt)
        :point-independent
        (with-temp-buffer
          (setq default-directory working)
          (insert "unrelated buffer contents")
          (goto-char 9)
          (shrink-path-prompt)))))))
"##;
    let expect = expect![[
        r#"OK (:directory "~/W/N/.b/cache/" :parts ("~/W/N/.b/" . "cache") :prompt "~/W/N/.b/cache [main] λ " :faces ((0 9 "~/W/N/.b/" font-lock-comment-face) (9 14 "cache" font-lock-constant-face) (14 21 " [main]" font-lock-function-name-face) (21 24 " λ " default)) :point-independent ("~/W/N/.b/" . "cache"))"#
    ]];
    ParityBatchCase::value(
        "readme_eshell_prompt_renders_deep_hidden_directory_with_exact_faces",
        elisp_form,
        expect,
    )
}

fn modeline_file_labels_preserve_filename_and_control_tail_truncation() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-shrink-path-test-with-home
 "shrink-path-file-labels"
 (lambda (_root home)
   (let* ((file (expand-file-name
                 "Projects/Release Engineering/.secrets/artifact-λ.json"
                 home))
          (directory (file-name-directory file)))
     (neomacs-shrink-path-test-write file "{\"ready\":true}\n")
     (list
      :directory (shrink-path-dirs directory)
      :directory-truncated (shrink-path-dirs directory t)
      :file (shrink-path-file file)
      :file-truncated (shrink-path-file file t)
      :trailing-slash-equivalent
      (equal (shrink-path-dirs directory)
             (shrink-path-dirs (directory-file-name directory)))
      :root
      (list (shrink-path-dirs "/")
            (shrink-path-prompt "/"))))))
"##;
    let expect = expect![[
        r#"OK (:directory "~/P/R/.secrets/" :directory-truncated "~/P/R/.s/" :file "~/P/R/.secrets/artifact-λ.json" :file-truncated "~/P/R/.s/artifact-λ.json" :trailing-slash-equivalent t :root ("/" ("" . "/")))"#
    ]];
    ParityBatchCase::value(
        "modeline_file_labels_preserve_filename_and_control_tail_truncation",
        elisp_form,
        expect,
    )
}

fn unique_fish_path_expansion_tracks_live_home_and_distinguishes_existing_and_planned_files()
-> ParityBatchCase {
    let elisp_form = r##"
(neomacs-shrink-path-test-with-home
 "shrink-path-expand-unique"
 (lambda (_root home)
   (let ((source (expand-file-name "Projects/Neomacs/src/main.rs" home))
         (existing "~/P/N/s/main.rs")
         (planned "~/P/N/s/release-notes.md"))
     (neomacs-shrink-path-test-write source "fn main() {}\n")
     (list
      :fixture
      (list :live-home (file-equal-p (expand-file-name "~") home)
            :tree (directory-files home nil "^[^.]")
            :source-exists (file-exists-p source))
      :existing
      (list
       :abbreviated (shrink-path-expand existing)
       :absolute
       (neomacs-shrink-path-test-relative
        (shrink-path-expand existing t) home)
       :file-required
       (shrink-path-file-expand existing t))
      :planned
      (list
       :candidate (shrink-path-file-expand planned)
       :file-required (shrink-path-file-expand planned t)
       :absolute-candidate
       (neomacs-shrink-path-test-relative
        (shrink-path-file-expand planned nil t) home))))))
"##;
    let expect = expect![[
        r#"OK (:fixture (:live-home t :tree ("Projects") :source-exists t) :existing (:abbreviated "~/Projects/Neomacs/src/main.rs" :absolute "Projects/Neomacs/src/main.rs" :file-required "~/Projects/Neomacs/src/main.rs") :planned (:candidate "~/Projects/Neomacs/src/release-notes.md" :file-required nil :absolute-candidate "Projects/Neomacs/src/release-notes.md"))"#
    ]];
    ParityBatchCase::value(
        "unique_fish_path_expansion_tracks_live_home_and_distinguishes_existing_and_planned_files",
        elisp_form,
        expect,
    )
}

fn ambiguous_expansion_tracks_live_home_and_exposes_candidate_list_contract() -> ParityBatchCase {
    let elisp_form = r##"
(neomacs-shrink-path-test-with-home
 "shrink-path-expand-ambiguous"
 (lambda (_root home)
   (let* ((first (expand-file-name "Private/Notes/site/main.rs" home))
          (second (expand-file-name "Projects/Neomacs/src/main.rs" home))
          (short "~/P/N/s/main.rs"))
     (neomacs-shrink-path-test-write first "fn private() {}\n")
     (neomacs-shrink-path-test-write second "fn project() {}\n")
     (let ((abbreviated (shrink-path-expand short))
           (absolute (shrink-path-expand short t))
           (required
            (condition-case error-data
                (list :value (shrink-path-file-expand short t))
              (error
               (list :error (car error-data)
                     :data (cdr error-data)
                     :message (error-message-string error-data))))))
       (list
        :fixture
        (list :live-home (file-equal-p (expand-file-name "~") home)
              :tree (directory-files home nil "^[^.]")
              :files-exist (and (file-exists-p first)
                                (file-exists-p second)))
        :abbreviated abbreviated
        :absolute
        (neomacs-shrink-path-test-relative absolute home)
        :file-expand (shrink-path-file-expand short)
        :exists-required required)))))
"##;
    let expect = expect![[
        r#"OK (:fixture (:live-home t :tree ("Private" "Projects") :files-exist t) :abbreviated ("~/Private/Notes/site/main.rs" "~/Projects/Neomacs/src/main.rs") :absolute ("Private/Notes/site/main.rs" "Projects/Neomacs/src/main.rs") :file-expand ("~/Private/Notes/site/main.rs" "~/Projects/Neomacs/src/main.rs") :exists-required (:error wrong-type-argument :data (stringp ("~/Private/Notes/site/main.rs" "~/Projects/Neomacs/src/main.rs")) :message "Wrong type argument: stringp, (\"~/Private/Notes/site/main.rs\" \"~/Projects/Neomacs/src/main.rs\")"))"#
    ]];
    ParityBatchCase::value(
        "ambiguous_expansion_tracks_live_home_and_exposes_candidate_list_contract",
        elisp_form,
        expect,
    )
}

fn project_modeline_mixes_shrunk_root_relative_tree_and_filename_with_rejection() -> ParityBatchCase
{
    let elisp_form = r##"
(neomacs-shrink-path-test-with-home
 "shrink-path-mixed"
 (lambda (_root home)
   (let* ((project (file-name-as-directory
                    (expand-file-name "Projects/Neomacs" home)))
          (suite (file-name-as-directory
                  (expand-file-name "test/unit" project)))
          (file (expand-file-name "parser-test.el" suite))
          (root-file (expand-file-name "Cargo.toml" project))
          (outside (expand-file-name "Archives/parser-test.el" home)))
     (neomacs-shrink-path-test-write file ";; parser tests\n")
     (neomacs-shrink-path-test-write root-file "[workspace]\n")
     (neomacs-shrink-path-test-write outside ";; archived\n")
     (list
      :nested (shrink-path-file-mixed project suite file)
      :project-root (shrink-path-file-mixed project project root-file)
      :outside-file (shrink-path-file-mixed project suite outside)
      :outside-relative-root
      (shrink-path-file-mixed project
                              (file-name-directory outside)
                              outside)
      :prompt (shrink-path-prompt project)))))
"##;
    let expect = expect![[
        r#"OK (:nested ("~/P/" "Neomacs" "test/unit/" "parser-test.el") :project-root ("~/P/" "Neomacs" nil "Cargo.toml") :outside-file nil :outside-relative-root nil :prompt ("~/P/" . "Neomacs"))"#
    ]];
    ParityBatchCase::value(
        "project_modeline_mixes_shrunk_root_relative_tree_and_filename_with_rejection",
        elisp_form,
        expect,
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        readme_eshell_prompt_renders_deep_hidden_directory_with_exact_faces(),
        modeline_file_labels_preserve_filename_and_control_tail_truncation(),
        unique_fish_path_expansion_tracks_live_home_and_distinguishes_existing_and_planned_files(),
        ambiguous_expansion_tracks_live_home_and_exposes_candidate_list_contract(),
        project_modeline_mixes_shrunk_root_relative_tree_and_filename_with_rejection(),
    ]
}
