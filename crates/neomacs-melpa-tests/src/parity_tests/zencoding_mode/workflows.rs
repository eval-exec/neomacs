use expect_test::expect;

use super::ParityBatchCase;

fn expands_a_nested_release_dashboard_in_a_real_html_buffer() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (html-mode)
  (zencoding-mode 1)
  (let ((zencoding-preview-default nil)
        (zencoding-insert-flash-time -1)
        (zencoding-indentation 2)
        (sgml-basic-offset 2))
    (insert
     "<main>\n"
     "  section#release.dashboard data-owner=ops>h2.title+ul.artifacts>li.artifact*3\n"
     "  <footer>Ready Ω</footer>\n"
     "</main>\n")
    (goto-char (point-min))
    (forward-line 1)
    (end-of-line)
    (call-interactively (lookup-key zencoding-mode-keymap (kbd "C-j")))
    (list
     :mode major-mode
     :minor zencoding-mode
     :lighter (assq 'zencoding-mode minor-mode-alist)
     :keys (list
            (lookup-key zencoding-mode-keymap (kbd "C-j"))
            (lookup-key zencoding-mode-keymap (kbd "<C-return>")))
     :indent-settings (list zencoding-indentation sgml-basic-offset)
     :point (point)
     :line (line-number-at-pos)
     :column (current-column)
     :mark-active mark-active
     :text (buffer-substring-no-properties (point-min) (point-max))
     :flash (neomacs-zencoding-test--flash-state))))
"####;
    let expect = expect![[
        r####"OK (:mode html-mode :minor t :lighter (zencoding-mode " Zen") :keys (zencoding-expand-line zencoding-expand-line) :indent-settings (2 2) :point 8 :line 2 :column 0 :mark-active nil :text "<main>\n  <section id=\"release\" class=\"dashboard\" data-owner=\"ops\">\n    <h2 class=\"title\"></h2>\n    <ul class=\"artifacts\">\n    <li class=\"artifact\"></li>\n    <li class=\"artifact\"></li>\n    <li class=\"artifact\"></li>\n    </ul>\n  </section>\n  <footer>Ready Ω</footer>\n</main>\n" :flash (:start 8 :end 238 :face zencoding-preview-output :text "  <section id=\"release\" class=\"dashboard\" data-owner=\"ops\">\n    <h2 class=\"title\"></h2>\n    <ul class=\"artifacts\">\n    <li class=\"artifact\"></li>\n    <li class=\"artifact\"></li>\n    <li class=\"artifact\"></li>\n    </ul>\n  </section>"))"####
    ]];
    ParityBatchCase::value(
        "expands_a_nested_release_dashboard_in_a_real_html_buffer",
        elisp_form,
        expect,
    )
}

fn selects_real_markup_filters_from_each_project_file() -> ParityBatchCase {
    let elisp_form = r####"
(let ((zencoding-preview-default nil)
      (zencoding-insert-flash-time -1)
      (zencoding-indentation 2)
      results)
  (dolist (fixture
           '(("release.html"
              "nav#primary>ul.links>li.item*2"
              html-mode)
             ("release.haml"
              "section.release>h2.title+a.button href=/deploy"
              html-mode)
             ("release.clj"
              "#release>(h2.title+a href=/deploy)+p.status"
              fundamental-mode)
             ("release.notes"
              "article#deploy>h2.title+p.summary"
              fundamental-mode)
             ("commented.html"
              "section#deploy>div.notice|c"
              html-mode)
             ("escaped.html"
              "script src=&quot;deploy.js&quot;|e"
              html-mode)))
    (with-temp-buffer
      (setq buffer-file-name (expand-file-name (car fixture)))
      (funcall (nth 2 fixture))
      (insert "    " (nth 1 fixture))
      (goto-char (point-max))
      (zencoding-expand-line nil)
      (push
       (list
        :file (file-name-nondirectory buffer-file-name)
        :mode major-mode
        :filter (zencoding-default-filter)
        :point (point)
        :column (current-column)
        :text (buffer-substring-no-properties (point-min) (point-max))
        :flash (neomacs-zencoding-test--flash-state))
       results)))
  (nreverse results))
