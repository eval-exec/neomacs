use expect_test::expect;

use super::ParityBatchCase;

fn opens_and_fontifies_a_real_dashboard_configuration() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "yuck-mode-open"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (path (expand-file-name "dashboard.yuck" sandbox))
       buffer result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory sandbox t)
        (with-temp-file path
          (insert
           ";; Production status panel\n"
           "(defvar poll-interval \"5s\")\n"
           "(defpoll cpu :interval \"1s\" `scripts/cpu`)\n"
           "(defwidget status-card [metric]\n"
           "  (box :class \"status-card\" :orientation \"h\"\n"
           "    (label :text \"${metric.value}%\")\n"
           "    (progress :value {metric.value / 100})))\n"
           "(defwindow dashboard\n"
           "  :monitor 0\n"
           "  (geometry :x \"20px\" :y \"20px\"\n"
           "    (centerbox\n"
           "      (button :onclick \"eww close dashboard\" \"Close\")\n"
           "      (literal :content metric))))\n"))
        (setq buffer (find-file-noselect path))
        (with-current-buffer buffer
          (setq result
                (list
                 :file buffer-file-name
                 :mode major-mode
                 :mode-name mode-name
                 :derived (derived-mode-p 'prog-mode)
                 :indent indent-line-function
                 :comments (list comment-start comment-end comment-padding)
                 :font-lock (copy-tree font-lock-defaults)
                 :faces (neomacs-melpa-yuck-mode--face-runs)
                 :syntax
                 (list
                  (neomacs-melpa-yuck-mode--syntax-state-at
                   "Production" 2)
                  (neomacs-melpa-yuck-mode--syntax-state-at "5s" 0)
                  (neomacs-melpa-yuck-mode--syntax-state-at
                   "scripts/cpu" 2)
                  (neomacs-melpa-yuck-mode--syntax-state-at
                   "metric.value" 2))
                 :auto-mode
                 (cdr (assoc "\\.yuck\\'" auto-mode-alist))
                 :modified (buffer-modified-p)))
          result))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r####"OK (:file "[ORACLE-SANDBOX]/yuck-mode-open/dashboard.yuck" :mode yuck-mode :mode-name "Yuck" :derived prog-mode :indent lisp-indent-line :comments (";; " "" "") :font-lock (yuck-font-lock-keywords) :faces ((";; " font-lock-comment-delimiter-face 1 4) ("Production status panel\n" font-lock-comment-face 4 28) ("defvar" font-lock-keyword-face 29 35) ("\"5s\"" font-lock-string-face 50 54) ("defpoll" font-lock-keyword-face 57 64) (":interval" font-lock-builtin-face 69 78) ("\"1s\"" font-lock-string-face 79 83) ("`scripts/cpu`" font-lock-string-face 84 97) ("defwidget" font-lock-keyword-face 100 109) ("box" font-lock-type-face 134 137) (":class" font-lock-builtin-face 138 144) ("\"status-card\"" font-lock-string-face 145 158) (":orientation" font-lock-builtin-face 159 171) ("\"h\"" font-lock-string-face 172 175) ("label" font-lock-type-face 181 186) (":text" font-lock-builtin-face 187 192) ("\"${metric.value}%\"" font-lock-string-face 193 211) ("progress" font-lock-type-face 218 226) (":value" font-lock-builtin-face 227 233) ("defwindow" font-lock-keyword-face 259 268) (":monitor" font-lock-builtin-face 281 289) ("geometry" font-lock-type-face 295 303) (":x" font-lock-builtin-face 304 306) ("\"20px\"" font-lock-string-face 307 313) (":y" font-lock-builtin-face 314 316) ("\"20px\"" font-lock-string-face 317 323) ("centerbox" font-lock-type-face 329 338) ("button" font-lock-type-face 346 352) (":onclick" font-lock-builtin-face 353 361) ("\"eww close dashboard\"" font-lock-string-face 362 383) ("\"Close\"" font-lock-string-face 384 391) ("literal" font-lock-type-face 400 407) (":content" font-lock-builtin-face 408 416)) :syntax (("Production" 6 0 nil t 1) ("5s" 51 1 34 nil 50) ("scripts/cpu" 87 1 96 nil 84) ("metric.value" 198 3 34 nil 193)) :auto-mode yuck-mode :modified nil)"####
    ]];
    ParityBatchCase::value(
        "opens_and_fontifies_a_real_dashboard_configuration",
        elisp_form,
        expect,
    )
}

