use expect_test::expect;

use super::ParityBatchCase;

/// The user opens a module of a real Mix project, runs `M-x ac-alchemist-setup`
/// and lets auto-complete fire once.  Visiting the file already puts
/// `alchemist-mode` on `elixir-mode-hook`, which launches
/// `elixir <alchemist>/alchemist-server/run.exs dev` for the project;
/// `ac-alchemist-setup` then has to turn on `auto-complete-mode` and push its
/// source in front of `ac-sources`, and the first `ac-start` has to write one
/// `COMP` request built from the dotted expression before point.  The answer is
/// still in flight during that pass, which is why the source legitimately
/// produces no candidates yet.
fn ac_alchemist_setup_starts_the_project_server_and_sends_the_first_completion_request()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ac_alchemist_setup_starts_the_project_server_and_sends_the_first_completion_request",
        r##"(ac-alchemist-test-session
    '(("COMP" "String.to_"
       "String.to_\nto_atom/1\nto_charlist/1\nto_existing_atom/1\nto_float/1\nto_integer/1\nto_integer/2"))
  (let ((buffer (ac-alchemist-test-visit
                 "blogpost/lib/blogpost/slug.ex"
                 "defmodule Blogpost.Slug do\n  @moduledoc \"Turn a post title into a URL slug.\"\n\n  def build(title) do\n    String.to_\n  end\nend\n"
                 "String.to_")))
    (with-current-buffer buffer
      (list :major-mode major-mode
            :auto-complete-mode (progn (ac-alchemist-setup) auto-complete-mode)
            :ac-sources ac-sources
            :source ac-source-alchemist
            :project-root (alchemist-project-root)
            :server-name (alchemist-server-process-name)
            :server-live-after-visit (and (alchemist-server-process-p) t)
            :started (ac-start)
            :ac-point ac-point
            :ac-prefix ac-prefix
            :server-hint ac-alchemist--prefix
            :candidates-before-answer (ac-candidates)
            :answers (ac-alchemist-test-await 1)
            :server-status (process-status (alchemist-server-process))
            :candidate-cache ac-alchemist--candidate-cache
            :output-cache ac-alchemist--output-cache
            :buffer-unchanged (buffer-modified-p)
            :point (point)
            :elixir-log (ac-alchemist-test-elixir-log)))))"##,
        expect![[
            r#"OK (:major-mode elixir-mode :auto-complete-mode t :ac-sources (ac-source-alchemist ac-source-words-in-same-mode-buffers) :source ((init . ac-alchemist--complete-request) (prefix . ac-alchemist--prefix) (candidates . ac-alchemist--candidates) (document . ac-alchemist--show-document) (requires . -1)) :project-root "[ORACLE-SANDBOX]/blogpost/" :server-name "[ORACLE-SANDBOX]/blogpost/" :server-live-after-visit t :started t :ac-point 112 :ac-prefix "to_" :server-hint "String.to_" :candidates-before-answer nil :answers 1 :server-status run :candidate-cache ("to_integer/2" "to_integer/1" "to_float/1" "to_existing_atom/1" "to_charlist/1" "to_atom/1" "String.to_") :output-cache nil :buffer-unchanged nil :point 115 :elixir-log ("ARGV <alchemist>/alchemist-server/run.exs dev" "REQ COMP { \"String.to_\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP"))"#
        ]],
    )
}

