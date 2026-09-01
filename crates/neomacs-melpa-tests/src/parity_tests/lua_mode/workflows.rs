use expect_test::expect;

use super::ParityBatchCase;

fn release_module_authoring_indents_nested_control_flow_tables_and_literals() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (rename-buffer "*lua-release-authoring*" t)
  (lua-mode)
  (setq-local indent-tabs-mode nil)
  (insert
   "-- Prepare a release plan without changing literal payloads.\n"
   "local release = {}\n\n"
   "function release.prepare(items, options)\n"
   "local plan = {\n"
   "owner = options.owner,\n"
   "channels = {\n"
   "\"canary\",\n"
   "\"stable\"\n"
   "}\n"
   "}\n\n"
   "if options.enabled then\n"
   "for _, item in ipairs(items) do\n"
   "table.insert(plan, {\n"
   "name = item.name,\n"
   "version = item.version,\n"
   "metadata = {\n"
   "unicode = \"café λ\",\n"
   "notes = [[keep\n"
   "literal indentation]]\n"
   "}\n"
   "})\n"
   "end\n"
   "elseif options.fallback then\n"
   "repeat\n"
   "options.retries = options.retries - 1\n"
   "until options.retries == 0\n"
   "else\n"
   "return\n"
   "nil\n"
   "end\n\n"
   "return plan\n"
   "end\n")
  (indent-region (point-min) (point-max))
  (font-lock-ensure (point-min) (point-max))
  (list
   :mode (list major-mode mode-name (derived-mode-p 'prog-mode))
   :keys (list (key-binding (kbd "C-c C-l"))
               (key-binding (kbd "C-c C-f"))
               (key-binding [remap backward-up-list]))
   :policies (list indent-line-function
                   beginning-of-defun-function
                   end-of-defun-function
                   fill-paragraph-function
                   comment-start
                   comment-start-skip
                   parse-sexp-lookup-properties)
   :text (buffer-substring-no-properties (point-min) (point-max))))
"##;
    let expected = expect![[
        r#"OK (:mode (lua-mode "Lua" prog-mode) :keys (lua-send-buffer lua-search-documentation lua-backward-up-list) :policies (lua-indent-line lua-beginning-of-proc lua-end-of-proc lua--fill-paragraph "-- " "---*[ \11]*" t) :text "-- Prepare a release plan without changing literal payloads.\nlocal release = {}\n\nfunction release.prepare(items, options)\n   local plan = {\n      owner = options.owner,\n      channels = {\n         \"canary\",\n         \"stable\"\n      }\n   }\n\n   if options.enabled then\n      for _, item in ipairs(items) do\n         table.insert(plan, {\n                         name = item.name,\n                         version = item.version,\n                         metadata = {\n                            unicode = \"café λ\",\n                            notes = [[keep\nliteral indentation]]\n                         }\n         })\n      end\n   elseif options.fallback then\n      repeat\n         options.retries = options.retries - 1\n      until options.retries == 0\n   else\n      return\n         nil\n   end\n\n   return plan\nend\n")"#
    ]];
    ParityBatchCase::value(
        "release_module_authoring_indents_nested_control_flow_tables_and_literals",
        elisp_form,
        expected,
    )
}

fn publishing_module_fontifies_luadoc_builtins_labels_and_long_literals() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (rename-buffer "*lua-publishing-font-lock*" t)
  (lua-mode)
  (insert
   "#!/usr/bin/env lua\n"
   "--- Publish a release plan.\n"
   "-- @class ReleasePlan\n"
   "-- @param items deployment records\n"
   "local json, audit = require(\"json\"), require(\"audit\")\n"
   "local release = {}\n"
   "local banner = [=[for end -- literal]=]\n\n"
   "function release.publish(items, dry_run)\n"
   "   local ordered = table.sort(items)\n"
   "   if dry_run or ordered == nil then\n"
   "      goto skipped\n"
   "   end\n"
   "   ::skipped::\n"
   "   audit.write(json.encode(ordered))\n"
   "   return string.format(\"count=%d\", #ordered)\n"
   "end\n")
  (list
   :faces (lua-test-face-spans)
   :syntax (mapcar #'lua-test-syntax-at
                   '("deployment records" "for end -- literal"
                     "table.sort" "count=%d"))
   :text (buffer-substring-no-properties (point-min) (point-max))))