fn fontifies_the_complete_component_gallery_and_boundary_contexts() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   ";; Documentation mentions defwindow, box, and include without code.\n"
   "(include \"./shared-theme.yuck\")\n"
   "(deflisten notifications `scripts/notifications`)\n"
   "(defvar defvar-extra \"box include defwindow\")\n"
   "(defvar defvariable \"ordinary identifier\")\n"
   "(defwidget component-gallery [choices]\n"
   "  (scroll :hscroll false\n"
   "    (expander :name \"Controls\"\n"
   "      (revealer :reveal true\n"
   "        (my-box :class \"custom-container\"\n"
   "          (combo-box-text :items choices)\n"
   "          (checkbox :checked true)\n"
   "          (color-button :use-alpha true)\n"
   "          (color-chooser :rgba \"#ffaa00ff\")\n"
   "          (scale :value 42 :min 0 :max 100)\n"
   "          (input :value \"operator note\")\n"
   "          (calendar :day 3 :month 8 :year 2026)\n"
   "          (transform :rotate 15\n"
   "            (circular-progress :value 75))\n"
   "          (graph :value \"1,2,3,5,8\"))))))\n")
  (yuck-mode)
  (font-lock-ensure)
  (list
   :source (buffer-substring-no-properties (point-min) (point-max))
   :positive
   (mapcar
    #'neomacs-melpa-yuck-mode--face-segments
    '("include \"./shared"
      "deflisten notifications"
      "combo-box-text"
      "expander" "revealer" "checkbox" "color-button" "color-chooser"
      "scale" "input" "scroll" "calendar" "transform"
      "circular-progress" "graph"))
   :contexts
   (mapcar
    #'neomacs-melpa-yuck-mode--face-segments
    '("defvar-extra"
      "defvariable"
      "my-box"
      "Documentation mentions defwindow"
      "box include defwindow"))))
"####;
    let expect = expect![[
        r####"OK (:source ";; Documentation mentions defwindow, box, and include without code.\n(include \"./shared-theme.yuck\")\n(deflisten notifications `scripts/notifications`)\n(defvar defvar-extra \"box include defwindow\")\n(defvar defvariable \"ordinary identifier\")\n(defwidget component-gallery [choices]\n  (scroll :hscroll false\n    (expander :name \"Controls\"\n      (revealer :reveal true\n        (my-box :class \"custom-container\"\n          (combo-box-text :items choices)\n          (checkbox :checked true)\n          (color-button :use-alpha true)\n          (color-chooser :rgba \"#ffaa00ff\")\n          (scale :value 42 :min 0 :max 100)\n          (input :value \"operator note\")\n          (calendar :day 3 :month 8 :year 2026)\n          (transform :rotate 15\n            (circular-progress :value 75))\n          (graph :value \"1,2,3,5,8\"))))))\n" :positive (("include \"./shared" 70 87 (("include" font-lock-keyword-face 0 7) (" " nil 7 8) ("\"./shared" font-lock-string-face 8 17))) ("deflisten notifications" 102 125 (("deflisten" font-lock-keyword-face 0 9) (" notifications" nil 9 23))) ("combo-box-text" 417 431 (("combo-box-text" font-lock-type-face 0 14))) ("expander" 309 317 (("expander" font-lock-type-face 0 8))) ("revealer" 342 350 (("revealer" font-lock-type-face 0 8))) ("checkbox" 459 467 (("checkbox" font-lock-type-face 0 8))) ("color-button" 494 506 (("color-button" font-lock-type-face 0 12))) ("color-chooser" 535 548 (("color-chooser" font-lock-type-face 0 13))) ("scale" 579 584 (("scale" font-lock-type-face 0 5))) ("input" 623 628 (("input" font-lock-type-face 0 5))) ("scroll" 282 288 (("scroll" font-lock-type-face 0 6))) ("calendar" 664 672 (("calendar" font-lock-type-face 0 8))) ("transform" 712 721 (("transform" font-lock-type-face 0 9))) ("circular-progress" 746 763 (("circular-progress" font-lock-type-face 0 17))) ("graph" 787 792 (("graph" font-lock-type-face 0 5)))) :contexts (("defvar-extra" 159 171 (("defvar" font-lock-keyword-face 0 6) ("-extra" nil 6 12))) ("defvariable" 205 216 (("defvariable" nil 0 11))) ("my-box" 373 379 (("my-box" nil 0 6))) ("Documentation mentions defwindow" 4 36 (("Documentation mentions defwindow" font-lock-comment-face 0 32))) ("box include defwindow" 173 194 (("box include defwindow" font-lock-string-face 0 21)))))"####
    ]];
    ParityBatchCase::value(
        "fontifies_the_complete_component_gallery_and_boundary_contexts",
        elisp_form,
        expect,
    )
}

