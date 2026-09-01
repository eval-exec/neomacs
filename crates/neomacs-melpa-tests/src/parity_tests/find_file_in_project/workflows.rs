use expect_test::expect;

use super::ParityBatchCase;

fn real_project_completion_filters_and_visits_unicode_file() -> ParityBatchCase {
    let elisp_form = r####"
(ffip356-test-run
 "real-project-completion"
 (lambda (root)
   (let* ((project (file-name-as-directory
                    (ffip356-test-owned-path root "project")))
          (origin
           (ffip356-test-write-file
            root "project/src/nested/origin.dat" "Origin buffer.\n"))
          (target
           (ffip356-test-write-file
            root "project/space dir/Unicode界.txt"
            "Unicode target Ω.\nSecond line.\n"))
          (default-directory (file-name-as-directory
                              (file-name-directory origin)))
          (ffip-project-root nil)
          (ffip-project-root-function nil)
          (ffip-project-file '(".git"))
          (ffip-prune-patterns '("*/.git" "*/node_modules" "*/build"))
          (ffip-ignore-filenames '("*.log" "*.png"))
          (ffip-patterns '("*.el" "*.txt")))
     (make-directory (expand-file-name ".git" project) t)
     (ffip356-test-write-file root "project/src/Order Handler.el"
                              ";;; handler\n(defun order-handler () :ok)\n")
     (ffip356-test-write-file root "project/src/nested/order-helper.el"
                              ";;; helper\n")
     (ffip356-test-write-file root "project/node_modules/order.el"
                              ";;; pruned dependency\n")
     (ffip356-test-write-file root "project/build/generated.el"
                              ";;; pruned build\n")
     (ffip356-test-write-file root "project/logs/order.log" "ignored\n")
     (ffip356-test-write-file root "project/assets/order.png" "ignored\n")
     (ffip356-test-write-file root "project/README.md" "excluded pattern\n")
     (let ((origin-buffer (ffip356-test-visit root
                                               "project/src/nested/origin.dat")))
       (switch-to-buffer origin-buffer)
       (ffip356-test-arm-tool
        'find 'delegate "project"
        '("." "(" "-iwholename" "*/.git" "-or"
          "-iwholename" "*/node_modules" "-or"
          "-iwholename" "*/build" ")" "-prune" "-o"
          "-type" "f" "-not" "-name" "*.log"
          "-not" "-name" "*.png"
          "(" "-iwholename" "*.el" "-or"
          "-iwholename" "*.txt" ")" "-print")
        (concat
         "ffip356-find"
         "  . \\( -iwholename \"*/.git\" -or -iwholename \"*/node_modules\" -or -iwholename \"*/build\" \\) -prune -o -type f -not -name \"*.log\" -not -name \"*.png\"  \\( -iwholename \"*.el\" -or -iwholename \"*.txt\" \\)  -print"))
       (let ((input
              (ffip356-test-drive-input
               (lambda () (call-interactively #'find-file-in-project))
               '((:text "space dir/Unicode界.txt" :keys "RET"))))
             (find-trace (ffip356-test-finish-tool 'find)))
         (ffip356-test-own-buffer (current-buffer) root)
         (let* ((history (car ffip-find-files-history))
                (history-files
                 (sort (copy-tree (plist-get history :files))
                       (lambda (left right)
                         (string< (car left) (car right))))))
           (list
            :detected-root
            (ffip356-test-relative (ffip-get-project-root-directory) root)
            :find find-trace
            :input input
            :visited
            (list :file (ffip356-test-relative buffer-file-name root)
                  :name (buffer-name)
                  :mode major-mode
                  :bytes (buffer-substring-no-properties
                          (point-min) (point-max))
                  :disk-bytes (ffip356-test-file-bytes target)
                  :point (point)
                  :selected (eq (current-buffer) (window-buffer)))
            :history
            (list :files history-files
                  :keyword (plist-get history :keyword)
                  :directory-p (plist-get history :directory-p)
                  :function (plist-get history :function)
                  :forward-lines (plist-get history :forward-lines)
                  :root
                  (ffip356-test-relative
                   (plist-get history :default-directory) root)))))))))
"####;
    ParityBatchCase::value(
        "real-project-completion-filters-and-visits-unicode-file",
        elisp_form,
        expect![[
            r#"OK (:result (:detected-root "project/" :find ((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" "-or" "-iwholename" "*/node_modules" "-or" "-iwholename" "*/build" ")" "-prune" "-o" "-type" "f" "-not" "-name" "*.log" "-not" "-name" "*.png" "(" "-iwholename" "*.el" "-or" "-iwholename" "*.txt" ")" "-print"))) :input ((:prompt "Find in project/: " :initial-input "" :require-match nil :category project-file :candidates ("space dir/Unicode界.txt" "src/Order Handler.el" "src/nested/order-helper.el") :final-input "space dir/Unicode界.txt")) :visited (:file "project/space dir/Unicode界.txt" :name "Unicode界.txt" :mode text-mode :bytes "Unicode target Ω.\nSecond line.\n" :disk-bytes "Unicode target Ω.\nSecond line.\n" :point 1 :selected t) :history (:files (("space dir/Unicode界.txt" . "./space dir/Unicode界.txt") ("src/Order Handler.el" . "./src/Order Handler.el") ("src/nested/order-helper.el" . "./src/nested/order-helper.el")) :keyword nil :directory-p nil :function nil :forward-lines nil :root "project/")) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :timer-details nil :owned-buffer-live nil :owned-process-live nil :owned-timer-live nil :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :transient-mark-restored t :dired-restored t :advice-count 1 :advice-restored t :hijack nil :unread-events nil :active-minibuffer nil :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn selected_line_bounded_history_and_frozen_resume() -> ParityBatchCase {
    let elisp_form = r####"
(ffip356-test-run
 "history-resume"
 (lambda (root)
   (let* ((project (file-name-as-directory
                    (ffip356-test-owned-path root "project")))
          (default-directory project)
          (ffip-project-root project)
          (ffip-project-file '(".git"))
          (ffip-patterns nil)
          (ffip-prune-patterns '("*/.git"))
          (ffip-find-files-history-max-items 2)
          selected first-search third-search resume-observations fresh-search
          full-path)
     (make-directory (expand-file-name ".git" project) t)
     (ffip356-test-write-file root "project/docs/Guide.md"
                              "# Guide\n\nPractical notes.\n")
     (ffip356-test-write-file root "project/docs/app.el" ";;; docs app\n")
     (ffip356-test-write-file root "project/src/app.el"
                              ";;; source\n(defun app ()\n  :source)\n")
     (ffip356-test-write-file root "project/other.el" ";;; other\n")

     (ffip356-test-arm-tool
      'find 'delegate "project"
      '("." "(" "-iwholename" "*/.git" ")" "-prune" "-o"
        "-type" "f" "-iname" "*Guide.md*" "-print")
      "ffip356-find  . \\( -iwholename \"*/.git\" \\) -prune -o -type f    -iname \"*Guide.md*\" -print")
     (with-temp-buffer
       (setq default-directory project)
       (insert "Guide.md:3")
       (goto-char (point-min))
       (set-mark (point-max))
       (setq mark-active t)
       (let ((transient-mark-mode t))
         (call-interactively #'find-file-in-project-by-selected))
       (setq selected (current-buffer)))
     (let ((selected-trace (ffip356-test-finish-tool 'find)))
       (ffip356-test-own-buffer selected root)

       (ffip356-test-arm-tool
        'find 'delegate "project"
        '("." "(" "-iwholename" "*/.git" ")" "-prune" "-o"
          "-type" "f" "-iname" "*app.el*" "-print")
        "ffip356-find  . \\( -iwholename \"*/.git\" \\) -prune -o -type f    -iname \"*app.el*\" -print")
       (setq first-search
             (ffip356-test-drive-input
              (lambda () (ffip-find-files "app.el" nil))
              '((:text "src/app.el" :keys "RET"))))
       (ffip356-test-own-buffer (current-buffer) root)
       (let ((app-trace (ffip356-test-finish-tool 'find)))

         (ffip356-test-arm-tool
          'find 'delegate "project"
          '("." "(" "-iwholename" "*/.git" ")" "-prune" "-o"
            "-type" "f" "-iname" "*other.el*" "-print")
          "ffip356-find  . \\( -iwholename \"*/.git\" \\) -prune -o -type f    -iname \"*other.el*\" -print")
         (setq third-search (ffip-find-files "other.el" nil))
         (ffip356-test-own-buffer (current-buffer) root)
         (let ((other-trace (ffip356-test-finish-tool 'find))
               (frozen-history (copy-tree ffip-find-files-history)))
           (ffip356-test-write-file root "project/new/app.el" ";;; new app\n")
           (ffip-find-files-resume 0)
           (ffip356-test-own-buffer (current-buffer) root)
           (push (list :index 0
                       :file (ffip356-test-relative buffer-file-name root))
                 resume-observations)
           (let ((resume-reader
                  (ffip356-test-drive-input
                   (lambda () (ffip-find-files-resume 1))
                   '((:text "src/app.el" :keys "RET")))))
             (ffip356-test-own-buffer (current-buffer) root)
             (push (list :index 1 :reader resume-reader
                         :file (ffip356-test-relative buffer-file-name root))
                   resume-observations))
           (ffip356-test-assert-no-tool-call 'find)

           (ffip356-test-arm-tool
            'find 'delegate "project"
            '("." "(" "-iwholename" "*/.git" ")" "-prune" "-o"
              "-type" "f" "-iname" "*app.el*" "-print")
            "ffip356-find  . \\( -iwholename \"*/.git\" \\) -prune -o -type f    -iname \"*app.el*\" -print")
           (setq fresh-search
                 (ffip356-test-drive-input
                  (lambda () (ffip-find-files "app.el" nil))
                  '((:text "new/app.el" :keys "RET"))))
           (ffip356-test-own-buffer (current-buffer) root)
           (let ((fresh-trace (ffip356-test-finish-tool 'find))
                 (history-after-fresh (copy-tree ffip-find-files-history))
                 (fresh-file (ffip356-test-relative buffer-file-name root))
                 (fresh-bytes (buffer-string)))
             (ffip356-test-arm-tool
              'find 'delegate "project"
              '("." "(" "-iwholename" "*/.git" ")" "-prune" "-o"
                "-type" "f" "(" "-iwholename" "*.el" ")"
                "-iwholename" "*docs*" "-print")
              "ffip356-find  . \\( -iwholename \"*/.git\" \\) -prune -o -type f   \\( -iwholename \"*.el\" \\) -iwholename \"*docs*\" -print")
             (let ((ffip-match-path-instead-of-filename t)
                   (ffip-patterns '("*.el")))
               (ffip-find-files "docs" nil))
             (ffip356-test-own-buffer (current-buffer) root)
             (setq full-path
                   (list :trace (ffip356-test-finish-tool 'find)
                         :files
                         (copy-tree
                          (plist-get (car ffip-find-files-history) :files))
                         :file
                         (ffip356-test-relative buffer-file-name root)
                         :keyword
                         (plist-get (car ffip-find-files-history) :keyword)
                         :input-clean
                         (and (null unread-command-events)
                              (not (active-minibuffer-window)))))
             (list
              :selected
              (with-current-buffer selected
                (list :file (ffip356-test-relative buffer-file-name root)
                      :line (line-number-at-pos)
                      :point (point)
                      :text (buffer-substring-no-properties
                             (line-beginning-position) (line-end-position))))
              :filename-history ffip-filename-history
              :readers (list first-search fresh-search)
              :discarded-return third-search
              :traces (list selected-trace app-trace other-trace fresh-trace)
              :frozen-history frozen-history
              :resume (nreverse resume-observations)
              :history-after-fresh history-after-fresh
              :full-path full-path
              :fresh-file fresh-file
              :fresh-bytes fresh-bytes))))))))
"####;
    ParityBatchCase::value(
        "selected-line-search-bounded-history-and-frozen-resume",
        elisp_form,
        expect![[
            r#"OK (:result (:selected (:file "project/docs/Guide.md" :line 3 :point 10 :text "Practical notes.") :filename-history ("Guide.md:3") :readers (((:prompt "Find in project/: " :initial-input "" :require-match nil :category project-file :candidates ("docs/app.el" "src/app.el") :final-input "src/app.el")) ((:prompt "Find in project/: " :initial-input "" :require-match nil :category project-file :candidates ("docs/app.el" "new/app.el" "src/app.el") :final-input "new/app.el"))) :discarded-return nil :traces (((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" ")" "-prune" "-o" "-type" "f" "-iname" "*Guide.md*" "-print"))) ((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" ")" "-prune" "-o" "-type" "f" "-iname" "*app.el*" "-print"))) ((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" ")" "-prune" "-o" "-type" "f" "-iname" "*other.el*" "-print"))) ((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" ")" "-prune" "-o" "-type" "f" "-iname" "*app.el*" "-print")))) :frozen-history ((:files (("other.el" . "./other.el")) :keyword "other.el" :directory-p nil :function nil :forward-lines nil :default-directory "[ROOT]/project/") (:files (("src/app.el" . "./src/app.el") ("docs/app.el" . "./docs/app.el")) :keyword "app.el" :directory-p nil :function nil :forward-lines nil :default-directory "[ROOT]/project/")) :resume ((:index 0 :file "project/other.el") (:index 1 :reader ((:prompt "Find in project/: " :initial-input "" :require-match nil :category project-file :candidates ("docs/app.el" "src/app.el") :final-input "src/app.el")) :file "project/src/app.el")) :history-after-fresh ((:files (("src/app.el" . "./src/app.el") ("docs/app.el" . "./docs/app.el") ("new/app.el" . "./new/app.el")) :keyword "app.el" :directory-p nil :function nil :forward-lines nil :default-directory "[ROOT]/project/") (:files (("other.el" . "./other.el")) :keyword "other.el" :directory-p nil :function nil :forward-lines nil :default-directory "[ROOT]/project/")) :full-path (:trace ((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" ")" "-prune" "-o" "-type" "f" "(" "-iwholename" "*.el" ")" "-iwholename" "*docs*" "-print"))) :files (("docs/app.el" . "./docs/app.el")) :file "project/docs/app.el" :keyword "docs" :input-clean t) :fresh-file "project/new/app.el" :fresh-bytes ";;; new app\n") :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :timer-details nil :owned-buffer-live nil :owned-process-live nil :owned-timer-live nil :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :transient-mark-restored t :dired-restored t :advice-count 1 :advice-restored t :hijack nil :unread-events nil :active-minibuffer nil :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn directory_dired_and_relative_org_link() -> ParityBatchCase {
    let elisp_form = r####"
(ffip356-test-run
 "directory-relative-link"
 (lambda (root)
   (let* ((project (file-name-as-directory
                    (ffip356-test-owned-path root "project")))
          (default-directory project)
          (ffip-project-root project)
          (ffip-project-file '(".git"))
          (ffip-prune-patterns '("*/.git"))
          (ffip-patterns '("*.el"))
          conflict conflict-trace dired-buffer dired-state link-state)
     (make-directory (expand-file-name ".git" project) t)
     (ffip356-test-write-file root "project/components/widget Ω/inside.txt"
                              "inside Ω\n")
     (ffip356-test-write-file root "project/src/app.el"
                              ";;; Guide.md link source\n")
     (ffip356-test-write-file root "project/docs/Guide.md"
                              "# Guide\n\nLinked.\n")

     (ffip356-test-arm-tool
      'find 'delegate "project"
      '("." "(" "-iwholename" "*/.git" ")" "-prune" "-o"
        "-type" "d" "(" "-iwholename" "*.el" ")"
        "-iwholename" "*components/widget Ω" "-print")
      "ffip356-find  . \\( -iwholename \"*/.git\" \\) -prune -o -type d   \\( -iwholename \"*.el\" \\) -iwholename \"*components/widget Ω\" -print")
     (with-temp-buffer
       (setq default-directory project)
       (insert "components/widget Ω")
       (set-mark (point-min))
       (setq mark-active t)
       (let ((transient-mark-mode t))
         (setq conflict
               (ffip356-test-observe-messages
                (lambda ()
                  (call-interactively
                   #'find-directory-in-project-by-selected))))))
     (setq conflict-trace (ffip356-test-finish-tool 'find))

     (let ((ffip-patterns nil))
       (ffip356-test-arm-tool
        'find 'delegate "project"
        '("." "(" "-iwholename" "*/.git" ")" "-prune" "-o"
          "-type" "d" "-iwholename" "*components/widget Ω" "-print")
        "ffip356-find  . \\( -iwholename \"*/.git\" \\) -prune -o -type d    -iwholename \"*components/widget Ω\" -print")
       (with-temp-buffer
         (setq default-directory project)
         (insert "components/widget Ω")
         (set-mark (point-min))
         (setq mark-active t)
         (let ((transient-mark-mode t))
           (call-interactively #'find-directory-in-project-by-selected))
         (setq dired-buffer (current-buffer)))
       (let ((directory-trace (ffip356-test-finish-tool 'find)))
         (ffip356-test-own-buffer dired-buffer root)
         (with-current-buffer dired-buffer
           (goto-char (point-min))
           (search-forward "inside.txt")
           (dired-move-to-filename)
           (let ((filename-start (point)))
             (dired-move-to-end-of-filename)
           (setq dired-state
                 (list :trace directory-trace
                       :mode major-mode
                       :default-directory
                       (ffip356-test-relative default-directory root)
                       :dired-directory
                       (ffip356-test-relative dired-directory root)
                       :listing (sort (directory-files default-directory nil
                                                       nil t)
                                      #'string<)
                       :filename-property
                       (get-text-property filename-start 'dired-filename)
                       :line (line-number-at-pos)
                       :filename-width (- (point) filename-start)
                       :rendered-name
                       (buffer-substring-no-properties filename-start (point))
                       :name-properties
                       (ffip356-test-property-runs filename-start (point))
                       :selected (eq dired-buffer (window-buffer))))))))

     (let ((source (ffip356-test-visit root "project/src/app.el"))
           (ffip-patterns '("*.md"))
           (ffip-find-relative-path-callback #'ffip-copy-org-file-link))
       (switch-to-buffer source)
       (goto-char (point-min))
       (search-forward "Guide.md")
       (set-mark (- (point) (length "Guide.md")))
       (setq mark-active t)
       (let ((before (buffer-string)) (before-point (point)))
         (ffip356-test-arm-tool
          'find 'delegate "project"
          '("." "(" "-iwholename" "*/.git" ")" "-prune" "-o"
            "-type" "f" "(" "-iwholename" "*.md" ")"
            "-iname" "*Guide.md*" "-print")
          "ffip356-find  . \\( -iwholename \"*/.git\" \\) -prune -o -type f   \\( -iwholename \"*.md\" \\) -iname \"*Guide.md*\" -print")
         (let ((transient-mark-mode t))
           (call-interactively #'ffip-find-relative-path))
         (setq link-state
               (list :trace (ffip356-test-finish-tool 'find)
                     :file (ffip356-test-relative buffer-file-name root)
                     :kill-head (car kill-ring)
                     :yank-head (car kill-ring-yank-pointer)
                     :bytes (buffer-string)
                     :unchanged (equal (buffer-string) before)
                     :point (point)
                     :point-unchanged (= (point) before-point)
                     :modified (buffer-modified-p)))))
     (list :pattern-conflict conflict
           :conflict-trace conflict-trace
           :dired dired-state
           :relative-link link-state))))
"####;
    ParityBatchCase::value(
        "directory-dired-and-relative-org-link",
        elisp_form,
        expect![[
            r#"OK (:result (:pattern-conflict (:outcome (:value "Nothing found!") :messages ("Nothing found!")) :conflict-trace ((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" ")" "-prune" "-o" "-type" "d" "(" "-iwholename" "*.el" ")" "-iwholename" "*components/widget Ω" "-print"))) :dired (:trace ((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" ")" "-prune" "-o" "-type" "d" "-iwholename" "*components/widget Ω" "-print"))) :mode dired-mode :default-directory "project/components/widget Ω/" :dired-directory "project/components/widget Ω/" :listing ("." ".." "inside.txt") :filename-property t :line 4 :filename-width 10 :rendered-name "inside.txt" :name-properties ((:text "inside.txt" :face nil :font-lock-face nil)) :selected t) :relative-link (:trace ((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" ")" "-prune" "-o" "-type" "f" "(" "-iwholename" "*.md" ")" "-iname" "*Guide.md*" "-print"))) :file "project/src/app.el" :kill-head "[[file:../docs/Guide.md]]" :yank-head "[[file:../docs/Guide.md]]" :bytes ";;; Guide.md link source\n" :unchanged t :point 13 :point-unchanged t :modified nil)) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :timer-details nil :owned-buffer-live nil :owned-process-live nil :owned-timer-live nil :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :transient-mark-restored t :dired-restored t :advice-count 1 :advice-restored t :hijack nil :unread-events nil :active-minibuffer nil :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn at_point_resolution_and_reference_repair() -> ParityBatchCase {
    let elisp_form = r####"
(ffip356-test-run
 "at-point-repair"
 (lambda (root)
   (let* ((project (file-name-as-directory
                    (ffip356-test-owned-path root "project")))
          (origin-file
           (ffip356-test-write-file
            root "project/src/main.js"
            "const module = require(\"./lib/module\");\nconst env = \"$FFIP356_ASSET\";\nconst relative = \"../Guide.md\";\nconst absolute = \"../Unicode界.txt\";\n"))
          (default-directory project)
          (ffip-project-root project)
          (ffip-project-file '(".git"))
          (ffip-patterns nil)
          (ffip-prune-patterns '("*/.git"))
          (origin (ffip356-test-visit root "project/src/main.js"))
          direct env-route relative-repair absolute-repair)
     (make-directory (expand-file-name ".git" project) t)
     (ffip356-test-write-file root "project/src/lib/module.ts"
                              "export const source = 'module.ts';\n")
     (ffip356-test-write-file root "project/src/lib/module/index.js"
                              "module.exports = 'index.js';\n")
     (let ((asset (ffip356-test-write-file
                   root "project/assets/Env Ω.txt" "environment Ω\n")))
       (setenv "FFIP356_ASSET" asset))
     (ffip356-test-write-file root "project/docs/Guide.md" "# Guide\n")
     (ffip356-test-write-file root "project/assets/Unicode界.txt" "asset 界\n")

     (switch-to-buffer origin)
     (js-mode)
     (goto-char (point-min))
     (search-forward "./lib/module")
     (backward-char 3)
     (call-interactively #'find-file-in-project-at-point)
     (ffip356-test-own-buffer (current-buffer) root)
     (setq direct
           (list :file (ffip356-test-relative buffer-file-name root)
                 :bytes (buffer-string)
                 :mode major-mode
                 :find-not-called (ffip356-test-assert-no-tool-call 'find)))

     (switch-to-buffer origin)
     (goto-char (point-min))
     (search-forward "$FFIP356_ASSET")
     (backward-char 4)
     (call-interactively #'find-file-in-project-at-point)
     (ffip356-test-own-buffer (current-buffer) root)
     (setq env-route
           (list :file (ffip356-test-relative buffer-file-name root)
                 :bytes (buffer-string)
                 :find-not-called (ffip356-test-assert-no-tool-call 'find)))

     (switch-to-buffer origin)
     (goto-char (point-min))
     (search-forward "../Guide.md")
     (backward-char 4)
     (let ((before (buffer-string)))
       (ffip356-test-arm-tool
        'find 'delegate "project"
        '("." "(" "-iwholename" "*/.git" ")" "-prune" "-o"
          "-type" "f" "-iname" "*Guide.md*" "-print")
        "ffip356-find  . \\( -iwholename \"*/.git\" \\) -prune -o -type f    -iname \"*Guide.md*\" -print")
       (call-interactively #'ffip-fix-file-path-at-point)
       (setq relative-repair
             (list :before before :after (buffer-string)
                   :point (point) :mark (mark t)
                   :modified (buffer-modified-p)
                   :trace (ffip356-test-finish-tool 'find))))

     (goto-char (point-min))
     (search-forward "../Unicode界.txt")
     (backward-char 5)
     (let ((before (buffer-string))
           (replacement-start (car (bounds-of-thing-at-point 'filename))))
       (ffip356-test-arm-tool
        'find 'delegate "project"
        '("." "(" "-iwholename" "*/.git" ")" "-prune" "-o"
          "-type" "f" "-iname" "*Unicode界.txt*" "-print")
        "ffip356-find  . \\( -iwholename \"*/.git\" \\) -prune -o -type f    -iname \"*Unicode界.txt*\" -print")
       (ffip-fix-file-path-at-point t)
       (setq absolute-repair
             (list :before before :after (buffer-string)
                   :replacement-start replacement-start
                   :point-at-replacement-end
                   (and (eq (char-after) ?\")
                        (string-suffix-p
                         "Unicode界.txt"
                         (buffer-substring-no-properties
                          replacement-start (point))))
                   :mark (mark t)
                   :modified (buffer-modified-p)
                   :trace (ffip356-test-finish-tool 'find))))
     (list :origin (ffip356-test-relative origin-file root)
           :direct direct :environment env-route
           :relative-repair relative-repair
           :absolute-repair absolute-repair))))
"####;
    ParityBatchCase::value(
        "at-point-resolution-and-reference-repair",
        elisp_form,
        expect![[
            r#"OK (:result (:origin "project/src/main.js" :direct (:file "project/src/lib/module/index.js" :bytes "module.exports = 'index.js';\n" :mode js-mode :find-not-called t) :environment (:file "project/assets/Env Ω.txt" :bytes "environment Ω\n" :find-not-called t) :relative-repair (:before "const module = require(\"./lib/module\");\nconst env = \"$FFIP356_ASSET\";\nconst relative = \"../Guide.md\";\nconst absolute = \"../Unicode界.txt\";\n" :after "const module = require(\"./lib/module\");\nconst env = \"$FFIP356_ASSET\";\nconst relative = \"../docs/Guide.md\";\nconst absolute = \"../Unicode界.txt\";\n" :point 105 :mark nil :modified t :trace ((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" ")" "-prune" "-o" "-type" "f" "-iname" "*Guide.md*" "-print")))) :absolute-repair (:before "const module = require(\"./lib/module\");\nconst env = \"$FFIP356_ASSET\";\nconst relative = \"../docs/Guide.md\";\nconst absolute = \"../Unicode界.txt\";\n" :after "const module = require(\"./lib/module\");\nconst env = \"$FFIP356_ASSET\";\nconst relative = \"../docs/Guide.md\";\nconst absolute = \"[ROOT]/project/assets/Unicode界.txt\";\n" :replacement-start 126 :point-at-replacement-end t :mark nil :modified t :trace ((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" ")" "-prune" "-o" "-type" "f" "-iname" "*Unicode界.txt*" "-print"))))) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :timer-details nil :owned-buffer-live nil :owned-process-live nil :owned-timer-live nil :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :transient-mark-restored t :dired-restored t :advice-count 1 :advice-restored t :hijack nil :unread-events nil :active-minibuffer nil :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn external_find_failures_and_pure_lisp_recovery() -> ParityBatchCase {
    let elisp_form = r####"
(ffip356-test-run
 "find-failure-recovery"
 (lambda (root)
   (let* ((project (file-name-as-directory
                    (ffip356-test-owned-path root "project")))
          (default-directory project)
          (ffip-project-root project)
          (ffip-project-file '(".git"))
          (ffip-prune-patterns '("*/.git" "*/bin"))
          (ffip-patterns '("*.el"))
          quiet quiet-trace diagnostic diagnostic-trace fake-buffer
          fake-state fallback-reader)
     (make-directory (expand-file-name ".git" project) t)
     (ffip356-test-write-file root "project/Target Ω.el" "target Ω\n")
     (ffip356-test-write-file root "project/docs/Guide.md"
                              "# Guide\n\nFallback.\n")

     (ffip356-test-arm-tool
      'find 'quiet "project"
      '("." "(" "-iwholename" "*/.git" "-or" "-iwholename" "*/bin"
        ")" "-prune" "-o" "-type" "f" "(" "-iwholename" "*.el"
        ")" "-iname" "*Target*" "-print")
      "ffip356-find  . \\( -iwholename \"*/.git\" -or -iwholename \"*/bin\" \\) -prune -o -type f   \\( -iwholename \"*.el\" \\) -iname \"*Target*\" -print")
     (with-temp-buffer
       (setq default-directory project)
       (insert "Target")
       (set-mark (point-min))
       (setq mark-active t)
       (let ((transient-mark-mode t))
         (setq quiet
               (ffip356-test-observe-messages
                (lambda ()
                  (call-interactively
                   #'find-file-in-project-by-selected))))))
     (setq quiet-trace (ffip356-test-finish-tool 'find))
     (let ((quiet-state
            (list :observation quiet
                  :history ffip-find-files-history
                  :fake-buffer (and (get-buffer "controlled find failure Ω") t)
                  :fake-file
                  (file-exists-p
                   (expand-file-name "controlled find failure Ω" project)))))

       (ffip356-test-arm-tool
        'find 'diagnostic "project"
        '("." "(" "-iwholename" "*/.git" "-or" "-iwholename" "*/bin"
          ")" "-prune" "-o" "-type" "f" "(" "-iwholename" "*.el"
          ")" "-iname" "*Target*" "-print")
        "ffip356-find  . \\( -iwholename \"*/.git\" -or -iwholename \"*/bin\" \\) -prune -o -type f   \\( -iwholename \"*.el\" \\) -iname \"*Target*\" -print")
       (with-temp-buffer
         (setq default-directory project)
         (insert "Target")
         (set-mark (point-min))
         (setq mark-active t)
         (let ((transient-mark-mode t))
           (setq diagnostic
                 (ffip356-test-observe-messages
                  (lambda ()
                    (call-interactively
                     #'find-file-in-project-by-selected)))))
         (setq fake-buffer (current-buffer)))
       (setq diagnostic-trace (ffip356-test-finish-tool 'find))
       (ffip356-test-own-buffer fake-buffer root)
       (with-current-buffer fake-buffer
         (setq fake-state
               (list :observation diagnostic
                     :name (buffer-name)
                     :file (ffip356-test-relative buffer-file-name root)
                     :bytes (buffer-string)
                     :modified (buffer-modified-p)
                     :disk-exists (file-exists-p buffer-file-name)
                     :history (copy-tree ffip-find-files-history)))
         (set-buffer-modified-p nil))
       (kill-buffer fake-buffer)

       (let ((ffip-patterns '("*.md")))
         (setq fallback-reader
               (ffip356-test-drive-input
                (lambda ()
                  (call-interactively #'ffip-lisp-find-file-in-project))
                '((:text "Guide\\.md\\'" :keys "RET")))))
       (ffip356-test-own-buffer (current-buffer) root)
       (let ((recovery
              (list :reader fallback-reader
                    :file (ffip356-test-relative buffer-file-name root)
                    :line (line-number-at-pos)
                    :point (point)
                    :bytes (buffer-string)
                    :external-find-not-called
                    (ffip356-test-assert-no-tool-call 'find)
                    :fake-buffer-live (and (buffer-live-p fake-buffer) t)
                    :fake-file-exists
                    (file-exists-p
                     (expand-file-name "controlled find failure Ω" project)))))
         (list :quiet quiet-state :quiet-trace quiet-trace
               :diagnostic fake-state :diagnostic-trace diagnostic-trace
               :recovery recovery))))))
"####;
    ParityBatchCase::value(
        "external-find-failures-and-pure-lisp-recovery",
        elisp_form,
        expect![[
            r##"OK (:result (:quiet (:observation (:outcome (:value "Nothing found!") :messages ("Nothing found!")) :history nil :fake-buffer nil :fake-file nil) :quiet-trace ((:tool find :mode quiet :cwd "project" :argv ("." "(" "-iwholename" "*/.git" "-or" "-iwholename" "*/bin" ")" "-prune" "-o" "-type" "f" "(" "-iwholename" "*.el" ")" "-iname" "*Target*" "-print"))) :diagnostic (:observation (:outcome (:value nil) :messages nil) :name "controlled find failure Ω" :file "project/controlled find failure Ω" :bytes "" :modified nil :disk-exists nil :history ((:files (("controlled find failure Ω" . "controlled find failure Ω")) :keyword "Target" :directory-p nil :function nil :forward-lines nil :default-directory "[ROOT]/project/"))) :diagnostic-trace ((:tool find :mode diagnostic :cwd "project" :argv ("." "(" "-iwholename" "*/.git" "-or" "-iwholename" "*/bin" ")" "-prune" "-o" "-type" "f" "(" "-iwholename" "*.el" ")" "-iname" "*Target*" "-print"))) :recovery (:reader ((:prompt "Input regex (or press ENTER): " :initial-input "" :require-match nil :category nil :candidates nil :final-input "Guide\\.md\\'")) :file "project/docs/Guide.md" :line 1 :point 1 :bytes "# Guide\n\nFallback.\n" :external-find-not-called t :fake-buffer-live nil :fake-file-exists nil)) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :timer-details nil :owned-buffer-live nil :owned-process-live nil :owned-timer-live nil :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :transient-mark-restored t :dired-restored t :advice-count 1 :advice-restored t :hijack nil :unread-events nil :active-minibuffer nil :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn real_git_diff_review_filter_jump_and_apply() -> ParityBatchCase {
    let elisp_form = r####"
(ffip356-test-run
 "git-diff-review"
 (lambda (root)
   (let* ((project (file-name-as-directory
                    (ffip356-test-owned-path root "project")))
          (default-directory project)
          (ffip-project-root project)
          (ffip-project-file '(".git"))
          (ffip-patterns nil)
          (ffip-prune-patterns '("*/.git"))
          (ffip-find-files-history-max-items 0)
          (source-original
           ";;; app.el\n(defun app ()\n  :source)\n;; Guide.md\n")
          (source-changed
           ";;; app.el\n(defun app ()\n  :changed)\n;; Guide.md\n")
          (guide-original "# Guide\n\nOriginal Ω.\n")
          (guide-changed "# Guide\n\nChanged Ω.\n")
          source-buffer diff-buffer chooser initial filtered jump apply
          immediate saved malformed malformed-input-calls)
     (ffip356-test-write-file root "project/src/app.el" source-original)
     (ffip356-test-write-file root "project/docs/Guide Ω.md" guide-original)
     (ffip356-test-init-git root "project")
     (ffip356-test-write-file root "project/src/app.el" source-changed)
     (ffip356-test-write-file root "project/docs/Guide Ω.md" guide-changed)
     (setq source-buffer (ffip356-test-visit root "project/src/app.el"))
     (switch-to-buffer source-buffer)
     (revert-buffer t t)

     (let ((ffip-diff-backends
            '(("Cached changes" .
               "ffip356-git --no-pager diff --cached")
              ("Working tree vs HEAD" .
               "ffip356-git --no-pager diff HEAD"))))
       (ffip356-test-arm-tool
        'git 'delegate "project"
        '("--no-pager" "diff" "HEAD"))
       (setq chooser
             (ffip356-test-drive-input
              (lambda () (call-interactively #'ffip-show-diff))
              '((:text "1: Working tree vs HEAD" :keys "RET")))))
     (let ((git-trace (ffip356-test-finish-tool 'git)))
       (setq diff-buffer (get-buffer "*ffip-diff*"))
       (ffip356-test-own-buffer diff-buffer root)
       (with-current-buffer diff-buffer
         (font-lock-ensure)
         (setq initial
               (list :chooser chooser :git git-trace
                     :mode major-mode :read-only buffer-read-only
                     :truncate truncate-lines
                     :point (point)
                     :selected (eq diff-buffer (window-buffer))
                     :remap (lookup-key (current-local-map)
                                        [remap diff-goto-source])
                     :bytes (buffer-substring-no-properties
                             (point-min) (point-max))
                     :properties
                     (ffip356-test-property-runs (point-min) (point-max))))
         (goto-char (point-min))
         (re-search-forward "^@@")
         (setq filtered
               (ffip356-test-drive-input
                (lambda ()
                  (call-interactively
                   #'ffip-diff-filter-hunks-by-file-name))
                '((:text "src !docs" :keys "RET"))))
         (setq filtered
               (list :reader filtered
                     :bytes (buffer-substring-no-properties
                             (point-min) (point-max))
                     :kill-head (car kill-ring)))

         (goto-char (point-min))
         (re-search-forward "^@@")
         (ffip356-test-arm-tool
          'find 'delegate "project"
          '("." "(" "-iwholename" "*/.git" ")" "-prune" "-o"
            "-type" "f" "-iwholename" "*src/app.el*" "-print")
          "ffip356-find  . \\( -iwholename \"*/.git\" \\) -prune -o -type f    -iwholename \"*src/app.el*\" -print")
         (call-interactively #'ffip-diff-find-file))
       (ffip356-test-own-buffer (current-buffer) root)
       (setq jump
             (list :trace (ffip356-test-finish-tool 'find)
                   :file (ffip356-test-relative buffer-file-name root)
                   :line (line-number-at-pos)
                   :point (point)
                   :text (buffer-substring-no-properties
                          (line-beginning-position) (line-end-position))
                   :bytes (buffer-string)))

       (switch-to-buffer diff-buffer)
       (goto-char (point-min))
       (re-search-forward "^@@")
       (setq apply
             (ffip356-test-observe-messages
              (lambda () (ffip-diff-apply-hunk t))))
       (setq immediate
             (list :observation apply
                   :hijack ffip-read-file-name-hijacked-p
                   :buffer-bytes
                   (with-current-buffer source-buffer (buffer-string))
                   :buffer-modified
                   (with-current-buffer source-buffer (buffer-modified-p))
                   :disk-bytes
                   (ffip356-test-file-bytes
                    (expand-file-name "src/app.el" project))
                   :git-diff
                   (ffip356-test-git project "--no-pager" "diff" "HEAD")))
       (with-current-buffer source-buffer
         (save-buffer))
       (setq saved
             (list :hijack ffip-read-file-name-hijacked-p
                   :buffer-bytes
                   (with-current-buffer source-buffer (buffer-string))
                   :buffer-modified
                   (with-current-buffer source-buffer (buffer-modified-p))
                   :disk-bytes
                   (ffip356-test-file-bytes
                    (expand-file-name "src/app.el" project))
                   :remaining-git-diff
                   (ffip356-test-git project "--no-pager" "diff" "HEAD")
                   :diff-buffer-bytes
                   (with-current-buffer diff-buffer
                     (buffer-substring-no-properties
                      (point-min) (point-max)))))
       (with-temp-buffer
         (setq default-directory project)
         (insert "not a unified diff\n")
         (ffip-diff-mode)
         (goto-char (point-min))
         (setq malformed-input-calls 0)
         (let ((executing-kbd-macro t))
           (minibuffer-with-setup-hook
               (lambda ()
                 (setq malformed-input-calls
                       (1+ malformed-input-calls))
                 (error "unexpected malformed-apply minibuffer: %S"
                        (minibuffer-prompt)))
             (setq malformed
                   (ffip356-test-observe-messages
                    (lambda ()
                      (call-interactively #'ffip-diff-apply-hunk))))))
         (unless (zerop malformed-input-calls)
           (error "malformed apply unexpectedly requested input: %S"
                  malformed-input-calls))
         (setq malformed
               (list :observation malformed
                     :mode major-mode
                     :bytes (buffer-substring-no-properties
                             (point-min) (point-max))
                     :hijack-retained ffip-read-file-name-hijacked-p
                     :input-calls malformed-input-calls
                     :find-not-called
                     (ffip356-test-assert-no-tool-call 'find))))
       ;; The public function intentionally leaves this global true when GNU
       ;; diff-mode signals.  Preserve that observation, then restore the
       ;; dynamically owned baseline so this shared-batch case can tear down.
       (setq ffip-read-file-name-hijacked-p nil)
       (list :initial initial :filtered filtered :jump jump
             :apply (list :immediate immediate :saved saved
                          :malformed malformed
                          :hijack-reset ffip-read-file-name-hijacked-p))))))
"####;
    ParityBatchCase::value(
        "real-git-diff-review-filter-jump-and-apply",
        elisp_form,
        expect![[
            r#"OK (:result (:initial (:chooser ((:prompt "Run diff backend: " :initial-input "" :require-match nil :category project-file :candidates ("0: Cached changes" "1: Working tree vs HEAD") :final-input "1: Working tree vs HEAD")) :git ((:tool git :mode delegate :cwd "project" :argv ("--no-pager" "diff" "HEAD"))) :mode ffip-diff-mode :read-only t :truncate t :point 1 :selected t :remap ffip-diff-find-file :bytes "diff --git \"a/docs/Guide \\316\\251.md\" \"b/docs/Guide \\316\\251.md\"\nindex 2ed70c2..d36a293 100644\n--- \"a/docs/Guide \\316\\251.md\"\11\n+++ \"b/docs/Guide \\316\\251.md\"\11\n@@ -1,3 +1,3 @@\n # Guide\n \n-Original Ω.\n+Changed Ω.\ndiff --git a/src/app.el b/src/app.el\nindex d6acc5e..bf88545 100644\n--- a/src/app.el\n+++ b/src/app.el\n@@ -1,4 +1,4 @@\n ;;; app.el\n (defun app ()\n-  :source)\n+  :changed)\n ;; Guide.md\n" :properties ((:text "diff --git \"a/docs/Guide \\316\\251.md\" \"b/docs/Guide \\316\\251.md\"\nindex 2ed70c2..d36a293 100644\n--- " :face diff-header :font-lock-face nil) (:text "\"a/docs/Guide \\316\\251.md\"" :face (diff-file-header diff-header) :font-lock-face nil) (:text "\11\n+++ " :face diff-header :font-lock-face nil) (:text "\"b/docs/Guide \\316\\251.md\"" :face (diff-file-header diff-header) :font-lock-face nil) (:text "\11\n" :face diff-header :font-lock-face nil) (:text "@@ -1,3 +1,3 @@" :face diff-hunk-header :font-lock-face nil) (:text "\n" :face nil :font-lock-face nil) (:text " # Guide\n \n" :face diff-context :font-lock-face nil) (:text "-" :face diff-indicator-removed :font-lock-face nil) (:text "Original Ω.\n" :face diff-removed :font-lock-face nil) (:text "+" :face diff-indicator-added :font-lock-face nil) (:text "Changed Ω.\n" :face diff-added :font-lock-face nil) (:text "diff --git a/src/app.el b/src/app.el\nindex d6acc5e..bf88545 100644\n--- " :face diff-header :font-lock-face nil) (:text "a/src/app.el" :face (diff-file-header diff-header) :font-lock-face nil) (:text "\n+++ " :face diff-header :font-lock-face nil) (:text "b/src/app.el" :face (diff-file-header diff-header) :font-lock-face nil) (:text "\n" :face diff-header :font-lock-face nil) (:text "@@ -1,4 +1,4 @@" :face diff-hunk-header :font-lock-face nil) (:text "\n" :face nil :font-lock-face nil) (:text " ;;; app.el\n (defun app ()\n" :face diff-context :font-lock-face nil) (:text "-" :face diff-indicator-removed :font-lock-face nil) (:text "  :source)\n" :face diff-removed :font-lock-face nil) (:text "+" :face diff-indicator-added :font-lock-face nil) (:text "  :changed)\n" :face diff-added :font-lock-face nil) (:text " ;; Guide.md\n" :face diff-context :font-lock-face nil))) :filtered (:reader ((:prompt "File pattern (e.g., \"regex !exclude1 exclude2\"): " :initial-input "" :require-match nil :category nil :candidates nil :final-input "src !docs")) :bytes "diff --git a/src/app.el b/src/app.el\nindex d6acc5e..bf88545 100644\n--- a/src/app.el\n+++ b/src/app.el\n@@ -1,4 +1,4 @@\n ;;; app.el\n (defun app ()\n-  :source)\n+  :changed)\n ;; Guide.md\n" :kill-head #("diff --git \"a/docs/Guide \\316\\251.md\" \"b/docs/Guide \\316\\251.md\"\nindex 2ed70c2..d36a293 100644\n--- \"a/docs/Guide \\316\\251.md\"\11\n+++ \"b/docs/Guide \\316\\251.md\"\11\n@@ -1,3 +1,3 @@\n # Guide\n \n-Original Ω.\n+Changed Ω.\n" 0 65 (face diff-header) 65 95 (face diff-header) 95 99 (face diff-header) 99 125 (face (diff-file-header diff-header)) 125 127 (face diff-header) 127 131 (face diff-header) 131 157 (face (diff-file-header diff-header)) 157 159 (face diff-header) 159 174 (face diff-hunk-header) 175 184 (face diff-context) 184 186 (face diff-context) 186 187 (face diff-indicator-removed) 187 199 (face diff-removed) 199 200 (face diff-indicator-added) 200 211 (face diff-added))) :jump (:trace ((:tool find :mode delegate :cwd "project" :argv ("." "(" "-iwholename" "*/.git" ")" "-prune" "-o" "-type" "f" "-iwholename" "*src/app.el*" "-print"))) :file "project/src/app.el" :line 1 :point 1 :text ";;; app.el" :bytes ";;; app.el\n(defun app ()\n  :changed)\n;; Guide.md\n") :apply (:immediate (:observation (:outcome (:value nil) :messages ("Hunk undone")) :hijack nil :buffer-bytes ";;; app.el\n(defun app ()\n  :source)\n;; Guide.md\n" :buffer-modified t :disk-bytes ";;; app.el\n(defun app ()\n  :changed)\n;; Guide.md\n" :git-diff "diff --git \"a/docs/Guide \\316\\251.md\" \"b/docs/Guide \\316\\251.md\"\nindex 2ed70c2..d36a293 100644\n--- \"a/docs/Guide \\316\\251.md\"\11\n+++ \"b/docs/Guide \\316\\251.md\"\11\n@@ -1,3 +1,3 @@\n # Guide\n \n-Original Ω.\n+Changed Ω.\ndiff --git a/src/app.el b/src/app.el\nindex d6acc5e..bf88545 100644\n--- a/src/app.el\n+++ b/src/app.el\n@@ -1,4 +1,4 @@\n ;;; app.el\n (defun app ()\n-  :source)\n+  :changed)\n ;; Guide.md\n") :saved (:hijack nil :buffer-bytes ";;; app.el\n(defun app ()\n  :source)\n;; Guide.md\n" :buffer-modified nil :disk-bytes ";;; app.el\n(defun app ()\n  :source)\n;; Guide.md\n" :remaining-git-diff "diff --git \"a/docs/Guide \\316\\251.md\" \"b/docs/Guide \\316\\251.md\"\nindex 2ed70c2..d36a293 100644\n--- \"a/docs/Guide \\316\\251.md\"\11\n+++ \"b/docs/Guide \\316\\251.md\"\11\n@@ -1,3 +1,3 @@\n # Guide\n \n-Original Ω.\n+Changed Ω.\n" :diff-buffer-bytes "diff --git a/src/app.el b/src/app.el\nindex d6acc5e..bf88545 100644\n--- a/src/app.el\n+++ b/src/app.el\n@@ -1,4 +1,4 @@\n ;;; app.el\n (defun app ()\n-  :source)\n+  :changed)\n ;; Guide.md\n") :malformed (:observation (:outcome (:signal error :data ("Can’t find the beginning of the hunk") :message "Can’t find the beginning of the hunk") :messages nil) :mode ffip-diff-mode :bytes "not a unified diff\n" :hijack-retained t :input-calls 0 :find-not-called t) :hijack-reset nil)) :cleanup (:new-buffers nil :new-processes nil :new-timers 0 :timer-details nil :owned-buffer-live nil :owned-process-live nil :owned-timer-live nil :root-exists nil :root-owned nil :current-buffer-restored t :window-restored t :transient-mark-restored t :dired-restored t :advice-count 1 :advice-restored t :hijack nil :unread-events nil :active-minibuffer nil :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        real_project_completion_filters_and_visits_unicode_file(),
        selected_line_bounded_history_and_frozen_resume(),
        directory_dired_and_relative_org_link(),
        at_point_resolution_and_reference_repair(),
        real_git_diff_review_filter_jump_and_apply(),
        external_find_failures_and_pure_lisp_recovery(),
    ]
}
