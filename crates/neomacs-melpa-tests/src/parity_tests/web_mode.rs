use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, WEB_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const WEB_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const WEB_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'web-mode)

(defun neomacs-web-mode-test-setup (file-name contents)
  "Open CONTENTS as FILE-NAME in Web Mode and fully fontify it."
  (setq buffer-file-name
        (expand-file-name file-name temporary-file-directory))
  (insert contents)
  (web-mode)
  (font-lock-ensure (point-min) (point-max)))

(defun neomacs-web-mode-test-refresh ()
  "Rescan and fontify the whole buffer after a structural edit."
  (web-mode-buffer-scan)
  (web-mode-buffer-fontify)
  (font-lock-ensure (point-min) (point-max)))

(defun neomacs-web-mode-test-token (needle)
  "Describe NEEDLE's user-visible language and face at its first occurrence."
  (save-excursion
    (goto-char (point-min))
    (let ((case-fold-search nil))
      (search-forward needle))
    (let ((position (match-beginning 0)))
      (list :token needle
            :offset (- position (point-min))
            :language (web-mode-language-at-pos position)
            :face (or (get-text-property position 'font-lock-face)
                      (get-text-property position 'face))))))
"##;

fn web_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(WEB_MODE_MELPA_PIN, "web-mode.el")
        .expect("prepare revision-pinned Web Mode source below ./tmp")
        .with_prelude(WEB_MODE_TEST_PRELUDE)
        .with_timeout(WEB_MODE_TEST_TIMEOUT)
}

fn release_page_indents_and_classifies_html_css_and_javascript_together() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((indent-tabs-mode nil)
        (web-mode-markup-indent-offset 2)
        (web-mode-css-indent-offset 2)
        (web-mode-code-indent-offset 2))
    (neomacs-web-mode-test-setup
     "release.html"
     (concat
      "<!doctype html>\n"
      "<html>\n"
      "<head>\n"
      "<style>\n"
      ".card {\n"
      "color: #336699;\n"
      "}\n"
      "</style>\n"
      "<script>\n"
      "const release = {ready: true};\n"
      "if (release.ready) {\n"
      "console.log(\"ship\");\n"
      "}\n"
      "</script>\n"
      "</head>\n"
      "<body>\n"
      "<main class=\"card\">\n"
      "<h1>Release</h1>\n"
      "</main>\n"
      "</body>\n"
      "</html>\n"))
    (web-mode-buffer-indent)
    (neomacs-web-mode-test-refresh)
    (list :content-type web-mode-content-type
          :engine web-mode-engine
          :tokens
          (mapcar #'neomacs-web-mode-test-token
                  '("main" "class" "color" "const" "ship"))
          :buffer (buffer-substring-no-properties (point-min) (point-max)))))
"##;
    let expected = expect![[
        r####"OK (:content-type "html" :engine "none" :tokens ((:token "main" :offset 248 :language "html" :face web-mode-html-tag-face) (:token "class" :offset 253 :language "html" :face web-mode-html-attr-name-face) (:token "color" :offset 64 :language "css" :face nil) (:token "const" :offset 118 :language "javascript" :face nil) (:token "ship" :offset 195 :language "javascript" :face web-mode-javascript-string-face)) :buffer "<!doctype html>\n<html>\n  <head>\n    <style>\n     .card {\n       color: #336699;\n     }\n    </style>\n    <script>\n     const release = {ready: true};\n     if (release.ready) {\n       console.log(\"ship\");\n     }\n    </script>\n  </head>\n  <body>\n    <main class=\"card\">\n      <h1>Release</h1>\n    </main>\n  </body>\n</html>\n")"####
    ]];
    ParityBatchCase::value(
        "release_page_indents_and_classifies_html_css_and_javascript_together",
        elisp_form,
        expected,
    )
}

fn structural_refactor_renames_wraps_and_selects_the_release_heading() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((indent-tabs-mode nil)
        (web-mode-markup-indent-offset 2))
    (neomacs-web-mode-test-setup
     "release-card.html"
     (concat
      "<section id=\"dashboard\">\n"
      "<div class=\"release-card\">\n"
      "<h2>Release 2026</h2>\n"
      "<p>Status: ready</p>\n"
      "</div>\n"
      "</section>\n"))
    (goto-char (point-min))
    (search-forward "<div")
    (web-mode-element-rename "article")
    (neomacs-web-mode-test-refresh)
    (goto-char (point-min))
    (search-forward "<article")
    (web-mode-element-wrap "main")
    (neomacs-web-mode-test-refresh)
    (goto-char (point-min))
    (search-forward "Release 2026")
    (web-mode-element-content-select)
    (let ((selected (buffer-substring-no-properties
                     (region-beginning) (region-end))))
      (deactivate-mark)
      (web-mode-buffer-indent)
      (neomacs-web-mode-test-refresh)
      (list :selected-heading selected
            :tokens (mapcar #'neomacs-web-mode-test-token
                            '("main" "article" "release-card"))
            :buffer
            (buffer-substring-no-properties (point-min) (point-max))))))
"##;
    let expected = expect![[
        r####"OK (:selected-heading "Release 2026" :tokens ((:token "main" :offset 28 :language "html" :face web-mode-html-tag-face) (:token "article" :offset 39 :language "html" :face web-mode-html-tag-face) (:token "release-card" :offset 54 :language "html" :face web-mode-html-attr-value-face)) :buffer "<section id=\"dashboard\">\n  <main>\n    <article class=\"release-card\">\n      <h2>Release 2026</h2>\n      <p>Status: ready</p>\n    </article>\n  </main>\n</section>\n")"####
    ]];
    ParityBatchCase::value(
        "structural_refactor_renames_wraps_and_selects_the_release_heading",
        elisp_form,
        expected,
    )
}

fn erb_template_indents_and_round_trips_expression_comment() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((indent-tabs-mode nil)
        (web-mode-markup-indent-offset 2)
        (web-mode-code-indent-offset 2))
    (neomacs-web-mode-test-setup
     "release.html.erb"
     (concat
      "<% if release.ready? %>\n"
      "<article class=\"release\">\n"
      "<h1><%= release.name %></h1>\n"
      "<% release.artifacts.each do |artifact| %>\n"
      "<a href=\"<%= artifact.url %>\"><%= artifact.name %></a>\n"
      "<% end %>\n"
      "</article>\n"
      "<% end %>\n"))
    (web-mode-buffer-indent)
    (neomacs-web-mode-test-refresh)
    (let ((indented
           (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "release.name")
      (web-mode-comment-or-uncomment)
      (neomacs-web-mode-test-refresh)
      (goto-char (point-min))
      (search-forward "release.name")
      (let ((commented-expression
             (buffer-substring-no-properties
              (line-beginning-position) (line-end-position))))
        (web-mode-comment-or-uncomment)
        (neomacs-web-mode-test-refresh)
        (list :content-type web-mode-content-type
              :engine web-mode-engine
              :tokens
              (mapcar #'neomacs-web-mode-test-token
                      '("article" "if" "release.name" "artifact.url"))
              :commented-expression commented-expression
              :indented indented
              :restored
              (buffer-substring-no-properties (point-min) (point-max)))))))
"##;
    let expected = expect![[
        r####"OK (:content-type "html" :engine "erb" :tokens ((:token "article" :offset 27 :language "html" :face web-mode-html-tag-face) (:token "if" :offset 3 :language "erb" :face nil) (:token "release.name" :offset 64 :language "erb" :face nil) (:token "artifact.url" :offset 151 :language "erb" :face nil)) :commented-expression "    <h1><%#= release.name %></h1>" :indented "<% if release.ready? %>\n  <article class=\"release\">\n    <h1><%= release.name %></h1>\n    <% release.artifacts.each do |artifact| %>\n      <a href=\"<%= artifact.url %>\"><%= artifact.name %></a>\n    <% end %>\n  </article>\n<% end %>\n" :restored "<% if release.ready? %>\n  <article class=\"release\">\n    <h1><%= release.name %></h1>\n    <% release.artifacts.each do |artifact| %>\n      <a href=\"<%= artifact.url %>\"><%= artifact.name %></a>\n    <% end %>\n  </article>\n<% end %>\n")"####
    ]];
    ParityBatchCase::value(
        "erb_release_template_indents_control_flow_and_round_trips_an_expression_comment",
        elisp_form,
        expected,
    )
}

fn jsx_component_renames_status_and_selects_artifact_link() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((indent-tabs-mode nil)
        (web-mode-markup-indent-offset 2)
        (web-mode-code-indent-offset 2))
    (neomacs-web-mode-test-setup
     "ReleaseCard.jsx"
     (concat
      "const ReleaseCard = ({ release }) => (\n"
      "<section className=\"release-card\">\n"
      "<header>\n"
      "<h2>{release.name}</h2>\n"
      "<span>{release.ready ? \"Ready\" : \"Pending\"}</span>\n"
      "</header>\n"
      "{release.artifacts.map((artifact) => (\n"
      "<a key={artifact.id} href={artifact.url}>\n"
      "{artifact.name}\n"
      "</a>\n"
      "))}\n"
      "</section>\n"
      ");\n"))
    (web-mode-buffer-indent)
    (neomacs-web-mode-test-refresh)
    (goto-char (point-min))
    (search-forward "<span")
    (web-mode-element-rename "strong")
    (neomacs-web-mode-test-refresh)
    (goto-char (point-min))
    (search-forward "<a ")
    (web-mode-element-select)
    (let ((selected-link
           (buffer-substring-no-properties
            (region-beginning) (region-end))))
      (deactivate-mark)
      (list :content-type web-mode-content-type
            :engine web-mode-engine
            :selected-link selected-link
            :tokens
            (mapcar #'neomacs-web-mode-test-token
                    '("const" "section" "release.name" "strong"
                      "artifact.url"))
            :buffer
            (buffer-substring-no-properties (point-min) (point-max))))))