"##;
    let expected = expect![[
        r##"OK (:faces ((1 0 "#!/usr/bin/env lua" font-lock-comment-face) (2 0 "--- " font-lock-comment-delimiter-face) (2 4 "Publish a release plan.\n" font-lock-comment-face) (3 0 "-- " font-lock-comment-delimiter-face) (3 3 "@class" font-lock-keyword-face) (3 9 " " font-lock-comment-face) (3 10 "ReleasePlan" font-lock-variable-name-face) (3 21 "\n" font-lock-comment-face) (4 0 "-- " font-lock-comment-delimiter-face) (4 3 "@param" font-lock-keyword-face) (4 9 " " font-lock-comment-face) (4 10 "items" font-lock-variable-name-face) (4 15 " deployment records\n" font-lock-comment-face) (5 0 "local" font-lock-keyword-face) (5 6 "json" font-lock-variable-name-face) (5 12 "audit" font-lock-variable-name-face) (5 20 "require" font-lock-builtin-face) (5 28 "\"json\"" font-lock-string-face) (5 37 "require" font-lock-builtin-face) (5 45 "\"audit\"" font-lock-string-face) (6 0 "local" font-lock-keyword-face) (6 6 "release" font-lock-variable-name-face) (7 0 "local" font-lock-keyword-face) (7 6 "banner" font-lock-variable-name-face) (7 15 "[=[for end -- literal]=]" font-lock-string-face) (9 0 "function" font-lock-keyword-face) (9 9 "release.publish" font-lock-function-name-face) (9 25 "items" font-lock-variable-name-face) (9 32 "dry_run" font-lock-variable-name-face) (10 3 "local" font-lock-keyword-face) (10 9 "ordered" font-lock-variable-name-face) (10 19 "table" font-lock-builtin-face) (10 25 "sort" font-lock-builtin-face) (11 3 "if" font-lock-keyword-face) (11 14 "or" font-lock-keyword-face) (11 28 "nil" font-lock-constant-face) (11 32 "then" font-lock-keyword-face) (12 6 "goto" font-lock-keyword-face) (12 11 "skipped" font-lock-constant-face) (13 3 "end" font-lock-keyword-face) (14 3 "::skipped::" font-lock-constant-face) (16 3 "return" font-lock-keyword-face) (16 10 "string" font-lock-builtin-face) (16 17 "format" font-lock-builtin-face) (16 24 "\"count=%d\"" font-lock-string-face) (17 0 "end" font-lock-keyword-face)) :syntax (("deployment records" :string nil :comment t :start (4 0)) ("for end -- literal" :string t :comment nil :start (7 15)) ("table.sort" :string nil :comment nil :start nil) ("count=%d" :string 34 :comment nil :start (16 24))) :text "#!/usr/bin/env lua\n--- Publish a release plan.\n-- @class ReleasePlan\n-- @param items deployment records\nlocal json, audit = require(\"json\"), require(\"audit\")\nlocal release = {}\nlocal banner = [=[for end -- literal]=]\n\nfunction release.publish(items, dry_run)\n   local ordered = table.sort(items)\n   if dry_run or ordered == nil then\n      goto skipped\n   end\n   ::skipped::\n   audit.write(json.encode(ordered))\n   return string.format(\"count=%d\", #ordered)\nend\n")"##
    ]];
    ParityBatchCase::value(
        "publishing_module_fontifies_luadoc_builtins_labels_and_long_literals",
        elisp_form,
        expected,
    )
}