fn indents_a_nested_widget_and_preserves_strings_and_comments() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "(defwidget release-card [releases]\n"
   "(box :class \"release-grid\"\n"
   "(for release in releases\n"
   "(eventbox :onclick \"open --release '${release.id}'\"\n"
   ";; Keep the interpolation untouched.\n"
   "(overlay\n"
   "(image :path {release.icon})\n"
   "(label :text \"${release.title}\"))))))\n"
   "\n"
   "(defwindow releases\n"
   "(geometry :x \"2%\" :y \"4%\"\n"
   "(release-card :releases releases)))\n")
  (yuck-mode)
  (let ((before
         (buffer-substring-no-properties (point-min) (point-max))))
    (indent-region (point-min) (point-max))
    (let ((after-first
           (buffer-substring-no-properties (point-min) (point-max)))
          (indents (neomacs-melpa-yuck-mode--line-indents)))
      (indent-region (point-min) (point-max))
      (list
       :before before
       :after (buffer-substring-no-properties (point-min) (point-max))
       :idempotent
       (equal
        after-first
        (buffer-substring-no-properties (point-min) (point-max)))
       :indents indents
       :mode major-mode
       :indent-function indent-line-function
       :balanced (condition-case nil
                     (progn (check-parens) t)
                   (error nil))))))
"####;
    let expect = expect![[
        r####"OK (:before "(defwidget release-card [releases]\n(box :class \"release-grid\"\n(for release in releases\n(eventbox :onclick \"open --release '${release.id}'\"\n;; Keep the interpolation untouched.\n(overlay\n(image :path {release.icon})\n(label :text \"${release.title}\"))))))\n\n(defwindow releases\n(geometry :x \"2%\" :y \"4%\"\n(release-card :releases releases)))\n" :after "(defwidget release-card [releases]\n\11   (box :class \"release-grid\"\n\11\11(for release in releases\n\11\11     (eventbox :onclick \"open --release '${release.id}'\"\n\11\11\11       ;; Keep the interpolation untouched.\n\11\11\11       (overlay\n\11\11\11\11(image :path {release.icon})\n\11\11\11\11(label :text \"${release.title}\"))))))\n\n(defwindow releases\n\11   (geometry :x \"2%\" :y \"4%\"\n\11\11     (release-card :releases releases)))\n" :idempotent t :indents ((1 0 "(defwidget release-card [releases]") (2 11 "\11   (box :class \"release-grid\"") (3 16 "\11\11(for release in releases") (4 21 "\11\11     (eventbox :onclick \"open --release '${release.id}'\"") (5 31 "\11\11\11       ;; Keep the interpolation untouched.") (6 31 "\11\11\11       (overlay") (7 32 "\11\11\11\11(image :path {release.icon})") (8 32 "\11\11\11\11(label :text \"${release.title}\"))))))") (9 0 "") (10 0 "(defwindow releases") (11 11 "\11   (geometry :x \"2%\" :y \"4%\"") (12 21 "\11\11     (release-card :releases releases)))")) :mode yuck-mode :indent-function lisp-indent-line :balanced t)"####
    ]];
    ParityBatchCase::value(
        "indents_a_nested_widget_and_preserves_strings_and_comments",
        elisp_form,
        expect,
    )
}