"##;
    let expected = expect![[
        r####"OK (:content-type "jsx" :engine "none" :selected-link "<a key={artifact.id} href={artifact.url}>\n        {artifact.name}\n      </a>" :tokens ((:token "const" :offset 0 :language "jsx" :face nil) (:token "section" :offset 42 :language "jsx" :face web-mode-html-tag-face) (:token "release.name" :offset 100 :language "jsx" :face nil) (:token "strong" :offset 126 :language "jsx" :face web-mode-html-tag-face) (:token "artifact.url" :offset 270 :language "jsx" :face nil)) :buffer "const ReleaseCard = ({ release }) => (\n  <section className=\"release-card\">\n    <header>\n      <h2>{release.name}</h2>\n      <strong>{release.ready ? \"Ready\" : \"Pending\"}</strong>\n    </header>\n    {release.artifacts.map((artifact) => (\n      <a key={artifact.id} href={artifact.url}>\n        {artifact.name}\n      </a>\n    ))}\n  </section>\n);\n")"####
    ]];
    ParityBatchCase::value(
        "jsx_component_indents_renames_a_status_tag_and_selects_an_artifact_link",
        elisp_form,
        expected,
    )
}

fn inserting_a_new_list_item_incrementally_updates_navigation_and_faces() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((indent-tabs-mode nil)
        (web-mode-markup-indent-offset 2))
    (neomacs-web-mode-test-setup
     "artifact-list.html"
     "<ul>\n  <li>Draft</li>\n</ul>\n")
    (goto-char (point-min))
    (search-forward "</ul>")
    (beginning-of-line)
    (let ((insert-beg (point)))
      (insert "<li data-state=\"ready\">Ready</li>\n")
      (indent-region insert-beg (point))
      (font-lock-ensure insert-beg (point-max)))
    (goto-char (point-min))
    (search-forward "Ready")
    (web-mode-element-select)
    (let ((selected-item
           (buffer-substring-no-properties
            (region-beginning) (region-end))))
      (deactivate-mark)
      (list :selected-item selected-item
            :tokens
            (mapcar #'neomacs-web-mode-test-token
                    '("data-state" "ready" "Ready"))
            :buffer
            (buffer-substring-no-properties (point-min) (point-max))))))