fn ac_alchemist_completes_a_dotted_call_with_arity_annotated_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_alchemist_completes_a_dotted_call_with_arity_annotated_candidates",
        r##"(ac-alchemist-test-session
    '(("COMP" "String.to_"
       "String.to_\nto_atom/1\nto_charlist/1\nto_existing_atom/1\nto_float/1\nto_integer/1\nto_integer/2"))
  (let ((buffer (ac-alchemist-test-visit
                 "blogpost/lib/blogpost/slug.ex"
                 "defmodule Blogpost.Slug do\n  def build(title) do\n    String.to_\n  end\nend\n"
                 "String.to_")))
    (with-current-buffer buffer
      (ac-alchemist-setup)
      (ac-start)
      (ac-alchemist-test-await 1)
      (ac-complete-alchemist)
      (list :candidates (mapcar (lambda (candidate)
                                  (list (substring-no-properties candidate)
                                        (get-text-property 0 'symbol candidate)))
                                ac-candidates)
            :first-properties (text-properties-at 0 (car ac-candidates))
            :selected (substring-no-properties (ac-selected-candidate))
            :after-two-next (progn (ac-next)
                                   (ac-next)
                                   (substring-no-properties (ac-selected-candidate)))
            :completed (substring-no-properties (ac-complete))
            :menu-live (ac-menu-live-p)
            :buffer (buffer-substring-no-properties (point-min) (point-max))
            :point (point)
            :modified (buffer-modified-p)
            :answers (ac-alchemist-test-await 2)
            :elixir-log (ac-alchemist-test-elixir-log)))))"##,
        expect![[
            r#"OK (:candidates (("to_atom" "/1") ("to_float" "/1") ("to_integer" "/2") ("to_charlist" "/1") ("to_existing_atom" "/1")) :first-properties (document ac-alchemist--show-document symbol "/1") :selected "to_atom" :after-two-next "to_integer" :completed "to_integer" :menu-live nil :buffer "defmodule Blogpost.Slug do\n  def build(title) do\n    String.to_integer\n  end\nend\n" :point 71 :modified t :answers 2 :elixir-log ("ARGV <alchemist>/alchemist-server/run.exs dev" "REQ COMP { \"String.to_\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP" "REQ COMP { \"String.to_\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP"))"#
        ]],
    )
    .fresh_process()
}

fn ac_alchemist_documents_the_selected_function_from_the_alchemist_server() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_alchemist_documents_the_selected_function_from_the_alchemist_server",
        r##"(ac-alchemist-test-session
    '(("COMP" "String.to_"
       "String.to_\nto_atom/1\nto_charlist/1\nto_existing_atom/1\nto_float/1\nto_integer/1\nto_integer/2")
      ("DOCL" "String.to_integer/2"
       "\e[0m\e[1m                        def to_integer(string, base)\e[0m\e[0m\n\nReturns an integer whose text representation is `string` in base `base`.\n\n`string` must be the representation of an integer with 2 ≤ base ≤ 36 —\notherwise an ArgumentError is raised.\n\n## Examples\n\n    iex> String.to_integer(\"3FF\", 16)\n    1023"))
  (let ((buffer (ac-alchemist-test-visit
                 "blogpost/lib/blogpost/slug.ex"
                 "defmodule Blogpost.Slug do\n  def build(title) do\n    String.to_\n  end\nend\n"
                 "String.to_")))
    (with-current-buffer buffer
      (ac-alchemist-setup)
      (ac-start)
      (ac-alchemist-test-await 1)
      (ac-complete-alchemist)
      (ac-next)
      (ac-next)
      (let ((selected (ac-selected-candidate)))
        (list :selected (substring-no-properties selected)
              :arity (get-text-property 0 'symbol selected)
              :document-function (get-text-property 0 'document selected)
              :documentation (popup-item-documentation selected)
              :document-variable ac-alchemist--document
              :answers (ac-alchemist-test-await 3)
              :elixir-log (ac-alchemist-test-elixir-log))))))"##,
        expect![[
            r#"OK (:selected "to_integer" :arity "/2" :document-function ac-alchemist--show-document :documentation "                        def to_integer(string, base)\nReturns an integer whose text representation is `string` in base `base`.\n`string` must be the representation of an integer with 2 ≤ base ≤ 36 —\notherwise an ArgumentError is raised.\n## Examples\n    iex> String.to_integer(\"3FF\", 16)\n    1023" :document-variable "                        def to_integer(string, base)\nReturns an integer whose text representation is `string` in base `base`.\n`string` must be the representation of an integer with 2 ≤ base ≤ 36 —\notherwise an ArgumentError is raised.\n## Examples\n    iex> String.to_integer(\"3FF\", 16)\n    1023" :answers 3 :elixir-log ("ARGV <alchemist>/alchemist-server/run.exs dev" "REQ COMP { \"String.to_\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP" "REQ COMP { \"String.to_\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP" "REQ DOCL { \"String.to_integer/2\", [ context: Elixir, imports: [Blogpost.Slug], aliases: [] ] }" "ANS DOCL"))"#
        ]],
    )
    .fresh_process()
}