"####;
    let expect = expect![[
        r####"OK ((:file "release.html" :mode html-mode :filter #1=("html") :point 1 :column 0 :text "    <nav id=\"primary\">\n      <ul class=\"links\">\n      <li class=\"item\"></li>\n      <li class=\"item\"></li>\n      </ul>\n    </nav>" :flash (:start 1 :end 129 :face zencoding-preview-output :text "    <nav id=\"primary\">\n      <ul class=\"links\">\n      <li class=\"item\"></li>\n      <li class=\"item\"></li>\n      </ul>\n    </nav>")) (:file "release.haml" :mode html-mode :filter ("haml") :point 1 :column 0 :text "    %section.release\n      %h2.title\n      %a.button{:href => \"/deploy\"}" :flash (:start 1 :end 73 :face zencoding-preview-output :text "    %section.release\n      %h2.title\n      %a.button{:href => \"/deploy\"}")) (:file "release.clj" :mode fundamental-mode :filter ("hic") :point 1 :column 0 :text "    [:div#release\n      [:h2.title]\n      [:a {:href \"/deploy\"}]\n      [:p.status]]" :flash (:start 1 :end 84 :face zencoding-preview-output :text "    [:div#release\n      [:h2.title]\n      [:a {:href \"/deploy\"}]\n      [:p.status]]")) (:file "release.notes" :mode fundamental-mode :filter ("html") :point 1 :column 0 :text "    <article id=\"deploy\">\n      <h2 class=\"title\"></h2>\n      <p class=\"summary\">\n      </p>\n    </article>" :flash (:start 1 :end 108 :face zencoding-preview-output :text "    <article id=\"deploy\">\n      <h2 class=\"title\"></h2>\n      <p class=\"summary\">\n      </p>\n    </article>")) (:file "commented.html" :mode html-mode :filter #1# :point 1 :column 0 :text "    <!-- #deploy -->\n    <section id=\"deploy\">\n      <!-- .notice -->\n      <div class=\"notice\">\n      </div>\n      <!-- /.notice -->\n    </section>\n    <!-- /#deploy -->" :flash (:start 1 :end 171 :face zencoding-preview-output :text "    <!-- #deploy -->\n    <section id=\"deploy\">\n      <!-- .notice -->\n      <div class=\"notice\">\n      </div>\n      <!-- /.notice -->\n    </section>\n    <!-- /#deploy -->")) (:file "escaped.html" :mode html-mode :filter #1# :point 1 :column 0 :text "    &lt;script src=\"&amp;quot;deploy.js&amp;quot;\"&gt;\n    &lt;/script&gt;" :flash (:start 1 :end 75 :face zencoding-preview-output :text "    &lt;script src=\"&amp;quot;deploy.js&amp;quot;\"&gt;\n    &lt;/script&gt;")))"####
    ]];
    ParityBatchCase::value(
        "selects_real_markup_filters_from_each_project_file",
        elisp_form,
        expect,
    )
}