fn imenu_and_block_navigation_reach_real_module_definitions_and_control_flow() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (rename-buffer "*lua-module-navigation*" t)
  (lua-mode)
  (insert
   "local json = require(\"json\")\n"
   "metrics = require(\"metrics\")\n"
   "local M = {}\n\n"
   "local function normalize(item)\n"
   "   return item.name, item.version\n"
   "end\n\n"
   "function M.publish(items, options)\n"
   "   local function decorate(item)\n"
   "      if item.canary then\n"
   "         return \"canary:\" .. item.name\n"
   "      elseif item.stable then\n"
   "         return \"stable:\" .. item.name\n"
   "      else\n"
   "         return item.name\n"
   "      end\n"
   "   end\n"
   "   -- function fake() end\n"
   "   repeat\n"
   "      options.retries = options.retries - 1\n"
   "   until options.retries == 0\n"
   "   return decorate(items[1])\n"
   "end\n\n"
   "M.audit = function(record)\n"
   "   metrics.count(record.kind)\n"
   "end\n\n"
   "function M.transport:send(payload)\n"
   "   return json.encode(payload)\n"
   "end\n\n"
   "return M\n")
  (font-lock-ensure (point-min) (point-max))
  (let* ((index (funcall imenu-create-index-function))
         (publish (assoc "M.publish" index))
         imenu-jump
         defun-start
         defun-end
         elseif-match
         repeat-match
         forward-function)
    (imenu publish)
    (setq imenu-jump (lua-test-location (point)))
    (goto-char (point-min))
    (search-forward "return \"stable:\"")
    (beginning-of-defun)
    (setq defun-start (lua-test-location (point)))
    (end-of-defun)
    (setq defun-end (lua-test-location (point)))
    (goto-char (point-min))
    (search-forward "elseif")
    (backward-word)
    (lua-goto-matching-block)
    (setq elseif-match (lua-test-location (point)))
    (goto-char (point-min))
    (search-forward "repeat")
    (backward-word)
    (lua-goto-matching-block)
    (setq repeat-match (lua-test-location (point)))
    (goto-char (point-min))
    (search-forward "function M.publish")
    (beginning-of-line)
    (lua-forward-sexp)
    (setq forward-function (lua-test-location (point)))
    (list
     :index (lua-test-imenu-snapshot index)
     :imenu-jump imenu-jump
     :defun (list defun-start defun-end)
     :elseif-match elseif-match
     :repeat-match repeat-match
     :forward-function forward-function)))
"##;
    let expected = expect![[
        r#"OK (:index (("Requires" ("json" :at (1 0 "local json = require(\"json\")")) ("metrics" :at (2 0 "metrics = require(\"metrics\")"))) ("normalize" :at (5 0 "local function normalize(item)")) ("M.publish" :at (9 0 "function M.publish(items, options)")) ("decorate" :at (10 0 "   local function decorate(item)")) ("M.audit" :at (26 0 "M.audit = function(record)")) ("M.transport:send" :at (30 0 "function M.transport:send(payload)"))) :imenu-jump (9 0 "function M.publish(items, options)") :defun ((9 0 "function M.publish(items, options)") (25 0 "")) :elseif-match (11 6 "      if item.canary then") :repeat-match (22 3 "   until options.retries == 0") :forward-function (24 3 "end"))"#
    ]];
    ParityBatchCase::value(
        "imenu_and_block_navigation_reach_real_module_definitions_and_control_flow",
        elisp_form,
        expected,
    )
}

fn release_notes_fill_and_comment_continuation_preserve_code_boundaries() -> ParityBatchCase {
    let elisp_form = r##"
(let ((buffer (generate-new-buffer "*lua-release-notes*")))
  (unwind-protect
      (save-window-excursion
        (switch-to-buffer buffer)
        (lua-mode)
        (setq-local fill-column 48)
        (insert
         "\n"
         "-- Deploy each release only after every regional health check has completed and the rollback window has been recorded.\n"
         "local deployment = release.prepare(candidate)\n")
        (goto-char (point-min))
        (fill-paragraph)
        (goto-char (point-max))
        (insert "\n--- Record the operator decision")
        (execute-kbd-macro (kbd "M-j"))
        (execute-kbd-macro "and notify the incident channel")
        (list
         :fill-column fill-column
         :comment-command (key-binding (kbd "M-j"))
         :point (lua-test-location (point))
         :text (buffer-substring-no-properties (point-min) (point-max))))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))))