fn ac_alchemist_completes_a_bare_module_name_and_documents_the_module() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_alchemist_completes_a_bare_module_name_and_documents_the_module",
        r##"(ac-alchemist-test-session
    '(("COMP" "Str" "Str\nStream\nString\nStringIO")
      ("DOCL" "String"
       "\e[0m\e[1m                                 String\e[0m\e[0m\n\nStrings in Elixir are UTF-8 encoded binaries — «héllo» is 5 characters long."))
  (let ((buffer (ac-alchemist-test-visit
                 "blogpost/lib/blogpost/slug.ex"
                 "defmodule Blogpost.Slug do\n  def build(title) do\n    Str\n  end\nend\n"
                 "    Str")))
    (with-current-buffer buffer
      (ac-alchemist-setup)
      (ac-start)
      (ac-alchemist-test-await 1)
      (list :ac-point ac-point
            :ac-prefix ac-prefix
            :server-hint ac-alchemist--prefix
            :candidate-cache ac-alchemist--candidate-cache
            :candidates (progn (ac-complete-alchemist)
                               (mapcar (lambda (candidate)
                                         (list (substring-no-properties candidate)
                                               (get-text-property 0 'symbol candidate)))
                                       ac-candidates))
            :selected (substring-no-properties (ac-selected-candidate))
            :after-next (progn (ac-next) (substring-no-properties (ac-selected-candidate)))
            :documentation (popup-item-documentation (ac-selected-candidate))
            :completed (substring-no-properties (ac-complete))
            :buffer (buffer-substring-no-properties (point-min) (point-max))
            :point (point)
            :answers (ac-alchemist-test-await 3)
            :elixir-log (ac-alchemist-test-elixir-log)))))"##,
        expect![[
            r#"OK (:ac-point 54 :ac-prefix "Str" :server-hint "Str" :candidate-cache ("StringIO" "String" "Stream" "Str") :candidates (("Str" "  ") ("String" "  ") ("Stream" "  ") ("StringIO" "  ")) :selected "Str" :after-next "String" :documentation "                                 String\nStrings in Elixir are UTF-8 encoded binaries — «héllo» is 5 characters long." :completed "String" :buffer "defmodule Blogpost.Slug do\n  def build(title) do\n    String\n  end\nend\n" :point 60 :answers 3 :elixir-log ("ARGV <alchemist>/alchemist-server/run.exs dev" "REQ COMP { \"Str\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP" "REQ COMP { \"Str\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP" "REQ DOCL { \"String\", [ context: Elixir, imports: [Blogpost.Slug], aliases: [] ] }" "ANS DOCL"))"#
        ]],
    )
    .fresh_process()
}

fn ac_alchemist_reports_no_candidates_for_an_unknown_module_and_leaves_the_buffer_alone()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ac_alchemist_reports_no_candidates_for_an_unknown_module_and_leaves_the_buffer_alone",
        r##"(ac-alchemist-test-session nil
  (let ((buffer (ac-alchemist-test-visit
                 "blogpost/lib/blogpost/slug.ex"
                 "defmodule Blogpost.Slug do\n  def build(title) do\n    Strng.to_a\n  end\nend\n"
                 "Strng.to_a")))
    (with-current-buffer buffer
      (ac-alchemist-setup)
      (ac-start)
      (ac-alchemist-test-await 1)
      (list :server-hint ac-alchemist--prefix
            :candidate-cache ac-alchemist--candidate-cache
            :output-cache ac-alchemist--output-cache
            :candidates (ac-candidates)
            :restarted (ac-complete-alchemist)
            :ac-candidates ac-candidates
            :selected (ac-selected-candidate)
            :completed (ac-complete)
            :buffer (buffer-substring-no-properties (point-min) (point-max))
            :point (point)
            :modified (buffer-modified-p)
            :answers (ac-alchemist-test-await 2)
            :elixir-log (ac-alchemist-test-elixir-log)))))"##,
        expect![[
            r#"OK (:server-hint "Strng.to_a" :candidate-cache nil :output-cache nil :candidates nil :restarted t :ac-candidates nil :selected nil :completed nil :buffer "defmodule Blogpost.Slug do\n  def build(title) do\n    Strng.to_a\n  end\nend\n" :point 64 :modified nil :answers 2 :elixir-log ("ARGV <alchemist>/alchemist-server/run.exs dev" "REQ COMP { \"Strng.to_a\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP" "REQ COMP { \"Strng.to_a\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP"))"#
        ]],
    )
    .fresh_process()
}