"##;
    let expected = expect![[
        r####"OK (:selected-item "<li data-state=\"ready\">Ready</li>" :tokens ((:token "data-state" :offset 28 :language "html" :face web-mode-html-attr-custom-face) (:token "ready" :offset 40 :language "html" :face web-mode-html-attr-value-face) (:token "Ready" :offset 47 :language "html" :face nil)) :buffer "<ul>\n  <li>Draft</li>\n  <li data-state=\"ready\">Ready</li>\n</ul>\n")"####
    ]];
    ParityBatchCase::value(
        "inserting_a_new_list_item_incrementally_updates_navigation_and_faces",
        elisp_form,
        expected,
    )
}

fn dom_normalization_preserves_embedded_code() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((indent-tabs-mode nil)
        (web-mode-markup-indent-offset 2)
        (web-mode-code-indent-offset 2)
        (web-mode-normalization-rules
         '(("tag-case" . "lower-case")
           ("attr-case" . "lower-case")
           ("special-chars" . "unicode")
           ("smart-apostrophes" . t)
           ("smart-quotes" . t)
           ("whitespaces" . t)
           ("indentation" . t))))
    (neomacs-web-mode-test-setup
     "release-normalize.html"
     (concat
      "<DIV DATA-STATE=\"READY\">\n"
      "<P>Ren&eacute;'s \"release\" &#x2713;</P>\n"
      "<SCRIPT>const raw = \"&eacute;\";</SCRIPT>\n"
      "</DIV>\n"))
    (web-mode-dom-normalize)
    (neomacs-web-mode-test-refresh)
    (list :tokens
          (mapcar #'neomacs-web-mode-test-token
                  '("div" "data-state" "René" "release" "const"
                    "&eacute;"))
          :buffer
          (buffer-substring-no-properties (point-min) (point-max)))))