"##;
    let expected = expect![[
        r#"OK (:fill-column 48 :comment-command default-indent-new-line :point (8 35 "--- and notify the incident channel") :text "\n-- Deploy each release only after every regional\n-- health check has completed and the rollback\n-- window has been recorded.\nlocal deployment = release.prepare(candidate)\n\n--- Record the operator decision\n--- and notify the incident channel")"#
    ]];
    ParityBatchCase::value(
        "release_notes_fill_and_comment_continuation_preserve_code_boundaries",
        elisp_form,
        expected,
    )
}

fn interactive_release_branch_editing_reindents_closers_and_control_words() -> ParityBatchCase {
    let elisp_form = r##"
(let ((buffer (generate-new-buffer "release-workflow.lua")))
  (unwind-protect
      (save-window-excursion
        (switch-to-buffer buffer)
        (setq buffer-file-name "/project/release-workflow.lua")
        (set-auto-mode)
        (setq-local indent-tabs-mode nil)
        (setq-local blink-matching-paren nil)
        (electric-indent-local-mode 1)
        (abbrev-mode 1)
        (execute-kbd-macro "if release.enabled then")
        (execute-kbd-macro (kbd "M-j"))
        (execute-kbd-macro "publish({")
        (execute-kbd-macro (kbd "M-j"))
        (execute-kbd-macro "name = \"stable\",")
        (execute-kbd-macro (kbd "M-j"))
        (execute-kbd-macro "metadata = {")
        (execute-kbd-macro (kbd "M-j"))
        (execute-kbd-macro "owner = release.owner")
        (execute-kbd-macro (kbd "M-j"))
        (execute-kbd-macro "}")
        (execute-kbd-macro (kbd "M-j"))
        (execute-kbd-macro "})")
        (execute-kbd-macro (kbd "M-j"))
        (execute-kbd-macro "else")
        (execute-kbd-macro (kbd "M-j"))
        (execute-kbd-macro "rollback()")
        (execute-kbd-macro (kbd "M-j"))
        (execute-kbd-macro "end")
        (execute-kbd-macro (kbd "M-j"))
        (list
         :mode (list major-mode mode-name)
         :electric electric-indent-mode
         :abbrev abbrev-mode
         :point (lua-test-location (point))
         :text (buffer-substring-no-properties (point-min) (point-max))))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))))
"##;
    let expected = expect![[
        r#"OK (:mode (lua-mode "Lua") :electric t :abbrev t :point (11 0 "") :text "if release.enabled then\n   publish({\n         name = \"stable\",\n         metadata = {\n            owner = release.owner\n         }\n   })\nelse\n   rollback()\nend\n")"#
    ]];
    ParityBatchCase::value(
        "interactive_release_branch_editing_reindents_closers_and_control_words",
        elisp_form,
        expected,
    )
}

