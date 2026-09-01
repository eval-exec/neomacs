use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ERLANG_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const ERLANG_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const ERLANG_TEST_PRELUDE: &str = r##"
(defun erlang-test-observe-token (label token offset &optional occurrence)
  "Describe the face and syntax at OFFSET in TOKEN's OCCURRENCE."
  (goto-char (point-min))
  (dotimes (_ (or occurrence 1))
    (search-forward token))
  (let ((position (+ (match-beginning 0) offset)))
    (list label
          (buffer-substring-no-properties
           (match-beginning 0) (match-end 0))
          (get-text-property position 'face)
          (syntax-ppss-context (syntax-ppss position))
          (char-syntax (char-after position)))))
"##;

fn erlang_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ERLANG_MELPA_PIN, "erlang.el")
        .expect("prepare pinned Erlang Mode source below ./tmp")
        .with_prelude(ERLANG_TEST_PRELUDE)
        .with_timeout(ERLANG_TEST_TIMEOUT)
}

fn otp_checkout_server_is_indented_as_a_complete_editing_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "otp_checkout_server_is_indented_as_a_complete_editing_workflow",
        r##"
(with-temp-buffer
  (let ((erlang-indent-level 4))
    (erlang-mode)
    (setq-local indent-tabs-mode nil)
    (insert
     "-module(checkout_server).\n"
     "-behaviour(gen_server).\n"
     "-export([checkout/2, handle_info/2]).\n"
     "-record(state, {orders = 0, pending = #{}}).\n\n"
     "-spec checkout([map()], #state{}) -> {ok, map(), #state{}} | {error, empty}.\n"
     "checkout(Items, State = #state{orders = Count}) when is_list(Items) ->\n"
     "Subtotal = lists:sum([Price * Quantity || #{price := Price, quantity := Quantity} <- Items]),\n"
     "case Subtotal > 0 of\n"
     "true ->\n"
     "{ok, #{total => Subtotal, currency => <<\"USD\">>}, State#state{orders = Count + 1}};\n"
     "false ->\n"
     "{error, empty}\n"
     "end.\n\n"
     "handle_info({await, Ref}, State) ->\n"
     "receive\n"
     "{settled, Ref} ->\n"
     "{noreply, State#state{pending = maps:remove(Ref, State#state.pending)}}\n"
     "after 5000 ->\n"
     "{stop, timeout, State}\n"
     "end.\n")
    (erlang-indent-region (point-min) (point-max))
    (let (columns)
      (goto-char (point-min))
      (while (not (eobp))
        (unless (looking-at-p "[[:space:]]*$")
          (push (list (line-number-at-pos)
                      (current-indentation)
                      (buffer-substring-no-properties
                       (line-beginning-position)
                       (line-end-position)))
                columns))
        (forward-line 1))
      (list :major-mode major-mode
            :indent-function indent-line-function
            :region-function indent-region-function
            :source
            (buffer-substring-no-properties (point-min) (point-max))
            :lines (nreverse columns)))))