fn edits_a_live_preview_and_accepts_the_revised_markup() -> ParityBatchCase {
    let elisp_form = r####"
(let ((original-show-paren show-paren-mode)
      result)
  (unwind-protect
      (with-temp-buffer
        (html-mode)
        (zencoding-mode 1)
        (let ((zencoding-preview-default t)
              (zencoding-insert-flash-time -1)
              (zencoding-indentation 2)
              (sgml-basic-offset 2))
          (show-paren-mode 1)
          (insert "  ul#deployments>li.pending*2")
          (goto-char (point-max))
          (zencoding-expand-line nil)
          (run-hooks 'post-command-hook)
          (let ((initial
                 (list
                  :text (buffer-substring-no-properties (point-min) (point-max))
                  :point (point)
                  :show-paren show-paren-mode
                  :input (neomacs-zencoding-test--overlay-state
                          zencoding-preview-input)
                  :output (neomacs-zencoding-test--overlay-state
                           zencoding-preview-output)
                  :hooks (neomacs-zencoding-test--hook-state))))
            (goto-char (overlay-start zencoding-preview-input))
            (search-forward "pending" (overlay-end zencoding-preview-input))
            (replace-match "ready")
            (goto-char (overlay-end zencoding-preview-input))
            (run-hooks 'post-command-hook)
            (let ((edited
                   (list
                    :text (buffer-substring-no-properties
                           (point-min) (point-max))
                    :point (point)
                    :show-paren show-paren-mode
                    :input (neomacs-zencoding-test--overlay-state
                            zencoding-preview-input)
                    :output (neomacs-zencoding-test--overlay-state
                             zencoding-preview-output)
                    :hooks (neomacs-zencoding-test--hook-state))))
              (zencoding-preview-accept)
              (setq result
                    (list
                     :initial initial
                     :edited edited
                     :accepted
                     (list
                      :text (buffer-substring-no-properties
                             (point-min) (point-max))
                      :point (point)
                      :line (line-number-at-pos)
                      :column (current-column)
                      :show-paren show-paren-mode
                      :input zencoding-preview-input
                      :output zencoding-preview-output
                      :hooks (neomacs-zencoding-test--hook-state)
                      :flash (neomacs-zencoding-test--flash-state))))))))
    (neomacs-zencoding-test--cleanup-preview)
    (show-paren-mode (if original-show-paren 1 -1)))
  result)
"####;
    let expect = expect![[
        r####"OK (:initial (:text "  ul#deployments>li.pending*2\n" :point 30 :show-paren nil :input (:live t :start 3 :end 30 :front-advance nil :rear-advance nil :face zencoding-preview-input :key-ret zencoding-preview-accept :key-c-g zencoding-preview-abort :before nil :after nil) :output (:live t :start 31 :end 31 :front-advance nil :rear-advance nil :face zencoding-preview-output :key-ret nil :key-c-g nil :before " Zen preview. Choose with RET. Cancel by stepping out. \n" :after "  <ul id=\"deployments\">\n    <li class=\"pending\"></li>\n    <li class=\"pending\"></li>\n  </ul>\n") :hooks (:before-change t :post-command t :pending nil)) :edited (:text "  ul#deployments>li.ready*2\n" :point 28 :show-paren nil :input (:live t :start 3 :end 28 :front-advance nil :rear-advance nil :face zencoding-preview-input :key-ret zencoding-preview-accept :key-c-g zencoding-preview-abort :before nil :after nil) :output (:live t :start 29 :end 29 :front-advance nil :rear-advance nil :face zencoding-preview-output :key-ret nil :key-c-g nil :before " Zen preview. Choose with RET. Cancel by stepping out. \n" :after "  <ul id=\"deployments\">\n    <li class=\"ready\"></li>\n    <li class=\"ready\"></li>\n  </ul>\n") :hooks (:before-change t :post-command t :pending nil)) :accepted (:text "  <ul id=\"deployments\">\n    <li class=\"ready\"></li>\n    <li class=\"ready\"></li>\n  </ul>\n" :point 88 :line 4 :column 7 :show-paren t :input nil :output nil :hooks (:before-change nil :post-command nil :pending nil) :flash (:start 1 :end 88 :face zencoding-preview-output :text "  <ul id=\"deployments\">\n    <li class=\"ready\"></li>\n    <li class=\"ready\"></li>\n  </ul>")))"####
    ]];
    ParityBatchCase::value(
        "edits_a_live_preview_and_accepts_the_revised_markup",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn leaving_a_preview_preserves_the_unexpanded_draft() -> ParityBatchCase {
    let elisp_form = r####"
(let ((original-show-paren show-paren-mode)
      result)
  (unwind-protect
      (with-temp-buffer
        (html-mode)
        (zencoding-mode 1)
        (let ((zencoding-preview-default t))
          (show-paren-mode 1)
          (insert
           "header.site>nav>ul>li*2\n"
           "p.release-note")
          (goto-char (point-min))
          (end-of-line)
          (zencoding-expand-line nil)
          (run-hooks 'post-command-hook)
          (let ((preview
                 (list
                  :text (buffer-substring-no-properties (point-min) (point-max))
                  :input (neomacs-zencoding-test--overlay-state
                          zencoding-preview-input)
                  :output (neomacs-zencoding-test--overlay-state
                           zencoding-preview-output)
                  :hooks (neomacs-zencoding-test--hook-state)
                  :show-paren show-paren-mode)))
            (goto-char (point-max))
            (run-hooks 'post-command-hook)
            (setq result
                  (list
                   :preview preview
                   :aborted
                   (list
                    :text (buffer-substring-no-properties
                           (point-min) (point-max))
                    :point (point)
                    :line (line-number-at-pos)
                    :input zencoding-preview-input
                    :output zencoding-preview-output
                    :hooks (neomacs-zencoding-test--hook-state)
                    :show-paren show-paren-mode
                    :overlays (length (overlays-in (point-min) (point-max)))))))))
    (neomacs-zencoding-test--cleanup-preview)
    (show-paren-mode (if original-show-paren 1 -1)))
  result)
"####;
    let expect = expect![[
        r####"OK (:preview (:text "header.site>nav>ul>li*2\np.release-note" :input (:live t :start 1 :end 24 :front-advance nil :rear-advance nil :face zencoding-preview-input :key-ret zencoding-preview-accept :key-c-g zencoding-preview-abort :before nil :after nil) :output (:live t :start 25 :end 25 :front-advance nil :rear-advance nil :face zencoding-preview-output :key-ret nil :key-c-g nil :before " Zen preview. Choose with RET. Cancel by stepping out. \n" :after "<header class=\"site\">\n  <nav>\n    <ul>\n      <li></li>\n      <li></li>\n    </ul>\n  </nav>\n</header>\n") :hooks (:before-change t :post-command t :pending nil) :show-paren nil) :aborted (:text "header.site>nav>ul>li*2\np.release-note" :point 39 :line 2 :input nil :output nil :hooks (:before-change nil :post-command nil :pending nil) :show-paren t :overlays 0))"####
    ]];
    ParityBatchCase::value(
        "leaving_a_preview_preserves_the_unexpanded_draft",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn an_unknown_filter_fails_transactionally_in_preview_and_direct_use() -> ParityBatchCase {
    let elisp_form = r####"
(let ((original-show-paren show-paren-mode)
      result)
  (unwind-protect
      (with-temp-buffer
        (html-mode)
        (zencoding-mode 1)
        (let ((zencoding-preview-default t))
          (show-paren-mode 1)
          (insert "a|unknown\nKEEP")
          (goto-char (point-min))
          (end-of-line)
          (let ((preview-point (point)))
            (zencoding-expand-line nil)
            (run-hooks 'post-command-hook)
            (let ((invalid
                   (list
                    :text (buffer-substring-no-properties (point-min) (point-max))
                    :point (point)
                    :input (neomacs-zencoding-test--overlay-state
                            zencoding-preview-input)
                    :output (neomacs-zencoding-test--overlay-state
                             zencoding-preview-output)
                    :hooks (neomacs-zencoding-test--hook-state)
                    :show-paren show-paren-mode)))
              (zencoding-preview-accept)
              (let ((accepted
                     (list
                      :text (buffer-substring-no-properties
                             (point-min) (point-max))
                      :point (point)
                      :input zencoding-preview-input
                      :output zencoding-preview-output
                      :hooks (neomacs-zencoding-test--hook-state)
                      :show-paren show-paren-mode
                      :overlays (length (overlays-in (point-min) (point-max))))))
                (erase-buffer)
                (insert "a|unknown\nKEEP")
                (goto-char preview-point)
                (let ((zencoding-preview-default nil)
                      direct-outcome)
                  (setq direct-outcome
                        (condition-case error-data
                            (progn
                              (zencoding-expand-line nil)
                              :returned)
                          (error
                           (list (car error-data) (cdr error-data)))))
                  (setq result
                        (list
                         :preview invalid
                         :accepted accepted
                         :direct
                         (list
                          :outcome direct-outcome
                          :text (buffer-substring-no-properties
                                 (point-min) (point-max))
                          :point (point)
                          :line (line-number-at-pos)
                          :flash (neomacs-zencoding-test--flash-state))))))))))
    (neomacs-zencoding-test--cleanup-preview)
    (show-paren-mode (if original-show-paren 1 -1)))
  result)
"####;
    let expect = expect![[
        r####"OK (:preview (:text "a|unknown\nKEEP" :point 10 :input (:live t :start 1 :end 10 :front-advance nil :rear-advance nil :face zencoding-preview-input :key-ret zencoding-preview-accept :key-c-g zencoding-preview-abort :before nil :after nil) :output (:live t :start 11 :end 11 :front-advance nil :rear-advance nil :face zencoding-preview-output :key-ret nil :key-c-g nil :before " Zen preview. Choose with RET. Cancel by stepping out. \n" :after nil) :hooks (:before-change t :post-command t :pending nil) :show-paren nil) :accepted (:text "a|unknown\nKEEP" :point 10 :input nil :output nil :hooks (:before-change nil :post-command nil :pending nil) :show-paren t :overlays 0) :direct (:outcome (wrong-type-argument (arrayp nil)) :text "a|unknown\nKEEP" :point 10 :line 1 :flash nil))"####
    ]];
    ParityBatchCase::value(
        "an_unknown_filter_fails_transactionally_in_preview_and_direct_use",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn honors_preview_customization_prefix_arguments_and_active_regions() -> ParityBatchCase {
    let elisp_form = r####"
(let ((zencoding-insert-flash-time -1)
      results)
  (dolist (fixture
           '((:default t :prefix nil :region nil)
             (:default t :prefix (4) :region nil)
             (:default nil :prefix nil :region nil)
             (:default nil :prefix (4) :region nil)
             (:default nil :prefix nil :region t)))
    (with-temp-buffer
      (html-mode)
      (zencoding-mode 1)
      (let ((zencoding-preview-default (plist-get fixture :default))
            (current-prefix-arg (plist-get fixture :prefix)))
        (insert "div.release>span.status")
        (goto-char (point-max))
        (when (plist-get fixture :region)
          (set-mark (point-min))
          (setq mark-active t))
        (call-interactively 'zencoding-expand-line)
        (when (overlayp zencoding-preview-input)
          (run-hooks 'post-command-hook))
        (push
         (list
          :default (plist-get fixture :default)
          :prefix (plist-get fixture :prefix)
          :region (plist-get fixture :region)
          :path (if (overlayp zencoding-preview-input) :preview :direct)
          :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :mark (and (mark t) (mark t))
          :mark-active mark-active
          :input (neomacs-zencoding-test--overlay-state
                  zencoding-preview-input)
          :output (neomacs-zencoding-test--overlay-state
                   zencoding-preview-output)
          :flash (neomacs-zencoding-test--flash-state))
         results)
        (neomacs-zencoding-test--cleanup-preview))))
  (nreverse results))
"####;
    let expect = expect![[
        r####"OK ((:default t :prefix nil :region nil :path :preview :text "div.release>span.status\n" :point 24 :mark nil :mark-active nil :input (:live t :start 1 :end 24 :front-advance nil :rear-advance nil :face zencoding-preview-input :key-ret zencoding-preview-accept :key-c-g zencoding-preview-abort :before nil :after nil) :output (:live t :start 25 :end 25 :front-advance nil :rear-advance nil :face zencoding-preview-output :key-ret nil :key-c-g nil :before " Zen preview. Choose with RET. Cancel by stepping out. \n" :after "<div class=\"release\">\n  <span class=\"status\"></span>\n</div>\n") :flash nil) (:default t :prefix (4) :region nil :path :direct :text "<div class=\"release\">\n  <span class=\"status\"></span>\n</div>" :point 1 :mark nil :mark-active nil :input nil :output nil :flash (:start 1 :end 60 :face zencoding-preview-output :text "<div class=\"release\">\n  <span class=\"status\"></span>\n</div>")) (:default nil :prefix nil :region nil :path :direct :text "<div class=\"release\">\n  <span class=\"status\"></span>\n</div>" :point 1 :mark nil :mark-active nil :input nil :output nil :flash (:start 1 :end 60 :face zencoding-preview-output :text "<div class=\"release\">\n  <span class=\"status\"></span>\n</div>")) (:default nil :prefix (4) :region nil :path :preview :text "div.release>span.status\n" :point 24 :mark nil :mark-active nil :input (:live t :start 1 :end 24 :front-advance nil :rear-advance nil :face zencoding-preview-input :key-ret zencoding-preview-accept :key-c-g zencoding-preview-abort :before nil :after nil) :output (:live t :start 25 :end 25 :front-advance nil :rear-advance nil :face zencoding-preview-output :key-ret nil :key-c-g nil :before " Zen preview. Choose with RET. Cancel by stepping out. \n" :after "<div class=\"release\">\n  <span class=\"status\"></span>\n</div>\n") :flash nil) (:default nil :prefix nil :region t :path :preview :text "div.release>span.status\n" :point 24 :mark 1 :mark-active t :input (:live t :start 1 :end 24 :front-advance nil :rear-advance nil :face zencoding-preview-input :key-ret zencoding-preview-accept :key-c-g zencoding-preview-abort :before nil :after nil) :output (:live t :start 25 :end 25 :front-advance nil :rear-advance nil :face zencoding-preview-output :key-ret nil :key-c-g nil :before " Zen preview. Choose with RET. Cancel by stepping out. \n" :after "<div class=\"release\">\n  <span class=\"status\"></span>\n</div>\n") :flash nil))"####
    ]];
    ParityBatchCase::value(
        "honors_preview_customization_prefix_arguments_and_active_regions",
        elisp_form,
        expect,
    )
    .fresh_process()
}

pub(crate) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        expands_a_nested_release_dashboard_in_a_real_html_buffer(),
        selects_real_markup_filters_from_each_project_file(),
        edits_a_live_preview_and_accepts_the_revised_markup(),
        leaving_a_preview_preserves_the_unexpanded_draft(),
        an_unknown_filter_fails_transactionally_in_preview_and_direct_use(),
        honors_preview_customization_prefix_arguments_and_active_regions(),
    ]
}