fn source_submission_builds_exact_repl_commands_without_starting_a_process() -> ParityBatchCase {
    let elisp_form = r##"
(let ((source (generate-new-buffer "orders.lua"))
      (process-output (generate-new-buffer "*lua-submit-output*"))
      (lua-process 'lua-test-submit-process)
      (lua-process-buffer nil)
      (lua-always-show t)
      commands)
  (unwind-protect
      (save-window-excursion
        (switch-to-buffer source)
        (setq buffer-file-name "/project/orders.lua")
        (setq lua-process-buffer process-output)
        (lua-mode)
        (insert
         "#!/usr/bin/env lua\n"
         "local total = 0\n"
         "function invoice.total(items)\n"
         "   return #items\n"
         "end\n"
         "local final = invoice.total({})\n"
         "local note = \"café's\\tready\"\n")
        (cl-letf (((symbol-function 'comint-check-proc)
                   (lambda (_buffer) t))
                  ((symbol-function 'process-send-string)
                   (lambda (process command)
                     (unless (eq process 'lua-test-submit-process)
                       (error "unexpected Lua submission process"))
                     (push command commands)))
                  ((symbol-function 'process-buffer)
                   (lambda (_process) process-output)))
          (lua-send-buffer)
          (goto-char (point-min))
          (search-forward "local final")
          (lua-send-current-line)
          (goto-char (point-min))
          (search-forward "return #items")
          (call-interactively #'lua-send-defun)
          (goto-char (point-min))
          (search-forward "local note")
          (let ((start (line-beginning-position))
                (end (line-end-position)))
            (lua-send-region start end))
          (lua-send-string "print('ready')")
          (list
           :commands (nreverse commands)
           :shown (and (get-buffer-window process-output) t)
           :file buffer-file-name
           :text (buffer-substring-no-properties (point-min) (point-max)))))
    (when (buffer-live-p process-output)
      (kill-buffer process-output))
    (when (buffer-live-p source)
      (kill-buffer source))))
"##;
    let expected = expect![[
        r##"OK (:commands ("print(''); luamode_loadstring('local total = 0\\nfunction invoice.total(items)\\n   return #items\\nend\\nlocal final = invoice.total({})\\nlocal note = \\\"café\\'s\\\\tready\\\"\\n', '/project/orders.lua', 2);\n" "print(''); luamode_loadstring('local final = invoice.total({})', '/project/orders.lua', 6);\n" "print(''); luamode_loadstring('function invoice.total(items)\\n   return #items\\nend\\n', '/project/orders.lua', 3);\n" "print(''); luamode_loadstring('local note = \\\"café\\'s\\\\tready\\\"', '/project/orders.lua', 7);\n" "print('ready')\n") :shown t :file "/project/orders.lua" :text "#!/usr/bin/env lua\nlocal total = 0\nfunction invoice.total(items)\n   return #items\nend\nlocal final = invoice.total({})\nlocal note = \"café's\\tready\"\n")"##
    ]];
    ParityBatchCase::value(
        "source_submission_builds_exact_repl_commands_without_starting_a_process",
        elisp_form,
        expected,
    )
}

fn function_lookup_and_documentation_use_the_symbol_at_point_and_configured_boundary()
-> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (rename-buffer "*lua-documentation-workflow*" t)
  (lua-mode)
  (insert
   "local rendered = string.format(\"release=%s\", release.name)\n"
   "client.v2:send(rendered)\n"
   "table.insert.\n")
  (let* ((visited nil)
         (lua-documentation-url "https://docs.example.test/lua/5.4/manual.html")
         (lua-documentation-function
          (lambda (url) (push url visited)))
         format-name
         method-name
         trailing-name)
    (goto-char (point-min))
    (search-forward "string.format")
    (backward-char 3)
    (setq format-name (lua-funcname-at-point))
    (lua-search-documentation)
    (goto-char (point-min))
    (search-forward "client.v2:send")
    (backward-char 2)
    (setq method-name (lua-funcname-at-point))
    (lua-search-documentation)
    (goto-char (point-min))
    (search-forward "table.insert.")
    (backward-char)
    (setq trailing-name (lua-funcname-at-point))
    (lua-search-documentation)
    (list
     :names (list format-name method-name trailing-name)
     :visited (nreverse visited)
     :command (key-binding (kbd "C-c C-f"))
     :text (buffer-substring-no-properties (point-min) (point-max)))))
"##;
    let expected = expect![[
        r#"OK (:names ("string.format" "send" "table.insert") :visited ("https://docs.example.test/lua/5.4/manual.html#pdf-string.format" "https://docs.example.test/lua/5.4/manual.html#pdf-send" "https://docs.example.test/lua/5.4/manual.html#pdf-table.insert") :command lua-search-documentation :text "local rendered = string.format(\"release=%s\", release.name)\nclient.v2:send(rendered)\ntable.insert.\n")"#
    ]];
    ParityBatchCase::value(
        "function_lookup_and_documentation_use_the_symbol_at_point_and_configured_boundary",
        elisp_form,
        expected,
    )
}

fn flymake_backend_submits_the_live_buffer_and_reports_luacheck_diagnostics() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (rename-buffer "release-check.lua" t)
  (lua-mode)
  (insert
   "local release = {}\n"
   "local stale = release.previous\n"
   "function release.publish(candidate)\n"
   "   return candidate..\n"
   "end\n")
  (let ((lua-luacheck-program "/tools/luacheck")
        output
        make-contract
        sentinel
        backend
        submitted
        (eof-count 0)
        reported)
    (unwind-protect
        (progn
          (cl-letf (((symbol-function 'process-live-p) (lambda (_process) nil))
                    ((symbol-function 'make-process)
                     (lambda (&rest arguments)
                       (setq sentinel (plist-get arguments :sentinel))
                       (setq output (plist-get arguments :buffer))
                       (with-current-buffer output
                         (insert
                          "stdin:2:7-11: (W211) unused variable 'stale'\n"
                          "stdin:4:11-20: (E011) expected expression near <eof>\n"))
                       (setq make-contract
                             (list :name (plist-get arguments :name)
                                   :noquery (plist-get arguments :noquery)
                                   :connection-type
                                   (plist-get arguments :connection-type)
                                   :buffer-name (buffer-name output)
                                   :command (plist-get arguments :command)))
                       'lua-test-luacheck-process))
                    ((symbol-function 'process-send-region)
                     (lambda (process start end)
                       (setq submitted
                             (list process
                                   (buffer-substring-no-properties start end)))))
                    ((symbol-function 'process-send-eof)
                     (lambda (process)
                       (unless (eq process 'lua-test-luacheck-process)
                         (error "unexpected Lua Flymake process"))
                       (setq eof-count (1+ eof-count))))
                    ((symbol-function 'process-status)
                     (lambda (_process) 'exit))
                    ((symbol-function 'process-buffer)
                     (lambda (_process) output)))
            (setq backend
                  (car (memq #'lua-flymake flymake-diagnostic-functions)))
            (unless backend
              (error "lua-mode did not install its Flymake backend"))
            (funcall backend (lambda (diagnostics) (setq reported diagnostics)))
            (funcall sentinel 'lua-test-luacheck-process "finished\n")
            (list
             :hook flymake-diagnostic-functions
             :make make-contract
             :submitted submitted
             :eof-count eof-count
             :diagnostics
             (mapcar
              (lambda (diagnostic)
                (list
                 :locus (if (bufferp (flymake-diagnostic-buffer diagnostic))
                            :buffer
                          :file)
                 :range (list (flymake-diagnostic-beg diagnostic)
                              (flymake-diagnostic-end diagnostic))
                 :type (flymake-diagnostic-type diagnostic)
                 :text (flymake-diagnostic-text diagnostic)))
              reported))))
      (when (buffer-live-p output)
        (kill-buffer output)))))
"##;
    let expected = expect![[
        r#"OK (:hook (lua-flymake t) :make (:name "luacheck" :noquery t :connection-type pipe :buffer-name " *flymake-luacheck*" :command ("/tools/luacheck" "--codes" "--ranges" "--formatter" "plain" "-")) :submitted (lua-test-luacheck-process "local release = {}\nlocal stale = release.previous\nfunction release.publish(candidate)\n   return candidate..\nend\n") :eof-count 1 :diagnostics ((:locus :buffer :range ((2 . 7) (2 . 12)) :type :warning :text "(W211) unused variable 'stale'") (:locus :buffer :range ((4 . 11) (4 . 21)) :type :error :text "(E011) expected expression near <eof>")))"#
    ]];
    ParityBatchCase::value(
        "flymake_backend_submits_the_live_buffer_and_reports_luacheck_diagnostics",
        elisp_form,
        expected,
    )
}

fn hideshow_folds_and_restores_nested_release_blocks_through_lua_navigation() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (rename-buffer "release-folds.lua" t)
  (lua-mode)
  (insert
   "function release.publish(candidate)\n"
   "   if candidate.ready then\n"
   "      deploy(candidate)\n"
   "   else\n"
   "      queue(candidate)\n"
   "   end\n"
   "   audit(candidate)\n"
   "end\n\n"
   "return release\n")
  (hs-minor-mode 1)
  (let (function-hidden branch-hidden)
    (goto-char (point-min))
    (hs-hide-block)
    (let ((overlay
           (cl-find-if (lambda (candidate) (overlay-get candidate 'hs))
                       (overlays-in (point-min) (point-max)))))
      (setq function-hidden
            (list
             :range (list (lua-test-location (overlay-start overlay))
                          (lua-test-location (overlay-end overlay)))
             :kind (overlay-get overlay 'hs)
             :invisible (overlay-get overlay 'invisible)
             :display (substring-no-properties (overlay-get overlay 'display))
             :point (lua-test-location (point)))))
    (goto-char (point-min))
    (hs-show-block)
    (goto-char (point-min))
    (search-forward "if candidate")
    (backward-word 2)
    (hs-hide-block)
    (let ((overlay
           (cl-find-if (lambda (candidate) (overlay-get candidate 'hs))
                       (overlays-in (point-min) (point-max)))))
      (setq branch-hidden
            (list
             :range (list (lua-test-location (overlay-start overlay))
                          (lua-test-location (overlay-end overlay)))
             :kind (overlay-get overlay 'hs)
             :invisible (overlay-get overlay 'invisible)
             :display (substring-no-properties (overlay-get overlay 'display))
             :point (lua-test-location (point)))))
    (hs-show-all)
    (list
     :integration (list hs-block-start-regexp
                        hs-block-end-regexp
                        hs-forward-sexp-function)
     :function-hidden function-hidden
     :branch-hidden branch-hidden
     :remaining-hs-overlays
     (length (cl-remove-if-not
              (lambda (overlay) (overlay-get overlay 'hs))
              (overlays-in (point-min) (point-max))))
     :text (buffer-substring-no-properties (point-min) (point-max)))))
"##;
    let expected = expect![[
        r#"OK (:integration ("\\<\\(do\\|function\\|repeat\\|then\\)\\>" "\\<\\(end\\|until\\)\\>" lua-forward-sexp) :function-hidden (:range ((1 35 "function release.publish(candidate)") (8 0 "end")) :kind code :invisible hs :display "…" :point (1 0 "function release.publish(candidate)")) :branch-hidden (:range ((2 26 "   if candidate.ready then") (6 1 "   end")) :kind code :invisible hs :display "…" :point (2 3 "   if candidate.ready then")) :remaining-hs-overlays 0 :text "function release.publish(candidate)\n   if candidate.ready then\n      deploy(candidate)\n   else\n      queue(candidate)\n   end\n   audit(candidate)\nend\n\nreturn release\n")"#
    ]];
    ParityBatchCase::value(
        "hideshow_folds_and_restores_nested_release_blocks_through_lua_navigation",
        elisp_form,
        expected,
    )
}

fn repl_lifecycle_initializes_reuses_displays_hides_and_kills_the_comint_session() -> ParityBatchCase
{
    let elisp_form = r##"
(let ((source (generate-new-buffer "release-client.lua"))
      (lua-process nil)
      (lua-process-buffer nil)
      live-process-buffers
      process-buffer
      make-calls
      query-flags
      sent)
  (unwind-protect
      (save-window-excursion
        (switch-to-buffer source)
        (lua-mode)
        (cl-letf (((symbol-function 'comint-check-proc)
                   (lambda (buffer)
                     (member (if (bufferp buffer) (buffer-name buffer) buffer)
                             live-process-buffers)))
                  ((symbol-function 'make-comint)
                   (lambda (name program startfile &rest switches)
                     (push (list name program startfile switches) make-calls)
                     (setq process-buffer
                           (get-buffer-create (format "*%s*" name)))
                     (push (buffer-name process-buffer) live-process-buffers)
                     (with-current-buffer process-buffer
                       (erase-buffer)
                       (comint-mode)
                       (insert "> "))
                     process-buffer))
                  ((symbol-function 'get-buffer-process)
                   (lambda (_buffer) 'lua-test-repl-process))
                  ((symbol-function 'set-process-query-on-exit-flag)
                   (lambda (process flag)
                     (push (list process flag) query-flags)))
                  ((symbol-function 'process-status)
                   (lambda (_process) 'run))
                  ((symbol-function 'process-query-on-exit-flag)
                   (lambda (_process) nil))
                  ((symbol-function 'process-send-string)
                   (lambda (process string)
                     (push (list process string) sent)))
                  ((symbol-function 'process-buffer)
                   (lambda (_process) process-buffer)))
          (lua-start-process "lua" "/tools/lua" nil "-i" "-E")
          (let ((initialized
                 (with-current-buffer process-buffer
                   (list
                    :buffer (buffer-name)
                    :mode major-mode
                    :compilation-shell compilation-shell-minor-mode
                    :prompt comint-prompt-regexp
                    :traceback-installed
                    (equal (car compilation-error-regexp-alist)
                           (list lua-traceback-line-re 1 2))
                    :repl-buffer lua--repl-buffer-p
                    :text (buffer-substring-no-properties
                           (point-min) (point-max))))))
            (lua-start-process "lua" "/tools/lua" nil "-i" "-E")
            (lua-show-process-buffer)
            (let ((shown (and (get-buffer-window process-buffer) t)))
              (lua-hide-process-buffer)
              (let ((hidden (not (get-buffer-window process-buffer))))
                (lua-kill-process)
                (list
                 :make-calls (nreverse make-calls)
                 :query-flags (nreverse query-flags)
                 :sent (nreverse sent)
                 :initialized initialized
                 :shown shown
                 :hidden hidden
                 :killed (list (not (buffer-live-p process-buffer))
                               lua-process-buffer)))))))
    (when (buffer-live-p process-buffer)
      (kill-buffer process-buffer))
    (when (buffer-live-p source)
      (kill-buffer source))))
"##;
    let expected = expect![[
        r#"OK (:make-calls (("lua" "/tools/lua" nil ("-i" "-E"))) :query-flags ((lua-test-repl-process nil)) :sent ((lua-test-repl-process "local loadstring = loadstring or load function luamode_loadstring(str, displayname, lineoffset)   if lineoffset > 1 then     str = string.rep('\\n', lineoffset - 1) .. str   end    local x, e = loadstring(str, '@'..displayname)   if e then     error(e)   end   return x() end\n")) :initialized (:buffer "*lua*" :mode comint-mode :compilation-shell t :prompt "[^\n]*\\(>[\11 ]+\\)+$" :traceback-installed t :repl-buffer t :text "> ") :shown t :hidden t :killed (t nil))"#
    ]];
    ParityBatchCase::value(
        "repl_lifecycle_initializes_reuses_displays_hides_and_kills_the_comint_session",
        elisp_form,
        expected,
    )
}

fn matching_a_non_block_token_reports_the_public_navigation_error() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (lua-mode)
  (insert "local release = prepare()\n")
  (goto-char (point-min))
  (lua-goto-matching-block))
"##;
    let expected = expect![[r#"ERR (error "Not on a block control keyword or brace")"#]];
    ParityBatchCase::signal(
        "matching_a_non_block_token_reports_the_public_navigation_error",
        elisp_form,
        expected,
    )
}

fn sending_outside_a_function_reports_the_public_submission_error() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (lua-mode)
  (insert "local release = prepare()\nreturn release\n")
  (goto-char (point-min))
  (search-forward "prepare")
  (cl-letf (((symbol-function 'make-comint)
             (lambda (&rest _arguments)
               (error "unexpected Lua process creation")))
            ((symbol-function 'process-send-string)
             (lambda (&rest _arguments)
               (error "unexpected Lua process send"))))
    (lua-send-defun (point))))
"##;
    let expected = expect![[r#"ERR (error "Not on a function definition")"#]];
    ParityBatchCase::signal(
        "sending_outside_a_function_reports_the_public_submission_error",
        elisp_form,
        expected,
    )
}

pub(super) fn public_workflow_cases() -> Vec<ParityBatchCase> {
    vec![
        release_module_authoring_indents_nested_control_flow_tables_and_literals(),
        publishing_module_fontifies_luadoc_builtins_labels_and_long_literals(),
        imenu_and_block_navigation_reach_real_module_definitions_and_control_flow(),
        release_notes_fill_and_comment_continuation_preserve_code_boundaries(),
        interactive_release_branch_editing_reindents_closers_and_control_words(),
        source_submission_builds_exact_repl_commands_without_starting_a_process(),
        function_lookup_and_documentation_use_the_symbol_at_point_and_configured_boundary(),
        flymake_backend_submits_the_live_buffer_and_reports_luacheck_diagnostics(),
        hideshow_folds_and_restores_nested_release_blocks_through_lua_navigation(),
        repl_lifecycle_initializes_reuses_displays_hides_and_kills_the_comint_session(),
        matching_a_non_block_token_reports_the_public_navigation_error(),
        sending_outside_a_function_reports_the_public_submission_error(),
    ]
}