"##,
        expect![[
            r##"OK (:major-mode erlang-mode :indent-function erlang-indent-command :region-function erlang-indent-region :source "-module(checkout_server).\n-behaviour(gen_server).\n-export([checkout/2, handle_info/2]).\n-record(state, {orders = 0, pending = #{}}).\n\n-spec checkout([map()], #state{}) -> {ok, map(), #state{}} | {error, empty}.\ncheckout(Items, State = #state{orders = Count}) when is_list(Items) ->\n    Subtotal = lists:sum([Price * Quantity || #{price := Price, quantity := Quantity} <- Items]),\n    case Subtotal > 0 of\n        true ->\n            {ok, #{total => Subtotal, currency => <<\"USD\">>}, State#state{orders = Count + 1}};\n        false ->\n            {error, empty}\n    end.\n\nhandle_info({await, Ref}, State) ->\n    receive\n        {settled, Ref} ->\n            {noreply, State#state{pending = maps:remove(Ref, State#state.pending)}}\n    after 5000 ->\n            {stop, timeout, State}\n    end.\n" :lines ((1 0 "-module(checkout_server).") (2 0 "-behaviour(gen_server).") (3 0 "-export([checkout/2, handle_info/2]).") (4 0 "-record(state, {orders = 0, pending = #{}}).") (6 0 "-spec checkout([map()], #state{}) -> {ok, map(), #state{}} | {error, empty}.") (7 0 "checkout(Items, State = #state{orders = Count}) when is_list(Items) ->") (8 4 "    Subtotal = lists:sum([Price * Quantity || #{price := Price, quantity := Quantity} <- Items]),") (9 4 "    case Subtotal > 0 of") (10 8 "        true ->") (11 12 "            {ok, #{total => Subtotal, currency => <<\"USD\">>}, State#state{orders = Count + 1}};") (12 8 "        false ->") (13 12 "            {error, empty}") (14 4 "    end.") (16 0 "handle_info({await, Ref}, State) ->") (17 4 "    receive") (18 8 "        {settled, Ref} ->") (19 12 "            {noreply, State#state{pending = maps:remove(Ref, State#state.pending)}}") (20 4 "    after 5000 ->") (21 12 "            {stop, timeout, State}") (22 4 "    end.")))"##
        ]],
    )
}