fn comments_and_uncomments_a_selected_configuration_block() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "(defvar weather \"sunny\")\n"
   "(defvar alert \"none\")\n"
   "(label :text \"${weather}: ${alert}\")\n")
  (yuck-mode)
  (let (commented syntax-in-comment after-uncomment inline-comment)
    (goto-char (point-min))
    (forward-line 1)
    (let ((start (point)))
      (forward-line 2)
      (comment-region start (point))
      (setq commented
            (buffer-substring-no-properties (point-min) (point-max))
            syntax-in-comment
            (neomacs-melpa-yuck-mode--syntax-state-at "defvar alert" 2))
      (uncomment-region start (point)))
    (setq after-uncomment
          (buffer-substring-no-properties (point-min) (point-max)))
    (goto-char (point-min))
    (end-of-line)
    (comment-dwim nil)
    (insert "primary source")
    (setq inline-comment
          (list
           (buffer-substring-no-properties (point-min) (point-max))
           (neomacs-melpa-yuck-mode--syntax-state-at "primary source" 2)))
    (list
     :comment-settings (list comment-start comment-end comment-padding)
     :commented commented
     :syntax-in-comment syntax-in-comment
     :uncommented after-uncomment
     :inline inline-comment)))
"####;
    let expect = expect![[
        r####"OK (:comment-settings (";; " "" "") :commented "(defvar weather \"sunny\")\n;; (defvar alert \"none\")\n;; (label :text \"${weather}: ${alert}\")\n" :syntax-in-comment ("defvar alert" 32 0 nil t 26) :uncommented "(defvar weather \"sunny\")\n(defvar alert \"none\")\n(label :text \"${weather}: ${alert}\")\n" :inline ("(defvar weather \"sunny\") ;; primary source\n(defvar alert \"none\")\n(label :text \"${weather}: ${alert}\")\n" ("primary source" 31 0 nil t 26)))"####
    ]];
    ParityBatchCase::value(
        "comments_and_uncomments_a_selected_configuration_block",
        elisp_form,
        expect,
    )
}

