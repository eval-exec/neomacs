use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HTMLIZE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HTMLIZE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const HTMLIZE_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'htmlize)

(defface neomacs-htmlize-alert-face
  '((t :foreground "red" :weight bold))
  "Face used by the Htmlize parity publishing fixtures.")

(defconst neomacs-htmlize-test-face-overrides
  '(default (:foreground "#102030" :background "#f4f5f6")
    font-lock-builtin-face (:foreground "#2255aa")
    font-lock-comment-delimiter-face (:foreground "#667788" :slant italic)
    font-lock-comment-face (:foreground "#667788" :slant italic)
    font-lock-constant-face (:foreground "#8844aa")
    font-lock-doc-face (:foreground "#227755" :slant italic)
    font-lock-function-name-face (:foreground "#0055aa" :weight bold)
    font-lock-keyword-face (:foreground "#aa2255" :weight bold)
    font-lock-string-face (:foreground "#227755")
    font-lock-variable-name-face (:foreground "#996600")
    neomacs-htmlize-alert-face
    (:foreground "#cc1122" :background "#fff0d0" :weight bold
     :slant italic :underline t :strike-through t))
  "Display-independent face definitions for exact HTML snapshots.")

(defun neomacs-htmlize-test-root (name)
  "Create a deterministic sandbox directory for NAME and return it."
  (let ((root (expand-file-name
               (concat "htmlize-" name "/")
               (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-htmlize-test-write (path contents)
  "Write CONTENTS to PATH and return PATH."
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  path)

(defun neomacs-htmlize-test-text (buffer)
  "Return BUFFER's complete text without properties."
  (with-current-buffer buffer
    (buffer-substring-no-properties (point-min) (point-max))))

(defun neomacs-htmlize-test-render-current (&optional output-type)
  "Render current buffer using deterministic settings and OUTPUT-TYPE."
  (let ((htmlize-output-type (or output-type 'css))
        (htmlize-face-overrides neomacs-htmlize-test-face-overrides)
        (htmlize-css-name-prefix "neo-")
        (htmlize-use-rgb-txt nil)
        (htmlize-html-charset "utf-8")
        (htmlize-head-tags "    <meta name=\"generator-test\" content=\"neomacs\">\n")
        (htmlize-convert-nonascii-to-entities t))
    (htmlize-buffer)))

(defun neomacs-htmlize-test-render-text (text output-type decorate)
  "Render TEXT using OUTPUT-TYPE after calling DECORATE in its source buffer."
  (let ((source (generate-new-buffer "*htmlize-render-source*"))
        html)
    (unwind-protect
        (with-current-buffer source
          (insert text)
          (funcall decorate)
          (setq html (neomacs-htmlize-test-render-current output-type))
          (unwind-protect
              (neomacs-htmlize-test-text html)
            (kill-buffer html)))
      (when (buffer-live-p source)
        (kill-buffer source)))))

(defun neomacs-htmlize-test-temp-overlay-count ()
  "Count Htmlize's internal temporary overlays in the current buffer."
  (cl-count-if
   (lambda (overlay) (overlay-get overlay 'htmlize-tmp-overlay))
   (overlays-in (point-min) (point-max))))
"###;

fn htmlize_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HTMLIZE_MELPA_PIN, "htmlize.el")
        .expect("prepare revision-pinned Htmlize source below ./tmp")
        .with_prelude(HTMLIZE_TEST_PRELUDE)
        .with_timeout(HTMLIZE_TEST_TIMEOUT)
}

fn syntax_highlighted_elisp_buffer_becomes_a_complete_css_document_without_moving_source()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-htmlize-test-root "source"))
       (source (generate-new-buffer "release-plan.el"))
       html)
  (unwind-protect
      (with-current-buffer source
        (setq buffer-file-name (expand-file-name "release-plan.el" root)
              default-directory root)
        (insert
         ";;; release-plan.el --- deployment plan\n\n"
         "(defconst release-region 'us-east)\n\n"
         "(defun deploy-release (release)\n"
         "  \"Deploy RELEASE to the configured region.\"\n"
         "  ;; Keep operators informed.\n"
         "  (message \"Deploying %s to %s\" release release-region))\n")
        (emacs-lisp-mode)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "deploy-release")
        (let ((source-state
               (list :buffer (buffer-name)
                     :point (point)
                     :mode major-mode
                     :modified (buffer-modified-p)
                     :text (buffer-substring-no-properties
                            (point-min) (point-max)))))
          (setq html (neomacs-htmlize-test-render-current 'css))
          (list :source-before source-state
                :source-after
                (list :selected (eq (current-buffer) source)
                      :point (point)
                      :mode major-mode
                      :modified (buffer-modified-p))
                :html
                (with-current-buffer html
                  (list :buffer (buffer-name)
                        :mode major-mode
                        :default-directory
                        (file-name-nondirectory
                         (directory-file-name default-directory))
                        :places
                        (mapcar
                         (lambda (key)
                           (list key
                                 (marker-position
                                  (plist-get htmlize-buffer-places key))))
                         '(head-start head-end body-start content-start
                           content-end body-end))
                        :text
                        (buffer-substring-no-properties
                         (point-min) (point-max)))))))
    (when (buffer-live-p html) (kill-buffer html))
    (when (buffer-live-p source) (kill-buffer source))
    (when (file-exists-p root) (delete-directory root t))))
"###;
    let expected = expect![[
        r####"OK (:source-before (:buffer "release-plan.el" :point 99 :mode emacs-lisp-mode :modified t :text ";;; release-plan.el --- deployment plan\n\n(defconst release-region 'us-east)\n\n(defun deploy-release (release)\n  \"Deploy RELEASE to the configured region.\"\n  ;; Keep operators informed.\n  (message \"Deploying %s to %s\" release release-region))\n") :source-after (:selected t :point 99 :mode emacs-lisp-mode :modified t) :html (:buffer "release-plan.el.html" :mode fundamental-mode :default-directory "htmlize-source" :places ((head-start 107) (head-end 1416) (body-start 1419) (content-start 1430) (content-end 2040) (body-end 2050)) :text "<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01//EN\">\n<!-- Created by htmlize-1.59 in css mode. -->\n<html>\n  <head>\n    <title>release-plan.el</title>\n    <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">\n    <meta name=\"generator-test\" content=\"neomacs\">\n    <style type=\"text/css\">\n    <!--\n      body {\n        color: #102030;\n        background-color: #f4f5f6;\n      }\n      .neo-comment {\n        /* font-lock-comment-face */\n        color: #667788;\n        font-style: italic;\n      }\n      .neo-comment-delimiter {\n        /* font-lock-comment-delimiter-face */\n        color: #667788;\n        font-style: italic;\n      }\n      .neo-doc {\n        /* font-lock-doc-face */\n        color: #227755;\n        font-style: italic;\n      }\n      .neo-function-name {\n        /* font-lock-function-name-face */\n        color: #0055aa;\n        font-weight: bold;\n      }\n      .neo-keyword {\n        /* font-lock-keyword-face */\n        color: #aa2255;\n        font-weight: bold;\n      }\n      .neo-string {\n        /* font-lock-string-face */\n        color: #227755;\n      }\n      .neo-variable-name {\n        /* font-lock-variable-name-face */\n        color: #996600;\n      }\n\n      a {\n        color: inherit;\n        background-color: inherit;\n        font: inherit;\n        text-decoration: inherit;\n      }\n      a:hover {\n        text-decoration: underline;\n      }\n    -->\n    </style>\n  </head>\n  <body>\n    <pre>\n<span class=\"neo-comment-delimiter\">;;; </span><span class=\"neo-comment\">release-plan.el --- deployment plan\n</span>\n(<span class=\"neo-keyword\">defconst</span> <span class=\"neo-variable-name\">release-region</span> 'us-east)\n\n(<span class=\"neo-keyword\">defun</span> <span class=\"neo-function-name\">deploy-release</span> (release)\n  <span class=\"neo-doc\">\"Deploy RELEASE to the configured region.\"</span>\n  <span class=\"neo-comment-delimiter\">;; </span><span class=\"neo-comment\">Keep operators informed.\n</span>  (message <span class=\"neo-string\">\"Deploying %s to %s\"</span> release release-region))\n</pre>\n  </body>\n</html>\n"))"####
    ]];
    ParityBatchCase::value(
        "syntax_highlighted_elisp_buffer_becomes_a_complete_css_document_without_moving_source",
        elisp_form,
        expected,
    )
}

fn screenshot_command_copies_a_rich_visible_region_with_tabs_links_overlays_and_ellipsis()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((source (generate-new-buffer "*htmlize-incident-note*"))
      (kill-ring nil)
      (kill-ring-yank-pointer nil)
      (interprogram-cut-function nil)
      (htmlize-face-overrides neomacs-htmlize-test-face-overrides)
      (htmlize-css-name-prefix "neo-")
      (htmlize-use-rgb-txt nil)
      (htmlize-convert-nonascii-to-entities t)
      (htmlize-generate-hyperlinks nil)
      (htmlize-untabify t)
      (htmlize-pre-style t))
  (unwind-protect
      (progn
        (set-window-buffer (selected-window) source)
        (set-buffer source)
        (insert "STATUS\t<api>& café\nTOKEN=secret-value\nRunbook\n")
        (add-text-properties
         (point-min) (+ (point-min) 6)
         '(face neomacs-htmlize-alert-face))
        (goto-char (point-min))
        (search-forward "<api>")
        (let ((overlay (make-overlay (match-beginning 0) (match-end 0))))
          (overlay-put overlay 'face
                       '(:foreground "#ffffff" :background "#224466"))
          (overlay-put overlay 'before-string "[")
          (overlay-put overlay 'after-string "]"))
        (goto-char (point-min))
        (search-forward "secret-value")
        (add-text-properties (match-beginning 0) (match-end 0)
                             '(invisible secret))
        (setq buffer-invisibility-spec '((secret . t)))
        (goto-char (point-min))
        (search-forward "Runbook")
        (add-text-properties
         (match-beginning 0) (match-end 0)
         '(htmlize-link (:uri "https://ops.example/run?team=core&view=full")))
        (local-set-key (kbd "C-c h") #'htmlize-region-save-screenshot)
        (goto-char (point-min))
        (push-mark (point-max) t t)
        (activate-mark)
        (execute-kbd-macro (kbd "C-c h"))
        (list :source (buffer-substring-no-properties (point-min) (point-max))
              :clipboard (car kill-ring)
              :region-active mark-active
              :point (point)
              :persistent-overlays (length (overlays-in (point-min) (point-max)))
              :temporary-overlays
              (neomacs-htmlize-test-temp-overlay-count)))
    (when (buffer-live-p source) (kill-buffer source))))
"###;
    let expected = expect![[
        r####"OK (:source "STATUS\11<api>& café\nTOKEN=secret-value\nRunbook\n" :clipboard "<pre style=\"color: #102030; background-color: #f4f5f6;\">\n<span style=\"color: #cc1122; background-color: #fff0d0; font-weight: bold; font-style: italic; text-decoration: underline; text-decoration: line-through;\">STATUS</span>  <span style=\"color: #ffffff; background-color: #224466;\">[&lt;api&gt;]</span>&amp; caf&#233;\nTOKEN=...\n<a href=\"https://ops.example/run?team=core&amp;view=full\">Runbook</a>\n</pre>" :region-active nil :point 1 :persistent-overlays 1 :temporary-overlays 0)"####
    ]];
    ParityBatchCase::value(
        "screenshot_command_copies_a_rich_visible_region_with_tabs_links_overlays_and_ellipsis",
        elisp_form,
        expected,
    )
}

fn css_inline_css_and_legacy_font_modes_render_the_same_decorated_alert() -> ParityBatchCase {
    let elisp_form = r###"
(let ((text "ALERT: deploy <api> & verify\n"))
  (mapcar
   (lambda (output-type)
     (list
      output-type
      (neomacs-htmlize-test-render-text
       text output-type
       (lambda ()
         (add-text-properties
          (point-min) (+ (point-min) 5)
          '(face neomacs-htmlize-alert-face))
         (goto-char (point-min))
         (search-forward "api")
         (add-text-properties
          (match-beginning 0) (match-end 0)
          '(face (:foreground "#ffffff" :background "#225588"
                  :weight bold :underline t)))))))
   '(css inline-css font)))
"###;
    let expected = expect![[
        r####"OK ((css "<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01//EN\">\n<!-- Created by htmlize-1.59 in css mode. -->\n<html>\n  <head>\n    <title>*htmlize-render-source*</title>\n    <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">\n    <meta name=\"generator-test\" content=\"neomacs\">\n    <style type=\"text/css\">\n    <!--\n      body {\n        color: #102030;\n        background-color: #f4f5f6;\n      }\n      .custom {\n        /* (:foreground \"#ffffff\" :background \"#225588\" :weight bold :underline t) */\n        color: #ffffff;\n        background-color: #225588;\n        font-weight: bold;\n        text-decoration: underline;\n      }\n      .neo-neomacs-htmlize-alert {\n        /* neomacs-htmlize-alert-face */\n        color: #cc1122;\n        background-color: #fff0d0;\n        font-weight: bold;\n        font-style: italic;\n        text-decoration: underline;\n        text-decoration: line-through;\n      }\n\n      a {\n        color: inherit;\n        background-color: inherit;\n        font: inherit;\n        text-decoration: inherit;\n      }\n      a:hover {\n        text-decoration: underline;\n      }\n    -->\n    </style>\n  </head>\n  <body>\n    <pre>\n<span class=\"neo-neomacs-htmlize-alert\">ALERT</span>: deploy &lt;<span class=\"custom\">api</span>&gt; &amp; verify\n</pre>\n  </body>\n</html>\n") (inline-css "<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01//EN\">\n<!-- Created by htmlize-1.59 in inline-css mode. -->\n<html>\n  <head>\n    <title>*htmlize-render-source*</title>\n    <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">\n    <meta name=\"generator-test\" content=\"neomacs\">\n  </head>\n  <body style=\"color: #102030; background-color: #f4f5f6;\">\n    <pre>\n<span style=\"color: #cc1122; background-color: #fff0d0; font-weight: bold; font-style: italic; text-decoration: underline; text-decoration: line-through;\">ALERT</span>: deploy &lt;<span style=\"color: #ffffff; background-color: #225588; font-weight: bold; text-decoration: underline;\">api</span>&gt; &amp; verify\n</pre>\n  </body>\n</html>\n") (font "<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01//EN\">\n<!-- Created by htmlize-1.59 in font mode. -->\n<html>\n  <head>\n    <title>*htmlize-render-source*</title>\n    <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">\n    <meta name=\"generator-test\" content=\"neomacs\">\n  </head>\n  <body text=\"#102030\" bgcolor=\"#f4f5f6\">\n    <pre>\n<font color=\"#cc1122\"><b><i><u><strike>ALERT</strike></u></i></b></font>: deploy &lt;<font color=\"#ffffff\"><b><u>api</u></b></font>&gt; &amp; verify\n</pre>\n  </body>\n</html>\n"))"####
    ]];
    ParityBatchCase::value(
        "css_inline_css_and_legacy_font_modes_render_the_same_decorated_alert",
        elisp_form,
        expected,
    )
}

fn publishing_log_generates_safe_links_inline_svg_form_feed_and_defanged_local_variables()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((source (generate-new-buffer "operations.log"))
      html)
  (unwind-protect
      (with-current-buffer source
        (insert
         "Contact <ops@example.com>\n"
         "Runbook <URL:https://ops.example/run?env=prod&ticket=R-42>\n"
         "Dashboard [health]\n"
         "\f\n"
         "Local Variables:\n")
        (goto-char (point-min))
        (search-forward "Runbook")
        (add-text-properties
         (match-beginning 0) (match-end 0)
         '(htmlize-link (:uri "https://ops.example/a?x=1&label=\"blue\"")))
        (goto-char (point-min))
        (search-forward "[health]")
        (add-text-properties
         (match-beginning 0) (match-end 0)
         '(display (image :type svg
                          :data "<svg xmlns='http://www.w3.org/2000/svg'><circle/></svg>")))
        (let ((htmlize-use-images t)
              (htmlize-force-inline-images nil)
              (htmlize-generate-hyperlinks t)
              (htmlize-replace-form-feeds t))
          (setq html (neomacs-htmlize-test-render-current 'inline-css)))
        (let ((source-after
               (list :text (buffer-substring-no-properties
                            (point-min) (point-max))
                     :temporary-overlays
                     (neomacs-htmlize-test-temp-overlay-count)
                     :all-overlays
                     (length (overlays-in (point-min) (point-max))))))
          (list :source source-after
                :html (neomacs-htmlize-test-text html))))
    (when (buffer-live-p html) (kill-buffer html))
    (when (buffer-live-p source) (kill-buffer source))))
"###;
    let expected = expect![[
        r####"OK (:source (:text "Contact <ops@example.com>\nRunbook <URL:https://ops.example/run?env=prod&ticket=R-42>\nDashboard [health]\n\f\nLocal Variables:\n" :temporary-overlays 0 :all-overlays 0) :html "<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01//EN\">\n<!-- Created by htmlize-1.59 in inline-css mode. -->\n<html>\n  <head>\n    <title>operations.log</title>\n    <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">\n    <meta name=\"generator-test\" content=\"neomacs\">\n  </head>\n  <body style=\"color: #102030; background-color: #f4f5f6;\">\n    <pre>\nContact <a href=\"mailto:ops%40example.com\">&lt;ops@example.com&gt;</a>\n<a href=\"https://ops.example/a?x=1&amp;label=&quot;blue&quot;\">Runbook</a> <a href=\"https://ops.example/run?env=prod&amp;ticket=R-42\">&lt;URL:https://ops.example/run?env=prod&amp;ticket=R-42&gt;</a>\nDashboard <img src=\"data:image/svg+xml;base64,PHN2ZyB4bWxucz0naHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmcnPjxjaXJjbGUvPjwvc3ZnPg==\" alt=\"[health]\" />\n<hr />\nLocal Variables&#58;\n</pre>\n  </body>\n</html>\n")"####
    ]];
    ParityBatchCase::value(
        "publishing_log_generates_safe_links_inline_svg_form_feed_and_defanged_local_variables",
        elisp_form,
        expected,
    )
}

fn batch_file_conversion_uses_disk_contents_preserves_visited_buffers_and_runs_all_hooks()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((root (neomacs-htmlize-test-root "files"))
       (target (expand-file-name "published/" root))
       (elisp-file (expand-file-name "service.el" root))
       (text-file (expand-file-name "operators.txt" root))
       (visited nil)
       (hook-log nil)
       (htmlize-face-overrides neomacs-htmlize-test-face-overrides)
       (htmlize-css-name-prefix "neo-")
       (htmlize-use-rgb-txt nil)
       (htmlize-html-charset "utf-8")
       (htmlize-convert-nonascii-to-entities t)
       (htmlize-before-hook
        (list (lambda ()
                (push (list :before
                            (file-name-nondirectory buffer-file-name)
                            major-mode)
                      hook-log))))
       (htmlize-after-hook
        (list (lambda ()
                (push (list :after (buffer-name) major-mode) hook-log)
                (goto-char (point-max))
                (insert "<!-- after-hook -->\n"))))
       (htmlize-file-hook
        (list (lambda ()
                (push (list :file (buffer-name) major-mode) hook-log)
                (goto-char (point-max))
                (insert "<!-- file-hook -->\n")))))
  (unwind-protect
      (progn
        (make-directory target t)
        (neomacs-htmlize-test-write
         elisp-file
         ";;; service.el --- disk source\n\n(defun service-ready-p ()\n  \"Return non-nil when the service is ready.\"\n  t)\n")
        (neomacs-htmlize-test-write
         text-file
         "Operators: Ana & 李\nEscalate <ops@example.com>\n")
        (setq visited (find-file-noselect elisp-file))
        (with-current-buffer visited
          (goto-char (point-max))
          (insert ";; UNSAVED LOCAL NOTE\n"))
        (htmlize-many-files (list elisp-file text-file) target)
        (let* ((outputs
                (sort (directory-files target nil "\\.html\\'") #'string<))
               (published
                (mapcar
                 (lambda (name)
                   (list name
                         (with-temp-buffer
                           (insert-file-contents (expand-file-name name target))
                           (buffer-string))))
                 outputs)))
          (list :outputs published
                :visited
                (with-current-buffer visited
                  (list :modified (buffer-modified-p)
                        :contains-unsaved
                        (and (save-excursion
                               (goto-char (point-min))
                               (search-forward "UNSAVED LOCAL NOTE" nil t))
                             t)
                        :mode major-mode))
                :hooks (nreverse hook-log))))
    (when (buffer-live-p visited)
      (with-current-buffer visited (set-buffer-modified-p nil))
      (kill-buffer visited))
    (when (file-exists-p root) (delete-directory root t))))
"###;
    let expected = expect![[
        r####"OK (:outputs (("operators.txt.html" "<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01//EN\">\n<!-- Created by htmlize-1.59 in css mode. -->\n<html>\n  <head>\n    <title>operators.txt</title>\n    <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">\n    <style type=\"text/css\">\n    <!--\n      body {\n        color: #102030;\n        background-color: #f4f5f6;\n      }\n\n      a {\n        color: inherit;\n        background-color: inherit;\n        font: inherit;\n        text-decoration: inherit;\n      }\n      a:hover {\n        text-decoration: underline;\n      }\n    -->\n    </style>\n  </head>\n  <body>\n    <pre>\nOperators: Ana &amp; &#26446;\nEscalate <a href=\"mailto:ops%40example.com\">&lt;ops@example.com&gt;</a>\n</pre>\n  </body>\n</html>\n<!-- after-hook -->\n<!-- file-hook -->\n") ("service.el.html" "<!DOCTYPE html PUBLIC \"-//W3C//DTD HTML 4.01//EN\">\n<!-- Created by htmlize-1.59 in css mode. -->\n<html>\n  <head>\n    <title>service.el</title>\n    <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\">\n    <style type=\"text/css\">\n    <!--\n      body {\n        color: #102030;\n        background-color: #f4f5f6;\n      }\n      .neo-comment {\n        /* font-lock-comment-face */\n        color: #667788;\n        font-style: italic;\n      }\n      .neo-comment-delimiter {\n        /* font-lock-comment-delimiter-face */\n        color: #667788;\n        font-style: italic;\n      }\n      .neo-doc {\n        /* font-lock-doc-face */\n        color: #227755;\n        font-style: italic;\n      }\n      .neo-function-name {\n        /* font-lock-function-name-face */\n        color: #0055aa;\n        font-weight: bold;\n      }\n      .neo-keyword {\n        /* font-lock-keyword-face */\n        color: #aa2255;\n        font-weight: bold;\n      }\n\n      a {\n        color: inherit;\n        background-color: inherit;\n        font: inherit;\n        text-decoration: inherit;\n      }\n      a:hover {\n        text-decoration: underline;\n      }\n    -->\n    </style>\n  </head>\n  <body>\n    <pre>\n<span class=\"neo-comment-delimiter\">;;; </span><span class=\"neo-comment\">service.el --- disk source\n</span>\n(<span class=\"neo-keyword\">defun</span> <span class=\"neo-function-name\">service-ready-p</span> ()\n  <span class=\"neo-doc\">\"Return non-nil when the service is ready.\"</span>\n  t)\n</pre>\n  </body>\n</html>\n<!-- after-hook -->\n<!-- file-hook -->\n")) :visited (:modified t :contains-unsaved t :mode emacs-lisp-mode) :hooks ((:before "service.el" emacs-lisp-mode) (:after "service.el.html" fundamental-mode) (:file "service.el.html" fundamental-mode) (:before "operators.txt" text-mode) (:after "operators.txt.html" fundamental-mode) (:file "operators.txt.html" fundamental-mode)))"####
    ]];
    ParityBatchCase::value(
        "batch_file_conversion_uses_disk_contents_preserves_visited_buffers_and_runs_all_hooks",
        elisp_form,
        expected,
    )
}

#[test]
fn htmlize_package_batch() {
    assert_oracle_batch_cases(
        htmlize_oracle(),
        "htmlize-package-batch",
        "Htmlize",
        &[
            syntax_highlighted_elisp_buffer_becomes_a_complete_css_document_without_moving_source(),
            screenshot_command_copies_a_rich_visible_region_with_tabs_links_overlays_and_ellipsis(),
            css_inline_css_and_legacy_font_modes_render_the_same_decorated_alert(),
            publishing_log_generates_safe_links_inline_svg_form_feed_and_defanged_local_variables(),
            batch_file_conversion_uses_disk_contents_preserves_visited_buffers_and_runs_all_hooks(),
        ],
    );
}