fn semantic_fontification_distinguishes_real_erlang_language_roles() -> ParityBatchCase {
    ParityBatchCase::value(
        "semantic_fontification_distinguishes_real_erlang_language_roles",
        r##"
(with-temp-buffer
  (let ((font-lock-maximum-decoration 4))
    (erlang-mode)
    (insert
     "-module(checkout_pricing).\n"
     "-export([quote/2]).\n"
     "-define(TAX, 8).\n"
     "-record(order, {total, customer}).\n\n"
     "%% Price every line and preserve the quoted customer status.\n"
     "quote(Items, Customer) when is_list(Items) ->\n"
     "    Message = \"ready\",\n"
     "    Status = 'quoted-atom',\n"
     "    Letter = $A,\n"
     "    Total = lists:sum([Price * Quantity ||\n"
     "                       #{price := Price, quantity := Quantity} <- Items]),\n"
     "    case Total > ?TAX of\n"
     "        true -> #order{total = Total, customer = Customer};\n"
     "        false -> {error, Message, Status, Letter}\n"
     "    end.\n")
    (font-lock-ensure (point-min) (point-max))
    (list
     :mode major-mode
     :decoration font-lock-maximum-decoration
     :observations
     (list
      (erlang-test-observe-token 'attribute "-module" 0)
      (erlang-test-observe-token 'exported-function "quote(Items" 0)
      (erlang-test-observe-token 'variable "Items" 0)
      (erlang-test-observe-token 'guard "is_list" 0)
      (erlang-test-observe-token 'remote-module "lists:sum" 0)
      (erlang-test-observe-token 'remote-function "lists:sum" 6)
      (erlang-test-observe-token 'record "#order" 1)
      (erlang-test-observe-token 'macro "?TAX" 1)
      (erlang-test-observe-token 'keyword "case Total" 0)
      (erlang-test-observe-token 'string "\"ready\"" 1)
      (erlang-test-observe-token 'quoted-atom "'quoted-atom'" 1)
      (erlang-test-observe-token 'character "$A" 0)
      (erlang-test-observe-token 'comment "preserve" 0))
     :face-runs
     (let ((position (point-min)) runs)
       (while (< position (point-max))
         (let* ((face (get-text-property position 'face))
                (next (next-single-property-change
                       position 'face nil (point-max))))
           (when face
             (push (list (buffer-substring-no-properties position next) face)
                   runs))
           (setq position next)))
       (nreverse runs)))))
"##,
        expect![[
            r##"OK (:mode erlang-mode :decoration 4 :observations ((attribute "-module" font-lock-preprocessor-face nil 46) (exported-function "quote(Items" erlang-font-lock-exported-function-name-face nil 119) (variable "Items" font-lock-variable-name-face nil 119) (guard "is_list" font-lock-builtin-face nil 119) (remote-module "lists:sum" font-lock-type-face nil 119) (remote-function "lists:sum" font-lock-type-face nil 119) (record "#order" font-lock-type-face nil 119) (macro "?TAX" font-lock-constant-face nil 119) (keyword "case Total" font-lock-keyword-face nil 119) (string "\"ready\"" font-lock-string-face string 119) (quoted-atom "'quoted-atom'" font-lock-string-face string 119) (character "$A" font-lock-constant-face nil 47) (comment "preserve" font-lock-comment-face comment 119)) :face-runs (("-module" font-lock-preprocessor-face) ("-export" font-lock-preprocessor-face) ("quote/2" font-lock-type-face) ("-define" font-lock-preprocessor-face) ("TAX" font-lock-variable-name-face) ("-record" font-lock-preprocessor-face) ("order" font-lock-type-face) ("%% " font-lock-comment-delimiter-face) ("Price every line and preserve the quoted customer status.\n" font-lock-comment-face) ("quote" erlang-font-lock-exported-function-name-face) ("Items" font-lock-variable-name-face) ("Customer" font-lock-variable-name-face) ("when" font-lock-keyword-face) ("is_list" font-lock-builtin-face) ("Items" font-lock-variable-name-face) ("Message" font-lock-variable-name-face) ("\"ready\"" font-lock-string-face) ("Status" font-lock-variable-name-face) ("'quoted-atom'" font-lock-string-face) ("Letter" font-lock-variable-name-face) ("$A" font-lock-constant-face) ("Total" font-lock-variable-name-face) ("lists" font-lock-type-face) ("sum" font-lock-type-face) ("Price" font-lock-variable-name-face) ("Quantity" font-lock-variable-name-face) ("||" font-lock-keyword-face) ("Price" font-lock-variable-name-face) ("Quantity" font-lock-variable-name-face) ("<-" font-lock-keyword-face) ("Items" font-lock-variable-name-face) ("case" font-lock-keyword-face) ("Total" font-lock-variable-name-face) ("TAX" font-lock-constant-face) ("of" font-lock-keyword-face) (" " font-lock-function-name-face) ("order" font-lock-type-face) ("Total" font-lock-variable-name-face) ("Customer" font-lock-variable-name-face) (" " font-lock-function-name-face) ("Message" font-lock-variable-name-face) ("Status" font-lock-variable-name-face) ("Letter" font-lock-variable-name-face) ("end" font-lock-keyword-face)))"##
        ]],
    )
}

fn imenu_defun_navigation_and_region_marking_follow_multiclause_functions() -> ParityBatchCase {
    ParityBatchCase::value(
        "imenu_defun_navigation_and_region_marking_follow_multiclause_functions",
        r##"
(with-temp-buffer
  (require 'imenu)
  (erlang-mode)
  (insert
   "-module(checkout_pricing).\n"
   "-export([quote/2, normalize/1]).\n\n"
   "quote([], Customer) ->\n"
   "    {ok, Customer, 0};\n"
   "quote([#{price := Price} | Rest], Customer) ->\n"
   "    {ok, Customer, Price + element(3, quote(Rest, Customer))}.\n\n"
   "normalize(#{total := Total} = Order) when Total >= 0 ->\n"
   "    Order;\n"
   "normalize(Order) ->\n"
   "    Order#{total => 0}.\n")
  (let* ((index (imenu-default-create-index-function))
         (index-snapshot
          (mapcar (lambda (entry)
                    (list (car entry)
                          (if (markerp (cdr entry))
                              (marker-position (cdr entry))
                            (cdr entry))
                          (line-number-at-pos (cdr entry))))
                  index))
         navigation)
    (goto-char (point-max))
    (dotimes (_ 2)
      (let ((found (beginning-of-defun)))
        (push (list :found found
                    :point (point)
                    :line (line-number-at-pos)
                    :name (erlang-get-function-name-and-arity)
                    :text (buffer-substring-no-properties
                           (line-beginning-position)
                           (line-end-position)))
              navigation)))
    (goto-char (point-min))
    (search-forward "Price + element")
    (let ((current (erlang-current-defun)))
      (beginning-of-line)
      (erlang-mark-function)
      (list
       :index index-snapshot
       :navigation (nreverse navigation)
       :current current
       :marked
       (list (region-beginning)
             (region-end)
             (line-number-at-pos (region-beginning))
             (line-number-at-pos (region-end))
             (buffer-substring-no-properties
              (region-beginning) (region-end)))
       :point (point)
       :mark (mark t)
       :active (use-region-p)))))
"##,
        expect![[
            r##"OK (:index (("quote/2" 62 4) ("normalize/1" 219 9)) :navigation ((:found t :point 219 :line 9 :name "normalize/1" :text "normalize(#{total := Total} = Order) when Total >= 0 ->") (:found t :point 62 :line 4 :name "quote/2" :text "quote([], Customer) ->")) :current "quote" :marked (61 218 3 8 "\nquote([], Customer) ->\n    {ok, Customer, 0};\nquote([#{price := Price} | Rest], Customer) ->\n    {ok, Customer, Price + element(3, quote(Rest, Customer))}.\n") :point 61 :mark 218 :active nil)"##
        ]],
    )
}

fn edoc_completion_indentation_and_fontification_build_function_documentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "edoc_completion_indentation_and_fontification_build_function_documentation",
        r##"
(with-temp-buffer
  (require 'erlang-edoc)
  (erlang-mode)
  (setq-local indent-tabs-mode nil)
  (erlang-edoc-mode 1)
  (insert
   "%%% @doc Checkout pricing API.\n"
   "%%% @au\n"
   "%%% Use the {@mo} macro in generated links.\n"
   "%%% <ul>\n"
   "%%% <li>Quotes preserve the customer identifier.</li>\n"
   "%%% </ul>\n"
   "-module(checkout_pricing).\n\n"
   "%% @doc Calculate a quote.\n"
   "%% @pa\n"
   "quote(Items, Customer) ->\n"
   "    {ok, Items, Customer}.\n")
  (let (completions)
    (dolist (prefix '("@au" "@mo" "@pa"))
      (goto-char (point-min))
      (search-forward prefix)
      (let* ((capf (erlang-edoc-completion-at-point))
             (beg (nth 0 capf))
             (end (nth 1 capf))
             (table (nth 2 capf))
             (typed (buffer-substring-no-properties beg end))
             (candidates (all-completions typed table))
             (completed (completion-at-point)))
        (push (list prefix typed candidates completed
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))
              completions)))
    (indent-region (point-min) (point-max))
    (font-lock-ensure (point-min) (point-max))
    (list
     :mode erlang-edoc-mode
     :completion-hook
     (and (memq 'erlang-edoc-completion-at-point
                completion-at-point-functions)
          t)
     :completions (nreverse completions)
     :source (buffer-substring-no-properties (point-min) (point-max))
     :faces
     (list
      (erlang-test-observe-token 'module-doc-tag "@doc" 0)
      (erlang-test-observe-token 'author-tag "@author" 0)
      (erlang-test-observe-token 'module-macro "@module" 0)
      (erlang-test-observe-token 'function-param "@param" 0)
      (erlang-test-observe-token 'xml-item "<li>" 1)))))
"##,
        expect![[
            r##"OK (:mode t :completion-hook t :completions (("@au" "au" ("author") t "%%% @author") ("@mo" "mo" ("module") t "%%% Use the {@module} macro in generated links.") ("@pa" "pa" ("param") t "%% @param")) :source "%%% @doc Checkout pricing API.\n%%% @author\n%%% Use the {@module} macro in generated links.\n%%% <ul>\n%%% <li>Quotes preserve the customer identifier.</li>\n%%% </ul>\n-module(checkout_pricing).\n\n%% @doc Calculate a quote.\n%% @param\nquote(Items, Customer) ->\n    {ok, Items, Customer}.\n" :faces ((module-doc-tag "@doc" (erlang-edoc-tag font-lock-comment-face) comment 46) (author-tag "@author" (erlang-edoc-tag font-lock-comment-face) comment 46) (module-macro "@module" (erlang-edoc-macro font-lock-comment-face) comment 46) (function-param "@param" (erlang-edoc-tag font-lock-comment-face) comment 46) (xml-item "<li>" font-lock-comment-face comment 119)))"##
        ]],
    )
}

fn tempo_skeletons_scaffold_a_module_spec_and_receive_timeout_handler() -> ParityBatchCase {
    ParityBatchCase::value(
        "tempo_skeletons_scaffold_a_module_spec_and_receive_timeout_handler",
        r##"
(with-temp-buffer
  (setq buffer-file-name
        (expand-file-name "checkout_worker.erl"
                          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
  (let ((erlang-skel-use-separators nil))
    (erlang-mode)
    (setq-local indent-tabs-mode nil)
    (tempo-template-erlang-module)
    (insert
     "\nprocess(Order, State) ->\n"
     "    {ok, Order, State}.\n\n")
    (goto-char (point-min))
    (search-forward "\n\n")
    (backward-char)
    (tempo-template-erlang-spec)
    (goto-char (point-min))
    (search-forward "undefined.")
    (insert "\n")
    (goto-char (point-max))
    (insert "wait_for_settlement(Reference, State) ->\n")
    (let ((receive-start (point)))
      (save-excursion
        (tempo-template-erlang-after))
      (goto-char receive-start)
      (search-forward "_ ->")
      (replace-match "{settled, Reference} ->" t t)
      (search-forward "ok")
      (replace-match "{ok, State}" t t)
      (search-forward "after T ->")
      (replace-match "after 5000 ->" t t)
      (search-forward "ok")
      (replace-match "{error, timeout, State}" t t))
    (goto-char (point-max))
    (insert ".\n")
    (erlang-indent-region (point-min) (point-max))
    (list
     :module (erlang-get-module)
     :source (buffer-substring-no-properties (point-min) (point-max))
     :point
     (list (point)
           (line-number-at-pos)
           (current-column)
           (buffer-substring-no-properties
            (line-beginning-position) (line-end-position)))
     :functions
     (save-excursion
       (goto-char (point-max))
       (let (names)
         (while (erlang-beginning-of-function)
           (let ((name (erlang-get-function-name-and-arity)))
             (when name (push name names))))
         names)))))
"##,
        expect![[
            r##"OK (:module "checkout_worker" :source "-module(checkout_worker).\n-spec process(Order, State) -> undefined.\n\nprocess(Order, State) ->\n    {ok, Order, State}.\n\nwait_for_settlement(Reference, State) ->\n    receive\n        {settled, Reference} ->\n            {ok, State}\n    after 5000 ->\n            {error, timeout, State}\n    end.\n" :point (292 14 0 "") :functions ("process/2" "wait_for_settlement/2"))"##
        ]],
    )
}

fn identifiers_and_compile_options_cross_the_xref_and_shell_command_boundary() -> ParityBatchCase {
    ParityBatchCase::value(
        "identifiers_and_compile_options_cross_the_xref_and_shell_command_boundary",
        r##"
(with-temp-buffer
  (setq buffer-file-name
        (expand-file-name "src/checkout_pricing.erl"
                          (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
  (erlang-mode)
  (insert
   "-module(checkout_pricing).\n"
   "-import(lists, [map/2]).\n"
   "-define(TAX, 8).\n"
   "-record(order, {total}).\n\n"
   "quote(Items, Customer) ->\n"
   "    map(fun normalize/1, Items),\n"
   "    checkout_pricing:apply_tax(Customer, ?TAX),\n"
   "    #order{total = checkout_pricing:quote(Items, Customer)}.\n")
  (cl-labels
      ((observe (label token offset)
         (goto-char (point-min))
         (search-forward token)
         (goto-char (+ (match-beginning 0) offset))
         (let ((identifier (erlang-get-identifier-at-point)))
           (list label identifier
                 (erlang-id-to-string identifier)
                 (erlang-default-function-or-module)))))
    (let* ((options
            (list 'binary
                  'return_errors
                  (cons 'outdir "/workspace/ebin")
                  (vector 'd 'tax 8)
                  (vector 'parse_transform 'checkout_transform)))
           (formatted (inferior-erlang-format-opt options))
           (erlang-compile-use-outdir t)
           (normal-command
            (inferior-erlang-compute-compile-command
             "checkout_pricing" options))
           (cwd-command
            (let ((erlang-compile-use-outdir nil))
              (inferior-erlang-compute-compile-command
               "checkout_pricing" options))))
      (list
       :identifiers
       (list
        (observe 'imported-call "map(fun" 0)
        (observe 'local-function "normalize/1" 0)
        (observe 'qualified-call "checkout_pricing:apply_tax" 20)
        (observe 'macro "?TAX" 1)
        (observe 'record "#order" 1)
        (observe 'qualified-module "checkout_pricing:quote" 0))
       :roundtrips
       (mapcar
        (lambda (identifier)
          (let ((parsed (erlang-id-to-list identifier)))
            (list identifier parsed (erlang-id-to-string parsed))))
        '("fun/10"
          "qualified-function checkout_pricing:quote/2"
          "record checkout_order"
          "macro TAX"
          "module checkout_pricing"))
       :options formatted
       :normal-command normal-command
       :cwd-command cwd-command))))
"##,
        expect![[
            r##"OK (:identifiers ((imported-call (nil "lists" "map" 2) "lists:map/2" "lists:map") (local-function (nil "checkout_pricing" "normalize" nil) "checkout_pricing:normalize" "checkout_pricing:normalize") (qualified-call (qualified-function "checkout_pricing" "apply_tax" 2) "qualified-function checkout_pricing:apply_tax/2" "checkout_pricing:apply_tax") (macro (macro nil "TAX" nil) "macro TAX" "-define(TAX") (record (record nil "order" nil) "record order" "-record(order") (qualified-module (qualified-function "checkout_pricing" "quote" 2) "qualified-function checkout_pricing:quote/2" "checkout_pricing:quote")) :roundtrips (("fun/10" (nil nil "fun" 10) "fun/10") ("qualified-function checkout_pricing:quote/2" (qualified-function "checkout_pricing" "quote" 2) "qualified-function checkout_pricing:quote/2") ("record checkout_order" (record nil "checkout_order" nil) "record checkout_order") ("macro TAX" (macro nil "TAX" nil) "macro TAX") ("module checkout_pricing" (module nil "checkout_pricing" nil) "module checkout_pricing")) :options "[binary, return_errors, {outdir, \"/workspace/ebin\"}, {d, tax, 8}, {parse_transform, checkout_transform}]" :normal-command "c(\"checkout_pricing\", [binary, return_errors, {outdir, \"/workspace/ebin\"}, {d, tax, 8}, {parse_transform, checkout_transform}])." :cwd-command "f(Tmp8742), {ok, Tmp7236} = file:get_cwd(), file:set_cwd(\"/workspace/ebin\"), Tmp8742 = c(\"checkout_pricing\", [binary, return_errors, {d, tax, 8}, {parse_transform, checkout_transform}]), file:set_cwd(Tmp7236), f(Tmp7236), Tmp8742.")"##
        ]],
    )
}

#[test]
fn erlang_package_batch() {
    let cases = vec![
        otp_checkout_server_is_indented_as_a_complete_editing_workflow(),
        semantic_fontification_distinguishes_real_erlang_language_roles(),
        imenu_defun_navigation_and_region_marking_follow_multiclause_functions(),
        edoc_completion_indentation_and_fontification_build_function_documentation(),
        tempo_skeletons_scaffold_a_module_spec_and_receive_timeout_handler(),
        identifiers_and_compile_options_cross_the_xref_and_shell_command_boundary(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Erlang Mode parity test");
    assert_oracle_batch_cases(erlang_oracle(), test_name, "erlang_parity", &cases);
}