fn recovers_an_incomplete_widget_after_exact_syntax_and_indent_diagnostics() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   "(defwidget release-card [release]\n"
   "(box :class \"release-card\"\n"
   "(label :text \"Deploying ${release.name}")
  (yuck-mode)
  (indent-region (point-min) (point-max))
  (font-lock-ensure)
  (let* ((broken-text
          (buffer-substring-no-properties (point-min) (point-max)))
         (broken-indents (neomacs-melpa-yuck-mode--line-indents))
         (broken-faces (neomacs-melpa-yuck-mode--face-runs))
         (state (syntax-ppss (point-max)))
         (broken-syntax
          (list (nth 0 state) (nth 3 state) (nth 4 state) (nth 8 state)))
         (broken-check
          (condition-case caught
              (list 'ok (check-parens))
            (error (list 'error (car caught) (cdr caught)))))
         (broken-point (point)))
    ;; This is the ordinary recovery while typing: close the string and forms,
    ;; then reindent and re-fontify the edited buffer.
    (goto-char (point-max))
    (insert "\")))\n")
    (indent-region (point-min) (point-max))
    (font-lock-flush)
    (font-lock-ensure)
    (let ((repaired-check
           (condition-case caught
               (progn (check-parens) '(ok))
             (error (list 'error (car caught) (cdr caught))))))
      (list
       :broken
       (list
        :text broken-text
        :indents broken-indents
        :faces broken-faces
        :syntax broken-syntax
        :check broken-check
        :point broken-point)
       :repaired
       (list
        :text (buffer-substring-no-properties (point-min) (point-max))
        :indents (neomacs-melpa-yuck-mode--line-indents)
        :faces (neomacs-melpa-yuck-mode--face-runs)
        :check repaired-check
        :point (point)
        :modified (buffer-modified-p))))))
"####;
    let expect = expect![[
        r####"OK (:broken (:text "(defwidget release-card [release]\n\11   (box :class \"release-card\"\n\11\11(label :text \"Deploying ${release.name}" :indents ((1 0 "(defwidget release-card [release]") (2 11 "\11   (box :class \"release-card\"") (3 16 "\11\11(label :text \"Deploying ${release.name}")) :faces (("defwidget" font-lock-keyword-face 2 11) ("box" font-lock-type-face 40 43) (":class" font-lock-builtin-face 44 50) ("\"release-card\"" font-lock-string-face 51 65) ("label" font-lock-type-face 69 74) (":text" font-lock-builtin-face 75 80) ("\"Deploying ${release.name}" font-lock-string-face 81 107)) :syntax (3 34 nil 81) :check (error user-error ("Unmatched bracket or quote")) :point 1) :repaired (:text "(defwidget release-card [release]\n\11   (box :class \"release-card\"\n\11\11(label :text \"Deploying ${release.name}\")))\n" :indents ((1 0 "(defwidget release-card [release]") (2 11 "\11   (box :class \"release-card\"") (3 16 "\11\11(label :text \"Deploying ${release.name}\")))")) :faces (("defwidget" font-lock-keyword-face 2 11) ("box" font-lock-type-face 40 43) (":class" font-lock-builtin-face 44 50) ("\"release-card\"" font-lock-string-face 51 65) ("label" font-lock-type-face 69 74) (":text" font-lock-builtin-face 75 80) ("\"Deploying ${release.name}\"" font-lock-string-face 81 108)) :check (ok) :point 112 :modified t))"####
    ]];
    ParityBatchCase::value(
        "recovers_an_incomplete_widget_after_exact_syntax_and_indent_diagnostics",
        elisp_form,
        expect,
    )
}

fn navigates_nested_lists_vectors_maps_strings_and_comments() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert
   ";; Structural editing fixture\n"
   "(defwidget deployment [items]\n"
   "  (for item in items\n"
   "    (box :class `release-card`\n"
   "      (label :text \"${item.name}\")\n"
   "      (literal :content {item.metadata}))))\n")
  (yuck-mode)
  (font-lock-ensure)
  (let (sexps syntax motions)
    (dolist (needle '("(defwidget" "[items]" "{item.metadata}"))
      (goto-char (point-min))
      (search-forward needle)
      (goto-char (match-beginning 0))
      (let* ((start (point))
             (end (scan-sexps start 1)))
        (push
         (list needle start end
               (buffer-substring-no-properties start end))
         sexps)))
    (dolist (probe '(("release-card" 2)
                     ("${item.name}" 3)
                     ("item.metadata" 2)
                     ("Structural editing" 3)))
      (push
       (neomacs-melpa-yuck-mode--syntax-state-at
        (car probe) (cadr probe))
       syntax))
    (goto-char (point-min))
    (search-forward "(for")
    (goto-char (match-beginning 0))
    (let ((start (point)))
      (forward-sexp)
      (push
       (list :forward start (point)
             (buffer-substring-no-properties start (point)))
       motions)
      (backward-sexp)
      (push (list :backward (point) (= (point) start)) motions))
    (list
     :sexps (nreverse sexps)
     :syntax (nreverse syntax)
     :motions (nreverse motions)
     :faces (neomacs-melpa-yuck-mode--face-runs)
     :balanced (condition-case nil
                   (progn (check-parens) t)
                 (error nil)))))
"####;
    let expect = expect![[
        r####"OK (:sexps (("(defwidget" 31 191 "(defwidget deployment [items]\n  (for item in items\n    (box :class `release-card`\n      (label :text \"${item.name}\")\n      (literal :content {item.metadata}))))") ("[items]" 53 60 "[items]") ("{item.metadata}" 172 187 "{item.metadata}")) :syntax (("release-card" 101 3 96 nil 98) ("${item.name}" 136 4 34 nil 132) ("item.metadata" 175 5 nil nil nil) ("Structural editing" 7 0 nil t 1)) :motions ((:forward 63 190 "(for item in items\n    (box :class `release-card`\n      (label :text \"${item.name}\")\n      (literal :content {item.metadata})))") (:backward 63 t)) :faces ((";; " font-lock-comment-delimiter-face 1 4) ("Structural editing fixture\n" font-lock-comment-face 4 31) ("defwidget" font-lock-keyword-face 32 41) ("for" font-lock-keyword-face 64 67) ("box" font-lock-type-face 87 90) (":class" font-lock-builtin-face 91 97) ("`release-card`" font-lock-string-face 98 112) ("label" font-lock-type-face 120 125) (":text" font-lock-builtin-face 126 131) ("\"${item.name}\"" font-lock-string-face 132 146) ("literal" font-lock-type-face 155 162) (":content" font-lock-builtin-face 163 171)) :balanced t)"####
    ]];
    ParityBatchCase::value(
        "navigates_nested_lists_vectors_maps_strings_and_comments",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opens_and_fontifies_a_real_dashboard_configuration(),
        fontifies_the_complete_component_gallery_and_boundary_contexts(),
        indents_a_nested_widget_and_preserves_strings_and_comments(),
        comments_and_uncomments_a_selected_configuration_block(),
        recovers_an_incomplete_widget_after_exact_syntax_and_indent_diagnostics(),
        navigates_nested_lists_vectors_maps_strings_and_comments(),
    ]
}