fn ac_alchemist_shares_one_project_server_between_two_buffers_of_the_same_project()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ac_alchemist_shares_one_project_server_between_two_buffers_of_the_same_project",
        r##"(ac-alchemist-test-session
    '(("COMP" "String.re"
       "String.re\nreplace/3\nreplace/4\nreplace_leading/3\nreplace_prefix/3\nreplace_suffix/3\nreplace_trailing/3\nreverse/1")
      ("DOCL" "String.reverse/1"
       "def reverse(string)\n\nReturns a string where the graphemes of `string` are in reverse order.")
      ("COMP" "Enum.ma" "Enum.ma\nmap/2\nmap_every/3\nmax/1\nmax_by/2")
      ("DOCL" "Enum.max/1"
       "def max(enumerable)\n\nReturns the maximal element according to Erlang's term ordering."))
  (cl-flet ((complete-at
              (name text site)
              (with-current-buffer (ac-alchemist-test-visit name text site)
                (ac-alchemist-setup)
                (ac-alchemist-setup)
                (ac-start)
                (ac-alchemist-test-await 1)
                (ac-complete-alchemist)
                (list (buffer-name)
                      ac-sources
                      (mapcar #'substring-no-properties ac-candidates)
                      (substring-no-properties (ac-selected-candidate))
                      (popup-item-documentation (ac-selected-candidate))
                      (substring-no-properties (ac-complete))
                      (buffer-substring-no-properties (point-min) (point-max))
                      (point)))))
    (let ((slug (complete-at
                 "blogpost/lib/blogpost/slug.ex"
                 "defmodule Blogpost.Slug do\n  def build(title) do\n    String.re\n  end\nend\n"
                 "String.re"))
          (post (complete-at
                 "blogpost/lib/blogpost/post.ex"
                 "defmodule Blogpost.Post do\n  def titles(posts) do\n    Enum.ma\n  end\nend\n"
                 "Enum.ma")))
      (with-current-buffer (get-buffer "post.ex")
        (list :slug slug
              :post post
              :server-count (length alchemist-server-processes)
              :server-name (alchemist-server-process-name)
              :server-status (process-status (alchemist-server-process))
              :answers (ac-alchemist-test-await 6)
              :elixir-log (ac-alchemist-test-elixir-log))))))"##,
        expect![[
            r#"OK (:slug ("slug.ex" (ac-source-alchemist . #1=(ac-source-words-in-same-mode-buffers)) ("reverse" "replace" "replace_suffix" "replace_prefix" "replace_leading" "replace_trailing") "reverse" "def reverse(string)\nReturns a string where the graphemes of `string` are in reverse order." "reverse" "defmodule Blogpost.Slug do\n  def build(title) do\n    String.reverse\n  end\nend\n" 68) :post ("post.ex" (ac-source-alchemist . #1#) ("max" "map" "max_by" "map_every") "max" "def max(enumerable)\nReturns the maximal element according to Erlang's term ordering." "max" "defmodule Blogpost.Post do\n  def titles(posts) do\n    Enum.max\n  end\nend\n" 63) :server-count 1 :server-name "[ORACLE-SANDBOX]/blogpost/" :server-status run :answers 6 :elixir-log ("ARGV <alchemist>/alchemist-server/run.exs dev" "REQ COMP { \"String.re\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP" "REQ COMP { \"String.re\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP" "REQ DOCL { \"String.reverse/1\", [ context: Elixir, imports: [Blogpost.Slug], aliases: [] ] }" "ANS DOCL" "REQ COMP { \"Enum.ma\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP" "REQ COMP { \"Enum.ma\", [ context: [], imports: [], aliases: [] ] }" "ANS COMP" "REQ DOCL { \"Enum.max/1\", [ context: Elixir, imports: [Blogpost.Post], aliases: [] ] }" "ANS DOCL"))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ac_alchemist_setup_starts_the_project_server_and_sends_the_first_completion_request(),
        ac_alchemist_completes_a_dotted_call_with_arity_annotated_candidates(),
        ac_alchemist_documents_the_selected_function_from_the_alchemist_server(),
        ac_alchemist_completes_a_bare_module_name_and_documents_the_module(),
        ac_alchemist_reports_no_candidates_for_an_unknown_module_and_leaves_the_buffer_alone(),
        ac_alchemist_shares_one_project_server_between_two_buffers_of_the_same_project(),
    ]
}