"##;
    let expected = expect![[
        r####"OK (:tokens ((:token "div" :offset 1 :language "html" :face web-mode-html-tag-face) (:token "data-state" :offset 5 :language "html" :face web-mode-html-attr-custom-face) (:token "René" :offset 30 :language "html" :face nil) (:token "release" :offset 38 :language "html" :face nil) (:token "const" :offset 63 :language "javascript" :face nil) (:token "&eacute;" :offset 76 :language "javascript" :face web-mode-javascript-string-face)) :buffer "<div data-state=\"READY\">\n  <p>René's «release» ✓</p>\n  <script>const raw = \"&eacute;\";</script>\n</div>\n")"####
    ]];
    ParityBatchCase::value(
        "dom_normalization_changes_only_document_content_and_preserves_embedded_code",
        elisp_form,
        expected,
    )
}

fn blade_dashboard_indents_and_sorts_attributes() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (let ((indent-tabs-mode nil)
        (web-mode-markup-indent-offset 2)
        (web-mode-code-indent-offset 2))
    (neomacs-web-mode-test-setup
     "release.blade.php"
     (concat
      "@extends('layouts.app')\n"
      "@section('content')\n"
      "<div data-z=\"9\" id=\"release\" class=\"card\" aria-label=\"Release\">\n"
      "@if ($release->ready)\n"
      "<h1>{{ $release->name }}</h1>\n"
      "@foreach ($release->artifacts as $artifact)\n"
      "<a href=\"{{ $artifact->url }}\">{{ $artifact->name }}</a>\n"
      "@endforeach\n"
      "@else\n"
      "<p>Pending</p>\n"
      "@endif\n"
      "</div>\n"
      "@endsection\n"))
    (web-mode-buffer-indent)
    (neomacs-web-mode-test-refresh)
    (goto-char (point-min))
    (search-forward "<div")
    (web-mode-tag-attributes-sort)
    (neomacs-web-mode-test-refresh)
    (list :content-type web-mode-content-type
          :engine web-mode-engine
          :tokens
          (mapcar #'neomacs-web-mode-test-token
                  '("extends" "div" "aria-label" "$release->name"
                    "$artifact->url" "Pending"))
          :buffer
          (buffer-substring-no-properties (point-min) (point-max)))))
"##;
    let expected = expect![[
        r####"OK (:content-type "html" :engine "blade" :tokens ((:token "extends" :offset 1 :language "blade" :face nil) (:token "div" :offset 47 :language "html" :face web-mode-html-tag-face) (:token "aria-label" :offset 51 :language "html" :face web-mode-html-attr-name-face) (:token "$release->name" :offset 149 :language "blade" :face nil) (:token "$artifact->url" :offset 242 :language "blade" :face nil) (:token "Pending" :offset 324 :language "html" :face nil)) :buffer "@extends('layouts.app')\n@section('content')\n  <div aria-label=\"Release\" class=\"card\" data-z=\"9\" id=\"release\">\n    @if ($release->ready)\n      <h1>{{ $release->name }}</h1>\n      @foreach ($release->artifacts as $artifact)\n        <a href=\"{{ $artifact->url }}\">{{ $artifact->name }}</a>\n      @endforeach\n    @else\n      <p>Pending</p>\n    @endif\n  </div>\n@endsection\n")"####
    ]];
    ParityBatchCase::value(
        "blade_dashboard_indents_directives_and_sorts_real_component_attributes",
        elisp_form,
        expected,
    )
}

#[test]
fn web_mode_package_batch() {
    assert_oracle_batch_cases(
        web_mode_oracle(),
        "web-mode-package-batch",
        "Web Mode",
        &[
            release_page_indents_and_classifies_html_css_and_javascript_together(),
            structural_refactor_renames_wraps_and_selects_the_release_heading(),
            erb_template_indents_and_round_trips_expression_comment(),
            jsx_component_renames_status_and_selects_artifact_link(),
            inserting_a_new_list_item_incrementally_updates_navigation_and_faces(),
            dom_normalization_preserves_embedded_code(),
            blade_dashboard_indents_and_sorts_attributes(),
        ],
    );
}
